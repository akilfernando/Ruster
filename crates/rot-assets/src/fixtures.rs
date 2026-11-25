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
    pub second_clip_name: Option<&'static str>,
    /// `LINEAR`, `STEP` or `CUBICSPLINE` for the rotation sampler.
    pub rotation_interpolation: &'static str,
    /// glTF channel path for the first channel, normally `rotation`.
    pub rotation_path: &'static str,
    pub split_materials: bool,
    pub reverse_primitives: bool,
    pub material_fallbacks: bool,
    pub sibling_name: &'static str,
}

impl Default for AnimatedFixture {
    fn default() -> Self {
        Self {
            mid_angle: std::f32::consts::FRAC_PI_2,
            end_x: 2.0,
            clip_name: "Swing",
            second_clip_name: None,
            rotation_interpolation: "LINEAR",
            rotation_path: "rotation",
            split_materials: false,
            reverse_primitives: false,
            material_fallbacks: false,
            sibling_name: "Base",
        }
    }
}

/// `Rig` (Group) with two one-triangle mesh children, `Arm` and `Base`, and a
/// two second clip animating Arm rotation (three keys) and translation (two keys).
pub fn animated_glb(fixture: AnimatedFixture) -> Vec<u8> {
    let half = fixture.mid_angle / 2.0;
    let cubic = fixture.rotation_interpolation == "CUBICSPLINE";
    let rot_keys = [
        [0., 0., 0., 1.],
        [0., 0., half.sin(), half.cos()],
        [0., 0., 0., 1.],
    ];
    let rotations: Vec<f32> = rot_keys
        .into_iter()
        .flat_map(|key| {
            if cubic {
                vec![
                    0., 0., 0., 0., key[0], key[1], key[2], key[3], 0., 0., 0., 0.,
                ]
            } else {
                key.to_vec()
            }
        })
        .collect();
    let rot_count = if cubic { 9 } else { 3 };
    let rot_bytes = rot_count * 16;
    let trans_time_offset = 48 + rot_bytes;
    let trans_val_offset = trans_time_offset + 8;
    let floats: Vec<f32> = [
        // 0: positions, 36 bytes
        vec![0., 0., 0., 1., 0., 0., 0., 1., 0.],
        // 36: rotation times, 12 bytes
        vec![0., 1., 2.],
        // 48: rotation quaternions XYZW (3 or 9 VEC4s)
        rotations,
        // translation times, 8 bytes
        vec![0., 2.],
        // translations, 24 bytes
        vec![0., 0., 0., fixture.end_x, 0., 0.],
    ]
    .concat();
    let bin: Vec<u8> = floats.into_iter().flat_map(f32::to_le_bytes).collect();
    let mut document = json!({
        "asset":{"version":"2.0"},
        "scenes":[{"nodes":[0]}],
        "nodes":[
            {"name":"Rig","children":[1,2]},
            {"name":"Arm","mesh":0},
            {"name":fixture.sibling_name,"mesh":0,"translation":[0,-1,0]},
        ],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],
        "materials":[{"name":"Paint","pbrMetallicRoughness":{"metallicFactor":0,"roughnessFactor":0.6}}],
        "buffers":[{"byteLength":bin.len()}],
        "bufferViews":[
            {"buffer":0,"byteOffset":0,"byteLength":36},
            {"buffer":0,"byteOffset":36,"byteLength":12},
            {"buffer":0,"byteOffset":48,"byteLength":rot_bytes},
            {"buffer":0,"byteOffset":trans_time_offset,"byteLength":8},
            {"buffer":0,"byteOffset":trans_val_offset,"byteLength":24},
        ],
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
            {"bufferView":1,"componentType":5126,"count":3,"type":"SCALAR"},
            {"bufferView":2,"componentType":5126,"count":rot_count,"type":"VEC4"},
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
    });
    if let Some(name) = fixture.second_clip_name {
        let mut second = document["animations"][0].clone();
        second["name"] = json!(name);
        document["animations"].as_array_mut().unwrap().push(second);
    }
    if fixture.material_fallbacks {
        document["materials"][0] = json!({
            "pbrMetallicRoughness":{"roughnessFactor":0.0},
            "alphaMode":"BLEND",
            "doubleSided":true,
            "emissiveFactor":[0.2,0.1,0.0]
        });
    }
    if fixture.split_materials {
        document["meshes"][0]["primitives"]
            .as_array_mut()
            .unwrap()
            .push(json!({"attributes":{"POSITION":0},"material":1}));
        document["materials"].as_array_mut().unwrap().push(json!({
            "name":"Accent",
            "pbrMetallicRoughness":{"baseColorFactor":[0.1,0.6,0.8,1.0],"metallicFactor":0}
        }));
    }
    if fixture.reverse_primitives {
        document["meshes"][0]["primitives"]
            .as_array_mut()
            .unwrap()
            .reverse();
    }
    pack_glb(&document, &bin)
}

/// Static split mesh nodes sharing one point, plus a directional and a spot.
pub fn punctual_glb(edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    let template = animated_glb(AnimatedFixture {
        split_materials: true,
        ..Default::default()
    });
    let json_length = u32::from_le_bytes(template[12..16].try_into().unwrap()) as usize;
    let mut document: serde_json::Value =
        serde_json::from_slice(&template[20..20 + json_length]).unwrap();
    document.as_object_mut().unwrap().remove("animations");
    document["extensionsUsed"] = json!(["KHR_lights_punctual"]);
    document["extensionsRequired"] = json!(["KHR_lights_punctual"]);
    document["extensions"] = json!({"KHR_lights_punctual":{"lights":[
        {"type":"point","name":"Paint Lamp","intensity":2000,"range":0.04,"color":[0.25,0.5,1.0]},
        {"type":"directional","intensity":500},
        {"type":"spot","name":"Edge Lamp","spot":{}},
    ]}});
    let node_extension = |index| json!({"KHR_lights_punctual":{"light":index}});
    document["nodes"][1]["extensions"] = node_extension(0);
    document["nodes"][2]["extensions"] = node_extension(0);
    let nodes = document["nodes"].as_array_mut().unwrap();
    nodes.push(json!({"name":"Sun","extensions":node_extension(1),
        "rotation":[-std::f64::consts::FRAC_1_SQRT_2,0,0,std::f64::consts::FRAC_1_SQRT_2]}));
    nodes.push(json!({"name":"Spot","extensions":node_extension(2),"translation":[1,2,3]}));
    document["nodes"][0]["children"] = json!([1, 2, 3, 4]);
    edit(&mut document);
    pack_glb(&document, &template[28 + json_length..])
}

pub(crate) fn pack_glb(document: &serde_json::Value, bin: &[u8]) -> Vec<u8> {
    let mut document = serde_json::to_vec(document).expect("fixture JSON");
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
    bytes.extend_from_slice(bin);
    bytes
}
