//! XObjects: images and reusable drawings that a page draws with the `Do` operator.
//!
//! A page's content stream only holds drawing operators. Anything bigger, such as a bitmap, or
//! vector graphics that appear several times, is stored once as an *XObject* ("external object":
//! a stream plus a dictionary) and listed in the page's `/Resources`. The content stream then
//! names it and paints it with `Do` (ISO 32000-1, 8.8).
//!
//! An [`XObject`] is one of three things:
//!
//! - [`XObject::Image`]: a bitmap, held as a [`RawImage`] (pixels, width, height, pixel layout).
//!   See the [`image_types`](crate::image_types) module.
//! - [`XObject::Form`]: a [`FormXObject`], content of its own (a list of [`Op`]s with the fonts,
//!   images and so on it names) that can be painted any number of times. It has nothing to do
//!   with interactive PDF forms. Reading a PDF produces these for the forms it can model.
//! - [`XObject::External`]: an [`ExternalXObject`], any other stream, kept as a plain dictionary
//!   plus bytes. SVG graphics (`Svg::parse`, `svg` feature) and every XObject read from an
//!   existing PDF that is not a decodable bitmap or a form that can be modelled end up here.
//!
//! # Using one
//!
//! Register the object with [`PdfDocument::add_image`](crate::PdfDocument::add_image) or
//! [`PdfDocument::add_xobject`](crate::PdfDocument::add_xobject). Both return an [`XObjectId`](crate::XObjectId).
//! Paint it with [`Op::UseXobject`] and an [`XObjectTransform`] that says
//! where it goes. A complete example is on [`RawImage`].
//!
//! # Size and units
//!
//! A PDF paints an XObject into a 1 by 1 unit square at the origin of the current coordinate
//! system. When an XObject has a size in pixels ([`XObject::get_width_height`]),
//! [`XObjectTransform`] first scales the unit square to that size converted to points at its
//! `dpi` (300 when unset, so 1 pixel is 0.24 pt). Positions are in [`Pt`] from the bottom left
//! corner of the page.

use std::collections::BTreeMap;

use lopdf::StringFormat;
use serde_derive::{Deserialize, Serialize};

use crate::{
    date::OffsetDateTime,
    deserialize::PageState,
    graphics::Rect,
    image_types::RawImage,
    matrix::CurTransMat,
    units::{Pt, Px},
    Op, PdfResources,
};

/// An object that lives outside the content stream and is painted with `Do`
/// (ISO 32000-1, 8.8). The document keeps them in [`PdfResources`].
///
/// Serialized as `{"type": "image" | "form" | "external", "data": ...}`.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type", content = "data")]
pub enum XObject {
    /// A bitmap (`/Subtype /Image`, ISO 32000-1, 8.9). Encoded when the document is saved.
    Image(RawImage),
    /// Reusable vector content (`/Subtype /Form`, ISO 32000-1, 8.10). Not an interactive form.
    Form(FormXObject),
    /// A stream kept as it is, for XObjects this crate does not model, such as SVG graphics.
    External(ExternalXObject),
}

impl XObject {
    /// Width and height in pixels, if the XObject has them: always for an image, when set for
    /// a form or an external XObject. [`XObjectTransform`] uses it to scale the unit square.
    pub fn get_width_height(&self) -> Option<(Px, Px)> {
        match self {
            XObject::Image(raw_image) => Some((Px(raw_image.width), Px(raw_image.height))),
            XObject::Form(form_xobject) => form_xobject.size,
            XObject::External(external_xobject) => {
                Some((external_xobject.width?, external_xobject.height?))
            }
        }
    }
}

// The most that `ExternalStream::decompressed_content` inflates a stream to: a few bytes of zlib
// can expand to gigabytes.
const MAX_DECODED_STREAM_BYTES: usize = 256 * 1024 * 1024;

