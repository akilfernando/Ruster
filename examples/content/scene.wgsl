struct Camera { view_projection: mat4x4<f32> };
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
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage, read> instances: array<Instance>;
struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};
@vertex fn vs(v: Vertex, @builtin(instance_index) instance_index: u32) -> Output {
    var o: Output;
    let inst = instances[instance_index];
    let model = mat4x4<f32>(inst.model0, inst.model1, inst.model2, inst.model3);
    let normal = mat3x3<f32>(inst.normal0.xyz, inst.normal1.xyz, inst.normal2.xyz);
    o.position = camera.view_projection * model * vec4<f32>(v.position, 1.0);
    o.normal = normal * v.normal;
    o.color = inst.color;
    return o;
}
@fragment fn fs(v: Output) -> @location(0) vec4<f32> {
    let diffuse = max(dot(normalize(v.normal), normalize(vec3<f32>(0.4, 0.8, 0.6))), 0.0);
    return vec4<f32>(v.color.rgb * (0.28 + 0.72 * diffuse), 1.0);
}
