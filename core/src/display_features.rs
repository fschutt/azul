//! What the display of a phone has built in and what the system draws over a window, in WINDOW
//! coordinates: the cutouts (a punch-hole camera, a notch, the Dynamic Island), the rounded
//! corners, the system bars, the fingerprint sensor under the glass.
//!
//! The safe-area insets (`azul_css::system::SafeAreaInsets`) say how much of each edge an app
//! must keep clear. They are one number per side and MERGE the status bar, the navigation bar and
//! the cutout: a punch hole in the middle of the top edge and the clock on its left are one
//! `top`. This module has the SHAPES the insets are made of, so an app can draw into the gaps:
//! a tab strip that takes the status bar's place and slides around the camera, a fingerprint
//! symbol exactly over the sensor.
//!
//! Every coordinate is in logical px of the window (origin top left, as everywhere in azul), as
//! the platform shell reports it after each rotation or change of the window. The shells write
//! [`DisplayFeatures`] next to the insets; an app reads it from `CallbackInfo::get_display_features`
//! or, while it builds the DOM, from `LayoutCallbackInfo::get_display_features`.
//!
//! Where the platform has no honest answer the type says so instead of guessing: a cutout the
//! system did not measure is flagged `approximate`, a fingerprint sensor whose place the system
//! does not tell has no `spot`, and a desktop window has none of it at all ([`DisplayFeatures::none`]).
//!
//! # The test hook
//!
//! `WindowFlags::display_preset` ([`DisplayPreset`]) makes ANY window - a desktop window, a headless
//! window, a phone - behave as if it sat on the display of a Pixel 8 or of an iPhone: the insets
//! and the features come from [`DisplayPreset::safe_area`] and [`DisplayFeatures::simulate`] for
//! the window's current size, so a rotation or a resize of the window moves the simulated hole with
//! it. Host tests and screenshots of a desktop window use it; `Off` (the default) leaves what the
//! shell reports.

use alloc::vec::Vec;

use azul_css::{
    props::basic::pixel::{OptionPixelValue, PixelValue},
    system::SafeAreaInsets,
};

use crate::geom::{LogicalPosition, LogicalRect, LogicalRectVec, LogicalSize};

/// The shape of a cutout (what the system tells, or what the table of the model says).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum CutoutShape {
    /// A round hole in the glass: the punch-hole camera of a Pixel 8. The rect is its bounding
    /// square.
    #[default]
    Circle,
    /// A rectangle with rounded corners.
    RoundedRect,
    /// A pill: a rectangle whose short sides are half circles (the Dynamic Island of an iPhone).
    Pill,
    /// A notch: a cutout open to the edge of the display (the notch of an iPhone 13, a teardrop).
    Notch,
    /// The curved edge of a waterfall display (not a hole: content that runs over the edge is
    /// bent away from the viewer).
    WaterfallEdge,
}

/// One cutout of the display, in window coordinates.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct DisplayCutout {
    /// The bounding rect of the cutout. Android: `DisplayCutout.getBoundingRects()`; iOS: the
    /// table of the model (Apple has no API for the shape).
    pub rect: LogicalRect,
    /// What the cutout looks like inside its rect.
    pub shape: CutoutShape,
    /// `true` when the rect is not measured by the system: it comes from a table of the model or
    /// from a simulation and may be off by a few px. An app that must not draw under the cutout
    /// keeps a margin around an approximate one.
    pub approximate: bool,
}

impl_option!(
    DisplayCutout,
    OptionDisplayCutout,
    [Debug, Copy, Clone, PartialEq]
);

impl_vec!(
    DisplayCutout,
    DisplayCutoutVec,
    DisplayCutoutVecDestructor,
    DisplayCutoutVecDestructorType,
    DisplayCutoutVecSlice,
    OptionDisplayCutout
);
impl_vec_debug!(DisplayCutout, DisplayCutoutVec);
impl_vec_clone!(DisplayCutout, DisplayCutoutVec, DisplayCutoutVecDestructor);
impl_vec_partialeq!(DisplayCutout, DisplayCutoutVec);

/// The radius of each rounded corner of the display, in logical px (0 = a square corner).
#[derive(Debug, Copy, Clone, PartialEq, Default)]
#[repr(C)]
pub struct CornerRadii {
    /// The top left corner.
    pub top_left: f32,
    /// The top right corner.
    pub top_right: f32,
    /// The bottom right corner.
    pub bottom_right: f32,
    /// The bottom left corner.
    pub bottom_left: f32,
}

