// Procedural organism rendering. Each body, body part, carcass or effect is one quad, drawn in
// the fragment shader. Bodies are drawn in "radius units" (1.0 = a round body's edge) in the
// organism's own frame (x = forward); parts in "length units" along the part (x = outward).

#import bevy_sprite::mesh2d_functions as mesh_functions

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: vec4<f32>; // time, world units per px

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    // body: quad half extents x, y (radius units), radius in px, seed
    // part: quad half extents x, y (length units), nominal half width, length in px
    // fx:   quad half / radius, radius in px, kind, age
    @location(2) shape: vec4<f32>,
    @location(3) tint: vec4<f32>,   // linear rgb
    @location(4) body: vec4<f32>,   // body: elongation, segments/6, jaws, pattern; part: kind|toward<<4, size, mirror|index, seed
    @location(5) diet: vec4<f32>,   // photo, chemo, scavenge, hunt (carcass: consumed)
    @location(6) state: vec4<f32>,  // energy, health, flags, maturity
    @location(7) motion: vec4<f32>, // moving, walking, eating flash, hurt flash
    @location(8) chem: vec4<f32>,   // oxygenic, toxin, fixation, lure
    @location(9) wound: vec4<f32>,  // direction of the last bite (body-relative angle / 2π); part: how far toward
};

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) shape: vec4<f32>,
    @location(2) tint: vec4<f32>,
    @location(3) body: vec4<f32>,
    @location(4) diet: vec4<f32>,
    @location(5) state: vec4<f32>,
    @location(6) motion: vec4<f32>,
    @location(7) chem: vec4<f32>,
    @location(8) wound: vec4<f32>,
};

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    let world = mesh_functions::mesh2d_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));
    out.clip = mesh_functions::mesh2d_position_world_to_clip(world);
    out.uv = v.uv;
    out.shape = v.shape;
    out.tint = v.tint;
    out.body = v.body;
    out.diet = v.diet;
    out.state = v.state;
    out.motion = v.motion;
    out.chem = v.chem;
    out.wound = v.wound;
    return out;
}

const PI: f32 = 3.14159265;
const TAU: f32 = 6.2831853;

fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

fn rotate(p: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(c * p.x - s * p.y, s * p.x + c * p.y);
}

// Soft cellular dots: ~1 inside dots of radius `size` on a jittered grid with spacing 1.
fn dots(p: vec2<f32>, size: f32, seed: f32) -> f32 {
    let cell = floor(p);
    var best = 10.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let c = cell + vec2<f32>(f32(i), f32(j));
            let jitter = vec2<f32>(hash21(c + seed), hash21(c + seed + 17.0));
            best = min(best, length(p - c - jitter));
        }
    }
    return 1.0 - smoothstep(size * 0.6, size, best);
}

// A soft round dot of radius r at c.
fn dot_at(p: vec2<f32>, c: vec2<f32>, r: f32, aa: f32) -> f32 {
    return 1.0 - smoothstep(r - aa, r + aa, length(p - c));
}

const FLAG_SELECTED: u32 = 1u;
const FLAG_HIGHLIGHT: u32 = 2u;
const FLAG_DEAD: u32 = 4u;
const FLAG_MOTILE: u32 = 8u;
const FLAG_FX: u32 = 16u;
const FLAG_SICK: u32 = 32u;
const FLAG_PART: u32 = 64u;
const FLAG_DIVIDING: u32 = 128u;

fn fill(dist: f32, w: f32, aa: f32) -> f32 {
    return 1.0 - smoothstep(w - aa, w + aa, dist);
}

// Effects: gulp (1), bite (2), scavenging crumbs (3), impact shockwave (4).
fn effect(in: VertexOutput) -> vec4<f32> {
    let kind = u32(in.shape.z + 0.5);
    let age = in.shape.w;
    let p = in.uv * in.shape.x; // in units of the effect's radius
    let d = length(p);
    let aa = 1.5 / max(in.shape.y, 0.3);
    let seed = in.body.x * 50.0;
    let fade = 1.0 - age;
    var col = vec3<f32>(0.0);
    var a = 0.0;
    if kind == 1u {
        // Gulp: the prey collapses inward while a bright ring snaps shut.
        let r = 1.0 - age;
        let body = dot_at(p, vec2<f32>(0.0), max(r * 0.9, 0.02), aa);
        let ring = 1.0 - smoothstep(0.0, 0.08 + aa, abs(d - (0.3 + 1.2 * (1.0 - age * age))));
        col = in.tint.rgb * 1.2 * body + vec3<f32>(1.6, 1.4, 0.9) * ring;
        a = max(body * 0.9, ring * fade);
    } else if kind == 2u {
        // Bite: red droplets burst outward, plus a bright chomp arc.
        var drops = 0.0;
        for (var k = 0; k < 7; k++) {
            let ang = seed + f32(k) * 0.9 + hash21(vec2<f32>(seed, f32(k))) * 0.6;
            let dist = 0.2 + age * (0.9 + hash21(vec2<f32>(f32(k), seed)) * 0.9);
            let c = vec2<f32>(cos(ang), sin(ang)) * dist;
            drops = max(drops, dot_at(p, c, 0.16 * fade + aa * 0.5, aa));
        }
        let arc = (1.0 - smoothstep(0.0, 0.07 + aa, abs(d - 0.55 - age * 0.3))) * fade;
        let blood = mix(in.tint.rgb, vec3<f32>(0.9, 0.05, 0.03), 0.7) * 2.2;
        col = blood * drops + vec3<f32>(2.0, 1.4, 1.2) * arc;
        a = max(drops, arc) * fade;
    } else if kind == 3u {
        // Scavenging: crumbs of the carcass scatter and drift down.
        var crumbs = 0.0;
        for (var k = 0; k < 6; k++) {
            let ang = seed + f32(k) * 1.1;
            let c = vec2<f32>(cos(ang), sin(ang)) * (0.2 + age * 0.8) - vec2<f32>(0.0, age * age * 0.6);
            crumbs = max(crumbs, dot_at(p, c, 0.12 * fade + aa * 0.5, aa));
        }
        let grey = dot(in.tint.rgb, vec3<f32>(0.3, 0.5, 0.2));
        col = mix(vec3<f32>(grey), vec3<f32>(0.35, 0.22, 0.1), 0.6) * 1.6;
        a = crumbs * fade;
    } else {
        // Impact: blinding flash, then an expanding shockwave ring.
        let flash = exp(-age * 18.0) * (1.0 - smoothstep(0.0, 1.0, d));
        let ring_r = sqrt(age) * 1.3;
        let ring = (1.0 - smoothstep(0.0, 0.05 + aa, abs(d - ring_r))) * fade;
        let steam = (1.0 - smoothstep(0.0, ring_r, d)) * 0.25 * fade;
        col = vec3<f32>(4.0, 3.0, 1.8) * flash + vec3<f32>(2.2, 1.6, 1.0) * ring + vec3<f32>(0.6, 0.6, 0.6) * steam;
        a = clamp(flash + ring + steam, 0.0, 1.0);
    }
    return vec4<f32>(col, a);
}

// Distance from p to segment a-b, and the position along it (0..1).
fn seg(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return vec2<f32>(length(pa - ba * h), h);
}

// One eye, in eye-radius units (x = the direction it looks). Complexity grows continuously
// with its size: eyespot (pigment dot) -> pit eye (dark cup) -> lens eye (iris and pupil) ->
// camera eye (striated iris, glint, rim).
fn eye_art(e: vec2<f32>, level: f32, seed: f32, time: f32, aa_e: f32) -> vec4<f32> {
    let de = length(e);
    let a = 1.0 - smoothstep(1.0 - aa_e, 1.0 + aa_e, de);
    if a <= 0.0 {
        return vec4<f32>(0.0);
    }
    let blink = step(0.975, fract(time * 0.23 + seed * 7.13));
    let iris_col = mix(vec3<f32>(1.0, 0.62, 0.08), vec3<f32>(0.15, 0.55, 1.0), fract(seed * 3.7));
    let l2 = smoothstep(0.2, 0.3, level); // pit eye
    let l3 = smoothstep(0.45, 0.55, level); // lens eye
    let l4 = smoothstep(0.72, 0.82, level); // camera eye
    // 1: eyespot.
    var col = vec3<f32>(0.55, 0.05, 0.03);
    // 2: pit eye.
    let pit = mix(vec3<f32>(0.01, 0.01, 0.02), vec3<f32>(0.95, 0.9, 0.8), smoothstep(0.5, 0.8, de));
    col = mix(col, pit, l2);
    // 3: lens eye, pupil looking outward.
    let look = e - vec2<f32>(0.2, 0.0);
    let iris = 1.0 - smoothstep(0.56, 0.6 + aa_e, length(look));
    let pupil = 1.0 - smoothstep(0.24, 0.28 + aa_e, length(look));
    var lens = mix(vec3<f32>(0.92, 0.92, 0.88), iris_col, iris);
    lens = mix(lens, vec3<f32>(0.01), pupil);
    col = mix(col, lens, l3);
    // 4: camera eye.
    let stria = 0.7 + 0.3 * sin(atan2(look.y, look.x) * 16.0);
    var camera = mix(vec3<f32>(0.96), iris_col * stria, iris);
    camera = mix(camera, vec3<f32>(0.0), pupil);
    camera = mix(camera, vec3<f32>(0.06, 0.03, 0.03), smoothstep(0.8, 0.92, de));
    let glint = 1.0 - smoothstep(0.1, 0.17, length(e - vec2<f32>(-0.2, 0.3)));
    camera += vec3<f32>(2.5) * glint;
    col = mix(col, camera, l4);
    // Blink (lens and camera eyes only).
    if blink > 0.5 {
        let closed = mix(vec3<f32>(0.3, 0.25, 0.2), vec3<f32>(0.02), 1.0 - smoothstep(0.05, 0.1, abs(e.y)));
        col = mix(col, closed, l3);
    }
    return vec4<f32>(col, a);
}

// What a part's sway depends on.
struct Sway {
    s: f32,
    time: f32,
    phase: f32,
    activity: f32,
    grip: f32,
    arch: f32,
    gape: f32,
};

// Where a kind's midline sits across the part at `a` along it (in part lengths): the curve its
// look is drawn around. (`midline` in render/mod.rs repeats the swaying ones; keep them in step.)
fn kind_sway(kind: u32, a: f32, v: Sway) -> f32 {
    if kind == 1u {
        let amp = (0.08 + 0.08 * v.s) * (0.4 + 0.6 * min(1.0, v.activity * 2.0 + 0.3));
        return amp * sin(a * TAU * (1.0 + v.s) - v.time * (6.0 + 16.0 * v.activity) + v.phase) * a;
    } else if kind == 3u {
        let sway = 0.16 * sin(a * 3.0 - v.time * 1.6 + v.phase) * a;
        let curl = (0.18 + 0.12 * sin(v.time * 0.45 + v.phase)) * a * a * a;
        let hook = -v.arch * 0.6 * smoothstep(0.75, 1.0, a);
        let held = v.arch * sin(PI * clamp(a, 0.0, 1.0)) + hook + 0.03 * sin(v.time * 5.0 + v.phase) * a;
        return mix(sway + curl, held, v.grip);
    } else if kind == 4u {
        return 0.07 * sin(v.time * 0.8 + v.phase + a * 2.0) * a * a;
    } else if kind == 5u {
        return 0.25 * sin(v.time * (2.5 + 7.0 * v.activity) + v.phase) * a;
    } else if kind == 6u {
        return 0.25 * sin(v.time * (2.0 + 6.0 * v.activity) + v.phase) * a;
    } else if kind == 9u {
        return 0.06 * sin(v.time * 0.7 + v.phase) * a * a;
    } else if kind == 11u {
        return (0.12 * sin(v.time * 2.2 + v.phase) + 0.18) * a * a;
    } else if kind == 12u {
        return (0.15 * sin(v.time * 0.9 + v.phase) + 0.1) * a * a;
    } else if kind == 13u {
        return 0.08 * smoothstep(0.5, 1.0, v.s) * a * a;
    } else if kind == 14u {
        return (0.12 + v.gape) * sin(PI * a) - (0.18 + 0.12 * v.s) * a * a * a;
    }
    return 0.0;
}

