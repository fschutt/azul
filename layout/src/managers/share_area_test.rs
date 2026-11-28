//! The folder a share's copies live in: staging, the types file, what a provider URI may name, and the cleanup.

use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use azul_core::share::{ShareRequest, ShareStatus};
use azul_css::AzString;

use super::*;
use crate::managers::share::{plan, ShareLimits};

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("azul-share-area-test-{}", new_token()));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn source(&self, name: &str, bytes: &[u8]) -> AzString {
        let path = self.0.join("src").join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        AzString::from(path.to_string_lossy().into_owned())
    }
    fn area(&self) -> ShareArea {
        ShareArea::new(self.0.join("cache").join("azul-share"))
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}

fn stage_files(dir: &TestDir, files: &[(&str, &[u8])]) -> StagedShare {
    let mut request = ShareRequest::new();
    for (name, bytes) in files {
        request = request.with_item(azul_core::share::ShareItem::file(dir.source(name, bytes)));
    }
    let limits = ShareLimits::default();
    let planned = plan(&request, &limits).unwrap();
    dir.area().stage(&planned, &limits).unwrap()
}

#[test]
fn a_token_is_32_lower_case_hex_characters_and_never_repeats() {
    let a = new_token();
    let b = new_token();
    assert_eq!(a.len(), TOKEN_LEN);
    assert!(is_token(&a) && is_token(&b));
    assert_ne!(a, b);
    assert!(!is_token("ABCDEF0123456789ABCDEF0123456789"), "upper case is not a token");
    assert!(!is_token(&a[..31]));
    assert!(!is_token(&format!("{a}0")));
    assert!(!is_token("g".repeat(32).as_str()));
    assert!(!is_token(""));
}

#[test]
fn staging_copies_the_files_under_their_shown_names_with_their_types() {
    let dir = TestDir::new();
    let staged = stage_files(&dir, &[("Report.PDF", b"%PDF-1.7 one"), ("notes.md", b"# two")]);
    assert!(is_token(&staged.token));
    assert_eq!(staged.dir, dir.area().root().join(&staged.token));
    let names: Vec<&str> = staged.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["Report.PDF", "notes.md"]);
    assert_eq!(staged.files[0].mime, "application/pdf");
    assert_eq!(staged.files[0].size, 12);
    assert_eq!(fs::read(&staged.files[0].path).unwrap(), b"%PDF-1.7 one");
    assert_eq!(fs::read(&staged.files[1].path).unwrap(), b"# two");
    let types = fs::read_to_string(staged.dir.join(TYPES_FILE)).unwrap();
    assert_eq!(types, "Report.PDF\tapplication/pdf\nnotes.md\ttext/markdown\n");
}

#[test]
fn the_copy_is_a_copy_the_apps_file_is_untouched() {
    let dir = TestDir::new();
    let staged = stage_files(&dir, &[("a.txt", b"original")]);
    fs::write(&staged.files[0].path, b"changed by the receiver's side").unwrap();
    assert_eq!(fs::read(dir.0.join("src").join("a.txt")).unwrap(), b"original");
}

#[test]
fn a_provider_uri_path_resolves_to_the_staged_file_and_describes_it() {
    let dir = TestDir::new();
    let staged = stage_files(&dir, &[("Plan v2.md", b"hello"), ("x.unknownext", b"1234")]);
    let area = dir.area();
    let uri_path = format!("/{}/Plan v2.md", staged.token);
    let resolved = area.resolve(&uri_path).unwrap();
    assert_eq!(fs::read(resolved).unwrap(), b"hello");
    let info = area.describe(&uri_path).unwrap();
    assert_eq!(info, ShareFileInfo { name: "Plan v2.md".into(), size: 5, mime: "text/markdown".into() });
    let other = area.describe(&format!("/{}/x.unknownext", staged.token)).unwrap();
    assert_eq!((other.size, other.mime.as_str()), (4, "application/octet-stream"));
}

