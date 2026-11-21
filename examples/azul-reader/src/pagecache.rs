//! The pages drawn ahead: pictures of the pages around the one on screen.
//!
//! A page on screen is a clip window over the chapter's reading column, and laying that column
//! out is the cost of a frame: the whole chapter, once per page shown. So the pages around the
//! view - the spread on screen, the next one, the one before - are DRAWN on a worker thread
//! (`Pdf::render_pages_with_policy`: the chapter laid out on pages with the reading policy, a
//! run of pages drawn by the CPU renderer one under the other) at the window's DPI, and a page
//! whose picture is ready is shown as a clip window over that picture instead. A turn to a
//! drawn page costs nothing but the slide; the live column stays the fallback while a page is
//! not drawn yet. The pictures are for one layout ([`PageCache::key`]: the book, the chapter,
//! the settings and page size, the DPI, the paper): another key lets them all go.

use std::ops::Range;

use azul::{
    callbacks::{CallbackInfo, RefAny, WriteBackCallbackType},
    css::ColorU,
    dom::StyledDom,
    font::FontCacheSnapshot,
    image::{ImageCacheSnapshot, ImageRef},
    pdf::Pdf,
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
    xml::Xml,
};

use crate::{paginate, settings::ReadingSettings};

/// The most pages one run draws: a picture taller than this many pages could pass a GPU's
/// largest texture on a 2x display.
pub const MAX_RUN: usize = 4;

/// The pages kept drawn around the view starting at `page`: the spread before, the one on
/// screen, the one after.
#[must_use]
pub fn wanted(page: usize, per_view: usize, page_count: usize) -> Range<usize> {
    let per_view = per_view.max(1);
    page.saturating_sub(per_view).min(page_count)..(page + 2 * per_view).min(page_count)
}

/// Whether the run `(first, count)` holds a page around the view (it is kept).
#[must_use]
pub fn keeps(run: (usize, usize), page: usize, per_view: usize, page_count: usize) -> bool {
    let around = wanted(page, per_view, page_count);
    run.0 < around.end && run.0 + run.1 > around.start
}

/// The next run to draw (`(first page, count)`) for the view starting at `page`, `drawn`
/// holding the runs drawn or being drawn: the first page not drawn of the view and the next
/// spread (up to [`MAX_RUN`] pages in one run), else of the spread before.
#[must_use]
pub fn next_run(
    page: usize,
    per_view: usize,
    page_count: usize,
    drawn: &[(usize, usize)],
) -> Option<(usize, usize)> {
    let per_view = per_view.max(1);
    let is_drawn = |p: usize| drawn.iter().any(|(first, count)| p >= *first && p < first + count);
    let ahead = page.min(page_count)..(page + 2 * per_view).min(page_count);
    let before = page.saturating_sub(per_view).min(page_count)..page.min(page_count);
    for range in [ahead, before] {
        let Some(first) = range.clone().find(|p| !is_drawn(*p)) else {
            continue;
        };
        let count = (first..range.end)
            .take_while(|p| !is_drawn(*p))
            .take(MAX_RUN)
            .count();
        return Some((first, count));
    }
    None
}

/// A run of pages drawn as one picture, one under the other.
pub struct PageRun {
    /// The run's first page in the chapter.
    pub first: usize,
    /// How many pages the picture holds (each the page's text height tall).
    pub count: usize,
    /// The picture, at the DPI of the cache's key.
    pub image: ImageRef,
}

/// The pictures of the pages around the view, for one layout.
#[derive(Default)]
pub struct PageCache {
    /// What the pictures were drawn for (the book, the chapter, the settings and page size,
    /// the DPI, the paper); a view with another key shows none of them.
    pub key: String,
    /// The runs drawn.
    pub runs: Vec<PageRun>,
    /// The run being drawn on a worker (`(first, count)`), at most one at a time.
    pub drawing: Option<(usize, usize)>,
    /// A run could not be drawn: no more are asked for under this key, the live column shows
    /// the pages (without it a failing draw would be asked for again at every tick).
    pub failed: bool,
}

impl PageCache {
    /// The picture page `page` is in, and the page's place in it (pages down from its top).
    #[must_use]
    pub fn find(&self, page: usize) -> Option<(&PageRun, usize)> {
        self.runs
            .iter()
            .find(|run| page >= run.first && page < run.first + run.count)
            .map(|run| (run, page - run.first))
    }

    /// The runs drawn and the one being drawn, as `(first, count)`.
    #[must_use]
    pub fn covered(&self) -> Vec<(usize, usize)> {
        self.runs
            .iter()
            .map(|run| (run.first, run.count))
            .chain(self.drawing)
            .collect()
    }
}

/// A `#rrggbb` colour (the paper's), black for anything else.
#[must_use]
pub fn hex_color(text: &str) -> ColorU {
    let hex = text.trim_start_matches('#');
    let byte = |i: usize| {
        hex.get(i..i + 2)
            .and_then(|h| u8::from_str_radix(h, 16).ok())
            .unwrap_or(0)
    };
    ColorU {
        r: byte(0),
        g: byte(2),
        b: byte(4),
        a: 255,
    }
}

