//! The phone File screen: it opens on the list of destinations and a row tap opens that
//! destination's page, whose Back returns to the list.
//!
//! The phone layout is a process-wide switch read once per run, so these tests are gated on
//! `DECKCRAFT_PHONE=1`: with it unset they skip (a plain `cargo test` never shows the phone UI).

use deckcraft_engine::Session;
use deckcraft_ui_egui::backstage::Page;
use deckcraft_ui_egui::{Services, SlideApp};
use egui::accesskit::Role;
use egui_kittest::kittest::Queryable as _;

type Harness = egui_kittest::Harness<'static, SlideApp>;

fn harness() -> Option<Harness> {
    if !std::env::var("DECKCRAFT_PHONE").is_ok_and(|v| v == "1" || v == "true") {
        eprintln!("phone_file_list: skipping — run with DECKCRAFT_PHONE=1");
        return None;
    }
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(412.0, 915.0)).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        SlideApp::new(Session::with_new(), Services::default()),
    );
    h.run_steps(2);
    // The splash covers the first 1.6 s of a phone launch, so let it time out; a test may run after
    // another one already dismissed it, so open the File screen explicitly too.
    std::thread::sleep(std::time::Duration::from_millis(1800));
    h.state_mut().open_backstage(Page::Home);
    h.run_steps(4);
    Some(h)
}

fn tap(h: &mut Harness, label: &str) {
    h.get_by_role_and_label(Role::Button, label).click();
    h.run_steps(4);
}

#[test]
fn phone_opens_on_the_file_list() {
    let Some(h) = harness() else { return };
    let bs = h.state().backstage.as_ref().expect("the phone opens on the File screen");
    assert!(bs.list, "the File screen opens on the list of destinations");
    assert_eq!(bs.page, Page::Home);
}

/// A row tap used to change the page but leave the list showing, so it looked like nothing
/// happened (the phone's File screen was stuck one level up).
#[test]
fn a_list_row_opens_its_page_and_back_returns_to_the_list() {
    let Some(mut h) = harness() else { return };
    tap(&mut h, "New");
    let bs = h.state().backstage.as_ref().expect("still on the File screen");
    assert!(!bs.list, "tapping New leaves the list (page={}, list={})", bs.page.id(), bs.list);
    assert_eq!(bs.page, Page::New);

    tap(&mut h, "‹ Back");
    let bs = h.state().backstage.as_ref().expect("still on the File screen");
    assert!(bs.list, "Back returns to the list");
}

#[test]
fn every_destination_is_reachable() {
    for (label, page) in [
        ("Home", Page::Home),
        ("New", Page::New),
        ("Open", Page::Open),
        ("Info", Page::Info),
        ("Save As", Page::SaveAs),
        ("Print", Page::Print),
        ("Export", Page::Export),
        ("Account", Page::Account),
    ] {
        let Some(mut h) = harness() else { return };
        tap(&mut h, label);
        let bs = h.state().backstage.as_ref().unwrap();
        assert_eq!(bs.page, page, "{label} opens its page");
        assert!(!bs.list, "{label} leaves the list");
    }
}

#[test]
fn options_opens_the_preferences_dialog_over_the_list() {
    let Some(mut h) = harness() else { return };
    tap(&mut h, "Options");
    assert!(h.state().dialog.is_some(), "Options opens the preferences dialog");
}
