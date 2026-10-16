//! Round trips of decks built in code: model → PPTX → model.

use std::sync::Arc;

use deckcraft_color::{ColorTransform, Rgba, SchemeSlot};
use deckcraft_geom::Xfrm;
use deckcraft_model::anim::{AnimClass, AnimStart, Animation, TextBuild, Transition};
use deckcraft_model::chart::{Chart, ChartType};
use deckcraft_model::style::{
    ColorRef, Dash, Effects, Fill, Glow, Gradient, GradientShape, GradientStop, Line, LineEnd, PatternFill, PictureAdjust, PictureFill, PictureMode,
    Shadow,
};
use deckcraft_model::table::Table;
use deckcraft_model::text::{
    Action, Align, Anchor, AutoFit, BodyProps, Bullet, Caps, Hyperlink, Paragraph, Run, RunKind, RunProps, Spacing, Strike, TextBody,
};
use deckcraft_model::{
    Background, Comment, CustomPath, CustomShow, Geom, InkStroke, MediaClip, Presentation, Section, Shape, ShapeId, ShapeKind, Slide, SlideId,
    defaults,
};

fn png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb(rgb));
    let mut out = std::io::Cursor::new(vec![]);
    image::DynamicImage::ImageRgb8(img).write_to(&mut out, image::ImageFormat::Png).expect("png");
    out.into_inner()
}

fn shape(p: &mut Presentation, name: &str, x: f64, y: f64, w: f64, h: f64) -> Shape {
    Shape { id: ShapeId(p.alloc_id()), name: name.into(), xfrm: Some(Xfrm::new(x, y, w, h)), ..Default::default() }
}

fn new_slide(p: &mut Presentation, title: &str) -> Slide {
    let layout = defaults::layout_of_kind(p, deckcraft_model::LayoutType::TitleOnly).unwrap_or_default();
    let mut s = defaults::new_slide(p, layout);
    if let Some(t) = s.shapes.first_mut() {
        t.text = Some(TextBody::from_text(title));
    }
    s
}

