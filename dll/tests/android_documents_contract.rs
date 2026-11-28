//! What the Android `Documents` backend says, pinned in source (a phone cannot be driven where the tests run; the emulator proof is
//! `scripts/ci/android_files_proof.py`). Ledger WP-F, step 3.
//!
//! * the backend is installed into the App's services at the start, on Android only; no global state holds it;
//! * every call into another app's provider runs on a worker with a deadline (a provider that never answers must not freeze the
//!   caller): the only direct JNI entry besides that is the page of "All files access", which asks no provider;
//! * the framework's constants are READ from `DocumentsContract.Document` at run time, never copied (a flag bit written down from
//!   memory is a bug that only a phone finds), and the decisions about them (rows, flags, exceptions, locators) are the pure
//!   functions of `azul_layout::documents_saf`, not text matching in the JNI file;
//! * a write truncates and ends with the descriptor's own `close()` (a provider that checks the data at close reports from there); a
//!   read detaches the descriptor into a `File`; a name is checked before a create (a provider renames instead of failing); a
//!   document is asked for before it is renamed or deleted (a provider answers `false` / `null` for a missing one and a refused one
//!   alike).
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_documents_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Source without its `//` comments (a comment may mention what the code must not do). A `//` inside a string literal ends the
/// line early: the tests below never look for text after one.
fn code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

const BACKEND: &str = "dll/src/desktop/extra/documents/android.rs";
const INSTALL: &str = "dll/src/desktop/extra/documents/mod.rs";

/// The body of the first `fn name(` of `src`, up to the next top-level `\nfn ` / `\n}` line.
fn function<'a>(src: &'a str, name: &str) -> &'a str {
    let at = src
        .find(&format!("\nfn {name}("))
        .or_else(|| src.find(&format!("\nfn {name}<")))
        .unwrap_or_else(|| panic!("no fn {name}"));
    let rest = &src[at + 1..];
    let end = rest.find("\n}\n").map_or(rest.len(), |e| e + 3);
    &rest[..end]
}

fn position(haystack: &str, needle: &str) -> usize {
    haystack.find(needle).unwrap_or_else(|| panic!("`{needle}` is not in:\n{haystack}"))
}

/// The backend goes into the services at the start, and only where there is a JNI bridge to run it on.
///
/// Mutation: the install call removed from `App::start_app_services` (the state of 96d83ae2e0: a phone listed no folder the
/// person granted).
#[test]
fn the_backend_is_installed_at_the_start_on_android_with_jni_only() {
    let app = code("dll/src/desktop/app.rs");
    assert!(
        app.contains("crate::desktop::extra::documents::install(self.ptr.undo_manager.services())"),
        "App::start_app_services installs the backend into the App's own services"
    );
    let install = code(INSTALL);
    assert!(install.contains("services.documents.install("), "the install goes through the slot of the App's services");
    assert!(install.contains("android::AndroidDocuments::new()"));
    assert_eq!(
        install.matches("cfg(all(target_os = \"android\", feature = \"jni\"))").count(),
        2,
        "the module and the install call are Android + JNI only"
    );
    let extra = code("dll/src/desktop/extra/mod.rs");
    assert!(extra.contains("pub mod documents;"));
}

/// No process-wide state: the count of late calls is a field of the guard, the guard a field of the backend.
///
/// Mutation: a `static` registry of grants or a `thread_local!` of the JNI environment in the backend.
#[test]
fn the_backend_holds_no_global_state() {
    for file in [BACKEND, INSTALL] {
        let src = code(file);
        for forbidden in ["static ", "thread_local!", "OnceLock", "OnceCell", "lazy_static", "Mutex<", "AtomicU", "AtomicI", "AtomicBool"] {
            // `get_static_field` / `call_static_method` / `static_string` / `static_int` are framework calls, not state
            let uses = src
                .lines()
                .filter(|line| line.contains(forbidden))
                .filter(|line| !line.contains("get_static_field") && !line.contains("call_static_method") && !line.contains("static_string") && !line.contains("static_int"))
                .count();
            assert_eq!(uses, 0, "{file} must not hold `{forbidden}`");
        }
    }
}

