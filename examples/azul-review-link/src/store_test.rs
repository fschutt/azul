//! What the phone keeps: the text round trip, a file that is damaged or foreign or hostile, the atomic write, the shared memory store.

use super::*;

fn mac() -> SavedMac {
    SavedMac { mac_id: "ab12".repeat(16), name: "Studio Mac".into(), tickets: vec!["newest-ticket".into(), "older_ticket".into()] }
}

fn everything() -> Stored {
    Stored { secret_key: Some([7; 32]), mac: Some(mac()) }
}

/// A folder of its own for one test, removed when the test is over.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("azrv-store-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Scratch(dir)
    }

    fn file(&self) -> PathBuf {
        self.0.join("phone").join("companion.txt")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn what_is_kept_reads_back_the_same() {
    let text = render(&everything());
    assert!(text.starts_with("azreview-companion 1\n"));
    assert_eq!(parse(&text), Ok(everything()));
    // a phone that has a key and no Mac yet, and the other way round
    let key_only = Stored { secret_key: Some([1; 32]), mac: None };
    assert_eq!(parse(&render(&key_only)), Ok(key_only));
    assert_eq!(parse(&render(&Stored::default())), Ok(Stored::default()));
}

#[test]
fn the_text_is_for_a_person_and_has_no_secret_but_the_key() {
    let text = render(&everything());
    assert!(text.contains("\nmac ab12ab12"), "{text}");
    assert!(text.contains("\nname Studio Mac\n"));
    assert!(text.contains("\nticket newest-ticket\nticket older_ticket\n"));
    assert!(text.contains(&format!("\nkey {}\n", "07".repeat(32))));
}

#[test]
fn a_file_that_is_not_ours_or_is_newer_is_said_so_and_read_as_nothing() {
    assert_eq!(parse(""), Err("it is not a companion's file".to_string()));
    assert_eq!(parse("hello\nkey 00\n"), Err("it is not a companion's file".to_string()));
    assert_eq!(parse("azreview-companion 2\nmac ab\n"), Err("it is of another version of the companion".to_string()));
}

#[test]
fn lines_it_does_not_know_and_values_that_are_wrong_are_left_out_one_by_one() {
    let text = "azreview-companion 1\nfuture-thing 1 2 3\nkey nothex\nmac ab12\nname  A Mac \u{7}with\u{0} controls \nticket ok-1\nticket not a ticket!\nticket ok-1\nticket ok_2\n\nnoise";
    let got = parse(text).expect("a companion's file");
    assert_eq!(got.secret_key, None, "a key that is not 64 hex characters is no key");
    let mac = got.mac.expect("the Mac line is good");
    assert_eq!(mac.mac_id, "ab12");
    assert_eq!(mac.name, "A Mac with controls", "controls and the edges are cleaned away");
    assert_eq!(mac.tickets, ["ok-1", "ok_2"], "a ticket with spaces is no ticket, a repeated one is kept once");
}

#[test]
fn a_mac_whose_key_is_not_hex_is_no_mac() {
    assert_eq!(parse("azreview-companion 1\nmac XYZ\nname x\nticket t1\n").map(|s| s.mac), Ok(None));
    assert_eq!(parse("azreview-companion 1\nmac \nname x\n").map(|s| s.mac), Ok(None));
    let long = "a".repeat(129);
    assert_eq!(parse(&format!("azreview-companion 1\nmac {long}\n")).map(|s| s.mac), Ok(None), "over the cap");
}

#[test]
fn at_most_four_tickets_and_a_name_cut_on_a_character() {
    let mut text = String::from("azreview-companion 1\nmac ab\n");
    for i in 0..9 {
        text.push_str(&format!("ticket t{i}\n"));
    }
    text.push_str(&format!("name {}\n", "é".repeat(200)));
    let mac = parse(&text).expect("read").mac.expect("a Mac");
    assert_eq!(mac.tickets.len(), MAX_SAVED_TICKETS);
    assert!(mac.name.len() <= MAX_NAME_BYTES && mac.name.chars().all(|c| c == 'é'), "cut on a character boundary");
    // and what is written is held to the same: nothing over four, nothing that is no ticket
    let big = Stored {
        secret_key: None,
        mac: Some(SavedMac { mac_id: "ab".into(), name: "n\nl".into(), tickets: (0..9).map(|i| format!("t{i}")).chain(["bad ticket".to_string()]).collect() }),
    };
    let written = render(&big);
    assert_eq!(written.matches("\nticket ").count(), MAX_SAVED_TICKETS);
    assert!(written.contains("\nname nl\n"), "a newline in a name cannot start a new line of the file: {written}");
}

#[test]
fn a_file_store_that_was_never_written_is_empty_and_no_error() {
    let scratch = Scratch::new("never");
    assert_eq!(FileStore::new(scratch.file()).load(), Ok(Stored::default()));
}

#[test]
fn a_file_store_writes_atomically_in_folders_it_makes_and_reads_back() {
    let scratch = Scratch::new("roundtrip");
    let mut store = FileStore::new(scratch.file());
    assert_eq!(store.save(&everything()), Ok(()));
    assert_eq!(FileStore::new(scratch.file()).load(), Ok(everything()));
    assert!(!scratch.file().with_extension("txt.new").exists(), "the temporary file is gone");
    // a second save replaces the first
    let only_key = Stored { secret_key: Some([9; 32]), mac: None };
    assert_eq!(store.save(&only_key), Ok(()));
    assert_eq!(store.load(), Ok(only_key));
}

#[cfg(unix)]
#[test]
fn the_file_is_the_users_alone() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new("mode");
    FileStore::new(scratch.file()).save(&everything()).expect("saved");
    let mode = std::fs::metadata(scratch.file()).expect("the file").permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "it holds the phone's key");
}

