use alloc::{string::String, vec, vec::Vec};

use azul_css::system::AudioMetrics;

use super::*;

/// Both OS switches on: what a phone reports with "Touch sounds" enabled and the ringer up.
const SWITCHES_ON: AudioMetrics = AudioMetrics {
    event_sounds_enabled: true,
    input_feedback_sounds_enabled: true,
};

/// The route of `sound` on `platform` with the app's sounds on and the OS switches on.
fn plain(sound: SystemSound, platform: SoundPlatform) -> SoundRoute {
    route(sound, platform, SoundConfig::default(), SWITCHES_ON)
}

fn labels(platform: SoundPlatform) -> Vec<(&'static str, String)> {
    ALL_SYSTEM_SOUNDS
        .iter()
        .map(|sound| (sound.name(), plain(*sound, platform).label()))
        .collect()
}

fn label_of(sound: SystemSound, platform: SoundPlatform) -> String {
    plain(sound, platform).label()
}

#[test]
fn the_list_of_all_sounds_is_in_declaration_order_and_complete() {
    assert_eq!(ALL_SYSTEM_SOUNDS.len(), SYSTEM_SOUND_COUNT);
    for (position, sound) in ALL_SYSTEM_SOUNDS.iter().enumerate() {
        assert_eq!(
            sound.index(),
            position,
            "{sound:?} is not at its own index of ALL_SYSTEM_SOUNDS"
        );
    }
    // the last variant of the enum is the last of the list: a new variant has to be added to both
    assert_eq!(ALL_SYSTEM_SOUNDS[SYSTEM_SOUND_COUNT - 1], SystemSound::ScreenCapture);
    assert_eq!(SystemSound::ScreenCapture.index(), SYSTEM_SOUND_COUNT - 1);
}

#[test]
fn a_sound_is_found_by_its_name_in_any_case_and_an_unknown_name_is_not_a_sound() {
    for sound in &ALL_SYSTEM_SOUNDS {
        assert_eq!(SystemSound::from_name(sound.name()), Some(*sound));
        assert_eq!(
            SystemSound::from_name(&sound.name().to_ascii_lowercase()),
            Some(*sound)
        );
        assert_eq!(
            SystemSound::from_name(&alloc::format!("  {}  ", sound.name())),
            Some(*sound),
            "the surrounding spaces of a name are ignored"
        );
    }
    assert_eq!(SystemSound::from_name("Thunk"), None);
    assert_eq!(SystemSound::from_name(""), None);
}

#[test]
fn thirteen_sounds_are_input_feedback_and_twelve_are_events() {
    let input = ALL_SYSTEM_SOUNDS
        .iter()
        .filter(|sound| sound.class() == SoundClass::InputFeedback)
        .count();
    assert_eq!(input, 13);
    assert_eq!(SYSTEM_SOUND_COUNT - input, 12);
    assert_eq!(SystemSound::TabChange.class(), SoundClass::InputFeedback);
    assert_eq!(SystemSound::Notification.class(), SoundClass::Event);
    assert_eq!(SystemSound::CameraShutter.class(), SoundClass::Event);
}

#[test]
fn every_fallback_chain_ends_and_none_loops() {
    for start in &ALL_SYSTEM_SOUNDS {
        let mut seen = vec![*start];
        let mut current = start.fallback();
        while let Some(next) = current {
            assert!(
                !seen.contains(&next),
                "the fallback chain of {start:?} loops at {next:?}"
            );
            seen.push(next);
            assert!(seen.len() <= SYSTEM_SOUND_COUNT, "the chain of {start:?} is too long");
            current = next.fallback();
        }
    }
}

#[test]
fn the_fallbacks_follow_the_meaning() {
    assert_eq!(SystemSound::DropReject.fallback(), Some(SystemSound::Error));
    assert_eq!(SystemSound::DropAccept.fallback(), Some(SystemSound::Click));
    assert_eq!(SystemSound::TrashEmpty.fallback(), Some(SystemSound::Delete));
    assert_eq!(SystemSound::ScreenCapture.fallback(), Some(SystemSound::CameraShutter));
    assert_eq!(SystemSound::Question.fallback(), Some(SystemSound::Info));
    assert_eq!(SystemSound::TabChange.fallback(), Some(SystemSound::Tick));
    assert_eq!(SystemSound::Tick.fallback(), Some(SystemSound::Click));
    assert_eq!(SystemSound::Click.fallback(), None);
}

