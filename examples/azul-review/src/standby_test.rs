//! The microphone's standby: when the window asks the speech session to open it ahead of a dictation and when to close
//! it. A time the test passes in and no session: nothing here opens a microphone.

use super::*;

const IDLE: u64 = 30_000;

fn arming(standby: Standby) -> Arming {
    Arming::new(Config { standby, idle_ms: IDLE })
}

#[test]
fn the_choices_have_names_a_setting_and_a_switch_can_say_and_the_pen_near_is_the_default() {
    assert_eq!(Standby::default(), Standby::Near);
    let names: Vec<&str> = Standby::ALL.iter().map(|s| s.name()).collect();
    assert_eq!(names, vec!["off", "near", "always"]);
    for standby in Standby::ALL {
        assert_eq!(Standby::parse(standby.name()), Some(standby));
        assert_eq!(Standby::parse(&format!("  {}\n", standby.name().to_uppercase())), Some(standby));
    }
    assert_eq!(Standby::parse("sometimes"), None);
    assert_eq!(Standby::parse(""), None);
}

#[test]
fn an_idle_time_is_capped_into_what_a_person_can_mean_with_a_word_about_it() {
    assert_eq!(cap_idle_ms(IDLE), (IDLE, None));
    assert_eq!(cap_idle_ms(MIN_IDLE_MS), (MIN_IDLE_MS, None));
    assert_eq!(cap_idle_ms(MAX_IDLE_MS), (MAX_IDLE_MS, None));
    let (low, why) = cap_idle_ms(100);
    assert_eq!(low, MIN_IDLE_MS);
    assert!(why.is_some_and(|w| w.contains("100 ms") && w.contains("closing")), "the word names the value and what it was for");
    let (high, why) = cap_idle_ms(24 * 3_600_000);
    assert_eq!(high, MAX_IDLE_MS);
    assert!(why.is_some());
}

#[test]
fn a_window_that_keeps_nothing_ready_never_arms() {
    let mut a = arming(Standby::Off);
    assert_eq!(a.start(0), None);
    assert_eq!(a.near(10), None);
    assert_eq!(a.near(20), None);
    assert_eq!(a.tick(1_000_000), None);
    assert!(!a.is_armed());
}

#[test]
fn the_pen_coming_near_arms_once_and_its_moves_do_not_ask_again() {
    let mut a = arming(Standby::Near);
    assert_eq!(a.start(0), None, "nothing is open before the pen is near");
    assert_eq!(a.tick(5_000), None);
    assert!(!a.is_armed());
    assert_eq!(a.near(6_000), Some(Action::Arm));
    assert!(a.is_armed());
    assert_eq!(a.near(6_016), None);
    assert_eq!(a.near(6_032), None);
}

#[test]
fn a_pen_that_has_been_away_for_the_idle_time_closes_it_and_the_next_move_opens_it_again() {
    let mut a = arming(Standby::Near);
    assert_eq!(a.near(1_000), Some(Action::Arm));
    assert_eq!(a.tick(1_000 + IDLE - 1), None);
    assert_eq!(a.tick(1_000 + IDLE), Some(Action::Disarm));
    assert!(!a.is_armed());
    assert_eq!(a.tick(1_000 + IDLE + 1), None, "closed once");
    assert_eq!(a.near(100_000), Some(Action::Arm));
}

#[test]
fn every_move_of_the_pen_pushes_the_closing_back() {
    let mut a = arming(Standby::Near);
    assert_eq!(a.near(0), Some(Action::Arm));
    assert_eq!(a.near(20_000), None);
    assert_eq!(a.tick(IDLE), None, "the pen moved at 20 s: the idle time counts from there");
    assert_eq!(a.tick(20_000 + IDLE), Some(Action::Disarm));
}

#[test]
fn a_pen_out_of_the_tablets_range_closes_it_after_a_short_grace_and_one_back_in_time_keeps_it() {
    let mut a = arming(Standby::Near);
    assert_eq!(a.near(0), Some(Action::Arm));
    a.out_of_range(1_000);
    assert_eq!(a.tick(1_000 + AWAY_GRACE_MS - 1), None);
    assert_eq!(a.tick(1_000 + AWAY_GRACE_MS), Some(Action::Disarm));

    let mut b = arming(Standby::Near);
    assert_eq!(b.near(0), Some(Action::Arm));
    b.out_of_range(1_000);
    assert_eq!(b.near(1_000 + AWAY_GRACE_MS - 1), None, "the pen is back");
    assert_eq!(b.tick(1_000 + AWAY_GRACE_MS), None, "the grace is over for a pen that is here");
    assert!(b.is_armed());
}

#[test]
fn the_grace_never_outlasts_the_idle_time() {
    let mut a = Arming::new(Config { standby: Standby::Near, idle_ms: MIN_IDLE_MS });
    assert_eq!(a.near(0), Some(Action::Arm));
    a.out_of_range(MIN_IDLE_MS - 100);
    assert_eq!(a.tick(MIN_IDLE_MS - 1), None);
    assert_eq!(a.tick(MIN_IDLE_MS), Some(Action::Disarm), "the idle time was shorter than the grace");
}

