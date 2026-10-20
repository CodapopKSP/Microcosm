//! Charts drawn with the egui painter: population, the family tree, ecology and the
//! environment over time. Hover a chart to read it; click to pick a moment, which is marked on
//! every chart and described under the Population charts.

use std::collections::{BTreeMap, HashMap, HashSet};

use bevy_egui::egui;
use evo_core::World;
use evo_core::organism::DeathCause;
use evo_core::parts::PartKind;
use evo_core::stats::StatSample;
use evo_core::traits::{format_game_date, format_game_duration};

use super::{DIET_NAMES, color32, diet_color};
use crate::camera::CameraState;
use crate::sim::Selection;
use crate::ui_sfx::HoverSound;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartTab {
    #[default]
    Population,
    Tree,
    Ecology,
    Environment,
}

/// Chart settings, kept between frames.
#[derive(Default)]
pub struct ChartState {
    pub tab: ChartTab,
    tree: TreeState,
    /// A moment picked by clicking a chart (sim seconds).
    pinned: Option<f64>,
}

/// The charts' header, as the other cards have: icon, title, what they cover, and a close
/// button. Returns false when closed.
pub fn header(ui: &mut egui::Ui, world: &World) -> bool {
    use super::hud::{self, ACCENT, MUTED};
    let mut open = true;
    let samples = &world.stats.samples;
    let covers = match (samples.first(), samples.last()) {
        (Some(a), Some(b)) if samples.len() >= 2 => format!(
            "The last {} · hover a chart to read it, click to pick a moment",
            format_game_duration((b.time - a.time) as f32, &world.config.time)
        ),
        _ => "Population, family tree, ecology and environment over time".to_string(),
    };
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
        hud::icon_chart(ui.painter(), r.center(), 10.0, ACCENT);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(egui::RichText::new("Charts").size(17.0).strong());
            ui.label(egui::RichText::new(covers).size(12.0).color(MUTED));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if hud::round_button(ui, false, 14.0, hud::icon_close)
                .on_hover_text("Close the charts (G)")
                .clicked()
            {
                open = false;
            }
        });
    });
    ui.add_space(8.0);
    open
}

pub fn draw(
    ui: &mut egui::Ui,
    world: &World,
    selection: &mut Selection,
    state: &mut ChartState,
    cam: &mut CameraState,
) {
    super::hud::tab_bar(
        ui,
        &mut state.tab,
        &[
            (ChartTab::Population, "Population"),
            (ChartTab::Tree, "Family tree"),
            (ChartTab::Ecology, "Ecology"),
            (ChartTab::Environment, "Environment"),
        ],
    );
    ui.add_space(6.0);
    if state.tab == ChartTab::Tree {
        tree(ui, world, selection, &mut state.tree, cam);
        return;
    }
    let samples = &world.stats.samples;
    if samples.len() < 2 {
        ui.label(egui::RichText::new("Collecting data…").color(super::hud::MUTED));
        return;
    }
    // Charts grow with the window.
    let h = (ui.available_height() / 3.3).clamp(100.0, 600.0);
    let pinned = &mut state.pinned;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| match state.tab {
            ChartTab::Population => population(ui, world, selection, pinned, h),
            ChartTab::Ecology => ecology(ui, world, pinned, h),
            ChartTab::Environment => environment(ui, world, pinned, h),
            ChartTab::Tree => {}
        });
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    /// Layers stacked on each other from zero.
    Stacked,
    /// Lines, the axis fitted to the data (to show small changes).
    Lines,
    /// Lines, the axis from zero.
    LinesFromZero,
}

/// Add a filled quadrilateral (corners in order) to `mesh`.
fn quad(mesh: &mut egui::Mesh, corners: [egui::Pos2; 4], color: egui::Color32) {
    let i = mesh.vertices.len() as u32;
    for p in corners {
        mesh.colored_vertex(p, color);
    }
    mesh.add_triangle(i, i + 1, i + 2);
    mesh.add_triangle(i, i + 2, i + 3);
}

/// The sample nearest sim time `t`.
fn nearest(samples: &[StatSample], t: f64) -> usize {
    let i = samples.partition_point(|s| s.time < t);
    if i == 0 {
        0
    } else if i >= samples.len() {
        samples.len() - 1
    } else if (samples[i].time - t) < (t - samples[i - 1].time) {
        i
    } else {
        i - 1
    }
}

/// Births or deaths counted over one sample's interval, as a rate per game day.
fn per_day(world: &World, count: u32) -> f32 {
    count as f32 / world.config.stats.sample_interval.max(1e-3) * world.config.time.day_length
}

