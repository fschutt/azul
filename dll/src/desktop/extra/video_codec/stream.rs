//! Streaming H.264 decode worker for [`VideoWidget`](azul_layout::widgets::video::VideoWidget).
//!
//! Runs the Vulkan-Video decode on a background framework `Thread` (OFF the main
//! thread), exactly like the map widget's `tile_fetch_worker`. Frames are decoded
//! incrementally — there is NO up-front decode — and presented by WALL-CLOCK, so
//! late frames are dropped and the window opens immediately while playback speed
//! stays independent of decode/render speed. Pass [`video_decode_worker`] to
//! [`VideoWidget::dom_with_decoder`](azul_layout::widgets::video::VideoWidget::dom_with_decoder)
//! wrapped in a `ThreadCallback`, exactly like `MapWidget::dom_with_fetch`.

use azul_core::refany::RefAny;
use azul_core::task::ThreadReceiver;
use azul_layout::thread::{
    ThreadCallback, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg, WriteBackCallback,
};
use azul_layout::widgets::video::video_writeback;

/// FFI entry point the `VideoWidget::dom()` shim calls — wires the off-main
/// streaming decode worker, mirroring `map_widget_dom`. The worker lives here in
/// `azul-dll` (it pulls the gpu-video / mp4 dep tree kept out of `azul-layout`),
/// so `VideoWidget::dom()` in layout can only produce a placeholder; the real
/// streaming decode is injected here via the layout-internal `dom_with_decoder`
/// plumbing. The decode itself is `video-native`-gated inside the worker; this
/// wrapper (and the worker fn) are always present so the `unified` path resolves
/// in every `cabi_internal` build.
pub fn video_widget_dom(widget: azul_layout::widgets::video::VideoWidget) -> azul_core::dom::Dom {
    widget.dom_with_decoder(ThreadCallback {
        cb: video_decode_worker,
        ctx: azul_core::refany::OptionRefAny::None,
    })
}

/// Background decode worker. `init` is a `RefAny` holding the source:
/// - a `String` — a URL, fetched via an HTTP **range request**, or
/// - a `Vec<u8>` — raw MP4 bytes (e.g. a bundled/local sample).
///
/// It decodes the clip incrementally on this thread and streams frames to the
/// widget's `<img>` via `WriteBack` → `video_writeback` → `present_frame`,
/// paced by wall-clock with frame dropping.
pub extern "C" fn video_decode_worker(init: RefAny, sender: ThreadSender, recv: ThreadReceiver) {
    #[cfg(all(
        feature = "video-native",
        target_arch = "x86_64",
        any(target_os = "linux", target_os = "windows")
    ))]
    decode_stream(init, sender, recv);

    #[cfg(not(all(
        feature = "video-native",
        target_arch = "x86_64",
        any(target_os = "linux", target_os = "windows")
    )))]
    {
        // No Vulkan-Video decoder on this target: the widget keeps its placeholder.
        let _ = (init, sender, recv);
    }
}

