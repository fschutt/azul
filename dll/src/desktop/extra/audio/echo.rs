//! Acoustic echo cancellation - the `EchoCanceller` handle.
//!
//! In a call without headphones the microphone hears the speaker: every peer hears itself a
//! moment later. An echo canceller learns the path from what is played (the FAR end) to what
//! the microphone captures (the NEAR end) and subtracts its estimate of the echo from the
//! microphone, so only the person in the room is sent.
//!
//! Pure Rust, no platform API: one adaptive filter design for every target, tested here on
//! synthetic rooms.
//!
//! # Design
//!
//! A partitioned-block frequency-domain adaptive filter (overlap-save, the MDF of Soo & Pang as
//! Speex uses it): blocks of [`block_len`] samples, an FFT twice that long, as many partitions
//! as the echo tail needs. The step is normalized per frequency bin by the far end's power over
//! all partitions (frequency-domain NLMS); one partition per block gets the gradient constraint
//! (round robin), the rest adapt unconstrained.
//!
//! Double talk (the person in the room speaks while the far end plays) is what breaks a single
//! adaptive filter: it adapts on the near-end voice and smears it. So there are two filters, as
//! in Speex: a BACKGROUND filter adapts every block; the FOREGROUND filter makes the output and
//! takes the background's coefficients only when the background cancels significantly better
//! (the improvement must stand out against how far the two filters' outputs differ - a near-end
//! voice raises both filters' errors alike); a background that became significantly worse is
//! reset to the foreground. Should the foreground's output ever be louder than the microphone,
//! the microphone passes unchanged.
//!
//! Limits: a pure tone at the near end while the far end plays can be partly learned (an
//! adaptive filter notches a steady sinusoid); no residual-echo suppressor or comfort noise yet;
//! one rate for both ends (no resampler).

use core::ffi::c_void;

use azul_core::audio::AudioFrame;
use azul_css::F32Vec;

/// The echo tail an app should ask for when it does not know better (ms): a laptop's speaker to
/// its microphone, the room, and the audio buffers of both sides.
pub const DEFAULT_TAIL_MS: u32 = 300;

/// Samples in one block at `sample_rate`: 256 (5.3 ms at 48 kHz), 128 at 24 kHz and below.
pub fn block_len(sample_rate: u32) -> usize {
    if sample_rate <= 24_000 {
        128
    } else {
        256
    }
}

/// Most tail a canceller takes (ms): longer filters converge slowly and cost more than they cancel.
const MAX_TAIL_MS: u32 = 2000;
/// The background filter's step (frequency-domain NLMS): fast convergence, stable.
const STEP: f32 = 0.8;
/// How much of the last block's far-end power the per-bin normalization keeps.
const POWER_SMOOTHING: f32 = 0.7;
/// Speex's thresholds for taking the background's coefficients, and for resetting it.
const VAR1_UPDATE: f64 = 0.5;
const VAR2_UPDATE: f64 = 0.25;
const VAR_BACKTRACK: f64 = 4.0;
/// Blocks the ERLE statistic averages over (about half a second at 48 kHz).
const ERLE_BLOCKS: f32 = 94.0;

/// A complex number: an FFT sample.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Complex {
    re: f32,
    im: f32,
}

impl Complex {
    const ZERO: Complex = Complex { re: 0.0, im: 0.0 };

    fn mul(self, o: Complex) -> Complex {
        Complex {
            re: self.re * o.re - self.im * o.im,
            im: self.re * o.im + self.im * o.re,
        }
    }

    fn conj(self) -> Complex {
        Complex {
            re: self.re,
            im: -self.im,
        }
    }

    fn norm_sqr(self) -> f32 {
        self.re * self.re + self.im * self.im
    }
}

/// An iterative radix-2 FFT of one power-of-two size.
#[derive(Debug)]
struct Fft {
    n: usize,
    /// `exp(-2 pi i k / n)` for `k < n / 2`.
    twiddles: Vec<Complex>,
    /// The bit-reversed index of every position.
    reversed: Vec<usize>,
}

