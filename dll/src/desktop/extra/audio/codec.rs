//! Opus voice coding - the `AudioEncoder` / `AudioDecoder` handles.
//!
//! A call sends its microphone as 20 ms packets. As 16-bit PCM that is 768 kbit/s for one mono
//! 48 kHz voice (AzMeet sends every packet three times on the latest-wins frame path); as Opus it
//! is about 32 kbit/s, with the codec's own loss concealment behind it. These handles turn
//! [`AudioFrame`]s into Opus packets and back.
//!
//! **Native per platform**, like `VideoEncoder` (`video_codec/mod.rs`): the codec is whatever the
//! platform ships - Apple: **AudioToolbox**'s Opus `AudioConverter` (dlopen'd, no build-time
//! link; see `opus_apple.rs`); elsewhere none yet. The handles are honest: `create` hands out a
//! CLOSED handle (`is_open()` false) wherever no engine opens, says why on stderr once, and a
//! closed handle takes nothing and gives nothing back - so an app keeps its PCM path there.
//!
//! Packets are raw Opus packets (RFC 6716), one per [`OPUS_FRAME_MS`] of audio, at 48 kHz (Opus's
//! own rate). An encoder takes frames at its config's rate and converts (AudioConverter
//! resamples); a decoder hands frames out at its config's rate.

use core::ffi::c_void;

use azul_core::audio::{AudioConfig, AudioFrame, OptionAudioFrame};
use azul_css::{corety::OptionU8Vec, AzString, U8Vec};

/// Milliseconds of audio in one Opus packet.
pub const OPUS_FRAME_MS: u32 = 20;
/// Opus's own sample rate: packets always carry 48 kHz audio.
pub const OPUS_SAMPLE_RATE: u32 = 48_000;

/// The Opus engine of this build on this machine, or why there is none.
fn engine() -> Result<&'static str, String> {
    Err(String::from("this build has no Opus engine"))
}

/// Says why a codec handle did not open: once per distinct reason.
fn say_not_open(what: &str, why: &str) {
    static SAID: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let line = format!("{what}: {why}");
    let mut said = SAID
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !said.contains(&line) {
        eprintln!("[azul][audio] {line} - the handle is closed (is_open() = false)");
        said.push(line);
    }
}

