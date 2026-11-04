@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
struct Output { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) index: u32) -> Output {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var o: Output;
    o.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return o;
}
@fragment fn fs(v: Output) -> @location(0) vec4<f32> {
    return textureSample(image, image_sampler, v.uv);
}

// Egui supports both sRGB-converting and gamma framebuffers. Match its target convention.
@fragment fn fs_gamma(v: Output) -> @location(0) vec4<f32> {
    let color = textureSample(image, image_sampler, v.uv);
    let gamma = select(1.055 * pow(color.rgb, vec3<f32>(1.0 / 2.4)) - 0.055,
        color.rgb * 12.92, color.rgb <= vec3<f32>(0.0031308));
    return vec4<f32>(gamma, color.a);
}
