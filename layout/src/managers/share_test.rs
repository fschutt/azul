//! The share request's check (`plan`), the names it makes safe, and the slot the platform's backend lives in.

use std::{fs, path::PathBuf};

use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    share::{ShareItem, ShareRequest, ShareStatus},
};
use azul_css::AzString;

use super::*;
use crate::managers::share_area::new_token;

/// A folder of its own under the temp directory, removed when the test ends.
struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("azul-share-test-{}", new_token()));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn file(&self, name: &str, bytes: &[u8]) -> AzString {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap();
        AzString::from(path.to_string_lossy().into_owned())
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}

fn s(text: &str) -> AzString {
    AzString::from(text)
}

fn refused(status: &ShareStatus) -> &str {
    match status {
        ShareStatus::Refused(reason) => reason.as_str(),
        other => panic!("expected Refused, got {other:?}"),
    }
}

// ---- names ----

#[test]
fn a_name_loses_its_path_and_its_dangerous_characters() {
    assert_eq!(sanitize_display_name("report.pdf", 128), "report.pdf");
    assert_eq!(sanitize_display_name("/data/user/0/app/files/report.pdf", 128), "report.pdf");
    assert_eq!(sanitize_display_name("C:\\Users\\me\\report.pdf", 128), "report.pdf");
    assert_eq!(sanitize_display_name("a<b>c:d*e?f\"g|h.txt", 128), "a_b_c_d_e_f_g_h.txt");
    assert_eq!(sanitize_display_name("tab\there\nline\0nul.txt", 128), "tab_here_line_nul.txt");
    assert_eq!(sanitize_display_name("  spaced name .pdf ", 128), "spaced name .pdf");
}

#[test]
fn a_right_to_left_override_cannot_make_an_exe_look_like_a_picture() {
    let name = sanitize_display_name("holiday\u{202e}gpj.exe", 128);
    assert!(!name.contains('\u{202e}'), "{name:?}");
    assert!(name.ends_with(".exe"), "the real extension is the last one: {name:?}");
    for c in ['\u{200b}', '\u{200e}', '\u{2066}', '\u{feff}'] {
        assert!(!sanitize_display_name(&format!("a{c}b.txt"), 128).contains(c));
    }
}

#[test]
fn a_hidden_or_empty_name_becomes_a_visible_one() {
    assert_eq!(sanitize_display_name(".profile", 128), "profile");
    assert_eq!(sanitize_display_name("..", 128), "file");
    assert_eq!(sanitize_display_name("...", 128), "file");
    assert_eq!(sanitize_display_name("", 128), "file");
    assert_eq!(sanitize_display_name("   ", 128), "file");
    assert_eq!(sanitize_display_name("/", 128), "file");
    assert_eq!(sanitize_display_name("???", 128), "file");
    assert_eq!(sanitize_display_name("name.", 128), "name");
}

#[test]
fn a_long_name_is_cut_at_a_character_and_keeps_its_extension() {
    let long = format!("{}.pdf", "x".repeat(400));
    let cut = sanitize_display_name(&long, 128);
    assert_eq!(cut.len(), 128);
    assert!(cut.ends_with(".pdf"));
    // 4-byte characters: never cut in the middle of one
    let emoji = format!("{}.md", "\u{1f600}".repeat(100));
    let cut = sanitize_display_name(&emoji, 50);
    assert!(cut.len() <= 50, "{} bytes", cut.len());
    assert!(cut.ends_with(".md"));
    assert!(cut.is_char_boundary(cut.len()));
    // a name that is all extension still ends up short
    let all = sanitize_display_name(&format!("a.{}", "e".repeat(300)), 64);
    assert!(all.len() <= 64, "{} bytes", all.len());
}

#[test]
fn equal_names_are_numbered_in_order_and_case_is_ignored() {
    let names = |list: &[&str]| unique_names(list.iter().map(|n| (*n).to_string()).collect());
    assert_eq!(names(&["a.pdf", "a.pdf", "a.pdf"]), ["a.pdf", "a (2).pdf", "a (3).pdf"]);
    assert_eq!(names(&["A.pdf", "a.PDF"]), ["A.pdf", "a (2).PDF"]);
    assert_eq!(names(&["notes", "notes"]), ["notes", "notes (2)"]);
    assert_eq!(names(&["a.pdf", "a.pdf", "a (2).pdf"]), ["a.pdf", "a (2).pdf", "a (2) (2).pdf"]);
    assert_eq!(names(&["x.txt", "y.txt"]), ["x.txt", "y.txt"]);
    assert!(names(&[]).is_empty());
}

