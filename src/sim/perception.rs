//! Sensor ray perception for genome v2 ([Genome_Definition.md](../../docs/Genome_Definition.md)).

use bevy::prelude::*;

use crate::config::GeneticsDecodeV2;
use crate::genome::{ChemicalObservation, GenomeV2};
use crate::genome::PhenotypeV2;
use crate::sim::light::LightField;
use crate::sim::minerals::MineralField;

/// One enabled sensor eye (mask bit → heading + index × 45° clockwise).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SensorEye {
    /// Mask bit index `0..ray_count-1` (MSB = 0).
    pub index: u8,
    /// World-space ray direction (unit).
    pub direction: Vec2,
}

/// Closest hit along one sensor ray.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RayHit {
    pub distance: f32,
    pub agent_index: Option<usize>,
    pub light_at_hit: f32,
    pub chemical: ChemicalObservation,
}

/// Cached ray sample for debug draw and behavior (G5).
#[derive(Debug, Clone, PartialEq)]
pub struct SenseRaySample {
    pub eye_index: u8,
    pub origin: Vec2,
    pub endpoint: Vec2,
    pub half_width_deg: f32,
    pub hit: RayHit,
}

/// Cached perception written by [`super::systems::sense_rays`](crate::sim::systems::sense_rays) on scan ticks.
#[derive(Component, Clone, Default, PartialEq)]
pub struct SensePerception {
    pub rays: Vec<SenseRaySample>,
    pub pursue_matched: bool,
    pub avoid_matched: bool,
    /// Bearing (radians) to closest pursue match, if any.
    pub pursue_bearing: Option<f32>,
    /// Bearing (radians) to closest avoid match, if any.
    pub avoid_bearing: Option<f32>,
}

/// Corpse snapshot for ray queries (signature `0x0200`).
#[derive(Debug, Clone, Copy)]
pub struct CorpseSenseTarget {
    pub position: Vec2,
    pub radius: f32,
}

/// Agent snapshot for spatial queries (index into parallel arrays).
#[derive(Debug, Clone)]
pub struct AgentSenseTarget {
    pub entity: Entity,
    pub position: Vec2,
    pub radius: f32,
    pub genome: GenomeV2,
    pub self_light: f32,
}

/// Enabled eyes from sensor mask (`bit i` → heading − i×45° clockwise).
pub fn sensor_eyes_from_mask(mask: u8, heading: f32, ray_count: u8) -> Vec<SensorEye> {
    let n = ray_count.max(1).min(8);
    let mut eyes = Vec::new();
    for i in 0..n {
        let bit = 1_u8 << (7 - i);
        if mask & bit != 0 {
            let offset = -(i as f32) * std::f32::consts::FRAC_PI_4;
            eyes.push(SensorEye {
                index: i,
                direction: Vec2::from_angle(heading + offset),
            });
        }
    }
    eyes
}

/// Half-width of cone in radians from decoded per-eye width in degrees.
pub fn cone_half_angle_rad(half_width_deg: f32) -> f32 {
    (half_width_deg.max(0.0) * std::f32::consts::PI / 180.0).max(1e-6)
}

/// World bearing (radians) toward the stimulus that triggered a ray hit.
pub fn stimulus_bearing(
    ray: &SenseRaySample,
    origin: Vec2,
    targets: &[AgentSenseTarget],
) -> f32 {
    if let Some(idx) = ray.hit.agent_index {
        if let Some(t) = targets.get(idx) {
            let delta = t.position - origin;
            if delta.length_squared() > 1e-8 {
                return delta.y.atan2(delta.x);
            }
        }
    }
    let dir = ray.endpoint - ray.origin;
    if dir.length_squared() > 1e-8 {
        dir.y.atan2(dir.x)
    } else {
        0.0
    }
}

/// Whether `target` lies inside the sensor cone from `origin` along `direction`.
pub fn point_in_sensor_cone(
    origin: Vec2,
    direction: Vec2,
    reach: f32,
    half_width_rad: f32,
    target: Vec2,
    target_radius: f32,
) -> Option<f32> {
    let dir = direction.normalize_or_zero();
    if dir.length_squared() <= 1e-8 || reach <= 1e-6 {
        return None;
    }
    let to = target - origin;
    let along = to.dot(dir);
    if along <= 0.0 || along > reach + target_radius {
        return None;
    }
    let lateral = (to - dir * along).length();
    let angular_limit = along * half_width_rad.tan();
    if lateral <= angular_limit + target_radius {
        Some((along - target_radius).max(0.0))
    } else {
        None
    }
}

