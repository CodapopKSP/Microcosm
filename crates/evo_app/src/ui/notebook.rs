//! The field notebook: a paper book of hand-written entries with pencil sketches of the
//! specimens the viewer recorded. Page 1 is the contents; each species gets half a page.

use std::sync::Arc;

use bevy_egui::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, Sense, Stroke, Vec2,
    pos2, vec2,
};
use evo_core::genome::drive;
use evo_core::notebook::Entry;
use evo_core::rng::Rng;
use evo_core::traits::{format_game_date, format_game_duration};
use evo_core::{Traits, World};

use super::sketch::{self, GRAPHITE, Pencil, Subject};

pub const PAPER: Color32 = Color32::from_rgb(244, 237, 219);
const PAPER_EDGE: Color32 = Color32::from_rgb(226, 214, 188);
const RULE: Color32 = Color32::from_rgba_premultiplied(70, 95, 140, 40);
const MARGIN: Color32 = Color32::from_rgba_premultiplied(150, 60, 60, 60);
const COVER: Color32 = Color32::from_rgb(72, 48, 34);
const RED_PENCIL: Color32 = Color32::from_rgb(170, 60, 50);
/// Distance between ruled lines.
const LINE: f32 = 26.0;

const HAND: &str = "hand";
const HAND_BOLD: &str = "hand_bold";

/// Register the handwriting font (Kalam, SIL Open Font License) as the "hand" families, with
/// egui's own fonts as fallbacks for any missing symbols.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "kalam".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Kalam-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "kalam_bold".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Kalam-Bold.ttf"
        ))),
    );
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let family = |first: &str| {
        let mut v = vec![first.to_string()];
        v.extend(fallback.iter().cloned());
        v
    };
    fonts
        .families
        .insert(FontFamily::Name(HAND.into()), family("kalam"));
    fonts
        .families
        .insert(FontFamily::Name(HAND_BOLD.into()), family("kalam_bold"));
    ctx.set_fonts(fonts);
}

fn hand(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(HAND.into()))
}

fn hand_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(HAND_BOLD.into()))
}

#[derive(Default)]
pub struct NotebookState {
    /// Which two-page spread is open (0 = contents and the first entries).
    pub spread: usize,
}

impl NotebookState {
    /// Open to the spread holding entry `index`.
    pub fn show_entry(&mut self, index: usize) {
        self.spread = page_of(index) / 2;
    }
}

/// Page of entry `index` (page 0 is the contents; two entries per page).
fn page_of(index: usize) -> usize {
    1 + index / 2
}

/// Write `text` at `pos` (wrapped at `wrap`); returns the height used.
fn write(
    painter: &egui::Painter,
    pos: Pos2,
    text: &str,
    font: FontId,
    color: Color32,
    wrap: f32,
) -> f32 {
    let galley = painter.layout(text.to_string(), font, color, wrap);
    let h = galley.size().y;
    painter.galley(pos, galley, color);
    h
}

fn text_width(painter: &egui::Painter, text: &str, font: FontId) -> f32 {
    painter
        .layout_no_wrap(text.to_string(), font, GRAPHITE)
        .size()
        .x
}

/// What the viewer did in the notebook this frame.
pub enum Action {
    /// Tear out a species' entry.
    TearOut(u32),
}

