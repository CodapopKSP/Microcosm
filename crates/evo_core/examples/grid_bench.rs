use evo_core::grid::{Entry, SpatialGrid};
use evo_core::rng::Rng;
use glam::Vec2;
use std::time::Instant;

fn main() {
    let mut rng = Rng::new(1);
    let items: Vec<Entry> = (0..25_000)
        .map(|i| Entry {
            index: i,
            pos: Vec2::new(rng.range(0.0, 16000.0), 6000.0 - rng.range(0.0, 50.0)),
            radius: rng.range(1.5, 3.0),
            ..Default::default()
        })
        .collect();
    let mut grid = SpatialGrid::new(16000.0, 6000.0, 16.0, 4.0, 4);
    let n = 200;
    let t = Instant::now();
    for _ in 0..n {
        grid.rebuild(&items);
    }
    println!(
        "rebuild: {:.3} ms",
        t.elapsed().as_secs_f64() * 1000.0 / n as f64
    );
    let mut carcass = SpatialGrid::new(16000.0, 6000.0, 64.0, 4.0, 3);
    let t = Instant::now();
    for _ in 0..n {
        carcass.rebuild(&[]);
    }
    println!(
        "empty carcass rebuild: {:.3} ms",
        t.elapsed().as_secs_f64() * 1000.0 / n as f64
    );
}
