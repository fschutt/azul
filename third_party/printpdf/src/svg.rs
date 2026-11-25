//! SVG graphics as PDF XObjects. Requires the `svg` feature.
//!
//! PDF cannot draw SVG, so [`Svg::parse`] converts the SVG with the `svg2pdf` crate into a
//! one-page PDF and keeps that page as an [`ExternalXObject`]: a *form XObject* (a reusable
//! content stream, see [`xobject`](crate::xobject)) that carries its own resources, such as
//! gradients, patterns and images. The graphics stay vector. Text is drawn as outlines, so it is
//! not selectable.
//!
//! The form is scaled to a unit square like an image, and its size in pixels is its size in
//! points: it is made at 72 dpi, where 1 pixel is 1 pt. To paint it at its natural size, set
//! `dpi: Some(72.0)` in the [`XObjectTransform`](crate::XObjectTransform). The default of 300
//! dpi paints it at 24% of its size.
//!
//! # Examples
//!
//! ```
//! # #[cfg(feature = "svg")] {
//! use printpdf::*;
//!
//! let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
//!     <rect width="100" height="50" fill="red"/></svg>"#;
//! let xobject = Svg::parse(svg, &mut Vec::new()).unwrap();
//!
//! let mut doc = PdfDocument::new("SVG");
//! let id = doc.add_xobject(&xobject);
//! let op = Op::UseXobject {
//!     id,
//!     transform: XObjectTransform {
//!         dpi: Some(72.0), // paint at the natural size, 100 x 50 pt
//!         translate_x: Some(Pt(50.0)),
//!         translate_y: Some(Pt(700.0)),
//!         ..Default::default()
//!     },
//! };
//! let page = PdfPage::new(Mm(210.0), Mm(297.0), vec![op]);
//! let bytes = doc.with_pages(vec![page]).save(&PdfSaveOptions::default(), &mut Vec::new());
//! assert!(bytes.starts_with(b"%PDF"));
//! # }
//! ```

use std::collections::BTreeMap;

use svg2pdf::{usvg, ConversionOptions, PageOptions};

use crate::{units::Pt, xobject::ExternalXObject, DictItem, ExternalStream, PdfWarnMsg};

/// Recursion limit when inlining objects of the svg2pdf output. Its object graph is shallow;
/// the limit only guards against cyclic input.
const MAX_INLINE_DEPTH: usize = 64;

/// Entry point for SVG conversion: it has no fields, only [`Svg::parse`] and
/// [`Svg::parse_with_fonts`].
///
/// To paint the same SVG several times, register it once with
/// [`PdfDocument::add_xobject`](crate::PdfDocument::add_xobject) and use the id in each
/// [`Op::UseXobject`](crate::Op::UseXobject).
#[derive(Debug, Clone)]
pub struct Svg {}

impl Svg {
    /// Converts an SVG document (the text, not a file name) into a form XObject.
    ///
    /// The result has its size in pixels at 72 dpi (1 pixel is 1 pt), see the
    /// [module docs](crate::svg). On targets other than wasm, `<text>` uses the system fonts; to
    /// supply fonts use [`Svg::parse_with_fonts`]. `warnings` receives a message for each
    /// `<text>` element that was dropped because its font was not found.
    ///
    /// # Errors
    ///
    /// Returns a message when the SVG does not parse, the conversion fails or the result has no
    /// area.
    pub fn parse(
        svg_string: &str,
        warnings: &mut Vec<PdfWarnMsg>,
    ) -> Result<ExternalXObject, String> {
        Self::parse_with_fonts(svg_string, &BTreeMap::new(), warnings)
    }

