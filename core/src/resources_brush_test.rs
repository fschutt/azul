//! The CPU brush: `RawImage::paint_dot` and `RawImage::paint_stroke`.
//!
//! Three kinds of test, none of them against a clock:
//!
//! * the WORK it does, counted through the [`BrushMeter`] seam (dabs, rows, pixels tested, square roots taken,
//!   pixels composited): a stroke off the image stamps no dab, a dab tests the pixels of its box and takes a
//!   square root only where the soft edge needs a distance;
//! * what it PAINTS, against the algorithm it replaced, kept here verbatim as the reference ([`reference_dot`],
//!   [`reference_stroke`]): the same seeded strokes painted by both must agree byte for byte, and where the
//!   platform's `hypotf` is not the one the new code computes, within the tolerance stated at
//!   [`the_new_brush_paints_what_the_old_one_painted`];
//! * the TIME, measured by hand ([`how_long_the_brush_takes`], ignored: a timing is the machine's).

use super::*;

// ---------------------------------------------------------------------------------------------------------
// The meter of the tests, and the algorithm of before
// ---------------------------------------------------------------------------------------------------------

/// What a brush did, counted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Counts {
    dabs: u64,
    rows: u64,
    pixels: u64,
    roots: u64,
    blends: u64,
}

impl BrushMeter for Counts {
    fn dab(&mut self) {
        self.dabs += 1;
    }
    fn row(&mut self) {
        self.rows += 1;
    }
    fn pixel(&mut self) {
        self.pixels += 1;
    }
    fn root(&mut self) {
        self.roots += 1;
    }
    fn blend(&mut self) {
        self.blends += 1;
    }
}

