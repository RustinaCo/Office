//! GridCraft on Android: the same engine (`gridcraft-engine`) and the same egui
//! frontend (`gridcraft-ui-egui`) as the desktop app, driven by eframe's Android
//! (NativeActivity) backend.
//!
//! Platform services follow the `apps/wordcraft-web` pattern without touching
//! the engine:
//! - open: `open_async` raises the system file manager (SAF); the picked file
//!   is staged as a library copy and opened normally (recent/dirty/autosave
//!   all work).
//! - save: `pick_save` returns a library path at once (dialogs complete
//!   instantly); the picked user location is fulfilled when its URI arrives.
//!   The system picker shows the name prefilled and editable.
//! - export/download: staged in the library, then the user picks the
//!   destination in the system picker.
//! - clipboard: text put/get over JNI (copies publish plain text; HTML paste
//!   is not wired: plain text only).
//! - typing: a transparent Android `EditText` overlay (real keyboard with
//!   suggestions) mirrors its text into the cell editor every frame.
//! - dark canvas like modern Office (display-only: Auto text shows white,
//!   explicit colors and all exports stay exactly as authored).
//! No runtime permissions are needed for any of this (app-specific storage +
//! SAF grants + clipboard + IME are all permission-free).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "android")]
use std::sync::{Mutex, OnceLock};

use gridcraft_engine::Session;
use gridcraft_ui_egui::{Inbox, Services, SheetApp};

#[cfg(target_os = "android")]
mod bridge;
#[cfg(target_os = "android")]
mod fonts;
#[cfg(target_os = "android")]
mod insets;

/// Global touch zoom for the phone UI, as the app's "Interface size"
/// preference (percent; the app clamps 75..=200 and enforces it every frame).
/// Everything (fonts, buttons, grid) grows together, but nothing moves and
/// nothing is removed — desktop muscle memory stays valid.
#[cfg(target_os = "android")]
const TOUCH_INTERFACE_SCALE: u16 = 135;
#[cfg(target_os = "android")]
const TOUCH_ZOOM: f32 = TOUCH_INTERFACE_SCALE as f32 / 100.0;

/// Workbook files picked asynchronously (kept for API symmetry).
#[cfg(target_os = "android")]
static INBOX: OnceLock<Inbox> = OnceLock::new();
/// App library dir (`.../Android/data/<pkg>/files/workbooks`), permission-free.
#[cfg(target_os = "android")]
static LIB_DIR: OnceLock<PathBuf> = OnceLock::new();
/// A SAF (system file manager) save/export waiting for its destination URI.
#[cfg(target_os = "android")]
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
/// The IME overlay currently owns the cell editor.
#[cfg(target_os = "android")]
static OVERLAY: AtomicBool = AtomicBool::new(false);
/// Long-press-to-select-word tracking: (touch id, press pos in points, start).
/// A held press becomes a synthetic double-click, which is exactly what the
/// editor's own machinery turns into a word selection — no new UI, no new
/// selection code paths.
#[cfg(target_os = "android")]
static LONG_PRESS: Mutex<Option<(egui::TouchId, egui::Pos2, std::time::Instant)>> =
    Mutex::new(None);
/// A synthetic double-click to inject on the next `raw_input_hook`.
#[cfg(target_os = "android")]
static DOUBLE_CLICK: Mutex<Option<egui::Pos2>> = Mutex::new(None);
/// Long-press dwell + drift allowance.
#[cfg(target_os = "android")]
const LONG_PRESS_MS: u128 = 550;
#[cfg(target_os = "android")]
const LONG_PRESS_SLOP: f32 = 12.0;
/// Last applied system-bar theme + whether it was applied yet.
#[cfg(target_os = "android")]
static BARS_DARK: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "android")]
static BARS_SET: AtomicBool = AtomicBool::new(false);
/// Frame counter for the live inset re-query (startup runs before attach).
#[cfg(target_os = "android")]
static FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);


/// Last saved prefs snapshot (to write only on change).
#[cfg(target_os = "android")]
static LAST_SAVED: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// A user-visible file operation waiting on the system picker.
#[cfg(target_os = "android")]
enum Pending {
    /// Library file to copy to the picked URI.
    WriteUri { library: PathBuf },
    /// Open picked: nothing to do until bytes arrive (handled inline).
    Open,
}

