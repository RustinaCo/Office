//! Renders the whole DeckCraft window offscreen (wgpu, no window) to a PNG.
//!
//! `cargo run -p deckcraft-ui-egui --example ui_shot -- [--sample] [--size 412x900] [--scale 2]
//!  [--cmd 'id={json}']... [--file-tab | --page home|new|open|info|saveAs|print|export|account]
//!  [--sheet] [--show] [--steps 8] out.png`
//!
//! The phone layout comes from `DECKCRAFT_PHONE=1` (see `deckcraft_ui_egui::mobile`), which is the
//! same layout the Android build gets — this is how the phone UI is looked at without a device.
//! A phone opens on the splash and then on the File screen, so the phone layout gets a first frame
//! (which starts the splash clock), a wait, and only then the frames that are photographed.
//! `--page` opens the File screen on one of its destinations, `--sheet` closes it (the slide, as
//! after tapping Back) and `--show` starts the slide show.

use deckcraft_engine::Session;
use deckcraft_ui_egui::{SlideApp, backstage};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut sample = false;
    let mut size = (1440.0f32, 900.0f32);
    let mut scale = 2.0f32;
    let mut cmds: Vec<String> = vec![];
    let mut steps = 8usize;
    let mut file_tab = false;
    let mut page: Option<String> = None;
    let mut page_after: Option<String> = None;
    let mut sheet = false;
    let mut show = false;
    let mut out = "snapshot.png".to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--sample" => sample = true,
            "--file-tab" => file_tab = true,
            "--sheet" => sheet = true,
            "--show" => show = true,
            "--page" => {
                i += 1;
                page = args.get(i).cloned();
                page_after = page.clone();
            }
            "--size" => {
                i += 1;
                if let Some((w, h)) = args.get(i).and_then(|s| s.split_once('x')) {
                    size = (w.parse().unwrap_or(1440.0), h.parse().unwrap_or(900.0));
                }
            }
            "--scale" => {
                i += 1;
                scale = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(2.0);
            }
            "--steps" => {
                i += 1;
                steps = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(8);
            }
            "--cmd" => {
                i += 1;
                if let Some(c) = args.get(i) {
                    cmds.push(c.clone());
                }
            }
            other => out = other.to_string(),
        }
        i += 1;
    }
    let mut session = Session::new();
    if sample {
        deckcraft_engine::sample::open_sample(&mut session).expect("sample deck");
    }
    for c in &cmds {
        let (id, p) = c.split_once('=').map(|(a, b)| (a.to_string(), serde_json::from_str(b).unwrap_or(serde_json::json!({})))).unwrap_or((c.clone(), serde_json::json!({})));
        if let Err(e) = session.execute(&id, &p) {
            eprintln!("{id}: {e}");
        }
    }
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(size.0, size.1))
        .with_pixels_per_point(scale)
        .with_max_steps(12)
        .wgpu()
        .build_ui_state(
            move |ui, app: &mut SlideApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            {
                let mut app = SlideApp::new(session, Default::default());
                if let Some(id) = &page {
                    if let Some(p) = backstage::Page::parse(id) {
                        app.open_backstage(p);
                    }
                } else if file_tab {
                    app.open_backstage(backstage::Page::Home);
                }
                app
            },
        );
    // The phone splash covers the first ~1.6 s of real time, so the phone layout gets a first
    // frame (which starts the clock) and then a wait before the frames that are photographed.
    if std::env::var_os("DECKCRAFT_PHONE").is_some() {
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(1800));
        // Past the splash the phone opens on the File screen, so the layout a shot wants is
        // decided here, not when the app was built.
        harness.run_steps(2);
    }
    if let Some(id) = &page_after
        && let Some(p) = backstage::Page::parse(id)
    {
        harness.state_mut().open_backstage(p);
    }
    if sheet {
        harness.state_mut().backstage = None;
    }
    if show {
        // The full-screen show, not the presenter view (a second screen).
        harness.state_mut().ui.presenter_view = false;
        let _ = harness.state_mut().run("show.start", serde_json::json!({}));
    }
    harness.run_steps(steps);
    let img = harness.render().expect("render");
    img.save(&out).expect("save png");
    eprintln!("wrote {out} ({}x{})", img.width(), img.height());
}
