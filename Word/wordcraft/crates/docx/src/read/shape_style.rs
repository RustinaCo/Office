//! How a Word shape (`wps:wsp`) looks: its preset geometry, and the fill and outline it gets from its own properties (`wps:spPr`) or, failing those, from the theme through its
//! style references (`wps:style`: `a:fillRef`, `a:lnRef`, ECMA-376 Part 1 §20.1.4.2).

use wordcraft_doc::para::ShapeKind;
use wordcraft_doc::props::Rgb;

use super::drawing_color::{color, fill_colour};
use crate::units::{int, measure};
use crate::xml::El;

/// Outline widths of the theme's three line styles (`a:lnStyleLst`), points: what Word's
/// default themes give (6 350, 12 700 and 19 050 EMU).
const THEME_LINE_WIDTHS: [f32; 3] = [0.5, 1.0, 1.5];

/// The shape kind drawn for DrawingML preset geometry `prst` (ECMA-376 Part 1 §20.1.10.56).
/// Presets WordCraft has no shape for are drawn as the nearest one it has: callouts as the box,
/// round or cloud they point from, flowchart boxes as rectangles.
pub(crate) fn preset_kind(prst: &str) -> ShapeKind {
    match prst {
        "roundRect"
        | "round1Rect"
        | "round2SameRect"
        | "round2DiagRect"
        | "snipRoundRect"
        | "wedgeRoundRectCallout"
        | "flowChartAlternateProcess"
        | "flowChartTerminator"
        | "plaque" => ShapeKind::RoundedRectangle,
        "ellipse" | "wedgeEllipseCallout" | "cloudCallout" | "cloud" | "flowChartConnector" | "donut" | "flowChartOnlineStorage" => {
            ShapeKind::Ellipse
        }
        "triangle" | "rtTriangle" | "flowChartExtract" | "flowChartMerge" => ShapeKind::Triangle,
        "diamond" | "flowChartDecision" => ShapeKind::Diamond,
        "line" | "lineInv" => ShapeKind::Line,
        "straightConnector1" => ShapeKind::StraightConnector,
        "bentConnector2" | "bentConnector3" | "bentConnector4" | "bentConnector5" => ShapeKind::ElbowConnector,
        "curvedConnector2" | "curvedConnector3" | "curvedConnector4" | "curvedConnector5" => ShapeKind::CurvedConnector,
        "rightArrow" | "leftArrow" | "upArrow" | "downArrow" | "leftRightArrow" | "upDownArrow" | "notchedRightArrow" | "stripedRightArrow" => {
            ShapeKind::Arrow
        }
        "star4" | "star5" | "star6" | "star7" | "star8" | "star10" | "star12" | "star16" | "star24" | "star32" => ShapeKind::Star,
        "heart" => ShapeKind::Heart,
        _ => ShapeKind::Rectangle,
    }
}

/// A shape's fill and outline.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ShapePaint {
    pub fill: Option<Rgb>,
    pub stroke: Option<Rgb>,
    /// Points.
    pub stroke_width: f32,
}

/// The paint of a shape from its properties `sppr`, falling back on its style `style` (theme
/// colours `theme`) for what they leave out: an explicit `a:noFill` is no fill or outline, an
/// explicit colour wins, and a style reference with index 0 is none.
pub(crate) fn shape_paint(sppr: Option<&El>, style: Option<&El>, theme: &[Rgb]) -> ShapePaint {
    let style_ref = |name: &str| -> Option<(i64, &El)> {
        let r = style?.child(name)?;
        Some((r.attr("idx").and_then(int).unwrap_or(0), r))
    };
    let fill = match sppr {
        Some(s) if s.child("a:noFill").is_some() || s.child("a:blipFill").is_some() => None,
        Some(s) if s.els().any(|c| matches!(c.name.as_str(), "a:solidFill" | "a:gradFill" | "a:pattFill")) => fill_colour(s, theme),
        _ => style_ref("a:fillRef").filter(|(idx, _)| *idx > 0).and_then(|(_, r)| color(r, theme)),
    };
    let ln = sppr.and_then(|s| s.child("a:ln"));
    let line_ref = style_ref("a:lnRef").filter(|(idx, _)| *idx > 0);
    let stroke = match ln {
        Some(l) if l.child("a:noFill").is_some() => None,
        Some(l) if l.els().any(|c| matches!(c.name.as_str(), "a:solidFill" | "a:gradFill" | "a:pattFill")) => fill_colour(l, theme),
        _ => line_ref.and_then(|(_, r)| color(r, theme)),
    };
    let themed_width = line_ref.and_then(|(idx, _)| THEME_LINE_WIDTHS.get(usize::try_from(idx.clamp(1, 3) - 1).unwrap_or(0)).copied());
    let stroke_width = ln
        .and_then(|l| l.attr("w"))
        .and_then(|v| measure(v, 12_700.0))
        .or(themed_width)
        // Word's default outline when the file names none (no outline: no width).
        .unwrap_or(if stroke.is_some() { 0.75 } else { 0.0 })
        .clamp(0.0, 100.0);
    ShapePaint { fill, stroke, stroke_width }
}

