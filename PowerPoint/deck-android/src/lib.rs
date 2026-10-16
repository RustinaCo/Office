//! DeckCraft on Android: the same engine (`deckcraft-engine`) and the same egui
//! frontend (`deckcraft-ui-egui`) as the desktop app, driven by eframe's Android
//! (NativeActivity) backend.
//!
//! Platform services without touching the engine:
//! - open/picture/audio/video: `open_async` raises the system file manager
//!   (SAF); bytes land in `inbox`, which the app routes by content type
//!   (presentations open, media inserts).
//! - save: `pick_save` returns a library path at once (dialogs complete
//!   instantly); the picked user location is fulfilled when its URI arrives.
//! - export/download: staged in the library, then the user picks the
//!   destination in the system picker.
//! - media files: host `read`/`write` over plain filesystem paths (library).
//! - clipboard: copies are mirrored from egui output (slides keep their
//!   designed colors; the canvas around them is already dark).
//! - typing: a transparent Android `EditText` overlay (real keyboard with
//!   suggestions) feeds `ImeEvent::Preedit`/`Commit` into the app's public
//!   `synthetic` event queue — its own IME pipeline with proper undo.
//!   (System→app text paste is not wired: the paste menu reads internal state
//!   only. Audio output is silent: no `audio_out` backend.)
//! No runtime permissions are needed for any of this.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "android")]
use std::sync::{Mutex, OnceLock};

use deckcraft_engine::Session;
use deckcraft_ui_egui::{Inbox, Services, SlideApp};

#[cfg(target_os = "android")]
mod bridge;
#[cfg(target_os = "android")]
mod fonts;
#[cfg(target_os = "android")]
mod insets;

/// Global touch zoom for the phone UI, as the app's own `ui_scale`
/// (clamped 0.5–3.0 by the app itself every frame).
#[cfg(target_os = "android")]
const TOUCH_UI_SCALE: f32 = 1.35;
#[cfg(target_os = "android")]
const TOUCH_ZOOM: f32 = TOUCH_UI_SCALE;

/// Picked files waiting for the app router.
#[cfg(target_os = "android")]
static INBOX: OnceLock<Inbox> = OnceLock::new();
/// App library dir (`.../Android/data/<pkg>/files/presentations`), permission-free.
#[cfg(target_os = "android")]
static LIB_DIR: OnceLock<PathBuf> = OnceLock::new();
/// A SAF (system file manager) save/export waiting for its destination URI.
#[cfg(target_os = "android")]
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
/// The IME overlay is focused for slide text.
#[cfg(target_os = "android")]
static OVERLAY: AtomicBool = AtomicBool::new(false);
/// Last applied system-bar theme + whether it was applied yet.
#[cfg(target_os = "android")]
static BARS_DARK: AtomicBool = AtomicBool::new(false);
/// Long-press-to-select-word tracking: (touch id, press pos in points, start).
/// A held press becomes a synthetic double-click, which is exactly what the
/// canvas's own machinery turns into a word selection — no new UI, no new
/// selection code paths.
#[cfg(target_os = "android")]
static LONG_PRESS: Mutex<Option<(egui::TouchId, egui::Pos2, std::time::Instant)>> =
    Mutex::new(None);
/// Long-press dwell + drift allowance.
#[cfg(target_os = "android")]
const LONG_PRESS_MS: u128 = 550;
#[cfg(target_os = "android")]
const LONG_PRESS_SLOP: f32 = 12.0;
#[cfg(target_os = "android")]
static BARS_SET: AtomicBool = AtomicBool::new(false);
/// Frame counter for the live inset re-query (startup runs before attach).
#[cfg(target_os = "android")]
static FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);


/// Last saved prefs snapshot (to write only on change).
#[cfg(target_os = "android")]
static LAST_SAVED: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// A user-visible save waiting on the system picker.
#[cfg(target_os = "android")]
enum Pending {
    /// Library file to copy to the picked URI.
    WriteUri { library: PathBuf },
}

/// Settings dir (`ui.json` + `prefs.json`, same layout as desktop):
/// app-private internal storage, permission-free, survives restarts.
#[cfg(target_os = "android")]
fn config_dir() -> Option<PathBuf> {
    if std::env::var_os("DECKCRAFT_NO_PREFS").is_some() {
        return None;
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("deckcraft"))
}

