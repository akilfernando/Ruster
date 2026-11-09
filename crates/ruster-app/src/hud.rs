//! Shared passive viewport HUD painting. No widgets or input responses are created.
use crate::egui::{self, Color32, FontId, Painter, Rect, Vec2};
use ruster_scene::hud::{Hud, HudAnchor, ResolvedHud, MAX_HUD_LABELS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudPresentationMode {
    Committed,
    AuthoredPreview,
    Stale,
    Stopped,
}

impl HudPresentationMode {
    fn badge(self) -> Option<&'static str> {
        match self {
            Self::Committed => None,
            Self::AuthoredPreview => Some("HUD preview · authored defaults"),
            Self::Stale => Some("HUD · last committed · Reset required"),
            Self::Stopped => Some("HUD preview · stopped · authored defaults"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct HudPaintReport {
    pub clipped: bool,
    pub panel_rect: Option<Rect>,
    /// Natural definition indices corresponding to the laid-out visible rows.
    pub label_indices: Vec<usize>,
    /// Full row rectangles, including at most one partially clipped boundary row.
    pub label_rects: Vec<Rect>,
}

fn color(components: [f64; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (components[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (components[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (components[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        (components[3].clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

/// Paint within the already allocated 3D viewport. Effective geometry is clamped
/// to that viewport without changing the authored layout or creating hit areas.
/// Consumers attach their own scene/run/tick/commit identity to prepared values.
pub fn paint_hud(
    painter: &Painter,
    viewport: Rect,
    hud: &Hud,
    resolved: &ResolvedHud,
    mode: HudPresentationMode,
) -> HudPaintReport {
    if !hud.enabled || resolved.labels.is_empty() || !viewport.is_positive() {
        return HudPaintReport::default();
    }
    let painter = painter.with_clip_rect(viewport.intersect(painter.clip_rect()));
    let margin = (hud.margin as f32).clamp(0.0, viewport.width().min(viewport.height()) * 0.25);
    let available = viewport.shrink(margin);
    let width = (hud.width as f32).clamp(1.0, available.width().max(1.0));
    let padding = 8.0_f32.min(width * 0.25);
    let text_width = (width - padding * 2.0).max(1.0);
    let font = FontId::proportional((hud.font_size as f32).clamp(8.0, 48.0));
    let text_color = color(hud.text_color);
    let spacing = (hud.spacing as f32).clamp(0.0, 64.0);
    let badge = mode.badge();
    let has_unavailable = resolved
        .labels
        .iter()
        .any(|label| label.diagnostic.is_some());
    let badge_galley = badge.map(|badge| {
        painter.layout(
            badge.to_owned(),
            FontId::proportional(12.0),
            Color32::LIGHT_YELLOW,
            text_width,
        )
    });
    let badge_height = badge_galley
        .as_ref()
        .map_or(0.0, |galley| galley.size().y + spacing);
    let right = matches!(hud.anchor, HudAnchor::TopRight | HudAnchor::BottomRight);
    let bottom = matches!(hud.anchor, HudAnchor::BottomLeft | HudAnchor::BottomRight);
    let x = if right {
        available.right() - width
    } else {
        available.left()
    };
    let content_height = (available.height() - padding * 2.0 - badge_height).max(0.0);
    let count = resolved.labels.len().min(MAX_HUD_LABELS);
    let mut rows = Vec::new();
    let mut used_height = 0.0;
    // Walk inward from the anchored edge. Resolving every label is independent
    // from shaping text, so rows outside this viewport need no layout work.
    for offset in 0..count {
        let gap = if offset == 0 { 0.0 } else { spacing };
        if used_height + gap >= content_height {
            break;
        }
        let index = if bottom { count - 1 - offset } else { offset };
        let galley = painter.layout(
            resolved.labels[index].text.clone(),
            font.clone(),
            text_color,
            text_width,
        );
        let height = galley.size().y;
        used_height += gap;
        let row_y = if bottom {
            available.bottom() - padding - used_height - height
        } else {
            available.top() + padding + badge_height + used_height
        };
        rows.push((index, egui::pos2(x + padding, row_y), galley));
        used_height += height;
        if used_height > content_height {
            break;
        }
    }
    let clipped = rows.len() < count
        || used_height > content_height
        || padding * 2.0 + badge_height > available.height();
    let height = if clipped {
        available.height()
    } else {
        padding * 2.0 + badge_height + used_height
    };
    let y = if bottom {
        available.bottom() - height
    } else {
        available.top()
    };
    let panel_rect = Rect::from_min_size(egui::pos2(x, y), Vec2::new(width, height));
    painter.rect_filled(panel_rect, 5.0, color(hud.background));
    if let Some(galley) = badge_galley {
        painter
            .with_clip_rect(panel_rect.intersect(painter.clip_rect()))
            .galley(
                egui::pos2(x + padding, y + padding),
                galley,
                Color32::LIGHT_YELLOW,
            );
    }
    let content_top = (y + padding + badge_height).min(panel_rect.bottom());
    let content_bottom = (panel_rect.bottom() - padding).max(content_top);
    let content_rect = Rect::from_min_max(
        egui::pos2(x + padding, content_top),
        egui::pos2(x + width - padding, content_bottom),
    );
    let content_painter = painter.with_clip_rect(content_rect.intersect(painter.clip_rect()));
    if bottom {
        rows.reverse();
    }
    let mut label_indices = Vec::with_capacity(rows.len());
    let mut label_rects = Vec::with_capacity(rows.len());
    for (index, position, galley) in rows {
        label_indices.push(index);
        label_rects.push(Rect::from_min_size(position, galley.size()));
        content_painter.galley(position, galley, text_color);
    }
    // Draw the overflow/reference notice last so even a clipped group retains
    // a readable diagnostic. This is paint only and owns no pointer/keyboard area.
    if clipped || has_unavailable {
        let notice = if clipped {
            "HUD clipped to viewport"
        } else {
            "HUD has unavailable values"
        };
        let galley = painter.layout(
            notice.to_owned(),
            FontId::proportional(12.0),
            Color32::LIGHT_YELLOW,
            text_width,
        );
        let notice_height = galley.size().y.min(viewport.height());
        let notice_y = if bottom {
            (panel_rect.top() + padding + badge_height).min(panel_rect.bottom() - notice_height)
        } else {
            panel_rect.bottom() - notice_height - padding
        }
        .max(viewport.top());
        let notice_rect = Rect::from_min_size(
            egui::pos2(x, notice_y),
            Vec2::new(width, notice_height + padding),
        );
        painter.rect_filled(notice_rect, 0.0, Color32::from_black_alpha(240));
        painter.galley(
            egui::pos2(x + padding, notice_y),
            galley,
            Color32::LIGHT_YELLOW,
        );
    }
    HudPaintReport {
        clipped,
        panel_rect: Some(panel_rect),
        label_indices,
        label_rects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruster_scene::hud::ResolvedHudLabel;

    // The reference deliberately shapes the entire group. Production must
    // produce the same visible positions while shaping only the intersecting rows.
    fn complete_layout_reference(
        painter: &Painter,
        viewport: Rect,
        hud: &Hud,
        resolved: &ResolvedHud,
        mode: HudPresentationMode,
    ) -> (bool, Rect, Vec<(usize, Rect)>) {
        let margin = (hud.margin as f32).min(viewport.width().min(viewport.height()) * 0.25);
        let available = viewport.shrink(margin);
        let width = (hud.width as f32).min(available.width());
        let padding = 8.0_f32.min(width * 0.25);
        let wrap_width = width - padding * 2.0;
        let spacing = hud.spacing as f32;
        let badge_height = mode.badge().map_or(0.0, |badge| {
            painter
                .layout(
                    badge.into(),
                    FontId::proportional(12.0),
                    Color32::LIGHT_YELLOW,
                    wrap_width,
                )
                .size()
                .y
                + spacing
        });
        let sizes: Vec<_> = resolved
            .labels
            .iter()
            .map(|label| {
                painter
                    .layout(
                        label.text.clone(),
                        FontId::proportional(hud.font_size as f32),
                        color(hud.text_color),
                        wrap_width,
                    )
                    .size()
            })
            .collect();
        let total_rows = sizes.iter().map(|size| size.y).sum::<f32>()
            + spacing * sizes.len().saturating_sub(1) as f32;
        let natural_height = padding * 2.0 + badge_height + total_rows;
        let clipped = natural_height > available.height();
        let height = natural_height.min(available.height());
        let right = matches!(hud.anchor, HudAnchor::TopRight | HudAnchor::BottomRight);
        let bottom = matches!(hud.anchor, HudAnchor::BottomLeft | HudAnchor::BottomRight);
        let x = if right {
            available.right() - width
        } else {
            available.left()
        };
        let y = if bottom {
            available.bottom() - height
        } else {
            available.top()
        };
        let panel = Rect::from_min_size(egui::pos2(x, y), Vec2::new(width, height));
        let content_top = y + padding + badge_height;
        let content_bottom = panel.bottom() - padding;
        let mut cursor = if bottom {
            content_bottom - total_rows
        } else {
            content_top
        };
        let mut visible = Vec::new();
        for (index, size) in sizes.into_iter().enumerate() {
            let rect = Rect::from_min_size(egui::pos2(x + padding, cursor), size);
            if rect.bottom() > content_top && rect.top() < content_bottom {
                visible.push((index, rect));
            }
            cursor += size.y + spacing;
        }
        (clipped, panel, visible)
    }

    #[test]
    fn viewport_culling_matches_complete_group_for_all_anchors_and_badges() {
        let context = egui::Context::default();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1000.0, 1000.0),
                )),
                ..Default::default()
            },
            |ui| {
                for (count, height) in [(3, 600.0), (32, 180.0)] {
                    let resolved = ResolvedHud {
                        labels: (0..count)
                            .map(|index| ResolvedHudLabel {
                                id: format!("row-{index}"),
                                text: format!(
                                    "Row {index}\n{}",
                                    "Detailed reading ".repeat(index % 3 + 1)
                                ),
                                diagnostic: None,
                            })
                            .collect(),
                    };
                    let viewport =
                        Rect::from_min_size(egui::pos2(30.0, 40.0), Vec2::new(300.0, height));
                    for anchor in [
                        HudAnchor::TopLeft,
                        HudAnchor::TopRight,
                        HudAnchor::BottomLeft,
                        HudAnchor::BottomRight,
                    ] {
                        let hud = Hud {
                            enabled: true,
                            anchor,
                            width: 180.0,
                            ..Hud::default()
                        };
                        for mode in [
                            HudPresentationMode::Committed,
                            HudPresentationMode::AuthoredPreview,
                            HudPresentationMode::Stale,
                        ] {
                            let (clipped, panel, visible) = complete_layout_reference(
                                ui.painter(),
                                viewport,
                                &hud,
                                &resolved,
                                mode,
                            );
                            let report = paint_hud(ui.painter(), viewport, &hud, &resolved, mode);
                            assert_eq!(report.clipped, clipped);
                            let actual_panel = report.panel_rect.unwrap();
                            assert!(actual_panel.min.distance(panel.min) < 0.01);
                            assert!(actual_panel.max.distance(panel.max) < 0.01);
                            assert_eq!(
                                report.label_indices,
                                visible.iter().map(|(index, _)| *index).collect::<Vec<_>>()
                            );
                            assert_eq!(report.label_rects.len(), visible.len());
                            assert!(report
                                .label_indices
                                .windows(2)
                                .all(|pair| pair[0] < pair[1]));
                            for (actual, (_, expected)) in report.label_rects.iter().zip(visible) {
                                assert!(actual.min.distance(expected.min) < 0.01);
                                assert!(actual.max.distance(expected.max) < 0.01);
                            }
                            if clipped {
                                assert!(report.label_rects.len() < count);
                                if matches!(anchor, HudAnchor::BottomLeft | HudAnchor::BottomRight)
                                {
                                    assert_eq!(report.label_indices.last(), Some(&(count - 1)));
                                } else {
                                    assert_eq!(report.label_indices.first(), Some(&0));
                                }
                            } else {
                                assert_eq!(report.label_rects.len(), count);
                            }
                        }
                    }
                }
            },
        );
        output.textures_delta.clear();
    }

    #[test]
    fn anchors_wrap_and_clip_without_resizing_or_taking_input() {
        let context = egui::Context::default();
        let resolved = ResolvedHud {
            labels: vec![ResolvedHudLabel {
                id: "reading".into(),
                text: "A wrapped instrument reading with enough words to span lines".into(),
                diagnostic: None,
            }],
        };
        let viewport = Rect::from_min_size(egui::pos2(30.0, 40.0), Vec2::new(240.0, 180.0));
        let mut hud = Hud {
            enabled: true,
            width: 120.0,
            ..Hud::default()
        };
        for anchor in [
            HudAnchor::TopLeft,
            HudAnchor::TopRight,
            HudAnchor::BottomLeft,
            HudAnchor::BottomRight,
        ] {
            hud.anchor = anchor;
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(800.0, 600.0),
                    )),
                    events: vec![egui::Event::PointerMoved(viewport.center())],
                    ..Default::default()
                },
                |ui| {
                    let focus = ui.ctx().memory(|memory| memory.focused());
                    let pointer_owned = ui.ctx().egui_wants_pointer_input();
                    let available = ui.available_rect_before_wrap();
                    let report = paint_hud(
                        ui.painter(),
                        viewport,
                        &hud,
                        &resolved,
                        HudPresentationMode::Committed,
                    );
                    let panel = report.panel_rect.unwrap();
                    assert!(!report.clipped);
                    assert_eq!(report.label_indices, vec![0]);
                    assert!(viewport.contains_rect(panel));
                    assert!(report.label_rects[0].height() > hud.font_size as f32 * 2.0);
                    assert_eq!(ui.available_rect_before_wrap(), available);
                    assert_eq!(ui.ctx().memory(|memory| memory.focused()), focus);
                    assert_eq!(ui.ctx().egui_wants_pointer_input(), pointer_owned);
                    let right = matches!(anchor, HudAnchor::TopRight | HudAnchor::BottomRight);
                    let bottom = matches!(anchor, HudAnchor::BottomLeft | HudAnchor::BottomRight);
                    assert_eq!(
                        if right { panel.right() } else { panel.left() },
                        if right {
                            viewport.right() - 12.0
                        } else {
                            viewport.left() + 12.0
                        }
                    );
                    assert_eq!(
                        if bottom { panel.bottom() } else { panel.top() },
                        if bottom {
                            viewport.bottom() - 12.0
                        } else {
                            viewport.top() + 12.0
                        }
                    );
                    let compact = paint_hud(
                        ui.painter(),
                        Rect::from_min_size(viewport.min, Vec2::new(70.0, 35.0)),
                        &hud,
                        &resolved,
                        HudPresentationMode::AuthoredPreview,
                    );
                    assert!(compact.clipped);
                },
            );
            output.textures_delta.clear();
        }
    }
}
