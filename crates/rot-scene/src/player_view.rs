//! Renderer-independent authored perspective view and world anchor admission.
use crate::{validate_id, ResolvedPoses, Scene, SceneDelta, SceneError, Session};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlayerAnchor {
    Fixed { position: [f32; 3] },
    Follow { target_id: String, offset: [f32; 3] },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerView {
    pub anchor: PlayerAnchor,
    /// Radians. Zero looks from +Z toward the anchor.
    pub yaw: f32,
    /// Radians. The bounds leave room for a stable up vector at steep angles.
    pub pitch: f32,
    pub distance: f32,
    pub vertical_fov_degrees: f32,
}

/// A committed camera observation, containing no viewport or renderer state.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ResolvedPlayerView {
    pub anchor: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub vertical_fov_degrees: f32,
}

fn coordinate(value: [f32; 3]) -> bool {
    value
        .into_iter()
        .all(|component| component.is_finite() && component.abs() <= 10_000.0)
}

impl PlayerView {
    pub fn followed_target(&self) -> Option<&str> {
        match &self.anchor {
            PlayerAnchor::Fixed { .. } => None,
            PlayerAnchor::Follow { target_id, .. } => Some(target_id),
        }
    }

    pub fn validate_settings(&self) -> Result<(), SceneError> {
        if !self.yaw.is_finite()
            || self.yaw.abs() > 10_000.0
            || !self.pitch.is_finite()
            || self.pitch.abs() > 89.0_f32.to_radians()
            || !(0.1..=1000.0).contains(&self.distance)
            || !(10.0..=120.0).contains(&self.vertical_fov_degrees)
        {
            return Err(SceneError::Invalid("Player view requires yaw within ±10000 radians, pitch within ±89°, distance 0.1..1000 and vertical FOV 10..120°".into()));
        }
        match &self.anchor {
            PlayerAnchor::Fixed { position } if !coordinate(*position) => {
                return Err(SceneError::Invalid(
                    "Player view anchor must be finite and within ±10000 world units".into(),
                ));
            }
            PlayerAnchor::Follow { target_id, offset } => {
                validate_id(target_id)?;
                if !coordinate(*offset) {
                    return Err(SceneError::Invalid(
                        "Player view offset must be finite and within ±10000 world units".into(),
                    ));
                }
            }
            PlayerAnchor::Fixed { .. } => {}
        }
        Ok(())
    }

    fn resolve(
        &self,
        scene: &Scene,
        poses: Option<&ResolvedPoses>,
    ) -> Result<ResolvedPlayerView, SceneError> {
        self.validate_settings()?;
        let anchor = match &self.anchor {
            PlayerAnchor::Fixed { position } => *position,
            PlayerAnchor::Follow { target_id, offset } => {
                let object = scene.objects.iter().find(|object| object.id == *target_id);
                let group = scene.hierarchy.groups.get(target_id);
                let target = object.map(|object| &object.transform)
                    .or_else(|| group.map(|group| &group.transform))
                    .ok_or_else(|| SceneError::Invalid(format!(
                        "Player view target {target_id} must be an existing Object or Group; clear or retarget the player view first"
                    )))?;
                let position = poses
                    .and_then(|poses| poses.transforms.get(target_id))
                    .unwrap_or(target)
                    .translation;
                let mut anchor = [0.0; 3];
                for ((out, position), offset) in anchor.iter_mut().zip(position).zip(*offset) {
                    let resolved = f64::from(position) + f64::from(offset);
                    if !resolved.is_finite() || resolved.abs() > 10_000.0 {
                        return Err(SceneError::Invalid(
                            "Resolved player view anchor must remain within ±10000 world units"
                                .into(),
                        ));
                    }
                    *out = resolved as f32;
                }
                anchor
            }
        };
        if !coordinate(anchor) {
            return Err(SceneError::Invalid(
                "Player view anchor must be finite and within ±10000 world units".into(),
            ));
        }
        Ok(ResolvedPlayerView {
            anchor,
            yaw: self.yaw,
            pitch: self.pitch,
            distance: self.distance,
            vertical_fov_degrees: self.vertical_fov_degrees,
        })
    }
}

impl Scene {
    pub fn resolve_player_view(&self) -> Result<Option<ResolvedPlayerView>, SceneError> {
        let Some(view) = &self.player_view else {
            return Ok(None);
        };
        if view.followed_target().is_some() && !self.hierarchy.is_empty() {
            let poses = self.resolve_poses()?;
            view.resolve(self, Some(&poses)).map(Some)
        } else {
            view.resolve(self, None).map(Some)
        }
    }

    pub fn validate_player_view_with_poses(&self, poses: &ResolvedPoses) -> Result<(), SceneError> {
        if let Some(view) = &self.player_view {
            view.resolve(self, Some(poses))?;
        }
        Ok(())
    }
}

impl Session {
    pub fn set_player_view(
        &mut self,
        expected_revision: u64,
        view: Option<PlayerView>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        if self.scene().player_view == view {
            return Ok(self.scene().revision);
        }
        self.commit_change(SceneDelta::PlayerView {
            before: self.scene().player_view.clone(),
            after: view,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Object, Transform, PLAYER_VIEW_FORMAT_VERSION};

    fn followed() -> PlayerView {
        PlayerView {
            anchor: PlayerAnchor::Follow {
                target_id: "subject".into(),
                offset: [0., 1., 0.],
            },
            yaw: 0.,
            pitch: 0.,
            distance: 6.,
            vertical_fov_degrees: 60.,
        }
    }

    #[test]
    fn player_view_history_retains_version_and_protects_follow_target() {
        let mut scene = Scene::empty("view");
        let mut subject = Object::cube("subject", None);
        subject.transform = Transform {
            rotation: [0., 0., 1.],
            scale: [2.; 3],
            ..Default::default()
        };
        scene.objects.push(subject);
        let mut session = Session::new(scene).unwrap();
        let view = followed();
        session.set_player_view(0, Some(view.clone())).unwrap();
        assert_eq!(
            session
                .scene()
                .resolve_player_view()
                .unwrap()
                .unwrap()
                .anchor,
            [0., 1., 0.]
        );
        let before = session.snapshot_bytes().unwrap();
        let blocked = session.delete(1, "subject").unwrap_err();
        assert!(blocked.to_string().contains("clear or retarget"));
        assert_eq!(session.snapshot_bytes().unwrap(), before);
        session.undo(1).unwrap();
        let retained = session.snapshot_bytes().unwrap();
        let mut restored = Session::from_snapshot_bytes(&retained).unwrap();
        assert_eq!(restored.loaded_format_version(), PLAYER_VIEW_FORMAT_VERSION);
        assert!(restored.scene().player_view.is_none());
        let mut wrong: serde_json::Value = serde_json::from_slice(&retained).unwrap();
        wrong["format_version"] = 20.into();
        assert!(Session::from_snapshot_bytes(&serde_json::to_vec(&wrong).unwrap()).is_err());
        restored.redo(2).unwrap();
        assert_eq!(restored.scene().player_view, Some(view));
        restored.set_player_view(3, None).unwrap();
        restored.delete(4, "subject").unwrap();
    }
}
