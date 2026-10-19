//! Lines with right-to-left text: the line is split into bidi runs in display order
//! ([`crate::bidi::visual_runs`]), each run into pieces by the font that draws them (the text's
//! font, a fallback font for characters it lacks, or the stroke font), and each font piece is
//! shaped with `harfrust` (OpenType GSUB/GPOS: Arabic contextual forms, lam-alef and other
//! ligatures, mark positioning, mirroring) before its glyphs are placed. Shaping happens per
//! piece in the font that draws it, so Arabic in a stroke-font style is joined by the fallback
//! font. Fonts without OpenType Arabic features get Unicode presentation forms instead.

use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};

use cadcraft_geom::Vec2;

use crate::ttf::{self, Face};
use crate::{Shaped, bidi, stroke};

/// Longest line (characters) laid out this way; longer lines keep one glyph per character.
const MAX_CHARS: usize = 10_000;

/// What draws a piece of a run.
#[derive(Clone)]
enum Src {
    /// The built-in stroke font.
    Stroke,
    /// No font has the character: a missing-glyph box.
    Missing,
    /// An invisible formatting character outside any shaped piece.
    Hidden,
    /// A TrueType/OpenType face (the text's font or a fallback).
    Face(Face),
}

impl Src {
    fn same(&self, o: &Src) -> bool {
        match (self, o) {
            (Src::Stroke, Src::Stroke) | (Src::Missing, Src::Missing) | (Src::Hidden, Src::Hidden) => true,
            (Src::Face(a), Src::Face(b)) => a.same(b),
            _ => false,
        }
    }
    fn face(&self) -> Option<&Face> {
        match self {
            Src::Face(f) => Some(f),
            _ => None,
        }
    }
}

/// Shape one line containing right-to-left text in `primary` (None: the stroke font), with the
/// same conventions as the left-to-right paths: baseline at y = 0, x from 0, text height = cap
/// height, `%%` codes decoded. `None` when the line is too long (the caller then lays it out one
/// glyph per character) or shaping failed.
pub(crate) fn shape(primary: Option<&Face>, s: &str, height: f64, width_factor: f64, oblique: f64) -> Option<Shaped> {
    let chars = crate::decode_controls(s);
    if chars.is_empty() || chars.len() > MAX_CHARS {
        return None;
    }
    // A last-resort guard around the bidi and shaping libraries: a panic in them must not take
    // the drawing down with it (the line then falls back to one glyph per character).
    catch_unwind(AssertUnwindSafe(|| shape_chars(primary, &chars, height, width_factor, oblique))).ok()
}