/// A VML colour (`strokecolor`, `fillcolor`): `#rrggbb` or `#rgb` (Word follows it with the
/// theme slot, as in `#4472c4 [3204]`), or a colour name.
pub(crate) fn vml_color(v: &str) -> Option<Rgb> {
    let v = v.split_whitespace().next()?;
    if let Some(hex) = v.strip_prefix('#') {
        if hex.len() == 3 && hex.is_ascii() {
            let long: String = hex.chars().flat_map(|c| [c, c]).collect();
            return Rgb::parse(&long);
        }
        return Rgb::parse(hex);
    }
    super::drawing_color::preset_color(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wordcraft_doc::THEME_COLORS;

    fn el(xml: &str) -> El {
        let ns = r#"xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#;
        crate::xml::parse(xml.replace(r#"xmlns:wps="w" xmlns:a="a""#, ns).as_bytes()).unwrap()
    }

    #[test]
    fn theme_styled_connector_gets_its_line_from_the_style() {
        // A connector with no colour of its own (what Word writes for Insert › Shapes › Line):
        // its outline comes from `a:lnRef`, its width from the theme's first line style.
        let sp = el(r#"<wps:spPr xmlns:wps="w" xmlns:a="a"><a:prstGeom prst="line"/><a:ln><a:tailEnd type="triangle"/></a:ln></wps:spPr>"#);
        let st = el(
            r#"<wps:style xmlns:wps="w" xmlns:a="a"><a:lnRef idx="1"><a:schemeClr val="accent4"/></a:lnRef><a:fillRef idx="0"><a:schemeClr val="accent4"/></a:fillRef></wps:style>"#,
        );
        let p = shape_paint(Some(&sp), Some(&st), &THEME_COLORS);
        assert_eq!(p.stroke, Some(THEME_COLORS[7]));
        assert_eq!(p.stroke_width, 0.5);
        assert_eq!(p.fill, None);
    }

    #[test]
    fn explicit_paint_wins_over_the_style() {
        let sp = el(r#"<wps:spPr xmlns:wps="w" xmlns:a="a"><a:noFill/><a:ln><a:solidFill><a:srgbClr val="EE0000"/></a:solidFill></a:ln></wps:spPr>"#);
        let st = el(
            r#"<wps:style xmlns:wps="w" xmlns:a="a"><a:lnRef idx="2"><a:schemeClr val="accent1"><a:shade val="15000"/></a:schemeClr></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef></wps:style>"#,
        );
        let p = shape_paint(Some(&sp), Some(&st), &THEME_COLORS);
        assert_eq!((p.fill, p.stroke, p.stroke_width), (None, Some(Rgb(0xEE, 0, 0)), 1.0));
        // Preset and scheme colours as the box's own fill and outline.
        let sp = el(
            r#"<wps:spPr xmlns:wps="w" xmlns:a="a"><a:solidFill><a:schemeClr val="lt1"/></a:solidFill><a:ln w="6350"><a:solidFill><a:prstClr val="black"/></a:solidFill></a:ln></wps:spPr>"#,
        );
        let p = shape_paint(Some(&sp), None, &THEME_COLORS);
        assert_eq!((p.fill, p.stroke, p.stroke_width), (Some(THEME_COLORS[1]), Some(Rgb(0, 0, 0)), 0.5));
        // A style fill with index 0 is none; an outline set to no fill is none.
        let sp = el(r#"<wps:spPr xmlns:wps="w" xmlns:a="a"><a:ln><a:noFill/></a:ln></wps:spPr>"#);
        let st = el(
            r#"<wps:style xmlns:wps="w" xmlns:a="a"><a:lnRef idx="2"><a:schemeClr val="accent1"/></a:lnRef><a:fillRef idx="0"><a:schemeClr val="accent1"/></a:fillRef></wps:style>"#,
        );
        let p = shape_paint(Some(&sp), Some(&st), &THEME_COLORS);
        assert_eq!((p.fill, p.stroke), (None, None));
    }

    #[test]
    fn hostile_style_references_do_not_panic() {
        let st = el(
            r#"<wps:style xmlns:wps="w" xmlns:a="a"><a:lnRef idx="-99999999999"><a:schemeClr val="nope"/></a:lnRef><a:fillRef idx="99999999999"/></wps:style>"#,
        );
        let p = shape_paint(None, Some(&st), &[]);
        assert_eq!((p.fill, p.stroke), (None, None));
        let st = el(r#"<wps:style xmlns:wps="w" xmlns:a="a"><a:lnRef idx="77"><a:srgbClr val="010203"/></a:lnRef></wps:style>"#);
        assert_eq!(shape_paint(None, Some(&st), &[]).stroke_width, 1.5);
    }

    #[test]
    fn presets_map_to_the_nearest_shape() {
        assert_eq!(preset_kind("straightConnector1"), ShapeKind::StraightConnector);
        assert_eq!(preset_kind("bentConnector3"), ShapeKind::ElbowConnector);
        assert_eq!(preset_kind("wedgeRoundRectCallout"), ShapeKind::RoundedRectangle);
        assert_eq!(preset_kind("wedgeEllipseCallout"), ShapeKind::Ellipse);
        assert_eq!(preset_kind("wedgeRectCallout"), ShapeKind::Rectangle);
        assert_eq!(preset_kind("flowChartDecision"), ShapeKind::Diamond);
    }

    #[test]
    fn vml_colours() {
        assert_eq!(vml_color("#ffc000 [3207]"), Some(Rgb(0xFF, 0xC0, 0)));
        assert_eq!(vml_color("#f00"), Some(Rgb(0xFF, 0, 0)));
        assert_eq!(vml_color("black"), Some(Rgb(0, 0, 0)));
        assert_eq!(vml_color("#é"), None);
        assert_eq!(vml_color(""), None);
    }
}
