//! In-place text editing: entering/leaving text, typing, deleting, caret movement and selection.

use std::sync::Arc;

use deckcraft_model::edit::{self as ed, Pos};
use deckcraft_model::resolve::{self, Ctx};
use deckcraft_model::text::{AutoFit, Hyperlink, Run, RunKind, RunProps, TextBody};
use deckcraft_model::{Presentation, ShapeId, ShapeKind};
use serde_json::{Value, json};

use super::*;
use crate::{DocState, Result, Selection, Session, Target, TextSel};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(noundo "text.edit", "Edit Text", [], Some("Enter"), "{id?, at?: [paragraph, char], end?: bool, cell?: [row, col], notes?: bool}", has_slide, edit),
        cmd!(noundo "text.exit", "Stop Editing Text", [], None, "{}", has_doc, exit),
        cmd!("text.insert", "Type Text", [], None, "{text} (\\n = new paragraph, \\u000b = line break)", has_text, insert),
        cmd!("text.delete", "Delete Text", [], Some("Backspace"), "{dir?: backward|forward|wordBackward|wordForward}", has_text, delete),
        // The input method's composition: shown in place, an undo step only once committed.
        CommandSpec {
            id: "text.compose",
            label: "Compose Text",
            menu: &[],
            shortcut: None,
            params: "{text, active?: [start, end]} — an input method's composition (preedit) at the insertion point; empty text cancels it",
            enabled: has_text,
            run: compose,
            journal: false,
            undoable: false,
        },
        cmd!(
            noundo "text.commit",
            "Commit Composition",
            [],
            None,
            "{text} — the input method's result replaces its composition, as one undo step",
            has_text,
            commit
        ),
        cmd!(noundo "text.move", "Move Insertion Point", [], None, "{to: left|right|up|down|wordLeft|wordRight|lineStart|lineEnd|paraStart|paraEnd|start|end, extend?: bool}", has_text, move_caret),
        cmd!(
            "text.delete",
            "Delete Text",
            [],
            Some("Backspace"),
            "{dir?: backward|forward|wordBackward|wordForward} — the word directions are Ctrl+Backspace/Delete: they work from the start of a selection without deleting it",
            has_text,
            delete
        ),
        cmd!(noundo "text.move", "Move Insertion Point", [], None, "{to: left|right|up|down|wordLeft|wordRight|lineStart|lineEnd|paraStart|paraEnd|paraUp|paraDown|start|end, extend?: bool} — left/right are on screen: in a right-to-left paragraph left goes forward", has_text, move_caret),
        cmd!(noundo "text.select", "Select Text", [], None, "{anchor: [p, c], caret: [p, c]}", has_text, select),
        cmd!(noundo "text.selectAll", "Select All Text", [], None, "{}", has_text, select_all),
        cmd!(noundo "text.selectWord", "Select Word", [], None, "{at?: [p, c]}", has_text, select_word),
        cmd!(noundo "text.selectParagraph", "Select Paragraph", [], None, "{at?: [p, c]}", has_text, select_para),
        cmd!(
            "text.set",
            "Set Text",
            [],
            None,
            "{id?, text, cell?: [r, c]} — replaces the whole text, keeping the first run's formatting",
            has_slide,
            set
        ),
        cmd!(query "text.get", "Get Text", [], None, "{id?} → {text, paragraphs}", has_slide, get),
    ]
}

/// The text body a text selection edits.
pub fn body_of<'a>(st: &'a DocState, t: &TextSel) -> Option<&'a TextBody> {
    if t.notes {
        return st.current_slide().map(|s| &s.notes);
    }
    let sh = st.shape(t.shape)?;
    if let (Some((r, c)), ShapeKind::Table(tb)) = (t.cell, &sh.kind) {
        return tb.cell(r, c).map(|x| &x.text);
    }
    sh.text.as_ref()
}

fn body_mut<'a>(doc: &'a mut Presentation, sel: &Selection, t: &TextSel) -> Option<&'a mut TextBody> {
    if t.notes {
        return doc.slides.get_mut(sel.slide).map(|s| &mut Arc::make_mut(s).notes);
    }
    let shapes = crate::shapes_mut(doc, sel)?;
    let sh = deckcraft_model::find_shape_mut(shapes, t.shape)?;
    if let Some((r, c)) = t.cell {
        if let ShapeKind::Table(tb) = &mut sh.kind {
            return tb.cell_mut(r, c).map(|x| &mut x.text);
        }
        return None;
    }
    Some(sh.text.get_or_insert_with(TextBody::default))
}

/// Edit the active text body and selection; refits auto-sizing text boxes afterwards.
pub(crate) fn edit_text<T>(s: &mut Session, f: impl FnOnce(&mut TextBody, &mut TextSel) -> T) -> Result<T> {
    let t0 = s.doc()?.selection.text.clone().ok_or_else(|| bad("text", "no text insertion point"))?;
    // Materialise an inherited placeholder box before editing (so autofit can change it).
    let r = s.edit(|doc, sel| {
        let mut t = t0.clone();
        let body = body_mut(doc, sel, &t).ok_or_else(|| bad("text", "the text being edited is gone"))?;
        let r = f(body, &mut t);
        sel.text = Some(t);
        Ok(r)
    })?;
    if !t0.notes && t0.cell.is_none() {
        fit_text_box(s, t0.shape)?;
    }
    Ok(r)
}

