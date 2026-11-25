//! Input and output types of the JSON API, with the plain Rust function behind each JS function.
//!
//! [`api`](super::api) deserializes the JSON it gets from JS into one of the `*Input` types,
//! calls the function here, and serializes the `*Output` (or the error string) back. These types
//! and functions are available on every target, so native code can use the same operations
//! without the JSON and base64 round trip. Each operation has a sync function and an `_async`
//! twin that gives the same result.

use std::collections::BTreeMap;

use base64::Engine;
use serde_derive::{Deserialize, Serialize};

use crate::{
    Base64OrRaw, FontId, GeneratePdfOptions, LayerInternalId, PdfDocument, PdfPage,
    PdfParseOptions, PdfResources, PdfSaveOptions, PdfToSvgOptions, PdfWarnMsg, XObjectId,
};

/// Input of [`html_to_document`] (`Pdf_HtmlToDocument`).
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
pub struct HtmlToDocumentInput {
    /// HTML to render. Required.
    pub html: String,
    /// Images by name, drawn by `<img src="name">`, as base64 strings or byte arrays.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub images: BTreeMap<String, Base64OrRaw>,
    /// Fonts by name, used by `font-family: name`, as base64 strings or byte arrays. They are
    /// added to the fonts of [`register_fonts`]; an entry here wins over a registered one with
    /// the same name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fonts: BTreeMap<String, Base64OrRaw>,
    /// Page size, margins and the like. The keys are snake_case (`page_width`), unlike the
    /// other inputs.
    #[serde(default, skip_serializing_if = "GeneratePdfOptions::is_default")]
    pub options: GeneratePdfOptions,
}

/// Output of [`html_to_document`].
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct HtmlToDocumentOutput {
    /// The rendered document.
    pub doc: PdfDocument,
    /// Problems that did not stop the rendering.
    #[serde(default)]
    pub warnings: Vec<PdfWarnMsg>,
}

/// Renders HTML to a document with [`PdfDocument::from_html`].
///
/// Requires the `html` feature; without it the function returns an error.
///
/// # Errors
///
/// Returns the message of `from_html` when the HTML cannot be laid out.
#[cfg(feature = "html")]
pub fn html_to_document(input: HtmlToDocumentInput) -> Result<HtmlToDocumentOutput, String> {
    let mut warnings = Vec::new();
    let fonts = fonts_with_registered(&input.fonts);
    let pdf = PdfDocument::from_html(
        &input.html,
        &input.images,
        &fonts,
        &input.options,
        &mut warnings,
    )?;
    Ok(HtmlToDocumentOutput { doc: pdf, warnings })
}

#[cfg(not(feature = "html"))]
const ERR: &str = "Pdf_HtmlToDocument failed: feature --html not enabled for printpdf crate";

/// Renders HTML to a document; without the `html` feature this always returns an error.
#[cfg(not(feature = "html"))]
pub fn html_to_document(_input: HtmlToDocumentInput) -> Result<HtmlToDocumentOutput, String> {
    Err(ERR.to_string())
}

/// Like [`html_to_document`], as an async function.
#[cfg(feature = "html")]
pub async fn html_to_document_async(
    input: HtmlToDocumentInput,
) -> Result<HtmlToDocumentOutput, String> {
    let mut warnings = Vec::new();
    let fonts = fonts_with_registered(&input.fonts);
    let pdf = PdfDocument::from_html(
        &input.html,
        &input.images,
        &fonts,
        &input.options,
        &mut warnings,
    )?;
    Ok(HtmlToDocumentOutput { doc: pdf, warnings })
}

/// Like [`html_to_document`], as an async function; without the `html` feature this always
/// returns an error.
#[cfg(not(feature = "html"))]
pub async fn html_to_document_async(
    _input: HtmlToDocumentInput,
) -> Result<HtmlToDocumentOutput, String> {
    Err(ERR.to_string())
}

/// Input of [`bytes_to_document`] (`Pdf_BytesToDocument`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BytesToDocumentInput {
    /// The PDF file, as a base64 string or a byte array.
    pub bytes: Base64OrRaw,
    /// How to parse the file.
    #[serde(default)]
    pub options: PdfParseOptions,
}

/// Output of [`bytes_to_document`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BytesToDocumentOutput {
    /// The parsed document.
    pub doc: PdfDocument,
    /// Parts of the file that could not be read.
    #[serde(default)]
    pub warnings: Vec<PdfWarnMsg>,
}

/// Parses a PDF file with [`PdfDocument::parse`].
///
/// # Errors
///
/// Returns an error when the base64 is invalid or the file cannot be parsed.
pub fn bytes_to_document(input: BytesToDocumentInput) -> Result<BytesToDocumentOutput, String> {
    let bytes = input
        .bytes
        .decode_bytes()
        .map_err(|e| format!("failed to decode input bytes: {e}"))?;

    let mut warnings = Vec::new();
    let doc = PdfDocument::parse(&bytes, &input.options, &mut warnings)
        .map_err(|e| format!("failed to parse PDF: {e}"))?;

    Ok(BytesToDocumentOutput { doc, warnings })
}

