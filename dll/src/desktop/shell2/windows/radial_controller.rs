//! Surface Dial via `RadialController` (WinRT).
//!
//! # Why this is the interesting dial backend
//!
//! It is the ONLY one that reports a real ANGLE and the only one that can ever
//! fill `DialState::contact_position`.
//!
//! The Wayland tablet-pad dial reports rotation but no position; the Android
//! Wear crown reports detents and no angle at all (9c-i-a records that there is
//! no honest conversion). `RadialController` gives
//! `RotationDeltaInDegrees` - an actual angular delta - and, when the Dial is
//! placed ON a Surface Studio's display, a screen contact point so an app can
//! draw a radial menu around it.
//!
//! # `CreateForWindow` is what makes it reachable
//!
//! `RadialController::CreateForCurrentView()` is the UWP entry point and is
//! useless to a Win32 app - there is no CoreWindow. The desktop route is the
//! `IRadialControllerInterop` activation factory's `CreateForWindow(hwnd)`,
//! which is why this needs `Win32_UI_Input_Radial` on top of `UI_Input`.
//!
//! # Failure is ordinary
//!
//! No Dial paired, a Windows build without the interface, or a machine that
//! never had the feature all land in the same place: `None`, and no dial
//! events. Nothing else in the shell changes.

use azul_core::geom::{LogicalPosition, OptionLogicalPosition};
use windows::{
    core::Interface,
    Foundation::TypedEventHandler,
    Win32::UI::Input::Radial::IRadialControllerInterop,
    UI::Input::{
        RadialController, RadialControllerButtonClickedEventArgs,
        RadialControllerRotationChangedEventArgs,
    },
};

use crate::desktop::shell2::common::debug_server::LogCategory;
use crate::log_debug;

/// Keeps the controller alive for the window's lifetime.
///
/// Dropping it unregisters the app from the Dial's menu, so it is held rather
/// than created per event.
pub struct RadialControllerOwner {
    _controller: RadialController,
}

/// Push one dial update into the engine, resolving the window from its HWND
/// the way `dnd.rs` and `direct_manipulation.rs` do - the event handler
/// outlives any borrow the shell could hand it.
fn emit_dial(hwnd: isize, delta_rad: f32, pressed: bool, contact: OptionLogicalPosition) {
    let hwnd_t = hwnd as super::dlopen::HWND;
    let Some(window_ptr) = super::registry::get_window(hwnd_t) else {
        return;
    };
    // SAFETY: the registry holds the live `Box::into_raw` pointer, which is
    // how every other callback in this shell reaches its window.
    let window: &mut super::Win32Window = unsafe { &mut *window_ptr };

    if let Some(ref mut lw) = window.common.layout_window {
        lw.gesture_drag_manager.update_dial_state(
            azul_layout::managers::gesture::DialState {
                // Windows exposes no per-dial id; a machine pairs one Dial.
                device_id: 0,
                delta_rad,
                // The Dial HAS physical detents, but the API reports a
                // continuous angle and never says a detent was crossed - so
                // claiming a count would be inventing one. The mirror of the
                // Android backend, which reports detents and no angle.
                detent_count: 0.0,
                pressed,
                contact_position: contact,
            },
        );
    }
    let result = super::PlatformWindow::process_window_events(window, 0);
    window.route_main_window_result(hwnd_t, result);
}

impl RadialControllerOwner {
    /// Create a controller for `hwnd` and subscribe to rotation and clicks.
    ///
    /// `None` where there is no Dial support at all, which is the common case
    /// and not an error.
    pub fn new(hwnd: isize) -> Option<Self> {
        let hwnd_t = windows::Win32::Foundation::HWND(hwnd as *mut core::ffi::c_void);

        // The activation factory, cast to the INTEROP interface. This is the
        // step that turns a UWP-only API into a desktop-reachable one.
        let interop: IRadialControllerInterop =
            windows::core::factory::<RadialController, IRadialControllerInterop>().ok()?;
        let controller: RadialController =
            unsafe { interop.CreateForWindow(hwnd_t) }.ok()?;

        // Rotation. `RotationDeltaInDegrees` is a real angular delta, so this
        // is the one backend where `delta_rad` is honest rather than zero.
        let rotation_handler = TypedEventHandler::<
            RadialController,
            RadialControllerRotationChangedEventArgs,
        >::new(move |_sender, args| {
            if let Some(args) = args.as_ref() {
                let degrees = args.RotationDeltaInDegrees().unwrap_or(0.0);
                // A contact point only exists when the Dial is ON the screen -
                // a Surface Studio. Off-screen use has none, which is why the
                // field is an Option rather than a zero.
                let contact = args
                    .Contact()
                    .ok()
                    .and_then(|c| c.Position().ok())
                    .map_or(OptionLogicalPosition::None, |p| {
                        OptionLogicalPosition::Some(LogicalPosition::new(p.X, p.Y))
                    });
                emit_dial(hwnd, (degrees as f32).to_radians(), false, contact);
            }
            Ok(())
        });
        controller.RotationChanged(&rotation_handler).ok()?;

        // The click. `update_dial_state` arms `DialClick` on the EDGE, so
        // reporting pressed=true here is enough - it does not have to be
        // released, and there is no release event to hook anyway.
        let click_handler = TypedEventHandler::<
            RadialController,
            RadialControllerButtonClickedEventArgs,
        >::new(move |_sender, args| {
            if let Some(args) = args.as_ref() {
                let contact = args
                    .Contact()
                    .ok()
                    .and_then(|c| c.Position().ok())
                    .map_or(OptionLogicalPosition::None, |p| {
                        OptionLogicalPosition::Some(LogicalPosition::new(p.X, p.Y))
                    });
                emit_dial(hwnd, 0.0, true, contact);
            }
            Ok(())
        });
        controller.ButtonClicked(&click_handler).ok()?;

        log_debug!(LogCategory::Input, "[Win32] RadialController created");
        Some(Self {
            _controller: controller,
        })
    }
}