/// Grow/shrink an auto-sizing text box (Resize shape to fit text) to its text.
pub fn fit_text_box(s: &mut Session, id: ShapeId) -> Result<()> {
    let st = s.doc()?;
    let Some(sh) = st.shape(id).cloned() else { return Ok(()) };
    let Some(body) = sh.text.clone() else { return Ok(()) };
    let doc = st.doc.clone();
    let sel = st.selection.clone();
    let Some(ctx) = ctx_for(&doc, &sel) else { return Ok(()) };
    let bp = resolve::body(&ctx, &sh);
    if bp.autofit != Some(AutoFit::Shape) {
        return Ok(());
    }
    let x = xfrm_of(&doc, &sel, &sh);
    let geo = deckcraft_render::shape_geometry(&sh, x.w, x.h);
    let fields = deckcraft_text::NoFields;
    let mut nx = x;
    if bp.wrap == Some(false) {
        nx.w = deckcraft_text::fit_width(&ctx, &sh, &body, geo.text_rect, &fields).max(20.0);
    }
    let geo = deckcraft_render::shape_geometry(&sh, nx.w, nx.h);
    let need = deckcraft_text::fit_height(&ctx, &sh, &body, geo.text_rect, &fields);
    let extra = (nx.h - geo.text_rect.height()).max(0.0);
    nx.h = (need + extra).max(8.0);
    if (nx.h - x.h).abs() < 0.01 && (nx.w - x.w).abs() < 0.01 && sh.xfrm.is_some() {
        return Ok(());
    }
    // Change the box in place without an extra undo step (it is part of the edit).
    let st = s.doc_mut()?;
    let mut d = (*st.doc).clone();
    if let Some(list) = crate::shapes_mut(&mut d, &st.selection)
        && let Some(target) = deckcraft_model::find_shape_mut(list, id)
    {
        target.xfrm = Some(nx);
    }
    st.doc = Arc::new(d);
    st.revision += 1;
    Ok(())
}

pub(crate) fn ctx_for<'a>(doc: &'a Presentation, sel: &Selection) -> Option<Ctx<'a>> {
    match sel.target {
        Target::Slides => doc.slides.get(sel.slide).and_then(|s| Ctx::for_slide(doc, s)),
        Target::Master { master } => doc.masters.get(master).map(|m| Ctx::for_master(doc, m)),
        Target::Layout { master, layout } => doc.masters.get(master).and_then(|m| m.layouts.get(layout).map(|l| Ctx::for_layout(doc, m, l))),
    }
}

/// Lay out the text being edited (for vertical caret movement and hit testing).
pub fn layout_for(st: &DocState, t: &TextSel) -> Option<deckcraft_text::TextLayout> {
    if t.notes || t.cell.is_some() {
        return None;
    }
    let sh = st.shape(t.shape)?;
    let body = sh.text.as_ref()?;
    let ctx = ctx_for(&st.doc, &st.selection)?;
    let x = xfrm_of(&st.doc, &st.selection, sh);
    let geo = deckcraft_render::shape_geometry(sh, x.w, x.h);
    Some(deckcraft_text::layout(
        &ctx,
        sh,
        body,
        &deckcraft_text::Opts { rect: geo.text_rect, fields: &deckcraft_text::NoFields, prompt_color: None, no_shrink: false },
    ))
}

/// The insertion point on the slide: its line from top to bottom, in slide coordinates (flips and
/// vertical text as drawn). Text with no paragraph yet (a placeholder about to be typed in) is
/// measured as one empty paragraph, so the caret shows where typing begins. `None` for notes and
/// table cells, which aren't laid out on the slide here.
pub fn caret_line(st: &DocState, t: &TextSel) -> Option<(deckcraft_geom::Point, deckcraft_geom::Point)> {
    if t.notes || t.cell.is_some() {
        return None;
    }
    let sh = st.shape(t.shape)?;
    let ctx = ctx_for(&st.doc, &st.selection)?;
    let x = xfrm_of(&st.doc, &st.selection, sh);
    let geo = deckcraft_render::shape_geometry(sh, x.w, x.h);
    let mut body = sh.text.clone().unwrap_or_default();
    if body.paragraphs.is_empty() {
        body.paragraphs.push(deckcraft_model::text::Paragraph::default());
    }
    let l = deckcraft_text::layout(
        &ctx,
        sh,
        &body,
        &deckcraft_text::Opts { rect: geo.text_rect, fields: &deckcraft_text::NoFields, prompt_color: None, no_shrink: false },
    );
    // After End on a wrapped line the caret is drawn at that line's end.
    let aff = if t.at_line_end() { deckcraft_text::Affinity::Upstream } else { deckcraft_text::Affinity::Downstream };
    let (cx, top, bot) = l.caret_drawn(deckcraft_text::Pos { para: t.caret.0, ch: t.caret.1 }, aff)?;
    let m = deckcraft_render::text_transform(x.affine(), geo.text_rect, l.rotation);
    Some((m * deckcraft_geom::Point::new(cx, top), m * deckcraft_geom::Point::new(cx, bot)))
}

