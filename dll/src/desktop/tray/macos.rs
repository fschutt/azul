//! macOS system tray  -  `NSStatusItem` in the menu bar.
//!
//! # The lifetime rule this whole file is built around
//!
//! Apple, on `statusItemWithLength:`:
//!
//! > "Because the system status bar is shared by all applications, it cannot
//! > retain references to each application's status item objects. Instead, each
//! > application is responsible for retaining its own status items... When
//! > deallocated, the status item removes itself from the status bar."
//!
//! So the `Retained<NSStatusItem>` held here IS what keeps the icon on screen,
//! and dropping it is the removal API. There is no "hide" call to forget.
//!
//! Everything here is main-thread only.

use std::collections::HashMap;

use objc2::{rc::Retained, AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSBitmapFormat, NSBitmapImageRep, NSCellImagePosition, NSImage, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength,
};
use objc2_foundation::{NSPoint, NSSize, NSString};

use azul_core::tray::{TrayEvent, TrayEventType, TrayIconData, TrayIconSource};

use super::{queue_tray_event, TrayError};
use crate::desktop::shell2::macos::menu::{take_pending_menu_actions_matching, MenuState};

/// The menu bar is ~22pt tall; 18pt is the conventional artwork box.
const ICON_POINTS: u32 = 18;
/// Render at 2x so the item is sharp on Retina. AppKit downscales for 1x
/// displays, which is always better than upscaling an 18px original.
const ICON_PIXELS: u32 = ICON_POINTS * 2;

pub(super) struct PlatformTray {
    /// Dropping this removes the item from the menu bar. See the module docs.
    item: Retained<NSStatusItem>,
    /// Owns the NSMenu and the tag -> callback map.
    menu: MenuState,
    /// Tags this tray owns, so the shared menu-action queue can be drained
    /// without stealing another window's actions.
    owned_tags: HashMap<isize, ()>,
}

impl core::fmt::Debug for PlatformTray {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PlatformTray")
            .field("owned_tags", &self.owned_tags.len())
            .finish_non_exhaustive()
    }
}

/// macOS always has a menu bar. Unlike Linux there is no "is a tray running"
/// question to answer  -  the only failure mode is being off the main thread.
pub(super) fn is_available() -> bool {
    MainThreadMarker::new().is_some()
}

impl PlatformTray {
    pub(super) fn new(
        data: &TrayIconData,
        provider: &azul_core::icon::SharedIconProvider,
        font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
    ) -> Result<Self, TrayError> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Err(TrayError::Platform(
                "NSStatusItem must be created on the main thread".into(),
            ));
        };

        let bar = unsafe { NSStatusBar::systemStatusBar() };
        // Variable rather than square: a square item is exactly `bar.thickness`
        // wide, which clips a wider-than-tall icon. Variable sizes to content
        // and still collapses to roughly square for a square icon.
        let item = unsafe { bar.statusItemWithLength(NSVariableStatusItemLength) };

        let mut this = Self {
            item,
            menu: MenuState::new(),
            owned_tags: HashMap::new(),
        };

        // `autosaveName` persists the item's POSITION in the menu bar across
        // launches. Without one the system invents a name, which is why
        // third-party items lost their order on Big Sur (FB8732253). Keyed on
        // the caller's stable id, never on a path or pid.
        unsafe {
            this.item
                .setAutosaveName(Some(&NSString::from_str(data.id.as_str())));
        }

        this.apply(data, mtm, provider, font_manager)?;
        Ok(this)
    }

    pub(super) fn update(
        &mut self,
        _old: &TrayIconData,
        new: &TrayIconData,
        provider: &azul_core::icon::SharedIconProvider,
        font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
    ) -> Result<(), TrayError> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Err(TrayError::Platform(
                "NSStatusItem must be updated on the main thread".into(),
            ));
        };
        self.apply(new, mtm, provider, font_manager)
    }

    fn apply(
        &mut self,
        data: &TrayIconData,
        mtm: MainThreadMarker,
        provider: &azul_core::icon::SharedIconProvider,
        font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
    ) -> Result<(), TrayError> {
        use azul_core::tray::TrayStatus;

        // `Passive` means "the host may hide this". macOS has no such state, so
        // the honest mapping is to actually remove the item — which, per the
        // module docs, means clearing `isVisible` rather than dropping `item`
        // (we still need the object to bring it back).
        unsafe {
            self.item
                .setVisible(!matches!(data.status, TrayStatus::Passive));
        }

        let Some(button) = (unsafe { self.item.button(mtm) }) else {
            // Documented as optional; in practice always present since 10.10.
            return Err(TrayError::Platform("NSStatusItem has no button".into()));
        };

        // ---- icon ----
        match rgba_for(data, mtm, provider, font_manager) {
            Some(image) => unsafe {
                button.setImage(Some(&image));
                button.setImagePosition(NSCellImagePosition::ImageOnly);
            },
            None => unsafe { button.setImage(None) },
        }

        // ---- tooltip ----
        unsafe {
            match data.tooltip.as_ref() {
                Some(t) => button.setToolTip(Some(&NSString::from_str(t.as_str()))),
                None => button.setToolTip(None),
            }
        }

        // ---- menu ----
        //
        // Assigning a menu means AppKit opens it on mouse-DOWN for BOTH buttons
        // and the button's action never fires — so `Activate` is not
        // observable while a menu is attached. That is AppKit's behaviour, not
        // a limitation we can code around (Qt documents the same consequence),
        // and it is why `TrayEventType::ContextMenu` is documented as a
        // request rather than a command.
        match data.menu.as_ref() {
            Some(menu) => {
                if self.menu.update_if_changed(menu, mtm) {
                    self.owned_tags.clear();
                    // Record which tags are ours BEFORE any click can arrive,
                    // so the filtered drain never misses one.
                    for tag in self.menu.known_tags() {
                        self.owned_tags.insert(tag, ());
                    }
                }
                unsafe { self.item.setMenu(self.menu.get_nsmenu().map(|m| &**m)) };
            }
            None => {
                unsafe { self.item.setMenu(None) };
                self.owned_tags.clear();
            }
        }

        Ok(())
    }

    /// Drain menu clicks that belong to THIS tray and turn them into events.
    ///
    /// Menu items post their tag to a process-wide queue shared with every
    /// window's menu bar, so this takes only the tags it owns and leaves the
    /// rest for whoever does.
    pub(super) fn pump(&mut self) {
        if self.owned_tags.is_empty() {
            return;
        }
        for tag in take_pending_menu_actions_matching(|t| self.owned_tags.contains_key(&t)) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            queue_tray_event(TrayEvent::menu_item(tag as u32));
        }
    }

    /// The callback registered for a menu command, so the integration layer can
    /// invoke it against a window (a tray click has no window of its own, and a
    /// `CallbackInfo` needs one).
    pub(super) fn menu_callback(
        &self,
        command: u32,
    ) -> Option<&azul_core::menu::CoreMenuCallback> {
        self.menu.get_callback_for_tag(command as isize)
    }
}

