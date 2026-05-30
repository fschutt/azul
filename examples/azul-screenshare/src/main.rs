//! AzScreenShare — a screen-capture demo (P6 screencap-widget demo).
//!
//! One `ScreenCaptureWidget` in a layout — same "dumb widget" architecture as
//! the camera app, capturing a display/window instead of a camera. On mount
//! it starts a background capture thread whose writeback uploads each frame
//! into a GL texture and recomposites (no relayout). With the built-in
//! test-pattern worker it runs on any machine (a moving band); the real
//! ScreenCaptureKit / MediaProjection worker + a display/window picker are
//! follow-ups.
//!
//! Pure public `azul::` surface: `azul::widgets::ScreenCaptureWidget` +
//! `azul::misc::{ScreenCaptureConfig, ScreenCaptureSource}`.

use azul::prelude::*;
use azul::misc::{ScreenCaptureConfig, ScreenCaptureSource};
use azul::widgets::ScreenCaptureWidget;

/// What the preview captures. (A display/window picker lands with the
/// control-POD work; for now it's the primary display.)
struct ScreenShareAppState {
    source: ScreenCaptureSource,
}

impl ScreenShareAppState {
    fn new() -> Self {
        Self {
            source: ScreenCaptureSource::PrimaryDisplay,
        }
    }
}

const ROOT: &str = "display: flex; flex-direction: column; height: 100%; \
    align-items: center; justify-content: center; background: #0e0e14; \
    font-family: sans-serif;";
const TITLE: &str = "color: #e6e6f0; font-size: 22px; margin-bottom: 4px;";
const SUBTITLE: &str = "color: #6a7080; font-size: 13px; margin-bottom: 20px;";
// 16:9 preview box; the widget's <img> fills it with the captured texture.
const PREVIEW: &str = "width: 640px; height: 360px; border-radius: 10px; \
    border: 2px solid #2a2a3a; background: #16161f; overflow: hidden;";

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let source = data
        .downcast_ref::<ScreenShareAppState>()
        .map(|s| s.source)
        .unwrap_or(ScreenCaptureSource::PrimaryDisplay);

    let mut config = ScreenCaptureConfig::default();
    config.source = source;

    Dom::create_body().with_child(
        Dom::create_div()
            .with_css(ROOT)
            .with_child(Dom::create_text("🖥 AzScreenShare").with_css(TITLE))
            .with_child(Dom::create_text("live screen capture · ScreenCaptureWidget").with_css(SUBTITLE))
            .with_child(ScreenCaptureWidget::create(config).dom().with_css(PREVIEW)),
    )
}

fn main() {
    let data = RefAny::new(ScreenShareAppState::new());
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
