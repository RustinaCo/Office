//! WordArt text warps (DrawingML `a:bodyPr/a:prstTxWarp`).
//!
//! A warp preset is a list of bands laid over the shape's text rectangle (insets are ignored).
//! Text lines are shared out over the bands in order (the first ⌈n/k⌉ lines to the first band
//! and so on). A band either
//! - stretches its lines between a top and a bottom guide ([`Band::Fill`]): the ink box of the
//!   band's lines fills the band vertically (a band whose ink is shorter than the tallest band's
//!   keeps the same scale, centred), and horizontally the text is scaled so that the
//!   widest band is filled along the mean length of its guides (one scale for the whole text), or
//! - sets its lines at their natural size along one guide ([`Band::Follow`], arches and circles):
//!   glyphs keep their size, are placed by arc length (aligned as in the paragraph, the whole
//!   guide being the line's width: insets don't count) and stand on
//!   the guide's left normal; lines further from the guide stack outwards. Lines that are longer
//!   than the guide are shrunk to fit.
//!
//! Positions along a guide are by arc length; a fill band maps the same fraction of its top and
//! bottom guide's length and interpolates between them. The guide curves were derived from
//! observation: synthetic decks of `I` bars were drawn with PowerPoint and every curve fitted to
//! the measured bar ends (most fit within a point on a 420 × 200 pt box).
use kurbo::{Arc, BezPath, PathEl, Point, Rect, Vec2};

/// Flattening tolerance for guides, in points.
const TOL: f64 = 0.02;

/// A guide curve flattened to a polyline with cumulative arc lengths.
#[derive(Clone, Debug, Default)]
pub struct Guide {
    pts: Vec<Point>,
    cum: Vec<f64>,
    closed: bool,
}

impl Guide {
    fn from_points(raw: impl IntoIterator<Item = Point>, closed: bool) -> Guide {
        let mut pts: Vec<Point> = Vec::new();
        let mut cum: Vec<f64> = Vec::new();
        for p in raw {
            if !(p.x.is_finite() && p.y.is_finite()) {
                continue;
            }
            match pts.last() {
                Some(q) => {
                    let d = (p - *q).hypot();
                    if d > 1e-9 {
                        cum.push(cum.last().copied().unwrap_or(0.0) + d);
                        pts.push(p);
                    }
                }
                None => {
                    pts.push(p);
                    cum.push(0.0);
                }
            }
            if pts.len() > 100_000 {
                break;
            }
        }
        Guide { pts, cum, closed }
    }

    /// A guide through `path` (any curves), flattened.
    pub fn from_path(path: &BezPath, closed: bool) -> Guide {
        let mut pts = Vec::new();
        kurbo::flatten(path.iter(), TOL, |el| match el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => pts.push(p),
            _ => {}
        });
        Guide::from_points(pts, closed)
    }

    /// Total arc length.
    pub fn length(&self) -> f64 {
        self.cum.last().copied().unwrap_or(0.0)
    }

    /// Whether the guide is a closed loop (positions past its end wrap around).
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Point and unit tangent at arc length `s`. Open guides continue along their end tangents
    /// past either end; closed guides wrap.
    pub fn at(&self, s: f64) -> (Point, Vec2) {
        let len = self.length();
        let (Some(&first), Some(&last)) = (self.pts.first(), self.pts.last()) else {
            return (Point::ZERO, Vec2::new(1.0, 0.0));
        };
        if self.pts.len() < 2 || len <= 1e-9 {
            return (first, Vec2::new(1.0, 0.0));
        }
        let s = if s.is_finite() { s } else { 0.0 };
        let s = if self.closed { s.rem_euclid(len) } else { s };
        let seg = |i: usize| -> (Point, Point, f64) {
            let a = self.pts.get(i).copied().unwrap_or(first);
            let b = self.pts.get(i + 1).copied().unwrap_or(last);
            (a, b, self.cum.get(i).copied().unwrap_or(0.0))
        };
        let n = self.pts.len();
        let i = if s <= 0.0 {
            0
        } else if s >= len {
            n - 2
        } else {
            self.cum.partition_point(|c| *c <= s).saturating_sub(1).min(n - 2)
        };
        let (a, b, c0) = seg(i);
        let d = b - a;
        let l = d.hypot();
        let t = if l > 1e-12 { d / l } else { Vec2::new(1.0, 0.0) };
        (a + t * (s - c0), t)
    }

    /// The parallel curve `d` points away on the left of the direction of travel (above a
    /// guide running left to right).
    pub fn offset(&self, d: f64) -> Guide {
        let n = self.pts.len();
        if n < 2 || !d.is_finite() || d == 0.0 {
            return self.clone();
        }
        let unit = |i: usize| -> Vec2 {
            let (a, b) = (self.pts.get(i).copied().unwrap_or(Point::ZERO), self.pts.get(i + 1).copied().unwrap_or(Point::ZERO));
            let v = b - a;
            let l = v.hypot();
            if l > 1e-12 { v / l } else { Vec2::ZERO }
        };
        let pts = (0..n).map(|i| {
            let before = if i > 0 {
                unit(i - 1)
            } else if self.closed {
                unit(n - 2)
            } else {
                Vec2::ZERO
            };
            let after = if i + 1 < n {
                unit(i)
            } else if self.closed {
                unit(0)
            } else {
                Vec2::ZERO
            };
            let t = before + after;
            let l = t.hypot();
            let t = if l > 1e-12 { t / l } else { Vec2::new(1.0, 0.0) };
            self.pts.get(i).copied().unwrap_or(Point::ZERO) + Vec2::new(t.y, -t.x) * d
        });
        Guide::from_points(pts.collect::<Vec<_>>(), self.closed)
    }

    /// The point at fraction `t` of the length.
    pub fn at_frac(&self, t: f64) -> Point {
        self.at(t * self.length()).0
    }
}

