//! The canvas viewport: what the ONE canvas image node shows.
//!
//! The view is the visible part of the document at the current zoom and pan,
//! rendered at the node's physical pixel size: the composite (nearest
//! neighbour when zoomed in, a box average when zoomed out) over a
//! checkerboard where it is transparent, the workspace colour around the
//! document, the selection's marching ants and the tool overlays (a marquee
//! being dragged, the crop frame). A stroke redraws only the view rect its
//! document rect maps to; the app hands exactly that rect to the renderer
//! (`CallbackInfo::change_node_image_rect`). A view change (zoom, pan, a new
//! size) redraws the whole view.
//!
//! No azul types here: the maths is tested without a window.

use crate::raster::{IRect, Mask, TileGrid};

/// The zoom levels the zoom tool and View > Zoom In / Out step through
/// (view pixels per document pixel): 1 % .. 3200 %.
pub const ZOOM_STEPS: [f32; 23] = [
    0.01, 0.02, 0.03, 0.05, 0.0625, 0.083_333, 0.125, 0.166_667, 0.25, 0.333_333, 0.5, 0.666_667,
    1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 12.0, 16.0, 32.0,
];
pub const ZOOM_MIN: f32 = 0.01;
pub const ZOOM_MAX: f32 = 32.0;

/// The checkerboard's square side in view pixels.
const CHECKER: u32 = 8;

/// Where the document sits in the view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// View pixels per document pixel (1.0 = 100 %).
    pub zoom: f32,
    /// Where the document's top-left corner lands, in view pixels.
    pub pan_x: f32,
    pub pan_y: f32,
    /// The view's size in (physical) pixels.
    pub width: u32,
    pub height: u32,
}

impl View {
    /// The document fitted into a `width` x `height` view with a margin,
    /// centred, never above 100 %.
    #[must_use]
    pub fn fit(doc_w: u32, doc_h: u32, width: u32, height: u32) -> Self {
        let margin = 0.92;
        let zx = width as f32 * margin / doc_w.max(1) as f32;
        let zy = height as f32 * margin / doc_h.max(1) as f32;
        let zoom = zx.min(zy).clamp(ZOOM_MIN, 1.0);
        let mut view = Self {
            zoom,
            pan_x: 0.0,
            pan_y: 0.0,
            width,
            height,
        };
        view.center(doc_w, doc_h);
        view
    }

    /// Put the document's centre in the view's centre.
    pub fn center(&mut self, doc_w: u32, doc_h: u32) {
        self.pan_x = ((self.width as f32 - doc_w as f32 * self.zoom) / 2.0).round();
        self.pan_y = ((self.height as f32 - doc_h as f32 * self.zoom) / 2.0).round();
    }

    #[inline]
    #[must_use]
    pub fn doc_to_view(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.zoom + self.pan_x, y * self.zoom + self.pan_y)
    }

    #[inline]
    #[must_use]
    pub fn view_to_doc(&self, x: f32, y: f32) -> (f32, f32) {
        ((x - self.pan_x) / self.zoom, (y - self.pan_y) / self.zoom)
    }

    /// The view pixels a document rect covers, clipped to the view.
    #[must_use]
    pub fn doc_rect_to_view(&self, r: &IRect) -> Option<IRect> {
        let (x0, y0) = self.doc_to_view(r.x as f32, r.y as f32);
        let (x1, y1) = self.doc_to_view(r.right() as f32, r.bottom() as f32);
        IRect::covering(x0, y0, x1, y1)
            .inflate(1)
            .intersect(&self.bounds())
    }

    /// The whole view as a rect.
    #[must_use]
    pub const fn bounds(&self) -> IRect {
        IRect::new(0, 0, self.width as i32, self.height as i32)
    }

    /// Zoom to `zoom`, keeping the document point under (`vx`, `vy`) there.
    pub fn zoom_about(&mut self, zoom: f32, vx: f32, vy: f32) {
        let zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        let (dx, dy) = self.view_to_doc(vx, vy);
        self.zoom = zoom;
        self.pan_x = (vx - dx * zoom).round();
        self.pan_y = (vy - dy * zoom).round();
    }

    /// The next zoom step in (`dir` > 0) or out from the current zoom.
    #[must_use]
    pub fn step(&self, dir: i32) -> f32 {
        if dir > 0 {
            ZOOM_STEPS
                .iter()
                .copied()
                .find(|z| *z > self.zoom * 1.001)
                .unwrap_or(ZOOM_MAX)
        } else {
            ZOOM_STEPS
                .iter()
                .rev()
                .copied()
                .find(|z| *z < self.zoom / 1.001)
                .unwrap_or(ZOOM_MIN)
        }
    }

    /// The zoom as the status bar shows it ("33.3 %").
    #[must_use]
    pub fn percent_label(&self) -> String {
        let p = self.zoom * 100.0;
        if (p - p.round()).abs() < 0.05 {
            format!("{} %", p.round() as i64)
        } else {
            format!("{p:.1} %")
        }
    }
}