#[cfg(target_os = "android")]
fn load_prefs(app: &mut SlideApp) -> bool {
    let Some(dir) = config_dir() else { return false };
    if let Ok(b) = std::fs::read(dir.join("ui.json"))
        && let Ok(mut ui) = serde_json::from_slice::<deckcraft_ui_egui::UiState>(&b)
    {
        // A zoom saved on a wider screen would ask the phone GPU for a
        // texture it cannot make.
        ui.zoom = ui.zoom.map(|z| z.clamp(0.1, 3.0));
        app.ui = ui;
    } else {
        return false;
    }
    if let Ok(b) = std::fs::read(dir.join("prefs.json"))
        && let Ok(p) = serde_json::from_slice(&b)
    {
        app.session.prefs = p;
    }
    true
}

#[cfg(target_os = "android")]
fn save_prefs(app: &SlideApp) {
    let Some(dir) = config_dir() else { return };
    std::fs::create_dir_all(&dir).ok();
    let mut blob = Vec::new();
    if let Ok(b) = serde_json::to_vec(&app.ui) {
        let _ = std::fs::write(dir.join("ui.json"), &b);
        blob.extend_from_slice(&b);
    }
    if let Ok(b) = serde_json::to_vec(&app.session.prefs) {
        let _ = std::fs::write(dir.join("prefs.json"), &b);
        blob.extend_from_slice(&b);
    }
    *LAST_SAVED.lock().unwrap_or_else(|e| e.into_inner()) = Some(blob);
}

fn inbox() -> Inbox {
    #[cfg(target_os = "android")]
    return INBOX.get_or_init(Default::default).clone();
    #[cfg(not(target_os = "android"))]
    return Inbox::default();
}

pub struct DeckAndroidApp(SlideApp);

impl DeckAndroidApp {
    pub fn new(_cc: &eframe::CreationContext) -> Self {
        let session = Session::new();
        #[cfg(target_os = "android")]
        let services = android_services();
        #[cfg(not(target_os = "android"))]
        let services = Services::default();
        let mut app = SlideApp::new(session, services);
        #[cfg(target_os = "android")]
        {
            // Saved settings win (brightness, scale, ...); first run gets
            // the phone scale through the app's own `ui_scale` (clamped by
            // the app itself every frame).
            if !load_prefs(&mut app) {
                app.ui.ui_scale = TOUCH_UI_SCALE;
            }
        }
        DeckAndroidApp(app)
    }
}

#[cfg(target_os = "android")]
fn android_services() -> Services {
    Services {
        inbox: Some(inbox()),
        // System file manager; the app routes inbox bytes by content type.
        // (`pick_open` is deliberately unset so these flows use `open_async`.)
        open_async: Some(Box::new(|_kind| {
            bridge::launch_open(OPEN_MIMES);
        })),
        pick_save: Some(Box::new(|suggested| android_pick_save(suggested))),
        download: Some(Box::new(|name, bytes| android_download(name, bytes))),
        read: Some(Box::new(|path: &str| {
            std::fs::read(path).map_err(|e| e.to_string())
        })),
        write: Some(Box::new(|path: &str, bytes: &[u8]| {
            std::fs::write(path, bytes).map_err(|e| e.to_string())
        })),
        ..Default::default()
    }
}

#[cfg(target_os = "android")]
const OPEN_MIMES: &str = "application/vnd.openxmlformats-officedocument.presentationml.presentation,application/vnd.openxmlformats-officedocument.presentationml.template,application/vnd.openxmlformats-officedocument.presentationml.slideshow,text/plain,text/markdown,image/png,image/jpeg,image/gif,image/webp,audio/mpeg,audio/ogg,audio/wav,video/mp4,video/webm";

/// Provisional library save + system picker for the user-visible copy.
#[cfg(target_os = "android")]
fn android_pick_save(suggested: &str) -> Option<String> {
    let dir = LIB_DIR.get()?;
    std::fs::create_dir_all(dir).ok()?;
    let path = Path::new(suggested);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_else(|| "pptx".to_string());
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(sanitize_stem)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Presentation1".to_string());
    let library = unique_path(dir, &stem, &ext);
    bridge::launch_create(mime_for_ext(&ext), &format!("{stem}.{ext}"));
    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Pending::WriteUri {
        library: library.clone(),
    });
    Some(library.to_string_lossy().into_owned())
}