    /// Like [`Svg::parse`], but `<text>` also resolves against the fonts in `fonts`. On wasm
    /// this is the only way to get text, as there are no system fonts.
    ///
    /// The values are font files (TTF, OTF or TTC bytes). The keys are informational: the
    /// family names are read from the font data. A font can be referenced by its family
    /// (`"Roboto"`), its full name (`"Roboto Medium"`) or its PostScript name
    /// (`"Roboto-Medium"`). A `<text>` without a `font-family` uses the first supplied font
    /// instead of `"Times New Roman"`.
    ///
    /// # Errors
    ///
    /// The same as [`Svg::parse`].
    pub fn parse_with_fonts(
        svg_string: &str,
        fonts: &BTreeMap<String, Vec<u8>>,
        warnings: &mut Vec<PdfWarnMsg>,
    ) -> Result<ExternalXObject, String> {
        // svg2pdf turns the SVG into a one-page PDF. Its page becomes the form XObject: the
        // content stream is carried over as it is, with the page's whole `/Resources` tree
        // (color spaces, graphics states, patterns, shadings, ...) inlined, because the content
        // names those resources.
        let mut options = usvg::Options::default();
        #[cfg(not(target_arch = "wasm32"))]
        options.fontdb_mut().load_system_fonts();
        let supplied_families = register_supplied_fonts(&mut options, fonts);

        // 72 dpi makes 1 SVG pixel 1 pt, so svg2pdf's dpi transform (a `cm` at the start of the
        // content) is the identity. This matters: svg2pdf writes pattern and shading matrices
        // in page space, but patterns do not follow `cm` (ISO 32000-1, 8.7.3.1). At any other
        // dpi gradients and tiling patterns are drawn at the wrong scale and offset.
        let dpi = 72.0;
        options.dpi = dpi;
        let tree = usvg::Tree::from_str(svg_string, &options)
            .map_err(|err| format!("usvg parse: {err}"))?;

        // usvg silently drops `<text>` whose font cannot be resolved; warn about it.
        let text_in_svg = count_svg_text_elements(svg_string);
        if text_in_svg > 0 {
            let text_in_tree = count_tree_text_nodes(tree.root());
            if text_in_tree < text_in_svg {
                warnings.push(PdfWarnMsg::warning(
                    0,
                    0,
                    format!(
                        "svg: {} of {} <text> element(s) were dropped because no matching font \
                         was found (default family: {:?}, families of supplied fonts: {:?}). \
                         Pass the font via Svg::parse_with_fonts and reference it by one of its \
                         family names.",
                        text_in_svg - text_in_tree,
                        text_in_svg,
                        options.font_family,
                        supplied_families,
                    ),
                ));
            }
        }

        let mut co = ConversionOptions::default();
        co.compress = false;
        co.embed_text = false; // text is converted to outlines

        let po = PageOptions { dpi };
        let pdf_bytes = svg2pdf::to_pdf(&tree, co, po)
            .map_err(|err| format!("convert svg tree to pdf: {err}"))?;

        let doc = lopdf::Document::load_mem(&pdf_bytes)
            .map_err(|err| format!("convert svg tree to pdf: parse pdf: {err}"))?;

        let page_id = doc
            .page_iter()
            .next()
            .ok_or_else(|| "convert svg tree to pdf: no page rendered".to_string())?;

        let content = doc.get_page_content(page_id);

        let page_dict = doc
            .get_dictionary(page_id)
            .map_err(|err| format!("convert svg tree to pdf: page is not a dictionary: {err}"))?;

        // svg2pdf writes a `[0 0 w h]` MediaBox; fall back to the size of the usvg tree.
        let media_box = get_media_box(&doc, page_id).unwrap_or_else(|| {
            warnings.push(PdfWarnMsg::warning(
                0,
                0,
                "svg: generated page has no readable MediaBox, falling back to usvg tree size"
                    .to_string(),
            ));
            let size = tree.size();
            let scale = 72.0 / dpi;
            [0.0, 0.0, size.width() * scale, size.height() * scale]
        });

        let width_pt = media_box[2] - media_box[0];
        let height_pt = media_box[3] - media_box[1];
        if !(width_pt > 0.0) || !(height_pt > 0.0) {
            return Err(format!(
                "convert svg tree to pdf: page has degenerate media box {media_box:?}"
            ));
        }

        // Streams stay `DictItem::Stream` here; saving the document writes them as indirect
        // objects again (ISO 32000-1, 7.3.8.1).
        let resources = match page_dict.get(b"Resources") {
            Ok(res) => match inline_lopdf_object(&doc, res, 0, warnings) {
                DictItem::Dict { map } => DictItem::Dict { map },
                other => {
                    warnings.push(PdfWarnMsg::warning(
                        0,
                        0,
                        format!("svg: page /Resources is not a dictionary ({other:?}), ignoring"),
                    ));
                    DictItem::Dict {
                        map: BTreeMap::new(),
                    }
                }
            },
            Err(_) => DictItem::Dict {
                map: BTreeMap::new(),
            },
        };

        // /Matrix maps the form to the unit square, so it is placed like an image.
        let sx = 1.0 / width_pt;
        let sy = 1.0 / height_pt;

        let mut dict: BTreeMap<String, DictItem> = [
            ("Type", DictItem::Name("XObject".into())),
            ("Subtype", DictItem::Name("Form".into())),
            (
                "ProcSet",
                DictItem::Array(vec![
                    DictItem::Name("PDF".into()),
                    DictItem::Name("Text".into()),
                    DictItem::Name("ImageC".into()),
                    DictItem::Name("ImageB".into()),
                ]),
            ),
            (
                "BBox",
                DictItem::Array(vec![
                    DictItem::Real(media_box[0]),
                    DictItem::Real(media_box[1]),
                    DictItem::Real(media_box[2]),
                    DictItem::Real(media_box[3]),
                ]),
            ),
            (
                "Matrix",
                DictItem::Array(vec![
                    DictItem::Real(sx),
                    DictItem::Real(0.0),
                    DictItem::Real(0.0),
                    DictItem::Real(sy),
                    DictItem::Real(-media_box[0] * sx),
                    DictItem::Real(-media_box[1] * sy),
                ]),
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();

        dict.insert("Resources".to_string(), resources);

        // The page is an isolated transparency group; keeping it keeps group opacity and soft
        // masks correct.
        if let Ok(group) = page_dict.get(b"Group") {
            if let DictItem::Dict { map } = inline_lopdf_object(&doc, group, 0, warnings) {
                dict.insert("Group".to_string(), DictItem::Dict { map });
            }
        }

        Ok(ExternalXObject {
            stream: ExternalStream {
                dict,
                content,
                compress: false,
            },
            width: Some(Pt(width_pt).into_px(dpi)),
            height: Some(Pt(height_pt).into_px(dpi)),
            dpi: Some(dpi),
        })
    }
}

/// Loads the supplied fonts into the font database of `options`. Returns the family names they
/// are registered under, for diagnostics.
///
/// Each face is also registered under its legacy family, full and PostScript names, because
/// fontdb only indexes the typographic family while SVGs often say `font-family="Roboto Medium"`.
/// The first supplied family becomes the default family of `<text>` without a `font-family`.
fn register_supplied_fonts(
    options: &mut usvg::Options,
    fonts: &BTreeMap<String, Vec<u8>>,
) -> Vec<String> {
    use svg2pdf::usvg::fontdb;

    let mut registered_families: Vec<String> = Vec::new();
    let mut default_family: Option<String> = None;

    for bytes in fonts.values() {
        let db = options.fontdb_mut();
        let faces_before = db.len();
        db.load_font_data(bytes.clone());

        let new_faces: Vec<fontdb::FaceInfo> = db.faces().skip(faces_before).cloned().collect();

        for face in &new_faces {
            for (name, _) in &face.families {
                if !registered_families.iter().any(|f| f == name) {
                    registered_families.push(name.clone());
                }
            }
        }

        // The name-table aliases are only attached when the data holds exactly one face; in a
        // collection they could belong to another face.
        if let [face] = new_faces.as_slice() {
            if default_family.is_none() {
                default_family = face.families.first().map(|(n, _)| n.clone());
            }
            let known: Vec<String> = face
                .families
                .iter()
                .map(|(n, _)| n.to_ascii_lowercase())
                .collect();
            for alias in collect_font_name_aliases(bytes) {
                if known.contains(&alias.to_ascii_lowercase())
                    || registered_families
                        .iter()
                        .any(|f| f.eq_ignore_ascii_case(&alias))
                {
                    continue;
                }
                registered_families.push(alias.clone());
                if default_family.is_none() {
                    default_family = Some(alias.clone());
                }
                options.fontdb_mut().push_face_info(fontdb::FaceInfo {
                    id: fontdb::ID::dummy(),
                    source: face.source.clone(),
                    index: face.index,
                    families: vec![(alias, fontdb::Language::English_UnitedStates)],
                    post_script_name: face.post_script_name.clone(),
                    style: face.style,
                    weight: face.weight,
                    stretch: face.stretch,
                    monospaced: face.monospaced,
                });
            }
        } else if default_family.is_none() {
            default_family = new_faces
                .first()
                .and_then(|f| f.families.first().map(|(n, _)| n.clone()));
        }
    }

    if let Some(family) = default_family {
        options.font_family = family;
    }

    registered_families
}

/// The legacy family (name ID 1), full name (ID 4) and PostScript name (ID 6) from the `name`
/// table of a TrueType or OpenType font. Empty on any malformed data: the names are only aliases.
fn collect_font_name_aliases(data: &[u8]) -> Vec<String> {
    // The offsets come from the font, so every sum is checked: on a 32-bit target (wasm) a
    // `u32` offset plus a small number overflows a `usize`.
    fn u16_at(d: &[u8], o: usize) -> Option<u16> {
        Some(u16::from_be_bytes([*d.get(o)?, *d.get(o.checked_add(1)?)?]))
    }
    fn u32_at(d: &[u8], o: usize) -> Option<u32> {
        Some(u32::from_be_bytes([
            *d.get(o)?,
            *d.get(o.checked_add(1)?)?,
            *d.get(o.checked_add(2)?)?,
            *d.get(o.checked_add(3)?)?,
        ]))
    }

    let mut out: Vec<String> = Vec::new();
    let mut push_unique = |s: String| {
        let s = s.trim().to_string();
        if !s.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&s)) {
            out.push(s);
        }
    };

