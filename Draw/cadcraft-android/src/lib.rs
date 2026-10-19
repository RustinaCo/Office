//! Rustina Draw on Android: the same engine (`cadcraft-engine`) and the same egui
//! frontend (`cadcraft-ui-egui`) as CADCraft on the desktop, driven by eframe's Android
//! (NativeActivity) backend.
//!
//! The shell is a thin host: it owns a [`CadApp`] and hands it the platform services
//! the desktop app gets from `rfd` and the window system.
//!
//! - open: the system file manager (SAF) returns bytes over JNI; the shell feeds them
//!   to [`CadApp::open_bytes`], the same path the web build uses for pathless files.
//! - save/export: `pick_save` returns a library path at once (the Save dialog completes
//!   instantly), so the engine writes there; the system picker's destination is
//!   fulfilled when its URI arrives, and later writes to the library file are mirrored
//!   to the user's own copy.
//! - typing: a transparent Android `EditText` overlay (real keyboard with suggestions)
//!   is diffed against what was already sent; the delta becomes `Event::Text` and
//!   Backspace keys, which is exactly what CADCraft's command line and its dialogs
//!   read. A floating keyboard toggle raises it, because CADCraft's command line is a
//!   painter that never asks egui for IME by itself.
//! - clipboard: copies leave through egui's `CopyText` output command and are mirrored
//!   to the real clipboard over JNI.
//! - system bars: solid, theme-matched, with safe-area spacers drawn by the shell.
//! - settings: `$HOME/.config/cadcraft` (app-private internal storage), the same file
//!   names and formats the desktop app keeps.
//!
//! No runtime permissions are needed for any of this.
//!
//! ## Rendering
//!
//! The canvas draws on the CPU ([`CadApp::set_wgpu`] is not called): eframe still
//! presents through wgpu, but CADCraft's own geometry is tessellated by egui. That is
//! correct for any drawing; it is slower than the GPU canvas the desktop app installs.
//!
//! ## Known gaps
//!
//! - The floating keyboard toggle's position and the 1×1 IME overlay are reasoned, not
//!   device-tested.
//! - A picked or shared drawing opens pathless ([`CadApp::open_bytes`]), so its own Save
//!   asks for a destination instead of rewriting the file the user opened.
//! - IMPORT, RECOVER and New from Template start the SAF picker but cannot finish: their
//!   command needs a path, and the picker only returns bytes.
//! - Pasting is unavailable: eframe's Android backend does not read the clipboard, so
//!   egui never produces `Event::Paste` (the JNI read exists, unused).
//! - The soft keyboard supplies text, Backspace and Enter only — no Escape, arrows or
//!   modifiers; those gestures stay on the on-screen UI.
//! - A picked file is decoded on the UI thread (`open_bytes`), so a very large drawing
//!   freezes the frame while it opens.
//! - craft-fonts are embedded only when `CRAFT_FONTS_DIR` points at a craft-fonts
//!   checkout (`fonts/manifest.txt`); otherwise the build warns and uses the embedded
//!   stroke font, as any build without that optional input does.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

#[cfg(target_os = "android")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "android")]
use std::sync::{Mutex, OnceLock};

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
#[cfg(target_os = "android")]
use cadcraft_ui_egui::UiState;

#[cfg(target_os = "android")]
mod bridge;
#[cfg(target_os = "android")]
mod insets;

/// The phone's touch interface size as a zoom factor over the display's own scale
/// (the sibling crafting apps' 135 %).
#[cfg(target_os = "android")]
const FIRST_RUN_INTERFACE_SCALE: f32 = 1.35;
/// Display scale assumed while the platform query has not answered.
#[cfg(target_os = "android")]
const TOUCH_DISPLAY_SCALE: f32 = 2.0;

