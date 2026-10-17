use serde_json::json;

use crate::*;

fn session() -> Session {
    Session::with_new()
}

#[test]
fn new_presentation_and_slides() {
    let mut s = session();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 1);
    s.execute("slide.new", &json!({})).unwrap();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 3);
    assert_eq!(s.doc().unwrap().selection.slide, 2);
    s.execute("slide.duplicate", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 4);
    s.execute("slide.move", &json!({"from": 3, "to": 0})).unwrap();
    s.execute("slide.delete", &json!({"index": 0})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 3);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 4);
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 3);
}

#[test]
fn insert_shape_and_format() {
    let mut s = session();
    let r = s.execute("shape.insert", &json!({"preset": "roundRect", "rect": [100, 100, 200, 100], "text": "Hi"})).unwrap();
    let id = r["id"].as_u64().unwrap();
    s.execute("shape.fill", &json!({"color": "#ff0000"})).unwrap();
    s.execute("shape.line", &json!({"width": 3, "color": "accent2", "dash": "dash"})).unwrap();
    s.execute("shape.rotate", &json!({"deg": 30})).unwrap();
    s.execute("shape.move", &json!({"dx": 10, "dy": -5})).unwrap();
    s.execute("format.bold", &json!({})).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert_eq!(v["box"]["x"], 110.0);
    assert_eq!(v["box"]["rot"], 30.0);
    let runs = &v["shape"]["text"]["paragraphs"][0]["runs"][0]["props"];
    assert_eq!(runs["bold"], true);
    // Undo restores step by step.
    for _ in 0..5 {
        s.execute("edit.undo", &json!({})).unwrap();
    }
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert!(v["shape"]["fill"].is_null());
}

#[test]
fn text_editing_flow() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "Hello world"})).unwrap();
    s.execute("text.move", &json!({"to": "wordLeft", "extend": true})).unwrap();
    s.execute("format.italic", &json!({})).unwrap();
    s.execute("text.move", &json!({"to": "end"})).unwrap();
    s.execute("text.delete", &json!({"dir": "backward"})).unwrap();
    s.execute("text.insert", &json!({"text": "D\nSecond"})).unwrap();
    let t = s.execute("text.get", &json!({"id": title})).unwrap();
    assert_eq!(t["text"], "Hello worlD\nSecond");
    s.execute("text.exit", &json!({})).unwrap();
    assert!(s.doc().unwrap().selection.text.is_none());
    assert_eq!(s.doc().unwrap().selection.shapes.len(), 1);
}

#[test]
fn smart_quotes_and_autocorrect() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    for c in ["\"", "h", "i", "\"", " ", "t", "e", "h", " "] {
        s.execute("text.insert", &json!({"text": c})).unwrap();
    }
    let t = s.execute("text.get", &json!({"id": title})).unwrap();
    assert_eq!(t["text"], "“hi” the ");
}

#[test]
fn clipboard_copy_paste_shapes_and_slides() {
    let mut s = session();
    s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [10, 10, 50, 50]})).unwrap();
    s.execute("edit.copy", &json!({})).unwrap();
    s.execute("edit.paste", &json!({})).unwrap();
    s.execute("edit.paste", &json!({})).unwrap();
    let n = s.doc().unwrap().current_slide().unwrap().shapes.len();
    assert_eq!(n, 2 + 3);
    s.execute("edit.copy", &json!({"scope": "slides"})).unwrap();
    s.execute("edit.paste", &json!({"scope": "slides"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 2);
    assert!(s.doc().unwrap().doc.validate().is_empty());
}

#[test]
fn group_ungroup_roundtrip_positions() {
    let mut s = session();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 50, 50]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [300, 200, 50, 50]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.select", &json!({"ids": [a, b]})).unwrap();
    let g = s.execute("arrange.group", &json!({})).unwrap()["id"].as_u64().unwrap();
    s.execute("shape.move", &json!({"id": g, "dx": 10})).unwrap();
    s.execute("arrange.ungroup", &json!({"ids": [g]})).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": b})).unwrap();
    assert!((v["box"]["x"].as_f64().unwrap() - 310.0).abs() < 1e-6, "{v}");
}

#[test]
fn align_and_distribute() {
    let mut s = session();
    let mut ids = vec![];
    for (x, y) in [(10.0, 10.0), (200.0, 50.0), (500.0, 90.0)] {
        ids.push(s.execute("shape.insert", &json!({"preset": "rect", "rect": [x, y, 40, 40]})).unwrap()["id"].as_u64().unwrap());
    }
    s.execute("edit.select", &json!({"ids": ids})).unwrap();
    s.execute("arrange.align", &json!({"edge": "top"})).unwrap();
    s.execute("arrange.distribute", &json!({"dir": "horizontal"})).unwrap();
    let ys: Vec<f64> = ids.iter().map(|i| s.execute("shape.inspect", &json!({"id": i})).unwrap()["box"]["y"].as_f64().unwrap()).collect();
    assert!(ys.iter().all(|y| (*y - 10.0).abs() < 1e-6));
    let x1 = s.execute("shape.inspect", &json!({"id": ids[1]})).unwrap()["box"]["x"].as_f64().unwrap();
    assert!((x1 - 255.0).abs() < 1e-6, "{x1}");
}