/// A deck exercising most of what the model can hold.
pub fn rich_deck() -> Presentation {
    let mut p = defaults::new_presentation(None);
    p.props.title = "Rich deck".into();
    p.props.author = "DeckCraft tests".into();
    p.props.created = "2025-01-02T03:04:05Z".into();
    let img = p.add_media("image1.png", "image/png", png(40, 30, [200, 40, 40]));
    let img2 = p.add_media("image2.png", "image/png", png(8, 8, [20, 140, 220]));
    let video = p.add_media("clip.mp4", "video/mp4", b"\x00\x00\x00\x18ftypmp42not-a-real-video".to_vec());

    // Slide 1: title slide with notes and formatted text.
    {
        let first = p.slides[0].id;
        let s = p.slide_mut(first).expect("slide");
        s.notes = TextBody::from_text("Speaker notes line 1\nline 2");
        let t = &mut s.shapes[0];
        t.text = Some(TextBody {
            paragraphs: vec![Paragraph {
                runs: vec![
                    Run::with("Bold ", RunProps { bold: Some(true), size: Some(40.0), ..Default::default() }),
                    Run::with(
                        "red italic",
                        RunProps {
                            italic: Some(true),
                            fill: Some(Fill::solid(ColorRef::rgb(Rgba::rgb(255, 0, 0)))),
                            underline: Some("sng".into()),
                            strike: Some(Strike::Single),
                            caps: Some(Caps::Small),
                            spacing: Some(1.5),
                            baseline: Some(0.3),
                            font: Some("Lato".into()),
                            lang: Some("en-GB".into()),
                            ..Default::default()
                        },
                    ),
                    Run { text: String::new(), props: RunProps::default(), kind: RunKind::Break },
                    Run::with(
                        "link",
                        RunProps {
                            link: Some(Hyperlink {
                                action: Action::Url { url: "https://example.org/a?b=1&c=2".into() },
                                tooltip: "tip".into(),
                                highlight_click: false,
                            }),
                            ..Default::default()
                        },
                    ),
                ],
                ..Default::default()
            }],
            ..Default::default()
        });
        s.transition = Some(Transition {
            kind: "fade".into(),
            option: "throughBlack".into(),
            duration_ms: 700,
            advance_after_ms: Some(3000),
            ..Default::default()
        });
    }

    // Slide 2: shapes, fills, lines, effects, custom geometry, group, connector.
    let mut s = new_slide(&mut p, "Shapes");
    let mut a = shape(&mut p, "Gradient Star", 100.0, 120.0, 150.0, 150.0);
    a.geom = Geom::Preset { name: "star5".into(), adj: vec![20000.0] };
    a.fill = Some(Fill::Gradient(Gradient {
        tile: [0.0; 4],
        stops: vec![
            GradientStop { pos: 0.0, color: ColorRef::scheme(SchemeSlot::Accent1) },
            GradientStop { pos: 1.0, color: ColorRef::scheme(SchemeSlot::Accent2).with(ColorTransform::LumMod(75000)) },
        ],
        shape: GradientShape::Linear { angle: 45.0, scaled: false },
        rotate_with_shape: true,
    }));
    a.line = Some(Line { width: Some(3.0), dash: Some(Dash::Preset { name: "dash".into() }), ..Line::solid(ColorRef::rgb(Rgba::rgb(0, 0, 0)), 3.0) });
    a.effects = Some(Effects {
        outer_shadow: Some(Shadow {
            color: ColorRef::rgb(Rgba::BLACK).with(ColorTransform::Alpha(40000)),
            blur: 6.0,
            dist: 3.0,
            dir: 90.0,
            inner: false,
            sx: 1.0,
            sy: 1.0,
            kx: 0.0,
            ky: 0.0,
            align: "b".into(),
            rotate_with_shape: false,
        }),
        glow: Some(Glow { color: ColorRef::scheme(SchemeSlot::Accent6), radius: 8.0 }),
        soft_edge: Some(2.0),
        ..Default::default()
    });
    a.xfrm = Some(Xfrm { rot: 30.0, flip_h: true, ..Xfrm::new(100.0, 120.0, 150.0, 150.0) });
    let mut b = shape(&mut p, "Pattern Box", 300.0, 120.0, 120.0, 80.0);
    b.geom = Geom::Preset { name: "roundRect".into(), adj: vec![30000.0] };
    b.fill =
        Some(Fill::Pattern(PatternFill { preset: "dkDnDiag".into(), fg: ColorRef::scheme(SchemeSlot::Accent3), bg: ColorRef::rgb(Rgba::WHITE) }));
    b.text = Some(TextBody {
        body: BodyProps { anchor: Some(Anchor::Bottom), inset_l: Some(10.0), wrap: Some(false), autofit: Some(AutoFit::Shape), ..Default::default() },
        paragraphs: vec![Paragraph {
            props: deckcraft_model::text::ParaProps {
                align: Some(Align::Right),
                line_spacing: Some(Spacing::Pct(1.5)),
                space_before: Some(Spacing::Pts(6.0)),
                bullet: Some(Bullet::AutoNum { scheme: "romanUcPeriod".into(), start_at: 3 }),
                ..Default::default()
            },
            runs: vec![Run::new("numbered")],
            level: 2,
            ..Default::default()
        }],
        ..Default::default()
    });
    b.click = Some(Hyperlink { action: Action::NextSlide, tooltip: String::new(), highlight_click: true });
    let mut c = shape(&mut p, "Freeform", 450.0, 120.0, 100.0, 100.0);
    c.geom = Geom::Custom {
        paths: vec![CustomPath {
            w: 100000.0,
            h: 100000.0,
            d: "M 0 0 L 100000 0 L 50000 100000 Z".into(),
            fill: deckcraft_geom::preset::FillMode::Norm,
            stroke: true,
        }],
    };
    c.fill = Some(Fill::solid(ColorRef::scheme(SchemeSlot::Accent4)));
    let child1 = Shape { geom: Geom::preset("ellipse"), ..shape(&mut p, "Child 1", 0.0, 0.0, 50.0, 50.0) };
    let child2 = Shape { geom: Geom::preset("rect"), ..shape(&mut p, "Child 2", 60.0, 0.0, 50.0, 50.0) };
    let (c1, c2) = (child1.id, child2.id);
    let mut g = shape(&mut p, "Group", 600.0, 120.0, 220.0, 100.0);
    g.kind = ShapeKind::Group { children: vec![child1, child2], child: Xfrm::new(0.0, 0.0, 110.0, 50.0) };
    let mut cx = shape(&mut p, "Connector", 600.0, 300.0, 200.0, 0.0);
    cx.kind = ShapeKind::Connector { start: Some((c1, 2)), end: Some((c2, 0)) };
    cx.geom = Geom::preset("straightConnector1");
    cx.line = Some(Line {
        tail: Some(LineEnd { kind: "triangle".into(), w: "med".into(), len: "lg".into() }),
        ..Line::solid(ColorRef::scheme(SchemeSlot::Tx1), 2.0)
    });
    let mut tb = shape(&mut p, "Text Box", 100.0, 400.0, 300.0, 40.0);
    tb.text_box = true;
    tb.text = Some(TextBody::from_text("A text box"));
    tb.descr = "alt text".into();
    let mut bgs = shape(&mut p, "Uses background", 450.0, 400.0, 60.0, 60.0);
    bgs.fill = Some(Fill::Background);
    s.shapes.extend([a, b, c, g, cx, tb, bgs]);
    s.background = Some(Background::Fill { fill: Fill::solid(ColorRef::rgb(Rgba::rgb(250, 248, 240))) });
    s.transition = Some(Transition { kind: "morph".into(), option: "words".into(), duration_ms: 2000, ..Default::default() });
    let ids: Vec<ShapeId> = s.shapes.iter().map(|x| x.id).collect();
    s.animations = vec![
        Animation { shape: ids[1], effect: "fly".into(), option: "l".into(), duration_ms: 750, ..Default::default() },
        Animation { shape: ids[2], effect: "wipe".into(), option: "t".into(), start: AnimStart::WithPrevious, delay_ms: 250, ..Default::default() },
        Animation {
            shape: ids[3],
            class: AnimClass::Emphasis,
            effect: "spin".into(),
            start: AnimStart::AfterPrevious,
            duration_ms: 2000,
            ..Default::default()
        },
        Animation { shape: ids[4], class: AnimClass::Exit, effect: "fadeOut".into(), duration_ms: 400, ..Default::default() },
        Animation {
            shape: ids[2],
            class: AnimClass::Path,
            effect: "lines".into(),
            option: "right".into(),
            path: Some("M 0 0 L 0.25 0 E".into()),
            duration_ms: 2000,
            ..Default::default()
        },
        Animation { shape: ids[6], effect: "zoom".into(), option: "in".into(), trigger: Some(ids[3]), duration_ms: 500, ..Default::default() },
    ];
    s.comments = vec![Comment {
        author: "Ada".into(),
        initials: "AL".into(),
        text: "Nice star".into(),
        date: "2025-02-03T04:05:06.000".into(),
        x: 120.0,
        y: 64.0,
        ..Default::default()
    }];
    p.slides.push(Arc::new(s));

    // Slide 3: pictures, media, ink.
    let mut s = new_slide(&mut p, "Pictures");
    let mut pic = shape(&mut p, "Picture", 100.0, 120.0, 200.0, 150.0);
    pic.kind = ShapeKind::Picture {
        fill: PictureFill {
            media: img,
            crop: [0.1, 0.05, 0.2, 0.0],
            mode: PictureMode::Stretch { fill_rect: [0.0; 4] },
            alpha: Some(0.5),
            adjust: PictureAdjust { brightness: 0.2, contrast: -0.1, grayscale: true, ..Default::default() },
            upright: false,
        },
    };
    let mut tiled = shape(&mut p, "Tiled", 350.0, 120.0, 200.0, 150.0);
    tiled.fill = Some(Fill::Picture(PictureFill {
        media: img2,
        mode: PictureMode::Tile { tx: 0.0, ty: 0.0, sx: 0.5, sy: 0.5, flip: "none".into(), align: "tl".into() },
        ..Default::default()
    }));
    let mut vid = shape(&mut p, "Video", 600.0, 120.0, 240.0, 135.0);
    vid.kind = ShapeKind::Media(MediaClip {
        media: video,
        video: true,
        poster: Some(img),
        volume: 1.0,
        trim_start_ms: 500,
        fade_in_ms: 250,
        ..Default::default()
    });
    let mut ink = shape(&mut p, "Ink", 100.0, 350.0, 200.0, 100.0);
    ink.kind = ShapeKind::Ink {
        strokes: vec![InkStroke {
            points: vec![(100.0, 350.0, 0.5), (150.0, 400.0, 0.5), (300.0, 360.0, 0.5)],
            color: Rgba::rgb(0, 0, 255),
            width: 3.0,
            highlighter: false,
        }],
    };
    s.shapes.extend([pic, tiled, vid, ink]);
    s.transition = Some(Transition { kind: "vortex".into(), option: "r".into(), duration_ms: 3000, ..Default::default() });
    s.hidden = true;
    p.slides.push(Arc::new(s));

    // Slide 4: table with merges and a chart per type.
    let mut s = new_slide(&mut p, "Table");
    let mut t = Table::new(3, 3, 450.0, 30.0);
    for r in 0..3 {
        for c in 0..3 {
            if let Some(cell) = t.cell_mut(r, c) {
                cell.text = TextBody::from_text(&format!("r{r}c{c}"));
            }
        }
    }
    t.merge(1, 0, 2, 1);
    t.style = "light2-accent3".into();
    if let Some(cell) = t.cell_mut(0, 2) {
        cell.fill = Some(Fill::solid(ColorRef::rgb(Rgba::rgb(255, 255, 0))));
        cell.margins = Some([5.0, 5.0, 2.0, 2.0]);
        cell.anchor = Some(Anchor::Middle);
        cell.borders[3] = Some(Line::solid(ColorRef::rgb(Rgba::BLACK), 2.0));
    }
    let mut ts = shape(&mut p, "Table 1", 100.0, 120.0, 450.0, 90.0);
    ts.kind = ShapeKind::Table(t);
    s.shapes.push(ts);
    s.transition = Some(Transition {
        kind: "split".into(),
        option: "vertIn".into(),
        duration_ms: 1500,
        advance_on_click: false,
        advance_after_ms: Some(2000),
        ..Default::default()
    });
    p.slides.push(Arc::new(s));

    let kinds = [
        ChartType::Column,
        ChartType::StackedBar,
        ChartType::PercentColumn,
        ChartType::Line,
        ChartType::LineMarkers,
        ChartType::Area,
        ChartType::Pie,
        ChartType::Doughnut,
        ChartType::Scatter,
        ChartType::Radar,
        ChartType::Bubble,
    ];
    let mut s = new_slide(&mut p, "Charts");
    for (i, k) in kinds.iter().enumerate() {
        let mut ch = shape(&mut p, &format!("Chart {i}"), 20.0 + (i % 4) as f64 * 230.0, 100.0 + (i / 4) as f64 * 140.0, 220.0, 130.0);
        let mut chart = Chart::sample(*k);
        chart.title = Some(k.label().to_string());
        ch.kind = ShapeKind::Chart(Box::new(chart));
        s.shapes.push(ch);
    }
    let mut combo = Chart::sample(ChartType::Column);
    combo.kind = ChartType::Combo;
    combo.series[0].kind = Some(ChartType::Column);
    combo.series[1].kind = Some(ChartType::Line);
    combo.series[2].kind = Some(ChartType::Line);
    let mut ch = shape(&mut p, "Combo", 710.0, 380.0, 220.0, 130.0);
    ch.kind = ShapeKind::Chart(Box::new(combo));
    s.shapes.push(ch);
    p.slides.push(Arc::new(s));

    // Slide 6: bulleted content with a by-paragraph build, a slide link and an equation.
    let layout = defaults::layout_of_kind(&p, deckcraft_model::LayoutType::TitleAndContent).unwrap_or_default();
    let mut s = defaults::new_slide(&mut p, layout);
    s.shapes[0].text = Some(TextBody::from_text("Build"));
    s.shapes[1].text = Some(TextBody::from_text("One\nTwo\nThree"));
    let body = s.shapes[1].id;
    s.animations = vec![Animation { shape: body, effect: "fade".into(), text_build: TextBuild::ByParagraph, duration_ms: 500, ..Default::default() }];
    let target = p.slides[1].id;
    let mut link = shape(&mut p, "Go to shapes", 600.0, 450.0, 200.0, 40.0);
    link.text = Some(TextBody::from_text("Back to shapes"));
    link.click = Some(Hyperlink { action: Action::Slide { slide: target }, tooltip: String::new(), highlight_click: false });
    let mut eq = shape(&mut p, "Equation", 100.0, 450.0, 300.0, 40.0);
    eq.text = Some(TextBody {
        paragraphs: vec![Paragraph {
            runs: vec![Run {
                text: "x=1".into(),
                props: RunProps::default(),
                kind: RunKind::Math {
                    omml: r#"<m:oMathPara xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:oMath><m:r><m:t>x=1</m:t></m:r></m:oMath></m:oMathPara>"#.into(),
                },
            }],
            ..Default::default()
        }],
        ..Default::default()
    });
    s.shapes.extend([link, eq]);
    s.transition = Some(Transition { kind: "curtains".into(), duration_ms: 2000, ..Default::default() });
    p.slides.push(Arc::new(s));

    let ids: Vec<SlideId> = p.slides.iter().map(|s| s.id).collect();
    p.sections = vec![Section { name: "Start".into(), slides: ids[..2].to_vec() }, Section { name: "Rest".into(), slides: ids[2..].to_vec() }];
    p.custom_shows = vec![CustomShow { name: "Short".into(), slides: vec![ids[0], ids[3]] }];
    p.first_slide_number = 0;
    p
}