/// An XObject kept as a raw stream: a dictionary plus bytes, painted with `Do`.
///
/// Made by `Svg::parse` (`svg` feature) and when reading a PDF. The `stream` dictionary should
/// carry `/Type /XObject` and `/Subtype`, and for a form also `/BBox`.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalXObject {
    /// The dictionary and content of the stream.
    pub stream: ExternalStream,
    /// Width in pixels. With `height`, the size [`XObjectTransform`] scales the unit square to.
    #[serde(default)]
    pub width: Option<Px>,
    /// Height in pixels.
    #[serde(default)]
    pub height: Option<Px>,
    /// Resolution the pixel size refers to, in dots per inch. Informational: placing uses the
    /// `dpi` of the [`XObjectTransform`], not this one.
    #[serde(default)]
    pub dpi: Option<f32>,
}

/// A PDF stream: a dictionary and bytes.
#[derive(Debug, PartialEq, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalStream {
    /// The stream dictionary, keyed by name without the leading `/`.
    pub dict: BTreeMap<String, DictItem>,
    /// The bytes as they are stored. When the dictionary has a `/Filter`, they are encoded.
    pub content: Vec<u8>,
    /// Whether the writer may compress `content` (set false when it is already encoded).
    pub compress: bool,
}

impl ExternalStream {
    pub(crate) fn into_lopdf(&self) -> lopdf::Stream {
        lopdf::Stream::new(build_dict(&self.dict), self.content.clone())
            .with_compression(self.compress)
    }

    /// Like `into_lopdf`, but a [`DictItem::Stream`] nested at any depth in the dictionary is
    /// added to `doc` as its own object and replaced by a reference (ISO 32000-1, 7.3.8.1).
    pub(crate) fn into_lopdf_with_indirect_streams(
        &self,
        doc: &mut lopdf::Document,
    ) -> lopdf::Stream {
        let dict = lopdf::Dictionary::from_iter(
            self.dict
                .iter()
                .map(|(k, v)| (k.clone(), v.to_lopdf_with_indirect_streams(doc))),
        );
        lopdf::Stream::new(dict, self.content.clone()).with_compression(self.compress)
    }

    /// The content with its `/Filter`s decoded, or the stored bytes when they cannot be decoded
    /// or would decode to more than 256 MiB.
    pub fn decompressed_content(&self) -> Vec<u8> {
        self.into_lopdf()
            .decompressed_content_with_limit(MAX_DECODED_STREAM_BYTES)
            .unwrap_or(self.content.clone())
    }

    /// Parses the text of a content stream into [`Op`]s.
    ///
    /// # Errors
    ///
    /// Returns a message when the text is not a valid content stream.
    pub fn decode_ops(s: &str) -> Result<Vec<Op>, String> {
        Self::get_ops_internal(s.as_bytes())
    }

    /// Decodes the content (see [`ExternalStream::decompressed_content`]) and parses it into
    /// [`Op`]s.
    ///
    /// # Errors
    ///
    /// Returns a message when the content is not a valid content stream.
    pub fn get_ops(&self) -> Result<Vec<Op>, String> {
        Self::get_ops_internal(&self.decompressed_content())
    }

    fn get_ops_internal(s: &[u8]) -> Result<Vec<Op>, String> {
        let content = lopdf::content::Content::decode(s)
            .map_err(|e| format!("Failed to decode content stream: {}", e))?;

        let mut page_state = PageState::default();
        let mut printpdf_ops = Vec::new();

        for (op_id, op) in content.operations.iter().enumerate() {
            let parsed_op = crate::deserialize::parse_op(
                0,
                op_id,
                op,
                &mut page_state,
                &BTreeMap::new(),
                &mut Vec::new(),
            )?;
            printpdf_ops.extend(parsed_op);
        }

        Ok(printpdf_ops)
    }
}