/// Which line of a follow band sits on its guide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineAnchor {
    /// The last line; earlier lines stack outwards (arch up, circle).
    Last,
    /// The first line; later lines stack below it (arch down).
    First,
    /// The middle of the band's lines.
    Middle,
}

#[derive(Clone, Debug)]
pub enum Band {
    /// Text stretched between two guides.
    Fill { top: Guide, bottom: Guide },
    /// Text at its natural size along a guide.
    Follow { path: Guide, anchor: LineAnchor },
}

impl Band {
    /// The length text is measured against: the mean of a fill band's guides.
    pub fn length(&self) -> f64 {
        match self {
            Band::Fill { top, bottom } => (top.length() + bottom.length()) / 2.0,
            Band::Follow { path, .. } => path.length(),
        }
    }
}

/// A resolved warp: its bands in shape-local points.
#[derive(Clone, Debug)]
pub struct Warp {
    pub bands: Vec<Band>,
}

/// Presets whose text follows a guide at its natural size (no wrapping).
pub fn is_follow(name: &str) -> bool {
    matches!(name, "textArchUp" | "textArchDown" | "textCircle" | "textButton")
}

/// Default adjust values of a warp preset (`adj`, or `adj1`, `adj2`); empty for unknown presets.
pub fn adjust_defaults(name: &str) -> &'static [f64] {
    match name {
        "textPlain" => &[50_000.0],
        "textStop" => &[25_000.0],
        "textTriangle" | "textTriangleInverted" => &[50_000.0],
        "textChevron" => &[25_000.0],
        "textChevronInverted" => &[75_000.0],
        "textRingInside" | "textRingOutside" => &[60_000.0],
        "textArchUp" | "textCircle" | "textButton" => &[10_800_000.0],
        "textArchDown" => &[0.0],
        "textArchUpPour" | "textCirclePour" | "textButtonPour" => &[10_800_000.0, 50_000.0],
        "textArchDownPour" => &[0.0, 25_000.0],
        "textCurveUp" | "textCurveDown" => &[45_977.0],
        "textCanUp" => &[85_000.0],
        "textCanDown" => &[14_286.0],
        "textWave1" | "textWave2" => &[12_500.0, 0.0],
        "textDoubleWave1" | "textWave4" => &[6_250.0, 0.0],
        "textInflate" | "textDeflate" => &[18_750.0],
        "textInflateBottom" => &[60_000.0],
        "textDeflateBottom" | "textDeflateTop" => &[50_000.0],
        "textInflateTop" => &[40_000.0],
        "textDeflateInflate" => &[35_000.0],
        "textDeflateInflateDeflate" => &[25_000.0],
        "textFadeRight" | "textFadeLeft" | "textFadeUp" | "textFadeDown" => &[33_333.0],
        "textSlantUp" => &[55_556.0],
        "textSlantDown" => &[44_444.0],
        "textCascadeUp" | "textCascadeDown" => &[44_444.0],
        _ => &[],
    }
}