fn round(p: &Presentation) -> Presentation {
    let bytes = deckcraft_pptx::export(p).expect("export");
    deckcraft_pptx::import(&bytes).expect("import")
}

fn by_name<'a>(s: &'a Slide, name: &str) -> &'a Shape {
    let mut found = None;
    deckcraft_model::walk(&s.shapes, &mut |x, _| {
        if found.is_none() && x.name == name {
            found = Some(x);
        }
    });
    found.unwrap_or_else(|| panic!("no shape {name}"))
}

#[test]
fn rich_deck_writes_to_dir() {
    let p = rich_deck();
    let bytes = deckcraft_pptx::export(&p).expect("export");
    if let Ok(dir) = std::env::var("DECKCRAFT_PPTX_OUT") {
        std::fs::write(format!("{dir}/rich.pptx"), &bytes).expect("write");
    }
    assert!(bytes.len() > 1000);
}

#[test]
fn deck_level_round_trip() {
    let p = rich_deck();
    let q = round(&p);
    assert_eq!(q.slides.len(), p.slides.len());
    assert!(q.validate().is_empty(), "{:?}", q.validate());
    assert_eq!(q.slide_size, p.slide_size);
    assert_eq!(q.first_slide_number, 0);
    assert_eq!(q.props.title, "Rich deck");
    assert_eq!(q.props.author, "DeckCraft tests");
    assert_eq!(q.props.created, "2025-01-02T03:04:05Z");
    assert_eq!(
        q.sections.iter().map(|s| (s.name.clone(), s.slides.len())).collect::<Vec<_>>(),
        vec![("Start".to_string(), 2), ("Rest".to_string(), 4)]
    );
    assert_eq!(q.custom_shows.len(), 1);
    assert_eq!(q.custom_shows[0].slides, vec![q.slides[0].id, q.slides[3].id]);
    assert!(q.slides[2].hidden);
    // Themes and layouts.
    assert_eq!(q.masters.len(), 1);
    let (pm, qm) = (&p.masters[0], &q.masters[0]);
    assert_eq!(qm.theme.colors.colors, pm.theme.colors.colors);
    assert_eq!(qm.theme.fonts.major.latin, pm.theme.fonts.major.latin);
    assert_eq!(qm.layouts.len(), pm.layouts.len());
    for (a, b) in pm.layouts.iter().zip(qm.layouts.iter()) {
        assert_eq!((a.name.as_str(), a.kind, a.shapes.len()), (b.name.as_str(), b.kind, b.shapes.len()));
    }
    assert_eq!(qm.title_style, pm.title_style);
    assert_eq!(qm.body_style, pm.body_style);
    assert_eq!(q.default_text_style, p.default_text_style);
    for (a, b) in p.slides.iter().zip(q.slides.iter()) {
        let (la, lb) = (p.layout(a.layout).map(|x| &x.1.name), q.layout(b.layout).map(|x| &x.1.name));
        assert_eq!(la, lb);
    }
}

