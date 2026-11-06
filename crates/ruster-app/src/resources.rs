use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

/// Read-only trusted project resources. Paths are relative to an explicit canonical root,
/// never implicitly to the process's working directory. This is not hostile-filesystem isolation.
#[derive(Clone, Debug)]
pub struct ResourceRoot(PathBuf);

impl ResourceRoot {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let root = path
            .canonicalize()
            .map_err(|e| format!("resource root {}: {e}", path.display()))?;
        if !root.is_dir() {
            return Err(format!(
                "resource root {} is not a directory",
                root.display()
            ));
        }
        Ok(Self(root))
    }

    pub fn beside_executable(relative: impl AsRef<Path>) -> Result<Self, String> {
        validate_relative(relative.as_ref())?;
        let executable = std::env::current_exe()
            .map_err(|e| format!("cannot locate application executable: {e}"))?;
        let parent = executable
            .parent()
            .ok_or("application executable has no parent directory")?;
        Self::new(parent.join(relative))
    }

    pub fn resolve(&self, relative: impl AsRef<Path>) -> Result<PathBuf, String> {
        let relative = relative.as_ref();
        validate_relative(relative)?;
        let path = self.0.join(relative);
        let resolved = path
            .canonicalize()
            .map_err(|e| format!("resource {}: {e}", path.display()))?;
        if !resolved.starts_with(&self.0) {
            return Err(format!(
                "resource {} escapes resource root {}",
                path.display(),
                self.0.display()
            ));
        }
        Ok(resolved)
    }

    pub fn scene(&self, relative: impl AsRef<Path>) -> Result<ruster_scene::Scene, String> {
        let path = self.resolve(relative)?;
        if !path
            .metadata()
            .map_err(|e| format!("scene resource {}: {e}", path.display()))?
            .is_file()
        {
            return Err(format!(
                "scene resource {}: expected a regular file",
                path.display()
            ));
        }
        ruster_scene::Scene::load(&path)
            .map_err(|e| format!("scene resource {}: {e}", path.display()))
    }

    pub fn mesh(&self, relative: impl AsRef<Path>) -> Result<Arc<ruster_assets::Mesh>, String> {
        self.mesh_with_normal_policy(relative, ruster_assets::MeshNormalPolicy::Flat)
    }

    pub fn mesh_with_normal_policy(
        &self,
        relative: impl AsRef<Path>,
        policy: ruster_assets::MeshNormalPolicy,
    ) -> Result<Arc<ruster_assets::Mesh>, String> {
        let path = self.resolve(relative)?;
        let bytes = ruster_assets::read_regular_file(&path, ruster_assets::MAX_SOURCE_BYTES)
            .map_err(|e| format!("mesh resource {}: {e}", path.display()))?;
        ruster_assets::parse_obj_with_normal_policy(&bytes, policy)
            .map(Arc::new)
            .map_err(|e| format!("mesh resource {}: {e}", path.display()))
    }

    pub fn texture(
        &self,
        relative: impl AsRef<Path>,
    ) -> Result<Arc<ruster_assets::Texture>, String> {
        self.texture_with_role(relative, ruster_assets::TextureRole::BaseColor)
    }

    pub fn texture_with_role(
        &self,
        relative: impl AsRef<Path>,
        role: ruster_assets::TextureRole,
    ) -> Result<Arc<ruster_assets::Texture>, String> {
        let bytes = self.read(relative, ruster_assets::MAX_TEXTURE_SOURCE_BYTES)?;
        ruster_assets::decode_png_with_role(&bytes, role)
            .map(Arc::new)
            .map_err(|error| format!("texture resource: {error}"))
    }

    /// Bounded regular-file bytes beneath this read-only resource root.
    pub fn read(&self, relative: impl AsRef<Path>, limit: usize) -> Result<Vec<u8>, String> {
        let path = self.resolve(relative)?;
        ruster_assets::read_regular_file(&path, limit)
            .map_err(|error| format!("resource {}: {error}", path.display()))
    }
}

fn validate_relative(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!(
            "resource path must be nonempty, relative and contain no '.' or '..': {}",
            path.display()
        ));
    }
    Ok(())
}
