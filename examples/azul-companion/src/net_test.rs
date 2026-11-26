//! Where the pairing is kept, and what the app's connection starts as.

use azul_review_link::store::{SavedMac, Stored};
use azul_review_link::view::Session;

use super::*;

/// A folder of its own for one test, removed when the test is over.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("azcompanion-net-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn did_not_start(net: &Net) -> bool {
    net.notes.iter().any(|n| n.contains("the network did not start"))
}

#[test]
fn the_folder_is_the_variable_then_the_systems_data_folder_then_the_home_folder() {
    let os = Some(PathBuf::from("/data"));
    let home = Some(PathBuf::from("/home/u"));
    assert_eq!(data_dir_from(Some("/x/y"), os.clone(), home.clone()), Some(PathBuf::from("/x/y")));
    assert_eq!(data_dir_from(Some("  "), os.clone(), home.clone()), Some(PathBuf::from("/data/AzCompanion")), "a blank variable is no variable");
    assert_eq!(data_dir_from(None, os, home.clone()), Some(PathBuf::from("/data/AzCompanion")));
    assert_eq!(data_dir_from(None, None, home), Some(PathBuf::from("/home/u/.azcompanion")));
    assert_eq!(data_dir_from(None, Some(PathBuf::new()), Some(PathBuf::new())), None, "an empty path is no folder");
    assert_eq!(data_dir_from(None, None, None), None);
}

#[test]
fn a_phone_is_named_by_its_kind_and_the_start_of_its_key() {
    let name = phone_name("ab12cd34");
    assert!(name.ends_with(" ab12"), "{name}");
    assert!(!phone_name("").ends_with(' ') && !phone_name("").is_empty(), "no key: just the kind");
    assert_ne!(phone_name("aaaa1"), phone_name("bbbb1"), "two phones differ on the Mac's screen");
}

#[test]
fn a_new_phone_makes_its_key_keeps_it_and_starts_unpaired() {
    let dir = Scratch::new("new");
    let net = connect_in(Some(&dir.0), "0.1.0");
    assert_eq!(net.api.view().session, Session::Unpaired);
    let kept = FileStore::new(dir.0.join(STORE_FILE)).load().expect("what was kept can be read");
    assert!(kept.secret_key.is_some(), "the key is kept whether or not the network started");
    assert!(net.notes.iter().any(|n| n.contains("the pairing is kept in")), "{:?}", net.notes);
    // the next start has the same key: the Mac would know the phone
    let again = connect_in(Some(&dir.0), "0.1.0");
    assert_eq!(FileStore::new(dir.0.join(STORE_FILE)).load().expect("read").secret_key, kept.secret_key);
    drop((net, again));
}

#[test]
fn a_phone_that_was_paired_looks_for_its_mac_at_the_start() {
    let dir = Scratch::new("paired");
    let mac = SavedMac { mac_id: "ab12".into(), name: "Studio Mac".into(), tickets: vec!["some-ticket".into()] };
    FileStore::new(dir.0.join(STORE_FILE)).save(&Stored { secret_key: Some([2; 32]), mac: Some(mac) }).expect("kept");
    let net = connect_in(Some(&dir.0), "0.1.0");
    if did_not_start(&net) {
        // a library without the engine: the phone still runs, on a connection that finds no Mac
        assert_eq!(net.api.view().session, Session::Unpaired);
    } else {
        assert_eq!(net.api.view().session, Session::Searching { attempt: 1 });
        assert_eq!(net.api.view().mac_name, "Studio Mac");
    }
}

#[test]
fn a_phone_with_no_folder_keeps_nothing_and_says_so() {
    let net = connect_in(None, "0.1.0");
    assert!(net.notes.iter().any(|n| n.contains("paired again at every start")), "{:?}", net.notes);
    assert_eq!(net.api.view().session, Session::Unpaired);
}

#[test]
fn a_folder_that_cannot_be_used_is_said_and_the_phone_still_starts() {
    let dir = Scratch::new("blocked");
    std::fs::create_dir_all(&dir.0).expect("a folder");
    // a file where the folder should be: nothing can be read or written under it
    let blocker = dir.0.join("not-a-folder");
    std::fs::write(&blocker, b"x").expect("a file");
    let net = connect_in(Some(&blocker), "0.1.0");
    assert!(net.notes.iter().any(|n| n.contains("could not be read")), "{:?}", net.notes);
    assert!(net.notes.iter().any(|n| n.contains("could not be kept")), "{:?}", net.notes);
    assert_eq!(net.api.view().session, Session::Unpaired);
}