#[test]
fn text_and_formatting_round_trip() {
    let p = rich_deck();
    let q = round(&p);
    assert_eq!(q.slides[0].notes_text(), "Speaker notes line 1\nline 2");
    let (a, b) = (&p.slides[0].shapes[0], &q.slides[0].shapes[0]);
    assert_eq!(a.text, b.text);
    assert!(b.xfrm.is_none(), "placeholder without xfrm inherits");
    let sa = by_name(&p.slides[1], "Pattern Box");
    let sb = by_name(&q.slides[1], "Pattern Box");
    assert_eq!(sa.text, sb.text);
    assert_eq!(sa.click, sb.click);
    assert!(by_name(&q.slides[1], "Text Box").text_box);
    assert_eq!(by_name(&q.slides[1], "Text Box").descr, "alt text");
    let la = by_name(&p.slides[5], "Go to shapes");
    let lb = by_name(&q.slides[5], "Go to shapes");
    assert_eq!(lb.click.as_ref().map(|h| h.action.clone()), Some(Action::Slide { slide: q.slides[1].id }));
    assert!(la.click.is_some());
    let eq = by_name(&q.slides[5], "Equation");
    let run = &eq.text.as_ref().expect("text").paragraphs[0].runs[0];
    assert!(matches!(&run.kind, RunKind::Math { omml } if omml.contains("oMath")), "{run:?}");
    assert_eq!(run.text, "x=1");
}

#[test]
fn shapes_fills_lines_effects_round_trip() {
    let p = rich_deck();
    let q = round(&p);
    for name in ["Gradient Star", "Pattern Box", "Freeform", "Uses background"] {
        let (a, b) = (by_name(&p.slides[1], name), by_name(&q.slides[1], name));
        assert_eq!(a.geom, b.geom, "{name}");
        assert_eq!(a.fill, b.fill, "{name}");
        assert_eq!(a.line, b.line, "{name}");
        assert_eq!(a.effects, b.effects, "{name}");
        assert_eq!(a.xfrm, b.xfrm, "{name}");
    }
    let g = by_name(&q.slides[1], "Group");
    assert!(matches!(&g.kind, ShapeKind::Group { children, child } if children.len() == 2 && child.w == 110.0));
    let c = by_name(&q.slides[1], "Connector");
    let (c1, c2) = (by_name(&q.slides[1], "Child 1").id, by_name(&q.slides[1], "Child 2").id);
    assert!(matches!(c.kind, ShapeKind::Connector { start: Some((s, 2)), end: Some((e, 0)) } if s == c1 && e == c2), "{:?}", c.kind);
    assert_eq!(c.line, by_name(&p.slides[1], "Connector").line);
    // Ids are kept.
    assert_eq!(by_name(&q.slides[1], "Group").id, by_name(&p.slides[1], "Group").id);
}

#[test]
fn pictures_media_and_ink_round_trip() {
    let p = rich_deck();
    let q = round(&p);
    let (a, b) = (by_name(&p.slides[2], "Picture"), by_name(&q.slides[2], "Picture"));
    let (ShapeKind::Picture { fill: fa }, ShapeKind::Picture { fill: fb }) = (&a.kind, &b.kind) else { panic!("not pictures") };
    assert_eq!(fa.crop, fb.crop);
    assert_eq!(fa.alpha, fb.alpha);
    assert_eq!(fa.adjust, fb.adjust);
    assert_eq!(p.media(fa.media).map(|m| m.data.clone()), q.media(fb.media).map(|m| m.data.clone()));
    let t = by_name(&q.slides[2], "Tiled");
    assert!(matches!(&t.fill, Some(Fill::Picture(pf)) if matches!(pf.mode, PictureMode::Tile { sx, .. } if sx == 0.5)));
    let v = by_name(&q.slides[2], "Video");
    let ShapeKind::Media(m) = &v.kind else { panic!("not media: {:?}", v.kind) };
    assert!(m.video);
    assert_eq!((m.trim_start_ms, m.fade_in_ms), (500, 250));
    assert!(q.media(m.media).is_some_and(|x| x.data.starts_with(b"\x00\x00\x00\x18ftyp")));
    assert!(m.poster.is_some());
    // Ink comes back as freeform lines in a group.
    let ink = by_name(&q.slides[2], "Ink");
    assert!(matches!(&ink.kind, ShapeKind::Group { children, .. } if children.len() == 1 && matches!(children[0].geom, Geom::Custom { .. })));
}

