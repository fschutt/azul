//! Reading a PDF file into a [`PdfDocument`].
//!
//! # How a PDF file is organised
//!
//! A PDF file starts with a header such as `%PDF-1.7`, followed by numbered *objects*: numbers,
//! names, strings, arrays, dictionaries and *streams* (a dictionary plus a block of bytes, usually
//! compressed). Objects refer to each other by number. A *cross-reference table* near the end of
//! the file lists where each object starts, and the *trailer* names the root object, the
//! *catalog*. The catalog leads to the page tree, and each page leads to a *content stream*: the
//! list of drawing operators of that page, written operands first (`q` saves the graphics state,
//! `Tf` selects a font, `re` adds a rectangle, `f` fills it, ...). Fonts, images and other things
//! a page uses are *resources*, which the content stream names.
//!
//! # What parsing produces
//!
//! The syntax level (objects, the cross-reference table, stream filters) is done by [`lopdf`].
//! This module turns the loaded objects into a [`PdfDocument`]:
//!
//! - **Document info** (title, author, subject, keywords, dates, ...) becomes
//!   `metadata.info`. Keywords are split at commas. The XMP metadata stream is not read.
//! - **Pages** become [`PdfPage`]s with their `MediaBox`, `TrimBox` and `CropBox`. The boxes are
//!   inherited from parent page-tree nodes, and `TrimBox` and `CropBox` default to the media
//!   box. A page without a usable media box gets A4 and a warning. `/Rotate` is not read.
//! - **Content streams** become [`Op`]s. If a page has several streams they are joined first.
//!   An operator without a matching `Op` becomes [`Op::Unknown`]; saving drops those unless
//!   [`PdfSaveOptions::secure`](crate::PdfSaveOptions::secure) is `false`.
//! - **Fonts** that are embedded and can be read become `resources.fonts`, keyed by the resource
//!   name (`F1`, ...). The 14 standard fonts need no entry. A font that is not embedded is
//!   replaced by the closest standard font, with a warning, and a composite (Type0) font whose
//!   program cannot be read keeps only the code layout and ToUnicode CMap needed to decode its
//!   text; neither is kept in `resources.fonts`. Reading the program of an embedded simple
//!   (TrueType or Type1) font needs the `text_layout` feature.
//! - **XObjects** (reusable pieces of content, referenced by `Do`): images are decoded to
//!   [`XObject::Image`], with an `/SMask` merged in as alpha. Form XObjects become
//!   [`XObject::Form`]: the operators of the form's content stream are read as [`Op`]s, and its
//!   own `/Resources` (fonts, images, nested forms, graphics states, shadings) become a scope
//!   of its own, read by the same functions as the resources of the pages. Text that a page
//!   draws inside a form (macOS Quartz, Word and print drivers do that for the whole page) is
//!   part of the document model like any other text. A form that cannot be modelled without
//!   loss, and images that cannot be decoded (JPEG and JPEG 2000 need the `images` feature), are
//!   kept byte for byte as [`XObject::External`], so that saving does not lose them.
//! - **Graphics state parameter dictionaries** (ExtGState), **shadings** (axial and radial
//!   gradients only), **layers** (optional content groups), **bookmarks** (the top level of the
//!   outline, entries with an explicit destination) and **link annotations** (GoTo and URI
//!   actions) are read into the matching parts of the document. Link annotations are appended to
//!   the page as [`Op::LinkAnnotation`].
//!
//! Resources are collected from the catalog, the page tree and every page, and merged into one
//! document-wide map. For fonts, XObjects and shadings the first definition of a resource name
//! wins; for ExtGState the last one does.
//!
//! # Warnings and errors
//!
//! Parsing is lenient. Whatever cannot be read is skipped, and a [`PdfWarnMsg`] with a
//! [`PdfParseErrorSeverity`] is pushed to the `warnings` list the caller passes in. The page
//! and operator the message is about are in [`PdfWarnMsg::page`] and [`PdfWarnMsg::op_id`].
//! Parsing returns `Err` only when the document cannot be walked at all: lopdf cannot load the
//! file, the trailer has no `/Root`, the catalog or its `/Pages` entry is missing or has the
//! wrong type, or a page-tree node has no `/Kids`. A page object that cannot be read leaves an
//! empty page.
//!
//! Input that asks for unreasonable amounts of memory is skipped with a warning: a stream that
//! decodes to more than 256 MiB, and the references inside preserved XObjects once copying them
//! has produced more than a fixed number of objects or bytes for the document (the rest become
//! `null`).
//!
//! # Examples
//!
//! Parse, edit and save again:
//!
//! ```
//! use printpdf::*;
//!
//! let mut warnings = Vec::new();
//! let mut doc = PdfDocument::new("Original");
//! doc.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), vec![]));
//! let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);
//!
//! let mut parsed = PdfDocument::parse(&bytes, &PdfParseOptions::default(), &mut warnings).unwrap();
//! assert_eq!(parsed.pages.len(), 1);
//! parsed.metadata.info.document_title = "Edited".to_string();
//! parsed.pages.push(PdfPage::new(Mm(100.0), Mm(100.0), vec![]));
//! let edited = parsed.save(&PdfSaveOptions::default(), &mut warnings);
//! assert!(edited.starts_with(b"%PDF-"));
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::{
    Dictionary as LopdfDictionary, Document as LopdfDocument, Object as LopdfObject, Object,
    ObjectId, Stream as LopdfStream,
};
use serde_derive::{Deserialize, Serialize};

mod form_xobject;
use form_xobject::{parse_form, PendingForm, XObjectCtx};

use crate::{
    cmap::ToUnicodeCMap,
    conformance::PdfConformance,
    date::{parse_pdf_date, OffsetDateTime},
    font::ParsedFont,
    xobject::{ExternalStream, ExternalXObject},
    BuiltinFont, BuiltinOrExternalFontId, Color, DictItem, ExtendedGraphicsState,
    ExtendedGraphicsStateId, ExtendedGraphicsStateMap, FontId, LayerInternalId, Line,
    LineDashPattern, LinePoint, LinkAnnotation, Op, PageAnnotId, PageAnnotMap, PaintMode,
    PdfDocument, PdfDocumentInfo, PdfFontMap, PdfLayerMap, PdfMetadata, PdfPage, PdfResources,
    Point, Polygon, PolygonRing, Pt, RawImage, Rect, RenderingIntent, TextMatrix,
    TextRenderingMode, WindingOrder, XObject, XObjectId, XObjectMap,
};

/// Options for [`PdfDocument::parse`](crate::PdfDocument::parse).
///
/// Serialized to JSON with camelCase field names.
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfParseOptions {
    /// Currently has no effect: parsing never reads this field, and problems are reported as
    /// [`PdfWarnMsg`]s whatever its value.
    #[serde(default)]
    pub fail_on_error: bool,
}

/// One message about something the parser or serializer could not read, write or understand.
///
/// Serialized to JSON with camelCase field names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfWarnMsg {
    /// Zero-based index of the page the message is about. Messages about the document as a whole
    /// carry 0, and so does every message raised while saving.
    pub page: usize,
    /// Index of the content stream operator (when parsing) or [`Op`] (when saving) the message
    /// is about. 0 when it is not about a single operator.
    pub op_id: usize,
    /// How serious the problem is.
    pub severity: PdfParseErrorSeverity,
    /// Human-readable description.
    pub msg: String,
}

/// How serious a [`PdfWarnMsg`] is. Serialized to JSON in kebab-case (`"error"`, `"info"`,
/// `"warning"`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PdfParseErrorSeverity {
    /// Something was lost or could not be processed.
    Error,
    /// Progress or background information.
    Info,
    /// Something unusual was found; the parser used a fallback.
    Warning,
}

impl PdfWarnMsg {
    /// A message with [`PdfParseErrorSeverity::Error`].
    pub fn error(page: usize, op_id: usize, e: String) -> Self {
        PdfWarnMsg {
            page,
            op_id,
            severity: PdfParseErrorSeverity::Error,
            msg: e,
        }
    }

    /// A message with [`PdfParseErrorSeverity::Warning`].
    pub fn warning(page: usize, op_id: usize, msg: String) -> Self {
        PdfWarnMsg {
            page,
            op_id,
            severity: PdfParseErrorSeverity::Warning,
            msg,
        }
    }

    /// A message with [`PdfParseErrorSeverity::Info`].
    pub fn info(page: usize, op_id: usize, msg: String) -> Self {
        PdfWarnMsg {
            page,
            op_id,
            severity: PdfParseErrorSeverity::Info,
            msg,
        }
    }
}

struct InitialPdf {
    doc: lopdf::Document,
    objs_to_search_for_resources: Vec<(Object, Option<usize>)>,
    page_refs: Vec<(u32, u16)>,
    document_info: PdfDocumentInfo,
    /// Link annotations with the 0-based index of the page they belong to.
    link_annots: Vec<(usize, LinkAnnotation)>,
}

fn parse_pdf_from_bytes_start(
    bytes: &[u8],
    _opts: &PdfParseOptions,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Result<InitialPdf, String> {
    let doc = LopdfDocument::load_mem(bytes).map_err(|e| format!("Failed to load PDF: {}", e))?;

    let mut objs_to_search_for_resources = Vec::new();

    let root_obj = doc
        .trailer
        .get(b"Root")
        .map_err(|_| "Missing Root in trailer".to_string())?;

    objs_to_search_for_resources.push((root_obj.clone(), None));

    let root_ref = match root_obj {
        Object::Reference(r) => *r,
        _ => return Err("Invalid Root reference".to_string()),
    };

    let catalog_obj = doc
        .get_object(root_ref)
        .map_err(|e| format!("Failed to get catalog: {}", e))?;

    objs_to_search_for_resources.push((catalog_obj.clone(), None));

    let catalog = catalog_obj
        .as_dict()
        .map_err(|e| format!("Catalog is not a dictionary: {}", e))?;

    if let Ok(resources) = catalog.get(b"Resources") {
        objs_to_search_for_resources.push((resources.clone(), None));
    }

    let pages_obj = catalog
        .get(b"Pages")
        .map_err(|e| format!("Missing Pages key in catalog: {}", e))?;

    objs_to_search_for_resources.push((pages_obj.clone(), None));

    // /Pages is normally an indirect reference; an inline dictionary is accepted too.
    let pages_dict = match pages_obj {
        Object::Reference(r) => doc
            .get_object(*r)
            .map_err(|e| format!("Failed to get Pages object: {}", e))?
            .as_dict()
            .map_err(|e| format!("Pages object is not a dictionary: {}", e))?,
        Object::Dictionary(d) => {
            warnings.push(PdfWarnMsg::warning(
                0,
                0,
                "Catalog /Pages is an inline dictionary instead of a reference".to_string(),
            ));
            d
        }
        _ => return Err("Pages key is not a reference".to_string()),
    };

    if let Ok(resources) = pages_dict.get(b"Resources") {
        objs_to_search_for_resources.push((resources.clone(), None));
    }

    let page_refs = collect_page_refs(pages_dict, &doc, warnings)?;

    let page_objects = page_refs
        .iter()
        .enumerate()
        .filter_map(|(i, &page_ref)| {
            doc.get_object(page_ref)
                .ok()
                .map(|obj| (obj.clone(), Some(i)))
        })
        .collect::<Vec<_>>();

    objs_to_search_for_resources.extend(page_objects);

    let document_info = doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|s| get_dict_or_resolve_ref("document_info", &doc, s, warnings, None))
        .map(parse_document_info)
        .unwrap_or_default();

    let link_annots = links_and_bookmarks::extract_link_annotations(&doc);

    Ok(InitialPdf {
        doc,
        objs_to_search_for_resources,
        page_refs,
        document_info,
        link_annots,
    })
}

/// Parses a PDF file into a [`PdfDocument`]. This is what [`PdfDocument::parse`] calls; the
/// module documentation describes what is read and what is skipped.
///
/// Problems that do not stop parsing are pushed to `warnings`. `Err` carries a description of
/// the failure when the file cannot be read at all.
pub fn parse_pdf_from_bytes(
    bytes: &[u8],
    opts: &PdfParseOptions,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Result<PdfDocument, String> {
    let i = parse_pdf_from_bytes_start(bytes, opts, warnings)?;

    let fonts = parse_fonts_from_entire_pdf(&i.doc, &i.objs_to_search_for_resources, warnings);
    let font_decode = font_decode_map(&fonts);
    let xobjects = parse_xobjects_from_entire_pdf(
        &i.doc,
        &i.objs_to_search_for_resources,
        &font_decode,
        warnings,
    );
    let xobjects = process_xobjects(xobjects, warnings);

    parse_pdf_from_bytes_end(i, opts, fonts, font_decode, xobjects, warnings)
}

/// Like [`parse_pdf_from_bytes`], but decodes images with [`RawImage::decode_from_bytes_async`].
pub async fn parse_pdf_from_bytes_async(
    bytes: &[u8],
    opts: &PdfParseOptions,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Result<PdfDocument, String> {
    let i = parse_pdf_from_bytes_start(bytes, opts, warnings)?;

    let fonts = parse_fonts_from_entire_pdf(&i.doc, &i.objs_to_search_for_resources, warnings);
    let font_decode = font_decode_map(&fonts);
    let xobjects = parse_xobjects_from_entire_pdf(
        &i.doc,
        &i.objs_to_search_for_resources,
        &font_decode,
        warnings,
    );
    let xobjects = process_xobjects_async(xobjects, warnings).await;

    parse_pdf_from_bytes_end(i, opts, fonts, font_decode, xobjects, warnings)
}

/// How the text of each font resource name decodes; `Tf` operators select by that name.
///
/// Builtin entries are keyed by enum, not by resource name; `Tf` falls back to
/// [`BuiltinFont::from_id`] for the F1-F14 names.
fn font_decode_map(
    fonts: &BTreeMap<BuiltinOrExternalFontId, ParsedOrBuiltinFont>,
) -> BTreeMap<String, FontTextDecodeKind> {
    fonts
        .iter()
        .filter_map(|(id, pf)| match id {
            BuiltinOrExternalFontId::External(fid) => Some((fid.0.clone(), pf.decode_kind())),
            BuiltinOrExternalFontId::Builtin(_) => None,
        })
        .collect()
}

fn parse_pdf_from_bytes_end(
    initial_pdf: InitialPdf,
    _opts: &PdfParseOptions,
    fonts: BTreeMap<BuiltinOrExternalFontId, ParsedOrBuiltinFont>,
    font_decode: BTreeMap<String, FontTextDecodeKind>,
    xobjects: BTreeMap<XObjectId, XObject>,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Result<PdfDocument, String> {
    let mut pages = Vec::new();

    let InitialPdf {
        doc,
        objs_to_search_for_resources,
        page_refs,
        document_info,
        link_annots,
    } = initial_pdf;

    for (i, page_ref) in page_refs.into_iter().enumerate() {
        let parsed = doc
            .get_object(page_ref)
            .map_err(|e| format!("Failed to get page object: {}", e))
            .and_then(|obj| {
                obj.as_dict()
                    .map_err(|e| format!("Page object is not a dictionary: {}", e))
            })
            .and_then(|page_obj| parse_page(i, page_obj, &doc, &xobjects, &font_decode, warnings));

        // A page that cannot be read stays in the list as an empty page: the page numbers of
        // the bookmarks and links refer to the position in the list.
        let pdf_page = match parsed {
            Ok(page) => page,
            Err(e) => {
                warnings.push(PdfWarnMsg::error(
                    i,
                    0,
                    format!("Page {} could not be read, it is left empty: {e}", i + 1),
                ));
                PdfPage {
                    media_box: default_media_box(),
                    trim_box: default_media_box(),
                    crop_box: default_media_box(),
                    ops: Vec::new(),
                }
            }
        };

        pages.push(pdf_page);
    }

    // The serializer builds /Annots from `Op::LinkAnnotation`, so links go back on their pages as ops.
    for (page_idx, link) in link_annots {
        match pages.get_mut(page_idx) {
            Some(page) => page.ops.push(Op::LinkAnnotation { link }),
            None => warnings.push(PdfWarnMsg::warning(
                page_idx,
                0,
                "link annotation references a page that was not parsed".to_string(),
            )),
        }
    }

    let bookmarks = links_and_bookmarks::extract_bookmarks(&doc, warnings);
    let layers = layers::extract_layers(&doc);
    // `BDC /OC /Name` refers to a layer by its resource name under the page's /Properties, so the
    // layer map is keyed by those names. A layer without one gets `layer_<index>`.
    let layers: Vec<(LayerInternalId, crate::ops::Layer)> = layers
        .into_iter()
        .enumerate()
        .map(|(i, (name, layer))| {
            let key = name.unwrap_or_else(|| format!("layer_{}", i));
            (LayerInternalId(key), layer)
        })
        .collect();

    let extgstates = extract_extgstates(&doc, &objs_to_search_for_resources, warnings);

    let shadings = extract_shadings(&doc, &objs_to_search_for_resources, warnings);

    let fonts = font_resources(
        fonts
            .into_iter()
            .map(|(id, pf)| (FontId(id.get_id().to_string()), pf)),
    );

    let pdf_doc = PdfDocument {
        metadata: PdfMetadata {
            info: document_info,
            xmp: None,
        },
        resources: PdfResources {
            fonts: PdfFontMap { map: fonts },
            xobjects: XObjectMap { map: xobjects },
            extgstates: ExtendedGraphicsStateMap { map: extgstates },
            shadings: crate::ShadingMap { map: shadings },
            layers: PdfLayerMap {
                map: layers.into_iter().collect(),
            },
        },
        bookmarks: PageAnnotMap {
            map: bookmarks
                .into_iter()
                .enumerate()
                .map(|(i, bookmark)| (PageAnnotId(format!("bookmark_{}", i)), bookmark))
                .collect(),
        },
        pages,
    };

    Ok(pdf_doc)
}

/// The fonts of a scope that the document model keeps: the ones with a font program. A standard
/// font needs no entry, and a font without a program only decodes text.
fn font_resources(
    fonts: impl Iterator<Item = (FontId, ParsedOrBuiltinFont)>,
) -> BTreeMap<FontId, crate::font::PdfFont> {
    fonts
        .filter_map(|(id, pf)| {
            let parsed_font = pf.as_parsed_font()?;
            Some((id, crate::font::PdfFont::new(parsed_font)))
        })
        .collect()
}

/// An external font parsed out of the PDF, together with everything the
/// content-stream text decoder needs to know about it.
pub struct ParsedExternalFont {
    /// The parsed font program.
    pub font: ParsedFont,
    /// The ToUnicode CMap (parsed, or built from the font's cmap table when the PDF has none).
    /// Keyed by the code in the content stream: the CID for composite fonts, one byte for simple
    /// fonts.
    pub to_unicode: Option<Arc<ToUnicodeCMap>>,
    /// `true` for a composite font (`/Subtype /Type0`), whose text-showing operators carry
    /// big-endian codes of one or more bytes, and for a simple TrueType font without a Unicode
    /// `cmap`, which is read by code; `false` for any other simple (TrueType or Type1) font, with
    /// one byte per code.
    pub is_cid: bool,
    /// CID to glyph id, for composite fonts where the two differ: a `/CIDToGIDMap` stream
    /// (CIDFontType2) or the CFF charset of a CID-keyed CIDFontType0 program (ISO 32000-1,
    /// 9.7.4.2). For a simple font read by code it is the glyph of each code. `None` means
    /// CID == glyph id (Identity). Glyph ids are looked up through this map so that outlines and
    /// widths stay correct.
    pub cid_to_gid: Option<Arc<BTreeMap<u16, u16>>>,
    /// The Type0 `/Encoding` CMap when it is not Identity-H: an embedded CMap stream or
    /// Identity-V. A simple font read by code has a CMap of one-byte codes. `None` means
    /// Identity-H (2-byte codes, code == CID).
    pub encoding_cmap: Option<Arc<crate::cmap::CidCMap>>,
}

/// A font resource of the PDF: an embedded font, one of the 14 standard fonts, or a stub.
pub enum ParsedOrBuiltinFont {
    /// An embedded font program and its decoding tables.
    P(ParsedExternalFont),
    /// One of the 14 standard fonts.
    B(BuiltinFont),
    /// A composite (Type0) font whose font program could not be extracted or parsed (for
    /// example because it is not embedded). It cannot be embedded again, but its `/Encoding`
    /// CMap and ToUnicode CMap are kept so that its text still decodes.
    CidStub(
        Option<Arc<ToUnicodeCMap>>,
        Option<Arc<crate::cmap::CidCMap>>,
    ),
}

impl ParsedOrBuiltinFont {
    fn as_parsed_font(self) -> Option<ParsedFont> {
        match self {
            ParsedOrBuiltinFont::P(p) => Some(p.font),
            ParsedOrBuiltinFont::B(_) => None,
            ParsedOrBuiltinFont::CidStub(..) => None,
        }
    }

    /// How the text-showing operators decode their bytes when this font is selected.
    fn decode_kind(&self) -> FontTextDecodeKind {
        match self {
            ParsedOrBuiltinFont::B(b) => FontTextDecodeKind::Builtin(*b),
            ParsedOrBuiltinFont::P(p) => {
                if p.is_cid {
                    FontTextDecodeKind::Cid {
                        to_unicode: p.to_unicode.clone(),
                        encoding_cmap: p.encoding_cmap.clone(),
                        cid_to_gid: p.cid_to_gid.clone(),
                    }
                } else {
                    FontTextDecodeKind::Simple {
                        to_unicode: p.to_unicode.clone(),
                    }
                }
            }
            ParsedOrBuiltinFont::CidStub(to_unicode, encoding_cmap) => FontTextDecodeKind::Cid {
                to_unicode: to_unicode.clone(),
                encoding_cmap: encoding_cmap.clone(),
                // No font program, so no charset or CIDToGIDMap to resolve through.
                cid_to_gid: None,
            },
        }
    }

    #[allow(dead_code)]
    fn cmap(&self) -> Option<&ToUnicodeCMap> {
        match self {
            ParsedOrBuiltinFont::P(p) => p.to_unicode.as_deref(),
            ParsedOrBuiltinFont::B(_) => None,
            ParsedOrBuiltinFont::CidStub(c, _) => c.as_deref(),
        }
    }
}

/// How the bytes of the text-showing operators (`Tj`, `TJ`, `'`, `"`) decode for one font
/// resource. Resolved once per page from the font resources, then tracked across `Tf` operators
/// in [`PageState`]. The code width comes from the font, never from the length of the string.
#[derive(Debug, Clone)]
pub enum FontTextDecodeKind {
    /// One of the 14 standard fonts: one-byte WinAnsi codes.
    Builtin(BuiltinFont),
    /// Simple external font (TrueType/Type1): one-byte codes, decoded through
    /// the ToUnicode CMap when present, WinAnsi otherwise.
    Simple {
        /// ToUnicode CMap of the font, if it has one.
        to_unicode: Option<Arc<ToUnicodeCMap>>,
    },
    /// Composite (Type0) font, or a simple TrueType font without a Unicode `cmap` that is read
    /// by code. Content stream bytes become codes (through the codespace ranges of
    /// the `/Encoding` CMap; codes are 1 to 4 bytes wide), codes become CIDs (cidrange and
    /// cidchar entries), and CIDs become glyph ids (`cid_to_gid`: the `/CIDToGIDMap` stream of a
    /// CIDFontType2 or the CFF charset of a CID-keyed CIDFontType0). `encoding_cmap: None` is
    /// Identity-H: 2-byte big-endian codes, code == CID. `to_unicode` is keyed by the code.
    Cid {
        /// ToUnicode CMap, keyed by code.
        to_unicode: Option<Arc<ToUnicodeCMap>>,
        /// The `/Encoding` CMap, or `None` for Identity-H.
        encoding_cmap: Option<Arc<crate::cmap::CidCMap>>,
        /// CID to glyph id map, or `None` when they are identical.
        cid_to_gid: Option<Arc<BTreeMap<u16, u16>>>,
    },
}

/// The dictionary behind `xobj_obj`: an inline dictionary, a stream (its dictionary), or a
/// reference to either. Pushes an error to `warnings` and returns `None` for anything else.
fn get_dict_or_resolve_ref<'a>(
    id: &str,
    doc: &'a LopdfDocument,
    xobj_obj: &'a LopdfObject,
    warnings: &mut Vec<PdfWarnMsg>,
    page: Option<usize>,
) -> Option<&'a LopdfDictionary> {
    match xobj_obj {
        Object::Dictionary(dict) => Some(dict),
        // XObjects are streams, so a stream's dictionary counts as well.
        Object::Stream(stream) => Some(&stream.dict),
        Object::Reference(r) => match doc.get_dictionary(*r) {
            Ok(s) => Some(s),
            // `get_dictionary` rejects stream objects; take the stream's dictionary instead.
            Err(_) => match doc.get_object(*r) {
                Ok(Object::Stream(stream)) => Some(&stream.dict),
                Ok(Object::Dictionary(dict)) => Some(dict),
                _ => {
                    warnings.push(PdfWarnMsg::error(
                        page.unwrap_or(0),
                        0,
                        format!("{id}: Invalid dictionary reference {r:?}"),
                    ));
                    None
                }
            },
        },
        _ => {
            warnings.push(PdfWarnMsg::error(
                page.unwrap_or(0),
                0,
                "Unexpected type for XObject resource".to_string(),
            ));
            None
        }
    }
}

/// The stream behind `xobj_obj` (inline or by reference). Pushes an error to `warnings` and
/// returns `None` for anything else.
fn get_stream_or_resolve_ref<'a>(
    doc: &'a LopdfDocument,
    xobj_obj: &'a LopdfObject,
    warnings: &mut Vec<PdfWarnMsg>,
    page: Option<usize>,
) -> Option<&'a LopdfStream> {
    match xobj_obj {
        Object::Stream(dict) => Some(dict),
        Object::Reference(r) => match doc.get_object(*r) {
            Ok(Object::Stream(s)) => Some(s),
            Ok(_) => {
                warnings.push(PdfWarnMsg::error(
                    page.unwrap_or(0),
                    0,
                    format!("Invalid stream reference {r:?}"),
                ));
                None
            }
            Err(e) => {
                warnings.push(PdfWarnMsg::error(
                    page.unwrap_or(0),
                    0,
                    format!("Invalid stream reference {r:?}: {e:?}"),
                ));
                None
            }
        },
        _ => {
            warnings.push(PdfWarnMsg::error(
                page.unwrap_or(0),
                0,
                "Unexpected type for XObject resource".to_string(),
            ));
            None
        }
    }
}

