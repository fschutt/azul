//! The map's overlays: lines, areas and markers an app puts on a
//! [`MapWidget`](super::MapWidget), drawn in the map's own view so they
//! move with the tiles on every pan, pinch, turn and tilt, without a rebuild
//! of the app's DOM.
//!
//! A line or an area lies on the ground. It is drawn tile by tile inside the
//! tile grid: every tile it touches gets one node over the tile's box with the
//! part of the shape inside that tile, in the tile's own units (0 to 256 at
//! the tile's zoom level, the node's `viewBox`). A pan moves the tile boxes
//! and nothing else, so every part keeps its geometry and the renderer keeps
//! the mask it stroked the part with. The camera's turn and tilt apply to the
//! parts as they apply to the tiles. The parts are cut once per zoom level and
//! kept in the map's cache ([`OverlayCache`]).
//!
//! A marker stands upright on its place: the place's point on the grid goes
//! through the camera (the grid's own transform, [`MapCamera`]) and the marker
//! is placed over the grid, untransformed. A tap on a marker is the marker's:
//! the map fires its `on_marker_tap` hook and drops no pin.

use alloc::{collections::btree_map::BTreeMap, string::String, sync::Arc, vec::Vec};

use azul_core::{
    dom::{Dom, OptionDom},
    svg::{SvgLine, SvgMultiPolygon, SvgPath, SvgPathElement, SvgPathElementVec, SvgPathVec},
};
use azul_css::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    props::basic::{color::ColorU, SvgPoint},
    AzString,
};

use super::MapLatLon;

#[cfg(test)]
#[path = "dash_tests.rs"]
mod dash_tests;

impl_option!(
    MapLatLon,
    OptionMapLatLon,
    [Debug, Clone, Copy, PartialEq]
);
impl_vec!(
    MapLatLon,
    MapLatLonVec,
    MapLatLonVecDestructor,
    MapLatLonVecDestructorType,
    MapLatLonVecSlice,
    OptionMapLatLon
);
impl_vec_clone!(MapLatLon, MapLatLonVec, MapLatLonVecDestructor);
impl_vec_debug!(MapLatLon, MapLatLonVec);
impl_vec_mut!(MapLatLon, MapLatLonVec);
azul_css::impl_vec_partialeq!(MapLatLon, MapLatLonVec);

/// The blue a route is drawn in when the app names no colour.
const ROUTE_BLUE: ColorU = ColorU {
    r: 0x1a,
    g: 0x73,
    b: 0xe8,
    a: 0xff,
};

/// How a [`MapPolyline`] is stroked along its length, as CSS names the
/// border styles. The pattern scales with the line's width and runs on across
/// corners and tiles.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MapLineDash {
    /// One unbroken line.
    #[default]
    Solid,
    /// Dashes about three widths long with gaps of one and a half widths.
    Dashed,
    /// Round dots one width across, two widths apart: a walk.
    Dotted,
}

/// A line on the ground: a route, a track, a border. Its points are joined in
/// order with round joins and caps, `width` px wide on the screen at every
/// zoom; a `casing_width` wider than `width` draws an outline under it.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct MapPolyline {
    /// The points, in drawing order. Fewer than two draw nothing.
    pub points: MapLatLonVec,
    /// The line's colour.
    pub color: ColorU,
    /// The line's width in px on the screen.
    pub width: f32,
    /// The outline's colour (drawn under the line).
    pub casing_color: ColorU,
    /// The outline's whole width in px; no outline when it is not wider than
    /// `width` or its colour is transparent.
    pub casing_width: f32,
    /// Solid, dashed or dotted; the outline follows the dashes.
    pub dash: MapLineDash,
}

impl MapPolyline {
    /// A blue line, 5 px wide, through `points`.
    #[must_use]
    pub const fn create(points: MapLatLonVec) -> Self {
        Self {
            points,
            color: ROUTE_BLUE,
            width: 5.0,
            casing_color: ColorU::TRANSPARENT,
            casing_width: 0.0,
            dash: MapLineDash::Solid,
        }
    }

    /// The line in `color`.
    #[must_use]
    pub const fn with_color(mut self, color: ColorU) -> Self {
        self.color = color;
        self
    }

    /// The line `width` px wide.
    #[must_use]
    pub const fn with_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// An outline under the line, `width` px wide in all, in `color`.
    #[must_use]
    pub const fn with_casing(mut self, color: ColorU, width: f32) -> Self {
        self.casing_color = color;
        self.casing_width = width;
        self
    }

    /// The line `dash`ed: solid, dashed or dotted.
    #[must_use]
    pub const fn with_dash(mut self, dash: MapLineDash) -> Self {
        self.dash = dash;
        self
    }
}

