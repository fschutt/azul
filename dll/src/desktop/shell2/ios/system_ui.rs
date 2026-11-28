//! The system's chrome around the render view, and what the engine is told about the display.
//!
//! * `AzulViewController`: the window's root view controller, the one place UIKit asks about the status bar
//!   (`prefersStatusBarHidden`, `preferredStatusBarStyle`), the home indicator (`prefersHomeIndicatorAutoHidden`) and
//!   the edges whose system gesture needs a second swipe (`preferredScreenEdgesDeferringSystemGestures`). The answers
//!   are instance variables the window sets from `WindowFlags::system_bars` ([`apply`]); the numbers and what they
//!   come to are `ios_display::system_ui_of` (host-tested). A plain `UIViewController` answered "not hidden, default
//!   style, no deferral" to all four for ever: an app could neither hide the status bar nor take the top edge.
//! * [`display_features`]: the cutout, the corners, the bars and the fingerprint sensor in the window's coordinates, as
//!   the engine's `DisplayFeatures` (the shapes the safe-area insets are made of), from the host-tested table of
//!   `ios_display` and what UIKit does measure here (the interface orientation, the status bar's frame and whether it
//!   is hidden).
//!
//! Nothing here uses private API: the cutout's shape and the corner radius have no public number, so they come from the
//! table and the cutout is reported `approximate`.

use core::ptr;

use azul_core::display_features::{DisplayFeatures, SystemBarIcons, SystemBarMode, SystemBarsOptions};
use objc::{
    class, msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};

use super::{declare_class, CGRect};
use crate::desktop::shell2::common::{
    ios_display::{ios_display, system_ui_of, turn_of, IosSystemUi},
    ios_display_features::features_of,
};

const IVAR_STATUS_HIDDEN: &str = "azStatusBarHidden";
const IVAR_STATUS_STYLE: &str = "azStatusBarStyle";
const IVAR_HOME_HIDDEN: &str = "azHomeIndicatorHidden";
const IVAR_DEFERRED_EDGES: &str = "azDeferredEdges";

extern "C" fn prefers_status_bar_hidden(this: &Object, _cmd: Sel) -> bool {
    // SAFETY: the instance variable is declared by `declare_view_controller`.
    unsafe { *this.get_ivar::<u8>(IVAR_STATUS_HIDDEN) != 0 }
}

extern "C" fn preferred_status_bar_style(this: &Object, _cmd: Sel) -> i64 {
    // SAFETY: as above.
    unsafe { *this.get_ivar::<i64>(IVAR_STATUS_STYLE) }
}

extern "C" fn prefers_home_indicator_auto_hidden(this: &Object, _cmd: Sel) -> bool {
    // SAFETY: as above.
    unsafe { *this.get_ivar::<u8>(IVAR_HOME_HIDDEN) != 0 }
}

extern "C" fn preferred_screen_edges_deferring_system_gestures(this: &Object, _cmd: Sel) -> u64 {
    // SAFETY: as above.
    unsafe { *this.get_ivar::<u64>(IVAR_DEFERRED_EDGES) }
}

/// Declares `AzulViewController` (a subclass of `UIViewController`); `None` when the runtime refuses every name, and
/// the window then keeps a plain view controller (no options for the bars, nothing else lost).
pub(super) fn declare_view_controller() -> Option<&'static Class> {
    let mut decl = declare_class("AzulViewController", class!(UIViewController))?;
    decl.add_ivar::<u8>(IVAR_STATUS_HIDDEN);
    decl.add_ivar::<i64>(IVAR_STATUS_STYLE);
    decl.add_ivar::<u8>(IVAR_HOME_HIDDEN);
    decl.add_ivar::<u64>(IVAR_DEFERRED_EDGES);
    // SAFETY: the signatures are those of the UIViewController methods they override.
    unsafe {
        decl.add_method(
            sel!(prefersStatusBarHidden),
            prefers_status_bar_hidden as extern "C" fn(&Object, Sel) -> bool,
        );
        decl.add_method(
            sel!(preferredStatusBarStyle),
            preferred_status_bar_style as extern "C" fn(&Object, Sel) -> i64,
        );
        decl.add_method(
            sel!(prefersHomeIndicatorAutoHidden),
            prefers_home_indicator_auto_hidden as extern "C" fn(&Object, Sel) -> bool,
        );
        decl.add_method(
            sel!(preferredScreenEdgesDeferringSystemGestures),
            preferred_screen_edges_deferring_system_gestures as extern "C" fn(&Object, Sel) -> u64,
        );
    }
    Some(decl.register())
}

/// What the window's `system_bars` options come to for UIKit (the table is `ios_display::system_ui_of`; `HiddenSticky`
/// is `HiddenTransient` on iOS, which has no bar that stays once a swipe showed it).
#[must_use]
pub(super) fn ui_of_options(options: &SystemBarsOptions) -> IosSystemUi {
    let icons = match options.icons {
        SystemBarIcons::Auto => 0,
        SystemBarIcons::Light => 1,
        SystemBarIcons::Dark => 2,
    };
    system_ui_of(
        !matches!(options.status, SystemBarMode::Visible),
        !matches!(options.navigation, SystemBarMode::Visible),
        icons,
    )
}