/// One system bar (the status bar with the clock and the battery, the navigation bar or the home
/// indicator), in window coordinates.
#[derive(Debug, Copy, Clone, PartialEq, Default)]
#[repr(C)]
pub struct SystemBar {
    /// Where the bar is WHEN SHOWN. A bar the app or the system has hidden keeps its rect, so an
    /// app that takes the bar's place (a tab strip in the status bar's band) knows the band.
    pub rect: LogicalRect,
    /// Whether the bar is on screen at the moment.
    pub visible: bool,
}

impl_option!(SystemBar, OptionSystemBar, [Debug, Copy, Clone, PartialEq]);

/// The system bars of the window. `None` where the platform has no such bar (a desktop window).
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct SystemBars {
    /// The status bar: the clock, the battery, the notifications.
    pub status: OptionSystemBar,
    /// The navigation bar of Android (buttons or the gesture pill) or the home indicator of iOS.
    pub navigation: OptionSystemBar,
}

impl Default for SystemBars {
    fn default() -> Self {
        Self::none()
    }
}

impl SystemBars {
    /// No bars.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            status: OptionSystemBar::None,
            navigation: OptionSystemBar::None,
        }
    }
}

/// What kind of fingerprint sensor a device has.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum FingerprintKind {
    /// There is a sensor and the system does not say what kind.
    #[default]
    Unknown,
    /// An optical sensor under the display (the Pixel 8): the finger is put on a place of the
    /// screen the UI can mark.
    UnderDisplayOptical,
    /// An ultrasonic sensor under the display.
    UnderDisplayUltrasonic,
    /// A sensor in the side of the device, usually in the power button.
    SideMounted,
    /// A sensor on the back of the device.
    Rear,
    /// A sensor on the front outside the display (the home button of an older iPhone, Touch ID).
    Front,
}

/// Where a sensor under the display is: a disc in window coordinates.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct FingerprintSpot {
    /// The centre of the sensor's area.
    pub center: LogicalPosition,
    /// The radius of the sensor's area, in logical px.
    pub radius: f32,
}

impl_option!(
    FingerprintSpot,
    OptionFingerprintSpot,
    [Debug, Copy, Clone, PartialEq]
);

/// The fingerprint sensor of the device.
///
/// Android gives the kind where the platform's public API knows it and never the place of an
/// under-display sensor (the call that returns it is hidden): `spot` is `None` then, and an
/// app that wants a symbol puts it above the bottom edge or lets the system prompt draw it.
/// iOS has none (Face ID has no place, Touch ID is a button).
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct FingerprintSensor {
    /// What kind of sensor it is.
    pub kind: FingerprintKind,
    /// Where it is on the display. `None` where the sensor is not under the display, or the
    /// system does not tell.
    pub spot: OptionFingerprintSpot,
}

impl_option!(
    FingerprintSensor,
    OptionFingerprintSensor,
    [Debug, Copy, Clone, PartialEq]
);

/// Everything the display has built in and the system draws over the window: the cutouts, the
/// rounded corners, the system bars, the fingerprint sensor. See the module docs.
///
/// The shell writes it after each rotation and each change of the window; `none()` (and the
/// default) is a window that has none of it.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct DisplayFeatures {
    /// The cutouts, in window coordinates. Empty where there is none.
    pub cutouts: DisplayCutoutVec,
    /// The radii of the display's rounded corners.
    pub corner_radii: CornerRadii,
    /// The system bars.
    pub bars: SystemBars,
    /// The fingerprint sensor, or `None` for a device without one.
    pub fingerprint: OptionFingerprintSensor,
}

impl Default for DisplayFeatures {
    fn default() -> Self {
        Self::none()
    }
}

/// The most cutouts one window takes into account: a display has one to three, anything past
/// the cap is a broken report and is ignored.
pub const MAX_CUTOUTS: usize = 16;

/// A lane narrower than this many logical px is no lane (nothing fits in it).
pub const MIN_LANE_WIDTH: f32 = 1.0;

