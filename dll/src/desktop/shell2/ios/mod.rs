//! iOS backend.
//!
//! Structurally mirrors `shell2/android/mod.rs`: an `IOSWindow` carries the
//! cross-platform [`CommonWindowState`] + a [`CpuBackend`], plus the native
//! `UIWindow` / `UIViewController` / `UIView` handles. The render path is
//! identical in spirit to Android — CPU rendering to an `AzulPixmap`, blitted
//! to the layer via `CGImage` + `CALayer.contents` (Sprint C wires the blit;
//! this module currently lands the type surface + entry point so the iOS
//! target compiles end-to-end).
//!
//! No iOS SDK is required to *type-check* this file — every UIKit/Foundation
//! symbol is referenced through the `objc` crate's `class!` / `msg_send!` /
//! `sel!` macros (which compile-check against the `objc` crate, not the
//! UIKit SDK). The SDK is only needed at link time, which lives in
//! `dll/build.rs::configure_ios`.

use crate::impl_platform_window_getters;
use std::{
    cell::RefCell,
    ffi::c_void,
    ptr,
    sync::{Arc, Once},
};

use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Protocol, Sel};
use objc::{class, msg_send, sel, sel_impl, Encode, Encoding};
use objc_id::Id;

use azul_core::{
    callbacks::RelayoutReason,
    gl::OptionGlContextPtr,
    hit_test::DocumentId,
    icon::SharedIconProvider,
    refany::RefAny,
    resources::{AppConfig, IdNamespace, ImageCache, RendererResources},
    window::{IOSHandle, RawWindowHandle},
};
use azul_layout::{
    window::{LayoutWindow, ScrollbarDragState},
    window_state::{FullWindowState, WindowCreateOptions},
};
use rust_fontconfig::{registry::FcFontRegistry, FcFontCache};

use crate::desktop::shell2::common::{
    debug_server::LogCategory,
    event::{self, CommonWindowState, HitTestNode, PlatformWindow},
    WindowError,
};
use crate::desktop::shell2::headless::CpuBackend;

pub mod accessibility;

use crate::desktop::wr_translate2::{AsyncHitTester, WrRenderApi};
use crate::{log_debug, log_error, log_info};

// ─── Core Graphics geometry types (FFI-safe; `Encode` impls let them
//     traverse `msg_send!` without depending on `core_graphics_sys`) ────

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

// Objective-C type encodings for the Core Graphics geometry structs.
// `{CGPoint=dd}` etc. matches the encoding `[UIScreen mainScreen].bounds`
// returns, which `msg_send!` walks to lay out the call. objc 0.2's
// `Encode` trait uses `fn encode() -> Encoding`, not the `const ENCODING`
// surface from objc2.
unsafe impl Encode for CGPoint {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CGPoint=dd}") }
    }
}
unsafe impl Encode for CGSize {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CGSize=dd}") }
    }
}
unsafe impl Encode for CGRect {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CGRect={CGPoint=dd}{CGSize=dd}}") }
    }
}

// ─── FFI bindings ─────────────────────────────────────────────────────

