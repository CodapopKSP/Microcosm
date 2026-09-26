// Procedural organism rendering. Each organism, carcass or effect is one quad; everything is
// drawn in the fragment shader in "radius units" (1.0 = the body's edge).

#import bevy_sprite::mesh2d_functions as mesh_functions

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: vec4<f32>; // time, world units per px

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) shape: vec4<f32>,  // quad half / radius, radius in px, heading (fx: kind), seed (fx: age)
    @location(3) tint: vec4<f32>,   // linear rgb
    @location(4) body: vec4<f32>,   // armor, spikes, jaws, pattern
    @location(5) diet: vec4<f32>,   // photo, chemo, scavenge, hunt (carcass: consumed)
    @location(6) state: vec4<f32>,  // energy, health, flags, maturity
    @location(7) motion: vec4<f32>, // speed, agility, eating flash, hurt flash
    @location(8) sense: vec4<f32>,  // sense range gene, field-of-view gene
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
    @location(7) sense: vec4<f32>,
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
    out.sense = v.sense;
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

// Flagella trailing behind a swimmer (q is in the organism's frame, x = forward).
fn flagella(q: vec2<f32>, speed: f32, agility: f32, seed: f32, time: f32, aa: f32) -> f32 {
    let count = 1 + i32(floor(agility * 3.99));
    let len = 0.5 + 1.6 * speed;
    let beat = 10.0 + 10.0 * speed;
    var a = 0.0;
    for (var k = 0; k < count; k++) {
        let spread = (f32(k) - f32(count - 1) * 0.5) * 0.55;
        let qq = rotate(q, -(PI + spread));
        let along = qq.x - 0.9;
        if along > 0.0 && along < len {
            let t = along / len;
            let wave = (0.1 + 0.08 * speed) * sin(along * 9.0 / len * 1.2 - time * beat + seed * 9.0 + f32(k) * 1.7) * t;
            let width = 0.075 * (1.0 - t * 0.75) + aa * 0.6;
            a = max(a, (1.0 - smoothstep(width * 0.5, width, abs(qq.y - wave))) * 0.85);
        }
    }
    return a;
}

// Distance from p to segment a-b, and the position along it (0..1).
fn seg(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return vec2<f32>(length(pa - ba * h), h);
}

// Eyes. Complexity grows continuously with the Range gene:
// eyespot (pigment dot) -> pit eye (dark cup) -> lens eye (iris and pupil) -> camera eye
// (striated iris, glint, rim). Field of view sets how many eyes and how far round the body.
fn eyes(q: vec2<f32>, range: f32, fov: f32, seed: f32, time: f32, aa: f32) -> vec4<f32> {
    if range < 0.03 {
        return vec4<f32>(0.0);
    }
    var n = 1;
    if fov > 0.25 {
        n = 2;
    }
    if fov > 0.6 {
        n = 4;
    }
    if fov > 0.88 {
        n = 6;
    }
    let spread = mix(0.55, 2.4, fov);
    let size = (0.08 + 0.24 * range) * select(1.0, 1.3, n == 1) * (1.1 - 0.08 * f32(n));
    let orbit = mix(0.58, 0.8, range);
    let blink = step(0.975, fract(time * 0.23 + seed * 7.13));
    let iris_col = mix(vec3<f32>(1.0, 0.62, 0.08), vec3<f32>(0.15, 0.55, 1.0), fract(seed * 3.7));
    let aa_e = aa / size;
    let l2 = smoothstep(0.2, 0.3, range); // pit eye
    let l3 = smoothstep(0.45, 0.55, range); // lens eye
    let l4 = smoothstep(0.72, 0.82, range); // camera eye
    var out = vec4<f32>(0.0);
    for (var k = 0; k < 6; k++) {
        if k >= n {
            break;
        }
        var ang = 0.55;
        if n > 1 {
            let npairs = f32(n / 2);
            let pair = f32(k / 2);
            let sgn = select(-1.0, 1.0, k % 2 == 0);
            ang = sgn * mix(0.55, spread, (pair + 0.5) / npairs);
        }
        let c = vec2<f32>(cos(ang), sin(ang)) * orbit;
        let e = (q - c) / size;
        let de = length(e);
        if de > 1.3 {
            continue;
        }
        let a = 1.0 - smoothstep(1.0 - aa_e, 1.0 + aa_e, de);
        // 1: eyespot.
        var col = vec3<f32>(0.55, 0.05, 0.03);
        // 2: pit eye.
        let pit = mix(vec3<f32>(0.01, 0.01, 0.02), vec3<f32>(0.95, 0.9, 0.8), smoothstep(0.5, 0.8, de));
        col = mix(col, pit, l2);
        // 3: lens eye, pupil looking forward.
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
        if a > out.a {
            out = vec4<f32>(col, a);
        }
    }
    return out;
}

