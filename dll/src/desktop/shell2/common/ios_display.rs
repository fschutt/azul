//! What an iPhone's display has built in, in the window's own coordinates, without UIKit: the cutout (the notch, the
//! Dynamic Island), the rounded corners, the bars, Touch ID's button. Host-compiled and tested like `ios_logic`;
//! `shell2/ios` turns the answer into the engine's `DisplayFeatures` and hands it to the layout window next to the
//! safe-area insets.
//!
//! WHAT iOS TELLS AND WHAT IT DOES NOT. `UIView.safeAreaInsets` is the one number the system gives: how far the content
//! must stay from each edge. The SHAPE of the notch or the island, the radius of the display's corners and the bars'
//! frames are not public API (`UIScreen._displayCornerRadius` is private and is not used: no private API in a shipped
//! app). So the shape comes from a table of the display classes, found by the screen's size in points (the portrait
//! size names the class: 402 x 874 is an iPhone 16 Pro), and every cutout is reported `approximate` so that an app keeps
//! a margin around it. Where the size is in no row, the safe area still tells that there IS a cutout (a top inset of 40
//! points or more on a phone), and the island of the common sizes stands in for it: better a margin around a camera
//! that is not there than a tab under one that is.
//!
//! The measured numbers are used where UIKit gives them: the home indicator is the bottom inset (21 points on a phone on
//! its side, 34 upright), the status bar of an iPad is the top inset.

/// The kind of cutout a class of iPhone has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IosCutout {
    /// A display without a cutout (the home button models, the iPads).
    None,
    /// The notch of the iPhone X to 14: open to the top edge.
    Notch,
    /// The Dynamic Island of the iPhone 14 Pro and later: a pill below the top edge.
    Island,
}

/// The fingerprint sensor of the device, as far as iOS lets anyone tell: Face ID has none, a home button phone has the
/// button below the display, and a device with Touch ID that is not such a phone (an iPad) has one whose place and kind
/// the system does not say. No sensor of an iPhone is ever under the glass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IosFingerprint {
    /// No fingerprint sensor (Face ID, or nothing).
    None,
    /// The Touch ID button below the display of a home button phone.
    Front,
    /// Touch ID, and the system does not say where or what kind (an iPad's top button or home button).
    Unknown,
}

/// A rectangle in the window's coordinates (points, origin top left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IosRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// One class of display of an iPhone, found by the portrait size of the screen in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IosDisplayClass {
    /// The short side of the screen in points (the width, held upright).
    pub short: f32,
    /// The long side of the screen in points (the height, held upright).
    pub long: f32,
    /// What the display has in its top edge.
    pub cutout: IosCutout,
    /// The width and the height of the cutout when the phone stands upright, points. Where several models share the
    /// size (the notch of an iPhone 12 is 209 x 30, that of an iPhone 13 is narrower and taller) the table keeps the
    /// larger of each: a margin too wide costs nothing, a tab under the camera does.
    pub cutout_size: (f32, f32),
    /// How far the cutout is from the top edge of the glass (0 for a notch).
    pub cutout_top: f32,
    /// The radius of the display's corners, points.
    pub corner_radius: f32,
    /// The height of the status bar upright, points.
    pub status_height: f32,
    /// Whether the phone has a home button with Touch ID (no Face ID, no cutout).
    pub home_button: bool,
}

/// One row of the table of display classes: the eight numbers read left to right in `IOS_DISPLAY_CLASSES`.
#[allow(clippy::too_many_arguments)]
const fn class(
    short: f32,
    long: f32,
    cutout: IosCutout,
    cutout_size: (f32, f32),
    cutout_top: f32,
    corner_radius: f32,
    status_height: f32,
    home_button: bool,
) -> IosDisplayClass {
    IosDisplayClass {
        short,
        long,
        cutout,
        cutout_size,
        cutout_top,
        corner_radius,
        status_height,
        home_button,
    }
}