fn shape_chars(primary: Option<&Face>, chars: &[(char, bool, bool)], height: f64, width_factor: f64, oblique: f64) -> Shaped {
    let h = if height.is_finite() && height > 0.0 { height } else { 1.0 };
    let wf = if width_factor.is_finite() && width_factor.abs() > 1e-6 { width_factor } else { 1.0 };
    let shear = oblique.tan().clamp(-10.0, 10.0);
    let sc = h / stroke::CAP;
    let cs: Vec<char> = chars.iter().map(|c| c.0).collect();
    let flags = |i: usize| chars.get(i).map(|c| (c.1, c.2)).unwrap_or((false, false));
    let mut out = Shaped::default();
    let mut spans: Vec<(f64, f64, bool, bool)> = Vec::new();
    let mut x = 0.0;
    // Whether the rightmost glyph so far is a stroke-font glyph.
    let mut stroke_last = false;
    for (run, rtl) in bidi::visual_runs(&cs, None) {
        let mut pieces = itemize(&cs, run, primary);
        if rtl {
            pieces.reverse();
        }
        for (src, r) in pieces {
            match &src {
                Src::Hidden => {}
                Src::Stroke | Src::Missing => {
                    let idx: Vec<usize> = if rtl { r.rev().collect() } else { r.collect() };
                    for i in idx {
                        let c = cs.get(i).copied().unwrap_or(' ');
                        let c = if rtl { bidi::mirror(c) } else { c };
                        match (&src, stroke::glyph(c)) {
                            (Src::Stroke, Some((_, spec))) => crate::draw_stroke_glyph(&mut out, spec, x, sc, wf, shear),
                            (Src::Missing, _) => crate::missing_glyph_box(&mut out, x, h, sc * wf),
                            _ => {}
                        }
                        let adv = crate::char_advance(c) * h * wf;
                        let (u, o) = flags(i);
                        spans.push((x, x + adv, u, o));
                        x += adv;
                        stroke_last = true;
                    }
                }
                Src::Face(face) => {
                    for g in shape_run(face, &cs, r, rtl) {
                        let outline = ttf::glyph_by_id(face, g.gid);
                        let k = h / outline.cap * wf;
                        let dy = g.dy * h / outline.cap;
                        let mut contours = ttf::place_glyph(&outline, x + g.dx * k, h, wf, shear);
                        for p in contours.iter_mut().flatten() {
                            *p += Vec2::new(dy * shear, dy);
                        }
                        if !contours.is_empty() {
                            out.glyphs.push(contours);
                        }
                        let adv = g.advance * k;
                        let (u, o) = flags(g.cluster);
                        spans.push((x, x + adv, u, o));
                        x += adv;
                        stroke_last = false;
                    }
                }
            }
        }
    }
    out.strokes.extend(crate::decorations(&spans, h));
    // A stroke-font glyph at the right end: the width leaves out its trailing gap, like
    // `shape_stroke`.
    out.width = if stroke_last { (x - stroke::GAP * sc * wf).max(0.0) } else { x };
    out
}

/// Split a logical-order run into pieces by what draws each character. Joiners and combining
/// marks stay in the piece before them (the shaper needs them in context); other characters
/// prefer the previous fallback face when it has them, so one word stays in one font.
fn itemize(cs: &[char], run: Range<usize>, primary: Option<&Face>) -> Vec<(Src, Range<usize>)> {
    let mut pieces: Vec<(Src, Range<usize>)> = Vec::new();
    for i in run {
        let c = cs.get(i).copied().unwrap_or(' ');
        let prev = pieces.last().and_then(|(s, _)| s.face()).cloned();
        let src = match prev {
            Some(f) if bidi::is_format_control(c) || bidi::is_mark(c) => Src::Face(f),
            None if bidi::is_format_control(c) => Src::Hidden,
            prev => resolve(c, primary, prev.as_ref()),
        };
        match pieces.last_mut() {
            Some((s, r)) if s.same(&src) => r.end = i + 1,
            _ => pieces.push((src, i..i + 1)),
        }
    }
    pieces
}

/// What draws `c`: the text's face when it has the character, else the previous fallback face,
/// else the first fallback font that has it (FONTFALLBACK, FONTALT, installed fonts).
fn resolve(c: char, primary: Option<&Face>, prev: Option<&Face>) -> Src {
    if let Some(p) = primary
        && (c.is_whitespace() || p.has_glyph(c))
    {
        return Src::Face(p.clone());
    }
    if primary.is_none() && (c.is_whitespace() || stroke::glyph(c).is_some()) {
        return Src::Stroke;
    }
    if let Some(f) = prev.filter(|f| f.has_glyph(c)) {
        return Src::Face(f.clone());
    }
    match (ttf::fallback_for(c), primary) {
        (Some(f), _) => Src::Face(f),
        // The text's font draws its own missing-glyph shape.
        (None, Some(p)) => Src::Face(p.clone()),
        (None, None) => Src::Missing,
    }
}

/// A shaped glyph: id, source character index, advance and offset in font units at 1000 per em.
struct Glyph {
    gid: u32,
    cluster: usize,
    advance: f64,
    dx: f64,
    dy: f64,
}

