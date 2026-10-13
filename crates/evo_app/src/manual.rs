//! The manual (`docs/manual/manual.md`), read in the game: the main menu's Documentation page.
//!
//! The Markdown is shown with `egui_commonmark`, split into a block per heading (so the contents
//! can scroll to it) with a numbered caption under each figure, from its alt text, as the PDF
//! build (`docs/manual/build.py`) does. Tables are drawn here rather than by the viewer, whose
//! tables don't wrap their text. Figures are built into the program; SVG figures are shown as
//! their PNG copies (made by `docs/manual/figures.py`).

use bevy_egui::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::menu::{ACCENT, MUTED, TEXT, card};

const SOURCE: &str = include_str!("../../../docs/manual/manual.md");

/// Every PNG and JPEG in `docs/manual/figures`, as (file name, bytes) (listed by build.rs).
const FIGURES: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/manual_figures.rs"));

/// Where the figures are found by egui's image loaders: `figures/x.png` in the Markdown becomes
/// this plus `figures/x.png`.
const URI: &str = "bytes://manual/";

/// The widest the text runs, so lines stay readable on a wide screen.
const TEXT_WIDTH: f32 = 860.0;

/// Body text colour (bold text is white).
const BODY: egui::Color32 = egui::Color32::from_rgb(200, 218, 218);
const RULE: egui::Color32 = egui::Color32::from_rgba_premultiplied(20, 36, 34, 36);

/// A heading in the contents.
struct Entry {
    level: usize,
    title: String,
    id: String,
}

/// A run of Markdown (starting at the heading with id `anchor`, if any), or a table (each cell
/// is Markdown too).
enum Block {
    Text {
        anchor: Option<String>,
        text: String,
    },
    Table {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
    },
}

pub struct Manual {
    blocks: Vec<Block>,
    contents: Vec<Entry>,
    cache: CommonMarkCache,
    figures_loaded: bool,
    /// The heading picked in the contents, to scroll to.
    scroll_to: Option<String>,
}

impl Default for Manual {
    fn default() -> Self {
        let (blocks, contents) = prepare(SOURCE);
        Manual {
            blocks,
            contents,
            cache: CommonMarkCache::default(),
            figures_loaded: false,
            scroll_to: None,
        }
    }
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// The cells of a table row: `| a | b |` → `["a", "b"]`.
fn cells(line: &str) -> Vec<String> {
    let inner = line.trim().trim_start_matches('|').trim_end_matches('|');
    inner.split('|').map(|c| c.trim().to_string()).collect()
}

fn is_table_rule(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// Split the manual into Markdown runs (a new one at each heading, with the heading's id) and
/// tables. Headings with ids go into the contents; figures get numbered captions, and SVG figures
/// are swapped for their PNG copies.
fn prepare(source: &str) -> (Vec<Block>, Vec<Entry>) {
    let mut blocks = Vec::new();
    let mut text = String::new();
    let mut anchor: Option<String> = None;
    let flush = |blocks: &mut Vec<Block>, anchor: &mut Option<String>, text: &mut String| {
        if !text.trim().is_empty() {
            blocks.push(Block::Text {
                anchor: anchor.take(),
                text: std::mem::take(text),
            });
        }
        text.clear();
    };
    let mut contents = Vec::new();
    let mut used = std::collections::HashSet::new();
    let mut in_code = false;
    let mut figure = 0;
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        if line.starts_with("```") {
            in_code = !in_code;
        }
        if in_code || line.starts_with("```") {
            text += line;
            text.push('\n');
            continue;
        }
        if line.starts_with('|') && lines.get(i).is_some_and(|l| is_table_rule(l)) {
            let header = cells(line);
            i += 1;
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].starts_with('|') {
                rows.push(cells(lines[i]));
                i += 1;
            }
            flush(&mut blocks, &mut anchor, &mut text);
            blocks.push(Block::Table { header, rows });
            continue;
        }
        let hashes = line.chars().take_while(|&c| c == '#').count();
        if (1..=3).contains(&hashes) && line[hashes..].starts_with(' ') {
            let title = line[hashes..].trim();
            let mut id = slug(title);
            let mut n = 2;
            while !used.insert(id.clone()) {
                // The same heading in two chapters.
                id = format!("{}-{n}", slug(title));
                n += 1;
            }
            flush(&mut blocks, &mut anchor, &mut text);
            text += line;
            text.push('\n');
            if hashes <= 2 {
                contents.push(Entry {
                    level: hashes,
                    title: title.replace(['*', '`'], ""),
                    id: id.clone(),
                });
            }
            anchor = Some(id);
            continue;
        }
        if line.starts_with("![")
            && line.ends_with(')')
            && let Some(split) = line.find("](")
        {
            figure += 1;
            let alt = &line[2..split];
            let src = &line[split + 2..line.len() - 1];
            let src = match src.strip_suffix(".svg") {
                Some(stem) => format!("{stem}.png"),
                None => src.to_string(),
            };
            text += &format!("![]({src})\n\n**Figure {figure}.** {alt}\n");
            continue;
        }
        text += line;
        text.push('\n');
    }
    flush(&mut blocks, &mut anchor, &mut text);
    (blocks, contents)
}

