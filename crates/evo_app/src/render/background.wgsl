// Ocean background: water lit by the simulated light field, rock from the terrain SDF,
// minerals, marine snow, vents, and the water chemistry (dead zones, red tides, oxygen bubbles).

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

const MAX_VENTS: u32 = 16u;

struct Background {
    world: vec4<f32>, // width, height, time, world units per pixel
    sun: vec4<f32>,   // sun (0..1), surface temp, day phase, vent count
    vents: array<vec4<f32>, 16>,
    effects: vec4<f32>, // impact-winter darkness, event count, map mode
    air: vec4<f32>, // air oxygen, air CO2 (1 = today)
    events: array<vec4<f32>, 8>, // x, width, kind (1 upwelling, 2 wall), param
    land: vec4<f32>,      // mountain outline: first sample x, spacing, sample count, on
    land_profile: array<vec4<f32>, 32>, // its height at each sample (4 per entry)
};

fn land_sample(k: i32) -> f32 {
    let i = clamp(k, 0, i32(bg.land.z) - 1);
    let v = bg.land_profile[i / 4];
    let j = i % 4;
    return select(select(select(v.x, v.y, j == 1), v.z, j == 2), v.w, j == 3);
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> bg: Background;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var terrain_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var terrain_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var light_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var light_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var fields_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var fields_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var chem_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var chem_sampler: sampler;

fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

// Random 0..1 at an integer lattice point, from integer arithmetic: it's exactly the same
// wherever it's computed. (A float hash like `hash21` can come out different for the same
// point in two inlined copies, by one rounding, and then neighboring noise cells disagree at
// their shared edge: hard-edged blocks.)
fn lattice(c: vec2<f32>) -> f32 {
    let x = bitcast<u32>(i32(c.x));
    let y = bitcast<u32>(i32(c.y));
    return f32(pcg(x * 0x8da6b343u ^ y * 0xd8163841u) >> 8u) / 16777216.0;
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = lattice(i);
    let b = lattice(i + vec2<f32>(1.0, 0.0));
    let c = lattice(i + vec2<f32>(0.0, 1.0));
    let d = lattice(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn pcg(v: u32) -> u32 {
    let state = v * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

// Random 0..1 for an integer cell, from integer arithmetic: unlike `hash21`, it stays random
// however far the cell is from the origin (float hashes of large coordinates repeat on a grid).
fn cell_hash(c: vec2<f32>, salt: u32) -> f32 {
    let x = bitcast<u32>(i32(c.x));
    let y = bitcast<u32>(i32(c.y));
    return f32(pcg(x ^ pcg(y ^ pcg(salt))) >> 8u) / 16777216.0;
}

// Specks on a jittered grid of `cell`-sized cells moving with `drift`: 1 inside a speck of
// radius `size` (never under ~1 px), present in a `density` fraction of cells (none at 0).
fn specks(wp: vec2<f32>, cell: f32, drift: vec2<f32>, density: f32, size: f32, px: f32, seed: f32) -> f32 {
    if density <= 0.0 {
        return 0.0;
    }
    let sp = (wp + drift) / cell;
    let c = floor(sp);
    let salt = u32(seed * 1000.0);
    let j = vec2<f32>(cell_hash(c, salt), cell_hash(c, salt + 1u)) * 0.8 + 0.1;
    let r = max(size, 0.8 * px) / cell;
    let d = length(sp - c - j);
    return (1.0 - smoothstep(r * 0.4, r, d)) * select(0.0, 1.0, cell_hash(c, salt + 2u) < density);
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
        // Without oxygen (and its ozone) the day sky is a hazy orange; lots of CO2 adds haze.
        let o2 = clamp(bg.air.x, 0.0, 1.5);
        let haze_day = mix(vec3<f32>(0.62, 0.42, 0.25), vec3<f32>(0.35, 0.34, 0.33), h);
        var day = mix(haze_day, sky_day, smoothstep(0.05, 0.6, o2));
        day = mix(day, vec3<f32>(0.5, 0.48, 0.42), clamp((bg.air.y - 1.5) * 0.25, 0.0, 0.5));
        var sky = mix(sky_night, day * 0.7, clamp(bg.sun.x * 1.3, 0.0, 1.0));
        // Stars at night (hidden by impact dust).
        let winter = bg.effects.x;
        // Fixed in the world (they don't reshuffle on zoom), about a pixel across at any zoom.
        let star_cell = floor(wp / 60.0);
        let twinkle = 0.4 + 0.6 * cell_hash(star_cell, 91u);
        let star = specks(wp, 60.0, vec2<f32>(0.0), 0.12, 0.9 * px, px, 0.9) * twinkle
            * (1.0 - clamp(bg.sun.x * 3.0, 0.0, 1.0));
        sky += vec3<f32>(star) * (1.0 - winter);
        // Impact winter: a churning brown-grey dust pall.
        let dust = fbm(vec2<f32>(wp.x * 0.0008 + time * 0.02, wp.y * 0.002));
        let pall = mix(vec3<f32>(0.09, 0.075, 0.06), vec3<f32>(0.2, 0.16, 0.12), dust);
        sky = mix(sky, pall, clamp(winter * 1.4, 0.0, 0.95));
        // The mountain above sea level (same profile as the terrain below the surface).
        let f = (wp.x - bg.land.x) / max(bg.land.y, 1e-3);
        if bg.land.w > 0.5 && f >= 0.0 && f <= bg.land.z - 1.0 {
            let k = i32(floor(f));
            let ridge = mix(land_sample(k), land_sample(k + 1), fract(f))
                + 4.0 * (fbm(vec2<f32>(wp.x * 0.05, 0.0)) - 0.5);
            let inside = 1.0 - smoothstep(-px, px, wp.y - ridge);
            if inside > 0.0 {
                let n = fbm(wp * 0.006) * 0.55 + fbm(wp * 0.03) * 0.3 + value_noise(wp * 0.15) * 0.15;
                // The same rock as below the waterline, a little brighter by day.
                var land = mix(vec3<f32>(0.03, 0.028, 0.03), vec3<f32>(0.11, 0.095, 0.08), n);
                land *= 0.92 + 0.08 * sin(wp.y * 0.018 + fbm(wp * 0.004) * 9.0);
                let daylight = 0.55 + 0.55 * clamp(bg.sun.x, 0.0, 1.0);
                let crest = 1.0 - smoothstep(0.0, 30.0, ridge - wp.y);
                land *= daylight * (1.0 + 0.4 * crest);
                land = mix(land, pall * 0.8, clamp(winter * 1.2, 0.0, 0.9));
                sky = mix(sky, land, inside);
            }
        }
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
    let chem = textureSample(chem_tex, chem_sampler, uv);
    let oxygen = chem.r * 1.5;
    let nutrients = chem.g;
    let toxin = chem.b;

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

    // Dead zones (almost no oxygen) turn the water murky purple-brown; blooms of toxin
    // redden it (red tides).
    let anoxic = (1.0 - smoothstep(0.05, 0.3, oxygen)) * select(1.0, 0.0, outside);
    let murk = 0.8 + 0.2 * fbm(wp * 0.01 + vec2<f32>(time * 0.01, 0.0));
    water = mix(water, vec3<f32>(0.035, 0.015, 0.03) * murk + water * 0.3, anoxic * 0.7);
    water += vec3<f32>(0.25, 0.02, 0.02) * toxin * (0.7 + 0.3 * fbm(wp * 0.02 - vec2<f32>(0.0, time * 0.05)));
    // What's dissolved in the water, visible when zoomed in: each chemical has its own look.
    let near = 1.0 - smoothstep(1.5, 5.0, px);
    let o2_rich = smoothstep(0.95, 1.3, oxygen);
    // Oxygen-rich water is a little clearer, and fizzes with rising bubbles when supersaturated.
    water += vec3<f32>(0.0, 0.015, 0.025) * o2_rich * select(1.0, 0.0, outside);
    if near > 0.0 && !outside {
        let wobble = vec2<f32>(sin(wp.y * 0.05 + time) * 3.0, 0.0);
        let bubbles = specks(wp + wobble, 9.0, vec2<f32>(0.0, -time * 18.0), o2_rich * 0.4, 1.1, px, 3.1);
        let rim = specks(wp + wobble, 9.0, vec2<f32>(0.0, -time * 18.0), o2_rich * 0.4, 0.6, px, 3.1);
        water += vec3<f32>(0.45, 0.6, 0.65) * (bubbles - rim * 0.6) * near;
        // Minerals: cyan sparks that twinkle.
        let spark = specks(wp, 14.0, vec2<f32>(sin(time * 0.3) * 4.0, -time * 2.0), minerals * 0.7, 0.9, px, 7.7);
        let twinkle = 0.5 + 0.5 * sin(time * 5.0 + hash21(floor(wp / 14.0)) * 30.0);
        water += vec3<f32>(0.3, 1.3, 1.6) * spark * twinkle * near;
        // Nutrients: pale green motes drifting in the light.
        let motes = specks(wp, 17.0, vec2<f32>(time * 1.5, -time * 0.8), nutrients * 0.35, 0.8, px, 12.9);
        water += vec3<f32>(0.25, 0.7, 0.2) * motes * (0.25 + 0.75 * brightness) * near;
        // Toxin: red specks swirling.
        let swirl = vec2<f32>(sin(wp.y * 0.02 + time * 0.7), cos(wp.x * 0.02 + time * 0.6)) * 6.0;
        let poison = specks(wp + swirl, 11.0, vec2<f32>(0.0, 0.0), toxin * 0.9, 1.0, px, 21.3);
        water += vec3<f32>(1.3, 0.15, 0.08) * poison * near;
        // Dead zones: dark silt hanging in the water.
        let silt = specks(wp, 13.0, vec2<f32>(time * 0.5, -time * 0.3), anoxic * 0.45, 1.2, px, 33.3);
        water = mix(water, vec3<f32>(0.02, 0.01, 0.015), silt * 0.8 * near);
    }
    // Nutrient-rich sunlit water is faintly greener.
    water += vec3<f32>(0.0, 0.02, 0.004) * nutrients * brightness * select(1.0, 0.0, outside);

    // Vents: a mineral chimney (a "black smoker") with dark smoke billowing out of its mouth,
    // and a hot glow there. The mouth sits a little above the vent point; the stack's base runs
    // down into the rock, which hides it, so it stands on whatever floor is below.
    let vent_count = u32(bg.sun.w);
    var vent_glow = vec3<f32>(0.0);
    for (var i = 0u; i < min(vent_count, MAX_VENTS); i++) {
        let v = bg.vents[i];
        let strength = v.z;
        let mouth = v.xy + vec2<f32>(0.0, 26.0);
        let q = wp - mouth;
        if abs(q.x) > 700.0 || q.y < -220.0 || q.y > 900.0 {
            continue;
        }
        let seed = hash21(floor(v.xy));
        let aa = max(px, 0.4);

        let dm = wp - mouth;
        let flicker = 0.8 + 0.2 * value_noise(vec2<f32>(time * 3.0, seed * 40.0));

        // Drawn back to front: a warm halo in the water, the smoke, the stack, the mouth's glow.
        let halo = vec3<f32>(0.06, 0.02, 0.007) * exp(-length(dm) / 90.0) * strength;
        water += halo;
        vent_glow += halo;

        // Smoke: dark puffs carried up and spreading, swaying with height; densest at the mouth.
        let rise = max(q.y, 0.0);
        let sway = sin(rise * 0.011 - time * 0.35 + seed * 6.28) * rise * 0.07;
        let width = 4.0 + rise * 0.2 + rise * rise * 0.00025;
        let dx = q.x - sway;
        let profile = exp(-(dx * dx) / (width * width) * 1.6);
        if q.y > -4.0 && profile > 0.01 {
            let up = rise - time * 28.0;
            let billow = fbm(vec2<f32>(dx / (width + 6.0) * 1.3 + seed * 3.0, up / (width * 1.4 + 14.0)));
            let fine = value_noise(vec2<f32>(dx * 0.12, up * 0.08));
            let fade = exp(-rise / 520.0) * smoothstep(-4.0, 2.0, q.y);
            let core = exp(-rise / 35.0);
            let puffs = smoothstep(0.3, 0.6, billow + 0.15 * fine);
            let density = clamp(profile * (puffs * 1.1 + core * 0.8) * fade, 0.0, 1.0) * strength;
            // Dark smoke, shaded so the billows read as a cloud even in dark water: where the
            // noise rises a little lighter, as if their tops caught some light.
            let lit = smoothstep(0.45, 0.85, billow + 0.2 * fine);
            var smoke = mix(vec3<f32>(0.01, 0.009, 0.01), vec3<f32>(0.085, 0.077, 0.07), lit);
            // The hot mouth lights the smoke from below, just above it.
            smoke += vec3<f32>(0.6, 0.22, 0.05) * exp(-rise / 14.0) * profile * strength * flicker;
            water = mix(water, smoke, density * 0.9);
            // Heat shimmer just above the mouth.
            let shimmer = (0.5 + 0.5 * sin(dx * 0.4 + rise * 0.25 - time * 9.0)) * exp(-rise / 60.0) * profile;
            water += vec3<f32>(0.02, 0.01, 0.004) * shimmer * strength;
        }

        // The stack: tapering upward, lumpy, leaning a little.
        let below = max(-q.y, 0.0);
        let lean = sin(below * 0.02 + seed * 6.28) * 3.0 + below * (seed - 0.5) * 0.06;
        let lumps = 1.0 + 0.3 * (value_noise(vec2<f32>(below * 0.09, seed * 50.0)) - 0.5)
            + 0.15 * (value_noise(vec2<f32>(below * 0.3, seed * 90.0)) - 0.5);
        let half_w = (6.0 + 14.0 * pow(min(below / 160.0, 1.0), 0.8)) * lumps;
        let side = abs(q.x - lean);
        let stack = (1.0 - smoothstep(half_w - aa, half_w + aa, side)) * (1.0 - smoothstep(-aa, aa, q.y));
        if stack > 0.0 {
            // Banded dark sulfides with rusty patches; embers in the cracks near the mouth.
            let band = 0.5 + 0.5 * sin(below * 0.35 + value_noise(vec2<f32>(q.x * 0.2, below * 0.1)) * 4.0);
            let grain = value_noise(wp * 0.25);
            var col = mix(vec3<f32>(0.012, 0.01, 0.012), vec3<f32>(0.05, 0.035, 0.025), band * 0.6 + grain * 0.4);
            col = mix(col, vec3<f32>(0.11, 0.05, 0.02), smoothstep(0.7, 0.92, value_noise(wp * 0.08 + seed * 13.0)) * 0.5);
            let heat = exp(-below / 12.0) * strength;
            let cracks = smoothstep(0.8, 0.95, value_noise(vec2<f32>(q.x * 0.5, below * 0.18 - time * 0.2) + seed * 7.0));
            col += vec3<f32>(1.4, 0.4, 0.07) * cracks * heat * flicker + vec3<f32>(0.12, 0.035, 0.01) * heat;
            // Rounded: darker toward the edges, a faint rim catching the light from above.
            let across = clamp(side / max(half_w, 1e-3), 0.0, 1.0);
            col *= 0.5 + 0.5 * (1.0 - across * across);
            col += vec3<f32>(0.05, 0.06, 0.06) * smoothstep(0.75, 1.0, across) * (0.1 + lin);
            water = mix(water, col, stack);
        }

        // The mouth: a small, bright, flickering glow.
        let core_r = 3.0 + 2.0 * strength;
        let glow = vec3<f32>(2.4, 0.85, 0.2) * exp(-dot(dm, dm) / (2.0 * core_r * core_r)) * flicker * strength;
        water += glow;
        vent_glow += glow;
    }

    // Impact winter: dusty haze near the surface.
    let winter = bg.effects.x;
    water = mix(water, vec3<f32>(0.08, 0.07, 0.055), winter * 0.6 * exp(-depth / 900.0));

    // Visible events: upwellings and moving rock walls.
    var wall_glow = vec3<f32>(0.0);
    let event_count = u32(bg.effects.y);
    for (var i = 0u; i < min(event_count, 8u); i++) {
        let e = bg.events[i];
        let kind = u32(e.z + 0.5);
        let dx = wp.x - e.x;
        let half_w = e.y * 0.5;
        if kind == 1u {
            // Upwelling: a column of cold, mineral-rich water with streaks rising to the surface.
            let across = 1.0 - smoothstep(half_w * 0.6, half_w, abs(dx));
            let strength = e.w * across * smoothstep(1800.0, 300.0, depth);
            let streak = pow(0.5 + 0.5 * sin(dx * 0.03 + fbm(vec2<f32>(dx * 0.004, wp.y * 0.003 - time * 0.4)) * 6.0), 6.0);
            let bubble_p = vec2<f32>(wp.x, wp.y - time * 30.0) / max(10.0, 2.0 * px);
            let bubble = step(0.992, hash21(floor(bubble_p))) * (1.0 - smoothstep(1.5, 5.0, px));
            water += (vec3<f32>(0.02, 0.12, 0.16) + vec3<f32>(0.05, 0.2, 0.22) * streak + vec3<f32>(0.4, 0.6, 0.6) * bubble) * strength;
        } else if kind == 2u && e.w < 0.0 {
            // A wall on the move: glowing cracks along it and a dust cloud at its top.
            let top = -e.w;
            let near = 1.0 - smoothstep(half_w, half_w + 120.0, abs(dx));
            let dust_cloud = exp(-pow((wp.y - top) / 120.0, 2.0)) * (1.0 - smoothstep(half_w, half_w + 300.0, abs(dx)));
            let churn = fbm(vec2<f32>(wp.x * 0.01, wp.y * 0.01 - time * 0.5));
            water = mix(water, vec3<f32>(0.18, 0.15, 0.11), dust_cloud * churn * 0.8);
            wall_glow += vec3<f32>(1.6, 0.5, 0.12) * near * step(wp.y, top) * pow(fbm(vec2<f32>(wp.x * 0.05, wp.y * 0.02)), 4.0) * 1.0;
        }
    }

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
    rock += wall_glow * smoothstep(-80.0, 0.0, sdf);
    // Depth fog on rock.
    rock *= mix(1.0, 0.5, clamp(depth / size.y, 0.0, 1.0));

    // Sediment: marine snow piles up on the floor as a soft brown carpet.
    let carpet = (1.0 - smoothstep(0.0, 10.0 + 25.0 * detritus, sdf)) * up * clamp(detritus * 3.0, 0.0, 1.0);
    let fluff = 0.7 + 0.3 * value_noise(wp * 0.3);
    water = mix(water, vec3<f32>(0.16, 0.12, 0.08) * fluff * (0.4 + lin), carpet * 0.8);

    // Chemistry map (key M): oxygen, nutrients or toxin laid over the water.
    let map_mode = u32(bg.effects.z + 0.5);
    if map_mode != 0u && !outside {
        var map_col = vec3<f32>(0.0);
        if map_mode == 1u {
            let o = clamp(oxygen, 0.0, 1.5) / 1.5;
            map_col = mix(vec3<f32>(0.45, 0.05, 0.4), vec3<f32>(0.1, 0.35, 0.95), smoothstep(0.0, 0.6, o));
            map_col = mix(map_col, vec3<f32>(0.6, 0.95, 1.0), smoothstep(0.66, 1.0, o));
        } else if map_mode == 2u {
            let c = chem.a; // CO2, 0..2 scaled to 0..1
            map_col = mix(vec3<f32>(0.05, 0.1, 0.3), vec3<f32>(0.95, 0.75, 0.2), c);
        } else if map_mode == 3u {
            map_col = mix(vec3<f32>(0.03, 0.03, 0.05), vec3<f32>(0.3, 1.0, 0.25), nutrients);
        } else if map_mode == 4u {
            map_col = mix(vec3<f32>(0.03, 0.03, 0.05), vec3<f32>(0.2, 0.95, 1.1), minerals);
        } else {
            map_col = mix(vec3<f32>(0.03, 0.03, 0.05), vec3<f32>(1.0, 0.15, 0.1), toxin);
        }
        water = mix(water, map_col * 0.6, 0.75);
    }

    let edge = smoothstep(-px, px, sdf);
    var col = mix(rock, water, edge);
    if outside {
        col = mix(rock * 0.35, vec3<f32>(0.01, 0.012, 0.02), 0.6);
    }
    // Never negative: bloom smears negative values into blocky artifacts.
    return vec4<f32>(max(col, vec3<f32>(0.0)), 1.0);
}