#[link(name = "Foundation", kind = "framework")]
extern "C" {
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

#[link(name = "UIKit", kind = "framework")]
extern "C" {
    fn UIApplicationMain(
        argc: i32,
        argv: *mut *mut u8,
        principalClassName: *mut Object,
        delegateClassName: *mut Object,
    ) -> i32;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
    fn CGColorSpaceRelease(cs: *mut c_void);
    fn CGDataProviderCreateWithData(
        info: *mut c_void,
        data: *const u8,
        size: usize,
        release: Option<extern "C" fn(*mut c_void, *const u8, usize)>,
    ) -> *mut c_void;
    fn CGDataProviderRelease(p: *mut c_void);
    fn CGImageCreate(
        width: usize,
        height: usize,
        bits_per_component: usize,
        bits_per_pixel: usize,
        bytes_per_row: usize,
        space: *mut c_void,
        bitmap_info: u32,
        provider: *mut c_void,
        decode: *const f64,
        should_interpolate: bool,
        intent: u32,
    ) -> *mut c_void;
    fn CGImageRelease(img: *mut c_void);
}

const K_CG_IMAGE_ALPHA_PREMULTIPLIED_LAST: u32 = 1;
const K_CG_BITMAP_BYTE_ORDER_DEFAULT: u32 = 0;
const K_CG_RENDERING_INTENT_DEFAULT: u32 = 0;

/// `CGDataProviderReleaseDataCallback` for the per-frame pixel copy handed to
/// `CGDataProviderCreateWithData` in [`display_layer`].
///
/// Core Graphics does NOT copy the bytes: the provider references them for the
/// lifetime of every `CGImage` built on it, and the layer retains that image
/// until the NEXT contents assignment — QuartzCore reads the bytes
/// asynchronously at the Core Animation commit at the end of the run-loop
/// turn. So the buffer must be owned by the provider and freed here, when CG
/// says it is done, not when the creating stack frame unwinds.
#[cfg(feature = "cpurender")]
extern "C" fn release_frame_pixels(_info: *mut c_void, data: *const u8, size: usize) {
    // Reconstitute the Box<[u8]> leaked in display_layer and drop it.
    // SAFETY: `data`/`size` are exactly the pointer + length of the boxed
    // slice passed to CGDataProviderCreateWithData below; CG invokes this
    // callback exactly once, when the last retain on the provider is gone.
    unsafe {
        drop(Box::from_raw(core::ptr::slice_from_raw_parts_mut(
            data as *mut u8,
            size,
        )));
    }
}

// ─── Global window pointer ────────────────────────────────────────────
//
// `extern "C"` Objective-C callbacks are static functions, so they reach
// back into Rust state via this singleton. Set in
// `application:didFinishLaunchingWithOptions:`, cleared by
// `applicationWillTerminate:` (TODO once we wire lifecycle methods).

static mut AZUL_IOS_WINDOW: *mut IOSWindow = ptr::null_mut();

/// Borrow the singleton AndroidWindow-style. None until `did_finish_launching`.
#[inline]
unsafe fn azul_ios_window<'a>() -> Option<&'a mut IOSWindow> {
    AZUL_IOS_WINDOW.as_mut()
}

/// The `CADisplayLink` driving [`display_tick`]. Stored (retained) so the
/// lifecycle hooks can pause it: a display link is NOT auto-paused when the
/// app leaves the foreground — the process merely gets SUSPENDED a few
/// seconds after `applicationDidEnterBackground:`, and until then the tick
/// keeps rendering + committing frames nobody can see. Main-thread only,
/// like every UIKit object here.
static mut AZUL_IOS_DISPLAY_LINK: *mut Object = ptr::null_mut();

/// Pause / resume the render tick. No-op before `install_display_link`.
unsafe fn set_display_link_paused(paused: bool) {
    let link = AZUL_IOS_DISPLAY_LINK;
    if !link.is_null() {
        let _: () = msg_send![link, setPaused: paused];
    }
}

// ─── AzulView (UIView subclass) ───────────────────────────────────────

extern "C" fn display_layer(_this: &Object, _cmd: Sel, layer: *mut Object) {
    // Sprint C iOS blit. Mirrors the Android render_frame() path:
    // regenerate layout if needed -> read cpu_backend.last_frame -> wrap
    // the AzulPixmap bytes in a CGImage -> assign to `layer.contents`.
    //
    // This is the `displayLayer:` CALayerDelegate override, NOT `drawRect:`.
    // UIView is its layer's delegate; because AzulView implements
    // `displayLayer:`, Core Animation calls it instead of allocating a
    // CGContext backing store — which is the ONLY arrangement under which a
    // manual `layer.contents` assignment sticks. The previous implementation
    // assigned `contents` inside `drawRect:`, and the view machinery replaces
    // `contents` with its (empty, never-drawn-into) backing store as soon as
    // `drawRect:` returns — so not one blitted frame ever reached the screen.
    // `setNeedsDisplay` still schedules this exactly like it scheduled
    // `drawRect:` (mark layer -> CA display pass -> delegate displayLayer:).
    let window = match unsafe { azul_ios_window() } {
        Some(w) => w,
        None => return,
    };

    if window.common.regeneration_pending() {
        if let Err(e) = window.regenerate_layout() {
            log_error!(LogCategory::Layout, "[iOS] regenerate_layout: {}", e);
        }
    }

    #[cfg(feature = "cpurender")]
    {
        // #27 native backbuffer: iOS stays LEGACY by design. The present
        // hands QuartzCore a copy precisely because CA reads the provider
        // bytes at the ASYNC commit (see the use-after-free note below) —
        // rendering the next frame directly into a buffer the compositor may
        // still be reading is the same hazard with fewer copies. Going
        // native here would need a CA-release-fenced buffer pool (Wayland
        // slot model with CG data-provider release callbacks as the busy
        // flags); design when an iOS device run exists to verify it.
        let pixmap = match window.cpu_backend.last_frame.as_ref() {
            Some(p) => p,
            None => return,
        };
        let (pw, ph) = (pixmap.width() as usize, pixmap.height() as usize);
        if pw == 0 || ph == 0 {
            return;
        }
        // Hand the provider its OWN copy of the frame. The old code passed a
        // pointer into `cpu_backend.last_frame` with `release: None` ("pixmap
        // outlives this call") — but the CGImage outlives the call: the layer
        // retains it until the next contents assignment, and QuartzCore reads
        // the provider bytes at the async CA commit. The very next
        // `render_frame` may reallocate or drop that buffer first (it takes
        // and replaces `last_frame` every pass), which is a use-after-free at
        // commit time (ERROR_CGDataProvider_BufferIsNotReadable / torn
        // frames). The copy is freed in `release_frame_pixels` when CG drops
        // its last reference.
        let owned: Box<[u8]> = Box::from(pixmap.data());
        let len = owned.len();
        let data_ptr = Box::into_raw(owned) as *mut u8;
        unsafe {
            let cs = CGColorSpaceCreateDeviceRGB();
            let provider = CGDataProviderCreateWithData(
                core::ptr::null_mut(),
                data_ptr,
                len,
                Some(release_frame_pixels),
            );
            if provider.is_null() {
                // CG never took ownership — reclaim the copy ourselves.
                drop(Box::from_raw(core::ptr::slice_from_raw_parts_mut(
                    data_ptr, len,
                )));
                CGColorSpaceRelease(cs);
                return;
            }
            let image = CGImageCreate(
                pw,
                ph,
                8,
                32,
                pw * 4,
                cs,
                K_CG_IMAGE_ALPHA_PREMULTIPLIED_LAST | K_CG_BITMAP_BYTE_ORDER_DEFAULT,
                provider,
                core::ptr::null(),
                false,
                K_CG_RENDERING_INTENT_DEFAULT,
            );
            if !image.is_null() {
                let _: () = msg_send![layer, setContents: image];
                CGImageRelease(image);
            }
            CGDataProviderRelease(provider);
            CGColorSpaceRelease(cs);
        }
    }
    // `layer` is only touched on the cpurender path.
    #[cfg(not(feature = "cpurender"))]
    let _ = layer;
}

/// Shared body for the four UITouch responder selectors. `phase`
/// follows UIKit semantics:
///   0 = began    (left_down=true, update cursor)
///   1 = moved    (update cursor only)
///   2 = ended    (left_down=false)
///   3 = cancelled
fn handle_touch(this: &Object, touches: *mut Object, event: *mut Object, phase: u8) {
    use azul_core::events::ProcessEventResult;
    use azul_core::geom::LogicalPosition;
    use azul_core::window::{CursorPosition, TouchPoint, TouchPointVec};

    let window = match unsafe { azul_ios_window() } {
        Some(w) => w,
        None => return,
    };

    // Walk every UITouch in the NSSet. iOS sends multi-finger / Pencil
    // events bundled together; the previous implementation called
    // `[touches anyObject]` and dropped fingers 2+ — multi-touch widgets
    // (paint canvases, pinch-to-zoom, two-finger rotations) saw only the
    // first finger. While walking we also extract Apple Pencil state on
    // any touch whose `type == .pencil` (UITouchTypePencil = 2).
    struct PencilSample {
        position: LogicalPosition,
        pressure: f32,
        x_tilt_deg: f32,
        y_tilt_deg: f32,
        /// Apple Pencil Pro barrel roll (radians), from `UITouch.rollAngle`
        /// (iOS 17.5+). `0.0` when the OS / pencil doesn't report it.
        barrel_roll_rad: f32,
    }
    let mut points: Vec<TouchPoint> = Vec::new();
    // Coalesced = the samples between the last frame and this one; predicted =
    // where UIKit thinks the finger is going. Both are asked for per UITouch
    // and both need the UIEvent, which is why they are gathered alongside the
    // main walk rather than in a second pass.
    let mut coalesced: Vec<TouchPoint> = Vec::new();
    let mut predicted: Vec<TouchPoint> = Vec::new();
    let mut pos: Option<LogicalPosition> = None;
    let mut pencil: Option<PencilSample> = None;
    let this_ptr = this as *const Object as *mut Object;

    unsafe {
        // [touches allObjects] → NSArray; iterate by index. NSSet has no
        // stable ordering but the order is deterministic *within* a
        // single call, which is what the consumer expects.
        let arr: *mut Object = msg_send![touches, allObjects];
        if !arr.is_null() {
            let count: usize = msg_send![arr, count];
            points.reserve(count);
            for i in 0..count {
                let touch: *mut Object = msg_send![arr, objectAtIndex: i];
                if touch.is_null() {
                    continue;
                }

                let p: CGPoint = msg_send![touch, locationInView: this_ptr];
                let touch_pos = LogicalPosition::new(p.x as f32, p.y as f32);

                // Apple guarantees the UITouch pointer identity is stable
                // for the lifetime of a single touch sequence (began →
                // moved* → ended/cancelled). Cast to u64 for the
                // TouchPoint.id slot.
                let id_u64 = touch as usize as u64;

                // Pressure: `force` is 0 on devices without 3D Touch /
                // Pencil. `maximumPossibleForce` returns 0 on those
                // devices, so the divisor guard falls back to the
                // TouchPoint sentinel (0.5) — matches the existing
                // android/headless behaviour.
                let force: f64 = msg_send![touch, force];
                let max_force: f64 = msg_send![touch, maximumPossibleForce];
                let normalized = if max_force > 0.0 {
                    (force / max_force).clamp(0.0, 1.0) as f32
                } else {
                    0.5
                };

                points.push(TouchPoint {
                    id: id_u64,
                    position: touch_pos,
                    force: normalized,
                    // Contact geometry: 0.0 = not reported by this backend.
                    major: 0.0,
                    minor: 0.0,
                    orientation_rad: 0.0,
                    tool_type: azul_core::window::TouchToolType::Unknown,
                });

                if pos.is_none() {
                    pos = Some(touch_pos);
                }

                // Coalesced and predicted samples for THIS touch.
                //
                // Both are UIEvent methods rather than UITouch ones, and both
                // return nil when unavailable — an old iOS, a simulator, or a
                // touch that has not moved — so the null check is not
                // defensive padding.
                //
                // UIKit INCLUDES the current touch as the last coalesced
                // entry, so drawing the coalesced list and then the main point
                // repeats the newest sample; the last one is dropped here.
                if !event.is_null() {
                    let mut gather = |sel_result: *mut Object, out: &mut Vec<TouchPoint>,
                                      drop_last: bool| {
                        if sel_result.is_null() {
                            return;
                        }
                        let count: usize = msg_send![sel_result, count];
                        let keep = if drop_last { count.saturating_sub(1) } else { count };
                        for idx in 0..keep {
                            let t: *mut Object = msg_send![sel_result, objectAtIndex: idx];
                            if t.is_null() {
                                continue;
                            }
                            let p: CGPoint = msg_send![t, locationInView: this_ptr];
                            let f: f64 = msg_send![t, force];
                            let maxf: f64 = msg_send![t, maximumPossibleForce];
                            out.push(TouchPoint {
                                id: id_u64,
                                position: LogicalPosition::new(p.x as f32, p.y as f32),
                                force: if maxf > 0.0 { (f / maxf) as f32 } else { 0.5 },
                                major: 0.0,
                                minor: 0.0,
                                orientation_rad: 0.0,
                                tool_type: azul_core::window::TouchToolType::Unknown,
                            });
                        }
                    };
                    let c: *mut Object = msg_send![event, coalescedTouchesForTouch: touch];
                    gather(c, &mut coalesced, true);
                    let pr: *mut Object = msg_send![event, predictedTouchesForTouch: touch];
                    gather(pr, &mut predicted, false);
                }

                // Pencil sample — first stylus wins (Apple Pencil is
                // single-instance; you can't pair two of them).
                if pencil.is_none() {
                    let touch_type: i64 = msg_send![touch, type];
                    if touch_type == 2 {
                        let altitude: f64 = msg_send![touch, altitudeAngle];
                        let azimuth: f64 = msg_send![touch, azimuthAngleInView: this_ptr];
                        let tilt_rad = (core::f64::consts::FRAC_PI_2 - altitude) as f32;
                        let orientation = azimuth as f32;
                        let (sin_o, cos_o) = orientation.sin_cos();
                        let tan_tilt = tilt_rad.tan();
                        let x_tilt_deg = (sin_o * tan_tilt).atan().to_degrees();
                        let y_tilt_deg = (-cos_o * tan_tilt).atan().to_degrees();
                        // Apple Pencil Pro barrel roll (iOS 17.5+). Guard
                        // with respondsToSelector so older iOS doesn't hit
                        // an unrecognized-selector trap.
                        let barrel_roll_rad: f32 = {
                            let responds: bool =
                                msg_send![touch, respondsToSelector: sel!(rollAngle)];
                            if responds {
                                let r: f64 = msg_send![touch, rollAngle];
                                r as f32
                            } else {
                                0.0
                            }
                        };
                        pencil = Some(PencilSample {
                            position: touch_pos,
                            pressure: normalized,
                            x_tilt_deg,
                            y_tilt_deg,
                            barrel_roll_rad,
                        });
                    }
                }
            }
        }
    }

    // Snapshot previous state for the diff pipeline; mirrors Android.
    window.snapshot_window_state_baseline("ios.handle_touch");

    {
        // Refresh TouchState FIRST — the mouse-button emulation below needs
        // to know how many fingers REMAIN after this phase. On ended /
        // cancelled UIKit hands us only the touches that ended in this phase
        // — the rest are still active. Filter the existing list against the
        // IDs UIKit just reported as ended, rather than clobbering it.
        let remaining = {
            let ts = window.common.touch_state_mut();
            match phase {
                0 | 1 => {
                    // Began / moved → merge by ID. Replace existing entries
                    // with the new sample; append new IDs.
                    let mut existing: Vec<TouchPoint> =
                        ts.touch_points.clone().into_library_owned_vec();
                    for new_point in &points {
                        if let Some(slot) = existing.iter_mut().find(|p| p.id == new_point.id) {
                            *slot = *new_point;
                        } else {
                            existing.push(*new_point);
                        }
                    }
                    ts.touch_points = TouchPointVec::from_vec(existing);
                    // Coalesced and predicted samples are per-frame: they
                    // describe the motion INTO this frame and the guess out
                    // of it, so last frame's are meaningless now. Replaced
                    // wholesale rather than appended.
                    ts.coalesced_points = TouchPointVec::from_vec(coalesced.clone());
                    ts.predicted_points = TouchPointVec::from_vec(predicted.clone());
                }
                2 | 3 => {
                    // Ended / cancelled → drop the reported IDs.
                    let drop_ids: Vec<u64> = points.iter().map(|p| p.id).collect();
                    let mut existing: Vec<TouchPoint> =
                        ts.touch_points.clone().into_library_owned_vec();
                    existing.retain(|p| !drop_ids.contains(&p.id));
                    ts.touch_points = TouchPointVec::from_vec(existing);
                    // A finished touch has no future, so a prediction that
                    // outlived it would draw a stroke past where the user
                    // lifted.
                    ts.coalesced_points = TouchPointVec::from_vec(Vec::new());
                    ts.predicted_points = TouchPointVec::from_vec(Vec::new());
                }
                _ => {}
            }
            ts.num_touches = ts.touch_points.len();
            ts.num_touches
        };

        let ms = window.common.mouse_state_mut();
        if let Some(p) = pos {
            ms.cursor_position = CursorPosition::InWindow(p);
        }
        match phase {
            0 => ms.left_down = true,
            // Only the LAST finger lifting releases the emulated left
            // button. UIKit reports each ended finger in its own
            // touchesEnded:/touchesCancelled: set, so clearing left_down
            // unconditionally cut the remaining fingers' press short
            // halfway through every multi-finger interaction (Android's
            // PointerUp arm already gets this right by leaving left_down
            // alone while the primary is still down).
            2 | 3 if remaining == 0 => ms.left_down = false,
            _ => {}
        }
    }

    if let Some(p) = pos {
        window.update_hit_test_at(p);
    }
    let r = window.process_window_events(0);
    if !matches!(r, ProcessEventResult::DoNothing) {
        window
            .common
            .request_regeneration(RelayoutReason::RefreshDom);
    }
    if let Some(lw) = window.common.layout_window.as_mut() {
        lw.gesture_drag_manager.clear_native_gesture();

        // Pencil events route through the same gesture manager that pen
        // tablets do on desktop. Apple Pencil has no eraser tip and no
        // barrel button at the UITouch level (Pencil 2 squeeze fires
        // `UIPencilInteraction` instead, a P2.3 follow-up), so both flags
        // stay `false` here; pressure, tilt, and barrel roll (Pencil Pro,
        // iOS 17.5+) are populated.
        if let Some(sample) = pencil.as_ref() {
            let in_contact = matches!(phase, 0 | 1);
            if in_contact {
                lw.gesture_drag_manager.update_pen_state_full(
                    sample.position,
                    sample.pressure,
                    (sample.x_tilt_deg, sample.y_tilt_deg),
                    true,
                    false, // is_eraser
                    false, // barrel_button_pressed
                    0,     // device_id (Apple Pencil has no public ID at this layer)
                    0.0,   // tangential_pressure (not reported by UITouch)
                    sample.barrel_roll_rad,
                    0, // tool_id (not reported by UITouch)
                );
            } else {
                lw.gesture_drag_manager.clear_pen_state();
            }
            window
                .common
                .request_regeneration(RelayoutReason::RefreshDom);
        }
    }

    // Ask the view to redraw — displayLayer: will pick up the new layout.
    let view = this as *const Object as *mut Object;
    let _: () = unsafe { msg_send![view, setNeedsDisplay] };
}

extern "C" fn touches_began(this: &Object, _cmd: Sel, touches: *mut Object, event: *mut Object) {
    handle_touch(this, touches, event, 0);
}
extern "C" fn touches_moved(this: &Object, _cmd: Sel, touches: *mut Object, event: *mut Object) {
    handle_touch(this, touches, event, 1);
}
extern "C" fn touches_ended(this: &Object, _cmd: Sel, touches: *mut Object, event: *mut Object) {
    handle_touch(this, touches, event, 2);
}
extern "C" fn touches_cancelled(
    this: &Object,
    _cmd: Sel,
    touches: *mut Object,
    event: *mut Object,
) {
    handle_touch(this, touches, event, 3);
}

/// UIKit calls `layoutSubviews` whenever the view's bounds change —
/// orientation rotation, split-view resize on iPad, `safeAreaInsets`
/// shift, etc. We re-read `[this bounds]`, refresh
/// `current_window_state.size.dimensions`, and flag a relayout. The
/// CADisplayLink will pick up the pending regeneration on its next
/// tick and call `present()` → `displayLayer:` → `regenerate_layout`.
extern "C" fn layout_subviews(this: &Object, _cmd: Sel) {
    use objc::sel;
    // Call super so UIView's own layout (autoresizing masks, constraints)
    // still runs. `objc_msgSendSuper` is fiddly via the objc 0.2 macro,
    // so we rely on the fact that `super.layoutSubviews` for `UIView` is
    // a no-op once we own the geometry — which we do.
    let _: () = unsafe { msg_send![this as *const Object as *mut Object, setNeedsDisplay] };
    if let Some(window) = unsafe { azul_ios_window() } {
        let bounds: CGRect = unsafe { msg_send![this as *const Object as *mut Object, bounds] };
        let w = bounds.size.width as f32;
        let h = bounds.size.height as f32;
        if w > 0.0 && h > 0.0 {
            let dims = window.common.current_window_state().size.dimensions;
            if (dims.width - w).abs() > 0.5 || (dims.height - h).abs() > 0.5 {
                log_info!(
                    LogCategory::Window,
                    "[iOS] layoutSubviews: bounds {}x{} -> {}x{}",
                    dims.width,
                    dims.height,
                    w,
                    h,
                );
                window
                    .common
                    .update_window_state(event::WindowStateSource::Os, |ws| {
                        ws.size.dimensions.width = w;
                        ws.size.dimensions.height = h;
                    });
                // `Resize` is the tag every desktop backend gives geometry
                // changes — the variant responsive layout callbacks branch
                // on; `RefreshDom` hid every rotation / split-view resize
                // from them. Preserve a still-pending `Initial` for the
                // launch pass (the first layoutSubviews can legitimately
                // differ from the UIScreen bounds used at construction,
                // e.g. iPad multitasking).
                let reason = if window.common.regeneration_pending()
                    && window.common.regeneration_reason() == RelayoutReason::Initial
                {
                    RelayoutReason::RefreshDom
                } else {
                    RelayoutReason::Resize
                };
                window.common.request_regeneration(reason);
            }
        }
        // Safe-area insets (notch / Dynamic Island / home indicator / rounded
        // corners) from the view, so get_safe_area_insets + CSS
        // env(safe-area-inset-*) reflect the device.
        let insets: UIEdgeInsets =
            unsafe { msg_send![this as *const Object as *mut Object, safeAreaInsets] };
        if let Some(lw) = window.common.layout_window.as_mut() {
            let mk = |v: f64| {
                if v > 0.5 {
                    azul_css::props::basic::OptionPixelValue::Some(
                        azul_css::props::basic::PixelValue::px(v as f32),
                    )
                } else {
                    azul_css::props::basic::OptionPixelValue::None
                }
            };
            lw.safe_area_insets = azul_css::system::SafeAreaInsets {
                top: mk(insets.top),
                bottom: mk(insets.bottom),
                left: mk(insets.left),
                right: mk(insets.right),
                // No keyboard inset at this site: a titlebar/desktop
                // surface never has an on-screen keyboard over it.
                keyboard: azul_css::props::basic::pixel::OptionPixelValue::None,
            };
        }
    }
    let _ = sel!(layoutSubviews);
}

/// `UIEdgeInsets` { top, left, bottom, right } (CGFloat) — for the struct-return
/// `msg_send![view, safeAreaInsets]`, mirroring the CoreLocation coordinate
/// pattern in geolocation/macos.rs (no UIKit-sys dependency).
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
struct UIEdgeInsets {
    top: f64,
    left: f64,
    bottom: f64,
    right: f64,
}
unsafe impl objc::Encode for UIEdgeInsets {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str("{UIEdgeInsets=dddd}") }
    }
}

