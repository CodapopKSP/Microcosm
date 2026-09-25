// Procedural organism rendering. Each organism is one quad; everything is drawn in the
// fragment shader in "radius units" (1.0 = the body's edge).

#import bevy_sprite::mesh2d_functions as mesh_functions

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: vec4<f32>; // time, world units per px

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) shape: vec4<f32>,  // quad half / radius, radius in px, heading, seed
    @location(3) tint: vec4<f32>,   // linear rgb
    @location(4) body: vec4<f32>,   // armor, spikes, jaws, pattern
    @location(5) diet: vec4<f32>,   // photo, chemo, scavenge, hunt
    @location(6) state: vec4<f32>,  // energy, health, flags, maturity
};

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) shape: vec4<f32>,
    @location(2) tint: vec4<f32>,
    @location(3) body: vec4<f32>,
    @location(4) diet: vec4<f32>,
    @location(5) state: vec4<f32>,
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
    return out;
}

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

// Soft cellular dots: returns ~1 inside dots of radius `size` on a jittered grid with spacing 1.
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

const FLAG_SELECTED: u32 = 1u;
const FLAG_HIGHLIGHT: u32 = 2u;
const FLAG_DEAD: u32 = 4u;
const FLAG_MOTILE: u32 = 8u;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let time = params.x;
    let size_ratio = in.shape.x;
    let radius_px = in.shape.y;
    let heading = in.shape.z;
    let seed = in.shape.w;
    let flags = u32(in.state.z * 255.0 + 0.5);
    let dead = (flags & FLAG_DEAD) != 0u;
    let selected = (flags & FLAG_SELECTED) != 0u;
    let highlight = (flags & FLAG_HIGHLIGHT) != 0u;
    let motile = (flags & FLAG_MOTILE) != 0u;

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
        let halo = exp(-max(d - 1.0, 0.0) * 4.0) * (1.0 - smoothstep(1.0, size_ratio, d));
        overlay = vec4<f32>(vec3<f32>(0.9, 0.9, 1.0) * 1.5, halo * 0.5);
    }

    // Far away: just a soft dot.
    if radius_px < 2.5 {
        let dot_r = max(1.0, 1.1 / max(radius_px, 0.01) * 0.9);
        let a = 1.0 - smoothstep(dot_r - aa, dot_r + aa, d);
        var col = base * select(1.0, 0.35, dead);
        col += vec3<f32>(0.1, 0.9, 1.0) * in.diet.y * 0.6; // chemotrophs glow
        let body = vec4<f32>(col, a * select(0.95, 0.5, dead));
        return mix(body, vec4<f32>(overlay.rgb, 1.0), overlay.a * (1.0 - body.a * 0.5));
    }

    if dead {
        let wobble = 0.08 * sin(atan2(p.y, p.x) * 7.0 + seed * 20.0);
        let a = 1.0 - smoothstep(1.0 + wobble - aa, 1.0 + wobble + aa, d);
        let grey = dot(base, vec3<f32>(0.3, 0.5, 0.2));
        var col = mix(vec3<f32>(grey), base, 0.3) * 0.35;
        col *= 0.8 + 0.2 * hash21(floor(p * 6.0) + seed);
        return vec4<f32>(col, a * 0.85);
    }

    let armor = in.body.x;
    let spikes = in.body.y;
    let jaws = in.body.z;
    let pattern = u32(in.body.w * 255.0 + 0.5);
    let photo = in.diet.x;
    let chemo = in.diet.y;
    let scav = in.diet.z;
    let hunt = in.diet.w;
    let energy = in.state.x;
    let health = in.state.y;

    // Body in the organism's own frame (x = forward).
    let q = rotate(p, -heading);
    let angle = atan2(q.y, q.x);

    // Membrane undulation.
    let edge = 1.0 + 0.035 * sin(angle * 5.0 + time * 1.3 + seed * 30.0);
    var alpha = 1.0 - smoothstep(edge - aa, edge + aa, d);

    // Spikes stick out past the membrane.
    let n_spikes = 6.0 + floor(seed * 6.0);
    let spike_len = spikes * 0.5;
    let spike_shape = pow(max(0.0, cos(n_spikes * angle * 0.5 + seed * 6.28)), 12.0);
    let spike_edge = edge + spike_len * spike_shape;
    let spike_alpha = (1.0 - smoothstep(spike_edge - aa, spike_edge + aa, d)) * step(0.02, spikes) * step(edge - aa, d);

    // Flagellum behind swimmers.
    var tail_alpha = 0.0;
    if motile {
        let tx = -q.x - 0.9;
        if tx > 0.0 && tx < 0.8 {
            let wave = 0.14 * sin(tx * 11.0 - time * 14.0 + seed * 9.0) * (tx / 0.8);
            let width = 0.07 * (1.0 - tx / 0.8) + aa;
            tail_alpha = (1.0 - smoothstep(width * 0.5, width, abs(q.y - wave))) * 0.8;
        }
    }

    // Interior shading: lit center, darker rim.
    var col = base * mix(1.25, 0.75, smoothstep(0.0, 1.0, d));

    // Pattern (cosmetic).
    let style = pattern >> 6u;
    let scale = 2.0 + f32(pattern & 63u) / 10.0;
    if style == 1u {
        col = mix(col, col * 0.55, dots(q * scale, 0.35, seed * 13.0) * 0.8);
    } else if style == 2u {
        let s = smoothstep(-0.2, 0.2, sin(q.x * scale * 3.0));
        col = mix(col, col * 0.6, s * 0.7);
    } else if style == 3u {
        let s = smoothstep(-0.2, 0.2, sin(d * scale * 4.0));
        col = mix(col, col * 0.6, s * 0.6);
    }

    // Diet markings.
    if photo > 0.05 {
        let chloro = dots(q * 5.0 + vec2<f32>(seed * 10.0), 0.28, seed * 7.0) * step(d, 0.85);
        col = mix(col, vec3<f32>(0.08, 0.45, 0.06), chloro * photo * 0.7);
    }
    if scav > 0.05 {
        let granule = dots(q * 7.0 - vec2<f32>(seed * 5.0), 0.2, seed * 3.0) * step(d, 0.9);
        col = mix(col, vec3<f32>(0.25, 0.13, 0.04), granule * scav * 0.75);
    }
    // Nucleus.
    let nucleus_c = vec2<f32>(-0.15, 0.1 * sin(seed * 20.0));
    let nucleus = 1.0 - smoothstep(0.24, 0.3, length(q - nucleus_c));
    col = mix(col, col * 0.45 + vec3<f32>(0.03, 0.02, 0.05), nucleus * 0.8);
    // Chemotrophs glow from within (blooms).
    col += vec3<f32>(0.15, 0.9, 1.1) * chemo * 1.6 * exp(-d * d * 5.0);

    // Mouth: a dark notch at the front, with teeth for strong jaws.
    if hunt > 0.05 {
        let mouth_open = 0.25 + 0.35 * hunt;
        let in_mouth = step(abs(angle), mouth_open) * smoothstep(0.55, 0.75, d);
        col = mix(col, vec3<f32>(0.08, 0.0, 0.01), in_mouth);
        let teeth = step(0.75, fract(angle * 12.0)) * step(abs(angle), mouth_open + 0.08) * smoothstep(0.78, 0.9, d) * jaws;
        col = mix(col, vec3<f32>(1.2, 1.1, 0.95), teeth);
        alpha *= 1.0 - in_mouth * step(0.9, d);
    }

    // Armor shell near the edge.
    let shell = 0.06 + 0.22 * armor;
    let in_shell = smoothstep(edge - shell - aa, edge - shell + aa, d) * armor;
    let shell_col = mix(vec3<f32>(0.22, 0.2, 0.17), base * 0.5, 0.35) * (0.8 + 0.4 * hash21(floor(q * 9.0)));
    col = mix(col, shell_col, clamp(in_shell * 1.5, 0.0, 1.0));

    // Membrane rim light.
    col += base * 0.5 * smoothstep(edge - 0.18, edge - 0.02, d) * (1.0 - armor);

    // Starving organisms fade; wounded ones redden.
    col *= 0.55 + 0.45 * smoothstep(0.0, 0.35, energy);
    col = mix(col, col * vec3<f32>(1.3, 0.5, 0.5), (1.0 - health) * 0.6);

    // Spikes and tail.
    let spike_col = mix(base * 0.4, vec3<f32>(0.85, 0.8, 0.7), 0.5);
    col = mix(col, spike_col, spike_alpha * (1.0 - alpha));
    alpha = max(alpha, spike_alpha);
    let tail_col = base * 0.8 + vec3<f32>(0.05);
    col = mix(col, tail_col, tail_alpha * (1.0 - alpha));
    alpha = max(alpha, tail_alpha);

    let body = vec4<f32>(col, alpha);
    return vec4<f32>(mix(body.rgb, overlay.rgb, overlay.a * (1.0 - body.a * 0.3)), max(body.a, overlay.a));
}
