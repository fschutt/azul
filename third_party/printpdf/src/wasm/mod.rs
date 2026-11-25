//! A JSON API for calling printpdf from JavaScript.
//!
//! JS code cannot share Rust structs, so every function takes one JSON string and returns one
//! JSON string. [`api`] holds those functions (the `Pdf_*` family, exported with `wasm-bindgen`
//! on wasm targets); [`structs`] holds the input and output types and the Rust function each
//! one calls, which work on every target without any JSON.
//!
//! The functions cover the common round trips: HTML to a document, PDF bytes to a document, a
//! document to PDF bytes, a page to SVG, the resources of a page, registering fonts once, and
//! decoding an image. Documents travel as the JSON form of [`crate::PdfDocument`].
//!
//! # Example
//!
//! HTML to a PDF download, with the two functions most pages need. The `Pdf_*` functions are
//! imported from the package that `wasm-pack build --target web` writes. A complete page is
//! [`web/minimal.html`](https://github.com/fschutt/printpdf/blob/master/web/minimal.html).
//!
//! ```js
//! import init, { Pdf_HtmlToDocument, Pdf_DocumentToBytes } from './pkg/printpdf.js';
//!
//! // { status, data }: status 0 is success, anything else is an error and `data` the message.
//! const call = async (fn, input) => {
//!   const result = JSON.parse(await fn(JSON.stringify(input)));
//!   if (result.status !== 0) throw new Error(result.data);
//!   return result.data;
//! };
//!
//! await init();
//! const { doc } = await call(Pdf_HtmlToDocument, { html: '<h1>Hello</h1>' });
//! const { bytes } = await call(Pdf_DocumentToBytes, { doc }); // base64
//! ```

pub mod api;
pub mod structs;
