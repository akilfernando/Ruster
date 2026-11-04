//! Native authoring frontend. egui paints through wgpu's Vulkan backend only.
mod model;
mod ui;

use std::path::PathBuf;

use eframe::egui;
use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew};
use model::Editor;
use ruster_core::ProjectSession;

// Backend diagnostics must remain visible even without an external logger.
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

const HELP: &str = "Ruster Editor — visual scenario authoring\n\nUsage: ruster-editor [--scenario PATH]\n       ruster-editor --help\n\nDefault: examples/logistics.scenario.json (relative to the working directory).\nRequires a desktop session and Vulkan adapter. Ctrl+N/O/S, Ctrl+Shift+S,\nCtrl+Z / Ctrl+Shift+Z; Space runs/pauses outside text fields.\n";

fn arguments(arguments: impl Iterator<Item = String>) -> Result<Option<PathBuf>, String> {
    let mut arguments = arguments;
    let mut path = PathBuf::from("examples/logistics.scenario.json");
    let mut supplied = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--help" | "-h" => return Ok(None),
            "--scenario" if !supplied => {
                path = arguments
                    .next()
                    .filter(|value| !value.starts_with('-'))
                    .map(PathBuf::from)
                    .ok_or("--scenario requires a file path")?;
                supplied = true;
            }
            _ => {
                return Err(format!(
                    "unknown or repeated argument: {argument}; use --help"
                ))
            }
        }
    }
    Ok(Some(path))
}

fn main() -> std::process::ExitCode {
    log::set_logger(&DIAGNOSTICS).expect("binary installs one logger");
    log::set_max_level(log::LevelFilter::Warn);
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ruster-editor: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let Some(path) = arguments(std::env::args().skip(1))? else {
        print!("{HELP}");
        return Ok(());
    };
    let project = ProjectSession::open(&path).map_err(|error| {
        format!(
            "cannot open {}: {error}. Check the path or use --scenario PATH.",
            path.display()
        )
    })?;
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 640.0])
            .with_app_id("org.ruster.editor"),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: WgpuSetup::CreateNew(setup),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native("Ruster Editor", options, Box::new(move |cc| {
        ui::configure(&cc.egui_ctx);
        let mut editor = Editor::new(project);
        if let Some(render) = cc.wgpu_render_state.as_ref() {
            let adapter = render.adapter.get_info();
            editor.adapter = format!("{} · {:?}", adapter.name, adapter.backend);
            eprintln!("Ruster Editor renderer: {}", editor.adapter);
        }
        Ok(Box::new(editor))
    })).map_err(|error| format!("desktop/Vulkan initialization failed: {error}. Verify the desktop session and Vulkan driver."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_fail_before_desktop_initialization() {
        assert_eq!(arguments(["--help".into()].into_iter()).unwrap(), None);
        assert!(arguments(["--scenario".into()].into_iter()).is_err());
        assert!(arguments(["--typo".into()].into_iter()).is_err());
        assert_eq!(
            arguments(["--scenario".into(), "custom.json".into()].into_iter()).unwrap(),
            Some(PathBuf::from("custom.json"))
        );
    }
}
