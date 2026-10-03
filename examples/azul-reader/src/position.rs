//! Where the reader is, and the pages of a chapter.
//!
//! A chapter is laid out ONCE as one continuous column at the page's text width; the
//! engine's pagination answers where each page starts in that column (its break ys). Page
//! `i` is the clip window `[start_i, start_i+1)` over the column ([`PageMap::span`]) - the
//! engine's own slicer does the same: content is never moved, only clipped.
//!
//! A reading [`Position`] is a chapter and how far into the chapter's laid-out height the
//! page starts, as a fraction: it survives a re-layout (another font size, another window
//! size) to about the same text, and a page turn is a page index in the current layout.
//! The book's progress weights the chapters by their size ([`crate::epub::Book::chapter_weights`]),
//! which no layout changes.

use serde::{Deserialize, Serialize};

/// Where the reader is: a chapter (its spine index) and how far into the chapter the page
/// starts (0 = its start, 1 = its last page).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Position {
    pub chapter: usize,
    pub fraction: f32,
}

impl Position {
    /// The start of chapter `chapter`.
    #[must_use]
    pub const fn chapter_start(chapter: usize) -> Self {
        Self {
            chapter,
            fraction: 0.0,
        }
    }

    /// The last page of chapter `chapter`.
    #[must_use]
    pub const fn chapter_end(chapter: usize) -> Self {
        Self {
            chapter,
            fraction: 1.0,
        }
    }

    /// This position inside a book of `chapters` chapters: the chapter at most the last, the
    /// fraction in `0..=1` (a NaN is 0).
    #[must_use]
    pub fn clamped(self, chapters: usize) -> Self {
        unimplemented!("RED: Position::clamped {chapters} {self:?}")
    }
}

/// The pages of one chapter at one layout: where each page starts in the chapter's column.
#[derive(Debug, Clone, PartialEq)]
pub struct PageMap {
    /// The start of every page, ascending, the first 0.
    starts: Vec<f32>,
    /// The column's height.
    total: f32,
}

impl Default for PageMap {
    fn default() -> Self {
        Self::single(0.0)
    }
}

impl PageMap {
    /// One page holding a column `total` high (an empty chapter, a layout not known yet).
    #[must_use]
    pub fn single(total: f32) -> Self {
        Self {
            starts: vec![0.0],
            total: total.max(0.0),
        }
    }

    /// The pages of a column `total` high from the engine's break ys: ascending, a break
    /// within half a pixel of the one before it dropped, only breaks strictly inside the
    /// column kept, the first page at 0.
    #[must_use]
    pub fn from_breaks(breaks: &[f32], total: f32) -> Self {
        unimplemented!("RED: PageMap::from_breaks {} {total}", breaks.len())
    }

    /// The number of pages (at least 1).
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.starts.len().max(1)
    }

    /// The column's height.
    #[must_use]
    pub fn total_height(&self) -> f32 {
        self.total
    }

    /// Page `page`'s clip window: its top in the column and its height (the last page runs to
    /// the column's end). A page past the end is the last page.
    #[must_use]
    pub fn span(&self, page: usize) -> (f32, f32) {
        unimplemented!("RED: PageMap::span {page} {}", self.starts.len())
    }

    /// The page the column's `y` is on.
    #[must_use]
    pub fn page_of_y(&self, y: f32) -> usize {
        unimplemented!("RED: PageMap::page_of_y {y} {}", self.starts.len())
    }

    /// The page a position's fraction lands on (1.0 = the last page), with half a pixel of
    /// tolerance so a page's own start fraction comes back to that page.
    #[must_use]
    pub fn page_of_fraction(&self, fraction: f32) -> usize {
        unimplemented!(
            "RED: PageMap::page_of_fraction {fraction} {}",
            self.starts.len()
        )
    }

    /// The fraction page `page` starts at.
    #[must_use]
    pub fn fraction_of_page(&self, page: usize) -> f32 {
        unimplemented!(
            "RED: PageMap::fraction_of_page {page} {}",
            self.starts.len()
        )
    }
}

/// The first page shown when page `page` is to be seen and the window shows `per_view`
/// pages side by side (1, or 2 for a spread whose left page is an even page).
#[must_use]
pub fn view_start(page: usize, per_view: usize) -> usize {
    unimplemented!("RED: view_start {page} {per_view}")
}

