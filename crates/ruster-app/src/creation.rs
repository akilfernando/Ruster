//! A small read-only launch descriptor for exported creator content.
//! The supported scene document owns authored state; this is not a second project editor.
use crate::{MeshMap, ResourceRoot, TextureMap};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, time::Duration};

pub const DESCRIPTOR_NAME: &str = "app.json";
pub const FORMAT: &str = "ruster-application";
pub const VERSION: u32 = 5;
pub const MAX_DESCRIPTOR_BYTES: usize = 64 * 1024;

/// Explicit launch framing. Navigation remains interactive after initialization.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialCamera {
    pub target: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub orthographic: bool,
}

impl InitialCamera {
    pub fn camera_state(self) -> Result<ruster_render::CameraState, String> {
        let state = ruster_render::CameraState {
            target: self.target,
            yaw: self.yaw,
            pitch: self.pitch,
            distance: self.distance,
            orthographic: self.orthographic,
        };
        state
            .is_valid()
            .then_some(state)
            .ok_or_else(|| "Invalid application initial_camera".into())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreationManifest {
    pub format: String,
    pub version: u32,
    pub title: String,
    pub scene: String,
    #[serde(deserialize_with = "unique_meshes")]
    pub meshes: BTreeMap<String, String>,
    /// Descriptor3 preserves the immutable import interpretation per mesh.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_normal_policies"
    )]
    pub mesh_normal_policies: BTreeMap<String, ruster_assets::MeshNormalPolicy>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_meshes"
    )]
    pub textures: BTreeMap<String, String>,
    /// Descriptor4 requires an explicit interpretation for every texture ID,
    /// including an empty map when there are no textures. Older descriptors omit it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "unique_texture_roles"
    )]
    pub texture_roles: Option<BTreeMap<String, ruster_assets::TextureRole>>,
    /// Descriptors1–4 keep their original integer-millisecond contract.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_tick"
    )]
    pub tick_millis: Option<u64>,
    /// Descriptor5 carries the exact editor/export fixed quantum.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_tick"
    )]
    pub tick_nanos: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_camera: Option<InitialCamera>,
}

// A present null is invalid, rather than becoming an omitted time unit.
fn present_tick<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    u64::deserialize(deserializer).map(Some)
}

fn unique_meshes<'de, D>(deserializer: D) -> Result<BTreeMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    unique_mapping(deserializer)
}

fn unique_normal_policies<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, ruster_assets::MeshNormalPolicy>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    unique_mapping(deserializer)
}

fn unique_texture_roles<'de, D>(
    deserializer: D,
) -> Result<Option<BTreeMap<String, ruster_assets::TextureRole>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // A present null must not become an omitted old-descriptor field.
    unique_mapping(deserializer).map(Some)
}

