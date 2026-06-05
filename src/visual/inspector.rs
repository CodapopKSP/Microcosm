//! Egui inspector panel for the selected agent (genome hex + live stats).

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::genome::{AnyGenome, AnyPhenotype};
use crate::sim::{
    Age, Agent, BehaviorState, Cooldowns, Energy, Genome, HazardExposure, Heading, Health,
    LightExposure, Lineage, MineralExposure, Phenotype, SensePerception, SimConfigResource,
    Velocity, VentExposure,
};

use super::DebugVisualState;

const TICKS_PER_SEC: f32 = 60.0;

/// Format raw genome bytes as lowercase hex with spaces every 2 bytes.
pub fn format_genome_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn genome_hex(genome: &AnyGenome) -> String {
    match genome {
        AnyGenome::V1(g) => g.to_hex(),
        AnyGenome::V2(g) => g.to_hex(),
    }
}

pub fn genome_byte_len(genome: &AnyGenome) -> usize {
    match genome {
        AnyGenome::V1(g) => g.bytes.len(),
        AnyGenome::V2(g) => g.bytes.len(),
    }
}

pub fn genome_version_label(genome: &AnyGenome) -> &'static str {
    match genome {
        AnyGenome::V1(_) => "EVO1",
        AnyGenome::V2(_) => "EVO2",
    }
}

fn stat_row(ui: &mut egui::Ui, label: &str, value: impl std::fmt::Display) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).weak());
        ui.label(value.to_string());
    });
}

fn section_heading(ui: &mut egui::Ui, title: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(title).strong());
    ui.separator();
}