#[cfg(all(
    feature = "video-native",
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
))]
fn decode_stream(mut init: RefAny, mut sender: ThreadSender, mut recv: ThreadReceiver) {
    use azul_core::task::{OptionThreadSendMsg, ThreadSendMsg};
    use azul_core::video::{OptionVideoFrame, VideoFrame};
    use azul_css::U8Vec;
    use std::time::{Duration, Instant};

    // Target output size (physical px) the widget last asked for via NodeResized.
    // While `None` the worker emits frames at the stream's native size; once set,
    // each presented frame is scaled to it OFF this thread, so the UI does no
    // interpolation and no relayout — just an image swap.
    let mut target_size: Option<(u32, u32)> = None;

    let log = std::env::var("AZ_VIDEO_FRAMELOG").is_ok();

    // 1. The thread-init is the `VideoConfig`; match its typed source → MP4 bytes
    //    (URL via range request / local file / in-memory bytes). No RefAny downcast
    //    ambiguity — the source is strongly typed.
    use azul_core::video::VideoSource;
    let config = match init.downcast_ref::<azul_core::video::VideoConfig>() {
        Some(c) => c.clone(),
        None => {
            if log {
                eprintln!("[vstream] init is not a VideoConfig");
            }
            return;
        }
    };
    // The widget can swap the source live (merge → ThreadSendMsg::Custom(VideoSource));
    // re-init the decode for the new source. `target_size` persists across sources.
    let mut current_source = config.source.clone();
    'session: loop {
        let bytes: Vec<u8> = match &current_source {
            VideoSource::Url(u) => {
                if log {
                    eprintln!("[vstream] fetching (Range: bytes=0-) {}", u.as_str());
                }
                match fetch_ranged(u.as_str()) {
                    Some(b) => b,
                    None => {
                        if log {
                            eprintln!("[vstream] fetch FAILED");
                        }
                        return;
                    }
                }
            }
            VideoSource::File(p) => {
                if log {
                    eprintln!("[vstream] reading file {}", p.as_str());
                }
                match std::fs::read(p.as_str()) {
                    Ok(b) => b,
                    Err(e) => {
                        if log {
                            eprintln!("[vstream] file read FAILED: {}", e);
                        }
                        return;
                    }
                }
            }
            VideoSource::Bytes(b) => b.as_ref().to_vec(),
        };
        if log {
            eprintln!("[vstream] got {} bytes", bytes.len());
        }

        // 2. Demux to H.264 Annex-B access units.
        let demuxed = match super::demux::demux_mp4_h264(&bytes) {
            Ok(d) => d,
            Err(e) => {
                if log {
                    eprintln!("[vstream] demux FAILED: {}", e);
                }
                return;
            }
        };
        let total = demuxed.chunks.len();
        if total == 0 {
            if log {
                eprintln!("[vstream] demux produced 0 chunks");
            }
            return;
        }
        let fps = if demuxed.fps > 0.0 { demuxed.fps } else { 30.0 };
        if log {
            eprintln!("[vstream] demuxed {} chunks @ {:.1} fps", total, fps);
        }

        // 3. Open the VK decoder and stream-decode, presenting by wall-clock.
        let decoder = super::VideoDecoder::open(false /* h264 */);
        let mut decoded: Vec<VideoFrame> = Vec::new();
        let mut chunk_idx = 0usize;
        let mut last_idx = usize::MAX;
        // Wall-clock origin for paced presentation. Seeking just moves this origin:
        // `start = now - ts` ⇒ elapsed = ts ⇒ present idx jumps to ts*fps (the frames
        // are already decoded, so no re-decode is needed).
        let mut start = Instant::now();

        loop {
            // Drain control messages from the main thread (non-blocking): a NodeResized
            // gives us a new target size (re-present the current frame scaled to it); a
            // terminate stops the worker.
            loop {
                match recv.recv() {
                    OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread) => return,
                    OptionThreadSendMsg::Some(ThreadSendMsg::Custom(mut r)) => {
                        if let Some(sz) = r.downcast_ref::<(u32, u32)>().map(|t| *t) {
                            if sz.0 > 0 && sz.1 > 0 && target_size != Some(sz) {
                                target_size = Some(sz);
                                last_idx = usize::MAX; // force a re-present at the new size
                                if log {
                                    eprintln!("[vstream] resize → target {}x{}", sz.0, sz.1);
                                }
                            }
                        } else if let Some(ts) = r.downcast_ref::<f32>().map(|t| *t) {
                            // Seek: reposition the wall-clock origin so the next present
                            // jumps to ts*fps.
                            let ts = ts.max(0.0);
                            start = Instant::now()
                                .checked_sub(Duration::from_secs_f32(ts))
                                .unwrap_or_else(Instant::now);
                            last_idx = usize::MAX;
                            if log {
                                eprintln!("[vstream] seek → {:.2}s", ts);
                            }
                        } else if let Some(src) = r.downcast_ref::<VideoSource>().map(|s| s.clone())
                        {
                            // Input-source change: restart the decode session with the new
                            // source (re-fetch/demux/decode). The `'session` loop re-resolves.
                            if log {
                                eprintln!("[vstream] source change → re-init");
                            }
                            current_source = src;
                            continue 'session;
                        }
                    }
                    OptionThreadSendMsg::Some(_) => {}
                    OptionThreadSendMsg::None => break,
                }
            }

            // Decode one access unit per iteration (drain every frame it yields),
            // flushing the reorder buffer after the final chunk.
            if chunk_idx < total {
                let mut f =
                    decoder.decode(U8Vec::from_vec(demuxed.chunks[chunk_idx].annexb.clone()));
                while let OptionVideoFrame::Some(frame) = f {
                    decoded.push(frame);
                    f = decoder.next_frame();
                }
                chunk_idx += 1;
                if chunk_idx == total {
                    let mut f = decoder.flush();
                    while let OptionVideoFrame::Some(frame) = f {
                        decoded.push(frame);
                        f = decoder.next_frame();
                    }
                }
            }

            if !decoded.is_empty() {
                let target = (start.elapsed().as_secs_f32() * fps) as usize;
                // Still decoding → clamp to the newest decoded frame (decode-bound).
                // Fully decoded → loop the clip by wall-clock, dropping late frames.
                let idx = if chunk_idx < total {
                    target.min(decoded.len() - 1)
                } else {
                    target % decoded.len()
                };
                if idx != last_idx {
                    if log && idx % 30 == 0 {
                        eprintln!(
                            "[vstream] t={:.2}s decoded={}/{} present_frame={}",
                            start.elapsed().as_secs_f32(),
                            decoded.len(),
                            total,
                            idx
                        );
                    }
                    last_idx = idx;
                    // Scale to the widget's requested size OFF this thread so the UI does
                    // no interpolation (the `<img>` shows it 1:1).
                    let frame = match target_size {
                        Some((tw, th)) if (tw, th) != (decoded[idx].width, decoded[idx].height) => {
                            scale_frame_bilinear(&decoded[idx], tw, th)
                        }
                        _ => decoded[idx].clone(),
                    };
                    let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
                        WriteBackCallback::new(video_writeback),
                        RefAny::new(frame),
                    )));
                    if !sent {
                        return; // widget gone → stop the worker entirely
                    }
                }
            }

            // Once fully decoded, pace so we don't busy-spin; while still decoding,
            // loop fast (the decode itself is the work) to catch up to real time.
            if chunk_idx >= total {
                std::thread::sleep(Duration::from_millis(8));
            }
        }
    }
}