/// Shape characters `r` of `cs` (one direction, one face) into glyphs in display order.
fn shape_run(face: &Face, cs: &[char], r: Range<usize>, rtl: bool) -> Vec<Glyph> {
    let run = cs.get(r.clone()).unwrap_or_default();
    let Ok(font) = harfrust::FontRef::from_index(&face.bytes, face.index) else { return Vec::new() };
    let data = harfrust::ShaperData::new(&font);
    let shaper = data.shaper(&font).build();
    let mut buf = harfrust::UnicodeBuffer::new();
    let arabic = run.iter().any(|c| matches!(*c, '\u{0600}'..='\u{08FF}'));
    if arabic && !face.has_joining_features() {
        for (c, i) in bidi::presentation_forms(run, |p| face.has_glyph(p)) {
            buf.add(c, u32::try_from(r.start + i).unwrap_or(u32::MAX));
        }
    } else {
        for (i, c) in run.iter().enumerate() {
            buf.add(*c, u32::try_from(r.start + i).unwrap_or(u32::MAX));
        }
    }
    buf.set_direction(if rtl { harfrust::Direction::RightToLeft } else { harfrust::Direction::LeftToRight });
    buf.guess_segment_properties();
    let shaped = shaper.shape(buf, harfrust::ShapeOptions::new());
    let k = 1000.0 / f64::from(shaper.units_per_em().max(1));
    shaped
        .glyph_infos()
        .iter()
        .zip(shaped.glyph_positions())
        .map(|(gi, gp)| Glyph {
            gid: gi.glyph_id,
            cluster: usize::try_from(gi.cluster).unwrap_or(usize::MAX),
            advance: f64::from(gp.x_advance) * k,
            dx: f64::from(gp.x_offset) * k,
            dy: f64::from(gp.y_offset) * k,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_font_rtl_lines_keep_latin_and_digits_in_the_stroke_font() {
        // With or without installed fonts: the Persian words are fallback glyphs or
        // missing-glyph boxes, the digits stay stroke glyphs, in the middle of the line (between
        // the two words), and the measured width matches the shaped one.
        let s = "قیمت 25 تومان";
        let sh = shape(None, s, 1.0, 1.0, 0.0).unwrap_or_default();
        assert!(sh.width > 0.0);
        assert!((crate::line_width(s, 1.0, 1.0) - sh.width).abs() < 1e-9);
        let digits: Vec<f64> = sh.strokes.iter().filter(|st| !(st.len() == 5 && st.first() == st.last())).flatten().map(|p| p.x).collect();
        assert!(!digits.is_empty());
        let (lo, hi) = (digits.iter().cloned().fold(f64::MAX, f64::min), digits.iter().cloned().fold(f64::MIN, f64::max));
        assert!(lo > 0.2 * sh.width && hi < 0.8 * sh.width, "digits between the words: {lo}..{hi} of {}", sh.width);
    }

    #[test]
    fn rtl_order_of_stroke_glyphs() {
        // Hebrew letters are R; in an RTL line the Latin word keeps its order and sits where the
        // bidi algorithm puts it: "abc" first visually when it ends the logical line.
        let rtl = crate::shape_stroke("\u{05D0} abc", 1.0, 1.0, 0.0);
        let ltr = crate::shape_stroke("abc", 1.0, 1.0, 0.0);
        let x0 = |s: &Shaped| s.strokes.iter().flatten().map(|p| p.x).fold(f64::MAX, f64::min);
        // "abc" is laid out at the left edge in both.
        assert!((x0(&rtl) - x0(&ltr)).abs() < 1e-9);
        assert!(rtl.width > ltr.width);
    }

    #[test]
    fn invisible_controls_take_no_space() {
        let a = crate::line_width("AB", 1.0, 1.0);
        assert!((crate::line_width("A\u{200C}B", 1.0, 1.0) - a).abs() < 1e-9);
        assert!((crate::line_width("A\u{200F}B", 1.0, 1.0) - a).abs() < 1e-9);
        let sh = crate::shape_stroke("A\u{200F}B", 1.0, 1.0, 0.0);
        assert!(!sh.strokes.iter().any(|s| s.len() == 5 && s.first() == s.last()), "no boxes for marks");
    }

    /// An installed font that shapes Arabic (Persian), or `None` (the test then skips).
    fn arabic_face() -> Option<Face> {
        ttf::fallback_for('\u{0633}').filter(|f| f.has_glyph('\u{06CC}') && f.has_glyph('\u{067E}'))
    }

    fn nominal(f: &Face, c: char) -> Option<u32> {
        use skrifa::MetadataProvider;
        f.font().and_then(|font| font.charmap().map(c)).map(|g| g.to_u32())
    }

    #[test]
    fn persian_is_shaped_by_an_installed_font_when_available() {
        let Some(face) = arabic_face() else { return };
        let cs: Vec<char> = "سلام".chars().collect();
        let gs = shape_run(&face, &cs, 0..cs.len(), true);
        // Display order is right to left: clusters come out descending.
        let clusters: Vec<usize> = gs.iter().map(|g| g.cluster).collect();
        assert!(clusters.windows(2).all(|w| w[0] >= w[1]), "{clusters:?}");
        // Seen is word-initial: not its isolated (nominal) glyph.
        let seen = gs.iter().find(|g| g.cluster == 0).map(|g| g.gid);
        assert!(seen.is_some() && seen != nominal(&face, '\u{0633}'), "initial form");
        // ZWNJ: in "می‌روم" yeh is final, in "میروم" medial.
        let with: Vec<char> = "می\u{200C}روم".chars().collect();
        let without: Vec<char> = "میروم".chars().collect();
        let yeh = |cs: &[char]| shape_run(&face, cs, 0..cs.len(), true).into_iter().find(|g| g.cluster == 1).map(|g| g.gid);
        assert!(yeh(&with).is_some() && yeh(&with) != yeh(&without), "ZWNJ breaks the join");
        // In a stroke-font style the line draws glyph outlines from the fallback font, no boxes.
        let sh = crate::shape_stroke("قیمت 25 تومان", 1.0, 1.0, 0.0);
        assert!(sh.glyphs.len() >= 6, "{}", sh.glyphs.len());
        assert!(!sh.strokes.iter().any(|s| s.len() == 5 && s.first() == s.last()), "no missing-glyph boxes");
        // The ASCII digits are stroke glyphs in the middle of the line.
        let stroke_x: Vec<f64> = sh.strokes.iter().flatten().map(|p| p.x).collect();
        let (lo, hi) = (stroke_x.iter().cloned().fold(f64::MAX, f64::min), stroke_x.iter().cloned().fold(f64::MIN, f64::max));
        assert!(lo > 0.2 * sh.width && hi < 0.8 * sh.width, "digits between the words: {lo}..{hi} of {}", sh.width);
        assert!((crate::line_width("قیمت 25 تومان", 1.0, 1.0) - sh.width).abs() < 1e-9);
    }

    #[test]
    fn hostile_rtl_lines_never_panic() {
        for s in ["\u{202E}%%u\u{05D0}%%", "\u{2067}\u{0644}\u{0627}\u{2069}%%o", "\u{064E}\u{064E}\u{05D0}", "\u{05D0}\u{E000}\u{10FFFF}"] {
            let _ = crate::shape_stroke(s, f64::NAN, -1.0, f64::INFINITY);
            let _ = crate::line_width(s, 0.0, 0.0);
        }
        let long = "\u{05D0}b ".repeat(MAX_CHARS);
        assert!(shape(None, &long, 1.0, 1.0, 0.0).is_none());
        assert!(crate::shape_stroke(&long, 1.0, 1.0, 0.0).width > 0.0);
    }
}