/// Like [`bytes_to_document`], as an async function.
pub async fn bytes_to_document_async(
    input: BytesToDocumentInput,
) -> Result<BytesToDocumentOutput, String> {
    let bytes = input
        .bytes
        .decode_bytes()
        .map_err(|e| format!("failed to decode input bytes: {e}"))?;

    let mut warnings = Vec::new();
    let doc = PdfDocument::parse_async(&bytes, &input.options, &mut warnings)
        .await
        .map_err(|e| format!("failed to parse PDF: {e}"))?;

    Ok(BytesToDocumentOutput { doc, warnings })
}

/// Input of [`document_to_bytes`] (`Pdf_DocumentToBytes`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentToBytesInput {
    /// The document to write. Required.
    pub doc: PdfDocument,
    /// How to write the file (image compression, subsetting and so on). The keys of
    /// [`PdfSaveOptions`] are snake_case.
    #[serde(default)]
    pub options: PdfSaveOptions,
    /// Return a byte array instead of a base64 string. Default `false`.
    #[serde(default)]
    pub return_byte_array: bool,
}

/// Output of [`document_to_bytes`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentToBytesOutput {
    /// The PDF file, as a base64 string, or as a byte array if `returnByteArray` was set.
    pub bytes: Base64OrRaw,
    /// Problems that did not stop the writing.
    pub warnings: Vec<PdfWarnMsg>,
}

/// Writes a document to PDF bytes with [`PdfDocument::save`].
///
/// Never returns an error; problems are reported as warnings.
pub fn document_to_bytes(input: DocumentToBytesInput) -> Result<DocumentToBytesOutput, String> {
    let mut warnings = Vec::new();
    let return_byte_array = input.return_byte_array;
    let bytes = input.doc.save(&input.options, &mut warnings);

    Ok(DocumentToBytesOutput {
        bytes: if return_byte_array {
            Base64OrRaw::Raw(bytes)
        } else {
            Base64OrRaw::B64(base64::prelude::BASE64_STANDARD.encode(&bytes))
        },
        warnings,
    })
}

/// Like [`document_to_bytes`], as an async function.
pub async fn document_to_bytes_async(
    input: DocumentToBytesInput,
) -> Result<DocumentToBytesOutput, String> {
    let mut warnings = Vec::new();
    let return_byte_array = input.return_byte_array;
    let bytes = input.doc.save_async(&input.options, &mut warnings).await;

    Ok(DocumentToBytesOutput {
        bytes: if return_byte_array {
            Base64OrRaw::Raw(bytes)
        } else {
            Base64OrRaw::B64(base64::prelude::BASE64_STANDARD.encode(&bytes))
        },
        warnings,
    })
}

/// Input of [`resources_for_page`] (`Pdf_ResourcesForPage`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesForPageInput {
    /// The page to list the resources of. Required.
    pub page: PdfPage,
}

/// Output of [`resources_for_page`]: the ids of the document resources a page uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesForPageOutput {
    /// Images and forms the page draws.
    pub xobjects: Vec<XObjectId>,
    /// Embedded fonts the page uses (the standard fonts are not listed).
    pub fonts: Vec<FontId>,
    /// Layers the page uses.
    pub layers: Vec<LayerInternalId>,
}

/// Lists the resources a page needs, so a caller can pick them out of the document's resources.
pub fn resources_for_page(input: ResourcesForPageInput) -> Result<ResourcesForPageOutput, String> {
    Ok(ResourcesForPageOutput {
        xobjects: input.page.get_xobject_ids(),
        fonts: input.page.get_external_font_ids(),
        layers: input.page.get_layers(),
    })
}

/// Like [`resources_for_page`], as an async function.
pub async fn resources_for_page_async(
    input: ResourcesForPageInput,
) -> Result<ResourcesForPageOutput, String> {
    resources_for_page(input)
}

/// Input of [`page_to_svg`] (`Pdf_PageToSvg`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageToSvgInput {
    /// The page to render. Required.
    pub page: PdfPage,
    /// The resources (fonts, images, graphics states) the page refers to, normally the
    /// `resources` of its document. Without them the text and images are missing.
    #[serde(default)]
    pub resources: PdfResources,
    /// How to render the page.
    #[serde(default)]
    pub options: PdfToSvgOptions,
}

/// Output of [`page_to_svg`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageToSvgOutput {
    /// The SVG of the page.
    pub svg: String,
    /// Problems that did not stop the rendering.
    pub warnings: Vec<PdfWarnMsg>,
}

/// Renders a page to SVG with [`PdfPage::to_svg`](crate::PdfPage::to_svg).
///
/// Never returns an error; problems are reported as warnings.
pub fn page_to_svg(input: PageToSvgInput) -> Result<PageToSvgOutput, String> {
    let mut warnings = Vec::new();
    let svg =
        crate::render::render_to_svg(&input.page, &input.resources, &input.options, &mut warnings);
    Ok(PageToSvgOutput { svg, warnings })
}

