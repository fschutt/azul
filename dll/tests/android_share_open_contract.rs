//! What the Android share path says, pinned in source (an APK cannot be installed where the tests run; the emulator proof is
//! `scripts/ci/android_files_proof.py`). Ledger WP-F, steps 1 and 2.
//!
//! * the manifest names ONE provider, the app's share provider: not exported, a grant per URI, no wide path map of the app's data
//!   (the classic leak of a FileProvider rooted at the data folder), the authority `<package>.azul.share` the Rust side builds;
//! * the provider is Java with no native method (it runs before the app's folders are prepared and in processes that never ran the
//!   engine), serves only `<token>/<name>` of the share area, read-only, behind a canonical-path check, and names the same token
//!   length, folder and types file as `azul_layout::managers::share_area`;
//! * the chooser is built from Rust: `Intent.createChooser`, a `ClipData` with the URIs, the read grant, `EXTRA_STREAM`; never a
//!   `file://` URI (`FileUriExposedException` from API 24 and a leak of a path).
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_share_open_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The XML without its comments (a comment may mention an attribute the file does not set).
fn xml_code(rel: &str) -> String {
    let mut text = read(rel);
    while let Some(start) = text.find("<!--") {
        let end = text[start..]
            .find("-->")
            .map(|e| start + e + 3)
            .expect("an unterminated comment");
        text.replace_range(start..end, "");
    }
    text
}

/// Source without its `//` comments.
fn code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The value of a `const NAME: &str = "value";` (or a Java `static final String NAME = "value";`).
fn constant(src: &str, name: &str) -> String {
    let at = src.find(&format!("{name}:")).or_else(|| src.find(&format!("String {name} ="))).unwrap_or_else(|| panic!("no constant {name}"));
    let rest = &src[at..];
    let open = rest.find('"').unwrap_or_else(|| panic!("no string after {name}"));
    let close = rest[open + 1..].find('"').unwrap();
    rest[open + 1..open + 1 + close].to_string()
}

