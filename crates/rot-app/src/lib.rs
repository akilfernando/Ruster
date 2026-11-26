//! Bounded Linux application host: application-owned behavior, shared Vulkan rendering.
#[cfg(feature = "audio-output")]
pub mod audio;
pub mod creation;
pub use rot_render::{
    player_view_projection, transform_camera_output_projection, transform_camera_projection,
};
mod native_actions;
pub use native_actions::NativeActionInput;

mod host;
pub mod hud;
mod resources;
#[cfg(test)]
mod tests;

pub use eframe::egui;
pub use host::{run, run_with_deferred_scene_renderer, HostOptions};
pub use resources::ResourceRoot;

use rot_assets::Mesh;
use rot_scene::Scene;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

pub type MeshMap = BTreeMap<String, Arc<Mesh>>;
pub type TextureMap = BTreeMap<String, Arc<rot_assets::Texture>>;

fn checked_mesh_bytes(current: usize, additional: usize) -> Result<usize, String> {
    current
        .checked_add(additional)
        .filter(|bytes| *bytes <= rot_assets::MAX_RESOLVED_BYTES)
        .ok_or_else(|| {
            format!(
                "application meshes exceed {} MiB resolved geometry",
                rot_assets::MAX_RESOLVED_BYTES / (1024 * 1024)
            )
        })
}

fn checked_texture_bytes(current: usize, additional: usize) -> Result<usize, String> {
    current
        .checked_add(additional)
        .filter(|bytes| *bytes <= rot_assets::MAX_TEXTURE_RESOLVED_BYTES)
        .ok_or_else(|| "application textures exceed 256 MiB resolved payload".into())
}

/// Validated immutable mesh snapshots alongside application-owned scene state.
/// Construct a new Content to replace meshes; this keeps retained GPU identities stable.
#[derive(Clone)]
pub struct Content {
    validation: std::sync::OnceLock<Result<(), String>>,
    transform_camera: Option<rot_scene::component_scene::TransformCameraPose>,
    scene: ContentScene,
    meshes: MeshMap,
    textures: TextureMap,
}

#[derive(Clone)]
enum ContentScene {
    Behavior(rot_scene::SceneDrawFrame),
}

impl Content {
    pub fn new(scene: Scene, meshes: MeshMap) -> Result<Self, String> {
        Self::with_textures(scene, meshes, TextureMap::new())
    }

    /// Capture validated immutable texture/mesh payloads once; runtime edits only
    /// revalidate authored references, not every pixel or vertex on each UI pass.
    pub fn with_textures(
        scene: Scene,
        meshes: MeshMap,
        textures: TextureMap,
    ) -> Result<Self, String> {
        Self::with_scene(
            ContentScene::Behavior(rot_scene::SceneDrawFrame::new(scene, None)),
            meshes,
            textures,
        )
    }

    /// Construct application content from an already-validated component runtime draw frame.
    pub fn from_component_frame(
        frame: rot_scene::SceneDrawFrame,
        meshes: MeshMap,
        textures: TextureMap,
    ) -> Result<Self, String> {
        Self::with_scene(ContentScene::Behavior(frame), meshes, textures)
    }

    fn with_scene(
        scene: ContentScene,
        meshes: MeshMap,
        textures: TextureMap,
    ) -> Result<Self, String> {
        if meshes.keys().any(|id| textures.contains_key(id)) {
            return Err("application mesh/texture asset IDs must be distinct".into());
        }
        for (id, mesh) in &meshes {
            rot_assets::validate_mesh(mesh).map_err(|e| format!("mesh {id}: {e}"))?;
        }
        meshes.values().try_fold(0, |bytes, mesh| {
            checked_mesh_bytes(bytes, mesh.payload_bytes())
        })?;
        for (id, texture) in &textures {
            rot_scene::validate_id(id).map_err(|error| error.to_string())?;
            rot_assets::validate_texture(texture)
                .map_err(|error| format!("texture {id}: {error}"))?;
        }
        textures.values().try_fold(0, |bytes, texture| {
            checked_texture_bytes(bytes, texture.payload_bytes())
        })?;
        let content = Self {
            validation: Default::default(),
            transform_camera: None,
            scene,
            meshes,
            textures,
        };
        content.validate()?;
        Ok(content)
    }

