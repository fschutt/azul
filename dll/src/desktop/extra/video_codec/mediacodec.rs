//! H.264 encode and decode through the NDK's MediaCodec (`AMediaCodec` / `AMediaFormat`): the
//! codec of an Android phone, hardware where the SoC has one, Google's software codec where not
//! (the emulator).
//!
//! `libmediandk.so` is opened at run time (`dlopen`, like `audio/aaudio.rs`): no new link-time
//! dependency, and a device without the library says "unavailable" with a reason instead of the
//! whole app failing to load. Every entry point the backend calls is API 21 except two optional
//! ones it checks for: `AMediaCodec_setParameters` (API 26: a key frame on demand, a new bitrate
//! without a restart) and `AMediaCodec_getName` / `getInputFormat` (API 28: the codec's name, so
//! hardware or software, and the stride its input wants). minSdk is 24, so the codecs run in the
//! synchronous mode (dequeue with a timeout) on the codec thread of the handle
//! (`CodecThread`): the asynchronous callbacks are API 28.
//!
//! Decoder: Annex-B access units in (one per input buffer, split from whatever chunk the app
//! hands over), the pictures out as the engine's [`VideoFrame`]: tightly packed NV12 where the
//! app asked for it, RGBA8 (the default) or BGRA8 otherwise. The codec is configured for the size
//! the stream's SPS says; its output buffers are read through the layout the output format gives
//! (stride, slice height, crop, colour format, or the `image-data` of a flexible format), and the
//! format-changed event updates it.
//!
//! Encoder: NV12, RGBA8 or BGRA8 frames in, Annex-B packets out. The SPS and PPS come from the
//! codec's CODEC_CONFIG buffer and are put in front of every key frame; a key frame on demand is
//! `request-sync` (API 26; before that the codec is restarted, whose first frame is one).
//!
//! Nothing here panics on a codec error. A call that fails answers a [`MediaCodecError`]; the
//! engine says it once in logcat (tag `azul-video`), makes the codec again (flush / stop / delete
//! and a new one) up to [`plan::MAX_CONSECUTIVE_FAILURES`] times in a row, and then stays closed:
//! the handle keeps answering nothing, which is what `EncoderHealth` of an app reads as "inert".
//! Every wait has a bound and every queue a cap.

use core::ffi::{c_char, c_int, c_long, c_void, CStr};
use std::{
    borrow::Cow,
    sync::Arc,
    time::{Duration, Instant},
};

use azul_core::{
    resources::{nv12_to_rgba, RawImageFormat},
    video::VideoFrame,
};
use azul_css::U8Vec;

use super::{
    h264,
    mediacodec_plan::{self as plan, MediaCodecError, Recovery},
    yuv420::{self, Matrix, Picture, Planes, RawOutputFormat, Source},
};

/// `AMEDIACODEC_BUFFER_FLAG_KEY_FRAME` (`BUFFER_FLAG_SYNC_FRAME`).
const FLAG_KEY_FRAME: u32 = 1;
/// `AMEDIACODEC_BUFFER_FLAG_CODEC_CONFIG`.
const FLAG_CODEC_CONFIG: u32 = 2;
/// `AMEDIACODEC_BUFFER_FLAG_END_OF_STREAM`.
const FLAG_END_OF_STREAM: u32 = 4;
/// `AMEDIACODEC_CONFIGURE_FLAG_ENCODE`.
const CONFIGURE_FLAG_ENCODE: u32 = 1;

/// `AMEDIACODEC_INFO_TRY_AGAIN_LATER`.
const INFO_TRY_AGAIN_LATER: isize = -1;
/// `AMEDIACODEC_INFO_OUTPUT_FORMAT_CHANGED`.
const INFO_OUTPUT_FORMAT_CHANGED: isize = -2;
/// `AMEDIACODEC_INFO_OUTPUT_BUFFERS_CHANGED`.
const INFO_OUTPUT_BUFFERS_CHANGED: isize = -3;

/// `BITRATE_MODE_CBR`.
const BITRATE_MODE_CBR: i32 = 2;

/// The microseconds between two presentation times a decoder's input gets (30 fps): the codec
/// reads them for ordering, not for pacing.
const DECODE_PTS_STEP_US: i64 = 33_333;

/// Android log priorities.
const LOG_INFO: c_int = 4;
const LOG_WARN: c_int = 5;

type RawCodec = *mut c_void;
type RawFormat = *mut c_void;

/// `AMediaCodecBufferInfo`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct BufferInfo {
    offset: i32,
    size: i32,
    presentation_time_us: i64,
    flags: u32,
}

// ---------------------------------------------------------------------------
// Logging and the system property of the API level
// ---------------------------------------------------------------------------

#[cfg(target_os = "android")]
fn android_log(priority: c_int, message: &str) {
    extern "C" {
        fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }
    let clean: Vec<u8> = message.bytes().filter(|&b| b != 0).collect();
    if let Ok(text) = std::ffi::CString::new(clean) {
        // SAFETY: both strings are NUL terminated and live for the call.
        unsafe {
            __android_log_write(priority, c"azul-video".as_ptr(), text.as_ptr());
        }
    }
}

#[cfg(not(target_os = "android"))]
fn android_log(_priority: c_int, message: &str) {
    eprintln!("[azul-video] {message}");
}

/// The device's API level (`ro.build.version.sdk`), for the log line that names the codec.
#[cfg(target_os = "android")]
fn system_api_level() -> Option<u32> {
    extern "C" {
        fn __system_property_get(name: *const c_char, value: *mut c_char) -> c_int;
    }
    let mut value = [0 as c_char; 92];
    // SAFETY: the name is NUL terminated; the buffer is PROP_VALUE_MAX (92) bytes, which is what
    // the call writes at most, terminator included.
    let length = unsafe { __system_property_get(c"ro.build.version.sdk".as_ptr(), value.as_mut_ptr()) };
    if length < 1 {
        return None;
    }
    // SAFETY: the call wrote a NUL terminated string into `value`.
    let text = unsafe { CStr::from_ptr(value.as_ptr()) }.to_str().ok()?;
    plan::parse_api_level(text)
}

#[cfg(not(target_os = "android"))]
fn system_api_level() -> Option<u32> {
    None
}

// ---------------------------------------------------------------------------
// libmediandk.so
// ---------------------------------------------------------------------------

/// The entry points of `libmediandk.so` the backend calls, resolved once per codec handle and
/// shared by everything that handle makes (no global). The library stays loaded as long as one
/// of them is alive.
pub(super) struct MediaNdk {
    create_decoder_by_type: unsafe extern "C" fn(*const c_char) -> RawCodec,
    create_encoder_by_type: unsafe extern "C" fn(*const c_char) -> RawCodec,
    codec_delete: unsafe extern "C" fn(RawCodec) -> i32,
    codec_configure: unsafe extern "C" fn(RawCodec, RawFormat, *mut c_void, *mut c_void, u32) -> i32,
    codec_start: unsafe extern "C" fn(RawCodec) -> i32,
    codec_stop: unsafe extern "C" fn(RawCodec) -> i32,
    codec_flush: unsafe extern "C" fn(RawCodec) -> i32,
    get_input_buffer: unsafe extern "C" fn(RawCodec, usize, *mut usize) -> *mut u8,
    get_output_buffer: unsafe extern "C" fn(RawCodec, usize, *mut usize) -> *mut u8,
    dequeue_input_buffer: unsafe extern "C" fn(RawCodec, i64) -> isize,
    queue_input_buffer: unsafe extern "C" fn(RawCodec, usize, c_long, usize, u64, u32) -> i32,
    dequeue_output_buffer: unsafe extern "C" fn(RawCodec, *mut BufferInfo, i64) -> isize,
    get_output_format: unsafe extern "C" fn(RawCodec) -> RawFormat,
    release_output_buffer: unsafe extern "C" fn(RawCodec, usize, bool) -> i32,
    format_new: unsafe extern "C" fn() -> RawFormat,
    format_delete: unsafe extern "C" fn(RawFormat) -> i32,
    format_set_int32: unsafe extern "C" fn(RawFormat, *const c_char, i32),
    format_set_string: unsafe extern "C" fn(RawFormat, *const c_char, *const c_char),
    format_get_int32: unsafe extern "C" fn(RawFormat, *const c_char, *mut i32) -> bool,
    format_get_buffer:
        unsafe extern "C" fn(RawFormat, *const c_char, *mut *mut c_void, *mut usize) -> bool,
    format_to_string: unsafe extern "C" fn(RawFormat) -> *const c_char,
    /// API 28: a codec's output format holds the crop window as a rect, not as four integers.
    format_get_rect:
        Option<unsafe extern "C" fn(RawFormat, *const c_char, *mut i32, *mut i32, *mut i32, *mut i32) -> bool>,
    /// API 26.
    set_parameters: Option<unsafe extern "C" fn(RawCodec, RawFormat) -> i32>,
    /// API 28.
    get_name: Option<unsafe extern "C" fn(RawCodec, *mut *mut c_char) -> i32>,
    /// API 28.
    release_name: Option<unsafe extern "C" fn(RawCodec, *mut c_char)>,
    /// API 28.
    get_input_format: Option<unsafe extern "C" fn(RawCodec) -> RawFormat>,
    api_level: Option<u32>,
    /// Last: the function pointers above point into it.
    _library: libloading::Library,
}

