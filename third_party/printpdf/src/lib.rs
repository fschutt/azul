//! Create, read, edit and render PDF files in Rust.
//!
//! A PDF is a list of pages. Each page holds a *content stream*: a list of small drawing
//! instructions such as "select this font", "move here", "show this text", "draw a rectangle".
//! `printpdf` models a document as plain Rust data. A [`PdfDocument`] owns its pages and its
//! shared resources (fonts, images, ...), a [`PdfPage`] owns a `Vec<`[`Op`]`>`, and every `Op`
//! is one instruction of the content stream. Saving turns this data into PDF bytes, parsing
//! turns PDF bytes back into it.
//!
//! # A first program
//!
//! ```
//! use printpdf::*;
//!
//! let mut doc = PdfDocument::new("My first PDF");
//!
//! let ops = vec![
//!     // A filled rectangle: 20 mm from the left and bottom edge, 60 mm x 20 mm.
//!     Op::SetFillColor { col: Color::Rgb(Rgb::new(0.9, 0.9, 0.5, None)) },
//!     Op::DrawRectangle {
//!         rectangle: Rect {
//!             x: Mm(20.0).into(),
//!             y: Mm(250.0).into(),
//!             width: Mm(60.0).into(),
//!             height: Mm(20.0).into(),
//!             mode: Some(PaintMode::Fill),
//!             winding_order: Some(WindingOrder::NonZero),
//!         },
//!     },
//!     // Text goes between StartTextSection and EndTextSection.
//!     Op::SetFillColor { col: Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)) },
//!     Op::StartTextSection,
//!     Op::SetTextCursor { pos: Point::new(Mm(25.0), Mm(258.0)) },
//!     Op::SetFont {
//!         font: PdfFontHandle::Builtin(BuiltinFont::Helvetica),
//!         size: Pt(18.0),
//!     },
//!     Op::ShowText { items: vec![TextItem::Text("Hello, PDF!".to_string())] },
//!     Op::EndTextSection,
//! ];
//!
//! let page = PdfPage::new(Mm(210.0), Mm(297.0), ops); // A4 portrait
//! let mut warnings = Vec::new();
//! let bytes: Vec<u8> = doc
//!     .with_pages(vec![page])
//!     .save(&PdfSaveOptions::default(), &mut warnings);
//!
//! assert!(bytes.starts_with(b"%PDF"));
//! // std::fs::write("hello.pdf", bytes).unwrap();
//! ```
//!
//! [`PdfDocument::save`] reports problems it can work around (for example text outside a text
//! section) as [`PdfWarnMsg`] entries in `warnings` instead of failing.
//!
//! # The document, page and `Op` model
//!
//! - [`PdfDocument`]: metadata ([`PdfMetadata`]), shared [`PdfResources`], bookmarks and pages.
//! - [`PdfResources`]: things that pages refer to by id and that are stored once per file:
//!   fonts ([`FontId`]), images and other XObjects ([`XObjectId`]), extended graphics states
//!   ([`ExtendedGraphicsStateId`]), shadings and layers. Register them with
//!   [`PdfDocument::add_font`], [`PdfDocument::add_image`] and the other `add_*` methods, which
//!   return the id to use in an `Op`.
//! - [`PdfPage`]: a page size and a `Vec<Op>`.
//! - [`Op`]: one content stream instruction. The documentation of each variant names the PDF
//!   operator it writes (`q`, `Tf`, `re`, ...).
//!
//! The *graphics state* is the set of current settings: colors, line width, font, transformation
//! matrix. `Op::SaveGraphicsState` (`q`) pushes it on a stack and `Op::RestoreGraphicsState`
//! (`Q`) pops it, so you can change settings for one shape and go back.
//!
//! The types derive `serde`, so a whole document can be written to JSON and read back.
//!
//! # Units and coordinates
//!
//! Positions and sizes are typed: [`Pt`] (points), [`Mm`] (millimeters) and [`Px`] (pixels).
//! One point is 1/72 inch, so 1 mm is about 2.8346 pt. Convert with `.into()`:
//! `let pt: Pt = Mm(10.0).into();`. A PDF itself only knows points; `Px` only exists to convert
//! image sizes at a given DPI.
//!
//! The origin of a page is its **bottom left** corner, x grows to the right and y grows
//! **up**. A [`Point`] with y = `Mm(280.0)` is near the top of an A4 page, not the bottom.
//!
//! # Fonts
//!
//! The 14 standard PDF fonts ([`BuiltinFont`]) need no embedding. For anything else, parse a
//! TrueType or OpenType file with [`ParsedFont::from_bytes`] and register it:
//!
//! ```
//! use printpdf::*;
//! # let font_bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/assets/fonts/RobotoMedium.ttf")).to_vec();
//! // `font_bytes` is the content of a .ttf or .otf file.
//! let mut doc = PdfDocument::new("Fonts");
//! let font = ParsedFont::from_bytes(&font_bytes, 0, &mut Vec::new()).unwrap();
//! let font_id = doc.add_font(&font);
//!
//! let ops = vec![
//!     Op::StartTextSection,
//!     Op::SetTextCursor { pos: Point::new(Mm(10.0), Mm(280.0)) },
//!     Op::SetFont { font: PdfFontHandle::External(font_id), size: Pt(16.0) },
//!     Op::ShowText { items: vec![TextItem::Text("Grüße".to_string())] },
//!     Op::EndTextSection,
//! ];
//! doc.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), ops));
//! // Only the glyphs that are used are embedded (`PdfSaveOptions::subset_fonts`).
//! let bytes = doc.save(&PdfSaveOptions::default(), &mut Vec::new());
//! assert!(bytes.starts_with(b"%PDF"));
//! ```
//!
//! PDF has no variable fonts, so a variable font (one file with weight or width axes) that goes
//! through `add_font` is embedded at its default instance. [`PdfDocument::add_variable_font`]
//! creates the instance you choose first (feature `text_layout`, see [`variable_font`]).
//!
//! # Images
//!
//! Requires the `images` feature (plus a format feature such as `png` or `bmp`). Decode an
//! image to a [`RawImage`], register it with [`PdfDocument::add_image`] and place it with
//! `Op::UseXobject`:
//!
//! ```
//! # #[cfg(feature = "png")]
//! # {
//! use printpdf::*;
//! # let png_bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/assets/img/dog_alpha.png"));
//! let mut doc = PdfDocument::new("Images");
//! let mut warnings = Vec::new();
//! let image = RawImage::decode_from_bytes(png_bytes, &mut warnings).unwrap();
//! let image_id = doc.add_image(&image);
//!
//! // One image pixel is drawn as one dot at 300 dpi unless `XObjectTransform::dpi` says otherwise.
//! let ops = vec![Op::UseXobject {
//!     id: image_id,
//!     transform: XObjectTransform {
//!         translate_x: Some(Mm(10.0).into()),
//!         translate_y: Some(Mm(100.0).into()),
//!         ..Default::default()
//!     },
//! }];
//! doc.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), ops));
//! # }
//! ```
//!
//! # Reading a PDF
//!
//! [`PdfDocument::parse`] reads PDF bytes into a [`PdfDocument`]. Fonts, images, bookmarks and
//! the page content streams (as `Op`s) are recovered. [`PdfDocument::extract_text`] returns
//! the text of every page, [`PdfDocument::page_to_svg`] renders a page as SVG.
//!
//! ```
//! use printpdf::*;
//! # let bytes = PdfDocument::new("x").save(&PdfSaveOptions::default(), &mut Vec::new());
//! let mut warnings = Vec::new();
//! let doc = PdfDocument::parse(&bytes, &PdfParseOptions::default(), &mut warnings).unwrap();
//! println!("{} pages", doc.page_count());
//! let text_per_page: Vec<Vec<String>> = doc.extract_text();
//! # assert_eq!(text_per_page.len(), doc.page_count());
//! ```
//!
//! The parser does not trust the file. What it cannot read (a page, a font, an image, a stream
//! that inflates past 256 MiB) is dropped with an entry in `warnings`, and a file it cannot load
//! at all returns `Err`. The code of this crate does not panic on bad input: CI runs clippy with
//! `unwrap`, `expect`, indexing and string slicing denied. That check does not cover the
//! dependencies (`lopdf`, `allsorts`, `image`, `azul-layout`).
//!
//! Merge documents with [`PdfDocument::append_document`] and [`PdfDocument::insert_document`].
//! The [`forms`] module reads and fills form fields, [`optimize`] shrinks finished files.
//!
//! # HTML to PDF
//!
//! Requires the `html` feature (on by default). [`PdfDocument::from_html`] lays out HTML and
//! CSS and breaks it into pages. Images and fonts used by the HTML are passed in by name.
//! The supported subset of HTML and CSS is smaller than a browser's; for pixel-exact control
//! position the content with `Op`s instead.
//!
//! ```
//! # #[cfg(feature = "html")]
//! # {
//! use std::collections::BTreeMap;
//! use printpdf::*;
//!
//! let html = r#"<html><body style="padding:10mm">
//!     <h1>Hello</h1><p>This text is laid out and paginated.</p>
//! </body></html>"#;
//!
//! let images: BTreeMap<String, Base64OrRaw> = BTreeMap::new(); // name used in <img src> -> bytes
//! let fonts: BTreeMap<String, Base64OrRaw> = BTreeMap::new();  // font family name -> bytes
//! let options = GeneratePdfOptions::default(); // A4, no margins
//! let mut warnings = Vec::new();
//! let doc = PdfDocument::from_html(html, &images, &fonts, &options, &mut warnings).unwrap();
//! let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);
//! # assert!(bytes.starts_with(b"%PDF"));
//! # }
//! ```
//!
//! # Feature flags
//!
//! Features exist to keep dependencies out of your build. Turn off what you do not use with
//! `default-features = false` and enable only the features you need. Without any feature,
//! printpdf writes and reads PDFs with the 14 built-in fonts and pulls in the smallest set of
//! dependencies.
//!
//! - **Default**
//!   - `html`: [`PdfDocument::from_html`] and the [`html`] module, which lay out HTML and CSS
//!     with `azul-layout`. It switches on these two, so they are on by default as well:
//!     - `text_layout_hyphenation`: hyphenation for English (US) in the layout (a 91 KB
//!       dictionary). Other languages are not hyphenated.
//!       - `text_layout`: text shaping and font parsing with `azul-layout` ([`shape`],
//!         `text_shaping`, [`ParsedFont`] with full font metrics).
//! - **Not default**
//!   - `images`: decode image files with the `image` crate ([`RawImage::decode_from_bytes`]).
//!     JPEG is always included. Enable the formats you need, each of them switches on `images`:
//!     - `png`, `gif`, `jpeg`, `bmp`, `tiff`, `webp`, `pnm`, `ico`, `tga`, `hdr`, `dds`
//!     - `rayon`: decode images on several threads.
//!   - `svg`: `Svg::parse` turns an SVG string into an XObject (via `svg2pdf`), and SVG works
//!     inside HTML. Switches on `html`.
//!   - `html_hyphenation_all`: `html`, with the hyphenation dictionaries of all 82 languages
//!     (`text_layout_hyphenation_all`, 2.9 MB more in the binary).
//!   - `html_multithreaded`: `html`, with fonts loaded on several threads. Not for WASM.
//!   - `text_layout`: the text shaping above, without `html`. Use it if you want
//!     [`shape`] but not the HTML layout.
//!   - `js-sys`: in the browser (`wasm32-unknown-unknown`), timestamps from `Date` and image
//!     decoding by the browser.
//!
//! # Where to go next
//!
//! - [`ops`]: every `Op` and the page type.
//! - [`graphics`], [`color`], [`matrix`], [`units`]: shapes, colors, transformation matrices, units.
//! - [`font`], [`text`], [`shape`]: fonts, text items and manual text shaping.
//! - [`xobject`], [`image_types`]: images and reusable drawings.
//! - [`serialize`], [`deserialize`]: writing and reading, with their options.
//! - [`annotation`]: bookmarks and links. [`conformance`]: PDF/A and PDF/X profiles.
//!
//! The repository's `examples/` folder has runnable programs for most of these.