#[test]
fn a_uri_path_that_names_anything_else_is_refused() {
    let dir = TestDir::new();
    let staged = stage_files(&dir, &[("a.txt", b"x")]);
    let area = dir.area();
    let t = &staged.token;
    // a secret next to the area and one inside the share's folder under a hidden name
    fs::write(area.root().join("secret.txt"), b"secret").unwrap();
    let refused = [
        format!("/{t}/../secret.txt"),
        format!("/{t}/.."),
        format!("/{t}/."),
        format!("/{t}/{TYPES_FILE}"),
        format!("/{t}/a.txt/"),
        format!("/{t}/sub/a.txt"),
        format!("/{t}/a.txt\0"),
        format!("/{t}/..\\a.txt"),
        format!("/{t}/"),
        format!("/{t}"),
        "/".to_string(),
        String::new(),
        format!("/{}/a.txt", "0".repeat(32)),
        "/ABCDEF0123456789ABCDEF0123456789/a.txt".to_string(),
        "/../a.txt".to_string(),
        format!("/{t}/a.txt/../../secret.txt"),
        format!("/{t}/{}", "n".repeat(300)),
    ];
    for path in &refused {
        assert!(area.resolve(path).is_none(), "{path:?} was served");
        assert!(area.describe(path).is_none(), "{path:?} was described");
    }
    // the token's folder itself is not a file
    assert!(area.resolve(&format!("/{t}/")).is_none());
    // the right one still works
    assert!(area.resolve(&format!("/{t}/a.txt")).is_some());
    // and without a leading slash too (what a different Uri API hands over)
    assert!(area.resolve(&format!("{t}/a.txt")).is_some());
}

#[cfg(unix)]
#[test]
fn a_link_inside_a_share_that_leaves_the_area_is_not_served() {
    let dir = TestDir::new();
    let staged = stage_files(&dir, &[("a.txt", b"x")]);
    let outside = dir.0.join("outside.txt");
    fs::write(&outside, b"not for the receiver").unwrap();
    std::os::unix::fs::symlink(&outside, staged.dir.join("link.txt")).unwrap();
    let area = dir.area();
    assert!(area.resolve(&format!("/{}/link.txt", staged.token)).is_none());
    assert!(area.resolve(&format!("/{}/a.txt", staged.token)).is_some());
}

#[test]
fn a_share_that_cannot_be_staged_leaves_nothing_behind() {
    let dir = TestDir::new();
    let good = dir.source("good.txt", b"x");
    let limits = ShareLimits::default();
    let planned = plan(&ShareRequest::file(good).with_item(azul_core::share::ShareItem::file(dir.source("gone.txt", b"y"))), &limits).unwrap();
    // the second file disappears between the check and the copy
    fs::remove_file(dir.0.join("src").join("gone.txt")).unwrap();
    let area = dir.area();
    let status = area.stage(&planned, &limits).unwrap_err();
    match status {
        ShareStatus::Failed(reason) => assert!(reason.as_str().contains("gone.txt"), "{}", reason.as_str()),
        other => panic!("{other:?}"),
    }
    let left: Vec<_> = fs::read_dir(area.root()).unwrap().collect();
    assert!(left.is_empty(), "{left:?}");
}

#[test]
fn a_file_that_grew_past_the_limit_after_the_check_is_not_shared() {
    let dir = TestDir::new();
    let path = dir.source("grows.bin", &[0; 10]);
    let limits = ShareLimits { max_file_bytes: 100, ..ShareLimits::default() };
    let planned = plan(&ShareRequest::file(path.clone()), &limits).unwrap();
    fs::write(path.as_str(), vec![0_u8; 5000]).unwrap();
    let area = dir.area();
    assert!(matches!(area.stage(&planned, &limits), Err(ShareStatus::Failed(_))));
    assert!(fs::read_dir(area.root()).unwrap().next().is_none());
}

#[test]
fn old_shares_are_removed_and_young_ones_stay() {
    let dir = TestDir::new();
    let first = stage_files(&dir, &[("a.txt", b"1")]);
    let second = stage_files(&dir, &[("b.txt", b"2")]);
    let area = dir.area();
    let now = SystemTime::now();
    assert_eq!(area.cleanup(now, MAX_AGE), 0, "both are young");
    assert!(first.dir.is_dir() && second.dir.is_dir());
    let in_two_days = now + Duration::from_secs(2 * 24 * 3600);
    assert_eq!(area.cleanup(in_two_days, MAX_AGE), 2);
    assert!(!first.dir.exists() && !second.dir.exists());
}

