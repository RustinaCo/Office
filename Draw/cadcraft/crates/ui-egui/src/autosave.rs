//! Autosave and Drawing Recovery in the app. The engine decides what to write and writes it
//! ([`cadcraft_engine::autosave`]); this runs it every frame, keeps SAVETIME and SAVEFILEPATH with
//! the preferences, and shows the Drawing Recovery list after a crash: each autosaved drawing can
//! be opened (as an unsaved copy) or discarded. On the web there is no autosave yet.

use cadcraft_engine::autosave::{self, Recoverable};
use egui::{RichText, vec2};
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

/// The host storage key for [`prefs_json`].
pub const PREFS_KEY: &str = "cadcraft.autosave";

/// SAVETIME and SAVEFILEPATH as JSON for the host's storage.
pub fn prefs_json() -> String {
    json!({ "savetime": autosave::savetime(), "saveFilePath": autosave::savefilepath() }).to_string()
}

/// Restore what [`prefs_json`] saved; malformed values are ignored.
pub fn load_prefs(json: &str) {
    let Ok(v) = serde_json::from_str::<Value>(json) else { return };
    if let Some(m) = v.get("savetime").and_then(Value::as_u64) {
        autosave::set_savetime(u32::try_from(m).unwrap_or(autosave::MAX_SAVETIME));
    }
    if let Some(p) = v.get("saveFilePath").and_then(Value::as_str) {
        autosave::set_savefilepath(p);
    }
}

/// What the GPU error notice says (see [`gpu_notice`]).
#[derive(Clone, Debug, PartialEq)]
pub struct GpuNotice {
    pub fault: crate::gpu::GpuFault,
    /// Drawings autosaved because of it (0: nothing unsaved, or autosave is off).
    pub autosaved: usize,
}

/// Drawing Recovery: autosave files left behind by an app that stopped, listed once at start.
#[derive(Clone, Debug, Default)]
pub struct Recovery {
    pub items: Vec<Recoverable>,
}

impl CadApp {
    /// Turn autosave on (desktop). Files go to SAVEFILEPATH, or to `default_dir` when it isn't
    /// set; files a crashed app left there are listed for Drawing Recovery.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_autosave(&mut self, default_dir: &std::path::Path) {
        if autosave::savefilepath().is_empty() {
            autosave::set_savefilepath(&default_dir.to_string_lossy());
        }
        let saver = autosave::Autosaver::new(autosave::instance_id());
        let dir = std::path::PathBuf::from(autosave::savefilepath());
        autosave::prune_locks(&dir);
        self.recovery.items = autosave::recoverable(&dir, &saver.instance);
        self.autosave = Some(saver);
    }

    /// Open recovery item `i` as an unsaved copy. Its autosave file stays until that drawing is
    /// saved or closed, so a second crash doesn't lose it.
    pub fn recover_item(&mut self, i: usize) -> Result<(), String> {
        let item = self.recovery.items.get(i).cloned().ok_or("no such recovery file")?;
        let r = self.run("recover", json!({ "path": item.path, "copy": true, "title": item.drawing }))?;
        self.recovery.items.remove(i);
        if let (Some(saver), Some(uid)) = (&mut self.autosave, r.get("uid").and_then(Value::as_u64)) {
            saver.adopt(&self.session, uid, &item.path);
        }
        self.ui.start_tab = false;
        self.canvas.zoom_pending = true;
        Ok(())
    }

    /// Delete recovery item `i`'s file.
    pub fn discard_item(&mut self, i: usize) {
        if i < self.recovery.items.len() {
            let _item = self.recovery.items.remove(i);
            #[cfg(not(target_arch = "wasm32"))]
            autosave::discard(&_item.path);
        }
    }

    /// Every frame: autosave what is due, and wake up when the next one is.
    pub fn autosave_tick(&mut self, _ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(wait) = self.autosave.as_mut().and_then(|s| s.tick(&self.session)) {
            _ctx.request_repaint_after(wait);
        }
    }

    /// Every frame: after a GPU error or device loss, autosave unsaved drawings at once (the
    /// session may not survive it) and show [`gpu_notice`].
    pub fn gpu_fault_tick(&mut self, ctx: &egui::Context) {
        let Some(fault) = self.gpu_faults.take() else { return };
        #[cfg(not(target_arch = "wasm32"))]
        let saved = self.autosave.as_mut().map_or(0, |a| a.save_now(&self.session));
        #[cfg(target_arch = "wasm32")]
        let saved = 0;
        let autosaved = self.gpu_notice.as_ref().map_or(0, |n| n.autosaved) + saved;
        self.gpu_notice = Some(GpuNotice { fault, autosaved });
        ctx.request_repaint();
    }

    /// The app closes normally: this app's autosave files are not needed any more.
    pub fn shutdown_autosave(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(s) = &mut self.autosave {
            s.shutdown();
        }
    }
}

