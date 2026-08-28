//! System tray / status icon  -  platform-agnostic model.
//!
//! The actual OS plumbing lives in `azul-dll` (`desktop/tray/`). This module
//! only defines the data the three backends agree on.
//!
//! # Why the API has this shape
//!
//! The three platforms share almost nothing below the "icon + retained menu
//! tree" level, and two of their constraints leak into any honest API:
//!
//! 1. **On Linux the menu is not a popup  -  it is a remote model.** SNI's `Menu`
//!    property points at a `com.canonical.dbusmenu` object, and the *panel*
//!    draws the menu, calling back into us with `GetLayout` / `AboutToShow`.
//!    So the menu must be a RETAINED tree with stable ids and a revision
//!    counter. An API shaped as `show_context_menu_at(x, y)` cannot be
//!    implemented on Linux and would have to be redone  -  hence
//!    [`TrayIconData::menu`] is state, not a call.
//!
//! 2. **`ContextMenu` is a REQUEST, not a command.** On Linux the panel may
//!    open the menu itself and never tell us; on Windows and macOS we open it.
//!    Callers must not assume their handler is the only thing that runs.
//!
//! Two more platform truths that the API deliberately does NOT hide:
//!
//! * **A tray may genuinely not exist.** On a vanilla GNOME there is no
//!   `org.kde.StatusNotifierWatcher` at all: registration fails silently and no
//!   icon ever appears. [`TrayIconData`] is therefore accepted on a best-effort
//!   basis and the app must have a story for "no tray"  -  see
//!   `App::tray_available()` in azul-dll.
//! * **Click semantics differ.** The SNI spec does not say which gesture
//!   activates an item; some desktops use single left click, some double. Never
//!   document a precise gesture for [`TrayEventType::Activate`].

use alloc::{string::String, vec::Vec};

use crate::{
    menu::{Menu, OptionMenu},
    window::IconKey,
};
use azul_css::{corety::U8Vec, AzString, OptionString};

/// RGBA8 image for a tray icon, at one specific size.
///
/// Unlike [`crate::window::WindowIcon`], which is fixed at 16x16 / 32x32, a
/// tray icon needs arbitrary sizes: Windows wants
/// `GetSystemMetricsForDpi(SM_CXSMICON)` (16 / 20 / 24 / 32 px as the taskbar's
/// DPI changes), macOS wants an 18x18 *point* template image (so 36x36 px on a
/// 2x display), and SNI wants an array of whatever sizes we care to publish so
/// the panel can pick.
///
/// `rgba` is straight, non-premultiplied RGBA8, `width * height * 4` bytes, top
/// row first. Every backend converts from this one representation:
/// Windows builds a BGRA `HBITMAP` + mask, macOS an `NSBitmapImageRep`, and
/// Linux byte-swaps to the ARGB32-big-endian that `IconPixmap` requires.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TrayIconImage {
    /// Cache key  -  lets a backend skip re-uploading an unchanged icon.
    pub key: IconKey,
    pub width: u32,
    pub height: u32,
    pub rgba: U8Vec,
}

impl PartialEq for TrayIconImage {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl Eq for TrayIconImage {}

impl TrayIconImage {
    /// `rgba` must be exactly `width * height * 4` bytes; returns `None`
    /// otherwise.
    ///
    /// Returns `OptionTrayIconImage` rather than `Option<Self>` so the
    /// signature crosses the C ABI unchanged.
    #[must_use]
    #[allow(
        clippy::new_ret_no_self,
        reason = "C-ABI: must return OptionTrayIconImage"
    )]
    pub fn new(width: u32, height: u32, rgba: U8Vec) -> OptionTrayIconImage {
        if width == 0 || height == 0 {
            return OptionTrayIconImage::None;
        }
        let Some(expected) = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
        else {
            return OptionTrayIconImage::None;
        };
        if rgba.as_ref().len() != expected {
            return OptionTrayIconImage::None;
        }
        OptionTrayIconImage::Some(Self {
            key: IconKey::new(),
            width,
            height,
            rgba,
        })
    }

    /// The icon's pixels as ARGB32 in **network (big-endian) byte order**, the
    /// wire format `org.kde.StatusNotifierItem`'s `IconPixmap` (`a(iiay)`)
    /// requires. Nothing else uses this layout, so it is computed on demand.
    #[must_use]
    pub fn to_argb32_be(&self) -> U8Vec {
        let src = self.rgba.as_ref();
        let mut out = Vec::with_capacity(src.len());
        for px in src.chunks_exact(4) {
            // RGBA -> ARGB, big-endian == [A, R, G, B] in memory order.
            out.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
        }
        U8Vec::from_vec(out)
    }
}

