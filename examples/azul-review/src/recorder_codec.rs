//! What encodes the recorder's pictures and sound, as traits, and the stand-ins the tests and the
//! headless e2e use in place of the hardware.
//!
//! The recorder never names an encoder. A picture goes to a [`VideoCodec`] (azul's `VideoEncoder`:
//! VideoToolbox on a Mac; its adapter is in `recorder_azul.rs`), sound to an [`AudioCodec`] (azul's
//! Opus `AudioEncoder`), and a [`CodecFactory`] opens them when a track starts and when a picture changes
//! its size. The mocks here make streams a muxer and a decoder accept in form: a keyframe carries a
//! real sequence parameter set for the frame's size, deltas are single slices, packets of sound are
//! Opus packets of 20 ms: scripted delays and refusals included, so a slow or broken encoder can be
//! tested without one.

use std::collections::VecDeque;

/// How a picture's pixels are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Four bytes a pixel, red first (what the CPU renderer's frame is).
    Rgba8,
    /// Four bytes a pixel, blue first.
    Bgra8,
    /// A plane of luma and a plane of interleaved chroma (what a camera hands out).
    Nv12,
}

/// One picture on its way to an encoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawFrame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub bytes: Vec<u8>,
}

/// An H.264 encoder.
pub trait VideoCodec {
    /// Hands a picture over without waiting; `false` when the encoder did not take it (it is behind:
    /// a live encoder drops rather than queue).
    fn submit(&mut self, frame: RawFrame, keyframe: bool) -> bool;

    /// The next encoded access unit (Annex-B), if one is ready: in the order the pictures were taken.
    fn poll(&mut self) -> Option<Vec<u8>>;

    /// Waits until everything submitted is encoded (before the encoder is closed).
    fn flush(&mut self);

    /// What the encoder is, for the manifest.
    fn name(&self) -> String;

    /// Whether it runs on the machine's video hardware.
    fn hardware(&self) -> bool;
}

/// An Opus encoder.
pub trait AudioCodec {
    /// Hands over interleaved samples; `false` when they were not taken.
    fn submit(&mut self, samples: &[f32]) -> bool;

    /// The next packet of 20 ms, if one is ready.
    fn poll(&mut self) -> Option<Vec<u8>>;

    /// What the encoder is, for the manifest.
    fn name(&self) -> String;
}

/// Opens encoders.
pub trait CodecFactory {
    /// An H.264 encoder for pictures of `width` x `height` at `kbps`.
    ///
    /// # Errors
    /// Why none opens here, in words.
    fn video(&self, width: u32, height: u32, kbps: u32) -> Result<Box<dyn VideoCodec>, String>;

    /// An Opus encoder for `channels` channels at `sample_rate` Hz and `kbps`.
    ///
    /// # Errors
    /// Why none opens here, in words.
    fn audio(&self, sample_rate: u32, channels: u16, kbps: u32) -> Result<Box<dyn AudioCodec>, String>;
}

// ---------------------------------------------------------------------------
// Stand-ins
// ---------------------------------------------------------------------------

/// A bit writer for the made-up sequence parameter sets.
#[derive(Default)]
struct Bits(Vec<bool>);

impl Bits {
    fn u(&mut self, n: u32, v: u32) -> &mut Self {
        for i in (0..n).rev() {
            self.0.push((v >> i) & 1 == 1);
        }
        self
    }

    fn ue(&mut self, v: u32) -> &mut Self {
        let x = v + 1;
        let len = 32 - x.leading_zeros();
        for _ in 1..len {
            self.0.push(false);
        }
        self.u(len, x)
    }

    fn nal(&self, header: u8, profile: u8, level: u8) -> Vec<u8> {
        let mut bits = self.0.clone();
        bits.push(true);
        while bits.len() % 8 != 0 {
            bits.push(false);
        }
        let rbsp: Vec<u8> = bits.chunks(8).map(|c| c.iter().fold(0_u8, |a, b| (a << 1) | u8::from(*b))).collect();
        let mut out = vec![header, profile, 0, level];
        let mut zeros = 0;
        for b in rbsp {
            if zeros >= 2 && b <= 3 {
                out.push(3);
                zeros = 0;
            }
            zeros = if b == 0 { zeros + 1 } else { 0 };
            out.push(b);
        }
        out
    }
}

/// A sequence parameter set (a whole NAL unit) for a picture of `width` x `height`: High profile,
/// 4:2:0, 8 bit, cropped to the exact size. What a decoder needs to know the size; nothing more.
#[must_use]
pub fn synthetic_sps(width: u32, height: u32) -> Vec<u8> {
    let mut b = Bits::default();
    b.ue(0); // seq_parameter_set_id
    b.ue(1).ue(0).ue(0).u(1, 0).u(1, 0); // 4:2:0, 8 bit, no bypass, no scaling matrix
    b.ue(0).ue(2); // log2_max_frame_num, pic_order_cnt_type 2
    b.ue(1).u(1, 0); // max_num_ref_frames, gaps
    let (mbs_w, mbs_h) = (width.div_ceil(16), height.div_ceil(16));
    b.ue(mbs_w - 1).ue(mbs_h - 1).u(1, 1).u(1, 1); // frame_mbs_only, direct_8x8
    let (pad_w, pad_h) = (mbs_w * 16 - width, mbs_h * 16 - height);
    if pad_w > 0 || pad_h > 0 {
        b.u(1, 1).ue(0).ue(pad_w / 2).ue(0).ue(pad_h / 2);
    } else {
        b.u(1, 0);
    }
    b.u(1, 0); // no VUI
    b.nal(0x67, 100, 40)
}

/// A picture parameter set of the kind the encoders here write.
#[must_use]
pub fn synthetic_pps() -> Vec<u8> {
    vec![0x68, 0xee, 0x3c, 0xb0]
}

