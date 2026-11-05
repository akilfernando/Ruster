use crate::{egui, Application, InputFrame, Runtime};
use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Linux development host options. max_frames bounds UI-frame smoke tests (1..=10000).
#[derive(Clone, Debug)]
pub struct HostOptions {
    pub title: String,
    pub app_id: String,
    pub tick_duration: Duration,
    pub max_frames: Option<u64>,
    pub start_running: bool,
    pub initial_camera: Option<ruster_render::CameraState>,
}

impl Default for HostOptions {
    fn default() -> Self {
        Self {
            title: "Application".into(),
            app_id: "org.ruster.application".into(),
            tick_duration: Duration::from_millis(100),
            max_frames: None,
            start_running: false,
            initial_camera: None,
        }
    }
}

struct Diagnostics;
impl log::Log for Diagnostics {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}: {}", record.level(), record.target(), record.args());
        }
    }
    fn flush(&self) {}
}
static DIAGNOSTICS: Diagnostics = Diagnostics;

/// Launch a single shared scene viewport. No editor, MCP listener, cache writes or new GPU path.
/// Existing caller loggers are preserved; otherwise warning/error diagnostics go to stderr.
pub fn run<A: Application>(app: A, options: HostOptions) -> Result<(), String> {
    if options.title.trim().is_empty() || options.app_id.trim().is_empty() {
        return Err("application title and app_id must not be empty".into());
    }
    if options.max_frames.is_some_and(|n| n == 0 || n > 10_000) {
        return Err("max_frames must be 1..=10000".into());
    }
    if options
        .initial_camera
        .is_some_and(|camera| !camera.is_valid())
    {
        return Err("invalid application initial camera".into());
    }
    let mut runtime = Runtime::new(app, options.tick_duration)?;
    runtime.set_running(options.start_running);
    if log::set_logger(&DIAGNOSTICS).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    ruster_render::configure_setup(&mut setup);
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([800.0, 540.0])
            .with_app_id(&options.app_id),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: WgpuSetup::CreateNew(setup),
            ..Default::default()
        },
        ..Default::default()
    };
    let title = format!("Ruster | {}", options.title);
    let exit_error = Arc::new(Mutex::new(None));
    let host_error = exit_error.clone();
    eframe::run_native(&title, native, Box::new(move |cc| {
        let render = cc.wgpu_render_state.as_ref().ok_or("Vulkan renderer unavailable")?;
        let metrics = ruster_render::install(render);
        Ok(Box::new(Host {
            navigation: ruster_render::NavigationOptions { camera: options.initial_camera, ..Default::default() },
            runtime, metrics, options, last_frame: Instant::now(), error: None,
            ui_frames: 0, exit_error: host_error,
        }))
    })).map_err(|e| format!("application desktop/Vulkan initialization failed: {e}; verify your desktop session and Vulkan driver"))?;
    let error = exit_error
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    error.map_or(Ok(()), |error| {
        Err(format!("application closed with unresolved error: {error}"))
    })
}

struct Host<A: Application> {
    runtime: Runtime<A>,
    options: HostOptions,
    last_frame: Instant,
    metrics: ruster_render::MetricsHandle,
    error: Option<String>,
    ui_frames: u64,
    exit_error: Arc<Mutex<Option<String>>>,
    navigation: ruster_render::NavigationOptions,
}

impl<A: Application> Host<A> {
    fn report(&mut self, result: Result<(), String>) {
        if let Err(error) = result {
            if self.error.as_ref() != Some(&error) {
                eprintln!("Ruster application: {error}");
            }
            self.error = Some(error);
            self.runtime.set_running(false);
        }
    }
    fn reset(&mut self) {
        match self.runtime.reset() {
            Ok(()) => self.error = None,
            Err(error) => self.report(Err(error)),
        }
        self.last_frame = Instant::now();
    }

    fn route_keyboard(&mut self, ctx: &egui::Context) {
        // UI gets first refusal: editing text or keyboard-focused widgets must not drive behavior.
        let mut input = InputFrame::default();
        if !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.focused) {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space))
                && self.error.is_none()
                && !self.runtime.app().completed()
            {
                self.runtime.set_running(!self.runtime.is_running());
            }
            input = ctx.input(|i| InputFrame {
                pressed: i
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        egui::Event::Key {
                            key,
                            pressed: true,
                            repeat: false,
                            ..
                        } if *key != egui::Key::Space => Some(*key),
                        _ => None,
                    })
                    .collect(),
                held: i
                    .keys_down
                    .iter()
                    .filter(|key| **key != egui::Key::Space)
                    .copied()
                    .collect(),
                modifiers: i.modifiers,
            });
        }
        // Empty state on focus loss also releases application-owned held-key behavior.
        if self.error.is_none() {
            let result = self.runtime.app_mut().input(&input);
            self.report(result);
        }
    }
}