fn count(v: f32) -> String {
    if v >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

fn percent(v: f32) -> String {
    format!("{:.0}%", v * 100.0)
}

/// A chart over the stats samples: `layers` (name, colour) with values from `value(sample,
/// layer)`. Hover reads every layer at that moment; a click picks the moment (`pinned`).
#[allow(clippy::too_many_arguments)]
fn chart(
    ui: &mut egui::Ui,
    world: &World,
    title: &str,
    height: f32,
    layers: &[(String, egui::Color32)],
    value: &dyn Fn(&StatSample, usize) -> f32,
    style: Style,
    format: &dyn Fn(f32) -> String,
    pinned: &mut Option<f64>,
) {
    let samples = &world.stats.samples;
    let time = &world.config.time;
    let n = layers.len();
    ui.label(egui::RichText::new(title).strong());
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    crate::ui_sfx::silent(&response);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, egui::Color32::from_gray(20));
    let t0 = samples[0].time;
    let t1 = samples.last().unwrap().time.max(t0 + 1.0);
    let x_of = |t: f64| rect.left() + ((t - t0) / (t1 - t0)) as f32 * rect.width();
    let total = |s: &StatSample| (0..n).map(|k| value(s, k)).sum::<f32>();
    let (lo, hi) = match style {
        Style::Stacked => (0.0, samples.iter().map(total).fold(0.0, f32::max)),
        _ => {
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for s in samples {
                for k in 0..n {
                    let v = value(s, k);
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
            }
            if style == Style::LinesFromZero {
                (lo.min(0.0), hi * 1.05)
            } else {
                let pad = ((hi - lo) * 0.1).max(hi.abs() * 0.01).max(1e-6);
                (lo - pad, hi + pad)
            }
        }
    };
    let hi = if hi - lo < 1e-9 { lo + 1.0 } else { hi };
    let y_of = |v: f32| rect.bottom() - 2.0 - (v - lo) / (hi - lo) * (rect.height() - 4.0);

    // Year lines.
    let year = (time.year_length * time.day_length) as f64;
    let years = (t1 - t0) / year;
    let step = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0]
        .into_iter()
        .find(|s| years / s <= 8.0)
        .unwrap_or(200.0);
    let mut y = ((t0 / year) / step).ceil() * step;
    while y * year <= t1 {
        let x = x_of(y * year);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, egui::Color32::from_gray(32)),
        );
        painter.text(
            egui::pos2(x + 3.0, rect.bottom() - 2.0),
            egui::Align2::LEFT_BOTTOM,
            format!("year {}", y as u64 + 1),
            egui::FontId::proportional(10.0),
            egui::Color32::from_gray(90),
        );
        y += step;
    }

    match style {
        Style::Stacked => {
            // Vertical slices in one mesh (separate shapes would leave anti-aliased seams).
            let mut mesh = egui::Mesh::default();
            for w in samples.windows(2) {
                let (a, b) = (&w[0], &w[1]);
                let (xa, xb) = (x_of(a.time), x_of(b.time));
                let (mut ya, mut yb) = (0.0, 0.0);
                for k in 0..n {
                    let (na, nb) = (ya + value(a, k), yb + value(b, k));
                    if na > ya || nb > yb {
                        quad(
                            &mut mesh,
                            [
                                egui::pos2(xa, y_of(ya)),
                                egui::pos2(xb, y_of(yb)),
                                egui::pos2(xb, y_of(nb)),
                                egui::pos2(xa, y_of(na)),
                            ],
                            layers[k].1.gamma_multiply(0.85),
                        );
                    }
                    (ya, yb) = (na, nb);
                }
            }
            painter.add(egui::Shape::mesh(mesh));
        }
        _ => {
            for (k, (_, color)) in layers.iter().enumerate() {
                let points: Vec<egui::Pos2> = samples
                    .iter()
                    .map(|s| egui::pos2(x_of(s.time), y_of(value(s, k))))
                    .collect();
                painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, *color)));
            }
        }
    }
    painter.text(
        rect.left_top() + egui::vec2(4.0, 2.0),
        egui::Align2::LEFT_TOP,
        format(hi),
        egui::FontId::proportional(11.0),
        egui::Color32::GRAY,
    );
    if style == Style::Lines {
        painter.text(
            rect.left_bottom() + egui::vec2(4.0, -12.0),
            egui::Align2::LEFT_BOTTOM,
            format(lo),
            egui::FontId::proportional(11.0),
            egui::Color32::GRAY,
        );
    }
    if let Some(t) = *pinned
        && (t0..=t1).contains(&t)
    {
        let x = x_of(t);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.5, egui::Color32::from_rgb(250, 210, 110)),
        );
    }
    let at = |p: egui::Pos2| {
        nearest(
            samples,
            t0 + ((p.x - rect.left()) / rect.width()) as f64 * (t1 - t0),
        )
    };
    if response.clicked()
        && let Some(p) = response.interact_pointer_pos()
    {
        let t = samples[at(p)].time;
        *pinned = if *pinned == Some(t) { None } else { Some(t) };
    }
    if let Some(p) = response.hover_pos() {
        let s = &samples[at(p)];
        let x = x_of(s.time);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, egui::Color32::from_gray(150)),
        );
        response.on_hover_ui_at_pointer(|ui| {
            ui.label(egui::RichText::new(format_game_date(s.time, time)).strong());
            let order: Vec<usize> = if style == Style::Stacked {
                (0..n).rev().collect()
            } else {
                (0..n).collect()
            };
            for k in order {
                let (name, color) = &layers[k];
                ui.colored_label(*color, format!("■ {name}: {}", format(value(s, k))));
            }
            if style == Style::Stacked && n > 1 {
                ui.label(format!("Total: {}", format(total(s))));
            }
        });
    }
    let last = samples.last().unwrap();
    ui.horizontal_wrapped(|ui| {
        for (k, (name, color)) in layers.iter().enumerate() {
            ui.colored_label(*color, format!("■ {name} {}", format(value(last, k))));
        }
    });
    ui.add_space(4.0);
}