#[test]
fn pointer_draw_move_resize() {
    let mut s = session();
    s.set_tool(ToolKind::Shape { preset: "rect".into() });
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 };
    s.pointer(ev(PointerKind::Down, 100.0, 100.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 200.0, 180.0)).unwrap();
    let r = s.pointer(ev(PointerKind::Up, 200.0, 180.0)).unwrap();
    let id = r["id"].as_u64().unwrap();
    assert_eq!(s.tool.kind, ToolKind::Select);
    // Move by dragging the middle.
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    s.pointer(ev(PointerKind::Down, 150.0, 140.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 170.0, 150.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 170.0, 150.0)).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert_eq!(v["box"]["x"], 120.0);
    assert_eq!(v["box"]["y"], 110.0);
    // One undo step per gesture.
    s.execute("edit.undo", &json!({})).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert_eq!(v["box"]["x"], 100.0);
    // Resize from the SE handle.
    s.execute("edit.select", &json!({"ids": [id]})).unwrap();
    s.pointer(ev(PointerKind::Down, 200.0, 180.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 250.0, 200.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 250.0, 200.0)).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert_eq!(v["box"]["w"], 150.0);
}

#[test]
fn click_into_placeholder_starts_editing() {
    let mut s = session();
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 };
    s.pointer(ev(PointerKind::Down, 480.0, 200.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 480.0, 200.0)).unwrap();
    assert!(s.doc().unwrap().selection.text.is_some());
    s.execute("text.insert", &json!({"text": "Typed"})).unwrap();
    assert_eq!(s.doc().unwrap().current_slide().unwrap().title(), "Typed");
}

#[test]
fn sample_deck_builds_and_renders() {
    let mut s = Session::new();
    sample::open_sample(&mut s).unwrap();
    let st = s.doc().unwrap();
    assert!(st.doc.slides.len() >= 8);
    assert!(st.doc.validate().is_empty());
    assert!(!st.is_dirty());
    for i in 0..st.doc.slides.len() {
        let img = deckcraft_render::render_slide(&st.doc, i, &deckcraft_render::RenderOpts { scale: 0.2, ..Default::default() });
        assert_eq!(img.width, 192);
    }
    let v = s.execute("document.inspect", &json!({})).unwrap();
    assert!(v["slides"].as_array().unwrap().len() >= 8);
}

#[test]
fn save_and_reopen_native() {
    let mut s = Session::new();
    sample::open_sample(&mut s).unwrap();
    let bytes = s.execute("file.saveBytes", &json!({})).unwrap()["data"].as_str().unwrap().to_string();
    let n = s.doc().unwrap().doc.slides.len();
    s.execute("file.openBytes", &json!({"name": "x.deckcraft", "data": bytes})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), n);
    assert_eq!(s.documents().len(), 2);
}

#[test]
fn every_command_survives_empty_and_hostile_params() {
    let mut s = Session::new();
    sample::open_sample(&mut s).unwrap();
    s.execute("edit.select", &json!({"ids": [s.doc().unwrap().current_slide().unwrap().shapes[0].id.0]})).unwrap();
    let ids: Vec<&str> = command_specs()
        .iter()
        .map(|c| c.id)
        .filter(|id| !matches!(*id, "file.close" | "file.open" | "file.save" | "file.saveAs" | "file.export" | "file.saveTemplate"))
        .collect();
    for p in [
        json!({}),
        json!(null),
        json!([1, 2]),
        json!({"index": 99999, "ids": [999999], "id": -1, "text": "x", "rect": "bad", "color": 5, "size": 1e308}),
    ] {
        for id in &ids {
            let _ = s.execute(id, &p);
        }
    }
    assert!(s.active().is_some());
    for d in s.documents() {
        assert!(d.doc.validate().is_empty(), "{:?}", d.doc.validate());
    }
}

#[test]
fn base64_roundtrip() {
    for data in [vec![], vec![0u8], vec![1, 2], vec![1, 2, 3], (0..=255u8).collect::<Vec<_>>()] {
        assert_eq!(cmd::base64_decode(&cmd::base64_encode(&data)).unwrap(), data);
    }
    assert!(cmd::base64_decode("!!").is_none());
}

#[test]
fn layout_change_keeps_text() {
    let mut s = session();
    s.execute("slide.new", &json!({"layout": "titleAndContent", "title": "T", "body": "B"})).unwrap();
    s.execute("slide.layout", &json!({"layout": "twoContent"})).unwrap();
    let v = s.execute("slide.inspect", &json!({})).unwrap();
    let texts: Vec<String> = v["shapes"].as_array().unwrap().iter().filter_map(|x| x["text"].as_str().map(String::from)).collect();
    assert!(texts.contains(&"T".to_string()) && texts.contains(&"B".to_string()), "{v}");
    assert_eq!(v["layout"], "Two Content");
}

