//! TrueType subsets for embedding fonts in exported files (PDF): only the glyphs a document
//! uses, plus the composite-glyph components they need. Glyph ids are kept (unused glyphs
//! become empty), so text written with the original ids shows the same glyphs.
//!
//! Embedding follows the font's licence bits (OS/2 `fsType`): fonts marked "restricted
//! licence", "no subsetting" or "bitmap only" are never embedded; callers draw their text as
//! outlines instead. Fonts without TrueType outlines (CFF-flavoured OpenType) aren't subset
//! here either.

use std::collections::BTreeSet;

use skrifa::instance::{LocationRef, Size};
use skrifa::raw::types::Tag;
use skrifa::{FontRef, MetadataProvider};

use crate::ttf::Face;

/// Largest subset written (bytes).
pub const MAX_SUBSET_BYTES: usize = 32 << 20;
/// Most components followed in one composite glyph, and most glyphs a subset may reach.
const MAX_COMPONENTS: usize = 256;

/// Whether a face may be embedded as a subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Embedding {
    /// Installable, editable or preview & print embedding (or no OS/2 table).
    Allowed,
    /// `fsType` restricted licence: no embedding.
    Restricted,
    /// `fsType` bit 8: the font may only be embedded whole.
    NoSubsetting,
    /// `fsType` bit 9: only bitmaps may be embedded.
    BitmapOnly,
    /// No TrueType outlines (`glyf` / `loca`), or the font doesn't parse.
    NotTrueType,
}

/// Why a subset couldn't be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubsetError {
    /// The font may not be embedded as a subset.
    NotAllowed(Embedding),
    /// A table is missing or malformed.
    Malformed(&'static str),
    /// The subset would be larger than [`MAX_SUBSET_BYTES`].
    TooLarge,
}

fn font(face: &Face) -> Option<FontRef<'_>> {
    FontRef::from_index(&face.bytes, face.index).ok()
}

fn tag(t: &[u8; 4]) -> Tag {
    Tag::new(t)
}

fn table<'a>(f: &FontRef<'a>, t: &[u8; 4]) -> Option<&'a [u8]> {
    f.table_data(tag(t)).map(|d| d.as_bytes())
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, *b.get(i + 3)?]))
}

/// The embedding permission of a face.
pub fn embedding(face: &Face) -> Embedding {
    let Some(f) = font(face) else { return Embedding::NotTrueType };
    if table(&f, b"glyf").is_none() || table(&f, b"loca").is_none() || table(&f, b"head").is_none() {
        return Embedding::NotTrueType;
    }
    let Some(fs_type) = table(&f, b"OS/2").and_then(|o| u16_at(o, 8)) else { return Embedding::Allowed };
    if fs_type & 0x0200 != 0 {
        Embedding::BitmapOnly
    } else if fs_type & 0x000F == 0x0002 {
        Embedding::Restricted
    } else if fs_type & 0x0100 != 0 {
        Embedding::NoSubsetting
    } else {
        Embedding::Allowed
    }
}

/// Font-wide metrics for a font descriptor, in units of 1/1000 em.
#[derive(Clone, Debug, PartialEq)]
pub struct FontInfo {
    /// PostScript name: letters, digits and hyphens only (never empty).
    pub postscript_name: String,
    /// Glyph bounding box: x min, y min, x max, y max.
    pub bbox: [f64; 4],
    pub ascent: f64,
    pub descent: f64,
    pub cap_height: f64,
    /// Degrees, counter-clockwise from vertical.
    pub italic_angle: f64,
}

/// A finite value or 0.
fn fin(v: f32) -> f64 {
    let v = f64::from(v);
    if v.is_finite() { v } else { 0.0 }
}

/// The descriptor metrics of a face.
pub fn info(face: &Face) -> Option<FontInfo> {
    let f = font(face)?;
    let m = f.metrics(Size::new(1000.0), LocationRef::default());
    let bbox = m.bounds.map(|b| [fin(b.x_min), fin(b.y_min), fin(b.x_max), fin(b.y_max)]).unwrap_or([0.0, -250.0, 1000.0, 1000.0]);
    let name: String = f
        .localized_strings(skrifa::string::StringId::POSTSCRIPT_NAME)
        .english_or_first()
        .map(|s| s.to_string())
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(63)
        .collect();
    Some(FontInfo {
        postscript_name: if name.is_empty() { "Font".into() } else { name },
        bbox,
        ascent: fin(m.ascent),
        descent: fin(m.descent),
        cap_height: m.cap_height.map(fin).unwrap_or(fin(m.ascent) * 0.72),
        italic_angle: fin(m.italic_angle),
    })
}

