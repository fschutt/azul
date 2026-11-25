//! The whiteboard's plane: points, rectangles, the curve of an arrow, polygons and the view that
//! maps the screen to the board. Logical px in `f32`, y down. No window, no azul: every function
//! is a value in, a value out, and is tested on its own (`whiteboard_geom_test.rs`).

use std::ops::{Add, Mul, Sub};

/// A point of the plane.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

impl Pt {
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// The distance to `other`.
    #[must_use]
    pub fn dist(self, other: Self) -> f32 {
        (self - other).len()
    }

    /// The length of the vector from the origin to this point.
    #[must_use]
    pub fn len(self) -> f32 {
        self.x.hypot(self.y)
    }

    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }

    /// The z of the cross product: positive when `other` is clockwise of this vector on a screen
    /// (y down).
    #[must_use]
    pub fn cross(self, other: Self) -> f32 {
        self.x * other.y - self.y * other.x
    }

    /// The point `t` of the way to `other`.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }

    /// The vector of length 1 in this direction (the zero vector stays zero).
    #[must_use]
    pub fn unit(self) -> Self {
        let len = self.len();
        if len > f32::EPSILON {
            self * (1.0 / len)
        } else {
            Self::default()
        }
    }

    /// This vector turned a quarter clockwise on a screen (y down): going right, it points down.
    #[must_use]
    pub const fn perp(self) -> Self {
        Self { x: -self.y, y: self.x }
    }

    /// Neither coordinate is NaN or infinite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Add for Pt {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Pt {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f32> for Pt {
    type Output = Self;
    fn mul(self, k: f32) -> Self {
        Self::new(self.x * k, self.y * k)
    }
}

/// A width and a height.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

impl Size {
    #[must_use]
    pub const fn new(w: f32, h: f32) -> Self {
        Self { w, h }
    }
}

/// A rectangle: its top-left corner and its size.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// The rectangle between two opposite corners, whichever way round they come.
    #[must_use]
    pub fn from_corners(a: Pt, b: Pt) -> Self {
        Self::new(a.x.min(b.x), a.y.min(b.y), (a.x - b.x).abs(), (a.y - b.y).abs())
    }

    /// The box around `points` (`None` for none).
    #[must_use]
    pub fn from_points(points: &[Pt]) -> Option<Self> {
        let first = points.first()?;
        let (mut lo, mut hi) = (*first, *first);
        for p in &points[1..] {
            lo = Pt::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Pt::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        Some(Self::from_corners(lo, hi))
    }

    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    #[must_use]
    pub fn origin(&self) -> Pt {
        Pt::new(self.x, self.y)
    }

    #[must_use]
    pub fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }

    #[must_use]
    pub fn center(&self) -> Pt {
        Pt::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    #[must_use]
    pub fn area(&self) -> f32 {
        self.w.max(0.0) * self.h.max(0.0)
    }

    /// Whether `p` is in the rectangle, its edges included.
    #[must_use]
    pub fn contains(&self, p: Pt) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    /// The rectangle `d` larger on every side (smaller for a negative `d`, empty when it is
    /// smaller than nothing).
    #[must_use]
    pub fn inflate(&self, d: f32) -> Self {
        Self::new(self.x - d, self.y - d, self.w + 2.0 * d, self.h + 2.0 * d)
    }

    #[must_use]
    pub fn translate(&self, by: Pt) -> Self {
        Self::new(self.x + by.x, self.y + by.y, self.w, self.h)
    }

    /// The smallest rectangle holding both.
    #[must_use]
    pub fn union(&self, o: &Self) -> Self {
        Self::from_corners(
            Pt::new(self.x.min(o.x), self.y.min(o.y)),
            Pt::new(self.right().max(o.right()), self.bottom().max(o.bottom())),
        )
    }

    /// What both share (`None` when they only touch or are apart).
    #[must_use]
    pub fn intersection(&self, o: &Self) -> Option<Self> {
        let (x0, y0) = (self.x.max(o.x), self.y.max(o.y));
        let (x1, y1) = (self.right().min(o.right()), self.bottom().min(o.bottom()));
        (x1 > x0 && y1 > y0).then(|| Self::new(x0, y0, x1 - x0, y1 - y0))
    }

    #[must_use]
    pub fn intersects(&self, o: &Self) -> bool {
        self.intersection(o).is_some()
    }

    /// How far `p` is from the rectangle: 0 inside, else the straight distance to the nearest
    /// point of it.
    #[must_use]
    pub fn distance_to(&self, p: Pt) -> f32 {
        let dx = (self.x - p.x).max(0.0).max(p.x - self.right());
        let dy = (self.y - p.y).max(0.0).max(p.y - self.bottom());
        dx.hypot(dy)
    }
}