fn pos_param(p: &Value, key: &str) -> Option<Pos> {
    let a = p.get(key)?.as_array()?;
    Some((a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize))
}

fn edit(s: &mut Session, p: &Value) -> Result<Value> {
    if bool_or(p, "notes", false) {
        let end = s.doc()?.current_slide().map(|x| ed::end(&x.notes)).unwrap_or((0, 0));
        s.select(|_, sel| {
            sel.text = Some(TextSel { notes: true, anchor: end, caret: end, ..Default::default() });
            sel.shapes.clear();
        })?;
        return ok();
    }
    let id = match id_param(p, "id") {
        Some(i) => i,
        None => s.doc()?.selection.shapes.first().copied().ok_or_else(|| bad("text.edit", "select a shape with text"))?,
    };
    let sh = shape_of(s, id)?;
    let cell = p.get("cell").and_then(Value::as_array).and_then(|a| Some((a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize)));
    let body = match (&sh.kind, cell) {
        (ShapeKind::Table(t), Some((r, c))) => t.cell(r, c).map(|x| x.text.clone()).ok_or_else(|| bad("text.edit", "no such cell"))?,
        (ShapeKind::Table(_), None) => return Err(bad("text.edit", "say which `cell` of the table")),
        (ShapeKind::Shape | ShapeKind::Connector { .. }, _) => sh.text.clone().unwrap_or_default(),
        _ => return Err(bad("text.edit", "this object doesn't hold text")),
    };
    let at = pos_param(p, "at").unwrap_or_else(|| ed::end(&body));
    // A shape without a text body gets one on first edit.
    if sh.text.is_none() && cell.is_none() {
        s.edit(|doc, sel| {
            if let Some(list) = crate::shapes_mut(doc, sel)
                && let Some(x) = deckcraft_model::find_shape_mut(list, id)
            {
                x.text = Some(TextBody::default());
            }
            Ok(())
        })?;
    }
    s.select(|_, sel| {
        sel.shapes = vec![id];
        sel.text = Some(TextSel { shape: id, anchor: at, caret: at, cell, notes: false, line_end: None });
    })?;
    Ok(json!({"id": id, "at": [at.0, at.1]}))
}

fn exit(s: &mut Session, _p: &Value) -> Result<Value> {
    s.select(|_, sel| {
        if let Some(t) = sel.text.take()
            && !t.notes
        {
            sel.shapes = vec![t.shape];
        }
    })?;
    ok()
}

/// Smart quotes and simple autocorrect applied to typed text.
fn typed(s: &Session, body: &TextBody, at: Pos, text: &str) -> String {
    if !s.prefs.smart_quotes || text.chars().count() != 1 {
        return text.to_string();
    }
    let prev = body.paragraphs.get(at.0).and_then(|p| p.text().chars().nth(at.1.wrapping_sub(1)));
    let opening = prev.is_none_or(|c| c.is_whitespace() || "([{".contains(c));
    match text {
        "\"" => (if opening { "“" } else { "”" }).to_string(),
        "'" => (if opening { "‘" } else { "’" }).to_string(),
        _ => text.to_string(),
    }
}

pub(crate) fn insert_text(s: &mut Session, text: &str, props: Option<RunProps>) -> Result<Value> {
    let text = {
        let st = s.doc()?;
        match &st.selection.text {
            Some(t) => {
                let body = body_of(st, t).cloned().unwrap_or_default();
                typed(s, &body, t.ordered().0, text)
            }
            None => text.to_string(),
        }
    };
    let pos = edit_text(s, |body, t| {
        let (a, b) = t.ordered();
        let at = ed::delete(body, a, b);
        let props = props.clone().or_else(|| if a != b { Some(ed::props_at(body, at)) } else { unlinked_at_link_end(body, at) });
        let c = ed::insert(body, at, &text, props);
        t.anchor = c;
        t.caret = c;
        c
    })?;
    autocorrect(s)?;
    Ok(json!({"caret": [pos.0, pos.1]}))
}

/// Typing right after a hyperlink doesn't extend it (as in PowerPoint): at the end of a linked
/// run, the props to type with are the run's without the link. `None` elsewhere.
fn unlinked_at_link_end(body: &TextBody, at: Pos) -> Option<RunProps> {
    let mut props = ed::props_at(body, at);
    props.link.as_ref()?;
    let len = body.paragraphs.get(at.0).map_or(0, |p| p.text().chars().count());
    // props_at(c + 1) is the props of the character at c.
    if at.1 < len && ed::props_at(body, (at.0, at.1 + 1)).link == props.link {
        return None;
    }
    props.link = None;
    Some(props)
}

