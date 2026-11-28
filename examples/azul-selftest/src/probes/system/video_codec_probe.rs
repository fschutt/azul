//! The video codec row: encode a synthetic moving box with `VideoEncoder`, decode it back with `VideoDecoder`,
//! and check the picture. The same path AzMeet takes to send and to see a camera, through the public API.
//!
//! The rule of the report: a device with no H.264 codec is UNAVAILABLE (not a failure); a capability that says
//! it is available and whose encoder or decoder then does not give back what went in IS one. What is checked:
//!
//! * the encoder opens and gives one packet per frame, the first a key frame with its SPS and PPS in front, the
//!   frame asked for as a key frame (`FORCED_KEY`) a key frame again, with the parameter sets in front;
//! * the decoder gives back every frame, at the size that went in;
//! * the picture: the PSNR of the luma of each decoded frame against the luma of the frame that was encoded
//!   (`MIN_PSNR_Y_MEAN` on average, `MIN_PSNR_Y_EACH` for the worst one) and of the RGB (`MIN_PSNR_RGB_MEAN`).
//!   35 dB is where flat synthetic content stops looking compressed at 0.4 bit a pixel, which any conforming
//!   H.264 encoder, hardware or software, beats by 5 dB or more; 28 dB on RGB leaves room for the chroma
//!   subsampling at the box's edges (about 40 dB for this clip) and still fails a swapped U and V (a blue
//!   box where the yellow one is, about 15 dB) or a wrong matrix.
//!
//! The numbers go into the verdict: frames, bytes, PSNR, the latency to the first packet and the first frame,
//! the milliseconds a frame takes, and whether the encoder runs in hardware.

use std::{
    thread::sleep,
    time::{Duration, Instant},
};

use azul::{
    image::{RawImageFormat, VideoEncodeCheck},
    vec::U8Vec,
    video::{VideoDecoder, VideoEncoder, VideoFrame},
    window::PlatformCapability,
};

use super::super::{describe_capability, Ctx, Step};
use crate::report::Verdict;

/// The size of the clip: a multiple of the macroblock, so every encoder takes it as it is.
pub const WIDTH: u32 = 320;
pub const HEIGHT: u32 = 240;
/// Frames of the clip, one every [`FRAME_US`] (30 fps), at [`KBPS`].
pub const FRAMES: u32 = 30;
pub const FRAME_US: u64 = 33_333;
pub const KBPS: u32 = 800;
/// The frame that is asked for as a key frame again, in the middle of the clip.
pub const FORCED_KEY: u32 = 15;
/// The PSNR a decoded frame must reach (dB): the luma on average, the luma of the worst frame, the RGB on
/// average.
pub const MIN_PSNR_Y_MEAN: f64 = 35.0;
pub const MIN_PSNR_Y_EACH: f64 = 30.0;
pub const MIN_PSNR_RGB_MEAN: f64 = 28.0;
/// The most a step of the round trip may take.
const WAIT_LIMIT: Duration = Duration::from_secs(15);
/// How long the first decoded frame is waited for before the rest of the clip is fed (a decoder with a
/// pipeline gives its first picture only after a few chunks).
const FIRST_FRAME_WAIT: Duration = Duration::from_millis(500);

/// The row.
pub fn video_codec(_ctx: &mut Ctx<'_>) -> Step {
    let verdict = verdict();
    // One short line of its own: logcat cuts the report line at about 4 KB, which is before this row.
    eprintln!("AZ_SELFTEST_ROW system.video_codec {} {}", verdict.code(), verdict.detail());
    Step::Done(verdict)
}

fn verdict() -> Verdict {
    let (available, backend) = describe_capability(&PlatformCapability::video_codec());
    if !available {
        return Verdict::Unavailable(format!("the video codec capability is not available: {backend}"));
    }
    let encode = VideoEncodeCheck::run();
    if !encode.hw_encode_ready {
        return Verdict::Unavailable(format!(
            "H.264 decode is available via {backend}, but no encoder is ({}): there is no stream to decode back",
            encode.summary.as_str()
        ));
    }
    let engine = encode
        .detail
        .as_str()
        .lines()
        .next()
        .unwrap_or("")
        .trim_start_matches("VideoEncoder in this build: ")
        .to_string();
    match round_trip(&backend, &engine) {
        Ok(measured) => match judge(&measured) {
            Ok(numbers) => Verdict::Pass(numbers),
            Err(why) => Verdict::Fail(why),
        },
        Err(why) => Verdict::Fail(why),
    }
}