#[test]
fn the_haptic_idiom_is_a_selection_for_the_toggles_and_the_notification_haptics_for_the_results() {
    for sound in [
        SystemSound::Tick,
        SystemSound::ToggleOn,
        SystemSound::ToggleOff,
        SystemSound::TabChange,
    ] {
        assert_eq!(sound.haptic(), Some(HapticPattern::Selection), "{sound:?}");
    }
    assert_eq!(SystemSound::Success.haptic(), Some(HapticPattern::Success));
    assert_eq!(SystemSound::Warning.haptic(), Some(HapticPattern::Warning));
    assert_eq!(SystemSound::Error.haptic(), Some(HapticPattern::Error));
    assert_eq!(SystemSound::DropReject.haptic(), Some(HapticPattern::Error));
    assert_eq!(SystemSound::Click.haptic(), None, "a click has no haptic idiom");
    assert_eq!(SystemSound::Notification.haptic(), None);
}

// --- the table of Android -------------------------------------------------------------------

#[test]
fn the_whole_table_of_android() {
    let android = labels(SoundPlatform::Android);
    let expected = [
        ("Click", "system:android:FX_KEY_CLICK"),
        ("Tick", "system:android:FX_FOCUS_NAVIGATION_DOWN"),
        ("ToggleOn", "system:android:FX_KEY_CLICK"),
        ("ToggleOff", "system:android:FX_KEY_CLICK"),
        ("MenuOpen", "none:no-system-route"),
        ("MenuClose", "none:no-system-route"),
        ("Expand", "none:no-system-route"),
        ("Collapse", "none:no-system-route"),
        ("TabChange", "system:android:FX_KEY_CLICK"),
        ("Back", "system:android:FX_BACK"),
        ("DragStart", "none:no-system-route"),
        ("DropAccept", "system:android:FX_KEY_CLICK"),
        ("DropReject", "system:android:FX_KEYPRESS_INVALID"),
        ("Notification", "ringtone:android:notification"),
        ("Success", "ringtone:android:notification"),
        ("Info", "ringtone:android:notification"),
        ("Question", "ringtone:android:notification"),
        ("Warning", "system:android:FX_KEYPRESS_INVALID"),
        ("Error", "system:android:FX_KEYPRESS_INVALID"),
        ("Bell", "system:android:FX_KEYPRESS_INVALID"),
        ("Delete", "none:no-system-route"),
        ("TrashEmpty", "none:no-system-route"),
        ("Send", "none:no-system-route"),
        ("CameraShutter", "media-action:android:SHUTTER_CLICK"),
        ("ScreenCapture", "media-action:android:SHUTTER_CLICK"),
    ];
    assert_eq!(android.len(), expected.len());
    for ((name, label), (want_name, want_label)) in android.iter().zip(expected.iter()) {
        assert_eq!(name, want_name);
        assert_eq!(label, want_label, "the route of {name} on Android");
    }
}

#[test]
fn the_tick_and_the_back_name_a_second_constant_for_an_os_that_lacks_the_first() {
    assert_eq!(
        plain(SystemSound::Back, SoundPlatform::Android),
        SoundRoute::Effect {
            platform: SoundPlatform::Android,
            name: "FX_BACK",
            fallback: Some("FX_KEY_CLICK"),
        },
        "FX_BACK is API 31: an older OS plays the click"
    );
    assert_eq!(
        plain(SystemSound::Tick, SoundPlatform::Android),
        SoundRoute::Effect {
            platform: SoundPlatform::Android,
            name: "FX_FOCUS_NAVIGATION_DOWN",
            fallback: Some("FX_KEY_CLICK"),
        }
    );
}

