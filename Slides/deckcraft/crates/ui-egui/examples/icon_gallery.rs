//! Renders every DeckCraft icon into a labelled PNG grid, on a light and a dark background.
//!
//! ```sh
//! cargo run -p deckcraft-ui-egui --example icon_gallery -- /tmp/gallery.png
//! ```
//!
//! Each cell shows the icon at 32 pt, at 16 pt, and disabled at 16 pt, with its name underneath.
//! An optional second argument filters icons by name substring.

use deckcraft_ui_egui::icons::{self, Icon};
use egui::{Align2, Color32, FontId, Rect, pos2, vec2};

const COLS: usize = 12;
const CELL_W: f32 = 110.0;
const CELL_H: f32 = 64.0;
const PAD: f32 = 12.0;

struct Theme {
    bg: Color32,
    ink: Color32,
    label: Color32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "icon_gallery.png".to_owned());
    let filter = args.next();
    let list: Vec<Icon> = Icon::ALL.iter().copied().filter(|i| filter.as_deref().is_none_or(|f| i.name().contains(f))).collect();

    let rows = list.len().div_ceil(COLS);
    let panel_h = rows as f32 * CELL_H + PAD * 2.0;
    let width = COLS as f32 * CELL_W + PAD * 2.0;
    let themes = [
        Theme { bg: Color32::from_rgb(0xff, 0xff, 0xff), ink: Color32::from_rgb(0x3a, 0x3a, 0x3a), label: Color32::from_rgb(0x70, 0x70, 0x70) },
        Theme { bg: Color32::from_rgb(0x2b, 0x2b, 0x2b), ink: Color32::from_rgb(0xe8, 0xe8, 0xe8), label: Color32::from_rgb(0xa8, 0xa8, 0xa8) },
    ];

    let mut harness =
        egui_kittest::Harness::builder().with_size(vec2(width, panel_h * themes.len() as f32)).with_pixels_per_point(2.0).wgpu().build_ui(|ui| {
            let painter = ui.painter();
            for (ti, theme) in themes.iter().enumerate() {
                let top = ti as f32 * panel_h;
                painter.rect_filled(Rect::from_min_size(pos2(0.0, top), vec2(width, panel_h)), 0.0, theme.bg);
                for (n, &icon) in list.iter().enumerate() {
                    let x = PAD + (n % COLS) as f32 * CELL_W;
                    let y = top + PAD + (n / COLS) as f32 * CELL_H;
                    let big = Rect::from_min_size(pos2(x + 14.0, y + 4.0), vec2(32.0, 32.0));
                    icons::paint(painter, big, icon, theme.ink, false);
                    let small = Rect::from_min_size(pos2(x + 56.0, y + 12.0), vec2(16.0, 16.0));
                    icons::paint(painter, small, icon, theme.ink, false);
                    let dis = Rect::from_min_size(pos2(x + 80.0, y + 12.0), vec2(16.0, 16.0));
                    icons::paint(painter, dis, icon, theme.ink, true);
                    painter.text(pos2(x + CELL_W * 0.5, y + 46.0), Align2::CENTER_CENTER, icon.name(), FontId::proportional(9.5), theme.label);
                }
            }
        });
    harness.run();
    let image = harness.render()?;
    if let Some(dir) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(dir)?;
    }
    image.save(&out)?;
    println!("wrote {} icons to {out}", list.len());
    Ok(())
}
