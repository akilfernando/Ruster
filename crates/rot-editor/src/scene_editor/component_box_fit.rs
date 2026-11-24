//! Fit one authored Box Collider using the same indexed geometry the renderer draws.
use super::{
    component_inspector::{geometry_name, mesh_renderer_heading},
    ComponentWorkspace,
};
use crate::{editor_theme, strings::Text};
use eframe::egui;
use rot_assets::Mesh;
use rot_scene::{
    component_scene::{Component, Geometry},
    Collider, ColliderShape,
};
use serde_json::{json, Value};

type FitError = (&'static str, String);

pub(super) struct Picker {
    revision: u64,
    asset_revision: u64,
    owner: String,
    collider: String,
    choices: Vec<(String, String)>,
    first_frame: bool,
}

impl ComponentWorkspace {
    pub(super) fn begin_box_fit(&mut self, collider: String) {
        if self.authored_edit_pending()
            || self.runtime.in_play_mode()
            || self.ai_transition_pending()
        {
            return;
        }
        let Some(row) = self.selected_index else {
            return;
        };
        let owner = &self.session.scene().objects[row];
        let choices = owner
            .components
            .iter()
            .filter_map(|component| match &component.kind {
                Component::MeshRenderer { geometry, .. } => Some((&component.id, geometry)),
                _ => None,
            })
            .enumerate()
            .map(|(index, (id, geometry))| {
                let heading = mesh_renderer_heading(Some(index + 1));
                let name = geometry_name(geometry, self.assets.as_ref());
                (id.clone(), format!("{heading} · {name} · {id}"))
            })
            .collect::<Vec<_>>();
        let picker = Picker {
            revision: self.session.revision(),
            asset_revision: self.material_asset_revision(),
            owner: owner.id.clone(),
            collider,
            choices,
            first_frame: true,
        };
        if picker.choices.len() <= 1 {
            self.submit_box_fit(picker, None);
        } else {
            self.box_fit = Some(picker);
        }
    }

    pub(super) fn show_box_fit(&mut self, ctx: &egui::Context) {
        let Some(mut picker) = self.box_fit.take() else {
            return;
        };
        if self.session.revision() != picker.revision
            || self.material_asset_revision() != picker.asset_revision
            || self
                .draft
                .as_ref()
                .is_none_or(|draft| draft.id != picker.owner)
            || self.ai_transition_pending()
        {
            self.error = Some(Text::BoxFitRetry.plain().into());
            return;
        }
        let response = egui::Modal::new(egui::Id::unique("box_collider_mesh_fit"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(ui.available_width().min(420.0));
                ui.heading(Text::BoxFitChooseRenderer.plain());
                let mut choice = None;
                let mut scroll = egui::ScrollArea::vertical().max_height(230.0);
                if picker.first_frame {
                    scroll = scroll.vertical_scroll_offset(0.0);
                }
                scroll.show(ui, |ui| {
                    for (index, (_, label)) in picker.choices.iter().enumerate() {
                        let response = ui
                            .add(egui::Button::new(label).truncate())
                            .on_hover_text(label);
                        if picker.first_frame && index == 0 {
                            response.request_focus();
                        }
                        if !picker.first_frame && response.clicked() {
                            choice = Some(index);
                        }
                    }
                });
                let cancel = ui.button(Text::ComponentCancel.plain()).clicked();
                (choice, cancel)
            });
        if response.inner.1
            || response.is_top_modal
                && ctx
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            return;
        }
        if let Some(choice) = response.inner.0 {
            let renderer = picker.choices[choice].0.clone();
            self.submit_box_fit(picker, Some(&renderer));
        } else {
            picker.first_frame = false;
            self.box_fit = Some(picker);
        }
    }

    fn submit_box_fit(&mut self, picker: Picker, renderer: Option<&str>) {
        match self.fit_box_to_mesh(
            picker.revision,
            Some(picker.asset_revision),
            &picker.owner,
            &picker.collider,
            renderer,
        ) {
            Ok(fit) => {
                self.error = None;
                self.box_fit_feedback = (!fit.minimum_axes.is_empty())
                    .then_some((self.session.revision(), picker.collider));
            }
            Err((_, message)) => self.error = Some(message),
        }
    }

    pub(super) fn fit_box_to_mesh(
        &mut self,
        revision: u64,
        asset_revision: Option<u64>,
        owner_id: &str,
        collider_id: &str,
        renderer_id: Option<&str>,
    ) -> Result<Fit, FitError> {
        if revision != self.session.revision() {
            return Err(("stale_revision", Text::BoxFitRetry.plain().into()));
        }
        if self.runtime.in_play_mode() {
            return Err((
                "play_active",
                "Stop Play before fitting a Box Collider.".into(),
            ));
        }
        if self.authored_edit_pending() || self.ai_transition_pending() {
            return Err(("drafts_pending", Text::ColliderFinishEdit.plain().into()));
        }
        let owner = self
            .session
            .scene()
            .objects
            .iter()
            .find(|owner| owner.id == owner_id)
            .ok_or((
                "object_not_found",
                "The collider owner no longer exists.".into(),
            ))?;
        let collider_index = owner
            .components
            .iter()
            .position(|component| component.id == collider_id)
            .ok_or((
                "component_not_found",
                "Choose a Box Collider on this object.".into(),
            ))?;
        let collider = &owner.components[collider_index];
        let Component::Collider {
            role,
            shape: ColliderShape::Box { .. },
            ..
        } = collider.kind
        else {
            return Err((
                "invalid_collider",
                "Fit to Mesh needs a Box Collider.".into(),
            ));
        };
        let mut renderers = owner
            .components
            .iter()
            .filter_map(|component| match &component.kind {
                Component::MeshRenderer { geometry, .. } => Some((component.id.as_str(), geometry)),
                _ => None,
            });
        let (renderer, geometry) = if let Some(id) = renderer_id {
            renderers.find(|(renderer, _)| *renderer == id).ok_or((
                "component_not_found",
                "Choose a Mesh Renderer on the collider's object.".into(),
            ))?
        } else {
            let renderer = renderers.next().ok_or((
                "mesh_renderer_required",
                "Add a Mesh Renderer to the Box Collider's object.".into(),
            ))?;
            if renderers.next().is_some() {
                return Err((
                    "ambiguous_mesh_renderer",
                    "Choose a specific Mesh Renderer.".into(),
                ));
            }
            renderer
        };

        let mesh = match geometry {
            Geometry::Cube => rot_render::builtin_cube(),
            Geometry::Primitive { shape } => rot_render::builtin_mesh(*shape).as_ref(),
            Geometry::Imported { asset } => {
                if asset_revision.is_none() {
                    return Err((
                        "asset_revision_required",
                        "Inspect the mesh and pin expected_asset_revision.".into(),
                    ));
                }
                if self.assets_loading() {
                    return Err(("assets_loading", "Wait for project assets to load.".into()));
                }
                if asset_revision != Some(self.material_asset_revision()) {
                    return Err((
                        "stale_asset_revision",
                        "Inspect the project assets again.".into(),
                    ));
                }
                self.assets
                    .as_ref()
                    .and_then(|store| store.mesh(asset))
                    .ok_or((
                        "mesh_unavailable",
                        format!("Mesh {asset} has no accepted geometry. Restore or reassign it."),
                    ))?
            }
        };
        let (center, half_extents, minimum_axes) = indexed_bounds(mesh)?;
        let new_collider = Collider {
            enabled: collider.enabled,
            role,
            offset: center,
            shape: ColliderShape::Box { half_extents },
        };
        new_collider
            .validate()
            .map_err(|error| ("invalid_fit", error.to_string()))?;
        let renderer = renderer.to_owned();
        let mut object = owner.clone();
        object.components[collider_index].kind = Component::Collider {
            role,
            offset: center,
            shape: new_collider.shape,
        };
        let next = self
            .session
            .put_object(revision, object)
            .map_err(|error| ("invalid_fit", error.to_string()))?;
        if next != revision {
            self.refresh();
        }
        Ok(Fit {
            renderer,
            center,
            half_extents,
            minimum_axes,
            changed: next != revision,
        })
    }
}