/// Things drawn over the document that are not pixels of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlays {
    /// A marquee or lasso being dragged: a polyline in DOCUMENT pixels.
    pub outline: Vec<(f32, f32)>,
    /// Close the outline (a marquee) or leave it open (a lasso in progress).
    pub closed: bool,
    /// The crop frame being dragged (document pixels): outside it is dimmed.
    pub crop: Option<IRect>,
    /// The gradient / shape / clone-source guide line (document pixels).
    pub guide: Option<((f32, f32), (f32, f32))>,
}

impl Overlays {
    /// The view pixels these overlays touch.
    #[must_use]
    pub fn view_bounds(&self, view: &View) -> Option<IRect> {
        let mut points: Vec<(f32, f32)> = self.outline.clone();
        if let Some(((a, b), (c, d))) = self.guide {
            points.push((a, b));
            points.push((c, d));
        }
        let mut out = None::<IRect>;
        if !points.is_empty() {
            let vs: Vec<(f32, f32)> = points.iter().map(|(x, y)| view.doc_to_view(*x, *y)).collect();
            let x0 = vs.iter().map(|p| p.0).fold(f32::MAX, f32::min);
            let y0 = vs.iter().map(|p| p.1).fold(f32::MAX, f32::min);
            let x1 = vs.iter().map(|p| p.0).fold(f32::MIN, f32::max);
            let y1 = vs.iter().map(|p| p.1).fold(f32::MIN, f32::max);
            out = Some(IRect::covering(x0, y0, x1, y1).inflate(2));
        }
        if self.crop.is_some() {
            // The dimming covers the whole view.
            out = Some(view.bounds());
        }
        out.and_then(|r| r.intersect(&view.bounds()))
    }
}

/// The canvas pixels: BGRA8, premultiplied (all opaque), row by row.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewBuffer {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl ViewBuffer {
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            bgra: vec![0; (width as usize) * (height as usize) * 4],
        }
    }

    /// The pixel at (`x`, `y`) as RGBA (tests, the eyedropper on overlays).
    #[must_use]
    pub fn rgba(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [self.bgra[i + 2], self.bgra[i + 1], self.bgra[i], self.bgra[i + 3]]
    }

    #[inline]
    fn put(&mut self, x: u32, y: u32, rgb: [u8; 3]) {
        let i = ((y * self.width + x) * 4) as usize;
        self.bgra[i] = rgb[2];
        self.bgra[i + 1] = rgb[1];
        self.bgra[i + 2] = rgb[0];
        self.bgra[i + 3] = 255;
    }
}

/// The colours of the view's own chrome, per mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewColors {
    pub workspace: [u8; 3],
    pub checker_light: [u8; 3],
    pub checker_dark: [u8; 3],
}

