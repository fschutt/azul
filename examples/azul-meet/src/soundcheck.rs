//! The sound check before a meeting, in the waiting room and in Settings > Audio & Video.
//!
//! "Test speakers" plays a short chime through the speaker. "Record a test clip" records a few
//! seconds of the microphone and plays them straight back, so a person hears what the others
//! will hear. This file holds the rules only, free of azul types: the app feeds it the
//! microphone's chunks and its clock, and plays the packets it hands out on the playout thread.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio;

    const RATE: u32 = 48_000;

    /// Feeds `ms` of a constant `value` in 20 ms chunks, as the microphone widget sends them.
    fn feed_ms(check: &mut SoundCheck, channels: u16, value: f32, ms: u64) {
        let chunk = audio::samples_per_packet(RATE) * usize::from(channels);
        for _ in 0..ms / 20 {
            check.feed(RATE, channels, &vec![value; chunk]);
        }
    }

    #[test]
    fn a_test_clip_records_three_seconds_and_then_plays_them_back() {
        let mut check = SoundCheck::new();
        check.start_recording(1_000);
        assert!(check.records());
        assert_eq!(check.seconds_left(1_000), 3);
        feed_ms(&mut check, 1, 0.1, 3_500);
        assert_eq!(check.tick(3_999), Tick::Nothing, "the clip is not over yet");
        let Tick::Play(rate, packets) = check.tick(4_000) else {
            panic!("the clip plays once it is recorded");
        };
        assert_eq!(rate, RATE);
        let clip_packets = (CLIP_MS / u64::from(audio::PACKET_MS)) as usize;
        assert_eq!(packets.len(), clip_packets + TAIL_PACKETS, "three seconds, then silence");
        assert!(packets[0].iter().all(|s| *s == audio::to_pcm16(0.1)));
        assert!(packets[clip_packets].iter().all(|s| *s == 0));
        let Phase::Playing { until, clip: true } = check.phase() else {
            panic!("the clip plays");
        };
        assert_eq!(check.tick(until - 1), Tick::Nothing);
        assert_eq!(check.tick(until), Tick::Ended);
        assert_eq!(check.phase(), Phase::Idle);
        assert_eq!(check.outcome(), Outcome::PlayedClip);
    }

    #[test]
    fn a_test_clip_of_only_silence_says_the_microphone_heard_nothing() {
        let mut check = SoundCheck::new();
        check.start_recording(0);
        feed_ms(&mut check, 1, 0.0, 3_000);
        assert_eq!(check.tick(CLIP_MS), Tick::Ended, "nothing worth playing back");
        assert_eq!(check.outcome(), Outcome::Quiet);
        assert_eq!(check.phase(), Phase::Idle);
    }

    #[test]
    fn a_recording_without_a_microphone_ends_by_itself() {
        let mut check = SoundCheck::new();
        check.start_recording(0);
        assert_eq!(check.tick(CLIP_MS), Tick::Ended);
        assert_eq!(check.outcome(), Outcome::NoMicrophone);
        assert!(!check.records());
    }

    #[test]
    fn the_speaker_test_plays_a_short_chime_and_ends_by_itself() {
        let mut check = SoundCheck::new();
        let packets = check.start_chime(RATE, 100);
        let chime_packets = (CHIME_MS / u64::from(audio::PACKET_MS)) as usize;
        assert_eq!(packets.len(), chime_packets + TAIL_PACKETS);
        let loudest = packets.iter().flatten().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        assert!(loudest > 0, "the chime is heard");
        assert!(
            loudest <= audio::to_pcm16(audio::TONE_AMPLITUDE).unsigned_abs(),
            "and no louder than the test tone"
        );
        assert_eq!(packets[0][0], 0, "it fades in");
        let Phase::Playing { until, clip: false } = check.phase() else {
            panic!("the chime plays");
        };
        assert_eq!(until, 100 + packets.len() as u64 * u64::from(audio::PACKET_MS));
        assert_eq!(check.tick(until - 1), Tick::Nothing);
        assert_eq!(check.tick(until), Tick::Ended);
        assert_eq!(check.outcome(), Outcome::PlayedChime);
    }

    #[test]
    fn a_new_check_replaces_the_one_running() {
        let mut check = SoundCheck::new();
        check.start_recording(0);
        feed_ms(&mut check, 1, 0.1, 1_000);
        let _ = check.start_chime(RATE, 1_000);
        assert!(!check.records(), "the chime stopped the recording");
        check.start_recording(1_100);
        assert!(check.records());
        assert_eq!(check.tick(1_100 + CLIP_MS), Tick::Ended, "the old clip is gone");
        assert_eq!(check.outcome(), Outcome::NoMicrophone);
        check.start_recording(9_000);
        check.cancel();
        assert_eq!(check.phase(), Phase::Idle);
    }

    #[test]
    fn a_stereo_microphone_is_mixed_to_mono_and_a_chunk_at_another_rate_is_skipped() {
        let mut check = SoundCheck::new();
        check.start_recording(0);
        let chunk = audio::samples_per_packet(RATE);
        let stereo: Vec<f32> = (0..chunk).flat_map(|_| [0.2, 0.0]).collect();
        for _ in 0..CLIP_MS / 20 {
            check.feed(RATE, 2, &stereo);
            check.feed(44_100, 1, &[0.9; 882]);
        }
        let Tick::Play(rate, packets) = check.tick(CLIP_MS) else {
            panic!("the clip plays");
        };
        assert_eq!(rate, RATE);
        assert!(packets[0].iter().all(|s| *s == audio::to_pcm16(0.1)));
    }
}
