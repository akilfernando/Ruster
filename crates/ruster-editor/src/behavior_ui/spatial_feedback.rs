//! Read-only presentation of the last committed spatial-query snapshot.
use super::*;
use ruster_behavior::{spatial::SweepOutcome, QueryObservation, QueryResult};

fn description(observation: &QueryObservation, scene: &ruster_scene::Scene) -> String {
    let name = |id: &str| {
        scene
            .objects
            .iter()
            .find(|object| object.id == id)
            .map_or_else(
                || id.to_string(),
                |object| object.display_name().to_string(),
            )
    };
    match &observation.query {
        QueryResult::Sweep {
            probe_id, result, ..
        } => {
            let probe = name(probe_id);
            let target = result
                .target_id
                .as_deref()
                .map_or_else(|| "blocker".into(), name);
            match result.outcome {
                SweepOutcome::Clear => format!("{probe} · path clear"),
                SweepOutcome::Blocked => format!("{probe} · sweep blocked by {target}"),
                SweepOutcome::InitialOverlap => format!("{probe} · starts inside {target}"),
                SweepOutcome::PrecisionLimited => {
                    format!("{probe} · no safe movement suggestion")
                }
            }
        }
        QueryResult::Overlap { result } => format!(
            "{} · {}",
            name(&result.sensor_id),
            if result.overlapping {
                "occupied"
            } else {
                "clear"
            }
        ),
    }
}

impl RuntimeUi {
    pub(super) fn spatial_feedback_summary(&self, ui: &mut egui::Ui) {
        let Some(frame) = &self.frame else {
            return;
        };
        let Some(queries) = &frame.query_observations else {
            return;
        };
        let (text, detail) = if self.stale || !self.visible {
            let text = "Previous query results · Reset to inspect the current scene".to_string();
            (text.clone(), text)
        } else {
            // Names and geometry stay in the hover/menu. The primary states must
            // remain readable in the fixed Control row at compact viewport widths.
            let mut states = Vec::with_capacity(2);
            let mut details = Vec::with_capacity(2);
            for sweep in [true, false] {
                if let Some(observation) = queries.iter().rev().find(|observation| {
                    matches!(observation.query, QueryResult::Sweep { .. }) == sweep
                }) {
                    states.push(match &observation.query {
                        QueryResult::Sweep { result, .. } => match result.outcome {
                            SweepOutcome::Clear => "Clear",
                            SweepOutcome::Blocked => "Hit",
                            SweepOutcome::InitialOverlap => "Inside",
                            SweepOutcome::PrecisionLimited => "Limited",
                        },
                        QueryResult::Overlap { result } => {
                            if result.overlapping {
                                "Occupied"
                            } else {
                                "Empty"
                            }
                        }
                    });
                    details.push(description(observation, &frame.scene));
                }
            }
            (
                format!("Queries · {}", states.join(" · ")),
                details.join("\n"),
            )
        };
        ui.add_sized(
            egui::vec2(ui.available_width().max(0.0), ui.spacing().interact_size.y),
            egui::Label::new(&text).truncate(),
        )
        .on_hover_text(detail);
    }