/// Where a page turn goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// Another page of the same chapter (the first page of the view).
    Page(usize),
    /// The first page of the next chapter.
    NextChapter,
    /// The last page of the previous chapter.
    PreviousChapterEnd,
    /// Nothing after the book's last page.
    AtEnd,
    /// Nothing before the book's first page.
    AtStart,
}

/// A page turn (`forward` or back by one view of `per_view` pages) from `page` of a chapter
/// of `page_count` pages, chapter `chapter` of `chapters`.
#[must_use]
pub fn turn(
    page: usize,
    per_view: usize,
    page_count: usize,
    chapter: usize,
    chapters: usize,
    forward: bool,
) -> Turn {
    unimplemented!("RED: turn {page} {per_view} {page_count} {chapter} {chapters} {forward}")
}

/// The book's progress (`0..=1`) at `position`, each chapter weighing `weights[chapter]`.
#[must_use]
pub fn book_progress(weights: &[u64], position: Position) -> f32 {
    unimplemented!("RED: book_progress {} {position:?}", weights.len())
}

/// The position at the book's progress `progress` (`0..=1`; the progress slider): the
/// chapter it falls in and how far into it.
#[must_use]
pub fn position_at_progress(weights: &[u64], progress: f32) -> Position {
    unimplemented!("RED: position_at_progress {} {progress}", weights.len())
}

