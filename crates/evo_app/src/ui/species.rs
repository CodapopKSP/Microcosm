//! Species list: a card down the left of the view with every living named species, most
//! numerous first. Click to highlight one on the map, double-click to find a member.

use bevy::prelude::Vec2;
use bevy_egui::egui::{self, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, vec2};
use evo_core::World;
use evo_core::traits::format_game_duration;

use super::hud::{self, ACCENT, MUTED, TEXT, round_button};
use super::{DIET_NAMES, color32, diet_color};
use crate::camera::CameraState;
use crate::sim::Selection;

/// Width of the species card (the same as the clock card above it).
pub const WIDTH: f32 = hud::COLUMN_WIDTH;
const ROW: f32 = 26.0;

fn icon_close(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new(1.8, color);
    let d = s * 0.6;
    p.line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
    p.line_segment([c + vec2(d, -d), c + vec2(-d, d)], stroke);
}

/// Show the species card inside `rect`. Returns false when it was closed.
pub fn show(
    ctx: &egui::Context,
    rect: Rect,
    world: &World,
    selection: &mut Selection,
    cam: &mut CameraState,
) -> bool {
    let mut open = true;
    egui::Area::new(egui::Id::new("species_card"))
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            // An area's room is last frame's size; give it the whole card's space.
            ui.set_max_size(rect.size());
            hud::card().show(ui, |ui| {
                ui.style_mut().visuals.override_text_color = Some(TEXT);
                let inner_w = rect.width() - 24.0;
                ui.set_width(inner_w);
                let mut living: Vec<_> = world.species.living().collect();
                living.sort_by_key(|s| std::cmp::Reverse(s.population));
                let named = world.species.species.values().filter(|s| s.established).count();

                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                    hud::icon_cells(ui.painter(), r.center(), 10.0, ACCENT);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(RichText::new("Species").size(17.0).strong());
                        ui.label(
                            RichText::new(format!("{} living  ·  {} ever named", living.len(), named))
                                .size(12.0)
                                .color(MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if round_button(ui, false, 14.0, icon_close)
                            .on_hover_text("Hide the species list (L)")
                            .clicked()
                        {
                            open = false;
                        }
                    });
                });
                ui.add_space(6.0);
                // Column labels.
                let (head, _) = ui.allocate_exact_size(vec2(inner_w, 16.0), Sense::hover());
                let p = ui.painter();
                let font = FontId::proportional(11.0);
                p.text(head.left_center() + vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, "NAME", font.clone(), MUTED);
                p.text(head.right_center() - vec2(14.0, 0.0), egui::Align2::RIGHT_CENTER, "POPULATION", font, MUTED);
                let (rule, _) = ui.allocate_exact_size(vec2(inner_w, 1.0), Sense::hover());
                ui.painter()
                    .rect_filled(rule, 0.0, Color32::from_rgba_unmultiplied(150, 240, 210, 30));
                ui.add_space(4.0);

                let total: u32 = living.iter().map(|s| s.population).sum::<u32>().max(1);
                let room = rect.bottom() - ui.cursor().top() - 10.0;
                egui::ScrollArea::vertical()
                    .max_height(room.max(80.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let w = inner_w - 10.0;
                        for s in living.iter().take(200) {
                            row(ui, world, s, w, total, selection, cam);
                        }
                    });
            });
        });
    open
}

/// One species: colour, diet, name, and population, with a faint bar for its share of all
/// life behind it.
fn row(
    ui: &mut egui::Ui,
    world: &World,
    s: &evo_core::species::Species,
    width: f32,
    total: u32,
    selection: &mut Selection,
    cam: &mut CameraState,
) {
    let selected = selection.species == Some(s.id);
    let (r, response) = ui.allocate_exact_size(vec2(width, ROW), Sense::click());
    crate::ui_sfx::silent(&response);
    let p = ui.painter();
    let hovered = response.hovered();
    // Background: lit when highlighted on the map.
    if selected {
        p.rect(r, 8.0, Color32::from_rgb(28, 62, 58), Stroke::new(1.0, ACCENT.gamma_multiply(0.6)), egui::StrokeKind::Inside);
    } else if hovered {
        p.rect_filled(r, 8.0, Color32::from_rgb(24, 36, 42));
    }
    // Share of all organisms.
    let share = s.population as f32 / total as f32;
    let bar = Rect::from_min_size(
        Pos2::new(r.left() + 4.0, r.bottom() - 4.0),
        vec2((r.width() - 8.0) * share.clamp(0.0, 1.0), 2.0),
    );
    p.rect_filled(bar, 1.0, color32(s.color).gamma_multiply(0.6));
    // The body colour, as a cell, with the main diet as a dot beside it.
    let cell = Pos2::new(r.left() + 13.0, r.center().y - 1.0);
    p.circle_filled(cell, 6.5, color32(s.color));
    p.circle_stroke(cell, 8.5, Stroke::new(1.0, Color32::from_white_alpha(30)));
    let t = evo_core::Traits::decode(&s.founder, &world.config);
    let diet = t.main_diet();
    p.circle_filled(Pos2::new(r.left() + 29.0, r.center().y - 1.0), 3.0, diet_color(diet));
    // Population, right-aligned.
    let pop_font = FontId::monospace(12.0);
    let pop = s.population.to_string();
    let pop_w = p.layout_no_wrap(pop.clone(), pop_font.clone(), TEXT).size().x;
    p.text(
        Pos2::new(r.right() - 8.0, r.center().y - 1.0),
        egui::Align2::RIGHT_CENTER,
        &pop,
        pop_font,
        if selected { ACCENT } else { MUTED },
    );
    // Name, clipped to the room left.
    let name_rect = Rect::from_min_max(
        Pos2::new(r.left() + 38.0, r.top()),
        Pos2::new(r.right() - pop_w - 16.0, r.bottom()),
    );
    p.with_clip_rect(name_rect).text(
        Pos2::new(name_rect.left(), r.center().y - 1.0),
        egui::Align2::LEFT_CENTER,
        &s.name,
        FontId::proportional(13.0),
        if selected { ACCENT } else { TEXT },
    );

    if response.clicked() {
        selection.species = if selected { None } else { Some(s.id) };
    }
    if response.double_clicked() {
        // Jump to a member.
        if let Some(o) = world.organisms.iter().find(|o| o.species == s.id) {
            selection.organism = Some(o.id);
            selection.follow = true;
            cam.focus(Vec2::new(o.pos.x, o.pos.y), Some((o.radius / 10.0).clamp(0.2, 3.0)));
        }
    }
    response.on_hover_ui(|ui| {
        ui.label(RichText::new(&s.name).strong());
        ui.label(format!("{} · {}", DIET_NAMES[diet], s.population));
        let age = (world.tick - s.founded) as f32 * world.config.time.dt;
        ui.label(format!("Founded {} ago", format_game_duration(age, &world.config.time)));
        if let Some(parent) = s.parent {
            ui.label(format!("Descended from {}", world.species.name(parent)));
        }
        ui.label(format!("Peak population {}", s.peak));
        ui.label(RichText::new(s.founder.to_string()).monospace().small());
        ui.label(RichText::new("Click to highlight, double-click to find one").weak());
    });
}
