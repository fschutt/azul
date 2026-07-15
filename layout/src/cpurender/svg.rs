#[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
use super::*;

use azul_core::resources::ImageRef;
use agg_rust::basics::{FillingRule, PATH_FLAGS_NONE};
use agg_rust::color::Rgba8;
use agg_rust::conv_stroke::ConvStroke;
use agg_rust::conv_transform::ConvTransform;
use agg_rust::path_storage::PathStorage;
use agg_rust::trans_affine::TransAffine;

/// Render raw SVG bytes to a PNG image.
///
/// Parses the SVG XML, walks the element tree, extracts path geometry +
/// fill/stroke attributes, and rasterizes via agg-rust directly (no CSS
/// layout involved).
#[cfg(all(feature = "std", feature = "xml"))]
/// # Errors
///
/// Returns an error string if the SVG cannot be parsed or rendered.
pub fn render_svg_to_png(
    svg_data: &[u8],
    target_width: u32,
    target_height: u32,
) -> Result<Vec<u8>, String> {
    let svg_str =
        core::str::from_utf8(svg_data).map_err(|e| format!("SVG is not valid UTF-8: {e}"))?;

    let nodes =
        crate::xml::parse_xml_string(svg_str).map_err(|e| format!("XML parse error: {e}"))?;

    // Find the <svg> root
    let node_slice: &[azul_core::xml::XmlNodeChild] = nodes.as_ref();
    let svg_node = node_slice
        .iter()
        .find_map(|n| {
            if let azul_core::xml::XmlNodeChild::Element(e) = n {
                let tag = e.node_type.as_str().to_lowercase();
                if tag == "svg" {
                    Some(e)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .ok_or_else(|| "No <svg> root element found".to_string())?;

    // Parse viewBox for coordinate mapping
    let vb = parse_viewbox(svg_node);
    let (vb_x, vb_y, vb_w, vb_h) =
        vb.unwrap_or_else(|| (0.0, 0.0, f64::from(target_width), f64::from(target_height)));

    let sx = f64::from(target_width) / vb_w;
    let sy = f64::from(target_height) / vb_h;
    let scale = sx.min(sy);

    let root_transform =
        TransAffine::new_custom(scale, 0.0, 0.0, scale, -vb_x * scale, -vb_y * scale);

    let mut pixmap = AzulPixmap::new(target_width, target_height)
        .ok_or_else(|| "Failed to create pixmap".to_string())?;
    pixmap.fill(255, 255, 255, 255);

    render_svg_group(svg_node, &mut pixmap, &root_transform);

    pixmap
        .encode_png()
        .map_err(|e| format!("PNG encode error: {e}"))
}

/// Like [`render_svg_to_png`] but returns the rendered pixmap as an [`ImageRef`]
/// (RGBA8) directly — no PNG round-trip.
///
/// The `MapWidget` uses this to render each
/// decoded tile SVG to a colour image node: `SvgNodeData::Path` in the DOM only
/// produces a clip mask (not a filled shape), so reuse the same `render_svg_group`
/// rasteriser the tiger uses (which reads SVG fill/stroke attrs) and embed the
/// result as an image.
/// # Errors
///
/// Returns an error string if the SVG cannot be parsed or rendered.
pub fn render_svg_to_imageref(
    svg_data: &[u8],
    target_width: u32,
    target_height: u32,
) -> Result<ImageRef, String> {
    let svg_str =
        core::str::from_utf8(svg_data).map_err(|e| format!("SVG is not valid UTF-8: {e}"))?;
    let nodes =
        crate::xml::parse_xml_string(svg_str).map_err(|e| format!("XML parse error: {e}"))?;
    let node_slice: &[azul_core::xml::XmlNodeChild] = nodes.as_ref();
    let svg_node = node_slice
        .iter()
        .find_map(|n| {
            if let azul_core::xml::XmlNodeChild::Element(e) = n {
                if e.node_type.as_str().to_lowercase() == "svg" {
                    Some(e)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .ok_or_else(|| "No <svg> root element found".to_string())?;

    let vb = parse_viewbox(svg_node);
    let (vb_x, vb_y, vb_w, vb_h) =
        vb.unwrap_or_else(|| (0.0, 0.0, f64::from(target_width), f64::from(target_height)));
    let scale = (f64::from(target_width) / vb_w).min(f64::from(target_height) / vb_h);
    let root_transform =
        TransAffine::new_custom(scale, 0.0, 0.0, scale, -vb_x * scale, -vb_y * scale);

    let mut pixmap = AzulPixmap::new(target_width, target_height)
        .ok_or_else(|| "Failed to create pixmap".to_string())?;
    // Transparent background so the tile container shows through any gaps.
    pixmap.fill(0, 0, 0, 0);
    render_svg_group(svg_node, &mut pixmap, &root_transform);

    let rgba = pixmap.data().to_vec();
    let raw = azul_core::resources::RawImage {
        pixels: azul_core::resources::RawImageData::U8(rgba.into()),
        width: target_width as usize,
        height: target_height as usize,
        premultiplied_alpha: false,
        data_format: azul_core::resources::RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    };
    ImageRef::new_rawimage(raw).ok_or_else(|| "Failed to build ImageRef from pixmap".to_string())
}

#[cfg(all(feature = "std", feature = "xml"))]
fn parse_viewbox(node: &azul_core::xml::XmlNode) -> Option<(f64, f64, f64, f64)> {
    let vb = node
        .attributes
        .get_key("viewbox")
        .or_else(|| node.attributes.get_key("viewBox"))?;
    let nums: Vec<f64> = vb
        .as_str()
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    if nums.len() == 4 {
        Some((nums[0], nums[1], nums[2], nums[3]))
    } else {
        None
    }
}

/// Inherited SVG style (fill, stroke, stroke-width) that cascades from parent groups.
#[cfg(all(feature = "std", feature = "xml"))]
#[derive(Clone)]
#[derive(Default)]
struct SvgInheritedStyle {
    fill: Option<String>,   // None = not set (inherit default black)
    stroke: Option<String>, // None = not set (inherit default none)
    stroke_width: Option<f64>,
}

#[cfg(all(feature = "std", feature = "xml"))]
fn render_svg_group(
    node: &azul_core::xml::XmlNode,
    pixmap: &mut AzulPixmap,
    parent_transform: &TransAffine,
) {
    render_svg_group_with_style(
        node,
        pixmap,
        parent_transform,
        &SvgInheritedStyle::default(),
    );
}

#[cfg(all(feature = "std", feature = "xml"))]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn render_svg_group_with_style(
    node: &azul_core::xml::XmlNode,
    pixmap: &mut AzulPixmap,
    parent_transform: &TransAffine,
    parent_style: &SvgInheritedStyle,
) {
    use agg_rust::math_stroke::{LineCap, LineJoin};
    use azul_core::xml::{XmlNode, XmlNodeChild};

    let group_transform = node.attributes.get_key("transform").map_or(*parent_transform, |t| {
        let mut tf = parse_svg_transform(t.as_str());
        tf.premultiply(parent_transform);
        tf
    });

    // Inherit style from this group's attributes
    let group_style = SvgInheritedStyle {
        fill: node
            .attributes
            .get_key("fill")
            .map(|s| s.as_str().to_string())
            .or_else(|| parent_style.fill.clone()),
        stroke: node
            .attributes
            .get_key("stroke")
            .map(|s| s.as_str().to_string())
            .or_else(|| parent_style.stroke.clone()),
        stroke_width: node
            .attributes
            .get_key("stroke-width")
            .and_then(|s| s.as_str().parse().ok())
            .or(parent_style.stroke_width),
    };

    for child in node.children.as_ref() {
        let XmlNodeChild::Element(child_node) = child else {
            continue;
        };

        let tag = child_node.node_type.as_str().to_lowercase();

        match tag.as_str() {
            "g" | "svg" => {
                render_svg_group_with_style(child_node, pixmap, &group_transform, &group_style);
            }
            "path" | "circle" | "rect" | "ellipse" | "line" | "polygon" | "polyline" => {
                let Some(path_storage) = build_agg_path(child_node) else {
                    continue;
                };

                // Flatten bezier curves into line segments for the rasterizer
                let mut curved = agg_rust::conv_curve::ConvCurve::new(path_storage);

                // Per-element transform
                let elem_transform = child_node.attributes.get_key("transform").map_or(group_transform, |t| {
                    let mut tf = parse_svg_transform(t.as_str());
                    tf.premultiply(&group_transform);
                    tf
                });

                // Fill: element overrides group
                let fill_attr = child_node
                    .attributes
                    .get_key("fill")
                    .map(|s| s.as_str().to_string())
                    .or_else(|| group_style.fill.clone());
                let fill_color = match fill_attr.as_deref() {
                    Some("none") => None,
                    Some(c) => parse_svg_color(c),
                    None => Some(Rgba8 {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255,
                    }), // SVG default
                };

                let fill_opacity = child_node
                    .attributes
                    .get_key("fill-opacity")
                    .and_then(|s| s.as_str().parse::<f64>().ok())
                    .unwrap_or(1.0);

                let opacity = child_node
                    .attributes
                    .get_key("opacity")
                    .and_then(|s| s.as_str().parse::<f64>().ok())
                    .unwrap_or(1.0);

                if let Some(mut color) = fill_color {
                    color.a = (f64::from(color.a) * fill_opacity * opacity).min(255.0) as u8;

                    let fill_rule_str = child_node
                        .attributes
                        .get_key("fill-rule")
                        .map(|s| s.as_str().to_string());
                    let rule = match fill_rule_str.as_deref() {
                        Some("evenodd") => FillingRule::EvenOdd,
                        _ => FillingRule::NonZero,
                    };

                    let mut transformed = ConvTransform::new(&mut curved, elem_transform);
                    agg_fill_path(pixmap, &mut transformed, &color, rule);
                }

                // Stroke: element overrides group
                let stroke_attr = child_node
                    .attributes
                    .get_key("stroke")
                    .map(|s| s.as_str().to_string())
                    .or_else(|| group_style.stroke.clone());
                let stroke_color = match stroke_attr.as_deref() {
                    Some("none") | None => None,
                    Some(c) => parse_svg_color(c),
                };

                if let Some(mut color) = stroke_color {
                    let stroke_opacity = child_node
                        .attributes
                        .get_key("stroke-opacity")
                        .and_then(|s| s.as_str().parse::<f64>().ok())
                        .unwrap_or(1.0);
                    color.a = (f64::from(color.a) * stroke_opacity * opacity).min(255.0) as u8;

                    let stroke_width = child_node
                        .attributes
                        .get_key("stroke-width")
                        .and_then(|s| s.as_str().parse::<f64>().ok())
                        .or(group_style.stroke_width)
                        .unwrap_or(1.0);

                    let mut conv_stroke = ConvStroke::new(&mut curved);
                    conv_stroke.set_width(stroke_width);
                    conv_stroke.set_line_cap(LineCap::Round);
                    conv_stroke.set_line_join(LineJoin::Round);

                    let mut transformed =
                        ConvTransform::new(&mut conv_stroke, elem_transform);
                    agg_fill_path(pixmap, &mut transformed, &color, FillingRule::NonZero);
                }
            }
            _ => {
                // Recurse into unknown containers (defs, symbol, etc.)
                render_svg_group_with_style(child_node, pixmap, &group_transform, &group_style);
            }
        }
    }
}

/// Build an agg `PathStorage` from an SVG shape element's attributes.
#[cfg(all(feature = "std", feature = "xml"))]
#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
fn build_agg_path(node: &azul_core::xml::XmlNode) -> Option<PathStorage> {
    const KAPPA: f64 = 0.552_284_749_8;
    let tag = node.node_type.as_str().to_lowercase();
    match tag.as_str() {
        "path" => {
            let d = node.attributes.get_key("d")?;
            let mp = azul_core::path_parser::parse_svg_path_d(d.as_str()).ok()?;
            Some(svg_multi_polygon_to_path_storage(&mp))
        }
        "circle" => {
            let cx = attr_f64(node, "cx");
            let cy = attr_f64(node, "cy");
            let r = attr_f64(node, "r");
            if r <= 0.0 {
                return None;
            }
            let mp = azul_core::path_parser::svg_circle_to_paths(cx as f32, cy as f32, r as f32);
            let multi = azul_core::svg::SvgMultiPolygon {
                rings: azul_core::svg::SvgPathVec::from_vec(vec![mp]),
            };
            Some(svg_multi_polygon_to_path_storage(&multi))
        }
        "rect" => {
            let x = attr_f64(node, "x");
            let y = attr_f64(node, "y");
            let w = attr_f64(node, "width");
            let h = attr_f64(node, "height");
            let rx = attr_f64(node, "rx");
            let ry = node.attributes.get_key("ry").map_or(rx, |v| v.as_str().parse().unwrap_or(rx));
            if w <= 0.0 || h <= 0.0 {
                return None;
            }
            let mp = azul_core::path_parser::svg_rect_to_path(
                x as f32, y as f32, w as f32, h as f32, rx as f32, ry as f32,
            );
            let multi = azul_core::svg::SvgMultiPolygon {
                rings: azul_core::svg::SvgPathVec::from_vec(vec![mp]),
            };
            Some(svg_multi_polygon_to_path_storage(&multi))
        }
        "ellipse" => {
            let cx = attr_f64(node, "cx");
            let cy = attr_f64(node, "cy");
            let rx = attr_f64(node, "rx");
            let ry = attr_f64(node, "ry");
            if rx <= 0.0 || ry <= 0.0 {
                return None;
            }
            // Use circle path with scaling
            let mp = azul_core::path_parser::svg_circle_to_paths(cx as f32, cy as f32, 1.0);
            let multi = azul_core::svg::SvgMultiPolygon {
                rings: azul_core::svg::SvgPathVec::from_vec(vec![mp]),
            };
            let mut ps = svg_multi_polygon_to_path_storage(&multi);
            // Scale ellipse: we'll just build it directly instead
            let mut path = PathStorage::new();
            let kx = rx * KAPPA;
            let ky = ry * KAPPA;
            path.move_to(cx, cy - ry);
            path.curve4(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy);
            path.curve4(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry);
            path.curve4(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy);
            path.curve4(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry);
            path.close_polygon(PATH_FLAGS_NONE);
            Some(path)
        }
        "line" => {
            let x1 = attr_f64(node, "x1");
            let y1 = attr_f64(node, "y1");
            let x2 = attr_f64(node, "x2");
            let y2 = attr_f64(node, "y2");
            let mut path = PathStorage::new();
            path.move_to(x1, y1);
            path.line_to(x2, y2);
            Some(path)
        }
        "polygon" | "polyline" => {
            let pts_str = node.attributes.get_key("points")?;
            let nums: Vec<f64> = pts_str
                .as_str()
                .split(|c: char| c == ',' || c.is_ascii_whitespace())
                .filter(|s| !s.is_empty())
                .filter_map(|s| s.parse().ok())
                .collect();
            if nums.len() < 4 {
                return None;
            }
            let mut path = PathStorage::new();
            path.move_to(nums[0], nums[1]);
            for chunk in nums[2..].chunks_exact(2) {
                path.line_to(chunk[0], chunk[1]);
            }
            if tag == "polygon" {
                path.close_polygon(PATH_FLAGS_NONE);
            }
            Some(path)
        }
        _ => None,
    }
}

#[cfg(all(feature = "std", feature = "xml"))]
fn attr_f64(node: &azul_core::xml::XmlNode, key: &str) -> f64 {
    let v: f64 = node
        .attributes
        .get_key(key)
        .and_then(|s| s.as_str().parse().ok())
        .unwrap_or(0.0);
    // Clamp to a finite range far beyond any real SVG coordinate. A pathological
    // attribute (r="1e308", width="1e400", NaN) otherwise flows into geometry as ±inf
    // and hangs AGG's adaptive Bézier/arc flattening, which subdivides forever trying to
    // meet a flatness tolerance it can never reach. Real coordinates are untouched.
    if v.is_nan() {
        0.0
    } else {
        v.clamp(-1.0e6, 1.0e6)
    }
}

/// Convert `SvgMultiPolygon` to agg `PathStorage`.
#[cfg(all(feature = "std", feature = "xml"))]
fn svg_multi_polygon_to_path_storage(mp: &azul_core::svg::SvgMultiPolygon) -> PathStorage {
    let mut path = PathStorage::new();
    for ring in mp.rings.as_ref() {
        let mut first = true;
        for item in ring.items.as_ref() {
            match item {
                azul_core::svg::SvgPathElement::Line(l) => {
                    if first {
                        path.move_to(f64::from(l.start.x), f64::from(l.start.y));
                        first = false;
                    }
                    path.line_to(f64::from(l.end.x), f64::from(l.end.y));
                }
                azul_core::svg::SvgPathElement::QuadraticCurve(q) => {
                    if first {
                        path.move_to(f64::from(q.start.x), f64::from(q.start.y));
                        first = false;
                    }
                    path.curve3(
                        f64::from(q.ctrl.x),
                        f64::from(q.ctrl.y),
                        f64::from(q.end.x),
                        f64::from(q.end.y),
                    );
                }
                azul_core::svg::SvgPathElement::CubicCurve(c) => {
                    if first {
                        path.move_to(f64::from(c.start.x), f64::from(c.start.y));
                        first = false;
                    }
                    path.curve4(
                        f64::from(c.ctrl_1.x),
                        f64::from(c.ctrl_1.y),
                        f64::from(c.ctrl_2.x),
                        f64::from(c.ctrl_2.y),
                        f64::from(c.end.x),
                        f64::from(c.end.y),
                    );
                }
            }
        }
        path.close_polygon(PATH_FLAGS_NONE);
    }
    path
}

/// Parse SVG transform attribute (supports matrix, translate, scale, rotate).
#[cfg(all(feature = "std", feature = "xml"))]
fn parse_svg_transform(s: &str) -> TransAffine {
    let s = s.trim();

    let parse_nums = |inner: &str| -> Vec<f64> {
        inner
            .split(|c: char| c == ',' || c.is_ascii_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect()
    };

    if let Some(inner) = s.strip_prefix("matrix(").and_then(|s| s.strip_suffix(')')) {
        let nums = parse_nums(inner);
        if nums.len() == 6 {
            return TransAffine::new_custom(nums[0], nums[1], nums[2], nums[3], nums[4], nums[5]);
        }
    } else if let Some(inner) = s
        .strip_prefix("translate(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let nums = parse_nums(inner);
        let tx = nums.first().copied().unwrap_or(0.0);
        let ty = nums.get(1).copied().unwrap_or(0.0);
        return TransAffine::new_custom(1.0, 0.0, 0.0, 1.0, tx, ty);
    } else if let Some(inner) = s.strip_prefix("scale(").and_then(|s| s.strip_suffix(')')) {
        let nums = parse_nums(inner);
        let sx = nums.first().copied().unwrap_or(1.0);
        let sy = nums.get(1).copied().unwrap_or(sx);
        return TransAffine::new_custom(sx, 0.0, 0.0, sy, 0.0, 0.0);
    } else if let Some(inner) = s.strip_prefix("rotate(").and_then(|s| s.strip_suffix(')')) {
        let nums = parse_nums(inner);
        let angle = nums.first().copied().unwrap_or(0.0).to_radians();
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        return TransAffine::new_custom(cos_a, sin_a, -sin_a, cos_a, 0.0, 0.0);
    }
    TransAffine::new()
}

/// Parse SVG color string (#RRGGBB, #RGB, named colors).
#[cfg(all(feature = "std", feature = "xml"))]
fn parse_svg_color(s: &str) -> Option<Rgba8> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        // The arms below index `hex` at fixed BYTE offsets, but `hex.len()` is a byte
        // count: a multibyte char (e.g. "#€123", € = 3 bytes) makes the byte length hit
        // the 6/3 arm while the slice boundary lands mid-character and panics. Valid hex
        // is ASCII, so reject anything else up front.
        if !hex.is_ascii() {
            return None;
        }
        return match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Rgba8 { r, g, b, a: 255 })
            }
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some(Rgba8 { r, g, b, a: 255 })
            }
            _ => None,
        };
    }
    match s.to_lowercase().as_str() {
        "black" => Some(Rgba8 {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }),
        "white" => Some(Rgba8 {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }),
        "red" => Some(Rgba8 {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }),
        "green" => Some(Rgba8 {
            r: 0,
            g: 128,
            b: 0,
            a: 255,
        }),
        "blue" => Some(Rgba8 {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        }),
        "yellow" => Some(Rgba8 {
            r: 255,
            g: 255,
            b: 0,
            a: 255,
        }),
        "orange" => Some(Rgba8 {
            r: 255,
            g: 165,
            b: 0,
            a: 255,
        }),
        "gold" => Some(Rgba8 {
            r: 255,
            g: 215,
            b: 0,
            a: 255,
        }),
        _ => None,
    }
}

#[cfg(all(test, feature = "std", feature = "xml"))]
#[allow(clippy::float_cmp)] // exact f64 compares are the point here: they assert parse fidelity, not arithmetic
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // pixel-index math in assertions
mod autotest_generated {
    use azul_core::{
        window::{AzStringPair, StringPairVec},
        xml::{XmlAttributeMap, XmlNode, XmlNodeChild, XmlNodeChildVec},
    };

    use super::*;
    use crate::cpurender::AzulPixmap;

    // ------------------------------------------------------------------
    // helpers
    // ------------------------------------------------------------------

    /// Build an `XmlNode` with the given tag + attributes and no children.
    fn el(tag: &str, pairs: &[(&str, &str)]) -> XmlNode {
        el_with(tag, pairs, Vec::new())
    }

    /// Build an `XmlNode` with the given tag, attributes and element children.
    fn el_with(tag: &str, pairs: &[(&str, &str)], children: Vec<XmlNode>) -> XmlNode {
        XmlNode {
            node_type: tag.into(),
            attributes: XmlAttributeMap {
                inner: StringPairVec::from_vec(
                    pairs
                        .iter()
                        .map(|(k, v)| AzStringPair {
                            key: (*k).into(),
                            value: (*v).into(),
                        })
                        .collect::<Vec<_>>(),
                ),
            },
            children: XmlNodeChildVec::from_vec(
                children.into_iter().map(XmlNodeChild::Element).collect(),
            ),
        }
    }

    fn pixmap(w: u32, h: u32) -> AzulPixmap {
        AzulPixmap::new(w, h).expect("pixmap alloc")
    }

    /// RGBA of pixel (x, y).
    fn px(p: &AzulPixmap, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * p.width() + x) * 4) as usize;
        let d = p.data();
        [d[i], d[i + 1], d[i + 2], d[i + 3]]
    }

    fn is_all_white(p: &AzulPixmap) -> bool {
        p.data().iter().all(|&b| b == 255)
    }

    const RED: Rgba8 = Rgba8 {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    const BLACK: Rgba8 = Rgba8 {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    /// 16x16 red square — the positive control for the two public entry points.
    const MINIMAL_SVG: &[u8] =
        br#"<svg viewBox="0 0 16 16"><rect x="0" y="0" width="16" height="16" fill="red"/></svg>"#;

    // ==================================================================
    // parse_svg_color  (parser)
    // ==================================================================

    #[test]
    fn parse_svg_color_empty_and_whitespace_are_none() {
        assert_eq!(parse_svg_color(""), None);
        assert_eq!(parse_svg_color("   "), None);
        assert_eq!(parse_svg_color("\t\n\r "), None);
        assert_eq!(parse_svg_color("#"), None);
    }

    #[test]
    fn parse_svg_color_valid_minimal_six_digit_hex() {
        assert_eq!(
            parse_svg_color("#ff0000"),
            Some(RED),
            "#ff0000 is the positive control"
        );
        assert_eq!(parse_svg_color("#000000"), Some(BLACK));
    }

    #[test]
    fn parse_svg_color_six_digit_hex_is_case_insensitive() {
        assert_eq!(parse_svg_color("#FF0000"), parse_svg_color("#ff0000"));
        assert_eq!(parse_svg_color("#AbCdEf"), parse_svg_color("#abcdef"));
    }

    #[test]
    fn parse_svg_color_three_digit_hex_expands_by_seventeen() {
        // #abc -> a*17=170, b*17=187, c*17=204 (and must not overflow: f*17 == 255)
        assert_eq!(
            parse_svg_color("#abc"),
            Some(Rgba8 {
                r: 170,
                g: 187,
                b: 204,
                a: 255
            })
        );
        assert_eq!(
            parse_svg_color("#fff"),
            Some(Rgba8 {
                r: 255,
                g: 255,
                b: 255,
                a: 255
            }),
            "the *17 expansion must land on exactly 255, not wrap"
        );
        assert_eq!(parse_svg_color("#000"), Some(BLACK));
        assert_eq!(parse_svg_color("#f00"), Some(RED));
    }

    #[test]
    fn parse_svg_color_wrong_hex_lengths_are_none() {
        // note: 8-digit #RRGGBBAA is valid CSS but unsupported here
        for s in ["#f", "#ff", "#ffff", "#fffff", "#fffffff", "#ffffffff"] {
            assert_eq!(parse_svg_color(s), None, "{s} must be rejected");
        }
    }

    #[test]
    fn parse_svg_color_non_hex_digits_are_none() {
        assert_eq!(parse_svg_color("#gggggg"), None);
        assert_eq!(parse_svg_color("#00ff0g"), None);
        assert_eq!(parse_svg_color("#zzz"), None);
        assert_eq!(parse_svg_color("#0x0000"), None);
        assert_eq!(parse_svg_color("#-10000"), None); // 7 bytes -> rejected on length
        assert_eq!(parse_svg_color("#-1-1-1"), None, "negative hex components");
    }

    #[test]
    fn parse_svg_color_leading_trailing_whitespace_is_trimmed() {
        assert_eq!(parse_svg_color("  #ff0000  "), Some(RED));
        assert_eq!(parse_svg_color("\n\tred\r\n"), Some(RED));
    }

    #[test]
    fn parse_svg_color_named_colors_are_case_insensitive() {
        assert_eq!(parse_svg_color("RED"), Some(RED));
        assert_eq!(parse_svg_color("Red"), Some(RED));
        assert_eq!(parse_svg_color("black"), Some(BLACK));
        assert_eq!(
            parse_svg_color("green"),
            Some(Rgba8 {
                r: 0,
                g: 128,
                b: 0,
                a: 255
            }),
            "SVG `green` is 008000, not 00ff00"
        );
        assert_eq!(
            parse_svg_color("gold"),
            Some(Rgba8 {
                r: 255,
                g: 215,
                b: 0,
                a: 255
            })
        );
    }

    #[test]
    fn parse_svg_color_unsupported_named_colors_are_none() {
        // Only 8 names are in the table; every other CSS keyword silently falls
        // back to None (the caller then paints nothing).
        for s in [
            "gray",
            "grey",
            "cyan",
            "magenta",
            "transparent",
            "currentColor",
        ] {
            assert_eq!(parse_svg_color(s), None, "{s} is not in the named table");
        }
    }

    #[test]
    fn parse_svg_color_leading_trailing_junk_is_rejected() {
        assert_eq!(parse_svg_color("red;garbage"), None);
        assert_eq!(parse_svg_color("#ff0000;"), None);
        assert_eq!(parse_svg_color("rgb(255,0,0)"), None, "rgb() is unsupported");
        assert_eq!(parse_svg_color("url(#grad)"), None, "paint servers -> None");
    }

    #[test]
    fn parse_svg_color_garbage_bytes_never_panic() {
        for s in [
            "!!!",
            "\u{0}\u{1}\u{2}",
            "########",
            "#\u{0}\u{0}\u{0}",
            "%%%",
        ] {
            let _ = parse_svg_color(s);
        }
    }

    #[test]
    fn parse_svg_color_extremely_long_input_is_none_and_does_not_hang() {
        let long_hex = format!("#{}", "f".repeat(1_000_000));
        assert_eq!(parse_svg_color(&long_hex), None);

        let long_name = "a".repeat(1_000_000);
        assert_eq!(parse_svg_color(&long_name), None);
    }

    #[test]
    fn parse_svg_color_unicode_is_none_not_panic() {
        // Emoji (4-byte) and CJK (3-byte) chars land on byte lengths that never
        // reach the 3/6-byte hex arms, so they are safely rejected.
        assert_eq!(parse_svg_color("\u{1F600}"), None);
        assert_eq!(parse_svg_color("#\u{1F600}"), None); // hex byte-len 4
        assert_eq!(parse_svg_color("\u{4F60}\u{597D}"), None);
        assert_eq!(parse_svg_color("e\u{301}"), None); // combining acute
        assert_eq!(
            parse_svg_color("#\u{e9}\u{e9}12"),
            None,
            "2-byte chars keep bytes 2/4 on char boundaries -> Err -> None"
        );
    }

    #[test]
    fn parse_svg_color_plus_prefixed_hex_components_do_not_panic() {
        // `u8::from_str_radix` accepts a leading '+', so "#+1+2+3" is 6 bytes of
        // "valid" hex and parses as rgb(1, 2, 3) even though it is not legal
        // SVG. Not a safety bug — only assert it is deterministic and total.
        let quirk = parse_svg_color("#+1+2+3");
        assert!(
            quirk.is_none()
                || quirk
                    == Some(Rgba8 {
                        r: 1,
                        g: 2,
                        b: 3,
                        a: 255
                    }),
            "lenient '+' hex must be rejected or rgb(1,2,3), got {quirk:?}"
        );
    }

    #[test]
    fn red_parse_svg_color_multibyte_hex_must_not_panic() {
        // BUG: the 3/6 arms slice `hex` by *byte* index (`&hex[0..2]`,
        // `&hex[0..1]`) after only checking `hex.len()`, which is a byte length.
        //
        // U+20AC EURO SIGN is 3 UTF-8 bytes, so:
        //   "#\u{20AC}123" -> hex = "\u{20AC}123", byte len 6 -> `&hex[0..2]`
        //   "#\u{20AC}"    -> hex = "\u{20AC}",    byte len 3 -> `&hex[0..1]`
        // Both slice *inside* the euro sign -> "byte index N is not a char
        // boundary" panic. An attacker-supplied `fill="#<3-byte char>123"`
        // crashes the renderer instead of falling back to no-paint.
        //
        // Correct behaviour: None. Fix = index `hex.as_bytes()` / reject
        // non-ASCII before slicing.
        assert_eq!(parse_svg_color("#\u{20AC}123"), None);
        assert_eq!(parse_svg_color("#\u{20AC}"), None);
    }

    // ==================================================================
    // parse_svg_transform  (parser)
    // ==================================================================

    fn assert_identity(t: &TransAffine) {
        assert_eq!(
            (t.sx, t.shy, t.shx, t.sy, t.tx, t.ty),
            (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
        );
    }

    #[test]
    fn parse_svg_transform_empty_and_whitespace_are_identity() {
        assert_identity(&parse_svg_transform(""));
        assert_identity(&parse_svg_transform("   "));
        assert_identity(&parse_svg_transform("\t\n\r "));
    }

    #[test]
    fn parse_svg_transform_valid_minimal_matrix() {
        let t = parse_svg_transform("matrix(1,2,3,4,5,6)");
        assert_eq!(
            (t.sx, t.shy, t.shx, t.sy, t.tx, t.ty),
            (1.0, 2.0, 3.0, 4.0, 5.0, 6.0)
        );
    }

    #[test]
    fn parse_svg_transform_matrix_accepts_space_separated_numbers() {
        let t = parse_svg_transform("matrix( 1 0 0 1 10 20 )");
        assert_eq!(
            (t.sx, t.shy, t.shx, t.sy, t.tx, t.ty),
            (1.0, 0.0, 0.0, 1.0, 10.0, 20.0)
        );
    }

    #[test]
    fn parse_svg_transform_matrix_with_wrong_arity_is_identity() {
        assert_identity(&parse_svg_transform("matrix(1,2,3)"));
        assert_identity(&parse_svg_transform("matrix(1,2,3,4,5,6,7)"));
        assert_identity(&parse_svg_transform("matrix()"));
    }

    #[test]
    fn parse_svg_transform_translate_defaults_missing_ty_to_zero() {
        let t = parse_svg_transform("translate(10)");
        assert_eq!((t.tx, t.ty), (10.0, 0.0));
        let t = parse_svg_transform("translate(10, 20)");
        assert_eq!((t.tx, t.ty), (10.0, 20.0));
        assert_identity(&parse_svg_transform("translate()"));
    }

    #[test]
    fn parse_svg_transform_scale_defaults_sy_to_sx() {
        let t = parse_svg_transform("scale(2)");
        assert_eq!((t.sx, t.sy), (2.0, 2.0), "uniform scale when sy is omitted");
        let t = parse_svg_transform("scale(2,3)");
        assert_eq!((t.sx, t.sy), (2.0, 3.0));
        assert_identity(&parse_svg_transform("scale()"));
    }

    #[test]
    fn parse_svg_transform_scale_zero_is_degenerate_but_not_a_panic() {
        let t = parse_svg_transform("scale(0)");
        assert_eq!((t.sx, t.sy), (0.0, 0.0));
    }

    #[test]
    fn parse_svg_transform_rotate_90_degrees() {
        let t = parse_svg_transform("rotate(90)");
        assert!((t.sx - 0.0).abs() < 1e-9, "cos(90deg) ~ 0, got {}", t.sx);
        assert!((t.shy - 1.0).abs() < 1e-9, "sin(90deg) == 1, got {}", t.shy);
        assert!((t.shx + 1.0).abs() < 1e-9, "-sin(90deg) == -1, got {}", t.shx);
        assert!((t.sy - 0.0).abs() < 1e-9);
        assert_eq!((t.tx, t.ty), (0.0, 0.0));
        assert_identity(&parse_svg_transform("rotate(0)"));
    }

    #[test]
    fn parse_svg_transform_rotate_with_center_ignores_the_center() {
        // `rotate(angle cx cy)` is legal SVG; the extra args are silently dropped
        // and the rotation happens around the origin instead of (cx, cy).
        let with_center = parse_svg_transform("rotate(90 50 50)");
        let without = parse_svg_transform("rotate(90)");
        assert_eq!((with_center.tx, with_center.ty), (0.0, 0.0));
        assert_eq!(with_center.sx, without.sx);
    }

    #[test]
    fn parse_svg_transform_unclosed_paren_is_identity() {
        assert_identity(&parse_svg_transform("translate(10"));
        assert_identity(&parse_svg_transform("matrix(1,2,3,4,5,6"));
        assert_identity(&parse_svg_transform("scale(2"));
    }

    #[test]
    fn parse_svg_transform_is_case_sensitive() {
        // Uppercase function names are not SVG-legal, so identity is acceptable —
        // pinned so that adding a lowercasing pass is a deliberate change.
        assert_identity(&parse_svg_transform("TRANSLATE(10,20)"));
        assert_identity(&parse_svg_transform("Scale(2)"));
    }

    #[test]
    fn parse_svg_transform_leading_trailing_junk() {
        // Leading/trailing *whitespace* is trimmed...
        let t = parse_svg_transform("  translate(10,20)  ");
        assert_eq!((t.tx, t.ty), (10.0, 20.0));
        // ...but trailing junk breaks the `)` suffix match -> identity.
        assert_identity(&parse_svg_transform("translate(10,20);garbage"));
        assert_identity(&parse_svg_transform("junk translate(10,20)"));
    }

    #[test]
    fn parse_svg_transform_transform_list_keeps_only_the_first_function() {
        // SVG allows a whitespace-separated transform *list*. This parser only
        // understands a single function: for "translate(10,20) scale(2)" the
        // strip_suffix(')') leaves "10,20) scale(2" as the argument text, whose
        // only parseable number is 10 -> ty and the whole scale() are dropped.
        //
        // Characterization, not an endorsement — see the report: a transform
        // list renders *wrong* (ty lost, scale ignored) rather than crashing.
        let t = parse_svg_transform("translate(10,20) scale(2)");
        assert_eq!((t.sx, t.sy), (1.0, 1.0), "scale() silently dropped");
        assert_eq!((t.tx, t.ty), (10.0, 0.0), "translate ty silently dropped");
    }

    #[test]
    fn parse_svg_transform_garbage_is_identity_never_panics() {
        for s in [
            "!!!",
            "()",
            "(((",
            ")))",
            "matrix",
            "translate",
            "\u{0}\u{1}",
            "matrix(,,,,,)",
            "scale(,)",
        ] {
            let t = parse_svg_transform(s);
            assert!(t.sx.is_finite(), "{s} produced a non-finite sx");
        }
    }

    #[test]
    fn parse_svg_transform_boundary_numbers_saturate_not_panic() {
        let t = parse_svg_transform("translate(1e400, -1e400)");
        assert!(t.tx.is_infinite() && t.tx > 0.0, "1e400 -> +inf");
        assert!(t.ty.is_infinite() && t.ty < 0.0, "-1e400 -> -inf");

        let t = parse_svg_transform("scale(NaN)");
        assert!(t.sx.is_nan() && t.sy.is_nan(), "NaN propagates, no panic");

        let t = parse_svg_transform("rotate(NaN)");
        assert!(t.sx.is_nan() && t.shy.is_nan());

        let t = parse_svg_transform("scale(inf)");
        assert!(t.sx.is_infinite());

        let t = parse_svg_transform("translate(-0, 9223372036854775807)");
        assert!(t.tx.is_sign_negative() && t.tx == 0.0, "-0 stays -0.0");
        assert!(t.ty.is_finite(), "i64::MAX fits in f64");

        let t = parse_svg_transform("matrix(1e308,1e308,1e308,1e308,1e308,1e308)");
        assert!(t.sx.is_finite());
    }

    #[test]
    fn parse_svg_transform_unicode_args_are_dropped_not_panic() {
        let t = parse_svg_transform("translate(\u{1F600},\u{1F600})");
        assert_eq!((t.tx, t.ty), (0.0, 0.0));
        assert_identity(&parse_svg_transform("\u{1F600}"));
        // rotate() with an unparseable angle defaults to 0 -> cos 1 / sin 0.
        assert_identity(&parse_svg_transform("rotate(\u{4F60}\u{597D})"));
    }

    #[test]
    fn parse_svg_transform_deeply_nested_input_does_not_stack_overflow() {
        // 10_000 nested "translate(" — the parser is flat (strip_prefix + split),
        // so this must stay linear, never recursive.
        const DEPTH: usize = 10_000;
        let s = format!("{}1{}", "translate(".repeat(DEPTH), ")".repeat(DEPTH));
        let t = parse_svg_transform(&s);
        // The inner text has no separators, so it is one unparseable token.
        assert_eq!((t.tx, t.ty), (0.0, 0.0));
        assert_eq!((t.sx, t.sy), (1.0, 1.0));
    }

    #[test]
    fn parse_svg_transform_extremely_long_arg_list_does_not_hang() {
        let inner = "1,".repeat(200_000);
        let s = format!("translate({inner}1)");
        let t = parse_svg_transform(&s);
        assert_eq!((t.tx, t.ty), (1.0, 1.0), "only the first two args are used");
    }

    // ==================================================================
    // parse_viewbox  (parser)
    // ==================================================================

    #[test]
    fn parse_viewbox_missing_attribute_is_none() {
        assert_eq!(parse_viewbox(&el("svg", &[])), None);
        assert_eq!(parse_viewbox(&el("svg", &[("width", "10")])), None);
    }

    #[test]
    fn parse_viewbox_valid_minimal_both_spellings() {
        // Real SVG uses camelCase `viewBox`; the lowercase key is the fallback
        // for lowercasing XML parsers. Both must work.
        assert_eq!(
            parse_viewbox(&el("svg", &[("viewBox", "0 0 100 50")])),
            Some((0.0, 0.0, 100.0, 50.0))
        );
        assert_eq!(
            parse_viewbox(&el("svg", &[("viewbox", "0 0 100 50")])),
            Some((0.0, 0.0, 100.0, 50.0))
        );
    }

    #[test]
    fn parse_viewbox_accepts_comma_and_mixed_separators() {
        assert_eq!(
            parse_viewbox(&el("svg", &[("viewBox", "0,0,100,50")])),
            Some((0.0, 0.0, 100.0, 50.0))
        );
        assert_eq!(
            parse_viewbox(&el("svg", &[("viewBox", " -1 , -2\t3.5\n4.5 ")])),
            Some((-1.0, -2.0, 3.5, 4.5))
        );
    }

    #[test]
    fn parse_viewbox_wrong_arity_is_none() {
        for v in ["", "   ", "0", "0 0", "0 0 100", "0 0 100 50 25"] {
            assert_eq!(
                parse_viewbox(&el("svg", &[("viewBox", v)])),
                None,
                "viewBox={v:?} must require exactly 4 numbers"
            );
        }
    }

    #[test]
    fn parse_viewbox_garbage_tokens_are_silently_dropped() {
        // filter_map(parse) drops unparseable tokens *before* the len == 4 check,
        // so junk in the middle of an otherwise-valid viewBox is ignored rather
        // than rejected. Characterization: lenient, not unsafe.
        assert_eq!(
            parse_viewbox(&el("svg", &[("viewBox", "0 0 junk 100 50")])),
            Some((0.0, 0.0, 100.0, 50.0))
        );
        assert_eq!(
            parse_viewbox(&el("svg", &[("viewBox", "0 0 100 50 trailing")])),
            Some((0.0, 0.0, 100.0, 50.0))
        );
        // ...but if nothing parses at all it is correctly None.
        assert_eq!(parse_viewbox(&el("svg", &[("viewBox", "a b c d")])), None);
        assert_eq!(parse_viewbox(&el("svg", &[("viewBox", "###")])), None);
    }

    #[test]
    fn parse_viewbox_unit_suffixes_are_rejected() {
        // "0 0 100px 50px" -> the px tokens do not parse -> only 2 numbers -> None.
        assert_eq!(
            parse_viewbox(&el("svg", &[("viewBox", "0 0 100px 50px")])),
            None
        );
    }

    #[test]
    fn parse_viewbox_boundary_numbers() {
        let vb = parse_viewbox(&el("svg", &[("viewBox", "NaN 0 100 50")])).expect("4 numbers");
        assert!(vb.0.is_nan(), "NaN parses and propagates, no panic");

        let vb = parse_viewbox(&el("svg", &[("viewBox", "0 0 1e400 -1e400")])).expect("4 numbers");
        assert!(vb.2.is_infinite() && vb.2 > 0.0);
        assert!(vb.3.is_infinite() && vb.3 < 0.0);

        let vb = parse_viewbox(&el("svg", &[("viewBox", "-0 0 0 0")])).expect("4 numbers");
        assert!(vb.0.is_sign_negative() && vb.0 == 0.0, "-0 stays -0.0");
        assert_eq!((vb.2, vb.3), (0.0, 0.0), "a zero-area viewBox is accepted");

        let vb = parse_viewbox(&el(
            "svg",
            &[("viewBox", "9223372036854775807 -9223372036854775808 1 1")],
        ))
        .expect("4 numbers");
        assert!(vb.0.is_finite() && vb.1.is_finite(), "i64 bounds fit in f64");

        let vb = parse_viewbox(&el("svg", &[("viewBox", "0 0 inf inf")])).expect("4 numbers");
        assert!(vb.2.is_infinite());
    }

    #[test]
    fn parse_viewbox_unicode_is_none_not_panic() {
        assert_eq!(
            parse_viewbox(&el(
                "svg",
                &[("viewBox", "\u{1F600} \u{1F600} \u{1F600} \u{1F600}")]
            )),
            None
        );
        assert_eq!(
            parse_viewbox(&el("svg", &[("viewBox", "\u{4F60}\u{597D}")])),
            None
        );
    }

    #[test]
    fn parse_viewbox_extremely_long_value_does_not_hang() {
        let huge = "1 ".repeat(200_000);
        assert_eq!(
            parse_viewbox(&el("svg", &[("viewBox", huge.as_str())])),
            None,
            "200k numbers != 4 -> None, and it must not hang getting there"
        );
    }

    // ==================================================================
    // attr_f64
    // ==================================================================

    #[test]
    fn attr_f64_missing_key_is_zero() {
        assert_eq!(attr_f64(&el("rect", &[]), "width"), 0.0);
        assert_eq!(attr_f64(&el("rect", &[("height", "5")]), "width"), 0.0);
    }

    #[test]
    fn attr_f64_valid_minimal() {
        assert_eq!(attr_f64(&el("rect", &[("width", "42.5")]), "width"), 42.5);
        assert_eq!(attr_f64(&el("rect", &[("x", "-3")]), "x"), -3.0);
        assert_eq!(attr_f64(&el("rect", &[("x", "1e3")]), "x"), 1000.0);
    }

    #[test]
    fn attr_f64_key_lookup_is_case_sensitive() {
        assert_eq!(attr_f64(&el("rect", &[("Width", "10")]), "width"), 0.0);
    }

    #[test]
    fn attr_f64_unparseable_values_fall_back_to_zero() {
        // NOTE: `f64::from_str` neither trims whitespace nor accepts CSS units,
        // so both of these collapse to 0.0 — which downstream means "no width"
        // and the shape is skipped entirely. See the report.
        assert_eq!(attr_f64(&el("rect", &[("width", "100px")]), "width"), 0.0);
        assert_eq!(attr_f64(&el("rect", &[("width", " 100 ")]), "width"), 0.0);
        assert_eq!(attr_f64(&el("rect", &[("width", "50%")]), "width"), 0.0);
        assert_eq!(attr_f64(&el("rect", &[("width", "")]), "width"), 0.0);
        assert_eq!(attr_f64(&el("rect", &[("width", "abc")]), "width"), 0.0);
        assert_eq!(
            attr_f64(&el("rect", &[("width", "\u{1F600}")]), "width"),
            0.0
        );
    }

    #[test]
    fn attr_f64_boundary_numbers_are_sanitized_to_finite() {
        // attr_f64 now clamps to a finite range (NaN -> 0, ±inf/huge -> ±1e6) so
        // pathological attributes cannot flow into geometry and hang the flattener.
        assert_eq!(attr_f64(&el("rect", &[("width", "NaN")]), "width"), 0.0);
        assert_eq!(attr_f64(&el("rect", &[("width", "inf")]), "width"), 1.0e6);
        assert_eq!(attr_f64(&el("rect", &[("width", "1e400")]), "width"), 1.0e6);
        assert_eq!(attr_f64(&el("rect", &[("width", "1e-400")]), "width"), 0.0);

        let neg_zero = attr_f64(&el("rect", &[("x", "-0")]), "x");
        assert!(neg_zero.is_sign_negative() && neg_zero == 0.0);

        let big = attr_f64(&el("rect", &[("x", "9223372036854775807")]), "x");
        assert!(big.is_finite() && big == 1.0e6);
    }

    #[test]
    fn attr_f64_extremely_long_value_does_not_hang() {
        let long = "9".repeat(1_000_000);
        let v = attr_f64(&el("rect", &[("width", long.as_str())]), "width");
        // Parses (saturating to +inf) then clamps to the finite ceiling.
        assert_eq!(v, 1.0e6, "a 1e999999-ish literal is sanitized to the finite ceiling");
    }

    // ==================================================================
    // build_agg_path
    // ==================================================================

    #[test]
    fn build_agg_path_unknown_tag_is_none() {
        assert!(build_agg_path(&el("g", &[])).is_none());
        assert!(build_agg_path(&el("text", &[])).is_none());
        assert!(build_agg_path(&el("", &[])).is_none());
        assert!(build_agg_path(&el("\u{1F600}", &[])).is_none());
    }

    #[test]
    fn build_agg_path_tag_matching_is_case_insensitive() {
        let p = build_agg_path(&el("LINE", &[("x2", "10"), ("y2", "10")]))
            .expect("uppercase <LINE> must be recognised");
        assert_eq!(p.total_vertices(), 2);
    }

    #[test]
    fn build_agg_path_path_valid_minimal() {
        let p = build_agg_path(&el("path", &[("d", "M 0 0 L 10 10")])).expect("valid d");
        assert!(p.total_vertices() >= 2, "move_to + line_to at minimum");
        assert!(
            agg_rust::basics::is_end_poly(p.last_command()),
            "every ring is terminated by close_polygon()"
        );
    }

    #[test]
    fn build_agg_path_path_missing_or_empty_d_is_none() {
        assert!(build_agg_path(&el("path", &[])).is_none(), "no d attribute");
        assert!(build_agg_path(&el("path", &[("d", "")])).is_none());
        assert!(build_agg_path(&el("path", &[("d", "   ")])).is_none());
        assert!(build_agg_path(&el("path", &[("d", "\t\n")])).is_none());
    }

    #[test]
    fn build_agg_path_path_moveto_only_is_an_empty_path_not_a_panic() {
        // "M 0 0" produces zero path *elements*, so the multipolygon has no rings
        // and the PathStorage comes back empty — Some, but with 0 vertices.
        let p = build_agg_path(&el("path", &[("d", "M 0 0")])).expect("parses");
        assert_eq!(p.total_vertices(), 0);
    }

    #[test]
    fn build_agg_path_path_garbage_d_is_handled_never_panics() {
        for d in [
            "garbage",
            "@@@@@",
            "M",
            "L 1",
            "M 0 0 Z 5", // stray arg after closepath (was a 100%-CPU infinite loop)
            "M0 0Z5",    // same, without separators
            "\u{1F600}",
            "M NaN NaN L inf inf",
        ] {
            // Must terminate without panicking; Some/None are both acceptable.
            let _ = build_agg_path(&el("path", &[("d", d)]));
        }
    }

    #[test]
    fn build_agg_path_path_extremely_long_d_does_not_hang() {
        let mut d = String::from("M 0 0");
        for i in 0..50_000 {
            d.push_str(&format!(" L {i} {i}"));
        }
        let p = build_agg_path(&el("path", &[("d", d.as_str())])).expect("parses");
        assert!(p.total_vertices() > 50_000);
    }

    #[test]
    fn build_agg_path_circle_non_positive_radius_is_none() {
        assert!(
            build_agg_path(&el("circle", &[])).is_none(),
            "r defaults to 0"
        );
        assert!(build_agg_path(&el("circle", &[("r", "0")])).is_none());
        assert!(build_agg_path(&el("circle", &[("r", "-5")])).is_none());
        assert!(
            build_agg_path(&el("circle", &[("r", "5px")])).is_none(),
            "a unit suffix makes attr_f64 return 0.0 -> rejected"
        );
    }

    #[test]
    fn build_agg_path_circle_valid_minimal() {
        let p = build_agg_path(&el("circle", &[("cx", "5"), ("cy", "5"), ("r", "5")]))
            .expect("valid circle");
        // 4 cubic segments: move_to(1) + 4*curve4(3) = 13, + close_polygon = 14
        assert_eq!(p.total_vertices(), 14);
    }

    #[test]
    fn build_agg_path_circle_nan_radius_is_rejected() {
        // attr_f64 now sanitizes NaN -> 0, so a NaN radius becomes r == 0 and is caught
        // by the `if r <= 0.0` guard — rejected up front instead of building a path full
        // of NaN coordinates that could hang the flattener.
        assert!(build_agg_path(&el("circle", &[("r", "NaN")])).is_none());
    }

    #[test]
    fn build_agg_path_circle_infinite_radius_does_not_panic() {
        let p = build_agg_path(&el("circle", &[("r", "1e400")])).expect("inf passes the guard");
        assert_eq!(p.total_vertices(), 14);
    }

    #[test]
    fn build_agg_path_rect_non_positive_size_is_none() {
        assert!(build_agg_path(&el("rect", &[])).is_none());
        assert!(
            build_agg_path(&el("rect", &[("width", "10")])).is_none(),
            "height defaults to 0"
        );
        assert!(
            build_agg_path(&el("rect", &[("height", "10")])).is_none(),
            "width defaults to 0"
        );
        assert!(build_agg_path(&el("rect", &[("width", "-1"), ("height", "10")])).is_none());
        assert!(build_agg_path(&el("rect", &[("width", "10"), ("height", "-1")])).is_none());
    }

    #[test]
    fn build_agg_path_rect_valid_minimal_is_four_lines() {
        let p = build_agg_path(&el(
            "rect",
            &[("x", "1"), ("y", "2"), ("width", "10"), ("height", "20")],
        ))
        .expect("valid rect");
        // 4 line elements: move_to(1) + 4*line_to(4) = 5, + close_polygon = 6
        assert_eq!(p.total_vertices(), 6);
        let mut x = 0.0;
        let mut y = 0.0;
        p.vertex_idx(0, &mut x, &mut y);
        assert_eq!((x, y), (1.0, 2.0), "the path starts at (x, y)");
    }

    #[test]
    fn build_agg_path_rect_unparseable_ry_falls_back_to_rx() {
        // `ry` uses `parse().unwrap_or(rx)`, so an unparseable ry inherits rx
        // rather than collapsing to 0. Both variants must build a rounded rect.
        let with_junk_ry = build_agg_path(&el(
            "rect",
            &[
                ("width", "10"),
                ("height", "10"),
                ("rx", "3"),
                ("ry", "junk"),
            ],
        ))
        .expect("rounded rect");
        let rx_only = build_agg_path(&el(
            "rect",
            &[("width", "10"), ("height", "10"), ("rx", "3")],
        ))
        .expect("rounded rect");
        assert_eq!(with_junk_ry.total_vertices(), rx_only.total_vertices());
        assert!(
            with_junk_ry.total_vertices() > 6,
            "rounded corners add curve vertices"
        );
    }

    #[test]
    fn build_agg_path_rect_nan_and_huge_sizes_do_not_panic() {
        // attr_f64 sanitizes NaN -> 0, so a NaN size is caught by `w <= 0.0` -> None.
        assert!(build_agg_path(&el("rect", &[("width", "NaN"), ("height", "NaN")])).is_none());

        // Huge sizes clamp to the finite ceiling and build a valid (bounded) path.
        let p = build_agg_path(&el("rect", &[("width", "1e400"), ("height", "1e400")]))
            .expect("huge size clamps to a finite, buildable rect");
        assert!(p.total_vertices() > 0);

        let p = build_agg_path(&el("rect", &[("width", "1e300"), ("height", "1e300")]))
            .expect("huge size clamps to a finite, buildable rect");
        assert!(p.total_vertices() > 0);
    }

    #[test]
    fn build_agg_path_ellipse_non_positive_radii_are_none() {
        assert!(build_agg_path(&el("ellipse", &[])).is_none());
        assert!(
            build_agg_path(&el("ellipse", &[("rx", "5")])).is_none(),
            "ry defaults to 0"
        );
        assert!(
            build_agg_path(&el("ellipse", &[("ry", "5")])).is_none(),
            "rx defaults to 0"
        );
        assert!(build_agg_path(&el("ellipse", &[("rx", "-1"), ("ry", "5")])).is_none());
    }

    #[test]
    fn build_agg_path_ellipse_valid_minimal() {
        let p = build_agg_path(&el(
            "ellipse",
            &[("cx", "10"), ("cy", "20"), ("rx", "10"), ("ry", "5")],
        ))
        .expect("valid ellipse");
        // move_to(1) + 4*curve4(3) = 13, + close_polygon = 14
        assert_eq!(p.total_vertices(), 14);
        let mut x = 0.0;
        let mut y = 0.0;
        p.vertex_idx(0, &mut x, &mut y);
        assert_eq!((x, y), (10.0, 15.0), "starts at (cx, cy - ry)");
    }

    #[test]
    fn build_agg_path_line_always_builds_two_vertices() {
        // <line> has no validity guard at all: an attribute-less line still
        // produces a 2-vertex path (which the rasterizer then fills to nothing).
        let p = build_agg_path(&el("line", &[])).expect("line with no attrs");
        assert_eq!(p.total_vertices(), 2);
        assert_eq!((p.last_x(), p.last_y()), (0.0, 0.0));

        let p = build_agg_path(&el(
            "line",
            &[("x1", "1"), ("y1", "2"), ("x2", "3"), ("y2", "4")],
        ))
        .expect("valid line");
        assert_eq!(p.total_vertices(), 2);
        assert_eq!((p.last_x(), p.last_y()), (3.0, 4.0));
    }

    #[test]
    fn build_agg_path_polygon_needs_at_least_two_points() {
        assert!(
            build_agg_path(&el("polygon", &[])).is_none(),
            "no points attribute"
        );
        assert!(build_agg_path(&el("polygon", &[("points", "")])).is_none());
        assert!(
            build_agg_path(&el("polygon", &[("points", "0,0")])).is_none(),
            "2 numbers < 4"
        );
        assert!(
            build_agg_path(&el("polygon", &[("points", "0 0 1")])).is_none(),
            "3 numbers < 4"
        );
        assert!(build_agg_path(&el("polygon", &[("points", "a b c d")])).is_none());
    }

    #[test]
    fn build_agg_path_polygon_closes_but_polyline_does_not() {
        let pts = [("points", "0,0 10,0 10,10")];
        let poly = build_agg_path(&el("polygon", &pts)).expect("polygon");
        let line = build_agg_path(&el("polyline", &pts)).expect("polyline");
        // move_to + 2 line_to = 3; polygon adds a close_polygon vertex.
        assert_eq!(line.total_vertices(), 3);
        assert_eq!(poly.total_vertices(), 4);
        assert!(
            agg_rust::basics::is_end_poly(poly.last_command())
                && agg_rust::basics::is_closed(poly.last_command()),
            "<polygon> must be closed"
        );
        assert_eq!(
            line.last_command(),
            agg_rust::basics::PATH_CMD_LINE_TO,
            "<polyline> must stay open"
        );
    }

    #[test]
    fn build_agg_path_polygon_odd_coordinate_count_drops_the_tail() {
        // chunks_exact(2) silently discards the unpaired trailing number.
        let p = build_agg_path(&el("polygon", &[("points", "0 0 10 10 20")])).expect("5 numbers");
        assert_eq!(p.total_vertices(), 3, "move_to + 1 line_to + close");
    }

    #[test]
    fn build_agg_path_polygon_nan_points_do_not_panic() {
        let p = build_agg_path(&el("polygon", &[("points", "NaN NaN inf inf")]))
            .expect("4 numbers parse");
        assert_eq!(p.total_vertices(), 3);
    }

    #[test]
    fn build_agg_path_polygon_extremely_long_points_does_not_hang() {
        let pts = "1 2 ".repeat(100_000);
        let p = build_agg_path(&el("polygon", &[("points", pts.as_str())])).expect("200k numbers");
        assert_eq!(
            p.total_vertices(),
            100_001,
            "move_to + 99_999 line_to + close"
        );
    }

    // ==================================================================
    // svg_multi_polygon_to_path_storage
    // ==================================================================

    fn point(x: f32, y: f32) -> azul_css::props::basic::SvgPoint {
        azul_css::props::basic::SvgPoint { x, y }
    }

    fn ring(items: Vec<azul_core::svg::SvgPathElement>) -> azul_core::svg::SvgPath {
        azul_core::svg::SvgPath {
            items: azul_core::svg::SvgPathElementVec::from_vec(items),
        }
    }

    fn multi(rings: Vec<azul_core::svg::SvgPath>) -> azul_core::svg::SvgMultiPolygon {
        azul_core::svg::SvgMultiPolygon {
            rings: azul_core::svg::SvgPathVec::from_vec(rings),
        }
    }

    fn line_el(x1: f32, y1: f32, x2: f32, y2: f32) -> azul_core::svg::SvgPathElement {
        azul_core::svg::SvgPathElement::Line(azul_core::svg::SvgLine {
            start: point(x1, y1),
            end: point(x2, y2),
        })
    }

    #[test]
    fn svg_multi_polygon_to_path_storage_empty_is_empty() {
        let p = svg_multi_polygon_to_path_storage(&multi(Vec::new()));
        assert_eq!(p.total_vertices(), 0);
    }

    #[test]
    fn svg_multi_polygon_to_path_storage_empty_ring_emits_no_stray_close() {
        // close_polygon() on an empty path is a no-op (last_command is STOP), so
        // an item-less ring must not push a dangling END_POLY vertex.
        let p = svg_multi_polygon_to_path_storage(&multi(vec![ring(Vec::new())]));
        assert_eq!(p.total_vertices(), 0);
    }

    #[test]
    fn svg_multi_polygon_to_path_storage_line_ring() {
        let p = svg_multi_polygon_to_path_storage(&multi(vec![ring(vec![line_el(
            0.0, 0.0, 10.0, 10.0,
        )])]));
        // move_to(start) + line_to(end) + close_polygon
        assert_eq!(p.total_vertices(), 3);
        let mut x = 0.0;
        let mut y = 0.0;
        assert_eq!(
            p.vertex_idx(0, &mut x, &mut y),
            agg_rust::basics::PATH_CMD_MOVE_TO
        );
        assert_eq!((x, y), (0.0, 0.0), "the first vertex is the line start");
        p.vertex_idx(1, &mut x, &mut y);
        assert_eq!((x, y), (10.0, 10.0));
    }

    #[test]
    fn svg_multi_polygon_to_path_storage_quadratic_and_cubic_arity() {
        let quad = azul_core::svg::SvgPathElement::QuadraticCurve(
            azul_css::props::basic::SvgQuadraticCurve {
                start: point(0.0, 0.0),
                ctrl: point(5.0, 5.0),
                end: point(10.0, 0.0),
            },
        );
        let cubic =
            azul_core::svg::SvgPathElement::CubicCurve(azul_css::props::basic::SvgCubicCurve {
                start: point(0.0, 0.0),
                ctrl_1: point(3.0, 3.0),
                ctrl_2: point(7.0, 3.0),
                end: point(10.0, 0.0),
            });

        // move_to + curve3 (2 vertices) + close
        let p = svg_multi_polygon_to_path_storage(&multi(vec![ring(vec![quad])]));
        assert_eq!(p.total_vertices(), 4);

        // move_to + curve4 (3 vertices) + close
        let p = svg_multi_polygon_to_path_storage(&multi(vec![ring(vec![cubic])]));
        assert_eq!(p.total_vertices(), 5);
    }

    #[test]
    fn svg_multi_polygon_to_path_storage_only_the_first_element_emits_a_move_to() {
        let p = svg_multi_polygon_to_path_storage(&multi(vec![ring(vec![
            line_el(0.0, 0.0, 1.0, 0.0),
            line_el(1.0, 0.0, 1.0, 1.0),
            line_el(1.0, 1.0, 0.0, 0.0),
        ])]));
        // 1 move_to + 3 line_to + 1 close
        assert_eq!(p.total_vertices(), 5);
        let mut x = 0.0;
        let mut y = 0.0;
        assert_eq!(
            p.vertex_idx(1, &mut x, &mut y),
            agg_rust::basics::PATH_CMD_LINE_TO,
            "the 2nd element must not restart the subpath"
        );
    }

    #[test]
    fn svg_multi_polygon_to_path_storage_multiple_rings_each_get_a_move_to() {
        let p = svg_multi_polygon_to_path_storage(&multi(vec![
            ring(vec![line_el(0.0, 0.0, 1.0, 1.0)]),
            ring(vec![line_el(5.0, 5.0, 6.0, 6.0)]),
        ]));
        assert_eq!(p.total_vertices(), 6, "3 vertices per ring");
        let mut x = 0.0;
        let mut y = 0.0;
        assert_eq!(
            p.vertex_idx(3, &mut x, &mut y),
            agg_rust::basics::PATH_CMD_MOVE_TO,
            "ring 2 restarts with a move_to"
        );
        assert_eq!((x, y), (5.0, 5.0));
    }

    #[test]
    fn svg_multi_polygon_to_path_storage_nan_and_infinite_coords_do_not_panic() {
        let p = svg_multi_polygon_to_path_storage(&multi(vec![ring(vec![line_el(
            f32::NAN,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        )])]));
        // total_vertices() is the real check: the extreme coords are stored, no panic,
        // no data loss. (last_x() is NOT usable here -- close_polygon() appends its own
        // bookkeeping vertex at (0,0), so last_x() reads that marker, not our coord.)
        assert_eq!(p.total_vertices(), 3);
    }

    #[test]
    fn svg_multi_polygon_to_path_storage_many_rings_does_not_hang() {
        let rings: Vec<_> = (0..20_000)
            .map(|i| {
                let f = i as f32;
                ring(vec![line_el(f, f, f + 1.0, f + 1.0)])
            })
            .collect();
        let p = svg_multi_polygon_to_path_storage(&multi(rings));
        assert_eq!(p.total_vertices(), 60_000);
    }

    // ==================================================================
    // render_svg_group / render_svg_group_with_style
    // ==================================================================

    #[test]
    fn render_svg_group_empty_node_paints_nothing() {
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&el("svg", &[]), &mut p, &TransAffine::new());
        assert!(is_all_white(&p), "no children -> untouched pixmap");
    }

    #[test]
    fn render_svg_group_text_children_are_skipped() {
        let mut node = el("svg", &[]);
        node.children = XmlNodeChildVec::from_vec(vec![XmlNodeChild::Text("hello".into())]);
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&node, &mut p, &TransAffine::new());
        assert!(is_all_white(&p));
    }

    #[test]
    fn render_svg_group_default_fill_is_black() {
        // A shape with no fill attribute anywhere inherits the SVG default: black.
        let svg = el_with(
            "svg",
            &[],
            vec![el(
                "rect",
                &[("x", "0"), ("y", "0"), ("width", "8"), ("height", "8")],
            )],
        );
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&svg, &mut p, &TransAffine::new());
        assert_eq!(px(&p, 4, 4), [0, 0, 0, 255]);
    }

    #[test]
    fn render_svg_group_fill_none_paints_nothing() {
        let svg = el_with(
            "svg",
            &[],
            vec![el(
                "rect",
                &[
                    ("x", "0"),
                    ("y", "0"),
                    ("width", "8"),
                    ("height", "8"),
                    ("fill", "none"),
                ],
            )],
        );
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&svg, &mut p, &TransAffine::new());
        assert!(is_all_white(&p), "fill=none must not paint");
    }

    #[test]
    fn render_svg_group_unparseable_fill_paints_nothing() {
        // parse_svg_color returns None for an unknown paint (e.g. url(#grad)),
        // which is treated exactly like fill="none" — silently no fill.
        let svg = el_with(
            "svg",
            &[],
            vec![el(
                "rect",
                &[("width", "8"), ("height", "8"), ("fill", "url(#gradient)")],
            )],
        );
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&svg, &mut p, &TransAffine::new());
        assert!(is_all_white(&p));
    }

    #[test]
    fn render_svg_group_fill_is_inherited_from_the_parent_group() {
        let svg = el_with(
            "svg",
            &[],
            vec![el_with(
                "g",
                &[("fill", "red")],
                vec![el("rect", &[("width", "8"), ("height", "8")])],
            )],
        );
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&svg, &mut p, &TransAffine::new());
        assert_eq!(
            px(&p, 4, 4),
            [255, 0, 0, 255],
            "<g fill> must cascade to <rect>"
        );
    }

    #[test]
    fn render_svg_group_element_fill_overrides_the_group_fill() {
        let svg = el_with(
            "svg",
            &[],
            vec![el_with(
                "g",
                &[("fill", "red")],
                vec![el(
                    "rect",
                    &[("width", "8"), ("height", "8"), ("fill", "blue")],
                )],
            )],
        );
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&svg, &mut p, &TransAffine::new());
        assert_eq!(px(&p, 4, 4), [0, 0, 255, 255]);
    }

    #[test]
    fn render_svg_group_group_transform_composes_with_the_element_transform() {
        // <g translate(4,0)> <rect translate(0,4) w=4 h=4> lands at (4, 4).
        let svg = el_with(
            "svg",
            &[],
            vec![el_with(
                "g",
                &[("transform", "translate(4,0)")],
                vec![el(
                    "rect",
                    &[
                        ("width", "4"),
                        ("height", "4"),
                        ("fill", "red"),
                        ("transform", "translate(0,4)"),
                    ],
                )],
            )],
        );
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&svg, &mut p, &TransAffine::new());
        assert_eq!(
            px(&p, 6, 6),
            [255, 0, 0, 255],
            "the bottom-right quadrant is filled"
        );
        assert_eq!(
            px(&p, 1, 1),
            [255, 255, 255, 255],
            "the top-left stays untouched"
        );
    }

    #[test]
    fn render_svg_group_defs_children_are_painted() {
        // SPEC GAP: the `_ =>` arm recurses into *any* unknown container, so
        // <defs> / <symbol> / <clipPath> content is rasterised even though the
        // SVG spec says those are definitions and must never be painted directly.
        // Characterization — see the report.
        let svg = el_with(
            "svg",
            &[],
            vec![el_with(
                "defs",
                &[],
                vec![el(
                    "rect",
                    &[("width", "8"), ("height", "8"), ("fill", "red")],
                )],
            )],
        );
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&svg, &mut p, &TransAffine::new());
        assert_eq!(
            px(&p, 4, 4),
            [255, 0, 0, 255],
            "<defs> content is painted (the spec says it must not be)"
        );
    }

    #[test]
    fn render_svg_group_opacity_greater_than_one_saturates_to_opaque() {
        let svg = el_with(
            "svg",
            &[],
            vec![el(
                "rect",
                &[
                    ("width", "8"),
                    ("height", "8"),
                    ("fill", "red"),
                    ("fill-opacity", "1000"),
                ],
            )],
        );
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&svg, &mut p, &TransAffine::new());
        assert_eq!(
            px(&p, 4, 4),
            [255, 0, 0, 255],
            "alpha clamps at 255, it must not wrap"
        );
    }

    #[test]
    fn render_svg_group_negative_opacity_becomes_transparent_not_wrapped() {
        // 255 * -1 = -255; `.min(255.0) as u8` is a *saturating* cast in Rust, so
        // it lands on 0 (fully transparent) rather than wrapping to 1.
        let svg = el_with(
            "svg",
            &[],
            vec![el(
                "rect",
                &[
                    ("width", "8"),
                    ("height", "8"),
                    ("fill", "red"),
                    ("fill-opacity", "-1"),
                ],
            )],
        );
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&svg, &mut p, &TransAffine::new());
        assert!(is_all_white(&p), "negative opacity must not paint");
    }

    #[test]
    fn render_svg_group_nan_and_garbage_opacity_do_not_panic() {
        for op in ["NaN", "inf", "-inf", "junk", "", "1e400"] {
            let svg = el_with(
                "svg",
                &[],
                vec![el(
                    "rect",
                    &[
                        ("width", "8"),
                        ("height", "8"),
                        ("fill", "red"),
                        ("fill-opacity", op),
                        ("opacity", op),
                    ],
                )],
            );
            let mut p = pixmap(8, 8);
            p.fill(255, 255, 255, 255);
            render_svg_group(&svg, &mut p, &TransAffine::new());
        }
    }

    #[test]
    fn render_svg_group_missing_stroke_paints_nothing() {
        // Unlike fill, a missing stroke means "no stroke" (not black), so a bare
        // <line> paints nothing at all.
        let svg = el_with(
            "svg",
            &[],
            vec![el(
                "line",
                &[("x1", "0"), ("y1", "4"), ("x2", "8"), ("y2", "4")],
            )],
        );
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&svg, &mut p, &TransAffine::new());
        assert!(is_all_white(&p), "no stroke attribute -> nothing painted");
    }

    #[test]
    fn render_svg_group_stroke_paints_a_line() {
        let svg = el_with(
            "svg",
            &[],
            vec![el(
                "line",
                &[
                    ("x1", "0"),
                    ("y1", "4"),
                    ("x2", "8"),
                    ("y2", "4"),
                    ("stroke", "red"),
                    ("stroke-width", "2"),
                ],
            )],
        );
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&svg, &mut p, &TransAffine::new());
        assert!(
            !is_all_white(&p),
            "a stroked line across the middle must paint something"
        );
    }

    #[test]
    fn render_svg_group_degenerate_stroke_widths_do_not_panic_or_hang() {
        for w in ["0", "-5", "NaN", "inf", "1e400", "junk"] {
            let svg = el_with(
                "svg",
                &[],
                vec![el(
                    "line",
                    &[
                        ("x1", "0"),
                        ("y1", "4"),
                        ("x2", "8"),
                        ("y2", "4"),
                        ("stroke", "black"),
                        ("stroke-width", w),
                    ],
                )],
            );
            let mut p = pixmap(8, 8);
            p.fill(255, 255, 255, 255);
            render_svg_group(&svg, &mut p, &TransAffine::new());
        }
    }

    #[test]
    fn render_svg_group_nan_transform_does_not_panic() {
        // A NaN transform maps every vertex to NaN. The contract asserted here is
        // only that the rasterizer survives it (whatever it chooses to paint) —
        // `f64 as i32` saturates rather than trapping, so this must not panic.
        let svg = el_with(
            "svg",
            &[],
            vec![el(
                "rect",
                &[
                    ("width", "8"),
                    ("height", "8"),
                    ("fill", "red"),
                    ("transform", "scale(NaN)"),
                ],
            )],
        );
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&svg, &mut p, &TransAffine::new());
        assert_eq!(p.data().len(), 8 * 8 * 4, "the pixmap must stay intact");
    }

    #[test]
    fn render_svg_group_infinite_transform_does_not_panic() {
        let svg = el_with(
            "svg",
            &[],
            vec![el(
                "circle",
                &[
                    ("cx", "4"),
                    ("cy", "4"),
                    ("r", "2"),
                    ("fill", "red"),
                    ("transform", "scale(1e400)"),
                ],
            )],
        );
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&svg, &mut p, &TransAffine::new());
    }

    #[test]
    fn render_svg_group_with_style_explicit_parent_style_is_used() {
        let style = SvgInheritedStyle {
            fill: Some("red".to_string()),
            stroke: None,
            stroke_width: None,
        };
        let svg = el_with(
            "svg",
            &[],
            vec![el("rect", &[("width", "8"), ("height", "8")])],
        );
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group_with_style(&svg, &mut p, &TransAffine::new(), &style);
        assert_eq!(px(&p, 4, 4), [255, 0, 0, 255], "the passed-in fill wins");
    }

    #[test]
    fn render_svg_group_with_style_on_a_1x1_pixmap_does_not_panic() {
        let svg = el_with(
            "svg",
            &[],
            vec![el(
                "rect",
                &[("width", "1000"), ("height", "1000"), ("fill", "red")],
            )],
        );
        let mut p = pixmap(1, 1);
        render_svg_group_with_style(
            &svg,
            &mut p,
            &TransAffine::new(),
            &SvgInheritedStyle::default(),
        );
        assert_eq!(px(&p, 0, 0), [255, 0, 0, 255]);
    }

    #[test]
    fn render_svg_group_deep_nesting_does_not_stack_overflow() {
        // render_svg_group_with_style recurses once per nesting level. Run on a
        // 128 MiB stack so that a genuinely linear-depth recursion is proven
        // safe instead of coin-flipping on the 2 MiB default test stack.
        let child = std::thread::Builder::new()
            .stack_size(128 * 1024 * 1024)
            .spawn(|| {
                const DEPTH: usize = 4_000;
                let mut node = el("rect", &[("width", "8"), ("height", "8"), ("fill", "red")]);
                for _ in 0..DEPTH {
                    node = el_with("g", &[], vec![node]);
                }
                let svg = el_with("svg", &[], vec![node]);
                let mut p = pixmap(8, 8);
                p.fill(255, 255, 255, 255);
                render_svg_group(&svg, &mut p, &TransAffine::new());
                px(&p, 4, 4)
            })
            .expect("spawn");
        assert_eq!(
            child
                .join()
                .expect("4000-deep <g> nesting must not overflow"),
            [255, 0, 0, 255]
        );
    }

    #[test]
    fn render_svg_group_many_siblings_does_not_hang() {
        let children: Vec<_> = (0..5_000)
            .map(|i| {
                el(
                    "rect",
                    &[
                        ("x", "0"),
                        ("y", "0"),
                        ("width", "8"),
                        ("height", "8"),
                        ("fill", if i % 2 == 0 { "red" } else { "blue" }),
                    ],
                )
            })
            .collect();
        let svg = el_with("svg", &[], children);
        let mut p = pixmap(8, 8);
        p.fill(255, 255, 255, 255);
        render_svg_group(&svg, &mut p, &TransAffine::new());
        assert_eq!(px(&p, 4, 4), [0, 0, 255, 255], "the last sibling wins");
    }

    // ==================================================================
    // render_svg_to_png  (parser, public)
    // ==================================================================

    const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G'];

    #[test]
    fn render_svg_to_png_valid_minimal() {
        let png = render_svg_to_png(MINIMAL_SVG, 16, 16).expect("positive control must render");
        assert!(png.starts_with(PNG_MAGIC), "the output must be a real PNG");
    }

    #[test]
    fn render_svg_to_png_round_trips_through_decode_png() {
        let png = render_svg_to_png(MINIMAL_SVG, 16, 16).expect("render");
        let decoded = AzulPixmap::decode_png(&png).expect("our own PNG must decode");
        assert_eq!((decoded.width(), decoded.height()), (16, 16));
        let [r, g, b, a] = px(&decoded, 8, 8);
        assert!(
            r > 200 && g < 60 && b < 60 && a == 255,
            "the red rect must survive the encode/decode round-trip, got {:?}",
            [r, g, b, a]
        );
    }

    #[test]
    fn render_svg_to_png_empty_and_whitespace_input_is_err() {
        assert!(render_svg_to_png(b"", 16, 16).is_err());
        assert!(render_svg_to_png(b"   ", 16, 16).is_err());
        assert!(render_svg_to_png(b"\t\n\r ", 16, 16).is_err());
    }

    #[test]
    fn render_svg_to_png_invalid_utf8_is_err_not_panic() {
        let err = render_svg_to_png(&[0xFF, 0xFE, 0x00], 16, 16).expect_err("invalid UTF-8");
        assert!(err.contains("UTF-8"), "expected a UTF-8 error, got: {err}");

        assert!(
            render_svg_to_png(&[0x80], 16, 16).is_err(),
            "lone continuation byte"
        );
        assert!(
            render_svg_to_png(&[0xED, 0xA0, 0x80], 16, 16).is_err(),
            "encoded surrogate"
        );
    }

    #[test]
    fn render_svg_to_png_garbage_is_err_never_panics() {
        for data in [
            &b"garbage"[..],
            &b"<<<<<<"[..],
            &b"<svg"[..],
            &b"</svg>"[..],
            &b"\x00\x01\x02\x03"[..],
            &b"{\"json\": true}"[..],
        ] {
            assert!(
                render_svg_to_png(data, 16, 16).is_err(),
                "{data:?} must not render"
            );
        }
    }

    #[test]
    fn render_svg_to_png_without_an_svg_root_is_err() {
        let err = render_svg_to_png(b"<html><body></body></html>", 16, 16).expect_err("no <svg>");
        assert!(err.contains("No <svg> root"), "got: {err}");
    }

    #[test]
    fn render_svg_to_png_root_tag_is_case_insensitive() {
        let png = render_svg_to_png(br#"<SVG viewBox="0 0 8 8"></SVG>"#, 8, 8)
            .expect("<SVG> must be recognised");
        assert!(png.starts_with(PNG_MAGIC));
    }

    #[test]
    fn render_svg_to_png_zero_target_dimensions_are_err_not_panic() {
        let err = render_svg_to_png(MINIMAL_SVG, 0, 16).expect_err("0 width");
        assert!(err.contains("pixmap"), "got: {err}");
        assert!(render_svg_to_png(MINIMAL_SVG, 16, 0).is_err());
        assert!(render_svg_to_png(MINIMAL_SVG, 0, 0).is_err());
    }

    #[test]
    fn render_svg_to_png_one_by_one_target() {
        let png = render_svg_to_png(MINIMAL_SVG, 1, 1).expect("1x1 must render");
        let decoded = AzulPixmap::decode_png(&png).expect("decode");
        assert_eq!((decoded.width(), decoded.height()), (1, 1));
    }

    #[test]
    fn render_svg_to_png_missing_viewbox_falls_back_to_the_target_size() {
        // Without a viewBox the scale is target/target == 1, so the rect maps 1:1.
        let png = render_svg_to_png(
            br#"<svg><rect x="0" y="0" width="16" height="16" fill="red"/></svg>"#,
            16,
            16,
        )
        .expect("render");
        let decoded = AzulPixmap::decode_png(&png).expect("decode");
        let [r, g, b, _] = px(&decoded, 8, 8);
        assert!(r > 200 && g < 60 && b < 60, "got {:?}", [r, g, b]);
    }

    #[test]
    fn render_svg_to_png_zero_area_viewbox_divides_by_zero_without_panicking() {
        // vb_w == 0 -> sx == inf -> scale == inf. Must degrade, not crash.
        let png = render_svg_to_png(
            br#"<svg viewBox="0 0 0 0"><rect width="8" height="8" fill="red"/></svg>"#,
            8,
            8,
        )
        .expect("must still produce a PNG");
        assert!(png.starts_with(PNG_MAGIC));
    }

    #[test]
    fn render_svg_to_png_nan_viewbox_does_not_panic() {
        let png = render_svg_to_png(
            br#"<svg viewBox="NaN NaN NaN NaN"><rect width="8" height="8" fill="red"/></svg>"#,
            8,
            8,
        )
        .expect("must still produce a PNG");
        assert!(png.starts_with(PNG_MAGIC));
    }

    #[test]
    fn render_svg_to_png_boundary_numeric_attributes_do_not_panic() {
        for svg in [
            &br#"<svg viewBox="0 0 8 8"><rect width="1e400" height="1e400" fill="red"/></svg>"#[..],
            &br#"<svg viewBox="0 0 8 8"><rect width="NaN" height="NaN" fill="red"/></svg>"#[..],
            &br#"<svg viewBox="0 0 8 8"><circle cx="0" cy="0" r="1e308" fill="red"/></svg>"#[..],
            &br#"<svg viewBox="0 0 8 8"><rect x="-0" y="-0" width="8" height="8" fill="red"/></svg>"#[..],
            &br#"<svg viewBox="1e-400 0 8 8"><rect width="8" height="8" fill="red"/></svg>"#[..],
            &br#"<svg viewBox="0 0 8 8"><line x1="-1e300" y1="-1e300" x2="1e300" y2="1e300" stroke="red"/></svg>"#[..],
        ] {
            let out = render_svg_to_png(svg, 8, 8);
            assert!(out.is_ok(), "{}", String::from_utf8_lossy(svg));
        }
    }

    #[test]
    fn render_svg_to_png_unicode_content_does_not_panic() {
        let svg = "<svg viewBox=\"0 0 8 8\"><title>\u{1F600} \u{4F60}\u{597D} e\u{301}</title>\
                   <rect width=\"8\" height=\"8\" fill=\"red\"/></svg>";
        let png = render_svg_to_png(svg.as_bytes(), 8, 8).expect("unicode text must not break");
        assert!(png.starts_with(PNG_MAGIC));
    }

    #[test]
    fn render_svg_to_png_extremely_long_input_does_not_hang() {
        // ~1 MB of text content inside an element the renderer never draws.
        let svg = format!(
            "<svg viewBox=\"0 0 8 8\"><desc>{}</desc><rect width=\"8\" height=\"8\" \
             fill=\"red\"/></svg>",
            "a".repeat(1_000_000)
        );
        let png = render_svg_to_png(svg.as_bytes(), 8, 8).expect("render");
        assert!(png.starts_with(PNG_MAGIC));
    }

    #[test]
    fn render_svg_to_png_deeply_nested_groups_do_not_stack_overflow() {
        // The XML tokenizer is iterative, but render_svg_group_with_style is not.
        // Give it a 128 MiB stack so a real 4000-deep document is a clean test.
        let out = std::thread::Builder::new()
            .stack_size(128 * 1024 * 1024)
            .spawn(|| {
                const DEPTH: usize = 4_000;
                let svg = format!(
                    "<svg viewBox=\"0 0 8 8\">{}<rect width=\"8\" height=\"8\" \
                     fill=\"red\"/>{}</svg>",
                    "<g>".repeat(DEPTH),
                    "</g>".repeat(DEPTH)
                );
                render_svg_to_png(svg.as_bytes(), 8, 8).is_ok()
            })
            .expect("spawn")
            .join()
            .expect("4000-deep nesting must not overflow the stack");
        assert!(out);
    }

    // ==================================================================
    // render_svg_to_imageref  (parser, public)
    // ==================================================================

    #[test]
    fn render_svg_to_imageref_valid_minimal() {
        let img = render_svg_to_imageref(MINIMAL_SVG, 16, 16).expect("positive control");
        let size = img.get_size();
        assert_eq!((size.width as u32, size.height as u32), (16, 16));
    }

    #[test]
    fn render_svg_to_imageref_empty_and_garbage_input_is_err() {
        assert!(render_svg_to_imageref(b"", 16, 16).is_err());
        assert!(render_svg_to_imageref(b"   ", 16, 16).is_err());
        assert!(render_svg_to_imageref(b"garbage", 16, 16).is_err());
        assert!(render_svg_to_imageref(b"<html></html>", 16, 16).is_err());
    }

    #[test]
    fn render_svg_to_imageref_invalid_utf8_is_err_not_panic() {
        let err = render_svg_to_imageref(&[0xFF, 0xFE, 0x00], 16, 16).expect_err("invalid UTF-8");
        assert!(err.contains("UTF-8"), "got: {err}");
    }

    #[test]
    fn render_svg_to_imageref_zero_target_dimensions_are_err_not_panic() {
        assert!(render_svg_to_imageref(MINIMAL_SVG, 0, 16).is_err());
        assert!(render_svg_to_imageref(MINIMAL_SVG, 16, 0).is_err());
        assert!(render_svg_to_imageref(MINIMAL_SVG, 0, 0).is_err());
    }

    #[test]
    fn render_svg_to_imageref_non_square_target_keeps_the_requested_size() {
        // The scale is min(sx, sy) — an aspect-mismatched target must still
        // produce a pixmap of exactly the requested dimensions.
        let img = render_svg_to_imageref(MINIMAL_SVG, 32, 8).expect("render");
        let size = img.get_size();
        assert_eq!((size.width as u32, size.height as u32), (32, 8));
    }

    #[test]
    fn render_svg_to_imageref_degenerate_viewbox_does_not_panic() {
        let img = render_svg_to_imageref(
            br#"<svg viewBox="0 0 0 0"><rect width="8" height="8" fill="red"/></svg>"#,
            8,
            8,
        )
        .expect("a zero-area viewBox must still build an ImageRef");
        let size = img.get_size();
        assert_eq!((size.width as u32, size.height as u32), (8, 8));
    }
}

