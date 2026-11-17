//! Atomic bounded lighting authoring. Runtime GPU state and editor markers live elsewhere.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightingEdit {
    pub revision: u64,
    /// Includes an explicit legacy sun created by promotion, if any.
    pub created_ids: Vec<String>,
}

impl Session {
    pub(crate) fn promoted_lighting(
        &self,
        requested_ids: &[&str],
        requested_directional: bool,
    ) -> Result<(Lighting, Vec<String>), SceneError> {
        if !self.scene().lighting.is_legacy() {
            return Ok((self.scene().lighting.clone(), Vec::new()));
        }
        if requested_directional {
            return Ok((
                Lighting::Authored {
                    ambient: Ambient::default(),
                    lights: BTreeMap::new(),
                },
                Vec::new(),
            ));
        }
        // Reserve the entire incoming selection as well as reversible IDs;
        // geometry may legitimately use the same light-prefixed namespace.
        let mut used = self.reserved_target_ids();
        used.extend(requested_ids.iter().copied());
        let sun = allocate_ids("light", 1, MAX_LIGHTS, used)?
            .pop()
            .expect("one allocated identity");
        Ok((Lighting::authored_default(sun.clone()), vec![sun]))
    }

    /// Creating the first light promotes legacy illumination in the same Undo
    /// transaction. A point retains the old sun as an explicit, counted light.
    pub fn create_light(
        &mut self,
        expected_revision: u64,
        id: &str,
        light: Light,
    ) -> Result<LightingEdit, SceneError> {
        self.check_revision(expected_revision)?;
        validate_id(id)?;
        light.validate()?;
        if self.contains_object_id(id) {
            return Err(SceneError::Invalid(format!(
                "scene target id {id} is present or reserved by history"
            )));
        }
        let before = self.scene().lighting.clone();
        let (mut after, mut created_ids) =
            self.promoted_lighting(&[id], matches!(light.kind, LightKind::Directional { .. }))?;
        let Lighting::Authored { lights, .. } = &mut after else {
            unreachable!()
        };
        lights.insert(id.into(), light);
        let revision = self.commit_change(SceneDelta::Lighting { before, after })?;
        created_ids.push(id.into());
        Ok(LightingEdit {
            revision,
            created_ids,
        })
    }

    /// Replace properties of an existing light without changing its kind/identity.
    pub fn set_light(
        &mut self,
        expected_revision: u64,
        id: &str,
        light: Light,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        validate_id(id)?;
        light.validate()?;
        let existing = self
            .scene()
            .lighting
            .light(id)
            .ok_or_else(|| SceneError::LightNotFound(id.into()))?;
        if std::mem::discriminant(&existing.kind) != std::mem::discriminant(&light.kind) {
            return Err(SceneError::Invalid(
                "light kind is immutable; delete and create explicitly".into(),
            ));
        }
        if existing == &light {
            return Ok(self.scene().revision);
        }
        let before = self.scene().lighting.clone();
        let mut after = before.clone();
        let Lighting::Authored { lights, .. } = &mut after else {
            unreachable!()
        };
        lights.insert(id.into(), light);
        self.commit_change(SceneDelta::Lighting { before, after })
    }

