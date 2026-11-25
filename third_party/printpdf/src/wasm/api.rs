#![allow(non_snake_case)]

//! The `Pdf_*` functions: each takes one JSON string and returns one JSON string.
//!
//! On wasm targets they are exported to JS with `wasm-bindgen`; on other targets they are
//! ordinary functions, which is handy for tests. The `Pdf_*Sync` functions run on every target.
//! The functions without the suffix are `async`, exist only on wasm with the `js-sys` feature
//! and take the same input.
//!
//! # The envelope
//!
//! Every function returns JSON of the form `{"status": n, "data": ...}`:
//!
//! | `status` | `data` |
//! | --- | --- |
//! | 0 | the output of the function |
//! | 1 | an error string: the input is not valid JSON of the expected shape |
//! | 2 | an error string: the function failed, for example the PDF could not be parsed |
//! | 3 | an error string: the output could not be serialized to JSON |
//!
//! Byte inputs (`bytes`, `images`, `fonts`) take a base64 string, with or without a
//! `data:...;base64,` prefix, or an array of numbers. Keys are camelCase (`returnByteArray`),
//! except inside `options` of [`Pdf_HtmlToDocumentSync`] and the save options of
//! [`Pdf_DocumentToBytesSync`], which are snake_case (`page_width`).
//!
//! ```js
//! const result = JSON.parse(Pdf_HtmlToDocumentSync(JSON.stringify({ html: "<h1>Hi</h1>" })));
//! if (result.status !== 0) throw new Error(result.data);
//! const pdf = JSON.parse(Pdf_DocumentToBytesSync(JSON.stringify({ doc: result.data.doc })));
//! const base64Pdf = pdf.data.bytes;
//! ```

#[cfg(all(target_family = "wasm", feature = "js-sys"))]
use std::{future::Future, pin::Pin};

use serde_derive::{Deserialize, Serialize};

/// The JSON envelope every function returns, see the [module docs](self).
#[derive(Serialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub struct PdfApiReturn<T: serde::Serialize> {
    /// 0 on success, otherwise 1, 2 or 3 (see the module docs).
    pub status: usize,
    /// The output of the function if `status` is 0, otherwise the error string.
    pub data: StatusOrData<T>,
}

/// The `data` of a [`PdfApiReturn`]. It is untagged: JSON holds the output or the error string
/// as it is.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(untagged)]
pub enum StatusOrData<T: serde::Serialize> {
    /// The output of the function.
    Ok(T),
    /// The error message.
    Error(String),
}

/// Renders HTML to a document.
///
/// Input: `{ html, images?, fonts?, options? }`, see
/// [`HtmlToDocumentInput`](crate::wasm::structs::HtmlToDocumentInput). Output: `{ doc, warnings }`.
/// The fonts of [`Pdf_RegisterFontsSync`] are available to the HTML too.
///
/// ```js
/// const html = "<!doctype html><html><body><h1>Hello!</h1></body></html>";
/// const result = JSON.parse(Pdf_HtmlToDocumentSync(JSON.stringify({ html })));
/// // { status: 0, data: { doc: { metadata, resources, bookmarks, pages: [...] },
/// //                      warnings: [{ page, opId, severity, msg }] } }
/// ```
#[allow(non_snake_case)]
#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub fn Pdf_HtmlToDocumentSync(input: String) -> String {
    api_inner(&input, crate::wasm::structs::html_to_document)
}

/// Parses a PDF file into a document.
///
/// Input: `{ bytes, options? }`, see
/// [`BytesToDocumentInput`](crate::wasm::structs::BytesToDocumentInput). Output: `{ doc, warnings }`,
/// the same shape as [`Pdf_HtmlToDocumentSync`].
///
/// ```js
/// const input = JSON.stringify({ bytes: btoa(pdfBinaryString) });
/// const result = JSON.parse(Pdf_BytesToDocumentSync(input));
/// console.log(result.data.doc.pages.length);
/// ```
#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub fn Pdf_BytesToDocumentSync(input: String) -> String {
    api_inner(&input, crate::wasm::structs::bytes_to_document)
}

