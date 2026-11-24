//! Compositor-confirmed window mode for the packaged Linux player.
use crate::creation::{PlayerWindow, PlayerWindowMode};
use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::time::{Duration, Instant};
#[cfg(target_os = "linux")]
use x11rb::{
    protocol::xproto::{Atom, AtomEnum, ConnectionExt},
    rust_connection::RustConnection,
};

const CONFIRM_TIME: Duration = Duration::from_secs(2);
const PROBE_INTERVAL: Duration = Duration::from_millis(50);
const SIZE_RESTORE_TIME: Duration = Duration::from_millis(500);

pub(super) struct WindowMode {
    probe: Probe,
    requested: Option<(bool, Instant)>,
    observed: Option<bool>,
    windowed_size: egui::Vec2,
    #[cfg(target_os = "linux")]
    last_size: Option<egui::Vec2>,
    restore_until: Option<Instant>,
    #[cfg(target_os = "linux")]
    next_probe: Instant,
    #[cfg(target_os = "linux")]
    probe_attempted: bool,
    f11_down: bool,
    warning: Option<&'static str>,
}

impl WindowMode {
    pub(super) fn new(owner: &impl HasWindowHandle, player: &PlayerWindow) -> Self {
        let probe = match owner.window_handle().map(|handle| handle.as_raw()) {
            Ok(RawWindowHandle::Wayland(_)) => Probe::Wayland,
            #[cfg(target_os = "linux")]
            Ok(RawWindowHandle::Xlib(handle)) => u32::try_from(handle.window)
                .ok()
                .and_then(Probe::x11)
                .unwrap_or(Probe::Unavailable),
            #[cfg(target_os = "linux")]
            Ok(RawWindowHandle::Xcb(handle)) => {
                Probe::x11(handle.window.get()).unwrap_or(Probe::Unavailable)
            }
            _ => Probe::Unavailable,
        };
        let now = Instant::now();
        let size = player.effective_size();
        Self {
            probe,
            requested: (player.effective_mode() == PlayerWindowMode::BorderlessFullscreen)
                .then_some((true, now + CONFIRM_TIME)),
            observed: None,
            windowed_size: egui::vec2(size.width as f32, size.height as f32),
            #[cfg(target_os = "linux")]
            last_size: None,
            restore_until: None,
            #[cfg(target_os = "linux")]
            next_probe: now,
            #[cfg(target_os = "linux")]
            probe_attempted: false,
            f11_down: false,
            warning: None,
        }
    }

