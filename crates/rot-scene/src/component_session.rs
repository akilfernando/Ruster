use super::{ComponentScene, ObjectNode};
use crate::{
    hierarchy::{close, Pose},
    SceneError, SceneLock, Transform,
};
use glam::EulerRot;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};

/// In-memory editing state for a v23 scene. History is never serialized.
pub struct ComponentSession {
    scene: ComponentScene,
    revision: u64,
    state: u64,
    saved_state: Option<u64>,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    saved: Option<(PathBuf, [u8; 32])>,
}

struct HistoryEntry {
    change: ObjectChange,
    state: u64,
}

enum ObjectChange {
    Insert { index: usize, object: ObjectNode },
    Remove { index: usize },
    Replace { index: usize, object: ObjectNode },
}

impl ObjectChange {
    fn apply(self, objects: &mut Vec<ObjectNode>) -> Self {
        match self {
            Self::Insert { index, object } => {
                objects.insert(index, object);
                Self::Remove { index }
            }
            Self::Remove { index } => Self::Insert {
                index,
                object: objects.remove(index),
            },
            Self::Replace { index, object } => Self::Replace {
                index,
                object: std::mem::replace(&mut objects[index], object),
            },
        }
    }
}

impl ComponentSession {
    pub fn new(scene: ComponentScene) -> Result<Self, SceneError> {
        scene.validate()?;
        Ok(Self {
            scene,
            revision: 0,
            state: 0,
            saved_state: None,
            undo: Vec::new(),
            redo: Vec::new(),
            saved: None,
        })
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, SceneError> {
        let lock = SceneLock::acquire(path.as_ref())?;
        let mut input = SceneInput {
            file: File::open(&lock.path)?,
            digest: Sha256::new(),
        };
        let scene = ComponentScene::read_json(&mut input)?;
        Ok(Self::from_saved(
            scene,
            lock.path.clone(),
            input.digest.finalize().into(),
        ))
    }

    /// Upgrade a legacy document in place once its consumers support v23.
    /// A failed conversion, interrupted write or detected file change leaves
    /// the source intact. Callers opening a scene for inspection use a reader.
    pub fn upgrade_legacy(path: impl AsRef<Path>) -> Result<Self, SceneError> {
        let lock = SceneLock::acquire(path.as_ref())?;
        let mut bytes = Vec::new();
        File::open(&lock.path)?
            .take(crate::MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        let legacy = crate::Session::from_snapshot_bytes(&bytes)?;
        let scene = ComponentScene::from_legacy(&legacy)?;
        let expected = Sha256::digest(&bytes).into();
        let hash = Self::publish(&scene, &lock.path, Some(expected))?;
        Ok(Self::from_saved(scene, lock.path.clone(), hash))
    }

    fn from_saved(scene: ComponentScene, path: PathBuf, digest: [u8; 32]) -> Self {
        Self {
            scene,
            revision: 0,
            state: 0,
            saved_state: Some(0),
            undo: Vec::new(),
            redo: Vec::new(),
            saved: Some((path, digest)),
        }
    }

    pub fn scene(&self) -> &ComponentScene {
        &self.scene
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_dirty(&self) -> bool {
        self.saved_state != Some(self.state)
    }

    /// Insert a new object or replace the authored data for an existing ID.
    /// Relationships are local. Supply an updated local transform if preserving
    /// a world pose during reparenting.
    pub fn put_object(&mut self, expected: u64, object: ObjectNode) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let index = self
            .scene
            .objects
            .iter()
            .position(|node| node.id == object.id);
        let change = match index {
            Some(index) if self.scene.objects[index] == object => return Ok(self.revision),
            Some(index) => ObjectChange::Replace { index, object },
            None => ObjectChange::Insert {
                index: self.scene.objects.len(),
                object,
            },
        };
        self.commit(change)
    }

    /// Move an object while keeping its world matrix. Reject a move whose
    /// local matrix cannot be represented by an authored transform.
    pub fn reparent(
        &mut self,
        expected: u64,
        id: &str,
        parent: Option<&str>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let index = self.object_index(id)?;
        let mut object = self.scene.objects[index].clone();
        if object.parent.as_deref() == parent {
            return Ok(self.revision);
        }
        let world = self.scene.world_matrices()?;
        let local = match parent {
            Some(parent) => world[self.object_index(parent)?].inverse() * world[index],
            None => world[index],
        };
        let determinant = local.determinant();
        let unrepresentable = || {
            SceneError::Invalid(
                "cannot preserve the world transform under the requested parent".into(),
            )
        };
        if !determinant.is_finite() || determinant <= 0.0 {
            return Err(unrepresentable());
        }
        let (scale, rotation, translation) = local.to_scale_rotation_translation();
        let (z, y, x) = rotation.to_euler(EulerRot::ZYX);
        let transform = Transform {
            translation: translation.as_vec3().to_array(),
            rotation: [x as f32, y as f32, z as f32],
            scale: scale.as_vec3().to_array(),
        };
        transform.validate()?;
        if !close(local, Pose::from_local(&transform).matrix()) {
            return Err(unrepresentable());
        }
        object.transform = transform;
        object.parent = parent.map(str::to_owned);
        self.commit(ObjectChange::Replace { index, object })
    }

    fn object_index(&self, id: &str) -> Result<usize, SceneError> {
        self.scene
            .objects
            .iter()
            .position(|node| node.id == id)
            .ok_or_else(|| SceneError::ObjectNotFound(id.into()))
    }

    /// Remove an object whose removal leaves all scene references valid.
    pub fn remove_object(&mut self, expected: u64, id: &str) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let index = self.object_index(id)?;
        self.commit(ObjectChange::Remove { index })
    }

    fn check_revision(&self, expected: u64) -> Result<(), SceneError> {
        if expected == self.revision {
            Ok(())
        } else {
            Err(SceneError::StaleRevision {
                expected,
                actual: self.revision,
            })
        }
    }

    fn commit(&mut self, change: ObjectChange) -> Result<u64, SceneError> {
        let next = self
            .revision
            .checked_add(1)
            .ok_or(SceneError::RevisionOverflow)?;
        let inverse = change.apply(&mut self.scene.objects);
        if let Err(error) = self.scene.validate() {
            inverse.apply(&mut self.scene.objects);
            return Err(error);
        }
        self.undo.push(HistoryEntry {
            change: inverse,
            state: self.state,
        });
        self.redo.clear();
        self.state = next;
        self.revision = next;
        Ok(next)
    }

    pub fn undo(&mut self, expected: u64) -> Result<u64, SceneError> {
        self.move_history(expected, true)
    }

    pub fn redo(&mut self, expected: u64) -> Result<u64, SceneError> {
        self.move_history(expected, false)
    }

    fn move_history(&mut self, expected: u64, undo: bool) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let next = self
            .revision
            .checked_add(1)
            .ok_or(SceneError::RevisionOverflow)?;
        let (from, to, empty) = if undo {
            (&mut self.undo, &mut self.redo, SceneError::NoUndo)
        } else {
            (&mut self.redo, &mut self.undo, SceneError::NoRedo)
        };
        let entry = from.pop().ok_or(empty)?;
        to.push(HistoryEntry {
            change: entry.change.apply(&mut self.scene.objects),
            state: std::mem::replace(&mut self.state, entry.state),
        });
        self.revision = next;
        Ok(next)
    }