    pub fn remove_light(&mut self, expected_revision: u64, id: &str) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        validate_id(id)?;
        if self.scene().lighting.light(id).is_none() {
            return Err(SceneError::LightNotFound(id.into()));
        }
        let before = self.scene().lighting.clone();
        let mut after = before.clone();
        let Lighting::Authored { lights, .. } = &mut after else {
            unreachable!()
        };
        lights.remove(id);
        self.commit_change(SceneDelta::Lighting { before, after })
    }

    pub fn set_ambient(
        &mut self,
        expected_revision: u64,
        ambient: Ambient,
    ) -> Result<LightingEdit, SceneError> {
        self.check_revision(expected_revision)?;
        ambient.validate()?;
        let before = self.scene().lighting.clone();
        let (mut after, created_ids) = self.promoted_lighting(&[], false)?;
        let Lighting::Authored {
            ambient: target, ..
        } = &mut after
        else {
            unreachable!()
        };
        *target = ambient;
        if before == after {
            return Ok(LightingEdit {
                revision: self.scene().revision,
                created_ids,
            });
        }
        let revision = self.commit_change(SceneDelta::Lighting { before, after })?;
        Ok(LightingEdit {
            revision,
            created_ids,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promotion_history_capacity_and_reserved_ids_are_atomic() {
        let mut session = Session::new(Scene::demo()).unwrap();
        let legacy = session.scene().clone();
        let edit = session
            .create_light(0, "light-1", Light::point(Some("Key".into())))
            .unwrap();
        assert_eq!(edit.created_ids, ["light-2", "light-1"]);
        assert_eq!(session.scene().lighting.light_count(), 2);
        assert_eq!(session.scene().objects, legacy.objects);
        let authored = session.scene().lighting.clone();
        session.undo(1).unwrap();
        assert!(session.scene().lighting.is_legacy());
        assert!(session
            .create_light(2, "light-1", Light::point(None))
            .is_err());
        session.redo(2).unwrap();
        assert_eq!(session.scene().lighting, authored);
        let mut invalid = Light::point(None);
        invalid.transform.scale[0] = 2.;
        let unchanged = session.clone();
        assert!(session.set_light(3, "light-1", invalid).is_err());
        assert_eq!(
            session.snapshot_bytes().unwrap(),
            unchanged.snapshot_bytes().unwrap()
        );
        for id in ["fill", "rim", "bounce"] {
            session
                .create_light(session.scene().revision, id, Light::point(None))
                .unwrap();
        }
        let before = session.snapshot_bytes().unwrap();
        assert!(session
            .create_light(session.scene().revision, "overflow", Light::point(None))
            .is_err());
        assert_eq!(session.snapshot_bytes().unwrap(), before);
        assert!(session
            .allocate_object_ids(1)
            .unwrap()
            .iter()
            .all(|id| !session.contains_object_id(id)));

        let mut full = Scene::empty("full");
        full.objects = (0..MAX_OBJECTS)
            .map(|n| Object::cube(format!("object-{n}"), None))
            .collect();
        let mut full = Session::new(full).unwrap();
        let bytes = full.snapshot_bytes().unwrap();
        assert!(full.create_light(0, "point", Light::point(None)).is_err());
        assert!(full.set_ambient(0, Ambient::default()).is_err());
        assert_eq!(full.snapshot_bytes().unwrap(), bytes);
    }

    #[test]
    fn authored_zero_roundtrip_and_pre_v7_rejection_preserve_legacy() {
        let mut scene = Scene::empty("lighting");
        scene.objects.push(Object::cube("light-1", None));
        let mut session = Session::new(scene).unwrap();
        let edit = session
            .set_ambient(
                0,
                Ambient {
                    color: [1.; 3],
                    intensity: 0.,
                },
            )
            .unwrap();
        assert_eq!(edit.created_ids, ["light-2"]);
        session.remove_light(1, "light-2").unwrap();
        assert!(
            matches!(session.scene().lighting, Lighting::Authored { ref lights, ref ambient } if lights.is_empty() && ambient.intensity == 0.)
        );
        let bytes = session.snapshot_bytes().unwrap();
        let restored = Session::from_snapshot_bytes(&bytes).unwrap();
        assert_eq!(restored.scene(), session.scene());
        let mut bad = serde_json::to_value(&session.document).unwrap();
        bad["format_version"] = 6.into();
        assert!(Document::decode(&serde_json::to_vec(&bad).unwrap())
            .unwrap_err()
            .to_string()
            .contains("lighting requires scene v7"));
        let mut legacy = serde_json::to_value(Document::new(Scene::demo())).unwrap();
        legacy["format_version"] = 6.into();
        assert!(Document::decode(&serde_json::to_vec(&legacy).unwrap())
            .unwrap()
            .scene
            .lighting
            .is_legacy());
        legacy["scene"]["lighting"] = serde_json::Value::Null;
        assert!(Document::decode(&serde_json::to_vec(&legacy).unwrap()).is_err());
        let new_scene = Scene::authored_empty("new");
        assert!(new_scene.objects.is_empty());
        assert_eq!(new_scene.target_count(), 1);
    }

    #[test]
    fn mixed_target_edits_and_clipboard_are_atomic_and_reversible() {
        let mut session = Session::new(Scene::demo()).unwrap();
        session
            .create_light(0, "point", Light::point(None))
            .unwrap();
        let start = session.scene().clone();
        let geometry_id = start.objects[0].id.clone();
        let mut geometry = start.objects[0].transform.clone();
        geometry.translation[0] += 2.;
        let mut light = start.lighting.light("point").unwrap().transform.clone();
        light.translation[0] -= 2.;
        session
            .edit_transforms(
                1,
                &[
                    (geometry_id.clone(), geometry.clone()),
                    ("point".into(), light.clone()),
                ],
            )
            .unwrap();
        assert_eq!(session.scene().revision, 2);
        assert_eq!(session.scene().target_transform("point"), Some(&light));
        session.undo(2).unwrap();
        assert_eq!(session.scene().objects, start.objects);
        assert_eq!(session.scene().lighting, start.lighting);
        session.redo(3).unwrap();
        let bytes = session.snapshot_bytes().unwrap();
        light.scale[1] = 2.;
        assert!(session
            .edit_transforms(
                4,
                &[(geometry_id.clone(), geometry), ("point".into(), light)]
            )
            .is_err());
        assert_eq!(session.snapshot_bytes().unwrap(), bytes);
        assert!(session.rename(3, "point", Some("stale".into())).is_err());
        session.rename(4, "point", Some("Fill".into())).unwrap();
        assert_eq!(session.scene().target_display_name("point"), Some("Fill"));
        session
            .duplicate_many(
                5,
                &[
                    (geometry_id.clone(), "copy".into()),
                    ("point".into(), "copy-point".into()),
                ],
            )
            .unwrap();
        assert!(session.scene().target_transform("copy").is_some());
        assert_eq!(
            session.scene().target_display_name("copy-point"),
            Some("Fill")
        );
        // Copying a Sun would exceed the directional cap; no geometry may leak.
        let bytes = session.snapshot_bytes().unwrap();
        assert!(session
            .duplicate_many(
                6,
                &[
                    (geometry_id.clone(), "leak".into()),
                    ("light-1".into(), "second-sun".into())
                ]
            )
            .is_err());
        assert_eq!(session.snapshot_bytes().unwrap(), bytes);
        let before_delete = session.scene().clone();
        session
            .delete_many(6, &["copy".into(), "copy-point".into()])
            .unwrap();
        assert!(session.scene().target_transform("copy").is_none());
        assert!(session.scene().target_transform("copy-point").is_none());
        session.undo(7).unwrap();
        assert_eq!(session.scene().objects, before_delete.objects);
        assert_eq!(session.scene().lighting, before_delete.lighting);
        let restored = Session::from_snapshot_bytes(&session.snapshot_bytes().unwrap()).unwrap();
        assert_eq!(restored.scene(), session.scene());

        let mut paste = Session::new(Scene::empty("paste")).unwrap();
        let incoming = (1..=4)
            .map(|n| (format!("light-{n}"), Light::point(None)))
            .collect();
        let edit = paste
            .create_many_with_materials_and_lights(
                0,
                vec![Object::cube("light-5", None)],
                BTreeMap::new(),
                incoming,
            )
            .unwrap();
        assert_eq!(edit.created_ids[0], "light-6");
        assert_eq!(paste.scene().target_count(), 6);
        paste.undo(1).unwrap();
        assert!(paste.scene().objects.is_empty());
        assert!(paste.scene().lighting.is_legacy());
        let mut incoming_sun = Session::new(Scene::empty("sun-paste")).unwrap();
        let edit = incoming_sun
            .create_many_with_materials_and_lights(
                0,
                Vec::new(),
                BTreeMap::new(),
                BTreeMap::from([("sun".into(), Light::directional(None))]),
            )
            .unwrap();
        assert_eq!(edit.created_ids, ["sun"]);
        assert_eq!(incoming_sun.scene().lighting.light_count(), 1);
    }

    #[test]
    fn history_cannot_change_light_kind_under_an_identity() {
        let before = Lighting::Authored {
            ambient: Ambient::default(),
            lights: BTreeMap::from([("same".into(), Light::point(None))]),
        };
        let after = Lighting::authored_default("same");
        let mut scene = Scene::empty("malformed");
        scene.lighting = before.clone();
        let unchanged = scene.clone();
        let delta = SceneDelta::Lighting { before, after };
        assert!(delta.apply(&mut scene, false).is_err());
        assert_eq!(scene, unchanged);
    }

    // Unique risk: complete off-default Sun settings must survive whole-light
    // history and mixed clipboard operations without changing kind/ID rules.
    #[test]
    fn shadow_settings_survive_disabled_history_and_atomic_mixed_clipboard() {
        let mut scene = Scene::demo();
        scene.lighting = Lighting::authored_default("sun");
        let mut session = Session::new(scene).unwrap();
        let mut sun = session.scene().lighting.light("sun").unwrap().clone();
        let enabled = SunShadow {
            enabled: true,
            quality: ShadowQuality::High,
            depth_bias_texels: 0.9,
            normal_bias_texels: 0.25,
        };
        sun.kind = LightKind::Directional { shadow: enabled };
        session.set_light(0, "sun", sun.clone()).unwrap();
        let enabled_lighting = session.scene().lighting.clone();
        sun.kind = LightKind::Directional {
            shadow: SunShadow {
                enabled: false,
                ..enabled
            },
        };
        session.set_light(1, "sun", sun.clone()).unwrap();
        let mut restored =
            Session::from_snapshot_bytes(&session.snapshot_bytes().unwrap()).unwrap();
        assert_eq!(restored.scene(), session.scene());
        restored.undo(2).unwrap();
        assert_eq!(restored.scene().lighting, enabled_lighting);
        restored.redo(3).unwrap();
        assert_eq!(restored.scene().lighting.light("sun"), Some(&sun));
        let unchanged = restored.snapshot_bytes().unwrap();
        assert!(restored
            .duplicate_many(
                4,
                &[
                    ("left".into(), "copy".into()),
                    ("sun".into(), "sun-copy".into())
                ]
            )
            .is_err());
        assert_eq!(restored.snapshot_bytes().unwrap(), unchanged);
        let mut paste = Session::new(Scene::empty("shadow-paste")).unwrap();
        paste
            .create_many_with_materials_and_lights(
                0,
                vec![Object::cube("pasted-object", None)],
                BTreeMap::new(),
                BTreeMap::from([("pasted-sun".into(), sun.clone())]),
            )
            .unwrap();
        let pasted = paste.scene().clone();
        paste.undo(1).unwrap();
        assert!(paste.scene().lighting.is_legacy());
        assert!(paste.scene().objects.is_empty());
        paste.redo(2).unwrap();
        assert_eq!(paste.scene().objects, pasted.objects);
        assert_eq!(paste.scene().lighting.light("pasted-sun"), Some(&sun));
        let reopened = Session::from_snapshot_bytes(&paste.snapshot_bytes().unwrap()).unwrap();
        assert_eq!(reopened.scene(), paste.scene());
    }
}
