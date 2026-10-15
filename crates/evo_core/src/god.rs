//! God powers, for the viewer's hidden debug panel: place organisms, smite, heal or move them,
//! and call off the world's events. Nothing in a normal run uses these.

use glam::Vec2;

use crate::World;
use crate::events::EventEffects;
use crate::genome::Genome;
use crate::organism::{DeathCause, Organism};
use crate::traits::Traits;

impl World {
    /// Place `count` well-fed adults with `genome` around `pos`, wherever there's open water
    /// (the first right at `pos` if it fits). They join the living species closest to the
    /// genome if one is close enough to count, else they found a new lineage. Returns the ids
    /// of those placed.
    pub fn spawn_at(&mut self, genome: Genome, pos: Vec2, count: u32) -> Vec<u64> {
        let traits = Traits::decode(&genome, &self.config);
        let r = traits.adult_radius;
        let species = self.species_for(&genome);
        let spread = r * 2.5 * (count as f32).sqrt();
        let mut placed = Vec::new();
        for n in 0..count {
            let mut rng = self.rng.fork();
            let spot = (0..40)
                .map(|k| {
                    if n == 0 && k == 0 {
                        pos
                    } else {
                        let reach = spread * (1.0 + k as f32 / 10.0);
                        pos + Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * reach
                    }
                })
                .find(|&p| self.terrain.is_free(p, r));
            let Some(p) = spot else { continue };
            let id = self.next_id;
            self.next_id += 1;
            let mass = traits.adult_mass;
            let mut o = Organism::new(
                id,
                0,
                species,
                0,
                genome,
                0,
                &self.config,
                p,
                mass,
                mass * traits.reserve_factor,
                rng,
                self.tick,
            );
            o.heading = o.rng.range(-std::f32::consts::PI, std::f32::consts::PI);
            self.species.on_birth(species, &self.config, self.tick);
            self.organisms.push(o);
            placed.push(id);
        }
        placed
    }

