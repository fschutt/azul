use super::*;

/// A constant tone: `level` left, `level / 2` right, at `rate`, for `seconds`; it remembers
/// where it was sought to.
struct Tone {
    rate: u32,
    level: f32,
    left: usize,
    sought: Arc<std::sync::Mutex<Vec<f64>>>,
}

impl Source for Tone {
    fn seek(&mut self, seconds: f64) -> bool {
        self.sought.lock().unwrap().push(seconds);
        true
    }

    fn next(&mut self) -> Option<(u32, u16, Vec<f32>)> {
        if self.left == 0 {
            return None;
        }
        let n = self.left.min(512);
        self.left -= n;
        let mut samples = Vec::with_capacity(n * 2);
        for _ in 0..n {
            samples.push(self.level);
            samples.push(self.level / 2.0);
        }
        Some((self.rate, 2, samples))
    }
}

fn tone(rate: u32, level: f32, seconds: f64, sought: &Arc<std::sync::Mutex<Vec<f64>>>) -> Box<dyn Source> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let left = (seconds * f64::from(rate)) as usize;
    Box::new(Tone {
        rate,
        level,
        left,
        sought: sought.clone(),
    })
}

fn span(start_s: f64, end_s: f64, source_s: f64) -> Span {
    Span {
        media: 1,
        start_s,
        end_s,
        source_s,
    }
}

#[test]
fn a_clip_sounds_from_the_second_its_file_plays_at_the_playhead() {
    let sought = Arc::new(std::sync::Mutex::new(Vec::new()));
    let s = sought.clone();
    // A clip from 1 s to 3 s that starts 0.5 s into its file; playing starts at 2 s.
    let mut mixer = Mixer::new(vec![span(1.0, 3.0, 0.5)], 2.0, move |_| Some(tone(RATE, 0.5, 10.0, &s)));
    let block = mixer.block(BLOCK);
    assert_eq!(block.len(), BLOCK * 2, "stereo");
    assert!((block[0] - 0.5).abs() < 1e-6 && (block[1] - 0.25).abs() < 1e-6, "{:?}", &block[..2]);
    assert_eq!(sought.lock().unwrap().as_slice(), &[1.5], "1 s into the clip, 0.5 s in the file");
}

#[test]
fn nothing_sounds_outside_the_clips_and_the_mix_ends_after_the_last() {
    let sought = Arc::new(std::sync::Mutex::new(Vec::new()));
    let s = sought.clone();
    let mut mixer = Mixer::new(vec![span(0.5, 0.52, 0.0)], 0.0, move |_| Some(tone(RATE, 0.5, 1.0, &s)));
    // 0..0.5 s: silence, and the file is not opened yet.
    for _ in 0..25 {
        assert!(mixer.block(BLOCK).iter().all(|s| *s == 0.0));
    }
    assert!(sought.lock().unwrap().is_empty());
    assert!(!mixer.finished());
    let clip = mixer.block(BLOCK);
    assert!(clip.iter().all(|s| *s > 0.0), "the 20 ms clip fills the block");
    assert!(mixer.finished());
    assert!(mixer.block(BLOCK).iter().all(|s| *s == 0.0));
}

#[test]
fn another_rate_is_resampled_and_two_clips_add_up_inside_full_scale() {
    let sought = Arc::new(std::sync::Mutex::new(Vec::new()));
    let s = sought.clone();
    // A 24 kHz file plays at the same speed: one second of it lasts one second of output.
    let mut slow = Mixer::new(vec![span(0.0, 2.0, 0.0)], 0.0, move |_| Some(tone(24_000, 0.5, 1.0, &s)));
    let mut heard = 0;
    for _ in 0..100 {
        heard += slow.block(BLOCK).chunks(2).filter(|p| p[0] > 0.0).count();
    }
    assert!((47_000..=48_100).contains(&heard), "{heard} samples of a one second file");

    let s = sought.clone();
    let mut loud = Mixer::new(vec![span(0.0, 1.0, 0.0), span(0.0, 1.0, 0.0)], 0.0, move |_| {
        Some(tone(RATE, 0.75, 1.0, &s))
    });
    let block = loud.block(16);
    assert_eq!((block[0], block[1]), (1.0, 0.75), "0.75 + 0.75 is held at full scale");
}

#[test]
fn a_file_that_cannot_be_opened_is_silence_and_is_not_tried_again() {
    let mut tries = 0;
    let mut mixer = Mixer::new(vec![span(0.0, 1.0, 0.0)], 0.0, |_| {
        tries += 1;
        None
    });
    for _ in 0..3 {
        assert!(mixer.block(BLOCK).iter().all(|s| *s == 0.0));
    }
    drop(mixer);
    assert_eq!(tries, 1);
}