/// Result of scanning the XObject resources: an image that still has to go through the (possibly
/// async) decoder, or an XObject that is already complete.
#[derive(Clone)]
pub(crate) enum PendingXObject {
    /// An image in a self-describing container format (JPEG via `DCTDecode`, JPEG 2000 via
    /// `JPXDecode`, ...). `fallback` keeps the original stream byte for byte in case the decoder
    /// does not understand the bytes, so that saving never drops the image.
    DecodeImage {
        bytes: Vec<u8>,
        smask: Option<SMaskData>,
        fallback: ExternalXObject,
    },
    /// Fully parsed: raw bitmaps, forms that cannot be modelled and unknown subtypes kept
    /// verbatim.
    Ready(XObject),
    /// A form that is read into the model; the images in its resources may still wait to be
    /// decoded.
    Form(Arc<PendingForm>),
}

/// A decoded (uncompressed) SMask alpha channel: 8-bit gray, `width * height` bytes.
#[derive(Clone)]
pub(crate) struct SMaskData {
    pub width: usize,
    pub height: usize,
    pub gray: Vec<u8>,
}

/// Merge an 8-bit gray SMask into a decoded [`RawImage`], turning RGB8 into
/// RGBA8 and R8 into RG8 (gray+alpha). Returns the image unchanged when the
/// dimensions do not match or the pixel format has no 8-bit alpha companion.
fn apply_smask_to_rawimage(mut img: RawImage, smask: SMaskData) -> RawImage {
    use crate::image_types::{RawImageData, RawImageFormat};
    if smask.width != img.width || smask.height != img.height {
        return img;
    }
    let Some(expected) = img.width.checked_mul(img.height) else {
        return img;
    };
    if smask.gray.len() < expected {
        return img;
    }
    match (&img.data_format, &img.pixels) {
        (RawImageFormat::RGB8, RawImageData::U8(rgb)) => {
            let (Some(rgb_len), Some(rgba_len)) =
                (expected.checked_mul(3), expected.checked_mul(4))
            else {
                return img;
            };
            if rgb.len() < rgb_len {
                return img;
            }
            let mut rgba = Vec::with_capacity(rgba_len);
            for (pixel, alpha) in rgb.chunks_exact(3).zip(&smask.gray).take(expected) {
                rgba.extend_from_slice(pixel);
                rgba.push(*alpha);
            }
            img.pixels = RawImageData::U8(rgba);
            img.data_format = RawImageFormat::RGBA8;
            img
        }
        (RawImageFormat::R8, RawImageData::U8(gray)) => {
            let Some(ga_len) = expected.checked_mul(2) else {
                return img;
            };
            if gray.len() < expected {
                return img;
            }
            let mut ga = Vec::with_capacity(ga_len);
            for (value, alpha) in gray.iter().zip(&smask.gray).take(expected) {
                ga.push(*value);
                ga.push(*alpha);
            }
            img.pixels = RawImageData::U8(ga);
            img.data_format = RawImageFormat::RG8;
            img
        }
        _ => img,
    }
}

fn process_xobjects(
    xobjects: BTreeMap<XObjectId, PendingXObject>,
    warnings: &mut Vec<PdfWarnMsg>,
) -> BTreeMap<XObjectId, XObject> {
    let mut map = BTreeMap::new();
    for (xobject_id, pending) in xobjects {
        match pending {
            PendingXObject::Ready(x) => {
                map.insert(xobject_id, x);
            }
            PendingXObject::Form(pending) => {
                map.insert(xobject_id, finish_form(pending, warnings));
            }
            PendingXObject::DecodeImage {
                bytes,
                smask,
                fallback,
            } => {
                warnings.push(PdfWarnMsg::info(
                    0,
                    0,
                    format!("process XObject {} ({} bytes)", xobject_id.0, bytes.len()),
                ));
                match RawImage::decode_from_bytes(&bytes, warnings) {
                    Ok(mut o) => {
                        if let Some(sm) = smask {
                            o = apply_smask_to_rawimage(o, sm);
                        }
                        map.insert(xobject_id, XObject::Image(o));
                    }
                    Err(e) => {
                        warnings.push(PdfWarnMsg::warning(
                            0,
                            0,
                            format!(
                                "could not decode image XObject {} ({} bytes): {e}; preserving \
                                 original stream",
                                xobject_id.0,
                                bytes.len()
                            ),
                        ));
                        map.insert(xobject_id, XObject::External(fallback));
                    }
                }
            }
        }
    }
    map
}

/// A form read from the PDF, with the images in its resources decoded.
fn finish_form(pending: Arc<PendingForm>, warnings: &mut Vec<PdfWarnMsg>) -> XObject {
    // Only a form that more than one place of the document uses is copied.
    let PendingForm {
        mut form,
        xobjects,
    } = Arc::try_unwrap(pending).unwrap_or_else(|shared| (*shared).clone());
    if let Some(resources) = form.resources.as_mut() {
        resources.xobjects.map = process_xobjects(xobjects, warnings);
    }
    XObject::Form(form)
}

/// Collects the `/Shading` resources (axial and radial gradients) into the typed
/// [`crate::Shading`] model. Other shading types, and function types other than 2 (exponential)
/// and 3 (stitching), are skipped with a warning; the supported shapes are the ones printpdf
/// writes itself, so its own gradients round-trip.
fn extract_shadings(
    doc: &LopdfDocument,
    objs_to_search_for_resources: &[(LopdfObject, Option<usize>)],
    warnings: &mut Vec<PdfWarnMsg>,
) -> BTreeMap<crate::ShadingId, crate::Shading> {
    let mut map = BTreeMap::new();
    for (obj, page_idx) in objs_to_search_for_resources {
        let page_num = page_idx.unwrap_or(0);
        let dict = match obj {
            LopdfObject::Dictionary(d) => Some(d),
            LopdfObject::Reference(r) => doc.get_dictionary(*r).ok(),
            _ => None,
        };
        let Some(dict) = dict else { continue };
        let resources = if let Ok(res) = dict.get(b"Resources") {
            get_dict_or_resolve_ref(
                &format!("extract_shadings Resources page {page_num}"),
                doc,
                res,
                warnings,
                Some(page_num),
            )
        } else {
            Some(dict)
        };
        let Some(resources) = resources else { continue };
        read_shadings(doc, resources, page_num, &mut map, warnings);
    }
    map
}

/// Adds the `/Shading` resources of one resource dictionary to `map`. A name that `map` has
/// already is skipped: the first definition wins, consistent with the other resources.
fn read_shadings(
    doc: &LopdfDocument,
    resources: &LopdfDictionary,
    page_num: usize,
    map: &mut BTreeMap<crate::ShadingId, crate::Shading>,
    warnings: &mut Vec<PdfWarnMsg>,
) {
    let Ok(shading_res) = resources.get(b"Shading") else {
        return;
    };
    let Some(shading_dict) = get_dict_or_resolve_ref(
        &format!("extract_shadings Shading page {page_num}"),
        doc,
        shading_res,
        warnings,
        Some(page_num),
    ) else {
        return;
    };
    for (key, value) in shading_dict.iter() {
        let id = crate::ShadingId(String::from_utf8_lossy(key).to_string());
        if map.contains_key(&id) {
            continue;
        }
        let Some(sh) = get_dict_or_resolve_ref(
            &format!("extract_shadings entry page {page_num}"),
            doc,
            value,
            warnings,
            Some(page_num),
        ) else {
            continue;
        };
        match parse_shading_dict(doc, sh) {
            Ok(parsed) => {
                map.insert(id, parsed);
            }
            Err(e) => warnings.push(PdfWarnMsg::warning(
                page_num,
                0,
                format!("shading {:?} not preserved: {e}", id.0),
            )),
        }
    }
}

