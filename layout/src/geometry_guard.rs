//! Geometry made safe for the AGG rasterizer before it gets there.
//!
//! AGG cannot take two kinds of coordinate. A non-finite one rounds to 0 in
//! its fixed point, and in a curve it keeps the flattener subdividing until
//! its depth limit, 2^33 steps. One far outside the target makes the
//! flattener, the dasher and the cell walker do work in proportion to the
//! distance, and past about eight million pixels the fixed point saturates.
//!
//! So a shape with a non-finite coordinate is dropped, and everything else is
//! clamped to a box, exactly: a segment is cut where it crosses one of the
//! box's lines and its outside pieces are moved onto the box's edge, which
//! leaves the winding number of every point inside the box as it was. A
//! curve reaching outside the box is split until each piece lies inside it
//! (kept as a curve) or wholly beyond one of its edges (replaced by its
//! chord: the piece and its chord enclose no point inside the box). Inside
//! the box nothing changes, and geometry that already fits is passed on
//! vertex for vertex.

use agg_rust::{
    basics::{
        is_close, is_end_poly, is_move_to, is_stop, is_vertex, VertexSource, PATH_CMD_LINE_TO,
        PATH_CMD_MOVE_TO, PATH_CMD_STOP, PATH_FLAGS_NONE,
    },
    path_storage::PathStorage,
};

/// A point in the box's coordinates.
pub(crate) type Point = (f64, f64);

/// How far from the origin geometry may reach before it is clamped, for the
/// paths built ahead of AGG's flattener and dasher: half of AGG's exact
/// fixed-point range, and hundreds of times the side of any mask or pixmap,
/// so nothing that can be on screen is ever clamped. A curve inside it
/// flattens into a few thousand points at most.
pub(crate) const COORD_LIMIT: f64 = 4_194_304.0;

/// The box a `w` x `h` target's fill geometry is clamped to on its way to the
/// rasterizer: far larger than anything on screen, so real geometry is
/// untouched, and inside AGG's exact fixed-point range, so its work stays
/// bounded.
pub(crate) fn target_bounds(w: u32, h: u32) -> ClampBox {
    ClampBox::around_origin((f64::from(w) + f64::from(h)).mul_add(4.0, 4096.0))
}

/// How often a curve reaching outside the box is halved at most, and into
/// how many pieces in all. Only pieces that straddle the box's edge are
/// split, a handful per level, so the depth is what bounds the work for an
/// ordinary curve, and the piece count for a pathological one. A piece left
/// over at either limit is drawn as its chord.
const MAX_SPLIT_DEPTH: u32 = 64;
const MAX_CURVE_PIECES: u32 = 4096;

/// An axis-aligned box that geometry is clamped to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ClampBox {
    /// Its left, top, right and bottom edges.
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl ClampBox {
    /// The box from `-limit` to `limit` on both axes.
    pub(crate) const fn around_origin(limit: f64) -> Self {
        Self {
            min_x: -limit,
            min_y: -limit,
            max_x: limit,
            max_y: limit,
        }
    }

    const fn contains(&self, p: Point) -> bool {
        p.0 >= self.min_x && p.0 <= self.max_x && p.1 >= self.min_y && p.1 <= self.max_y
    }

    const fn clamp(&self, p: Point) -> Point {
        (p.0.clamp(self.min_x, self.max_x), p.1.clamp(self.min_y, self.max_y))
    }

    /// Whether all of `points` lie beyond one and the same edge: then so does
    /// everything in their convex hull, a curve they control included.
    fn all_beyond_one_edge(&self, points: &[Point]) -> bool {
        points.iter().all(|p| p.0 < self.min_x)
            || points.iter().all(|p| p.0 > self.max_x)
            || points.iter().all(|p| p.1 < self.min_y)
            || points.iter().all(|p| p.1 > self.max_y)
    }

    /// The segment from `from` to `to` clamped to the box: the points where
    /// it crosses the box's four lines, in order, then `to`, each clamped.
    /// Between two of them the segment stays on one side of every line, so
    /// the clamped points joined by straight lines ARE the clamped segment.
    /// Fills the front of `out` and returns how many points it wrote.
    fn clamp_segment(&self, from: Point, to: Point, out: &mut [Point; 5]) -> usize {
        let mut crossings = [0.0_f64; 4];
        let mut count = 0;
        for (a, b, line) in [
            (from.0, to.0, self.min_x),
            (from.0, to.0, self.max_x),
            (from.1, to.1, self.min_y),
            (from.1, to.1, self.max_y),
        ] {
            if (a < line && b > line) || (a > line && b < line) {
                // Halved, so that `b - a` cannot overflow near f64::MAX.
                crossings[count] = (line * 0.5 - a * 0.5) / (b * 0.5 - a * 0.5);
                count += 1;
            }
        }
        crossings[..count].sort_by(f64::total_cmp);
        for (slot, t) in out.iter_mut().zip(&crossings[..count]) {
            *slot = self.clamp(lerp(from, to, *t));
        }
        out[count] = self.clamp(to);
        count + 1
    }
}

