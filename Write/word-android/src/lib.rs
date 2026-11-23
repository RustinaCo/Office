//! WordCraft on Android: the same engine (`wordcraft-engine`) and the same egui
//! frontend (`wordcraft-ui-egui`) as the desktop app, driven by eframe's Android
//! (NativeActivity) backend.
//!
//! Platform services follow the `apps/wordcraft-web` pattern without touching
//! the engine:
//! - open/picture/font: `open_async` raises the system file manager (SAF);
//!   bytes land in `inbox`, which the app routes by kind (document opens,
//!   pictures insert, fonts register).
//! - save: `pick_save` returns a library path at once (dialogs complete
//!   instantly); the picked user location is fulfilled when its URI arrives.
//! - export/download: staged in the library, then the user picks the
//!   destination in the system picker.
//! - clipboard: text put/get over JNI (copies are mirrored from egui output).
//! - typing: a transparent Android `EditText` overlay (real keyboard with
//!   suggestions) feeds `ImeEvent::Preedit`/`Commit` through `raw_input_hook`,
//!   i.e. the app's own IME pipeline with proper undo.
//! - dark pages default on (the app's own dark-page mode; exports stay light).
//! No runtime permissions are needed for any of this.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "android")]
use std::sync::{Mutex, OnceLock};

use wordcraft_engine::Session;
use wordcraft_ui_egui::{Inbox, Services, WordApp};

#[cfg(target_os = "android")]
mod bridge;
#[cfg(target_os = "android")]
mod fonts;
#[cfg(target_os = "android")]
mod insets;

/// The phone's touch interface size, as WordCraft's absolute "Interface size"
/// preference takes it (#822): the sibling apps' 135 % of the display's own
/// scale, which is what makes the phone UI (bars, fonts, start screen, dialogs)
/// the same size theirs is.
#[cfg(target_os = "android")]
const TOUCH_INTERFACE_SCALE_PERCENT: u16 = 135;
/// Display scale assumed while the platform query has not answered.
#[cfg(target_os = "android")]
const TOUCH_DISPLAY_SCALE: f32 = 2.0;

/// Picked files waiting for the app router.
#[cfg(target_os = "android")]
static INBOX: OnceLock<Inbox> = OnceLock::new();
/// App library dir (`.../Android/data/<pkg>/files/documents`), permission-free.
#[cfg(target_os = "android")]
static LIB_DIR: OnceLock<PathBuf> = OnceLock::new();
/// A SAF (system file manager) save/export waiting for its destination URI.
#[cfg(target_os = "android")]
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
/// The IME overlay is focused for the document.
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
/// A synthetic double-click to inject on the next `raw_input_hook`.
#[cfg(target_os = "android")]
static DOUBLE_CLICK: Mutex<Option<egui::Pos2>> = Mutex::new(None);
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

/// The library file the last download wrote: the app's own copy of the document, whose path
/// the app takes as the document's file (`Services::saved_path`).
#[cfg(target_os = "android")]
static LAST_LIBRARY: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The copy the user picked a destination for: library file → its URI → the stamp last written
/// there, so later saves of that library file (AutoSave included) reach the user's own file too.
#[cfg(target_os = "android")]
static MIRROR: Mutex<Option<Mirror>> = Mutex::new(None);

#[cfg(target_os = "android")]
struct Mirror {
    library: PathBuf,
    uri: String,
    stamp: (u64, u64),
}

/// Length and modification time of a file, as a cheap change stamp; `(0, 0)` when it can't be
/// read (a mirror then just skips this pass).
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

#[cfg(target_os = "android")]
fn last_library() -> Option<String> {
    LAST_LIBRARY.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|p| p.to_string_lossy().into_owned())
}

/// Write the user's picked file again when its library copy changed: the app keeps its document in
/// the library folder (the only file it can rewrite without asking), and the file the user chose is
/// kept in step, so Save and AutoSave both reach the document they made.
#[cfg(target_os = "android")]
fn mirror_tick() {
    let (library, uri, stamp) = {
        let m = MIRROR.lock().unwrap_or_else(|e| e.into_inner());
        match m.as_ref() {
            Some(m) => (m.library.clone(), m.uri.clone(), m.stamp),
            None => return,
        }
    };
    let now = file_stamp(&library);
    if now == stamp {
        return;
    }
    let Ok(bytes) = std::fs::read(&library) else { return };
    if bridge::write_uri(&uri, &bytes)
        && let Some(m) = MIRROR.lock().unwrap_or_else(|e| e.into_inner()).as_mut()
    {
        m.stamp = now;
    }
}