/// Number of adjust values a warp preset takes.
pub fn adjust_count(name: &str) -> usize {
    adjust_defaults(name).len()
}

/// Whether `name` is a warp preset DeckCraft draws.
pub fn is_supported(name: &str) -> bool {
    !adjust_defaults(name).is_empty()
}

/// Segments of a guide in unit coordinates (x, y in 0..1 over the text rectangle).
#[derive(Clone, Copy, Debug)]
enum Seg {
    M(f64, f64),
    L(f64, f64),
    Q(f64, f64, f64, f64),
    C(f64, f64, f64, f64, f64, f64),
    /// Elliptical arc: centre, radii, start and sweep in degrees (y down: positive = clockwise).
    A(f64, f64, f64, f64, f64, f64),
}

enum Spec {
    Fill(Vec<Seg>, Vec<Seg>),
    Follow(Vec<Seg>, LineAnchor),
}

struct Adj<'a> {
    raw: &'a [f64],
    defaults: &'static [f64],
}

impl Adj<'_> {
    fn raw(&self, i: usize) -> f64 {
        self.raw.get(i).copied().filter(|v| v.is_finite()).or_else(|| self.defaults.get(i).copied()).unwrap_or(0.0)
    }
    /// Adjust `i` as a fraction (1/100 000), pinned to `lo..=hi` (fractions).
    fn frac(&self, i: usize, lo: f64, hi: f64) -> f64 {
        (self.raw(i) / 100_000.0).clamp(lo, hi)
    }
    /// Adjust `i` as an angle in degrees (1/60 000), in 0..360.
    fn angle(&self, i: usize) -> f64 {
        (self.raw(i) / 60_000.0).rem_euclid(360.0)
    }
}

fn line(pts: &[(f64, f64)]) -> Vec<Seg> {
    pts.iter().enumerate().map(|(i, &(x, y))| if i == 0 { Seg::M(x, y) } else { Seg::L(x, y) }).collect()
}

/// Quadratic from (0, y0) through control (0.5, c) to (1, y0).
fn quad(y0: f64, c: f64) -> Vec<Seg> {
    vec![Seg::M(0.0, y0), Seg::Q(0.5, c, 1.0, y0)]
}

/// A sweep in (0, 360]: zero means a full turn.
fn turn(deg: f64) -> f64 {
    let d = deg.rem_euclid(360.0);
    if d < 1e-9 { 360.0 } else { d }
}

/// Arc of the ellipse inscribed in the rectangle, scaled by `s` about its centre.
fn ellipse(s: f64, start: f64, sweep: f64) -> Vec<Seg> {
    vec![Seg::A(0.5, 0.5, 0.5 * s, 0.5 * s, start, sweep)]
}

/// The arch-up arc for adjust angle `a`: symmetric about the top (270°), clockwise.
fn arch_up(a: f64) -> (f64, f64) {
    (a, turn(2.0 * (270.0 - a)))
}

/// The arch-down arc for adjust angle `a`: symmetric about the bottom (90°), counter-clockwise.
fn arch_down(a: f64) -> (f64, f64) {
    (180.0 - a, -turn(180.0 - 2.0 * a))
}

/// The circle arc for adjust angle `a`: from `a` clockwise, symmetric about 0°.
fn circle(a: f64) -> (f64, f64) {
    (a, turn(2.0 * (360.0 - a)))
}

/// One cubic wave period from x0 to x1 at height y: up first when `k` > 0 (y decreases).
fn wave(out: &mut Vec<Seg>, x0: f64, x1: f64, y: f64, k: f64) {
    let l = x1 - x0;
    out.push(Seg::C(x0 + l / 3.0, y - k, x0 + 2.0 * l / 3.0, y + k, x1, y));
}

/// Wave guides: `periods` cubic waves over the width, shifted apart by `shift` (fraction).
fn waves(a: f64, shift: f64, periods: usize, up_first: bool) -> Spec {
    let d = 2.0 * shift;
    let (top_x, bot_x) = if d >= 0.0 { ((0.0, 1.0 - d), (d, 1.0)) } else { ((-d, 1.0), (0.0, 1.0 + d)) };
    let k = if up_first { a * 10.0 / 3.0 } else { -a * 10.0 / 3.0 };
    let guide = |(x0, x1): (f64, f64), y: f64| {
        let mut v = vec![Seg::M(x0, y)];
        let n = periods.max(1) as f64;
        for i in 0..periods.max(1) {
            let f = i as f64;
            wave(&mut v, x0 + (x1 - x0) * f / n, x0 + (x1 - x0) * (f + 1.0) / n, y, k);
        }
        v
    };
    Spec::Fill(guide(top_x, a), guide(bot_x, 1.0 - a))
}

