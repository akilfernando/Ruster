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

// HDR display extension, post-processing composite, FXAA, and decoupled selection mask contour.
struct Display {
    parameters: vec4<f32>, // x: applied_hdr, y: exposure_stops, z: selection, w: pixels_per_point
    post_flags: vec4<f32>, // x: tonemapper (0 PBR Neutral, 1 ACES, 2 Reinhard, 3 None), y: bloom_intensity, z: ssao_enabled, w: fxaa_enabled
    grading_a: vec4<f32>,  // x: saturation, y: contrast, z: temperature, w: tint
    grading_b: vec4<f32>,  // x: vignette, y: inv_width, z: inv_height, w: reserved
};
@group(0) @binding(2) var<uniform> display: Display;
@group(0) @binding(3) var selection_mask: texture_2d<f32>;
@group(0) @binding(4) var bloom_texture: texture_2d<f32>;
@group(0) @binding(5) var ao_texture: texture_2d<f32>;

// Copyright 2024 The Khronos Group, Inc. SPDX-License-Identifier: Apache-2.0
// Modified: GLSL -> WGSL translation for Rot Engine, 2026-11-06.
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

fn aces_fitted(c: vec3<f32>) -> vec3<f32> {
    let x = c * 0.6;
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn grade_and_tonemap(uv: vec2<f32>) -> vec3<f32> {
    let raw = textureSampleLevel(image, image_sampler, uv, 0.0).rgb;
    if display.parameters.x < 0.5 {
        return raw;
    }
    var c = max(raw, vec3<f32>(0.0));
    if display.post_flags.z > 0.5 {
        let ao = textureSampleLevel(ao_texture, image_sampler, uv, 0.0).r;
        c *= ao;
    }
    if display.post_flags.y > 0.0 {
        let bloom = textureSampleLevel(bloom_texture, image_sampler, uv, 0.0).rgb;
        c += bloom * display.post_flags.y;
    }
    c = max(c, vec3<f32>(0.0)) * exp2(display.parameters.y);
    let temp = display.grading_a.z;
    let tint = display.grading_a.w;
    if abs(temp) > 1e-5 || abs(tint) > 1e-5 {
        c = max(c * vec3<f32>(1.0 + temp * 0.18, 1.0 - tint * 0.12, 1.0 - temp * 0.18), vec3<f32>(0.0));
    }
    let contrast = display.grading_a.y;
    if abs(contrast - 1.0) > 1e-5 {
        c = max((c - vec3<f32>(0.18)) * contrast + vec3<f32>(0.18), vec3<f32>(0.0));
    }
    let saturation = display.grading_a.x;
    if abs(saturation - 1.0) > 1e-5 {
        let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
        c = max(mix(vec3<f32>(luma), c, saturation), vec3<f32>(0.0));
    }
    let tm = display.post_flags.x;
    if tm < 0.5 {
        c = pbr_neutral(c);
    } else if tm < 1.5 {
        c = aces_fitted(c);
    } else if tm < 2.5 {
        c = c / (vec3<f32>(1.0) + c);
    } else {
        c = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    let vignette = display.grading_b.x;
    if vignette > 0.0 {
        let d = uv - vec2<f32>(0.5);
        let mask = clamp(1.0 - dot(d, d) * vignette * 2.5, 0.0, 1.0);
        c *= mask * (2.0 - mask);
    }
    return c;
}

fn luma_rec709(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn fxaa_color(uv: vec2<f32>) -> vec3<f32> {
    let center = grade_and_tonemap(uv);
    let rcp = display.grading_b.yz;
    let n = grade_and_tonemap(uv + vec2<f32>(0.0, -rcp.y));
    let s = grade_and_tonemap(uv + vec2<f32>(0.0, rcp.y));
    let w = grade_and_tonemap(uv + vec2<f32>(-rcp.x, 0.0));
    let e = grade_and_tonemap(uv + vec2<f32>(rcp.x, 0.0));

    let lc = luma_rec709(center);
    let ln = luma_rec709(n);
    let ls = luma_rec709(s);
    let lw = luma_rec709(w);
    let le = luma_rec709(e);

    let lmin = min(lc, min(min(ln, ls), min(lw, le)));
    let lmax = max(lc, max(max(ln, ls), max(lw, le)));
    let contrast = lmax - lmin;
    if contrast < max(0.0312, lmax * 0.125) {
        return center;
    }

    let nw = grade_and_tonemap(uv + vec2<f32>(-rcp.x, -rcp.y));
    let ne = grade_and_tonemap(uv + vec2<f32>(rcp.x, -rcp.y));
    let sw = grade_and_tonemap(uv + vec2<f32>(-rcp.x, rcp.y));
    let se = grade_and_tonemap(uv + vec2<f32>(rcp.x, rcp.y));
    let lnw = luma_rec709(nw);
    let lne = luma_rec709(ne);
    let lsw = luma_rec709(sw);
    let lse = luma_rec709(se);

    let edge_h = abs(-2.0 * lw + lnw + lsw)
        + 2.0 * abs(-2.0 * lc + ln + ls)
        + abs(-2.0 * le + lne + lse);
    let edge_v = abs(-2.0 * ln + lnw + lne)
        + 2.0 * abs(-2.0 * lc + lw + le)
        + abs(-2.0 * ls + lsw + lse);
    let horiz = edge_h >= edge_v;

    let avg = (n + s + w + e) * 0.125 + (nw + ne + sw + se) * 0.0625 + center * 0.25;
    let avg_luma = luma_rec709(avg);
    let subpix = clamp(abs(avg_luma - lc) / contrast, 0.0, 1.0);
    let subpix_blend = subpix * subpix * 0.75;

    let step = select(vec2<f32>(rcp.x, 0.0), vec2<f32>(0.0, rcp.y), horiz);
    let a = grade_and_tonemap(uv - step * 0.5);
    let b = grade_and_tonemap(uv + step * 0.5);
    return mix((a + b) * 0.5, avg, subpix_blend);
}

// The dedicated R8Unorm selection mask has 1 on selected visible fragments, 0
// elsewhere. Sample only inward: never paint an occluded object through its
// occluder, inflate a silhouette, expose triangle edges, or change material RGB.
fn selection_contour(uv: vec2<f32>, color: vec3<f32>, mask: f32) -> vec3<f32> {
    let texel = vec2<f32>(max(display.parameters.w, 1.0)) / vec2<f32>(textureDimensions(selection_mask));
    var near_edge = 0.0;
    var far_edge = 0.0;
    for (var i = 0u; i < 4u; i++) {
        let direction = array<vec2<f32>, 4>(vec2<f32>(1.0, 0.0), vec2<f32>(-1.0, 0.0), vec2<f32>(0.0, 1.0), vec2<f32>(0.0, -1.0))[i];
        near_edge = max(near_edge, 1.0 - textureSampleLevel(selection_mask, image_sampler, uv + direction * texel * 2.0, 0.0).r);
        far_edge = max(far_edge, 1.0 - textureSampleLevel(selection_mask, image_sampler, uv + direction * texel * 3.5, 0.0).r);
    }
    let backed = mix(color, vec3<f32>(0.018), mask * far_edge);
    // Linear sRGB for the Rot mushroom red (#FA2934).
    let rot_red = vec3<f32>(0.955973, 0.022174, 0.034340);
    return mix(backed, rot_red, mask * near_edge);
}

fn displayed_color(uv: vec2<f32>) -> vec4<f32> {
    var rgb: vec3<f32>;
    if display.post_flags.w > 0.5 && display.parameters.x >= 0.5 {
        rgb = fxaa_color(uv);
    } else {
        rgb = grade_and_tonemap(uv);
    }
    let mask = textureSample(selection_mask, image_sampler, uv).r;
    if display.parameters.z > 0.5 && mask > 0.0 {
        rgb = selection_contour(uv, rgb, mask);
    }
    return vec4<f32>(rgb, 1.0);
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