/// The point `t` of the way from `from` to `to`, without forming `to - from`
/// (which overflows for coordinates near `f64::MAX`).
fn lerp(from: Point, to: Point, t: f64) -> Point {
    (
        from.0 * (1.0 - t) + to.0 * t,
        from.1 * (1.0 - t) + to.1 * t,
    )
}

fn midpoint(a: Point, b: Point) -> Point {
    (a.0 * 0.5 + b.0 * 0.5, a.1 * 0.5 + b.1 * 0.5)
}

/// Builds an AGG path whose geometry is clamped to a box (see the module
/// notes): what the flattener and the dasher are handed is bounded, and
/// nothing inside the box changes. Non-finite points are ignored; a caller
/// that can should drop the whole shape instead, since a shape with a point
/// missing is not the shape either.
pub(crate) struct BoundedPath {
    /// The clamped path.
    out: PathStorage,
    /// The box it is clamped to.
    bounds: ClampBox,
    /// The contour's current point as given, before clamping; `None` before
    /// the first `move_to`.
    current: Option<Point>,
    /// Where the contour began, as given.
    start: Point,
}

impl core::fmt::Debug for BoundedPath {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BoundedPath")
            .field("vertices", &self.out.total_vertices())
            .field("bounds", &self.bounds)
            .field("current", &self.current)
            .field("start", &self.start)
            .finish()
    }
}

impl BoundedPath {
    pub(crate) fn new(bounds: ClampBox) -> Self {
        Self {
            out: PathStorage::new(),
            bounds,
            current: None,
            start: (0.0, 0.0),
        }
    }

    /// The finished path.
    pub(crate) fn finish(self) -> PathStorage {
        self.out
    }

    pub(crate) fn move_to(&mut self, to: Point) {
        if !is_finite(to) {
            return;
        }
        let (x, y) = self.bounds.clamp(to);
        self.out.move_to(x, y);
        self.current = Some(to);
        self.start = to;
    }

    pub(crate) fn line_to(&mut self, to: Point) {
        if !is_finite(to) {
            return;
        }
        let Some(from) = self.current else {
            self.move_to(to);
            return;
        };
        if self.bounds.contains(from) && self.bounds.contains(to) {
            self.out.line_to(to.0, to.1);
        } else {
            let mut points = [(0.0, 0.0); 5];
            let count = self.bounds.clamp_segment(from, to, &mut points);
            for &(x, y) in &points[..count] {
                self.out.line_to(x, y);
            }
        }
        self.current = Some(to);
    }

    pub(crate) fn quad_to(&mut self, ctrl: Point, to: Point) {
        if !(is_finite(ctrl) && is_finite(to)) {
            return;
        }
        let Some(from) = self.current else {
            self.move_to(to);
            return;
        };
        if self.bounds.contains(from) && self.bounds.contains(ctrl) && self.bounds.contains(to) {
            self.out.curve3(ctrl.0, ctrl.1, to.0, to.1);
            self.current = Some(to);
            return;
        }
        // Pieces are taken from the end of the stack, first half first, so
        // each one starts where the previous one ended.
        let mut pending = vec![([from, ctrl, to], 0_u32)];
        let mut pieces = 0_u32;
        while let Some(([p0, p1, p2], depth)) = pending.pop() {
            if self.bounds.contains(p0) && self.bounds.contains(p1) && self.bounds.contains(p2) {
                self.out.curve3(p1.0, p1.1, p2.0, p2.1);
                self.current = Some(p2);
            } else if depth >= MAX_SPLIT_DEPTH
                || pieces >= MAX_CURVE_PIECES
                || self.bounds.all_beyond_one_edge(&[p0, p1, p2])
            {
                self.line_to(p2);
            } else {
                pieces += 1;
                let (p01, p12) = (midpoint(p0, p1), midpoint(p1, p2));
                let p012 = midpoint(p01, p12);
                pending.push(([p012, p12, p2], depth + 1));
                pending.push(([p0, p01, p012], depth + 1));
            }
        }
    }