/// Picked/shared files waiting for the app (drained every frame).
#[cfg(target_os = "android")]
static INBOX: Mutex<Vec<(String, Vec<u8>)>> = Mutex::new(Vec::new());
/// App library dir (`.../Android/data/<pkg>/files/documents`), permission-free.
#[cfg(target_os = "android")]
static LIB_DIR: OnceLock<PathBuf> = OnceLock::new();
/// A library file waiting for the system picker's destination URI.
#[cfg(target_os = "android")]
static AWAIT_URI: Mutex<Option<PathBuf>> = Mutex::new(None);
/// Library writes waiting for their file to exist (the engine writes after `pick_save`
/// returns, the picker answers later).
#[cfg(target_os = "android")]
static WRITES: Mutex<Vec<Write>> = Mutex::new(Vec::new());
/// The copies the user picked destinations for, kept in step with their library files.
#[cfg(target_os = "android")]
static MIRROR: Mutex<Vec<Mirror>> = Mutex::new(Vec::new());
/// The IME overlay (a real `EditText`) is focused.
#[cfg(target_os = "android")]
static OVERLAY: AtomicBool = AtomicBool::new(false);
/// The user asked for the keyboard (the floating toggle), independent of egui's IME.
#[cfg(target_os = "android")]
static WANT_KEYBOARD: AtomicBool = AtomicBool::new(false);
/// The overlay text already turned into events.
#[cfg(target_os = "android")]
static IME_SENT: Mutex<String> = Mutex::new(String::new());
/// Text handed to the overlay, waiting for the UI thread to apply it. Until it does,
/// the overlay still holds the *previous* text and must not be diffed (that would
/// replay it as typing).
#[cfg(target_os = "android")]
static IME_SEED: Mutex<Option<String>> = Mutex::new(None);
/// Frames spent waiting for a seed (cleared after [`IME_SEED_BUDGET`]).
#[cfg(target_os = "android")]
static IME_SEED_WAIT: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "android")]
const IME_SEED_BUDGET: u32 = 40;
/// Flush the remaining draft before hiding the overlay (set when the keyboard goes away).
#[cfg(target_os = "android")]
static IME_FLUSH: AtomicBool = AtomicBool::new(false);
/// The Java side's keyboard must be hidden this frame.
#[cfg(target_os = "android")]
static IME_HIDE: AtomicBool = AtomicBool::new(false);
/// Last applied system-bar theme + whether it was applied yet.
#[cfg(target_os = "android")]
static BARS_DARK: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "android")]
static BARS_SET: AtomicBool = AtomicBool::new(false);
/// Frame counter for the live inset re-query (startup runs before attach).
#[cfg(target_os = "android")]
static FRAMES: AtomicU64 = AtomicU64::new(0);

/// Long-press-to-context-menu tracking: (touch id, press pos in points, start).
#[cfg(target_os = "android")]
static LONG_PRESS: Mutex<Option<(egui::TouchId, egui::Pos2, std::time::Instant)>> =
    Mutex::new(None);
/// A synthetic secondary click to inject on the next `raw_input_hook`.
#[cfg(target_os = "android")]
static LONG_CLICK: Mutex<Option<egui::Pos2>> = Mutex::new(None);
/// Long-press dwell + drift allowance.
#[cfg(target_os = "android")]
const LONG_PRESS_MS: u128 = 550;
#[cfg(target_os = "android")]
const LONG_PRESS_SLOP: f32 = 12.0;

/// Last settings snapshot (to write only on change).
#[cfg(target_os = "android")]
static LAST_SAVED: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// A library write waiting for its bytes (see [`WRITES`]).
#[cfg(target_os = "android")]
struct Write {
    library: PathBuf,
    uri: String,
    waited: u32,
}

/// A picked destination kept in step with its library file.
#[cfg(target_os = "android")]
struct Mirror {
    library: PathBuf,
    uri: String,
    stamp: (u64, u64),
}

/// How many picked destinations are kept in step (one per drawing/export in flight).
#[cfg(target_os = "android")]
const MAX_MIRRORS: usize = 8;

/// Length and modification time of a file, as a cheap change stamp; `(0, 0)` when it
/// can't be read (a mirror then just skips this pass).
#[cfg(target_os = "android")]
fn file_stamp(path: &Path) -> (u64, u64) {
    match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(t) => (
            std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
        ),
        Err(_) => (0, 0),
    }
}

/// Settings dir (`workspace.json`, `profile.json`, `prefs.json`, `autosave.json`):
/// app-private internal storage, permission-free, survives restarts.
#[cfg(target_os = "android")]
fn settings_dir() -> Option<PathBuf> {
    if std::env::var_os("CADCRAFT_NO_PREFS").is_some() {
        return None;
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("cadcraft"))
}