#[test]
fn the_tab_change_of_a_pager_ticks_on_android_and_is_a_selection_on_ios() {
    assert_eq!(
        label_of(SystemSound::TabChange, SoundPlatform::Android),
        "system:android:FX_KEY_CLICK"
    );
    assert_eq!(
        label_of(SystemSound::TabChange, SoundPlatform::Ios),
        "haptic:Selection"
    );
    for desktop in [
        SoundPlatform::MacOs,
        SoundPlatform::Windows,
        SoundPlatform::Linux,
        SoundPlatform::Other,
    ] {
        assert_eq!(
            label_of(SystemSound::TabChange, desktop),
            "none:no-system-route",
            "{desktop:?} is silent under the native widget sounds"
        );
    }
}

// --- the table of iOS -----------------------------------------------------------------------

#[test]
fn the_whole_table_of_ios_is_the_haptic_idiom_and_nothing_else() {
    let ios = labels(SoundPlatform::Ios);
    for (name, label) in &ios {
        let sound = SystemSound::from_name(name).expect("a name of the list");
        match sound.haptic() {
            Some(pattern) => assert_eq!(label, &alloc::format!("haptic:{pattern:?}"), "{name}"),
            None => assert_eq!(label, "none:no-system-route", "{name}"),
        }
        assert!(
            !label.starts_with("system:") && !label.starts_with("ringtone:"),
            "iOS plays no sound: the built-in ids are undocumented ({name}: {label})"
        );
    }
}

// --- gating ----------------------------------------------------------------------------------

#[test]
fn the_apps_mute_silences_every_sound_and_a_haptic_too() {
    let muted = SoundConfig {
        enabled: false,
        ..SoundConfig::default()
    };
    for platform in [SoundPlatform::Android, SoundPlatform::Ios, SoundPlatform::Linux] {
        for sound in &ALL_SYSTEM_SOUNDS {
            let routed = route(*sound, platform, muted, SWITCHES_ON);
            if matches!(routed, SoundRoute::MediaAction { .. }) {
                continue;
            }
            assert_eq!(
                routed,
                SoundRoute::Silent(Silence::AppMuted),
                "{sound:?} on {platform:?} must be silent when the app is muted"
            );
        }
    }
}

#[test]
fn the_camera_shutter_survives_the_mute_on_android_where_the_os_requires_it() {
    let muted = SoundConfig {
        enabled: false,
        ..SoundConfig::default()
    };
    for sound in [SystemSound::CameraShutter, SystemSound::ScreenCapture] {
        assert_eq!(
            route(sound, SoundPlatform::Android, muted, SWITCHES_ON),
            SoundRoute::MediaAction {
                platform: SoundPlatform::Android,
                name: "SHUTTER_CLICK",
                only_if_required: true,
            },
            "{sound:?}: the app's mute does not waive a sound the region requires"
        );
        assert_eq!(
            route(sound, SoundPlatform::Android, SoundConfig::default(), SWITCHES_ON),
            SoundRoute::MediaAction {
                platform: SoundPlatform::Android,
                name: "SHUTTER_CLICK",
                only_if_required: false,
            }
        );
    }
}

#[test]
fn touch_sounds_off_silences_every_android_effect_even_the_error_that_is_an_event() {
    let touch_sounds_off = AudioMetrics {
        event_sounds_enabled: true,
        input_feedback_sounds_enabled: false,
    };
    for sound in [
        SystemSound::Click,
        SystemSound::Tick,
        SystemSound::TabChange,
        SystemSound::Back,
        SystemSound::Error,
        SystemSound::Warning,
        SystemSound::Bell,
    ] {
        assert_eq!(
            route(sound, SoundPlatform::Android, SoundConfig::default(), touch_sounds_off),
            SoundRoute::Silent(Silence::OsInputFeedbackOff),
            "{sound:?} is played by AudioManager.playSoundEffect, which obeys Touch sounds"
        );
    }
    // the tone of the notification is not a touch sound: the ringer mode decides, not the switch
    assert_eq!(
        route(
            SystemSound::Notification,
            SoundPlatform::Android,
            SoundConfig::default(),
            touch_sounds_off
        )
        .label(),
        "ringtone:android:notification"
    );
}