// ─── UIKit gesture-recognizer handlers (Sprint M iOS side) ───────────
//
// Each handler is attached as the `action:` selector of a
// `UI*GestureRecognizer` instance constructed in `IOSWindow::new`. UIKit
// fires the action with `(sender: UIGestureRecognizer*)`; we read the
// recognizer's `state` to decide whether to inject. For tap/long-press
// we only inject on `UIGestureRecognizerStateRecognized` (== 3) /
// `Began` (== 1); for continuous recognizers (pinch / rotation) we'd
// inject on `Changed` (== 2). Action-selector signatures: the Rust
// function takes `(target: &Object, _cmd: Sel, sender: *mut Object)`.

const UI_GESTURE_RECOGNIZER_STATE_RECOGNIZED: i64 = 3;
const UI_GESTURE_RECOGNIZER_STATE_BEGAN: i64 = 1;
const UI_GESTURE_RECOGNIZER_STATE_CHANGED: i64 = 2;

fn inject(window: &mut IOSWindow, gesture: azul_layout::managers::gesture::NativeGestureEvent) {
    if let Some(lw) = window.common.layout_window.as_mut() {
        lw.gesture_drag_manager.inject_native_gesture(gesture);
        window
            .common
            .request_regeneration(RelayoutReason::RefreshDom);
    }
}

