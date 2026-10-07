//! The blocking work of AzPdf, each on an azul `Thread`: reading and parsing
//! a PDF, making the pages (page -> SVG -> DOM) and the thumbnails (page ->
//! SVG -> pixels), the pages' text for search, and the sample document. Every
//! answer comes back to the UI thread as an [`Outcome`] through the thread's
//! write-back; no callback ever parses a page.
//!
//! A page in the view is a DOM ([`page_dom`]): `ParsedPdf::page_to_svg`
//! (printpdf's page renderer), read as markup into an `<svg>` subtree that
//! azul lays out like any other - its shapes drawn with clip shapes, its text
//! real text (selectable) in the fonts the page embeds, at any zoom. A
//! thumbnail is a picture: the same SVG through `ParsedSvg::render` (azul's
//! CPU SVG renderer) at the rail's width, on white paper - the function the
//! `--export-png` switch runs ([`render_page_raw`]).

use std::path::Path;

use azul::{
    error::{ResultParsedSvgSvgParseError, ResultU8VecString, ResultXmlXmlError},
    image::{ImageRef, RawImage},
    option::OptionColorU,
    pdf::{ParsedPdf, Pdf, PdfFieldValue, PdfFormFieldKind, PdfStamp},
    prelude::*,
    svg::{ParsedSvg, SvgFitTo, SvgParseOptions, SvgRenderOptions},
    vec::{PdfFieldValueVec, PdfStampVec, U8VecRef},
    xml::Xml,
};

use crate::model::{file_title, is_pdf_bytes, Field, FieldKind, FieldWidget, PageSize};

/// What a render of a page makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The page in the page view: its DOM (any size; no width).
    Page,
    /// The page in the thumbnail rail: a picture `width` px wide.
    Thumb,
}

/// A parsed document, as the UI keeps it.
#[derive(Clone)]
pub struct Doc {
    pub path: String,
    pub title: String,
    /// azul's parse, shared (a clone is a reference count) with the render
    /// threads.
    pub pdf: ParsedPdf,
    pub sizes: Vec<PageSize>,
    /// The outline: title, 0-based page.
    pub outline: Vec<(String, usize)>,
    /// What the parser skipped or guessed.
    pub warnings: usize,
    /// The fields of the PDF's form (none for most PDFs).
    pub fields: Vec<Field>,
}

impl Doc {
    /// The document `pdf` read from `path`.
    #[must_use]
    pub fn from_pdf(path: &str, pdf: ParsedPdf) -> Doc {
        let sizes = (0..pdf.page_count())
            .map(|i| {
                let size = pdf.page_size(i);
                PageSize {
                    width_pt: size.width_pt,
                    height_pt: size.height_pt,
                }
            })
            .collect();
        let outline = (0..pdf.outline_count())
            .map(|i| {
                (
                    pdf.outline_title(i).as_str().to_string(),
                    pdf.outline_page(i),
                )
            })
            .collect();
        let title = pdf.get_title().as_str().trim().to_string();
        let title = if title.is_empty() {
            file_title(path)
        } else {
            title
        };
        let warnings = pdf.get_warnings().as_slice().len();
        let fields = form_fields(&pdf);
        Doc {
            path: path.to_string(),
            title,
            pdf,
            sizes,
            outline,
            warnings,
            fields,
        }
    }

    /// The number of pages.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.sizes.len()
    }
}

/// One piece of blocking work.
pub enum Job {
    /// Read the PDF at `path` and parse it.
    Open { generation: u64, path: String },
    /// Make these pages (kind, page, width in px - 0 for a page DOM), in order.
    Render {
        generation: u64,
        pdf: ParsedPdf,
        pages: Vec<(Kind, usize, u32)>,
    },
    /// Every page's text, for search.
    Texts { generation: u64, pdf: ParsedPdf },
    /// The PDF with its form filled from `values`, `stamps` drawn on, and
    /// flattened: what "Export filled PDF" saves as `name`.
    Fill {
        generation: u64,
        pdf: ParsedPdf,
        values: Vec<(String, String)>,
        stamps: Vec<PdfStamp>,
        name: String,
    },
    /// Build the sample document's bytes.
    Sample,
}

/// What a job answers.
pub enum Outcome {
    Opened {
        generation: u64,
        path: String,
        result: Result<Doc, String>,
    },
    /// A page's DOM made (`None`: the page has none).
    PageDom {
        generation: u64,
        page: usize,
        dom: Option<Dom>,
    },
    /// A thumbnail drawn (`None`: it could not be drawn).
    Rendered {
        generation: u64,
        kind: Kind,
        page: usize,
        width: u32,
        image: Option<ImageRef>,
    },
    /// A render thread finished (its slot is free again).
    RenderDone {
        generation: u64,
    },
    Texts {
        generation: u64,
        texts: Vec<String>,
    },
    Sample {
        result: Result<Vec<u8>, String>,
    },
    /// The filled PDF's bytes, to save as `name`.
    Filled {
        generation: u64,
        name: String,
        result: Result<Vec<u8>, String>,
    },
}

