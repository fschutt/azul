//! Every place that attaches a thread to the JVM and then calls Java releases the local references it
//! made.
//!
//! A `JObject`, `JString` or `JClass` that a JNI call hands back is a LOCAL reference. It lives until the
//! thread returns to Java or detaches, and `android_main` (the thread every azul helper calls Java from)
//! is attached for the whole run and never returns to Java: the JNI tips say "if you attach a native
//! thread with AttachCurrentThread, the code you are running will never automatically free local
//! references until the thread detaches". `find_app_class` alone made three per call, a haptic tap five,
//! a paste seven, none released: on Android 7 the table is capped (512) and the VM aborts the process,
//! and everywhere the process grows with every tap.
//!
//! `LocalFrame::push` (dll/src/desktop/extra/mod.rs) pushes a frame and pops it when it drops. This test
//! reads the source of every `.rs` file of `dll/src` and requires one within a few lines after every
//! `attach_current_thread()`. It needs nothing built:
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_jni_scopes_release_their_local_references.rs -o /tmp/t && /tmp/t

use std::{
    fs,
    path::{Path, PathBuf},
};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// How many lines after the attach the frame may be pushed (the statement that binds the environment
/// can span several lines).
const WITHIN: usize = 6;

#[test]
fn every_attach_to_the_jvm_pushes_a_local_frame() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../dll/src");
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    files.sort();
    let mut attaches = 0;
    let mut problems = Vec::new();
    for file in files {
        let src = fs::read_to_string(&file).unwrap();
        let lines: Vec<&str> = src.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            // the closure form (`attach_current_thread(|env| ...)`) of android-activity manages its own
            // frame; the helpers of azul use the guard form
            if !code.contains("attach_current_thread()") {
                continue;
            }
            attaches += 1;
            let end = (i + 1 + WITHIN).min(lines.len());
            let framed = lines[i..end]
                .iter()
                .any(|l| l.split("//").next().unwrap_or("").contains("LocalFrame::push"));
            if !framed {
                problems.push(format!(
                    "{}:{}: attaches the thread and never releases its local references (no LocalFrame::push within {WITHIN} lines)",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    i + 1
                ));
            }
        }
    }
    assert!(attaches >= 15, "the shell and its extras attach in a dozen places; found {attaches} (is the scan reading them?)");
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}