/// Parse a saved workspace. A docking layout this build does not know is dropped on
/// its own (schemas evolve independently of the other preferences); transient dialogs
/// and the start page never come back from a file.
#[cfg(target_os = "android")]
fn workspace_from_json(bytes: &[u8]) -> Option<UiState> {
    let restore = |mut ui: UiState| {
        ui.dialog = None;
        ui.start_tab = false;
        ui
    };
    if let Ok(ui) = serde_json::from_slice::<UiState>(bytes) {
        return Some(restore(ui));
    }
    let mut value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let docking = value.as_object_mut().and_then(|o| o.remove("docking"));
    let mut ui: UiState = serde_json::from_value(value).ok()?;
    if let Some(docking) = docking
        && let Ok(workspace) =
            serde_json::from_value::<cadcraft_ui_egui::docking::Workspace>(docking)
        && workspace.validate().is_ok()
    {
        ui.docking = workspace;
    }
    Some(restore(ui))
}

/// Read the saved settings into `app`; returns whether a profile existed (first run when
/// it did not).
#[cfg(target_os = "android")]
fn load_settings(app: &mut CadApp) -> bool {
    let Some(dir) = settings_dir() else { return true };
    if let Ok(bytes) = std::fs::read(dir.join("workspace.json"))
        && let Some(ui) = workspace_from_json(&bytes)
    {
        app.ui = ui;
    }
    let profile = std::fs::read_to_string(dir.join("profile.json")).ok();
    if let Some(profile) = &profile {
        app.load_profile(profile);
    }
    if let Ok(prefs) = std::fs::read_to_string(dir.join("prefs.json")) {
        app.load_prefs(&prefs);
    }
    if let Ok(prefs) = std::fs::read_to_string(dir.join("autosave.json")) {
        cadcraft_ui_egui::autosave::load_prefs(&prefs);
    }
    let has_profile = profile.is_some();
    if !has_profile {
        app.options.interface_scale = FIRST_RUN_INTERFACE_SCALE;
    }
    has_profile
}

/// Everything the four settings files hold, as one buffer for change detection.
#[cfg(target_os = "android")]
fn settings_snapshot(app: &CadApp) -> Vec<u8> {
    let mut ui = app.ui.clone();
    ui.dialog = None;
    ui.start_tab = false;
    let workspace = serde_json::to_vec(&ui).unwrap_or_default();
    let mut out = Vec::with_capacity(workspace.len() + 4096);
    out.extend_from_slice(&(workspace.len() as u64).to_le_bytes());
    out.extend_from_slice(&workspace);
    for text in [
        app.profile_json(),
        app.prefs_json(),
        cadcraft_ui_egui::autosave::prefs_json(),
    ] {
        out.extend_from_slice(&(text.len() as u64).to_le_bytes());
        out.extend_from_slice(text.as_bytes());
    }
    out
}

/// Write the settings when they changed (tiny JSON files; no time throttle, idle frames
/// may never come).
#[cfg(target_os = "android")]
fn save_settings(app: &CadApp) {
    let snapshot = settings_snapshot(app);
    {
        let mut last = LAST_SAVED.lock().unwrap_or_else(|e| e.into_inner());
        if last.as_ref() == Some(&snapshot) {
            return;
        }
        *last = Some(snapshot);
    }
    let Some(dir) = settings_dir() else { return };
    std::fs::create_dir_all(&dir).ok();
    let mut ui = app.ui.clone();
    ui.dialog = None;
    ui.start_tab = false;
    if let Ok(bytes) = serde_json::to_vec(&ui) {
        write_atomic(&dir.join("workspace.json"), &bytes);
    }
    write_atomic(&dir.join("profile.json"), app.profile_json().as_bytes());
    write_atomic(&dir.join("prefs.json"), app.prefs_json().as_bytes());
    write_atomic(
        &dir.join("autosave.json"),
        cadcraft_ui_egui::autosave::prefs_json().as_bytes(),
    );
}

/// Replace `path` in one step, so a killed process leaves either the old or the new file.
#[cfg(target_os = "android")]
fn write_atomic(path: &Path, bytes: &[u8]) {
    let Some(parent) = path.parent() else { return };
    let temporary = parent.join(format!(
        ".{}-{}.tmp",
        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        std::process::id()
    ));
    if std::fs::write(&temporary, bytes).is_ok() {
        let _ = std::fs::rename(&temporary, path);
    } else {
        let _ = std::fs::remove_file(&temporary);
    }
}