/// Like [`page_to_svg`], as an async function.
pub async fn page_to_svg_async(input: PageToSvgInput) -> Result<PageToSvgOutput, String> {
    let mut warnings = Vec::new();
    let svg = crate::render::render_to_svg_async(
        &input.page,
        &input.resources,
        &input.options,
        &mut warnings,
    )
    .await;
    Ok(PageToSvgOutput { svg, warnings })
}

// Fonts registered with `register_fonts`, merged under the fonts of every `html_to_document`
// call, so a page does not have to send megabytes of fonts with every render.
static REGISTERED_FONTS: std::sync::Mutex<BTreeMap<String, Vec<u8>>> =
    std::sync::Mutex::new(BTreeMap::new());

// The registered fonts overlaid with the call's own fonts.
#[cfg(feature = "html")]
pub(crate) fn fonts_with_registered(
    input_fonts: &BTreeMap<String, Base64OrRaw>,
) -> BTreeMap<String, Base64OrRaw> {
    let mut merged: BTreeMap<String, Base64OrRaw> = REGISTERED_FONTS
        .lock()
        .map(|reg| {
            reg.iter()
                .map(|(k, v)| (k.clone(), Base64OrRaw::Raw(v.clone())))
                .collect()
        })
        .unwrap_or_default();
    for (k, v) in input_fonts {
        merged.insert(k.clone(), v.clone());
    }
    merged
}

/// Input of [`register_fonts`] (`Pdf_RegisterFonts`).
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
pub struct RegisterFontsInput {
    /// Fonts to register by name (`"Roboto.ttf"`), as base64 strings or byte arrays. A font with
    /// a name that is already registered replaces it.
    #[serde(default)]
    pub fonts: BTreeMap<String, Base64OrRaw>,
    /// Remove all registered fonts before adding these. Default `false`.
    #[serde(default)]
    pub replace: bool,
}

/// Output of [`register_fonts`].
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct RegisterFontsOutput {
    /// Number of fonts added by this call.
    pub registered: usize,
    /// Number of fonts in the registry after this call.
    pub total: usize,
}

/// Registers fonts for all later [`html_to_document`] calls.
///
/// The registry is global to the process. Call this once at startup with the default fonts
/// instead of sending them with every render.
///
/// # Errors
///
/// Returns an error when a font is not valid base64. The fonts before it in the map stay
/// registered.
pub fn register_fonts(input: RegisterFontsInput) -> Result<RegisterFontsOutput, String> {
    let mut reg = REGISTERED_FONTS
        .lock()
        .map_err(|_| "font registry lock poisoned".to_string())?;
    if input.replace {
        reg.clear();
    }
    for (name, b) in &input.fonts {
        let bytes = b
            .decode_bytes()
            .map_err(|e| format!("font {name:?}: {e}"))?;
        reg.insert(name.clone(), bytes);
    }
    Ok(RegisterFontsOutput {
        registered: input.fonts.len(),
        total: reg.len(),
    })
}

/// Like [`register_fonts`], as an async function.
pub async fn register_fonts_async(
    input: RegisterFontsInput,
) -> Result<RegisterFontsOutput, String> {
    register_fonts(input)
}

/// Input of [`decode_image`] (`Pdf_DecodeImage`).
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct DecodeImageInput {
    /// An image file (PNG, JPEG and the other formats enabled by the image features), as a base64
    /// string or a byte array.
    pub bytes: Base64OrRaw,
}

/// Output of [`decode_image`].
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct DecodeImageOutput {
    /// The decoded pixels. Put it into `resources.xobjects` of a document as an image XObject
    /// and draw it with a `use-xobject` op.
    pub image: crate::RawImage,
    /// Problems that did not stop the decoding.
    #[serde(default)]
    pub warnings: Vec<PdfWarnMsg>,
}

/// Decodes an image file into a [`RawImage`](crate::RawImage), which is what the document model
/// holds for images.
///
/// Requires the `images` feature; without it the function returns an error.
///
/// # Errors
///
/// Returns an error when the base64 is invalid or the image cannot be decoded.
#[cfg(feature = "images")]
pub fn decode_image(input: DecodeImageInput) -> Result<DecodeImageOutput, String> {
    let bytes = input.bytes.decode_bytes()?;
    let mut warnings = Vec::new();
    let image = crate::RawImage::decode_from_bytes(&bytes, &mut warnings)?;
    Ok(DecodeImageOutput { image, warnings })
}

/// Decodes an image file; without the `images` feature this always returns an error.
#[cfg(not(feature = "images"))]
pub fn decode_image(_input: DecodeImageInput) -> Result<DecodeImageOutput, String> {
    Err("Pdf_DecodeImage failed: no image format features enabled for printpdf crate".to_string())
}

/// Like [`decode_image`], as an async function.
pub async fn decode_image_async(input: DecodeImageInput) -> Result<DecodeImageOutput, String> {
    decode_image(input)
}
