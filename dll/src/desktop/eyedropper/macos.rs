//! macOS eyedropper: the system's own `NSColorSampler` (10.15+).
//!
//! AppKit shows the familiar magnifier loupe, handles the click / Escape,
//! and hands the colour to the completion block on the main thread. No
//! screen-recording permission is involved - the system samples on the
//! app's behalf, which is why this beats reading the screen ourselves
//! (`CGDisplayCreateImage` would need the Screen Recording grant, which
//! only takes effect after an app restart: a terrible thing to ask for a
//! colour picker).
//!
//! The sampler retains itself for the session; we keep nothing. The block
//! parks the answer in the layout manager's result channel and wakes every
//! window so the asking window's next pass reads it.

use azul_css::props::basic::color::ColorU;
use block2::RcBlock;
use objc2_app_kit::{NSColor, NSColorSampler, NSColorSpace};

/// Start a sampling session for `request_id`. Returns `false` when the
/// class is missing (pre-10.15), so the caller can fall back.
#[must_use]
pub fn start(request_id: u64) -> bool {
    use objc2::{class, msg_send, runtime::AnyClass};
    // `NSColorSampler` is 10.15+: probe the class before touching it, the
    // way the rest of the macOS backend treats newer AppKit.
    let Some(_cls) = AnyClass::get(c"NSColorSampler") else {
        crate::plog_warn!("[eyedropper] macos: NSColorSampler unavailable (macOS < 10.15)");
        return false;
    };
    let sampler: objc2::rc::Retained<NSColorSampler> = unsafe {
        let alloc: objc2::rc::Allocated<NSColorSampler> = msg_send![class!(NSColorSampler), alloc];
        NSColorSampler::init(alloc)
    };
    let handler = RcBlock::new(move |color: *mut NSColor| {
        let picked = if color.is_null() {
            None
        } else {
            // SAFETY: AppKit passes a live NSColor (or nil, handled above).
            unsafe { srgb_components(&*color) }
        };
        crate::plog_info!("[eyedropper] macos: sampler finished: {:?}", picked);
        super::finish(request_id, picked);
        crate::desktop::shell2::macos::wake_all_windows();
    });
    unsafe {
        sampler.showSamplerWithSelectionHandler(&handler);
    }
    true
}

/// The colour's sRGB components as 8-bit channels (the sampler may return
/// a colour in the display's space; convert first).
unsafe fn srgb_components(color: &NSColor) -> Option<ColorU> {
    let srgb = unsafe { NSColorSpace::sRGBColorSpace() };
    let c = unsafe { color.colorUsingColorSpace(&srgb) }?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=1 clamped
    let ch = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    unsafe {
        Some(ColorU {
            r: ch(c.redComponent()),
            g: ch(c.greenComponent()),
            b: ch(c.blueComponent()),
            a: 255,
        })
    }
}
