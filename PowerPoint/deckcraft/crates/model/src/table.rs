//! Tables (graphic frames with `a:tbl`).

use serde::{Deserialize, Serialize};

use crate::style::{ColorRef, Fill, Line};
use crate::text::{Anchor, TextBody};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cell {
    pub text: TextBody,
    pub fill: Option<Fill>,
    /// Left, right, top, bottom borders; `None` = from the table style.
    pub borders: [Option<Line>; 4],
    pub diag_down: Option<Line>,
    pub diag_up: Option<Line>,
    /// Margins (l, r, t, b) in points; `None` = defaults (7.2, 7.2, 3.6, 3.6).
    pub margins: Option<[f64; 4]>,
    pub anchor: Option<Anchor>,
    pub grid_span: u32,
    pub row_span: u32,
    /// Covered by a merge to the left / above.
    pub h_merge: bool,
    pub v_merge: bool,
    pub vertical: Option<crate::text::TextDir>,
}

impl Cell {
    pub fn new(text: &str) -> Self {
        Cell { text: TextBody::from_text(text), grid_span: 1, row_span: 1, ..Default::default() }
    }
    pub fn is_covered(&self) -> bool {
        self.h_merge || self.v_merge
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Row {
    pub height: f64,
    pub cells: Vec<Cell>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Table {
    /// Column widths in points.
    pub cols: Vec<f64>,
    pub rows: Vec<Row>,
    /// Table style id (our built-in style names or a PPTX style GUID).
    pub style: String,
    pub first_row: bool,
    pub first_col: bool,
    pub last_row: bool,
    pub last_col: bool,
    pub band_row: bool,
    pub band_col: bool,
    /// `a:tblPr` fill: a fill behind the whole table (over the style's own background).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<Fill>,
}

impl Default for Table {
    fn default() -> Self {
        Table {
            cols: vec![],
            rows: vec![],
            style: "medium2-accent1".into(),
            first_row: true,
            first_col: false,
            last_row: false,
            last_col: false,
            band_row: true,
            band_col: false,
            background: None,
        }
    }
}

impl Table {
    pub fn new(rows: usize, cols: usize, width: f64, row_height: f64) -> Self {
        let rows = rows.clamp(1, 1000);
        let cols = cols.clamp(1, 1000);
        let cw = if cols > 0 { width / cols as f64 } else { width };
        Table {
            cols: vec![cw; cols],
            rows: (0..rows).map(|_| Row { height: row_height, cells: (0..cols).map(|_| Cell::new("")).collect() }).collect(),
            ..Default::default()
        }
    }
    pub fn n_rows(&self) -> usize {
        self.rows.len()
    }
    pub fn n_cols(&self) -> usize {
        self.cols.len()
    }
    pub fn cell(&self, r: usize, c: usize) -> Option<&Cell> {
        self.rows.get(r).and_then(|row| row.cells.get(c))
    }
    pub fn cell_mut(&mut self, r: usize, c: usize) -> Option<&mut Cell> {
        self.rows.get_mut(r).and_then(|row| row.cells.get_mut(c))
    }
    pub fn width(&self) -> f64 {
        self.cols.iter().sum()
    }
    pub fn height(&self) -> f64 {
        self.rows.iter().map(|r| r.height).sum()
    }
    pub fn insert_row(&mut self, at: usize) {
        let at = at.min(self.rows.len());
        let h = self.rows.get(at.saturating_sub(1)).map(|r| r.height).unwrap_or(30.0);
        self.rows.insert(at, Row { height: h, cells: (0..self.cols.len()).map(|_| Cell::new("")).collect() });
    }
    pub fn insert_col(&mut self, at: usize) {
        let at = at.min(self.cols.len());
        let w = self.cols.get(at.saturating_sub(1)).copied().unwrap_or(80.0);
        self.cols.insert(at, w);
        for r in &mut self.rows {
            let i = at.min(r.cells.len());
            r.cells.insert(i, Cell::new(""));
        }
    }
    pub fn delete_row(&mut self, at: usize) -> bool {
        if self.rows.len() <= 1 || at >= self.rows.len() {
            return false;
        }
        self.rows.remove(at);
        true
    }
    pub fn delete_col(&mut self, at: usize) -> bool {
        if self.cols.len() <= 1 || at >= self.cols.len() {
            return false;
        }
        self.cols.remove(at);
        for r in &mut self.rows {
            if at < r.cells.len() {
                r.cells.remove(at);
            }
        }
        true
    }
    /// Merge the rectangle (r0,c0)–(r1,c1) inclusive into its top-left cell.
    pub fn merge(&mut self, r0: usize, c0: usize, r1: usize, c1: usize) -> bool {
        let (r0, r1) = (r0.min(r1), r0.max(r1));
        let (c0, c1) = (c0.min(c1), c0.max(c1));
        if r1 >= self.rows.len() || c1 >= self.cols.len() || (r0 == r1 && c0 == c1) {
            return false;
        }
        let mut text = vec![];
        for r in r0..=r1 {
            for c in c0..=c1 {
                if let Some(cell) = self.cell_mut(r, c) {
                    if !cell.text.is_empty() {
                        text.append(&mut cell.text.paragraphs);
                    }
                    cell.h_merge = c > c0;
                    cell.v_merge = r > r0;
                    cell.grid_span = 1;
                    cell.row_span = 1;
                }
            }
        }
        if let Some(cell) = self.cell_mut(r0, c0) {
            cell.grid_span = (c1 - c0 + 1) as u32;
            cell.row_span = (r1 - r0 + 1) as u32;
            cell.h_merge = false;
            cell.v_merge = false;
            if !text.is_empty() {
                cell.text.paragraphs = text;
            }
        }
        true
    }
    /// Undo a merge at (r, c).
    pub fn split(&mut self, r: usize, c: usize) -> bool {
        let Some(cell) = self.cell(r, c) else { return false };
        let (gs, rs) = (cell.grid_span.max(1) as usize, cell.row_span.max(1) as usize);
        if gs == 1 && rs == 1 {
            return false;
        }
        for rr in r..r + rs {
            for cc in c..c + gs {
                if let Some(x) = self.cell_mut(rr, cc) {
                    x.h_merge = false;
                    x.v_merge = false;
                    x.grid_span = 1;
                    x.row_span = 1;
                }
            }
        }
        true
    }
    /// Make every row have exactly `cols.len()` cells (repair after loading).
    pub fn normalize(&mut self) {
        let n = self.cols.len();
        for r in &mut self.rows {
            r.cells.resize_with(n, || Cell::new(""));
            for c in &mut r.cells {
                c.grid_span = c.grid_span.max(1);
                c.row_span = c.row_span.max(1);
            }
        }
    }
}

/// A table style defined in the file (`ppt/tableStyles.xml`), as PowerPoint writes the built-in
/// styles a deck uses: how each part of a table (whole table, banded rows and columns, first and
/// last rows and columns, corner cells) is filled, bordered and lettered.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TableStyleDef {
    /// The style's GUID (`a:tableStyleId` on tables).
    pub id: String,
    pub name: String,
    pub parts: Vec<(TablePart, TablePartStyle)>,
    /// `a:tblBg`: a fill behind the whole table (only `fill` and `fill_ref` are used).
    pub background: Option<TablePartStyle>,
    /// The definition as read, written back unchanged.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub raw: String,
}

impl TableStyleDef {
    pub fn part(&self, p: TablePart) -> Option<&TablePartStyle> {
        self.parts.iter().find(|(k, _)| *k == p).map(|(_, s)| s)
    }
}

/// The parts of a table style, in the order they apply (later ones win).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TablePart {
    WholeTbl,
    Band1H,
    Band2H,
    Band1V,
    Band2V,
    LastCol,
    FirstCol,
    LastRow,
    FirstRow,
    SeCell,
    SwCell,
    NeCell,
    NwCell,
}

impl TablePart {
    pub const ALL: [TablePart; 13] = [
        TablePart::WholeTbl,
        TablePart::Band1H,
        TablePart::Band2H,
        TablePart::Band1V,
        TablePart::Band2V,
        TablePart::LastCol,
        TablePart::FirstCol,
        TablePart::LastRow,
        TablePart::FirstRow,
        TablePart::SeCell,
        TablePart::SwCell,
        TablePart::NeCell,
        TablePart::NwCell,
    ];