/// `secs` since 1970 as `YYYY-MM-DD HH:MM` (UTC).
fn utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let (h, m) = ((secs % 86_400) / 3600, (secs % 3600) / 60);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}")
}

/// The Drawing Recovery window, while autosave files from a crash are waiting.
pub fn show(app: &mut CadApp, ctx: &egui::Context) {
    if app.recovery.items.is_empty() {
        return;
    }
    let t = Tokens::get();
    let mut open = None;
    let mut discard = None;
    let mut later = false;
    egui::Modal::new(egui::Id::new("cc_drawing_recovery")).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.label(RichText::new("Drawing Recovery").strong());
        ui.add_space(4.0);
        ui.label("CADCraft did not close normally. These drawings were autosaved:");
        ui.add_space(6.0);
        egui::Grid::new("cc_recovery_list").num_columns(4).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
            for (i, item) in app.recovery.items.iter().enumerate() {
                ui.label(&item.drawing);
                ui.label(RichText::new(format!("{} UTC · {} KB", utc(item.modified), item.bytes.div_ceil(1024))).color(t.text_dim));
                if ui.button("Open").clicked() {
                    open = Some(i);
                }
                if ui.button("Discard").clicked() {
                    discard = Some(i);
                }
                ui.end_row();
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Opened drawings have no file name: save them to keep them.").color(t.text_dim));
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Later").on_hover_text("Keep the files and ask again next time").clicked() {
                later = true;
            }
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            later = true;
        }
    });
    if let Some(i) = open
        && let Err(e) = app.recover_item(i)
    {
        app.set_status(e);
    }
    if let Some(i) = discard {
        app.discard_item(i);
    }
    if later {
        app.recovery.items.clear();
    }
    if open.is_some() || discard.is_some() || later {
        ctx.request_repaint();
    }
}

/// The notice after a GPU error or device loss: what happened, what was autosaved, and what to
/// do. Not modal, so drawings can still be saved while the device works.
pub fn gpu_notice(app: &mut CadApp, ctx: &egui::Context) {
    let Some(n) = &app.gpu_notice else { return };
    let t = Tokens::get();
    let mut dismiss = false;
    egui::Window::new("Graphics error")
        .id(egui::Id::new("cc_gpu_notice"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 48.0))
        .show(ctx, |ui| {
            ui.set_width(440.0);
            if n.fault.lost {
                ui.label("The graphics device stopped working (a driver reset or a GPU that stopped responding). CADCraft can't draw again until it restarts.");
            } else {
                let times = if n.fault.errors == 1 { String::new() } else { format!(" ({} times)", n.fault.errors) };
                ui.label(format!("The graphics driver reported an error{times}. CADCraft kept running, but the drawing may not display correctly."));
            }
            ui.add_space(4.0);
            match n.autosaved {
                0 => ui.label("Nothing needed autosaving."),
                1 => ui.label("A recovery copy of the open drawing with unsaved changes was autosaved."),
                k => ui.label(format!("Recovery copies of {k} drawings with unsaved changes were autosaved.")),
            };
            ui.label(RichText::new("Save your work and restart CADCraft. Recovery copies are offered at the next start if needed.").strong());
            ui.add_space(4.0);
            ui.collapsing("Details", |ui| {
                ui.label(RichText::new(&n.fault.message).color(t.text_dim).monospace());
            });
            // A row just as tall as the button (a plain right-to-left layout takes the rest).
            ui.allocate_ui_with_layout(vec2(ui.available_width(), 0.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Dismiss").clicked() {
                    dismiss = true;
                }
            });
        });
    if dismiss {
        app.gpu_notice = None;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn utc_dates() {
        assert_eq!(super::utc(0), "1970-01-01 00:00");
        assert_eq!(super::utc(951_782_400 + 3_660), "2000-02-29 01:01");
        assert_eq!(super::utc(1_791_676_800), "2026-10-11 00:00");
    }
}