/// Settings dir (`ui.json` + `prefs.json`, same layout as desktop):
/// app-private internal storage, permission-free, survives restarts.
#[cfg(target_os = "android")]
fn config_dir() -> Option<PathBuf> {
    if std::env::var_os("GRIDCRAFT_NO_PREFS").is_some() {
        return None;
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("gridcraft"))
}

#[cfg(target_os = "android")]
fn load_prefs(app: &mut SheetApp) -> bool {
    let Some(dir) = config_dir() else { return false };
    // Same lenient load as desktop: unknown fields are ignored by serde default.
    if let Ok(b) = std::fs::read(dir.join("ui.json"))
        && let Ok(ui) = serde_json::from_slice::<gridcraft_ui_egui::UiState>(&b)
    {

        let saved = serde_json::from_slice(&b)
            .ok()
            .and_then(|v| gridcraft_ui_egui::i18n::saved_language(&v));
        let language = saved.unwrap_or(app.ui.language);
        app.ui = ui;
        app.ui.language = language;
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
fn save_prefs(app: &SheetApp) {
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

pub struct GridAndroidApp(SheetApp);

impl GridAndroidApp {
    pub fn new(_cc: &eframe::CreationContext) -> Self {
        let mut session = Session::new();
        session.new_workbook();
        #[cfg(target_os = "android")]
        let services = android_services();
        #[cfg(not(target_os = "android"))]
        let services = Services::default();
        let mut app = SheetApp::new(session, services);
        #[cfg(target_os = "android")]
        {
            // Saved settings win (theme, interface size, ...); first run gets
            // the phone interface size: the app's own "Interface size"
            // preference owns the egui zoom factor (SheetApp::logic enforces
            // it every frame), so driving the preference is the conflict-free
            // way to scale the whole UI up for touch.
            if !load_prefs(&mut app) {
                app.ui.interface_scale = TOUCH_INTERFACE_SCALE;
            }
        }
        GridAndroidApp(app)
    }
}

#[cfg(target_os = "android")]
fn android_services() -> Services {
    Services {
        inbox: Some(inbox()),
        // System file manager (SAF): the open flow completes when the picked
        // bytes arrive and are opened from the library copy.
        open_async: Some(Box::new(|| {
            *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Pending::Open);
            bridge::launch_open(OPEN_MIMES);
        })),
        // Save/Save As/Export: the engine writes the library copy NOW (so the
        // dialog completes instantly), and the picked user location is
        // fulfilled when its URI arrives. The system picker shows the name
        // prefilled and editable.
        pick_save: Some(Box::new(|suggested| android_pick_save(suggested))),
        download: Some(Box::new(|name, bytes| android_download(name, bytes))),
        copy_html: Some(Box::new(|_html: &str, text: &str| {
            bridge::put_text(text);
            Ok(())
        })),
        paste_text: Some(Box::new(|| bridge::get_text())),
        ..Default::default()
    }
}

#[cfg(target_os = "android")]
const OPEN_MIMES: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,application/vnd.openxmlformats-officedocument.spreadsheetml.macroenabled,application/vnd.ms-excel,text/csv,text/plain";

/// Provisional library save + system picker for the user-visible copy.
/// Always returns a path (any extension the engine can write); the picker
/// result only decides the second, user-facing write.
#[cfg(target_os = "android")]
fn android_pick_save(suggested: &str) -> Option<String> {
    let dir = LIB_DIR.get()?;
    std::fs::create_dir_all(dir).ok()?;
    let path = Path::new(suggested);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_else(|| "xlsx".to_string());
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(sanitize_stem)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Book1".to_string());
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
        .unwrap_or_else(|| "Book1".to_string());
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
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "txt" | "tsv" => "text/plain",
        "xlsx" | "xlsm" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xls" => "application/vnd.ms-excel",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        _ => "application/octet-stream",
    }
}

impl eframe::App for GridAndroidApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.logic(ctx);
        self.0.poll_printing(ctx);
        if ctx.input(|input| input.viewport().close_requested()) && self.0.printing.busy() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.0.raw_input_hook(raw);
        #[cfg(target_os = "android")]
        inject_double_click(raw);
        // Files dropped on the window open as workbooks (desktop; no-op on Android).
        for f in std::mem::take(&mut raw.dropped_files) {
            if let Some(p) = f
                .path()
                .to_str()
                .map(str::to_string)
                .filter(|p| !p.is_empty())
            {
                self.0.open_path(&p);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_mobile_chrome(&ctx, ui);
        #[cfg(target_os = "android")]
        {
            // Solid system bars matching the *rendered* UI theme (the app has
            // its own light/dark preference, independent of the OS setting),
            // re-applied on change. White bars + dark icons in light mode and
            // vice versa; the bars are never hidden.
            let dark = self.0.effective_dark;
            if !BARS_SET.load(Ordering::SeqCst) || BARS_DARK.load(Ordering::SeqCst) != dark {
                bridge::style_bars(dark);
                BARS_DARK.store(dark, Ordering::SeqCst);
                BARS_SET.store(true, Ordering::SeqCst);
            }
            // Live inset re-query: startup runs before the decor view is
            // attached, so converge once the window is live (every 60 frames
            // is plenty; the values only change on nav-mode/rotation change).
            if FRAMES.fetch_add(1, Ordering::Relaxed) % 60 == 0
                && let Some((top, bottom)) = bridge::system_insets()
            {
                insets::refresh(top, bottom);
            }
            self.poll_picker();
            self.ime_pump();
            self.track_long_press(&ctx);
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
            // Keep the keyboard up while editing: the first show can land
            // before the view is served and get ignored.
            static KB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            if OVERLAY.load(Ordering::SeqCst) && KB.fetch_add(1, Ordering::Relaxed) % 30 == 0 {
                bridge::ime_show();
            }
        }
        self.0.ui(ui);
    }
}

#[cfg(target_os = "android")]
impl GridAndroidApp {
    /// Long-press with the cell editor open selects the word under the finger,
    /// like Excel: track the press here (read-only over input events), inject
    /// the double-click in `raw_input_hook` (next frame).
    fn track_long_press(&self, ctx: &egui::Context) {
        let editing = self.0.editor.is_some() && ctx.memory(|m| m.focused().is_some());
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
            *DOUBLE_CLICK.lock().unwrap_or_else(|e| e.into_inner()) = Some(*pos);
            *tracked = None;
        }
    }
}