/// A PDF object inside a stream dictionary: a simplified, serializable copy of
/// [`lopdf::Object`].
///
/// Serialized as `{"type": "<variant in kebab-case>", "data": ...}`.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type", content = "data")]
pub enum DictItem {
    /// An array, `[ ... ]`.
    Array(Vec<DictItem>),
    /// A string. `literal` selects the `( ... )` form over the hexadecimal `< ... >` form.
    String {
        /// The bytes of the string.
        data: Vec<u8>,
        /// True for the literal form, false for hexadecimal.
        literal: bool,
    },
    /// Bytes, written as a hexadecimal string.
    Bytes(Vec<u8>),
    /// `true` or `false`.
    Bool(bool),
    /// A real number (written as a PDF real, like [`DictItem::Real`]).
    Float(f32),
    /// An integer.
    Int(i64),
    /// A real number.
    Real(f32),
    /// A name, `/Name`, as bytes without the slash.
    Name(Vec<u8>),
    /// An indirect reference `obj gen R`.
    Ref {
        /// Object number.
        obj: u32,
        /// Generation number.
        gen: u16,
    },
    /// A dictionary, `<< ... >>`.
    Dict {
        /// The entries, keyed by name without the slash.
        map: BTreeMap<String, DictItem>,
    },
    /// A stream with its own dictionary.
    Stream {
        /// The stream.
        stream: ExternalStream,
    },
    /// `null`.
    Null,
}

impl DictItem {
    /// Converts to a [`lopdf::Object`]. A nested [`DictItem::Stream`] stays an inline stream.
    pub fn to_lopdf(&self) -> lopdf::Object {
        use lopdf::{Object, StringFormat};
        match self {
            DictItem::Array(items) => {
                let objs = items.iter().map(|item| item.to_lopdf()).collect();
                Object::Array(objs)
            }
            DictItem::String { data, literal } => {
                let format = if *literal {
                    StringFormat::Literal
                } else {
                    StringFormat::Hexadecimal
                };
                Object::String(data.clone(), format)
            }
            DictItem::Bytes(data) => {
                Object::String(data.clone(), StringFormat::Hexadecimal)
            }
            DictItem::Bool(b) => Object::Boolean(*b),
            DictItem::Float(f) => Object::Real(*f),
            DictItem::Int(i) => Object::Integer(*i),
            DictItem::Real(f) => Object::Real(*f),
            DictItem::Name(name) => Object::Name(name.clone()),
            DictItem::Ref { obj, gen } => Object::Reference((*obj, *gen)),
            DictItem::Dict { map } => {
                let dict = map
                    .iter()
                    .map(|(k, v)| (k.as_bytes().to_vec(), v.to_lopdf()))
                    .collect();
                Object::Dictionary(dict)
            }
            DictItem::Stream { stream } => {
                let stream_obj = stream.into_lopdf();
                Object::Stream(stream_obj)
            }
            DictItem::Null => Object::Null,
        }
    }

    /// Like [`DictItem::to_lopdf`], but nested streams become indirect objects of `doc`
    /// and are replaced by references (ISO 32000-1, 7.3.8.1).
    pub(crate) fn to_lopdf_with_indirect_streams(
        &self,
        doc: &mut lopdf::Document,
    ) -> lopdf::Object {
        use lopdf::Object;
        match self {
            DictItem::Array(items) => Object::Array(
                items
                    .iter()
                    .map(|item| item.to_lopdf_with_indirect_streams(doc))
                    .collect(),
            ),
            DictItem::Dict { map } => Object::Dictionary(lopdf::Dictionary::from_iter(
                map.iter().map(|(k, v)| {
                    (k.as_bytes().to_vec(), v.to_lopdf_with_indirect_streams(doc))
                }),
            )),
            DictItem::Stream { stream } => {
                let stream_obj = stream.into_lopdf_with_indirect_streams(doc);
                Object::Reference(doc.add_object(stream_obj))
            }
            other => other.to_lopdf(),
        }
    }