/// Translate a ShadingType 2/3 dictionary (with an exponential or stitching
/// function) into [`crate::Shading`] — the exact inverse of `Shading::to_dict`.
fn parse_shading_dict(
    doc: &LopdfDocument,
    dict: &LopdfDictionary,
) -> Result<crate::Shading, String> {
    fn as_f32(o: &LopdfObject) -> Option<f32> {
        match o {
            LopdfObject::Integer(i) => Some(*i as f32),
            LopdfObject::Real(r) => Some(*r),
            _ => None,
        }
    }
    fn num_array<const N: usize>(doc: &LopdfDocument, o: &LopdfObject) -> Option<[f32; N]> {
        let o = match o {
            LopdfObject::Reference(r) => doc.get_object(*r).ok()?,
            other => other,
        };
        let arr = o.as_array().ok()?;
        let v: Vec<f32> = arr.iter().filter_map(as_f32).collect();
        <[f32; N]>::try_from(v).ok()
    }
    fn rgb(v: &[f32]) -> [f32; 3] {
        match *v {
            [gray] => [gray, gray, gray], // DeviceGray
            [r, g, b] => [r, g, b],
            _ => [0.0, 0.0, 0.0],
        }
    }
    fn resolve<'a>(doc: &'a LopdfDocument, o: &'a LopdfObject) -> &'a LopdfObject {
        match o {
            LopdfObject::Reference(r) => doc.get_object(*r).unwrap_or(o),
            other => other,
        }
    }
    fn fn_dict(o: &LopdfObject) -> Option<&LopdfDictionary> {
        match o {
            LopdfObject::Dictionary(d) => Some(d),
            LopdfObject::Stream(s) => Some(&s.dict),
            _ => None,
        }
    }

    let shading_type = dict
        .get(b"ShadingType")
        .ok()
        .and_then(as_f32)
        .ok_or("missing ShadingType")? as i64;

    let geometry = match shading_type {
        2 => {
            let coords = dict
                .get(b"Coords")
                .ok()
                .and_then(|o| num_array::<4>(doc, o))
                .ok_or("axial shading without 4-element Coords")?;
            crate::ShadingGeometry::Axial { coords }
        }
        3 => {
            let coords = dict
                .get(b"Coords")
                .ok()
                .and_then(|o| num_array::<6>(doc, o))
                .ok_or("radial shading without 6-element Coords")?;
            crate::ShadingGeometry::Radial { coords }
        }
        other => return Err(format!("unsupported ShadingType {other}")),
    };

    let extend = dict
        .get(b"Extend")
        .ok()
        .map(|o| resolve(doc, o))
        .and_then(|o| o.as_array().ok())
        .map(|a| {
            let b = |i: usize| matches!(a.get(i), Some(LopdfObject::Boolean(true)));
            (b(0), b(1))
        })
        .unwrap_or((false, false));

    let func_obj = resolve(
        doc,
        dict.get(b"Function")
            .map_err(|_| "shading without Function")?,
    );
    let func = fn_dict(func_obj).ok_or("shading Function is not a dictionary/stream")?;
    let func_type = func
        .get(b"FunctionType")
        .ok()
        .and_then(as_f32)
        .ok_or("Function without FunctionType")? as i64;

    // One exponential segment over [t0, t1] → two stops.
    fn exponential_stops(
        doc: &LopdfDocument,
        f: &LopdfDictionary,
        t0: f32,
        t1: f32,
    ) -> Result<Vec<crate::GradientStop>, String> {
        let get = |k: &[u8]| -> Vec<f32> {
            f.get(k)
                .ok()
                .and_then(|o| {
                    let o = match o {
                        LopdfObject::Reference(r) => doc.get_object(*r).ok()?,
                        other => other,
                    };
                    o.as_array().ok()
                })
                .map(|a| {
                    a.iter()
                        .filter_map(|x| match x {
                            LopdfObject::Integer(i) => Some(*i as f32),
                            LopdfObject::Real(r) => Some(*r),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_else(|| vec![0.0])
        };
        let c0 = get(b"C0");
        let c1 = get(b"C1");
        Ok(vec![
            crate::GradientStop {
                offset: t0,
                color: rgb(&c0),
            },
            crate::GradientStop {
                offset: t1,
                color: rgb(&c1),
            },
        ])
    }

    let mut stops = match func_type {
        2 => exponential_stops(doc, func, 0.0, 1.0)?,
        3 => {
            let bounds = func
                .get(b"Bounds")
                .ok()
                .map(|o| resolve(doc, o))
                .and_then(|o| o.as_array().ok())
                .map(|a| a.iter().filter_map(as_f32).collect::<Vec<_>>())
                .unwrap_or_default();
            let funcs = func
                .get(b"Functions")
                .ok()
                .map(|o| resolve(doc, o))
                .and_then(|o| o.as_array().ok())
                .ok_or("stitching function without Functions array")?;
            let mut edges = Vec::with_capacity(funcs.len() + 1);
            edges.push(0.0);
            edges.extend(bounds.iter().copied());
            edges.push(1.0);
            let mut all = Vec::new();
            for (i, sub) in funcs.iter().enumerate() {
                let sub = resolve(doc, sub);
                let sub = fn_dict(sub).ok_or("stitching sub-function is not a dictionary")?;
                let (t0, t1) = (
                    edges.get(i).copied().unwrap_or(0.0),
                    edges.get(i + 1).copied().unwrap_or(1.0),
                );
                all.extend(exponential_stops(doc, sub, t0, t1)?);
            }
            all
        }
        other => return Err(format!("unsupported FunctionType {other}")),
    };

    // Merge duplicate boundary stops (segment N's end == segment N+1's start).
    stops.dedup_by(|b, a| (a.offset - b.offset).abs() < 1e-6 && a.color == b.color);
    if stops.is_empty() {
        return Err("shading function produced no stops".to_string());
    }

    Ok(crate::Shading {
        geometry,
        stops,
        extend,
    })
}

// Collects the ExtGState resources of all resource dictionaries (the last definition of a name wins).
fn extract_extgstates(
    doc: &LopdfDocument,
    objs_to_search_for_resources: &[(LopdfObject, Option<usize>)],
    warnings: &mut Vec<PdfWarnMsg>,
) -> BTreeMap<ExtendedGraphicsStateId, ExtendedGraphicsState> {
    objs_to_search_for_resources
        .iter()
        .filter_map(|(obj, page_idx)| {
            let page_num = page_idx.unwrap_or(0);

            let dict = match obj {
                LopdfObject::Dictionary(d) => Some(d),
                LopdfObject::Reference(r) => doc.get_dictionary(*r).ok(),
                _ => None,
            }?;

            let resources = if let Ok(res) = dict.get(b"Resources") {
                get_dict_or_resolve_ref(
                    &format!("extract_extgstates Resources page {page_num}"),
                    doc,
                    res,
                    warnings,
                    Some(page_num),
                )
            } else {
                // If this is already a Resources dict
                Some(dict)
            }?;

            read_extgstates(doc, resources, page_num, warnings)
        })
        .fold(BTreeMap::new(), |mut acc, extgstates| {
            acc.extend(extgstates);
            acc
        })
}

/// The `/ExtGState` resources of one resource dictionary.
fn read_extgstates(
    doc: &LopdfDocument,
    resources: &LopdfDictionary,
    page_num: usize,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Option<BTreeMap<ExtendedGraphicsStateId, ExtendedGraphicsState>> {
    let extgstate = resources.get(b"ExtGState").ok()?;
    let extgstate_dict = get_dict_or_resolve_ref(
        &format!("extract_extgstates ExtGState page {page_num}"),
        doc,
        extgstate,
        warnings,
        Some(page_num),
    )?;

    let gs_entries = extgstate_dict
        .iter()
        .filter_map(|(key, value)| {
            let gs_dict = get_dict_or_resolve_ref(
                &format!("extract_extgstates GsDict page {page_num}"),
                doc,
                value,
                warnings,
                Some(page_num),
            )?;
            let gs_id = ExtendedGraphicsStateId(String::from_utf8_lossy(key).to_string());
            let gs = extgstate::parse_extgstate(gs_dict);
            Some((gs_id, gs))
        })
        .collect::<BTreeMap<_, _>>();

    Some(gs_entries)
}

async fn process_xobjects_async(
    xobjects: BTreeMap<XObjectId, PendingXObject>,
    warnings: &mut Vec<PdfWarnMsg>,
) -> BTreeMap<XObjectId, XObject> {
    let mut map = BTreeMap::new();
    for (xobject_id, pending) in xobjects {
        match pending {
            PendingXObject::Ready(x) => {
                map.insert(xobject_id, x);
            }
            // The images inside a form are decoded without the async decoder.
            PendingXObject::Form(pending) => {
                map.insert(xobject_id, finish_form(pending, warnings));
            }
            PendingXObject::DecodeImage {
                bytes,
                smask,
                fallback,
            } => match RawImage::decode_from_bytes_async(&bytes, warnings).await {
                Ok(mut o) => {
                    if let Some(sm) = smask {
                        o = apply_smask_to_rawimage(o, sm);
                    }
                    map.insert(xobject_id, XObject::Image(o));
                }
                Err(e) => {
                    warnings.push(PdfWarnMsg::warning(
                        0,
                        0,
                        format!(
                            "could not decode image XObject {}: {e}; preserving original stream",
                            xobject_id.0
                        ),
                    ));
                    map.insert(xobject_id, XObject::External(fallback));
                }
            },
        }
    }
    map
}

/// Maximum recursion depth (container nesting + reference hops) when inlining
/// indirect references into a preserved XObject dictionary. Real-world SVG
/// conversions nest forms → resources → patterns → shadings → functions →
/// ICC streams, so this must be generous; actual cycles are caught by the
/// reference path stack, not the depth cap.
const MAX_RESOLVE_DEPTH: usize = 96;

/// Most objects that inlining references into the preserved XObjects of one document may copy.
const MAX_INLINED_NODES: usize = 2_000_000;

/// Most bytes of nested streams and strings that the same inlining may copy for one document.
const MAX_INLINED_BYTES: usize = 256 * 1024 * 1024;

/// What is left of the inlining budget of one document.
///
/// Inlining copies an object every time it is reached, so objects that are shared at many
/// levels (each level refers to the next one twice) multiply: a few kilobytes of input can
/// stand for more copies than fit in memory. Once the budget is spent, the rest becomes `null`.
struct InlineBudget {
    nodes: usize,
    bytes: usize,
    /// Something was replaced by `null` because the budget ran out.
    exhausted: bool,
    /// The warning about it has been written.
    reported: bool,
}

impl InlineBudget {
    fn new() -> Self {
        InlineBudget {
            nodes: MAX_INLINED_NODES,
            bytes: MAX_INLINED_BYTES,
            exhausted: false,
            reported: false,
        }
    }

    /// Takes one object that holds `bytes` bytes of data. Returns `false` when the budget does
    /// not cover it.
    fn take(&mut self, bytes: usize) -> bool {
        match (self.nodes.checked_sub(1), self.bytes.checked_sub(bytes)) {
            (Some(nodes), Some(left)) => {
                self.nodes = nodes;
                self.bytes = left;
                true
            }
            _ => {
                self.exhausted = true;
                false
            }
        }
    }
}

/// Convert a lopdf object into a [`DictItem`], resolving indirect references
/// by *inlining* the referenced objects.
///
/// Preserved XObjects are carried across the parse/save round trip as
/// [`ExternalXObject`]s. Their dictionaries routinely reference other objects
/// (a Form XObject's `/Resources`, pattern functions, ICC profiles, ...) — but
/// object ids are meaningless in the re-saved document, so a verbatim
/// `Reference` would dangle. Inlining the referenced object keeps the XObject
/// self-contained; `into_lopdf_with_indirect_streams` re-hoists nested streams
/// into indirect objects at save time.
///
/// `ref_path` holds the reference ids currently being resolved (the *path*,
/// not all visited ids — legitimate documents share objects diamond-style);
/// re-entering an id on the same path is a cycle and yields `Null`. Every object copied is
/// taken from `budget`; when it is spent the rest yields `Null` as well.
fn dictitem_from_lopdf_resolved(
    doc: &LopdfDocument,
    obj: &LopdfObject,
    depth: usize,
    ref_path: &mut Vec<ObjectId>,
    budget: &mut InlineBudget,
) -> DictItem {
    if depth > MAX_RESOLVE_DEPTH {
        return DictItem::Null;
    }
    let bytes = match obj {
        Object::String(s, _) => s.len(),
        Object::Name(n) => n.len(),
        Object::Stream(s) => s.content.len(),
        _ => 0,
    };
    if !budget.take(bytes) {
        return DictItem::Null;
    }
    match obj {
        Object::Reference(r) => {
            if ref_path.contains(r) {
                // reference cycle
                return DictItem::Null;
            }
            match doc.get_object(*r) {
                Ok(target) => {
                    ref_path.push(*r);
                    let item =
                        dictitem_from_lopdf_resolved(doc, target, depth + 1, ref_path, budget);
                    ref_path.pop();
                    item
                }
                Err(_) => DictItem::Null,
            }
        }
        Object::Array(objects) => DictItem::Array(
            objects
                .iter()
                .map(|o| dictitem_from_lopdf_resolved(doc, o, depth + 1, ref_path, budget))
                .collect(),
        ),
        Object::Dictionary(dict) => DictItem::Dict {
            map: dict
                .iter()
                .map(|(k, v)| {
                    (
                        String::from_utf8_lossy(k).to_string(),
                        dictitem_from_lopdf_resolved(doc, v, depth + 1, ref_path, budget),
                    )
                })
                .collect(),
        },
        Object::Stream(stream) => DictItem::Stream {
            stream: external_stream_preserved(doc, stream, depth + 1, ref_path, budget),
        },
        other => DictItem::from_lopdf(other),
    }
}

/// Preserve a lopdf stream verbatim as an [`ExternalStream`]: the dictionary
/// with references inlined (minus `/Length`, which is recomputed at save time)
/// and the *raw, still-encoded* content, so existing `/Filter` entries remain
/// valid.
fn external_stream_preserved(
    doc: &LopdfDocument,
    stream: &LopdfStream,
    depth: usize,
    ref_path: &mut Vec<ObjectId>,
    budget: &mut InlineBudget,
) -> ExternalStream {
    let dict = stream
        .dict
        .iter()
        .filter(|(k, _)| k.as_slice() != b"Length")
        .map(|(k, v)| {
            (
                String::from_utf8_lossy(k).to_string(),
                dictitem_from_lopdf_resolved(doc, v, depth, ref_path, budget),
            )
        })
        .collect();
    ExternalStream {
        dict,
        content: stream.content.clone(),
        // The content is already encoded exactly as the preserved /Filter
        // describes; re-compressing it would corrupt the stream.
        compress: false,
    }
}

/// Pushes a warning, once per document, when `budget` ran out while copying the references of
/// the XObject `xobject_id`.
fn warn_if_budget_spent(
    budget: &mut InlineBudget,
    xobject_id: &XObjectId,
    page: usize,
    warnings: &mut Vec<PdfWarnMsg>,
) {
    if budget.exhausted && !budget.reported {
        budget.reported = true;
        warnings.push(PdfWarnMsg::warning(
            page,
            0,
            format!(
                "XObject {}: the references inside it exceed the limit for one document \
                 ({MAX_INLINED_NODES} objects or {MAX_INLINED_BYTES} bytes); the rest are \
                 replaced by null",
                xobject_id.0
            ),
        ));
    }
}

/// Preserve an XObject stream verbatim for the round trip. Pushes a warning, once per document,
/// when `budget` ran out while copying the references of the stream.
fn preserve_xobject_stream(
    doc: &LopdfDocument,
    stream: &LopdfStream,
    xobject_id: &XObjectId,
    page: usize,
    budget: &mut InlineBudget,
    warnings: &mut Vec<PdfWarnMsg>,
) -> ExternalXObject {
    let stream = external_stream_preserved(doc, stream, 0, &mut Vec::new(), budget);
    warn_if_budget_spent(budget, xobject_id, page, warnings);
    ExternalXObject {
        stream,
        // Leave width/height/dpi unset: placement comes from the content
        // stream's own `cm` + the preserved `/BBox`/`/Matrix`, and a `Some`
        // width/height would make the serializer inject an extra scale.
        width: None,
        height: None,
        dpi: None,
    }
}

/// Number of color components for a (resolved) `/ColorSpace` entry, for the
/// raw-bitmap path. Returns `None` for colorspaces that `RawImage` cannot
/// represent losslessly (Indexed, Separation, DeviceN, ...).
fn colorspace_components(doc: &LopdfDocument, cs: &LopdfObject, depth: usize) -> Option<usize> {
    if depth > MAX_RESOLVE_DEPTH {
        return None;
    }
    match cs {
        Object::Reference(r) => colorspace_components(doc, doc.get_object(*r).ok()?, depth + 1),
        Object::Name(n) => match n.as_slice() {
            b"DeviceRGB" | b"CalRGB" => Some(3),
            b"DeviceGray" | b"CalGray" => Some(1),
            _ => None,
        },
        Object::Array(arr) => {
            let first = arr.first()?;
            let family = match first {
                Object::Name(n) => n.as_slice(),
                _ => return None,
            };
            match family {
                b"ICCBased" => {
                    let stream = arr.get(1)?;
                    let stream = match stream {
                        Object::Reference(r) => match doc.get_object(*r).ok()? {
                            Object::Stream(s) => s,
                            _ => return None,
                        },
                        Object::Stream(s) => s,
                        _ => return None,
                    };
                    match stream.dict.get(b"N").ok()? {
                        Object::Integer(1) => Some(1),
                        Object::Integer(3) => Some(3),
                        _ => None,
                    }
                }
                b"CalRGB" => Some(3),
                b"CalGray" => Some(1),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The `/Filter` chain of a stream as a list of names.
fn stream_filters(stream: &LopdfStream) -> Vec<Vec<u8>> {
    match stream.dict.get(b"Filter") {
        Ok(Object::Name(n)) => vec![n.clone()],
        Ok(Object::Array(arr)) => arr
            .iter()
            .filter_map(|o| match o {
                Object::Name(n) => Some(n.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Most bytes one stream of the input document may decode to. A few kilobytes of Flate data can
/// stand for gigabytes, so every stream that is decoded here is bounded.
const MAX_STREAM_BYTES: usize = 256 * 1024 * 1024;

/// How [`decode_limited`] ended.
enum Decoded {
    Data(Vec<u8>),
    /// A filter of the stream cannot be undone, or its data is damaged.
    Undecodable,
    /// The decoded data would be larger than the limit.
    TooLarge,
}

/// Decodes the filters of `stream` into at most `limit` bytes.
fn decode_limited(stream: &LopdfStream, limit: usize) -> Decoded {
    match stream.decompressed_content_with_limit(limit) {
        Ok(data) => Decoded::Data(data),
        Err(lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded { .. })) => {
            Decoded::TooLarge
        }
        Err(_) => Decoded::Undecodable,
    }
}

/// The decoded bytes of `stream`, or its stored bytes when a filter cannot be undone. Pushes a
/// warning that names `what` and returns `None` when the decoded data is larger than `limit`
/// bytes.
fn decode_stream(
    stream: &LopdfStream,
    limit: usize,
    what: &str,
    page: usize,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Option<Vec<u8>> {
    match decode_limited(stream, limit) {
        Decoded::Data(data) => Some(data),
        Decoded::Undecodable => Some(stream.content.clone()),
        Decoded::TooLarge => {
            warnings.push(PdfWarnMsg::error(
                page,
                0,
                format!("{what}: the decoded stream is larger than {limit} bytes; skipped"),
            ));
            None
        }
    }
}

/// Resolve and decode the `/SMask` of an image stream to 8-bit gray alpha.
fn extract_smask(
    doc: &LopdfDocument,
    stream: &LopdfStream,
    page_num: usize,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Option<SMaskData> {
    let smask_obj = stream.dict.get(b"SMask").ok()?;
    let smask_stream = match smask_obj {
        Object::Reference(r) => match doc.get_object(*r).ok()? {
            Object::Stream(s) => s,
            _ => return None,
        },
        Object::Stream(s) => s,
        _ => return None,
    };
    let width = usize::try_from(smask_stream.dict.get(b"Width").ok()?.as_i64().ok()?).ok()?;
    let height = usize::try_from(smask_stream.dict.get(b"Height").ok()?.as_i64().ok()?).ok()?;
    let bpc = smask_stream
        .dict
        .get(b"BitsPerComponent")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(8);
    if bpc != 8 {
        return None;
    }
    // The data cannot be larger than a stream may decode to, so a mask that declares more is
    // incomplete.
    let size = width.checked_mul(height)?;
    if size > MAX_STREAM_BYTES {
        return None;
    }
    let gray = decode_stream(smask_stream, MAX_STREAM_BYTES, "SMask", page_num, warnings)?;
    if gray.len() < size {
        return None;
    }
    Some(SMaskData {
        width,
        height,
        gray,
    })
}

/// Try to construct a [`RawImage`] directly from a raw-bitmap image stream
/// (no filter, or a lossless filter lopdf can undo). This is how printpdf
/// itself writes images by default (`FlateDecode` over raw pixels), so this
/// path is what makes printpdf's own output round-trippable: the pixel data
/// carries no container magic bytes, so `RawImage::decode_from_bytes` (which
/// sniffs PNG/JPEG/... signatures) can never decode it.
#[cfg(feature = "images")]
pub(crate) fn raw_bitmap_from_stream(
    doc: &LopdfDocument,
    stream: &LopdfStream,
) -> Option<RawImage> {
    raw_bitmap_from_stream_in(doc, stream, 0, &mut Vec::new())
}

/// [`raw_bitmap_from_stream`], with the page and the warnings of the parse that reads the image.
fn raw_bitmap_from_stream_in(
    doc: &LopdfDocument,
    stream: &LopdfStream,
    page_num: usize,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Option<RawImage> {
    use crate::image_types::{RawImageData, RawImageFormat};

    let width = usize::try_from(stream.dict.get(b"Width").ok()?.as_i64().ok()?).ok()?;
    let height = usize::try_from(stream.dict.get(b"Height").ok()?.as_i64().ok()?).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    let bpc = stream
        .dict
        .get(b"BitsPerComponent")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(8);
    // Samples of 8 and 16 bits are read; other depths are kept as they are.
    let bytes_per_sample: usize = match bpc {
        8 => 1,
        16 => 2,
        _ => return None,
    };
    let components = colorspace_components(doc, stream.dict.get(b"ColorSpace").ok()?, 0)?;
    // /ImageMask stencils and predictor-encoded data have extra semantics the
    // raw path does not model; preserve those streams instead.
    if let Ok(im) = stream.dict.get(b"ImageMask") {
        if im.as_bool().unwrap_or(false) {
            return None;
        }
    }
    if let Ok(Object::Dictionary(dp)) = stream.dict.get(b"DecodeParms") {
        if dp
            .get(b"Predictor")
            .map(|p| p.as_i64().unwrap_or(1))
            .unwrap_or(1)
            > 1
        {
            return None;
        }
    }

    // The data cannot be larger than a stream may decode to, so a bitmap that declares more is
    // incomplete.
    let expected = width
        .checked_mul(height)?
        .checked_mul(components)?
        .checked_mul(bytes_per_sample)?;
    if expected > MAX_STREAM_BYTES {
        return None;
    }
    // A stream that cannot be decoded, or that is too large, is left to the caller, which tries it
    // as a container file and otherwise keeps it as it is (saying so when it is too large).
    let Decoded::Data(data) = decode_limited(stream, MAX_STREAM_BYTES) else {
        return None;
    };

    match (bpc, components) {
        (8, 3) | (8, 1) => {
            if data.len() < expected {
                return None;
            }
            let format = if components == 3 {
                RawImageFormat::RGB8
            } else {
                RawImageFormat::R8
            };
            let mut pixels = data;
            pixels.truncate(expected);
            let mut img = RawImage {
                pixels: RawImageData::U8(pixels),
                width,
                height,
                data_format: format,
                tag: Vec::new(),
            };
            if let Some(smask) = extract_smask(doc, stream, page_num, warnings) {
                img = apply_smask_to_rawimage(img, smask);
            }
            Some(img)
        }
        (16, 3) | (16, 1) => {
            let format = if components == 3 {
                RawImageFormat::RGB16
            } else {
                RawImageFormat::R16
            };
            let pixels: Vec<u16> = data
                .get(..expected)?
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_be_bytes(*c))
                .collect();
            Some(RawImage {
                pixels: RawImageData::U16(pixels),
                width,
                height,
                data_format: format,
                tag: Vec::new(),
            })
        }
        _ => None,
    }
}

/// Parses all XObjects of a Resources dictionary into `xobj_map`. Images are decoded (or queued
/// for decoding); everything else, such as Form XObjects, or images with an unsupported filter or
/// color space, is kept verbatim so that a parse and save never drops page content. A name that
/// `xobj_map` has already is skipped: the first definition of a name wins.
fn parse_xobjects_internal(
    doc: &LopdfDocument,
    resources: &LopdfDictionary,
    warnings: &mut Vec<PdfWarnMsg>,
    page: Option<usize>,
    xobj_map: &mut BTreeMap<XObjectId, PendingXObject>,
    ctx: &mut XObjectCtx<'_>,
) {
    let page_num = page.unwrap_or_default();
    let xobj_obj = match resources.get(b"XObject") {
        Ok(s) => s,
        Err(_) => return,
    };

    // The XObject entry may be a dictionary or a reference.
    let xobj_dict = match get_dict_or_resolve_ref(
        &format!("parse_xobjects_internal 1 page {page_num}"),
        doc,
        xobj_obj,
        warnings,
        page,
    ) {
        Some(s) => s,
        None => return,
    };

    for (key, value) in xobj_dict.iter() {
        let xobject_id = XObjectId(String::from_utf8_lossy(key).to_string());
        if xobj_map.contains_key(&xobject_id) {
            continue;
        }

        // Every XObject is a stream object.
        let stream = match get_stream_or_resolve_ref(doc, value, warnings, page) {
            Some(s) => s,
            None => continue,
        };

        let subtype = match stream.dict.get(b"Subtype") {
            Ok(Object::Name(o)) => o.clone(),
            _ => {
                // Missing /Subtype: sniff — image streams have /Width + /Height.
                if stream.dict.get(b"Width").is_ok() && stream.dict.get(b"Height").is_ok() {
                    b"Image".to_vec()
                } else {
                    warnings.push(PdfWarnMsg::warning(
                        page_num,
                        0,
                        format!(
                            "parse-xobjects: XObject {} has no /Subtype; preserving verbatim",
                            xobject_id.0
                        ),
                    ));
                    let preserved =
                        preserve_xobject_stream(doc, stream, &xobject_id, page_num, &mut ctx.budget, warnings);
                    xobj_map.insert(
                        xobject_id,
                        PendingXObject::Ready(XObject::External(preserved)),
                    );
                    continue;
                }
            }
        };

        match subtype.as_slice() {
            b"Image" => {
                let filters = stream_filters(stream);
                let has_container_format = filters
                    .iter()
                    .any(|f| f == b"DCTDecode" || f == b"JPXDecode");
                let what = format!("XObject {}", xobject_id.0);

                let entry = if has_container_format {
                    // JPEG / JPEG2000: the (possibly Flate-wrapped) stream
                    // content *is* the container file.
                    let bytes = if filters.len() == 1 {
                        Some(stream.content.clone())
                    } else {
                        decode_stream(stream, MAX_STREAM_BYTES, &what, page_num, warnings)
                    };
                    match bytes {
                        Some(bytes) => PendingXObject::DecodeImage {
                            bytes,
                            smask: extract_smask(doc, stream, page_num, warnings),
                            fallback: preserve_xobject_stream(
                                doc, stream, &xobject_id, page_num, &mut ctx.budget, warnings,
                            ),
                        },
                        // Too large to decode: the stream stays as it is.
                        None => PendingXObject::Ready(XObject::External(
                            preserve_xobject_stream(
                                doc, stream, &xobject_id, page_num, &mut ctx.budget, warnings,
                            ),
                        )),
                    }
                } else if let Some(img) = raw_bitmap_from_stream_in(doc, stream, page_num, warnings)
                {
                    // Without the `images` feature the bitmap stream is kept verbatim as an
                    // external XObject instead of a decoded image.
                    if cfg!(feature = "images") {
                        PendingXObject::Ready(XObject::Image(img))
                    } else {
                        PendingXObject::Ready(XObject::External(preserve_xobject_stream(
                            doc, stream, &xobject_id, page_num, &mut ctx.budget, warnings,
                        )))
                    }
                } else {
                    // Could still be an embedded container file without a
                    // filter entry (rare), or something we cannot model —
                    // try the decoder, fall back to byte-level preservation.
                    match decode_stream(stream, MAX_STREAM_BYTES, &what, page_num, warnings) {
                        Some(bytes) => PendingXObject::DecodeImage {
                            bytes,
                            smask: extract_smask(doc, stream, page_num, warnings),
                            fallback: preserve_xobject_stream(
                                doc, stream, &xobject_id, page_num, &mut ctx.budget, warnings,
                            ),
                        },
                        None => PendingXObject::Ready(XObject::External(
                            preserve_xobject_stream(
                                doc, stream, &xobject_id, page_num, &mut ctx.budget, warnings,
                            ),
                        )),
                    }
                };
                xobj_map.insert(xobject_id, entry);
            }
            b"Form" => {
                // A form that can be modelled becomes a `XObject::Form`; otherwise it is kept
                // verbatim, references inlined.
                let entry = match parse_form(doc, value, stream, &xobject_id, page_num, ctx, warnings)
                {
                    Some(form) => PendingXObject::Form(form),
                    None => PendingXObject::Ready(XObject::External(preserve_xobject_stream(
                        doc,
                        stream,
                        &xobject_id,
                        page_num,
                        &mut ctx.budget,
                        warnings,
                    ))),
                };
                xobj_map.insert(xobject_id, entry);
            }
            _other => {
                // Any other subtype is kept verbatim, references inlined.
                let preserved =
                    preserve_xobject_stream(doc, stream, &xobject_id, page_num, &mut ctx.budget, warnings);
                xobj_map.insert(
                    xobject_id,
                    PendingXObject::Ready(XObject::External(preserved)),
                );
            }
        }
    }
}

fn parse_fonts_from_entire_pdf(
    doc: &LopdfDocument,
    objs_to_search_for_resources: &[(LopdfObject, Option<usize>)],
    warnings: &mut Vec<PdfWarnMsg>,
) -> BTreeMap<BuiltinOrExternalFontId, ParsedOrBuiltinFont> {
    let mut map = BuiltinFont::all_ids()
        .iter()
        .map(|b| {
            (
                BuiltinOrExternalFontId::Builtin(*b),
                ParsedOrBuiltinFont::B(*b),
            )
        })
        .collect::<BTreeMap<_, _>>();

    for (obj, page_idx) in objs_to_search_for_resources {
        let page_num = page_idx.unwrap_or(0);
        let dict = match obj {
            LopdfObject::Dictionary(d) => Some(d),
            LopdfObject::Reference(r) => doc.get_dictionary(*r).ok(),
            _ => None,
        };
        let Some(dict) = dict else { continue };
        let Ok(resources_obj) = dict.get(b"Resources") else {
            continue;
        };
        let Some(resources_dict) = get_dict_or_resolve_ref(
            &format!("parse_fonts_from_entire_pdf 1 page {page_num}"),
            doc,
            resources_obj,
            warnings,
            Some(page_num),
        ) else {
            continue;
        };
        // The first definition of a font name wins, so names that are already in `map` are not
        // read again.
        let fonts =
            parsefont::parse_fonts_enhanced(doc, resources_dict, warnings, Some(page_num), &map);
        for (font_id, parsed_font) in fonts {
            map.entry(BuiltinOrExternalFontId::External(font_id))
                .or_insert(parsed_font);
        }
    }
    map
}

fn parse_xobjects_from_entire_pdf(
    doc: &LopdfDocument,
    objs_to_search_for_resources: &[(LopdfObject, Option<usize>)],
    font_decode: &BTreeMap<String, FontTextDecodeKind>,
    warnings: &mut Vec<PdfWarnMsg>,
) -> BTreeMap<XObjectId, PendingXObject> {
    let mut xobjects = BTreeMap::new();
    let mut ctx = XObjectCtx::new(font_decode);
    for (obj, page_idx) in objs_to_search_for_resources {
        let page_num = page_idx.unwrap_or(0);
        let dict = match obj {
            LopdfObject::Dictionary(d) => Some(d),
            LopdfObject::Reference(r) => doc.get_dictionary(*r).ok(),
            _ => None,
        };
        let Some(dict) = dict else { continue };
        let Ok(resources_obj) = dict.get(b"Resources") else {
            continue;
        };
        let Some(resources_dict) = get_dict_or_resolve_ref(
            &format!("parse_xobjects_from_entire_pdf 1 page {page_num}"),
            doc,
            resources_obj,
            warnings,
            Some(page_num),
        ) else {
            continue;
        };
        parse_xobjects_internal(
            doc,
            resources_dict,
            warnings,
            Some(page_num),
            &mut xobjects,
            &mut ctx,
        );
    }
    xobjects
}

/// Recursively collects page object references from a Pages tree dictionary.
fn collect_page_refs(
    dict: &LopdfDictionary,
    doc: &LopdfDocument,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Result<Vec<ObjectId>, String> {
    let mut pages = Vec::new();
    let mut visited = std::collections::BTreeSet::new();
    collect_page_refs_inner(dict, doc, &mut pages, &mut visited, warnings, 0)?;
    Ok(pages)
}

fn collect_page_refs_inner(
    dict: &LopdfDictionary,
    doc: &LopdfDocument,
    pages: &mut Vec<ObjectId>,
    visited: &mut std::collections::BTreeSet<ObjectId>,
    warnings: &mut Vec<PdfWarnMsg>,
    depth: usize,
) -> Result<(), String> {
    // A malformed (or hostile) page tree can contain reference cycles; without
    // the visited set + depth cap this recursion would never terminate.
    if depth > 64 {
        warnings.push(PdfWarnMsg::warning(
            0,
            0,
            "Pages tree deeper than 64 levels, ignoring deeper nodes".to_string(),
        ));
        return Ok(());
    }

    // The Pages tree must have a "Kids" array (possibly behind a reference).
    let kids = dict
        .get(b"Kids")
        .map_err(|e| format!("Pages dictionary missing Kids key: {}", e))?;
    let kids = match kids {
        Object::Reference(r) => doc
            .get_object(*r)
            .map_err(|e| format!("Failed to resolve Pages.Kids reference: {}", e))?,
        other => other,
    };

    let page_refs = kids
        .as_array()
        .map(|s| {
            s.iter()
                .filter_map(|k| k.as_reference().ok())
                .collect::<Vec<_>>()
        })
        .map_err(|_| "Pages.Kids is not an array".to_string())?;

    for r in page_refs {
        if !visited.insert(r) {
            warnings.push(PdfWarnMsg::warning(
                0,
                0,
                format!(
                    "Pages tree contains object {:?} more than once, skipping",
                    r
                ),
            ));
            continue;
        }
        let kid_obj = match doc.get_object(r) {
            Ok(o) => o,
            Err(e) => {
                warnings.push(PdfWarnMsg::warning(
                    0,
                    0,
                    format!("Failed to get pages-tree kid object {:?}: {}", r, e),
                ));
                continue;
            }
        };

        if let Ok(kid_dict) = kid_obj.as_dict() {
            // /Type is required by the spec but missing in plenty of real
            // documents; infer the node kind from the presence of /Kids.
            let type_name = match kid_dict.get(b"Type") {
                Ok(Object::Name(t)) => Some(t.clone()),
                _ => None,
            };
            match type_name.as_deref() {
                Some(b"Page") => pages.push(r),
                Some(b"Pages") => {
                    collect_page_refs_inner(kid_dict, doc, pages, visited, warnings, depth + 1)?
                }
                Some(other) => {
                    warnings.push(PdfWarnMsg::warning(
                        0,
                        0,
                        format!(
                            "Unknown pages-tree kid type {:?}, skipping",
                            String::from_utf8_lossy(other)
                        ),
                    ));
                }
                None => {
                    if kid_dict.has(b"Kids") {
                        collect_page_refs_inner(kid_dict, doc, pages, visited, warnings, depth + 1)?
                    } else {
                        warnings.push(PdfWarnMsg::warning(
                            0,
                            0,
                            format!("Pages-tree kid {:?} missing /Type, treating as /Page", r),
                        ));
                        pages.push(r);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Look up an inheritable page attribute (`MediaBox`, `CropBox`, `Resources`,
/// `Rotate`, ...): first on the page dictionary itself, then up the `/Parent`
/// chain (ISO 32000-1, 7.7.3.4). Depth-capped to survive cyclic parent links.
fn get_inheritable_page_attr<'a>(
    page: &'a LopdfDictionary,
    doc: &'a LopdfDocument,
    key: &[u8],
) -> Option<&'a Object> {
    let mut current = page;
    for _ in 0..64 {
        if let Ok(obj) = current.get(key) {
            return Some(obj);
        }
        let parent = current.get(b"Parent").ok()?;
        let parent_ref = parent.as_reference().ok()?;
        current = doc.get_dictionary(parent_ref).ok()?;
    }
    None
}

/// Append the bytes of one content stream, given an entry of the `/Contents`
/// value (reference or inline stream). Warns instead of failing on bad
/// entries: a partly readable page is better than no document.
fn append_content_stream(
    doc: &LopdfDocument,
    obj: &Object,
    page_num: usize,
    content_data: &mut Vec<u8>,
    warnings: &mut Vec<PdfWarnMsg>,
) {
    let stream = match obj {
        Object::Reference(r) => match doc.get_object(*r).and_then(|o| o.as_stream()) {
            Ok(s) => s,
            Err(e) => {
                warnings.push(PdfWarnMsg::error(
                    page_num,
                    0,
                    format!("Failed to resolve content stream {:?}: {}", r, e),
                ));
                return;
            }
        },
        Object::Stream(s) => s,
        _other => {
            warnings.push(PdfWarnMsg::error(
                page_num,
                0,
                "Content entry is not a stream".to_string(),
            ));
            return;
        }
    };
    // The streams of one page share the limit of a stream.
    let limit = MAX_STREAM_BYTES.saturating_sub(content_data.len());
    let Some(data) = decode_stream(stream, limit, "Page content", page_num, warnings) else {
        return;
    };
    content_data.extend(data);
    // PDF 32000-1, 7.8.2: when /Contents is an array, the division between
    // streams can fall in the middle of a token; the streams must be parsed
    // as if separated by (at least) a byte of whitespace.
    content_data.push(b'\n');
}

/// Parses a single page dictionary into a PdfPage.
fn parse_page(
    num: usize,
    page: &LopdfDictionary,
    doc: &LopdfDocument,
    xobjects: &BTreeMap<XObjectId, XObject>,
    font_decode: &BTreeMap<String, FontTextDecodeKind>,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Result<PdfPage, String> {
    // MediaBox is required but may be inherited from the parent Pages node. Without one, fall
    // back to A4 with a warning.
    let media_box = match get_inheritable_page_attr(page, doc, b"MediaBox") {
        Some(obj) => match parse_rect(obj) {
            Ok(r) => r,
            Err(e) => {
                warnings.push(PdfWarnMsg::warning(
                    num,
                    0,
                    format!("Invalid MediaBox: {e}; defaulting to A4"),
                ));
                default_media_box()
            }
        },
        None => {
            warnings.push(PdfWarnMsg::warning(
                num,
                0,
                "Page missing MediaBox (also not inherited); defaulting to A4".to_string(),
            ));
            default_media_box()
        }
    };
    // TrimBox and CropBox are optional (and inheritable); use MediaBox as default.
    let trim_box = match get_inheritable_page_attr(page, doc, b"TrimBox").map(parse_rect) {
        Some(Ok(r)) => r,
        _ => media_box.clone(),
    };
    let crop_box = match get_inheritable_page_attr(page, doc, b"CropBox").map(parse_rect) {
        Some(Ok(r)) => r,
        _ => media_box.clone(),
    };

    // Get the Contents entry (could be a reference, an array, or a stream).
    // Contents is *optional* — a page without it is simply empty.
    let mut content_data = Vec::new();
    match page.get(b"Contents") {
        Ok(Object::Array(arr)) => {
            for obj in arr {
                append_content_stream(doc, obj, num, &mut content_data, warnings);
            }
        }
        Ok(Object::Reference(r)) => {
            // A /Contents reference may point at a stream *or* at an array of
            // stream references.
            match doc.get_object(*r) {
                Ok(Object::Array(arr)) => {
                    for obj in arr {
                        append_content_stream(doc, obj, num, &mut content_data, warnings);
                    }
                }
                Ok(other) => {
                    append_content_stream(doc, other, num, &mut content_data, warnings);
                }
                Err(e) => {
                    warnings.push(PdfWarnMsg::error(
                        num,
                        0,
                        format!("Failed to get content stream: {}", e),
                    ));
                }
            }
        }
        Ok(other) => {
            append_content_stream(doc, other, num, &mut content_data, warnings);
        }
        Err(_) => {
            warnings.push(PdfWarnMsg::info(
                num,
                0,
                "Page has no /Contents (empty page)".to_string(),
            ));
        }
    }

    // A page whose content stream fails to tokenize becomes an empty page
    // with a warning instead of failing the entire document.
    let ops = match decode_content(&content_data) {
        Ok(operations) => convert_operations(num, &operations, xobjects, font_decode, warnings)?,
        Err(e) => {
            warnings.push(PdfWarnMsg::error(num, 0, e));
            Vec::new()
        }
    };

    Ok(PdfPage {
        media_box,
        trim_box,
        crop_box,
        ops,
    })
}

/// Splits the bytes of a content stream (a page's or a form's) into operators and operands.
fn decode_content(bytes: &[u8]) -> Result<Vec<lopdf::content::Operation>, String> {
    lopdf::content::Content::decode(bytes)
        .map(|content| content.operations)
        .map_err(|e| format!("Failed to decode content stream: {}", e))
}

/// Turns the operators of a content stream into [`Op`]s. `font_decode` says how the text of each
/// font resource name decodes: it is the scope the content stream names its resources in, the
/// resources of the document for a page, and the ones of the form for a form. `page` is the page
/// the warnings are about.
fn convert_operations(
    page: usize,
    operations: &[lopdf::content::Operation],
    xobjects: &BTreeMap<XObjectId, XObject>,
    font_decode: &BTreeMap<String, FontTextDecodeKind>,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Result<Vec<Op>, String> {
    let mut state = PageState {
        font_decode: font_decode.clone(),
        ..PageState::default()
    };
    let mut ops = Vec::new();
    for (op_id, op) in operations.iter().enumerate() {
        ops.extend(parse_op(page, op_id, op, &mut state, xobjects, warnings)?);
    }
    Ok(ops)
}

/// A4 portrait in PDF points, the fallback when a document specifies no
/// usable MediaBox anywhere.
fn default_media_box() -> Rect {
    Rect::from_xywh(Pt(0.0), Pt(0.0), Pt(595.28), Pt(841.89))
}

/// State carried across operators while one page's content stream is converted by
/// [`parse_op`]: the current font, text position, open path and so on.
#[derive(Debug, Clone, Default)]
pub struct PageState {
    /// True if we are inside a `BT ... ET` text block
    pub in_text_mode: bool,

    /// Current font resource and size (only relevant if `in_text_mode` = true)
    pub current_font: Option<crate::BuiltinOrExternalFontId>,

    /// Size selected by the last `Tf`.
    pub current_font_size: Option<crate::units::Pt>,

    /// Current font resource name (e.g., "F5") - tracked only to emit warnings if text appears without Tf
    pub current_font_resource: Option<String>,

    /// How text-showing operator bytes decode for each font resource name of
    /// this page's resources. Filled in by `parse_page` before the ops run.
    pub font_decode: BTreeMap<String, FontTextDecodeKind>,

    /// The decode kind of the currently selected font (set by `Tf`).
    pub current_font_decode: Option<FontTextDecodeKind>,

    /// Current transformation matrix stack. Each entry is a 6-float array [a b c d e f]. The
    /// stack has a depth limit; `q` operators beyond it are counted in
    /// [`PageState::untracked_saves`].
    pub transform_stack: Vec<[f32; 6]>,

    /// Number of open `q` operators beyond the depth limit of [`PageState::transform_stack`]. A
    /// `Q` ends one of these before it pops the stack.
    pub untracked_saves: usize,

    /// Absolute translation of the text line matrix. `Td` is relative, so it is accumulated here
    /// and emitted as an absolute [`Op::SetTextCursor`]. Reset by `BT`, set by `Tm`.
    pub text_line: [f32; 2],
    /// Text leading (from `TL`/`TD`), so `T*` advances the tracked line by the
    /// right amount.
    pub text_leading: f32,

    /// Name of the current marked content, if any (set by BMC/BDC). The stack has a depth limit;
    /// `BMC` and `BDC` operators beyond it are counted in
    /// [`PageState::untracked_marked_content`].
    pub current_marked_content: Vec<String>,

    /// Number of open `BMC` and `BDC` operators beyond the depth limit of
    /// [`PageState::current_marked_content`]. An `EMC` ends one of these before it pops the stack.
    pub untracked_marked_content: usize,

    /// The path under construction (`m`, `l`, `c`, `re`, ...), until a painting operator ends it.
    pub path_builder: PathBuilder,

    /// Font and size of the last `SetFont` op that was emitted.
    pub last_emitted_font_size: Option<(FontId, Pt)>,

    /// Holds a rectangle whose winding order is set by `W` and `W*`.
    pub rectangle_builder: RectangleBuilder,
}

// A content stream can open `q`, `BMC` and `BDC` without ever closing them. Past these depths the
// operators are only counted: their ops are still emitted and the matching `Q` and `EMC` still
// balance, but the stacks stay small. ISO 32000-1, Annex C, gives 28 as the `q` depth a reader has
// to support.
const MAX_GRAPHICS_STATE_DEPTH: usize = 256;
const MAX_MARKED_CONTENT_DEPTH: usize = 256;

/// Holds the rectangle that `W` and `W*` set the winding order of.
#[derive(Debug, Clone, Default)]
pub struct RectangleBuilder {
    rectangle: Option<Rect>,
}

impl RectangleBuilder {
    /// Replaces the held rectangle with one at `(x, y)` of the given size.
    pub fn new(&mut self, x: Pt, y: Pt, width: Pt, height: Pt) {
        self.rectangle = Some(Rect::from_xywh(x, y, width, height));
    }

    /// Sets the winding order of the held rectangle, if there is one.
    pub fn set_winding_order(&mut self, winding_order: WindingOrder) {
        if let Some(ref mut rectangle) = self.rectangle {
            rectangle.winding_order = Some(winding_order);
        }
    }

    /// [`Op::DrawRectangle`] for the held rectangle, or no ops when there is none.
    pub fn get_ops(&self) -> Vec<Op> {
        match &self.rectangle {
            Some(rectangle) => vec![Op::DrawRectangle {
                rectangle: rectangle.clone(),
            }],
            None => vec![],
        }
    }

    /// Drops the held rectangle.
    pub fn clear(&mut self) {
        self.rectangle = None;
    }
}

/// One segment of a subpath.
#[derive(Debug, Clone, PartialEq)]
pub enum PathOperation {
    /// Starts a new subpath at the point (`m`).
    MoveTo(Point),
    /// Straight line to the point (`l`).
    LineTo(Point),
    /// Cubic Bezier curve to `endpoint` (`c`, `v`, `y`).
    CurveTo {
        /// First control point.
        control1: Point,
        /// Second control point.
        control2: Point,
        /// End point of the curve.
        endpoint: Point,
    },
    /// Closes the subpath with a straight line to its start (`h`).
    ClosePath,
}

/// A subpath: a sequence of segments and whether it was closed with `h`.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfSubPath {
    operations: Vec<PathOperation>,
    is_closed: bool,
}

/// A path (list of subpaths) with the paint mode and winding order that were chosen by the
/// operator that ended it.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfPath {
    subpaths: Vec<PdfSubPath>,
    paint_mode: PaintMode,
    winding_order: WindingOrder,
}

/// Collects the path operators (`m`, `l`, `c`, `v`, `y`, `h`, `re`) of a content stream until a
/// painting operator turns them into a [`PdfPath`].
#[derive(Debug, Clone, Default)]
pub struct PathBuilder {
    current_subpath: Vec<PathOperation>,
    subpaths: Vec<Vec<PathOperation>>,
    current_point: Option<Point>,
    start_point: Option<Point>,
    /// True once `W` or `W*` has emitted the clip of the path under construction.
    clip_emitted: bool,
}

impl PathBuilder {
    /// `m`: starts a new subpath at `point`.
    pub fn move_to(&mut self, point: Point) {
        if !self.current_subpath.is_empty() {
            self.subpaths
                .push(std::mem::take(&mut self.current_subpath));
        }

        self.current_subpath.push(PathOperation::MoveTo(point));
        self.current_point = Some(point);
        self.start_point = Some(point);
    }

    /// `l`: line to `point`; starts a subpath there if there is no current point.
    pub fn line_to(&mut self, point: Point) {
        if self.current_point.is_none() {
            self.move_to(point);
            return;
        }

        self.current_subpath.push(PathOperation::LineTo(point));
        self.current_point = Some(point);
    }

    /// `c`: cubic curve with two control points.
    pub fn curve_to(&mut self, control1: Point, control2: Point, endpoint: Point) {
        if self.current_point.is_none() {
            self.move_to(control1);
            self.curve_to(control1, control2, endpoint);
            return;
        }

        self.current_subpath.push(PathOperation::CurveTo {
            control1,
            control2,
            endpoint,
        });
        self.current_point = Some(endpoint);
    }

    /// v-curve: current point is first control point
    pub fn v_curve_to(&mut self, control2: Point, endpoint: Point) {
        if let Some(current) = self.current_point {
            self.curve_to(current, control2, endpoint);
        } else {
            // If no current point, treat as regular curve_to
            self.move_to(control2);
            self.curve_to(control2, control2, endpoint);
        }
    }

    /// y-curve: endpoint is second control point
    pub fn y_curve_to(&mut self, control1: Point, endpoint: Point) {
        if self.current_point.is_none() {
            self.move_to(control1);
        }

        self.curve_to(control1, endpoint, endpoint);
    }

    /// `h`: closes the current subpath. Does nothing on an empty subpath.
    pub fn close_path(&mut self) {
        if self.current_subpath.is_empty() {
            return;
        }

        self.current_subpath.push(PathOperation::ClosePath);

        // Connect back to start point
        if let (Some(start), Some(current)) = (self.start_point, self.current_point) {
            if current != start {
                self.current_point = Some(start);
            }
        }
    }

    /// Forgets all subpaths and the current point.
    pub fn clear(&mut self) {
        self.current_subpath.clear();
        self.subpaths.clear();
        self.current_point = None;
        self.start_point = None;
        self.clip_emitted = false;
    }

    #[cfg(test)]
    pub fn finalize_subpath(&mut self) {
        if !self.current_subpath.is_empty() {
            self.subpaths
                .push(std::mem::take(&mut self.current_subpath));
            self.current_point = None;
            self.start_point = None;
        }
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.current_subpath.is_empty() && self.subpaths.is_empty()
    }

    /// The collected subpaths as a [`PdfPath`] with the given paint mode and winding order.
    pub fn build(&self, paint_mode: PaintMode, winding_order: WindingOrder) -> PdfPath {
        let mut all_subpaths = self.subpaths.clone();
        if !self.current_subpath.is_empty() {
            all_subpaths.push(self.current_subpath.clone());
        }

        let subpaths = all_subpaths
            .into_iter()
            .map(|ops| {
                let is_closed = ops.iter().any(|op| matches!(op, PathOperation::ClosePath));
                PdfSubPath {
                    operations: ops,
                    is_closed,
                }
            })
            .collect();

        PdfPath {
            subpaths,
            paint_mode,
            winding_order,
        }
    }
}

/// Converts a path to an op. A single open subpath that is only stroked becomes
/// [`Op::DrawLine`]; anything else becomes [`Op::DrawPolygon`] with one ring per subpath.
pub fn path_to_op(path: &PdfPath) -> Op {
    let mut all_rings = Vec::new();

    for subpath in &path.subpaths {
        let points = subpath_to_line_points(subpath);
        if !points.is_empty() {
            all_rings.push(PolygonRing { points });
        }
    }

    if matches!(path.subpaths.as_slice(), [only] if !only.is_closed)
        && path.paint_mode == PaintMode::Stroke
    {
        if let Some(ring) = all_rings.first() {
            return Op::DrawLine {
                line: Line {
                    points: ring.points.clone(),
                    is_closed: false,
                },
            };
        }
    }

    Op::DrawPolygon {
        polygon: Polygon {
            rings: all_rings,
            mode: path.paint_mode,
            winding_order: path.winding_order,
        },
    }
}

/// Parse a PDF content stream into path operations
#[cfg(test)]
fn parse_pdf_stream(stream: &str) -> Vec<(String, Vec<f32>)> {
    let mut operations = Vec::new();
    let mut current_operands = Vec::new();

    // Simple tokenizer for PDF path operations
    let tokens = stream.split_whitespace().peekable();

    for token in tokens {
        // Check if token is a number
        if let Ok(num) = token.parse::<f32>() {
            current_operands.push(num);
            continue;
        }

        // If not a number, it's an operator
        if token == "m"
            || token == "l"
            || token == "c"
            || token == "v"
            || token == "y"
            || token == "h"
            || token == "f"
            || token == "S"
            || token == "s"
            || token == "f*"
            || token == "B"
            || token == "B*"
            || token == "b"
            || token == "b*"
        {
            // Store the operation
            operations.push((token.to_string(), current_operands.clone()));
            current_operands = Vec::new();
        }
    }

    operations
}

/// Apply a parsed operation to the PathBuilder
#[cfg(test)]
fn apply_path_operation(builder: &mut PathBuilder, op: &(String, Vec<f32>)) {
    match op.0.as_str() {
        "m" => {
            if op.1.len() >= 2 {
                builder.move_to(Point {
                    x: Pt(op.1[0]),
                    y: Pt(op.1[1]),
                });
            }
        }
        "l" => {
            if op.1.len() >= 2 {
                builder.line_to(Point {
                    x: Pt(op.1[0]),
                    y: Pt(op.1[1]),
                });
            }
        }
        "c" => {
            if op.1.len() >= 6 {
                builder.curve_to(
                    Point {
                        x: Pt(op.1[0]),
                        y: Pt(op.1[1]),
                    },
                    Point {
                        x: Pt(op.1[2]),
                        y: Pt(op.1[3]),
                    },
                    Point {
                        x: Pt(op.1[4]),
                        y: Pt(op.1[5]),
                    },
                );
            }
        }
        "v" => {
            if op.1.len() >= 4 {
                builder.v_curve_to(
                    Point {
                        x: Pt(op.1[0]),
                        y: Pt(op.1[1]),
                    },
                    Point {
                        x: Pt(op.1[2]),
                        y: Pt(op.1[3]),
                    },
                );
            }
        }
        "y" => {
            if op.1.len() >= 4 {
                builder.y_curve_to(
                    Point {
                        x: Pt(op.1[0]),
                        y: Pt(op.1[1]),
                    },
                    Point {
                        x: Pt(op.1[2]),
                        y: Pt(op.1[3]),
                    },
                );
            }
        }
        "h" => {
            builder.close_path();
        }
        "f" | "S" | "s" | "f*" | "B" | "B*" | "b" | "b*" => {
            // These are painting operators, not path construction
            // For the test, we handle them by finalizing the current subpath
            builder.finalize_subpath();
        }
        _ => {}
    }
}

/// Convert a PdfSubPath to a sequence of LinePoints with proper bezier flags
fn subpath_to_line_points(subpath: &PdfSubPath) -> Vec<LinePoint> {
    let mut points = Vec::new();

    for operation in &subpath.operations {
        match operation {
            PathOperation::MoveTo(point) => {
                points.push(LinePoint {
                    p: *point,
                    bezier: false,
                });
            }
            PathOperation::LineTo(point) => {
                points.push(LinePoint {
                    p: *point,
                    bezier: false,
                });
            }
            PathOperation::CurveTo {
                control1,
                control2,
                endpoint,
            } => {
                // First control point - always bezier=true
                points.push(LinePoint {
                    p: *control1,
                    bezier: true,
                });
                // Second control point - always bezier=true
                points.push(LinePoint {
                    p: *control2,
                    bezier: true,
                });
                // Endpoint - always bezier=false
                points.push(LinePoint {
                    p: *endpoint,
                    bezier: false,
                });
            }
            PathOperation::ClosePath => {
                // Nothing to add for close path
            }
        }
    }

    points
}

#[test]
fn test_2() {
    let pdf_stream = "
    20.5 344.5 m
    20.5 344.5 22 333.5 10.5 346.5 c
    S"
    .trim();

    let expected_output = Op::DrawLine {
        line: Line {
            points: vec![
                LinePoint {
                    p: Point {
                        x: Pt(20.5),
                        y: Pt(344.5),
                    },
                    bezier: false,
                },
                LinePoint {
                    p: Point {
                        x: Pt(20.5),
                        y: Pt(344.5),
                    },
                    bezier: true,
                },
                LinePoint {
                    p: Point {
                        x: Pt(22.0),
                        y: Pt(333.5),
                    },
                    bezier: true,
                },
                LinePoint {
                    p: Point {
                        x: Pt(10.5),
                        y: Pt(346.5),
                    },
                    bezier: false,
                },
            ],
            is_closed: false,
        },
    };

    // Parse using the PathBuilder
    let mut builder = PathBuilder::default();
    let ops = parse_pdf_stream(pdf_stream);
    for op in ops {
        apply_path_operation(&mut builder, &op);
    }

    // Build path and generate commands
    let path = builder.build(PaintMode::Stroke, WindingOrder::NonZero);
    let draw_op = path_to_op(&path);

    // Verify bezier flags are correctly set
    assert_eq!(draw_op, expected_output);
}

#[test]
fn test_bezier_path_parsing() {
    // Test case from tiger.svg
    let pdf_stream = "-122.3 84.285 m -122.3 84.285 -122.2 86.179 -123.03 86.16 c -123.85 86.141 \
                      -140.3 38.066 -160.83 40.309 c -160.83 40.309 -143.05 32.956 -122.3 84.285 \
                      c h f";

    // Parse using the PathBuilder
    let mut builder = PathBuilder::default();
    let ops = parse_pdf_stream(pdf_stream);
    for op in ops {
        apply_path_operation(&mut builder, &op);
    }

    // Build path and generate commands
    let path = builder.build(PaintMode::Fill, WindingOrder::NonZero);
    let draw_op = path_to_op(&path);

    // Verify bezier flags are correctly set
    if let Op::DrawPolygon { polygon } = draw_op {
        assert_eq!(polygon.rings.len(), 1);
        assert_eq!(polygon.rings[0].points.len(), 10);

        // Check flags for points
        let points = &polygon.rings[0].points;
        assert!(!points[0].bezier); // Start point
        assert!(points[1].bezier); // Control point 1
        assert!(points[2].bezier); // Control point 2
        assert!(!points[3].bezier); // End point
        assert!(points[4].bezier); // Next control point 1
                                   // And so on...
    } else {
        panic!("Expected DrawPolygon");
    }
}

/// Decode the byte string of one text-showing operator according to the
/// currently selected font (see [`FontTextDecodeKind`]).
///
/// * Built-in / simple fonts: one byte per code → decoded to a
///   [`crate::text::TextItem::Text`] (ToUnicode CMap first, WinAnsi fallback),
///   which the serializer re-encodes correctly for whichever font ends up
///   selected at save time.
/// * Composite (Type0) fonts: two-byte big-endian CIDs → [`crate::text::TextItem::GlyphIds`]
///   with each glyph's Unicode string attached from the ToUnicode CMap, so
///   text extraction and regenerated ToUnicode CMaps stay correct.
/// * No font information: the string is decoded with
///   [`decode_tj_string_as_glyph_ids`](crate::text::decode_tj_string_as_glyph_ids).
fn decode_show_text_string(
    bytes: &[u8],
    kind: Option<&FontTextDecodeKind>,
    page: usize,
    op_id: usize,
    warnings: &mut Vec<PdfWarnMsg>,
) -> crate::text::TextItem {
    use crate::text::{decode_simple_font_bytes, Codepoint, TextItem};
    match kind {
        Some(FontTextDecodeKind::Builtin(_)) => {
            TextItem::Text(decode_simple_font_bytes(bytes, None))
        }
        Some(FontTextDecodeKind::Simple { to_unicode }) => {
            TextItem::Text(decode_simple_font_bytes(bytes, to_unicode.as_deref()))
        }
        Some(FontTextDecodeKind::Cid {
            to_unicode,
            encoding_cmap,
            cid_to_gid,
        }) => {
            // bytes -> codes: variable-width per the /Encoding CMap's codespace
            // ranges, or fixed 2-byte big-endian for Identity-H (`None`).
            let codes: Vec<(u32, u8)> = match encoding_cmap {
                Some(cmap) => cmap.split_codes(bytes),
                None => {
                    if !bytes.len().is_multiple_of(2) {
                        warnings.push(PdfWarnMsg::warning(
                            page,
                            op_id,
                            format!(
                                "CID font string has odd byte length {}, dropping \
                                 trailing byte",
                                bytes.len()
                            ),
                        ));
                    }
                    bytes
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| (u32::from(u16::from_be_bytes(*c)), 2))
                        .collect()
                }
            };
            let glyphs = codes
                .into_iter()
                .map(|(code, len)| {
                    // ToUnicode is keyed by the content-stream CODE; the glyph id
                    // stored on the Codepoint must be the real GID: code -> CID
                    // (encoding CMap) -> GID (CIDToGIDMap stream / CFF charset),
                    // so outlines, widths and re-subsetting stay correct.
                    let cid_str = to_unicode
                        .as_deref()
                        .and_then(|cmap| cmap.lookup_string(code));
                    let cid = encoding_cmap
                        .as_deref()
                        .map_or(code, |m| m.cid_for_code(code, len));
                    let cid = u16::try_from(cid).unwrap_or(0);
                    let gid = cid_to_gid
                        .as_deref()
                        .and_then(|m| m.get(&cid).copied())
                        .unwrap_or(cid);
                    match cid_str {
                        Some(s) => Codepoint::with_cid(gid, 0.0, s),
                        None => Codepoint::new(gid, 0.0),
                    }
                })
                .collect();
            TextItem::GlyphIds(glyphs)
        }
        None => {
            // No font information: text shown before any usable `Tf`, which is rare.
            let items = crate::text::decode_tj_string_as_glyph_ids(bytes);
            items
                .into_iter()
                .next()
                .unwrap_or(TextItem::GlyphIds(Vec::new()))
        }
    }
}

/// Decode a whole `TJ` array (strings interleaved with kerning offsets).
fn decode_show_text_array(
    arr: &[Object],
    kind: Option<&FontTextDecodeKind>,
    page: usize,
    op_id: usize,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Vec<crate::text::TextItem> {
    use crate::text::TextItem;
    let mut items = Vec::new();
    for obj in arr {
        match obj {
            Object::String(bytes, _) => {
                items.push(decode_show_text_string(bytes, kind, page, op_id, warnings));
            }
            Object::Integer(i) => items.push(TextItem::Offset(*i as f32)),
            Object::Real(r) => items.push(TextItem::Offset(*r)),
            _ => {}
        }
    }
    items
}

/// Decode a text-showing string to a plain String (for the `'` and `"`
/// operators, whose printpdf ops carry `String` text).
fn decode_show_text_to_string(
    bytes: &[u8],
    kind: Option<&FontTextDecodeKind>,
    page: usize,
    op_id: usize,
    warnings: &mut Vec<PdfWarnMsg>,
) -> String {
    use crate::text::TextItem;
    match decode_show_text_string(bytes, kind, page, op_id, warnings) {
        TextItem::Text(s) => s,
        TextItem::GlyphIds(glyphs) => glyphs
            .iter()
            .map(|g| g.cid.clone().unwrap_or_else(|| '\u{FFFD}'.to_string()))
            .collect(),
        TextItem::Offset(_) => String::new(),
    }
}

/// Records an opened marked content sequence (`BMC`, `BDC`) on the stack of `state`. Past the
/// depth limit it is only counted, with one warning where the limit is crossed.
fn open_marked_content(
    state: &mut PageState,
    tag: String,
    page: usize,
    op_id: usize,
    warnings: &mut Vec<PdfWarnMsg>,
) {
    if state.current_marked_content.len() < MAX_MARKED_CONTENT_DEPTH {
        state.current_marked_content.push(tag);
        return;
    }
    if state.untracked_marked_content == 0 {
        warnings.push(PdfWarnMsg::warning(
            page,
            op_id,
            format!(
                "Marked content nested deeper than {MAX_MARKED_CONTENT_DEPTH} levels; \
                 the nesting is no longer tracked"
            ),
        ));
    }
    state.untracked_marked_content = state.untracked_marked_content.saturating_add(1);
}

/// Converts one content stream operator into zero or more [`Op`]s.
///
/// `state` carries the current font, text position and open path between calls: path operators
/// (`m`, `l`, `c`, ...) accumulate in it and an op is emitted only when a painting operator
/// (`S`, `f`, `B`, ...) ends the path. `page` and `op_id` are used for warnings only. A malformed
/// operator pushes a warning and produces no op; an unknown one becomes [`Op::Unknown`]. The
/// `Result` is `Err` for no case today. `_xobjects` is unused.
pub fn parse_op(
    page: usize,
    op_id: usize,
    op: &lopdf::content::Operation,
    state: &mut PageState,
    _xobjects: &BTreeMap<XObjectId, XObject>,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Result<Vec<Op>, String> {
    use crate::units::Pt;
    let mut out_ops = Vec::new();
    match op.operator.as_str() {
        "q" => {
            if state.transform_stack.len() < MAX_GRAPHICS_STATE_DEPTH {
                let top = state
                    .transform_stack
                    .last()
                    .copied()
                    .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                state.transform_stack.push(top);
            } else {
                if state.untracked_saves == 0 {
                    warnings.push(PdfWarnMsg::warning(
                        page,
                        op_id,
                        format!(
                            "'q' nested deeper than {MAX_GRAPHICS_STATE_DEPTH} levels; \
                             the nesting is no longer tracked"
                        ),
                    ));
                }
                state.untracked_saves = state.untracked_saves.saturating_add(1);
            }
            out_ops.push(Op::SaveGraphicsState);
        }
        "Q" => {
            if state.untracked_saves > 0 {
                state.untracked_saves = state.untracked_saves.saturating_sub(1);
            } else if state.transform_stack.pop().is_none() {
                // Unbalanced `Q`: warn, and still emit the op.
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'Q' with empty transform stack".to_string(),
                ));
            }
            out_ops.push(Op::RestoreGraphicsState);
        }
        "MP" => {
            let [operand] = op.operands.as_slice() else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "MP expects 1 operand".to_string(),
                ));
                return Ok(Vec::new());
            };

            let id = match as_name(operand) {
                Some(name) => name,
                None => {
                    warnings.push(PdfWarnMsg::error(
                        page,
                        op_id,
                        "MP operand is not a name".to_string(),
                    ));
                    return Ok(Vec::new());
                }
            };

            out_ops.push(Op::Marker { id });
        }
        "CS" => {
            let [operand] = op.operands.as_slice() else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "CS expects 1 operand".to_string(),
                ));
                return Ok(Vec::new());
            };

            let id = match as_name(operand) {
                Some(name) => name,
                None => {
                    warnings.push(PdfWarnMsg::error(
                        page,
                        op_id,
                        "CS operand is not a name".to_string(),
                    ));
                    return Ok(Vec::new());
                }
            };

            out_ops.push(Op::SetColorSpaceStroke { id });
        }
        "cs" => {
            let [operand] = op.operands.as_slice() else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "cs expects 1 operand".to_string(),
                ));
                return Ok(Vec::new());
            };

            let id = match as_name(operand) {
                Some(name) => name,
                None => {
                    warnings.push(PdfWarnMsg::error(
                        page,
                        op_id,
                        "cs operand is not a name".to_string(),
                    ));
                    return Ok(Vec::new());
                }
            };

            out_ops.push(Op::SetColorSpaceFill { id });
        }
        "ri" => {
            let [operand] = op.operands.as_slice() else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "ri expects 1 operand".to_string(),
                ));
                return Ok(Vec::new());
            };
            let intent = match as_name(operand) {
                Some(i) => i,
                None => {
                    warnings.push(PdfWarnMsg::error(
                        page,
                        op_id,
                        "ri operand is not a name".to_string(),
                    ));
                    return Ok(Vec::new());
                }
            };
            let intent = match intent.as_str() {
                "AbsoluteColorimetric" => RenderingIntent::AbsoluteColorimetric,
                "RelativeColorimetric" => RenderingIntent::RelativeColorimetric,
                "Saturation" => RenderingIntent::Saturation,
                "Perceptual" => RenderingIntent::Perceptual,
                other => {
                    warnings.push(PdfWarnMsg::error(
                        page,
                        op_id,
                        format!(
                            "Unknown rendering intent '{}', defaulting to RelativeColorimetric",
                            other
                        ),
                    ));
                    RenderingIntent::RelativeColorimetric
                }
            };
            out_ops.push(Op::SetRenderingIntent { intent });
        }
        "rg" => {
            if let [r, g, b] = op.operands.as_slice() {
                let r = to_f32(r);
                let g = to_f32(g);
                let b = to_f32(b);
                out_ops.push(Op::SetFillColor {
                    col: Color::Rgb(crate::Rgb {
                        r,
                        g,
                        b,
                        icc_profile: None,
                    }),
                });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'rg' expects 3 operands".to_string(),
                ));
            }
        }
        "RG" => {
            if let [r, g, b] = op.operands.as_slice() {
                let r = to_f32(r);
                let g = to_f32(g);
                let b = to_f32(b);
                out_ops.push(Op::SetOutlineColor {
                    col: Color::Rgb(crate::Rgb {
                        r,
                        g,
                        b,
                        icc_profile: None,
                    }),
                });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'RG' expects 3 operands".to_string(),
                ));
            }
        }
        "k" => {
            if let [c, m, y, k] = op.operands.as_slice() {
                let c = to_f32(c);
                let m = to_f32(m);
                let y = to_f32(y);
                let k = to_f32(k);
                out_ops.push(Op::SetFillColor {
                    col: Color::Cmyk(crate::Cmyk {
                        c,
                        m,
                        y,
                        k,
                        icc_profile: None,
                    }),
                });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'k' expects 4 operands".to_string(),
                ));
            }
        }
        "K" => {
            if let [c, m, y, k] = op.operands.as_slice() {
                let c = to_f32(c);
                let m = to_f32(m);
                let y = to_f32(y);
                let k = to_f32(k);
                out_ops.push(Op::SetOutlineColor {
                    col: Color::Cmyk(crate::Cmyk {
                        c,
                        m,
                        y,
                        k,
                        icc_profile: None,
                    }),
                });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'K' expects 4 operands".to_string(),
                ));
            }
        }
        "g" => {
            if let [gray] = op.operands.as_slice() {
                let gray = to_f32(gray);
                out_ops.push(Op::SetFillColor {
                    col: Color::Greyscale(crate::Greyscale {
                        percent: gray,
                        icc_profile: None,
                    }),
                });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'g' expects 1 operand".to_string(),
                ));
            }
        }
        "G" => {
            if let [gray] = op.operands.as_slice() {
                let gray = to_f32(gray);
                out_ops.push(Op::SetOutlineColor {
                    col: Color::Greyscale(crate::Greyscale {
                        percent: gray,
                        icc_profile: None,
                    }),
                });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'G' expects 1 operand".to_string(),
                ));
            }
        }

        "TJ" => {
            if !state.in_text_mode {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'TJ' outside of text mode!".to_string(),
                ));
            }

            if op.operands.is_empty() {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'TJ' with no operands".to_string(),
                ));
            } else if let Some(arr) = op.operands.first().and_then(|o| o.as_array().ok()) {
                // Warn if no font was set via Tf operator
                if state.current_font_resource.is_none() {
                    warnings.push(PdfWarnMsg::warning(
                        page,
                        op_id,
                        "Warning: 'TJ' without prior 'Tf' (font setup). Renderer will use default font.".to_string(),
                    ));
                }

                // Decode according to the *current font*: 1-byte WinAnsi codes
                // for builtin/simple fonts, 2-byte CIDs for composite fonts.
                let text_items = decode_show_text_array(
                    arr,
                    state.current_font_decode.as_ref(),
                    page,
                    op_id,
                    warnings,
                );

                out_ops.push(Op::ShowText { items: text_items });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'TJ' operand is not an array".to_string(),
                ));
            }
        }

        "Tj" => {
            if !state.in_text_mode {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'Tj' outside of text mode!".to_string(),
                ));
            }

            if op.operands.is_empty() {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'Tj' with no operands".to_string(),
                ));
            } else if let Some(lopdf::Object::String(bytes, _format)) = op.operands.first() {
                // Warn if no font was set via Tf operator
                if state.current_font_resource.is_none() {
                    warnings.push(PdfWarnMsg::warning(
                        page,
                        op_id,
                        "Warning: 'Tj' without prior 'Tf' (font setup). Renderer will use default font.".to_string(),
                    ));
                }

                // Decode according to the current font (see decode_show_text_string).
                let text_items = vec![decode_show_text_string(
                    bytes,
                    state.current_font_decode.as_ref(),
                    page,
                    op_id,
                    warnings,
                )];

                out_ops.push(Op::ShowText { items: text_items });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'Tj' operand is not string".to_string(),
                ));
            }
        }
        "T*" => {
            // Move to the start of the next line: keep x, drop y by the leading.
            state.text_line[1] -= state.text_leading;
            out_ops.push(Op::AddLineBreak);
        }
        "TL" => {
            if let [operand] = op.operands.as_slice() {
                let val = to_f32(operand);
                state.text_leading = val;
                out_ops.push(Op::SetLineHeight { lh: Pt(val) });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    format!("Warning: 'TL' expects 1 operand, got {}", op.operands.len()),
                ));
            }
        }
        "Ts" => {
            if let [operand] = op.operands.as_slice() {
                let rise = to_f32(operand);
                out_ops.push(Op::SetLineOffset { multiplier: rise });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    format!("Warning: 'Ts' expects 1 operand, got {}", op.operands.len()),
                ));
            }
        }
        "Tw" => {
            if let [operand] = op.operands.as_slice() {
                let spacing = to_f32(operand);
                out_ops.push(Op::SetWordSpacing { pt: Pt(spacing) });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    format!("Warning: 'Tw' expects 1 operand, got {}", op.operands.len()),
                ));
            }
        }
        "Tz" => {
            if let [operand] = op.operands.as_slice() {
                let scale_percent = to_f32(operand);
                out_ops.push(Op::SetHorizontalScaling {
                    percent: scale_percent,
                });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    format!("Warning: 'Tz' expects 1 operand, got {}", op.operands.len()),
                ));
            }
        }
        "TD" => {
            if let [tx, ty] = op.operands.as_slice() {
                // TD = Td + set leading to -ty. Relative move; track it and the leading.
                let tx = to_f32(tx);
                let ty = to_f32(ty);
                state.text_line[0] += tx;
                state.text_line[1] += ty;
                state.text_leading = -ty;
                out_ops.push(Op::MoveTextCursorAndSetLeading { tx, ty });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    format!("'TD' expects 2 operands, got {}", op.operands.len()),
                ));
            }
        }
        "Tm" => {
            if let [a, b, c, d, e, f] = op.operands.as_slice() {
                let a = to_f32(a);
                let b = to_f32(b);
                let c = to_f32(c);
                let d = to_f32(d);
                let e = to_f32(e);
                let f = to_f32(f);
                // Tm sets the text line matrix absolutely; track its translation
                // so a following relative `Td` accumulates from here.
                state.text_line = [e, f];
                out_ops.push(Op::SetTextMatrix {
                    matrix: TextMatrix::Raw([a, b, c, d, e, f]),
                });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'Tm' expects 6 operands".to_string(),
                ));
            }
        }
        "Tc" => {
            let [operand] = op.operands.as_slice() else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Tc expects 1 operand".to_string(),
                ));
                return Ok(Vec::new());
            };
            let spacing = to_f32(operand);
            out_ops.push(Op::SetCharacterSpacing {
                multiplier: spacing,
            });
        }
        "Tr" => {
            let [operand] = op.operands.as_slice() else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Tr expects 1 operand".to_string(),
                ));
                return Ok(Vec::new());
            };
            let mode = match operand {
                Object::Integer(i) => *i,
                Object::Real(r) => *r as i64,
                _ => {
                    warnings.push(PdfWarnMsg::error(
                        page,
                        op_id,
                        "Tr operand is not numeric".to_string(),
                    ));
                    return Ok(Vec::new());
                }
            };
            out_ops.push(Op::SetTextRenderingMode {
                mode: TextRenderingMode::from_i64(mode),
            });
        }
        "gs" => {
            let [operand] = op.operands.as_slice() else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "gs expects 1 operand".to_string(),
                ));
                return Ok(Vec::new());
            };

            let gs_name = match as_name(operand) {
                Some(name) => name,
                None => {
                    warnings.push(PdfWarnMsg::error(
                        page,
                        op_id,
                        "gs operand is not a name".to_string(),
                    ));
                    return Ok(Vec::new());
                }
            };

            out_ops.push(Op::LoadGraphicsState {
                gs: ExtendedGraphicsStateId(gs_name),
            });
        }

        "BT" => {
            // `BT` resets the text matrix and the text line matrix.
            state.in_text_mode = true;
            state.text_line = [0.0, 0.0];
            out_ops.push(Op::StartTextSection);
        }
        "ET" => {
            state.in_text_mode = false;
            out_ops.push(Op::EndTextSection);
        }

        "Tf" => {
            if let [font, size] = op.operands.as_slice() {
                if let Some(font_name) = as_name(font) {
                    state.current_font = Some(BuiltinOrExternalFontId::from_str(&font_name));
                    state.current_font_resource = Some(font_name.clone());
                    // The document's own font resources win over the F1-F14 builtin naming: a
                    // foreign PDF may call an embedded font "F1".
                    state.current_font_decode = match state.font_decode.get(&font_name) {
                        Some(kind) => Some(kind.clone()),
                        None => BuiltinFont::from_id(&font_name).map(FontTextDecodeKind::Builtin),
                    };
                }
                let size_val = to_f32(size);
                state.current_font_size = Some(crate::units::Pt(size_val));

                if let (Some(font_resource), Some(sz)) =
                    (&state.current_font_resource, &state.current_font_size)
                {
                    // Builtin or external is decided by the resolved decode
                    // kind (falling back to the F1-F14 name heuristic when the
                    // document's resources don't describe the font).
                    let font_handle = match &state.current_font_decode {
                        Some(FontTextDecodeKind::Builtin(b)) => {
                            crate::ops::PdfFontHandle::Builtin(*b)
                        }
                        Some(_) => crate::ops::PdfFontHandle::External(crate::FontId(
                            font_resource.clone(),
                        )),
                        None => {
                            if let Some(builtin) = BuiltinFont::from_id(font_resource) {
                                crate::ops::PdfFontHandle::Builtin(builtin)
                            } else {
                                crate::ops::PdfFontHandle::External(crate::FontId(
                                    font_resource.clone(),
                                ))
                            }
                        }
                    };

                    out_ops.push(Op::SetFont {
                        font: font_handle,
                        size: *sz,
                    });
                }
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    format!(
                        "Warning: 'Tf' expects 2 operands, got {}",
                        op.operands.len()
                    ),
                ));
                out_ops.push(Op::Unknown {
                    key: "Tf".into(),
                    value: op.operands.iter().map(DictItem::from_lopdf).collect(),
                });
            }
        }

        "Td" => {
            if let [tx, ty] = op.operands.as_slice() {
                // `Td` is relative to the text line matrix: accumulate it and emit an absolute
                // cursor, since `SetTextCursor` is absolute.
                let tx = to_f32(tx);
                let ty = to_f32(ty);
                state.text_line[0] += tx;
                state.text_line[1] += ty;
                out_ops.push(Op::SetTextCursor {
                    pos: crate::graphics::Point {
                        x: crate::units::Pt(state.text_line[0]),
                        y: crate::units::Pt(state.text_line[1]),
                    },
                });
            }
        }

        // Marked content (`BMC`, `BDC`, `EMC`). `BDC /OC /Name` is optional content (a layer).
        // Marked content can nest.
        "BDC" => {
            // Typically something like: [Name("Span"), Properties as a Dictionary]
            // In case of optional content: [Name("OC"), Name("MyLayer")]
            let [tag, properties] = op.operands.as_slice() else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "BDC expects 2 operands".to_string(),
                ));
                return Ok(Vec::new());
            };
            let Some(marked_content_nm) = as_name(tag) else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "BDC operand is not a name".to_string(),
                ));
                return Ok(Vec::new());
            };
            if marked_content_nm.as_str() == "OC" {
                let Some(layer_nm) = as_name(properties) else {
                    warnings.push(PdfWarnMsg::error(
                        page,
                        op_id,
                        "BDC OC layer operand is not a name".to_string(),
                    ));
                    return Ok(Vec::new());
                };
                // `BeginLayer` rather than `BeginOptionalContent`: the serializer builds the
                // page's /Properties entries only from `BeginLayer` ops. Both write the same bytes.
                out_ops.push(Op::BeginLayer {
                    layer_id: crate::LayerInternalId(layer_nm),
                });
            } else {
                match properties.as_dict() {
                    Ok(dict) => {
                        out_ops.push(Op::BeginMarkedContentWithProperties {
                            tag: marked_content_nm.clone(),
                            properties: DictItem::from_lopdf(&Object::Dictionary(dict.to_owned())),
                        });
                    }
                    Err(_err) => {
                        warnings.push(PdfWarnMsg::warning(
                            page,
                            op_id,
                            "BDC properties is not a dictionary".to_string(),
                        ));
                        out_ops.push(Op::Unknown {
                            key: "BDC".into(),
                            value: op.operands.iter().map(DictItem::from_lopdf).collect(),
                        });
                    }
                }
            }
            open_marked_content(state, marked_content_nm, page, op_id, warnings);
        }
        "BMC" => {
            let [operand] = op.operands.as_slice() else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "BMC expects 1 operand".to_string(),
                ));
                return Ok(Vec::new());
            };
            let marked_content_nm = match as_name(operand) {
                Some(t) => t,
                None => {
                    warnings.push(PdfWarnMsg::error(
                        page,
                        op_id,
                        "BMC operand is not a name".to_string(),
                    ));
                    return Ok(Vec::new());
                }
            };
            out_ops.push(Op::BeginMarkedContent {
                tag: marked_content_nm.clone(),
            });
            open_marked_content(state, marked_content_nm, page, op_id, warnings);
        }
        "EMC" => {
            if state.untracked_marked_content > 0 {
                state.untracked_marked_content = state.untracked_marked_content.saturating_sub(1);
                out_ops.push(Op::EndMarkedContent {});
            } else if let Some(_marked_content_str) = state.current_marked_content.pop() {
                out_ops.push(Op::EndMarkedContent {});
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'EMC' with no current_marked_content".to_string(),
                ));
            }
        }

        "cm" => {
            if let [a, b, c, d, e, f] = op.operands.as_slice() {
                let floats = [to_f32(a), to_f32(b), to_f32(c), to_f32(d), to_f32(e), to_f32(f)];
                if let Some(top) = state.transform_stack.last_mut() {
                    // multiply top by these floats
                    let combined = crate::matrix::CurTransMat::combine_matrix(*top, floats);
                    *top = combined;
                }
                out_ops.push(Op::SetTransformationMatrix {
                    matrix: crate::matrix::CurTransMat::Raw(floats),
                });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: 'cm' expects 6 floats".to_string(),
                ));
            }
        }

        "m" => {
            // Start a new subpath
            let x = to_f32(op.operands.first().unwrap_or(&lopdf::Object::Null));
            let y = to_f32(op.operands.get(1).unwrap_or(&lopdf::Object::Null));
            state.path_builder.move_to(Point { x: Pt(x), y: Pt(y) });
        }
        "l" => {
            let x = to_f32(op.operands.first().unwrap_or(&lopdf::Object::Null));
            let y = to_f32(op.operands.get(1).unwrap_or(&lopdf::Object::Null));
            state.path_builder.line_to(Point { x: Pt(x), y: Pt(y) });
        }
        "c" => {
            // c x1 y1 x2 y2 x3 y3
            if let [x1, y1, x2, y2, x3, y3] = op.operands.as_slice() {
                let x1 = to_f32(x1);
                let y1 = to_f32(y1);
                let x2 = to_f32(x2);
                let y2 = to_f32(y2);
                let x3 = to_f32(x3);
                let y3 = to_f32(y3);

                state.path_builder.curve_to(
                    Point {
                        x: Pt(x1),
                        y: Pt(y1),
                    },
                    Point {
                        x: Pt(x2),
                        y: Pt(y2),
                    },
                    Point {
                        x: Pt(x3),
                        y: Pt(y3),
                    },
                );
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    format!("'c' expects 6 operands, got {}", op.operands.len()),
                ));
            }
        }
        "v" => {
            // v x2 y2 x3 y3
            // The first control point is implied to be the current point
            if let [x2, y2, x3, y3] = op.operands.as_slice() {
                let x2 = to_f32(x2);
                let y2 = to_f32(y2);
                let x3 = to_f32(x3);
                let y3 = to_f32(y3);

                state.path_builder.v_curve_to(
                    Point {
                        x: Pt(x2),
                        y: Pt(y2),
                    },
                    Point {
                        x: Pt(x3),
                        y: Pt(y3),
                    },
                );
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    format!("'v' expects 4 operands, got {}", op.operands.len()),
                ));
            }
        }
        "y" => {
            // y x1 y1 x3 y3
            // The second control point is implied to be the final point
            if let [x1, y1, x3, y3] = op.operands.as_slice() {
                let x1 = to_f32(x1);
                let y1 = to_f32(y1);
                let x3 = to_f32(x3);
                let y3 = to_f32(y3);

                state.path_builder.y_curve_to(
                    Point {
                        x: Pt(x1),
                        y: Pt(y1),
                    },
                    Point {
                        x: Pt(x3),
                        y: Pt(y3),
                    },
                );
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    format!("'y' expects 4 operands, got {}", op.operands.len()),
                ));
            }
        }
        "h" => {
            // closepath, i.e. connect last point to first point
            state.path_builder.close_path();
        }
        "re" => {
            if let [x, y, width, height] = op.operands.as_slice() {
                let x = Pt(to_f32(x));
                let y = Pt(to_f32(y));
                let width = Pt(to_f32(width));
                let height = Pt(to_f32(height));
                // `re` appends a closed rectangle to the current path (ISO 32000-1, 8.5.2.1); the
                // operator after it (`f`, `S`, `B`, `W n`) fills, strokes or clips it.
                let corner = |dx: f32, dy: f32| Point {
                    x: Pt(x.0 + dx),
                    y: Pt(y.0 + dy),
                };
                state.path_builder.move_to(corner(0.0, 0.0));
                state.path_builder.line_to(corner(width.0, 0.0));
                state.path_builder.line_to(corner(width.0, height.0));
                state.path_builder.line_to(corner(0.0, height.0));
                state.path_builder.close_path();
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "re expects 4 operands".to_string(),
                ));
            }
        }
        "S" => {
            // Stroke - doesn't close the path
            let path = state
                .path_builder
                .build(PaintMode::Stroke, WindingOrder::NonZero);
            if !path.subpaths.is_empty() {
                out_ops.push(path_to_op(&path));
            }
            state.path_builder.clear();
        }
        "f" => {
            // Fill with non-zero winding rule
            let path = state
                .path_builder
                .build(PaintMode::Fill, WindingOrder::NonZero);
            if !path.subpaths.is_empty() {
                out_ops.push(path_to_op(&path));
            }
            state.path_builder.clear();
        }
        "f*" => {
            // Fill with even-odd winding rule
            let path = state
                .path_builder
                .build(PaintMode::Fill, WindingOrder::EvenOdd);
            if !path.subpaths.is_empty() {
                out_ops.push(path_to_op(&path));
            }
            state.path_builder.clear();
        }
        "B" => {
            // Fill and stroke with non-zero winding
            let path = state
                .path_builder
                .build(PaintMode::FillStroke, WindingOrder::NonZero);
            if !path.subpaths.is_empty() {
                out_ops.push(path_to_op(&path));
            }
            state.path_builder.clear();
        }
        "B*" => {
            // Fill and stroke with even-odd winding
            let path = state
                .path_builder
                .build(PaintMode::FillStroke, WindingOrder::EvenOdd);
            if !path.subpaths.is_empty() {
                out_ops.push(path_to_op(&path));
            }
            state.path_builder.clear();
        }
        "b" => {
            // Close, fill, and stroke with non-zero winding
            state.path_builder.close_path();
            let path = state
                .path_builder
                .build(PaintMode::FillStroke, WindingOrder::NonZero);
            if !path.subpaths.is_empty() {
                out_ops.push(path_to_op(&path));
            }
            state.path_builder.clear();
        }
        "b*" => {
            // Close, fill, and stroke with even-odd winding
            state.path_builder.close_path();
            let path = state
                .path_builder
                .build(PaintMode::FillStroke, WindingOrder::EvenOdd);
            if !path.subpaths.is_empty() {
                out_ops.push(path_to_op(&path));
            }
            state.path_builder.clear();
        }
        "s" => {
            // Close and stroke
            state.path_builder.close_path();
            let path = state
                .path_builder
                .build(PaintMode::Stroke, WindingOrder::NonZero);
            if !path.subpaths.is_empty() {
                out_ops.push(path_to_op(&path));
            }
            state.path_builder.clear();
        }
        "n" => {
            // End path without filling or stroking (a preceding `W` / `W*`
            // already took it as the clip).
            state.path_builder.clear();
        }
        "W" => {
            // Set clip path using non-zero winding rule. `W` does not consume the path, so a
            // second `W` on a growing path would emit the whole path again and the output would
            // grow with the square of the input: a path is clipped once.
            if state.path_builder.clip_emitted {
                warnings.push(PdfWarnMsg::warning(
                    page,
                    op_id,
                    "'W' on a path that is already clipped; skipped".to_string(),
                ));
            } else {
                let path = state
                    .path_builder
                    .build(PaintMode::Clip, WindingOrder::NonZero);
                if !path.subpaths.is_empty() {
                    out_ops.push(path_to_op(&path));
                    state.path_builder.clip_emitted = true;
                }
            }
            // Note: We don't clear the path here since clipping doesn't consume the path
            state
                .rectangle_builder
                .set_winding_order(WindingOrder::NonZero);
        }
        "W*" => {
            // Set clip path using even-odd winding rule; a path is clipped once (see `W`).
            if state.path_builder.clip_emitted {
                warnings.push(PdfWarnMsg::warning(
                    page,
                    op_id,
                    "'W*' on a path that is already clipped; skipped".to_string(),
                ));
            } else {
                let path = state
                    .path_builder
                    .build(PaintMode::Clip, WindingOrder::EvenOdd);
                if !path.subpaths.is_empty()
                    || out_ops.last().is_some_and(|last_op| match last_op {
                        Op::DrawRectangle { .. } => true,
                        _ => false,
                    })
                {
                    out_ops.push(path_to_op(&path));
                    state.path_builder.clip_emitted = true;
                }
            }
            // Note: We don't clear the path here since clipping doesn't consume the path
            state
                .rectangle_builder
                .set_winding_order(WindingOrder::EvenOdd);
        }

        "w" => {
            if let Some(val) = op.operands.first() {
                let width = to_f32(val);
                out_ops.push(Op::SetOutlineThickness {
                    pt: crate::units::Pt(width),
                });
            }
        }

        "M" => {
            if let Some(val) = op.operands.first() {
                let limit = to_f32(val);
                out_ops.push(Op::SetMiterLimit { limit: Pt(limit) });
            }
        }

        "j" => {
            // "0 j" => miter join, "1 j" => round join, "2 j" => bevel
            if let Some(val) = op.operands.first() {
                let style_num = to_f32(val).round() as i64;
                let style = match style_num {
                    0 => crate::graphics::LineJoinStyle::Miter,
                    1 => crate::graphics::LineJoinStyle::Round,
                    2 => crate::graphics::LineJoinStyle::Bevel,
                    _ => crate::graphics::LineJoinStyle::Miter, // fallback
                };
                out_ops.push(Op::SetLineJoinStyle { join: style });
            }
        }

        "J" => {
            // "0 J" => butt cap, "1 J" => round cap, "2 J" => projecting square
            if let Some(val) = op.operands.first() {
                let style_num = to_f32(val).round() as i64;
                let style = match style_num {
                    0 => crate::graphics::LineCapStyle::Butt,
                    1 => crate::graphics::LineCapStyle::Round,
                    2 => crate::graphics::LineCapStyle::ProjectingSquare,
                    _ => crate::graphics::LineCapStyle::Butt, // fallback
                };
                out_ops.push(Op::SetLineCapStyle { cap: style });
            }
        }

        "d" => {
            // "d [2 2] 0" => dash 2 on, 2 off, offset=0
            // operand 0 is the dash array, operand 1 is the dash offset
            if let [arr_obj, offset] = op.operands.as_slice() {
                if let Ok(arr) = arr_obj.as_array() {
                    let pattern = arr.iter().map(to_f32).collect();
                    let offset = to_f32(offset);
                    let dash = LineDashPattern { offset, pattern };
                    out_ops.push(Op::SetLineDashPattern { dash });
                }
            }
        }

        "sc" | "scn" => {
            // The operand count picks the color space: 1 gray, 3 RGB, 4 CMYK.
            let floats = op.operands.iter().map(to_f32).collect::<Vec<_>>();
            match floats.as_slice() {
                [gray] => {
                    // grayscale
                    out_ops.push(Op::SetFillColor {
                        col: Color::Greyscale(crate::Greyscale {
                            percent: *gray,
                            icc_profile: None,
                        }),
                    });
                }
                [r, g, b] => {
                    // rgb
                    out_ops.push(Op::SetFillColor {
                        col: Color::Rgb(crate::Rgb {
                            r: *r,
                            g: *g,
                            b: *b,
                            icc_profile: None,
                        }),
                    });
                }
                [c, m, y, k] => {
                    // cmyk
                    out_ops.push(Op::SetFillColor {
                        col: Color::Cmyk(crate::Cmyk {
                            c: *c,
                            m: *m,
                            y: *y,
                            k: *k,
                            icc_profile: None,
                        }),
                    });
                }
                _ => {
                    // fallback
                    out_ops.push(Op::Unknown {
                        key: op.operator.clone(),
                        value: op.operands.iter().map(DictItem::from_lopdf).collect(),
                    });
                }
            }
        }

        // Stroke color; the operand count picks the color space as for `sc`.
        "SC" | "SCN" => {
            let floats = op.operands.iter().map(to_f32).collect::<Vec<_>>();
            match floats.as_slice() {
                [gray] => {
                    out_ops.push(Op::SetOutlineColor {
                        col: Color::Greyscale(crate::Greyscale {
                            percent: *gray,
                            icc_profile: None,
                        }),
                    });
                }
                [r, g, b] => {
                    out_ops.push(Op::SetOutlineColor {
                        col: Color::Rgb(crate::Rgb {
                            r: *r,
                            g: *g,
                            b: *b,
                            icc_profile: None,
                        }),
                    });
                }
                [c, m, y, k] => {
                    out_ops.push(Op::SetOutlineColor {
                        col: Color::Cmyk(crate::Cmyk {
                            c: *c,
                            m: *m,
                            y: *y,
                            k: *k,
                            icc_profile: None,
                        }),
                    });
                }
                _ => {
                    out_ops.push(Op::Unknown {
                        key: op.operator.clone(),
                        value: op.operands.iter().map(DictItem::from_lopdf).collect(),
                    });
                }
            }
        }

        // `'` and `"` show text like `Tj`. Without these arms they would become `Op::Unknown`,
        // which the default (`secure`) save drops.
        "'" => {
            if let Some(lopdf::Object::String(bytes, _)) = op.operands.first() {
                let text = decode_show_text_to_string(
                    bytes,
                    state.current_font_decode.as_ref(),
                    page,
                    op_id,
                    warnings,
                );
                out_ops.push(Op::MoveToNextLineShowText { text });
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    "Warning: `'` operand is not a string".to_string(),
                ));
            }
        }
        "\"" => {
            if let [word_spacing, char_spacing, text] = op.operands.as_slice() {
                let word_spacing = to_f32(word_spacing);
                let char_spacing = to_f32(char_spacing);
                if let lopdf::Object::String(bytes, _) = text {
                    let text = decode_show_text_to_string(
                        bytes,
                        state.current_font_decode.as_ref(),
                        page,
                        op_id,
                        warnings,
                    );
                    out_ops.push(Op::SetSpacingMoveAndShowText {
                        word_spacing,
                        char_spacing,
                        text,
                    });
                } else {
                    warnings.push(PdfWarnMsg::error(
                        page,
                        op_id,
                        "Warning: `\"` third operand is not a string".to_string(),
                    ));
                }
            } else {
                warnings.push(PdfWarnMsg::error(
                    page,
                    op_id,
                    format!(
                        "Warning: `\"` expects 3 operands, got {}",
                        op.operands.len()
                    ),
                ));
            }
        }

        // `sh` paints a named /Shading resource over the current clip.
        "sh" => {
            if let Some(name_str) = as_name(op.operands.first().unwrap_or(&lopdf::Object::Null)) {
                out_ops.push(Op::PaintShading {
                    id: crate::ShadingId(name_str),
                });
            }
        }

        "Do" => {
            if let Some(name_str) = as_name(op.operands.first().unwrap_or(&lopdf::Object::Null)) {
                let xobj_id = crate::XObjectId(name_str);
                // The document's own `cm` operators position the XObject; `no_auto_scale` stops the
                // serializer from scaling the unit square again.
                out_ops.push(Op::UseXobject {
                    id: xobj_id,
                    transform: crate::xobject::XObjectTransform {
                        no_auto_scale: true,
                        ..Default::default()
                    },
                });
            }
        }

        // Anything else is kept as `Op::Unknown`, with a warning.
        other => {
            warnings.push(PdfWarnMsg::error(
                page,
                op_id,
                format!("Info: unhandled operator '{}'", other),
            ));
            out_ops.push(Op::Unknown {
                key: op.operator.clone(),
                value: op.operands.iter().map(DictItem::from_lopdf).collect(),
            });
        }
    }

    Ok(out_ops)
}