/// Column widths for a table `width` wide: as wide as their text, until that doesn't fit; then
/// the narrow columns keep their width and the wide ones share what's left.
fn column_widths(header: &[String], rows: &[Vec<String>], width: f32, gap: f32) -> Vec<f32> {
    let n = header.len();
    let natural: Vec<f32> = (0..n)
        .map(|c| {
            let longest = std::iter::once(header)
                .chain(rows.iter().map(|r| r.as_slice()))
                .filter_map(|r| r.get(c))
                .map(|s| s.chars().count())
                .max()
                .unwrap_or(0);
            longest as f32 * 7.6 + 4.0
        })
        .collect();
    let room = width - gap * (n.saturating_sub(1)) as f32;
    if natural.iter().sum::<f32>() <= room {
        return natural;
    }
    let mut fixed = vec![false; n];
    loop {
        let flexible = fixed.iter().filter(|f| !**f).count().max(1) as f32;
        let used: f32 = (0..n).filter(|&c| fixed[c]).map(|c| natural[c]).sum();
        let share = (room - used) / flexible;
        let mut changed = false;
        for c in 0..n {
            if !fixed[c] && natural[c] <= share {
                fixed[c] = true;
                changed = true;
            }
        }
        if !changed {
            return (0..n)
                .map(|c| if fixed[c] { natural[c] } else { share.max(60.0) })
                .collect();
        }
    }
}

impl Manual {
    /// The contents beside the text, both scrolling on their own.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        if !self.figures_loaded {
            egui_extras::install_image_loaders(ui.ctx());
            for (name, bytes) in FIGURES {
                ui.ctx()
                    .include_bytes(format!("{URI}figures/{name}"), *bytes);
            }
            self.figures_loaded = true;
        }
        let full = ui.available_rect_before_wrap();
        let gap = 14.0;
        let side = 250.0f32.min(full.width() * 0.3);
        let text_width = (full.width() - side - gap).min(TEXT_WIDTH + 36.0);
        let left = full.center().x - (side + gap + text_width) * 0.5;
        let contents_rect = egui::Rect::from_min_size(
            egui::pos2(left, full.top()),
            egui::vec2(side, full.height()),
        );
        let text_rect = egui::Rect::from_min_size(
            egui::pos2(left + side + gap, full.top()),
            egui::vec2(text_width, full.height()),
        );

        panel(ui, contents_rect, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("manual_contents")
                .auto_shrink(false)
                .show(ui, |ui| {
                    for entry in &self.contents {
                        let text = if entry.level == 1 {
                            egui::RichText::new(&entry.title).size(15.0).color(ACCENT)
                        } else {
                            egui::RichText::new(&entry.title).size(13.0).color(MUTED)
                        };
                        if entry.level == 1 {
                            ui.add_space(6.0);
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(if entry.level == 1 { 0.0 } else { 12.0 });
                            let r = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
                            let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
                            if r.hovered() {
                                ui.painter().text(
                                    r.rect.left_center() - egui::vec2(8.0, 0.0),
                                    egui::Align2::CENTER_CENTER,
                                    "›",
                                    egui::FontId::proportional(14.0),
                                    TEXT,
                                );
                            }
                            if r.clicked() {
                                self.scroll_to = Some(entry.id.clone());
                            }
                        });
                    }
                });
        });

        panel(ui, text_rect, |ui| {
            let style = ui.style_mut();
            style.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
            style.text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(30.0));
            style.text_styles.insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.5));
            // The menus force one text colour; here bold text (and headings) must stand out.
            style.visuals.override_text_color = None;
            style.visuals.widgets.noninteractive.fg_stroke.color = BODY;
            style.visuals.widgets.active.fg_stroke.color = egui::Color32::WHITE;
            egui::ScrollArea::vertical()
                .id_salt("manual_text")
                .auto_shrink(false)
                .show(ui, |ui| {
                    let width = ui.available_width();
                    for (k, block) in self.blocks.iter().enumerate() {
                        ui.push_id(k, |ui| match block {
                            Block::Text { anchor, text } => {
                                if anchor.is_some() && *anchor == self.scroll_to {
                                    ui.scroll_to_cursor(Some(egui::Align::TOP));
                                    self.scroll_to = None;
                                }
                                viewer(width).show(ui, &mut self.cache, text);
                            }
                            Block::Table { header, rows } => {
                                table(ui, &mut self.cache, header, rows, width);
                            }
                        });
                    }
                    ui.add_space(24.0);
                });
        });
    }
}

