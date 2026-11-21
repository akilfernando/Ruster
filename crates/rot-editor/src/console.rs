//! The editor's message log, Console panel and toasts (UX-STANDARDS §3, §5, §6).
//!
//! Every message the editor shows lands here once: notices, errors, script
//! diagnostics (with file and line), import and build results, and engine
//! warnings from the `log` crate. The status bar shows the latest one; the
//! Console lists them all; toasts confirm what the creator just did.
use crate::strings::Text;
use eframe::egui::{self, RichText};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::ui_kit::tokens::*;

const CAPACITY: usize = 2000;
const TOAST_TIME: Duration = Duration::from_secs(4);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
    Error,
}

impl Level {
    fn index(self) -> usize {
        self as usize
    }
    fn icon(self) -> &'static str {
        match self {
            Self::Info => "●",
            Self::Warning => "⚠",
            Self::Error => "×",
        }
    }
    fn color(self, ui: &egui::Ui) -> egui::Color32 {
        let palette = crate::editor_theme::colors(ui);
        match self {
            Self::Info => palette.secondary,
            Self::Warning => palette.warning,
            Self::Error => palette.error,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub level: Level,
    pub source: &'static str,
    /// The full message. The list and status bar show its first line.
    pub text: String,
    /// Script source and line, for double-click to open.
    pub location: Option<(String, u32)>,
    /// Identical consecutive messages count up instead of repeating.
    pub repeats: u32,
}

impl Entry {
    pub fn summary(&self) -> &str {
        self.text
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
            .trim()
    }
}

struct Toast {
    text: String,
    shown: Instant,
}

/// Engine log records waiting for the Console (filled by the `log` backend).
static ENGINE_LOG: Mutex<Vec<(Level, String)>> = Mutex::new(Vec::new());

/// Called by the editor's `log` backend for warnings and errors.
pub fn engine_log(level: log::Level, text: String) {
    let level = if level == log::Level::Error {
        Level::Error
    } else {
        Level::Warning
    };
    if let Ok(mut queue) = ENGINE_LOG.lock() {
        if queue.len() < CAPACITY {
            queue.push((level, text));
        }
    }
}

#[derive(Default)]
pub struct Console {
    entries: VecDeque<Entry>,
    counts: [usize; 3],
    /// The last text seen from each observed slot, so a message set once
    /// and kept on screen is logged once.
    observed: Vec<(&'static str, String)>,
    selected: Option<usize>,
    collapse: bool,
    hide: [bool; 3],
    toasts: Vec<Toast>,
    /// The Console tab is showing instead of Assets.
    pub open: bool,
}

/// What the creator asked for from the Console this frame.
pub enum Request {
    /// Open a script in the configured code editor at this line.
    OpenSource(String, u32),
}

impl Console {
    pub fn push(&mut self, level: Level, source: &'static str, text: impl Into<String>) {
        self.push_entry(Entry {
            level,
            source,
            text: text.into(),
            location: None,
            repeats: 1,
        });
    }

    pub fn push_entry(&mut self, entry: Entry) {
        if entry.text.trim().is_empty() {
            return;
        }
        if let Some(last) = self.entries.back_mut() {
            if last.level == entry.level && last.text == entry.text && last.source == entry.source {
                last.repeats += 1;
                return;
            }
        }
        self.counts[entry.level.index()] += 1;
        if self.entries.len() == CAPACITY {
            if let Some(old) = self.entries.pop_front() {
                self.counts[old.level.index()] -= 1;
            }
            self.selected = self.selected.and_then(|index| index.checked_sub(1));
        }
        self.entries.push_back(entry);
    }

    /// Log `text` from `source` when it differs from what that slot showed
    /// last frame. Returns true when a new message was logged.
    pub fn observe(
        &mut self,
        slot: &'static str,
        source: &'static str,
        level: Level,
        text: &str,
    ) -> bool {
        let slot = match self.observed.iter().position(|(name, _)| *name == slot) {
            Some(index) => index,
            None => {
                self.observed.push((slot, String::new()));
                self.observed.len() - 1
            }
        };
        if self.observed[slot].1 == text {
            return false;
        }
        self.observed[slot].1 = text.to_owned();
        if text.trim().is_empty() {
            return false;
        }
        self.push(level, source, text);
        true
    }

    /// Move queued engine warnings and errors into the log.
    pub fn drain_engine_log(&mut self) {
        let records = match ENGINE_LOG.lock() {
            Ok(mut queue) => std::mem::take(&mut *queue),
            Err(_) => return,
        };
        for (level, text) in records {
            self.push(level, "Engine", text);
        }
    }

    /// Swap the newest entry for a richer one describing the same message.
    pub fn replace_latest(&mut self, entry: Entry) {
        if let Some(last) = self.entries.back_mut() {
            if last.text == entry.text && last.level == entry.level {
                last.location = entry.location;
                last.source = entry.source;
                return;
            }
        }
        self.push_entry(entry);
    }

    pub fn latest(&self) -> Option<&Entry> {
        self.entries.back()
    }

    pub fn count(&self, level: Level) -> usize {
        self.counts[level.index()]
    }

    #[cfg(test)]
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.counts = [0; 3];
        self.selected = None;
    }

    /// A short confirmation of something the creator just did.
    pub fn toast(&mut self, text: impl Into<String>) {
        let text = text.into();
        let first = text
            .split(". ")
            .next()
            .unwrap_or(&text)
            .trim_end_matches('.');
        if first.is_empty() {
            return;
        }
        self.toasts.retain(|toast| toast.text != first);
        self.toasts.push(Toast {
            text: first.to_owned(),
            shown: Instant::now(),
        });
        if self.toasts.len() > 3 {
            self.toasts.remove(0);
        }
    }

    /// Bottom-right toasts. Repaints only while one is showing.
    pub fn toasts_ui(&mut self, ctx: &egui::Context, bottom_right: egui::Pos2) {
        self.toasts
            .retain(|toast| toast.shown.elapsed() < TOAST_TIME);
        let Some(oldest) = self.toasts.first() else {
            return;
        };
        ctx.request_repaint_after(TOAST_TIME.saturating_sub(oldest.shown.elapsed()));
        egui::Area::new(egui::Id::unique("rot_toasts"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::RIGHT_BOTTOM)
            .fixed_pos(bottom_right - egui::vec2(SPACE_L, SPACE_L))
            .interactable(false)
            .show(ctx, |ui| {
                for toast in &self.toasts {
                    egui::Frame::popup(ui.style())
                        .inner_margin(egui::Margin::symmetric(SPACE_M as i8, SPACE_S as i8))
                        .show(ui, |ui| {
                            ui.set_max_width(360.0);
                            ui.add(egui::Label::new(&toast.text).truncate());
                        });
                    ui.add_space(SPACE_XS);
                }
            });
    }

    /// The Console panel: toolbar, message list and the selected message.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<Request> {
        let mut request = None;
        ui.horizontal(|ui| {
            if crate::ui_kit::button(ui, Text::Clear.plain()).clicked() {
                self.clear();
            }
            ui.toggle_value(&mut self.collapse, "Collapse")
                .on_hover_text(Text::ConsoleShowIdenticalMessagesOnce.plain());
            ui.separator();
            for level in [Level::Info, Level::Warning, Level::Error] {
                let mut shown = !self.hide[level.index()];
                let label = RichText::new(format!("{} {}", level.icon(), self.count(level)))
                    .color(level.color(ui));
                if ui
                    .toggle_value(&mut shown, label)
                    .on_hover_text(match level {
                        Level::Info => Text::ShowMessages.plain(),
                        Level::Warning => Text::ShowWarnings.plain(),
                        Level::Error => Text::ShowErrors.plain(),
                    })
                    .changed()
                {
                    self.hide[level.index()] = !shown;
                }
            }
        });
        ui.separator();
        let visible: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| !self.hide[entry.level.index()])
            .filter(|(index, entry)| {
                // Collapse keeps the latest of each identical message.
                !self.collapse
                    || !self
                        .entries
                        .iter()
                        .skip(index + 1)
                        .any(|later| later.text == entry.text && later.level == entry.level)
            })
            .map(|(index, _)| index)
            .collect();
        if visible.is_empty() {
            crate::ui_kit::empty_state(
                ui,
                crate::strings::tr(ui.ctx(), crate::strings::Text::EmptyConsole),
                None,
            );
            return None;
        }
        let detail_height = if self.selected.is_some() { 72.0 } else { 0.0 };
        egui::ScrollArea::vertical()
            .id_salt("console_list")
            .max_height((ui.available_height() - detail_height).max(ROW_HEIGHT))
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show_rows(ui, ROW_HEIGHT, visible.len(), |ui, rows| {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for row in rows {
                        let index = visible[row];
                        let entry = &self.entries[index];
                        let repeats = if self.collapse {
                            self.entries
                                .iter()
                                .filter(|other| {
                                    other.text == entry.text && other.level == entry.level
                                })
                                .map(|other| other.repeats)
                                .sum()
                        } else {
                            entry.repeats
                        };
                        let mut line = format!(
                            "{}  [{}] {}",
                            entry.level.icon(),
                            entry.source,
                            entry.summary()
                        );
                        if repeats > 1 {
                            line.push_str(&format!("  ×{repeats}"));
                        }
                        let response = ui.add(
                            egui::Button::selectable(
                                self.selected == Some(index),
                                RichText::new(line).color(if entry.level == Level::Info {
                                    ui.visuals().text_color()
                                } else {
                                    entry.level.color(ui)
                                }),
                            )
                            .min_size(egui::vec2(0.0, ROW_HEIGHT))
                            .truncate(),
                        );
                        if response.clicked() {
                            self.selected = Some(index);
                        }
                        if response.double_clicked() {
                            if let Some((source, line)) = &entry.location {
                                request = Some(Request::OpenSource(source.clone(), *line));
                            }
                        }
                        if let Some((source, line)) = &entry.location {
                            response.on_hover_text(format!(
                                "{source}:{line} · double-click to open in the code editor"
                            ));
                        }
                    }
                });
            });
        if let Some(entry) = self.selected.and_then(|index| self.entries.get(index)) {
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("console_detail")
                .max_height(detail_height)
                .show(ui, |ui| {
                    let mut text = entry.text.clone();
                    if let Some((source, line)) = &entry.location {
                        text.push_str(&format!("\n{source}:{line}"));
                    }
                    ui.add(
                        egui::TextEdit::multiline(&mut text)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .interactive(true),
                    );
                });
        }
        request
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Double-clicking a script error asks to open its file at its line.
    #[test]
    fn double_clicking_a_script_error_opens_its_location() {
        let mut console = Console {
            open: true,
            ..Default::default()
        };
        console.push_entry(Entry {
            level: Level::Error,
            source: "Script",
            text: "attempt to call a nil value".into(),
            location: Some(("mover".into(), 12)),
            repeats: 1,
        });
        let ctx = egui::Context::default();
        let mut request = None;
        let mut row = egui::Pos2::ZERO;
        for events in [vec![], vec![], vec!["click"], vec!["click"]] {
            let pointer = |pressed| egui::Event::PointerButton {
                pos: row,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let events = events
                .into_iter()
                .flat_map(|_| {
                    [
                        egui::Event::PointerMoved(row),
                        pointer(true),
                        pointer(false),
                    ]
                })
                .collect();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    time: Some(0.0),
                    ..Default::default()
                },
                |ui| {
                    if let Some(found) = console.ui(ui) {
                        request = Some(found);
                    }
                },
            );
            output.textures_delta.clear();
            row = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text)
                        if text.galley.job.text.contains("attempt to call") =>
                    {
                        Some(text.pos + text.galley.size() / 2.0)
                    }
                    _ => None,
                })
                .unwrap_or(row);
        }
        match request {
            Some(Request::OpenSource(source, line)) => {
                assert_eq!((source.as_str(), line), ("mover", 12));
            }
            None => panic!("double-click must request the script location"),
        }
    }

    #[test]
    fn observed_messages_are_logged_once_and_repeats_count_up() {
        let mut console = Console::default();
        assert!(console.observe("notice", "Editor", Level::Info, "Saved"));
        assert!(
            !console.observe("notice", "Editor", Level::Info, "Saved"),
            "unchanged"
        );
        assert!(
            !console.observe("notice", "Editor", Level::Info, ""),
            "cleared"
        );
        assert!(
            console.observe("notice", "Editor", Level::Info, "Saved"),
            "shown again"
        );
        assert_eq!(console.entries().count(), 1);
        assert_eq!(console.latest().unwrap().repeats, 2);
        console.observe("script", "Script", Level::Error, "boom");
        assert_eq!(console.count(Level::Error), 1);
        assert_eq!(console.count(Level::Info), 1);
        engine_log(log::Level::Warn, "adapter lost".into());
        console.drain_engine_log();
        assert_eq!(console.latest().unwrap().source, "Engine");
        assert_eq!(console.count(Level::Warning), 1);
        console.clear();
        assert_eq!(console.entries().count(), 0);
        assert_eq!(console.count(Level::Error), 0);
    }
}