// Roughly how far a kind's look reaches either side of its midline at `a` along it (in part
// lengths; w = the part's nominal half width, s = its size): its outline, which some of the
// in-between art morphs from and to.
fn kind_width(kind: u32, a: f32, w: f32, s: f32) -> f32 {
    let x = clamp(a, 0.0, 1.0);
    if kind == 1u {
        return w * (1.0 - 0.7 * x);
    } else if kind == 3u {
        return w * (1.0 - 0.65 * x);
    } else if kind == 4u {
        return max(w * pow(sin(PI * clamp((x - 0.08) / 0.92, 0.0, 1.0)), 0.7), w * 0.14 * step(x, 0.2));
    } else if kind == 5u {
        // A fan: wider out to its rounded rim.
        return min(x * w * 1.6, sqrt(max(0.0, 1.0 - x * x)));
    } else if kind == 6u {
        let blade = w * sqrt(max(0.0, 1.0 - pow((x - 0.55) / 0.45, 2.0)));
        return max(w * 0.22 * (1.0 - x * 0.5) * step(x, 0.5), blade);
    } else if kind == 8u {
        return w * mix(0.28, 0.95, smoothstep(0.4, 1.0, x));
    } else if kind == 9u {
        return w * (1.0 - 0.3 * x);
    } else if kind == 10u {
        return w * sqrt(max(0.0, 1.0 - pow(2.0 * x - 1.0, 2.0)));
    } else if kind == 11u {
        return w * (1.0 - 0.6 * x);
    } else if kind == 12u {
        let br = w * (0.55 + 0.45 * s);
        let bulb = sqrt(max(0.0, br * br - (x - 0.8) * (x - 0.8)));
        return max(w * 0.22 * step(x, 0.78), bulb);
    } else if kind == 13u {
        return w * (1.0 - x);
    } else if kind == 14u {
        return w * (1.0 - pow(x, 1.4) * 0.85);
    }
    // Cilia bands, legs and shell plates: about their nominal width all along.
    return w;
}

// One kind's look for a body part: colour and coverage. Every kind has its own look, and grows
// more elaborate with its size.
fn part_look(in: VertexOutput, kind: u32) -> vec4<f32> {
    let flags = u32(in.state.z * 255.0 + 0.5);
    let dead = (flags & FLAG_DEAD) != 0u;
    let s = in.body.y;
    let mi = u32(in.body.z * 255.0 + 0.5);
    let mirror = select(1.0, -1.0, (mi & 1u) == 1u);
    let index = f32(mi >> 1u);
    let seed = in.body.w;
    let lp = in.uv * in.shape.xy;
    let a = lp.x + 0.5;               // 0 at the base, 1 at the tip
    let w = in.shape.z;               // nominal half width
    let len_px = in.shape.w;
    let aa = 1.0 / max(len_px, 0.5); // about one pixel
    let time = select(params.x, 0.0, dead);
    let activity = select(in.motion.x, 0.0, dead);
    let walking = select(in.motion.y, 0.0, dead);
    let eating = in.motion.z;
    let phase = seed * 40.0 + index * 1.9;
    let gape = eating * abs(sin(time * 14.0 + index)) * 0.2;
    let sway = Sway(s, time, phase, activity, in.chem.z, (in.chem.w * 2.0 - 1.0) * 0.75, gape);
    let c = lp.y * mirror;            // across (mirrored copies curl the other way)
    let q = vec2<f32>(a, c);
    let base = in.tint.rgb;
    let inside = step(0.0, a) * step(a, 1.0);
    // Fine detail only once there are enough pixels to show it.
    let detail = smoothstep(6.0, 16.0, len_px);
    var col = base;
    var alpha = 0.0;

    if kind == 1u {
        // Flagellum: a beating whip; bigger ones get a basal body, then a fringe of fine hairs.
        let y = kind_sway(kind, a, sway);
        let hw = w * (1.0 - 0.7 * a) + aa * 0.4;
        let dist = abs(c - y);
        alpha = fill(dist, hw, aa) * inside;
        col = mix(base * 0.8 + vec3<f32>(0.06), base * 1.1 + vec3<f32>(0.12), a);
        let hairs = step(fract(a * 30.0 + phase), 0.22) * step(dist, hw * 3.5 + aa) * inside
            * step(0.12, a) * smoothstep(0.55, 0.75, s) * detail;
        col = mix(col, base * 1.3 + vec3<f32>(0.2), hairs * (1.0 - alpha));
        alpha = max(alpha, hairs * 0.55);
        let basal = fill(length(q), w * 1.8, aa) * smoothstep(0.25, 0.4, s);
        col = mix(col, base * 0.55, basal);
        alpha = max(alpha, basal);
    } else if kind == 2u {
        // Cilia band: rows of short hairs beating in waves; denser and longer when bigger.
        let spacing = mix(0.4, 0.13, s);
        let band = fill(abs(c), w, aa) * (1.0 - smoothstep(0.1, 0.16, a)) * step(-0.08, a);
        var hairs = 0.0;
        let i0 = round(c / spacing);
        for (var k = -2; k <= 2; k++) {
            let i = i0 + f32(k);
            let y0 = i * spacing;
            if abs(y0) > w {
                continue;
            }
            let tilt = 0.35 * sin(time * 12.0 * (0.4 + activity) - i * 0.7 + phase);
            hairs = max(hairs, fill(abs(c - y0 - tilt * a * a), 0.035 + 0.02 * s + aa * 0.3, aa) * inside);
        }
        col = mix(base * 0.7, base * 0.9 + vec3<f32>(0.3, 0.3, 0.25), hairs);
        alpha = max(band, hairs * 0.9);
    } else if kind == 3u {
        // Tentacle: a swaying, curling arm; ringed when larger, then suckers and a clubbed tip.
        // Holding something, it arches out to it (the quad already points at it) and its tip
        // hooks around it.
        let y = kind_sway(kind, a, sway);
        var hw = w * (1.0 - 0.65 * a);
        hw += w * 0.6 * smoothstep(0.78, 0.92, a) * (1.0 - smoothstep(0.94, 1.0, a)) * smoothstep(0.75, 0.95, s);
        let dist = abs(c - y);
        alpha = fill(dist, hw + aa * 0.3, aa) * step(0.0, a) * (1.0 - smoothstep(1.0 - aa, 1.0 + aa, a));
        col = base * mix(1.1, 0.7, dist / max(hw, 1e-3));
        let ring = 1.0 - smoothstep(0.08, 0.2, abs(fract(a * 14.0) - 0.5));
        col *= 1.0 - 0.3 * ring * smoothstep(0.3, 0.45, s) * detail;
        if s > 0.55 {
            let ai = (floor(a * 12.0) + 0.5) / 12.0;
            let sucker = fill(length(vec2<f32>(a - ai, c - (y - hw * 0.45))), hw * 0.32, aa) * step(0.1, a);
            col = mix(col, vec3<f32>(0.95, 0.8, 0.75), sucker * smoothstep(0.55, 0.7, s) * detail * alpha);
        }
    } else if kind == 4u {
        // Frond: a leafy blade (green, or purple for sulfur-based photosynthesis); veins, a
        // serrated edge and a gas bladder as it grows.
        let y = kind_sway(kind, a, sway);
        var hw = w * pow(sin(PI * clamp((a - 0.08) / 0.92, 0.0, 1.0)), 0.7);
        hw *= 1.0 - 0.15 * smoothstep(0.65, 0.85, s) * abs(sin(a * 45.0));
        hw = max(hw, w * 0.14 * step(a, 0.2));
        let dist = abs(c - y);
        alpha = fill(dist, hw + aa * 0.3, aa) * inside;
        // Leaves take the pigment's own colour (green releases oxygen, purple uses minerals),
        // vivid with strong photosynthesis and pale and faded with weak; the body colour is
        // only a faint hint, so complementary body colours don't turn them grey.
        // Mixed pigments pass through golden-brown rather than purple + green = grey.
        let purple = vec3<f32>(0.5, 0.12, 0.48);
        let golden = vec3<f32>(0.58, 0.44, 0.1);
        let green = vec3<f32>(0.14, 0.58, 0.1);
        let pigment = select(
            mix(golden, green, in.chem.x * 2.0 - 1.0),
            mix(purple, golden, in.chem.x * 2.0),
            in.chem.x < 0.5,
        );
        let faded = mix(pigment, vec3<f32>(0.8, 0.82, 0.72), 0.45);
        let leaf = mix(faded, pigment, smoothstep(0.0, 0.6, in.diet.x));
        col = mix(leaf, base, 0.08) * mix(1.15, 0.8, dist / max(hw, 1e-3));
        col = mix(col, col * 0.6, fill(dist, 0.015 + aa * 0.5, aa) * smoothstep(0.2, 0.35, s));
        let v = fract(a * 9.0 - dist / max(w, 1e-3) * 1.2);
        let vein = 1.0 - smoothstep(0.0, 0.08, min(v, 1.0 - v));
        col *= 1.0 - 0.25 * vein * smoothstep(0.35, 0.5, s) * detail;
        let bladder = fill(length(q - vec2<f32>(0.14, y)), w * 0.55, aa) * smoothstep(0.8, 0.92, s);
        col = mix(col, vec3<f32>(0.75, 0.65, 0.3), bladder);
        alpha = max(alpha, bladder);
    } else if kind == 5u {
        // Fin: a fan-shaped membrane that flaps; rays appear, then a scalloped edge between them.
        let fq = vec2<f32>(a, c - kind_sway(kind, a, sway));
        let spread = atan(w * 1.6);
        let phi = atan2(fq.y, max(fq.x, 1e-4));
        let rho = length(fq);
        let n = 2.0 + floor(s * 6.0);
        let across = clamp(phi / spread * 0.5 + 0.5, 0.0, 1.0);
        let r = fract(across * n);
        let between = min(r, 1.0 - r);
        let scallop = 0.12 * smoothstep(0.6, 0.8, s) * (1.0 - between * 2.0);
        let reach = (0.75 + 0.25 * cos(phi / spread * 1.3)) * (1.0 - scallop);
        let membrane = fill(abs(phi), spread, aa / max(rho, 0.05)) * fill(rho, reach, aa) * step(0.0, fq.x);
        let ray = (1.0 - smoothstep(0.0, 0.08 + aa / max(rho, 0.05), between)) * smoothstep(0.25, 0.4, s) * detail;
        col = mix(base * 0.85, base * 1.3 + vec3<f32>(0.12), ray);
        alpha = membrane * (0.7 + 0.3 * ray);
    } else if kind == 6u {
        // Tail fluke: a paddle that forks into a crescent as it grows.
        let cc = (c - kind_sway(kind, a, sway)) / max(w, 1e-3);
        let aw = aa / max(w, 1e-3);
        let fork = smoothstep(0.3, 0.8, s);
        let stalk = fill(abs(cc), 0.22 * (1.0 - a * 0.5), aw) * step(0.0, a) * step(a, 0.5);
        var blade = fill(length(vec2<f32>((a - 0.55) / 0.45, cc)), 1.0, aa * 2.5);
        let notch = length(vec2<f32>((a - 1.08) / (0.12 + 0.35 * fork), cc / (0.2 + 0.45 * fork)));
        blade *= smoothstep(1.0 - aa * 3.0, 1.0 + aa * 3.0, notch);
        alpha = max(stalk, blade);
        col = base * mix(1.1, 0.75, abs(cc));
        let ray = 1.0 - smoothstep(0.0, 0.12, abs(fract(cc * 3.0 + 0.5) - 0.5));
        col *= 1.0 - 0.2 * ray * fork * detail * step(0.5, a);
    } else if kind == 7u {
        // Leg: jointed, with more joints as it grows, knobbed joints, then a claw. Steps while walking.
        let nseg = 1 + u32(floor(s * 2.99));
        let step_t = time * 7.0 + phase;
        var p0 = vec2<f32>(0.0, 0.0);
        var ang = -0.25 + 0.25 * sin(step_t + 0.6) * walking;
        let seglen = 1.0 / f32(nseg);
        var dist = 1e3;
        var joints = 1e3;
        for (var k = 0u; k < 3u; k++) {
            if k >= nseg {
                break;
            }
            if k == 1u {
                ang += 0.7 + 0.25 * sin(step_t) * walking;
                joints = min(joints, length(q - p0) - w * 1.2);
            }
            if k == 2u {
                ang -= 1.1 + 0.25 * sin(step_t + 1.2) * walking;
                joints = min(joints, length(q - p0) - w * 1.1);
            }
            let p1 = p0 + vec2<f32>(cos(ang), sin(ang)) * seglen;
            let h = seg(q, p0, p1);
            let thick = w * (1.0 - 0.35 * (f32(k) + h.y) / f32(nseg));
            dist = min(dist, h.x - thick);
            p0 = p1;
        }
        let claw_w = smoothstep(0.65, 0.8, s);
        if claw_w > 0.0 {
            for (var k = 0; k < 2; k++) {
                let side = select(-1.0, 1.0, k == 1);
                let tip = p0 + vec2<f32>(cos(ang + side * 0.6), sin(ang + side * 0.6)) * 0.14;
                dist = min(dist, seg(q, p0, tip).x - w * 0.35 * claw_w);
            }
        }
        let knob = smoothstep(0.35, 0.5, s);
        dist = min(dist, mix(1e3, joints, knob));
        alpha = 1.0 - smoothstep(-aa, aa, dist);
        let chitin = mix(base, vec3<f32>(0.35, 0.25, 0.15), 0.35);
        col = chitin * mix(1.1, 0.7, clamp(-dist / max(w, 1e-3), 0.0, 1.0));
        col = mix(col, chitin * 0.5, (1.0 - smoothstep(-aa, aa * 2.0, joints)) * knob * 0.6);
    } else if kind == 8u {
        // Holdfast: a sucker disc that grows into a branching root system.
        let stem = fill(abs(c), w * 0.28, aa) * step(0.0, a) * step(a, 0.5);
        let n = 2 + i32(floor(s * 5.0));
        var roots = 0.0;
        for (var k = 0; k < 7; k++) {
            if k >= n {
                break;
            }
            let t = (f32(k) + 0.5) / f32(n) * 2.0 - 1.0;
            let tip = vec2<f32>(0.97, t * w * 0.95);
            let mid = vec2<f32>(0.68, t * w * 0.45 + 0.05 * sin(f32(k) * 2.3 + seed * 9.0));
            let h1 = seg(q, vec2<f32>(0.4, 0.0), mid);
            let h2 = seg(q, mid, tip);
            let along = select(0.5 + 0.5 * h2.y, 0.5 * h1.y, h1.x < h2.x);
            roots = max(roots, fill(min(h1.x, h2.x), w * 0.2 * (1.0 - 0.6 * along) + aa * 0.3, aa));
        }
        roots *= smoothstep(0.2, 0.35, s);
        let disc = fill(length(vec2<f32>((a - 0.85) / 0.15, c / max(w * 0.7, 1e-3))), 1.0, aa * 6.0)
            * (1.0 - smoothstep(0.3, 0.5, s));
        alpha = max(stem, max(roots, disc));
        col = mix(base, vec3<f32>(0.32, 0.22, 0.1), 0.55) * (0.85 + 0.15 * hash21(floor(q * 20.0) + seed));
    } else if kind == 9u {
        // Stalk: a slender stem, ringed when larger.
        let y = kind_sway(kind, a, sway);
        let hw = w * (1.0 - 0.3 * a);
        let dist = abs(c - y);
        alpha = fill(dist, hw + aa * 0.3, aa) * inside;
        col = base * mix(0.95, 0.65, dist / max(hw, 1e-3));
        let ring = 1.0 - smoothstep(0.1, 0.2, abs(fract(a * 10.0) - 0.5));
        col *= 1.0 - 0.25 * ring * smoothstep(0.45, 0.6, s) * detail;
    } else if kind == 10u {
        // Eye.
        let e = (q - vec2<f32>(0.5, 0.0)) / 0.5;
        let eye = eye_art(e, s, seed + index * 0.37, time, aa / 0.5);
        col = eye.rgb;
        alpha = eye.a;
    } else if kind == 11u {
        // Antenna: a twitching feeler; segmented, then feathery.
        let y = kind_sway(kind, a, sway);
        let hw = w * (1.0 - 0.6 * a);
        let dist = abs(c - y);
        alpha = fill(dist, hw + aa * 0.4, aa) * inside;
        col = mix(base, vec3<f32>(0.85, 0.75, 0.55), 0.3);
        let joint = 1.0 - smoothstep(0.1, 0.2, abs(fract(a * 10.0) - 0.5));
        col *= 1.0 - 0.3 * joint * smoothstep(0.35, 0.5, s) * detail;
        let plume = smoothstep(0.65, 0.8, s) * detail;
        if plume > 0.0 {
            let f = fract(a * 16.0 - dist / 0.12);
            let hair = step(f, 0.15) * step(dist, 0.12) * step(0.15, a) * inside;
            col = mix(col, vec3<f32>(0.9, 0.85, 0.7), hair * (1.0 - alpha));
            alpha = max(alpha, hair * plume * 0.7);
        }
    } else if kind == 12u {
        // Lure: a stalk with a glowing bulb; brighter and larger as it grows.
        let y = kind_sway(kind, a, sway);
        let stem = fill(abs(c - y), w * 0.22 + aa * 0.3, aa) * step(0.0, a) * step(a, 0.78);
        let bulb_c = vec2<f32>(0.8, kind_sway(kind, 0.8, sway));
        let br = w * (0.55 + 0.45 * s);
        let bd = length(q - bulb_c);
        let bulb = fill(bd, br, aa);
        let pulse = 0.75 + 0.25 * sin(time * 2.0 + phase);
        let glow_col = mix(vec3<f32>(0.35, 1.0, 0.85), base, 0.25) * (1.5 + 4.0 * s) * pulse;
        col = mix(base * 0.7, select(glow_col, vec3<f32>(0.4), dead), bulb);
        alpha = max(stem, bulb);
        if !dead {
            // Fade out before the quad's edge so the glow has no corners.
            let edge_fade = 1.0 - smoothstep(0.6, 1.0, max(abs(in.uv.x), abs(in.uv.y)));
            let halo = exp(-max(bd - br, 0.0) / (br * (0.5 + 1.0 * s))) * (0.3 + 0.5 * s) * pulse * edge_fade;
            col = mix(col, glow_col, halo * (1.0 - alpha));
            alpha = max(alpha, halo * 0.6);
        }
    } else if kind == 13u {
        // Spine: a thorn, then banded, then a barbed spike.
        let y = kind_sway(kind, a, sway);
        let barb = smoothstep(0.75, 0.9, s) * max(0.0, 1.0 - fract(a * 5.0) * 3.0) * step(0.25, a);
        let hw = w * (1.0 - a) + w * 0.5 * barb * (1.0 - a);
        alpha = fill(abs(c - y), hw + aa * 0.3, aa) * inside;
        col = mix(base * 0.4, vec3<f32>(0.9, 0.85, 0.72), 0.55 + 0.3 * a);
        let band = 1.0 - smoothstep(0.15, 0.25, abs(fract(a * 6.0) - 0.5));
        col *= 1.0 - 0.3 * band * smoothstep(0.4, 0.55, s) * detail;
    } else if kind == 14u {
        // Jaw: a chitin mandible that bows out and hooks in, snapping while eating; serrated
        // along its inner edge once it's large enough.
        let y = kind_sway(kind, a, sway);
        let hw = w * (1.0 - pow(a, 1.4) * 0.85);
        let dist = c - y;
        alpha = fill(abs(dist), hw + aa * 0.3, aa) * inside;
        let chitin = mix(vec3<f32>(0.2, 0.11, 0.05), vec3<f32>(0.85, 0.7, 0.5), a * a);
        col = mix(chitin, base * 0.5, 0.2) * mix(1.15, 0.8, abs(dist) / max(hw, 1e-3));
        let teeth_w = smoothstep(0.3, 0.45, s);
        if teeth_w > 0.0 {
            let count = 3.0 + floor(s * 5.0);
            let t = fract(a * count);
            let height = w * (0.3 + 0.3 * s) * (1.0 - abs(t - 0.5) * 2.0);
            let inward = -dist - hw;
            let tooth = step(-aa, inward) * step(inward, height) * step(0.15, a) * step(a, 0.95) * teeth_w;
            col = mix(col, vec3<f32>(0.3, 0.17, 0.07), tooth);
            alpha = max(alpha, tooth);
        }
    } else if kind == 15u {
        // Shell plate: a thin scale, then a plate with growth lines, then a thick ridged plate.
        let rb = 1.0 / (0.08 + 0.25 * s);
        let ac = a + c * c / (2.0 * rb);
        alpha = fill(abs(ac - 0.5), 0.5, aa) * fill(abs(c), w, aa);
        col = mix(base * 0.45, vec3<f32>(0.55, 0.5, 0.4), 0.45);
        let lines = 1.0 - smoothstep(0.0, 0.15, abs(fract(ac * (2.0 + 4.0 * s)) - 0.5));
        col *= 1.0 - 0.25 * lines * smoothstep(0.35, 0.5, s) * detail;
        let knobs = fill(length(vec2<f32>(ac - 0.75, fract(c * 1.5 + 0.5) - 0.5)), 0.12, aa) * smoothstep(0.7, 0.85, s);
        col = mix(col, col * 1.4 + vec3<f32>(0.08), knobs * detail);
        col += vec3<f32>(0.12) * smoothstep(0.7, 1.0, ac);
        alpha *= mix(0.75, 1.0, smoothstep(0.0, 0.4, s));
    }

    return vec4<f32>(col, alpha);
}

