use super::{
    prefab::Materialization, CameraView, Component, ComponentScene, ObjectHierarchy, ObjectNode,
    PrefabInstance, PrefabProperty, PrefabSource,
};
use crate::input::{self, ActionDefinition};
use crate::{
    BehaviorSource, BindingTarget, DisplayProfile, OpaqueMaterial, PlayerAnchor, PropertyValue,
    SceneError, SceneLock, Transform,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// In-memory editing state for a v23 scene. History is never serialized.
#[derive(Clone)]
pub struct ComponentSession {
    scene: ComponentScene,
    revision: u64,
    state: u64,
    saved_state: Option<u64>,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    saved: Option<SavedDocument>,
}

#[derive(Clone)]
struct SavedDocument {
    path: PathBuf,
    digest: [u8; 32],
    bytes: u64,
}

/// Stable identities assigned in one duplicate transaction.
pub struct DuplicatedObjects {
    pub root_ids: Vec<String>,
    pub object_ids: BTreeMap<String, String>,
    pub component_ids: BTreeMap<String, String>,
}

/// Identity of the bytes written by a successful scene publication.
pub struct SavedComponentScene {
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone)]
struct HistoryEntry {
    changes: Vec<SceneChange>,
    state: u64,
    prefab_file: Option<PrefabFileEdit>,
}

/// One guarded side effect grouped with its scene transaction. The editor
/// resolves the project asset ID before comparing and publishing bytes.
#[derive(Clone, Debug)]
pub struct PrefabFileEdit {
    pub asset: String,
    pub path_hint: String,
    pub before: Option<Arc<[u8]>>,
    pub after: Option<Arc<[u8]>>,
}

fn prefab_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn no_prefab_publisher<R>(_: &PrefabFileEdit) -> Result<R, SceneError> {
    Err(SceneError::Invalid(
        "Use prefab-aware history to edit the source file.".into(),
    ))
}

impl PrefabFileEdit {
    fn inverse(mut self) -> Self {
        std::mem::swap(&mut self.before, &mut self.after);
        self
    }
}

#[derive(Clone)]
enum SceneChange {
    Insert {
        index: usize,
        object: ObjectNode,
    },
    Remove {
        index: usize,
    },
    Replace {
        index: usize,
        object: ObjectNode,
    },
    InsertCopies(Vec<(usize, ObjectNode)>),
    RemoveCopies(Vec<usize>),
    Display(DisplayProfile),
    Hud(crate::hud::Hud),
    GameplayCamera(Option<String>),
    Action {
        id: String,
        value: Option<ActionDefinition>,
    },
    Material {
        id: String,
        value: Option<OpaqueMaterial>,
    },
    Source {
        id: String,
        value: Option<BehaviorSource>,
    },
    Prefab {
        root: String,
        value: Option<PrefabInstance>,
    },
}

impl SceneChange {
    fn apply(self, scene: &mut ComponentScene) -> Self {
        let objects = &mut scene.objects;
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
            Self::InsertCopies(copies) => {
                let mut original = std::mem::take(objects).into_iter();
                let mut removed = Vec::with_capacity(copies.len());
                objects.reserve(original.len() + copies.len());
                let mut consumed = 0;
                for (index, object) in copies {
                    objects.extend(original.by_ref().take(index + 1 - consumed));
                    consumed = index + 1;
                    removed.push(objects.len());
                    objects.push(object);
                }
                objects.extend(original);
                Self::RemoveCopies(removed)
            }
            Self::RemoveCopies(indices) => {
                let mut original = std::mem::take(objects).into_iter();
                let mut copies = Vec::with_capacity(indices.len());
                objects.reserve(original.len() - indices.len());
                let mut consumed = 0;
                for index in indices {
                    objects.extend(original.by_ref().take(index - consumed));
                    consumed = index + 1;
                    copies.push((objects.len() - 1, original.next().expect("copied object")));
                }
                objects.extend(original);
                Self::InsertCopies(copies)
            }
            Self::Hud(value) => Self::Hud(std::mem::replace(&mut scene.hud, value)),
            Self::Display(value) => Self::Display(std::mem::replace(&mut scene.display, value)),
            Self::GameplayCamera(value) => {
                Self::GameplayCamera(std::mem::replace(&mut scene.gameplay_camera, value))
            }
            Self::Action { id, value } => {
                let value = match value {
                    Some(action) => scene.actions.insert(id.clone(), action),
                    None => scene.actions.remove(&id),
                };
                Self::Action { id, value }
            }
            Self::Material { id, value } => {
                let previous = match value {
                    Some(material) => scene.materials.insert(id.clone(), material),
                    None => scene.materials.remove(&id),
                };
                Self::Material {
                    id,
                    value: previous,
                }
            }
            Self::Source { id, value } => {
                let previous = match value {
                    Some(source) => scene.sources.insert(id.clone(), source),
                    None => scene.sources.remove(&id),
                };
                Self::Source {
                    id,
                    value: previous,
                }
            }
            Self::Prefab { root, value } => {
                let previous = match value {
                    Some(instance) => scene.prefabs.insert(root.clone(), instance),
                    None => scene.prefabs.remove(&root),
                };
                Self::Prefab {
                    root,
                    value: previous,
                }
            }
        }
    }
}