// ---- the check of a request ----

fn limits() -> ShareLimits {
    ShareLimits::default()
}

#[test]
fn a_request_without_items_is_refused_and_says_so() {
    let status = plan(&ShareRequest::new(), &limits()).unwrap_err();
    assert!(refused(&status).contains("no item"), "{status:?}");
}

#[test]
fn too_many_items_are_refused_with_both_numbers() {
    let mut request = ShareRequest::new();
    for i in 0..(ShareRequest::MAX_ITEMS + 1) {
        request = request.with_item(ShareItem::text(s(&format!("t{i}"))));
    }
    let status = plan(&request, &limits()).unwrap_err();
    let reason = refused(&status);
    assert!(reason.contains("65") && reason.contains("64"), "{reason}");
}

#[test]
fn the_most_items_are_accepted() {
    let mut request = ShareRequest::new();
    for i in 0..ShareRequest::MAX_ITEMS {
        request = request.with_item(ShareItem::text(s(&format!("t{i}"))));
    }
    assert_eq!(plan(&request, &limits()).unwrap().items.len(), ShareRequest::MAX_ITEMS);
}

#[test]
fn a_text_is_checked_for_emptiness_and_length() {
    let status = plan(&ShareRequest::text(s("   \n")), &limits()).unwrap_err();
    assert!(refused(&status).contains("empty"));
    let big = "x".repeat(limits().max_text_bytes + 1);
    let status = plan(&ShareRequest::text(s(&big)), &limits()).unwrap_err();
    assert!(refused(&status).contains("too long"));
    assert!(plan(&ShareRequest::text(s("ok")), &limits()).is_ok());
}

#[test]
fn a_link_needs_a_scheme() {
    for good in ["https://example.org/a?b=c", "geo:48.52,8.67", "mailto:someone@example.org", "az-app://open/1"] {
        assert!(plan(&ShareRequest::url(s(good)), &limits()).is_ok(), "{good}");
    }
    for bad in ["example.org/path", "", "://x", "1http://x", "http:", "just words"] {
        let status = plan(&ShareRequest::url(s(bad)), &limits()).unwrap_err();
        assert!(matches!(status, ShareStatus::Refused(_)), "{bad:?}: {status:?}");
    }
}

#[test]
fn a_title_over_its_limit_is_refused() {
    let request = ShareRequest::text(s("x")).with_title(s(&"t".repeat(limits().max_title_bytes + 1)));
    assert!(refused(&plan(&request, &limits()).unwrap_err()).contains("title"));
}

#[test]
fn open_with_needs_exactly_one_file() {
    let dir = TestDir::new();
    let a = dir.file("a.pdf", b"a");
    let b = dir.file("b.pdf", b"b");
    let two = ShareRequest::file(a.clone()).with_item(ShareItem::file(b)).with_open_with(true);
    assert!(refused(&plan(&two, &limits()).unwrap_err()).contains("exactly one file"));
    let with_text = ShareRequest::file(a.clone()).with_item(ShareItem::text(s("x"))).with_open_with(true);
    assert!(matches!(plan(&with_text, &limits()), Err(ShareStatus::Refused(_))));
    let text_only = ShareRequest::text(s("x")).with_open_with(true);
    assert!(matches!(plan(&text_only, &limits()), Err(ShareStatus::Refused(_))));
    let one = ShareRequest::file(a).with_open_with(true);
    let planned = plan(&one, &limits()).unwrap();
    assert!(planned.open_with);
    assert_eq!(planned.files().count(), 1);
}

