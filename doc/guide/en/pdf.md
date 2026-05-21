---
slug: pdf
title: Generating PDFs (HTML-to-PDF)
language: en
canonical_slug: pdf
audience: external
maturity: beta
guide_order: 255
topic_only: false
short_desc: Render a styled Dom to PDF bytes - headless, no window, no file I/O
prerequisites: [hello-world, styling]
tracked_files:
  - dll/src/desktop/extra/pdf/mod.rs
  - examples/azul-doc/src/main.rs
last_generated_rev: bf54ec9cbc7c1d3b7f6a2e6c0a3c5e8a9d0b1c2d
generated_at: 2026-05-21T00:00:00Z
default-search-keys:
  - Pdf
  - from_dom
  - dom_to_pdf
  - U8Vec
  - write_json
  - read_json
---

# Generating PDFs (HTML-to-PDF)

## Introduction

Azul can render a styled `Dom` to a PDF the same way it lays out a window -
headless, through the layout engine - and hand you the PDF as bytes. This is
the "HTML-to-PDF" use case: you build your document as a `Dom` with CSS
(headings, tables, images, page breaks), call one function, and get a
`U8Vec`. It is completely **decoupled from the window** - no `App`, no event
loop - and does **no file I/O**: you receive the bytes and save / upload /
stream them yourself. (This mirrors the printpdf-WASM API: `dom -> PDF pages`.)

## The API

`Pdf` is a tiny stateless handle (`azul::misc::Pdf`):

```rust
use azul::misc::Pdf;

// `doc` is any styled Dom. Page size is in pixels at 96 DPI:
//   A4     = 794 x 1123    Letter = 816 x 1056
let bytes = Pdf::new().from_dom(doc, 794.0, 1123.0);   // -> U8Vec

// You own the bytes - persist them however you like:
std::fs::write("out.pdf", bytes.as_slice()).unwrap();
```

The free function `dom_to_pdf(dom, page_width_px, page_height_px) -> U8Vec` does
the same thing without constructing a handle.

Content taller than one page is **paginated automatically**: the headless
layout splits the document across as many pages as it needs, each
`page_width_px` x `page_height_px`.

## A complete example

`examples/azul-doc` is the P5 goal app. Its export callback builds the document
`Dom` and exports it - no window involved in the conversion:

```rust
fn doc_page() -> Dom {
    Dom::create_body()
        .with_child(Dom::create_text("Invoice #42").with_css("font-size: 32px;"))
        .with_child(/* ... tables, paragraphs, images ... */)
}

extern "C" fn on_export(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(s) = data.downcast_ref::<DocState>() {
        let bytes = Pdf::new().from_dom(doc_page(), 794.0, 1123.0); // A4 @ 96 DPI
        let _ = std::fs::write(&s.export_path, bytes.as_slice());
    }
    Update::DoNothing
}
```

Because the conversion is headless, you can also generate PDFs from a server, a
CLI, or a background `Thread` - anywhere you can build a `Dom`.

## The JSON model

For programmatic / advanced PDF construction you can go through the printpdf
JSON representation: `pdf.write_json(json) -> U8Vec` serializes a document
model to PDF bytes, and `pdf.read_json(bytes) -> Json` parses an existing PDF
back into the model. `from_dom` is the high-level path; `write_json` is the
escape hatch when you need to place content the layout engine wouldn't.

## Feature gating

The `Pdf` handle is always present (it codegen-exposes with no feature gating),
but the `printpdf` engine behind it is opt-in via the `pdf` Cargo feature.
Without the feature, `from_dom` / `write_json` return empty bytes - so a build
that doesn't need PDF export doesn't pay for the dependency.

## See also

- [styling](styling.md) - the CSS your document `Dom` uses.
- [headless-rendering](headless-rendering.md) - the same layout engine, no window.
- [hello-world](hello-world.md) - building a `Dom`.