    pub fn scene(&self) -> &Scene {
        let ContentScene::Behavior(scene) = &self.scene;
        scene.scene()
    }

    pub fn draw_frame(&self) -> &rot_scene::SceneDrawFrame {
        let ContentScene::Behavior(scene) = &self.scene;
        scene
    }

    pub fn has_player_camera(&self) -> bool {
        let ContentScene::Behavior(scene) = &self.scene;
        self.transform_camera.is_some()
            || scene.transform_camera().is_some()
            || scene.scene().player_view.is_some()
            || scene
                .hierarchy()
                .and_then(|hierarchy| hierarchy.player_view())
                .is_some()
    }

    pub fn transform_camera(&self) -> Option<rot_scene::component_scene::TransformCameraPose> {
        let ContentScene::Behavior(scene) = &self.scene;
        self.transform_camera.or(scene.transform_camera())
    }

    /// Edit legacy behavior scene data.
    pub fn scene_mut(&mut self) -> &mut Scene {
        let ContentScene::Behavior(scene) = &mut self.scene;
        self.validation.take();
        scene.scene_mut()
    }
    pub fn set_behavior_draw_frame(&mut self, frame: &rot_scene::SceneDrawFrame) {
        let ContentScene::Behavior(scene) = &mut self.scene;
        if !std::ptr::eq(scene.scene(), frame.scene()) {
            self.validation.take();
        }
        *scene = frame.clone();
    }

    pub fn set_transform_camera(
        &mut self,
        camera: Option<rot_scene::component_scene::TransformCameraPose>,
    ) {
        self.transform_camera = camera;
    }

    pub fn meshes(&self) -> &MeshMap {
        &self.meshes
    }

    pub fn textures(&self) -> &TextureMap {
        &self.textures
    }

    /// Check runtime scene mutations without scanning the immutable mesh payload again.
    pub fn validate(&self) -> Result<(), String> {
        let ContentScene::Behavior(scene) = &self.scene;
        self.validation
            .get_or_init(|| {
                if scene.is_component_frame() {
                    scene
                        .scene()
                        .lighting
                        .validate_clustered()
                        .map_err(|error| format!("application scene: {error}"))?;
                } else {
                    scene
                        .scene()
                        .validate()
                        .map_err(|error| format!("application scene: {error}"))?;
                }
                self.validate_references()
            })
            .clone()
    }

