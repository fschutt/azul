//! The speech module's own tests: the language tags the engines get, SAPI's markup, and the
//! headless stand-in (what a headless run says and hears). The platform engines are not
//! reached: a test must not talk.

use super::{
    headless, language_tag, listen_with, sapi_text, say_with, Backend, HeardStatus, SpeechStatus,
};

#[test]
fn a_language_is_tagged_as_the_engines_want_it() {
    assert_eq!(language_tag("de"), "de-DE");
    assert_eq!(language_tag("en"), "en-US");
    assert_eq!(language_tag("fr"), "fr-FR");
    assert_eq!(language_tag("pt_BR"), "pt-BR");
    assert_eq!(language_tag("de-AT"), "de-AT");
    assert_eq!(language_tag("EN-gb"), "en-GB");
    assert_eq!(language_tag(" "), "");
}

#[test]
fn sapi_gets_its_language_as_markup_and_the_text_escaped() {
    let (markup, is_xml) = sapi_text("Rechts abbiegen <jetzt> & weiter", "de-DE");
    assert!(is_xml);
    assert_eq!(markup, "<lang langid=\"407\">Rechts abbiegen &lt;jetzt&gt; &amp; weiter</lang>");
    let (plain, is_xml) = sapi_text("Turn left", "");
    assert!(!is_xml, "the system's voice needs no markup");
    assert_eq!(plain, "Turn left");
    let (unknown, is_xml) = sapi_text("Bonjour", "xx-YY");
    assert!(!is_xml, "a language SAPI has no id for is said in the system's voice");
    assert_eq!(unknown, "Bonjour");
    assert_eq!(sapi_text("Hallo", "de").0, "<lang langid=\"407\">Hallo</lang>");
}

#[test]
fn a_headless_run_logs_what_it_is_asked_to_say_and_says_nothing() {
    let text = "In 200 m, turn right onto Unter den Linden (headless test 1)";
    assert_eq!(say_with(Backend::Headless, text, "en", false), SpeechStatus::Queued);
    assert_eq!(say_with(Backend::Headless, "Jetzt rechts abbiegen (headless test 1)", "de", true), SpeechStatus::Queued);
    let spoken = headless::spoken();
    let first = spoken.iter().find(|u| u.text == text).expect("logged");
    assert_eq!(first.language, "en-US");
    assert!(!first.interrupt);
    let second = spoken
        .iter()
        .find(|u| u.text == "Jetzt rechts abbiegen (headless test 1)")
        .expect("logged");
    assert_eq!(second.language, "de-DE");
    assert!(second.interrupt);
    assert!(!super::speaking_with(Backend::Headless), "nothing is ever being said");
    assert!(super::available_with(Backend::Headless), "the app's prompts go on, logged");
}

#[test]
fn an_empty_text_is_not_said() {
    let before = headless::spoken().len();
    assert_eq!(say_with(Backend::Headless, "   ", "en", false), SpeechStatus::Queued);
    assert_eq!(headless::spoken().len(), before);
}

#[test]
fn a_headless_run_hears_what_the_test_says_or_nothing() {
    headless::set_heard(Some("navigate to Leipzig"));
    let heard = listen_with(Backend::Headless, "en", 100);
    assert_eq!(heard.status, HeardStatus::Heard);
    assert_eq!(heard.text.as_str(), "navigate to Leipzig");
    headless::set_heard(Some(""));
    assert_eq!(listen_with(Backend::Headless, "en", 100).status, HeardStatus::NothingHeard);
    headless::set_heard(None);
}

/// What Android's recognizer answered (the Java helper's status code, its text, why not, how
/// sure) as the engine's answer: the codes are `HeardStatus`'s order, an unknown one a failure.
#[test]
fn an_android_recognizers_answer_reads_as_the_engines() {
    use super::heard_from_android;
    let heard = heard_from_android(0, "navigiere nach Leipzig", "", 0.82);
    assert_eq!(heard.status, HeardStatus::Heard);
    assert_eq!(heard.text.as_str(), "navigiere nach Leipzig");
    assert!((heard.confidence - 0.82).abs() < 1e-6);
    assert_eq!(heard_from_android(0, "  ", "", 0.0).status, HeardStatus::NothingHeard, "no words");
    assert_eq!(heard_from_android(1, "", "", 0.0).status, HeardStatus::NothingHeard);
    let denied = heard_from_android(2, "", "the microphone is not allowed", 0.0);
    assert_eq!(denied.status, HeardStatus::Denied);
    assert_eq!(denied.message.as_str(), "the microphone is not allowed");
    assert_eq!(heard_from_android(3, "", "", 0.0).status, HeardStatus::Unavailable);
    assert_eq!(heard_from_android(4, "", "network", 0.0).status, HeardStatus::Failed);
    assert_eq!(heard_from_android(99, "", "", 0.0).status, HeardStatus::Failed);
    assert!((heard_from_android(0, "a", "", 7.0).confidence - 1.0).abs() < 1e-6, "kept to 0..1");
}