fn inbox() -> Inbox {
    #[cfg(target_os = "android")]
    return INBOX.get_or_init(Default::default).clone();
    #[cfg(not(target_os = "android"))]
    return Inbox::default();
}

/// Settings dir (`ui.json`, same layout as desktop): app-private internal
/// storage, permission-free, survives restarts.
#[cfg(target_os = "android")]
fn settings_dir() -> Option<PathBuf> {
    if std::env::var_os("WORDCRAFT_NO_PREFS").is_some() {
        return None;
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("wordcraft"))
}

#[cfg(target_os = "android")]
fn load_prefs(app: &mut WordApp) -> bool {
    let Some(dir) = settings_dir() else { return false };
    let Ok(bytes) = std::fs::read(dir.join("ui.json")) else { return false };
    let Some(ui) = wordcraft_ui_egui::UiState::from_json(&bytes) else { return false };
    app.apply_prefs(ui);
    true
}

#[cfg(target_os = "android")]
fn save_prefs(app: &WordApp) {
    let Some(dir) = settings_dir() else { return };
    std::fs::create_dir_all(&dir).ok();
    if let Ok(b) = serde_json::to_vec(&app.prefs()) {
        let _ = std::fs::write(dir.join("ui.json"), &b);
        *LAST_SAVED.lock().unwrap_or_else(|e| e.into_inner()) = Some(b);
    }
}

pub struct WordAndroidApp(WordApp);

impl WordAndroidApp {
    pub fn new(_cc: &eframe::CreationContext) -> Self {
        let region = wordcraft_ui_egui::i18n::system_region();
        let doc = wordcraft_engine::blank_document(&region);
        let mut session = Session::new(doc);
        session.system_region = region;
        #[cfg(target_os = "android")]
        let services = android_services();
        #[cfg(not(target_os = "android"))]
        let services = Services::default();
        let mut app = WordApp::new(session, services);
        #[cfg(target_os = "android")]
        {
            // Saved settings win (theme, interface size, dark pages, ...);
            // first run gets the phone interface size through the app's own
            // preference (it owns the egui zoom factor) and dark pages by
            // default, like Word's dark-page mode. Exports always render
            // light (engine-side).
            let first_run = !load_prefs(&mut app);
            if first_run {
                app.ui.dark_page = true;
            }
            // Early Android builds stored a bare 135 %, which draws the whole
            // UI a third of the touch size on a phone (the preference is an
            // absolute pixels-per-point, #822); that value migrates too.
            if let Some(size) = touch_interface_size()
                && (first_run || app.ui.interface_size.percent() == Some(TOUCH_INTERFACE_SCALE_PERCENT))
            {
                app.ui.interface_size = size;
            }
            // Fonts the user added from their own files (#582) live in the app's settings folder:
            // load them, and store fonts added later there, so an added font lasts forever on a
            // phone too (there is no system font folder the user could drop one into).
            if let Some(dir) = settings_dir().map(|d| d.join("fonts")) {
                wordcraft_ui_egui::load_user_fonts(&dir);
            }
        }
        WordAndroidApp(app)
    }
}

/// The phone's touch interface size as WordCraft's preference takes it: the
/// display's own scale × 135 % (the sibling apps' touch size, see
/// [`TOUCH_INTERFACE_SCALE_PERCENT`]). `None` only if the size is refused.
#[cfg(target_os = "android")]
fn touch_interface_size() -> Option<wordcraft_ui_egui::theme::InterfaceSize> {
    let scale = f64::from(insets::display_scale().unwrap_or(TOUCH_DISPLAY_SCALE));
    wordcraft_ui_egui::theme::InterfaceSize::from_percent(scale * f64::from(TOUCH_INTERFACE_SCALE_PERCENT))
}