fn rgb_preview(ui: &mut egui::Ui, rgb: [f32; 3]) {
    let color = egui::Color32::from_rgb(
        (rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
        (rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
        (rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
    );
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 3.0, color);
    ui.painter()
        .rect_stroke(rect, 3.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
}

fn format_energy_cap(max: f32) -> String {
    if max.is_finite() {
        format!("{max:.2}")
    } else {
        "∞".into()
    }
}

/// Draw the agent inspector when an organism is selected.
pub fn draw_agent_inspector(
    mut contexts: EguiContexts,
    state: Res<DebugVisualState>,
    config: Res<SimConfigResource>,
    agents: Query<
        (
            &Transform,
            &Genome,
            &Phenotype,
            &Energy,
            &Health,
            &Lineage,
            &Age,
            &Agent,
            &Heading,
            &Velocity,
            &LightExposure,
            &HazardExposure,
            &VentExposure,
            &MineralExposure,
            &BehaviorState,
        ),
        With<Agent>,
    >,
    cooldowns: Query<&Cooldowns>,
    perceptions: Query<&SensePerception>,
) {
    let Some(selected) = state.selected else {
        return;
    };

    let Ok((
        transform,
        genome,
        phenotype,
        energy,
        health,
        lineage,
        age,
        agent,
        heading,
        velocity,
        light,
        hazard,
        vent,
        minerals,
        behavior,
    )) = agents.get(selected)
    else {
        egui::Window::new("Agent")
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0))
            .resizable(true)
            .default_width(320.0)
            .show(contexts.ctx_mut(), |ui| {
                ui.label("Selected agent despawned.");
            });
        return;
    };

    let repro_cooldown = cooldowns.get(selected).map(|c| c.repro_ticks_remaining).unwrap_or(0);
    let sensor_rays = perceptions.get(selected).map(|p| p.rays.len()).unwrap_or(0);

    let decode_v2 = config.0.genetics.decode_v2.as_ref();
    let hex_raw = genome_hex(&genome.0);
    let hex_spaced = format_genome_hex(match &genome.0 {
        AnyGenome::V1(g) => &g.bytes,
        AnyGenome::V2(g) => &g.bytes,
    });
    let chromosome_lines = match &genome.0 {
        AnyGenome::V2(g) => g.chromosomes_display_lines(),
        AnyGenome::V1(_) => Vec::new(),
    };
    let chromosome_text = match &genome.0 {
        AnyGenome::V2(g) => g.chromosomes_display_text(),
        AnyGenome::V1(_) => hex_spaced.clone(),
    };
    let pos = transform.translation.truncate();
    let speed = velocity.0.length();
    let age_secs = age.ticks as f32 / TICKS_PER_SEC;

    egui::Window::new(format!("Agent #{}", lineage.id))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0))
        .resizable(true)
        .default_width(360.0)
        .show(contexts.ctx_mut(), |ui| {
            section_heading(ui, "Vitality");
            stat_row(
                ui,
                "Energy",
                format!(
                    "{:.2} / {}",
                    energy.internal_energy,
                    format_energy_cap(energy.max_internal_energy)
                ),
            );
            if energy.max_internal_energy.is_finite() {
                stat_row(
                    ui,
                    "Repro at",
                    format!("{:.2}", phenotype.repro_threshold_energy()),
                );
            }
            stat_row(
                ui,
                "Health",
                format!("{:.2} / {:.2}", health.core_hp, health.core_max_hp),
            );
            if health.has_part_hp() {
                stat_row(ui, "Part HP slots", health.part_hp.len());
            }

            section_heading(ui, "Lineage");
            stat_row(ui, "Entity", format!("{selected:?}"));
            stat_row(ui, "Generation", lineage.generation);
            stat_row(
                ui,
                "Parent",
                lineage
                    .parent_id
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "—".into()),
            );
            stat_row(
                ui,
                "Age",
                format!("{} ticks ({:.1}s)", age.ticks, age_secs),
            );
            stat_row(ui, "Repro cooldown", format!("{repro_cooldown} ticks"));

            section_heading(ui, "State");
            stat_row(ui, "Position", format!("({:.1}, {:.1})", pos.x, pos.y));
            stat_row(ui, "Radius", format!("{:.2}", agent.radius));
            stat_row(
                ui,
                "Heading",
                format!("{:.1}°", heading.0.to_degrees()),
            );
            stat_row(ui, "Speed", format!("{:.2}", speed));
            stat_row(ui, "Behavior", format!("{:?}", behavior.mode));
            stat_row(ui, "Light", format!("{:.3}", light.0));
            stat_row(ui, "Hazard dmg/tick", format!("{:.5}", hazard.damage_raw));
            stat_row(ui, "Vent flux", format!("{:.4}", vent.vent_flux));
            stat_row(ui, "Minerals", format!("{:.4}", minerals.concentration));
            stat_row(ui, "Sensor rays", sensor_rays);

            if let AnyPhenotype::V2(p) = &phenotype.0 {
                section_heading(ui, "Phenotype");
                let color = p.color;
                ui.horizontal(|ui| {
                    ui.label("Color");
                    rgb_preview(ui, color);
                    ui.label(format!(
                        "RGB ({}, {}, {})",
                        p.color_rgb[0], p.color_rgb[1], p.color_rgb[2]
                    ));
                });
                stat_row(ui, "Body radius", format!("{:.2}", p.body_radius));
                stat_row(ui, "Max speed", format!("{:.2}", p.max_speed_effective));
                stat_row(ui, "Photo effective", format!("{:.3}", p.photo_effective));
                if let Some(d) = decode_v2 {
                    stat_row(
                        ui,
                        "Photo / sec",
                        format!("{:.3}", p.photo_gain_per_sec(d)),
                    );
                }
                stat_row(ui, "Metabolism / sec", format!("{:.3}", p.metabolism_demand));
                stat_row(
                    ui,
                    "Repro threshold",
                    format!("{:.1}%", p.repro_threshold_frac * 100.0),
                );
                stat_row(
                    ui,
                    "Offspring endowment",
                    format!("{:.1}%", p.offspring_endowment_frac * 100.0),
                );
                stat_row(
                    ui,
                    "Move desire",
                    format!("{:.1}%", p.movement_desire * 100.0),
                );
                stat_row(ui, "Sensor eyes", p.sensor_eye_count);
                stat_row(ui, "Pursue slots", p.pursue_slot_count);
                stat_row(ui, "Avoid slots", p.avoid_slot_count);
            } else if let Some(p) = phenotype.v1() {
                section_heading(ui, "Phenotype");
                let color = p.color;
                ui.horizontal(|ui| {
                    ui.label("Color");
                    rgb_preview(ui, color);
                    ui.label(format!(
                        "R {:.0}%  G {:.0}%  B {:.0}%",
                        color[0] * 100.0,
                        color[1] * 100.0,
                        color[2] * 100.0
                    ));
                });
                stat_row(ui, "Body radius", format!("{:.2}", p.body_radius));
                stat_row(ui, "Max speed", format!("{:.2}", p.max_speed));
                stat_row(ui, "Photo strength", format!("{:.3}", p.photo_strength));
                stat_row(ui, "Metabolism / tick", format!("{:.4}", p.metabolism_idle));
                stat_row(ui, "Repro threshold", format!("{:.2}", p.repro_threshold));
                stat_row(ui, "Move desire", format!("{:.1}%", p.desire_to_move * 100.0));
                stat_row(ui, "Greenness", format!("{:.3}", p.greenness));
            }

            section_heading(ui, "Genome");
            stat_row(
                ui,
                "Version",
                format!(
                    "{} ({} bytes)",
                    genome_version_label(&genome.0),
                    genome_byte_len(&genome.0)
                ),
            );
            ui.horizontal(|ui| {
                if ui.button("Copy hex").clicked() {
                    ui.ctx().copy_text(hex_raw.clone());
                }
                if ui.button("Copy grouped").clicked() {
                    ui.ctx().copy_text(chromosome_text.clone());
                }
            });
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| {
                    if chromosome_lines.is_empty() {
                        ui.add(
                            egui::TextEdit::multiline(&mut hex_spaced.as_str())
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY),
                        );
                    } else {
                        for (label, hex) in &chromosome_lines {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{label:>5}"))
                                        .monospace()
                                        .strong(),
                                );
                                ui.label(egui::RichText::new(hex).monospace());
                            });
                        }
                    }
                });
        });
}