/// Bilinear-resize a tightly-packed RGBA8 `VideoFrame` to `tw`×`th`, on the decode
/// thread, so the UI renderer doesn't have to interpolate (the `<img>` shows it 1:1).
#[cfg(all(
    feature = "video-native",
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
))]
fn scale_frame_bilinear(
    src: &azul_core::video::VideoFrame,
    tw: u32,
    th: u32,
) -> azul_core::video::VideoFrame {
    use azul_css::U8Vec;
    let (sw, sh) = (src.width, src.height);
    if sw == 0 || sh == 0 || tw == 0 || th == 0 {
        return src.clone();
    }
    let s = src.bytes.as_ref();
    if s.len() < (sw as usize) * (sh as usize) * 4 {
        return src.clone();
    }
    let mut out = vec![0u8; (tw as usize) * (th as usize) * 4];
    let rx = sw as f32 / tw as f32;
    let ry = sh as f32 / th as f32;
    for ty in 0..th {
        let fy = ((ty as f32 + 0.5) * ry - 0.5).max(0.0);
        let y0 = fy.floor() as u32;
        let y1 = (y0 + 1).min(sh - 1);
        let wy = fy - y0 as f32;
        for tx in 0..tw {
            let fx = ((tx as f32 + 0.5) * rx - 0.5).max(0.0);
            let x0 = fx.floor() as u32;
            let x1 = (x0 + 1).min(sw - 1);
            let wx = fx - x0 as f32;
            let o = ((ty * tw + tx) * 4) as usize;
            for c in 0..4 {
                let px = |x: u32, y: u32| s[(((y * sw + x) * 4) as usize) + c] as f32;
                let top = px(x0, y0) * (1.0 - wx) + px(x1, y0) * wx;
                let bot = px(x0, y1) * (1.0 - wx) + px(x1, y1) * wx;
                out[o + c] = (top * (1.0 - wy) + bot * wy).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    azul_core::video::VideoFrame {
        width: tw,
        height: th,
        bytes: U8Vec::from_vec(out),
    }
}

/// Fetch `url` via an HTTP **range request** (`Range: bytes=0-`). BBB is small so
/// a single open-ended range fetches the whole clip in one 206 response; the point
/// is that loading goes through a real range request (progressive byte-range
/// streaming is a future refinement).
#[cfg(all(
    feature = "video-native",
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
))]
fn fetch_ranged(url: &str) -> Option<Vec<u8>> {
    use azul_css::AzString;
    use azul_layout::http::{HttpRequestConfig, ResultU8VecHttpError};
    let cfg = HttpRequestConfig::new().with_header("Range", "bytes=0-");
    match cfg.download_bytes(AzString::from(url.to_string())) {
        ResultU8VecHttpError::Ok(b) => Some(b.as_slice().to_vec()),
        ResultU8VecHttpError::Err(_) => None,
    }
}