#[cfg(target_os = "android")]
fn android_services() -> Services {
    Services {
        inbox: Some(inbox()),
        // System file manager; the app routes inbox bytes by kind (document,
        // picture, font). The `kind` only selects the picker's filter.
        open_async: Some(Box::new(|kind| {
            bridge::launch_open(open_mimes(kind));
        })),
        pick_save: Some(Box::new(|suggested| android_pick_save(suggested))),
        download: Some(Box::new(|name, bytes| android_download(name, bytes))),
        // The copy a save just wrote in the app's own folder: a real, writable path, so the
        // document takes it as its own file and Save/AutoSave keep writing there (the user's
        // picked destination is kept up to date by the mirror instead).
        saved_path: Some(Box::new(|_name| last_library())),
        clipboard_text: Some(Box::new(|| bridge::get_text())),
        ..Default::default()
    }
}

#[cfg(target_os = "android")]
fn open_mimes(kind: &str) -> &'static str {
    match kind {
        "picture" => "image/png,image/jpeg,image/gif,image/webp,image/bmp",
        "font" => "font/ttf,font/otf,application/font-sfnt,application/font-woff",
        _ => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document,text/plain,application/pdf,image/png,image/jpeg"
        }
    }
}

/// Provisional library save + system picker for the user-visible copy.
/// Always returns a path so dialogs complete instantly; the picker result
/// only decides the second, user-facing write.
#[cfg(target_os = "android")]
fn android_pick_save(suggested: &str) -> Option<String> {
    let dir = LIB_DIR.get()?;
    std::fs::create_dir_all(dir).ok()?;
    let path = Path::new(suggested);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_else(|| "docx".to_string());
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(sanitize_stem)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Document1".to_string());
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
fn android_download(name: &str, bytes: &[u8]) -> Result<(), String> {
    let dir = LIB_DIR.get().ok_or_else(|| "no library dir".to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
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
        .unwrap_or_else(|| "Document1".to_string());
    let library = unique_path(dir, &stem, &ext);
    std::fs::write(&library, bytes).map_err(|e| e.to_string())?;
    bridge::launch_create(mime_for_ext(&ext), &format!("{stem}.{ext}"));
    *LAST_LIBRARY.lock().unwrap_or_else(|e| e.into_inner()) = Some(library.clone());
    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Pending::WriteUri { library });
    Ok(())
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
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "txt" => "text/plain",
        "html" | "htm" => "text/html",
        _ => "application/octet-stream",
    }
}

impl eframe::App for WordAndroidApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.logic(ctx);
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.0.raw_input_hook(raw);
        #[cfg(target_os = "android")]
        drain_ime_queue(raw);
        #[cfg(target_os = "android")]
        inject_double_click(raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_mobile_chrome(&ctx, ui);
        // NOTE: the app runs BEFORE the block below: canvas reports IME
        // state through ctx.output(), which must be fresh (same pass), not
        // the taken output of the previous pass.
        self.0.ui(ui);
        #[cfg(target_os = "android")]
        {
            // Solid system bars matching the *rendered* UI theme (the app has
            // its own light/dark state, independent of the OS setting).
            // Re-applied on change and periodically (an early call can land
            // before the window is ready). Never hidden.
            let dark = wordcraft_ui_egui::theme::Tokens::get(&ctx).dark;
            let n = FRAMES.fetch_add(1, Ordering::Relaxed);
            if !BARS_SET.load(Ordering::SeqCst)
                || BARS_DARK.load(Ordering::SeqCst) != dark
                || n % 60 == 0
            {
                bridge::style_bars(dark);
                BARS_DARK.store(dark, Ordering::SeqCst);
                BARS_SET.store(true, Ordering::SeqCst);
            }
            // Live inset re-query: startup runs before the decor view is
            // attached, so converge once the window is live.
            if n % 60 == 0
                && let Some((top, bottom)) = bridge::system_insets()
            {
                insets::refresh(top, bottom);
            }
            // Keep the keyboard up while text entry is actually wanted: the
            // first show can land before the view is served and get ignored.
            // Gating on the app's live IME request (not just the overlay flag)
            // matters: after a toolbar tap the overlay is still up for a few
            // frames, and re-asserting it there is what made the keyboard
            // close and pop straight back up.
            let ime_wanted = ctx.output(|o| o.ime.is_some());
            if OVERLAY.load(Ordering::SeqCst) && ime_wanted && n % 30 == 0 {
                bridge::ime_show();
            }
            self.poll_picker();
            mirror_tick();
            self.ime_poll_overlay(&ctx);
            self.mirror_copies(&ctx);
            // Persist settings whenever they change (two tiny JSON writes;
            // no time throttle: idle frames may never come, e.g. the user
            // force-stops right after toggling the theme).
                let changed = LAST_SAVED
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .is_none_or(|last| {
                        serde_json::to_vec(&self.0.prefs()).ok().is_some_and(|b| *last != b)
                    });
                if changed {
                    save_prefs(&self.0);
                }
            self.track_long_press(&ctx);
        }
    }
}