fn chemical_at_point(
    light_field: &LightField,
    minerals: &MineralField,
    point: Vec2,
    self_light: f32,
) -> ChemicalObservation {
    let sample_light = light_field.sample(point);
    ChemicalObservation {
        dead_organism: false,
        minerals_raw: minerals.minerals_raw_at(point),
        light_delta: sample_light - self_light,
    }
}

/// Cast one ray and return the closest hit among `targets` (excluding `self_index`).
pub fn cast_sensor_ray(
    origin: Vec2,
    eye: SensorEye,
    reach: f32,
    half_width_deg: f32,
    self_index: usize,
    targets: &[AgentSenseTarget],
    corpses: &[CorpseSenseTarget],
    corpses_enabled: bool,
    light_field: &LightField,
    minerals: &MineralField,
    self_light: f32,
) -> RayHit {
    let half_rad = cone_half_angle_rad(half_width_deg);
    let mut best: Option<(f32, usize)> = None;

    for (i, t) in targets.iter().enumerate() {
        if i == self_index {
            continue;
        }
        if let Some(dist) = point_in_sensor_cone(origin, eye.direction, reach, half_rad, t.position, t.radius)
        {
            if best.map(|(d, _)| dist < d).unwrap_or(true) {
                best = Some((dist, i));
            }
        }
    }

    let mut best_corpse: Option<(f32, usize)> = None;
    if corpses_enabled {
        for (i, c) in corpses.iter().enumerate() {
            if let Some(dist) =
                point_in_sensor_cone(origin, eye.direction, reach, half_rad, c.position, c.radius)
            {
                if best_corpse.map(|(d, _)| dist < d).unwrap_or(true) {
                    best_corpse = Some((dist, i));
                }
            }
        }
    }

    let endpoint = origin + eye.direction * reach;
    let (distance, agent_index, hit_pos, corpse_hit) = match (best, best_corpse) {
        (Some((da, _ia)), Some((dc, _))) if dc < da => {
            (dc, None, origin + eye.direction * dc, true)
        }
        (Some((da, ia)), _) => (da, Some(ia), origin + eye.direction * da, false),
        (None, Some((dc, _))) => (dc, None, origin + eye.direction * dc, true),
        (None, None) => (reach, None, endpoint, false),
    };

    let mut chemical = chemical_at_point(light_field, minerals, hit_pos, self_light);
    if corpse_hit {
        chemical.dead_organism = true;
    }

    RayHit {
        distance,
        agent_index,
        light_at_hit: light_field.sample(hit_pos),
        chemical,
    }
}