/// Everything the round trip measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Measured {
    /// The decoder's capability text and the encoder's engine line (`encodes with MediaCodec (c2.android.avc.encoder, software)`).
    pub decoder: String,
    pub encoder: String,
    pub hardware: bool,
    pub frames_in: u32,
    pub frames_out: u32,
    /// Decoded frames that are not `WIDTH x HEIGHT` RGBA.
    pub wrong_size: u32,
    pub packets: u32,
    pub bytes: usize,
    /// Packets with an IDR slice, and of them the ones that do not begin with an SPS followed by a PPS.
    pub key_packets: u32,
    pub key_packets_without_sets: u32,
    pub first_packet_is_key_with_sets: bool,
    pub forced_key_is_key_with_sets: bool,
    pub psnr_y_mean: f64,
    pub psnr_y_min: f64,
    pub psnr_rgb_mean: f64,
    pub psnr_rgb_min: f64,
    pub encode_first_ms: f64,
    pub encode_ms_per_frame: f64,
    pub decode_first_ms: f64,
    pub decode_ms_per_frame: f64,
}

fn ms_since(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// Encodes the clip, decodes it back and measures. `Err` says why the round trip could not run to its end.
fn round_trip(backend: &str, engine: &str) -> Result<Measured, String> {
    let originals: Vec<Vec<u8>> = (0..FRAMES).map(box_frame_rgba).collect();

    let mut encoder = VideoEncoder::open(WIDTH, HEIGHT, false, KBPS);
    if !encoder.is_open() {
        return Err(format!(
            "the capability says available ({backend}) and VideoEncoder::open({WIDTH}x{HEIGHT}, {KBPS} kbit/s) handed out no encoder"
        ));
    }
    let hardware = encoder.is_hardware();
    let mut packets: Vec<Vec<u8>> = Vec::new();
    let mut first_packet_ms = None;
    let started = Instant::now();
    for (index, rgba) in originals.iter().enumerate() {
        let index = index as u32;
        let frame = VideoFrame {
            width: WIDTH,
            height: HEIGHT,
            bytes: U8Vec::from(rgba.clone()),
            format: RawImageFormat::RGBA8,
        };
        let key = index == 0 || index == FORCED_KEY;
        if !encoder.encode_at(frame, u64::from(index) * FRAME_US, key) {
            return Err(format!("VideoEncoder::encode_at refused frame {index} of {FRAMES}"));
        }
        take_packets(&mut encoder, &mut packets, &mut first_packet_ms, started);
        if started.elapsed() > WAIT_LIMIT {
            return Err(format!("the encoder took more than {} s for {index} frames", WAIT_LIMIT.as_secs()));
        }
    }
    encoder.flush();
    take_packets(&mut encoder, &mut packets, &mut first_packet_ms, started);
    let encode_total_ms = ms_since(started);
    encoder.close();
    if packets.is_empty() {
        return Err(format!(
            "the encoder opened ({engine}) and gave no packet for {FRAMES} frames in {encode_total_ms:.0} ms"
        ));
    }

    let mut decoder = VideoDecoder::open(false);
    if !decoder.is_open() {
        return Err(format!("the capability says available ({backend}) and VideoDecoder::open handed out no decoder"));
    }
    decoder.set_output_format(RawImageFormat::RGBA8);
    let mut frames: Vec<VideoFrame> = Vec::new();
    let mut first_frame_ms = None;
    let started = Instant::now();
    for (index, packet) in packets.iter().enumerate() {
        decoder.decode(U8Vec::from(packet.clone()));
        if index == 0 {
            // The first picture's latency: wait for it a little, as a viewer of a call does.
            while first_frame_ms.is_none() && started.elapsed() < FIRST_FRAME_WAIT {
                match decoder.next_frame().into_option() {
                    Some(frame) => {
                        first_frame_ms = Some(ms_since(started));
                        frames.push(frame);
                    }
                    None => sleep(Duration::from_millis(1)),
                }
            }
        }
        while let Some(frame) = decoder.next_frame().into_option() {
            first_frame_ms.get_or_insert_with(|| ms_since(started));
            frames.push(frame);
        }
        if started.elapsed() > WAIT_LIMIT {
            return Err(format!("the decoder took more than {} s for {index} chunks", WAIT_LIMIT.as_secs()));
        }
    }
    if let Some(frame) = decoder.flush().into_option() {
        first_frame_ms.get_or_insert_with(|| ms_since(started));
        frames.push(frame);
    }
    while let Some(frame) = decoder.recv_frame().into_option() {
        frames.push(frame);
    }
    let decode_total_ms = ms_since(started);
    decoder.close();

    // The picture.
    let mut wrong_size = 0u32;
    let (mut y_sum, mut y_min, mut rgb_sum, mut rgb_min, mut compared) = (0.0f64, f64::MAX, 0.0f64, f64::MAX, 0u32);
    for (frame, original) in frames.iter().zip(&originals) {
        let rgba = frame.bytes.as_slice();
        if frame.width != WIDTH || frame.height != HEIGHT || rgba.len() != original.len() {
            wrong_size += 1;
            continue;
        }
        let y = psnr(&luma(original), &luma(rgba));
        let rgb = psnr_rgb(original, rgba);
        y_sum += y;
        y_min = y_min.min(y);
        rgb_sum += rgb;
        rgb_min = rgb_min.min(rgb);
        compared += 1;
    }
    let mean = |sum: f64| if compared == 0 { 0.0 } else { sum / f64::from(compared) };
    let min = |value: f64| if compared == 0 { 0.0 } else { value };

    // The stream.
    let (mut key_packets, mut key_packets_without_sets) = (0u32, 0u32);
    for packet in &packets {
        if is_key(packet) {
            key_packets += 1;
            if !begins_with_parameter_sets(packet) {
                key_packets_without_sets += 1;
            }
        }
    }
    let key_with_sets = |packet: Option<&Vec<u8>>| packet.is_some_and(|p| is_key(p) && begins_with_parameter_sets(p));
    let counted = f64::from(FRAMES.max(1));
    Ok(Measured {
        decoder: backend.to_string(),
        encoder: engine.to_string(),
        hardware,
        frames_in: FRAMES,
        frames_out: frames.len() as u32,
        wrong_size,
        packets: packets.len() as u32,
        bytes: packets.iter().map(Vec::len).sum(),
        key_packets,
        key_packets_without_sets,
        first_packet_is_key_with_sets: key_with_sets(packets.first()),
        forced_key_is_key_with_sets: key_with_sets(packets.get(FORCED_KEY as usize)),
        psnr_y_mean: mean(y_sum),
        psnr_y_min: min(y_min),
        psnr_rgb_mean: mean(rgb_sum),
        psnr_rgb_min: min(rgb_min),
        encode_first_ms: first_packet_ms.unwrap_or(encode_total_ms),
        encode_ms_per_frame: encode_total_ms / counted,
        decode_first_ms: first_frame_ms.unwrap_or(decode_total_ms),
        decode_ms_per_frame: decode_total_ms / counted,
    })
}

/// Takes the packets the encoder has ready.
fn take_packets(encoder: &mut VideoEncoder, packets: &mut Vec<Vec<u8>>, first_ms: &mut Option<f64>, started: Instant) {
    while let Some(packet) = encoder.recv_packet().into_option() {
        first_ms.get_or_insert_with(|| ms_since(started));
        packets.push(packet.as_slice().to_vec());
    }
}

// ---------------------------------------------------------------------------------------------------------
// What is checked, and what is computed: std only, tested in video_codec_probe_test.rs
// ---------------------------------------------------------------------------------------------------------

/// The verdict of the numbers: `Ok` the numbers for the report, `Err` what was wrong, with the numbers.
pub fn judge(m: &Measured) -> Result<String, String> {
    let kbps = m.bytes as f64 * 8.0 / (f64::from(m.frames_in.max(1)) * FRAME_US as f64 / 1_000_000.0) / 1000.0;
    let numbers = format!(
        "{}x{}, {}/{} frames, {} packets {} bytes ({:.0} of {} kbit/s), key packets {} (forced at {}), psnr_y mean {:.1} min {:.1} dB, psnr_rgb mean {:.1} min {:.1} dB, encode first packet {:.0} ms {:.1} ms/frame, decode first frame {:.0} ms {:.1} ms/frame, encoder hardware={}, decoder {}, encoder {}",
        WIDTH,
        HEIGHT,
        m.frames_out,
        m.frames_in,
        m.packets,
        m.bytes,
        kbps,
        KBPS,
        m.key_packets,
        FORCED_KEY,
        m.psnr_y_mean,
        m.psnr_y_min,
        m.psnr_rgb_mean,
        m.psnr_rgb_min,
        m.encode_first_ms,
        m.encode_ms_per_frame,
        m.decode_first_ms,
        m.decode_ms_per_frame,
        m.hardware,
        m.decoder,
        m.encoder
    );
    let wrong = |why: String| Err(format!("{why} [{numbers}]"));
    if m.frames_out != m.frames_in {
        return wrong(format!("the decoder gave back {} of {} frames", m.frames_out, m.frames_in));
    }
    if m.wrong_size > 0 {
        return wrong(format!("{} decoded frames are not {}x{} RGBA", m.wrong_size, WIDTH, HEIGHT));
    }
    if !m.first_packet_is_key_with_sets {
        return wrong(String::from("the first packet is not a key frame with its SPS and PPS in front"));
    }
    if !m.forced_key_is_key_with_sets {
        return wrong(format!(
            "frame {FORCED_KEY} was asked for as a key frame and its packet is not one with its SPS and PPS in front"
        ));
    }
    if m.key_packets_without_sets > 0 {
        return wrong(format!("{} key frames have no SPS and PPS in front", m.key_packets_without_sets));
    }
    if m.psnr_y_mean < MIN_PSNR_Y_MEAN || m.psnr_y_min < MIN_PSNR_Y_EACH {
        return wrong(format!(
            "the decoded luma is too far from the encoded one (mean {:.1} dB needs {MIN_PSNR_Y_MEAN}, worst {:.1} dB needs {MIN_PSNR_Y_EACH})",
            m.psnr_y_mean, m.psnr_y_min
        ));
    }
    if m.psnr_rgb_mean < MIN_PSNR_RGB_MEAN {
        return wrong(format!(
            "the luma is right and the colour is not (rgb mean {:.1} dB needs {MIN_PSNR_RGB_MEAN}: swapped chroma planes or a wrong matrix)",
            m.psnr_rgb_mean
        ));
    }
    Ok(numbers)
}

/// Frame `index` of the clip as RGBA: a smooth gradient and a yellow box with a dark border that moves 8 pixels
/// to the right a frame.
pub fn box_frame_rgba(index: u32) -> Vec<u8> {
    const BOX: u32 = 48;
    let x0 = (index * 8) % (WIDTH - BOX);
    let y0 = 96;
    let mut out = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let inside = x >= x0 && x < x0 + BOX && y >= y0 && y < y0 + BOX;
            let border = inside && (x < x0 + 2 || x >= x0 + BOX - 2 || y < y0 + 2 || y >= y0 + BOX - 2);
            let pixel = if border {
                [20, 20, 20, 255]
            } else if inside {
                [230, 220, 40, 255]
            } else {
                [(x * 255 / (WIDTH - 1)) as u8, (y * 255 / (HEIGHT - 1)) as u8, 96, 255]
            };
            out.extend_from_slice(&pixel);
        }
    }
    out
}