const AUTOCORRECT: &[(&str, &str)] = &[
    ("teh", "the"),
    ("adn", "and"),
    ("recieve", "receive"),
    ("seperate", "separate"),
    ("occured", "occurred"),
    ("thier", "their"),
    ("wich", "which"),
    ("(c)", "©"),
    ("(r)", "®"),
    ("(tm)", "™"),
    ("->", "→"),
    ("<-", "←"),
    ("--", "–"),
];

/// After a space or punctuation, replace a just-typed autocorrect entry.
fn autocorrect(s: &mut Session) -> Result<()> {
    if !s.prefs.autocorrect {
        return Ok(());
    }
    let st = s.doc()?;
    let Some(t) = st.selection.text.clone() else { return Ok(()) };
    let Some(body) = body_of(st, &t) else { return Ok(()) };
    let line: Vec<char> = body.paragraphs.get(t.caret.0).map(|p| p.text().chars().collect()).unwrap_or_default();
    let end = t.caret.1;
    if end == 0 || !line.get(end - 1).is_some_and(|c| c.is_whitespace() || ".,;:!?".contains(*c)) {
        return Ok(());
    }
    let word_end = end - 1;
    let mut start = word_end;
    while start > 0 && line.get(start - 1).is_some_and(|c| !c.is_whitespace()) {
        start -= 1;
    }
    let word: String = line.get(start..word_end).map(|w| w.iter().collect()).unwrap_or_default();
    let Some((_, rep)) = AUTOCORRECT.iter().find(|(k, _)| k.eq_ignore_ascii_case(&word)) else { return Ok(()) };
    let rep = if word.chars().next().is_some_and(char::is_uppercase) { ed::change_case(rep, "sentence") } else { rep.to_string() };
    let delta = rep.chars().count() as isize - word.chars().count() as isize;
    let st = s.doc_mut()?;
    let mut d = (*st.doc).clone();
    let sel = st.selection.clone();
    if let Some(b) = body_mut(&mut d, &sel, &t) {
        ed::delete(b, (t.caret.0, start), (t.caret.0, word_end));
        ed::insert(b, (t.caret.0, start), &rep, None);
    }
    st.doc = Arc::new(d);
    let c = (t.caret.0, (end as isize + delta).max(0) as usize);
    if let Some(tt) = st.selection.text.as_mut() {
        tt.caret = c;
        tt.anchor = c;
    }
    st.revision += 1;
    Ok(())
}

pub(crate) fn paste_body(s: &mut Session, src: &TextBody) -> Result<Value> {
    let pos = edit_text(s, |body, t| {
        let (a, b) = t.ordered();
        let at = ed::delete(body, a, b);
        let c = ed::insert_body(body, at, src);
        t.anchor = c;
        t.caret = c;
        c
    })?;
    Ok(json!({"caret": [pos.0, pos.1]}))
}