fn apply_changes(
    changes: impl IntoIterator<Item = SceneChange>,
    scene: &mut ComponentScene,
) -> Vec<SceneChange> {
    let mut inverse: Vec<_> = changes
        .into_iter()
        .map(|change| change.apply(scene))
        .collect();
    inverse.reverse();
    inverse
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
        Self::from_open_file(&lock.path, File::open(&lock.path)?)
    }

    /// Decode a regular file opened by a caller that has pinned its pathname
    /// and identity. Later saves still verify the original content digest.
    pub fn from_open_file(path: &Path, file: File) -> Result<Self, SceneError> {
        if !path.is_absolute() || !file.metadata()?.is_file() {
            return Err(SceneError::Invalid(
                "Expected an absolute saved scene and a regular file.".into(),
            ));
        }
        let mut input = SceneInput {
            file,
            digest: Sha256::new(),
        };
        let scene = ComponentScene::read_json(&mut input)?;
        let bytes = input.file.stream_position()?;
        Ok(Self::from_saved(
            scene,
            SavedDocument {
                path: path.to_path_buf(),
                digest: input.digest.finalize().into(),
                bytes,
            },
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
        let (digest, bytes) = Self::publish(&scene, &lock.path, Some(expected))?;
        Ok(Self::from_saved(
            scene,
            SavedDocument {
                path: lock.path.clone(),
                digest,
                bytes,
            },
        ))
    }

    fn from_saved(scene: ComponentScene, saved: SavedDocument) -> Self {
        Self {
            scene,
            revision: 0,
            state: 0,
            saved_state: Some(0),
            undo: Vec::new(),
            redo: Vec::new(),
            saved: Some(saved),
        }
    }

    pub fn scene(&self) -> &ComponentScene {
        &self.scene
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn retained_huds(&self) -> impl Iterator<Item = &crate::hud::Hud> {
        self.undo
            .iter()
            .chain(&self.redo)
            .flat_map(|entry| &entry.changes)
            .filter_map(|change| match change {
                SceneChange::Hud(hud) => Some(hud),
                _ => None,
            })
    }

    pub fn allocate_hud_id(
        &self,
        panel: bool,
        draft: &crate::hud::Hud,
    ) -> Result<String, SceneError> {
        let mut used = std::collections::HashSet::new();
        for hud in [&self.scene.hud, draft]
            .into_iter()
            .chain(self.retained_huds())
        {
            if panel {
                used.extend(hud.panels.iter().map(|panel| panel.id.as_str()));
            } else {
                used.extend(hud.elements().map(|label| label.id.as_str()));
            }
        }
        crate::allocate_ids(if panel { "panel" } else { "label" }, 1, 1, used)
            .map(|mut ids| ids.remove(0))
    }

    pub fn set_hud(&mut self, expected: u64, hud: crate::hud::Hud) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        if self.scene.hud == hud {
            return Ok(self.revision);
        }
        for panel in &hud.panels {
            if !self.scene.hud.panels.iter().any(|old| old.id == panel.id)
                && self
                    .retained_huds()
                    .any(|old| old.panels.iter().any(|old| old.id == panel.id))
            {
                return Err(SceneError::Invalid(
                    "HUD panel ID is reserved by Undo history. Choose a fresh ID.".into(),
                ));
            }
        }
        for label in hud.elements() {
            if self.scene.hud.panel_for_label(&label.id).is_none()
                && self
                    .retained_huds()
                    .any(|old| old.panel_for_label(&label.id).is_some())
            {
                return Err(SceneError::Invalid(
                    "HUD label ID is reserved by Undo history. Choose a fresh ID.".into(),
                ));
            }
        }
        self.commit(vec![SceneChange::Hud(hud)])
    }

    pub fn set_action(
        &mut self,
        expected: u64,
        id: &str,
        value: Option<ActionDefinition>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        crate::validate_id(id)?;
        let previous = self.scene.actions.get(id);
        if previous == value.as_ref() {
            return Ok(self.revision);
        }
        if previous.is_none() && value.is_some() && self.contains_action_id(id) {
            return Err(SceneError::Invalid(
                "action identity is reserved by reversible history; Undo or choose a fresh ID"
                    .into(),
            ));
        }
        self.commit(vec![SceneChange::Action {
            id: id.into(),
            value,
        }])
    }

    fn action_ids(&self) -> impl Iterator<Item = &str> {
        self.scene.actions.keys().map(String::as_str).chain(
            self.undo
                .iter()
                .chain(&self.redo)
                .flat_map(|entry| &entry.changes)
                .filter_map(|change| match change {
                    SceneChange::Action { id, .. } => Some(id.as_str()),
                    _ => None,
                }),
        )
    }

    pub fn contains_action_id(&self, id: &str) -> bool {
        self.action_ids().any(|used| used == id)
    }

    pub fn allocate_action_ids(&self, count: usize) -> Result<Vec<String>, SceneError> {
        let next = self
            .revision
            .checked_add(1)
            .ok_or_else(|| SceneError::Invalid("revision exhausted".into()))?;
        crate::allocate_ids(
            &format!("action-{next}"),
            count,
            input::MAX_ACTIONS,
            self.action_ids().collect(),
        )
    }

    pub fn is_dirty(&self) -> bool {
        self.saved_state != Some(self.state)
    }

    /// Capture the exact bytes of the saved v23 document. The recorded length
    /// bounds the read even if the scene has since been replaced or grown.
    pub fn saved_bytes(&self) -> Result<Vec<u8>, SceneError> {
        self.with_saved_file(|saved, mut file| {
            let mut bytes = Vec::new();
            let len = usize::try_from(saved.bytes)
                .map_err(|_| SceneError::Invalid("The saved scene cannot fit in memory.".into()))?;
            bytes
                .try_reserve_exact(len)
                .map_err(|_| SceneError::Invalid("The saved scene cannot fit in memory.".into()))?;
            (&mut file)
                .take(saved.bytes.saturating_add(1))
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 != saved.bytes
                || <[u8; 32]>::from(Sha256::digest(&bytes)) != saved.digest
            {
                return Err(SceneError::Conflict);
            }
            Ok(bytes)
        })
    }

    /// Keep the saved document's cooperative lock through an adjacent publish
    /// after comparing the exact bytes without allocating a second copy.
    pub fn with_verified_saved<T>(&self, publish: impl FnOnce() -> T) -> Result<T, SceneError> {
        self.with_saved_file(|saved, file| {
            if digest(file.take(saved.bytes.saturating_add(1)))? != saved.digest {
                return Err(SceneError::Conflict);
            }
            Ok(publish())
        })
    }

    fn with_saved_file<T>(
        &self,
        read: impl FnOnce(&SavedDocument, File) -> Result<T, SceneError>,
    ) -> Result<T, SceneError> {
        let saved = self
            .saved
            .as_ref()
            .filter(|_| !self.is_dirty())
            .ok_or_else(|| SceneError::Invalid("Save the scene before building.".into()))?;
        let lock = SceneLock::acquire(&saved.path)?;
        let file = File::open(&lock.path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() != saved.bytes {
            return Err(SceneError::Conflict);
        }
        read(saved, file)
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
            Some(index) => SceneChange::Replace { index, object },
            None => SceneChange::Insert {
                index: self.scene.objects.len(),
                object,
            },
        };
        self.commit([change])
    }

    /// Apply related object edits together, with one revision and Undo entry.
    /// The input supplies local transforms and may refer to other objects in
    /// this edit. Each object ID may appear at most once.
    pub fn put_objects(
        &mut self,
        expected: u64,
        objects: Vec<ObjectNode>,
    ) -> Result<u64, SceneError> {
        self.put_objects_with_materials(expected, objects, BTreeMap::new())
    }

    /// Commit a hierarchy and its new material copies together.
    pub fn put_objects_with_materials(
        &mut self,
        expected: u64,
        objects: Vec<ObjectNode>,
        materials: BTreeMap<String, OpaqueMaterial>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let existing: HashMap<_, _> = self
            .scene
            .objects
            .iter()
            .enumerate()
            .map(|(index, object)| (object.id.as_str(), index))
            .collect();
        let mut seen = HashSet::with_capacity(objects.len());
        for object in &objects {
            if !seen.insert(object.id.as_str()) {
                return Err(SceneError::Invalid(format!(
                    "object {} occurs more than once in the edit",
                    object.id
                )));
            }
        }
        let mut end = self.scene.objects.len();
        let mut changes = Vec::new();
        for (id, material) in materials {
            if self.scene.materials.contains_key(&id) {
                return Err(SceneError::Invalid(format!("material {id} already exists")));
            }
            changes.push(SceneChange::Material {
                id,
                value: Some(material),
            });
        }
        for object in objects {
            match existing.get(object.id.as_str()).copied() {
                Some(index) if self.scene.objects[index] == object => {}
                Some(index) => changes.push(SceneChange::Replace { index, object }),
                None => {
                    changes.push(SceneChange::Insert { index: end, object });
                    end += 1;
                }
            }
        }
        if changes.is_empty() {
            return Ok(self.revision);
        }
        self.commit(changes)
    }

    /// Designate an existing Camera, or clear only the designation.
    pub fn set_gameplay_camera(
        &mut self,
        expected: u64,
        camera: Option<String>,
    ) -> Result<u64, SceneError> {
        self.edit_gameplay_camera(expected, camera, None)
    }

    /// Insert or update a camera owner and designate its camera in one edit.
    pub fn put_gameplay_camera(
        &mut self,
        expected: u64,
        camera: String,
        owner: ObjectNode,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        if !owner.components.iter().any(|component| {
            component.id == camera && matches!(component.kind, Component::Camera { .. })
        }) {
            return Err(SceneError::Invalid(
                "Camera owner does not contain the chosen Camera.".into(),
            ));
        }
        self.edit_gameplay_camera(expected, Some(camera), Some(owner))
    }

    fn edit_gameplay_camera(
        &mut self,
        expected: u64,
        camera: Option<String>,
        owner: Option<ObjectNode>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let mut changes = Vec::new();
        if let Some(object) = owner {
            match self
                .scene
                .objects
                .iter()
                .position(|node| node.id == object.id)
            {
                Some(index) if self.scene.objects[index] == object => {}
                Some(index) => changes.push(SceneChange::Replace { index, object }),
                None => changes.push(SceneChange::Insert {
                    index: self.scene.objects.len(),
                    object,
                }),
            }
        }
        if self.scene.gameplay_camera != camera {
            changes.push(SceneChange::GameplayCamera(camera));
        }
        if changes.is_empty() {
            Ok(self.revision)
        } else {
            self.commit(changes)
        }
    }

    /// Change scene-wide display settings as one reversible authoring edit.
    pub fn set_display(
        &mut self,
        expected: u64,
        display: DisplayProfile,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        display.validate()?;
        if display == self.scene.display {
            return Ok(self.revision);
        }
        self.commit([SceneChange::Display(display)])
    }

    /// Create or replace a scene-owned material, optionally assigning a new
    /// material to one exact Mesh Renderer as part of the same transaction.
    pub fn set_material(
        &mut self,
        expected: u64,
        id: &str,
        value: OpaqueMaterial,
        assign: Option<(&str, &str)>,
    ) -> Result<u64, SceneError> {
        self.set_material_binding(expected, id, value, assign, false)
    }

    /// Create and assign a unique definition, recording exact prefab renderer
    /// ownership when the destination is a linked part.
    pub fn set_unique_material(
        &mut self,
        expected: u64,
        id: &str,
        value: OpaqueMaterial,
        owner: &str,
        component: &str,
    ) -> Result<u64, SceneError> {
        self.set_material_binding(expected, id, value, Some((owner, component)), true)
    }

    fn set_material_binding(
        &mut self,
        expected: u64,
        id: &str,
        value: OpaqueMaterial,
        assign: Option<(&str, &str)>,
        unique: bool,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let previous = self.scene.materials.get(id);
        if assign.is_none() && previous == Some(&value) {
            return Ok(self.revision);
        }
        let mut changes = vec![SceneChange::Material {
            id: id.to_owned(),
            value: Some(value),
        }];
        if let Some((object_id, component_id)) = assign {
            if previous.is_some() {
                return Err(SceneError::Invalid(
                    "Choose a new material ID to create and assign.".into(),
                ));
            }
            let index = self
                .scene
                .objects
                .iter()
                .position(|object| object.id == object_id)
                .ok_or_else(|| SceneError::ObjectNotFound(object_id.to_owned()))?;
            let mut object = self.scene.objects[index].clone();
            match object
                .components
                .iter_mut()
                .find(|component| component.id == component_id)
                .map(|component| &mut component.kind)
            {
                Some(Component::MeshRenderer { material, .. }) => *material = Some(id.to_owned()),
                _ => {
                    return Err(SceneError::Invalid(
                        "Choose a Mesh Renderer to assign the material.".into(),
                    ))
                }
            }
            changes.push(SceneChange::Replace { index, object });
            if unique {
                if let Some((root, instance)) = self.scene.prefabs.iter().find(|(_, instance)| {
                    instance.parts.values().any(|part| part.object == object_id)
                }) {
                    let mut instance = instance.clone();
                    let part = instance
                        .parts
                        .values_mut()
                        .find(|part| part.object == object_id)
                        .expect("instance owns the renderer");
                    let source = part
                        .components
                        .iter()
                        .find_map(|(source, actual)| {
                            (actual == component_id).then_some(source.clone())
                        })
                        .ok_or_else(|| {
                            SceneError::Invalid("The prefab renderer is missing.".into())
                        })?;
                    part.material_overrides.insert(source, id.to_owned());
                    changes.push(SceneChange::Prefab {
                        root: root.clone(),
                        value: Some(instance),
                    });
                    return self.commit_with_file(changes, None, no_prefab_publisher);
                }
            }
        }
        self.commit(changes)
    }

    /// Copy selected subtrees as siblings in one history entry.
    pub fn duplicate_selection(
        &mut self,
        expected: u64,
        ids: &[&str],
    ) -> Result<DuplicatedObjects, SceneError> {
        self.check_revision(expected)?;
        if ids.is_empty() {
            return Err(SceneError::Invalid("Select an object to duplicate.".into()));
        }
        let objects = &self.scene.objects;
        let hierarchy = ObjectHierarchy::build(objects)?;
        let requested: HashSet<_> = ids.iter().copied().collect();
        let mut copied = vec![false; objects.len()];
        let mut roots = vec![false; objects.len()];
        let mut found = 0;
        for index in hierarchy.order {
            let selected = requested.contains(objects[index].id.as_str());
            found += usize::from(selected);
            let inherited = hierarchy.parents[index].is_some_and(|parent| copied[parent]);
            copied[index] = selected || inherited;
            roots[index] = selected && !inherited;
        }
        if found < requested.len() {
            let present: HashSet<_> = objects.iter().map(|object| object.id.as_str()).collect();
            let id = ids
                .iter()
                .find(|id| !present.contains(**id))
                .expect("missing selection");
            return Err(SceneError::ObjectNotFound((*id).into()));
        }
        let members: HashSet<_> = self
            .scene
            .prefabs
            .values()
            .flat_map(|instance| instance.parts.values().map(|part| part.object.as_str()))
            .collect();
        for (index, object) in objects
            .iter()
            .enumerate()
            .filter(|(index, _)| copied[*index])
        {
            if members.contains(object.id.as_str())
                || (roots[index]
                    && object
                        .parent
                        .as_deref()
                        .is_some_and(|id| members.contains(id)))
            {
                return Err(SceneError::Invalid(
                    "Linked prefab parts cannot be duplicated. Place another instance from Assets."
                        .into(),
                ));
            }
            if object.components.iter().any(|component| {
                matches!(&component.kind,
                Component::Script { binding, .. } if binding.target == BindingTarget::Application)
            }) {
                return Err(SceneError::Invalid(
                    "An Application script cannot be duplicated with an object.".into(),
                ));
            }
        }

        let mut used = HashSet::new();
        for object in objects {
            used.insert(object.id.as_str());
            for component in &object.components {
                used.insert(component.id.as_str());
                if let Component::Script { binding, .. } = &component.kind {
                    // Authoring permits an unresolved script reference. Do not
                    // accidentally give an existing dangling reference meaning.
                    used.extend(binding.properties.values().filter_map(|value| match value {
                        PropertyValue::ObjectRef(id) => id.as_deref(),
                        _ => None,
                    }));
                }
            }
        }
        let mut fresh = (1_u64..)
            .map(|n| format!("duplicate-{n}"))
            .filter(|id| !used.contains(id.as_str()));
        let mut result = DuplicatedObjects {
            root_ids: Vec::new(),
            object_ids: BTreeMap::new(),
            component_ids: BTreeMap::new(),
        };
        for (index, object) in objects
            .iter()
            .enumerate()
            .filter(|(index, _)| copied[*index])
        {
            let id = fresh.next().expect("finite scene IDs");
            if roots[index] {
                result.root_ids.push(id.clone());
            }
            result.object_ids.insert(object.id.clone(), id);
            for component in &object.components {
                result.component_ids.insert(
                    component.id.clone(),
                    fresh.next().expect("finite scene IDs"),
                );
            }
        }
        let remap = |id: &mut String| {
            if let Some(copy_id) = result.object_ids.get(id) {
                id.clone_from(copy_id);
            }
        };
        let copies = objects
            .iter()
            .enumerate()
            .filter(|(index, _)| copied[*index])
            .map(|(index, object)| {
                let mut copy = object.clone();
                copy.id.clone_from(&result.object_ids[&object.id]);
                if let Some(parent) = &mut copy.parent {
                    remap(parent);
                }
                for component in &mut copy.components {
                    component.id = result.component_ids[&component.id].clone();
                    match &mut component.kind {
                        Component::Camera {
                            view: CameraView::Orbit(view),
                        } => {
                            if let PlayerAnchor::Follow { target_id, .. } = &mut view.anchor {
                                remap(target_id);
                            }
                        }
                        Component::Script { binding, .. } => {
                            if let BindingTarget::Object(id) | BindingTarget::Group(id) =
                                &mut binding.target
                            {
                                remap(id);
                            }
                            for value in binding.properties.values_mut() {
                                if let PropertyValue::ObjectRef(Some(id)) = value {
                                    remap(id);
                                }
                            }
                        }
                        Component::Animator { binding } => {
                            binding.targets.values_mut().for_each(remap)
                        }
                        _ => {}
                    }
                }
                (index, copy)
            })
            .collect();
        self.commit([SceneChange::InsertCopies(copies)])?;
        Ok(result)
    }

    /// Move an object while keeping its world matrix. Reject a move whose
    /// local matrix cannot be represented by an authored transform.
    pub fn reparent(
        &mut self,
        expected: u64,
        id: &str,
        parent: Option<&str>,
    ) -> Result<u64, SceneError> {
        self.reparent_many(expected, &[id], parent)
    }

    /// Reparent the topmost objects in a selection as one Undo entry.
    pub fn reparent_many(
        &mut self,
        expected: u64,
        ids: &[&str],
        parent: Option<&str>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let mut indices = self.selection_roots(ids)?;
        indices.retain(|&index| self.scene.objects[index].parent.as_deref() != parent);
        if indices.is_empty() {
            return Ok(self.revision);
        }
        let moved = self.with_parent(&indices, parent)?;
        let changes = indices
            .into_iter()
            .zip(moved)
            .map(|(index, object)| SceneChange::Replace { index, object });
        self.commit(changes)
    }

    /// Insert an identity parent around the topmost selected objects in file order.
    pub fn group_selection(
        &mut self,
        expected: u64,
        id: &str,
        name: &str,
        ids: &[&str],
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let mut indices = self.selection_roots(ids)?;
        indices.sort_unstable();
        let Some(&first) = indices.first() else {
            return Err(SceneError::Invalid("Select an object to group.".into()));
        };
        let objects = &self.scene.objects;
        let first_parent = &objects[first].parent;
        let parent = if indices
            .iter()
            .all(|&index| &objects[index].parent == first_parent)
        {
            first_parent.as_deref()
        } else {
            None
        };
        let moved = self
            .with_parent(&indices, parent)
            .map_err(|_| SceneError::CannotPreserveWorldTransform)?;
        if indices
            .iter()
            .any(|&index| objects[index].parent.as_deref() != parent)
        {
            // A decomposition error small enough for a root can be magnified
            // by descendants. Prove the whole affected subtree before editing.
            let hierarchy = ObjectHierarchy::build(objects)?;
            let mut world = self
                .scene
                .world_matrices()
                .map_err(|_| SceneError::CannotPreserveWorldTransform)?;
            let mut changed = vec![false; objects.len()];
            let roots: HashMap<_, _> = indices.iter().copied().zip(&moved).collect();
            for index in hierarchy.order {
                let matrix = if let Some(root) = roots.get(&index) {
                    crate::hierarchy::Pose::from_local(&root.transform).matrix()
                } else if let Some(parent) = hierarchy.parents[index].filter(|&p| changed[p]) {
                    world[parent]
                        * crate::hierarchy::Pose::from_local(&objects[index].transform).matrix()
                } else {
                    continue;
                };
                if !crate::hierarchy::close(world[index], matrix) {
                    return Err(SceneError::CannotPreserveWorldTransform);
                }
                world[index] = matrix;
                changed[index] = true;
            }
        }
        let changes = indices.into_iter().zip(moved).map(|(index, mut object)| {
            object.parent = Some(id.into());
            SceneChange::Replace { index, object }
        });
        let group = ObjectNode {
            id: id.into(),
            name: name.into(),
            parent: parent.map(str::to_owned),
            enabled: true,
            transform: Transform::default(),
            components: Vec::new(),
        };
        self.commit(changes.chain([SceneChange::Insert {
            index: first,
            object: group,
        }]))
    }

    /// Place an object immediately before or after a sibling in file order.
    /// If needed, adopt the sibling's parent while preserving the world pose.
    pub fn place_next_to(
        &mut self,
        expected: u64,
        id: &str,
        sibling: &str,
        before: bool,
    ) -> Result<u64, SceneError> {
        self.place_many_next_to(expected, &[id], sibling, before)
    }

    /// Place the topmost selected objects beside an anchor, in selection order.
    pub fn place_many_next_to(
        &mut self,
        expected: u64,
        ids: &[&str],
        sibling: &str,
        before: bool,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let anchor = self.object_index(sibling)?;
        let indices = self.selection_roots(ids)?;
        if indices.is_empty() || ids.contains(&sibling) {
            return Ok(self.revision);
        }
        let objects = &self.scene.objects;
        let parent = objects[anchor].parent.as_deref();
        // A group already beside the anchor in the requested order is a no-op.
        let adjacent = if before {
            let mut siblings = (0..anchor)
                .rev()
                .filter(|&index| objects[index].parent.as_deref() == parent);
            indices
                .iter()
                .rev()
                .all(|&index| siblings.next() == Some(index))
        } else {
            let mut siblings = (anchor + 1..objects.len())
                .filter(|&index| objects[index].parent.as_deref() == parent);
            indices.iter().all(|&index| siblings.next() == Some(index))
        };
        if adjacent {
            return Ok(self.revision);
        }

        let moved = self.with_parent(&indices, parent)?;
        let destination =
            anchor + usize::from(!before) - indices.iter().filter(|&&index| index < anchor).count();
        let mut from_indices = indices;
        from_indices.sort_unstable();
        let remove = from_indices
            .into_iter()
            .rev()
            .map(|index| SceneChange::Remove { index });
        let insert = moved
            .into_iter()
            .enumerate()
            .map(|(offset, object)| SceneChange::Insert {
                index: destination + offset,
                object,
            });
        self.commit(remove.chain(insert))
    }

    fn selection_roots(&self, ids: &[&str]) -> Result<Vec<usize>, SceneError> {
        if let [id] = ids {
            return Ok(vec![self.object_index(id)?]);
        }
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let objects = &self.scene.objects;
        let indices: HashMap<_, _> = objects
            .iter()
            .enumerate()
            .map(|(index, object)| (object.id.as_str(), index))
            .collect();
        let hierarchy = ObjectHierarchy::build(objects)?;
        let mut covered = vec![false; objects.len()];
        let mut ordered = Vec::with_capacity(ids.len());
        for id in ids {
            let index = indices
                .get(id)
                .copied()
                .ok_or_else(|| SceneError::ObjectNotFound((*id).into()))?;
            if !std::mem::replace(&mut covered[index], true) {
                ordered.push(index);
            }
        }
        let mut below_selection = vec![false; objects.len()];
        for index in hierarchy.order {
            below_selection[index] = hierarchy.parents[index].is_some_and(|parent| covered[parent]);
            covered[index] |= below_selection[index];
        }
        ordered.retain(|&index| !below_selection[index]);
        Ok(ordered)
    }

    fn with_parent(
        &self,
        indices: &[usize],
        parent: Option<&str>,
    ) -> Result<Vec<ObjectNode>, SceneError> {
        let objects = &self.scene.objects;
        let changed = indices
            .iter()
            .any(|&index| objects[index].parent.as_deref() != parent);
        let world = changed.then(|| self.scene.world_matrices()).transpose()?;
        let parent_inverse = match (&world, parent) {
            (Some(world), Some(parent)) => world[self.object_index(parent)?].inverse(),
            _ => glam::DMat4::IDENTITY,
        };
        indices
            .iter()
            .map(|&index| {
                let mut object = objects[index].clone();
                if let Some(world) = world
                    .as_ref()
                    .filter(|_| object.parent.as_deref() != parent)
                {
                    object.transform = Transform::try_from_affine(parent_inverse * world[index])
                        .map_err(|error| {
                            SceneError::Invalid(format!(
                        "cannot preserve the world transform under the requested parent: {error}"
                    ))
                        })?;
                    object.parent = parent.map(str::to_owned);
                }
                Ok(object)
            })
            .collect()
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
        self.commit([SceneChange::Remove { index }])
    }

    /// Remove the object and its descendants in one edit. References from
    /// outside the subtree still have to validate after removal.
    pub fn remove_subtree(&mut self, expected: u64, id: &str) -> Result<u64, SceneError> {
        self.remove_subtrees(expected, &[id])
    }

    /// Delete the selected subtrees as one edit, even when they overlap.
    pub fn remove_subtrees(&mut self, expected: u64, ids: &[&str]) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let roots = self.selection_roots(ids)?;
        if roots.is_empty() {
            return Ok(self.revision);
        }
        let hierarchy = ObjectHierarchy::build(&self.scene.objects)?;
        let mut remove = vec![false; self.scene.objects.len()];
        for index in roots {
            remove[index] = true;
        }
        for index in hierarchy.order {
            remove[index] |= hierarchy.parents[index].is_some_and(|parent| remove[parent]);
        }
        let changes = remove
            .into_iter()
            .enumerate()
            .rev()
            .filter_map(|(index, remove)| remove.then_some(SceneChange::Remove { index }));
        self.commit(changes)
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

    fn prefab_object_edits(&self, changes: &[SceneChange]) -> Result<Vec<SceneChange>, SceneError> {
        let new_objects: HashMap<_, _> = changes
            .iter()
            .filter_map(|change| match change {
                SceneChange::Insert { object, .. } | SceneChange::Replace { object, .. } => {
                    Some((object.id.as_str(), object))
                }
                _ => None,
            })
            .collect();
        let owners: HashMap<_, _> = self
            .scene
            .prefabs
            .iter()
            .flat_map(|(root, instance)| {
                instance
                    .parts
                    .values()
                    .map(move |part| (part.object.as_str(), root.as_str()))
            })
            .collect();
        let mut links = BTreeMap::new();
        for index in changes.iter().filter_map(|change| match change {
            SceneChange::Remove { index } | SceneChange::Replace { index, .. } => Some(*index),
            _ => None,
        }) {
            let old = &self.scene.objects[index];
            let Some(&root) = owners.get(old.id.as_str()) else {
                continue;
            };
            if let Some(new) = new_objects.get(old.id.as_str()) {
                let link = links
                    .entry(root)
                    .or_insert_with(|| Some(self.scene.prefabs[root].clone()));
                if let Some(instance) = link {
                    instance.record_object_edit(old, new)?;
                }
            } else if old.id == root {
                links.insert(root, None);
            }
        }
        Ok(links
            .into_iter()
            .filter_map(|(root, value)| {
                (value.as_ref() != self.scene.prefabs.get(root)).then(|| SceneChange::Prefab {
                    root: root.to_owned(),
                    value,
                })
            })
            .collect())
    }

    /// Save a selection as the first instance without changing its placement.
    /// The publisher runs after scene validation and before history is advanced.
    pub fn link_prefab(
        &mut self,
        expected: u64,
        asset: String,
        path_hint: String,
        source: &PrefabSource,
        publish: impl FnOnce(&PrefabFileEdit) -> Result<(), SceneError>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let root = source.root();
        if &PrefabSource::from_selection(&self.scene, root)? != source {
            return Err(SceneError::Conflict);
        }
        let current = self.prefab_objects();
        let bytes = source.bytes()?;
        let instance = source.new_instance(root, asset.clone(), prefab_hash(&bytes), true);
        let materialized = source.materialize(root, instance, &current, None)?;
        let changes = self.prefab_changes(vec![(root, materialized)], true)?;
        self.commit_with_file(
            changes,
            Some(PrefabFileEdit {
                asset,
                path_hint,
                before: None,
                after: Some(bytes.into()),
            }),
            publish,
        )
    }

    /// Place another linked copy of a prefab from the accepted project source.
    #[allow(clippy::too_many_arguments)]
    pub fn place_prefab(
        &mut self,
        expected: u64,
        asset: String,
        hash: String,
        source: &PrefabSource,
        root: &str,
        transform: Transform,
        parent: Option<String>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let current = self.prefab_objects();
        if current.contains_key(root) {
            return Err(SceneError::Invalid(format!(
                "Object ID {root} already exists."
            )));
        }
        let instance = source.new_instance(root, asset, hash, false);
        let materialized =
            source.materialize(root, instance, &current, Some((transform, parent)))?;
        let changes = self.prefab_changes(vec![(root, materialized)], false)?;
        self.commit_with_file(changes, None, no_prefab_publisher)
    }

    pub fn refresh_prefab(
        &mut self,
        expected: u64,
        asset: &str,
        hash: &str,
        source: &PrefabSource,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let changes = self.prefab_refresh_changes(asset, hash, source, None)?;
        if changes.is_empty() {
            return Ok(self.revision);
        }
        let revision = self.commit_with_file(changes, None, no_prefab_publisher)?;
        // An externally changed source is the new baseline. Historical
        // indexed object snapshots predate that source and may refer to
        // removed parts. Keep the accepted scene and start new history.
        self.undo.clear();
        self.redo.clear();
        Ok(revision)
    }

    pub fn revert_prefab_override(
        &mut self,
        expected: u64,
        root: &str,
        object: &str,
        property: &PrefabProperty,
        source: &PrefabSource,
        hash: &str,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let mut instance = self.prefab_instance(root, hash)?.clone();
        instance.take_override(object, property)?;
        let current = self.prefab_objects();
        let materialized = source.materialize(root, instance, &current, None)?;
        let changes = self.prefab_changes(vec![(root, materialized)], false)?;
        self.commit_with_file(changes, None, no_prefab_publisher)
    }

    /// Apply one property from an instance to the source and update every copy.
    /// The publisher must compare the original bytes and replace the file
    /// atomically. A failed publication rolls the scene change back.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_prefab_override(
        &mut self,
        expected: u64,
        root: &str,
        object: &str,
        property: &PrefabProperty,
        source_bytes: Arc<[u8]>,
        path_hint: String,
        publish: impl FnOnce(&PrefabFileEdit) -> Result<(), SceneError>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected)?;
        let mut instance = self
            .prefab_instance(root, &prefab_hash(&source_bytes))?
            .clone();
        let source = PrefabSource::read_json(&source_bytes)?;
        let (source_id, source_property) = instance.take_override(object, property)?;
        let object = &self.scene.objects[self.object_index(object)?];
        let edited = source.with_override(
            &source_id,
            source_property,
            object,
            property,
            &self.scene.materials,
        )?;
        let bytes = edited.bytes()?;
        let digest = prefab_hash(&bytes);
        let asset = instance.asset.clone();
        let changes =
            self.prefab_refresh_changes(&asset, &digest, &edited, Some((root, instance)))?;
        self.commit_with_file(
            changes,
            Some(PrefabFileEdit {
                asset,
                path_hint,
                before: Some(source_bytes),
                after: Some(bytes.into()),
            }),
            publish,
        )
    }

    fn prefab_instance(&self, root: &str, hash: &str) -> Result<&PrefabInstance, SceneError> {
        let instance = self
            .scene
            .prefabs
            .get(root)
            .ok_or_else(|| SceneError::Invalid("Choose a prefab instance.".into()))?;
        if instance.source_hash != hash {
            return Err(SceneError::Conflict);
        }
        Ok(instance)
    }

    fn prefab_objects(&self) -> HashMap<&str, &ObjectNode> {
        self.scene
            .objects
            .iter()
            .map(|object| (object.id.as_str(), object))
            .collect()
    }

    fn prefab_refresh_changes(
        &self,
        asset: &str,
        hash: &str,
        source: &PrefabSource,
        changed: Option<(&str, PrefabInstance)>,
    ) -> Result<Vec<SceneChange>, SceneError> {
        let current = self.prefab_objects();
        let updates = self
            .scene
            .prefabs
            .iter()
            .filter(|(root, instance)| {
                instance.asset == asset
                    && (instance.source_hash != hash
                        || changed.as_ref().is_some_and(|(other, _)| *root == other))
            })
            .map(|(root, instance)| {
                let mut instance = changed
                    .as_ref()
                    .filter(|(other, _)| root.as_str() == *other)
                    .map_or_else(|| instance.clone(), |(_, value)| value.clone());
                hash.clone_into(&mut instance.source_hash);
                Ok((
                    root.as_str(),
                    source.materialize(root, instance, &current, None)?,
                ))
            })
            .collect::<Result<Vec<_>, SceneError>>()?;
        self.prefab_changes(updates, false)
    }

    fn prefab_changes(
        &self,
        updates: Vec<(&str, Materialization)>,
        link_original: bool,
    ) -> Result<Vec<SceneChange>, SceneError> {
        if updates.is_empty() {
            return Ok(Vec::new());
        }
        let indices: HashMap<_, _> = self
            .scene
            .objects
            .iter()
            .enumerate()
            .map(|(i, object)| (object.id.as_str(), i))
            .collect();
        let mut changes = Vec::new();
        let mut remove = Vec::new();
        let mut insert = Vec::new();
        for (root, update) in updates {
            let old = self.scene.prefabs.get(root);
            if link_original && old.is_some() {
                return Err(SceneError::Invalid(
                    "This object already belongs to a prefab.".into(),
                ));
            }
            let previous_ids: HashSet<_> = old
                .into_iter()
                .flat_map(|instance| instance.parts.values())
                .map(|part| part.object.as_str())
                .collect();
            let retained: HashSet<_> = update
                .objects
                .iter()
                .map(|object| object.id.as_str())
                .collect();
            for id in previous_ids.difference(&retained) {
                remove.push(indices[id]);
            }
            for object in update.objects {
                if let Some(&index) = indices.get(object.id.as_str()) {
                    if !link_original && !previous_ids.contains(object.id.as_str()) {
                        return Err(SceneError::Invalid(format!(
                            "Prefab object ID {} already exists.",
                            object.id
                        )));
                    }
                    if self.scene.objects[index] != object {
                        changes.push(SceneChange::Replace { index, object });
                    }
                } else {
                    insert.push(object);
                }
            }
            for (id, value) in update.materials {
                if let Some(previous) = self.scene.materials.get(&id) {
                    if old.is_none_or(|instance| {
                        !instance.materials.values().any(|existing| existing == &id)
                    }) {
                        return Err(SceneError::Invalid(format!(
                            "Prefab material ID {id} already exists."
                        )));
                    }
                    if previous == &value {
                        continue;
                    }
                }
                changes.push(SceneChange::Material {
                    id,
                    value: Some(value),
                });
            }
            for (id, value) in update.sources {
                if let Some(previous) = self.scene.sources.get(&id) {
                    if old.is_none_or(|instance| {
                        !instance.sources.values().any(|existing| existing == &id)
                    }) {
                        return Err(SceneError::Invalid(format!(
                            "Prefab script source ID {id} already exists."
                        )));
                    }
                    if previous == &value {
                        continue;
                    }
                }
                changes.push(SceneChange::Source {
                    id,
                    value: Some(value),
                });
            }
            changes.push(SceneChange::Prefab {
                root: root.to_owned(),
                value: Some(update.instance),
            });
        }
        remove.sort_unstable_by(|a, b| b.cmp(a));
        let end = self.scene.objects.len() - remove.len();
        changes.extend(
            remove
                .into_iter()
                .map(|index| SceneChange::Remove { index }),
        );
        changes.extend(insert.into_iter().enumerate().map(|(offset, object)| {
            SceneChange::Insert {
                index: end + offset,
                object,
            }
        }));
        Ok(changes)
    }

    fn commit(
        &mut self,
        changes: impl IntoIterator<Item = SceneChange>,
    ) -> Result<u64, SceneError> {
        let mut changes: Vec<_> = changes.into_iter().collect();
        if !self.scene.prefabs.is_empty() {
            changes.extend(self.prefab_object_edits(&changes)?);
        }
        self.commit_with_file(changes, None, no_prefab_publisher)
    }

    fn commit_with_file(
        &mut self,
        changes: impl IntoIterator<Item = SceneChange>,
        file: Option<PrefabFileEdit>,
        publish: impl FnOnce(&PrefabFileEdit) -> Result<(), SceneError>,
    ) -> Result<u64, SceneError> {
        let next = self
            .revision
            .checked_add(1)
            .ok_or(SceneError::RevisionOverflow)?;
        let mut inverse = apply_changes(changes, &mut self.scene);
        let mut released = HashSet::new();
        for change in &inverse {
            if let SceneChange::Prefab {
                root,
                value: Some(old),
            } = change
            {
                let current = self.scene.prefabs.get(root);
                for (part_id, part) in &old.parts {
                    let current_part = current.and_then(|instance| instance.parts.get(part_id));
                    for (component, material) in &part.material_overrides {
                        if current_part.and_then(|part| part.material_overrides.get(component))
                            != Some(material)
                        {
                            released.insert(material.clone());
                        }
                    }
                }
            }
        }
        if !released.is_empty() {
            for id in self
                .scene
                .objects
                .iter()
                .flat_map(|object| &object.components)
                .filter_map(|component| match &component.kind {
                    Component::MeshRenderer { material, .. } => material.as_ref(),
                    _ => None,
                })
                .chain(self.scene.prefabs.values().flat_map(|instance| {
                    instance
                        .parts
                        .values()
                        .flat_map(|part| part.material_overrides.values())
                }))
            {
                released.remove(id);
            }
            let cleanup = released
                .into_iter()
                .filter(|id| self.scene.materials.contains_key(id))
                .map(|id| SceneChange::Material { id, value: None })
                .collect::<Vec<_>>();
            inverse.splice(..0, apply_changes(cleanup, &mut self.scene));
        }
        if self.scene.gameplay_camera.as_deref().is_some_and(|id| {
            !inverse
                .iter()
                .any(|change| matches!(change, SceneChange::GameplayCamera(_)))
                && inverse.iter().any(|change| match change {
                    SceneChange::Insert { object, .. } | SceneChange::Replace { object, .. } => {
                        object.components.iter().any(|component| {
                            component.id == id && matches!(component.kind, Component::Camera { .. })
                        })
                    }
                    _ => false,
                })
                && self.scene.designated_camera().is_none()
        }) {
            inverse.insert(
                0,
                SceneChange::GameplayCamera(self.scene.gameplay_camera.take()),
            );
        }
        if let Err(error) = self
            .scene
            .validate()
            .and_then(|_| file.as_ref().map_or(Ok(()), publish))
        {
            for change in inverse {
                change.apply(&mut self.scene);
            }
            return Err(error);
        }
        self.undo.push(HistoryEntry {
            changes: inverse,
            state: self.state,
            prefab_file: file.map(PrefabFileEdit::inverse),
        });
        self.redo.clear();
        self.state = next;
        self.revision = next;
        Ok(next)
    }

    /// Whether the next history command changes the authored input map.
    pub fn action_history_pending(&self, undo: bool) -> bool {
        let history = if undo { &self.undo } else { &self.redo };
        history.last().is_some_and(|entry| {
            entry
                .changes
                .iter()
                .any(|change| matches!(change, SceneChange::Action { .. }))
        })
    }

    pub fn undo(&mut self, expected: u64) -> Result<u64, SceneError> {
        self.move_history(expected, true, no_prefab_publisher)
    }

    pub fn redo(&mut self, expected: u64) -> Result<u64, SceneError> {
        self.move_history(expected, false, no_prefab_publisher)
    }

    pub fn move_history(
        &mut self,
        expected: u64,
        undo: bool,
        publish: impl FnOnce(&PrefabFileEdit) -> Result<String, SceneError>,
    ) -> Result<u64, SceneError> {
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
        let resolved_path = from
            .last()
            .ok_or(empty)?
            .prefab_file
            .as_ref()
            .map(publish)
            .transpose()?;
        let mut entry = from.pop().expect("history entry checked");
        if let (Some(file), Some(path)) = (&mut entry.prefab_file, resolved_path) {
            file.path_hint = path;
        }
        to.push(HistoryEntry {
            changes: apply_changes(entry.changes, &mut self.scene),
            state: std::mem::replace(&mut self.state, entry.state),
            prefab_file: entry.prefab_file.map(PrefabFileEdit::inverse),
        });
        self.revision = next;
        Ok(next)
    }

    pub fn path(&self) -> Option<&Path> {
        self.saved.as_ref().map(|saved| saved.path.as_path())
    }

    /// Write one current scene under the cooperative lock. Saving preserves the
    /// in-memory history. A new destination must not exist.
    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        self.save_recorded(path).map(|_| ())
    }

    pub fn save_recorded(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<SavedComponentScene, SceneError> {
        let lock = SceneLock::acquire(path.as_ref())?;
        let baseline = self
            .saved
            .as_ref()
            .filter(|saved| saved.path == lock.path)
            .map(|saved| saved.digest);
        let (digest, bytes) = Self::publish(&self.scene, &lock.path, baseline)?;
        Ok(self.record_saved(lock.path.clone(), digest, bytes))
    }

    /// Capture the current authored state for background save preparation.
    #[cfg(unix)]
    pub fn active_save(&self, expected_path: &Path) -> Result<ComponentSave, SceneError> {
        let saved = self
            .saved
            .as_ref()
            .filter(|saved| saved.path == expected_path)
            .ok_or(SceneError::Conflict)?;
        Ok(ComponentSave {
            path: saved.path.clone(),
            baseline: saved.digest,
            revision: self.revision,
            state: self.state,
            scene: self.scene.clone(),
        })
    }

    /// Publish verified, fsynced bytes under the existing cooperative lock.
    /// Reject changes to the authored state, association or disk baseline.
    #[cfg(unix)]
    pub fn publish_active_save(
        &mut self,
        prepared: PreparedComponentSave,
        authorize: impl FnOnce() -> Result<(), SceneError>,
    ) -> Result<SavedComponentScene, SceneError> {
        if self.revision != prepared.revision
            || self.state != prepared.state
            || self.saved.as_ref().is_none_or(|saved| {
                saved.path != prepared.path || saved.digest != prepared.baseline
            })
        {
            return Err(SceneError::Conflict);
        }
        let lock = SceneLock::acquire(&prepared.path)?;
        if lock.path != prepared.path {
            return Err(SceneError::Conflict);
        }
        authorize()?;
        if disk_stamp(&lock.path)? != prepared.stamp {
            return Err(SceneError::Conflict);
        }
        crate::publish_scene_write(prepared.temporary, &lock.path, false)?;
        Ok(self.record_saved(lock.path.clone(), prepared.digest, prepared.bytes))
    }

    fn record_saved(&mut self, path: PathBuf, digest: [u8; 32], bytes: u64) -> SavedComponentScene {
        self.saved = Some(SavedDocument {
            path,
            digest,
            bytes,
        });
        self.saved_state = Some(self.state);
        SavedComponentScene {
            sha256: format!("{:x}", sha2::digest::Output::<Sha256>::from(digest)),
            bytes,
        }
    }

    fn write_snapshot(
        scene: &ComponentScene,
        writer: &mut File,
    ) -> Result<([u8; 32], u64), SceneError> {
        {
            let mut buffered = BufWriter::new(&mut *writer);
            scene.write_json(&mut buffered)?;
            buffered.flush()?;
        }
        let bytes = writer.stream_position()?;
        writer.rewind()?;
        Ok((digest(writer)?, bytes))
    }

    fn publish(
        scene: &ComponentScene,
        path: &Path,
        baseline: Option<[u8; 32]>,
    ) -> Result<([u8; 32], u64), SceneError> {
        let (temporary, result) =
            crate::prepare_scene_write(path, |writer| Self::write_snapshot(scene, writer))?;
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
        crate::publish_scene_write(temporary, path, baseline.is_none())?;
        Ok(result)
    }
}

