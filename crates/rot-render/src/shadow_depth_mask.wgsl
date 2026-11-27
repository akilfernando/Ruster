struct SunShadow { view_projection: mat4x4<f32>, parameters: vec4<f32> };
struct Instance {
    model0: vec4<f32>,
    model1: vec4<f32>,
    model2: vec4<f32>,
    model3: vec4<f32>,
    normal0: vec4<f32>,
    normal1: vec4<f32>,
    normal2: vec4<f32>,
    color: vec4<f32>,
    base_color: vec4<f32>,
    response: vec4<f32>,
    emission_color: vec4<f32>,
    uv_transform: vec4<f32>,
    pbr_params: vec4<f32>,
    pbr_maps: vec4<f32>,
};
@group(0) @binding(0) var<uniform> sun_shadow: SunShadow;
@group(1) @binding(0) var<storage, read> instances: array<Instance>;
@group(1) @binding(1) var<storage, read> skin_matrices: array<mat4x4<f32>>;
@group(2) @binding(0) var base_color_texture: texture_2d<f32>;
@group(2) @binding(1) var base_color_sampler: sampler;
struct Vertex {
    @location(0) position: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(4) joints: vec4<u32>,
    @location(5) weights: vec4<f32>,
};
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) alpha_params: vec2<f32>,
};
@vertex fn vs(v: Vertex, @builtin(instance_index) instance_index: u32) -> Output {
    let inst = instances[instance_index];
    let skin_count = bitcast<u32>(inst.normal1.w);
    var out: Output;
    if skin_count > 0u {
        let base = bitcast<u32>(inst.normal0.w);
        let limit = skin_count - 1u;
        let skin_mat = skin_matrices[base + min(v.joints.x, limit)] * v.weights.x
            + skin_matrices[base + min(v.joints.y, limit)] * v.weights.y
            + skin_matrices[base + min(v.joints.z, limit)] * v.weights.z
            + skin_matrices[base + min(v.joints.w, limit)] * v.weights.w;
        out.position = sun_shadow.view_projection * skin_mat * vec4<f32>(v.position, 1.0);
    } else {
        let model = mat4x4<f32>(inst.model0, inst.model1, inst.model2, inst.model3);
        out.position = sun_shadow.view_projection * model * vec4<f32>(v.position, 1.0);
    }
    out.uv = vec2<f32>(v.uv.x, 1.0 - v.uv.y) * inst.uv_transform.xy + inst.uv_transform.zw;
    out.alpha_params = vec2<f32>(inst.pbr_params.w, inst.pbr_maps.w);
    return out;
}
@fragment fn fs(v: Output) {
    let alpha = clamp(textureSample(base_color_texture, base_color_sampler, v.uv).a * v.alpha_params.x, 0.0, 1.0);
    if v.alpha_params.y >= 0.0 && alpha < v.alpha_params.y {
        discard;
    }
}