/// Every call into a provider goes through the guard; the one other entry into JNI is the settings page.
///
/// Mutation: `with_env(` called straight from a backend method (a provider that never answers would freeze the app's thread).
#[test]
fn every_call_into_a_provider_runs_on_a_worker_under_the_deadline() {
    let src = code(BACKEND);
    assert_eq!(
        src.matches("with_env(").count(),
        2,
        "`with_env(` appears twice: in `java_within` (under the guard) and for the page of All files access (no provider is asked)"
    );
    // the method lives inside `impl AndroidDocuments`, indented: find it by its signature
    let at = position(&src, "fn java_within<");
    let body = &src[at..src[at..].find("\n    }\n").map_or(src.len(), |e| at + e)];
    assert!(body.contains("with_env("));
    assert!(body.contains("self.guard.run_within(") && body.contains("self.guard.run("), "both deadlines go through the guard");
    let backend = &src[position(&src, "impl DocumentBackend for AndroidDocuments")..];
    // every method that touches the framework asks `self.java`; the two without a provider are the page of All files access
    // (direct) and the name of the backend
    let methods = ["fn forget(", "fn stat(", "fn children(", "fn read(", "fn open_read(", "fn write(", "fn create_file(", "fn create_dir(", "fn rename(", "fn delete("];
    for method in methods {
        let start = position(backend, method);
        let end = backend[start + 1..].find("\n    fn ").map_or(backend.len(), |e| start + 1 + e);
        let body = &backend[start..end];
        assert!(body.contains("self.java("), "{method} runs under the guard: {body}");
        assert!(body.contains("self.local."), "{method} hands a locator that is a path to the desktop's backend");
    }
    assert!(!src.contains("thread::spawn") && !src.contains("std::thread"), "the guard owns the workers; the backend starts none");
}

/// The constants of `DocumentsContract.Document` are read at run time; the decisions are the pure functions of the layout crate.
///
/// Mutation: `const FLAG_SUPPORTS_RENAME: i32 = 1 << 6;` written down in the backend.
#[test]
fn the_frameworks_constants_are_read_and_the_decisions_are_the_pure_functions() {
    let src = code(BACKEND);
    for field in [
        "COLUMN_DOCUMENT_ID",
        "COLUMN_DISPLAY_NAME",
        "COLUMN_MIME_TYPE",
        "COLUMN_SIZE",
        "COLUMN_LAST_MODIFIED",
        "COLUMN_FLAGS",
        "MIME_TYPE_DIR",
        "FLAG_SUPPORTS_WRITE",
        "FLAG_SUPPORTS_DELETE",
        "FLAG_DIR_SUPPORTS_CREATE",
        "FLAG_SUPPORTS_RENAME",
    ] {
        assert!(src.contains(&format!("\"{field}\"")), "{field} is read from the framework");
    }
    for copied in ["const FLAG_", "const COLUMN_", "vnd.android.document/directory", "\"_display_name\"", "\"document_id\"", "\"mime_type\""] {
        assert!(!src.contains(copied), "the backend must not copy `{copied}` from the framework");
    }
    for pure in ["classify_failure(", "entry_from_row(", "Locator::parse(", "read_range(", "cap_listing(", "sort_listing("] {
        assert!(src.contains(pure), "the backend uses the pure function {pure}");
    }
    // the exception texts are decided in `documents_saf`, not here
    for text in ["SecurityException", "FileNotFoundException", "IllegalStateException", "EACCES", "ENOENT"] {
        assert!(!src.contains(text), "the backend must not match on `{text}`: that is `classify_failure`");
    }
}