    pub fn xml(self) -> &'static str {
        match self {
            TablePart::WholeTbl => "wholeTbl",
            TablePart::Band1H => "band1H",
            TablePart::Band2H => "band2H",
            TablePart::Band1V => "band1V",
            TablePart::Band2V => "band2V",
            TablePart::LastCol => "lastCol",
            TablePart::FirstCol => "firstCol",
            TablePart::LastRow => "lastRow",
            TablePart::FirstRow => "firstRow",
            TablePart::SeCell => "seCell",
            TablePart::SwCell => "swCell",
            TablePart::NeCell => "neCell",
            TablePart::NwCell => "nwCell",
        }
    }

    pub fn from_xml(s: &str) -> Option<TablePart> {
        TablePart::ALL.into_iter().find(|p| p.xml() == s)
    }
}

/// One part of a table style. A theme reference (`a:fillRef`, `a:lnRef`) is kept as the theme's
/// style index with its colour.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TablePartStyle {
    pub fill: Option<Fill>,
    pub fill_ref: Option<(u32, ColorRef)>,
    /// Left, right, top, bottom, inside horizontal, inside vertical.
    pub borders: [Option<TableBorder>; 6],
    pub text_color: Option<ColorRef>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
}

/// A border of a table style part: a line, or a theme line style reference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TableBorder {
    Line { line: Line },
    Ref { idx: u32, color: ColorRef },
}

