//! Drawing templates (#552): SAVEAS to a .dwt, NEW/QNEW from a template file, and the default
//! template (QNEWTEMPLATE). The io hooks here keep drawings in memory: a "file" holds the number
//! of the drawing written, so the test sees exactly what the engine hands the file formats.

use std::sync::Mutex;

use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use cadcraft_engine::doc::Drawing;
use serde_json::json;

static FILES: Mutex<Vec<(String, Drawing)>> = Mutex::new(Vec::new());

fn write(d: &Drawing, name: &str) -> Result<Vec<u8>, String> {
    let mut files = FILES.lock().unwrap();
    files.push((name.to_string(), d.clone()));
    Ok(format!("TEST {}", files.len() - 1).into_bytes())
}

fn read(bytes: &[u8], _name: &str) -> Result<Drawing, String> {
    let text = String::from_utf8_lossy(bytes);
    let i: usize = text.strip_prefix("TEST ").and_then(|n| n.trim().parse().ok()).ok_or("not a drawing")?;
    FILES.lock().unwrap().get(i).map(|f| f.1.clone()).ok_or_else(|| "no such drawing".into())
}

fn hooks() {
    set_io(IoHooks { read, write, plot: None });
}

fn path(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("cadcraft-templates-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name).to_string_lossy().into_owned()
}

/// A drawing with an office layer and a title block circle, saved as a template.
fn office_template(s: &mut Session, name: &str) -> String {
    s.execute("layer.new", &json!({ "name": "A-WALL", "color": 3 })).unwrap();
    s.execute("circle", &json!({ "center": [5, 5], "radius": 2 })).unwrap();
    let p = path(name);
    s.execute("saveas", &json!({ "path": p })).unwrap();
    p
}

#[test]
fn saveas_dwt_then_new_from_the_template() {
    hooks();
    let mut s = Session::new();
    let dwt = office_template(&mut s, "office.dwt");
    // A template is saved like a drawing: the format sees the .dwt name, the drawing is saved.
    assert!(FILES.lock().unwrap().iter().any(|(n, _)| n.ends_with("office.dwt")));
    let st = s.state().unwrap();
    assert!(!st.is_dirty());
    assert_eq!(st.title, "office.dwt");

    let r = s.execute("new", &json!({ "template": dwt })).unwrap();
    let st = s.state().unwrap();
    assert_eq!(r["title"], json!(st.title));
    assert!(st.title.starts_with("Drawing"), "{}", st.title);
    assert!(st.path.is_none(), "an untitled drawing: saving asks for a name");
    assert!(!st.is_dirty());
    assert!(st.doc.layers.iter().any(|l| l.name == "A-WALL"));
    assert_eq!(st.doc.model.iter().count(), 1);
    assert!(s.execute("qsave", &json!({})).is_err(), "QSAVE needs a name: the template stays as it is");
}

#[test]
fn qnewtemplate_is_the_default_for_new_and_qnew() {
    hooks();
    let mut s = Session::new();
    let dwt = office_template(&mut s, "default.dwt");
    let docs = s.docs.len();

    s.execute("setvar", &json!({ "name": "QNEWTEMPLATE", "value": dwt })).unwrap();
    assert_eq!(s.execute("getvar", &json!({ "name": "qnewtemplate" })).unwrap()["value"], json!(dwt));
    s.cmdline("qnew").unwrap();
    assert_eq!(s.docs.len(), docs + 1);
    assert!(s.doc().unwrap().layers.iter().any(|l| l.name == "A-WALL"), "QNEW starts from the default template");
    s.execute("new", &json!({})).unwrap();
    assert!(s.doc().unwrap().layers.iter().any(|l| l.name == "A-WALL"));

    // Units asked for, or "." for no template: a blank drawing.
    s.execute("new", &json!({ "metric": true })).unwrap();
    assert!(s.doc().unwrap().layers.iter().all(|l| l.name != "A-WALL"));
    assert_eq!(s.doc().unwrap().header.i64("MEASUREMENT", 0), 1);
    s.execute("new", &json!({ "template": "." })).unwrap();
    assert!(s.doc().unwrap().layers.iter().all(|l| l.name != "A-WALL"));

    s.execute("setvar", &json!({ "name": "QNEWTEMPLATE", "value": "." })).unwrap();
    assert_eq!(s.execute("getvar", &json!({ "name": "QNEWTEMPLATE" })).unwrap()["value"], json!(""));
    s.execute("new", &json!({})).unwrap();
    assert!(s.doc().unwrap().layers.iter().all(|l| l.name != "A-WALL"));
}

#[test]
fn a_template_that_cannot_be_read_makes_no_drawing() {
    hooks();
    let mut s = Session::new();
    let docs = s.docs.len();
    let err = s.execute("new", &json!({ "template": "/no/such/folder/office.dwt" })).unwrap_err().to_string();
    assert!(err.contains("office.dwt"), "{err}");
    let junk = path("junk.dwt");
    std::fs::write(&junk, b"not a drawing at all").unwrap();
    assert!(s.execute("new", &json!({ "template": junk })).is_err());
    assert!(s.execute("new", &json!({ "template": 42 })).is_err());
    assert_eq!(s.docs.len(), docs);
}