    pub(super) fn input(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput, blurred: bool) {
        let toggle = self.consume_f11(raw, blurred);
        let now = Instant::now();
        let info = raw.viewport();
        let size = info
            .inner_rect
            .map(|rect| rect.size())
            .filter(|size| size.is_finite() && size.x > 0.0 && size.y > 0.0);
        #[cfg(target_os = "linux")]
        let previous_size = std::mem::replace(&mut self.last_size, size);
        match &self.probe {
            Probe::Wayland => self.observed = info.fullscreen,
            #[cfg(target_os = "linux")]
            Probe::X11(x11) => {
                if toggle
                    || previous_size != size
                    || !self.probe_attempted
                    || self.requested.is_some() && now >= self.next_probe
                {
                    self.observed = x11.fullscreen();
                    self.probe_attempted = true;
                    self.next_probe = now + PROBE_INTERVAL;
                }
            }
            Probe::Unavailable => {}
        }
        if let Some((goal, deadline)) = self.requested {
            if self.observed == Some(goal) {
                self.requested = None;
                self.warning = None;
                if !goal {
                    self.restore_until = Some(now + SIZE_RESTORE_TIME);
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.windowed_size));
                }
            } else if now >= deadline {
                self.requested = None;
                self.warning = Some(if goal {
                    "The desktop did not confirm fullscreen."
                } else {
                    "The desktop did not confirm windowed mode."
                });
            } else {
                ctx.request_repaint_after(PROBE_INTERVAL);
            }
        }

        if self.observed == Some(false) && self.requested.is_none() {
            if let Some(deadline) = self.restore_until {
                if size.is_some_and(|size| (size - self.windowed_size).length_sq() <= 1.0) {
                    self.restore_until = None;
                } else if now >= deadline {
                    self.restore_until = None;
                    self.warning = Some("The desktop could not restore the previous window size.");
                } else {
                    ctx.request_repaint_after(PROBE_INTERVAL);
                }
            }
            if self.restore_until.is_none() {
                if let Some(size) = size {
                    self.windowed_size = size;
                }
            }
        }
        if toggle && self.requested.is_none() {
            if let Some(actual) = self.observed {
                self.requested = Some((!actual, now + CONFIRM_TIME));
                self.warning = None;
                self.restore_until = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!actual));
                ctx.request_repaint_after(PROBE_INTERVAL);
            } else {
                self.warning = Some("The desktop did not report its window mode.");
            }
        }
    }

    /// Every F11 event belongs here, including releases, repeats and blurred
    /// batches. A held key can request a change at most once.
    fn consume_f11(&mut self, raw: &mut egui::RawInput, blurred: bool) -> bool {
        let mut toggle = false;
        if blurred {
            self.f11_down = false;
        }
        raw.events.retain(|event| {
            let egui::Event::Key {
                key: egui::Key::F11,
                pressed,
                repeat,
                ..
            } = event
            else {
                return true;
            };
            if !pressed {
                self.f11_down = false;
            } else if !blurred && !repeat && !self.f11_down {
                self.f11_down = true;
                toggle = true;
            }
            false
        });
        toggle
    }

    pub(super) fn show(&self, ui: &mut egui::Ui) {
        let label = match (self.requested, self.observed) {
            (Some((true, _)), _) => "Switching to fullscreen…",
            (Some((false, _)), _) => "Switching to windowed…",
            (None, Some(true)) => "Borderless Fullscreen · F11",
            (None, Some(false)) => "Windowed · F11",
            (None, None) => "Window Mode Unavailable · F11",
        };
        ui.add(egui::Label::new(label).truncate())
            .on_hover_text("F11 switches between Windowed and Borderless Fullscreen.");
    }

    pub(super) fn warning(&self) -> Option<&'static str> {
        self.warning
    }
}

enum Probe {
    Wayland,
    #[cfg(target_os = "linux")]
    X11(X11Probe),
    Unavailable,
}

#[cfg(target_os = "linux")]
struct X11Probe {
    connection: RustConnection,
    window: u32,
    state: Atom,
    fullscreen: Atom,
}

#[cfg(target_os = "linux")]
impl Probe {
    fn x11(window: u32) -> Option<Self> {
        let (connection, _) = x11rb::connect(None).ok()?;
        let atom = |name: &[u8]| Some(connection.intern_atom(false, name).ok()?.reply().ok()?.atom);
        let state = atom(b"_NET_WM_STATE")?;
        let fullscreen = atom(b"_NET_WM_STATE_FULLSCREEN")?;
        Some(Self::X11(X11Probe {
            connection,
            window,
            state,
            fullscreen,
        }))
    }
}

#[cfg(target_os = "linux")]
impl X11Probe {
    fn fullscreen(&self) -> Option<bool> {
        // The X11 winit getter may report the requested mode before the window
        // manager changes it. Read the compositor's property on this window.
        let reply = self
            .connection
            .get_property(false, self.window, self.state, AtomEnum::ATOM, 0, 64)
            .ok()?
            .reply()
            .ok()?;
        if reply.type_ == u32::from(AtomEnum::NONE) {
            return Some(false);
        }
        if reply.bytes_after != 0 {
            return None;
        }
        let fullscreen = reply.value32()?.any(|state| state == self.fullscreen);
        Some(fullscreen)
    }
}