#[test]
fn a_window_that_is_no_longer_in_front_closes_it_at_once_and_the_pen_back_opens_it_again() {
    let mut a = arming(Standby::Near);
    assert_eq!(a.near(0), Some(Action::Arm));
    assert_eq!(a.deactivated(500), Some(Action::Disarm), "the system's indicator is off with the window");
    assert_eq!(a.deactivated(600), None);
    assert_eq!(a.near(10_000), Some(Action::Arm));
    assert_eq!(a.tick(10_001), None, "back in front: the idle time counts again");
}

#[test]
fn a_dictation_in_use_is_never_closed_under_it_and_the_idle_time_counts_from_its_end() {
    let mut a = arming(Standby::Near);
    assert_eq!(a.near(0), Some(Action::Arm));
    assert_eq!(a.busy(true, 1_000), None);
    assert_eq!(a.tick(10 * IDLE), None, "a long dictation");
    assert_eq!(a.deactivated(10 * IDLE), None, "not even by the window going to the back");
    assert!(a.is_armed());
    assert_eq!(a.busy(false, 10 * IDLE + 5), Some(Action::Disarm), "the window went to the back meanwhile");
}

#[test]
fn a_dictation_that_ends_with_the_pen_still_near_leaves_it_open_for_the_idle_time() {
    let mut a = arming(Standby::Near);
    assert_eq!(a.near(0), Some(Action::Arm));
    assert_eq!(a.busy(true, 100), None);
    assert_eq!(a.near(50_000), None, "the pen moves while it listens");
    assert_eq!(a.busy(false, 60_000), None);
    assert_eq!(a.tick(60_000 + IDLE - 1), None);
    assert_eq!(a.tick(60_000 + IDLE), Some(Action::Disarm));
}

#[test]
fn a_dictation_that_began_cold_does_not_arm_the_session_it_is_using() {
    let mut a = arming(Standby::Near);
    assert_eq!(a.busy(true, 0), None);
    assert_eq!(a.near(10), None, "the dictation's own call opens the microphone");
    assert!(!a.is_armed());
    assert_eq!(a.busy(false, 20_000), Some(Action::Arm), "the pen was near while it listened: ready for the next one");
}

#[test]
fn always_arms_at_the_start_and_nothing_closes_it() {
    let mut a = arming(Standby::Always);
    assert_eq!(a.start(0), Some(Action::Arm));
    assert_eq!(a.start(1), None);
    assert_eq!(a.tick(100 * IDLE), None);
    a.out_of_range(10);
    assert_eq!(a.tick(100 * IDLE), None);
    assert_eq!(a.deactivated(100 * IDLE), None, "an explicit choice: the indicator stays on");
    assert!(a.is_armed());
}

#[test]
fn a_choice_made_in_the_settings_applies_at_once() {
    let mut a = arming(Standby::Near);
    assert_eq!(a.near(0), Some(Action::Arm));
    assert_eq!(a.set_config(Config { standby: Standby::Off, idle_ms: IDLE }, 100), Some(Action::Disarm));
    assert_eq!(a.near(200), None);

    assert_eq!(a.set_config(Config { standby: Standby::Always, idle_ms: IDLE }, 300), Some(Action::Arm));
    // Back to "when the pen is near": it is open now and closes after the idle time unless the pen moves.
    assert_eq!(a.set_config(Config { standby: Standby::Near, idle_ms: IDLE }, 400), None);
    assert!(a.is_armed());
    assert_eq!(a.tick(400 + IDLE - 1), None);
    assert_eq!(a.tick(400 + IDLE), Some(Action::Disarm));
}

#[test]
fn a_new_idle_time_applies_to_the_pen_that_is_away() {
    let mut a = arming(Standby::Near);
    assert_eq!(a.near(0), Some(Action::Arm));
    assert_eq!(a.set_config(Config { standby: Standby::Near, idle_ms: MIN_IDLE_MS }, 10_000), Some(Action::Disarm));
}

#[test]
fn a_setting_chosen_while_a_dictation_listens_waits_for_its_end() {
    let mut a = arming(Standby::Always);
    assert_eq!(a.start(0), Some(Action::Arm));
    assert_eq!(a.busy(true, 10), None);
    assert_eq!(a.set_config(Config { standby: Standby::Off, idle_ms: IDLE }, 20), None);
    assert!(a.is_armed(), "the session is in use");
    assert_eq!(a.busy(false, 30), Some(Action::Disarm));
}

#[test]
fn times_far_in_the_future_do_not_wrap() {
    let mut a = Arming::new(Config { standby: Standby::Near, idle_ms: MAX_IDLE_MS });
    assert_eq!(a.near(u64::MAX - 10), Some(Action::Arm));
    a.out_of_range(u64::MAX - 5);
    assert_eq!(a.tick(u64::MAX - 4), None);
}

#[test]
fn the_actions_have_names_a_script_can_read() {
    assert_eq!(Action::Arm.name(), "armed");
    assert_eq!(Action::Disarm.name(), "disarmed");
}