/// The BT.601 luma (16..=235) of every pixel of an RGBA image.
pub fn luma(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .map(|px| {
            let (r, g, b) = (i32::from(px[0]), i32::from(px[1]), i32::from(px[2]));
            (16 + ((66 * r + 129 * g + 25 * b + 128) >> 8)).clamp(0, 255) as u8
        })
        .collect()
}

/// The PSNR in dB of two equally long byte runs; 99 for identical ones, 0 for runs of different lengths.
pub fn psnr(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let sum: u64 = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| {
            let d = i64::from(x) - i64::from(y);
            (d * d) as u64
        })
        .sum();
    if sum == 0 {
        return 99.0;
    }
    let mse = sum as f64 / a.len() as f64;
    (10.0 * (255.0f64 * 255.0 / mse).log10()).min(99.0)
}

/// The PSNR of the red, green and blue bytes of two RGBA images (the alpha is not compared).
pub fn psnr_rgb(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }
    let strip = |image: &[u8]| -> Vec<u8> {
        image.chunks_exact(4).flat_map(|px| [px[0], px[1], px[2]]).collect()
    };
    psnr(&strip(a), &strip(b))
}

/// The `nal_unit_type` of every NAL unit of an Annex-B buffer, in order.
pub fn nal_types(annexb: &[u8]) -> Vec<u8> {
    let mut types = Vec::new();
    let mut i = 0usize;
    while i + 3 < annexb.len() {
        if annexb[i] == 0 && annexb[i + 1] == 0 && annexb[i + 2] == 1 {
            types.push(annexb[i + 3] & 0x1f);
            i += 3;
        } else {
            i += 1;
        }
    }
    types
}

/// Whether a packet holds an IDR slice.
pub fn is_key(packet: &[u8]) -> bool {
    nal_types(packet).contains(&5)
}

/// Whether a packet begins with an SPS and has a PPS after it, before its slices.
pub fn begins_with_parameter_sets(packet: &[u8]) -> bool {
    let types = nal_types(packet);
    types.first() == Some(&7) && types.get(1) == Some(&8)
}

#[cfg(test)]
#[path = "video_codec_probe_test.rs"]
mod video_codec_probe_test;
