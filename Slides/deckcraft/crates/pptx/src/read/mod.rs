//! PPTX import: package → [`Presentation`].

mod chart;
pub(crate) mod dml;
mod shapes;
mod theme;
mod timing;

pub(crate) use chart::{chart_from as chart_from_el, read_chart_xml};

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use deckcraft_geom::{Size, emu_to_pt};
use deckcraft_model::text::TextBody;
use deckcraft_model::{
    Background, Comment, CustomShow, Geom, HeaderFooter, Layout, LayoutId, LayoutType, Master, MasterId, MediaId, MediaItem, PhType, Presentation,
    Properties, Section, ShowSettings, Slide, SlideId, defaults,
};

use crate::PptxError;
use crate::opc::{self, Package, Rels};
use crate::xml::{Doc, El, Node};

/// Import state shared by all parts.
pub struct Imp<'a> {
    pub pkg: &'a Package,
    pub media: Vec<MediaItem>,
    pub media_by_part: HashMap<String, MediaId>,
    pub next: u32,
    pub slide_by_part: HashMap<String, SlideId>,
    pub custom_show_names: HashMap<String, String>,
    /// GUIDs (upper case) of the table styles the file defines.
    pub table_style_ids: Vec<String>,
}

impl Imp<'_> {
    pub fn alloc(&mut self) -> u32 {
        self.next = self.next.saturating_add(1);
        self.next
    }
    /// Media item for an internal part, read once.
    pub fn media_for(&mut self, part: &str) -> Option<MediaId> {
        let key = opc::norm(part);
        if let Some(m) = self.media_by_part.get(&key) {
            return Some(*m);
        }
        let data = self.pkg.get(part)?.to_vec();
        let name = opc::file_of(self.pkg.original_name(part).unwrap_or(part)).to_string();
        let content_type = self.pkg.content_type(part).filter(|c| !c.is_empty()).unwrap_or_else(|| crate::mime_for(&name).to_string());
        let id = MediaId(self.alloc());
        self.media.push(MediaItem { id, name, content_type, data: Arc::new(data), link: None });
        self.media_by_part.insert(key, id);
        Some(id)
    }
    /// Media item for an external link.
    pub fn media_link(&mut self, url: &str) -> MediaId {
        if let Some(m) = self.media.iter().find(|m| m.link.as_deref() == Some(url)) {
            return m.id;
        }
        let id = MediaId(self.alloc());
        let name = url.rsplit('/').next().unwrap_or(url).to_string();
        let content_type = crate::mime_for(&name).to_string();
        self.media.push(MediaItem { id, name, content_type, data: Arc::new(vec![]), link: Some(url.to_string()) });
        id
    }
}

/// One part being read: its name, relationships and root namespace declarations.
pub struct Part {
    pub name: String,
    pub rels: Rels,
    pub ns: Vec<(String, String)>,
}

impl Part {
    pub fn new(pkg: &Package, name: &str, doc: &Doc) -> Self {
        Part { name: name.to_string(), rels: pkg.rels(name), ns: doc.ns_decls() }
    }
    /// Serialize an element with the namespace declarations it needs from the part root.
    pub fn keep(&self, el: &El) -> String {
        keep_xml(el, &self.ns)
    }
}

/// Serialize `el`, declaring every prefix it uses that the source root declared.
pub fn keep_xml(el: &El, ns: &[(String, String)]) -> String {
    let mut used = HashSet::new();
    fn rec(e: &El, used: &mut HashSet<String>, depth: usize) {
        if depth > crate::xml::MAX_DEPTH {
            return;
        }
        if let Some((p, _)) = e.name.split_once(':') {
            used.insert(p.to_string());
        }
        for (k, _) in &e.attrs {
            if let Some((p, _)) = k.split_once(':')
                && p != "xmlns"
                && p != "xml"
            {
                used.insert(p.to_string());
            }
        }
        for c in e.elements() {
            rec(c, used, depth + 1);
        }
    }
    rec(el, &mut used, 0);
    let mut el = el.clone();
    for (k, v) in ns {
        let p = k.strip_prefix("xmlns:").unwrap_or("");
        if !p.is_empty() && used.contains(p) && !el.attrs.iter().any(|(a, _)| a == k) {
            el.attrs.push((k.clone(), v.clone()));
        }
    }
    el.to_xml()
}

/// Namespace prefixes whose `mc:Choice` content we understand.
const SUPPORTED_MC: [&str; 9] = ["p14", "p15", "p159", "a14", "a16", "p1510", "a15", "c14", "p188"];

