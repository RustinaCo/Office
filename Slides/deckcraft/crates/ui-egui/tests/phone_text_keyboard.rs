//! Phone: while a slide text box is being edited, a tap on the ribbon/title bar puts the
//! on-screen keyboard away, and a tap on the slide brings it back and keeps editing. The slide's
//! text box is not an egui widget, so this is the "keyboard dismissed" state the canvas reports.
//!
//! Phone mode is read from `DECKCRAFT_PHONE` once per process, so this lives in its own test
//! binary (run with `DECKCRAFT_PHONE=1`).

use deckcraft_engine::Session;
use deckcraft_ui_egui::{Services, SlideApp};
use egui::{Event, Modifiers, PointerButton, pos2};
use serde_json::json;

type Harness = egui_kittest::Harness<'static, SlideApp>;

fn phone() -> bool {
    std::env::var("DECKCRAFT_PHONE").is_ok_and(|v| v == "1" || v == "true")
}

fn build() -> Harness {
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(412.0, 915.0)).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        SlideApp::new(Session::with_new(), Services::default()),
    );
    h.run_steps(2);
    // The phone start screen waits a moment before it can be dismissed.
    std::thread::sleep(std::time::Duration::from_millis(1800));
    deckcraft_ui_egui::start::dismiss();
    h.run_steps(4);
    h
}

fn tap(h: &Harness, p: egui::Pos2) {
    h.event(Event::PointerMoved(p));
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::default() });
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::default() });
}

fn ime(h: &Harness) -> bool {
    h.output().platform_output.ime.is_some()
}

#[test]
fn chrome_tap_dismisses_keyboard_and_slide_tap_restores_it() {
    if !phone() {
        eprintln!("skipping: set DECKCRAFT_PHONE=1");
        return;
    }
    let mut h = build();
    assert!(h.state().backstage.is_none(), "the File screen should have been left");
    let title = h.state().session.active().expect("a presentation").doc.slides[0].shapes[0].id.0;

    // Editing the title: the shell is asked for the keyboard.
    h.state_mut().run("text.edit", json!({ "id": title })).expect("edit");
    h.run_steps(2);
    assert!(h.state().session.active().expect("a presentation").selection.text.is_some(), "a text box is being edited");
    assert!(ime(&h), "editing slide text turns the keyboard on");
    assert!(!h.state().keyboard_dismissed, "nothing has dismissed it yet");

    // A tap on the chrome (title bar / ribbon, above the canvas) puts it away.
    let canvas = h.state().canvas_rect.expect("the canvas is laid out");
    assert!(canvas.min.y > 40.0, "the chrome sits above the canvas");
    tap(&h, pos2(200.0, 30.0));
    h.run_steps(3);
    assert!(!canvas.contains(pos2(200.0, 30.0)), "the tap is outside the slide");
    assert!(h.state().keyboard_dismissed, "a chrome tap dismisses the keyboard");
    assert!(!ime(&h), "the keyboard is away after the chrome tap");
    assert!(h.state().session.active().expect("a presentation").selection.text.is_some(), "editing continues");

    // A tap on the slide brings it back.
    tap(&h, h.state().slide_rect.expect("the slide is laid out").center());
    h.run_steps(3);
    assert!(!h.state().keyboard_dismissed, "a slide tap clears the dismissal");
    assert!(ime(&h), "the keyboard is back while editing the slide");

    // Leaving the text box drops the state, so the next edit starts with the keyboard on.
    h.state_mut().run("text.exit", json!({})).expect("exit");
    h.run_steps(2);
    assert!(!h.state().keyboard_dismissed, "no dismissal is remembered once editing stops");
}
