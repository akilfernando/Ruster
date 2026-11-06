//! Scalar feedback only; this owns no meshes, textures or GPU resource bundles.
use super::SceneEditor;
use eframe::egui;
use ruster_render::{ShadowObservation, ShadowStatus};
use serde_json::{json, Value};

fn status_name(status: ShadowStatus) -> &'static str {
    match status {
        ShadowStatus::Disabled => "disabled",
        ShadowStatus::Ready => "ready",
        ShadowStatus::Pending => "pending_quality",
        ShadowStatus::NoGeometry => "no_geometry",
        ShadowStatus::InvalidCoverage => "invalid_coverage",
        ShadowStatus::UnsupportedDevice => "unsupported_device",
        ShadowStatus::CustomShader => "unsupported_custom_shader",
        ShadowStatus::MultipleViewports => "unsupported_multiple_viewports",
    }
}

fn quality(size: u32) -> &'static str {
    match size {
        1024 => "Standard",
        2048 => "High",
        _ => "disabled",
    }
}

fn detail_advice(o: ShadowObservation) -> Option<&'static str> {
    if o.precision_limited {
        Some("Geometry may lose shadow precision at this scale. Move it nearer the origin or use larger features.")
    } else if !o.detail_limited {
        None
    } else if o.applied_map_size == 2048 {
        Some("Shadow detail is limited across this scene. Move distant geometry closer or use larger features.")
    } else {
        Some("Shadows lose small details across this scene. Try High quality or move distant geometry closer.")
    }
}

impl SceneEditor {
    pub(super) fn surface_feedback_ui(&self, ui: &mut egui::Ui) {
        if let Some(o) = self
            .render
            .as_ref()
            .and_then(ruster_render::display_observation)
        {
            if let Some(message) = o.status.warning() {
                ui.colored_label(ui.visuals().warn_fg_color, message)
                    .on_hover_text("Status from the last viewport update; runtime preview changes take effect after Reset.");
            }
            if o.resolution_limited {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "Viewport resolution is reduced to fit the render-target limit.",
                );
            }
        }
    }

    fn last_shadow_observation(&self) -> Option<ShadowObservation> {
        ruster_render::shadow_observation(self.render.as_ref()?)
    }

    pub(super) fn surface_feedback(&self) -> Value {
        let observation = self
            .render
            .as_ref()
            .and_then(ruster_render::display_observation);
        let Some(o) = observation else {
            return serde_json::json!({"status":"not_prepared","scope":"last_prepared_callback_not_gpu_completion","may_be_stale":true});
        };
        serde_json::json!({"status":format!("{:?}",o.status),"scope":"last_prepared_callback_not_gpu_completion","may_be_stale":true,"prepared_frames":o.prepared_frames,
            "requested_hdr":o.requested_hdr,"applied_hdr":o.applied_hdr,"available_hdr":o.available_hdr,
            "requested_exposure_stops":o.requested_exposure_stops,"applied_exposure_stops":o.applied_exposure_stops,
            "requested_samples":o.requested_samples,"applied_samples":o.applied_samples,
            "requested_target_size":o.requested_target_size,"applied_target_size":o.applied_target_size,
            "resolution_limited":o.resolution_limited,"logical_live_bytes":o.live_bytes,"logical_retiring_bytes":o.retiring_bytes,
            "asset_pending":o.asset_pending,"asset_epoch_phase":o.asset_epoch_phase,
            "asset_live_mesh_bytes":o.asset_live_mesh_bytes,"asset_retired_mesh_bytes":o.asset_retired_mesh_bytes,
            "asset_live_texture_bytes":o.asset_live_texture_bytes,"asset_retired_texture_bytes":o.asset_retired_texture_bytes,
            "asset_scope":"Logical active/retired GPU asset payload and submission-maintenance phase; excludes targets, shadows, staging padding and driver/allocator overhead. Prepared scene state is not presentation."})
    }

    pub(super) fn shadow_feedback(&self) -> Value {
        let Some(o) = self.last_shadow_observation() else {
            return json!({"available":false,"reason":"no_prepared_viewport"});
        };
        json!({"available":true,
            "scope":"Last prepared viewport callback, possibly from an earlier authored/runtime view; not current document state or GPU completion. Hidden commands do not advance this report until a viewport prepares.",
            "prepared_frames":o.prepared_frames.to_string(),"status":status_name(o.status),
            "requested_map_size":o.requested_map_size,"applied_map_size":o.applied_map_size,
            "pending":o.pending,"cleanup_pending":o.retiring_bytes>0,
            "live_map_payload_bytes":o.live_bytes,"retiring_map_payload_bytes":o.retiring_bytes,
            "compatibility_payload_bytes":84,"world_units_per_texel":o.units_per_texel,
            "detail_limited":o.detail_limited,
            "precision_limited":o.precision_limited,
            "map_allocations":o.allocations,"encoded_depth_passes":o.encoded_depth_passes,
            "uniform_uploads":o.uniform_uploads,"uniform_payload_bytes":80,
            "detail_action":detail_advice(o)})
    }

    pub(super) fn shadow_feedback_ui(&self, ui: &mut egui::Ui) {
        let Some(o) = self.last_shadow_observation() else {
            return;
        };
        let message = match o.status {
            ShadowStatus::NoGeometry => {
                Some("Add geometry to cast and receive Sun shadows.".into())
            }
            ShadowStatus::InvalidCoverage => Some(
                "Shadow coverage is invalid or too large. Move distant geometry closer.".into(),
            ),
            ShadowStatus::UnsupportedDevice => {
                Some("Sun shadows are unavailable on this GPU.".into())
            }
            ShadowStatus::CustomShader => Some("Sun shadows need the built-in shader.".into()),
            ShadowStatus::MultipleViewports => {
                Some("Sun shadows support one viewport per renderer.".into())
            }
            ShadowStatus::Pending => Some(format!(
                "Applying {} shadows; the viewport is still using {}.",
                quality(o.requested_map_size),
                quality(o.applied_map_size)
            )),
            _ => detail_advice(o).map(str::to_owned),
        };
        if let Some(message) = message {
            ui.colored_label(ui.visuals().warn_fg_color, message)
                .on_hover_text(
                "Status from the last viewport update; runtime preview changes take effect after Reset.",
            );
        }
    }
}