/// One side of a box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    /// Every side.
    pub const ALL: [Self; 4] = [Self::Top, Self::Right, Self::Bottom, Self::Left];

    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::Top => Self::Bottom,
            Self::Right => Self::Left,
            Self::Bottom => Self::Top,
            Self::Left => Self::Right,
        }
    }

    /// The name in the board file.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Right => "right",
            Self::Bottom => "bottom",
            Self::Left => "left",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// The middle of a side of `r`.
#[must_use]
pub fn side_mid(r: &Rect, side: Side) -> Pt {
    let c = r.center();
    match side {
        Side::Top => Pt::new(c.x, r.y),
        Side::Right => Pt::new(r.right(), c.y),
        Side::Bottom => Pt::new(c.x, r.bottom()),
        Side::Left => Pt::new(r.x, c.y),
    }
}

/// The side of `r` whose line `p` is nearest to.
#[must_use]
pub fn nearest_side(r: &Rect, p: Pt) -> Side {
    let distances = [
        (Side::Left, (p.x - r.x).abs()),
        (Side::Right, (p.x - r.right()).abs()),
        (Side::Top, (p.y - r.y).abs()),
        (Side::Bottom, (p.y - r.bottom()).abs()),
    ];
    let mut best = distances[0];
    for d in &distances[1..] {
        if d.1 < best.1 {
            best = *d;
        }
    }
    best.0
}

/// The side of `r` whose rim `p` is on: a band of `band` px along the edge, inside and outside
/// (`None` in the middle of the box and away from it).
#[must_use]
pub fn rim_side(r: &Rect, p: Pt, band: f32) -> Option<Side> {
    if !r.inflate(band).contains(p) || r.inflate(-band).contains(p) {
        return None;
    }
    Some(nearest_side(r, p))
}

/// Where the line from the middle of `r` to `toward` leaves `r`: the point an arrow starts from.
/// With no direction (`toward` at the middle) it is the middle of the right side.
#[must_use]
pub fn ray_exit(r: &Rect, toward: Pt) -> Pt {
    let c = r.center();
    let d = toward - c;
    if d.x.abs() <= f32::EPSILON && d.y.abs() <= f32::EPSILON {
        return side_mid(r, Side::Right);
    }
    let tx = if d.x.abs() > f32::EPSILON { (r.w * 0.5) / d.x.abs() } else { f32::INFINITY };
    let ty = if d.y.abs() > f32::EPSILON { (r.h * 0.5) / d.y.abs() } else { f32::INFINITY };
    c + d * tx.min(ty)
}

/// The most an arrow may bend: the control point of its curve is this many chord lengths from the
/// chord (a loop is no arrow).
pub const MAX_BEND: f32 = 0.75;
/// A stroke that bends less than this is drawn straight (a hand does not draw a perfect line).
pub const STRAIGHT_BEND: f32 = 0.06;
/// How many pieces a curve is cut in to measure it and to find what is near it.
const CURVE_PIECES: usize = 24;

/// The curve of an arrow: a quadratic Bezier from `a` to `b` pulled towards `c`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    pub a: Pt,
    pub c: Pt,
    pub b: Pt,
}

impl Curve {
    /// The curve from `a` to `b` whose control point is `bend` chord lengths off the middle of the
    /// chord: 0 is straight, a positive `bend` bulges to the right of the way the arrow goes (on a
    /// screen with y down, going right it bulges down).
    #[must_use]
    pub fn bent(a: Pt, b: Pt, bend: f32) -> Self {
        let chord = b - a;
        let mid = a.lerp(b, 0.5);
        Self { a, b, c: mid + chord.perp().unit() * (bend * chord.len()) }
    }

    /// The point `t` (0 to 1) of the way along the curve.
    #[must_use]
    pub fn at(&self, t: f32) -> Pt {
        let u = 1.0 - t;
        self.a * (u * u) + self.c * (2.0 * u * t) + self.b * (t * t)
    }

    /// The direction of the curve at `t`, of length 1.
    #[must_use]
    pub fn tangent_at(&self, t: f32) -> Pt {
        let u = 1.0 - t;
        let d = (self.c - self.a) * (2.0 * u) + (self.b - self.c) * (2.0 * t);
        let unit = d.unit();
        if unit == Pt::default() {
            (self.b - self.a).unit()
        } else {
            unit
        }
    }

    /// The curve as `pieces` straight pieces: `pieces + 1` points.
    #[must_use]
    pub fn flatten(&self, pieces: usize) -> Vec<Pt> {
        let n = pieces.max(1);
        (0..=n).map(|i| self.at(i as f32 / n as f32)).collect()
    }

    /// How long the curve is.
    #[must_use]
    pub fn length(&self) -> f32 {
        path_length(&self.flatten(CURVE_PIECES))
    }

