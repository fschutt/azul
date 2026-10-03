//! The blocking work of AzPdf, each on an azul `Thread`: reading and parsing
//! a PDF, rendering pages (page -> SVG -> pixels), the pages' text for
//! search, and the sample document. Every answer comes back to the UI thread
//! as an [`Outcome`] through the thread's write-back; no callback ever parses
//! or draws a page.
//!
//! The render path is azul's: `ParsedPdf::page_to_svg` (printpdf's page
//! renderer), then `ParsedSvg::render` (azul's CPU SVG renderer) at the width
//! the view asks for, on white paper. The `--export-png` switch runs the very
//! same function ([`render_page_raw`]), so the Chrome probe measures what the
//! viewer shows.

use std::path::Path;

use azul::{
    error::ResultParsedSvgSvgParseError,
    image::{ImageRef, RawImage},
    option::OptionColorU,
    pdf::{ParsedPdf, Pdf},
    prelude::*,
    svg::{ParsedSvg, SvgFitTo, SvgParseOptions, SvgRenderOptions},
    vec::U8VecRef,
};

use crate::model::{file_title, is_pdf_bytes, PageSize};

/// Which picture of a page a render is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The page in the page view.
    Page,
    /// The page in the thumbnail rail.
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
        Doc {
            path: path.to_string(),
            title,
            pdf,
            sizes,
            outline,
            warnings,
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
    /// Render these pages (kind, page, width in px), in order.
    Render {
        generation: u64,
        pdf: ParsedPdf,
        pages: Vec<(Kind, usize, u32)>,
    },
    /// Every page's text, for search.
    Texts { generation: u64, pdf: ParsedPdf },
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
    /// One page rendered (`None`: it could not be drawn).
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
            // One answer per page, so pages appear as they are drawn.
            for (kind, page, width) in pages {
                let image = render_page(&pdf, page, width);
                send(
                    &mut sender,
                    Outcome::Rendered {
                        generation,
                        kind,
                        page,
                        width,
                        image,
                    },
                );
            }
            send(&mut sender, Outcome::RenderDone { generation });
        }
        Job::Texts { generation, pdf } => {
            let texts = page_texts(&pdf);
            send(&mut sender, Outcome::Texts { generation, texts });
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

/// Page `page` (0-based) as SVG text: azul's PDF -> SVG.
#[must_use]
pub fn page_svg(pdf: &ParsedPdf, page: usize) -> Option<String> {
    pdf.page_to_svg(page)
        .into_option()
        .map(|svg| svg.as_str().to_string())
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
