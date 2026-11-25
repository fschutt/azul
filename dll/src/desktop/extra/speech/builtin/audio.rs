//! The audio side of the built-in recognizer, pure: a resampler from whatever rate the microphone
//! delivers to the 16 kHz the model takes, and an end-pointer that finds the one utterance in a
//! stream (where speech starts, where it ends, a little before and after). std only.

// The items are `pub` for the tests and for the parts of the module that are re-exported.
#![allow(unreachable_pub)]

/// The rate the model takes.
pub const MODEL_RATE: u32 = 16_000;

/// Streaming resampler to [`MODEL_RATE`]: a windowed-sinc kernel evaluated at each output's
/// position (so it low-passes and interpolates in one step, for any ratio: 48000, 44100, 24000,
/// 8000 Hz). Mono.
#[derive(Debug)]
pub struct Resampler {
    /// Input samples per output sample.
    step: f64,
    /// Kernel half-width in input samples.
    half: usize,
    /// Cutoff in cycles per input sample.
    cutoff: f64,
    /// The input kept between calls, with `half` zeros in front at the start.
    buffer: Vec<f32>,
    /// Where the next output sits in `buffer`, in input samples.
    position: f64,
    passthrough: bool,
    /// The kernel sampled every `1 / KERNEL_PHASES` of an input sample over `-half..=half`:
    /// evaluating the window and the sinc for every tap of every output would cost a tenth of a
    /// core for the sound of one microphone.
    table: Vec<f32>,
}

/// Steps of the kernel table per input sample.
const KERNEL_PHASES: usize = 128;

/// Zero crossings of the sinc on each side of the centre: more is sharper and dearer.
const ZERO_CROSSINGS: f64 = 14.0;

impl Resampler {
    /// A resampler for input at `rate` Hz (a rate of 0 is taken as the model's: nothing to do).
    #[must_use]
    pub fn new(rate: u32) -> Self {
        if rate == 0 || rate == MODEL_RATE {
            return Self {
                step: 1.0,
                half: 0,
                cutoff: 0.5,
                buffer: Vec::new(),
                position: 0.0,
                passthrough: true,
                table: Vec::new(),
            };
        }
        let step = f64::from(rate) / f64::from(MODEL_RATE);
        // Below the model's Nyquist frequency: 0.45 of the lower rate (the top of the band is not
        // speech anyway); above, the input's own Nyquist is the limit.
        let cutoff = 0.45 * f64::from(rate.min(MODEL_RATE)) / f64::from(rate);
        let half = (ZERO_CROSSINGS / (2.0 * cutoff)).ceil() as usize;
        let mut resampler = Self {
            step,
            half,
            cutoff,
            buffer: vec![0.0; half],
            position: half as f64,
            passthrough: false,
            table: Vec::new(),
        };
        resampler.table = (0..=2 * half * KERNEL_PHASES)
            .map(|i| resampler.kernel(i as f64 / KERNEL_PHASES as f64 - half as f64) as f32)
            .collect();
        resampler
    }

    /// The kernel at `offset` input samples from its centre, exactly.
    fn kernel(&self, offset: f64) -> f64 {
        let x = offset * 2.0 * self.cutoff;
        let sinc = if x.abs() < 1e-9 {
            1.0
        } else {
            (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
        };
        // Blackman window over the half-width.
        let w = offset / self.half as f64;
        let window = if w.abs() >= 1.0 {
            0.0
        } else {
            0.42 + 0.5 * (std::f64::consts::PI * w).cos() + 0.08 * (2.0 * std::f64::consts::PI * w).cos()
        };
        2.0 * self.cutoff * sinc * window
    }

    /// The kernel at `offset`, read from the table (linear between two steps).
    fn weight_at(&self, offset: f64) -> f64 {
        let at = (offset + self.half as f64) * KERNEL_PHASES as f64;
        if at <= 0.0 || at >= (self.table.len() - 1) as f64 {
            return 0.0;
        }
        let low = at.floor();
        let fraction = at - low;
        let low = low as usize;
        f64::from(self.table[low]) * (1.0 - fraction) + f64::from(self.table[low + 1]) * fraction
    }

    /// What `input` (more of the stream) adds to the output. A call may return nothing while the
    /// kernel still waits for later samples.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if self.passthrough {
            return input.to_vec();
        }
        self.buffer.extend_from_slice(input);
        let mut out = Vec::with_capacity((input.len() as f64 / self.step) as usize + 2);
        loop {
            let centre = self.position.floor();
            let first = centre as isize - self.half as isize + 1;
            let last = centre as isize + self.half as isize;
            if last as usize >= self.buffer.len() {
                break;
            }
            let mut sum = 0.0;
            let mut weight = 0.0;
            for index in first..=last {
                let k = self.weight_at(self.position - index as f64);
                // Before the start of the stream there is silence.
                if index >= 0 {
                    sum += f64::from(self.buffer[index as usize]) * k;
                }
                weight += k;
            }
            out.push(if weight.abs() > 1e-12 { (sum / weight) as f32 } else { 0.0 });
            self.position += self.step;
        }
        // Keep what the next output still needs.
        let keep_from = (self.position.floor() as isize - self.half as isize).max(0) as usize;
        if keep_from > 0 {
            self.buffer.drain(..keep_from);
            self.position -= keep_from as f64;
        }
        out
    }
}

