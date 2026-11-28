//! The working copies of what the app was opened with: names, caps, cleanup.

use std::{
    fs,
    io::{Cursor, Read},
    path::PathBuf,
    time::{Duration, SystemTime},
};

use super::*;
use crate::managers::share_area::new_token;

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("azul-inbox-test-{}", new_token()));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn inbox(&self) -> Inbox {
        Inbox::new(self.0.join("files").join("azul-inbox"))
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}

fn small() -> OpenLimits {
    OpenLimits {
        max_file_bytes: 1024,
        max_request_bytes: 2048,
        max_name_bytes: 32,
    }
}

#[test]
fn a_file_is_copied_under_a_safe_name_with_its_bytes() {
    let dir = TestDir::new();
    let mut request = dir.inbox().begin(OpenLimits::default()).unwrap();
    let copied = request.copy_in("../../etc/Report: final?.PDF", &mut Cursor::new(b"%PDF-1.7 hello".to_vec()), Some(14)).unwrap();
    assert_eq!(copied.name, "Report_ final_.PDF");
    assert_eq!(copied.size, 14);
    assert!(copied.path.starts_with(request.dir()));
    assert_eq!(fs::read(&copied.path).unwrap(), b"%PDF-1.7 hello");
    assert_eq!(request.total(), 14);
}

#[test]
fn two_files_of_one_name_in_one_request_get_two_names() {
    let dir = TestDir::new();
    let mut request = dir.inbox().begin(OpenLimits::default()).unwrap();
    let a = request.copy_in("image.png", &mut Cursor::new(vec![1, 2, 3]), None).unwrap();
    let b = request.copy_in("image.png", &mut Cursor::new(vec![4, 5]), None).unwrap();
    assert_eq!((a.name.as_str(), b.name.as_str()), ("image.png", "image (2).png"));
    assert_eq!(fs::read(a.path).unwrap(), [1, 2, 3]);
    assert_eq!(fs::read(b.path).unwrap(), [4, 5]);
}

#[test]
fn a_file_whose_size_is_known_to_be_over_the_limit_is_refused_before_a_byte_is_read() {
    struct Boom;
    impl Read for Boom {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("nothing may be read of a file that is too large")
        }
    }
    let dir = TestDir::new();
    let mut request = dir.inbox().begin(small()).unwrap();
    let why = request.copy_in("big.bin", &mut Boom, Some(5 * 1024 * 1024)).unwrap_err();
    assert!(why.contains("big.bin") && why.contains("5 MiB") && why.contains("too large"), "{why}");
    assert_eq!(fs::read_dir(request.dir()).unwrap().count(), 0, "no file is left");
}

#[test]
fn a_stream_that_grows_past_the_limit_is_stopped_and_its_partial_file_removed() {
    let dir = TestDir::new();
    let mut request = dir.inbox().begin(small()).unwrap();
    // the provider said nothing of the size (a cloud file) and sends 5000 bytes
    let why = request.copy_in("stream.bin", &mut Cursor::new(vec![7_u8; 5000]), None).unwrap_err();
    assert!(why.contains("too large"), "{why}");
    assert_eq!(fs::read_dir(request.dir()).unwrap().count(), 0);
    assert_eq!(request.total(), 0, "a refused file costs nothing of the budget");
}

#[test]
fn the_files_of_one_request_share_a_budget() {
    let dir = TestDir::new();
    let mut request = dir.inbox().begin(small()).unwrap();
    request.copy_in("one.bin", &mut Cursor::new(vec![0_u8; 1000]), Some(1000)).unwrap();
    request.copy_in("two.bin", &mut Cursor::new(vec![0_u8; 1000]), Some(1000)).unwrap();
    // 2000 used of 2048: a third of 1000 bytes does not fit, with or without a size hint
    let known = request.copy_in("three.bin", &mut Cursor::new(vec![0_u8; 1000]), Some(1000)).unwrap_err();
    assert!(known.contains("no room left"), "{known}");
    let unknown = request.copy_in("four.bin", &mut Cursor::new(vec![0_u8; 1000]), None).unwrap_err();
    assert!(unknown.contains("no room left"), "{unknown}");
    // a small one still fits
    assert!(request.copy_in("five.bin", &mut Cursor::new(vec![0_u8; 40]), Some(40)).is_ok());
    assert_eq!(request.total(), 2040);
}

#[test]
fn a_reader_that_fails_says_where_and_leaves_no_file() {
    struct Broken(u8);
    impl Read for Broken {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.0 == 0 {
                self.0 = 1;
                buf[..4].copy_from_slice(b"abcd");
                Ok(4)
            } else {
                Err(std::io::Error::other("the provider went away"))
            }
        }
    }
    let dir = TestDir::new();
    let mut request = dir.inbox().begin(OpenLimits::default()).unwrap();
    let why = request.copy_in("cloud.docx", &mut Broken(0), Some(100)).unwrap_err();
    assert!(why.contains("cloud.docx") && why.contains("the provider went away"), "{why}");
    assert_eq!(fs::read_dir(request.dir()).unwrap().count(), 0);
}

#[test]
fn an_empty_file_is_a_file() {
    let dir = TestDir::new();
    let mut request = dir.inbox().begin(OpenLimits::default()).unwrap();
    let copied = request.copy_in("empty.txt", &mut Cursor::new(Vec::new()), Some(0)).unwrap();
    assert_eq!(copied.size, 0);
    assert!(copied.path.is_file());
}

#[test]
fn every_request_has_a_folder_of_its_own() {
    let dir = TestDir::new();
    let inbox = dir.inbox();
    let first = inbox.begin(OpenLimits::default()).unwrap();
    let second = inbox.begin(OpenLimits::default()).unwrap();
    assert_ne!(first.dir(), second.dir());
    assert!(first.dir().starts_with(inbox.root()) && second.dir().starts_with(inbox.root()));
}

#[test]
fn only_folders_nobody_touched_for_a_month_are_removed() {
    let dir = TestDir::new();
    let inbox = dir.inbox();
    let mut old = inbox.begin(OpenLimits::default()).unwrap();
    old.copy_in("a.txt", &mut Cursor::new(vec![1]), None).unwrap();
    let old_dir = old.dir().to_path_buf();
    fs::create_dir(inbox.root().join("not-a-request")).unwrap();
    let now = SystemTime::now();
    assert_eq!(inbox.cleanup(now, KEEP_FOR), 0, "a young request stays");
    let in_two_months = now + Duration::from_secs(60 * 24 * 3600);
    assert_eq!(inbox.cleanup(in_two_months, KEEP_FOR), 1);
    assert!(!old_dir.exists());
    assert!(inbox.root().join("not-a-request").is_dir(), "what is not a request is not ours to remove");
}

#[test]
fn a_hostile_name_is_made_into_a_plain_one() {
    let dir = TestDir::new();
    let mut request = dir.inbox().begin(small()).unwrap();
    for (wanted, expect) in [
        ("..", "file"),
        ("", "file"),
        ("a/b/c.txt", "c.txt"),
        (".hidden", "hidden"),
        ("tab\tnew\nline.txt", "tab_new_line.txt"),
    ] {
        let copied = request.copy_in(wanted, &mut Cursor::new(vec![0]), None).unwrap();
        assert!(copied.name.starts_with(expect.split('.').next().unwrap()), "{wanted:?} -> {}", copied.name);
        assert!(copied.path.parent() == Some(request.dir()), "stays inside the request folder: {wanted:?}");
    }
}
