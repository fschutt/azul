//! Integer rectangles in document (or view) pixels.

/// A rectangle of whole pixels: `x`, `y` is the top-left pixel, `w` x `h`
/// pixels from there. Empty when `w` or `h` is not positive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub struct IRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl IRect {
    #[must_use]
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    /// The rect from two corners, in any order; the second corner is
    /// exclusive.
    #[must_use]
    pub fn from_corners(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        let (ax, bx) = (x0.min(x1), x0.max(x1));
        let (ay, by) = (y0.min(y1), y0.max(y1));
        Self::new(ax, ay, bx - ax, by - ay)
    }

    /// The pixels a float rectangle touches (rounded outward).
    #[must_use]
    pub fn covering(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        let ax = x0.min(x1).floor() as i32;
        let ay = y0.min(y1).floor() as i32;
        let bx = x0.max(x1).ceil() as i32;
        let by = y0.max(y1).ceil() as i32;
        Self::new(ax, ay, bx - ax, by - ay)
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    /// One past the last column.
    #[must_use]
    pub const fn right(&self) -> i32 {
        self.x + self.w
    }

    /// One past the last row.
    #[must_use]
    pub const fn bottom(&self) -> i32 {
        self.y + self.h
    }

    #[must_use]
    pub const fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }

    /// The bounding box of both; an empty rect adds nothing.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let r = self.right().max(other.right());
        let b = self.bottom().max(other.bottom());
        Self::new(x, y, r - x, b - y)
    }

    /// The overlap, or `None` when they do not overlap (touching edges do not).
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        (r > x && b > y).then(|| Self::new(x, y, r - x, b - y))
    }

    /// This rect grown by `n` pixels on every side.
    #[must_use]
    pub const fn inflate(&self, n: i32) -> Self {
        Self::new(self.x - n, self.y - n, self.w + 2 * n, self.h + 2 * n)
    }

    /// This rect moved by (`dx`, `dy`).
    #[must_use]
    pub const fn offset(&self, dx: i32, dy: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    /// The number of pixels.
    #[must_use]
    pub const fn area(&self) -> i64 {
        if self.is_empty() {
            0
        } else {
            self.w as i64 * self.h as i64
        }
    }
}
