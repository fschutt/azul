//! Where the lines of a page are when lines are put in between them.
//!
//! PAGE BREAKS ARE STABLE: the first line of every page is the first line it was, the page count never changes, and a page
//! that receives lines (an agent's suggestion that waits, or one the reviewer took) GROWS: the lines after the ones put in
//! are pushed down, the page is taller, and nothing else moves. Ink, notes and the reviewer's slashes are kept where the
//! page was first drawn (in the page's own px, the lines as they were); this map is what carries a point of that drawing to
//! where it is shown, and a pen's point back to where it is kept: the ink goes down with the line it was drawn on.
//!
//! A point drawn over the lines that were put in has no line of the page under it; it is kept at the top of the line that
//! follows them (it was drawn between two lines, and that is where the two are).
//!
//! Pure: numbers in, numbers out.

use crate::page_marks::Insert;

/// The most rows a page grows by: more than a conversation puts in (fifty suggestions of a few lines), and a bound on what an
/// archive somebody edited can ask a page to draw.
pub const MAX_EXTRA_ROWS: usize = 500;

/// Lines put in after a line of the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// After which line of the page (0 is the first; none: before the first). A line the page does not have is the last.
    pub after: Option<usize>,
    /// The lines.
    pub lines: Vec<String>,
    /// The reviewer took them (they are green; while they wait they are yellow).
    pub accepted: bool,
    /// Whose they are: the conversation that suggested them.
    pub conversation: u64,
}

impl Block {
    /// How many rows the block takes.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.lines.len()
    }
}

/// What a row of the grown page is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    /// Line `i` of the page as it was.
    Line(usize),
    /// A row of lines put in; `boundary` is the line that follows them (the page's own count after the last line).
    Put { boundary: usize },
}

/// The rows of a page and where each line of the page is among them.
#[derive(Debug, Clone, PartialEq)]
pub struct LineMap {
    line_count: usize,
    pad: f32,
    line_h: f32,
    /// The blocks in the order they are drawn (by the line they follow, the order given among the same line's).
    blocks: Vec<Block>,
    /// Every row of the grown page, top to bottom.
    rows: Vec<Row>,
    /// The row of each line of the page; one more: where the code ends.
    line_row: Vec<usize>,
    /// The row each block begins at.
    block_row: Vec<usize>,
    extra: usize,
}

impl LineMap {
    /// The map of a page nothing was put in, for code that has no page to ask: every point is where it was.
    #[must_use]
    pub const fn plain() -> Self {
        Self {
            line_count: 0,
            pad: 0.0,
            line_h: 0.0,
            blocks: Vec::new(),
            rows: Vec::new(),
            line_row: Vec::new(),
            block_row: Vec::new(),
            extra: 0,
        }
    }

    /// The map of a page of `line_count` lines with `blocks` put in; the code starts `pad` px under the top of the page and a
    /// row is `line_h` px high.
    #[must_use]
    pub fn new(line_count: usize, blocks: Vec<Block>, pad: f32, line_h: f32) -> Self {
        let key = |b: &Block| -> i64 {
            match b.after {
                _ if line_count == 0 => -1,
                None => -1,
                Some(after) => i64::try_from(after.min(line_count - 1)).unwrap_or(-1),
            }
        };
        let mut blocks: Vec<Block> = blocks.into_iter().filter(|b| b.rows() > 0).collect();
        // by the line they follow; the sort is stable, so blocks after the same line stay in the order given
        blocks.sort_by_key(|b| key(b));
        let mut rows: Vec<Row> = Vec::with_capacity(line_count + blocks.iter().map(Block::rows).sum::<usize>());
        let mut line_row = Vec::with_capacity(line_count + 1);
        let mut block_row = vec![0; blocks.len()];
        let mut next_block = 0;
        let put_after = |rows: &mut Vec<Row>, block_row: &mut Vec<usize>, next_block: &mut usize, line: i64, boundary: usize| {
            while *next_block < blocks.len() && key(&blocks[*next_block]) == line {
                block_row[*next_block] = rows.len();
                rows.extend(std::iter::repeat(Row::Put { boundary }).take(blocks[*next_block].rows()));
                *next_block += 1;
            }
        };
        put_after(&mut rows, &mut block_row, &mut next_block, -1, 0);
        for i in 0..line_count {
            line_row.push(rows.len());
            rows.push(Row::Line(i));
            put_after(&mut rows, &mut block_row, &mut next_block, i64::try_from(i).unwrap_or(i64::MAX), i + 1);
        }
        line_row.push(rows.len());
        let extra = rows.len() - line_count;
        Self { line_count, pad, line_h, blocks, rows, line_row, block_row, extra }
    }