fn population(
    ui: &mut egui::Ui,
    world: &World,
    selection: &mut Selection,
    pinned: &mut Option<f64>,
    h: f32,
) {
    let diets: Vec<(String, egui::Color32)> = DIET_NAMES
        .iter()
        .enumerate()
        .map(|(i, name)| (name.to_string(), diet_color(i)))
        .collect();
    chart(
        ui,
        world,
        "Population by diet",
        h,
        &diets,
        &|s, k| s.diets[k] as f32,
        Style::Stacked,
        &count,
        pinned,
    );
    species_share(ui, world, selection, pinned, h);
    chart(
        ui,
        world,
        "Living species",
        h * 0.6,
        &[("Species".into(), egui::Color32::from_rgb(140, 220, 140))],
        &|s, _| s.species as f32,
        Style::LinesFromZero,
        &count,
        pinned,
    );
    chart(
        ui,
        world,
        "Births and deaths per day",
        h * 0.6,
        &[
            ("Births".into(), egui::Color32::from_rgb(120, 200, 255)),
            ("Deaths".into(), egui::Color32::from_rgb(230, 120, 110)),
        ],
        &|s, k| per_day(world, if k == 0 { s.births } else { s.deaths }),
        Style::LinesFromZero,
        &count,
        pinned,
    );
    moment(ui, world, selection, pinned);
}

/// The species' shares of the population over time; hover names one, click highlights it.
fn species_share(
    ui: &mut egui::Ui,
    world: &World,
    selection: &mut Selection,
    pinned: &Option<f64>,
    h: f32,
) {
    let samples = &world.stats.samples;
    ui.label(egui::RichText::new("Species (share of the population)").strong());
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), h * 1.2),
        egui::Sense::click(),
    );
    crate::ui_sfx::silent(&response);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, egui::Color32::from_gray(20));
    // The top species over the whole window; the rest are left as the gap at the top.
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
    let t0 = samples[0].time;
    let t1 = samples.last().unwrap().time.max(t0 + 1.0);
    let x_of = |t: f64| rect.left() + ((t - t0) / (t1 - t0)) as f32 * rect.width();
    let pop_of = |s: &StatSample, id: u32| {
        s.species_populations
            .iter()
            .find(|p| p.0 == id)
            .map(|p| p.1)
            .unwrap_or(0) as f32
    };
    let mut hovered: Option<(u32, usize)> = None;
    let pointer = response.hover_pos();
    let mut mesh = egui::Mesh::default();
    for (i, w) in samples.windows(2).enumerate() {
        let (a, b) = (&w[0], &w[1]);
        let (xa, xb) = (x_of(a.time), x_of(b.time));
        let (total_a, total_b) = (a.population.max(1) as f32, b.population.max(1) as f32);
        let (mut ya, mut yb) = (rect.bottom(), rect.bottom());
        for &id in &ids {
            let ha = pop_of(a, id) / total_a * rect.height();
            let hb = pop_of(b, id) / total_b * rect.height();
            if ha + hb > 0.0 {
                let color = world
                    .species
                    .get(id)
                    .map(|s| color32(s.color))
                    .unwrap_or(egui::Color32::GRAY);
                if let Some(p) = pointer
                    && p.x >= xa
                    && p.x <= xb
                    && p.y <= ya
                    && p.y >= ya - ha
                {
                    hovered = Some((id, i));
                }
                let alpha = if selection.species == Some(id) {
                    1.0
                } else if selection.species.is_some() {
                    0.4
                } else {
                    0.8
                };
                quad(
                    &mut mesh,
                    [
                        egui::pos2(xa, ya),
                        egui::pos2(xb, yb),
                        egui::pos2(xb, yb - hb),
                        egui::pos2(xa, ya - ha),
                    ],
                    color.gamma_multiply(alpha),
                );
            }
            ya -= ha;
            yb -= hb;
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    if let Some(t) = *pinned
        && (t0..=t1).contains(&t)
    {
        let x = x_of(t);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.5, egui::Color32::from_rgb(250, 210, 110)),
        );
    }
    if let Some((id, i)) = hovered {
        if response.clicked() {
            selection.species = Some(id);
        }
        let s = &samples[i];
        response.on_hover_ui_at_pointer(|ui| {
            ui.label(egui::RichText::new(world.species.name(id)).strong());
            ui.label(format_game_date(s.time, &world.config.time));
            let n = pop_of(s, id);
            ui.label(format!(
                "{n:.0} organisms, {:.0}% of all",
                n / s.population.max(1) as f32 * 100.0
            ));
            ui.label(egui::RichText::new("Click to highlight it on the map").weak());
        });
    }
    ui.add_space(4.0);
}

