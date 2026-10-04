//! Body parts: the part catalog, decoding the P chromosome, and laying parts out on the body.
//!
//! A genome has eight part slots of three bytes each:
//!
//! - **kind**: low nibble = part type (0 = empty slot), high nibble = symmetry
//!   (0 single, 1 mirrored pair, 2–5 radial ×3–×6).
//! - **size**: 0–255; strength, length, and how elaborate the part looks.
//! - **place**: high 3 bits = where (0–5 a body segment, 6 every segment, 7 on the tip of the
//!   previous part), low 5 bits = angle around the attachment point (0 = forward, 16 = back).
//!
//! Layout happens in body-local units: 1.0 = the body's radius, x points forward.
//! The simulation aggregates parts into traits once at birth; the renderer lays them out
//! to draw them.

use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};

use crate::genome::{Genome, gene};

pub const SLOTS: usize = 8;
/// Most part instances one organism can have (after symmetry and repetition).
pub const MAX_INSTANCES: usize = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PartKind {
    Flagellum = 1,
    Cilia,
    Tentacle,
    Frond,
    Fin,
    Fluke,
    Leg,
    Holdfast,
    Stalk,
    Eye,
    Antenna,
    Lure,
    Spine,
    Jaw,
    Shell,
}

/// Physical properties of a part kind at size 0 and size 1 (linear in between).
pub struct Spec {
    /// Length and width in body radii.
    pub length: (f32, f32),
    pub width: (f32, f32),
    /// Mass as a fraction of the body's, per unit of size.
    pub mass: f32,
    /// Running cost as a fraction of basal metabolism, per unit of size.
    pub upkeep: f32,
    /// Added drag per unit of size.
    pub drag: f32,
}

impl PartKind {
    pub const ALL: [PartKind; 15] = [
        PartKind::Flagellum,
        PartKind::Cilia,
        PartKind::Tentacle,
        PartKind::Frond,
        PartKind::Fin,
        PartKind::Fluke,
        PartKind::Leg,
        PartKind::Holdfast,
        PartKind::Stalk,
        PartKind::Eye,
        PartKind::Antenna,
        PartKind::Lure,
        PartKind::Spine,
        PartKind::Jaw,
        PartKind::Shell,
    ];

