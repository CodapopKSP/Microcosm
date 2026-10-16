//! God powers, for the viewer's Acts of God panel and its hidden debug panel: place organisms,
//! smite, heal or move them, start events where the player points, pour substances into the
//! water, and call events off. Nothing in a normal run uses these.

use glam::Vec2;

use crate::World;
use crate::environment::Substance;
use crate::events::EventEffects;
use crate::stats::EventKind;
use crate::genome::Genome;
use crate::organism::{DeathCause, Organism};
use crate::traits::Traits;

/// How far a pour of a substance spreads (units), unless told otherwise: the smallest pour.
pub const SUBSTANCE_RADIUS: f32 = 20.0;

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

    /// Strike `species` (or the named species it belongs to) with a plague, whatever its share
    /// of all life. False if that species already has one or has no members.
    pub fn act_plague(&mut self, species: u32) -> bool {
        let root = self.species.established_ancestor(species).unwrap_or(species);
        let Some(pop) = self.species.get(root).map(|s| s.population) else {
            return false;
        };
        if pop == 0 || self.happenings.plague_rate(root) > 0.0 {
            return false;
        }
        let time = self.time();
        self.happenings.add_plague(root, time, &self.config);
        let name = self.species.name(root).to_string();
        self.log(
            EventKind::Disaster,
            format!("Plague! A disease sweeps through {name}, {pop} strong."),
            None,
            Some(root),
        );
        true
    }

    /// Start an upwelling centred on `x`.
    pub fn act_upwelling(&mut self, x: f32) {
        let time = self.time();
        let start = self.happenings.start_upwelling(x, time, &self.config);
        let fx = EventEffects {
            start_upwelling: Some(start),
            ..Default::default()
        };
        self.apply_events(fx, time);
    }

    /// A meteor strikes at `at`: everything within the impact radius dies, minerals are
    /// released, and dust dims the sun for days. Returns how many it killed.
    pub fn act_meteor(&mut self, at: Vec2) -> usize {
        let time = self.time();
        let before = self.organisms.iter().filter(|o| o.dead.is_none()).count();
        self.happenings.start_impact_winter(time, &self.config);
        let fx = EventEffects {
            impact: Some(at),
            ..Default::default()
        };
        self.apply_events(fx, time);
        let after = self.organisms.iter().filter(|o| o.dead.is_none()).count();
        self.remove_dead();
        before - after
    }

    /// Start a rock wall rising from the seafloor at `x`.
    pub fn act_rock_wall(&mut self, x: f32) {
        let time = self.time();
        let start = self.happenings.start_barrier(x, time, &self.config);
        let fx = EventEffects {
            start_barrier: Some(start),
            ..Default::default()
        };
        self.apply_events(fx, time);
    }

    /// Start a heat wave (`warmer`) or a cold snap, over the whole sea.
    pub fn act_temperature_shift(&mut self, warmer: bool) {
        let time = self.time();
        let delta = self
            .happenings
            .start_temperature_shift(warmer, time, &self.config);
        let (what, verb) = if warmer {
            ("A heat wave", "warms")
        } else {
            ("A cold snap", "chills")
        };
        self.log(
            EventKind::Environment,
            format!("{what} {verb} the sea by up to {:.0} °C at the surface.", delta.abs()),
            None,
            None,
        );
    }

    /// Start the sea falling by `drop` (it stays low a while, then rises back). False if the
    /// sea is already changing.
    pub fn act_sea_level(&mut self, drop: f32) -> bool {
        if self
            .happenings
            .is_active(|e| matches!(e, crate::events::WorldEvent::SeaLevel { .. }))
        {
            return false;
        }
        let time = self.time();
        let drop = self.happenings.start_sea_level(drop, time, &self.config);
        self.log(
            EventKind::Environment,
            format!("The sea begins to fall, by {drop:.0} units."),
            None,
            None,
        );
        true
    }

    /// Start a storm, over the whole sea.
    pub fn act_storm(&mut self) {
        let time = self.time();
        self.happenings.start_storm(time, &self.config);
        self.log(
            EventKind::Environment,
            "A storm churns the sea.".into(),
            None,
            None,
        );
    }

    /// Pour a standard dose of `substance` into the water within `radius` of `at`: the same
    /// amount whatever the radius, so a wider pour is thinner.
    pub fn add_substance(&mut self, substance: Substance, at: Vec2, radius: f32) {
        self.env.add_substance(substance, at, radius, substance.dose());
    }

    /// How many living organisms a strike of radius `r` at `at` would reach (as a meteor's).
    pub fn organisms_within(&self, at: Vec2, r: f32) -> usize {
        self.organisms
            .iter()
            .filter(|o| o.dead.is_none() && o.pos.distance(at) < r + o.radius)
            .count()
    }

    /// The living organisms whose centres lie in the rectangle between `a` and `b`.
    pub fn organisms_in(&self, a: Vec2, b: Vec2) -> Vec<u64> {
        let (min, max) = (a.min(b), a.max(b));
        self.organisms
            .iter()
            .filter(|o| {
                o.dead.is_none()
                    && o.pos.x >= min.x
                    && o.pos.x <= max.x
                    && o.pos.y >= min.y
                    && o.pos.y <= max.y
            })
            .map(|o| o.id)
            .collect()
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
    use crate::environment::Substance;
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
    fn acts_of_god_strike_where_they_are_aimed() {
        let mut w = world();
        let at = open_water(&w);

        // A plague on the clicked organism's species, even a small one; not twice.
        let ids = w.spawn_at(w.ancestor, at, 3);
        let species = w.find(ids[0]).unwrap().species;
        let root = w.species.established_ancestor(species).unwrap_or(species);
        assert!(w.act_plague(species));
        assert!(w.happenings.plague_rate(root) > 0.0);
        assert!(!w.act_plague(species), "already has one");

        // An upwelling and a rock wall where they were aimed.
        w.act_upwelling(1234.0);
        w.act_rock_wall(4321.0);
        assert!(w.happenings.active.iter().any(|e| matches!(e,
            WorldEvent::Upwelling { x, .. } if *x == 1234.0)));
        assert!(w.happenings.active.iter().any(|e| matches!(e,
            WorldEvent::Barrier { x, .. } if *x == 4321.0)));

        // A meteor kills what's around the point and darkens the sky.
        let near = w.spawn_at(w.ancestor, at, 5);
        let killed = w.act_meteor(at);
        assert!(killed >= near.len(), "killed {killed}");
        assert!(near.iter().all(|id| w.find(*id).is_none()));
        assert!(w.happenings.is_active(|e| matches!(e, WorldEvent::ImpactWinter { .. })));
    }

    #[test]
    fn heat_waves_and_cold_snaps_change_the_whole_sea() {
        let mut w = world();
        let at = open_water(&w);
        let far = Vec2::new((at.x + w.config.world.width * 0.5) % w.config.world.width, at.y);
        let before = (w.env.temperature.sample(at), w.env.temperature.sample(far));
        w.act_temperature_shift(true);
        // Let it build up (it takes 15% of its length), then let the water update.
        let duration = w.config.events.temperature_shift_duration;
        if let Some(WorldEvent::TemperatureShift { started, until, .. }) =
            w.happenings.active.last_mut()
        {
            *started -= duration as f64 * 0.5;
            *until -= duration as f64 * 0.5;
        }
        for _ in 0..40 {
            w.step();
        }
        let after = (w.env.temperature.sample(at), w.env.temperature.sample(far));
        assert!(after.0 > before.0 + 3.0, "warmer here: {before:?} -> {after:?}");
        assert!(after.1 > before.1 + 3.0, "and far away: {before:?} -> {after:?}");
        assert!(w.call_off_event(w.happenings.active.len() - 1));
        w.act_temperature_shift(false);
        assert!(matches!(w.happenings.active.last(),
            Some(WorldEvent::TemperatureShift { delta, .. }) if *delta < 0.0));
    }

    #[test]
    fn storms_churn_and_stir_the_upper_water() {
        let mut w = world();
        let h = w.config.world.height;
        let x = open_water(&w).x;
        let surface = Vec2::new(x, h - 20.0);
        let before = w.env.nutrients.sample(surface);
        w.act_storm();
        if let Some(WorldEvent::Storm { started, until, .. }) = w.happenings.active.last_mut() {
            *started -= 600.0;
            *until -= 600.0;
        }
        w.step();
        let flow = crate::currents::storm_flow(
            surface,
            w.time() as f32,
            h,
            w.env.storm,
            &w.config.events,
            w.config.world.seed,
        );
        assert!(flow.length() > 1.0, "the water churns: {flow:?}");
        for _ in 0..600 {
            w.step();
        }
        let after = w.env.nutrients.sample(surface);
        assert!(after > before, "richer water stirred up: {before} -> {after}");
    }

    #[test]
    fn the_sea_falls_and_returns() {
        let mut w = world();
        let h = w.config.world.height;
        let drop = 300.0;
        // A swimmer just under the surface, in open water.
        let x = (1..20)
            .map(|k| w.config.world.width * 0.05 * k as f32)
            .find(|&x| w.terrain.floor_at(x) < h - drop - 400.0
                && w.terrain.is_free(Vec2::new(x, h - 10.0), 5.0)
                && w.terrain.is_free(Vec2::new(x, h - drop - 50.0), 5.0))
            .expect("open water");
        let swimmer = w.spawn_at(w.ancestor, Vec2::new(x, h - 5.0), 1)[0];
        // A quick fall (10 s, still slower than things drop), stepped through.
        w.config.events.sea_level_change = 10.0;
        let change = 10.0;
        assert!(w.act_sea_level(drop));
        assert!(!w.act_sea_level(drop), "one at a time");
        for _ in 0..(11.0 / w.config.time.dt) as usize {
            w.step();
        }
        let sea = w.sea_level();
        assert!((sea - (h - drop)).abs() < 1.0, "sea at {sea}");
        assert_eq!(w.env.sea_level, sea);
        // The drained water holds nothing; the swimmer was carried down with the surface.
        let air = Vec2::new(x, h - drop * 0.5);
        let i = w.env.oxygen.index_at(air);
        assert!(w.env.oxygen.solid[i] && w.env.oxygen.data[i] == 0.0);
        if let Some(o) = w.find(swimmer) {
            assert!(o.pos.y <= sea, "carried down with the surface: {} vs {sea}", o.pos.y);
        }
        // When the sea comes back, the water fills with ordinary seawater again.
        if let Some(WorldEvent::SeaLevel { started, low, .. }) = w.happenings.active.last_mut() {
            *started -= *low + change + 10.0;
        }
        w.step();
        w.step();
        assert_eq!(w.sea_level(), h);
        assert!(!w.env.oxygen.solid[i] && w.env.oxygen.data[i] > 0.5, "refilled");
    }

    #[test]
    fn organisms_out_of_the_water_suffocate() {
        let mut w = world();
        let h = w.config.world.height;
        // An open spot resting on rock, a few hundred units down.
        let spot = (1..400)
            .flat_map(|k| {
                let x = w.config.world.width * k as f32 / 400.0;
                (0..300).map(move |j| Vec2::new(x, h - 150.0 - j as f32 * 4.0))
            })
            .find(|&p| w.terrain.is_free(p, 3.0) && !w.terrain.is_free(p - Vec2::new(0.0, 6.0), 3.0))
            .expect("room on some rock");
        let id = w.spawn_at(w.ancestor, spot, 1)[0];
        assert!(w.act_sea_level(h - spot.y + 200.0));
        let change = w.config.events.sea_level_change as f64;
        if let Some(WorldEvent::SeaLevel { started, .. }) = w.happenings.active.last_mut() {
            *started -= change + 10.0;
        }
        let hp = w.find(id).map(|o| o.hp).unwrap();
        for _ in 0..15 {
            w.step();
        }
        match w.find(id) {
            Some(o) => {
                assert!(o.pos.y > w.sea_level(), "stranded on the rock");
                assert!(o.hp < hp, "suffocating: {hp} -> {}", o.hp);
            }
            None => {} // already dead
        }
    }

    #[test]
    fn substances_are_poured_where_aimed() {
        let mut w = world();
        let at = open_water(&w);
        let far = at + Vec2::new(2000.0, 0.0);
        let amount = |f: &crate::field::Field| f.total() * f.cell_area() as f64;
        for radius in [SUBSTANCE_RADIUS, 300.0] {
            let before = (amount(&w.env.nutrients), w.env.nutrients.sample(far));
            w.add_substance(Substance::Nutrients, at, radius);
            let after = (amount(&w.env.nutrients), w.env.nutrients.sample(far));
            let added = after.0 - before.0;
            let dose = Substance::Nutrients.dose() as f64;
            assert!((added - dose).abs() < dose * 1e-3, "radius {radius}: added {added}");
            assert_eq!(after.1, before.1, "nothing far away");
        }
        w.add_substance(Substance::Toxin, at, SUBSTANCE_RADIUS);
        assert!(w.env.toxin_present && w.env.toxin.sample(at) > 0.0);
    }

    #[test]
    fn organisms_in_a_box() {
        let mut w = world();
        let at = open_water(&w);
        let ids = w.spawn_at(w.ancestor, at, 4);
        let found = w.organisms_in(at - Vec2::splat(100.0), at + Vec2::splat(100.0));
        assert!(ids.iter().all(|id| found.contains(id)));
        assert!(w.organisms_in(at, at + Vec2::splat(0.001)).len() <= 1);
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
