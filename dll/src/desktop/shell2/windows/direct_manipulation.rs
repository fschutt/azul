//! Precision-touchpad pinch and pan via DirectManipulation (7c-i-a).
//!
//! # Why this exists alongside the Ctrl+wheel synthesis
//!
//! 7c-i made pinch WORK on Windows laptops by synthesizing it from Ctrl+wheel,
//! which is how a precision touchpad reports zoom to an app that has not opted
//! into anything else. That is correct and it is what browsers do, but it is
//! quantised: every notch is a fixed 10% step, there is no pan, and a real
//! mouse with Ctrl held is indistinguishable from a touchpad.
//!
//! DirectManipulation is the API that gives the actual two-finger geometry: a
//! continuous scale factor and a translation, from the touchpad only. It is
//! also the only Win32 path that reports touchpad PAN as a gesture rather than
//! as wheel deltas.
//!
//! # The sequence
//!
//! Established from Microsoft's docs and Firefox's `DirectManipulationOwner`:
//!
//! ```text
//!   CoCreateInstance(DirectManipulationManager)   -> IDirectManipulationManager
//!   GetUpdateManager()                            -> IDirectManipulationUpdateManager
//!   CreateViewport(None, hwnd)                    -> IDirectManipulationViewport
//!   ActivateConfiguration(INTERACTION | SCALING | TRANSLATION_X | TRANSLATION_Y)
//!   SetViewportOptions(MANUALUPDATE)
//!   AddEventHandler(hwnd, handler)                -> cookie
//!   SetViewportRect(client rect)
//!   manager.Activate(hwnd)  +  viewport.Enable()
//!   ... WM_POINTERDOWN -> viewport.SetContact(pointerId)
//!   ... each frame     -> update_manager.Update()
//!   ... OnContentUpdated -> GetContentTransform(&mut [f32; 6])[0] is the scale
//! ```
//!
//! `MANUALUPDATE` is deliberate: without it DirectManipulation drives its own
//! clock and delivers content updates on a thread of its choosing, which is
//! wrong for an engine that already has a frame loop. With it, `Update()` is
//! pumped from the same place everything else is.
//!
//! # Failure is normal, not exceptional
//!
//! DirectManipulation is absent on Server SKUs without the desktop experience,
//! and `CoCreateInstance` fails on a machine with no touchpad stack. Every
//! entry point here returns quietly in that case and the Ctrl+wheel path
//! carries on working - which is why the two coexist rather than one replacing
//! the other.

use windows::{
    core::{implement, Interface, Ref, Result as WinResult},
    Win32::{
        Foundation::{HWND, RECT},
        Graphics::DirectManipulation::{
            DirectManipulationManager, IDirectManipulationContent, IDirectManipulationManager,
            IDirectManipulationUpdateManager, IDirectManipulationViewport,
            IDirectManipulationViewportEventHandler, IDirectManipulationViewportEventHandler_Impl,
            DIRECTMANIPULATION_CONFIGURATION_INTERACTION, DIRECTMANIPULATION_CONFIGURATION_SCALING,
            DIRECTMANIPULATION_CONFIGURATION_TRANSLATION_X,
            DIRECTMANIPULATION_CONFIGURATION_TRANSLATION_Y, DIRECTMANIPULATION_STATUS,
            DIRECTMANIPULATION_VIEWPORT_OPTIONS_MANUALUPDATE,
        },
        System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
    },
};

use crate::desktop::shell2::common::debug_server::LogCategory;
use crate::log_debug;

/// The last scale DirectManipulation reported, so `OnContentUpdated` can emit a
/// DELTA. The transform is absolute-since-gesture-start, but `DetectedPinch`
/// carries a per-event scale like every other backend, so the two have to be
/// differenced.
///
/// Process-global rather than per-window: DirectManipulation drives one gesture
/// at a time across the desktop, and the handler has no path back to its owner
/// beyond the HWND it was built with.
static LAST_SCALE: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(0x3f80_0000); // 1.0f32 bits

