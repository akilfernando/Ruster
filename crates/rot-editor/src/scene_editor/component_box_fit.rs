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