/// Draw the notebook window. `open` is cleared when it's closed.
pub fn draw(
    ctx: &egui::Context,
    world: &World,
    state: &mut NotebookState,
    open: &mut bool,
) -> Option<Action> {
    let screen = ctx.content_rect();
    let size = vec2(
        (screen.width() - 80.0).clamp(640.0, 1080.0),
        (screen.height() - 120.0).clamp(420.0, 700.0),
    );
    let entries = &world.notebook.entries;
    let pages = 1 + entries.len().div_ceil(2);
    let spreads = pages.div_ceil(2).max(1);
    state.spread = state.spread.min(spreads - 1);
    let mut action = None;

    egui::Window::new("Field Notebook")
        .title_bar(false)
        .frame(egui::Frame::NONE)
        .resizable(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(screen.center())
        .show(ctx, |ui| {
            let (book, _) = ui.allocate_exact_size(size, Sense::hover());
            let painter = ui.painter_at(book.expand(12.0));
            // Cover and its shadow.
            painter.rect_filled(book.translate(vec2(0.0, 6.0)).expand(2.0), 16.0, Color32::from_black_alpha(90));
            painter.rect_filled(book, 14.0, COVER);
            painter.rect_stroke(book.shrink(5.0), 10.0, Stroke::new(1.0, Color32::from_rgb(110, 78, 56)), egui::StrokeKind::Middle);
            let inner = book.shrink(14.0);
            let mid = inner.center().x;
            let left = Rect::from_min_max(inner.min, pos2(mid - 1.0, inner.max.y));
            let right = Rect::from_min_max(pos2(mid + 1.0, inner.min.y), inner.max);

            for (side, page_rect) in [left, right].into_iter().enumerate() {
                let page = state.spread * 2 + side;
                paper(&painter, page_rect, page as u64, side == 0);
                if page >= pages {
                    continue;
                }
                let content = Rect::from_min_max(
                    page_rect.min + vec2(if side == 0 { 58.0 } else { 46.0 }, 22.0),
                    page_rect.max - vec2(if side == 0 { 24.0 } else { 34.0 }, 36.0),
                );
                if page == 0 {
                    contents(ui, &painter, content, world, state);
                } else {
                    let first = (page - 1) * 2;
                    let half = content.height() / 2.0;
                    for k in 0..2 {
                        let Some(entry) = entries.get(first + k) else {
                            continue;
                        };
                        let r = Rect::from_min_size(
                            content.min + vec2(0.0, half * k as f32),
                            vec2(content.width(), half - 6.0),
                        );
                        if k == 1 {
                            // A pencilled divider between the two halves.
                            let mut pen = Pencil::new(&painter, 7 + page as u64);
                            let y = r.top() - 6.0;
                            pen.line(&[pos2(r.left() + 30.0, y), pos2(r.right() - 30.0, y + 1.5)], 1.0, 0.5);
                        }
                        if let Some(a) = entry_page(ui, &painter, r, world, entry, first + k) {
                            action = Some(a);
                        }
                    }
                }
                // Page number.
                let num = format!("{}", page + 1);
                let y = page_rect.bottom() - 26.0;
                let x = if side == 0 { page_rect.left() + 30.0 } else { page_rect.right() - 40.0 };
                write(&painter, pos2(x, y), &num, hand(15.0), GRAPHITE.gamma_multiply(0.7), 100.0);
            }

            // Turning pages.
            // A pencilled arrow next to the words (the font has no arrow glyphs).
            let arrow = |ui: &mut egui::Ui, at: Pos2, text: &str, forward: bool| -> bool {
                let font = hand_bold(17.0);
                let w = text_width(&painter, text, font.clone());
                let rect = Rect::from_center_size(at, vec2(w + 50.0, 26.0));
                let r = ui.interact(rect, ui.id().with(text), Sense::click());
                let color = if r.hovered() { RED_PENCIL } else { GRAPHITE };
                painter.text(at, egui::Align2::CENTER_CENTER, text, font, color);
                let sign = if forward { 1.0 } else { -1.0 };
                let tail = at + vec2(sign * (w / 2.0 + 8.0), 2.0);
                let head = tail + vec2(sign * 22.0, 0.0);
                let mut pen = Pencil::new(&painter, if forward { 11 } else { 12 });
                pen.line_colored(&[tail, head], 1.3, 0.9, color);
                pen.line_colored(&[head + vec2(-sign * 7.0, -5.0), head], 1.3, 0.9, color);
                pen.line_colored(&[head + vec2(-sign * 7.0, 5.0), head], 1.3, 0.9, color);
                r.clicked()
            };
            if state.spread > 0
                && arrow(ui, pos2(left.center().x, left.bottom() - 18.0), "turn back", false)
            {
                state.spread -= 1;
            }
            if state.spread + 1 < spreads
                && arrow(ui, pos2(right.center().x, right.bottom() - 18.0), "turn over", true)
            {
                state.spread += 1;
            }

            // Close: a pencilled cross in the corner.
            let cross = Rect::from_center_size(right.right_top() + vec2(-18.0, 18.0), vec2(22.0, 22.0));
            let r = ui.interact(cross, ui.id().with("close"), Sense::click());
            let mut pen = Pencil::new(&painter, 99);
            let c = cross.shrink(5.0);
            let dark = if r.hovered() { 1.0 } else { 0.7 };
            pen.line(&[c.left_top(), c.right_bottom()], 1.6, dark);
            pen.line(&[c.right_top(), c.left_bottom()], 1.6, dark);
            if r.on_hover_text("Close (N)").clicked() {
                *open = false;
            }
        });
    action
}

/// A sheet of paper: cream with faint specks, ruled lines, a margin line, and shading toward
/// the spine.
fn paper(painter: &egui::Painter, rect: Rect, seed: u64, is_left: bool) {
    painter.rect_filled(rect, 4.0, PAPER);
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0, PAPER_EDGE), egui::StrokeKind::Inside);
    let mut rng = Rng::new(seed * 7919 + 13);
    for _ in 0..140 {
        let p = pos2(
            rect.left() + rng.f32() * rect.width(),
            rect.top() + rng.f32() * rect.height(),
        );
        painter.circle_filled(p, 0.6 + rng.f32(), Color32::from_rgba_unmultiplied(150, 120, 80, 18));
    }
    let mut y = rect.top() + 70.0;
    while y < rect.bottom() - 30.0 {
        painter.line_segment(
            [pos2(rect.left() + 8.0, y), pos2(rect.right() - 8.0, y)],
            Stroke::new(1.0, RULE),
        );
        y += LINE;
    }
    let mx = if is_left { rect.left() + 48.0 } else { rect.left() + 36.0 };
    painter.line_segment(
        [pos2(mx, rect.top()), pos2(mx, rect.bottom())],
        Stroke::new(1.0, MARGIN),
    );
    // The spine's shadow.
    for i in 0..14 {
        let a = (14 - i) as u8 * 3;
        let x = if is_left { rect.right() - i as f32 } else { rect.left() + i as f32 };
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(80, 60, 40, a)),
        );
    }
}

