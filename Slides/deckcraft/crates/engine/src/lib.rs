//! The DeckCraft engine façade.
//!
//! Every user-visible action is a command with a stable id (`slide.new`, `shape.insert`,
//! `text.insert`, `format.bold`…) and JSON parameters. The egui UI, the CLI, the control channel
//! and MCP all go through [`Session::execute`]. Pointer tools reduce to commands too, so every
//! gesture is journaled, undoable and replayable.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod cmd;
pub mod connect;
pub mod fonts;
pub mod guard;
pub mod keys;
pub mod links;
pub mod recovery;
pub mod sample;
pub mod tools;

use std::sync::Arc;

use deckcraft_model::{Presentation, Shape, ShapeId, Slide, SlideId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use cmd::{CommandInfo, CommandSpec, command_specs, find_command};
pub use deckcraft_geom as geom;
pub use deckcraft_media as media;
pub use deckcraft_model as model;
pub use deckcraft_render as render;
pub use tools::{Mods, PointerEvent, PointerKind, ToolKind};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("command `{0}` is not available right now: {1}")]
    Disabled(String, String),
    #[error("invalid parameters for `{cmd}`: {msg}")]
    BadParams { cmd: String, msg: String },
    #[error("no active presentation")]
    NoDocument,
    #[error("{0}")]
    Other(String),
    #[error("internal error in `{0}` (the presentation was kept as it was): {1}")]
    Internal(String, String),
}

impl From<deckcraft_model::ModelError> for EngineError {
    fn from(e: deckcraft_model::ModelError) -> Self {
        EngineError::Other(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;

/// Text being edited in place: a shape's body (or a table cell, or the slide's notes).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSel {
    pub shape: ShapeId,
    /// Selection anchor and caret: (paragraph, char).
    pub anchor: (usize, usize),
    pub caret: (usize, usize),
    /// Editing a table cell (row, col).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<(usize, usize)>,
    /// Editing the slide's speaker notes (shape is ignored).
    #[serde(default)]
    pub notes: bool,
    /// The caret position that sits at the end of a wrapped line (after End) rather than at the
    /// start of the next line. Ignored once the caret moves anywhere else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_end: Option<(usize, usize)>,
}

impl TextSel {
    pub fn is_range(&self) -> bool {
        self.anchor != self.caret
    }
    pub fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.caret { (self.anchor, self.caret) } else { (self.caret, self.anchor) }
    }
    /// Is the caret drawn at the end of the line before (upstream), not the start of the next?
    pub fn at_line_end(&self) -> bool {
        self.line_end == Some(self.caret)
    }
}

/// Which shape tree is being edited: slides (normal), or a master/layout (Slide Master view).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Target {
    #[default]
    Slides,
    Master {
        master: usize,
    },
    Layout {
        master: usize,
        layout: usize,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    /// Current slide index.
    pub slide: usize,
    /// Slides selected in the thumbnail pane / sorter (includes the current one when non-empty).
    pub slides: Vec<SlideId>,
    /// Selected shapes on the current slide (top-level or inside groups).
    pub shapes: Vec<ShapeId>,
    pub text: Option<TextSel>,
    /// Selected table cells: (table, (r0, c0), (r1, c1)).
    pub cells: Option<(ShapeId, (usize, usize), (usize, usize))>,
    pub target: Target,
}

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub label: String,
    pub doc: Arc<Presentation>,
    pub selection: Selection,
}

#[derive(Clone, Debug, Default)]
pub struct History {
    pub undo: Vec<HistoryEntry>,
    pub redo: Vec<HistoryEntry>,
    pub limit: usize,
}

/// A live pointer interaction: the document before it began (one undo step at the end).
#[derive(Clone, Debug)]
pub struct Interaction {
    pub label: String,
    pub doc: Arc<Presentation>,
    pub selection: Selection,
}

/// Text an input method is composing (#101): its preedit sits in the edited text body, at `start`
/// for `len` characters, until it is committed, replaced or cancelled. The presentation from
/// before it began makes the whole composition one undo step.
#[derive(Clone, Debug)]
pub struct Composition {
    /// Where the composed text starts: (paragraph, char).
    pub start: (usize, usize),
    /// Its length in characters (it never spans paragraphs).
    pub len: usize,
    /// The part the input method is converting, relative to `start` (drawn with a thicker line).
    pub active: Option<(usize, usize)>,
    /// The formatting the composed text is typed with (as `text.insert` would choose).
    pub props: Option<deckcraft_model::text::RunProps>,
    /// The presentation and selection before the composition began.
    pub before: Arc<Presentation>,
    pub before_selection: Selection,
}

