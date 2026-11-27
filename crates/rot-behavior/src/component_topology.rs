//! Runtime-only population changes for persistent Lua callbacks.
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use rot_scene::component_scene::{Component, ComponentScene, ObjectHierarchy, ObjectNode};
use rot_scene::{PropertyValue, Transform};

/// Separate limits for queued and committed runtime objects plus components.
/// Authored population is outside this budget. Static templates stay bounded
/// even when Lua can issue many native calls under its instruction limit.
const MAX_RUNTIME_ROWS: usize = 100_000;

#[derive(Debug)]
pub(super) struct ObjectSession {
    authored: RefCell<Arc<ComponentScene>>,
    authored_count: usize,
    hierarchy: Arc<ObjectHierarchy>,
    next_id: Cell<u64>,
    children: OnceCell<Vec<Vec<usize>>>,
    reserved: OnceCell<HashSet<String>>,
}

impl ObjectSession {
    pub(super) fn new(authored: Arc<ComponentScene>, hierarchy: Arc<ObjectHierarchy>) -> Self {
        let authored_count = authored.objects.len();
        Self {
            authored: RefCell::new(authored),
            authored_count,
            hierarchy,
            next_id: Cell::new(1),
            children: OnceCell::new(),
            reserved: OnceCell::new(),
        }
    }

    pub(super) fn set_authored(&self, scene: Arc<ComponentScene>) {
        debug_assert!(scene.objects.len() >= self.authored_count);
        *self.authored.borrow_mut() = scene;
    }

    pub(super) fn authored_count(&self) -> usize {
        self.authored_count
    }

    fn reserve_ids(&self) -> &HashSet<String> {
        self.reserved.get_or_init(|| {
            let authored = self.authored.borrow();
            let mut ids = HashSet::new();
            for object in &authored.objects {
                ids.insert(object.id.clone());
                for component in &object.components {
                    ids.insert(component.id.clone());
                    if let Component::Script { binding, .. } = &component.kind {
                        for value in binding.properties.values() {
                            if let PropertyValue::ObjectRef(Some(id))
                            | PropertyValue::SensorRef(id) = value
                            {
                                ids.insert(id.clone());
                            }
                        }
                    }
                }
            }
            ids
        })
    }

    pub(super) fn spawn(
        &self,
        template: usize,
        pose: Transform,
        parent: Option<&str>,
        remaining: usize,
    ) -> Result<Vec<ObjectNode>, String> {
        let authored = self.authored.borrow();
        let children = self.children.get_or_init(|| {
            let mut children = vec![Vec::new(); self.authored_count];
            for (index, &parent) in self.hierarchy.parents.iter().enumerate() {
                if let Some(parent) = parent {
                    children[parent].push(index);
                }
            }
            children
        });
        let mut indices = vec![template];
        let mut current = 0;
        let mut rows = 0;
        while let Some(&index) = indices.get(current) {
            let object = &authored.objects[index];
            rows += 1 + object.components.len();
            if rows > remaining || indices.len() + children[index].len() > remaining {
                return Err(format!("runtime spawn queue permits at most {MAX_RUNTIME_ROWS} objects and components combined per tick"));
            }
            for component in &object.components {
                let kind = match component.kind {
                    Component::Script { .. } => "Script",
                    Component::Camera { .. } => "Camera",
                    Component::Animator { .. } => "Animator",
                    Component::Joint { .. } => "Joint",
                    Component::CharacterController { .. } => "CharacterController",
                    Component::MeshRenderer { .. }
                    | Component::Light { .. }
                    | Component::Collider { .. }
                    | Component::RigidBody { .. } => continue,
                };
                return Err(format!(
                    "cannot spawn template containing {kind} {} on {}",
                    component.id, object.id
                ));
            }
            indices.extend_from_slice(&children[index]);
            current += 1;
        }

        let reserved = self.reserve_ids();
        let nonce = loop {
            let nonce = self.next_id.get();
            self.next_id
                .set(nonce.checked_add(1).ok_or("runtime object IDs exhausted")?);
            let collision = indices.iter().enumerate().any(|(row, &index)| {
                let object_id = format!("rot-spawn-{nonce}-o{row}");
                reserved.contains(&object_id)
                    || (0..authored.objects[index].components.len())
                        .any(|column| reserved.contains(&format!("{object_id}-c{column}")))
            });
            if !collision {
                break nonce;
            }
        };
        let mut assigned = HashMap::<usize, String>::with_capacity(indices.len());
        let mut objects = Vec::with_capacity(indices.len());
        for (row, index) in indices.into_iter().enumerate() {
            let mut object = authored.objects[index].clone();
            object.id = format!("rot-spawn-{nonce}-o{row}");
            for (column, component) in object.components.iter_mut().enumerate() {
                component.id = format!("{}-c{column}", object.id);
            }
            if index == template {
                object.parent = parent.map(str::to_owned);
                object.transform = pose.clone();
                object.enabled = true;
            } else {
                let parent = self.hierarchy.parents[index].expect("template descendant parent");
                object.parent = Some(assigned[&parent].clone());
            }
            assigned.insert(index, object.id.clone());
            objects.push(object);
        }
        Ok(objects)
    }
}