/// IME overlay lifecycle + composing feed. The overlay (a real `EditText`)
/// owns suggestions/autocorrect; diffs travel as `ImeEvent::Preedit`
/// (live display) and the final text as `ImeEvent::Commit` (one undo step)
/// through `raw_input_hook` — the app's own IME pipeline.
#[cfg(target_os = "android")]
static IME_TEXT: Mutex<String> = Mutex::new(String::new());
#[cfg(target_os = "android")]
static IME_COMMIT: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "android")]
impl WordAndroidApp {
    fn ime_poll_overlay(&self, ctx: &egui::Context) {
        let ime_active = ctx.output(|o| o.ime.is_some());
        if ime_active && !OVERLAY.load(Ordering::SeqCst) {
            bridge::ime_focus("");
            *IME_TEXT.lock().unwrap_or_else(|e| e.into_inner()) = String::new();
            IME_COMMIT.store(false, Ordering::SeqCst);
            OVERLAY.store(true, Ordering::SeqCst);
            return;
        }
        if !OVERLAY.load(Ordering::SeqCst) {
            return;
        }
        if bridge::ime_take_done() {
            // Done on the keyboard: commit the draft at once.
            IME_COMMIT.store(true, Ordering::SeqCst);
            bridge::ime_hide();
            OVERLAY.store(false, Ordering::SeqCst);
            return;
        }
        // Focus left the text: hide only after it stays away — a single
        // stray frame (dialog, popup, rotation) must not flash-hide the
        // keyboard and strand the composition.
        static IDLE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        if ime_active {
            IDLE.store(0, Ordering::Relaxed);
            return;
        }
        if IDLE.fetch_add(1, Ordering::Relaxed) >= 10 {
            IDLE.store(0, Ordering::Relaxed);
            IME_COMMIT.store(true, Ordering::SeqCst);
            bridge::ime_hide();
            OVERLAY.store(false, Ordering::SeqCst);
        }
    }

