//! Temporary: would a nutrient-seeking drive be useful? Nutrient field and organism stats.
use glam::Vec2;
fn pct(v: &mut Vec<f32>, p: f32) -> f32 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() - 1) as f32 * p) as usize]
}
fn main() {
    let w = evo_core::World::load(std::path::Path::new("saves/autosave.bin")).unwrap();
    let f = &w.env.nutrients;
    let h = w.config.world.height;
    let ocean = w.config.ocean_width();
    println!("time {:.2} h, pop {}", w.time() / 3600.0, w.organisms.len());
    println!("reference: surface {} deep {} (nutricline {})", w.config.chemistry.nutrient_surface, w.config.chemistry.nutrient_deep, w.config.chemistry.nutricline_depth);
    // By depth band: mean, and how much it varies sideways (coefficient of variation, min–max).
    for (lo, hi) in [(0.0, 100.0), (100.0, 300.0), (300.0, 900.0), (900.0, 2500.0), (2500.0, 6000.0)] {
        let mut vals = Vec::new();
        for y in 0..f.ny {
            let depth = h - (y as f32 + 0.5) * f.cell;
            if depth < lo || depth >= hi { continue; }
            for x in 0..f.nx {
                let px = (x as f32 + 0.5) * f.cell;
                let i = f.index(x, y);
                if !f.solid[i] && px < ocean { vals.push(f.data[i]); }
            }
        }
        if vals.is_empty() { continue; }
        let mean = vals.iter().sum::<f32>() / vals.len() as f32;
        let sd = (vals.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / vals.len() as f32).sqrt();
        println!("depth {lo:>5}-{hi:<5}: mean {mean:.3} sd {sd:.3} (cv {:.2}) p5 {:.3} p50 {:.3} p95 {:.3}", sd / mean.max(1e-6), pct(&mut vals, 0.05), pct(&mut vals, 0.5), pct(&mut vals, 0.95));
    }
    // At the organisms: concentration, and the steering pull a sense-based drive would feel.
    let mut here = Vec::new();
    let mut pull = Vec::new();
    let mut starved = 0;
    let mut growing = 0;
    let mut store = Vec::new();
    for o in &w.organisms {
        let p = o.pos;
        let c = f.sample(p);
        here.push(c);
        let d = (o.traits.sense_radii * o.radius).max(f.cell);
        let g = f.gradient(p, d) / (c + 1e-3);
        pull.push((g.length() * 30.0).min(1.0));
        let cap = o.max_nutrients(&w.config);
        store.push(o.nutrients / cap.max(1e-6));
        if o.mass < o.traits.adult_mass * 0.98 {
            growing += 1;
            if o.nutrients < 0.05 * cap { starved += 1; }
        }
    }
    let n = w.organisms.len() as f32;
    println!("at organisms: nutrients p5 {:.3} p50 {:.3} p95 {:.3}", pct(&mut here.clone(), 0.05), pct(&mut here.clone(), 0.5), pct(&mut here, 0.95));
    println!("drive pull (0..1): p25 {:.2} p50 {:.2} p75 {:.2} p95 {:.2}", pct(&mut pull.clone(), 0.25), pct(&mut pull.clone(), 0.5), pct(&mut pull.clone(), 0.75), pct(&mut pull, 0.95));
    println!("nutrient store (fraction full): p10 {:.2} p50 {:.2} p90 {:.2}", pct(&mut store.clone(), 0.1), pct(&mut store.clone(), 0.5), pct(&mut store, 0.9));
    println!("growing: {:.0}% of organisms; of them {:.0}% have an (almost) empty nutrient store", growing as f32 / n * 100.0, starved as f32 / growing.max(1) as f32 * 100.0);
    // Sideways at the surface: how far to water 50% richer than where an organism is?
    let mut dist = Vec::new();
    for o in w.organisms.iter().step_by(20) {
        let c = f.sample(o.pos);
        let mut best = f32::MAX;
        for k in 1..60 {
            let step = k as f32 * 32.0;
            for dir in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
                if f.sample(o.pos + dir * step) > 1.5 * c + 0.005 { best = best.min(step); }
            }
            if best < f32::MAX { break; }
        }
        dist.push(best.min(1920.0));
    }
    println!("distance to 50% richer water: p25 {:.0} p50 {:.0} p75 {:.0}", pct(&mut dist.clone(), 0.25), pct(&mut dist.clone(), 0.5), pct(&mut dist, 0.75));
}