impl ViewColors {
    #[must_use]
    pub const fn for_mode(dark: bool) -> Self {
        if dark {
            Self {
                workspace: [0x28, 0x29, 0x2d],
                checker_light: [0xcc, 0xcc, 0xcc],
                checker_dark: [0x99, 0x99, 0x99],
            }
        } else {
            Self {
                workspace: [0xb9, 0xbc, 0xc2],
                checker_light: [0xff, 0xff, 0xff],
                checker_dark: [0xcc, 0xcc, 0xcc],
            }
        }
    }
}

/// The composite's colour at the document point (`u`, `v`) for one view
/// pixel: nearest when zoomed in, the average of up to 4 x 4 samples of the
/// pixel's footprint when zoomed out. Straight RGBA.
fn sample(composite: &TileGrid, u: f32, v: f32, zoom: f32) -> [u8; 4] {
    if zoom >= 1.0 {
        return composite.pixel(u.floor() as u32, v.floor() as u32);
    }
    let foot = 1.0 / zoom;
    let n = (foot.ceil() as u32).clamp(1, 4);
    let step = foot / n as f32;
    let (w, h) = (composite.width() as f32, composite.height() as f32);
    let mut acc = [0.0f32; 4];
    let mut count = 0.0;
    for j in 0..n {
        for i in 0..n {
            let su = u - foot / 2.0 + (i as f32 + 0.5) * step;
            let sv = v - foot / 2.0 + (j as f32 + 0.5) * step;
            if su < 0.0 || sv < 0.0 || su >= w || sv >= h {
                continue;
            }
            let p = composite.pixel(su as u32, sv as u32);
            let a = f32::from(p[3]) / 255.0;
            acc[0] += f32::from(p[0]) * a;
            acc[1] += f32::from(p[1]) * a;
            acc[2] += f32::from(p[2]) * a;
            acc[3] += a;
            count += 1.0;
        }
    }
    if count == 0.0 || acc[3] <= 0.0 {
        return [0, 0, 0, 0];
    }
    [
        (acc[0] / acc[3]).round() as u8,
        (acc[1] / acc[3]).round() as u8,
        (acc[2] / acc[3]).round() as u8,
        ((acc[3] / count) * 255.0).round() as u8,
    ]
}

/// Whether the selection covers the document pixel under view pixel (x, y).
fn selected_at(view: &View, mask: &Mask, x: i32, y: i32) -> bool {
    let (u, v) = view.view_to_doc(x as f32 + 0.5, y as f32 + 0.5);
    if u < 0.0 || v < 0.0 {
        return false;
    }
    mask.get(u as u32, v as u32) >= 128
}

/// Draw view rect `r`: the composite over the checkerboard, the workspace
/// around the document, the marching ants of `selection` (dash phase
/// `phase`), then the overlays.
#[allow(clippy::too_many_arguments)]
pub fn render(
    buf: &mut ViewBuffer,
    view: &View,
    composite: &TileGrid,
    selection: Option<&Mask>,
    phase: u32,
    overlays: &Overlays,
    colors: ViewColors,
    r: IRect,
) {
    let Some(r) = r.intersect(&IRect::new(0, 0, buf.width as i32, buf.height as i32)) else {
        return;
    };
    let (dw, dh) = (composite.width() as f32, composite.height() as f32);
    for y in r.y..r.bottom() {
        for x in r.x..r.right() {
            let (u, v) = view.view_to_doc(x as f32 + 0.5, y as f32 + 0.5);
            let mut rgb = if u < 0.0 || v < 0.0 || u >= dw || v >= dh {
                colors.workspace
            } else {
                let checker = if ((x as u32 / CHECKER) + (y as u32 / CHECKER)) % 2 == 0 {
                    colors.checker_light
                } else {
                    colors.checker_dark
                };
                let p = sample(composite, u, v, view.zoom);
                let a = u32::from(p[3]);
                [0, 1, 2].map(|c| ((u32::from(p[c]) * a + u32::from(checker[c]) * (255 - a) + 127) / 255) as u8)
            };
            if let Some(crop) = overlays.crop {
                let inside = u >= crop.x as f32 && v >= crop.y as f32 && u < crop.right() as f32 && v < crop.bottom() as f32;
                if !inside {
                    rgb = rgb.map(|c| (u32::from(c) * 2 / 5) as u8);
                }
            }
            if let Some(mask) = selection {
                let here = selected_at(view, mask, x, y);
                let edge = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dx, dy)| selected_at(view, mask, x + dx, y + dy) != here);
                if edge && here {
                    rgb = ant(x, y, phase);
                }
            }
            buf.put(x as u32, y as u32, rgb);
        }
    }
    draw_overlays(buf, view, overlays, phase, r);
}

