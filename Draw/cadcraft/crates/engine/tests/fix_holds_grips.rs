//! A point held by GCFIX stays where it is when its grip is dragged (#1134): the Fix locks the
//! position the point had, not wherever the edit left it.

use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle};
use cadcraft_engine::geom::Vec2;
use serde_json::json;

fn line(s: &mut Session, a: [f64; 2], b: [f64; 2]) -> Handle {
    let r = s.execute("line", &json!({ "points": [a, b] })).unwrap();
    Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap()
}

fn ends(s: &Session, h: Handle) -> (Vec2, Vec2) {
    match &s.doc().unwrap().entity(h).unwrap().kind {
        EntityKind::Line(l) => (l.a.xy(), l.b.xy()),
        k => panic!("not a line: {k:?}"),
    }
}

fn grip_drag(s: &mut Session, h: Handle, from: Vec2, to: &str) {
    s.set_selection(vec![h]);
    let i = s.grips_of(h).unwrap().iter().position(|g| g.near(from, 1e-9)).unwrap();
    s.start_grip_edit(&[(h, i)]).unwrap();
    s.cmdline(to).unwrap();
    assert!(s.running.is_none(), "{}", s.prompt_text());
}

#[test]
fn dragging_the_grip_of_a_fixed_end_leaves_it_in_place() {
    let mut s = Session::new();
    let a = line(&mut s, [0.0, 0.0], [10.0, 0.0]);
    s.execute("gcfix", &json!({ "h1": a.hex(), "p1": "start" })).unwrap();

    grip_drag(&mut s, a, Vec2::new(0.0, 0.0), "3,4");
    let (start, end) = ends(&s, a);
    assert!(start.near(Vec2::new(0.0, 0.0), 1e-6), "the fixed start moved to {start:?}");
    assert!(end.near(Vec2::new(10.0, 0.0), 1e-6), "{end:?}");

    // The free end still follows its grip.
    grip_drag(&mut s, a, Vec2::new(10.0, 0.0), "10,5");
    let (start, end) = ends(&s, a);
    assert!(start.near(Vec2::new(0.0, 0.0), 1e-6), "{start:?}");
    assert!(end.near(Vec2::new(10.0, 5.0), 1e-6), "{end:?}");
}