impl_option!(
    TrayIconImage,
    OptionTrayIconImage,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl_vec!(
    TrayIconImage,
    TrayIconImageVec,
    TrayIconImageVecDestructor,
    TrayIconImageVecDestructorType,
    TrayIconImageVecSlice,
    OptionTrayIconImage
);
impl_vec_debug!(TrayIconImage, TrayIconImageVec);
impl_vec_clone!(TrayIconImage, TrayIconImageVec, TrayIconImageVecDestructor);
impl_vec_partialeq!(TrayIconImage, TrayIconImageVec);

/// Where a tray icon's pixels come from.
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
#[derive(Default)]
pub enum TrayIconSource {
    /// No icon. Most desktops render this as an invisible item, so it is
    /// almost never what you want.
    #[default]
    None,
    /// Explicit RGBA bitmaps, ideally at several sizes so each platform can
    /// pick  -  see [`TrayIconData::best_icon`].
    ///
    /// Only needed for an icon that genuinely is not in a pack  -  typically one
    /// generated at runtime, since the icon registry is frozen once the
    /// provider is shared (`App::run` consumes the handle).
    Rgba(TrayIconImageVec),
    /// An **icon spec**  -  exactly the string an `<icon>` node takes: a bare
    /// name (`"settings"`), a pack-qualified name (`"mypack:logo"`), or a
    /// comma-separated fallback list (`"mypack:logo, settings"`).
    ///
    /// This is the preferred form, for two reasons.
    ///
    /// It resolves through the SAME registry and resolver `<icon>` DOM nodes
    /// use, so anything registered there works with no tray-specific icon
    /// path: Material Icons (the default pack), an image pack loaded from a
    /// ZIP, or a custom resolver. Because resolution yields a `StyledDom`
    /// which is then
    /// rendered, an icon can be anything expressible as a DOM: a font glyph, a
    /// bitmap, later an SVG or an emoji.
    ///
    /// And a spec can be rendered at ANY size on demand, which a fixed bitmap
    /// cannot: a tray needs the same icon at several sizes and cannot know
    /// them up front (Windows re-asks at every taskbar DPI: 16/20/24/32; macOS
    /// wants 18pt, which is 36px on a 2x display; SNI publishes an array).
    Named(AzString),
}

/// Hint about what the tray item represents. Maps to SNI's `Category`; Windows
/// and macOS have no equivalent and ignore it.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum TrayCategory {
    #[default]
    ApplicationStatus,
    Communications,
    SystemServices,
    Hardware,
}

impl TrayCategory {
    /// The exact string the SNI `Category` property expects.
    #[must_use]
    pub const fn sni_name(self) -> &'static str {
        match self {
            Self::ApplicationStatus => "ApplicationStatus",
            Self::Communications => "Communications",
            Self::SystemServices => "SystemServices",
            Self::Hardware => "Hardware",
        }
    }
}

/// Attention state. Maps to SNI's `Status`.
///
/// On Windows and macOS only `Passive` is meaningful (it hides the icon);
/// `Active` and `NeedsAttention` both simply show it, because neither platform
/// has a "demanding attention" tray state.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum TrayStatus {
    /// The host MAY hide the item.
    Passive,
    #[default]
    Active,
    /// Draws attention; SNI hosts swap in `AttentionIcon`.
    NeedsAttention,
}

impl TrayStatus {
    #[must_use]
    pub const fn sni_name(self) -> &'static str {
        match self {
            Self::Passive => "Passive",
            Self::Active => "Active",
            Self::NeedsAttention => "NeedsAttention",
        }
    }
}

/// What the user did to the tray icon.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum TrayEventType {
    /// Primary activation. **Do not document a precise gesture**: it is left
    /// click or keyboard Enter on Windows, a click on macOS, and entirely
    /// desktop-dependent on Linux (the SNI spec does not specify it).
    Activate,
    /// Middle click on Windows/Linux; not emitted on macOS.
    SecondaryActivate,
    /// The user asked for the context menu. On Windows and macOS we then open
    /// it; **on Linux the panel already opened it itself** from the exported
    /// dbusmenu, so this is informational there.
    ContextMenu,
    /// Scroll wheel over the icon. Linux only (SNI `Scroll`); Windows and
    /// macOS never emit it.
    Scroll,
    /// A menu item was chosen. Carries the item's command id.
    MenuItem,
}

/// Scroll axis for [`TrayEventType::Scroll`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum TrayScrollAxis {
    #[default]
    Vertical,
    Horizontal,
}

/// One thing that happened on the tray icon.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TrayEvent {
    pub kind: TrayEventType,
    /// Set for [`TrayEventType::MenuItem`]; the chosen item's command id.
    pub menu_command: u32,
    /// Set for [`TrayEventType::Scroll`].
    pub scroll_delta: i32,
    pub scroll_axis: TrayScrollAxis,
}