#[cfg(unix)]
pub struct ComponentSave {
    path: PathBuf,
    baseline: [u8; 32],
    revision: u64,
    state: u64,
    scene: ComponentScene,
}

#[cfg(unix)]
pub struct PreparedComponentSave {
    path: PathBuf,
    baseline: [u8; 32],
    revision: u64,
    state: u64,
    stamp: DiskStamp,
    temporary: tempfile::NamedTempFile,
    digest: [u8; 32],
    bytes: u64,
}

#[cfg(unix)]
impl ComponentSave {
    /// Stage and sync the new file, then verify the original bytes. The disk
    /// stamp lets publication reject later changes without re-reading the file.
    pub fn prepare(self) -> Result<PreparedComponentSave, SceneError> {
        let (temporary, (prepared_digest, bytes)) =
            crate::prepare_scene_write(&self.path, |writer| {
                ComponentSession::write_snapshot(&self.scene, writer)
            })?;
        let stamp = disk_stamp(&self.path)?;
        let current = File::open(&self.path)?;
        if DiskStamp::regular(&current.metadata()?)? != stamp
            || digest(&current)? != self.baseline
            || DiskStamp::regular(&current.metadata()?)? != stamp
            || disk_stamp(&self.path)? != stamp
        {
            return Err(SceneError::Conflict);
        }
        Ok(PreparedComponentSave {
            path: self.path,
            baseline: self.baseline,
            revision: self.revision,
            state: self.state,
            stamp,
            temporary,
            digest: prepared_digest,
            bytes,
        })
    }
}

