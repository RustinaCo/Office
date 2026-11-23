//! Fonts the user adds without rebuilding: `font.add`, `font.list`, `font.remove`.
//!
//! An added font joins the process-wide font database on top of the bundled and system fonts
//! (never replacing a face already there), so it shows in the font lists and renders like any
//! other. Where it is kept between runs is [`FontDb::load_user_fonts`]'s folder (desktop); without
//! one it lasts for the session.

use deckcraft_fonts::{FontDb, UserFont};
use serde_json::{Value, json};

use super::*;
use crate::{EngineError, Result, Session, UiRequest};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "font.add",
            "Add Font…",
            ["Home", "Font"],
            None,
            "{path?: string, data?: base64, name?: string (file name for `data`)} — a TTF, OTF, TTC or OTC file; none: pick a file",
            always,
            add
        ),
        cmd!(query "font.list", "Added Fonts", ["Home", "Font"], None, "{all?: bool (also every available family)}", always, list),
        cmd!(query "font.remove", "Remove Added Font", ["Home", "Font"], None, "{family: string}", always, remove),
    ]
}

fn font_json(f: &UserFont) -> Value {
    json!({
        "file": f.file,
        "families": f.families(),
        "faces": f.faces.iter().map(|(family, style)| json!({"family": family, "style": style})).collect::<Vec<_>>(),
        "path": f.path.as_ref().map(|p| p.to_string_lossy().to_string()),
    })
}

/// Read a font file named by the user, refusing one larger than the cap before reading it.
#[cfg(not(target_arch = "wasm32"))]
fn read_font_file(path: &str) -> Result<Vec<u8>> {
    let too_large = || EngineError::Other(format!("the font file is larger than {} MB", deckcraft_fonts::MAX_FONT_FILE_BYTES >> 20)).to_string();
    let len = std::fs::metadata(path).map(|m| m.len()).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
    if len > deckcraft_fonts::MAX_FONT_FILE_BYTES as u64 {
        return Err(bad("font.add", too_large()));
    }
    std::fs::read(path).map_err(|e| EngineError::Other(format!("{path}: {e}")))
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    let (name, bytes) = if let Some(d) = str_param(p, "data") {
        // Base64 is a third larger than the file: refuse before decoding an oversized one.
        if d.len() / 4 * 3 > deckcraft_fonts::MAX_FONT_FILE_BYTES + 3 {
            return Err(bad("font.add", format!("the font file is larger than {} MB", deckcraft_fonts::MAX_FONT_FILE_BYTES >> 20)));
        }
        let bytes = base64_decode(d).ok_or_else(|| bad("font.add", "bad base64 `data`"))?;
        (str_param(p, "name").unwrap_or("font").to_string(), bytes)
    } else if let Some(path) = str_param(p, "path") {
        #[cfg(not(target_arch = "wasm32"))]
        {
            (path.to_string(), read_font_file(path)?)
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = path;
            return Err(EngineError::Other("reading files by path isn't available on the web".into()));
        }
    } else {
        // From the ribbon: the UI shows a file picker and runs this again with the file.
        s.ui_requests.push(UiRequest::PickFile { purpose: "font".into() });
        return Ok(json!({"pending": true}));
    };
    let r = FontDb::global().install_font(&name, &bytes).map_err(|e| EngineError::Other(e.to_string()))?;
    let families = r.font.families().join(", ");
    let text = match &r.warning {
        Some(w) => format!("Added {families} ({w})"),
        None => format!("Added {families}"),
    };
    s.ui_requests.push(UiRequest::Message { text });
    let mut out = font_json(&r.font);
    if let Some(w) = r.warning {
        out["warning"] = json!(w);
    }
    Ok(out)
}

fn list(_: &mut Session, p: &Value) -> Result<Value> {
    let db = FontDb::global();
    let mut out = json!({
        "fonts": db.user_fonts().iter().map(font_json).collect::<Vec<_>>(),
        "folder": db.user_font_dir().map(|p| p.to_string_lossy().to_string()),
    });
    if bool_or(p, "all", false) {
        out["families"] = json!(db.families());
    }
    Ok(out)
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let family = str_param(p, "family").map(str::trim).filter(|f| !f.is_empty()).ok_or_else(|| bad("font.remove", "missing `family`"))?;
    let gone = FontDb::global().remove_user_font(family).map_err(|e| EngineError::Other(e.to_string()))?;
    s.ui_requests.push(UiRequest::Message { text: format!("Removed {family}") });
    Ok(json!({"removed": gone.iter().map(font_json).collect::<Vec<_>>()}))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../../fonts/tests/fixtures/legacy-semibold.ttf");

    #[test]
    fn add_lists_and_removes_a_font() {
        let mut s = Session::new();
        let data = base64_encode(FIXTURE);
        let out = s.execute("font.add", &json!({"name": "Legacy.ttf", "data": data})).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(out["families"], json!(["DeckCraft Legacy"]));
        let listed = s.execute("font.list", &json!({})).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(listed["fonts"].as_array().map(Vec::len), Some(1));
        assert!(out.get("warning").is_none(), "tests have no user font folder");
        // Adding it again says it is already available.
        assert!(s.execute("font.add", &json!({"name": "Legacy.ttf", "data": base64_encode(FIXTURE)})).is_err());
        let removed = s.execute("font.remove", &json!({"family": "DeckCraft Legacy"})).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(removed["removed"].as_array().map(Vec::len), Some(1));
        assert!(s.execute("font.remove", &json!({"family": "DeckCraft Legacy"})).is_err());
    }

    #[test]
    fn add_with_no_file_asks_the_ui_to_pick_one() {
        let mut s = Session::new();
        let out = s.execute("font.add", &json!({})).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(out["pending"], json!(true));
        assert_eq!(s.ui_requests, vec![UiRequest::PickFile { purpose: "font".into() }]);
    }

    #[test]
    fn junk_input_is_an_error_not_a_panic() {
        let mut s = Session::new();
        assert!(s.execute("font.add", &json!({"data": "%%%"})).is_err());
        assert!(s.execute("font.add", &json!({"name": "x.ttf", "data": base64_encode(b"junk")})).is_err());
        assert!(s.execute("font.remove", &json!({})).is_err());
    }
}