// ---- Parts changing kind ----
//
// A part partway along a path to a related kind (see `PartKind::paths`) has art of its own for
// that pair of kinds, drawn continuously in how far along it is (t: 0 = the lower-numbered
// kind, 1 = the other): features grow, shrink, split, fuse and bend into the other kind's,
// rather than one kind fading into the other.

// What a part's look is drawn from.
struct PartIn {
    a: f32,        // along the part: 0 at the base, 1 at the tip (in part lengths)
    c: f32,        // across it (mirrored copies curl the other way)
    w: f32,        // nominal half width
    s: f32,        // size
    aa: f32,       // about one pixel
    detail: f32,   // fine detail only once there are enough pixels to show it
    time: f32,
    activity: f32,
    walking: f32,
    phase: f32,
    seed: f32,
    index: f32,
    base: vec3<f32>,
    oxygenic: f32,
    photo: f32,
    dead: bool,
    uv: vec2<f32>,
    sway: Sway,
};

fn part_in(in: VertexOutput) -> PartIn {
    let flags = u32(in.state.z * 255.0 + 0.5);
    let dead = (flags & FLAG_DEAD) != 0u;
    let mi = u32(in.body.z * 255.0 + 0.5);
    let mirror = select(1.0, -1.0, (mi & 1u) == 1u);
    let index = f32(mi >> 1u);
    let lp = in.uv * in.shape.xy;
    let time = select(params.x, 0.0, dead);
    let activity = select(in.motion.x, 0.0, dead);
    let phase = in.body.w * 40.0 + index * 1.9;
    let gape = in.motion.z * abs(sin(time * 14.0 + index)) * 0.2;
    var p: PartIn;
    p.a = lp.x + 0.5;
    p.c = lp.y * mirror;
    p.w = in.shape.z;
    p.s = in.body.y;
    p.aa = 1.0 / max(in.shape.w, 0.5);
    p.detail = smoothstep(6.0, 16.0, in.shape.w);
    p.time = time;
    p.activity = activity;
    p.walking = select(in.motion.y, 0.0, dead);
    p.phase = phase;
    p.seed = in.body.w;
    p.index = index;
    p.base = in.tint.rgb;
    p.oxygenic = in.chem.x;
    p.photo = in.diet.x;
    p.dead = dead;
    p.uv = in.uv;
    p.sway = Sway(in.body.y, time, phase, activity, in.chem.z, (in.chem.w * 2.0 - 1.0) * 0.75, gape);
    return p;
}

