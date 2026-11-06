//! Bounded Linux application host: application-owned behavior, shared Vulkan rendering.
pub mod creation;
mod host;
mod resources;
#[cfg(test)]
mod tests;

pub use eframe::egui;
pub use host::{run, HostOptions};
pub use resources::ResourceRoot;

use ruster_assets::Mesh;
use ruster_scene::Scene;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

pub type MeshMap = BTreeMap<String, Arc<Mesh>>;
pub type TextureMap = BTreeMap<String, Arc<ruster_assets::Texture>>;

/// Validated immutable mesh snapshots alongside application-owned scene state.
/// Construct a new Content to replace meshes; this keeps retained GPU identities stable.
#[derive(Clone)]
pub struct Content {
    scene: Scene,
    meshes: MeshMap,
    textures: TextureMap,
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
        if meshes.len() + textures.len() > ruster_assets::MAX_ASSETS {
            return Err("application asset count exceeds 64".into());
        }
        if meshes.keys().any(|id| textures.contains_key(id)) {
            return Err("application mesh/texture asset IDs must be distinct".into());
        }
        let mut bytes = 0;
        for (id, mesh) in &meshes {
            ruster_assets::validate_mesh(mesh).map_err(|e| format!("mesh {id}: {e}"))?;
            bytes += mesh.payload_bytes();
        }
        if bytes > ruster_assets::MAX_RESOLVED_BYTES {
            return Err("application meshes exceed 48 MiB resolved geometry".into());
        }
        let mut texture_bytes = 0;
        for (id, texture) in &textures {
            ruster_scene::validate_id(id).map_err(|error| error.to_string())?;
            ruster_assets::validate_texture(texture)
                .map_err(|error| format!("texture {id}: {error}"))?;
            texture_bytes += texture.payload_bytes();
        }
        if texture_bytes > ruster_assets::MAX_TEXTURE_RESOLVED_BYTES {
            return Err("application textures exceed 64 MiB resolved payload".into());
        }
        let content = Self {
            scene,
            meshes,
            textures,
        };
        content.validate()?;
        Ok(content)
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }
    pub fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }
    pub fn meshes(&self) -> &MeshMap {
        &self.meshes
    }

    pub fn textures(&self) -> &TextureMap {
        &self.textures
    }

    /// Check runtime scene mutations without scanning the immutable mesh payload again.
    pub fn validate(&self) -> Result<(), String> {
        self.scene
            .validate()
            .map_err(|e| format!("application scene: {e}"))?;
        for object in &self.scene.objects {
            if let Some(material) = object
                .material
                .as_ref()
                .and_then(|id| self.scene.materials.get(id))
            {
                for (texture, role) in [
                    (
                        &material.base_color_texture,
                        ruster_assets::TextureRole::BaseColor,
                    ),
                    (
                        &material.roughness_texture,
                        ruster_assets::TextureRole::Roughness,
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
                    if !object
                        .mesh
                        .as_ref()
                        .and_then(|id| self.meshes.get(id))
                        .is_some_and(|mesh| mesh.has_complete_uvs())
                    {
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

/// Application state and rules remain independent from the host and editor.
/// Callbacks must preserve valid state on error; this host cannot roll back arbitrary state.
pub trait Application: 'static {
    fn content(&self) -> &Content;
    fn tick(&mut self, step: Step) -> Result<(), String>;
    fn reset(&mut self) -> Result<(), String>;
    fn ui(&mut self, _ui: &mut egui::Ui) -> Result<(), String> {
        Ok(())
    }
    fn input(&mut self, _input: &InputFrame) -> Result<(), String> {
        Ok(())
    }
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