fn viewer(width: f32) -> CommonMarkViewer<'static> {
    CommonMarkViewer::new()
        .default_implicit_uri_scheme(URI)
        .show_alt_text_on_hover(false)
        .max_image_width(Some(width as usize))
}

/// A table whose cells wrap: a header row, then the rows, with faint rules between them.
fn table(
    ui: &mut egui::Ui,
    cache: &mut CommonMarkCache,
    header: &[String],
    rows: &[Vec<String>],
    width: f32,
) {
    let gap = 14.0;
    let widths = column_widths(header, rows, width, gap);
    ui.add_space(6.0);
    let rule = |ui: &mut egui::Ui, strong: bool| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
        let color = if strong { MUTED.gamma_multiply(0.6) } else { RULE };
        ui.painter().rect_filled(rect, 0.0, color);
    };
    let row = |ui: &mut egui::Ui, cache: &mut CommonMarkCache, row: &[String], bold: bool| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (c, w) in widths.iter().enumerate() {
                let cell = row.get(c).map(String::as_str).unwrap_or("");
                ui.allocate_ui_with_layout(
                    egui::vec2(*w, 0.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.set_width(*w);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.style_mut()
                            .text_styles
                            .insert(egui::TextStyle::Body, egui::FontId::proportional(13.5));
                        ui.push_id(c, |ui| {
                            if bold && !cell.is_empty() {
                                viewer(*w).show(ui, cache, &format!("**{cell}**"));
                            } else {
                                viewer(*w).show(ui, cache, cell);
                            }
                        });
                    },
                );
            }
        });
    };
    ui.push_id("header", |ui| row(ui, cache, header, true));
    rule(ui, true);
    for (r, cells) in rows.iter().enumerate() {
        ui.push_id(r, |ui| row(ui, cache, cells, false));
        rule(ui, false);
    }
    ui.add_space(10.0);
}

/// A card filling `rect` exactly (its margins inside the rect, not around it).
fn panel(ui: &mut egui::Ui, rect: egui::Rect, add: impl FnOnce(&mut egui::Ui)) {
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        let frame = card();
        let inner = rect.size() - frame.total_margin().sum();
        frame.show(ui, |ui| {
            ui.set_min_size(inner);
            ui.set_max_size(inner);
            add(ui);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(blocks: &[Block]) -> String {
        blocks
            .iter()
            .filter_map(|b| match b {
                Block::Text { text, .. } => Some(text.as_str()),
                Block::Table { .. } => None,
            })
            .collect()
    }

    #[test]
    fn headings_get_ids_and_figures_get_captions() {
        let (blocks, contents) = prepare(
            "# The World\n\n## Temperature\n\n![A chart.](figures/a.svg)\n\n# Life\n\n## Temperature\n",
        );
        let anchors: Vec<_> = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Text { anchor, .. } => anchor.clone(),
                Block::Table { .. } => None,
            })
            .collect();
        assert_eq!(anchors, ["the-world", "temperature", "life", "temperature-2"]);
        assert!(texts(&blocks).contains("![](figures/a.png)\n\n**Figure 1.** A chart."));
        assert_eq!(contents.len(), 4);
        assert_eq!(contents[0].id, "the-world");
    }

    #[test]
    fn tables_are_split_out() {
        let (blocks, _) = prepare("Before.\n\n| A | B |\n|---|---|\n| 1 | `x` |\n| 2 | 3 |\n\nAfter.\n");
        assert_eq!(blocks.len(), 3);
        let Block::Table { header, rows } = &blocks[1] else {
            panic!("no table")
        };
        assert_eq!(header, &["A", "B"]);
        assert_eq!(rows, &[vec!["1", "`x`"], vec!["2", "3"]]);
    }

    #[test]
    fn wide_tables_wrap_their_widest_columns() {
        let header = vec!["Gene".to_string(), "Meaning".to_string()];
        let rows = vec![vec!["B0".to_string(), "x".repeat(400)]];
        let w = column_widths(&header, &rows, 600.0, 10.0);
        assert!(w[0] < 60.0 && (w[0] + w[1] + 10.0 - 600.0).abs() < 1.0, "{w:?}");
    }

    #[test]
    fn every_figure_is_built_in() {
        let (blocks, _) = prepare(SOURCE);
        for line in texts(&blocks).lines().filter(|l| l.starts_with("![](figures/")) {
            let name = &line["![](figures/".len()..line.len() - 1];
            assert!(
                FIGURES.iter().any(|(n, _)| *n == name),
                "{name} is missing from docs/manual/figures"
            );
        }
    }
}