    /// Converts a [`lopdf::Object`]. References stay references, they are not resolved.
    pub fn from_lopdf(o: &lopdf::Object) -> Self {
        use lopdf::Object;
        match o {
            Object::Null => DictItem::Null,
            Object::Boolean(t) => DictItem::Bool(*t),
            Object::Integer(i) => DictItem::Int(*i),
            Object::Real(r) => DictItem::Real(*r),
            Object::Name(items) => DictItem::Name(items.clone()),
            Object::String(items, string_format) => DictItem::String {
                data: items.clone(),
                literal: *string_format == StringFormat::Literal,
            },
            Object::Array(objects) => {
                DictItem::Array(objects.iter().map(DictItem::from_lopdf).collect())
            }
            Object::Dictionary(dictionary) => DictItem::Dict {
                map: dictionary
                    .iter()
                    .map(|s| {
                        (
                            String::from_utf8_lossy(s.0).to_string(),
                            DictItem::from_lopdf(s.1),
                        )
                    })
                    .collect(),
            },
            Object::Stream(stream) => DictItem::Stream {
                stream: ExternalStream {
                    compress: stream.allows_compression,
                    content: stream.content.clone(),
                    dict: stream
                        .dict
                        .iter()
                        .map(|s| {
                            (
                                String::from_utf8_lossy(s.0).to_string(),
                                DictItem::from_lopdf(s.1),
                            )
                        })
                        .collect(),
                },
            },
            Object::Reference((a, b)) => DictItem::Ref { obj: *a, gen: *b },
        }
    }
}

/// Filters (ISO 32000-1, 7.4) that can encode image bytes. Not used by the crate itself; see
/// [`ImageCompression`](crate::ImageCompression) to choose how images are written.
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ImageFilter {
    /// `ASCII85Decode`: binary as printable ASCII, no compression.
    Ascii85,
    /// `LZWDecode`: lossless Lempel-Ziv-Welch compression.
    Lzw,
    /// `DCTDecode`: lossy JPEG (baseline).
    DCT,
    /// `JPXDecode`: lossy or lossless JPEG 2000.
    JPX,
}

