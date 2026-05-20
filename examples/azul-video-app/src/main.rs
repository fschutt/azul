//! AzVideo — a video-playback demo (P6 video-widget demo).
//!
//! One `VideoWidget` in a layout — same "dumb widget" architecture as the
//! camera/screenshare apps, decoding a video source instead. On mount it
//! starts a background decode thread whose writeback uploads each frame into a
//! GL texture and recomposites (no relayout). With the built-in test-pattern
//! worker it runs on any machine (scrolling colour bars); the real vk-video
//! decode + HTTP-range worker + a URL input are follow-ups.
//!
//! Pure public `azul::` surface: `azul::widgets::VideoWidget` +
//! `azul::misc::VideoConfig`.

use azul::prelude::*;
use azul::misc::VideoConfig;
use azul::widgets::VideoWidget;

/// Minimal app state. (A URL input + playback controls land with the
/// control-POD work; for now the source is the config default + the
/// test-pattern worker ignores it.)
struct VideoAppState;

const ROOT: &str = "display: flex; flex-direction: column; height: 100%; \
    align-items: center; justify-content: center; background: #0e0e14; \
    font-family: sans-serif;";
const TITLE: &str = "color: #e6e6f0; font-size: 22px; margin-bottom: 4px;";
const SUBTITLE: &str = "color: #6a7080; font-size: 13px; margin-bottom: 20px;";
const PREVIEW: &str = "width: 640px; height: 360px; border-radius: 10px; \
    border: 2px solid #2a2a3a; background: #16161f; overflow: hidden;";

extern "C" fn layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    // Source is set on the config for the real decoder; the test pattern
    // ignores it, so the default (empty source) is fine here.
    let config = VideoConfig::default();

    Dom::create_body().with_child(
        Dom::create_div()
            .with_css(ROOT)
            .with_child(Dom::create_text("🎬 AzVideo").with_css(TITLE))
            .with_child(Dom::create_text("video playback · VideoWidget").with_css(SUBTITLE))
            .with_child(VideoWidget::create(config).dom().with_css(PREVIEW)),
    )
}

fn main() {
    let data = RefAny::new(VideoAppState);
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