/// What a run of pages is drawn from.
pub struct DrawRequest {
    /// The cache key it is drawn for ([`PageCache::key`]).
    pub key: String,
    /// The run: its first page and how many pages.
    pub first: usize,
    pub count: usize,
    /// The chapter's reading tree, the settings, the page's text area: the column the
    /// pagination laid out.
    pub chapter: Xml,
    pub settings: ReadingSettings,
    pub text_width: f32,
    pub text_height: f32,
    /// The app theme is flora (the column's serif is flora's Garamond).
    pub flora: bool,
    /// The paper's background and ink.
    pub paper: ColorU,
    pub ink: String,
    /// Device pixels per logical pixel.
    pub dpi: f32,
    pub fonts: FontCacheSnapshot,
    /// The window's images (the chapter's pictures among them).
    pub images: ImageCacheSnapshot,
}

/// A run drawn.
pub struct Drawn {
    /// The cache key it was drawn for: an answer for another key is dropped.
    pub key: String,
    /// The run: its first page and how many pages.
    pub first: usize,
    pub count: usize,
    /// The picture; `None` when it could not be drawn.
    pub image: Option<ImageRef>,
}

/// Draws a run of pages (on a worker thread: [`spawn_draw`]).
#[must_use]
pub fn draw(request: DrawRequest) -> Drawn {
    let column = paginate::column_dom(
        &request.chapter,
        &request.settings,
        request.text_width,
        request.flora,
    );
    // The page frame on screen gives the column its ink; here the column has none around it.
    let page = azul::dom::Dom::create_div()
        .with_css(format!("display: block; margin: 0px; padding: 0px; color: {};", request.ink))
        .with_child(column);
    let image = Pdf::create()
        .render_pages_with_policy(
            StyledDom::create_from_dom(page),
            request.text_width,
            request.text_height,
            request.fonts,
            request.images,
            paginate::reading_policy(),
            request.first,
            request.count,
            request.dpi,
            request.paper,
        )
        .into_option()
        .and_then(|raw| ImageRef::create_rawimage(raw).into_option());
    Drawn {
        key: request.key,
        first: request.first,
        count: request.count,
        image,
    }
}

struct DrawInit {
    request: Option<DrawRequest>,
    on_done: WriteBackCallbackType,
}

extern "C" fn draw_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((request, on_done)) = init
        .downcast_mut::<DrawInit>()
        .and_then(|mut i| Some((i.request.take()?, i.on_done)))
    else {
        return;
    };
    let drawn = draw(request);
    // In an Option, so the UI thread takes it out whole ([`take_drawn`]).
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(Some(drawn)),
    )));
}

/// Draws `request` on an azul `Thread`; `on_done(app, Drawn, info)` gets it on the UI thread
/// ([`take_drawn`]).
pub fn spawn_draw(
    info: &mut CallbackInfo,
    app: &RefAny,
    request: DrawRequest,
    on_done: WriteBackCallbackType,
) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(DrawInit {
                request: Some(request),
                on_done,
            }),
            app.clone(),
            draw_worker,
        ),
    );
}

/// The run out of a write-back's message (`None` if it is not one).
#[must_use]
pub fn take_drawn(msg: &mut RefAny) -> Option<Drawn> {
    let mut guard = msg.downcast_mut::<Option<Drawn>>()?;
    guard.take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_and_the_next_spread_are_drawn_first_in_one_run() {
        assert_eq!(next_run(4, 2, 20, &[]), Some((4, 4)));
    }

    #[test]
    fn then_the_spread_before_the_view_is_drawn() {
        assert_eq!(next_run(4, 2, 20, &[(4, 4)]), Some((2, 2)));
    }

    #[test]
    fn nothing_is_drawn_when_the_pages_around_the_view_are() {
        assert_eq!(next_run(4, 2, 20, &[(2, 2), (4, 4)]), None);
        assert_eq!(next_run(4, 2, 20, &[(0, 4), (4, 4)]), None);
    }

    #[test]
    fn the_runs_stop_at_the_chapters_ends() {
        assert_eq!(next_run(18, 2, 20, &[]), Some((18, 2)));
        assert_eq!(next_run(18, 2, 20, &[(18, 2)]), Some((16, 2)));
        assert_eq!(next_run(0, 1, 1, &[]), Some((0, 1)));
        assert_eq!(next_run(0, 1, 1, &[(0, 1)]), None);
    }

    #[test]
    fn a_run_far_from_the_view_is_let_go() {
        assert!(keeps((2, 2), 4, 2, 20));
        assert!(keeps((6, 4), 4, 2, 20));
        assert!(!keeps((10, 4), 4, 2, 20));
        assert!(!keeps((0, 2), 4, 2, 20));
    }
}