    let mut parse = || -> Option<()> {
        // in a collection, use the first font
        let sfnt_start = if data.get(0..4)? == b"ttcf" {
            u32_at(data, 12)? as usize
        } else {
            0
        };

        let num_tables = u16_at(data, sfnt_start.checked_add(4)?)? as usize;
        let mut name_table = None;
        for i in 0..num_tables {
            let rec = sfnt_start.checked_add(12)?.checked_add(i.checked_mul(16)?)?;
            if data.get(rec..rec.checked_add(4)?)? == b"name" {
                name_table = Some(u32_at(data, rec.checked_add(8)?)? as usize);
                break;
            }
        }
        let name_table = name_table?;

        let count = u16_at(data, name_table.checked_add(2)?)? as usize;
        let string_storage =
            name_table.checked_add(u16_at(data, name_table.checked_add(4)?)? as usize)?;

        for i in 0..count {
            let rec = name_table.checked_add(6)?.checked_add(i.checked_mul(12)?)?;
            let platform = u16_at(data, rec)?;
            let name_id = u16_at(data, rec.checked_add(6)?)?;
            if !matches!(name_id, 1 | 4 | 6) {
                continue;
            }
            let len = u16_at(data, rec.checked_add(8)?)? as usize;
            let off = string_storage.checked_add(u16_at(data, rec.checked_add(10)?)? as usize)?;
            let bytes = data.get(off..off.checked_add(len)?)?;
            match platform {
                // Unicode / Windows: UTF-16 BE
                0 | 3 => {
                    let units: Vec<u16> = bytes
                        .as_chunks::<2>().0.iter()
                        .map(|c| u16::from_be_bytes([c[0], c[1]]))
                        .collect();
                    if let Ok(s) = String::from_utf16(&units) {
                        push_unique(s);
                    }
                }
                // Macintosh: Latin-1, which is right for the ASCII names found there
                1 => {
                    push_unique(bytes.iter().map(|&b| b as char).collect());
                }
                _ => {}
            }
        }
        Some(())
    };
    let _ = parse();