impl core::fmt::Debug for MediaNdk {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MediaNdk")
            .field("api_level", &self.api_level)
            .field("set_parameters", &self.set_parameters.is_some())
            .field("get_name", &self.get_name.is_some())
            .finish_non_exhaustive()
    }
}

impl MediaNdk {
    /// Opens `libmediandk.so` and resolves what the backend calls. `Err` says what is missing.
    pub(super) fn load() -> Result<Arc<MediaNdk>, MediaCodecError> {
        let library = crate::desktop::open_first_lib(&["libmediandk.so"]).ok_or_else(|| {
            MediaCodecError::Library(String::from(
                "libmediandk.so did not load: this device has no NDK media library",
            ))
        })?;
        macro_rules! required {
            ($name:literal) => {
                // SAFETY: the symbol is typed by the field it initialises: the NDK's C signature.
                match unsafe { library.get(concat!($name, "\0").as_bytes()) } {
                    Ok(symbol) => *symbol,
                    Err(error) => {
                        return Err(MediaCodecError::Library(format!(
                            "libmediandk.so has no {}: {error}",
                            $name
                        )))
                    }
                }
            };
        }
        macro_rules! optional {
            ($name:literal) => {
                // SAFETY: as `required!`.
                unsafe { library.get(concat!($name, "\0").as_bytes()) }
                    .ok()
                    .map(|symbol| *symbol)
            };
        }
        let ndk = MediaNdk {
            create_decoder_by_type: required!("AMediaCodec_createDecoderByType"),
            create_encoder_by_type: required!("AMediaCodec_createEncoderByType"),
            codec_delete: required!("AMediaCodec_delete"),
            codec_configure: required!("AMediaCodec_configure"),
            codec_start: required!("AMediaCodec_start"),
            codec_stop: required!("AMediaCodec_stop"),
            codec_flush: required!("AMediaCodec_flush"),
            get_input_buffer: required!("AMediaCodec_getInputBuffer"),
            get_output_buffer: required!("AMediaCodec_getOutputBuffer"),
            dequeue_input_buffer: required!("AMediaCodec_dequeueInputBuffer"),
            queue_input_buffer: required!("AMediaCodec_queueInputBuffer"),
            dequeue_output_buffer: required!("AMediaCodec_dequeueOutputBuffer"),
            get_output_format: required!("AMediaCodec_getOutputFormat"),
            release_output_buffer: required!("AMediaCodec_releaseOutputBuffer"),
            format_new: required!("AMediaFormat_new"),
            format_delete: required!("AMediaFormat_delete"),
            format_set_int32: required!("AMediaFormat_setInt32"),
            format_set_string: required!("AMediaFormat_setString"),
            format_get_int32: required!("AMediaFormat_getInt32"),
            format_get_buffer: required!("AMediaFormat_getBuffer"),
            format_to_string: required!("AMediaFormat_toString"),
            format_get_rect: optional!("AMediaFormat_getRect"),
            set_parameters: optional!("AMediaCodec_setParameters"),
            get_name: optional!("AMediaCodec_getName"),
            release_name: optional!("AMediaCodec_releaseName"),
            get_input_format: optional!("AMediaCodec_getInputFormat"),
            api_level: system_api_level(),
            _library: library,
        };
        Ok(Arc::new(ndk))
    }
}

/// `Ok` when a `media_status_t` is `AMEDIA_OK`.
fn status(call: &'static str, code: i32) -> Result<(), MediaCodecError> {
    if code == 0 {
        Ok(())
    } else {
        Err(MediaCodecError::Status { call, code })
    }
}

fn no_codec() -> MediaCodecError {
    MediaCodecError::NoCodec { encoder: false }
}

// ---------------------------------------------------------------------------
// AMediaFormat
// ---------------------------------------------------------------------------

/// An `AMediaFormat`, deleted when it goes.
struct Format {
    ndk: Arc<MediaNdk>,
    raw: RawFormat,
}

impl Format {
    fn new(ndk: &Arc<MediaNdk>) -> Result<Format, MediaCodecError> {
        // SAFETY: a plain constructor.
        let raw = unsafe { (ndk.format_new)() };
        Format::adopt(ndk, raw).ok_or_else(|| {
            MediaCodecError::Unsupported(String::from("AMediaFormat_new gave no format"))
        })
    }

    /// Takes over a format the NDK handed out (its owner deletes it), `None` for null.
    fn adopt(ndk: &Arc<MediaNdk>, raw: RawFormat) -> Option<Format> {
        (!raw.is_null()).then(|| Format {
            ndk: Arc::clone(ndk),
            raw,
        })
    }

    fn set_i32(&self, key: &CStr, value: i32) {
        // SAFETY: `raw` is a live format, `key` NUL terminated.
        unsafe { (self.ndk.format_set_int32)(self.raw, key.as_ptr(), value) }
    }

    fn set_string(&self, key: &CStr, value: &CStr) {
        // SAFETY: as `set_i32`; the format copies the string.
        unsafe { (self.ndk.format_set_string)(self.raw, key.as_ptr(), value.as_ptr()) }
    }

    fn get_i32(&self, key: &CStr) -> Option<i32> {
        let mut value = 0i32;
        // SAFETY: `value` is a live i32 for the call.
        let found = unsafe { (self.ndk.format_get_int32)(self.raw, key.as_ptr(), &mut value) };
        found.then_some(value)
    }

    /// A small binary entry (`image-data`, `csd-0`), copied out.
    fn get_buffer(&self, key: &CStr) -> Option<Vec<u8>> {
        let mut data: *mut c_void = core::ptr::null_mut();
        let mut size = 0usize;
        // SAFETY: both out parameters are live for the call.
        let found =
            unsafe { (self.ndk.format_get_buffer)(self.raw, key.as_ptr(), &mut data, &mut size) };
        if !found || data.is_null() || size == 0 || size > 4096 {
            return None;
        }
        // SAFETY: the format owns `size` bytes at `data` until it changes; they are copied now.
        Some(unsafe { core::slice::from_raw_parts(data.cast::<u8>(), size) }.to_vec())
    }

    /// A rect entry (`crop`): left, top, right and bottom, both edges inclusive. `None` when the
    /// format has none or this Android cannot read one (API < 28).
    fn get_rect(&self, key: &CStr) -> Option<(i32, i32, i32, i32)> {
        let get = self.ndk.format_get_rect?;
        let (mut left, mut top, mut right, mut bottom) = (0i32, 0i32, 0i32, 0i32);
        // SAFETY: the four out parameters are live for the call.
        let found =
            unsafe { get(self.raw, key.as_ptr(), &mut left, &mut top, &mut right, &mut bottom) };
        found.then_some((left, top, right, bottom))
    }