fn take_scale_delta(absolute: f32) -> f32 {
    let prev = f32::from_bits(LAST_SCALE.load(core::sync::atomic::Ordering::Relaxed));
    LAST_SCALE.store(absolute.to_bits(), core::sync::atomic::Ordering::Relaxed);
    if prev.abs() < f32::EPSILON {
        return 1.0;
    }
    absolute / prev
}

fn reset_scale() {
    LAST_SCALE.store(1.0f32.to_bits(), core::sync::atomic::Ordering::Relaxed);
}

/// The COM callback object. Holds only the HWND and resolves the owning window
/// lazily, exactly as `dnd.rs`'s `FileDropTarget` does - a COM object outlives
/// any borrow the shell could hand it.
#[implement(IDirectManipulationViewportEventHandler)]
struct DmEventHandler {
    hwnd: isize,
}

#[allow(non_snake_case)]
impl IDirectManipulationViewportEventHandler_Impl for DmEventHandler_Impl {
    fn OnViewportStatusChanged(
        &self,
        _viewport: Ref<IDirectManipulationViewport>,
        current: DIRECTMANIPULATION_STATUS,
        _previous: DIRECTMANIPULATION_STATUS,
    ) -> WinResult<()> {
        // Any transition out of RUNNING ends the gesture, so the next one
        // starts from 1.0 rather than inheriting the last scale.
        const RUNNING: i32 = 4;
        if current.0 != RUNNING {
            reset_scale();
        }
        Ok(())
    }

    fn OnViewportUpdated(&self, _viewport: Ref<IDirectManipulationViewport>) -> WinResult<()> {
        Ok(())
    }

    fn OnContentUpdated(
        &self,
        _viewport: Ref<IDirectManipulationViewport>,
        content: Ref<IDirectManipulationContent>,
    ) -> WinResult<()> {
        let Some(content) = content.as_ref() else {
            return Ok(());
        };
        // A 3x2 affine: [scale_x, shear_y, shear_x, scale_y, dx, dy].
        let mut transform = [0.0f32; 6];
        unsafe { content.GetContentTransform(&mut transform)? };
        let absolute = transform[0];
        if absolute <= 0.0 {
            return Ok(());
        }
        let delta = take_scale_delta(absolute);
        // Sub-per-mille changes are DirectManipulation settling, not the user
        // pinching; forwarding them would emit a pinch per frame while a
        // finger rests on the pad.
        if (delta - 1.0).abs() < 0.001 {
            return Ok(());
        }
        emit_pinch(self.hwnd, delta);
        Ok(())
    }
}

/// Everything one window needs to keep DirectManipulation alive.
///
/// Dropping it releases the COM objects, which is what detaches the viewport -
/// there is no explicit teardown call to forget.
pub struct DirectManipulationOwner {
    manager: IDirectManipulationManager,
    update_manager: IDirectManipulationUpdateManager,
    viewport: IDirectManipulationViewport,
    hwnd: HWND,
    cookie: u32,
}

impl DirectManipulationOwner {
    /// Build and enable a viewport for `hwnd`, or `None` where
    /// DirectManipulation is unavailable.
    ///
    /// `None` is a normal outcome, not an error: the API is absent on Server
    /// SKUs without the desktop experience, and `CoCreateInstance` fails on a
    /// machine with no touchpad stack. The Ctrl+wheel path keeps working.
    pub fn new(hwnd: isize, width: i32, height: i32) -> Option<Self> {
        let hwnd_t = HWND(hwnd as *mut core::ffi::c_void);
        unsafe {
            let manager: IDirectManipulationManager =
                CoCreateInstance(&DirectManipulationManager, None, CLSCTX_INPROC_SERVER).ok()?;
            let update_manager: IDirectManipulationUpdateManager =
                manager.GetUpdateManager().ok()?;
            // `None` frame info: MANUALUPDATE means we drive the clock, so
            // there is no frame-info provider for DM to call back into.
            let viewport: IDirectManipulationViewport =
                manager.CreateViewport(None, hwnd_t).ok()?;

            let config = DIRECTMANIPULATION_CONFIGURATION_INTERACTION
                | DIRECTMANIPULATION_CONFIGURATION_SCALING
                | DIRECTMANIPULATION_CONFIGURATION_TRANSLATION_X
                | DIRECTMANIPULATION_CONFIGURATION_TRANSLATION_Y;
            viewport.ActivateConfiguration(config).ok()?;
            viewport
                .SetViewportOptions(DIRECTMANIPULATION_VIEWPORT_OPTIONS_MANUALUPDATE)
                .ok()?;

            let handler: IDirectManipulationViewportEventHandler =
                DmEventHandler { hwnd }.into();
            let cookie = viewport.AddEventHandler(Some(hwnd_t), &handler).ok()?;

            let rect = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            viewport.SetViewportRect(&rect).ok()?;
            manager.Activate(hwnd_t).ok()?;
            viewport.Enable().ok()?;
            reset_scale();

            log_debug!(
                LogCategory::Input,
                "[Win32] DirectManipulation viewport enabled ({}x{})",
                width,
                height
            );
            Some(Self {
                manager,
                update_manager,
                viewport,
                hwnd: hwnd_t,
                cookie,
            })
        }
    }