/// Page 1: the title and a list of the species recorded.
fn contents(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    r: Rect,
    world: &World,
    state: &mut NotebookState,
) {
    let mut pen = Pencil::new(painter, 3);
    let title = "Field Notebook";
    let font = hand_bold(38.0);
    let w = text_width(painter, title, font.clone());
    write(painter, r.min, title, font, GRAPHITE, r.width());
    pen.line(&[r.min + vec2(0.0, 50.0), r.min + vec2(w, 48.0)], 1.6, 0.8);
    pen.line(&[r.min + vec2(6.0, 55.0), r.min + vec2(w * 0.8, 54.0)], 1.0, 0.5);
    write(
        painter,
        r.min + vec2(2.0, 62.0),
        "Observations of life in the Microcosm",
        hand(17.0),
        GRAPHITE.gamma_multiply(0.85),
        r.width(),
    );
    let entries = &world.notebook.entries;
    let mut y = r.top() + 70.0 + LINE * 2.0 - 22.0;
    if entries.is_empty() {
        write(
            painter,
            pos2(r.left(), y),
            "Nothing recorded yet.\n\nClick an organism to inspect it, then press \
             \"Record in Field Notebook\" to sketch it here. Record more of the same \
             species and their sketches sit side by side.",
            hand(17.0),
            GRAPHITE.gamma_multiply(0.85),
            r.width(),
        );
        return;
    }
    write(painter, pos2(r.left(), y), "Contents", hand_bold(20.0), GRAPHITE, r.width());
    y += LINE;
    for (i, e) in entries.iter().enumerate() {
        if y > r.bottom() - LINE {
            write(painter, pos2(r.left(), y), "…", hand(17.0), GRAPHITE, r.width());
            break;
        }
        let page = page_of(i) + 1;
        let name = format!("{}.  {}", i + 1, e.name);
        let row = Rect::from_min_size(pos2(r.left(), y), vec2(r.width(), LINE - 2.0));
        let resp = ui.interact(row, ui.id().with(("contents", i)), Sense::click());
        let color = if resp.hovered() { RED_PENCIL } else { GRAPHITE };
        write(painter, row.min, &name, hand(17.0), color, r.width() - 60.0);
        let num = format!("p. {page}");
        let nw = text_width(painter, &num, hand(17.0));
        write(painter, pos2(r.right() - nw, y), &num, hand(17.0), color, 80.0);
        // Dotted leader.
        let name_w = text_width(painter, &name, hand(17.0));
        let mut x = r.left() + name_w + 8.0;
        while x < r.right() - nw - 8.0 {
            painter.circle_filled(pos2(x, y + 17.0), 0.8, GRAPHITE.gamma_multiply(0.5));
            x += 6.0;
        }
        if resp.clicked() {
            state.show_entry(i);
        }
        y += LINE;
    }
}

