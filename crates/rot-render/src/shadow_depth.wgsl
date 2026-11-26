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
};
@group(0) @binding(0) var<uniform> sun_shadow: SunShadow;
@group(1) @binding(0) var<storage, read> instances: array<Instance>;
struct Vertex {
    @location(0) position: vec3<f32>,
};
@vertex fn vs(v: Vertex, @builtin(instance_index) instance_index: u32) -> @builtin(position) vec4<f32> {
    let inst = instances[instance_index];
    let model = mat4x4<f32>(inst.model0, inst.model1, inst.model2, inst.model3);
    return sun_shadow.view_projection * model * vec4<f32>(v.position, 1.0);
}
