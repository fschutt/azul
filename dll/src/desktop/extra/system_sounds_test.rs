use azul_core::sound::{route, SoundConfig, SoundPlatform, SoundRoute, ALL_SYSTEM_SOUNDS};
use azul_css::system::AudioMetrics;

use super::*;

/// Every constant the route table of Windows names has a `MessageBeep` kind in the player, and
/// the kinds are the documented values of `winuser.h`.
#[test]
fn every_windows_route_names_a_kind_the_player_knows() {
    let on = AudioMetrics {
        event_sounds_enabled: true,
        input_feedback_sounds_enabled: true,
    };
    let mut named = 0;
    for sound in &ALL_SYSTEM_SOUNDS {
        if let SoundRoute::Effect { name, .. } =
            route(*sound, SoundPlatform::Windows, SoundConfig::default(), on)
        {
            named += 1;
            assert!(
                message_beep_kind(name).is_some(),
                "{sound:?}: the Windows table names {name}, which the player cannot play"
            );
        }
    }
    assert_eq!(named, 6, "Info, Question, Warning, Error, DropReject and Bell");
    assert_eq!(message_beep_kind("MB_OK"), Some(0x00));
    assert_eq!(message_beep_kind("MB_ICONHAND"), Some(0x10));
    assert_eq!(message_beep_kind("MB_ICONQUESTION"), Some(0x20));
    assert_eq!(message_beep_kind("MB_ICONEXCLAMATION"), Some(0x30));
    assert_eq!(message_beep_kind("MB_ICONASTERISK"), Some(0x40));
    assert_eq!(message_beep_kind("MB_ICONERROR_NOPE"), None);
}

/// The player is never asked to play something that is not an effect of a desktop, and when it is
/// it answers a failure that names the route, not silence.
#[test]
fn a_route_that_is_not_a_desktop_effect_is_a_failure_that_names_it() {
    let on = AudioMetrics {
        event_sounds_enabled: true,
        input_feedback_sounds_enabled: true,
    };
    let android = route(
        azul_core::sound::SystemSound::Notification,
        SoundPlatform::Android,
        SoundConfig::default(),
        on,
    );
    match play(&android) {
        SoundOutcome::Failed(why) => assert!(why.contains("ringtone:android:notification"), "{why}"),
        other => panic!("expected a failure, got {other:?}"),
    }
}

/// The alert sound's id is the documented `kSystemSoundID_UserPreferredAlert`.
#[test]
fn the_alert_sound_is_the_users_preferred_one() {
    assert_eq!(USER_PREFERRED_ALERT, 0x1000);
}