#![warn(missing_docs)]
// A failure comes back as an `Err`, a warning or a skipped item, never as a panic: on
// `wasm32-unknown-unknown` a panic traps and cannot be caught. `cargo clippy` enforces it.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use std::collections::BTreeMap;

use serde_derive::{Deserialize, Serialize};

pub mod annotation;
pub use annotation::*;
pub mod cmap;
pub use cmap::*;
pub mod text;
pub use text::*;
pub mod wasm;
pub use wasm::*;
pub mod conformance;
pub use conformance::*;
pub mod matrix;
pub use matrix::*;
pub mod units;
pub use units::*;
pub mod date;
pub use date::*;
pub mod font;
pub use font::*;
#[cfg(feature = "text_layout")]
pub mod variable_font;
#[cfg(feature = "text_layout")]
pub use variable_font::*;
pub mod shape;
pub use shape::*;
pub mod graphics;
pub use graphics::*;
pub mod shading;
pub use shading::*;
pub mod ops;
pub mod text_boxes;
pub use ops::*;
pub mod color;
pub use color::*;
pub mod xobject;
pub use xobject::*;
pub mod image_types;
pub use image_types::*;
#[cfg(feature = "svg")]
pub mod svg;
#[cfg(feature = "svg")]
pub use svg::*;
#[cfg(feature = "images")]
pub mod image;
#[cfg(feature = "images")]
pub use image::*;
pub mod optimize;
pub use optimize::*;
#[cfg(feature = "html")]
pub mod html;
#[cfg(feature = "html")]
pub use html::*;