impl Fft {
    fn new(n: usize) -> Fft {
        debug_assert!(n.is_power_of_two() && n >= 2);
        let bits = n.trailing_zeros();
        let twiddles = (0..n / 2)
            .map(|k| {
                let angle = -2.0 * core::f64::consts::PI * k as f64 / n as f64;
                Complex {
                    re: angle.cos() as f32,
                    im: angle.sin() as f32,
                }
            })
            .collect();
        let reversed = (0..n)
            .map(|i| i.reverse_bits() >> (usize::BITS - bits))
            .collect();
        Fft {
            n,
            twiddles,
            reversed,
        }
    }

    /// The DFT of `a` in place (`inverse`: the inverse, scaled by `1 / n`).
    fn transform(&self, a: &mut [Complex], inverse: bool) {
        let n = self.n;
        for i in 0..n {
            let j = self.reversed[i];
            if i < j {
                a.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let half = len / 2;
            let stride = n / len;
            for start in (0..n).step_by(len) {
                for k in 0..half {
                    let w = self.twiddles[k * stride];
                    let w = if inverse { w.conj() } else { w };
                    let u = a[start + k];
                    let v = a[start + k + half].mul(w);
                    a[start + k] = Complex {
                        re: u.re + v.re,
                        im: u.im + v.im,
                    };
                    a[start + k + half] = Complex {
                        re: u.re - v.re,
                        im: u.im - v.im,
                    };
                }
            }
            len <<= 1;
        }
        if inverse {
            let scale = 1.0 / n as f32;
            for v in a.iter_mut() {
                v.re *= scale;
                v.im *= scale;
            }
        }
    }

    /// The half spectrum (bins `0..=n/2`) of the real signal `x` (`n` samples) into `half`;
    /// `scratch` holds `n` values.
    fn forward_real(&self, x: &[f32], half: &mut [Complex], scratch: &mut [Complex]) {
        for (s, v) in scratch.iter_mut().zip(x) {
            *s = Complex { re: *v, im: 0.0 };
        }
        self.transform(scratch, false);
        half.copy_from_slice(&scratch[..self.n / 2 + 1]);
    }

    /// The real signal (`n` samples, into `out`) of the half spectrum `half`; `scratch` holds
    /// `n` values.
    fn inverse_real(&self, half: &[Complex], out: &mut [f32], scratch: &mut [Complex]) {
        let n = self.n;
        scratch[..=n / 2].copy_from_slice(half);
        for k in 1..n / 2 {
            scratch[n - k] = half[k].conj();
        }
        self.transform(scratch, true);
        for (o, s) in out.iter_mut().zip(scratch.iter()) {
            *o = s.re;
        }
    }
}

/// The adaptive filters and the far end's recent spectra (see the module docs).
#[derive(Debug)]
struct EchoCore {
    /// Samples in a block; the FFT is twice as long.
    block: usize,
    fft: Fft,
    /// The far end's last block (overlap-save keeps one block of history).
    prev_far: Vec<f32>,
    /// The far end's spectra, newest first: one per partition.
    far: std::collections::VecDeque<Vec<Complex>>,
    /// The background (adapting) and foreground (output) filters, per partition.
    background: Vec<Vec<Complex>>,
    foreground: Vec<Vec<Complex>>,
    /// The far end's power per bin over all partitions, smoothed.
    power: Vec<f32>,
    /// Speex's foreground-update statistics.
    davg1: f64,
    davg2: f64,
    dvar1: f64,
    dvar2: f64,
    /// The partition the gradient constraint goes to next.
    constrain: usize,
    /// Smoothed near-end and output power while the far end plays (the ERLE statistic).
    near_power: f32,
    out_power: f32,
    // Work buffers.
    scratch: Vec<Complex>,
    spectrum: Vec<Complex>,
    time: Vec<f32>,
}

impl EchoCore {
    fn new(block: usize, partitions: usize) -> EchoCore {
        let n = block * 2;
        let bins = block + 1;
        let zeros = || vec![Complex::ZERO; bins];
        EchoCore {
            block,
            fft: Fft::new(n),
            prev_far: vec![0.0; block],
            far: (0..partitions).map(|_| zeros()).collect(),
            background: (0..partitions).map(|_| zeros()).collect(),
            foreground: (0..partitions).map(|_| zeros()).collect(),
            power: vec![0.0; bins],
            davg1: 0.0,
            davg2: 0.0,
            dvar1: 0.0,
            dvar2: 0.0,
            constrain: 0,
            near_power: 0.0,
            out_power: 0.0,
            scratch: vec![Complex::ZERO; n],
            spectrum: zeros(),
            time: vec![0.0; n],
        }
    }

