//! Store builds. iOS and Android (and any build made with the `store_app` feature) are delivered by
//! a store that updates them, and a store forbids an app to download or install code of its own
//! (App Store Review Guideline 2.5.2, Google Play's Device and Network Abuse policy). Such a build
//! has no code that fetches, stages or installs a program.
//!
//! What is checked, and where it runs:
//!
//! * on every cfg: the install kind, and a gate over the SOURCE of the updater that fails when
//!   something that fetches or installs a program is not compiled out of a store build, or
//!   something that stays calls it, or something that reaches the network is not on the list of
//!   what reads a manifest;
//! * on a desktop (`cargo test -p azul-layout --lib --features updater,store_app --
//!   updater::store_tests`): the store build itself, against a server that counts what it is asked
//!   for: the manifest may be read, the program is never requested.

use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use super::*;

/// The `cfg` that takes code out of a store build, as the source spells it on every such item.
const NOT_STORE: &str =
    r#"not(any(target_os = "ios", target_os = "android", feature = "store_app"))"#;
/// The same condition, on the stand-in that takes the place of an item on a store build.
const STORE: &str = r#"any(target_os = "ios", target_os = "android", feature = "store_app")"#;

/// Calls of what a store build does not have.
const COMPILED_OUT: [&str; 6] = [
    "download_update(",
    "download_update_capped(",
    "download_and_verify(",
    "apply_update(",
    "copy_verified(",
    "swap_moving_the_old_aside(",
];