/// `RawImage::paint_dot` as it was before the squared distances: the whole box of the dab is walked and every
/// pixel of it takes a `hypotf`. Verbatim (the meter calls are the only addition), the reference of the tests.
fn reference_dot<M: BrushMeter>(img: &mut RawImage, cx: f32, cy: f32, brush: Brush, meter: &mut M) {
    let r = brush.radius;
    if !(r > 0.0) || img.width == 0 || img.height == 0 {
        return;
    }
    let bgr = match img.data_format {
        RawImageFormat::RGBA8 => false,
        RawImageFormat::BGRA8 => true,
        _ => return,
    };
    let (w, h) = (img.width as i32, img.height as i32);
    let buf: &mut [u8] = match img.pixels {
        RawImageData::U8(ref mut v) => v.as_mut(),
        _ => return,
    };
    meter.dab();
    let flow = brush.flow.clamp(0.0, 1.0) * (f32::from(brush.color.a) / 255.0);
    let (cr, cg, cb) = (
        f32::from(brush.color.r),
        f32::from(brush.color.g),
        f32::from(brush.color.b),
    );
    let x0 = (cx - r).floor().max(0.0) as i32;
    let y0 = (cy - r).floor().max(0.0) as i32;
    let x1 = ((cx + r).ceil() as i32).min(w);
    let y1 = ((cy + r).ceil() as i32).min(h);
    for y in y0..y1 {
        meter.row();
        for x in x0..x1 {
            meter.pixel();
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let dist = dx.hypot(dy);
            meter.root();
            if dist > r {
                continue;
            }
            let a = brush_dab_coverage(dist / r, brush.hardness) * flow;
            if a <= 0.0 {
                continue;
            }
            let idx = ((y * w + x) as usize) * 4;
            if idx + 4 > buf.len() {
                continue;
            }
            let (ri, gi, bi, ai) = if bgr {
                (idx + 2, idx + 1, idx, idx + 3)
            } else {
                (idx, idx + 1, idx + 2, idx + 3)
            };
            meter.blend();
            let inv = 1.0 - a;
            buf[ri] = (cr * a + f32::from(buf[ri]) * inv)
                .round()
                .clamp(0.0, 255.0) as u8;
            buf[gi] = (cg * a + f32::from(buf[gi]) * inv)
                .round()
                .clamp(0.0, 255.0) as u8;
            buf[bi] = (cb * a + f32::from(buf[bi]) * inv)
                .round()
                .clamp(0.0, 255.0) as u8;
            buf[ai] = ((a + (f32::from(buf[ai]) / 255.0) * inv) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
}

/// `RawImage::paint_stroke` as it was before the clipping: a dab at every step along the whole segment, on the
/// image or not. Verbatim, the reference of the tests.
fn reference_stroke<M: BrushMeter>(
    img: &mut RawImage,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    brush: Brush,
    meter: &mut M,
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = dx.hypot(dy);
    if !len.is_finite() {
        return;
    }
    let step = (brush.radius * brush.spacing.max(0.01)).max(0.5);
    let n = (len / step).floor() as i32;
    if n <= 0 {
        reference_dot(img, x1, y1, brush, meter);
        return;
    }
    for i in 0..=n {
        let t = i as f32 / n as f32;
        reference_dot(img, x0 + dx * t, y0 + dy * t, brush, meter);
    }
}

// ---------------------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------------------

/// A seeded generator (SplitMix64): the strokes of a test are the same on every machine and every run.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// In `[0, 1)`.
    fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / 16_777_216.0
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }
    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

fn image(width: usize, height: usize, format: RawImageFormat, mut byte: impl FnMut() -> u8) -> RawImage {
    let bytes: Vec<u8> = (0..width * height * 4).map(|_| byte()).collect();
    RawImage {
        pixels: RawImageData::U8(bytes.into()),
        width,
        height,
        premultiplied_alpha: true,
        data_format: format,
        tag: Vec::new().into(),
    }
}

fn blank(width: usize, height: usize) -> RawImage {
    image(width, height, RawImageFormat::RGBA8, || 0)
}

fn bytes_of(img: &RawImage) -> Vec<u8> {
    img.pixels.get_u8_vec_ref().expect("a u8 image").as_ref().to_vec()
}

fn red(alpha: u8) -> ColorU {
    ColorU { r: 220, g: 30, b: 40, a: alpha }
}

fn brush(radius: f32, hardness: f32) -> Brush {
    Brush { color: red(255), radius, hardness, flow: 1.0, spacing: 0.5 }
}

/// How two renderings differ.
#[derive(Debug, Default, Clone, Copy)]
struct Diff {
    bytes: usize,
    pixels: usize,
    max: u8,
}

fn diff(a: &[u8], b: &[u8]) -> Diff {
    assert_eq!(a.len(), b.len());
    let mut d = Diff::default();
    for (pa, pb) in a.chunks(4).zip(b.chunks(4)) {
        let mut any = false;
        for (x, y) in pa.iter().zip(pb) {
            if x != y {
                d.bytes += 1;
                d.max = d.max.max(x.abs_diff(*y));
                any = true;
            }
        }
        d.pixels += usize::from(any);
    }
    d
}

/// Whether this platform's `hypotf` is the correctly rounded hypotenuse the new code computes in double
/// precision (`sqrt` of the sum of the squares in `f64`, rounded once to `f32`): glibc's is, `libm`'s and other
/// C libraries' are allowed to be an ulp off now and then. Where it is, the old and the new brush must agree on
/// every byte; where it is not, a distance that rounds differently may move a coverage by an ulp.
fn hypotf_is_correctly_rounded() -> bool {
    let mut rng = Rng(0x00C0_FFEE);
    (0..200_000).all(|_| {
        let (a, b) = (rng.range(-40.0, 40.0), rng.range(-40.0, 40.0));
        let wide = (f64::from(a) * f64::from(a) + f64::from(b) * f64::from(b)).sqrt() as f32;
        a.hypot(b) == wide
    })
}

// ---------------------------------------------------------------------------------------------------------
// The work
// ---------------------------------------------------------------------------------------------------------

fn counted_stroke(img: &mut RawImage, seg: (f32, f32, f32, f32), b: Brush) -> Counts {
    let mut counts = Counts::default();
    img.paint_stroke_metered(seg.0, seg.1, seg.2, seg.3, b, &mut counts);
    counts
}

fn reference_counted_stroke(img: &mut RawImage, seg: (f32, f32, f32, f32), b: Brush) -> Counts {
    let mut counts = Counts::default();
    reference_stroke(img, seg.0, seg.1, seg.2, seg.3, b, &mut counts);
    counts
}

/// A segment that lies off the image is not walked: the brush stamps no dab, tests no pixel and paints nothing,
/// whatever its length and whichever side it lies on. Before, every dab along the whole segment was stamped (and
/// turned away by its box): 1251 of them for 5,000 px with a radius of 8 and half a radius between dabs.
#[test]
fn a_segment_off_the_image_costs_no_dab_and_no_pixel() {
    let b = brush(8.0, 0.6);
    // The image is 200 x 100: the segments run along the right, the left, above and below it, and one slants
    // away from a corner.
    let segments = [
        (1000.0, 40.0, 6000.0, 40.0),
        (-6000.0, 40.0, -1000.0, 40.0),
        (20.0, -6000.0, 30.0, -1000.0),
        (20.0, 1000.0, 30.0, 6000.0),
        (300.0, 300.0, 4000.0, 3000.0),
        (-50.0, -50.0, -2000.0, -3000.0),
    ];
    for seg in segments {
        let mut img = blank(200, 100);
        let counts = counted_stroke(&mut img, seg, b);
        assert_eq!(
            counts,
            Counts::default(),
            "the segment {seg:?} lies off the image: nothing is stamped, tested or composited"
        );
        assert!(bytes_of(&img).iter().all(|byte| *byte == 0), "{seg:?} painted");
    }

    // What the same segment cost before: a dab every 4 px (spacing 0.5 of a radius of 8), 5,000 px.
    let mut img = blank(200, 100);
    let before = reference_counted_stroke(&mut img, segments[0], b);
    assert_eq!(before.dabs, 1251, "the old walk stamped a dab along the whole segment");
    assert_eq!(before.pixels, 0, "and its boxes were empty");
    eprintln!(
        "BRUSH_WORK off-image segment of 5000 px, radius 8: dabs {} -> 0",
        before.dabs
    );
}

/// A segment that crosses the image stamps exactly the dabs whose box touches it: no dab of the part off the
/// image, and every dab of the part in view.
#[test]
fn a_segment_that_crosses_the_image_stamps_the_dabs_that_reach_it_and_no_other() {
    let b = brush(4.0, 0.6);
    // Along y = 50 of a 200 x 100 image, from 3,000 px left of it to 3,000 px right of it; and a slanting one.
    let segments: [(f32, f32, f32, f32); 4] = [
        (-3000.0, 50.0, 3000.0, 50.0),
        (-3000.0, -1500.0, 3000.0, 1500.0),
        (-2.0, 50.0, 4000.0, 50.0),
        (150.0, 50.0, -4000.0, 50.0),
    ];
    for seg in segments {
        let (x0, y0, x1, y1) = seg;
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len = dx.hypot(dy);
        let n = (len / (b.radius * b.spacing).max(0.5)).floor() as i32;
        // The dabs whose box is not empty, by the rule of the old code, one by one.
        let reaching = (0..=n)
            .filter(|i| {
                let t = *i as f32 / n as f32;
                let (cx, cy) = (x0 + dx * t, y0 + dy * t);
                let lo_x = (cx - b.radius).floor().max(0.0) as i32;
                let hi_x = ((cx + b.radius).ceil() as i32).min(200);
                let lo_y = (cy - b.radius).floor().max(0.0) as i32;
                let hi_y = ((cy + b.radius).ceil() as i32).min(100);
                lo_x < hi_x && lo_y < hi_y
            })
            .count() as u64;
        assert!(reaching > 0, "{seg:?}: the test segment must reach the image");

        let mut new = blank(200, 100);
        let counts = counted_stroke(&mut new, seg, b);
        assert_eq!(
            counts.dabs, reaching,
            "{seg:?}: the dabs stamped are the dabs whose box reaches the image"
        );

        // And the picture is the one the old walk made.
        let mut old = blank(200, 100);
        let before = reference_counted_stroke(&mut old, seg, b);
        assert_eq!(bytes_of(&new), bytes_of(&old), "{seg:?}: the same picture");
        assert_eq!(before.dabs, n as u64 + 1, "{seg:?}: the old walk stamped every dab");
        eprintln!(
            "BRUSH_WORK segment {seg:?}: dabs {} -> {} ({} px tested -> {})",
            before.dabs, counts.dabs, before.pixels, counts.pixels
        );
    }
}

/// A dab off the image costs nothing at all, not even a row.
#[test]
fn a_dab_off_the_image_costs_nothing() {
    for (cx, cy) in [(-100.0, -100.0), (500.0, 20.0), (20.0, 500.0), (-9.0, 50.0), (209.0, 50.0)] {
        let mut img = blank(200, 100);
        let mut counts = Counts::default();
        img.paint_dot_metered(cx, cy, brush(8.0, 0.6), &mut counts);
        assert_eq!(counts.pixels, 0, "({cx}, {cy})");
        assert_eq!(counts.rows, 0);
        assert_eq!(counts.roots, 0);
        assert_eq!(counts.blends, 0);
        assert!(bytes_of(&img).iter().all(|byte| *byte == 0));

        // A stroke of one dab (a segment shorter than a step) is turned away before the dab is stamped.
        let mut dot = blank(200, 100);
        let counts = counted_stroke(&mut dot, (cx, cy, cx + 0.1, cy), brush(8.0, 0.6));
        assert_eq!(counts, Counts::default(), "({cx}, {cy}) as a stroke of one dab");
    }
}

/// The cost of a dab on the image: the pixels of its box are tested (the box of a dab is a few percent more
/// than the disc), and a square root is taken only for a pixel on the soft edge - not for a pixel outside the
/// disc, not for one in the hard core, and for none at all when the edge is hard. Before, every pixel of the
/// box took a `hypotf` call.
#[test]
fn a_dab_takes_a_square_root_only_where_the_soft_edge_needs_one() {
    let pi = core::f64::consts::PI;
    for radius in [1.0_f32, 3.0, 8.0, 20.0] {
        for hardness in [0.0_f32, 0.5, 0.6, 0.85, 1.0] {
            let b = brush(radius, hardness);
            let r = f64::from(radius);
            let (cx, cy) = (50.3, 50.7);

            let mut img = blank(100, 100);
            let mut counts = Counts::default();
            img.paint_dot_metered(cx, cy, b, &mut counts);

            let box_side = f64::from(2.0 * radius + 2.0);
            assert!(
                counts.pixels as f64 <= box_side * box_side,
                "r {radius}, hardness {hardness}: {} pixels tested for a box of at most {box_side}^2",
                counts.pixels
            );
            // The soft edge: the ring between the hard core and the rim (the pixels on both circles add a
            // perimeter's worth).
            let ring = pi * r * r * (1.0 - f64::from(hardness) * f64::from(hardness)) + 2.0 * pi * r + 8.0;
            assert!(
                counts.roots as f64 <= ring,
                "r {radius}, hardness {hardness}: {} square roots for a soft edge of {ring:.0} pixels",
                counts.roots
            );
            if hardness >= 1.0 {
                assert!(counts.roots <= 4, "a hard edge needs no distance but at its very rim: {}", counts.roots);
            }
            assert!(counts.blends > 0 && counts.blends <= counts.pixels);

            // And what it replaced: a square root for every pixel of the box.
            let mut old_img = blank(100, 100);
            let mut old = Counts::default();
            reference_dot(&mut old_img, cx, cy, b, &mut old);
            assert_eq!(old.roots, old.pixels, "the old dab took a hypotf for every pixel of its box");
            assert!(counts.pixels <= old.pixels, "the new dab tests no more pixels than the old one did");
            assert!(counts.roots < old.roots || old.roots <= 4, "r {radius}, hardness {hardness}");
            eprintln!(
                "BRUSH_WORK dab r {radius} hardness {hardness}: pixels {} -> {}, roots {} -> {}, composited {}",
                old.pixels, counts.pixels, old.roots, counts.roots, counts.blends
            );
        }
    }
}

/// The pixels a dab tests grow with its area (its box, a few percent more than the disc): twice the radius,
/// four times the pixels.
#[test]
fn the_pixels_a_dab_tests_grow_with_its_area() {
    let tested = |radius: f32| {
        let mut img = blank(400, 400);
        let mut counts = Counts::default();
        img.paint_dot_metered(200.4, 200.6, brush(radius, 0.6), &mut counts);
        counts.pixels as f64
    };
    for radius in [5.0_f32, 10.0] {
        let ratio = tested(2.0 * radius) / tested(radius);
        assert!((3.2..=4.8).contains(&ratio), "radius {radius} -> {}: {ratio}", 2.0 * radius);
    }
}

/// A segment with coordinates so large that the old walk would have stamped two billion dabs ends at once: only
/// the dabs that can reach the image are walked.
#[test]
fn a_segment_with_huge_finite_coordinates_is_walked_only_where_it_meets_the_image() {
    let b = brush(8.0, 0.6);
    let segments = [
        (0.0, 0.0, 1.0e30, 1.0e30),
        (-1.0e30, -1.0e30, 1.0e30, 1.0e30),
        (1.0e38, 0.0, -1.0e38, 0.0),
        (30.0, 30.0, -3.0e38, 2.0e38),
        (-1.0e9, 50.0, 1.0e9, 50.0),
    ];
    for seg in segments {
        let mut img = blank(200, 100);
        let counts = counted_stroke(&mut img, seg, b);
        assert!(counts.dabs <= 2_000, "{seg:?}: {} dabs", counts.dabs);
    }
    // One that is not finite is not walked at all (it never was), nor is one that is NaN.
    for seg in [
        (0.0, 0.0, f32::INFINITY, 0.0),
        (f32::NAN, 0.0, 1.0, 1.0),
        (0.0, f32::NEG_INFINITY, 1.0, 1.0),
        (-3.0e38, 0.0, 3.0e38, 0.0),
    ] {
        let mut img = blank(200, 100);
        assert_eq!(counted_stroke(&mut img, seg, b), Counts::default(), "{seg:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------
// The picture
// ---------------------------------------------------------------------------------------------------------

/// One seeded case: an image and strokes, to be painted by both algorithms.
struct Case {
    width: usize,
    height: usize,
    format: RawImageFormat,
    noise: bool,
    strokes: Vec<(f32, f32, f32, f32, Brush)>,
}

fn random_brush(rng: &mut Rng, hard: bool) -> Brush {
    let radius = if rng.chance(70) {
        rng.pick(&[0.3, 0.5, 0.9, 1.0, 1.5, 2.0, 2.5, 3.0, 4.7, 6.0, 8.0, 12.5])
    } else {
        rng.range(0.2, 20.0)
    };
    let hardness = if hard {
        rng.pick(&[1.0, 1.0, 1.5])
    } else if rng.chance(60) {
        rng.pick(&[0.0, 0.25, 0.5, 0.6, 0.85, 0.99])
    } else {
        rng.range(-0.2, 0.995)
    };
    let (flow, alpha) = if hard {
        (1.0, 255)
    } else {
        (
            if rng.chance(60) { 1.0 } else { rng.range(0.05, 1.0) },
            if rng.chance(70) { 255 } else { rng.below(256) as u8 },
        )
    };
    Brush {
        color: ColorU {
            r: rng.below(256) as u8,
            g: rng.below(256) as u8,
            b: rng.below(256) as u8,
            a: alpha,
        },
        radius,
        hardness,
        flow,
        spacing: rng.pick(&[0.05, 0.1, 0.2, 0.25, 0.5, 1.0]),
    }
}

fn random_case(seed: u64, hard: bool) -> Case {
    let mut rng = Rng(seed);
    let width = 1 + rng.below(56);
    let height = 1 + rng.below(40);
    let (w, h) = (width as f32, height as f32);
    let format = if rng.chance(50) { RawImageFormat::BGRA8 } else { RawImageFormat::RGBA8 };
    let mut strokes = Vec::new();
    let mut b = random_brush(&mut rng, hard);
    for _ in 0..1 + rng.below(4) {
        if rng.chance(40) {
            b = random_brush(&mut rng, hard);
        }
        let inside = |rng: &mut Rng| (rng.range(0.0, w), rng.range(0.0, h));
        let anywhere = |rng: &mut Rng| (rng.range(-2.0 * w, 3.0 * w), rng.range(-2.0 * h, 3.0 * h));
        let (a, c) = match rng.below(10) {
            // both ends on the image
            0..=2 => (inside(&mut rng), inside(&mut rng)),
            // one end on the image, one off it
            3..=5 => (inside(&mut rng), anywhere(&mut rng)),
            // both ends anywhere, near or far: crossing, touching or missing the image
            6 | 7 => (anywhere(&mut rng), anywhere(&mut rng)),
            // far away on both sides: a long segment that crosses the image or passes it
            8 => (
                (rng.range(-3000.0, -50.0), rng.range(-3000.0, 3000.0)),
                (rng.range(50.0, 3000.0), rng.range(-3000.0, 3000.0)),
            ),
            // a dab, or a stroke shorter than a step, on pixel centres and corners
            _ => {
                let p = (rng.below(width + 1) as f32 + 0.5 * rng.below(2) as f32, rng.below(height + 1) as f32 + 0.5 * rng.below(2) as f32);
                if rng.chance(50) { (p, p) } else { (p, (p.0 + rng.range(-0.2, 0.2), p.1 + rng.range(-0.2, 0.2))) }
            }
        };
        strokes.push((a.0, a.1, c.0, c.1, b));
    }
    Case { width, height, format, noise: rng.chance(50), strokes }
}

/// Paints a case by the new brush and by the old, returns both pictures and the pixels the strokes changed.
fn paint_both(case: &Case, seed: u64) -> (Vec<u8>, Vec<u8>, usize) {
    let mut noise = Rng(seed ^ 0xDEAD_BEEF);
    let mut a = image(case.width, case.height, case.format, || if case.noise { noise.below(256) as u8 } else { 0 });
    let mut b = a.clone();
    let start = bytes_of(&a);
    for &(x0, y0, x1, y1, brush) in &case.strokes {
        a.paint_stroke(x0, y0, x1, y1, brush);
        reference_stroke(&mut b, x0, y0, x1, y1, brush, &mut NoMeter);
    }
    let (new, old) = (bytes_of(&a), bytes_of(&b));
    let touched = diff(&start, &old).pixels;
    (new, old, touched)
}

/// The new brush paints what the old one painted.
///
/// The same seeded strokes - on the image, half on it, off it, far off it, dots, on pixel centres - with seeded
/// brushes (radii from 0.2 to 20, every hardness, partial flow and alpha, both byte orders, noise under the
/// stroke or none) painted by [`reference_stroke`] and by `RawImage::paint_stroke`.
///
/// What may differ, and by how much. The new brush computes a distance as `sqrt(dx * dx + dy * dy)` in double
/// precision rounded once to `f32`; the old one called `hypotf`. Where the platform's `hypotf` is the correctly
/// rounded one (glibc's is: it is that expression) the two are the same number and EVERY BYTE agrees. Where it is
/// not (another C library, or `libm` without std), a distance may differ by one ulp (6e-8 relative); that moves
/// a coverage by a few 1e-7 and a composited byte by one level at most, in about one soft-edge byte in ten
/// thousand, and a pixel on the very rim of a HARD brush (hardness 1: coverage 1 up to the radius) may be in on
/// one side and out on the other: one in a few thousand rim pixels. The test then allows a byte to be one level
/// off, in at most 1 in 5,000 of the bytes of soft strokes, and 1 in 2,000 of the pixels of hard ones.
#[test]
fn the_new_brush_paints_what_the_old_one_painted() {
    let exact = hypotf_is_correctly_rounded();
    let cases: u64 = if cfg!(debug_assertions) { 200 } else { 4000 };
    for hard in [false, true] {
        let (mut total, mut compared_bytes, mut compared_pixels, mut touched) = (Diff::default(), 0usize, 0usize, 0usize);
        let mut first_failure = None;
        for seed in 0..cases {
            let seed = seed + if hard { 1_000_000 } else { 0 };
            let case = random_case(seed, hard);
            let (new, old, changed) = paint_both(&case, seed);
            let d = diff(&new, &old);
            if d.bytes > 0 && first_failure.is_none() {
                first_failure = Some((seed, d));
            }
            total.bytes += d.bytes;
            total.pixels += d.pixels;
            total.max = total.max.max(d.max);
            compared_bytes += new.len();
            compared_pixels += new.len() / 4;
            touched += changed;
        }
        eprintln!(
            "BRUSH_EQUIVALENCE {} edges: {cases} cases, {compared_bytes} bytes, {touched} pixels painted, {} bytes in {} pixels differ (max {} levels); hypotf is the double-precision one: {exact}; first difference: {first_failure:?}",
            if hard { "hard" } else { "soft" },
            total.bytes,
            total.pixels,
            total.max
        );
        assert!(touched > compared_pixels / 20, "the strokes must paint something ({touched} of {compared_pixels} pixels)");
        if exact {
            assert_eq!(total.bytes, 0, "an exact hypotf: every byte agrees (first difference at seed/diff {first_failure:?})");
        } else if hard {
            assert!(
                total.pixels <= compared_pixels / 2000,
                "{} of {compared_pixels} pixels differ (first {first_failure:?})",
                total.pixels
            );
        } else {
            assert!(total.max <= 1, "a soft edge moves a byte by one level at most: {}", total.max);
            assert!(
                total.bytes <= compared_bytes / 5000,
                "{} of {compared_bytes} bytes differ (first {first_failure:?})",
                total.bytes
            );
        }
    }
}

/// Dabs on pixel centres and corners with radii that are whole or half numbers: the distances in play are
/// mostly exact (3-4-5 and half-integer triangles, squares), the rim cases are the ones an inexact test would
/// move. The pictures agree on every byte on a platform with a correctly rounded `hypotf`; elsewhere a rim
/// pixel of a hard brush may be in or out and a soft edge is a level off at most.
#[test]
fn a_dab_on_a_pixel_centre_or_corner_is_the_dab_it_was() {
    let exact = hypotf_is_correctly_rounded();
    for format in [RawImageFormat::RGBA8, RawImageFormat::BGRA8] {
        for radius in [0.5_f32, 1.0, 1.5, 2.0, 2.5, 3.0, 5.0, 6.5, 10.0] {
            for hardness in [0.0_f32, 0.5, 0.6, 0.85, 1.0] {
                for (cx, cy) in [(10.0, 10.0), (10.5, 10.5), (10.5, 10.0), (3.0, 4.5), (0.0, 0.0), (19.5, 19.5)] {
                    let b = brush(radius, hardness);
                    let mut new = image(20, 20, format, || 7);
                    let mut old = new.clone();
                    new.paint_dot(cx, cy, b);
                    reference_dot(&mut old, cx, cy, b, &mut NoMeter);
                    let d = diff(&bytes_of(&new), &bytes_of(&old));
                    if exact {
                        assert_eq!(d.bytes, 0, "{format:?} r {radius} hardness {hardness} at ({cx}, {cy})");
                    } else {
                        // a rim pixel of a hard brush may be in or out, anything else is a level off at most
                        assert!(hardness >= 1.0 || d.max <= 1, "{format:?} r {radius} hardness {hardness} at ({cx}, {cy}): {d:?}");
                        assert!(d.pixels <= 3, "{format:?} r {radius} hardness {hardness} at ({cx}, {cy}): {d:?}");
                    }
                }
            }
        }
    }
}

/// Brushes and images that are not what a program means to hand over - NaN and infinite values, a flow out of
/// range, an alpha of nothing, a radius of a hair or of a galaxy, a spacing that is zero, negative or NaN, a
/// one-pixel image, a buffer shorter than the size the fields claim - paint what the old brush painted (mostly
/// nothing, or the same garbage) and do not panic.
#[test]
fn a_brush_or_an_image_that_is_not_what_it_should_be_paints_what_the_old_brush_painted() {
    let base = Brush { color: red(255), radius: 4.0, hardness: 0.5, flow: 1.0, spacing: 0.25 };
    let mut brushes = vec![base];
    for hardness in [f32::NAN, -1.0, 2.0, f32::INFINITY, f32::NEG_INFINITY] {
        brushes.push(Brush { hardness, ..base });
    }
    for flow in [f32::NAN, -3.0, 0.0, 7.0, f32::INFINITY, 0.5] {
        brushes.push(Brush { flow, ..base });
    }
    for radius in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0e-30, 1.0e30, f32::MAX, 0.3, 100.0] {
        brushes.push(Brush { radius, ..base });
    }
    for spacing in [0.0, -1.0, f32::NAN, f32::INFINITY, 1.0e-9, 50.0] {
        brushes.push(Brush { spacing, ..base });
    }
    brushes.push(Brush { color: red(0), ..base });
    brushes.push(Brush { color: red(1), ..base });
    let segments = [
        (2.0, 2.0, 2.0, 2.0),
        (1.5, 1.5, 17.5, 9.5),
        (-30.0, 5.0, 60.0, 5.0),
        (9.0, -4.0, 9.5, 30.0),
        (5.0, 5.0, 5.2, 5.1),
    ];
    let exact = hypotf_is_correctly_rounded();
    for b in brushes {
        for seg in segments {
            for (w, h) in [(20, 12), (1, 1), (1, 9), (9, 1)] {
                let mut new = image(w, h, RawImageFormat::BGRA8, || 90);
                let mut old = new.clone();
                new.paint_stroke(seg.0, seg.1, seg.2, seg.3, b);
                reference_stroke(&mut old, seg.0, seg.1, seg.2, seg.3, b, &mut NoMeter);
                let d = diff(&bytes_of(&new), &bytes_of(&old));
                // The pictures may differ by the ulp of a distance (see `the_new_brush_paints_what_the_old_one_painted`).
                assert!(d.max <= 1 || (b.hardness >= 1.0 || b.hardness.is_nan()), "{b:?} {seg:?} {w}x{h}: {d:?}");
                if exact {
                    assert_eq!(d.bytes, 0, "{b:?} {seg:?} {w}x{h}");
                }
            }
        }
    }

    // A buffer shorter than the claimed size is painted as far as it goes, as before.
    let mut new = RawImage {
        pixels: RawImageData::U8(vec![40u8; 4 * 25].into()),
        width: 100,
        height: 100,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    };
    let mut old = new.clone();
    new.paint_stroke(0.0, 0.0, 40.0, 0.0, brush(3.0, 0.6));
    reference_stroke(&mut old, 0.0, 0.0, 40.0, 0.0, brush(3.0, 0.6), &mut NoMeter);
    assert_eq!(diff(&bytes_of(&new), &bytes_of(&old)).bytes, 0);
    new.paint_dot(50.0, 50.0, brush(4.0, 0.6));

    // Formats that are not 8-bit RGBA / BGRA are left alone.
    for format in [RawImageFormat::R8, RawImageFormat::RGB8, RawImageFormat::RGBA16, RawImageFormat::RGBAF32] {
        let mut img = image(8, 8, format, || 3);
        let before = bytes_of(&img);
        img.paint_stroke(0.0, 0.0, 8.0, 8.0, brush(3.0, 0.6));
        assert_eq!(bytes_of(&img), before, "{format:?}");
    }
}

/// A dab that covers a pixel fully stores the colour: what the blend of the old code worked out to.
#[test]
fn a_pixel_under_the_hard_core_of_an_opaque_dab_is_the_colour() {
    for format in [RawImageFormat::RGBA8, RawImageFormat::BGRA8] {
        let mut img = image(21, 21, format, || 77);
        img.paint_dot(10.5, 10.5, brush(8.0, 0.6));
        let px = bytes_of(&img);
        let centre = (10 * 21 + 10) * 4;
        let expected = if format == RawImageFormat::BGRA8 { [40, 30, 220, 255] } else { [220, 30, 40, 255] };
        assert_eq!(&px[centre..centre + 4], &expected, "{format:?}");
        // A corner of the image, outside the disc, is as it was.
        assert_eq!(&px[0..4], &[77, 77, 77, 77]);
    }
}

// ---------------------------------------------------------------------------------------------------------
// The time (by hand)
// ---------------------------------------------------------------------------------------------------------

/// How long the brush takes, old against new, on this machine: the time of a dab by radius, a segment off the
/// image, a segment on it, and a whiteboard of 399 pieces at 2x. Not an assertion - a timing is the machine's -
/// and only meaningful in a release build:
/// `cargo test --release -p azul-core --lib -- --ignored --nocapture how_long_the_brush_takes`.
#[test]
#[ignore = "X-EDGE-C-2: a measurement, not a check (run by hand, see the doc comment)"]
fn how_long_the_brush_takes() {
    use std::{hint::black_box, time::Instant};

    fn best_of(reps: usize, mut f: impl FnMut()) -> f64 {
        let mut best = f64::MAX;
        for _ in 0..reps {
            let start = Instant::now();
            f();
            best = best.min(start.elapsed().as_secs_f64());
        }
        best
    }
    // The two timed the same work: their pictures differ in none of their bytes (or, where the platform's `hypotf` is
    // not the correctly rounded one, in a few by a level).
    let same_work = |a: &RawImage, b: &RawImage| {
        let d = diff(&bytes_of(a), &bytes_of(b));
        eprintln!("BRUSH_BENCH   the pictures differ in {} bytes (max {} levels)", d.bytes, d.max);
    };
    let row = |what: &str, old_ns: f64, new_ns: f64, unit: &str| {
        eprintln!("BRUSH_BENCH {what:<46} old {old_ns:>12.1} {unit}   new {new_ns:>12.1} {unit}   {:>6.2}x", old_ns / new_ns);
    };

    // The time of one dab by radius (the pen's own brush: hardness 0.6, opaque), at random places of a 512 x 512 image.
    for radius in [1.0_f32, 3.0, 8.0, 20.0] {
        let dabs = ((2.0e6 / f64::from(radius * radius)) as usize).clamp(4_000, 200_000);
        let b = brush(radius, 0.6);
        let mut rng = Rng(5);
        let places: Vec<(f32, f32)> = (0..dabs).map(|_| (rng.range(30.0, 480.0), rng.range(30.0, 480.0))).collect();
        let mut old_img = image(512, 512, RawImageFormat::RGBA8, || 9);
        let mut new_img = old_img.clone();
        let old = best_of(5, || {
            for &(x, y) in &places {
                reference_dot(&mut old_img, black_box(x), black_box(y), b, &mut NoMeter);
            }
        });
        let new = best_of(5, || {
            for &(x, y) in &places {
                new_img.paint_dot(black_box(x), black_box(y), b);
            }
        });
        same_work(&old_img, &new_img);
        row(&format!("one dab, radius {radius}"), old * 1e9 / dabs as f64, new * 1e9 / dabs as f64, "ns");
    }

    // A segment of 5,000 px that lies off a 256 x 256 image (right of it), and one that crosses it.
    for radius in [3.0_f32, 8.0] {
        let mut b = brush(radius, 0.85);
        b.spacing = 0.5;
        let mut old_img = blank(256, 256);
        let mut new_img = old_img.clone();
        let calls = 200;
        let old = best_of(5, || {
            for i in 0..calls {
                let y = 20.0 + i as f32;
                reference_stroke(&mut old_img, black_box(1000.0), y, black_box(6000.0), y, b, &mut NoMeter);
            }
        });
        let new = best_of(5, || {
            for i in 0..calls {
                let y = 20.0 + i as f32;
                new_img.paint_stroke(black_box(1000.0), y, black_box(6000.0), y, b);
            }
        });
        row(&format!("segment of 5000 px off the image, radius {radius}"), old * 1e6 / f64::from(calls), new * 1e6 / f64::from(calls), "us");
    }
    {
        let b = brush(3.0, 0.85);
        let mut old_img = blank(256, 256);
        let mut new_img = old_img.clone();
        let calls = 200;
        let old = best_of(5, || {
            for i in 0..calls {
                let y = 20.0 + i as f32;
                reference_stroke(&mut old_img, black_box(-2000.0), y, black_box(2000.0), y, b, &mut NoMeter);
            }
        });
        let new = best_of(5, || {
            for i in 0..calls {
                let y = 20.0 + i as f32;
                new_img.paint_stroke(black_box(-2000.0), y, black_box(2000.0), y, b);
            }
        });
        same_work(&old_img, &new_img);
        row("segment of 4000 px crossing the image, radius 3", old * 1e6 / f64::from(calls), new * 1e6 / f64::from(calls), "us");
    }

    // The whiteboard of AzReview at 2x: a 2560 x 1490 overlay over a board three times its area, 399 pieces (the
    // cutouts' outlines, the arrows between them), a nib of 3 to 5 px, hardness 0.85, a dab every half radius.
    {
        let mut rng = Rng(399);
        let pieces: Vec<(Vec<(f32, f32)>, Brush)> = (0..399)
            .map(|k| {
                let mut p = (rng.range(-1500.0, 4000.0), rng.range(-900.0, 2400.0));
                let mut line = vec![p];
                // an arrow is one long segment, an outline a few short ones
                let hops = if k % 2 == 0 { 1 } else { 4 };
                for _ in 0..hops {
                    let len = if k % 2 == 0 { rng.range(150.0, 1500.0) } else { rng.range(40.0, 300.0) };
                    let angle = rng.range(0.0, core::f32::consts::TAU);
                    p = (p.0 + len * angle.cos(), p.1 + len * angle.sin());
                    line.push(p);
                }
                let mut b = brush(rng.range(3.0, 5.0), 0.85);
                b.spacing = 0.5;
                (line, b)
            })
            .collect();
        let mut old_img = blank(2560, 1490);
        let mut new_img = old_img.clone();
        let old = best_of(5, || {
            for (line, b) in &pieces {
                for w in line.windows(2) {
                    reference_stroke(&mut old_img, w[0].0, w[0].1, w[1].0, w[1].1, *b, &mut NoMeter);
                }
            }
        });
        let new = best_of(5, || {
            for (line, b) in &pieces {
                for w in line.windows(2) {
                    new_img.paint_stroke(w[0].0, w[0].1, w[1].0, w[1].1, *b);
                }
            }
        });
        same_work(&old_img, &new_img);
        let (mut old_counts, mut new_counts) = (Counts::default(), Counts::default());
        let (mut count_old, mut count_new) = (blank(2560, 1490), blank(2560, 1490));
        for (line, b) in &pieces {
            for w in line.windows(2) {
                reference_stroke(&mut count_old, w[0].0, w[0].1, w[1].0, w[1].1, *b, &mut old_counts);
                count_new.paint_stroke_metered(w[0].0, w[0].1, w[1].0, w[1].1, *b, &mut new_counts);
            }
        }
        row("whiteboard of 399 pieces at 2x (one frame)", old * 1e3, new * 1e3, "ms");
        eprintln!(
            "BRUSH_BENCH   work of that frame: dabs {} -> {}, pixels tested {} -> {}, square roots {} -> {}, composited {}",
            old_counts.dabs, new_counts.dabs, old_counts.pixels, new_counts.pixels, old_counts.roots, new_counts.roots, new_counts.blends
        );
    }
}
