struct Camera { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) model0: vec4<f32>,
    @location(3) model1: vec4<f32>,
    @location(4) model2: vec4<f32>,
    @location(5) model3: vec4<f32>,
    @location(6) normal0: vec4<f32>,
    @location(7) normal1: vec4<f32>,
    @location(8) normal2: vec4<f32>,
    @location(9) color: vec4<f32>,
};
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};
@vertex fn vs(v: Vertex) -> Output {
    var o: Output;
    let model = mat4x4<f32>(v.model0, v.model1, v.model2, v.model3);
    let normal = mat3x3<f32>(v.normal0.xyz, v.normal1.xyz, v.normal2.xyz);
    o.position = camera.view_projection * model * vec4<f32>(v.position, 1.0);
    o.normal = normal * v.normal;
    o.color = v.color;
    return o;
}
@fragment fn fs(v: Output) -> @location(0) vec4<f32> {
    let diffuse = max(dot(normalize(v.normal), normalize(vec3<f32>(0.4, 0.8, 0.6))), 0.0);
    return vec4<f32>(v.color.rgb * (0.28 + 0.72 * diffuse), 1.0);
}