/// Replace every `mc:AlternateContent` by the children of the branch we can read best.
pub fn resolve_mc(el: &mut El, depth: usize) {
    if depth > crate::xml::MAX_DEPTH {
        return;
    }
    if el.children.iter().any(|c| matches!(c, Node::El(e) if e.is("AlternateContent"))) {
        let mut out = Vec::with_capacity(el.children.len());
        for c in std::mem::take(&mut el.children) {
            match c {
                Node::El(ac) if ac.is("AlternateContent") => {
                    let choice = ac.children_named("Choice").find(|ch| {
                        let req_ok = ch.attr("Requires").unwrap_or("").split_whitespace().all(|r| SUPPORTED_MC.contains(&r));
                        // Ink content parts are not modelled; prefer their fallback picture.
                        let modelled = !ch.elements().any(|e| e.is("contentPart"));
                        req_ok && modelled
                    });
                    let branch = choice.or_else(|| ac.child("Fallback"));
                    if let Some(b) = branch {
                        for n in &b.children {
                            if let Node::El(e) = n {
                                let mut e = e.clone();
                                // Carry namespace declarations from the branch onto its content.
                                for (k, v) in b.attrs.iter().chain(ac.attrs.iter()) {
                                    if k.starts_with("xmlns") && !e.attrs.iter().any(|(a, _)| a == k) {
                                        e.attrs.push((k.clone(), v.clone()));
                                    }
                                }
                                out.push(Node::El(e));
                            }
                        }
                    }
                }
                other => out.push(other),
            }
        }
        el.children = out;
    }
    for c in el.elements_mut() {
        resolve_mc(c, depth + 1);
    }
}

fn load(pkg: &Package, name: &str) -> Option<Doc> {
    let mut d = pkg.xml(name)?;
    resolve_mc(&mut d.root, 0);
    Some(d)
}

fn max_cnvpr(el: &El, m: &mut u32) {
    let mut v = vec![];
    el.find_all("cNvPr", &mut v);
    for c in v {
        if let Some(id) = c.u32("id")
            && id < shapes::MAX_KEPT_ID
        {
            *m = (*m).max(id);
        }
    }
}

