//! One completion epoch and one GPU-only retired asset generation.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// Borrowed planning state exists only during prepare, never across submission.
pub(super) struct MeshPlan<'a> {
    sources: BTreeMap<&'a str, &'a Arc<ruster_assets::Mesh>>,
}
#[derive(Default)]
struct MeshValidation {
    stamps: BTreeMap<String, (Weak<ruster_assets::Mesh>, bool)>,
    rejection: Option<&'static str>,
}
impl<'a> MeshPlan<'a> {
    fn new(
        current: &BTreeMap<String, GpuMesh>,
        groups: &'a [DrawGroup],
        validation: &mut MeshValidation,
    ) -> Self {
        // Shared geometry can occur in several material/sampler groups. Validate
        // each changed unique ID once, never once per group or idle frame.
        let mut sources = groups
            .iter()
            .filter_map(|group| group.key.as_deref().zip(group.mesh.as_ref()))
            .collect::<BTreeMap<_, _>>();
        validation
            .stamps
            .retain(|id, _| sources.contains_key(id.as_str()));
        let mut rejection = (sources.len() > MAX_MESHES)
            .then_some("Ruster GPU mesh closure exceeds 64 unique IDs; meshes use cube fallback");
        if rejection.is_some() {
            sources.clear();
        }
        sources.retain(|id, mesh| {
            let identity = Arc::downgrade(mesh);
            let valid = if current.get(*id).is_some_and(|gpu| gpu.matches(mesh)) {
                true
            } else if let Some((_, valid)) = validation
                .stamps
                .get(*id)
                .filter(|(old, _)| Weak::ptr_eq(old, &identity))
            {
                *valid
            } else {
                let valid = ruster_assets::validate_mesh(mesh).is_ok();
                validation
                    .stamps
                    .insert((*id).to_owned(), (identity, valid));
                valid
            };
            if !valid && rejection.is_none() {
                rejection =
                    Some("Ruster mesh geometry is malformed; unavailable meshes use cube fallback");
            }
            valid
        });
        let bytes = sources
            .values()
            .try_fold(0u64, |sum, mesh| sum.checked_add(mesh_bytes(mesh)));
        if sources.len() > MAX_MESHES
            || bytes.is_none_or(|bytes| bytes > ruster_assets::MAX_RESOLVED_BYTES as u64)
        {
            // A stable fallback closure owns no rejected buffers. In particular,
            // invalid source identity changes cannot create repeated epochs.
            rejection = Some("Ruster requested mesh GPU vertex/index payload exceeds 48 MiB; source meshes may be valid but the renderer vertex layout is larger; meshes use cube fallback");
            sources.clear();
        }
        if rejection != validation.rejection {
            if let Some(message) = rejection {
                eprintln!("{message}");
            }
        }
        validation.rejection = rejection;
        Self { sources }
    }
    pub fn changed(&self, current: &BTreeMap<String, GpuMesh>) -> bool {
        current.len() != self.sources.len()
            || self
                .sources
                .iter()
                .any(|(id, mesh)| current.get(*id).is_none_or(|gpu| !gpu.matches(mesh)))
    }
    pub fn apply(
        self,
        device: &wgpu::Device,
        current: &mut BTreeMap<String, GpuMesh>,
    ) -> (u64, u64) {
        current.retain(|id, _| self.sources.contains_key(id.as_str()));
        let (mut uploads, mut bytes) = (0, 0);
        for (id, mesh) in self.sources {
            if current.get(id).is_some_and(|gpu| gpu.matches(mesh)) {
                continue;
            }
            let vertices = mesh
                .vertices
                .iter()
                .enumerate()
                .map(|(index, vertex)| Vertex {
                    position: vertex.position,
                    normal: vertex.normal,
                    uv: mesh
                        .texcoords
                        .as_ref()
                        .and_then(|uvs| uvs.get(index))
                        .copied()
                        .unwrap_or([0.; 2]),
                })
                .collect::<Vec<_>>();
            let gpu = GpuMesh::new(device, Some(Arc::downgrade(mesh)), &vertices, &mesh.indices);
            uploads += 1;
            bytes += gpu.bytes;
            current.insert(id.to_owned(), gpu);
        }
        (uploads, bytes)
    }
}
fn mesh_bytes(mesh: &ruster_assets::Mesh) -> u64 {
    (mesh.vertices.len() * size_of::<Vertex>() + mesh.indices.len() * size_of::<u32>()) as u64
}

