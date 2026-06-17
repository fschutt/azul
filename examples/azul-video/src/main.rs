//! AzVideo — streaming H.264 video player (P6).
//!
//! The `VideoWidget` decodes Big Buck Bunny on a BACKGROUND thread (off the main
//! thread, via Vulkan Video), streaming frames to its `<img>` — there is NO
//! up-front decode, so the window opens immediately and playback is wall-clock
//! paced (late frames dropped). The clip is loaded from a URL via an HTTP range
//! request. Architecturally identical to the slippy-map widget: `dom()` wires a
//! background worker (here the VK decode) that `WriteBack`s results to the widget.

use azul::prelude::*;
use azul::widgets::VideoWidget;
use azul::misc::{VideoConfig, VideoSource};
use azul::desktop::extra::video_codec::VideoStartupCheck;

/// Big Buck Bunny H.264/MP4, 360p — fetched on the decode thread via a range request.
const BBB_URL: &str =
    "https://test-videos.co.uk/vids/bigbuckbunny/mp4/h264/360/Big_Buck_Bunny_360_10s_2MB.mp4";

struct VideoAppState {
    /// Status lines for the side panel (the decode itself runs on the worker thread).
    status: Vec<String>,
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let lines = match data.downcast_ref::<VideoAppState>() {
        Some(s) => s.status.clone(),
        None => Vec::new(),
    };

    let mut root = Dom::create_body().with_css(
        "display: flex; flex-direction: column; height: 100%; box-sizing: border-box; \
         padding: 16px; background: #0e0e14; font-family: sans-serif; color: #e6e6f0;",
    );
    root = root.with_child(
        Dom::create_text("AzVideo — streaming H.264 (Big Buck Bunny)")
            .with_css("font-size: 22px; margin-bottom: 10px;"),
    );
    for line in &lines {
        root = root.with_child(
            Dom::create_text(line.as_str())
                .with_css("font-size: 13px; color: #b8c0d0; margin-bottom: 5px;"),
        );
    }
    root = root.with_child(
        Dom::create_text("streaming from URL — decoded on a background thread (range request)")
            .with_css("font-size: 12px; color: #7ad17a; margin: 10px 0 5px 0;"),
    );

    // The VideoWidget streams: the typed `VideoSource::Url` in the config + `dom()`
    // wires the off-main VK decode worker. Flex-fill the window (no hardcoded size)
    // so the CPU renderer must interpolate/scale the decoded frame (GPU does it
    // natively) — and no border-radius/overflow (those clip the `<img>` in cpurender).
    let mut config = VideoConfig::default();
    config.source = VideoSource::Url(BBB_URL.into());
    root = root.with_child(
        VideoWidget::create(config)
            .dom()
            .with_css("flex-grow: 1; width: 100%; border: 2px solid #2a2a3a;"),
    );

    root
}

fn main() {
    // Probe HW-decode readiness for the status panel; the actual demux+decode
    // happens on the widget's background thread (see VideoWidget::dom()).
    let check = VideoStartupCheck::run();
    let mut status = Vec::new();
    status.push(format!(
        "VK hardware H.264 decode: {} — {}",
        if check.hw_decode_ready { "READY" } else { "not available" },
        check.summary.as_str(),
    ));
    status.push(format!("Source (range request): {}", BBB_URL));
    for line in &status {
        eprintln!("[azvideo] {}", line);
    }

    let data = RefAny::new(VideoAppState { status });
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