    /// The format as text (`AMediaFormat_toString`), for the log.
    fn describe(&self) -> String {
        // SAFETY: the string belongs to the format and is copied at once.
        let text = unsafe { (self.ndk.format_to_string)(self.raw) };
        if text.is_null() {
            return String::new();
        }
        // SAFETY: NUL terminated, owned by the format.
        unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
    }

    /// What this decoder output format says of the buffers. The crop window is the `crop` rect
    /// (what the codec's own format has; Java's `crop-left` ... keys are made from it), else the
    /// four integers if a vendor's format has those.
    fn output_layout(&self) -> RawOutputFormat {
        let (crop_left, crop_top, crop_right, crop_bottom) = match self.get_rect(c"crop") {
            Some((l, t, r, b)) => (Some(l), Some(t), Some(r), Some(b)),
            None => (
                self.get_i32(c"crop-left"),
                self.get_i32(c"crop-top"),
                self.get_i32(c"crop-right"),
                self.get_i32(c"crop-bottom"),
            ),
        };
        RawOutputFormat {
            color_format: self.get_i32(c"color-format"),
            width: self.get_i32(c"width"),
            height: self.get_i32(c"height"),
            stride: self.get_i32(c"stride"),
            slice_height: self.get_i32(c"slice-height"),
            crop_left,
            crop_top,
            crop_right,
            crop_bottom,
            image_data: self.get_buffer(c"image-data"),
            color_standard: self.get_i32(c"color-standard"),
            color_range: self.get_i32(c"color-range"),
        }
    }
}

impl Drop for Format {
    fn drop(&mut self) {
        // SAFETY: `raw` is a live format of ours, deleted once.
        unsafe {
            (self.ndk.format_delete)(self.raw);
        }
    }
}

// ---------------------------------------------------------------------------
// AMediaCodec
// ---------------------------------------------------------------------------

/// What a dequeue of an output buffer answered.
enum Dequeued {
    Buffer { index: usize, info: BufferInfo },
    FormatChanged,
    BuffersChanged,
    TryAgain,
}

/// An `AMediaCodec`: stopped and deleted when it goes.
struct Codec {
    ndk: Arc<MediaNdk>,
    raw: RawCodec,
    started: bool,
}

impl Codec {
    /// A new H.264 codec of the system's choice (`video/avc`).
    fn create(ndk: &Arc<MediaNdk>, encoder: bool) -> Result<Codec, MediaCodecError> {
        let create = if encoder {
            ndk.create_encoder_by_type
        } else {
            ndk.create_decoder_by_type
        };
        // SAFETY: the type is NUL terminated; a null answer is handled.
        let raw = unsafe { create(c"video/avc".as_ptr()) };
        if raw.is_null() {
            return Err(MediaCodecError::NoCodec { encoder });
        }
        Ok(Codec {
            ndk: Arc::clone(ndk),
            raw,
            started: false,
        })
    }

    /// The codec's name (`c2.android.avc.decoder`); `None` below API 28, which cannot say.
    fn name(&self) -> Option<String> {
        let get = self.ndk.get_name?;
        let mut name: *mut c_char = core::ptr::null_mut();
        // SAFETY: `name` is a live out parameter; the NDK owns the string until it is released.
        let code = unsafe { get(self.raw, &mut name) };
        if code != 0 || name.is_null() {
            return None;
        }
        // SAFETY: NUL terminated, released right after the copy.
        let text = unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned();
        if let Some(release) = self.ndk.release_name {
            // SAFETY: the string came from `get_name` of this codec.
            unsafe { release(self.raw, name) };
        }
        Some(text)
    }

    fn configure(&self, format: &Format, encoder: bool) -> Result<(), MediaCodecError> {
        let flags = if encoder { CONFIGURE_FLAG_ENCODE } else { 0 };
        // SAFETY: live codec and format; no surface, no crypto.
        let code = unsafe {
            (self.ndk.codec_configure)(
                self.raw,
                format.raw,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                flags,
            )
        };
        status("AMediaCodec_configure", code)
    }

    fn start(&mut self) -> Result<(), MediaCodecError> {
        // SAFETY: a configured codec.
        let code = unsafe { (self.ndk.codec_start)(self.raw) };
        status("AMediaCodec_start", code)?;
        self.started = true;
        Ok(())
    }

    /// Stops the codec (it can be configured again). The status is not interesting: a codec in
    /// an error state is deleted next.
    fn stop(&mut self) {
        if self.started {
            // SAFETY: a started codec.
            unsafe {
                (self.ndk.codec_stop)(self.raw);
            }
            self.started = false;
        }
    }

    fn flush(&self) -> Result<(), MediaCodecError> {
        // SAFETY: a live codec.
        status("AMediaCodec_flush", unsafe { (self.ndk.codec_flush)(self.raw) })
    }

    /// The index of an input buffer, `None` when none came free within `timeout_us`.
    fn dequeue_input(&self, timeout_us: i64) -> Result<Option<usize>, MediaCodecError> {
        // SAFETY: a started codec.
        let index = unsafe { (self.ndk.dequeue_input_buffer)(self.raw, timeout_us) };
        match usize::try_from(index) {
            Ok(index) => Ok(Some(index)),
            Err(_) if index == INFO_TRY_AGAIN_LATER => Ok(None),
            Err(_) => Err(MediaCodecError::Status {
                call: "AMediaCodec_dequeueInputBuffer",
                code: i32::try_from(index).unwrap_or(i32::MIN),
            }),
        }
    }

    /// An input buffer (the pointer and its capacity), valid until it is queued.
    fn input_buffer(&self, index: usize) -> Result<(*mut u8, usize), MediaCodecError> {
        let mut capacity = 0usize;
        // SAFETY: an index this codec handed out.
        let data = unsafe { (self.ndk.get_input_buffer)(self.raw, index, &mut capacity) };
        if data.is_null() {
            return Err(MediaCodecError::Unsupported(format!(
                "AMediaCodec_getInputBuffer({index}) gave no buffer"
            )));
        }
        Ok((data, capacity))
    }

    fn queue_input(
        &self,
        index: usize,
        len: usize,
        pts_us: i64,
        flags: u32,
    ) -> Result<(), MediaCodecError> {
        // SAFETY: an index this codec handed out, `len` within its capacity.
        let code = unsafe {
            (self.ndk.queue_input_buffer)(
                self.raw,
                index,
                0,
                len,
                u64::try_from(pts_us.max(0)).unwrap_or(0),
                flags,
            )
        };
        status("AMediaCodec_queueInputBuffer", code)
    }

    fn dequeue_output(&self, timeout_us: i64) -> Result<Dequeued, MediaCodecError> {
        let mut info = BufferInfo::default();
        // SAFETY: a started codec; `info` is live for the call.
        let index = unsafe { (self.ndk.dequeue_output_buffer)(self.raw, &mut info, timeout_us) };
        if let Ok(index) = usize::try_from(index) {
            return Ok(Dequeued::Buffer { index, info });
        }
        match index {
            INFO_TRY_AGAIN_LATER => Ok(Dequeued::TryAgain),
            INFO_OUTPUT_FORMAT_CHANGED => Ok(Dequeued::FormatChanged),
            INFO_OUTPUT_BUFFERS_CHANGED => Ok(Dequeued::BuffersChanged),
            other => Err(MediaCodecError::Status {
                call: "AMediaCodec_dequeueOutputBuffer",
                code: i32::try_from(other).unwrap_or(i32::MIN),
            }),
        }
    }

