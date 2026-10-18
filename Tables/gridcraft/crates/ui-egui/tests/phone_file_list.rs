//! The phone File screen: it opens on the list of destinations and a row tap opens that
//! destination's page, whose Back returns to the list.
//!
//! The phone layout is a process-wide switch read once per run, so these tests are gated on
//! `GRIDCRAFT_PHONE=1`: with it unset they skip (a plain `cargo test` never shows the phone UI).

use egui_kittest::kittest::Queryable as _;
use gridcraft_engine::Session;
use gridcraft_ui_egui::backstage::{Backstage, Page};
use gridcraft_ui_egui::SheetApp;

type Harness = egui_kittest::Harness<'static, SheetApp>;

/// A phone-sized app sitting on the File screen. `launched` is what the app sets when it opens the
/// screen itself on launch; anything the user opens (the File tab, a menu) leaves it false.
fn build(launched: bool) -> Harness {
    let mut session = Session::new();
    session.new_workbook();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(412.0, 915.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        SheetApp::new(session, Default::default()),
    );
    h.run_steps(2);
    // The splash covers the first 1.6 s of a phone launch, so let it time out; a test may run after
    // another one already dismissed it, so open the File screen explicitly too.
    std::thread::sleep(std::time::Duration::from_millis(1800));
    let mut bs = Backstage::new(Page::Home);
    bs.launched = launched;
    h.state_mut().backstage = Some(bs);
    h.run_steps(4);
    h
}

/// The File screen as the File tab opens it, with the sheet behind it.
fn harness() -> Option<Harness> {
    if !std::env::var("GRIDCRAFT_PHONE").is_ok_and(|v| v == "1" || v == "true") {
        eprintln!("phone_file_list: skipping — run with GRIDCRAFT_PHONE=1");
        return None;
    }
    Some(build(false))
}

/// The app's first screen: the File screen as the phone shows it on launch.
fn first_screen() -> Option<Harness> {
    if !std::env::var("GRIDCRAFT_PHONE").is_ok_and(|v| v == "1" || v == "true") {
        eprintln!("phone_file_list: skipping — run with GRIDCRAFT_PHONE=1");
        return None;
    }
    Some(build(true))
}

fn tap(h: &mut Harness, label: &str) {
    h.get_by_role_and_label(egui::accesskit::Role::Button, label).click();
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
        ("About", Page::About),
        ("Options", Page::Options),
    ] {
        let Some(mut h) = harness() else { return };
        tap(&mut h, label);
        let bs = h.state().backstage.as_ref().unwrap();
        assert_eq!(bs.page, page, "{label} opens its page");
        assert!(!bs.list, "{label} leaves the list");
    }
}

/// Close is not a phone row (Android's Back closes the workbook) and About is one: the phone has
/// no Help tab to reach it from.
#[test]
fn close_is_gone_and_about_is_offered() {
    let Some(mut h) = harness() else { return };
    assert!(h.query_all_by_label("About").next().is_some(), "About is a row");
    assert!(h.query_all_by_label("Close").next().is_none(), "Close is not a row of the phone list");
    tap(&mut h, "About");
    let bs = h.state().backstage.as_ref().unwrap();
    assert_eq!(bs.page, Page::About);
}

/// The list the app opens on is the app's first screen: the same rows, but no Back bar, since
/// there is nothing behind it to step into (Android's Back is the way off it). The File tab, with
/// the sheet behind it, does show Back — and tapping it leaves the File screen.
#[test]
fn the_launch_list_has_no_back_but_the_file_tab_does() {
    let Some(h) = first_screen() else { return };
    assert!(h.query_all_by_label("‹ Back").next().is_none(), "the first screen shows no Back");
    for row in ["Home", "New", "Open", "Info", "Save As", "Print", "Export", "About", "Options"] {
        assert!(h.query_all_by_label(row).next().is_some(), "{row} is still a row");
    }

    let Some(mut h) = harness() else { return };
    assert!(h.query_all_by_label("‹ Back").next().is_some(), "the File tab shows Back");
    tap(&mut h, "‹ Back");
    assert!(h.state().backstage.is_none(), "Back leaves the File screen for the sheet");
}