/// Re-exports of azul's text layout types, for use with [`shape`].
#[cfg(feature = "text_layout")]
pub mod text_shaping {
    pub use azul_layout::text3::cache::{FontManager, UnifiedLayout, ParsedFontTrait, LoadedFonts};
    pub use azul_layout::text3::glyphs::{get_glyph_runs_pdf, PdfGlyphRun, PdfPositionedGlyph};
    pub use azul_css::props::basic::ColorU;
    pub use crate::shape::{layout_to_ops, layout_to_ops_with_offset};
    #[cfg(feature = "html")]
    pub use crate::html::bridge::render_unified_layout_public;
}
#[cfg(feature = "html")]
pub mod components;
#[cfg(feature = "html")]
pub use components::*;
pub mod utils;
use utils::*;
pub mod serialize;
/// Op clean-up the serializer runs with `PdfSaveOptions::optimize`
mod text_merge;
/// How the consumers of a page's ops walk into the forms it draws
mod form_walk;
pub(crate) mod encoding;
pub use serialize::*;
pub mod forms;
pub mod deserialize;
pub use deserialize::*;
/// Rendering PDF to SVG (always available)
pub(crate) mod render;
pub use render::*;

/// Page setup for [`PdfDocument::from_html`].
///
/// Every field is optional and falls back to the default named in its documentation. The type is serialized to JSON with the field names as written; `None` fields are omitted.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct GeneratePdfOptions {
    /// Page width in mm. Default 210.0 (A4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_width: Option<f32>,
    /// Page height in mm. Default 297.0 (A4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_height: Option<f32>,
    /// Top margin in mm. Default 0.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_top: Option<f32>,
    /// Right margin in mm. Default 0.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_right: Option<f32>,
    /// Bottom margin in mm. Default 0.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_bottom: Option<f32>,
    /// Left margin in mm. Default 0.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_left: Option<f32>,
    /// Page numbers in the footer, one per page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_page_numbers: Option<bool>,
    /// Text for the header of every page, except the first if `skip_first_page` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_text: Option<String>,
    /// Text for the footer, in addition to the page numbers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footer_text: Option<String>,
    /// Leave header and footer off the first page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_first_page: Option<bool>,
}

impl Default for GeneratePdfOptions {
    /// A4 portrait (210 x 297 mm) with no margins.
    fn default() -> Self {
        Self {
            page_width: Some(210.0),
            page_height: Some(297.0),
            margin_top: None,
            margin_right: None,
            margin_bottom: None,
            margin_left: None,
            show_page_numbers: None,
            header_text: None,
            footer_text: None,
            skip_first_page: None,
        }
    }
}