#[cfg(target_os = "android")]
fn sanitize_stem(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Staged export/download: library file first, user destination second.
#[cfg(target_os = "android")]
fn android_download(name: &str, bytes: &[u8]) {
    let Some(dir) = LIB_DIR.get() else { return };
    std::fs::create_dir_all(dir).ok();
    let path = Path::new(name);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_else(|| "bin".to_string());
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(sanitize_stem)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Presentation1".to_string());
    let library = unique_path(dir, &stem, &ext);
    if std::fs::write(&library, bytes).is_err() {
        return;
    }
    bridge::launch_create(mime_for_ext(&ext), &format!("{stem}.{ext}"));
    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Pending::WriteUri { library });
}

#[cfg(target_os = "android")]
fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    for n in 2..1000 {
        if !candidate.exists() {
            break;
        }
        candidate = dir.join(format!("{stem} ({n}).{ext}"));
    }
    candidate
}

#[cfg(target_os = "android")]
fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "pdf" => "application/pdf",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

impl eframe::App for DeckAndroidApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.logic(ctx);
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.0.raw_input_hook(raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_mobile_chrome(&ctx, ui);
        self.0.ui(ui);
        #[cfg(target_os = "android")]
        {
            // Solid system bars matching the *rendered* UI theme, re-applied
            // on change. Never hidden.
            let dark = self.0.ui.brightness == deckcraft_ui_egui::theme::Brightness::Dark;
            if !BARS_SET.load(Ordering::SeqCst) || BARS_DARK.load(Ordering::SeqCst) != dark {
                bridge::style_bars(dark);
                BARS_DARK.store(dark, Ordering::SeqCst);
                BARS_SET.store(true, Ordering::SeqCst);
            }
            // Live inset re-query: startup runs before the decor view is
            // attached, so converge once the window is live.
            if FRAMES.fetch_add(1, Ordering::Relaxed) % 60 == 0
                && let Some((top, bottom)) = bridge::system_insets()
            {
                insets::refresh(top, bottom);
            }
            self.poll_picker();
            self.ime_pump(&ctx);
            self.mirror_copies(&ctx);
            // Persist settings whenever they change (two tiny JSON writes;
            // no time throttle: idle frames may never come, e.g. the user
            // force-stops right after toggling the theme).
                let cur = (serde_json::to_vec(&self.0.ui).ok(), serde_json::to_vec(&self.0.session.prefs).ok());
                let changed = LAST_SAVED
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .is_none_or(|last| {
                        let mut blob = Vec::new();
                        if let Some(b) = &cur.0 {
                            blob.extend_from_slice(b);
                        }
                        if let Some(b) = &cur.1 {
                            blob.extend_from_slice(b);
                        }
                        *last != blob
                    });
                if changed {
                    save_prefs(&self.0);
                }
            self.track_long_press(&ctx);
            // Keep the keyboard up while editing: the first show can land
            // before the view is served and get ignored.
            static KB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            if OVERLAY.load(Ordering::SeqCst) && KB.fetch_add(1, Ordering::Relaxed) % 30 == 0 {
                bridge::ime_show();
            }
        }
    }
}

/// IME overlay lifecycle. Composing text travels as `ImeEvent::Preedit`
/// (live display) and the final text as `ImeEvent::Commit` (one undo step),
/// pushed into the app's public `synthetic` queue — its own IME pipeline.
#[cfg(target_os = "android")]
static IME_TEXT: Mutex<String> = Mutex::new(String::new());

