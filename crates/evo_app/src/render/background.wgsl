// Ocean background: water lit by the simulated light field, rock from the terrain SDF,
// minerals, marine snow, and vents.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

const MAX_VENTS: u32 = 16u;

struct Background {
    world: vec4<f32>, // width, height, time, world units per pixel
    sun: vec4<f32>,   // sun (0..1), surface temp, day phase, vent count
    vents: array<vec4<f32>, 16>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> bg: Background;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var terrain_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var terrain_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var light_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var light_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var fields_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var fields_sampler: sampler;

fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 4; i++) {
        sum += amp * value_noise(q);
        q *= 2.03;
        amp *= 0.5;
    }
    return sum;
}

fn sdf_at(uv: vec2<f32>) -> f32 {
    return (textureSample(terrain_tex, terrain_sampler, uv).r - 0.5) * 128.0;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let size = bg.world.xy;
    let time = bg.world.z;
    let px = bg.world.w;
    let wp = in.world_position.xy;
    let uv = vec2<f32>(wp.x / size.x, 1.0 - wp.y / size.y);
    let depth = size.y - wp.y;
    let outside = wp.x < 0.0 || wp.x > size.x || wp.y < 0.0 || wp.y > size.y;

    // Above the surface: night/day sky.
    if wp.y > size.y {
        let h = clamp((wp.y - size.y) / 1200.0, 0.0, 1.0);
        let sky_day = mix(vec3<f32>(0.35, 0.55, 0.75), vec3<f32>(0.12, 0.28, 0.55), h);
        let sky_night = mix(vec3<f32>(0.02, 0.03, 0.07), vec3<f32>(0.005, 0.008, 0.02), h);
        var sky = mix(sky_night, sky_day * 0.7, clamp(bg.sun.x * 1.3, 0.0, 1.0));
        // Stars at night.
        let star = step(0.9985, hash21(floor(wp / max(2.0, 1.5 * px)))) * (1.0 - clamp(bg.sun.x * 3.0, 0.0, 1.0));
        sky += vec3<f32>(star);
        return vec4<f32>(sky, 1.0);
    }

    // Soften the simulated shadows horizontally with depth (light scatters in water).
    let spread = clamp(depth * 0.12, 0.0, 260.0) / size.x;
    var light = textureSample(light_tex, light_sampler, uv).r * 0.28;
    light += textureSample(light_tex, light_sampler, uv + vec2<f32>(spread, 0.0)).r * 0.2;
    light += textureSample(light_tex, light_sampler, uv - vec2<f32>(spread, 0.0)).r * 0.2;
    light += textureSample(light_tex, light_sampler, uv + vec2<f32>(spread * 2.2, 0.0)).r * 0.16;
    light += textureSample(light_tex, light_sampler, uv - vec2<f32>(spread * 2.2, 0.0)).r * 0.16;
    light = select(light, 0.0, outside);
    let lin = light * light;
    let fields = textureSample(fields_tex, fields_sampler, uv);
    let minerals = fields.r;
    let detritus = fields.g;

    // Water: deep navy to sunlit teal, following the simulated light (shadows included).
    // `lin` is irradiance relative to the brightest noon; perceived brightness is roughly log.
    let brightness = clamp(1.0 + log2(max(lin, 1e-4)) / 9.0, 0.0, 1.0);
    let deep = vec3<f32>(0.002, 0.005, 0.016);
    let mid = vec3<f32>(0.006, 0.04, 0.085);
    let shallow = vec3<f32>(0.04, 0.22, 0.30);
    var water = mix(deep, mid, smoothstep(0.0, 0.55, brightness));
    water = mix(water, shallow, smoothstep(0.55, 1.0, brightness));
    // Gentle darkening with depth even where light is similar, for a sense of scale.
    water *= mix(1.0, 0.6, clamp(depth / size.y, 0.0, 1.0));

    // Soft god rays in the upper water.
    let ray_x = wp.x * 0.0016 + sin(wp.x * 0.0005 + time * 0.05) * 2.0 + depth * 0.0003;
    let rays = pow(0.5 + 0.5 * sin(ray_x + time * 0.12), 6.0) * 0.6
        + pow(0.5 + 0.5 * sin(ray_x * 1.7 - time * 0.09 + 1.3), 8.0) * 0.4;
    water += vec3<f32>(0.035, 0.07, 0.065) * rays * lin * exp(-depth / 350.0);

    // Surface shimmer.
    let surf = exp(-depth / (6.0 * px + 4.0));
    water += vec3<f32>(0.4, 0.6, 0.6) * surf * (0.3 + 0.7 * bg.sun.x);

    // Minerals glow faintly cyan; detritus as drifting marine snow.
    water += vec3<f32>(0.0, 0.16, 0.2) * minerals;
    water += vec3<f32>(0.03, 0.025, 0.015) * detritus;
    // Flakes: one jittered speck per 12-unit cell, sinking slowly; faded out when too small to see.
    let snow_cell = 12.0;
    let sp = vec2<f32>(wp.x, wp.y + time * 5.0) / snow_cell;
    let sc = floor(sp);
    let jitter = vec2<f32>(hash21(sc), hash21(sc + 31.7));
    let flake_r = max(0.9, 0.8 * px) / snow_cell;
    let fd = length(sp - sc - jitter);
    let flake = (1.0 - smoothstep(flake_r * 0.5, flake_r, fd)) * step(hash21(sc + 7.3), detritus * 0.8)
        * (1.0 - smoothstep(1.5, 4.0, px));
    water += vec3<f32>(0.22, 0.2, 0.16) * flake;

    // Vents: hot glow and a shimmering plume.
    let vent_count = u32(bg.sun.w);
    var vent_glow = vec3<f32>(0.0);
    for (var i = 0u; i < min(vent_count, MAX_VENTS); i++) {
        let v = bg.vents[i];
        let dv = wp - v.xy;
        let strength = v.z;
        let r = v.w;
        let core_r = r * 0.45;
        let core = exp(-dot(dv, dv) / (2.0 * core_r * core_r));
        let rise = max(dv.y, 0.0);
        let plume_w = r * (0.5 + rise / 500.0);
        let wobble = sin(rise * 0.02 - time * 1.3) * plume_w * 0.3;
        let plume = exp(-(dv.x - wobble) * (dv.x - wobble) / (2.0 * plume_w * plume_w)) * exp(-rise / 450.0) * step(0.0, dv.y);
        let flicker = 0.75 + 0.25 * value_noise(vec2<f32>(dv.x * 0.03, rise * 0.015 - time * 1.8));
        let halo = exp(-length(dv) / (r * 2.5));
        vent_glow += (vec3<f32>(2.4, 0.85, 0.2) * core * 1.1 + vec3<f32>(0.12, 0.045, 0.02) * (plume * flicker + halo)) * strength;
    }
    water += vent_glow;

    // Rock. The edge is roughened with noise so the terrain grid doesn't show.
    let rough = (fbm(wp * 0.04) - 0.5) * 10.0 + (value_noise(wp * 0.2) - 0.5) * 2.5;
    let sdf = sdf_at(uv) + rough;
    let texel = 1.0 / vec2<f32>(textureDimensions(terrain_tex));
    let gx = sdf_at(uv + vec2<f32>(texel.x, 0.0)) - sdf_at(uv - vec2<f32>(texel.x, 0.0));
    let gy = sdf_at(uv - vec2<f32>(0.0, texel.y)) - sdf_at(uv + vec2<f32>(0.0, texel.y));
    let normal = normalize(vec2<f32>(gx, gy) + vec2<f32>(0.0, 1e-4));
    let n = fbm(wp * 0.006) * 0.55 + fbm(wp * 0.03) * 0.3 + value_noise(wp * 0.15) * 0.15;
    var rock = mix(vec3<f32>(0.03, 0.028, 0.03), vec3<f32>(0.11, 0.095, 0.08), n);
    // Faint, wavy strata.
    rock *= 0.92 + 0.08 * sin(wp.y * 0.018 + fbm(wp * 0.004) * 9.0);
    // Rim lit from above by the local light, plus sediment on upward faces.
    let rim = smoothstep(-18.0, 0.0, sdf);
    let up = max(normal.y, 0.0);
    rock += vec3<f32>(0.22, 0.28, 0.27) * rim * up * (0.06 + lin * 1.1);
    rock = mix(rock, vec3<f32>(0.15, 0.115, 0.07), rim * up * clamp(detritus * 2.0 + 0.2, 0.0, 0.85));
    rock += vent_glow * 0.5 * rim;
    // Depth fog on rock.
    rock *= mix(1.0, 0.5, clamp(depth / size.y, 0.0, 1.0));

    let edge = smoothstep(-px, px, sdf);
    var col = mix(rock, water, edge);
    if outside {
        col = mix(rock * 0.35, vec3<f32>(0.01, 0.012, 0.02), 0.6);
    }
    return vec4<f32>(col, 1.0);
}
