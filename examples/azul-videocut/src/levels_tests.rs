//! The audio meters' levels: a file's envelope per channel, the reading at the playhead from
//! the unmuted audio tracks, and the bar and the number the meters show for it.

use std::collections::HashMap;

use super::*;
use crate::model::{Edit, MediaItem, Pattern, Project, TrackKind};

/// 100 sample frames per slice at 5 kHz (20 ms).
const RATE: u32 = 5_000;

#[test]
fn a_stereo_file_keeps_each_channels_loudest_sample_per_slice() {
    let mut b = EnvelopeBuilder::default();
    // First slice: left peaks at 0.5, right at -0.25; second slice: right at 1.0.
    let mut first = vec![0.0f32; 200];
    first[10] = 0.5;
    first[21] = -0.25;
    let mut second = vec![0.0f32; 200];
    second[51] = 1.0;
    b.push(RATE, 2, &first);
    b.push(RATE, 2, &second);
    let env = b.finish();
    assert_eq!(env.peaks, vec![[0.5, 0.25], [0.0, 1.0]]);
}

#[test]
fn a_mono_file_shows_the_same_level_on_both_meters_and_a_short_tail_is_a_slice() {
    let mut b = EnvelopeBuilder::default();
    let mut samples = vec![0.0f32; 130];
    samples[120] = -0.8;
    b.push(RATE, 1, &samples);
    let env = b.finish();
    assert_eq!(env.peaks, vec![[0.0, 0.0], [0.8, 0.8]], "100 frames, then a tail of 30");
}

#[test]
fn a_frame_split_over_two_pushes_still_counts_whole_slices() {
    let mut b = EnvelopeBuilder::default();
    let mut samples = vec![0.0f32; 60];
    samples[59] = 0.3;
    b.push(RATE, 1, &samples);
    let mut rest = vec![0.0f32; 40];
    rest[0] = 0.6;
    b.push(RATE, 1, &rest);
    assert_eq!(b.finish().peaks, vec![[0.6, 0.6]]);
}

#[test]
fn a_level_outside_the_file_is_silence() {
    let env = Envelope {
        peaks: vec![[0.5, 0.5], [0.25, 0.75]],
    };
    assert_eq!(env.at(0.0), [0.5, 0.5]);
    assert_eq!(env.at(0.03), [0.25, 0.75]);
    assert_eq!(env.at(0.05), [0.0, 0.0]);
    assert_eq!(env.at(-1.0), [0.0, 0.0]);
}

/// 25 fps; "tone" on A1 from frame 10, playing its file from its second 1 (frame 25).
fn project() -> (Project, u64) {
    let mut p = Project::create("p-1".into(), "Teaser".into(), 640, 360, 25);
    let mut tone = MediaItem::generated("tone", Pattern::Bars, 100, 0, 0);
    tone.has_video = false;
    tone.has_audio = true;
    let tone = p.add_media(tone);
    let a1 = p
        .sequence
        .tracks
        .iter()
        .position(|t| t.kind == TrackKind::Audio)
        .expect("A1");
    let clip = p.clip_from_media(tone, 25, 50);
    p.edit(Edit::Overwrite {
        track: a1,
        at: 10,
        clip,
    })
    .expect("overwrite A1");
    (p, tone)
}

#[test]
fn the_reading_at_the_playhead_is_the_clips_level_at_its_place_in_the_file() {
    let (mut p, tone) = project();
    // One second in 50 slices; slice 60 (1.2 s) is loud on the left.
    let mut peaks = vec![[0.1, 0.1]; 150];
    peaks[60] = [0.9, 0.2];
    let envelopes = HashMap::from([(tone, Envelope { peaks })]);
    // Frame 15 plays the file's frame 30: 1.2 s.
    assert_eq!(reading(&p, 15, &envelopes), [0.9, 0.2]);
    assert_eq!(reading(&p, 5, &envelopes), [0.0, 0.0], "before the clip: silence");
    assert_eq!(reading(&p, 15, &HashMap::new()), [0.0, 0.0], "its levels not read yet");
    let a1 = p
        .sequence
        .tracks
        .iter()
        .position(|t| t.kind == TrackKind::Audio)
        .unwrap();
    p.edit(Edit::SetTrackHidden {
        track: a1,
        hidden: true,
    })
    .unwrap();
    assert_eq!(reading(&p, 15, &envelopes), [0.0, 0.0], "a muted track is silent");
}

#[test]
fn the_bar_and_the_number_follow_the_level_in_decibels() {
    assert_eq!(fill(1.0), 1.0);
    assert_eq!(fill(0.0), 0.0);
    assert!((fill(0.5) - 0.9).abs() < 0.01, "-6 dB of a 60 dB scale: {}", fill(0.5));
    assert_eq!(fill(0.0001), 0.0, "-80 dB is under the floor");
    assert_eq!(peak_db(0.5), Some(-6));
    assert_eq!(peak_db(1.0), Some(0));
    assert_eq!(peak_db(0.0), None);
    assert_eq!(peak_db(0.0005), None, "under the floor reads as silence");
}
