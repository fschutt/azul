//! `azul-doc bundle macos <crate>` - wrap a cargo-built macOS binary in the
//! smallest `.app` macOS treats as an application.
//!
//! Why this exists: `UNUserNotificationCenter` (native notifications) only
//! serves a process that runs from a registered, signed app bundle. An
//! unbundled `target/release/<app>` has no bundle record, so azul reports
//! notifications as unavailable there (and never touches the center, which
//! would abort the process). Measured requirements (scripts/
//! NOTIFICATIONS_RESEARCH_2026_09_28.md, section 2.1):
//!
//! * an `Info.plist` with `CFBundleIdentifier` (without one: SIGABRT);
//! * a code signature - ad-hoc is enough, but the assembled bundle must be signed, the
//!   linker's own signature of the bare binary does not count (without: `UNErrorDomain` 1);
//! * LaunchServices registration (without: the same error). A bundle under `/var/folders` is
//!   registered but refused; `~/Applications` works.
//!
//! What it does, in this order: copy the binary to `X.app/Contents/MacOS/`,
//! write `Info.plist`, copy the dylibs the build produced into
//! `Contents/Frameworks/` and point the binary at the copies, sign the dylibs
//! and then the bundle ad-hoc (inside out, never `--deep`), and register it
//! with `lsregister`.
//!
//! Not done (yet): an `.icns` icon, entitlements (local notifications need
//! none), a Developer ID signature / notarization for distribution, and the
//! dylibs' own non-system dependencies.

use std::path::{Path, PathBuf};

/// What goes into the bundle's `Info.plist` and its directory name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacBundleSpec {
    /// `CFBundleName` / `CFBundleDisplayName`, and the `.app` directory name.
    pub app_name: String,
    /// `CFBundleExecutable`: the binary's file name in `Contents/MacOS/`.
    pub executable: String,
    /// `CFBundleIdentifier`: what macOS keys the notification permission on.
    pub bundle_id: String,
    /// The crate version; see [`bundle_version`].
    pub version: String,
}

/// Where the parts of a bundle live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundlePaths {
    pub app: PathBuf,
    pub contents: PathBuf,
    pub macos: PathBuf,
    pub frameworks: PathBuf,
    pub info_plist: PathBuf,
    pub executable: PathBuf,
}

/// A dylib the binary links that belongs INSIDE the bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledDylib {
    /// The reference exactly as the binary records it - what
    /// `install_name_tool -change` matches.
    pub reference: String,
    /// The file name it is copied to in `Contents/Frameworks/`.
    pub name: String,
    /// The file to copy, when the reference is an absolute path. `None` for
    /// an `@rpath/` (or `@executable_path/`, `@loader_path/`) reference, which
    /// is looked up in the build's output directory.
    pub source: Option<PathBuf>,
}

/// The `Info.plist` of the bundle.
pub fn info_plist(spec: &MacBundleSpec) -> String {
    let _ = spec;
    unimplemented!("info_plist")
}

/// `CFBundleShortVersionString` / `CFBundleVersion` from a crate version:
/// the numeric `major.minor.patch` prefix (`1.0.0-beta.2` -> `1.0.0`).
pub fn bundle_version(cargo_version: &str) -> String {
    let _ = cargo_version;
    unimplemented!("bundle_version")
}

/// The default `CFBundleIdentifier` of a crate: `com.azul.<name>`, using
/// only what Apple allows in one (letters, digits, `-`, `.`).
pub fn bundle_id_for(crate_name: &str) -> String {
    let _ = crate_name;
    unimplemented!("bundle_id_for")
}

/// `version = "..."` of the `[package]` table; `None` when the table has
/// none or inherits it (`version.workspace = true`).
pub fn package_version(cargo_toml: &str) -> Option<String> {
    let _ = cargo_toml;
    unimplemented!("package_version")
}

/// The dylibs from `otool -L <binary>` that the bundle must carry: every
/// `@rpath/` / `@executable_path/` / `@loader_path/` reference, and every
/// absolute one inside the build's `target_dir`. The system's libraries and
/// anything installed elsewhere stay where they are.
pub fn plan_dylibs(otool_l: &str, target_dir: &Path) -> Vec<BundledDylib> {
    let _ = (otool_l, target_dir);
    unimplemented!("plan_dylibs")
}