/// File format hooks for the engine (it stays I/O-agnostic), as the desktop app installs
/// them. Without this OPEN, SAVEAS and the exporters have no reader/writer.
pub fn install_io() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: Some(|d, space, opts| cadcraft_io::plotter::plot(d, space, opts).map_err(|e| e.to_string())),
    });
    cadcraft_engine::cmd::file::set_write_as(|d, name, o| {
        cadcraft_io::write_with(d, name, &cadcraft_io::SaveOptions { version: o.version, binary: o.binary }).map_err(|e| e.to_string())
    });
    // AutoLISP's entget, entmake and entmod see entities as DXF groups.
    cadcraft_engine::cmd::file::set_entity_codec(cadcraft_engine::cmd::file::EntityCodec {
        groups: cadcraft_io::entity_groups,
        entity: cadcraft_io::entity_from_groups,
    });
}

/// The MIME types a drawing can arrive as. Android's table has no entry for DXF or DWG,
/// so the binary default and (for ASCII DXF sniffed as text) `text/plain` are listed
/// too; the picker shows them all.
#[cfg(target_os = "android")]
const OPEN_MIMES: &str = "image/vnd.dxf,application/dxf,application/dwg,image/vnd.dwg,application/acad,application/octet-stream,image/svg+xml,text/plain";

#[cfg(target_os = "android")]
fn android_services() -> Services {
    Services {
        // The picker has no path to return: it answers through `take_open`, which the
        // shell drains into `open_bytes`. Returning "nothing picked" here is what makes
        // OPEN/File ▸ Open return to the UI immediately.
        pick_open: Some(Box::new(|| {
            bridge::launch_open(OPEN_MIMES);
            None
        })),
        pick_save: Some(Box::new(android_pick_save)),
    }
}

/// Provisional library save + system picker for the user-visible copy. Always returns a
/// path so Save completes instantly; the picker result only decides the second,
/// user-facing write.
#[cfg(target_os = "android")]
fn android_pick_save(suggested: &str) -> Option<String> {
    let dir = LIB_DIR.get()?;
    std::fs::create_dir_all(dir).ok()?;
    let path = Path::new(suggested);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| matches!(e.as_str(), "dxf" | "dwg" | "dwt" | "svg" | "svgz" | "png" | "pdf"))
        .unwrap_or_else(|| "dxf".to_string());
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(sanitize_stem)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Drawing1".to_string());
    let library = unique_path(dir, &stem, &ext);
    bridge::launch_create(mime_for_ext(&ext), &format!("{stem}.{ext}"));
    *AWAIT_URI.lock().unwrap_or_else(|e| e.into_inner()) = Some(library.clone());
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
        "dxf" => "image/vnd.dxf",
        "dwg" => "application/acad",
        "svg" | "svgz" => "image/svg+xml",
        "png" => "image/png",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// The host app: a [`CadApp`] plus the phone chrome.
pub struct CadAndroidApp(CadApp);

impl CadAndroidApp {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let _ = cc;
        // OPEN/SAVEAS need the format readers and writers before the first frame.
        install_io();
        let mut app = CadApp::new(Session::empty(), services());
        #[cfg(target_os = "android")]
        {
            load_settings(&mut app);
            // The interface language follows the device (`fa-IR`, `de-DE`, …).
            if let Some(tag) = bridge::locale_tag() {
                app.system_languages = vec![tag];
            }
            if let Some(dir) = settings_dir() {
                // Autosave files default to `<settings>/autosave`; startup LISP
                // (acad.lsp, acaddoc.lsp) is read from `<settings>/support`.
                app.enable_autosave(&dir.join("autosave"));
                let support = dir.join("support");
                std::fs::create_dir_all(&support).ok();
                app.session.set_lisp_support_dir(Some(support));
            }
        }
        app.session.new_drawing(false);
        CadAndroidApp(app)
    }
}

#[cfg(target_os = "android")]
fn services() -> Services {
    android_services()
}

#[cfg(not(target_os = "android"))]
fn services() -> Services {
    Services::default()
}