    /// How far `p` is from the curve.
    #[must_use]
    pub fn distance_to(&self, p: Pt) -> f32 {
        self.flatten(CURVE_PIECES)
            .windows(2)
            .map(|w| dist_point_segment(p, w[0], w[1]))
            .fold(f32::MAX, f32::min)
    }

    /// The box around the curve.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        Rect::from_points(&self.flatten(CURVE_PIECES)).unwrap_or_default()
    }
}

/// How much the stroke `points` bends (the `bend` of [`Curve::bent`] from its first point to its
/// last): read from how far it strays from the chord. A stroke that is nearly straight reads 0,
/// and none is read past [`MAX_BEND`].
#[must_use]
pub fn fit_bend(points: &[Pt]) -> f32 {
    let (Some(&a), Some(&b)) = (points.first(), points.last()) else {
        return 0.0;
    };
    let chord = b - a;
    let len = chord.len();
    if len < 1.0 {
        return 0.0;
    }
    let side = chord.perp().unit();
    // The farthest the stroke strays from the chord, with its sign. A quadratic curve's middle is
    // half the control point's offset away from the chord.
    let mut stray = 0.0_f32;
    for p in points {
        let d = (*p - a).dot(side);
        if d.abs() > stray.abs() {
            stray = d;
        }
    }
    let bend = 2.0 * stray / len;
    if bend.abs() < STRAIGHT_BEND {
        0.0
    } else {
        bend.clamp(-MAX_BEND, MAX_BEND)
    }
}

/// The head of an arrow: its tip and the two corners of its base, `len` behind the tip along
/// `dir`, `half_width` to each side.
#[must_use]
pub fn arrowhead(tip: Pt, dir: Pt, len: f32, half_width: f32) -> [Pt; 3] {
    let d = if dir.unit() == Pt::default() { Pt::new(1.0, 0.0) } else { dir.unit() };
    let base = tip - d * len;
    let side = d.perp() * half_width;
    [tip, base + side, base - side]
}

/// How far `p` is from the segment `a` to `b` (a segment of one point is that point).
#[must_use]
pub fn dist_point_segment(p: Pt, a: Pt, b: Pt) -> f32 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    let t = if len2 <= f32::EPSILON { 0.0 } else { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) };
    p.dist(a + ab * t)
}

/// Where the segment `p1`-`p2` crosses the segment `p3`-`p4` (`None` when they do not, or run
/// side by side).
#[must_use]
pub fn segment_crossing(p1: Pt, p2: Pt, p3: Pt, p4: Pt) -> Option<Pt> {
    let r = p2 - p1;
    let s = p4 - p3;
    let denominator = r.cross(s);
    if denominator.abs() <= f32::EPSILON {
        return None;
    }
    let t = (p3 - p1).cross(s) / denominator;
    let u = (p3 - p1).cross(r) / denominator;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| p1 + r * t)
}

/// The length of the line through `points`.
#[must_use]
pub fn path_length(points: &[Pt]) -> f32 {
    points.windows(2).map(|w| w[0].dist(w[1])).sum()
}

/// The area `poly` encloses, signed by the way round it goes (0 for fewer than three points).
#[must_use]
pub fn polygon_area(poly: &[Pt]) -> f32 {
    if poly.len() < 3 {
        return 0.0;
    }
    let mut twice = 0.0;
    for (i, p) in poly.iter().enumerate() {
        let q = poly[(i + 1) % poly.len()];
        twice += p.x * q.y - q.x * p.y;
    }
    twice * 0.5
}

/// Whether `p` is inside `poly` by the even-odd rule (a line that crosses the edge an odd number
/// of times is inside).
#[must_use]
pub fn point_in_polygon(poly: &[Pt], p: Pt) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// `poly` cut to what is on the inner side of one edge of a rectangle.
fn clip_edge(poly: &[Pt], inside: impl Fn(Pt) -> bool, cut: impl Fn(Pt, Pt) -> Pt) -> Vec<Pt> {
    let mut out = Vec::with_capacity(poly.len() + 4);
    for (i, &cur) in poly.iter().enumerate() {
        let prev = poly[(i + poly.len() - 1) % poly.len()];
        match (inside(prev), inside(cur)) {
            (true, true) => out.push(cur),
            (true, false) => out.push(cut(prev, cur)),
            (false, true) => {
                out.push(cut(prev, cur));
                out.push(cur);
            }
            (false, false) => {}
        }
    }
    out
}