#[test]
fn themes_and_slide_size() {
    let mut s = session();
    s.execute("design.theme", &json!({"name": "Ember"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.masters[0].theme.name, "Ember");
    s.execute("design.slideSize", &json!({"preset": "standard"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slide_size.width, 720.0);
    s.execute("design.headerFooter", &json!({"slideNumber": true, "footer": true, "footerText": "Hello"})).unwrap();
    let v = s.execute("slide.inspect", &json!({})).unwrap();
    assert!(v["shapes"].as_array().unwrap().iter().any(|x| x["placeholder"] == "sldNum"));
}

fn line_ends(s: &Session, id: u64) -> (deckcraft_geom::Point, deckcraft_geom::Point) {
    let st = s.doc().unwrap();
    let sh = st.shape(deckcraft_model::ShapeId(id as u32)).unwrap();
    connect::endpoints(&cmd::xfrm_of(&st.doc, &st.selection, sh))
}

#[test]
fn connectors_follow_glued_shapes() {
    let mut s = session();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [400, 300, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let r = s.execute("shape.connect", &json!({"from": a, "to": b, "preset": "bentConnector3"})).unwrap();
    let c = r["id"].as_u64().unwrap();
    let (p0, p1) = line_ends(&s, c);
    // Closest pair: right side of A → left side of B... or bottom/top; either way on the boxes.
    let sites_a: Vec<Vec<f64>> = serde_json::from_value(s.execute("shape.sites", &json!({"id": a})).unwrap()).unwrap();
    assert!(sites_a.iter().any(|q| (q[0] - p0.x).abs() < 1e-6 && (q[1] - p0.y).abs() < 1e-6));
    // Move B: the connector's end follows, its start stays.
    s.execute("edit.select", &json!({"ids": [b]})).unwrap();
    s.execute("shape.move", &json!({"dx": 50, "dy": 20})).unwrap();
    let (q0, q1) = line_ends(&s, c);
    assert!((q0 - p0).hypot() < 1e-6);
    assert!(((q1 - p1) - deckcraft_geom::Vec2::new(50.0, 20.0)).hypot() < 1e-6, "{p1:?} → {q1:?}");
    // Undo puts both back in one step.
    s.execute("edit.undo", &json!({})).unwrap();
    assert!((line_ends(&s, c).1 - p1).hypot() < 1e-6);
    // Deleting a glued shape unglues that end and leaves the line where it was.
    s.execute("edit.select", &json!({"ids": [a]})).unwrap();
    s.execute("edit.delete", &json!({})).unwrap();
    let st = s.doc().unwrap();
    let sh = st.shape(deckcraft_model::ShapeId(c as u32)).unwrap();
    assert!(matches!(sh.kind, deckcraft_model::ShapeKind::Connector { start: None, end: Some(_) }));
}

#[test]
fn drawing_a_line_between_shapes_glues_it() {
    let mut s = session();
    let a = s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [100, 100, 100, 100]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [400, 100, 100, 100]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.deselect", &json!({})).unwrap();
    s.tool.kind = tools::ToolKind::Shape { preset: "straightConnector1".into() };
    let ev = |kind, x: f64, y: f64| tools::PointerEvent { kind, x, y, mods: Default::default(), tol: 4.0 };
    // Right site of the ellipse (200,150) → left site of the rect (400,150), a little off.
    s.pointer(ev(tools::PointerKind::Down, 202.0, 151.0)).unwrap();
    s.pointer(ev(tools::PointerKind::Drag, 300.0, 150.0)).unwrap();
    assert!(!s.tool.sites.is_empty() || s.tool.glue.is_none());
    s.pointer(ev(tools::PointerKind::Drag, 398.0, 149.0)).unwrap();
    assert!(s.tool.glue.is_some());
    s.pointer(ev(tools::PointerKind::Up, 398.0, 149.0)).unwrap();
    let st = s.doc().unwrap();
    let line = st.shapes().iter().find(|x| x.is_line()).unwrap();
    match line.kind {
        deckcraft_model::ShapeKind::Connector { start: Some((x, _)), end: Some((y, _)) } => {
            assert_eq!((x.0 as u64, y.0 as u64), (a, b));
        }
        ref k => panic!("not glued: {k:?}"),
    }
    let id = line.id.0 as u64;
    let (p0, p1) = line_ends(&s, id);
    assert!((p0.x - 200.0).abs() < 1e-6 && (p1.x - 400.0).abs() < 1e-6, "{p0:?} {p1:?}");
}

#[test]
fn freeform_tools_and_command() {
    let mut s = session();
    let before = s.doc().unwrap().shapes().len();
    // Composite command: one undo step.
    let r = s.execute("shape.freeform", &json!({"points": [[100, 100], [200, 120], [150, 220]], "closed": true})).unwrap();
    assert!(r["id"].as_u64().is_some());
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().shapes().len(), before);
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().shapes().len(), before + 1);

    let ev = |kind, x: f64, y: f64| tools::PointerEvent { kind, x, y, mods: Default::default(), tol: 3.0 };
    // Polygon: click, click, click, click on the start → closed, filled.
    s.set_tool(tools::ToolKind::Shape { preset: "freeform".into() });
    for (x, y) in [(300.0, 300.0), (400.0, 300.0), (400.0, 400.0)] {
        s.pointer(ev(tools::PointerKind::Down, x, y)).unwrap();
        s.pointer(ev(tools::PointerKind::Up, x, y)).unwrap();
    }
    assert!(s.tool.drawing_freeform());
    s.pointer(ev(tools::PointerKind::Down, 301.0, 301.0)).unwrap();
    assert!(!s.tool.drawing_freeform());
    let st = s.doc().unwrap();
    let sh = st.shapes().last().unwrap();
    match &sh.geom {
        deckcraft_model::Geom::Custom { paths } => assert!(paths[0].d.ends_with('Z') && paths[0].d.matches(" L ").count() == 2, "{}", paths[0].d),
        g => panic!("{g:?}"),
    }
    let x = sh.xfrm.unwrap();
    assert_eq!((x.x, x.y, x.w, x.h), (300.0, 300.0, 100.0, 100.0));

    // Scribble: one drag, open path.
    s.set_tool(tools::ToolKind::Shape { preset: "scribble".into() });
    s.pointer(ev(tools::PointerKind::Down, 10.0, 10.0)).unwrap();
    for i in 1..20 {
        s.pointer(ev(tools::PointerKind::Drag, 10.0 + i as f64 * 5.0, 10.0 + (i as f64).sin() * 10.0)).unwrap();
    }
    let r = s.pointer(ev(tools::PointerKind::Up, 110.0, 10.0)).unwrap();
    assert!(r["id"].as_u64().is_some());
    assert_eq!(s.tool.kind, tools::ToolKind::Select);

    // Curve: clicks then Enter → smooth open path.
    s.set_tool(tools::ToolKind::Shape { preset: "curve".into() });
    for (x, y) in [(500.0, 100.0), (550.0, 50.0), (600.0, 100.0), (650.0, 50.0)] {
        s.pointer(ev(tools::PointerKind::Down, x, y)).unwrap();
        s.pointer(ev(tools::PointerKind::Up, x, y)).unwrap();
    }
    let r = s.key("Enter", Default::default()).unwrap();
    let id = deckcraft_model::ShapeId(r["id"].as_u64().unwrap() as u32);
    let st = s.doc().unwrap();
    match &st.shape(id).unwrap().geom {
        deckcraft_model::Geom::Custom { paths } => assert_eq!(paths[0].d.matches(" C ").count(), 3),
        g => panic!("{g:?}"),
    }
}

#[test]
fn double_click_in_table_uses_grown_row_heights() {
    // Rows stored 4 pt tall are drawn tall enough for their text, so 6 pt down is still row 0.
    let mut s = session();
    let data = json!([["a", "b"], ["c", "d"], ["e", "f"]]);
    let id = s.execute("insert.table", &json!({"rows": 3, "cols": 2, "rect": [100.0, 100.0, 400.0, 120.0], "data": data})).unwrap()["id"].clone();
    for row in 0..3 {
        s.execute("table.rowHeight", &json!({"id": id, "row": row, "height": 4.0})).unwrap();
    }
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 };
    s.pointer(ev(PointerKind::DoubleClick, 120.0, 106.0)).unwrap();
    assert_eq!(s.doc().unwrap().selection.text.as_ref().and_then(|t| t.cell), Some((0, 0)));
}

#[test]
fn grown_table_frame_covers_its_drawn_rows() {
    // Rows stored 4 pt tall are drawn taller: the box, its handles and hit tests follow the drawn rows.
    let mut s = session();
    let data = json!([["a", "b"], ["c", "d"], ["e", "f"]]);
    let id = s.execute("insert.table", &json!({"rows": 3, "cols": 2, "rect": [100.0, 100.0, 400.0, 120.0], "data": data})).unwrap()["id"].clone();
    for row in 0..3 {
        s.execute("table.rowHeight", &json!({"id": id, "row": row, "height": 4.0})).unwrap();
    }
    let st = s.doc().unwrap();
    let sh = st.shapes().iter().find(|sh| json!(sh.id) == id).unwrap().clone();
    let deckcraft_model::ShapeKind::Table(t) = &sh.kind else { panic!("not a table") };
    let drawn: f64 = tools::table_rows(st, t).iter().sum();
    assert!(drawn > 12.0, "rows should grow: {drawn}");
    let x = cmd::xfrm_of(&st.doc, &st.selection, &sh);
    assert!((x.h - drawn).abs() < 1e-6, "frame {} vs drawn {drawn}", x.h);
    let below_stored = deckcraft_geom::Point::new(120.0, 100.0 + drawn - 2.0);
    assert!(matches!(s.hit_test(below_stored, 1.0), Some(tools::Hit::Shape { .. })));
}

#[test]
fn right_to_left_paragraphs() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "سنة 2024 (Retrait) فقط"})).unwrap();
    s.execute("format.rtl", &json!({})).unwrap();
    let para = |s: &Session| s.doc().unwrap().current_slide().unwrap().shapes[0].text.as_ref().unwrap().paragraphs[0].props.clone();
    assert_eq!(para(&s).rtl, Some(true));
    // Left (the default) becomes right: the text stays at its start edge.
    assert_eq!(para(&s).align, Some(deckcraft_model::text::Align::Right));
    s.execute("format.align", &json!({"align": "justifyLow"})).unwrap();
    assert_eq!(para(&s).align, Some(deckcraft_model::text::Align::JustLow));
    // Toggling back keeps a justified alignment.
    s.execute("format.rtl", &json!({})).unwrap();
    assert_eq!(para(&s).rtl, Some(false));
    assert_eq!(para(&s).align, Some(deckcraft_model::text::Align::JustLow));
    s.execute("format.rtl", &json!({"on": true})).unwrap();
    s.execute("format.alignLeft", &json!({})).unwrap();
    s.execute("format.rtl", &json!({"on": false})).unwrap();
    assert_eq!(para(&s).align, Some(deckcraft_model::text::Align::Right));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(para(&s).rtl, Some(true));
}