/// How the end-pointer tells speech from the room.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vad {
    /// How much before the first voiced frame is kept, ms (the first consonant is quiet).
    pub preroll_ms: u32,
    /// How long it takes to be sure speech has begun, ms: a click is not speech.
    pub onset_ms: u32,
    /// How long without speech ends the utterance, ms.
    pub hangover_ms: u32,
    /// How much of that silence is kept after the last voiced frame, ms.
    pub tail_ms: u32,
    /// The least energy that can be speech, as RMS of samples in -1..1 (about -56 dBFS).
    pub min_rms: f32,
    /// How far above the noise floor a frame must be.
    pub ratio: f32,
    /// Once speech is going, the part of its mean loudness a frame must reach to count as the
    /// speech going on.
    pub continue_ratio: f32,
}

impl Default for Vad {
    fn default() -> Self {
        Self {
            preroll_ms: 300,
            onset_ms: 100,
            hangover_ms: 800,
            tail_ms: 300,
            min_rms: 0.0015,
            ratio: 3.0,
            continue_ratio: 0.25,
        }
    }
}

/// One utterance cut out of the stream.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub samples: Vec<f32>,
    /// Where it begins in the stream, ms from the first sample pushed.
    pub start_ms: u64,
}

/// Where the end-pointer stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// No speech yet.
    Waiting,
    /// Speech is going on (or just paused).
    Speaking,
    /// The utterance is over: [`Endpointer::take`] has it.
    Done,
}

/// The samples in a frame: 20 ms.
const FRAME: usize = (MODEL_RATE as usize) / 50;
/// How many frames the noise floor looks back over: 3 s.
const FLOOR_FRAMES: usize = 150;
/// How many frames it takes before the floor is trusted over the starting value.
const FLOOR_MIN_FRAMES: usize = 10;

/// Finds one utterance in a stream of 16 kHz mono samples.
#[derive(Debug)]
pub struct Endpointer {
    vad: Vad,
    /// A partial frame carried to the next push.
    carry: Vec<f32>,
    /// Everything since the pre-roll before the first voiced frame (or the last `preroll` of
    /// the room while waiting).
    kept: Vec<f32>,
    /// Index in the stream of `kept[0]`.
    kept_from: u64,
    /// Frames pushed so far.
    frames: u64,
    /// The RMS of the last frames: the room is as loud as the quietest of them.
    history: std::collections::VecDeque<f32>,
    /// What the room is taken to be before there is any history.
    initial_floor: f32,
    /// Voiced frames in a row, and quiet frames since the last voiced one.
    voiced_run: u32,
    quiet_run: u32,
    speaking: bool,
    done: bool,
    /// The stream index just past the last voiced frame.
    last_voiced_end: u64,
    /// The RMS of the voiced frames since the speech began: their sum and their number.
    speech_sum: f64,
    speech_frames: u32,
    /// Where the speech began (the first frame of the voiced run that started it).
    speech_from: u64,
}

impl Endpointer {
    #[must_use]
    pub fn new(vad: Vad) -> Self {
        Self::with_floor(vad, 0.0004)
    }

    /// An end-pointer that starts from the noise floor an earlier call found (the room does not
    /// change between two calls of a dictation).
    #[must_use]
    pub fn with_floor(vad: Vad, floor: f32) -> Self {
        Self {
            vad,
            carry: Vec::new(),
            kept: Vec::new(),
            kept_from: 0,
            frames: 0,
            history: std::collections::VecDeque::with_capacity(FLOOR_FRAMES + 1),
            initial_floor: if floor.is_finite() { floor.clamp(0.0, 0.05) } else { 0.0004 },
            voiced_run: 0,
            quiet_run: 0,
            speaking: false,
            done: false,
            last_voiced_end: 0,
            speech_sum: 0.0,
            speech_frames: 0,
            speech_from: 0,
        }
    }

    /// How loud the room is taken to be: the quietest frame of the last three seconds.
    #[must_use]
    pub fn noise_floor(&self) -> f32 {
        if self.history.len() < FLOOR_MIN_FRAMES {
            self.initial_floor
        } else {
            self.history.iter().copied().fold(f32::MAX, f32::min)
        }
    }