impl eframe::App for CadAndroidApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.logic(ctx);
        #[cfg(target_os = "android")]
        {
            self.poll_picker(ctx);
            self.mirror_tick();
            if std::mem::take(&mut self.0.quit_requested) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.0.raw_input_hook(raw);
        #[cfg(target_os = "android")]
        {
            drain_ime(raw);
            inject_long_click(raw);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_mobile_chrome(&ctx, ui);
        self.0.ui(ui);
        #[cfg(target_os = "android")]
        {
            let dark = self.0.shown_theme() == egui::Theme::Dark;
            if keyboard_toggle(ui, OVERLAY.load(Ordering::SeqCst), dark) {
                WANT_KEYBOARD.store(!OVERLAY.load(Ordering::SeqCst), Ordering::SeqCst);
            }
            self.ime_tick(&ctx);
            self.mirror_copies(&ctx);
            let n = FRAMES.fetch_add(1, Ordering::Relaxed);
            if !BARS_SET.load(Ordering::SeqCst)
                || BARS_DARK.load(Ordering::SeqCst) != dark
                || n.is_multiple_of(60)
            {
                bridge::style_bars(dark);
                BARS_DARK.store(dark, Ordering::SeqCst);
                BARS_SET.store(true, Ordering::SeqCst);
            }
            // Live inset re-query: startup runs before the decor view is attached, so
            // converge once the window is live.
            if n.is_multiple_of(60)
                && let Some((top, bottom)) = bridge::system_insets()
            {
                insets::refresh(top, bottom);
            }
            save_settings(&self.0);
            self.track_long_press(&ctx);
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = ctx;
        }
    }

    fn on_exit(&mut self) {
        self.0.shutdown_autosave();
        #[cfg(target_os = "android")]
        save_settings(&self.0);
    }
}

// ---- IME overlay ------------------------------------------------------------

/// Ask the overlay for `text`, and treat its old contents as already sent until the UI
/// thread has applied the new ones.
#[cfg(target_os = "android")]
fn ime_reseed(text: &str) {
    *IME_SENT.lock().unwrap_or_else(|e| e.into_inner()) = text.to_string();
    *IME_SEED.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.to_string());
    IME_SEED_WAIT.store(0, Ordering::Relaxed);
    bridge::ime_focus(text);
}

/// The overlay's settled text. While a seed is in flight the overlay still holds the
/// previous text, so that is reported instead (no diff, no replay).
#[cfg(target_os = "android")]
fn ime_text() -> String {
    let polled = bridge::ime_poll();
    let mut seed = IME_SEED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(wanted) = seed.as_ref() {
        if *wanted == polled || IME_SEED_WAIT.fetch_add(1, Ordering::Relaxed) >= IME_SEED_BUDGET {
            *seed = None;
            IME_SEED_WAIT.store(0, Ordering::Relaxed);
        } else {
            return IME_SENT.lock().unwrap_or_else(|e| e.into_inner()).clone();
        }
    }
    polled
}

/// Feed the overlay's changes into this frame's input (runs after the app's own
/// `raw_input_hook`, before the frame consumes input).
///
/// Keystrokes commit INCREMENTALLY: the overlay text is diffed against what was already
/// sent, appended characters arrive as `Event::Text` and deletions as Backspace keys, so
/// every key lands in the command line (or a focused dialog field) immediately. The
/// keyboard's Done action submits with Enter and starts a fresh, empty line.
///
/// NOTE: CADCraft's command line reads `Event::Text` and `egui::Key::Backspace`; it does
/// not handle `ImeEvent::Commit`. This is why a diff becomes text events, not an IME
/// commit.
#[cfg(target_os = "android")]
fn drain_ime(raw: &mut egui::RawInput) {
    if IME_FLUSH.swap(false, Ordering::SeqCst) {
        // The keyboard is going away: send what is still unsent, then hide.
        let text = ime_text();
        commit_diff(raw, &text);
        IME_HIDE.store(true, Ordering::SeqCst);
        OVERLAY.store(false, Ordering::SeqCst);
        return;
    }
    if !OVERLAY.load(Ordering::SeqCst) {
        return;
    }
    let done = bridge::ime_take_done();
    let text = ime_text();
    commit_diff(raw, &text);
    if done {
        raw.events.push(key(egui::Key::Enter));
        // Continue with a fresh line: Enter submitted the command (or completed the
        // prompt) and the command line's buffer is empty again. The overlay keeps the
        // keyboard; only its text is cleared.
        ime_reset_quiet("");
    }
}

/// Clear the overlay to `text` without asking for focus again (the keyboard stays up).
#[cfg(target_os = "android")]
fn ime_reset_quiet(text: &str) {
    *IME_SENT.lock().unwrap_or_else(|e| e.into_inner()) = text.to_string();
    *IME_SEED.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.to_string());
    IME_SEED_WAIT.store(0, Ordering::Relaxed);
    bridge::ime_set_text(text);
}

#[cfg(target_os = "android")]
fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    }
}