/// Whether `object` answers `selector` (an API newer than the oldest iOS the shell runs on is asked about first: an
/// unrecognised selector is an exception, and an exception through Rust frames is an abort).
unsafe fn responds(object: *mut Object, selector: Sel) -> bool {
    if object.is_null() {
        return false;
    }
    msg_send![object, respondsToSelector: selector]
}

/// Hand the view controller what the system's chrome is to look like, and have UIKit ask again.
///
/// # Safety
/// `view_controller` is null or a live `AzulViewController` (the shell's own, on the main thread).
pub(super) unsafe fn apply(view_controller: *mut Object, ui: IosSystemUi) {
    if view_controller.is_null() {
        return;
    }
    // Only an `AzulViewController` has the instance variables; a plain one (the declaration failed) keeps its answers.
    if (*view_controller).class().instance_variable(IVAR_STATUS_HIDDEN).is_none() {
        return;
    }
    let vc = &mut *view_controller;
    vc.set_ivar::<u8>(IVAR_STATUS_HIDDEN, u8::from(ui.status_bar_hidden));
    vc.set_ivar::<i64>(IVAR_STATUS_STYLE, ui.status_bar_style);
    vc.set_ivar::<u8>(IVAR_HOME_HIDDEN, u8::from(ui.home_indicator_hidden));
    vc.set_ivar::<u64>(IVAR_DEFERRED_EDGES, ui.deferred_edges);
    let _: () = msg_send![view_controller, setNeedsStatusBarAppearanceUpdate];
    if responds(view_controller, sel!(setNeedsUpdateOfHomeIndicatorAutoHidden)) {
        let _: () = msg_send![view_controller, setNeedsUpdateOfHomeIndicatorAutoHidden];
    }
    if responds(view_controller, sel!(setNeedsUpdateOfScreenEdgesDeferringSystemGestures)) {
        let _: () = msg_send![view_controller, setNeedsUpdateOfScreenEdgesDeferringSystemGestures];
    }
}

/// The window scene of the view (iOS 13 and later), or null.
unsafe fn scene_of(view: *mut Object) -> *mut Object {
    if view.is_null() {
        return ptr::null_mut();
    }
    let window: *mut Object = msg_send![view, window];
    if !responds(window, sel!(windowScene)) {
        return ptr::null_mut();
    }
    msg_send![window, windowScene]
}

/// What UIKit measures of the status bar: whether it is hidden and the height of its frame (0 while hidden or when
/// UIKit does not say), from the scene's `UIStatusBarManager`.
unsafe fn status_bar_of(view: *mut Object) -> Option<(bool, f32)> {
    let scene = scene_of(view);
    if !responds(scene, sel!(statusBarManager)) {
        return None;
    }
    let manager: *mut Object = msg_send![scene, statusBarManager];
    if manager.is_null() {
        return None;
    }
    let hidden: bool = msg_send![manager, isStatusBarHidden];
    let frame: CGRect = msg_send![manager, statusBarFrame];
    Some((hidden, frame.size.height as f32))
}

/// `UIInterfaceOrientation` of the scene the view is in: 1 upright, 2 upside down, 3 landscape right, 4 landscape left;
/// 0 when UIKit does not say (no window yet, before iOS 13).
unsafe fn interface_orientation_of(view: *mut Object) -> i64 {
    let scene = scene_of(view);
    if !responds(scene, sel!(interfaceOrientation)) {
        return 0;
    }
    msg_send![scene, interfaceOrientation]
}

/// Whether the device is an iPad (`userInterfaceIdiom == UIUserInterfaceIdiomPad`, 1).
unsafe fn is_pad() -> bool {
    let device: *mut Object = msg_send![class!(UIDevice), currentDevice];
    if device.is_null() {
        return false;
    }
    let idiom: i64 = msg_send![device, userInterfaceIdiom];
    idiom == 1
}

/// The display features of the window whose render view is `view`, `width` x `height` points with `insets` (top,
/// right, bottom, left) as `layoutSubviews` read them. `options` are the app's `system_bars`; `touch_id_hardware` is
/// asked once at window creation.
///
/// # Safety
/// `view` is null or the shell's own render view, on the main thread.
pub(super) unsafe fn display_features(
    view: *mut Object,
    width: f32,
    height: f32,
    insets: (f32, f32, f32, f32),
    touch_id_hardware: bool,
    options: &SystemBarsOptions,
) -> DisplayFeatures {
    let turn = turn_of(width, height, interface_orientation_of(view));
    let mut display = ios_display(width, height, insets, is_pad(), touch_id_hardware, turn);
    let measured = status_bar_of(view);
    // The status bar's own frame, where UIKit gives one, is what the bar measures (the table of the class is for a bar
    // that is hidden at the moment); the engine gets the measured height while the bar is shown.
    if let (Some(bar), Some((false, measured_height))) = (display.status_bar.as_mut(), measured) {
        if measured_height > 0.0 {
            bar.height = measured_height;
        }
    }
    let system_hides = matches!(measured, Some((true, _)));
    let status_visible = !system_hides && !options.status_hidden();
    let navigation_visible = matches!(options.navigation, SystemBarMode::Visible);
    features_of(&display, status_visible, navigation_visible)
}
