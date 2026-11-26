//! The resampler (a tone keeps its pitch and loudness, what is above the target band does not
//! fold back into it) and the end-pointer (one utterance out of a room with noise, clicks and
//! pauses), on synthetic signals that are the same every run.

use super::{Endpointer, Progress, Resampler, Vad, MODEL_RATE};

/// A sine of `hz` and `amplitude`, `ms` long, at `rate`.
fn tone(rate: u32, hz: f64, amplitude: f32, ms: u32) -> Vec<f32> {
    let n = (u64::from(rate) * u64::from(ms) / 1000) as usize;
    (0..n)
        .map(|i| (amplitude as f64 * (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(rate)).sin()) as f32)
        .collect()
}

/// Room noise of about `rms`: a fixed pseudo-random sequence (the same in every run).
fn room(ms: u32, rms: f32) -> Vec<f32> {
    let n = (u64::from(MODEL_RATE) * u64::from(ms) / 1000) as usize;
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    (0..n)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // Uniform in -1..1 has an RMS of 1/sqrt(3).
            let uniform = (state >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
            (uniform * 3f64.sqrt()) as f32 * rms
        })
        .collect()
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

/// Feeds `signal` in chunks of 7 ms (not a multiple of a frame: carry-over is part of the test).
fn feed(endpointer: &mut Endpointer, signal: &[f32]) -> Progress {
    let mut progress = Progress::Waiting;
    for chunk in signal.chunks(112) {
        progress = endpointer.push(chunk);
    }
    progress
}

#[test]
fn the_model_rate_passes_through_untouched() {
    let mut resampler = Resampler::new(MODEL_RATE);
    let input = tone(MODEL_RATE, 440.0, 0.5, 100);
    assert_eq!(resampler.process(&input), input);
    assert_eq!(Resampler::new(0).process(&input), input, "an unknown rate is taken as the model's");
}

#[test]
fn a_tone_keeps_its_pitch_and_loudness_from_every_rate_the_hardware_has() {
    for rate in [48_000u32, 44_100, 32_000, 24_000, 8_000] {
        let mut resampler = Resampler::new(rate);
        // 1 kHz is below every Nyquist frequency here (8 kHz delivers up to 4 kHz).
        let input = tone(rate, 1_000.0, 0.5, 1_000);
        let mut out = Vec::new();
        // In chunks, as a stream comes.
        for chunk in input.chunks(480) {
            out.extend(resampler.process(chunk));
        }
        let expected = MODEL_RATE as usize;
        assert!((out.len() as i64 - expected as i64).abs() <= 80, "{rate} Hz: {} samples for 1 s", out.len());
        // Away from the two ends (the kernel has no input there): the amplitude is the input's.
        let middle = &out[2_000..out.len() - 2_000];
        let level = rms(middle) * std::f32::consts::SQRT_2;
        assert!((level - 0.5).abs() < 0.02, "{rate} Hz: amplitude {level}");
        // And the pitch: count the rising zero crossings (1 kHz is 1000 a second).
        let crossings = middle.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        let seconds = middle.len() as f32 / MODEL_RATE as f32;
        let hz = crossings as f32 / seconds;
        assert!((hz - 1_000.0).abs() < 15.0, "{rate} Hz: {hz} Hz");
    }
}

#[test]
fn what_is_above_the_models_band_is_filtered_and_does_not_fold_into_it() {
    // 12 kHz at 48 kHz is above the model's 8 kHz Nyquist frequency: naive decimation folds it
    // to 4 kHz as loud as it was.
    let mut resampler = Resampler::new(48_000);
    let out = resampler.process(&tone(48_000, 12_000.0, 0.5, 1_000));
    let middle = &out[2_000..out.len() - 2_000];
    assert!(rms(middle) * std::f32::consts::SQRT_2 < 0.02, "folded back: {}", rms(middle));
}

#[test]
fn resampling_in_chunks_gives_what_resampling_in_one_piece_gives() {
    let input = tone(44_100, 700.0, 0.3, 600);
    let whole = Resampler::new(44_100).process(&input);
    let mut resampler = Resampler::new(44_100);
    let mut pieces = Vec::new();
    for chunk in input.chunks(333) {
        pieces.extend(resampler.process(chunk));
    }
    let n = whole.len().min(pieces.len());
    assert!(n > 8_000);
    assert!((whole.len() as i64 - pieces.len() as i64).abs() <= 1);
    for i in 0..n {
        assert!((whole[i] - pieces[i]).abs() < 1e-4, "sample {i}");
    }
}

#[test]
fn silence_in_a_quiet_room_never_starts_an_utterance() {
    let mut endpointer = Endpointer::new(Vad::default());
    assert_eq!(feed(&mut endpointer, &room(3_000, 0.0004)), Progress::Waiting);
    assert_eq!(endpointer.take(), None);
    assert_eq!(endpointer.finish(), None);
    assert_eq!(endpointer.elapsed_ms(), 3_000);
}

#[test]
fn one_utterance_is_cut_out_with_a_little_before_and_after() {
    let mut endpointer = Endpointer::new(Vad::default());
    let mut signal = room(1_000, 0.0004);
    signal.extend(tone(MODEL_RATE, 250.0, 0.05, 1_200));
    signal.extend(room(1_500, 0.0004));
    assert_eq!(feed(&mut endpointer, &signal), Progress::Done);
    let segment = endpointer.take().expect("the utterance");
    // Speech runs from 1000 to 2200 ms; 300 ms before it and 300 ms after it are kept.
    assert!((segment.start_ms as i64 - 700).abs() <= 40, "starts at {} ms", segment.start_ms);
    let length_ms = segment.samples.len() as i64 * 1000 / i64::from(MODEL_RATE);
    assert!((length_ms - 1_800).abs() <= 60, "{length_ms} ms long");
    // The speech is in it.
    assert!(rms(&segment.samples) > 0.02);
}