#[cfg(target_os = "android")]
impl DeckAndroidApp {
    fn ime_pump(&mut self, ctx: &egui::Context) {
        let ime_active = ctx.output(|o| o.ime.is_some());
        if ime_active && !OVERLAY.load(Ordering::SeqCst) {
            bridge::ime_focus("");
            *IME_TEXT.lock().unwrap_or_else(|e| e.into_inner()) = String::new();
            OVERLAY.store(true, Ordering::SeqCst);
            return;
        }
        if !OVERLAY.load(Ordering::SeqCst) {
            return;
        }
        if bridge::ime_take_done() {
            // Done on the keyboard: flush whatever is still unsent, diffed
            // so already-committed text is never repeated.
            let overlay = bridge::ime_poll();
            Self::commit_diff(&mut self.0.synthetic, &overlay);
            bridge::ime_hide();
            OVERLAY.store(false, Ordering::SeqCst);
            return;
        }
        if !ime_active {
            // Focus left the text: hide only after it stays away — a single
            // stray frame must not flash-hide the keyboard.
            static IDLE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            if IDLE.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 10 {
                IDLE.store(0, std::sync::atomic::Ordering::Relaxed);
                let overlay = bridge::ime_poll();
                Self::commit_diff(&mut self.0.synthetic, &overlay);
                bridge::ime_hide();
                OVERLAY.store(false, Ordering::SeqCst);
            }
            return;
        }
        // Keystrokes commit incrementally (see word-android): every key lands
        // in the document at once instead of pooling until Done.
        let overlay = bridge::ime_poll();
        Self::commit_diff(&mut self.0.synthetic, &overlay);
    }

    /// Diff `overlay` against sent text and push the delta (Backspaces + an
    /// `Ime::Commit` suffix) into the app's synthetic queue.
    fn commit_diff(queue: &mut Vec<egui::Event>, overlay: &str) {
        let mut sent = IME_TEXT.lock().unwrap_or_else(|e| e.into_inner());
        if *sent == overlay {
            return;
        }
        let common = sent
            .chars()
            .zip(overlay.chars())
            .take_while(|(a, b)| a == b)
            .count();
        let backspaces = sent.chars().count().saturating_sub(common);
        let suffix_byte = overlay
            .char_indices()
            .nth(common)
            .map(|(i, _)| i)
            .unwrap_or(overlay.len());
        let suffix = &overlay[suffix_byte..];
        for _ in 0..backspaces {
            queue.push(egui::Event::Key {
                key: egui::Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            });
        }
        if !suffix.is_empty() {
            queue.push(egui::Event::Ime(egui::ImeEvent::Commit(suffix.to_string())));
        }
        *sent = overlay.to_string();
    }

    /// Long-press with slide text focused selects the word under the finger:
    /// track the press here (read-only over input events); the double-click
    /// pair goes straight into the app's synthetic queue. Only while the
    /// canvas owns the keyboard, so ribbon long-presses are untouched.
    fn track_long_press(&mut self, ctx: &egui::Context) {
        let editing = ctx.output(|o| o.ime.is_some());
        if !editing {
            *LONG_PRESS.lock().unwrap_or_else(|e| e.into_inner()) = None;
            return;
        }
        let mut tracked = LONG_PRESS.lock().unwrap_or_else(|e| e.into_inner());
        ctx.input(|i| {
            for e in &i.events {
                if let egui::Event::Touch { id, phase, pos, .. } = e {
                    match phase {
                        egui::TouchPhase::Start => {
                            *tracked = Some((*id, *pos, std::time::Instant::now()));
                        }
                        egui::TouchPhase::Move => {
                            if let Some((tid, start, _)) = tracked.as_ref()
                                && tid == id
                                && start.distance(*pos) > LONG_PRESS_SLOP
                            {
                                *tracked = None;
                            }
                        }
                        egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                            *tracked = None;
                        }
                    }
                }
            }
        });
        if let Some((_, pos, t0)) = tracked.as_ref()
            && t0.elapsed().as_millis() >= LONG_PRESS_MS
        {
            use egui::{Event, Modifiers, PointerButton};
            let pos = *pos;
            self.0.synthetic.push(Event::PointerMoved(pos));
            for _ in 0..2 {
                self.0.synthetic.push(Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                });
                self.0.synthetic.push(Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::default(),
                });
            }
            *tracked = None;
        }
    }

    /// Copies leave through egui output; mirror the text to the real clipboard
    /// (eframe's own clipboard backend cannot run on Android).
    fn mirror_copies(&self, ctx: &egui::Context) {
        let copied: Vec<String> = ctx.output(|o| {
            o.commands
                .iter()
                .filter_map(|e| match e {
                    egui::output::OutputCommand::CopyText(t) => Some(t.clone()),
                    _ => None,
                })
                .collect()
        });
        for t in copied {
            bridge::put_text(&t);
        }
    }
}