fn spec(name: &str, a: &Adj) -> Option<Vec<Spec>> {
    use Seg::*;
    let fill = |t: Vec<Seg>, b: Vec<Seg>| vec![Spec::Fill(t, b)];
    let top = || line(&[(0.0, 0.0), (1.0, 0.0)]);
    let bottom = || line(&[(0.0, 1.0), (1.0, 1.0)]);
    Some(match name {
        "textPlain" => {
            let d = 2.0 * (a.frac(0, 0.3, 0.7) - 0.5);
            if d < 0.0 {
                fill(line(&[(-d, 0.0), (1.0, 0.0)]), line(&[(0.0, 1.0), (1.0 + d, 1.0)]))
            } else {
                fill(line(&[(0.0, 0.0), (1.0 - d, 0.0)]), line(&[(d, 1.0), (1.0, 1.0)]))
            }
        }
        "textStop" => {
            let y = a.frac(0, 0.14286, 0.5);
            let t = 1.0 / 3.0;
            fill(line(&[(0.0, y), (t, 0.0), (1.0 - t, 0.0), (1.0, y)]), line(&[(0.0, 1.0 - y), (t, 1.0), (1.0 - t, 1.0), (1.0, 1.0 - y)]))
        }
        "textTriangle" => {
            let y = a.frac(0, 0.0, 1.0);
            fill(line(&[(0.0, y), (0.5, 0.0), (1.0, y)]), bottom())
        }
        "textTriangleInverted" => {
            let y = a.frac(0, 0.0, 1.0);
            fill(top(), line(&[(0.0, y), (0.5, 1.0), (1.0, y)]))
        }
        "textChevron" => {
            let y = a.frac(0, 0.0, 0.5);
            fill(line(&[(0.0, y), (0.5, 0.0), (1.0, y)]), line(&[(0.0, 1.0), (0.5, 1.0 - y), (1.0, 1.0)]))
        }
        "textChevronInverted" => {
            let y = a.frac(0, 0.5, 1.0);
            fill(line(&[(0.0, 0.0), (0.5, 1.0 - y), (1.0, 0.0)]), line(&[(0.0, y), (0.5, 1.0), (1.0, y)]))
        }
        "textFadeRight" => {
            let y = a.frac(0, 0.0, 0.49999);
            fill(line(&[(0.0, 0.0), (1.0, y)]), line(&[(0.0, 1.0), (1.0, 1.0 - y)]))
        }
        "textFadeLeft" => {
            let y = a.frac(0, 0.0, 0.49999);
            fill(line(&[(0.0, y), (1.0, 0.0)]), line(&[(0.0, 1.0 - y), (1.0, 1.0)]))
        }
        "textFadeUp" => {
            let x = a.frac(0, 0.0, 0.49999);
            fill(line(&[(x, 0.0), (1.0 - x, 0.0)]), bottom())
        }
        "textFadeDown" => {
            let x = a.frac(0, 0.0, 0.49999);
            fill(top(), line(&[(x, 1.0), (1.0 - x, 1.0)]))
        }
        "textSlantUp" => {
            let y = a.frac(0, 0.44444, 1.0);
            fill(line(&[(0.0, y), (1.0, 0.0)]), line(&[(0.0, 1.0), (1.0, 1.0 - y)]))
        }
        "textSlantDown" => {
            let y = a.frac(0, 0.0, 0.55556);
            fill(line(&[(0.0, 0.0), (1.0, 1.0 - y)]), line(&[(0.0, y), (1.0, 1.0)]))
        }
        "textCascadeUp" => {
            let y = a.frac(0, 0.0, 1.0);
            fill(line(&[(0.0, (1.0 - y) / 4.0), (1.0, 0.0)]), line(&[(0.0, 1.0), (1.0, y)]))
        }
        "textCascadeDown" => {
            let y = a.frac(0, 0.0, 1.0);
            fill(line(&[(0.0, 0.0), (1.0, (1.0 - y) / 4.0)]), line(&[(0.0, y), (1.0, 1.0)]))
        }
        "textInflate" => {
            let y = a.frac(0, 0.0, 0.2);
            fill(quad(y, -y), quad(1.0 - y, 1.0 + y))
        }
        "textDeflate" => {
            let y = a.frac(0, 0.0, 0.375);
            fill(quad(0.0, 2.0 * y), quad(1.0, 1.0 - 2.0 * y))
        }
        "textInflateBottom" => {
            let y = a.frac(0, 0.6, 1.0);
            fill(top(), quad(y, 1.0))
        }
        "textDeflateBottom" => {
            let y = a.frac(0, 0.0625, 1.0);
            fill(top(), quad(1.0, 2.0 * y - 1.0))
        }
        "textInflateTop" => {
            let y = a.frac(0, 0.0, 0.5);
            fill(quad(y, 0.0), bottom())
        }
        "textDeflateTop" => {
            let y = a.frac(0, 0.0, 0.9375);
            fill(quad(0.0, 2.0 * y), bottom())
        }
        "textDeflateInflate" => {
            // The bands are split by a curve from mid-height up to `y`, with a gap of a tenth of
            // a band's height.
            let y = a.frac(0, 0.05, 0.95);
            let g = 0.05;
            vec![Spec::Fill(top(), quad(0.5 - g, 2.0 * y - 0.5 - g)), Spec::Fill(quad(0.5 + g, 2.0 * y - 0.5 + g), bottom())]
        }
        "textDeflateInflateDeflate" => {
            let y = a.frac(0, 0.01, 1.0 / 3.0);
            let g = 1.0 / 30.0;
            let (t1, t2) = (1.0 / 3.0, 2.0 / 3.0);
            let (c1, c2) = (2.0 * y - t1, 2.0 * (1.0 - y) - t2);
            vec![
                Spec::Fill(top(), quad(t1 - g, c1 - g)),
                Spec::Fill(quad(t1 + g, c1 + g), quad(t2 - g, c2 - g)),
                Spec::Fill(quad(t2 + g, c2 + g), bottom()),
            ]
        }
        "textCurveUp" | "textCurveDown" => {
            let y = a.frac(0, 0.0, 0.56818);
            let b = vec![M(0.0, 1.0 - 3.0 * y / 8.0), C(1.0 / 3.0, 1.0 + y / 8.0, 2.0 / 3.0, 1.0 + y / 8.0, 1.0, 1.0 - 3.0 * y / 8.0)];
            let t = if name == "textCurveUp" {
                vec![M(0.0, y), C(1.0 / 3.0, 1.25 * y, 2.0 / 3.0, 0.75 * y, 1.0, 0.0)]
            } else {
                vec![M(0.0, 0.0), C(1.0 / 3.0, 0.75 * y, 2.0 / 3.0, 1.25 * y, 1.0, y)]
            };
            fill(t, b)
        }
        "textCanUp" => {
            let r = 1.0 - a.frac(0, 0.5, 1.0);
            fill(vec![A(0.5, r, 0.5, r, 180.0, 180.0)], vec![A(0.5, 1.0, 0.5, r, 180.0, 180.0)])
        }
        "textCanDown" => {
            let r = a.frac(0, 0.0, 0.5);
            fill(vec![A(0.5, 0.0, 0.5, r, 180.0, -180.0)], vec![A(0.5, 1.0 - r, 0.5, r, 180.0, -180.0)])
        }
        "textRingInside" | "textRingOutside" => {
            let h = a.frac(0, 0.5, 0.99);
            let sweep = if name == "textRingInside" { 360.0 } else { -360.0 };
            fill(vec![A(0.5, h / 2.0, 0.5, h / 2.0, 180.0, sweep)], vec![A(0.5, 1.0 - h / 2.0, 0.5, h / 2.0, 180.0, sweep)])
        }
        "textWave1" => vec![waves(a.frac(0, 0.0, 0.2), a.frac(1, -0.1, 0.1), 1, true)],
        "textWave2" => vec![waves(a.frac(0, 0.0, 0.2), a.frac(1, -0.1, 0.1), 1, false)],
        "textDoubleWave1" => vec![waves(a.frac(0, 0.0, 0.125), a.frac(1, -0.1, 0.1), 2, true)],
        "textWave4" => vec![waves(a.frac(0, 0.0, 0.125), a.frac(1, -0.1, 0.1), 2, false)],
        "textArchUp" => {
            let (s, w) = arch_up(a.angle(0));
            vec![Spec::Follow(ellipse(1.0, s, w), LineAnchor::Last)]
        }
        "textArchDown" => {
            let (s, w) = arch_down(a.angle(0));
            vec![Spec::Follow(ellipse(1.0, s, w), LineAnchor::First)]
        }
        "textCircle" => {
            let (s, w) = circle(a.angle(0));
            vec![Spec::Follow(ellipse(1.0, s, w), LineAnchor::Last)]
        }
        "textButton" => {
            let t = a.angle(0);
            let (s, w) = arch_up(t);
            let half = 0.5 * t.to_radians().cos().abs();
            vec![
                Spec::Follow(ellipse(1.0, s, w), LineAnchor::Last),
                Spec::Follow(line(&[(0.5 - half, 0.5), (0.5 + half, 0.5)]), LineAnchor::Middle),
                Spec::Follow(ellipse(1.0, 360.0 - s, -w), LineAnchor::First),
            ]
        }
        "textArchUpPour" => {
            let (s, w) = arch_up(a.angle(0));
            let r = a.frac(1, 0.0, 0.99);
            fill(ellipse(1.0, s, w), ellipse(r, s, w))
        }
        "textArchDownPour" => {
            let (s, w) = arch_down(a.angle(0));
            let r = a.frac(1, 0.0, 0.99);
            fill(ellipse(r, s, w), ellipse(1.0, s, w))
        }
        "textCirclePour" => {
            let (s, w) = circle(a.angle(0));
            let r = a.frac(1, 0.0, 0.99);
            fill(ellipse(1.0, s, w), ellipse(r, s, w))
        }
        "textButtonPour" => {
            let (s, w) = arch_up(a.angle(0));
            let r = a.frac(1, 0.0, 0.99);
            // The middle band runs between chords of the inner ellipse halfway to its top and
            // bottom.
            let (dy, half) = (0.25 * r, 0.25 * r * 3f64.sqrt());
            vec![
                Spec::Fill(ellipse(1.0, s, w), ellipse(r, s, w)),
                Spec::Fill(line(&[(0.5 - half, 0.5 - dy), (0.5 + half, 0.5 - dy)]), line(&[(0.5 - half, 0.5 + dy), (0.5 + half, 0.5 + dy)])),
                Spec::Fill(ellipse(r, 360.0 - s, -w), ellipse(1.0, 360.0 - s, -w)),
            ]
        }
        _ => return None,
    })
}

