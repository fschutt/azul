//! What the iOS `Documents` backend says, pinned in source (the Objective-C cannot run where the tests run). Ledger WP-F, step 3b.
//!
//! * a folder the person grants is kept as a BOOKMARK in the app's own small file (`PlacesStore`), made when the document picker
//!   answers a folder request, and resolved again (security-scoped, access started once) when the places are listed, so that it is
//!   there after a restart; a stale bookmark is made again, one that does not resolve is not listed and not deleted;
//! * every call on a path is the desktop's backend (the resolved folder's path is an ordinary path once its access is open);
//! * no process-wide state: the resolved list is a field of the backend.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/ios_documents_contract.rs -o /tmp/t && /tmp/t

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

const BACKEND: &str = "dll/src/desktop/extra/documents/apple.rs";
const PICKER: &str = "dll/src/desktop/extra/file_picker/ios.rs";

/// The bookmark is made when a folder is picked, and the folder's access is started before it is.
///
/// Mutation: the `remember_folder` call removed from the delegate (a granted folder gone at the next start).
#[test]
fn a_picked_folder_is_bookmarked_when_the_picker_answers() {
    let picker = code(PICKER);
    let started = picker.find("startAccessingSecurityScopedResource").expect("access is started");
    let remembered = picker.find("documents::apple::remember_folder(url)").expect("the folder is remembered");
    assert!(started < remembered, "access first, then the bookmark");
    assert!(picker.contains("PickKind::Folder") && picker.contains("if folder {"), "only a FOLDER pick is bookmarked, not a copy of a file");
    let backend = code(BACKEND);
    assert!(backend.contains("bookmarkDataWithOptions") && backend.contains("PlacesStore::new(file).add("));
}

/// The places resolve the bookmarks once, refresh a stale one and leave one that does not resolve alone.
///
/// Mutation: `startAccessingSecurityScopedResource` called for every listing (a leak of a sandbox extension per call), or a bookmark
/// that does not resolve removed from the store (a volume that is only not mounted yet would lose its folder).
#[test]
fn the_places_resolve_each_bookmark_once_and_never_delete_what_does_not_resolve() {
    let backend = code(BACKEND);
    assert!(backend.contains("URLByResolvingBookmarkData") && backend.contains("bookmarkDataIsStale"));
    let granted = &backend[backend.find("fn granted(&self)").expect("granted")..];
    let granted = &granted[..granted.find("\nimpl DocumentBackend").expect("the end of granted")];
    assert!(granted.contains("known.iter().find(|g| g.bookmark == kept.bookmark)"), "a bookmark already resolved is not resolved again:\n{granted}");
    let none = granted.find("let Some(resolved) =").expect("a bookmark that does not resolve");
    let skipped = &granted[none..none + 300];
    assert!(skipped.contains("continue"), "it is skipped:\n{skipped}");
    assert!(!skipped.contains("store.remove"), "and it stays in the store:\n{skipped}");
    assert!(granted.contains("resolved.refreshed"), "a stale bookmark is made again");
}

/// The backend is installed on iOS, holds no global state, and calls every path through the desktop's backend.
#[test]
fn the_backend_is_installed_on_ios_with_no_global_state_and_paths_go_to_the_local_backend() {
    let install = code("dll/src/desktop/extra/documents/mod.rs");
    assert!(install.contains("apple::IosDocuments::new()") && install.contains("target_os = \"ios\""));
    let backend = code(BACKEND);
    for forbidden in ["static ", "thread_local!", "OnceLock", "lazy_static", "AtomicU"] {
        assert!(!backend.contains(forbidden), "{BACKEND} must not hold `{forbidden}`");
    }
    for call in ["self.local.stat(", "self.local.children(", "self.local.read(", "self.local.write(", "self.local.create_file(", "self.local.create_dir(", "self.local.rename(", "self.local.delete(", "self.local.open_read("] {
        assert!(backend.contains(call), "{call}");
    }
    assert!(backend.contains("AllFilesAccess::Unavailable"), "iOS has no all-files grant");
}
