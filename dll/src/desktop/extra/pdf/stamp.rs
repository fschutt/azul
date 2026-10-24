//! A stamp's drawing (a signature) - an SVG of `<path>`s - as the paths
//! printpdf draws onto a page (`printpdf::forms::FormStamp`).
//!
//! Read: the `<svg>`'s `viewBox` (else its `width` / `height`) and every
//! `<path>` in it (in groups too; a group's `transform` is not applied) with
//! its `d`, `stroke`, `stroke-width` and `fill` (SVG's default fill is black;
//! `none` paints nothing). That is what AzPdf's signature pad writes.

use azul_core::xml::{XmlNode, XmlNodeChild};
use printpdf::forms::{StampPath, StampSegment};

/// The view box (`[x, y, width, height]`) and the paths of the SVG `svg`;
/// `None` when it is no SVG or draws nothing.
pub(super) fn svg_paths(svg: &str) -> Option<([f32; 4], Vec<StampPath>)> {
    let xml = azul_layout::xml::parse_xml(svg).ok()?;
    let root = xml.root.as_ref().iter().find_map(|child| match child {
        XmlNodeChild::Element(node) if tag(node) == "svg" => Some(node),
        _ => None,
    })?;
    let numbers = |value: &str| -> Vec<f32> {
        value
            .split(|c: char| c == ',' || c.is_ascii_whitespace())
            .filter_map(|n| n.trim_end_matches("px").parse().ok())
            .collect()
    };
    let view_box = match attribute(root, "viewBox").map(numbers).as_deref() {
        Some(&[x, y, w, h]) if w > 0.0 && h > 0.0 => [x, y, w, h],
        _ => {
            let w = attribute(root, "width").and_then(|v| numbers(v).first().copied())?;
            let h = attribute(root, "height").and_then(|v| numbers(v).first().copied())?;
            [0.0, 0.0, w, h]
        }
    };
    let mut paths = Vec::new();
    collect_paths(root, &mut paths, 0);
    (!paths.is_empty()).then_some((view_box, paths))
}

fn tag(node: &XmlNode) -> String {
    let name = node.node_type.inner.as_str();
    name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase()
}

fn attribute<'a>(node: &'a XmlNode, name: &str) -> Option<&'a str> {
    node.attributes
        .as_ref()
        .iter()
        .find(|pair| pair.key.as_str() == name)
        .map(|pair| pair.value.as_str())
}

fn colour(value: Option<&str>, default: Option<[f32; 3]>) -> Option<[f32; 3]> {
    let Some(value) = value.map(str::trim) else {
        return default;
    };
    if value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("transparent") {
        return None;
    }
    let c = azul_css::props::basic::color::parse_css_color(value).ok()?;
    Some([
        f32::from(c.r) / 255.0,
        f32::from(c.g) / 255.0,
        f32::from(c.b) / 255.0,
    ])
}

fn collect_paths(node: &XmlNode, out: &mut Vec<StampPath>, depth: usize) {
    if depth > 32 {
        return;
    }
    for child in node.children.as_ref() {
        let XmlNodeChild::Element(child) = child else {
            continue;
        };
        match tag(child).as_str() {
            "path" => {
                if let Some(path) = path_of(child) {
                    out.push(path);
                }
            }
            "g" | "svg" => collect_paths(child, out, depth + 1),
            _ => {}
        }
    }
}

fn path_of(node: &XmlNode) -> Option<StampPath> {
    use azul_core::svg::SvgPathElement;

    let rings = azul_core::path_parser::parse_svg_path_d(attribute(node, "d")?).ok()?;
    let stroke = colour(attribute(node, "stroke"), None).map(|rgb| {
        let width = attribute(node, "stroke-width")
            .and_then(|w| w.trim().trim_end_matches("px").parse().ok())
            .unwrap_or(1.0);
        (width, rgb)
    });
    let fill = colour(attribute(node, "fill"), Some([0.0, 0.0, 0.0]));
    let mut segments = Vec::new();
    for ring in rings.rings.as_ref() {
        let items = ring.items.as_ref();
        let Some(first) = items.first() else {
            continue;
        };
        let start = match first {
            SvgPathElement::Line(l) => l.start,
            SvgPathElement::QuadraticCurve(q) => q.start,
            SvgPathElement::CubicCurve(c) => c.start,
        };
        segments.push(StampSegment::MoveTo(start.x, start.y));
        let mut end = start;
        for item in items {
            match item {
                SvgPathElement::Line(l) => {
                    segments.push(StampSegment::LineTo(l.end.x, l.end.y));
                    end = l.end;
                }
                SvgPathElement::QuadraticCurve(q) => {
                    // The cubic of a quadratic: control points 2/3 of the
                    // way to the quadratic's.
                    let c1 = (
                        q.start.x + (q.ctrl.x - q.start.x) * 2.0 / 3.0,
                        q.start.y + (q.ctrl.y - q.start.y) * 2.0 / 3.0,
                    );
                    let c2 = (
                        q.end.x + (q.ctrl.x - q.end.x) * 2.0 / 3.0,
                        q.end.y + (q.ctrl.y - q.end.y) * 2.0 / 3.0,
                    );
                    segments.push(StampSegment::CubicTo(c1.0, c1.1, c2.0, c2.1, q.end.x, q.end.y));
                    end = q.end;
                }
                SvgPathElement::CubicCurve(c) => {
                    segments.push(StampSegment::CubicTo(
                        c.ctrl_1.x, c.ctrl_1.y, c.ctrl_2.x, c.ctrl_2.y, c.end.x, c.end.y,
                    ));
                    end = c.end;
                }
            }
        }
        if (end.x - start.x).abs() < f32::EPSILON && (end.y - start.y).abs() < f32::EPSILON && items.len() > 1 {
            segments.push(StampSegment::Close);
        }
    }
    (!segments.is_empty()).then_some(StampPath {
        segments,
        stroke,
        fill,
    })
}
