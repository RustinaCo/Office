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

fn phone() -> bool {
    std::env::var("DECKCRAFT_PHONE").is_ok_and(|v| v == "1" || v == "true")
}

/// A phone-sized app sitting on the File screen, with a presentation unless asked for none: the
/// same screen in both cases, which is the point of the first-screen test below.
fn build(presentation: bool) -> Harness {
    let session = if presentation { Session::with_new() } else { Session::new() };
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(412.0, 915.0)).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        SlideApp::new(session, Services::default()),
    );
    h.run_steps(2);
    // The splash covers the first 1.6 s of a phone launch, so let it time out; a test may run after
    // another one already dismissed it, so open the File screen explicitly too.
    std::thread::sleep(std::time::Duration::from_millis(1800));
    h.state_mut().open_backstage(Page::Home);
    h.run_steps(4);
    h
}

fn harness() -> Option<Harness> {
    if !phone() {
        eprintln!("phone_file_list: skipping — run with DECKCRAFT_PHONE=1");
        return None;
    }
    Some(build(true))
}

/// The app's first screen: nothing open, and the File screen as the phone shows it on launch.
fn first_screen() -> Option<Harness> {
    if !phone() {
        eprintln!("phone_file_list: skipping — run with DECKCRAFT_PHONE=1");
        return None;
    }
    Some(build(false))
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
        ("About", Page::About),
    ] {
        let Some(mut h) = harness() else { return };
        tap(&mut h, label);
        let bs = h.state().backstage.as_ref().unwrap();
        assert_eq!(bs.page, page, "{label} opens its page");
        assert!(!bs.list, "{label} leaves the list");
    }
}

/// Close and Account are not phone rows; About is one.
#[test]
fn close_and_account_are_gone_and_about_is_offered() {
    let Some(mut h) = harness() else { return };
    assert!(h.query_all_by_label("About").next().is_some(), "About is a row");
    assert!(h.query_all_by_label("Close").next().is_none(), "Close is not a row of the phone list");
    assert!(h.query_all_by_label("Account").next().is_none(), "Account is not a row of the phone list");
    tap(&mut h, "About");
    let bs = h.state().backstage.as_ref().unwrap();
    assert_eq!(bs.page, Page::About);
}

/// With nothing open the File list is the app's first screen: the same rows, but no Back bar —
/// there is nowhere to go back to (Android's Back is the way off it).
#[test]
fn the_first_screen_keeps_every_row_but_has_no_back() {
    let Some(h) = first_screen() else { return };
    assert!(h.state().session.active().is_none(), "nothing is open");
    assert!(h.query_all_by_label("‹ Back").next().is_none(), "the first screen shows no Back");
    for row in ["Home", "New", "Open", "Info", "Save As", "Print", "Export", "About"] {
        assert!(h.query_all_by_label(row).next().is_some(), "{row} is still a row");
    }
}