impl TrayEvent {
    #[must_use]
    pub const fn simple(kind: TrayEventType) -> Self {
        Self {
            kind,
            menu_command: 0,
            scroll_delta: 0,
            scroll_axis: TrayScrollAxis::Vertical,
        }
    }
    #[must_use]
    pub const fn menu_item(command: u32) -> Self {
        Self {
            kind: TrayEventType::MenuItem,
            menu_command: command,
            scroll_delta: 0,
            scroll_axis: TrayScrollAxis::Vertical,
        }
    }
}

impl_option!(
    TrayEvent,
    OptionTrayEvent,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Everything a tray icon shows.
///
/// This is *state*: set it, mutate it, and the backend republishes. It is
/// deliberately not a set of imperative calls, because the Linux backend has
/// to be able to answer the panel's `GetLayout` at any moment.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct TrayIconData {
    /// Stable identifier for this app's tray item. Used as SNI's `Id`, and as
    /// the seed for macOS's `NSStatusItem.autosaveName` (which is what makes
    /// the item keep its position in the menu bar across launches).
    ///
    /// Must be stable across runs and must NOT be derived from a path or pid.
    /// Reverse-DNS is the convention: `"org.example.myapp"`.
    pub id: AzString,
    /// Human-readable application name (SNI `Title`).
    pub title: AzString,
    /// Where the icon's pixels come from  -  a named icon-pack entry (preferred)
    /// or explicit RGBA bitmaps.
    pub icon: TrayIconSource,
    /// Shown when `status == NeedsAttention` on SNI hosts. Ignored elsewhere.
    pub attention_icon: TrayIconSource,
    pub tooltip: OptionString,
    pub category: TrayCategory,
    pub status: TrayStatus,
    /// The retained context menu. `None` means "no menu": on Windows and macOS
    /// a context-menu request then just reports [`TrayEventType::ContextMenu`],
    /// and on Linux the `Menu` property is left unset and hosts fall back to
    /// calling `ContextMenu()`.
    pub menu: OptionMenu,
}

impl Default for TrayIconData {
    fn default() -> Self {
        Self {
            id: AzString::from_const_str("azul.tray"),
            title: AzString::from_const_str(""),
            icon: TrayIconSource::None,
            attention_icon: TrayIconSource::None,
            tooltip: OptionString::None,
            category: TrayCategory::ApplicationStatus,
            status: TrayStatus::Active,
            menu: OptionMenu::None,
        }
    }
}

impl TrayIconData {
    #[must_use]
    pub fn new(id: AzString, title: AzString) -> Self {
        Self {
            id,
            title,
            ..Self::default()
        }
    }

    /// Use an icon from the icon registry, by the same spec an `<icon>` node
    /// takes  -  `"settings"`, `"mypack:logo"`, or a fallback list. Preferred:
    /// it renders at whatever size each platform asks for.
    #[must_use]
    pub fn with_named_icon(mut self, spec: AzString) -> Self {
        self.icon = TrayIconSource::Named(spec);
        self
    }

    /// Use an explicit RGBA bitmap. Prefer [`Self::with_named_icon`] unless the
    /// icon genuinely is not in a pack.
    #[must_use]
    pub fn with_icon(mut self, icon: TrayIconImage) -> Self {
        self.icon = TrayIconSource::Rgba(TrayIconImageVec::from_vec(alloc::vec![icon]));
        self
    }

    #[must_use]
    pub fn with_menu(mut self, menu: Menu) -> Self {
        self.menu = OptionMenu::Some(menu);
        self
    }

    #[must_use]
    pub fn with_tooltip(mut self, tooltip: AzString) -> Self {
        self.tooltip = OptionString::Some(tooltip);
        self
    }

    /// The icon closest to `target_px`, preferring the smallest one that is at
    /// least `target_px` (upscaling a small icon looks far worse than
    /// downscaling a large one  -  this is why Windows' own `LoadIconMetric`
    /// scales down from a larger frame rather than up).
    /// Only meaningful for [`TrayIconSource::Rgba`]; a `Named` icon is
    /// rasterized at the exact size instead, so it never needs picking.
    ///
    /// Returns an owned `OptionTrayIconImage` rather than `Option<&_>` because
    /// a borrow cannot cross the C ABI. The clone is one `U8Vec` bump.
    #[must_use]
    pub fn best_icon(&self, target_px: u32) -> OptionTrayIconImage {
        let TrayIconSource::Rgba(ref icons) = self.icon else {
            return OptionTrayIconImage::None;
        };
        let icons = icons.as_ref();
        icons
            .iter()
            .filter(|i| i.width >= target_px)
            .min_by_key(|i| i.width)
            .or_else(|| icons.iter().max_by_key(|i| i.width))
            .cloned()
            .into()
    }
}


#[cfg(test)]
#[path = "tray_test.rs"]
mod tray_test;