    /// An output buffer (the pointer and its capacity), valid until it is released.
    fn output_buffer(&self, index: usize) -> Result<(*mut u8, usize), MediaCodecError> {
        let mut capacity = 0usize;
        // SAFETY: an index this codec handed out.
        let data = unsafe { (self.ndk.get_output_buffer)(self.raw, index, &mut capacity) };
        if data.is_null() {
            return Err(MediaCodecError::Unsupported(format!(
                "AMediaCodec_getOutputBuffer({index}) gave no buffer"
            )));
        }
        Ok((data, capacity))
    }

    /// The bytes `info` says an output buffer holds, copied out.
    fn read_output(&self, index: usize, info: &BufferInfo) -> Result<Vec<u8>, MediaCodecError> {
        let (data, capacity) = self.output_buffer(index)?;
        let (Ok(offset), Ok(size)) = (usize::try_from(info.offset), usize::try_from(info.size))
        else {
            return Err(MediaCodecError::Unsupported(format!(
                "an output buffer with offset {} and size {}",
                info.offset, info.size
            )));
        };
        if offset.checked_add(size).map_or(true, |end| end > capacity) {
            return Err(MediaCodecError::Unsupported(format!(
                "an output buffer of {size} bytes at {offset} in a capacity of {capacity}"
            )));
        }
        // SAFETY: `offset + size` is inside the `capacity` bytes the codec gave us, and the buffer
        // is ours until it is released.
        Ok(unsafe { core::slice::from_raw_parts(data.add(offset), size) }.to_vec())
    }

    fn release_output(&self, index: usize) -> Result<(), MediaCodecError> {
        // SAFETY: an index this codec handed out, released once.
        let code = unsafe { (self.ndk.release_output_buffer)(self.raw, index, false) };
        status("AMediaCodec_releaseOutputBuffer", code)
    }

    /// The output format as it is now.
    fn output_format(&self) -> Option<Format> {
        // SAFETY: a started codec; the format is ours to delete.
        Format::adopt(&self.ndk, unsafe { (self.ndk.get_output_format)(self.raw) })
    }

    /// The input format (API 28): the stride and slice height an encoder's buffers have.
    fn input_format(&self) -> Option<Format> {
        let get = self.ndk.get_input_format?;
        // SAFETY: a configured codec; the format is ours to delete.
        Format::adopt(&self.ndk, unsafe { get(self.raw) })
    }

    /// Applies `params` to the running codec. `Ok(false)` when this Android has no way (API < 26).
    fn set_parameters(&self, params: &Format) -> Result<bool, MediaCodecError> {
        let Some(set) = self.ndk.set_parameters else {
            return Ok(false);
        };
        // SAFETY: live codec and format.
        status("AMediaCodec_setParameters", unsafe { set(self.raw, params.raw) })?;
        Ok(true)
    }
}

impl Drop for Codec {
    fn drop(&mut self) {
        self.stop();
        // SAFETY: a live codec of ours, deleted once.
        unsafe {
            (self.ndk.codec_delete)(self.raw);
        }
    }
}

// ---------------------------------------------------------------------------
// What the capability probes ask
// ---------------------------------------------------------------------------

/// Whether the NDK's media library is here with everything the backend calls (cheap: what
/// `VideoDecoder::open` / `VideoEncoder::open` check before they make a codec).
pub(super) fn availability() -> Result<(), String> {
    MediaNdk::load().map(|_| ()).map_err(|e| e.to_string())
}

/// Proof that a codec of the kind exists: makes one, names it, deletes it. The engine's label
/// (`MediaCodec (c2.android.avc.decoder, software)`), or why none could be made.
pub(super) fn prove(encoder: bool) -> Result<String, String> {
    let ndk = MediaNdk::load().map_err(|e| e.to_string())?;
    let codec = Codec::create(&ndk, encoder).map_err(|e| e.to_string())?;
    Ok(plan::engine_label(codec.name().as_deref()))
}

// ---------------------------------------------------------------------------
// The decoder
// ---------------------------------------------------------------------------

/// An H.264 decoder on a MediaCodec. Made and used on one thread (the handle's codec thread).
pub(super) struct MediaCodecDecoder {
    ndk: Arc<MediaNdk>,
    codec: Option<Codec>,
    name: Option<String>,
    /// The size the codec was configured for: the stream's.
    configured: Option<(u32, u32)>,
    /// The last SPS the stream sent, parsed.
    sps: Option<h264::SpsInfo>,
    /// The last SPS and PPS as Annex-B, put in front of a key frame that has none (a codec that
    /// was made again starts with one).
    parameter_sets: Vec<u8>,
    /// The layout of the output buffers, from the last output format.
    picture: Option<(Picture, RawOutputFormat)>,
    /// What the app asked frames to be handed out as.
    output: RawImageFormat,
    /// Access units queued whose picture is not out yet.
    in_flight: u32,
    next_pts_us: i64,
    /// Nothing is fed until a key frame: a picture without its reference is garbage.
    need_key: bool,
    consecutive_failures: u32,
    gave_up: bool,
    said: plan::Said,
}

impl core::fmt::Debug for MediaCodecDecoder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MediaCodecDecoder")
            .field("name", &self.name)
            .field("configured", &self.configured)
            .field("in_flight", &self.in_flight)
            .finish_non_exhaustive()
    }
}

impl MediaCodecDecoder {
    /// Makes a decoder: a codec of the system's choice is created (and named), configured later,
    /// when the first SPS says the size. `Err` says why there is none.
    pub(super) fn open_h264() -> Result<MediaCodecDecoder, String> {
        let ndk = MediaNdk::load().map_err(|e| e.to_string())?;
        let codec = Codec::create(&ndk, false).map_err(|e| e.to_string())?;
        let name = codec.name();
        android_log(
            LOG_INFO,
            &format!(
                "decoder: {} (api {})",
                plan::engine_label(name.as_deref()),
                ndk.api_level.map_or_else(|| String::from("?"), |level| level.to_string())
            ),
        );
        Ok(MediaCodecDecoder {
            ndk,
            codec: Some(codec),
            name,
            configured: None,
            sps: None,
            parameter_sets: Vec::new(),
            picture: None,
            output: RawImageFormat::RGBA8,
            in_flight: 0,
            next_pts_us: 0,
            need_key: true,
            consecutive_failures: 0,
            gave_up: false,
            said: plan::Said::default(),
        })
    }

    /// The decoder's codec name, if the platform said it.
    pub(super) fn codec_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Hand frames out as an NV12 variant, BGRA8 or RGBA8 (anything else is RGBA8).
    pub(super) fn set_output_format(&mut self, format: RawImageFormat) {
        self.output = if format.is_nv12() || format == RawImageFormat::BGRA8 {
            format
        } else {
            RawImageFormat::RGBA8
        };
    }

    fn say(&mut self, priority: c_int, line: &str) {
        if self.said.is_new(line) {
            android_log(priority, line);
        }
    }

    /// Feeds an Annex-B chunk (any number of access units); the pictures that came out are
    /// returned. A chunk the decoder cannot take is dropped with a log line, never a panic.
    pub(super) fn decode(&mut self, data: &[u8]) -> Vec<VideoFrame> {
        let mut frames = Vec::new();
        if self.gave_up {
            return frames;
        }
        if data.len() > plan::MAX_ACCESS_UNIT_BYTES {
            self.say(
                LOG_WARN,
                &format!("decoder: a chunk of {} bytes is over the cap, dropped", data.len()),
            );
            return frames;
        }
        for unit in h264::split_access_units(data) {
            if let Err(error) = self.feed(unit, &mut frames) {
                self.fail(&error);
            }
        }
        frames
    }

