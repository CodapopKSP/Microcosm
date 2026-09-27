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
    @location(4) body: vec4<f32>,   // body: elongation, segments/6, jaws, pattern; part: kind, size, mirror|index, seed
    @location(5) diet: vec4<f32>,   // photo, chemo, scavenge, hunt (carcass: consumed)
    @location(6) state: vec4<f32>,  // energy, health, flags, maturity
    @location(7) motion: vec4<f32>, // moving, walking, eating flash, hurt flash
    @location(8) chem: vec4<f32>,   // oxygenic, toxin, fixation, lure
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
    } else if kind == 5u {
        // Budding: the neck between parent and bud, narrowing in the middle until it pinches off.
        let q = in.uv * vec2<f32>(in.shape.x, 1.1); // width units
        let len = max(in.shape.x - 1.0, 0.01);
        let along = clamp(abs(q.x) / len, 0.0, 1.0);
        let pinch = 1.0 - smoothstep(0.35, 0.8, age);
        let hw = mix(0.9 * pinch, 0.9, along * along);
        let cap = length(vec2<f32>(max(abs(q.x) - len, 0.0), q.y));
        let dist = select(abs(q.y), cap, abs(q.x) > len);
        a = (1.0 - smoothstep(hw - aa, hw + aa, dist)) * step(0.02, hw) * 0.95;
        col = in.tint.rgb * mix(1.1, 0.8, abs(q.y) / max(hw, 1e-3));
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

// A body part. Every kind has its own look, and grows more elaborate with its size.
fn part(in: VertexOutput) -> vec4<f32> {
    let flags = u32(in.state.z * 255.0 + 0.5);
    let dead = (flags & FLAG_DEAD) != 0u;
    let kind = u32(in.body.x * 255.0 + 0.5);
    let s = in.body.y;
    let mi = u32(in.body.z * 255.0 + 0.5);
    let mirror = select(1.0, -1.0, (mi & 1u) == 1u);
    let index = f32(mi >> 1u);
    let seed = in.body.w;
    let lp = in.uv * in.shape.xy;
    let a = lp.x + 0.5;               // 0 at the base, 1 at the tip
    let c = lp.y * mirror;            // across (mirrored copies curl the other way)
    let q = vec2<f32>(a, c);
    let w = in.shape.z;               // nominal half width
    let len_px = in.shape.w;
    let aa = 1.0 / max(len_px, 0.5); // about one pixel
    let time = select(params.x, 0.0, dead);
    let activity = select(in.motion.x, 0.0, dead);
    let walking = select(in.motion.y, 0.0, dead);
    let eating = in.motion.z;
    let phase = seed * 40.0 + index * 1.9;
    let base = in.tint.rgb;
    let inside = step(0.0, a) * step(a, 1.0);
    // Fine detail only once there are enough pixels to show it.
    let detail = smoothstep(6.0, 16.0, len_px);
    var col = base;
    var alpha = 0.0;

    if kind == 1u {
        // Flagellum: a beating whip; bigger ones get a basal body, then a fringe of fine hairs.
        let amp = (0.08 + 0.08 * s) * (0.4 + 0.6 * min(1.0, activity * 2.0 + 0.3));
        let y = amp * sin(a * TAU * (1.0 + s) - time * (6.0 + 16.0 * activity) + phase) * a;
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
        let sway = 0.16 * sin(a * 3.0 - time * 1.6 + phase) * a;
        let curl = (0.18 + 0.12 * sin(time * 0.45 + phase)) * a * a * a;
        let y = sway + curl;
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
        let y = 0.07 * sin(time * 0.8 + phase + a * 2.0) * a * a;
        var hw = w * pow(sin(PI * clamp((a - 0.08) / 0.92, 0.0, 1.0)), 0.7);
        hw *= 1.0 - 0.15 * smoothstep(0.65, 0.85, s) * abs(sin(a * 45.0));
        hw = max(hw, w * 0.14 * step(a, 0.2));
        let dist = abs(c - y);
        alpha = fill(dist, hw + aa * 0.3, aa) * inside;
        let pigment = mix(vec3<f32>(0.42, 0.1, 0.38), vec3<f32>(0.1, 0.45, 0.08), in.chem.x);
        col = mix(base, pigment, 0.55 + 0.35 * in.diet.x) * mix(1.15, 0.8, dist / max(hw, 1e-3));
        col = mix(col, col * 0.6, fill(dist, 0.015 + aa * 0.5, aa) * smoothstep(0.2, 0.35, s));
        let v = fract(a * 9.0 - dist / max(w, 1e-3) * 1.2);
        let vein = 1.0 - smoothstep(0.0, 0.08, min(v, 1.0 - v));
        col *= 1.0 - 0.25 * vein * smoothstep(0.35, 0.5, s) * detail;
        let bladder = fill(length(q - vec2<f32>(0.14, y)), w * 0.55, aa) * smoothstep(0.8, 0.92, s);
        col = mix(col, vec3<f32>(0.75, 0.65, 0.3), bladder);
        alpha = max(alpha, bladder);
    } else if kind == 5u {
        // Fin: a fan-shaped membrane that flaps; rays appear, then a scalloped edge between them.
        let flap = 0.25 * sin(time * (2.5 + 7.0 * activity) + phase);
        let fq = vec2<f32>(a, c - flap * a);
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
        let beat = 0.25 * sin(time * (2.0 + 6.0 * activity) + phase);
        let cc = (c - beat * a) / max(w, 1e-3);
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
        let y = 0.06 * sin(time * 0.7 + phase) * a * a;
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
        let y = (0.12 * sin(time * 2.2 + phase) + 0.18) * a * a;
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
        let sway = 0.15 * sin(time * 0.9 + phase);
        let y = (sway + 0.1) * a * a;
        let stem = fill(abs(c - y), w * 0.22 + aa * 0.3, aa) * step(0.0, a) * step(a, 0.78);
        let bulb_c = vec2<f32>(0.8, (sway + 0.1) * 0.64);
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
        let y = 0.08 * smoothstep(0.5, 1.0, s) * a * a;
        let barb = smoothstep(0.75, 0.9, s) * max(0.0, 1.0 - fract(a * 5.0) * 3.0) * step(0.25, a);
        let hw = w * (1.0 - a) + w * 0.5 * barb * (1.0 - a);
        alpha = fill(abs(c - y), hw + aa * 0.3, aa) * inside;
        col = mix(base * 0.4, vec3<f32>(0.9, 0.85, 0.72), 0.55 + 0.3 * a);
        let band = 1.0 - smoothstep(0.15, 0.25, abs(fract(a * 6.0) - 0.5));
        col *= 1.0 - 0.3 * band * smoothstep(0.4, 0.55, s) * detail;
    } else if kind == 14u {
        // Jaw: a mandible that bows out and hooks in, snapping while eating; teeth on its inner
        // edge once it's large enough.
        let gape = eating * abs(sin(time * 14.0 + index)) * 0.2;
        let y = (0.12 + gape) * sin(PI * a) - (0.18 + 0.12 * s) * a * a * a;
        let hw = w * (1.0 - pow(a, 1.4) * 0.85);
        let dist = c - y;
        alpha = fill(abs(dist), hw + aa * 0.3, aa) * inside;
        let chitin = mix(vec3<f32>(0.2, 0.11, 0.05), vec3<f32>(0.85, 0.7, 0.5), a * a);
        col = mix(chitin, base * 0.5, 0.2) * mix(1.15, 0.8, abs(dist) / max(hw, 1e-3));
        let teeth_w = smoothstep(0.3, 0.45, s);
        if teeth_w > 0.0 {
            let count = 3.0 + floor(s * 5.0);
            let t = fract(a * count);
            let height = w * (0.5 + 0.5 * s) * (1.0 - abs(t - 0.5) * 2.0);
            let inward = -dist - hw;
            let tooth = step(-aa, inward) * step(inward, height) * step(0.15, a) * step(a, 0.95) * teeth_w;
            col = mix(col, vec3<f32>(1.2, 1.15, 1.0), tooth);
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

    // Starving organisms fade; wounded ones redden (like the body).
    col *= 0.55 + 0.45 * smoothstep(0.0, 0.35, in.state.x);
    if dead {
        let grey = dot(col, vec3<f32>(0.3, 0.5, 0.2));
        col = mix(mix(vec3<f32>(grey), col, 0.2) * 0.6, col, in.motion.y);
        alpha *= 0.85 * (1.0 - in.motion.x);
    }
    col = mix(col, vec3<f32>(2.2, 0.15, 0.08), in.motion.w * 0.6 * step(0.01, alpha));
    return vec4<f32>(col, alpha);
}

// What a mouth does to the body: darken the interior, carve the opening, lips, plus parts
// drawn on top (teeth, mandibles, proboscis, cilia).
struct Mouth {
    dark: f32,
    cut: f32,
    lip: f32,
    top: vec4<f32>,
};

// Mouths grow continuously with hunting (teeth with the jaws), or with scavenging:
// hunters: cytostome (small dark pore) -> open mouth with lips -> maw lined with teeth.
// (Mandibles are body parts, drawn separately.)
// scavengers: siphon pore -> proboscis ringed with beating cilia.
fn mouth(q: vec2<f32>, dq: f32, angle: f32, hunt: f32, jaws: f32, scav: f32, eating: f32, base: vec3<f32>, seed: f32, time: f32, aa: f32) -> Mouth {
    var m: Mouth;
    m.dark = 0.0;
    m.cut = 0.0;
    m.lip = 0.0;
    m.top = vec4<f32>(0.0);
    let gape = eating * abs(sin(time * 16.0));
    if hunt > 0.04 {
        // Cytostome: a small dark oval at the front edge.
        let pore = smoothstep(0.04, 0.12, hunt) * (1.0 - smoothstep(0.28, 0.38, hunt));
        let oval = length((q - vec2<f32>(0.8, 0.0)) / vec2<f32>(0.12 + 0.2 * hunt, 0.08 + 0.25 * hunt));
        m.dark = max(m.dark, (1.0 - smoothstep(0.8, 1.0, oval)) * pore);
        // Open mouth: a notch that widens with hunting (and while eating).
        let open_w = smoothstep(0.28, 0.38, hunt);
        let opening = (0.12 + 0.5 * hunt + 0.2 * gape) * open_w;
        let inside = step(abs(angle), opening) * open_w;
        m.dark = max(m.dark, inside * smoothstep(0.45, 0.7, dq));
        m.cut = inside * smoothstep(0.86, 0.96, dq);
        let edge_dist = abs(abs(angle) - opening) * dq;
        m.lip = open_w * (1.0 - smoothstep(0.04, 0.1 + aa, edge_dist)) * step(0.5, dq) * step(dq, 1.02);
        // Teeth along both jaw edges, more and longer with the Jaws gene.
        let teeth_w = smoothstep(0.5, 0.62, hunt);
        if teeth_w > 0.0 {
            let inward = (opening - abs(angle)) * dq;
            let count = 3.0 + floor(jaws * 5.0);
            let t = fract((dq - 0.58) / 0.42 * count);
            let height = (0.07 + 0.14 * jaws) * (1.0 - abs(t - 0.5) * 2.0);
            let tooth = teeth_w * step(0.0, inward) * step(inward, height) * step(0.58, dq) * step(dq, 0.99);
            if tooth > m.top.a {
                m.top = vec4<f32>(vec3<f32>(1.25, 1.2, 1.05), tooth);
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
        col = mix(col, vec3<f32>(2.0, 0.2, 0.1), hurt * 0.8);
        col += vec3<f32>(1.0, 0.8, 0.4) * eating * 0.6;
        let body = vec4<f32>(col, a * select(0.95, 0.6, dead) * (1.0 - select(0.0, in.motion.x, dead)));
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
        col = mix(col, vec3<f32>(0.07, 0.0, 0.01), mo.dark);
        col = mix(col, vec3<f32>(0.75, 0.25, 0.3), mo.lip * 0.8);
        alpha *= 1.0 - mo.cut;
        mouth_top = mo.top;

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
    col += base * 0.5 * smoothstep(edge - 0.18, edge - 0.02, db);

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
        // Feeding: a warm glow and specks of food swirling in.
        if eating > 0.01 {
            col += vec3<f32>(0.9, 0.6, 0.25) * eating * 0.2 * smoothstep(0.6, 1.0, dq) * alpha;
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

    // Being bitten: a red flash.
    col = mix(col, vec3<f32>(2.2, 0.15, 0.08), hurt * 0.7 * step(0.01, alpha));

    let body = vec4<f32>(col, alpha);
    return vec4<f32>(mix(body.rgb, overlay.rgb, overlay.a * (1.0 - body.a * 0.3)), max(body.a, overlay.a));
}
