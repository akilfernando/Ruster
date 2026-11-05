//! A small read-only launch descriptor for exported creator content.
//! Scene5 remains the authored document; this is not a second project editor.
use crate::{MeshMap, ResourceRoot};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, time::Duration};

pub const DESCRIPTOR_NAME: &str = "app.json";
pub const FORMAT: &str = "ruster-application";
pub const VERSION: u32 = 1;
pub const MAX_DESCRIPTOR_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreationManifest {
    pub format: String,
    pub version: u32,
    pub title: String,
    pub scene: String,
    #[serde(deserialize_with = "unique_meshes")]
    pub meshes: BTreeMap<String, String>,
    pub tick_millis: u64,
}

fn unique_meshes<'de, D>(deserializer: D) -> Result<BTreeMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Unique;
    impl<'de> serde::de::Visitor<'de> for Unique {
        type Value = BTreeMap<String, String>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("at most 64 unique mesh IDs and relative resource paths")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut values = BTreeMap::new();
            while let Some((id, source)) = map.next_entry::<String, String>()? {
                if values.len() >= ruster_assets::MAX_ASSETS || values.insert(id, source).is_some()
                {
                    return Err(serde::de::Error::custom("too many or duplicate mesh IDs"));
                }
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(Unique)
}

fn resource_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 256
        || Path::new(path).is_absolute()
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(
            "Application resources require bounded relative paths without traversal".into(),
        );
    }
    Ok(())
}

impl CreationManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT || self.version != VERSION {
            return Err(
                "Unsupported application descriptor; expected ruster-application version 1".into(),
            );
        }
        if self.title.trim().is_empty()
            || self.title.trim() != self.title
            || self.title.len() > 128
            || self.title.chars().any(char::is_control)
        {
            return Err("Application title must be 1–128 UTF-8 bytes without controls or surrounding whitespace".into());
        }
        if !(1..=1000).contains(&self.tick_millis) {
            return Err("Application tick_millis must be 1–1000".into());
        }
        resource_path(&self.scene)?;
        if self.meshes.len() > ruster_assets::MAX_ASSETS {
            return Err("Application mesh mappings exceed 64".into());
        }
        for (id, path) in &self.meshes {
            // Use the scene owner's identity grammar rather than a new dialect.
            ruster_scene::validate_id(id).map_err(|error| error.to_string())?;
            resource_path(path)?;
        }
        Ok(())
    }

    pub fn load(root: &ResourceRoot) -> Result<Self, String> {
        let bytes = root.read(DESCRIPTOR_NAME, MAX_DESCRIPTOR_BYTES)?;
        let manifest: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid {DESCRIPTOR_NAME}: {error}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn tick_duration(&self) -> Duration {
        Duration::from_millis(self.tick_millis)
    }

    pub fn load_session(&self, root: &ResourceRoot) -> Result<ruster_scene::Session, String> {
        self.validate()?;
        let bytes = root.read(&self.scene, ruster_scene::MAX_FILE_BYTES as usize)?;
        ruster_scene::Session::from_snapshot_bytes(&bytes).map_err(|error| error.to_string())
    }

    pub fn load_meshes(&self, root: &ResourceRoot) -> Result<MeshMap, String> {
        self.validate()?;
        self.meshes
            .iter()
            .map(|(id, path)| root.mesh(path).map(|mesh| (id.clone(), mesh)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Content;

    // Protect descriptor interpretation/resource confinement, not the existing
    // scene, OBJ parser or Lua evaluation matrices.
    #[test]
    fn creator_descriptor_roundtrip_and_read_only_boundaries() {
        let directory = tempfile::tempdir().unwrap();
        let scene_path = directory.path().join("scene.json");
        let mut scene = ruster_scene::Scene::demo();
        scene.objects[0].mesh = Some("marker".into());
        let mut session = ruster_scene::Session::new(scene).unwrap();
        session.save(&scene_path).unwrap();
        let original = std::fs::read(&scene_path).unwrap();
        std::fs::write(
            directory.path().join("marker.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        let manifest = CreationManifest {
            format: FORMAT.into(),
            version: VERSION,
            title: "My creation 世界".into(),
            scene: "scene.json".into(),
            meshes: [("marker".into(), "marker.obj".into())].into(),
            tick_millis: 100,
        };
        std::fs::write(
            directory.path().join(DESCRIPTOR_NAME),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let root = ResourceRoot::new(directory.path()).unwrap();
        let loaded = CreationManifest::load(&root).unwrap();
        assert_eq!(loaded.title, manifest.title);
        let input = loaded.load_session(&root).unwrap();
        assert!(
            input.path().is_none(),
            "runtime resources confer no save association"
        );
        Content::new(input.scene().clone(), loaded.load_meshes(&root).unwrap()).unwrap();
        assert_eq!(std::fs::read(&scene_path).unwrap(), original);
        for path in [
            "../outside.json",
            "/outside.json",
            "./scene.json",
            "x/../scene.json",
            "x\\scene.json",
        ] {
            let mut invalid = manifest.clone();
            invalid.scene = path.into();
            assert!(invalid.validate().is_err(), "{path}");
        }
        for tick in [0, 1001] {
            let mut invalid = manifest.clone();
            invalid.tick_millis = tick;
            assert!(invalid.validate().is_err());
        }
        let mut invalid = manifest.clone();
        invalid.version = 2;
        assert!(invalid.validate().is_err());
        let json = serde_json::to_string(&manifest).unwrap();
        let duplicate = json.replace(
            "\"marker\":\"marker.obj\"",
            "\"marker\":\"marker.obj\",\"marker\":\"other.obj\"",
        );
        assert!(serde_json::from_str::<CreationManifest>(&duplicate).is_err());
        let unknown = json.trim_end_matches('}').to_owned() + ",\"unknown\":true}";
        assert!(serde_json::from_str::<CreationManifest>(&unknown).is_err());
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            std::fs::write(outside.path().join("escape.obj"), b"not a mesh").unwrap();
            std::os::unix::fs::symlink(
                outside.path().join("escape.obj"),
                directory.path().join("alias.obj"),
            )
            .unwrap();
            let mut escape = manifest;
            escape.meshes.insert("marker".into(), "alias.obj".into());
            assert!(escape.load_meshes(&root).is_err());
        }
    }
}