/// A write truncates and closes the descriptor through Java; a read owns the detached descriptor.
///
/// Mutation: mode `"w"` (it does not truncate from Android 10), or the descriptor detached and closed raw after a write (an error
/// of a reliable pipe is lost).
#[test]
fn a_write_truncates_and_ends_with_the_descriptors_own_close_and_a_read_owns_its_descriptor() {
    let src = code(BACKEND);
    let write = function(&src, "write");
    assert!(write.contains("\"wt\""), "write and truncate");
    let opened = position(write, "open_descriptor(");
    let fd = position(write, "\"getFd\"");
    let written = position(write, "write_descriptor(fd, bytes)");
    let closed = position(write, "\"close\"");
    assert!(opened < fd && fd < written && written < closed, "open, getFd, write, close - in this order");
    assert!(!write.contains("detachFd"), "a write keeps the descriptor with its ParcelFileDescriptor");
    assert!(write.contains("written.map_err") && write.contains("closed?"), "the failure of the write AND of the close are both reported");
    let open = function(&src, "open_file");
    assert!(open.contains("\"r\"") && open.contains("\"detachFd\"") && open.contains("File::from_raw_fd(fd)"));
    assert!(src.contains("ManuallyDrop::new(unsafe { File::from_raw_fd(fd) })"), "a write must not close the descriptor itself");
}

/// A provider renames instead of failing: names are checked before a create; a missing document is asked for before a rename or
/// a delete (their `false` / `null` cannot tell a missing document from a refusal).
///
/// Mutation: `createDocument` called without `name_is_taken` (a second "report.pdf" would come back as "report (1).pdf" and the app
/// would think it saved the first).
#[test]
fn a_name_is_checked_before_a_create_and_a_document_is_asked_for_before_a_rename_or_a_delete() {
    let src = code(BACKEND);
    let create = function(&src, "create");
    assert!(position(create, "name_is_taken(") < position(create, "\"createDocument\""));
    assert!(create.contains("DocumentError::Exists"));
    for name in ["rename", "delete"] {
        let body = function(&src, name);
        let verb = if name == "rename" { "\"renameDocument\"" } else { "\"deleteDocument\"" };
        assert!(position(body, "stat(env, activity, locator, what)") < position(body, verb), "{name} asks for the document first");
    }
    let rename = function(&src, "rename");
    assert!(rename.contains("can_rename"), "a provider that cannot rename says Unsupported before anything is tried");
}

/// The persisted grants of the system are the places; a grant is released through the system, not forgotten in a file of ours.
///
/// Mutation: a `places.json` of the app's own.
#[test]
fn the_persisted_grants_are_the_places_and_forgetting_releases_the_system_grant() {
    let src = code(BACKEND);
    assert!(function(&src, "raw_places").contains("\"getPersistedUriPermissions\""));
    assert!(function(&src, "forget").contains("\"releasePersistableUriPermission\""));
    for own_file in ["places.json", "write_all(", "fs::write"] {
        let hits = src.matches(own_file).count();
        // the one `write_all(` is the write of a document through its descriptor
        assert!(own_file != "write_all(" || hits == 1, "{own_file}");
        assert!(own_file == "write_all(" || hits == 0, "the grants are the system's, not a file of the app's: {own_file}");
    }
    // a place's name is asked of the provider with a short deadline of its own, so that one hung provider does not hold the list back
    assert!(src.contains("PLACE_NAME_DEADLINE") && src.contains("java_within(Some(PLACE_NAME_DEADLINE)"));
}

/// "All files access" is `MANAGE_EXTERNAL_STORAGE`, available only when the app declares it, and asked for through the system's
/// own page, from the caller's gesture.
#[test]
fn all_files_access_is_only_there_when_the_app_declares_it_and_is_asked_through_the_systems_page() {
    let src = code(BACKEND);
    let access = function(&src, "all_files_access");
    assert!(access.contains("\"android.permission.MANAGE_EXTERNAL_STORAGE\"") && access.contains("AllFilesAccess::Unavailable"));
    assert!(access.contains("\"isExternalStorageManager\""));
    assert!(function(&src, "request_all_files_access").contains("\"android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION\""));
}