/// Reusable content with its own coordinate system, resources and list of [`Op`]s
/// (ISO 32000-1, 8.10). Paint it with [`Op::UseXobject`].
///
/// A form is a small page: its [`ops`](FormXObject::ops) are drawn inside the
/// [`bbox`](FormXObject::bbox), in the coordinate system that [`matrix`](FormXObject::matrix)
/// maps onto the page. The fonts, images, graphics states and shadings that the ops name are the
/// form's own [`resources`](FormXObject::resources), so a name in a form never clashes with a
/// name on a page or in another form.
///
/// Reading a PDF turns every form it can model into a `FormXObject`, so
/// [`PdfDocument::extract_text`](crate::PdfDocument::extract_text),
/// [`PdfPage::extract_text_boxes`](crate::PdfPage::extract_text_boxes) and
/// [`PdfPage::to_svg`](crate::PdfPage::to_svg) see the text and drawings inside it. Pages that
/// macOS (Quartz), Word or a print driver write put the whole page into one form. A form that
/// cannot be modelled without loss stays an [`ExternalXObject`], byte for byte.
///
/// To make one in code, use [`FormXObject::new`] and register it with
/// [`PdfDocument::add_form`](crate::PdfDocument::add_form).
///
/// This is not an interactive PDF form (AcroForm).
///
/// # Examples
///
/// A watermark that is painted twice:
///
/// ```
/// # use printpdf::*;
/// let mut doc = PdfDocument::new("Watermark");
///
/// // The content, in the form's own coordinate system (200 x 40 pt).
/// let ops = vec![
///     Op::StartTextSection,
///     Op::SetFont { font: PdfFontHandle::Builtin(BuiltinFont::HelveticaBold), size: Pt(36.0) },
///     Op::SetTextCursor { pos: Point { x: Pt(0.0), y: Pt(8.0) } },
///     Op::ShowText { items: vec![TextItem::Text("DRAFT".to_string())] },
///     Op::EndTextSection,
/// ];
/// let id = doc.add_form(&FormXObject::new(Rect::from_wh(Pt(200.0), Pt(40.0)), ops));
///
/// let at = |x: f32, y: f32| Op::UseXobject {
///     id: id.clone(),
///     transform: XObjectTransform {
///         translate_x: Some(Pt(x)),
///         translate_y: Some(Pt(y)),
///         ..Default::default()
///     },
/// };
/// doc.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), vec![at(100.0, 400.0), at(100.0, 100.0)]));
///
/// // The text of the form is part of the page.
/// assert_eq!(doc.extract_text()[0], vec!["DRAFT", "DRAFT"]);
/// let bytes = doc.save(&PdfSaveOptions::default(), &mut Vec::new());
/// assert!(bytes.starts_with(b"%PDF"));
/// ```
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormXObject {
    /// `/FormType`. [`FormType::Type1`] is the only form type there is.
    pub form_type: FormType,
    /// Width and height in pixels. Not written: [`XObjectTransform`] uses it to scale the
    /// unit square to the size of the form (see [`XObject::get_width_height`]). `None` for a
    /// form read from a PDF, which is painted in its own coordinate system.
    pub size: Option<(Px, Px)>,
    /// `/BBox`: the area of the form, in its own coordinate system. What the ops paint outside
    /// of it is clipped (ISO 32000-1, 8.10.2).
    #[serde(default = "empty_bbox")]
    pub bbox: Rect,
    /// `/Matrix`: maps the coordinate system of the form into the one it is painted in, applied
    /// before the placement of the [`Op::UseXobject`] (ISO 32000-1, 8.10.2).
    pub matrix: Option<CurTransMat>,
    /// The content: what the form paints, as the ops of a page would. Names in the ops
    /// (`Op::SetFont`, `Op::UseXobject`, `Op::LoadGraphicsState`, `Op::PaintShading`) are looked
    /// up in [`resources`](FormXObject::resources).
    #[serde(default)]
    pub ops: Vec<Op>,
    /// `/Resources`: the fonts, images, nested forms, graphics states and shadings the ops
    /// name. They are local to the form. With `None` the form has no `/Resources` and uses the
    /// resources of the page that paints it, as forms did before PDF 1.2: its names are looked
    /// up in the document's [`PdfResources`], or in the scope of the form that paints it.
    ///
    /// The `layers` of a form's resources are not used: layers belong to the document.
    #[serde(default)]
    pub resources: Option<PdfResources>,
    /// `/Group`: treats the form as a transparency group (ISO 32000-1, 11.6.6). A group with
    /// more entries than its type (`/I`, `/K`, `/CS`) is in `extra` instead.
    pub group: Option<GroupXObject>,
    /// `/Ref`: marks the form as a stand-in for a page of another PDF file
    /// (ISO 32000-1, 8.10.4).
    pub ref_dict: Option<BTreeMap<String, DictItem>>,
    /// `/Metadata` (ISO 32000-1, 14.3.2). Written as an indirect dictionary object.
    pub metadata: Option<BTreeMap<String, DictItem>>,
    /// `/PieceInfo`: data of the application that made the form (ISO 32000-1, 14.5).
    pub piece_info: Option<BTreeMap<String, DictItem>>,
    /// `/LastModified`: when the form was last changed. Needed when `piece_info` is set
    /// (ISO 32000-1, 14.5).
    pub last_modified: Option<OffsetDateTime>,
    /// `/StructParent`: key of the form in the structure parent tree, when the whole form is
    /// one structure content item (ISO 32000-1, 14.7.4.4). Not written when `struct_parents`
    /// is set.
    pub struct_parent: Option<i64>,
    /// `/StructParents`: key of the form in the structure parent tree, when it contains
    /// marked-content sequences that are content items (ISO 32000-1, 14.7.4.4). Takes
    /// precedence over `struct_parent`: a form has at most one of the two.
    pub struct_parents: Option<i64>,
    /// `/OPI`: Open Prepress Interface dictionary (ISO 32000-1, 14.11.7).
    pub opi: Option<BTreeMap<String, DictItem>>,
    /// `/OC`: optional content group or membership dictionary. A form that is hidden by it is
    /// skipped as if its `Do` were absent (ISO 32000-1, 8.11).
    pub oc: Option<BTreeMap<String, DictItem>>,
    /// `/Name`: the name the form is listed under in the resources. Obsolete since PDF 1.1
    /// (ISO 32000-1, 8.10.2), written as a string.
    pub name: Option<String>,
    /// What the model has no field for, kept as it was read and written back: entries of the
    /// form's dictionary (`/OC`, `/Metadata`, `/PieceInfo`, `/Group` with attributes), and under
    /// the key `Resources` the resource categories that have no typed map
    /// (`/ColorSpace`, `/Pattern`, `/Properties`, `/ProcSet`) and the graphics states and
    /// shadings that the typed maps of [`resources`](FormXObject::resources) cannot hold
    /// without loss. An entry written here wins over the field or typed entry of the same name.
    #[serde(default)]
    pub extra: BTreeMap<String, DictItem>,
}