impl GeneratePdfOptions {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Binary data as raw bytes or as a base64 string.
///
/// A string may also be a `data:...;base64,...` URL. The image and font maps of
/// [`PdfDocument::from_html`] use this type.
///
/// Base64 exists for JavaScript callers, where `atob` / `btoa` avoid mismatches between
/// `ArrayBuffer`, `Uint8Array` and byte arrays. The enum is `#[serde(untagged)]`: JSON can hold
/// either a string or an array of numbers.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(untagged)]
pub enum Base64OrRaw {
    /// Base64 string, with or without a `data:...;base64,` prefix.
    B64(String),
    /// Raw bytes
    Raw(Vec<u8>),
}

impl Default for Base64OrRaw {
    fn default() -> Self {
        Base64OrRaw::Raw(Vec::new())
    }
}

impl Base64OrRaw {
    /// Returns the bytes, decoding base64 (and dropping a `data:...;base64,` prefix) if needed.
    ///
    /// # Errors
    ///
    /// Returns the decoder's message when a [`Base64OrRaw::B64`] value is not valid base64.
    pub fn decode_bytes(&self) -> Result<Vec<u8>, String> {
        use base64::Engine;
        match self {
            Base64OrRaw::B64(r) => base64::prelude::BASE64_STANDARD
                .decode(get_base64_substr(r))
                .map_err(|e| e.to_string()),
            Base64OrRaw::Raw(r) => Ok(r.clone()),
        }
    }
}

fn get_base64_substr(input: &str) -> &str {
    match input.strip_prefix("data:").and_then(|rest| rest.split_once(',')) {
        Some((metadata, payload)) if metadata.contains("base64") => payload,
        // not a data URL, or a data URL that is not marked base64: decode the whole string
        _ => input,
    }
}

/// Id of a bookmark.
///
/// It is the key in [`PdfDocument::bookmarks`].
///
/// The wrapped string is the resource name used in the PDF.
#[derive(Debug, PartialEq, Clone, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PageAnnotId(pub String);

impl PageAnnotId {
    /// Creates an id from a random 32 character string.
    pub fn new() -> Self {
        Self(crate::utils::random_character_string_32())
    }
}

impl Default for PageAnnotId {
    fn default() -> Self {
        Self::new()
    }
}

/// Id of an XObject, for [`Op::UseXobject`].
///
/// It is the key in [`PdfResources::xobjects`]. An XObject is an image, an SVG or another reusable
/// drawing.
///
/// The wrapped string is the resource name used in the PDF.
#[derive(Debug, PartialEq, Clone, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct XObjectId(pub String);

impl XObjectId {
    /// Creates an id from a random 32 character string.
    pub fn new() -> Self {
        Self(crate::utils::random_character_string_32())
    }
}

impl Default for XObjectId {
    fn default() -> Self {
        Self::new()
    }
}

/// Id of an embedded font, for [`PdfFontHandle::External`].
///
/// It is the key in [`PdfResources::fonts`].
///
/// The wrapped string is the resource name used in the PDF.
#[derive(Debug, PartialEq, Hash, Clone, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FontId(pub String);

impl FontId {
    /// Creates an id from a random 32 character string.
    pub fn new() -> Self {
        Self(crate::utils::random_character_string_32())
    }
}

impl Default for FontId {
    fn default() -> Self {
        Self::new()
    }
}

/// Id of a layer, for [`Op::BeginLayer`].
///
/// A layer is an optional content group. The id is the key in [`PdfResources::layers`].
///
/// The wrapped string is the resource name used in the PDF.
#[derive(Debug, PartialEq, Clone, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LayerInternalId(pub String);

impl LayerInternalId {
    /// Creates an id from a random 32 character string.
    pub fn new() -> Self {
        Self(crate::utils::random_character_string_32())
    }
}

impl Default for LayerInternalId {
    fn default() -> Self {
        Self::new()
    }
}

/// Id of an extended graphics state, for [`Op::LoadGraphicsState`].
///
/// It is the key in [`PdfResources::extgstates`].
///
/// The wrapped string is the resource name used in the PDF.
#[derive(Debug, PartialEq, Clone, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExtendedGraphicsStateId(pub String);

impl ExtendedGraphicsStateId {
    /// Creates an id from a random 32 character string.
    pub fn new() -> Self {
        Self(crate::utils::random_character_string_32())
    }
}

impl Default for ExtendedGraphicsStateId {
    fn default() -> Self {
        Self::new()
    }
}

/// Id of an ICC color profile.
///
/// The wrapped string is the resource name used in the PDF.
#[derive(Debug, PartialEq, Clone, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IccProfileId(pub String);

impl IccProfileId {
    /// Creates an id from a random 32 character string.
    pub fn new() -> Self {
        Self(crate::utils::random_character_string_32())
    }
}

impl Default for IccProfileId {
    fn default() -> Self {
        Self::new()
    }
}

/// A PDF document with its pages and shared resources.
///
/// It holds the metadata, the shared resources, the bookmarks and the pages.
///
/// Build one with [`PdfDocument::new`], [`PdfDocument::parse`] or [`PdfDocument::from_html`],
/// write it with [`PdfDocument::save`]. Serializes to JSON with camelCase field names.
///
/// # Examples
///
/// ```
/// use printpdf::*;
///
/// let mut doc = PdfDocument::new("Two blank pages");
/// doc.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), Vec::new()));
/// doc.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), Vec::new()));
/// let bytes = doc.save(&PdfSaveOptions::default(), &mut Vec::new());
/// assert_eq!(doc.page_count(), 2);
/// assert!(bytes.starts_with(b"%PDF"));
/// ```
#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfDocument {
    /// Title, author, dates and the PDF conformance level.
    pub metadata: PdfMetadata,
    /// Fonts, XObjects, graphics states, shadings and layers that pages refer to by id.
    pub resources: PdfResources,
    /// Bookmarks (the outline panel of a viewer). Page numbers start at 1.
    pub bookmarks: PageAnnotMap,
    /// The pages in reading order. The first page has index 0 here.
    pub pages: Vec<PdfPage>,
}

impl PdfDocument {
    /// Creates a document without pages. `name` becomes the document title.
    pub fn new(name: &str) -> Self {
        Self {
            metadata: PdfMetadata {
                info: PdfDocumentInfo {
                    document_title: name.to_string(),
                    ..Default::default()
                },
                xmp: None,
            },
            resources: PdfResources::default(),
            bookmarks: PageAnnotMap::default(),
            pages: Vec::new(),
        }
    }

    /// Reads a PDF from its bytes.
    ///
    /// Problems that can be worked around are pushed to `warnings`.
    ///
    /// # Errors
    ///
    /// Returns a message when the bytes are not a loadable PDF or the file has no usable
    /// catalog.
    pub fn parse(
        bytes: &[u8],
        opts: &PdfParseOptions,
        warnings: &mut Vec<PdfWarnMsg>,
    ) -> Result<Self, String> {
        self::deserialize::parse_pdf_from_bytes(bytes, opts, warnings)
    }

    /// Like [`PdfDocument::parse`], but decodes images with the async decoder (the browser's
    /// own on WASM with the `js-sys` feature).
    pub async fn parse_async(
        bytes: &[u8],
        opts: &PdfParseOptions,
        warnings: &mut Vec<PdfWarnMsg>,
    ) -> Result<Self, String> {
        self::deserialize::parse_pdf_from_bytes_async(bytes, opts, warnings).await
    }

    /// Registers an extended graphics state (blend mode, opacity, overprint, ...) and returns
    /// the id to pass to `Op::LoadGraphicsState`.
    pub fn add_graphics_state(&mut self, gs: ExtendedGraphicsState) -> ExtendedGraphicsStateId {
        let id = ExtendedGraphicsStateId::new();
        self.resources.extgstates.map.insert(id.clone(), gs);
        id
    }

