//! Scene-owned framing observation helpers.
use rot_scene::Scene;
use serde_json::{json, Value};

pub(super) fn camera_observation(scene: &Scene) -> Value {
    let Ok(Some(view)) = scene.resolve_player_view() else {
        return Value::Null;
    };
    resolved_camera_observation(view)
}

pub(super) fn resolved_camera_observation(view: rot_scene::ResolvedPlayerView) -> Value {
    let matrices: Vec<_> = [4. / 3., 16. / 9.]
        .into_iter()
        .filter_map(|aspect| {
            rot_render::player_view_projection(view, aspect).map(|p| {
                json!({"aspect": aspect, "eye": p.eye, "view": p.view, "projection": p.projection})
            })
        })
        .collect();
    json!({"resolved":view, "matrices":matrices})
}