#[test]
fn header_footer_fixed_date_is_the_slides_own_text() {
    use deckcraft_model::text::RunKind;
    let mut s = session();
    let date = |s: &Session| {
        let slide = s.doc().unwrap().current_slide().unwrap();
        slide.shapes.iter().find(|x| x.ph_type() == Some(deckcraft_model::PhType::Date)).and_then(|x| x.text.clone()).unwrap()
    };
    let kind = |s: &Session| date(s).paragraphs[0].runs[0].kind.clone();
    // Automatic: a date field, saved with today's date.
    s.execute("design.headerFooter", &json!({"date": true})).unwrap();
    assert_eq!(kind(&s), RunKind::Field { field: "datetime1".into() });
    assert_eq!(date(&s).text().matches('/').count(), 2, "M/D/YYYY");
    // Fixed: plain text in the slide's own placeholder (other slides' fields aren't affected).
    s.execute("design.headerFooter", &json!({"date": true, "dateText": "Launch day"})).unwrap();
    assert_eq!((kind(&s), date(&s).text()), (RunKind::Text, "Launch day".to_string()));
    // Back to automatic.
    s.execute("design.headerFooter", &json!({"date": true, "dateText": ""})).unwrap();
    assert!(matches!(kind(&s), RunKind::Field { .. }));
    // A new date field is saved with the date it shows.
    s.execute("text.edit", &json!({"id": s.doc().unwrap().current_slide().unwrap().shapes[0].id.0})).unwrap();
    s.execute("text.insert", &json!({"text": "Updated "})).unwrap();
    s.execute("insert.dateTime", &json!({"format": "datetime10"})).unwrap();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].text.clone().unwrap();
    let run = title.paragraphs[0].runs.iter().find(|r| matches!(r.kind, RunKind::Field { .. })).cloned().unwrap();
    assert!(run.text.contains(':'), "H:mm {run:?}");
}