#[test]
fn the_cleanup_touches_only_folders_named_by_a_token() {
    let dir = TestDir::new();
    let staged = stage_files(&dir, &[("a.txt", b"1")]);
    let area = dir.area();
    fs::create_dir(area.root().join("not-a-share")).unwrap();
    fs::write(area.root().join("loose-file"), b"x").unwrap();
    let removed = area.cleanup(SystemTime::now() + Duration::from_secs(10 * 24 * 3600), MAX_AGE);
    assert_eq!(removed, 1);
    assert!(!staged.dir.exists());
    assert!(area.root().join("not-a-share").is_dir());
    assert!(area.root().join("loose-file").is_file());
}

#[test]
fn a_loop_that_shares_all_day_keeps_only_the_newest_shares() {
    let dir = TestDir::new();
    let mut last = None;
    for i in 0..(MAX_SHARES + 8) {
        last = Some(stage_files(&dir, &[("a.txt", format!("{i}").as_bytes())]));
    }
    let area = dir.area();
    let count = fs::read_dir(area.root()).unwrap().count();
    assert!(count <= MAX_SHARES, "{count} shares kept");
    // the newest survives
    assert!(last.unwrap().dir.is_dir());
}

#[test]
fn staging_works_when_the_area_does_not_exist_yet() {
    let dir = TestDir::new();
    let area = dir.area();
    assert!(!area.root().exists());
    let _ = stage_files(&dir, &[("a.txt", b"1")]);
    assert!(area.root().is_dir());
}

#[test]
fn a_name_is_one_uri_segment_whatever_it_holds_and_decodes_back() {
    for name in [
        "plain.pdf",
        "with space.pdf",
        "a/b.txt",
        "100% done?.md",
        "umlaut \u{e4}\u{f6}\u{fc}.txt",
        "emoji \u{1f600}.png",
        "a#b&c=d;e+f.txt",
        "..",
        "\u{202e}rtl.exe",
    ] {
        let encoded = percent_encode(name);
        assert!(!encoded.contains('/') && !encoded.contains('?') && !encoded.contains('#'), "{encoded}");
        assert!(encoded.bytes().all(|b| b.is_ascii_alphanumeric() || b"-._~%".contains(&b)), "{encoded}");
        assert_eq!(percent_decode(&encoded).as_deref(), Some(name), "{name:?} via {encoded}");
    }
    assert_eq!(percent_encode("a b"), "a%20b");
    assert_eq!(percent_encode("\u{e4}"), "%C3%A4");
}

#[test]
fn a_broken_escape_does_not_decode() {
    for bad in ["%", "%2", "%zz", "a%2", "%C3%28", "%FF"] {
        assert_eq!(percent_decode(bad), None, "{bad}");
    }
    assert_eq!(percent_decode("plain").as_deref(), Some("plain"));
}

#[test]
fn the_uri_of_a_staged_file_resolves_back_to_it() {
    let dir = TestDir::new();
    let staged = stage_files(&dir, &[("Plan v2 (final).md", b"hello")]);
    let uri = content_uri("com.example.app.azul.share", &staged.token, &staged.files[0].name);
    assert!(uri.starts_with("content://com.example.app.azul.share/"));
    let path = uri.strip_prefix("content://com.example.app.azul.share").unwrap();
    // what `Uri.getPath()` hands the provider: the path, decoded
    let decoded = format!("/{}", percent_decode(path.trim_start_matches('/')).unwrap());
    // the decoding turns the encoded segment into the name: the token and the name stay two segments
    let area = dir.area();
    assert_eq!(area.describe(&decoded).unwrap().name, "Plan v2 (final).md");
    assert_eq!(fs::read(area.resolve(&decoded).unwrap()).unwrap(), b"hello");
}
