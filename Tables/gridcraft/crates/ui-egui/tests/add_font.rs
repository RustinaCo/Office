//! Add Font (#582) through the UI: the `font.add`/`font.list`/`font.remove` commands install a
//! font file into the user font folder, and Options ▸ Fonts offers Add Font… and lists what was
//! added.

use std::sync::{Arc, Mutex};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use gridcraft_engine::Session;
use gridcraft_ui_egui::{Services, SheetApp, fonts};
use serde_json::json;

/// The font catalog is process-wide; these tests take turns with it.
static LOCK: Mutex<()> = Mutex::new(());

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gridcraft-add-font-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn harness(dir: &std::path::Path, services: Services) -> Harness<'static, SheetApp> {
    fonts::load_user_fonts(dir);
    let mut session = Session::new();
    session.new_workbook();
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 1400.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        SheetApp::new(session, services),
    );
    h.run_steps(4);
    h
}

#[test]
fn font_commands_install_list_and_remove_a_file() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = temp_dir("cmd");
    let mut h = harness(&dir, Services::default());
    let data = gridcraft_engine::io::base64_encode(epaint_default_fonts::UBUNTU_LIGHT);

    let added = h.state_mut().run("font.add", json!({"name": "Ubuntu-Light.ttf", "data": data})).expect("font.add");
    assert_eq!(added["families"], json!(["Ubuntu"]));
    let path = added["path"].as_str().expect("a stored path").to_string();
    assert!(path.starts_with(&dir.to_string_lossy().to_string()), "{path} isn't in {dir:?}");
    assert!(std::path::Path::new(&path).exists());

    let list = h.state_mut().run("font.list", json!({})).expect("font.list");
    assert_eq!(list["fonts"].as_array().map(Vec::len), Some(1));
    assert_eq!(list["fonts"][0]["file"], json!("Ubuntu-Light.ttf"));
    assert_eq!(list["fonts"][0]["families"], json!(["Ubuntu"]));

    let removed = h.state_mut().run("font.remove", json!({"family": "Ubuntu"})).expect("font.remove");
    assert_eq!(removed["removed"].as_array().map(Vec::len), Some(1));
    assert!(!std::path::Path::new(&path).exists(), "the stored file is deleted");
    let list = h.state_mut().run("font.list", json!({})).expect("font.list");
    assert_eq!(list["fonts"].as_array().map(Vec::len), Some(0));
    // A second removal reports an error the caller sees, instead of panicking.
    assert!(h.state_mut().run("font.remove", json!({"family": "Ubuntu"})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn options_fonts_offers_add_font_and_lists_the_added_ones() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = temp_dir("ui");
    let picked = Arc::new(Mutex::new(0));
    let capture = picked.clone();
    let mut h = harness(
        &dir,
        Services {
            pick_font: Some(Box::new(move || {
                *capture.lock().unwrap_or_else(|e| e.into_inner()) += 1;
                None
            })),
            ..Default::default()
        },
    );
    h.state_mut().run("ui.backstage", json!({"page": "options"})).unwrap();
    h.run_steps(3);

    // The Fonts section's button opens the platform picker (which we cancel).
    h.get_by_role_and_label(Role::Button, "Add Font…").click();
    h.run_steps(3);
    assert_eq!(*picked.lock().unwrap_or_else(|e| e.into_inner()), 1, "Add Font… opens the font picker");

    // A font added through the command shows in the page, with a Remove button that takes it out.
    let data = gridcraft_engine::io::base64_encode(epaint_default_fonts::UBUNTU_LIGHT);
    h.state_mut().run("font.add", json!({"name": "Ubuntu-Light.ttf", "data": data})).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Ubuntu").is_some(), "the added family is listed");
    h.get_by_role_and_label(Role::Button, "Remove").click();
    h.run_steps(3);
    let list = h.state_mut().run("font.list", json!({})).unwrap();
    assert_eq!(list["fonts"].as_array().map(Vec::len), Some(0), "Remove forgets the font");
    assert!(h.query_by_label("Ubuntu").is_none(), "the family is gone from the page");
    let _ = std::fs::remove_dir_all(&dir);
}