fn empty_bbox() -> Rect {
    Rect::from_wh(Pt(0.0), Pt(0.0))
}

impl FormXObject {
    /// A form that paints `ops` inside `bbox`, in its own coordinate system, with no matrix,
    /// no resources of its own and no group.
    ///
    /// Without resources the names in `ops` are looked up in the resources of the document,
    /// so a font registered with [`PdfDocument::add_font`](crate::PdfDocument::add_font) can be
    /// selected. The standard fonts need no resource at all. To keep the form independent of the
    /// document, give it [`resources`](FormXObject::with_resources).
    pub fn new(bbox: Rect, ops: Vec<Op>) -> Self {
        FormXObject {
            form_type: FormType::Type1,
            size: None,
            bbox,
            matrix: None,
            ops,
            resources: None,
            group: None,
            ref_dict: None,
            metadata: None,
            piece_info: None,
            last_modified: None,
            struct_parent: None,
            struct_parents: None,
            opi: None,
            oc: None,
            name: None,
            extra: BTreeMap::new(),
        }
    }

    /// A form from the bytes of a content stream, such as `0 0 1 rg 10 10 50 50 re f`.
    ///
    /// The operators become [`Op`]s the way they do when a page is read: text is decoded with
    /// no font information, so use [`FormXObject::new`] for text in an embedded font.
    ///
    /// # Errors
    ///
    /// Returns a message when `content` is not a valid content stream.
    pub fn from_content_stream(bbox: Rect, content: &[u8]) -> Result<Self, String> {
        let ops = ExternalStream::get_ops_internal(content)?;
        Ok(FormXObject::new(bbox, ops))
    }

    /// Sets [`matrix`](FormXObject::matrix) and returns the form.
    pub fn with_matrix(mut self, matrix: CurTransMat) -> Self {
        self.matrix = Some(matrix);
        self
    }

    /// Sets [`resources`](FormXObject::resources) to a scope of the form's own and returns the
    /// form.
    pub fn with_resources(mut self, resources: PdfResources) -> Self {
        self.resources = Some(resources);
        self
    }

    /// Sets [`group`](FormXObject::group) and returns the form.
    pub fn with_group(mut self, group: GroupXObject) -> Self {
        self.group = Some(group);
        self
    }
}

/// Converts a map of [`DictItem`]s to a [`lopdf::Dictionary`]. Nested streams stay inline.
pub fn build_dict(r: &BTreeMap<String, DictItem>) -> lopdf::Dictionary {
    lopdf::Dictionary::from_iter(r.iter().map(|(k, v)| (k.clone(), v.to_lopdf())))
}

/// The `/FormType` of a form XObject (ISO 32000-1, 8.10.2).
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FormType {
    /// The only form type defined.
    Type1,
}

impl FormType {
    /// The number written as `/FormType`.
    pub(crate) fn number(&self) -> i64 {
        match self {
            FormType::Type1 => 1,
        }
    }
}

