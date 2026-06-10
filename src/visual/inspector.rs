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
    genome.to_hex()
}

pub fn genome_byte_len(genome: &AnyGenome) -> usize {
    genome.bytes.len()
}

pub fn genome_version_label(_genome: &AnyGenome) -> &'static str {
    "EVO2"
}

fn stat_row(ui: &mut egui::Ui, label: &str, value: impl std::fmt::Display) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).weak());
        ui.label(value.to_string());
    });
}

/// Render label/value pairs in two side-by-side columns to save vertical space.
fn stat_columns(ui: &mut egui::Ui, items: &[(&str, String)]) {
    if items.is_empty() {
        return;
    }
    let mid = items.len().div_ceil(2);
    ui.columns(2, |columns| {
        for (col_idx, col) in columns.iter_mut().enumerate() {
            let start = col_idx * mid;
            let end = (start + mid).min(items.len());
            for (label, value) in &items[start..end] {
                stat_row(col, label, value);
            }
        }
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

/// Render grouped genome bytes in a wrapping row of label + hex chips.
pub fn draw_genome_chromosomes(ui: &mut egui::Ui, chromosome_lines: &[(String, String)]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        for (label, hex) in chromosome_lines {
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::symmetric(6.0, 4.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(label).monospace().strong());
                        ui.label(egui::RichText::new(hex).monospace());
                    });
                });
        }
    });
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
    let hex_spaced = format_genome_hex(&genome.0.bytes);
    let chromosome_lines = genome.0.chromosomes_display_lines();
    let chromosome_text = genome.0.chromosomes_display_text();
    let pos = transform.translation.truncate();
    let speed = velocity.0.length();
    let age_secs = age.ticks as f32 / TICKS_PER_SEC;

    egui::Window::new(format!("Agent #{}", lineage.id))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0))
        .resizable(true)
        .default_width(400.0)
        .show(contexts.ctx_mut(), |ui| {
            let mut vitality = vec![
                (
                    "Energy",
                    format!(
                        "{:.2} / {}",
                        energy.internal_energy,
                        format_energy_cap(energy.max_internal_energy)
                    ),
                ),
                (
                    "Health",
                    format!("{:.2} / {:.2}", health.core_hp, health.core_max_hp),
                ),
            ];
            if energy.max_internal_energy.is_finite() {
                vitality.push((
                    "Repro at",
                    format!(
                        "{:.2}",
                        crate::sim::repro_energy_gate(&phenotype.0)
                    ),
                ));
            }
            if health.has_part_hp() {
                vitality.push(("Part HP slots", health.part_hp.len().to_string()));
            }
            section_heading(ui, "Vitality");
            stat_columns(ui, &vitality);

            section_heading(ui, "Lineage");
            stat_columns(
                ui,
                &[
                    ("Entity", format!("{selected:?}")),
                    ("Generation", lineage.generation.to_string()),
                    (
                        "Parent",
                        lineage
                            .parent_id
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "—".into()),
                    ),
                    ("Age", format!("{} ticks ({:.1}s)", age.ticks, age_secs)),
                    ("Repro cooldown", format!("{repro_cooldown} ticks")),
                ],
            );

            section_heading(ui, "State");
            stat_columns(
                ui,
                &[
                    ("Position", format!("({:.1}, {:.1})", pos.x, pos.y)),
                    ("Radius", format!("{:.2}", agent.radius)),
                    ("Heading", format!("{:.1}°", heading.0.to_degrees())),
                    ("Speed", format!("{:.2}", speed)),
                    ("Behavior", format!("{:?}", behavior.mode)),
                    ("Light", format!("{:.3}", light.0)),
                    ("Hazard dmg/tick", format!("{:.5}", hazard.damage_raw)),
                    ("Vent flux", format!("{:.4}", vent.vent_flux)),
                    ("Minerals", format!("{:.4}", minerals.concentration)),
                    ("Sensor rays", sensor_rays.to_string()),
                ],
            );

            {
                let p = &phenotype.0;
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
                let mut phenotype_stats = vec![
                    ("Body radius", format!("{:.2}", p.body_radius)),
                    ("Max speed", format!("{:.2}", p.max_speed_effective)),
                    ("Photo effective", format!("{:.3}", p.photo_effective)),
                    ("Metabolism / sec", format!("{:.3}", p.metabolism_demand)),
                    (
                        "Repro threshold",
                        format!("{:.1}%", p.repro_threshold_frac * 100.0),
                    ),
                    (
                        "Offspring endowment",
                        format!("{:.1}%", p.offspring_endowment_frac * 100.0),
                    ),
                    (
                        "Move desire",
                        format!("{:.1}%", p.movement_desire * 100.0),
                    ),
                    ("Sensor eyes", p.sensor_eye_count.to_string()),
                    ("Pursue slots", p.pursue_slot_count.to_string()),
                    ("Avoid slots", p.avoid_slot_count.to_string()),
                ];
                if let Some(d) = decode_v2 {
                    phenotype_stats.insert(
                        3,
                        ("Photo / sec", format!("{:.3}", p.photo_gain_per_sec(d))),
                    );
                }
                stat_columns(ui, &phenotype_stats);
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
            if chromosome_lines.is_empty() {
                ui.add(
                    egui::TextEdit::multiline(&mut hex_spaced.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY),
                );
            } else {
                draw_genome_chromosomes(ui, &chromosome_lines);
            }
        });
}
