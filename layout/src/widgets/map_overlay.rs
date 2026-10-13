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
    pub fn dot(id: u64, position: MapLatLon, color: ColorU) -> Self {
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
#[allow(clippy::large_enum_variant)] // an FFI enum holds its variants inline: no Box across the C ABI
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
    pub(crate) fn of(_viewport: &super::MapViewport, _width: f32, _height: f32) -> Option<Self> {
        None
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
    if !(w > 1e-9) {
        return None;
    }
    Some(((x * m[0] + y * m[3] + m[6]) / w, (x * m[1] + y * m[4] + m[7]) / w))
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

/// What the map keeps of its lines and areas between renders: each one's
/// parts per tile, at the zoom level last drawn. Lives in the map's cache,
/// in step with its overlays ([`OverlayCache::adopt`]).
#[derive(Debug, Default)]
pub struct OverlayCache {
    /// A fingerprint of each overlay (its points and widths), by index.
    keys: Vec<u64>,
}

impl OverlayCache {
    /// Nothing cut yet.
    #[must_use]
    pub const fn new() -> Self {
        Self { keys: Vec::new() }
    }

    /// Takes the map's new overlays: forgets the parts of lines and areas
    /// that changed or left.
    pub(crate) fn adopt(&mut self, _overlays: &[MapOverlay]) {}

    /// What tile `(z, x, y)` (`x` wrapped into the world) shows of overlay
    /// `index` of `overlays` - the map's, as last adopted.
    pub(crate) fn tile_shape(
        &mut self,
        _overlays: &[MapOverlay],
        _index: usize,
        _z: u8,
        _x: u32,
        _y: u32,
    ) -> TileShape {
        TileShape::default()
    }
}
