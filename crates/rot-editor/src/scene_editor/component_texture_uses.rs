//! Exact authored texture references, indexed on request for one texture.
use super::ComponentWorkspace;
use crate::{editor_theme, strings::Text, ui_kit::tokens::ROW_HEIGHT};
use eframe::egui;
#[cfg(unix)]
use rot_assets::AssetKind;
use rot_scene::component_scene::{Component, ComponentScene};
#[cfg(unix)]
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[cfg(unix)]
const PAGE_SIZE: usize = 16;

#[derive(Clone, Copy)]
enum Slot {
    BaseColor,
    Roughness,
}

impl Slot {
    #[cfg(unix)]
    fn wire(self) -> &'static str {
        match self {
            Self::BaseColor => "base_color",
            Self::Roughness => "roughness",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::BaseColor => Text::TextureUsesBaseColor.plain(),
            Self::Roughness => Text::TextureUsesRoughness.plain(),
        }
    }
}

struct Use {
    material: usize,
    slot: Slot,
    renderer: Option<(usize, usize)>,
    enabled: bool,
    effectively_enabled: bool,
    prefab_owned: bool,
}

pub(super) struct TextureUses {
    revision: u64,
    asset_id: String,
    materials: Vec<String>,
    rows: Vec<Use>,
}

pub(super) struct Dialog {
    uses: TextureUses,
    selected: usize,
    initial_focus: bool,
}

impl TextureUses {
    fn build(
        scene: &ComponentScene,
        revision: u64,
        asset_id: &str,
        hierarchy_rows: &[(usize, usize)],
        parents: &[Option<usize>],
    ) -> Self {
        let mut result = Self {
            revision,
            asset_id: asset_id.into(),
            materials: Vec::new(),
            rows: Vec::new(),
        };
        let mut slots = HashMap::new();
        for (material_id, material) in &scene.materials {
            let base = material.base_color_texture.as_deref() == Some(asset_id);
            let roughness = material.roughness_texture.as_deref() == Some(asset_id);
            if base || roughness {
                slots.insert(
                    material_id.as_str(),
                    (result.materials.len(), base, roughness),
                );
                result.materials.push(material_id.clone());
            }
        }
        if slots.is_empty() {
            return result;
        }
        let prefab_objects: HashSet<_> = scene
            .prefabs
            .values()
            .flat_map(|instance| instance.parts.values().map(|part| part.object.as_str()))
            .collect();
        let prefab_materials: HashSet<_> = scene
            .prefabs
            .values()
            .flat_map(|instance| instance.materials.values().map(String::as_str))
            .collect();
        let mut owner_enabled = vec![false; scene.objects.len()];
        for &(index, _) in hierarchy_rows {
            owner_enabled[index] = scene.objects[index].enabled
                && parents[index].is_none_or(|parent| owner_enabled[parent]);
        }
        let mut assigned = vec![false; slots.len()];
        let mut push =
            |material, base, roughness, renderer, enabled, effectively_enabled, prefab_owned| {
                for (used, slot) in [(base, Slot::BaseColor), (roughness, Slot::Roughness)] {
                    if used {
                        result.rows.push(Use {
                            material,
                            slot,
                            renderer,
                            enabled,
                            effectively_enabled,
                            prefab_owned,
                        });
                    }
                }
            };
        for (owner, object) in scene.objects.iter().enumerate() {
            for (renderer, component) in object.components.iter().enumerate() {
                let Component::MeshRenderer {
                    material: Some(id), ..
                } = &component.kind
                else {
                    continue;
                };
                if let Some(&(material, base, roughness)) = slots.get(id.as_str()) {
                    assigned[material] = true;
                    push(
                        material,
                        base,
                        roughness,
                        Some((owner, renderer)),
                        component.enabled,
                        component.enabled && owner_enabled[owner],
                        prefab_objects.contains(object.id.as_str()),
                    );
                }
            }
        }
        for id in &result.materials {
            let (material, base, roughness) = slots[id.as_str()];
            if !assigned[material] {
                push(
                    material,
                    base,
                    roughness,
                    None,
                    false,
                    false,
                    prefab_materials.contains(id.as_str()),
                );
            }
        }
        result
    }
}