#[test]
fn the_event_switch_silences_the_notification_tone_and_the_shutter_but_not_a_click() {
    let events_off = AudioMetrics {
        event_sounds_enabled: false,
        input_feedback_sounds_enabled: true,
    };
    for sound in [SystemSound::Notification, SystemSound::Success, SystemSound::CameraShutter] {
        assert_eq!(
            route(sound, SoundPlatform::Android, SoundConfig::default(), events_off),
            SoundRoute::Silent(Silence::OsEventSoundsOff),
            "{sound:?}"
        );
    }
    assert_eq!(
        route(SystemSound::Click, SoundPlatform::Android, SoundConfig::default(), events_off)
            .label(),
        "system:android:FX_KEY_CLICK"
    );
}

#[test]
fn a_haptic_is_not_silenced_by_a_sound_switch() {
    let all_off = AudioMetrics {
        event_sounds_enabled: false,
        input_feedback_sounds_enabled: false,
    };
    assert_eq!(
        route(SystemSound::TabChange, SoundPlatform::Ios, SoundConfig::default(), all_off).label(),
        "haptic:Selection"
    );
    assert_eq!(
        route(SystemSound::Error, SoundPlatform::Ios, SoundConfig::default(), all_off).label(),
        "haptic:Error"
    );
}

#[test]
fn a_sound_with_no_route_says_so_and_does_not_borrow_another_gate() {
    let all_off = AudioMetrics {
        event_sounds_enabled: false,
        input_feedback_sounds_enabled: false,
    };
    assert_eq!(
        route(SystemSound::MenuOpen, SoundPlatform::Android, SoundConfig::default(), all_off),
        SoundRoute::Silent(Silence::NoSystemRoute)
    );
    assert_eq!(
        route(SystemSound::Click, SoundPlatform::Windows, SoundConfig::default(), SWITCHES_ON),
        SoundRoute::Silent(Silence::NoSystemRoute)
    );
}

#[test]
fn the_default_metrics_of_an_os_that_was_not_asked_keep_touch_sounds_off() {
    // `AudioMetrics::default()` is what a window has before its shell reads the OS: input
    // feedback is OFF, so a sound is not played on a guess.
    assert_eq!(
        route(
            SystemSound::Click,
            SoundPlatform::Android,
            SoundConfig::default(),
            AudioMetrics::default()
        ),
        SoundRoute::Silent(Silence::OsInputFeedbackOff)
    );
}

#[test]
fn widgets_make_sounds_on_android_only_and_only_when_the_app_allows_it() {
    let native = SoundConfig::default();
    assert!(native.widgets_play(SoundPlatform::Android));
    for platform in [
        SoundPlatform::Ios,
        SoundPlatform::MacOs,
        SoundPlatform::Windows,
        SoundPlatform::Linux,
        SoundPlatform::Other,
    ] {
        assert!(!native.widgets_play(platform), "{platform:?}");
    }
    let on = SoundConfig {
        widget_sounds: WidgetSounds::On,
        ..native
    };
    let off = SoundConfig {
        widget_sounds: WidgetSounds::Off,
        ..native
    };
    assert!(on.widgets_play(SoundPlatform::Windows));
    assert!(!off.widgets_play(SoundPlatform::Android));
    let muted = SoundConfig {
        enabled: false,
        ..on
    };
    assert!(!muted.widgets_play(SoundPlatform::Android));
}

#[test]
fn every_sound_has_a_route_on_every_platform_and_a_label() {
    for platform in [
        SoundPlatform::Android,
        SoundPlatform::Ios,
        SoundPlatform::MacOs,
        SoundPlatform::Windows,
        SoundPlatform::Linux,
        SoundPlatform::Other,
    ] {
        for sound in &ALL_SYSTEM_SOUNDS {
            for audio in [SWITCHES_ON, AudioMetrics::default()] {
                for enabled in [true, false] {
                    let config = SoundConfig {
                        enabled,
                        ..SoundConfig::default()
                    };
                    let routed = route(*sound, platform, config, audio);
                    assert!(!routed.label().is_empty(), "{sound:?} on {platform:?}");
                }
            }
        }
    }
}

// --- the manager -----------------------------------------------------------------------------

fn manager() -> SoundManager {
    SoundManager::new()
}

