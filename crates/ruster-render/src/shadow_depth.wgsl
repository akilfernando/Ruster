struct SunShadow { view_projection: mat4x4<f32>, parameters: vec4<f32> };
@group(0) @binding(0) var<uniform> sun_shadow: SunShadow;
struct Vertex {
    @location(0) position: vec3<f32>,
    @location(2) model0: vec4<f32>, @location(3) model1: vec4<f32>,
    @location(4) model2: vec4<f32>, @location(5) model3: vec4<f32>,
};
@vertex fn vs(v: Vertex) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(v.model0, v.model1, v.model2, v.model3);
    return sun_shadow.view_projection * model * vec4<f32>(v.position, 1.0);
}