impl DisplayFeatures {
    /// No cutout, no rounded corner, no bar, no sensor: a desktop window, a headless window.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            cutouts: DisplayCutoutVec::new(),
            corner_radii: CornerRadii {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            },
            bars: SystemBars::none(),
            fingerprint: OptionFingerprintSensor::None,
        }
    }

    /// Whether there is nothing at all.
    #[must_use]
    pub fn is_none(&self) -> bool {
        *self == Self::none()
    }

    /// The cutouts that stand in the horizontal band `top..bottom` (window coordinates): the ones
    /// a row of content in that band has to go around. A report that is not a number is
    /// skipped, and so is anything past [`MAX_CUTOUTS`].
    #[must_use]
    pub fn cutouts_in_band(&self, top: f32, bottom: f32) -> Vec<DisplayCutout> {
        self.cutouts
            .as_ref()
            .iter()
            .take(MAX_CUTOUTS)
            .filter(|c| {
                let r = &c.rect;
                let (x, y, w, h) = (r.origin.x, r.origin.y, r.size.width, r.size.height);
                x.is_finite()
                    && y.is_finite()
                    && w.is_finite()
                    && h.is_finite()
                    && w > 0.0
                    && h > 0.0
                    && y < bottom
                    && y + h > top
            })
            .copied()
            .collect()
    }

    /// The free stretches of the band `top..bottom` between `left` and `right`: the band
    /// minus every cutout in it, each cutout widened by `margin` on both sides. A row that
    /// must not be drawn under the camera lays its content out in these lanes. Left to right,
    /// disjoint, each at least [`MIN_LANE_WIDTH`] wide; one lane the whole band wide where no
    /// cutout is in it.
    #[must_use]
    pub fn free_lanes(
        &self,
        top: f32,
        bottom: f32,
        left: f32,
        right: f32,
        margin: f32,
    ) -> LogicalRectVec {
        let height = (bottom - top).max(0.0);
        let margin = if margin.is_finite() && margin > 0.0 {
            margin
        } else {
            0.0
        };
        let mut gaps: Vec<(f32, f32)> = self
            .cutouts_in_band(top, bottom)
            .iter()
            .map(|c| {
                (
                    c.rect.origin.x - margin,
                    c.rect.origin.x + c.rect.size.width + margin,
                )
            })
            .collect();
        gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut lanes: Vec<LogicalRect> = Vec::new();
        let mut cursor = left;
        for (gap_left, gap_right) in gaps {
            if gap_left - cursor >= MIN_LANE_WIDTH {
                let end = gap_left.min(right);
                if end - cursor >= MIN_LANE_WIDTH {
                    lanes.push(lane(cursor, top, end - cursor, height));
                }
            }
            if gap_right > cursor {
                cursor = gap_right;
            }
        }
        if right - cursor >= MIN_LANE_WIDTH {
            lanes.push(lane(cursor, top, right - cursor, height));
        }
        LogicalRectVec::from_vec(lanes)
    }
}

fn lane(x: f32, y: f32, width: f32, height: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition { x, y }, LogicalSize { width, height })
}

/// A device whose display a window can be made to look like, the test hook of the module
/// (`WindowFlags::display_preset`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum DisplayPreset {
    /// No simulation: the window has what the shell reports (nothing, on a desktop).
    #[default]
    Off,
    /// A Pixel 8: one punch hole in the middle of the top edge, a 48 dp status bar, a gesture
    /// bar, an optical fingerprint sensor under the glass.
    Pixel8,
    /// An iPhone with the Dynamic Island (15 Pro, 16): a pill of 126 x 37 pt at the top, a
    /// 59 pt top inset, the home indicator.
    IPhoneDynamicIsland,
    /// An iPhone with the notch (X to 13): 209 x 30 pt at the top, a 47 pt top inset.
    IPhoneNotch,
    /// An iPhone with a home button (SE): no cutout, a 20 pt status bar, Touch ID.
    IPhoneHomeButton,
    /// A phone with a punch hole in the top corner (the Galaxy S10 family).
    CornerHole,
    /// A phone with two holes in the top edge: the case where a row has three lanes.
    DoubleHole,
}

/// The size of a hole, in logical px.
const HOLE: f32 = 24.0;
/// The height of the status bar of the Android presets, in logical px.
const ANDROID_STATUS: f32 = 48.0;
/// The height of the gesture bar of the Android presets, in logical px.
const ANDROID_NAVIGATION: f32 = 24.0;