pub fn import(bytes: &[u8]) -> Result<Presentation, PptxError> {
    let pkg = Package::open(bytes)?;
    let root_rels = pkg.rels("");
    let pres_path = root_rels.first_of("officeDocument").map(|r| r.target.clone()).unwrap_or_else(|| "ppt/presentation.xml".into());
    let pres_doc = load(&pkg, &pres_path).ok_or_else(|| PptxError::NotPptx(format!("missing or unreadable {pres_path}")))?;
    let pres_el = &pres_doc.root;
    if !pres_el.is("presentation") {
        return Err(PptxError::NotPptx(format!("{pres_path} is not a presentation part")));
    }
    let pres_part = Part::new(&pkg, &pres_path, &pres_doc);

    // Masters, layouts and slides to read.
    let master_paths: Vec<String> = pres_el
        .child("sldMasterIdLst")
        .map(|l| l.children_named("sldMasterId").filter_map(|m| pres_part.rels.target(m.attr("r:id").unwrap_or(""))).map(String::from).collect())
        .unwrap_or_default();
    let master_paths: Vec<String> =
        if master_paths.is_empty() { pres_part.rels.all_of("slideMaster").map(|r| r.target.clone()).collect() } else { master_paths };
    let slide_entries: Vec<(u32, String)> = pres_el
        .child("sldIdLst")
        .map(|l| {
            l.children_named("sldId")
                .take(10_000)
                .filter_map(|s| Some((s.u32("id").unwrap_or(0), pres_part.rels.target(s.attr("r:id").unwrap_or(""))?.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let mut docs: HashMap<String, Doc> = HashMap::new();
    let mut max_id = 0u32;
    let mut all_parts: Vec<String> = master_paths.clone();
    for m in &master_paths {
        all_parts.extend(pkg.rels(m).all_of("slideLayout").map(|r| r.target.clone()));
    }
    all_parts.extend(slide_entries.iter().map(|(_, p)| p.clone()));
    for p in &all_parts {
        let key = opc::norm(p);
        if docs.contains_key(&key) {
            continue;
        }
        if let Some(d) = load(&pkg, p) {
            max_cnvpr(&d.root, &mut max_id);
            docs.insert(key, d);
        }
    }

    let mut imp = Imp {
        pkg: &pkg,
        media: vec![],
        media_by_part: HashMap::new(),
        next: max_id.max(1),
        slide_by_part: HashMap::new(),
        custom_show_names: HashMap::new(),
        table_style_ids: Vec::new(),
    };
    // Slide ids first so hyperlinks anywhere can point at slides.
    let mut slide_ids: Vec<(u32, String, SlideId)> = vec![];
    for (file_id, path) in &slide_entries {
        if !docs.contains_key(&opc::norm(path)) {
            log::warn!("pptx: slide {path} is missing or unreadable; skipped");
            continue;
        }
        let id = SlideId(imp.alloc());
        imp.slide_by_part.insert(opc::norm(path), id);
        slide_ids.push((*file_id, path.clone(), id));
    }
    if let Some(l) = pres_el.child("custShowLst") {
        for cs in l.children_named("custShow") {
            if let (Some(id), Some(name)) = (cs.attr("id"), cs.attr("name")) {
                imp.custom_show_names.insert(id.to_string(), name.to_string());
            }
        }
    }

    let mut p = defaults::blank_presentation(defaults::WIDE, Default::default(), false);
    p.masters.clear();
    p.media.clear();
    p.default_text_style = Default::default();

    if let Some(sz) = pres_el.child("sldSz") {
        let w = sz.i64("cx").map(emu_to_pt).unwrap_or(960.0);
        let h = sz.i64("cy").map(emu_to_pt).unwrap_or(540.0);
        p.slide_size = Size::new(sane_len(w, 960.0), sane_len(h, 540.0));
    }
    if let Some(sz) = pres_el.child("notesSz") {
        let w = sz.i64("cx").map(emu_to_pt).unwrap_or(540.0);
        let h = sz.i64("cy").map(emu_to_pt).unwrap_or(720.0);
        p.notes_size = Size::new(sane_len(w, 540.0), sane_len(h, 720.0));
    }
    p.first_slide_number = pres_el.u32("firstSlideNum").unwrap_or(1).min(9999);
    if let Some(dts) = pres_el.child("defaultTextStyle") {
        p.default_text_style = dml::list_style(&mut imp, &pres_part, dts);
    }
    if let Some(r) = pres_part.rels.first_of("tableStyles")
        && let Some(d) = load(&pkg, &r.target)
    {
        let part = Part::new(&pkg, &r.target, &d);
        p.table_styles = table_styles(&mut imp, &part, &d.root);
        imp.table_style_ids = p.table_styles.iter().map(|s| s.id.clone()).collect();
    }

    // Masters and layouts.
    let mut layout_by_part: HashMap<String, LayoutId> = HashMap::new();
    for mp in &master_paths {
        let Some(doc) = docs.get(&opc::norm(mp)) else {
            log::warn!("pptx: master {mp} missing; skipped");
            continue;
        };
        let master = read_master(&mut imp, mp, doc, &docs, &mut layout_by_part);
        p.masters.push(Arc::new(master));
    }
    if p.masters.is_empty() {
        log::warn!("pptx: no readable slide master; using a default one");
        let (m, _) = defaults::build_master(p.slide_size, Default::default(), imp.next);
        imp.next = imp.next.saturating_add(200);
        p.masters.push(Arc::new(m));
    }
    let fallback_layout = p.masters.first().and_then(|m| m.layouts.first()).map(|l| l.id).unwrap_or_default();

    // Slides.
    let mut file_slide_ids: HashMap<u32, SlideId> = HashMap::new();
    for (file_id, path, id) in &slide_ids {
        let Some(doc) = docs.get(&opc::norm(path)) else { continue };
        let part = Part::new(&pkg, path, doc);
        let layout = part.rels.first_of("slideLayout").and_then(|r| layout_by_part.get(&opc::norm(&r.target)).copied()).unwrap_or(fallback_layout);
        let mut slide = read_slide(&mut imp, &part, doc, *id, layout);
        // Notes.
        if let Some(n) = part.rels.first_of("notesSlide")
            && let Some(nd) = load(&pkg, &n.target)
        {
            let np = Part::new(&pkg, &n.target, &nd);
            slide.notes = read_notes(&mut imp, &np, &nd.root);
        }
        // Comments.
        for c in part.rels.all_of("comments") {
            if let Some(cd) = pkg.xml(&c.target) {
                slide.comments.extend(read_comments(&pkg, &pres_part, &cd.root));
            }
        }
        file_slide_ids.insert(*file_id, *id);
        p.slides.push(Arc::new(slide));
    }

    // Sections.
    if let Some(ext) = pres_el.child("extLst") {
        if let Some(sl) = ext.find("sectionLst") {
            for s in sl.children_named("section").take(10_000) {
                let slides = s
                    .child("sldIdLst")
                    .map(|l| l.children_named("sldId").filter_map(|x| x.u32("id").and_then(|i| file_slide_ids.get(&i).copied())).collect())
                    .unwrap_or_default();
                p.sections.push(Section { name: s.attr("name").unwrap_or("").to_string(), slides });
            }
        }
        let kept: String = ext
            .children_named("ext")
            .filter(|e| e.attr("uri") != Some(crate::SECTION_EXT_URI) && !has_rel_refs(e))
            .map(|e| pres_part.keep(e))
            .collect();
        if !kept.is_empty() {
            p.raw_ext = Some(kept);
        }
    }
    // Custom shows.
    if let Some(l) = pres_el.child("custShowLst") {
        for cs in l.children_named("custShow") {
            let slides = cs
                .child("sldLst")
                .map(|l| {
                    l.children_named("sld")
                        .filter_map(|s| pres_part.rels.target(s.attr("r:id").unwrap_or("")))
                        .filter_map(|t| imp.slide_by_part.get(&opc::norm(t)).copied())
                        .collect()
                })
                .unwrap_or_default();
            p.custom_shows.push(CustomShow { name: cs.attr("name").unwrap_or("").to_string(), slides });
        }
    }
    // Show settings.
    if let Some(r) = pres_part.rels.first_of("presProps")
        && let Some(d) = pkg.xml(&r.target)
        && let Some(sp) = d.root.child("showPr")
    {
        p.show = read_show(sp, &imp.custom_show_names);
    }
    // Notes master.
    if let Some(r) = pres_part.rels.first_of("notesMaster")
        && let Some(d) = load(&pkg, &r.target)
    {
        let part = Part::new(&pkg, &r.target, &d);
        let mut ctx = shapes::IdCtx::default();
        let mut m = Master { id: MasterId(imp.alloc()), name: "Notes Master".into(), ..Default::default() };
        if let Some(t) = part.rels.first_of("theme")
            && let Some(td) = pkg.xml(&t.target)
        {
            let tp = Part::new(&pkg, &t.target, &td);
            m.theme = theme::read_theme(&mut imp, &tp, &td.root);
        }
        if let Some(tree) = d.root.path(&["cSld", "spTree"]) {
            m.shapes = shapes::sp_tree(&mut imp, &part, &mut ctx, tree, 0);
        }
        if let Some(cm) = d.root.child("clrMap") {
            m.color_map = color_map(cm);
        }
        p.notes_master = Some(Arc::new(m));
    }
    // Document properties.
    if let Some(r) = root_rels.first_of("core-properties")
        && let Some(d) = pkg.xml(&r.target)
    {
        read_core(&d.root, &mut p.props);
    }
    if let Some(r) = root_rels.first_of("extended-properties")
        && let Some(d) = pkg.xml(&r.target)
    {
        p.props.company = d.root.child("Company").map(|c| c.text()).unwrap_or_default();
    }

    inherit_placeholder_geometry(&mut p);
    p.header_footer = header_footer(&p);
    p.media = std::mem::take(&mut imp.media);
    p.next_id = imp.next;
    p.fix_next_id();
    Ok(p)
}

fn sane_len(v: f64, dflt: f64) -> f64 {
    if v.is_finite() && (1.0..=20_000.0).contains(&v) { v } else { dflt }
}

/// Does the element (or a descendant) reference a relationship?
pub fn has_rel_refs(el: &El) -> bool {
    fn rec(e: &El, depth: usize) -> bool {
        if depth > crate::xml::MAX_DEPTH {
            return false;
        }
        let own = e.attrs.iter().any(|(k, v)| {
            !k.starts_with("xmlns")
                && k.contains(':')
                && matches!(k.rsplit(':').next(), Some("id" | "embed" | "link" | "dm" | "lo" | "qs" | "cs" | "pict" | "href"))
                && !v.is_empty()
        });
        own || e.elements().any(|c| rec(c, depth + 1))
    }
    rec(el, 0)
}

pub fn color_map(cm: &El) -> Vec<(String, String)> {
    cm.attrs.iter().filter(|(k, _)| !k.starts_with("xmlns")).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Placeholders read without a geometry (an empty custom geometry, see `shapes::sp`) take the
/// geometry of the layout or master placeholder they inherit from, as PowerPoint draws them: a
/// layout's picture placeholder cut into a shape keeps that shape on the slide. Anything left
/// is a rectangle.
fn inherit_placeholder_geometry(p: &mut Presentation) {
    let unset = |g: &Geom| matches!(g, Geom::Custom { paths } if paths.is_empty());
    // Masters first, then layouts (from their masters), then slides (from layouts and masters).
    for m in &mut p.masters {
        let m = Arc::make_mut(m);
        walk_shapes_mut(&mut m.shapes, &mut |s| {
            if unset(&s.geom) {
                s.geom = Geom::default();
            }
        });
    }
    let mut layout_geoms: Vec<(LayoutId, usize, Geom)> = vec![];
    for m in &p.masters {
        for l in &m.layouts {
            let ctx = deckcraft_model::resolve::Ctx::for_layout(p, m, l);
            for (i, s) in l.shapes.iter().enumerate() {
                if unset(&s.geom) {
                    let (_, mp) = deckcraft_model::resolve::parents(&ctx, s);
                    layout_geoms.push((l.id, i, mp.map(|m| m.geom.clone()).filter(|g| !unset(g)).unwrap_or_default()));
                }
            }
        }
    }
    for m in &mut p.masters {
        let m = Arc::make_mut(m);
        for l in &mut m.layouts {
            for (id, i, g) in layout_geoms.iter().filter(|(id, _, _)| *id == l.id) {
                let _ = id;
                if let Some(s) = l.shapes.get_mut(*i) {
                    s.geom = g.clone();
                }
            }
        }
    }
    let mut slide_geoms: Vec<(usize, usize, Geom)> = vec![];
    for (si, sl) in p.slides.iter().enumerate() {
        let Some(ctx) = deckcraft_model::resolve::Ctx::for_slide(p, sl) else { continue };
        for (i, s) in sl.shapes.iter().enumerate() {
            if unset(&s.geom) {
                let (lp, mp) = deckcraft_model::resolve::parents(&ctx, s);
                let g = lp.map(|l| l.geom.clone()).filter(|g| !unset(g)).or_else(|| mp.map(|m| m.geom.clone()).filter(|g| !unset(g)));
                slide_geoms.push((si, i, g.unwrap_or_default()));
            }
        }
    }
    for (si, i, g) in slide_geoms {
        if let Some(sl) = p.slides.get_mut(si)
            && let Some(s) = Arc::make_mut(sl).shapes.get_mut(i)
        {
            s.geom = g;
        }
    }
    // Placeholders inside groups (rare) and anything still unset: rectangles.
    for sl in &mut p.slides {
        walk_shapes_mut(&mut Arc::make_mut(sl).shapes, &mut |s| {
            if unset(&s.geom) {
                s.geom = Geom::default();
            }
        });
    }
    for m in &mut p.masters {
        for l in &mut Arc::make_mut(m).layouts {
            walk_shapes_mut(&mut l.shapes, &mut |s| {
                if unset(&s.geom) {
                    s.geom = Geom::default();
                }
            });
        }
    }
}

/// Every shape, groups included.
fn walk_shapes_mut(shapes: &mut [deckcraft_model::Shape], f: &mut dyn FnMut(&mut deckcraft_model::Shape)) {
    for s in shapes {
        f(s);
        if let deckcraft_model::ShapeKind::Group { children, .. } = &mut s.kind {
            walk_shapes_mut(children, f);
        }
    }
}

/// `a:tblStyleLst`: the table styles the file defines.
fn table_styles(imp: &mut Imp, part: &Part, root: &El) -> Vec<deckcraft_model::table::TableStyleDef> {
    use deckcraft_model::table::{TableBorder, TablePart, TablePartStyle, TableStyleDef};
    root.children_named("tblStyle")
        .take(256)
        .filter_map(|st| {
            let id = st.attr("styleId")?.to_ascii_uppercase();
            let mut def = TableStyleDef { id, name: st.attr("styleName").unwrap_or("").to_string(), raw: part.keep(st), ..Default::default() };
            if let Some(bg) = st.child("tblBg") {
                let mut ps = TablePartStyle::default();
                if let Some(f) = bg.elements().find(|e| e.local().ends_with("Fill")) {
                    ps.fill = dml::fill_el(imp, part, f);
                }
                if let Some(fr) = bg.child("fillRef") {
                    ps.fill_ref = Some((
                        fr.u32("idx").unwrap_or(0),
                        dml::color(fr).unwrap_or(deckcraft_model::ColorRef::scheme(deckcraft_color::SchemeSlot::Accent1)),
                    ));
                }
                def.background = Some(ps);
            }
            for el in st.elements() {
                let Some(kind) = TablePart::from_xml(el.local()) else { continue };
                let mut ps = TablePartStyle::default();
                if let Some(tx) = el.child("tcTxStyle") {
                    ps.text_color = dml::color(tx);
                    ps.bold = tx.attr("b").map(|v| v == "on");
                    ps.italic = tx.attr("i").map(|v| v == "on");
                }
                if let Some(tc) = el.child("tcStyle") {
                    if let Some(f) = tc.child("fill").and_then(|f| f.elements().next()) {
                        ps.fill = dml::fill_el(imp, part, f);
                    }
                    if let Some(fr) = tc.child("fillRef") {
                        ps.fill_ref = Some((
                            fr.u32("idx").unwrap_or(0),
                            dml::color(fr).unwrap_or(deckcraft_model::ColorRef::scheme(deckcraft_color::SchemeSlot::Accent1)),
                        ));
                    }
                    if let Some(bd) = tc.child("tcBdr") {
                        for (i, side) in ["left", "right", "top", "bottom", "insideH", "insideV"].iter().enumerate() {
                            let Some(b) = bd.child(side) else { continue };
                            let border = if let Some(ln) = b.child("ln") {
                                Some(TableBorder::Line { line: dml::line(imp, part, ln) })
                            } else {
                                b.child("lnRef").map(|r| TableBorder::Ref {
                                    idx: r.u32("idx").unwrap_or(0),
                                    color: dml::color(r).unwrap_or(deckcraft_model::ColorRef::scheme(deckcraft_color::SchemeSlot::Accent1)),
                                })
                            };
                            if let Some(slot) = ps.borders.get_mut(i) {
                                *slot = border;
                            }
                        }
                    }
                }
                def.parts.push((kind, ps));
            }
            Some(def)
        })
        .collect()
}

fn background(imp: &mut Imp, part: &Part, csld: &El) -> Option<Background> {
    let bg = csld.child("bg")?;
    if let Some(pr) = bg.child("bgPr") {
        return Some(Background::Fill { fill: dml::fill_of(imp, part, pr).unwrap_or(deckcraft_model::Fill::None) });
    }
    let r = bg.child("bgRef")?;
    Some(Background::Ref {
        idx: r.u32("idx").unwrap_or(1001),
        color: dml::color(r).unwrap_or(deckcraft_model::ColorRef::scheme(deckcraft_color::SchemeSlot::Bg1)),
    })
}

fn read_master(imp: &mut Imp, path: &str, doc: &Doc, docs: &HashMap<String, Doc>, layout_by_part: &mut HashMap<String, LayoutId>) -> Master {
    let pkg = imp.pkg;
    let part = Part::new(pkg, path, doc);
    let root = &doc.root;
    let mut m = Master { id: MasterId(0), preserve: root.bool("preserve").unwrap_or(false), ..Default::default() };
    if let Some(t) = part.rels.first_of("theme")
        && let Some(td) = pkg.xml(&t.target)
    {
        let tp = Part::new(pkg, &t.target, &td);
        m.theme = theme::read_theme(imp, &tp, &td.root);
    }
    m.id = MasterId(imp.alloc());
    if let Some(cm) = root.child("clrMap") {
        m.color_map = color_map(cm);
    }
    if let Some(csld) = root.child("cSld") {
        m.name = csld.attr("name").unwrap_or("").to_string();
        m.background = background(imp, &part, csld);
        if let Some(tree) = csld.child("spTree") {
            let mut ctx = shapes::IdCtx::default();
            m.shapes = shapes::sp_tree(imp, &part, &mut ctx, tree, 0);
        }
    }
    if m.name.is_empty() {
        m.name = m.theme.name.clone();
    }
    if let Some(ts) = root.child("txStyles") {
        if let Some(s) = ts.child("titleStyle") {
            m.title_style = dml::list_style(imp, &part, s);
        }
        if let Some(s) = ts.child("bodyStyle") {
            m.body_style = dml::list_style(imp, &part, s);
        }
        if let Some(s) = ts.child("otherStyle") {
            m.other_style = dml::list_style(imp, &part, s);
        }
    }
    // Layouts in list order, then any other related layouts.
    let mut lpaths: Vec<String> = root
        .child("sldLayoutIdLst")
        .map(|l| l.children_named("sldLayoutId").filter_map(|x| part.rels.target(x.attr("r:id").unwrap_or(""))).map(String::from).collect())
        .unwrap_or_default();
    for r in part.rels.all_of("slideLayout") {
        if !lpaths.iter().any(|x| opc::norm(x) == opc::norm(&r.target)) {
            lpaths.push(r.target.clone());
        }
    }
    for lp in lpaths.iter().take(1000) {
        let key = opc::norm(lp);
        if layout_by_part.contains_key(&key) {
            continue;
        }
        let Some(ld) = docs.get(&key) else { continue };
        let lpart = Part::new(pkg, lp, ld);
        let l = read_layout(imp, &lpart, ld);
        layout_by_part.insert(key, l.id);
        m.layouts.push(l);
    }
    if m.layouts.is_empty() {
        // A master must have a layout for slides to use.
        imp.next = imp.next.saturating_add(1);
        m.layouts.push(Layout {
            id: LayoutId(imp.next),
            name: "Blank".into(),
            kind: LayoutType::Blank,
            show_master_shapes: true,
            ..Default::default()
        });
    }
    if let Some(ext) = root.child("extLst").filter(|e| !has_rel_refs(e)) {
        m.raw_ext = Some(ext.elements().map(|e| part.keep(e)).collect());
    }
    m
}

fn read_layout(imp: &mut Imp, part: &Part, doc: &Doc) -> Layout {
    let root = &doc.root;
    let mut l = Layout {
        id: LayoutId(imp.alloc()),
        kind: LayoutType::from_xml(root.attr("type").unwrap_or("cust")),
        show_master_shapes: root.bool("showMasterSp").unwrap_or(true),
        preserve: root.bool("preserve").unwrap_or(false),
        ..Default::default()
    };
    if let Some(csld) = root.child("cSld") {
        l.name = csld.attr("name").unwrap_or("").to_string();
        l.background = background(imp, part, csld);
        if let Some(tree) = csld.child("spTree") {
            let mut ctx = shapes::IdCtx::default();
            l.shapes = shapes::sp_tree(imp, part, &mut ctx, tree, 0);
        }
    }
    l.color_map = color_map_override(root);
    if let Some(ext) = root.child("extLst").filter(|e| !has_rel_refs(e)) {
        l.raw_ext = Some(ext.elements().map(|e| part.keep(e)).collect());
    }
    l
}

/// A layout's or slide's `clrMapOvr/overrideClrMapping` (None for `masterClrMapping` or none).
fn color_map_override(root: &El) -> Option<Vec<(String, String)>> {
    root.child("clrMapOvr")?.child("overrideClrMapping").map(color_map)
}

fn read_slide(imp: &mut Imp, part: &Part, doc: &Doc, id: SlideId, layout: LayoutId) -> Slide {
    let root = &doc.root;
    let mut s = Slide {
        id,
        layout,
        hidden: root.bool("show") == Some(false),
        show_master_shapes: root.bool("showMasterSp").unwrap_or(true),
        color_map: color_map_override(root),
        ..Default::default()
    };
    let mut ctx = shapes::IdCtx::default();
    if let Some(csld) = root.child("cSld") {
        s.name = csld.attr("name").unwrap_or("").to_string();
        s.background = background(imp, part, csld);
        if let Some(tree) = csld.child("spTree") {
            s.shapes = shapes::sp_tree(imp, part, &mut ctx, tree, 0);
        }
        if let Some(c) = csld.child("controls") {
            let pics = shapes::controls(imp, part, &mut ctx, c);
            s.shapes.extend(pics);
        }
    }
    if let Some(t) = root.child("transition") {
        s.transition = timing::transition(imp, part, t);
    }
    if let Some(t) = root.child("timing") {
        s.animations = timing::animations(imp, part, &ctx, t, &s.shapes);
    }
    if let Some(ext) = root.child("extLst").filter(|e| !has_rel_refs(e)) {
        s.raw_ext = Some(ext.elements().map(|e| part.keep(e)).collect());
    }
    s
}

fn read_notes(imp: &mut Imp, part: &Part, root: &El) -> TextBody {
    let Some(tree) = root.path(&["cSld", "spTree"]) else { return TextBody::default() };
    for sp in tree.children_named("sp") {
        let ph = sp.path(&["nvSpPr", "nvPr", "ph"]);
        let is_body = ph.is_some_and(|ph| matches!(ph.attr("type"), Some("body") | None) && ph.attr("type") != Some("sldImg"));
        if is_body && let Some(tx) = sp.child("txBody") {
            let mut t = dml::text_body(imp, part, tx);
            // Notes keep only content: drop body formatting from the notes master placeholder.
            t.list_style = Default::default();
            if t.is_empty() {
                return TextBody::default();
            }
            return t;
        }
    }
    TextBody::default()
}

fn read_comments(pkg: &Package, pres: &Part, root: &El) -> Vec<Comment> {
    // Authors: legacy commentAuthors.xml (numeric ids) and modern authors.xml (GUIDs).
    let mut authors: HashMap<String, (String, String)> = HashMap::new();
    for kind in ["commentAuthors", "authors"] {
        if let Some(r) = pres.rels.first_of(kind)
            && let Some(d) = pkg.xml(&r.target)
        {
            for a in d.root.elements() {
                if let Some(id) = a.attr("id") {
                    authors.insert(id.to_string(), (a.attr("name").unwrap_or("").to_string(), a.attr("initials").unwrap_or("").to_string()));
                }
            }
        }
    }
    fn one(e: &El, authors: &HashMap<String, (String, String)>, depth: usize) -> Comment {
        let (author, initials) = e.attr("authorId").and_then(|a| authors.get(a).cloned()).unwrap_or_default();
        let text = if let Some(t) = e.child("text") {
            t.text()
        } else if let Some(tb) = e.child("txBody") {
            tb.children_named("p").map(|p| p.text()).collect::<Vec<_>>().join("\n")
        } else {
            String::new()
        };
        let pos = e.child("pos");
        let replies = if depth < 4 {
            e.child("replyLst").map(|l| l.children_named("reply").take(1000).map(|r| one(r, authors, depth + 1)).collect()).unwrap_or_default()
        } else {
            vec![]
        };
        Comment {
            author,
            initials,
            text,
            date: e.attr("dt").or(e.attr("created")).unwrap_or("").to_string(),
            x: pos.and_then(|p| p.f64("x")).unwrap_or(0.0) / 8.0,
            y: pos.and_then(|p| p.f64("y")).unwrap_or(0.0) / 8.0,
            resolved: e.attr("status") == Some("resolved"),
            replies,
            shape: None,
        }
    }
    root.children_named("cm").take(10_000).map(|c| one(c, &authors, 0)).collect()
}

fn read_show(sp: &El, shows: &HashMap<String, String>) -> ShowSettings {
    let mut s = ShowSettings {
        loop_until_esc: sp.bool("loop").unwrap_or(false),
        without_narration: sp.bool("showNarration") == Some(false),
        without_animation: sp.bool("showAnimation") == Some(false),
        use_timings: sp.bool("useTimings").unwrap_or(true),
        ..Default::default()
    };
    if sp.child("browse").is_some() {
        s.show_type = "browsed".into();
    } else if sp.child("kiosk").is_some() {
        s.show_type = "kiosk".into();
    }
    if let Some(r) = sp.child("sldRg") {
        s.range = Some((r.u32("st").unwrap_or(1).max(1), r.u32("end").unwrap_or(1).max(1)));
    }
    if let Some(c) = sp.child("custShow") {
        let id = c.attr("id").unwrap_or("");
        s.custom_show = Some(shows.get(id).cloned().unwrap_or_else(|| id.to_string()));
    }
    if let Some(c) = sp.child("penClr").and_then(dml::color)
        && let deckcraft_model::ColorBase::Rgb { rgb } = c.base
    {
        s.pen_color = rgb;
    }
    s
}

fn read_core(root: &El, p: &mut Properties) {
    for e in root.elements() {
        let t = e.text();
        match e.local() {
            "title" => p.title = t,
            "subject" => p.subject = t,
            "creator" => p.author = t,
            "keywords" => p.keywords = t,
            "description" => p.comments = t,
            "category" => p.category = t,
            "lastModifiedBy" => p.last_modified_by = t,
            "revision" => p.revision = t.trim().parse().unwrap_or(0),
            "created" => p.created = t,
            "modified" => p.modified = t,
            _ => {}
        }
    }
}

/// Which footer placeholders the slides show.
fn header_footer(p: &Presentation) -> HeaderFooter {
    let mut hf = HeaderFooter::default();
    for s in &p.slides {
        for sh in &s.shapes {
            match sh.ph_type() {
                Some(PhType::SlideNum) => hf.slide_number = true,
                Some(PhType::Date) => {
                    hf.date = true;
                    if let Some(t) = &sh.text
                        && !t.paragraphs.iter().flat_map(|p| &p.runs).any(|r| matches!(r.kind, deckcraft_model::text::RunKind::Field { .. }))
                        && hf.date_text.is_empty()
                    {
                        hf.date_text = t.text();
                    }
                }
                Some(PhType::Footer) => {
                    hf.footer = true;
                    if hf.footer_text.is_empty()
                        && let Some(t) = &sh.text
                    {
                        hf.footer_text = t.text();
                    }
                }
                _ => {}
            }
        }
    }
    hf
}