/// An area on the ground: a park, a district, a search radius. Its outline
/// closes back to the first point; the inside is filled with `fill`, the
/// outline stroked `stroke_width` px wide in `stroke`.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct MapPolygon {
    /// The outline. Fewer than three points draw nothing.
    pub points: MapLatLonVec,
    /// The inside's colour; transparent for none.
    pub fill: ColorU,
    /// The outline's colour.
    pub stroke: ColorU,
    /// The outline's width in px; 0 for none.
    pub stroke_width: f32,
}

impl MapPolygon {
    /// A light blue area with a blue outline.
    #[must_use]
    pub const fn create(points: MapLatLonVec) -> Self {
        Self {
            points,
            fill: ColorU {
                r: 0x1a,
                g: 0x73,
                b: 0xe8,
                a: 0x33,
            },
            stroke: ROUTE_BLUE,
            stroke_width: 2.0,
        }
    }

    /// The inside in `fill`.
    #[must_use]
    pub const fn with_fill(mut self, fill: ColorU) -> Self {
        self.fill = fill;
        self
    }

    /// The outline `width` px wide in `color`.
    #[must_use]
    pub const fn with_stroke(mut self, color: ColorU, width: f32) -> Self {
        self.stroke = color;
        self.stroke_width = width;
        self
    }
}

/// Something standing on a place: a pin, a dot, a photo. Its `icon` is any
/// DOM; the icon's point `(anchor_x, anchor_y)` (px from its top-left)
/// stands on `position` and the icon stays upright when the map turns or
/// tilts. An open `popup` shows centred above the icon.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct MapMarker {
    /// What the app knows the marker by: handed to the map's
    /// `on_marker_tap` hook and part of the marker's DOM key.
    pub id: u64,
    /// The place it stands on.
    pub position: MapLatLon,
    /// What it shows.
    pub icon: Dom,
    /// The icon's point on the place, px from the icon's left edge.
    pub anchor_x: f32,
    /// The icon's point on the place, px from the icon's top edge.
    pub anchor_y: f32,
    /// What a screen reader calls it; empty for nothing.
    pub name: AzString,
    /// A card shown above the icon while `popup_open`.
    pub popup: OptionDom,
    /// Whether the popup shows.
    pub popup_open: bool,
}

impl MapMarker {
    /// A marker `id` showing `icon` on `position`, the icon's top-left corner
    /// on the place (see [`Self::with_anchor`]).
    #[must_use]
    pub const fn create(id: u64, position: MapLatLon, icon: Dom) -> Self {
        Self {
            id,
            position,
            icon,
            anchor_x: 0.0,
            anchor_y: 0.0,
            name: AzString::from_const_str(""),
            popup: OptionDom::None,
            popup_open: false,
        }
    }

    /// A round dot of `color` with a white rim, centred on `position`.
    #[must_use]
    pub fn create_dot(id: u64, position: MapLatLon, color: ColorU) -> Self {
        let css = alloc::format!(
            "width: 10px; height: 10px; border-radius: 8px; border: 3px solid #ffffff; \
             background: {}; box-shadow: 0px 1px 3px rgba(0, 0, 0, 0.35);",
            css_color(color)
        );
        Self::create(id, position, Dom::create_div().with_css(css.as_str())).with_anchor(8.0, 8.0)
    }

    /// The icon's point `(x, y)` (px from its top-left) stands on the place.
    #[must_use]
    pub const fn with_anchor(mut self, x: f32, y: f32) -> Self {
        self.anchor_x = x;
        self.anchor_y = y;
        self
    }

    /// What a screen reader calls the marker.
    #[must_use]
    pub fn with_name(mut self, name: AzString) -> Self {
        self.name = name;
        self
    }

    /// A card above the icon, shown while `open`.
    #[must_use]
    pub fn with_popup(mut self, popup: Dom, open: bool) -> Self {
        self.popup = OptionDom::Some(popup);
        self.popup_open = open;
        self
    }
}

/// One thing on the map: a line, an area or a marker. The map draws them in
/// order, the lines and areas on the ground (under the place names), the
/// markers over everything.
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq)]
// An FFI enum holds its variants inline: no Box across the C ABI.
#[allow(clippy::large_enum_variant, variant_size_differences)]
pub enum MapOverlay {
    Polyline(MapPolyline),
    Polygon(MapPolygon),
    Marker(MapMarker),
}

impl_option!(
    MapOverlay,
    OptionMapOverlay,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    MapOverlay,
    MapOverlayVec,
    MapOverlayVecDestructor,
    MapOverlayVecDestructorType,
    MapOverlayVecSlice,
    OptionMapOverlay
);
impl_vec_clone!(MapOverlay, MapOverlayVec, MapOverlayVecDestructor);
impl_vec_debug!(MapOverlay, MapOverlayVec);
impl_vec_mut!(MapOverlay, MapOverlayVec);
azul_css::impl_vec_partialeq!(MapOverlay, MapOverlayVec);