/// Cast all enabled eyes for one observer.
pub fn perceive_agent(
    self_index: usize,
    origin: Vec2,
    heading: f32,
    phenotype: &PhenotypeV2,
    decode: &GeneticsDecodeV2,
    targets: &[AgentSenseTarget],
    corpses: &[CorpseSenseTarget],
    corpses_enabled: bool,
    light_field: &LightField,
    minerals: &MineralField,
    self_light: f32,
) -> Vec<SenseRaySample> {
    let eyes = sensor_eyes_from_mask(phenotype.sensor_mask, heading, decode.sensor_ray_count);
    eyes.into_iter()
        .map(|eye| {
            let hit = cast_sensor_ray(
                origin,
                eye,
                phenotype.sensor_reach_per_eye,
                phenotype.sensor_width_deg_per_eye,
                self_index,
                targets,
                corpses,
                corpses_enabled,
                light_field,
                minerals,
                self_light,
            );
            let endpoint = origin + eye.direction * phenotype.sensor_reach_per_eye;
            SenseRaySample {
                eye_index: eye.index,
                origin,
                endpoint,
                half_width_deg: phenotype.sensor_width_deg_per_eye,
                hit,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::genome::decode_v2;
    use crate::genome::GenomeV2;
    use crate::sim::minerals::MineralField;
    use crate::sim::world::WorldBounds;

    fn v2_config() -> SimConfig {
        SimConfig::load_from_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/genome_v2.example.toml"),
        )
        .unwrap()
    }

    #[test]
    fn mask_0x80_enables_single_eye_at_heading() {
        let eyes = sensor_eyes_from_mask(0x80, 0.0, 8);
        assert_eq!(eyes.len(), 1);
        assert_eq!(eyes[0].index, 0);
        assert!(eyes[0].direction.dot(Vec2::X) > 0.99);
    }

    #[test]
    fn eight_eye_mask_produces_eight_rays() {
        let eyes = sensor_eyes_from_mask(0xFF, 0.0, 8);
        assert_eq!(eyes.len(), 8);
    }

    #[test]
    fn ray_detects_agent_in_forward_cone() {
        let config = v2_config();
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let light = LightField::from_environment(&config.environment, &bounds);
        let minerals = MineralField::from_environment(&config.environment);
        let genome = GenomeV2::ancestor_from_config(&config).unwrap();
        let p = decode_v2(&genome, &config).unwrap();
        let mut ph = p.clone();
        ph.sensor_mask = 0x80;
        ph.sensor_eye_count = 1;
        ph.sensor_reach_per_eye = 500.0;
        ph.sensor_width_deg_per_eye = 45.0;

        let targets = vec![
            AgentSenseTarget {
                entity: Entity::from_raw(1),
                position: Vec2::new(50.0, 0.0),
                radius: 4.0,
                genome: genome.clone(),
                self_light: 0.5,
            },
            AgentSenseTarget {
                entity: Entity::from_raw(2),
                position: Vec2::ZERO,
                radius: 4.0,
                genome: genome.clone(),
                self_light: 0.5,
            },
        ];

        let d2 = config.genetics.decode_v2.as_ref().unwrap();
        let samples = perceive_agent(
            1,
            Vec2::ZERO,
            0.0,
            &ph,
            d2,
            &targets,
            &[],
            false,
            &light,
            &minerals,
            0.5,
        );
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].hit.agent_index, Some(0));
    }

    #[test]
    fn ray_hits_corpse_for_dead_organism_signature() {
        let config = v2_config();
        let bounds = WorldBounds::from_world_size(100.0, 100.0);
        let light = LightField::from_environment(&config.environment, &bounds);
        let minerals = MineralField::from_environment(&config.environment);
        let genome = GenomeV2::ancestor_from_config(&config).unwrap();
        let p = decode_v2(&genome, &config).unwrap();
        let mut ph = p.clone();
        ph.sensor_mask = 0x80;
        ph.sensor_eye_count = 1;
        ph.sensor_reach_per_eye = 200.0;
        ph.sensor_width_deg_per_eye = 45.0;
        let d2 = config.genetics.decode_v2.as_ref().unwrap();
        let corpses = vec![CorpseSenseTarget {
            position: Vec2::new(80.0, 0.0),
            radius: 6.0,
        }];
        let samples = perceive_agent(
            0,
            Vec2::ZERO,
            0.0,
            &ph,
            d2,
            &[],
            &corpses,
            true,
            &light,
            &minerals,
            0.5,
        );
        assert_eq!(samples.len(), 1);
        assert!(samples[0].hit.chemical.dead_organism);
    }

    #[test]
    fn minerals_raw_nonzero_inside_enabled_vent_field() {
        let mut config = v2_config();
        config.environment.minerals_enabled = true;
        config.environment.vents = vec![crate::config::VentZone {
            center: [0.0, 0.0],
            radius: 50.0,
            vent_flux: 0.04,
        }];
        let field = MineralField::from_environment(&config.environment);
        assert!(field.minerals_raw_at(Vec2::ZERO) > 0);
    }

    #[test]
    fn blind_mask_yields_no_rays() {
        let config = v2_config();
        let eyes = sensor_eyes_from_mask(0x00, 0.0, 8);
        assert!(eyes.is_empty());
        let bounds = WorldBounds::from_world_size(100.0, 100.0);
        let light = LightField::from_environment(&config.environment, &bounds);
        let minerals = MineralField::from_environment(&config.environment);
        let genome = GenomeV2::ancestor_from_config(&config).unwrap();
        let p = decode_v2(&genome, &config).unwrap();
        let d2 = config.genetics.decode_v2.as_ref().unwrap();
        let samples = perceive_agent(
            0,
            Vec2::ZERO,
            0.0,
            &p,
            d2,
            &[],
            &[],
            false,
            &light,
            &minerals,
            0.5,
        );
        assert!(samples.is_empty());
    }
}
