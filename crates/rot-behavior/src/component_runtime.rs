//! Native flat-array runtime for `ComponentScene`, retiring projection caps.
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use glam::{DMat4, DQuat};
use rot_scene::component_scene::{
    AnimatorStart, Component, ComponentScene, ObjectHierarchy, ScriptFile, ScriptMovement,
};
use rot_scene::input::{ActionFrame, ActionValue, ValidatedActionFrame};
use rot_scene::{Authoring, BehaviorSource, BindingTarget, Clip, ComponentDrawPlan, Transform};

use super::component_topology::{ObjectSession, Population, TopologyEdits};
use super::lua_component::{
    self, CurrentCallback, LightIntensities, Module, ModuleProbe, Modules, ObjectLookup,
};
use super::spatial::{world_collider_transform, ComponentColliderSlot};
use super::spatial_host::{kinematic_owner_component, ComponentQueryInput};
use super::{
    audio, lifecycle, same_transform, BindingOutput, Diagnostic, DiagnosticCode, Engine, HostInput,
    Phase, ResolveSnapshot, Runtime, RuntimeState, GROUP_ROTATION_HOST_API_VERSION,
    KINEMATIC_HOST_API_VERSION, LIFECYCLE_HOST_API_VERSION, STATE_HOST_API_VERSION,
};

