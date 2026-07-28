//! Does a mounted DOM actually reach a framebuffer?
//!
//! NOTHING in this repo's test suite exercised a real present path on any
//! backend, and that absence is why seven redraw bugs accumulated unnoticed and
//! were all found in a single evening by reading code rather than by running it:
//!
//!   * an occluded Wayland window blocked FOREVER inside eglSwapBuffers,
//!     because Mesa's own throttle waits on a frame callback the compositor is
//!     entitled to withhold (b94eeb146);
//!   * azul's own frame-callback latch had no watchdog and was armed even on
//!     frames that committed no buffer (dc4ab4ebb, 4b98281e4);
//!   * a Mount/AfterMount callback returning Update::RefreshDom was ignored on
//!     ALL SEVEN backends (bdc595c62);
//!   * a redraw request raised DURING a render was erased by the clear that
//!     followed it (fac29bb28);
//!   * macOS cleared the regeneration flag after an async setNeedsDisplay that
//!     performed no layout, so drawRect: blitted a STALE frame — in the DEFAULT
//!     configuration (1e0998b48);
//!   * an X11 window that had just been mapped stayed BLANK (d12847735);
//!   * a Wayland popup painted once and could never repaint (9ba9745d0).
//!
//! Every one of those ends in the same user-visible symptom — the screen does
//! not show what the DOM says — and not one could turn a test red.
//!
//! headless is the only backend that can be driven in CI, and it has a real CPU
//! present path (`cpu_backend.render_frame` -> `cpu_backend.last_frame`). So
//! these assert the weakest thing that is still worth something: after mounting a
//! DOM with visible content, PIXELS EXIST and they are not uniformly the
//! background colour.
//!
//! This does not cover the GPU paths, or Wayland/X11/Windows/macOS presentation,
//! which is where most of the bugs above actually lived. It is a floor, not a
//! ceiling — the real fix is a per-backend present harness. Treat a failure here
//! as "the compositor-independent half is broken too".

use std::cell::RefCell;
use std::sync::Arc;

use azul_core::callbacks::{LayoutCallback, LayoutCallbackInfo};
use azul_core::dom::{Dom, NodeData};
use azul_core::icon::{IconProviderHandle, SharedIconProvider};
use azul_core::refany::RefAny;
use azul_core::resources::AppConfig;
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

use azul::desktop::shell2::common::PlatformWindow;
use azul::desktop::shell2::headless::HeadlessWindow;

/// A body with one opaque child, so a correct render cannot produce a uniformly
/// blank framebuffer.
extern "C" fn layout_solid_block(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let mut block = NodeData::create_div();
    block.set_style(azul_css::css::Css::from_string(
        "* { width: 80px; height: 40px; background: #ff0000; }".into(),
    ));
    Dom::create_body().with_child(Dom::create_from_data(block))
}

fn make_window(cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom) -> HeadlessWindow {
    let mut options = WindowCreateOptions::default();
    options.window_state.layout_callback = LayoutCallback {
        cb,
        ctx: azul_core::refany::OptionRefAny::None,
    };
    HeadlessWindow::new(
        options,
        Arc::new(RefCell::new(RefAny::new(()))),
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        AppConfig::default(),
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::default()),
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

/// A DOM with visible content must produce a framebuffer.
#[test]
fn a_mounted_dom_reaches_the_framebuffer() {
    let mut window = make_window(layout_solid_block);
    window
        .regenerate_layout()
        .expect("regenerate_layout must succeed");

    assert!(
        window.cpu_backend.last_frame.is_some(),
        "no frame was produced at all — regenerate_layout completed but nothing reached \
         cpu_backend.last_frame, so there is no present path to speak of",
    );
}

/// ...and that framebuffer must not be uniformly one colour.
///
/// This is the assertion that separates "rendered" from "cleared". Every blank-
/// window bug in the list at the top of this file produced a framebuffer; what
/// they failed to produce was CONTENT. A test that only checked `is_some()`
/// would have gone green through all of them.
#[test]
fn the_framebuffer_is_not_uniformly_blank() {
    let mut window = make_window(layout_solid_block);
    window
        .regenerate_layout()
        .expect("regenerate_layout must succeed");

    let frame = window
        .cpu_backend
        .last_frame
        .as_ref()
        .expect("no frame produced — see a_mounted_dom_reaches_the_framebuffer");

    let data = frame.data();
    assert!(
        !data.is_empty(),
        "the framebuffer has zero bytes, so the window rendered at a degenerate size",
    );

    // Compare whole pixels, not bytes: a uniform colour is uniform per 4-byte
    // group, and a byte-wise scan would call a red-on-white image "varied"
    // simply because R differs from G within one pixel.
    let first = data.chunks_exact(4).next().unwrap_or(&[0, 0, 0, 0]);
    let varied = data.chunks_exact(4).any(|px| px != first);

    assert!(
        varied,
        "every pixel is identical ({first:?}) — the DOM declared an 80x40 opaque block and none \
         of it reached the framebuffer. This is the blank-window class: the frame exists, the \
         content does not.",
    );
}
