//! The decision about asking for speech recognition and the microphone: what the statuses allow
//! (the bare binary a terminal started, which the old rule refused although the system would have
//! allowed it), what a "no" says, and when asking is not safe.

use super::{
    app_name_from_path, declined, decide, Access, Decision, Gate, Gates, Refusal, MICROPHONE_KEY,
    SPEECH_KEY,
};

/// A bare binary: no Info.plist, so neither usage text.
fn bare(speech: Access, microphone: Access) -> Gates {
    Gates {
        speech,
        microphone,
        speech_key: false,
        microphone_key: false,
        starter: Some("Terminal".to_string()),
    }
}

/// An app bundle that declares both texts.
fn bundled(speech: Access, microphone: Access) -> Gates {
    Gates {
        speech_key: true,
        microphone_key: true,
        starter: None,
        ..bare(speech, microphone)
    }
}

fn refusal(decision: Decision) -> (Refusal, String) {
    match decision {
        Decision::Refuse { why, message } => (why, message),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// The case the old rule got wrong: macOS 15.5 answered `Authorized` for both permissions to a
/// binary without any Info.plist (Terminal held them), and recognition worked. No usage text is
/// needed when nothing is asked.
#[test]
fn a_bare_binary_whose_starter_holds_both_permissions_listens() {
    assert_eq!(decide(&bare(Access::Authorized, Access::Authorized)), Decision::Listen);
    assert_eq!(decide(&bundled(Access::Authorized, Access::Authorized)), Decision::Listen);
}

#[test]
fn a_no_is_a_denial_that_says_where_to_change_it() {
    let (why, message) = refusal(decide(&bare(Access::Denied, Access::Authorized)));
    assert_eq!(why, Refusal::Denied);
    assert!(message.contains("System Settings > Privacy & Security > Speech Recognition"), "{message}");
    assert!(message.contains("Terminal"), "the app that is held responsible is named: {message}");
    assert!(!message.contains("Microphone"), "only what is off is named: {message}");

    let (why, message) = refusal(decide(&bare(Access::Authorized, Access::Denied)));
    assert_eq!(why, Refusal::Denied);
    assert!(message.contains("System Settings > Privacy & Security > Microphone"), "{message}");
}

#[test]
fn both_refusals_are_said_in_one_message() {
    let (why, message) = refusal(decide(&bundled(Access::Denied, Access::Denied)));
    assert_eq!(why, Refusal::Denied);
    assert!(message.contains("Speech Recognition") && message.contains("Microphone"), "{message}");
    assert!(message.contains("this app"), "an app that declares its texts is held responsible itself: {message}");
}

#[test]
fn a_restriction_is_a_denial_only_an_administrator_can_lift() {
    let (why, message) = refusal(decide(&bundled(Access::Restricted, Access::Authorized)));
    assert_eq!(why, Refusal::Denied);
    assert!(message.contains("administrator"), "{message}");
    assert!(!message.contains("allow it in System Settings"), "the user cannot change it: {message}");
    let (_, message) = refusal(decide(&bundled(Access::Authorized, Access::Restricted)));
    assert!(message.contains("microphone") && message.contains("administrator"), "{message}");
}

#[test]
fn a_no_wins_over_an_undecided_one_and_nothing_is_asked() {
    for gates in [
        bundled(Access::Denied, Access::NotDetermined),
        bundled(Access::NotDetermined, Access::Denied),
        bare(Access::Restricted, Access::NotDetermined),
    ] {
        let (why, _) = refusal(decide(&gates));
        assert_eq!(why, Refusal::Denied, "{gates:?}");
    }
}

#[test]
fn an_app_with_its_texts_asks_for_what_is_undecided_only() {
    assert_eq!(
        decide(&bundled(Access::NotDetermined, Access::NotDetermined)),
        Decision::Ask { speech: true, microphone: true }
    );
    assert_eq!(
        decide(&bundled(Access::Authorized, Access::NotDetermined)),
        Decision::Ask { speech: false, microphone: true }
    );
    assert_eq!(
        decide(&bundled(Access::NotDetermined, Access::Authorized)),
        Decision::Ask { speech: true, microphone: false }
    );
}

/// The system stops a process that asks without its usage text: refuse before asking, and say who
/// would have been asked and what to do.
#[test]
fn an_undecided_permission_without_its_text_is_refused_without_asking() {
    let (why, message) = refusal(decide(&bare(Access::NotDetermined, Access::NotDetermined)));
    assert_eq!(why, Refusal::Unavailable);
    assert!(message.contains(SPEECH_KEY) && message.contains(MICROPHONE_KEY), "{message}");
    assert!(message.contains("Terminal"), "{message}");
    assert!(message.contains("azul-doc bundle"), "{message}");
    assert!(message.contains("-sectcreate __TEXT __info_plist"), "{message}");
    assert!(message.contains("speech recognition and the microphone"), "{message}");
}

#[test]
fn only_the_undecided_permission_needs_its_text() {
    // Speech is allowed, the microphone is not decided and its text is missing.
    let gates = Gates {
        speech_key: true,
        ..bare(Access::Authorized, Access::NotDetermined)
    };
    let (why, message) = refusal(decide(&gates));
    assert_eq!(why, Refusal::Unavailable);
    assert!(message.contains(MICROPHONE_KEY) && !message.contains(SPEECH_KEY), "{message}");
    assert!(message.contains("the microphone") && !message.contains("speech recognition"), "{message}");

    // The text of an allowed permission is not needed: the microphone's alone lets it ask.
    let gates = Gates {
        microphone_key: true,
        ..bare(Access::Authorized, Access::NotDetermined)
    };
    assert_eq!(decide(&gates), Decision::Ask { speech: false, microphone: true });
}

#[test]
fn without_a_name_for_the_starter_the_message_says_terminal_in_general() {
    let gates = Gates {
        starter: None,
        ..bare(Access::Denied, Access::Authorized)
    };
    let (_, message) = refusal(decide(&gates));
    assert!(message.contains("the app this program was started from (usually your terminal)"), "{message}");
    let blank = Gates {
        starter: Some("  ".to_string()),
        ..gates
    };
    let (_, message) = refusal(decide(&blank));
    assert!(message.contains("usually your terminal"), "{message}");
}

/// The two status enums disagree on 1 and 2 (speech: denied, restricted; capture: restricted,
/// denied): reading one with the other's table would turn a "no" into "an administrator's rule".
#[test]
fn the_two_status_scales_are_read_each_with_its_own_table() {
    assert_eq!(Access::from_speech(0), Access::NotDetermined);
    assert_eq!(Access::from_speech(1), Access::Denied);
    assert_eq!(Access::from_speech(2), Access::Restricted);
    assert_eq!(Access::from_speech(3), Access::Authorized);
    assert_eq!(Access::from_capture(0), Access::NotDetermined);
    assert_eq!(Access::from_capture(1), Access::Restricted);
    assert_eq!(Access::from_capture(2), Access::Denied);
    assert_eq!(Access::from_capture(3), Access::Authorized);
    // A status a later system adds is read as undecided: never as a yes.
    assert_eq!(Access::from_speech(7), Access::NotDetermined);
    assert_eq!(Access::from_capture(-1), Access::NotDetermined);
    assert_ne!(Access::from_speech(9), Access::Authorized);
}

#[test]
fn what_the_user_is_told_after_a_prompt_without_a_yes() {
    let gates = bare(Access::NotDetermined, Access::NotDetermined);
    let unanswered = declined(Gate::Speech, false, &gates);
    assert!(unanswered.contains("no answer") && unanswered.contains("speech recognition"), "{unanswered}");
    let refused = declined(Gate::Microphone, true, &gates);
    assert!(refused.contains("Privacy & Security > Microphone") && refused.contains("Terminal"), "{refused}");
}

#[test]
fn the_app_a_process_runs_from_is_named_by_its_bundle() {
    assert_eq!(
        app_name_from_path("/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal").as_deref(),
        Some("Terminal")
    );
    // A helper inside an app is named by the app.
    assert_eq!(
        app_name_from_path(
            "/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper.app/Contents/MacOS/Code Helper"
        )
        .as_deref(),
        Some("Visual Studio Code")
    );
    assert_eq!(app_name_from_path("/usr/bin/login"), None);
    assert_eq!(app_name_from_path("/opt/homebrew/bin/zsh"), None);
    assert_eq!(app_name_from_path("/weird/.app/Contents"), None, "an empty name is none");
    assert_eq!(app_name_from_path(""), None);
}