struct Retired {
    meshes: BTreeMap<String, GpuMesh>,
    materials: material_gpu::Generation,
}
enum Epoch {
    AwaitingSubmission,
    Armed(Arc<AtomicBool>),
}
#[derive(Default)]
pub(super) struct AssetGpu {
    retired: Option<Retired>,
    epoch: Option<Epoch>,
    error: Option<String>,
    mesh_validation: MeshValidation,
    // Hold observation of a GENUINE callback, never invent completion. The
    // lifecycle probe needs deterministic backpressure on a fast real device.
    #[cfg(test)]
    pub hold_acknowledgement: bool,
}
impl AssetGpu {
    pub fn plan_mesh<'a>(
        &mut self,
        current: &BTreeMap<String, GpuMesh>,
        groups: &'a [DrawGroup],
    ) -> MeshPlan<'a> {
        MeshPlan::new(current, groups, &mut self.mesh_validation)
    }
    pub fn pending(&self) -> bool {
        self.epoch.is_some()
    }
    pub fn phase(&self) -> &'static str {
        if self.error.is_some() {
            return "error";
        }
        match self.epoch {
            None => "idle",
            Some(Epoch::AwaitingSubmission) => "awaiting_submission",
            Some(Epoch::Armed(_)) => "armed",
        }
    }
    pub fn begin(
        &mut self,
        meshes: &BTreeMap<String, GpuMesh>,
        materials: &MaterialGpu,
        context: &egui::Context,
    ) {
        assert!(self.epoch.is_none() && self.retired.is_none());
        // Handle clones share unchanged GPU resources; CPU identities stay Weak.
        // Count both generations conservatively, even for shared handles.
        self.retired = Some(Retired {
            meshes: meshes.clone(),
            materials: materials.generation(),
        });
        self.epoch = Some(Epoch::AwaitingSubmission);
        context.request_repaint_after(std::time::Duration::from_millis(16));
    }
    /// Caller guarantees a preceding prepare has crossed queue submission.
    /// Native owner logic and a unique next-frame callback satisfy that contract;
    /// an additional layout pass / duplicate callback does not.
    pub fn arm(&mut self, queue: &wgpu::Queue, context: &egui::Context) -> Result<bool, String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if !self.reap() {
            return Ok(false);
        }
        if matches!(self.epoch, Some(Epoch::AwaitingSubmission)) {
            let completed = Arc::new(AtomicBool::new(false));
            let flag = completed.clone();
            let context = context.clone();
            queue.on_submitted_work_done(move || {
                flag.store(true, Ordering::Release);
                context.request_repaint();
            });
            self.epoch = Some(Epoch::Armed(completed));
        }
        Ok(true)
    }
    pub fn fail(&mut self, message: String) {
        self.error = Some(message);
    }
    pub fn reap(&mut self) -> bool {
        let held = {
            #[cfg(test)]
            {
                self.hold_acknowledgement
            }
            #[cfg(not(test))]
            {
                false
            }
        };
        if !held && matches!(&self.epoch, Some(Epoch::Armed(done)) if done.load(Ordering::Acquire))
        {
            self.retired = None;
            self.epoch = None;
            return false;
        }
        self.pending()
    }
    #[cfg(test)]
    pub fn genuinely_completed(&self) -> bool {
        matches!(&self.epoch, Some(Epoch::Armed(done)) if done.load(Ordering::Acquire))
    }
    pub fn retired_bytes(&self) -> (u64, u64) {
        self.retired.as_ref().map_or((0, 0), |old| {
            (
                old.meshes.values().map(|mesh| mesh.bytes).sum(),
                old.materials.bytes(),
            )
        })
    }
}