/// Half a page: one species' entry.
fn entry_page(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    r: Rect,
    world: &World,
    entry: &Entry,
    index: usize,
) -> Option<Action> {
    let mut action = None;
    let latest = entry.specimens.last()?;
    let t = Traits::decode(&latest.genome, &world.config);
    let cfg = &world.config;
    let mut pen = Pencil::new(painter, entry.species as u64 * 31 + 5);

    // Heading: number and name, underlined; when it was first recorded.
    let name = format!("{}.  {}", index + 1, entry.name);
    // One line: a long name is written smaller.
    let room = r.width() - 150.0;
    let full = text_width(painter, &name, hand_bold(23.0));
    let size = (23.0 * room / full.max(1.0)).clamp(14.0, 23.0);
    let name_font = hand_bold(size);
    let nw = text_width(painter, &name, name_font.clone());
    write(painter, r.min + vec2(0.0, (23.0 - size) * 0.8), &name, name_font, GRAPHITE, f32::INFINITY);
    pen.line(&[r.min + vec2(0.0, 33.0), r.min + vec2(nw, 32.0)], 1.2, 0.7);
    let when = format!("first seen {}", format_game_date(entry.time, &cfg.time));
    let ww = text_width(painter, &when, hand(13.5));
    write(painter, pos2(r.right() - ww, r.top() + 6.0), &when, hand(13.5), GRAPHITE.gamma_multiply(0.75), 200.0);
    // How it's doing now.
    let status = match world.species.get(entry.species) {
        Some(s) if s.extinct.is_some() => Some(("extinct".to_string(), RED_PENCIL)),
        Some(s) => Some((format!("{} alive now", s.population), GRAPHITE)),
        None => None,
    };
    if let Some((text, color)) = status {
        let sw = text_width(painter, &text, hand(13.5));
        write(painter, pos2(r.right() - sw, r.top() + 22.0), &text, hand(13.5), color.gamma_multiply(0.85), 200.0);
    }

    // Tear out (on hover).
    if ui.rect_contains_pointer(r) {
        let tear = Rect::from_min_size(r.right_top() + vec2(-60.0, 40.0), vec2(60.0, 18.0));
        let resp = ui.interact(tear, ui.id().with(("tear", entry.species)), Sense::click());
        let color = if resp.hovered() { RED_PENCIL } else { GRAPHITE.gamma_multiply(0.5) };
        painter.text(tear.right_center(), egui::Align2::RIGHT_CENTER, "tear out", hand(13.0), color);
        if resp.on_hover_text("Remove this entry from the notebook").clicked() {
            action = Some(Action::TearOut(entry.species));
        }
    }

    // The sketches: oldest on the left; each new one pushes the others over and shrinks them.
    let band = Rect::from_min_max(
        pos2(r.left(), r.top() + 40.0),
        pos2(r.right(), r.top() + 40.0 + (r.height() * 0.4).max(80.0)),
    );
    let n = entry.specimens.len();
    let slot_w = band.width() / n as f32;
    for (i, s) in entry.specimens.iter().enumerate() {
        let slot = Rect::from_min_size(
            pos2(band.left() + slot_w * i as f32, band.top()),
            vec2(slot_w, band.height()),
        );
        let newest = i + 1 == n;
        let st = Traits::decode(&s.genome, cfg);
        let draw_rect = Rect::from_min_max(slot.min + vec2(6.0, 2.0), slot.max - vec2(6.0, 16.0));
        // Keep the sketch's labels inside the slot.
        let labelled = newest && n <= 2;
        let fig = if labelled { draw_rect.shrink2(vec2(draw_rect.width() * 0.12, 4.0)) } else { draw_rect };
        let labels = sketch::draw_specimen(
            painter,
            fig,
            &Subject { genome: &s.genome, traits: &st },
            s.organism,
            if newest { 1.0 } else { 0.8 },
        );
        if labelled {
            label_parts(painter, draw_rect, &labels, s.organism);
        }
        let caption = format!("No. {}  ·  gen. {}", i + 1, s.generation);
        let cw = text_width(painter, &caption, hand(12.5));
        write(
            painter,
            pos2(slot.center().x - cw / 2.0, slot.bottom() - 15.0),
            &caption,
            hand(12.5),
            GRAPHITE.gamma_multiply(0.7),
            slot_w,
        );
    }

    // The genome goes at the bottom; measure it first so nothing runs into it.
    let genome_text = latest.genome.to_string();
    let genome_font = hand(12.0);
    let genome = painter.layout(
        genome_text,
        genome_font,
        GRAPHITE.gamma_multiply(0.8),
        r.width() - 52.0,
    );
    let genome_h = genome.size().y + 6.0;

    // Notes (left) and measurements (right).
    let top = band.bottom() + 6.0;
    let col_w = r.width() * 0.58;
    let notes = Rect::from_min_max(pos2(r.left(), top), pos2(r.left() + col_w - 10.0, r.bottom() - genome_h));
    let stats = Rect::from_min_max(pos2(r.left() + col_w + 6.0, top), pos2(r.right(), r.bottom() - genome_h));
    let small = hand(14.5);
    let mut y = notes.top();
    y += write(painter, pos2(notes.left(), y), &diet_text(&t), small.clone(), GRAPHITE, notes.width());
    for line in observations(&t).into_iter().take(5) {
        if y > notes.bottom() - 14.0 {
            break;
        }
        y += write(painter, pos2(notes.left(), y), &format!("– {line}"), small.clone(), GRAPHITE, notes.width());
    }
    let mut y = stats.top();
    let tc = &cfg.time;
    for line in [
        format!("size: radius {:.1}", t.adult_radius),
        format!("lives: {}", format_game_duration(t.lifespan, tc)),
        format!("water: {:.0} °C ± {:.0}", t.temp_pref, t.temp_tol),
        format!("young: {} per brood", t.brood),
        format!("parts: {}", t.part_count),
    ] {
        if y > stats.bottom() - 14.0 {
            break;
        }
        y += write(painter, pos2(stats.left(), y), &line, small.clone(), GRAPHITE.gamma_multiply(0.9), stats.width());
    }

    // The genome, copied out in small handwriting.
    let gy = r.bottom() - genome_h + 2.0;
    write(painter, pos2(r.left(), gy), "genome", hand_bold(12.5), GRAPHITE.gamma_multiply(0.8), 80.0);
    painter.galley(pos2(r.left() + 52.0, gy), genome, GRAPHITE);
    action
}

