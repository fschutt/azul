//! AzCamera — a dummy camera app (P6 camera-widget demo).
//!
//! The whole app is one `CameraWidget` embedded in a layout. The widget is a
//! "dumb widget" (no camera logic in core): on mount it starts a background
//! capture thread whose writeback uploads each frame into a GL texture and
//! recomposites — so the preview updates live without relayout. With the
//! built-in test-pattern worker this runs on any machine (no webcam): you get
//! a colour-cycling preview, which verifies the whole widget pipeline. The
//! real AVFoundation/Camera2 capture worker + a front/back switch button are
//! follow-ups.
//!
//! Pure public `azul::` surface: `azul::widgets::CameraWidget` +
//! `azul::camera::{CameraConfig, CameraFacing}`.

use azul::prelude::*;
use azul::camera::{CameraConfig, CameraFacing};
use azul::widgets::CameraWidget;

/// Which camera the preview requests. (A front/back switch button lands with
/// the control-POD methods in a later tick; for now it's fixed.)
struct CameraAppState {
    facing: CameraFacing,
}

impl CameraAppState {
    fn new() -> Self {
        Self {
            facing: CameraFacing::Front,
        }
    }
}

const ROOT: &str = "display: flex; flex-direction: column; height: 100%; \
    align-items: center; justify-content: center; background: #0e0e14; \
    font-family: sans-serif;";
const TITLE: &str = "color: #e6e6f0; font-size: 22px; margin-bottom: 4px;";
const SUBTITLE: &str = "color: #6a7080; font-size: 13px; margin-bottom: 20px;";
// The preview box — CSS-sized; the widget's <img> fills it with the texture.
const PREVIEW: &str = "width: 480px; height: 360px; border-radius: 12px; \
    border: 2px solid #2a2a3a; background: #16161f; overflow: hidden;";

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let facing = data
        .downcast_ref::<CameraAppState>()
        .map(|s| s.facing)
        .unwrap_or(CameraFacing::Front);

    // Backend-default resolution/fps + BGRA8; just pick the camera.
    let mut config = CameraConfig::default();
    config.facing = facing;

    Dom::create_body().with_child(
        Dom::create_div()
            .with_css(ROOT)
            .with_child(Dom::create_text("📷 AzCamera").with_css(TITLE))
            .with_child(Dom::create_text("live preview · CameraWidget").with_css(SUBTITLE))
            .with_child(CameraWidget::create(config).dom().with_css(PREVIEW)),
    )
}

fn main() {
    let data = RefAny::new(CameraAppState::new());
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
