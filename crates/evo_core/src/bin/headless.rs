//! Headless runner: simulate without a window, print progress, write metrics, save the world.
//!
//! ```text
//! cargo run --release -p evo_core --bin evo_headless -- --hours 2 --report 300
//! ```

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use evo_core::organism::DeathCause;
use evo_core::traits::format_duration;
use evo_core::{Config, World};

#[derive(Parser)]
#[command(about = "Run the evolution simulation without rendering")]
struct Args {
    /// Config file.
    #[arg(long, default_value = "configs/default.toml")]
    config: PathBuf,
    /// Override the world seed.
    #[arg(long)]
    seed: Option<u64>,
    /// Sim hours to run (sim seconds = hours × 3600).
    #[arg(long, default_value_t = 0.5)]
    hours: f64,
    /// Sim seconds between progress lines.
    #[arg(long, default_value_t = 120.0)]
    report: f64,
    /// Write one JSON line per statistics sample.
    #[arg(long)]
    metrics: Option<PathBuf>,
    /// Save the world here when done.
    #[arg(long)]
    save: Option<PathBuf>,
    /// Resume from a saved world instead of starting fresh.
    #[arg(long)]
    load: Option<PathBuf>,
    /// Worker threads (default: all cores).
    #[arg(long)]
    threads: Option<usize>,
    /// Print the default config as TOML and exit.
    #[arg(long)]
    dump_config: bool,
    /// Print extra diagnostics (energy flow, depth, maturity) with each report.
    #[arg(long)]
    diag: bool,
    /// Trigger an event at the start (plague, upwelling, impact, barrier). Repeatable.
    #[arg(long)]
    trigger: Vec<evo_core::events::Trigger>,
}

fn main() {
    let args = Args::parse();
    if args.dump_config {
        print!("{}", toml::to_string_pretty(&Config::default()).unwrap());
        return;
    }
    if let Some(n) = args.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
            .ok();
    }

    let mut world = match &args.load {
        Some(path) => World::load(path).unwrap_or_else(|e| fail(&e)),
        None => {
            let mut config = Config::load(&args.config).unwrap_or_else(|e| fail(&e));
            if let Some(seed) = args.seed {
                config.world.seed = seed;
            }
            let start = Instant::now();
            let w = World::new(config);
            eprintln!("world generated in {:.2}s", start.elapsed().as_secs_f64());
            w
        }
    };

    for t in &args.trigger {
        world.trigger(*t);
    }

    let mut metrics = args.metrics.as_ref().map(|p| {
        std::io::BufWriter::new(std::fs::File::create(p).unwrap_or_else(|e| fail(&e.to_string())))
    });

    let dt = world.config.time.dt as f64;
    let total_ticks = (args.hours * 3600.0 / dt).round() as u64;
    let report_ticks = ((args.report / dt).round() as u64).max(1);
    let start_tick = world.tick;
    let started = Instant::now();
    let mut last_report = Instant::now();
    let mut last_report_tick = world.tick;

    print_header();
    report(&world, 0.0);
    for _ in 0..total_ticks {
        world.step();
        if let Some(out) = metrics.as_mut() {
            // A sample taken this tick is new; write it.
            if let Some(s) = world.stats.samples.last().filter(|s| s.tick == world.tick) {
                writeln!(out, "{}", serde_json::to_string(s).unwrap()).ok();
            }
        }
        if (world.tick - start_tick) % report_ticks == 0 {
            let elapsed = last_report.elapsed().as_secs_f64();
            let tps = (world.tick - last_report_tick) as f64 / elapsed.max(1e-9);
            report(&world, tps);
            if args.diag {
                diagnostics(&world);
            }
            last_report = Instant::now();
            last_report_tick = world.tick;
        }
    }
    let elapsed = started.elapsed().as_secs_f64();
    let ticks = world.tick - start_tick;
    eprintln!(
        "\n{} ticks in {:.1}s ({:.0} ticks/s, {:.1}x real time)",
        ticks,
        elapsed,
        ticks as f64 / elapsed,
        ticks as f64 * dt / elapsed
    );
    summary(&world);
    if args.diag {
        mean_genome(&world);
    }
    println!("\nms per tick by phase:");
    for (name, ms) in world.timings.per_tick_ms() {
        println!("  {name:<12} {ms:>7.3}");
    }

    if let Some(path) = &args.save {
        match world.save(path) {
            Ok(()) => eprintln!("saved to {}", path.display()),
            Err(e) => eprintln!("save failed: {e}"),
        }
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}

