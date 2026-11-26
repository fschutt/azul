//! How a consumer of a prebuilt static `libazul.a` learns which system libraries to link it with.
//!
//! A static archive carries no link metadata, so `dll/build_link.rs` named the libraries of the
//! desktop targets in a list written by hand, "discovered one link error wave at a time". iOS was
//! never on that list (the Apple branch names AppKit and IOKit, which an iPhone does not have).
//! The job that builds libazul for a target now writes what rustc itself prints for the archive
//! (`rustc --print native-static-libs`) next to it, and the consumer reads that. These tests pin
//! the reading; the file is `include!`d the way `dll/build.rs` and the pre-rendered `azul` crate
//! include it, so they need nothing built:
//!
//!     rustc --edition 2021 --test dll/tests/build_link_native_libs.rs -o /tmp/t && /tmp/t

#![allow(dead_code)]

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

include!("../build_link.rs");

#[test]
fn an_ios_archive_names_its_frameworks_and_libraries_in_order() {
    let (libs, ignored) =
        parse_native_static_libs("-framework Foundation -framework UIKit -lSystem -lc++ -liconv\n");
    assert_eq!(
        libs,
        vec![
            NativeLib::Framework("Foundation".into()),
            NativeLib::Framework("UIKit".into()),
            NativeLib::Library("System".into()),
            NativeLib::Library("c++".into()),
            NativeLib::Library("iconv".into()),
        ]
    );
    assert!(ignored.is_empty(), "{ignored:?}");
}

#[test]
fn an_msvc_archive_names_import_libraries_by_their_file() {
    let (libs, ignored) = parse_native_static_libs("kernel32.lib advapi32.lib ntdll.lib");
    assert_eq!(
        libs,
        vec![
            NativeLib::Library("kernel32".into()),
            NativeLib::Library("advapi32".into()),
            NativeLib::Library("ntdll".into()),
        ]
    );
    assert!(ignored.is_empty());
}

#[test]
fn a_search_path_is_kept_and_a_linker_argument_is_reported_not_dropped() {
    let (libs, ignored) = parse_native_static_libs("-L/opt/sdk/usr/lib -lm -Wl,--as-needed");
    assert_eq!(
        libs,
        vec![
            NativeLib::SearchPath("/opt/sdk/usr/lib".into()),
            NativeLib::Library("m".into()),
        ]
    );
    assert_eq!(ignored, vec!["-Wl,--as-needed".to_string()]);
}

#[test]
fn a_framework_flag_without_a_name_is_reported() {
    let (libs, ignored) = parse_native_static_libs("-lc -framework");
    assert_eq!(libs, vec![NativeLib::Library("c".into())]);
    assert_eq!(ignored, vec!["-framework".to_string()]);
}

#[test]
fn an_empty_list_is_an_empty_answer() {
    let (libs, ignored) = parse_native_static_libs("  \n");
    assert!(libs.is_empty() && ignored.is_empty());
}

#[test]
fn the_list_sits_next_to_the_archive_under_the_archives_own_name() {
    assert_eq!(
        native_libs_file(Path::new("/lib/ios/libazul.a")),
        PathBuf::from("/lib/ios/libazul.a.native-libs")
    );
    // a platform-suffixed release name keeps its suffix, so two archives in one folder do not share a list
    assert_eq!(
        native_libs_file(Path::new("libazul.macos-x86_64.a")),
        PathBuf::from("libazul.macos-x86_64.a.native-libs")
    );
}