#[test]
fn a_store_install_names_its_store_and_never_updates_itself() {
    assert!(!InstallKind::AppStore.allows_self_update());
    for requested in [UpdateMode::SelfUpdate, UpdateMode::NotifyOnly] {
        assert_eq!(
            effective_mode(requested, &InstallKind::AppStore),
            UpdateMode::NotifyOnly,
            "{requested:?}"
        );
    }
    assert_eq!(
        effective_mode(UpdateMode::Disabled, &InstallKind::AppStore),
        UpdateMode::Disabled
    );
    let hint = InstallKind::AppStore.package_manager_hint();
    assert!(
        !hint.is_empty(),
        "a store install says where its updates come from"
    );
    if cfg!(target_os = "ios") {
        assert_eq!(hint, "the App Store");
    }
    if cfg!(target_os = "android") {
        assert_eq!(hint, "Google Play");
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android", feature = "store_app")))]
#[test]
fn a_desktop_build_never_finds_that_it_is_a_store_install() {
    for snap in [false, true] {
        for path in [
            "/usr/bin/azwriter",
            "/home/u/.local/bin/azwriter",
            "/opt/azwriter",
            "C:\\Users\\u\\azwriter.exe",
        ] {
            assert_ne!(
                InstallKind::detect_with(Path::new(path), snap),
                InstallKind::AppStore,
                "{path}"
            );
        }
    }
}

// ----- the gate over the source -----

/// One top-level item of a source file: its attributes, the name it declares and its text.
struct Item {
    attrs: String,
    name: String,
    text: String,
}

/// The name an item declares: `download_update` for `pub fn download_update(`, `MAX_UPDATE_BYTES`
/// for `const MAX_UPDATE_BYTES: u64`.
fn declared_name(header: &str) -> String {
    for keyword in [
        "fn ", "const ", "static ", "struct ", "enum ", "type ", "mod ",
    ] {
        if let Some(at) = header.find(keyword) {
            let name: String = header[at + keyword.len()..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return name;
            }
        }
    }
    header.trim().to_string()
}

/// The top-level items of a source: a line that starts in column 0 with a letter starts an item,
/// the attribute lines in column 0 above it are its attributes, everything indented or closing
/// belongs to the item it is under.
fn items(source: &str) -> Vec<Item> {
    let mut found: Vec<Item> = Vec::new();
    let mut attrs = String::new();
    let mut current: Option<Item> = None;
    for line in source.lines() {
        let top = line
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '#' || c == '/');
        if !top {
            if let Some(item) = current.as_mut() {
                item.text.push_str(line);
                item.text.push('\n');
            }
            continue;
        }
        if let Some(item) = current.take() {
            found.push(item);
            attrs.clear();
        }
        if line.starts_with("//") {
            continue;
        }
        if line.starts_with("#[") || line.starts_with("#![") {
            attrs.push_str(line);
            attrs.push('\n');
            continue;
        }
        current = Some(Item {
            attrs: std::mem::take(&mut attrs),
            name: declared_name(line),
            text: format!("{line}\n"),
        });
    }
    found.extend(current);
    found
}

/// The items of `items` that declare `name`.
fn named<'a>(items: &'a [Item], name: &str) -> Vec<&'a Item> {
    items.iter().filter(|item| item.name == name).collect()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir)
        .expect("read the source folder")
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_code_that_fetches_or_installs_a_program_is_compiled_out_of_a_store_build() {
    let source = include_str!("updater.rs");
    let production = &source[..source
        .find("\nmod tests {")
        .expect("the tests module of updater.rs")];
    let items = items(production);

    // what fetches, stages or installs a program has no place in a store build
    for name in [
        "MAX_UPDATE_BYTES",
        "check_pin",
        "staged_artifact_name",
        "download_update",
        "download_update_capped",
        "download_and_verify",
        "copy_verified",
        "archive_kind",
        "swap_moving_the_old_aside",
        "apply_update",
    ] {
        let found = named(&items, name);
        assert!(
            !found.is_empty(),
            "{name} is not in updater.rs any more: change this list"
        );
        for item in found {
            assert!(
                item.attrs.contains(NOT_STORE),
                "{name} is compiled into a store build: its attributes are {:?}",
                item.attrs
            );
        }
    }

    // the entry points every build has (the C API names them) refuse on a store build
    for name in [
        "stage_program_update",
        "stage_program_update_with",
        "stage_automatically",
    ] {
        let found = named(&items, name);
        assert_eq!(
            found.len(),
            2,
            "{name}: the version that fetches and the stand-in of a store build"
        );
        assert!(
            found.iter().any(|i| i.attrs.contains(NOT_STORE)),
            "{name}: no version is compiled out of a store build"
        );
        assert!(
            found
                .iter()
                .any(|i| i.attrs.contains(STORE) && !i.attrs.contains(NOT_STORE)),
            "{name}: a store build has no stand-in"
        );
    }

    // nothing that stays in a store build calls what was compiled out of it
    for item in &items {
        if item.attrs.contains(NOT_STORE) {
            continue;
        }
        for call in COMPILED_OUT {
            assert!(
                !item.text.contains(call),
                "{} calls {call} and is compiled into a store build",
                item.name
            );
        }
    }

    // the network is reached, in a store build, only by what reads a manifest or a signature file
    for item in &items {
        if item.attrs.contains(NOT_STORE) || !item.text.contains("crate::http::") {
            continue;
        }
        assert!(
            [
                "resolve_oci",
                "resolve_release_candidates",
                "hydrate_sidecars"
            ]
            .contains(&item.name.as_str()),
            "{} reaches the network and is compiled into a store build: only what reads a \
             manifest or a signature file may, so read what it fetches and add it to this list, \
             or compile it out",
            item.name
        );
    }

    // the dialog that installs is compiled out too
    let dialogs = include_str!("dialogs/mod.rs");
    let at = dialogs
        .find("pub mod update_version;")
        .expect("the update dialog module");
    let attribute = dialogs[..at].trim_end().lines().last().unwrap_or_default();
    assert!(
        attribute.contains(NOT_STORE),
        "the update dialog installs a program and is compiled into a store build: {attribute:?}"
    );

    // and no other file calls what was compiled out
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    for file in files {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        if name == "updater.rs" || name.starts_with("updater_") || name == "update_version.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        for call in COMPILED_OUT {
            assert!(
                !text.contains(call),
                "{} calls {call}, which a store build does not have",
                file.display()
            );
        }
    }
}

// ----- a store build, running -----

#[cfg(any(target_os = "ios", target_os = "android", feature = "store_app"))]
mod on_a_store_build {
    use super::*;

    const PROGRAM: &[u8] = b"\x7fELF the program that a store build must never fetch";

    /// What the server was asked for.
    #[derive(Default)]
    struct Hits {
        manifest: AtomicUsize,
        program: AtomicUsize,
    }

