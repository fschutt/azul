//! A Media Center gallery's geometry: tiles in columns that fill the page's height (two or three
//! rows of covers, more of songs) and run off to the right; the gallery scrolls a column at a
//! time to keep the focused tile in view, and only the columns in view (and one either side) are
//! built. The arrow keys move the focus as Media Center does: Up / Down within a column, Left /
//! Right a column at a time. Plain Rust, tested without a window.

/// What a gallery shows, which decides its tiles' size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileKind {
    /// Square art over two lines: an album, an artist, a genre.
    Square,
    /// A picture (4:3) over one line.
    Picture,
    /// A video (16:9) over two lines.
    Video,
    /// A song: a row of text.
    Song,
}

impl TileKind {
    /// The art's size (logical px) and the caption's height under it.
    #[must_use]
    pub const fn art(self) -> (f32, f32, f32) {
        match self {
            TileKind::Square => (156.0, 156.0, 46.0),
            TileKind::Picture => (208.0, 140.0, 28.0),
            TileKind::Video => (224.0, 126.0, 46.0),
            TileKind::Song => (320.0, 56.0, 0.0),
        }
    }

    /// The whole tile: the art and its caption.
    #[must_use]
    pub const fn size(self) -> (f32, f32) {
        let (w, h, caption) = self.art();
        (w, h + caption)
    }
}

/// The space between two tiles, across and down.
pub const GAP_X: f32 = 22.0;
pub const GAP_Y: f32 = 14.0;

/// A gallery laid into its area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    pub kind: TileKind,
    /// Tiles in a column.
    pub rows: usize,
    /// Columns that fit in the area (the last may be cut).
    pub visible_cols: usize,
}

impl Grid {
    /// The grid of `kind` in an area `width` x `height` (logical px).
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn new(kind: TileKind, width: f32, height: f32) -> Grid {
        let (w, h) = kind.size();
        let rows = ((height.max(0.0) + GAP_Y) / (h + GAP_Y)).floor().max(1.0) as usize;
        let visible_cols = ((width.max(0.0) + GAP_X) / (w + GAP_X)).ceil().max(1.0) as usize;
        Grid {
            kind,
            rows,
            visible_cols,
        }
    }

    /// The column of tile `index`.
    #[must_use]
    pub const fn col(&self, index: usize) -> usize {
        index / self.rows
    }

    /// The columns `count` tiles take.
    #[must_use]
    pub const fn cols(&self, count: usize) -> usize {
        count.div_ceil(self.rows)
    }

    /// Where tile `index` stands in the sheet (logical px from the sheet's corner).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn position(&self, index: usize) -> (f32, f32) {
        let (w, h) = self.kind.size();
        let col = (index / self.rows) as f32;
        let row = (index % self.rows) as f32;
        (col * (w + GAP_X), row * (h + GAP_Y))
    }

    /// How far the sheet is moved left for `first_col` to be the first column in view.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn offset(&self, first_col: usize) -> f32 {
        let (w, _) = self.kind.size();
        first_col as f32 * (w + GAP_X)
    }

    /// The first column in view once `index` is focused, from `first_col` before: the gallery
    /// moves only when the focus comes within a column of the edge (the last column in view is
    /// often cut), and never past the last column.
    #[must_use]
    pub fn scrolled(&self, first_col: usize, index: usize, count: usize) -> usize {
        let col = self.col(index);
        let room = self.visible_cols.saturating_sub(1).max(1);
        let mut first = first_col;
        if col < first + 1 {
            first = col.saturating_sub(1);
        } else if col + 2 > first + room {
            first = (col + 2).saturating_sub(room);
        }
        let last_first = self.cols(count).saturating_sub(room);
        first.min(last_first)
    }

    /// The tiles to build when `first_col` is the first in view: those in view and one column
    /// either side (so a step's new column is already there to slide in).
    #[must_use]
    pub fn built(&self, first_col: usize, count: usize) -> std::ops::Range<usize> {
        let from = first_col.saturating_sub(1) * self.rows;
        let to = ((first_col + self.visible_cols + 1) * self.rows).min(count);
        from.min(to)..to
    }
}

/// An arrow key in a gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Up,
    Down,
    Left,
    Right,
}

/// Where the focus goes from `index` (of `count` tiles) on `step`; `None` when it would leave
/// the gallery (Up from the first row goes to the views above it).
#[must_use]
pub fn step(grid: &Grid, index: usize, count: usize, step: Step) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let index = index.min(count - 1);
    let row = index % grid.rows;
    match step {
        Step::Up => (row > 0).then(|| index - 1),
        Step::Down => (row + 1 < grid.rows && index + 1 < count).then(|| index + 1),
        Step::Left => index.checked_sub(grid.rows),
        Step::Right => {
            let next = index + grid.rows;
            if next < count {
                Some(next)
            } else if grid.col(count - 1) > grid.col(index) {
                // The last column is shorter: its last tile.
                Some(count - 1)
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rows_fill_the_height_and_the_columns_run_right() {
        let g = Grid::new(TileKind::Square, 1000.0, 450.0);
        assert_eq!(g.rows, 2, "two rows of covers in 450 px");
        assert_eq!(g.visible_cols, 6);
        assert_eq!(g.position(0), (0.0, 0.0));
        assert_eq!(g.position(1), (0.0, 216.0));
        assert_eq!(g.position(2), (178.0, 0.0));
        assert_eq!(g.cols(5), 3);
        let songs = Grid::new(TileKind::Song, 1000.0, 450.0);
        assert_eq!(songs.rows, 6);
        assert_eq!(Grid::new(TileKind::Video, 10.0, 10.0).rows, 1, "always one row");
    }

    #[test]
    fn the_arrows_move_down_a_column_and_across_columns() {
        let g = Grid::new(TileKind::Square, 1000.0, 450.0); // 2 rows
        assert_eq!(step(&g, 0, 7, Step::Down), Some(1));
        assert_eq!(step(&g, 1, 7, Step::Down), None, "the bottom row");
        assert_eq!(step(&g, 0, 7, Step::Up), None, "up to the views");
        assert_eq!(step(&g, 1, 7, Step::Up), Some(0));
        assert_eq!(step(&g, 1, 7, Step::Right), Some(3));
        assert_eq!(step(&g, 5, 7, Step::Right), Some(6), "onto the short last column");
        assert_eq!(step(&g, 6, 7, Step::Right), None);
        assert_eq!(step(&g, 3, 7, Step::Left), Some(1));
        assert_eq!(step(&g, 1, 7, Step::Left), None);
        assert_eq!(step(&g, 0, 0, Step::Right), None);
    }

    #[test]
    fn the_gallery_scrolls_only_near_its_edge_and_builds_what_is_in_view() {
        let g = Grid::new(TileKind::Square, 1000.0, 450.0); // 2 rows, 6 columns in view
        let count = 40; // 20 columns
        assert_eq!(g.scrolled(0, 6, count), 0, "column 3 is well in view");
        assert_eq!(g.scrolled(0, 8, count), 1, "column 4 is near the cut edge");
        assert_eq!(g.scrolled(5, 8, count), 3, "back left: one column of margin");
        assert_eq!(g.scrolled(0, 39, count), 15, "never past the last column");
        assert_eq!(g.offset(3), 534.0);
        assert_eq!(g.built(0, count), 0..14);
        assert_eq!(g.built(5, count), 8..24);
        assert_eq!(g.built(15, count), 28..40);
        assert_eq!(g.built(0, 3), 0..3);
    }
}