/// A colour as CSS writes it, or the `system:<name>` keyword when it is a
/// system colour token: a line in the app's accent follows the theme.
pub(crate) fn css_color(color: ColorU) -> String {
    azul_css::props::basic::color::SystemColorRef::from_color_token(color)
        .map_or_else(|| color.to_hex(), |system| String::from(system.as_css_str()))
}

/// Whether `color` paints anything: an opaque or translucent colour, or a
/// system colour token (whose own alpha is 0: it names the theme's colour).
const fn paints(color: ColorU) -> bool {
    color.a > 0
        || azul_css::props::basic::color::SystemColorRef::from_color_token(color).is_some()
}

/// The camera of a map view: the transform its tile grid is drawn with
/// (`perspective() rotateX() rotate()` about the grid's centre), as a plane
/// homography, and its inverse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MapCamera {
    /// Grid point `(x, y, 1)` to screen `(x', y', w')`, row vector times
    /// matrix (`ComputedTransform3D::plane_homography`).
    forward: [f64; 9],
    /// Screen back to the grid.
    inverse: [f64; 9],
}

impl MapCamera {
    /// The camera of a `width` x `height` view at `viewport`; `None` for the
    /// flat, unturned view, whose grid IS the screen.
    ///
    /// It is built by the engine's own transform code from the same numbers
    /// the grid's CSS carries ([`super::camera_angles`]), so a marker placed
    /// through it stands exactly where the renderer draws its place.
    pub(crate) fn of(viewport: &super::MapViewport, width: f32, height: f32) -> Option<Self> {
        use azul_core::transform::{ComputedTransform3D, RotationMode};
        use azul_css::props::{
            basic::{AngleValue, PixelValue},
            style::{StyleTransform, StyleTransformOrigin},
        };
        let (distance, pitch, bearing) = super::camera_angles(viewport, width, height)?;
        let transform = ComputedTransform3D::from_style_transform_vec(
            &[
                StyleTransform::Perspective(PixelValue::px(distance)),
                StyleTransform::RotateX(AngleValue::deg(pitch)),
                StyleTransform::Rotate(AngleValue::deg(bearing)),
            ],
            &StyleTransformOrigin {
                x: PixelValue::const_percent(50),
                y: PixelValue::const_percent(50),
            },
            width,
            height,
            RotationMode::ForWebRender,
        );
        let forward = transform.plane_homography().map(f64::from);
        let mut inverse = invert(&forward)?;
        // A homography is only defined up to a factor, and the inverse can
        // come out with the factor negative: then every point in front of the
        // eye would read as behind it. The view's centre is always in front
        // (the transform turns about it), so its sign decides.
        let (cx, cy) = (f64::from(width) * 0.5, f64::from(height) * 0.5);
        if cx * inverse[2] + cy * inverse[5] + inverse[8] < 0.0 {
            for v in &mut inverse {
                *v = -*v;
            }
        }
        Some(Self { forward, inverse })
    }

    /// Where grid point `p` shows on the screen; `None` behind the eye.
    pub(crate) fn project(&self, p: (f64, f64)) -> Option<(f64, f64)> {
        apply(&self.forward, p)
    }

    /// Which grid point shows at screen point `p`; `None` above the horizon.
    pub(crate) fn unproject(&self, p: (f64, f64)) -> Option<(f64, f64)> {
        apply(&self.inverse, p)
    }
}

/// `[x y 1] * m`, divided by its `w`; `None` when `w` is not positive.
fn apply(m: &[f64; 9], (x, y): (f64, f64)) -> Option<(f64, f64)> {
    let w = x * m[2] + y * m[5] + m[8];
    // Behind the eye or at it (NaN too): no point on the screen.
    if w.partial_cmp(&1e-9) != Some(core::cmp::Ordering::Greater) {
        return None;
    }
    Some(((x * m[0] + y * m[3] + m[6]) / w, (x * m[1] + y * m[4] + m[7]) / w))
}

/// The inverse of a row-major 3x3 matrix; `None` when it has none.
#[allow(clippy::many_single_char_names)] // the matrix entries, as a textbook names them
fn invert(m: &[f64; 9]) -> Option<[f64; 9]> {
    let [a, b, c, d, e, f, g, h, i] = *m;
    let c00 = e * i - f * h;
    let c01 = -(d * i - f * g);
    let c02 = d * h - e * g;
    let det = a * c00 + b * c01 + c * c02;
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let k = 1.0 / det;
    Some([
        c00 * k,
        -(b * i - c * h) * k,
        (b * f - c * e) * k,
        c01 * k,
        (a * i - c * g) * k,
        -(a * f - c * d) * k,
        c02 * k,
        -(a * h - b * g) * k,
        (a * e - b * d) * k,
    ])
}

/// What one tile shows of one line or area, in the tile's units (0 to 256 on
/// both axes at the tile's zoom level).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TileShape {
    /// The area's inside, cut to the tile; `None` for a line, or an area
    /// that misses the tile.
    pub(crate) fill: Option<Arc<SvgMultiPolygon>>,
    /// The line, or the area's outline, cut to the tile (and a margin of
    /// half the widest stroke around it); `None` when it misses the tile.
    pub(crate) stroke: Option<Arc<SvgMultiPolygon>>,
}