/// Built-in table styles: (id, label). Styles are generated from the theme accents.
pub fn table_styles() -> Vec<(String, String)> {
    let mut v = vec![("none".to_string(), "No Style, No Grid".to_string()), ("grid".to_string(), "No Style, Table Grid".to_string())];
    for (kind, label) in [
        ("light1", "Light Style 1"),
        ("light2", "Light Style 2"),
        ("medium2", "Medium Style 2"),
        ("medium4", "Medium Style 4"),
        ("dark1", "Dark Style 1"),
    ] {
        v.push((format!("{kind}-tx1"), label.to_string()));
        for a in 1..=6 {
            v.push((format!("{kind}-accent{a}"), format!("{label} - Accent {a}")));
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_delete_merge_split() {
        let mut t = Table::new(3, 3, 300.0, 30.0);
        assert_eq!((t.n_rows(), t.n_cols()), (3, 3));
        t.insert_row(1);
        t.insert_col(3);
        assert_eq!((t.n_rows(), t.n_cols()), (4, 4));
        assert!(t.delete_row(0) && t.delete_col(0));
        assert_eq!((t.n_rows(), t.n_cols()), (3, 3));
        assert!(t.merge(0, 0, 1, 1));
        assert_eq!(t.cell(0, 0).map(|c| (c.grid_span, c.row_span)), Some((2, 2)));
        assert!(t.cell(1, 1).is_some_and(|c| c.is_covered()));
        assert!(t.split(0, 0));
        assert!(!t.cell(1, 1).is_some_and(|c| c.is_covered()));
        assert!(!t.merge(5, 5, 6, 6));
        assert!(!t.delete_row(99));
        let mut one = Table::new(1, 1, 10.0, 10.0);
        assert!(!one.delete_row(0) && !one.delete_col(0));
        assert!(table_styles().len() > 30);
    }
}