/// The advance width of glyph `gid` in units of 1/1000 em.
pub fn advance(face: &Face, gid: u16) -> f64 {
    font(face)
        .and_then(|f| f.glyph_metrics(Size::new(1000.0), LocationRef::default()).advance_width(skrifa::GlyphId::new(u32::from(gid))))
        .map(fin)
        .unwrap_or(0.0)
}

/// Glyph data offsets from `loca`: `n + 1` entries, checked against the `glyf` length.
fn loca_offsets(loca: &[u8], long: bool, n: usize, glyf_len: usize) -> Option<Vec<usize>> {
    let mut v = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let o = if long { u32_at(loca, i * 4)? as usize } else { usize::from(u16_at(loca, i * 2)?) * 2 };
        v.push(o.min(glyf_len));
    }
    Some(v)
}

/// The data of glyph `g` (empty when its range is bad).
fn glyph_data<'a>(glyf: &'a [u8], offsets: &[usize], g: usize) -> &'a [u8] {
    match (offsets.get(g), offsets.get(g + 1)) {
        (Some(a), Some(b)) if a < b => glyf.get(*a..*b).unwrap_or(&[]),
        _ => &[],
    }
}

/// The glyph ids a composite glyph is built from.
fn components(data: &[u8]) -> Vec<u16> {
    let mut out = Vec::new();
    // numberOfContours < 0: a composite.
    if u16_at(data, 0).is_none_or(|n| (n as i16) >= 0) {
        return out;
    }
    let mut pos = 10;
    while out.len() < MAX_COMPONENTS {
        let (Some(flags), Some(gid)) = (u16_at(data, pos), u16_at(data, pos + 2)) else { break };
        out.push(gid);
        pos += 4 + if flags & 0x0001 != 0 { 4 } else { 2 };
        pos += if flags & 0x0008 != 0 {
            2
        } else if flags & 0x0040 != 0 {
            4
        } else if flags & 0x0080 != 0 {
            8
        } else {
            0
        };
        if flags & 0x0020 == 0 {
            break;
        }
    }
    out
}

/// The sum of a table's big-endian 32-bit words (zero-padded).
fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, c| {
        let mut w = [0u8; 4];
        for (d, s) in w.iter_mut().zip(c) {
            *d = *s;
        }
        sum.wrapping_add(u32::from_be_bytes(w))
    })
}

/// A subset of `face` with the glyphs `gids` (and `.notdef` and the components they use). The
/// other glyphs keep their ids but have no outline.
pub fn subset(face: &Face, gids: &BTreeSet<u16>) -> Result<Vec<u8>, SubsetError> {
    let allowed = embedding(face);
    if allowed != Embedding::Allowed {
        return Err(SubsetError::NotAllowed(allowed));
    }
    let f = font(face).ok_or(SubsetError::Malformed("font"))?;
    let head = table(&f, b"head").filter(|h| h.len() >= 54).ok_or(SubsetError::Malformed("head"))?;
    let maxp = table(&f, b"maxp").ok_or(SubsetError::Malformed("maxp"))?;
    let glyf = table(&f, b"glyf").ok_or(SubsetError::Malformed("glyf"))?;
    let loca = table(&f, b"loca").ok_or(SubsetError::Malformed("loca"))?;
    let n = usize::from(u16_at(maxp, 4).ok_or(SubsetError::Malformed("maxp"))?);
    let long = u16_at(head, 50).ok_or(SubsetError::Malformed("head"))? != 0;
    let offsets = loca_offsets(loca, long, n, glyf.len()).ok_or(SubsetError::Malformed("loca"))?;
    // The glyphs to keep: asked for, .notdef, and composite components (transitively).
    let mut keep: BTreeSet<u16> = BTreeSet::new();
    let mut todo: Vec<u16> = std::iter::once(0).chain(gids.iter().copied()).filter(|g| usize::from(*g) < n).collect();
    while let Some(g) = todo.pop() {
        if !keep.insert(g) {
            continue;
        }
        for c in components(glyph_data(glyf, &offsets, usize::from(g))) {
            if usize::from(c) < n && !keep.contains(&c) {
                todo.push(c);
            }
        }
    }
    let mut new_glyf: Vec<u8> = Vec::new();
    let mut new_loca: Vec<u8> = Vec::with_capacity((n + 1) * 4);
    for g in 0..n {
        new_loca.extend_from_slice(&u32::try_from(new_glyf.len()).map_err(|_| SubsetError::TooLarge)?.to_be_bytes());
        if u16::try_from(g).is_ok_and(|g| keep.contains(&g)) {
            new_glyf.extend_from_slice(glyph_data(glyf, &offsets, g));
            while !new_glyf.len().is_multiple_of(4) {
                new_glyf.push(0);
            }
        }
        if new_glyf.len() > MAX_SUBSET_BYTES {
            return Err(SubsetError::TooLarge);
        }
    }
    new_loca.extend_from_slice(&u32::try_from(new_glyf.len()).map_err(|_| SubsetError::TooLarge)?.to_be_bytes());
    let mut new_head = head.to_vec();
    // checkSumAdjustment is set once the file is complete; offsets are long now.
    new_head.get_mut(8..12).ok_or(SubsetError::Malformed("head"))?.copy_from_slice(&[0; 4]);
    new_head.get_mut(50..52).ok_or(SubsetError::Malformed("head"))?.copy_from_slice(&1u16.to_be_bytes());
    let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![(*b"glyf", new_glyf), (*b"head", new_head), (*b"loca", new_loca), (*b"maxp", maxp.to_vec())];
    for t in [b"cvt ", b"fpgm", b"hhea", b"hmtx", b"prep"] {
        if let Some(d) = table(&f, t) {
            tables.push((*t, d.to_vec()));
        }
    }
    if !tables.iter().any(|(t, _)| t == b"hhea") || !tables.iter().any(|(t, _)| t == b"hmtx") {
        return Err(SubsetError::Malformed("hhea/hmtx"));
    }
    tables.sort_by_key(|t| t.0);
    let out = sfnt(&tables);
    if out.len() > MAX_SUBSET_BYTES { Err(SubsetError::TooLarge) } else { Ok(out) }
}