/// Diff `overlay` against the sent text and push the delta (Backspaces + text).
#[cfg(target_os = "android")]
fn commit_diff(raw: &mut egui::RawInput, overlay: &str) {
    let mut sent = IME_SENT.lock().unwrap_or_else(|e| e.into_inner());
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
        raw.events.push(key(egui::Key::Backspace));
    }
    if !suffix.is_empty() {
        // One text event for the whole suffix: CADCraft appends a text event in one go
        // (an IME suggestion can replace several characters at once).
        raw.events.push(egui::Event::Text(suffix.to_string()));
    }
    *sent = overlay.to_string();
}

/// IME overlay lifecycle: show for the user's toggle or a focused egui text field, seed
/// it from the command line, keep it in step when the command line changes behind its
/// back, and flush before hiding.
#[cfg(target_os = "android")]
impl CadAndroidApp {
    fn ime_tick(&mut self, ctx: &egui::Context) {
        if IME_HIDE.swap(false, Ordering::SeqCst) {
            bridge::ime_hide();
        }
        let dialog_wants = ctx.output(|o| o.ime.is_some());
        let want = WANT_KEYBOARD.load(Ordering::SeqCst) || dialog_wants;
        if !want {
            if OVERLAY.load(Ordering::SeqCst) {
                IME_FLUSH.store(true, Ordering::SeqCst);
            }
            return;
        }
        if !OVERLAY.load(Ordering::SeqCst) {
            // A dialog field owns the keyboard: start it empty. Otherwise the command
            // line's own buffer is the line being typed (history recall, a prompt).
            let seed = if ctx.egui_wants_keyboard_input() {
                String::new()
            } else {
                self.0.cmd.buffer.clone()
            };
            ime_reseed(&seed);
            OVERLAY.store(true, Ordering::SeqCst);
            return;
        }
        if !ctx.egui_wants_keyboard_input() {
            // The command line changed without the overlay's doing it (Up recalled a
            // history line, a command completed): show what it holds now.
            let sent = IME_SENT.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if sent != self.0.cmd.buffer {
                ime_reseed(&self.0.cmd.buffer);
            }
        }
        // The first show can land before the view is served and be ignored.
        if FRAMES.load(Ordering::Relaxed).is_multiple_of(30) {
            bridge::ime_show();
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

    /// Long-press injects a right-click — the canvas's context/Enter gesture — which is
    /// the touch equivalent a drawing has (there is no "word" to select). Only while the
    /// keyboard overlay is down, so typing is never disturbed.
    fn track_long_press(&self, ctx: &egui::Context) {
        if OVERLAY.load(Ordering::SeqCst) {
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
            *LONG_CLICK.lock().unwrap_or_else(|e| e.into_inner()) = Some(*pos);
            *tracked = None;
        }
    }
}

/// Inject the pending long-press as a right-click at that position.
#[cfg(target_os = "android")]
fn inject_long_click(raw: &mut egui::RawInput) {
    let Some(pos) = LONG_CLICK
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
    else {
        return;
    };
    use egui::{Event, Modifiers, PointerButton};
    raw.events.push(Event::PointerMoved(pos));
    raw.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Secondary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    raw.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Secondary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
}

/// The floating keyboard toggle: a hand-drawn keyboard glyph (no font or image asset).
/// Sits below the status bar at the right edge, above every panel.
#[cfg(target_os = "android")]
fn keyboard_toggle(ui: &mut egui::Ui, on: bool, dark: bool) -> bool {
    let ctx = ui.ctx().clone();
    let (top_dp, _) = insets::current();
    let zoom = (ctx.pixels_per_point() / insets::display_scale().unwrap_or(TOUCH_DISPLAY_SCALE))
        .max(0.1);
    let offset = egui::vec2(-10.0, top_dp / zoom + 8.0);
    let mut clicked = false;
    egui::Area::new(egui::Id::new("cadcraft_android_keyboard"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::RIGHT_TOP, offset)
        .interactable(true)
        .show(&ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(42.0, 36.0), egui::Sense::click());
            clicked = resp.clicked();
            let bg = if on {
                egui::Color32::from_rgb(0x2f, 0x7d, 0xf6)
            } else if dark {
                egui::Color32::from_rgb(0x33, 0x33, 0x33)
            } else {
                egui::Color32::from_rgb(0xe8, 0xe8, 0xe8)
            };
            let fg = if on {
                egui::Color32::WHITE
            } else if dark {
                egui::Color32::from_rgb(0xdd, 0xdd, 0xdd)
            } else {
                egui::Color32::from_rgb(0x22, 0x22, 0x22)
            };
            let painter = ui.painter();
            painter.rect_filled(rect, 7.0, bg);
            painter.rect_stroke(
                rect,
                7.0,
                egui::Stroke::new(1.0, fg.gamma_multiply(0.55)),
                egui::StrokeKind::Inside,
            );
            // Three rows of keys and a space bar.
            let key = 4.0;
            let gap = 2.5;
            for row in 0..3 {
                let count = if row == 2 { 3 } else { 5 };
                let width = count as f32 * key + (count as f32 - 1.0) * gap;
                let x0 = rect.center().x - width / 2.0;
                let y = rect.top() + 8.0 + row as f32 * (key + gap);
                for i in 0..count {
                    let x = x0 + i as f32 * (key + gap);
                    painter.rect_filled(
                        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(key, key)),
                        1.0,
                        fg,
                    );
                }
            }
            let bar = egui::Rect::from_center_size(
                egui::pos2(rect.center().x, rect.bottom() - 6.0),
                egui::vec2(rect.width() * 0.42, 3.0),
            );
            painter.rect_filled(bar, 1.0, fg);
        });
    if clicked {
        // Nothing else to do: egui gives a click no keyboard focus, so the command
        // line keeps receiving the keys (`egui_wants_keyboard_input` stays false).
    }
    clicked
}