    out
}

/// Number of `<text>` elements in the SVG source. `<textPath>` only occurs inside `<text>` and is
/// not counted.
fn count_svg_text_elements(svg: &str) -> usize {
    let bytes = svg.as_bytes();
    svg.match_indices("<text")
        .filter(|(pos, tag)| {
            let after = pos.saturating_add(tag.len());
            !matches!(bytes.get(after), Some(c) if c.is_ascii_alphanumeric())
        })
        .count()
}

/// Number of text nodes that survived usvg parsing (it drops text whose font is not found).
fn count_tree_text_nodes(group: &usvg::Group) -> usize {
    let mut n = 0;
    for child in group.children() {
        match child {
            usvg::Node::Text(_) => n += 1,
            usvg::Node::Group(g) => n += count_tree_text_nodes(g),
            _ => {}
        }
    }
    n
}

/// Reads the (possibly inherited) `/MediaBox` of `page_id` as
/// `[min_x, min_y, max_x, max_y]`.
fn get_media_box(doc: &lopdf::Document, page_id: lopdf::ObjectId) -> Option<[f32; 4]> {
    let mut dict = doc.get_dictionary(page_id).ok()?;
    for _ in 0..MAX_INLINE_DEPTH {
        if let Ok(mb) = dict.get(b"MediaBox") {
            let mb = match mb {
                lopdf::Object::Reference(id) => doc.get_object(*id).ok()?,
                other => other,
            };
            let arr = mb.as_array().ok()?;
            if arr.len() != 4 {
                return None;
            }
            let mut vals = [0.0_f32; 4];
            for (val, obj) in vals.iter_mut().zip(arr.iter()) {
                *val = lopdf_number(obj)?;
            }
            return Some([
                vals[0].min(vals[2]),
                vals[1].min(vals[3]),
                vals[0].max(vals[2]),
                vals[1].max(vals[3]),
            ]);
        }
        // MediaBox is inheritable via the page tree
        let parent = dict.get(b"Parent").ok()?.as_reference().ok()?;
        dict = doc.get_dictionary(parent).ok()?;
    }
    None
}