#[test]
fn a_click_is_not_speech_and_does_not_leave_the_end_pointer_waiting_for_its_end() {
    let mut endpointer = Endpointer::new(Vad::default());
    let mut signal = room(800, 0.0004);
    signal.extend(tone(MODEL_RATE, 1_000.0, 0.3, 40));
    signal.extend(room(2_000, 0.0004));
    assert_eq!(feed(&mut endpointer, &signal), Progress::Waiting);
    assert_eq!(endpointer.take(), None);
}

#[test]
fn a_pause_inside_the_sentence_does_not_split_it() {
    let mut endpointer = Endpointer::new(Vad::default());
    let mut signal = room(500, 0.0004);
    signal.extend(tone(MODEL_RATE, 250.0, 0.05, 700));
    signal.extend(room(450, 0.0004)); // a breath: shorter than the 800 ms that end an utterance
    signal.extend(tone(MODEL_RATE, 300.0, 0.05, 700));
    signal.extend(room(1_200, 0.0004));
    assert_eq!(feed(&mut endpointer, &signal), Progress::Done);
    let segment = endpointer.take().expect("one utterance");
    let length_ms = segment.samples.len() as i64 * 1000 / i64::from(MODEL_RATE);
    // 300 before + 700 + 450 + 700 + 300 after.
    assert!((length_ms - 2_450).abs() <= 80, "{length_ms} ms long");
}

#[test]
fn a_stop_in_the_middle_gives_the_speech_so_far() {
    let mut endpointer = Endpointer::new(Vad::default());
    let mut signal = room(600, 0.0004);
    signal.extend(tone(MODEL_RATE, 250.0, 0.05, 900));
    assert_eq!(feed(&mut endpointer, &signal), Progress::Speaking);
    assert_eq!(endpointer.take(), None, "not over yet");
    let segment = endpointer.finish().expect("what was said so far");
    assert!((segment.start_ms as i64 - 300).abs() <= 40, "{}", segment.start_ms);
    assert!(segment.samples.len() as i64 * 1000 / i64::from(MODEL_RATE) >= 1_100);
}

#[test]
fn a_noisy_room_is_learned_and_a_new_call_starts_from_what_the_last_one_learned() {
    // Fan noise at 0.004 RMS is above the least energy of speech: the first frames look like
    // speech, then the three-second minimum shows what the room is.
    let noisy = room(4_000, 0.004);
    let mut first = Endpointer::new(Vad::default());
    feed(&mut first, &noisy);
    assert!(first.noise_floor() > 0.002 && first.noise_floor() < 0.006, "{}", first.noise_floor());

    // The next call starts from that floor: the same noise is not speech, a voice above it is.
    let mut next = Endpointer::with_floor(Vad::default(), first.noise_floor());
    assert_eq!(feed(&mut next, &room(2_000, 0.004)), Progress::Waiting);
    let mut voice = tone(MODEL_RATE, 250.0, 0.12, 1_000);
    voice.extend(room(1_500, 0.004));
    assert_eq!(feed(&mut next, &voice), Progress::Done);
    assert!(next.take().is_some());
}

#[test]
fn nonsense_starting_floors_are_kept_to_what_a_room_can_be() {
    for floor in [f32::NAN, f32::INFINITY, -1.0, 1e9] {
        let endpointer = Endpointer::with_floor(Vad::default(), floor);
        let kept = endpointer.noise_floor();
        assert!(kept.is_finite() && (0.0..=0.05).contains(&kept), "{floor} became {kept}");
    }
}

#[test]
fn a_burst_in_the_room_after_the_speech_does_not_hold_the_end_off() {
    // The loudspeaker's tail and clicks: 100 ms of 0.004 RMS now and then, above the 0.0015 that
    // can be speech but a small part of the 0.035 the voice had.
    let mut endpointer = Endpointer::new(Vad::default());
    let mut signal = room(500, 0.0004);
    signal.extend(tone(MODEL_RATE, 250.0, 0.05, 1_000));
    signal.extend(room(300, 0.0004));
    signal.extend(tone(MODEL_RATE, 800.0, 0.0057, 100)); // RMS 0.004
    signal.extend(room(400, 0.0004));
    signal.extend(tone(MODEL_RATE, 800.0, 0.0057, 100));
    signal.extend(room(1_500, 0.0004));
    assert_eq!(feed(&mut endpointer, &signal), Progress::Done);
    let segment = endpointer.take().expect("the utterance");
    let length_ms = segment.samples.len() as i64 * 1000 / i64::from(MODEL_RATE);
    // 300 before + 1000 of speech + 300 after: the bursts are not part of it.
    assert!((length_ms - 1_600).abs() <= 80, "{length_ms} ms long");
    // And it ended on time: 800 ms after the speech, not after the last burst.
    assert!(endpointer.elapsed_ms() < 500 + 1_000 + 800 + 100, "ended at {} ms", endpointer.elapsed_ms());
}

#[test]
fn quiet_speech_is_not_cut_by_the_continuing_threshold() {
    // A quiet speaker (RMS 0.0035): the threshold for the speech going on stays the room's.
    let mut endpointer = Endpointer::new(Vad::default());
    let mut signal = room(400, 0.0004);
    signal.extend(tone(MODEL_RATE, 250.0, 0.005, 1_500));
    signal.extend(room(1_500, 0.0004));
    assert_eq!(feed(&mut endpointer, &signal), Progress::Done);
    let length_ms = endpointer.take().expect("the utterance").samples.len() as i64 * 1000 / i64::from(MODEL_RATE);
    assert!((length_ms - 2_100).abs() <= 80, "{length_ms} ms long");
}