fn px(v: f32) -> OptionPixelValue {
    if v.is_finite() && v > 0.0 {
        OptionPixelValue::Some(PixelValue::px(v))
    } else {
        OptionPixelValue::None
    }
}

fn insets(top: f32, bottom: f32, left: f32, right: f32) -> SafeAreaInsets {
    SafeAreaInsets {
        top: px(top),
        bottom: px(bottom),
        left: px(left),
        right: px(right),
        keyboard: OptionPixelValue::None,
    }
}

fn cutout(x: f32, y: f32, w: f32, h: f32, shape: CutoutShape) -> DisplayCutout {
    DisplayCutout {
        rect: lane(x, y, w, h),
        shape,
        approximate: true,
    }
}

fn bar(x: f32, y: f32, w: f32, h: f32) -> OptionSystemBar {
    OptionSystemBar::Some(SystemBar {
        rect: lane(x, y, w, h),
        visible: true,
    })
}

fn all_corners(radius: f32) -> CornerRadii {
    CornerRadii {
        top_left: radius,
        top_right: radius,
        bottom_right: radius,
        bottom_left: radius,
    }
}

fn sensor(kind: FingerprintKind, spot: Option<(f32, f32, f32)>) -> OptionFingerprintSensor {
    OptionFingerprintSensor::Some(FingerprintSensor {
        kind,
        spot: match spot {
            Some((x, y, radius)) => OptionFingerprintSpot::Some(FingerprintSpot {
                center: LogicalPosition { x, y },
                radius,
            }),
            None => OptionFingerprintSpot::None,
        },
    })
}

impl DisplayPreset {
    /// The name the e2e op `set_display_preset` takes.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Pixel8 => "pixel8",
            Self::IPhoneDynamicIsland => "iphone_island",
            Self::IPhoneNotch => "iphone_notch",
            Self::IPhoneHomeButton => "iphone_home_button",
            Self::CornerHole => "corner_hole",
            Self::DoubleHole => "double_hole",
        }
    }

    /// The preset a name stands for (see [`Self::name`]), `None` for any other name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        const ALL: [DisplayPreset; 7] = [
            DisplayPreset::Off,
            DisplayPreset::Pixel8,
            DisplayPreset::IPhoneDynamicIsland,
            DisplayPreset::IPhoneNotch,
            DisplayPreset::IPhoneHomeButton,
            DisplayPreset::CornerHole,
            DisplayPreset::DoubleHole,
        ];
        let wanted = name.trim();
        ALL.iter()
            .copied()
            .find(|p| p.name().eq_ignore_ascii_case(wanted))
    }

    /// Whether this preset simulates a device.
    #[must_use]
    pub const fn is_simulated(self) -> bool {
        !matches!(self, Self::Off)
    }

    /// The safe-area insets of the simulated device for a window of `size`: what its shell would
    /// report. A window wider than tall is the device turned on its side. `Off` has none (the
    /// shell's own are used then). The keyboard inset is never part of a simulation.
    #[must_use]
    pub fn safe_area(self, size: LogicalSize) -> SafeAreaInsets {
        let landscape = size.width > size.height;
        match (self, landscape) {
            (Self::Off, _) | (Self::IPhoneHomeButton, true) => SafeAreaInsets::default(),
            (Self::Pixel8, false) => insets(ANDROID_STATUS, ANDROID_NAVIGATION, 0.0, 0.0),
            (Self::Pixel8, true) => insets(24.0, ANDROID_NAVIGATION, 36.0, 0.0),
            (Self::IPhoneDynamicIsland, false) => insets(59.0, 34.0, 0.0, 0.0),
            (Self::IPhoneDynamicIsland, true) => insets(0.0, 21.0, 59.0, 59.0),
            (Self::IPhoneNotch, false) => insets(47.0, 34.0, 0.0, 0.0),
            (Self::IPhoneNotch, true) => insets(0.0, 21.0, 47.0, 47.0),
            (Self::IPhoneHomeButton, false) => insets(20.0, 0.0, 0.0, 0.0),
            (Self::CornerHole, false) | (Self::DoubleHole, false) => {
                insets(40.0, ANDROID_NAVIGATION, 0.0, 0.0)
            }
            (Self::CornerHole, true) | (Self::DoubleHole, true) => {
                insets(24.0, ANDROID_NAVIGATION, 40.0, 0.0)
            }
        }
    }
}