/// (text, linked?) of each run in the first paragraph of the current slide's title.
fn title_runs(s: &Session) -> Vec<(String, bool)> {
    let sh = &s.doc().unwrap().current_slide().unwrap().shapes[0];
    let p = &sh.text.as_ref().unwrap().paragraphs[0];
    p.runs.iter().map(|r| (r.text.clone(), r.props.link.is_some())).collect()
}

#[test]
fn hyperlink_with_a_bare_caret_inserts_the_address() {
    // #31: with the caret at the end of the text, Insert Hyperlink used to format an empty range
    // and change nothing.
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "See "})).unwrap();
    s.execute("insert.hyperlink", &json!({"url": "https://example.org"})).unwrap();
    s.execute("text.insert", &json!({"text": " now"})).unwrap();
    assert_eq!(
        title_runs(&s),
        vec![("See ".into(), false), ("https://example.org".into(), true), (" now".into(), false)],
        "the address is inserted as a link, and typing after it doesn't extend it"
    );
}

#[test]
fn hyperlink_display_text_and_targets() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("insert.hyperlink", &json!({"url": "mailto:a@b.org"})).unwrap();
    assert_eq!(title_runs(&s), vec![("a@b.org".into(), true)], "email links show the address without mailto:");
    s.execute("text.insert", &json!({"text": " "})).unwrap();
    s.execute("insert.hyperlink", &json!({"slide": 0, "text": "start"})).unwrap();
    assert_eq!(title_runs(&s), vec![("a@b.org".into(), true), (" ".into(), false), ("start".into(), true)]);
}

#[test]
fn hyperlink_on_a_word_or_shape_links_it() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "Hello world"})).unwrap();
    s.execute("text.move", &json!({"to": "left"})).unwrap();
    s.execute("insert.hyperlink", &json!({"url": "https://example.org"})).unwrap();
    assert_eq!(title_runs(&s), vec![("Hello ".into(), false), ("world".into(), true)], "a caret inside a word links the word");
    s.execute("text.exit", &json!({})).unwrap();
    s.execute("insert.hyperlink", &json!({"url": "https://example.org"})).unwrap();
    assert!(s.doc().unwrap().current_slide().unwrap().shapes[0].click.is_some(), "a selected shape gets the link as its click action");
}

/// A .pptx (base64) whose first slide's title is set in `font`.
fn deck_in_font(font: &str) -> String {
    let mut p = deckcraft_model::Presentation::default();
    let slide = std::sync::Arc::make_mut(&mut p.slides[0]);
    let mut body = deckcraft_model::text::TextBody::from_text("Hello");
    body.paragraphs[0].runs[0].props.font = Some(font.into());
    slide.shapes[0].text = Some(body);
    crate::cmd::base64_encode(&crate::cmd::file::save_bytes(&p, "pptx").unwrap())
}

#[test]
fn opening_a_file_lists_the_fonts_it_uses_that_are_missing() {
    let mut s = Session::with_new();
    let r = s.execute("file.openBytes", &json!({"name": "a.pptx", "data": deck_in_font("Qqq Nonexistent Sans")})).unwrap();
    assert_eq!(r["missingFonts"], json!(["Qqq Nonexistent Sans"]));
    assert_eq!(s.doc().unwrap().missing_fonts, vec!["Qqq Nonexistent Sans".to_string()]);
    // A font that is there (bundled or installed) isn't listed; new documents list nothing.
    let have = deckcraft_fonts::FontDb::global().face(deckcraft_fonts::FALLBACK_FAMILY, "Regular").family.clone();
    let r = s.execute("file.openBytes", &json!({"name": "b.pptx", "data": deck_in_font(&have)})).unwrap();
    assert_eq!(r["missingFonts"], json!([]));
    s.execute("file.new", &json!({})).unwrap();
    assert!(s.doc().unwrap().missing_fonts.is_empty());
}

#[test]
fn clicks_in_flipped_shapes_hit_the_text_where_it_is_drawn() {
    // Flips don't mirror text, so a click lands on the character drawn under it.
    let mut s = session();
    let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 400, 100], "text": "Hello flipped world"})).unwrap();
    let id = deckcraft_model::ShapeId(r["id"].as_u64().unwrap() as u32);
    let pos = |s: &Session, x: f64, y: f64| {
        let st = s.doc().unwrap();
        tools::text_pos(st, st.shape(id).unwrap(), deckcraft_geom::Point::new(x, y))
    };
    let xs = [240.0, 300.0, 360.0];
    let plain = xs.map(|x| pos(&s, x, 150.0));
    assert!(plain[0].1 < plain[1].1 && plain[1].1 < plain[2].1, "{plain:?}");
    s.execute("shape.flip", &json!({"axis": "horizontal"})).unwrap();
    assert_eq!(xs.map(|x| pos(&s, x, 150.0)), plain);
    // flipV turns the text 180° about the box centre (300, 150).
    s.execute("shape.flip", &json!({"axis": "horizontal"})).unwrap();
    s.execute("shape.flip", &json!({"axis": "vertical"})).unwrap();
    assert_eq!(xs.map(|x| pos(&s, 600.0 - x, 150.0)), plain);
}