#[cfg(unix)]
#[derive(PartialEq, Eq)]
struct DiskStamp {
    identity: (u64, u64),
    bytes: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

#[cfg(unix)]
impl DiskStamp {
    fn regular(metadata: &std::fs::Metadata) -> Result<Self, SceneError> {
        use std::os::unix::fs::MetadataExt;
        if !metadata.is_file() {
            return Err(SceneError::Conflict);
        }
        Ok(Self {
            identity: (metadata.dev(), metadata.ino()),
            bytes: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }
}

#[cfg(unix)]
fn disk_stamp(path: &Path) -> Result<DiskStamp, SceneError> {
    DiskStamp::regular(&path.symlink_metadata()?)
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
    use crate::{hierarchy::close, Scene, Session};

    #[test]
    fn duplicate_selection_remaps_a_whole_assembly_and_keeps_independent_history() {
        use crate::{AnimationBinding, PlayerView};
        let mut scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("shot")).unwrap()).unwrap();
        // File order need not be topological. Both copies must keep sibling order.
        for (id, parent) in [
            ("wheel", Some("rig")),
            ("sign", Some("rig")),
            ("rig", Some("stage")),
            ("marker", Some("stage")),
            ("stage", None),
        ] {
            scene.objects.push(ObjectNode {
                id: id.into(),
                name: id.into(),
                enabled: true,
                parent: parent.map(str::to_owned),
                transform: Transform {
                    translation: [1., 2., 3.],
                    ..Default::default()
                },
                components: Vec::new(),
            });
        }
        let component = |id: &str, kind| crate::component_scene::ComponentInstance {
            id: id.into(),
            enabled: true,
            kind,
        };
        scene.objects[2].components = vec![
            component(
                "rig-animator",
                Component::Animator {
                    binding: AnimationBinding {
                        clip: "bridge-lift".into(),
                        looping: true,
                        targets: BTreeMap::from([("Rig/Wheel".into(), "wheel".into())]),
                    },
                },
            ),
            component(
                "rig-camera",
                Component::Camera {
                    view: CameraView::Orbit(PlayerView {
                        anchor: PlayerAnchor::Follow {
                            target_id: "marker".into(),
                            offset: [0.; 3],
                        },
                        yaw: 0.,
                        pitch: 0.,
                        distance: 5.,
                        vertical_fov_degrees: 45.,
                    }),
                },
            ),
            component(
                "rig-script",
                Component::Script {
                    binding: crate::BehaviorBinding {
                        target: BindingTarget::Object("rig".into()),
                        source_id: "script".into(),
                        enabled: true,
                        properties: BTreeMap::from([
                            ("self".into(), PropertyValue::ObjectRef(Some("rig".into()))),
                            (
                                "outside".into(),
                                PropertyValue::ObjectRef(Some("stage".into())),
                            ),
                            (
                                "missing".into(),
                                PropertyValue::ObjectRef(Some("duplicate-1".into())),
                            ),
                        ]),
                        state_defaults: Default::default(),
                        commands: Default::default(),
                    },
                    source_kind: None,
                    movement: Default::default(),
                },
            ),
        ];
        scene.gameplay_camera = Some("rig-camera".into());
        let mut session = ComponentSession::new(scene.clone()).unwrap();
        assert!(session.duplicate_selection(0, &["rig", "missing"]).is_err());
        assert_eq!(session.scene(), &scene);
        let result = session
            .duplicate_selection(0, &["marker", "wheel", "rig"])
            .unwrap();
        let ids = &result.object_ids;
        assert_eq!(result.root_ids, [ids["rig"].clone(), ids["marker"].clone()]);
        assert_eq!(ids.len(), 4);
        assert!(!ids.values().any(|id| id == "duplicate-1"));
        let objects = &session.scene().objects;
        assert_eq!(
            objects
                .iter()
                .map(|object| object.id.as_str())
                .collect::<Vec<_>>(),
            [
                "wheel",
                &ids["wheel"],
                "sign",
                &ids["sign"],
                "rig",
                &ids["rig"],
                "marker",
                &ids["marker"],
                "stage"
            ]
        );
        assert_eq!(objects[1].parent.as_deref(), Some(ids["rig"].as_str()));
        assert_eq!(objects[5].transform, scene.objects[2].transform);
        assert_eq!(objects[5].parent.as_deref(), Some("stage"));
        let [animator, camera, script] = objects[5].components.as_slice() else {
            panic!("copied components")
        };
        assert_eq!(animator.id, result.component_ids["rig-animator"]);
        let Component::Animator { binding } = &animator.kind else {
            panic!("copied animator")
        };
        assert_eq!(binding.targets["Rig/Wheel"], ids["wheel"]);
        assert!(matches!(&camera.kind, Component::Camera { view }
            if view.followed_target() == Some(ids["marker"].as_str())));
        assert_eq!(
            session.scene().gameplay_camera.as_deref(),
            Some("rig-camera")
        );
        let Component::Script { binding, .. } = &script.kind else {
            panic!("copied script")
        };
        assert_eq!(binding.target, BindingTarget::Object(ids["rig"].clone()));
        assert_eq!(
            binding.properties["self"],
            PropertyValue::ObjectRef(Some(ids["rig"].clone()))
        );
        assert_eq!(
            binding.properties["outside"],
            PropertyValue::ObjectRef(Some("stage".into()))
        );
        let copied = session.scene().clone();
        assert_eq!(session.undo(1).unwrap(), 2);
        assert_eq!(session.scene(), &scene);
        assert_eq!(session.redo(2).unwrap(), 3);
        assert_eq!(session.scene(), &copied);
        let mut edited = copied.objects[1].clone();
        edited.transform.translation[0] = 9.;
        session.put_object(3, edited).unwrap();
        assert_eq!(session.scene().objects[0], scene.objects[0]);
        assert_eq!(session.scene().objects[1].transform.translation[0], 9.);
    }