    /// Registers an axial or radial shading (a gradient) and returns the id for `Op::PaintShading`.
    pub fn add_shading(&mut self, shading: crate::shading::Shading) -> crate::shading::ShadingId {
        let id = crate::shading::ShadingId::new();
        self.resources.shadings.map.insert(id.clone(), shading);
        id
    }

    /// Registers a layer (optional content group) and returns the id for `Op::BeginLayer`.
    pub fn add_layer(&mut self, layer: &Layer) -> LayerInternalId {
        let id = LayerInternalId::new();
        self.resources.layers.map.insert(id.clone(), layer.clone());
        id
    }

    /// Registers a font and returns the id for [`PdfFontHandle::External`].
    pub fn add_font(&mut self, font: &ParsedFont) -> FontId {
        let id = FontId::new();
        let pdf_font = crate::font::PdfFont::new(font.clone());
        self.resources.fonts.map.insert(id.clone(), pdf_font);
        id
    }

    /// Extracts the text of every page. The outer `Vec` has one entry per page, the inner one
    /// holds the text of each text section of that page (see [`PdfPage::extract_text`]).
    pub fn extract_text(&self) -> Vec<Vec<String>> {
        self.pages
            .iter()
            .map(|page| page.extract_text(&self.resources))
            .collect()
    }

    /// Registers an image as an XObject and returns the id for `Op::UseXobject`.
    pub fn add_image(&mut self, image: &RawImage) -> XObjectId {
        let id = XObjectId::new();
        self.resources
            .xobjects
            .map
            .insert(id.clone(), XObject::Image(image.clone()));
        id
    }

    /// Registers an external XObject (for example the result of `Svg::parse`) and returns the
    /// id for `Op::UseXobject`.
    pub fn add_xobject(&mut self, parsed_svg: &ExternalXObject) -> XObjectId {
        let id = XObjectId::new();
        self.resources
            .xobjects
            .map
            .insert(id.clone(), XObject::External(parsed_svg.clone()));
        id
    }

    /// Registers a form (content that pages draw any number of times, such as a watermark or a
    /// letterhead) and returns the id for `Op::UseXobject`. See [`FormXObject`] for an example.
    pub fn add_form(&mut self, form: &FormXObject) -> XObjectId {
        let id = XObjectId::new();
        self.resources
            .xobjects
            .map
            .insert(id.clone(), XObject::Form(form.clone()));
        id
    }

    /// Adds a bookmark that jumps to `page` (counted from 1) and returns its id.
    ///
    /// A bookmark whose page does not exist is left out when saving.
    pub fn add_bookmark(&mut self, name: &str, page: usize) -> PageAnnotId {
        let id = PageAnnotId(format!(
            "bookmark_{:020}_{}",
            self.bookmarks.map.len(),
            crate::utils::random_character_string_32()
        ));
        self.bookmarks.map.insert(
            id.clone(),
            PageAnnotation {
                name: name.to_string(),
                page,
            },
        );
        id
    }

    /// Lays out HTML and CSS and breaks it into pages. Requires the `html` feature.
    ///
    /// `images` maps the name used in `<img src="...">` to the image bytes, `fonts` maps a font
    /// family name used in CSS to the font bytes. `options` sets the page size and margins.
    /// System fonts are used for families that are not in `fonts`. The document title is
    /// "PDF Document"; change it through [`PdfDocument::metadata`] afterwards.
    ///
    /// # Errors
    ///
    /// Returns the messages of the conversion (joined with `; `) when no page could be
    /// produced, or the decoder's message when a base64 font is invalid. The same messages
    /// are added to `warnings`.
    #[cfg(feature = "html")]
    pub fn from_html(
        html: &str,
        images: &BTreeMap<String, Base64OrRaw>,
        fonts: &BTreeMap<String, Base64OrRaw>,
        options: &GeneratePdfOptions,
        warnings: &mut Vec<PdfWarnMsg>,
    ) -> Result<Self, String> {
        Self::from_html_with_cache(html, images, fonts, options, warnings, None)
    }

    /// Like [`PdfDocument::from_html`], but takes a shared font pool so that fonts are loaded
    /// once when many documents are rendered.
    ///
    /// Build the pool once with [`crate::html::build_font_pool`] and pass it to every call.
    #[cfg(feature = "html")]
    pub fn from_html_with_cache(
        html: &str,
        images: &BTreeMap<String, Base64OrRaw>,
        fonts: &BTreeMap<String, Base64OrRaw>,
        options: &GeneratePdfOptions,
        warnings: &mut Vec<PdfWarnMsg>,
        font_pool: Option<crate::html::SharedFontPool>,
    ) -> Result<Self, String> {
        use crate::html::{XmlRenderOptions, PageMargins};
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        let mut pdf = Self::new("PDF Document");

        let mut xml_options = XmlRenderOptions::default();
        xml_options.page_width = Mm(options.page_width.unwrap_or(210.0));
        xml_options.page_height = Mm(options.page_height.unwrap_or(297.0));

        xml_options.margins = PageMargins {
            top: Mm(options.margin_top.unwrap_or(0.0)),
            right: Mm(options.margin_right.unwrap_or(0.0)),
            bottom: Mm(options.margin_bottom.unwrap_or(0.0)),
            left: Mm(options.margin_left.unwrap_or(0.0)),
        };

        xml_options.show_page_numbers = options.show_page_numbers.unwrap_or(false);
        xml_options.header_text = options.header_text.clone();
        xml_options.footer_text = options.footer_text.clone();
        xml_options.skip_first_page = options.skip_first_page.unwrap_or(false);

        // Images are not copied into `xml_options`: the render looks up (and
        // base64-decodes, and decodes) only the `<img src>` its pages draw, in
        // `images` itself. An entry the page does not show costs nothing.
        for (key, font) in fonts {
            let bytes = match font {
                Base64OrRaw::Raw(b) => b.clone(),
                Base64OrRaw::B64(s) => STANDARD.decode(s).map_err(|e| format!("Base64 decode error: {}", e))?,
            };
            xml_options.fonts.insert(key.clone(), bytes);
        }

        xml_options.font_pool = font_pool;

        match crate::html::xml_to_pdf_pages_with_images(html, &xml_options, images, warnings) {
            Ok((pages, font_data, images, bridge_res)) => {
                // The font id matches the one the bridge wrote into the page ops.
                for (font_hash, parsed_font) in font_data.iter() {
                    let font_id = FontId(format!("F{}", font_hash.font_hash));

                    let pdf_font = crate::font::PdfFont::new(parsed_font.clone());
                    pdf.resources.fonts.map.insert(font_id, pdf_font);
                }
                // `<img>` images become image XObjects under the ids of the page's `UseXobject` ops.
                for (_src, (xobject_id, raw_image)) in images.into_iter() {
                    pdf.resources
                        .xobjects
                        .map
                        .insert(xobject_id, crate::XObject::Image(raw_image));
                }

                // Opacity graphics states, gradient shadings and shadow XObjects that the
                // page ops refer to.
                bridge_res.register_into(&mut pdf.resources);

                pdf.pages.extend(pages);
                Ok(pdf)
            }
            Err(errs) => Err(html_conversion_failed(warnings, errs)),
        }
    }