    /// The echo `filter` estimates for the current block (`block` samples into `out`).
    fn estimate(&mut self, filter: &[Vec<Complex>], out: &mut [f32]) {
        for v in self.spectrum.iter_mut() {
            *v = Complex::ZERO;
        }
        for (w, x) in filter.iter().zip(self.far.iter()) {
            for ((y, w), x) in self.spectrum.iter_mut().zip(w).zip(x) {
                let p = w.mul(*x);
                y.re += p.re;
                y.im += p.im;
            }
        }
        self.fft
            .inverse_real(&self.spectrum, &mut self.time, &mut self.scratch);
        out.copy_from_slice(&self.time[self.block..]);
    }

    /// One block: `far` was played, `near` is what the microphone captured meanwhile; `out`
    /// gets the microphone with the echo removed.
    fn process(&mut self, far: &[f32], near: &[f32], out: &mut [f32]) {
        let b = self.block;
        // The far end's newest spectrum: the last two blocks (overlap-save).
        self.time[..b].copy_from_slice(&self.prev_far);
        self.time[b..].copy_from_slice(far);
        self.prev_far.copy_from_slice(far);
        let mut newest = self.far.pop_back().unwrap_or_default();
        newest.resize(b + 1, Complex::ZERO);
        let input = core::mem::take(&mut self.time);
        self.fft
            .forward_real(&input, &mut newest, &mut self.scratch);
        self.time = input;
        self.far.push_front(newest);

        // Both filters' echo estimates and errors.
        let mut echo_b = vec![0.0f32; b];
        let mut echo_f = vec![0.0f32; b];
        let background = core::mem::take(&mut self.background);
        self.estimate(&background, &mut echo_b);
        self.background = background;
        let foreground = core::mem::take(&mut self.foreground);
        self.estimate(&foreground, &mut echo_f);
        self.foreground = foreground;
        let mut err_b: Vec<f32> = near.iter().zip(&echo_b).map(|(d, y)| d - y).collect();
        let mut err_f: Vec<f32> = near.iter().zip(&echo_f).map(|(d, y)| d - y).collect();
        let energy = |x: &[f32]| x.iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>();
        let see = energy(&err_b);
        let mut sff = energy(&err_f);
        let dbf = echo_b
            .iter()
            .zip(&echo_f)
            .map(|(a, c)| {
                let d = f64::from(a - c);
                d * d
            })
            .sum::<f64>()
            + 1e-12;

        // Take the background's coefficients when it cancels significantly better; reset it
        // when it got significantly worse (Speex's test).
        let gain = sff - see;
        self.davg1 = 0.6 * self.davg1 + 0.4 * gain;
        self.davg2 = 0.85 * self.davg2 + 0.15 * gain;
        self.dvar1 = 0.36 * self.dvar1 + 0.16 * sff * dbf;
        self.dvar2 = 0.7225 * self.dvar2 + 0.0225 * sff * dbf;
        let take = gain * gain.abs() > sff * dbf
            || self.davg1 * self.davg1.abs() > VAR1_UPDATE * self.dvar1
            || self.davg2 * self.davg2.abs() > VAR2_UPDATE * self.dvar2;
        let backtrack = -gain * gain.abs() > VAR_BACKTRACK * sff * dbf
            || -self.davg1 * self.davg1.abs() > VAR_BACKTRACK * self.dvar1
            || -self.davg2 * self.davg2.abs() > VAR_BACKTRACK * self.dvar2;
        if take {
            for (f, w) in self.foreground.iter_mut().zip(&self.background) {
                f.copy_from_slice(w);
            }
            err_f.copy_from_slice(&err_b);
            sff = see;
            self.reset_statistics();
        } else if backtrack {
            for (w, f) in self.background.iter_mut().zip(&self.foreground) {
                w.copy_from_slice(f);
            }
            err_b.copy_from_slice(&err_f);
            self.reset_statistics();
        }

        self.adapt(&err_b);

        // The output: the foreground's error, unless it is louder than the microphone.
        let near_energy = energy(near);
        if sff > near_energy {
            out.copy_from_slice(near);
        } else {
            out.copy_from_slice(&err_f);
        }
        self.count_erle(far, near_energy, energy(out));
    }

