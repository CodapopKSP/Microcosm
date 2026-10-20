//! Pencil sketches of organisms for the field notebook. They trace the same shapes the game
//! draws (`organism.wgsl`): the body's five-lobed membrane, its nucleus and organelles, the
//! mouth, and each part kind's own outline, posed at rest. Graphite lines and hatching over
//! watercolour washes in the colours the game gives the body and each part.
//!
//! Every stroke is wobbled by a seeded random generator, so a sketch looks hand-drawn but
//! stays exactly the same from frame to frame.

use bevy_egui::egui::{self, Color32, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};
use evo_core::Traits;
use evo_core::genome::Genome;
use evo_core::parts::{self, BodyShape, Instance, PartKind};
use evo_core::rng::Rng;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

/// Pencil graphite on paper.
pub const GRAPHITE: Color32 = Color32::from_rgb(52, 50, 58);
/// How far the membrane bulges in and out (the game uses 0.035; a line drawing needs a
/// little more to show the same five-sided look).
const WOBBLE: f32 = 0.05;

/// A pencil drawing onto `painter`: body-local coordinates (x forward, y up, 1 = body
/// radius) are mapped onto the page.
pub struct Pencil<'a> {
    painter: &'a egui::Painter,
    rng: Rng,
    origin: Pos2,
    scale: f32,
    /// Darkness multiplier (fainter for older sketches).
    pub weight: f32,
    /// The colour the part being drawn is washed in (`limb` and `fill` use it).
    tint: Option<Color32>,
}

impl<'a> Pencil<'a> {
    pub fn new(painter: &'a egui::Painter, seed: u64) -> Self {
        Self {
            painter,
            rng: Rng::new(seed),
            origin: Pos2::ZERO,
            scale: 1.0,
            weight: 1.0,
            tint: None,
        }
    }

    fn to_screen(&self, p: Vec2) -> Pos2 {
        self.origin + vec2(p.x, -p.y) * self.scale
    }

    /// A pencil line through screen points: two slightly different passes, wobbling a
    /// little and overshooting the ends, as a hand would.
    pub fn line(&mut self, pts: &[Pos2], width: f32, darkness: f32) {
        self.line_colored(pts, width, darkness, GRAPHITE);
    }