    /// One access unit: configure for its stream if need be, queue it, take what is ready.
    fn feed(&mut self, unit: &[u8], frames: &mut Vec<VideoFrame>) -> Result<(), MediaCodecError> {
        let mut has_sps = false;
        for span in h264::nal_spans(unit) {
            if span.nal_type(unit) == h264::NAL_SPS {
                has_sps = true;
                if let Some(info) = h264::parse_sps(span.bytes(unit)) {
                    self.sps = Some(info);
                }
            }
        }
        if has_sps {
            let sets = h264::parameter_sets(unit);
            if !sets.is_empty() {
                self.parameter_sets = sets;
            }
        }
        // Without an SPS there is nothing to configure a codec for: the stream starts at its
        // first key frame with parameter sets.
        let Some(size) = self.sps.as_ref().map(|sps| (sps.width, sps.height)) else {
            return Ok(());
        };
        if self.configured != Some(size) {
            self.configure(size)?;
        }
        let is_key = h264::contains_idr(unit);
        if self.need_key {
            if !is_key {
                return Ok(());
            }
            self.need_key = false;
        }
        // A key frame without its parameter sets (a stream that sends them once) gets the ones
        // this decoder kept.
        let bytes: Cow<'_, [u8]> = if is_key && !has_sps && !self.parameter_sets.is_empty() {
            Cow::Owned([self.parameter_sets.as_slice(), unit].concat())
        } else {
            Cow::Borrowed(unit)
        };
        self.queue_unit(&bytes, frames)
    }

    /// Configures (and starts) the codec for a stream of `size`: a codec that has run is stopped
    /// and configured again.
    fn configure(&mut self, (width, height): (u32, u32)) -> Result<(), MediaCodecError> {
        plan::check_decode_size(width, height)?;
        let ndk = Arc::clone(&self.ndk);
        let mut codec = match self.codec.take() {
            Some(mut codec) => {
                codec.stop();
                codec
            }
            None => Codec::create(&ndk, false)?,
        };
        let format = Format::new(&ndk)?;
        format.set_string(c"mime", c"video/avc");
        format.set_i32(c"width", i32::try_from(width).unwrap_or(i32::MAX));
        format.set_i32(c"height", i32::try_from(height).unwrap_or(i32::MAX));
        // A picture's worth of bitstream with room to spare: the codec's own default is tied to
        // the size and refuses what it did not expect.
        let max_input = (u64::from(width) * u64::from(height) * 3 / 2).clamp(256 * 1024, 8 << 20);
        format.set_i32(c"max-input-size", i32::try_from(max_input).unwrap_or(i32::MAX));
        // Realtime and low latency: ignored by a codec that does not know the keys.
        format.set_i32(c"priority", 0);
        format.set_i32(c"low-latency", 1);
        codec.configure(&format, false)?;
        codec.start()?;
        self.codec = Some(codec);
        self.configured = Some((width, height));
        self.picture = None;
        self.in_flight = 0;
        self.need_key = true;
        self.said = plan::Said::default();
        android_log(
            LOG_INFO,
            &format!(
                "decoder: configured for {width}x{height}: {}",
                plan::engine_label(self.name.as_deref())
            ),
        );
        Ok(())
    }

    /// Queues one access unit, after taking what is ready (that frees buffers), and waits a
    /// moment for the first picture it produces.
    fn queue_unit(
        &mut self,
        bytes: &[u8],
        frames: &mut Vec<VideoFrame>,
    ) -> Result<(), MediaCodecError> {
        let started = Instant::now();
        let index = loop {
            self.drain(frames, 0)?;
            let codec = self.codec.as_ref().ok_or_else(no_codec)?;
            if let Some(index) = codec.dequeue_input(plan::INPUT_WAIT_US)? {
                break index;
            }
            if started.elapsed() > Duration::from_millis(plan::INPUT_STALL_MS) {
                return Err(MediaCodecError::Timeout("an input buffer of the decoder"));
            }
        };
        let pts = self.next_pts_us;
        self.next_pts_us += DECODE_PTS_STEP_US;
        let codec = self.codec.as_ref().ok_or_else(no_codec)?;
        let (data, capacity) = codec.input_buffer(index)?;
        if bytes.len() > capacity {
            // The buffer goes back empty: the codec keeps it otherwise.
            codec.queue_input(index, 0, pts, 0)?;
            return Err(MediaCodecError::Unsupported(format!(
                "an access unit of {} bytes does not fit the decoder's input buffer of {capacity}",
                bytes.len()
            )));
        }
        // SAFETY: `data` has `capacity >= bytes.len()` bytes and is ours until it is queued.
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len()) };
        codec.queue_input(index, bytes.len(), pts, 0)?;
        self.in_flight += 1;
        self.drain(frames, plan::OUTPUT_WAIT_US)
    }

    /// Takes the pictures that are ready: waits up to `first_wait_us` for the first (only while
    /// an access unit is in flight), then takes what is there without waiting.
    fn drain(
        &mut self,
        frames: &mut Vec<VideoFrame>,
        first_wait_us: i64,
    ) -> Result<(), MediaCodecError> {
        let mut wait = if self.in_flight > 0 { first_wait_us } else { 0 };
        for _ in 0..plan::MAX_DRAIN_STEPS {
            let codec = self.codec.as_ref().ok_or_else(no_codec)?;
            match codec.dequeue_output(wait)? {
                Dequeued::TryAgain => return Ok(()),
                Dequeued::BuffersChanged => {}
                Dequeued::FormatChanged => self.read_output_format(),
                Dequeued::Buffer { index, info } => {
                    wait = 0;
                    self.take_output(index, &info, frames)?;
                }
            }
        }
        Ok(())
    }

    /// Reads the new output format: where the pictures lie in the buffers from now on.
    fn read_output_format(&mut self) {
        let Some(format) = self.codec.as_ref().and_then(Codec::output_format) else {
            return;
        };
        let raw = format.output_layout();
        let line = format!("decoder: output format {}", format.describe());
        let hint = self
            .sps
            .as_ref()
            .map(|sps| (sps.width as usize, sps.height as usize));
        match yuv420::plan_output(&raw, hint) {
            Ok(picture) => {
                self.say(LOG_INFO, &line);
                self.picture = Some((picture, raw));
            }
            Err(why) => {
                self.say(LOG_WARN, &format!("{line}: {why}"));
                self.picture = None;
            }
        }
    }

    /// One output buffer: the picture is made and the buffer given back, whatever happened.
    fn take_output(
        &mut self,
        index: usize,
        info: &BufferInfo,
        frames: &mut Vec<VideoFrame>,
    ) -> Result<(), MediaCodecError> {
        let made = self.make_frame(index, info);
        if let Some(codec) = self.codec.as_ref() {
            codec.release_output(index)?;
        }
        if info.flags & FLAG_CODEC_CONFIG == 0 && info.size > 0 {
            self.in_flight = self.in_flight.saturating_sub(1);
        }
        match made {
            Ok(Some(frame)) => {
                self.consecutive_failures = 0;
                frames.push(frame);
            }
            Ok(None) => {}
            Err(why) => self.say(LOG_WARN, &format!("decoder: a picture was dropped: {why}")),
        }
        Ok(())
    }

    /// The frame an output buffer holds, `None` for a buffer with no picture in it.
    fn make_frame(&self, index: usize, info: &BufferInfo) -> Result<Option<VideoFrame>, String> {
        if info.size <= 0 || info.flags & FLAG_CODEC_CONFIG != 0 {
            return Ok(None);
        }
        let (picture, raw) = self
            .picture
            .as_ref()
            .ok_or_else(|| String::from("no usable output format yet"))?;
        let bytes = self
            .codec
            .as_ref()
            .ok_or_else(|| String::from("the codec is gone"))?
            .read_output(index, info)
            .map_err(|e| e.to_string())?;
        let nv12 = picture
            .to_nv12(&bytes)
            .ok_or_else(|| format!("{} bytes are too short for the {}x{} picture", bytes.len(), picture.width, picture.height))?;
        let (matrix, full_range) = yuv420::picture_colour(
            raw.color_standard,
            raw.color_range,
            self.sps.as_ref().and_then(|sps| sps.colour),
            picture.height,
        );
        let nv12_format = RawImageFormat::nv12(matrix == Matrix::Bt709, full_range);
        let (width, height) = (
            u32::try_from(picture.width).map_err(|e| e.to_string())?,
            u32::try_from(picture.height).map_err(|e| e.to_string())?,
        );
        if self.output.is_nv12() {
            return Ok(Some(VideoFrame::with_format(
                width,
                height,
                U8Vec::from_vec(nv12),
                nv12_format,
            )));
        }
        let mut rgba = nv12_to_rgba(&nv12, picture.width, picture.height, nv12_format)
            .ok_or_else(|| String::from("the NV12 picture did not convert"))?;
        if self.output == RawImageFormat::BGRA8 {
            for pixel in rgba.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            return Ok(Some(VideoFrame::with_format(
                width,
                height,
                U8Vec::from_vec(rgba),
                RawImageFormat::BGRA8,
            )));
        }
        Ok(Some(VideoFrame::new(width, height, U8Vec::from_vec(rgba))))
    }

    /// End of stream: the codec is told, the pictures it held back for reordering are returned,
    /// and it is flushed so the next key frame starts a new run.
    pub(super) fn flush(&mut self) -> Vec<VideoFrame> {
        let mut frames = Vec::new();
        if self.gave_up || self.codec.is_none() || self.configured.is_none() {
            return frames;
        }
        if let Err(error) = self.end_of_stream(&mut frames) {
            self.fail(&error);
        }
        frames
    }

    fn end_of_stream(&mut self, frames: &mut Vec<VideoFrame>) -> Result<(), MediaCodecError> {
        let started = Instant::now();
        let total = Duration::from_millis(plan::DRAIN_TOTAL_MS);
        let index = loop {
            self.drain(frames, 0)?;
            let codec = self.codec.as_ref().ok_or_else(no_codec)?;
            if let Some(index) = codec.dequeue_input(plan::INPUT_WAIT_US)? {
                break index;
            }
            if started.elapsed() > total {
                return Err(MediaCodecError::Timeout("an input buffer for the end of the stream"));
            }
        };
        let pts = self.next_pts_us;
        self.codec
            .as_ref()
            .ok_or_else(no_codec)?
            .queue_input(index, 0, pts, FLAG_END_OF_STREAM)?;
        let mut last_output = Instant::now();
        while started.elapsed() < total {
            let codec = self.codec.as_ref().ok_or_else(no_codec)?;
            match codec.dequeue_output(plan::OUTPUT_WAIT_US)? {
                Dequeued::TryAgain => {
                    if last_output.elapsed() > Duration::from_millis(plan::DRAIN_IDLE_MS) {
                        break;
                    }
                }
                Dequeued::BuffersChanged => {}
                Dequeued::FormatChanged => self.read_output_format(),
                Dequeued::Buffer { index, info } => {
                    last_output = Instant::now();
                    let end = info.flags & FLAG_END_OF_STREAM != 0;
                    self.take_output(index, &info, frames)?;
                    if end {
                        break;
                    }
                }
            }
        }
        // After the end of the stream the codec takes no input until it is flushed.
        self.codec.as_ref().ok_or_else(no_codec)?.flush()?;
        self.in_flight = 0;
        self.need_key = true;
        Ok(())
    }

    /// A call failed: say it once, and make the codec again unless it keeps failing.
    fn fail(&mut self, error: &MediaCodecError) {
        self.say(LOG_WARN, &format!("decoder: {error}"));
        match error {
            MediaCodecError::Timeout(_) => {
                // A stalled codec: flush it, the next key frame starts a new run.
                let flushed = self.codec.as_ref().is_some_and(|codec| codec.flush().is_ok());
                if flushed {
                    self.in_flight = 0;
                    self.need_key = true;
                } else {
                    self.remake();
                }
            }
            error if error.codec_is_lost() => self.remake(),
            _ => {}
        }
    }

    /// Makes the codec again (the stream is configured at its next key frame), or gives up.
    fn remake(&mut self) {
        self.consecutive_failures += 1;
        if plan::recovery_after(self.consecutive_failures) == Recovery::GiveUp {
            self.say(LOG_WARN, "decoder: too many failures in a row, closed");
            self.codec = None;
            self.gave_up = true;
            return;
        }
        self.codec = Codec::create(&self.ndk, false).ok();
        self.configured = None;
        self.picture = None;
        self.in_flight = 0;
        self.need_key = true;
    }
}