    pub(crate) fn cubic_to(&mut self, ctrl_1: Point, ctrl_2: Point, to: Point) {
        if !(is_finite(ctrl_1) && is_finite(ctrl_2) && is_finite(to)) {
            return;
        }
        let Some(from) = self.current else {
            self.move_to(to);
            return;
        };
        let inside = |b: &ClampBox, points: &[Point; 4]| points.iter().all(|p| b.contains(*p));
        if inside(&self.bounds, &[from, ctrl_1, ctrl_2, to]) {
            self.out.curve4(ctrl_1.0, ctrl_1.1, ctrl_2.0, ctrl_2.1, to.0, to.1);
            self.current = Some(to);
            return;
        }
        let mut pending = vec![([from, ctrl_1, ctrl_2, to], 0_u32)];
        let mut pieces = 0_u32;
        while let Some((cubic, depth)) = pending.pop() {
            let [p0, p1, p2, p3] = cubic;
            if inside(&self.bounds, &cubic) {
                self.out.curve4(p1.0, p1.1, p2.0, p2.1, p3.0, p3.1);
                self.current = Some(p3);
            } else if depth >= MAX_SPLIT_DEPTH
                || pieces >= MAX_CURVE_PIECES
                || self.bounds.all_beyond_one_edge(&cubic)
            {
                self.line_to(p3);
            } else {
                pieces += 1;
                let (p01, p12, p23) = (midpoint(p0, p1), midpoint(p1, p2), midpoint(p2, p3));
                let (p012, p123) = (midpoint(p01, p12), midpoint(p12, p23));
                let p0123 = midpoint(p012, p123);
                pending.push(([p0123, p123, p23, p3], depth + 1));
                pending.push(([p0, p01, p012, p0123], depth + 1));
            }
        }
    }

    /// Closes the contour along its straight closing edge, clamped like any
    /// other. Only needed where that edge leaves the box: inside it, the edge
    /// the rasterizer closes a contour with is the same line.
    pub(crate) fn close_exactly(&mut self) {
        let Some(from) = self.current else {
            return;
        };
        if from != self.start
            && (!self.bounds.contains(from) || !self.bounds.contains(self.start))
        {
            self.line_to(self.start);
        }
    }

    /// [`Self::close_exactly`], then an explicit close command.
    pub(crate) fn close_polygon(&mut self) {
        if self.current.is_some() {
            self.close_exactly();
            self.out.close_polygon(PATH_FLAGS_NONE);
            self.current = Some(self.start);
        }
    }
}

/// How [`add_svg_rings`] ends each ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RingEnd {
    /// Left open: a stroke draws only the ring's own segments.
    Open,
    /// Closed along its exact closing edge, as a fill closes it.
    Filled,
    /// Closed exactly, and ended with an explicit close command.
    ClosePolygon,
}

/// Adds `path` to `out` ring by ring, each point mapped by `map`. A ring with
/// a non-finite mapped point is left out whole: NaN and infinity are not SVG
/// numbers, and a ring with a point taken out would be another shape.
pub(crate) fn add_svg_rings(
    out: &mut BoundedPath,
    path: &azul_core::svg::SvgMultiPolygon,
    map: &dyn Fn(azul_css::props::basic::SvgPoint) -> Point,
    end: RingEnd,
) {
    use azul_core::svg::SvgPathElement;

    let finite = |p: azul_css::props::basic::SvgPoint| {
        let (x, y) = map(p);
        x.is_finite() && y.is_finite()
    };
    for ring in path.rings.as_ref() {
        let items = ring.items.as_ref();
        let ring_is_finite = items.iter().all(|item| match item {
            SvgPathElement::Line(l) => finite(l.start) && finite(l.end),
            SvgPathElement::QuadraticCurve(q) => {
                finite(q.start) && finite(q.ctrl) && finite(q.end)
            }
            SvgPathElement::CubicCurve(c) => {
                finite(c.start) && finite(c.ctrl_1) && finite(c.ctrl_2) && finite(c.end)
            }
        });
        let Some(first) = items.first() else {
            continue;
        };
        if !ring_is_finite {
            continue;
        }
        out.move_to(map(match first {
            SvgPathElement::Line(l) => l.start,
            SvgPathElement::QuadraticCurve(q) => q.start,
            SvgPathElement::CubicCurve(c) => c.start,
        }));
        for item in items {
            match item {
                SvgPathElement::Line(l) => out.line_to(map(l.end)),
                SvgPathElement::QuadraticCurve(q) => out.quad_to(map(q.ctrl), map(q.end)),
                SvgPathElement::CubicCurve(c) => {
                    out.cubic_to(map(c.ctrl_1), map(c.ctrl_2), map(c.end));
                }
            }
        }
        match end {
            RingEnd::Open => {}
            RingEnd::Filled => out.close_exactly(),
            RingEnd::ClosePolygon => out.close_polygon(),
        }
    }
}