    pub fn from_code(code: u8) -> Option<PartKind> {
        let c = code & 0x0F;
        if c == 0 {
            None
        } else {
            Some(PartKind::ALL[c as usize - 1])
        }
    }

    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn name(self) -> &'static str {
        match self {
            PartKind::Flagellum => "flagellum",
            PartKind::Cilia => "cilia band",
            PartKind::Tentacle => "tentacle",
            PartKind::Frond => "frond",
            PartKind::Fin => "fin",
            PartKind::Fluke => "tail fluke",
            PartKind::Leg => "leg",
            PartKind::Holdfast => "holdfast",
            PartKind::Stalk => "stalk",
            PartKind::Eye => "eye",
            PartKind::Antenna => "antenna",
            PartKind::Lure => "lure",
            PartKind::Spine => "spine",
            PartKind::Jaw => "jaw",
            PartKind::Shell => "shell plate",
        }
    }

    pub fn plural(self) -> &'static str {
        match self {
            PartKind::Flagellum => "flagella",
            PartKind::Cilia => "cilia bands",
            PartKind::Tentacle => "tentacles",
            PartKind::Frond => "fronds",
            PartKind::Fin => "fins",
            PartKind::Fluke => "tail flukes",
            PartKind::Leg => "legs",
            PartKind::Holdfast => "holdfasts",
            PartKind::Stalk => "stalks",
            PartKind::Eye => "eyes",
            PartKind::Antenna => "antennae",
            PartKind::Lure => "lures",
            PartKind::Spine => "spines",
            PartKind::Jaw => "jaws",
            PartKind::Shell => "shell plates",
        }
    }

    /// What the part does, for the inspector.
    pub fn role(self) -> &'static str {
        match self {
            PartKind::Flagellum => "whips the water to swim (best for small bodies)",
            PartKind::Cilia => "beats slowly: a little thrust and filter feeding",
            PartKind::Tentacle => "grabs food and prey near its base and pulls them to the mouth",
            PartKind::Frond => "catches sunlight over a wide area",
            PartKind::Fin => "swims and steers (best for larger bodies)",
            PartKind::Fluke => "powerful swimming stroke (best for larger bodies)",
            PartKind::Leg => "rows through the water, walks along rock, grips against currents",
            PartKind::Holdfast => "anchors to rock and draws nutrients from it",
            PartKind::Stalk => "raises the parts attached to it",
            PartKind::Eye => "sees in the direction it faces",
            PartKind::Antenna => "smells food and neighbors in every direction",
            PartKind::Lure => "glows; creatures drawn to light swim toward it",
            PartKind::Spine => "hurts attackers",
            PartKind::Jaw => "bites (works best facing forward)",
            PartKind::Shell => "armor that blocks damage but weighs you down",
        }
    }

    /// Drawn on top of the body (the rest are drawn behind it).
    pub fn in_front(self) -> bool {
        matches!(self, PartKind::Eye | PartKind::Jaw | PartKind::Shell)
    }

    pub fn spec(self) -> Spec {
        let s = |length, width, mass, upkeep, drag| Spec {
            length,
            width,
            mass,
            upkeep,
            drag,
        };
        match self {
            PartKind::Flagellum => s((0.6, 2.4), (0.06, 0.1), 0.02, 0.12, 0.02),
            PartKind::Cilia => s((0.15, 0.4), (0.3, 0.8), 0.02, 0.1, 0.05),
            PartKind::Tentacle => s((0.7, 3.2), (0.08, 0.16), 0.05, 0.12, 0.12),
            PartKind::Frond => s((0.5, 2.7), (0.2, 0.7), 0.06, 0.05, 0.2),
            PartKind::Fin => s((0.35, 1.25), (0.3, 0.8), 0.06, 0.1, 0.04),
            PartKind::Fluke => s((0.35, 1.15), (0.5, 1.2), 0.1, 0.15, 0.03),
            PartKind::Leg => s((0.5, 1.8), (0.1, 0.16), 0.08, 0.12, 0.1),
            PartKind::Holdfast => s((0.3, 1.3), (0.3, 0.9), 0.06, 0.05, 0.15),
            PartKind::Stalk => s((0.4, 2.0), (0.07, 0.11), 0.03, 0.03, 0.04),
            PartKind::Eye => s((0.16, 0.6), (0.16, 0.6), 0.03, 0.15, 0.01),
            PartKind::Antenna => s((0.5, 2.7), (0.05, 0.1), 0.02, 0.08, 0.03),
            PartKind::Lure => s((0.5, 2.1), (0.2, 0.4), 0.03, 0.12, 0.05),
            PartKind::Spine => s((0.2, 1.0), (0.12, 0.24), 0.04, 0.06, 0.08),
            PartKind::Jaw => s((0.25, 0.85), (0.18, 0.38), 0.06, 0.1, 0.04),
            PartKind::Shell => s((0.08, 0.33), (0.5, 1.4), 0.12, 0.12, 0.1),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Segment(u8),
    EverySegment,
    OnPrevious,
}

/// One decoded part gene.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub slot: usize,
    pub kind: PartKind,
    /// 1 (single), 2 (mirrored pair), 3–6 (radial).
    pub count: u8,
    /// 0..1
    pub size: f32,
    pub anchor: Anchor,
    /// Radians, body-local (0 = forward, π = back).
    pub angle: f32,
}

impl Part {
    pub fn symmetry_label(&self) -> &'static str {
        match self.count {
            1 => "single",
            2 => "pair",
            3 => "ring of 3",
            4 => "ring of 4",
            5 => "ring of 5",
            _ => "ring of 6",
        }
    }
}

/// Byte offset of slot `k`'s kind byte.
#[inline]
pub fn slot_offset(k: usize) -> usize {
    gene::PARTS + 3 * k
}

pub fn symmetry_count(kind_byte: u8) -> u8 {
    1 + (kind_byte >> 4).min(5)
}

pub fn decode_anchor(place: u8) -> (Anchor, f32) {
    let where_ = place >> 5;
    let angle = (place & 31) as f32 / 32.0 * TAU;
    let anchor = match where_ {
        6 => Anchor::EverySegment,
        7 => Anchor::OnPrevious,
        k => Anchor::Segment(k),
    };
    (anchor, angle)
}