extern "C" fn on_double_tap(_this: &Object, _cmd: Sel, sender: *mut Object) {
    use azul_layout::managers::gesture::NativeGestureEvent;
    let state: i64 = unsafe { msg_send![sender, state] };
    if state != UI_GESTURE_RECOGNIZER_STATE_RECOGNIZED {
        return;
    }
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(w, NativeGestureEvent::DoubleClick);
    }
}

extern "C" fn on_long_press(_this: &Object, _cmd: Sel, sender: *mut Object) {
    use azul_core::geom::LogicalPosition;
    use azul_layout::managers::gesture::{DetectedLongPress, NativeGestureEvent};
    let state: i64 = unsafe { msg_send![sender, state] };
    if state != UI_GESTURE_RECOGNIZER_STATE_BEGAN {
        return;
    }
    let p: CGPoint = unsafe { msg_send![sender, locationInView: ptr::null_mut::<Object>()] };
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(
            w,
            NativeGestureEvent::LongPress(DetectedLongPress {
                position: LogicalPosition {
                    x: p.x as f32,
                    y: p.y as f32,
                },
                duration_ms: 0,
                callback_invoked: false,
                session_id: 0,
            }),
        );
    }
}

extern "C" fn on_swipe_left(_t: &Object, _c: Sel, _s: *mut Object) {
    use azul_layout::managers::gesture::{GestureDirection, NativeGestureEvent};
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(w, NativeGestureEvent::Swipe(GestureDirection::Left));
    }
}
extern "C" fn on_swipe_right(_t: &Object, _c: Sel, _s: *mut Object) {
    use azul_layout::managers::gesture::{GestureDirection, NativeGestureEvent};
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(w, NativeGestureEvent::Swipe(GestureDirection::Right));
    }
}
extern "C" fn on_swipe_up(_t: &Object, _c: Sel, _s: *mut Object) {
    use azul_layout::managers::gesture::{GestureDirection, NativeGestureEvent};
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(w, NativeGestureEvent::Swipe(GestureDirection::Up));
    }
}
extern "C" fn on_swipe_down(_t: &Object, _c: Sel, _s: *mut Object) {
    use azul_layout::managers::gesture::{GestureDirection, NativeGestureEvent};
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(w, NativeGestureEvent::Swipe(GestureDirection::Down));
    }
}

extern "C" fn on_pinch(_this: &Object, _cmd: Sel, sender: *mut Object) {
    use azul_core::geom::LogicalPosition;
    use azul_layout::managers::gesture::{DetectedPinch, NativeGestureEvent};
    let state: i64 = unsafe { msg_send![sender, state] };
    if state != UI_GESTURE_RECOGNIZER_STATE_CHANGED {
        return;
    }
    let scale: f64 = unsafe { msg_send![sender, scale] };
    let p: CGPoint = unsafe { msg_send![sender, locationInView: ptr::null_mut::<Object>()] };
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(
            w,
            NativeGestureEvent::Pinch(DetectedPinch {
                scale: scale as f32,
                center: LogicalPosition {
                    x: p.x as f32,
                    y: p.y as f32,
                },
                initial_distance: 0.0,
                current_distance: 0.0,
                duration_ms: 0,
            }),
        );
    }
}

extern "C" fn on_rotation(_this: &Object, _cmd: Sel, sender: *mut Object) {
    use azul_core::geom::LogicalPosition;
    use azul_layout::managers::gesture::{DetectedRotation, NativeGestureEvent};
    let state: i64 = unsafe { msg_send![sender, state] };
    if state != UI_GESTURE_RECOGNIZER_STATE_CHANGED {
        return;
    }
    let rotation: f64 = unsafe { msg_send![sender, rotation] };
    let p: CGPoint = unsafe { msg_send![sender, locationInView: ptr::null_mut::<Object>()] };
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(
            w,
            NativeGestureEvent::Rotation(DetectedRotation {
                angle_radians: rotation as f32,
                center: LogicalPosition {
                    x: p.x as f32,
                    y: p.y as f32,
                },
                duration_ms: 0,
            }),
        );
    }
}

/// Dynamically register an empty NSObject subclass whose only purpose
/// is to be the `target:` of every gesture recognizer. UIKit expects an
/// Objective-C object; an empty subclass is the cheapest legal one.
fn get_or_create_gesture_target_class() -> &'static Class {
    static ONCE: Once = Once::new();
    static mut CLS: *const Class = ptr::null();
    ONCE.call_once(|| unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("AzulGestureTarget", superclass).unwrap();
        decl.add_method(
            sel!(onDoubleTap:),
            on_double_tap as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(onLongPress:),
            on_long_press as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(onSwipeLeft:),
            on_swipe_left as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(onSwipeRight:),
            on_swipe_right as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(onSwipeUp:),
            on_swipe_up as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(onSwipeDown:),
            on_swipe_down as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(onPinch:),
            on_pinch as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(onRotation:),
            on_rotation as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(displayTick:),
            display_tick as extern "C" fn(&Object, Sel, *mut Object),
        );
        CLS = decl.register();
    });
    unsafe { &*CLS }
}

extern "C" fn display_tick(_this: &Object, _cmd: Sel, _link: *mut Object) {
    // Cheap rate limit: only ask for a redraw when the layout has changed since
    // the last frame.
    //
    // The comment here used to claim "touch / timer / async-thread results all
    // flip it". That was false for two of the three: process_timers_and_threads
    // had NO call site anywhere in ios/mod.rs, so no Timer ever fired, no
    // background Thread writeback was ever collected, and no animation advanced
    // on iOS. Only touch flipped the flag. The CADisplayLink tick is the natural
    // place to drive it — it fires every screen refresh — so it now does, and the
    // comment is true.
    if let Some(window) = unsafe { azul_ios_window() } {
        if window.process_timers_and_threads() {
            window
                .common
                .request_regeneration(RelayoutReason::RefreshDom);
        }
        // Accessibility actions arrive off-loop (VoiceOver invokes
        // `accessibilityActivate` and friends whenever the user gestures) and
        // are queued, not applied, so that UIKit is never re-entered
        // mid-traversal. The display tick is where they get applied — the same
        // per-frame slot `run.rs` gives the four desktop backends.
        #[cfg(feature = "a11y")]
        window.process_accessibility_actions();
        // The device light/dark setting. iOS built its window from
        // SystemStyle::default() and never read the real one, so an app launched
        // on a dark-mode device rendered light and stayed light. Polled here
        // because UITraitCollection is main-thread-only and this tick is the
        // main thread.
        #[cfg(target_os = "ios")]
        if unsafe { adopt_device_appearance(&mut window.common) } {
            let _ = window.process_window_events(0);
            window
                .common
                .request_regeneration(RelayoutReason::ThemeChange);
        }
        if window.common.regeneration_pending() {
            let _ = window.present();
        }
    }
}

/// The device's light/dark setting, as a [`WindowTheme`].
///
/// `UITraitCollection.currentTraitCollection.userInterfaceStyle`:
/// 0 = unspecified, 1 = light, 2 = dark (`UIUserInterfaceStyle`). Unspecified
/// yields `None` — it means UIKit is not expressing a preference, so whatever
/// the window already carries is the better answer.
///
/// MAIN THREAD ONLY, like all of UIKit. That is why this is polled from the
/// display tick rather than from a watcher thread, which is the opposite of the
/// Linux backends: there the probe is a blocking D-Bus round trip and MUST be
/// threaded, here it is one message send and must NOT be.
#[cfg(target_os = "ios")]
unsafe fn probe_user_interface_style() -> Option<azul_core::window::WindowTheme> {
    let traits: *mut Object = msg_send![class!(UITraitCollection), currentTraitCollection];
    if traits.is_null() {
        return None;
    }
    let style: i64 = msg_send![traits, userInterfaceStyle];
    match style {
        1 => Some(azul_core::window::WindowTheme::LightMode),
        2 => Some(azul_core::window::WindowTheme::DarkMode),
        _ => None,
    }
}

/// Adopt the device appearance into `common`, returning whether it changed.
///
/// iOS did not read the setting AT ALL — the window is built with
/// `SystemStyle::default()`, so an app launched on a dark-mode device rendered
/// light and stayed light. This is therefore the initial read as much as the
/// change notification; the display tick runs it every frame and it returns
/// `false` whenever the theme already matches, so a steady state costs one
/// message send.
#[cfg(target_os = "ios")]
unsafe fn adopt_device_appearance(
    common: &mut crate::desktop::shell2::common::event::CommonWindowState,
) -> bool {
    let Some(theme) = probe_user_interface_style() else {
        return false;
    };
    if common.current_window_state().theme == theme {
        return false;
    }
    // The diff pipeline compares against previous_window_state to decide a
    // ThemeChanged event fired; without this snapshot no callback runs.
    common.snapshot_window_state_baseline("ios.adopt_device_appearance");
    common.update_unsynced_state(|ws| ws.theme = theme);
    true
}