    /// A server that offers version 9.0.0 of a program, pinned by its digest: the manifest at
    /// `/manifest.json`, the program at `/program.bin`. Returns its address and its counters.
    fn offer() -> (String, Arc<Hits>) {
        use sha2::Digest as _;
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&sha2::Sha256::digest(PROGRAM));
        let manifest = format!(
            r#"{{"latest": {{"version": "9.0.0", "download_url": "{base}/program.bin", "digest": "sha256:{}"}}}}"#,
            sha256_hex(&hash)
        );
        let hits = Arc::new(Hits::default());
        let counted = Arc::clone(&hits);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut request = String::new();
                if reader.read_line(&mut request).is_err() {
                    continue;
                }
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) if line == "\r\n" => break,
                        Ok(_) => {}
                    }
                }
                let body: Vec<u8> = match request.split_whitespace().nth(1).unwrap_or_default() {
                    "/manifest.json" => {
                        counted.manifest.fetch_add(1, Ordering::SeqCst);
                        manifest.clone().into_bytes()
                    }
                    "/program.bin" => {
                        counted.program.fetch_add(1, Ordering::SeqCst);
                        PROGRAM.to_vec()
                    }
                    _ => Vec::new(),
                };
                let status = if body.is_empty() {
                    "404 Not Found"
                } else {
                    "200 OK"
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        (base, hits)
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("azul-store-{}-{name}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        dir
    }

    /// What a check finds in the manifest the server offers.
    fn offered(base: &str) -> ReleaseInfo {
        let mut state = UpdateState::default();
        let result = check_for_updates_blocking(
            &format!("{base}/manifest.json"),
            "1.0.0",
            "stable",
            &mut state,
            UpdateAudience::Tester,
        );
        let UpdateCheckResult::Available(release) = result else {
            panic!("a check reads the manifest of a store build too: {result:?}");
        };
        release
    }

    #[test]
    fn a_store_build_is_a_store_install_wherever_its_binary_lies() {
        for snap in [false, true] {
            for path in [
                "/usr/bin/azwriter",
                "/home/u/.local/bin/azwriter",
                "/data/app/azwriter",
            ] {
                assert_eq!(
                    InstallKind::detect_with(Path::new(path), snap),
                    InstallKind::AppStore,
                    "{path}, snap {snap}"
                );
            }
        }
    }

    #[test]
    fn a_store_build_reads_the_manifest_and_never_asks_for_the_program() {
        let (base, hits) = offer();
        let release = offered(&base);
        assert_eq!(release.version.as_str(), "9.0.0");
        assert_eq!(
            hits.manifest.load(Ordering::SeqCst),
            1,
            "the manifest is read"
        );

        let dir = scratch("stage");
        let staging = dir.join("staging");
        let state = dir.join("state");

        let why = stage_program_update(&release, "azapp", &staging, "", true, &state)
            .expect_err("a store build stages nothing");
        assert!(why.contains("store"), "the refusal says why: {why}");
        let why = stage_program_update_with(
            &release,
            &Expect::here("azapp"),
            &staging,
            &UpdatePolicy::DigestOnly,
            &state,
        )
        .expect_err("a store build stages nothing, under any policy");
        assert!(why.contains("store"), "the refusal says why: {why}");

        assert_eq!(
            hits.program.load(Ordering::SeqCst),
            0,
            "not one request for the program"
        );
        assert!(!staging.exists(), "no staging folder was made");
        drop(std::fs::remove_dir_all(&dir));
    }

    #[test]
    fn the_automatic_staging_of_a_check_fetches_nothing_on_a_store_build() {
        let (base, hits) = offer();
        let release = offered(&base);
        let dir = scratch("automatic");
        // an app that CAN prove an update (the digest pin it chose) and asks for the download
        let env = crate::appenv::AppEnv {
            update_allow_unsigned: true,
            update_mode: UpdateMode::SelfUpdate,
            ..crate::appenv::AppEnv::default()
        };
        let staged = stage_automatically(
            &env,
            &UpdateCheckResult::Available(release),
            UpdateOptions {
                download_automatically: true,
            },
            UpdateMode::SelfUpdate,
            1_700_000_000,
            &dir,
        );
        assert_eq!(staged, azul_css::OptionString::None);
        assert_eq!(
            hits.program.load(Ordering::SeqCst),
            0,
            "not one request for the program"
        );
        assert!(!dir.join("staging").exists(), "no staging folder was made");
        drop(std::fs::remove_dir_all(&dir));
    }
}