fn run(
    manager: &mut SoundManager,
    platform: SoundPlatform,
    now_ms: Option<u64>,
) -> Vec<SoundDecision> {
    manager.process(platform, SWITCHES_ON, now_ms)
}

#[test]
fn an_empty_manager_has_nothing_pending_and_an_empty_recording() {
    let mut m = manager();
    assert!(!m.has_pending());
    assert_eq!(m.record_count(), 0);
    assert!(run(&mut m, SoundPlatform::Android, None).is_empty());
    assert_eq!(m.config(), SoundConfig::default());
}

#[test]
fn a_request_is_recorded_with_its_route_and_returned_to_be_played() {
    let mut m = manager();
    m.play(SystemSound::TabChange);
    assert!(m.has_pending());
    let admitted = run(&mut m, SoundPlatform::Android, Some(1000));
    assert_eq!(admitted.len(), 1);
    assert_eq!(admitted[0].sound, SystemSound::TabChange);
    assert_eq!(admitted[0].route.label(), "system:android:FX_KEY_CLICK");
    assert!(!m.has_pending());

    let records: Vec<&SoundRecord> = m.records().collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].sequence, 1);
    assert_eq!(records[0].suppressed_by, None);
    assert_eq!(records[0].outcome, SoundOutcome::Pending);
    assert!(records[0].was_admitted());

    m.record_outcome(admitted[0].sequence, SoundOutcome::Played);
    assert_eq!(m.records().next().expect("one record").outcome, SoundOutcome::Played);
}

#[test]
fn identical_requests_of_one_pass_are_recorded_and_merged_into_the_first() {
    let mut m = manager();
    for _ in 0..60 {
        m.play(SystemSound::Tick);
    }
    let admitted = run(&mut m, SoundPlatform::Android, Some(500));
    assert_eq!(admitted.len(), 1, "a per-frame callback plays one tick, not sixty");
    assert_eq!(m.record_count(), 60, "but every request is in the recording");
    let coalesced = m
        .records()
        .filter(|record| record.suppressed_by == Some(COALESCED))
        .count();
    assert_eq!(coalesced, 59);
}

#[test]
fn a_different_sound_between_two_equal_ones_breaks_the_merge() {
    let mut m = manager();
    m.play(SystemSound::Tick);
    m.play(SystemSound::Click);
    m.play(SystemSound::Tick);
    let admitted = run(&mut m, SoundPlatform::Android, None);
    assert_eq!(admitted.len(), 3);
}

#[test]
fn the_same_sound_again_too_soon_is_dropped_by_its_minimum_interval() {
    let mut m = manager();
    m.play(SystemSound::Click);
    assert_eq!(run(&mut m, SoundPlatform::Android, Some(1000)).len(), 1);
    m.play(SystemSound::Click);
    let second = run(&mut m, SoundPlatform::Android, Some(1060));
    assert!(second.is_empty(), "60 ms after the click, the interval of a click is 100 ms");
    assert_eq!(
        m.records().last().expect("a record").suppressed_by,
        Some(MIN_INTERVAL)
    );
    m.play(SystemSound::Click);
    assert_eq!(
        run(&mut m, SoundPlatform::Android, Some(1100)).len(),
        1,
        "100 ms after the first PLAY (the dropped one does not move the interval)"
    );
}

#[test]
fn a_fling_through_pages_ticks_once_per_page_when_the_pages_are_40_ms_apart_or_more() {
    let mut m = manager();
    let mut played = 0;
    // ten crossings, decelerating: 40, 50, 60 ... 130 ms apart
    let mut now = 10_000;
    let mut gap = 40;
    for _ in 0..10 {
        m.play(SystemSound::TabChange);
        played += run(&mut m, SoundPlatform::Android, Some(now)).len();
        now += gap;
        gap += 10;
    }
    assert_eq!(played, 10, "every crossing of the fling is heard");
    assert_eq!(SystemSound::TabChange.min_interval_ms(), 40);
    assert_eq!(SystemSound::Tick.min_interval_ms(), 40);
    assert_eq!(SystemSound::Click.min_interval_ms(), 100);
}