impl<A: Application> eframe::App for Host<A> {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) && !self.runtime.app_mut().request_close()
        {
            self.runtime.set_running(false);
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.ui_frames = self.ui_frames.saturating_add(1);
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;
        let advanced = self.runtime.advance(elapsed).map(|_| ());
        self.report(advanced);

        egui::Panel::top("application-lifecycle").show(root, |ui| {
            ui.horizontal(|ui| {
                let running = self.runtime.is_running();
                let completed = self.runtime.app().completed();
                let enabled = self.error.is_none() && !completed;
                if ui
                    .add_enabled(
                        enabled,
                        egui::Button::new(if running { "Pause" } else { "Run" }),
                    )
                    .on_hover_text("Space outside text fields and focused controls")
                    .clicked()
                {
                    self.runtime.set_running(!running);
                }
                if ui
                    .add_enabled(enabled && !running, egui::Button::new("Step"))
                    .clicked()
                {
                    let result = self.runtime.step();
                    self.report(result);
                }
                if ui
                    .button("Reset")
                    .on_hover_text("Restore application state and pause")
                    .clicked()
                {
                    self.reset();
                }
                ui.separator();
                ui.label(if completed {
                    "Complete"
                } else if self.runtime.is_running() {
                    "Running"
                } else {
                    "Paused"
                });
                ui.label(format!("Tick {}", self.runtime.tick()));
            });
            if let Some(error) = &self.error {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("{error} · Correct the settings/content and Reset."),
                );
            }
        });
        egui::Panel::right("application-controls")
            .default_size(280.0)
            .resizable(true)
            .show(root, |ui| {
                let result = self.runtime.app_mut().ui(ui);
                self.report(result);
            });

        if self.runtime.app_mut().take_reset_request() {
            self.reset();
        }
        self.route_keyboard(&ctx);

        // Application UI/input may have edited scene data. Invalid state never reaches the GPU.
        let valid = self.runtime.app().content().validate();
        let can_render = valid.is_ok();
        self.report(valid);
        egui::CentralPanel::default().show(root, |ui| {
            if can_render {
                let app = self.runtime.app();
                let content = app.content();
                let selected = app.selected();
                let picked = ruster_render::viewport_with_navigation(
                    ui,
                    content.scene(),
                    selected.as_slice(),
                    content.meshes(),
                    &mut self.navigation,
                )
                .picked;
                if let Some(id) = picked {
                    self.runtime.app_mut().select(&id);
                }
            } else {
                ui.label(
                    "Scene unavailable. Correct the application content and Reset to recover.",
                );
            }
        });
        if self.runtime.is_running() {
            ctx.request_repaint_after(
                self.options
                    .tick_duration
                    .saturating_sub(self.runtime.accumulator),
            );
        }
        if let Some(limit) = self.options.max_frames {
            if self.ui_frames >= limit {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint();
            }
        }
    }
}