#[derive(Clone, Debug)]
pub struct DocState {
    pub doc: Arc<Presentation>,
    pub selection: Selection,
    pub history: History,
    pub path: Option<String>,
    pub revision: u64,
    pub saved_doc: Arc<Presentation>,
    pub interaction: Option<Interaction>,
    pub uid: u64,
    pub title: String,
    /// Fonts the file's text uses that aren't installed, found when it was opened (empty for new
    /// documents); the app mentions them once.
    pub missing_fonts: Vec<String>,
    /// An input method's composition in progress ([`Composition`]).
    pub composition: Option<Composition>,
}

static NEXT_UID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl DocState {
    pub fn new(doc: Presentation, path: Option<String>, title: String) -> Self {
        let doc = Arc::new(doc);
        DocState {
            saved_doc: doc.clone(),
            doc,
            selection: Selection::default(),
            history: History { limit: 500, ..Default::default() },
            path,
            revision: 1,
            interaction: None,
            uid: NEXT_UID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            title,
            missing_fonts: Vec::new(),
            composition: None,
        }
    }
    pub fn is_dirty(&self) -> bool {
        !Arc::ptr_eq(&self.doc, &self.saved_doc)
    }
    pub fn title(&self) -> String {
        self.path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_stem())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.title.clone())
    }
    pub fn current_slide(&self) -> Option<&Slide> {
        self.doc.slides.get(self.selection.slide).map(|s| s.as_ref())
    }
    /// Shapes of the edited tree (current slide, or the master/layout in Slide Master view).
    pub fn shapes(&self) -> &[Shape] {
        match self.selection.target {
            Target::Slides => self.current_slide().map(|s| s.shapes.as_slice()).unwrap_or(&[]),
            Target::Master { master } => self.doc.masters.get(master).map(|m| m.shapes.as_slice()).unwrap_or(&[]),
            Target::Layout { master, layout } => {
                self.doc.masters.get(master).and_then(|m| m.layouts.get(layout)).map(|l| l.shapes.as_slice()).unwrap_or(&[])
            }
        }
    }
    pub fn shape(&self, id: ShapeId) -> Option<&Shape> {
        deckcraft_model::find_shape(self.shapes(), id)
    }
    pub fn selected_shapes(&self) -> Vec<&Shape> {
        self.selection.shapes.iter().filter_map(|id| self.shape(*id)).collect()
    }
}

/// Mutable access to the edited shape tree of a presentation.
pub fn shapes_mut<'a>(doc: &'a mut Presentation, sel: &Selection) -> Option<&'a mut Vec<Shape>> {
    match sel.target {
        Target::Slides => doc.slides.get_mut(sel.slide).map(|s| &mut Arc::make_mut(s).shapes),
        Target::Master { master } => doc.masters.get_mut(master).map(|m| &mut Arc::make_mut(m).shapes),
        Target::Layout { master, layout } => {
            doc.masters.get_mut(master).and_then(|m| Arc::make_mut(m).layouts.get_mut(layout)).map(|l| &mut l.shapes)
        }
    }
}

/// Engine preferences (the UI keeps its own).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    pub snap_to_grid: bool,
    pub grid_spacing: f64,
    pub smart_guides: bool,
    /// Ctrl+D / Paste offset.
    pub duplicate_offset: f64,
    pub autocorrect: bool,
    pub smart_quotes: bool,
    pub author: String,
    pub undo_limit: usize,
    /// Minutes between AutoRecover saves of unsaved presentations.
    pub recovery_minutes: f64,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            snap_to_grid: true,
            grid_spacing: 6.0,
            smart_guides: true,
            duplicate_offset: 12.0,
            autocorrect: true,
            smart_quotes: true,
            author: "Presenter".into(),
            undo_limit: 500,
            recovery_minutes: 1.0,
        }
    }
}

/// Things a command or tool asks the UI to do (open a dialog, show a pane, start the show…).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum UiRequest {
    Dialog {
        id: String,
        params: Value,
    },
    Pane {
        id: String,
    },
    StartShow {
        from: usize,
    },
    Message {
        text: String,
    },
    PickFile {
        purpose: String,
    },
    /// Media playback for the UI host (`media.play` / `pause` / `stop` / `seek`): `action` is
    /// `play`, `pause`, `toggle`, `stop` or `seek`; `ms` the media time to start from / seek to.
    Media {
        action: String,
        shape: ShapeId,
        ms: Option<u64>,
    },
}

/// Playback state of a media shape as the UI host reports it (see [`Session::media_status`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaStatus {
    /// `loading`, `playing`, `paused` or `ended`.
    pub state: String,
    pub position_ms: u64,
    pub duration_ms: u64,
    /// Why the media is silent / blank, when it can't be decoded.
    pub error: Option<String>,
}