/// One pixel of a marching-ants dash: black and white runs of 4 along the
/// diagonal, shifted by `phase`.
fn ant(x: i32, y: i32, phase: u32) -> [u8; 3] {
    if ((x + y) as u32).wrapping_add(phase) / 4 % 2 == 0 {
        [0, 0, 0]
    } else {
        [255, 255, 255]
    }
}

/// The dashed outline, the crop frame and the guide line, inside `clip`.
fn draw_overlays(buf: &mut ViewBuffer, view: &View, overlays: &Overlays, phase: u32, clip: IRect) {
    let mut lines: Vec<((f32, f32), (f32, f32))> = Vec::new();
    let pts: Vec<(f32, f32)> = overlays
        .outline
        .iter()
        .map(|(x, y)| view.doc_to_view(*x, *y))
        .collect();
    for w in pts.windows(2) {
        lines.push((w[0], w[1]));
    }
    if overlays.closed && pts.len() > 2 {
        lines.push((pts[pts.len() - 1], pts[0]));
    }
    if let Some(crop) = overlays.crop {
        let a = view.doc_to_view(crop.x as f32, crop.y as f32);
        let b = view.doc_to_view(crop.right() as f32, crop.bottom() as f32);
        lines.push((a, (b.0, a.1)));
        lines.push(((b.0, a.1), b));
        lines.push((b, (a.0, b.1)));
        lines.push(((a.0, b.1), a));
    }
    if let Some((a, b)) = overlays.guide {
        lines.push((view.doc_to_view(a.0, a.1), view.doc_to_view(b.0, b.1)));
    }
    for (a, b) in lines {
        let steps = (b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil().max(1.0) as i32;
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let x = (a.0 + (b.0 - a.0) * t).floor() as i32;
            let y = (a.1 + (b.1 - a.1) * t).floor() as i32;
            if clip.contains(x, y) {
                buf.put(x as u32, y as u32, ant(s, 0, phase));
            }
        }
    }
}

