//! The sound while playing: the clips on the unmuted audio tracks, mixed at the playhead and
//! handed to azul's `AudioSink` a short lead ahead of the listener.
//!
//! Each clip that sounds ([`spans`]) gets a voice when the playhead reaches it: its file's
//! decoder, sought to the second the clip plays, resampled to the output's rate (linear, which
//! is plenty for listening while editing) and remixed to stereo. The voices are summed and kept
//! inside full scale. Only playing forward at normal speed sounds: a shuttle (2x, 4x, reverse)
//! would need time stretching to be worth hearing.

use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc,
};

use azul::{
    audio::{AudioConfig, AudioFileDecoder, AudioFrame, AudioSink},
    str::String as AzString,
    vec::{F32Vec, U8Vec},
};
use azul_storage::Drive;

use crate::{
    decode::MediaFiles,
    model::{Frame, MediaItem, MediaSource, Project, TrackKind},
};

/// The output's rate (samples per second per channel); stereo.
pub const RATE: u32 = 48_000;

/// One block handed to the output: 20 ms.
pub const BLOCK: usize = 960;

/// How far ahead of the listener the output is kept, in samples per channel: 100 ms, so a
/// stop is heard at once.
const LEAD: u64 = 4_800;

/// A clip that sounds, in seconds of the sequence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub media: u64,
    /// When it starts and ends on the timeline.
    pub start_s: f64,
    pub end_s: f64,
    /// The second of its file it starts with.
    pub source_s: f64,
}

/// The clips that sound in `project`: every enabled clip on an unmuted audio track.
#[must_use]
pub fn spans(project: &Project) -> Vec<Span> {
    let fps = f64::from(project.sequence.fps.max(1));
    #[allow(clippy::cast_precision_loss)]
    let seconds = |f: Frame| f as f64 / fps;
    project
        .sequence
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio && !track.hidden)
        .flat_map(|track| track.clips.iter())
        .filter(|clip| clip.enabled)
        .map(|clip| Span {
            media: clip.media,
            start_s: seconds(clip.start),
            end_s: seconds(clip.start + clip.length),
            source_s: seconds(clip.source_in),
        })
        .collect()
}

/// Where a voice's samples come from: a file's decoder, or a test's tone.
pub trait Source {
    /// Goes to `seconds` into the file; false when it cannot.
    fn seek(&mut self, seconds: f64) -> bool;
    /// The next decoded piece: its rate, its channel count and its interleaved samples.
    fn next(&mut self) -> Option<(u32, u16, Vec<f32>)>;
}

impl Source for AudioFileDecoder {
    fn seek(&mut self, seconds: f64) -> bool {
        AudioFileDecoder::seek(self, seconds)
    }

    fn next(&mut self) -> Option<(u32, u16, Vec<f32>)> {
        let frame = self.next_frame().into_option()?;
        Some((frame.sample_rate, frame.channels, frame.samples.as_ref().to_vec()))
    }
}

/// A clip being heard: its source, what was decoded of it as stereo pairs, and where the
/// output is in that (in samples of the source's rate, with a fraction).
struct Voice {
    source: Box<dyn Source>,
    rate: u32,
    pairs: Vec<[f32; 2]>,
    pos: f64,
    ended: bool,
}

impl Voice {
    fn new(mut source: Box<dyn Source>, seconds: f64) -> Self {
        let ended = !source.seek(seconds.max(0.0));
        Self {
            source,
            rate: 0,
            pairs: Vec::new(),
            pos: 0.0,
            ended,
        }
    }

    /// Decodes until the pair after `index` is there (or the file ends).
    fn fill(&mut self, index: usize) {
        while !self.ended && self.pairs.len() <= index + 1 {
            match self.source.next() {
                Some((rate, channels, samples)) => {
                    if self.rate == 0 {
                        self.rate = rate.max(1);
                    }
                    let channels = usize::from(channels.max(1));
                    self.pairs.extend(samples.chunks_exact(channels).map(|s| {
                        if channels == 1 {
                            [s[0], s[0]]
                        } else {
                            [s[0], s[1]]
                        }
                    }));
                }
                None => self.ended = true,
            }
        }
    }

    /// The next output sample, and the voice moved on by one output sample.
    fn sample(&mut self) -> [f32; 2] {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = self.pos.floor() as usize;
        self.fill(index);
        let Some(a) = self.pairs.get(index).copied() else {
            return [0.0; 2];
        };
        let b = self.pairs.get(index + 1).copied().unwrap_or(a);
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let frac = (self.pos - index as f64) as f32;
        self.pos += f64::from(self.rate.max(1)) / f64::from(RATE);
        // What was played is let go now and then, so a long clip does not stay in memory.
        if index > 8_192 {
            self.pairs.drain(..index);
            #[allow(clippy::cast_precision_loss)]
            let back = index as f64;
            self.pos -= back;
        }
        [a[0] + (b[0] - a[0]) * frac, a[1] + (b[1] - a[1]) * frac]
    }
}

/// The sequence's sound from a time on, block after block.
pub struct Mixer<F: FnMut(u64) -> Option<Box<dyn Source>>> {
    spans: Vec<Span>,
    voices: Vec<Option<Voice>>,
    /// Whether a span's voice was opened (a file that cannot be opened is not tried again).
    opened: Vec<bool>,
    open: F,
    /// The time of the next output sample, in samples of [`RATE`] from the sequence's start.
    at: u64,
}

impl<F: FnMut(u64) -> Option<Box<dyn Source>>> Mixer<F> {
    /// The mix of `spans` from `start_s` on; `open` gives a media item's source.
    pub fn new(spans: Vec<Span>, start_s: f64, open: F) -> Self {
        let n = spans.len();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let at = (start_s.max(0.0) * f64::from(RATE)).round() as u64;
        Self {
            spans,
            voices: (0..n).map(|_| None).collect(),
            opened: vec![false; n],
            open,
            at,
        }
    }