/// A thread's start data: the job, taken out once.
pub struct JobInit {
    pub job: Option<Job>,
}

/// A thread's answer, taken out once by the write-back.
pub struct Done {
    pub outcome: Option<Outcome>,
}

/// Sends `outcome` to the UI thread.
fn send(sender: &mut ThreadSender, outcome: Outcome) {
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        crate::on_job_done,
        RefAny::new(Done {
            outcome: Some(outcome),
        }),
    )));
}

/// Runs on a worker thread: the job, then its answer(s) to the UI thread.
pub extern "C" fn job_thread(
    mut init: RefAny,
    mut sender: ThreadSender,
    _receiver: ThreadReceiver,
) {
    let Some(job) = init
        .downcast_mut::<JobInit>()
        .and_then(|mut init| init.job.take())
    else {
        return;
    };
    match job {
        Job::Open { generation, path } => {
            let result = open(&path);
            send(
                &mut sender,
                Outcome::Opened {
                    generation,
                    path,
                    result,
                },
            );
        }
        Job::Render {
            generation,
            pdf,
            pages,
        } => {
            // One answer per page, so pages appear as they are made.
            for (kind, page, width) in pages {
                let outcome = match kind {
                    Kind::Page => Outcome::PageDom {
                        generation,
                        page,
                        dom: page_dom(&pdf, page),
                    },
                    Kind::Thumb => Outcome::Rendered {
                        generation,
                        kind,
                        page,
                        width,
                        image: render_page(&pdf, page, width),
                    },
                };
                send(&mut sender, outcome);
            }
            send(&mut sender, Outcome::RenderDone { generation });
        }
        Job::Texts { generation, pdf } => {
            let texts = page_texts(&pdf);
            send(&mut sender, Outcome::Texts { generation, texts });
        }
        Job::Fill {
            generation,
            pdf,
            values,
            stamps,
            name,
        } => {
            let result = fill_pdf(&pdf, &values, stamps, true);
            send(
                &mut sender,
                Outcome::Filled {
                    generation,
                    name,
                    result,
                },
            );
        }
        Job::Sample => {
            send(
                &mut sender,
                Outcome::Sample {
                    result: sample_pdf(),
                },
            );
        }
    }
}

// ==== The work itself (also called by the --export-* switches) ====

/// Reads and parses the PDF at `path`: the document, or why not, as a
/// sentence.
pub fn open(path: &str) -> Result<Doc, String> {
    let bytes = azul_appkit::files::read_outside(Path::new(path))?;
    if !is_pdf_bytes(&bytes) {
        return Err(format!("{} is not a PDF.", file_title(path)));
    }
    let pdf = ParsedPdf::create_from_bytes(U8VecRef::from(&bytes[..]));
    if !pdf.is_valid() {
        return Err(format!(
            "{} could not be read: {}",
            file_title(path),
            pdf.get_error().as_str()
        ));
    }
    Ok(Doc::from_pdf(path, pdf))
}

/// The fields of `pdf`'s form, as the model keeps them.
#[must_use]
pub fn form_fields(pdf: &ParsedPdf) -> Vec<Field> {
    pdf.form_fields()
        .as_slice()
        .iter()
        .map(|f| Field {
            name: f.name.as_str().to_string(),
            kind: match f.kind {
                PdfFormFieldKind::Text => FieldKind::Text,
                PdfFormFieldKind::CheckBox => FieldKind::CheckBox,
                PdfFormFieldKind::RadioButton => FieldKind::Radio,
                PdfFormFieldKind::ComboBox | PdfFormFieldKind::ListBox => FieldKind::Choice,
                PdfFormFieldKind::PushButton | PdfFormFieldKind::Signature => FieldKind::Other,
            },
            value: f.value.as_str().to_string(),
            options: f
                .options
                .as_slice()
                .iter()
                .map(|o| o.as_str().to_string())
                .collect(),
            widgets: f
                .widgets
                .as_slice()
                .iter()
                .filter(|w| !w.hidden)
                .map(|w| FieldWidget {
                    page: w.page,
                    rect: (w.rect.x, w.rect.y, w.rect.width, w.rect.height),
                    on_state: w.on_state.as_str().to_string(),
                })
                .collect(),
            read_only: f.read_only,
            multiline: f.multiline,
            password: f.password,
            max_len: f.max_length,
            font_size: f.font_size_pt,
        })
        .collect()
}

/// `pdf` with its form filled from `values` (field name, value), `stamps`
/// drawn on, flattened or not: azul's `ParsedPdf::fill_form` (printpdf on
/// the PDF's own bytes).
pub fn fill_pdf(
    pdf: &ParsedPdf,
    values: &[(String, String)],
    stamps: Vec<PdfStamp>,
    flatten: bool,
) -> Result<Vec<u8>, String> {
    let values: Vec<PdfFieldValue> = values
        .iter()
        .map(|(name, value)| PdfFieldValue::create(name.as_str(), value.as_str()))
        .collect();
    match pdf.fill_form(
        PdfFieldValueVec::from_vec(values),
        PdfStampVec::from_vec(stamps),
        flatten,
    ) {
        ResultU8VecString::Ok(bytes) => Ok(bytes.as_ref().to_vec()),
        ResultU8VecString::Err(why) => Err(why.as_str().to_string()),
    }
}

