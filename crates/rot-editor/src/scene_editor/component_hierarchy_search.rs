//! Name and stable-ID lookup in v23 Hierarchy order.
use super::ComponentWorkspace;
use crate::{
    strings::Text,
    ui_kit::{
        self,
        tokens::{CONTROL_HEIGHT, MIN_HIT, ROW_HEIGHT, SPACE_XXS},
    },
};
use eframe::egui;
use rot_scene::component_scene::ObjectNode;

const MAX_PARENTS: usize = 4;

#[derive(Default)]
pub(super) struct SearchIndex {
    revision: Option<u64>,
    entries: Vec<(usize, String, String)>,
}

impl SearchIndex {
    fn update(&mut self, revision: u64, rows: &[(usize, usize)], objects: &[ObjectNode]) {
        if self.revision == Some(revision) {
            return;
        }
        self.entries = rows
            .iter()
            .map(|&(index, _)| {
                let object = &objects[index];
                (
                    index,
                    object.name.to_lowercase(),
                    object.id.to_ascii_lowercase(),
                )
            })
            .collect();
        self.revision = Some(revision);
    }

    fn matches(&self, query: &str) -> impl Iterator<Item = usize> + '_ {
        let query = query.trim().to_lowercase();
        self.entries.iter().filter_map(move |(index, name, id)| {
            (name.contains(&query) || id.contains(&query)).then_some(*index)
        })
    }
}

fn parents_for<'a>(
    index: usize,
    objects: &'a [ObjectNode],
    parents: &[Option<usize>],
) -> (Vec<&'a str>, bool) {
    let mut path = Vec::new();
    let mut parent = parents[index];
    while let Some(index) = parent {
        if path.len() == MAX_PARENTS {
            break;
        }
        path.push(objects[index].name.as_str());
        parent = parents[index];
    }
    path.reverse();
    (path, parent.is_some())
}

#[derive(Default)]
pub(super) struct Search {
    query: String,
    revision: Option<u64>,
    results: Vec<usize>,
    active: usize,
    focused_control: Option<egui::Id>,
    pub(super) focus_requested: bool,
}

impl Search {
    pub(super) fn is_active(&self) -> bool {
        !self.query.trim().is_empty()
    }

    fn clear(&mut self) {
        self.query.clear();
        self.results.clear();
        self.active = 0;
        self.focused_control = None;
    }
}

impl ComponentWorkspace {
    pub(super) fn select_and_reveal_hierarchy(&mut self, id: String, ctx: &egui::Context) {
        self.hierarchy_search.clear();
        self.hierarchy_search.focus_requested = false;
        self.hierarchy_search_reveal = Some(id.clone());
        self.select(Some(id));
        ctx.request_repaint();
    }

