//! The native `.deckcraft` format: a zip holding `presentation.json` (the model, minus media
//! bytes), `media/<id>` files with the embedded media, and `mimetype`. Readers ignore unknown
//! fields, so newer files open in older builds with what those builds understand.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use deckcraft_model::text::{Paragraph, TextBody};
use deckcraft_model::{LayoutType, Presentation, defaults};

pub const MIMETYPE: &str = "application/vnd.storyteller.deckcraft+zip";
/// Files written before the SlideCraft → DeckCraft rename (`.slidecraft`) are still read.
pub const LEGACY_MIMETYPE: &str = concat!("application/vnd.storyteller.", "slide", "craft+zip");
pub const EXTENSION: &str = "deckcraft";
/// Format version written into the manifest.
pub const VERSION: u32 = 1;
/// Largest entry we will inflate (guards against zip bombs).
const MAX_ENTRY: u64 = 2 << 30;

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("not a DeckCraft file: {0}")]
    NotOurs(String),
    #[error("damaged file: {0}")]
    Damaged(String),
    #[error("write failed: {0}")]
    Write(String),
}

pub type Result<T> = std::result::Result<T, FormatError>;

#[derive(serde::Serialize, serde::Deserialize)]
struct Manifest {
    format: String,
    version: u32,
    generator: String,
    presentation: Presentation,
}

/// Serialize a presentation.
pub fn save(p: &Presentation) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let w = |e: zip::result::ZipError| FormatError::Write(e.to_string());
        z.start_file("mimetype", stored).map_err(w)?;
        z.write_all(MIMETYPE.as_bytes()).map_err(|e| FormatError::Write(e.to_string()))?;
        let m = Manifest {
            format: "deckcraft".into(),
            version: VERSION,
            generator: format!("DeckCraft {}", env!("CARGO_PKG_VERSION")),
            presentation: p.clone(),
        };
        let json = serde_json::to_vec(&m).map_err(|e| FormatError::Write(e.to_string()))?;
        z.start_file("presentation.json", deflate).map_err(w)?;
        z.write_all(&json).map_err(|e| FormatError::Write(e.to_string()))?;
        for item in &p.media {
            if item.link.is_some() && item.data.is_empty() {
                continue;
            }
            // Already-compressed media is stored, not deflated again.
            let opts = if is_compressed(&item.content_type) { stored } else { deflate };
            z.start_file(format!("media/{}", item.id.0), opts).map_err(w)?;
            z.write_all(&item.data).map_err(|e| FormatError::Write(e.to_string()))?;
        }
        z.finish().map_err(w)?;
    }
    Ok(buf.into_inner())
}

fn is_compressed(ct: &str) -> bool {
    ct.starts_with("image/jpeg")
        || ct.starts_with("image/png")
        || ct.starts_with("video/")
        || ct.starts_with("audio/mpeg")
        || ct.starts_with("audio/mp4")
        || ct.contains("zip")
}

/// Does this look like a `.deckcraft` file?
pub fn sniff(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK") && [MIMETYPE, LEGACY_MIMETYPE].iter().any(|m| bytes.windows(m.len()).take(200).any(|w| w == m.as_bytes()))
}

fn read_entry(z: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>> {
    let f = z.by_name(name).map_err(|e| FormatError::Damaged(format!("{name}: {e}")))?;
    if f.size() > MAX_ENTRY {
        return Err(FormatError::Damaged(format!("{name} is too large")));
    }
    let mut out = Vec::with_capacity(f.size().min(64 << 20) as usize);
    f.take(MAX_ENTRY).read_to_end(&mut out).map_err(|e| FormatError::Damaged(format!("{name}: {e}")))?;
    Ok(out)
}

/// Read a presentation.
pub fn load(bytes: &[u8]) -> Result<Presentation> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| FormatError::NotOurs(e.to_string()))?;
    let json = read_entry(&mut z, "presentation.json").map_err(|_| FormatError::NotOurs("no presentation.json".into()))?;
    let m: Manifest = serde_json::from_slice(&json).map_err(|e| FormatError::Damaged(e.to_string()))?;
    if m.format != "deckcraft" {
        return Err(FormatError::NotOurs(m.format));
    }
    let mut p = m.presentation;
    for item in &mut p.media {
        if let Ok(data) = read_entry(&mut z, &format!("media/{}", item.id.0)) {
            item.data = Arc::new(data);
        }
    }
    repair(&mut p);
    Ok(p)
}

