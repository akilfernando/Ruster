//! Native flat-array runtime for `ComponentScene`, retiring projection caps.
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use glam::{DMat4, DQuat};
use rot_scene::component_scene::{AnimatorStart, Component, ComponentScene, ObjectHierarchy};
use rot_scene::input::{ActionFrame, ActionValue, ValidatedActionFrame};
use rot_scene::{Authoring, BindingTarget, Clip, ComponentDrawPlan, Transform};

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
    pub(crate) local_transforms: Vec<Transform>,
    pub(crate) world_matrices: Vec<DMat4>,
    pub(crate) world_rotations: Vec<DQuat>,
    pub(crate) scripts: Vec<ComponentScriptSlot>,
    pub(crate) animations: Vec<ResolvedComponentAnimation>,
    pub(crate) colliders: Vec<ComponentColliderSlot>,
    pub(crate) draw_plan: Arc<ComponentDrawPlan>,
    pub(crate) audio_emitters: Option<Arc<BTreeMap<String, usize>>>,
}

impl ComponentWorld {
    pub(crate) fn is_static(&self) -> bool {
        self.scripts.is_empty() && self.animations.is_empty()
    }
}

fn diag(code: DiagnosticCode, phase: Phase, message: &str) -> Diagnostic {
    Diagnostic::plain(code, phase, message)
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
                    Component::Script { binding, .. } if binding.enabled => {
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
                        });
                    }
                    Component::Collider {
                        role,
                        offset,
                        shape,
                    } => {
                        colliders.push(ComponentColliderSlot {
                            owner_index,
                            owner_id: object.id.clone(),
                            collider: rot_scene::Collider {
                                enabled: true,
                                role: *role,
                                offset: *offset,
                                shape: *shape,
                            },
                        });
                    }
                    _ => {}
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
        let draw_frame = draw_plan
            .build_frame(scene, &world_matrices, &world_rotations)
            .map_err(|error| {
                diag(
                    DiagnosticCode::InvalidOutput,
                    Phase::Setup,
                    &error.to_string(),
                )
            })?;

        let kinematic_owner =
            kinematic_owner_component(scene, &hierarchy, &object_indices, &animated_objects)
                .map_err(|message| {
                    diag(DiagnosticCode::InvalidAuthoring, Phase::Setup, &message)
                })?;

        let has_sound_binding = scripts.iter().any(|slot| {
            authoring.bindings[&slot.component_id]
                .properties
                .values()
                .any(|property| matches!(property, rot_scene::PropertyValue::SoundRef(_)))
        });
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
            let queries = self.query_invocation_component(
                ComponentQueryInput {
                    colliders: &colliders,
                    local: &local_transforms,
                    world: &world_matrices,
                    rotations: &world_rotations,
                    parents: &hierarchy.parents,
                },
                &authoring,
                0,
                revision,
                kinematic_owner.as_ref(),
            )?;
            let audio_cues = RefCell::new(Vec::new());
            let mut trial_transforms: Option<Vec<Transform>> = None;
            for slot in &scripts {
                let output = self.evaluate_component_binding(
                    slot,
                    &authoring,
                    &local_transforms,
                    &object_indices,
                    0.0,
                    &input,
                    queries.as_ref(),
                    has_sound_binding.then_some(&audio_cues),
                    &state,
                )?;
                if let Some(transform) = output.transform {
                    if !same_transform(&local_transforms[slot.owner_index], &transform) {
                        trial_transforms.get_or_insert_with(|| local_transforms.clone())
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
        let scene_arc = Arc::new(scene.clone());
        Ok(Runtime {
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
                scene: scene_arc,
                hierarchy,
                object_indices,
                local_transforms,
                world_matrices,
                world_rotations,
                scripts,
                animations,
                colliders,
                draw_plan,
                audio_emitters,
            }),
        })
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

        self.begin()?;
        self.begin_debug(
            runtime.tick,
            next,
            runtime.commit_revision,
            commit_revision,
            false,
        );
        let queries = self.query_invocation_component(
            ComponentQueryInput {
                colliders: &world.colliders,
                local: &world.local_transforms,
                world: &world.world_matrices,
                rotations: &world.world_rotations,
                parents: &world.hierarchy.parents,
            },
            &runtime.authoring,
            runtime.tick,
            runtime.draw_frame.scene().revision,
            runtime.kinematic_owner.as_ref(),
        )?;
        let mut staged_transforms: Option<Vec<Transform>> = None;
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
                staged_transforms.get_or_insert_with(|| world.local_transforms.clone());
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

        let audio_cues = RefCell::new(Vec::new());
        for slot in &world.scripts {
            let output = self.evaluate_component_binding(
                slot,
                &runtime.authoring,
                &world.local_transforms,
                &world.object_indices,
                self.limits.fixed_dt,
                &validated,
                queries.as_ref(),
                runtime.has_sound_binding.then_some(&audio_cues),
                &runtime.state,
            )?;
            if let Some(transform) = output.transform {
                if !same_transform(&world.local_transforms[slot.owner_index], &transform) {
                    staged_transforms.get_or_insert_with(|| world.local_transforms.clone())
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

        let staged_frames = if let Some(ref next_local) = staged_transforms {
            let mut next_world = Vec::with_capacity(next_local.len());
            let mut next_rot = Vec::with_capacity(next_local.len());
            world
                .scene
                .resolve_world_matrices_into(&world.hierarchy, next_local, &mut next_world)
                .map_err(|error| {
                    diag(
                        DiagnosticCode::InvalidOutput,
                        Phase::Commit,
                        &error.to_string(),
                    )
                })?;
            world
                .scene
                .resolve_world_rotations_into(&world.hierarchy, next_local, &mut next_rot);
            validate_component_colliders(
                &world.scene,
                &world.colliders,
                next_local,
                &next_world,
                &next_rot,
                &world.hierarchy.parents,
                Phase::Commit,
            )?;
            let next_draw = world
                .draw_plan
                .build_frame(&world.scene, &next_world, &next_rot)
                .map_err(|error| {
                    diag(
                        DiagnosticCode::InvalidOutput,
                        Phase::Commit,
                        &error.to_string(),
                    )
                })?;
            Some((next_world, next_rot, next_draw))
        } else {
            None
        };

        staged_state.validate_total()?;
        let transform_camera = staged_frames
            .as_ref()
            .map(|(_, _, draw)| draw.transform_camera())
            .unwrap_or_else(|| runtime.transform_camera());
        let mut cues = audio_cues.into_inner();
        if let Some(emitters) = &world.audio_emitters {
            let active_world = staged_frames
                .as_ref()
                .map_or(world.world_matrices.as_slice(), |(w, _, _)| w.as_slice());
            audio::resolve_cues_component(&mut cues, emitters, transform_camera, active_world);
        }
        self.check_cancelled()?;
        let observations =
            queries.map_or_else(Vec::new, |queries| queries.observations.into_inner());
        runtime.audio_batch = (!cues.is_empty()).then_some(super::AudioBatch {
            tick: next,
            commit_revision,
            cues,
        });
        if let (Some(next_local), Some((next_world, next_rot, next_draw))) =
            (staged_transforms, staged_frames)
        {
            runtime.draw_frame = next_draw;
            let world_mut = runtime
                .component_world
                .as_mut()
                .expect("component world checked above");
            world_mut.local_transforms = next_local;
            world_mut.world_matrices = next_world;
            world_mut.world_rotations = next_rot;
        }
        for index in pressed {
            runtime.animation_starts[index] = Some(next);
        }
        runtime.tick = next;
        runtime.commit_revision = commit_revision;
        runtime.last_step_commit_revision = Some(commit_revision);
        runtime.query_observations = observations;
        runtime.state = staged_state;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_component_binding(
        &mut self,
        slot: &ComponentScriptSlot,
        authoring: &Authoring,
        local_transforms: &[Transform],
        object_indices: &HashMap<String, usize>,
        dt: f64,
        frame: &ValidatedActionFrame,
        spatial: Option<&super::spatial_host::QueryInvocation>,
        audio: Option<&RefCell<Vec<super::AudioCue>>>,
        state: &RuntimeState,
    ) -> Result<BindingOutput, Diagnostic> {
        let binding = &authoring.bindings[&slot.component_id];
        let source = authoring.sources.get(&binding.source_id);
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
            let main = self
                .compile(&binding.source_id, source)
                .map_err(|mut failure| {
                    if let Some(path) = source.label.as_deref().filter(|path| {
                        binding.source_id.starts_with("asset-lua-") && path.ends_with(".lua")
                    }) {
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
            let result = self.evaluate_with_input(
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
            );
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
            actions: BTreeMap::new(),
            hud: Default::default(),
            gameplay_camera: None,
        }
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
    fn static_5000_object_step_preserves_draw_frame_arcs_with_zero_clones() {
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

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 5).unwrap();
        assert!(runtime.is_static_component());
        let initial_scene = runtime.draw_frame().snapshot();
        let initial_hierarchy = Arc::clone(runtime.draw_frame().hierarchy().unwrap());
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 1);
        assert!(Arc::ptr_eq(
            &initial_scene,
            &runtime.draw_frame().snapshot()
        ));
        assert!(Arc::ptr_eq(
            &initial_hierarchy,
            runtime.draw_frame().hierarchy().unwrap()
        ));
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
}
