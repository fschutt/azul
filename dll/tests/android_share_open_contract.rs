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

// ---- step 2: share IN / open with ----

/// The activity is ONE instance whoever starts it, and takes the filters the build generates from the app's declaration.
///
/// Mutation: `launchMode="singleTop"` back (the state of 96d83ae2e0: an intent from another app started a second `AzulActivity`,
/// a second `android_main`, a black window); the placeholder removed (no filter reaches the manifest).
#[test]
fn the_activity_is_single_task_and_takes_the_generated_open_filters() {
    let manifest = xml_code("scripts/android/AndroidManifest.xml");
    let activity = &manifest[manifest.find("<activity").unwrap()..];
    let activity = &activity[..activity.find("</activity>").unwrap()];
    assert_eq!(attribute(&manifest, "<activity", "android:launchMode").as_deref(), Some("singleTask"));
    assert!(!manifest.contains("singleTop"), "no activity is singleTop: a share from another app would start a second one");
    assert!(
        activity.contains("@OPEN_FILTERS@"),
        "the generated filters are placed inside the activity element:\n{activity}"
    );
    // the placeholder is substituted by the generator's output, and by nothing when the app declares nothing
    let script = read("scripts/build-android.sh");
    assert!(script.contains("documents_manifest.py\" android \"$AZ_DOCUMENTS\""), "the filters come from the declaration");
    assert!(script.contains("-e \"/@OPEN_FILTERS@/r $OPEN_FILE\"") && script.contains("-e \"/@OPEN_FILTERS@/d\""));
    // the only MAIN / LAUNCHER filter in the template is the launcher's: no static VIEW / SEND filter is hand-written there
    for hand_written in ["android.intent.action.VIEW", "android.intent.action.SEND", "android.intent.action.EDIT"] {
        assert!(
            !activity.contains(hand_written),
            "{hand_written} is generated from the app's [[app.documents]], not written in the template"
        );
    }
}

/// Both intents reach the engine, and an auth redirect is not read as a file or a link.
///
/// Mutation: the forward in `onCreate` removed (the intent that STARTED the app is lost); the forward in `onNewIntent` moved before
/// the auth check (a sign-in redirect would reach the app's open handler as a link).
#[test]
fn the_activity_forwards_the_intent_that_started_it_and_the_ones_that_reach_it() {
    let java = code("scripts/android/AzulActivity.java");
    let on_create = {
        let at = java.find("protected void onCreate(").expect("onCreate");
        let rest = &java[at..];
        let end = rest.find("\n    }\n").expect("end of onCreate");
        &rest[..end]
    };
    let fresh = &on_create[on_create.find("if (savedInstanceState == null)").expect("the fresh-creation block")..];
    assert!(
        fresh.contains("AzulOpenIntent.forward(this, getIntent(), true)"),
        "the intent that started the app is forwarded, only on a fresh creation:\n{fresh}"
    );
    let on_new_intent = {
        let at = java.find("protected void onNewIntent(").expect("onNewIntent");
        let rest = &java[at..];
        &rest[..rest.find("\n    }\n").expect("end of onNewIntent")]
    };
    let auth = on_new_intent.find("AzulAuthSession.onIntent(intent)").expect("the auth check");
    let open = on_new_intent.find("AzulOpenIntent.forward(this, intent, false)").expect("the open forward");
    assert!(auth < open, "an auth redirect returns before the open forward");
    assert!(on_new_intent.contains("setIntent(intent)"));
}

/// The Java class holds one native and says nothing when the library is not loaded; Rust has the symbol with the arguments.
#[test]
fn the_open_intent_class_has_its_native_and_the_rust_symbol() {
    let java = code("scripts/android/AzulOpenIntent.java");
    assert!(java.contains("private static native void nativeOnIntent(Activity activity, Intent intent, boolean launchedApp)"));
    assert!(java.contains("catch (UnsatisfiedLinkError"), "no crash in onCreate when the library is not loaded");
    assert_eq!(java.matches(" native ").count(), 1, "one native, no logic");
    let rust = code("dll/src/desktop/extra/open_files/android.rs");
    let at = rust.find("fn Java_com_azul_open_AzulOpenIntent_nativeOnIntent(").expect("the JNI symbol");
    let signature = &rust[at..at + 400];
    for argument in ["raw_env", "_class", "activity: jni::sys::jobject", "intent: jni::sys::jobject", "launched_app: jni::sys::jboolean"] {
        assert!(signature.contains(argument), "{argument}:\n{signature}");
    }
    assert!(rust.contains("#[no_mangle]"));
}

/// The reading of an intent is cheap on the UI thread; the copying is a worker's, through a file descriptor, within limits.
///
/// Mutation: the copy run in the native (the UI thread blocks as long as a cloud provider streams: the ANR dialog after five seconds).
#[test]
fn the_copy_is_a_workers_through_a_descriptor_and_a_refusal_is_an_item() {
    let rust = code("dll/src/desktop/extra/open_files/android.rs");
    assert!(rust.contains("std::thread::Builder::new()"), "a worker thread");
    assert!(rust.contains("openFileDescriptor") && rust.contains("detachFd") && rust.contains("from_raw_fd"));
    assert!(!rust.contains("openInputStream"), "no stream read on a Java thread: a descriptor is a File");
    assert!(rust.contains("takePersistableUriPermission"), "the persistable grant is taken where the sender offered it");
    assert!(rust.contains("open_intent::classify"), "the classification is the tested pure function");
    assert!(rust.contains("copy_in(") && rust.contains("file_failed("), "every file is an item, a failure with its reason");
    assert!(rust.contains("WAIT_FOR_APP"), "the request that started the app waits for the App");
    // the native itself ends by clearing a pending exception: it would be thrown at the Java caller
    let native = &rust[rust.find("fn Java_com_azul_open_AzulOpenIntent_nativeOnIntent(").unwrap()..];
    assert!(native.contains("exception_clear"));
}

/// Both phone loops run the app's handler.
///
/// Mutation: the pump removed from the Android loop (the request is queued and nobody runs the handler).
#[test]
fn both_phone_loops_pump_the_open_requests() {
    for path in ["dll/src/desktop/shell2/android/mod.rs", "dll/src/desktop/shell2/ios/mod.rs"] {
        let src = code(path);
        assert!(src.contains("extra::open_files::pump("), "{path} runs the open handler");
    }
    let events = code("dll/src/desktop/app_events.rs");
    assert!(events.contains("extra::open_files::take_deliveries()") && events.contains("extra::open_files::invoke_deliveries("));
    assert!(events.contains("extra::open_files::defer_deliveries("), "a request with no window waits for one");
}