    /// Like [`PdfDocument::from_html`], and also returns debug information about the display
    /// lists and the PDF operations. Requires the `html` feature.
    #[cfg(feature = "html")]
    pub fn from_html_debug(
        html: &str,
        images: &BTreeMap<String, Base64OrRaw>,
        fonts: &BTreeMap<String, Base64OrRaw>,
        options: &GeneratePdfOptions,
        warnings: &mut Vec<PdfWarnMsg>,
    ) -> Result<(Self, crate::html::PdfDebugInfo), String> {
        Self::from_html_debug_with_cache(html, images, fonts, options, warnings, None)
    }

    /// Like [`PdfDocument::from_html_debug`], but takes a shared font pool.
    #[cfg(feature = "html")]
    pub fn from_html_debug_with_cache(
        html: &str,
        images: &BTreeMap<String, Base64OrRaw>,
        fonts: &BTreeMap<String, Base64OrRaw>,
        options: &GeneratePdfOptions,
        warnings: &mut Vec<PdfWarnMsg>,
        font_pool: Option<crate::html::SharedFontPool>,
    ) -> Result<(Self, crate::html::PdfDebugInfo), String> {
        use crate::html::{XmlRenderOptions, PageMargins};
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        let mut pdf = Self::new("PDF Document");

        let mut xml_options = XmlRenderOptions::default();
        xml_options.page_width = Mm(options.page_width.unwrap_or(210.0));
        xml_options.page_height = Mm(options.page_height.unwrap_or(297.0));

        xml_options.margins = PageMargins {
            top: Mm(options.margin_top.unwrap_or(0.0)),
            right: Mm(options.margin_right.unwrap_or(0.0)),
            bottom: Mm(options.margin_bottom.unwrap_or(0.0)),
            left: Mm(options.margin_left.unwrap_or(0.0)),
        };

        xml_options.show_page_numbers = options.show_page_numbers.unwrap_or(false);
        xml_options.header_text = options.header_text.clone();
        xml_options.footer_text = options.footer_text.clone();
        xml_options.skip_first_page = options.skip_first_page.unwrap_or(false);

        // Images are not copied into `xml_options`: the render looks up (and
        // base64-decodes, and decodes) only the `<img src>` its pages draw, in
        // `images` itself. An entry the page does not show costs nothing.
        for (key, font) in fonts {
            let bytes = match font {
                Base64OrRaw::Raw(b) => b.clone(),
                Base64OrRaw::B64(s) => STANDARD.decode(s).map_err(|e| format!("Base64 decode error: {}", e))?,
            };
            xml_options.fonts.insert(key.clone(), bytes);
        }

        xml_options.font_pool = font_pool;

        match crate::html::xml_to_pdf_pages_debug_with_images(html, &xml_options, images, warnings) {
            Ok((pages, font_data, debug_info, bridge_res, images)) => {
                for (font_hash, parsed_font) in font_data.iter() {
                    let font_id = FontId(format!("F{}", font_hash.font_hash));
                    let pdf_font = crate::font::PdfFont::new(parsed_font.clone());
                    pdf.resources.fonts.map.insert(font_id, pdf_font);
                }
                for (_src, (xobject_id, raw_image)) in images.into_iter() {
                    pdf.resources
                        .xobjects
                        .map
                        .insert(xobject_id, crate::XObject::Image(raw_image));
                }

                bridge_res.register_into(&mut pdf.resources);

                pdf.pages.extend(pages);
                Ok((pdf, debug_info))
            }
            Err(errs) => Err(html_conversion_failed(warnings, errs)),
        }
    }
}

// A conversion that produced no pages is an error: the messages are the `Err` (joined) and
// also stay in `warnings`.
#[cfg(feature = "html")]
fn html_conversion_failed(warnings: &mut Vec<PdfWarnMsg>, errs: Vec<PdfWarnMsg>) -> String {
    let msg = if errs.is_empty() {
        "the HTML could not be converted".to_string()
    } else {
        errs.iter().map(|e| e.msg.as_str()).collect::<Vec<_>>().join("; ")
    };
    warnings.extend(errs);
    msg
}

impl PdfDocument {
    /// Renders a page as an SVG string. `page` counts from 1; returns `None` when the page does
    /// not exist (page 0 is treated as page 1).
    pub fn page_to_svg(
        &self,
        page: usize,
        opts: &PdfToSvgOptions,
        warnings: &mut Vec<PdfWarnMsg>,
    ) -> Option<String> {
        Some(
            self.pages
                .get(page.saturating_sub(1))?
                .to_svg(&self.resources, opts, warnings),
        )
    }

    /// Like [`PdfDocument::page_to_svg`], with the async image encoder.
    pub async fn page_to_svg_async(
        &self,
        page: usize,
        opts: &PdfToSvgOptions,
        warnings: &mut Vec<PdfWarnMsg>,
    ) -> Option<String> {
        Some(
            self.pages
                .get(page.saturating_sub(1))?
                .to_svg_async(&self.resources, opts, warnings)
                .await,
        )
    }