#[test]
fn tables_round_trip() {
    let p = rich_deck();
    let q = round(&p);
    let (ShapeKind::Table(a), ShapeKind::Table(b)) = (&by_name(&p.slides[3], "Table 1").kind, &by_name(&q.slides[3], "Table 1").kind) else {
        panic!()
    };
    // A cell always has at least one paragraph in the file; compare without empty ones.
    let norm = |t: &Table| {
        let mut t = t.clone();
        for r in &mut t.rows {
            for c in &mut r.cells {
                c.text.paragraphs.retain(|p| !p.is_empty());
            }
        }
        t
    };
    assert_eq!(norm(a), norm(b));
}

#[test]
fn charts_round_trip() {
    let p = rich_deck();
    let q = round(&p);
    for (sa, sb) in p.slides[4].shapes.iter().zip(q.slides[4].shapes.iter()) {
        let (ShapeKind::Chart(a), ShapeKind::Chart(b)) = (&sa.kind, &sb.kind) else { continue };
        assert_eq!(a.kind, b.kind, "{}", sa.name);
        assert_eq!(a.title, b.title);
        assert_eq!(a.series.len(), b.series.len());
        for (x, y) in a.series.iter().zip(b.series.iter()) {
            assert_eq!(x.name, y.name);
            assert_eq!(x.values, y.values, "{}", sa.name);
        }
        if !matches!(a.kind, ChartType::Scatter | ChartType::Bubble) {
            assert_eq!(a.categories, b.categories);
        }
        assert!(b.raw.is_some());
    }
    // Second export reuses the kept chart XML unchanged.
    let r = round(&q);
    for (sa, sb) in q.slides[4].shapes.iter().zip(r.slides[4].shapes.iter()) {
        if let (ShapeKind::Chart(a), ShapeKind::Chart(b)) = (&sa.kind, &sb.kind) {
            let (mut a, mut b) = (a.clone(), b.clone());
            a.raw = None;
            b.raw = None;
            assert_eq!(a, b);
        }
    }
}

#[test]
fn transitions_and_animations_round_trip() {
    let p = rich_deck();
    let q = round(&p);
    for (a, b) in p.slides.iter().zip(q.slides.iter()) {
        match (&a.transition, &b.transition) {
            (Some(x), Some(y)) => {
                assert_eq!(
                    (&x.kind, &x.option, x.duration_ms, x.advance_on_click, x.advance_after_ms),
                    (&y.kind, &y.option, y.duration_ms, y.advance_on_click, y.advance_after_ms)
                );
            }
            (None, None) => {}
            other => panic!("{other:?}"),
        }
        assert_eq!(a.animations.len(), b.animations.len());
        for (x, y) in a.animations.iter().zip(b.animations.iter()) {
            assert_eq!(
                (&x.effect, &x.option, x.class, x.start, x.duration_ms, x.delay_ms, x.text_build),
                (&y.effect, &y.option, y.class, y.start, y.duration_ms, y.delay_ms, y.text_build)
            );
            assert_eq!(x.shape, y.shape);
            assert_eq!(x.trigger, y.trigger);
            if x.class == AnimClass::Path {
                assert_eq!(x.path, y.path);
            }
        }
    }
}

#[test]
fn comments_round_trip() {
    let p = rich_deck();
    let q = round(&p);
    let c = &q.slides[1].comments;
    assert_eq!(c.len(), 1);
    assert_eq!((c[0].author.as_str(), c[0].initials.as_str(), c[0].text.as_str()), ("Ada", "AL", "Nice star"));
    assert_eq!((c[0].x, c[0].y), (120.0, 64.0));
}

#[test]
fn every_transition_kind_survives() {
    let mut p = defaults::new_presentation(None);
    let base = p.slides[0].as_ref().clone();
    p.slides.clear();
    for (kind, ..) in deckcraft_model::anim::TRANSITIONS {
        let mut s = base.clone();
        s.id = SlideId(p.alloc_id());
        s.transition = Some(Transition { kind: kind.to_string(), duration_ms: 1200, ..Default::default() });
        p.slides.push(Arc::new(s));
    }
    let q = round(&p);
    for (a, b) in p.slides.iter().zip(q.slides.iter()) {
        let k = a.transition.as_ref().map(|t| t.kind.clone()).unwrap_or_default();
        if k == "none" {
            continue;
        }
        assert_eq!(b.transition.as_ref().map(|t| t.kind.clone()), Some(k.clone()));
        assert_eq!(b.transition.as_ref().map(|t| t.duration_ms), Some(1200), "{k}");
    }
}

#[test]
fn every_animation_effect_survives() {
    let mut p = defaults::new_presentation(None);
    let mut s = p.slides[0].as_ref().clone();
    let mut sh = shape(&mut p, "Target", 100.0, 100.0, 100.0, 100.0);
    sh.text = Some(TextBody::from_text("hi"));
    let id = sh.id;
    s.shapes.push(sh);
    for (effect, _, class, _, dur, opts) in deckcraft_model::anim::ANIMATIONS {
        s.animations.push(Animation {
            shape: id,
            class: *class,
            effect: effect.to_string(),
            option: opts.first().map(|o| o.to_string()).unwrap_or_default(),
            duration_ms: (*dur).max(1),
            ..Default::default()
        });
    }
    p.slides[0] = Arc::new(s);
    let q = round(&p);
    let (a, b) = (&p.slides[0].animations, &q.slides[0].animations);
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        // Effects sharing a preset number come back as the first effect with that number.
        let same_preset = deckcraft_model::anim::animation_info(&x.effect, x.class).map(|i| i.3)
            == deckcraft_model::anim::animation_info(&y.effect, y.class).map(|i| i.3);
        assert!(x.effect == y.effect || same_preset, "{} vs {}", x.effect, y.effect);
        assert_eq!(x.class, y.class);
    }
}

/// Writes bisection variants of the rich deck to `DECKCRAFT_PPTX_BISECT` (manual checks).
#[test]
fn bisect_variants() {
    let Ok(dir) = std::env::var("DECKCRAFT_PPTX_BISECT") else { return };
    let p = rich_deck();
    let n = p.slides.len();
    for i in 0..n {
        let mut q = p.clone();
        q.slides = vec![p.slides[i].clone()];
        q.sections.clear();
        q.custom_shows.clear();
        std::fs::write(format!("{dir}/bis-slide{i}.pptx"), deckcraft_pptx::export(&q).expect("export")).expect("write");
    }
    let mut q = p.clone();
    q.sections.clear();
    q.custom_shows.clear();
    q.first_slide_number = 1;
    for s in q.slides.iter_mut() {
        Arc::make_mut(s).comments.clear();
    }
    std::fs::write(format!("{dir}/bis-nodeck.pptx"), deckcraft_pptx::export(&q).expect("export")).expect("write");
}

