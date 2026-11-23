//! Residual equations for each constraint type.

use cadcraft_doc::{Constraint, ConstraintKind, DistAxis, GeomRef};
use cadcraft_geom::Vec2;

use crate::curves::circle_at;
use crate::model::Model;

/// Contact parameter samples per curve when guessing where two curves touch.
const CONTACT_SAMPLES: usize = 72;

/// How a constraint's references were interpreted (decided once from the starting geometry).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Form {
    /// Point–point.
    PP,
    /// Point on/at a line (refs: point, line).
    PL {
        swap: bool,
    },
    /// Point on/at a circle (refs: point, circle).
    PC {
        swap: bool,
    },
    /// One line (or two points for horizontal/vertical).
    L1,
    /// Two points (horizontal/vertical/distance).
    P2,
    LL,
    CC,
    /// Line–circle (tangent).
    LC {
        swap: bool,
    },
    /// Symmetric line endpoints, with endpoint pairing crossed or not.
    SymLines {
        crossed: bool,
    },
    /// Circle or arc alone.
    C1,
    /// Arc sweep (angular on an arc).
    ArcSweep,
    /// Two lines measured along the rays from their intersection to each line's far end, the
    /// angle the dimension draws; `rev` marks a line whose far end is its start point.
    Rays {
        rev: [bool; 2],
    },
    /// Fix of a derived point (residual to an anchor).
    Anchor(Vec2),
    /// Fix handled by locking parameters (no rows).
    Lock,
    /// Point on an ellipse (refs: point, ellipse).
    PE {
        swap: bool,
    },
    /// Line tangent to an ellipse.
    LE {
        swap: bool,
    },
    /// Two curves (circles, arcs, ellipses; at least one ellipse) touching at a contact point
    /// given by one auxiliary parameter per curve.
    Contact,
    /// A spline end on a line, circle, arc or ellipse and tangent to it (`spline`: which
    /// reference is the spline).
    EndOn {
        spline: usize,
        at_end: bool,
    },
    /// Two curve ends joined with a common tangent (G1), and common curvature when `g2`.
    Join {
        a_end: bool,
        b_end: bool,
        g2: bool,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Built {
    pub id: u32,
    pub kind: ConstraintKind,
    pub refs: Vec<GeomRef>,
    pub form: Form,
    /// Target value (lengths in drawing units, angles in radians).
    pub target: f64,
    /// Orientation chosen from the starting geometry (±1).
    pub sign: f64,
    pub rows: usize,
    /// Starting values of auxiliary unknowns (contact parameters), appended to the parameter
    /// vector at `aux` by [`crate::interpret`].
    pub aux_init: Vec<f64>,
    pub aux: usize,
}

fn dir_n(a: Vec2, b: Vec2) -> Vec2 {
    let d = b - a;
    let l = d.len();
    if l > 1e-12 { d * (1.0 / l) } else { d }
}

fn ray(a: Vec2, b: Vec2, rev: bool) -> Vec2 {
    if rev { dir_n(b, a) } else { dir_n(a, b) }
}

fn sgn(v: f64) -> f64 {
    if v < 0.0 { -1.0 } else { 1.0 }
}

fn wrap(a: f64) -> f64 {
    let t = std::f64::consts::TAU;
    let r = (a + std::f64::consts::PI).rem_euclid(t) - std::f64::consts::PI;
    if r.is_finite() { r } else { 0.0 }
}

fn r_at(c: &Constraint, i: usize) -> Option<&GeomRef> {
    c.refs.get(i)
}

impl Built {
    /// Interpret a constraint against the model at `x`. `target` is the evaluated value for
    /// dimensional constraints (degrees for angular). Errors explain what references are needed.
    pub fn new(c: &Constraint, m: &Model, x: &[f64], target: f64) -> Result<Built, String> {
        use ConstraintKind as K;
        let refs = c.refs.clone();
        let pt = |i: usize| r_at(c, i).and_then(|r| m.point(x, r));
        let ln = |i: usize| r_at(c, i).and_then(|r| m.line(x, r));
        let ci = |i: usize| r_at(c, i).and_then(|r| m.circle(x, r));
        let n = c.refs.len();
        let el = |i: usize| r_at(c, i).and_then(|r| m.ellipse(x, r));
        let lnx = |i: usize| ln(i).or_else(|| el(i).map(|e| (e.c - e.mj, e.c + e.mj)));
        let mut b = Built { id: c.id, kind: c.kind, refs, form: Form::PP, target, sign: 1.0, rows: 0, aux_init: Vec::new(), aux: 0 };
        let need = |what: &str| Err(format!("{} needs {what}", c.kind.name()));
        match c.kind {
            K::Coincident => {
                if n != 2 {
                    return need("two references");
                }
                b.form = if pt(0).is_some() && pt(1).is_some() {
                    b.rows = 2;
                    Form::PP
                } else if pt(0).is_some() && ln(1).is_some() {
                    Form::PL { swap: false }
                } else if pt(1).is_some() && ln(0).is_some() {
                    Form::PL { swap: true }
                } else if pt(0).is_some() && ci(1).is_some() {
                    Form::PC { swap: false }
                } else if pt(1).is_some() && ci(0).is_some() {
                    Form::PC { swap: true }
                } else if pt(0).is_some() && el(1).is_some() {
                    Form::PE { swap: false }
                } else if pt(1).is_some() && el(0).is_some() {
                    Form::PE { swap: true }
                } else {
                    return need("two points, or a point and a line/circle/arc/ellipse");
                };
                if b.rows == 0 {
                    b.rows = 1;
                }
            }
            K::Horizontal | K::Vertical => {
                if n == 1 && lnx(0).is_some() {
                    b.form = Form::L1;
                } else if n == 2 && pt(0).is_some() && pt(1).is_some() {
                    b.form = Form::P2;
                } else {
                    return need("a line, an ellipse or two points");
                }
                b.rows = 1;
            }
            K::Parallel | K::Perpendicular | K::Collinear => {
                if !(n == 2 && ln(0).is_some() && ln(1).is_some()) {
                    return need("two lines");
                }
                b.form = Form::LL;
                b.rows = if c.kind == K::Collinear { 2 } else { 1 };
            }
            K::Concentric => {
                let center = |i: usize| ci(i).map(|(c, _)| c).or_else(|| el(i).map(|e| e.c)).or_else(|| pt(i));
                let curve = |i: usize| ci(i).is_some() || el(i).is_some();
                if !(n == 2 && center(0).is_some() && center(1).is_some() && (curve(0) || curve(1))) {
                    return need("two circles, arcs or ellipses");
                }
                b.form = Form::CC;
                b.rows = 2;
            }
            K::Equal => {
                if n != 2 {
                    return need("two objects");
                }
                b.form = if ln(0).is_some() && ln(1).is_some() {
                    Form::LL
                } else if ci(0).is_some() && ci(1).is_some() {
                    Form::CC
                } else {
                    return need("two lines or two circles/arcs");
                };
                b.rows = 1;
            }
            K::Tangent | K::Smooth => {
                if n != 2 {
                    return need("two objects");
                }
                if c.refs.iter().any(|r| m.is_spline(r)) {
                    return Self::spline_form(b, m, x);
                }
                b.rows = 1;
                if let (Some((a, bb)), Some((cc, _))) = (ln(0), ci(1)) {
                    b.form = Form::LC { swap: false };
                    b.sign = sgn(dir_n(a, bb).cross(cc - a));
                } else if let (Some((a, bb)), Some((cc, _))) = (ln(1), ci(0)) {
                    b.form = Form::LC { swap: true };
                    b.sign = sgn(dir_n(a, bb).cross(cc - a));
                } else if let (Some((c1, r1)), Some((c2, r2))) = (ci(0), ci(1)) {
                    b.form = Form::CC;
                    // sign > 0: external tangency; < 0: internal.
                    let d = c1.dist(c2);
                    b.sign = if d >= r1.abs().max(r2.abs()) { 1.0 } else { -1.0 };
                    b.target = sgn(r1 - r2);
                } else if let (Some((a, bb)), Some(e)) = (ln(0), el(1)) {
                    b.form = Form::LE { swap: false };
                    b.sign = sgn(dir_n(a, bb).cross(e.c - a));
                } else if let (Some((a, bb)), Some(e)) = (ln(1), el(0)) {
                    b.form = Form::LE { swap: true };
                    b.sign = sgn(dir_n(a, bb).cross(e.c - a));
                } else if (ci(0).is_some() || el(0).is_some()) && (ci(1).is_some() || el(1).is_some()) {
                    b.form = Form::Contact;
                    b.rows = 3;
                    b.aux_init = b.contact_guess(m, x).ok_or_else(|| format!("{} could not find where the curves touch", c.kind.name()))?;
                } else if ln(0).is_some() && ln(1).is_some() && c.kind == K::Smooth {
                    b.form = Form::LL;
                } else {
                    return need("a line and a circle/arc/ellipse, two curves, or a spline end and a curve");
                }
            }
            K::Symmetric => {
                if n != 3 || ln(2).is_none() {
                    return need("two objects and a symmetry line");
                }
                if pt(0).is_some() && pt(1).is_some() {
                    b.form = Form::PP;
                    b.rows = 2;
                } else if let (Some((a1, b1)), Some((a2, b2)), Some((la, lb))) = (ln(0), ln(1), ln(2)) {
                    let straight = a1.mirror(la, lb).dist(a2) + b1.mirror(la, lb).dist(b2);
                    let crossed = a1.mirror(la, lb).dist(b2) + b1.mirror(la, lb).dist(a2);
                    b.form = Form::SymLines { crossed: crossed < straight };
                    b.rows = 4;
                } else if ci(0).is_some() && ci(1).is_some() {
                    b.form = Form::CC;
                    b.rows = 3;
                } else {
                    return need("two points, lines or circles and a symmetry line");
                }
            }
            K::Fix => {
                if n != 1 {
                    return need("one reference");
                }
                let r = r_at(c, 0).ok_or_else(|| "Fix needs one reference".to_string())?;
                if m.lock_indices(r).is_some() {
                    b.form = Form::Lock;
                    b.rows = 0;
                } else if let Some(p) = pt(0) {
                    b.form = Form::Anchor(p);
                    b.rows = 2;
                } else {
                    return need("a point or an object");
                }
            }
            K::Distance(axis) => {
                b.rows = 1;
                if n == 1 && ln(0).is_some() {
                    b.form = Form::L1;
                } else if n == 2 && pt(0).is_some() && pt(1).is_some() {
                    b.form = Form::P2;
                } else if n == 2 && axis == DistAxis::Aligned && pt(0).is_some() && ln(1).is_some() {
                    b.form = Form::PL { swap: false };
                } else if n == 2 && axis == DistAxis::Aligned && pt(1).is_some() && ln(0).is_some() {
                    b.form = Form::PL { swap: true };
                } else {
                    return need("a line or two points");
                }
                if axis != DistAxis::Aligned || matches!(b.form, Form::PL { .. }) {
                    // Keep the current orientation: measure the signed value.
                    b.sign = 1.0;
                    let mut out = Vec::new();
                    b.target = 0.0;
                    b.eval(m, x, &mut out);
                    b.sign = sgn(out.first().copied().unwrap_or(0.0));
                    b.target = target;
                }
                if !(target.is_finite() && target >= 0.0) {
                    return Err(format!("{} value must be non-negative", c.kind.name()));
                }
            }
            K::Angular => {
                b.rows = 1;
                if let (2, Some((a1, b1)), Some((a2, b2))) = (n, ln(0), ln(1)) {
                    // Start-to-end directions would measure the supplement when a line is drawn
                    // towards the vertex, so use the rays the dimension draws.
                    let rev = match cadcraft_geom::line_line_infinite(a1, b1, a2, b2) {
                        Some((_, t, u)) => [t > 0.5, u > 0.5],
                        None => [false, false],
                    };
                    b.form = Form::Rays { rev };
                    let (d1, d2) = (ray(a1, b1, rev[0]), ray(a2, b2, rev[1]));
                    b.sign = sgn(d1.cross(d2).atan2(d1.dot(d2)));
                } else if n == 1 && m.arc_sweep(x, r_at(c, 0).ok_or_else(|| "Angular needs references".to_string())?).is_some() {
                    b.form = Form::ArcSweep;
                } else {
                    return need("two lines or an arc");
                }
                b.target = target.to_radians();
                // Two lines measure a signed angle in [-180, 180] and the sign is re-read from the geometry, so a larger
                // target would flip the sign once solved and never be satisfied; only an arc sweep can reach a full turn.
                let max = if matches!(b.form, Form::Rays { .. }) { std::f64::consts::PI } else { std::f64::consts::TAU };
                if !b.target.is_finite() || b.target.abs() > max {
                    return Err("angle out of range".into());
                }
            }
            K::Radius | K::Diameter => {
                if !(n == 1 && ci(0).is_some()) {
                    return need("a circle or arc");
                }
                b.form = Form::C1;
                b.rows = 1;
                if !target.is_finite() || crate::curves::not_above(target, 0.0) {
                    return Err(format!("{} value must be positive", c.kind.name()));
                }
            }
        }
        Ok(b)
    }

    /// Append this constraint's residuals (exactly `rows` values).
    pub fn eval(&self, m: &Model, x: &[f64], out: &mut Vec<f64>) {
        let start = out.len();
        self.eval_inner(m, x, out);
        out.resize(start + self.rows, 0.0);
    }

    fn eval_inner(&self, m: &Model, x: &[f64], out: &mut Vec<f64>) -> Option<()> {
        use ConstraintKind as K;
        let r = |i: usize| self.refs.get(i);
        let pt = |i: usize| m.point(x, r(i)?);
        let ln = |i: usize| m.line(x, r(i)?);
        let ci = |i: usize| m.circle(x, r(i)?);
        let el = |i: usize| m.ellipse(x, r(i)?);
        match (self.kind, self.form) {
            (_, Form::Lock) => {}
            (_, Form::PE { swap }) => {
                let (p, e) = if swap { (pt(1)?, el(0)?) } else { (pt(0)?, el(1)?) };
                out.push(e.on(p));
            }
            (_, Form::LE { swap }) => {
                let ((a, b), e) = if swap { (ln(1)?, el(0)?) } else { (ln(0)?, el(1)?) };
                let d = dir_n(a, b);
                out.push(self.sign * d.cross(e.c - a) - e.support(d.perp()));
            }
            (_, Form::Contact) => {
                let (p0, d0) = self.curve_at(m, x, 0, *x.get(self.aux)?)?;
                let (p1, d1) = self.curve_at(m, x, 1, *x.get(self.aux + 1)?)?;
                out.extend([p0.x - p1.x, p0.y - p1.y, d0.normalized().cross(d1.normalized())]);
            }
            (_, Form::EndOn { spline, at_end }) => {
                let f = m.end_frame(x, r(spline)?, at_end)?;
                let o = 1 - spline.min(1);
                if let Some((a, b)) = ln(o) {
                    let d = dir_n(a, b);
                    out.extend([d.cross(f.p - a), f.t.cross(d)]);
                } else if let Some((c, rad)) = ci(o) {
                    out.extend([f.p.dist(c) - rad, f.t.dot(dir_n(c, f.p))]);
                } else {
                    let e = el(o)?;
                    out.extend([e.on(f.p), f.t.dot(e.normal(f.p))]);
                }
            }
            (_, Form::Join { a_end, b_end, g2 }) => {
                // Orient both so the first curve runs into the joint and the second out of it.
                let fa = m.end_frame(x, r(0)?, a_end)?;
                let fb = m.end_frame(x, r(1)?, b_end)?;
                let fa = if a_end { fa } else { fa.reversed() };
                let fb = if b_end { fb.reversed() } else { fb };
                out.extend([fa.p.x - fb.p.x, fa.p.y - fb.p.y, fa.t.cross(fb.t)]);
                if g2 {
                    out.push(fa.k - fb.k);
                }
            }
            (_, Form::Anchor(a)) => {
                let p = pt(0)?;
                out.extend([p.x - a.x, p.y - a.y]);
            }
            (K::Coincident, Form::PP) => {
                let (p, q) = (pt(0)?, pt(1)?);
                out.extend([p.x - q.x, p.y - q.y]);
            }
            (K::Coincident, Form::PL { swap }) => {
                let (p, (a, b)) = if swap { (pt(1)?, ln(0)?) } else { (pt(0)?, ln(1)?) };
                out.push(dir_n(a, b).cross(p - a));
            }
            (K::Coincident, Form::PC { swap }) => {
                let (p, (c, rad)) = if swap { (pt(1)?, ci(0)?) } else { (pt(0)?, ci(1)?) };
                out.push(p.dist(c) - rad);
            }
            (K::Horizontal | K::Vertical, f) => {
                let (a, b) = if f == Form::L1 { ln(0).or_else(|| el(0).map(|e| (e.c - e.mj, e.c + e.mj)))? } else { (pt(0)?, pt(1)?) };
                out.push(if self.kind == K::Horizontal { a.y - b.y } else { a.x - b.x });
            }
            (K::Parallel | K::Smooth, Form::LL) => {
                let ((a1, b1), (a2, b2)) = (ln(0)?, ln(1)?);
                out.push(dir_n(a1, b1).cross(dir_n(a2, b2)));
            }
            (K::Perpendicular, _) => {
                let ((a1, b1), (a2, b2)) = (ln(0)?, ln(1)?);
                out.push(dir_n(a1, b1).dot(dir_n(a2, b2)));
            }
            (K::Collinear, _) => {
                let ((a1, b1), (a2, b2)) = (ln(0)?, ln(1)?);
                let d = dir_n(a1, b1);
                out.extend([d.cross(a2 - a1), d.cross(b2 - a1)]);
            }
            (K::Concentric, _) => {
                let center = |i: usize| ci(i).map(|(c, _)| c).or_else(|| el(i).map(|e| e.c)).or_else(|| pt(i));
                let (p, q) = (center(0)?, center(1)?);
                out.extend([p.x - q.x, p.y - q.y]);
            }
            (K::Equal, Form::LL) => {
                let ((a1, b1), (a2, b2)) = (ln(0)?, ln(1)?);
                out.push(a1.dist(b1) - a2.dist(b2));
            }
            (K::Equal, _) => {
                let ((_, r1), (_, r2)) = (ci(0)?, ci(1)?);
                out.push(r1 - r2);
            }
            (K::Tangent | K::Smooth, Form::LC { swap }) => {
                let ((a, b), (c, rad)) = if swap { (ln(1)?, ci(0)?) } else { (ln(0)?, ci(1)?) };
                out.push(self.sign * dir_n(a, b).cross(c - a) - rad);
            }
            (K::Tangent | K::Smooth, _) => {
                let ((c1, r1), (c2, r2)) = (ci(0)?, ci(1)?);
                let d = c1.dist(c2);
                out.push(if self.sign > 0.0 { d - (r1 + r2) } else { d - self.target * (r1 - r2) });
            }
            (K::Symmetric, Form::PP) => {
                let (p, q, (a, b)) = (pt(0)?, pt(1)?, ln(2)?);
                let d = dir_n(a, b);
                out.extend([d.cross(p.mid(q) - a), d.dot(q - p)]);
            }
            (K::Symmetric, Form::SymLines { crossed }) => {
                let ((a1, b1), (a2, b2), (a, b)) = (ln(0)?, ln(1)?, ln(2)?);
                let (a2, b2) = if crossed { (b2, a2) } else { (a2, b2) };
                let d = dir_n(a, b);
                out.extend([d.cross(a1.mid(a2) - a), d.dot(a2 - a1), d.cross(b1.mid(b2) - a), d.dot(b2 - b1)]);
            }
            (K::Symmetric, _) => {
                let ((p, r1), (q, r2), (a, b)) = (ci(0)?, ci(1)?, ln(2)?);
                let d = dir_n(a, b);
                out.extend([d.cross(p.mid(q) - a), d.dot(q - p), r1 - r2]);
            }
            (K::Distance(axis), f) => {
                let v = match (axis, f) {
                    (_, Form::PL { swap }) => {
                        let (p, (a, b)) = if swap { (pt(1)?, ln(0)?) } else { (pt(0)?, ln(1)?) };
                        dir_n(a, b).cross(p - a)
                    }
                    _ => {
                        let (a, b) = if f == Form::L1 { ln(0)? } else { (pt(0)?, pt(1)?) };
                        match axis {
                            DistAxis::Aligned => a.dist(b),
                            DistAxis::Horizontal => b.x - a.x,
                            DistAxis::Vertical => b.y - a.y,
                        }
                    }
                };
                out.push(self.sign * v - self.target);
            }
            (K::Angular, Form::ArcSweep) => {
                out.push(m.arc_sweep(x, r(0)?)? - self.target);
            }
            (K::Angular, Form::Rays { rev }) => {
                let ((a1, b1), (a2, b2)) = (ln(0)?, ln(1)?);
                let (d1, d2) = (ray(a1, b1, rev[0]), ray(a2, b2, rev[1]));
                out.push(wrap(d1.cross(d2).atan2(d1.dot(d2)) - self.sign * self.target));
            }
            (K::Radius, _) => out.push(ci(0)?.1 - self.target),
            (K::Diameter, _) => out.push(2.0 * ci(0)?.1 - self.target),
            _ => {}
        }
        Some(())
    }

    /// Point and derivative of curve reference `i` (circle, arc or ellipse) at parameter `t`.
    fn curve_at(&self, m: &Model, x: &[f64], i: usize, t: f64) -> Option<(Vec2, Vec2)> {
        let r = self.refs.get(i)?;
        if let Some((c, rad)) = m.circle(x, r) {
            return Some(circle_at(c, rad, t));
        }
        let e = m.ellipse(x, r)?;
        Some((e.at(t), e.d1(t)))
    }

    /// Contact parameters where two curves come closest (sampled), for [`Form::Contact`].
    fn contact_guess(&self, m: &Model, x: &[f64]) -> Option<Vec<f64>> {
        let step = std::f64::consts::TAU / CONTACT_SAMPLES as f64;
        let s0: Vec<(f64, Vec2)> = (0..CONTACT_SAMPLES).filter_map(|k| Some((k as f64 * step, self.curve_at(m, x, 0, k as f64 * step)?.0))).collect();
        let s1: Vec<(f64, Vec2)> = (0..CONTACT_SAMPLES).filter_map(|k| Some((k as f64 * step, self.curve_at(m, x, 1, k as f64 * step)?.0))).collect();
        let mut best: Option<(f64, f64, f64)> = None;
        for (t0, p0) in &s0 {
            for (t1, p1) in &s1 {
                let d = p0.dist(*p1);
                if d.is_finite() && best.is_none_or(|b| d < b.0) {
                    best = Some((d, *t0, *t1));
                }
            }
        }
        best.map(|(_, a, b)| vec![a, b])
    }

    /// Tangent or Smooth with a spline: the spline end and the other curve's nearest end are
    /// joined (Smooth: G2; Tangent between splines: G1), or the spline's end touches a line,
    /// circle, arc or ellipse tangentially. A `start`/`end` reference picks the end.
    fn spline_form(mut b: Built, m: &Model, x: &[f64]) -> Result<Built, String> {
        let (Some(r0), Some(r1)) = (b.refs.first().copied(), b.refs.get(1).copied()) else {
            return Err(format!("{} needs two objects", b.kind.name()));
        };
        let ends = |r: &GeomRef| -> &'static [bool] {
            match r.sub {
                cadcraft_doc::Sub::Start => &[false],
                cadcraft_doc::Sub::End => &[true],
                _ => &[false, true],
            }
        };
        let join = b.kind == ConstraintKind::Smooth || (m.is_spline(&r0) && m.is_spline(&r1));
        if join {
            let mut best: Option<(f64, bool, bool)> = None;
            for &ea in ends(&r0) {
                for &eb in ends(&r1) {
                    if let (Some(fa), Some(fb)) = (m.end_frame(x, &r0, ea), m.end_frame(x, &r1, eb)) {
                        let d = fa.p.dist(fb.p);
                        if d.is_finite() && best.is_none_or(|bb| d < bb.0) {
                            best = Some((d, ea, eb));
                        }
                    }
                }
            }
            let Some((_, a_end, b_end)) = best else {
                return Err(format!("{} needs an open spline and a line, arc, elliptical arc, polyline segment or open spline", b.kind.name()));
            };
            let g2 = b.kind == ConstraintKind::Smooth;
            b.form = Form::Join { a_end, b_end, g2 };
            b.rows = if g2 { 4 } else { 3 };
            return Ok(b);
        }
        let (si, rs, ro) = if m.is_spline(&r0) { (0, r0, r1) } else { (1, r1, r0) };
        let dist = |p: Vec2| -> Option<f64> {
            if let Some((a, bb)) = m.line(x, &ro) {
                Some(dir_n(a, bb).cross(p - a).abs())
            } else if let Some((c, rad)) = m.circle(x, &ro) {
                Some((p.dist(c) - rad).abs())
            } else {
                m.ellipse(x, &ro).map(|e| e.on(p).abs())
            }
        };
        let mut best: Option<(f64, bool)> = None;
        for &e in ends(&rs) {
            if let Some(d) = m.end_frame(x, &rs, e).and_then(|f| dist(f.p))
                && d.is_finite()
                && best.is_none_or(|bb| d < bb.0)
            {
                best = Some((d, e));
            }
        }
        let Some((_, at_end)) = best else {
            return Err("Tangent needs an open spline and a line, circle, arc, ellipse or spline".into());
        };
        b.form = Form::EndOn { spline: si, at_end };
        b.rows = 2;
        Ok(b)
    }

    /// The current measured value (degrees for angular), for dimensional constraints.
    pub fn measure(&self, m: &Model, x: &[f64]) -> f64 {
        let mut b = self.clone();
        b.target = 0.0;
        let mut out = Vec::new();
        b.eval(m, x, &mut out);
        let v = out.first().copied().unwrap_or(0.0).abs();
        if self.kind == ConstraintKind::Angular { v.to_degrees() } else { v }
    }
}