/// Construct a `CADisplayLink` that fires `display_tick:` on the shared
/// AzulGestureTarget every screen refresh and add it to the main run loop.
unsafe fn install_display_link(_view: *mut Object) {
    use objc::sel;
    // The gesture target class also carries the display tick selector —
    // one extra method, same shared NSObject instance (constructed in
    // install_gesture_recognizers ahead of this call).
    let target_class = get_or_create_gesture_target_class();
    let target_alloc: *mut Object = msg_send![target_class, alloc];
    let target: *mut Object = msg_send![target_alloc, init];

    let link: *mut Object = msg_send![
        class!(CADisplayLink),
        displayLinkWithTarget: target
                      selector: sel!(displayTick:)
    ];
    let main_loop: *mut Object = msg_send![class!(NSRunLoop), mainRunLoop];
    let default_mode_cstr = b"kCFRunLoopDefaultMode\0".as_ptr() as *const i8;
    let mode: *mut Object = msg_send![class!(NSString), stringWithUTF8String: default_mode_cstr];
    let _: () = msg_send![link, addToRunLoop: main_loop forMode: mode];

    // Keep a (retained) handle so the background/foreground lifecycle hooks
    // can pause the tick — `displayLinkWithTarget:` returns an autoreleased
    // object that only the run loop keeps alive otherwise.
    let _: *mut Object = msg_send![link, retain];
    AZUL_IOS_DISPLAY_LINK = link;
}

/// Attach UITap / UILongPress / UISwipe(×4) / UIPinch / UIRotation
/// recognizers to `view`. The shared `target` object is leaked — its
/// lifetime is tied to the application.
unsafe fn install_gesture_recognizers(view: *mut Object) {
    use objc::sel;
    let target_class = get_or_create_gesture_target_class();
    let target_alloc: *mut Object = msg_send![target_class, alloc];
    let target: *mut Object = msg_send![target_alloc, init];

    // Helper closure to alloc + init + addGestureRecognizer:
    let attach_basic = |class_name: &Class, action: Sel| {
        let r_alloc: *mut Object = msg_send![class_name, alloc];
        let r: *mut Object = msg_send![r_alloc, initWithTarget: target action: action];
        let _: () = msg_send![view, addGestureRecognizer: r];
        r
    };

    // Double-tap (UITapGestureRecognizer with numberOfTapsRequired = 2)
    let tap = attach_basic(class!(UITapGestureRecognizer), sel!(onDoubleTap:));
    let _: () = msg_send![tap, setNumberOfTapsRequired: 2i64];

    let _ = attach_basic(class!(UILongPressGestureRecognizer), sel!(onLongPress:));
    let _ = attach_basic(class!(UIPinchGestureRecognizer), sel!(onPinch:));
    let _ = attach_basic(class!(UIRotationGestureRecognizer), sel!(onRotation:));

    // Swipe recognizers need one instance per direction (UISwipeGestureRecognizer's
    // `direction` is a bitmask but UIKit fires the action once per direction).
    // direction enum values: Right=1, Left=2, Up=4, Down=8.
    let attach_swipe = |dir: u64, action: Sel| {
        let r_alloc: *mut Object = msg_send![class!(UISwipeGestureRecognizer), alloc];
        let r: *mut Object = msg_send![r_alloc, initWithTarget: target action: action];
        let _: () = msg_send![r, setDirection: dir];
        let _: () = msg_send![view, addGestureRecognizer: r];
    };
    attach_swipe(1, sel!(onSwipeRight:));
    attach_swipe(2, sel!(onSwipeLeft:));
    attach_swipe(4, sel!(onSwipeUp:));
    attach_swipe(8, sel!(onSwipeDown:));
}

fn get_or_create_view_class() -> &'static Class {
    static ONCE: Once = Once::new();
    static mut AZUL_VIEW_CLASS: *const Class = ptr::null();
    ONCE.call_once(|| unsafe {
        let superclass = class!(UIView);
        let mut decl = ClassDecl::new("AzulView", superclass).unwrap();

        // `displayLayer:` (CALayerDelegate), deliberately NOT `drawRect:`.
        // A view that implements `drawRect:` gets a CGContext backing store,
        // and the view machinery assigns THAT to `layer.contents` right after
        // `drawRect:` returns — clobbering any contents set manually inside
        // it, which is exactly what the first version of this backend did
        // (the screen stayed empty). Implementing `displayLayer:` instead
        // makes Core Animation hand the display pass to us, no backing store
        // is created, and the `layer.contents = CGImage` assignment sticks.
        decl.add_method(
            sel!(displayLayer:),
            display_layer as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(touchesBegan:withEvent:),
            touches_began as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(touchesMoved:withEvent:),
            touches_moved as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(touchesEnded:withEvent:),
            touches_ended as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(touchesCancelled:withEvent:),
            touches_cancelled as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        // Text input. UIKit will not deliver text to a view that does not
        // declare it wants it, and `canBecomeFirstResponder` returning false
        // (the default) means it is never even asked — which is why the shell
        // could not type despite having a full touch path.
        decl.add_method(
            sel!(canBecomeFirstResponder),
            ui_can_become_first_responder as extern "C" fn(&Object, Sel) -> bool,
        );
        decl.add_method(sel!(hasText), ui_has_text as extern "C" fn(&Object, Sel) -> bool);
        decl.add_method(
            sel!(insertText:),
            ui_insert_text as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(deleteBackward),
            ui_delete_backward as extern "C" fn(&Object, Sel),
        );
        // Hardware keys and TV-remote buttons — a separate stream from both
        // touches and UIKeyInput.
        decl.add_method(
            sel!(pressesBegan:withEvent:),
            ui_presses_began as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(pressesEnded:withEvent:),
            ui_presses_ended as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(layoutSubviews),
            layout_subviews as extern "C" fn(&Object, Sel),
        );

        // UIAccessibilityContainer conformance. `UIAccessibilityContainer` is
        // an INFORMAL protocol (an NSObject category), so it must NOT go
        // through `add_protocol` — `Protocol::get` returns None for it and the
        // unwrap would abort at launch. Adding the four methods IS the
        // conformance. Without them, VoiceOver sees one opaque view: every
        // button, link and text node azul draws is invisible to it, which is
        // the state iOS shipped in.
        #[cfg(feature = "a11y")]
        accessibility::install_container_methods(&mut decl);

        // UIKeyInput conformance. Unlike UIAccessibilityContainer this IS a
        // formal protocol, but it is declared by UIKit and only present once
        // UIKit has loaded — so the lookup is checked rather than unwrapped,
        // the way the note above warns about. The three methods are already
        // added; declaring conformance is what makes UIKit ASK for them and
        // raise the keyboard.
        if let Some(p) = Protocol::get("UIKeyInput") {
            decl.add_protocol(p);
        }

        AZUL_VIEW_CLASS = decl.register();
    });
    unsafe { &*AZUL_VIEW_CLASS }
}

// ─── AppDelegate ──────────────────────────────────────────────────────

extern "C" fn did_finish_launching(
    _this: &Object,
    _cmd: Sel,
    _app: *mut Object,
    _opts: *mut Object,
) -> bool {
    unsafe {
        let (app_data, undo_manager, config, fc_cache, font_registry, root_window) =
            match super::run::INITIAL_OPTIONS.take() {
                Some(opts) => opts,
                None => {
                    log_error!(
                        LogCategory::EventLoop,
                        "[iOS] did_finish_launching: INITIAL_OPTIONS unset — \
                         azul_run() must run before UIApplicationMain"
                    );
                    return false;
                }
            };

        let window = match IOSWindow::new(
            root_window,
            fc_cache,
            config,
            app_data,
            undo_manager,
            font_registry,
        ) {
            Ok(w) => w,
            Err(e) => {
                log_error!(LogCategory::EventLoop, "[iOS] IOSWindow::new: {:?}", e);
                return false;
            }
        };
        AZUL_IOS_WINDOW = Box::into_raw(Box::new(window));
        log_info!(
            LogCategory::EventLoop,
            "[iOS] application:didFinishLaunching: ok"
        );
    }
    true
}

// ─── Lifecycle selectors ──────────────────────────────────────────────
//
// UIApplicationDelegate's four foreground/background hooks. They give
// Azul a chance to pause timers / save state when the app heads to the
// background, and to resume / refresh when it returns. For now each is
// a logged stub; concrete pause/resume goes into Sprint M-iOS-life. The
// existence of the methods means the AppDelegate conforms to the full
// protocol — the runtime won't post warnings about missing optional
// methods.

extern "C" fn app_did_become_active(_this: &Object, _cmd: Sel, _app: *mut Object) {
    log_info!(LogCategory::EventLoop, "[iOS] applicationDidBecomeActive:");
    // Covers the resume path that skips willEnterForeground (first launch
    // does both; unpausing twice is a harmless idempotent setter).
    unsafe { set_display_link_paused(false) };
    if let Some(window) = unsafe { azul_ios_window() } {
        // Force a redraw so the layer contents are fresh after
        // returning from background.
        window
            .common
            .request_regeneration(RelayoutReason::RefreshDom);
        let _ = window.present();
    }
}

extern "C" fn app_will_resign_active(_this: &Object, _cmd: Sel, _app: *mut Object) {
    log_info!(LogCategory::EventLoop, "[iOS] applicationWillResignActive:");
    // Deliberately does NOT pause the display link: resign-active also fires
    // for transient overlays (control center, incoming-call banner, app
    // switcher) where the app remains visible and animations should keep
    // running. The pause happens in applicationDidEnterBackground:.
}