    /// A nonempty search replaces the visible tree without changing its state.
    pub(super) fn show_hierarchy_search(
        &mut self,
        ui: &mut egui::Ui,
        blocked: bool,
        popup_was_open: bool,
    ) -> bool {
        let input_id = ui.make_persistent_id("hierarchy_search_input");
        let previously_rendered_focus = self.hierarchy_search.focused_control.take();
        let (focused_before, owns_escape) = ui.memory(|memory| {
            let focused = memory.focused();
            let owns_escape = match focused {
                Some(id) => id == input_id || previously_rendered_focus == Some(id),
                None => {
                    memory.had_focus_last_frame(input_id)
                        || previously_rendered_focus
                            .is_some_and(|id| memory.had_focus_last_frame(id))
                }
            };
            (focused == Some(input_id), owns_escape)
        });
        let escape = !blocked
            && self.hierarchy_search.is_active()
            && owns_escape
            && !popup_was_open
            && !egui::Popup::is_any_open(ui.ctx())
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let (up, down, enter) = if !blocked && focused_before && self.hierarchy_search.is_active() {
            ui.input_mut(|input| {
                (
                    input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                )
            })
        } else {
            (false, false, false)
        };
        if escape {
            self.hierarchy_search.clear();
            ui.memory_mut(|memory| {
                memory.surrender_focus(input_id);
                if let Some(id) = previously_rendered_focus {
                    memory.surrender_focus(id);
                }
            });
        }
        let edit = ui
            .add_enabled_ui(!blocked, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = SPACE_XXS;
                    let (_, icon) = ui.allocate_space(egui::vec2(14.0, CONTROL_HEIGHT));
                    let center = icon.center() - egui::vec2(1.0, 1.0);
                    let stroke = egui::Stroke::new(1.25, ui.visuals().weak_text_color());
                    ui.painter().circle_stroke(center, 3.5, stroke);
                    ui.painter().line_segment(
                        [center + egui::vec2(2.5, 2.5), center + egui::vec2(5.5, 5.5)],
                        stroke,
                    );
                    let clear_width = if self.hierarchy_search.query.is_empty() {
                        0.0
                    } else {
                        MIN_HIT + SPACE_XXS
                    };
                    let edit = ui.add_sized(
                        [
                            (ui.available_width() - clear_width).max(16.0),
                            CONTROL_HEIGHT,
                        ],
                        egui::TextEdit::singleline(&mut self.hierarchy_search.query)
                            .id(input_id)
                            .hint_text(Text::LabelFindObjects.plain())
                            .char_limit(128),
                    );
                    edit.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::TextInput,
                            !blocked,
                            Text::LabelFindObjects.plain(),
                        )
                    });
                    if !self.hierarchy_search.query.is_empty() {
                        let clear =
                            ui_kit::icon_button(ui, "×", Text::ClearSearch.plain(), None, false);
                        if clear.clicked() {
                            self.hierarchy_search.clear();
                            edit.request_focus();
                        } else if !blocked && ui.memory(|memory| memory.has_focus(clear.id)) {
                            self.hierarchy_search.focused_control = Some(clear.id);
                        }
                    }
                    if !blocked && std::mem::take(&mut self.hierarchy_search.focus_requested) {
                        edit.request_focus();
                    }
                    edit
                })
                .inner
            })
            .inner;
        if !self.hierarchy_search.is_active() {
            return false;
        }

        let revision = self.session.revision();
        let objects = &self.session.scene().objects;
        let search = &mut self.hierarchy_search;
        let changed = edit.changed() || search.revision != Some(revision);
        if changed {
            self.hierarchy_search_index
                .update(revision, &self.rows, objects);
            search.results = self.hierarchy_search_index.matches(&search.query).collect();
            search.revision = Some(revision);
            search.active = 0;
        }

        if search.results.is_empty() {
            ui.weak(Text::HierarchySearchEmpty.plain());
            return true;
        }

        let list = egui::ScrollArea::vertical().id_salt("hierarchy_search_results");
        let result_height = ROW_HEIGHT * 2.0;
        let extent = result_height + ui.spacing().item_spacing.y;
        let previous = search.active;
        if up {
            search.active = search.active.saturating_sub(1);
        } else if down {
            search.active = (search.active + 1).min(search.results.len() - 1);
        }
        let mut list = list;
        if changed || previous != search.active {
            let visible_rows = ((ui.available_height() / extent).floor() as usize).max(1);
            // Keep an advancing selection in view without pinning it to the top.
            let offset = if search.active < previous {
                search.active
            } else {
                search.active.saturating_sub(visible_rows - 1)
            };
            list = list.vertical_scroll_offset(offset as f32 * extent);
        }
        let mut reveal = enter.then_some(search.results[search.active]);
        let window_focused = ui.input(|input| input.focused);
        list.show_rows(ui, result_height, search.results.len(), |ui, range| {
            for row in range {
                let index = search.results[row];
                let object = &objects[index];
                let (parents, truncated) = parents_for(index, objects, &self.parents);
                let breadcrumb = if parents.is_empty() {
                    Text::ComponentSceneRoot.plain().to_owned()
                } else {
                    let prefix = if truncated { "… / " } else { "" };
                    format!("{prefix}{}", parents.join(" / "))
                };
                let (_, rect) = ui.allocate_space(egui::vec2(ui.available_width(), result_height));
                let response = ui
                    .add_enabled_ui(!blocked, |ui| {
                        ui.interact(
                            rect,
                            ui.make_persistent_id(("hierarchy_search_result", &object.id)),
                            egui::Sense::click(),
                        )
                    })
                    .inner;
                let focused = ui.memory(|memory| memory.has_focus(response.id));
                if !blocked && focused {
                    search.focused_control = Some(response.id);
                }
                let show_focus = window_focused && focused;
                let selected = row == search.active;
                let label = format!("{} · {}", object.name, object.id);
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::Role::Button,
                        !blocked,
                        selected,
                        format!("{label} · {breadcrumb}"),
                    )
                });
                let visuals = ui.visuals();
                let widget = ui.style().interact(&response);
                if selected || response.hovered() || show_focus {
                    let fill = if selected {
                        visuals.selection.bg_fill
                    } else {
                        widget.weak_bg_fill
                    };
                    let stroke = if show_focus {
                        widget.fg_stroke
                    } else {
                        egui::Stroke::NONE
                    };
                    ui.painter().rect(
                        rect,
                        widget.corner_radius,
                        fill,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
                let x = rect.left() + SPACE_XXS;
                let width = (rect.width() - 2.0 * SPACE_XXS).max(0.0);
                for (text, y, style, color) in [
                    (
                        label,
                        rect.top(),
                        egui::TextStyle::Body,
                        if selected {
                            visuals.selection.stroke.color
                        } else {
                            widget.text_color()
                        },
                    ),
                    (
                        breadcrumb.clone(),
                        rect.top() + ROW_HEIGHT,
                        egui::TextStyle::Small,
                        if selected {
                            visuals.selection.stroke.color
                        } else {
                            visuals.weak_text_color()
                        },
                    ),
                ] {
                    let galley = egui::WidgetText::from(text).into_galley(
                        ui,
                        Some(egui::TextWrapMode::Truncate),
                        width,
                        style,
                    );
                    let y = y + (ROW_HEIGHT - galley.size().y) * 0.5;
                    ui.painter().galley(egui::pos2(x, y), galley, color);
                }
                let response =
                    response.on_hover_text(format!("{}\n{breadcrumb}\n{}", object.name, object.id));
                if response.clicked() {
                    reveal = Some(index);
                }
            }
        });
        if let Some(index) = reveal {
            let id = objects[index].id.clone();
            if self.apply() {
                self.select_and_reveal_hierarchy(id, ui.ctx());
                ui.memory_mut(|memory| memory.surrender_focus(input_id));
            }
        }
        true
    }

    #[cfg(unix)]
    pub(super) fn inspect_hierarchy_search(
        &mut self,
        token: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, (&'static str, String)> {
        use super::mcp::decimal;
        use serde_json::json;
        const PAGE: usize = 16;

        let invalid = || {
            (
                "invalid_arguments",
                "Pass search and pin both scene guards for continuation.".into(),
            )
        };
        let map = args.as_object().ok_or_else(invalid)?;
        if map.keys().any(|key| {
            !matches!(
                key.as_str(),
                "search" | "offset" | "expected_session_token" | "expected_revision"
            )
        }) {
            return Err(invalid());
        }
        let query = args["search"]
            .as_str()
            .map(str::trim)
            .filter(|query| {
                !query.is_empty() && query.len() <= 128 && !query.chars().any(char::is_control)
            })
            .ok_or_else(invalid)?;
        let offset = map
            .get("offset")
            .map_or(Some(0), decimal)
            .and_then(|offset| usize::try_from(offset).ok())
            .ok_or_else(invalid)?;
        let guard = [
            map.get("expected_session_token"),
            map.get("expected_revision"),
        ];
        let revision = self.session.revision();
        match guard {
            [Some(expected), Some(pinned)] => {
                if expected.as_str().ok_or_else(invalid)? != token {
                    return Err(("stale_session", "Inspect the open scene again.".into()));
                }
                if decimal(pinned).ok_or_else(invalid)? != revision {
                    return Err((
                        "stale_revision",
                        "Start the Hierarchy search again from its first page.".into(),
                    ));
                }
            }
            [None, None] if offset == 0 => {}
            _ => return Err(invalid()),
        }
        let scene = self.session.scene();
        let objects = &scene.objects;
        self.hierarchy_search_index
            .update(revision, &self.rows, objects);
        let mut rows = Vec::with_capacity(PAGE);
        let mut total = 0;
        for index in self.hierarchy_search_index.matches(query) {
            if total >= offset && rows.len() < PAGE {
                let object = &objects[index];
                let (path, truncated) = parents_for(index, objects, &self.parents);
                rows.push(json!({
                    "id":object.id, "name":object.name, "parent_id":object.parent,
                    "parent_path":path, "parent_path_truncated":truncated, "enabled":object.enabled,
                    "transform":object.transform, "component_count":object.components.len(),
                }));
            }
            total += 1;
        }
        if offset > total {
            return Err(invalid());
        }
        let end = offset + rows.len();
        Ok(json!({
            "scene_id":scene.id, "search":query, "objects":rows, "object_count":objects.len(),
            "match_count":total, "next_offset":(end < total).then(|| end.to_string()),
            "stored_coordinate_space":"local","loaded_format_version":23
        }))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::workspace::Action;
    use rot_scene::component_scene::{ComponentScene, ComponentSession};
    use rot_scene::{Scene, Session};
    use serde_json::json;

    #[test]
    fn hierarchy_search_escape_respects_an_inspector_draft_and_popup() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("scene.json");
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.objects[0].name = "Inspection Target".into();
        let mut editor =
            ComponentWorkspace::new(path, ComponentSession::new(scene.clone()).unwrap());
        editor.select(Some(scene.objects[0].id.clone()));

        let ctx = egui::Context::default();
        let draw = |editor: &mut ComponentWorkspace, events, popup| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280., 800.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    if popup {
                        egui::Popup::open_id(ui.ctx(), egui::Id::unique("escape-owner-fixture"));
                    }
                    editor.show_workspace(ui, false);
                    if popup {
                        assert!(ui.input(|input| input.key_pressed(egui::Key::Escape)));
                        egui::Popup::close_all(ui.ctx());
                    }
                },
            );
            output.textures_delta.clear();
            output.shapes
        };
        let key = |key| {
            [true, false]
                .into_iter()
                .map(|pressed| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .collect()
        };
        editor.hierarchy_search.focus_requested = true;
        draw(&mut editor, vec![], false);
        let shapes = draw(
            &mut editor,
            vec![egui::Event::Text("Inspection".into())],
            false,
        );
        assert_eq!(editor.hierarchy_search.query, "Inspection");
        let name = shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text)
                    if text.pos.x > 800. && text.galley.text() == "Inspection Target" =>
                {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("visible Inspector name field");
        draw(&mut editor, vec![egui::Event::PointerMoved(name)], false);
        for pressed in [true, false] {
            draw(
                &mut editor,
                vec![egui::Event::PointerButton {
                    pos: name,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
                false,
            );
        }
        draw(&mut editor, vec![egui::Event::Text(" draft".into())], false);
        assert_ne!(editor.draft.as_ref().unwrap().name, scene.objects[0].name);
        draw(&mut editor, key(egui::Key::Escape), true);
        assert!(editor.draft_dirty());
        assert_eq!(editor.hierarchy_search.query, "Inspection");

        draw(&mut editor, key(egui::Key::Escape), false);
        assert_eq!(editor.hierarchy_search.query, "Inspection");
        assert_eq!(editor.draft.as_ref().unwrap().name, scene.objects[0].name);
        assert!(!editor.draft_dirty());
        assert_eq!(editor.session.revision(), 0);

        editor.hierarchy_search.focus_requested = true;
        draw(&mut editor, vec![], false);
        draw(&mut editor, key(egui::Key::Escape), false);
        assert!(!editor.hierarchy_search.is_active());
        assert_eq!(editor.session.scene(), &scene);
    }

    #[test]
    fn hierarchy_search_keeps_the_draft_and_paginates_exact_hierarchy_order() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("scene.json");
        let mut scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("find")).unwrap()).unwrap();
        let mut add = |id: &str, name: &str, parent: Option<&str>, enabled| {
            scene.objects.push(ObjectNode {
                id: id.into(),
                name: name.into(),
                parent: parent.map(str::to_owned),
                enabled,
                transform: Default::default(),
                components: Vec::new(),
            });
        };
        add("wheel-z", "Étage Wheel", Some("z"), false);
        add("z", "Pier West", None, true);
        add("a", "Pier East", None, true);
        for index in 0..20 {
            add(
                &format!("wheel-a{index:02}"),
                "Étage Wheel",
                Some("a"),
                true,
            );
        }
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.select(Some("z".into()));
        editor.draft.as_mut().unwrap().name.clear();
        let ctx = egui::Context::default();
        let frame = |editor: &mut ComponentWorkspace, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    if let Some(action) = editor.workspace.component_shortcuts(ui.ctx(), false) {
                        editor.command(action);
                    }
                    let popup = egui::Popup::is_any_open(ui.ctx());
                    editor.show_hierarchy_search(ui, false, popup);
                },
            );
            output.textures_delta.clear();
        };
        let press = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let shortcut = || egui::Event::Key {
            key: egui::Key::F,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                command: true,
                ..egui::Modifiers::NONE
            },
        };
        frame(&mut editor, vec![shortcut()]);
        frame(&mut editor, vec![egui::Event::Text("étaGE".into())]);
        assert_eq!(editor.hierarchy_search.results.len(), 21);
        assert_eq!(editor.selection_anchor.as_deref(), Some("z"));
        assert!(editor.draft_dirty());

        let request = |editor: &mut ComponentWorkspace, args| {
            editor.ai_command("session", "scene_inspect", &args)
        };
        let result = request(&mut editor, json!({"search":"ÉTAGE"}));
        assert_eq!(result["result"]["match_count"], 21, "{result}");
        assert_eq!(result["result"]["objects"][0]["id"], "wheel-z");
        assert_eq!(result["result"]["objects"][0]["enabled"], false);
        assert_eq!(
            result["result"]["objects"][0]["parent_path"],
            json!(["Pier West"])
        );
        assert_eq!(
            result["result"]["objects"][1]["parent_path"],
            json!(["Pier East"])
        );
        let mut next = json!({"search":"wheel","offset":"16","expected_session_token":"session","expected_revision":"0"});
        let result = request(&mut editor, next.clone());
        assert_eq!(result["result"]["objects"][0]["id"], "wheel-a15");
        assert_eq!(result["result"]["next_offset"], serde_json::Value::Null);
        next["expected_revision"] = json!("1");
        assert_eq!(
            request(&mut editor, next)["error"]["code"],
            "stale_revision"
        );
        assert_eq!(editor.hierarchy_search.query, "étaGE");
        frame(&mut editor, vec![press(egui::Key::ArrowDown)]);
        assert_eq!(editor.hierarchy_search.active, 1);
        frame(&mut editor, vec![press(egui::Key::Enter)]);
        assert_eq!(editor.session.revision(), 0);
        assert!(editor.hierarchy_search.is_active());
        assert_eq!(editor.selection_anchor.as_deref(), Some("z"));
        assert!(editor.draft_dirty());
        frame(&mut editor, vec![press(egui::Key::Escape)]);
        assert!(!editor.hierarchy_search.is_active());
        assert!(editor.draft_dirty());
        assert_eq!(editor.session.scene(), &scene);

        editor.command(Action::Undo);
        frame(&mut editor, vec![shortcut()]);
        frame(&mut editor, vec![egui::Event::Text("WHEEL-Z".into())]);
        frame(&mut editor, vec![press(egui::Key::Enter)]);
        assert_eq!(editor.selection_anchor.as_deref(), Some("wheel-z"));
        assert_eq!(editor.hierarchy_search_reveal.as_deref(), Some("wheel-z"));
        assert_eq!(editor.session.revision(), 0);
        assert!(!editor.hierarchy_search.is_active());
    }
}
