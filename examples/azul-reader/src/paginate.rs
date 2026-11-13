//! The pages of a chapter: the engine's pagination of the reading column.
//!
//! The chapter's DOM ([`crate::content::Chapter::xml`] through `Dom::create_from_parsed_xml`)
//! sits in the READING COLUMN - a box exactly the page's text width with the reader's type
//! (font, size, line height, alignment) - and the column is laid out once by azul's paged
//! layout at the page's text height. Its breaks are the page starts ([`PageMap`]). The SAME
//! column is what the reader shows, clipped per page: the pagination and the screen lay out
//! one box the same way.
//!
//! A reader's pages end between two lines, never inside one: the pagination runs the
//! READING POLICY ([`reading_policy`]: whole lines, widows and orphans, pictures and table rows
//! kept whole), not the default plain slicing.

use std::sync::Arc;

use azul::{
    callbacks::{CallbackInfo, RefAny, WriteBackCallbackType},
    dom::{Dom, StyledDom},
    error::ResultRawImageDecodeImageError,
    font::FontCacheSnapshot,
    image::{ImageCacheSnapshot, ImageRef, RawImage},
    pdf::{BreakPolicy, Pdf},
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
    vec::U8VecRef,
    xml::Xml,
};

use crate::{
    content::{read_chapter, Chapter, ReadOptions},
    epub::Container,
    position::PageMap,
    settings::ReadingSettings,
};

/// How a reader's pages break: never inside a line, two lines at least at a page's top and
/// bottom (CSS `widows` / `orphans`), a box that asks to stay whole (`break-inside: avoid`:
/// a picture, a heading with the reader's sheet) and a table row kept whole.
#[must_use]
pub fn reading_policy() -> BreakPolicy {
    BreakPolicy {
        max_push_distance: 0.33,
        honor_break_inside: true,
        widows_orphans: true,
        atomic_lines: true,
        atomic_table_rows: true,
        repeat_table_headers: false,
    }
}

/// The pages of `column` (a styled reading column) on pages of `width` x `height` text area.
#[must_use]
pub fn page_map(
    column: StyledDom,
    width: f32,
    height: f32,
    fonts: FontCacheSnapshot,
    images: ImageCacheSnapshot,
) -> PageMap {
    let snapshot = Pdf::create().compute_pagination_with_policy(
        column,
        width,
        height,
        fonts,
        images,
        reading_policy(),
    );
    let breaks: Vec<f32> = (0..snapshot.break_count())
        .map(|i| snapshot.break_y(i))
        .collect();
    PageMap::from_breaks(&breaks, snapshot.total_content_height())
}

/// The reading column: the chapter's DOM in a box of the page's text width with the
/// reader's type stated on the box itself (the same box in the pagination and on screen);
/// `flora` = the app theme is flora (its serif is flora's Garamond).
#[must_use]
pub fn column_dom(chapter: &Xml, settings: &ReadingSettings, text_width: f32, flora: bool) -> Dom {
    Dom::create_div()
        .with_css(settings.column_css(text_width, flora))
        .with_child(Dom::create_from_parsed_xml(Xml::clone(chapter)))
}

/// What a chapter is laid out for.
pub struct ChapterRequest {
    pub book_id: String,
    pub chapter: usize,
    /// The app's counter of layouts asked for: an answer for an older one is stale.
    pub generation: u64,
    pub container: Arc<Container>,
    /// The chapter's path in the container, and whether it is HTML.
    pub path: String,
    pub html: bool,
    pub settings: ReadingSettings,
    pub text_width: f32,
    pub text_height: f32,
    /// The app theme is flora (the column's serif is flora's Garamond).
    pub flora: bool,
    /// The window's fonts (`FontCacheSnapshot::from_layout_info`).
    pub fonts: FontCacheSnapshot,
}

/// A chapter, read, decoded and paginated.
pub struct ChapterReady {
    pub book_id: String,
    pub chapter: usize,
    pub generation: u64,
    /// The settings and page size it was laid out for ([`ReadingSettings::layout_key`]).
    pub layout_key: String,
    pub content: Chapter,
    pub pages: PageMap,
    /// The chapter's pictures, decoded: `(image-cache name, image)`.
    pub images: Vec<(String, ImageRef)>,
}