    /// Replaces the pages of the document and returns `self`, so calls can be chained.
    ///
    /// To merge the pages of another document instead, use [`PdfDocument::append_document`].
    pub fn with_pages(&mut self, pages: Vec<PdfPage>) -> &mut Self {
        self.pages = pages;
        self
    }

    /// Appends the pages of `other` to this document.
    ///
    /// The resources of `other` are merged in (on an id clash the resource already in this
    /// document wins) and its bookmarks are moved to the new page numbers.
    ///
    /// # Examples
    ///
    /// ```
    /// use printpdf::*;
    ///
    /// let mut main_doc = PdfDocument::new("Main");
    /// main_doc.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), Vec::new()));
    /// let mut other_doc = PdfDocument::new("Other");
    /// other_doc.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), Vec::new()));
    /// main_doc.append_document(other_doc);
    /// assert_eq!(main_doc.page_count(), 2);
    /// ```
    pub fn append_document(&mut self, other: PdfDocument) {
        let page_offset = self.pages.len();
        self.merge_resources(other.resources);
        self.merge_bookmarks(other.bookmarks, page_offset);
        self.pages.extend(other.pages);
    }

    /// Inserts the pages of `other` so that `insert_at` pages stay in front of them.
    ///
    /// `insert_at` is 0 to insert at the beginning and `page_count()` to append; larger values
    /// are clamped. Resources and bookmarks are merged like in
    /// [`PdfDocument::append_document`], and bookmarks of this document are moved to the new
    /// page numbers.
    ///
    /// # Examples
    ///
    /// ```
    /// use printpdf::*;
    ///
    /// let mut main_doc = PdfDocument::new("Main");
    /// main_doc.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), Vec::new()));
    /// let mut title = PdfDocument::new("Title");
    /// title.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), Vec::new()));
    /// main_doc.insert_document(0, title); // the title page becomes page 1
    /// assert_eq!(main_doc.page_count(), 2);
    /// ```
    pub fn insert_document(&mut self, insert_at: usize, other: PdfDocument) {
        let insert_at = insert_at.min(self.pages.len());
        let pages_to_insert = other.pages.len();

        for annot in self.bookmarks.map.values_mut() {
            if annot.page >= insert_at {
                annot.page = annot.page.saturating_add(pages_to_insert);
            }
        }

        self.merge_resources(other.resources);
        self.merge_bookmarks(other.bookmarks, insert_at);

        let mut new_pages = Vec::with_capacity(self.pages.len() + pages_to_insert);
        new_pages.extend(self.pages.drain(..insert_at));
        new_pages.extend(other.pages);
        new_pages.append(&mut self.pages);
        self.pages = new_pages;
    }

    // Existing entries win on an id clash.
    fn merge_resources(&mut self, other: PdfResources) {
        for (id, font) in other.fonts.map {
            self.resources.fonts.map.entry(id).or_insert(font);
        }
        for (id, xobj) in other.xobjects.map {
            self.resources.xobjects.map.entry(id).or_insert(xobj);
        }
        for (id, gs) in other.extgstates.map {
            self.resources.extgstates.map.entry(id).or_insert(gs);
        }
        for (id, sh) in other.shadings.map {
            self.resources.shadings.map.entry(id).or_insert(sh);
        }
        for (id, layer) in other.layers.map {
            self.resources.layers.map.entry(id).or_insert(layer);
        }
    }

    // Bookmark page numbers start at 1, so adding `page_offset` moves them behind that many pages.
    fn merge_bookmarks(&mut self, other: PageAnnotMap, page_offset: usize) {
        for (id, mut annot) in other.map {
            annot.page = annot.page.saturating_add(page_offset);
            self.bookmarks.map.insert(id, annot);
        }
    }

    /// Returns the number of pages.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Writes the PDF file to `w`. An I/O error of the writer is not reported.
    pub fn save_writer<W: std::io::Write>(
        &self,
        w: &mut W,
        opts: &PdfSaveOptions,
        warnings: &mut Vec<PdfWarnMsg>,
    ) {
        self::serialize::serialize_pdf(self, opts, w, warnings);
    }
    /// Writes the PDF file and returns its bytes.
    ///
    /// Problems that do not stop the output (text outside a text section, a color outside
    /// 0.0 to 1.0, a missing font) are added to `warnings`.
    pub fn save(&self, opts: &PdfSaveOptions, warnings: &mut Vec<PdfWarnMsg>) -> Vec<u8> {
        self::serialize::serialize_pdf_into_bytes(self, opts, warnings)
    }

    /// Builds the `lopdf::Document` that [`PdfDocument::save`] writes, for further changes with
    /// `lopdf`. Streams are not compressed yet.
    pub fn to_lopdf_document(
        &self,
        opts: &PdfSaveOptions,
        warnings: &mut Vec<PdfWarnMsg>,
    ) -> lopdf::Document {
        self::serialize::to_lopdf_doc(self, opts, warnings)
    }

    /// Like [`PdfDocument::save`], for async callers. The work itself is synchronous.
    pub async fn save_async(
        &self,
        opts: &PdfSaveOptions,
        warnings: &mut Vec<PdfWarnMsg>,
    ) -> Vec<u8> {
        self::serialize::serialize_pdf_into_bytes(self, opts, warnings)
    }
}

/// The objects that pages share, stored once per file.
///
/// Pages refer to them by id. Add to them with the `add_*` methods of [`PdfDocument`]. Serialized as camelCase JSON.
#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfResources {
    /// Fonts, by the [`FontId`] that `Op::SetFont` uses.
    #[serde(default)]
    pub fonts: PdfFontMap,
    /// XObjects (images, SVG drawings, embedded form content), by the id `Op::UseXobject` uses.
    #[serde(default)]
    pub xobjects: XObjectMap,
    /// Extended graphics states (blend mode, opacity, overprint), by the id `Op::LoadGraphicsState` uses.
    #[serde(default)]
    pub extgstates: ExtendedGraphicsStateMap,
    /// Axial and radial shadings (gradients), by the id `Op::PaintShading` uses.
    #[serde(default)]
    pub shadings: ShadingMap,
    /// Layers (optional content groups), by the id `Op::BeginLayer` uses.
    #[serde(default)]
    pub layers: PdfLayerMap,
}