    pub fn line_colored(&mut self, pts: &[Pos2], width: f32, darkness: f32, color: Color32) {
        if pts.len() < 2 {
            return;
        }
        // Resample every ~3 px so the wobble has points to move.
        let mut dense = Vec::with_capacity(pts.len() * 4);
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let n = ((b - a).length() / 3.0).ceil().max(1.0) as usize;
            for i in 0..n {
                dense.push(a + (b - a) * (i as f32 / n as f32));
            }
        }
        dense.push(*pts.last().unwrap());
        if dense.len() < 2 {
            return;
        }
        for pass in 0..2 {
            let p1 = self.rng.f32() * TAU;
            let p2 = self.rng.f32() * TAU;
            let amp = 0.3 + 0.4 * self.rng.f32();
            let over = 0.8 + 1.2 * self.rng.f32();
            let mut out = Vec::with_capacity(dense.len() + 2);
            let mut s = 0.0;
            for i in 0..dense.len() {
                let p = dense[i];
                let tangent = if i + 1 < dense.len() {
                    dense[i + 1] - p
                } else {
                    p - dense[i - 1]
                };
                let normal = tangent.normalized().rot90();
                if i > 0 {
                    s += (p - dense[i - 1]).length();
                }
                let off = amp * ((s * 0.07 + p1).sin() * 0.6 + (s * 0.19 + p2).sin() * 0.4);
                out.push(if normal.is_finite() {
                    p + normal * off
                } else {
                    p
                });
            }
            // Overshoot both ends a touch.
            let start = (out[0] - out[1]).normalized();
            let end = (out[out.len() - 1] - out[out.len() - 2]).normalized();
            if start.is_finite() && end.is_finite() {
                out.insert(0, out[0] + start * over);
                let last = *out.last().unwrap();
                out.push(last + end * over * 0.6);
            }
            let alpha =
                (darkness * self.weight * if pass == 0 { 0.8 } else { 0.45 }).clamp(0.0, 1.0);
            let w = width * if pass == 0 { 1.0 } else { 0.7 };
            self.painter.add(Shape::line(
                out,
                Stroke::new(w, color.gamma_multiply(alpha)),
            ));
        }
    }

    /// A pencil line through body-local points.
    fn path(&mut self, pts: &[Vec2], width: f32, darkness: f32) {
        let screen: Vec<Pos2> = pts.iter().map(|&p| self.to_screen(p)).collect();
        self.line(&screen, width, darkness);
    }

    fn path_colored(&mut self, pts: &[Vec2], width: f32, darkness: f32, color: Color32) {
        let screen: Vec<Pos2> = pts.iter().map(|&p| self.to_screen(p)).collect();
        self.line_colored(&screen, width, darkness, color);
    }

    fn closed(&mut self, pts: &[Vec2], width: f32, darkness: f32) {
        let mut v = pts.to_vec();
        if let Some(&first) = pts.first() {
            v.push(first);
        }
        self.path(&v, width, darkness);
    }

    fn circle(&mut self, c: Vec2, r: f32, width: f32, darkness: f32) {
        let n = ((r * self.scale * 0.8) as usize).clamp(8, 28);
        let ring: Vec<Vec2> = (0..n)
            .map(|i| c + polar(i as f32 / n as f32 * TAU) * r)
            .collect();
        self.closed(&ring, width, darkness);
    }

    /// Paper-coloured fill (hides what's behind), in body-local points.
    fn blank(&self, pts: &[Vec2]) {
        let screen: Vec<Pos2> = pts.iter().map(|&p| self.to_screen(p)).collect();
        self.painter.add(Shape::convex_polygon(
            screen,
            super::notebook::PAPER,
            Stroke::NONE,
        ));
    }

    /// Paper fill, then a wash in the part's colour, over a body-local polygon.
    fn fill(&mut self, pts: &[Vec2]) {
        self.blank(pts);
        if let Some(color) = self.tint {
            self.wash_polygon(pts, color, 0.8);
        }
    }

    fn wash_polygon(&mut self, pts: &[Vec2], color: Color32, strength: f32) {
        let screen: Vec<Pos2> = pts.iter().map(|&p| self.to_screen(p)).collect();
        let bounds = Rect::from_points(&screen);
        self.wash(bounds, color, strength, &|p| {
            if in_polygon(pts, p) { 1.0 } else { 0.0 }
        });
    }

    /// A watercolour wash over `bounds` (screen), as much paint as `cover` says (body-local
    /// point → 0..1): translucent, a little blotchy, darker where it pooled at the edges as it
    /// dried, and drifting a pixel or so off the pencil line.
    fn wash(&mut self, bounds: Rect, color: Color32, strength: f32, cover: &dyn Fn(Vec2) -> f32) {
        let bounds = bounds.expand(2.0);
        if !bounds.is_positive() {
            return;
        }
        let cell = (bounds.width().max(bounds.height()) / 110.0).max(1.4);
        let nx = (bounds.width() / cell).ceil() as usize + 2;
        let ny = (bounds.height() / cell).ceil() as usize + 2;
        let phase: [f32; 4] = std::array::from_fn(|_| self.rng.f32() * TAU);
        let drift = 0.6 + 0.8 * self.rng.f32();
        let grain_seed = self.rng.f32() * 100.0;
        // Positions relative to the sketch, so the paint doesn't shimmer as the page scrolls.
        let at = |i: usize, j: usize| bounds.min + vec2(i as f32, j as f32) * cell;
        let mut paint = vec![0.0f32; nx * ny];
        for j in 0..ny {
            for i in 0..nx {
                let rel = at(i, j) - self.origin;
                let off = vec2((rel.y * 0.09 + phase[0]).sin(), (rel.x * 0.08 + phase[1]).sin());
                let local = (rel + off * drift) / self.scale;
                paint[j * nx + i] = cover(vec2(local.x, -local.y)).clamp(0.0, 1.0);
            }
        }
        // Pigment pools at the edge: compare each point with its neighbourhood (a summed-area
        // table gives the local average).
        let mut sum = vec![0.0f32; (nx + 1) * (ny + 1)];
        for j in 0..ny {
            for i in 0..nx {
                sum[(j + 1) * (nx + 1) + i + 1] = paint[j * nx + i] + sum[j * (nx + 1) + i + 1]
                    + sum[(j + 1) * (nx + 1) + i]
                    - sum[j * (nx + 1) + i];
            }
        }
        let r = ((2.5 / cell).round() as usize).max(1);
        let mut mesh = egui::Mesh::default();
        for j in 0..ny {
            for i in 0..nx {
                let k = j * nx + i;
                let (i0, i1) = (i.saturating_sub(r), (i + r + 1).min(nx));
                let (j0, j1) = (j.saturating_sub(r), (j + r + 1).min(ny));
                let total = sum[j1 * (nx + 1) + i1] - sum[j0 * (nx + 1) + i1]
                    - sum[j1 * (nx + 1) + i0]
                    + sum[j0 * (nx + 1) + i0];
                let average = total / ((i1 - i0) * (j1 - j0)) as f32;
                let edge = ((paint[k] - average) * 2.2).clamp(0.0, 1.0);
                let rel = at(i, j) - self.origin;
                let blotch = 0.85
                    + 0.15 * (rel.x * 0.11 + phase[2]).sin() * (rel.y * 0.13 + phase[3]).sin()
                    + 0.1 * (hash(i as f32 + grain_seed, j as f32) - 0.5);
                let alpha = paint[k] * strength * self.weight * (0.7 * blotch + 0.5 * edge);
                mesh.colored_vertex(at(i, j), color.gamma_multiply(alpha.clamp(0.0, 1.0)));
            }
        }
        for j in 0..ny - 1 {
            for i in 0..nx - 1 {
                let k = j * nx + i;
                let quad = [k, k + 1, k + nx, k + nx + 1];
                if quad.iter().any(|&q| paint[q] > 0.0) {
                    let [a, b, c, d] = quad.map(|q| q as u32);
                    mesh.add_triangle(a, b, c);
                    mesh.add_triangle(b, d, c);
                }
            }
        }
        if !mesh.indices.is_empty() {
            self.painter.add(Shape::mesh(mesh));
        }
    }

    fn dot(&self, c: Vec2, r: f32, color: Color32) {
        self.painter
            .circle_filled(self.to_screen(c), (r * self.scale).max(0.7), color);
    }

    /// Parallel hatching (in screen space at `angle`) over `bounds`, kept to where `inside`
    /// says (body-local point → how dark, 0 = none). Lines are broken where it's light.
    fn hatch(
        &mut self,
        bounds: Rect,
        angle: f32,
        spacing: f32,
        color: Color32,
        darkness: f32,
        inside: &dyn Fn(Vec2) -> f32,
    ) {
        let dir = Vec2::angled(angle);
        let normal = dir.rot90();
        let center = bounds.center();
        let reach = bounds.size().length() * 0.5 + 2.0;
        let mut offset = -reach;
        while offset <= reach {
            let jitter = (self.rng.f32() - 0.5) * spacing * 0.35;
            let base = center + normal * (offset + jitter);
            let mut run: Vec<Pos2> = Vec::new();
            let mut t = -reach;
            while t <= reach {
                let p = base + dir * t;
                let local = (p - self.origin) / self.scale;
                let shade = inside(vec2(local.x, -local.y));
                if shade > 0.0 && self.rng.f32() < shade.min(1.0) * 4.0 {
                    run.push(p);
                } else if run.len() > 1 {
                    let r = std::mem::take(&mut run);
                    self.hatch_stroke(&r, color, darkness);
                } else {
                    run.clear();
                }
                t += 1.5;
            }
            if run.len() > 1 {
                self.hatch_stroke(&run, color, darkness);
            }
            offset += spacing;
        }
    }

    fn hatch_stroke(&mut self, run: &[Pos2], color: Color32, darkness: f32) {
        let (a, b) = (run[0], run[run.len() - 1]);
        let alpha = (darkness * self.weight * (0.7 + 0.3 * self.rng.f32())).clamp(0.0, 1.0);
        self.painter
            .line_segment([a, b], Stroke::new(0.8, color.gamma_multiply(alpha)));
    }

    /// A limb drawn along a midline with a half-width at each point: two edge lines when it's
    /// wide enough on the page to show, otherwise a single line.
    fn limb(&mut self, mid: &[(Vec2, f32)], line_w: f32, darkness: f32) {
        if mid.len() < 2 {
            return;
        }
        let widest = mid.iter().map(|m| m.1).fold(0.0, f32::max) * self.scale;
        if widest < 1.3 {
            let pts: Vec<Vec2> = mid.iter().map(|m| m.0).collect();
            let w = (widest * 2.0).clamp(line_w * 0.8, line_w * 1.6);
            if let Some(color) = self.tint {
                self.path_colored(&pts, w * 2.0, 0.6, color);
            }
            self.path(&pts, w, darkness);
            return;
        }
        let side = |sign: f32| -> Vec<Vec2> {
            (0..mid.len())
                .map(|i| {
                    let (p, hw) = mid[i];
                    let next = mid[(i + 1).min(mid.len() - 1)].0;
                    let prev = mid[i.saturating_sub(1)].0;
                    let n = (next - prev).normalized().rot90();
                    p + n * hw * sign
                })
                .collect()
        };
        let (a, b) = (side(1.0), side(-1.0));
        // Paper inside, so overlapping limbs read as in front.
        for i in 0..mid.len() - 1 {
            self.blank(&[a[i], a[i + 1], b[i + 1], b[i]]);
        }
        if let Some(color) = self.tint {
            let outline: Vec<Vec2> = a.iter().chain(b.iter().rev()).copied().collect();
            self.wash_polygon(&outline, color, 0.8);
        }
        self.path(&a, line_w, darkness);
        self.path(&b, line_w, darkness);
        // Close the end.
        self.path(
            &[a[a.len() - 1], b[b.len() - 1]],
            line_w * 0.8,
            darkness * 0.8,
        );
    }
}

