//! What is handed to the share sheet: the names, the sizes, the staged place, what the status bar says.

use super::*;

/// A message with one attachment, `note.txt` ("hello").
const WITH_ATTACHMENT: &[u8] = b"From: a@example.org\r\nTo: b@example.org\r\nSubject: Report\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"b\"\r\n\r\n\
--b\r\nContent-Type: text/plain\r\n\r\nSee the note.\r\n\
--b\r\nContent-Type: text/plain; name=\"note.txt\"\r\nContent-Disposition: attachment; filename=\"note.txt\"\r\n\r\nhello\r\n--b--\r\n";

#[test]
fn a_display_name_keeps_what_a_file_name_may_hold() {
    assert_eq!(display_name("Report 2026.pdf", "x"), "Report 2026.pdf");
    assert_eq!(display_name("a/b\\c:d*e?f\"g<h>i|j", "x"), "a_b_c_d_e_f_g_h_i_j");
    assert_eq!(display_name("tab\there\nnewline", "x"), "tab_here_newline");
    assert_eq!(display_name("  .hidden  ", "x"), "hidden");
    assert_eq!(display_name("..", "fallback"), "fallback");
    assert_eq!(display_name("", "fallback"), "fallback");
    assert_eq!(display_name("   ", "fallback"), "fallback");
}

#[test]
fn a_display_name_is_cut_at_a_character_not_in_the_middle_of_one() {
    let long = "ä".repeat(MAX_NAME_CHARS + 20);
    let cut = display_name(&long, "x");
    assert_eq!(cut.chars().count(), MAX_NAME_CHARS);
    assert!(cut.chars().all(|c| c == 'ä'));
}

#[test]
fn a_message_is_an_eml_file_named_after_its_subject() {
    let offer = message_offer(b"Subject: Hi\r\n\r\nbody".to_vec(), "Hi: there/you").unwrap();
    assert_eq!(offer.name, "Hi_ there_you.eml");
    assert_eq!(offer.mime, "message/rfc822");
    assert_eq!(offer.title, "Hi_ there_you");
    let nameless = message_offer(b"x".to_vec(), "").unwrap();
    assert_eq!(nameless.name, "message.eml");
}

#[test]
fn nothing_and_too_much_are_refused() {
    assert_eq!(message_offer(Vec::new(), "s"), Err(Refusal::Nothing));
    assert_eq!(check_size(MAX_SHARED_BYTES), Ok(()));
    assert_eq!(check_size(MAX_SHARED_BYTES + 1), Err(Refusal::TooBig(MAX_SHARED_BYTES + 1)));
}

#[test]
fn an_attachment_is_the_file_it_is_with_its_own_name_and_type() {
    let offer = attachment_offer(WITH_ATTACHMENT, 0).unwrap();
    assert_eq!(offer.name, "note.txt");
    assert_eq!(offer.mime, "text/plain");
    assert_eq!(offer.title, "note.txt");
    assert_eq!(offer.bytes, b"hello".to_vec());
}

#[test]
fn an_attachment_that_is_not_there_is_nothing() {
    assert_eq!(attachment_offer(WITH_ATTACHMENT, 1), Err(Refusal::Nothing));
    assert_eq!(attachment_offer(b"not a message", 0), Err(Refusal::Nothing));
}

#[test]
fn a_share_is_staged_under_the_data_root_by_the_uid_and_the_place_of_the_part() {
    let root = Path::new("/data");
    assert_eq!(staged_path(root, 7, None), Path::new("/data/staged-shares/7.eml"));
    assert_eq!(staged_path(root, 7, Some(2)), Path::new("/data/staged-shares/7-2.part"));
    assert_ne!(staged_path(root, 7, Some(1)), staged_path(root, 7, Some(2)));
}

#[test]
fn the_status_bar_says_nothing_where_the_sheet_is_up_and_the_reason_where_it_is_not() {
    crate::l10n::in_english();
    assert_eq!(notice_of(&Handed::Shown), "");
    let unsupported = notice_of(&Handed::Unsupported(String::from("no share sheet")));
    assert!(unsupported.starts_with("Sharing is not available here"), "{unsupported}");
    assert!(unsupported.contains("no share sheet"), "{unsupported}");
    assert!(notice_of(&Handed::Failed(String::from("boom"))).contains("boom"));
    assert!(notice_of(&Handed::Refused(String::from("too many"))).contains("too many"));
    assert_eq!(refusal_notice(&Refusal::Nothing), "There is nothing to share.");
    assert!(refusal_notice(&Refusal::TooBig(300 * 1024 * 1024)).contains("too big"));
}
