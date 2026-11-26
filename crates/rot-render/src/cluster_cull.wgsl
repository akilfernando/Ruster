struct ClusterParams {
    view: mat4x4<f32>,
    inv_projection: mat4x4<f32>,
    // x: screen_w, y: screen_h, z: near, w: far
    screen_z_params: vec4<f32>,
    // x: z_scale, y: z_bias, z: is_ortho (>0.5), w: reserved
    slice_params: vec4<f32>,
    // x: cx, y: cy, z: cz, w: local_light_count
    grid_counts: vec4<u32>,
};

struct LocalLight {
    position_range: vec4<f32>,
    color_inv_range_sq: vec4<f32>,
    spot_axis_outer: vec4<f32>,
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: ClusterParams;
@group(0) @binding(1) var<storage, read> lights: array<LocalLight>;
@group(0) @binding(2) var<storage, read_write> cluster_counts: array<u32>;
@group(0) @binding(3) var<storage, read_write> cluster_indices: array<u32>;

const MAX_LIGHTS_PER_CLUSTER: u32 = 64u;

fn unproject_tile_xy(ndc_x: f32, ndc_y: f32) -> vec3<f32> {
    let h = params.inv_projection * vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    let w = select(h.w, 1.0, abs(h.w) < 1e-6);
    return h.xyz / w;
}

@compute @workgroup_size(4, 4, 4)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let cx = params.grid_counts.x;
    let cy = params.grid_counts.y;
    let cz = params.grid_counts.z;
    if gid.x >= cx || gid.y >= cy || gid.z >= cz {
        return;
    }

    let cluster_idx = gid.x + gid.y * cx + gid.z * cx * cy;
    let light_count = params.grid_counts.w;
    if light_count == 0u {
        cluster_counts[cluster_idx] = 0u;
        return;
    }

    // Fragment @builtin(position).y = 0 is top of viewport (NDC y = +1).
    let x0 = -1.0 + 2.0 * f32(gid.x) / f32(cx);
    let x1 = -1.0 + 2.0 * f32(gid.x + 1u) / f32(cx);
    let y_top = 1.0 - 2.0 * f32(gid.y) / f32(cy);
    let y_bot = 1.0 - 2.0 * f32(gid.y + 1u) / f32(cy);

    let near = params.screen_z_params.z;
    let far = params.screen_z_params.w;
    let t0 = f32(gid.z) / f32(cz);
    let t1 = f32(gid.z + 1u) / f32(cz);
    var z_near_slice: f32;
    var z_far_slice: f32;
    if params.slice_params.z >= 0.5 {
        z_near_slice = mix(near, far, t0);
        z_far_slice = mix(near, far, t1);
    } else {
        let ratio = max(far / max(near, 1e-5), 1.0001);
        z_near_slice = near * pow(ratio, t0);
        z_far_slice = near * pow(ratio, t1);
    }

    var aabb_min = vec3<f32>(1e30);
    var aabb_max = vec3<f32>(-1e30);
    let corners = array<vec2<f32>, 4>(
        vec2<f32>(x0, y_bot),
        vec2<f32>(x1, y_bot),
        vec2<f32>(x0, y_top),
        vec2<f32>(x1, y_top),
    );
    for (var c = 0u; c < 4u; c = c + 1u) {
        let corner = corners[c];
        let ray = unproject_tile_xy(corner.x, corner.y);
        var p_near: vec3<f32>;
        var p_far: vec3<f32>;
        if params.slice_params.z >= 0.5 {
            p_near = vec3<f32>(ray.xy, -z_near_slice);
            p_far = vec3<f32>(ray.xy, -z_far_slice);
        } else {
            let denom = max(-ray.z, 1e-6);
            p_near = ray * (z_near_slice / denom);
            p_far = ray * (z_far_slice / denom);
        }
        aabb_min = min(aabb_min, min(p_near, p_far));
        aabb_max = max(aabb_max, max(p_near, p_far));
    }

    let base_offset = cluster_idx * MAX_LIGHTS_PER_CLUSTER;
    var count = 0u;
    for (var i = 0u; i < light_count; i = i + 1u) {
        let light = lights[i];
        let radius = light.position_range.w;
        let center = (params.view * vec4<f32>(light.position_range.xyz, 1.0)).xyz;
        let closest = clamp(center, aabb_min, aabb_max);
        let delta = center - closest;
        if dot(delta, delta) <= radius * radius {
            if count < MAX_LIGHTS_PER_CLUSTER {
                cluster_indices[base_offset + count] = i;
                count = count + 1u;
            }
        }
    }
    cluster_counts[cluster_idx] = count;
}