/// Build a guide from unit-space segments mapped onto `r`.
fn guide(segs: &[Seg], r: Rect) -> Guide {
    let mut p = BezPath::new();
    let mut started = false;
    let mut closed = false;
    for s in segs {
        match *s {
            Seg::M(x, y) => {
                p.move_to((x, y));
                started = true;
            }
            Seg::L(x, y) => {
                if started {
                    p.line_to((x, y))
                } else {
                    p.move_to((x, y))
                }
                started = true;
            }
            Seg::Q(cx, cy, x, y) => {
                if started {
                    p.quad_to((cx, cy), (x, y));
                }
            }
            Seg::C(x1, y1, x2, y2, x, y) => {
                if started {
                    p.curve_to((x1, y1), (x2, y2), (x, y));
                }
            }
            Seg::A(cx, cy, rx, ry, start, sweep) => {
                let arc = Arc::new((cx, cy), (rx.max(0.0), ry.max(0.0)), start.to_radians(), sweep.to_radians(), 0.0);
                let s0 = Point::new(cx + rx * start.to_radians().cos(), cy + ry * start.to_radians().sin());
                if started {
                    p.line_to(s0);
                } else {
                    p.move_to(s0);
                }
                started = true;
                for el in arc.append_iter(1e-5) {
                    p.push(el);
                }
                closed = sweep.abs() >= 359.999;
            }
        }
    }
    let m = kurbo::Affine::new([r.width(), 0.0, 0.0, r.height(), r.x0, r.y0]);
    p.apply_affine(m);
    Guide::from_path(&p, closed)
}