/// Fulfill system-picker round-trips: picked files go to the inbox (the app
/// routes by content type), picked save destinations receive library bytes.
#[cfg(target_os = "android")]
impl DeckAndroidApp {
    fn poll_picker(&mut self) {
        if bridge::take_cancelled() {
            *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = None;
            eprintln!("deck-android: picker cancelled");
            return;
        }
        if let Some((name, bytes)) = bridge::take_open() {
            inbox().lock().map(|mut i| i.push((name, bytes))).ok();
            return;
        }
        if let Some(uri) = bridge::take_create_uri() {
            let pending = PENDING.lock().unwrap_or_else(|e| e.into_inner()).take();
            if let Some(Pending::WriteUri { library }) = pending {
                match std::fs::read(&library) {
                    Ok(bytes) if bridge::write_uri(&uri, &bytes) => {
                        eprintln!(
                            "deck-android: wrote {} to picked location",
                            library.display()
                        )
                    }
                    _ => eprintln!("deck-android: write to picked location failed"),
                }
            }
        }
    }
}

#[cfg(target_os = "android")]
impl DeckAndroidApp {
    /// Safe-area spacers that dock above/below the app chrome. Called at the
    /// top of `ui()` so the spacers reserve space before shared panels draw.
    fn apply_mobile_chrome(&self, ctx: &egui::Context, ui: &mut egui::Ui) {
        // OPAQUE spacers in the bar color (black in dark mode, white in
        // light): on API 35+ the system ignores setStatusBarColor
        // (edge-to-edge is enforced), so the bar background is drawn by us
        // showing through the transparent system bars.
        let bar = if self.0.ui.brightness == deckcraft_ui_egui::theme::Brightness::Dark {
            egui::Color32::BLACK
        } else {
            egui::Color32::WHITE
        };
        // Spacer heights are points; the ui-scale zoom shrinks points, so
        // divide the dp insets back down to keep their physical size exact.
        let (top_dp, bottom_dp) = insets::current();
        let frame = egui::Frame::NONE.fill(bar);
        egui::Panel::top("deck_android_safe_top")
            .exact_size(top_dp / TOUCH_ZOOM)
            .frame(frame)
            .show_separator_line(false)
            .resizable(false)
            .show(ui, |_ui| {});
        egui::Panel::bottom("deck_android_safe_bottom")
            .exact_size(bottom_dp / TOUCH_ZOOM)
            .frame(frame)
            .show_separator_line(false)
            .resizable(false)
            .show(ui, |_ui| {});
    }
}

#[cfg(not(target_os = "android"))]
impl DeckAndroidApp {
    fn apply_mobile_chrome(&self, _ctx: &egui::Context, _ui: &mut egui::Ui) {}
}

/// Android entry point: eframe drives the NativeActivity event loop.
///
/// winit's Android event loop is single-use per process, but Android can
/// re-run `android_main` in the *same* process. A second `run_native` would
/// die with `WinitEventLoop(RecreationAttempt)`, so the first entry claims a
/// flag and exits the process when its loop ends; re-entries exit at once.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        std::process::exit(0);
    }
    insets::init(&app);
    bridge::init(&app);
    // Home for the font scanner: the app has no system fonts, so bundled
    // fonts are seeded into `$HOME/.fonts` (see `fonts::install`). Internal
    // storage is private to the app; nothing else reads `$HOME` on Android.
    // SAFETY: runs once on the `android_main` thread before eframe starts any
    // threads, and no other thread reads the process environment meanwhile.
    if let Some(home) = app.internal_data_path() {
        unsafe { std::env::set_var("HOME", &home) };
        fonts::install();
    }
    // App library: external files dir (user-visible over MTP, no permission).
    let lib = app
        .external_data_path()
        .or_else(|| app.internal_data_path())
        .map(|p| p.join("presentations"));
    if let Some(dir) = lib {
        std::fs::create_dir_all(&dir).ok();
        LIB_DIR.set(dir).ok();
    }
    // A file shared *to* us opens through the inbox router.
    if let Some((name, bytes)) = bridge::inbound_share() {
        eprintln!("deck-android: inbound share {name} ({} bytes)", bytes.len());
        inbox().lock().map(|mut i| i.push((name, bytes))).ok();
    }
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "DeckCraft",
        options,
        Box::new(|cc| Ok(Box::new(DeckAndroidApp::new(cc)))),
    );
    std::process::exit(0);
}