const fn is_finite(p: Point) -> bool {
    p.0.is_finite() && p.1.is_finite()
}

/// A vertex source on its way to the rasterizer, clamped to a box exactly
/// (see the module notes) and with its non-finite vertices taken out: a
/// contour that starts at one is dropped up to the next `move_to`, a
/// non-finite vertex inside a contour is skipped (the edge runs to the next
/// finite one). The rasterizer covers the same pixels inside the box as with
/// the source itself, while its fixed point never saturates and the edges it
/// walks stay bounded.
pub(crate) struct ClampedVertexSource<'a> {
    /// The source.
    inner: &'a mut dyn VertexSource,
    /// The box.
    bounds: ClampBox,
    /// The contour's current point as the source gave it; `None` outside a
    /// contour.
    current: Option<Point>,
    /// Where the contour began, as the source gave it.
    start: Point,
    /// The contour began at a non-finite point and is being dropped.
    skipping: bool,
    /// Vertices made from one source vertex and not handed out yet: the
    /// points where a segment crosses the box's lines, a contour's exact
    /// closing edge. At most six at a time.
    pending: [(f64, f64, u32); 8],
    /// How many of `pending` are queued, and the next to hand out.
    pending_len: usize,
    pending_next: usize,
}

impl core::fmt::Debug for ClampedVertexSource<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ClampedVertexSource")
            .field("bounds", &self.bounds)
            .field("current", &self.current)
            .field("skipping", &self.skipping)
            .finish_non_exhaustive()
    }
}

impl<'a> ClampedVertexSource<'a> {
    pub(crate) fn new(inner: &'a mut dyn VertexSource, bounds: ClampBox) -> Self {
        Self {
            inner,
            bounds,
            current: None,
            start: (0.0, 0.0),
            skipping: false,
            pending: [(0.0, 0.0, PATH_CMD_STOP); 8],
            pending_len: 0,
            pending_next: 0,
        }
    }

    const fn push(&mut self, (x, y): Point, cmd: u32) {
        self.pending[self.pending_len] = (x, y, cmd);
        self.pending_len += 1;
    }

    /// Queues the segment from `from` to `to`, clamped.
    fn push_segment(&mut self, from: Point, to: Point) {
        if self.bounds.contains(from) && self.bounds.contains(to) {
            self.push(to, PATH_CMD_LINE_TO);
        } else {
            let mut points = [(0.0, 0.0); 5];
            let count = self.bounds.clamp_segment(from, to, &mut points);
            for &point in &points[..count] {
                self.push(point, PATH_CMD_LINE_TO);
            }
        }
    }

    /// Queues the contour's closing edge where it leaves the box. The
    /// rasterizer closes every contour itself, with a straight line between
    /// the CLAMPED points, which is the same edge only inside the box.
    fn push_close(&mut self) {
        if let Some(from) = self.current {
            if from != self.start
                && (!self.bounds.contains(from) || !self.bounds.contains(self.start))
            {
                self.push_segment(from, self.start);
            }
        }
    }

