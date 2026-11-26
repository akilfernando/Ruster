struct BloomUniform {
    texel_size: vec4<f32>, // xy: 1/src_w, 1/src_h
    params: vec4<f32>,     // x: threshold, y: soft_knee, z: filter_radius
};

@group(0) @binding(0) var<uniform> u: BloomUniform;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;

struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex fn vs(@builtin(vertex_index) index: u32) -> Output {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var o: Output;
    o.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return o;
}

fn luma_weight(c: vec3<f32>) -> f32 {
    let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    return 1.0 / (1.0 + max(luma, 0.0));
}

fn prefilter_threshold(rgb: vec3<f32>) -> vec3<f32> {
    let c = clamp(rgb, vec3<f32>(0.0), vec3<f32>(65504.0));
    let brightness = max(c.r, max(c.g, c.b));
    let threshold = u.params.x;
    let knee = threshold * u.params.y + 1e-5;
    let soft = clamp(brightness - threshold + knee, 0.0, 2.0 * knee);
    let contribution = max(soft * soft / (4.0 * knee), max(brightness - threshold, 0.0));
    return c * (contribution / max(brightness, 1e-5));
}

fn downsample_13_tap(uv: vec2<f32>, karis: bool) -> vec3<f32> {
    let t = u.texel_size.xy;
    let a = textureSampleLevel(src_tex, linear_sampler, uv + vec2<f32>(-2.0, -2.0) * t, 0.0).rgb;
    let b = textureSampleLevel(src_tex, linear_sampler, uv + vec2<f32>(0.0, -2.0) * t, 0.0).rgb;
    let c = textureSampleLevel(src_tex, linear_sampler, uv + vec2<f32>(2.0, -2.0) * t, 0.0).rgb;
    let d = textureSampleLevel(src_tex, linear_sampler, uv + vec2<f32>(-1.0, -1.0) * t, 0.0).rgb;
    let e = textureSampleLevel(src_tex, linear_sampler, uv, 0.0).rgb;
    let f = textureSampleLevel(src_tex, linear_sampler, uv + vec2<f32>(1.0, -1.0) * t, 0.0).rgb;
    let g = textureSampleLevel(src_tex, linear_sampler, uv + vec2<f32>(-2.0, 0.0) * t, 0.0).rgb;
    let h = textureSampleLevel(src_tex, linear_sampler, uv + vec2<f32>(2.0, 0.0) * t, 0.0).rgb;
    let i = textureSampleLevel(src_tex, linear_sampler, uv + vec2<f32>(-1.0, 1.0) * t, 0.0).rgb;
    let j = textureSampleLevel(src_tex, linear_sampler, uv + vec2<f32>(1.0, 1.0) * t, 0.0).rgb;
    let k = textureSampleLevel(src_tex, linear_sampler, uv + vec2<f32>(-2.0, 2.0) * t, 0.0).rgb;
    let l = textureSampleLevel(src_tex, linear_sampler, uv + vec2<f32>(0.0, 2.0) * t, 0.0).rgb;
    let m = textureSampleLevel(src_tex, linear_sampler, uv + vec2<f32>(2.0, 2.0) * t, 0.0).rgb;

    let q0 = (a + b + g + e) * 0.25;
    let q1 = (b + c + e + h) * 0.25;
    let q2 = (g + e + k + l) * 0.25;
    let q3 = (e + h + l + m) * 0.25;
    let q4 = (d + f + i + j) * 0.25;

    if karis {
        let p0 = prefilter_threshold(q0);
        let p1 = prefilter_threshold(q1);
        let p2 = prefilter_threshold(q2);
        let p3 = prefilter_threshold(q3);
        let p4 = prefilter_threshold(q4);
        let w0 = luma_weight(p0) * 0.125;
        let w1 = luma_weight(p1) * 0.125;
        let w2 = luma_weight(p2) * 0.125;
        let w3 = luma_weight(p3) * 0.125;
        let w4 = luma_weight(p4) * 0.5;
        return (p0 * w0 + p1 * w1 + p2 * w2 + p3 * w3 + p4 * w4) / max(w0 + w1 + w2 + w3 + w4, 1e-5);
    }

    return q4 * 0.5 + (q0 + q1 + q2 + q3) * 0.125;
}

@fragment fn fs_prefilter(v: Output) -> @location(0) vec4<f32> {
    return vec4<f32>(downsample_13_tap(v.uv, true), 1.0);
}

@fragment fn fs_downsample(v: Output) -> @location(0) vec4<f32> {
    return vec4<f32>(downsample_13_tap(v.uv, false), 1.0);
}

@fragment fn fs_upsample(v: Output) -> @location(0) vec4<f32> {
    let t = u.texel_size.xy * u.params.z;
    let c0 = textureSampleLevel(src_tex, linear_sampler, v.uv + vec2<f32>(-t.x, -t.y), 0.0).rgb;
    let c1 = textureSampleLevel(src_tex, linear_sampler, v.uv + vec2<f32>(0.0, -t.y), 0.0).rgb * 2.0;
    let c2 = textureSampleLevel(src_tex, linear_sampler, v.uv + vec2<f32>(t.x, -t.y), 0.0).rgb;
    let c3 = textureSampleLevel(src_tex, linear_sampler, v.uv + vec2<f32>(-t.x, 0.0), 0.0).rgb * 2.0;
    let c4 = textureSampleLevel(src_tex, linear_sampler, v.uv, 0.0).rgb * 4.0;
    let c5 = textureSampleLevel(src_tex, linear_sampler, v.uv + vec2<f32>(t.x, 0.0), 0.0).rgb * 2.0;
    let c6 = textureSampleLevel(src_tex, linear_sampler, v.uv + vec2<f32>(-t.x, t.y), 0.0).rgb;
    let c7 = textureSampleLevel(src_tex, linear_sampler, v.uv + vec2<f32>(0.0, t.y), 0.0).rgb * 2.0;
    let c8 = textureSampleLevel(src_tex, linear_sampler, v.uv + vec2<f32>(t.x, t.y), 0.0).rgb;
    let blurred = (c0 + c1 + c2 + c3 + c4 + c5 + c6 + c7 + c8) * (1.0 / 16.0);
    return vec4<f32>(clamp(blurred, vec3<f32>(0.0), vec3<f32>(65504.0)), 1.0);
}