impl ComponentWorkspace {
    fn texture_uses(&mut self, asset_id: &str) -> &TextureUses {
        let revision = self.session.revision();
        if self
            .texture_uses
            .as_ref()
            .is_none_or(|uses| uses.revision != revision || uses.asset_id != asset_id)
        {
            self.texture_uses = Some(TextureUses::build(
                self.session.scene(),
                revision,
                asset_id,
                &self.rows,
                &self.parents,
            ));
        }
        self.texture_uses.as_ref().expect("texture uses")
    }

    #[cfg(unix)]
    pub(super) fn inspect_texture_uses(
        &mut self,
        token: &str,
        args: &Value,
    ) -> Result<Value, (&'static str, String)> {
        use super::mcp::decimal;

        let invalid = || {
            (
                "invalid_arguments",
                "Pass usage_asset_id and use all three returned guards for continuation.".into(),
            )
        };
        let map = args.as_object().ok_or_else(invalid)?;
        if map.keys().any(|key| {
            !matches!(
                key.as_str(),
                "usage_asset_id"
                    | "offset"
                    | "expected_session_token"
                    | "expected_revision"
                    | "expected_asset_revision"
            )
        }) {
            return Err(invalid());
        }
        let id = args["usage_asset_id"]
            .as_str()
            .filter(|id| rot_scene::validate_id(id).is_ok())
            .ok_or_else(invalid)?;
        let start = map
            .get("offset")
            .map_or(Some(0), decimal)
            .and_then(|offset| usize::try_from(offset).ok())
            .ok_or_else(invalid)?;
        let guard_count = [
            "expected_session_token",
            "expected_revision",
            "expected_asset_revision",
        ]
        .iter()
        .filter(|key| map.contains_key(**key))
        .count();
        if (start > 0 || guard_count > 0) && guard_count != 3 {
            return Err(invalid());
        }
        if guard_count == 3
            && args["expected_session_token"]
                .as_str()
                .ok_or_else(invalid)?
                != token
        {
            return Err(("stale_session", "Inspect the current scene again.".into()));
        }
        if guard_count == 3
            && decimal(&args["expected_revision"]).ok_or_else(invalid)? != self.session.revision()
        {
            return Err((
                "stale_revision",
                "Start again at the first usage page.".into(),
            ));
        }
        if self.assets_loading() {
            return Err((
                "assets_loading",
                "Wait for the project assets to load.".into(),
            ));
        }
        let asset_revision = self.material_asset_revision();
        if guard_count == 3
            && decimal(&args["expected_asset_revision"]).ok_or_else(invalid)? != asset_revision
        {
            return Err((
                "stale_asset_revision",
                "Start again at the first usage page.".into(),
            ));
        }
        let info = self
            .assets
            .as_ref()
            .and_then(|assets| assets.asset_info(id));
        let derived = info.is_none()
            && self
                .assets
                .as_ref()
                .is_some_and(|assets| assets.texture(id).is_some());
        let kind = info
            .map(|info| info.kind)
            .or(derived.then_some(AssetKind::Texture));
        let status = info
            .map(|info| match info.status {
                rot_assets::AssetStatus::Ready => "ready",
                rot_assets::AssetStatus::Invalid => "invalid",
                rot_assets::AssetStatus::Missing => "missing",
            })
            .unwrap_or(if derived { "ready" } else { "missing" });
        if kind.is_some_and(|kind| kind != AssetKind::Texture) {
            return Err(("not_texture", "Use the ID of a texture asset.".into()));
        }
        self.texture_uses(id);
        let uses = self.texture_uses.as_ref().expect("texture uses");
        if kind.is_none() && uses.rows.is_empty() {
            return Err((
                "asset_not_found",
                "No catalog texture or authored texture reference has this ID.".into(),
            ));
        }
        if start > uses.rows.len() {
            return Err(invalid());
        }
        let scene = self.session.scene();
        let end = (start + PAGE_SIZE).min(uses.rows.len());
        let rows = uses.rows[start..end].iter().map(|row| {
            let material = &uses.materials[row.material];
            let owner = row.renderer.map(|(index, renderer)| {
                let object = &scene.objects[index];
                (&object.id, &object.name, &object.components[renderer].id, object.enabled)
            });
            json!({
                "material_id": material, "material_name": scene.materials[material].name,
                "slot": row.slot.wire(), "object_id": owner.map(|owner| owner.0),
                "object_name": owner.map(|owner| owner.1), "renderer_id": owner.map(|owner| owner.2),
                "object_enabled": owner.map(|owner| owner.3),
                "renderer_enabled": owner.map(|_| row.enabled),
                "effectively_enabled": owner.map(|_| row.effectively_enabled),
                "prefab_owned": row.prefab_owned,
            })
        }).collect::<Vec<_>>();
        Ok(
            json!({"asset_id":id, "asset_status":status, "asset_revision":asset_revision.to_string(),
            "total":uses.rows.len(), "next_offset": (end < uses.rows.len()).then(|| end.to_string()),
            "uses":rows}),
        )
    }

