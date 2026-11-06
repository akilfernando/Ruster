struct SunShadow { view_projection: mat4x4<f32>, parameters: vec4<f32> };
@group(3) @binding(0) var<uniform> sun_shadow: SunShadow;
@group(3) @binding(1) var sun_depth: texture_depth_2d;
@group(3) @binding(2) var sun_compare: sampler_comparison;
fn sun_visibility(world: vec3<f32>, normal: vec3<f32>) -> f32 {
    let receiver_clip = sun_shadow.view_projection * vec4<f32>(world, 1.0);
    let receiver_ndc = receiver_clip.xyz / receiver_clip.w;
    let receiver_uv = vec2<f32>(receiver_ndc.x * 0.5 + 0.5, 0.5 - receiver_ndc.y * 0.5);
    let light_clip = sun_shadow.view_projection * vec4<f32>(world + normal * sun_shadow.parameters.w, 1.0);
    let ndc = light_clip.xyz / light_clip.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    // Positional derivatives describe the receiver plane, independent of its
    // interpolated shading normal. Evaluate before varying coverage returns.
    let dx = dpdx(vec3<f32>(receiver_uv, receiver_ndc.z));
    let dy = dpdy(vec3<f32>(receiver_uv, receiver_ndc.z));
    let determinant = dx.x * dy.y - dx.y * dy.x;
    let scale = abs(dx.x * dy.y) + abs(dx.y * dy.x);
    var gradient = vec2<f32>(0.0);
    if abs(determinant) > max(1e-12, scale * 1e-5) {
        gradient = vec2<f32>(dx.z * dy.y - dy.z * dx.y,
                             dy.z * dx.x - dx.z * dy.x) / determinant;
    }
    if abs(ndc.x) > 1.0 || abs(ndc.y) > 1.0 || ndc.z < 0.0 || ndc.z > 1.0 { return 1.0; }
    let texel = floor(uv / sun_shadow.parameters.y);
    var visibility = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            // Nearest comparison reads the actual texel center. Correcting
            // nominal +/-1 offsets alone leaves a half-texel depth mismatch.
            let sample_uv = (texel + vec2<f32>(f32(x), f32(y)) + vec2<f32>(0.5)) * sun_shadow.parameters.y;
            let reference = ndc.z + dot(gradient, sample_uv - uv) - sun_shadow.parameters.z;
            visibility += textureSampleCompareLevel(sun_depth, sun_compare,
                sample_uv, reference);
        }
    }
    return visibility / 9.0;
}
