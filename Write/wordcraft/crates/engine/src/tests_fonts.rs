//! Fonts added at run time (#582): `font.add`, `font.list`, `font.remove`.

use serde_json::json;
use wordcraft_doc::Pos;

use crate::Session;
use crate::cmd::insert::base64_encode;

/// The bundled face `file` of family `original` renamed `family` (as long as `original`), so it
/// is a font the database doesn't have.
fn renamed(file: &str, original: &str, family: &str) -> Vec<u8> {
    assert_eq!(family.len(), original.len());
    let mut data = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts").join(file)).unwrap();
    let utf16 = |s: &str| s.encode_utf16().flat_map(u16::to_be_bytes).collect::<Vec<u8>>();
    for (from, to) in [(utf16(original), utf16(family)), (original.as_bytes().to_vec(), family.as_bytes().to_vec())] {
        let mut i = 0;
        while let Some(at) = data[i..].windows(from.len()).position(|w| w == from) {
            data[i + at..i + at + to.len()].copy_from_slice(&to);
            i += at + to.len();
        }
    }
    data
}

/// The caret's x after `text` set in `font`.
fn text_end_x(s: &mut Session, font: &str, text: &str) -> f32 {
    s.run("document.setText", &json!({"text": text})).unwrap();
    s.run("select.all", &json!({})).unwrap();
    s.run("format.font", &json!({"name": font})).unwrap();
    s.layout().caret_on(&Pos::body(0, text.len()), 0).unwrap().x
}

#[test]
fn an_added_font_is_listed_and_lays_out_text_already_set_in_it() {
    // A monospaced face, so it measures unlike whatever proportional font the system falls back
    // to (Source Serif 4 itself where no system fonts are installed, as on the FreeBSD runner).
    // The name has no Serif/Sans/Mono in it, which would pick a fallback of that kind.
    const FAMILY: &str = "Cmdfont Face 4";
    // Narrow letters: set monospaced they take far more room than in any proportional font.
    const TEXT: &str = "illicit little lilies fill tiny jill";
    let mut s = Session::new(wordcraft_doc::Document::new());
    let mono = text_end_x(&mut s, "JetBrains Mono", TEXT);
    // Not installed yet: the text falls back to another font.
    let missing = text_end_x(&mut s, FAMILY, TEXT);
    assert!((missing - mono).abs() > 1.0, "the fallback measures differently: {missing} vs {mono}");

    let data = base64_encode(&renamed("JetBrainsMono-Regular.ttf", "JetBrains Mono", FAMILY));
    let r = s.run("font.add", &json!({"data": data, "name": "CmdfontFace4-Regular.ttf"})).unwrap();
    assert_eq!(r["families"], json!([FAMILY]));
    assert_eq!(r["faces"][0]["style"], "Regular");
    // The text already set in it re-lays out with it, without touching the document.
    let x = s.layout().caret_on(&Pos::body(0, TEXT.len()), 0).unwrap().x;
    assert!((x - mono).abs() < 0.01, "measured with the added font: {x} vs {mono} (missing {missing})");

    let list = s.run("font.list", &json!({"all": true})).unwrap();
    assert!(list["fonts"].as_array().unwrap().iter().any(|f| f["families"] == json!([FAMILY])));
    assert!(list["families"].as_array().unwrap().iter().any(|f| f == FAMILY));
    // It exports to PDF like any other font.
    assert!(crate::io::save_bytes("out.pdf", &s.doc).unwrap().starts_with(b"%PDF"));

    // Adding it again changes nothing.
    let again = s.run("font.add", &json!({"data": data, "name": "again.ttf"}));
    assert!(again.unwrap_err().to_string().contains("already available"));

    let removed = s.run("font.remove", &json!({"family": FAMILY})).unwrap();
    assert_eq!(removed["removed"].as_array().unwrap().len(), 1);
    let x = s.layout().caret_on(&Pos::body(0, TEXT.len()), 0).unwrap().x;
    assert!((x - missing).abs() < 0.01, "falls back again once removed");
    assert!(s.run("font.remove", &json!({"family": FAMILY})).is_err());
}

#[test]
fn adding_a_font_never_changes_text_that_does_not_use_it() {
    const FAMILY: &str = "Metric Serif 4";
    let mut s = Session::new(crate::sample::sample_document());
    let before = s.layout();
    let positions = |s: &mut Session| {
        let l = s.layout();
        (0..s.doc.body.len().min(12)).filter_map(|b| l.caret_on(&Pos::body(b, 0), 0).map(|c| (c.page, c.x, c.top))).collect::<Vec<_>>()
    };
    let a = positions(&mut s);
    s.run("font.add", &json!({"data": base64_encode(&renamed("SourceSerif4-Bold.ttf", "Source Serif 4", FAMILY)), "name": "m.ttf"})).unwrap();
    assert_eq!(positions(&mut s), a);
    assert_eq!(s.layout().pages.len(), before.pages.len());
}

#[test]
fn bad_font_input_is_an_error() {
    let mut s = Session::new(wordcraft_doc::Document::new());
    for p in [
        json!({"data": base64_encode(b"definitely not a font")}),
        json!({"data": "%%% not base64"}),
        json!({"data": base64_encode(b"wOF2\0\x01\0\0\xff\xff\xff\xff")}),
        json!({"path": "/nonexistent/font.ttf"}),
        json!({"path": env!("CARGO_MANIFEST_DIR")}),
        json!({"path": concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")}),
        json!({"url": "https://fonts.googleapis.com/css2?family=Roboto"}),
    ] {
        assert!(s.run("font.add", &p).is_err(), "{p}");
    }
    assert!(s.run("font.remove", &json!({})).is_err());
    assert!(s.run("font.remove", &json!({"family": "Source Sans 3"})).is_err(), "the bundled fonts stay");
    // No file: the UI is asked for a picker.
    let r = s.run("font.add", &json!({})).unwrap();
    assert_eq!(r["pending"], true);
    assert!(s.ui_requests.iter().any(|r| r["open"] == "addFont"));
}