    #[test]
    fn game_camera_designation_is_atomic_and_survives_suspension_and_undo() {
        use crate::component_scene::{Component, ComponentInstance};
        use crate::{PlayerAnchor, PlayerView};
        let root = ObjectNode {
            id: "rig".into(),
            name: "Rig".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: Vec::new(),
        };
        let mut scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("shot")).unwrap()).unwrap();
        scene.objects.push(root.clone());
        let mut session = ComponentSession::new(scene).unwrap();
        let view = PlayerView {
            anchor: PlayerAnchor::Fixed {
                position: [3., 2., 1.],
            },
            yaw: 0.2,
            pitch: 0.12,
            distance: 9.,
            vertical_fov_degrees: 45.,
        };
        let camera = ObjectNode {
            id: "camera-owner".into(),
            name: "Camera".into(),
            parent: Some(root.id.clone()),
            enabled: true,
            transform: Transform::default(),
            components: vec![ComponentInstance {
                id: "shot".into(),
                enabled: true,
                kind: Component::Camera {
                    view: view.clone().into(),
                },
            }],
        };
        assert_eq!(
            session
                .put_gameplay_camera(0, "shot".into(), camera.clone())
                .unwrap(),
            1
        );
        assert_eq!(
            session
                .put_gameplay_camera(1, "shot".into(), camera.clone())
                .unwrap(),
            1
        );
        let active = |session: &ComponentSession, expected: bool| {
            let scene = session.scene();
            assert_eq!(scene.gameplay_camera.as_deref(), Some("shot"));
            assert_eq!(
                scene
                    .effective_camera(&scene.validate().unwrap())
                    .map(|(_, view)| view.clone()),
                expected.then(|| view.clone().into())
            );
        };
        active(&session, true);
        let mut disabled = root.clone();
        disabled.enabled = false;
        session.put_object(1, disabled).unwrap();
        active(&session, false);
        session.undo(2).unwrap();
        active(&session, true);
        session.remove_subtree(3, "rig").unwrap();
        assert!(session.scene().gameplay_camera.is_none());
        session.undo(4).unwrap();
        active(&session, true);
        let mut changed = camera.clone();
        changed.components[0].kind = Component::Collider {
            role: crate::ColliderRole::Blocker,
            offset: [0.; 3],
            shape: crate::ColliderShape::default(),
        };
        session.put_object(5, changed).unwrap();
        assert!(session.scene().gameplay_camera.is_none());
        session.undo(6).unwrap();
        active(&session, true);
        session.set_gameplay_camera(7, None).unwrap();
        assert!(session.scene().gameplay_camera.is_none());
        assert_eq!(session.scene().objects, [root, camera]);
    }

    #[test]
    fn group_selection_preserves_hierarchy_world_pose_and_history() {
        let mut scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("group")).unwrap()).unwrap();
        scene.objects = [
            ("stage", None, [2., 0., 1.], [0., 0.7, 0.], [2., 1., 1.]),
            ("a", Some("stage"), [1., 0., 0.], [0., 0.3, 0.], [1.; 3]),
            ("leaf", Some("a"), [0., 1., 0.], [0.; 3], [1.; 3]),
            ("b", Some("stage"), [2., 0., 0.], [0.; 3], [1.; 3]),
            ("free", None, [-2., 0., 1.], [0.; 3], [1.; 3]),
        ]
        .into_iter()
        .map(|(id, parent, translation, rotation, scale)| ObjectNode {
            id: id.into(),
            name: id.into(),
            enabled: true,
            parent: parent.map(str::to_owned),
            transform: Transform {
                translation,
                rotation,
                scale,
            },
            components: Vec::new(),
        })
        .collect();
        let world = scene.world_matrices().unwrap();
        for (members, parent, ordered_members) in [
            (vec!["free", "stage"], None, vec!["stage", "free"]),
            (vec!["b", "leaf", "a", "a"], Some("stage"), vec!["a", "b"]),
            (vec!["free", "b"], None, vec!["b", "free"]),
        ] {
            let mut session = ComponentSession::new(scene.clone()).unwrap();
            assert!(matches!(
                session.group_selection(1, "new-group", "Group", &members),
                Err(SceneError::StaleRevision { .. })
            ));
            assert_eq!(
                session
                    .group_selection(0, "new-group", "Group", &members)
                    .unwrap(),
                1
            );
            let grouped = session.scene().clone();
            let node = grouped
                .objects
                .iter()
                .find(|node| node.id == "new-group")
                .unwrap();
            assert_eq!(node.parent.as_deref(), parent);
            assert_eq!(node.transform, Transform::default());
            assert!(node.components.is_empty());
            assert_eq!(
                grouped
                    .objects
                    .iter()
                    .filter(|node| node.parent.as_deref() == Some("new-group"))
                    .map(|node| node.id.as_str())
                    .collect::<Vec<_>>(),
                ordered_members,
            );
            for ((node, got), (original, want)) in grouped
                .objects
                .iter()
                .zip(grouped.world_matrices().unwrap())
                .filter(|(node, _)| node.id != "new-group")
                .zip(scene.objects.iter().zip(&world))
            {
                assert_eq!(node.id, original.id);
                assert!(close(*want, got), "world pose changed on {}", node.id);
                if parent.is_some() {
                    assert_eq!(node.transform, original.transform);
                }
                if node.id == "leaf" {
                    assert_eq!(node.parent.as_deref(), Some("a"));
                }
            }
            assert_eq!(session.undo(1).unwrap(), 2);
            assert_eq!(session.scene(), &scene);
            assert_eq!(session.redo(2).unwrap(), 3);
            assert_eq!(session.scene(), &grouped);
            let folder = tempfile::tempdir().unwrap();
            let path = folder.path().join("group.scene.json");
            session.save(&path).unwrap();
            assert_eq!(ComponentSession::open(&path).unwrap().scene(), &grouped);
        }

        let mut session = ComponentSession::new(scene.clone()).unwrap();
        let refused = session.group_selection(0, "new-group", "Group", &["free", "a"]);
        assert!(matches!(
            refused,
            Err(SceneError::CannotPreserveWorldTransform)
        ));
        assert_eq!(session.revision(), 0);
        assert_eq!(session.scene(), &scene);
        assert!(matches!(session.undo(0), Err(SceneError::NoUndo)));
    }

    #[test]
    fn placing_beside_a_sibling_reorders_and_reparents_in_one_edit() {
        let mut scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("order")).unwrap()).unwrap();
        scene.objects = [
            ("stage", None),
            ("red", Some("stage")),
            ("blue", None),
            ("green", Some("stage")),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (id, parent))| ObjectNode {
            id: id.into(),
            name: id.into(),
            enabled: true,
            parent: parent.map(str::to_owned),
            transform: Transform {
                translation: [index as f32 + 1.0, 0.0, 0.0],
                ..Transform::default()
            },
            components: Vec::new(),
        })
        .collect();
        let mut session = ComponentSession::new(scene.clone()).unwrap();
        let original_world = scene.world_matrices().unwrap();
        assert_eq!(session.place_next_to(0, "red", "green", true).unwrap(), 0);
        assert_eq!(session.place_next_to(0, "blue", "green", false).unwrap(), 1);
        let ordered = session.scene().clone();
        assert_eq!(
            ordered
                .objects
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            ["stage", "red", "green", "blue"]
        );
        assert_eq!(ordered.objects[3].parent.as_deref(), Some("stage"));
        assert!(close(
            ordered.world_matrices().unwrap()[3],
            original_world[2]
        ));

        assert!(session.place_next_to(1, "stage", "red", true).is_err());
        assert_eq!(session.revision(), 1);
        assert_eq!(session.scene(), &ordered);
        session.undo(1).unwrap();
        assert_eq!(session.scene(), &scene);
        session.redo(2).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        session.save(&path).unwrap();
        assert_eq!(ComponentSession::open(path).unwrap().scene(), &ordered);
    }

    #[test]
    fn grouped_hierarchy_edits_keep_selected_descendants_attached() {
        let mut scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("group")).unwrap()).unwrap();
        scene.objects = [
            ("stage", None),
            ("car", None),
            ("wheel", Some("car")),
            ("marker", None),
        ]
        .into_iter()
        .map(|(id, parent)| ObjectNode {
            id: id.into(),
            name: id.into(),
            enabled: true,
            parent: parent.map(str::to_owned),
            transform: Transform::default(),
            components: Vec::new(),
        })
        .collect();
        let mut session = ComponentSession::new(scene.clone()).unwrap();
        let selected = ["wheel", "car", "marker"];
        assert_eq!(
            session.reparent_many(0, &selected, Some("stage")).unwrap(),
            1
        );
        assert_eq!(session.scene().objects[2].parent.as_deref(), Some("car"));
        assert_eq!(session.scene().objects[3].parent.as_deref(), Some("stage"));
        let nested = session.scene().clone();
        assert!(session
            .reparent_many(1, &["stage", "marker"], Some("wheel"))
            .is_err());
        assert_eq!(session.scene(), &nested);
        assert_eq!(session.revision(), 1);
        session.undo(1).unwrap();
        assert_eq!(session.scene(), &scene);
        session.redo(2).unwrap();
        assert_eq!(session.remove_subtrees(3, &selected).unwrap(), 4);
        assert_eq!(session.scene().objects, [scene.objects[0].clone()]);
        session.undo(4).unwrap();
        assert_eq!(session.scene(), &nested);
    }

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
    fn grouped_edits_admit_the_final_hierarchy_and_restore_order() {
        let scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        let mut session = ComponentSession::new(scene.clone()).unwrap();
        let root = ObjectNode {
            id: "root".into(),
            name: "Root".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: Vec::new(),
        };
        let mut left = scene.objects[0].clone();
        left.parent = Some(root.id.clone());
        let mut right = scene.objects[2].clone();
        right.parent = Some(root.id.clone());
        assert!(session
            .put_objects(0, vec![left.clone(), right.clone()])
            .is_err());
        assert_eq!(session.scene(), &scene);
        assert_eq!(session.revision(), 0);
        assert!(matches!(session.undo(0), Err(SceneError::NoUndo)));

        assert_eq!(session.put_objects(0, vec![left, right, root]).unwrap(), 1);
        let grouped = session.scene().clone();
        assert_eq!(session.undo(1).unwrap(), 2);
        assert_eq!(session.scene(), &scene);
        assert_eq!(session.redo(2).unwrap(), 3);
        assert_eq!(session.scene(), &grouped);

        assert_eq!(session.remove_subtree(3, "root").unwrap(), 4);
        assert_eq!(session.scene().objects, [scene.objects[1].clone()]);
        assert_eq!(session.undo(4).unwrap(), 5);
        assert_eq!(session.scene(), &grouped);
        assert_eq!(session.redo(5).unwrap(), 6);
        assert_eq!(session.scene().objects, [scene.objects[1].clone()]);
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
