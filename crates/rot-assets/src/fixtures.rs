//! Hand-built GLB bytes shared by tests across crates. Not a creator API.
use serde_json::json;

/// What the `Swing` clip of [`animated_glb`] does to the `Rig/Arm` node.
#[derive(Clone, Copy, Debug)]
pub struct AnimatedFixture {
    /// Rotation about +Z at one second, in radians. Zero at zero and two seconds.
    pub mid_angle: f32,
    /// X translation reached at two seconds, linear from zero.
    pub end_x: f32,
    pub clip_name: &'static str,
    /// `LINEAR`, `STEP` or `CUBICSPLINE` for the rotation sampler.
    pub rotation_interpolation: &'static str,
    /// glTF channel path for the first channel, normally `rotation`.
    pub rotation_path: &'static str,
}

impl Default for AnimatedFixture {
    fn default() -> Self {
        Self {
            mid_angle: std::f32::consts::FRAC_PI_2,
            end_x: 2.0,
            clip_name: "Swing",
            rotation_interpolation: "LINEAR",
            rotation_path: "rotation",
        }
    }
}

/// `Rig` (Group) with two one-triangle mesh children, `Arm` and `Base`, and a
/// two second clip animating Arm rotation (three keys) and translation (two keys).
pub fn animated_glb(fixture: AnimatedFixture) -> Vec<u8> {
    let half = fixture.mid_angle / 2.0;
    let floats: Vec<f32> = [
        // 0: positions, 36 bytes
        vec![0., 0., 0., 1., 0., 0., 0., 1., 0.],
        // 36: rotation times, 12 bytes
        vec![0., 1., 2.],
        // 48: rotation quaternions XYZW, 48 bytes
        vec![
            0.,
            0.,
            0.,
            1.,
            0.,
            0.,
            half.sin(),
            half.cos(),
            0.,
            0.,
            0.,
            1.,
        ],
        // 96: translation times, 8 bytes
        vec![0., 2.],
        // 104: translations, 24 bytes
        vec![0., 0., 0., fixture.end_x, 0., 0.],
    ]
    .concat();
    let bin: Vec<u8> = floats.into_iter().flat_map(f32::to_le_bytes).collect();
    let mut document = serde_json::to_vec(&json!({
        "asset":{"version":"2.0"},
        "scenes":[{"nodes":[0]}],
        "nodes":[
            {"name":"Rig","children":[1,2]},
            {"name":"Arm","mesh":0},
            {"name":"Base","mesh":0,"translation":[0,-1,0]},
        ],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],
        "materials":[{"name":"Paint","pbrMetallicRoughness":{"metallicFactor":0,"roughnessFactor":0.6}}],
        "buffers":[{"byteLength":bin.len()}],
        "bufferViews":[
            {"buffer":0,"byteOffset":0,"byteLength":36},
            {"buffer":0,"byteOffset":36,"byteLength":12},
            {"buffer":0,"byteOffset":48,"byteLength":48},
            {"buffer":0,"byteOffset":96,"byteLength":8},
            {"buffer":0,"byteOffset":104,"byteLength":24},
        ],
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
            {"bufferView":1,"componentType":5126,"count":3,"type":"SCALAR"},
            {"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"},
            {"bufferView":3,"componentType":5126,"count":2,"type":"SCALAR"},
            {"bufferView":4,"componentType":5126,"count":2,"type":"VEC3"},
        ],
        "animations":[{
            "name":fixture.clip_name,
            "channels":[
                {"sampler":0,"target":{"node":1,"path":fixture.rotation_path}},
                {"sampler":1,"target":{"node":1,"path":"translation"}},
            ],
            "samplers":[
                {"input":1,"output":2,"interpolation":fixture.rotation_interpolation},
                {"input":3,"output":4},
            ],
        }],
    }))
    .expect("fixture JSON");
    document.resize(document.len().next_multiple_of(4), b' ');
    let total = 28 + document.len() + bin.len();
    let mut bytes = b"glTF".to_vec();
    bytes.extend(2_u32.to_le_bytes());
    bytes.extend((total as u32).to_le_bytes());
    bytes.extend((document.len() as u32).to_le_bytes());
    bytes.extend(b"JSON");
    bytes.extend(document);
    bytes.extend((bin.len() as u32).to_le_bytes());
    bytes.extend(b"BIN\0");
    bytes.extend(bin);
    bytes
}