/// A line's or an area's points on the world at one zoom level, in px of
/// that level (256 per tile), ready to be cut into tiles.
#[derive(Debug)]
struct Projected {
    /// The points in order. Longitudes are unwrapped: a line that crosses
    /// the antimeridian keeps going past the world's edge instead of
    /// jumping back across the whole map.
    points: Vec<(f64, f64)>,
    /// The smallest x and y of `points`, widened by `margin`.
    min: (f64, f64),
    /// The largest x and y of `points`, widened by `margin`.
    max: (f64, f64),
    /// How far past a tile's edge its part of the stroke reaches.
    margin: f64,
    /// An area: its outline closes back to the first point and it has an
    /// inside to fill.
    area: bool,
    /// What is stroked: the line, its dashes (cut on the whole line, so the
    /// pattern runs on across tiles), or the area's closed outline.
    runs: Vec<Vec<(f64, f64)>>,
}

/// Parts kept before the cache starts over: a few screens of tiles for a
/// handful of lines, small next to the tiles' own drawings.
const MAX_PARTS: usize = 4096;

/// What the map keeps of its lines and areas between renders: each one's
/// parts per tile, at the zoom level last drawn. Lives in the map's cache,
/// in step with its overlays ([`OverlayCache::adopt`]).
#[derive(Debug, Default)]
pub struct OverlayCache {
    /// A fingerprint of each overlay's shape (its points and widths), by
    /// index: an overlay whose fingerprint changed is cut again, one that
    /// kept it reuses its parts across rebuilds of the app's DOM.
    keys: Vec<u64>,
    /// The zoom level `projected` and `parts` belong to. A new level starts
    /// both over: a tile of another level is another cut.
    zoom: Option<u8>,
    /// Each line's or area's points at `zoom`, by overlay index.
    projected: BTreeMap<usize, Arc<Projected>>,
    /// The parts cut so far at `zoom`, by overlay index and tile (column
    /// wrapped into the world, row). A pan finds its tiles here.
    parts: BTreeMap<(usize, u32, u32), TileShape>,
}

impl OverlayCache {
    /// Nothing cut yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            keys: Vec::new(),
            zoom: None,
            projected: BTreeMap::new(),
            parts: BTreeMap::new(),
        }
    }

    /// Takes the map's new overlays: forgets the parts of lines and areas
    /// that changed or left, keeps the rest.
    pub(crate) fn adopt(&mut self, overlays: &[MapOverlay]) {
        let keys: Vec<u64> = overlays.iter().map(fingerprint).collect();
        let stale = |index: usize| self.keys.get(index) != keys.get(index);
        let stale_indices: Vec<usize> = (0..self.keys.len().max(keys.len()))
            .filter(|i| stale(*i))
            .collect();
        for index in stale_indices {
            self.projected.remove(&index);
            self.parts.retain(|(i, _, _), _| *i != index);
        }
        self.keys = keys;
    }

    /// What tile `(z, x, y)` (`x` wrapped into the world) shows of overlay
    /// `index` of `overlays` - the map's, as last adopted.
    pub(crate) fn tile_shape(
        &mut self,
        overlays: &[MapOverlay],
        index: usize,
        z: u8,
        x: u32,
        y: u32,
    ) -> TileShape {
        if self.zoom != Some(z) {
            self.zoom = Some(z);
            self.projected.clear();
            self.parts.clear();
        }
        if let Some(shape) = self.parts.get(&(index, x, y)) {
            return shape.clone();
        }
        let Some(projected) = self.projected_of(overlays, index, z) else {
            return TileShape::default();
        };
        let shape = cut(&projected, z, x, y);
        if self.parts.len() >= MAX_PARTS {
            self.parts.clear();
        }
        self.parts.insert((index, x, y), shape.clone());
        shape
    }

    /// Overlay `index`'s points at zoom `z`, projected once per level.
    fn projected_of(
        &mut self,
        overlays: &[MapOverlay],
        index: usize,
        z: u8,
    ) -> Option<Arc<Projected>> {
        if let Some(p) = self.projected.get(&index) {
            return Some(Arc::clone(p));
        }
        let projected = Arc::new(project_overlay(overlays.get(index)?, z)?);
        self.projected.insert(index, Arc::clone(&projected));
        Some(projected)
    }
}