    /// Turns one source vertex into the vertices queued for the rasterizer.
    fn queue(&mut self, point: Point, cmd: u32) {
        let finite = is_finite(point);
        if is_stop(cmd) {
            self.push_close();
            self.current = None;
            self.push((0.0, 0.0), PATH_CMD_STOP);
        } else if is_move_to(cmd) {
            self.push_close();
            self.skipping = !finite;
            if finite {
                self.push(self.bounds.clamp(point), PATH_CMD_MOVE_TO);
                self.current = Some(point);
                self.start = point;
            } else {
                self.current = None;
            }
        } else if is_vertex(cmd) {
            if self.skipping || !finite {
                return;
            }
            if let Some(from) = self.current {
                self.push_segment(from, point);
            } else {
                // A line with no contour to continue: the rasterizer draws it
                // from wherever it stands, as it would have unclamped.
                self.push(self.bounds.clamp(point), PATH_CMD_LINE_TO);
                self.start = point;
            }
            self.current = Some(point);
        } else if is_end_poly(cmd) {
            if self.skipping {
                return;
            }
            if is_close(cmd) {
                self.push_close();
                if self.current.is_some() {
                    self.current = Some(self.start);
                }
            }
            self.push(point, cmd);
        } else {
            self.push(point, cmd);
        }
    }
}

impl VertexSource for ClampedVertexSource<'_> {
    fn rewind(&mut self, path_id: u32) {
        self.inner.rewind(path_id);
        self.current = None;
        self.skipping = false;
        self.pending_len = 0;
        self.pending_next = 0;
    }

    fn vertex(&mut self, x: &mut f64, y: &mut f64) -> u32 {
        loop {
            if self.pending_next < self.pending_len {
                let (px, py, cmd) = self.pending[self.pending_next];
                self.pending_next += 1;
                *x = px;
                *y = py;
                return cmd;
            }
            self.pending_len = 0;
            self.pending_next = 0;
            let (mut vx, mut vy) = (0.0, 0.0);
            let cmd = self.inner.vertex(&mut vx, &mut vy);
            self.queue((vx, vy), cmd);
        }
    }
}

#[cfg(test)]
mod tests {
    use agg_rust::{
        basics::{FillingRule, PATH_CMD_END_POLY, PATH_FLAGS_CLOSE},
        color::Rgba8,
        conv_curve::ConvCurve,
        pixfmt_rgba::PixfmtRgba32,
        rasterizer_scanline_aa::RasterizerScanlineAa,
        renderer_base::RendererBase,
        renderer_scanline::render_scanlines_aa_solid,
        rendering_buffer::RowAccessor,
        scanline_u::ScanlineU8,
    };

    use super::*;

    /// The side of the test raster, in pixels.
    const SIDE: u32 = 64;