/// Build an `NSImage` for the tray, or `None` if the data carries no icon.
fn rgba_for(
    data: &TrayIconData,
    mtm: MainThreadMarker,
    provider: &azul_core::icon::SharedIconProvider,
    font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
) -> Option<Retained<NSImage>> {
    let (w, h, rgba) = match data.icon {
        TrayIconSource::None => return None,
        TrayIconSource::Rgba(ref v) => {
            let img = data.best_icon(ICON_PIXELS).into_option()?;
            let _ = v;
            (img.width, img.height, img.rgba.as_ref().to_vec())
        }
        TrayIconSource::Named(ref spec) => {
            let rendered = crate::desktop::tray::render_named_icon(spec.as_str(), ICON_PIXELS, provider, font_manager)?;
            (rendered.width, rendered.height, rendered.rgba)
        }
    };

    let image = nsimage_from_rgba(&rgba, w, h, ICON_POINTS, mtm)?;

    // A TEMPLATE image is drawn as an alpha mask and tinted by AppKit per
    // appearance (light/dark menu bar), per highlight state, and per accent
    // colour. Without this the icon keeps its own colour and is invisible on
    // one of the two menu-bar appearances. The consequence for the caller is
    // that colour is DISCARDED — which is correct for a status item, and is
    // why the renderer is asked for an opaque-black glyph.
    unsafe { image.setTemplate(true) };
    Some(image)
}

/// Wrap straight (non-premultiplied) RGBA8 in an `NSImage` of `points` square.
///
/// `NSImage.size` is in POINTS and `pixelsWide` in PIXELS; the ratio is the
/// scale factor, so a 36px buffer at 18pt is a 2x image and AppKit picks the
/// right one per display.
pub(crate) fn nsimage_from_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
    points: u32,
    _mtm: MainThreadMarker,
) -> Option<Retained<NSImage>> {
    let expected = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
    if rgba.len() != expected {
        return None;
    }

    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            // NULL, so the rep allocates and OWNS its buffer.
            //
            // This is load-bearing: NSBitmapImageRep does NOT copy a `planes`
            // buffer you hand it, it aliases the pointer. Passing a Rust
            // slice's pointer here is a use-after-free the moment the owner
            // drops. We memcpy into `bitmapData()` below instead.
            core::ptr::null_mut(),
            width as isize,
            height as isize,
            8,  // bitsPerSample
            4,  // samplesPerPixel: R,G,B,A
            true,  // hasAlpha
            false, // isPlanar (interleaved)
            // NSDeviceRGB, not NSCalibratedRGB — the latter shifts colours on
            // modern displays.
            objc2_app_kit::NSDeviceRGBColorSpace,
            // Straight alpha, R first in memory. Setting neither AlphaFirst nor
            // an endian flag means byte-order RGBA, which is what our renderer
            // produces.
            NSBitmapFormat::AlphaNonpremultiplied,
            (width as isize) * 4, // bytesPerRow
            32,                   // bitsPerPixel
        )
    }?;

    unsafe {
        let dst = rep.bitmapData();
        if dst.is_null() {
            return None;
        }
        core::ptr::copy_nonoverlapping(rgba.as_ptr(), dst, rgba.len());
    }

    #[allow(clippy::cast_precision_loss)] // icon sizes are small
    let pts = points as f64;
    let image = unsafe { NSImage::initWithSize(NSImage::alloc(), NSSize::new(pts, pts)) };
    unsafe { image.addRepresentation(&rep) };
    Some(image)
}