    /// Write one current scene under the cooperative lock. Saving preserves the
    /// in-memory history. A new destination must not exist.
    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        let lock = SceneLock::acquire(path.as_ref())?;
        let baseline = self
            .saved
            .as_ref()
            .filter(|(path, _)| *path == lock.path)
            .map(|(_, digest)| *digest);
        let digest = Self::publish(&self.scene, &lock.path, baseline)?;
        self.saved = Some((lock.path.clone(), digest));
        self.saved_state = Some(self.state);
        Ok(())
    }

    fn publish(
        scene: &ComponentScene,
        path: &Path,
        baseline: Option<[u8; 32]>,
    ) -> Result<[u8; 32], SceneError> {
        crate::write_scene_atomically(path, baseline.is_none(), |writer| {
            scene.write_json(&mut *writer)?;
            writer.flush()?;
            writer.rewind()?;
            let prepared_digest = digest(writer)?;
            if let Some(saved) = baseline {
                crate::reject_nonregular(path)?;
                let current = match File::open(path) {
                    Ok(file) => file,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        return Err(SceneError::Conflict);
                    }
                    Err(error) => return Err(error.into()),
                };
                if digest(current)? != saved {
                    return Err(SceneError::Conflict);
                }
            }
            Ok(prepared_digest)
        })
    }
}