#[test]
fn inserted_hyperlinks_look_like_links() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "Read the docs"})).unwrap();
    s.execute("text.move", &json!({"to": "wordLeft", "extend": true})).unwrap();
    assert_eq!(s.execute("format.state", &json!({})).unwrap()["underline"], false);
    s.execute("insert.hyperlink", &json!({"url": "https://example.org"})).unwrap();
    // Underlined (and hlink-coloured: same resolution as files) without touching the run's own props.
    assert_eq!(s.execute("format.state", &json!({})).unwrap()["underline"], true);
    let body = s.doc().unwrap().current_slide().unwrap().shapes[0].text.clone().unwrap();
    let run = body.paragraphs[0].runs.iter().find(|r| r.props.link.is_some()).unwrap();
    assert!(run.props.fill.is_none() && run.props.underline.is_none(), "{run:?}");
    // Underline off is the run's own u="none", as in PowerPoint.
    s.execute("format.underline", &json!({})).unwrap();
    assert_eq!(s.execute("format.state", &json!({})).unwrap()["underline"], false);
}

/// #270: shapes off the slide (in the editor's pasteboard) are hit, selected, moved and marqueed
/// like any other; one can be dragged back onto the slide or out of it.
#[test]
fn shapes_off_the_slide_can_be_selected_and_moved() {
    let mut s = session();
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    let off = s.execute("shape.insert", &json!({"preset": "rect", "rect": [1000.0, 100.0, 120.0, 80.0]})).unwrap()["id"].as_u64().unwrap();
    let half = s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [-60.0, 300.0, 120.0, 80.0]})).unwrap()["id"].as_u64().unwrap();
    // Inserting off the slide keeps the box where it was asked for.
    assert_eq!(s.execute("shape.inspect", &json!({"id": off})).unwrap()["box"]["x"], 1000.0);
    s.execute("edit.select", &json!({"ids": []})).unwrap();
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 };
    // Hover and click wholly off the slide.
    assert!(matches!(s.hit_test(deckcraft_geom::Point::new(1060.0, 140.0), 3.0), Some(tools::Hit::Shape { id, .. }) if id.0 as u64 == off));
    s.pointer(ev(PointerKind::Down, 1060.0, 140.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 1060.0, 140.0)).unwrap();
    assert_eq!(s.doc().unwrap().selection.shapes.iter().map(|i| i.0 as u64).collect::<Vec<_>>(), vec![off]);
    // Drag it further off, then back onto the slide.
    s.pointer(ev(PointerKind::Down, 1060.0, 140.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 1110.0, 160.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 1110.0, 160.0)).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": off})).unwrap();
    assert_eq!((v["box"]["x"].as_f64(), v["box"]["y"].as_f64()), (Some(1050.0), Some(120.0)));
    s.pointer(ev(PointerKind::Down, 1110.0, 160.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 500.0, 160.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 500.0, 160.0)).unwrap();
    assert_eq!(s.execute("shape.inspect", &json!({"id": off})).unwrap()["box"]["x"], 440.0);
    // The off-slide half of a straddling shape is hit too.
    s.pointer(ev(PointerKind::Down, -30.0, 340.0)).unwrap();
    s.pointer(ev(PointerKind::Up, -30.0, 340.0)).unwrap();
    assert_eq!(s.doc().unwrap().selection.shapes.iter().map(|i| i.0 as u64).collect::<Vec<_>>(), vec![half]);
    // A marquee started on the pasteboard selects what it encloses there.
    s.pointer(ev(PointerKind::Down, -100.0, 250.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 80.0, 420.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 80.0, 420.0)).unwrap();
    assert_eq!(s.doc().unwrap().selection.shapes.iter().map(|i| i.0 as u64).collect::<Vec<_>>(), vec![half]);
}

fn title_text(s: &Session) -> String {
    let sh = &s.doc().unwrap().current_slide().unwrap().shapes[0];
    sh.text.as_ref().map(|t| t.paragraphs.iter().map(|p| p.text()).collect::<Vec<_>>().join("\n")).unwrap_or_default()
}