/// Finer bisection of one slide (`DECKCRAFT_PPTX_BISECT_SLIDE=<index>`): one variant per removed shape,
/// plus no animations / no transition.
#[test]
fn bisect_slide_variants() {
    let (Ok(dir), Ok(idx)) = (std::env::var("DECKCRAFT_PPTX_BISECT"), std::env::var("DECKCRAFT_PPTX_BISECT_SLIDE")) else { return };
    let p = rich_deck();
    let i: usize = idx.parse().expect("index");
    let base = p.slides[i].as_ref().clone();
    let one = |s: Slide| {
        let mut q = p.clone();
        q.slides = vec![Arc::new(s)];
        q.sections.clear();
        q.custom_shows.clear();
        deckcraft_pptx::export(&q).expect("export")
    };
    let mut s = base.clone();
    s.animations.clear();
    std::fs::write(format!("{dir}/v-noanim.pptx"), one(s)).expect("write");
    let mut s = base.clone();
    s.transition = None;
    std::fs::write(format!("{dir}/v-notrans.pptx"), one(s)).expect("write");
    for k in 0..base.shapes.len() {
        let mut s = base.clone();
        s.animations.clear();
        s.transition = None;
        let keep = s.shapes[k].clone();
        s.shapes.retain(|x| x.id == keep.id);
        std::fs::write(format!("{dir}/v-only{k}.pptx"), one(s)).expect("write");
    }
}

#[test]
fn rtl_paragraphs_script_fonts_and_just_low_round_trip() {
    let mut p = rich_deck();
    let fonts = &mut Arc::make_mut(&mut p.masters[0]).theme.fonts;
    fonts.minor.cs = "Noto Naskh Arabic".into();
    fonts.minor.scripts = vec![
        deckcraft_model::theme::ScriptFont { script: "Arab".into(), typeface: "Noto Naskh Arabic".into() },
        deckcraft_model::theme::ScriptFont { script: "Hebr".into(), typeface: "Noto Sans Hebrew".into() },
    ];
    let slide = Arc::make_mut(&mut p.slides[0]);
    let sh = slide.shapes.iter_mut().find(|s| s.text.is_some()).expect("a text shape");
    let body = sh.text.as_mut().expect("text");
    body.paragraphs = vec![Paragraph { runs: vec![Run::new("سنة 2024 (Retrait) فقط")], ..Default::default() }];
    body.paragraphs[0].props.rtl = Some(true);
    body.paragraphs[0].props.align = Some(Align::JustLow);
    let name = sh.name.clone();
    let q = round(&p);
    assert_eq!(q.masters[0].theme.fonts.minor.scripts, p.masters[0].theme.fonts.minor.scripts);
    assert_eq!(q.masters[0].theme.fonts.minor.script("arab"), Some("Noto Naskh Arabic"));
    let para = &by_name(&q.slides[0], &name).text.as_ref().expect("text").paragraphs[0];
    assert_eq!(para.props.rtl, Some(true));
    assert_eq!(para.props.align, Some(Align::JustLow));
    assert_eq!(para.runs[0].text, "سنة 2024 (Retrait) فقط");
}

#[test]
fn date_fields_keep_their_type_and_saved_text() {
    // An automatic date is a field (its saved text is what readers without a clock show); a fixed
    // date is plain text.
    let mut p = Presentation::default();
    let mut auto = shape(&mut p, "Auto date", 10.0, 10.0, 200.0, 40.0);
    let mut body = TextBody::from_text("");
    body.paragraphs[0].runs =
        vec![Run { text: "March 3, 2025".into(), props: RunProps::default(), kind: RunKind::Field { field: "datetime4".into() } }];
    auto.text = Some(body);
    let mut fixed = shape(&mut p, "Fixed date", 10.0, 60.0, 200.0, 40.0);
    fixed.text = Some(TextBody::from_text("Launch day"));
    Arc::make_mut(&mut p.slides[0]).shapes = vec![auto, fixed];
    let q = round(&p);
    for name in ["Auto date", "Fixed date"] {
        assert_eq!(by_name(&q.slides[0], name).text, by_name(&p.slides[0], name).text, "{name}");
    }
}

#[test]
fn colour_map_overrides_round_trip() {
    let mut p = defaults::new_presentation(None);
    let dark: Vec<(String, String)> =
        [("bg1", "dk1"), ("tx1", "lt1"), ("bg2", "dk2"), ("tx2", "lt2")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    let layout = p.slides[0].layout;
    std::sync::Arc::make_mut(&mut p.masters[0]).layouts.iter_mut().find(|l| l.id == layout).expect("layout").color_map = Some(dark.clone());
    let first = p.slides[0].id;
    p.slide_mut(first).expect("slide").color_map = Some(dark.clone());
    let q = round(&p);
    let get = |m: &Option<Vec<(String, String)>>, k: &str| m.as_ref().and_then(|m| m.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone()));
    let ql = q.masters[0].layouts.iter().find(|l| l.color_map.is_some()).expect("the layout keeps its override");
    assert_eq!(get(&ql.color_map, "bg1").as_deref(), Some("dk1"));
    assert_eq!(get(&q.slides[0].color_map, "tx1").as_deref(), Some("lt1"));
    // Layouts and slides without one keep following the master.
    assert!(q.masters[0].layouts.iter().filter(|l| l.color_map.is_none()).count() >= 1);
}

#[test]
fn a_picture_duotone_round_trips() {
    let mut p = defaults::new_presentation(None);
    let img = p.add_media("image1.png", "image/png", png(4, 4, [128, 128, 128]));
    let mut pic = shape(&mut p, "Duotone", 10.0, 10.0, 100.0, 100.0);
    let mut pf = PictureFill { media: img, ..Default::default() };
    pf.adjust.duotone = Some((ColorRef::scheme(SchemeSlot::Accent1).with(ColorTransform::Shade(30000)), ColorRef::rgb(Rgba::rgb(250, 240, 200))));
    pic.kind = deckcraft_model::ShapeKind::Picture { fill: pf };
    let first = p.slides[0].id;
    p.slide_mut(first).expect("slide").shapes.push(pic);
    let q = round(&p);
    let sh = by_name(&q.slides[0], "Duotone");
    let deckcraft_model::ShapeKind::Picture { fill } = &sh.kind else { panic!("picture") };
    let (d, l) = fill.adjust.duotone.as_ref().expect("duotone kept");
    assert!(matches!(d.base, deckcraft_model::ColorBase::Scheme { slot: SchemeSlot::Accent1 }));
    assert!(matches!(l.base, deckcraft_model::ColorBase::Rgb { rgb } if rgb == Rgba::rgb(250, 240, 200)));
}

