//! Benchmark: load a save, run a fixed number of ticks, print ms per tick by phase and a
//! fingerprint of the final state (to check that an optimization didn't change the results).
//!
//! ```text
//! cargo run --release -p evo_core --example bench -- saves/stable_ledge_6y.bin 3000
//! ```
use evo_core::World;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or("saves/stable_ledge_6y.bin".into());
    let ticks: u64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(3000);
    let mut world = World::load(std::path::Path::new(&path)).unwrap();
    let start = std::time::Instant::now();
    for _ in 0..ticks {
        world.step();
    }
    let wall = start.elapsed().as_secs_f64();
    for (name, ms) in world.timings.per_tick_ms() {
        println!("  {name:12} {ms:6.3}");
    }
    println!(
        "{ticks} ticks in {wall:.2}s: {:.3} ms/tick, pop {}",
        wall * 1000.0 / ticks as f64,
        world.organisms.len()
    );
    // FNV-1a over the positions, energies and genomes of every organism.
    let mut h: u64 = 0xcbf29ce484222325;
    let mut eat = |b: &[u8]| {
        for &x in b {
            h = (h ^ x as u64).wrapping_mul(0x100000001b3);
        }
    };
    for o in &world.organisms {
        eat(&o.id.to_le_bytes());
        eat(&o.pos.x.to_bits().to_le_bytes());
        eat(&o.pos.y.to_bits().to_le_bytes());
        eat(&o.energy.to_bits().to_le_bytes());
        eat(&o.genome.0);
    }
    for c in &world.carcasses {
        eat(&c.pos.x.to_bits().to_le_bytes());
        eat(&c.energy.to_bits().to_le_bytes());
    }
    println!("fingerprint {h:016x}");
    if let Some(out) = args.next() {
        world.save(std::path::Path::new(&out)).unwrap();
    }
}
