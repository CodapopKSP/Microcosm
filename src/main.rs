use clap::Parser;
use evolution_sim::cli::Cli;
use evolution_sim::config::SimConfig;
use evolution_sim::genome::{
    ancestor_any_from_config, decode_phenotype, AnyGenome, AnyPhenotype,
};
use evolution_sim::sim::{build_sim_app, SimRunMode};
use std::path::Path;

fn main() {
    let cli = Cli::parse();
    let config_path = cli.config_path();

    let sim_config = cli.load_config().unwrap_or_else(|err| {
        eprintln!("{err}");
        std::process::exit(1);
    });

    let ancestor = ancestor_any_from_config(&sim_config).unwrap_or_else(|err| {
        eprintln!("invalid ancestor genome in config: {err}");
        std::process::exit(1);
    });

    let ancestor_phenotype = decode_phenotype(&ancestor, &sim_config).unwrap_or_else(|err| {
        eprintln!("failed to decode ancestor phenotype: {err}");
        std::process::exit(1);
    });

    log_config_loaded(&config_path, &sim_config, &ancestor, &ancestor_phenotype);

    let mode = if cli.headless {
        eprintln!(
            "headless: running {} ticks (no window)",
            sim_config.run.max_ticks
        );
        SimRunMode::Headless
    } else {
        SimRunMode::Visual
    };

    build_sim_app(sim_config, ancestor, mode).run();
}

fn log_config_loaded(
    config_path: &Path,
    sim_config: &SimConfig,
    ancestor: &AnyGenome,
    ancestor_phenotype: &AnyPhenotype,
) {
    if sim_config.is_seeded() {
        eprintln!(
            "loaded {} (seeded, master_seed={})",
            config_path.display(),
            sim_config.run.master_seed.unwrap()
        );
    } else {
        eprintln!("loaded {} (nondeterministic)", config_path.display());
    }
    let (bytes_len, version, radius, photo) = match (ancestor, ancestor_phenotype) {
        (AnyGenome::V1(g), AnyPhenotype::V1(p)) => (
            g.bytes.len(),
            g.version,
            p.body_radius,
            p.photo_strength,
        ),
        (AnyGenome::V2(g), AnyPhenotype::V2(p)) => (
            g.bytes.len(),
            g.version,
            p.body_radius,
            p.photo_effective,
        ),
        _ => {
            eprintln!("ancestor genome/phenotype version mismatch");
            return;
        }
    };
    eprintln!(
        "ancestor ok ({} bytes, v{}, radius {:.1}, photo {:.2})",
        bytes_len, version, radius, photo
    );
}

#[cfg(test)]
mod tests {
    use bevy::app::App;
    use bevy::prelude::Transform;
    use evolution_sim::config::SimConfig;
    use evolution_sim::genome::{ancestor_any_from_config, AnyGenome};
    use evolution_sim::sim::{
        try_build_spawn, Agent, Cooldowns, Energy, Genome as GenomeComponent, Health,
        LightExposure, Lineage, Phenotype, Velocity,
    };

    #[test]
    fn spawned_agents_have_full_lifecycle_components() {
        let config = SimConfig::load_default().unwrap();
        let genome = ancestor_any_from_config(&config).unwrap();
        let params = try_build_spawn(genome, &config, 1, None, 0).unwrap();

        let mut app = App::new();
        app.world_mut().spawn((
            params.life,
            Velocity(params.velocity),
            LightExposure(0.0),
            Transform::from_translation(params.position.extend(0.0)),
        ));

        let mut query = app.world_mut().query::<(
            &GenomeComponent,
            &Phenotype,
            &Energy,
            &Health,
            &Lineage,
            &Cooldowns,
            &Agent,
            &LightExposure,
        )>();
        let count = query.iter(app.world()).count();
        assert_eq!(count, 1);

        let (_, pheno, energy, health, lineage, cooldowns, agent, exposure) =
            query.single(app.world());
        assert_eq!(exposure.0, 0.0);
        assert_eq!(agent.radius, pheno.body_radius());
        assert_eq!(energy.internal_energy, config.economy.spawn_energy);
        assert_eq!(
            health.core_hp,
            pheno.v1().map(|p| p.core_max_hp).unwrap_or(health.core_max_hp)
        );
        assert!(health.part_hp.is_empty());
        assert_eq!(lineage.generation, 0);
        assert_eq!(cooldowns.repro_ticks_remaining, 0);
    }
}
