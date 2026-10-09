// Ocean background: water lit by the simulated light field, rock from the terrain SDF,
// minerals, marine snow, vents, and the water chemistry (dead zones, red tides, oxygen bubbles).

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

const MAX_VENTS: u32 = 16u;

struct Background {
    world: vec4<f32>, // width, height, time, world units per pixel
    sun: vec4<f32>,   // sun (0..1), surface temp, day phase, vent count
    vents: array<vec4<f32>, 16>, // x, y, strength, how far below it its chimney reaches
    effects: vec4<f32>, // impact-winter darkness, event count, map mode
    air: vec4<f32>, // air oxygen, air CO2 (1 = today)
    events: array<vec4<f32>, 8>, // x, width, kind (1 upwelling, 2 wall), param
    land: vec4<f32>,      // mountain outline: first sample x, spacing, sample count, on
    land_profile: array<vec4<f32>, 64>, // its height at each sample (4 per entry)
    sky: vec4<f32>,  // year phase (0 = start of spring), surface ice 0..1, moon phase (0 = full), snowfall 0..1
    orbit: vec4<f32>, // sun and moon path: center x, half width, height above the surface, 0
    vent_ceilings: array<vec4<f32>, 16>, // rock over each vent: height (1e4 = none), reach left, right
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

const TAU: f32 = 6.2831853;

// How much each season (spring, summer, autumn, winter) shows at this point of the year:
// each peaks mid-season and blends into its neighbours.
fn season_weights(phase: f32) -> vec4<f32> {
    var w = vec4<f32>(0.0);
    for (var i = 0; i < 4; i++) {
        let d = fract(phase - (f32(i) + 0.5) * 0.25 + 0.5) - 0.5;
        let c = max(cos(d * TAU), 0.0);
        w[i] = c * c;
    }
    return w / max(w.x + w.y + w.z + w.w, 1e-4);
}

// Ice on the surface above x: (cover 0..1, thickness in world units). Floes with open
// leads between them; more of the surface freezes over as `amount` grows.
fn ice_at(x: f32, amount: f32, time: f32, px: f32) -> vec2<f32> {
    if amount <= 0.0 {
        return vec2<f32>(0.0);
    }
    let drift = time * 0.6;
    let n = value_noise(vec2<f32>((x + drift) / 260.0, 3.7)) * 0.65
        + value_noise(vec2<f32>((x + drift) / 60.0, 9.1)) * 0.35;
    let level = amount * 0.95 + 0.05;
    let soft = max(0.012, px / 260.0);
    let cover = 1.0 - smoothstep(level - soft, level + soft, n);
    // Thicker floes ride higher and sink deeper; chunky enough to read when zoomed out. Each
    // floe thins toward its edges, so they're rounded slabs rather than cut blocks.
    let inner = sqrt(clamp((level - n) / 0.06, 0.0, 1.0));
    let thick = (8.0 + 26.0 * amount) * (0.7 + 0.6 * value_noise(vec2<f32>((x + drift) / 45.0, 1.3)));
    return vec2<f32>(cover, max(thick * (0.25 + 0.75 * inner), 4.0 * px));
}

// The colour of ice at height `y` within a floe of thickness `thick` whose waterline is at
// `y = 0` (most of a floe sits below it).
fn ice_color(p: vec2<f32>, y: f32, thick: f32, sun: f32) -> vec3<f32> {
    let lit = 0.12 + 0.88 * clamp(sun * 1.4, 0.0, 1.0);
    let t = clamp((y + 0.85 * thick) / thick, 0.0, 1.0); // 0 = underside, 1 = top
    var c = mix(vec3<f32>(0.22, 0.38, 0.5), vec3<f32>(0.62, 0.78, 0.88), t);
    c *= 0.88 + 0.12 * value_noise(p * vec2<f32>(0.08, 0.3));
    // Snow on top.
    c = mix(c, vec3<f32>(0.9, 0.94, 0.97), smoothstep(0.82, 0.95, t));
    // Moonlight keeps it faintly blue at night.
    return c * lit + vec3<f32>(0.02, 0.035, 0.06) * (1.0 - lit);
}

// Smooth minimum (blends two distance fields with rounded joins).
fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

// A vent's outlets for each layout: (x, y of the mouth relative to the vent point, size);
// size 0 = unused.
fn vent_outlets(variant: u32) -> array<vec3<f32>, 3> {
    if variant == 1u {
        // Twin chimneys.
        return array<vec3<f32>, 3>(vec3<f32>(-9.0, 34.0, 0.95), vec3<f32>(17.0, 16.0, 0.65), vec3<f32>(0.0));
    } else if variant == 2u {
        // A chimney with three spires (middle tallest, right lowest).
        return array<vec3<f32>, 3>(vec3<f32>(-19.5, 50.0, 0.7), vec3<f32>(0.0, 58.0, 0.85), vec3<f32>(19.5, 33.0, 0.6));
    } else if variant == 3u {
        // A squat cone with one wide mouth.
        return array<vec3<f32>, 3>(vec3<f32>(0.0, 12.0, 1.45), vec3<f32>(0.0), vec3<f32>(0.0));
    }
    // A tall chimney.
    return array<vec3<f32>, 3>(vec3<f32>(0.0, 44.0, 1.0), vec3<f32>(0.0), vec3<f32>(0.0));
}

struct Plume {
    density: f32, // smoke here (0 = none; not capped)
    height: f32,  // fraction of the way to where it stops rising (0 at the mouth)
    across: f32,  // across the plume: -1..1 edge to edge (0 = its middle)
    lit: f32,     // -1..1: the underside of a billow (in its own shadow) .. its top (lit)
};

// A steady smoke plume from a mouth at the origin, as black smokers vent: a jet that widens
// as it takes in water, slows near the height where it's no heavier than the water around it
// (`top`), then spreads out sideways and drifts with the current (`current`, -1..1) until it
// dissolves. Rock overhead (`roof` above the mouth) stops it: it spreads out sideways in a
// layer underneath instead. Its eddies ride up with it: each patch of smoke keeps its shape as it rises,
// stretched sideways as the plume widens and squashed as it slows, and churns as it goes.
fn smoke_plume(p: vec2<f32>, size: f32, strength: f32, current: f32, roof: vec3<f32>, time: f32, seed: f32) -> Plume {
    var out: Plume;
    out.density = 0.0;
    out.height = 0.0;
    out.across = 1.0;
    out.lit = 0.0;
    if p.y < -2.0 || strength <= 0.01 || p.y > roof.x + 16.0 {
        return out;
    }
    let h = max(p.y, 0.0);
    let top = 240.0 * (0.6 + 0.4 * size) * (0.6 + 0.4 * strength);
    let s = h / top;
    let rise = 55.0 * (0.7 + 0.3 * strength);
    let amount = 0.5 + 0.5 * strength;
    // Bent by the current more the higher it gets, with a slow sway.
    let sway = value_noise(vec2<f32>(h * 0.005 - time * 0.07, seed * 50.0)) - 0.5;
    let center = current * 80.0 * s * s + sway * (6.0 + 50.0 * s);
    let dx = p.x - center;
    // Half-width: widening steadily, much faster where it spreads out on top (or flares out
    // as it meets rock overhead).
    let rising = 7.0 * size + 0.17 * h + 0.25 * max(h - top, 0.0);
    let layer = 20.0 + 0.12 * roof.x;
    let pool = smoothstep(roof.x - 2.0 * layer, roof.x, h);
    let w = rising * (1.0 + 0.8 * pool * pool);
    let u = dx / w;
    if abs(u) < 1.8 {
        // When the smoke here left the mouth: it rises at `rise` there, slowing with height.
        let slow = 900.0;
        let launched = time - (h + h * h / (2.0 * slow)) / rise;
        // The pattern rides with the smoke (fixed in launch time), churning more the longer
        // it has been out. Eddies grow as the plume widens, but less than it does (more of
        // them across a wide plume).
        let v = launched * 1.5;
        let x = dx / (10.0 + 0.4 * w);
        let churn = fbm(vec2<f32>(x * 0.7 + seed * 7.0, v * 0.5 + time * 0.12 + 11.0)) - 0.5;
        let uw = u + churn * (0.25 + 0.45 * min(s, 1.5));
        let lp = vec2<f32>(x + churn * 1.2 + seed * 13.0, v * 0.8 + churn * 1.5);
        let lumps = fbm(lp);
        // A little higher up (toward the light): less smoke there means this is a billow's top.
        let above = fbm(lp + vec2<f32>(0.08 * sign(uw), -0.22));
        // Lumpy edges: the outline bulges into billows and breaks into wisps.
        let edge = abs(uw) + (lumps - 0.5) * (1.5 + 0.5 * min(s, 1.0));
        let soft = 0.06 + 0.22 * min(s, 1.5);
        let profile = 1.0 - smoothstep(0.55 - soft, 0.55 + soft, edge);
        // Thickest in the jet, thinning as it mixes, gone a while after it stops rising.
        let dilute = 1.0 / (1.0 + h / 70.0);
        let fade = (1.0 - smoothstep(1.0, 2.6, s)) * smoothstep(-2.0, 3.0, p.y)
            * (1.0 - smoothstep(roof.x, roof.x + 16.0, h));
        out.density = profile * (0.55 + 0.9 * lumps) * (0.35 + 2.4 * dilute) * fade * amount;
        out.height = s;
        out.across = uw;
        out.lit = clamp((lumps - above) * 9.0, -1.0, 1.0);
    }

    // Under rock: the smoke that reaches it spreads out sideways in a layer beneath it, thick
    // over the plume and thinning out toward the rock's edge on each side (`roof`: its height,
    // reach left, reach right), flowing outward and slowing as it goes.
    let below = roof.x - h;
    if below < 3.0 * layer {
        let sr = roof.x / top;
        if sr < 2.4 {
            let sway_r = value_noise(vec2<f32>(roof.x * 0.005 - time * 0.07, seed * 50.0)) - 0.5;
            let at = p.x - (current * 80.0 * sr * sr + sway_r * (6.0 + 50.0 * sr));
            let along = abs(at);
            let reach = min(320.0 * (0.6 + 0.4 * size), 0.85 * select(roof.y, roof.z, at > 0.0));
            if along < reach && reach > 1.0 {
                let f = along / reach;
                // Each side its own pattern, blended where they meet over the plume.
                let side = smoothstep(-20.0, 20.0, at) * 5.3;
                let flow = (time * rise * 0.5 - along - along * along / (2.0 * max(reach, 60.0))) / (0.9 * layer);
                let lp = vec2<f32>(below / (0.8 * layer) + side + seed * 3.0, flow);
                let lumps = fbm(lp);
                let thick = layer * (1.25 - 0.75 * f);
                let edge = below / thick + (lumps - 0.5) * 1.2;
                let profile = 1.0 - smoothstep(0.55, 1.05, edge);
                let dilute = 1.0 / (1.0 + roof.x / 70.0);
                // Fed by the plume: none where the plume doesn't reach the rock.
                let fed = 1.0 - smoothstep(1.0, 2.4, sr);
                let d = profile * (0.55 + 0.9 * lumps) * (0.35 + 2.4 * dilute) * amount * fed
                    * (1.0 - smoothstep(0.55, 1.0, f)) * (1.0 - smoothstep(-4.0, 16.0, -below));
                if d > 0.0 {
                    let total = out.density + d;
                    out.height = (out.height * out.density + sr * d) / total;
                    out.across = (out.across * out.density + 0.5 * d) / total;
                    // The underside of the layer is in its own shadow.
                    out.lit = (out.lit * out.density - 0.3 * d) / total;
                    out.density = total;
                }
            }
        }
    }
    return out;
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

    // Above the surface: the sky, by season and time of day.
    let year = bg.sky.x;
    let ice = bg.sky.y;
    if wp.y > size.y {
        let h = clamp((wp.y - size.y) / 1200.0, 0.0, 1.0);
        // Each season has its own sky: fresh spring blue, deep summer blue, warm hazy autumn,
        // pale grey winter.
        let sw = season_weights(year);
        let horizon = vec3<f32>(0.44, 0.7, 0.78) * sw.x + vec3<f32>(0.38, 0.62, 0.86) * sw.y
            + vec3<f32>(0.66, 0.56, 0.44) * sw.z + vec3<f32>(0.66, 0.72, 0.78) * sw.w;
        let zenith = vec3<f32>(0.1, 0.38, 0.66) * sw.x + vec3<f32>(0.06, 0.22, 0.62) * sw.y
            + vec3<f32>(0.2, 0.26, 0.44) * sw.z + vec3<f32>(0.3, 0.38, 0.52) * sw.w;
        let sky_day = mix(horizon, zenith, h);
        let night_low = mix(vec3<f32>(0.02, 0.03, 0.07), vec3<f32>(0.025, 0.035, 0.08), sw.w);
        let sky_night = mix(night_low, vec3<f32>(0.005, 0.008, 0.02), h);
        // Without oxygen (and its ozone) the day sky is a hazy orange; lots of CO2 adds haze.
        let o2 = clamp(bg.air.x, 0.0, 1.5);
        let haze_day = mix(vec3<f32>(0.62, 0.42, 0.25), vec3<f32>(0.35, 0.34, 0.33), h);
        var day = mix(haze_day, sky_day, smoothstep(0.05, 0.6, o2));
        day = mix(day, vec3<f32>(0.5, 0.48, 0.42), clamp((bg.air.y - 1.5) * 0.25, 0.0, 0.5));
        var sky = mix(sky_night, day * 0.7, clamp(bg.sun.x * 1.3, 0.0, 1.0));

        // The sun and moon rise out of the sea at one point, arc across part of the sky and
        // set into the sea further along: the sun from 6:00 to 18:00 (higher in summer), the
        // moon through the night. They're in the world like everything else.
        let day_phase = bg.sun.z;
        let elevation = -cos(day_phase * TAU);
        let arc_height = bg.orbit.z * (0.85 + 0.15 * sin(year * TAU));
        let sun_u = fract(day_phase - 0.25 + 0.1) * 2.0 - 0.2;
        let sun_pos = vec2<f32>(
            bg.orbit.x + bg.orbit.y * (2.0 * sun_u - 1.0),
            size.y + sin(sun_u * 3.14159265) * arc_height,
        );
        // Dawn and dusk: the low sky glows orange-pink, most around the sun.
        let twilight = 1.0 - smoothstep(0.0, 0.3, abs(elevation + 0.03));
        let near_sun = exp(-abs(wp.x - sun_pos.x) / 3000.0);
        let dusk = twilight * (1.0 - h) * (1.0 - h) * (0.35 + 0.65 * near_sun);
        sky = mix(sky, vec3<f32>(0.95, 0.45, 0.28) * 0.75, clamp(dusk, 0.0, 1.0) * 0.8);

        // Stars at night (hidden by impact dust).
        let winter = bg.effects.x;
        // Fixed in the world (they don't reshuffle on zoom), about a pixel across at any zoom.
        let star_cell = floor(wp / 60.0);
        let twinkle = 0.4 + 0.6 * cell_hash(star_cell, 91u);
        var star = specks(wp, 60.0, vec2<f32>(0.0), 0.12, 0.9 * px, px, 0.9) * twinkle
            * (1.0 - clamp(bg.sun.x * 3.0, 0.0, 1.0));

        if sun_u > -0.2 && sun_u < 1.2 {
            let r = 80.0;
            let d = length(wp - sun_pos);
            let low = 1.0 - smoothstep(0.0, 0.45, elevation);
            let disc_col = mix(vec3<f32>(2.2, 2.0, 1.6), vec3<f32>(2.2, 0.9, 0.4), low);
            let disc = 1.0 - smoothstep(r - px, r + px, d);
            sky += disc_col * 0.18 * exp(-d / (r * 2.2)) + vec3<f32>(1.0, 0.8, 0.5) * 0.06 * exp(-d / (r * 9.0));
            sky = mix(sky, disc_col, disc);
        }
        let moon_u = fract(day_phase - 0.75 + 0.1) * 2.0 - 0.2;
        if moon_u > -0.2 && moon_u < 1.2 {
            let r = 60.0;
            let moon_pos = vec2<f32>(
                bg.orbit.x + bg.orbit.y * (2.0 * moon_u - 1.0),
                size.y + sin(moon_u * 3.14159265) * arc_height,
            );
            let q = wp - moon_pos;
            let d = length(q);
            let disc = 1.0 - smoothstep(r - px, r + px, d);
            // Phase: a shadow disc slides across (new moon at 0.5, full at 0 and 1).
            let shadow_pos = vec2<f32>((bg.sky.z - 0.5) * 4.0 * r, 0.0);
            let shadow = 1.0 - smoothstep(r - px, r + px, length(q - shadow_pos));
            let craters = 0.82 + 0.18 * value_noise(q / r * 3.0 + vec2<f32>(5.0, 1.0));
            let face = vec3<f32>(1.15, 1.2, 1.3) * craters;
            let lit = disc * (1.0 - shadow);
            let fullness = abs(bg.sky.z - 0.5) * 2.0;
            sky += vec3<f32>(0.25, 0.3, 0.4) * 0.12 * fullness * exp(-d / (r * 2.5));
            sky = mix(sky, sky * 0.6 + vec3<f32>(0.02, 0.025, 0.035), disc * shadow);
            sky = mix(sky, face, lit);
            star *= 1.0 - disc;
        }
        sky += vec3<f32>(star) * (1.0 - winter);

        // Winter: snow drifting down (only in winter, however much ice there is).
        let snowfall = bg.sky.w;
        if snowfall > 0.0 {
            // Sized and spaced on screen (it's weather close to the viewer), in two layers.
            let near_cell = 34.0 * px;
            let flurry = vec2<f32>(sin(time * 0.5 + wp.y / (60.0 * px)) * 6.0 * px, time * 45.0 * px);
            var snow = specks(wp, near_cell, flurry, snowfall * 0.3, 1.6 * px, px, 5.5);
            let far_flurry = vec2<f32>(sin(time * 0.3 + 2.0) * 4.0 * px, time * 22.0 * px);
            snow += 0.6 * specks(wp, 21.0 * px, far_flurry, snowfall * 0.25, 1.0 * px, px, 6.5);
            sky += vec3<f32>(0.8, 0.83, 0.88) * snow * (0.3 + 0.7 * clamp(bg.sun.x * 2.0, 0.0, 1.0));
        }

        // Impact winter: a churning brown-grey dust pall.
        let dust = fbm(vec2<f32>(wp.x * 0.0008 + time * 0.02, wp.y * 0.002));
        let pall = mix(vec3<f32>(0.09, 0.075, 0.06), vec3<f32>(0.2, 0.16, 0.12), dust);
        sky = mix(sky, pall, clamp(winter * 1.4, 0.0, 0.95));

        // The tops of the ice floes, standing a little out of the water.
        let floe = ice_at(wp.x, ice, time, px);
        if floe.x > 0.0 {
            let y = wp.y - size.y;
            let top = 0.15 * floe.y;
            let inside = (1.0 - smoothstep(top - px, top + px, y)) * floe.x;
            sky = mix(sky, ice_color(wp, y, floe.y, bg.sun.x), inside);
        }
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

    // Caves (painted maps): the back wall of a hollow in the rock, the same stone as the
    // terrain but far darker, and darker still in the shadow of the rock around it.
    let cave = textureSample(terrain_tex, terrain_sampler, uv).g;
    if cave > 0.01 && !outside {
        let cn = fbm(wp * 0.006) * 0.55 + fbm(wp * 0.03) * 0.3 + value_noise(wp * 0.15) * 0.15;
        var wall = mix(vec3<f32>(0.03, 0.028, 0.03), vec3<f32>(0.11, 0.095, 0.08), cn);
        wall *= 0.92 + 0.08 * sin(wp.y * 0.018 + fbm(wp * 0.004) * 9.0);
        // Distance into the cave from its rock walls: deep shadow near them (an overhang's
        // shade), a little lighter toward the middle; most shadow under ceilings.
        let to_rock = sdf_at(uv);
        let open = smoothstep(0.0, 90.0, to_rock);
        // Under a ceiling the rock overhead casts the deepest shade.
        let above = sdf_at(uv + vec2<f32>(0.0, -70.0 / size.y));
        let ceiling = 1.0 - smoothstep(-20.0, 60.0, above);
        wall *= (0.3 + 0.25 * open) * (1.0 - 0.45 * ceiling);
        // Faint, wavering streaks of sediment trickling down the back wall.
        let wave = fbm(vec2<f32>(wp.y * 0.01, wp.x * 0.002)) * 40.0;
        let streak = smoothstep(0.65, 0.9, value_noise(vec2<f32>((wp.x + wave) * 0.03, wp.y * 0.003)));
        wall = mix(wall, vec3<f32>(0.05, 0.038, 0.024), streak * 0.2);
        wall *= mix(1.0, 0.7, clamp(depth / size.y, 0.0, 1.0));
        water = mix(water, wall + water * 0.15, smoothstep(0.0, 1.0, cave));
    }

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

    // Vents: hydrothermal chimneys ("black smokers") of the same rock as the seafloor, with
    // a steady plume of smoke rising out and a hot glow at each mouth. Each vent picks one
    // of four layouts from its position: a tall chimney, a twin, a chimney with
    // three spires, or a squat cone. The outlets are only looks; the vent works the same either way. Stacks
    // reach down to the first rock below the vent and a little into it (`root`), no further.
    let vent_count = u32(bg.sun.w);
    var vent_glow = vec3<f32>(0.0);
    for (var i = 0u; i < min(vent_count, MAX_VENTS); i++) {
        let v = bg.vents[i];
        let strength = v.z;
        let root = v.w;
        let q0 = wp - v.xy;
        if abs(q0.x) > 700.0 || q0.y < -root - 20.0 || q0.y > 900.0 {
            continue;
        }
        let seed = hash21(floor(v.xy));
        let variant = u32(cell_hash(floor(v.xy / 7.0), 4242u) * 4.0);
        let aa = max(px, 0.4);
        let flicker = 0.8 + 0.2 * value_noise(vec2<f32>(time * 3.0, seed * 40.0));
        let outlets = vent_outlets(variant);

        // A warm halo in the water.
        let halo = vec3<f32>(0.06, 0.02, 0.007) * exp(-length(q0 - vec2<f32>(0.0, 26.0)) / 90.0) * strength;
        water += halo;
        vent_glow += halo;

        // Smoke from every outlet, merged into one plume.
        let current = (fract(seed * 9.13) - 0.5) * 2.0;
        let roof = bg.vent_ceilings[i];
        let ceiling = roof.x;
        var density = 0.0;
        var height = 0.0;
        var across = 0.0;
        var lit = 0.0;
        for (var k = 0u; k < 3u; k++) {
            let o = outlets[k];
            if o.z <= 0.0 {
                continue;
            }
            let mouth = vec2<f32>(o.x, o.y);
            let plume = smoke_plume(q0 - mouth, o.z, strength, current,
                vec3<f32>(roof.x - o.y, roof.y + o.x, roof.z - o.x), time, seed + f32(k) * 0.37);
            density += plume.density;
            height += plume.height * plume.density;
            across += abs(plume.across) * plume.density;
            lit += plume.lit * plume.density;
        }
        if density > 1e-3 {
            height /= density;
            across /= density;
            lit /= density;
            let cover = 1.0 - exp(-2.2 * density);
            // Sulfide-black where it's thick and fresh; thin edges and older smoke pale to a
            // brownish grey, lit a little from above and tinted by the water it's mixing into.
            let thin = smoothstep(0.2, 1.0, across) * 0.6 + smoothstep(0.3, 1.6, height) * 0.4;
            var smoke = mix(vec3<f32>(0.012, 0.010, 0.009), vec3<f32>(0.075, 0.068, 0.062), thin);
            smoke += vec3<f32>(0.10, 0.11, 0.11) * (0.15 + lin) * thin * 0.35;
            // Billows: tops catch the light, undersides sit in their own shadow.
            smoke *= 1.0 + 0.45 * lit;
            smoke = mix(smoke, water * 0.8 + vec3<f32>(0.012, 0.012, 0.01), smoothstep(0.6, 2.0, height) * 0.5);
            // Glowing faintly from the hot mouth below.
            let low = exp(-max(q0.y - 30.0, 0.0) / 22.0);
            smoke += vec3<f32>(0.35, 0.12, 0.03) * low * strength * flicker * 0.5;
            water = mix(water, smoke, cover * (0.97 - 0.4 * smoothstep(0.8, 2.2, height)));
        }

        // Bubbles streaming up from each mouth, faster than the smoke.
        if px < 4.0 {
            for (var k = 0u; k < 3u; k++) {
                let o = outlets[k];
                if o.z <= 0.0 {
                    continue;
                }
                let mouth = vec2<f32>(o.x, o.y);
                for (var b = 0; b < 6; b++) {
                    let fb = f32(b);
                    let sk = seed * 7.0 + f32(k) * 3.1 + fb * 1.37;
                    let age = fract(time / 4.0 + fb / 6.0 + fract(sk));
                    let cyc = floor(time / 4.0 + fb / 6.0 + fract(sk));
                    let jb = hash21(vec2<f32>(fb + cyc * 7.0, sk));
                    let c = mouth + vec2<f32>(
                        (jb - 0.5) * 10.0 * o.z + sin(age * 18.0 + jb * 6.28) * 3.0,
                        age * (150.0 + 80.0 * jb),
                    );
                    let br = (1.2 + 1.6 * jb) * (0.6 + 0.4 * o.z);
                    let d = length(wp - v.xy - c);
                    // Popping out of sight as they reach rock overhead.
                    let fade = (1.0 - smoothstep(0.7, 1.0, age)) * smoothstep(0.0, 0.05, age) * strength
                        * (1.0 - smoothstep(ceiling - 12.0, ceiling - 4.0, c.y));
                    let body = 1.0 - smoothstep(br - aa, br + aa, d);
                    let rim = smoothstep(br * 0.55 - aa, br * 0.55 + aa, d) * body;
                    let glint = 1.0 - smoothstep(br * 0.25, br * 0.25 + aa, length(wp - v.xy - c - vec2<f32>(-br * 0.35, br * 0.35)));
                    water = mix(water, vec3<f32>(0.35, 0.5, 0.55), rim * fade * 0.7);
                    water += vec3<f32>(0.4, 0.5, 0.5) * glint * body * fade * 0.6;
                }
            }
        }

        // The ground's own rock (same as the terrain below), for the stacks to be made of.
        let rn = fbm(wp * 0.006) * 0.55 + fbm(wp * 0.03) * 0.3 + value_noise(wp * 0.15) * 0.15;
        var stone = mix(vec3<f32>(0.03, 0.028, 0.03), vec3<f32>(0.11, 0.095, 0.08), rn);
        stone *= 0.92 + 0.08 * sin(wp.y * 0.018 + fbm(wp * 0.004) * 9.0);
        stone *= mix(1.0, 0.5, clamp(depth / size.y, 0.0, 1.0));

        // Clustered outlets: one chimney with three spires side by side. The middle spire is
        // the tallest, the right the lowest; a shallow notch splits the left and middle, a deeper
        // one the middle and right. The left side slopes gently down to a wide base, the right
        // drops almost straight.
        if variant == 2u {
            let y = q0.y;
            let wob = (value_noise(vec2<f32>(y * 0.09, seed * 60.0)) - 0.5) * 3.0;
            // Sides: straight lines from the spires' outer edges down to the base (and on into
            // the rock).
            let left = mix(-27.0, -41.0, clamp((46.0 - y) / 91.0, 0.0, 1.6)) + wob;
            let right = mix(26.0, 35.0, clamp((30.0 - y) / 80.0, 0.0, 1.6)) - wob;
            // The top: each spire a column with a rounded cap, joined by the notches' floors
            // (only between the outer spires: outside them the sloping sides rise straight to
            // the outer spires' caps).
            let lo = outlets[0];
            let hi = outlets[2];
            var top = -1e3;
            if q0.x > lo.x && q0.x < hi.x {
                let past = smoothstep(-3.0, 3.0, q0.x - 11.0);
                top = mix(22.0, -2.0, past);
            }
            for (var k = 0u; k < 3u; k++) {
                let o = outlets[k];
                let hw = 7.5 * select(1.0, 1.1, k == 1u);
                let u = (q0.x - o.x) / hw;
                // Steep walls on the sides that face another spire; the outer sides round off
                // into the sloping flanks.
                let outer = (k == 0u && u < 0.0) || (k == 2u && u > 0.0);
                let wall = select(120.0 * max(abs(u) - 1.0, 0.0), 0.0, outer);
                top = max(top, o.y - 4.0 * u * u * u * u - wall);
            }
            top += (value_noise(vec2<f32>(q0.x * 0.15, seed * 70.0)) - 0.5) * 2.0;
            // The base ends a little below the vent (buried in its floor), so it never shows
            // through open water or caves underneath.
            let d = max(max(max(left - q0.x, q0.x - right), y - top), -root - y);
            let fill = 1.0 - smoothstep(-aa, aa, d);
            var heat = 0.0;
            for (var k = 0u; k < 3u; k++) {
                let o = outlets[k];
                heat += exp(-length(q0 - vec2<f32>(o.x, o.y)) / (9.0 * o.z));
            }
            if fill > 0.0 {
                let band = 0.5 + 0.5 * sin(q0.y * 0.3 + value_noise(vec2<f32>(q0.x * 0.2, q0.y * 0.1)) * 4.0);
                let dust = vec3<f32>(0.15, 0.115, 0.07) * mix(1.0, 0.5, clamp(depth / size.y, 0.0, 1.0));
                var col = mix(stone, dust, 0.35 + 0.2 * value_noise(wp * 0.05 + seed * 3.0)) * (0.9 + 0.2 * band);
                col = mix(col, vec3<f32>(0.09, 0.045, 0.02) * (0.6 + 0.4 * rn), smoothstep(0.72, 0.92, value_noise(wp * 0.08 + seed * 13.0)) * 0.25);
                let hot = min(heat, 1.0) * strength;
                let cracks = smoothstep(0.8, 0.95, value_noise(vec2<f32>(q0.x * 0.5, q0.y * 0.18 - time * 0.2) + seed * 7.0));
                col += vec3<f32>(1.4, 0.4, 0.07) * cracks * hot * flicker + vec3<f32>(0.12, 0.035, 0.01) * hot;
                // Rounded: darker toward the edges, with a rim catching the light.
                let inner = clamp(-d / 7.0, 0.0, 1.0);
                col *= 0.78 + 0.22 * inner;
                col += vec3<f32>(0.22, 0.28, 0.27) * (1.0 - smoothstep(0.0, 0.3, inner)) * (0.04 + lin * 0.6);
                water = mix(water, col, fill);
            }
        }

        // The stacks: tapering up to their mouths, lumpy, leaning a little.
        for (var k = 0u; k < 3u; k++) {
            let o = outlets[k];
            if o.z <= 0.0 {
                continue;
            }
            let mouth = vec2<f32>(o.x, o.y);
            let q = q0 - mouth;
            let below = max(-q.y, 0.0);
            let sk = seed + f32(k) * 0.37;
            let lean = sin(below * 0.02 + sk * 6.28) * 2.5 + below * (fract(sk * 7.3) - 0.5) * 0.06;
            let lumps = 1.0 + 0.3 * (value_noise(vec2<f32>(below * 0.09, sk * 50.0)) - 0.5)
                + 0.15 * (value_noise(vec2<f32>(below * 0.3, sk * 90.0)) - 0.5);
            // Squat cones flare quickly; chimneys taper over a long stack.
            let flare = select(160.0, 45.0, variant == 3u);
            let half_w = (5.0 + 14.0 * pow(min(below / flare, 1.0), 0.8)) * o.z * lumps;
            let side = abs(q.x - lean);
            // (The cluster's stacks are drawn above, as one branching chimney.)
            let stack = (1.0 - smoothstep(half_w - aa, half_w + aa, side)) * (1.0 - smoothstep(-aa, aa, q.y))
                * smoothstep(-root - aa, -root + aa, q0.y)
                * select(1.0, 0.0, variant == 2u);
            if stack > 0.0 {
                // Seafloor rock, darker and banded with sulfides, rusty in patches.
                let band = 0.5 + 0.5 * sin(below * 0.3 + value_noise(vec2<f32>(q.x * 0.2, below * 0.1)) * 4.0);
                // A dusting of the floor's sediment, as on the terrain, so it sits in the ground.
                let dust = vec3<f32>(0.15, 0.115, 0.07) * mix(1.0, 0.5, clamp(depth / size.y, 0.0, 1.0));
                var col = mix(stone, dust, 0.35 + 0.2 * value_noise(wp * 0.05 + sk * 3.0)) * (0.9 + 0.2 * band);
                col = mix(col, vec3<f32>(0.09, 0.045, 0.02) * (0.6 + 0.4 * rn), smoothstep(0.72, 0.92, value_noise(wp * 0.08 + sk * 13.0)) * 0.25);
                // Heat and glowing cracks near the mouth.
                let heat = exp(-below / (10.0 * o.z)) * strength;
                let cracks = smoothstep(0.8, 0.95, value_noise(vec2<f32>(q.x * 0.5, below * 0.18 - time * 0.2) + sk * 7.0));
                col += vec3<f32>(1.4, 0.4, 0.07) * cracks * heat * flicker + vec3<f32>(0.12, 0.035, 0.01) * heat;
                // Rounded: darker toward the edges, a rim catching the light from above (as
                // the terrain's edges do).
                let across = clamp(side / max(half_w, 1e-3), 0.0, 1.0);
                col *= 0.78 + 0.22 * (1.0 - across * across);
                col += vec3<f32>(0.22, 0.28, 0.27) * smoothstep(0.8, 1.0, across) * (0.04 + lin * 0.6);
                water = mix(water, col, stack);
            }
            // The mouth: a small, bright, flickering glow.
            let core_r = (3.0 + 2.0 * strength) * o.z;
            let glow = vec3<f32>(2.4, 0.85, 0.2) * exp(-dot(q, q) / (2.0 * core_r * core_r)) * flicker * strength;
            water += glow;
            vent_glow += glow;
        }
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
            // Upwelling: a column of cold, mineral-rich water with streaks rising to the surface
            // (in open water: hidden behind cave walls).
            let across = 1.0 - smoothstep(half_w * 0.6, half_w, abs(dx));
            let strength = e.w * across * smoothstep(1800.0, 300.0, depth) * (1.0 - smoothstep(0.0, 1.0, cave));
            let streak = pow(0.5 + 0.5 * sin(dx * 0.03 + fbm(vec2<f32>(dx * 0.004, wp.y * 0.003 - time * 0.4)) * 6.0), 6.0);
            // Round bubbles rising and wobbling, in two sizes (bigger ones rise faster), fixed in
            // the water so they don't reshuffle on zoom; only drawn close enough to see.
            var bubble = 0.0;
            let near_up = 1.0 - smoothstep(1.5, 5.0, px);
            if near_up > 0.0 {
                let wob = vec2<f32>(sin(wp.y * 0.04 + time * 1.3 + dx * 0.01) * 4.0, 0.0);
                let big = specks(wp + wob, 26.0, vec2<f32>(0.0, -time * 34.0), 0.18, 2.4, px, 41.7);
                let big_rim = specks(wp + wob, 26.0, vec2<f32>(0.0, -time * 34.0), 0.18, 1.5, px, 41.7);
                let small = specks(wp + wob * 0.6, 13.0, vec2<f32>(0.0, -time * 22.0), 0.15, 1.2, px, 57.3);
                bubble = ((big - big_rim * 0.6) + small * 0.8) * near_up;
            }
            water += (vec3<f32>(0.02, 0.12, 0.16) + vec3<f32>(0.05, 0.2, 0.22) * streak + vec3<f32>(0.45, 0.65, 0.68) * bubble) * strength;
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

    // Ice floes on the surface (winter): most of each floe sits below the waterline.
    let floe = ice_at(wp.x, ice, time, px);
    if floe.x > 0.0 && !outside {
        let below = 0.85 * floe.y;
        let inside = (1.0 - smoothstep(below - px, below + px, depth)) * floe.x;
        water = mix(water, ice_color(wp, -depth, floe.y, bg.sun.x), inside);
    }

    let edge = smoothstep(-px, px, sdf);
    var col = mix(rock, water, edge);
    if outside {
        col = mix(rock * 0.35, vec3<f32>(0.01, 0.012, 0.02), 0.6);
    }
    // Never negative: bloom smears negative values into blocky artifacts.
    return vec4<f32>(max(col, vec3<f32>(0.0)), 1.0);
}