/// Leader lines from parts to their names, in the margin of the sketch.
fn label_parts(painter: &egui::Painter, area: Rect, labels: &[(Pos2, Vec2, &str)], seed: u64) {
    let mut pen = Pencil::new(painter, seed ^ 0xABCD);
    let font = hand(13.0);
    let mut used: Vec<Rect> = Vec::new();
    for &(at, out, name) in labels.iter().take(4) {
        let w = text_width(painter, name, font.clone());
        let side = if out.x >= 0.0 { 1.0 } else { -1.0 };
        // A little way out from the part, kept inside the sketch's area.
        let x = if side > 0.0 {
            (at.x + 28.0).min(area.right() - w)
        } else {
            (at.x - 28.0 - w).max(area.left())
        };
        let mut y = (at.y + out.y * 20.0 - 8.0).clamp(area.top(), area.bottom() - 16.0);
        let mut rect = Rect::from_min_size(pos2(x, y), vec2(w, 16.0));
        // Don't stack labels on top of each other.
        while used.iter().any(|u| u.intersects(rect)) && y < area.bottom() - 16.0 {
            y += 16.0;
            rect = Rect::from_min_size(pos2(x, y), vec2(w, 16.0));
        }
        used.push(rect);
        let end = if side > 0.0 { rect.left_center() - vec2(3.0, 0.0) } else { rect.right_center() + vec2(3.0, 0.0) };
        pen.line(&[at, end], 0.7, 0.6);
        painter.circle_filled(at, 1.4, GRAPHITE.gamma_multiply(0.8));
        write(painter, rect.min, name, font.clone(), GRAPHITE.gamma_multiply(0.9), w + 4.0);
    }
}

