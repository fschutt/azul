//! Opus through Apple's **AudioToolbox** `AudioConverter` (macOS / iOS): the engine behind
//! `AudioEncoder` / `AudioDecoder` (`codec.rs`).
//!
//! Every symbol is `dlopen`ed at runtime (the VideoToolbox backend's rule: no build-time
//! framework link, the dylib loads on any macOS version, and a missing symbol leaves the handles
//! closed instead of failing to load).
//!
//! An encoder is one converter from interleaved `f32` PCM at the app's rate to Opus at 48 kHz,
//! [`super::codec::OPUS_FRAME_MS`] per packet (the converter resamples when the rates differ); a
//! decoder is the converter the other way. Both run synchronously on the caller's thread: an Opus
//! packet of 20 ms takes a fraction of a millisecond.
//!
//! The converter pulls its input through a callback. The callback hands out everything queued in
//! one buffer the engine keeps alive until the callback runs again (the converter may still read
//! the rest of it at the start of the next call), and answers [`NO_DATA_YET`] when nothing is
//! queued - a non-zero status ends the current call without ending the stream (zero packets with
//! status 0 would mean end of stream, after which a converter needs a reset).

use core::ffi::c_void;
use std::sync::OnceLock;

/// `kAudioFormatLinearPCM` ('lpcm').
const FORMAT_LINEAR_PCM: u32 = 0x6C70_636D;
/// `kAudioFormatOpus` ('opus').
const FORMAT_OPUS: u32 = 0x6F70_7573;
/// `kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked` (native endian: little on every Apple
/// target azul builds for).
const FLAGS_FLOAT_PACKED: u32 = (1 << 0) | (1 << 3);
/// `kAudioConverterEncodeBitRate` ('brat'): UInt32 bits per second.
const PROPERTY_ENCODE_BIT_RATE: u32 = 0x6272_6174;
/// `kAudioConverterPropertyMaximumOutputPacketSize` ('xops'): UInt32 bytes.
const PROPERTY_MAX_OUTPUT_PACKET_SIZE: u32 = 0x786F_7073;
/// `kAudioConverterCompressionMagicCookie` ('cmgc').
const PROPERTY_COMPRESSION_COOKIE: u32 = 0x636D_6763;
/// `kAudioConverterDecompressionMagicCookie` ('dmgc').
const PROPERTY_DECOMPRESSION_COOKIE: u32 = 0x646D_6763;
/// What the input callback answers when nothing is queued ('nodt'): any non-zero status ends the
/// current `AudioConverterFillComplexBuffer` call and keeps the stream going.
const NO_DATA_YET: i32 = 0x6E6F_6474;
/// Opus's largest packet (RFC 6716: 1275 bytes per frame, up to 48 frames of 2.5 ms in a
/// packet - far more than 20 ms ever needs).
const MAX_OPUS_PACKET: usize = 4000;
/// The longest Opus packet, in samples per channel at 48 kHz (120 ms).
const MAX_PACKET_FRAMES_48K: usize = 5760;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct AudioStreamBasicDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

impl AudioStreamBasicDescription {
    /// Interleaved 32-bit float PCM.
    fn float_pcm(sample_rate: u32, channels: u32) -> Self {
        AudioStreamBasicDescription {
            sample_rate: f64::from(sample_rate),
            format_id: FORMAT_LINEAR_PCM,
            format_flags: FLAGS_FLOAT_PACKED,
            bytes_per_packet: 4 * channels,
            frames_per_packet: 1,
            bytes_per_frame: 4 * channels,
            channels_per_frame: channels,
            bits_per_channel: 32,
            reserved: 0,
        }
    }