extern "C" fn app_did_enter_background(_this: &Object, _cmd: Sel, _app: *mut Object) {
    log_info!(
        LogCategory::EventLoop,
        "[iOS] applicationDidEnterBackground:"
    );
    // Stop the render tick. CADisplayLink is NOT auto-paused in the
    // background — the process is merely suspended a few seconds from now,
    // and until then every vsync would run layout/CPU-render/CA-commit for
    // an invisible app (the earlier comment claiming the link "already stops
    // firing when the app is inactive" was wrong on both counts).
    // iOS gives ~5 s of background time. Sprint M-iOS-life will use it
    // to checkpoint app_data / hand off to BGTaskScheduler.
    unsafe { set_display_link_paused(true) };
}

extern "C" fn app_will_enter_foreground(_this: &Object, _cmd: Sel, _app: *mut Object) {
    log_info!(
        LogCategory::EventLoop,
        "[iOS] applicationWillEnterForeground:"
    );
    unsafe { set_display_link_paused(false) };
}

extern "C" fn app_will_terminate(_this: &Object, _cmd: Sel, _app: *mut Object) {
    log_info!(LogCategory::EventLoop, "[iOS] applicationWillTerminate:");
    unsafe {
        if !AZUL_IOS_WINDOW.is_null() {
            // Drop the window in a controlled scope so its CommonWindowState
            // releases the RefAny + LayoutWindow before the process dies.
            let _ = Box::from_raw(AZUL_IOS_WINDOW);
            AZUL_IOS_WINDOW = core::ptr::null_mut();
        }
    }
}

fn get_or_create_app_delegate_class() -> &'static Class {
    static ONCE: Once = Once::new();
    static mut APP_DELEGATE_CLASS: *const Class = ptr::null();
    ONCE.call_once(|| unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("AppDelegate", superclass).unwrap();

        decl.add_protocol(Protocol::get("UIApplicationDelegate").unwrap());

        decl.add_method(
            sel!(application:didFinishLaunchingWithOptions:),
            did_finish_launching as extern "C" fn(&Object, Sel, *mut Object, *mut Object) -> bool,
        );

        // Lifecycle hooks — see comments on each function.
        decl.add_method(
            sel!(applicationDidBecomeActive:),
            app_did_become_active as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(applicationWillResignActive:),
            app_will_resign_active as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(applicationDidEnterBackground:),
            app_did_enter_background as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(applicationWillEnterForeground:),
            app_will_enter_foreground as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(applicationWillTerminate:),
            app_will_terminate as extern "C" fn(&Object, Sel, *mut Object),
        );

        APP_DELEGATE_CLASS = decl.register();
    });
    unsafe { &*APP_DELEGATE_CLASS }
}

/// Bootstraps the UIKit run-loop. Never returns.
pub unsafe fn launch_app() {
    let pool = objc_autoreleasePoolPush();
    let _ = get_or_create_app_delegate_class();

    // NSString*: the principal class + delegate class names UIApplicationMain
    // uses to instantiate the application + delegate. `obj_alloc_init` is
    // simpler than constructing an NSString.
    let principal_cstr = b"UIApplication\0".as_ptr() as *const i8;
    let delegate_cstr = b"AppDelegate\0".as_ptr() as *const i8;
    let principal_name: *mut Object =
        msg_send![class!(NSString), stringWithUTF8String: principal_cstr];
    let delegate_name: *mut Object =
        msg_send![class!(NSString), stringWithUTF8String: delegate_cstr];

    UIApplicationMain(0, ptr::null_mut(), principal_name, delegate_name);

    objc_autoreleasePoolPop(pool);
}

// ─── IOSWindow ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderBackend {
    Cpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IOSEvent {
    Close,
}

pub struct IOSWindow {
    /// Cross-platform window state.
    pub common: CommonWindowState,
    /// CPU rendering backend (replaces WebRender).
    pub cpu_backend: CpuBackend,
    /// Native UIWindow.
    ui_window: Id<Object>,
    /// Custom UIView (AzulView subclass).
    ui_view: Id<Object>,
    /// UIViewController.
    ui_view_controller: Id<Object>,
    /// Rendering backend selector (CPU only until Sprint M-iOS-GPU).
    pub backend: RenderBackend,
    /// `false` after `applicationWillTerminate:`.
    pub is_open: bool,
    /// Shared icon provider — needed by `regenerate_layout()`.
    pub icon_provider: SharedIconProvider,
    /// Optional shared font registry for async font discovery.
    pub font_registry: Option<Arc<FcFontRegistry>>,
    /// UIKit accessibility bridge: owns the `UIAccessibilityElement` list
    /// VoiceOver navigates and the queue the actions it invokes land in.
    /// Named to match the four desktop backends' field of the same name; the
    /// difference is that this one is hand-written, because `accesskit` has no
    /// UIKit adapter.
    pub accessibility_adapter: accessibility::IOSAccessibilityAdapter,
}

impl IOSWindow {
    pub fn new(
        options: WindowCreateOptions,
        fc_cache: Arc<FcFontCache>,
        mut config: AppConfig,
        app_data: RefAny,
        undo_manager: event::SharedUndoManager,
        font_registry: Option<Arc<FcFontRegistry>>,
    ) -> Result<Self, WindowError> {
        let mut full_window_state = options.window_state;

        let icon_provider_handle = core::mem::take(&mut config.icon_provider);
        let icon_provider = SharedIconProvider::from_handle(icon_provider_handle);

        let mut layout_window = LayoutWindow::new(fc_cache.as_ref().clone())
            .map_err(|e| WindowError::PlatformError(format!("Layout init failed: {:?}", e)))?;
        layout_window.current_window_state = full_window_state.clone();
        layout_window.routes = config.routes.clone();

        // Build the native UI tree. Bounds come from `[[UIScreen mainScreen] bounds]`.
        let (ui_window, ui_view_controller, ui_view) = unsafe {
            let screen: *mut Object = msg_send![class!(UIScreen), mainScreen];
            let bounds: CGRect = msg_send![screen, bounds];
            // `[screen scale]` is 1 / 2 / 3 (pixels per point). azul-layout
            // uses 96 dpi as its 1× baseline, so dpi = scale × 96 maps
            // 2× retina → 192, 3× retina → 288, both yielding the right
            // `regenerate_layout` dpi_factor. bounds.size is already in
            // points (logical units), so we feed it straight in.
            let scale: f64 = msg_send![screen, scale];
            let dpi = (scale * 96.0).round() as u32;
            full_window_state.size.dpi = dpi.max(1);
            full_window_state.size.dimensions.width = bounds.size.width as f32;
            full_window_state.size.dimensions.height = bounds.size.height as f32;
            log_info!(
                LogCategory::Window,
                "[iOS] screen scale={} -> dpi={} bounds={}x{}",
                scale,
                dpi,
                bounds.size.width,
                bounds.size.height,
            );

            let window_alloc: *mut Object = msg_send![class!(UIWindow), alloc];
            let window: *mut Object = msg_send![window_alloc, initWithFrame: bounds];

            let vc_alloc: *mut Object = msg_send![class!(UIViewController), alloc];
            let vc: *mut Object = msg_send![vc_alloc, init];

            let view_class = get_or_create_view_class();
            let view_alloc: *mut Object = msg_send![view_class, alloc];
            let view: *mut Object = msg_send![view_alloc, initWithFrame: bounds];

            let _: () = msg_send![vc, setView: view];
            let _: () = msg_send![window, setRootViewController: vc];
            let _: () = msg_send![window, makeKeyAndVisible];

            // Attach UIKit gesture recognizers (Sprint M iOS side).
            // Each recognizer forwards to AZUL_IOS_WINDOW.common
            //  .layout_window.gesture_drag_manager.inject_native_gesture
            // so CallbackInfo::get_swipe_direction etc. observe a result.
            install_gesture_recognizers(view);

            // Install a CADisplayLink so the view redraws at the screen
            // refresh rate (60 / 120 Hz). Without this, frames only tick
            // on touch / timer events — fine for forms, wrong for any
            // animation. The display link target is the same shared
            // AzulGestureTarget NSObject; selector goes to `display_tick:`.
            install_display_link(view);

            // `Id::from_ptr` retains the object; balanced by Drop.
            (Id::from_ptr(window), Id::from_ptr(vc), Id::from_ptr(view))
        };

        let mut common = CommonWindowState::new(
            full_window_state,
            fc_cache,
            Arc::new(azul_css::system::SystemStyle::default()),
            Arc::new(RefCell::new(app_data)),
            undo_manager,
        );
        common.layout_window = Some(layout_window);
        common.cpu_hit_tester = Some(azul_layout::headless::CpuHitTester::new());

        Ok(Self {
            common,
            cpu_backend: CpuBackend::new(),
            ui_window,
            ui_view,
            ui_view_controller,
            backend: RenderBackend::Cpu,
            is_open: true,
            icon_provider,
            font_registry,
            accessibility_adapter: accessibility::IOSAccessibilityAdapter::new(),
        })
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }
    pub fn close(&mut self) {
        // WebRender's Renderer must be deinit()'d, not dropped — texture
        // deletion has to happen inside a frame. Never doing so crashed debug
        // builds on close and leaked GPU resources in release.
        self.common.deinit_renderer();
        self.is_open = false;
    }

    pub fn poll_event(&mut self) -> Option<IOSEvent> {
        None
    }