/// The display classes of the iPhones iOS 17 and later runs on, by portrait size in points. The numbers are the
/// community's measurements of the hardware (Apple publishes the sizes of the screens, not the shape of the cutout):
/// the island is 126 x 37 points 11 points below the top edge (125 x 37, 14 points down on the 16 Pro), the widest notch
/// 209 x 30, the corner radii 39 (the X family) to 62 (the 16 Pro).
pub static IOS_DISPLAY_CLASSES: [IosDisplayClass; 12] = [
    // Dynamic Island
    class(440.0, 956.0, IosCutout::Island, (125.0, 37.0), 14.0, 62.0, 54.0, false), // 16 Pro Max
    class(402.0, 874.0, IosCutout::Island, (125.0, 37.0), 14.0, 62.0, 54.0, false), // 16 Pro
    class(430.0, 932.0, IosCutout::Island, (126.0, 37.0), 11.0, 55.0, 54.0, false), // 14 Pro Max, 15 Plus, 15 Pro Max, 16 Plus
    class(393.0, 852.0, IosCutout::Island, (126.0, 37.0), 11.0, 55.0, 54.0, false), // 14 Pro, 15, 15 Pro, 16
    // the notch
    class(428.0, 926.0, IosCutout::Notch, (209.0, 34.0), 0.0, 53.0, 47.0, false), // 12 Pro Max, 13 Pro Max, 14 Plus
    class(390.0, 844.0, IosCutout::Notch, (209.0, 34.0), 0.0, 47.0, 47.0, false), // 12, 12 Pro, 13, 13 Pro, 14
    class(375.0, 812.0, IosCutout::Notch, (209.0, 34.0), 0.0, 44.0, 44.0, false), // X, XS, 11 Pro, 12 mini, 13 mini
    class(414.0, 896.0, IosCutout::Notch, (209.0, 30.0), 0.0, 41.0, 48.0, false), // XR, 11, XS Max, 11 Pro Max
    // the home button (Touch ID)
    class(375.0, 667.0, IosCutout::None, (0.0, 0.0), 0.0, 0.0, 20.0, true), // SE (2nd, 3rd generation), 8, 7, 6s, 6
    class(414.0, 736.0, IosCutout::None, (0.0, 0.0), 0.0, 0.0, 20.0, true), // 8 Plus, 7 Plus, 6s Plus, 6 Plus
    class(320.0, 568.0, IosCutout::None, (0.0, 0.0), 0.0, 0.0, 20.0, true), // SE (1st generation), 5s
    class(320.0, 480.0, IosCutout::None, (0.0, 0.0), 0.0, 0.0, 20.0, true), // 4s and older: no longer run, kept for a simulator
];

/// A top inset of this many points or more on a phone says the display has a cutout whatever its size is.
pub const CUTOUT_INSET_POINTS: f32 = 40.0;

/// The tolerance of a size match: a Simulator window or a zoomed display differs by a fraction of a point.
const SIZE_TOLERANCE_POINTS: f32 = 0.6;

/// The class of an iPhone's display for a window of `width` x `height` points (either orientation), or `None` for a
/// size in no row.
#[must_use]
pub fn display_class(width: f32, height: f32) -> Option<&'static IosDisplayClass> {
    let (short, long) = if width <= height { (width, height) } else { (height, width) };
    IOS_DISPLAY_CLASSES
        .iter()
        .find(|c| (c.short - short).abs() <= SIZE_TOLERANCE_POINTS && (c.long - long).abs() <= SIZE_TOLERANCE_POINTS)
}

/// The class standing in for a phone whose size is in no row but whose safe area says it has a cutout: the island of
/// the common sizes, with the corner radius of the larger phones.
pub static UNKNOWN_CUTOUT_CLASS: IosDisplayClass =
    class(0.0, 0.0, IosCutout::Island, (126.0, 37.0), 11.0, 55.0, 54.0, false);

/// What the display has, in the window's coordinates, for the window's current orientation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IosDisplay {
    /// The cutout, when the display has one: its bounding rect and what it looks like. Always approximate.
    pub cutout: Option<(IosRect, IosCutout)>,
    /// The radius of every corner of the display, points (0: square corners, or not known).
    pub corner_radius: f32,
    /// The status bar's rect WHEN SHOWN (an iPhone on its side has none: `None`).
    pub status_bar: Option<IosRect>,
    /// The home indicator's rect (the bottom inset), `None` where there is none.
    pub home_indicator: Option<IosRect>,
    /// The fingerprint sensor: never a place on the glass.
    pub fingerprint: IosFingerprint,
}

/// How the window is turned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// Upright (or upside down on an iPad).
    Portrait,
    /// On its side with the top of the phone (the cutout) at the LEFT edge of the screen:
    /// `UIInterfaceOrientationLandscapeRight` (the home button is on the right).
    CutoutLeft,
    /// On its side with the top of the phone at the RIGHT edge: `UIInterfaceOrientationLandscapeLeft`.
    CutoutRight,
}

/// The turn of the window from its size and, where UIKit says it, the interface orientation (`1` upright, `2` upside
/// down, `3` landscape right, `4` landscape left); `0` or any other number: unknown. A window wider than tall that
/// does not say which side takes the cutout at the left.
#[must_use]
pub const fn turn_of(width: f32, height: f32, interface_orientation: i64) -> Turn {
    if width <= height {
        return Turn::Portrait;
    }
    match interface_orientation {
        4 => Turn::CutoutRight,
        _ => Turn::CutoutLeft,
    }
}