/// Inject the pending synthetic double-click (two press/release pairs at one
/// position in a single pass reads as a double-click), which the editor turns
/// into a word selection.
#[cfg(target_os = "android")]
fn inject_double_click(raw: &mut egui::RawInput) {
    let Some(pos) = DOUBLE_CLICK
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
    else {
        return;
    };
    use egui::{Event, Modifiers, PointerButton};
    raw.events.push(Event::PointerMoved(pos));
    for _ in 0..2 {
        raw.events.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::default(),
        });
        raw.events.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::default(),
        });
    }
}

/// Mirror the Android IME overlay into the cell/formula editor. The overlay
/// (a real `EditText`) owns composing, suggestions and autocorrect; egui's
/// widget stays focused and displays the mirrored text. One direction only
/// (overlay -> editor), so there is no echo loop.
#[cfg(target_os = "android")]
impl GridAndroidApp {
    fn ime_pump(&mut self) {
        let editing = self.0.editor.is_some();
        if editing && !OVERLAY.load(Ordering::SeqCst) {
            let seed = self
                .0
                .editor
                .as_ref()
                .map(|ed| ed.text.clone())
                .unwrap_or_default();
            bridge::ime_focus(&seed);
            OVERLAY.store(true, Ordering::SeqCst);
            return;
        }
        if !OVERLAY.load(Ordering::SeqCst) {
            return;
        }
        if !editing {
            bridge::ime_hide();
            OVERLAY.store(false, Ordering::SeqCst);
            return;
        }
        if bridge::ime_take_done() {
            // Done on the keyboard commits like Enter in the formula bar.
            self.0.commit_edit(0, 0, false, false);
            bridge::ime_hide();
            OVERLAY.store(false, Ordering::SeqCst);
            return;
        }
        let overlay = bridge::ime_poll();
        if let Some(ed) = self.0.editor.as_mut()
            && ed.text != overlay
        {
            // Move the caret to the end only while the text grows (typing);
            // a shrink/replacement (autocorrect, selection edit) keeps the
            // touch-made caret/selection instead of collapsing it.
            let grew = overlay.chars().count() >= ed.text.chars().count();
            ed.text = overlay;
            if grew {
                ed.caret = ed.text.chars().count();
                ed.sync_caret = true;
                ed.caret_to_end = true;
            }
        }
    }
}