// ---- file picker, shares, mirrors -------------------------------------------

#[cfg(target_os = "android")]
impl CadAndroidApp {
    /// Fulfil system-picker round-trips and open the files that arrived without a path.
    fn poll_picker(&mut self, ctx: &egui::Context) {
        if bridge::take_cancelled() {
            *AWAIT_URI.lock().unwrap_or_else(|e| e.into_inner()) = None;
            eprintln!("cadcraft-android: picker cancelled");
        }
        if let Some((name, bytes)) = bridge::take_open() {
            self.0.open_bytes(&name, &bytes);
            ctx.request_repaint();
        }
        if let Some(uri) = bridge::take_create_uri()
            && let Some(library) = AWAIT_URI
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
        {
            WRITES
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(Write { library, uri, waited: 0 });
        }
        // Shared into the app (SEND) before the first frame.
        let arriving: Vec<(String, Vec<u8>)> = std::mem::take(
            &mut *INBOX.lock().unwrap_or_else(|e| e.into_inner()),
        );
        for (name, bytes) in arriving {
            self.0.open_bytes(&name, &bytes);
            ctx.request_repaint();
        }
        // The engine writes the library file right after `pick_save` returns, but the
        // picker answers later; retry until the bytes are there (or give up).
        let mut writes = WRITES.lock().unwrap_or_else(|e| e.into_inner());
        writes.retain_mut(|write| {
            if write.waited > 600 {
                return false;
            }
            match std::fs::read(&write.library) {
                Ok(bytes) if bridge::write_uri(&write.uri, &bytes) => {
                    eprintln!(
                        "cadcraft-android: wrote {} to picked location",
                        write.library.display()
                    );
                    remember_mirror(write.library.clone(), write.uri.clone());
                    false
                }
                _ => {
                    write.waited += 1;
                    true
                }
            }
        });
    }

    /// Keep the user's picked file in step with its library copy: the app keeps its
    /// document in the library folder (the only file it can rewrite without asking),
    /// so Save and SAVEAS after the first pick still reach the user's own file.
    fn mirror_tick(&self) {
        let mut mirrors = MIRROR.lock().unwrap_or_else(|e| e.into_inner());
        mirrors.retain_mut(|mirror| {
            if !mirror.library.exists() {
                return false;
            }
            let now = file_stamp(&mirror.library);
            if now == mirror.stamp || now == (0, 0) {
                return true;
            }
            let Ok(bytes) = std::fs::read(&mirror.library) else { return true };
            if bridge::write_uri(&mirror.uri, &bytes) {
                mirror.stamp = now;
            }
            true
        });
    }
}

#[cfg(target_os = "android")]
fn remember_mirror(library: PathBuf, uri: String) {
    let stamp = file_stamp(&library);
    let mut mirrors = MIRROR.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = mirrors.iter_mut().find(|m| m.library == library) {
        existing.uri = uri;
        existing.stamp = stamp;
        return;
    }
    if mirrors.len() >= MAX_MIRRORS {
        mirrors.remove(0);
    }
    mirrors.push(Mirror { library, uri, stamp });
}

// ---- safe-area chrome -------------------------------------------------------