#[test]
fn line_break_options_round_trip() {
    let mut p = defaults::new_presentation(None);
    let first = p.slides[0].id;
    let s = p.slide_mut(first).expect("slide");
    let t = s.shapes[0].text.insert(TextBody::from_text("x"));
    t.paragraphs[0].props.latin_line_break = Some(true);
    t.paragraphs[0].props.hanging_punct = Some(false);
    let q = round(&p);
    let props = &q.slides[0].shapes[0].text.as_ref().expect("text").paragraphs[0].props;
    assert_eq!(props.latin_line_break, Some(true));
    assert_eq!(props.hanging_punct, Some(false));
}

#[test]
fn a_table_fill_round_trips() {
    let mut p = defaults::new_presentation(None);
    let mut t = Table::new(2, 2, 200.0, 40.0);
    t.background = Some(Fill::solid(ColorRef::rgb(Rgba::rgb(0, 112, 192))));
    let mut sh = shape(&mut p, "Filled table", 10.0, 10.0, 200.0, 80.0);
    sh.kind = deckcraft_model::ShapeKind::Table(t);
    let first = p.slides[0].id;
    p.slide_mut(first).expect("slide").shapes.push(sh);
    let q = round(&p);
    let deckcraft_model::ShapeKind::Table(t) = &by_name(&q.slides[0], "Filled table").kind else { panic!("table") };
    assert!(
        matches!(&t.background, Some(Fill::Solid { color }) if matches!(color.base, deckcraft_model::ColorBase::Rgb { rgb } if rgb == Rgba::rgb(0, 112, 192)))
    );
}

