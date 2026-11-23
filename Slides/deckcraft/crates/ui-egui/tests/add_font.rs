//! Adding a font the user owns: the `Add Font…` ribbon command picks a file and installs it, and a
//! dropped/picked font file is routed to the same command. Either way the family joins the font
//! list and can be removed again.

use std::sync::{Arc, Mutex};

use deckcraft_engine::Session;
use deckcraft_ui_egui::{Services, SlideApp};
use serde_json::json;

const FIXTURE: &[u8] = include_bytes!("../../fonts/tests/fixtures/legacy-semibold.ttf");
const FAMILY: &str = "DeckCraft Legacy";

/// The process-wide font database is shared, so these tests take turns.
fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn app_picking(purpose_log: Arc<Mutex<Vec<String>>>, path: std::path::PathBuf) -> SlideApp {
    let pick = Box::new(move |purpose: &str| {
        purpose_log.lock().unwrap_or_else(|e| e.into_inner()).push(purpose.to_string());
        (purpose == "font").then(|| path.to_string_lossy().to_string())
    });
    SlideApp::new(Session::with_new(), Services { pick_open: Some(pick), ..Default::default() })
}

#[test]
fn add_font_command_picks_a_file_and_installs_it() {
    let _guard = lock();
    assert!(!deckcraft_fonts::FontDb::global().has_family(FAMILY), "the fixture isn't installed to begin with");

    let dir = std::env::temp_dir().join(format!("deck-add-font-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Picked Font.ttf");
    std::fs::write(&path, FIXTURE).unwrap();
    let picked = Arc::new(Mutex::new(Vec::new()));
    let mut app = app_picking(picked.clone(), path.clone());

    app.run("app.addFont", json!({})).unwrap();
    assert_eq!(picked.lock().unwrap_or_else(|e| e.into_inner()).as_slice(), ["font"], "the command asks for a font purpose");

    // It joins the font list the ribbon shows and the added-fonts list, and the document is
    // untouched (a font is not content).
    assert!(deckcraft_ui_egui::ribbon::font_families(&app).iter().any(|f| f == FAMILY));
    let listed = app.run("font.list", json!({})).unwrap();
    assert_eq!(listed["fonts"].as_array().map(Vec::len), Some(1));

    // Removing it takes it out of the list again; other tests see a clean database.
    app.run("font.remove", json!({"family": FAMILY})).unwrap();
    assert!(!deckcraft_fonts::FontDb::global().has_family(FAMILY));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_dropped_font_file_is_added_like_a_picked_one() {
    let _guard = lock();
    assert!(!deckcraft_fonts::FontDb::global().has_family(FAMILY), "the fixture isn't installed to begin with");
    let mut app = SlideApp::new(Session::with_new(), Services::default());
    let slides = app.session.active().map(|d| d.doc.slides.len());

    app.open_or_insert_bytes("Dropped Font.ttf", FIXTURE);

    assert!(deckcraft_ui_egui::ribbon::font_families(&app).iter().any(|f| f == FAMILY));
    assert_eq!(app.session.active().map(|d| d.doc.slides.len()), slides, "adding a font doesn't touch the presentation");

    app.run("font.remove", json!({"family": FAMILY})).unwrap();
    assert!(!deckcraft_fonts::FontDb::global().has_family(FAMILY));
}