struct SceneInput {
    file: File,
    digest: Sha256,
}

impl Read for SceneInput {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let len = self.file.read(bytes)?;
        self.digest.update(&bytes[..len]);
        Ok(len)
    }
}

fn digest(mut input: impl Read) -> io::Result<[u8; 32]> {
    let mut hash = Sha256::new();
    io::copy(&mut input, &mut hash)?;
    Ok(hash.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Scene, Session};

    #[test]
    fn guarded_session_saves_reopens_and_restores_savepoint() {
        let scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        let mut session = ComponentSession::new(scene).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let parent = ObjectNode {
            id: "root".into(),
            name: "Root".into(),
            enabled: true,
            parent: None,
            transform: Transform {
                translation: [2.0, 1.0, 0.0],
                scale: [2.0; 3],
                ..Transform::default()
            },
            components: Vec::new(),
        };
        assert_eq!(session.put_object(0, parent.clone()).unwrap(), 1);
        let world = session.scene().world_matrices().unwrap()[0];
        assert!(matches!(
            session.reparent(0, "left", Some("root")),
            Err(SceneError::StaleRevision { .. })
        ));
        assert_eq!(session.reparent(1, "left", Some("root")).unwrap(), 2);
        assert!(close(world, session.scene().world_matrices().unwrap()[0]));

        assert!(session.reparent(2, "root", Some("left")).is_err());
        assert!(session.remove_object(2, "root").is_err());
        assert_eq!(session.revision(), 2);
        session.save(&path).unwrap();
        let saved = std::fs::read(&path).unwrap();
        assert!(!session.is_dirty());

        assert_eq!(session.undo(2).unwrap(), 3);
        assert!(session.is_dirty());
        assert_eq!(session.scene().objects[0].parent, None);
        assert_eq!(session.redo(3).unwrap(), 4);
        assert!(!session.is_dirty());
        session.save(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        let mut reopened = ComponentSession::open(&path).unwrap();
        assert_eq!(reopened.scene(), session.scene());
        assert!(!reopened.is_dirty());
        assert!(matches!(reopened.undo(0), Err(SceneError::NoUndo)));

        let mut changed = saved;
        changed.push(b' ');
        std::fs::write(&path, &changed).unwrap();
        assert!(matches!(reopened.save(&path), Err(SceneError::Conflict)));
        assert_eq!(std::fs::read(&path).unwrap(), changed);
    }

    #[test]
    fn upgrade_replaces_only_supported_unchanged_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.json");
        let mut legacy = Session::new(Scene::demo()).unwrap();
        legacy.save(&path).unwrap();

        let expected = ComponentScene::from_legacy(&legacy).unwrap();
        let session = ComponentSession::upgrade_legacy(&path).unwrap();
        assert_eq!(session.scene(), &expected);
        assert_eq!(ComponentSession::open(&path).unwrap().scene(), &expected);

        let current = std::fs::read(&path).unwrap();
        let newer = String::from_utf8(current.clone())
            .unwrap()
            .replace("\"format_version\": 23", "\"format_version\": 24");
        for unsupported in [&current[..], newer.as_bytes(), b"{"] {
            std::fs::write(&path, unsupported).unwrap();
            assert!(ComponentSession::upgrade_legacy(&path).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), unsupported);
        }
    }

    #[test]
    fn world_preserving_move_rejects_shear() {
        let scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        let mut session = ComponentSession::new(scene).unwrap();
        let parent = ObjectNode {
            id: "root".into(),
            name: "Root".into(),
            enabled: true,
            parent: None,
            transform: Transform {
                rotation: [0.0, 0.0, 0.7],
                scale: [2.0, 1.0, 1.0],
                ..Transform::default()
            },
            components: Vec::new(),
        };
        session.put_object(0, parent).unwrap();
        assert!(session.reparent(1, "left", Some("root")).is_err());
        assert_eq!(session.revision(), 1);
        assert_eq!(session.scene().objects[0].parent, None);
    }
}
