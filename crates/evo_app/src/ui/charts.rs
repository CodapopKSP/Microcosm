//! Charts drawn with the egui painter: population by diet, and a species "river".

use std::collections::BTreeMap;

use bevy_egui::egui;
use evo_core::World;
use evo_core::traits::format_duration;

use super::{DIET_NAMES, color32, diet_color};
use crate::sim::Selection;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartTab {
    #[default]
    Population,
    Tree,
}

pub fn draw(ui: &mut egui::Ui, world: &World, selection: &mut Selection, tab: &mut ChartTab) {
    ui.horizontal(|ui| {
        ui.selectable_value(tab, ChartTab::Population, "Population");
        ui.selectable_value(tab, ChartTab::Tree, "Family tree");
    });
    ui.separator();
    match tab {
        ChartTab::Population => population(ui, world, selection),
        ChartTab::Tree => tree(ui, world, selection),
    }
}

/// Phylogeny over time: each established species is a line from founding to extinction (or now),
/// branching from its nearest established ancestor.
fn tree(ui: &mut egui::Ui, world: &World, selection: &mut Selection) {
    let reg = &world.species;
    let established: Vec<&evo_core::species::Species> =
        reg.species.values().filter(|s| s.established).collect();
    if established.is_empty() {
        ui.label("No species yet.");
        return;
    }
    // Parent links between established species.
    let parent_of = |id: u32| -> Option<u32> {
        let p = reg.get(id)?.parent?;
        reg.established_ancestor(p)
    };
    let mut children: BTreeMap<Option<u32>, Vec<u32>> = BTreeMap::new();
    for s in &established {
        children.entry(parent_of(s.id)).or_default().push(s.id);
    }
    for list in children.values_mut() {
        list.sort_by_key(|id| reg.get(*id).map(|s| s.founded).unwrap_or(0));
    }
    // Depth-first order gives each species a row.
    let mut rows: Vec<u32> = Vec::new();
    let mut stack: Vec<u32> = children
        .get(&None)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .rev()
        .collect();
    while let Some(id) = stack.pop() {
        rows.push(id);
        if let Some(kids) = children.get(&Some(id)) {
            stack.extend(kids.iter().rev());
        }
    }
    let row_of: BTreeMap<u32, usize> = rows.iter().enumerate().map(|(i, &id)| (id, i)).collect();

    let living = established.iter().filter(|s| s.extinct.is_none()).count();
    ui.label(
        egui::RichText::new(format!(
            "{} species ever named · {} living · hover for names, click to highlight",
            rows.len(),
            living
        ))
        .weak(),
    );
    let now = world.tick.max(1) as f32;
    let row_h = 7.0;
    egui::ScrollArea::vertical()
        .max_height(ui.available_height())
        .show(ui, |ui| {
            let height = (rows.len() as f32 * row_h + 10.0).max(120.0);
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), height),
                egui::Sense::click(),
            );
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 3.0, egui::Color32::from_gray(18));
            let x_of = |tick: u64| rect.left() + 6.0 + (tick as f32 / now) * (rect.width() - 12.0);
            let y_of = |row: usize| rect.top() + 5.0 + row as f32 * row_h + row_h * 0.5;
            let pointer = response.hover_pos();
            let mut hovered: Option<u32> = None;
            for (&id, &row) in &row_of {
                let s = reg.get(id).unwrap();
                let end = s.extinct.unwrap_or(world.tick);
                let (x0, x1, y) = (x_of(s.founded), x_of(end), y_of(row));
                let alive = s.extinct.is_none();
                let color = super::color32(s.color);
                let color = if alive {
                    color
                } else {
                    color.gamma_multiply(0.45)
                };
                if let Some(p) = parent_of(id).and_then(|p| row_of.get(&p)) {
                    painter.line_segment(
                        [egui::pos2(x0, y_of(*p)), egui::pos2(x0, y)],
                        egui::Stroke::new(1.0, egui::Color32::from_gray(70)),
                    );
                }
                let width = if selection.species == Some(id) {
                    5.0
                } else if alive {
                    3.0
                } else {
                    1.5
                };
                painter.line_segment(
                    [egui::pos2(x0, y), egui::pos2(x1.max(x0 + 1.0), y)],
                    egui::Stroke::new(width, color),
                );
                if let Some(p) = pointer
                    && (p.y - y).abs() <= row_h * 0.5
                    && p.x >= x0 - 3.0
                    && p.x <= x1 + 3.0
                {
                    hovered = Some(id);
                }
            }
            if let Some(id) = hovered {
                let s = reg.get(id).unwrap();
                let dt = world.config.time.dt;
                let span = format!(
                    "founded {} · {}",
                    format_duration(s.founded as f32 * dt),
                    match s.extinct {
                        Some(t) => format!("extinct at {}", format_duration(t as f32 * dt)),
                        None => format!("alive, {} now", s.population),
                    }
                );
                let r = response.on_hover_ui(|ui| {
                    ui.label(egui::RichText::new(&s.name).strong());
                    ui.label(span);
                    ui.label(format!("peak population {}", s.peak));
                });
                if r.clicked() {
                    selection.species = Some(id);
                }
            }
        });
}

