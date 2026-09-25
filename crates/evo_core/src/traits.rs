//! Decoded traits (phenotype) of a genome. Computed once at birth.

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::genome::{Genome, drive, gene};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Traits {
    pub adult_radius: f32,
    pub adult_mass: f32,

    pub armor: f32,
    pub spikes: f32,
    pub jaws: f32,
    /// −1 (sinks) .. +1 (floats), after armor weight.
    pub buoyancy: f32,
    /// Energy storage per unit of mass.
    pub reserve_factor: f32,
    /// Seconds.
    pub lifespan: f32,

    /// Diet capabilities after the generalist penalty.
    pub photo: f32,
    pub chemo: f32,
    pub scavenge: f32,
    pub hunt: f32,

    /// Fraction of max HP regenerated per second.
    pub regen: f32,
    /// Max HP per unit of mass.
    pub hp_factor: f32,

    /// Speed gene as a fraction; max speed also depends on current radius.
    pub speed: f32,
    /// Max speed multiplier (speed gene × scale × armor penalty); × √radius for units/s.
    pub speed_coeff: f32,
    /// Radians per second.
    pub turn_rate: f32,
    pub restlessness: f32,

    /// Signed drive weights (see `genome::drive`).
    pub drives: [f32; drive::COUNT],

    /// Newborn mass as a fraction of adult mass.
    pub birth_fraction: f32,
    pub brood: u32,
    /// Reserve fullness required to reproduce.
    pub repro_threshold: f32,

    /// Sense range in body radii (0 = blind).
    pub sense_radii: f32,
    /// Cosine of half the field of view.
    pub fov_cos: f32,
    pub fov_deg: f32,

    /// °C.
    pub temp_pref: f32,
    pub temp_tol: f32,

    /// Basal metabolism multiplier: 1 + Σ upkeep.
    pub upkeep: f32,
    /// Light absorptance of the body (0..1).
    pub absorptance: f32,
    /// Part of the absorptance due to photosynthetic pigment.
    pub pigment: f32,

    /// Display color, sRGB 0..1.
    pub color: [f32; 3],
    pub hue: u8,
    pub saturation: u8,
    pub pattern: u8,
}

/// Diet capabilities with the generalist penalty (docs/DESIGN.md §2): each diet is
/// weakened in proportion to the investment in the others, `c_i = x_i (1 − k Σ_{j≠i} x_j)`.
/// Entry into a new diet is linear (gentle), while true generalists lose overall.
pub fn diet_capabilities(genes: [f32; 4], penalty: f32) -> [f32; 4] {
    let total: f32 = genes.iter().sum();
    genes.map(|x| (x * (1.0 - penalty * (total - x))).max(0.0))
}