// ---------------------------------------------------------------------------
// The encoder
// ---------------------------------------------------------------------------

/// An H.264 encoder on a MediaCodec. Made and used on one thread (the handle's codec thread).
pub(super) struct MediaCodecEncoder {
    ndk: Arc<MediaNdk>,
    codec: Option<Codec>,
    name: Option<String>,
    config: plan::EncoderConfig,
    /// The input layout the codec's buffers have.
    input: Planes,
    /// The matrix and range the frames are tagged with.
    tag: (Matrix, bool),
    /// The SPS and PPS of the stream, put in front of every key frame.
    parameter_sets: Vec<u8>,
    started_at: Instant,
    last_pts_us: i64,
    /// Frames queued whose packet is not out yet.
    in_flight: u32,
    /// Frames this codec instance took.
    frames_in: u64,
    consecutive_failures: u32,
    gave_up: bool,
    said: plan::Said,
}

impl core::fmt::Debug for MediaCodecEncoder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MediaCodecEncoder")
            .field("name", &self.name)
            .field("config", &self.config)
            .field("in_flight", &self.in_flight)
            .finish_non_exhaustive()
    }
}

impl MediaCodecEncoder {
    /// Opens an encoder for `width` x `height` pictures at `bitrate_kbps`. `Err` says why there
    /// is none: no codec, a size no encoder takes, a format every colour layout of which the
    /// codec refused.
    pub(super) fn open(width: u32, height: u32, bitrate_kbps: u32) -> Result<MediaCodecEncoder, String> {
        let config = plan::EncoderConfig::realtime(width, height, bitrate_kbps)
            .map_err(|e| e.to_string())?;
        let ndk = MediaNdk::load().map_err(|e| e.to_string())?;
        let mut encoder = MediaCodecEncoder {
            ndk,
            codec: None,
            name: None,
            config,
            input: Planes::semi_planar(width as usize, height as usize),
            tag: default_tag(),
            parameter_sets: Vec::new(),
            started_at: Instant::now(),
            last_pts_us: -1,
            in_flight: 0,
            frames_in: 0,
            consecutive_failures: 0,
            gave_up: false,
            said: plan::Said::default(),
        };
        encoder.start_codec().map_err(|e| e.to_string())?;
        Ok(encoder)
    }

    /// Whether the encoder's codec is hardware; false for software and when it is not known.
    pub(super) fn hardware(&self) -> bool {
        plan::is_hardware(self.name.as_deref()) == Some(true)
    }

    /// The rate the encoder spends now (kbit/s).
    pub(super) fn bitrate_kbps(&self) -> u32 {
        self.config.bitrate_kbps
    }

    fn say(&mut self, priority: c_int, line: &str) {
        if self.said.is_new(line) {
            android_log(priority, line);
        }
    }