/// Clipboard contents copied within the app.
#[derive(Clone, Debug, Default)]
pub struct Clipboard {
    pub shapes: Vec<Shape>,
    pub slides: Vec<Slide>,
    /// Media the copied shapes/slides reference.
    pub media: Vec<deckcraft_model::MediaItem>,
    pub text: Option<deckcraft_model::TextBody>,
    pub plain: String,
    /// How many times the shapes were pasted (each paste offsets further).
    pub pastes: u32,
}

pub struct Session {
    docs: Vec<DocState>,
    active: Option<usize>,
    pub prefs: Prefs,
    pub journal: Vec<(String, Value)>,
    pub clipboard: Clipboard,
    pub ui_requests: Vec<UiRequest>,
    pub tool: tools::ToolState,
    pub(crate) untitled: u32,
    /// Format Painter: copied shape/text formatting and whether it stays on (double-click).
    pub painter: Option<(cmd::format::Painted, bool)>,
    /// Animation Painter: copied shape animations and whether it stays on (double-click).
    pub anim_painter: Option<(Vec<deckcraft_model::anim::Animation>, bool)>,
    /// Set as Default Shape: the look new shapes get.
    pub default_look: Option<cmd::format::Painted>,
    /// Nesting of commands run by commands: only the outermost one is an undo step and a journal
    /// entry.
    depth: u32,
    /// Crash-recovery folder (the app sets it; saving or closing a presentation removes its entry).
    pub recovery_dir: Option<std::path::PathBuf>,
    /// Media playback state, written by the UI host every frame (empty without a host).
    pub media_status: std::collections::HashMap<ShapeId, MediaStatus>,
    /// Animated GIF pictures paused with `media.gifPlay` (the others play).
    pub gif_paused: std::collections::HashSet<ShapeId>,
    /// Text keys follow the Mac (Option+arrows move by word, Cmd+arrows to the line or text
    /// ends) instead of Windows and Linux (Ctrl+arrows by word or paragraph). Defaults to the
    /// platform DeckCraft was built for.
    pub mac_keys: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        guard::install_panic_hook();
        Session {
            docs: vec![],
            active: None,
            prefs: Prefs::default(),
            journal: vec![],
            clipboard: Clipboard::default(),
            ui_requests: vec![],
            tool: tools::ToolState::default(),
            untitled: 0,
            default_look: None,
            painter: None,
            anim_painter: None,
            depth: 0,
            recovery_dir: None,
            media_status: Default::default(),
            gif_paused: Default::default(),
            mac_keys: cfg!(target_os = "macos"),
        }
    }
    /// A session with one new presentation open.
    pub fn with_new() -> Self {
        let mut s = Self::new();
        let _ = s.execute("file.new", &Value::Null);
        s
    }
    pub fn documents(&self) -> &[DocState] {
        &self.docs
    }
    pub fn active_index(&self) -> Option<usize> {
        self.active
    }
    pub fn active(&self) -> Option<&DocState> {
        self.active.and_then(|i| self.docs.get(i))
    }
    pub fn active_mut(&mut self) -> Option<&mut DocState> {
        self.active.and_then(|i| self.docs.get_mut(i))
    }
    pub fn doc(&self) -> Result<&DocState> {
        self.active().ok_or(EngineError::NoDocument)
    }
    pub fn doc_mut(&mut self) -> Result<&mut DocState> {
        self.active_mut().ok_or(EngineError::NoDocument)
    }
    pub fn set_active(&mut self, i: usize) {
        if i < self.docs.len() {
            self.active = Some(i);
        }
    }
    pub fn add_document(&mut self, d: DocState) -> usize {
        self.docs.push(d);
        self.active = Some(self.docs.len() - 1);
        self.docs.len() - 1
    }
    pub fn close_document(&mut self, i: usize) {
        if i < self.docs.len() {
            let d = self.docs.remove(i);
            if let Some(dir) = &self.recovery_dir {
                recovery::discard(dir, d.uid);
            }
            self.active = if self.docs.is_empty() { None } else { Some(i.min(self.docs.len() - 1)) };
        }
    }
    pub fn next_untitled(&mut self) -> String {
        self.untitled += 1;
        format!("Presentation{}", self.untitled)
    }

    /// Run command `id`. A panic inside it becomes [`EngineError::Internal`] and leaves the
    /// presentation as it was.
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value> {
        self.guarded(id, |s| s.execute_unguarded(id, params))
    }

    fn execute_unguarded(&mut self, id: &str, params: &Value) -> Result<Value> {
        let spec = find_command(id).ok_or_else(|| EngineError::UnknownCommand(id.into()))?;
        // Any other edit (or a click, a key, undo) settles an input method's composition first.
        if self.depth == 0 && (spec.journal || spec.undoable) && !matches!(id, "text.compose" | "text.commit") {
            self.end_composition();
        }
        if let Err(e) = (spec.enabled)(self) {
            let named = (e == cmd::NOTHING_SELECTED || e == cmd::SELECT_TWO) && (params.get("ids").is_some() || params.get("id").is_some());
            if !named {
                return Err(EngineError::Disabled(id.into(), e));
            }
        }
        let before = self.active().map(|d| (d.uid, d.doc.clone(), d.selection.clone()));
        let outer = self.depth == 0;
        self.depth += 1;
        let r = (spec.run)(self, params);
        self.depth -= 1;
        let r = r?;
        if let Some(st) = self.active_mut() {
            clamp_selection(st);
        }
        if let (Some((uid, old, sel)), Some(st)) = (before, self.active_mut())
            && st.uid == uid
            && !Arc::ptr_eq(&old, &st.doc)
            && st.interaction.is_none()
            && spec.undoable
            && outer
        {
            push_undo(st, HistoryEntry { label: spec.label.to_string(), doc: old, selection: sel });
        }
        if spec.journal && outer {
            self.journal.push((id.to_string(), params.clone()));
            if self.journal.len() > 10_000 {
                self.journal.drain(..1000);
            }
        }
        Ok(r)
    }

    pub fn commands(&self) -> Vec<CommandInfo> {
        command_specs().iter().map(|c| c.info(self)).collect()
    }

    /// Mutate the active presentation (copy-on-write) and bump the revision.
    pub fn edit<T>(&mut self, f: impl FnOnce(&mut Presentation, &mut Selection) -> Result<T>) -> Result<T> {
        let st = self.doc_mut()?;
        let mut doc = (*st.doc).clone();
        let mut sel = st.selection.clone();
        let r = f(&mut doc, &mut sel)?;
        connect::reroute(&mut doc, &sel);
        st.doc = Arc::new(doc);
        st.selection = sel;
        st.revision += 1;
        Ok(r)
    }

    /// Keep an input method's composition as it stands (#101): its text stays and the whole
    /// composition becomes one undo step. Nothing happens without one.
    pub fn end_composition(&mut self) {
        let Some(st) = self.active_mut() else { return };
        let Some(c) = st.composition.take() else { return };
        if !Arc::ptr_eq(&c.before, &st.doc) {
            push_undo(st, HistoryEntry { label: "Type Text".into(), doc: c.before, selection: c.before_selection });
        }
    }

    /// Change only the selection (no undo step).
    pub fn select(&mut self, f: impl FnOnce(&Presentation, &mut Selection)) -> Result<()> {
        let st = self.doc_mut()?;
        let doc = st.doc.clone();
        // A caret placed anywhere starts downstream; only End puts it back at a line's end.
        if let Some(t) = st.selection.text.as_mut() {
            t.line_end = None;
        }
        f(&doc, &mut st.selection);
        clamp_selection(st);
        st.revision += 1;
        Ok(())
    }
}

