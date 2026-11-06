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


// HDR display extension. Existing SDR functions above retain their arithmetic.
struct Display { parameters: vec4<f32> };
@group(0) @binding(2) var<uniform> display: Display;

// Copyright 2024 The Khronos Group, Inc. SPDX-License-Identifier: Apache-2.0
// Modified: GLSL -> WGSL translation for Ruster, 2026-11-06.
// Source pinned at 180b1a7bddec33f73fe41712a2963cc3ad8e5547:
// https://github.com/KhronosGroup/ToneMapping/blob/180b1a7bddec33f73fe41712a2963cc3ad8e5547/PBR_Neutral/pbrNeutral.glsl
// Attribution is in upstream .reuse/dep5; see PBR_NEUTRAL_LICENSE.txt.
fn pbr_neutral(input_color: vec3<f32>) -> vec3<f32> {
    let start_compression = 0.8 - 0.04;
    let desaturation = 0.15;
    let x = min(input_color.r, min(input_color.g, input_color.b));
    let offset = select(0.04, x - 6.25 * x * x, x < 0.08);
    var color = input_color - vec3<f32>(offset);
    let peak = max(color.r, max(color.g, color.b));
    if peak < start_compression { return color; }
    let d = 1.0 - start_compression;
    let new_peak = 1.0 - d * d / (peak + d - start_compression);
    color *= new_peak / peak;
    let g = 1.0 - 1.0 / (desaturation * (peak - new_peak) + 1.0);
    return mix(color, vec3<f32>(new_peak), g);
}
fn displayed_color(uv: vec2<f32>) -> vec4<f32> {
    let color = textureSample(image, image_sampler, uv);
    if display.parameters.x < 0.5 { return color; }
    return vec4<f32>(pbr_neutral(max(color.rgb, vec3<f32>(0.0)) * exp2(display.parameters.y)), color.a);
}
@fragment fn fs_display(v: Output) -> @location(0) vec4<f32> {
    return displayed_color(v.uv);
}
@fragment fn fs_display_gamma(v: Output) -> @location(0) vec4<f32> {
    let color = displayed_color(v.uv);
    let gamma = select(1.055 * pow(color.rgb, vec3<f32>(1.0 / 2.4)) - 0.055,
        color.rgb * 12.92, color.rgb <= vec3<f32>(0.0031308));
    return vec4<f32>(gamma, color.a);
}