#[test]
fn a_file_that_cannot_be_read_is_a_failure_with_its_name_and_the_reason() {
    let dir = TestDir::new();
    let missing = format!("{}/gone.pdf", dir.0.display());
    let status = plan(&ShareRequest::file(s(&missing)), &limits()).unwrap_err();
    match status {
        ShareStatus::Failed(reason) => {
            assert!(reason.as_str().contains("gone.pdf"), "{}", reason.as_str());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_folder_cannot_be_shared() {
    let dir = TestDir::new();
    let status = plan(&ShareRequest::file(s(&dir.0.to_string_lossy())), &limits()).unwrap_err();
    assert!(refused(&status).contains("folder"), "{status:?}");
}

#[test]
fn a_file_over_the_limit_is_refused_with_its_size_and_the_limit() {
    let dir = TestDir::new();
    let path = dir.file("big.bin", &vec![0_u8; 3 * 1024 * 1024]);
    let tight = ShareLimits { max_file_bytes: 2 * 1024 * 1024, ..limits() };
    let reason = refused(&plan(&ShareRequest::file(path), &tight).unwrap_err()).to_string();
    assert!(reason.contains("big.bin") && reason.contains("3 MiB") && reason.contains("2 MiB"), "{reason}");
}

#[test]
fn files_over_the_total_limit_together_are_refused() {
    let dir = TestDir::new();
    let one = dir.file("one.bin", &vec![0_u8; 600 * 1024]);
    let two = dir.file("two.bin", &vec![0_u8; 600 * 1024]);
    let tight = ShareLimits { max_total_bytes: 1024 * 1024, ..limits() };
    let request = ShareRequest::file(one).with_item(ShareItem::file(two));
    assert!(refused(&plan(&request, &tight).unwrap_err()).contains("together"));
}

#[test]
fn the_type_comes_from_the_app_else_from_the_name_else_it_is_a_stream() {
    let dir = TestDir::new();
    let md = dir.file("notes.md", b"# x");
    let unknown = dir.file("blob.zzz", b"x");
    let planned = plan(
        &ShareRequest::file(md.clone())
            .with_item(ShareItem::file(md).with_mime(s("text/x-special; charset=utf-8")).with_name(s("renamed.md")))
            .with_item(ShareItem::file(unknown.clone()))
            .with_item(ShareItem::file(unknown).with_mime(s("not a type")).with_name(s("again.zzz"))),
        &limits(),
    )
    .unwrap();
    let types: Vec<(&str, &str)> = planned.files().map(|(_, name, mime, _)| (name, mime)).collect();
    assert_eq!(
        types,
        [
            ("notes.md", "text/markdown"),
            ("renamed.md", "text/x-special"),
            ("blob.zzz", mime::OCTET_STREAM),
            ("again.zzz", mime::OCTET_STREAM),
        ]
    );
}

#[test]
fn the_name_of_a_file_is_the_apps_or_the_files_and_names_are_unique() {
    let dir = TestDir::new();
    let a = dir.file("a.pdf", b"1");
    let planned = plan(
        &ShareRequest::file(a.clone())
            .with_item(ShareItem::file(a.clone()))
            .with_item(ShareItem::file(a).with_name(s("../../etc/passwd"))),
        &limits(),
    )
    .unwrap();
    let names: Vec<&str> = planned.files().map(|(_, name, _, _)| name).collect();
    assert_eq!(names, ["a.pdf", "a (2).pdf", "passwd"]);
}

#[test]
fn the_plan_knows_its_text_its_total_and_the_type_of_the_whole_share() {
    let dir = TestDir::new();
    let png = dir.file("a.png", &[0; 10]);
    let jpg = dir.file("b.jpg", &[0; 20]);
    let planned = plan(
        &ShareRequest::file(png).with_item(ShareItem::file(jpg)).with_item(ShareItem::text(s("look"))).with_item(ShareItem::url(s("https://x.y/"))),
        &limits(),
    )
    .unwrap();
    assert_eq!(planned.total_bytes, 30);
    assert_eq!(planned.texts(), ["look", "https://x.y/"]);
    assert_eq!(planned.mime(), "image/*");
    let only_text = plan(&ShareRequest::text(s("hi")), &limits()).unwrap();
    assert_eq!(only_text.mime(), "text/plain");
}

#[test]
fn the_anchor_and_the_title_reach_the_plan() {
    let rect = LogicalRect::new(LogicalPosition::new(10.0, 20.0), LogicalSize::new(30.0, 40.0));
    let request = ShareRequest::text(s("x")).with_title(s("Send it")).with_anchor(rect);
    let planned = plan(&request, &limits()).unwrap();
    assert_eq!(planned.title, "Send it");
    assert_eq!(planned.anchor, Some(rect));
}

// ---- the slot ----

fn shown_backend() -> ShareBackend {
    fn share(plan: &SharePlan) -> ShareStatus {
        if plan.items.len() == 1 {
            ShareStatus::Shown
        } else {
            ShareStatus::Failed(s("no app can receive this"))
        }
    }
    fn unavailable() -> Option<String> {
        None
    }
    ShareBackend { share, unavailable }
}

fn desktop_backend() -> ShareBackend {
    fn share(_: &SharePlan) -> ShareStatus {
        ShareStatus::Shown
    }
    fn unavailable() -> Option<String> {
        Some("this desktop has no system share sheet".to_string())
    }
    ShareBackend { share, unavailable }
}

#[test]
fn a_runtime_without_a_backend_says_it_cannot_share_and_why() {
    let channel = ShareChannel::new();
    assert!(!channel.can_share());
    assert!(channel.unavailable_reason().unwrap().contains("no share sheet"));
    match channel.share(&ShareRequest::text(s("x"))) {
        ShareStatus::Unsupported(reason) => assert!(reason.as_str().contains("no share sheet")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_desktop_backend_answers_unsupported_with_its_reason() {
    let channel = ShareChannel::new();
    channel.install(Some(desktop_backend()));
    assert!(!channel.can_share());
    match channel.share(&ShareRequest::text(s("x"))) {
        ShareStatus::Unsupported(reason) => assert_eq!(reason.as_str(), "this desktop has no system share sheet"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_phone_backend_is_asked_with_the_checked_plan_and_its_answer_is_the_answer() {
    let channel = ShareChannel::new();
    channel.install(Some(shown_backend()));
    assert!(channel.can_share());
    assert_eq!(channel.share(&ShareRequest::text(s("x"))), ShareStatus::Shown);
    let two = ShareRequest::text(s("x")).with_item(ShareItem::url(s("https://a.b/")));
    assert_eq!(channel.share(&two), ShareStatus::Failed(s("no app can receive this")));
    // a request that is not one never reaches the backend
    assert!(matches!(channel.share(&ShareRequest::new()), ShareStatus::Refused(_)));
}

#[test]
fn the_recorder_keeps_the_checked_requests_and_shows_nothing() {
    let channel = ShareChannel::new();
    channel.install(Some(desktop_backend()));
    channel.record_instead();
    assert!(channel.can_share() && channel.is_recording());
    let dir = TestDir::new();
    let file = dir.file("r.pdf", b"x");
    let request = ShareRequest::file(file).with_title(s("Report"));
    assert_eq!(channel.share(&request), ShareStatus::Shown);
    // a bad request is refused while recording too: a scenario sees what the phone would see
    assert!(matches!(channel.share(&ShareRequest::new()), ShareStatus::Refused(_)));
    let recorded = channel.recorded();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].plan.title, "Report");
    assert_eq!(recorded[0].plan.files().next().unwrap().1, "r.pdf");
}

#[test]
fn clones_of_the_slot_are_one_slot() {
    let channel = ShareChannel::new();
    let other = channel.clone();
    other.record_instead();
    assert!(channel.is_recording());
    assert_eq!(channel.share(&ShareRequest::text(s("x"))), ShareStatus::Shown);
    assert_eq!(other.recorded().len(), 1);
}

#[test]
fn the_limits_of_an_app_can_be_changed() {
    let channel = ShareChannel::new();
    channel.record_instead();
    channel.set_limits(ShareLimits { max_items: 1, ..limits() });
    let two = ShareRequest::text(s("a")).with_item(ShareItem::text(s("b")));
    assert!(matches!(channel.share(&two), ShareStatus::Refused(_)));
    assert_eq!(channel.share(&ShareRequest::text(s("a"))), ShareStatus::Shown);
}

#[test]
fn the_recorder_keeps_the_newest_shares_only() {
    let channel = ShareChannel::new();
    channel.record_instead();
    for i in 0..(MAX_RECORDED + 5) {
        assert_eq!(channel.share(&ShareRequest::text(s(&format!("t{i}")))), ShareStatus::Shown);
    }
    let recorded = channel.recorded();
    assert_eq!(recorded.len(), MAX_RECORDED);
    assert_eq!(recorded[0].plan.texts(), ["t5"]);
    assert_eq!(recorded[MAX_RECORDED - 1].plan.texts(), [format!("t{}", MAX_RECORDED + 4).as_str()]);
}