// Inside the part's length, softly at the tip.
fn along(p: PartIn) -> f32 {
    return step(0.0, p.a) * (1.0 - smoothstep(1.0 - p.aa, 1.0 + p.aa, p.a));
}

// A frond's leaf colour (as the frond look colours it).
fn leaf_pigment(p: PartIn) -> vec3<f32> {
    let purple = vec3<f32>(0.5, 0.12, 0.48);
    let golden = vec3<f32>(0.58, 0.44, 0.1);
    let green = vec3<f32>(0.14, 0.58, 0.1);
    let pigment = select(
        mix(golden, green, p.oxygenic * 2.0 - 1.0),
        mix(purple, golden, p.oxygenic * 2.0),
        p.oxygenic < 0.5,
    );
    let faded = mix(pigment, vec3<f32>(0.8, 0.82, 0.72), 0.45);
    return mix(mix(faded, pigment, smoothstep(0.0, 0.6, p.photo)), p.base, 0.08);
}

// Lines where fract(u) is near 0.
fn lines_at(u: f32, width: f32) -> f32 {
    let f = fract(u);
    return 1.0 - smoothstep(0.0, width, min(f, 1.0 - f));
}

// Flagellum → tentacle: the whip thickens into an arm. Its fringe of hairs draws in, grooves
// deepen into rings, suckers bud and grow, a club swells at the tip and the basal body shrinks.
fn flagellum_to_tentacle(p: PartIn, t: f32) -> vec4<f32> {
    let a = p.a;
    let c = p.c;
    let w = p.w;
    let s = p.s;
    let aa = p.aa;
    let y = mix(kind_sway(1u, a, p.sway), kind_sway(3u, a, p.sway), t);
    let ringed = smoothstep(0.3, 0.45, s) * p.detail;
    let ring = 1.0 - smoothstep(0.08, 0.2, abs(fract(a * 14.0) - 0.5));
    var hw = w * mix(1.0 - 0.7 * a, 1.0 - 0.65 * a, t);
    hw *= 1.0 - 0.12 * ring * ringed * t;
    hw += w * 0.6 * smoothstep(0.78, 0.92, a) * (1.0 - smoothstep(0.94, 1.0, a))
        * smoothstep(0.75, 0.95, s) * t;
    let dist = abs(c - y);
    let inside = along(p);
    var alpha = fill(dist, hw + aa * 0.4, aa) * inside;
    let whip = mix(p.base * 0.8 + vec3<f32>(0.06), p.base * 1.1 + vec3<f32>(0.12), a);
    let arm = p.base * mix(1.1, 0.7, dist / max(hw, 1e-3)) * (1.0 - 0.3 * ring * ringed);
    var col = mix(whip, arm, t);
    let reach = hw * (1.0 + 2.5 * (1.0 - t));
    let hairs = step(fract(a * 30.0 + p.phase), 0.22) * step(dist, reach + aa) * inside
        * step(0.12, a) * smoothstep(0.55, 0.75, s) * p.detail;
    col = mix(col, p.base * 1.3 + vec3<f32>(0.2), hairs * (1.0 - alpha));
    alpha = max(alpha, hairs * 0.55);
    if s > 0.55 {
        let ai = (floor(a * 12.0) + 0.5) / 12.0;
        let sucker = fill(length(vec2<f32>(a - ai, c - (y - hw * 0.45))), hw * 0.32 * t, aa)
            * step(0.1, a);
        col = mix(col, vec3<f32>(0.95, 0.8, 0.75), sucker * smoothstep(0.55, 0.7, s) * p.detail * alpha);
    }
    let basal = fill(length(vec2<f32>(a, c)), w * 1.8 * (1.0 - t), aa) * smoothstep(0.25, 0.4, s);
    col = mix(col, p.base * 0.55, basal);
    alpha = max(alpha, basal);
    return vec4<f32>(col, alpha);
}

// Flagellum → cilia band: the whip splits into a row of short hairs, side by side, spreading
// out from the middle as a band grows across its base; the whip's beat becomes their ripple.
fn flagellum_to_cilia(p: PartIn, t: f32) -> vec4<f32> {
    let a = p.a;
    let c = p.c;
    let s = p.s;
    let aa = p.aa;
    let spacing = mix(0.4, 0.13, s);
    let span = p.w * t;
    // Each strand stays about a whip's thickness, however wide the band grows.
    let thick = mix(min(p.w, 0.06) * (1.0 - 0.7 * a), 0.035 + 0.02 * s, t) + aa * 0.3;
    let inside = step(0.0, a) * step(a, 1.0);
    let beat = kind_sway(1u, a, p.sway) * (1.0 - t);
    var hairs = 0.0;
    let i0 = round(c / spacing);
    for (var k = -2; k <= 2; k++) {
        let i = i0 + f32(k);
        let y0 = i * spacing;
        if abs(y0) > span + 1e-4 && i != 0.0 {
            continue;
        }
        let tilt = 0.35 * sin(p.time * 12.0 * (0.4 + p.activity) - i * 0.7 + p.phase) * a * a * t;
        hairs = max(hairs, fill(abs(c - y0 - beat - tilt), thick, aa) * inside);
    }
    let band = fill(abs(c), span, aa) * step(-0.08, a) * (1.0 - smoothstep(0.1 * t, 0.16 * t + aa, a))
        * step(0.001, t);
    let whip = mix(p.base * 0.8 + vec3<f32>(0.06), p.base * 1.1 + vec3<f32>(0.12), a);
    let hair = p.base * 0.9 + vec3<f32>(0.3, 0.3, 0.25);
    var col = mix(p.base * 0.7, mix(whip, hair, t), hairs);
    var alpha = max(band, hairs * mix(1.0, 0.9, t));
    // The whip's own fringe of fine hairs draws in as it splits.
    let dist = abs(c - beat);
    let fringe = step(fract(a * 30.0 + p.phase), 0.22) * step(dist, thick * (1.0 + 2.5 * (1.0 - t)) + aa)
        * inside * step(0.12, a) * smoothstep(0.55, 0.75, s) * p.detail;
    col = mix(col, p.base * 1.3 + vec3<f32>(0.2), fringe * (1.0 - alpha));
    alpha = max(alpha, fringe * 0.55);
    let basal = fill(length(vec2<f32>(a, c)), thick * 1.8 * (1.0 - t), aa) * smoothstep(0.25, 0.4, s);
    col = mix(col, p.base * 0.55, basal);
    return vec4<f32>(col, max(alpha, basal));
}

// Cilia band → fin: the hairs fan out into rays, and a web grows between them from the base out.
fn cilia_to_fin(p: PartIn, t: f32) -> vec4<f32> {
    let a = p.a;
    let s = p.s;
    let aa = p.aa;
    let w = p.w;
    let c = p.c - 0.25 * sin(p.time * (2.5 + 7.0 * p.activity) + p.phase) * a * t;
    let spread = atan(w * 1.6);
    let n = 3.0 + floor(s * 6.0);
    // The fin's rim: rays and web end at a rounded edge (straight across for the band's hairs).
    let rho = length(vec2<f32>(a, c));
    let rim = 0.75 + 0.25 * cos(atan2(c, max(a, 1e-4)) / spread * 1.3);
    let out = mix(a, rho / rim, t);
    let inside = step(0.0, a) * (1.0 - smoothstep(1.0 - aa, 1.0 + aa, out));
    var rays = 0.0;
    for (var k = 0; k < 9; k++) {
        if f32(k) >= n {
            break;
        }
        let across = f32(k) / (n - 1.0) * 2.0 - 1.0;
        let y = mix(across * w, a * tan(spread * across), t);
        let tilt = 0.35 * sin(p.time * 12.0 * (0.4 + p.activity) - f32(k) * 0.7 + p.phase) * a * a
            * (1.0 - t);
        rays = max(rays, fill(abs(c - y - tilt), mix(0.035 + 0.02 * s, 0.015, t) + aa * 0.3, aa));
    }
    rays *= inside;
    let edge = mix(w, a * tan(spread), t);
    // The web grows out from the base, reaching the rim by the time it's a fin.
    let reach = mix(0.16, 1.0, min(1.0, t * 1.3));
    let web = fill(abs(c), edge, aa) * step(mix(-0.08, 0.0, t), a) * (1.0 - smoothstep(reach - aa, reach + aa, out));
    let hair_col = mix(p.base * 0.7, p.base * 0.9 + vec3<f32>(0.3, 0.3, 0.25), rays);
    let fin_col = mix(p.base * 0.85, p.base * 1.3 + vec3<f32>(0.12), rays);
    let col = mix(hair_col, fin_col, t);
    let alpha = max(web * mix(1.0, 0.7 + 0.3 * rays, t), rays * mix(0.9, 1.0, t));
    return vec4<f32>(col, alpha);
}

// Frond → fin: the leaf stiffens into a paddle. Its outline widens toward the tip, its serrated
// edge smooths, its veins swing round into rays and the gas bladder shrinks away.
fn frond_to_fin(p: PartIn, t: f32) -> vec4<f32> {
    let a = p.a;
    let s = p.s;
    let aa = p.aa;
    let w = p.w;
    let c = p.c - mix(kind_sway(4u, a, p.sway), kind_sway(5u, a, p.sway), t);
    var hw = mix(kind_width(4u, a, w, s), kind_width(5u, a, w, s), t);
    hw *= 1.0 - 0.15 * smoothstep(0.65, 0.85, s) * abs(sin(a * 45.0)) * (1.0 - t);
    let dist = abs(c);
    var alpha = fill(dist, hw + aa * 0.3, aa) * step(0.0, a) * step(a, 1.0);
    let n = 2.0 + floor(s * 6.0);
    let vein = a * 9.0 - dist / max(w, 1e-3) * 1.2;
    let ray = (c / max(hw, 1e-3) * 0.5 + 0.5) * n;
    let line = lines_at(mix(vein, ray, t), mix(0.08, 0.06, t)) * smoothstep(0.3, 0.45, s) * p.detail;
    var leaf = leaf_pigment(p) * mix(1.15, 0.8, dist / max(hw, 1e-3));
    leaf = mix(leaf, leaf * 0.6, fill(dist, (0.015 + aa * 0.5) * (1.0 - t), aa) * smoothstep(0.2, 0.35, s));
    leaf *= 1.0 - 0.25 * line;
    let fin = mix(p.base * 0.85, p.base * 1.3 + vec3<f32>(0.12), line);
    var col = mix(leaf, fin, t);
    alpha *= mix(1.0, 0.7 + 0.3 * line, t);
    let bladder = fill(length(vec2<f32>(a - 0.14, c)), w * 0.55 * (1.0 - t), aa) * smoothstep(0.8, 0.92, s);
    col = mix(col, vec3<f32>(0.75, 0.65, 0.3), bladder);
    alpha = max(alpha, bladder);
    return vec4<f32>(col, alpha);
}

