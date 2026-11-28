//! What the MediaCodec backend decides without a device (`mediacodec_plan_test.rs`): its error
//! type, what a status code or a codec name means, the limits an encoder is configured within, and
//! the waits and caps every loop of the backend obeys. Pure Rust, so a Linux runner tests it.

use core::fmt;

/// The smallest picture side an encoder is opened for: below it hardware encoders refuse, and a
/// macroblock is 16 pixels.
pub(super) const MIN_SIDE: u32 = 16;
/// The largest: hardware H.264 encoders stop at 4096 pixels (level 5.1 is 4096x2304).
pub(super) const MAX_SIDE: u32 = 4096;
/// The largest picture side a decoder is configured for.
pub(super) const MAX_DECODE_SIDE: u32 = 8192;
/// The lowest bitrate an encoder is asked for (kbit/s), as VideoToolbox's floor.
pub(super) const MIN_KBPS: u32 = 64;
/// The highest (kbit/s): a call, a screen share and a clip's export are all far below it.
pub(super) const MAX_KBPS: u32 = 50_000;
/// The frame rate an encoder is configured for: rate control spends bits per frame by it.
pub(super) const DEFAULT_FPS: u32 = 30;
/// Seconds between the key frames an encoder makes on its own; an app asks for more on demand.
pub(super) const DEFAULT_I_FRAME_INTERVAL_S: u32 = 2;

/// How long one wait for an input buffer lasts (microseconds).
pub(super) const INPUT_WAIT_US: i64 = 10_000;
/// How long a picture waits for an input buffer in all before it is dropped (milliseconds): a
/// realtime codec that does not take input for a quarter of a second is stalled.
pub(super) const INPUT_STALL_MS: u64 = 250;
/// How long a call waits for the first output after it queued an input (microseconds): a
/// software codec answers within it, a hardware one often does, and a call never waits more.
pub(super) const OUTPUT_WAIT_US: i64 = 10_000;
/// How long a drain that waits for the end of a stream goes without any output before it ends
/// (milliseconds).
pub(super) const DRAIN_IDLE_MS: u64 = 200;
/// How long a drain takes in all at most (milliseconds).
pub(super) const DRAIN_TOTAL_MS: u64 = 2_000;
/// How many output buffers one drain takes before it gives the thread back.
pub(super) const MAX_DRAIN_STEPS: usize = 256;
/// How many failures in a row a codec is made again after; then the backend stops trying until
/// the app opens a new one.
pub(super) const MAX_CONSECUTIVE_FAILURES: u32 = 5;
/// The most bytes one access unit may have: a larger one is refused (a malformed stream must not
/// become a buffer of that size).
pub(super) const MAX_ACCESS_UNIT_BYTES: usize = 8 * 1024 * 1024;
/// How many distinct log lines a codec says: the same line twice is said once.
const MAX_SAID: usize = 16;

/// What went wrong in the backend. Every codec call answers with one of these; none panics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum MediaCodecError {
    /// `libmediandk.so`, or a function of it the backend needs, is not on this device.
    Library(String),
    /// The system has no H.264 codec of this kind (`AMediaCodec_create*ByType` gave none).
    NoCodec { encoder: bool },
    /// An NDK call answered a `media_status_t` that is not `AMEDIA_OK`.
    Status { call: &'static str, code: i32 },
    /// A picture, a stream or a format this backend does not take.
    Unsupported(String),
    /// A wait ran out.
    Timeout(&'static str),
}

impl fmt::Display for MediaCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MediaCodecError::Library(why) => write!(f, "{why}"),
            MediaCodecError::NoCodec { encoder } => write!(
                f,
                "this device has no H.264 {} (video/avc)",
                if *encoder { "encoder" } else { "decoder" }
            ),
            MediaCodecError::Status { call, code } => {
                write!(f, "{call} answered {code} ({})", describe_status(*code))
            }
            MediaCodecError::Unsupported(what) => write!(f, "{what}"),
            MediaCodecError::Timeout(what) => write!(f, "timed out waiting for {what}"),
        }
    }
}

impl std::error::Error for MediaCodecError {}

impl MediaCodecError {
    /// Whether the codec has to be made again: the system took it away, or it is in a state no
    /// call leaves. A refused picture or a wait that ran out is not that.
    pub(super) const fn codec_is_lost(&self) -> bool {
        matches!(self, MediaCodecError::Status { .. } | MediaCodecError::NoCodec { .. })
    }
}

/// What a `media_status_t` of the NDK says.
pub(super) const fn describe_status(code: i32) -> &'static str {
    match code {
        0 => "ok",
        1100 => "insufficient resources: another app holds the codec",
        1101 => "the system reclaimed the codec",
        -10000 => "unknown error",
        -10001 => "malformed input",
        -10002 => "unsupported",
        -10003 => "invalid object",
        -10004 => "invalid parameter",
        -10005 => "invalid operation: the codec is in the wrong state",
        -10006 => "end of stream",
        -10007 => "I/O error",
        -10008 => "would block",
        _ => "a status this backend does not know",
    }
}

/// Whether a codec of this name runs on the CPU. Android's own rule (`MediaCodecInfo`'s
/// `isSoftwareOnly`): the codecs Google ships (`c2.android.*`, `c2.google.*`, `OMX.google.*`) are
/// software, and so is a name that is neither a Codec2 nor an OMX one; the vendors' software
/// codecs say so in the name (`OMX.SEC.avc.sw.dec`, `...swdec`).
pub(super) fn is_software_codec(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("arc.") {
        return false;
    }
    lower.starts_with("omx.google.")
        || lower.starts_with("c2.android.")
        || lower.starts_with("c2.google.")
        || (!lower.starts_with("omx.") && !lower.starts_with("c2."))
        || lower.contains(".sw.")
        || lower.ends_with(".sw")
        || lower.contains("swdec")
        || lower.contains("swenc")
}