    /// Opus at 48 kHz, `frames_per_packet` samples per channel in a packet.
    fn opus(channels: u32, frames_per_packet: u32) -> Self {
        AudioStreamBasicDescription {
            sample_rate: f64::from(super::codec::OPUS_SAMPLE_RATE),
            format_id: FORMAT_OPUS,
            frames_per_packet,
            channels_per_frame: channels,
            ..AudioStreamBasicDescription::default()
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct AudioBuffer {
    number_channels: u32,
    data_byte_size: u32,
    data: *mut c_void,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct AudioBufferList {
    number_buffers: u32,
    buffers: [AudioBuffer; 1],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct AudioStreamPacketDescription {
    start_offset: i64,
    variable_frames_in_packet: u32,
    data_byte_size: u32,
}

/// `AudioConverterComplexInputDataProc`.
type InputProc = unsafe extern "C" fn(
    converter: *mut c_void,
    io_number_data_packets: *mut u32,
    io_data: *mut AudioBufferList,
    out_packet_description: *mut *mut AudioStreamPacketDescription,
    user_data: *mut c_void,
) -> i32;

/// The AudioToolbox entry points this engine uses, resolved once.
struct AtLib {
    _lib: libloading::Library,
    new: unsafe extern "C" fn(
        *const AudioStreamBasicDescription,
        *const AudioStreamBasicDescription,
        *mut *mut c_void,
    ) -> i32,
    dispose: unsafe extern "C" fn(*mut c_void) -> i32,
    fill: unsafe extern "C" fn(
        *mut c_void,
        InputProc,
        *mut c_void,
        *mut u32,
        *mut AudioBufferList,
        *mut AudioStreamPacketDescription,
    ) -> i32,
    set_property: unsafe extern "C" fn(*mut c_void, u32, u32, *const c_void) -> i32,
    get_property: unsafe extern "C" fn(*mut c_void, u32, *mut u32, *mut c_void) -> i32,
    get_property_info: unsafe extern "C" fn(*mut c_void, u32, *mut u32, *mut u8) -> i32,
}

// Function pointers and a library handle: shareable.
unsafe impl Send for AtLib {}
unsafe impl Sync for AtLib {}

static AT: OnceLock<Option<AtLib>> = OnceLock::new();

impl AtLib {
    fn get() -> Option<&'static AtLib> {
        AT.get_or_init(|| unsafe {
            let lib = match libloading::Library::new(
                "/System/Library/Frameworks/AudioToolbox.framework/AudioToolbox",
            ) {
                Ok(lib) => lib,
                Err(e) => {
                    crate::plog_warn!("[audio] Opus disabled: dlopen AudioToolbox failed: {}", e);
                    return None;
                }
            };
            macro_rules! f {
                ($sym:literal) => {
                    match lib.get($sym) {
                        Ok(s) => *s,
                        Err(_) => {
                            crate::plog_warn!(
                                "[audio] Opus disabled: missing symbol {}",
                                String::from_utf8_lossy($sym)
                            );
                            return None;
                        }
                    }
                };
            }
            let new = f!(b"AudioConverterNew\0");
            let dispose = f!(b"AudioConverterDispose\0");
            let fill = f!(b"AudioConverterFillComplexBuffer\0");
            let set_property = f!(b"AudioConverterSetProperty\0");
            let get_property = f!(b"AudioConverterGetProperty\0");
            let get_property_info = f!(b"AudioConverterGetPropertyInfo\0");
            Some(AtLib {
                _lib: lib,
                new,
                dispose,
                fill,
                set_property,
                get_property,
                get_property_info,
            })
        })
        .as_ref()
    }
}

/// Whether AudioToolbox loaded (the converter itself may still refuse Opus: `open` says).
pub(super) fn is_available() -> bool {
    AtLib::get().is_some()
}

/// A converter from `from` to `to`, or the OSStatus it was refused with.
unsafe fn new_converter(
    lib: &AtLib,
    from: &AudioStreamBasicDescription,
    to: &AudioStreamBasicDescription,
) -> Result<*mut c_void, i32> {
    let mut converter: *mut c_void = core::ptr::null_mut();
    let status = unsafe { (lib.new)(from, to, &mut converter) };
    if status != 0 || converter.is_null() {
        return Err(status);
    }
    Ok(converter)
}

/// A UInt32 property of `converter`, or `None`.
unsafe fn u32_property(lib: &AtLib, converter: *mut c_void, id: u32) -> Option<u32> {
    let mut value = 0u32;
    let mut size = core::mem::size_of::<u32>() as u32;
    let status = unsafe {
        (lib.get_property)(
            converter,
            id,
            &mut size,
            &mut value as *mut u32 as *mut c_void,
        )
    };
    (status == 0).then_some(value)
}

/// A variable-size property's bytes (a magic cookie), or `None`.
unsafe fn bytes_property(lib: &AtLib, converter: *mut c_void, id: u32) -> Option<Vec<u8>> {
    let mut size = 0u32;
    let mut writable = 0u8;
    if unsafe { (lib.get_property_info)(converter, id, &mut size, &mut writable) } != 0 || size == 0
    {
        return None;
    }
    let mut bytes = vec![0u8; size as usize];
    let status =
        unsafe { (lib.get_property)(converter, id, &mut size, bytes.as_mut_ptr() as *mut c_void) };
    if status != 0 {
        return None;
    }
    bytes.truncate(size as usize);
    Some(bytes)
}

/// The PCM an encoder's converter reads: what `encode` queued, and the buffer handed out last
/// (alive until the callback runs again).
struct PcmFeed {
    queued: Vec<f32>,
    handed: Vec<f32>,
    channels: u32,
}

/// The converter's input callback for an encoder: everything queued, as one buffer.
unsafe extern "C" fn feed_pcm(
    _converter: *mut c_void,
    io_number_data_packets: *mut u32,
    io_data: *mut AudioBufferList,
    out_packet_description: *mut *mut AudioStreamPacketDescription,
    user_data: *mut c_void,
) -> i32 {
    unsafe {
        if !out_packet_description.is_null() {
            *out_packet_description = core::ptr::null_mut();
        }
        let feed = &mut *(user_data as *mut PcmFeed);
        let channels = feed.channels.max(1) as usize;
        if feed.queued.len() < channels {
            *io_number_data_packets = 0;
            return NO_DATA_YET;
        }
        // The converter is done with the buffer handed out before: it asked for more.
        feed.handed = core::mem::take(&mut feed.queued);
        let frames = feed.handed.len() / channels;
        feed.handed.truncate(frames * channels);
        // PCM: a packet is a frame. More than the minimum asked for is fine: the converter keeps
        // the rest for its next call.
        *io_number_data_packets = frames as u32;
        (*io_data).number_buffers = 1;
        (*io_data).buffers[0] = AudioBuffer {
            number_channels: feed.channels,
            data_byte_size: (feed.handed.len() * 4) as u32,
            data: feed.handed.as_mut_ptr() as *mut c_void,
        };
        0
    }
}

/// An Opus encoder: interleaved `f32` PCM at `sample_rate` in, Opus packets out.
pub(super) struct OpusEncoderEngine {
    converter: *mut c_void,
    feed: Box<PcmFeed>,
    out: Vec<u8>,
}

impl OpusEncoderEngine {
    /// An encoder for `channels` (1 or 2) at `sample_rate`, `bitrate_kbps`, packets of
    /// `frames_per_packet` samples at 48 kHz; or why AudioToolbox refused.
    pub(super) fn open(
        sample_rate: u32,
        channels: u32,
        bitrate_kbps: u32,
        frames_per_packet: u32,
    ) -> Result<Self, String> {
        let lib = AtLib::get().ok_or_else(|| String::from("AudioToolbox did not load"))?;
        let from = AudioStreamBasicDescription::float_pcm(sample_rate, channels);
        let to = AudioStreamBasicDescription::opus(channels, frames_per_packet);
        unsafe {
            let converter = new_converter(lib, &from, &to).map_err(|status| {
                format!(
                    "AudioToolbox refused an Opus encoder ({sample_rate} Hz x{channels} -> Opus, \
                     OSStatus {status})"
                )
            })?;
            let bits = bitrate_kbps.max(6).saturating_mul(1000);
            let _ = (lib.set_property)(
                converter,
                PROPERTY_ENCODE_BIT_RATE,
                core::mem::size_of::<u32>() as u32,
                &bits as *const u32 as *const c_void,
            );
            let room = u32_property(lib, converter, PROPERTY_MAX_OUTPUT_PACKET_SIZE)
                .map_or(MAX_OPUS_PACKET, |n| (n as usize).max(MAX_OPUS_PACKET));
            crate::plog_info!(
                "[audio] Opus encoder open (AudioToolbox): {} Hz x{} at {} kbit/s",
                sample_rate,
                channels,
                bitrate_kbps
            );
            Ok(OpusEncoderEngine {
                converter,
                feed: Box::new(PcmFeed {
                    queued: Vec::new(),
                    handed: Vec::new(),
                    channels,
                }),
                out: vec![0u8; room],
            })
        }
    }

    /// The encoder's magic cookie (what a decoder of its stream may be told), if it has one.
    fn cookie(&self) -> Option<Vec<u8>> {
        let lib = AtLib::get()?;
        unsafe { bytes_property(lib, self.converter, PROPERTY_COMPRESSION_COOKIE) }
    }

    /// Queues `samples` (interleaved, whole frames) and returns every packet that is complete.
    pub(super) fn encode(&mut self, samples: &[f32]) -> Vec<Vec<u8>> {
        let Some(lib) = AtLib::get() else {
            return Vec::new();
        };
        self.feed.queued.extend_from_slice(samples);
        let mut packets = Vec::new();
        loop {
            let mut count: u32 = 1;
            let mut description = AudioStreamPacketDescription::default();
            let mut list = AudioBufferList {
                number_buffers: 1,
                buffers: [AudioBuffer {
                    number_channels: self.feed.channels,
                    data_byte_size: self.out.len() as u32,
                    data: self.out.as_mut_ptr() as *mut c_void,
                }],
            };
            let status = unsafe {
                (lib.fill)(
                    self.converter,
                    feed_pcm,
                    &mut *self.feed as *mut PcmFeed as *mut c_void,
                    &mut count,
                    &mut list,
                    &mut description,
                )
            };
            if count == 0 {
                if status != 0 && status != NO_DATA_YET {
                    crate::plog_warn!("[audio] Opus encode failed: OSStatus {}", status);
                }
                break;
            }
            let start = usize::try_from(description.start_offset).unwrap_or(0);
            let len = if description.data_byte_size > 0 {
                description.data_byte_size as usize
            } else {
                list.buffers[0].data_byte_size as usize
            };
            if let Some(packet) = self.out.get(start..start.saturating_add(len)) {
                if !packet.is_empty() {
                    packets.push(packet.to_vec());
                }
            }
            if status != 0 {
                break;
            }
        }
        packets
    }
}

impl Drop for OpusEncoderEngine {
    fn drop(&mut self) {
        if let Some(lib) = AtLib::get() {
            unsafe {
                let _ = (lib.dispose)(self.converter);
            }
        }
    }
}

/// The packet a decoder's converter reads: one, once.
struct PacketFeed {
    packet: Vec<u8>,
    description: AudioStreamPacketDescription,
    pending: bool,
    channels: u32,
}

/// The converter's input callback for a decoder: the one packet `decode` was given.
unsafe extern "C" fn feed_packet(
    _converter: *mut c_void,
    io_number_data_packets: *mut u32,
    io_data: *mut AudioBufferList,
    out_packet_description: *mut *mut AudioStreamPacketDescription,
    user_data: *mut c_void,
) -> i32 {
    unsafe {
        let feed = &mut *(user_data as *mut PacketFeed);
        if !feed.pending {
            *io_number_data_packets = 0;
            if !out_packet_description.is_null() {
                *out_packet_description = core::ptr::null_mut();
            }
            return NO_DATA_YET;
        }
        feed.pending = false;
        feed.description = AudioStreamPacketDescription {
            start_offset: 0,
            variable_frames_in_packet: 0,
            data_byte_size: feed.packet.len() as u32,
        };
        *io_number_data_packets = 1;
        (*io_data).number_buffers = 1;
        (*io_data).buffers[0] = AudioBuffer {
            number_channels: feed.channels,
            data_byte_size: feed.packet.len() as u32,
            data: feed.packet.as_mut_ptr() as *mut c_void,
        };
        if !out_packet_description.is_null() {
            *out_packet_description = &mut feed.description;
        }
        0
    }
}

/// An Opus decoder: Opus packets in, interleaved `f32` PCM at `sample_rate` out.
pub(super) struct OpusDecoderEngine {
    converter: *mut c_void,
    feed: Box<PacketFeed>,
    out: Vec<f32>,
    channels: u32,
}

impl OpusDecoderEngine {
    /// A decoder of `channels` (1 or 2) Opus with packets of `frames_per_packet` samples at
    /// 48 kHz, handing PCM out at `sample_rate`; or why AudioToolbox refused.
    pub(super) fn open(
        sample_rate: u32,
        channels: u32,
        frames_per_packet: u32,
    ) -> Result<Self, String> {
        let lib = AtLib::get().ok_or_else(|| String::from("AudioToolbox did not load"))?;
        let from = AudioStreamBasicDescription::opus(channels, frames_per_packet);
        let to = AudioStreamBasicDescription::float_pcm(sample_rate, channels);
        unsafe {
            let converter = new_converter(lib, &from, &to).map_err(|status| {
                format!(
                    "AudioToolbox refused an Opus decoder (Opus x{channels} -> {sample_rate} Hz, \
                     OSStatus {status})"
                )
            })?;
            // Every AzMeet sender's stream has the same header (channels, 48 kHz, no gain): the
            // cookie of an encoder made here for the same format describes it. Told where the
            // decoder wants one; ignored where it does not.
            let cookie = OpusEncoderEngine::open(
                super::codec::OPUS_SAMPLE_RATE,
                channels,
                32,
                frames_per_packet,
            )
            .ok()
            .and_then(|encoder| encoder.cookie());
            if let Some(cookie) = cookie {
                let _ = (lib.set_property)(
                    converter,
                    PROPERTY_DECOMPRESSION_COOKIE,
                    cookie.len() as u32,
                    cookie.as_ptr() as *const c_void,
                );
            }
            // Room for the longest Opus packet, resampled to the output rate.
            let frames = MAX_PACKET_FRAMES_48K * sample_rate.max(1) as usize
                / super::codec::OPUS_SAMPLE_RATE as usize
                + 64;
            crate::plog_info!(
                "[audio] Opus decoder open (AudioToolbox): {} Hz x{}",
                sample_rate,
                channels
            );
            Ok(OpusDecoderEngine {
                converter,
                feed: Box::new(PacketFeed {
                    packet: Vec::new(),
                    description: AudioStreamPacketDescription::default(),
                    pending: false,
                    channels,
                }),
                out: vec![0.0f32; frames * channels as usize],
                channels,
            })
        }
    }

    /// The samples (interleaved) `packet` decodes to - possibly none for the first packet, which
    /// the decoder may keep for its look-ahead; `None` when the converter failed on it.
    pub(super) fn decode(&mut self, packet: &[u8]) -> Option<Vec<f32>> {
        let lib = AtLib::get()?;
        self.feed.packet.clear();
        self.feed.packet.extend_from_slice(packet);
        self.feed.pending = true;
        let channels = self.channels.max(1) as usize;
        let mut samples = Vec::new();
        loop {
            let room = self.out.len() / channels;
            let mut count = room as u32;
            let mut list = AudioBufferList {
                number_buffers: 1,
                buffers: [AudioBuffer {
                    number_channels: self.channels,
                    data_byte_size: (self.out.len() * 4) as u32,
                    data: self.out.as_mut_ptr() as *mut c_void,
                }],
            };
            let status = unsafe {
                (lib.fill)(
                    self.converter,
                    feed_packet,
                    &mut *self.feed as *mut PacketFeed as *mut c_void,
                    &mut count,
                    &mut list,
                    core::ptr::null_mut(),
                )
            };
            let frames = (count as usize).min(room);
            samples.extend_from_slice(&self.out[..frames * channels]);
            if status != 0 && status != NO_DATA_YET {
                crate::plog_warn!("[audio] Opus decode failed: OSStatus {}", status);
                self.feed.pending = false;
                return None;
            }
            // Ran dry (the packet is in), or the buffer was not filled: done.
            if status == NO_DATA_YET || frames < room {
                break;
            }
        }
        Some(samples)
    }
}

impl Drop for OpusDecoderEngine {
    fn drop(&mut self) {
        if let Some(lib) = AtLib::get() {
            unsafe {
                let _ = (lib.dispose)(self.converter);
            }
        }
    }
}
