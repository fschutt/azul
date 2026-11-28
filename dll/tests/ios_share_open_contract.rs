//! What the iOS share and open path says, pinned in source (the Simulator proof is `scripts/ci/ios_files_proof.sh`). Ledger WP-F.
//!
//! * the app delegate takes `application:openURL:options:` and hands it to `extra::open_files::apple`, and tells it when the app
//!   became active (a request before that is the one that STARTED the app);
//! * the share sheet is presented anchored (an iPad's popover without a source terminates the app), from the main thread only;
//! * the build writes the document types, the types the app defines, in-place opening and the URL schemes ONLY from the app's
//!   declaration (`AZ_DOCUMENTS`), never from the template;
//! * no `.m` / `.mm` / `.swift` file is added for any of it (the rule of the glue: objc2 only).
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/ios_share_open_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Mutation: the delegate method removed (UIKit drops the file or the link another app sends).
#[test]
fn the_delegate_takes_open_url_and_notes_when_the_app_is_active() {
    let shell = code("dll/src/desktop/shell2/ios/mod.rs");
    assert!(shell.contains("sel!(application:openURL:options:)"), "the delegate declares the selector");
    assert!(shell.contains("extra::open_files::apple::did_open_url("));
    let active = &shell[shell.find("extern \"C\" fn app_did_become_active").expect("applicationDidBecomeActive")..];
    let active = &active[..active.find("\n}\n").unwrap()];
    assert!(active.contains("extra::open_files::apple::note_active()"), "a later request is not the one that started the app");
}

#[test]
fn a_file_is_copied_by_a_worker_within_its_security_scope_and_the_scope_ends() {
    let apple = code("dll/src/desktop/extra/open_files/apple.rs");
    assert!(apple.contains("std::thread::Builder::new()"), "the copy is a worker's, not the main thread's");
    assert!(apple.contains("startAccessingSecurityScopedResource") && apple.contains("stopAccessingSecurityScopedResource"));
    assert!(apple.contains("UIApplicationOpenURLOptionsOpenInPlaceKey"));
    assert!(apple.contains("copy_in(") && apple.contains("file_failed("), "every file is an item, a failure with its reason");
}

/// Mutation: the popover source removed (an iPad raises an exception when the sheet has none).
#[test]
fn the_share_sheet_is_anchored_for_a_tablet_and_shown_from_the_main_thread() {
    let apple = code("dll/src/desktop/extra/share/apple.rs");
    assert!(apple.contains("popoverPresentationController") && apple.contains("setSourceView") && apple.contains("setSourceRect"));
    assert!(apple.contains("isMainThread"), "UIKit is main-thread only");
    assert!(apple.contains("UIActivityViewController") && apple.contains("UIDocumentInteractionController"));
    assert!(!apple.contains("file://"), "file URLs are made by NSURL, not by hand");
}

/// Mutation: a static `CFBundleDocumentTypes` written into the template (every app would claim every type).
#[test]
fn the_plist_gets_document_types_only_from_the_declaration() {
    let template = read("scripts/ios/Info.plist");
    for key in ["CFBundleDocumentTypes", "UTImportedTypeDeclarations", "CFBundleURLTypes", "LSSupportsOpeningDocumentsInPlace", "UIFileSharingEnabled"] {
        assert!(!template.contains(key), "{key} is generated from the app's [[app.documents]], not written in the template");
    }
    let script = read("scripts/build-ios.sh");
    assert!(script.contains("import documents_manifest") && script.contains("documents_manifest.ios_plist("));
    assert!(script.contains("AZ_DOCUMENTS") && script.contains("AZ_IOS_FILE_SHARING"));
    assert!(script.contains("documents_manifest.validate("), "a bad declaration stops the build with its reason");
}

#[test]
fn no_objective_c_or_swift_file_was_added_for_the_glue() {
    fn walk(dir: &Path, found: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else if matches!(path.extension().and_then(|e| e.to_str()), Some("m" | "mm" | "swift")) {
                found.push(path.display().to_string());
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    walk(&root.join("dll/src"), &mut found);
    walk(&root.join("scripts/ios"), &mut found);
    assert!(found.is_empty(), "the glue is Rust (objc2): {found:?}");
}