    pub fn present(&mut self) -> Result<(), WindowError> {
        let view = &*self.ui_view as *const Object as *mut Object;
        unsafe {
            let _: () = msg_send![view, setNeedsDisplay];
        }
        Ok(())
    }
    pub fn request_redraw(&mut self) {
        let _ = self.present();
    }

    /// Drain the accessibility actions UIKit queued and apply them.
    ///
    /// Mirrors `Win32Window::process_accessibility_actions` /
    /// `X11Window::process_accessibility_actions`: poll the adapter, route each
    /// action through `LayoutWindow::process_accessibility_action`, dispatch the
    /// callbacks it maps to, honour the `Update` they return. iOS had NO such
    /// method, so `accessibilityActivate` had nowhere to go even once the
    /// container existed.
    ///
    /// Called from the `CADisplayLink` tick rather than from the UIKit callback
    /// itself: applying an action inline would re-enter the layout window while
    /// UIKit is mid-traversal on the main thread, and could run a user callback
    /// that mutates the very tree UIKit is walking.
    #[cfg(feature = "a11y")]
    pub fn process_accessibility_actions(&mut self) {
        let mut actions = Vec::new();
        while let Some(action) = self.accessibility_adapter.poll_action() {
            actions.push(action);
        }
        if actions.is_empty() {
            return;
        }
        self.dispatch_accessibility_actions(actions);
        // Unconditional, like every desktop backend: Focus / Blur / the Scroll*
        // family map to no callback and return an empty affected set while
        // having genuinely changed what is on screen.
        self.request_redraw();
    }

    /// Rebuild the UIKit element list from the current layout.
    ///
    /// `accesskit` has no UIKit backend, so this is iOS's equivalent of the
    /// desktop backends' `adapter.update_tree(tree_update)`.
    #[cfg(feature = "a11y")]
    fn refresh_accessibility_tree(&mut self) {
        let Some(lw) = self.common.layout_window.as_ref() else {
            return;
        };
        let snapshot = lw.build_a11y_snapshot();
        let view = (&*self.ui_view as *const Object) as *mut Object;
        self.accessibility_adapter.update_snapshot(snapshot, view);
        self.common.a11y_dirty = false;
    }

    /// Run a full layout regeneration pass and CPU-render the resulting
    /// display list. Mirrors `AndroidWindow::regenerate_layout()`. Called
    /// from the `displayLayer:` handler when a regeneration is pending
    /// (Sprint C-iOS wires that).
    pub fn regenerate_layout_inner(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        // Captured BEFORE the pass: `regenerate_layout` (the trait wrapper that
        // calls this) drains lifecycle callbacks between passes, and one of
        // those returning `Update::RefreshDom` raises a NEW request. Retiring
        // by epoch at the end means this pass only retires the request it
        // actually saw.
        let regen_epoch_seen = self.common.regen_epoch();

        // Consume the reason tag BEFORE borrowing the layout window: this is
        // the regeneration this window asked for, and the tag travels with
        // the request (see CommonWindowState::request_regeneration).
        let relayout_reason = self.common.take_relayout_reason();

        let borrows = self.common.layout_borrows();
        let layout_window = borrows.layout_window.ok_or("No layout window")?;

        let debug_enabled = crate::desktop::shell2::common::debug_server::is_debug_enabled();
        let mut debug_messages = if debug_enabled {
            Some(Vec::new())
        } else {
            None
        };

        let result = crate::desktop::shell2::common::layout::regenerate_layout(
            layout_window,
            borrows.app_data,
            borrows.current_window_state,
            borrows.renderer_resources,
            borrows.gl_context_ptr,
            borrows.fc_cache,
            &self.font_registry,
            borrows.system_style,
            &self.icon_provider,
            &mut debug_messages,
            relayout_reason,
        )?;

        if let Some(msgs) = debug_messages {
            for msg in msgs {
                crate::desktop::shell2::common::debug_server::log(
                    crate::desktop::shell2::common::debug_server::LogLevel::Debug,
                    crate::desktop::shell2::common::debug_server::LogCategory::Layout,
                    msg.message.as_str().to_string(),
                    None,
                );
            }
        }

        if let Some(lw) = self.common.layout_window.as_ref() {
            self.cpu_backend
                .hit_tester
                .rebuild_from_layout_with_gpu(&lw.layout_results, Some(&lw.gpu_state_manager));
        }

        // Drain lifecycle events (Mount / AfterMount / Unmount) produced by this
        // layout's reconciliation — the SAME step headless + X11 run. Without it,
        // EventFilter::Component(AfterMount) callbacks never fire on iOS (e.g. the
        // MapWidget's first tile fetch never starts).

        // CPU-render the frame — populates `self.cpu_backend.last_frame`,
        // ready for `displayLayer:` to blit into the layer (Sprint C-iOS).
        #[cfg(feature = "cpurender")]
        {
            let ws = self.common.current_window_state();
            let width = ws.size.dimensions.width;
            let height = ws.size.dimensions.height;
            let dpi = ws.size.dpi as f32 / 96.0;
            // Shared per-frame content preparation (journal clock, image
            // callbacks through the content chokepoint, scrollbar cache).
            // Before this, image callbacks were NEVER invoked on this host —
            // every callback image rendered as the announced grey placeholder.
            if let Some(lw) = self.common.layout_window.as_mut() {
                lw.prepare_frame_cpu();
            }
            if let Some(lw) = self.common.layout_window.as_ref() {
                self.cpu_backend.render_frame(
                    lw,
                    &self.common.renderer_resources,
                    width,
                    height,
                    dpi,
                );
            }
        }

        // Republish the accessibility tree. Bounds, labels and the focused
        // element all just changed; VoiceOver reads a cached element list, so
        // without this it would keep highlighting where things USED to be.
        // Same slot the desktop backends push their `TreeUpdate` from.
        #[cfg(feature = "a11y")]
        self.refresh_accessibility_tree();

        self.common
            .clear_regeneration_unless_reraised(regen_epoch_seen);
        Ok(result)
    }
}

impl PlatformWindow for IOSWindow {
    fn regenerate_layout_once(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        // The single pass. The bounded lifecycle loop lives in the trait
        // default `regenerate_layout`, which is what frame paths call.
        self.regenerate_layout_inner()
    }

    impl_platform_window_getters!(common);

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::IOS(IOSHandle {
            ui_window: (&*self.ui_window as *const Object) as *mut c_void,
            ui_view: (&*self.ui_view as *const Object) as *mut c_void,
            ui_view_controller: (&*self.ui_view_controller as *const Object) as *mut c_void,
        })
    }

    fn prepare_callback_invocation(&mut self) -> event::InvokeSingleCallbackBorrows<'_> {
        let borrows = self.common.layout_borrows();
        event::InvokeSingleCallbackBorrows {
            layout_window: borrows
                .layout_window
                .expect("Layout window must exist for callback invocation"),
            window_handle: RawWindowHandle::IOS(IOSHandle {
                ui_window: std::ptr::null_mut(),
                ui_view: std::ptr::null_mut(),
                ui_view_controller: std::ptr::null_mut(),
            }),
            gl_context_ptr: borrows.gl_context_ptr,
            fc_cache_clone: (**borrows.fc_cache).clone(),
            system_style: borrows.system_style.clone(),
            previous_window_state: borrows.previous_window_state,
            current_window_state: borrows.current_window_state,
            renderer_resources: borrows.renderer_resources,
        }
    }

    fn start_timer(&mut self, timer_id: usize, timer: azul_layout::timer::Timer) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            lw.timers
                .insert(azul_core::task::TimerId { id: timer_id }, timer);
        }
    }
    fn stop_timer(&mut self, timer_id: usize) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            lw.timers.remove(&azul_core::task::TimerId { id: timer_id });
        }
    }
    fn start_thread_poll_timer(&mut self) {}
    fn stop_thread_poll_timer(&mut self) {}

    fn add_threads(
        &mut self,
        threads: std::collections::BTreeMap<azul_core::task::ThreadId, azul_layout::thread::Thread>,
    ) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            for (id, thread) in threads {
                lw.threads.insert(id, thread);
            }
        }
    }
    fn remove_threads(
        &mut self,
        thread_ids: &std::collections::BTreeSet<azul_core::task::ThreadId>,
    ) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            for id in thread_ids {
                lw.threads.remove(id);
            }
        }
    }

    fn queue_window_create(&mut self, _options: WindowCreateOptions) {
        // No popup windows on iOS — sub-windows would require a
        // separate UIWindow or modal UIViewController.
    }

    fn show_menu_from_callback(
        &mut self,
        _menu: &azul_core::menu::Menu,
        _position: azul_core::geom::LogicalPosition,
        _anchor: Option<azul_core::geom::LogicalRect>,
    ) {
    }

    fn show_tooltip_from_callback(
        &mut self,
        _text: &str,
        _position: azul_core::geom::LogicalPosition,
    ) {
    }

    fn hide_tooltip_from_callback(&mut self) {}

    /// UIKit owns the window geometry outright, so there is nothing to push and
    /// no OS-sync baseline to diff (`os_synced_state` stays `None`).
    fn sync_window_state(&mut self) {}
}

// ===== Text input and hardware keys =====
//
// UIKit will not send a view text unless the view says it wants it. Two
// protocols do that, at very different costs:
//
// - `UIKeyInput` is three methods (`hasText`, `insertText:`, `deleteBackward`)
//   and is enough to make the soft keyboard appear and deliver typed
//   characters, including from an IME's candidate bar once committed.
// - `UITextInput` is ~25 methods over `UITextPosition` / `UITextRange` object
//   graphs, and buys marked-text (live preedit), the edit menu, and dictation.
//
// This implements `UIKeyInput`, which closes "the shell cannot type at all",
// and leaves the full protocol as a follow-up. That order matters: a partial
// `UITextInput` conformance is worse than none, because UIKit probes for the
// protocol and then calls methods that would have to return real
// `UITextPosition` objects — returning nil from those crashes rather than
// degrades.

