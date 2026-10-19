//! Double-clicking a name in the Parameters Manager edits it in place, as in AutoCAD (#1134):
//! the keys go to the name field, not to the command line, and Enter renames the parameter.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Event, Modifiers, PointerButton, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

fn harness() -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.run("parameters", json!({ "name": "user1", "expr": "4" })).unwrap();
    app.run("parameters", json!({ "name": "user2", "expr": "user1*2" })).unwrap();
    app.ui.dialog = Some("parameters".into());
    // 60 frames a second: the default step (0.25 s) spreads two clicks past the double-click delay.
    let mut h = Harness::builder().with_size(vec2(1280.0, 760.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    h
}

fn double_click_label(h: &mut Harness<'_, CadApp>, label: &str) {
    let pos = h.get_by_label(label).rect().center();
    h.event(Event::PointerMoved(pos));
    h.run_steps(2);
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
            h.run_steps(1);
        }
    }
    h.run_steps(2);
}

fn parameters(h: &Harness<'_, CadApp>) -> Vec<(String, String)> {
    let d = h.state().session.doc().unwrap();
    d.parametric.parameters.iter().map(|p| (p.name.clone(), p.expr.clone())).collect()
}

#[test]
fn double_click_types_a_new_name_and_enter_renames() {
    let mut h = harness();
    double_click_label(&mut h, "user1");
    h.event(Event::Text("width".into()));
    h.run_steps(2);
    assert_eq!(h.state().cmd.buffer, "", "the command line got the keys");

    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(parameters(&h), vec![("width".into(), "4".into()), ("user2".into(), "width*2".into())]);
    assert!(h.query_by_label("width").is_some(), "the table shows the new name");
    assert!(h.state().cmd.buffer.is_empty());
}

#[test]
fn escape_leaves_the_name_as_it_was() {
    let mut h = harness();
    double_click_label(&mut h, "user1");
    h.event(Event::Text("x".into()));
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(parameters(&h), vec![("user1".into(), "4".into()), ("user2".into(), "user1*2".into())]);
    assert!(h.state().cmd.buffer.is_empty());
}