/// The warp `name` with adjust values `adj` (raw guide values; missing ones take the defaults)
/// over text rectangle `rect`. `None` for unknown presets, `textNoShape` or a non-finite rect.
pub fn warp(name: &str, adj: &[f64], rect: Rect) -> Option<Warp> {
    if ![rect.x0, rect.y0, rect.x1, rect.y1].iter().all(|v| v.is_finite()) {
        return None;
    }
    let r = Rect::new(rect.x0.min(rect.x1), rect.y0.min(rect.y1), rect.x0.max(rect.x1), rect.y0.max(rect.y1));
    let a = Adj { raw: adj, defaults: adjust_defaults(name) };
    let specs = spec(name, &a)?;
    let bands = specs
        .into_iter()
        .map(|s| match s {
            Spec::Fill(t, b) => Band::Fill { top: guide(&t, r), bottom: guide(&b, r) },
            Spec::Follow(p, anchor) => Band::Follow { path: guide(&p, r), anchor },
        })
        .collect();
    Some(Warp { bands })
}

/// The band line `line` of `lines` goes to when there are `bands` bands: the first ⌈n/k⌉ lines
/// share the first band, and so on.
pub fn band_of_line(line: usize, lines: usize, bands: usize) -> usize {
    if bands <= 1 || lines == 0 {
        return 0;
    }
    // Band b holds lines ceil(b·n/k) .. ceil((b+1)·n/k).
    (0..bands).rev().find(|&b| (b * lines).div_ceil(bands) <= line).unwrap_or(0)
}