// Fin → leg: the paddle's middle ray thickens into a limb that gains joints and bends at them,
// while the web shrinks back along it; knobs and a claw grow as it becomes a leg.
fn fin_to_leg(p: PartIn, t: f32) -> vec4<f32> {
    let s = p.s;
    let aa = p.aa;
    let w = p.w;
    let q = vec2<f32>(p.a, p.c - 0.25 * sin(p.time * (2.5 + 7.0 * p.activity) + p.phase) * p.a * (1.0 - t));
    let nseg = 1u + u32(floor(s * 2.99));
    let step_t = p.time * 7.0 + p.phase;
    var p0 = vec2<f32>(0.0, 0.0);
    var ang = (-0.25 + 0.25 * sin(step_t + 0.6) * p.walking) * t;
    let seglen = 1.0 / f32(nseg);
    var limb = 1e3;
    var middle = 1e3;
    var at = 0.0;
    var joints = 1e3;
    for (var k = 0u; k < 3u; k++) {
        if k >= nseg {
            break;
        }
        if k == 1u {
            ang += (0.7 + 0.25 * sin(step_t) * p.walking) * t;
            joints = min(joints, length(q - p0) - w * 1.2);
        }
        if k == 2u {
            ang -= (1.1 + 0.25 * sin(step_t + 1.2) * p.walking) * t;
            joints = min(joints, length(q - p0) - w * 1.1);
        }
        let p1 = p0 + vec2<f32>(cos(ang), sin(ang)) * seglen;
        let h = seg(q, p0, p1);
        let thick = w * mix(0.12, 1.0 - 0.35 * (f32(k) + h.y) / f32(nseg), t);
        if h.x < middle {
            middle = h.x;
            at = (f32(k) + h.y) / f32(nseg);
        }
        limb = min(limb, h.x - thick);
        p0 = p1;
    }
    let claw_w = smoothstep(0.65, 0.8, s) * t;
    if claw_w > 0.0 {
        for (var k = 0; k < 2; k++) {
            let side = select(-1.0, 1.0, k == 1);
            let tip = p0 + vec2<f32>(cos(ang + side * 0.6), sin(ang + side * 0.6)) * 0.14;
            limb = min(limb, seg(q, p0, tip).x - w * 0.35 * claw_w);
        }
    }
    let knob = smoothstep(0.35, 0.5, s) * t;
    limb = min(limb, mix(1e3, joints, knob));
    let bone = 1.0 - smoothstep(-aa, aa, limb);
    // The web, either side of the limb, as wide as the fin's outline there, shrinking.
    let web_w = kind_width(5u, at, w, s) * (1.0 - t);
    let web = fill(middle, web_w, aa) * step(0.0, q.x);
    let n = 2.0 + floor(s * 6.0);
    let ray = lines_at(middle / max(web_w, 1e-3) * n * 0.5, 0.1) * smoothstep(0.25, 0.4, s) * p.detail;
    let web_col = mix(p.base * 0.85, p.base * 1.3 + vec3<f32>(0.12), ray);
    let chitin = mix(p.base, vec3<f32>(0.35, 0.25, 0.15), 0.35 * t);
    var bone_col = mix(p.base * 1.2 + vec3<f32>(0.1), chitin * mix(1.1, 0.7, clamp(-limb / max(w, 1e-3), 0.0, 1.0)), t);
    bone_col = mix(bone_col, chitin * 0.5, (1.0 - smoothstep(-aa, aa * 2.0, joints)) * knob * 0.6);
    let col = mix(web_col, bone_col, bone);
    return vec4<f32>(col, max(bone, web * (0.7 + 0.3 * ray)));
}

// Fin → tail fluke: the paddle's base narrows into a stalk, its blade rounds out, and a notch
// opens at its tip, forking it into a crescent.
fn fin_to_fluke(p: PartIn, t: f32) -> vec4<f32> {
    let a = p.a;
    let s = p.s;
    let aa = p.aa;
    let w = p.w;
    let c = p.c - mix(kind_sway(5u, a, p.sway), kind_sway(6u, a, p.sway), t);
    let hw = mix(kind_width(5u, a, w, s), kind_width(6u, a, w, s), t);
    var alpha = fill(abs(c), hw + aa * 0.3, aa) * step(0.0, a) * step(a, 1.05);
    let cc = c / max(w, 1e-3);
    let fork = smoothstep(0.3, 0.8, s);
    let notch = length(vec2<f32>((a - 1.08 - 0.4 * (1.0 - t)) / (0.12 + 0.35 * fork), cc / (0.2 + 0.45 * fork)));
    alpha *= smoothstep(1.0 - aa * 3.0, 1.0 + aa * 3.0, notch);
    let n = 2.0 + floor(s * 6.0);
    let line = lines_at(mix((c / max(hw, 1e-3) * 0.5 + 0.5) * n, cc * 3.0, t), 0.1)
        * mix(smoothstep(0.25, 0.4, s), fork * step(0.5, a), t) * p.detail;
    let fin = mix(p.base * 0.85, p.base * 1.3 + vec3<f32>(0.12), line);
    let fluke = p.base * mix(1.1, 0.75, abs(cc)) * (1.0 - 0.2 * line);
    alpha *= mix(0.7 + 0.3 * line, 1.0, t);
    return vec4<f32>(mix(fin, fluke, t), alpha);
}

// Leg → holdfast: the limb straightens and shortens into a stem, its knobs and claw shrink, and
// roots grow out from its end (or, on a small one, a sucker disc).
fn leg_to_holdfast(p: PartIn, t: f32) -> vec4<f32> {
    let s = p.s;
    let aa = p.aa;
    let w = p.w;
    let q = vec2<f32>(p.a, p.c);
    let nseg = 1u + u32(floor(s * 2.99));
    let step_t = p.time * 7.0 + p.phase;
    let bend = 1.0 - t;
    var p0 = vec2<f32>(0.0, 0.0);
    var ang = (-0.25 + 0.25 * sin(step_t + 0.6) * p.walking) * bend;
    let seglen = mix(1.0, 0.45, t) / f32(nseg);
    var dist = 1e3;
    var joints = 1e3;
    for (var k = 0u; k < 3u; k++) {
        if k >= nseg {
            break;
        }
        if k == 1u {
            ang += (0.7 + 0.25 * sin(step_t) * p.walking) * bend;
            joints = min(joints, length(q - p0) - w * 1.2);
        }
        if k == 2u {
            ang -= (1.1 + 0.25 * sin(step_t + 1.2) * p.walking) * bend;
            joints = min(joints, length(q - p0) - w * 1.1);
        }
        let p1 = p0 + vec2<f32>(cos(ang), sin(ang)) * seglen;
        let h = seg(q, p0, p1);
        let thick = w * mix(1.0 - 0.35 * (f32(k) + h.y) / f32(nseg), 0.28, t);
        dist = min(dist, h.x - thick);
        p0 = p1;
    }
    let claw_w = smoothstep(0.65, 0.8, s) * bend;
    if claw_w > 0.0 {
        for (var k = 0; k < 2; k++) {
            let side = select(-1.0, 1.0, k == 1);
            let tip = p0 + vec2<f32>(cos(ang + side * 0.6), sin(ang + side * 0.6)) * 0.14;
            dist = min(dist, seg(q, p0, tip).x - w * 0.35 * claw_w);
        }
    }
    let knob = smoothstep(0.35, 0.5, s) * bend;
    dist = min(dist, mix(1e3, joints, knob));
    let limb = 1.0 - smoothstep(-aa, aa, dist);
    // Roots, growing out from the end of the limb to where a holdfast's reach.
    let n = 2 + i32(floor(s * 5.0));
    var roots = 0.0;
    for (var k = 0; k < 7; k++) {
        if k >= n {
            break;
        }
        let r = (f32(k) + 0.5) / f32(n) * 2.0 - 1.0;
        let tip = p0 + (vec2<f32>(0.97, r * w * 0.95) - p0) * t;
        let mid = p0 + (vec2<f32>(0.68, r * w * 0.45 + 0.05 * sin(f32(k) * 2.3 + p.seed * 9.0)) - p0) * t;
        let h1 = seg(q, p0, mid);
        let h2 = seg(q, mid, tip);
        let at = select(0.5 + 0.5 * h2.y, 0.5 * h1.y, h1.x < h2.x);
        roots = max(roots, fill(min(h1.x, h2.x), w * 0.2 * (1.0 - 0.6 * at) * min(1.0, t * 3.0) + aa * 0.3, aa));
    }
    roots *= smoothstep(0.2, 0.35, s) * step(0.001, t);
    let disc = fill(length(vec2<f32>((p.a - 0.85) / 0.15, p.c / max(w * 0.7 * t, 1e-3))), 1.0, aa * 6.0)
        * (1.0 - smoothstep(0.3, 0.5, s)) * step(0.001, t);
    let chitin = mix(p.base, vec3<f32>(0.35, 0.25, 0.15), 0.35);
    let leg_col = chitin * mix(1.1, 0.7, clamp(-dist / max(w, 1e-3), 0.0, 1.0));
    let root_col = mix(p.base, vec3<f32>(0.32, 0.22, 0.1), 0.55) * (0.85 + 0.15 * hash21(floor(q * 20.0) + p.seed));
    var col = mix(leg_col, root_col, t);
    col = mix(col, chitin * 0.5, (1.0 - smoothstep(-aa, aa * 2.0, joints)) * knob * 0.6);
    col = mix(col, root_col, max(roots, disc) * (1.0 - limb));
    return vec4<f32>(col, max(limb, max(roots, disc)));
}

// The feathery plume of a large antenna, its hairs `reach` long.
fn plume(p: PartIn, dist: f32, reach: f32) -> f32 {
    let f = fract(p.a * 16.0 - dist / 0.12);
    return step(f, 0.15) * step(dist, reach) * step(0.15, p.a) * step(p.a, 1.0)
        * smoothstep(0.65, 0.8, p.s) * p.detail;
}

const FEELER: vec3<f32> = vec3<f32>(0.85, 0.75, 0.55);
const PLUME: vec3<f32> = vec3<f32>(0.9, 0.85, 0.7);

// Tentacle → antenna: the arm thins into a feeler. Its rings tighten into joints, its suckers and
// club shrink away, and a feathery plume grows along it.
fn tentacle_to_antenna(p: PartIn, t: f32) -> vec4<f32> {
    let a = p.a;
    let c = p.c;
    let w = p.w;
    let s = p.s;
    let aa = p.aa;
    let y = mix(kind_sway(3u, a, p.sway), kind_sway(11u, a, p.sway), t);
    let club = w * 0.6 * smoothstep(0.78, 0.92, a) * (1.0 - smoothstep(0.94, 1.0, a))
        * smoothstep(0.75, 0.95, s) * (1.0 - t);
    let hw = w * mix(1.0 - 0.65 * a, 1.0 - 0.6 * a, t) + club;
    let dist = abs(c - y);
    var alpha = fill(dist, hw + aa * 0.35, aa) * along(p);
    let ring = 1.0 - smoothstep(0.08, 0.2, abs(fract(a * mix(14.0, 10.0, t)) - 0.5));
    let marks = mix(smoothstep(0.3, 0.45, s), smoothstep(0.35, 0.5, s), t) * p.detail * 0.3;
    var col = mix(p.base * mix(1.1, 0.7, dist / max(hw, 1e-3)), mix(p.base, FEELER, 0.3), t);
    col *= 1.0 - marks * ring;
    if s > 0.55 {
        let ai = (floor(a * 12.0) + 0.5) / 12.0;
        let sucker = fill(length(vec2<f32>(a - ai, c - (y - hw * 0.45))), hw * 0.32 * (1.0 - t), aa)
            * step(0.1, a);
        col = mix(col, vec3<f32>(0.95, 0.8, 0.75), sucker * smoothstep(0.55, 0.7, s) * p.detail * alpha);
    }
    let hair = plume(p, dist, 0.12 * t);
    col = mix(col, PLUME, hair * (1.0 - alpha));
    alpha = max(alpha, hair * 0.7);
    return vec4<f32>(col, alpha);
}

// Stalk → antenna: the stem thins and starts to twitch, its rings turn into joints and a
// feathery plume grows along it.
fn stalk_to_antenna(p: PartIn, t: f32) -> vec4<f32> {
    let a = p.a;
    let w = p.w;
    let s = p.s;
    let aa = p.aa;
    let y = mix(kind_sway(9u, a, p.sway), kind_sway(11u, a, p.sway), t);
    let hw = w * mix(1.0 - 0.3 * a, 1.0 - 0.6 * a, t);
    let dist = abs(p.c - y);
    var alpha = fill(dist, hw + aa * mix(0.3, 0.4, t), aa) * step(0.0, a) * step(a, 1.0);
    let ring = 1.0 - smoothstep(0.1, 0.2, abs(fract(a * 10.0) - 0.5));
    let marks = mix(0.25 * smoothstep(0.45, 0.6, s), 0.3 * smoothstep(0.35, 0.5, s), t) * p.detail;
    var col = mix(p.base * mix(0.95, 0.65, dist / max(hw, 1e-3)), mix(p.base, FEELER, 0.3), t);
    col *= 1.0 - marks * ring;
    let hair = plume(p, dist, 0.12 * t);
    col = mix(col, PLUME, hair * (1.0 - alpha));
    alpha = max(alpha, hair * 0.7);
    return vec4<f32>(col, alpha);
}