    /// Audio milliseconds pushed so far (whole frames).
    #[must_use]
    pub fn elapsed_ms(&self) -> u64 {
        self.frames * 20
    }

    fn ms_to_frames(ms: u32) -> u32 {
        ms.div_ceil(20)
    }

    fn rms(frame: &[f32]) -> f32 {
        if frame.is_empty() {
            return 0.0;
        }
        (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt()
    }

    /// Takes the next stretch of the stream.
    pub fn push(&mut self, samples: &[f32]) -> Progress {
        if self.done {
            return Progress::Done;
        }
        self.carry.extend_from_slice(samples);
        let mut at = 0;
        while !self.done && self.carry.len() - at >= FRAME {
            let frame: Vec<f32> = self.carry[at..at + FRAME].to_vec();
            at += FRAME;
            self.frame(&frame);
        }
        self.carry.drain(..at);
        self.progress()
    }

    fn progress(&self) -> Progress {
        if self.done {
            Progress::Done
        } else if self.speaking {
            Progress::Speaking
        } else {
            Progress::Waiting
        }
    }

    fn frame(&mut self, frame: &[f32]) {
        let begin = self.frames * FRAME as u64;
        let end = begin + FRAME as u64;
        self.frames += 1;
        let rms = Self::rms(frame);
        let mut threshold = (self.noise_floor() * self.vad.ratio).max(self.vad.min_rms);
        if self.speaking && self.speech_frames > 0 {
            // Once it is going, a stretch of the room that is louder than the room's usual
            // (a click, the loudspeaker's tail) must not hold the end off: a frame counts as the
            // speech going on only when it is a good part of how loud the speech was.
            let mean = (self.speech_sum / f64::from(self.speech_frames)) as f32;
            threshold = threshold.max(self.vad.continue_ratio * mean);
        }
        let voiced = rms > threshold;
        self.history.push_back(rms);
        if self.history.len() > FLOOR_FRAMES {
            self.history.pop_front();
        }
        self.kept.extend_from_slice(frame);
        if voiced {
            self.voiced_run += 1;
            self.quiet_run = 0;
            if !self.speaking {
                if self.voiced_run == 1 {
                    self.speech_from = begin;
                }
                if self.voiced_run >= Self::ms_to_frames(self.vad.onset_ms).max(1) {
                    self.speaking = true;
                }
            }
            self.last_voiced_end = end;
            if self.speaking {
                self.speech_sum += f64::from(rms);
                self.speech_frames += 1;
            }
        } else {
            self.quiet_run += 1;
            if !self.speaking {
                // A short burst that did not last is the room, not speech.
                self.voiced_run = 0;
            } else if self.quiet_run >= Self::ms_to_frames(self.vad.hangover_ms) {
                self.done = true;
            }
        }
        if !self.speaking {
            // While waiting only the pre-roll before a possible start is worth keeping.
            let keep = (self.vad.preroll_ms as usize * MODEL_RATE as usize / 1000)
                + (Self::ms_to_frames(self.vad.onset_ms) as usize + 1) * FRAME;
            if self.kept.len() > keep {
                let drop = self.kept.len() - keep;
                self.kept.drain(..drop);
                self.kept_from += drop as u64;
            }
        }
    }

    /// The utterance, once [`Progress::Done`] (or [`Self::finish`]): `None` when there was
    /// none or it was shorter than a blink.
    pub fn take(&mut self) -> Option<Segment> {
        if !self.done {
            return None;
        }
        self.cut()
    }

    /// Ends the utterance now (the time is up, the caller cancelled): the speech so far, when
    /// there is some.
    pub fn finish(&mut self) -> Option<Segment> {
        if !self.speaking {
            return None;
        }
        self.done = true;
        self.cut()
    }

    fn cut(&mut self) -> Option<Segment> {
        if !self.speaking {
            return None;
        }
        let preroll = u64::from(self.vad.preroll_ms) * u64::from(MODEL_RATE) / 1000;
        let tail = u64::from(self.vad.tail_ms) * u64::from(MODEL_RATE) / 1000;
        let from = self.speech_from.saturating_sub(preroll).max(self.kept_from);
        let to = (self.last_voiced_end + tail).min(self.kept_from + self.kept.len() as u64);
        if to <= from {
            return None;
        }
        let begin = (from - self.kept_from) as usize;
        let finish = (to - self.kept_from) as usize;
        Some(Segment {
            samples: self.kept[begin..finish].to_vec(),
            start_ms: from * 1000 / u64::from(MODEL_RATE),
        })
    }
}

#[cfg(test)]
#[path = "audio_test.rs"]
mod audio_test;