/// Reads, decodes and paginates one chapter (on a worker thread: [`spawn_chapter`]).
#[must_use]
pub fn load_chapter(request: &ChapterRequest) -> ChapterReady {
    let options = ReadOptions {
        book: request.book_id.clone(),
        page_width: request.text_width,
        page_height: request.text_height,
    };
    // A picture is kept at most twice the page's size in memory.
    let max_px = (request.text_width.max(request.text_height) * 2.0).max(64.0) as u32;
    let mut decoded: Vec<(String, RawImage)> = Vec::new();
    let content = {
        let container = &request.container;
        let mut size_of = |path: &str| -> Option<(u32, u32)> {
            if let Some((_, raw)) = decoded.iter().find(|(p, _)| p == path) {
                return Some((raw.width as u32, raw.height as u32));
            }
            let bytes = container.get(path)?;
            let raw = match RawImage::decode_image_bytes_any(U8VecRef::from(bytes)) {
                ResultRawImageDecodeImageError::Ok(raw) => raw,
                ResultRawImageDecodeImageError::Err(_) => return None,
            };
            let natural = (raw.width as u32, raw.height as u32);
            let raw = if natural.0.max(natural.1) > max_px {
                raw.thumbnail(max_px, max_px).into_option().unwrap_or(raw)
            } else {
                raw
            };
            decoded.push((path.to_string(), raw));
            Some(natural)
        };
        read_chapter(
            container,
            &request.path,
            request.html,
            &options,
            &mut size_of,
        )
    };
    let column = column_dom(
        &content.xml,
        &request.settings,
        request.text_width,
        request.flora,
    );
    let pages = page_map(
        StyledDom::create_from_dom(column),
        request.text_width,
        request.text_height,
        request.fonts.clone(),
        ImageCacheSnapshot::empty(),
    );
    let images = decoded
        .into_iter()
        .filter_map(|(path, raw)| {
            let src = content.images.iter().find(|i| i.path == path)?.src.clone();
            Some((src, ImageRef::create_rawimage(raw).into_option()?))
        })
        .collect();
    ChapterReady {
        book_id: request.book_id.clone(),
        chapter: request.chapter,
        generation: request.generation,
        layout_key: request
            .settings
            .layout_key(request.text_width, request.text_height, request.flora),
        content,
        pages,
        images,
    }
}

struct ChapterInit {
    request: Option<ChapterRequest>,
    on_done: WriteBackCallbackType,
}

extern "C" fn chapter_worker(
    mut init: RefAny,
    mut sender: ThreadSender,
    _receiver: ThreadReceiver,
) {
    let Some((request, on_done)) = init
        .downcast_mut::<ChapterInit>()
        .and_then(|mut i| Some((i.request.take()?, i.on_done)))
    else {
        return;
    };
    let ready = load_chapter(&request);
    // In an Option, so the UI thread takes it out whole ([`take_ready`]).
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(Some(ready)),
    )));
}

/// Lays chapter `request` out on an azul `Thread`; `on_done(app, ChapterReady, info)` gets
/// it on the UI thread ([`take_ready`]).
pub fn spawn_chapter(
    info: &mut CallbackInfo,
    app: &RefAny,
    request: ChapterRequest,
    on_done: WriteBackCallbackType,
) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(ChapterInit {
                request: Some(request),
                on_done,
            }),
            app.clone(),
            chapter_worker,
        ),
    );
}

/// The chapter out of a write-back's message (`None` if it is not one).
#[must_use]
pub fn take_ready(msg: &mut RefAny) -> Option<ChapterReady> {
    let mut guard = msg.downcast_mut::<Option<ChapterReady>>()?;
    guard.take()
}

#[cfg(test)]
mod tests {
    use azul::dom::Dom;

    use super::*;

    #[test]
    fn a_page_of_text_never_ends_inside_a_line() {
        // Twenty paragraphs of three to four lines of 20 px on pages 105 px high: the plain
        // slicing would cut at 105, 210, ... - through a line every time.
        let mut column = Dom::create_div().with_css(
            "display: block; width: 300px; font-size: 10px; line-height: 20px; \
             font-family: sans-serif; margin: 0px; padding: 0px;",
        );
        for i in 0..20 {
            column.add_child(
                Dom::create_p_with_text(format!(
                    "Paragraph {i}: it was the best of times, it was the worst of times, it \
                     was the age of wisdom, it was the age of foolishness."
                ))
                .with_css("margin: 0px; padding: 0px;"),
            );
        }
        let pages = page_map(
            StyledDom::create_from_dom(column),
            300.0,
            105.0,
            FontCacheSnapshot::empty(),
            ImageCacheSnapshot::empty(),
        );
        assert!(pages.page_count() >= 3, "{} pages", pages.page_count());
        for page in 1..pages.page_count() {
            let (top, height) = pages.span(page);
            let into_line = top % 20.0;
            assert!(
                into_line < 0.01 || into_line > 19.99,
                "page {page} starts {into_line} px into a line (at {top})"
            );
            assert!(
                height <= 105.0 + 0.01,
                "page {page} is {height} px, the page 105"
            );
        }
    }
}
