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
        vb.unwrap_or((0.0, 0.0, f64::from(target_width), f64::from(target_height)));

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
        vb.unwrap_or((0.0, 0.0, f64::from(target_width), f64::from(target_height)));
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
    node.attributes
        .get_key(key)
        .and_then(|s| s.as_str().parse().ok())
        .unwrap_or(0.0)
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

