//! Temporary analysis: who lives near the permanent vents in a saved world?
use evo_core::World;
use glam::Vec2;

fn main() {
    let path = std::env::args().nth(1).expect("save path");
    let w = World::load(std::path::Path::new(&path)).unwrap();
    let h = w.config.world.height;
    for (k, &v) in w.terrain.vent_sites.iter().enumerate() {
        for radius in [150.0f32, 350.0, 700.0] {
            let near: Vec<_> = w.organisms.iter().filter(|o| o.pos.distance(v) < radius).collect();
            let n = near.len().max(1) as f32;
            let mean = |f: &dyn Fn(&evo_core::Organism) -> f32| near.iter().map(|o| f(o)).sum::<f32>() / n;
            println!(
                "vent {k} (depth {:.0}) within {radius:>3}: {:4} organisms, chemo {:.2} photo {:.2} scav {:.2} hunt {:.2}, prefers {:.1}°C ±{:.1}, water {:.1}°C, shells {:.1}",
                h - v.y, near.len(), mean(&|o| o.traits.chemo), mean(&|o| o.traits.photo),
                mean(&|o| o.traits.scavenge), mean(&|o| o.traits.hunt), mean(&|o| o.traits.temp_pref),
                mean(&|o| o.traits.temp_tol), mean(&|o| o.temperature),
                mean(&|o| evo_core::parts::decode_parts(&o.genome).iter()
                    .filter(|p| p.kind == evo_core::parts::PartKind::Shell).map(|p| p.count as f32).sum::<f32>()),
            );
        }
    }
    // The ledge beside the shelf vent.
    let v = w.terrain.vent_sites[0];
    let ledge = |o: &&evo_core::Organism| o.pos.x > v.x + 120.0 && o.pos.x < v.x + 280.0 && (o.pos.y - (v.y + 40.0)).abs() < 110.0;
    let on: Vec<_> = w.organisms.iter().filter(ledge).collect();
    println!("on the warm ledge: {} organisms; chemo-main {}", on.len(), on.iter().filter(|o| o.traits.main_diet() == 1).count());
    // Where the chemo-main organisms are.
    let chemo: Vec<Vec2> = w.organisms.iter().filter(|o| o.traits.main_diet() == 1).map(|o| o.pos).collect();
    let mut bins = std::collections::BTreeMap::new();
    for p in &chemo { *bins.entry(((p.x / 1000.0) as i32, ((h - p.y) / 300.0) as i32)).or_insert(0) += 1; }
    let mut top: Vec<_> = bins.into_iter().collect();
    top.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for ((x, d), n) in top.iter().take(6) { println!("  chemo-main {n:4} around x {}..{}k, depth {}..{}", x, x + 1, d * 300, d * 300 + 300); }
}