/// A hash of what decides an overlay's cut: its kind, points, stroke widths
/// and dash. Colours are not in it, they are the nodes' CSS.
fn fingerprint(overlay: &MapOverlay) -> u64 {
    // FNV-1a: tiny, stable, and needs no hasher from std.
    fn mix(h: u64, v: u64) -> u64 {
        let mut h = h;
        for byte in v.to_le_bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    }
    let with_points = |h: u64, points: &MapLatLonVec| {
        points.as_slice().iter().fold(h, |h, p| {
            mix(mix(h, p.lat_deg.to_bits()), p.lon_deg.to_bits())
        })
    };
    let h = 0xcbf2_9ce4_8422_2325;
    match overlay {
        MapOverlay::Polyline(line) => with_points(
            mix(
                mix(
                    mix(mix(h, 1), u64::from(line.width.to_bits())),
                    u64::from(line.casing_width.to_bits()),
                ),
                line.dash as u64,
            ),
            &line.points,
        ),
        MapOverlay::Polygon(area) => with_points(
            mix(mix(h, 2), u64::from(area.stroke_width.to_bits())),
            &area.points,
        ),
        MapOverlay::Marker(marker) => mix(mix(h, 3), marker.id),
    }
}

/// The points of a line or an area at zoom `z`; `None` for a marker and for
/// a shape with too few points to draw.
fn project_overlay(overlay: &MapOverlay, z: u8) -> Option<Projected> {
    let (points, widest, area, pattern) = match overlay {
        MapOverlay::Polyline(line) if line.points.as_slice().len() >= 2 => (
            line.points.as_slice(),
            line.width.max(line.casing_width),
            false,
            dash_pattern(line.dash, line.width),
        ),
        MapOverlay::Polygon(area) if area.points.as_slice().len() >= 3 => {
            (area.points.as_slice(), area.stroke_width, true, None)
        }
        _ => return None,
    };
    let world = 256.0 * f64::from(1u32 << u32::from(z.min(30)));
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(points.len());
    for p in points {
        if !p.lat_deg.is_finite() || !p.lon_deg.is_finite() {
            continue;
        }
        let lat = p.lat_deg.clamp(-85.051_128, 85.051_128);
        let mut x = super::lon_to_tile_x(p.lon_deg, 1.0) * world;
        let y = super::lat_to_tile_y(lat, 1.0) * world;
        if let Some(&(prev_x, _)) = out.last() {
            // Of the point's copies around the world, the one nearest the
            // previous point: the short way across the antimeridian.
            x -= ((x - prev_x) / world).round() * world;
        }
        out.push((x, y));
    }
    if out.len() < if area { 3 } else { 2 } {
        return None;
    }
    // Half the widest stroke at the largest it can be in tile units (a tile
    // is drawn 256 to 512 px wide, so a unit is at most a px), and a px for
    // the edge's anti-aliasing.
    let margin = f64::from(widest.max(0.0)) * 0.5 + 1.0;
    let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    for &(x, y) in &out {
        min = (min.0.min(x), min.1.min(y));
        max = (max.0.max(x), max.1.max(y));
    }
    let runs = match pattern {
        _ if area => {
            let mut outline = out.clone();
            outline.push(out[0]);
            alloc::vec![outline]
        }
        Some((dash, gap)) => dash_runs(&out, dash, gap),
        None => alloc::vec![out.clone()],
    };
    Some(Projected {
        points: out,
        min: (min.0 - margin, min.1 - margin),
        max: (max.0 + margin, max.1 + margin),
        margin,
        area,
        runs,
    })
}

/// How long a dot's stroke is, in px of a zoom level: long enough to give the
/// stroker a direction for its round caps, short enough to read as a dot.
const DOT_LENGTH: f64 = 0.05;

/// More dashes than this on one line (a long route zoomed in far) and the
/// line is drawn solid: cutting it would cost more than it shows.
const MAX_DASHES: usize = 50_000;

/// The `(dash, gap)` lengths in px of a zoom level for a line `width` px wide
/// dashed as `dash`; `None` for a solid line. The round caps reach half a
/// width past each end of a dash, so the gaps are wider than the line. A
/// level's px are the screen's at the level's own zoom and grow to two
/// screen px towards the next level, the pattern with them: the dashes are
/// cut once per level, like the rest of the line.
pub(crate) fn dash_pattern(dash: MapLineDash, width: f32) -> Option<(f64, f64)> {
    let w = f64::from(width);
    if !w.is_finite() || w <= 0.0 {
        return None;
    }
    match dash {
        MapLineDash::Solid => None,
        MapLineDash::Dashed => Some((2.0 * w, 2.5 * w)),
        MapLineDash::Dotted => Some((DOT_LENGTH, 2.0 * w - DOT_LENGTH)),
    }
}