struct Spawn {
    objects: Vec<ObjectNode>,
    binding: String,
}

#[derive(Default)]
pub(super) struct TopologyEdits {
    spawns: Vec<Spawn>,
    pending: HashSet<String>,
    destroyed: HashSet<String>,
    attributed: usize,
    queued_rows: usize,
}

pub(super) struct Population {
    pub(super) scene: ComponentScene,
    pub(super) local: Vec<Transform>,
    /// The originating callback for each appended subtree, in scene order.
    pub(super) origins: Vec<(Range<usize>, String)>,
}

impl TopologyEdits {
    pub(super) fn is_empty(&self) -> bool {
        self.spawns.is_empty() && self.destroyed.is_empty()
    }

    pub(super) fn is_pending(&self, id: &str) -> bool {
        self.pending.contains(id)
    }

    pub(super) fn remaining_queue_rows(&self) -> usize {
        MAX_RUNTIME_ROWS - self.queued_rows
    }

    pub(super) fn spawn(&mut self, objects: Vec<ObjectNode>) {
        self.queued_rows += objects
            .iter()
            .map(|object| 1 + object.components.len())
            .sum::<usize>();
        self.pending.insert(objects[0].id.clone());
        self.spawns.push(Spawn {
            objects,
            binding: String::new(),
        });
    }

    pub(super) fn destroy(&mut self, id: &str) {
        self.destroyed.insert(id.to_owned());
    }

    pub(super) fn attribute(&mut self, binding: &str) {
        for spawn in &mut self.spawns[self.attributed..] {
            spawn.binding = binding.to_owned();
        }
        self.attributed = self.spawns.len();
    }

    pub(super) fn materialize(
        self,
        scene: &ComponentScene,
        hierarchy: &ObjectHierarchy,
        object_indices: &HashMap<String, usize>,
        local: &[Transform],
        authored_count: usize,
    ) -> Result<Option<Population>, (String, String)> {
        if self.is_empty() {
            return Ok(None);
        }
        let mut removed = Vec::new();
        let mut removed_count = 0;
        if self
            .destroyed
            .iter()
            .any(|id| object_indices.contains_key(id))
        {
            removed.resize(scene.objects.len(), false);
            for &index in &hierarchy.order {
                removed[index] = self.destroyed.contains(&scene.objects[index].id)
                    || hierarchy.parents[index].is_some_and(|parent| removed[parent]);
                removed_count += usize::from(removed[index]);
            }
        }
        let mut accepted_roots = HashSet::new();
        let mut additions = Vec::new();
        let mut origins = Vec::new();
        let mut live_rows = scene
            .objects
            .iter()
            .enumerate()
            .skip(authored_count)
            .filter(|(index, _)| !removed.get(*index).copied().unwrap_or(false))
            .map(|(_, object)| 1 + object.components.len())
            .sum::<usize>();
        for Spawn { objects, binding } in self.spawns {
            let root = &objects[0];
            if self.destroyed.contains(&root.id) {
                continue;
            }
            if let Some(parent) = &root.parent {
                let stays = object_indices
                    .get(parent)
                    .is_some_and(|&index| !removed.get(index).copied().unwrap_or(false));
                if !stays && !accepted_roots.contains(parent) {
                    return Err((
                        binding,
                        format!("cannot spawn under removed parent {parent:?}"),
                    ));
                }
            }
            live_rows += objects
                .iter()
                .map(|object| 1 + object.components.len())
                .sum::<usize>();
            if live_rows > MAX_RUNTIME_ROWS {
                return Err((binding, format!("runtime population permits at most {MAX_RUNTIME_ROWS} spawned objects and components combined")));
            }
            accepted_roots.insert(root.id.clone());
            let start = scene.objects.len() - removed_count + additions.len();
            origins.push((start..start + objects.len(), binding));
            additions.extend(objects);
        }
        if removed_count == 0 && additions.is_empty() {
            return Ok(None);
        }

        let mut scene = scene.clone();
        let mut kept = Vec::with_capacity(scene.objects.len() - removed_count + additions.len());
        let mut index = 0;
        scene.objects.retain(|_| {
            let keep = !removed.get(index).copied().unwrap_or(false);
            if keep {
                kept.push(local[index].clone());
            }
            index += 1;
            keep
        });
        kept.extend(additions.iter().map(|object| object.transform.clone()));
        scene.objects.extend(additions);
        Ok(Some(Population {
            scene,
            local: kept,
            origins,
        }))
    }
}