impl DisplayFeatures {
    /// The features of the device `preset` names, for a window of `size` (logical px). A window
    /// wider than tall is the device on its side: the hole moves to the left edge, as Android
    /// and iOS turn it. Every cutout is flagged `approximate`. `Off` is [`Self::none`].
    #[must_use]
    pub fn simulate(preset: DisplayPreset, size: LogicalSize) -> Self {
        match preset {
            DisplayPreset::Off => Self::none(),
            DisplayPreset::Pixel8 => pixel_8(size),
            DisplayPreset::IPhoneDynamicIsland => iphone_island(size),
            DisplayPreset::IPhoneNotch => iphone_notch(size),
            DisplayPreset::IPhoneHomeButton => iphone_home_button(size),
            DisplayPreset::CornerHole => corner_hole(size),
            DisplayPreset::DoubleHole => double_hole(size),
        }
    }
}

/// `features` with `cutouts` as its cutouts.
fn with_cutouts(mut features: DisplayFeatures, cutouts: Vec<DisplayCutout>) -> DisplayFeatures {
    features.cutouts = DisplayCutoutVec::from_vec(cutouts);
    features
}

/// The bars every Android preset has: `status` tall at the top, the gesture bar at the bottom.
fn android_bars(size: LogicalSize, status: f32) -> SystemBars {
    SystemBars {
        status: bar(0.0, 0.0, size.width, status),
        navigation: bar(
            0.0,
            size.height - ANDROID_NAVIGATION,
            size.width,
            ANDROID_NAVIGATION,
        ),
    }
}

fn pixel_8(size: LogicalSize) -> DisplayFeatures {
    let (w, h) = (size.width, size.height);
    let half = HOLE / 2.0;
    let landscape = w > h;
    let mut features = DisplayFeatures::none();
    features.corner_radii = all_corners(40.0);
    features.bars = android_bars(size, if landscape { 24.0 } else { ANDROID_STATUS });
    let hole = if landscape {
        // On its side the camera is at the left edge, half way down.
        features.fingerprint = sensor(
            FingerprintKind::UnderDisplayOptical,
            Some((w - 130.0, h / 2.0, 28.0)),
        );
        cutout(12.0, h / 2.0 - half, HOLE, HOLE, CutoutShape::Circle)
    } else {
        features.fingerprint = sensor(
            FingerprintKind::UnderDisplayOptical,
            Some((w / 2.0, h - 130.0, 28.0)),
        );
        cutout(w / 2.0 - half, 12.0, HOLE, HOLE, CutoutShape::Circle)
    };
    with_cutouts(features, vec![hole])
}

fn iphone_island(size: LogicalSize) -> DisplayFeatures {
    let (w, h) = (size.width, size.height);
    let mut features = DisplayFeatures::none();
    features.corner_radii = all_corners(55.0);
    if w > h {
        // The status bar is hidden on an iPhone on its side.
        features.bars.navigation = bar(0.0, h - 21.0, w, 21.0);
        let island = cutout(11.0, h / 2.0 - 63.0, 37.0, 126.0, CutoutShape::Pill);
        with_cutouts(features, vec![island])
    } else {
        features.bars.status = bar(0.0, 0.0, w, 54.0);
        features.bars.navigation = bar(0.0, h - 34.0, w, 34.0);
        let island = cutout(w / 2.0 - 63.0, 11.0, 126.0, 37.0, CutoutShape::Pill);
        with_cutouts(features, vec![island])
    }
}

fn iphone_notch(size: LogicalSize) -> DisplayFeatures {
    let (w, h) = (size.width, size.height);
    let mut features = DisplayFeatures::none();
    features.corner_radii = all_corners(47.0);
    if w > h {
        features.bars.navigation = bar(0.0, h - 21.0, w, 21.0);
        let notch = cutout(0.0, h / 2.0 - 104.5, 30.0, 209.0, CutoutShape::Notch);
        with_cutouts(features, vec![notch])
    } else {
        features.bars.status = bar(0.0, 0.0, w, 47.0);
        features.bars.navigation = bar(0.0, h - 34.0, w, 34.0);
        let notch = cutout(w / 2.0 - 104.5, 0.0, 209.0, 30.0, CutoutShape::Notch);
        with_cutouts(features, vec![notch])
    }
}