/// #101: Korean typed through an input method arrives as preedit updates (ㅎ → 하 → 한 → 한ㄱ →
/// 한그 → 한글) and a commit. The preedit is shown in place and replaced on each update; the
/// committed text is one undo step, and nothing on the way is an undo step of its own.
#[test]
fn ime_composition_replaces_its_preedit_and_commits_as_one_undo_step() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "A "})).unwrap();
    let undo0 = s.doc().unwrap().history.undo.len();
    for pre in ["ㅎ", "하", "한", "한ㄱ", "한그", "한글"] {
        s.compose_text(pre, None).unwrap();
        assert_eq!(title_text(&s), format!("A {pre}"), "the preedit is shown in place");
        let c = s.doc().unwrap().composition.clone().unwrap();
        assert_eq!((c.start, c.len), ((0, 2), pre.chars().count()));
        assert_eq!(s.doc().unwrap().selection.text.as_ref().unwrap().caret, (0, 2 + pre.chars().count()), "caret after the preedit");
        assert_eq!(s.doc().unwrap().history.undo.len(), undo0, "no undo step while composing");
    }
    s.commit_text("한글").unwrap();
    assert_eq!(title_text(&s), "A 한글");
    assert!(s.doc().unwrap().composition.is_none());
    assert_eq!(s.doc().unwrap().history.undo.len(), undo0 + 1, "one undo step for the composition");
    assert_eq!(s.doc().unwrap().history.undo.last().unwrap().label, "Type Text");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(title_text(&s), "A ");
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(title_text(&s), "A 한글");

    // Windows' Korean IME commits a syllable when the next begins: an empty preedit, the commit,
    // then a new preedit.
    s.compose_text("ㄴ", None).unwrap();
    s.compose_text("", None).unwrap();
    s.commit_text("ㄴ").unwrap();
    s.compose_text("ㅁ", None).unwrap();
    assert_eq!(title_text(&s), "A 한글ㄴㅁ");
    s.commit_text("마").unwrap();
    assert_eq!(title_text(&s), "A 한글ㄴ마");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(title_text(&s), "A 한글ㄴ");
}

/// #101: an empty preedit cancels the composition (Escape, or Backspace over its last jamo):
/// the text and the selection it replaced are as they were, with no undo step.
#[test]
fn ime_cancel_restores_the_text_and_selection() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "Hello world"})).unwrap();
    s.execute("text.select", &json!({"anchor": [0, 6], "caret": [0, 11]})).unwrap();
    let undo0 = s.doc().unwrap().history.undo.len();
    let rev = s.doc().unwrap().revision;
    s.compose_text("せかい", Some((0, 3))).unwrap();
    assert_eq!(title_text(&s), "Hello せかい", "the preedit replaces the selection");
    assert_eq!(s.doc().unwrap().composition.as_ref().unwrap().active, Some((0, 3)));
    s.compose_text("世界", Some((0, 9))).unwrap();
    assert_eq!(s.doc().unwrap().composition.as_ref().unwrap().active, Some((0, 2)), "clamped to the preedit");
    s.compose_text("", None).unwrap();
    assert_eq!(title_text(&s), "Hello world");
    let t = s.doc().unwrap().selection.text.clone().unwrap();
    assert_eq!((t.anchor, t.caret), ((0, 6), (0, 11)), "the selection is back");
    assert_eq!(s.doc().unwrap().history.undo.len(), undo0);
    assert!(s.doc().unwrap().revision > rev, "redrawn");
    // A commit of nothing without a composition is nothing.
    s.commit_text("").unwrap();
    assert_eq!(s.doc().unwrap().history.undo.len(), undo0);
    // A commit without a preedit (some input methods send only the result) is one step.
    s.commit_text("世界").unwrap();
    assert_eq!(title_text(&s), "Hello 世界");
    assert_eq!(s.doc().unwrap().history.undo.len(), undo0 + 1);
}

/// #101: anything else settles the composition first: its text stays and is its own undo step.
#[test]
fn ime_composition_is_kept_by_a_key_a_click_or_another_command() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    let undo0 = s.doc().unwrap().history.undo.len();
    s.compose_text("안", None).unwrap();
    s.key("Right", Mods::default()).unwrap();
    assert!(s.doc().unwrap().composition.is_none());
    assert_eq!(title_text(&s), "안");
    assert_eq!(s.doc().unwrap().history.undo.len(), undo0 + 1);

    s.compose_text("녕", None).unwrap();
    s.pointer(PointerEvent { kind: PointerKind::Move, x: 5.0, y: 5.0, mods: Mods::default(), tol: 4.0 }).unwrap();
    assert!(s.doc().unwrap().composition.is_some(), "hovering keeps composing");
    s.pointer(PointerEvent { kind: PointerKind::Down, x: 5.0, y: 5.0, mods: Mods::default(), tol: 4.0 }).unwrap();
    assert!(s.doc().unwrap().composition.is_none(), "a click settles it");
    assert_eq!(title_text(&s), "안녕");

    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.compose_text("하", None).unwrap();
    s.execute("text.get", &json!({"id": title})).unwrap();
    assert!(s.doc().unwrap().composition.is_some(), "a query doesn't");
    s.execute("text.insert", &json!({"text": "!"})).unwrap();
    assert_eq!(title_text(&s), "안녕하!");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(title_text(&s), "안녕하");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(title_text(&s), "안녕");

    // Text changed under the composition (here behind its back): a new one starts, no panic.
    s.compose_text("ㄱ", None).unwrap();
    s.doc_mut().unwrap().composition.as_mut().unwrap().len = 99;
    s.compose_text("가", None).unwrap();
    assert!(title_text(&s).ends_with('가'));
    // Control characters (an Enter sent as text) never split the paragraph.
    s.commit_text("가\n\r\u{b}").unwrap();
    assert_eq!(s.doc().unwrap().current_slide().unwrap().shapes[0].text.as_ref().unwrap().paragraphs.len(), 1);
}

