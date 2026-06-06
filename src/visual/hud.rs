//! Top-left sim HUD: organism count and frame rate.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::sim::Agent;

#[derive(Resource, Default)]
pub struct FpsCounter {
    frames: u32,
    elapsed: f32,
    fps: f32,
}

pub fn tick_fps_counter(time: Res<Time>, mut counter: ResMut<FpsCounter>) {
    counter.frames += 1;
    counter.elapsed += time.delta_secs();
    if counter.elapsed >= 0.25 {
        counter.fps = counter.frames as f32 / counter.elapsed;
        counter.frames = 0;
        counter.elapsed = 0.0;
    }
}

pub fn draw_sim_hud(mut contexts: EguiContexts, fps: Res<FpsCounter>, agents: Query<(), With<Agent>>) {
    let organism_count = agents.iter().count();
    let ctx = contexts.ctx_mut();
    egui::Area::new(egui::Id::new("sim_hud"))
        .fixed_pos(egui::pos2(12.0, 12.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(egui::Color32::from_black_alpha(160))
                .inner_margin(egui::Margin::same(8.0))
                .rounding(4.0)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(format!("Organism Count: {organism_count}"))
                            .strong()
                            .size(16.0),
                    );
                    ui.label(format!("FPS: {:.0}", fps.fps));
                });
        });
}