    /// The next `n` samples per channel, interleaved stereo, inside full scale.
    pub fn block(&mut self, n: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(n * 2);
        for _ in 0..n {
            #[allow(clippy::cast_precision_loss)]
            let t = self.at as f64 / f64::from(RATE);
            let mut sum = [0.0_f32; 2];
            for (k, span) in self.spans.iter().enumerate() {
                if t < span.start_s || t >= span.end_s {
                    // Past its end: let the file go.
                    if t >= span.end_s {
                        self.voices[k] = None;
                    }
                    continue;
                }
                if !self.opened[k] {
                    self.opened[k] = true;
                    self.voices[k] = (self.open)(span.media)
                        .map(|source| Voice::new(source, span.source_s + (t - span.start_s)));
                }
                if let Some(voice) = self.voices[k].as_mut() {
                    let s = voice.sample();
                    sum[0] += s[0];
                    sum[1] += s[1];
                }
            }
            out.push(sum[0].clamp(-1.0, 1.0));
            out.push(sum[1].clamp(-1.0, 1.0));
            self.at += 1;
        }
        out
    }

    /// Whether every clip is behind the mix (nothing more will sound).
    #[must_use]
    pub fn finished(&self) -> bool {
        #[allow(clippy::cast_precision_loss)]
        let t = self.at as f64 / f64::from(RATE);
        self.spans.iter().all(|s| t >= s.end_s)
    }
}

/// A media item's decoder (its path, or its bytes from the drive); why not, else.
///
/// # Errors
///
/// A generated item, a file that cannot be read or decoded.
pub fn open_decoder(
    media: &MediaItem,
    drive: &dyn Drive,
    files: &MediaFiles,
) -> Result<AudioFileDecoder, String> {
    let decoder = match &media.source {
        MediaSource::Path { path } => AudioFileDecoder::open(AzString::from(path.as_str())),
        MediaSource::Stored { key } => {
            let extension = key.rsplit_once('.').map_or("", |(_, ext)| ext).to_string();
            let bytes = files.bytes(media, drive)?;
            AudioFileDecoder::create(U8Vec::from_vec(bytes.to_vec()), AzString::from(extension))
        }
        MediaSource::Generated { .. } => {
            return Err(String::from("a generated item has no sound"));
        }
    };
    if decoder.is_open() {
        Ok(decoder)
    } else {
        Err(decoder
            .error_message()
            .into_option()
            .map_or_else(|| String::from("no sound"), |why| why.as_str().to_string()))
    }
}

/// What the sound thread reports, shared with the window.
#[derive(Debug, Default)]
pub struct SoundReport {
    /// Samples per channel handed to the output.
    pub samples: AtomicU64,
    /// The loudest sample handed to it (`f32` bits).
    pub peak: AtomicU32,
}

impl SoundReport {
    /// The loudest sample so far.
    #[must_use]
    pub fn peak(&self) -> f32 {
        f32::from_bits(self.peak.load(Ordering::Relaxed))
    }

    fn add(&self, block: &[f32]) {
        let loudest = block.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        if loudest > self.peak() {
            self.peak.store(loudest.to_bits(), Ordering::Relaxed);
        }
        self.samples
            .fetch_add((block.len() / 2) as u64, Ordering::Relaxed);
    }
}

/// Plays `project`'s sound from `start` (a sequence frame) until `stopped` says so or nothing
/// more sounds. Blocking: from the playback's sound thread.
pub fn play(
    project: &Project,
    start: Frame,
    drive: &Arc<dyn Drive>,
    files: &Arc<MediaFiles>,
    report: &SoundReport,
    stopped: &mut dyn FnMut() -> bool,
) -> Result<(), String> {
    let spans = spans(project);
    if spans.is_empty() {
        return Ok(());
    }
    let sink = AudioSink::open(AudioConfig {
        sample_rate: RATE,
        channels: 2,
    });
    if !sink.is_open() {
        return Err(sink
            .error_message()
            .into_option()
            .map_or_else(|| String::from("no audio output"), |why| why.as_str().to_string()));
    }
    let fps = f64::from(project.sequence.fps.max(1));
    #[allow(clippy::cast_precision_loss)]
    let start_s = start as f64 / fps;
    let media: Vec<MediaItem> = project.media.clone();
    let mut mixer = Mixer::new(spans, start_s, |id| {
        let item = media.iter().find(|m| m.id == id)?;
        match open_decoder(item, drive.as_ref(), files) {
            Ok(decoder) => Some(Box::new(decoder) as Box<dyn Source>),
            Err(why) => {
                eprintln!("[azvideocut] no sound from {}: {why}", item.name);
                None
            }
        }
    });
    let mut pending: Option<Vec<f32>> = None;
    while !stopped() {
        if pending.is_none() {
            if mixer.finished() {
                break;
            }
            pending = Some(mixer.block(BLOCK));
        }
        if sink.queued_frames() >= LEAD {
            std::thread::sleep(std::time::Duration::from_millis(5));
            continue;
        }
        let Some(block) = pending.take() else {
            continue;
        };
        let frame = AudioFrame {
            sample_rate: RATE,
            channels: 2,
            samples: F32Vec::from(block.clone()),
        };
        if sink.try_play(frame) {
            report.add(&block);
        } else {
            // The output is full: the same block a moment later.
            pending = Some(block);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    // A stop is heard at once: what is queued is dropped.
    sink.clear();
    Ok(())
}

#[cfg(test)]
#[path = "sound_tests.rs"]
mod tests;
