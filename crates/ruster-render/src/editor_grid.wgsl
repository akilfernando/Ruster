// A world-space ground guide. It shares the scene camera and depth attachment.
struct Camera {
    view_projection: mat4x4<f32>,
    eye_mode: vec4<f32>,
}
@group(0) @binding(0) var<uniform> camera: Camera;

struct Varying {
    @builtin(position) position: vec4<f32>,
    @location(0) ground: vec2<f32>,
    @location(1) @interpolate(flat) prominence: f32,
}

@vertex
fn vs(@builtin(vertex_index) vertex: u32) -> Varying {
    let line = vertex / 2u;
    let coordinate = f32(line % 89u) - 44.0;
    let endpoint = select(-44.0, 44.0, vertex % 2u != 0u);
    let ground = select(vec2(coordinate, endpoint), vec2(endpoint, coordinate), line >= 89u);
    var out: Varying;
    out.position = camera.view_projection * vec4(ground.x, -0.001, ground.y, 1.0);
    out.ground = ground;
    out.prominence = select(0.14, 0.26, (i32(coordinate) % 5) == 0);
    return out;
}

@fragment
fn fs(input: Varying) -> @location(0) vec4<f32> {
    let fade = 1.0 - smoothstep(18.0, 43.5, length(input.ground));
    return vec4(0.21, 0.16, 0.15, input.prominence * fade);
}