/// What the binary's reference to a bundled dylib becomes.
pub fn relinked_reference(name: &str) -> String {
    let _ = name;
    unimplemented!("relinked_reference")
}

/// LaunchServices registers a bundle under `/var/folders` (the per-user
/// temporary directory) but UN refuses it there.
pub fn is_refused_location(dir: &Path) -> bool {
    let _ = dir;
    unimplemented!("is_refused_location")
}

/// The layout of `<out_dir>/<app_name>.app`.
pub fn bundle_paths(out_dir: &Path, spec: &MacBundleSpec) -> BundlePaths {
    let _ = (out_dir, spec);
    unimplemented!("bundle_paths")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> MacBundleSpec {
        MacBundleSpec {
            app_name: "AzWidgets".to_string(),
            executable: "AzWidgets".to_string(),
            bundle_id: "com.azul.azwidgets".to_string(),
            version: "0.1.0".to_string(),
        }
    }

    /// The `<string>` that follows `<key>key</key>`, or `"true"` for `<true/>`.
    fn plist_value(plist: &str, key: &str) -> Option<String> {
        let marker = format!("<key>{key}</key>");
        let at = plist.find(&marker)? + marker.len();
        let rest = plist[at..].trim_start();
        if rest.starts_with("<true/>") {
            return Some("true".to_string());
        }
        let rest = rest.strip_prefix("<string>")?;
        Some(rest[..rest.find("</string>")?].to_string())
    }

    #[test]
    fn the_info_plist_carries_what_launch_services_and_notifications_need() {
        let plist = info_plist(&spec());
        assert!(plist.starts_with("<?xml"), "{plist}");
        assert!(plist.contains("<!DOCTYPE plist"), "{plist}");
        assert!(plist.trim_end().ends_with("</plist>"), "{plist}");
        for (key, value) in [
            ("CFBundleIdentifier", "com.azul.azwidgets"),
            ("CFBundleExecutable", "AzWidgets"),
            ("CFBundleName", "AzWidgets"),
            ("CFBundlePackageType", "APPL"),
            ("CFBundleShortVersionString", "0.1.0"),
            ("CFBundleVersion", "0.1.0"),
            ("CFBundleInfoDictionaryVersion", "6.0"),
            ("NSHighResolutionCapable", "true"),
        ] {
            assert_eq!(
                plist_value(&plist, key).as_deref(),
                Some(value),
                "{key} in\n{plist}"
            );
        }
    }

    #[test]
    fn the_executable_key_names_the_binary_not_the_app() {
        let mut spec = spec();
        spec.app_name = "Azul Widgets".to_string();
        let plist = info_plist(&spec);
        assert_eq!(
            plist_value(&plist, "CFBundleExecutable").as_deref(),
            Some("AzWidgets"),
            "launchd starts Contents/MacOS/<CFBundleExecutable>"
        );
        assert_eq!(
            plist_value(&plist, "CFBundleName").as_deref(),
            Some("Azul Widgets")
        );
    }

    #[test]
    fn plist_text_is_xml_escaped() {
        let mut spec = spec();
        spec.app_name = "Tom & Jerry <3>".to_string();
        assert_eq!(
            plist_value(&info_plist(&spec), "CFBundleName").as_deref(),
            Some("Tom &amp; Jerry &lt;3&gt;")
        );
    }

    #[test]
    fn a_prerelease_version_keeps_only_its_numbers() {
        assert_eq!(bundle_version("0.1.0"), "0.1.0");
        assert_eq!(bundle_version("1.0.0-beta.2"), "1.0.0");
        assert_eq!(bundle_version("2.3.4+build.5"), "2.3.4");
        assert_eq!(bundle_version("nightly"), "0");
        let mut spec = spec();
        spec.version = "1.0.0-alpha.1".to_string();
        assert_eq!(
            plist_value(&info_plist(&spec), "CFBundleShortVersionString").as_deref(),
            Some("1.0.0")
        );
    }

    #[test]
    fn a_bundle_identifier_uses_only_the_characters_apple_allows() {
        assert_eq!(bundle_id_for("AzWidgets"), "com.azul.azwidgets");
        assert_eq!(bundle_id_for("my_app 2"), "com.azul.my-app-2");
        assert_eq!(bundle_id_for("azul-paint"), "com.azul.azul-paint");
        assert_eq!(bundle_id_for("__"), "com.azul.app");
    }

    #[test]
    fn the_package_version_comes_from_the_package_table_only() {
        let toml = "[package]\nname = \"AzWidgets\"\nversion = \"1.2.3\"\nedition = \
                    \"2021\"\n\n[dependencies]\nfoo = { version = \"9\" }\n";
        assert_eq!(package_version(toml).as_deref(), Some("1.2.3"));

        let dependency_first = "[dependencies]\nversion = \"9\"\n\n[package]\nname = \"x\"\n";
        assert_eq!(package_version(dependency_first), None);

        let inherited = "[package]\nname = \"x\"\nversion.workspace = true\n";
        assert_eq!(package_version(inherited), None);

        let lookalike = "[package]\nversioned = \"no\"\nversion = \"0.4.0\"\n";
        assert_eq!(package_version(lookalike).as_deref(), Some("0.4.0"));
    }

    #[test]
    fn only_the_builds_own_dylibs_are_bundled() {
        let otool = "/Users/me/azul/target/release/AzWidgets:\n\
            \t/Users/me/azul/target/release/build/azul-dll-a0e8/out/libazul.dylib (compatibility version 0.0.0, current version 0.0.0)\n\
            \t@rpath/libextra.dylib (compatibility version 1.0.0, current version 1.0.0)\n\
            \t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1351.0.0)\n\
            \t/System/Library/Frameworks/AppKit.framework/Versions/C/AppKit (compatibility version 45.0.0, current version 2575.0.0)\n\
            \t/opt/homebrew/lib/libz.1.dylib (compatibility version 1.0.0, current version 1.3.1)\n";
        let plan = plan_dylibs(otool, Path::new("/Users/me/azul/target"));
        assert_eq!(
            plan,
            vec![
                BundledDylib {
                    reference: "/Users/me/azul/target/release/build/azul-dll-a0e8/out/libazul.dylib"
                        .to_string(),
                    name: "libazul.dylib".to_string(),
                    source: Some(PathBuf::from(
                        "/Users/me/azul/target/release/build/azul-dll-a0e8/out/libazul.dylib"
                    )),
                },
                BundledDylib {
                    reference: "@rpath/libextra.dylib".to_string(),
                    name: "libextra.dylib".to_string(),
                    source: None,
                },
            ],
            "the binary itself, the system's libraries and a Homebrew library stay out"
        );
    }

    #[test]
    fn a_fat_binary_lists_each_dylib_once() {
        let otool = "AzWidgets (architecture x86_64):\n\
            \t@rpath/libazul.dylib (compatibility version 0.0.0, current version 0.0.0)\n\
            AzWidgets (architecture arm64):\n\
            \t@rpath/libazul.dylib (compatibility version 0.0.0, current version 0.0.0)\n";
        assert_eq!(plan_dylibs(otool, Path::new("/nowhere")).len(), 1);
    }

    #[test]
    fn a_bundled_dylib_is_found_next_to_the_executable() {
        assert_eq!(
            relinked_reference("libazul.dylib"),
            "@executable_path/../Frameworks/libazul.dylib"
        );
    }

    #[test]
    fn a_bundle_is_never_put_where_notifications_are_refused() {
        assert!(is_refused_location(Path::new("/var/folders/ab/xyz/T/out")));
        assert!(is_refused_location(Path::new("/private/var/folders/ab/xyz/T")));
        assert!(!is_refused_location(Path::new("/Users/me/Applications")));
        assert!(!is_refused_location(Path::new("/Applications")));
    }

    #[test]
    fn the_bundle_is_the_minimal_app_layout() {
        let paths = bundle_paths(Path::new("/Users/me/Applications"), &spec());
        assert_eq!(paths.app, PathBuf::from("/Users/me/Applications/AzWidgets.app"));
        assert_eq!(
            paths.info_plist,
            PathBuf::from("/Users/me/Applications/AzWidgets.app/Contents/Info.plist")
        );
        assert_eq!(
            paths.executable,
            PathBuf::from("/Users/me/Applications/AzWidgets.app/Contents/MacOS/AzWidgets")
        );
        assert_eq!(
            paths.frameworks,
            PathBuf::from("/Users/me/Applications/AzWidgets.app/Contents/Frameworks")
        );
    }
}