    /// Long-press with document text focused selects the word under the
    /// finger, like Word: track the press here (read-only over input events),
    /// inject the double-click in `raw_input_hook` (next frame). Only while
    /// the canvas owns the keyboard, so ribbon long-presses are untouched.
    fn track_long_press(&self, ctx: &egui::Context) {
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
            *DOUBLE_CLICK.lock().unwrap_or_else(|e| e.into_inner()) = Some(*pos);
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

/// Feed pending IME state into the event stream (runs after the app's own
/// `raw_input_hook`, before the frame consumes input).
///
/// Keystrokes commit INCREMENTALLY: the overlay text is diffed against what
/// was already sent (`IME_TEXT`), appended characters commit at once and
/// deletions become Backspaces, so every key lands in the document
/// immediately (with proper undo) instead of pooling invisibly until Done.
/// Autocorrect-style replacements diff down to backspaces + suffix too, so no
/// separate composing state (and no duplication on Done) is needed.
#[cfg(target_os = "android")]
fn drain_ime_queue(raw: &mut egui::RawInput) {
    if !OVERLAY.load(Ordering::SeqCst) && !IME_COMMIT.load(Ordering::SeqCst) {
        return;
    }
    if IME_COMMIT.load(Ordering::SeqCst) {
        // Done on the keyboard, or focus left the text: flush whatever is
        // still unsent, diffed so already-committed text is never repeated.
        let overlay = bridge::ime_poll();
        commit_diff(raw, &overlay);
        IME_COMMIT.store(false, Ordering::SeqCst);
        return;
    }
    let overlay = bridge::ime_poll();
    commit_diff(raw, &overlay);
}

/// Diff `overlay` against sent text and push the delta (Backspaces +
/// an `Ime::Commit` suffix). Updates the sent watermark.
#[cfg(target_os = "android")]
fn commit_diff(raw: &mut egui::RawInput, overlay: &str) {
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
        raw.events.push(egui::Event::Key {
            key: egui::Key::Backspace,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        });
    }
    if !suffix.is_empty() {
        raw.events
            .push(egui::Event::Ime(egui::ImeEvent::Commit(suffix.to_string())));
    }
    *sent = overlay.to_string();
}

/// Fulfill system-picker round-trips: picked files go to the inbox (the app
/// routes by kind), picked save destinations receive library bytes.
#[cfg(target_os = "android")]
impl WordAndroidApp {
    fn poll_picker(&mut self) {
        if bridge::take_cancelled() {
            *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = None;
            eprintln!("word-android: picker cancelled");
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
                            "word-android: wrote {} to picked location",
                            library.display()
                        );
                        // Remember where the user's own copy went: later saves of this library
                        // file (Save, AutoSave) follow it there.
                        let stamp = file_stamp(&library);
                        *MIRROR.lock().unwrap_or_else(|e| e.into_inner()) = Some(Mirror { library, uri, stamp });
                    }
                    _ => eprintln!("word-android: write to picked location failed"),
                }
            }
        }
    }
}

#[cfg(target_os = "android")]
impl WordAndroidApp {
    /// Safe-area spacers that dock above/below the app chrome. Called at the
    /// top of `ui()` so the spacers reserve space before shared panels draw.
    fn apply_mobile_chrome(&self, ctx: &egui::Context, ui: &mut egui::Ui) {
        // OPAQUE spacers in the bar color (black in dark mode, white in
        // light): on API 35+ the system ignores setStatusBarColor
        // (edge-to-edge is enforced), so the bar background is drawn by us
        // showing through the transparent system bars.
        let bar = if wordcraft_ui_egui::theme::Tokens::get(ctx).dark {
            egui::Color32::BLACK
        } else {
            egui::Color32::WHITE
        };
        // Spacer heights are points, and a point is `interface size ÷ display
        // scale` physical pixels, so divide the dp insets by that ratio to keep
        // the system bars their physical size whatever the interface size is.
        let (top_dp, bottom_dp) = insets::current();
        let zoom = (ctx.pixels_per_point() / insets::display_scale().unwrap_or(TOUCH_DISPLAY_SCALE)).max(0.1);
        let frame = egui::Frame::NONE.fill(bar);
        egui::Panel::top("word_android_safe_top")
            .exact_size(top_dp / zoom)
            .frame(frame)
            .show_separator_line(false)
            .resizable(false)
            .show(ui, |_ui| {});
        egui::Panel::bottom("word_android_safe_bottom")
            .exact_size(bottom_dp / zoom)
            .frame(frame)
            .show_separator_line(false)
            .resizable(false)
            .show(ui, |_ui| {});
    }
}

#[cfg(not(target_os = "android"))]
impl WordAndroidApp {
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
        .map(|p| p.join("documents"));
    if let Some(dir) = lib {
        std::fs::create_dir_all(&dir).ok();
        LIB_DIR.set(dir).ok();
    }
    // A file shared *to* us opens through the inbox router.
    if let Some((name, bytes)) = bridge::inbound_share() {
        eprintln!("word-android: inbound share {name} ({} bytes)", bytes.len());
        inbox().lock().map(|mut i| i.push((name, bytes))).ok();
    }
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "WordCraft",
        options,
        Box::new(|cc| Ok(Box::new(WordAndroidApp::new(cc)))),
    );
    std::process::exit(0);
}

/// Inject the pending synthetic double-click (two press/release pairs at one
/// position in a single pass reads as a double-click), which the canvas turns
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
