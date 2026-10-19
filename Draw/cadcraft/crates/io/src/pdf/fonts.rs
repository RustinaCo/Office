//! Real text in PDF plots: the outline-font glyphs a plot draws ([`DisplayList::texts`]) are
//! written as text in embedded TrueType subsets, so the text can be selected, searched and
//! copied. Each font is a `Type0` font with `Identity-H` encoding over a `CIDFontType2`
//! descendant whose CIDs are the glyph ids (`/CIDToGIDMap /Identity`), with a `ToUnicode`
//! map for copying and searching.
//!
//! Fonts whose licence bits forbid embedding a subset, fonts without TrueType outlines, and
//! anything past the caps on fonts and font data stay as filled outlines.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use cadcraft_fonts::subset::{self, FontInfo};
use cadcraft_fonts::ttf::Face;
use cadcraft_render::{DisplayList, PlotText};

use super::{Objects, num, refs, stream};

/// Most embedded fonts in one file.
const MAX_FONTS: usize = 64;
/// Most embedded font data in one file (uncompressed bytes).
const MAX_FONT_BYTES: usize = 64 << 20;

/// A face's identity: its loaded file and face index.
type FaceKey = (usize, usize, u32);

fn key(f: &Face) -> FaceKey {
    (f.bytes.as_ptr() as usize, f.bytes.len(), f.index)
}

/// An embedded font.
struct Font {
    face: Face,
    /// Glyphs used, and the character each shows.
    chars: BTreeMap<u16, char>,
    /// The subset file.
    data: Vec<u8>,
    info: FontInfo,
}

/// The fonts of a document: those that can be embedded, by face.
#[derive(Default)]
pub(super) struct Fonts {
    fonts: Vec<Font>,
    by_face: HashMap<FaceKey, Option<usize>>,
}

impl Fonts {
    /// Collect the glyphs of every plotted text glyph and subset their fonts.
    pub(super) fn collect<'a>(lists: impl IntoIterator<Item = &'a DisplayList>) -> Fonts {
        let mut used: Vec<(Face, BTreeMap<u16, char>)> = Vec::new();
        let mut index: HashMap<FaceKey, usize> = HashMap::new();
        for list in lists {
            for t in usable(list) {
                let k = key(&t.glyph.face);
                let i = match index.get(&k) {
                    Some(i) => *i,
                    None => {
                        index.insert(k, used.len());
                        used.push((t.glyph.face.clone(), BTreeMap::new()));
                        used.len() - 1
                    }
                };
                if let Some((_, chars)) = used.get_mut(i) {
                    chars.entry(t.glyph.gid).or_insert(t.glyph.ch);
                }
            }
        }
        let mut out = Fonts::default();
        let mut total = 0usize;
        for (face, chars) in used {
            let k = key(&face);
            let font = (out.fonts.len() < MAX_FONTS)
                .then(|| subset::subset(&face, &chars.keys().copied().collect::<BTreeSet<u16>>()).ok())
                .flatten()
                .filter(|data| total.saturating_add(data.len()) <= MAX_FONT_BYTES)
                .and_then(|data| Some(Font { info: subset::info(&face)?, face, chars, data }));
            match font {
                Some(f) => {
                    total = total.saturating_add(f.data.len());
                    out.by_face.insert(k, Some(out.fonts.len()));
                    out.fonts.push(f);
                }
                None => {
                    out.by_face.insert(k, None);
                }
            }
        }
        out
    }

    pub(super) fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    /// The embedded font (index) a glyph is written in; `None`: draw its outline.
    fn font_of(&self, t: &PlotText) -> Option<usize> {
        self.by_face.get(&key(&t.glyph.face)).copied().flatten()
    }

    /// The plotted glyphs of `list` written as text: outline primitive → (font, glyph).
    pub(super) fn texts<'a>(&self, list: &'a DisplayList) -> HashMap<u32, (usize, &'a PlotText)> {
        usable(list).filter_map(|t| Some((t.prim, (self.font_of(t)?, t)))).collect()
    }

    /// Write the font objects; returns each font's `Type0` object, by font index.
    pub(super) fn write(&self, objs: &mut Objects, compress: bool) -> Vec<usize> {
        self.fonts.iter().map(|f| write_font(objs, f, compress)).collect()
    }
}

/// The text glyphs of a list that stand for a primitive of it.
fn usable(list: &DisplayList) -> impl Iterator<Item = &PlotText> {
    list.texts.iter().filter(|t| (t.prim as usize) < list.prims.len() && t.glyph.gid != 0)
}

/// Six capital letters naming a subset (PDF subset fonts are `ABCDEF+Name`), from its glyphs.
fn subset_tag(f: &Font) -> String {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for b in f.info.postscript_name.bytes().chain(f.chars.keys().flat_map(|g| g.to_be_bytes())) {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3);
    }
    (0..6).map(|i| char::from(b'A' + ((h >> (i * 8)) % 26) as u8)).collect()
}

fn write_font(objs: &mut Objects, f: &Font, compress: bool) -> usize {
    let name = format!("{}+{}", subset_tag(f), f.info.postscript_name);
    let file = objs.add(stream(&f.data, compress, &format!(" /Length1 {}", f.data.len())));
    let mut bbox = String::new();
    for (i, v) in f.info.bbox.iter().enumerate() {
        if i > 0 {
            bbox.push(' ');
        }
        num(&mut bbox, v.round());
    }
    let mut metrics = String::new();
    for (k, v) in [("Ascent", f.info.ascent), ("Descent", f.info.descent), ("CapHeight", f.info.cap_height), ("ItalicAngle", f.info.italic_angle)] {
        let _ = write!(metrics, " /{k} ");
        num(&mut metrics, v.round());
    }
    let descriptor =
        objs.add(format!("<< /Type /FontDescriptor /FontName /{name} /Flags 4 /FontBBox [{bbox}]{metrics} /StemV 80 /FontFile2 {file} 0 R >>"));
    let mut widths = String::new();
    for g in f.chars.keys() {
        let _ = write!(widths, "{g} [");
        num(&mut widths, subset::advance(&f.face, *g).round());
        widths.push_str("] ");
    }
    let cid = objs.add(format!(
        "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /CIDToGIDMap /Identity /DW 1000 /W [{}] >>",
        widths.trim_end()
    ));
    let to_unicode = objs.add(stream(to_unicode(&f.chars).as_bytes(), compress, ""));
    objs.add(format!(
        "<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{}] /ToUnicode {to_unicode} 0 R >>",
        refs([cid])
    ))
}

/// A `ToUnicode` CMap from glyph ids (2-byte codes) to the characters they show.
fn to_unicode(chars: &BTreeMap<u16, char>) -> String {
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<(&u16, &char)> = chars.iter().collect();
    // At most 100 entries per block.
    for block in entries.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", block.len());
        for (g, c) in block {
            let _ = write!(s, "<{g:04X}> <");
            for u in c.encode_utf16(&mut [0; 2]) {
                let _ = write!(s, "{u:04X}");
            }
            s.push_str(">\n");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s
}