fn unique_mapping<'de, D, T>(deserializer: D) -> Result<BTreeMap<String, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Unique<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Unique<T> {
        type Value = BTreeMap<String, T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("at most 64 unique mesh IDs and relative resource paths")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut values = BTreeMap::new();
            while let Some((id, source)) = map.next_entry::<String, T>()? {
                if values.len() >= ruster_assets::MAX_ASSETS || values.insert(id, source).is_some()
                {
                    return Err(serde::de::Error::custom("too many or duplicate mesh IDs"));
                }
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(Unique(std::marker::PhantomData))
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
        if self.format != FORMAT || !(1..=VERSION).contains(&self.version) {
            return Err(
                "Unsupported application descriptor; expected ruster-application version 1 through 5"
                    .into(),
            );
        }
        if self.version == 1 && !self.textures.is_empty() {
            return Err("Application textures require descriptor version 2".into());
        }
        if self.version < 3 && !self.mesh_normal_policies.is_empty() {
            return Err("Mesh normal policies require descriptor version 3".into());
        }
        if self.version >= 3 && !self.meshes.keys().eq(self.mesh_normal_policies.keys()) {
            return Err("Descriptor3 and later require one normal policy per mesh".into());
        }
        if self.version < 4 && self.texture_roles.is_some() {
            return Err("Texture roles require descriptor version 4".into());
        }
        if self.version >= 4 {
            let roles = self
                .texture_roles
                .as_ref()
                .ok_or("Descriptor4 and later require texture_roles, including an empty map")?;
            if !self.textures.keys().eq(roles.keys()) {
                return Err("Descriptor4 and later require exactly one role per texture".into());
            }
        }
        if self.title.trim().is_empty()
            || self.title.trim() != self.title
            || self.title.len() > 128
            || self.title.chars().any(char::is_control)
        {
            return Err("Application title must be 1–128 UTF-8 bytes without controls or surrounding whitespace".into());
        }
        match (self.version, self.tick_millis, self.tick_nanos) {
            (1..=4, Some(millis), None) if (1..=1000).contains(&millis) => {},
            (5, None, Some(nanos)) if (1_000_000..=1_000_000_000).contains(&nanos) => {},
            _ => return Err("Descriptors1–4 require only tick_millis (1–1000); Descriptor5 requires only tick_nanos (1000000–1000000000).".into()),
        }
        if let Some(camera) = self.initial_camera {
            camera.camera_state()?;
        }
        resource_path(&self.scene)?;
        if self.meshes.len() + self.textures.len() > ruster_assets::MAX_ASSETS {
            return Err("Application mesh mappings exceed 64".into());
        }
        for (id, path) in self.meshes.iter().chain(&self.textures) {
            // Use the scene owner's identity grammar rather than a new dialect.
            ruster_scene::validate_id(id).map_err(|error| error.to_string())?;
            resource_path(path)?;
        }
        Ok(())
    }

    pub fn load(root: &ResourceRoot) -> Result<Self, String> {
        let bytes = root.read(DESCRIPTOR_NAME, MAX_DESCRIPTOR_BYTES)?;
        let raw: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid {DESCRIPTOR_NAME}: {error}"))?;
        if raw["version"].as_u64().is_some_and(|version| version < 4)
            && raw.get("texture_roles").is_some()
        {
            return Err("Texture roles require descriptor version 4".into());
        }
        if raw["version"].as_u64().is_some_and(|version| version < 3)
            && raw.get("mesh_normal_policies").is_some()
        {
            return Err("Mesh normal policies require descriptor version 3".into());
        }
        if raw["version"].as_u64().is_some_and(|version| version < 5)
            && raw.get("tick_nanos").is_some()
        {
            return Err("Nanosecond ticks require descriptor version 5".into());
        }
        if raw["version"].as_u64() == Some(5) && raw.get("tick_millis").is_some() {
            return Err("Descriptor5 uses tick_nanos; tick_millis must be absent".into());
        }
        let manifest: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid {DESCRIPTOR_NAME}: {error}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn tick_duration(&self) -> Duration {
        match (self.version, self.tick_millis, self.tick_nanos) {
            (1..=4, Some(millis), None) => Duration::from_millis(millis),
            (5, None, Some(nanos)) => Duration::from_nanos(nanos),
            // Invalid caller-constructed DTOs cannot select a fallback unit;
            // load/validate rejects them, and runtime scheduling rejects zero.
            _ => Duration::ZERO,
        }
    }

    pub fn load_session(&self, root: &ResourceRoot) -> Result<ruster_scene::Session, String> {
        self.validate()?;
        let bytes = root.read(&self.scene, ruster_scene::MAX_FILE_BYTES as usize)?;
        let session = ruster_scene::Session::from_snapshot_bytes(&bytes)
            .map_err(|error| error.to_string())?;
        if session.loaded_format_version() >= 17 && self.version < 5 {
            return Err("Scene17 runtime_hud_panels requires descriptor version 5".into());
        }
        if session.loaded_format_version() >= 16 && self.version < 5 {
            return Err("Scene16 runtime_hud_controls requires descriptor version 5".into());
        }
        if session.loaded_format_version() >= 15 && self.version < 5 {
            return Err("Scene15 runtime_hud requires descriptor version 5".into());
        }
        if session.loaded_format_version() >= 14 && self.version < 5 {
            return Err("Scene14 lifecycle_commands requires descriptor version 5".into());
        }
        if session.loaded_format_version() >= 13 && self.version < 5 {
            return Err("Scene13 runtime_state requires descriptor version 5".into());
        }
        if session.loaded_format_version() >= 12 && self.version < 5 {
            return Err("Scene12 spatial_queries requires descriptor version 5".into());
        }
        if session.loaded_format_version() >= 11 && self.version < 5 {
            return Err("Scene11 input_actions requires descriptor version 5".into());
        }
        if session.loaded_format_version() >= 10 && self.version < 4 {
            return Err("Scene10 roughness_maps requires descriptor version 4".into());
        }
        Ok(session)
    }

    pub fn load_textures(&self, root: &ResourceRoot) -> Result<TextureMap, String> {
        self.validate()?;
        let mut textures = TextureMap::new();
        let mut bytes = 0;
        for (id, path) in &self.textures {
            let role = self
                .texture_roles
                .as_ref()
                .and_then(|roles| roles.get(id))
                .copied()
                .unwrap_or(ruster_assets::TextureRole::BaseColor);
            let texture = root.texture_with_role(path, role)?;
            bytes =
                super::checked_texture_bytes(bytes, texture.payload_bytes()).map_err(|error| {
                    format!("texture {id}: {error}; reduce the creation's texture content")
                })?;
            textures.insert(id.clone(), texture);
        }
        Ok(textures)
    }

    pub fn load_meshes(&self, root: &ResourceRoot) -> Result<MeshMap, String> {
        self.validate()?;
        let mut meshes = MeshMap::new();
        let mut bytes = 0;
        for (id, path) in &self.meshes {
            let mesh = root.mesh_with_normal_policy(
                path,
                self.mesh_normal_policies
                    .get(id)
                    .copied()
                    .unwrap_or(ruster_assets::MeshNormalPolicy::Flat),
            )?;
            bytes = super::checked_mesh_bytes(bytes, mesh.payload_bytes()).map_err(|error| {
                format!("mesh {id}: {error}; reduce the creation's mesh content")
            })?;
            meshes.insert(id.clone(), mesh);
        }
        Ok(meshes)
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
        scene.lighting = ruster_scene::Lighting::authored_default("sun");
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
            mesh_normal_policies: [("marker".into(), ruster_assets::MeshNormalPolicy::Flat)].into(),
            textures: BTreeMap::new(),
            texture_roles: Some(BTreeMap::new()),
            tick_millis: None,
            tick_nanos: Some(16_666_667),
            initial_camera: None,
        };
        std::fs::write(
            directory.path().join(DESCRIPTOR_NAME),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let root = ResourceRoot::new(directory.path()).unwrap();
        let loaded = CreationManifest::load(&root).unwrap();
        assert_eq!(loaded.title, manifest.title);
        assert!(loaded.initial_camera.is_none());
        assert!(serde_json::to_value(&manifest)
            .unwrap()
            .get("initial_camera")
            .is_none());
        assert_eq!(loaded.tick_duration(), Duration::from_nanos(16_666_667));
        // Descriptor5 remains the nanos carrier for Scene12, including disabled
        // components. Earlier descriptor versions cannot relabel this content.
        let mut spatial: serde_json::Value = serde_json::from_slice(&original).unwrap();
        spatial["format_version"] = serde_json::json!(12);
        spatial["scene"]["objects"][0]["collider"] =
            serde_json::to_value(ruster_scene::Collider::default()).unwrap();
        std::fs::write(&scene_path, serde_json::to_vec(&spatial).unwrap()).unwrap();
        assert_eq!(
            loaded.load_session(&root).unwrap().scene().objects[0].collider,
            Some(ruster_scene::Collider::default())
        );
        let mut old_spatial = manifest.clone();
        old_spatial.version = 4;
        old_spatial.tick_millis = Some(100);
        old_spatial.tick_nanos = None;
        assert!(old_spatial
            .load_session(&root)
            .unwrap_err()
            .contains("Scene12 spatial_queries"));
        let mut state: serde_json::Value = serde_json::from_slice(&original).unwrap();
        state["format_version"] = serde_json::json!(13);
        state["authoring"] = serde_json::json!({"sources":{},"bindings":{
            "application":{"target":{"type":"application"},"source_id":"unfinished",
                "enabled":false,"properties":{},"state_defaults":{
                    "mode":{"type":"text","value":"ready"}}}
        }});
        std::fs::write(&scene_path, serde_json::to_vec(&state).unwrap()).unwrap();
        let state_session = loaded.load_session(&root).unwrap();
        assert_eq!(state_session.authoring().bindings.len(), 1);
        assert_eq!(state_session.scene(), session.scene());
        assert!(old_spatial
            .load_session(&root)
            .unwrap_err()
            .contains("Scene13 runtime_state"));
        state["format_version"] = serde_json::json!(14);
        state["authoring"]["bindings"]["application"]["commands"] =
            serde_json::json!({"restart":{"arguments":{}}});
        std::fs::write(&scene_path, serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            loaded
                .load_session(&root)
                .unwrap()
                .binding("application")
                .unwrap()
                .commands
                .len(),
            1
        );
        assert!(old_spatial
            .load_session(&root)
            .unwrap_err()
            .contains("Scene14 lifecycle_commands"));
        state["format_version"] = serde_json::json!(15);
        state["authoring"]["hud"] = serde_json::json!({"enabled":true,"elements":[
            {"id":"instructions","source":{"kind":"static","text":"Creator instructions 世界"}}
        ]});
        std::fs::write(&scene_path, serde_json::to_vec(&state).unwrap()).unwrap();
        let hud_session = loaded.load_session(&root).unwrap();
        assert!(hud_session.authoring().hud.enabled);
        assert_eq!(hud_session.authoring().hud.elements().count(), 1);
        assert_eq!(loaded.version, 5);
        assert!(old_spatial
            .load_session(&root)
            .unwrap_err()
            .contains("Scene15 runtime_hud"));
        state["format_version"] = serde_json::json!(16);
        state["authoring"]["hud"]["elements"][0]["source"] = serde_json::json!({
            "kind":"button","caption":"Restart","command":{
                "binding_id":"application","command_id":"restart","arguments":{}}
        });
        std::fs::write(&scene_path, serde_json::to_vec(&state).unwrap()).unwrap();
        assert!(loaded.load_session(&root).unwrap().authoring().hud.enabled);
        assert!(old_spatial
            .load_session(&root)
            .unwrap_err()
            .contains("Scene16 runtime_hud_controls"));
        let migrated = loaded.load_session(&root).unwrap();
        state["format_version"] = serde_json::json!(17);
        state["authoring"]["hud"] = serde_json::to_value(&migrated.authoring().hud).unwrap();
        state["authoring"]["hud"]["panels"][0]["anchor"] = serde_json::json!("center");
        std::fs::write(&scene_path, serde_json::to_vec(&state).unwrap()).unwrap();
        let panels = loaded.load_session(&root).unwrap();
        assert_eq!(
            panels.authoring().hud.panels[0].anchor,
            ruster_scene::hud::HudAnchor::Center
        );
        assert!(old_spatial
            .load_session(&root)
            .unwrap_err()
            .contains("Scene17 runtime_hud_panels"));
        let mut input_scene: serde_json::Value = serde_json::from_slice(&original).unwrap();
        input_scene["format_version"] = serde_json::json!(11);
        std::fs::write(&scene_path, serde_json::to_vec(&input_scene).unwrap()).unwrap();
        loaded.load_session(&root).unwrap();
        assert!(old_spatial
            .load_session(&root)
            .unwrap_err()
            .contains("Scene11 input_actions"));
        std::fs::write(&scene_path, &original).unwrap();
        for version in 1..=4 {
            let mut old = manifest.clone();
            old.version = version;
            old.tick_millis = Some(100);
            old.tick_nanos = None;
            if version < 4 {
                old.texture_roles = None;
            }
            if version < 3 {
                old.mesh_normal_policies.clear();
            }
            old.validate().unwrap();
            assert_eq!(old.tick_duration(), Duration::from_millis(100));
            let mut old_scene: serde_json::Value = serde_json::from_slice(&original).unwrap();
            old_scene["format_version"] = serde_json::json!(if version == 4 { 10 } else { 9 });
            std::fs::write(&scene_path, serde_json::to_vec(&old_scene).unwrap()).unwrap();
            old.load_session(&root).unwrap();
            std::fs::write(&scene_path, &original).unwrap();
            for value in [serde_json::json!(null), serde_json::json!(16_666_667)] {
                let mut raw = serde_json::to_value(&old).unwrap();
                raw["tick_nanos"] = value;
                std::fs::write(
                    directory.path().join(DESCRIPTOR_NAME),
                    serde_json::to_vec(&raw).unwrap(),
                )
                .unwrap();
                assert!(CreationManifest::load(&root).is_err());
            }
        }
        for bad in [
            "millis-null",
            "millis",
            "both",
            "missing",
            "null",
            "fractional",
            "negative",
            "low",
            "high",
        ] {
            let mut raw = serde_json::to_value(&manifest).unwrap();
            match bad {
                "millis-null" => raw["tick_millis"] = serde_json::Value::Null,
                "millis" => {
                    raw.as_object_mut().unwrap().remove("tick_nanos");
                    raw["tick_millis"] = serde_json::json!(100);
                }
                "both" => raw["tick_millis"] = serde_json::json!(100),
                "missing" => {
                    raw.as_object_mut().unwrap().remove("tick_nanos");
                }
                "null" => raw["tick_nanos"] = serde_json::Value::Null,
                "fractional" => raw["tick_nanos"] = serde_json::json!(16_666_667.5),
                "negative" => raw["tick_nanos"] = serde_json::json!(-1),
                "low" => raw["tick_nanos"] = serde_json::json!(999_999),
                _ => raw["tick_nanos"] = serde_json::json!(1_000_000_001),
            }
            std::fs::write(
                directory.path().join(DESCRIPTOR_NAME),
                serde_json::to_vec(&raw).unwrap(),
            )
            .unwrap();
            assert!(CreationManifest::load(&root).is_err(), "{bad}");
        }
        let mut framed = manifest.clone();
        framed.initial_camera = Some(InitialCamera {
            target: [1.0, 2.0, 3.0],
            yaw: 0.25,
            pitch: -0.5,
            distance: 8.0,
            orthographic: true,
        });
        let roundtrip: CreationManifest =
            serde_json::from_slice(&serde_json::to_vec(&framed).unwrap()).unwrap();
        roundtrip.validate().unwrap();
        assert_eq!(roundtrip.initial_camera, framed.initial_camera);
        for distance in [0.0, f32::INFINITY, 1.1e7] {
            framed.initial_camera.as_mut().unwrap().distance = distance;
            assert!(framed.validate().is_err());
        }
        let input = loaded.load_session(&root).unwrap();
        assert_eq!(input.scene().lighting, session.scene().lighting);
        assert!(
            input.path().is_none(),
            "runtime resources confer no save association"
        );
        Content::new(input.scene().clone(), loaded.load_meshes(&root).unwrap()).unwrap();
        assert_eq!(std::fs::read(&scene_path).unwrap(), original);
        // The actual runtime resource path must preserve the frozen import policy.
        std::fs::write(
            directory.path().join("marker.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 2 0\nf 1//1 2//1 3//1\n",
        )
        .unwrap();
        let mut smooth = manifest.clone();
        smooth.mesh_normal_policies.insert(
            "marker".into(),
            ruster_assets::MeshNormalPolicy::AuthoredWhenComplete,
        );
        assert_eq!(
            smooth.load_meshes(&root).unwrap()["marker"].vertices[0].normal,
            [0., 1., 0.]
        );
        let mut legacy = manifest.clone();
        legacy.version = 2;
        legacy.tick_millis = Some(100);
        legacy.tick_nanos = None;
        legacy.mesh_normal_policies.clear();
        legacy.texture_roles = None;
        assert_eq!(
            legacy.load_meshes(&root).unwrap()["marker"].vertices[0].normal,
            [0., 0., 1.]
        );
        for bad in [
            serde_json::json!(null),
            serde_json::json!({}),
            serde_json::json!({"marker":"flat"}),
        ] {
            let mut raw = serde_json::to_value(&legacy).unwrap();
            raw["mesh_normal_policies"] = bad;
            std::fs::write(
                directory.path().join(DESCRIPTOR_NAME),
                serde_json::to_vec(&raw).unwrap(),
            )
            .unwrap();
            assert!(CreationManifest::load(&root)
                .unwrap_err()
                .contains("version 3"));
        }
        smooth.mesh_normal_policies.clear();
        assert!(smooth.validate().is_err());

        for version in 1..=3 {
            let mut old = manifest.clone();
            old.version = version;
            old.tick_millis = Some(100);
            old.tick_nanos = None;
            old.texture_roles = None;
            if version < 3 {
                old.mesh_normal_policies.clear();
            }
            old.validate().unwrap();
            assert!(old
                .load_session(&root)
                .unwrap_err()
                .contains("Scene17 runtime_hud_panels"));
            // Genuine old documents still load through old descriptors.
            let mut old_scene: serde_json::Value = serde_json::from_slice(&original).unwrap();
            old_scene["format_version"] = serde_json::json!(9);
            std::fs::write(&scene_path, serde_json::to_vec(&old_scene).unwrap()).unwrap();
            old.load_session(&root).unwrap();
            std::fs::write(&scene_path, &original).unwrap();
            for roles in [serde_json::json!(null), serde_json::json!({})] {
                let mut raw = serde_json::to_value(&old).unwrap();
                raw["texture_roles"] = roles;
                std::fs::write(
                    directory.path().join(DESCRIPTOR_NAME),
                    serde_json::to_vec(&raw).unwrap(),
                )
                .unwrap();
                assert!(CreationManifest::load(&root)
                    .unwrap_err()
                    .contains("version 4"));
            }
        }
        let mut missing_roles = manifest.clone();
        missing_roles.texture_roles = None;
        assert!(missing_roles.validate().is_err());
        let mut extra_role = manifest.clone();
        extra_role
            .texture_roles
            .as_mut()
            .unwrap()
            .insert("extra".into(), ruster_assets::TextureRole::Roughness);
        assert!(extra_role.validate().is_err());
        let mut textured = manifest.clone();
        textured
            .textures
            .insert("roughness".into(), "roughness.png".into());
        assert!(textured.validate().is_err());
        textured
            .texture_roles
            .as_mut()
            .unwrap()
            .insert("roughness".into(), ruster_assets::TextureRole::Roughness);
        textured.validate().unwrap();
        // One RGB PNG exercises descriptor-controlled interpretation through the
        // actual read-only resource loader, with no catalog or cache available.
        let png = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90\x77\x53\xde\x00\x00\x00\x0c\x49\x44\x41\x54\x78\x9c\x63\x68\x50\x70\x00\x00\x02\x04\x00\xe1\xa5\xdf\x7f\x0d\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";
        std::fs::write(directory.path().join("roughness.png"), png).unwrap();
        let textures = textured.load_textures(&root).unwrap();
        assert_eq!(
            textures["roughness"].role,
            ruster_assets::TextureRole::Roughness
        );
        assert_eq!(textures["roughness"].mips[0].bytes, [128]);
        let mut old_color = textured.clone();
        old_color.version = 3;
        old_color.tick_millis = Some(100);
        old_color.tick_nanos = None;
        old_color.texture_roles = None;
        assert_eq!(
            old_color.load_textures(&root).unwrap()["roughness"].role,
            ruster_assets::TextureRole::BaseColor
        );
        let serialized = serde_json::to_string(&textured).unwrap();
        let duplicate = serialized.replace(
            "\"roughness\":\"roughness_linear\"",
            "\"roughness\":\"roughness_linear\",\"roughness\":\"base_color_srgb\"",
        );
        assert!(serde_json::from_str::<CreationManifest>(&duplicate).is_err());

        // Each loader must reject its crossing resource before reaching a
        // later resource failure. The public admission fixture journeys cover
        // both suffix variants and observe their actual read/decode sequence.
        let mut large_png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut large_png, 2048, 2048);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&vec![128; 2048 * 2048 * 3])
                .unwrap();
        }
        assert!(large_png.len() <= ruster_assets::MAX_TEXTURE_SOURCE_BYTES);
        std::fs::write(directory.path().join("large.png"), large_png).unwrap();
        std::fs::write(directory.path().join("after-budget.png"), b"malformed PNG").unwrap();
        let mut over_textures = manifest.clone();
        over_textures.meshes.clear();
        over_textures.mesh_normal_policies.clear();
        over_textures.textures = (1..=4)
            .map(|index| (format!("t{index:02}"), "large.png".into()))
            .collect();
        over_textures
            .textures
            .insert("t05".into(), "after-budget.png".into());
        over_textures.texture_roles = Some(
            over_textures
                .textures
                .keys()
                .map(|id| (id.clone(), ruster_assets::TextureRole::BaseColor))
                .collect(),
        );
        let texture_error = over_textures.load_textures(&root).unwrap_err();

        let large_obj = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\n".to_owned()
            + &"f 1/1 2/2 3/3\n".repeat(ruster_assets::MAX_TRIANGLES);
        assert!(large_obj.len() <= ruster_assets::MAX_SOURCE_BYTES);
        std::fs::write(directory.path().join("large.obj"), large_obj).unwrap();
        let mut over_meshes = manifest.clone();
        over_meshes.textures.clear();
        over_meshes.texture_roles = Some(BTreeMap::new());
        over_meshes.meshes = (1..=15)
            .map(|index| (format!("m{index:02}"), "large.obj".into()))
            .collect();
        over_meshes
            .meshes
            .insert("m16".into(), "after-budget.obj".into());
        over_meshes.mesh_normal_policies = over_meshes
            .meshes
            .keys()
            .map(|id| (id.clone(), ruster_assets::MeshNormalPolicy::Flat))
            .collect();
        let mesh_error = over_meshes.load_meshes(&root).unwrap_err();
        assert!(
            texture_error.contains("64 MiB")
                && texture_error.contains("t04")
                && mesh_error.contains("48 MiB")
                && mesh_error.contains("m15"),
            "first-crossing texture error {texture_error:?}; mesh error {mesh_error:?}"
        );
        assert!(!directory.path().join(".ruster-assets.json").exists());
        assert!(!directory.path().join(".ruster-cache").exists());

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
        for tick in [0, 999_999, 1_000_000_001] {
            let mut invalid = manifest.clone();
            invalid.tick_nanos = Some(tick);
            assert!(invalid.validate().is_err());
        }
        let mut invalid = manifest.clone();
        invalid.version = VERSION + 1;
        assert!(invalid.validate().is_err());
        let mut legacy = manifest.clone();
        legacy.version = 1;
        legacy.tick_millis = Some(100);
        legacy.tick_nanos = None;
        legacy.mesh_normal_policies.clear();
        legacy.texture_roles = None;
        legacy
            .validate()
            .expect("old descriptors still read without textures");
        legacy
            .textures
            .insert("base-color".into(), "color.png".into());
        assert!(
            legacy.validate().is_err(),
            "texture resources must not hide under descriptor1"
        );
        legacy.version = VERSION;
        legacy.tick_millis = None;
        legacy.tick_nanos = manifest.tick_nanos;
        legacy.mesh_normal_policies = manifest.mesh_normal_policies.clone();
        legacy.texture_roles =
            Some([("base-color".into(), ruster_assets::TextureRole::BaseColor)].into());
        legacy.validate().unwrap();
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