impl<A: Application> Drop for Host<A> {
    fn drop(&mut self) {
        let metrics = self
            .metrics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *self
            .exit_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = self.error.clone();
        eprintln!(
            "Ruster application summary: tick={} completed={} ui_frames={} frames={} objects={} mesh_uploads={} mesh_upload_bytes={} draw_calls={} unresolved_error={}",
            self.runtime.tick(),
            self.runtime.app().completed(),
            self.ui_frames,
            metrics.frames,
            metrics.objects,
            metrics.mesh_uploads,
            metrics.mesh_upload_bytes,
            metrics.draw_calls,
            self.error.is_some()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Content, MeshMap, Step};

    struct KeyboardApp {
        content: Content,
        inputs: Vec<InputFrame>,
    }

    impl Application for KeyboardApp {
        fn content(&self) -> &Content {
            &self.content
        }
        fn tick(&mut self, _step: Step) -> Result<(), String> {
            Ok(())
        }
        fn reset(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn input(&mut self, input: &InputFrame) -> Result<(), String> {
            self.inputs.push(input.clone());
            Ok(())
        }
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn raw_input(focused: bool, modifiers: egui::Modifiers) -> egui::RawInput {
        egui::RawInput {
            focused,
            events: vec![
                egui::Event::ModifiersChanged(modifiers),
                key(egui::Key::Space, modifiers),
                key(egui::Key::A, modifiers),
            ],
            ..Default::default()
        }
    }

    fn run_ui(ctx: &egui::Context, input: egui::RawInput, run: impl FnMut(&mut egui::Ui)) {
        let mut output = ctx.run_ui(input, run);
        // This is a routing test without a painter; explicitly discard generated font textures.
        output.textures_delta.clear();
    }

    #[test]
    fn real_egui_focus_routes_shortcuts_text_and_held_state() {
        let app = KeyboardApp {
            content: Content::new(ruster_scene::Scene::demo(), MeshMap::new()).unwrap(),
            inputs: Vec::new(),
        };
        let options = HostOptions::default();
        let mut host = Host {
            navigation: ruster_render::NavigationOptions::default(),
            runtime: Runtime::new(app, options.tick_duration).unwrap(),
            options,
            last_frame: Instant::now(),
            metrics: Arc::new(Mutex::new(ruster_render::Metrics::default())),
            error: None,
            ui_frames: 0,
            exit_error: Arc::new(Mutex::new(None)),
        };
        let ctx = egui::Context::default();

        run_ui(&ctx, raw_input(true, egui::Modifiers::NONE), |ui| {
            assert!(!ui.ctx().egui_wants_keyboard_input());
            host.route_keyboard(ui.ctx());
        });
        assert!(
            host.runtime.is_running(),
            "Space starts an unfocused-widget runtime"
        );
        let input = host.runtime.app().inputs.last().unwrap();
        assert_eq!(input.pressed, [egui::Key::A]);
        assert_eq!(input.held, [egui::Key::A]);
        assert_eq!(input.modifiers, egui::Modifiers::NONE);

        // No widget owns focus: this specifically exercises the native-focus guard.
        run_ui(&ctx, raw_input(false, egui::Modifiers::NONE), |ui| {
            assert!(!ui.ctx().egui_wants_keyboard_input());
            host.route_keyboard(ui.ctx());
        });
        assert!(
            host.runtime.is_running(),
            "unfocused window must not toggle Run"
        );
        let input = host.runtime.app().inputs.last().unwrap();
        assert!(input.pressed.is_empty());
        assert!(
            input.held.is_empty(),
            "focus loss releases held application keys"
        );

        host.runtime.set_running(false);
        let ctx = egui::Context::default();
        run_ui(&ctx, raw_input(true, egui::Modifiers::CTRL), |ui| {
            host.route_keyboard(ui.ctx());
        });
        assert!(
            !host.runtime.is_running(),
            "Ctrl+Space is not the host shortcut"
        );
        let input = host.runtime.app().inputs.last().unwrap();
        assert_eq!(input.pressed, [egui::Key::A]);
        assert_eq!(input.held, [egui::Key::A]);
        assert_eq!(input.modifiers, egui::Modifiers::CTRL);

        let ctx = egui::Context::default();
        let mut text = String::new();
        run_ui(
            &ctx,
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |ui| {
                ui.add(egui::TextEdit::singleline(&mut text).id_salt("settings"))
                    .request_focus();
                assert!(ui.ctx().egui_wants_keyboard_input());
                host.route_keyboard(ui.ctx());
            },
        );
        let mut raw = raw_input(true, egui::Modifiers::NONE);
        raw.events.push(egui::Event::Text("a".into()));
        run_ui(&ctx, raw, |ui| {
            let field = ui.add(egui::TextEdit::singleline(&mut text).id_salt("settings"));
            assert!(field.has_focus());
            assert!(ui.ctx().text_edit_focused());
            host.route_keyboard(ui.ctx());
        });
        assert_eq!(text, "a", "the actual text widget receives text input");
        assert!(
            !host.runtime.is_running(),
            "text editing must not toggle Run"
        );
        let input = host.runtime.app().inputs.last().unwrap();
        assert!(input.pressed.is_empty());
        assert!(
            input.held.is_empty(),
            "text-widget focus releases application keys"
        );
    }
}