/// `points` cut into dashes `dash` long with gaps `gap` long between them,
/// the first dash at the first point. The pattern runs on around corners (a
/// dash that turns one keeps the corner point), and a dash cut short by the
/// line's end is kept. The whole line as one run when the pattern is not a
/// usable one or would make more than [`MAX_DASHES`] dashes.
fn dash_runs(points: &[(f64, f64)], dash: f64, gap: f64) -> Vec<Vec<(f64, f64)>> {
    let usable = dash.is_finite() && gap.is_finite() && dash > 0.0 && gap > 0.0;
    if !usable || points.len() < 2 {
        return alloc::vec![points.to_vec()];
    }
    let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut run: Vec<(f64, f64)> = alloc::vec![points[0]];
    // Whether the pen is down (in a dash), and how much of the dash or the
    // gap it is in is left.
    let mut on = true;
    let mut left = dash;
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        if !len.is_finite() || len <= 0.0 {
            continue;
        }
        let mut done = 0.0;
        while len - done > left {
            done += left;
            let p = (a.0 + (b.0 - a.0) * done / len, a.1 + (b.1 - a.1) * done / len);
            if on {
                run.push(p);
                runs.push(core::mem::take(&mut run));
                if runs.len() > MAX_DASHES {
                    return alloc::vec![points.to_vec()];
                }
                left = gap;
            } else {
                run = alloc::vec![p];
                left = dash;
            }
            on = !on;
        }
        left -= len - done;
        if on {
            run.push(b);
        }
    }
    if on && run.len() >= 2 {
        runs.push(run);
    }
    runs
}

/// Cuts `shape` to tile `(z, x, y)`: the parts of each copy of it around the
/// world that fall on the tile, moved into the tile's units.
#[allow(clippy::cast_possible_truncation)] // tile units are small: 0 to 256 and a margin
fn cut(shape: &Projected, z: u8, x: u32, y: u32) -> TileShape {
    let world = 256.0 * f64::from(1u32 << u32::from(z.min(30)));
    let (left, top) = (f64::from(x) * 256.0, f64::from(y) * 256.0);
    let mut fill_rings: Vec<SvgPath> = Vec::new();
    let mut stroke_rings: Vec<SvgPath> = Vec::new();
    // An unwrapped line can reach one world past either edge.
    for copy in [-1.0, 0.0, 1.0] {
        let (x0, y0) = (left + copy * world, top);
        let (x1, y1) = (x0 + 256.0, y0 + 256.0);
        // `min` and `max` already hold the margin.
        if shape.max.0 < x0 || shape.min.0 > x1 || shape.max.1 < y0 || shape.min.1 > y1 {
            continue;
        }
        let m = shape.margin;
        let local = |(px, py): (f64, f64)| SvgPoint {
            x: (px - x0) as f32,
            y: (py - y0) as f32,
        };
        if shape.area {
            let ring = clip_polygon(&shape.points, [x0, y0, x1, y1]);
            if ring.len() >= 3 {
                fill_rings.push(closed_path(ring.into_iter().map(local).collect()));
            }
        }
        for run in &shape.runs {
            for part in clip_polyline(run, [x0 - m, y0 - m, x1 + m, y1 + m]) {
                stroke_rings.push(open_path(part.into_iter().map(local).collect()));
            }
        }
    }
    let multi = |rings: Vec<SvgPath>| {
        (!rings.is_empty())
            .then(|| Arc::new(SvgMultiPolygon::create(SvgPathVec::from_vec(rings))))
    };
    TileShape {
        fill: multi(fill_rings),
        stroke: multi(stroke_rings),
    }
}

/// `points` joined by straight segments.
fn open_path(points: Vec<SvgPoint>) -> SvgPath {
    let items: Vec<SvgPathElement> = points
        .windows(2)
        .map(|w| SvgPathElement::Line(SvgLine { start: w[0], end: w[1] }))
        .collect();
    SvgPath::create(SvgPathElementVec::from_vec(items))
}

/// `points` joined by straight segments and back to the first.
fn closed_path(mut points: Vec<SvgPoint>) -> SvgPath {
    if let Some(&first) = points.first() {
        points.push(first);
    }
    open_path(points)
}

/// The parts of the line through `points` inside `rect` (`[x0, y0, x1, y1]`),
/// each a run of connected points: a line that leaves the rect and comes
/// back is two runs.
fn clip_polyline(points: &[(f64, f64)], rect: [f64; 4]) -> Vec<Vec<(f64, f64)>> {
    let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut open = false;
    for pair in points.windows(2) {
        match clip_segment(pair[0], pair[1], rect) {
            Some((a, b, starts_inside, ends_inside)) => {
                if !(open && starts_inside) {
                    runs.push(alloc::vec![a]);
                }
                if let Some(run) = runs.last_mut() {
                    run.push(b);
                }
                open = ends_inside;
            }
            None => open = false,
        }
    }
    runs
}

/// A clipped segment: its two ends, and whether each is the original's end.
type ClippedSegment = ((f64, f64), (f64, f64), bool, bool);

/// The part of segment `a`-`b` inside `rect` (Liang-Barsky), with whether
/// it starts at `a` and ends at `b` (they are inside); `None` when it misses.
fn clip_segment(a: (f64, f64), b: (f64, f64), rect: [f64; 4]) -> Option<ClippedSegment> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-dx, a.0 - rect[0]),
        (dx, rect[2] - a.0),
        (-dy, a.1 - rect[1]),
        (dy, rect[3] - a.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    if t0 > t1 {
        return None;
    }
    let at = |t: f64| (a.0 + dx * t, a.1 + dy * t);
    Some((at(t0), at(t1), t0 <= 0.0, t1 >= 1.0))
}

