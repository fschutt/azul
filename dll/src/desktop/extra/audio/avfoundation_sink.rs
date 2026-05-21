//! iOS audio playback via objc2 AVAudioEngine (cpal can't cross-compile to
//! iOS). An `AVAudioPlayerNode` feeds the engine's main mixer; `play` wraps each
//! interleaved-f32 frame in an `AVAudioPCMBuffer` and schedules it (no
//! completion handler). Counterpart to `avfoundation_mic` (capture).

use objc2::rc::Retained;
use objc2::AllocAnyThread;
use objc2_avf_audio::{
    AVAudioCommonFormat, AVAudioEngine, AVAudioFormat, AVAudioPCMBuffer, AVAudioPlayerNode,
};

/// An open AVAudioEngine playback graph for `AudioSink::play`.
pub struct AvfSink {
    engine: Retained<AVAudioEngine>,
    player: Retained<AVAudioPlayerNode>,
    format: Retained<AVAudioFormat>,
    channels: u16,
}

// Single-threaded use assumed (same assertion as the cpal/AAudio sinks).
unsafe impl Send for AvfSink {}
unsafe impl Sync for AvfSink {}

impl AvfSink {
    /// Build + start an engine with a player node connected to the main mixer,
    /// using an interleaved Float32 format. `None` on failure.
    pub fn open(rate: u32, channels: u16) -> Option<AvfSink> {
        let ch = channels.max(1) as u32;
        let sample_rate = if rate == 0 { 48_000.0 } else { rate as f64 };
        unsafe {
            let format = AVAudioFormat::initWithCommonFormat_sampleRate_channels_interleaved(
                AVAudioFormat::alloc(),
                AVAudioCommonFormat::PCMFormatFloat32,
                sample_rate,
                ch,
                true,
            )?;
            let engine = AVAudioEngine::new();
            let player = AVAudioPlayerNode::new();
            engine.attachNode(&player);
            let mixer = engine.mainMixerNode();
            engine.connect_to_format(&player, &mixer, Some(&format));
            engine.prepare();
            if engine.startAndReturnError().is_err() {
                return None;
            }
            player.play();
            Some(AvfSink {
                engine,
                player,
                format,
                channels: channels.max(1),
            })
        }
    }

    /// Wrap `samples` (interleaved f32) in a PCM buffer + schedule it.
    pub fn play(&self, samples: &[f32]) {
        let frames = (samples.len() / self.channels.max(1) as usize) as u32;
        if frames == 0 {
            return;
        }
        unsafe {
            let buf = match AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(
                AVAudioPCMBuffer::alloc(),
                &self.format,
                frames,
            ) {
                Some(b) => b,
                None => return,
            };
            buf.setFrameLength(frames);
            let data = buf.floatChannelData();
            if !data.is_null() {
                // Interleaved format -> channel 0 holds all interleaved samples.
                std::ptr::copy_nonoverlapping(samples.as_ptr(), (*data).as_ptr(), samples.len());
            }
            self.player.scheduleBuffer_completionHandler(&buf, std::ptr::null_mut());
        }
    }
}

impl Drop for AvfSink {
    fn drop(&mut self) {
        unsafe { self.engine.stop() };
    }
}