/// Write a TrueType file from `(tag, data)` tables sorted by tag, with checksums and the
/// `head` checksum adjustment.
fn sfnt(tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let count = tables.len() as u16;
    let mut pow = 1u16;
    let mut sel = 0u16;
    while pow.saturating_mul(2) <= count {
        pow = pow.saturating_mul(2);
        sel += 1;
    }
    let range = pow.saturating_mul(16);
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    for v in [count, range, sel, count.saturating_mul(16).saturating_sub(range)] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    let mut offset = 12 + 16 * tables.len();
    let mut head_at = None;
    for (t, d) in tables {
        out.extend_from_slice(t);
        out.extend_from_slice(&checksum(d).to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(d.len() as u32).to_be_bytes());
        offset += d.len().div_ceil(4) * 4;
    }
    for (t, d) in tables {
        if t == b"head" {
            head_at = Some(out.len());
        }
        out.extend_from_slice(d);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }
    if let Some(h) = head_at {
        let adj = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
        if let Some(slot) = out.get_mut(h + 8..h + 12) {
            slot.copy_from_slice(&adj.to_be_bytes());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_components_are_followed_and_bounded() {
        // numberOfContours = -1, then two components (word args, MORE_COMPONENTS on the first).
        let mut g = vec![0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0];
        g.extend_from_slice(&[0x00, 0x21, 0x00, 0x05, 0, 0, 0, 0]);
        g.extend_from_slice(&[0x00, 0x00, 0x00, 0x07, 0, 0]);
        assert_eq!(components(&g), [5, 7]);
        // Truncated and endless (always MORE_COMPONENTS) data stay bounded.
        assert_eq!(components(&g[..14]), [5]);
        let mut endless = vec![0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0];
        for _ in 0..2000 {
            endless.extend_from_slice(&[0x00, 0x20, 0x00, 0x01, 0, 0]);
        }
        assert_eq!(components(&endless).len(), MAX_COMPONENTS);
        assert!(components(&[0, 1, 0]).is_empty());
    }

    #[test]
    fn junk_fonts_are_not_embedded() {
        let junk = Face { bytes: std::sync::Arc::new(b"not a font".to_vec()), index: 0 };
        assert_eq!(embedding(&junk), Embedding::NotTrueType);
        assert!(subset(&junk, &BTreeSet::from([1])).is_err());
        assert!(info(&junk).is_none());
        assert_eq!(advance(&junk, 3), 0.0);
    }

    #[test]
    fn sfnt_directory_and_checksums() {
        let mut head = vec![0u8; 54];
        head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
        let out = sfnt(&[(*b"head", head), (*b"maxp", vec![0, 0, 0x50, 0, 0, 1])]);
        assert_eq!(u16_at(&out, 4), Some(2));
        assert_eq!(out.len() % 4, 0);
        // The whole file sums to the magic number once the adjustment is in.
        assert_eq!(checksum(&out), 0xB1B0_AFBA);
    }
}
