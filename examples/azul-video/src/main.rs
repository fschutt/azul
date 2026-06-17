//! AzVideo — streaming H.264 player.
//!
//! The `VideoWidget` decodes Big Buck Bunny on a background thread (off-main, via
//! Vulkan Video), streaming frames to a VirtualView `<img>`. The video fills the
//! window via flex (no hardcoded size) with rounded corners + a drop shadow +
//! padding, to validate that the CPU and GPU renderers composite + clip the scaled
//! frame like an image.

use azul::prelude::*;
use azul::widgets::VideoWidget;
use azul::misc::{VideoConfig, VideoSource};
use azul::desktop::extra::video_codec::VideoStartupCheck;

/// Big Buck Bunny H.264/MP4, 360p — fetched on the decode thread via a range request.
const BBB_URL: &str =
    "https://test-videos.co.uk/vids/bigbuckbunny/mp4/h264/360/Big_Buck_Bunny_360_10s_2MB.mp4";

extern "C" fn layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let mut config = VideoConfig::default();
    config.source = VideoSource::Url(BBB_URL.into());

    // Parent fills the window: `display: flex` + `height: 100%` (resolves against
    // the viewport) + padding. The video is a flex child that grows to fill the
    // box, clipped to a rounded rect (overflow: hidden) with a drop shadow — so we
    // can verify the CPU and GPU renderers scale + composite + clip it like an image.
    // Fill the window: body is height:100% (resolves against the viewport) + padding;
    // the video is width:100%/height:100% of the body's content box (flex-grow is
    // avoided — it currently blows the main axis up to inf, a separate solver bug).
    // border-radius + overflow:hidden + box-shadow exercise compositing/clipping.
    Dom::create_body()
        .with_css(
            "height: 100%; margin: 0; box-sizing: border-box; padding: 20px; \
             background: #0e0e14;",
        )
        .with_child(
            VideoWidget::create(config).dom().with_css(
                "width: 100%; height: 100%; border-radius: 16px; overflow: hidden; \
                 box-shadow: 0px 0px 40px #000000;",
            ),
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

    let data = RefAny::new(());
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