    /// Hand a pointer to DirectManipulation, from `WM_POINTERDOWN`.
    ///
    /// This is what makes the gesture BEGIN: without a contact the viewport
    /// stays idle no matter how many fingers are on the pad.
    pub fn set_contact(&self, pointer_id: u32) {
        unsafe {
            let _ = self.viewport.SetContact(pointer_id);
        }
    }

    /// Pump the state machine. MANUALUPDATE means nothing moves without this.
    pub fn update(&self) {
        unsafe {
            // `None`: MANUALUPDATE means we own the clock, so there is no
            // frame-info provider for DM to consult.
            let _ = self.update_manager.Update(None);
        }
    }

    /// Follow a resize, or the viewport keeps hit-testing the old client area.
    pub fn resize(&self, width: i32, height: i32) {
        let rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        unsafe {
            let _ = self.viewport.SetViewportRect(&rect);
        }
    }
}

impl Drop for DirectManipulationOwner {
    fn drop(&mut self) {
        unsafe {
            let _ = self.viewport.RemoveEventHandler(self.cookie);
            let _ = self.viewport.Disable();
            let _ = self.viewport.Abandon();
            let _ = self.manager.Deactivate(self.hwnd);
        }
    }
}

/// Push one pinch into the engine, resolving the window from the HWND the same
/// way `dnd.rs` does - a COM object outlives any borrow the shell could hand it,
/// so the registry lookup happens per callback rather than being captured.
fn emit_pinch(hwnd: isize, scale: f32) {
    use azul_layout::managers::gesture::{DetectedPinch, NativeGestureEvent};

    let hwnd_t = hwnd as super::dlopen::HWND;
    let Some(window_ptr) = super::registry::get_window(hwnd_t) else {
        return;
    };
    // SAFETY: the registry holds the live `Box::into_raw` pointer for the
    // window, which is how every other callback in this shell reaches it.
    let window: &mut super::Win32Window = unsafe { &mut *window_ptr };

    // The gesture centre: DirectManipulation reports the transform, not the
    // contact point, so the current cursor position is the honest centre -
    // it tracks the pad and is where the user is looking.
    // `get_position()` is an Option - the cursor can be outside the window, and
    // a touchpad gesture does not require it to be inside. The window origin is
    // the honest fallback rather than a fabricated centre.
    let center = window
        .common
        .current_window_state()
        .mouse_state
        .cursor_position
        .get_position()
        .unwrap_or(azul_core::geom::LogicalPosition::zero());

    const PINCH_NOMINAL_DISTANCE: f32 = 100.0;
    if let Some(ref mut lw) = window.common.layout_window {
        lw.gesture_drag_manager
            .inject_native_gesture(NativeGestureEvent::Pinch(DetectedPinch {
                scale,
                center,
                initial_distance: PINCH_NOMINAL_DISTANCE,
                current_distance: PINCH_NOMINAL_DISTANCE * scale,
                duration_ms: 0,
            }));
    }
    let result = super::PlatformWindow::process_window_events(window, 0);
    window.route_main_window_result(hwnd_t, result);
}
