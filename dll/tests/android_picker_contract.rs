//! What the Android file picker says, pinned in source (a phone cannot be driven where the tests run; the emulator proof is
//! `scripts/ci/android_files_proof.py`). Ledger WP-F, step 3a (rows FP-1 .. FP-4 of 2026-11-28).
//!
//! * the request on the screen is parked in the App's own registry (`AppServices::pickers`), not in a process-wide map and counter:
//!   two Apps in one process never share an id, and a late answer finds nothing;
//! * the intent is built and started from Rust; the Java class is a stateless hand-over of the activity result to ONE native, and
//!   both sides name the same request-code window;
//! * a request that could not be started is taken back out of the registry and answered with the reason (never left waiting for an
//!   answer that cannot come); a cancel is answered at once; the files are imported by the shared worker.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_picker_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Source without its `//` comments.
fn code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

const RUST: &str = "dll/src/desktop/extra/file_picker/android.rs";
const JAVA: &str = "scripts/android/AzulFilePicker.java";

/// The brace-balanced body after the first `signature`.
fn body<'a>(src: &'a str, signature: &str) -> &'a str {
    let start = src.find(signature).unwrap_or_else(|| panic!("no `{signature}`"));
    let open = start + src[start..].find('{').unwrap();
    let mut depth = 0;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..open + i + 1];
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces after `{signature}`");
}

/// No `static` map, counter or lock: the picker's state is the App's.
///
/// Mutation: `static PENDING_PICKERS: Mutex<BTreeMap<u64, FilePickerHandle>>` and `static REQUEST_ID_COUNTER: AtomicU64` back (the
/// state of 96d83ae2e0).
#[test]
fn the_pickers_on_the_screen_are_the_apps_own_and_not_a_process_wide_map() {
    let rust = code(RUST);
    for forbidden in ["static ", "thread_local!", "OnceLock", "Mutex<", "AtomicU", "AtomicBool", "lazy_static"] {
        let hits = rust.lines().filter(|l| l.contains(forbidden)).count();
        assert_eq!(hits, 0, "{RUST} must not hold `{forbidden}`");
    }
    let java = code(JAVA);
    for forbidden in ["static final ConcurrentHashMap", "AtomicInteger", "static int", "static long", "static final Map", "HashMap"] {
        assert!(!java.contains(forbidden), "{JAVA} must keep no state: `{forbidden}`");
    }
    assert!(rust.contains("services.pickers.register(") && rust.contains("services.pickers.take::<PendingPick>("));
}

/// The intent is Rust's; the Java side and the Rust side name the same request-code window.
///
/// Mutation: the base changed on one side only (the result of a picker would be taken for someone else's).
#[test]
fn the_intent_is_built_in_rust_and_both_sides_name_the_same_request_codes() {
    let rust = code(RUST);
    for needed in [
        "\"android.intent.action.OPEN_DOCUMENT\"",
        "\"android.intent.category.OPENABLE\"",
        "\"android.intent.extra.ALLOW_MULTIPLE\"",
        "\"android.intent.extra.MIME_TYPES\"",
        "startActivityForResult",
        "REQUEST_CODE_BASE + i32::try_from(id)",
    ] {
        assert!(rust.contains(needed), "the picker's intent is built in Rust: {needed}");
    }
    let java = code(JAVA);
    for builder in ["new Intent(", "startActivityForResult", "addCategory", "setType("] {
        assert!(!java.contains(builder), "Java builds no intent: {builder}");
    }
    let rust_base = rust
        .lines()
        .find(|l| l.contains("const REQUEST_CODE_BASE"))
        .and_then(|l| l.split('=').nth(1))
        .map(|v| v.trim().trim_end_matches(';').replace('_', "").to_lowercase())
        .expect("the Rust base");
    let java_base = java
        .lines()
        .find(|l| l.contains("REQUEST_CODE_BASE ="))
        .and_then(|l| l.split('=').nth(1))
        .map(|v| v.trim().trim_end_matches(';').replace('_', "").to_lowercase())
        .expect("the Java base");
    let number = |text: &str| i64::from_str_radix(text.trim_start_matches("0x"), 16).unwrap_or_else(|e| panic!("{text}: {e}"));
    assert_eq!(number(&rust_base), number(&java_base), "the request-code window is the same on both sides");
    assert!(java.contains("nativeOnActivityResult(activity, requestCode - REQUEST_CODE_BASE, resultCode, data)"));
    assert!(rust.contains("Java_com_azul_picker_AzulFilePicker_nativeOnActivityResult"));
}

/// A request that cannot be started is taken back out and answered with the reason; a cancel is answered at once.
///
/// Mutation: the `take` before `fail` removed (a request that waits for ever for an answer that cannot come).
#[test]
fn a_request_that_could_not_start_is_answered_and_a_cancel_is_answered_at_once() {
    let rust = code(RUST);
    let dispatch = body(&rust, "pub fn dispatch_open_file(\n    handle: FilePickerHandle,\n    _title: AzString,\n    _default_path: OptionString,\n    filter_descriptors");
    let taken = dispatch.find("services.pickers.take::<PendingPick>(id)").expect("the request is taken back");
    let failed = dispatch.rfind("super::fail(&handle, &failed)").expect("and answered with the reason");
    assert!(taken < failed, "taken back, then answered:\n{dispatch}");
    let native = body(&rust, "pub unsafe extern \"system\" fn Java_com_azul_picker_AzulFilePicker_nativeOnActivityResult(");
    assert!(native.contains("FilePickerStatus::Cancelled"), "a cancel is answered on the spot");
    assert!(native.contains("result_code != RESULT_OK || data.is_null()"));
    assert!(native.contains("classify_pick(&info)"), "the files are what the pure classifier says");
}

/// A picked FILE is copied (its grant is not kept: a folder is what a persisted grant is for); a folder's grant is kept.
#[test]
fn a_picked_file_is_copied_and_does_not_burn_a_persisted_grant() {
    let rust = code(RUST);
    let intent = body(&rust, "fn open_intent<'a>(");
    assert!(intent.contains("FLAG_GRANT_READ_URI_PERMISSION"));
    assert!(!intent.contains("PERSISTABLE"), "the system keeps at most 128 persisted grants (512 from Android 11): files are copied, folders are granted");
}