#[test]
fn wordart_warps_and_their_adjusts_round_trip() {
    let mut p = Presentation::default();
    let cases: [(&str, &str, Vec<f64>); 5] = [
        ("Arch pour", "textArchUpPour", vec![13_500_000.0, 30_000.0]),
        ("Wave", "textWave1", vec![18_000.0, -5_000.0]),
        ("Inflate", "textInflate", vec![30_000.0]),
        ("Arch", "textArchUp", vec![]),
        ("Ring", "textRingInside", vec![75_000.0]),
    ];
    let mut shapes = vec![];
    for (i, (name, warp, adj)) in cases.iter().enumerate() {
        let mut s = shape(&mut p, name, 10.0, 10.0 + 50.0 * i as f64, 300.0, 40.0);
        let mut body = TextBody::from_text("WordArt");
        body.body.warp = Some((*warp).into());
        body.body.warp_adj = adj.clone();
        s.text = Some(body);
        shapes.push(s);
    }
    Arc::make_mut(&mut p.slides[0]).shapes = shapes;
    let q = round(&p);
    for (name, warp, adj) in &cases {
        let b = &by_name(&q.slides[0], name).text.as_ref().expect("text").body;
        assert_eq!(b.warp.as_deref(), Some(*warp), "{name}");
        assert_eq!(&b.warp_adj, adj, "{name}");
    }
    // The single-adjust preset is written with the guide name `adj`, two-adjust ones with
    // `adj1`/`adj2`, and a preset with no adjusts given gets an empty list.
    let bytes = deckcraft_pptx::export(&p).expect("export");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("ppt/slides/slide1.xml").expect("slide"), &mut xml).expect("read");
    assert!(xml.contains(r#"<a:prstTxWarp prst="textInflate"><a:avLst><a:gd name="adj" fmla="val 30000"/></a:avLst></a:prstTxWarp>"#), "{xml}");
    assert!(xml.contains(r#"<a:gd name="adj1" fmla="val 13500000"/><a:gd name="adj2" fmla="val 30000"/>"#));
    assert!(xml.contains(r#"<a:prstTxWarp prst="textArchUp"><a:avLst/></a:prstTxWarp>"#));
    // A file that gives only `adj2` keeps the preset's default for `adj1`; hostile values are
    // read as numbers or dropped.
    let edited = xml
        .replace(r#"<a:gd name="adj1" fmla="val 13500000"/>"#, "")
        .replace(r#"<a:gd name="adj" fmla="val 30000"/>"#, r#"<a:gd name="adj" fmla="val NaN"/>"#);
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).expect("entry");
        let name = f.name().to_string();
        let mut data = vec![];
        std::io::Read::read_to_end(&mut f, &mut data).expect("read");
        if name == "ppt/slides/slide1.xml" {
            data = edited.clone().into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).expect("start");
        std::io::Write::write_all(&mut out, &data).expect("write");
    }
    let bytes = out.finish().expect("finish").into_inner();
    let q = deckcraft_pptx::import(&bytes).expect("import");
    let pour = &by_name(&q.slides[0], "Arch pour").text.as_ref().expect("text").body;
    assert_eq!(pour.warp_adj, vec![10_800_000.0, 30_000.0]);
    let inflate = &by_name(&q.slides[0], "Inflate").text.as_ref().expect("text").body;
    assert_eq!(inflate.warp.as_deref(), Some("textInflate"));
    assert!(inflate.warp_adj.is_empty());
}

/// Slide XML of a deck exported with one slide per transition (kind, option, ms, advance after).
fn transition_deck(list: &[(&str, &str, u32, Option<u32>)]) -> (Presentation, Vec<String>) {
    let mut p = defaults::new_presentation(None);
    let base = p.slides[0].as_ref().clone();
    p.slides.clear();
    for (kind, option, ms, adv) in list {
        let mut s = base.clone();
        s.id = SlideId(p.alloc_id());
        s.transition = Some(Transition {
            kind: kind.to_string(),
            option: option.to_string(),
            duration_ms: *ms,
            advance_on_click: adv.is_none(),
            advance_after_ms: *adv,
            ..Default::default()
        });
        p.slides.push(Arc::new(s));
    }
    let bytes = deckcraft_pptx::export(&p).expect("export");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
    let xml = (1..=list.len())
        .map(|i| {
            let mut x = String::new();
            std::io::Read::read_to_string(&mut zip.by_name(&format!("ppt/slides/slide{i}.xml")).expect("slide"), &mut x).expect("read");
            x
        })
        .collect();
    (p, xml)
}

#[test]
fn transition_xml_matches_powerpoints_elements() {
    // The elements, attributes and namespaces PowerPoint itself writes for these effect options
    // (observed on files it saved), inside `mc:AlternateContent` with a `p:fade` fallback for
    // the 2010+ effects.
    let cases: &[(&str, &str, &[&str], &str)] = &[
        ("wipe", "ru", &[r#"<p:strips dir="ru"/>"#], "p14"),
        ("wipe", "u", &[r#"<p:wipe dir="u"/>"#], "p14"),
        ("zoom", "in", &[r#"<p14:warp dir="in"/>"#], "p14"),
        ("zoom", "rotate", &["<p:newsflash/>"], "p14"),
        ("shape", "out", &[r#"<p:zoom dir="out"/>"#], "p14"),
        ("glitter", "hexD", &[r#"<p14:glitter dir="d" pattern="hexagon"/>"#], "p14"),
        ("shred", "particlesOut", &[r#"<p14:shred pattern="rectangle" dir="out"/>"#], "p14"),
        ("wind", "l", &[r#"<p15:prstTrans prst="wind" invX="1"/>"#], "p15"),
        ("wind", "r", &[r#"<p15:prstTrans prst="wind"/>"#], "p15"),
        ("fallOver", "r", &[r#"<p15:prstTrans prst="fallOver" invX="1"/>"#], "p15"),
        ("pageCurlDouble", "singleR", &[r#"<p15:prstTrans prst="pageCurlSingle" invX="1"/>"#], "p15"),
        ("orbit", "u", &[r#"<p14:prism dir="u" isContent="1" isInverted="1"/>"#], "p14"),
        ("vortex", "r", &[r#"<p14:vortex dir="r"/>"#], "p14"),
        ("morph", "words", &[r#"<p159:morph option="byWord"/>"#], "p159"),
    ];
    let list: Vec<(&str, &str, u32, Option<u32>)> = cases.iter().map(|c| (c.0, c.1, 2000, Some(1000))).collect();
    let (p, xml) = transition_deck(&list);
    for ((kind, option, wants, req), x) in cases.iter().zip(&xml) {
        for w in *wants {
            assert!(x.contains(w), "{kind}/{option}: {w} in {x}");
        }
        assert!(x.contains(&format!(r#"Requires="{req}""#)), "{kind}/{option}");
        assert!(x.contains(r#"p14:dur="2000""#) && x.contains(r#"advTm="1000""#) && x.contains(r#"advClick="0""#), "{kind}/{option}");
        assert!(x.contains("<mc:Fallback>"), "{kind}/{option}");
    }
    // Back in, everything is as it was.
    let bytes = deckcraft_pptx::export(&p).expect("export");
    let q = deckcraft_pptx::import(&bytes).expect("import");
    for ((kind, option, ..), s) in cases.iter().zip(&q.slides) {
        let t = s.transition.as_ref().expect("transition");
        assert_eq!(
            (t.kind.as_str(), t.option.as_str(), t.duration_ms, t.advance_after_ms, t.advance_on_click),
            (*kind, *option, 2000, Some(1000), false)
        );
    }
}

#[test]
fn transition_defaults_read_like_powerpoint() {
    // PowerPoint leaves out an option when it is the element's default; these are its defaults.
    let cases: &[(&str, &str, &str)] = &[
        ("<p:push/>", "push", "l"),
        ("<p:wipe/>", "wipe", "l"),
        ("<p:strips/>", "wipe", "lu"),
        ("<p:randomBar/>", "randomBar", "horz"),
        ("<p:checker/>", "checker", "horz"),
        ("<p:blinds/>", "blinds", "horz"),
        ("<p:comb/>", "comb", "horz"),
        ("<p:zoom/>", "shape", "out"),
        (r#"<p:zoom dir="in"/>"#, "shape", "in"),
        ("<p:split/>", "split", "horzOut"),
        ("<p14:doors/>", "doors", "horz"),
        ("<p14:window/>", "window", "horz"),
        ("<p14:pan/>", "pan", "l"),
        ("<p14:warp/>", "zoom", "out"),
        ("<p14:glitter/>", "glitter", "l"),
        ("<p14:shred/>", "shred", "stripsIn"),
        ("<p14:reveal/>", "reveal", "smoothlyLeft"),
        ("<p14:prism/>", "cube", "l"),
        (r#"<p15:prstTrans prst="airplane"/>"#, "airplane", "r"),
        (r#"<p15:prstTrans prst="peelOff"/>"#, "peelOff", "l"),
        (r#"<p15:prstTrans prst="pageCurlSingle"/>"#, "pageCurlDouble", "singleL"),
    ];
    let (_, xml) = transition_deck(&[("fade", "smoothly", 2000, None)]);
    let Some(x) = xml.first() else { panic!("no slide") };
    let start = x.find("<mc:AlternateContent").expect("transition");
    let end = x.find("</mc:AlternateContent>").expect("transition end") + "</mc:AlternateContent>".len();
    let p14 = r#"xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main""#;
    let p15 = r#"xmlns:p15="http://schemas.microsoft.com/office/powerpoint/2012/main""#;
    for (el, kind, option) in cases {
        let tr = format!(r#"<p:transition {p14} {p15} spd="slow" p14:dur="2000">{el}</p:transition>"#);
        let edited = format!("{}{}{}", &x[..start], tr, &x[end..]);
        let mut p = defaults::new_presentation(None);
        let base = p.slides[0].as_ref().clone();
        p.slides = vec![Arc::new(base)];
        let bytes = deckcraft_pptx::export(&p).expect("export");
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).expect("entry");
            let name = f.name().to_string();
            let mut data = vec![];
            std::io::Read::read_to_end(&mut f, &mut data).expect("read");
            if name == "ppt/slides/slide1.xml" {
                data = edited.clone().into_bytes();
            }
            out.start_file(name, zip::write::SimpleFileOptions::default()).expect("start");
            std::io::Write::write_all(&mut out, &data).expect("write");
        }
        let bytes = out.finish().expect("finish").into_inner();
        let q = deckcraft_pptx::import(&bytes).expect("import");
        let t = q.slides[0].transition.as_ref().expect("transition");
        assert_eq!((t.kind.as_str(), t.option.as_str(), t.duration_ms), (*kind, *option, 2000), "{el}");
    }
}