pub(crate) fn insert_field(s: &mut Session, field: &str, text: &str) -> Result<Value> {
    let field = field.to_string();
    let text = if text.is_empty() { "#".to_string() } else { text.to_string() };
    edit_text(s, move |body, t| {
        let (a, b) = t.ordered();
        let at = ed::delete(body, a, b);
        let props = ed::props_at(body, at);
        let src = TextBody {
            paragraphs: vec![deckcraft_model::text::Paragraph {
                runs: vec![Run { text: text.clone(), props, kind: RunKind::Field { field: field.clone() } }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let c = ed::insert_body(body, at, &src);
        t.anchor = c;
        t.caret = c;
    })?;
    ok()
}

/// Text an input method sends: no paragraph or line breaks (Enter is a key of its own).
fn ime_text(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

/// Is the composition still where it was put (the paragraph holds all of it)?
fn composition_fits(st: &DocState, c: &crate::Composition) -> bool {
    let Some(t) = &st.selection.text else { return false };
    let Some(body) = body_of(st, t) else { return false };
    body.paragraphs.get(c.start.0).is_some_and(|p| c.start.1.saturating_add(c.len) <= p.char_len())
}

/// `text.compose`: put an input method's preedit at the insertion point (replacing the selection)
/// or replace the composition with the new preedit. An empty preedit cancels: the presentation
/// goes back to how it was before the composition began.
fn compose(s: &mut Session, p: &Value) -> Result<Value> {
    let text = ime_text(str_param(p, "text").ok_or_else(|| bad("text.compose", "missing `text`"))?);
    let n = text.chars().count();
    let active = p.get("active").and_then(Value::as_array).and_then(|a| {
        let (x, y) = (a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize);
        (x < y).then_some((x.min(n), y.min(n)))
    });
    let current = s.doc()?.composition.clone();
    // A composition whose text was changed under it is kept as it is; a new one starts.
    let current = match current {
        Some(c) if !composition_fits(s.doc()?, &c) => {
            s.end_composition();
            None
        }
        c => c,
    };
    match current {
        None if text.is_empty() => Ok(json!({"composing": false})),
        None => {
            let st = s.doc()?;
            let (before, before_selection) = (st.doc.clone(), st.selection.clone());
            let (start, props) = edit_text(s, |body, t| {
                let (a, b) = t.ordered();
                let at = ed::delete(body, a, b);
                let props = if a != b { Some(ed::props_at(body, at)) } else { unlinked_at_link_end(body, at) };
                let c = ed::insert(body, at, &text, props.clone());
                t.anchor = c;
                t.caret = c;
                (at, props)
            })?;
            s.doc_mut()?.composition = Some(crate::Composition { start, len: n, active, props, before, before_selection });
            Ok(json!({"composing": true, "start": [start.0, start.1], "len": n}))
        }
        Some(c) if text.is_empty() => {
            let st = s.doc_mut()?;
            st.doc = c.before;
            st.selection = c.before_selection;
            st.composition = None;
            st.revision += 1;
            Ok(json!({"composing": false}))
        }
        Some(c) => {
            let (start, len, props) = (c.start, c.len, c.props.clone());
            edit_text(s, |body, t| {
                ed::delete(body, start, (start.0, start.1 + len));
                let e = ed::insert(body, start, &text, props);
                t.anchor = e;
                t.caret = e;
            })?;
            s.doc_mut()?.composition = Some(crate::Composition { len: n, active, ..c });
            Ok(json!({"composing": true, "start": [start.0, start.1], "len": n}))
        }
    }
}

/// `text.commit`: the input method's result replaces the composition (or is typed at the
/// insertion point when there is none) and the whole composition becomes one undo step.
fn commit(s: &mut Session, p: &Value) -> Result<Value> {
    let text = ime_text(str_param(p, "text").ok_or_else(|| bad("text.commit", "missing `text`"))?);
    if s.doc()?.composition.is_some() || !text.is_empty() {
        compose(s, &json!({"text": text}))?;
    }
    s.end_composition();
    let caret = s.doc()?.selection.text.as_ref().map(|t| t.caret).unwrap_or_default();
    Ok(json!({"caret": [caret.0, caret.1]}))
}

/// Underlines under an input method's composition (#101), in slide coordinates: `(from, to,
/// thick)` per line, thick under the part being converted. Empty without a composition or when
/// the edited text isn't laid out on the slide (notes, table cells).
pub fn composition_underlines(st: &DocState) -> Vec<(deckcraft_geom::Point, deckcraft_geom::Point, bool)> {
    let (Some(c), Some(t)) = (&st.composition, &st.selection.text) else { return vec![] };
    let Some(sh) = st.shape(t.shape) else { return vec![] };
    let Some(l) = layout_for(st, t) else { return vec![] };
    let x = xfrm_of(&st.doc, &st.selection, sh);
    let tr = deckcraft_render::shape_geometry(sh, x.w, x.h).text_rect;
    let m = deckcraft_render::text_transform(x.affine(), tr, l.rotation);
    let pos = |ch: usize| deckcraft_text::Pos { para: c.start.0, ch: c.start.1 + ch };
    let mut out = vec![];
    let mut add = |a: usize, b: usize, thick: bool| {
        for (x0, x1, y) in l.composition_underlines(pos(a), pos(b)) {
            out.push((m * deckcraft_geom::Point::new(x0, y), m * deckcraft_geom::Point::new(x1, y), thick));
        }
    };
    add(0, c.len, false);
    if let Some((a, b)) = c.active {
        add(a, b.min(c.len), true);
    }
    out
}

fn insert(s: &mut Session, p: &Value) -> Result<Value> {
    let text = str_param(p, "text").ok_or_else(|| bad("text.insert", "missing `text`"))?.to_string();
    insert_text(s, &text, None)
}

pub(crate) fn delete_dir(s: &mut Session, dir: &str) -> Result<Value> {
    let dir = dir.to_string();
    edit_text(s, move |body, t| {
        let (a, b) = t.ordered();
        let at = if dir == "wordBackward" || dir == "wordForward" {
            // Ctrl+Backspace/Delete work from the start of a selection and leave the rest of it
            // (PowerPoint for Windows).
            ed::delete_word(body, a, dir == "wordForward")
        } else if a != b {
            ed::delete(body, a, b)
        } else {
            let other = match dir.as_str() {
                "forward" => ed::char_move(body, a, true),
                _ => {
                    // Backspace at the start of an indented/bulleted paragraph outdents first.
                    if a.1 == 0
                        && let Some(para) = body.paragraphs.get_mut(a.0)
                        && para.level > 0
                    {
                        para.level -= 1;
                        t.anchor = a;
                        t.caret = a;
                        return;
                    }
                    ed::char_move(body, a, false)
                }
            };
            ed::delete(body, a, other)
        };
        t.anchor = at;
        t.caret = at;
    })?;
    ok()
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    delete_dir(s, str_param(p, "dir").unwrap_or("backward"))
}

/// Is paragraph `para` right-to-left (its lines run from the right)?
fn para_rtl(layout: Option<&deckcraft_text::TextLayout>, body: &TextBody, para: usize) -> bool {
    match layout.and_then(|l| l.lines.iter().find(|x| x.para == para)) {
        Some(line) => line.rtl,
        None => body.paragraphs.get(para).and_then(|x| x.props.rtl).unwrap_or(false),
    }
}

fn move_caret(s: &mut Session, p: &Value) -> Result<Value> {
    use deckcraft_text::Affinity;
    let to = str_param(p, "to").unwrap_or("right").to_string();
    let extend = bool_or(p, "extend", false);
    let st = s.doc()?;
    let t = st.selection.text.clone().ok_or_else(|| bad("text.move", "not editing text"))?;
    let body = body_of(st, &t).cloned().unwrap_or_default();
    let layout = layout_for(st, &t);
    let lp = |c: Pos| deckcraft_text::Pos { para: c.0, ch: c.1 };
    let caret = t.caret;
    let aff = if t.at_line_end() { Affinity::Upstream } else { Affinity::Downstream };
    // Left and right are on screen: in a right-to-left paragraph left goes forward in the text.
    let rtl = para_rtl(layout.as_ref(), &body, caret.0);
    let to = match (to.as_str(), rtl) {
        ("left", true) => "right",
        ("right", true) => "left",
        ("wordLeft", true) => "wordRight",
        ("wordRight", true) => "wordLeft",
        (x, _) => x,
    }
    .to_string();
    let collapse_to = |left: bool| if left { t.ordered().0 } else { t.ordered().1 };
    let mut upstream = false;
    let new = match to.as_str() {
        "left" if t.is_range() && !extend => collapse_to(true),
        "right" if t.is_range() && !extend => collapse_to(false),
        "left" => ed::char_move(&body, caret, false),
        "right" => ed::char_move(&body, caret, true),
        // Ctrl+arrows with a selection go on from its start (left) or end (right).
        "wordLeft" => ed::word_move(&body, if extend { caret } else { collapse_to(true) }, false),
        "wordRight" => ed::word_move(&body, if extend { caret } else { collapse_to(false) }, true),
        "paraStart" => (caret.0, 0),
        "paraEnd" => (caret.0, body.paragraphs.get(caret.0).map(|x| x.char_len()).unwrap_or(0)),
        "paraUp" => ed::para_up(&body, caret),
        "paraDown" => ed::para_down(&body, caret),
        "start" => (0, 0),
        "end" => ed::end(&body),
        "up" | "down" | "lineStart" | "lineEnd" => match &layout {
            Some(l) => {
                let x = l.caret_drawn(lp(caret), aff).map(|c| c.0).unwrap_or(0.0);
                let (r, a) = match to.as_str() {
                    "up" => l.vertical_affine(lp(caret), aff, false, x),
                    "down" => l.vertical_affine(lp(caret), aff, true, x),
                    "lineStart" => (l.line_bounds_affine(lp(caret), aff).0, Affinity::Downstream),
                    // End stays on its line: after a soft wrap the caret sits at the line's end.
                    _ => l.line_end_caret(lp(caret), aff),
                };
                upstream = a == Affinity::Upstream;
                (r.para, r.ch)
            }
            None => match to.as_str() {
                "up" => (caret.0.saturating_sub(1), caret.1),
                "down" => ((caret.0 + 1).min(body.paragraphs.len().saturating_sub(1)), caret.1),
                "lineStart" => (caret.0, 0),
                _ => (caret.0, body.paragraphs.get(caret.0).map(|x| x.char_len()).unwrap_or(0)),
            },
        },
        other => return Err(bad("text.move", format!("unknown direction `{other}`"))),
    };
    let new = (new.0.min(body.paragraphs.len().saturating_sub(1)), new.1.min(body.paragraphs.get(new.0).map(|x| x.char_len()).unwrap_or(0)));
    s.select(|_, sel| {
        if let Some(tt) = sel.text.as_mut() {
            tt.caret = new;
            tt.line_end = if upstream { Some(new) } else { None };
            if !extend {
                tt.anchor = new;
            }
        }
    })?;
    Ok(json!({"caret": [new.0, new.1], "lineEnd": upstream}))
}

fn select(s: &mut Session, p: &Value) -> Result<Value> {
    let a = pos_param(p, "anchor").ok_or_else(|| bad("text.select", "missing `anchor`"))?;
    let c = pos_param(p, "caret").unwrap_or(a);
    s.select(|_, sel| {
        if let Some(t) = sel.text.as_mut() {
            t.anchor = a;
            t.caret = c;
        }
    })?;
    ok()
}

fn select_all(s: &mut Session, _p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let t = st.selection.text.clone().ok_or_else(|| bad("text.selectAll", "not editing text"))?;
    let end = body_of(st, &t).map(ed::end).unwrap_or((0, 0));
    s.select(|_, sel| {
        if let Some(t) = sel.text.as_mut() {
            t.anchor = (0, 0);
            t.caret = end;
        }
    })?;
    ok()
}

fn select_word(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let t = st.selection.text.clone().ok_or_else(|| bad("text.selectWord", "not editing text"))?;
    let at = pos_param(p, "at").unwrap_or(t.caret);
    let body = body_of(st, &t).cloned().unwrap_or_default();
    let (a, b) = ed::word_at(&body, at);
    s.select(|_, sel| {
        if let Some(t) = sel.text.as_mut() {
            t.anchor = a;
            t.caret = b;
        }
    })?;
    ok()
}

fn select_para(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let t = st.selection.text.clone().ok_or_else(|| bad("text.selectParagraph", "not editing text"))?;
    let at = pos_param(p, "at").unwrap_or(t.caret);
    let body = body_of(st, &t);
    let n = body.and_then(|b| b.paragraphs.get(at.0)).map(|x| x.char_len()).unwrap_or(0);
    // The paragraph mark is part of the paragraph (PowerPoint's triple-click selects it too).
    let end = if body.is_some_and(|b| at.0 + 1 < b.paragraphs.len()) { (at.0 + 1, 0) } else { (at.0, n) };
    s.select(|_, sel| {
        if let Some(t) = sel.text.as_mut() {
            t.anchor = (at.0, 0);
            t.caret = end;
        }
    })?;
    ok()
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let text = str_param(p, "text").ok_or_else(|| bad("text.set", "missing `text`"))?.to_string();
    let id = match id_param(p, "id") {
        Some(i) => i,
        None => s.doc()?.selection.shapes.first().copied().ok_or_else(|| bad("text.set", "missing `id` (or select a shape)"))?,
    };
    let cell = p.get("cell").and_then(Value::as_array).and_then(|a| Some((a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize)));
    edit_shapes(s, &json!({"id": id}), "text.set", |sh| {
        let target = match (&mut sh.kind, cell) {
            (ShapeKind::Table(t), Some((r, c))) => t.cell_mut(r, c).map(|x| &mut x.text),
            (ShapeKind::Table(_), None) => None,
            _ => Some(sh.text.get_or_insert_with(TextBody::default)),
        };
        let Some(body) = target else { return Err(bad("text.set", "no text here (tables need `cell`)")) };
        let keep = body.paragraphs.first().and_then(|p| p.runs.first()).map(|r| r.props.clone()).unwrap_or_default();
        let levels: Vec<(u8, deckcraft_model::text::ParaProps)> = body.paragraphs.iter().map(|p| (p.level, p.props.clone())).collect();
        let mut nb = TextBody::from_text(&text);
        for (i, para) in nb.paragraphs.iter_mut().enumerate() {
            // Leading tabs set the outline level.
            let tabs = para.runs.first().map(|r| r.text.chars().take_while(|c| *c == '\t').count()).unwrap_or(0);
            if tabs > 0
                && let Some(r) = para.runs.first_mut()
            {
                r.text = r.text.trim_start_matches('\t').to_string();
                para.level = tabs.min(8) as u8;
            } else if let Some((lv, pp)) = levels.get(i) {
                para.level = *lv;
                para.props = pp.clone();
            }
            for r in &mut para.runs {
                r.props = keep.clone();
            }
        }
        body.paragraphs = nb.paragraphs;
        Ok(())
    })?;
    fit_text_box(s, id)?;
    ok()
}

fn get(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let id = match id_param(p, "id") {
        Some(i) => i,
        None => match &st.selection.text {
            Some(t) => t.shape,
            None => st.selection.shapes.first().copied().ok_or_else(|| bad("text.get", "missing `id`"))?,
        },
    };
    let sh = st.shape(id).ok_or_else(|| bad("text.get", "no such shape"))?;
    let t = sh.text.clone().unwrap_or_default();
    Ok(json!({"text": t.text(), "paragraphs": t.paragraphs.iter().map(|p| json!({"level": p.level, "text": p.text()})).collect::<Vec<_>>()}))
}

/// Apply run formatting to the text selection (or, when not editing, to all text of the
/// selected shapes). A caret without a range formats the word it's in (PowerPoint behaviour),
/// or sets the formatting for what is typed next at a word boundary.
pub(crate) fn format_run(s: &mut Session, f: &(dyn Fn(&mut RunProps) + Send + Sync)) -> Result<Value> {
    let st = s.doc()?;
    if let Some(t) = st.selection.text.clone() {
        let (a, b) = format_range(st, &t);
        let collapsed = a == b;
        edit_text(s, |body, _| {
            if collapsed {
                // Next typed text: put the props on an empty paragraph's end mark or keep pending.
                ed::format(body, a, b, f);
            } else {
                ed::format(body, a, b, f);
            }
        })?;
        if collapsed {
            // Remember as pending typing props by inserting nothing: handled via end mark for
            // empty paragraphs; mid-paragraph carets keep surrounding formatting.
        }
        return ok();
    }
    let ids = st.selection.shapes.clone();
    if ids.is_empty() {
        return Err(bad("format", "select text or a shape"));
    }
    edit_shapes(s, &json!({"ids": ids}), "format", |sh| {
        match &mut sh.kind {
            ShapeKind::Table(t) => {
                for row in &mut t.rows {
                    for c in &mut row.cells {
                        ed::format_all(&mut c.text, f);
                    }
                }
            }
            _ => {
                let body = sh.text.get_or_insert_with(TextBody::default);
                ed::format_all(body, f);
            }
        }
        Ok(())
    })?;
    for id in ids {
        fit_text_box(s, id)?;
    }
    ok()
}

/// What `format_run` formats while editing: the selection, or for a bare caret the word it's in
/// (without its trailing space). Empty when the caret is at a word boundary.
fn format_range(st: &DocState, t: &TextSel) -> (Pos, Pos) {
    let body = body_of(st, t).cloned().unwrap_or_default();
    let (mut a, mut b) = t.ordered();
    if a == b {
        let (wa, wb) = ed::word_at(&body, a);
        let inside = wa.1 < a.1 && a.1 < wb.1;
        if inside {
            a = wa;
            b = wb;
            // Don't include the trailing space.
            let line: Vec<char> = body.paragraphs.get(b.0).map(|p| p.text().chars().collect()).unwrap_or_default();
            while b.1 > a.1 && line.get(b.1 - 1) == Some(&' ') {
                b.1 -= 1;
            }
        }
    }
    (a, b)
}

/// Link the text being edited: the selection or the word at the caret, as `format_run` would.
/// With nothing to link (caret at a word boundary, empty paragraph) `display` is inserted at the
/// caret as linked text in the surrounding formatting, as PowerPoint's Insert Hyperlink does.
pub(crate) fn link_text(s: &mut Session, link: Hyperlink, display: &str) -> Result<Value> {
    let st = s.doc()?;
    let Some(t) = st.selection.text.clone() else { return Err(bad("insert.hyperlink", "not editing text")) };
    let (a, b) = format_range(st, &t);
    if a != b {
        return format_run(s, &move |rp| rp.link = Some(link.clone()));
    }
    if display.is_empty() {
        return Err(bad("insert.hyperlink", "nothing to link: select text or give `text`"));
    }
    let pos = edit_text(s, |body, t| {
        let mut props = ed::props_at(body, a);
        props.link = Some(link);
        let c = ed::insert(body, a, display, Some(props));
        t.anchor = c;
        t.caret = c;
        c
    })?;
    Ok(json!({"caret": [pos.0, pos.1]}))
}

/// Apply paragraph changes to the paragraphs touched by the selection (or all paragraphs of the
/// selected shapes).
pub(crate) fn format_para(s: &mut Session, f: &(dyn Fn(&mut deckcraft_model::text::Paragraph) + Send + Sync)) -> Result<Value> {
    let st = s.doc()?;
    if let Some(t) = st.selection.text.clone() {
        let (a, b) = t.ordered();
        edit_text(s, |body, _| {
            if body.paragraphs.is_empty() {
                body.paragraphs.push(Default::default());
            }
            for pi in a.0..=b.0 {
                if let Some(p) = body.paragraphs.get_mut(pi) {
                    f(p);
                }
            }
        })?;
        return ok();
    }
    let ids = st.selection.shapes.clone();
    if ids.is_empty() {
        return Err(bad("format", "select text or a shape"));
    }
    edit_shapes(s, &json!({"ids": ids}), "format", |sh| {
        match &mut sh.kind {
            ShapeKind::Table(t) => {
                for row in &mut t.rows {
                    for c in &mut row.cells {
                        c.text.paragraphs.iter_mut().for_each(f);
                    }
                }
            }
            _ => {
                let body = sh.text.get_or_insert_with(TextBody::default);
                if body.paragraphs.is_empty() {
                    body.paragraphs.push(Default::default());
                }
                body.paragraphs.iter_mut().for_each(f);
            }
        }
        Ok(())
    })?;
    for id in ids {
        fit_text_box(s, id)?;
    }
    ok()
}

/// Apply body (text box) changes to the edited or selected shapes.
pub(crate) fn format_body(s: &mut Session, f: &(dyn Fn(&mut deckcraft_model::text::BodyProps) + Send + Sync)) -> Result<Value> {
    let st = s.doc()?;
    let ids: Vec<ShapeId> = match &st.selection.text {
        Some(t) if !t.notes => vec![t.shape],
        _ => st.selection.shapes.clone(),
    };
    if ids.is_empty() {
        return Err(bad("format", "select a shape"));
    }
    edit_shapes(s, &json!({"ids": ids}), "format", |sh| {
        f(&mut sh.text.get_or_insert_with(TextBody::default).body);
        Ok(())
    })?;
    for id in ids {
        fit_text_box(s, id)?;
    }
    ok()
}