/// A small picture of the whole composite for the Navigator panel: fits
/// `max_w` x `max_h`, over the checkerboard. Returns (width, height, BGRA).
#[must_use]
pub fn thumbnail(composite: &TileGrid, max_w: u32, max_h: u32, colors: ViewColors) -> ViewBuffer {
    let (dw, dh) = (composite.width().max(1), composite.height().max(1));
    let zoom = (max_w as f32 / dw as f32).min(max_h as f32 / dh as f32).min(1.0);
    let w = ((dw as f32 * zoom).round() as u32).max(1);
    let h = ((dh as f32 * zoom).round() as u32).max(1);
    let view = View {
        zoom,
        pan_x: 0.0,
        pan_y: 0.0,
        width: w,
        height: h,
    };
    let mut buf = ViewBuffer::new(w, h);
    render(&mut buf, &view, composite, None, 0, &Overlays::default(), colors, view.bounds());
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIGHT: ViewColors = ViewColors::for_mode(false);

    fn view(zoom: f32, pan_x: f32, pan_y: f32, w: u32, h: u32) -> View {
        View {
            zoom,
            pan_x,
            pan_y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn a_view_point_maps_to_the_document_and_back() {
        let v = view(2.5, 30.0, -12.0, 400, 300);
        let (dx, dy) = v.view_to_doc(105.0, 63.0);
        let (vx, vy) = v.doc_to_view(dx, dy);
        assert!((vx - 105.0).abs() < 1e-4 && (vy - 63.0).abs() < 1e-4);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer_in_place() {
        let mut v = view(1.0, 10.0, 20.0, 800, 600);
        let before = v.view_to_doc(300.0, 200.0);
        v.zoom_about(4.0, 300.0, 200.0);
        let after = v.view_to_doc(300.0, 200.0);
        assert!((before.0 - after.0).abs() < 0.5 && (before.1 - after.1).abs() < 0.5);
        assert_eq!(v.zoom, 4.0);
        v.zoom_about(1000.0, 0.0, 0.0);
        assert_eq!(v.zoom, ZOOM_MAX, "3200 % is the most");
    }

    #[test]
    fn fitting_centres_the_document_and_never_enlarges_it() {
        let v = View::fit(4000, 3000, 800, 600);
        assert!(v.zoom < 0.2);
        let (cx, cy) = v.doc_to_view(2000.0, 1500.0);
        assert!((cx - 400.0).abs() <= 1.0 && (cy - 300.0).abs() <= 1.0);
        assert_eq!(View::fit(100, 100, 800, 600).zoom, 1.0);
    }

    #[test]
    fn the_zoom_steps_walk_from_one_to_thirty_two_hundred_percent() {
        let mut v = view(1.0, 0.0, 0.0, 10, 10);
        assert_eq!(v.step(1), 2.0);
        assert_eq!(v.step(-1), 0.666_667);
        v.zoom = 32.0;
        assert_eq!(v.step(1), ZOOM_MAX);
        v.zoom = 0.01;
        assert_eq!(v.step(-1), ZOOM_MIN);
        assert_eq!(view(0.333_333, 0.0, 0.0, 1, 1).percent_label(), "33.3 %");
        assert_eq!(view(2.0, 0.0, 0.0, 1, 1).percent_label(), "200 %");
    }

    #[test]
    fn a_document_rect_maps_to_the_view_pixels_it_covers() {
        let v = view(2.0, 10.0, 10.0, 100, 100);
        assert_eq!(v.doc_rect_to_view(&IRect::new(5, 5, 3, 2)), Some(IRect::new(19, 19, 8, 6)));
        assert_eq!(v.doc_rect_to_view(&IRect::new(500, 5, 3, 2)), None, "off the view");
    }

    #[test]
    fn zoomed_in_each_document_pixel_is_a_block_and_the_workspace_surrounds_it() {
        let mut rgba = vec![0u8; 2 * 2 * 4];
        rgba[0..4].copy_from_slice(&[255, 0, 0, 255]);
        rgba[4..8].copy_from_slice(&[0, 0, 255, 255]);
        rgba[8..12].copy_from_slice(&[0, 255, 0, 255]);
        rgba[12..16].copy_from_slice(&[255, 255, 255, 255]);
        let doc = TileGrid::from_rgba(2, 2, &rgba);
        let v = view(4.0, 2.0, 2.0, 12, 12);
        let mut buf = ViewBuffer::new(12, 12);
        render(&mut buf, &v, &doc, None, 0, &Overlays::default(), LIGHT, v.bounds());
        assert_eq!(buf.rgba(2, 2), [255, 0, 0, 255]);
        assert_eq!(buf.rgba(5, 5), [255, 0, 0, 255]);
        assert_eq!(buf.rgba(6, 2), [0, 0, 255, 255]);
        assert_eq!(buf.rgba(9, 9), [255, 255, 255, 255]);
        let ws = LIGHT.workspace;
        assert_eq!(buf.rgba(0, 0), [ws[0], ws[1], ws[2], 255]);
        assert_eq!(buf.rgba(11, 11), [ws[0], ws[1], ws[2], 255]);
    }

    #[test]
    fn a_transparent_pixel_shows_the_checkerboard() {
        let doc = TileGrid::new(64, 64);
        let v = view(1.0, 0.0, 0.0, 64, 64);
        let mut buf = ViewBuffer::new(64, 64);
        render(&mut buf, &v, &doc, None, 0, &Overlays::default(), LIGHT, v.bounds());
        let l = LIGHT.checker_light;
        let d = LIGHT.checker_dark;
        assert_eq!(buf.rgba(0, 0), [l[0], l[1], l[2], 255]);
        assert_eq!(buf.rgba(8, 0), [d[0], d[1], d[2], 255]);
        assert_eq!(buf.rgba(8, 8), [l[0], l[1], l[2], 255]);
    }

    #[test]
    fn the_ants_run_along_the_selection_edge_only() {
        let doc = TileGrid::filled(40, 40, [128, 128, 128, 255]);
        let mask = Mask::from_shape(40, 40, &crate::raster::Shape::Rect(IRect::new(10, 10, 20, 20)));
        let v = view(1.0, 0.0, 0.0, 40, 40);
        let mut buf = ViewBuffer::new(40, 40);
        render(&mut buf, &v, &doc, Some(&mask), 0, &Overlays::default(), LIGHT, v.bounds());
        let edge_row: Vec<[u8; 4]> = (10..30).map(|x| buf.rgba(x, 10)).collect();
        assert!(edge_row.iter().any(|p| *p == [0, 0, 0, 255]));
        assert!(edge_row.iter().any(|p| *p == [255, 255, 255, 255]));
        assert_eq!(buf.rgba(20, 20), [128, 128, 128, 255], "inside: the image");
        assert_eq!(buf.rgba(2, 2), [128, 128, 128, 255], "outside: the image");
        // The next phase moves the dashes.
        let mut later = ViewBuffer::new(40, 40);
        render(&mut later, &v, &doc, Some(&mask), 2, &Overlays::default(), LIGHT, v.bounds());
        assert_ne!(later, buf);
    }

    #[test]
    fn a_partial_render_touches_only_its_rect() {
        let doc = TileGrid::filled(50, 50, [10, 20, 30, 255]);
        let v = view(1.0, 0.0, 0.0, 50, 50);
        let mut buf = ViewBuffer::new(50, 50);
        render(&mut buf, &v, &doc, None, 0, &Overlays::default(), LIGHT, IRect::new(5, 5, 10, 10));
        assert_eq!(buf.rgba(6, 6), [10, 20, 30, 255]);
        assert_eq!(buf.rgba(30, 30), [0, 0, 0, 0], "outside the rect: untouched");
    }

    #[test]
    fn zoomed_out_a_view_pixel_averages_its_footprint() {
        // Two columns, black and light grey; one view pixel covers all four.
        let mut rgba = vec![0u8; 2 * 2 * 4];
        for row in 0..2 {
            rgba[row * 8..row * 8 + 4].copy_from_slice(&[0, 0, 0, 255]);
            rgba[row * 8 + 4..row * 8 + 8].copy_from_slice(&[200, 200, 200, 255]);
        }
        let doc = TileGrid::from_rgba(2, 2, &rgba);
        let v = view(0.5, 0.0, 0.0, 1, 1);
        let mut buf = ViewBuffer::new(1, 1);
        render(&mut buf, &v, &doc, None, 0, &Overlays::default(), LIGHT, v.bounds());
        assert_eq!(buf.rgba(0, 0)[0], 100);
    }

    #[test]
    fn the_navigator_thumbnail_fits_its_box() {
        let doc = TileGrid::filled(400, 200, [1, 2, 3, 255]);
        let t = thumbnail(&doc, 100, 100, LIGHT);
        assert_eq!((t.width, t.height), (100, 50));
        assert_eq!(t.rgba(50, 25), [1, 2, 3, 255]);
    }
}
