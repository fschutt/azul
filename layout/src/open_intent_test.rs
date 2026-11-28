//! What an intent asks of the app: which items are files, links, texts, and what the sender wants done.

use azul_core::open_files::{OpenAction, MAX_ITEMS as MAX_ITEMS_PER_REQUEST};

use super::*;

fn intent(action: &str) -> IntentInfo {
    IntentInfo {
        action: action.to_string(),
        ..IntentInfo::default()
    }
}

const VIEW: &str = "android.intent.action.VIEW";
const SEND: &str = "android.intent.action.SEND";
const SEND_MULTIPLE: &str = "android.intent.action.SEND_MULTIPLE";

#[test]
fn a_launch_from_the_launcher_asks_nothing() {
    for action in ["android.intent.action.MAIN", "", "android.intent.action.PICK", "com.example.CUSTOM"] {
        assert_eq!(classify(&intent(action)), None, "{action:?}");
    }
    // not even with data: only the actions the manifest filters declare are requests
    let mut weird = intent("android.intent.action.MAIN");
    weird.data = Some("content://x/y".to_string());
    assert_eq!(classify(&weird), None);
}

#[test]
fn a_view_of_a_content_uri_is_a_file_to_view() {
    let mut info = intent(VIEW);
    info.data = Some("content://com.android.providers.downloads.documents/document/msf%3A42".to_string());
    info.mime = "text/markdown".to_string();
    let plan = classify(&info).unwrap();
    assert_eq!(plan.action, OpenAction::View);
    assert_eq!(plan.files, ["content://com.android.providers.downloads.documents/document/msf%3A42"]);
    assert!(plan.links.is_empty() && plan.texts.is_empty());
    assert!(!plan.write && !plan.persistable);
}

#[test]
fn an_edit_with_a_write_grant_is_writable() {
    let mut info = intent("android.intent.action.EDIT");
    info.data = Some("content://x/doc".to_string());
    info.flags = FLAG_GRANT_READ_URI_PERMISSION | FLAG_GRANT_WRITE_URI_PERMISSION | FLAG_GRANT_PERSISTABLE_URI_PERMISSION;
    let plan = classify(&info).unwrap();
    assert_eq!(plan.action, OpenAction::Edit);
    assert!(plan.write && plan.persistable);
}

#[test]
fn a_view_of_a_link_is_a_link_and_not_a_file() {
    for link in ["mailto:someone@example.org?subject=hi", "geo:48.52,8.67?q=Tuebingen", "https://example.org/a", "azmeet://join/abc"] {
        let mut info = intent(VIEW);
        info.data = Some(link.to_string());
        let plan = classify(&info).unwrap();
        assert_eq!(plan.links, [link]);
        assert!(plan.files.is_empty(), "{link}");
    }
}

#[test]
fn a_sendto_mailto_is_a_link() {
    let mut info = intent("android.intent.action.SENDTO");
    info.data = Some("mailto:a@b.c".to_string());
    let plan = classify(&info).unwrap();
    assert_eq!(plan.action, OpenAction::View);
    assert_eq!(plan.links, ["mailto:a@b.c"]);
}

#[test]
fn a_send_of_one_stream_is_a_shared_file() {
    let mut info = intent(SEND);
    info.mime = "image/png".to_string();
    info.streams = vec!["content://media/external/images/media/7".to_string()];
    let plan = classify(&info).unwrap();
    assert_eq!(plan.action, OpenAction::Send);
    assert_eq!(plan.files, ["content://media/external/images/media/7"]);
}

#[test]
fn a_send_multiple_keeps_the_order_and_drops_repeats_the_clip_data_adds() {
    let mut info = intent(SEND_MULTIPLE);
    info.streams = vec!["content://x/a".to_string(), "content://x/b".to_string(), "content://x/c".to_string()];
    // the framework copies the streams into the clip: the same URIs again, plus one more
    info.clip_uris = vec!["content://x/a".to_string(), "content://x/b".to_string(), "content://x/d".to_string()];
    let plan = classify(&info).unwrap();
    assert_eq!(plan.files, ["content://x/a", "content://x/b", "content://x/c", "content://x/d"]);
}

#[test]
fn a_shared_text_is_a_text_and_rides_with_its_files() {
    let mut info = intent(SEND);
    info.text = Some("  look at this \n".to_string());
    let plan = classify(&info).unwrap();
    assert_eq!(plan.texts, ["look at this"]);
    assert!(plan.files.is_empty());

    info.streams = vec!["content://x/photo".to_string()];
    let both = classify(&info).unwrap();
    assert_eq!((both.files.len(), both.texts.len()), (1, 1));
}

#[test]
fn a_view_never_makes_a_text_of_a_stray_extra() {
    let mut info = intent(VIEW);
    info.data = Some("content://x/y".to_string());
    info.text = Some("not shared".to_string());
    assert!(classify(&info).unwrap().texts.is_empty());
}

#[test]
fn an_intent_with_nothing_in_it_asks_nothing() {
    assert_eq!(classify(&intent(VIEW)), None);
    assert_eq!(classify(&intent(SEND)), None);
    let mut blank = intent(SEND);
    blank.text = Some("   ".to_string());
    blank.streams = vec![String::new(), "  ".to_string()];
    assert_eq!(classify(&blank), None);
    // a data string that is no URI at all
    let mut junk = intent(VIEW);
    junk.data = Some("just words".to_string());
    assert_eq!(classify(&junk), None);
}

#[test]
fn a_stream_that_is_a_link_is_not_copied_as_a_file() {
    let mut info = intent(SEND);
    info.streams = vec!["geo:1,2".to_string(), "content://x/f".to_string()];
    let plan = classify(&info).unwrap();
    assert_eq!(plan.files, ["content://x/f"]);
    assert_eq!(plan.links, ["geo:1,2"]);
}

#[test]
fn a_flood_of_items_is_cut_at_the_limit_files_last() {
    let mut info = intent(SEND_MULTIPLE);
    info.streams = (0..(MAX_ITEMS_PER_REQUEST + 10)).map(|i| format!("content://x/{i}")).collect();
    info.text = Some("t".to_string());
    let plan = classify(&info).unwrap();
    assert_eq!(plan.files.len() + plan.links.len() + plan.texts.len(), MAX_ITEMS_PER_REQUEST);
    assert_eq!(plan.truncated, 11);
    assert!(plan.texts.is_empty(), "the text goes first when something has to");
    assert_eq!(plan.files.first().map(String::as_str), Some("content://x/0"));
}

#[test]
fn the_scheme_of_a_string() {
    assert_eq!(scheme_of("content://a/b").as_deref(), Some("content"));
    assert_eq!(scheme_of("MAILTO:a@b").as_deref(), Some("mailto"));
    assert_eq!(scheme_of("az-app+x.y://q").as_deref(), Some("az-app+x.y"));
    for bad in ["", "no scheme", ":x", "1http://a", "http:", "ht tp://a", "/path/only"] {
        assert_eq!(scheme_of(bad), None, "{bad:?}");
    }
}
