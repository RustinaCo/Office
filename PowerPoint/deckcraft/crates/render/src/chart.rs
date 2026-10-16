//! Charts: title, plot area with axes and gridlines, series marks, legend.

use deckcraft_color::{ColorTransform, Rgba, SchemeSlot};
use deckcraft_model::Shape;
use deckcraft_model::chart::{Chart, ChartType, axis_scale};
use deckcraft_model::resolve::Ctx;
use deckcraft_model::style::{ColorRef, Fill};
use deckcraft_model::text::{Align, Anchor, Bullet, TextBody};
use kurbo::{Affine, BezPath, Point, Rect, Shape as _};
use vello_cpu::RenderContext;

use crate::color;

const SLOTS: [SchemeSlot; 6] =
    [SchemeSlot::Accent1, SchemeSlot::Accent2, SchemeSlot::Accent3, SchemeSlot::Accent4, SchemeSlot::Accent5, SchemeSlot::Accent6];

/// Colour of series/point `i`: accents, then darker and lighter rounds.
pub fn series_color(ctx: &Ctx, c: &Chart, i: usize) -> Rgba {
    let slot = SLOTS.get(i % 6).copied().unwrap_or(SchemeSlot::Accent1);
    let base = ctx.color(&ColorRef::scheme(slot), None);
    let mono = c.palette.starts_with("monochrome");
    let base = if mono { ctx.color(&ColorRef::scheme(SchemeSlot::Accent1), None) } else { base };
    let round = if mono { i } else { i / 6 };
    match round {
        0 => base,
        1 => deckcraft_color::apply(base, &[ColorTransform::Shade(60000)]),
        2 => deckcraft_color::apply(base, &[ColorTransform::Tint(60000)]),
        n => deckcraft_color::apply(base, &[ColorTransform::LumMod(100000 - (n as i32 % 5) * 12000)]),
    }
}

fn series_fill(ctx: &Ctx, c: &Chart, i: usize, s: Option<&deckcraft_model::chart::Series>) -> Rgba {
    match s.and_then(|s| s.fill.as_ref()) {
        Some(Fill::Solid { color: cc }) => ctx.color(cc, None),
        _ => series_color(ctx, c, i),
    }
}

/// A line series' colour: its line's own (where PowerPoint keeps it), else its fill's, else the
/// palette's.
fn series_line(ctx: &Ctx, c: &Chart, i: usize, s: &deckcraft_model::chart::Series) -> Rgba {
    match s.line.as_ref().and_then(|l| l.fill.as_ref()) {
        Some(Fill::Solid { color: cc }) => ctx.color(cc, None),
        _ => series_fill(ctx, c, i, Some(s)),
    }
}

/// A category label on two lines, split at its middle space or dash ("03.11.2025 - 09.11.2025").
fn split_label(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mid = chars.len() / 2;
    let best = chars.iter().enumerate().filter(|(_, c)| c.is_whitespace()).map(|(i, _)| i).min_by_key(|i| i.abs_diff(mid));
    match best {
        Some(i) if i > 0 && i + 1 < chars.len() => {
            let (a, b) = chars.split_at(i);
            format!("{}\n{}", a.iter().collect::<String>().trim_end(), b.iter().collect::<String>().trim_start())
        }
        _ => name.to_string(),
    }
}

fn label(ctx: &mut RenderContext, rctx: &Ctx, m: Affine, text: &str, r: Rect, size: f64, align: Align, anchor: Anchor, col: Rgba, bold: bool) {
    let mut body = TextBody::from_text(text);
    body.body.inset_l = Some(0.0);
    body.body.inset_r = Some(0.0);
    body.body.inset_t = Some(0.0);
    body.body.inset_b = Some(0.0);
    body.body.anchor = Some(anchor);
    body.body.wrap = Some(false);
    for p in &mut body.paragraphs {
        p.props.align = Some(align);
        p.props.bullet = Some(Bullet::None);
        p.props.margin_left = Some(0.0);
        p.props.indent = Some(0.0);
        for run in &mut p.runs {
            run.props.size = Some(size);
            run.props.bold = Some(bold);
            run.props.fill = Some(Fill::solid(ColorRef::rgb(col)));
        }
    }
    let tmp = Shape { text: Some(body.clone()), ..Default::default() };
    crate::draw_text(ctx, rctx, &tmp, &body, r, m, &deckcraft_text::NoFields, None);
}

fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 { format!("{}", v.round() as i64) } else { format!("{v:.1}") }
}

/// Where the legend goes for `c:legendPos`, and the plot area left over.
/// `size` is the legend block (width, height); the result is its top-left corner and the plot area.
/// `r` (also the OOXML default and the fallback for unknown values) and `l` are vertical stacks centred
/// on the area, `tr` is a vertical stack in the top-right corner, `t` and `b` are centred rows.
/// An overlaid legend leaves the plot area as it was.
fn legend_layout(pos: &str, size: (f64, f64), area: Rect, base: f64, overlay: bool) -> (Point, Rect) {
    let (lw, lh) = size;
    let gap = base * 0.5;
    let cx = (area.x0 + area.x1) / 2.0;
    let cy = ((area.y0 + area.y1) / 2.0 - lh / 2.0).max(area.y0);
    let mut plot = area;
    let origin = match pos {
        "b" => {
            plot.y1 = area.y1 - lh - gap;
            Point::new(cx - lw / 2.0, area.y1 - lh)
        }
        "t" => {
            plot.y0 = area.y0 + lh + gap;
            Point::new(cx - lw / 2.0, area.y0)
        }
        "l" => {
            plot.x0 = area.x0 + lw + gap;
            Point::new(area.x0, cy)
        }
        "tr" => {
            plot.x1 = area.x1 - lw - gap;
            Point::new(area.x1 - lw, area.y0)
        }
        _ => {
            plot.x1 = area.x1 - lw - gap;
            Point::new(area.x1 - lw, cy)
        }
    };
    (origin, if overlay { area } else { plot })
}

