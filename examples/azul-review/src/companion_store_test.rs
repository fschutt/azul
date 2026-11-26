//! What the companion keeps, without a window: the file's text both ways, what a damaged file makes, the caps, and the text the
//! computer announces on the local network.

use azul_review_link::advert;

use super::*;

fn key_of(byte: u8) -> [u8; KEY_BYTES] {
    [byte; KEY_BYTES]
}

fn phone(id: &str, name: &str) -> TrustRecord {
    TrustRecord::phone(id, name)
}

#[test]
fn nothing_kept_is_the_companion_off_with_no_key_and_no_phone() {
    let stored = Stored::default();
    assert!(!stored.enabled, "the companion is off until the user turns it on");
    assert_eq!(stored.key(), None);
    assert!(stored.phones.is_empty());
    for blank in ["", "  ", "\n\n"] {
        let (read, problems) = Stored::parse(blank);
        assert_eq!(read, Stored::default(), "a file with nothing in it is the default: {blank:?}");
        assert!(problems.is_empty(), "and it is not a problem: a first start has none");
    }
}

#[test]
fn what_is_kept_is_read_back_as_it_was() {
    let mut stored = Stored { enabled: true, ..Stored::default() };
    stored.set_key(key_of(0xAB));
    stored.phones = vec![phone("a".repeat(64).as_str(), "Sam's phone"), phone("b".repeat(64).as_str(), "Test phone")];
    let (read, problems) = Stored::parse(&stored.to_json());
    assert_eq!(read, stored);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn the_file_is_written_the_same_way_every_time_with_the_key_as_lowercase_hex() {
    let mut stored = Stored { enabled: true, ..Stored::default() };
    stored.set_key(key_of(0xAB));
    let text = stored.to_json();
    assert_eq!(text, stored.to_json());
    assert!(text.ends_with("}\n"), "a final newline: {text:?}");
    assert!(text.contains(&format!("\"key\": \"{}\"", "ab".repeat(KEY_BYTES))), "{text}");
    assert!(text.contains("\"version\": 1"), "{text}");
    assert!(text.contains("\"enabled\": true"), "{text}");
}

#[test]
fn a_log_line_never_says_the_key() {
    let mut stored = Stored::default();
    stored.set_key(key_of(0xCD));
    let said = format!("{stored:?} {:#?}", stored);
    assert!(!said.contains("cdcd"), "the key is a secret: {said}");
    assert!(!said.contains("205"), "nor its bytes in decimal: {said}");
    assert!(said.contains("<kept>"));
}

#[test]
fn a_file_that_is_not_json_is_the_default_and_says_so() {
    let (read, problems) = Stored::parse("this is not json");
    assert_eq!(read, Stored::default());
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("not readable"), "{problems:?}");
    // JSON of another shape is the same
    let (read, problems) = Stored::parse("[1, 2, 3]");
    assert_eq!(read, Stored::default());
    assert_eq!(problems.len(), 1);
}

#[test]
fn a_key_that_is_no_key_is_dropped_and_said_and_the_rest_is_kept() {
    let bad_keys: Vec<String> = vec!["abc".to_string(), "z".repeat(64), "ab".repeat(31), "ab".repeat(33), "é".repeat(32)];
    for bad in &bad_keys {
        let text = format!("{{\"version\": 1, \"enabled\": true, \"key\": \"{bad}\", \"phones\": [{{\"id\": \"p1\", \"name\": \"Phone\"}}]}}");
        let (read, problems) = Stored::parse(&text);
        assert_eq!(read.key(), None, "{bad:?}");
        assert!(read.enabled, "the switch is the user's and stays: {bad:?}");
        assert_eq!(read.phones.len(), 1, "the phone stays: {bad:?}");
        assert!(problems.iter().any(|p| p.contains("key")), "{bad:?}: {problems:?}");
    }
}

#[test]
fn a_key_in_capitals_is_read() {
    assert_eq!(from_hex_key(&"AB".repeat(KEY_BYTES)), Some(key_of(0xAB)));
    assert_eq!(from_hex_key(&format!(" {} ", "ab".repeat(KEY_BYTES))), Some(key_of(0xAB)), "a space round it does not matter");
    assert_eq!(from_hex_key(""), None);
    assert_eq!(from_hex_key(&"é".repeat(32)), None, "64 bytes that are not 64 digits (and must not be cut inside a character)");
}

#[test]
fn phones_without_a_usable_key_and_phones_twice_are_left_out() {
    let text = r#"{"version": 1, "phones": [
        {"id": "", "name": "No key"},
        {"id": "has space", "name": "Bad key"},
        {"id": "p1", "name": "First"},
        {"id": "p1", "name": "Again"},
        {"id": "p2", "name": "Second"}
    ]}"#;
    let (read, problems) = Stored::parse(text);
    assert_eq!(read.phones, vec![phone("p1", "First"), phone("p2", "Second")]);
    assert_eq!(problems.len(), 2, "one line for each of the two that had no key: {problems:?}");
}

