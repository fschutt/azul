# printpdf

[![CI](https://github.com/fschutt/printpdf/actions/workflows/ci.yaml/badge.svg)](https://github.com/fschutt/printpdf/actions/workflows/ci.yaml)
[![Dependencies](https://deps.rs/repo/github/fschutt/printpdf/status.svg)](https://deps.rs/repo/github/fschutt/printpdf)

`printpdf` is a Rust library for creating, reading, writing and rendering PDF documents.

[Website](https://fschutt.github.io/printpdf) | [Crates.io](https://crates.io/crates/printpdf) | [Documentation](https://docs.rs/printpdf) | [Donate](https://github.com/sponsors/fschutt)

> [!NOTE]
> HTML-to-PDF rendering (`PdfDocument::from_html`, `html` feature) works for
> basic layouts - tables, page breaks, footnotes - but is still
> evolving. For pixel-exact control, position PDF elements manually with ops.

## Features

- Pages, Bookmarks, link annotations (read / write)
- Layers (read / write)
- Graphics: lines, shapes, bezier curves, SVG content (read / write)
- Gradients: axial and radial (read / write), also from CSS `linear-gradient` and `radial-gradient` in HTML
- Images encoding / decoding (read support: experimental / write supported with `image`)
- Embedded fonts, Unicode support (read support: experimental / write)
- Minifying file size (auto-subsetting fonts)
- Advanced graphics - overprint, blending, etc.
- Advanced typography - character / word scaling and spacing, superscript, subscript, etc.
- Embedding SVGs (uses `svg2pdf` crate internally)

Experimental features:

- Rendering PDF pages to SVG (one standalone SVG file per page)
    - You can use the resulting SVG to render the PDF page into an image using `resvg`
- Extracting text from PDF pages (auto-decodes Unicode, line breaks and text positions)
- XHTML layouting for simple page layout (using `azul-layout` + `xmlparser`)
    - Good enough for basic page layouting, book rendering, reports and forms,
      with automatic page-breaking

NOT supported are:

- Patterns (tiling and shading patterns as fill or stroke colors)
- File attachments
- Open Prepress Interface
- Halftoning images
- Conformance / error checking for various PDF standards
- Embedded Javascript

See the WASM32 demo live at: https://fschutt.github.io/printpdf

## Writing PDF

### Basic example

```rust
use printpdf::*;

fn main() {
    let mut doc = PdfDocument::new("My first PDF");
    let page1_contents = vec![Op::Marker { id: "debugging-marker".to_string() }];
    let page1 = PdfPage::new(Mm(210.0), Mm(297.0), page1_contents);
    let mut warnings = Vec::new();
    let pdf_bytes: Vec<u8> = doc
        .with_pages(vec![page1])
        .save(&PdfSaveOptions::default(), &mut warnings);
}
```

### Graphics

```rust
use printpdf::*;

fn main() {
    let mut doc = PdfDocument::new("My first PDF");

    // Square shape. `bezier: true` would make the point a bezier control
    // handle for a curve. If you want holes, wind one ring counterclockwise.
    let line = Line {
        points: vec![
            LinePoint { p: Point::new(Mm(100.0), Mm(100.0)), bezier: false },
            LinePoint { p: Point::new(Mm(100.0), Mm(200.0)), bezier: false },
            LinePoint { p: Point::new(Mm(300.0), Mm(200.0)), bezier: false },
            LinePoint { p: Point::new(Mm(300.0), Mm(100.0)), bezier: false },
        ],
        is_closed: true,
    };

    // Triangle shape
    let polygon = Polygon {
        rings: vec![PolygonRing {
            points: vec![
                LinePoint { p: Point::new(Mm(150.0), Mm(150.0)), bezier: false },
                LinePoint { p: Point::new(Mm(150.0), Mm(250.0)), bezier: false },
                LinePoint { p: Point::new(Mm(350.0), Mm(250.0)), bezier: false },
            ],
        }],
        mode: PaintMode::FillStroke,
        winding_order: WindingOrder::NonZero,
    };
    
    // Graphics config
    let fill_color = Color::Cmyk(Cmyk::new(0.0, 0.23, 0.0, 0.0, None));
    let outline_color = Color::Rgb(Rgb::new(0.75, 1.0, 0.64, None));
    let dash_pattern = LineDashPattern::new(0.0, &[20.0, 10.0]); // dash, gap

    let extgstate = ExtendedGraphicsState::default()
        .with_overprint_stroke(true)
        .with_blend_mode(BlendMode::multiply());
    
    let page1_contents = vec![
        // add line1 (square)
        Op::SetOutlineColor { col: Color::Rgb(Rgb::new(0.75, 1.0, 0.64, None)) },
        Op::SetOutlineThickness { pt: Pt(10.0) },
        Op::DrawLine { line: line },
    
        // add line2 (triangle)
        Op::SaveGraphicsState,
        Op::LoadGraphicsState { gs: doc.add_graphics_state(extgstate) },
        Op::SetLineDashPattern { dash: dash_pattern },
        Op::SetLineJoinStyle { join: LineJoinStyle::Round },
        Op::SetLineCapStyle { cap: LineCapStyle::Round },
        Op::SetFillColor { col: fill_color },
        Op::SetOutlineThickness { pt: Pt(15.0) },
        Op::SetOutlineColor { col: outline_color },
        Op::DrawPolygon { polygon: polygon },
        Op::RestoreGraphicsState,
    ];
    
    let page1 = PdfPage::new(Mm(210.0), Mm(297.0), page1_contents);
    let mut warnings = Vec::new();
    let pdf_bytes: Vec<u8> = doc
        .with_pages(vec![page1])
        .save(&PdfSaveOptions::default(), &mut warnings);
}
```

### Images

- Images only get compressed in release mode. You might get huge PDFs (6 or more MB) in debug mode.
-  To make this process faster, use `BufReader` instead of directly reading from the file.
- Scaling of images is implicitly done to fit one pixel = one dot at 300 dpi.

```rust
use printpdf::*;

fn main() {
    let mut doc = PdfDocument::new("My first PDF");
    let image_bytes = include_bytes!("assets/img/BMP_test.bmp");
    let mut warnings = Vec::new();
    // requires --feature bmp
    let image = RawImage::decode_from_bytes(image_bytes, &mut warnings).unwrap();

    // In the PDF, an image is an `XObject`, identified by a unique `XObjectId`
    let image_xobject_id = doc.add_image(&image);

    let page1_contents = vec![
        Op::UseXobject { 
            id: image_xobject_id.clone(), 
            transform: XObjectTransform::default() 
        }
    ];

    let page1 = PdfPage::new(Mm(210.0), Mm(297.0), page1_contents);
    let pdf_bytes: Vec<u8> = doc
        .with_pages(vec![page1])
        .save(&PdfSaveOptions::default(), &mut warnings);
}
```

### Fonts

```rust
use printpdf::*;

fn main() {
    let mut doc = PdfDocument::new("My first PDF");

    let roboto_bytes = include_bytes!("assets/fonts/RobotoMedium.ttf");
    let font_index = 0; // face index inside a .ttc collection; 0 for a plain .ttf/.otf
    let mut font_warnings = Vec::new();
    let font = ParsedFont::from_bytes(roboto_bytes, font_index, &mut font_warnings).unwrap();

    // printpdf automatically keeps track of which fonts are used in the PDF
    let font_id = doc.add_font(&font);

    let text_pos = Point {
        x: Mm(10.0).into(),
        y: Mm(100.0).into(),
    }; // from bottom left

    let mut warnings = Vec::new();

    // Text-showing ops must sit between StartTextSection and EndTextSection -
    // most viewers drop text outside of one (save() warns if you forget).
    let page1_contents = vec![
        Op::StartTextSection,
        Op::SetTextCursor { pos: text_pos },
        Op::SetLineHeight { lh: Pt(33.0) },
        Op::SetFont {
            font: PdfFontHandle::External(font_id.clone()),
            size: Pt(16.0),
        },
        Op::ShowText {
            items: vec![TextItem::Text("Lorem ipsum".to_string())],
        },
        Op::AddLineBreak,
        Op::ShowText {
            items: vec![TextItem::Text("dolor sit amet".to_string())],
        },
        Op::EndTextSection,
    ];

    let save_options = PdfSaveOptions {
        subset_fonts: true, // auto-subset fonts on save
        ..Default::default()
    };

    let page1 = PdfPage::new(Mm(210.0), Mm(297.0), page1_contents);
    let pdf_bytes: Vec<u8> = doc
        .with_pages(vec![page1])
        .save(&save_options, &mut warnings);
}
```

### Tables, HTML

For creating tables, etc. printpdf uses a basic layout system, similar to wkhtmltopdf 
(although more limited in terms of features). It's good enough for basic page layouting, 
book rendering and reports / forms / etc. Includes automatic page-breaking.

Since printpdf supports WASM, there is an interactive demo at 
https://fschutt.github.io/printpdf - try playing with the HTML.

See [docs/HTML.md](docs/HTML.md) for the supported HTML/CSS.

```rust
// needs --features="html"
use printpdf::*;

fn main() {

    // See https://fschutt.github.io/printpdf for an interactive WASM demo!

    let html = r#"
    <html>
        <!-- printpdf automatically breaks content into pages -->
        <body style="padding:10mm">
            <p style="color: red; font-family: sans-serif;">Hello!</p>
            <div style="width:200px;height:200px;background:red;">
                <p>World!</p>
            </div>
        </body>
    </html>
    "#;

    // named images to be used in the HTML, i.e. "image1.png" => Base64OrRaw::Raw(bytes)
    let images = BTreeMap::new();
    // named fonts to be used in the HTML, i.e. "Roboto" => Base64OrRaw::Raw(roboto_bytes)
    let fonts = BTreeMap::new();

    // page size, margins, etc. - all optional; printpdf auto-page-breaks
    let options = GeneratePdfOptions {
        page_width: Some(210.0),  // mm
        page_height: Some(297.0), // mm
        ..Default::default()
    };

    let mut warnings = Vec::new();
    let pdf_bytes = PdfDocument::from_html(html, &images, &fonts, &options, &mut warnings)
        .unwrap()
        .save(&PdfSaveOptions::default(), &mut warnings);
}
```

## Command line

[`ppdf`](ppdf) is a command-line tool built on printpdf. It converts a PDF to JSON
and back, so a PDF can be edited with `jq`, and it extracts text, text boxes, SVG
pages, fonts and images. It also renders HTML to PDF.

```sh
cargo install ppdf
ppdf json in.pdf | jq '.pages |= [.[1], .[0]] + .[2:]' | ppdf pdf - -o swapped.pdf
ppdf text in.pdf
```

## Roadmap

The goal of printpdf is to be a general-use PDF library, such as
libharu or similar. PDFs generated by printpdf should always adhere
to a PDF standard, except if you turn it off. Currently, only the
standard `PDF/X-3:2002` is covered (i.e. valid PDF according to Adobe
Acrobat). Over time, there will be more standards supported.

The printpdf wiki is live at: https://github.com/fschutt/printpdf/wiki

Here are some resources I found while working on this library:

- [`PDFXPlorer`, shows the DOM tree of a PDF, needs .NET 2.0](http://www.o2sol.com/pdfxplorer/download.htm)
- [Official PDF 1.7 reference](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/pdfreference1.7old.pdf)
- [\[GERMAN\] How to embed unicode fonts in PDF](http://www.p2501.ch/pdf-howto/typographie/vollzugriff/direkt)
- [PDF X/1-a Validator](https://www.pdf-online.com/osa/validate.aspx)
- [PDF X/3 technical notes](http://www.pdfxreport.com/lib/exe/fetch.php?media=en:technote_pdfx_checks.pdf)

## License / Support

Library is licensed MIT.

You can donate (one-time or recurrent) at https://github.com/sponsors/fschutt. Thanks!