// Eye → antenna: a feeler grows out from under the eye, carrying it outward and shrinking it as
// it goes, until only the feeler is left.
fn eye_to_antenna(p: PartIn, t: f32) -> vec4<f32> {
    let a = p.a;
    let w = p.w;
    let aa = p.aa;
    let q = vec2<f32>(a, p.c);
    let y = kind_sway(11u, a, p.sway) * t;
    let hw = w * (1.0 - 0.6 * a) * mix(0.25, 1.0, t);
    let dist = abs(p.c - y);
    let feeler = fill(dist, hw + aa * 0.4, aa) * step(0.0, a) * (1.0 - smoothstep(1.0 - aa, 1.0 + aa, a));
    var col = mix(p.base, FEELER, 0.3);
    var alpha = feeler;
    let hair = plume(p, dist, 0.12 * t);
    col = mix(col, PLUME, hair * (1.0 - alpha));
    alpha = max(alpha, hair * 0.7);
    // The eye rides on the feeler's tip, shrinking as the feeler grows.
    let radius = min(0.5, w * sqrt(1.0 - t)) + 1e-4;
    let tip = 1.0 - radius;
    let centre = vec2<f32>(tip, kind_sway(11u, tip, p.sway) * t);
    let eye = eye_art((q - centre) / radius, p.s, p.seed + p.index * 0.37, p.time, aa / radius);
    col = mix(col, eye.rgb, eye.a);
    alpha = max(alpha, eye.a);
    return vec4<f32>(col, alpha);
}

// Eye → lure: the eye rises on a stalk that grows beneath it, shrinking into a bulb, while light
// spreads out from its pupil until it glows.
fn eye_to_lure(p: PartIn, t: f32) -> vec4<f32> {
    let a = p.a;
    let w = p.w;
    let s = p.s;
    let aa = p.aa;
    let q = vec2<f32>(a, p.c);
    let sway = 0.15 * sin(p.time * 0.9 + p.phase) + 0.1;
    let y = sway * a * a * t;
    let centre = vec2<f32>(mix(0.5, 0.8, t), sway * 0.64 * t);
    let radius = min(0.5, w * mix(1.0, 0.55 + 0.45 * s, t));
    let stem = fill(abs(p.c - y), w * 0.22 * t + aa * 0.3, aa) * step(0.0, a) * step(a, centre.x)
        * step(0.001, t);
    let eye = eye_art((q - centre) / radius, s, p.seed + p.index * 0.37, p.time, aa / radius);
    let pulse = 0.75 + 0.25 * sin(p.time * 2.0 + p.phase);
    let glow = mix(vec3<f32>(0.35, 1.0, 0.85), p.base, 0.25) * (1.5 + 4.0 * s) * pulse;
    // Light spreads out from the pupil until it fills the eye.
    let e = (q - centre) / radius;
    let lit = 1.0 - smoothstep(t * 1.05 - aa / radius, t * 1.05 + aa / radius,
        length(e - vec2<f32>(0.2 * (1.0 - t), 0.0)));
    let bulb = select(mix(eye.rgb, glow, lit), vec3<f32>(0.4), p.dead && t > 0.5);
    var col = mix(p.base * 0.7, bulb, eye.a);
    var alpha = max(stem, eye.a);
    if !p.dead {
        let edge_fade = 1.0 - smoothstep(0.6, 1.0, max(abs(p.uv.x), abs(p.uv.y)));
        let bd = length(q - centre);
        let halo = exp(-max(bd - radius, 0.0) / (radius * (0.5 + 1.0 * s))) * (0.3 + 0.5 * s) * pulse
            * edge_fade * t;
        col = mix(col, glow, halo * (1.0 - alpha));
        alpha = max(alpha, halo * 0.6);
    }
    return vec4<f32>(col, alpha);
}

// Spine → jaw: the thorn bows out and hooks in, its barbs shrink away and teeth grow along its
// inner edge as it becomes a mandible.
fn spine_to_jaw(p: PartIn, t: f32) -> vec4<f32> {
    let a = p.a;
    let w = p.w;
    let s = p.s;
    let aa = p.aa;
    let y = mix(kind_sway(13u, a, p.sway), kind_sway(14u, a, p.sway), t);
    let barb = smoothstep(0.75, 0.9, s) * max(0.0, 1.0 - fract(a * 5.0) * 3.0) * step(0.25, a) * (1.0 - t);
    let hw = mix(w * (1.0 - a) + w * 0.5 * barb * (1.0 - a), w * (1.0 - pow(clamp(a, 0.0, 1.0), 1.4) * 0.85), t);
    let dist = p.c - y;
    var alpha = fill(abs(dist), hw + aa * 0.3, aa) * step(0.0, a) * step(a, 1.0);
    let band = 1.0 - smoothstep(0.15, 0.25, abs(fract(a * 6.0) - 0.5));
    let spine = mix(p.base * 0.4, vec3<f32>(0.9, 0.85, 0.72), 0.55 + 0.3 * a)
        * (1.0 - 0.3 * band * smoothstep(0.4, 0.55, s) * p.detail);
    let chitin = mix(vec3<f32>(0.2, 0.11, 0.05), vec3<f32>(0.85, 0.7, 0.5), a * a);
    let jaw = mix(chitin, p.base * 0.5, 0.2) * mix(1.15, 0.8, abs(dist) / max(hw, 1e-3));
    var col = mix(spine, jaw, t);
    let teeth_w = smoothstep(0.3, 0.45, s);
    if teeth_w > 0.0 {
        let count = 3.0 + floor(s * 5.0);
        let f = fract(a * count);
        let height = w * (0.3 + 0.3 * s) * (1.0 - abs(f - 0.5) * 2.0) * t;
        let inward = -dist - hw;
        let tooth = step(-aa, inward) * step(inward, height) * step(0.15, a) * step(a, 0.95) * teeth_w
            * step(0.001, t);
        col = mix(col, vec3<f32>(0.3, 0.17, 0.07), tooth);
        alpha = max(alpha, tooth);
    }
    return vec4<f32>(col, alpha);
}

// Spine → shell plate: the thorn flattens out sideways and curves into a plate, its bands
// spreading into growth lines.
fn spine_to_shell(p: PartIn, t: f32) -> vec4<f32> {
    let w = p.w;
    let s = p.s;
    let aa = p.aa;
    let c = p.c - kind_sway(13u, p.a, p.sway) * (1.0 - t);
    let rb = 1.0 / (0.08 + 0.25 * s);
    let ac = p.a + c * c / (2.0 * rb) * t;
    let hw = w * mix(1.0 - clamp(ac, 0.0, 1.0), 1.0, t);
    var alpha = fill(abs(ac - 0.5), 0.5, aa) * fill(abs(c), hw + aa * 0.3, aa);
    let freq = mix(6.0, 2.0 + 4.0 * s, t);
    let line = 1.0 - smoothstep(mix(0.15, 0.0, t), mix(0.25, 0.15, t), abs(fract(ac * freq) - 0.5));
    let marks = mix(0.3 * smoothstep(0.4, 0.55, s), 0.25 * smoothstep(0.35, 0.5, s), t) * p.detail;
    let spine = mix(p.base * 0.4, vec3<f32>(0.9, 0.85, 0.72), 0.55 + 0.3 * ac);
    let shell = mix(p.base * 0.45, vec3<f32>(0.55, 0.5, 0.4), 0.45);
    var col = mix(spine, shell, t) * (1.0 - marks * line);
    let knobs = fill(length(vec2<f32>(ac - 0.75, fract(c * 1.5 + 0.5) - 0.5)), 0.12 * t, aa) * smoothstep(0.7, 0.85, s);
    col = mix(col, col * 1.4 + vec3<f32>(0.08), knobs * p.detail);
    col += vec3<f32>(0.12) * smoothstep(0.7, 1.0, ac) * t;
    alpha *= mix(1.0, mix(0.75, 1.0, smoothstep(0.0, 0.4, s)), t);
    return vec4<f32>(col, alpha);
}

// The art for a part `t` of the way from kind `lo` to the related kind `hi` (lo < hi).
fn link_look(p: PartIn, lo: u32, hi: u32, t: f32) -> vec4<f32> {
    if lo == 1u && hi == 3u {
        return flagellum_to_tentacle(p, t);
    } else if lo == 1u && hi == 2u {
        return flagellum_to_cilia(p, t);
    } else if lo == 2u && hi == 5u {
        return cilia_to_fin(p, t);
    } else if lo == 4u && hi == 5u {
        return frond_to_fin(p, t);
    } else if lo == 5u && hi == 7u {
        return fin_to_leg(p, t);
    } else if lo == 5u && hi == 6u {
        return fin_to_fluke(p, t);
    } else if lo == 7u && hi == 8u {
        return leg_to_holdfast(p, t);
    } else if lo == 3u && hi == 11u {
        return tentacle_to_antenna(p, t);
    } else if lo == 9u && hi == 11u {
        return stalk_to_antenna(p, t);
    } else if lo == 10u && hi == 11u {
        return eye_to_antenna(p, t);
    } else if lo == 10u && hi == 12u {
        return eye_to_lure(p, t);
    } else if lo == 13u && hi == 14u {
        return spine_to_jaw(p, t);
    } else if lo == 13u && hi == 15u {
        return spine_to_shell(p, t);
    }
    // Not a link (the paths in parts.rs and these must match).
    return vec4<f32>(1.0, 0.0, 1.0, 1.0);
}

// A body part: its kind's look, or, partway to a related kind (the other kind in the high
// nibble, how far in wound.x), that pair's own in-between art.
fn part(in: VertexOutput) -> vec4<f32> {
    let flags = u32(in.state.z * 255.0 + 0.5);
    let dead = (flags & FLAG_DEAD) != 0u;
    let code = u32(in.body.x * 255.0 + 0.5);
    let own = code & 15u;
    let toward = code >> 4u;
    let t = in.wound.x;
    var look: vec4<f32>;
    if toward != 0u && t > 0.0 {
        look = link_look(part_in(in), min(own, toward), max(own, toward), select(1.0 - t, t, own < toward));
    } else {
        look = part_look(in, own);
    }
    var col = look.rgb;
    var alpha = look.a;

    // Starving organisms fade; wounded ones redden (like the body).
    col *= 0.55 + 0.45 * smoothstep(0.0, 0.35, in.state.x);
    if dead {
        let grey = dot(col, vec3<f32>(0.3, 0.5, 0.2));
        col = mix(mix(vec3<f32>(grey), col, 0.2) * 0.6, col, in.motion.y);
        alpha *= 0.85 * (1.0 - in.motion.x);
    }
    // Hurt: one quick, faint flash.
    let hurt = in.motion.w;
    col = mix(col, col * vec3<f32>(1.35, 0.75, 0.7), hurt * hurt * hurt * 0.2);
    return vec4<f32>(col, alpha);
}

// What a mouth does to the body: darken the gullet, a pale fold along its rim, a small notch in
// the outline, plus things drawn on top (cilia, grasping hooks, proboscis).
struct Mouth {
    dark: f32,
    cut: f32,
    lip: f32,
    top: vec4<f32>,
};

