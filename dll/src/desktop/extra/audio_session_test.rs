use super::*;

/// The default session "disallows audio recording" and an ambient one is silenced by the
/// Ring/Silent switch and the screen lock: the two uses ask for the categories that allow what
/// they do.
#[test]
fn a_sink_asks_for_playback_and_a_microphone_for_a_session_that_can_record() {
    assert_eq!(category_for(SessionUse::Playback), "AVAudioSessionCategoryPlayback");
    assert_eq!(category_for(SessionUse::Record), "AVAudioSessionCategoryPlayAndRecord");
    assert_eq!(mode_for(SessionUse::Playback), "AVAudioSessionModeDefault");
    assert_eq!(mode_for(SessionUse::Record), "AVAudioSessionModeMeasurement");
}

/// `AllowBluetooth` (0x4) and `DefaultToSpeaker` (0x8) for recording, nothing for playback.
#[test]
fn only_recording_asks_for_category_options() {
    assert_eq!(options_for(SessionUse::Playback), 0);
    assert_eq!(options_for(SessionUse::Record), 0x4 | 0x8);
}

/// Off iOS there is no audio session: asking is a success that does nothing.
#[cfg(not(target_os = "ios"))]
#[test]
fn a_host_without_an_audio_session_answers_ok() {
    assert_eq!(activate(SessionUse::Playback), Ok(()));
    assert_eq!(activate(SessionUse::Record), Ok(()));
}