/// Lists the resources a page uses.
///
/// Input: `{ page }`. Output: `{ xobjects, fonts, layers }`, the ids to take from the
/// document's resources to render the page on its own.
#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub fn Pdf_ResourcesForPageSync(input: String) -> String {
    api_inner(&input, crate::wasm::structs::resources_for_page)
}

/// Renders a page to SVG.
///
/// Input: `{ page, resources?, options? }`, see
/// [`PageToSvgInput`](crate::wasm::structs::PageToSvgInput); pass the `resources` of the document so
/// that fonts and images are found. Output: `{ svg, warnings }`.
#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub fn Pdf_PageToSvgSync(input: String) -> String {
    api_inner(&input, crate::wasm::structs::page_to_svg)
}

/// Writes a document to a PDF file.
///
/// Input: `{ doc, options?, returnByteArray? }`, see
/// [`DocumentToBytesInput`](crate::wasm::structs::DocumentToBytesInput). Output: `{ bytes, warnings }`
/// where `bytes` is a base64 string, or an array of numbers with `returnByteArray`.
#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub fn Pdf_DocumentToBytesSync(input: String) -> String {
    api_inner(&input, crate::wasm::structs::document_to_bytes)
}

fn api_inner<'a, T, Q>(input: &'a str, f: fn(Q) -> Result<T, String>) -> String
where
    T: serde::Serialize,
    Q: serde::Deserialize<'a>,
{
    serde_json::to_string(&match serde_json::from_str::<Q>(input) {
        Ok(input) => match (f)(input) {
            Ok(o) => PdfApiReturn {
                status: 0,
                data: StatusOrData::Ok(o),
            },
            Err(e) => PdfApiReturn {
                status: 2,
                data: StatusOrData::Error(e),
            },
        },
        Err(e) => PdfApiReturn {
            status: 1,
            data: StatusOrData::Error(format!("failed to deserialize input: {e}")),
        },
    })
    .unwrap_or_else(|e| output_serialization_error_envelope(&e))
}

// The status 3 envelope, for a response that cannot be serialized (for example a font whose
// `Serialize` fails). It is always valid JSON, so callers that parse the result do not fail with
// an unrelated "Unexpected end of JSON input".
fn output_serialization_error_envelope(e: &serde_json::Error) -> String {
    let msg = serde_json::to_string(&format!("failed to serialize output: {e}"))
        .unwrap_or_else(|_| "\"failed to serialize output\"".to_string());
    format!("{{\"status\":3,\"data\":{msg}}}")
}

/// Async version of [`Pdf_HtmlToDocumentSync`].
#[cfg(all(target_family = "wasm", feature = "js-sys"))]
#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub async fn Pdf_HtmlToDocument(input: String) -> String {
    api_inner_async(&input, |x| {
        Box::pin(crate::wasm::structs::html_to_document_async(x))
    })
    .await
}

/// Async version of [`Pdf_BytesToDocumentSync`].
#[cfg(all(target_family = "wasm", feature = "js-sys"))]
#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub async fn Pdf_BytesToDocument(input: String) -> String {
    api_inner_async(&input, |x| {
        Box::pin(crate::wasm::structs::bytes_to_document_async(x))
    })
    .await
}

/// Async version of [`Pdf_DocumentToBytesSync`].
#[cfg(all(target_family = "wasm", feature = "js-sys"))]
#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub async fn Pdf_DocumentToBytes(input: String) -> String {
    api_inner_async(&input, |x| {
        Box::pin(crate::wasm::structs::document_to_bytes_async(x))
    })
    .await
}

/// Async version of [`Pdf_ResourcesForPageSync`].
#[cfg(all(target_family = "wasm", feature = "js-sys"))]
#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub async fn Pdf_ResourcesForPage(input: String) -> String {
    api_inner_async(&input, |x| {
        Box::pin(crate::wasm::structs::resources_for_page_async(x))
    })
    .await
}