pub(super) struct Fit {
    renderer: String,
    center: [f32; 3],
    half_extents: [f32; 3],
    minimum_axes: Vec<&'static str>,
    changed: bool,
}
impl Fit {
    pub(super) fn response(&self, object: &str, collider: &str, dirty: bool) -> Value {
        json!({"object_id":object,"component_id":collider,"renderer_component_id":self.renderer,
            "center":self.center,"half_extents":self.half_extents,"minimum_axes":self.minimum_axes,
            "changed":self.changed,"dirty":dirty})
    }
}

type Bounds = ([f32; 3], [f32; 3], Vec<&'static str>);

fn indexed_bounds(mesh: &Mesh) -> Result<Bounds, FitError> {
    let invalid_mesh = || {
        (
            "mesh_unavailable",
            "This mesh has no usable indexed triangles.".into(),
        )
    };
    if mesh.indices.is_empty() || !mesh.indices.len().is_multiple_of(3) {
        return Err(invalid_mesh());
    }
    let mut low = glam::Vec3::splat(f32::INFINITY);
    let mut high = glam::Vec3::splat(f32::NEG_INFINITY);
    for &index in &mesh.indices {
        let vertex = mesh.vertices.get(index as usize).ok_or_else(invalid_mesh)?;
        let position = glam::Vec3::from_array(vertex.position);
        if !position.is_finite() {
            return Err(invalid_mesh());
        }
        low = low.min(position);
        high = high.max(position);
    }
    let low = low.to_array().map(f64::from);
    let high = high.to_array().map(f64::from);
    let mut center = [0.0; 3];
    let mut half_extents = [0.0; 3];
    let mut minimum_axes = Vec::new();
    // Refit around the stored f32 center so quantizing it cannot cut off an edge.
    for (axis, label) in ["x", "y", "z"].into_iter().enumerate() {
        center[axis] = ((low[axis] + high[axis]) * 0.5) as f32;
        let mid = f64::from(center[axis]);
        let extent = (mid - low[axis]).max(high[axis] - mid);
        if extent > 1000.0 || mid.abs() > 10_000.0 {
            return Err((
                "invalid_fit",
                "These local mesh bounds exceed the Box Collider's Center or Size limits.".into(),
            ));
        }
        if extent < 0.001 {
            minimum_axes.push(label);
        }
        let narrowed = extent as f32;
        let enclosing = if f64::from(narrowed) < extent {
            narrowed.next_up()
        } else {
            narrowed
        };
        half_extents[axis] = enclosing.max(0.001);
    }
    Ok((center, half_extents, minimum_axes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::{
        component_scene::{ComponentInstance, ComponentScene, ComponentSession},
        ColliderRole, Scene, SceneError, Session,
    };

    #[test]
    fn bounds_use_only_indices_and_refuse_invalid_geometry() {
        let mut mesh = Mesh {
            vertices: [
                [1.5, -2.0, 4.0],
                [4.5, -2.0, 4.0],
                [1.5, 1.0, 4.0],
                [999.0; 3],
            ]
            .into_iter()
            .map(|position| rot_assets::Vertex {
                position,
                normal: [0.0, 0.0, 1.0],
            })
            .collect(),
            indices: vec![0, 1, 2],
            texcoords: None,
        };
        assert_eq!(
            indexed_bounds(&mesh).unwrap(),
            ([3.0, -0.5, 4.0], [1.5, 1.5, 0.001], vec!["z"])
        );
        mesh.vertices[0].position[0] = 9999.0;
        assert_eq!(indexed_bounds(&mesh).unwrap_err().0, "invalid_fit");
        mesh.indices[2] = 20;
        assert_eq!(indexed_bounds(&mesh).unwrap_err().0, "mesh_unavailable");
    }

    #[test]
    fn guarded_fit_resolves_exact_renderer_and_changes_only_box_geometry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("yard.scene.json");
        std::fs::create_dir(directory.path().join("Assets")).unwrap();
        std::fs::write(
            directory.path().join("Assets/Barrier.obj"),
            "v 1 -2 4\nv 5 -2 4\nv 1 2 4\nf 1 2 3\n",
        )
        .unwrap();
        let mut store = rot_assets::AssetStore::open(directory.path()).unwrap();
        store.import("barrier", "Assets/Barrier.obj").unwrap();
        let legacy = Session::new(Scene::demo()).unwrap();
        let mut scene = ComponentScene::from_legacy(&legacy).unwrap();
        let renderer = ComponentInstance {
            id: "barrier-mesh".into(),
            enabled: false,
            kind: Component::MeshRenderer {
                geometry: Geometry::Imported {
                    asset: "barrier".into(),
                },
                material: None,
                tint: [1.0; 4],
            },
        };
        let sensor = ComponentInstance {
            id: "trigger".into(),
            enabled: false,
            kind: Component::Collider {
                role: ColliderRole::Sensor,
                offset: [0.0; 3],
                shape: ColliderShape::default(),
            },
        };
        scene.objects[0].components.extend([renderer, sensor]);
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.assets = Some(store.project_snapshot());

        let mut args = json!({
            "expected_session_token":"guard","expected_revision":"0",
            "edits":[{"operation":"fit_box_to_mesh","object_id":"left","component_id":"trigger"}],
        });
        let submit = |editor: &mut ComponentWorkspace, args: &Value| {
            editor.ai_command("guard", "scene_components_edit", args)
        };
        assert_eq!(
            submit(&mut editor, &args)["error"]["code"],
            "ambiguous_mesh_renderer"
        );
        args["edits"][0]["renderer_component_id"] = json!("trigger");
        assert_eq!(
            submit(&mut editor, &args)["error"]["code"],
            "component_not_found"
        );
        args["edits"][0]["renderer_component_id"] = json!("barrier-mesh");
        assert_eq!(
            submit(&mut editor, &args)["error"]["code"],
            "asset_revision_required"
        );
        args["expected_asset_revision"] = json!((store.revision() + 1).to_string());
        assert_eq!(
            submit(&mut editor, &args)["error"]["code"],
            "stale_asset_revision"
        );
        assert_eq!(editor.session.scene(), &scene);
        assert!(matches!(editor.session.undo(0), Err(SceneError::NoUndo)));

        args["expected_asset_revision"] = json!(store.revision().to_string());
        let result = submit(&mut editor, &args);
        assert_eq!(result["result"]["revision"], "1", "{result}");
        assert_eq!(result["result"]["renderer_component_id"], "barrier-mesh");
        assert_eq!(result["result"]["center"], json!([3.0, 0.0, 4.0]));
        assert_eq!(result["result"]["minimum_axes"], json!(["z"]));
        let mut expected = scene.clone();
        expected.objects[0].components.last_mut().unwrap().kind = Component::Collider {
            role: ColliderRole::Sensor,
            offset: [3.0, 0.0, 4.0],
            shape: ColliderShape::Box {
                half_extents: [2.0, 2.0, 0.001],
            },
        };
        assert_eq!(editor.session.scene(), &expected);
        args["expected_revision"] = json!("1");
        let no_op = submit(&mut editor, &args);
        assert_eq!(no_op["result"]["changed"], false);
        assert_eq!(editor.session.revision(), 1);

        editor.command(crate::workspace::Action::Undo);
        assert_eq!(editor.session.scene(), &scene);
        editor.command(crate::workspace::Action::Redo);
        assert_eq!(editor.session.scene(), &expected);
        assert!(editor.save());
        assert_eq!(ComponentSession::open(path).unwrap().scene(), &expected);
    }
}