#[test]
fn crossings_faster_than_40_ms_are_thinned_out_not_stacked() {
    let mut m = manager();
    let mut played = 0;
    // a crossing every 15 ms for 300 ms
    for step in 0..20u64 {
        m.play(SystemSound::TabChange);
        played += run(&mut m, SoundPlatform::Android, Some(20_000 + step * 15)).len();
    }
    assert!(
        (7..=8).contains(&played),
        "300 ms at one play per 40 ms is 7 or 8 plays, got {played}"
    );
    assert_eq!(m.record_count(), 20);
}

#[test]
fn an_event_sound_is_not_repeated_more_than_twice_in_a_row() {
    let mut m = manager();
    let mut results = Vec::new();
    for step in 0..4u64 {
        m.play(SystemSound::Notification);
        results.push(run(&mut m, SoundPlatform::Android, Some(30_000 + step * 200)).len());
    }
    assert_eq!(results, vec![1, 1, 0, 0], "the third identical event in a row is dropped");
    assert_eq!(
        m.records().last().expect("a record").suppressed_by,
        Some(REPEAT_LIMIT)
    );

    // a gap of a second ends the run
    m.play(SystemSound::Notification);
    assert_eq!(run(&mut m, SoundPlatform::Android, Some(32_500)).len(), 1);
}

#[test]
fn another_sound_between_two_events_ends_the_run() {
    let mut m = manager();
    let mut played = 0;
    for step in 0..6u64 {
        let sound = if step % 3 == 2 {
            SystemSound::Success
        } else {
            SystemSound::Notification
        };
        m.play(sound);
        played += run(&mut m, SoundPlatform::Android, Some(40_000 + step * 150)).len();
    }
    assert_eq!(played, 6, "no sound was the third identical one in a row");
}

#[test]
fn input_feedback_has_no_repeat_limit_a_tick_per_page_is_the_point() {
    let mut m = manager();
    let mut played = 0;
    for step in 0..12u64 {
        m.play(SystemSound::Click);
        played += run(&mut m, SoundPlatform::Android, Some(50_000 + step * 100)).len();
    }
    assert_eq!(played, 12);
}

#[test]
fn a_headless_run_records_every_request_with_no_pacing_and_plays_nothing() {
    let mut m = manager();
    for _ in 0..5 {
        m.play(SystemSound::TabChange);
        m.record_without_playing(SoundPlatform::Android, SWITCHES_ON, None, "headless");
    }
    assert_eq!(m.record_count(), 5);
    for record in m.records() {
        assert_eq!(record.suppressed_by, None, "no pacing without a clock");
        assert_eq!(record.outcome, SoundOutcome::Skipped(String::from("headless")));
        assert_eq!(record.route.label(), "system:android:FX_KEY_CLICK");
    }
}

#[test]
fn a_muted_manager_records_the_request_as_app_muted() {
    let mut m = manager();
    m.set_muted(true);
    assert!(!m.config().enabled);
    m.play(SystemSound::Error);
    assert!(run(&mut m, SoundPlatform::Android, Some(1)).is_empty());
    assert_eq!(
        m.records().next().expect("a record").suppressed_by,
        Some("app-muted")
    );
    m.set_muted(false);
    assert!(m.config().enabled);
    m.play(SystemSound::Error);
    assert_eq!(run(&mut m, SoundPlatform::Android, Some(5000)).len(), 1);
}

#[test]
fn a_silenced_sound_does_not_use_up_the_interval_of_the_next_one() {
    let mut m = manager();
    m.play(SystemSound::Click);
    let off = AudioMetrics::default();
    assert!(m.process(SoundPlatform::Android, off, Some(1000)).is_empty());
    m.play(SystemSound::Click);
    assert_eq!(
        run(&mut m, SoundPlatform::Android, Some(1010)).len(),
        1,
        "the first click never played, so the second is not too soon after it"
    );
}

