//! When the recorder pauses itself and what it says about the disk. No window, no azul, a clock the
//! test moves by passing numbers.

use super::*;
use crate::recorder_settings::{Quality, AUDIO_KBPS};

const MIN: u64 = 60_000_000;

// ---- the disk ----

#[test]
fn the_disk_is_full_under_what_the_recorder_keeps_free_and_low_under_half_as_much_again() {
    let keep = 10 * GB;
    assert_eq!(disk_state(None, keep), DiskState::Unknown);
    assert_eq!(disk_state(Some(0), keep), DiskState::Full);
    assert_eq!(disk_state(Some(keep - 1), keep), DiskState::Full, "one byte under: pause");
    assert_eq!(disk_state(Some(keep), keep), DiskState::Low, "exactly what is kept is not full");
    assert_eq!(disk_state(Some(16 * GB - 1), keep), DiskState::Low);
    assert_eq!(disk_state(Some(16 * GB), keep), DiskState::Fine, "10 + 5 + 1 GB");
    assert_eq!(disk_state(Some(500 * GB), keep), DiskState::Fine);
}

#[test]
fn a_recording_paused_for_the_disk_goes_on_only_with_a_margin() {
    let keep = 10 * GB;
    assert!(!may_resume_after_quota(None, keep), "an unknown disk never resumes by itself");
    assert!(!may_resume_after_quota(Some(keep), keep));
    assert!(!may_resume_after_quota(Some(keep + GB - 1), keep));
    assert!(may_resume_after_quota(Some(keep + GB), keep));
}

#[test]
fn the_hours_left_are_the_room_over_the_rate() {
    assert_eq!(hours_left(Some(40 * GB), 10 * GB, 3 * GB), Some(10.0));
    assert_eq!(hours_left(Some(11 * GB), 10 * GB, 2 * GB), Some(0.5));
    assert_eq!(hours_left(Some(9 * GB), 10 * GB, GB), None, "already under: no hours");
    assert_eq!(hours_left(Some(40 * GB), 10 * GB, 0), None, "no rate to go by");
    assert_eq!(hours_left(None, 10 * GB, GB), None);
}

#[test]
fn what_a_recording_will_write_is_estimated_from_its_settings() {
    let all = RecorderSettings::default();
    let both = expected_bytes_per_hour(&all, (1_920, 1_248));
    // The display at 1920 wide: a quarter of 2.6 Mbit/s for the screen, the camera at 800 kbit/s, the voice
    // at 32: ~0.67 GB an hour.
    assert!((650_000_000..750_000_000).contains(&both), "{both}");

    let voice_only = RecorderSettings { screen: false, camera: false, ..RecorderSettings::default() };
    assert_eq!(expected_bytes_per_hour(&voice_only, (1_920, 1_248)), u64::from(AUDIO_KBPS) * 1_000 / 8 * 3_600);
    let nothing = RecorderSettings { screen: false, camera: false, microphone: false, ..RecorderSettings::default() };
    assert_eq!(expected_bytes_per_hour(&nothing, (100, 100)), 0);

    let low = RecorderSettings { quality: Quality::Low, ..RecorderSettings::default() };
    assert!(expected_bytes_per_hour(&low, (1_920, 1_248)) < both);
}

// ---- the rate ----

#[test]
fn a_rate_is_not_believed_before_it_spans_half_a_minute() {
    let mut m = RateMeter::new(10 * MIN);
    assert_eq!(m.bytes_per_hour(), None, "nothing measured");
    m.record(0, 0);
    assert_eq!(m.bytes_per_hour(), None, "one sample");
    m.record(10_000_000, 10_000_000);
    assert_eq!(m.bytes_per_hour(), None, "ten seconds");
    m.record(30_000_000, 30_000_000);
    assert_eq!(m.bytes_per_hour(), Some(3_600_000_000), "1 MB a second for 30 s is 3.6 GB an hour");
}