/// The polygon `points` cut to `rect` (Sutherland-Hodgman). The rect is
/// convex, so the result is one ring; it can be empty.
fn clip_polygon(points: &[(f64, f64)], rect: [f64; 4]) -> Vec<(f64, f64)> {
    let mut ring: Vec<(f64, f64)> = points.to_vec();
    // Each edge of the rect: is a point inside it, and where a segment
    // crosses it.
    for edge in 0..4 {
        if ring.is_empty() {
            break;
        }
        let inside = |p: (f64, f64)| match edge {
            0 => p.0 >= rect[0],
            1 => p.0 <= rect[2],
            2 => p.1 >= rect[1],
            _ => p.1 <= rect[3],
        };
        let cross = |a: (f64, f64), b: (f64, f64)| {
            let t = match edge {
                0 => (rect[0] - a.0) / (b.0 - a.0),
                1 => (rect[2] - a.0) / (b.0 - a.0),
                2 => (rect[1] - a.1) / (b.1 - a.1),
                _ => (rect[3] - a.1) / (b.1 - a.1),
            };
            (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
        };
        let input = core::mem::take(&mut ring);
        for (i, &current) in input.iter().enumerate() {
            let previous = input[(i + input.len() - 1) % input.len()];
            match (inside(previous), inside(current)) {
                (true, true) => ring.push(current),
                (true, false) => ring.push(cross(previous, current)),
                (false, true) => {
                    ring.push(cross(previous, current));
                    ring.push(current);
                }
                (false, false) => {}
            }
        }
    }
    ring
}

/// One tile's box in the map's grid, as the render placed it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TileBox {
    /// The column as drawn, not wrapped (at zoom 0 and 1 one tile can show
    /// twice): part of the parts' node keys.
    pub(crate) raw_x: i32,
    /// The column wrapped into the world: the tile the parts are cut for.
    pub(crate) x: u32,
    /// The row.
    pub(crate) y: u32,
    /// The box's left edge in the grid, px.
    pub(crate) left: i32,
    /// The box's top edge in the grid, px.
    pub(crate) top: i32,
    /// The box's width, px.
    pub(crate) width: i32,
    /// The box's height, px.
    pub(crate) height: i32,
}

/// The grid's nodes for the lines and areas: per overlay, one node over each
/// tile box it touches, holding that tile's part of it in the tile's units
/// (the node's `viewBox`). A pan moves the boxes and leaves every part's
/// geometry as it was, so the renderer's stroke masks are reused.
pub(crate) fn ground_nodes(
    cache: &mut OverlayCache,
    overlays: &[MapOverlay],
    z: u8,
    boxes: &[TileBox],
    tile_px: f32,
) -> Vec<Dom> {
    use azul_core::dom::SvgNodeData;
    // Strokes are as wide on the screen at every zoom: in tile units they
    // shrink as the tile is drawn bigger.
    let units_per_px = 256.0 / tile_px.max(1.0);
    let layer = "position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px;";
    let stroke_node = |shape: &Arc<SvgMultiPolygon>, color: ColorU, width_px: f32| {
        let css = alloc::format!(
            "{layer} border: {:.3}px solid {};",
            width_px * units_per_px,
            css_color(color)
        );
        Dom::create_div()
            .with_css(css.as_str())
            .with_svg_data(SvgNodeData::Path((**shape).clone()))
    };
    let mut out = Vec::new();
    for (index, overlay) in overlays.iter().enumerate() {
        if matches!(overlay, MapOverlay::Marker(_)) {
            continue;
        }
        for b in boxes {
            let shape = cache.tile_shape(overlays, index, z, b.x, b.y);
            let mut layers: Vec<Dom> = Vec::new();
            match overlay {
                MapOverlay::Polyline(line) => {
                    let Some(stroke) = &shape.stroke else {
                        continue;
                    };
                    if line.casing_width > line.width && paints(line.casing_color) {
                        layers.push(stroke_node(stroke, line.casing_color, line.casing_width));
                    }
                    if line.width > 0.0 && paints(line.color) {
                        layers.push(stroke_node(stroke, line.color, line.width));
                    }
                }
                MapOverlay::Polygon(area) => {
                    if let Some(fill) = shape.fill.as_ref().filter(|_| paints(area.fill)) {
                        let css = alloc::format!("{layer} background: {};", css_color(area.fill));
                        layers.push(
                            Dom::create_div()
                                .with_css(css.as_str())
                                .with_svg_data(SvgNodeData::Path((**fill).clone())),
                        );
                    }
                    if let Some(stroke) = &shape.stroke {
                        if area.stroke_width > 0.0 && paints(area.stroke) {
                            layers.push(stroke_node(stroke, area.stroke, area.stroke_width));
                        }
                    }
                }
                MapOverlay::Marker(_) => {}
            }
            if layers.is_empty() {
                continue;
            }
            // The box clips its parts: a neighbour's stroke never paints
            // over this tile's.
            let css = alloc::format!(
                "position: absolute; left: {}px; top: {}px; width: {}px; height: {}px; \
                 overflow: hidden;",
                b.left, b.top, b.width, b.height
            );
            let mut node = Dom::create_div()
                .with_css(css.as_str())
                .with_svg_data(SvgNodeData::ViewBox {
                    min_x: 0.0,
                    min_y: 0.0,
                    width: 256.0,
                    height: 256.0,
                })
                .with_key(("azul-map-ground", index, z, b.raw_x, b.y));
            for part in layers {
                node = node.with_child(part);
            }
            out.push(node);
        }
    }
    out
}