    /// The species a placed genome belongs to: the closest living one within the species
    /// threshold, else a new root lineage of its own.
    fn species_for(&mut self, genome: &Genome) -> u32 {
        let sc = &self.config.species;
        let closest = self
            .species
            .living()
            .map(|s| (s.id, genome.distance(&s.founder, sc.color_weight, sc.gene_cap)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match closest {
            Some((id, d)) if d <= sc.threshold => id,
            _ => self.species.add_root(genome, &self.config, self.tick),
        }
    }

    /// Kill these organisms on the spot (they leave carcasses as usual). Returns how many died.
    pub fn smite(&mut self, ids: &[u64]) -> usize {
        let mut killed = 0;
        for o in &mut self.organisms {
            if o.dead.is_none() && ids.contains(&o.id) {
                o.dead = Some(DeathCause::Catastrophe);
                killed += 1;
            }
        }
        if killed > 0 {
            self.remove_dead();
        }
        killed
    }

    /// Fill these organisms' health and energy. Returns how many were healed.
    pub fn heal(&mut self, ids: &[u64]) -> usize {
        let mut healed = 0;
        for o in &mut self.organisms {
            if o.dead.is_none() && ids.contains(&o.id) {
                o.hp = o.max_hp();
                o.energy = o.max_energy();
                healed += 1;
            }
        }
        healed
    }

    /// Pick an organism up and put it down at `pos`, at rest. False if it isn't alive or
    /// doesn't fit there.
    pub fn move_organism(&mut self, id: u64, pos: Vec2) -> bool {
        let Some(o) = self.organisms.iter_mut().find(|o| o.id == id && o.dead.is_none()) else {
            return false;
        };
        if !self.terrain.is_free(pos, o.radius) {
            return false;
        }
        o.pos = pos;
        o.prev_pos = pos;
        o.vel = Vec2::ZERO;
        true
    }

    /// End every active event (plagues, upwellings, impact winters, rock walls) and drop any
    /// that were about to start. Walls sink at once. Returns how many were ended.
    pub fn call_off_events(&mut self) -> usize {
        self.happenings.pending.clear();
        let ended = std::mem::take(&mut self.happenings.active);
        let count = ended.len();
        if count > 0 {
            let fx = EventEffects {
                ended,
                ..Default::default()
            };
            self.apply_events(fx, self.time());
        }
        count
    }

    /// End one active event (by its place in `happenings.active`), as if it had run its course:
    /// a wall sinks at once, an impact winter clears. Returns whether there was such an event.
    pub fn call_off_event(&mut self, index: usize) -> bool {
        if index >= self.happenings.active.len() {
            return false;
        }
        let ended = self.happenings.active.remove(index);
        let fx = EventEffects {
            ended: vec![ended],
            ..Default::default()
        };
        self.apply_events(fx, self.time());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use crate::events::{Trigger, WorldEvent};

    fn world() -> World {
        let mut config = Config::default();
        config.population.initial = 20;
        config.events.enabled = false;
        World::new(config)
    }

    /// Somewhere in open water near the surface.
    fn open_water(w: &World) -> Vec2 {
        let h = w.config.world.height;
        (1..20)
            .map(|k| Vec2::new(w.config.world.width * 0.05 * k as f32, h - 200.0))
            .find(|&p| w.terrain.is_free(p, 40.0))
            .expect("open water")
    }

    #[test]
    fn spawned_organisms_join_a_close_species_or_found_one() {
        let mut w = world();
        let at = open_water(&w);
        let n = w.organisms.len();
        // The ancestor's own genome joins the founding species.
        let ids = w.spawn_at(w.ancestor, at, 5);
        assert_eq!(ids.len(), 5);
        assert_eq!(w.organisms.len(), n + 5);
        let first = w.find(ids[0]).unwrap().clone();
        assert_eq!(first.pos, at, "the first goes where asked");
        assert_eq!(first.species, w.organisms[0].species);
        assert_eq!(first.energy, first.max_energy());
        // A very different genome founds a lineage of its own.
        let other = crate::genome::Genome::parse(
            "B:207060300000 C:10C040 D:00FF0000 G:400080000000 H:2030 \
             N:80808080808080808000 P:086010 000000 000000 000000 000000 000000 000000 000000 \
             R:FF00B0 T:FFFF",
        )
        .unwrap();
        let ids = w.spawn_at(other, at, 1);
        let s = w.find(ids[0]).unwrap().species;
        assert_ne!(s, first.species);
        assert!(w.species.get(s).unwrap().established);
        assert_eq!(w.species.get(s).unwrap().population, 1);
    }

    #[test]
    fn smite_heal_and_move() {
        let mut w = world();
        let (a, b) = (w.organisms[0].id, w.organisms[1].id);
        w.organisms[1].hp *= 0.2;
        w.organisms[1].energy = 0.0;
        assert_eq!(w.heal(&[b]), 1);
        let o = w.find(b).unwrap();
        assert_eq!((o.hp, o.energy), (o.max_hp(), o.max_energy()));
        let to = open_water(&w);
        assert!(w.move_organism(b, to));
        assert_eq!(w.find(b).unwrap().pos, to);
        assert!(!w.move_organism(b, Vec2::new(-50.0, -50.0)), "not outside the world");
        let n = w.organisms.len();
        assert_eq!(w.smite(&[a]), 1);
        assert!(w.find(a).is_none());
        assert_eq!(w.organisms.len(), n - 1);
        assert_eq!(w.smite(&[a]), 0, "already gone");
    }

    #[test]
    fn calling_off_events_ends_them_and_sinks_walls() {
        let mut w = world();
        for t in [Trigger::Upwelling, Trigger::Impact, Trigger::Barrier] {
            w.trigger(t);
        }
        for _ in 0..3 {
            w.step();
        }
        // Let the wall rise a while.
        if let Some(WorldEvent::Barrier { started, .. }) = w
            .happenings
            .active
            .iter_mut()
            .find(|e| matches!(e, WorldEvent::Barrier { .. }))
        {
            *started -= 3600.0;
        }
        w.step();
        let wall_x = w.happenings.active.iter().find_map(|e| match e {
            WorldEvent::Barrier { x, built, .. } => Some((*x, *built)),
            _ => None,
        });
        let (x, built) = wall_x.expect("a wall is up");
        let floor = w.terrain.floor_at(x);
        assert!(built > floor + 50.0, "the wall has risen: {built} over {floor}");
        let mid = Vec2::new(x, (floor + built) * 0.5);
        assert!(!w.terrain.is_free(mid, 1.0), "rock where the wall stands");
        w.trigger(Trigger::Plague);
        assert_eq!(w.call_off_events(), 3);
        assert!(w.happenings.active.is_empty() && w.happenings.pending.is_empty());
        assert!(w.terrain.is_free(mid, 1.0), "the wall is gone");
        assert_eq!(w.happenings.sun_factor(w.time()), 1.0);
    }

    #[test]
    fn one_event_can_be_called_off_alone() {
        let mut w = world();
        for t in [Trigger::Upwelling, Trigger::Impact] {
            w.trigger(t);
        }
        w.step();
        // Age the winter so its dust has thickened.
        if let Some(WorldEvent::ImpactWinter { started, .. }) = w
            .happenings
            .active
            .iter_mut()
            .find(|e| matches!(e, WorldEvent::ImpactWinter { .. }))
        {
            *started -= 600.0;
        }
        assert!(w.happenings.sun_factor(w.time()) < 1.0, "the sun is dimmed");
        let winter = w
            .happenings
            .active
            .iter()
            .position(|e| matches!(e, WorldEvent::ImpactWinter { .. }))
            .expect("an impact winter");
        assert!(w.call_off_event(winter));
        assert_eq!(w.happenings.sun_factor(w.time()), 1.0, "the sun is back");
        assert!(
            matches!(w.happenings.active[..], [WorldEvent::Upwelling { .. }]),
            "the upwelling goes on"
        );
        assert!(!w.call_off_event(5), "no such event");
    }
}