#[test]
fn the_recording_is_bounded_and_the_numbering_goes_on() {
    let mut m = manager();
    for _ in 0..(MAX_SOUND_RECORDS + 10) {
        m.play(SystemSound::Click);
        m.record_without_playing(SoundPlatform::Linux, SWITCHES_ON, None, "headless");
    }
    assert_eq!(m.record_count(), MAX_SOUND_RECORDS);
    let first = m.records().next().expect("a record").sequence;
    assert_eq!(first, 11, "the ten oldest were dropped");
    assert_eq!(
        m.records().last().expect("a record").sequence,
        (MAX_SOUND_RECORDS + 10) as u64
    );
    m.clear_records();
    assert_eq!(m.record_count(), 0);
    m.play(SystemSound::Click);
    m.record_without_playing(SoundPlatform::Linux, SWITCHES_ON, None, "headless");
    assert_eq!(
        m.records().next().expect("a record").sequence,
        (MAX_SOUND_RECORDS + 11) as u64
    );
}

#[test]
fn the_queue_is_bounded_and_the_dropped_requests_are_counted_once() {
    let mut m = manager();
    for index in 0..(MAX_PENDING_SOUNDS + 5) {
        // alternate so that the merge of equal neighbours does not hide the bound
        m.play(if index % 2 == 0 {
            SystemSound::Click
        } else {
            SystemSound::Tick
        });
    }
    assert_eq!(m.take_dropped(), 5);
    assert_eq!(m.take_dropped(), 0, "the count is taken, not read");
    let admitted = run(&mut m, SoundPlatform::Android, None);
    assert_eq!(admitted.len(), MAX_PENDING_SOUNDS);
}

#[test]
fn a_record_describes_itself_in_one_line() {
    let mut m = manager();
    m.play(SystemSound::TabChange);
    let admitted = run(&mut m, SoundPlatform::Android, Some(100));
    m.record_outcome(admitted[0].sequence, SoundOutcome::Played);
    m.play(SystemSound::TabChange);
    m.record_without_playing(SoundPlatform::Android, SWITCHES_ON, Some(110), "headless");
    let lines = m.describe_recent(10);
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0],
        "#1 TabChange [input] route=system:android:FX_KEY_CLICK suppressed_by=- outcome=played"
    );
    assert_eq!(
        lines[1],
        "#2 TabChange [input] route=system:android:FX_KEY_CLICK suppressed_by=min-interval \
         outcome=skipped:min-interval"
    );
    assert_eq!(m.describe_recent(1).len(), 1);
    assert_eq!(m.describe_recent(0).len(), 0);
}

#[test]
fn an_outcome_names_what_happened() {
    assert_eq!(SoundOutcome::Pending.label(), "pending");
    assert_eq!(SoundOutcome::Played.label(), "played");
    assert_eq!(SoundOutcome::Skipped(String::from("headless")).label(), "skipped:headless");
    assert_eq!(SoundOutcome::Failed(String::from("no class")).label(), "failed:no class");
}

#[test]
fn recording_an_outcome_of_an_unknown_sequence_changes_nothing() {
    let mut m = manager();
    m.play(SystemSound::Click);
    let admitted = run(&mut m, SoundPlatform::Android, Some(7));
    assert_eq!(admitted.len(), 1);
    m.record_outcome(999, SoundOutcome::Played);
    assert_eq!(m.records().next().expect("a record").outcome, SoundOutcome::Pending);
}

#[test]
fn a_helper_line_is_the_label_of_the_outcome_it_reports() {
    for outcome in [
        SoundOutcome::Played,
        SoundOutcome::Skipped(String::from("os:touch-sounds-off")),
        SoundOutcome::Failed(String::from("AudioManager has no constant FX_BACK")),
    ] {
        assert_eq!(SoundOutcome::from_helper_line(&outcome.label()), outcome);
    }
    assert_eq!(
        SoundOutcome::from_helper_line("  played\n"),
        SoundOutcome::Played,
        "the line is trimmed"
    );
}

#[test]
fn a_helper_line_that_is_not_an_answer_is_a_failure_that_quotes_it() {
    for line in ["", "ok", "Played", "pending", "playedx"] {
        match SoundOutcome::from_helper_line(line) {
            SoundOutcome::Failed(why) => {
                assert!(why.contains(&alloc::format!("{line:?}")), "{why}");
            }
            other => panic!("{line:?} must not be an answer, got {other:?}"),
        }
    }
}