/// One laid-out line as the warp sees it (layout coordinates, points).
#[derive(Clone, Copy, Debug)]
pub struct WarpLine {
    /// Ink box of the line's glyphs; `None` for a line without ink.
    pub ink: Option<Rect>,
    /// Advance extent of the line (left, right).
    pub extent: (f64, f64),
    /// Reference height for follow bands: the y that sits on the guide.
    pub ref_y: f64,
}

#[derive(Clone, Copy, Debug)]
enum LineMap {
    Fill {
        band: usize,
        cx: f64,
        k: f64,
        len: f64,
        y0: f64,
        y1: f64,
    },
    /// `guide`: the line's own parallel guide in [`Mapper::offsets`], if it is off the band's.
    Follow {
        band: usize,
        guide: Option<usize>,
        e0: f64,
        s0: f64,
        k: f64,
        ref_y: f64,
    },
}

/// Maps points of laid-out lines onto a warp.
#[derive(Clone, Debug)]
pub struct Mapper<'a> {
    warp: &'a Warp,
    lines: Vec<LineMap>,
    offsets: Vec<Guide>,
    zoom: f64,
}

impl Warp {
    /// Plan the mapping of `lines`, laid out in the inner text box `inner` (the line's place in
    /// it gives its alignment along follow guides; the insets themselves don't shorten a guide).
    pub fn mapper(&self, lines: &[WarpLine], inner: Rect) -> Mapper<'_> {
        let nb = self.bands.len().max(1);
        let n = lines.len();
        let band_of: Vec<usize> = (0..n).map(|i| band_of_line(i, n, nb).min(nb - 1)).collect();
        // Ink of the whole text, for the shared horizontal centre and scale of fill bands.
        let ink_all = lines.iter().filter_map(|l| l.ink).reduce(|a, b| a.union(b));
        let cx = ink_all.map(|r| r.center().x).unwrap_or(0.0);
        let mut k_fill = f64::INFINITY;
        let mut zoom: f64 = 1.0;
        let mut band_y: Vec<Option<(f64, f64)>> = vec![None; nb];
        for (i, l) in lines.iter().enumerate() {
            let Some(r) = l.ink else { continue };
            let b = band_of.get(i).copied().unwrap_or(0);
            if let Some(slot) = band_y.get_mut(b) {
                *slot = Some(match *slot {
                    Some((y0, y1)) => (y0.min(r.y0), y1.max(r.y1)),
                    None => (r.y0, r.y1),
                });
            }
        }
        // Every fill band maps a box as tall as the tallest band's ink, centred on its own ink, so
        // a band with fewer lines keeps them at the same size, centred between its guides.
        let tallest = band_y.iter().flatten().map(|(y0, y1)| y1 - y0).fold(0.0, f64::max);
        let band_y: Vec<Option<(f64, f64)>> =
            band_y.iter().map(|r| r.map(|(y0, y1)| ((y0 + y1 - tallest) / 2.0, (y0 + y1 + tallest) / 2.0))).collect();
        for (b, band) in self.bands.iter().enumerate() {
            if let Band::Fill { top, bottom } = band {
                let half = lines
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| band_of.get(*i) == Some(&b))
                    .filter_map(|(_, l)| l.ink)
                    .map(|r| (r.x0 - cx).abs().max((r.x1 - cx).abs()))
                    .fold(0.0, f64::max);
                if half > 1e-9 {
                    k_fill = k_fill.min(band.length() / (2.0 * half));
                }
                if let Some(Some((y0, y1))) = band_y.get(b) {
                    let thick = [0.0, 0.25, 0.5, 0.75, 1.0].iter().map(|&u| (top.at_frac(u) - bottom.at_frac(u)).hypot()).fold(0.0, f64::max);
                    if y1 - y0 > 1e-9 {
                        zoom = zoom.max(thick / (y1 - y0));
                    }
                }
            }
        }
        if !k_fill.is_finite() {
            k_fill = 1.0;
        }
        zoom = zoom.max(k_fill);
        let mut maps = Vec::with_capacity(n);
        let mut offsets = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            let b = band_of.get(i).copied().unwrap_or(0);
            let in_band = |j: &usize| band_of.get(*j) == Some(&b);
            match self.bands.get(b) {
                Some(Band::Fill { .. }) | None => {
                    let (y0, y1) = band_y.get(b).copied().flatten().unwrap_or((0.0, 1.0));
                    let len = self.bands.get(b).map(Band::length).unwrap_or(0.0);
                    maps.push(LineMap::Fill { band: b, cx, k: k_fill, len, y0, y1 });
                }
                Some(Band::Follow { path, anchor }) => {
                    let len = path.length();
                    let widest = (0..n).filter(in_band).filter_map(|j| lines.get(j)).map(|l| l.extent.1 - l.extent.0).fold(0.0, f64::max);
                    let k = if widest > len && widest > 1e-9 { len / widest } else { 1.0 };
                    let members: Vec<&WarpLine> = (0..n).filter(in_band).filter_map(|j| lines.get(j)).collect();
                    let ref_y = match anchor {
                        LineAnchor::Last => members.last().map(|l| l.ref_y),
                        LineAnchor::First => members.first().map(|l| l.ref_y),
                        LineAnchor::Middle => match (members.first(), members.last()) {
                            (Some(a), Some(z)) => Some((a.ref_y + z.ref_y) / 2.0),
                            _ => None,
                        },
                    }
                    .unwrap_or(l.ref_y);
                    // Lines off the guide run on a parallel curve at their natural length.
                    let d = (ref_y - l.ref_y) * k;
                    let (guide, len) = if d.abs() > 1e-6 && d.is_finite() && offsets.len() < 4096 {
                        let g = path.offset(d);
                        let len = g.length();
                        offsets.push(g);
                        (Some(offsets.len() - 1), len)
                    } else {
                        (None, len)
                    };
                    let (e0, e1) = l.extent;
                    let w = (e1 - e0).max(0.0) * k;
                    let (gl, gr) = ((e0 - inner.x0).max(0.0), (inner.x1 - e1).max(0.0));
                    let f = if gl + gr > 1e-9 { gl / (gl + gr) } else { 0.5 };
                    let s0 = f * (len - w);
                    maps.push(LineMap::Follow { band: b, guide, e0, s0, k, ref_y: l.ref_y });
                }
            }
        }
        Mapper { warp: self, lines: maps, offsets, zoom: if zoom.is_finite() { zoom.clamp(1.0, 1.0e4) } else { 1.0 } }
    }
}