/// The number of a `pub const NAME: usize = N;` or a Java `static final int NAME = N;`.
fn number(src: &str, name: &str) -> usize {
    let at = src.find(&format!("{name}:")).or_else(|| src.find(&format!("int {name} ="))).unwrap_or_else(|| panic!("no constant {name}"));
    let rest = &src[at..];
    let eq = rest.find('=').unwrap();
    let end = rest.find(';').unwrap();
    rest[eq + 1..end].trim().parse().unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The attribute `name` of the first element that starts with `tag`.
fn attribute(xml: &str, tag: &str, name: &str) -> Option<String> {
    let start = xml.find(tag)?;
    let end = start + xml[start..].find('>')?;
    let element = &xml[start..end];
    let key = format!("{name}=\"");
    let at = element.find(&key)? + key.len();
    let close = element[at..].find('"')?;
    Some(element[at..at + close].to_string())
}

/// The manifest has the app's share provider, and only it.
///
/// Mutation: the `<provider>` removed (the state of 96d83ae2e0: nothing of ours could be handed to another app).
#[test]
fn the_manifest_names_the_share_provider_not_exported_and_granting_per_uri() {
    let manifest = xml_code("scripts/android/AndroidManifest.xml");
    assert_eq!(manifest.matches("<provider").count(), 1, "exactly one provider");
    assert_eq!(
        attribute(&manifest, "<provider", "android:name").as_deref(),
        Some("com.azul.share.AzulShareProvider")
    );
    assert_eq!(
        attribute(&manifest, "<provider", "android:authorities").as_deref(),
        Some("@PACKAGE@.azul.share"),
        "the authority is <package>.azul.share, the one the Rust side builds"
    );
    assert_eq!(
        attribute(&manifest, "<provider", "android:exported").as_deref(),
        Some("false"),
        "another app reaches the provider only through a grant"
    );
    assert_eq!(attribute(&manifest, "<provider", "android:grantUriPermissions").as_deref(), Some("true"));
    for forbidden in ["<path-permission", "<grant-uri-permission", "android:readPermission", "android:writePermission"] {
        assert!(!manifest.contains(forbidden), "{forbidden}: a wide grant of the provider");
    }
    // no file path map of the data folder: the provider serves one folder, in code
    assert!(!read("scripts/android/AndroidManifest.xml").contains("FILE_PROVIDER_PATHS"));
}

/// The Rust side builds the authority the manifest declares and the folder the provider serves.
///
/// Mutation: `AUTHORITY_SUFFIX` changed to `.fileprovider` (the receiver would be handed a URI no provider answers).
#[test]
fn rust_and_java_agree_on_the_authority_the_folder_the_token_and_the_types_file() {
    let rust = code("dll/src/desktop/extra/share/android.rs");
    let java = code("scripts/android/AzulShareProvider.java");
    let area = code("layout/src/managers/share_area.rs");

    let suffix = constant(&rust, "AUTHORITY_SUFFIX");
    assert_eq!(suffix, ".azul.share");
    let manifest = xml_code("scripts/android/AndroidManifest.xml");
    assert_eq!(
        attribute(&manifest, "<provider", "android:authorities").unwrap(),
        format!("@PACKAGE@{suffix}")
    );

    assert_eq!(constant(&rust, "AREA_FOLDER"), constant(&java, "AREA"), "the folder the engine stages in is the folder the provider serves");
    assert_eq!(constant(&area, "TYPES_FILE"), constant(&java, "TYPES"), "the types file");
    assert_eq!(number(&area, "TOKEN_LEN"), number(&java, "TOKEN_LEN"), "the length of a token");
    assert_eq!(number(&area, "TOKEN_LEN"), 32);
}

/// The provider serves only what it must and cannot be talked into more.
///
/// Mutation: the canonical-path check removed (a `..` that survived the segment checks would leave the area); `openFile` taking
/// "w" (a receiver could write into the share).
#[test]
fn the_provider_serves_one_folder_read_only_behind_a_canonical_path_check() {
    let java = code("scripts/android/AzulShareProvider.java");
    assert!(java.contains("extends ContentProvider"));
    assert!(!java.contains(" native "), "a provider is created before the app's folders are prepared and in processes that never ran the engine: no native method");
    assert!(!java.contains("System.loadLibrary"), "the engine's library is not loaded by a provider");
    // the checks of a path
    assert!(java.contains("getPathSegments()") && java.contains("segments.size() != 2"));
    assert!(java.contains("isToken(token)"));
    assert!(java.contains("name.startsWith(\".\")"), "the hidden types file is not servable");
    assert!(java.contains("getCanonicalPath()") && java.contains("startsWith(inside)"));
    assert!(java.contains("file.isFile()"));
    // read-only
    assert!(java.contains("!\"r\".equals(mode)"));
    assert!(java.contains("ParcelFileDescriptor.MODE_READ_ONLY"));
    for method in ["insert(", "update(", "delete("] {
        let at = java.find(&format!("public {} {method}", if method == "insert(" { "Uri" } else { "int" })).unwrap_or_else(|| panic!("no {method}"));
        assert!(java[at..(at + 400).min(java.len())].contains("UnsupportedOperationException"), "{method} is refused");
    }
    // the columns a receiver asks
    assert!(java.contains("OpenableColumns.DISPLAY_NAME") && java.contains("OpenableColumns.SIZE"));
    // and the area is under the cache directory, never the files directory
    assert!(java.contains("getCacheDir()") && !java.contains("getFilesDir()"));
}

/// The chooser is made of what lets the chosen app read the files.
///
/// Mutation: the `ClipData` / the grant flag / the chooser removed (the receiver gets "permission denied" or the system's default
/// app); a `file://` URI.
#[test]
fn the_chooser_carries_the_uris_in_a_clip_with_a_read_grant_and_never_a_file_uri() {
    let rust = code("dll/src/desktop/extra/share/android.rs");
    for needed in [
        "android.intent.action.SEND",
        "android.intent.action.SEND_MULTIPLE",
        "android.intent.action.VIEW",
        "android.intent.extra.STREAM",
        "createChooser",
        "newRawUri",
        "setClipData",
        "FLAG_GRANT_READ_URI_PERMISSION",
        "startActivity",
        "content_uri(",
    ] {
        assert!(rust.contains(needed), "the chooser lacks {needed}");
    }
    assert!(!rust.contains("file://") && !rust.contains("fromFile"), "no file URI leaves the app");
    // a staged copy that never reached a sheet is removed again
    assert!(rust.contains("remove_dir_all"));
    // the share is staged in the CACHE directory, the one the provider serves
    assert!(rust.contains("getCacheDir"));
}

/// The share backend is installed where the App starts its services, and desktops answer unsupported (never a no-op).
///
/// Mutation: `share::install` not called (every share answered "no platform backend was installed").
#[test]
fn the_app_installs_the_share_backend_when_it_starts_its_services() {
    let app = code("dll/src/desktop/app.rs");
    let start = app.find("fn start_app_services").expect("start_app_services");
    let body = &app[start..(start + 1500).min(app.len())];
    assert!(
        body.contains("extra::share::install("),
        "App::start_app_services installs the share backend:\n{body}"
    );
}