/// The area `poly` shares with `r`. `poly` may be concave: what is left of a concave outline after
/// the cut has edges that run along the rectangle's and back, which add no area.
#[must_use]
pub fn polygon_rect_overlap(poly: &[Pt], r: &Rect) -> f32 {
    if poly.len() < 3 || r.w <= 0.0 || r.h <= 0.0 {
        return 0.0;
    }
    let (x0, y0, x1, y1) = (r.x, r.y, r.right(), r.bottom());
    let at_x = |x: f32| move |a: Pt, b: Pt| Pt::new(x, a.y + (b.y - a.y) * (x - a.x) / (b.x - a.x));
    let at_y = |y: f32| move |a: Pt, b: Pt| Pt::new(a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y), y);
    let mut out = clip_edge(poly, |p| p.x >= x0, at_x(x0));
    if out.len() >= 3 {
        out = clip_edge(&out, |p| p.x <= x1, at_x(x1));
    }
    if out.len() >= 3 {
        out = clip_edge(&out, |p| p.y >= y0, at_y(y0));
    }
    if out.len() >= 3 {
        out = clip_edge(&out, |p| p.y <= y1, at_y(y1));
    }
    polygon_area(&out).abs()
}

/// The most points [`simplify`] looks at: a longer line is thinned evenly first, so a pen that was
/// left on the glass cannot make the work grow with the square of its length.
pub const MAX_SIMPLIFY_POINTS: usize = 4_000;

/// `points` with the ones that lie within `epsilon` of the line between their neighbours left out
/// (Ramer-Douglas-Peucker, with a stack: a long line does not recurse). The ends stay.
#[must_use]
pub fn simplify(points: &[Pt], epsilon: f32) -> Vec<Pt> {
    let thinned: Vec<Pt>;
    let points = if points.len() > MAX_SIMPLIFY_POINTS {
        let stride = points.len().div_ceil(MAX_SIMPLIFY_POINTS);
        let mut v: Vec<Pt> = points.iter().step_by(stride).copied().collect();
        if let Some(last) = points.last() {
            if v.last() != Some(last) {
                v.push(*last);
            }
        }
        thinned = v;
        &thinned[..]
    } else {
        points
    };
    let n = points.len();
    if n < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0_usize, n - 1)];
    while let Some((lo, hi)) = stack.pop() {
        if hi <= lo + 1 {
            continue;
        }
        let (mut at, mut far) = (lo, 0.0_f32);
        for (i, p) in points.iter().enumerate().take(hi).skip(lo + 1) {
            let d = dist_point_segment(*p, points[lo], points[hi]);
            if d > far {
                far = d;
                at = i;
            }
        }
        if far > epsilon {
            keep[at] = true;
            stack.push((lo, at));
            stack.push((at, hi));
        }
    }
    points.iter().zip(keep).filter_map(|(p, k)| k.then_some(*p)).collect()
}

/// A number as short as it reads back, to two decimals: `10.5`, `3`, `0`.
#[must_use]
pub fn short_number(v: f32) -> String {
    let rounded = if v.is_finite() { (f64::from(v) * 100.0).round() / 100.0 } else { 0.0 };
    let mut s = format!("{rounded:.2}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" {
        s = "0".to_string();
    }
    s
}

/// `poly` as the `d` of an SVG path: lines between its points, closed (`M1 2L10.5 2L10.5 8Z`).
#[must_use]
pub fn svg_path(poly: &[Pt]) -> String {
    let mut d = String::new();
    for (i, p) in poly.iter().enumerate() {
        d.push(if i == 0 { 'M' } else { 'L' });
        d.push_str(&short_number(p.x));
        d.push(' ');
        d.push_str(&short_number(p.y));
    }
    if !poly.is_empty() {
        d.push('Z');
    }
    d
}

/// Which part of the board the window shows: `pan` is the board point at the window's top-left,
/// `zoom` how many window px a board px is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub pan: Pt,
    pub zoom: f32,
}

impl Default for View {
    fn default() -> Self {
        Self { pan: Pt::default(), zoom: 1.0 }
    }
}

impl View {
    /// The board point shown at `screen`.
    #[must_use]
    pub fn to_board(&self, screen: Pt) -> Pt {
        self.pan + screen * (1.0 / self.zoom)
    }

    /// Where the board point `board` is shown.
    #[must_use]
    pub fn to_screen(&self, board: Pt) -> Pt {
        (board - self.pan) * self.zoom
    }

    /// The view zoomed by `factor` (kept in `min`..`max`) around the window point `at`: the board
    /// point under `at` stays under it.
    #[must_use]
    pub fn zoomed_at(&self, at: Pt, factor: f32, min: f32, max: f32) -> Self {
        if !factor.is_finite() || factor <= 0.0 {
            return *self;
        }
        let zoom = (self.zoom * factor).clamp(min, max);
        let under = self.to_board(at);
        Self { pan: under - at * (1.0 / zoom), zoom }
    }
}

#[cfg(test)]
#[path = "whiteboard_geom_test.rs"]
mod whiteboard_geom_test;