#[test]
fn more_phones_than_the_cap_keep_the_first_ones_and_say_so() {
    let phones: Vec<String> = (0..MAX_PAIRED_PHONES + 3).map(|i| format!("{{\"id\": \"p{i}\", \"name\": \"Phone {i}\"}}")).collect();
    let text = format!("{{\"version\": 1, \"phones\": [{}]}}", phones.join(","));
    let (read, problems) = Stored::parse(&text);
    assert_eq!(read.phones.len(), MAX_PAIRED_PHONES);
    assert_eq!(read.phones[0].peer, "p0");
    assert_eq!(read.phones[MAX_PAIRED_PHONES - 1].peer, format!("p{}", MAX_PAIRED_PHONES - 1));
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains(&MAX_PAIRED_PHONES.to_string()), "{problems:?}");
}

#[test]
fn a_phone_name_is_cleaned_and_cut_on_a_character_boundary() {
    assert_eq!(clean_name("  Sam\u{0}'s\nphone\t "), "Sam'sphone");
    let long = "ä".repeat(200);
    let cut = clean_name(&long);
    assert!(cut.len() <= MAX_NAME_BYTES, "{} bytes", cut.len());
    assert!(cut.chars().all(|c| c == 'ä'), "no half of a character is left");
    assert_eq!(clean_name(""), "");
}

#[test]
fn the_phones_the_host_names_are_kept_cut_to_the_cap_and_cleaned() {
    let records: Vec<TrustRecord> = (0..7).map(|i| phone(&format!("p{i}"), &format!("Name\n{i}"))).collect();
    let mut stored = Stored::default();
    stored.set_phones(&records);
    assert_eq!(stored.phones.len(), MAX_PAIRED_PHONES);
    assert_eq!(stored.phones[1], phone("p1", "Name1"));
    stored.set_phones(&[phone("bad id", "x"), phone("good", "y")]);
    assert_eq!(stored.phones, vec![phone("good", "y")], "a key that is none is not kept");
}

#[test]
fn a_field_this_build_does_not_know_is_ignored_and_a_newer_file_is_read_as_far_as_it_goes() {
    let text = r#"{"version": 7, "enabled": true, "future": {"a": 1}, "phones": []}"#;
    let (read, problems) = Stored::parse(text);
    assert!(read.enabled);
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("newer"), "{problems:?}");
}

#[test]
fn the_announcement_says_what_this_is_and_its_name_and_nothing_else() {
    let text = advertised("Studio Mac");
    let parsed = advert::parse(&text).expect("the text is one of the format");
    assert_eq!(parsed.name, "Studio Mac");
    assert_eq!(parsed.role, Some(Role::Host));
    assert!(parsed.user.is_empty(), "no user name: {text}");
    assert!(text.len() <= advert::MAX_ADVERT_BYTES, "{} bytes", text.len());
    assert!(!text[advert::ADVERT_PREFIX.len()..].contains('/'), "no path: {text}");
    // it is the same for every folder and every key: it is made from the name alone
    assert_eq!(text, advertised("Studio Mac"));
    assert_ne!(text, advertised("Other"));
}

#[test]
fn the_announcement_has_no_key_in_it_whatever_the_name() {
    let mut stored = Stored::default();
    stored.set_key(key_of(0x5A));
    let hex = to_hex(&stored.key().expect("a key"));
    let names: Vec<String> = vec![DEFAULT_NAME.to_string(), String::new(), "x".repeat(500), "a;b=c\\d\nline".to_string(), "\u{202E}reversed".to_string()];
    for name in &names {
        let text = advertised(name);
        assert!(!text.contains(&hex), "{name:?}");
        assert!(!text.contains("5a5a5a"), "{name:?}");
        assert!(text.len() <= advert::MAX_ADVERT_BYTES, "{name:?}: {} bytes", text.len());
        assert!(advert::parse(&text).is_some(), "{name:?}: {text}");
        assert!(!text.contains('\n'), "{name:?}");
    }
}

#[test]
fn the_file_is_in_the_bookkeeping_folder_of_the_data_root_where_a_drive_does_not_look() {
    let root = Path::new("/data/Azlin");
    let path = store_path(root, "review");
    assert!(path.starts_with(root), "{}", path.display());
    assert!(path.ends_with(".azlin/companion/review.json"), "{}", path.display());
    // another app's folder is another file
    assert_ne!(store_path(root, "other"), path);
}