fn lopdf_number(obj: &lopdf::Object) -> Option<f32> {
    match obj {
        lopdf::Object::Integer(i) => Some(*i as f32),
        lopdf::Object::Real(r) => Some(*r),
        _ => None,
    }
}

/// Converts an object of the svg2pdf output into a self-contained [`DictItem`], replacing every
/// indirect reference by the object it points to. A dangling reference becomes `Null` with a
/// warning.
fn inline_lopdf_object(
    doc: &lopdf::Document,
    obj: &lopdf::Object,
    depth: usize,
    warnings: &mut Vec<PdfWarnMsg>,
) -> DictItem {
    use lopdf::Object;

    if depth > MAX_INLINE_DEPTH {
        warnings.push(PdfWarnMsg::warning(
            0,
            0,
            "svg: resource tree exceeds maximum depth (cyclic reference?), truncating".to_string(),
        ));
        return DictItem::Null;
    }

    match obj {
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(resolved) => inline_lopdf_object(doc, resolved, depth + 1, warnings),
            Err(e) => {
                warnings.push(PdfWarnMsg::warning(
                    0,
                    0,
                    format!("svg: unresolvable reference {id:?} in svg2pdf output: {e}"),
                ));
                DictItem::Null
            }
        },
        Object::Array(items) => DictItem::Array(
            items
                .iter()
                .map(|o| inline_lopdf_object(doc, o, depth + 1, warnings))
                .collect(),
        ),
        Object::Dictionary(d) => DictItem::Dict {
            map: d
                .iter()
                .map(|(k, v)| {
                    (
                        String::from_utf8_lossy(k).to_string(),
                        inline_lopdf_object(doc, v, depth + 1, warnings),
                    )
                })
                .collect(),
        },
        Object::Stream(s) => {
            // Decoded content lets the saved file choose its own filters. Streams lopdf cannot
            // decode (such as DCTDecode images) keep their raw bytes and /Filter.
            let (content, strip_filters) = match s.decompressed_content() {
                Ok(c) => (c, true),
                Err(_) => (s.content.clone(), false),
            };
            let dict = s
                .dict
                .iter()
                .filter(|(k, _)| {
                    let k = k.as_slice();
                    // Length is recomputed on write
                    k != b"Length" && (!strip_filters || (k != b"Filter" && k != b"DecodeParms"))
                })
                .map(|(k, v)| {
                    (
                        String::from_utf8_lossy(k).to_string(),
                        inline_lopdf_object(doc, v, depth + 1, warnings),
                    )
                })
                .collect();
            DictItem::Stream {
                stream: ExternalStream {
                    dict,
                    content,
                    compress: false,
                },
            }
        }
        other => DictItem::from_lopdf(other),
    }
}