fn iphone_home_button(size: LogicalSize) -> DisplayFeatures {
    let mut features = DisplayFeatures::none();
    if size.width <= size.height {
        features.bars.status = bar(0.0, 0.0, size.width, 20.0);
    }
    features.fingerprint = sensor(FingerprintKind::Front, None);
    features
}

fn corner_hole(size: LogicalSize) -> DisplayFeatures {
    let (w, h) = (size.width, size.height);
    let landscape = w > h;
    let mut features = DisplayFeatures::none();
    features.corner_radii = all_corners(36.0);
    features.bars = android_bars(size, if landscape { 24.0 } else { 40.0 });
    features.fingerprint = sensor(
        FingerprintKind::UnderDisplayUltrasonic,
        Some((w / 2.0, h - 140.0, 24.0)),
    );
    let hole = if landscape {
        cutout(12.0, 12.0, 28.0, 28.0, CutoutShape::Circle)
    } else {
        cutout(w - 56.0, 12.0, 28.0, 28.0, CutoutShape::Circle)
    };
    with_cutouts(features, vec![hole])
}

fn double_hole(size: LogicalSize) -> DisplayFeatures {
    let (w, h) = (size.width, size.height);
    let half = HOLE / 2.0;
    let landscape = w > h;
    let mut features = DisplayFeatures::none();
    features.corner_radii = all_corners(36.0);
    features.bars = android_bars(size, if landscape { 24.0 } else { 40.0 });
    let holes = if landscape {
        vec![
            cutout(12.0, h / 2.0 - 90.0 - half, HOLE, HOLE, CutoutShape::Circle),
            cutout(12.0, h / 2.0 + 90.0 - half, HOLE, HOLE, CutoutShape::Circle),
        ]
    } else {
        vec![
            cutout(w / 2.0 - 90.0 - half, 12.0, HOLE, HOLE, CutoutShape::Circle),
            cutout(w / 2.0 + 90.0 - half, 12.0, HOLE, HOLE, CutoutShape::Circle),
        ]
    };
    with_cutouts(features, holes)
}

/// How the app wants a system bar shown ([`SystemBarsOptions`]).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum SystemBarMode {
    /// Shown, as the system has it (the default).
    #[default]
    Visible,
    /// Hidden. A swipe from the edge of the screen shows the bar for a moment and it hides again
    /// by itself (Android `BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE`).
    HiddenTransient,
    /// Hidden. A swipe from the edge shows the bar and it stays until the app or the user hides
    /// it again (Android `BEHAVIOR_DEFAULT` after `hide()`). iOS has no such mode and treats it
    /// as `HiddenTransient`.
    HiddenSticky,
}

/// The colour of the icons the system draws in the status bar and the navigation bar.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum SystemBarIcons {
    /// The shell picks from what the window draws behind the bar (the default).
    #[default]
    Auto,
    /// Light icons, for a dark background.
    Light,
    /// Dark icons, for a light background.
    Dark,
}

/// What the app asks of the system bars and the cutout area (`WindowFlags::system_bars`).
///
/// A tab strip that takes the place of the clock and the battery icons asks for
/// `status: HiddenTransient` and `draw_into_cutout: true` (and `extend_into_safe_area`, so that
/// the layout starts at the top edge of the window).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub struct SystemBarsOptions {
    /// The status bar.
    pub status: SystemBarMode,
    /// The navigation bar (Android) or the home indicator (iOS).
    pub navigation: SystemBarMode,
    /// The colour of the icons in the bars.
    pub icons: SystemBarIcons,
    /// Let the window draw into the area of the cutouts: Android
    /// `LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS` (`SHORT_EDGES` before Android 11), edge to edge.
    /// `false`: the system's default for the window (the area stays black in landscape).
    pub draw_into_cutout: bool,
}

impl SystemBarsOptions {
    /// Whether the status bar is hidden by the app.
    #[must_use]
    pub const fn status_hidden(&self) -> bool {
        !matches!(self.status, SystemBarMode::Visible)
    }
}

#[cfg(test)]
#[path = "display_features_test.rs"]
mod display_features_test;