/// A progress as a whole percentage: `"37%"`.
#[must_use]
pub fn percent_label(progress: f32) -> String {
    unimplemented!("RED: percent_label {progress}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn the_pages_start_at_the_engines_breaks_inside_the_column() {
        let pages = PageMap::from_breaks(
            &[600.0, 0.0, 1200.0, 1200.3, 2400.0, -5.0, f32::NAN],
            1800.0,
        );
        assert_eq!(
            pages.page_count(),
            3,
            "0, 600, 1200: a repeat, a break at or past the end, NaN go"
        );
        assert_eq!(pages.span(0), (0.0, 600.0));
        assert_eq!(pages.span(1), (600.0, 600.0));
        assert_eq!(
            pages.span(2),
            (1200.0, 600.0),
            "the last page runs to the column's end"
        );
        assert_eq!(
            pages.span(9),
            (1200.0, 600.0),
            "past the end: the last page"
        );
        assert_eq!(PageMap::from_breaks(&[], 300.0).page_count(), 1);
        assert_eq!(PageMap::default().page_count(), 1);
    }

    #[test]
    fn a_y_and_a_fraction_find_their_page_and_a_page_its_fraction() {
        let pages = PageMap::from_breaks(&[400.0, 800.0], 1000.0);
        assert_eq!(pages.page_of_y(0.0), 0);
        assert_eq!(pages.page_of_y(399.9), 0);
        assert_eq!(pages.page_of_y(400.0), 1);
        assert_eq!(pages.page_of_y(5000.0), 2);
        assert_eq!(pages.page_of_y(-3.0), 0);
        assert!(close(pages.fraction_of_page(1), 0.4));
        assert!(close(pages.fraction_of_page(2), 0.8));
        for page in 0..3 {
            assert_eq!(
                pages.page_of_fraction(pages.fraction_of_page(page)),
                page,
                "page {page} round trip"
            );
        }
        assert_eq!(pages.page_of_fraction(1.0), 2, "1.0 = the last page");
        assert_eq!(pages.page_of_fraction(0.55), 1);
        assert_eq!(
            PageMap::single(0.0).page_of_fraction(0.7),
            0,
            "an empty column has one page"
        );
        assert_eq!(PageMap::single(0.0).fraction_of_page(0), 0.0);
    }

    #[test]
    fn a_position_after_a_reflow_lands_on_the_page_with_about_the_same_text() {
        // 100 lines of 20 px on 10 pages; then a bigger font: 100 lines of 30 px on 15 pages.
        let small = PageMap::from_breaks(
            &(1..10).map(|i| i as f32 * 200.0).collect::<Vec<_>>(),
            2000.0,
        );
        let big = PageMap::from_breaks(
            &(1..15).map(|i| i as f32 * 200.0).collect::<Vec<_>>(),
            3000.0,
        );
        let at = small.fraction_of_page(6); // line 60
        assert_eq!(
            big.page_of_fraction(at),
            9,
            "line 60 is on the big layout's page 9 (lines 60-66)"
        );
    }

    #[test]
    fn a_spread_starts_on_an_even_page() {
        assert_eq!(view_start(5, 1), 5);
        assert_eq!(view_start(5, 2), 4);
        assert_eq!(view_start(4, 2), 4);
        assert_eq!(view_start(0, 2), 0);
        assert_eq!(view_start(3, 0), 3, "no pages per view counts as one");
    }

    #[test]
    fn a_page_turn_goes_through_the_chapter_then_to_the_next_and_stops_at_the_ends() {
        assert_eq!(turn(0, 1, 3, 0, 2, true), Turn::Page(1));
        assert_eq!(turn(2, 1, 3, 0, 2, true), Turn::NextChapter);
        assert_eq!(turn(2, 1, 3, 1, 2, true), Turn::AtEnd);
        assert_eq!(turn(1, 1, 3, 1, 2, false), Turn::Page(0));
        assert_eq!(turn(0, 1, 3, 1, 2, false), Turn::PreviousChapterEnd);
        assert_eq!(turn(0, 1, 3, 0, 2, false), Turn::AtStart);
        // A spread of two: pages 0-1, 2-3, 4.
        assert_eq!(
            turn(1, 2, 5, 0, 1, true),
            Turn::Page(2),
            "from the view 0-1"
        );
        assert_eq!(turn(2, 2, 5, 0, 1, true), Turn::Page(4));
        assert_eq!(turn(4, 2, 5, 0, 1, true), Turn::AtEnd);
        assert_eq!(
            turn(5, 2, 5, 0, 1, false),
            Turn::Page(2),
            "a page past the end turns back from the last view"
        );
        assert_eq!(turn(3, 2, 5, 0, 1, false), Turn::Page(0));
    }

    #[test]
    fn the_books_progress_weighs_the_chapters_and_comes_back_as_a_position() {
        let weights = [100, 300, 600];
        assert!(close(
            book_progress(&weights, Position::chapter_start(0)),
            0.0
        ));
        assert!(close(
            book_progress(
                &weights,
                Position {
                    chapter: 1,
                    fraction: 0.5
                }
            ),
            0.25
        ));
        assert!(close(
            book_progress(&weights, Position::chapter_end(2)),
            1.0
        ));
        assert!(
            close(
                book_progress(
                    &weights,
                    Position {
                        chapter: 9,
                        fraction: 2.0
                    }
                ),
                1.0
            ),
            "clamped"
        );
        assert_eq!(book_progress(&[], Position::default()), 0.0);
        let back = position_at_progress(&weights, 0.25);
        assert_eq!(back.chapter, 1);
        assert!(close(back.fraction, 0.5));
        assert_eq!(
            position_at_progress(&weights, 0.0),
            Position::chapter_start(0)
        );
        assert_eq!(
            position_at_progress(&weights, 1.0),
            Position::chapter_end(2)
        );
        assert_eq!(
            position_at_progress(&weights, -1.0),
            Position::chapter_start(0)
        );
        assert_eq!(position_at_progress(&[], 0.5), Position::default());
    }

    #[test]
    fn a_position_is_kept_inside_the_book() {
        assert_eq!(
            Position {
                chapter: 7,
                fraction: 1.5
            }
            .clamped(3),
            Position::chapter_end(2)
        );
        assert_eq!(
            Position {
                chapter: 1,
                fraction: f32::NAN
            }
            .clamped(3),
            Position::chapter_start(1)
        );
        assert_eq!(
            Position {
                chapter: 1,
                fraction: -0.5
            }
            .clamped(0),
            Position::chapter_start(0)
        );
    }

    #[test]
    fn a_progress_reads_as_a_whole_percentage() {
        assert_eq!(percent_label(0.254), "25%");
        assert_eq!(percent_label(0.996), "100%");
        assert_eq!(percent_label(-1.0), "0%");
        assert_eq!(percent_label(f32::NAN), "0%");
    }

    #[test]
    fn a_position_is_stored_as_json() {
        let p = Position {
            chapter: 3,
            fraction: 0.5,
        };
        let json = serde_json::to_string(&p).expect("json");
        assert_eq!(json, "{\"chapter\":3,\"fraction\":0.5}");
        assert_eq!(serde_json::from_str::<Position>(&json).expect("back"), p);
    }
}