/// Page `page` (0-based) as SVG text: azul's PDF -> SVG.
#[must_use]
pub fn page_svg(pdf: &ParsedPdf, page: usize) -> Option<String> {
    pdf.page_to_svg(page)
        .into_option()
        .map(|svg| svg.as_str().to_string())
}

/// Page `page` as a DOM: its SVG (azul's PDF -> SVG) read as markup into an
/// `<svg>` subtree (`Dom::create_from_parsed_xml_fragment`). Its shapes are
/// drawn with clip shapes; its text is text in the page's own fonts - the SVG
/// embeds them, azul loads them with the DOM and gives them back when no page
/// shows them any more. Sized by its node's CSS: crisp at any zoom. `None` if
/// the page does not exist.
#[must_use]
pub fn page_dom(pdf: &ParsedPdf, page: usize) -> Option<Dom> {
    let svg = pdf.page_to_svg(page).into_option()?;
    // XML as written; markup that is not well-formed through the lenient
    // (HTML) reader, which builds the same tree.
    let xml = match Xml::from_str(svg.clone()) {
        ResultXmlXmlError::Ok(xml) => xml,
        ResultXmlXmlError::Err(_) => Xml::create_from_html(svg),
    };
    Some(Dom::create_from_parsed_xml_fragment(xml))
}

/// Page `page` drawn `width` px wide on white paper: the page's SVG through
/// azul's SVG renderer. `None` if the page does not exist or cannot be drawn.
#[must_use]
pub fn render_page_raw(pdf: &ParsedPdf, page: usize, width: u32) -> Option<RawImage> {
    let svg = pdf.page_to_svg(page).into_option()?;
    let parsed = match ParsedSvg::from_string(svg, SvgParseOptions::create_default()) {
        ResultParsedSvgSvgParseError::Ok(parsed) => parsed,
        ResultParsedSvgSvgParseError::Err(_) => return None,
    };
    let mut options = SvgRenderOptions::create_default();
    options.fit = SvgFitTo::Width(width);
    options.background_color = OptionColorU::Some(ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    });
    parsed.render(options).into_option()
}

/// [`render_page_raw`] as an image the DOM shows.
#[must_use]
pub fn render_page(pdf: &ParsedPdf, page: usize, width: u32) -> Option<ImageRef> {
    ImageRef::create_rawimage(render_page_raw(pdf, page, width)?).into_option()
}

/// Every page's text, one string per page (its text blocks, a line each).
#[must_use]
pub fn page_texts(pdf: &ParsedPdf) -> Vec<String> {
    (0..pdf.page_count())
        .map(|page| {
            pdf.page_text(page)
                .as_slice()
                .iter()
                .map(|block| block.as_str().to_string())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

/// A4 at 96 dpi, the sample document's page size in CSS px.
const SAMPLE_PAGE_PX: (f32, f32) = (794.0, 1123.0);

/// The sample document (`--sample`): a few pages of headings, text and
/// coloured boxes, made by azul's own PDF writer.
pub fn sample_pdf() -> Result<Vec<u8>, String> {
    let mut body = Dom::create_body().with_css("padding: 48px; font-family: sans-serif;");
    body.add_child(Dom::create_h1_with_text("AzPdf sample"));
    body.add_child(Dom::create_p_with_text(
        "This document was written by azul's PDF writer (Pdf::from_dom). AzPdf reads it \
         back with ParsedPdf: every page becomes SVG, and azul's SVG renderer draws it.",
    ));
    let swatches = [
        ("#d93025", "Red"),
        ("#1e8e3e", "Green"),
        ("#1a73e8", "Blue"),
        ("#f9ab00", "Amber"),
    ];
    let mut row = Dom::create_div().with_css("display: flex; flex-direction: row; gap: 12px;");
    for (colour, name) in swatches {
        row.add_child(
            Dom::create_div()
                .with_css(
                    format!(
                        "width: 120px; height: 80px; background: {colour}; color: #ffffff; \
                         padding: 8px;"
                    )
                    .as_str(),
                )
                .with_child(Dom::create_span_with_text(name)),
        );
    }
    body.add_child(row);
    for chapter in 1..=6 {
        body.add_child(Dom::create_h2_with_text(
            format!("Chapter {chapter}").as_str(),
        ));
        for paragraph in 1..=5 {
            body.add_child(Dom::create_p_with_text(
                format!(
                    "Paragraph {paragraph} of chapter {chapter}. Search for the word \
                     needle to find it on every page; zoom in and out, jump to a page, \
                     and look at the thumbnails on the left."
                )
                .as_str(),
            ));
        }
    }
    let bytes = Pdf::create()
        .from_dom(body, SAMPLE_PAGE_PX.0, SAMPLE_PAGE_PX.1)
        .as_ref()
        .to_vec();
    if bytes.is_empty() {
        Err("azul's PDF writer made no bytes (a build without the `pdf` feature?)".to_string())
    } else {
        Ok(bytes)
    }
}