// What a mouth does to the body: darken the interior, carve the opening, lips, plus parts
// drawn on top (teeth, mandibles, proboscis, cilia).
struct Mouth {
    dark: f32,
    cut: f32,
    lip: f32,
    top: vec4<f32>,
};

// Mouths grow continuously with hunting (and jaws), or with scavenging:
// hunters: cytostome (small dark pore) -> open mouth with lips -> maw lined with teeth -> mandibles.
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
        // Mandibles: curved pincers that snap while feeding.
        let mand_w = open_w * smoothstep(0.3, 0.5, jaws);
        if mand_w > 0.0 {
            let spread = 0.1 + 0.2 * (1.0 - gape);
            for (var k = 0; k < 2; k++) {
                let sgn = select(-1.0, 1.0, k == 1);
                let root = vec2<f32>(cos(opening + 0.08), sgn * sin(opening + 0.08)) * 0.93;
                let tip = vec2<f32>(1.0 + 0.5 * jaws, sgn * spread * 0.5);
                let mid = (root + tip) * 0.5 + vec2<f32>(0.05, sgn * (0.12 + 0.1 * jaws));
                let s1 = seg(q, root, mid);
                let s2 = seg(q, mid, tip);
                let along = select(0.5 + 0.5 * s2.y, 0.5 * s1.y, s1.x < s2.x);
                let dist = min(s1.x, s2.x);
                let width = (0.07 + 0.09 * jaws) * (1.0 - along * 0.75) + aa * 0.5;
                let a = (1.0 - smoothstep(width - aa, width, dist)) * mand_w;
                let chitin = mix(vec3<f32>(0.2, 0.1, 0.05), vec3<f32>(0.85, 0.7, 0.5), along * along) * (0.8 + 0.4 * smoothstep(width * 0.8, 0.0, dist));
                if a > m.top.a {
                    m.top = vec4<f32>(chitin, a);
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

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let flags = u32(in.state.z * 255.0 + 0.5);
    if (flags & FLAG_FX) != 0u {
        return effect(in);
    }
    let time = params.x;
    let size_ratio = in.shape.x;
    let radius_px = in.shape.y;
    let heading = in.shape.z;
    let seed = in.shape.w;
    let dead = (flags & FLAG_DEAD) != 0u;
    let selected = (flags & FLAG_SELECTED) != 0u;
    let highlight = (flags & FLAG_HIGHLIGHT) != 0u;
    let motile = (flags & FLAG_MOTILE) != 0u;
    let sick = (flags & FLAG_SICK) != 0u;
    let eating = in.motion.z;
    let hurt = in.motion.w;

    let p = in.uv * size_ratio;          // radius units
    let d = length(p);
    let aa = 1.2 / max(radius_px, 0.3);  // ~one pixel in radius units
    let base = in.tint.rgb;

    // Selection ring and species highlight (drawn even for tiny organisms).
    var overlay = vec4<f32>(0.0);
    if selected {
        let ring_r = select(1.35, 1.0 + 6.0 * aa, radius_px < 6.0);
        let pulse = 0.6 + 0.4 * sin(time * 5.0);
        let ring = 1.0 - smoothstep(0.0, 1.5 * aa + 0.04, abs(d - ring_r));
        overlay = vec4<f32>(vec3<f32>(1.0, 0.95, 0.6) * 3.0 * pulse, ring);
    } else if highlight {
        let halo = exp(-max(d - 1.0, 0.0) * 4.0) * (1.0 - smoothstep(1.0, size_ratio, d)) * smoothstep(0.95, 1.1, d);
        overlay = vec4<f32>(vec3<f32>(0.9, 0.9, 1.0) * 1.5, halo * 0.5);
    }

    // Far away: just a soft dot.
    if radius_px < 2.5 {
        let dot_r = max(1.0, 1.1 / max(radius_px, 0.01) * 0.9);
        let a = 1.0 - smoothstep(dot_r - aa, dot_r + aa, d);
        var col = base;
        if dead {
            col = vec3<f32>(dot(base, vec3<f32>(0.3, 0.5, 0.2))) * 0.45;
        }
        col += vec3<f32>(0.1, 0.9, 1.0) * in.diet.y * 0.6 * f32(!dead); // chemotrophs glow
        col = mix(col, vec3<f32>(2.0, 0.2, 0.1), hurt * 0.8);
        col += vec3<f32>(1.0, 0.8, 0.4) * eating * 0.6;
        let body = vec4<f32>(col, a * select(0.95, 0.6, dead));
        return mix(body, vec4<f32>(overlay.rgb, 1.0), overlay.a * (1.0 - body.a * 0.5));
    }

    let armor = in.body.x;
    let spikes = in.body.y;
    let jaws = in.body.z;
    let pattern = u32(in.body.w * 255.0 + 0.5);

    // Body in the organism's own frame (x = forward).
    var q = rotate(p, -heading);

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
        let gap = 0.1 + 0.1 * consumed;
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

    // Membrane undulation (still for the dead).
    let wobble_amp = select(0.035, 0.05, dead);
    let wobble_t = select(time * 1.3, 0.0, dead);
    let edge = 1.0 + wobble_amp * sin(angle * 5.0 + wobble_t + seed * 30.0);
    var alpha = 1.0 - smoothstep(edge - aa, edge + aa, dq);

    // Spikes stick out past the membrane.
    let n_spikes = 6.0 + floor(seed * 6.0);
    let spike_len = spikes * 0.5;
    let spike_shape = pow(max(0.0, cos(n_spikes * angle * 0.5 + seed * 6.28)), 12.0);
    let spike_edge = edge + spike_len * spike_shape;
    let spike_alpha = (1.0 - smoothstep(spike_edge - aa, spike_edge + aa, dq)) * step(0.02, spikes) * step(edge - aa, dq);

    // Flagella behind swimmers: longer when fast, more of them when agile.
    var tail_alpha = 0.0;
    if motile {
        tail_alpha = flagella(q, in.motion.x, in.motion.y, seed, time, aa);
    }

    // Interior shading: lit center, darker rim.
    var col = base * mix(1.25, 0.75, smoothstep(0.0, 1.0, dq));

    // Pattern (cosmetic).
    let style = pattern >> 6u;
    let scale = 2.0 + f32(pattern & 63u) / 10.0;
    if style == 1u {
        col = mix(col, col * 0.55, dots(q * scale, 0.35, seed * 13.0) * 0.8);
    } else if style == 2u {
        let s = smoothstep(-0.2, 0.2, sin(q.x * scale * 3.0));
        col = mix(col, col * 0.6, s * 0.7);
    } else if style == 3u {
        let s = smoothstep(-0.2, 0.2, sin(dq * scale * 4.0));
        col = mix(col, col * 0.6, s * 0.6);
    }

    var mouth_top = vec4<f32>(0.0);
    if !dead {
        let photo = in.diet.x;
        let chemo = in.diet.y;
        let scav = in.diet.z;
        let hunt = in.diet.w;
        // Diet markings.
        if photo > 0.05 {
            let chloro = dots(q * 5.0 + vec2<f32>(seed * 10.0), 0.28, seed * 7.0) * step(dq, 0.85);
            col = mix(col, vec3<f32>(0.08, 0.45, 0.06), chloro * photo * 0.7);
        }
        if scav > 0.05 {
            let granule = dots(q * 7.0 - vec2<f32>(seed * 5.0), 0.2, seed * 3.0) * step(dq, 0.9);
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
            let blotch = dots(q * 3.0 + vec2<f32>(seed * 4.0), 0.4, seed * 5.0);
            let pulse = 0.6 + 0.4 * sin(time * 4.0 + seed * 10.0);
            col = mix(col, vec3<f32>(0.75, 0.8, 0.1) * pulse, blotch * 0.8);
        }
    }

    // Nucleus.
    let nucleus_c = vec2<f32>(-0.15, 0.1 * sin(seed * 20.0));
    let nucleus = 1.0 - smoothstep(0.24, 0.3, length(q - nucleus_c));
    col = mix(col, col * 0.45 + vec3<f32>(0.03, 0.02, 0.05), nucleus * 0.8);

    // Armor shell near the edge.
    let shell = 0.06 + 0.22 * armor;
    let in_shell = smoothstep(edge - shell - aa, edge - shell + aa, dq) * armor;
    let shell_col = mix(vec3<f32>(0.22, 0.2, 0.17), base * 0.5, 0.35) * (0.8 + 0.4 * hash21(floor(q * 9.0)));
    col = mix(col, shell_col, clamp(in_shell * 1.5, 0.0, 1.0));

    // Membrane rim light.
    col += base * 0.5 * smoothstep(edge - 0.18, edge - 0.02, dq) * (1.0 - armor);

    // Spikes and tail.
    let spike_col = mix(base * 0.4, vec3<f32>(0.85, 0.8, 0.7), 0.5);
    col = mix(col, spike_col, spike_alpha * (1.0 - alpha));
    alpha = max(alpha, spike_alpha);
    let tail_col = base * 0.8 + vec3<f32>(0.05);
    col = mix(col, tail_col, tail_alpha * (1.0 - alpha));
    alpha = max(alpha, tail_alpha);

    if !dead {
        // Detail fades in once the organism is big enough on screen to show it.
        let detail = smoothstep(5.0, 12.0, radius_px);
        let ey = eyes(q, in.sense.x, in.sense.y, seed, time, aa);
        col = mix(col, ey.rgb, ey.a * detail);
        alpha = max(alpha, ey.a * detail);
        col = mix(col, mouth_top.rgb, mouth_top.a * detail);
        alpha = max(alpha, mouth_top.a * detail);
    }

    if dead {
        // A grey, lifeless version of the organism with raw edges where it was torn or bitten.
        let grey = dot(col, vec3<f32>(0.3, 0.5, 0.2));
        col = mix(vec3<f32>(grey), col, 0.2) * 0.6;
        col *= 0.85 + 0.15 * hash21(floor(q * 7.0) + seed);
        col = mix(col, vec3<f32>(0.35, 0.08, 0.06), fresh_wound * 0.6);
        alpha *= 1.0 - torn_away;
        alpha *= 0.92;
    } else {
        // Feeding: a warm glow and specks of food swirling in.
        if eating > 0.01 {
            col += vec3<f32>(0.9, 0.6, 0.25) * eating * 0.2 * smoothstep(0.6, 1.0, dq) * alpha;
            var specks = 0.0;
            for (var k = 0; k < 6; k++) {
                let phase = fract(time * 1.2 + f32(k) / 6.0);
                let ang = seed * 6.0 + f32(k) * 1.047 + phase * 2.0;
                let c = vec2<f32>(cos(ang), sin(ang)) * (1.7 - phase * 0.7);
                specks = max(specks, dot_at(p, c, 0.07 + aa * 0.5, aa) * (1.0 - phase));
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