/// The `/Group` dictionary of a form: the attributes of a group of objects that are
/// composited together (ISO 32000-1, 8.10.3 and 11.6.6).
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupXObject {
    /// The kind of group, written as `/S`.
    #[serde(default)]
    pub group_type: GroupXObjectType,
}

/// The group subtype of a [`GroupXObject`].
#[derive(Debug, Default, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GroupXObjectType {
    /// A transparency group (`/S /Transparency`), the only group type there is.
    #[default]
    TransparencyGroup,
}

impl GroupXObjectType {
    /// The PDF name of the group type, without the slash.
    pub fn get_id(&self) -> &'static str {
        match self {
            GroupXObjectType::TransparencyGroup => "Transparency",
        }
    }
}

/// A reference to a page of another PDF file (ISO 32000-1, 8.10.4). Not used by the crate.
#[derive(Debug, PartialEq, Clone, Default, Deserialize, Serialize)]
pub struct ReferenceXObject {
    /// The target document.
    pub file: Vec<u8>,
    /// The page of the target document.
    pub page: i64,
    /// Document id and version id of the target, taken from its metadata.
    pub id: [i64; 2],
}

/// A PostScript XObject (ISO 32000-1, 8.8.2). Not used by the crate.
#[derive(Debug, PartialEq, Clone, Default, Deserialize, Serialize)]
pub struct PostScriptXObject {
    /// Content for interpreters that only support PostScript level 1.
    #[allow(dead_code)]
    pub level1: Option<Vec<u8>>,
}

/// Where and how large an XObject is painted. All fields are optional; the default paints it at
/// the bottom left corner of the page, at its pixel size at 300 dpi.
///
/// The matrices are applied in this order: scale the unit square to the pixel size of the
/// XObject (at `dpi`), `scale_x` and `scale_y`, rotate around `rotate`'s center, then move by
/// `translate_x` and `translate_y`. Only the XObject is affected, the rest of the page is not.
///
/// # Examples
///
/// ```
/// use printpdf::*;
///
/// // A 64 x 64 pixel grey image, painted 128 pt wide, 200 pt from the left and bottom edge.
/// let image = RawImage {
///     pixels: RawImageData::U8(vec![128; 64 * 64]),
///     width: 64,
///     height: 64,
///     data_format: RawImageFormat::R8,
///     tag: Vec::new(),
/// };
/// let mut doc = PdfDocument::new("Placing an image");
/// let id = doc.add_image(&image);
/// let op = Op::UseXobject {
///     id,
///     transform: XObjectTransform {
///         dpi: Some(72.0), // at 72 dpi, 1 pixel is 1 pt
///         scale_x: Some(2.0),
///         scale_y: Some(2.0),
///         translate_x: Some(Pt(200.0)),
///         translate_y: Some(Pt(200.0)),
///         ..Default::default()
///     },
/// };
/// let page = PdfPage::new(Mm(210.0), Mm(297.0), vec![op]);
/// let bytes = doc.with_pages(vec![page]).save(&PdfSaveOptions::default(), &mut Vec::new());
/// assert!(bytes.starts_with(b"%PDF"));
/// ```
#[derive(Debug, Copy, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct XObjectTransform {
    /// Distance to the right, in points, from the origin of the current coordinate system
    /// (the left edge of the page). Applied last.
    #[serde(default)]
    pub translate_x: Option<Pt>,
    /// Distance upwards, in points, from the origin (the bottom edge of the page). Applied last.
    #[serde(default)]
    pub translate_y: Option<Pt>,
    /// Rotation, counter-clockwise, around a point of the XObject.
    #[serde(default)]
    pub rotate: Option<XObjectRotation>,
    /// Horizontal scale factor on top of the size scale, 1.0 for none.
    #[serde(default)]
    pub scale_x: Option<f32>,
    /// Vertical scale factor on top of the size scale, 1.0 for none.
    #[serde(default)]
    pub scale_y: Option<f32>,
    /// Dots per inch that turn the pixel size of the XObject into points: 1 pixel is
    /// `72 / dpi` pt. Also converts the rotation center. 300 when unset.
    #[serde(default)]
    pub dpi: Option<f32>,
    /// Skips the scale from the unit square to the pixel size. Set it for content that already
    /// places itself with its own `cm` operators, such as XObjects read from a PDF.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_auto_scale: bool,
}