fn print_header() {
    println!(
        "{:>9} {:>7} {:>6} {:>4} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>7} {:>6} {:>6}",
        "time",
        "season",
        "pop",
        "spp",
        "photo",
        "chemo",
        "scav",
        "hunt",
        "motile",
        "maxR",
        "carcass",
        "sun",
        "tps"
    );
}

fn report(world: &World, tps: f64) {
    let mut diets = [0u32; 4];
    let mut motile = 0;
    let mut max_r: f32 = 0.0;
    for o in &world.organisms {
        diets[o.traits.main_diet()] += 1;
        if o.traits.is_motile() {
            motile += 1;
        }
        max_r = max_r.max(o.radius);
    }
    let species = world.species.living().count();
    println!(
        "{:>9} {:>7} {:>6} {:>4} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6.1} {:>7} {:>6.2} {:>6.0}",
        format_duration(world.time() as f32),
        world.env.clock.season_name(),
        world.organisms.len(),
        species,
        diets[0],
        diets[1],
        diets[2],
        diets[3],
        motile,
        max_r,
        world.carcasses.len(),
        world.env.clock.sun,
        tps
    );
}

fn summary(world: &World) {
    let s = &world.stats;
    println!("\nbirths {}  deaths {}", s.total_births, s.total_deaths);
    for (i, cause) in DeathCause::ALL.iter().enumerate() {
        println!("  {:<12} {}", cause.label(), s.deaths_by_cause[i]);
    }
    let mut living: Vec<_> = world.species.living().collect();
    living.sort_by_key(|s| std::cmp::Reverse(s.population));
    println!(
        "\nliving species: {}  (ever established: {})",
        living.len(),
        world
            .species
            .species
            .values()
            .filter(|s| s.established)
            .count()
    );
    for sp in living.iter().take(15) {
        println!(
            "  {:>6}  {:<32} founded {:>8}  {}",
            sp.population,
            sp.name,
            format_duration((sp.founded as f64 * world.config.time.dt as f64) as f32),
            sp.founder
        );
    }
    println!("\nvents:");
    for v in &world.env.vents {
        println!(
            "  x={:.0} y={:.0} strength {:.2} ({} of {} h)",
            v.pos.x,
            v.pos.y,
            v.strength,
            format_duration(v.age),
            v.lifetime / 3600.0
        );
    }
    println!("\nrecent events:");
    for e in world.events.entries.iter().rev().take(15).rev() {
        println!("  [{:>8}] {}", format_duration(e.time as f32), e.text);
    }
}

fn diagnostics(world: &World) {
    let n = world.organisms.len().max(1) as f32;
    let h = world.config.world.height;
    let (mut gain, mut cost, mut efrac, mut adult, mut depth, mut agefrac, mut light) =
        (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for o in &world.organisms {
        gain += o.gain_rate;
        cost += o.cost_rate;
        efrac += o.energy / o.max_energy().max(1e-6);
        adult += o.is_adult() as u32 as f32;
        depth += h - o.pos.y;
        agefrac += o.age / o.traits.lifespan;
        light += world.env.light.level_at(o.pos);
    }
    let (m, d) = world.env.field_totals();
    println!(
        "          gain {:.4}/s cost {:.4}/s reserves {:.0}% adult {:.0}% depth {:.0} age {:.0}% light {:.4} (sun {:.4}) minerals {:.0} detritus {:.0}",
        gain / n,
        cost / n,
        efrac / n * 100.0,
        adult / n * 100.0,
        depth / n,
        agefrac / n * 100.0,
        light / n,
        world.env.clock.sun,
        m,
        d
    );
}

/// Population mean of every gene, next to the ancestor's value.
fn mean_genome(world: &World) {
    use evo_core::genome::{CHROMOSOMES, GENOME_LEN};
    let n = world.organisms.len().max(1) as f64;
    let mut sums = [0f64; GENOME_LEN];
    for o in &world.organisms {
        for (i, s) in sums.iter_mut().enumerate() {
            *s += o.genome.0[i] as f64;
        }
    }
    println!("\nmean genome vs ancestor:");
    for c in &CHROMOSOMES {
        let mut line = format!("  {} {:<13}", c.letter, c.name);
        for (k, i) in c.range().enumerate() {
            let mean = sums[i] / n;
            let anc = world.ancestor.0[i] as f64;
            let mark = if (mean - anc).abs() >= 8.0 { "*" } else { " " };
            line += &format!(
                " {}:{:02X}{}",
                c.genes[k].name.chars().take(5).collect::<String>(),
                mean.round() as u8,
                mark
            );
        }
        println!("{line}");
    }
}
