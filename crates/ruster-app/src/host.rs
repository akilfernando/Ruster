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
            action_surface: None,
            action_surface_rect: None,
            action_control_owned: false,
            action_claim_frame: None,
            last_control_frame: None,
            last_action_frame: None,
            navigation: ruster_render::NavigationOptions { camera: options.initial_camera, ..Default::default() },
            runtime, metrics, render: Some(render.clone()), options, last_frame: Instant::now(), error: None,
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
    render: Option<eframe::egui_wgpu::RenderState>,
    error: Option<String>,
    ui_frames: u64,
    exit_error: Arc<Mutex<Option<String>>>,
    navigation: ruster_render::NavigationOptions,
    action_surface: Option<egui::Id>,
    action_surface_rect: Option<egui::Rect>,
    action_control_owned: bool,
    action_claim_frame: Option<u64>,
    last_control_frame: Option<u64>,
    last_action_frame: Option<u64>,
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
        if self.runtime.app().keyboard_actions().is_some() {
            self.route_action_keyboard(ctx);
            return;
        }
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
    fn release_action_control(&mut self, ctx: &egui::Context) {
        self.action_control_owned = false;
        self.runtime.app_mut().cancel_action_input();
        if let Some(surface) = self.action_surface {
            ctx.memory_mut(|memory| memory.surrender_focus(surface));
        }
    }
    fn route_action_keyboard(&mut self, ctx: &egui::Context) {
        let frame = ctx.cumulative_frame_nr();
        // A genuine UI activation requests the surface focus, but must not
        // also admit the Enter/key that activated its button in this frame.
        if self.action_claim_frame == Some(ctx.cumulative_frame_nr()) {
            self.last_action_frame = Some(frame);
            self.runtime.app_mut().cancel_action_input();
            return;
        }
        let focused = ctx.input(|i| i.focused);
        if self.error.is_some() || !focused {
            self.release_action_control(ctx);
            self.last_action_frame = Some(frame);
            return;
        }
        if !self.runtime.is_running() {
            self.release_action_control(ctx);
            if !ctx.egui_wants_keyboard_input()
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space))
            {
                self.runtime.set_running(true);
            }
            self.last_action_frame = Some(frame);
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.release_action_control(ctx);
            self.last_action_frame = Some(frame);
            return;
        }
        if !self.action_control_owned {
            self.runtime.app_mut().cancel_action_input();
            if !ctx.egui_wants_keyboard_input()
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space))
            {
                self.runtime.set_running(false);
            }
            self.last_action_frame = Some(frame);
            return;
        }
        // The current viewport Response is built after routing. Egui will
        // surrender its focus on outside clicks then, so reject that interaction
        // now, before a held key could advance a same-frame tick.
        let outside_pointer = ctx
            .input(|input| input.pointer.any_pressed() || input.pointer.any_click())
            && ctx
                .input(|input| input.pointer.interact_pos())
                .is_none_or(|position| {
                    self.action_surface_rect
                        .is_none_or(|rect| !rect.contains(position))
                });
        if outside_pointer || egui::Popup::is_any_open(ctx) {
            self.release_action_control(ctx);
            self.last_action_frame = Some(frame);
            return;
        }
        let owns_focus = ctx.memory(|memory| {
            self.action_surface
                .is_some_and(|surface| memory.focused() == Some(surface))
        });
        let forbidden_modifier = ctx.input(|i| {
            let chord = |m: egui::Modifiers| m.ctrl || m.alt || m.mac_cmd;
            chord(i.modifiers)
                || i.events.iter().any(|e| match e {
                    egui::Event::Key { modifiers, .. } => chord(*modifiers),
                    egui::Event::ModifiersChanged(modifiers) => chord(*modifiers),
                    _ => false,
                })
        });
        if !owns_focus || forbidden_modifier {
            self.release_action_control(ctx);
            self.last_action_frame = Some(frame);
            return;
        }
        // Discarded layout passes must not replay a short down/up after an
        // earlier pass's tick already consumed it.
        if self.last_action_frame == Some(frame) {
            return;
        }
        self.last_action_frame = Some(frame);
        let events: Vec<_> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Key {
                        key,
                        pressed,
                        repeat,
                        ..
                    } if !repeat => ruster_scene::input::InputKey::from_logical_name(key.name())
                        .map(|key| (key, *pressed)),
                    _ => None,
                })
                .take(257)
                .collect()
        });
        if events.len() > 256 {
            self.release_action_control(ctx);
            self.report(Err(
                "Keyboard input overflow; input suspended. Reset to recover.".into(),
            ));
            return;
        }
        let result = self.runtime.app_mut().action_input(true, &events);
        self.report(result);
    }

    // Shared production layout also permits the GPU-free owner case to inspect
    // real panel/callback bounds without constructing a native eframe::Frame.
    #[cfg(test)]
    fn scene_ui(&mut self, root: &mut egui::Ui) {
        self.scene_ui_with_input_time(root, None);
    }
    fn scene_ui_with_input_time(&mut self, root: &mut egui::Ui, input_time: Option<Duration>) {
        let ctx = root.ctx().clone();
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
                if self.runtime.app().keyboard_actions().is_some() {
                    let label = if self.action_control_owned { "Release controls (Esc)" } else { "Control runtime" };
                    if ui.add_enabled(self.runtime.is_running() && self.error.is_none() && self.action_surface.is_some(), egui::Button::new(label))
                        .on_hover_text("Keyboard controls belong to the scene. Esc releases them; use Pause or Esc then Space to pause.")
                        .clicked() && self.last_control_frame != Some(ctx.cumulative_frame_nr()) {
                        self.last_control_frame = Some(ctx.cumulative_frame_nr());
                        if self.action_control_owned {
                            self.release_action_control(&ctx);
                        } else {
                            self.action_control_owned = true;
                            self.action_claim_frame = Some(ctx.cumulative_frame_nr());
                            if let Some(surface) = self.action_surface {
                                ctx.memory_mut(|memory| memory.request_focus(surface));
                            }
                        }
                    }
                }

                if let Some(message) = self.metrics.lock().ok().and_then(|metrics| {
                    (metrics.frames > 0)
                        .then(|| metrics.display_status.warning())
                        .flatten()
                }) {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(message).color(ui.visuals().warn_fg_color),
                        )
                        .truncate(),
                    )
                    .on_hover_text(message);
                }
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
        if let Some(elapsed) = input_time {
            let advanced = self.runtime.advance(elapsed).map(|_| ());
            self.report(advanced);
        }

        // Application UI/input may have edited scene data. Invalid state never reaches the GPU.
        let valid = self.runtime.app().content().validate();
        let can_render = valid.is_ok();
        self.report(valid);
        egui::CentralPanel::default().show(root, |ui| {
            if can_render {
                let app = self.runtime.app();
                let content = app.content();
                let selected = app.selected();
                let viewport = ruster_render::viewport_with_materials(
                    ui,
                    content.scene(),
                    selected.as_slice(),
                    content.meshes(),
                    content.textures(),
                    &mut self.navigation,
                );
                if self.runtime.app().keyboard_actions().is_some() {
                    self.action_surface = Some(viewport.response.id);
                    self.action_surface_rect = Some(viewport.rect);
                    // The activation button is outside the viewport. Request
                    // focus only after its real Response has processed that click.
                    if self.action_claim_frame == Some(ctx.cumulative_frame_nr())
                        && self.action_control_owned
                    {
                        viewport.response.request_focus();
                    }
                }
                if let Some(id) = viewport.picked {
                    self.runtime.app_mut().select(&id);
                }
            } else {
                ui.label(
                    "Scene unavailable. Correct the application content and Reset to recover.",
                );
            }
        });
    }
}