fn polar(angle: f32) -> Vec2 {
    vec2(angle.cos(), angle.sin())
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Even-odd test: is `p` inside the polygon?
fn in_polygon(pts: &[Vec2], p: Vec2) -> bool {
    let mut inside = false;
    let mut j = pts.len().wrapping_sub(1);
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// A linear colour (as the shader works in) back to sRGB.
fn srgb(c: Vec3) -> Color32 {
    let encode = |c: f32| {
        let c = c.clamp(0.0, 1.0);
        let s = if c <= 0.0031308 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0).round() as u8
    };
    Color32::from_rgb(encode(c.x), encode(c.y), encode(c.z))
}

/// The body colour, linear.
fn base_linear(t: &Traits) -> Vec3 {
    Vec3::from(t.color.map(to_linear))
}

/// Chlorophyll's colour: purple for sulfur photosynthesis through golden-brown to green for
/// oxygen-releasing, as the game colours fronds.
fn leaf_pigment(oxygenic: f32) -> Vec3 {
    let purple = Vec3::new(0.5, 0.12, 0.48);
    let golden = Vec3::new(0.58, 0.44, 0.1);
    let green = Vec3::new(0.14, 0.58, 0.1);
    if oxygenic < 0.5 {
        purple.lerp(golden, oxygenic * 2.0)
    } else {
        golden.lerp(green, oxygenic * 2.0 - 1.0)
    }
}

/// An eye's iris (the game picks it per eye from amber to blue).
fn iris_color(eye_seed: f32) -> Vec3 {
    Vec3::new(1.0, 0.62, 0.08).lerp(Vec3::new(0.15, 0.55, 1.0), (eye_seed * 3.7).fract())
}

/// A part's colour as the game shades it (`organism.wgsl`, mid-length and unlit).
fn part_color(kind: PartKind, t: &Traits) -> Color32 {
    let base = base_linear(t);
    let v = Vec3::new;
    srgb(match kind {
        PartKind::Flagellum => base * 0.95 + Vec3::splat(0.09),
        PartKind::Cilia => base * 0.7,
        PartKind::Tentacle => base * 0.9,
        PartKind::Frond => {
            let pigment = leaf_pigment(t.oxygenic);
            let faded = pigment.lerp(v(0.8, 0.82, 0.72), 0.45);
            faded.lerp(pigment, smoothstep(0.0, 0.6, t.photo)).lerp(base, 0.08)
        }
        PartKind::Fin => base * 0.95 + Vec3::splat(0.03),
        PartKind::Fluke => base * 0.95,
        PartKind::Leg => base.lerp(v(0.35, 0.25, 0.15), 0.35) * 0.9,
        PartKind::Holdfast => base.lerp(v(0.32, 0.22, 0.1), 0.55),
        PartKind::Stalk => base * 0.8,
        // The eye itself is coloured where it's drawn.
        PartKind::Eye => base,
        PartKind::Antenna => base.lerp(v(0.85, 0.75, 0.55), 0.3),
        PartKind::Lure => base * 0.7,
        PartKind::Spine => (base * 0.4).lerp(v(0.9, 0.85, 0.72), 0.7),
        PartKind::Jaw => v(0.2, 0.11, 0.05).lerp(v(0.85, 0.7, 0.5), 0.25).lerp(base * 0.5, 0.2),
        PartKind::Shell => (base * 0.45).lerp(v(0.55, 0.5, 0.4), 0.45),
    })
}

fn hash(a: f32, b: f32) -> f32 {
    let q = vec2((a * 123.34).fract(), (b * 456.21).fract());
    let d = q.x * (q.x + 45.32) + q.y * (q.y + 45.32);
    let r = vec2(q.x + d, q.y + d);
    (r.x * r.y).fract().abs()
}

/// Everything about a specimen needed to draw it.
pub struct Subject<'g> {
    pub genome: &'g Genome,
    pub traits: &'g Traits,
}

/// The body as the game shapes it, with the membrane's five-lobed wobble.
struct Body {
    shape: BodyShape,
    /// Phase of the wobble (each organism's is different).
    phase: f32,
}

impl Body {
    /// Normalized coordinates: the whole body is the unit circle (as in the shader).
    fn norm(&self, p: Vec2) -> Vec2 {
        vec2(
            p.x / self.shape.half_length,
            p.y / self.shape.width_at(p.x).max(1e-4),
        )
    }

    fn denorm(&self, q: Vec2) -> Vec2 {
        let x = q.x * self.shape.half_length;
        vec2(x, q.y * self.shape.width_at(x))
    }

    fn edge(&self, p: Vec2) -> f32 {
        let q = self.norm(p);
        1.0 + WOBBLE * (q.y.atan2(q.x) * 5.0 + self.phase).sin()
    }

    /// Distance to the outline (1 = on it), bead by bead for segmented bodies.
    fn distance(&self, p: Vec2) -> f32 {
        let b = &self.shape;
        let d = if b.segments <= 1 {
            self.norm(p).length()
        } else {
            (0..b.segments)
                .map(|k| {
                    let (cx, half) = b.segment(k);
                    let a = half * parts::SEGMENT_OVERLAP;
                    let w = b.segment_width(k);
                    vec2((p.x - cx) / a, p.y / w).length()
                })
                .fold(f32::MAX, f32::min)
        };
        d / self.edge(p)
    }

    fn inside(&self, p: Vec2) -> bool {
        self.distance(p) <= 1.0
    }

    /// Outline(s): one for a single-segment body, one bead per segment otherwise.
    fn outlines(&self) -> Vec<Vec<Vec2>> {
        let b = &self.shape;
        let n = 60;
        if b.segments <= 1 {
            let pts = (0..n)
                .map(|i| {
                    let th = i as f32 / n as f32 * TAU;
                    let e = 1.0 + WOBBLE * (th * 5.0 + self.phase).sin();
                    self.denorm(polar(th) * e)
                })
                .collect();
            return vec![pts];
        }
        (0..b.segments)
            .map(|k| {
                let (cx, half) = b.segment(k);
                let a = half * parts::SEGMENT_OVERLAP;
                let w = b.segment_width(k);
                (0..n)
                    .map(|i| {
                        let th = i as f32 / n as f32 * TAU;
                        let p = vec2(cx + a * th.cos(), w * th.sin());
                        let c = vec2(cx, 0.0);
                        c + (p - c) * self.edge(p)
                    })
                    .collect()
            })
            .collect()
    }
}

/// Body-local bounding box of the organism (body, mouth and parts).
fn extent(body: &BodyShape, instances: &[Instance], t: &Traits) -> Rect {
    let snout = if t.scavenge > 0.35 && t.hunt < 0.3 {
        body.half_length * (0.9 + 0.6 * smoothstep(0.35, 1.0, t.scavenge) + 0.3)
    } else {
        0.0
    };
    let mut r = Rect::from_min_max(
        pos2(-body.half_length * 1.06, -body.half_width * 1.15),
        pos2(
            body.nose().max(body.half_length).max(snout) * 1.06,
            body.half_width * 1.15,
        ),
    );
    for i in instances {
        let (d, n) = (polar(i.dir), polar(i.dir).rot90());
        let base = vec2(i.base.x, i.base.y);
        // How far each kind reaches sideways (in part lengths) as drawn.
        let w = i.width / i.length.max(1e-4) * 0.5;
        let lat = match i.kind {
            PartKind::Tentacle => 0.4,
            PartKind::Flagellum => 0.12,
            PartKind::Fin => (w * 1.6).atan().sin() * 0.9 + 0.25,
            PartKind::Fluke => w + 0.25,
            PartKind::Antenna | PartKind::Lure => 0.3 + w,
            PartKind::Leg => 0.45,
            PartKind::Eye => 0.5,
            PartKind::Jaw => 0.15 + w,
            _ => w,
        };
        for (a, c) in [
            (0.0, 0.0),
            (1.0, 0.0),
            (0.7, lat),
            (0.7, -lat),
            (1.0, lat * 0.6),
            (1.0, -lat * 0.6),
        ] {
            let p = base + d * i.length * a + n * i.length * c;
            r.extend_with(pos2(p.x, p.y));
        }
    }
    if t.toxin > 0.05 {
        r = r.union(Rect::from_center_size(
            Pos2::ZERO,
            vec2(body.half_length, body.half_width) * 2.5,
        ));
    }
    r
}