impl XObjectTransform {
    /// The matrices that place the XObject, in the order they apply (see the type docs).
    ///
    /// `wh` is the pixel size of the XObject ([`XObject::get_width_height`]); with `None`, the
    /// scale from the unit square to the pixel size is left out.
    pub fn get_ctms(&self, wh: Option<(Px, Px)>) -> Vec<CurTransMat> {
        let mut transforms = Vec::new();
        let dpi = self.dpi.unwrap_or(300.0);

        if let Some((w, h)) = wh {
            // PDF paints an image into a 1x1 square, so scale it to its size in points.
            transforms.push(CurTransMat::Scale(w.into_pt(dpi).0, h.into_pt(dpi).0));
        }

        if self.scale_x.is_some() || self.scale_y.is_some() {
            let scale_x = self.scale_x.unwrap_or(1.0);
            let scale_y = self.scale_y.unwrap_or(1.0);
            transforms.push(CurTransMat::Scale(scale_x, scale_y));
        }

        if let Some(rotate) = self.rotate.as_ref() {
            transforms.push(CurTransMat::Translate(
                Pt(-rotate.rotation_center_x.into_pt(dpi).0),
                Pt(-rotate.rotation_center_y.into_pt(dpi).0),
            ));
            transforms.push(CurTransMat::Rotate(rotate.angle_ccw_degrees));
            transforms.push(CurTransMat::Translate(
                rotate.rotation_center_x.into_pt(dpi),
                rotate.rotation_center_y.into_pt(dpi),
            ));
        }

        if self.translate_x.is_some() || self.translate_y.is_some() {
            transforms.push(CurTransMat::Translate(
                self.translate_x.unwrap_or(Pt(0.0)),
                self.translate_y.unwrap_or(Pt(0.0)),
            ));
        }

        transforms
    }

    /// The matrices of [`XObjectTransform::get_ctms`] combined into one `[a b c d e f]`: what the
    /// `cm` operator before the `Do` of the XObject sets, applied before the CTM of the page.
    pub(crate) fn placement_matrix(&self, wh: Option<(Px, Px)>) -> [f32; 6] {
        self.get_ctms(wh)
            .into_iter()
            .fold(CurTransMat::Identity.as_array(), |m, q| {
                CurTransMat::combine_matrix(m, q.as_array())
            })
    }

    /// The matrices of [`XObjectTransform::get_ctms`] without the size scale, combined into one
    /// SVG `matrix(a b c d e f)` string.
    pub fn as_svg_transform(&self) -> String {
        let ctms = self.get_ctms(None);
        let mut combined = CurTransMat::Identity;

        for t in ctms {
            let new_arr = CurTransMat::combine_matrix(combined.as_array(), t.as_array());
            combined = CurTransMat::Raw(new_arr);
        }

        let arr = combined.as_array();
        format!(
            "matrix({} {} {} {} {} {})",
            arr[0], arr[1], arr[2], arr[3], arr[4], arr[5]
        )
    }
}

/// A rotation of an XObject around a center point.
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XObjectRotation {
    /// Angle in degrees, counter-clockwise.
    #[serde(default)]
    pub angle_ccw_degrees: f32,
    /// Horizontal position of the center in pixels, measured from the left edge of the
    /// XObject and converted to points with the `dpi` of the [`XObjectTransform`].
    #[serde(default)]
    pub rotation_center_x: Px,
    /// Vertical position of the center in pixels, measured from the bottom edge of the XObject.
    #[serde(default)]
    pub rotation_center_y: Px,
}