/// Everything recorded at the picked moment.
fn moment(ui: &mut egui::Ui, world: &World, selection: &mut Selection, pinned: &mut Option<f64>) {
    let samples = &world.stats.samples;
    ui.separator();
    let Some(t) = *pinned else {
        ui.label(egui::RichText::new("Click any chart to see the details of that moment.").weak());
        return;
    };
    let s = &samples[nearest(samples, t)];
    ui.horizontal(|ui| {
        ui.heading(format_game_date(s.time, &world.config.time));
        if ui.button("Clear").on_hover_sound().clicked() {
            *pinned = None;
        }
    });
    let pop = s.population.max(1) as f32;
    let share = |n: u32| format!("{n} ({:.0}%)", n as f32 / pop * 100.0);
    egui::Grid::new("moment")
        .num_columns(2)
        .spacing([16.0, 2.0])
        .show(ui, |ui| {
            let mut row = |name: &str, value: String| {
                ui.label(egui::RichText::new(name).weak());
                ui.label(value);
                ui.end_row();
            };
            row(
                "Population",
                format!("{} · biomass {:.0}", s.population, s.biomass),
            );
            row("Living species", format!("{}", s.species));
            row(
                "Births / deaths",
                format!(
                    "{} / {} per day",
                    count(per_day(world, s.births)),
                    count(per_day(world, s.deaths))
                ),
            );
            row(
                "Body size",
                format!(
                    "mean radius {:.1} · largest {:.1}",
                    s.mean_radius, s.max_radius
                ),
            );
            row("Moving", share(s.motile));
            row(
                "With body parts",
                format!(
                    "{} · {:.1} parts on average",
                    share(s.with_parts),
                    s.mean_parts
                ),
            );
            let bands = ["surface", "sunlit", "twilight", "deep"];
            row(
                "Where",
                bands
                    .iter()
                    .zip(s.depth_bands)
                    .map(|(b, n)| format!("{b} {n}"))
                    .collect::<Vec<_>>()
                    .join(" · "),
            );
            let mut causes: Vec<(usize, u32)> = s
                .death_causes
                .iter()
                .copied()
                .enumerate()
                .filter(|c| c.1 > 0)
                .collect();
            causes.sort_by_key(|c| std::cmp::Reverse(c.1));
            if !causes.is_empty() {
                row(
                    "Dying of",
                    causes
                        .iter()
                        .take(4)
                        .map(|&(k, n)| format!("{} {}", DeathCause::ALL[k].label(), n))
                        .collect::<Vec<_>>()
                        .join(" · "),
                );
            }
        });
    ui.horizontal_wrapped(|ui| {
        for (i, name) in DIET_NAMES.iter().enumerate() {
            ui.colored_label(diet_color(i), format!("■ {name} {}", share(s.diets[i])));
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Biggest species then (click to highlight)").strong());
    let mut top = s.species_populations.clone();
    top.sort_by_key(|p| std::cmp::Reverse(p.1));
    for &(id, n) in top.iter().take(12) {
        ui.horizontal(|ui| {
            if let Some(sp) = world.species.get(id) {
                super::swatch(ui, sp.color, 10.0);
            }
            let gone = world.species.get(id).is_some_and(|sp| sp.extinct.is_some());
            let mut text = format!("{} — {}", world.species.name(id), share(n));
            if gone {
                text += " · since gone";
            }
            if ui
                .selectable_label(selection.species == Some(id), text)
                .clicked()
            {
                selection.species = Some(id);
            }
        });
    }
}

fn ecology(ui: &mut egui::Ui, world: &World, pinned: &mut Option<f64>, h: f32) {
    let samples = &world.stats.samples;
    let bands = vec![
        (
            "Surface (< 300)".to_string(),
            egui::Color32::from_rgb(150, 230, 230),
        ),
        (
            "Sunlit (300–900)".to_string(),
            egui::Color32::from_rgb(70, 170, 190),
        ),
        (
            "Twilight (900–2500)".to_string(),
            egui::Color32::from_rgb(60, 90, 170),
        ),
        (
            "Deep (> 2500)".to_string(),
            egui::Color32::from_rgb(110, 60, 150),
        ),
    ];
    chart(
        ui,
        world,
        "Where they live (by depth)",
        h,
        &bands,
        &|s, k| s.depth_bands[k] as f32,
        Style::Stacked,
        &count,
        pinned,
    );

    // Deaths by cause: only causes that happened in this window.
    let palette = [
        egui::Color32::from_rgb(200, 170, 90),  // starvation
        egui::Color32::from_rgb(150, 150, 150), // old age
        egui::Color32::from_rgb(230, 90, 90),   // eaten whole
        egui::Color32::from_rgb(240, 140, 80),  // predation
        egui::Color32::from_rgb(250, 210, 90),  // temperature
        egui::Color32::from_rgb(200, 120, 220), // spikes
        egui::Color32::from_rgb(120, 200, 120), // disease
        egui::Color32::from_rgb(255, 100, 40),  // catastrophe
        egui::Color32::from_rgb(110, 150, 255), // suffocation
        egui::Color32::from_rgb(170, 240, 90),  // poison
    ];
    let causes: Vec<usize> = (0..10)
        .filter(|&k| samples.iter().any(|s| s.death_causes[k] > 0))
        .collect();
    if !causes.is_empty() {
        let layers: Vec<(String, egui::Color32)> = causes
            .iter()
            .map(|&k| (DeathCause::ALL[k].label().to_string(), palette[k]))
            .collect();
        chart(
            ui,
            world,
            "Deaths per day, by cause",
            h,
            &layers,
            &|s, k| per_day(world, s.death_causes[causes[k]]),
            Style::Stacked,
            &count,
            pinned,
        );
    }

    // Body parts: the most carried kinds.
    let share = |s: &StatSample, k: usize| s.part_carriers[k] as f32 / s.population.max(1) as f32;
    let mut kinds: Vec<(usize, f32)> = (0..15)
        .map(|k| (k, samples.iter().map(|s| share(s, k)).fold(0.0, f32::max)))
        .filter(|&(_, m)| m > 0.01)
        .collect();
    kinds.sort_by(|a, b| b.1.total_cmp(&a.1));
    kinds.truncate(8);
    if !kinds.is_empty() {
        let palette = [
            egui::Color32::from_rgb(120, 200, 255),
            egui::Color32::from_rgb(255, 160, 120),
            egui::Color32::from_rgb(140, 230, 140),
            egui::Color32::from_rgb(240, 210, 100),
            egui::Color32::from_rgb(210, 140, 240),
            egui::Color32::from_rgb(100, 220, 210),
            egui::Color32::from_rgb(240, 120, 170),
            egui::Color32::from_rgb(190, 190, 190),
        ];
        let layers: Vec<(String, egui::Color32)> = kinds
            .iter()
            .enumerate()
            .map(|(i, &(k, _))| (PartKind::ALL[k].plural().to_string(), palette[i]))
            .collect();
        chart(
            ui,
            world,
            "Body parts carried (share of organisms)",
            h,
            &layers,
            &|s, i| share(s, kinds[i].0),
            Style::LinesFromZero,
            &percent,
            pinned,
        );
    }
    chart(
        ui,
        world,
        "Body plans",
        h * 0.7,
        &[
            (
                "With body parts".into(),
                egui::Color32::from_rgb(140, 230, 140),
            ),
            ("Moving".into(), egui::Color32::from_rgb(120, 200, 255)),
        ],
        &|s, k| {
            let n = if k == 0 { s.with_parts } else { s.motile };
            n as f32 / s.population.max(1) as f32
        },
        Style::LinesFromZero,
        &percent,
        pinned,
    );
    chart(
        ui,
        world,
        "Parts per organism (mean)",
        h * 0.6,
        &[("Parts".into(), egui::Color32::from_rgb(240, 210, 100))],
        &|s, _| s.mean_parts,
        Style::LinesFromZero,
        &|v| format!("{v:.2}"),
        pinned,
    );
    chart(
        ui,
        world,
        "Largest body (radius)",
        h * 0.6,
        &[("Largest".into(), egui::Color32::from_rgb(255, 160, 120))],
        &|s, _| s.max_radius,
        Style::LinesFromZero,
        &|v| format!("{v:.1}"),
        pinned,
    );
    chart(
        ui,
        world,
        "Mean body (radius)",
        h * 0.6,
        &[("Mean".into(), egui::Color32::from_rgb(200, 200, 200))],
        &|s, _| s.mean_radius,
        Style::Lines,
        &|v| format!("{v:.2}"),
        pinned,
    );
}

fn environment(ui: &mut egui::Ui, world: &World, pinned: &mut Option<f64>, h: f32) {
    chart(
        ui,
        world,
        "Surface temperature (°C)",
        h * 0.7,
        &[("Surface".into(), egui::Color32::from_rgb(250, 170, 90))],
        &|s, _| s.surface_temp,
        Style::Lines,
        &|v| format!("{v:.1} °C"),
        pinned,
    );
    chart(
        ui,
        world,
        "The air (1.00 = today's Earth), scaled to show changes",
        h * 0.8,
        &[
            ("Oxygen".into(), egui::Color32::from_rgb(110, 170, 255)),
            ("CO₂".into(), egui::Color32::from_rgb(230, 170, 90)),
        ],
        &|s, k| if k == 0 { s.air_oxygen } else { s.air_co2 },
        Style::Lines,
        &|v| format!("{v:.3}"),
        pinned,
    );
    chart(
        ui,
        world,
        "Oxygen in the sunlit ocean (top 400; 1.0 = saturated)",
        h * 0.7,
        &[("Oxygen".into(), egui::Color32::from_rgb(110, 170, 255))],
        &|s, _| s.surface_oxygen,
        Style::Lines,
        &|v| format!("{v:.3}"),
        pinned,
    );
    chart(
        ui,
        world,
        "Nutrients in the sunlit ocean",
        h * 0.7,
        &[("Nutrients".into(), egui::Color32::from_rgb(120, 220, 110))],
        &|s, _| s.surface_nutrients,
        Style::Lines,
        &|v| format!("{v:.3}"),
        pinned,
    );
    chart(
        ui,
        world,
        "Dead zones (share of the ocean nearly without oxygen)",
        h * 0.7,
        &[("Dead zones".into(), egui::Color32::from_rgb(170, 90, 150))],
        &|s, _| s.anoxic_fraction,
        Style::LinesFromZero,
        &|v| format!("{:.1}%", v * 100.0),
        pinned,
    );
}

/// Family tree settings, kept between frames.
pub struct TreeState {
    /// Species whose descendants are folded away.
    folded: HashSet<u32>,
    /// Leave out lineages with no living species at all.
    hide_extinct: bool,
    /// Row height in points (names show from about 11).
    row_h: f32,
    /// Scroll to the selected species on the next frame.
    reveal: bool,
    /// Mark world events (vents, walls, plagues, meteors…) on the time axis.
    show_events: bool,
}

impl Default for TreeState {
    fn default() -> Self {
        Self {
            folded: HashSet::new(),
            hide_extinct: false,
            row_h: 14.0,
            reveal: false,
            show_events: true,
        }
    }
}

fn tree(
    ui: &mut egui::Ui,
    world: &World,
    selection: &mut Selection,
    state: &mut TreeState,
    cam: &mut CameraState,
) {
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
    // Roots run oldest first; children run newest first, so each child's link down from its
    // parent only passes rows founded later than it and never crosses another species' line.
    for (parent, list) in children.iter_mut() {
        list.sort_by_key(|id| (reg.get(*id).map(|s| s.founded).unwrap_or(0), *id));
        if parent.is_some() {
            list.reverse();
        }
    }
    let kids = |id: u32| children.get(&Some(id)).map(Vec::as_slice).unwrap_or(&[]);

    // Depth-first order, then each lineage's size and whether anything in it still lives.
    let mut order: Vec<u32> = Vec::new();
    let mut stack: Vec<u32> = children.get(&None).cloned().unwrap_or_default();
    stack.reverse();
    while let Some(id) = stack.pop() {
        order.push(id);
        stack.extend(kids(id).iter().rev());
    }
    let mut descendants: HashMap<u32, usize> = HashMap::new();
    let mut living_below: HashMap<u32, usize> = HashMap::new();
    for &id in order.iter().rev() {
        let (mut n, mut alive) = (0, 0);
        for &k in kids(id) {
            n += 1 + descendants[&k];
            alive +=
                living_below[&k] + usize::from(reg.get(k).is_some_and(|s| s.extinct.is_none()));
        }
        descendants.insert(id, n);
        living_below.insert(id, alive);
    }
    let lives = |id: u32| living_below[&id] > 0 || reg.get(id).is_some_and(|s| s.extinct.is_none());

    // Toolbar.
    let living = established.iter().filter(|s| s.extinct.is_none()).count();
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(format!("{} named · {} living", order.len(), living)).weak());
        ui.separator();
        if ui
            .button("Fold dead lineages")
            .on_hover_sound()
            .on_hover_text("Fold every lineage with nothing left alive in it")
            .clicked()
        {
            for &id in &order {
                if !kids(id).is_empty() && !lives(id) {
                    state.folded.insert(id);
                }
            }
        }
        if ui.button("Unfold all").on_hover_sound().clicked() {
            state.folded.clear();
        }
        ui.checkbox(&mut state.hide_extinct, "Hide dead lineages");
        ui.checkbox(&mut state.show_events, "Events")
            .on_hover_text("Mark vents, walls, upwellings, plagues and meteors on the time axis");
        if let Some(sel) = selection.species
            && ui
                .button("Show selected")
                .on_hover_sound()
                .on_hover_text("Unfold the way to the highlighted species and scroll to it")
                .clicked()
        {
            let mut a = parent_of(sel);
            while let Some(p) = a {
                state.folded.remove(&p);
                a = parent_of(p);
            }
            state.hide_extinct &= lives(sel);
            state.reveal = true;
        }
        ui.add(egui::Slider::new(&mut state.row_h, 5.0..=24.0).text("row"));
    });

    // Rows: skip what's folded away or hidden.
    let mut rows: Vec<u32> = Vec::new();
    let mut stack: Vec<u32> = children.get(&None).cloned().unwrap_or_default();
    stack.reverse();
    while let Some(id) = stack.pop() {
        if state.hide_extinct && !lives(id) {
            continue;
        }
        rows.push(id);
        if !state.folded.contains(&id) {
            stack.extend(kids(id).iter().rev());
        }
    }
    let row_of: HashMap<u32, usize> = rows.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    // The highlighted species' ancestors, drawn brighter.
    let mut line: HashSet<u32> = HashSet::new();
    let mut a = selection.species;
    while let Some(id) = a {
        line.insert(id);
        a = parent_of(id);
    }

    let dt = world.config.time.dt;
    let now = world.tick.max(1) as f32;
    let row_h = state.row_h;
    let axis_h = 28.0;
    let labels = row_h >= 11.0;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(ui.available_height())
        .show(ui, |ui| {
            let height = (rows.len() as f32 * row_h + axis_h + 8.0).max(120.0);
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), height),
                egui::Sense::click(),
            );
            crate::ui_sfx::silent(&response);
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 3.0, egui::Color32::from_gray(18));
            // Leave room on the right for names.
            let right = rect.right() - if labels { 150.0 } else { 8.0 };
            let x_of =
                |tick: u64| rect.left() + 18.0 + (tick as f32 / now) * (right - rect.left() - 18.0);
            let y_of = |row: usize| rect.top() + axis_h + 4.0 + row as f32 * row_h + row_h * 0.5;
            let visible = ui.clip_rect();

            // Time axis: a tick every few years.
            let year = world.config.time.year_length * world.config.time.day_length;
            let years = now * dt / year;
            let step = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0]
                .into_iter()
                .find(|s| years / s <= 10.0)
                .unwrap_or(1000.0);
            let mut y = step;
            while y <= years {
                let x = x_of((y * year / dt) as u64);
                painter.line_segment(
                    [
                        egui::pos2(x, rect.top() + axis_h),
                        egui::pos2(x, rect.bottom()),
                    ],
                    egui::Stroke::new(1.0, egui::Color32::from_gray(28)),
                );
                painter.text(
                    egui::pos2(x, rect.top() + 2.0),
                    egui::Align2::CENTER_TOP,
                    format!("year {y:.0}"),
                    egui::FontId::proportional(10.0),
                    egui::Color32::from_gray(110),
                );
                y += step;
            }

            // World events: a marker under the axis and a faint line down the tree.
            let pointer = response.hover_pos();
            let marker_y = rect.top() + axis_h - 6.0;
            let mut event_hovered: Option<&evo_core::stats::Event> = None;
            if state.show_events {
                let mut best = 5.0;
                for e in &world.events.world {
                    let x = x_of(e.tick);
                    let color = super::event_color(e.kind);
                    painter.line_segment(
                        [egui::pos2(x, marker_y), egui::pos2(x, rect.bottom())],
                        egui::Stroke::new(1.0, color.gamma_multiply(0.22)),
                    );
                    let r = 3.5;
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(x, marker_y - r),
                            egui::pos2(x + r, marker_y),
                            egui::pos2(x, marker_y + r),
                            egui::pos2(x - r, marker_y),
                        ],
                        color,
                        egui::Stroke::NONE,
                    ));
                    if let Some(p) = pointer
                        && (p.y - marker_y).abs() < 8.0
                        && (p.x - x).abs() < best
                    {
                        best = (p.x - x).abs();
                        event_hovered = Some(e);
                    }
                }
            }

            // Which row (and whether its fold arrow) the pointer is over.
            let row_at = |p: egui::Pos2| {
                let r = ((p.y - rect.top() - axis_h - 4.0) / row_h).floor();
                (r >= 0.0 && (r as usize) < rows.len()).then_some(r as usize)
            };
            let hovered = pointer.and_then(row_at);
            let arrow_at = |row: usize| {
                let s = reg.get(rows[row]).unwrap();
                egui::pos2(x_of(s.founded) - 8.0, y_of(row))
            };
            let on_arrow = |p: egui::Pos2, row: usize| {
                !kids(rows[row]).is_empty() && p.distance(arrow_at(row)) < (row_h * 0.6).max(6.0)
            };

            for (row, &id) in rows.iter().enumerate() {
                let y = y_of(row);
                if y < visible.top() - row_h || y > visible.bottom() + row_h {
                    continue;
                }
                let s = reg.get(id).unwrap();
                let end = s.extinct.unwrap_or(world.tick);
                let (x0, x1) = (x_of(s.founded), x_of(end).max(x_of(s.founded) + 1.5));
                let alive = s.extinct.is_none();
                let on_line = line.contains(&id);
                if hovered == Some(row) || selection.species == Some(id) {
                    let band = egui::Rect::from_min_max(
                        egui::pos2(rect.left(), y - row_h * 0.5),
                        egui::pos2(rect.right(), y + row_h * 0.5),
                    );
                    let shade = if selection.species == Some(id) {
                        45
                    } else {
                        32
                    };
                    painter.rect_filled(band, 0.0, egui::Color32::from_gray(shade));
                }
                // Link down from the parent.
                if let Some(&p) = parent_of(id).and_then(|p| row_of.get(&p)) {
                    let gray = if on_line { 150 } else { 70 };
                    painter.line_segment(
                        [egui::pos2(x0, y_of(p)), egui::pos2(x0, y)],
                        egui::Stroke::new(1.0, egui::Color32::from_gray(gray)),
                    );
                }
                let color = color32(s.color);
                let color = if alive {
                    color
                } else {
                    color.gamma_multiply(0.45)
                };
                let thick = (row_h * 0.45).clamp(2.0, 7.0);
                let width = if on_line {
                    thick + 2.0
                } else if alive {
                    thick
                } else {
                    (thick * 0.5).max(1.0)
                };
                painter.line_segment(
                    [egui::pos2(x0, y), egui::pos2(x1, y)],
                    egui::Stroke::new(width, color),
                );
                // Fold arrow, and how much is folded away.
                let hidden = if state.folded.contains(&id) {
                    descendants[&id]
                } else {
                    0
                };
                if !kids(id).is_empty() {
                    // A triangle: pointing right when folded, down when open.
                    let hot = pointer.is_some_and(|p| on_arrow(p, row));
                    let c = arrow_at(row);
                    let r = (row_h * 0.3).clamp(2.5, 5.0);
                    let points = if hidden > 0 {
                        vec![
                            c + egui::vec2(-r * 0.6, -r),
                            c + egui::vec2(r * 0.9, 0.0),
                            c + egui::vec2(-r * 0.6, r),
                        ]
                    } else {
                        vec![
                            c + egui::vec2(-r, -r * 0.6),
                            c + egui::vec2(r, -r * 0.6),
                            c + egui::vec2(0.0, r * 0.9),
                        ]
                    };
                    let gray = egui::Color32::from_gray(if hot { 230 } else { 140 });
                    painter.add(egui::Shape::convex_polygon(
                        points,
                        gray,
                        egui::Stroke::NONE,
                    ));
                }
                if labels {
                    let mut text = s.name.clone();
                    if hidden > 0 {
                        text += &format!("  (+{hidden})");
                    }
                    let gray = if alive { 210 } else { 120 };
                    painter.text(
                        egui::pos2(x1 + 5.0, y),
                        egui::Align2::LEFT_CENTER,
                        text,
                        egui::FontId::proportional((row_h * 0.72).clamp(9.0, 14.0)),
                        egui::Color32::from_gray(gray),
                    );
                }
                if state.reveal && selection.species == Some(id) {
                    ui.scroll_to_rect(
                        egui::Rect::from_center_size(
                            egui::pos2(x0, y),
                            egui::vec2(1.0, row_h * 6.0),
                        ),
                        Some(egui::Align::Center),
                    );
                    state.reveal = false;
                }
            }

            // An event marker: say what happened, and click to go there.
            if let Some(e) = event_hovered {
                let x = x_of(e.tick);
                painter.line_segment(
                    [egui::pos2(x, marker_y), egui::pos2(x, rect.bottom())],
                    egui::Stroke::new(1.5, super::event_color(e.kind)),
                );
                if response.clicked()
                    && let Some(pos) = e.pos
                {
                    selection.follow = false;
                    cam.focus(bevy::prelude::Vec2::new(pos.x, pos.y), Some(2.5));
                }
                response.on_hover_ui_at_pointer(|ui| {
                    ui.label(
                        egui::RichText::new(format_game_date(e.time, &world.config.time)).strong(),
                    );
                    ui.colored_label(super::event_color(e.kind), &e.text);
                    if e.pos.is_some() {
                        ui.label(egui::RichText::new("Click to go there").weak());
                    }
                });
                return;
            }

            // Clicks: the arrow folds, anywhere else on a row focuses the species.
            if response.clicked()
                && let Some(p) = response.interact_pointer_pos()
                && let Some(row) = row_at(p)
            {
                let id = rows[row];
                if on_arrow(p, row) {
                    crate::ui_sfx::silent(&response);
                    if !state.folded.remove(&id) {
                        state.folded.insert(id);
                    }
                } else {
                    selection.species = Some(id);
                    if let Some(o) = world.organisms.iter().find(|o| o.species == id) {
                        selection.organism = Some(o.id);
                        selection.follow = true;
                        cam.focus(
                            bevy::prelude::Vec2::new(o.pos.x, o.pos.y),
                            Some((o.radius / 10.0).clamp(0.2, 3.0)),
                        );
                    }
                }
            }
            if let Some(row) = hovered {
                let id = rows[row];
                let s = reg.get(id).unwrap();
                response.on_hover_ui_at_pointer(|ui| {
                    ui.horizontal(|ui| {
                        super::swatch(ui, s.color, 12.0);
                        ui.label(egui::RichText::new(&s.name).strong());
                    });
                    if let Some(p) = parent_of(id) {
                        ui.label(format!("Descended from {}", reg.name(p)));
                    }
                    ui.label(format!(
                        "Founded in {}",
                        format_game_date(s.founded as f64 * dt as f64, &world.config.time)
                    ));
                    match s.extinct {
                        Some(t) => ui.label(format!(
                            "Extinct in {} (lasted {})",
                            format_game_date(t as f64 * dt as f64, &world.config.time),
                            format_game_duration((t - s.founded) as f32 * dt, &world.config.time)
                        )),
                        None => ui.label(format!("Alive: {} now", s.population)),
                    };
                    ui.label(format!("Peak {} · {} born in all", s.peak, s.total_born));
                    let n = descendants[&id];
                    if n > 0 {
                        ui.label(format!(
                            "{n} descendant species, {} living",
                            living_below[&id]
                        ));
                    }
                    let hint = if s.extinct.is_none() {
                        "Click to find one"
                    } else {
                        "Click to highlight"
                    };
                    let fold = if n > 0 {
                        " · arrow folds its lineage"
                    } else {
                        ""
                    };
                    ui.label(egui::RichText::new(format!("{hint}{fold}")).weak());
                });
            }
        });
}