#[derive(Clone, Debug)]
pub(super) struct ComponentScriptSlot {
    pub(super) component_id: String,
    pub(super) owner_index: usize,
    pub(super) owner_id: String,
    pub(super) has_object_input: bool,
    movement: ScriptMovement,
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedComponentAnimation {
    pub(super) clip: Arc<Clip>,
    pub(super) resolved_tracks: Vec<(usize, usize)>,
    pub(super) looping: bool,
    pub(super) button: Option<String>,
    pub(super) speed: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct ComponentWorld {
    pub(crate) scene: Arc<ComponentScene>,
    pub(crate) hierarchy: Arc<ObjectHierarchy>,
    pub(crate) object_indices: Arc<HashMap<String, usize>>,
    pub(crate) local_transforms: Arc<Vec<Transform>>,
    pub(crate) light_intensities: Arc<LightIntensities>,
    pub(crate) world_matrices: Arc<Vec<DMat4>>,
    pub(crate) world_rotations: Arc<Vec<DQuat>>,
    pub(crate) scripts: Vec<ComponentScriptSlot>,
    modules: Option<Rc<RefCell<Modules>>>,
    pub(crate) animations: Vec<ResolvedComponentAnimation>,
    pub(crate) colliders: Arc<Vec<ComponentColliderSlot>>,
    pub(crate) draw_plan: Arc<ComponentDrawPlan>,
    pub(crate) audio_emitters: Option<Arc<BTreeMap<String, usize>>>,
    audio_requests: Option<audio::CueRequests>,
}

struct RebuiltPopulation {
    scene: Arc<ComponentScene>,
    hierarchy: Arc<ObjectHierarchy>,
    object_indices: Arc<HashMap<String, usize>>,
    local: Arc<Vec<Transform>>,
    world: Vec<DMat4>,
    rotations: Vec<DQuat>,
    colliders: Vec<ComponentColliderSlot>,
    draw_plan: Arc<ComponentDrawPlan>,
    draw: rot_scene::SceneDrawFrame,
    lookup: Rc<ObjectLookup>,
    light_intensities: Arc<LightIntensities>,
}

impl RebuiltPopulation {
    fn install(
        self,
        world: &mut ComponentWorld,
        modules: &mut Modules,
    ) -> rot_scene::SceneDrawFrame {
        world.scene = self.scene;
        world.hierarchy = self.hierarchy;
        world.object_indices = self.object_indices;
        world.local_transforms = self.local;
        world.world_matrices = Arc::new(self.world);
        world.world_rotations = Arc::new(self.rotations);
        world.colliders = Arc::new(self.colliders);
        world.draw_plan = self.draw_plan;
        world.light_intensities = self.light_intensities;
        modules.lookup = self.lookup;
        self.draw
    }
}

impl ComponentWorld {
    pub(crate) fn is_static(&self) -> bool {
        self.scripts.is_empty() && self.animations.is_empty()
    }

    fn query_snapshot(&self) -> ComponentQueryInput {
        ComponentQueryInput {
            colliders: Arc::clone(&self.colliders),
            local: Arc::clone(&self.local_transforms),
            world: Arc::clone(&self.world_matrices),
            rotations: Arc::clone(&self.world_rotations),
            hierarchy: Arc::clone(&self.hierarchy),
        }
    }

    fn resolve_staged_transforms(
        &self,
        local: &[Transform],
        staged_lights: &LightIntensities,
    ) -> Result<(Vec<DMat4>, Vec<DQuat>, rot_scene::SceneDrawFrame), Diagnostic> {
        let mut world = Vec::with_capacity(local.len());
        let mut rotations = Vec::with_capacity(local.len());
        self.scene
            .resolve_world_matrices_into(&self.hierarchy, local, &mut world)
            .map_err(invalid_commit)?;
        self.scene
            .resolve_world_rotations_into(&self.hierarchy, local, &mut rotations);
        validate_component_colliders(
            &self.scene,
            &self.colliders,
            local,
            &world,
            &rotations,
            &self.hierarchy.parents,
            Phase::Commit,
        )?;
        let draw = self
            .draw_plan
            .build_frame_with_light_intensities(&self.scene, &world, &rotations, |id| {
                staged_lights
                    .get(id)
                    .or_else(|| self.light_intensities.get(id))
                    .copied()
            })
            .map_err(invalid_commit)?;
        Ok((world, rotations, draw))
    }

    fn rebuild_population(
        &self,
        edits: TopologyEdits,
        local: &[Transform],
        staged_lights: &LightIntensities,
        authoring: &Authoring,
        session: &Rc<ObjectSession>,
        revision: u64,
    ) -> Result<Option<RebuiltPopulation>, Diagnostic> {
        let Some(Population {
            scene,
            local,
            origins,
        }) = edits
            .materialize(
                &self.scene,
                &self.hierarchy,
                &self.object_indices,
                local,
                session.authored_count(),
            )
            .map_err(|(binding, message)| topology_diagnostic(authoring, &binding, &message))?
        else {
            return Ok(None);
        };
        let origin = origins.last().map_or("", |(_, binding)| binding.as_str());
        let invalid = |error: rot_scene::SceneError| {
            topology_diagnostic(authoring, origin, &error.to_string())
        };
        let hierarchy = Arc::new(scene.validate().map_err(invalid)?);
        let object_indices = Arc::new(
            scene
                .objects
                .iter()
                .enumerate()
                .map(|(index, object)| (object.id.clone(), index))
                .collect(),
        );
        // Authored rows are never removed or moved, so these bindings, animation tracks and
        // positional audio indices survive without recompiling modules or restarting clocks.
        let animated_objects: HashSet<_> = self
            .animations
            .iter()
            .flat_map(|animation| animation.resolved_tracks.iter().map(|&(_, index)| index))
            .collect();
        let kinematic =
            kinematic_owner_component(&scene, &hierarchy, &object_indices, &animated_objects)
                .map_err(|error| topology_diagnostic(authoring, origin, &error))?;
        let mut colliders = Vec::new();
        for (owner, object) in scene.objects.iter().enumerate() {
            if hierarchy.effectively_enabled[owner] {
                colliders.extend(
                    object
                        .components
                        .iter()
                        .filter_map(|component| collider_slot(owner, &object.id, component)),
                );
            }
        }
        if kinematic.is_some() && !origins.is_empty() {
            let script_owners: HashSet<_> = self
                .scripts
                .iter()
                .filter(|slot| slot.has_object_input)
                .map(|slot| slot.owner_index)
                .collect();
            let mut driven = vec![false; scene.objects.len()];
            for &index in &hierarchy.order {
                driven[index] = script_owners.contains(&index)
                    || animated_objects.contains(&index)
                    || hierarchy.parents[index].is_some_and(|parent| driven[parent]);
            }
            let first_added = origins[0].0.start;
            if let Some(blocker) = colliders.iter().find(|slot| {
                slot.owner_index >= first_added
                    && driven[slot.owner_index]
                    && slot.collider.role == rot_scene::ColliderRole::Blocker
            }) {
                let binding = origins
                    .iter()
                    .find(|(range, _)| range.contains(&blocker.owner_index))
                    .expect("spawn origin")
                    .1
                    .as_str();
                return Err(topology_diagnostic(
                    authoring,
                    binding,
                    &format!(
                        "cannot spawn kinematic blocker {:?} under a moving object",
                        blocker.owner_id
                    ),
                ));
            }
        }
        let mut protections = kinematic.as_ref().map_or_else(HashMap::new, |owner| {
            owner.protected_component_transforms(authoring, &object_indices, &hierarchy, &colliders)
        });
        protections.extend(
            animated_objects
                .iter()
                .map(|&index| (index, "an Animator controls this Transform")),
        );
        let scene = Arc::new(scene);
        let lookup = Rc::new(ObjectLookup::new(
            Rc::clone(session),
            Arc::clone(&scene),
            Arc::clone(&object_indices),
            protections,
        ));
        let mut light_intensities = self.light_intensities.as_ref().clone();
        light_intensities.retain(|id, _| lookup.contains_light(id));
        commit_light_intensities(
            &mut light_intensities,
            staged_lights.iter().map(|(id, &value)| (id.clone(), value)),
            &lookup,
        );

        let mut world = Vec::new();
        let mut rotations = Vec::new();
        scene
            .resolve_world_matrices_into(&hierarchy, &local, &mut world)
            .map_err(invalid)?;
        scene.resolve_world_rotations_into(&hierarchy, &local, &mut rotations);
        validate_component_colliders(
            &scene,
            &colliders,
            &local,
            &world,
            &rotations,
            &hierarchy.parents,
            Phase::Commit,
        )
        .map_err(|error| topology_diagnostic(authoring, origin, &error.message))?;
        let draw_plan = Arc::new(
            ComponentDrawPlan::new(&scene, &hierarchy, &object_indices, revision)
                .map_err(invalid)?,
        );
        let draw = draw_plan
            .build_frame_with_light_intensities(&scene, &world, &rotations, |id| {
                light_intensities.get(id).copied()
            })
            .map_err(invalid)?;
        Ok(Some(RebuiltPopulation {
            scene,
            hierarchy,
            object_indices,
            local: Arc::new(local),
            world,
            rotations,
            colliders,
            draw_plan,
            draw,
            lookup,
            light_intensities: Arc::new(light_intensities),
        }))
    }
}

fn collider_slot(
    owner_index: usize,
    owner_id: &str,
    component: &rot_scene::component_scene::ComponentInstance,
) -> Option<ComponentColliderSlot> {
    let Component::Collider {
        role,
        offset,
        shape,
    } = component.kind
    else {
        return None;
    };
    component.enabled.then(|| ComponentColliderSlot {
        owner_index,
        owner_id: owner_id.to_owned(),
        collider: rot_scene::Collider {
            enabled: true,
            role,
            offset,
            shape,
        },
    })
}

fn topology_diagnostic(authoring: &Authoring, binding_id: &str, reason: &str) -> Diagnostic {
    let error = diag(DiagnosticCode::InvalidOutput, Phase::Commit, reason);
    if let Some(binding) = authoring.bindings.get(binding_id) {
        error.context(
            binding_id,
            binding,
            authoring.sources.get(&binding.source_id),
        )
    } else {
        error
    }
}

fn commit_light_intensities(
    committed: &mut LightIntensities,
    staged: impl IntoIterator<Item = (String, f32)>,
    lookup: &ObjectLookup,
) {
    for (id, intensity) in staged {
        if !lookup.contains_light(&id) {
            continue;
        }
        if intensity == lookup.authored_light_intensity(&id) {
            committed.remove(&id);
        } else {
            committed.insert(id, intensity);
        }
    }
}

fn diag(code: DiagnosticCode, phase: Phase, message: &str) -> Diagnostic {
    Diagnostic::plain(code, phase, message)
}

fn invalid_commit(error: rot_scene::SceneError) -> Diagnostic {
    diag(
        DiagnosticCode::InvalidOutput,
        Phase::Commit,
        &error.to_string(),
    )
}

fn stage_file_writes(
    callback: &CurrentCallback,
    committed: &[Transform],
    staged: &mut Option<Vec<Transform>>,
    staged_lights: &mut LightIntensities,
    owner_index: usize,
    implicit: Option<Transform>,
    binding: &str,
) {
    let mut stage = |index, pose| {
        if !same_transform(&staged.as_deref().unwrap_or(committed)[index], &pose) {
            staged.get_or_insert_with(|| committed.to_vec())[index] = pose;
        }
    };
    callback.stage_writes(&mut stage, staged_lights, binding);
    if let Some(pose) = implicit {
        if !same_transform(&committed[owner_index], &pose) {
            stage(owner_index, pose);
        }
    }
}

fn validate_component_colliders(
    scene: &ComponentScene,
    colliders: &[ComponentColliderSlot],
    local: &[Transform],
    world: &[DMat4],
    rotations: &[DQuat],
    parents: &[Option<usize>],
    phase: Phase,
) -> Result<(), Diagnostic> {
    for slot in colliders {
        let transform = world_collider_transform(
            &local[slot.owner_index],
            world[slot.owner_index],
            rotations[slot.owner_index],
            parents[slot.owner_index].is_some(),
        )
        .map_err(|error| {
            diag(
                DiagnosticCode::InvalidOutput,
                phase,
                &format!(
                    "{} world transform: {error}",
                    scene.objects[slot.owner_index].id
                ),
            )
        })?;
        slot.collider.resolve(&transform).map_err(|error| {
            diag(
                DiagnosticCode::InvalidOutput,
                phase,
                &format!("{} collider: {error}", scene.objects[slot.owner_index].id),
            )
        })?;
    }
    Ok(())
}

impl Engine {
    /// Replace all live instances of one baked file as a unit. Their module
    /// locals restart, while the committed tick and owner transforms are retained.
    /// Legacy file scripts keep their Reset-only admission.
    pub fn reload_component_source(
        &mut self,
        runtime: &mut Runtime,
        source_id: &str,
        source: BehaviorSource,
    ) -> Result<bool, Diagnostic> {
        self.replace_component_source(runtime, source_id, source, None)
    }

    /// Reload a captured file by baked ID and exact project asset provenance.
    pub fn reload_component_file(
        &mut self,
        runtime: &mut Runtime,
        source_id: &str,
        file: ScriptFile,
    ) -> Result<bool, Diagnostic> {
        let Some(captured) = runtime
            .component_scene()
            .and_then(|scene| scene.script_files.get(source_id))
        else {
            return Ok(false);
        };
        if captured.asset_id != file.asset_id {
            return Err(diag(
                DiagnosticCode::InvalidAuthoring,
                Phase::Setup,
                "Replacement Lua file does not match the captured project asset",
            ));
        }
        file.validate().map_err(|error| {
            diag(
                DiagnosticCode::InvalidAuthoring,
                Phase::Setup,
                &error.to_string(),
            )
        })?;
        let source = file.compatibility_source(ScriptMovement::Ordinary);
        self.replace_component_source(runtime, source_id, source, Some(&file.path))
    }

    fn replace_component_source(
        &mut self,
        runtime: &mut Runtime,
        source_id: &str,
        source: BehaviorSource,
        file_path: Option<&str>,
    ) -> Result<bool, Diagnostic> {
        let Some(world) = &runtime.component_world else {
            return Ok(false);
        };
        let Some(modules) = &world.modules else {
            return Ok(false);
        };
        let mut modules = modules.borrow_mut();
        modules.validate(self, runtime.tick)?;
        let affected = modules.instances.keys().copied().filter(|&index| {
            let slot = &world.scripts[index];
            runtime.authoring.bindings[&slot.component_id].source_id == source_id
        });
        let mut affected = affected.peekable();
        if affected.peek().is_none() {
            return Ok(false);
        }
        let input = ValidatedActionFrame::neutral(&runtime.authoring.actions).map_err(|error| {
            diag(
                DiagnosticCode::InvalidInput,
                Phase::Setup,
                &error.to_string(),
            )
        })?;
        self.begin()?;
        self.begin_debug(
            runtime.tick,
            runtime.tick,
            runtime.commit_revision,
            runtime.commit_revision,
            true,
        );
        let queries = self
            .query_invocation_component(
                || world.query_snapshot(),
                true,
                &runtime.authoring,
                runtime.tick,
                runtime.draw_frame.scene().revision,
                runtime.kinematic_owner.as_ref(),
            )?
            .expect("component reload can run persistent queries");
        let audio_cues = world.audio_requests.as_ref().map(audio::CueRequests::trial);
        let callback = CurrentCallback::new(
            &self.lua,
            &input,
            &modules.lookup,
            &world.local_transforms,
            &world.light_intensities,
            &queries,
            audio_cues
                .as_ref()
                .map(|trial| trial.context(&runtime.authoring)),
        );
        let mut replacements = Vec::new();
        let mut staged_transforms = None;
        let mut staged_lights = LightIntensities::new();
        let describe_reload_error = |mut error: Diagnostic| {
            if let Some(path) = file_path.or(source.label.as_deref()) {
                let location = error
                    .line
                    .map_or_else(|| path.to_owned(), |line| format!("{path}:{line}"));
                error.message = super::bounded_message(&format!("{location}: {}", error.message));
            }
            error
        };
        for index in affected {
            let slot = &world.scripts[index];
            let binding = &runtime.authoring.bindings[&slot.component_id];
            let stage = (|| {
                source.validate().map_err(|error| {
                    diag(
                        DiagnosticCode::InvalidAuthoring,
                        Phase::Setup,
                        &error.to_string(),
                    )
                })?;
                if file_path.is_none()
                    && (world.scene.script_files.contains_key(source_id)
                        || !lua_component::legacy_file_source(source_id, &source))
                {
                    return Err(diag(
                        DiagnosticCode::UnsupportedBackend,
                        Phase::Compile,
                        "Replacement must be a file-backed Lua component",
                    ));
                }
                let kinematic = slot.movement == ScriptMovement::KinematicSphere;
                let mut module = self.load_replacement_module(
                    source_id,
                    &source.text,
                    &slot.component_id,
                    &binding.properties,
                    kinematic,
                )?;
                let original = &world.local_transforms[slot.owner_index];
                let implicit = module.start(self, original, &binding.properties)?;
                if kinematic {
                    if let Some(output) = &implicit {
                        queries
                            .validate_kinematic_output(&slot.component_id, original, output)
                            .map_err(|message| {
                                diag(DiagnosticCode::InvalidOutput, Phase::Output, &message)
                            })?;
                    }
                }
                stage_file_writes(
                    &callback,
                    &world.local_transforms,
                    &mut staged_transforms,
                    &mut staged_lights,
                    slot.owner_index,
                    implicit,
                    &slot.component_id,
                );
                module.source_hash.clone_from(&self.cache[source_id].hash);
                module.last_attempt = runtime.tick;
                Ok(module)
            })();
            let module = stage.map_err(|error| {
                describe_reload_error(error.context(&slot.component_id, binding, Some(&source)))
            })?;
            replacements.push((index, module));
        }
        let edits = callback.take_topology();
        drop(callback);
        let rebuilt = if edits.is_empty() {
            None
        } else {
            world
                .rebuild_population(
                    edits,
                    staged_transforms
                        .as_deref()
                        .unwrap_or(&world.local_transforms),
                    &staged_lights,
                    &runtime.authoring,
                    &modules.lookup.session,
                    runtime.draw_frame.scene().revision,
                )
                .map_err(|mut error| {
                    if error.binding_id.is_some() {
                        error.source_hash = Some(super::source_hash(&source.text).into_boxed_str());
                    }
                    describe_reload_error(error)
                })?
        };
        if rebuilt.is_none() {
            if let Some(trial) = staged_transforms {
                world.resolve_staged_transforms(&trial, &staged_lights)?;
            }
        }
        self.check_cancelled()?;
        modules.instances.extend(replacements);
        Arc::make_mut(&mut runtime.authoring)
            .sources
            .insert(source_id.to_owned(), source);
        Ok(true)
    }

    /// Initialize a native flat-array runtime directly from an authored/baked `ComponentScene`.
    pub fn reset_component(
        &mut self,
        scene: &ComponentScene,
        revision: u64,
    ) -> Result<Runtime, Diagnostic> {
        let hierarchy = Arc::new(scene.validate().map_err(|error| {
            diag(
                DiagnosticCode::InvalidAuthoring,
                Phase::Setup,
                &error.to_string(),
            )
        })?);
        let object_indices: Arc<HashMap<String, usize>> = Arc::new(
            scene
                .objects
                .iter()
                .enumerate()
                .map(|(index, object)| (object.id.clone(), index))
                .collect(),
        );
        for component in scene.objects.iter().flat_map(|object| &object.components) {
            if let Component::Script {
                binding,
                source_kind: Some(_),
                ..
            } = &component.kind
            {
                return Err(diag(
                    DiagnosticCode::InvalidAuthoring,
                    Phase::Setup,
                    &format!(
                        "Lua asset {} requires a project snapshot",
                        binding.source_id
                    ),
                ));
            }
        }
        let draw_plan = Arc::new(
            ComponentDrawPlan::new(scene, &hierarchy, &object_indices, revision).map_err(
                |error| {
                    diag(
                        DiagnosticCode::InvalidAuthoring,
                        Phase::Setup,
                        &error.to_string(),
                    )
                },
            )?,
        );

        let mut animations = Vec::new();
        let mut animated_objects = HashSet::new();
        for (owner_index, object) in scene.objects.iter().enumerate() {
            if !hierarchy.effectively_enabled[owner_index] {
                continue;
            }
            for component in &object.components {
                let Component::Animator {
                    binding,
                    start_playback,
                    playback_speed,
                } = &component.kind
                else {
                    continue;
                };
                if !component.enabled {
                    continue;
                }
                start_playback
                    .validate_action(&scene.actions, &object.id)
                    .map_err(|error| {
                        diag(
                            DiagnosticCode::InvalidAuthoring,
                            Phase::Setup,
                            &error.to_string(),
                        )
                    })?;
                let clip = self.clips.get(&binding.clip).ok_or_else(|| {
                    diag(
                        DiagnosticCode::InvalidAuthoring,
                        Phase::Setup,
                        &format!(
                            "animation on {} uses clip {}, which has no accepted content; Refresh its model or remove the animation",
                            object.id, binding.clip
                        ),
                    )
                })?;
                let resolved_tracks = clip
                    .resolve_indexed_tracks(binding, &object_indices)
                    .map_err(|error| {
                        diag(
                            DiagnosticCode::InvalidAuthoring,
                            Phase::Setup,
                            &error.to_string(),
                        )
                    })?;
                for &(_, target_index) in &resolved_tracks {
                    animated_objects.insert(target_index);
                }
                let button = match start_playback {
                    AnimatorStart::OnPlay {} => None,
                    AnimatorStart::OnButtonPress { action_id } => Some(action_id.clone()),
                };
                animations.push(ResolvedComponentAnimation {
                    clip: Arc::clone(clip),
                    resolved_tracks,
                    looping: binding.looping,
                    button,
                    speed: f64::from(*playback_speed),
                });
            }
        }

        let mut scripts = Vec::new();
        let mut colliders = Vec::new();
        for (owner_index, object) in scene.objects.iter().enumerate() {
            if !hierarchy.effectively_enabled[owner_index] {
                continue;
            }
            for component in &object.components {
                if !component.enabled {
                    continue;
                }
                match &component.kind {
                    Component::Script {
                        binding, movement, ..
                    } if binding.enabled => {
                        let has_object_input = match &binding.target {
                            BindingTarget::Object(id) | BindingTarget::Group(id) => {
                                if animated_objects.contains(&owner_index) {
                                    return Err(diag(
                                        DiagnosticCode::InvalidAuthoring,
                                        Phase::Setup,
                                        &format!(
                                            "behavior {} and an imported animation both write the transform of {id}; disable one of them",
                                            component.id
                                        ),
                                    ));
                                }
                                true
                            }
                            BindingTarget::Application => false,
                        };
                        scripts.push(ComponentScriptSlot {
                            component_id: component.id.clone(),
                            owner_index,
                            owner_id: object.id.clone(),
                            has_object_input,
                            movement: *movement,
                        });
                    }
                    _ => colliders.extend(collider_slot(owner_index, &object.id, component)),
                }
            }
        }
        scripts.sort_by(|left, right| left.component_id.cmp(&right.component_id));

        let authoring = Arc::new(scene.authoring_snapshot(&hierarchy));
        let mut local_transforms: Vec<Transform> = scene
            .objects
            .iter()
            .map(|object| object.transform.clone())
            .collect();
        for animation in &animations {
            animation
                .clip
                .apply_indexed(
                    &animation.resolved_tracks,
                    animation.looping,
                    0.0,
                    &mut local_transforms,
                )
                .map_err(|error| {
                    diag(
                        DiagnosticCode::InvalidOutput,
                        Phase::Setup,
                        &error.to_string(),
                    )
                })?;
        }
        let mut local_transforms = Arc::new(local_transforms);
        let mut world_matrices = Vec::with_capacity(local_transforms.len());
        let mut world_rotations = Vec::with_capacity(local_transforms.len());
        scene
            .resolve_world_matrices_into(&hierarchy, &local_transforms, &mut world_matrices)
            .map_err(|error| {
                diag(
                    DiagnosticCode::InvalidOutput,
                    Phase::Setup,
                    &error.to_string(),
                )
            })?;
        scene.resolve_world_rotations_into(&hierarchy, &local_transforms, &mut world_rotations);
        validate_component_colliders(
            scene,
            &colliders,
            &local_transforms,
            &world_matrices,
            &world_rotations,
            &hierarchy.parents,
            Phase::Setup,
        )?;
        let mut draw_frame = draw_plan
            .build_frame(scene, &world_matrices, &world_rotations)
            .map_err(|error| {
                diag(
                    DiagnosticCode::InvalidOutput,
                    Phase::Setup,
                    &error.to_string(),
                )
            })?;

        let colliders = Arc::new(colliders);
        let mut world_matrices = Arc::new(world_matrices);
        let mut world_rotations = Arc::new(world_rotations);

        let kinematic_owner =
            kinematic_owner_component(scene, &hierarchy, &object_indices, &animated_objects)
                .map_err(|message| {
                    diag(DiagnosticCode::InvalidAuthoring, Phase::Setup, &message)
                })?;

        let has_sound_binding = scripts
            .iter()
            .any(|slot| audio::has_sound_cue(&authoring.bindings[&slot.component_id].properties));
        let audio_requests = has_sound_binding.then(audio::CueRequests::default);
        let audio_emitters = if has_sound_binding {
            let emitters =
                audio::emitters_component(&scripts, &authoring, draw_plan.has_transform_camera())?;
            (!emitters.is_empty()).then(|| Arc::new(emitters))
        } else {
            None
        };

        let command_schema = lifecycle::catalog_identity(&authoring)?;
        let commit_revision_required = authoring.bindings.values().any(|binding| {
            binding.enabled
                && authoring
                    .sources
                    .get(&binding.source_id)
                    .is_some_and(|source| source.host_api_version == LIFECYCLE_HOST_API_VERSION)
        });
        let defaults = RuntimeState::defaults(&authoring)?;
        let mut state = defaults.clone();

        let mut modules = BTreeMap::new();
        let mut lookup = None;
        let mut scene_arc = None;
        let mut topology = TopologyEdits::default();
        let mut start_lights = LightIntensities::new();
        if !scripts.is_empty() {
            let input = ValidatedActionFrame::neutral(&authoring.actions).map_err(|error| {
                diag(
                    DiagnosticCode::InvalidInput,
                    Phase::Setup,
                    &error.to_string(),
                )
            })?;
            self.begin()?;
            self.begin_debug(0, 0, 0, 0, true);
            for slot in &scripts {
                let binding = &authoring.bindings[&slot.component_id];
                if authoring
                    .sources
                    .get(&binding.source_id)
                    .is_some_and(|source| source.host_api_version == LIFECYCLE_HOST_API_VERSION)
                {
                    let patch = self.evaluate_lifecycle(
                        &slot.component_id,
                        binding,
                        &authoring,
                        &defaults,
                        lifecycle::Invocation::Initialize,
                    )?;
                    RuntimeState::apply(&mut state, &slot.component_id, patch).map_err(
                        |error| {
                            error.context(
                                &slot.component_id,
                                binding,
                                authoring.sources.get(&binding.source_id),
                            )
                        },
                    )?;
                }
            }
            state.validate_total()?;
            let mut trial_state = state.clone();
            let audio_cues = audio_requests.as_ref().map(audio::CueRequests::trial);
            let mut trial_transforms: Option<Vec<Transform>> = None;
            let mut start_transforms = None;
            lookup = scripts
                .iter()
                .any(|slot| {
                    let binding = &authoring.bindings[&slot.component_id];
                    if scene.script_files.contains_key(&binding.source_id) {
                        return true;
                    }
                    authoring
                        .sources
                        .get(&binding.source_id)
                        .is_some_and(|source| {
                            lua_component::legacy_file_source(&binding.source_id, source)
                        })
                })
                .then(|| {
                    let mut protected =
                        kinematic_owner.as_ref().map_or_else(HashMap::new, |owner| {
                            owner.protected_component_transforms(
                                &authoring,
                                &object_indices,
                                &hierarchy,
                                &colliders,
                            )
                        });
                    protected.extend(
                        animated_objects
                            .iter()
                            .map(|&index| (index, "an Animator controls this Transform")),
                    );
                    let shared = scene_arc.get_or_insert_with(|| Arc::new(scene.clone()));
                    let session = Rc::new(ObjectSession::new(
                        Arc::clone(shared),
                        Arc::clone(&hierarchy),
                    ));
                    Rc::new(ObjectLookup::new(
                        session,
                        Arc::clone(shared),
                        Arc::clone(&object_indices),
                        protected,
                    ))
                });
            let queries = self.query_invocation_component(
                || ComponentQueryInput {
                    colliders: Arc::clone(&colliders),
                    local: Arc::clone(&local_transforms),
                    world: Arc::clone(&world_matrices),
                    rotations: Arc::clone(&world_rotations),
                    hierarchy: Arc::clone(&hierarchy),
                },
                lookup.is_some(),
                &authoring,
                0,
                revision,
                kinematic_owner.as_ref(),
            )?;
            let authored_lights = Arc::default();
            let callback = lookup.as_ref().map(|lookup| {
                CurrentCallback::new(
                    &self.lua,
                    &input,
                    lookup,
                    &local_transforms,
                    &authored_lights,
                    queries
                        .as_ref()
                        .expect("component callbacks can run queries"),
                    audio_cues.as_ref().map(|trial| trial.context(&authoring)),
                )
            });
            for (index, slot) in scripts.iter().enumerate() {
                let mut module = None;
                let output = self.evaluate_component_binding(
                    slot,
                    &authoring,
                    &scene.script_files,
                    &local_transforms,
                    &object_indices,
                    0.0,
                    &input,
                    queries.as_deref(),
                    audio_cues.as_ref().map(audio::CueTrial::requests),
                    &state,
                    Some(&mut module),
                )?;
                if let Some(module) = module {
                    modules.insert(index, module);
                    stage_file_writes(
                        callback.as_ref().expect("file callback snapshot"),
                        &local_transforms,
                        &mut start_transforms,
                        &mut start_lights,
                        slot.owner_index,
                        output.transform,
                        &slot.component_id,
                    );
                } else if let Some(transform) = output.transform {
                    if !same_transform(&local_transforms[slot.owner_index], &transform) {
                        trial_transforms.get_or_insert_with(|| local_transforms.as_ref().clone())
                            [slot.owner_index] = transform;
                    }
                }
                RuntimeState::apply(&mut trial_state, &slot.component_id, output.state).map_err(
                    |error| {
                        let binding = &authoring.bindings[&slot.component_id];
                        error.context(
                            &slot.component_id,
                            binding,
                            authoring.sources.get(&binding.source_id),
                        )
                    },
                )?;
            }
            if let Some(callback) = callback {
                topology = callback.take_topology();
            }
            drop(queries);
            if let Some(trial) = trial_transforms {
                let mut trial_world = Vec::with_capacity(trial.len());
                let mut trial_rot = Vec::with_capacity(trial.len());
                scene
                    .resolve_world_matrices_into(&hierarchy, &trial, &mut trial_world)
                    .map_err(|error| {
                        diag(
                            DiagnosticCode::InvalidOutput,
                            Phase::Commit,
                            &error.to_string(),
                        )
                    })?;
                scene.resolve_world_rotations_into(&hierarchy, &trial, &mut trial_rot);
                validate_component_colliders(
                    scene,
                    &colliders,
                    &trial,
                    &trial_world,
                    &trial_rot,
                    &hierarchy.parents,
                    Phase::Commit,
                )?;
                draw_plan
                    .build_frame(scene, &trial_world, &trial_rot)
                    .map_err(|error| {
                        diag(
                            DiagnosticCode::InvalidOutput,
                            Phase::Commit,
                            &error.to_string(),
                        )
                    })?;
            }
            trial_state.validate_total()?;
            if let Some(start_transforms) = start_transforms {
                local_transforms = Arc::new(start_transforms);
                scene
                    .resolve_world_matrices_into(
                        &hierarchy,
                        &local_transforms,
                        Arc::make_mut(&mut world_matrices),
                    )
                    .map_err(|error| {
                        diag(
                            DiagnosticCode::InvalidOutput,
                            Phase::Commit,
                            &error.to_string(),
                        )
                    })?;
                scene.resolve_world_rotations_into(
                    &hierarchy,
                    &local_transforms,
                    Arc::make_mut(&mut world_rotations),
                );
                validate_component_colliders(
                    scene,
                    &colliders,
                    &local_transforms,
                    &world_matrices,
                    &world_rotations,
                    &hierarchy.parents,
                    Phase::Commit,
                )?;
                draw_frame = draw_plan
                    .build_frame_with_light_intensities(
                        scene,
                        &world_matrices,
                        &world_rotations,
                        |id| start_lights.get(id).copied(),
                    )
                    .map_err(invalid_commit)?;
            } else if !start_lights.is_empty() {
                draw_frame = draw_frame
                    .with_light_intensities(&start_lights)
                    .map_err(invalid_commit)?;
            }
        }

        self.check_cancelled()?;
        let animation_starts = if animations
            .iter()
            .any(|animation| animation.button.is_some())
        {
            vec![None; animations.len()]
        } else {
            Vec::new()
        };
        let mut runtime = Runtime {
            draw_frame,
            authoring,
            tick: 0,
            commit_revision: 0,
            last_step_commit_revision: None,
            last_hud_activation: 0,
            pending_hud_action: None,
            commit_revision_required,
            command_schema,
            query_observations: Vec::new(),
            audio_batch: None,
            has_sound_binding,
            positional_emitters: None,
            kinematic_owner,
            animations: Arc::default(),
            animation_starts,
            state,
            component_world: Some(ComponentWorld {
                scene: scene_arc.unwrap_or_else(|| Arc::new(scene.clone())),
                hierarchy,
                object_indices,
                local_transforms,
                light_intensities: Arc::new(start_lights),
                world_matrices,
                world_rotations,
                scripts,
                modules: lookup.and_then(|lookup| Modules::new(self, modules, lookup)),
                animations,
                colliders,
                draw_plan,
                audio_emitters,
                audio_requests,
            }),
        };
        if !topology.is_empty() {
            let world = runtime
                .component_world
                .as_mut()
                .expect("Reset constructed component world");
            let module_set = world
                .modules
                .as_ref()
                .expect("topology requires Lua modules")
                .clone();
            let mut modules = module_set.borrow_mut();
            if let Some(rebuilt) = world.rebuild_population(
                topology,
                &world.local_transforms,
                &LightIntensities::new(),
                &runtime.authoring,
                &modules.lookup.session,
                revision,
            )? {
                self.check_cancelled()?;
                runtime.draw_frame = rebuilt.install(world, &mut modules);
            }
        }
        Ok(runtime)
    }

    pub(super) fn step_component_with_input(
        &mut self,
        runtime: &mut Runtime,
        input: &ActionFrame,
    ) -> Result<(), Diagnostic> {
        let world = runtime
            .component_world
            .as_ref()
            .expect("component world checked by caller");
        let validated = input
            .validate_against(&runtime.authoring.actions)
            .map_err(|error| {
                diag(
                    DiagnosticCode::InvalidInput,
                    Phase::Setup,
                    &error.to_string(),
                )
            })?;
        let next = runtime.tick.checked_add(1).ok_or_else(|| {
            diag(
                DiagnosticCode::TickExhausted,
                Phase::Setup,
                "runtime tick exhausted; reset before continuing",
            )
        })?;
        let commit_revision = runtime.commit_revision.checked_add(1).ok_or_else(|| {
            diag(
                DiagnosticCode::CommitExhausted,
                Phase::Setup,
                "commit revision exhausted; Reset before continuing",
            )
        })?;
        if world.is_static() {
            runtime.audio_batch = None;
            runtime.query_observations.clear();
            runtime.tick = next;
            runtime.commit_revision = commit_revision;
            runtime.last_step_commit_revision = Some(commit_revision);
            return Ok(());
        }

        let module_set = world.modules.clone();
        let mut modules = module_set.as_ref().map(|modules| modules.borrow_mut());
        if let Some(modules) = &modules {
            modules.validate(self, runtime.tick)?;
            if let Some((&index, _)) = modules
                .instances
                .iter()
                .find(|(_, module)| module.last_attempt > runtime.tick)
            {
                let slot = &world.scripts[index];
                let binding = &runtime.authoring.bindings[&slot.component_id];
                return Err(diag(
                    DiagnosticCode::Runtime, Phase::Setup,
                    "A previous tick was discarded after Lua ran. Restart Play or replace the affected script before continuing.",
                ).context(&slot.component_id, binding, runtime.authoring.sources.get(&binding.source_id)));
            }
        }

        self.begin()?;
        self.begin_debug(
            runtime.tick,
            next,
            runtime.commit_revision,
            commit_revision,
            false,
        );
        let queries = self.query_invocation_component(
            || world.query_snapshot(),
            modules.is_some(),
            &runtime.authoring,
            runtime.tick,
            runtime.draw_frame.scene().revision,
            runtime.kinematic_owner.as_ref(),
        )?;
        let mut staged_transforms: Option<Vec<Transform>> = None;
        let mut staged_lights = LightIntensities::new();
        let mut staged_state = runtime.state.clone();
        let mut pressed = Vec::new();
        for (index, animation) in world.animations.iter().enumerate() {
            let ticks = if let Some(action) = &animation.button {
                if matches!(validated.get(action), Some(ActionValue::Button(state)) if state.pressed)
                {
                    pressed.push(index);
                    0
                } else if let Some(start) = runtime.animation_starts[index] {
                    next - start
                } else {
                    continue;
                }
            } else {
                next
            };
            let transforms =
                staged_transforms.get_or_insert_with(|| world.local_transforms.as_ref().clone());
            animation
                .clip
                .apply_indexed(
                    &animation.resolved_tracks,
                    animation.looping,
                    ticks as f64 * self.limits.fixed_dt * animation.speed,
                    transforms,
                )
                .map_err(|error| {
                    diag(
                        DiagnosticCode::InvalidOutput,
                        Phase::Commit,
                        &error.to_string(),
                    )
                })?;
        }

        let audio_cues = world.audio_requests.as_ref().map(audio::CueRequests::trial);
        let callback = modules.as_ref().map(|modules| {
            CurrentCallback::new(
                &self.lua,
                &validated,
                &modules.lookup,
                &world.local_transforms,
                &world.light_intensities,
                queries
                    .as_ref()
                    .expect("component callbacks can run queries"),
                audio_cues
                    .as_ref()
                    .map(|trial| trial.context(&runtime.authoring)),
            )
        });
        for (index, slot) in world.scripts.iter().enumerate() {
            let output = if let Some(module) = modules
                .as_mut()
                .and_then(|modules| modules.instances.get_mut(&index))
            {
                let binding = &runtime.authoring.bindings[&slot.component_id];
                self.binding_count.set(0);
                if let Some(session) = self.debugger.borrow_mut().as_mut() {
                    session.context(super::debug::Context {
                        source_id: binding.source_id.clone(),
                        source_hash: module.source_hash.clone(),
                        binding_id: slot.component_id.clone(),
                        object_id: Some(slot.owner_id.clone()),
                        phase: Phase::Update,
                    });
                }
                let original = &world.local_transforms[slot.owner_index];
                let result = module.update(
                    self,
                    original,
                    &binding.properties,
                    self.limits.fixed_dt,
                    next,
                );
                if let Some(session) = self.debugger.borrow_mut().as_mut() {
                    session.clear();
                }
                let implicit = result.map_err(|error| {
                    error.context(
                        &slot.component_id,
                        binding,
                        runtime.authoring.sources.get(&binding.source_id),
                    )
                })?;
                if slot.movement == ScriptMovement::KinematicSphere {
                    if let Some(output) = &implicit {
                        queries
                            .as_ref()
                            .expect("kinematic snapshot")
                            .validate_kinematic_output(&slot.component_id, original, output)
                            .map_err(|message| {
                                diag(DiagnosticCode::InvalidOutput, Phase::Output, &message)
                                    .context(
                                        &slot.component_id,
                                        binding,
                                        runtime.authoring.sources.get(&binding.source_id),
                                    )
                            })?;
                    }
                }
                stage_file_writes(
                    callback.as_ref().expect("file callback snapshot"),
                    &world.local_transforms,
                    &mut staged_transforms,
                    &mut staged_lights,
                    slot.owner_index,
                    implicit,
                    &slot.component_id,
                );
                BindingOutput {
                    transform: None,
                    state: rot_scene::StateFields::new(),
                }
            } else {
                self.evaluate_component_binding(
                    slot,
                    &runtime.authoring,
                    &world.scene.script_files,
                    &world.local_transforms,
                    &world.object_indices,
                    self.limits.fixed_dt,
                    &validated,
                    queries.as_deref(),
                    audio_cues.as_ref().map(audio::CueTrial::requests),
                    &runtime.state,
                    None,
                )?
            };
            if let Some(transform) = output.transform {
                if !same_transform(&world.local_transforms[slot.owner_index], &transform) {
                    staged_transforms
                        .get_or_insert_with(|| world.local_transforms.as_ref().clone())
                        [slot.owner_index] = transform;
                }
            }
            RuntimeState::apply(&mut staged_state, &slot.component_id, output.state).map_err(
                |error| {
                    let binding = &runtime.authoring.bindings[&slot.component_id];
                    error.context(
                        &slot.component_id,
                        binding,
                        runtime.authoring.sources.get(&binding.source_id),
                    )
                },
            )?;
        }
        let topology =
            callback.map_or_else(TopologyEdits::default, |callback| callback.take_topology());
        let rebuilt = if topology.is_empty() {
            None
        } else {
            let lookup = &modules
                .as_ref()
                .expect("topology requires Lua modules")
                .lookup;
            world.rebuild_population(
                topology,
                staged_transforms
                    .as_deref()
                    .unwrap_or(&world.local_transforms),
                &staged_lights,
                &runtime.authoring,
                &lookup.session,
                runtime.draw_frame.scene().revision,
            )?
        };

        let staged_frames = staged_transforms
            .as_ref()
            .filter(|_| rebuilt.is_none())
            .map(|local| world.resolve_staged_transforms(local, &staged_lights))
            .transpose()?;
        let staged_light_frame =
            (rebuilt.is_none() && staged_frames.is_none() && !staged_lights.is_empty())
                .then(|| runtime.draw_frame.with_light_intensities(&staged_lights))
                .transpose()
                .map_err(invalid_commit)?;

        staged_state.validate_total()?;
        let transform_camera = rebuilt
            .as_ref()
            .map(|population| population.draw.transform_camera())
            .or_else(|| {
                staged_frames
                    .as_ref()
                    .map(|(_, _, draw)| draw.transform_camera())
            })
            .unwrap_or_else(|| runtime.transform_camera());
        let mut cues = audio_cues.map_or_else(Vec::new, audio::CueTrial::take);
        if let Some(emitters) = &world.audio_emitters {
            let active_world = rebuilt
                .as_ref()
                .map(|population| population.world.as_slice())
                .or_else(|| staged_frames.as_ref().map(|(world, _, _)| world.as_slice()))
                .unwrap_or(&world.world_matrices);
            audio::resolve_cues_component(&mut cues, emitters, transform_camera, active_world);
        }
        self.check_cancelled()?;
        let observations = queries.map_or_else(Vec::new, |queries| queries.observations.take());
        runtime.audio_batch = (!cues.is_empty()).then_some(super::AudioBatch {
            tick: next,
            commit_revision,
            cues,
        });
        let world_mut = runtime
            .component_world
            .as_mut()
            .expect("component world checked above");
        let topology_changed = rebuilt.is_some();
        if let Some(rebuilt) = rebuilt {
            runtime.draw_frame = rebuilt.install(
                world_mut,
                modules.as_mut().expect("topology requires Lua modules"),
            );
        } else if let (Some(next_local), Some((next_world, next_rot, next_draw))) =
            (staged_transforms, staged_frames)
        {
            runtime.draw_frame = next_draw;
            world_mut.local_transforms = Arc::new(next_local);
            world_mut.world_matrices = Arc::new(next_world);
            world_mut.world_rotations = Arc::new(next_rot);
        } else if let Some(frame) = staged_light_frame {
            runtime.draw_frame = frame;
        }
        if !topology_changed && !staged_lights.is_empty() {
            let lookup = &modules
                .as_ref()
                .expect("light writes require Lua modules")
                .lookup;
            let committed = Arc::make_mut(&mut world_mut.light_intensities);
            commit_light_intensities(committed, staged_lights, lookup);
        }
        for index in pressed {
            runtime.animation_starts[index] = Some(next);
        }
        runtime.tick = next;
        runtime.commit_revision = commit_revision;
        runtime.last_step_commit_revision = Some(commit_revision);
        runtime.query_observations = observations;
        runtime.state = staged_state;
        if let Some(modules) = &mut modules {
            modules.tick = next;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_component_binding(
        &mut self,
        slot: &ComponentScriptSlot,
        authoring: &Authoring,
        files: &BTreeMap<String, ScriptFile>,
        local_transforms: &[Transform],
        object_indices: &HashMap<String, usize>,
        dt: f64,
        frame: &ValidatedActionFrame,
        spatial: Option<&super::spatial_host::QueryInvocation>,
        audio: Option<&RefCell<Vec<super::AudioCue>>>,
        state: &RuntimeState,
        probe: Option<&mut Option<Module>>,
    ) -> Result<BindingOutput, Diagnostic> {
        let binding = &authoring.bindings[&slot.component_id];
        let source = authoring.sources.get(&binding.source_id);
        let file = files.get(&binding.source_id);
        let result = (|| {
            let source = source.ok_or_else(|| {
                diag(
                    DiagnosticCode::MissingSource,
                    Phase::Compile,
                    "binding source is missing; repair its stable source ID",
                )
            })?;
            let group = matches!(binding.target, BindingTarget::Group(_));
            if group != (source.host_api_version == GROUP_ROTATION_HOST_API_VERSION) {
                return Err(diag(
                    DiagnosticCode::UnsupportedBackend,
                    Phase::Compile,
                    "Group targets require lua54 host API6, which only supports Group targets",
                ));
            }
            if !matches!(
                source.host_api_version,
                STATE_HOST_API_VERSION | LIFECYCLE_HOST_API_VERSION | KINEMATIC_HOST_API_VERSION
            ) && (binding.target == BindingTarget::Application
                || !binding.state_defaults.is_empty())
            {
                return Err(diag(
                    DiagnosticCode::UnsupportedBackend,
                    Phase::Compile,
                    "Application targets require API4/5; runtime state requires API4/5/7",
                ));
            }
            if !binding.commands.is_empty() && source.host_api_version != LIFECYCLE_HOST_API_VERSION
            {
                return Err(diag(
                    DiagnosticCode::UnsupportedBackend,
                    Phase::Compile,
                    "Application command declarations require lua54 host API5",
                ));
            }
            let main = match file {
                Some(file) => self.compile_lua(&binding.source_id, &file.text),
                None => self.compile(&binding.source_id, source),
            }
            .map_err(|mut failure| {
                let path = file.map(|file| file.path.as_str()).or_else(|| {
                    source.label.as_deref().filter(|path| {
                        binding.source_id.starts_with("asset-lua-") && path.ends_with(".lua")
                    })
                });
                if let Some(path) = path {
                    failure.message =
                        super::bounded_message(&format!("{path}: {}", failure.message));
                    failure.context(&slot.component_id, binding, Some(source))
                } else {
                    failure
                }
            })?;
            if let Some(session) = self.debugger.borrow_mut().as_mut() {
                session.context(super::debug::Context {
                    source_id: binding.source_id.clone(),
                    source_hash: self
                        .cache
                        .get(&binding.source_id)
                        .expect("just compiled source")
                        .hash
                        .clone(),
                    binding_id: slot.component_id.clone(),
                    object_id: slot.has_object_input.then(|| slot.owner_id.clone()),
                    phase: Phase::Module,
                });
            }
            self.binding_count.set(0);
            if self.cancel.load(Ordering::SeqCst)
                || self
                    .debugger
                    .borrow()
                    .as_ref()
                    .is_some_and(super::debug::DebugSession::cancelled)
            {
                return Err(diag(
                    DiagnosticCode::Cancelled,
                    Phase::Module,
                    "behavior cancelled",
                ));
            }
            let input = slot
                .has_object_input
                .then(|| &local_transforms[slot.owner_index]);
            let mut detected = None;
            let module_probe = probe.as_ref().and_then(|_| {
                (file.is_some()
                    || (matches!(binding.target, BindingTarget::Object(_))
                        && lua_component::legacy_file_source(&binding.source_id, source)))
                .then(|| ModuleProbe {
                    target: &mut detected,
                    kinematic: slot.movement == ScriptMovement::KinematicSphere,
                })
            });
            let result = self.evaluate_with_module_probe(
                &main,
                input,
                &binding.properties,
                ResolveSnapshot::Component {
                    transforms: local_transforms,
                    indices: object_indices,
                },
                dt,
                HostInput {
                    api_version: source.host_api_version,
                    frame,
                    spatial,
                    audio,
                    binding_id: &slot.component_id,
                    state,
                },
                module_probe,
            );
            if let (Some(probe), Some(mut module)) = (probe, detected) {
                module
                    .source_hash
                    .clone_from(&self.cache[&binding.source_id].hash);
                *probe = Some(module);
            }
            let cleanup = self
                .lua
                .create_table()
                .and_then(|empty| main.set_environment(empty));
            if let Err(e) = cleanup {
                self.cache.remove(&binding.source_id);
                drop(main);
                return match result {
                    Err(original) => Err(original),
                    Ok(_) => Err(self.error(e, Phase::Module, DiagnosticCode::Runtime)),
                };
            }
            result
        })();
        if let Some(session) = self.debugger.borrow_mut().as_mut() {
            session.clear();
        }
        if result.is_err() {
            let _ = self.lua.gc_collect();
        }
        result.map_err(|e| e.context(&slot.component_id, binding, source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play_check::{self, CheckIdentity, Comparison, Expectation, PlayCheck, Selection};
    use crate::{Limits, LANGUAGE_ID};
    use rot_scene::component_scene::{
        AnimatorStart, CameraView, ComponentInstance, ComponentScene, Geometry, ObjectNode,
    };
    use rot_scene::{
        AnimationBinding, BehaviorBinding, BehaviorSource, ColliderRole, ColliderShape,
        DisplayProfile, LightKind,
    };
    use std::sync::atomic::AtomicBool;

    fn script_source(text: &str, api: u32) -> BehaviorSource {
        BehaviorSource {
            label: None,
            language_id: LANGUAGE_ID.into(),
            host_api_version: api,
            text: text.into(),
        }
    }

    fn base_scene(id: &str) -> ComponentScene {
        ComponentScene {
            id: id.into(),
            objects: Vec::new(),
            prefabs: BTreeMap::new(),
            materials: BTreeMap::new(),
            display: DisplayProfile::default(),
            background: None,
            ambient: None,
            sources: BTreeMap::new(),
            script_files: BTreeMap::new(),
            actions: BTreeMap::new(),
            hud: Default::default(),
            gameplay_camera: None,
        }
    }

    const GATE_FILE_ID: &str = "sliding-gate";

    fn file_script(text: &str) -> ScriptFile {
        ScriptFile {
            asset_id: "gate-file".into(),
            path: "Scripts/gate.lua".into(),
            text: text.into(),
        }
    }

    fn shared_module_scene(text: &str) -> ComponentScene {
        use rot_scene::component_scene::ScriptSourceKind;
        use rot_scene::input::{ActionDefinition, ActionKind};
        let mut scene = base_scene("gate");
        scene.actions.insert(
            "slide".into(),
            ActionDefinition {
                label: None,
                kind: ActionKind::Button {
                    keys: Vec::new(),
                    primary_mouse: false,
                },
            },
        );
        for (id, speed) in [("left", 1.0), ("right", 2.0)] {
            scene.objects.push(ObjectNode {
                id: id.into(),
                name: id.into(),
                enabled: true,
                parent: None,
                transform: Transform::default(),
                components: vec![ComponentInstance {
                    id: format!("{id}-script"),
                    enabled: true,
                    kind: Component::Script {
                        source_kind: Some(ScriptSourceKind::Asset),
                        movement: Default::default(),
                        binding: BehaviorBinding {
                            source_id: "gate-file".into(),
                            target: BindingTarget::Object(id.into()),
                            enabled: true,
                            properties: BTreeMap::from([(
                                "speed".into(),
                                rot_scene::PropertyValue::Number(speed),
                            )]),
                            state_defaults: BTreeMap::new(),
                            commands: BTreeMap::new(),
                        },
                    },
                }],
            });
        }
        let keys = scene
            .bake_script_assets_with_keys(BTreeMap::from([("gate-file".into(), file_script(text))]))
            .unwrap();
        let generated_id = &keys["gate-file"][&ScriptMovement::Ordinary];
        let file = scene.script_files.remove(generated_id).unwrap();
        scene.script_files.insert(GATE_FILE_ID.into(), file);
        for component in scene
            .objects
            .iter_mut()
            .flat_map(|object| &mut object.components)
        {
            if let Component::Script { binding, .. } = &mut component.kind {
                binding.source_id = GATE_FILE_ID.into();
            }
        }
        scene
    }

    #[test]
    fn file_modules_start_once_in_isolation_and_retained_input_reads_each_tick() {
        use rot_scene::input::ButtonState;
        let scene = shared_module_scene(
            r#"
            local count = 0
            local button
            return {
                on_start = function(transform, props, host)
                    count = count + 1
                    button = host.button
                    transform.translation[2] = count
                end,
                on_update = function(transform, props, dt)
                    if button("slide").held then
                        count = count + 1
                        transform.translation[1] = math.abs(count) * props.speed
                    end
                end
            }
        "#,
        );
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        for id in ["left", "right"] {
            assert_eq!(runtime.object_translation(id), Some([0.0, 1.0, 0.0]));
        }
        let held = ActionFrame(BTreeMap::from([(
            "slide".into(),
            ActionValue::Button(ButtonState {
                held: true,
                pressed: true,
                released: false,
            }),
        )]));
        engine.step_with_input(&mut runtime, &held).unwrap();
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([2.0, 1.0, 0.0]));
        engine.step_with_input(&mut runtime, &held).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([3.0, 1.0, 0.0]));
        assert_eq!(runtime.object_translation("right"), Some([6.0, 1.0, 0.0]));
    }

    #[test]
    fn numbered_baked_artifacts_preserve_admission_and_unknown_versions() {
        let mut scene = shared_module_scene(
            "return {on_update = function(transform) transform.translation[1] = 2 end}",
        );
        let old_id = "asset-lua-1";
        let file = scene.script_files.remove(GATE_FILE_ID).unwrap();
        scene.sources.insert(
            old_id.into(),
            file.compatibility_source(ScriptMovement::Ordinary),
        );
        for component in scene
            .objects
            .iter_mut()
            .flat_map(|object| &mut object.components)
        {
            if let Component::Script { binding, .. } = &mut component.kind {
                binding.source_id = old_id.into();
            }
        }
        let roundtrip = |scene: &ComponentScene| {
            let mut bytes = Vec::new();
            scene.write_json(&mut bytes).unwrap();
            let decoded = ComponentScene::read_json(bytes.as_slice()).unwrap();
            assert_eq!(&decoded, scene);
            decoded
        };
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&roundtrip(&scene), 0).unwrap();
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([2.0, 0.0, 0.0]));

        scene.sources.get_mut(old_id).unwrap().host_api_version = 99;
        let error = engine.reset_component(&roundtrip(&scene), 0).unwrap_err();
        assert_eq!(error.code, DiagnosticCode::UnsupportedBackend);
    }

    #[test]
    fn retained_object_handles_read_committed_local_transforms_through_reload() {
        let mut scene = shared_module_scene(
            r#"
            local beacon, find
            return {
                on_start = function(transform, props, host)
                    find = host.find
                    beacon = find("Beacon")
                    if props.speed == 1 then
                        transform.translation[1] = 4
                    else
                        transform.translation[1] = beacon.transform.translation[1]
                        transform.translation[2] = find("left").transform.translation[1]
                        if find("beacon") ~= nil or find("missing") ~= nil then
                            transform.translation[3] = 99
                        end
                    end
                end,
                on_update = function(transform, props)
                    if props.speed == 1 then
                        transform.translation[1] = transform.translation[1] + 1
                    else
                        local copy = beacon.transform
                        copy.translation[1] = 100
                        transform.translation[1] = beacon.transform.translation[1]
                        transform.translation[2] = find("duplicate").transform.translation[1]
                    end
                end,
            }
        "#,
        );
        let parent = "offset-parent";
        scene.objects[0].name = "Beacon".into();
        scene.objects[0].parent = Some(parent.into());
        scene.objects[0].transform.translation[0] = 2.0;
        scene.objects[1].name = "left".into();
        for (id, name, x) in [
            ("duplicate", "Beacon", 9.0),
            (parent, "Offset Parent", 100.0),
        ] {
            scene.objects.push(ObjectNode {
                id: id.into(),
                name: name.into(),
                enabled: true,
                parent: None,
                transform: Transform {
                    translation: [x, 0.0, 0.0],
                    ..Transform::default()
                },
                components: Vec::new(),
            });
        }
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([4.0, 0.0, 0.0]));
        assert_eq!(runtime.object_translation("right"), Some([2.0, 2.0, 0.0]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("right"), Some([4.0, 9.0, 0.0]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([6.0, 0.0, 0.0]));
        assert_eq!(runtime.object_translation("right"), Some([5.0, 9.0, 0.0]));

        let replacement = file_script(
            r#"
            local beacon, start
            return {
                on_start = function(transform, props, host)
                    beacon = host.find("Beacon")
                    start = beacon.transform.translation[1]
                end,
                on_update = function(transform, props)
                    transform.translation[1] = beacon.transform.translation[1] + props.speed
                    transform.translation[2] = start
                end,
            }
        "#,
        );
        assert!(engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, replacement)
            .unwrap());
        assert_eq!(runtime.tick(), 2);
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("right"), Some([8.0, 6.0, 0.0]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("right"), Some([9.0, 6.0, 0.0]));
    }

    fn add_test_object(
        scene: &mut ComponentScene,
        id: &str,
        parent: Option<&str>,
        components: Vec<ComponentInstance>,
    ) {
        scene.objects.push(ObjectNode {
            id: id.into(),
            name: id.into(),
            enabled: true,
            parent: parent.map(str::to_owned),
            transform: Transform::default(),
            components,
        });
    }

    fn test_light(id: &str, kind: LightKind) -> ComponentInstance {
        ComponentInstance {
            id: id.into(),
            enabled: true,
            kind: Component::Light {
                color: [1.0; 3],
                intensity: 0.25,
                kind,
            },
        }
    }

    fn test_collider(id: &str, role: ColliderRole, shape: ColliderShape) -> ComponentInstance {
        ComponentInstance {
            id: id.into(),
            enabled: true,
            kind: Component::Collider {
                role,
                offset: [0.0; 3],
                shape,
            },
        }
    }

    #[test]
    fn persistent_and_numbered_sound_share_authored_cues_and_the_commit_budget() {
        use rot_scene::input::ButtonState;
        use rot_scene::{PropertyValue, SoundRef};
        let mut scene = shared_module_scene(
            r#"
            local play
            return {
                on_start = function(transform, props, host)
                    play = host.play_sound
                    for i = 1, 5 do play("confirm") end
                end,
                on_update = function(transform, props)
                    props.confirm.asset_id = "forged"
                    props.confirm.gain = 16
                    for i = 1, 5 do play("confirm") end
                    transform.translation[1] = transform.translation[1] + 1
                end,
            }
        "#,
        );
        let cue = |asset: &str, gain| {
            PropertyValue::SoundRef(SoundRef {
                asset_id: Some(asset.into()),
                gain,
                playback: Default::default(),
            })
        };
        for (object, (asset, gain)) in scene
            .objects
            .iter_mut()
            .zip([("near", 0.25), ("far", 0.75)])
        {
            let Component::Script { binding, .. } = &mut object.components[0].kind else {
                unreachable!()
            };
            binding
                .properties
                .insert("confirm".into(), cue(asset, gain));
        }
        let numbered_source = |trial_count| {
            script_source(
                &format!(
                    r#"
            return {{ update = function(transform, props, dt, host)
                local button = host.button("slide")
                if not button.released then
                    local count = dt == 0 and {trial_count} or (button.held and 7 or 6)
                    for i = 1, count do host.play_sound("confirm") end
                end
                return transform
            end }}
        "#
                ),
                super::super::INPUT_HOST_API_VERSION,
            )
        };
        add_test_object(
            &mut scene,
            "numbered",
            None,
            vec![ComponentInstance {
                id: "z-numbered".into(),
                enabled: true,
                kind: Component::Script {
                    source_kind: None,
                    movement: Default::default(),
                    binding: BehaviorBinding {
                        source_id: "numbered-cue".into(),
                        target: BindingTarget::Object("numbered".into()),
                        enabled: true,
                        properties: BTreeMap::from([("confirm".into(), cue("other", 0.5))]),
                        state_defaults: Default::default(),
                        commands: Default::default(),
                    },
                },
            }],
        );
        let mut engine = Engine::new(Limits::default()).unwrap();
        scene
            .sources
            .insert("numbered-cue".into(), numbered_source(7));
        let error = engine.reset_component(&scene, 0).unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("z-numbered"));
        assert!(error.message.contains("exceeds16 sound cue requests"));
        scene
            .sources
            .insert("numbered-cue".into(), numbered_source(6));
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        assert!(runtime.audio_batch().is_none());
        engine.step(&mut runtime).unwrap();
        let committed = runtime.audio_batch().unwrap().clone();
        assert_eq!(committed.tick, 1);
        assert_eq!(committed.cues.len(), 16);
        for (range, binding, asset, gain) in [
            (0..5, "left-script", "near", 0.25),
            (5..10, "right-script", "far", 0.75),
            (10..16, "z-numbered", "other", 0.5),
        ] {
            assert!(committed.cues[range]
                .iter()
                .all(|cue| cue.binding_id == binding
                    && cue.property_name == "confirm"
                    && cue.asset_id == asset
                    && cue.gain == gain));
        }
        let input = |held, released| {
            ActionFrame(BTreeMap::from([(
                "slide".into(),
                ActionValue::Button(ButtonState {
                    held,
                    pressed: false,
                    released,
                }),
            )]))
        };
        let error = engine
            .step_with_input(&mut runtime, &input(true, false))
            .unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("z-numbered"));
        assert!(error.message.contains("exceeds16 sound cue requests"));
        assert_eq!(runtime.tick(), 1);
        assert_eq!(runtime.object_translation("left"), Some([1.0, 0.0, 0.0]));
        assert_eq!(runtime.audio_batch(), Some(&committed));

        let replacement = |count| {
            file_script(&format!(
                r#"
            return {{ on_start = function(transform, props, host)
                for i = 1, {count} do host.play_sound("confirm") end
            end }}
        "#
            ))
        };
        let error = engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, replacement(9))
            .unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("right-script"));
        assert!(error.message.contains("exceeds16 sound cue requests"));
        assert_eq!(runtime.audio_batch(), Some(&committed));
        assert!(engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, replacement(8))
            .unwrap());
        assert_eq!(runtime.audio_batch(), Some(&committed));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.audio_batch().unwrap().cues.len(), 6);
        engine
            .step_with_input(&mut runtime, &input(false, true))
            .unwrap();
        assert!(runtime.audio_batch().is_none());
    }

    #[test]
    fn persistent_positional_sound_uses_the_final_topology_frame() {
        use rot_scene::{PropertyValue, SoundPlayback, SoundRef};
        let mut scene = shared_module_scene(
            r#"
            return { on_update = function(transform, props, dt, host)
                host.play_sound("confirm")
                local stage = host.find("stage")
                local pose = stage.transform
                pose.translation[1] = 6
                stage:set_transform(pose)
                local camera = host.find("camera")
                local pose = camera.transform
                pose.translation[1] = 2
                camera:set_transform(pose)
                local template = host.find("template")
                host.spawn(template, template.transform)
            end }
        "#,
        );
        scene.objects.truncate(1);
        scene.objects[0].parent = Some("stage".into());
        scene.objects[0].transform.translation[2] = -3.0;
        let Component::Script { binding, .. } = &mut scene.objects[0].components[0].kind else {
            unreachable!()
        };
        binding.properties.insert(
            "confirm".into(),
            PropertyValue::SoundRef(SoundRef {
                asset_id: Some("beacon-cue".into()),
                gain: 0.8,
                playback: SoundPlayback::Positional { range: 10.0 },
            }),
        );
        add_test_object(&mut scene, "stage", None, Vec::new());
        scene.objects.last_mut().unwrap().transform.translation[0] = -4.0;
        add_test_object(
            &mut scene,
            "camera",
            None,
            vec![ComponentInstance {
                id: "game-view".into(),
                enabled: true,
                kind: Component::Camera {
                    view: CameraView::transform_perspective(60.0),
                },
            }],
        );
        scene.gameplay_camera = Some("game-view".into());
        add_test_object(&mut scene, "template", None, Vec::new());
        scene.objects.last_mut().unwrap().enabled = false;
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        assert!(runtime.audio_batch().is_none());
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("rot-spawn-1-o0"), Some([0.0; 3]));
        let batch = runtime.audio_batch().unwrap();
        assert_eq!(batch.cues.len(), 1);
        let cue = &batch.cues[0];
        assert_eq!(cue.asset_id, "beacon-cue");
        assert!((cue.gain - 0.4).abs() < 1e-6);
        assert!((cue.pan.unwrap() - 0.8).abs() < 1e-6);
    }

    #[test]
    fn persistent_and_numbered_queries_share_one_world_and_budget() {
        use super::super::{QueryResult, SPATIAL_HOST_API_VERSION};
        use rot_scene::input::ButtonState;
        let mut scene = shared_module_scene(
            r#"
            local probe, wall, sensor, sweep, overlap, ticks
            return {
                on_start = function(transform, props, host)
                    probe = host.find("probe")
                    wall = host.find("wall")
                    sensor = host.find("sensor")
                    sweep, overlap = host.sweep, host.overlap
                    ticks = 0
                    if not overlap(probe, {5, 0, 0}, sensor).overlapping then
                        transform.scale[1] = 0
                    end
                end,
                on_update = function(transform, props, dt, host)
                    ticks = ticks + 1
                    local pose = wall.transform
                    pose.translation[1] = ticks * 10
                    wall:set_transform(pose)
                    local hit = overlap(probe, {5, 0, 0}, sensor)
                    if hit.probe_id ~= "probe" or hit.sensor_id ~= "sensor" or hit.position[1] ~= 5
                        or not hit.overlapping or hit.signed_distance >= 0 then
                        transform.scale[1] = 0
                    end
                    for i = 1, 15 do
                        local result = sweep(probe, {4, 0, 0})
                        if ticks == 1 and (result.outcome ~= "blocked" or result.target_id ~= "wall")
                            or ticks == 2 and (result.outcome ~= "clear" or result.target_id ~= nil) then
                            transform.scale[1] = 0
                        end
                    end
                    transform.translation[1] = ticks
                end,
            }
        "#,
        );
        let numbered = &mut scene.objects[1].components[0];
        numbered.id = "z-numbered".into();
        let Component::Script { binding, .. } = &mut numbered.kind else {
            unreachable!()
        };
        binding.source_id = "numbered-queries".into();
        scene.sources.insert(
            "numbered-queries".into(),
            script_source(
                r#"
            return { update = function(transform, props, dt, host)
                local button = host.button("slide")
                if dt > 0 and button.held then
                    for i = 1, (button.pressed and 16 or 17) do
                        host.sweep("probe", {4, 0, 0})
                    end
                end
                return transform
            end }
        "#,
                SPATIAL_HOST_API_VERSION,
            ),
        );
        add_test_object(
            &mut scene,
            "probe",
            None,
            vec![test_collider(
                "probe-ball",
                ColliderRole::Blocker,
                ColliderShape::Sphere { radius: 0.25 },
            )],
        );
        for (id, role, x) in [
            ("wall", ColliderRole::Blocker, 2.0),
            ("sensor", ColliderRole::Sensor, 5.0),
        ] {
            add_test_object(
                &mut scene,
                id,
                None,
                vec![test_collider(
                    &format!("{id}-box"),
                    role,
                    ColliderShape::Box {
                        half_extents: [0.25, 0.5, 0.5],
                    },
                )],
            );
            scene.objects.last_mut().unwrap().transform.translation[0] = x;
        }
        let builds = super::super::spatial_host::component_world_builds;
        let initial_builds = builds();
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 7).unwrap();
        assert_eq!(builds(), initial_builds + 1);
        assert!(runtime.query_observations().is_empty());
        let held = |pressed| {
            ActionFrame(BTreeMap::from([(
                "slide".into(),
                ActionValue::Button(ButtonState {
                    held: true,
                    pressed,
                    released: false,
                }),
            )]))
        };
        engine.step_with_input(&mut runtime, &held(true)).unwrap();
        assert_eq!(builds(), initial_builds + 2);
        assert_eq!(runtime.object_translation("wall"), Some([10.0, 0.0, 0.0]));
        let committed = runtime.query_observations().to_vec();
        assert_eq!(committed.len(), 32);
        assert!(
            matches!(&committed[0].query, QueryResult::Overlap { result } if result.overlapping)
        );
        assert!(committed.iter().enumerate().all(|(index, entry)| {
            entry.scene_revision == 7 && entry.snapshot_tick == 0
                && entry.binding_id == if index < 16 { "left-script" } else { "z-numbered" }
                && (index == 0 || matches!(&entry.query, QueryResult::Sweep { result, .. } if result.target_id.as_deref() == Some("wall")))
        }));

        let error = engine
            .step_with_input(&mut runtime, &held(false))
            .unwrap_err();
        assert_eq!(error.code, DiagnosticCode::QueryLimit);
        assert_eq!(error.binding_id.as_deref(), Some("z-numbered"));
        assert_eq!(builds(), initial_builds + 3);
        assert_eq!(runtime.tick(), 1);
        assert_eq!(runtime.object_translation("left"), Some([1.0, 0.0, 0.0]));
        assert_eq!(runtime.object_translation("wall"), Some([10.0, 0.0, 0.0]));
        assert_eq!(runtime.query_observations(), committed);

        let replacement = file_script(
            r#"
            local result
            return {
                on_start = function(transform, props, host)
                    result = host.sweep(host.find("probe"), {4, 0, 0})
                    if result.outcome ~= "clear" then transform.scale[1] = 0 end
                end,
                on_update = function(transform)
                    transform.translation[2] = result.suggested_translation[1]
                end,
            }
        "#,
        );
        assert!(engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, replacement)
            .unwrap());
        assert_eq!(builds(), initial_builds + 4);
        assert_eq!(runtime.query_observations(), committed);
        engine.step(&mut runtime).unwrap();
        assert_eq!(builds(), initial_builds + 4);
        assert!(runtime.query_observations().is_empty());
        assert_eq!(runtime.object_translation("left"), Some([1.0, 4.0, 0.0]));
    }

    #[test]
    fn query_handles_follow_committed_population_across_spawn_and_destroy() {
        let start = super::super::spatial_host::component_world_builds();
        let mut scene = shared_module_scene(
            r#"
            local probe
            return {
                on_start = function(transform, props, host)
                    local template = host.find("template")
                    probe = host.spawn(template, template.transform)
                end,
                on_update = function(transform, props, dt, host)
                    if probe.transform ~= nil then host.destroy(probe) end
                    local hit = host.sweep(probe, {0, 0, 0})
                    if hit.outcome ~= "clear" then transform.scale[1] = 0 end
                end,
            }
        "#,
        );
        scene.objects.truncate(1);
        add_test_object(
            &mut scene,
            "template",
            None,
            vec![test_collider(
                "probe-template",
                ColliderRole::Blocker,
                ColliderShape::Sphere { radius: 0.25 },
            )],
        );
        scene.objects.last_mut().unwrap().enabled = false;
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut pending = scene.clone();
        pending.script_files.get_mut(GATE_FILE_ID).unwrap().text = r#"
            return { on_start = function(transform, props, host)
                local template = host.find("template")
                local probe = host.spawn(template, template.transform)
                host.sweep(probe, {0, 0, 0})
            end }
        "#
        .into();
        let error = engine.reset_component(&pending, 0).unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("left-script"));
        assert!(error.message.contains("requires live Object handles"));
        assert_eq!(super::super::spatial_host::component_world_builds(), start);

        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("rot-spawn-1-o0"), None);
        assert!(matches!(&runtime.query_observations()[0].query,
            super::super::QueryResult::Sweep { probe_id, .. } if probe_id == "rot-spawn-1-o0"));
        assert_eq!(
            super::super::spatial_host::component_world_builds(),
            start + 1
        );
        let error = engine.step(&mut runtime).unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("left-script"));
        assert!(error.message.contains("requires live Object handles"));
        assert_eq!(
            super::super::spatial_host::component_world_builds(),
            start + 1
        );
        assert_eq!(runtime.tick(), 1);
    }

    fn rendered_intensity(runtime: &Runtime, id: &str) -> f32 {
        runtime
            .draw_frame()
            .hierarchy()
            .unwrap()
            .lighting()
            .light(id)
            .unwrap()
            .intensity
    }

    #[test]
    fn spawned_subtrees_preserve_flags_and_surviving_handles_after_compaction() {
        let mut scene = shared_module_scene(
            r#"
            local first, second, first_light, second_light, child, ticks
            return {
                on_start = function(transform, props, host)
                    local template = host.find("template")
                    local pose = template.transform
                    pose.translation[1] = 4
                    first = host.spawn(template, pose, host.find("left"))
                    second = host.spawn(template, pose)
                    if first.transform ~= nil or first:component("Light") ~= nil
                        or host.find("rot-spawn-2-o0") ~= nil then
                        transform.scale[1] = 0
                    end
                    ticks = 0
                end,
                on_update = function(transform, props, dt, host)
                    ticks = ticks + 1
                    if ticks == 1 then
                        first_light = first:component("Light")
                        second_light = second:component("Light")
                        child = host.find("rot-spawn-3-o1")
                        local displaced = first.transform
                        displaced.translation[1] = 99
                        first:set_transform(displaced)
                        first_light:set_intensity(9)
                        second_light:set_intensity(5)
                        host.destroy(first)
                        host.destroy(first)
                    elseif ticks == 2 then
                        if first.transform ~= nil or first_light.intensity ~= nil
                            or second_light.intensity ~= 5 then
                            transform.scale[1] = 0
                        end
                        local pose = second.transform
                        pose.translation[1] = 6
                        second:set_transform(pose)
                        second_light:set_intensity(7)
                        host.destroy(child)
                    else
                        if child.transform ~= nil then transform.scale[1] = 0 end
                        host.destroy(second)
                    end
                end,
            }
        "#,
        );
        scene.objects.truncate(1);
        add_test_object(
            &mut scene,
            "template",
            None,
            vec![
                test_light("template-light", LightKind::Point { range: 8.0 }),
                ComponentInstance {
                    id: "template-collider".into(),
                    enabled: true,
                    kind: Component::Collider {
                        role: ColliderRole::Blocker,
                        offset: [0.0; 3],
                        shape: ColliderShape::Sphere { radius: 0.25 },
                    },
                },
            ],
        );
        scene.objects[1].enabled = false;
        let mesh = ComponentInstance {
            id: "rot-spawn-1-o0-c0".into(),
            enabled: true,
            kind: Component::MeshRenderer {
                geometry: Geometry::Cube,
                tint: [1.0; 4],
                material: None,
            },
        };
        add_test_object(&mut scene, "visible-child", Some("template"), vec![mesh]);
        add_test_object(&mut scene, "hidden-child", Some("template"), Vec::new());
        scene.objects[3].enabled = false;
        let original = scene.clone();
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        let objects = &runtime.component_scene().unwrap().objects;
        let first = objects
            .iter()
            .find(|object| object.id == "rot-spawn-2-o0")
            .unwrap();
        assert_eq!(first.parent.as_deref(), Some("left"));
        assert_eq!(first.name, "template");
        assert!(first.enabled);
        assert!(
            !objects
                .iter()
                .find(|object| object.id == "rot-spawn-2-o2")
                .unwrap()
                .enabled
        );
        assert_eq!(runtime.draw_frame().scene().objects.len(), 2);
        assert_eq!(runtime.component_world.as_ref().unwrap().colliders.len(), 2);

        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("rot-spawn-2-o0"), None);
        assert_eq!(rendered_intensity(&runtime, "rot-spawn-3-o0-c0"), 5.0);
        assert_eq!(runtime.draw_frame().scene().objects.len(), 1);
        assert_eq!(runtime.component_world.as_ref().unwrap().colliders.len(), 1);
        assert!(!runtime
            .component_world
            .as_ref()
            .unwrap()
            .light_intensities
            .contains_key("rot-spawn-2-o0-c0"));
        engine.step(&mut runtime).unwrap();
        assert_eq!(
            runtime.object_translation("rot-spawn-3-o0"),
            Some([6.0, 0.0, 0.0])
        );
        assert_eq!(runtime.object_translation("rot-spawn-3-o1"), None);
        assert_eq!(rendered_intensity(&runtime, "rot-spawn-3-o0-c0"), 7.0);
        assert!(runtime.draw_frame().scene().objects.is_empty());
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.component_scene().unwrap(), &original);
        assert_eq!(scene, original);
    }

    #[test]
    fn failed_topology_and_reload_preview_do_not_publish_or_reuse_pending_ids() {
        let mut scene = shared_module_scene(
            r#"
            local template, root
            return {
                on_start = function(transform, props, host)
                    template = host.find("template")
                    root = host.spawn(template, template.transform)
                end,
                on_update = function(transform, props, dt, host)
                    host.spawn(template, template.transform, root)
                    host.destroy(root)
                    transform.translation[1] = 42
                end,
            }
        "#,
        );
        scene.objects.truncate(1);
        add_test_object(&mut scene, "template", None, Vec::new());
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        let frame = runtime.draw_frame().clone();
        let world = runtime.component_world.as_ref().unwrap().scene.clone();
        let error = engine.step(&mut runtime).unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("left-script"));
        assert!(error.message.contains("cannot spawn under removed parent"));
        assert_eq!(runtime.tick(), 0);
        assert_eq!(runtime.object_translation("left"), Some([0.0; 3]));
        let replacement = file_script(
            r#"
            local template, trial, canceled, tick
            return {
                on_start = function(transform, props, host)
                    template = host.find("template")
                    trial = host.spawn(template, template.transform)
                    host.destroy(host.find("rot-spawn-1-o0"))
                    tick = 0
                end,
                on_update = function(transform, props, dt, host)
                    tick = tick + 1
                    if trial.transform ~= nil then transform.scale[1] = 0 end
                    if tick == 1 then
                        canceled = host.spawn(template, template.transform)
                        host.destroy(canceled)
                        host.destroy(canceled)
                    else
                        if canceled.transform ~= nil then transform.scale[1] = 0 end
                        local root = host.spawn(template, template.transform)
                        host.spawn(template, template.transform, root)
                    end
                end,
            }
        "#,
        );
        assert!(engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, replacement)
            .unwrap());
        engine.step(&mut runtime).unwrap();
        assert!(Arc::ptr_eq(
            &world,
            &runtime.component_world.as_ref().unwrap().scene
        ));
        assert!(Arc::ptr_eq(
            frame.hierarchy().unwrap(),
            runtime.draw_frame().hierarchy().unwrap()
        ));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("rot-spawn-1-o0"), Some([0.0; 3]));
        for sequence in 2..=4 {
            assert_eq!(
                runtime.object_translation(&format!("rot-spawn-{sequence}-o0")),
                None
            );
        }
        let spawned_child = runtime
            .component_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == "rot-spawn-6-o0")
            .unwrap();
        assert_eq!(spawned_child.parent.as_deref(), Some("rot-spawn-5-o0"));

        let invalid = file_script(
            r#"
            return { on_start = function(transform, props, host)
                local template = host.find("template")
                local root = host.find("rot-spawn-1-o0")
                host.spawn(template, template.transform, root)
                host.destroy(root)
            end }
        "#,
        );
        let expected = super::super::source_hash(&invalid.text);
        let error = engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, invalid)
            .unwrap_err();
        assert_eq!(error.source_hash.as_deref(), Some(expected.as_str()));
        assert!(error.message.starts_with("Scripts/gate.lua:"));
        assert_eq!(runtime.object_translation("rot-spawn-1-o0"), Some([0.0; 3]));
    }

    #[test]
    fn light_only_commits_share_geometry_and_survive_transform_rebuilds() {
        let mut scene = shared_module_scene(
            r#"
            local lamp, point, ticks
            return {
                on_start = function(transform, props, host)
                    lamp = host.find("lamp")
                    point = lamp:component("Light")
                    ticks = 0
                    point:set_intensity(2)
                    lamp:component("Light", "a-spot"):set_intensity(3)
                    lamp:component("Light", "sun"):set_intensity(4)
                    if host.find("left"):component("Light", "point") ~= nil
                        or lamp:component("Light", "missing") ~= nil
                        or lamp:component("light", "point") ~= nil then
                        transform.translation[3] = 99
                    end
                end,
                on_update = function()
                    ticks = ticks + 1
                    if ticks <= 2 then
                        point:set_intensity(6)
                    else
                        local pose = lamp.transform
                        pose.translation[1] = ticks
                        lamp:set_transform(pose)
                        if ticks == 4 then point:set_intensity(8) end
                    end
                end,
            }
            "#,
        );
        scene.objects.truncate(1);
        add_test_object(
            &mut scene,
            "lamp",
            None,
            vec![
                test_light("point", LightKind::Point { range: 8.0 }),
                test_light(
                    "a-spot",
                    LightKind::Spot {
                        range: 8.0,
                        inner_degrees: 15.0,
                        outer_degrees: 30.0,
                    },
                ),
                test_light(
                    "sun",
                    LightKind::Directional {
                        shadow: Default::default(),
                    },
                ),
                ComponentInstance {
                    id: "lamp-mesh".into(),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.0; 4],
                        material: None,
                    },
                },
            ],
        );
        add_test_object(
            &mut scene,
            "camera",
            None,
            vec![ComponentInstance {
                id: "camera-view".into(),
                enabled: true,
                kind: Component::Camera {
                    view: CameraView::transform_perspective(50.0),
                },
            }],
        );
        scene.gameplay_camera = Some("camera-view".into());
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([0.0; 3]));
        assert_eq!(rendered_intensity(&runtime, "point"), 2.0);
        assert_eq!(rendered_intensity(&runtime, "a-spot"), 3.0);
        assert_eq!(rendered_intensity(&runtime, "sun"), 4.0);
        let initial_frame = runtime.draw_frame().clone();
        let world_buffer = runtime.world_matrices().unwrap().as_ptr();
        engine.step(&mut runtime).unwrap();
        assert_eq!(rendered_intensity(&runtime, "point"), 6.0);
        assert_eq!(
            initial_frame
                .hierarchy()
                .unwrap()
                .lighting()
                .light("point")
                .unwrap()
                .intensity,
            2.0
        );
        let lit = runtime.draw_frame().clone();
        assert!(Arc::ptr_eq(&initial_frame.snapshot(), &lit.snapshot()));
        assert_eq!(
            initial_frame.hierarchy().unwrap().matrices().as_ptr(),
            lit.hierarchy().unwrap().matrices().as_ptr()
        );
        assert_eq!(world_buffer, runtime.world_matrices().unwrap().as_ptr());
        assert!(lit.transform_camera().is_some());
        assert_eq!(initial_frame.transform_camera(), lit.transform_camera());
        engine.step(&mut runtime).unwrap();
        assert!(Arc::ptr_eq(
            lit.hierarchy().unwrap(),
            runtime.draw_frame().hierarchy().unwrap()
        ));

        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("lamp"), Some([3.0, 0.0, 0.0]));
        assert_eq!(rendered_intensity(&runtime, "point"), 6.0);
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("lamp"), Some([4.0, 0.0, 0.0]));
        assert_eq!(rendered_intensity(&runtime, "point"), 8.0);
        let updated = runtime.updated_component_scene().unwrap();
        let mut bytes = Vec::new();
        updated.write_json(&mut bytes).unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            wire["scene"]["objects"][1]["components"][0]["intensity"],
            8.0
        );

        let replacement = file_script(
            r#"
            local light
            return {
                on_start = function(transform, props, host)
                    light = host.find("lamp"):component("Light")
                    light:set_intensity(12)
                end,
                on_update = function() light:set_intensity(0.25) end,
            }
        "#,
        );
        assert!(engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, replacement)
            .unwrap());
        assert_eq!(rendered_intensity(&runtime, "point"), 8.0);
        assert_eq!(runtime.tick(), 4);
        engine.step(&mut runtime).unwrap();
        assert_eq!(rendered_intensity(&runtime, "point"), 0.25);
        assert!(!runtime
            .component_world
            .as_ref()
            .unwrap()
            .light_intensities
            .contains_key("point"));
    }

    #[test]
    fn light_writes_follow_callback_order_and_roll_back_with_transforms() {
        let mut scene = shared_module_scene(
            r#"
            local lamp, light, ticks
            return {
                on_start = function(transform, props, host)
                    lamp = host.find("lamp")
                    light = lamp:component("Light", "point")
                    ticks = 0
                    if props.speed == 1 then
                        light:set_intensity(2)
                    else
                        transform.translation[3] = light.intensity
                        light:set_intensity(3)
                    end
                end,
                on_update = function(transform, props)
                    ticks = ticks + 1
                    if props.speed == 1 then
                        light:set_intensity(5)
                        if ticks == 3 then
                            local pose = lamp.transform
                            pose.translation[1] = 4
                            lamp:set_transform(pose)
                        end
                    else
                        transform.translation[2] = light.intensity
                        if ticks == 1 then
                            light:set_intensity(light.intensity)
                        elseif ticks == 2 then
                            light:set_intensity(6)
                        else
                            light:set_intensity(16.00000001)
                        end
                    end
                end,
            }
            "#,
        );
        scene.objects.swap(0, 1);
        add_test_object(
            &mut scene,
            "lamp",
            None,
            vec![test_light("point", LightKind::Point { range: 8.0 })],
        );
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        assert_eq!(rendered_intensity(&runtime, "point"), 3.0);
        assert_eq!(runtime.object_translation("right"), Some([0.0, 0.0, 0.25]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(rendered_intensity(&runtime, "point"), 3.0);
        assert_eq!(runtime.object_translation("right"), Some([0.0, 3.0, 0.25]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(rendered_intensity(&runtime, "point"), 6.0);

        let last_frame = runtime.draw_frame().clone();
        let error = engine.step(&mut runtime).unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("right-script"));
        assert!(error.line.is_some());
        assert!(error
            .message
            .contains("light intensity must be finite in 0..=16"));
        assert_eq!(runtime.tick(), 2);
        assert_eq!(runtime.object_translation("lamp"), Some([0.0; 3]));
        assert_eq!(runtime.object_translation("right"), Some([0.0, 3.0, 0.25]));
        assert_eq!(rendered_intensity(&runtime, "point"), 6.0);
        assert!(Arc::ptr_eq(
            last_frame.hierarchy().unwrap(),
            runtime.draw_frame().hierarchy().unwrap()
        ));
    }

    #[test]
    fn explicit_transform_writes_merge_in_script_order_from_committed_reads() {
        let mut scene = shared_module_scene(
            r#"
            local target, owner, ticks
            local function write_x(object, x)
                local pose = object.transform
                pose.translation[1] = x
                object:set_transform(pose)
            end
            return {
                on_start = function(transform, props, host)
                    target = host.find("target")
                    ticks = 0
                    if props.speed == 1 then
                        owner = host.find("left")
                        write_x(target, 2)
                        write_x(owner, 8)
                        transform.translation[1] = 7
                    else
                        transform.translation[2] = target.transform.translation[1]
                        write_x(target, 12)
                    end
                end,
                on_update = function(transform, props)
                    ticks = ticks + 1
                    if props.speed == 1 then
                        write_x(owner, 8)
                        write_x(owner, 9)
                        write_x(target, 30)
                    else
                        local committed = target.transform
                        transform.translation[2] = committed.translation[1]
                        if ticks == 1 then
                            target:set_transform(committed)
                        else
                            write_x(target, 40)
                        end
                    end
                end,
            }
        "#,
        );
        scene.objects.swap(0, 1);
        add_test_object(&mut scene, "target", None, Vec::new());
        add_test_object(
            &mut scene,
            "child",
            Some("target"),
            vec![ComponentInstance {
                id: "child-mesh".into(),
                enabled: true,
                kind: Component::MeshRenderer {
                    geometry: Geometry::Cube,
                    tint: [1.0; 4],
                    material: None,
                },
            }],
        );
        let rendered_x = |runtime: &Runtime| {
            runtime.draw_frame().hierarchy().unwrap().matrices()[0]
                .w_axis
                .x
        };
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([7.0, 0.0, 0.0]));
        assert_eq!(runtime.object_translation("right"), Some([0.0, 0.0, 0.0]));
        assert_eq!(rendered_x(&runtime), 12.0);
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([9.0, 0.0, 0.0]));
        assert_eq!(runtime.object_translation("right"), Some([0.0, 12.0, 0.0]));
        assert_eq!(rendered_x(&runtime), 12.0);
        engine.step(&mut runtime).unwrap();
        assert_eq!(rendered_x(&runtime), 40.0);
    }

    #[test]
    fn later_script_fault_rolls_back_explicit_transforms_and_reload_leaves_pose_committed() {
        let mut scene = shared_module_scene(
            r#"
            return { on_update = function(transform, props, dt, host)
                local panel = host.find("panel")
                local pose = panel.transform
                pose.translation[1] = 30
                if props.speed == 2 then
                    pose.scale[1] = 0
                end
                panel:set_transform(pose)
            end }
        "#,
        );
        add_test_object(
            &mut scene,
            "panel",
            None,
            vec![ComponentInstance {
                id: "panel-collider".into(),
                enabled: true,
                kind: Component::Collider {
                    role: ColliderRole::Blocker,
                    shape: ColliderShape::Sphere { radius: 0.25 },
                    offset: [0.0; 3],
                },
            }],
        );
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        let error = engine.step(&mut runtime).unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("right-script"));
        assert_eq!(error.phase, Phase::Update);
        assert!(error.line.is_some());
        assert_eq!(runtime.tick(), 0);
        assert_eq!(runtime.object_translation("panel"), Some([0.0, 0.0, 0.0]));
        assert!(engine
            .reload_component_file(
                &mut runtime,
                GATE_FILE_ID,
                file_script(
                    r#"
                    local panel
                    return {
                        on_start = function(transform, props, host)
                            panel = host.find("panel")
                            local pose = panel.transform
                            pose.translation[1] = 80
                            panel:set_transform(pose)
                        end,
                        on_update = function(transform, props)
                            local pose = panel.transform
                            pose.translation[1] = pose.translation[1] + props.speed
                            panel:set_transform(pose)
                        end,
                    }
                    "#,
                ),
            )
            .unwrap());
        assert_eq!(runtime.tick(), 0);
        assert_eq!(runtime.object_translation("panel"), Some([0.0, 0.0, 0.0]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("panel"), Some([2.0, 0.0, 0.0]));

        let error = engine
            .reload_component_file(
                &mut runtime,
                GATE_FILE_ID,
                file_script(
                    r#"
                    return { on_start = function(transform, props, host)
                        local panel = host.find("panel")
                        local pose = panel.transform
                        pose.scale[1] = 2
                        panel:set_transform(pose)
                    end }
                    "#,
                ),
            )
            .unwrap_err();
        assert_eq!(error.phase, Phase::Commit);
        assert!(error.message.contains("panel collider"));
        assert_eq!(runtime.object_translation("panel"), Some([2.0, 0.0, 0.0]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("panel"), Some([4.0, 0.0, 0.0]));
    }

    #[test]
    fn transform_setter_respects_animator_and_kinematic_ownership() {
        use rot_scene::input::ButtonState;
        let mut scene = shared_module_scene("return { on_update = function() end }");
        scene.sources.insert(
            "controller".into(),
            script_source(
                "return { update = function(transform) return { transform = transform } end }",
                KINEMATIC_HOST_API_VERSION,
            ),
        );
        let collider = |id: &str, role, shape| ComponentInstance {
            id: id.into(),
            enabled: true,
            kind: Component::Collider {
                role,
                shape,
                offset: [0.0; 3],
            },
        };
        let box_shape = ColliderShape::Box {
            half_extents: [0.5; 3],
        };
        add_test_object(
            &mut scene,
            "actor",
            None,
            vec![
                collider(
                    "actor-collider",
                    ColliderRole::Blocker,
                    ColliderShape::Sphere { radius: 0.25 },
                ),
                ComponentInstance {
                    id: "kinematic-script".into(),
                    enabled: true,
                    kind: Component::Script {
                        source_kind: None,
                        movement: Default::default(),
                        binding: BehaviorBinding {
                            source_id: "controller".into(),
                            target: BindingTarget::Object("actor".into()),
                            enabled: true,
                            properties: BTreeMap::from([(
                                "checkpoint".into(),
                                rot_scene::PropertyValue::SensorRef("sensor".into()),
                            )]),
                            state_defaults: BTreeMap::new(),
                            commands: BTreeMap::new(),
                        },
                    },
                },
            ],
        );
        add_test_object(&mut scene, "wall-parent", None, Vec::new());
        add_test_object(
            &mut scene,
            "wall",
            Some("wall-parent"),
            vec![collider("wall-collider", ColliderRole::Blocker, box_shape)],
        );
        add_test_object(
            &mut scene,
            "sensor",
            None,
            vec![collider("sensor-collider", ColliderRole::Sensor, box_shape)],
        );
        add_test_object(
            &mut scene,
            "rig",
            None,
            vec![ComponentInstance {
                id: "animator".into(),
                enabled: true,
                kind: Component::Animator {
                    binding: AnimationBinding {
                        clip: "lift".into(),
                        looping: false,
                        targets: BTreeMap::from([("Panel".into(), "animated".into())]),
                    },
                    start_playback: AnimatorStart::OnButtonPress {
                        action_id: "slide".into(),
                    },
                    playback_speed: 1.0,
                },
            }],
        );
        add_test_object(&mut scene, "animated", Some("rig"), Vec::new());
        let clip = Clip::decode(
            br#"{"format":"rot-clip","version":1,"duration":1.0,"tracks":[
            {"node":"Panel","property":"translation","interpolation":"linear","times":[0.0,1.0],"values":[[0.0,0.0,0.0],[1.0,0.0,0.0]]}]}"#,
        ).unwrap();
        let mut engine = Engine::new(Limits::default()).unwrap();
        engine.set_clips(BTreeMap::from([("lift".into(), Arc::new(clip))]));
        for (target, protected_by, play_clip) in [
            ("animated", "Animator", false),
            ("animated", "Animator", true),
            ("actor", "kinematic actor", false),
            ("wall", "blocker", false),
            ("wall-parent", "blocker", false),
            ("sensor", "sensor", false),
        ] {
            scene.script_files.get_mut(GATE_FILE_ID).unwrap().text = format!(
                r#"
                local ticks = 0
                return {{ on_update = function(transform, props, dt, host)
                    local object = host.find("{target}")
                    local pose = object.transform
                    ticks = ticks + 1
                    if ticks == 3 then pose.translation[1] = pose.translation[1] + 1 end
                    object:set_transform(pose)
                end }}
                "#,
            );
            let mut runtime = engine.reset_component(&scene, 0).unwrap();
            let input = if play_clip {
                ActionFrame(BTreeMap::from([(
                    "slide".into(),
                    ActionValue::Button(ButtonState {
                        held: true,
                        pressed: true,
                        released: false,
                    }),
                )]))
            } else {
                ActionFrame::default()
            };
            engine.step_with_input(&mut runtime, &input).unwrap();
            engine.step(&mut runtime).unwrap();
            let committed = runtime.object_translation(target).unwrap();
            assert_eq!(committed[0] > 0.0, play_clip);
            let error = engine.step(&mut runtime).unwrap_err();
            assert_eq!(error.binding_id.as_deref(), Some("left-script"));
            assert!(error.line.is_some());
            assert!(error.message.contains(protected_by), "{error}");
            assert_eq!(runtime.tick(), 2);
            assert_eq!(runtime.object_translation(target), Some(committed));
        }
    }

    #[test]
    fn invalid_reload_retains_live_modules_and_failed_tick_can_be_repaired() {
        let scene = shared_module_scene(
            r#"
            local updates = 0
            return { on_update = function(transform, props)
                updates = updates + 1
                if updates == 3 and props.speed == 2 then
                    transform.scale[1] = 0
                end
                transform.translation[1] = updates
            end }
        "#,
        );
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        engine.step(&mut runtime).unwrap();
        let error = engine
            .reload_component_file(
                &mut runtime,
                GATE_FILE_ID,
                file_script("return {\n    on_update = function(\n}"),
            )
            .unwrap_err();
        assert_eq!(error.code, DiagnosticCode::Compile);
        assert_eq!(error.line, Some(3));
        assert!(error.message.contains("Scripts/gate.lua:3"));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 2);
        assert_eq!(runtime.object_translation("left"), Some([2.0, 0.0, 0.0]));

        let error = engine.step(&mut runtime).unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("right-script"));
        assert_eq!(runtime.tick(), 2);
        assert_eq!(runtime.object_translation("left"), Some([2.0, 0.0, 0.0]));
        assert_eq!(runtime.object_translation("right"), Some([2.0, 0.0, 0.0]));
        assert_eq!(engine.step(&mut runtime).unwrap_err().phase, Phase::Setup);

        let mut replacement = file_script(
            r#"
            local start
            return {
                on_start = function(transform)
                    start = transform.translation[1]
                    transform.translation[2] = 100
                end,
                on_update = function(transform, props)
                    transform.translation[1] = start + props.speed
                end
            }
        "#,
        );
        let wrong_asset = ScriptFile {
            asset_id: "other-file".into(),
            ..replacement.clone()
        };
        let error = engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, wrong_asset)
            .unwrap_err();
        assert!(error.message.contains("captured project asset"));
        replacement.path = "Assets/renamed gate.lua".into();
        assert!(engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, replacement)
            .unwrap());
        assert_eq!(runtime.tick(), 2);
        assert_eq!(runtime.object_translation("left"), Some([2.0, 0.0, 0.0]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 3);
        assert_eq!(runtime.object_translation("left"), Some([3.0, 0.0, 0.0]));
        assert_eq!(runtime.object_translation("right"), Some([4.0, 0.0, 0.0]));
    }

    #[test]
    fn dynamic_hierarchy_with_renderable_parent_spot_and_camera_updates_together() {
        let mut scene = base_scene("parent_motion");
        scene.sources.insert(
            "move_src".into(),
            script_source(
                r#"return {update = function(obj, props, dt)
                    obj.translation[1] = obj.translation[1] + 2.0 * dt
                    return obj
                end}"#,
                1,
            ),
        );
        scene.objects.push(ObjectNode {
            id: "parent".into(),
            name: "Parent".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![
                ComponentInstance {
                    id: "parent-mesh".into(),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.; 4],
                        material: None,
                    },
                },
                ComponentInstance {
                    id: "parent-script".into(),
                    enabled: true,
                    kind: Component::Script {
                        source_kind: None,
                        movement: Default::default(),
                        binding: BehaviorBinding {
                            source_id: "move_src".into(),
                            target: BindingTarget::Object("parent".into()),
                            enabled: true,
                            properties: BTreeMap::new(),
                            state_defaults: BTreeMap::new(),
                            commands: BTreeMap::new(),
                        },
                    },
                },
            ],
        });
        scene.objects.push(ObjectNode {
            id: "child".into(),
            name: "Child".into(),
            enabled: true,
            parent: Some("parent".into()),
            transform: Transform {
                translation: [0., 3., 0.],
                ..Default::default()
            },
            components: vec![
                ComponentInstance {
                    id: "child-mesh".into(),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [0.5, 0.5, 1., 1.],
                        material: None,
                    },
                },
                ComponentInstance {
                    id: "child-spot".into(),
                    enabled: true,
                    kind: Component::Light {
                        color: [1.; 3],
                        intensity: 1.0,
                        kind: LightKind::Spot {
                            range: 10.0,
                            inner_degrees: 15.0,
                            outer_degrees: 30.0,
                        },
                    },
                },
                ComponentInstance {
                    id: "child-cam".into(),
                    enabled: true,
                    kind: Component::Camera {
                        view: CameraView::transform_perspective(45.0),
                    },
                },
            ],
        });
        scene.gameplay_camera = Some("child-cam".into());

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 7).unwrap();
        assert!(!runtime.is_static_component());
        assert!(runtime.draw_frame().is_component_frame());
        assert_eq!(runtime.draw_frame().scene().objects.len(), 2);
        engine.step(&mut runtime).unwrap();
        let dt = Limits::default().fixed_dt as f32;
        assert!((runtime.object_translation("parent").unwrap()[0] - 2.0 * dt).abs() < 1e-6);
        let child_world = runtime.world_matrices().unwrap()[1].w_axis.truncate();
        assert!((child_world.x - f64::from(2.0 * dt)).abs() < 1e-5);
        assert!((child_world.y - 3.0).abs() < 1e-5);
        let spot_pos = runtime
            .draw_frame()
            .hierarchy()
            .unwrap()
            .lighting()
            .light("child-spot")
            .unwrap()
            .transform
            .translation;
        assert!((spot_pos[0] - 2.0 * dt).abs() < 1e-5);
        assert!((runtime.transform_camera().unwrap().eye[0] - 2.0 * dt).abs() < 1e-5);
    }

    #[test]
    fn disabled_subtree_hidden_renderer_and_disabled_animator_behave_correctly() {
        let mut scene = base_scene("disabled_items");
        scene.sources.insert(
            "noop".into(),
            script_source("return {update = function(obj) return obj end}", 1),
        );
        scene.objects.push(ObjectNode {
            id: "active".into(),
            name: "Active".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![
                ComponentInstance {
                    id: "hidden-mesh".into(),
                    enabled: false,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.; 4],
                        material: None,
                    },
                },
                ComponentInstance {
                    id: "disabled-anim".into(),
                    enabled: false,
                    kind: Component::Animator {
                        binding: AnimationBinding {
                            clip: "missing-clip".into(),
                            looping: true,
                            targets: BTreeMap::from([("A".into(), "active_child".into())]),
                        },
                        start_playback: AnimatorStart::OnPlay {},
                        playback_speed: 1.0,
                    },
                },
                ComponentInstance {
                    id: "active-script".into(),
                    enabled: true,
                    kind: Component::Script {
                        source_kind: None,
                        movement: Default::default(),
                        binding: BehaviorBinding {
                            source_id: "noop".into(),
                            target: BindingTarget::Object("active".into()),
                            enabled: true,
                            properties: BTreeMap::new(),
                            state_defaults: BTreeMap::new(),
                            commands: BTreeMap::new(),
                        },
                    },
                },
            ],
        });
        scene.objects.push(ObjectNode {
            id: "active_child".into(),
            name: "ActiveChild".into(),
            enabled: true,
            parent: Some("active".into()),
            transform: Transform::default(),
            components: vec![],
        });
        scene.objects.push(ObjectNode {
            id: "off_parent".into(),
            name: "OffParent".into(),
            enabled: false,
            parent: None,
            transform: Transform::default(),
            components: vec![],
        });
        scene.objects.push(ObjectNode {
            id: "off_child".into(),
            name: "OffChild".into(),
            enabled: true,
            parent: Some("off_parent".into()),
            transform: Transform::default(),
            components: vec![ComponentInstance {
                id: "off-child-mesh".into(),
                enabled: true,
                kind: Component::MeshRenderer {
                    geometry: Geometry::Cube,
                    tint: [1.; 4],
                    material: None,
                },
            }],
        });

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        // Both hidden-mesh (disabled component) and off-child-mesh (disabled ancestor) omitted.
        assert!(runtime.draw_frame().scene().objects.is_empty());
        // Disabled animator with missing clip did not prevent stepping.
        engine.step(&mut runtime).unwrap();
    }

    #[test]
    fn multiple_components_on_one_object_run_natively() {
        let mut scene = base_scene("multi_comp");
        scene.sources.insert(
            "move".into(),
            script_source(
                "return {update = function(obj) obj.translation[2] = obj.translation[2] + 0.5; return obj end}",
                1,
            ),
        );
        scene.objects.push(ObjectNode {
            id: "combo".into(),
            name: "Combo".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![
                ComponentInstance {
                    id: "mesh-a".into(),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.; 4],
                        material: None,
                    },
                },
                ComponentInstance {
                    id: "mesh-b".into(),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [0.5, 0.5, 0.5, 1.],
                        material: None,
                    },
                },
                ComponentInstance {
                    id: "col-a".into(),
                    enabled: true,
                    kind: Component::Collider {
                        role: ColliderRole::Blocker,
                        offset: [0.; 3],
                        shape: ColliderShape::Sphere { radius: 0.5 },
                    },
                },
                ComponentInstance {
                    id: "col-b".into(),
                    enabled: true,
                    kind: Component::Collider {
                        role: ColliderRole::Sensor,
                        offset: [0., 1., 0.],
                        shape: ColliderShape::Box {
                            half_extents: [0.5; 3],
                        },
                    },
                },
                ComponentInstance {
                    id: "light-a".into(),
                    enabled: true,
                    kind: Component::Light {
                        color: [1.; 3],
                        intensity: 0.5,
                        kind: LightKind::Point { range: 5.0 },
                    },
                },
                ComponentInstance {
                    id: "light-b".into(),
                    enabled: true,
                    kind: Component::Light {
                        color: [1., 0.5, 0.],
                        intensity: 0.5,
                        kind: LightKind::Point { range: 5.0 },
                    },
                },
                ComponentInstance {
                    id: "script".into(),
                    enabled: true,
                    kind: Component::Script {
                        source_kind: None,
                        movement: Default::default(),
                        binding: BehaviorBinding {
                            source_id: "move".into(),
                            target: BindingTarget::Object("combo".into()),
                            enabled: true,
                            properties: BTreeMap::new(),
                            state_defaults: BTreeMap::new(),
                            commands: BTreeMap::new(),
                        },
                    },
                },
            ],
        });
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 1).unwrap();
        assert_eq!(runtime.draw_frame().scene().objects.len(), 2);
        assert_eq!(
            runtime.draw_frame().hierarchy().unwrap().matrices().len(),
            2
        );
        engine.step(&mut runtime).unwrap();
        let draw = runtime.draw_frame().hierarchy().unwrap();
        assert!(
            (draw
                .lighting()
                .light("light-a")
                .unwrap()
                .transform
                .translation[1]
                - 0.5)
                .abs()
                < 1e-6
        );
        assert!(
            (draw
                .lighting()
                .light("light-b")
                .unwrap()
                .transform
                .translation[1]
                - 0.5)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn over_256_objects_with_script_and_animator_and_play_check_pass() {
        let mut scene = base_scene("dense_dynamic");
        let clip = Clip::decode(
            br#"{"format":"rot-clip","version":1,"duration":1.0,"tracks":[
            {"node":"Arm","property":"translation","interpolation":"linear","times":[0.0,1.0],"values":[[0.0,0.0,0.0],[0.0,1.0,0.0]]}]}"#,
        )
        .unwrap();
        scene.sources.insert(
            "runner".into(),
            script_source(
                "return {update = function(obj) obj.translation[1] = obj.translation[1] + 1.0; return obj end}",
                1,
            ),
        );
        scene.objects.push(ObjectNode {
            id: "rig".into(),
            name: "Rig".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![ComponentInstance {
                id: "rig-anim".into(),
                enabled: true,
                kind: Component::Animator {
                    binding: AnimationBinding {
                        clip: "lift".into(),
                        looping: false,
                        targets: BTreeMap::from([("Arm".into(), "arm".into())]),
                    },
                    start_playback: AnimatorStart::OnPlay {},
                    playback_speed: 1.0,
                },
            }],
        });
        scene.objects.push(ObjectNode {
            id: "arm".into(),
            name: "Arm".into(),
            enabled: true,
            parent: Some("rig".into()),
            transform: Transform::default(),
            components: vec![ComponentInstance {
                id: "arm-mesh".into(),
                enabled: true,
                kind: Component::MeshRenderer {
                    geometry: Geometry::Cube,
                    tint: [1.; 4],
                    material: None,
                },
            }],
        });
        scene.objects.push(ObjectNode {
            id: "mover".into(),
            name: "Mover".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![
                ComponentInstance {
                    id: "mover-mesh".into(),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.; 4],
                        material: None,
                    },
                },
                ComponentInstance {
                    id: "mover-script".into(),
                    enabled: true,
                    kind: Component::Script {
                        source_kind: None,
                        movement: Default::default(),
                        binding: BehaviorBinding {
                            source_id: "runner".into(),
                            target: BindingTarget::Object("mover".into()),
                            enabled: true,
                            properties: BTreeMap::new(),
                            state_defaults: BTreeMap::new(),
                            commands: BTreeMap::new(),
                        },
                    },
                },
            ],
        });
        for index in 0..397 {
            scene.objects.push(ObjectNode {
                id: format!("prop-{index}"),
                name: format!("Prop {index}"),
                enabled: true,
                parent: None,
                transform: Transform::default(),
                components: vec![ComponentInstance {
                    id: format!("prop-mesh-{index}"),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.; 4],
                        material: None,
                    },
                }],
            });
        }

        let mut engine = Engine::new(Limits::default()).unwrap();
        let clips = BTreeMap::from([("lift".into(), Arc::new(clip))]);
        engine.set_clips(clips.clone());
        let mut runtime = engine.reset_component(&scene, 1).unwrap();
        assert_eq!(runtime.draw_frame().scene().objects.len(), 399);
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("mover"), Some([1.0, 0.0, 0.0]));
        assert!(runtime.object_translation("arm").unwrap()[1] > 0.0);

        let identity = CheckIdentity::for_saved_scene(
            "1".repeat(64),
            &scene.actions,
            "2".repeat(64),
            crate::DEFAULT_FIXED_TICK.as_nanos() as u64,
        )
        .unwrap();
        let case = PlayCheck {
            format: play_check::FORMAT.into(),
            version: play_check::VERSION,
            name: "Component >256".into(),
            identity: identity.clone(),
            frames: vec![ActionFrame::default(); 3],
            expectations: vec![Expectation {
                select: Selection::Translation {
                    object_id: "mover".into(),
                },
                comparison: Comparison::Equals {
                    expected: rot_scene::StateValue::Vec3([3.0, 0.0, 0.0]),
                    tolerance: Some(1e-6),
                },
            }],
        };
        let report = play_check::run_component(
            &scene,
            &case,
            identity,
            &clips,
            Arc::new(AtomicBool::new(false)),
        );
        assert_eq!(report.status, play_check::Status::Passed, "{report:?}");
    }

    #[test]
    fn quiet_5000_object_step_preserves_population_and_draw_frame_arcs() {
        let mut scene = base_scene("dense_static");
        scene.objects = (0..5_000)
            .map(|index| ObjectNode {
                id: format!("node-{index}"),
                name: format!("Node {index}"),
                enabled: true,
                parent: (index > 0).then(|| "node-0".into()),
                transform: Transform::default(),
                components: vec![ComponentInstance {
                    id: format!("mesh-{index}"),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.; 4],
                        material: None,
                    },
                }],
            })
            .collect();

        scene.objects[0].components.push(test_collider(
            "quiet-probe",
            ColliderRole::Blocker,
            ColliderShape::Sphere { radius: 0.25 },
        ));
        let builds = super::super::spatial_host::component_world_builds;
        let initial_builds = builds();

        let mut engine = Engine::new(Limits::default()).unwrap();
        for has_script in [false, true] {
            if has_script {
                let mut quiet = shared_module_scene("return {on_update = function() end}");
                let mut component = quiet.objects[0].components.pop().unwrap();
                let Component::Script { binding, .. } = &mut component.kind else {
                    unreachable!("test file script")
                };
                binding.target = BindingTarget::Object("node-0".into());
                scene.objects[0].components.push(component);
                scene.script_files = quiet.script_files;
            }
            let mut runtime = engine.reset_component(&scene, 5).unwrap();
            assert_eq!(runtime.is_static_component(), !has_script);
            let initial_scene = runtime.draw_frame().snapshot();
            let initial_hierarchy = Arc::clone(runtime.draw_frame().hierarchy().unwrap());
            let world = runtime.component_world.as_ref().unwrap();
            let initial_population = Arc::clone(&world.scene);
            let initial_local = Arc::clone(&world.local_transforms);
            let initial_colliders = Arc::clone(&world.colliders);
            let initial_rotations = Arc::clone(&world.world_rotations);
            engine.step(&mut runtime).unwrap();
            let world = runtime.component_world.as_ref().unwrap();
            assert_eq!(runtime.tick(), 1);
            assert!(Arc::ptr_eq(&initial_population, &world.scene));
            assert!(Arc::ptr_eq(&initial_local, &world.local_transforms));
            assert!(Arc::ptr_eq(&initial_colliders, &world.colliders));
            assert!(Arc::ptr_eq(&initial_rotations, &world.world_rotations));
            assert_eq!(builds(), initial_builds);
            assert!(Arc::ptr_eq(
                &initial_scene,
                &runtime.draw_frame().snapshot()
            ));
            assert!(Arc::ptr_eq(
                &initial_hierarchy,
                runtime.draw_frame().hierarchy().unwrap()
            ));
        }
    }

    #[test]
    fn failed_component_step_rolls_back_transforms_state_and_tick() {
        let mut scene = base_scene("rollback");
        scene.sources.insert(
            "bad".into(),
            script_source(
                r#"return {update = function(obj)
                    if obj.translation[1] >= 1.0 then
                        return {translation = {1/0, 0, 0}, rotation = {0,0,0}, scale = {1,1,1}}
                    end
                    obj.translation[1] = obj.translation[1] + 1.0
                    return obj
                end}"#,
                1,
            ),
        );
        scene.objects.push(ObjectNode {
            id: "mover".into(),
            name: "Mover".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![ComponentInstance {
                id: "mover-script".into(),
                enabled: true,
                kind: Component::Script {
                    source_kind: None,
                    movement: Default::default(),
                    binding: BehaviorBinding {
                        source_id: "bad".into(),
                        target: BindingTarget::Object("mover".into()),
                        enabled: true,
                        properties: BTreeMap::new(),
                        state_defaults: BTreeMap::new(),
                        commands: BTreeMap::new(),
                    },
                },
            }],
        });
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 1);
        assert_eq!(runtime.object_translation("mover"), Some([1.0, 0.0, 0.0]));
        let err = engine.step(&mut runtime).unwrap_err();
        assert_eq!(err.code, DiagnosticCode::InvalidOutput);
        assert_eq!(runtime.tick(), 1);
        assert_eq!(runtime.object_translation("mover"), Some([1.0, 0.0, 0.0]));
    }

    fn persistent_kinematic_scene(script_text: &str) -> ComponentScene {
        use rot_scene::input::{ActionDefinition, ActionKind, InputKey};
        use rot_scene::PropertyValue;
        let mut scene = base_scene("kinematic-persistent");
        scene.actions.insert(
            "step_right".into(),
            ActionDefinition {
                label: None,
                kind: ActionKind::Button {
                    keys: vec![InputKey::ArrowRight],
                    primary_mouse: false,
                },
            },
        );
        scene
            .script_files
            .insert(GATE_FILE_ID.into(), file_script(script_text));
        scene.objects.push(ObjectNode {
            id: "mover".into(),
            name: "KinematicMover".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![
                ComponentInstance {
                    id: "mover-collider".into(),
                    enabled: true,
                    kind: Component::Collider {
                        role: ColliderRole::Blocker,
                        offset: [0.0; 3],
                        shape: ColliderShape::Sphere { radius: 0.5 },
                    },
                },
                ComponentInstance {
                    id: "mover-script".into(),
                    enabled: true,
                    kind: Component::Script {
                        source_kind: None,
                        movement: ScriptMovement::KinematicSphere,
                        binding: BehaviorBinding {
                            source_id: GATE_FILE_ID.into(),
                            target: BindingTarget::Object("mover".into()),
                            enabled: true,
                            properties: BTreeMap::from([
                                ("checkpoint".into(), PropertyValue::SensorRef("zone".into())),
                                (
                                    "chime".into(),
                                    PropertyValue::SoundRef(rot_scene::SoundRef {
                                        asset_id: Some("chime-wav".into()),
                                        gain: 0.8,
                                        playback: Default::default(),
                                    }),
                                ),
                            ]),
                            state_defaults: BTreeMap::new(),
                            commands: BTreeMap::new(),
                        },
                    },
                },
            ],
        });
        scene.objects.push(ObjectNode {
            id: "wall".into(),
            name: "BlockerWall".into(),
            enabled: true,
            parent: None,
            transform: Transform {
                translation: [1.5, 0.0, 0.0],
                ..Transform::default()
            },
            components: vec![ComponentInstance {
                id: "wall-collider".into(),
                enabled: true,
                kind: Component::Collider {
                    role: ColliderRole::Blocker,
                    offset: [0.0; 3],
                    shape: ColliderShape::Box {
                        half_extents: [0.25, 1.0, 1.0],
                    },
                },
            }],
        });
        scene.objects.push(ObjectNode {
            id: "zone".into(),
            name: "CheckpointZone".into(),
            enabled: true,
            parent: None,
            transform: Transform {
                translation: [1.0, 0.0, 0.0],
                ..Transform::default()
            },
            components: vec![ComponentInstance {
                id: "zone-collider".into(),
                enabled: true,
                kind: Component::Collider {
                    role: ColliderRole::Sensor,
                    offset: [0.0; 3],
                    shape: ColliderShape::Box {
                        half_extents: [0.3, 1.0, 1.0],
                    },
                },
            }],
        });
        scene.objects.push(ObjectNode {
            id: "marker".into(),
            name: "MarkerTemplate".into(),
            enabled: false,
            parent: None,
            transform: Transform::default(),
            components: Vec::new(),
        });
        scene
    }

    #[test]
    fn persistent_kinematic_lua_moves_senses_plays_audio_and_exposes_object_identity() {
        let scene = persistent_kinematic_scene(
            r#"local ticks, marker_ref = 0, nil
local function check(ok)
    if not ok then local missing = nil; missing() end
end
return {
    on_start = function(o, props, host)
        local me = host.find("mover")
        check(me and me.id == "mover" and me.name == "KinematicMover")
        local template = host.find("MarkerTemplate")
        check(template and template.id == "marker" and template.name == "MarkerTemplate")
        marker_ref = host.spawn(template, template.transform)
        check(marker_ref ~= nil)
    end,
    on_update = function(o, props, dt, host)
        ticks = ticks + 1
        local slide = host.move_and_slide({0.45, 0, 0})
        o.translation = slide.suggested_translation
        local hit = host.sensor_crossing("checkpoint")
        if hit.state == "entered" then
            host.play_sound("chime")
            if marker_ref then
                check(marker_ref.id ~= nil and marker_ref.name == "MarkerTemplate")
                host.destroy(marker_ref)
            end
        elseif ticks == 3 and marker_ref then
            check(marker_ref.id == nil and marker_ref.name == nil)
            local wall = host.find("wall")
            check(wall:component("Light") == nil)
        end
    end,
}"#,
        );

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        // Tick 0 spawned marker_ref, so total objects = 5.
        assert_eq!(runtime.component_scene().unwrap().objects.len(), 5);

        // Tick 1: mover reaches x = 0.45, overlapping zone [0.3..0.9] -> "entered", plays cue once, destroys marker.
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 1);
        assert_eq!(runtime.component_scene().unwrap().objects.len(), 4);
        assert_eq!(runtime.object_translation("mover"), Some([0.45, 0.0, 0.0]));
        assert_eq!(
            runtime.audio_batch().unwrap().cues.len(),
            1,
            "entered sensor should emit one Sound Cue"
        );
        assert!(runtime.query_observations().iter().any(|obs| matches!(
            &obs.query,
            crate::spatial_host::QueryResult::SensorCrossing { state, .. } if *state == "entered"
        )));

        // Tick 2: mover stops just ahead of wall at x ~ 0.75 (inside zone [0.7..1.3]) -> "inside", no second audio cue.
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 2);
        let x2 = runtime.object_translation("mover").unwrap()[0];
        assert!(
            x2 > 0.7 && x2 <= 0.75,
            "expected mover near wall clearance <= 0.75, got {x2}"
        );
        assert!(runtime.query_observations().iter().any(|obs| matches!(
            &obs.query,
            crate::spatial_host::QueryResult::SensorCrossing { state, .. } if *state == "inside"
        )));
        assert!(runtime.audio_batch().is_none());

        // Tick 3: destroyed marker_ref.id and marker_ref.name are nil; local `ticks` counter preserved.
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 3);
    }

    #[test]
    fn persistent_kinematic_rejects_tampered_writes_and_supports_live_reload() {
        let mut scene = persistent_kinematic_scene(
            r#"return {
    on_update = function(o, props, dt, host)
        local slide = host.move_and_slide({0.2, 0, 0})
        o.translation = slide.suggested_translation
    end,
}"#,
        );
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 1);
        assert_eq!(runtime.object_translation("mover"), Some([0.2, 0.0, 0.0]));

        // Live reload replaces the persistent kinematic module and preserves pose and tick.
        let reloaded = engine
            .reload_component_file(
                &mut runtime,
                GATE_FILE_ID,
                file_script(
                    r#"local count = 0
return {
    on_start = function(o, props, host)
        if not (host.move_and_slide and host.sensor_crossing) then
            local missing = nil
            missing()
        end
    end,
    on_update = function(o, props, dt, host)
        count = count + 1
        local slide = host.move_and_slide({0.1, 0, 0})
        o.translation = slide.suggested_translation
        if count == 2 then
            o.translation[1] = 99.0 -- tamper without solver authorization
            host.play_sound("chime")
        end
    end,
}"#,
                ),
            )
            .unwrap();
        assert!(reloaded);
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 2);
        assert!((runtime.object_translation("mover").unwrap()[0] - 0.3).abs() < 1e-5);

        // Tampering on count==2 fails closed and preserves tick=2, translation=0.3, and zero audio.
        let err = engine.step(&mut runtime).unwrap_err();
        assert!(
            err.message.contains("exact move_and_slide translation"),
            "{}",
            err.message
        );
        assert_eq!(runtime.tick(), 2);
        assert!((runtime.object_translation("mover").unwrap()[0] - 0.3).abs() < 1e-5);
        assert!(runtime.audio_batch().is_none());

        // Broken reload (syntax/runtime) is rejected while keeping committed state intact, then clean reload recovers.
        let invalid = engine
            .reload_component_file(
                &mut runtime,
                GATE_FILE_ID,
                file_script("return { on_start = function(o) o.translation[1] = 5 end }"),
            )
            .unwrap_err();
        assert!(invalid.message.contains("exact move_and_slide translation"));
        assert_eq!(runtime.tick(), 2);

        scene.script_files.get_mut(GATE_FILE_ID).unwrap().text = r#"return {
    on_update = function(o, props, dt, host)
        host.move_and_slide({0, 0, 0})
        host.move_and_slide({0, 0, 0})
    end,
}"#
        .into();
        let mut rt_double = engine.reset_component(&scene, 0).unwrap();
        let err_double = engine.step(&mut rt_double).unwrap_err();
        assert!(err_double.message.contains("only one move_and_slide"));

        scene.script_files.get_mut(GATE_FILE_ID).unwrap().text = r#"return {
    on_update = function(o, props, dt, host)
        local p = o
        host.find("mover"):set_transform({translation={1,0,0}, rotation=p.rotation, scale=p.scale})
    end,
}"#
        .into();
        let mut rt_set = engine.reset_component(&scene, 0).unwrap();
        let err_set = engine.step(&mut rt_set).unwrap_err();
        assert!(err_set.message.contains("outside its controller"));
    }
}