/// Layers by id.
///
/// Serialized as the plain map.
#[derive(Debug, PartialEq, Default, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PdfLayerMap {
    /// The layers.
    pub map: BTreeMap<LayerInternalId, Layer>,
}

/// Fonts by id.
///
/// Serialized as the plain map.
#[derive(Debug, PartialEq, Default, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PdfFontMap {
    /// The fonts.
    pub map: BTreeMap<FontId, crate::font::PdfFont>,
}

/// XObjects by id.
///
/// Serialized as the plain map.
#[derive(Debug, PartialEq, Default, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct XObjectMap {
    /// The XObjects.
    pub map: BTreeMap<XObjectId, XObject>,
}

/// Bookmarks by id.
///
/// Serialized as the plain map.
#[derive(Debug, PartialEq, Default, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PageAnnotMap {
    /// The bookmarks.
    pub map: BTreeMap<PageAnnotId, PageAnnotation>,
}

/// Extended graphics states by id.
///
/// Serialized as the plain map.
#[derive(Debug, PartialEq, Default, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExtendedGraphicsStateMap {
    /// The graphics states.
    pub map: BTreeMap<ExtendedGraphicsStateId, ExtendedGraphicsState>,
}

/// Shadings by id.
///
/// Serialized as the plain map.
#[derive(Debug, PartialEq, Default, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ShadingMap {
    /// The shadings.
    pub map: BTreeMap<ShadingId, crate::shading::Shading>,
}

/// Document metadata that a PDF stores in two places.
///
/// The "Info" dictionary always has it, and conformance levels that require XMP metadata have it
/// there too. Keeping both in one struct keeps them in sync.
#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfMetadata {
    /// Title, author, dates and conformance level.
    #[serde(default)]
    pub info: PdfDocumentInfo,
    /// Extra XMP fields. Ignored on save when the PDF conformance does not allow XMP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xmp: Option<XmpMetadata>,
}

impl PdfMetadata {
    // Fills the XMP template with the fields of `info`.
    pub(crate) fn xmp_metadata_string(&self) -> String {
        let trapping = if self.info.trapped { "True" } else { "False" };

        let instance_id = random_character_string_32();
        let create_date = to_pdf_xmp_date(&self.info.creation_date);
        let modification_date = to_pdf_xmp_date(&self.info.modification_date);
        let metadata_date = to_pdf_xmp_date(&self.info.metadata_date);

        let pdf_x_version = self.info.conformance.get_identifier_string();
        let document_version = self.info.version.to_string();
        let document_id = self.info.identifier.to_string();

        let rendition_class = match self.xmp.as_ref().and_then(|s| s.rendition_class.clone()) {
            Some(class) => class,
            None => "".to_string(),
        };

        format!(
            include_str!("./res/catalog_xmp_metadata.txt"),
            create = create_date,
            modify = modification_date,
            mdate = metadata_date,
            title = self.info.document_title,
            id = document_id,
            instance = instance_id,
            class = rendition_class,
            version = document_version,
            pdfx = pdf_x_version,
            trapping = trapping,
            creator = self.info.creator,
            subject = self.info.subject,
            keywords = self.info.keywords.join(","),
            identifier = self.info.identifier,
            producer = self.info.producer
        )
    }
}

/// XMP fields that have no counterpart in [`PdfDocumentInfo`].
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XmpMetadata {
    /// Value of `xmpMM:RenditionClass`: "default", "proof", a web-viewable class, or `None` to
    /// leave it empty.
    #[serde(default)]
    pub rendition_class: Option<String>,
}

/// The "Info" dictionary of a PDF.
///
/// It holds the title, author, dates and the conformance level.
///
/// [`Default`] gives empty strings and the Unix epoch for all three dates.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfDocumentInfo {
    /// Whether the document has been trapped (prepress color overlap correction).
    pub trapped: bool,
    /// Version number of the document. Written to the XMP metadata (`xmpMM:VersionID`) only;
    /// it is not the PDF file version.
    pub version: u32,
    /// When the document was created.
    pub creation_date: OffsetDateTime,
    /// When the document was last modified.
    pub modification_date: OffsetDateTime,
    /// When the metadata was last modified.
    pub metadata_date: OffsetDateTime,
    /// The PDF standard (PDF/A, PDF/X, ...) the document claims to follow.
    pub conformance: PdfConformance,
    /// Document title.
    pub document_title: String,
    /// Author of the content.
    pub author: String,
    /// Application that created the content.
    pub creator: String,
    /// Application that wrote the PDF file.
    pub producer: String,
    /// Keywords, written comma separated.
    pub keywords: Vec<String>,
    /// Subject of the document.
    pub subject: String,
    /// Identifier of the document (for example an ISBN or an internal id).
    pub identifier: String,
}

impl Default for PdfDocumentInfo {
    fn default() -> Self {
        Self {
            trapped: false,
            version: 1,
            creation_date: OffsetDateTime::epoch(),
            modification_date: OffsetDateTime::epoch(),
            metadata_date: OffsetDateTime::epoch(),
            conformance: PdfConformance::default(),
            document_title: String::new(),
            author: String::new(),
            creator: String::new(),
            producer: String::new(),
            keywords: Vec::new(),
            subject: String::new(),
            identifier: String::new(),
        }
    }
}

#[cfg(test)]
mod with_pages_tests {
    use super::*;

    #[test]
    fn with_pages_replaces() {
        let mut doc = PdfDocument::new("test");
        doc.pages = vec![
            PdfPage::new(Mm(210.0), Mm(297.0), Vec::new()),
            PdfPage::new(Mm(210.0), Mm(297.0), Vec::new()),
        ];
        assert_eq!(doc.pages.len(), 2);

        doc.with_pages(vec![PdfPage::new(Mm(100.0), Mm(100.0), Vec::new())]);
        assert_eq!(doc.pages.len(), 1, "with_pages should replace, not append");
        assert_eq!(doc.pages[0].media_box.width, Pt::from(Mm(100.0)));
    }
}