/// HSV → sRGB with fixed value; hue in 0..1.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h6 = (h.rem_euclid(1.0)) * 6.0;
    let i = h6.floor() as i32;
    let f = h6 - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i.rem_euclid(6) {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl Traits {
    pub fn decode(genome: &Genome, config: &Config) -> Traits {
        let gc = &config.genes;
        let up = &config.upkeep;
        let light = &config.light;
        let f = |i: usize| genome.frac(i);

        let adult_radius =
            gc.min_radius * 2f32.powf(genome.get(gene::SIZE) as f32 / gc.size_doubling);
        let adult_mass = adult_radius * adult_radius;

        let armor = f(gene::ARMOR);
        let spikes = f(gene::SPIKES);
        let jaws = f(gene::JAWS);
        let buoyancy_gene = genome.signed(gene::BUOYANCY);
        let buoyancy = (buoyancy_gene - armor * config.physics.armor_density).clamp(-1.0, 1.0);
        let reserves = f(gene::RESERVES);
        let longevity = f(gene::LONGEVITY);

        let [photo, chemo, scavenge, hunt] = diet_capabilities(
            [
                f(gene::PHOTO),
                f(gene::CHEMO),
                f(gene::SCAVENGE),
                f(gene::HUNT),
            ],
            gc.generalist_penalty,
        );

        let regen = f(gene::REGEN);
        let toughness = f(gene::TOUGHNESS);
        let speed = f(gene::SPEED);
        let agility = f(gene::AGILITY);
        let restlessness = f(gene::RESTLESSNESS);

        let mut drives = [0.0; drive::COUNT];
        for (k, d) in drives.iter_mut().enumerate() {
            *d = genome.signed(gene::DRIVES + k);
        }

        let birth_fraction =
            0.5 * 2f32.powf(-((255 - genome.get(gene::OFFSPRING_SIZE)) as f32) / 32.0);
        let brood = 1 + (genome.get(gene::BROOD) >> 5) as u32;
        let repro_threshold = 0.3 + 0.7 * f(gene::PATIENCE);

        let range = f(gene::SENSE_RANGE);
        let fov_frac = f(gene::FOV);
        let fov_deg = lerp(gc.fov_min_deg, 360.0, fov_frac);
        let fov_cos = if fov_deg >= 359.9 {
            -1.0
        } else {
            (fov_deg.to_radians() * 0.5).cos()
        };

        let tolerance = f(gene::TEMP_TOL);

        let upkeep = 1.0
            + up.armor * armor
            + up.spikes * spikes
            + up.jaws * jaws
            + up.buoyancy * f(gene::BUOYANCY)
            + up.reserves * reserves
            + up.longevity * longevity
            + up.photo * photo
            + up.chemo * chemo
            + up.scavenge * scavenge
            + up.hunt * hunt
            + up.regen * regen
            + up.toughness * toughness
            + up.speed * speed
            + up.agility * agility
            + up.senses * range * (0.5 + fov_frac)
            + up.tolerance * tolerance;

        let pigment = light.pigment_absorptance * photo;
        let absorptance = (light.body_absorptance + pigment).min(0.98);

        let hue = genome.get(gene::HUE);
        let saturation = genome.get(gene::SATURATION);
        let color = hsv_to_rgb(
            hue as f32 / 256.0,
            lerp(0.2, 1.0, saturation as f32 / 255.0),
            0.92,
        );

        Traits {
            adult_radius,
            adult_mass,
            armor,
            spikes,
            jaws,
            buoyancy,
            reserve_factor: lerp(gc.reserves_min, gc.reserves_max, reserves),
            lifespan: gc.lifespan_base
                * adult_radius.sqrt()
                * lerp(1.0, gc.longevity_max, longevity),
            photo,
            chemo,
            scavenge,
            hunt,
            regen: lerp(gc.regen_min, gc.regen_max, regen),
            hp_factor: lerp(gc.toughness_min, gc.toughness_max, toughness),
            speed,
            speed_coeff: speed
                * gc.speed_scale
                * (1.0 - config.physics.armor_speed_penalty * armor),
            turn_rate: lerp(gc.turn_min, gc.turn_max, agility),
            restlessness,
            drives,
            birth_fraction,
            brood,
            repro_threshold,
            sense_radii: range * gc.sense_radii,
            fov_cos,
            fov_deg,
            temp_pref: f(gene::TEMP_PREF) * gc.temp_max,
            temp_tol: lerp(gc.tolerance_min, gc.tolerance_max, tolerance),
            upkeep,
            absorptance,
            pigment,
            color,
            hue,
            saturation,
            pattern: genome.get(gene::PATTERN),
        }
    }

    #[inline]
    pub fn max_speed(&self, radius: f32) -> f32 {
        self.speed_coeff * radius.sqrt()
    }

    #[inline]
    pub fn is_motile(&self) -> bool {
        self.speed_coeff > 0.0
    }

    /// Dominant diet as an index 0..4 (photo, chemo, scavenge, hunt).
    pub fn main_diet(&self) -> usize {
        let caps = [self.photo, self.chemo, self.scavenge, self.hunt];
        let mut best = 0;
        for i in 1..4 {
            if caps[i] > caps[best] {
                best = i;
            }
        }
        best
    }
}

/// Plain-English meaning of one gene value, for the inspector.
pub fn describe_gene(index: usize, genome: &Genome, traits: &Traits, config: &Config) -> String {
    let v = genome.get(index);
    let pct = |x: f32| format!("{:.0}%", x * 100.0);
    let signed = |x: f32| {
        if x.abs() < 0.02 {
            "indifferent".to_string()
        } else if x > 0.0 {
            format!("toward {:+.0}%", x * 100.0)
        } else {
            format!("away {:+.0}%", x * 100.0)
        }
    };
    let gc = &config.genes;
    match index {
        gene::ARMOR => format!(
            "{} damage blocked",
            pct(traits.armor * config.feeding.armor_protection)
        ),
        gene::SPIKES => format!(
            "reflects {} of bites",
            pct(traits.spikes * config.feeding.spike_damage)
        ),
        gene::JAWS => format!("bite ×{:.1}", 0.2 + 1.8 * traits.jaws),
        gene::SIZE => format!("adult radius {:.1}", traits.adult_radius),
        gene::BUOYANCY => {
            let b = genome.signed(gene::BUOYANCY);
            if b.abs() < 0.05 {
                "neutral".into()
            } else if b > 0.0 {
                format!("floats {}", pct(b))
            } else {
                format!("sinks {}", pct(-b))
            }
        }
        gene::RESERVES => format!("stores {:.1}× body mass", traits.reserve_factor),
        gene::LONGEVITY => format!("lives ~{}", format_duration(traits.lifespan)),
        gene::HUE => format!("{:.0}°", v as f32 / 256.0 * 360.0),
        gene::SATURATION => pct(lerp(0.2, 1.0, v as f32 / 255.0)),
        gene::PATTERN => ["plain", "spots", "stripes", "rings"][(v >> 6) as usize].into(),
        gene::PHOTO => format!("{} effective", pct(traits.photo)),
        gene::CHEMO => format!("{} effective", pct(traits.chemo)),
        gene::SCAVENGE => format!("{} effective", pct(traits.scavenge)),
        gene::HUNT => format!("{} effective", pct(traits.hunt)),
        gene::REGEN => format!("{:.1}% HP/s", traits.regen * 100.0),
        gene::TOUGHNESS => format!("{:.2} HP per mass", traits.hp_factor),
        gene::SPEED => {
            if traits.speed <= 0.0 {
                "cannot swim".into()
            } else {
                format!("{:.1} u/s as adult", traits.max_speed(traits.adult_radius))
            }
        }
        gene::AGILITY => format!("turns {:.0}°/s", traits.turn_rate.to_degrees()),
        gene::RESTLESSNESS => pct(traits.restlessness),
        i if (gene::DRIVES..gene::DRIVES + drive::COUNT).contains(&i) => signed(genome.signed(i)),
        gene::OFFSPRING_SIZE => format!(
            "newborn {} of adult mass",
            pct_precise(traits.birth_fraction)
        ),
        gene::BROOD => format!("{} per brood", traits.brood),
        gene::PATIENCE => format!("breeds at {} reserves", pct(traits.repro_threshold)),
        gene::SENSE_RANGE => {
            if traits.sense_radii <= 0.0 {
                "blind".into()
            } else {
                format!(
                    "{:.0} radii ({:.0} u)",
                    traits.sense_radii,
                    (traits.sense_radii * traits.adult_radius).min(gc.sense_cap)
                )
            }
        }
        gene::FOV => format!("{:.0}°", traits.fov_deg),
        gene::TEMP_PREF => format!("{:.1} °C", traits.temp_pref),
        gene::TEMP_TOL => format!("±{:.1} °C", traits.temp_tol),
        _ => String::new(),
    }
}

fn pct_precise(x: f32) -> String {
    if x >= 0.1 {
        format!("{:.0}%", x * 100.0)
    } else {
        format!("{:.1}%", x * 100.0)
    }
}

/// `90 s`, `12 min`, `3.5 h`.
pub fn format_duration(seconds: f32) -> String {
    if seconds < 120.0 {
        format!("{seconds:.0} s")
    } else if seconds < 7200.0 {
        format!("{:.0} min", seconds / 60.0)
    } else {
        format!("{:.1} h", seconds / 3600.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ancestor() -> (Genome, Config) {
        let config = Config::default();
        (Genome::parse(&config.population.ancestor).unwrap(), config)
    }

    #[test]
    fn ancestor_decodes_as_designed() {
        let (g, config) = ancestor();
        let t = Traits::decode(&g, &config);
        assert!((t.adult_radius - 2.0).abs() < 1e-4);
        assert!((t.photo - 1.0).abs() < 1e-6);
        assert_eq!(t.hunt, 0.0);
        assert!(!t.is_motile());
        assert_eq!(t.brood, 1);
        assert!((t.birth_fraction - 0.5).abs() < 1e-6);
        assert!(t.buoyancy > 0.0);
        assert_eq!(t.sense_radii, 0.0);
        assert!(
            t.color[1] > t.color[0] && t.color[1] > t.color[2],
            "ancestor should be green"
        );
    }

    #[test]
    fn size_gene_doubles_every_0x20() {
        let (mut g, config) = ancestor();
        g.0[gene::SIZE] = 0x40;
        let r4 = Traits::decode(&g, &config).adult_radius;
        g.0[gene::SIZE] = 0x60;
        let r8 = Traits::decode(&g, &config).adult_radius;
        assert!((r4 - 4.0).abs() < 1e-3 && (r8 - 8.0).abs() < 1e-3);
        g.0[gene::SIZE] = 0xFF;
        assert!(Traits::decode(&g, &config).adult_radius > 240.0);
    }

    #[test]
    fn generalists_are_penalized() {
        let spec = diet_capabilities([1.0, 0.0, 0.0, 0.0], 0.6);
        let generalist = diet_capabilities([1.0, 0.0, 0.0, 1.0], 0.6);
        assert_eq!(spec[0], 1.0);
        assert!((generalist[0] - 0.4).abs() < 1e-6 && (generalist[3] - 0.4).abs() < 1e-6);
        assert!(
            generalist.iter().sum::<f32>() < 1.0,
            "generalists must lose overall"
        );
        // A small second diet costs the main diet little and works linearly.
        let trace = diet_capabilities([1.0, 0.0, 0.0, 0.05], 0.6);
        assert!(trace[0] > 0.96 && trace[3] > 0.015);
    }

    #[test]
    fn capabilities_cost_upkeep() {
        let (mut g, config) = ancestor();
        let base = Traits::decode(&g, &config).upkeep;
        g.0[gene::ARMOR] = 0xFF;
        assert!(Traits::decode(&g, &config).upkeep > base);
    }

    #[test]
    fn every_gene_has_a_description() {
        let (g, config) = ancestor();
        let t = Traits::decode(&g, &config);
        for i in 0..crate::genome::GENOME_LEN {
            assert!(!describe_gene(i, &g, &t, &config).is_empty(), "gene {i}");
        }
    }
}