/// Async version of [`Pdf_PageToSvgSync`].
#[cfg(all(target_family = "wasm", feature = "js-sys"))]
#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub async fn Pdf_PageToSvg(input: String) -> String {
    api_inner_async(&input, |x| {
        Box::pin(crate::wasm::structs::page_to_svg_async(x))
    })
    .await
}

#[cfg(all(target_family = "wasm", feature = "js-sys"))]
async fn api_inner_async<T, Q>(
    input: &str,
    f: fn(Q) -> Pin<Box<dyn Future<Output = Result<T, String>>>>,
) -> String
where
    T: serde::Serialize,
    Q: for<'de> serde::Deserialize<'de>,
{
    match serde_json::from_str::<Q>(input) {
        Ok(input_obj) => match f(input_obj).await {
            Ok(data) => serde_json::to_string(&PdfApiReturn {
                status: 0,
                data: StatusOrData::Ok(data),
            })
            .unwrap_or_else(|e| output_serialization_error_envelope(&e)),
            Err(e) => serde_json::to_string(&PdfApiReturn {
                status: 2,
                data: StatusOrData::<T>::Error(e),
            })
            .unwrap_or_else(|e| output_serialization_error_envelope(&e)),
        },
        Err(e) => serde_json::to_string(&PdfApiReturn {
            status: 1,
            data: StatusOrData::<T>::Error(format!("failed to deserialize input: {}", e)),
        })
        .unwrap_or_else(|e| output_serialization_error_envelope(&e)),
    }
}

/// Registers fonts for all later [`Pdf_HtmlToDocumentSync`] calls.
///
/// Input: `{ fonts, replace? }`, see
/// [`RegisterFontsInput`](crate::wasm::structs::RegisterFontsInput). Output: `{ registered, total }`.
/// The fonts are merged under the `fonts` of each render, which win on a name clash. Call this
/// once at startup instead of sending the fonts with every render.
///
/// ```js
/// const r = JSON.parse(Pdf_RegisterFontsSync(JSON.stringify({
///   fonts: { "Helvetica.ttf": HELVETICA_B64 },
/// })));
/// // { status: 0, data: { registered: 1, total: 1 } }
/// ```
#[allow(non_snake_case)]
#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub fn Pdf_RegisterFontsSync(input: String) -> String {
    api_inner(&input, crate::wasm::structs::register_fonts)
}

/// Decodes an image file (PNG, JPEG and so on) into the image JSON that `doc.resources.xobjects`
/// holds, so JS can add an image to a parsed document and draw it with a `use-xobject` op.
///
/// Input: `{ bytes }`. Output: `{ image, warnings }`.
///
/// ```js
/// const r = JSON.parse(Pdf_DecodeImageSync(JSON.stringify({ bytes: pngB64 })));
/// // { status: 0, data: { image: { width, height, ... }, warnings: [] } }
/// ```
#[allow(non_snake_case)]
#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub fn Pdf_DecodeImageSync(input: String) -> String {
    api_inner(&input, crate::wasm::structs::decode_image)
}

/// Async version of [`Pdf_RegisterFontsSync`].
#[cfg(all(target_family = "wasm", feature = "js-sys"))]
#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub async fn Pdf_RegisterFonts(input: String) -> String {
    api_inner_async(&input, |x| {
        Box::pin(crate::wasm::structs::register_fonts_async(x))
    })
    .await
}

/// Async version of [`Pdf_DecodeImageSync`].
#[cfg(all(target_family = "wasm", feature = "js-sys"))]
#[cfg_attr(target_family = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub async fn Pdf_DecodeImage(input: String) -> String {
    api_inner_async(&input, |x| {
        Box::pin(crate::wasm::structs::decode_image_async(x))
    })
    .await
}

/// Runs when the wasm module loads and forwards Rust panic messages to the browser console.
/// Without it a panic shows up as an opaque `RuntimeError: unreachable`.
#[cfg(all(target_family = "wasm", feature = "js-sys"))]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn printpdf_wasm_init() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("printpdf wasm panic: {info}").into());
    }));
}