/// `UIKeyInput.hasText` — whether there is anything to delete.
extern "C" fn ui_has_text(_this: &Object, _cmd: Sel) -> bool {
    let Some(window) = (unsafe { azul_ios_window() }) else {
        return false;
    };
    window
        .common
        .layout_window
        .as_ref()
        .is_some_and(|lw| lw.text_edit_manager.get_editing_node_id().is_some())
}

/// `UIKeyInput.insertText:` — committed text from the keyboard or an IME.
extern "C" fn ui_insert_text(_this: &Object, _cmd: Sel, text: *mut Object) {
    let Some(window) = (unsafe { azul_ios_window() }) else {
        return;
    };
    let s = unsafe { ns_string_to_rust(text) };
    if s.is_empty() {
        return;
    }
    if let Some(lw) = window.common.layout_window.as_mut() {
        // Commit rather than a bare insert, so CompositionEnd carries the
        // string on iOS exactly as it does on the desktop shells. An IME's
        // candidate bar arrives here already committed — UIKeyInput has no
        // marked-text concept, which is precisely what UITextInput would add.
        lw.text_edit_manager.commit_composition(s.clone());
        let _ = lw.record_text_input(&s);
    }
    let result = window.process_window_events(0);
    window.handle_process_event_result(result);
}

/// `UIKeyInput.deleteBackward` — backspace.
///
/// Separate from `insertText:` because UIKit models it that way: a backspace
/// is never delivered as a key event to a `UIKeyInput` responder, so without
/// this method the key does nothing at all.
extern "C" fn ui_delete_backward(_this: &Object, _cmd: Sel) {
    let Some(window) = (unsafe { azul_ios_window() }) else {
        return;
    };
    if let Some(lw) = window.common.layout_window.as_mut() {
        if let Some(focused) = lw.focus_manager.get_focused_node().copied() {
            lw.delete_selection(focused, true);
        }
    }
    let result = window.process_window_events(0);
    window.handle_process_event_result(result);
}

/// `canBecomeFirstResponder` — required, or UIKit never asks for text.
extern "C" fn ui_can_become_first_responder(_this: &Object, _cmd: Sel) -> bool {
    true
}

/// `pressesBegan:withEvent:` — hardware keyboard and TV-remote buttons.
///
/// `UIPress` is a separate stream from touches and from `UIKeyInput`: an
/// external keyboard's arrow keys, Escape and function keys arrive here and
/// nowhere else, and on tvOS this is the ONLY input path a remote has. Without
/// it an iPad with a Magic Keyboard could type letters (via `insertText:`) but
/// could not move the caret.
extern "C" fn ui_presses_began(this: &Object, _cmd: Sel, presses: *mut Object, event: *mut Object) {
    handle_presses(this, presses, true);
    // Still call super: UIKit routes unhandled presses up the responder chain
    // to the system, and swallowing them breaks the remote's Menu button.
    unsafe { forward_presses_to_super(this, sel!(pressesBegan:withEvent:), presses, event) };
}

extern "C" fn ui_presses_ended(this: &Object, _cmd: Sel, presses: *mut Object, event: *mut Object) {
    handle_presses(this, presses, false);
    unsafe { forward_presses_to_super(this, sel!(pressesEnded:withEvent:), presses, event) };
}

/// Copy an `NSString` into a Rust `String`.
///
/// `UTF8String` returns a pointer into an autoreleased buffer, so it must be
/// copied before returning — holding it past the current autorelease pool is
/// a use-after-free, and this is called from inside UIKit callbacks that pop
/// theirs on return.
unsafe fn ns_string_to_rust(s: *mut Object) -> String {
    if s.is_null() {
        return String::new();
    }
    let utf8: *const core::ffi::c_char = msg_send![s, UTF8String];
    if utf8.is_null() {
        return String::new();
    }
    core::ffi::CStr::from_ptr(utf8)
        .to_string_lossy()
        .into_owned()
}

/// Map a `UIPress.key.keyCode` (a USB HID usage) onto a `VirtualKeyCode`.
///
/// UIKit reports HID usage codes rather than any Apple-specific numbering,
/// which is convenient: these are the same values a USB keyboard puts on the
/// wire, so the table is the HID spec's, not iOS's.
fn ios_hid_keycode_to_virtual(code: i64) -> Option<azul_core::window::VirtualKeyCode> {
    use azul_core::window::VirtualKeyCode as V;
    Some(match code {
        0x4F => V::Right,
        0x50 => V::Left,
        0x51 => V::Down,
        0x52 => V::Up,
        0x28 => V::Return,
        0x29 => V::Escape,
        0x2A => V::Back,
        0x2B => V::Tab,
        0x2C => V::Space,
        0x4A => V::Home,
        0x4D => V::End,
        0x4B => V::PageUp,
        0x4E => V::PageDown,
        0x49 => V::Insert,
        0x4C => V::Delete,
        0x3A..=0x45 => match code {
            0x3A => V::F1,
            0x3B => V::F2,
            0x3C => V::F3,
            0x3D => V::F4,
            0x3E => V::F5,
            0x3F => V::F6,
            0x40 => V::F7,
            0x41 => V::F8,
            0x42 => V::F9,
            0x43 => V::F10,
            0x44 => V::F11,
            _ => V::F12,
        },
        _ => return None,
    })
}

/// Fold a `UIPress` set into the keyboard state.
fn handle_presses(_this: &Object, presses: *mut Object, is_down: bool) {
    use azul_core::window::OptionVirtualKeyCode;

    let Some(window) = (unsafe { azul_ios_window() }) else {
        return;
    };
    unsafe {
        let enumerator: *mut Object = msg_send![presses, objectEnumerator];
        loop {
            let press: *mut Object = msg_send![enumerator, nextObject];
            if press.is_null() {
                break;
            }
            // `key` is nil for a press that is not a keyboard key — a remote's
            // Select or Menu button — so this must be checked rather than
            // assumed.
            let key: *mut Object = msg_send![press, key];
            if key.is_null() {
                continue;
            }
            let code: i64 = msg_send![key, keyCode];
            let Some(vk) = ios_hid_keycode_to_virtual(code) else {
                continue;
            };
            let ks = window.common.keyboard_state_mut();
            if is_down {
                ks.current_virtual_keycode = OptionVirtualKeyCode::Some(vk);
            } else {
                ks.current_virtual_keycode = OptionVirtualKeyCode::None;
            }
        }
    }
    let result = window.process_window_events(0);
    window.handle_process_event_result(result);
}

/// Pass a press set on to `super`, so unhandled keys still reach the system.
unsafe fn forward_presses_to_super(
    this: &Object,
    sel: Sel,
    presses: *mut Object,
    event: *mut Object,
) {
    let superclass: *const Object = msg_send![this, superclass];
    let sup = objc::runtime::Super {
        receiver: this,
        superclass: &*(superclass as *const objc::runtime::Class),
    };
    let _: () = objc::__send_super_message(&sup, sel, (presses, event)).unwrap_or(());
}

// ===== Apple Pencil interactions =====
//
// Squeeze and double-tap do NOT arrive through `touchesBegan:` — they are not
// touches at all. The pencil reports them over its own Bluetooth channel and
// UIKit surfaces them through `UIPencilInteraction`, an interaction object
// attached to a view. Without one, `EventType::PenSqueeze` and `PenDoubleTap`
// have no producer on any platform, which is the state 5b left them in.

/// `UIPencilInteractionDelegate.pencilInteractionDidTap:` — Pencil 2 and
/// later, a double-tap on the barrel.
extern "C" fn pencil_did_tap(_this: &Object, _cmd: Sel, _interaction: *mut Object) {
    inject_pen_gesture(false);
}

/// `UIPencilInteractionDelegate.pencilInteraction:didReceiveSqueeze:` —
/// Pencil Pro only.
///
/// Fires for every phase of the squeeze (began / changed / ended), and only
/// the END is a completed gesture — acting on `began` would fire the moment
/// the user touched the barrel, before they had committed to anything.
extern "C" fn pencil_did_squeeze(
    _this: &Object,
    _cmd: Sel,
    _interaction: *mut Object,
    squeeze: *mut Object,
) {
    // UIPencilInteraction.Squeeze.Phase: 0 = began, 1 = changed, 2 = ended,
    // 3 = cancelled.
    let phase: i64 = unsafe { msg_send![squeeze, phase] };
    if phase != 2 {
        return;
    }
    inject_pen_gesture(true);
}

/// Feed a pen barrel gesture into the engine.
///
/// Both go through the pen state rather than the gesture manager's native
/// injection, because they belong to the PEN — an app reads them from the same
/// place it reads pressure and tilt, and a squeeze while no pen is in
/// proximity is not a thing that can happen.
fn inject_pen_gesture(is_squeeze: bool) {
    let Some(window) = (unsafe { azul_ios_window() }) else {
        return;
    };
    if let Some(lw) = window.common.layout_window.as_mut() {
        lw.gesture_drag_manager.note_pen_barrel_gesture(is_squeeze);
    }
    let result = window.process_window_events(0);
    window.handle_process_event_result(result);
}