/// #101: the first preedit on a selected shape starts editing it and replaces its text, as typing
/// does; with nothing selected there is nowhere to compose.
#[test]
fn ime_on_a_selected_shape_replaces_its_text() {
    let mut s = session();
    let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 200, 100], "text": "Old"})).unwrap();
    let id = r["id"].as_u64().unwrap();
    s.select(|_, sel| {
        sel.text = None;
        sel.shapes = vec![ShapeId(id as u32)];
    })
    .unwrap();
    assert_eq!(s.compose_text("", None).unwrap()["handled"], false, "an empty preedit doesn't start editing");
    s.compose_text("新", None).unwrap();
    s.commit_text("新しい").unwrap();
    let v = s.execute("text.get", &json!({"id": id})).unwrap();
    assert_eq!(v["text"], "新しい");
    s.select(|_, sel| {
        sel.text = None;
        sel.shapes.clear();
    })
    .unwrap();
    assert_eq!(s.compose_text("x", None).unwrap()["handled"], false);
    assert_eq!(s.commit_text("x").unwrap()["handled"], false);
}

/// #101: the composition is underlined in place, under the composed characters, with a thicker
/// line under the part being converted.
#[test]
fn ime_composition_underlines_follow_the_text() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    assert!(cmd::text::composition_underlines(s.doc().unwrap()).is_empty());
    s.compose_text("にほんご", Some((2, 4))).unwrap();
    let u = cmd::text::composition_underlines(s.doc().unwrap());
    let thin: Vec<_> = u.iter().filter(|x| !x.2).collect();
    let thick: Vec<_> = u.iter().filter(|x| x.2).collect();
    assert_eq!((thin.len(), thick.len()), (1, 1), "{u:?}");
    let sh = s.doc().unwrap().current_slide().unwrap().shapes[0].clone();
    let x = cmd::xfrm_of(&s.doc().unwrap().doc, &s.doc().unwrap().selection, &sh);
    for (a, b, _) in &u {
        assert!(b.x > a.x && (a.y - b.y).abs() < 1e-9, "a horizontal line: {a:?} {b:?}");
        assert!(a.x >= x.x - 1.0 && b.x <= x.x + x.w + 1.0 && a.y > x.y && a.y < x.y + x.h, "inside the title box");
    }
    assert!(thick[0].0.x > thin[0].0.x && (thick[0].1.x - thin[0].1.x).abs() < 1e-6, "the converted part is the end");
    s.commit_text("日本語").unwrap();
    assert!(cmd::text::composition_underlines(s.doc().unwrap()).is_empty());
}

/// File › Info (#76): the active presentation's name, location, counts and properties.
#[test]
fn file_info_reports_the_presentation() {
    let mut s = session();
    s.execute("slide.new", &json!({})).unwrap();
    s.execute("slide.new", &json!({})).unwrap();
    s.execute("slide.go", &json!({"index": 1})).unwrap();
    s.execute("slide.hide", &json!({})).unwrap();
    s.execute("file.properties", &json!({"title": "Quarterly", "author": "Ada"})).unwrap();
    let v = s.execute("file.info", &json!({})).unwrap();
    assert_eq!(v["slides"], 3);
    assert_eq!(v["hiddenSlides"], 1);
    assert_eq!(v["path"], serde_json::Value::Null);
    assert_eq!(v["dirty"], true);
    assert_eq!(v["properties"]["title"], "Quarterly");
    assert_eq!(v["properties"]["author"], "Ada");
    assert!(v["title"].as_str().is_some_and(|t| !t.is_empty()));
    // A query: no undo step, and nothing changes.
    let before = s.doc().unwrap().history.undo.len();
    s.execute("file.info", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().history.undo.len(), before);
    // Saved to disk, it reports where and how big.
    let dir = std::env::temp_dir().join(format!("deckcraft-info-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Info test.pptx").to_string_lossy().to_string();
    s.execute("file.saveAs", &json!({"path": path})).unwrap();
    let v = s.execute("file.info", &json!({})).unwrap();
    assert_eq!(v["path"], path.as_str());
    assert_eq!(v["format"], "pptx");
    assert_eq!(v["dirty"], false);
    assert!(v["bytes"].as_u64().is_some_and(|b| b > 0));
    let _ = std::fs::remove_dir_all(&dir);
    // Nothing open: an error, not a panic.
    let mut empty = Session::new();
    assert!(empty.execute("file.info", &json!({})).is_err());
}

/// The Print page's Slides box (#76): PowerPoint-style ranges, 1-based.
#[test]
fn slide_ranges_parse_like_powerpoints_print_box() {
    use cmd::file::parse_slide_range as r;
    assert_eq!(r("1-3, 5", 6).unwrap(), vec![0, 1, 2, 4]);
    assert_eq!(r("3-1;2", 6).unwrap(), vec![0, 1, 2], "reversed ranges and repeats");
    assert_eq!(r(" 6 ", 6).unwrap(), vec![5]);
    assert!(r("7", 6).is_err(), "past the last slide");
    assert!(r("0", 6).is_err());
    assert!(r("a-b", 6).is_err());
    assert!(r(" , ", 6).is_err());
    assert!(r("1-99999999999999999999999", 6).is_err(), "hostile numbers are refused");
    // file.export takes the same text for a PDF.
    let mut s = session();
    s.execute("slide.new", &json!({})).unwrap();
    let doc = s.doc().unwrap().doc.clone();
    let one = cmd::file::pdf_bytes(&doc, &json!({"slides": "2"})).unwrap();
    let both = cmd::file::pdf_bytes(&doc, &json!({"slides": "1-2"})).unwrap();
    assert!(one.len() < both.len());
    assert!(cmd::file::pdf_bytes(&doc, &json!({"slides": "5"})).is_err());
    let opts = cmd::file::pdf_options(&json!({"slides": [0, 7, 1]}), 2).unwrap();
    assert_eq!(opts.slides, Some(vec![0, 1]), "indices past the end are dropped");
}
