//! AzVideo — streaming H.264 player with a scrubbing timeline.
//!
//! The `VideoWidget` decodes Big Buck Bunny on a background thread (off-main, via
//! Vulkan Video), streaming frames to a VirtualView `<img>` that fills the window
//! (rounded + drop-shadowed). A clickable timeline at the bottom sets
//! `VideoConfig.timestamp`; the widget's merge callback turns that change into a
//! seek message to the worker (a wall-clock reposition — no re-decode, no relayout
//! thrash), so the UI stays responsive.

use azul::prelude::*;
use azul::widgets::VideoWidget;
use azul::misc::{VideoConfig, VideoSource, Url};
use azul::desktop::extra::video_codec::VideoStartupCheck;

/// Big Buck Bunny H.264/MP4, 360p, ~10s — fetched on the decode thread via a range request.
const BBB_URL: &str =
    "https://test-videos.co.uk/vids/bigbuckbunny/mp4/h264/360/Big_Buck_Bunny_360_10s_2MB.mp4";
/// Clip length (the BBB sample is ~10s) — maps a timeline click to a timestamp.
const DURATION_SECS: f32 = 10.0;
/// Fixed timeline width in px (so the click→fraction needs only the cursor-in-node x).
const TIMELINE_W: f32 = 600.0;

struct VidApp {
    /// Scrub position in seconds (drives `VideoConfig.timestamp`).
    scrub_secs: f32,
}

/// Timeline click → set the scrub position → RefreshDom (the widget's merge then
/// seeks the worker). `get_cursor_relative_to_node` gives x within the fixed-width bar.
extern "C" fn scrub_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let frac = match info.get_cursor_relative_to_node().into_option() {
        Some(c) => (c.x / TIMELINE_W).clamp(0.0, 1.0),
        None => return Update::DoNothing,
    };
    if let Some(mut st) = data.downcast_mut::<VidApp>() {
        st.scrub_secs = frac * DURATION_SECS;
    }
    Update::RefreshDom
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let scrub = data
        .downcast_ref::<VidApp>()
        .map(|s| s.scrub_secs)
        .unwrap_or(0.0);

    let mut config = VideoConfig::default();
    // Parse into the typed `Url` when the parser is available, otherwise fall back
    // to an href-only `Url` (the decode worker only reads `Url::as_str()`).
    config.source = VideoSource::Url(
        Url::parse(BBB_URL).unwrap_or_else(|_| Url {
            href: BBB_URL.into(),
            ..Default::default()
        }),
    );
    config.timestamp = scrub;

    let frac_pct = ((scrub / DURATION_SECS).clamp(0.0, 1.0) * 100.0) as u32;
    let playhead_css = format!(
        "position: absolute; top: -4px; left: {}%; margin-left: -7px; width: 14px; \
         height: 14px; background: #ffffff; border-radius: 7px;",
        frac_pct
    );

    // Video fills the window (width/height:100% — flex-grow still blows the main axis
    // to inf, a separate solver bug); the timeline is an absolute overlay at the bottom.
    Dom::create_body()
        .with_css(
            "position: relative; height: 100%; margin: 0; box-sizing: border-box; \
             padding: 20px; background: #0e0e14;",
        )
        .with_child(
            VideoWidget::create(config).dom().with_css(
                "width: 100%; height: 100%; border-radius: 16px; overflow: hidden; \
                 box-shadow: 0px 0px 40px #000000;",
            ),
        )
        .with_child(
            Dom::create_div()
                .with_css(
                    "position: absolute; bottom: 40px; left: 40px; width: 600px; \
                     height: 8px; background: #3a3a4a; border-radius: 4px;",
                )
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    scrub_click,
                )
                .with_child(Dom::create_div().with_css(playhead_css.as_str())),
        )
}

fn main() {
    let check = VideoStartupCheck::run();
    eprintln!(
        "[azvideo] VK hardware H.264 decode: {} — {}",
        if check.hw_decode_ready { "READY" } else { "not available" },
        check.summary.as_str(),
    );
    eprintln!("[azvideo] streaming (range request): {}", BBB_URL);
    eprintln!("[azvideo] click the timeline to scrub");

    let data = RefAny::new(VidApp { scrub_secs: 0.0 });
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