/// Keep the selection pointing at things that exist.
fn clamp_selection(st: &mut DocState) {
    let n = st.doc.slides.len();
    if n == 0 {
        st.selection.slide = 0;
    } else if st.selection.slide >= n {
        st.selection.slide = n - 1;
    }
    let ids: Vec<ShapeId> = st.selection.shapes.iter().copied().filter(|id| st.shape(*id).is_some()).collect();
    st.selection.shapes = ids;
    if let Some(t) = &st.selection.text
        && !t.notes
        && st.shape(t.shape).is_none()
    {
        st.selection.text = None;
    }
    let slides: Vec<SlideId> = st.selection.slides.iter().copied().filter(|id| st.doc.slide(*id).is_some()).collect();
    st.selection.slides = slides;
    match st.selection.target {
        Target::Master { master } if master >= st.doc.masters.len() => st.selection.target = Target::Slides,
        Target::Layout { master, layout } if st.doc.masters.get(master).is_none_or(|m| layout >= m.layouts.len()) => {
            st.selection.target = Target::Slides
        }
        _ => {}
    }
}

pub(crate) fn push_undo(st: &mut DocState, e: HistoryEntry) {
    st.history.undo.push(e);
    st.history.redo.clear();
    if st.history.undo.len() > st.history.limit.max(1) {
        st.history.undo.remove(0);
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_keys;
