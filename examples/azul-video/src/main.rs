//! AzVideo — H.264 video pipeline demo (P6).
//!
//! Demonstrates the end-to-end video path that exists today on the public
//! azul surface, in order:
//!   1. `VideoStartupCheck::run()` — probe `VK_KHR_video_decode_h264` readiness
//!      on this GPU (the hardware-decode capability check).
//!   2. `http_get(url)` — fetch the Big Buck Bunny H.264 MP4 over HTTPS.
//!   3. `demux_mp4_h264(bytes)` — demux the MP4 into an H.264 Annex-B stream
//!      (reports geometry + access-unit count + SPS/PPS).
//!   4. feed each access unit to `VideoDecoder` — which TODAY is a stub that
//!      counts units and yields no frames; the Vulkan-Video decode backend is
//!      the next step. So this shows the pipeline + readiness rather than
//!      decoded frames, with the test-pattern `VideoWidget` standing in for the
//!      eventual live frames.
//!
//! The C `azul-video` player (the production goal) drives the same calls through
//! the FFI surface and adds the driver-provision msgbox/autofix on top.

use azul::prelude::*;
use azul::widgets::VideoWidget;
use azul::misc::VideoConfig;
use azul::desktop::http::http_get;
use azul::desktop::extra::video_codec::{VideoDecoder, VideoStartupCheck};
use azul::desktop::extra::video_codec::demux::demux_mp4_h264;

/// Small (~1 MB, 10 s) Big Buck Bunny H.264/MP4 clip — fast to fetch + demux.
const BBB_URL: &str =
    "https://test-videos.co.uk/vids/bigbuckbunny/mp4/h264/360/Big_Buck_Bunny_360_10s_1MB.mp4";

struct VideoAppState {
    status: Vec<String>,
}

/// Run the fetch → demux → probe → feed pipeline once at startup, returning a
/// human-readable line per stage for the on-screen status panel.
fn run_pipeline() -> Vec<String> {
    let mut out = Vec::new();

    // 1. Hardware-decode capability probe.
    let check = VideoStartupCheck::run();
    out.push(format!(
        "VK hardware H.264 decode: {} — {}",
        if check.hw_decode_ready { "READY" } else { "not available" },
        check.summary.as_str(),
    ));

    // 2. Fetch the clip over HTTP.
    match http_get(BBB_URL) {
        Ok(resp) => {
            let bytes = resp.body.as_ref();
            out.push(format!(
                "HTTP GET -> {} bytes (status {})",
                bytes.len(),
                resp.status_code,
            ));

            // 3. Demux MP4 -> H.264 Annex-B.
            match demux_mp4_h264(bytes) {
                Ok(d) => {
                    out.push(format!(
                        "Demuxed H.264: {}x{} @ {:.1} fps · {} access units · SPS {} B · PPS {} B",
                        d.width,
                        d.height,
                        d.fps,
                        d.chunks.len(),
                        d.sps.len(),
                        d.pps.len(),
                    ));

                    // 4. Open the decoder. The decode backend is a stub today
                    //    (yields no frames); the demux + AU count above is the
                    //    verifiable part. The C player feeds each AU via the FFI
                    //    VideoDecoder; real frames await the VK Video backend.
                    let dec = VideoDecoder::open(false);
                    out.push(format!(
                        "VideoDecoder opened={} · {} access units ready to feed (decode is a stub \
                         → real frames await the VK Video backend)",
                        dec.is_open(),
                        d.chunks.len(),
                    ));
                }
                Err(e) => out.push(format!("Demux failed: {}", e)),
            }
        }
        Err(e) => out.push(format!("HTTP fetch failed: {:?}", e)),
    }

    out
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let lines = match data.downcast_ref::<VideoAppState>() {
        Some(s) => s.status.clone(),
        None => Vec::new(),
    };

    let mut root = Dom::create_body().with_css(
        "display: flex; flex-direction: column; padding: 16px; background: #0e0e14; \
         font-family: sans-serif; color: #e6e6f0;",
    );
    root = root.with_child(
        Dom::create_text("AzVideo — H.264 pipeline (Big Buck Bunny)")
            .with_css("font-size: 22px; margin-bottom: 10px;"),
    );

    for line in &lines {
        root = root.with_child(
            Dom::create_text(line.as_str())
                .with_css("font-size: 13px; color: #b8c0d0; margin-bottom: 5px;"),
        );
    }

    root = root.with_child(
        Dom::create_text("test pattern (decoder not yet wired):")
            .with_css("font-size: 12px; color: #6a7080; margin: 10px 0 5px 0;"),
    );
    root = root.with_child(
        VideoWidget::create(VideoConfig::default())
            .dom()
            .with_css(
                "width: 480px; height: 270px; border: 2px solid #2a2a3a; \
                 border-radius: 8px; overflow: hidden;",
            ),
    );

    root
}

fn main() {
    let status = run_pipeline();
    for line in &status {
        eprintln!("[azvideo] {}", line);
    }

    let data = RefAny::new(VideoAppState { status });
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