    pub(super) fn begin_texture_uses(&mut self, asset_id: String) {
        if !self.authored_edit_pending() {
            self.texture_uses(&asset_id);
            self.texture_uses_dialog = Some(Dialog {
                uses: self.texture_uses.take().expect("texture uses"),
                selected: 0,
                initial_focus: true,
            });
        }
    }

    pub(super) fn show_texture_uses(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.texture_uses_dialog.take() else {
            return;
        };
        if dialog.uses.revision != self.session.revision() {
            self.error = Some(Text::TextureUsesChanged.plain().into());
            return;
        }
        let uses = &dialog.uses;
        let scene = self.session.scene();
        dialog.selected = dialog.selected.min(uses.rows.len().saturating_sub(1));
        let mut reveal = false;
        let mut close = false;
        let enter = !dialog.initial_focus && ctx.input(|input| input.key_pressed(egui::Key::Enter));
        let response = egui::Modal::new(egui::Id::unique("component_texture_uses"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 64.).clamp(160., 560.));
                ui.heading(Text::TextureUses.plain());
                ui.add(egui::Label::new(&uses.asset_id).selectable(true).truncate())
                    .on_hover_text(&uses.asset_id);
                if uses.rows.is_empty() {
                    ui.label(Text::TextureUsesEmpty.plain());
                } else {
                    ui.label(format!(
                        "{} {}",
                        uses.rows.len(),
                        Text::TextureUsesMatches.plain()
                    ));
                    let list_id = ui.scope_id().with("texture_use_rows");
                    let focus = ui.interact(
                        ui.available_rect_before_wrap(),
                        list_id,
                        egui::Sense::focusable_noninteractive(),
                    );
                    focus.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::GenericContainer,
                            true,
                            Text::TextureUsesMatches.plain(),
                        )
                    });
                    if dialog.initial_focus {
                        focus.request_focus();
                    }
                    let mut scroll = egui::ScrollArea::vertical()
                        .max_height((ctx.content_rect().height() - 170.).clamp(45., 300.));
                    if focus.has_focus() && !dialog.initial_focus {
                        let before = dialog.selected;
                        ctx.input_mut(|input| {
                            if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                                dialog.selected = (dialog.selected + 1).min(uses.rows.len() - 1);
                            }
                            if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                                dialog.selected = dialog.selected.saturating_sub(1);
                            }
                            reveal = input.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                        });
                        if before != dialog.selected {
                            scroll = scroll.vertical_scroll_offset(
                                dialog.selected as f32 * (ROW_HEIGHT + ui.spacing().item_spacing.y),
                            );
                        }
                    }
                    scroll.show_rows(ui, ROW_HEIGHT, uses.rows.len(), |ui, range| {
                        for index in range {
                            let row = &uses.rows[index];
                            let material = &uses.materials[row.material];
                            let value = &scene.materials[material];
                            let owner = row.renderer.map(|(object, renderer)| {
                                let object = &scene.objects[object];
                                (&object.name, &object.components[renderer].id)
                            });
                            let (name, id) = owner
                                .map_or((Text::TextureUsesUnassigned.plain(), ""), |(name, id)| {
                                    (name.as_str(), id.as_str())
                                });
                            let mut label = format!(
                                "{name}  ·  {}  ·  {}",
                                value.name.as_deref().unwrap_or(material),
                                row.slot.label()
                            );
                            if owner.is_some() && !row.effectively_enabled {
                                label.push_str("  ·  Disabled");
                            }
                            if row.prefab_owned {
                                label.push_str("  ·  Prefab");
                            }
                            let response = ui
                                .add(
                                    egui::Button::selectable(index == dialog.selected, &label)
                                        .truncate(),
                                )
                                .on_hover_text(format!("{label}\n{material}\n{id}"));
                            if response.clicked() {
                                dialog.selected = index;
                                focus.request_focus();
                            }
                            if response.double_clicked() || (response.has_focus() && enter) {
                                dialog.selected = index;
                                reveal = true;
                            }
                        }
                    });
                }
                if let Some(row) = uses.rows.get(dialog.selected) {
                    let material = &uses.materials[row.material];
                    let (object, renderer) = row.renderer.map_or(
                        (Text::TextureUsesUnassigned.plain(), ""),
                        |(index, renderer)| {
                            let object = &scene.objects[index];
                            (
                                object.name.as_str(),
                                object.components[renderer].id.as_str(),
                            )
                        },
                    );
                    let label = format!(
                        "{object} · {} · {}",
                        scene.materials[material]
                            .name
                            .as_deref()
                            .unwrap_or(material),
                        row.slot.label()
                    );
                    ui.add(egui::Label::new(&label).truncate())
                        .on_hover_text(&label);
                    if !renderer.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label(Text::TextureUsesRenderer.plain());
                            ui.add(egui::Label::new(renderer).selectable(true).truncate())
                                .on_hover_text(renderer);
                        });
                    }
                }
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    reveal |= ui
                        .add_enabled(
                            uses.rows
                                .get(dialog.selected)
                                .is_some_and(|row| row.renderer.is_some()),
                            egui::Button::new(Text::TextureUsesReveal.plain()),
                        )
                        .clicked();
                    let button = ui.button(Text::TextureUsesClose.plain());
                    if dialog.initial_focus && uses.rows.is_empty() {
                        button.request_focus();
                    }
                    close |= button.clicked();
                });
            });
        dialog.initial_focus = false;
        if reveal {
            if let Some((index, renderer)) =
                uses.rows.get(dialog.selected).and_then(|row| row.renderer)
            {
                if !self.authored_edit_pending() {
                    let object = &self.session.scene().objects[index];
                    let id = object.id.clone();
                    self.texture_use_reveal = Some((index, object.components[renderer].id.clone()));
                    self.select(Some(id));
                    self.edit_scene_during_play = true;
                    ctx.request_repaint();
                    return;
                }
            }
        }
        if !close && !response.should_close() {
            self.texture_uses_dialog = Some(dialog);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use rot_assets::{AssetStore, TextureRole};
    use rot_scene::component_scene::{ComponentSession, PrefabSource};
    use rot_scene::{OpaqueMaterial, Scene, Session};

    #[test]
    fn exact_texture_pages_include_disabled_renderers_prefabs_and_unassigned_materials() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        std::fs::create_dir(root.join("Assets")).unwrap();
        let png = |name: &str| {
            let file = std::fs::File::create(root.join("Assets").join(name)).unwrap();
            let mut encoder = png::Encoder::new(file, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[80, 100, 110])
                .unwrap();
        };
        png("Panel.png");
        png("Unused.png");
        let (mut store, _) = AssetStore::open_project(root).unwrap();
        store
            .interpret_project_texture("Assets/Panel.png", TextureRole::Roughness, || true)
            .unwrap();
        let role_id = |source: &str, role: TextureRole| {
            store
                .infos()
                .find(|info| info.source == source && info.texture_role == Some(role))
                .unwrap()
                .id
                .clone()
        };
        let color = role_id("Assets/Panel.png", TextureRole::BaseColor);
        let roughness = role_id("Assets/Panel.png", TextureRole::Roughness);
        let unused = role_id("Assets/Unused.png", TextureRole::BaseColor);
        assert_ne!(color, roughness);

        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.materials.insert(
            "panel".into(),
            OpaqueMaterial {
                name: Some("Brushed panel".into()),
                base_color_texture: Some(color.clone()),
                roughness_texture: Some(roughness.clone()),
                ..Default::default()
            },
        );
        scene.materials.insert(
            "orphan".into(),
            OpaqueMaterial {
                base_color_texture: Some(color.clone()),
                roughness_texture: Some("missing-texture".into()),
                ..Default::default()
            },
        );
        let owner = scene.objects[0].id.clone();
        let mut renderer = scene.objects[0].components[0].clone();
        let Component::MeshRenderer { material, .. } = &mut renderer.kind else {
            panic!("demo cube renderer")
        };
        *material = Some("panel".into());
        scene.objects[0].enabled = false;
        scene.objects[0].components = (0..17)
            .map(|n| {
                let mut renderer = renderer.clone();
                renderer.id = format!("panel-renderer-{n}");
                renderer.enabled = n != 1;
                renderer
            })
            .collect();
        let path = root.join("scene.json");
        scene.save_new(&path).unwrap();
        let mut session = ComponentSession::open(&path).unwrap();
        let prefab = PrefabSource::from_selection(session.scene(), &owner).unwrap();
        session
            .link_prefab(
                0,
                "panel-prefab".into(),
                "Assets/Panel.prefab".into(),
                &prefab,
                |_| Ok(()),
            )
            .unwrap();
        let mut editor = ComponentWorkspace::new(path, session);
        editor.assets = Some(store);

        let inspect = |editor: &mut ComponentWorkspace, args: Value| {
            editor.ai_command("scene-session", "asset_inspect", &args)
        };
        let first = inspect(&mut editor, json!({"usage_asset_id":color}));
        assert_eq!(first["ok"], true, "{first}");
        let result = &first["result"];
        assert_eq!(result["total"], 19);
        assert_eq!(result["next_offset"], "16");
        assert_eq!(result["uses"][0]["renderer_id"], "panel-renderer-0");
        assert_eq!(result["uses"][0]["object_id"], owner);
        assert_eq!(result["uses"][0]["slot"], "base_color");
        assert_eq!(result["uses"][0]["effectively_enabled"], false);
        assert_eq!(result["uses"][1]["renderer_enabled"], false);
        assert_eq!(result["uses"][1]["prefab_owned"], true);

        let mut next = json!({"usage_asset_id":color,"offset":"16",
            "expected_session_token":result["session_token"],"expected_revision":result["revision"],
            "expected_asset_revision":result["asset_revision"]});
        let second = inspect(&mut editor, next.clone());
        assert_eq!(second["ok"], true, "{second}");
        assert_eq!(second["result"]["uses"][1]["material_id"], "orphan");
        assert_eq!(second["result"]["uses"][1]["renderer_id"], Value::Null);
        assert_eq!(second["result"]["next_offset"], Value::Null);
        for (key, value, code) in [
            ("expected_session_token", "wrong", "stale_session"),
            ("expected_revision", "2", "stale_revision"),
            ("expected_asset_revision", "99", "stale_asset_revision"),
        ] {
            let mut stale = next.clone();
            stale[key] = json!(value);
            assert_eq!(inspect(&mut editor, stale)["error"]["code"], code);
        }
        next.as_object_mut()
            .unwrap()
            .remove("expected_asset_revision");
        assert_eq!(
            inspect(&mut editor, next)["error"]["code"],
            "invalid_arguments"
        );
        let alias = inspect(&mut editor, json!({"usage_asset_id":roughness}));
        assert_eq!(alias["result"]["total"], 18);
        assert_eq!(alias["result"]["uses"][0]["slot"], "roughness");
        let missing = inspect(&mut editor, json!({"usage_asset_id":"missing-texture"}));
        assert_eq!(missing["result"]["total"], 1);
        assert_eq!(missing["result"]["asset_status"], "missing");
        let zero = inspect(&mut editor, json!({"usage_asset_id":unused}));
        assert_eq!(zero["result"]["total"], 0);
        assert_eq!(editor.session.revision(), 1);
    }
}