fn population(ui: &mut egui::Ui, world: &World, selection: &mut Selection) {
    let samples = &world.stats.samples;
    if samples.len() < 2 {
        ui.label("Collecting data…");
        return;
    }
    let t0 = samples.first().unwrap().time;
    let t1 = samples.last().unwrap().time.max(t0 + 1.0);
    ui.label(egui::RichText::new(format!("Last {}", format_duration((t1 - t0) as f32))).weak());

    ui.label("Population by diet");
    let height = 150.0;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, egui::Color32::from_gray(20));
    let max_pop = samples
        .iter()
        .map(|s| s.population)
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let x_of = |t: f64| rect.left() + ((t - t0) / (t1 - t0)) as f32 * rect.width();
    let y_of = |v: f32| rect.bottom() - v / max_pop * (rect.height() - 4.0);
    // Stacked areas: photo at the bottom, then chemo, scavenge, hunt.
    for diet in (0..4).rev() {
        let mut points = Vec::with_capacity(samples.len() * 2);
        for s in samples {
            let top: u32 = s.diets[..=diet].iter().sum();
            points.push(egui::pos2(x_of(s.time), y_of(top as f32)));
        }
        for s in samples.iter().rev() {
            let bottom: u32 = s.diets[..diet].iter().sum();
            points.push(egui::pos2(x_of(s.time), y_of(bottom as f32)));
        }
        // Draw as vertical slices (egui polygons must be convex).
        let n = samples.len();
        for i in 0..n - 1 {
            let quad = vec![
                points[i],
                points[i + 1],
                points[2 * n - 2 - i],
                points[2 * n - 1 - i],
            ];
            // Skip empty slices (zero height at both ends), which would render as hairlines.
            if (quad[0].y - quad[3].y).abs() < 0.5 && (quad[1].y - quad[2].y).abs() < 0.5 {
                continue;
            }
            painter.add(egui::Shape::convex_polygon(
                quad,
                diet_color(diet).gamma_multiply(0.85),
                egui::Stroke::NONE,
            ));
        }
    }
    painter.text(
        rect.left_top() + egui::vec2(4.0, 2.0),
        egui::Align2::LEFT_TOP,
        format!("{}", max_pop as u32),
        egui::FontId::proportional(11.0),
        egui::Color32::GRAY,
    );
    ui.horizontal(|ui| {
        for (i, name) in DIET_NAMES.iter().enumerate() {
            ui.colored_label(diet_color(i), format!("■ {name}"));
        }
    });

    ui.add_space(6.0);
    ui.label("Species (share of population)");
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 180.0),
        egui::Sense::click(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, egui::Color32::from_gray(20));
    // Top species over the whole window, the rest lumped together.
    let mut totals: BTreeMap<u32, u64> = BTreeMap::new();
    for s in samples {
        for &(id, pop) in &s.species_populations {
            *totals.entry(id).or_default() += pop as u64;
        }
    }
    let mut top: Vec<(u32, u64)> = totals.into_iter().collect();
    top.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    top.truncate(24);
    let ids: Vec<u32> = top.iter().map(|&(id, _)| id).collect();
    let x_of = |t: f64| rect.left() + ((t - t0) / (t1 - t0)) as f32 * rect.width();
    let mut hovered: Option<u32> = None;
    let pointer = response.hover_pos();
    for w in samples.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let (xa, xb) = (x_of(a.time), x_of(b.time));
        let total_a = a.population.max(1) as f32;
        let total_b = b.population.max(1) as f32;
        let mut ya = rect.bottom();
        let mut yb = rect.bottom();
        for &id in &ids {
            let pa = a
                .species_populations
                .iter()
                .find(|p| p.0 == id)
                .map(|p| p.1)
                .unwrap_or(0) as f32;
            let pb = b
                .species_populations
                .iter()
                .find(|p| p.0 == id)
                .map(|p| p.1)
                .unwrap_or(0) as f32;
            let ha = pa / total_a * rect.height();
            let hb = pb / total_b * rect.height();
            if ha + hb > 0.0 {
                let color = world
                    .species
                    .get(id)
                    .map(|s| color32(s.color))
                    .unwrap_or(egui::Color32::GRAY);
                let quad = vec![
                    egui::pos2(xa, ya),
                    egui::pos2(xb, yb),
                    egui::pos2(xb, yb - hb),
                    egui::pos2(xa, ya - ha),
                ];
                if let Some(p) = pointer
                    && p.x >= xa
                    && p.x <= xb
                    && p.y <= ya
                    && p.y >= ya - ha
                {
                    hovered = Some(id);
                }
                let alpha = if selection.species == Some(id) {
                    1.0
                } else {
                    0.8
                };
                painter.add(egui::Shape::convex_polygon(
                    quad,
                    color.gamma_multiply(alpha),
                    egui::Stroke::NONE,
                ));
            }
            ya -= ha;
            yb -= hb;
        }
    }
    if let Some(id) = hovered {
        let name = world.species.name(id).to_string();
        let r = response.on_hover_text(name);
        if r.clicked() {
            selection.species = Some(id);
        }
    }

    ui.add_space(6.0);
    let last = samples.last().unwrap();
    ui.label(format!(
        "Births {} · deaths {} in the last sample · largest radius {:.1} · biomass {:.0}",
        last.births, last.deaths, last.max_radius, last.biomass
    ));
}