    /// Runs `work` on its own thread, failing the test when it is still
    /// running after `seconds`: an unguarded input spins for minutes.
    fn finishes_within<T: Send + 'static>(
        seconds: u64,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(work());
        });
        rx.recv_timeout(std::time::Duration::from_secs(seconds))
            .expect("the work finishes (and does not panic)")
    }

    /// The coverage `path` paints into a SIDE x SIDE raster, one byte per
    /// pixel, rasterized the way the mask and pixmap fills do: clipped to the
    /// raster, and through `ClampedVertexSource` when `limit` is given.
    fn coverage(path: &mut dyn VertexSource, limit: Option<f64>) -> Vec<u8> {
        let mut rgba = vec![0_u8; (SIDE * SIDE * 4) as usize];
        {
            // SAFETY: `rgba` holds SIDE rows of SIDE * 4 bytes and outlives `ra`.
            let mut ra = unsafe {
                RowAccessor::new_with_buf(rgba.as_mut_ptr(), SIDE, SIDE, (SIDE * 4) as i32)
            };
            let mut rb = RendererBase::new(PixfmtRgba32::new(&mut ra));
            let mut ras = RasterizerScanlineAa::new();
            ras.filling_rule(FillingRule::NonZero);
            ras.clip_box(-2.0, -2.0, f64::from(SIDE) + 2.0, f64::from(SIDE) + 2.0);
            match limit {
                Some(limit) => {
                    let mut clamped =
                        ClampedVertexSource::new(path, ClampBox::around_origin(limit));
                    ras.add_path(&mut clamped, 0);
                }
                None => ras.add_path(path, 0),
            }
            let mut sl = ScanlineU8::new();
            let white = Rgba8::new(255, 255, 255, 255);
            render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &white);
        }
        rgba.chunks_exact(4).map(|px| px[3]).collect()
    }

    fn polygon(points: &[Point]) -> PathStorage {
        let mut path = PathStorage::new();
        path.move_to(points[0].0, points[0].1);
        for p in &points[1..] {
            path.line_to(p.0, p.1);
        }
        path.close_polygon(PATH_FLAGS_NONE);
        path
    }

    fn vertices(path: &PathStorage) -> Vec<(f64, f64, u32)> {
        (0..path.total_vertices())
            .map(|i| {
                let (mut x, mut y) = (0.0, 0.0);
                let cmd = path.vertex_idx(i, &mut x, &mut y);
                (x, y, cmd)
            })
            .collect()
    }

    #[test]
    fn a_segment_leaving_the_box_is_cut_where_it_crosses_the_box() {
        let mut out = [(0.0, 0.0); 5];
        let n = ClampBox::around_origin(10.0).clamp_segment((-20.0, 0.0), (20.0, 5.0), &mut out);
        assert_eq!(&out[..n], &[(-10.0, 1.25), (10.0, 3.75), (10.0, 5.0)]);
    }

    #[test]
    fn geometry_inside_the_box_is_passed_on_vertex_for_vertex() {
        let mut bounded = BoundedPath::new(ClampBox::around_origin(COORD_LIMIT));
        let mut plain = PathStorage::new();
        bounded.move_to((1.0, 2.0));
        plain.move_to(1.0, 2.0);
        bounded.line_to((30.0, 2.0));
        plain.line_to(30.0, 2.0);
        bounded.quad_to((40.0, 20.0), (30.0, 40.0));
        plain.curve3(40.0, 20.0, 30.0, 40.0);
        bounded.cubic_to((20.0, 50.0), (5.0, 50.0), (1.0, 30.0));
        plain.curve4(20.0, 50.0, 5.0, 50.0, 1.0, 30.0);
        bounded.close_polygon();
        plain.close_polygon(PATH_FLAGS_NONE);
        assert_eq!(vertices(&bounded.finish()), vertices(&plain));
    }

    // A transform of scale(1e9) puts a curve's control points a billion
    // billion pixels out. Flattened whole that is two billion points;
    // clamped, the pieces beyond the box are chords and the rest is small.
    #[test]
    fn a_curve_reaching_far_outside_the_box_flattens_into_few_points() {
        let points = finishes_within(30, || {
            let mut bounded = BoundedPath::new(ClampBox::around_origin(COORD_LIMIT));
            bounded.move_to((0.0, 0.0));
            bounded.cubic_to((1e18, 1e18), (-1e18, 1e18), (10.0, 0.0));
            let mut path = bounded.finish();
            let mut flat = ConvCurve::new(&mut path);
            flat.rewind(0);
            let (mut x, mut y) = (0.0, 0.0);
            let mut n = 0_usize;
            while !is_stop(flat.vertex(&mut x, &mut y)) {
                assert!(x.abs() <= COORD_LIMIT && y.abs() <= COORD_LIMIT, "({x}, {y})");
                n += 1;
            }
            n
        });
        assert!(points < 100_000, "{points} points");
    }

    // What the clamp promises: inside the box the rasterizer paints exactly
    // what it paints for the geometry itself. These edges run out to four
    // million pixels, still inside AGG's exact range, and come back.
    #[test]
    fn clamping_changes_no_pixel_inside_the_box() {
        for points in [
            vec![(10.0, 10.0), (4e6, 30.0), (10.0, 50.0)],
            vec![(-4e6, -3e6), (60.0, 5.0), (3e6, 4e6)],
            vec![(50.0, 50.0), (4e6, 60.0), (3.9e6, -4e6), (20.0, 5.0)],
        ] {
            let exact = coverage(&mut polygon(&points), None);
            let clamped = coverage(&mut polygon(&points), Some(1000.0));
            assert_eq!(clamped, exact, "{points:?}");
            assert!(exact.iter().any(|&a| a > 0), "the shape is on the raster: {points:?}");
        }
    }

    // NaN rounds to 0 in AGG's fixed point: a vertex at NaN pulls the shape
    // to the origin. The clamp skips it instead.
    #[test]
    fn a_nan_vertex_is_skipped() {
        let triangle = [(10.0, 10.0), (60.0, 10.0), (30.0, 60.0)];
        let exact = coverage(&mut polygon(&triangle), None);
        let nan = f64::NAN;
        let with_nan = [(10.0, 10.0), (nan, nan), (60.0, 10.0), (30.0, 60.0)];
        assert_eq!(coverage(&mut polygon(&with_nan), Some(1000.0)), exact);
    }

    #[test]
    fn a_contour_starting_at_nan_is_dropped() {
        let mut path = polygon(&[(f64::NAN, 0.0), (50.0, 50.0), (60.0, 10.0)]);
        let triangle = [(10.0, 10.0), (60.0, 10.0), (30.0, 60.0)];
        path.concat_path(&mut polygon(&triangle), 0);
        assert_eq!(
            coverage(&mut path, Some(1000.0)),
            coverage(&mut polygon(&triangle), None),
            "only the finite contour is painted"
        );
    }

    fn line(from: (f32, f32), to: (f32, f32)) -> azul_core::svg::SvgPathElement {
        let point = |(x, y)| azul_css::props::basic::SvgPoint { x, y };
        azul_core::svg::SvgPathElement::Line(azul_core::svg::SvgLine {
            start: point(from),
            end: point(to),
        })
    }

    fn shape(rings: Vec<Vec<azul_core::svg::SvgPathElement>>) -> azul_core::svg::SvgMultiPolygon {
        azul_core::svg::SvgMultiPolygon {
            rings: azul_core::svg::SvgPathVec::from_vec(
                rings
                    .into_iter()
                    .map(|items| azul_core::svg::SvgPath {
                        items: azul_core::svg::SvgPathElementVec::from_vec(items),
                    })
                    .collect(),
            ),
        }
    }

    fn built(path: &azul_core::svg::SvgMultiPolygon, end: RingEnd) -> PathStorage {
        let mut out = BoundedPath::new(ClampBox::around_origin(COORD_LIMIT));
        add_svg_rings(&mut out, path, &|p| (f64::from(p.x), f64::from(p.y)), end);
        out.finish()
    }

    #[test]
    fn a_ring_with_a_non_finite_point_is_left_out_whole() {
        let nan = f32::NAN;
        let finite_ring = vec![line((20.0, 20.0), (30.0, 20.0)), line((30.0, 20.0), (25.0, 30.0))];
        let with_nan = shape(vec![
            vec![
                line((0.0, 0.0), (10.0, 0.0)),
                line((10.0, 0.0), (nan, 5.0)),
                line((nan, 5.0), (0.0, 0.0)),
            ],
            finite_ring.clone(),
        ]);
        assert_eq!(
            vertices(&built(&with_nan, RingEnd::Filled)),
            vertices(&built(&shape(vec![finite_ring]), RingEnd::Filled))
        );
    }

    // A fill closes a ring with a straight edge from its last point to its
    // first. Where that edge leaves the box, the edge the rasterizer draws
    // between the CLAMPED points would cut through the box somewhere else,
    // so the builder adds the exact one.
    #[test]
    fn a_filled_ring_is_closed_along_its_exact_closing_edge() {
        let far = COORD_LIMIT as f32 * 4.0;
        let ring = shape(vec![vec![line((0.0, 0.0), (far, far * 0.5))]]);
        let open = vertices(&built(&ring, RingEnd::Open));
        let filled = vertices(&built(&ring, RingEnd::Filled));
        let limit = COORD_LIMIT;
        assert_eq!(
            &filled[open.len()..],
            &[
                (limit, limit, PATH_CMD_LINE_TO),
                (limit, limit * 0.5, PATH_CMD_LINE_TO),
                (0.0, 0.0, PATH_CMD_LINE_TO),
            ],
            "the closing edge runs back down the line y = x / 2"
        );
    }

    #[test]
    fn a_closing_command_is_passed_on_after_the_exact_closing_edge() {
        let mut path = polygon(&[(10.0, 10.0), (4e6, 20.0), (10.0, 40.0)]);
        let mut clamped = ClampedVertexSource::new(&mut path, ClampBox::around_origin(100.0));
        clamped.rewind(0);
        let (mut x, mut y) = (0.0, 0.0);
        let mut commands = Vec::new();
        loop {
            let cmd = clamped.vertex(&mut x, &mut y);
            if is_stop(cmd) {
                break;
            }
            assert!(x.abs() <= 100.0 && y.abs() <= 100.0, "({x}, {y})");
            commands.push(cmd);
        }
        assert_eq!(commands.first(), Some(&PATH_CMD_MOVE_TO));
        assert_eq!(commands.last(), Some(&(PATH_CMD_END_POLY | PATH_FLAGS_CLOSE)));
    }
}