#[cfg(target_os = "android")]
impl CadAndroidApp {
    /// Safe-area spacers that dock above/below the app chrome. Called at the top of
    /// `ui()` so the spacers reserve space before shared panels draw.
    fn apply_mobile_chrome(&self, ctx: &egui::Context, ui: &mut egui::Ui) {
        // OPAQUE spacers in the bar color (black in dark mode, white in light): on API
        // 35+ the system ignores setStatusBarColor (edge-to-edge is enforced), so the
        // bar background is drawn by us showing through the transparent system bars.
        let bar = if self.0.shown_theme() == egui::Theme::Dark {
            egui::Color32::BLACK
        } else {
            egui::Color32::WHITE
        };
        // Spacer heights are points, and a point is `interface scale ÷ display scale`
        // physical pixels, so divide the dp insets by that ratio to keep the system bars
        // their physical size whatever the interface scale is.
        let (top_dp, bottom_dp) = insets::current();
        let zoom = (ctx.pixels_per_point() / insets::display_scale().unwrap_or(TOUCH_DISPLAY_SCALE))
            .max(0.1);
        let frame = egui::Frame::NONE.fill(bar);
        egui::Panel::top("cadcraft_android_safe_top")
            .exact_size(top_dp / zoom)
            .frame(frame)
            .show_separator_line(false)
            .resizable(false)
            .show(ui, |_ui| {});
        egui::Panel::bottom("cadcraft_android_safe_bottom")
            .exact_size(bottom_dp / zoom)
            .frame(frame)
            .show_separator_line(false)
            .resizable(false)
            .show(ui, |_ui| {});
    }
}

#[cfg(not(target_os = "android"))]
impl CadAndroidApp {
    fn apply_mobile_chrome(&self, _ctx: &egui::Context, _ui: &mut egui::Ui) {}
}

/// The Android logger: `log::` records go to standard error, which the platform writes to
/// logcat (tag `Rustina Draw`). The desktop app writes the same records to a file instead
/// (`apps/cadcraft/src/logging.rs`), so every `log::warn!`/`log::error!` in the workspace
/// (the GPU canvas set-up, a broken app icon) is visible in both.
///
/// Levels follow the desktop default filter (`warn,cadcraft*=info`): warnings and errors from
/// everything, plus `info` from CADCraft's own crates. Logging never panics.
#[cfg(target_os = "android")]
struct StderrLogger;

#[cfg(target_os = "android")]
impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        match metadata.level() {
            log::Level::Error | log::Level::Warn => true,
            log::Level::Info => metadata.target().starts_with("cadcraft"),
            log::Level::Debug | log::Level::Trace => false,
        }
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] {}: {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

/// Android entry point: eframe drives the NativeActivity event loop.
///
/// winit's Android event loop is single-use per process, but Android can re-run
/// `android_main` in the *same* process. A second `run_native` would die with
/// `WinitEventLoop(RecreationAttempt)`, so the first entry claims a flag and exits the
/// process when its loop ends; re-entries exit at once.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        std::process::exit(0);
    }
    // `log` records go to stderr, which Android writes to logcat under the app's tag;
    // the desktop app writes them to a file instead (see its `logging` module).
    if log::set_logger(&StderrLogger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    insets::init(&app);
    bridge::init(&app);
    // Internal storage is private to the app; it is `$HOME` for the settings files and
    // for anything else that reads the variable. The drawing fonts are compiled in
    // (CRAFT_FONTS_DIR), so nothing has to be seeded into `$HOME/.fonts`.
    // SAFETY: runs once on the `android_main` thread before eframe starts any threads,
    // and no other thread reads the process environment meanwhile.
    if let Some(home) = app.internal_data_path() {
        unsafe { std::env::set_var("HOME", &home) };
    }
    // App library: external files dir (user-visible over MTP, no permission).
    let library = app
        .external_data_path()
        .or_else(|| app.internal_data_path())
        .map(|p| p.join("documents"));
    if let Some(dir) = library {
        std::fs::create_dir_all(&dir).ok();
        LIB_DIR.set(dir).ok();
    }
    // A file shared *to* us opens through the inbox after the first frame.
    if let Some((name, bytes)) = bridge::inbound_share() {
        eprintln!(
            "cadcraft-android: inbound share {name} ({} bytes)",
            bytes.len()
        );
        INBOX
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((name, bytes));
    }
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "Rustina Draw",
        options,
        Box::new(|cc| Ok(Box::new(CadAndroidApp::new(cc)))),
    );
    std::process::exit(0);
}