#[test]
fn the_rate_follows_the_last_window_not_the_whole_session() {
    let mut m = RateMeter::new(2 * MIN);
    // 1 MB a second for five minutes, then nothing for three.
    for s in 0..=300_u64 {
        m.record(s * 1_000_000, s * 1_000_000);
    }
    assert_eq!(m.bytes_per_hour(), Some(3_600_000_000));
    for s in 301..=480_u64 {
        m.record(s * 1_000_000, 300 * 1_000_000);
    }
    assert_eq!(m.bytes_per_hour(), Some(0), "the last two minutes wrote nothing");
}

#[test]
fn the_meter_keeps_a_bounded_number_of_samples() {
    let mut m = RateMeter::new(2 * MIN);
    for s in 0..10_000_u64 {
        m.record(s * 1_000_000, s * 1_000);
    }
    assert!(m.samples.len() <= 125, "{} samples kept for a window of 120 s of one-second samples", m.samples.len());
    assert_eq!(m.bytes_per_hour(), Some(3_600_000_000 / 1_000));
}

// ---- idle ----

#[test]
fn a_stretch_without_activity_pauses_once_and_the_next_activity_resumes() {
    let mut w = IdleWatch::new(Some(5 * MIN), 0);
    assert_eq!(w.activity(MIN), IdleChange::Nothing);
    assert_eq!(w.check(5 * MIN), IdleChange::Nothing, "four minutes idle: not yet");
    assert_eq!(w.check(6 * MIN), IdleChange::Paused { idle_us: 5 * MIN });
    assert!(w.is_paused());
    assert_eq!(w.check(30 * MIN), IdleChange::Nothing, "paused once, not again and again");
    assert_eq!(w.activity(31 * MIN), IdleChange::Resumed);
    assert!(!w.is_paused());
    assert_eq!(w.check(34 * MIN), IdleChange::Nothing, "the idle time counts from the last activity");
    assert_eq!(w.check(36 * MIN), IdleChange::Paused { idle_us: 5 * MIN });
}

#[test]
fn an_idle_pause_of_none_never_pauses() {
    let mut w = IdleWatch::new(None, 0);
    assert_eq!(w.check(1_000_000 * MIN), IdleChange::Nothing);
}

#[test]
fn activity_keeps_a_recording_awake_and_an_old_timestamp_does_not_move_the_clock_back() {
    let mut w = IdleWatch::new(Some(5 * MIN), 0);
    for m in 1..30 {
        let t = m * 3 * MIN; // something every three minutes, for an hour and a half
        assert_eq!(w.activity(t), IdleChange::Nothing);
        assert_eq!(w.check(t + 2 * MIN), IdleChange::Nothing);
    }
    w.activity(100 * MIN);
    w.activity(10 * MIN); // late news of an earlier moment
    assert_eq!(w.check(103 * MIN), IdleChange::Nothing, "counted from 100 minutes");
}

#[test]
fn a_resume_by_hand_starts_the_count_again() {
    let mut w = IdleWatch::new(Some(5 * MIN), 0);
    assert!(matches!(w.check(6 * MIN), IdleChange::Paused { .. }));
    w.reset(60 * MIN);
    assert!(!w.is_paused());
    assert_eq!(w.check(61 * MIN), IdleChange::Nothing, "not paused again at once");
}

// ---- length ----

#[test]
fn a_session_ends_at_its_length_and_is_warned_ten_minutes_before() {
    let max = 24 * 60 * MIN;
    assert_eq!(length_state(0, max), LengthState::Fine);
    assert_eq!(length_state(max - 11 * MIN, max), LengthState::Fine);
    assert_eq!(length_state(max - 10 * MIN, max), LengthState::Ending { left_us: 10 * MIN });
    assert_eq!(length_state(max - 1, max), LengthState::Ending { left_us: 1 });
    assert_eq!(length_state(max, max), LengthState::Over);
    assert_eq!(length_state(max + MIN, max), LengthState::Over);
}