    fn reset_statistics(&mut self) {
        self.davg1 = 0.0;
        self.davg2 = 0.0;
        self.dvar1 = 0.0;
        self.dvar2 = 0.0;
    }

    /// The background filter's step on its own error `err` (one block), normalized per bin by
    /// the far end's power; one partition gets the gradient constraint.
    fn adapt(&mut self, err: &[f32]) {
        let b = self.block;
        self.time[..b].fill(0.0);
        self.time[b..].copy_from_slice(err);
        let input = core::mem::take(&mut self.time);
        let mut e = core::mem::take(&mut self.spectrum);
        self.fft.forward_real(&input, &mut e, &mut self.scratch);
        self.time = input;
        let delta = 1e-6 * (2 * b) as f32;
        for (k, p) in self.power.iter_mut().enumerate() {
            let total: f32 = self.far.iter().map(|x| x[k].norm_sqr()).sum();
            *p = POWER_SMOOTHING * *p + (1.0 - POWER_SMOOTHING) * total;
        }
        for (w, x) in self.background.iter_mut().zip(self.far.iter()) {
            for (k, wk) in w.iter_mut().enumerate() {
                let step = STEP / (self.power[k] + delta);
                let g = x[k].conj().mul(e[k]);
                wk.re += step * g.re;
                wk.im += step * g.im;
            }
        }
        self.spectrum = e;
        // The gradient constraint on one partition: its impulse response must fit one block.
        let p = self.constrain;
        self.constrain = (self.constrain + 1) % self.background.len();
        let mut w = core::mem::take(&mut self.background[p]);
        let mut time = core::mem::take(&mut self.time);
        self.fft.inverse_real(&w, &mut time, &mut self.scratch);
        time[b..].fill(0.0);
        self.fft.forward_real(&time, &mut w, &mut self.scratch);
        self.time = time;
        self.background[p] = w;
    }

    /// The ERLE statistic: near-end and output power, smoothed, while the far end plays.
    fn count_erle(&mut self, far: &[f32], near_energy: f64, out_energy: f64) {
        let playing = far.iter().any(|v| v.abs() > 1e-4);
        if !playing {
            return;
        }
        let a = 1.0 / ERLE_BLOCKS;
        self.near_power = (1.0 - a) * self.near_power + a * near_energy as f32;
        self.out_power = (1.0 - a) * self.out_power + a * out_energy as f32;
    }

    fn erle_db(&self) -> f32 {
        if self.near_power <= 0.0 {
            return 0.0;
        }
        10.0 * (self.near_power / self.out_power.max(1e-20)).log10()
    }
}

/// The canceller between two free-running streams: the far end as it is fed, the microphone in
/// frames of any size; blocks are cut from both, and the output keeps the input's size one block
/// later.
#[derive(Debug)]
struct EchoStream {
    core: EchoCore,
    sample_rate: u32,
    /// Far-end samples fed and not used yet, oldest first.
    far: std::collections::VecDeque<f32>,
    /// Most far-end samples kept: older ones belong to no microphone block any more.
    max_far: usize,
    /// Microphone samples waiting for a whole block.
    near: Vec<f32>,
    /// Processed samples waiting to go out (primed with one block of silence: the latency).
    out: std::collections::VecDeque<f32>,
}

impl EchoStream {
    fn new(sample_rate: u32, tail_ms: u32) -> EchoStream {
        let block = block_len(sample_rate);
        let tail = (u64::from(sample_rate) * u64::from(tail_ms.min(MAX_TAIL_MS)) / 1000) as usize;
        let partitions = tail.div_ceil(block).max(1);
        EchoStream {
            core: EchoCore::new(block, partitions),
            sample_rate,
            far: std::collections::VecDeque::new(),
            max_far: tail + 2 * block,
            near: Vec::with_capacity(block),
            out: std::iter::repeat(0.0).take(block).collect(),
        }
    }

    fn far_end(&mut self, samples: &[f32]) {
        self.far.extend(samples.iter().copied());
        let excess = self.far.len().saturating_sub(self.max_far);
        self.far.drain(..excess);
    }