/// The non-empty parts of a genome, in slot order.
pub fn decode_parts(genome: &Genome) -> Vec<Part> {
    let mut parts = Vec::with_capacity(SLOTS);
    for slot in 0..SLOTS {
        let o = slot_offset(slot);
        let Some(kind) = PartKind::from_code(genome.0[o]) else {
            continue;
        };
        let (anchor, angle) = decode_anchor(genome.0[o + 2]);
        parts.push(Part {
            slot,
            kind,
            count: symmetry_count(genome.0[o]),
            size: genome.0[o + 1] as f32 / 255.0,
            anchor,
            angle,
        });
    }
    parts
}

/// Extra body length per additional segment, as a fraction of a one-segment body.
/// (`organism.wgsl` repeats these three constants to draw the body.)
pub const SEGMENT_LENGTH: f32 = 0.5;
/// Segments are drawn as overlapping beads this much longer than their share of the body,
/// which leaves a narrow waist between them.
pub const SEGMENT_OVERLAP: f32 = 1.12;
/// The head segment is this much wider than its neighbors.
pub const HEAD_WIDTH: f32 = 1.08;

/// Body shape in body-local units (1.0 = the body's radius, x = forward).
///
/// Elongation stretches the body (keeping its area) and tapers it toward the tail; each extra
/// segment adds length, so a four-segment worm is much longer than a one-segment blob.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyShape {
    /// 0..1 from the elongation gene.
    pub elongation: f32,
    pub segments: u8,
    /// Half-length along x and half-width (at the front) along y.
    pub half_length: f32,
    pub half_width: f32,
    /// How much narrower the tail end is than the front (0..1).
    pub taper: f32,
}

impl BodyShape {
    pub fn new(elongation: f32, segments: u8) -> BodyShape {
        let segments = segments.clamp(1, 6);
        let aspect = 1.0 + 3.0 * elongation;
        let extra = (segments - 1) as f32;
        BodyShape {
            elongation,
            segments,
            half_length: aspect.sqrt() * (1.0 + SEGMENT_LENGTH * extra),
            half_width: 1.0 / aspect.sqrt(),
            taper: 0.3 * elongation + if segments > 1 { 0.15 } else { 0.0 },
        }
    }

    pub fn from_genome(genome: &Genome) -> BodyShape {
        let elongation = genome.frac(gene::ELONGATION);
        let segments = 1 + (genome.get(gene::SEGMENTS) as u32 * 6 / 256) as u8;
        BodyShape::new(elongation, segments)
    }

    /// Half-width of the body at position `x` along it (narrower toward the tail).
    pub fn width_at(&self, x: f32) -> f32 {
        let x = x.clamp(-self.half_length, self.half_length);
        self.half_width * (1.0 - self.taper * (self.half_length - x) / (2.0 * self.half_length))
    }

    /// Center (x) and half-length of segment `k` (0 = head).
    pub fn segment(&self, k: u8) -> (f32, f32) {
        let n = self.segments.max(1) as f32;
        let half = self.half_length / n;
        let k = (k as f32).min(n - 1.0);
        (self.half_length - (2.0 * k + 1.0) * half, half)
    }

    /// Half-width of segment `k`'s bead.
    pub fn segment_width(&self, k: u8) -> f32 {
        let (cx, _) = self.segment(k);
        let head = if k == 0 && self.segments > 1 {
            HEAD_WIDTH
        } else {
            1.0
        };
        self.width_at(cx) * head
    }

    /// Point on segment `k`'s outline at angle `theta`, and the outward direction there.
    pub fn attach(&self, k: u8, theta: f32) -> (Vec2, f32) {
        let (cos, sin) = (theta.cos(), theta.sin());
        if self.segments <= 1 {
            let hl = self.half_length;
            let x = hl * cos;
            let w = self.width_at(x);
            return (Vec2::new(x, w * sin), (sin / w).atan2(cos / hl));
        }
        let (cx, half) = self.segment(k);
        let a = half * SEGMENT_OVERLAP;
        let b = self.segment_width(k);
        // Outward normal of an ellipse at parametric angle theta.
        (Vec2::new(cx + a * cos, b * sin), (sin / b).atan2(cos / a))
    }

    /// The front of the head (distance from the center along x).
    pub fn nose(&self) -> f32 {
        if self.segments <= 1 {
            self.half_length
        } else {
            let (cx, half) = self.segment(0);
            cx + half * SEGMENT_OVERLAP
        }
    }

    /// Half-width of the head.
    pub fn head_width(&self) -> f32 {
        if self.segments <= 1 {
            self.width_at(self.half_length * 0.5)
        } else {
            self.segment_width(0)
        }
    }
}