// Mouths grow continuously with hunting, or with scavenging:
// hunters: cytostome (a small dark pore) -> an oral groove, a funnel into the cell lined with
// cilia beating inward, that widens and ripples while swallowing; jaws at the mouth add hooked
// grasping spines at its corners. (Mandibles are body parts, drawn separately.)
// scavengers: siphon pore -> proboscis ringed with beating cilia.
fn mouth(q: vec2<f32>, dq: f32, angle: f32, hunt: f32, jaws: f32, scav: f32, eating: f32, base: vec3<f32>, seed: f32, time: f32, aa: f32) -> Mouth {
    var m: Mouth;
    m.dark = 0.0;
    m.cut = 0.0;
    m.lip = 0.0;
    m.top = vec4<f32>(0.0);
    let gulp = eating * (0.5 + 0.5 * sin(time * 9.0));
    if hunt > 0.04 {
        // Cytostome: a small dark oval at the front edge, until the groove takes over.
        let pore = smoothstep(0.04, 0.12, hunt) * (1.0 - smoothstep(0.25, 0.4, hunt));
        let oval = length((q - vec2<f32>(0.82, 0.0)) / vec2<f32>(0.1 + 0.15 * hunt, 0.07 + 0.2 * hunt));
        m.dark = max(m.dark, (1.0 - smoothstep(0.8, 1.0, oval)) * pore);

        // Oral groove: a funnel from the front edge into the cell, narrowing inward.
        let open_w = smoothstep(0.25, 0.4, hunt);
        let half = (0.08 + 0.26 * hunt) * (1.0 + 0.45 * gulp) * open_w;
        let depth = 0.3 + 0.3 * hunt;
        let u = 1.0 - q.x;                            // depth from the front
        let taper = clamp(1.0 - u / depth, 0.0, 1.0);
        let w = half * sqrt(taper);
        let e = w - abs(q.y);                         // > 0 inside the funnel
        let inside = smoothstep(-aa, aa, e) * step(0.0, u) * step(u, depth) * open_w;
        // The gullet darkens inward and ripples while swallowing.
        let ripple = eating * 0.35 * (0.5 + 0.5 * sin(u / depth * 14.0 + time * 12.0));
        m.dark = max(m.dark, inside * (0.85 + 0.15 * (1.0 - taper) - ripple));
        // A pale fold of membrane along the rim.
        m.lip = open_w * (1.0 - smoothstep(0.0, 0.035 + aa, abs(e + 0.03))) * step(0.0, u) * step(u, depth * 0.9);

        // Cilia: rooted along both walls of the groove and reaching into it, slanted back down
        // the throat and beating in waves, sweeping food in.
        let cilia_w = open_w * smoothstep(0.3, 0.5, hunt);
        if cilia_w > 0.0 {
            let inward = e;                            // distance in from the wall
            let reach = min(0.06 + 0.05 * hunt, w * 0.85);
            let beat = 0.5 + 0.35 * sin(time * (10.0 + 10.0 * eating) - u * 30.0 + seed * 9.0);
            let lean = fract((u - inward * beat) * 18.0);
            let hair = step(0.004, inward) * step(inward, reach) * step(lean, 0.2)
                * step(0.01, u) * step(u, depth * 0.85) * cilia_w;
            if hair > m.top.a {
                m.top = vec4<f32>(base * 0.6 + vec3<f32>(0.35, 0.33, 0.3), hair * 0.6);
            }
        }

        // Jaws at the mouth: hooked chitin spines at each corner, snapping shut while eating.
        let hooks_w = smoothstep(0.03, 0.12, jaws) * open_w;
        if hooks_w > 0.0 {
            let pairs = 1 + i32(jaws > 0.55);
            let close = 0.35 + 0.65 * (1.0 - eating * abs(sin(time * 14.0)));
            for (var k = 0; k < pairs; k++) {
                let back_k = f32(k) * 0.1;
                for (var side = -1.0; side <= 1.0; side += 2.0) {
                    let corner = vec2<f32>(1.0 - back_k - 0.02, side * (half * sqrt(max(1.0 - back_k / depth, 0.0)) + 0.02));
                    let reach = 0.1 + 0.14 * jaws;
                    let mid = corner + vec2<f32>(reach * 0.55, -side * reach * 0.35 * close);
                    let tip = corner + vec2<f32>(reach * 0.6, -side * reach * (1.0 - 0.1 * close)) * vec2<f32>(close, 1.0);
                    let s1 = seg(q, corner, mid);
                    let s2 = seg(q, mid, tip);
                    let thick1 = mix(0.035, 0.025, s1.y) * (0.8 + 0.4 * jaws);
                    let thick2 = mix(0.025, 0.004, s2.y) * (0.8 + 0.4 * jaws);
                    let a = max(fill(s1.x, thick1, aa), fill(s2.x, thick2, aa)) * hooks_w;
                    if a > m.top.a {
                        let along = select(0.5 + 0.5 * s2.y, 0.5 * s1.y, s1.x < s2.x);
                        let chitin = mix(vec3<f32>(0.75, 0.55, 0.25), vec3<f32>(0.25, 0.12, 0.05), along);
                        m.top = vec4<f32>(chitin, a);
                    }
                }
            }
        }
    }
    // Scavengers (not already hunters): a siphon pore that grows into a proboscis with cilia.
    if scav > 0.12 && hunt < 0.3 {
        let pore = smoothstep(0.12, 0.2, scav);
        let len = 0.6 * smoothstep(0.35, 1.0, scav);
        let r = 0.06 + 0.07 * scav;
        let tip = vec2<f32>(0.9 + len, 0.0);
        let tube = seg(q, vec2<f32>(0.75, 0.0), tip);
        let tube_a = (1.0 - smoothstep(r - aa, r + aa, tube.x)) * pore * step(0.02, len);
        if tube_a > m.top.a {
            let ring = 0.85 + 0.15 * sin(tube.y * 40.0);
            let outline = smoothstep(r * 0.55, r, tube.x);
            let tube_col = mix(vec3<f32>(0.85, 0.7, 0.55), vec3<f32>(0.25, 0.15, 0.1), outline) * ring;
            m.top = vec4<f32>(tube_col, tube_a);
        }
        // Dark opening at the tip.
        let hole = (1.0 - smoothstep(r * 0.55, r * 0.7 + aa, length(q - tip))) * pore;
        m.top = mix(m.top, vec4<f32>(vec3<f32>(0.06, 0.03, 0.02), 1.0), hole);
        m.dark = max(m.dark, hole);
        // Cilia beating around the opening.
        let cilia_w = smoothstep(0.35, 0.5, scav);
        for (var k = 0; k < 5; k++) {
            let ang = (f32(k) - 2.0) * 0.45 + 0.15 * sin(time * 12.0 + f32(k) * 1.3 + seed * 5.0);
            let dir = vec2<f32>(cos(ang), sin(ang));
            let h = seg(q, tip + dir * r, tip + dir * (r + 0.15 + 0.15 * scav));
            let a = (1.0 - smoothstep(0.015 + aa * 0.5, 0.025 + aa, h.x)) * cilia_w;
            if a > m.top.a {
                m.top = vec4<f32>(vec3<f32>(0.9, 0.8, 0.65), a * 0.9);
            }
        }
    }
    return m;
}

// Body shape constants (keep in sync with `parts.rs`).
const SEGMENT_LENGTH: f32 = 0.5;
const SEGMENT_OVERLAP: f32 = 1.12;
const HEAD_WIDTH: f32 = 1.08;

struct Body {
    hl: f32,       // half-length
    hw: f32,       // half-width at the front
    taper: f32,    // tail narrowing
    segments: u32,
};

fn body_shape(elongation: f32, segments: u32) -> Body {
    var b: Body;
    let n = clamp(segments, 1u, 6u);
    let aspect = 1.0 + 3.0 * elongation;
    b.hl = sqrt(aspect) * (1.0 + SEGMENT_LENGTH * f32(n - 1u));
    b.hw = 1.0 / sqrt(aspect);
    b.taper = 0.3 * elongation + select(0.0, 0.15, n > 1u);
    b.segments = n;
    return b;
}

// Half-width of the body at x (narrower toward the tail).
fn body_width(b: Body, x: f32) -> f32 {
    let xc = clamp(x, -b.hl, b.hl);
    return b.hw * (1.0 - b.taper * (b.hl - xc) / (2.0 * b.hl));
}

