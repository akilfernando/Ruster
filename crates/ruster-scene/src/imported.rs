//! Atomic placement of a static imported template as ordinary authored scene values.
use crate::*;

/// A closed imported hierarchy beneath a new creator-owned placement root.
///
/// All IDs are caller-allocated scene IDs, not source indices. `hierarchy` contains
/// descendants only, with each descendant parented to another imported group or
/// `root_id`. Materials are new scene copies; object references must name these
/// copies. Mesh and texture IDs remain stable asset references, validated by the
/// asset boundary. This command creates no prefab link, edit lock or refresh hook:
/// later source-resource refresh leaves all placed scene values untouched.
#[derive(Clone, Debug)]
pub struct ImportedPlacement {
    pub root_id: String,
    pub root: Group,
    pub hierarchy: Hierarchy,
    pub objects: Vec<Object>,
    pub materials: BTreeMap<String, OpaqueMaterial>,
}

impl Session {
    /// Insert a complete static imported model in one guarded revision/Undo step.
    /// Failure preserves the scene, revision, dirty state and reversible history.
    pub fn place_imported(
        &mut self,
        expected_revision: u64,
        placement: ImportedPlacement,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        let invalid = |message: &str| SceneError::Invalid(message.into());
        if placement.objects.is_empty()
            || placement.hierarchy.groups.contains_key(&placement.root_id)
            || placement.hierarchy.parents.contains_key(&placement.root_id)
        {
            return Err(invalid(
                "imported placement requires meshes and one distinct root",
            ));
        }
        let mut imported = Scene::empty("imported-placement");
        imported.objects = placement.objects;
        imported.materials = placement.materials;
        imported.hierarchy = placement.hierarchy;
        imported
            .hierarchy
            .groups
            .insert(placement.root_id.clone(), placement.root);
        // Validate the isolated closure first: no links to existing scene targets
        // or materials may turn insertion into an accidental edit or attachment.
        imported.validate()?;
        for id in imported
            .hierarchy
            .groups
            .keys()
            .map(String::as_str)
            .chain(imported.objects.iter().map(|object| object.id.as_str()))
        {
            if self.contains_object_id(id) {
                return Err(invalid(
                    "imported target ID is present or reserved by history",
                ));
            }
            if id != placement.root_id && !imported.hierarchy.parents.contains_key(id) {
                return Err(invalid(
                    "every imported descendant must belong to the placement root",
                ));
            }
        }
        for object in &imported.objects {
            if object.mesh.is_none() || object.primitive.is_some() || object.collider.is_some() {
                return Err(invalid(
                    "imported placement accepts static mesh objects only",
                ));
            }
        }
        for id in imported.materials.keys() {
            if self.contains_material_id(id) {
                return Err(invalid(
                    "imported material ID is present or reserved by history",
                ));
            }
        }
        let before = self.scene().hierarchy.clone();
        let mut after = before.clone();
        after.groups.extend(imported.hierarchy.groups);
        after.parents.extend(imported.hierarchy.parents);
        let start = self.scene().objects.len();
        self.commit_change(Change {
            scene: vec![
                SceneDelta::Objects {
                    created: imported
                        .objects
                        .into_iter()
                        .enumerate()
                        .map(|(offset, object)| IndexedObject {
                            index: start + offset,
                            object,
                        })
                        .collect(),
                    deleted: Vec::new(),
                },
                SceneDelta::Hierarchy { before, after },
            ],
            materials: imported
                .materials
                .into_iter()
                .map(|(id, value)| ValueDelta {
                    id,
                    before: None,
                    after: Some(value),
                })
                .collect(),
            ..Change::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> ImportedPlacement {
        let group = |name: &str, x| Group {
            name: Some(name.into()),
            transform: Transform {
                translation: [x, 0., 0.],
                ..Transform::default()
            },
        };
        let mut object = Object::cube("part", Some("Mesh part".into()));
        object.mesh = Some("model-part-mesh".into());
        object.material = Some("model-material".into());
        object.transform.translation = [0., 0., 3.];
        ImportedPlacement {
            root_id: "placement".into(),
            root: group("Placement", 5.),
            hierarchy: Hierarchy {
                groups: BTreeMap::from([("node".into(), group("Imported node", 2.))]),
                parents: BTreeMap::from([
                    ("node".into(), "placement".into()),
                    ("part".into(), "node".into()),
                ]),
            },
            objects: vec![object],
            materials: BTreeMap::from([(
                "model-material".into(),
                OpaqueMaterial {
                    base_color_texture: Some("model-base-color".into()),
                    ..OpaqueMaterial::default()
                },
            )]),
        }
    }

    // Unique boundary: one transaction couples new topology, geometry and material
    // definitions, including failure and persisted history, unlike ordinary creation.
    #[test]
    fn imported_placement_atomic_history_and_closure() {
        let mut session = Session::new(Scene::empty("scene")).unwrap();
        let initial = session.snapshot_bytes().unwrap();
        let mut invalid = fixture();
        invalid
            .hierarchy
            .parents
            .insert("part".into(), "outside".into());
        assert!(session.place_imported(0, invalid).is_err());
        assert_eq!(session.snapshot_bytes().unwrap(), initial);
        let mut invalid = fixture();
        invalid.hierarchy.parents.remove("part");
        assert!(session.place_imported(0, invalid).is_err());
        assert_eq!(session.snapshot_bytes().unwrap(), initial);
        let mut invalid = fixture();
        invalid.root.transform.translation[0] = 10000.;
        assert!(session.place_imported(0, invalid).is_err());
        assert_eq!(session.snapshot_bytes().unwrap(), initial);
        assert!(session.place_imported(1, fixture()).is_err());
        assert_eq!(session.snapshot_bytes().unwrap(), initial);

        assert_eq!(session.place_imported(0, fixture()).unwrap(), 1);
        assert_eq!(session.undo_len(), 1);
        assert_eq!(
            session.scene().resolve_poses().unwrap().transforms["part"].translation,
            [7., 0., 3.]
        );
        assert!(session.has_asset_references());
        let placed = session.snapshot_bytes().unwrap();
        assert!(session.place_imported(1, fixture()).is_err());
        assert_eq!(session.snapshot_bytes().unwrap(), placed);
        let mut reopened = Session::from_snapshot_bytes(&placed).unwrap();
        reopened.undo(1).unwrap();
        assert!(reopened.scene().objects.is_empty());
        assert!(reopened.scene().hierarchy.is_empty());
        assert!(reopened.scene().materials.is_empty());
        let undone = reopened.snapshot_bytes().unwrap();
        assert!(reopened.place_imported(2, fixture()).is_err()); // history reserves all IDs
        assert_eq!(reopened.snapshot_bytes().unwrap(), undone);
        reopened = Session::from_snapshot_bytes(&undone).unwrap();
        reopened.redo(2).unwrap();
        assert_eq!(reopened.scene().objects, session.scene().objects);
        assert_eq!(reopened.scene().materials, session.scene().materials);
        assert_eq!(reopened.scene().hierarchy, session.scene().hierarchy);
        let mut edited = reopened.scene().objects[0].transform.clone();
        edited.translation[2] = 4.;
        reopened
            .edit_transforms(3, &[("part".into(), edited)])
            .unwrap();
        assert_eq!(
            reopened.scene().objects[0].mesh.as_deref(),
            Some("model-part-mesh")
        );
        assert_eq!(
            reopened.scene().objects[0].material.as_deref(),
            Some("model-material")
        );
    }
}