/// One drawn/functional copy of a part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    pub kind: PartKind,
    pub slot: usize,
    pub size: f32,
    /// Attachment point and direction (body-local).
    pub base: Vec2,
    pub dir: f32,
    pub length: f32,
    pub width: f32,
    /// +1 or −1 for mirrored copies (animation phase and curl direction).
    pub mirror: f32,
    /// Index among the copies of this part (animation phase).
    pub index: u8,
    /// Kind of the part this one is attached to, if any.
    pub parent: Option<PartKind>,
    /// Index (in the laid-out instances) of the part this one is attached to, if any: it sits
    /// on that part's tip and moves with it.
    pub attached_to: Option<usize>,
}

impl Instance {
    pub fn tip(&self) -> Vec2 {
        self.base + Vec2::from_angle(self.dir) * self.length
    }
}

fn lerp((a, b): (f32, f32), t: f32) -> f32 {
    a + (b - a) * t
}

fn wrap_angle(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

/// Directions of a part's copies around `theta` (mirrored pair or radial ring).
fn copy_angles(count: u8, theta: f32) -> impl Iterator<Item = (f32, f32, u8)> {
    (0..count).map(move |i| {
        if count == 2 {
            let m = if i == 0 { 1.0 } else { -1.0 };
            (theta * m, m, i)
        } else {
            (theta + i as f32 * TAU / count as f32, 1.0, i)
        }
    })
}

/// Lay out `parts` on `body`, appending instances to `out` (cleared first).
pub fn layout(parts: &[Part], body: &BodyShape, out: &mut Vec<Instance>) {
    out.clear();
    let mut previous: Option<(usize, usize)> = None; // range of the previous part's instances
    for part in parts {
        let spec = part.kind.spec();
        let length = lerp(spec.length, part.size);
        let width = lerp(spec.width, part.size);
        let start = out.len();
        let push = |out: &mut Vec<Instance>,
                    base: Vec2,
                    dir: f32,
                    mirror: f32,
                    index: u8,
                    parent: Option<usize>| {
            if out.len() < MAX_INSTANCES {
                // Eyes and shell plates sit on the surface; everything else grows outward.
                let base = match part.kind {
                    PartKind::Eye | PartKind::Shell => base - Vec2::from_angle(dir) * length * 0.35,
                    _ => base,
                };
                out.push(Instance {
                    kind: part.kind,
                    slot: part.slot,
                    size: part.size,
                    base,
                    dir: wrap_angle(dir),
                    length,
                    width,
                    mirror,
                    index,
                    parent: parent.map(|p| out[p].kind),
                    attached_to: parent,
                });
            }
        };
        match part.anchor {
            Anchor::Segment(_) | Anchor::EverySegment => {
                let segments: Vec<u8> = match part.anchor {
                    Anchor::Segment(k) => vec![k.min(body.segments - 1)],
                    _ => (0..body.segments).collect(),
                };
                for &k in &segments {
                    for (theta, mirror, index) in copy_angles(part.count, part.angle) {
                        let (point, dir) = body.attach(k, theta);
                        push(out, point, dir, mirror, index, None);
                    }
                }
            }
            Anchor::OnPrevious => {
                // Attach to every copy of the previous part (or the head if there is none).
                let theta = wrap_angle(part.angle);
                match previous {
                    Some((a, b)) if b > a => {
                        for p in a..b {
                            let parent = out[p];
                            let tip = parent.tip();
                            for (t, mirror, index) in copy_angles(part.count, theta) {
                                let dir = parent.dir + t * parent.mirror;
                                push(out, tip, dir, mirror * parent.mirror, index, Some(p));
                            }
                        }
                    }
                    _ => {
                        for (t, mirror, index) in copy_angles(part.count, theta) {
                            let (point, dir) = body.attach(0, t);
                            push(out, point, dir, mirror, index, None);
                        }
                    }
                }
            }
        }
        if out.len() > start {
            previous = Some((start, out.len()));
        }
    }
}

/// Plain-English description of one part (for the inspector).
pub fn describe(part: &Part, body: &BodyShape) -> String {
    let what = if part.count == 1 {
        format!("a {}", part.kind.name())
    } else if part.count == 2 {
        format!("a pair of {}", part.kind.plural())
    } else {
        format!("a ring of {} {}", part.count, part.kind.plural())
    };
    let deg = part.angle.to_degrees().round() as i32;
    let facing = match deg {
        0..=20 | 340..=360 => "facing forward".to_string(),
        160..=200 => "facing back".to_string(),
        d if d < 180 => format!("angled {d}°"),
        d => format!("angled {}°", 360 - d),
    };
    let place = match part.anchor {
        Anchor::Segment(k) if body.segments > 1 => {
            format!("on segment {}", k.min(body.segments - 1) + 1)
        }
        Anchor::Segment(_) => "on the body".to_string(),
        Anchor::EverySegment if body.segments > 1 => "on every segment".to_string(),
        Anchor::EverySegment => "on the body".to_string(),
        Anchor::OnPrevious => "on the tip of the previous part".to_string(),
    };
    format!("{what}, size {:.0}%, {place}, {facing}", part.size * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn genome_with(parts: &[(u8, u8, u8)]) -> Genome {
        let mut g = Genome::parse(&crate::config::Config::default().population.ancestor).unwrap();
        for (k, &(kind, size, place)) in parts.iter().enumerate() {
            let o = slot_offset(k);
            g.0[o] = kind;
            g.0[o + 1] = size;
            g.0[o + 2] = place;
        }
        g
    }

    #[test]
    fn empty_slots_decode_to_nothing() {
        let g = genome_with(&[]);
        assert!(decode_parts(&g).is_empty());
    }

    #[test]
    fn symmetry_and_repetition_multiply_instances() {
        // A pair of legs on every segment of a 4-segment body = 8 legs.
        let mut g = genome_with(&[(0x10 | PartKind::Leg.code(), 0x80, 6 << 5 | 8)]);
        g.0[gene::SEGMENTS] = 0x90; // 1 + 0x90*6/256 = 4 segments
        let body = BodyShape::from_genome(&g);
        assert_eq!(body.segments, 4);
        let mut out = Vec::new();
        layout(&decode_parts(&g), &body, &mut out);
        assert_eq!(out.len(), 8);
        assert!(out.iter().all(|i| i.kind == PartKind::Leg));
        // Mirrored: half point up, half down.
        assert_eq!(out.iter().filter(|i| i.dir > 0.0).count(), 4);
    }

    #[test]
    fn parts_can_attach_to_parts() {
        // A stalk facing forward with an eye on its tip.
        let g = genome_with(&[
            (PartKind::Stalk.code(), 0x80, 0),
            (PartKind::Eye.code(), 0x80, 7 << 5),
        ]);
        let body = BodyShape::from_genome(&g);
        let mut out = Vec::new();
        layout(&decode_parts(&g), &body, &mut out);
        assert_eq!(out.len(), 2);
        let (stalk, eye) = (out[0], out[1]);
        assert_eq!(eye.parent, Some(PartKind::Stalk));
        assert_eq!(eye.attached_to, Some(0), "it rides on the stalk's tip");
        assert_eq!(stalk.attached_to, None);
        assert!(eye.base.x > stalk.base.x, "eye sits out on the stalk");
    }

    #[test]
    fn elongated_bodies_keep_their_area() {
        let mut g = genome_with(&[]);
        g.0[gene::ELONGATION] = 0xFF;
        let b = BodyShape::from_genome(&g);
        assert!((b.half_length * b.half_width - 1.0).abs() < 1e-5);
        assert!(b.half_length > 1.9);
        assert!(
            b.width_at(-b.half_length) < b.width_at(b.half_length),
            "tapers to the tail"
        );
        // Front attachment is at the nose.
        let (p, dir) = b.attach(0, 0.0);
        assert!((p.x - b.half_length).abs() < 1e-5 && dir.abs() < 1e-5);
    }

    #[test]
    fn segments_lengthen_the_body() {
        let one = BodyShape::new(0.3, 1);
        let two = BodyShape::new(0.3, 2);
        let four = BodyShape::new(0.3, 4);
        assert!(two.half_length > one.half_length * 1.4);
        assert!(four.half_length > two.half_length * 1.5);
        // Parts on the sides of each segment sit on that segment.
        let (p0, _) = four.attach(0, std::f32::consts::FRAC_PI_2);
        let (p3, _) = four.attach(3, std::f32::consts::FRAC_PI_2);
        assert!(
            p0.x > 0.0 && p3.x < 0.0 && p0.y > p3.y,
            "head is wider than the tail"
        );
        assert!(four.nose() > four.half_length);
    }
}