// Normalized distance to the outline (x; < 1 inside) at q0 (radius units), and how close q0
// is to a crease between two segments (y, 0..1).
fn segment_distance(b: Body, q0: vec2<f32>) -> vec2<f32> {
    if b.segments <= 1u {
        return vec2<f32>(length(q0 / vec2<f32>(b.hl, body_width(b, q0.x))), 0.0);
    }
    let n = f32(b.segments);
    let half = b.hl / n;
    var d1 = 1e3;
    var d2 = 1e3;
    for (var k = 0u; k < 6u; k++) {
        if k >= b.segments {
            break;
        }
        let c = b.hl - (2.0 * f32(k) + 1.0) * half;
        let w = body_width(b, c) * select(1.0, HEAD_WIDTH, k == 0u);
        let dk = length((q0 - vec2<f32>(c, 0.0)) / vec2<f32>(half * SEGMENT_OVERLAP, w));
        if dk < d1 {
            d2 = d1;
            d1 = dk;
        } else if dk < d2 {
            d2 = dk;
        }
    }
    let crease = (1.0 - smoothstep(0.0, 0.2, d2 - d1)) * step(d1, 1.0);
    return vec2<f32>(d1, crease);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let flags = u32(in.state.z * 255.0 + 0.5);
    if (flags & FLAG_FX) != 0u {
        return effect(in);
    }
    if (flags & FLAG_PART) != 0u {
        return part(in);
    }
    let time = params.x;
    let radius_px = in.shape.z;
    let seed = in.shape.w;
    let dead = (flags & FLAG_DEAD) != 0u;
    let selected = (flags & FLAG_SELECTED) != 0u;
    let highlight = (flags & FLAG_HIGHLIGHT) != 0u;
    let sick = (flags & FLAG_SICK) != 0u;
    let eating = in.motion.z;
    let hurt = in.motion.w;

    // Body shape (matches `parts::BodyShape`): an ellipse stretched by elongation and tapered
    // toward the tail; each extra segment adds length, and segments are drawn as beads.
    let bs = body_shape(in.body.x, u32(in.body.y * 6.0 + 0.5));
    let q0 = in.uv * in.shape.xy;        // radius units, organism frame (x = forward)
    let p = q0 / vec2<f32>(bs.hl, body_width(bs, q0.x)); // whole body = unit circle
    let d = length(p);
    let aa = 1.2 / max(radius_px * body_width(bs, -bs.hl), 0.3); // ~one pixel, normalized
    let base = in.tint.rgb;

    // Dividing off its parent (drawn underneath): this newborn shows only on its own side of
    // where the two cells meet, blending into the parent until new walls form there.
    // `side` > 0 is the parent's side (radius units).
    let dividing = (flags & FLAG_DIVIDING) != 0u;
    var side = -1e3;
    var walls = 0.0;
    var cut = 1.0;
    if dividing {
        let toward = in.wound.y * TAU;
        side = dot(q0, vec2<f32>(cos(toward), sin(toward))) - (in.wound.z - 0.5) * 4.0;
        walls = in.wound.w;
        // (Solid up to the meeting line, so it covers the parent's outline there, and fading
        // into the parent past it while they're still one cell.)
        let feather = mix(0.35, 0.0, walls) + aa;
        cut = 1.0 - smoothstep(0.0, feather, side);
    }

    // Selection ring and species highlight (drawn even for tiny organisms).
    var overlay = vec4<f32>(0.0);
    if selected {
        let ring_r = select(1.25, 1.0 + 6.0 * aa, radius_px < 6.0);
        let pulse = 0.6 + 0.4 * sin(time * 5.0);
        let ring = 1.0 - smoothstep(0.0, 1.5 * aa + 0.04, abs(d - ring_r));
        overlay = vec4<f32>(vec3<f32>(1.0, 0.95, 0.6) * 3.0 * pulse, ring);
    } else if highlight {
        let halo = exp(-max(d - 1.0, 0.0) * 4.0) * (1.0 - smoothstep(1.2, 1.35, d)) * smoothstep(0.95, 1.1, d);
        overlay = vec4<f32>(vec3<f32>(0.9, 0.9, 1.0) * 1.5, halo * 0.5);
    }

    // Far away: just a soft dot.
    if radius_px < 2.5 {
        let dot_r = max(1.0, 1.1 / max(radius_px, 0.01) * 0.9);
        let a = 1.0 - smoothstep(dot_r - aa, dot_r + aa, d);
        var col = base;
        if dead {
            // Just-dead bodies keep their colour briefly; everything dead can fade out.
            col = mix(vec3<f32>(dot(base, vec3<f32>(0.3, 0.5, 0.2))) * 0.45, base, in.motion.y);
        }
        col += vec3<f32>(0.1, 0.9, 1.0) * in.diet.y * 0.6 * f32(!dead); // chemotrophs glow
        col += vec3<f32>(0.4, 1.2, 1.0) * in.chem.w * 1.5 * f32(!dead); // lures glow
        col = mix(col, col * vec3<f32>(1.4, 0.7, 0.65), hurt * hurt * hurt * 0.2);
        let body = vec4<f32>(col, a * cut * select(0.95, 0.6, dead) * (1.0 - select(0.0, in.motion.x, dead)));
        return mix(body, vec4<f32>(overlay.rgb, 1.0), overlay.a * (1.0 - body.a * 0.5));
    }

    let jaws = in.body.z;
    let pattern = u32(in.body.w * 255.0 + 0.5);
    var q = p;

    // Carcasses are torn in two along a jagged line; bites come out of the edge.
    var torn_away = 0.0;
    var fresh_wound = 0.0;
    if dead {
        let consumed = in.diet.x;
        let tear_angle = seed * TAU;
        let n = vec2<f32>(cos(tear_angle), sin(tear_angle));
        let perp = vec2<f32>(-n.y, n.x);
        let zig = (abs(fract(dot(q, perp) * 3.5 + seed * 7.0) - 0.5) - 0.25) * 0.28;
        let side = dot(q, n) - zig;
        let gap = (0.1 + 0.1 * consumed) * (1.0 - in.motion.y);
        if abs(side) < gap * 0.5 {
            torn_away = 1.0;
        }
        // Each half drifts apart a little.
        q = q - n * sign(side) * gap * 0.5;
        // Bite marks grow with how much has been eaten.
        let bites = i32(floor(consumed * 9.0));
        for (var k = 0; k < bites; k++) {
            let ang = seed * 13.0 + f32(k) * 2.39996;
            let c = vec2<f32>(cos(ang), sin(ang)) * 1.0;
            let br = 0.28 + 0.12 * hash21(vec2<f32>(seed, f32(k)));
            let bd = length(q - c);
            if bd < br {
                torn_away = 1.0;
            }
            fresh_wound = max(fresh_wound, 1.0 - smoothstep(br, br + 0.12, bd));
        }
    }
    let dq = length(q);
    let angle = atan2(q.y, q.x);
    // Distance to the outline (1 = edge), bead by bead for segmented bodies, and a crease
    // where neighboring segments meet.
    let back = vec2<f32>(q.x * bs.hl, q.y * body_width(bs, q.x * bs.hl));
    let beads = segment_distance(bs, back);
    let db = beads.x;
    let crease = beads.y;

    // Membrane undulation (still for the dead).
    let wobble_amp = select(0.035, 0.05, dead);
    let wobble_t = select(time * 1.3, 0.0, dead);
    let edge = 1.0 + wobble_amp * sin(angle * 5.0 + wobble_t + seed * 30.0);
    var alpha = 1.0 - smoothstep(edge - aa, edge + aa, db);

    // Interior shading: lit center, darker rim (per segment).
    var col = base * mix(1.25, 0.75, smoothstep(0.0, 1.0, db));

    // Pattern (cosmetic).
    let style = pattern >> 6u;
    let scale = 2.0 + f32(pattern & 63u) / 10.0;
    if style == 1u {
        col = mix(col, col * 0.55, dots(q0 * scale, 0.35, seed * 13.0) * 0.8);
    } else if style == 2u {
        let s = smoothstep(-0.2, 0.2, sin(q0.x * scale * 3.0));
        col = mix(col, col * 0.6, s * 0.7);
    } else if style == 3u {
        let s = smoothstep(-0.2, 0.2, sin(dq * scale * 4.0));
        col = mix(col, col * 0.6, s * 0.6);
    }
    // Segment boundaries.
    col *= 1.0 - 0.45 * crease;

    var mouth_top = vec4<f32>(0.0);
    var mouth_dark = 0.0;
    if !dead {
        let photo = in.diet.x;
        let chemo = in.diet.y;
        let scav = in.diet.z;
        let hunt = in.diet.w;
        // Chloroplasts: green when they release oxygen, purple when they use minerals instead.
        if photo > 0.05 {
            let chloro = dots(q0 * 5.0 + vec2<f32>(seed * 10.0), 0.28, seed * 7.0) * step(dq, 0.85);
            let pigment = mix(vec3<f32>(0.4, 0.06, 0.36), vec3<f32>(0.08, 0.45, 0.06), in.chem.x);
            col = mix(col, pigment, chloro * photo * 0.7);
        }
        // Nitrogen fixers: a few large pale cells.
        if in.chem.z > 0.05 {
            let cyst = dots(q0 * 2.2 + vec2<f32>(seed * 3.0), 0.35, seed * 11.0) * step(dq, 0.8);
            col = mix(col, vec3<f32>(0.85, 0.85, 0.6), cyst * in.chem.z * 0.8);
        }
        if scav > 0.05 {
            let granule = dots(q0 * 7.0 - vec2<f32>(seed * 5.0), 0.2, seed * 3.0) * step(dq, 0.9);
            col = mix(col, vec3<f32>(0.25, 0.13, 0.04), granule * scav * 0.75);
        }
        // Chemotrophs glow from within (blooms).
        col += vec3<f32>(0.15, 0.9, 1.1) * chemo * 1.6 * exp(-dq * dq * 5.0);

        // Mouth (see `mouth`): darkened interior, carved opening, lips.
        let mo = mouth(q, dq, angle, hunt, jaws, scav, eating, base, seed, time, aa);
        col = mix(col, base * 0.06 + vec3<f32>(0.03, 0.01, 0.03), mo.dark);
        col = mix(col, base * 1.35 + vec3<f32>(0.08), mo.lip * 0.7);
        alpha *= 1.0 - mo.cut;
        mouth_top = mo.top;
        mouth_dark = mo.dark;

        // Starving organisms fade; wounded ones redden.
        let energy = in.state.x;
        let health = in.state.y;
        col *= 0.55 + 0.45 * smoothstep(0.0, 0.35, energy);
        col = mix(col, col * vec3<f32>(1.3, 0.5, 0.5), (1.0 - health) * 0.6);

        // Plague: sickly yellow-green blotches that pulse.
        if sick {
            let blotch = dots(q0 * 3.0 + vec2<f32>(seed * 4.0), 0.4, seed * 5.0);
            let pulse = 0.6 + 0.4 * sin(time * 4.0 + seed * 10.0);
            col = mix(col, vec3<f32>(0.75, 0.8, 0.1) * pulse, blotch * 0.8);
        }
    }

    // Nucleus (in the head segment of segmented bodies).
    var nq = q;
    if bs.segments > 1u {
        let half = bs.hl / f32(bs.segments);
        let head_w = body_width(bs, bs.hl - half) * HEAD_WIDTH;
        nq = (back - vec2<f32>(bs.hl - half, 0.0)) / vec2<f32>(half * SEGMENT_OVERLAP, head_w);
    }
    let nucleus_c = vec2<f32>(-0.15, 0.1 * sin(seed * 20.0));
    let nucleus = 1.0 - smoothstep(0.24, 0.3, length(nq - nucleus_c));
    col = mix(col, col * 0.45 + vec3<f32>(0.03, 0.02, 0.05), nucleus * 0.8);

    // Membrane rim light.
    col += base * 0.5 * smoothstep(edge - 0.18, edge - 0.02, db) * (1.0 - mouth_dark);

    if dividing {
        // The new walls across the waist: this cell's on its side, the parent's just past it,
        // with a dark seam between them.
        let inside = 1.0 - smoothstep(edge - aa, edge + aa, db);
        let rim = 1.0 - smoothstep(0.02, 0.16, abs(side));
        let seam = 1.0 - smoothstep(0.0, 0.025 + aa, abs(side));
        col += base * 0.5 * rim * walls * step(side, 0.0);
        alpha *= cut;
        if side > 0.0 {
            col = mix(col, base * (0.75 + 0.5 * rim), walls);
            alpha = max(alpha, walls * rim * inside);
        }
        col *= 1.0 - 0.65 * seam * walls;
        alpha = max(alpha, seam * walls * inside * 0.9);
    }

    if !dead {
        // Detail fades in once the organism is big enough on screen to show it.
        let detail = smoothstep(5.0, 12.0, radius_px);
        col = mix(col, mouth_top.rgb, mouth_top.a * detail);
        alpha = max(alpha, mouth_top.a * detail);
        // Toxic organisms shimmer with a faint red haze.
        let toxin = in.chem.y;
        if toxin > 0.05 {
            let haze = exp(-max(dq - 1.0, 0.0) * 6.0) * smoothstep(0.9, 1.05, dq) * (1.0 - smoothstep(1.2, 1.35, dq));
            let pulse = 0.6 + 0.4 * sin(time * 2.5 + seed * 20.0 + angle * 3.0);
            let haze_a = haze * toxin * 0.6 * pulse;
            col = mix(col, vec3<f32>(1.6, 0.25, 0.1), haze_a * (1.0 - alpha));
            alpha = max(alpha, haze_a);
        }
    }

    if dead {
        // A grey, lifeless version of the organism with raw edges where it was torn or bitten.
        // motion.y: how freshly dead (colour drains to grey); motion.x: fading away.
        let grey = dot(col, vec3<f32>(0.3, 0.5, 0.2));
        let corpse = mix(vec3<f32>(grey), col, 0.2) * 0.6 * (0.85 + 0.15 * hash21(floor(q * 7.0) + seed));
        col = mix(corpse, col, in.motion.y);
        col = mix(col, vec3<f32>(0.35, 0.08, 0.06), fresh_wound * 0.6);
        alpha *= 1.0 - torn_away;
        alpha *= 0.92 * (1.0 - in.motion.x);
    } else {
        // Feeding: specks of food swirling in (no glow).
        if eating > 0.01 {
            var specks = 0.0;
            for (var k = 0; k < 6; k++) {
                let phase = fract(time * 1.2 + f32(k) / 6.0);
                let ang = seed * 6.0 + f32(k) * 1.047 + phase * 2.0;
                let c = vec2<f32>(cos(ang), sin(ang)) * (1.3 - phase * 0.4);
                specks = max(specks, dot_at(q, c, 0.07 + aa * 0.5, aa) * (1.0 - phase));
            }
            let speck_col = vec3<f32>(0.7, 0.55, 0.35);
            col = mix(col, speck_col, specks * eating * (1.0 - alpha));
            alpha = max(alpha, specks * eating);
        }
    }

    // Being bitten (or a carcass being eaten): one quick flash, a reddened wound on the side the
    // bite came from, and a few fine bits drifting away from it.
    if hurt > 0.01 {
        let wa = in.wound.x * TAU;
        let wdir = vec2<f32>(cos(wa), sin(wa));
        let flash = hurt * hurt * hurt;
        col = mix(col, vec3<f32>(1.3, 0.85, 0.8), flash * 0.15 * alpha);
        let wound_at = wdir * 0.92;
        let near = 1.0 - smoothstep(0.1, 0.45, length(q - wound_at));
        col = mix(col, col * vec3<f32>(1.2, 0.55, 0.5), hurt * 0.5 * near * alpha);
        // The bits, in radius units (round even on long bodies).
        let start = vec2<f32>(wound_at.x * bs.hl, wound_at.y * body_width(bs, wound_at.x * bs.hl));
        let aa0 = 1.2 / max(radius_px, 0.3);
        let out = 1.0 - hurt; // 0 when bitten, 1 when the effect is over
        var bits = 0.0;
        for (var k = 0; k < 6; k++) {
            let h1 = hash21(vec2<f32>(seed * 7.0, f32(k)));
            let h2 = hash21(vec2<f32>(f32(k) + 3.0, seed * 5.0));
            let spread = (h1 - 0.5) * 1.2;
            let d = vec2<f32>(cos(wa + spread), sin(wa + spread));
            let at = start + d * (0.06 + (0.35 + 0.6 * h2) * out);
            bits = max(bits, dot_at(q0, at, 0.025 + 0.015 * hurt + aa0 * 0.5, aa0));
        }
        let bits_col = mix(base * 0.7, vec3<f32>(0.5, 0.1, 0.07), 0.4);
        col = mix(col, bits_col, bits * (1.0 - alpha * 0.5));
        alpha = max(alpha, bits * hurt);
    }

    let body = vec4<f32>(col, alpha);
    return vec4<f32>(mix(body.rgb, overlay.rgb, overlay.a * (1.0 - body.a * 0.3)), max(body.a, overlay.a));
}
