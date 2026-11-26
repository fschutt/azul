//! The Android shared objects are linked for 16 KB memory pages.
//!
//! "NDK version r28 and higher compile 16 KB-aligned by default"; for r27 and lower the page-sizes guide
//! (https://developer.android.com/guide/practices/page-sizes) says to link with
//! `-Wl,-z,max-page-size=16384 -Wl,-z,common-page-size=16384` and to "check the output lines to ensure that
//! the load segments don't have values less than 2**14". The build uses NDK r27. Every PT_LOAD of
//! libazul.so and of the app's library had `p_align = 0x1000` (4 KB): on an Android 15 phone that runs
//! with 16 KB pages the library does not load (the APK stores it compressed, so the system's
//! 4 KB-compatibility mode, which needs uncompressed and 4 KB-aligned ELF files in the APK, does not
//! apply), and Google Play requires apps that target API 35 or higher to support 16 KB pages on 64-bit
//! devices.
//!
//! libazul.so is linked by this workspace's `dll/build.rs` (`rustc-cdylib-link-arg`, which reaches the
//! cdylib of THAT package only); the app's own cdylib (a different package) gets the flags from the
//! workspace's `.cargo/config.toml`, which a `RUSTFLAGS` in the environment replaces. The built files are
//! checked by briefs/mobile_api_research_android_check_elf_16kb.py; this test pins where the flags come
//! from. It needs nothing built:
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_libraries_are_linked_for_16_kb_pages.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn the_dll_build_script_links_libazul_for_16_kb_pages() {
    let build = read("dll/build.rs");
    let android = build
        .split("fn configure_android()")
        .nth(1)
        .expect("dll/build.rs has configure_android");
    assert!(
        android.contains("rustc-cdylib-link-arg=-Wl,-z,max-page-size=16384"),
        "libazul.so must be linked with -z max-page-size=16384 (NDK r27 defaults to 4 KB)"
    );
    assert!(android.contains("rustc-cdylib-link-arg=-Wl,-z,common-page-size=16384"));
}

#[test]
fn every_android_target_links_the_apps_cdylib_for_16_kb_pages() {
    let config = read(".cargo/config.toml");
    for triple in ["aarch64-linux-android", "x86_64-linux-android"] {
        let section = config
            .split(&format!("[target.{triple}]"))
            .nth(1)
            .unwrap_or_else(|| panic!(".cargo/config.toml has no [target.{triple}]"))
            .split("\n[")
            .next()
            .unwrap();
        assert!(
            section.contains("rustflags")
                && section.contains("max-page-size=16384")
                && section.contains("common-page-size=16384"),
            "[target.{triple}] must carry the 16 KB link flags in its rustflags"
        );
    }
}