/// How a [`MockVideoCodec`] behaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoScript {
    /// Packets come out this many pictures after the picture they belong to.
    pub latency: usize,
    /// Every this-many-th submitted picture is refused (0: none).
    pub refuse_every: usize,
    /// It reports itself as hardware.
    pub hardware: bool,
    /// How many of the latest pictures it keeps for a test to look at (a long soak keeps none).
    pub keep: usize,
}

impl Default for VideoScript {
    fn default() -> Self {
        Self {
            latency: 0,
            refuse_every: 0,
            hardware: true,
            keep: usize::MAX,
        }
    }
}

/// A stand-in H.264 encoder. The access unit it makes for picture `n` is a keyframe (parameter sets
/// and an IDR slice) or a delta (a slice) whose bytes name `n`, so a test can follow a picture
/// through the recorder to the file.
pub struct MockVideoCodec {
    width: u32,
    height: u32,
    script: VideoScript,
    submitted: usize,
    /// The pictures taken so far (the numbers of the packets), whether kept or not.
    accepted: usize,
    ready: VecDeque<Vec<u8>>,
    waiting: VecDeque<Vec<u8>>,
    /// The latest pictures taken (as many as the script keeps), newest last, for the test to look at.
    pub taken: Vec<RawFrame>,
}

impl MockVideoCodec {
    /// An encoder for `width` x `height` that behaves as `script` says.
    #[must_use]
    pub fn new(width: u32, height: u32, script: VideoScript) -> Self {
        Self {
            width,
            height,
            script,
            submitted: 0,
            accepted: 0,
            ready: VecDeque::new(),
            waiting: VecDeque::new(),
            taken: Vec::new(),
        }
    }

    fn unit(&self, n: usize, keyframe: bool) -> Vec<u8> {
        let tag = u8::try_from(n % 251).unwrap_or(0);
        let mut out = Vec::new();
        let mut nal = |bytes: &[u8]| {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(bytes);
        };
        if keyframe {
            nal(&synthetic_sps(self.width, self.height));
            nal(&synthetic_pps());
            nal(&[0x65, tag, 1, 2, 3, 4, 5, 6]);
        } else {
            nal(&[0x41, tag, 9, 9, 9, 9, 9, 9]);
        }
        out
    }
}

impl VideoCodec for MockVideoCodec {
    fn submit(&mut self, frame: RawFrame, keyframe: bool) -> bool {
        self.submitted += 1;
        if self.script.refuse_every > 0 && self.submitted % self.script.refuse_every == 0 {
            return false;
        }
        // The first picture of a stream is a keyframe whatever was asked.
        let keyframe = keyframe || self.accepted == 0;
        let unit = self.unit(self.accepted, keyframe);
        self.accepted += 1;
        self.taken.push(frame);
        let extra = self.taken.len().saturating_sub(self.script.keep);
        self.taken.drain(..extra);
        self.waiting.push_back(unit);
        while self.waiting.len() > self.script.latency {
            if let Some(u) = self.waiting.pop_front() {
                self.ready.push_back(u);
            }
        }
        true
    }

    fn poll(&mut self) -> Option<Vec<u8>> {
        self.ready.pop_front()
    }

    fn flush(&mut self) {
        while let Some(u) = self.waiting.pop_front() {
            self.ready.push_back(u);
        }
    }

    fn name(&self) -> String {
        String::from("mock H.264")
    }

    fn hardware(&self) -> bool {
        self.script.hardware
    }
}

/// A stand-in Opus encoder: a packet for every 960 samples of one channel (20 ms at 48 kHz).
pub struct MockAudioCodec {
    channels: usize,
    pending: Vec<f32>,
    made: u64,
    ready: VecDeque<Vec<u8>>,
}

impl MockAudioCodec {
    #[must_use]
    pub fn new(channels: u16) -> Self {
        Self {
            channels: usize::from(channels.max(1)),
            pending: Vec::new(),
            made: 0,
            ready: VecDeque::new(),
        }
    }
}

impl AudioCodec for MockAudioCodec {
    fn submit(&mut self, samples: &[f32]) -> bool {
        self.pending.extend_from_slice(samples);
        let packet = 960 * self.channels;
        while self.pending.len() >= packet {
            self.pending.drain(..packet);
            self.made += 1;
            // 0xf8: CELT full band, 20 ms, one frame; then a count that names the packet.
            self.ready.push_back(vec![0xf8, u8::try_from(self.made % 251).unwrap_or(0), 1, 2, 3, 4]);
        }
        true
    }

    fn poll(&mut self) -> Option<Vec<u8>> {
        self.ready.pop_front()
    }

    fn name(&self) -> String {
        String::from("mock Opus")
    }
}

/// A factory of stand-ins; it can be told to fail.
#[derive(Debug, Clone, Copy, Default)]
pub struct MockFactory {
    pub script: VideoScript,
    /// No video encoder opens.
    pub no_video: bool,
    /// No audio encoder opens.
    pub no_audio: bool,
}

impl CodecFactory for MockFactory {
    fn video(&self, width: u32, height: u32, _kbps: u32) -> Result<Box<dyn VideoCodec>, String> {
        if self.no_video {
            return Err(String::from("no H.264 encoder on this machine"));
        }
        Ok(Box::new(MockVideoCodec::new(width, height, self.script)))
    }

    fn audio(&self, _sample_rate: u32, channels: u16, _kbps: u32) -> Result<Box<dyn AudioCodec>, String> {
        if self.no_audio {
            return Err(String::from("no Opus encoder on this machine"));
        }
        Ok(Box::new(MockAudioCodec::new(channels)))
    }
}

#[cfg(test)]
#[path = "recorder_codec_test.rs"]
mod recorder_codec_test;