/// The markers' nodes, over the grid and upright: each icon's anchor on the
/// screen point `place` gives its position (the camera's projection of the
/// place's grid point), with its popup centred above it. A marker behind the
/// eye or far outside the `width` x `height` view is left out.
pub(crate) fn marker_nodes(
    overlays: &[MapOverlay],
    place: &dyn Fn(MapLatLon) -> Option<(f64, f64)>,
    width: f32,
    height: f32,
    cache: &azul_core::refany::RefAny,
) -> Vec<Dom> {
    use azul_core::dom::{EventFilter, HoverEventFilter};

    use crate::callbacks::Callback;
    // How far outside the view a marker is still drawn: its icon may reach in.
    const SLACK: f64 = 256.0;
    let (w, h) = (f64::from(width), f64::from(height));
    let mut out = Vec::new();
    for overlay in overlays {
        let MapOverlay::Marker(marker) = overlay else {
            continue;
        };
        let Some((sx, sy)) = place(marker.position) else {
            continue;
        };
        if !(sx > -SLACK && sx < w + SLACK && sy > -SLACK && sy < h + SLACK) {
            continue;
        }
        let left = sx - f64::from(marker.anchor_x);
        let top = sy - f64::from(marker.anchor_y);
        let handle = azul_core::refany::RefAny::new(super::MarkerHandle {
            cache: cache.clone(),
            id: marker.id,
        });
        let css = alloc::format!("position: absolute; left: {left:.1}px; top: {top:.1}px;");
        let mut node = Dom::create_div()
            .with_css(css.as_str())
            .with_key(("azul-map-marker", marker.id))
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseDown),
                handle.clone(),
                Callback::from_ptr(super::map_marker_on_press),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::TouchStart),
                handle.clone(),
                Callback::from_ptr(super::map_marker_on_press),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseUp),
                handle.clone(),
                Callback::from_ptr(super::map_marker_on_release),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::TouchEnd),
                handle,
                Callback::from_ptr(super::map_marker_on_release),
            )
            .with_child(marker.icon.clone());
        if !marker.name.as_str().is_empty() {
            node = node.with_accessibility_info(azul_core::a11y::AccessibilityInfo::named(
                marker.name.clone(),
                azul_core::a11y::AccessibilityRole::PushButton,
            ));
        }
        out.push(node);
        if let (OptionDom::Some(popup), true) = (&marker.popup, marker.popup_open) {
            out.push(popup_node(popup, sx, top, h, marker.id, cache));
        }
    }
    out
}

/// The widest popup card the map centres above a marker, px.
const POPUP_MAX_WIDTH: f64 = 320.0;

/// A marker's popup: a box `POPUP_MAX_WIDTH` wide centred on `x`, its bottom
/// 6 px above `icon_top` (in a view `view_height` tall), the card centred in
/// it. A press on it is the card's, not the map's: it neither drags the map
/// nor drops a pin.
fn popup_node(
    popup: &Dom,
    x: f64,
    icon_top: f64,
    view_height: f64,
    id: u64,
    cache: &azul_core::refany::RefAny,
) -> Dom {
    use azul_core::dom::{EventFilter, HoverEventFilter};

    use crate::callbacks::Callback;
    let css = alloc::format!(
        "position: absolute; left: {:.1}px; width: {POPUP_MAX_WIDTH:.0}px; bottom: {:.1}px; \
         display: flex; flex-direction: column; align-items: center;",
        x - POPUP_MAX_WIDTH * 0.5,
        view_height - icon_top + 6.0
    );
    let mut node = Dom::create_div()
        .with_css(css.as_str())
        .with_key(("azul-map-popup", id));
    for filter in [
        HoverEventFilter::MouseDown,
        HoverEventFilter::MouseUp,
        HoverEventFilter::TouchStart,
        HoverEventFilter::TouchEnd,
    ] {
        node = node.with_callback(
            EventFilter::Hover(filter),
            cache.clone(),
            Callback::from_ptr(super::map_popup_keeps_its_press),
        );
    }
    node.with_child(popup.clone())
}