impl<A: Application> eframe::App for Host<A> {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(render) = &self.render {
            let result = ruster_render::maintain_assets(render, ctx).map(|_| ());
            self.report(result);
        }
    }
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
        let authored_input = self.runtime.app().keyboard_actions().is_some();
        if !authored_input {
            let advanced = self.runtime.advance(elapsed).map(|_| ());
            self.report(advanced);
        }
        self.scene_ui_with_input_time(root, authored_input.then_some(elapsed));
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
        actions: Option<ruster_scene::input::ActionMap>,
        action_events: Vec<Vec<(ruster_scene::input::InputKey, bool)>>,
        action_owned: bool,
        text: String,
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
        fn keyboard_actions(&self) -> Option<&ruster_scene::input::ActionMap> {
            self.actions.as_ref()
        }
        fn action_input(
            &mut self,
            owned: bool,
            events: &[(ruster_scene::input::InputKey, bool)],
        ) -> Result<(), String> {
            self.action_owned = owned;
            self.action_events.push(events.to_vec());
            Ok(())
        }
        fn cancel_action_input(&mut self) {
            self.action_owned = false;
        }
        fn ui(&mut self, ui: &mut egui::Ui) -> Result<(), String> {
            if self.actions.is_some() {
                ui.text_edit_singleline(&mut self.text);
            }
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

    // Unique boundary: the shared native host must acquire a real viewport
    // response, suppress activation/repeat keys and fold input once per frame.
    #[test]
    fn authored_controls_use_real_viewport_focus_and_do_not_replay_discarded_passes() {
        use ruster_scene::input::{ActionDefinition, ActionKind, InputKey};
        let app = KeyboardApp {
            content: Content::new(ruster_scene::Scene::demo(), MeshMap::new()).unwrap(),
            inputs: Vec::new(),
            actions: Some(std::collections::BTreeMap::from([(
                "fire".into(),
                ActionDefinition {
                    label: None,
                    kind: ActionKind::Button {
                        keys: vec![InputKey::Space],
                    },
                },
            )])),
            action_events: Vec::new(),
            action_owned: false,
            text: String::new(),
        };
        let options = HostOptions::default();
        let mut host = Host {
            action_surface: None,
            action_surface_rect: None,
            action_control_owned: false,
            action_claim_frame: None,
            last_control_frame: None,
            last_action_frame: None,
            navigation: ruster_render::NavigationOptions::default(),
            runtime: Runtime::new(app, options.tick_duration).unwrap(),
            options,
            last_frame: Instant::now(),
            metrics: Arc::new(Mutex::new(ruster_render::Metrics::default())),
            render: None,
            error: None,
            ui_frames: 0,
            exit_error: Arc::new(Mutex::new(None)),
        };
        host.runtime.set_running(true);
        let ctx = egui::Context::default();
        let mut time = 0.;
        let mut draw = |host: &mut Host<KeyboardApp>,
                        events: Vec<egui::Event>,
                        focused: bool,
                        discard: bool| {
            time += 0.1;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused,
                    events,
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 800.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    host.scene_ui(ui);
                    if discard && ctx.current_pass_index() == 0 {
                        ctx.request_discard("input pass replay guard");
                    }
                },
            );
            output.textures_delta.clear();
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) if text.galley.job.text == "Control runtime" => {
                    Some(egui::Rect::from_min_size(text.pos, text.galley.size()).center())
                }
                _ => None,
            })
        };
        let mut control = None;
        for _ in 0..3 {
            control = draw(&mut host, vec![], true, false);
        }
        let position = control.expect("real production Control runtime button");
        let click = |position| {
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        draw(&mut host, click(position), true, true);
        assert!(
            host.action_control_owned,
            "actual button must claim control"
        );
        assert!(
            !host.runtime.app().action_owned,
            "activation frame admits no gameplay input"
        );
        let before = host.runtime.app().action_events.len();
        let mut release = key(egui::Key::Space, egui::Modifiers::NONE);
        if let egui::Event::Key { pressed, .. } = &mut release {
            *pressed = false;
        }
        draw(
            &mut host,
            vec![key(egui::Key::Space, egui::Modifiers::NONE), release],
            true,
            true,
        );
        assert!(
            host.runtime.is_running(),
            "mapped Space belongs to gameplay while controlled"
        );
        assert_eq!(
            host.runtime.app().action_events.len(),
            before + 1,
            "discarded pass cannot duplicate a tap"
        );
        assert_eq!(
            host.runtime.app().action_events.last().unwrap(),
            &vec![(InputKey::Space, true), (InputKey::Space, false)]
        );
        assert!(host.runtime.app().action_owned);
        draw(&mut host, vec![], false, false);
        assert!(!host.action_control_owned);
        assert!(
            !host.runtime.app().action_owned,
            "real window blur cancels control"
        );
        let control = draw(&mut host, vec![], true, false).unwrap();
        draw(&mut host, click(control), true, false);
        assert!(host.action_control_owned);
        draw(
            &mut host,
            vec![key(egui::Key::Space, egui::Modifiers::NONE)],
            true,
            false,
        );
        draw(
            &mut host,
            vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
            true,
            false,
        );
        let control = draw(&mut host, vec![], true, false).unwrap();
        draw(&mut host, click(control), true, false);
        assert!(host.action_control_owned);
        let mut repeat = key(egui::Key::Space, egui::Modifiers::NONE);
        if let egui::Event::Key { repeat: flag, .. } = &mut repeat {
            *flag = true;
        }
        draw(&mut host, vec![repeat], true, false);
        assert!(
            host.runtime.app().action_events.last().unwrap().is_empty(),
            "OS repeat cannot revive a held key after cancellation"
        );
        let before_outside = host.runtime.app().action_events.len();
        let mut outside = click(egui::pos2(1190., 790.));
        outside.push(key(egui::Key::A, egui::Modifiers::NONE));
        draw(&mut host, outside, true, false);
        assert_eq!(
            host.runtime.app().action_events.len(),
            before_outside,
            "outside interaction cancels before admitting a same-frame key"
        );
        assert!(!host.action_control_owned);
        assert!(host.runtime.is_running(), "Escape releases without pausing");
        let mut released_space = key(egui::Key::Space, egui::Modifiers::NONE);
        if let egui::Event::Key { pressed, .. } = &mut released_space {
            *pressed = false;
        }
        draw(&mut host, vec![released_space], true, false);
        draw(
            &mut host,
            vec![key(egui::Key::Space, egui::Modifiers::NONE)],
            true,
            false,
        );
        assert!(
            !host.runtime.is_running(),
            "unowned Space retains the lifecycle shortcut"
        );
    }

    #[test]
    fn real_egui_focus_routes_shortcuts_text_and_held_state() {
        let app = KeyboardApp {
            content: Content::new(ruster_scene::Scene::demo(), MeshMap::new()).unwrap(),
            inputs: Vec::new(),
            actions: None,
            action_events: Vec::new(),
            action_owned: false,
            text: String::new(),
        };
        let options = HostOptions::default();
        let mut host = Host {
            action_surface: None,
            action_surface_rect: None,
            action_control_owned: false,
            action_claim_frame: None,
            last_control_frame: None,
            last_action_frame: None,
            navigation: ruster_render::NavigationOptions::default(),
            runtime: Runtime::new(app, options.tick_duration).unwrap(),
            options,
            last_frame: Instant::now(),
            metrics: Arc::new(Mutex::new(ruster_render::Metrics::default())),
            render: None,
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

        // Transient renderer Pending must not resize the application's actual
        // viewport or push ordinary lifecycle controls outside a narrow window.
        let layout_ctx = egui::Context::default();
        let mut time = 0.0;
        let mut layout = |host: &mut Host<KeyboardApp>| {
            time += 0.1;
            let mut output = layout_ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 540.),
                    )),
                    ..Default::default()
                },
                |root| host.scene_ui(root),
            );
            output.textures_delta.clear();
            for action in ["Run", "Step", "Reset"] {
                assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::epaint::Shape::Text(text) if text.galley.job.text == action
                    && shape.clip_rect.contains_rect(egui::Rect::from_min_size(text.pos, text.galley.size())))),
                    "lifecycle control must remain visible at800: {action}");
            }
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Callback(callback) => {
                        Some(callback.rect.intersect(shape.clip_rect))
                    }
                    _ => None,
                })
                .expect("production scene layout must emit its real viewport callback")
        };
        for _ in 0..3 {
            layout(&mut host);
        }
        let ordinary = layout(&mut host);
        for status in [
            ruster_render::DisplayStatus::Pending,
            ruster_render::DisplayStatus::PendingUnavailable,
        ] {
            {
                let mut metrics = host.metrics.lock().unwrap();
                metrics.frames = 1;
                metrics.display_status = status;
            }
            for _ in 0..3 {
                assert_eq!(
                    layout(&mut host),
                    ordinary,
                    "resource waiting warning must share the ordinary header footprint"
                );
            }
            host.metrics.lock().unwrap().display_status = ruster_render::DisplayStatus::LegacySdr;
            for _ in 0..3 {
                assert_eq!(
                    layout(&mut host),
                    ordinary,
                    "clearing a waiting warning must preserve the same viewport bounds"
                );
            }
        }
    }
}
