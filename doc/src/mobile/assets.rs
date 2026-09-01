//! The build scripts and Android/iOS templates, compiled INTO the binary.
//!
//! # Why
//!
//! `mobile build` and `mobile run` used to shell out to
//! `<project_root>/scripts/build-android.sh`, and `project_root` came from
//! `env!("CARGO_MANIFEST_DIR")` — an absolute path recorded on whatever machine
//! produced the binary. That is fine while azul-doc is only ever `cargo run`
//! from the azul checkout, and completely broken the moment it is a CI artifact
//! someone downloads: the path names a directory that does not exist on their
//! disk, so half the subcommands fail on a stat of somebody else's home folder.
//!
//! So the scripts and the templates they read travel with the binary and are
//! written out to a cache directory on first use. A downloaded azul-doc can
//! then build an APK for a crate that is not in the azul tree at all, which is
//! the actual promise of shipping it.
//!
//! # Layout
//!
//! Materialized to `<cache>/azul-doc/<version>/`, reproducing the repo-relative
//! paths the scripts expect of each other (`scripts/android/*.java` next to
//! `scripts/build-android.sh`). The scripts locate the workspace they BUILD via
//! `AZ_WORKSPACE_ROOT`, not via their own location — that is the one change
//! they needed, and it is also correct in-tree.

use std::path::{Path, PathBuf};

/// Bumped whenever an embedded file changes in a way that must invalidate an
/// already-materialized cache. Cheap insurance: the directory name carries it,
/// so a stale cache is never read, only orphaned.
const ASSET_REVISION: &str = "1";

/// `(relative path, contents, executable)`
const ASSETS: &[(&str, &str, bool)] = &[
    (
        "scripts/build-android.sh",
        include_str!("../../../scripts/build-android.sh"),
        true,
    ),
    (
        "scripts/build-ios.sh",
        include_str!("../../../scripts/build-ios.sh"),
        true,
    ),
    (
        "scripts/mobile-check-all.sh",
        include_str!("../../../scripts/mobile-check-all.sh"),
        true,
    ),
    (
        "scripts/cargo_bin_path.py",
        include_str!("../../../scripts/cargo_bin_path.py"),
        false,
    ),
    (
        "scripts/android/AndroidManifest.xml",
        include_str!("../../../scripts/android/AndroidManifest.xml"),
        false,
    ),
    (
        "scripts/android/AzulActivity.java",
        include_str!("../../../scripts/android/AzulActivity.java"),
        false,
    ),
    (
        "scripts/android/AzulAccessibilityBridge.java",
        include_str!("../../../scripts/android/AzulAccessibilityBridge.java"),
        false,
    ),
    (
        "scripts/android/AzulFilePicker.java",
        include_str!("../../../scripts/android/AzulFilePicker.java"),
        false,
    ),
    (
        "scripts/android/NativeGestureBridge.java",
        include_str!("../../../scripts/android/NativeGestureBridge.java"),
        false,
    ),
    (
        "scripts/ios/Info.plist",
        include_str!("../../../scripts/ios/Info.plist"),
        false,
    ),
    (
        "scripts/ios/entitlements.xcent",
        include_str!("../../../scripts/ios/entitlements.xcent"),
        false,
    ),
];

/// `$XDG_CACHE_HOME` / `~/.cache` / `~/Library/Caches`, plus our own subdir.
fn cache_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("AZUL_DOC_CACHE") {
        return PathBuf::from(dir);
    }
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(dir).join("azul-doc");
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    if cfg!(target_os = "macos") {
        home.join("Library").join("Caches").join("azul-doc")
    } else {
        home.join(".cache").join("azul-doc")
    }
}

/// Write the embedded assets out (if not already current) and return the
/// directory that now contains `scripts/`.
///
/// Prefers the live repo when we are running inside one: an in-tree edit to
/// `build-android.sh` should take effect without recompiling azul-doc, which is
/// how it was developed and how it will keep being debugged.
pub fn ensure(project_root: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(root) = project_root {
        if root.join("scripts").join("build-android.sh").is_file() {
            return Ok(root.to_path_buf());
        }
    }

    let dir = cache_root().join(format!(
        "{}-{ASSET_REVISION}",
        env!("CARGO_PKG_VERSION")
    ));
    for (rel, contents, exec) in ASSETS {
        let path = dir.join(rel);
        // Content-compare rather than blindly rewriting: several azul-doc
        // processes can run at once (the e2e matrix does), and rewriting a
        // script another process is mid-read truncates it under them.
        if std::fs::read_to_string(&path).ok().as_deref() == Some(*contents) {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Write-then-rename so a reader never observes a partial file.
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, contents)?;
        std::fs::rename(&tmp, &path)?;
        if *exec {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
            }
        }
    }
    Ok(dir)
}