/// Sketch a specimen fitted inside `rect`. Returns, for leader-line labels, one point per
/// part kind (screen), the direction it points (screen), and the part's name.
pub fn draw_specimen(
    painter: &egui::Painter,
    rect: Rect,
    subject: &Subject,
    seed: u64,
    weight: f32,
) -> Vec<(Pos2, Vec2, &'static str)> {
    let t = subject.traits;
    let shape = BodyShape::from_genome(subject.genome);
    let decoded = parts::decode_parts(subject.genome);
    let mut instances = Vec::new();
    parts::layout(&decoded, &shape, &mut instances);
    let mut rng = Rng::new(seed ^ 0x5EED_5EED);
    let body = Body {
        shape,
        phase: rng.f32() * TAU,
    };
    let organism_seed = rng.f32();

    let bounds = extent(&shape, &instances, t);
    let scale = (rect.width() / bounds.width()).min(rect.height() / bounds.height()) * 0.95;
    let mut pen = Pencil::new(painter, seed);
    pen.weight = weight;
    pen.scale = scale;
    let mid = bounds.center();
    pen.origin = rect.center() - vec2(mid.x, -mid.y) * scale;
    let line_w = (scale * 0.03).clamp(0.9, 1.7);

    // Parts behind the body first, as the game draws them.
    for inst in &instances {
        if !inst.kind.in_front() {
            draw_part(&mut pen, inst, line_w, organism_seed, t);
        }
    }
    pen.tint = None;

    // The body: paper fill (hides part roots), a wash, organelles, shading, outline.
    let outlines = body.outlines();
    for o in &outlines {
        pen.blank(o);
    }
    let hl = shape.half_length;
    let hw = shape.half_width;
    let screen_bounds = Rect::from_two_pos(
        pen.to_screen(vec2(-hl * 1.2, hw * 1.3)),
        pen.to_screen(vec2(shape.nose().max(hl) * 1.2, -hw * 1.3)),
    );
    let tint = super::color32(t.color);
    pen.wash(screen_bounds, tint, 0.85, &|p| if body.inside(p) { 1.0 } else { 0.0 });
    organelles(&mut pen, &body, t, organism_seed, screen_bounds);
    pattern(&mut pen, &body, t, line_w, screen_bounds);
    // Lit centre, darker rim (per bead), in graphite.
    let rim = |p: Vec2| -> f32 {
        let d = body.distance(p);
        if d > 1.0 {
            return 0.0;
        }
        let lower = (-p.y / hw).max(0.0) * 0.25;
        (smoothstep(0.55, 1.0, d) * 0.8 + lower * smoothstep(0.2, 1.0, d)).max(0.0)
    };
    pen.hatch(
        screen_bounds,
        0.8,
        (scale * 0.065).clamp(1.8, 3.2),
        GRAPHITE,
        0.3,
        &rim,
    );
    pen.hatch(
        screen_bounds,
        -0.75,
        (scale * 0.08).clamp(2.2, 3.8),
        GRAPHITE,
        0.2,
        &|p| (rim(p) - 0.5).max(0.0),
    );
    nucleus(&mut pen, &body, organism_seed, line_w);
    mouth(&mut pen, &body, t, line_w);
    // Membrane: the outline, and a faint inner line just inside it.
    for o in &outlines {
        pen.closed(o, line_w * 1.2, 0.95);
    }
    if body.shape.segments <= 1 {
        let inner: Vec<Vec2> = outlines[0].iter().map(|&p| p * 0.93).collect();
        pen.closed(&inner, line_w * 0.5, 0.25);
    }

    // Toxin: the game's faint red haze, as a dotted red-pencil halo.
    if t.toxin > 0.05 {
        let red = Color32::from_rgb(180, 50, 40);
        for k in 0..40 {
            let th = k as f32 / 40.0 * TAU;
            let a = body.denorm(polar(th) * 1.14);
            let b = body.denorm(polar(th + 0.06) * 1.14);
            pen.path_colored(&[a, b], line_w * 0.9, 0.35 + 0.5 * t.toxin.min(1.0), red);
        }
    }

    for inst in &instances {
        if inst.kind.in_front() {
            draw_part(&mut pen, inst, line_w, organism_seed, t);
        }
    }
    pen.tint = None;

    // One label per part kind, at the part's middle, pointing outward.
    let mut labels: Vec<(Pos2, Vec2, &'static str)> = Vec::new();
    for inst in &instances {
        if labels.iter().any(|l| l.2 == inst.kind.name()) {
            continue;
        }
        let mid = vec2(inst.base.x, inst.base.y) + polar(inst.dir) * inst.length * 0.6;
        labels.push((
            pen.to_screen(mid),
            vec2(inst.dir.cos(), -inst.dir.sin()),
            inst.kind.name(),
        ));
    }
    labels
}

/// The jittered-grid dot pattern the shader uses for organelles: one dot per cell.
fn grid_dots(scale: f32, seed: f32, extent: Vec2) -> Vec<Vec2> {
    let mut out = Vec::new();
    let (nx, ny) = (
        (extent.x * scale).ceil() as i32 + 1,
        (extent.y * scale).ceil() as i32 + 1,
    );
    for j in -ny..=ny {
        for i in -nx..=nx {
            let (ci, cj) = (i as f32, j as f32);
            let jitter = vec2(
                hash(ci + seed, cj + seed),
                hash(ci + seed + 17.0, cj + seed + 17.0),
            );
            out.push((vec2(ci, cj) + jitter) / scale);
        }
    }
    out
}

/// Chloroplasts (green, or purple for sulfur photosynthesis), scavengers' granules, nitrogen
/// fixers' pale cysts, and a chemotroph's glow, in the game's colours.
fn organelles(pen: &mut Pencil, body: &Body, t: &Traits, seed: f32, bounds: Rect) {
    let ext = vec2(body.shape.half_length, body.shape.half_width) * 1.1;
    let keep = |p: Vec2, limit: f32| body.distance(p) <= limit;
    if t.photo > 0.05 {
        let pigment =
            srgb(Vec3::new(0.4, 0.06, 0.36).lerp(Vec3::new(0.08, 0.45, 0.06), t.oxygenic));
        let alpha = (0.25 + 0.6 * t.photo).min(0.85) * pen.weight;
        for p in grid_dots(5.0, seed * 10.0, ext) {
            if keep(p, 0.85) {
                pen.dot(p, 0.045, pigment.gamma_multiply(alpha));
            }
        }
    }
    if t.fixation > 0.05 {
        for p in grid_dots(2.2, seed * 3.0, ext) {
            if keep(p, 0.8) {
                pen.circle(p, 0.11, 0.6, 0.35 + 0.4 * t.fixation);
            }
        }
    }
    if t.scavenge > 0.05 {
        let brown = srgb(Vec3::new(0.25, 0.13, 0.04));
        let alpha = (0.3 + 0.6 * t.scavenge).min(0.85) * pen.weight;
        for p in grid_dots(7.0, seed * 5.0, ext) {
            if keep(p, 0.9) {
                pen.dot(p, 0.03, brown.gamma_multiply(alpha));
            }
        }
    }
    if t.chemo > 0.05 {
        // A glow from within: a soft cyan wash and a few rings around the centre.
        let cyan = Color32::from_rgb(40, 150, 170);
        pen.wash(bounds, srgb(Vec3::new(0.15, 0.9, 1.0)), 0.7 * t.chemo.min(1.0), &|p| {
            let d = body.norm(p).length();
            if d <= 1.0 { (-d * d * 5.0).exp() } else { 0.0 }
        });
        for k in 1..4 {
            let r = 0.12 * k as f32;
            let ring: Vec<Vec2> = (0..=20)
                .map(|i| body.denorm(polar(i as f32 / 20.0 * TAU) * r))
                .collect();
            pen.path_colored(
                &ring,
                0.8,
                (0.6 - 0.12 * k as f32) * t.chemo.min(1.0) + 0.15,
                cyan,
            );
        }
    }
}

fn nucleus(pen: &mut Pencil, body: &Body, seed: f32, line_w: f32) {
    let b = &body.shape;
    let c = vec2(-0.15, 0.1 * (seed * 20.0).sin());
    let (center, rx, ry) = if b.segments > 1 {
        // In the head segment.
        let (cx, half) = b.segment(0);
        let a = half * parts::SEGMENT_OVERLAP;
        let w = b.segment_width(0);
        (vec2(cx + c.x * a, c.y * w), 0.27 * a, 0.27 * w)
    } else {
        (
            body.denorm(c),
            0.27 * b.half_length,
            0.27 * b.width_at(c.x * b.half_length),
        )
    };
    let ring: Vec<Vec2> = (0..18)
        .map(|i| {
            let a = i as f32 / 18.0 * TAU;
            center + vec2(a.cos() * rx, a.sin() * ry) * (1.0 + 0.05 * (a * 3.0).sin())
        })
        .collect();
    pen.closed(&ring, line_w * 0.8, 0.7);
    // Darker inside, as in the game.
    let bounds = Rect::from_two_pos(
        pen.to_screen(center - vec2(rx, ry)),
        pen.to_screen(center + vec2(rx, ry)),
    );
    pen.hatch(bounds, 0.4, 1.8, GRAPHITE, 0.35, &|p| {
        if ((p - center) / vec2(rx, ry)).length() < 0.9 {
            0.8
        } else {
            0.0
        }
    });
}

/// Hunters' oral groove (a pore when small) and scavengers' proboscis, at the front.
fn mouth(pen: &mut Pencil, body: &Body, t: &Traits, line_w: f32) {
    let (hunt, scav) = (t.hunt, t.scavenge);
    if hunt > 0.04 {
        if hunt < 0.35 {
            // Cytostome: a small dark oval near the front edge.
            let c = vec2(0.82, 0.0);
            let (rx, ry) = (0.1 + 0.15 * hunt, 0.07 + 0.2 * hunt);
            let ring: Vec<Vec2> = (0..12)
                .map(|i| {
                    let a = i as f32 / 12.0 * TAU;
                    body.denorm(c + vec2(a.cos() * rx, a.sin() * ry))
                })
                .collect();
            pen.closed(&ring, line_w * 0.8, 0.8);
        }
        if hunt > 0.25 {
            // Oral groove: a funnel from the front edge into the cell.
            let half = 0.08 + 0.26 * hunt;
            let depth = 0.3 + 0.3 * hunt;
            for sign in [1.0f32, -1.0] {
                let edge: Vec<Vec2> = (0..=10)
                    .map(|i| {
                        let u = depth * i as f32 / 10.0;
                        let w = half * (1.0 - u / depth).max(0.0).sqrt();
                        body.denorm(vec2(1.02 - u, w * sign))
                    })
                    .collect();
                pen.path(&edge, line_w, 0.9);
            }
            // The gullet is dark.
            let tip = body.denorm(vec2(1.0 - depth, 0.0));
            let a = body.denorm(vec2(1.0, half * 0.8));
            let b = body.denorm(vec2(1.0, -half * 0.8));
            let bounds = Rect::from_two_pos(pen.to_screen(a), pen.to_screen(b))
                .union(Rect::from_center_size(pen.to_screen(tip), vec2(2.0, 2.0)));
            let (tri_a, tri_b, tri_c) = (a, b, tip);
            pen.hatch(bounds.expand(2.0), 1.2, 1.6, GRAPHITE, 0.6, &|p| {
                if in_triangle(p, tri_a, tri_b, tri_c) {
                    1.0
                } else {
                    0.0
                }
            });
        }
    }
    if scav > 0.12 && hunt < 0.3 {
        let len = 0.6 * smoothstep(0.35, 1.0, scav);
        let r = 0.06 + 0.07 * scav;
        if len > 0.02 {
            let x0 = body.shape.half_length * 0.75;
            let x1 = body.shape.half_length * (0.9 + len);
            let rr = r * body.shape.half_width;
            let tube = [vec2(x0, rr), vec2(x1, rr), vec2(x1, -rr), vec2(x0, -rr)];
            pen.blank(&tube);
            pen.wash_polygon(&tube, srgb(base_linear(t) * 1.35 + Vec3::splat(0.08)), 0.7);
            pen.path(&[vec2(x0, rr), vec2(x1, rr)], line_w, 0.9);
            pen.path(&[vec2(x0, -rr), vec2(x1, -rr)], line_w, 0.9);
            // Rings along the tube, and the opening.
            let rings = ((x1 - x0) / (rr * 1.2)).clamp(2.0, 8.0) as usize;
            for k in 1..rings {
                let x = x0 + (x1 - x0) * k as f32 / rings as f32;
                pen.path(&[vec2(x, rr), vec2(x, -rr)], line_w * 0.5, 0.45);
            }
            pen.dot(
                vec2(x1, 0.0),
                rr * 0.6,
                GRAPHITE.gamma_multiply(0.85 * pen.weight),
            );
            if scav > 0.35 {
                for k in 0..5 {
                    let a = (k as f32 - 2.0) * 0.45;
                    let d = polar(a);
                    let tip = vec2(x1, 0.0);
                    pen.path(
                        &[
                            tip + d * rr,
                            tip + d * (rr + (0.15 + 0.15 * scav) * body.shape.half_width),
                        ],
                        line_w * 0.5,
                        0.6,
                    );
                }
            }
        } else {
            pen.dot(
                body.denorm(vec2(0.9, 0.0)),
                0.08 * body.shape.half_width,
                GRAPHITE.gamma_multiply(0.8),
            );
        }
    }
}

fn in_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let cross = |u: Vec2, v: Vec2| u.x * v.y - u.y * v.x;
    let (d1, d2, d3) = (
        cross(b - a, p - a),
        cross(c - b, p - b),
        cross(a - c, p - c),
    );
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// Spots, stripes or rings, as the shader lays them out.
fn pattern(pen: &mut Pencil, body: &Body, t: &Traits, line_w: f32, bounds: Rect) {
    let style = t.pattern >> 6;
    let scale = 2.0 + (t.pattern & 63) as f32 / 10.0;
    let ext = vec2(body.shape.half_length, body.shape.half_width) * 1.1;
    let dark = super::color32(t.color).gamma_multiply(0.55);
    match style {
        1 => {
            for p in grid_dots(scale, 13.0, ext) {
                if body.distance(p) < 0.85 {
                    pen.dot(p, 0.35 * 0.8 / scale, dark.gamma_multiply(0.7 * pen.weight));
                    pen.circle(p, 0.35 * 0.8 / scale, line_w * 0.5, 0.4);
                }
            }
        }
        2 => {
            // Bands across the body where sin(x · scale · 3) > 0 (darkest in the middle of
            // each, soft at the edges, as the shader shades them), washed in the darker
            // colour; a light pencil line marks each band's edges, kept inside the outline.
            let f = scale * 3.0;
            let band = |x: f32| smoothstep(-0.2, 0.2, (x * f).sin());
            pen.wash(bounds, dark, 1.0, &|p| if body.inside(p) { band(p.x) } else { 0.0 });
            let k0 = (-body.shape.half_length * f / PI).floor() as i32;
            let k1 = (body.shape.nose() * f / PI).ceil() as i32;
            for k in k0..=k1 {
                let x = k as f32 * PI / f;
                let w = body.shape.width_at(x) * 1.05;
                let across: Vec<Vec2> = (0..=24)
                    .map(|i| vec2(x, -w + 2.0 * w * i as f32 / 24.0))
                    .collect();
                for run in across.split(|&p| !body.inside(p)).filter(|r| r.len() > 1) {
                    pen.path_colored(run, line_w * 0.8, 0.45, dark);
                }
            }
        }
        3 => {
            // Concentric rings.
            let step = TAU / (scale * 4.0);
            let mut r = step;
            while r < 0.95 {
                let ring: Vec<Vec2> = (0..=36)
                    .map(|i| body.denorm(polar(i as f32 / 36.0 * TAU) * r))
                    .collect();
                pen.path_colored(&ring, line_w * 1.6, 0.5, dark);
                r += step;
            }
        }
        _ => {}
    }
}

/// One part, traced from its shape in the game (at rest).
fn draw_part(pen: &mut Pencil, inst: &Instance, w_line: f32, seed: f32, t: &Traits) {
    let base = vec2(inst.base.x, inst.base.y);
    let d = polar(inst.dir);
    let n = d.rot90();
    let len = inst.length;
    let m = if inst.mirror < 0.0 { -1.0 } else { 1.0 };
    // The shader's part frame: a along (0..1), c across, both in part lengths; w is the
    // nominal half width in part lengths.
    let w = inst.width / len.max(1e-4) * 0.5;
    let s = inst.size;
    let phase = ((seed * 255.0) as u32 + inst.slot as u32 * 37) as f32 % 256.0 / 255.0 * 40.0
        + inst.index.min(100) as f32 * 1.9;
    let at = |a: f32, c: f32| base + d * (a * len) + n * (c * m * len);
    let samples = |k: usize| (0..=k).map(move |i| i as f32 / k as f32);
    pen.tint = Some(part_color(inst.kind, t));
    match inst.kind {
        PartKind::Flagellum => {
            let amp = (0.08 + 0.08 * s) * 0.6;
            let mid: Vec<(Vec2, f32)> = samples(28)
                .map(|a| {
                    let y = amp * (a * TAU * (1.0 + s) + phase).sin() * a;
                    (at(a, y), w * (1.0 - 0.7 * a) * len)
                })
                .collect();
            pen.limb(&mid, w_line, 0.85);
            if s > 0.25 {
                pen.circle(at(0.0, 0.0), w * 1.8 * len, w_line * 0.7, 0.7);
            }
            if s > 0.55 {
                // A fringe of fine hairs.
                for a in samples(14).skip(2) {
                    let y = amp * (a * TAU * (1.0 + s) + phase).sin() * a;
                    let p = at(a, y);
                    pen.path(
                        &[p - n * w * 3.0 * len, p + n * w * 3.0 * len],
                        w_line * 0.4,
                        0.4,
                    );
                }
            }
        }
        PartKind::Cilia => {
            // A band at the base with rows of short hairs.
            let spacing = 0.4 + (0.13 - 0.4) * s;
            pen.fill(&[at(-0.08, w), at(0.16, w), at(0.16, -w), at(-0.08, -w)]);
            pen.closed(
                &[at(-0.08, w), at(0.16, w), at(0.16, -w), at(-0.08, -w)],
                w_line * 0.6,
                0.6,
            );
            let count = (w / spacing).floor() as i32;
            for i in -count..=count {
                let y0 = i as f32 * spacing;
                let tilt = 0.35 * (-(i as f32) * 0.7 + phase).sin();
                let hair: Vec<Vec2> = samples(6).map(|a| at(a, y0 + tilt * a * a)).collect();
                pen.path(&hair, w_line * 0.6, 0.75);
            }
        }
        PartKind::Tentacle => {
            let sway = |a: f32| 0.16 * (a * 3.0 + phase).sin() * a;
            let curl = |a: f32| (0.18 + 0.12 * phase.sin()) * a * a * a;
            let club = |a: f32| {
                w * 0.6
                    * smoothstep(0.78, 0.92, a)
                    * (1.0 - smoothstep(0.94, 1.0, a))
                    * smoothstep(0.75, 0.95, s)
            };
            let mid: Vec<(Vec2, f32)> = samples(28)
                .map(|a| {
                    (
                        at(a, sway(a) + curl(a)),
                        (w * (1.0 - 0.65 * a) + club(a)) * len,
                    )
                })
                .collect();
            pen.limb(&mid, w_line, 0.85);
            if s > 0.3 {
                // Rings.
                for k in 1..14 {
                    let a = k as f32 / 14.0;
                    let y = sway(a) + curl(a);
                    let hw = w * (1.0 - 0.65 * a);
                    pen.path(&[at(a, y - hw), at(a, y + hw)], w_line * 0.4, 0.35);
                }
            }
            if s > 0.55 {
                // Suckers along one side.
                for k in 1..12 {
                    let a = (k as f32 + 0.5) / 12.0;
                    let y = sway(a) + curl(a);
                    let hw = w * (1.0 - 0.65 * a);
                    pen.circle(at(a, y - hw * 0.45), hw * 0.32 * len, w_line * 0.5, 0.55);
                }
            }
        }
        PartKind::Frond => {
            let y = |a: f32| 0.07 * (phase + a * 2.0).sin() * a * a;
            let half = |a: f32| {
                let mut h = w * (PI * ((a - 0.08) / 0.92).clamp(0.0, 1.0)).sin().powf(0.7);
                h *= 1.0 - 0.15 * smoothstep(0.65, 0.85, s) * (a * 45.0).sin().abs();
                h.max(w * 0.14 * if a <= 0.2 { 1.0 } else { 0.0 })
            };
            let leaf: Vec<Vec2> = samples(36)
                .map(|a| at(a, y(a) + half(a)))
                .chain(samples(36).map(|a| 1.0 - a).map(|a| at(a, y(a) - half(a))))
                .collect();
            pen.fill(&leaf);
            pen.closed(&leaf, w_line, 0.85);
            // The midrib and veins.
            if s > 0.2 {
                let rib: Vec<Vec2> = samples(10).map(|a| at(a * 0.95, y(a * 0.95))).collect();
                pen.path(&rib, w_line * 0.6, 0.6);
            }
            if s > 0.35 {
                for k in 1..8 {
                    let a = k as f32 / 9.0;
                    for sign in [1.0, -1.0] {
                        pen.path(
                            &[
                                at(a, y(a)),
                                at(a + 0.08, y(a + 0.08) + sign * half(a + 0.08) * 0.85),
                            ],
                            w_line * 0.45,
                            0.45,
                        );
                    }
                }
            }
            if s > 0.8 {
                // The gas bladder.
                let c = at(0.14, y(0.14));
                let r = w * 0.55 * len;
                let ring: Vec<Vec2> = (0..16).map(|j| c + polar(j as f32 / 16.0 * TAU) * r).collect();
                pen.wash_polygon(&ring, srgb(Vec3::new(0.75, 0.65, 0.3)), 0.9);
                pen.circle(c, r, w_line * 0.7, 0.75);
            }
        }
        PartKind::Fin => {
            // A fan: rays spread from the base to a rounded (later scalloped) edge.
            let flap = 0.25 * phase.sin();
            let spread = (w * 1.6).atan();
            let rays = 2.0 + (s * 6.0).floor();
            let reach = |phi: f32| {
                let across = (phi / spread * 0.5 + 0.5).clamp(0.0, 1.0);
                let r = (across * rays).fract();
                let between = r.min(1.0 - r);
                let scallop = 0.12 * smoothstep(0.6, 0.8, s) * (1.0 - between * 2.0);
                (0.75 + 0.25 * (phi / spread * 1.3).cos()) * (1.0 - scallop)
            };
            let point = |phi: f32, rho: f32| {
                let a = rho * phi.cos();
                let c = rho * phi.sin() + flap * a;
                at(a, c)
            };
            let mut fan = vec![at(0.0, 0.0)];
            fan.extend(samples(40).map(|u| {
                let phi = -spread + 2.0 * spread * u;
                point(phi, reach(phi))
            }));
            pen.fill(&fan);
            pen.closed(&fan, w_line, 0.85);
            if s > 0.25 {
                for k in 0..=(rays as usize) {
                    let phi = -spread + 2.0 * spread * k as f32 / rays;
                    pen.path(
                        &[at(0.0, 0.0), point(phi, reach(phi) * 0.97)],
                        w_line * 0.5,
                        0.55,
                    );
                }
            }
        }
        PartKind::Fluke => {
            // A paddle on a stalk that forks into a crescent as it grows.
            let beat = 0.25 * phase.sin();
            let fork = smoothstep(0.3, 0.8, s);
            let to_world = |a: f32, cc: f32| at(a, cc * w + beat * a);
            let notch_c = vec2(1.08, 0.0);
            let notch_r = vec2(0.12 + 0.35 * fork, 0.2 + 0.45 * fork);
            let blade: Vec<Vec2> = samples(48)
                .map(|u| {
                    let th = u * TAU;
                    let mut p = vec2(0.55 + 0.45 * th.cos(), th.sin());
                    // Carve the notch out of the back of the blade.
                    let rel = (p - notch_c) / notch_r;
                    if rel.length() < 1.0 {
                        p = notch_c + rel.normalized() * notch_r;
                    }
                    to_world(p.x, p.y)
                })
                .collect();
            pen.fill(&blade);
            pen.closed(&blade, w_line, 0.9);
            pen.path(&[to_world(0.0, 0.22), to_world(0.5, 0.17)], w_line, 0.85);
            pen.path(&[to_world(0.0, -0.22), to_world(0.5, -0.17)], w_line, 0.85);
            if fork > 0.2 {
                for k in -1..=1 {
                    let cc = k as f32 / 3.0;
                    pen.path(
                        &[to_world(0.55, cc * 0.5), to_world(0.92, cc)],
                        w_line * 0.45,
                        0.45,
                    );
                }
            }
        }
        PartKind::Leg => {
            // Jointed: more joints as it grows, knobs at the joints, then a claw.
            let nseg = 1 + (s * 2.99).floor() as usize;
            let seglen = 1.0 / nseg as f32;
            let mut p0 = vec2(0.0, 0.0);
            let mut ang: f32 = -0.25;
            let mut joints = Vec::new();
            let mut mid: Vec<(Vec2, f32)> = vec![(at(0.0, 0.0), w * len)];
            for k in 0..nseg {
                if k == 1 {
                    ang += 0.7;
                    joints.push(p0);
                }
                if k == 2 {
                    ang -= 1.1;
                    joints.push(p0);
                }
                let p1 = p0 + polar(ang) * seglen;
                let thick = w * (1.0 - 0.35 * (k as f32 + 1.0) / nseg as f32);
                mid.push((at(p1.x, p1.y), thick * len));
                p0 = p1;
            }
            pen.limb(&mid, w_line, 0.9);
            if s > 0.35 {
                for j in joints {
                    pen.circle(at(j.x, j.y), w * 1.15 * len, w_line * 0.6, 0.7);
                }
            }
            if s > 0.65 {
                for side in [-1.0f32, 1.0] {
                    let tip = p0 + polar(ang + side * 0.6) * 0.14;
                    pen.path(&[at(p0.x, p0.y), at(tip.x, tip.y)], w_line * 1.1, 0.9);
                }
            }
        }
        PartKind::Holdfast => {
            // A sucker disc on a stem, growing into a branching root system.
            pen.limb(
                &[
                    (at(0.0, 0.0), w * 0.28 * len),
                    (at(0.5, 0.0), w * 0.28 * len),
                ],
                w_line,
                0.85,
            );
            if s < 0.5 {
                let disc: Vec<Vec2> = samples(20)
                    .map(|u| {
                        let th = u * TAU;
                        at(0.85 + 0.15 * th.cos(), w * 0.7 * th.sin())
                    })
                    .collect();
                pen.fill(&disc);
                pen.closed(&disc, w_line, 0.85);
            }
            if s > 0.2 {
                let roots = 2 + (s * 5.0).floor() as usize;
                for k in 0..roots {
                    let tt = (k as f32 + 0.5) / roots as f32 * 2.0 - 1.0;
                    let midp = (
                        0.68,
                        tt * w * 0.45 + 0.05 * (k as f32 * 2.3 + seed * 9.0).sin(),
                    );
                    let tip = (0.97, tt * w * 0.95);
                    pen.limb(
                        &[
                            (at(0.4, 0.0), w * 0.2 * len),
                            (at(midp.0, midp.1), w * 0.14 * len),
                            (at(tip.0, tip.1), w * 0.06 * len),
                        ],
                        w_line * 0.8,
                        0.8,
                    );
                }
            }
        }
        PartKind::Stalk => {
            let mid: Vec<(Vec2, f32)> = samples(16)
                .map(|a| (at(a, 0.06 * phase.sin() * a * a), w * (1.0 - 0.3 * a) * len))
                .collect();
            pen.limb(&mid, w_line, 0.85);
            if s > 0.45 {
                for k in 1..10 {
                    let a = k as f32 / 10.0;
                    let y = 0.06 * phase.sin() * a * a;
                    let hw = w * (1.0 - 0.3 * a);
                    pen.path(&[at(a, y - hw), at(a, y + hw)], w_line * 0.4, 0.35);
                }
            }
        }
        PartKind::Eye => {
            // Eyespot -> pit eye -> lens eye -> camera eye, as it grows.
            let c = at(0.5, 0.0);
            let r = 0.5 * len;
            let ring = |rad: f32| -> Vec<Vec2> {
                (0..20)
                    .map(|j| c + polar(j as f32 / 20.0 * TAU) * rad)
                    .collect()
            };
            pen.blank(&ring(r));
            if s < 0.25 {
                pen.dot(
                    c,
                    r * 0.55,
                    srgb(Vec3::new(0.55, 0.05, 0.03)).gamma_multiply(0.85 * pen.weight),
                );
                pen.closed(&ring(r * 0.55), w_line * 0.6, 0.7);
                return;
            }
            pen.closed(&ring(r), w_line, 0.95);
            if s < 0.5 {
                // Pit eye: a dark cup opening outward.
                let cup: Vec<Vec2> = samples(12)
                    .map(|u| c + polar(PI * 0.5 + u * PI) * r * 0.7)
                    .collect();
                pen.path(&cup, w_line * 1.4, 0.9);
                return;
            }
            // The iris, in the colour the game gives this eye.
            let eye_seed = ((seed * 255.0) as u8) as f32 / 255.0 + inst.index as f32 * 0.37;
            pen.wash_polygon(&ring(r * 0.62), srgb(iris_color(eye_seed)), 0.95);
            pen.closed(&ring(r * 0.62), w_line * 0.7, 0.7);
            if s > 0.77 {
                // Striated iris and a rim.
                for k in 0..12 {
                    let a = k as f32 / 12.0 * TAU;
                    pen.path(
                        &[c + polar(a) * r * 0.32, c + polar(a) * r * 0.6],
                        w_line * 0.35,
                        0.4,
                    );
                }
                pen.closed(&ring(r * 1.12), w_line * 0.5, 0.45);
            }
            let pupil = c + d * r * 0.12;
            pen.dot(pupil, r * 0.3, GRAPHITE.gamma_multiply(0.9 * pen.weight));
            pen.dot(
                pupil + vec2(-r * 0.1, r * 0.12),
                r * 0.09,
                super::notebook::PAPER,
            );
        }
        PartKind::Antenna => {
            // A feeler: segmented, then feathery.
            let y = |a: f32| (0.12 * phase.sin() + 0.18) * a * a;
            let mid: Vec<(Vec2, f32)> = samples(16)
                .map(|a| (at(a, y(a)), w * (1.0 - 0.6 * a) * len))
                .collect();
            pen.limb(&mid, w_line * 0.8, 0.85);
            if s > 0.35 {
                for k in 1..10 {
                    let a = k as f32 / 10.0;
                    let hw = w * (1.0 - 0.6 * a) * 1.6;
                    pen.path(&[at(a, y(a) - hw), at(a, y(a) + hw)], w_line * 0.4, 0.5);
                }
            }
            if s > 0.65 {
                for k in 3..16 {
                    let a = k as f32 / 16.0;
                    for sign in [1.0f32, -1.0] {
                        pen.path(
                            &[at(a, y(a)), at(a + 0.05, y(a) + sign * 0.12)],
                            w_line * 0.35,
                            0.45,
                        );
                    }
                }
            }
        }
        PartKind::Lure => {
            let sway = 0.15 * phase.sin();
            let y = |a: f32| (sway + 0.1) * a * a;
            let mid: Vec<(Vec2, f32)> = samples(12)
                .map(|u| u * 0.78)
                .map(|a| (at(a, y(a)), w * 0.22 * len))
                .collect();
            pen.limb(&mid, w_line * 0.8, 0.85);
            let bulb = at(0.8, (sway + 0.1) * 0.64);
            let r = w * (0.55 + 0.45 * s) * len;
            let disc: Vec<Vec2> = (0..16).map(|j| bulb + polar(j as f32 / 16.0 * TAU) * r).collect();
            pen.blank(&disc);
            // Its glowing bulb.
            let glow = Vec3::new(0.35, 1.0, 0.85).lerp(base_linear(t), 0.25);
            pen.wash_polygon(&disc, srgb(glow), 0.9);
            pen.circle(bulb, r, w_line, 0.9);
            // Its glow: short rays.
            for j in 0..10 {
                let a = j as f32 / 10.0 * TAU + 0.2;
                pen.path(
                    &[bulb + polar(a) * r * 1.5, bulb + polar(a) * r * (2.0 + s)],
                    w_line * 0.45,
                    0.4,
                );
            }
        }
        PartKind::Spine => {
            // A thorn; banded when bigger, then barbed.
            let y = |a: f32| 0.08 * smoothstep(0.5, 1.0, s) * a * a;
            let barbs = smoothstep(0.75, 0.9, s);
            let hw = |a: f32| {
                let barb = barbs
                    * (1.0 - (a * 5.0).fract() * 3.0).max(0.0)
                    * if a >= 0.25 { 1.0 } else { 0.0 };
                w * (1.0 - a) + w * 0.5 * barb * (1.0 - a)
            };
            let outline: Vec<Vec2> = samples(20)
                .map(|a| at(a, y(a) + hw(a)))
                .chain(samples(20).map(|a| 1.0 - a).map(|a| at(a, y(a) - hw(a))))
                .collect();
            pen.fill(&outline);
            pen.closed(&outline, w_line, 0.9);
            if s > 0.4 {
                for k in 1..6 {
                    let a = (k as f32 + 0.5) / 6.0;
                    pen.path(
                        &[at(a, y(a) - hw(a)), at(a, y(a) + hw(a))],
                        w_line * 0.5,
                        0.5,
                    );
                }
            }
        }
        PartKind::Jaw => {
            // A mandible that bows out and hooks in, toothed on the inside when bigger.
            let y = |a: f32| 0.12 * (PI * a).sin() - (0.18 + 0.12 * s) * a * a * a;
            let hw = |a: f32| w * (1.0 - a.powf(1.4) * 0.85);
            let outline: Vec<Vec2> = samples(20)
                .map(|a| at(a, y(a) + hw(a)))
                .chain(samples(20).map(|a| 1.0 - a).map(|a| at(a, y(a) - hw(a))))
                .collect();
            pen.fill(&outline);
            pen.closed(&outline, w_line * 1.1, 0.95);
            if s > 0.3 {
                let count = 3 + (s * 5.0).floor() as usize;
                for k in 0..count {
                    let a = 0.15 + 0.8 * (k as f32 + 0.5) / count as f32;
                    let h = w * (0.3 + 0.3 * s);
                    let inner = y(a) - hw(a);
                    let half = 0.4 / count as f32;
                    pen.path(
                        &[at(a - half, inner), at(a, inner - h), at(a + half, inner)],
                        w_line * 0.6,
                        0.75,
                    );
                }
            }
        }
        PartKind::Shell => {
            // A curved plate (thin scale, then growth lines, then a ridged plate).
            let rb = 1.0 / (0.08 + 0.25 * s);
            let bend = |c: f32| c * c / (2.0 * rb);
            let plate: Vec<Vec2> = samples(16)
                .map(|u| -w + 2.0 * w * u)
                .map(|c| at(1.0 - bend(c), c))
                .chain(
                    samples(16)
                        .map(|u| w - 2.0 * w * u)
                        .map(|c| at(-bend(c), c)),
                )
                .collect();
            pen.fill(&plate);
            pen.closed(&plate, w_line, 0.9);
            if s > 0.35 {
                let lines = (2.0 + 4.0 * s) as usize;
                for k in 1..lines {
                    let ac = k as f32 / lines as f32;
                    let arc: Vec<Vec2> = samples(10)
                        .map(|u| -w + 2.0 * w * u)
                        .map(|c| at(ac - bend(c), c))
                        .collect();
                    pen.path(&arc, w_line * 0.45, 0.45);
                }
            }
            if s > 0.7 {
                let mut c = -w + 0.33;
                while c < w {
                    pen.circle(at(0.75 - bend(c), c), 0.1 * len, w_line * 0.5, 0.6);
                    c += 0.67;
                }
            }
        }
    }
}