/// An Opus encoder handle: [`AudioFrame`]s in, Opus packets out.
#[repr(C)]
pub struct AudioEncoder {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

impl Clone for AudioEncoder {
    fn clone(&self) -> Self {
        AudioEncoder {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

impl Default for AudioEncoder {
    fn default() -> Self {
        AudioEncoder {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl AudioEncoder {
    /// An Opus encoder for frames in `config` (sample rate, 1 or 2 channels) at `bitrate_kbps`
    /// (32 is good for one voice). A closed handle wherever this build has no Opus engine.
    pub fn create(config: AudioConfig, bitrate_kbps: u32) -> AudioEncoder {
        let _ = (config, bitrate_kbps);
        if let Err(why) = engine() {
            say_not_open("AudioEncoder::create", &why);
        }
        AudioEncoder::default()
    }

    /// The engine this build would use ("AudioToolbox", or "none").
    pub fn backend_name() -> AzString {
        AzString::from_const_str("none")
    }

    /// Whether the encoder opened.
    pub fn is_open(&self) -> bool {
        !self.ptr.is_null()
    }

    /// Hands `frame` (interleaved `f32`, in the config's rate and channels) to the encoder.
    /// Every [`OPUS_FRAME_MS`] of audio it has taken comes out as one packet
    /// ([`recv_packet`](Self::recv_packet)). False when the encoder is not open or the frame is
    /// in another format.
    pub fn encode(&mut self, frame: AudioFrame) -> bool {
        let _ = frame;
        false
    }

    /// The next Opus packet, or `None` when no whole packet is ready yet.
    pub fn recv_packet(&mut self) -> OptionU8Vec {
        OptionU8Vec::None
    }

    /// Release the encoder. (Drop does this too.)
    pub fn close(&mut self) {
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl Drop for AudioEncoder {
    fn drop(&mut self) {
        self.close();
    }
}

/// An Opus decoder handle: Opus packets in, [`AudioFrame`]s out.
#[repr(C)]
pub struct AudioDecoder {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

impl Clone for AudioDecoder {
    fn clone(&self) -> Self {
        AudioDecoder {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

impl Default for AudioDecoder {
    fn default() -> Self {
        AudioDecoder {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl AudioDecoder {
    /// An Opus decoder that hands frames out in `config` (the output's sample rate, 1 or 2
    /// channels). A closed handle wherever this build has no Opus engine.
    pub fn create(config: AudioConfig) -> AudioDecoder {
        let _ = config;
        if let Err(why) = engine() {
            say_not_open("AudioDecoder::create", &why);
        }
        AudioDecoder::default()
    }

    /// Whether the decoder opened.
    pub fn is_open(&self) -> bool {
        !self.ptr.is_null()
    }

    /// Decodes one Opus packet into its audio (20 ms for AzMeet's packets), in the config's
    /// rate and channels. `None` when the decoder is not open or the packet does not decode.
    pub fn decode(&mut self, packet: U8Vec) -> OptionAudioFrame {
        let _ = packet;
        OptionAudioFrame::None
    }

    /// Release the decoder. (Drop does this too.)
    pub fn close(&mut self) {
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl Drop for AudioDecoder {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod opus_tests {
    use azul_core::audio::{AudioConfig, AudioFrame};
    use azul_css::{corety::OptionU8Vec, F32Vec, U8Vec};

    use super::{AudioDecoder, AudioEncoder, OPUS_FRAME_MS};

    const RATE: u32 = 48_000;
    /// Samples in one 20 ms packet at 48 kHz.
    const PACKET: usize = (RATE * OPUS_FRAME_MS / 1000) as usize;
    const TONE_HZ: f32 = 440.0;
    const AMPLITUDE: f32 = 0.3;

    /// 20 ms number `n` of a steady 440 Hz tone, mono 48 kHz.
    fn tone_packet(n: usize) -> AudioFrame {
        let samples: Vec<f32> = (0..PACKET)
            .map(|i| {
                let t = (n * PACKET + i) as f32 / RATE as f32;
                AMPLITUDE * (2.0 * core::f32::consts::PI * TONE_HZ * t).sin()
            })
            .collect();
        AudioFrame {
            sample_rate: RATE,
            channels: 1,
            samples: F32Vec::from_vec(samples),
        }
    }

    /// The share of `block`'s power at `hz` (Goertzel at an exact bin: `block.len()` must hold
    /// a whole number of cycles), 1.0 for a pure tone at `hz`.
    fn tone_share(block: &[f32], hz: f32) -> f32 {
        let n = block.len() as f32;
        let w = 2.0 * core::f32::consts::PI * hz / RATE as f32;
        let coeff = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for x in block {
            let s0 = x + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        let at_hz = s1 * s1 + s2 * s2 - coeff * s1 * s2;
        let energy: f32 = block.iter().map(|x| x * x).sum();
        if energy <= 0.0 {
            return 0.0;
        }
        at_hz / (n / 2.0 * energy)
    }

    /// One second of a voice-like tone goes through Opus and back: every 20 ms is one small
    /// packet (a few dozen bytes, not the 1920 of 16-bit PCM), and what the decoder gives back is
    /// that tone at that loudness. Where this build has no Opus engine the handles are closed.
    #[test]
    fn a_tone_goes_through_opus_in_small_packets_and_comes_back_the_same_tone() {
        let config = AudioConfig::new(RATE, 1);
        let mut encoder = AudioEncoder::create(config, 32);
        let mut decoder = AudioDecoder::create(config);
        if cfg!(not(any(target_os = "macos", target_os = "ios"))) {
            assert!(
                !encoder.is_open() && !decoder.is_open(),
                "no Opus engine off Apple yet"
            );
            return;
        }
        assert!(
            encoder.is_open(),
            "AudioToolbox's Opus encoder opens on this Mac"
        );
        assert!(
            decoder.is_open(),
            "AudioToolbox's Opus decoder opens on this Mac"
        );

        let mut packets: Vec<Vec<u8>> = Vec::new();
        for n in 0..50 {
            assert!(
                encoder.encode(tone_packet(n)),
                "the encoder takes 20 ms of mono 48 kHz"
            );
            while let OptionU8Vec::Some(packet) = encoder.recv_packet() {
                packets.push(packet.as_ref().to_vec());
            }
        }
        // The encoder may hold a packet back for its look-ahead: one second in, at least 45 out.
        assert!(
            packets.len() >= 45,
            "{} packets for 50 x 20 ms",
            packets.len()
        );
        let largest = packets.iter().map(Vec::len).max().unwrap_or(0);
        let total: usize = packets.iter().map(Vec::len).sum();
        eprintln!(
            "Opus: {} packets, {total} bytes, largest {largest} (PCM16 would be {} bytes)",
            packets.len(),
            packets.len() * PACKET * 2
        );
        assert!(largest <= 200, "a 20 ms packet of {largest} bytes");
        assert!(
            total <= 8000,
            "one second of voice in {total} bytes (64 kbit/s or less)"
        );

        let mut decoded: Vec<f32> = Vec::new();
        for packet in &packets {
            let frame = decoder
                .decode(U8Vec::from_vec(packet.clone()))
                .into_option()
                .expect("an Opus packet the encoder made decodes");
            assert_eq!((frame.sample_rate, frame.channels), (RATE, 1));
            decoded.extend_from_slice(frame.samples.as_ref());
        }
        assert!(
            decoded.len() >= 45 * PACKET,
            "{} samples decoded from {} packets",
            decoded.len(),
            packets.len()
        );
        // Past the codec's start-up (100 ms): blocks of 1200 samples (25 ms, eleven whole cycles
        // of 440 Hz) are that tone, at its loudness.
        let steady = &decoded[RATE as usize / 10..];
        let blocks: Vec<&[f32]> = steady.chunks_exact(1200).collect();
        assert!(blocks.len() >= 20);
        let share =
            blocks.iter().map(|b| tone_share(b, TONE_HZ)).sum::<f32>() / blocks.len() as f32;
        let rms = (steady.iter().map(|x| x * x).sum::<f32>() / steady.len() as f32).sqrt();
        let want_rms = AMPLITUDE / core::f32::consts::SQRT_2;
        eprintln!(
            "decoded: {:.3} of the power at 440 Hz, rms {rms:.3} (sent {want_rms:.3})",
            share
        );
        assert!(
            share >= 0.8,
            "only {share:.3} of the decoded power is the 440 Hz tone"
        );
        assert!(
            rms >= want_rms * 0.5 && rms <= want_rms * 1.5,
            "decoded rms {rms:.3}, sent {want_rms:.3}"
        );
    }

    /// A closed handle (a default one, or one off Apple) takes nothing and gives nothing.
    #[test]
    fn a_closed_codec_handle_takes_nothing_and_gives_nothing() {
        let mut encoder = AudioEncoder::default();
        assert!(!encoder.is_open());
        assert!(!encoder.encode(tone_packet(0)));
        assert!(encoder.recv_packet().into_option().is_none());
        let mut decoder = AudioDecoder::default();
        assert!(!decoder.is_open());
        assert!(decoder
            .decode(U8Vec::from_vec(vec![0xfc, 0xff, 0xfe]))
            .into_option()
            .is_none());
    }

    /// Frames in another format than the encoder was made for are refused, not misread.
    #[test]
    fn an_encoder_refuses_frames_in_another_format() {
        let mut encoder = AudioEncoder::create(AudioConfig::new(RATE, 1), 32);
        if !encoder.is_open() {
            return;
        }
        let mut stereo = tone_packet(0);
        stereo.channels = 2;
        assert!(
            !encoder.encode(stereo),
            "a stereo frame into a mono encoder"
        );
        let mut other_rate = tone_packet(0);
        other_rate.sample_rate = 44_100;
        assert!(
            !encoder.encode(other_rate),
            "a 44.1 kHz frame into a 48 kHz encoder"
        );
    }
}