    fn validate_references(&self) -> Result<(), String> {
        let scene = self.scene();
        for object in &scene.objects {
            if let Some(material) = object
                .material
                .as_ref()
                .and_then(|id| scene.materials.get(id))
            {
                for (texture, role) in [
                    (
                        &material.base_color_texture,
                        rot_assets::TextureRole::BaseColor,
                    ),
                    (
                        &material.roughness_texture,
                        rot_assets::TextureRole::Roughness,
                    ),
                    (
                        &material.metallic_texture,
                        rot_assets::TextureRole::Metallic,
                    ),
                    (&material.normal_texture, rot_assets::TextureRole::Normal),
                    (
                        &material.emissive_texture,
                        rot_assets::TextureRole::Emissive,
                    ),
                    (
                        &material.occlusion_texture,
                        rot_assets::TextureRole::Occlusion,
                    ),
                ] {
                    let Some(texture) = texture else {
                        continue;
                    };
                    let accepted = self.textures.get(texture).ok_or_else(|| format!("object {} requires missing texture {texture}; load accepted texture content before starting", object.id))?;
                    if accepted.role != role {
                        return Err(format!(
                            "object {} texture {texture} has the wrong role for its material slot",
                            object.id
                        ));
                    }
                    if !rot_render::object_has_complete_uvs(object, &self.meshes) {
                        return Err(format!(
                            "object {} needs a mesh with complete UVs for its textured material",
                            object.id
                        ));
                    }
                }
            }
            if let Some(id) = &object.mesh {
                if !self.meshes.contains_key(id) {
                    return Err(format!(
                        "object {} references missing mesh {id}; load that mesh before starting",
                        object.id
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Explicit fixed step; the host never passes wall-clock time into application behavior.
#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub tick: u64,
    pub dt: Duration,
}

/// Keyboard state after UI focus/consumption. Space is reserved for the host's Run/Pause.
#[derive(Clone, Debug, Default)]
pub struct InputFrame {
    pub pressed: Vec<egui::Key>,
    pub held: Vec<egui::Key>,
    pub modifiers: egui::Modifiers,
}

/// Current-frame overlay ownership, established before scene navigation/input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewportInputOwnership {
    pub pointer: bool,
    pub keyboard: bool,
    pub keep_game_keyboard: bool,
}

/// Application state and rules remain independent from the host and editor.
/// Callbacks must preserve valid state on error; this host cannot roll back arbitrary state.
pub trait Application: 'static {
    fn content(&self) -> &Content;
    fn tick(&mut self, step: Step) -> Result<(), String>;
    fn reset(&mut self) -> Result<(), String>;
    /// A scene switch starts a new simulation. Drop old catch-up debt so the
    /// first presented scene is its initial committed frame.
    fn take_scene_switch(&mut self) -> bool {
        false
    }
    /// A committed host transition. Live output may pause or resume here.
    fn playback_changed(&mut self, _playing: bool) {}
    /// Called after a running Step and Content validation both succeed.
    fn live_step_committed(&mut self) {}
    fn ui(&mut self, _ui: &mut egui::Ui) -> Result<(), String> {
        Ok(())
    }
    /// Playback-aware controls. Existing applications retain their ordinary UI callback.
    fn ui_with_playback(&mut self, ui: &mut egui::Ui, _playing: bool) -> Result<(), String> {
        self.ui(ui)
    }
    /// Opt in to one completed UI frame of buffered gameplay/viewport actions.
    /// Legacy applications keep their established input/advance ordering.
    fn buffered_viewport_input(&self) -> bool {
        false
    }
    /// Safety cancellation and one pending action run before buffered advancement.
    /// This hook is called once per native UI frame, including discarded passes.
    fn viewport_frame_begin(&mut self, _ctx: &egui::Context) -> Result<(), String> {
        Ok(())
    }
    /// Register current geometry before navigation/picking, without advancing state
    /// or changing the captured scene. Interactive implementations must opt into
    /// buffered_viewport_input. Paint this same preparation through viewport_overlay
    /// after scene submission; queue actions for viewport_frame_begin.
    fn viewport_interaction(
        &mut self,
        _ui: &mut egui::Ui,
        _rect: egui::Rect,
    ) -> Result<ViewportInputOwnership, String> {
        Ok(ViewportInputOwnership::default())
    }
    /// A current Game keyboard lease allows an overlay to own only the pointer.
    fn viewport_interaction_with_game_focus(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        _game_focus: Option<egui::Id>,
    ) -> Result<ViewportInputOwnership, String> {
        self.viewport_interaction(ui, rect)
    }
    /// Discard unexecuted native viewport work on a raw focus-loss boundary.
    fn cancel_viewport_input(&mut self) {}
    /// Passive painting after advancement and viewport creation. The painter is
    /// clipped to the scene viewport. Existing applications need no overlay.
    fn viewport_overlay(
        &mut self,
        _painter: &egui::Painter,
        _rect: egui::Rect,
    ) -> Result<(), String> {
        Ok(())
    }
    fn input(&mut self, _input: &InputFrame) -> Result<(), String> {
        Ok(())
    }
    /// Opt-in authored logical actions. None preserves legacy keyboard routing.
    fn keyboard_actions(&self) -> Option<&rot_scene::input::ActionMap> {
        None
    }
    /// Admitted transitions only, after actual focused input-surface ownership.
    /// Hosts discard repeat/modifier/UI events; false cancels without key releases.
    fn action_input(
        &mut self,
        _owned: bool,
        _events: &[(rot_scene::input::InputKey, bool)],
    ) -> Result<(), String> {
        Ok(())
    }
    /// Ordered keyboard and optional pointer transitions for authored actions.
    fn action_physical_input(
        &mut self,
        events: &[(rot_scene::input::PhysicalInput, bool)],
    ) -> Result<(), String> {
        let keys = events
            .iter()
            .filter_map(|(input, pressed)| match input {
                rot_scene::input::PhysicalInput::Key(key) => Some((*key, *pressed)),
                rot_scene::input::PhysicalInput::PrimaryMouse => None,
            })
            .collect::<Vec<_>>();
        self.action_input(true, &keys)
    }
    /// Must not evaluate scripts or block. Pause/error/reset revoke pending input.
    fn cancel_action_input(&mut self) {}

    /// Return false to veto native close and present application-owned recovery UI.
    fn request_close(&mut self) -> bool {
        true
    }
    /// Consume a successful document replacement request through the host Reset lifecycle.
    fn take_reset_request(&mut self) -> bool {
        false
    }
    fn selected(&self) -> Option<&str> {
        None
    }
    fn select(&mut self, _id: &str) {}
    /// A normal completion condition pauses scheduling without turning it into an error.
    fn completed(&self) -> bool {
        false
    }
}

/// Headless lifecycle and fixed-step scheduling, also used by the native host.
pub struct Runtime<A: Application> {
    app: A,
    tick: u64,
    dt: Duration,
    running: bool,
    accumulator: Duration,
}

impl<A: Application> Runtime<A> {
    pub fn new(app: A, tick_duration: Duration) -> Result<Self, String> {
        if !(Duration::from_millis(1)..=Duration::from_secs(1)).contains(&tick_duration) {
            return Err("tick duration must be 1 ms..=1 s".into());
        }
        app.content().validate()?;
        Ok(Self {
            app,
            tick: 0,
            dt: tick_duration,
            running: false,
            accumulator: Duration::ZERO,
        })
    }
    pub fn app(&self) -> &A {
        &self.app
    }
    pub fn app_mut(&mut self) -> &mut A {
        &mut self.app
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn is_running(&self) -> bool {
        self.running
    }
    pub fn set_running(&mut self, running: bool) {
        if !running {
            self.app.cancel_action_input();
        }
        if self.running != running {
            self.app.playback_changed(running);
        }
        self.running = running;
        self.accumulator = Duration::ZERO;
    }
    pub fn step(&mut self) -> Result<(), String> {
        if self.app.completed() {
            self.set_running(false);
            return Ok(());
        }
        let result = (|| {
            let tick = self
                .tick
                .checked_add(1)
                .ok_or("application tick overflow; Reset to restart")?;
            self.app.tick(Step { tick, dt: self.dt })?;
            self.app.content().validate()?;
            self.tick = tick;
            if self.app.take_scene_switch() {
                self.accumulator = Duration::ZERO;
                self.app.playback_changed(self.running);
            }
            if self.running {
                self.app.live_step_committed();
            }
            if self.app.completed() {
                self.set_running(false);
            }
            Ok(())
        })();
        if result.is_err() {
            self.set_running(false);
        }
        result
    }
    /// Reset pauses and clears scheduling debt. Applications restore their own initial state.
    pub fn reset(&mut self) -> Result<(), String> {
        self.set_running(false);
        self.app.reset()?;
        self.app.content().validate()?;
        self.tick = 0;
        Ok(())
    }
    /// Interactive time is clamped to max(250 ms, one tick), at most four ticks per UI frame.
    /// Surplus debt is dropped after a stall/suspend; use explicit step for reproducible replay.
    pub fn advance(&mut self, elapsed: Duration) -> Result<usize, String> {
        if !self.running {
            return Ok(0);
        }
        if self.app.completed() {
            self.set_running(false);
            return Ok(0);
        }
        self.accumulator += elapsed.min(self.dt.max(Duration::from_millis(250)));
        let mut count = 0;
        while self.running && self.accumulator >= self.dt && count < 4 {
            self.accumulator -= self.dt;
            self.step()?;
            count += 1;
        }
        if self.accumulator >= self.dt {
            self.accumulator = Duration::ZERO;
        }
        Ok(count)
    }
}