    fn process(&mut self, samples: &[f32]) -> Vec<f32> {
        let b = self.core.block;
        let mut far = vec![0.0f32; b];
        let mut out = vec![0.0f32; b];
        for s in samples {
            self.near.push(*s);
            if self.near.len() < b {
                continue;
            }
            // The far end of this block: what was fed first (silence where nothing was).
            for f in far.iter_mut() {
                *f = self.far.pop_front().unwrap_or(0.0);
            }
            self.core.process(&far, &self.near, &mut out);
            self.out.extend(out.iter().copied());
            self.near.clear();
        }
        self.out
            .drain(..samples.len().min(self.out.len()))
            .collect()
    }
}

/// An echo canceller handle: feed it what is played ([`far_end`](Self::far_end)) and pass the
/// microphone through [`process`](Self::process). Its methods may be called from different
/// threads (the playout's and the microphone's): the state sits behind a lock.
#[repr(C)]
pub struct EchoCanceller {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

impl Clone for EchoCanceller {
    fn clone(&self) -> Self {
        EchoCanceller {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

impl Default for EchoCanceller {
    fn default() -> Self {
        EchoCanceller {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl EchoCanceller {
    /// A canceller for mono audio at `sample_rate` that removes echoes arriving up to `tail_ms`
    /// after the sound was played (the audio buffers' delay plus the room's reverberation;
    /// [`DEFAULT_TAIL_MS`] when unsure). Closed for a zero rate or tail.
    pub fn create(sample_rate: u32, tail_ms: u32) -> EchoCanceller {
        if sample_rate == 0 || tail_ms == 0 {
            return EchoCanceller::default();
        }
        let inner = Box::new(std::sync::Mutex::new(EchoStream::new(sample_rate, tail_ms)));
        EchoCanceller {
            ptr: Box::into_raw(inner) as *mut c_void,
            run_destructor: true,
        }
    }

    /// The stream behind an open handle, locked (through a poisoned lock: the state is plain
    /// data).
    fn stream(&self) -> Option<std::sync::MutexGuard<'_, EchoStream>> {
        let inner = unsafe { (self.ptr as *const std::sync::Mutex<EchoStream>).as_ref() }?;
        Some(
            inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// Whether the canceller opened.
    pub fn is_open(&self) -> bool {
        !self.ptr.is_null()
    }

    /// The far end: `frame` is about to be played (mono, at the canceller's rate). Feed every
    /// frame that goes to the speaker, in order, when it is handed to the output. False when the
    /// canceller is not open or the frame is in another format.
    pub fn far_end(&self, frame: AudioFrame) -> bool {
        let Some(mut stream) = self.stream() else {
            return false;
        };
        if frame.sample_rate != stream.sample_rate || frame.channels != 1 {
            return false;
        }
        stream.far_end(frame.samples.as_ref());
        true
    }

    /// The near end: `frame` as the microphone captured it (mono, at the canceller's rate),
    /// returned with the echo of the far end removed - as many samples as came in, [`latency`]
    /// samples later (one block). A frame in another format, or a closed canceller, returns
    /// the frame unchanged.
    ///
    /// [`latency`]: Self::latency_samples
    pub fn process(&self, frame: AudioFrame) -> AudioFrame {
        let Some(mut stream) = self.stream() else {
            return frame;
        };
        if frame.sample_rate != stream.sample_rate || frame.channels != 1 {
            return frame;
        }
        let out = stream.process(frame.samples.as_ref());
        AudioFrame {
            sample_rate: frame.sample_rate,
            channels: 1,
            samples: F32Vec::from_vec(out),
        }
    }

    /// How many samples `process` delays the microphone by (one block).
    pub fn latency_samples(&self) -> u32 {
        self.stream().map_or(0, |stream| {
            u32::try_from(stream.core.block).unwrap_or(u32::MAX)
        })
    }

    /// How much of the echo is removed right now, in dB (echo return loss enhancement, over the
    /// last half second the far end played); 0 while nothing played.
    pub fn erle_db(&self) -> f32 {
        self.stream().map_or(0.0, |stream| stream.core.erle_db())
    }

    /// Release the canceller. (Drop does this too.)
    pub fn close(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr as *mut std::sync::Mutex<EchoStream>));
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl Drop for EchoCanceller {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod echo_tests {
    use azul_core::audio::AudioFrame;
    use azul_css::F32Vec;

    use super::{block_len, EchoCanceller};

    const RATE: u32 = 48_000;

    /// Deterministic white noise in -1..1.
    fn noise(seed: u32, n: usize, amplitude: f32) -> Vec<f32> {
        let mut s = seed.max(1);
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                (s as f32 / u32::MAX as f32 * 2.0 - 1.0) * amplitude
            })
            .collect()
    }

    /// A room: `delay_ms` of silence, then `taps_ms` of exponentially decaying random taps
    /// (decay constant `decay_ms`), scaled to an L2 norm of `gain`.
    fn room(seed: u32, delay_ms: u32, taps_ms: u32, decay_ms: f32, gain: f32) -> Vec<f32> {
        let delay = (RATE * delay_ms / 1000) as usize;
        let taps = (RATE * taps_ms / 1000) as usize;
        let random = noise(seed, taps, 0.6);
        let mut h = vec![0.0f32; delay + taps];
        for (i, r) in random.iter().enumerate() {
            h[delay + i] = r * (-(i as f32) / (RATE as f32 * decay_ms / 1000.0)).exp();
        }
        let norm = h.iter().map(|v| v * v).sum::<f32>().sqrt();
        h.iter().map(|v| v * gain / norm).collect()
    }

    fn convolve(x: &[f32], h: &[f32]) -> Vec<f32> {
        let taps: Vec<(usize, f32)> = h
            .iter()
            .enumerate()
            .filter(|(_, v)| **v != 0.0)
            .map(|(i, v)| (i, *v))
            .collect();
        (0..x.len())
            .map(|n| {
                taps.iter()
                    .take_while(|(i, _)| *i <= n)
                    .map(|(i, v)| v * x[n - i])
                    .sum()
            })
            .collect()
    }

    /// Speech-like near-end talk: noise under a 4 Hz syllable envelope.
    fn talk(n: usize) -> Vec<f32> {
        noise(999, n, 0.25)
            .into_iter()
            .enumerate()
            .map(|(i, v)| {
                let envelope =
                    0.5 + 0.5 * (2.0 * core::f32::consts::PI * 4.0 * i as f32 / RATE as f32).sin();
                v * envelope
            })
            .collect()
    }

    fn power_db(x: &[f32]) -> f32 {
        let p = x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32;
        10.0 * (p + 1e-20).log10()
    }

    fn mono(samples: Vec<f32>) -> AudioFrame {
        AudioFrame {
            sample_rate: RATE,
            channels: 1,
            samples: F32Vec::from_vec(samples),
        }
    }

    /// Runs a call: `far` is played (fed 20 ms at a time), `near` is what the microphone
    /// captured; returns the canceller's output, lined up with `near` (its latency removed).
    fn run(aec: &EchoCanceller, far: &[f32], near: &[f32]) -> Vec<f32> {
        let chunk = (RATE / 50) as usize;
        let mut out = Vec::with_capacity(near.len());
        for (f, n) in far.chunks(chunk).zip(near.chunks(chunk)) {
            assert!(
                aec.far_end(mono(f.to_vec())),
                "an open canceller takes the far end"
            );
            let processed = aec.process(mono(n.to_vec()));
            assert_eq!(
                processed.samples.as_ref().len(),
                n.len(),
                "as many samples as came in"
            );
            out.extend_from_slice(processed.samples.as_ref());
        }
        let latency = aec.latency_samples() as usize;
        out.drain(..latency.min(out.len()));
        out
    }

    /// The echo of white noise through a room (20 ms away, 80 ms of reverberation, 6 dB down):
    /// after three seconds of learning, the fourth second is at least 25 dB quieter than the
    /// microphone heard it.
    #[test]
    fn a_converged_canceller_removes_the_echo_of_the_far_end() {
        let aec = EchoCanceller::create(RATE, 200);
        assert!(aec.is_open());
        let n = RATE as usize * 4;
        let far = noise(12345, n, 0.3);
        let near = convolve(&far, &room(7, 20, 80, 20.0, 0.5));
        let out = run(&aec, &far, &near);
        let last = (RATE as usize * 3)..out.len();
        let erle = power_db(&near[last.clone()]) - power_db(&out[last]);
        assert!(
            erle >= 25.0,
            "the echo is only {erle:.1} dB quieter after three seconds"
        );
        assert!(
            aec.erle_db() >= 20.0,
            "the canceller reports {:.1} dB",
            aec.erle_db()
        );
    }

    /// The person in the room starts speaking while the far end plays (double talk): their
    /// voice goes out as they spoke it, and the echo stays cancelled.
    #[test]
    fn near_end_talk_passes_through_while_the_echo_stays_cancelled() {
        let aec = EchoCanceller::create(RATE, 200);
        let n = RATE as usize * 6;
        let far = noise(12345, n, 0.3);
        let echo = convolve(&far, &room(7, 20, 80, 20.0, 0.5));
        let start = RATE as usize * 3;
        let mut voice = vec![0.0f32; n];
        voice[start..].copy_from_slice(&talk(n - start));
        let near: Vec<f32> = echo.iter().zip(&voice).map(|(e, v)| e + v).collect();
        let out = run(&aec, &far, &near);
        let during = (RATE as usize * 4)..out.len();
        let residual: Vec<f32> = during.clone().map(|i| out[i] - voice[i]).collect();
        let erle = power_db(&echo[during.clone()]) - power_db(&residual);
        assert!(
            erle >= 20.0,
            "while the near end talks the echo is only {erle:.1} dB down"
        );
        let kept = power_db(&out[during.clone()]) - power_db(&voice[during]);
        assert!(
            kept.abs() <= 1.0,
            "the near-end voice came out {kept:.2} dB off"
        );
    }

    /// The echo path changes (the laptop moved): the canceller learns the new one.
    #[test]
    fn a_new_echo_path_is_learned_again() {
        let aec = EchoCanceller::create(RATE, 200);
        let n = RATE as usize * 7;
        let far = noise(12345, n, 0.3);
        let first = convolve(&far, &room(7, 20, 80, 20.0, 0.5));
        let second = convolve(&far, &room(31337, 35, 60, 15.0, 0.7));
        let cut = RATE as usize * 3;
        let near: Vec<f32> = (0..n)
            .map(|i| if i < cut { first[i] } else { second[i] })
            .collect();
        let out = run(&aec, &far, &near);
        let last = (RATE as usize * 6)..out.len();
        let erle = power_db(&near[last.clone()]) - power_db(&out[last]);
        assert!(
            erle >= 20.0,
            "three seconds after the change the echo is {erle:.1} dB down"
        );
    }

    /// Nothing played: the microphone comes back as it was, one block later.
    #[test]
    fn without_a_far_end_the_microphone_passes_unchanged() {
        let aec = EchoCanceller::create(RATE, 200);
        assert_eq!(aec.latency_samples() as usize, block_len(RATE));
        let voice = talk(RATE as usize);
        let mut out = Vec::new();
        for chunk in voice.chunks(441) {
            out.extend_from_slice(aec.process(mono(chunk.to_vec())).samples.as_ref());
        }
        let latency = block_len(RATE);
        assert!(
            out[..latency].iter().all(|v| *v == 0.0),
            "the first block is the delay"
        );
        assert_eq!(&out[latency..], &voice[..voice.len() - latency]);
    }

    /// A closed canceller, or a frame in another format, changes nothing.
    #[test]
    fn a_closed_canceller_or_another_format_changes_nothing() {
        let closed = EchoCanceller::default();
        assert!(!closed.is_open());
        let frame = mono(vec![0.5; 960]);
        assert!(!closed.far_end(frame.clone()));
        assert_eq!(closed.process(frame.clone()), frame);
        let aec = EchoCanceller::create(RATE, 200);
        let mut stereo = mono(vec![0.25; 960]);
        stereo.channels = 2;
        assert!(!aec.far_end(stereo.clone()));
        assert_eq!(aec.process(stereo.clone()), stereo);
        let mut other_rate = mono(vec![0.25; 441]);
        other_rate.sample_rate = 44_100;
        assert_eq!(aec.process(other_rate.clone()), other_rate);
        assert!(!EchoCanceller::create(0, 200).is_open());
        assert!(!EchoCanceller::create(RATE, 0).is_open());
    }
}