    /// The codec's format for `color_format`.
    fn format(&self, color_format: i32, with_bitrate_mode: bool) -> Result<Format, MediaCodecError> {
        let format = Format::new(&self.ndk)?;
        let config = &self.config;
        format.set_string(c"mime", c"video/avc");
        format.set_i32(c"width", i32::try_from(config.width).unwrap_or(i32::MAX));
        format.set_i32(c"height", i32::try_from(config.height).unwrap_or(i32::MAX));
        format.set_i32(c"bitrate", config.bitrate_bps());
        format.set_i32(c"frame-rate", i32::try_from(config.fps).unwrap_or(30));
        format.set_i32(c"i-frame-interval", i32::try_from(config.i_frame_interval_s).unwrap_or(2));
        format.set_i32(c"color-format", color_format);
        if with_bitrate_mode {
            format.set_i32(c"bitrate-mode", BITRATE_MODE_CBR);
        }
        // Realtime, no B-frames, and the colour the frames are in (the VUI of the stream):
        // keys a codec does not know are ignored.
        format.set_i32(c"priority", 0);
        format.set_i32(c"low-latency", 1);
        format.set_i32(c"max-bframes", 0);
        let (matrix, full_range) = self.tag;
        format.set_i32(c"color-standard", if matrix == Matrix::Bt709 { 1 } else { 4 });
        format.set_i32(c"color-range", if full_range { 1 } else { 2 });
        format.set_i32(c"color-transfer", 3);
        Ok(format)
    }