/// Fulfill system-picker round-trips: user cancellation, picked opens (saved
/// as a library copy, then opened normally so recent/dirty/autosave work),
/// and picked save destinations (library bytes copied out).
#[cfg(target_os = "android")]
impl GridAndroidApp {
    fn poll_picker(&mut self) {
        if bridge::take_cancelled() {
            *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = None;
            eprintln!("grid-android: picker cancelled");
            return;
        }
        if let Some((name, bytes)) = bridge::take_open() {
            *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = None;
            let Some(dir) = LIB_DIR.get() else { return };
            std::fs::create_dir_all(dir).ok();
            let stem = Path::new(&name)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(sanitize_stem)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Book1".to_string());
            let ext = Path::new(&name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("xlsx");
            let library = unique_path(dir, &stem, ext);
            match std::fs::write(&library, &bytes) {
                Ok(()) => self.0.open_path(&library.to_string_lossy()),
                Err(e) => eprintln!("grid-android: cannot stage picked file: {e}"),
            }
            return;
        }
        if let Some(uri) = bridge::take_create_uri() {
            let pending = PENDING.lock().unwrap_or_else(|e| e.into_inner()).take();
            if let Some(Pending::WriteUri { library }) = pending {
                match std::fs::read(&library) {
                    Ok(bytes) if bridge::write_uri(&uri, &bytes) => {
                        eprintln!(
                            "grid-android: wrote {} to picked location",
                            library.display()
                        )
                    }
                    _ => eprintln!("grid-android: write to picked location failed"),
                }
            }
        }
    }
}

#[cfg(target_os = "android")]
impl GridAndroidApp {
    /// Safe-area spacers that dock above/below the app's own ribbon and
    /// status bar. Called at the top of `ui()` so the spacers reserve space
    /// before the shared panels are shown.
    ///
    /// The spacers are OPAQUE in the bar color the user asked for (black in
    /// dark mode, white in light): on API 35+ the system ignores
    /// `setStatusBarColor` (edge-to-edge is enforced), so the bar background
    /// is drawn by us showing through the transparent system bars. Icon
    /// contrast still comes from `style_bars` (appearance flags).
    fn apply_mobile_chrome(&self, ctx: &egui::Context, ui: &mut egui::Ui) {
        use gridcraft_ui_egui::theme::Tokens;
        let bar = if Tokens::get(ctx).dark {
            egui::Color32::BLACK
        } else {
            egui::Color32::WHITE
        };
        // Spacer heights are points; the interface-scale zoom shrinks points,
        // so divide the dp insets back down to keep their physical size exact.
        let (top_dp, bottom_dp) = insets::current();
        let frame = egui::Frame::NONE.fill(bar);
        egui::Panel::top("grid_android_safe_top")
            .exact_size(top_dp / TOUCH_ZOOM)
            .frame(frame)
            .show_separator_line(false)
            .resizable(false)
            .show(ui, |_ui| {});
        egui::Panel::bottom("grid_android_safe_bottom")
            .exact_size(bottom_dp / TOUCH_ZOOM)
            .frame(frame)
            .show_separator_line(false)
            .resizable(false)
            .show(ui, |_ui| {});
    }
}

#[cfg(not(target_os = "android"))]
impl GridAndroidApp {
    fn apply_mobile_chrome(&self, _ctx: &egui::Context, _ui: &mut egui::Ui) {}
}

/// Android entry point: eframe drives the NativeActivity event loop.
///
/// winit's Android event loop is single-use per process, but Android can
/// re-run `android_main` in the *same* process (activity recreation, tap the
/// icon again after Back, ...). A second `run_native` would die with
/// `WinitEventLoop(RecreationAttempt)`, so:
/// - the first entry claims a `STARTED` flag and, when its loop ends,
///   exits the process outright — the next launch is a fresh process;
/// - any re-entry in an already-started process exits immediately for the
///   same reason (the system then restarts the activity in a new process).
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
        .map(|p| p.join("workbooks"));
    if let Some(dir) = lib {
        std::fs::create_dir_all(&dir).ok();
        LIB_DIR.set(dir).ok();
    }
    // A file shared *to* us (file manager › Share › Exel) opens directly.
    if let Some((name, bytes)) = bridge::inbound_share() {
        eprintln!("grid-android: inbound share {name} ({} bytes)", bytes.len());
        inbox().lock().map(|mut i| i.push((name, bytes))).ok();
    }
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "GridCraft",
        options,
        Box::new(|cc| Ok(Box::new(GridAndroidApp::new(cc)))),
    );
    std::process::exit(0);
}