    /// [`Self::new`] with the blocks held to [`MAX_EXTRA_ROWS`]: the last one that fits is cut and the ones after it are left
    /// out. The map and how many rows were not kept.
    #[must_use]
    pub fn capped(line_count: usize, blocks: Vec<Block>, pad: f32, line_h: f32) -> (Self, usize) {
        let mut room = MAX_EXTRA_ROWS;
        let mut dropped = 0;
        let kept: Vec<Block> = blocks
            .into_iter()
            .filter_map(|mut b| {
                if b.rows() > room {
                    dropped += b.rows() - room;
                    b.lines.truncate(room);
                }
                room -= b.rows();
                (b.rows() > 0).then_some(b)
            })
            .collect();
        (Self::new(line_count, kept, pad, line_h), dropped)
    }

    /// The map of a page that begins at line `first_line` of the file and has `line_count` lines, with the lines the agents
    /// would put in (`page_marks::inserts_of`): each is a block after the line of the page it follows.
    #[must_use]
    pub fn from_inserts(first_line: usize, line_count: usize, inserts: &[Insert], pad: f32, line_h: f32) -> Self {
        let blocks = inserts
            .iter()
            .map(|i| Block {
                // a line before the page's first is no line of it: the lines go before its first
                after: i.after_line.checked_sub(first_line),
                lines: i.rows.clone(),
                accepted: i.accepted,
                conversation: i.conversation,
            })
            .collect();
        Self::capped(line_count, blocks, pad, line_h).0
    }

    /// Nothing was put in: every point is where it was.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        self.extra == 0
    }

    /// How many rows the page has grown by.
    #[must_use]
    pub fn extra_rows(&self) -> usize {
        self.extra
    }

    /// The blocks in the order they are drawn.
    #[must_use]
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// The blocks that follow line `line` of the page (none: that come before its first), in the order they are drawn: the
    /// page draws a line and then these.
    pub fn blocks_after(&self, line: Option<usize>) -> impl Iterator<Item = &Block> + '_ {
        let key = move |b: &Block| -> Option<usize> {
            match b.after {
                _ if self.line_count == 0 => None,
                None => None,
                Some(after) => Some(after.min(self.line_count - 1)),
            }
        };
        self.blocks.iter().filter(move |b| key(*b) == line)
    }

    /// A number that is the same for maps that draw the same and not for maps that draw differently (an ink frame drawn from one
    /// is not the frame of the other): 0 for a plain page.
    #[must_use]
    pub fn signature(&self) -> u64 {
        if self.extra == 0 {
            return 0;
        }
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut feed = |v: u64| {
            for b in v.to_le_bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        };
        for b in &self.blocks {
            feed(b.after.map_or(u64::MAX, |a| a as u64));
            feed(b.rows() as u64);
        }
        h | 1
    }

    /// The height of the page when its own is `base`.
    #[must_use]
    pub fn grown_height(&self, base: f32) -> f32 {
        base + self.extra as f32 * self.line_h
    }

    /// Where line `i` of the page begins, top of the page: the line after the last is where the code ends.
    #[must_use]
    pub fn line_top(&self, i: usize) -> f32 {
        let row = self.line_row.get(i).copied().unwrap_or_else(|| self.rows.len() + i.saturating_sub(self.line_count));
        self.pad + row as f32 * self.line_h
    }

    /// Where block `k` begins.
    #[must_use]
    pub fn block_top(&self, k: usize) -> f32 {
        self.pad + self.block_row.get(k).copied().unwrap_or(self.rows.len()) as f32 * self.line_h
    }

    /// Where a point of the page as it was is shown: the lines before it keep their places, the lines put in above it push it
    /// down, the position in its line is kept. A number that is no number is as it was.
    #[must_use]
    pub fn to_display(&self, y: f32) -> f32 {
        if !y.is_finite() || y < self.pad || self.extra == 0 {
            return y;
        }
        let rel = y - self.pad;
        let line = (rel / self.line_h).floor();
        if line >= self.line_count as f32 {
            return y + self.extra as f32 * self.line_h;
        }
        let i = line as usize;
        let into = rel - line * self.line_h;
        self.pad + self.line_row[i] as f32 * self.line_h + into
    }

    /// Where a point that is shown is kept: the inverse of [`Self::to_display`]; one over the lines put in is the top of the
    /// line that follows them.
    #[must_use]
    pub fn to_original(&self, y: f32) -> f32 {
        if !y.is_finite() || y < self.pad || self.extra == 0 {
            return y;
        }
        let rel = y - self.pad;
        let row = (rel / self.line_h).floor();
        if row >= self.rows.len() as f32 {
            return y - self.extra as f32 * self.line_h;
        }
        let r = row as usize;
        let into = rel - row * self.line_h;
        match self.rows[r] {
            Row::Line(i) => self.pad + i as f32 * self.line_h + into,
            Row::Put { boundary } => self.pad + boundary as f32 * self.line_h,
        }
    }
}

#[cfg(test)]
#[path = "line_map_test.rs"]
mod line_map_test;