/// How the engine names itself in a capability and a log line: the codec's name and whether it is
/// hardware or software. Below Android 9 the NDK cannot say the name (`AMediaCodec_getName` is
/// API 28), and then neither whether it is hardware.
pub(super) fn engine_label(name: Option<&str>) -> String {
    match name {
        Some(name) => format!(
            "MediaCodec ({name}, {})",
            if is_software_codec(name) {
                "software"
            } else {
                "hardware"
            }
        ),
        None => String::from("MediaCodec (the codec's name needs Android 9)"),
    }
}

/// Whether the codec named `name` is hardware; `None` when the name is not known.
pub(super) fn is_hardware(name: Option<&str>) -> Option<bool> {
    name.map(|n| !is_software_codec(n))
}

/// The API level in the text of `ro.build.version.sdk`.
pub(super) fn parse_api_level(text: &str) -> Option<u32> {
    text.trim()
        .parse::<u32>()
        .ok()
        .filter(|level| (1..1000).contains(level))
}

/// What an encoder is opened with: the size, the rate, the frame rate and the key frame interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub bitrate_kbps: u32,
    pub fps: u32,
    pub i_frame_interval_s: u32,
}

/// `kbps` within [`MIN_KBPS`], [`MAX_KBPS`], and whether it had to be moved.
pub(super) const fn clamp_bitrate(kbps: u32) -> (u32, bool) {
    if kbps < MIN_KBPS {
        (MIN_KBPS, true)
    } else if kbps > MAX_KBPS {
        (MAX_KBPS, true)
    } else {
        (kbps, false)
    }
}

/// Whether a picture of this size can go to an encoder: even sides (4:2:0), within the limits.
pub(super) fn check_size(width: u32, height: u32) -> Result<(), MediaCodecError> {
    if width % 2 != 0 || height % 2 != 0 {
        return Err(MediaCodecError::Unsupported(format!(
            "a {width}x{height} picture has an odd side: H.264 4:2:0 needs even ones"
        )));
    }
    if !(MIN_SIDE..=MAX_SIDE).contains(&width) || !(MIN_SIDE..=MAX_SIDE).contains(&height) {
        return Err(MediaCodecError::Unsupported(format!(
            "a {width}x{height} picture is outside what the H.264 encoders take \
             ({MIN_SIDE}..={MAX_SIDE} pixels a side)"
        )));
    }
    Ok(())
}

/// Whether a stream of this size can go to a decoder: from one macroblock up to 8192 pixels a
/// side (level 6.2), so a bitstream error cannot become a codec configured for a huge picture.
pub(super) fn check_decode_size(width: u32, height: u32) -> Result<(), MediaCodecError> {
    if !(MIN_SIDE..=MAX_DECODE_SIDE).contains(&width) || !(MIN_SIDE..=MAX_DECODE_SIDE).contains(&height) {
        return Err(MediaCodecError::Unsupported(format!(
            "a {width}x{height} stream is outside what the decoder is configured for \
             ({MIN_SIDE}..={MAX_DECODE_SIDE} pixels a side)"
        )));
    }
    Ok(())
}

impl EncoderConfig {
    /// The configuration of a realtime encoder for `width` x `height` at `bitrate_kbps`: the
    /// bitrate moved into the limits, the size refused when no encoder takes it.
    pub(super) fn realtime(
        width: u32,
        height: u32,
        bitrate_kbps: u32,
    ) -> Result<Self, MediaCodecError> {
        check_size(width, height)?;
        Ok(EncoderConfig {
            width,
            height,
            bitrate_kbps: clamp_bitrate(bitrate_kbps).0,
            fps: DEFAULT_FPS,
            i_frame_interval_s: DEFAULT_I_FRAME_INTERVAL_S,
        })
    }

    /// The bitrate in bits a second, as `KEY_BIT_RATE` has it.
    pub(super) fn bitrate_bps(&self) -> i32 {
        i32::try_from(self.bitrate_kbps.saturating_mul(1000)).unwrap_or(i32::MAX)
    }
}

/// The input colour formats an encoder is tried with, in the order of preference: NV12
/// (`COLOR_FormatYUV420SemiPlanar`), I420 (`COLOR_FormatYUV420Planar`), then the flexible format
/// (read as I420, the layout most codecs give it).
pub(super) const ENCODER_INPUT_FORMATS: [i32; 3] = [21, 19, 0x7F42_0888];

/// What to do after a codec call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Recovery {
    /// Make the codec again: the next picture goes to a new one.
    Remake,
    /// The codec failed too often in a row: stay closed, answer nothing.
    GiveUp,
}

/// The recovery after the `consecutive`-th failure in a row (the first is 1).
pub(super) const fn recovery_after(consecutive: u32) -> Recovery {
    if consecutive > MAX_CONSECUTIVE_FAILURES {
        Recovery::GiveUp
    } else {
        Recovery::Remake
    }
}

/// Remembers the log lines that were said, so a codec that fails on every frame says it once.
#[derive(Debug, Default)]
pub(super) struct Said {
    lines: Vec<String>,
}

impl Said {
    /// Whether `line` is new, and so is to be said now. After [`MAX_SAID`] different lines
    /// nothing new is said.
    pub(super) fn is_new(&mut self, line: &str) -> bool {
        if self.lines.iter().any(|said| said == line) || self.lines.len() >= MAX_SAID {
            return false;
        }
        self.lines.push(line.to_string());
        true
    }
}

#[cfg(test)]
#[path = "mediacodec_plan_test.rs"]
mod mediacodec_plan_test;