pub fn draw(ctx: &mut RenderContext, rctx: &Ctx, c: &Chart, m: Affine, w: f64, h: f64) {
    let text_col = c.text_color.as_ref().map(|t| rctx.color(t, None)).unwrap_or(Rgba::rgb(0x59, 0x59, 0x59));
    let grid_col = Rgba::rgb(0xD9, 0xD9, 0xD9);
    if let Some(f) = &c.chart_fill {
        crate::paint::fill_path(
            ctx,
            rctx,
            f,
            None,
            &Rect::new(0.0, 0.0, w, h).to_path(0.1),
            Rect::new(0.0, 0.0, w, h),
            m,
            deckcraft_geom::preset::FillMode::Norm,
            1.0,
            rctx.pres,
        );
    }
    // The file's text sizes when it has them (PowerPoint draws chart text at them); otherwise a size
    // that suits the chart.
    let base = c.text_size.unwrap_or_else(|| (h.min(w) / 22.0).clamp(7.0, 18.0));
    let title_size = c.title_size.unwrap_or(base * 1.4);
    let mut area = Rect::new(w * 0.04, h * 0.04, w * 0.96, h * 0.96);
    if let Some(t) = &c.title {
        label(ctx, rctx, m, t, Rect::new(0.0, area.y0, w, area.y0 + title_size * 1.3), title_size, Align::Center, Anchor::Top, text_col, false);
        area.y0 += title_size * 1.7;
    }
    let pie = matches!(c.kind, ChartType::Pie | ChartType::Doughnut);
    // Legend placed by `c:legendPos`.
    let legend_items: Vec<(String, Rgba)> = if pie {
        c.categories.iter().enumerate().map(|(i, n)| (n.clone(), series_color(rctx, c, i))).collect()
    } else {
        c.series
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let k = c.series_kind(s);
                let line = !k.is_bar_like() && !matches!(k, ChartType::Area | ChartType::StackedArea | ChartType::FilledRadar);
                (s.name.clone(), if line { series_line(rctx, c, i, s) } else { series_fill(rctx, c, i, Some(s)) })
            })
            .collect()
    };
    if let (Some(pos), false) = (&c.legend, legend_items.is_empty()) {
        let vertical = matches!(pos.as_str(), "l" | "r" | "tr");
        let item_ws: Vec<f64> = legend_items.iter().map(|(n, _)| n.chars().count() as f64 * base * 0.55 + base * 1.6).collect();
        let size = if vertical {
            (item_ws.iter().copied().fold(0.0, f64::max).min(w * 0.4), legend_items.len() as f64 * base * 1.5)
        } else {
            (item_ws.iter().sum::<f64>().min(w * 0.9), base * 1.4)
        };
        let (origin, plot_area) = legend_layout(pos, size, area, base, c.legend_overlay);
        area = plot_area;
        let (mut x, mut y) = (origin.x, origin.y);
        for (name, col) in &legend_items {
            ctx.set_transform(m);
            ctx.set_paint(color(*col, 1.0));
            ctx.fill_rect(&Rect::new(x, y + base * 0.3, x + base * 0.7, y + base));
            let tw = name.chars().count() as f64 * base * 0.55 + base;
            label(ctx, rctx, m, name, Rect::new(x + base, y, x + base + tw, y + base * 1.3), base, Align::Left, Anchor::Middle, text_col, false);
            if vertical {
                y += base * 1.5;
            } else {
                x += tw + base * 0.6;
            }
        }
    }
    if pie {
        let Some(s) = c.series.first() else { return };
        let vals: Vec<f64> = s.values.iter().map(|v| v.unwrap_or(0.0).max(0.0)).collect();
        let total: f64 = vals.iter().sum();
        if total <= 0.0 {
            return;
        }
        let r = (area.width().min(area.height()) / 2.0) * 0.95;
        let cen = area.center();
        let mut a0 = -std::f64::consts::FRAC_PI_2;
        for (i, v) in vals.iter().enumerate() {
            let sweep = v / total * std::f64::consts::TAU;
            let mut p = BezPath::new();
            p.move_to(cen);
            let arc = kurbo::Arc::new(cen, (r, r), a0, sweep, 0.0);
            p.line_to(Point::new(cen.x + r * a0.cos(), cen.y + r * a0.sin()));
            p.extend(arc.append_iter(0.1));
            p.close_path();
            ctx.set_transform(m);
            let col = s
                .point_fills
                .iter()
                .find(|(k, _)| *k == i)
                .and_then(|(_, f)| if let Fill::Solid { color: cc } = f { Some(rctx.color(cc, None)) } else { None })
                .unwrap_or_else(|| series_color(rctx, c, i));
            ctx.set_paint(color(col, 1.0));
            ctx.fill_path(&p);
            ctx.set_paint(color(Rgba::WHITE, 1.0));
            ctx.set_stroke(kurbo::Stroke::new(1.0));
            ctx.stroke_path(&p);
            if c.data_labels {
                let mid = a0 + sweep / 2.0;
                let lp = Point::new(cen.x + r * 0.65 * mid.cos(), cen.y + r * 0.65 * mid.sin());
                label(
                    ctx,
                    rctx,
                    m,
                    &fmt_num(*v),
                    Rect::new(lp.x - 30.0, lp.y - 10.0, lp.x + 30.0, lp.y + 10.0),
                    base,
                    Align::Center,
                    Anchor::Middle,
                    Rgba::WHITE,
                    false,
                );
            }
            a0 += sweep;
        }
        if c.kind == ChartType::Doughnut {
            ctx.set_transform(m);
            ctx.set_paint(color(Rgba::WHITE, 1.0));
            ctx.fill_path(&kurbo::Circle::new(cen, r * c.hole_size.clamp(0.1, 0.9)).to_path(0.1));
        }
        return;
    }
    let combo = c.kind == ChartType::Combo;
    // Bars are drawn first and lines over them; a combo chart has both.
    let bar_kind = c.series.iter().map(|s| c.series_kind(s)).find(|k| k.is_bar_like());
    let horiz = if combo { bar_kind.is_some_and(ChartType::horizontal) } else { c.kind.horizontal() };
    // As many major units as the value axis has room for labels (PowerPoint's automatic unit).
    let axis_len = if horiz { area.width() - base * 4.5 } else { area.height() - base * 1.8 };
    let max_intervals = (axis_len / if horiz { base * 6.0 } else { base * 1.9 }).floor().clamp(2.0, 12.0) as usize;
    let (min, max, step) = c.axis_scale(false, max_intervals).unwrap_or_else(|| axis_scale(0.0, 1.0, None, None, None, max_intervals, true));
    // A combo chart's secondary value axis, on the right (vertical value axes only).
    let second = if horiz { None } else { c.axis_scale(true, max_intervals) };
    let ticks = |(lo, hi, st): (f64, f64, f64)| {
        let n = (((hi - lo) / st.max(1e-12)).round() as i64).clamp(1, 50);
        (0..=n).map(move |k| lo + k as f64 * st)
    };
    // Label gutters as wide as the longest value label.
    let label_w =
        |sec: bool, sc: (f64, f64, f64)| ticks(sc).map(|v| c.axis_label(sec, v).chars().count()).max().unwrap_or(1) as f64 * base * 0.55 + base * 0.6;
    let gutter = if horiz {
        base * 4.5
    } else if c.y_hidden {
        base * 0.5
    } else {
        label_w(false, (min, max, step)).clamp(base * 1.5, area.width() * 0.3)
    };
    let gutter_r = second.filter(|_| !c.y2_hidden).map(|sc| label_w(true, sc).clamp(base * 1.5, area.width() * 0.3)).unwrap_or(0.0);
    let plot = if horiz {
        Rect::new(area.x0 + gutter, area.y0, area.x1, area.y1 - base * 1.6)
    } else {
        Rect::new(area.x0 + gutter, area.y0, area.x1 - gutter_r, area.y1 - base * 1.8)
    };
    if let Some(f) = &c.plot_fill {
        crate::paint::fill_path(ctx, rctx, f, None, &plot.to_path(0.1), plot, m, deckcraft_geom::preset::FillMode::Norm, 1.0, rctx.pres);
    }
    let span = (max - min).max(1e-9);
    let to_v = |v: f64| if horiz { plot.x0 + (v - min) / span * plot.width() } else { plot.y1 - (v - min) / span * plot.height() };
    // A series' value as a position on its own axis.
    let to_axis = |sec: bool, v: f64| match (sec, second) {
        (true, Some((min2, max2, _))) => plot.y1 - (v - min2) / (max2 - min2).max(1e-9) * plot.height(),
        _ => to_v(v),
    };
    // Gridlines and value labels.
    for v in ticks((min, max, step)) {
        let pos = to_v(v);
        let text = if c.y_hidden { String::new() } else { c.axis_label(false, v) };
        let mut g = BezPath::new();
        if horiz {
            g.move_to((pos, plot.y0));
            g.line_to((pos, plot.y1));
            label(
                ctx,
                rctx,
                m,
                &text,
                Rect::new(pos - 30.0, plot.y1 + 2.0, pos + 30.0, plot.y1 + base * 1.5),
                base,
                Align::Center,
                Anchor::Top,
                text_col,
                false,
            );
        } else {
            g.move_to((plot.x0, pos));
            g.line_to((plot.x1, pos));
            label(
                ctx,
                rctx,
                m,
                &text,
                Rect::new(area.x0, pos - base, plot.x0 - base * 0.4, pos + base),
                base,
                Align::Right,
                Anchor::Middle,
                text_col,
                false,
            );
        }
        if c.gridlines {
            ctx.set_transform(m);
            ctx.set_paint(color(grid_col, 1.0));
            ctx.set_stroke(kurbo::Stroke::new(0.75));
            ctx.stroke_path(&g);
        }
    }
    // The secondary axis' labels, on the right, without gridlines of their own.
    if let Some(sc) = second.filter(|_| !c.y2_hidden) {
        for v in ticks(sc) {
            let pos = to_axis(true, v);
            label(
                ctx,
                rctx,
                m,
                &c.axis_label(true, v),
                Rect::new(plot.x1 + base * 0.4, pos - base, area.x1, pos + base),
                base,
                Align::Left,
                Anchor::Middle,
                text_col,
                false,
            );
        }
    }
    let ncat = c.categories.len().max(c.series.iter().map(|s| s.values.len()).max().unwrap_or(0)).max(1);
    let band = if horiz { plot.height() / ncat as f64 } else { plot.width() / ncat as f64 };
    // Category labels: wrapped onto two lines when they don't fit their band (long date ranges).
    let wraps = !horiz && c.categories.iter().any(|n| n.chars().count() as f64 * base * 0.5 > band);
    for (i, name) in c.categories.iter().enumerate() {
        let mid = if horiz { plot.y0 + band * (i as f64 + 0.5) } else { plot.x0 + band * (i as f64 + 0.5) };
        if horiz {
            label(
                ctx,
                rctx,
                m,
                name,
                Rect::new(area.x0, mid - base, plot.x0 - base * 0.4, mid + base),
                base,
                Align::Right,
                Anchor::Middle,
                text_col,
                false,
            );
        } else {
            let text = if wraps { split_label(name) } else { name.clone() };
            label(
                ctx,
                rctx,
                m,
                &text,
                Rect::new(mid - band / 2.0, plot.y1 + 3.0, mid + band / 2.0, plot.y1 + base * if wraps { 2.8 } else { 1.6 }),
                base,
                Align::Center,
                Anchor::Top,
                text_col,
                false,
            );
        }
    }
    let zero = to_v(0.0f64.clamp(min, max));
    if let Some(bk) = bar_kind {
        let bars: Vec<(usize, &deckcraft_model::chart::Series)> =
            c.series.iter().enumerate().filter(|(_, s)| c.series_kind(s).is_bar_like()).collect();
        let ns = bars.len().max(1);
        let gap = c.gap_width.clamp(0.0, 5.0);
        let group = band / (1.0 + gap);
        let stacked = bk.stacked();
        let bar = if stacked { group } else { group / ns as f64 };
        for i in 0..ncat {
            let mut pos_acc = 0.0;
            let mut neg_acc = 0.0;
            let total: f64 = if bk.percent() {
                bars.iter().map(|(_, s)| s.values.get(i).copied().flatten().unwrap_or(0.0).abs()).sum::<f64>().max(1e-9)
            } else {
                1.0
            };
            for (bi, (si, s)) in bars.iter().enumerate() {
                let sec = c.on_secondary(s);
                let v = s.values.get(i).copied().flatten().unwrap_or(0.0) / total;
                let start = band * i as f64 + (band - group) / 2.0 + if stacked { 0.0 } else { bar * bi as f64 };
                let (a, b) = if stacked {
                    if v >= 0.0 {
                        let r = (pos_acc, pos_acc + v);
                        pos_acc += v;
                        r
                    } else {
                        let r = (neg_acc + v, neg_acc);
                        neg_acc += v;
                        r
                    }
                } else {
                    (0.0f64.min(v), 0.0f64.max(v))
                };
                let r = if horiz {
                    Rect::new(to_v(a), plot.y0 + start, to_v(b), plot.y0 + start + bar)
                } else {
                    Rect::new(plot.x0 + start, to_axis(sec, b), plot.x0 + start + bar, to_axis(sec, a))
                };
                ctx.set_transform(m);
                ctx.set_paint(color(series_fill(rctx, c, *si, Some(s)), 1.0));
                ctx.fill_rect(&r.abs());
                if c.data_labels {
                    let t = c.data_label(s, s.values.get(i).copied().flatten().unwrap_or(0.0));
                    let lr = if horiz {
                        Rect::new(r.x1 + 2.0, r.y0, r.x1 + 40.0, r.y1)
                    } else {
                        Rect::new(r.x0 - 10.0, r.y0 - base * 1.4, r.x1 + 10.0, r.y0)
                    };
                    label(
                        ctx,
                        rctx,
                        m,
                        &t,
                        lr,
                        base * 0.9,
                        if horiz { Align::Left } else { Align::Center },
                        if horiz { Anchor::Middle } else { Anchor::Bottom },
                        text_col,
                        false,
                    );
                }
            }
        }
    }
    // Lines, areas, scatter, radar (drawn as lines): the series that aren't bars.
    let mut acc = vec![0.0; ncat];
    for (si, s) in c.series.iter().enumerate() {
        let kind = c.series_kind(s);
        if kind.is_bar_like() {
            continue;
        }
        let sec = c.on_secondary(s);
        let stacked = kind.stacked();
        let area_kind = matches!(kind, ChartType::Area | ChartType::StackedArea | ChartType::FilledRadar);
        let col = if area_kind { series_fill(rctx, c, si, Some(s)) } else { series_line(rctx, c, si, s) };
        let mut pts = vec![];
        for i in 0..ncat {
            let Some(v) = s.values.get(i).copied().flatten() else { continue };
            let v = if stacked {
                let base_v = acc.get(i).copied().unwrap_or(0.0);
                if let Some(a) = acc.get_mut(i) {
                    *a += v;
                }
                base_v + v
            } else {
                v
            };
            let x = match kind {
                ChartType::Scatter | ChartType::Bubble => {
                    let xs: Vec<f64> = s.x_values.iter().flatten().copied().collect();
                    let (xl, xh) = (xs.iter().copied().fold(f64::MAX, f64::min), xs.iter().copied().fold(f64::MIN, f64::max));
                    let xv = s.x_values.get(i).copied().flatten().unwrap_or(i as f64);
                    if xh > xl { plot.x0 + (xv - xl) / (xh - xl) * plot.width() } else { plot.x0 + band * (i as f64 + 0.5) }
                }
                _ => plot.x0 + band * (i as f64 + 0.5),
            };
            pts.push(Point::new(x, to_axis(sec, v)));
        }
        if pts.is_empty() {
            continue;
        }
        let mut p = BezPath::new();
        for (k, q) in pts.iter().enumerate() {
            if k == 0 {
                p.move_to(*q);
            } else {
                p.line_to(*q);
            }
        }
        ctx.set_transform(m);
        if area_kind {
            let mut a = p.clone();
            if let (Some(l), Some(f)) = (pts.last(), pts.first()) {
                a.line_to((l.x, zero));
                a.line_to((f.x, zero));
                a.close_path();
            }
            ctx.set_paint(color(col, 0.85));
            ctx.fill_path(&a);
        } else if kind != ChartType::Scatter {
            let lw = s.line.as_ref().and_then(|l| l.width).filter(|w| w.is_finite() && *w > 0.0).unwrap_or(2.25).clamp(0.25, 20.0);
            ctx.set_paint(color(col, 1.0));
            ctx.set_stroke(kurbo::Stroke::new(lw).with_join(kurbo::Join::Round).with_caps(kurbo::Cap::Round));
            ctx.stroke_path(&p);
        }
        if matches!(kind, ChartType::LineMarkers | ChartType::Scatter | ChartType::Bubble) {
            ctx.set_paint(color(col, 1.0));
            for q in &pts {
                ctx.fill_path(&kurbo::Circle::new(*q, 3.5).to_path(0.1));
            }
        }
    }
    // Axis line.
    let mut ax = BezPath::new();
    if horiz {
        ax.move_to((zero, plot.y0));
        ax.line_to((zero, plot.y1));
    } else {
        ax.move_to((plot.x0, zero));
        ax.line_to((plot.x1, zero));
    }
    ctx.set_transform(m);
    ctx.set_paint(color(Rgba::rgb(0xBF, 0xBF, 0xBF), 1.0));
    ctx.set_stroke(kurbo::Stroke::new(0.75));
    ctx.stroke_path(&ax);
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect::new(0.0, 0.0, 400.0, 300.0);

    #[test]
    fn right_legend_is_a_centred_column_and_narrows_the_plot() {
        let (o, plot) = legend_layout("r", (60.0, 100.0), AREA, 10.0, false);
        assert_eq!((o.x, o.y), (340.0, 100.0));
        assert_eq!((plot.x0, plot.x1, plot.y0, plot.y1), (0.0, 335.0, 0.0, 300.0));
    }

    #[test]
    fn left_top_and_corner_positions() {
        let (o, plot) = legend_layout("l", (60.0, 100.0), AREA, 10.0, false);
        assert_eq!((o.x, o.y, plot.x0, plot.x1), (0.0, 100.0, 65.0, 400.0));
        let (o, plot) = legend_layout("t", (100.0, 14.0), AREA, 10.0, false);
        assert_eq!((o.x, o.y, plot.y0, plot.y1), (150.0, 0.0, 19.0, 300.0));
        let (o, plot) = legend_layout("tr", (60.0, 100.0), AREA, 10.0, false);
        assert_eq!((o.x, o.y, plot.x1), (340.0, 0.0, 335.0));
    }

    #[test]
    fn bottom_legend_keeps_the_old_row_layout() {
        let (o, plot) = legend_layout("b", (100.0, 14.0), AREA, 10.0, false);
        assert_eq!((o.x, o.y, plot.y1, plot.x1), (150.0, 286.0, 281.0, 400.0));
    }

    #[test]
    fn overlay_does_not_shrink_the_plot() {
        let (o, plot) = legend_layout("r", (60.0, 100.0), AREA, 10.0, true);
        assert_eq!(plot, AREA);
        assert_eq!(o.x, 340.0);
    }
}