fn diet_text(t: &Traits) -> String {
    let foods = [
        (t.photo, "sunlight"),
        (t.chemo, "vent minerals"),
        (t.scavenge, "dead matter"),
        (t.hunt, "other organisms"),
    ];
    let mut sorted = foods.to_vec();
    sorted.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let (best, name) = sorted[0];
    if best <= 0.01 {
        return "Feeds on: nothing much.".into();
    }
    let extras: Vec<&str> = sorted[1..]
        .iter()
        .filter(|f| f.0 >= best * 0.35 && f.0 > 0.05)
        .map(|f| f.1)
        .collect();
    if extras.is_empty() {
        format!("Feeds on {name}.")
    } else {
        format!("Feeds on {name}, also {}.", extras.join(" and "))
    }
}

/// Plain notes on what stands out about it, most notable first.
fn observations(t: &Traits) -> Vec<String> {
    let d = &t.drives;
    let mut notes: Vec<(f32, String)> = Vec::new();
    let mut add = |score: f32, text: String| {
        if score > 0.0 {
            notes.push((score, text));
        }
    };
    if t.swim > 0.3 {
        add(0.9, "swims fast".into());
    } else if t.swim > 0.03 {
        add(0.6, "swims".into());
    } else if t.crawl_coeff > 0.05 {
        add(0.7, "walks along the rock".into());
    } else if t.anchor > 0.0 {
        add(0.7, "anchors itself to rock".into());
    } else {
        add(0.4, "drifts with the current".into());
    }
    if t.eye_count > 0 {
        let n = t.eye_count;
        add(0.85, format!("{n} eye{}", if n == 1 { "" } else { "s" }));
    }
    if t.antenna_count > 0 {
        add(0.6, "feels the water with antennae".into());
    }
    add(t.toxin * 1.5, "poisonous".into());
    add(t.armor * 1.6, "armoured with shell plates".into());
    add(t.spikes * 1.6, "covered in spines".into());
    add(t.jaws * 1.2, "strong jaws".into());
    add(t.lure * 2.0, "glows to lure prey".into());
    add(t.grasp_count as f32 * 0.4, "grabs food with tentacles".into());
    add(t.filter * 0.6 - 0.1, "sweeps food from the water".into());
    add(t.fixation * 1.2 - 0.2, "makes its own nitrogen".into());
    add((0.35 - t.aerobic) * 2.5, "lives without oxygen".into());
    add((0.5 - t.oxygenic) * 1.5 * t.photo, "photosynthesis without oxygen".into());
    add((t.temp_pref - 26.0) / 8.0, "likes warm water".into());
    add((12.0 - t.temp_pref) / 8.0, "likes cold water".into());
    add(d[drive::KIN] * 0.9, "stays close to its own kind".into());
    add(-d[drive::KIN] * 0.9, "keeps away from its own kind".into());
    add(-d[drive::LARGER] * 0.9, "flees from larger things".into());
    add(d[drive::SMALLER] * t.hunt * 1.5, "chases smaller prey".into());
    add(d[drive::LIGHT] * 0.7, "seeks the light".into());
    add(-d[drive::LIGHT] * 0.7, "shuns the light".into());
    add(d[drive::CARRION] * 0.7, "seeks out carcasses".into());
    add((t.segments as f32 - 1.5) * 0.3, format!("{} segments", t.segments));
    add(t.elongation - 0.4, "long, worm-like body".into());
    notes.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    notes.into_iter().map(|(_, s)| s).collect()
}