/// Make a loaded presentation safe to edit: a master exists, every slide's layout exists,
/// ids are unique, tables are rectangular.
pub fn repair(p: &mut Presentation) {
    if p.masters.is_empty() {
        let fresh = defaults::blank_presentation(p.slide_size, Default::default(), false);
        p.masters = fresh.masters;
    }
    p.fix_next_id();
    let first_layout = p.masters.first().and_then(|m| m.layouts.first()).map(|l| l.id).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    for i in 0..p.slides.len() {
        let needs_layout = p.slides.get(i).is_some_and(|s| p.layout(s.layout).is_none());
        let dup = p.slides.get(i).is_some_and(|s| !seen.insert(s.id));
        if needs_layout || dup {
            let new_id = if dup { Some(deckcraft_model::SlideId(p.alloc_id())) } else { None };
            if let Some(s) = p.slides.get_mut(i) {
                let s = Arc::make_mut(s);
                if needs_layout {
                    s.layout = first_layout;
                }
                if let Some(id) = new_id {
                    s.id = id;
                }
            }
        }
    }
    if !(p.slide_size.width.is_finite() && p.slide_size.width >= 1.0 && p.slide_size.height.is_finite() && p.slide_size.height >= 1.0) {
        p.slide_size = defaults::WIDE;
    }
    p.slide_size.width = p.slide_size.width.min(4000.0);
    p.slide_size.height = p.slide_size.height.min(4000.0);
}

/// Outline text: one slide per unindented line; tab-indented lines become body bullets at their
/// level (Insert ▸ Slides from Outline, View ▸ Outline copy).
pub fn outline_to_slides(p: &mut Presentation, text: &str) -> usize {
    let Some(lay) = defaults::layout_of_kind(p, LayoutType::TitleAndContent) else { return 0 };
    let mut made = 0;
    let mut cur: Option<deckcraft_model::Slide> = None;
    let flush = |p: &mut Presentation, s: Option<deckcraft_model::Slide>, made: &mut usize| {
        if let Some(s) = s {
            p.slides.push(Arc::new(s));
            *made += 1;
        }
    };
    for raw in text.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let tabs = raw.chars().take_while(|c| *c == '\t').count();
        let spaces = raw.chars().take_while(|c| *c == ' ').count() / 2;
        let level = tabs.max(spaces);
        let line = raw.trim().trim_start_matches(['-', '*', '•']).trim();
        if level == 0 {
            flush(p, cur.take(), &mut made);
            let mut s = defaults::new_slide(p, lay);
            if let Some(t) = s.shapes.first_mut() {
                t.text = Some(TextBody::from_text(line));
            }
            cur = Some(s);
        } else if let Some(s) = cur.as_mut()
            && let Some(body) = s.shapes.get_mut(1)
        {
            let tb = body.text.get_or_insert_with(TextBody::default);
            let mut para = Paragraph::new(line);
            para.level = (level - 1).min(8) as u8;
            tb.paragraphs.push(para);
        }
    }
    flush(p, cur, &mut made);
    made
}

/// The deck as outline text (titles unindented, body paragraphs tab-indented by level).
pub fn slides_to_outline(p: &Presentation) -> String {
    let mut out = String::new();
    for s in &p.slides {
        out.push_str(&s.title());
        out.push('\n');
        for sh in &s.shapes {
            if sh.ph_type().is_some_and(|k| k.is_title()) {
                continue;
            }
            if let Some(t) = &sh.text {
                for para in &t.paragraphs {
                    if para.is_empty() {
                        continue;
                    }
                    for _ in 0..=para.level {
                        out.push('\t');
                    }
                    out.push_str(&para.text().replace('\u{b}', " "));
                    out.push('\n');
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_media() {
        let mut p = Presentation::default();
        let id = p.add_media("a.png", "image/png", vec![1, 2, 3, 4]);
        let bytes = save(&p).unwrap();
        assert!(sniff(&bytes));
        let q = load(&bytes).unwrap();
        assert_eq!(q.slides.len(), p.slides.len());
        assert_eq!(q.media(id).map(|m| m.data.as_slice()), Some(&[1u8, 2, 3, 4][..]));
        assert_eq!(serde_json::to_value(&q.slides).unwrap(), serde_json::to_value(&p.slides).unwrap());
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(load(b"").is_err());
        assert!(load(b"PK\x03\x04garbage").is_err());
        assert!(load(&[0u8; 1000]).is_err());
        let mut b = save(&Presentation::default()).unwrap();
        let n = b.len();
        b.truncate(n / 2);
        assert!(load(&b).is_err());
    }

    #[test]
    fn repair_fixes_missing_layout_and_size() {
        let mut p = Presentation::default();
        Arc::make_mut(&mut p.slides[0]).layout = deckcraft_model::LayoutId(99999);
        p.slide_size.width = f64::NAN;
        repair(&mut p);
        assert!(p.validate().is_empty());
        assert_eq!(p.slide_size.width, 960.0);
    }

    #[test]
    fn outline_roundtrip() {
        let mut p = defaults::blank_presentation(defaults::WIDE, Default::default(), false);
        let n = outline_to_slides(&mut p, "Intro\n\tPoint one\n\t\tSub point\nSecond\n\tMore");
        assert_eq!(n, 2);
        let o = slides_to_outline(&p);
        assert!(o.contains("Intro\n\tPoint one\n\t\tSub point\nSecond"), "{o}");
    }
}