impl Mapper<'_> {
    /// How much the warp magnifies text at most (for flattening tolerances), ≥ 1.
    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    /// Map point `p` of line `line` (layout coordinates) onto the warp.
    pub fn map(&self, line: usize, p: Point) -> Point {
        let out = match self.lines.get(line) {
            Some(&LineMap::Fill { band, cx, k, len, y0, y1 }) => {
                let Some(Band::Fill { top, bottom }) = self.warp.bands.get(band) else { return p };
                let u = if len > 1e-9 { 0.5 + (p.x - cx) * k / len } else { 0.5 };
                let v = if y1 - y0 > 1e-9 { (p.y - y0) / (y1 - y0) } else { 0.5 };
                let (t, b) = (top.at_frac(u), bottom.at_frac(u));
                t.lerp(b, v)
            }
            Some(&LineMap::Follow { band, guide, e0, s0, k, ref_y }) => {
                let path = match (guide.and_then(|g| self.offsets.get(g)), self.warp.bands.get(band)) {
                    (Some(g), _) => g,
                    (None, Some(Band::Follow { path, .. })) => path,
                    _ => return p,
                };
                let s = s0 + (p.x - e0) * k;
                let d = (ref_y - p.y) * k;
                let (q, t) = path.at(s);
                q + Vec2::new(t.y, -t.x) * d
            }
            None => p,
        };
        if out.x.is_finite() && out.y.is_finite() { out } else { p }
    }
}

#[cfg(test)]
mod tests;