/// The display of the phone (or iPad) the window sits on: the cutout, the corners, the bars. `insets` are UIKit's
/// `safeAreaInsets` as the shell read them (top, right, bottom, left, points); `is_pad` is
/// `userInterfaceIdiom == iPad` (an iPad has no cutout and no corner radius that is public); `touch_id_hardware` is
/// `LAContext.biometryType == .touchID` (asked once, quietly: it never shows a sheet).
#[must_use]
pub fn ios_display(
    width: f32,
    height: f32,
    insets: (f32, f32, f32, f32),
    is_pad: bool,
    touch_id_hardware: bool,
    turn: Turn,
) -> IosDisplay {
    let (top, right, bottom, left) = insets;
    let side = left.max(right);
    let inset_says_cutout = !is_pad && (top >= CUTOUT_INSET_POINTS || side >= CUTOUT_INSET_POINTS);
    let known = if is_pad { None } else { display_class(width, height) };
    let class = known.or(if inset_says_cutout { Some(&UNKNOWN_CUTOUT_CLASS) } else { None });
    let (cutout, corner_radius, status_height, home_button) = match class {
        Some(c) => (cutout_rect(c, width, height, turn), c.corner_radius, c.status_height, c.home_button),
        None => (None, 0.0, top, false),
    };
    let fingerprint = if home_button {
        IosFingerprint::Front
    } else if touch_id_hardware {
        IosFingerprint::Unknown
    } else {
        IosFingerprint::None
    };
    // The status bar: an iPhone's is only there upright (on its side it is hidden and the top inset is 0); an iPad's
    // is the top inset.
    let status_bar = if top > 0.0 {
        let height_of_bar = if is_pad { top } else { status_height };
        Some(IosRect { x: 0.0, y: 0.0, width, height: height_of_bar })
    } else {
        None
    };
    let home_indicator = if bottom > 0.0 {
        Some(IosRect { x: 0.0, y: (height - bottom).max(0.0), width, height: bottom })
    } else {
        None
    };
    IosDisplay { cutout, corner_radius, status_bar, home_indicator, fingerprint }
}

/// The cutout's bounding rect for the window's turn: centred on the top edge upright, centred on the left or the right
/// edge on its side (the phone's top is where the cutout is, and the phone's top is that edge).
fn cutout_rect(class: &IosDisplayClass, width: f32, height: f32, turn: Turn) -> Option<(IosRect, IosCutout)> {
    if class.cutout == IosCutout::None {
        return None;
    }
    let (w, h) = class.cutout_size;
    let rect = match turn {
        Turn::Portrait => IosRect { x: width / 2.0 - w / 2.0, y: class.cutout_top, width: w, height: h },
        Turn::CutoutLeft => IosRect { x: class.cutout_top, y: height / 2.0 - w / 2.0, width: h, height: w },
        Turn::CutoutRight => IosRect {
            x: width - class.cutout_top - h,
            y: height / 2.0 - w / 2.0,
            width: h,
            height: w,
        },
    };
    Some((rect, class.cutout))
}

/// What the view controller answers UIKit with about the system's chrome (`prefersStatusBarHidden`,
/// `preferredStatusBarStyle`, `prefersHomeIndicatorAutoHidden`, `preferredScreenEdgesDeferringSystemGestures`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IosSystemUi {
    /// `prefersStatusBarHidden`.
    pub status_bar_hidden: bool,
    /// `preferredStatusBarStyle`: [`STATUS_BAR_STYLE_DEFAULT`], [`STATUS_BAR_STYLE_LIGHT`] or [`STATUS_BAR_STYLE_DARK`].
    pub status_bar_style: i64,
    /// `prefersHomeIndicatorAutoHidden`.
    pub home_indicator_hidden: bool,
    /// `preferredScreenEdgesDeferringSystemGestures`: a `UIRectEdge` bit set.
    pub deferred_edges: u64,
}

/// `UIStatusBarStyleDefault`: dark or light by the interface style, the system's choice.
pub const STATUS_BAR_STYLE_DEFAULT: i64 = 0;
/// `UIStatusBarStyleLightContent`: light icons, for a dark background.
pub const STATUS_BAR_STYLE_LIGHT: i64 = 1;
/// `UIStatusBarStyleDarkContent` (iOS 13): dark icons, for a light background.
pub const STATUS_BAR_STYLE_DARK: i64 = 3;
/// `UIRectEdgeTop`.
pub const EDGE_TOP: u64 = 1;
/// `UIRectEdgeBottom`.
pub const EDGE_BOTTOM: u64 = 4;

/// What the app's window options come to on iOS. A hidden bar is what the app asked for, and a gesture that would have
/// shown it needs a SECOND swipe: the system gesture of that edge is deferred (the first swipe shows the bar for a
/// moment, the second is the system's own), the iOS spelling of "hidden, a swipe reveals it". `icons`: `0` the
/// system's choice, `1` light, `2` dark.
#[must_use]
pub const fn system_ui_of(status_hidden: bool, navigation_hidden: bool, icons: u8) -> IosSystemUi {
    let mut deferred_edges = 0;
    if status_hidden {
        deferred_edges |= EDGE_TOP;
    }
    if navigation_hidden {
        deferred_edges |= EDGE_BOTTOM;
    }
    IosSystemUi {
        status_bar_hidden: status_hidden,
        status_bar_style: match icons {
            1 => STATUS_BAR_STYLE_LIGHT,
            2 => STATUS_BAR_STYLE_DARK,
            _ => STATUS_BAR_STYLE_DEFAULT,
        },
        home_indicator_hidden: navigation_hidden,
        deferred_edges,
    }
}

#[cfg(test)]
#[path = "ios_display_test.rs"]
mod ios_display_test;