/// Unix epoch, used when a date is missing or unreadable.
fn default_date() -> OffsetDateTime {
    OffsetDateTime::epoch()
}

/// Reads the document information dictionary (`/Info` of the trailer).
///
/// Missing or unreadable entries fall back to empty strings, `false` or the Unix epoch.
/// `/Keywords` is split at commas, and text strings are decoded from UTF-16BE (with byte order
/// mark) or UTF-8. The metadata date is set to the modification date, and the version to 1.
pub fn parse_document_info(dict: &LopdfDictionary) -> PdfDocumentInfo {
    let trapped = dict
        .get(b"Trapped")
        .ok()
        .and_then(|obj| match &obj {
            Object::String(s, _) => Some(s == b"True"),
            Object::Name(n) => Some(n == b"True"),
            _ => None,
        })
        .unwrap_or_default();

    let creation_date = dict
        .get(b"CreationDate")
        .ok()
        .and_then(|obj| match &obj {
            Object::String(s, _) => parse_pdf_date(String::from_utf8_lossy(s).as_ref()).ok(),
            _ => None,
        })
        .unwrap_or_else(default_date);

    let modification_date = dict
        .get(b"ModDate")
        .ok()
        .and_then(|obj| match &obj {
            Object::String(s, _) => parse_pdf_date(String::from_utf8_lossy(s).as_ref()).ok(),
            _ => None,
        })
        .unwrap_or_else(default_date);

    // If metadata date wasn't separately written, we default to the modification date.
    let metadata_date = modification_date;

    let conformance = dict
        .get(b"GTS_PDFXVersion")
        .ok()
        .and_then(|obj| match &obj {
            Object::String(s, _) => Some(PdfConformance::from_identifier_string(
                &String::from_utf8_lossy(s),
            )),
            _ => None,
        })
        .unwrap_or_default();

    let document_title = dict
        .get(b"Title")
        .ok()
        .and_then(|obj| match &obj {
            Object::String(s, _) => Some(decode_text_from_utf16be(s)),
            _ => None,
        })
        .unwrap_or_default();

    let author = dict
        .get(b"Author")
        .ok()
        .and_then(|obj| match &obj {
            Object::String(s, _) => Some(decode_text_from_utf16be(s)),
            _ => None,
        })
        .unwrap_or_default();

    let creator = dict
        .get(b"Creator")
        .ok()
        .and_then(|obj| match &obj {
            Object::String(s, _) => Some(decode_text_from_utf16be(s)),
            _ => None,
        })
        .unwrap_or_default();

    let producer = dict
        .get(b"Producer")
        .ok()
        .and_then(|obj| match &obj {
            Object::String(s, _) => Some(decode_text_from_utf16be(s)),
            _ => None,
        })
        .unwrap_or_default();

    let subject = dict
        .get(b"Subject")
        .ok()
        .and_then(|obj| match &obj {
            Object::String(s, _) => Some(decode_text_from_utf16be(s)),
            _ => None,
        })
        .unwrap_or_default();

    let identifier = dict
        .get(b"Identifier")
        .ok()
        .and_then(|obj| match &obj {
            Object::String(s, _) => Some(decode_text_from_utf16be(s)),
            _ => None,
        })
        .unwrap_or_default();

    let keywords = dict
        .get(b"Keywords")
        .ok()
        .and_then(|obj| match &obj {
            Object::String(s, _) => Some(decode_text_from_utf16be(s)),
            _ => None,
        })
        .map(|joined| {
            joined
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    PdfDocumentInfo {
        trapped,
        version: 1, // No version key was written, so we default to 1.
        creation_date,
        modification_date,
        metadata_date,
        conformance,
        document_title,
        author,
        creator,
        producer,
        keywords,
        subject,
        identifier,
    }
}

/// Parses a PDF rectangle from an Object (an array of four numbers).
fn parse_rect(obj: &Object) -> Result<crate::graphics::Rect, String> {
    if let Object::Array(arr) = obj {
        let [llx, lly, urx, ury] = arr.as_slice() else {
            return Err("Rectangle array does not have 4 elements".to_string());
        };
        let number = |o: &Object| match o {
            Object::Integer(i) => Ok(*i as f32),
            Object::Real(r) => Ok(*r),
            _ => Err("Rectangle element is not a number".to_string()),
        };
        // In PDF the rectangle is given as [llx, lly, urx, ury].
        let x = number(llx)?;
        let y = number(lly)?;
        let urx = number(urx)?;
        let ury = number(ury)?;
        let width = urx - x;
        let height = ury - y;
        Ok(crate::graphics::Rect::from_xywh(
            crate::units::Pt(x),
            crate::units::Pt(y),
            crate::units::Pt(width),
            crate::units::Pt(height),
        ))
    } else {
        Err("Rectangle is not an array".to_string())
    }
}

/// Helper to parse an operand into f32
fn to_f32(obj: &Object) -> f32 {
    match obj {
        Object::Integer(i) => *i as f32,
        Object::Real(r) => *r,
        _ => 0.0,
    }
}

/// Helper to parse an operand as a PDF name
fn as_name(obj: &Object) -> Option<String> {
    if let Object::Name(ref bytes) = obj {
        Some(String::from_utf8_lossy(bytes).to_string())
    } else {
        None
    }
}

/// The pages of the document in page-tree order, each with its page number (starting at 1).
///
/// This is the list [`LopdfDocument::get_pages`] gives, read one page at a time: `get_pages`
/// collects the pages into a map first, and that reserves memory for the number of pages the
/// `/Count` entries of the page tree announce, which a file can set to any number.
pub(crate) fn numbered_pages(doc: &LopdfDocument) -> Vec<(usize, ObjectId)> {
    let mut pages = Vec::new();
    for (index, id) in doc.page_iter().enumerate() {
        pages.push((index.saturating_add(1), id));
    }
    pages
}

mod links_and_bookmarks {
    use std::collections::{BTreeMap, BTreeSet};

    use lopdf::{Dictionary, Document, Object, ObjectId};

    use crate::{
        annotation::{
            Actions, BorderArray, ColorArray, DashPhase, Destination, HighlightingMode,
            LinkAnnotation, PageAnnotation,
        },
        graphics::Rect,
        PdfWarnMsg, Pt,
    };

    /// Reads the top level of the outline (`/First` and `/Next` links; children are not
    /// followed) as bookmarks with a title and a page number. Entries without a title or a
    /// `/Dest` array are skipped. Returns an empty vector if the catalog has no outline. A
    /// `/Next` chain that returns to an entry it has read ends there, with a warning.
    pub fn extract_bookmarks(doc: &Document, warnings: &mut Vec<PdfWarnMsg>) -> Vec<PageAnnotation> {
        try_extract_bookmarks(doc, warnings).unwrap_or_default()
    }

    fn try_extract_bookmarks(
        doc: &Document,
        warnings: &mut Vec<PdfWarnMsg>,
    ) -> Option<Vec<PageAnnotation>> {
        let catalog_id = match doc.trailer.get(b"Root").ok()? {
            Object::Reference(id) => *id,
            _ => return None,
        };
        let catalog = doc.get_object(catalog_id).ok()?.as_dict().ok()?;

        let outlines_ref = match catalog.get(b"Outlines").ok()? {
            Object::Reference(id) => *id,
            _ => return None,
        };
        let outlines = doc.get_object(outlines_ref).ok()?.as_dict().ok()?;

        let mut current_ref = match outlines.get(b"First").ok()? {
            Object::Reference(id) => Some(*id),
            _ => None,
        };

        // Build a mapping from page object id to page number.
        let page_map: BTreeMap<ObjectId, usize> = super::numbered_pages(doc)
            .into_iter()
            .map(|(num, id)| (id, num))
            .collect();

        let mut bookmarks = Vec::new();
        let mut visited = BTreeSet::new();
        while let Some(bm_ref) = current_ref {
            if !visited.insert(bm_ref) {
                warnings.push(PdfWarnMsg::warning(
                    0,
                    0,
                    format!(
                        "Bookmarks: the /Next chain returns to object {} {} after {} entries; \
                         the rest is ignored",
                        bm_ref.0,
                        bm_ref.1,
                        visited.len()
                    ),
                ));
                break;
            }
            let bm_dict = doc.get_object(bm_ref).ok()?.as_dict().ok()?;
            let title = match bm_dict.get(b"Title").ok()? {
                Object::String(s, _) => s.clone(),
                _ => {
                    current_ref = next_bookmark(bm_dict);
                    continue;
                }
            };
            let dest_array = match bm_dict.get(b"Dest").ok()? {
                Object::Array(arr) => arr,
                _ => {
                    current_ref = next_bookmark(bm_dict);
                    continue;
                }
            };
            // The first element should be a reference to a page.
            let page_num = if let Some(Object::Reference(page_id)) = dest_array.first() {
                *page_map.get(page_id).unwrap_or(&0)
            } else {
                0
            };

            bookmarks.push(PageAnnotation {
                name: decode_possible_utf16be(&title),
                page: page_num,
            });

            current_ref = next_bookmark(bm_dict);
        }
        Some(bookmarks)
    }

    /// Decodes a byte array that might be UTF-16BE encoded.
    /// Returns a String regardless of the input encoding.
    fn decode_possible_utf16be(bytes: &[u8]) -> String {
        // Early return if not UTF-16BE BOM
        let Some(body) = bytes.strip_prefix(&[0xFE, 0xFF]) else {
            return String::from_utf8_lossy(bytes).to_string();
        };

        // Decode as UTF-16BE after the BOM
        let (pairs, rest) = body.as_chunks::<2>();
        let mut chars: Vec<u16> = pairs.iter().map(|p| u16::from_be_bytes(*p)).collect();

        // Handle odd length (shouldn't happen in well-formed UTF-16)
        if let [high] = rest {
            chars.push(u16::from(*high) << 8);
        }

        String::from_utf16_lossy(&chars)
    }

    fn next_bookmark(dict: &Dictionary) -> Option<ObjectId> {
        dict.get(b"Next").ok().and_then(|obj| match obj {
            Object::Reference(id) => Some(*id),
            _ => None,
        })
    }

    /// Link annotations of all pages, with the zero-based index of their page, so that the parser
    /// can put them back on the page as `Op::LinkAnnotation`. Only `/Subtype /Link` annotations
    /// with a `/Rect` and a GoTo or URI action in `/A` are read.
    pub fn extract_link_annotations(doc: &Document) -> Vec<(usize, LinkAnnotation)> {
        let pages = super::numbered_pages(doc);
        let page_map: BTreeMap<ObjectId, usize> =
            pages.iter().map(|&(num, id)| (id, num)).collect();

        pages
            .iter()
            .flat_map(|&(page_num, page_id)| {
                let page_idx = page_num.saturating_sub(1);
                let page_dict = doc.get_object(page_id).ok()?.as_dict().ok()?;
                let annots = page_dict.get(b"Annots").ok()?.as_array().ok()?;
                Some(
                    annots
                        .iter()
                        .filter_map(|annot_obj| {
                            // Expect a reference to an annotation dictionary
                            // (tolerate inline dictionaries too).
                            let annot_dict = match annot_obj {
                                Object::Reference(id) => {
                                    doc.get_object(*id).ok()?.as_dict().ok()?
                                }
                                Object::Dictionary(d) => d,
                                _ => return None,
                            };
                            if annot_dict.get(b"Subtype").ok()?.as_name().ok()? != b"Link" {
                                return None;
                            }
                            let rect_arr = annot_dict.get(b"Rect").ok()?.as_array().ok()?;
                            if rect_arr.len() != 4 {
                                return None;
                            }
                            let coords: Vec<f32> = rect_arr
                                .iter()
                                .filter_map(|obj| match obj {
                                    Object::Real(r) => Some(*r),
                                    Object::Integer(i) => Some(*i as f32),
                                    _ => None,
                                })
                                .collect();
                            let &[llx, lly, urx, ury] = coords.as_slice() else {
                                return None;
                            };
                            // /Rect is [llx lly urx ury]; from_xywh wants
                            // x, y, width, height.
                            let rect =
                                Rect::from_xywh(Pt(llx), Pt(lly), Pt(urx - llx), Pt(ury - lly));

                            let action_dict = annot_dict.get(b"A").ok()?.as_dict().ok()?;
                            let actions = extract_action(action_dict, &page_map)?;

                            let border = annot_dict
                                .get(b"Border")
                                .ok()
                                .and_then(|obj| obj.as_array().ok())
                                .map(|s| extract_border_array(s.as_slice()))
                                .unwrap_or_else(BorderArray::default);

                            let color = annot_dict
                                .get(b"C")
                                .ok()
                                .and_then(|obj| obj.as_array().ok())
                                .map(|s| extract_color_array(s.as_slice()))
                                .unwrap_or_else(ColorArray::default);

                            let highlighting = annot_dict
                                .get(b"H")
                                .and_then(|obj| obj.as_name())
                                .map(|n| match n {
                                    n if n == b"N" => HighlightingMode::None,
                                    n if n == b"I" => HighlightingMode::Invert,
                                    n if n == b"O" => HighlightingMode::Outline,
                                    n if n == b"P" => HighlightingMode::Push,
                                    _ => HighlightingMode::Invert,
                                })
                                .unwrap_or(HighlightingMode::Invert);

                            Some((
                                page_idx,
                                LinkAnnotation {
                                    rect,
                                    actions,
                                    border,
                                    color,
                                    highlighting,
                                },
                            ))
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .collect()
    }

    /// Converts a PDF array into our BorderArray.
    fn extract_border_array(arr: &[Object]) -> BorderArray {
        let nums: Vec<f32> = arr
            .iter()
            .filter_map(|obj| match obj {
                Object::Integer(i) => Some(*i as f32),
                Object::Real(r) => Some(*r),
                _ => None,
            })
            .collect();
        match *nums.as_slice() {
            [a, b, c] => BorderArray::Solid([a, b, c]),
            [a, b, c, phase] => BorderArray::Dashed(
                [a, b, c],
                DashPhase {
                    dash_array: vec![],
                    phase,
                },
            ),
            _ => BorderArray::default(),
        }
    }

    /// Converts a PDF array into our ColorArray.
    fn extract_color_array(arr: &[Object]) -> ColorArray {
        let nums: Vec<f32> = arr
            .iter()
            .filter_map(|obj| match obj {
                Object::Integer(i) => Some(*i as f32),
                Object::Real(r) => Some(*r),
                _ => None,
            })
            .collect();

        match *nums.as_slice() {
            [] => ColorArray::Transparent,
            [gray] => ColorArray::Gray([gray]),
            [r, g, b] => ColorArray::Rgb([r, g, b]),
            [c, m, y, k] => ColorArray::Cmyk([c, m, y, k]),
            _ => ColorArray::default(),
        }
    }

    /// Converts a PDF action dictionary into our Actions enum.
    fn extract_action(dict: &Dictionary, page_map: &BTreeMap<ObjectId, usize>) -> Option<Actions> {
        let action_type = dict.get(b"S").ok()?.as_name().ok()?;
        match action_type {
            n if n == b"GoTo" => {
                let arr = dict.get(b"D").ok()?.as_array().ok()?;
                let page = if let Some(Object::Reference(page_id)) = arr.first() {
                    *page_map.get(page_id).unwrap_or(&0)
                } else {
                    0
                };
                let left = arr.get(2).and_then(|obj| match obj {
                    Object::Real(r) => Some(*r),
                    Object::Integer(i) => Some(*i as f32),
                    _ => None,
                });
                let top = arr.get(3).and_then(|obj| match obj {
                    Object::Real(r) => Some(*r),
                    Object::Integer(i) => Some(*i as f32),
                    _ => None,
                });
                let zoom = arr.get(4).and_then(|obj| match obj {
                    Object::Real(r) => Some(*r),
                    Object::Integer(i) => Some(*i as f32),
                    _ => None,
                });
                Some(Actions::Goto(Destination::Xyz {
                    page,
                    left,
                    top,
                    zoom,
                }))
            }
            n if n == b"URI" => {
                let uri = String::from_utf8_lossy(dict.get(b"URI").ok()?.as_str().ok()?);
                Some(Actions::Uri(uri.to_string()))
            }
            _ => None,
        }
    }
}

mod layers {

    use std::collections::{BTreeMap, BTreeSet};

    use lopdf::{Document, Object, ObjectId};

    use crate::ops::{Layer, LayerIntent, LayerSubtype};

    /// Resolve one level of indirection, returning the target object.
    fn resolve<'a>(doc: &'a Document, obj: &'a Object) -> &'a Object {
        match obj {
            Object::Reference(r) => doc.get_object(*r).unwrap_or(obj),
            other => other,
        }
    }

    /// Extract layers (optional content groups) from the PDF document,
    /// together with the resource name under which each is exposed to content
    /// streams.
    ///
    /// Content streams reference layers as `BDC /OC /SomeName`, where
    /// `/SomeName` is a key of the page's `/Resources` → `/Properties`
    /// dictionary that points at the OCG object. The layer map of the parsed
    /// document must be keyed by those names — the parser stores them in
    /// `Op::BeginOptionalContent { layer_id }` — otherwise every re-saved
    /// `BDC` references a property that does not exist and viewers drop the
    /// layer content ("Marked Content 'X' is unknown").
    ///
    /// Returns `(Some(resource_name), layer)` for OCGs reachable from some
    /// page's `/Properties`, `(None, layer)` for OCGs only listed in the
    /// catalog's `/OCProperties`.
    pub fn extract_layers(doc: &Document) -> Vec<(Option<String>, Layer)> {
        // OCG object id -> resource name (first one wins across pages).
        let mut ocg_names: BTreeMap<ObjectId, String> = BTreeMap::new();
        for (_page_num, page_id) in super::numbered_pages(doc) {
            let Ok(page_dict) = doc.get_dictionary(page_id) else {
                continue;
            };
            let Ok(res_obj) = page_dict.get(b"Resources") else {
                continue;
            };
            let Object::Dictionary(res_dict) = resolve(doc, res_obj) else {
                continue;
            };
            let Ok(props_obj) = res_dict.get(b"Properties") else {
                continue;
            };
            let Object::Dictionary(props) = resolve(doc, props_obj) else {
                continue;
            };
            for (name, val) in props.iter() {
                if let Object::Reference(ocg_id) = val {
                    ocg_names
                        .entry(*ocg_id)
                        .or_insert_with(|| String::from_utf8_lossy(name).to_string());
                }
            }
        }

        let catalog = match doc.trailer.get(b"Root").ok().map(|o| resolve(doc, o)) {
            Some(Object::Dictionary(dict)) => dict,
            _ => return vec![],
        };

        let ocprops = match catalog.get(b"OCProperties").ok().map(|o| resolve(doc, o)) {
            Some(Object::Dictionary(dict)) => dict,
            _ => return vec![],
        };

        let ocgs = match ocprops.get(b"OCGs").ok().map(|o| resolve(doc, o)) {
            Some(Object::Array(arr)) => arr,
            _ => return vec![],
        };

        // Also, if available, get the "ON" array from the "D" dictionary
        // to decide which layers are turned on by default.
        let default_on = match ocprops.get(b"D").ok().map(|o| resolve(doc, o)) {
            Some(Object::Dictionary(d_dict)) => {
                if let Ok(Object::Array(on_arr)) = d_dict.get(b"ON") {
                    on_arr
                        .iter()
                        .filter_map(|obj| {
                            if let Object::Reference(id) = obj {
                                Some(*id)
                            } else {
                                None
                            }
                        })
                        .collect::<BTreeSet<ObjectId>>()
                } else {
                    BTreeSet::new()
                }
            }
            _ => BTreeSet::new(),
        };

        let mut layers = Vec::new();
        for obj in ocgs {
            let layer_ref = match obj {
                Object::Reference(id) => id,
                _ => continue,
            };
            let layer_dict = match doc.get_object(*layer_ref) {
                Ok(Object::Dictionary(dict)) => dict,
                _ => continue,
            };

            let name = match layer_dict.get(b"Name").ok() {
                Some(Object::String(s, _)) => s.clone(),
                _ => continue,
            };

            // Extract the intent from the "Intent" key: either a name, an
            // inline array of names, or a reference to such an array.
            let intent = match layer_dict.get(b"Intent").ok().map(|o| resolve(doc, o)) {
                Some(Object::Array(arr)) => match arr.first() {
                    Some(Object::Name(n)) if n.as_slice() == b"View" => LayerIntent::View,
                    _ => LayerIntent::Design,
                },
                Some(Object::Name(n)) if n.as_slice() == b"View" => LayerIntent::View,
                _ => LayerIntent::Design,
            };

            let (creator, usage_subtype) =
                match layer_dict.get(b"Usage").ok().map(|o| resolve(doc, o)) {
                    Some(Object::Dictionary(usage_dict)) => {
                        if let Some(Object::Dictionary(creator_info)) =
                            usage_dict.get(b"CreatorInfo").ok().map(|o| resolve(doc, o))
                        {
                            let creator = match creator_info.get(b"Creator").ok() {
                                Some(Object::String(s, _)) => {
                                    String::from_utf8_lossy(s).to_string()
                                }
                                _ => String::new(),
                            };
                            let usage_subtype = match creator_info.get(b"Subtype").ok() {
                                Some(Object::Name(n)) => match n.as_slice() {
                                    b"Artwork" => LayerSubtype::Artwork,
                                    _ => LayerSubtype::Artwork,
                                },
                                _ => LayerSubtype::Artwork,
                            };
                            (creator, usage_subtype)
                        } else {
                            (String::new(), LayerSubtype::Artwork)
                        }
                    }
                    _ => (String::new(), LayerSubtype::Artwork),
                };

            // Decide final intent: if this layer's object ID is in the default "ON" set,
            // we treat its intent as View.
            let final_intent = if default_on.contains(layer_ref) {
                LayerIntent::View
            } else {
                intent
            };

            layers.push((
                ocg_names.get(layer_ref).cloned(),
                Layer {
                    name: String::from_utf8_lossy(&name).to_string(),
                    creator,
                    intent: final_intent,
                    usage: usage_subtype,
                },
            ));
        }

        layers
    }
}

mod extgstate {

    use std::collections::HashSet;

    use lopdf::{Dictionary as LoDictionary, Object};

    use crate::{
        graphics::{
            BlendMode, ChangedField, ExtendedGraphicsState, LineCapStyle, LineDashPattern,
            LineJoinStyle, OverprintMode, RenderingIntent,
        },
        BuiltinFont, BuiltinOrExternalFontId, FontId,
    };

    /// Given a PDF ExtGState dictionary, parse it into an ExtendedGraphicsState.
    pub fn parse_extgstate(dict: &LoDictionary) -> ExtendedGraphicsState {
        let mut gs = ExtendedGraphicsState::default();
        let mut changed = HashSet::new();

        if let Ok(obj) = dict.get(b"LW") {
            if let Some(num) = parse_f32(obj) {
                gs.line_width = num;
                changed.insert(ChangedField::LineWidth);
            }
        }

        if let Ok(obj) = dict.get(b"LC") {
            if let Some(num) = parse_i64(obj) {
                gs.line_cap = match num {
                    0 => LineCapStyle::Butt,
                    1 => LineCapStyle::Round,
                    2 => LineCapStyle::ProjectingSquare,
                    _ => LineCapStyle::Butt,
                };
                changed.insert(ChangedField::LineCap);
            }
        }

        if let Ok(obj) = dict.get(b"LJ") {
            if let Some(num) = parse_i64(obj) {
                gs.line_join = match num {
                    0 => LineJoinStyle::Miter,
                    1 => LineJoinStyle::Round,
                    2 => LineJoinStyle::Bevel,
                    _ => LineJoinStyle::Miter,
                };
                changed.insert(ChangedField::LineJoin);
            }
        }

        if let Ok(obj) = dict.get(b"ML") {
            if let Some(num) = parse_f32(obj) {
                gs.miter_limit = num;
                changed.insert(ChangedField::MiterLimit);
            }
        }

        if let Ok(obj) = dict.get(b"FL") {
            if let Some(num) = parse_f32(obj) {
                gs.flatness_tolerance = num;
                changed.insert(ChangedField::FlatnessTolerance);
            }
        }

        if let Ok(obj) = dict.get(b"RI") {
            if let Some(name) = parse_name(obj) {
                gs.rendering_intent = match name.as_str() {
                    "AbsoluteColorimetric" => RenderingIntent::AbsoluteColorimetric,
                    "RelativeColorimetric" => RenderingIntent::RelativeColorimetric,
                    "Saturation" => RenderingIntent::Saturation,
                    "Perceptual" => RenderingIntent::Perceptual,
                    _ => RenderingIntent::RelativeColorimetric,
                };
                changed.insert(ChangedField::RenderingIntent);
            }
        }

        if let Ok(obj) = dict.get(b"SA") {
            if let Some(b) = parse_bool(obj) {
                gs.stroke_adjustment = b;
                changed.insert(ChangedField::StrokeAdjustment);
            }
        }

        if let Ok(obj) = dict.get(b"OP") {
            if let Some(b) = parse_bool(obj) {
                gs.overprint_fill = b;
                changed.insert(ChangedField::OverprintFill);
            }
        }

        if let Ok(obj) = dict.get(b"op") {
            if let Some(b) = parse_bool(obj) {
                gs.overprint_stroke = b;
                changed.insert(ChangedField::OverprintStroke);
            }
        }

        if let Ok(obj) = dict.get(b"OPM") {
            if let Some(num) = parse_i64(obj) {
                gs.overprint_mode = match num {
                    0 => OverprintMode::EraseUnderlying,
                    1 => OverprintMode::KeepUnderlying,
                    _ => OverprintMode::EraseUnderlying,
                };
                changed.insert(ChangedField::OverprintMode);
            }
        }

        // `CA` is the stroking alpha, `ca` the nonstroking (fill) alpha.
        if let Ok(obj) = dict.get(b"CA") {
            if let Some(num) = parse_f32(obj) {
                gs.current_stroke_alpha = num;
                changed.insert(ChangedField::CurrentStrokeAlpha);
            }
        }

        if let Ok(obj) = dict.get(b"ca") {
            if let Some(num) = parse_f32(obj) {
                gs.current_fill_alpha = num;
                changed.insert(ChangedField::CurrentFillAlpha);
            }
        }

        if let Ok(obj) = dict.get(b"BM") {
            if let Some(name) = parse_name(obj) {
                gs.blend_mode = parse_blend_mode(&name);
                changed.insert(ChangedField::BlendMode);
            }
        }

        if let Ok(obj) = dict.get(b"AIS") {
            if let Some(b) = parse_bool(obj) {
                gs.alpha_is_shape = b;
                changed.insert(ChangedField::AlphaIsShape);
            }
        }

        if let Ok(obj) = dict.get(b"TK") {
            if let Some(b) = parse_bool(obj) {
                gs.text_knockout = b;
                changed.insert(ChangedField::TextKnockout);
            }
        }

        if let Ok(obj) = dict.get(b"D") {
            if let Ok(arr) = obj.as_array() {
                let pattern = arr.iter().filter_map(parse_f32).collect();
                gs.line_dash_pattern = Some(LineDashPattern {
                    offset: 0.0,
                    pattern,
                }); // default offset = 0
                changed.insert(ChangedField::LineDashPattern);
            }
        }

        if let Ok(obj) = dict.get(b"Font") {
            if let Some(name) = parse_name(obj) {
                gs.font = Some(match BuiltinFont::from_id(&name) {
                    Some(s) => BuiltinOrExternalFontId::Builtin(s),
                    None => BuiltinOrExternalFontId::External(FontId(name)),
                });
                changed.insert(ChangedField::Font);
            }
        }

        if let Ok(obj) = dict.get(b"SM") {
            if let Some(name) = parse_name(obj) {
                // Only `/SM /None` is understood.
                if name == "None" {
                    gs.soft_mask = None;
                } else {
                    // Other soft masks are dropped.
                    gs.soft_mask = None;
                }
                changed.insert(ChangedField::SoftMask);
            }
        }

        // Other keys (black generation, under-color removal, transfer functions, halftones) are
        // not read.

        gs.changed_fields = changed;
        gs
    }

    /// Helper: Convert a PDF Object into an f32, if possible.
    fn parse_f32(obj: &Object) -> Option<f32> {
        match obj {
            Object::Real(r) => Some(*r),
            Object::Integer(i) => Some(*i as f32),
            _ => None,
        }
    }

    /// Helper: Convert a PDF Object into an i64, if possible.
    fn parse_i64(obj: &Object) -> Option<i64> {
        match obj {
            Object::Integer(i) => Some(*i),
            Object::Real(r) => Some(*r as i64),
            _ => None,
        }
    }

    /// Helper: Convert a PDF Object into a bool, if possible.
    fn parse_bool(obj: &Object) -> Option<bool> {
        match obj {
            Object::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// Helper: Convert a PDF Name object into a String.
    fn parse_name(obj: &Object) -> Option<String> {
        match obj {
            Object::Name(bytes) => String::from_utf8(bytes.clone()).ok(),
            _ => None,
        }
    }

    /// Helper: Convert a name string into a BlendMode.
    fn parse_blend_mode(s: &str) -> BlendMode {
        use crate::graphics::{BlendMode, NonSeperableBlendMode, SeperableBlendMode};
        match s {
            "Normal" => BlendMode::Seperable(SeperableBlendMode::Normal),
            "Multiply" => BlendMode::Seperable(SeperableBlendMode::Multiply),
            "Screen" => BlendMode::Seperable(SeperableBlendMode::Screen),
            "Overlay" => BlendMode::Seperable(SeperableBlendMode::Overlay),
            "Darken" => BlendMode::Seperable(SeperableBlendMode::Darken),
            "Lighten" => BlendMode::Seperable(SeperableBlendMode::Lighten),
            "ColorDodge" => BlendMode::Seperable(SeperableBlendMode::ColorDodge),
            "ColorBurn" => BlendMode::Seperable(SeperableBlendMode::ColorBurn),
            "HardLight" => BlendMode::Seperable(SeperableBlendMode::HardLight),
            "SoftLight" => BlendMode::Seperable(SeperableBlendMode::SoftLight),
            "Difference" => BlendMode::Seperable(SeperableBlendMode::Difference),
            "Exclusion" => BlendMode::Seperable(SeperableBlendMode::Exclusion),
            "Hue" => BlendMode::NonSeperable(NonSeperableBlendMode::Hue),
            "Saturation" => BlendMode::NonSeperable(NonSeperableBlendMode::Saturation),
            "Color" => BlendMode::NonSeperable(NonSeperableBlendMode::Color),
            "Luminosity" => BlendMode::NonSeperable(NonSeperableBlendMode::Luminosity),
            _ => BlendMode::Seperable(SeperableBlendMode::Normal),
        }
    }
}

mod parsefont {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use lopdf::{Dictionary, Document, Object};

    use super::{decode_stream, ParsedExternalFont, ParsedOrBuiltinFont, MAX_STREAM_BYTES};
    use crate::{
        cmap::ToUnicodeCMap,
        deserialize::{get_dict_or_resolve_ref, PdfWarnMsg},
        font::ParsedFont,
        BuiltinFont, FontId,
    };

    /// Build a code -> unicode map from the font's own cmap table, for fonts
    /// that ship without a usable `/ToUnicode` CMap. This makes glyph runs
    /// extractable and lets the serializer regenerate a correct ToUnicode CMap
    /// instead of mapping every glyph to U+FFFD.
    ///
    /// ToUnicode is keyed by the content-stream code — the CID under Identity-H.
    /// When the descendant font maps CIDs to different glyph ids (`cid_to_gid`),
    /// the font-cmap gid must be re-keyed through the inverse of that map;
    /// otherwise code == gid.
    fn synthesize_to_unicode(
        font: &ParsedFont,
        cid_to_gid: Option<&BTreeMap<u16, u16>>,
    ) -> Option<ToUnicodeCMap> {
        let gid_to_cid: Option<BTreeMap<u16, u16>> =
            cid_to_gid.map(|m| m.iter().map(|(cid, gid)| (*gid, *cid)).collect());
        let mut mappings: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        // The full scalar-value range: supplementary-plane codepoints (CJK Ext B+,
        // emoji) are exactly what the CJK fonts hitting this path use.
        for cp in 0x0000..=0x10FFFFu32 {
            // skip UTF-16 surrogate range, not scalar values
            if (0xD800..=0xDFFF).contains(&cp) {
                continue;
            }
            if let Some(gid) = font.lookup_glyph_index(cp) {
                let code = gid_to_cid
                    .as_ref()
                    .and_then(|m| m.get(&gid).copied())
                    .unwrap_or(gid);
                mappings.entry(code as u32).or_insert_with(|| vec![cp]);
            }
        }
        if mappings.is_empty() {
            None
        } else {
            Some(ToUnicodeCMap { mappings })
        }
    }

    /// Resolve a Type0 font's `/Encoding` into a [`crate::cmap::CidCMap`], or
    /// `None` for Identity-H (the fast path: 2-byte codes, code == CID).
    ///
    /// Fully resolved: embedded CMap streams (codespace/cidrange/cidchar/
    /// notdefrange, `usecmap` of the identity CMaps) and `Identity-V` (decodes
    /// identically to Identity-H; the vertical writing mode itself is not
    /// preserved — re-saves are horizontal, and parsing says so). Predefined
    /// CJK CMaps require Adobe's CMap data files, which printpdf does not ship:
    /// those warn and fall back to 2-byte identity.
    fn extract_encoding_cmap(
        doc: &Document,
        font_dict: &Dictionary,
        font_id: &FontId,
        warnings: &mut Vec<PdfWarnMsg>,
        page_num: usize,
    ) -> Option<crate::cmap::CidCMap> {
        use crate::cmap::CidCMap;

        fn from_stream(
            s: &lopdf::Stream,
            font_id: &FontId,
            warnings: &mut Vec<PdfWarnMsg>,
            page_num: usize,
        ) -> Option<CidCMap> {
            let data = decode_stream(
                s,
                MAX_STREAM_BYTES,
                &format!("Type0 font {} /Encoding CMap", font_id.0),
                page_num,
                warnings,
            )?;
            match CidCMap::parse(&String::from_utf8_lossy(&data)) {
                Ok((cmap, unresolved_usecmap)) => {
                    if let Some(base) = unresolved_usecmap {
                        warnings.push(PdfWarnMsg::warning(
                            page_num,
                            0,
                            format!(
                                "Type0 font {}: embedded CMap uses /{base} as its \
                                 usecmap base, which printpdf cannot resolve — codes \
                                 the CMap itself does not map will decode as notdef",
                                font_id.0
                            ),
                        ));
                    }
                    if cmap.wmode == 1 {
                        warnings.push(PdfWarnMsg::warning(
                            page_num,
                            0,
                            format!(
                                "Type0 font {}: vertical writing (WMode 1) is decoded \
                                 but not preserved — re-saves are horizontal",
                                font_id.0
                            ),
                        ));
                    }
                    Some(cmap)
                }
                Err(e) => {
                    warnings.push(PdfWarnMsg::warning(
                        page_num,
                        0,
                        format!(
                            "Type0 font {}: embedded /Encoding CMap failed to parse \
                             ({e}); assuming 2-byte Identity codes",
                            font_id.0
                        ),
                    ));
                    None
                }
            }
        }

        match font_dict.get(b"Encoding") {
            Ok(Object::Name(n)) if n.as_slice() == b"Identity-H" => None,
            Ok(Object::Name(n)) if n.as_slice() == b"Identity-V" => {
                warnings.push(PdfWarnMsg::warning(
                    page_num,
                    0,
                    format!(
                        "Type0 font {} uses Identity-V: codes decode correctly, but \
                         vertical writing is not preserved — re-saves are horizontal",
                        font_id.0
                    ),
                ));
                Some(CidCMap::identity(1))
            }
            Ok(Object::Name(n)) => {
                warnings.push(PdfWarnMsg::warning(
                    page_num,
                    0,
                    format!(
                        "Type0 font {} uses the predefined CMap /{} — printpdf does \
                         not ship Adobe's CMap data; text may decode and re-save \
                         incorrectly",
                        font_id.0,
                        String::from_utf8_lossy(n),
                    ),
                ));
                None
            }
            Ok(Object::Stream(s)) => from_stream(s, font_id, warnings, page_num),
            Ok(Object::Reference(r)) => match doc.get_object(*r) {
                Ok(Object::Stream(s)) => from_stream(s, font_id, warnings, page_num),
                _ => None,
            },
            _ => None, // absent: Identity-H by convention
        }
    }

    /// The descendant font's CID -> glyph-id map, when it is not identity.
    ///
    /// Two sources, matching how viewers resolve Identity-H codes (ISO 32000-1,
    /// 9.7.4.2):
    /// - CIDFontType2: an explicit `/CIDToGIDMap` stream — 2 big-endian bytes per
    ///   CID. (The `/Identity` name, or no entry, means CID == GID.)
    /// - CIDFontType0 with a CID-keyed CFF program: the CFF charset, inverted.
    ///
    /// Only non-identity entries are stored; lookups fall back to CID == GID.
    ///
    /// The CFF-charset source is checked against the document's ToUnicode CMap and the font's own
    /// cmap: some producers (older printpdf versions among them) wrote glyph ids as Identity-H
    /// codes even for CID-keyed CFF fonts whose charset says otherwise. PDFium renders such
    /// files with code == GID, and reading them per spec would scramble glyph identities on
    /// re-save. When the identity reading explains the document's own code, unicode and glyph
    /// triple better than the charset reading, the charset map is discarded. An explicit
    /// `/CIDToGIDMap` stream is never second-guessed.
    fn extract_cid_to_gid_map(
        doc: &Document,
        font_dict: &Dictionary,
        font_id: &FontId,
        parsed_font: &ParsedFont,
        to_unicode: Option<&ToUnicodeCMap>,
        warnings: &mut Vec<PdfWarnMsg>,
        page_num: usize,
    ) -> Option<BTreeMap<u16, u16>> {
        let descendant = get_descendant_font_dict(doc, font_dict, font_id, warnings, page_num)?;

        let mut stream_to_map = |s: &lopdf::Stream| -> Option<BTreeMap<u16, u16>> {
            // Already-plain streams return the content as-is on error paths of some lopdf
            // versions; the raw bytes are the fallback.
            let data = decode_stream(
                s,
                MAX_STREAM_BYTES,
                &format!("font {} /CIDToGIDMap", font_id.0),
                page_num,
                warnings,
            )?;
            Some(
                data.as_chunks::<2>()
                    .0
                    .iter()
                    .enumerate()
                    // A CID is 16 bits wide.
                    .take(1 << 16)
                    .filter_map(|(cid, b)| {
                        let cid = u16::try_from(cid).ok()?;
                        let gid = u16::from_be_bytes(*b);
                        (gid != cid).then_some((cid, gid))
                    })
                    .collect(),
            )
        };

        match descendant.get(b"CIDToGIDMap") {
            Ok(Object::Stream(s)) => return stream_to_map(s),
            Ok(Object::Reference(r)) => {
                if let Ok(Object::Stream(s)) = doc.get_object(*r) {
                    return stream_to_map(s);
                }
            }
            _ => {} // absent or /Identity
        }

        // CID-keyed CFF: invert the charset's gid -> cid map. Identity entries are
        // dropped so `None`/empty means "no remapping needed".
        #[cfg(feature = "text_layout")]
        let bytes = parsed_font.source_bytes()?;
        #[cfg(feature = "text_layout")]
        let bytes = bytes.as_slice();
        #[cfg(not(feature = "text_layout"))]
        let bytes = parsed_font.original_bytes.as_slice();

        let map: BTreeMap<u16, u16> = crate::font::cff_charset_gid_to_cid_map(bytes, 0)?
            .into_iter()
            .filter_map(|(gid, cid)| (gid != cid).then_some((cid, gid)))
            .collect();
        if map.is_empty() {
            return None;
        }

        // Disambiguate spec-correct files from legacy code==GID files: for every
        // ToUnicode entry whose character the font's cmap knows, check which
        // reading of the code lands on the cmap's glyph.
        if let Some(tu) = to_unicode {
            let mut spec_hits = 0usize;
            let mut identity_hits = 0usize;
            for (code, unis) in tu.mappings.iter() {
                let Some(&cp) = unis.first() else { continue };
                let Ok(code16) = u16::try_from(*code) else {
                    continue;
                };
                let Some(cmap_gid) = parsed_font.lookup_glyph_index(cp) else {
                    continue;
                };
                let spec_gid = map.get(&code16).copied().unwrap_or(code16);
                if spec_gid == cmap_gid {
                    spec_hits += 1;
                }
                if code16 == cmap_gid {
                    identity_hits += 1;
                }
            }
            if identity_hits > spec_hits {
                warnings.push(PdfWarnMsg::info(
                    page_num,
                    0,
                    format!(
                        "font {}: CID-keyed CFF charset is not identity, but the \
                         document's codes match glyph ids, not CIDs ({} vs {} cmap \
                         agreements) — treating codes as glyph ids (legacy printpdf \
                         convention, #280)",
                        font_id.0, identity_hits, spec_hits
                    ),
                ));
                return None;
            }
        }

        Some(map)
    }

    /// Parses the `/Font` resources of one resource dictionary, keyed by resource name.
    ///
    /// A font whose name is a key of `known` is skipped: the first definition of a name wins.
    pub(crate) fn parse_fonts_enhanced(
        doc: &Document,
        resources: &Dictionary,
        warnings: &mut Vec<PdfWarnMsg>,
        page: Option<usize>,
        known: &BTreeMap<crate::BuiltinOrExternalFontId, ParsedOrBuiltinFont>,
    ) -> BTreeMap<FontId, ParsedOrBuiltinFont> {
        let mut fonts_map = BTreeMap::new();
        let page_num = page.unwrap_or_default();

        let fonts_dict = match get_fonts_dict_from_resources(doc, resources, warnings, page_num) {
            Some(dict) => dict,
            None => return fonts_map,
        };

        for (key, value) in fonts_dict.iter() {
            let font_id = FontId(String::from_utf8_lossy(key).to_string());
            if known.contains_key(&crate::BuiltinOrExternalFontId::External(font_id.clone())) {
                continue;
            }

            let font_dict = match get_dict_or_resolve_ref(
                &format!("parse_fonts font_dict page {page_num}"),
                doc,
                value,
                warnings,
                page,
            ) {
                Some(s) => s,
                None => continue,
            };

            let font_type = match get_font_subtype(font_dict, &font_id, warnings, page_num) {
                Some(t) => t,
                None => continue,
            };

            // Extract ToUnicode CMap directly from the font dictionary (common to all font types)
            let to_unicode_cmap = extract_to_unicode_cmap(doc, font_dict, warnings, page_num);

            if &font_type == b"Type0" {
                // Resolve the Type0 /Encoding: how content-stream bytes become codes
                // (codespace ranges — 1–4 bytes, variable width) and codes become
                // CIDs. `None` = Identity-H, the fast path. Embedded CMap streams
                // and Identity-V are resolved fully; predefined CJK CMaps
                // (90ms-RKSJ-H, GBK-EUC-H, …) need Adobe's CMap data files, which
                // printpdf does not ship — those warn and fall back to 2-byte
                // identity.
                let encoding_cmap =
                    extract_encoding_cmap(doc, font_dict, &font_id, warnings, page_num)
                        .map(Arc::new);
                // An external font is a Type0 (CID) font; this branch does not need `text_layout`.
                match process_type0_font(doc, font_dict, &font_id, warnings, page_num) {
                    Some(parsed_font) => {
                        let cid_to_gid = extract_cid_to_gid_map(
                            doc,
                            font_dict,
                            &font_id,
                            &parsed_font,
                            to_unicode_cmap.as_ref(),
                            warnings,
                            page_num,
                        )
                        .map(Arc::new);
                        // Without a ToUnicode CMap, glyph runs would extract as
                        // U+FFFD and the re-saved ToUnicode would be garbage;
                        // synthesize one from the font's own cmap table.
                        let to_unicode = to_unicode_cmap
                            .or_else(|| synthesize_to_unicode(&parsed_font, cid_to_gid.as_deref()))
                            .map(Arc::new);
                        fonts_map.insert(
                            font_id,
                            ParsedOrBuiltinFont::P(ParsedExternalFont {
                                font: parsed_font,
                                to_unicode,
                                is_cid: true,
                                cid_to_gid,
                                encoding_cmap,
                            }),
                        );
                    }
                    None => {
                        // Keep the code layout + ToUnicode for decoding even
                        // though the font itself cannot be re-embedded.
                        warnings.push(PdfWarnMsg::warning(
                            page_num,
                            0,
                            format!(
                                "Type0 font {} has no parsable font program; text will decode \
                                 via its ToUnicode CMap but the font cannot be re-embedded",
                                font_id.0
                            ),
                        ));
                        fonts_map.insert(
                            font_id,
                            ParsedOrBuiltinFont::CidStub(
                                to_unicode_cmap.map(Arc::new),
                                encoding_cmap,
                            ),
                        );
                    }
                }
            } else {
                match process_standard_font(doc, font_dict, &font_id, warnings, page_num) {
                    Some(ParsedOrBuiltinFont::B(builtin)) => {
                        fonts_map.insert(font_id, ParsedOrBuiltinFont::B(builtin));
                    }
                    Some(ParsedOrBuiltinFont::P(p)) => {
                        // Without a ToUnicode CMap the text of a simple font follows its
                        // `/Encoding` (base encoding, `/Differences`) and, for a CFF program,
                        // the program's own encoding.
                        let to_unicode = to_unicode_cmap
                            .or_else(|| simple_font_encoding_cmap(doc, font_dict))
                            .map(Arc::new);
                        fonts_map.insert(
                            font_id,
                            ParsedOrBuiltinFont::P(keep_glyph_codes(ParsedExternalFont {
                                to_unicode,
                                ..p
                            })),
                        );
                    }
                    Some(ParsedOrBuiltinFont::CidStub(..)) | None => {}
                }
            }
        }

        fonts_map
    }

    /// A CMap of one-byte codes, every code its own CID.
    #[cfg(feature = "text_layout")]
    const ONE_BYTE_CMAP: &str = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap \
        /CMapName /OneByte def /CMapType 1 def \
        1 begincodespacerange <00> <FF> endcodespacerange \
        1 begincidrange <00> <FF> 0 endcidrange endcmap end end";

    /// Reads a simple font whose program has no Unicode `cmap` as a font of one-byte codes, with
    /// the glyph of each code taken from the program's own map. A lookup by character finds the
    /// wrong glyph or none in such a font, so its text keeps the glyph ids. The characters come
    /// from the ToUnicode CMap or the `/Encoding`, and from WinAnsi for a font that has neither.
    /// A font without such a map is returned as it is.
    #[cfg(feature = "text_layout")]
    fn keep_glyph_codes(font: ParsedExternalFont) -> ParsedExternalFont {
        if font.is_cid {
            return font;
        }
        let Some(glyphs) = font.font.source_bytes().and_then(|bytes| {
            crate::font::code_to_glyph_without_unicode(bytes.as_slice(), font.font.original_index)
        }) else {
            return font;
        };
        let Ok((codes, _)) = crate::cmap::CidCMap::parse(ONE_BYTE_CMAP) else {
            return font;
        };
        let to_unicode = font
            .to_unicode
            .clone()
            .unwrap_or_else(|| Arc::new(win_ansi_cmap()));
        ParsedExternalFont {
            is_cid: true,
            to_unicode: Some(to_unicode),
            cid_to_gid: Some(Arc::new(glyphs)),
            encoding_cmap: Some(Arc::new(codes)),
            ..font
        }
    }

    /// The characters of the codes 0x20 to 0xFF in WinAnsi.
    #[cfg(feature = "text_layout")]
    fn win_ansi_cmap() -> ToUnicodeCMap {
        let mappings = (0x20u8..=0xFF)
            .filter_map(|code| {
                let c = crate::text::win_ansi_char(code);
                (c != '\u{FFFD}').then(|| (u32::from(code), vec![u32::from(c)]))
            })
            .collect();
        ToUnicodeCMap { mappings }
    }

    /// Without `text_layout` a simple font has no font program to read.
    #[cfg(not(feature = "text_layout"))]
    fn keep_glyph_codes(font: ParsedExternalFont) -> ParsedExternalFont {
        font
    }

    /// The characters of the codes of a simple font from its `/Encoding` and the built-in encoding
    /// of its CFF program, as a CMap. `None` when the font names no encoding at all.
    fn simple_font_encoding_cmap(doc: &Document, font_dict: &Dictionary) -> Option<ToUnicodeCMap> {
        let resolve = |o: &Object| match o {
            Object::Reference(r) => doc.get_object(*r).ok().cloned(),
            other => Some(other.clone()),
        };
        let encoding = crate::encoding::SimpleEncoding::from_font_dict(font_dict, resolve);
        #[cfg(feature = "text_layout")]
        let builtin = {
            let mut ignored = Vec::new();
            let program = get_font_descriptor(doc, font_dict, &FontId(String::new()), &mut ignored, 0)
                .and_then(|descriptor| descriptor.get(b"FontFile3").ok())
                .and_then(|file| get_font_data(doc, file, &FontId(String::new()), &mut ignored, 0));
            program.map(|bytes| crate::font::cff_builtin_encoding(&bytes)).unwrap_or_default()
        };
        #[cfg(not(feature = "text_layout"))]
        let builtin = BTreeMap::new();
        if encoding.is_empty() && builtin.is_empty() {
            return None;
        }
        Some(encoding.to_unicode(&builtin))
    }

    /// Parses the `/ToUnicode` CMap stream of a font dictionary. Warns and returns `None` when
    /// it cannot be parsed.
    pub(crate) fn extract_to_unicode_cmap(
        doc: &Document,
        font_dict: &Dictionary,
        warnings: &mut Vec<PdfWarnMsg>,
        page_num: usize,
    ) -> Option<ToUnicodeCMap> {
        if let Ok(to_unicode_ref) = font_dict.get(b"ToUnicode") {
            let to_unicode_stream = match to_unicode_ref {
                Object::Stream(s) => Some(s),
                Object::Reference(r) => match doc.get_object(*r) {
                    Ok(Object::Stream(s)) => Some(s),
                    _ => None,
                },
                _ => None,
            };

            if let Some(stream) = to_unicode_stream {
                let content = decode_stream(
                    stream,
                    MAX_STREAM_BYTES,
                    "ToUnicode CMap",
                    page_num,
                    warnings,
                )?;

                if let Ok(cmap_str) = String::from_utf8(content) {
                    match ToUnicodeCMap::parse(&cmap_str) {
                        Ok(cmap) => {
                            return Some(cmap);
                        }
                        Err(e) => {
                            warnings.push(PdfWarnMsg::warning(
                                page_num,
                                0,
                                format!("Failed to parse ToUnicode CMap: {}", e),
                            ));
                        }
                    }
                }
            }
        }

        None
    }

    /// Get fonts dictionary from PDF resources
    fn get_fonts_dict_from_resources<'a>(
        doc: &'a Document,
        resources: &'a Dictionary,
        warnings: &mut Vec<PdfWarnMsg>,
        page_num: usize,
    ) -> Option<&'a Dictionary> {
        let font_map = match resources.get(b"Font") {
            Ok(s) => s,
            Err(_) => return None,
        };

        get_dict_or_resolve_ref(
            &format!("parse_fonts Font page {page_num}"),
            doc,
            font_map,
            warnings,
            Some(page_num),
        )
    }

    /// Get font subtype from font dictionary
    fn get_font_subtype(
        font_dict: &Dictionary,
        font_id: &FontId,
        warnings: &mut Vec<PdfWarnMsg>,
        page_num: usize,
    ) -> Option<Vec<u8>> {
        match font_dict.get(b"Subtype") {
            Ok(Object::Name(n)) => Some(n.clone()),
            _ => {
                warnings.push(PdfWarnMsg::warning(
                    page_num,
                    0,
                    format!("Font {} missing Subtype", font_id.0),
                ));
                None
            }
        }
    }

    /// Process a Type0 (composite) font
    fn process_type0_font(
        doc: &Document,
        font_dict: &Dictionary,
        font_id: &FontId,
        warnings: &mut Vec<PdfWarnMsg>,
        page_num: usize,
    ) -> Option<ParsedFont> {
        let descendant_font_dict =
            get_descendant_font_dict(doc, font_dict, font_id, warnings, page_num)?;

        let font_descriptor =
            get_font_descriptor(doc, descendant_font_dict, font_id, warnings, page_num)?;

        process_font_data(doc, font_dict, font_descriptor, font_id, warnings, page_num)
    }

    /// Get the descendant font dictionary for a Type0 font
    fn get_descendant_font_dict<'a>(
        doc: &'a Document,
        font_dict: &'a Dictionary,
        font_id: &FontId,
        warnings: &mut Vec<PdfWarnMsg>,
        page_num: usize,
    ) -> Option<&'a Dictionary> {
        match font_dict.get(b"DescendantFonts") {
            Ok(Object::Array(arr)) if !arr.is_empty() => {
                let first = arr.first();
                match first.and_then(|f| f.as_dict().ok()).or_else(|| {
                    first
                        .and_then(|f| f.as_reference().ok())
                        .and_then(|id| doc.get_dictionary(id).ok())
                }) {
                    Some(d) => Some(d),
                    None => {
                        warnings.push(PdfWarnMsg::warning(
                            page_num,
                            0,
                            format!("Cannot resolve descendant font for {}", font_id.0),
                        ));
                        None
                    }
                }
            }
            Ok(Object::Reference(id)) => doc.get_dictionary(*id).ok(),
            _ => {
                warnings.push(PdfWarnMsg::warning(
                    page_num,
                    0,
                    format!("Type0 font {} missing DescendantFonts", font_id.0),
                ));
                None
            }
        }
    }

    /// Get the font descriptor dictionary
    fn get_font_descriptor<'a>(
        doc: &'a Document,
        font_dict: &'a Dictionary,
        font_id: &FontId,
        warnings: &mut Vec<PdfWarnMsg>,
        page_num: usize,
    ) -> Option<&'a Dictionary> {
        match font_dict.get(b"FontDescriptor") {
            Ok(Object::Dictionary(fd)) => Some(fd),
            Ok(Object::Reference(r)) => match doc.get_dictionary(*r) {
                Ok(fd) => Some(fd),
                Err(_) => {
                    warnings.push(PdfWarnMsg::warning(
                        page_num,
                        0,
                        format!("Cannot resolve FontDescriptor for {}", font_id.0),
                    ));
                    None
                }
            },
            _ => {
                warnings.push(PdfWarnMsg::warning(
                    page_num,
                    0,
                    format!("Font {} missing FontDescriptor", font_id.0),
                ));
                None
            }
        }
    }

    /// Process font data (extract, decompress, parse) and handle ToUnicode CMap
    fn process_font_data(
        doc: &Document,
        _font_dict: &Dictionary,
        font_descriptor: &Dictionary,
        font_id: &FontId,
        warnings: &mut Vec<PdfWarnMsg>,
        page_num: usize,
    ) -> Option<ParsedFont> {
        for font_file_key in &[b"FontFile", b"FontFile2" as &[u8], b"FontFile3"] {
            if let Ok(font_file_ref) = font_descriptor.get(font_file_key) {
                let font_data = get_font_data(doc, font_file_ref, font_id, warnings, page_num)?;

                warnings.push(PdfWarnMsg::info(
                    page_num,
                    0,
                    format!(
                        "Found font data for {} ({} bytes)",
                        font_id.0,
                        font_data.len()
                    ),
                ));

                let mut font_warnings = Vec::new();
                if let Some(parsed_font) = ParsedFont::from_bytes(&font_data, 0, &mut font_warnings)
                {
                    for fw in font_warnings {
                        warnings.push(PdfWarnMsg::warning(page_num, 0, format!("{fw:?}")));
                    }
                    return Some(parsed_font);
                } else {
                    warnings.push(PdfWarnMsg::error(
                        page_num,
                        0,
                        format!("Failed to parse font data for {}", font_id.0),
                    ));
                }
            }
        }
        None
    }

    /// Get and decompress font data from a font file reference
    fn get_font_data(
        doc: &Document,
        font_file_ref: &Object,
        font_id: &FontId,
        warnings: &mut Vec<PdfWarnMsg>,
        page_num: usize,
    ) -> Option<Vec<u8>> {
        let font_stream = match font_file_ref {
            Object::Stream(s) => s,
            Object::Reference(r) => match doc.get_object(*r) {
                Ok(Object::Stream(s)) => s,
                _ => return None,
            },
            _ => return None,
        };

        decode_stream(
            font_stream,
            MAX_STREAM_BYTES,
            &format!("Font {} font file", font_id.0),
            page_num,
            warnings,
        )
    }

    /// Process a Type1 font
    #[cfg(feature = "text_layout")]
    fn process_type1_font(
        doc: &Document,
        font_dict: &Dictionary,
        font_id: &FontId,
        warnings: &mut Vec<PdfWarnMsg>,
        page_num: usize,
    ) -> Option<ParsedFont> {
        let font_descriptor = get_font_descriptor(doc, font_dict, font_id, warnings, page_num)?;

        process_font_data(doc, font_dict, font_descriptor, font_id, warnings, page_num)
    }

    /// Process a standard font (Type1, TrueType, etc.)
    fn process_standard_font(
        _doc: &Document,
        font_dict: &Dictionary,
        font_id: &FontId,
        warnings: &mut Vec<PdfWarnMsg>,
        page_num: usize,
    ) -> Option<ParsedOrBuiltinFont> {
        let basefont = match font_dict.get(b"BaseFont") {
            Ok(Object::Name(basefont_bytes)) => String::from_utf8_lossy(basefont_bytes).to_string(),
            _ => return None,
        };

        match BuiltinFont::from_id(&basefont) {
            Some(builtin) => Some(ParsedOrBuiltinFont::B(builtin)),
            None => {
                #[cfg(feature = "text_layout")]
                {
                    match process_type1_font(_doc, font_dict, font_id, warnings, page_num) {
                        Some(parsed_font) => {
                            Some(ParsedOrBuiltinFont::P(ParsedExternalFont {
                                font: parsed_font,
                                to_unicode: None,
                                is_cid: false,
                                // Simple font: one-byte codes, no CID indirection.
                                cid_to_gid: None,
                                encoding_cmap: None,
                            }))
                        }
                        None => {
                            let substitute = builtin_substitute(&basefont);
                            warnings.push(PdfWarnMsg::warning(
                                page_num,
                                0,
                                format!(
                                    "Font {} ({}) has no embedded font program; substituting \
                                     builtin {:?}",
                                    font_id.0, basefont, substitute
                                ),
                            ));
                            Some(ParsedOrBuiltinFont::B(substitute))
                        }
                    }
                }
                #[cfg(not(feature = "text_layout"))]
                {
                    let substitute = builtin_substitute(&basefont);
                    warnings.push(PdfWarnMsg::warning(
                        page_num,
                        0,
                        format!(
                            "Font {} ({}) has no embedded font program; substituting builtin \
                             {:?}",
                            font_id.0, basefont, substitute
                        ),
                    ));
                    Some(ParsedOrBuiltinFont::B(substitute))
                }
            }
        }
    }

    /// Picks the closest standard-14 font for a simple font that is not embedded (`Arial`,
    /// `TimesNewRomanPSMT`, ...), so that its text stays readable and can be written again.
    fn builtin_substitute(basefont: &str) -> BuiltinFont {
        use crate::BuiltinFont::*;
        // Strip the "ABCDEF+" subset prefix if present.
        let name = basefont
            .rsplit('+')
            .next()
            .unwrap_or(basefont)
            .to_ascii_lowercase();
        let bold = name.contains("bold");
        let italic = name.contains("italic") || name.contains("oblique");
        if name.contains("times")
            || name.contains("serif")
            || name.contains("georgia")
            || name.contains("garamond")
            || name.contains("book")
        {
            match (bold, italic) {
                (true, true) => TimesBoldItalic,
                (true, false) => TimesBold,
                (false, true) => TimesItalic,
                (false, false) => TimesRoman,
            }
        } else if name.contains("courier") || name.contains("mono") || name.contains("consol") {
            match (bold, italic) {
                (true, true) => CourierBoldOblique,
                (true, false) => CourierBold,
                (false, true) => CourierOblique,
                (false, false) => Courier,
            }
        } else if name.contains("symbol") {
            Symbol
        } else if name.contains("zapf") || name.contains("dingbat") {
            ZapfDingbats
        } else {
            match (bold, italic) {
                (true, true) => HelveticaBoldOblique,
                (true, false) => HelveticaBold,
                (false, true) => HelveticaOblique,
                (false, false) => Helvetica,
            }
        }
    }
}

/// Decodes a PDF text string: UTF-16BE when it starts with the byte order mark `FE FF`,
/// otherwise UTF-8 (lossy).
fn decode_text_from_utf16be(bytes: &[u8]) -> String {
    let Some(body) = bytes.strip_prefix(&[0xFE, 0xFF]) else {
        // not UTF-16BE encoded, use UTF-8
        return String::from_utf8_lossy(bytes).into_owned();
    };

    let (chunks, remainder) = body.as_chunks::<2>();
    let string = char::decode_utf16(chunks.iter().copied().map(u16::from_be_bytes))
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    if remainder.is_empty() {
        string
    } else {
        string + "\u{FFFD}"
    }
}
