//! Sampling profile of the simulation: load a save, run ticks under a timer-signal profiler,
//! print the functions with the most self and inclusive time, and write a flamegraph.
//!
//! ```text
//! cargo run --release -p evo_core --example profile -- saves/stable_ledge_6y.bin 1500
//! ```
use std::collections::HashMap;

use evo_core::World;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or("saves/stable_ledge_6y.bin".into());
    let ticks: u64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(1500);
    let mut world = World::load(std::path::Path::new(&path)).unwrap();
    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(997)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
        .unwrap();
    for _ in 0..ticks {
        world.step();
    }
    let report = guard.report().build().unwrap();
    let clean = |s: String| {
        let s = s.split("::h").next().unwrap_or(&s).to_string();
        s.replace("evo_core::", "")
    };
    let mut own: HashMap<String, isize> = HashMap::new();
    let mut incl: HashMap<String, isize> = HashMap::new();
    let mut total = 0isize;
    for (frames, &n) in &report.data {
        total += n;
        // Innermost first; each frame may hold several inlined symbols (innermost first).
        let names: Vec<String> = frames
            .frames
            .iter()
            .flat_map(|f| f.iter().map(|s| clean(s.name())))
            .collect();
        // Self time by the innermost frame in our own source, as file:line plus function.
        let here = frames.frames.iter().flat_map(|f| f.iter()).find(|s| {
            s.filename.as_ref().is_some_and(|f| f.to_string_lossy().contains("evo_core/src"))
        });
        if let Some(sym) = here {
            let file = sym.filename.as_ref().unwrap().to_string_lossy();
            let file = file.rsplit('/').next().unwrap().to_string();
            let key = format!("{file}:{} {}", sym.lineno.unwrap_or(0), clean(sym.name()));
            *own.entry(key).or_default() += n;
        }
        let mut seen = std::collections::HashSet::new();
        for name in names {
            if seen.insert(name.clone()) {
                *incl.entry(name).or_default() += n;
            }
        }
    }
    let show = |title: &str, m: &HashMap<String, isize>, k: usize| {
        println!("{title} ({total} samples)");
        let mut v: Vec<_> = m.iter().collect();
        v.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        for (name, n) in v.into_iter().take(k) {
            let name: String = name.chars().take(110).collect();
            println!("  {:5.1}%  {name}", *n as f64 * 100.0 / total as f64);
        }
    };
    show("self time (first non-library frame)", &own, 45);
    show("inclusive time", &incl, 70);
    if let Ok(f) = std::fs::File::create("profile.svg") {
        report.flamegraph(f).ok();
    }
}