    /// Makes the codec: the input colour formats in the order of [`plan::ENCODER_INPUT_FORMATS`],
    /// the first the codec configures and starts with. The old codec, if any, is deleted.
    fn start_codec(&mut self) -> Result<(), MediaCodecError> {
        self.codec = None;
        let mut first_error = None;
        for &color_format in &plan::ENCODER_INPUT_FORMATS {
            // CBR first (what a call wants); a codec that refuses the mode gets none.
            for with_bitrate_mode in [true, false] {
                match self.try_start(color_format, with_bitrate_mode) {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        let line = format!(
                            "encoder: colour format {color_format:#x}, cbr {with_bitrate_mode}: {error}"
                        );
                        self.say(LOG_INFO, &line);
                        if first_error.is_none() {
                            first_error = Some(error);
                        }
                    }
                }
            }
        }
        Err(first_error.unwrap_or(MediaCodecError::NoCodec { encoder: true }))
    }

    fn try_start(&mut self, color_format: i32, with_bitrate_mode: bool) -> Result<(), MediaCodecError> {
        let ndk = Arc::clone(&self.ndk);
        let mut codec = Codec::create(&ndk, true)?;
        let format = self.format(color_format, with_bitrate_mode)?;
        codec.configure(&format, true)?;
        codec.start()?;
        // The stride and slice height the codec's input buffers have (API 28 says; before it a
        // codec takes the picture as it is).
        let (width, height) = (self.config.width as usize, self.config.height as usize);
        let (mut stride, mut slice_height) = (width, height);
        if let Some(input) = codec.input_format() {
            if let Some(value) = input.get_i32(c"stride").and_then(|v| usize::try_from(v).ok()) {
                if value >= width {
                    stride = value;
                }
            }
            if let Some(value) = input.get_i32(c"slice-height").and_then(|v| usize::try_from(v).ok()) {
                if value >= height {
                    slice_height = value;
                }
            }
        }
        self.input = if color_format == yuv420::COLOR_FORMAT_YUV420_SEMI_PLANAR {
            Planes::semi_planar(stride, slice_height)
        } else {
            Planes::planar(stride, slice_height)
        };
        if self.name.is_none() {
            self.name = codec.name();
        }
        android_log(
            LOG_INFO,
            &format!(
                "encoder: {} {}x{} {} kbit/s, colour format {color_format:#x}, stride {stride} x {slice_height} (api {})",
                plan::engine_label(self.name.as_deref()),
                self.config.width,
                self.config.height,
                self.config.bitrate_kbps,
                self.ndk.api_level.map_or_else(|| String::from("?"), |level| level.to_string())
            ),
        );
        self.codec = Some(codec);
        self.in_flight = 0;
        self.frames_in = 0;
        self.parameter_sets.clear();
        Ok(())
    }

    /// Spends `kbps` from the next frame on: `video-bitrate` on a running codec (API 26); before
    /// it the new rate is the codec's at its next restart.
    pub(super) fn set_bitrate(&mut self, kbps: u32) {
        let (kbps, moved) = plan::clamp_bitrate(kbps);
        if moved {
            self.say(
                LOG_INFO,
                &format!("encoder: a bitrate outside {}..{} kbit/s was moved into it", plan::MIN_KBPS, plan::MAX_KBPS),
            );
        }
        if kbps == self.config.bitrate_kbps {
            // The app asks every frame for the rate it wants; a rate that is the codec's already
            // (an out-of-range one is moved to it each time) is no call.
            return;
        }
        self.config.bitrate_kbps = kbps;
        let applied = Format::new(&self.ndk).and_then(|params| {
            params.set_i32(c"video-bitrate", self.config.bitrate_bps());
            match self.codec.as_ref() {
                Some(codec) => codec.set_parameters(&params),
                None => Ok(true),
            }
        });
        match applied {
            Ok(true) => {}
            Ok(false) => self.say(
                LOG_INFO,
                "encoder: this Android cannot change the bitrate of a running codec (API 26), the new rate applies at the next restart",
            ),
            Err(error) => self.fail(&error),
        }
    }

    /// Encodes one frame (a key frame when asked); the packets that are ready come back, which
    /// may be those of earlier frames or none (a codec answers a little later).
    pub(super) fn encode(
        &mut self,
        frame: &VideoFrame,
        force_keyframe: bool,
        micros: Option<i64>,
    ) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        if self.gave_up {
            return packets;
        }
        if let Err(error) = self.submit(frame, force_keyframe, micros, &mut packets) {
            self.fail(&error);
        }
        packets
    }

    /// What `frame` is for the codec: its pixels and the matrix they are in. RGB frames are
    /// tagged with the matrix of the convention of their height.
    fn source_of<'a>(&self, frame: &'a VideoFrame) -> Result<(Source<'a>, (Matrix, bool)), MediaCodecError> {
        let bytes = frame.bytes.as_ref();
        let expected = frame.expected_len().ok_or_else(|| {
            MediaCodecError::Unsupported(format!("a {:?} frame is not one the encoder takes", frame.format))
        })?;
        if bytes.len() < expected {
            return Err(MediaCodecError::Unsupported(format!(
                "a {}x{} {:?} frame needs {expected} bytes and has {}",
                frame.width,
                frame.height,
                frame.format,
                bytes.len()
            )));
        }
        let height = frame.height as usize;
        if frame.format.is_nv12() {
            let matrix = if frame.format.is_rec709() { Matrix::Bt709 } else { Matrix::Bt601 };
            return Ok((Source::Nv12(bytes), (matrix, frame.format.is_full_range())));
        }
        let (matrix, _) = yuv420::picture_colour(None, None, None, height);
        match frame.format {
            RawImageFormat::RGBA8 => Ok((Source::Rgba(bytes), (matrix, false))),
            RawImageFormat::BGRA8 => Ok((Source::Bgra(bytes), (matrix, false))),
            other => Err(MediaCodecError::Unsupported(format!(
                "a {other:?} frame is not one the encoder takes (NV12, RGBA8, BGRA8)"
            ))),
        }
    }

    fn submit(
        &mut self,
        frame: &VideoFrame,
        force_keyframe: bool,
        micros: Option<i64>,
        packets: &mut Vec<Vec<u8>>,
    ) -> Result<(), MediaCodecError> {
        let (source, tag) = self.source_of(frame)?;
        // A new size, a codec that was lost, or the first frame in another matrix: a new codec.
        let size = (frame.width, frame.height);
        let resized = size != (self.config.width, self.config.height);
        if resized {
            plan::check_size(size.0, size.1)?;
            self.config.width = size.0;
            self.config.height = size.1;
        }
        let retagged = tag != self.tag && self.frames_in == 0;
        if retagged {
            self.tag = tag;
        }
        if self.codec.is_none() || resized || retagged {
            self.start_codec()?;
        }
        if force_keyframe && self.frames_in > 0 {
            self.request_sync()?;
        }
        let started = Instant::now();
        let index = loop {
            self.drain(packets, 0)?;
            let codec = self.codec.as_ref().ok_or_else(no_codec)?;
            if let Some(index) = codec.dequeue_input(plan::INPUT_WAIT_US)? {
                break index;
            }
            if started.elapsed() > Duration::from_millis(plan::INPUT_STALL_MS) {
                return Err(MediaCodecError::Timeout("an input buffer of the encoder"));
            }
        };
        let pts = self.next_pts_us(micros);
        let (width, height) = (frame.width as usize, frame.height as usize);
        let needed = Picture::new(self.input, width, height).required_len().ok_or_else(|| {
            MediaCodecError::Unsupported(String::from("the input layout overflows"))
        })?;
        let aligned = self.input.y.row_inc.saturating_mul(self.input_rows()).saturating_mul(3) / 2;
        let codec = self.codec.as_ref().ok_or_else(no_codec)?;
        let (data, capacity) = codec.input_buffer(index)?;
        if capacity < needed {
            codec.queue_input(index, 0, pts, 0)?;
            return Err(MediaCodecError::Unsupported(format!(
                "the encoder's input buffer holds {capacity} bytes and a {width}x{height} picture in its layout needs {needed}"
            )));
        }
        // SAFETY: `data` has `capacity` bytes and is ours until it is queued.
        let buffer = unsafe { core::slice::from_raw_parts_mut(data, capacity) };
        if yuv420::write_picture(buffer, &self.input, width, height, source, self.tag.0).is_none() {
            codec.queue_input(index, 0, pts, 0)?;
            return Err(MediaCodecError::Unsupported(String::from(
                "the picture did not fit the encoder's input buffer",
            )));
        }
        let queued = needed.max(aligned).min(capacity);
        codec.queue_input(index, queued, pts, 0)?;
        self.in_flight += 1;
        self.frames_in += 1;
        self.drain(packets, plan::OUTPUT_WAIT_US)
    }

    /// The slice height of the input layout: where the chroma plane starts, in rows.
    fn input_rows(&self) -> usize {
        self.input
            .u
            .offset
            .checked_div(self.input.y.row_inc.max(1))
            .unwrap_or(0)
    }

    /// The presentation time of the next frame: the app's stamp, else the time since the encoder
    /// opened (so a dropped frame is visible to the rate control); always rising.
    fn next_pts_us(&mut self, micros: Option<i64>) -> i64 {
        let wanted = micros.unwrap_or_else(|| {
            i64::try_from(self.started_at.elapsed().as_micros()).unwrap_or(i64::MAX)
        });
        let pts = wanted.max(self.last_pts_us + 1);
        self.last_pts_us = pts;
        pts
    }

    /// Asks for a key frame in the next picture.
    fn request_sync(&mut self) -> Result<(), MediaCodecError> {
        let params = Format::new(&self.ndk)?;
        params.set_i32(c"request-sync", 0);
        let applied = self.codec.as_ref().ok_or_else(no_codec)?.set_parameters(&params)?;
        if applied {
            return Ok(());
        }
        // Before API 26: a codec's first frame is a key frame, so a new codec is one.
        self.start_codec()
    }

    /// Takes the packets that are ready: waits up to `first_wait_us` for the first (only while a
    /// frame is in flight), then takes what is there.
    fn drain(&mut self, packets: &mut Vec<Vec<u8>>, first_wait_us: i64) -> Result<(), MediaCodecError> {
        let mut wait = if self.in_flight > 0 { first_wait_us } else { 0 };
        for _ in 0..plan::MAX_DRAIN_STEPS {
            let codec = self.codec.as_ref().ok_or_else(no_codec)?;
            match codec.dequeue_output(wait)? {
                Dequeued::TryAgain => return Ok(()),
                Dequeued::BuffersChanged => {}
                Dequeued::FormatChanged => self.read_csd(),
                Dequeued::Buffer { index, info } => {
                    wait = 0;
                    self.take_packet(index, &info, packets)?;
                }
            }
        }
        Ok(())
    }

    /// The parameter sets an output format carries (`csd-0`, `csd-1`), for a codec that gives no
    /// CODEC_CONFIG buffer.
    fn read_csd(&mut self) {
        let Some(format) = self.codec.as_ref().and_then(Codec::output_format) else {
            return;
        };
        let line = format!("encoder: output format {}", format.describe());
        self.say(LOG_INFO, &line);
        if !self.parameter_sets.is_empty() {
            return;
        }
        let mut sets = Vec::new();
        for key in [c"csd-0", c"csd-1"] {
            if let Some(bytes) = format.get_buffer(key) {
                sets.extend_from_slice(&h264::parameter_sets(&bytes));
            }
        }
        if !sets.is_empty() {
            self.parameter_sets = sets;
        }
    }

    /// One output buffer: a packet (with the parameter sets in front of a key frame) or the
    /// stream's parameter sets. The buffer is given back whatever happened.
    fn take_packet(
        &mut self,
        index: usize,
        info: &BufferInfo,
        packets: &mut Vec<Vec<u8>>,
    ) -> Result<(), MediaCodecError> {
        let codec = self.codec.as_ref().ok_or_else(no_codec)?;
        let read = codec.read_output(index, info);
        codec.release_output(index)?;
        let bytes = read?;
        if bytes.is_empty() {
            return Ok(());
        }
        let is_key = info.flags & FLAG_KEY_FRAME != 0 || h264::contains_idr(&bytes);
        if info.flags & FLAG_CODEC_CONFIG != 0 && !h264::contains_idr(&bytes) {
            let sets = h264::parameter_sets(&bytes);
            self.parameter_sets = if sets.is_empty() { bytes } else { sets };
            return Ok(());
        }
        self.in_flight = self.in_flight.saturating_sub(1);
        self.consecutive_failures = 0;
        if is_key && h264::contains_nal(&bytes, h264::NAL_SPS) {
            self.parameter_sets = h264::parameter_sets(&bytes);
        }
        packets.push(if is_key {
            h264::with_parameter_sets(&self.parameter_sets, &bytes)
        } else {
            bytes
        });
        Ok(())
    }

    /// Waits (bounded) for the packets of every frame that was queued: `flush` of the handle.
    pub(super) fn flush(&mut self) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        if self.gave_up || self.codec.is_none() {
            return packets;
        }
        let started = Instant::now();
        let mut idle_since = Instant::now();
        while self.in_flight > 0
            && started.elapsed() < Duration::from_millis(plan::DRAIN_TOTAL_MS)
            && idle_since.elapsed() < Duration::from_millis(plan::DRAIN_IDLE_MS)
        {
            let before = packets.len();
            if let Err(error) = self.drain(&mut packets, plan::OUTPUT_WAIT_US) {
                self.fail(&error);
                break;
            }
            if packets.len() != before {
                idle_since = Instant::now();
            }
        }
        // Frames a rate control dropped never come out: do not wait for them again.
        self.in_flight = 0;
        packets
    }

    /// A call failed: say it once, and make the codec again unless it keeps failing.
    fn fail(&mut self, error: &MediaCodecError) {
        self.say(LOG_WARN, &format!("encoder: {error}"));
        match error {
            MediaCodecError::Timeout(_) => {
                let flushed = self.codec.as_ref().is_some_and(|codec| codec.flush().is_ok());
                if flushed {
                    self.in_flight = 0;
                } else {
                    self.lose_codec();
                }
            }
            error if error.codec_is_lost() => self.lose_codec(),
            _ => {}
        }
    }

    /// The codec is gone: the next frame makes a new one, or the encoder stays closed.
    fn lose_codec(&mut self) {
        self.consecutive_failures += 1;
        self.codec = None;
        self.in_flight = 0;
        if plan::recovery_after(self.consecutive_failures) == Recovery::GiveUp {
            self.say(LOG_WARN, "encoder: too many failures in a row, closed");
            self.gave_up = true;
        }
    }
}

/// The matrix and range a new encoder is configured for before it has seen a frame: Rec.709,
/// video range (what a camera's HD format is).
const fn default_tag() -> (Matrix, bool) {
    (Matrix::Bt709, false)
}