#[test]
fn a_damaged_file_is_an_error_text_and_the_phone_starts_empty() {
    let scratch = Scratch::new("damaged");
    std::fs::create_dir_all(scratch.file().parent().expect("a folder")).expect("folder");
    std::fs::write(scratch.file(), b"\xff\xfe not text at all").expect("written");
    let error = FileStore::new(scratch.file()).load().expect_err("not text");
    assert!(error.contains("not text"), "{error}");
    std::fs::write(scratch.file(), "something else entirely\n").expect("written");
    let error = FileStore::new(scratch.file()).load().expect_err("not ours");
    assert!(error.contains("not a companion's file"), "{error}");
}

#[test]
fn a_file_over_the_cap_is_not_read() {
    let scratch = Scratch::new("big");
    std::fs::create_dir_all(scratch.file().parent().expect("a folder")).expect("folder");
    let mut text = String::from("azreview-companion 1\n");
    while (text.len() as u64) <= MAX_STORE_BYTES {
        text.push_str("noise noise noise noise noise noise noise noise\n");
    }
    std::fs::write(scratch.file(), text).expect("written");
    let error = FileStore::new(scratch.file()).load().expect_err("over the cap");
    assert!(error.contains("larger than"), "{error}");
}

#[test]
fn a_folder_where_the_file_should_be_is_an_error_not_a_crash() {
    let scratch = Scratch::new("isdir");
    std::fs::create_dir_all(scratch.file()).expect("a folder in the file's place");
    assert!(FileStore::new(scratch.file()).load().is_err());
    assert!(FileStore::new(scratch.file()).save(&everything()).is_err());
}

#[test]
fn the_memory_store_is_shared_between_its_clones_and_counts_what_was_saved() {
    let store = MemoryStore::new();
    let mut writer = store.clone();
    assert_eq!(writer.load(), Ok(Stored::default()));
    writer.save(&everything()).expect("saved");
    writer.save(&everything()).expect("saved");
    assert_eq!(store.peek(), everything(), "what one clone wrote the other sees");
    assert_eq!(store.saves(), 2);
    assert_eq!(MemoryStore::with(everything()).peek(), everything());
}

#[test]
fn the_phones_key_is_made_once_kept_and_read_back_after() {
    use crate::pairing::SeededEntropy;
    let store = MemoryStore::new();
    let mut writer = store.clone();
    let (first, said) = load_identity(&mut writer, &mut SeededEntropy(11));
    assert!(said.is_empty(), "{said:?}");
    assert_ne!(first, [0; 32], "a key, not zeros");
    assert_eq!(store.peek().secret_key, Some(first), "kept");
    // the second start has another source of randomness and still has the key it had
    let (second, _) = load_identity(&mut writer, &mut SeededEntropy(99));
    assert_eq!(second, first);
    assert_eq!(store.saves(), 1, "kept once");
    // the Mac that was kept is not touched by making the key
    let mut with_mac = MemoryStore::with(Stored { secret_key: None, mac: Some(mac()) });
    let (key, _) = load_identity(&mut with_mac, &mut SeededEntropy(5));
    assert_eq!(with_mac.peek(), Stored { secret_key: Some(key), mac: Some(mac()) });
}

#[test]
fn a_store_that_cannot_be_read_or_written_still_gives_a_key_and_says_so() {
    use crate::pairing::SeededEntropy;
    let scratch = Scratch::new("identity-broken");
    std::fs::create_dir_all(scratch.file()).expect("a folder where the file should be");
    let mut store = FileStore::new(scratch.file());
    let (key, said) = load_identity(&mut store, &mut SeededEntropy(3));
    assert_ne!(key, [0; 32]);
    assert_eq!(said.len(), 2, "one for reading, one for keeping: {said:?}");
}