    pub(super) fn spatial_feedback_menu(&mut self, ui: &mut egui::Ui) {
        let Some(frame) = &self.frame else {
            return;
        };
        let Some(queries) = &frame.query_observations else {
            return;
        };
        if queries.is_empty() {
            return;
        }
        let selected = &mut self.spatial_feedback_index;
        *selected = (*selected).min(queries.len() - 1);
        let retained = self.stale || !self.visible;
        egui::menu::SubMenuButton::new("Spatial feedback")
            .config(
                egui::menu::MenuConfig::default()
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
            )
            .ui(ui, |ui| {
                ui.set_max_width(
                    420.0_f32.min((ui.ctx().content_rect().width() - 32.0).max(180.0)),
                );
                ui.label(format!("Committed tick {}", frame.tick));
                if retained {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "Previous scene snapshot · Reset required",
                    );
                }
                ui.label("Queries inspect the scene. Your script chooses movement.");
                ui.horizontal(|ui| {
                    let previous =
                        ui.add_enabled(*selected > 0, egui::Button::new("Previous query"));
                    let next = ui.add_enabled(
                        *selected + 1 < queries.len(),
                        egui::Button::new("Next query"),
                    );
                    if previous.clicked() {
                        *selected -= 1;
                        if *selected == 0 {
                            next.request_focus();
                        } else {
                            previous.request_focus();
                        }
                    }
                    if next.clicked() {
                        *selected += 1;
                        if *selected + 1 == queries.len() {
                            previous.request_focus();
                        } else {
                            next.request_focus();
                        }
                    }
                });
                ui.label(format!("Query {} of {}", *selected + 1, queries.len()));
                let observation = &queries[*selected];
                ui.separator();
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(description(observation, &frame.scene)).strong(),
                    )
                    .wrap(),
                );
                ui.label(format!(
                    "Queried tick {} · scene revision {}",
                    observation.snapshot_tick, observation.scene_revision
                ));
                match &observation.query {
                    QueryResult::Sweep { result, .. } => {
                        if let Some(point) = result.point {
                            ui.label(format!(
                                "Contact [{:.4}, {:.4}, {:.4}]",
                                point[0], point[1], point[2]
                            ))
                            .on_hover_text(format!("{point:?}"));
                        }
                        if let Some(normal) = result.normal {
                            ui.label(format!(
                                "Normal [{:.4}, {:.4}, {:.4}]",
                                normal[0], normal[1], normal[2]
                            ))
                            .on_hover_text(format!("{normal:?}"));
                        }
                        ui.label(format!("Clearance {} scene units", result.clearance));
                        if matches!(result.outcome, SweepOutcome::InitialOverlap) {
                            ui.label("Move the probe out of the blocker, then Reset.");
                        }
                    }
                    QueryResult::Overlap { result } => {
                        ui.label(format!(
                            "Queried position [{:.4}, {:.4}, {:.4}]",
                            result.position[0], result.position[1], result.position[2]
                        ));
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruster_behavior::spatial::SweepResult;

    // Unique UI risk: retained query data must not look like current contact
    // after authoring invalidates the runtime. Labels use the captured names.
    #[test]
    fn query_feedback_names_geometry_and_marks_retained_snapshots() {
        let mut scene = ruster_scene::Scene::empty("feedback");
        scene.objects = vec![
            ruster_scene::Object::cube("probe", Some("Inspection probe".into())),
            ruster_scene::Object::cube("wall", Some("Thin barrier".into())),
        ];
        let observation = QueryObservation {
            binding_id: "motion".into(),
            snapshot_tick: 0,
            scene_revision: 4,
            query: QueryResult::Sweep {
                probe_id: "probe".into(),
                delta: [4.0, 0.0, 0.0],
                result: SweepResult {
                    outcome: SweepOutcome::Blocked,
                    suggested_translation: Some([-0.26, 0.0, 0.0]),
                    fraction: 0.435,
                    target_id: Some("wall".into()),
                    point: None,
                    normal: None,
                    clearance: 0.0001,
                },
            },
        };
        let mut runtime = RuntimeUi {
            visible: true,
            frame: Some(Frame {
                commit_revision: 0,
                last_step_commit_revision: None,
                commit_revision_required: false,
                command_schema_hash: None,
                authoring: std::sync::Arc::new(Default::default()),
                state: std::sync::Arc::new(ruster_behavior::RuntimeState::default()),
                key: RunKey::default(),
                tick: 1,
                scene: Arc::new(scene),
                input: None,
                query_observations: Some(Arc::from([observation])),
            }),
            ..Default::default()
        };
        let captured = runtime.frame.as_ref().unwrap();
        assert_eq!(
            description(
                &captured.query_observations.as_ref().unwrap()[0],
                &captured.scene
            ),
            "Inspection probe · sweep blocked by Thin barrier"
        );
        let ctx = egui::Context::default();
        for (width, stale) in [(900.0, false), (300.0, true)] {
            runtime.stale = stale;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 400.0),
                    )),
                    ..Default::default()
                },
                |ui| runtime.spatial_feedback_summary(ui),
            );
            let text: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            output.textures_delta.clear();
            assert!(
                text.iter().any(|text| text.contains(if stale {
                    "Previous query results"
                } else {
                    "Queries · Hit"
                })),
                "{text:?}"
            );
        }

        // The full32-result sequence is navigable through ordinary buttons and
        // keyboard activation, while every page fits the compact popup.
        runtime.stale = false;
        let observation = runtime
            .frame
            .as_ref()
            .unwrap()
            .query_observations
            .as_ref()
            .unwrap()[0]
            .clone();
        runtime.frame.as_mut().unwrap().query_observations = Some(Arc::from(vec![observation; 32]));
        fn draw(
            ctx: &egui::Context,
            runtime: &mut RuntimeUi,
            events: Vec<egui::Event>,
        ) -> Vec<egui::epaint::ClippedShape> {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640.0, 480.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    ui.menu_button("Runtime", |ui| runtime.spatial_feedback_menu(ui));
                },
            );
            output.textures_delta.clear();
            output.shapes
        }
        fn point(shapes: &[egui::epaint::ClippedShape], label: &str) -> egui::Pos2 {
            shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(egui::Rect::from_min_size(text.pos, text.galley.size()).center())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing label {label}"))
        }
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
        let key = |key, modifiers| {
            vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers,
                },
            ]
        };
        let shapes = draw(&ctx, &mut runtime, vec![]);
        let parent = point(&shapes, "Runtime");
        draw(&ctx, &mut runtime, click(parent));
        let shapes = draw(&ctx, &mut runtime, vec![]);
        let menu = point(&shapes, "Spatial feedback");
        draw(&ctx, &mut runtime, click(menu));
        assert!(
            egui::Popup::is_any_open(&ctx),
            "query menu opens through its button"
        );
        let shapes = draw(&ctx, &mut runtime, vec![]);
        let next = point(&shapes, "Next query");
        draw(&ctx, &mut runtime, click(next));
        assert_eq!(runtime.spatial_feedback_index, 1);
        draw(
            &ctx,
            &mut runtime,
            key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert_eq!(
            runtime.spatial_feedback_index, 2,
            "Enter activates the focused query button"
        );
        draw(
            &ctx,
            &mut runtime,
            key(egui::Key::Tab, egui::Modifiers::SHIFT),
        );
        draw(
            &ctx,
            &mut runtime,
            key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert_eq!(
            runtime.spatial_feedback_index, 1,
            "Shift-Tab reaches Previous query"
        );
        draw(
            &ctx,
            &mut runtime,
            key(egui::Key::Escape, egui::Modifiers::NONE),
        );
        assert!(!egui::Popup::is_any_open(&ctx));
        // Retire the closed popup's previous-pass hit rectangles before a new
        // pointer gesture. This is an ordinary paint pass, without input replay.
        draw(&ctx, &mut runtime, vec![]);
        draw(&ctx, &mut runtime, click(parent));
        let shapes = draw(&ctx, &mut runtime, vec![]);
        draw(
            &ctx,
            &mut runtime,
            click(point(&shapes, "Spatial feedback")),
        );
        draw(&ctx, &mut runtime, vec![]);
        assert!(egui::Popup::is_any_open(&ctx));
        draw(&ctx, &mut runtime, click(egui::pos2(620.0, 460.0)));
        assert!(
            !egui::Popup::is_any_open(&ctx),
            "outside click dismisses query menus"
        );
        let queries = runtime
            .frame
            .as_ref()
            .unwrap()
            .query_observations
            .as_ref()
            .unwrap();
        runtime.frame.as_mut().unwrap().query_observations = Some(Arc::from(queries[..2].to_vec()));
        runtime.spatial_feedback_index = 31;
        draw(&ctx, &mut runtime, vec![]);
        draw(&ctx, &mut runtime, click(parent));
        draw(&ctx, &mut runtime, vec![]);
        assert_eq!(
            runtime.spatial_feedback_index, 1,
            "reopening feedback clamps the index to the smaller snapshot"
        );
        let shapes = draw(&ctx, &mut runtime, vec![]);
        draw(
            &ctx,
            &mut runtime,
            click(point(&shapes, "Spatial feedback")),
        );
        let shapes = draw(&ctx, &mut runtime, vec![]);
        draw(&ctx, &mut runtime, click(point(&shapes, "Previous query")));
        assert_eq!(runtime.spatial_feedback_index, 0);
        // At either end the activated button becomes disabled. Keep focus on
        // the remaining navigation button, rather than dropping it to the app.
        draw(&ctx, &mut runtime, vec![]);
        draw(
            &ctx,
            &mut runtime,
            key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert_eq!(runtime.spatial_feedback_index, 1);
        draw(&ctx, &mut runtime, vec![]);
        draw(
            &ctx,
            &mut runtime,
            key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert_eq!(runtime.spatial_feedback_index, 0);
        runtime.begin_reset(RunKey::default());
        assert_eq!(runtime.spatial_feedback_index, 0);
    }
}
