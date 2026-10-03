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
//! write `Info.plist`, write the icon to `Contents/Resources/AppIcon.icns`,
//! copy the dylibs the build produced - and THEIR dylibs, walked with
//! `otool -L` ([`plan_dylib_tree`]) - into `Contents/Frameworks/`, give each
//! copy its bundle install name (`install_name_tool -id`) and point every
//! reference at the copies (`-change`, in the binary and in the dylibs),
//! sign the dylibs and then the bundle ad-hoc (inside out, never `--deep`),
//! and register it with `lsregister`.
//!
//! * **The icon** comes from `--icon <path>` or the crate's `[package.metadata.bundle] icon`
//!   (the `cargo-bundle` key, [`configured_icons`]): an `.icns` is copied, square PNGs of the
//!   sizes an `.icns` holds are wrapped into one as they are ([`icns_from_pngs`]; no `iconutil`,
//!   no resampling).
//! * **The id** (`CFBundleIdentifier`) comes from `--bundle-id`, else the crate's
//!   `[package.metadata.bundle] identifier` (the id the app sets as `AppConfig::app_id`,
//!   [`default_bundle_id`]), else `com.azul.<binary>`. The running app keeps the bundle's id
//!   and logs a warning when its `app_id` names another app.
//! * **The libraries**: by default the build's own ([`DylibScope::Build`]); `--portable` also
//!   takes Homebrew's and MacPorts' - everything outside `/usr/lib` and `/System`
//!   ([`DylibScope::NonSystem`]) - for a Mac that does not have them.
//!
//! Not done: entitlements (local notifications need none) and a signature for
//! DISTRIBUTION. The ad-hoc signature is enough on the Mac that built the
//! bundle; Gatekeeper on any other Mac refuses it. Distribution needs a paid
//! Apple Developer account and, by hand:
//!
//! 1. Sign inside out with the "Developer ID Application" certificate and the hardened runtime:
//!    `codesign --force --options runtime --timestamp --sign "Developer ID Application: <Name>
//!    (<TEAMID>)"` on each `Contents/Frameworks/*.dylib`, then on `X.app`.
//! 2. Zip it (`ditto -c -k --keepParent X.app X.zip`) and submit it:
//!    `xcrun notarytool submit X.zip --apple-id <id> --team-id <TEAMID> --password
//!    <app-specific password> --wait` (or `--keychain-profile <name>` after `xcrun notarytool
//!    store-credentials`).
//! 3. Staple the ticket to the bundle: `xcrun stapler staple X.app`; check with
//!    `spctl --assess --type execute -vv X.app` ("source=Notarized Developer ID").
//!
//! These need credentials this command does not have, so it stops at the ad-hoc signature.

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
    /// `CFBundleIconFile`: the `.icns` in `Contents/Resources/`, if the
    /// bundle has an icon.
    pub icon_file: Option<String>,
}

/// Where the parts of a bundle live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundlePaths {
    pub app: PathBuf,
    pub contents: PathBuf,
    pub macos: PathBuf,
    pub frameworks: PathBuf,
    /// `Contents/Resources/`: the icon.
    pub resources: PathBuf,
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

/// Escape text for a plist `<string>`.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// The `Info.plist` of the bundle.
pub fn info_plist(spec: &MacBundleSpec) -> String {
    let name = xml_escape(&spec.app_name);
    let executable = xml_escape(&spec.executable);
    let id = xml_escape(&spec.bundle_id);
    let version = xml_escape(&bundle_version(&spec.version));
    let icon = spec.icon_file.as_deref().map_or_else(String::new, |file| {
        format!(
            "    <key>CFBundleIconFile</key>\n    <string>{}</string>\n",
            xml_escape(file)
        )
    });
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>{name}</string>
    <key>CFBundleExecutable</key>
    <string>{executable}</string>
{icon}    <key>CFBundleIdentifier</key>
    <string>{id}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>{name}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
"#
    )
}

/// `CFBundleShortVersionString` / `CFBundleVersion` from a crate version:
/// the numeric `major.minor.patch` prefix (`1.0.0-beta.2` -> `1.0.0`).
pub fn bundle_version(cargo_version: &str) -> String {
    let numeric: String = cargo_version
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let numeric = numeric.trim_end_matches('.');
    if numeric.is_empty() {
        "0".to_string()
    } else {
        numeric.to_string()
    }
}

/// The `CFBundleIdentifier` of a binary nobody named: `com.azul.<name>`,
/// using only what Apple allows in one (letters, digits, `-`, `.`). A crate
/// that declares `[package.metadata.bundle] identifier` gets that instead
/// ([`default_bundle_id`]).
///
/// The app's ONE identity (`wire::AppIdentity`), not a derivation of its own:
/// the same binary running unbundled, or on Windows (its toast AUMID), names
/// itself exactly this unless it sets `AppConfig::app_id`.
pub fn bundle_id_for(binary_name: &str) -> String {
    azul_layout::managers::notification::wire::AppIdentity::from_executable(binary_name)
        .apple_bundle_id()
}

/// `version = "..."` of the `[package]` table; `None` when the table has
/// none or inherits it (`version.workspace = true`).
///
/// A line scan, like `mobile::run::Target::resolve`: azul-doc has no TOML
/// parser, and this one field is all it needs.
pub fn package_version(cargo_toml: &str) -> Option<String> {
    let mut in_package = false;
    for line in cargo_toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some(rest) = t.strip_prefix("version") else {
            continue;
        };
        // `versioned = ..` and `version.workspace = true` are not it.
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim().strip_prefix('"')?;
        return value.split('"').next().map(str::to_string);
    }
    None
}

/// The dylibs from `otool -L <binary>` that the bundle must carry: every
/// `@rpath/` / `@executable_path/` / `@loader_path/` reference, and every
/// absolute one inside the build's `target_dir`. The system's libraries and
/// anything installed elsewhere stay where they are.
pub fn plan_dylibs(otool_l: &str, target_dir: &Path) -> Vec<BundledDylib> {
    plan_dylibs_in(otool_l, target_dir, DylibScope::Build)
}

/// A library of the system - never bundled: `/usr/lib/` and `/System/`.
fn is_system_library(reference: &str) -> bool {
    reference.starts_with("/usr/lib/") || reference.starts_with("/System/")
}

/// [`plan_dylibs`] for a [`DylibScope`]: `NonSystem` also takes every
/// absolute reference outside the system's directories.
pub fn plan_dylibs_in(otool_l: &str, target_dir: &Path, scope: DylibScope) -> Vec<BundledDylib> {
    let mut out: Vec<BundledDylib> = Vec::new();
    for line in otool_l.lines() {
        // Dependencies are indented; an unindented line names the file (or,
        // for a fat binary, one of its architectures).
        if !line.starts_with(char::is_whitespace) {
            continue;
        }
        let entry = line.trim();
        let reference = match entry.rsplit_once(" (compatibility version") {
            Some((path, _)) => path.trim(),
            None => entry,
        };
        let name = match reference.rsplit('/').next() {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => continue,
        };
        let relative = ["@rpath/", "@executable_path/", "@loader_path/"]
            .iter()
            .any(|prefix| reference.starts_with(prefix));
        let source = if relative {
            None
        } else if Path::new(reference).starts_with(target_dir)
            || (scope == DylibScope::NonSystem
                && reference.starts_with('/')
                && !is_system_library(reference))
        {
            Some(PathBuf::from(reference))
        } else {
            continue;
        };
        if out.iter().any(|d| d.reference == reference) {
            continue;
        }
        out.push(BundledDylib {
            reference: reference.to_string(),
            name,
            source,
        });
    }
    out
}

/// What the binary's reference to a bundled dylib becomes.
pub fn relinked_reference(name: &str) -> String {
    format!("@executable_path/../Frameworks/{name}")
}

/// Which libraries travel with the bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DylibScope {
    /// The build's own dylibs ([`plan_dylibs`]): enough on the machine that built it.
    Build,
    /// Every library that is not the system's (`/usr/lib`, `/System`): also Homebrew's and
    /// MacPorts' - what a bundle needs on a Mac that does not have them (`--portable`).
    NonSystem,
}

/// The file an `install_name_tool -change` edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelinkFile {
    /// `Contents/MacOS/<executable>`.
    Executable,
    /// `Contents/Frameworks/<name>`.
    Dylib(String),
}

/// One reference to rewrite: in `file`, `from` becomes `to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relink {
    pub file: RelinkFile,
    pub from: String,
    pub to: String,
}

/// Every dylib the bundle carries - the binary's, and theirs, and theirs -
/// and every reference to rewrite so each finds the bundled copy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DylibTree {
    pub dylibs: Vec<BundledDylib>,
    pub relinks: Vec<Relink>,
}

/// Walk the binary's dylibs and theirs, breadth first: `exe_otool_l` is
/// `otool -L` of the binary, `listing_of` answers `otool -L` of a planned
/// dylib (`None` when it cannot be read - it is then bundled without its own
/// dependencies). Each dylib is bundled once, however many others link it,
/// and a cycle ends at the first repeat. A dylib's listing names its own
/// install name too; that is no dependency (the bundle sets it with
/// `install_name_tool -id`).
pub fn plan_dylib_tree(
    exe_otool_l: &str,
    target_dir: &Path,
    scope: DylibScope,
    mut listing_of: impl FnMut(&BundledDylib) -> Option<String>,
) -> DylibTree {
    let mut tree = DylibTree::default();
    let mut queue: std::collections::VecDeque<(RelinkFile, Vec<BundledDylib>)> =
        std::collections::VecDeque::new();
    queue.push_back((
        RelinkFile::Executable,
        plan_dylibs_in(exe_otool_l, target_dir, scope),
    ));
    while let Some((file, dependencies)) = queue.pop_front() {
        for dependency in dependencies {
            tree.relinks.push(Relink {
                file: file.clone(),
                from: dependency.reference.clone(),
                to: relinked_reference(&dependency.name),
            });
            if tree.dylibs.iter().any(|d| d.name == dependency.name) {
                continue;
            }
            let own: Vec<BundledDylib> = listing_of(&dependency)
                .map(|listing| {
                    plan_dylibs_in(&listing, target_dir, scope)
                        .into_iter()
                        .filter(|d| d.name != dependency.name)
                        .collect()
                })
                .unwrap_or_default();
            queue.push_back((RelinkFile::Dylib(dependency.name.clone()), own));
            tree.dylibs.push(dependency);
        }
    }
    tree
}

// ────────── [package.metadata.bundle] ───────────────────────────────────

/// The quoted strings of `key` in the crate's `[package.metadata.bundle]`
/// table - the table `cargo-bundle` reads, so a crate set up for it needs
/// nothing more. The value is one string or an array, which may span lines.
///
/// A line scan, like [`package_version`]: azul-doc has no TOML parser.
fn bundle_metadata(cargo_toml: &str, key: &str) -> Vec<String> {
    let mut in_table = false;
    let mut collecting = false;
    let mut value = String::new();
    for line in cargo_toml.lines() {
        let t = line.trim();
        if collecting {
            value.push_str(t);
            if t.contains(']') {
                break;
            }
            continue;
        }
        if t.starts_with('[') {
            in_table = t == "[package.metadata.bundle]";
            continue;
        }
        if !in_table {
            continue;
        }
        // `icon = ...`, not `icons = ...` or `icon_x = ...` (any `key`).
        let Some(rest) = t.strip_prefix(key) else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim();
        value.push_str(rest);
        if rest.starts_with('[') && !rest.contains(']') {
            collecting = true;
            continue;
        }
        break;
    }
    // Every quoted string: the pieces between the 1st and 2nd quote, the
    // 3rd and 4th, ...
    value
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// `identifier` of `[package.metadata.bundle]`: the app's reverse-DNS id,
/// declared at build time. The app sets the same string as
/// `AppConfig::app_id`; `bundle macos` and `mobile build` default the bundle
/// id / package to it. `None` when it is absent or blank.
pub fn configured_identifier(cargo_toml: &str) -> Option<String> {
    bundle_metadata(cargo_toml, "identifier")
        .into_iter()
        .next()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
}

/// The default `CFBundleIdentifier`: the crate's configured identifier in
/// its Apple form (no `_`; the running app accepts that form as the same
/// id), else the id derived from the binary ([`bundle_id_for`]).
pub fn default_bundle_id(cargo_toml: Option<&str>, binary_name: &str) -> String {
    match cargo_toml.and_then(configured_identifier) {
        Some(id) => azul_layout::managers::notification::wire::AppIdentity::declared(
            &id,
            binary_name,
        )
        .apple_bundle_id(),
        None => bundle_id_for(binary_name),
    }
}

// ────────── The icon ───────────────────────────────────────────────────

/// The icon files a crate configures: `icon` of `[package.metadata.bundle]`
/// ([`bundle_metadata`]). A list of paths (PNGs of the sizes an `.icns`
/// holds, and/or an `.icns`) or one path, relative to the crate's directory.
pub fn configured_icons(cargo_toml: &str) -> Vec<String> {
    bundle_metadata(cargo_toml, "icon")
}

/// A PNG's pixel size `(width, height)`, from its `IHDR` chunk; `None` for
/// anything that is not a PNG.
pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if png.len() < 24 || png[..8] != SIGNATURE || &png[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    Some((width, height))
}

/// The ICNS element type that holds a PNG of this square size (the
/// PNG-encoded types macOS 10.7 and later read).
pub fn icns_type_for(size: u32) -> Option<&'static [u8; 4]> {
    Some(match size {
        16 => b"icp4",
        32 => b"icp5",
        64 => b"icp6",
        128 => b"ic07",
        256 => b"ic08",
        512 => b"ic09",
        1024 => b"ic10",
        _ => return None,
    })
}

/// An `.icns` holding these PNGs as they are, each under the element type
/// for its size ([`icns_type_for`]) - no image conversion, no `iconutil`.
/// A size it already holds is kept once (the first). `Err` names a PNG that
/// is not square or not a size an `.icns` holds, or says there is none.
///
/// The format: `icns` + the file's length, then per element its type, its
/// length (with its own 8-byte header) and its data; lengths big-endian.
pub fn icns_from_pngs(pngs: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    let mut elements: Vec<(&'static [u8; 4], &[u8])> = Vec::new();
    for (i, png) in pngs.iter().enumerate() {
        let number = i + 1;
        let (width, height) =
            png_size(png).ok_or_else(|| format!("icon {number} is not a PNG"))?;
        if width != height {
            return Err(format!(
                "icon {number} is {width}x{height} px: an .icns holds square icons"
            ));
        }
        let ty = icns_type_for(width).ok_or_else(|| {
            format!(
                "icon {number} is {width} px: an .icns holds 16, 32, 64, 128, 256, 512 or \
                 1024 px"
            )
        })?;
        if elements.iter().any(|(t, _)| *t == ty) {
            continue;
        }
        elements.push((ty, png.as_slice()));
    }
    if elements.is_empty() {
        return Err("there is no icon to put into the .icns".to_string());
    }
    let total = 8 + elements.iter().map(|(_, d)| 8 + d.len()).sum::<usize>();
    let total_u32 = u32::try_from(total)
        .map_err(|_| "the icons are larger than an .icns can hold".to_string())?;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"icns");
    out.extend_from_slice(&total_u32.to_be_bytes());
    for (ty, data) in elements {
        out.extend_from_slice(ty);
        out.extend_from_slice(&((8 + data.len()) as u32).to_be_bytes());
        out.extend_from_slice(data);
    }
    Ok(out)
}

/// The icon's file name in `Contents/Resources/` (`CFBundleIconFile`).
pub const ICON_FILE: &str = "AppIcon.icns";

/// LaunchServices registers a bundle under `/var/folders` (the per-user
/// temporary directory) but UN refuses it there.
pub fn is_refused_location(dir: &Path) -> bool {
    dir.starts_with("/var/folders") || dir.starts_with("/private/var/folders")
}

/// The layout of `<out_dir>/<app_name>.app`.
pub fn bundle_paths(out_dir: &Path, spec: &MacBundleSpec) -> BundlePaths {
    let app = out_dir.join(format!("{}.app", spec.app_name));
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    BundlePaths {
        executable: macos.join(&spec.executable),
        info_plist: contents.join("Info.plist"),
        frameworks: contents.join("Frameworks"),
        resources: contents.join("Resources"),
        macos,
        contents,
        app,
    }
}

// ────────── The command ────────────────────────────────────────────────

/// `lsregister`, which registers a bundle with LaunchServices - what Finder
/// does when it first sees one. Without it UN answers "Notifications are not
/// allowed for this application".
const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Versions/A/\
                          Frameworks/LaunchServices.framework/Versions/A/Support/lsregister";

fn print_usage() {
    println!("Usage:");
    println!("  azul-doc bundle macos <crate> [options]");
    println!();
    println!("  Wrap a cargo-built binary in a signed, registered .app, so macOS treats it as an");
    println!("  application (native notifications need that). Build the binary first.");
    println!();
    println!("  <crate>              a Cargo.toml, a directory with one, or an examples/ crate");
    println!("  --release | --debug  which build to bundle (default --release)");
    println!("  --profile <name>     a custom cargo profile's build");
    println!("  --bin <name>         the binary, if it is not named like the package");
    println!("  --exe <path>         bundle this binary instead of looking in target/");
    println!("  --bundle-id <id>     CFBundleIdentifier (default: `identifier` of");
    println!("                       [package.metadata.bundle] - the id the app sets as");
    println!("                       AppConfig::app_id - else com.azul.<binary>, the id an");
    println!("                       unnamed app gives itself unbundled and on Windows)");
    println!("  --name <name>        the app's name (default: the binary's)");
    println!("  --icon <path>        an .icns, or a square PNG of 16..1024 px (default: `icon` of");
    println!("                       [package.metadata.bundle] in the crate's Cargo.toml)");
    println!("  --portable           also bundle Homebrew / MacPorts libraries (every non-system");
    println!("                       one), for a Mac that does not have them");
    println!("  --out <dir>          where the .app goes (default ~/Applications)");
    println!("  --no-register        skip the LaunchServices registration");
    println!("  --dry-run            print the plan, write nothing");
    println!();
    println!("  The signature is ad hoc: fine on this Mac. Distribution needs a Developer ID");
    println!("  signature and notarization - see the module docs of doc/src/bundle.rs.");
}

#[derive(Debug, Default)]
struct BundleArgs {
    spec: Option<String>,
    profile: Option<String>,
    bin: Option<String>,
    exe: Option<PathBuf>,
    bundle_id: Option<String>,
    name: Option<String>,
    icon: Option<PathBuf>,
    out: Option<PathBuf>,
    portable: bool,
    no_register: bool,
    dry_run: bool,
}

fn parse_args(args: &[&str]) -> anyhow::Result<BundleArgs> {
    let mut a = BundleArgs::default();
    let mut i = 0;
    while i < args.len() {
        let value = |i: usize, flag: &str| -> anyhow::Result<String> {
            args.get(i + 1)
                .map(|v| (*v).to_string())
                .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
        };
        match args[i] {
            "--release" => a.profile = Some("release".to_string()),
            "--debug" => a.profile = Some("debug".to_string()),
            "--profile" => {
                let p = value(i, "--profile")?;
                // cargo's `dev` profile writes to target/debug.
                a.profile = Some(if p == "dev" { "debug".to_string() } else { p });
                i += 1;
            }
            "--bin" => {
                a.bin = Some(value(i, "--bin")?);
                i += 1;
            }
            "--exe" => {
                a.exe = Some(PathBuf::from(value(i, "--exe")?));
                i += 1;
            }
            "--bundle-id" => {
                a.bundle_id = Some(value(i, "--bundle-id")?);
                i += 1;
            }
            "--name" => {
                a.name = Some(value(i, "--name")?);
                i += 1;
            }
            "--icon" => {
                a.icon = Some(PathBuf::from(value(i, "--icon")?));
                i += 1;
            }
            "--out" => {
                a.out = Some(PathBuf::from(value(i, "--out")?));
                i += 1;
            }
            "--portable" => a.portable = true,
            "--no-register" => a.no_register = true,
            "--dry-run" | "-n" => a.dry_run = true,
            flag if flag.starts_with('-') => anyhow::bail!("unknown option {flag}"),
            positional => {
                if a.spec.is_some() {
                    anyhow::bail!("one crate at a time (got a second one: {positional})");
                }
                a.spec = Some(positional.to_string());
            }
        }
        i += 1;
    }
    Ok(a)
}

/// `azul-doc bundle ...`
pub fn handle_bundle_command(project_root: &Path, args: &[&str]) -> anyhow::Result<()> {
    match args {
        ["macos", rest @ ..] => bundle_macos(project_root, rest),
        _ => {
            print_usage();
            Ok(())
        }
    }
}

/// `CARGO_TARGET_DIR` (relative to the workspace), else `<workspace>/target`.
fn cargo_target_dir(workspace_root: &Path) -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            if dir.is_absolute() {
                dir
            } else {
                workspace_root.join(dir)
            }
        }
        None => workspace_root.join("target"),
    }
}

/// A path the user typed. `main()` has already changed into `doc/`, so a
/// relative one is tried as given and then against the checkout's root.
fn user_path(project_root: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() || p.exists() {
        return p.to_path_buf();
    }
    project_root.join(p)
}

fn run(cmd: &mut std::process::Command) -> anyhow::Result<()> {
    let shown = format!("{cmd:?}");
    let status = cmd
        .status()
        .map_err(|e| anyhow::anyhow!("could not start {shown}: {e}"))?;
    if !status.success() {
        anyhow::bail!("{shown} failed ({status})");
    }
    Ok(())
}

/// `otool -L <file>`.
fn otool_l(file: &Path) -> anyhow::Result<String> {
    let listing = std::process::Command::new("otool")
        .arg("-L")
        .arg(file)
        .output()
        .map_err(|e| anyhow::anyhow!("otool (Xcode command line tools) could not run: {e}"))?;
    if !listing.status.success() {
        anyhow::bail!("otool -L {} failed", file.display());
    }
    Ok(String::from_utf8_lossy(&listing.stdout).into_owned())
}

/// The file a planned dylib is copied from: its absolute reference, or - for
/// an `@rpath/` one - the build's output directory.
fn dylib_source(dylib: &BundledDylib, target_dir: &Path, profile: &str) -> Option<PathBuf> {
    dylib
        .source
        .clone()
        .or_else(|| {
            [
                target_dir.join(profile).join(&dylib.name),
                target_dir.join(profile).join("deps").join(&dylib.name),
            ]
            .into_iter()
            .find(|p| p.is_file())
        })
        .filter(|p| p.is_file())
}

/// A copied library keeps its mode, and Homebrew's are read-only (0444):
/// `install_name_tool` and `codesign` must write the copy.
#[cfg(unix)]
fn make_writable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(permissions.mode() | 0o200);
    std::fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn make_writable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// The bundle's `.icns`: an `.icns` among `sources` as it is, else the PNGs
/// wrapped into one ([`icns_from_pngs`]). Files that cannot be used are
/// named and skipped - an app without an icon still runs.
fn icon_bytes(sources: &[PathBuf]) -> Option<Vec<u8>> {
    fn is_icns(p: &Path) -> bool {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("icns"))
    }
    if let Some(icns) = sources.iter().find(|p| is_icns(p)) {
        match std::fs::read(icns) {
            Ok(bytes) => return Some(bytes),
            Err(e) => println!("[bundle] WARN icon {} not read: {e}", icns.display()),
        }
    }
    let mut pngs: Vec<Vec<u8>> = Vec::new();
    for path in sources.iter().filter(|p| !is_icns(p)) {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                println!("[bundle] WARN icon {} not read: {e}", path.display());
                continue;
            }
        };
        match png_size(&bytes) {
            Some((w, h)) if w == h && icns_type_for(w).is_some() => pngs.push(bytes),
            Some((w, h)) => println!(
                "[bundle] WARN icon {} is {w}x{h} px: an .icns holds square PNGs of 16, 32, 64, \
                 128, 256, 512 or 1024 px - skipped",
                path.display()
            ),
            None => println!(
                "[bundle] WARN icon {} is not a PNG (or an .icns) - skipped",
                path.display()
            ),
        }
    }
    if pngs.is_empty() {
        return None;
    }
    match icns_from_pngs(&pngs) {
        Ok(icns) => Some(icns),
        Err(e) => {
            println!("[bundle] WARN no icon: {e}");
            None
        }
    }
}

fn bundle_macos(project_root: &Path, args: &[&str]) -> anyhow::Result<()> {
    use std::{fs, process::Command};

    let a = parse_args(args)?;
    let Some(spec_arg) = a.spec.as_deref() else {
        print_usage();
        anyhow::bail!("which crate? e.g. azul-doc bundle macos azul-widgets");
    };
    if !cfg!(target_os = "macos") && !a.dry_run {
        anyhow::bail!(
            "bundle macos signs with codesign and registers with lsregister - run it on macOS \
             (--dry-run prints the plan anywhere)"
        );
    }

    let target = crate::mobile::run::Target::resolve(
        project_root,
        spec_arg,
        &crate::mobile::Opts::default(),
    )?;
    let manifest = target.manifest_dir.join("Cargo.toml");
    let manifest_text = fs::read_to_string(&manifest).ok();
    let version = manifest_text
        .as_deref()
        .and_then(package_version)
        .unwrap_or_else(|| "0.1.0".to_string());
    // `--icon`, else the crate's `[package.metadata.bundle] icon`.
    let icon_sources: Vec<PathBuf> = match &a.icon {
        Some(p) => vec![user_path(project_root, p)],
        None => manifest_text
            .as_deref()
            .map(configured_icons)
            .unwrap_or_default()
            .into_iter()
            .map(|p| target.manifest_dir.join(p))
            .collect(),
    };
    let icon = icon_bytes(&icon_sources);
    let target_dir = cargo_target_dir(&target.workspace_root);
    let profile = a.profile.clone().unwrap_or_else(|| "release".to_string());
    let bin = a.bin.clone().unwrap_or_else(|| target.crate_name.clone());
    let exe_src = match &a.exe {
        Some(p) => user_path(project_root, p),
        None => target_dir.join(&profile).join(&bin),
    };
    if !exe_src.is_file() {
        let flag = match profile.as_str() {
            "release" => "--release".to_string(),
            "debug" => String::new(),
            other => format!("--profile {other}"),
        };
        anyhow::bail!(
            "{} does not exist - build it first: cargo build {flag} -p {}",
            exe_src.display(),
            target.crate_name
        );
    }
    let executable = exe_src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("{} has no usable file name", exe_src.display()))?
        .to_string();
    let spec = MacBundleSpec {
        app_name: a.name.clone().unwrap_or_else(|| executable.clone()),
        // The crate's declared identifier, else from the BINARY's name, like
        // the running app derives its identity (desktop::app_identity) - not
        // from the package name.
        bundle_id: a
            .bundle_id
            .clone()
            .unwrap_or_else(|| default_bundle_id(manifest_text.as_deref(), &executable)),
        executable,
        version,
        icon_file: icon.as_ref().map(|_| ICON_FILE.to_string()),
    };
    let out_dir = match &a.out {
        Some(p) => user_path(project_root, p),
        None => PathBuf::from(
            std::env::var_os("HOME")
                .ok_or_else(|| anyhow::anyhow!("HOME is not set; pass --out <dir>"))?,
        )
        .join("Applications"),
    };
    if is_refused_location(&out_dir) {
        anyhow::bail!(
            "{} is under /var/folders: LaunchServices registers a bundle there, but the \
             notification center refuses it. Use ~/Applications (the default) or another \
             directory",
            out_dir.display()
        );
    }
    let paths = bundle_paths(&out_dir, &spec);

    // Which libraries travel with it: the binary's, and theirs, and theirs.
    let scope = if a.portable {
        DylibScope::NonSystem
    } else {
        DylibScope::Build
    };
    let tree = if cfg!(target_os = "macos") {
        let listing = otool_l(&exe_src)?;
        plan_dylib_tree(&listing, &target_dir, scope, |dylib| {
            dylib_source(dylib, &target_dir, &profile).and_then(|p| otool_l(&p).ok())
        })
    } else {
        DylibTree::default()
    };

    println!("[bundle] {} -> {}", exe_src.display(), paths.app.display());
    println!("[bundle]   CFBundleIdentifier {}", spec.bundle_id);
    if icon.is_some() {
        println!("[bundle]   icon -> Contents/Resources/{ICON_FILE}");
    } else if icon_sources.is_empty() {
        println!(
            "[bundle]   no icon (--icon <path>, or `icon` in [package.metadata.bundle] of {})",
            manifest.display()
        );
    }
    for dylib in &tree.dylibs {
        println!(
            "[bundle]   {} -> Contents/Frameworks/{}",
            dylib.reference, dylib.name
        );
    }
    if a.dry_run {
        println!("[bundle] dry run: nothing written");
        return Ok(());
    }

    // A fresh bundle every time: a stale dylib or plist from an earlier run
    // would be signed into this one.
    if paths.app.is_dir() {
        fs::remove_dir_all(&paths.app)?;
    }
    fs::create_dir_all(&paths.macos)?;
    fs::copy(&exe_src, &paths.executable)?;
    fs::write(&paths.info_plist, info_plist(&spec))?;
    fs::write(paths.contents.join("PkgInfo"), "APPL????")?;
    if let Some(icns) = &icon {
        fs::create_dir_all(&paths.resources)?;
        fs::write(paths.resources.join(ICON_FILE), icns)?;
    }

    // The dylibs, each under the install name it has in the bundle.
    let mut bundled: Vec<String> = Vec::new();
    for dylib in &tree.dylibs {
        let Some(source) = dylib_source(dylib, &target_dir, &profile) else {
            println!(
                "[bundle] WARN {} is not in the build output; left as {} (it must be found \
                 there at run time)",
                dylib.name, dylib.reference
            );
            continue;
        };
        fs::create_dir_all(&paths.frameworks)?;
        let dest = paths.frameworks.join(&dylib.name);
        fs::copy(&source, &dest)?;
        make_writable(&dest)?;
        if let Err(e) = run(Command::new("install_name_tool")
            .arg("-id")
            .arg(relinked_reference(&dylib.name))
            .arg(&dest))
        {
            println!("[bundle] WARN could not set the install name of {} ({e})", dylib.name);
        }
        bundled.push(dylib.name.clone());
    }

    // Every reference to a bundled dylib - the binary's and the dylibs'
    // own - pointed at the copy. Can fail when the new reference is longer
    // than the old one and the file has no header padding left; it then
    // keeps loading the original, which works on this machine.
    for relink in &tree.relinks {
        let target_name = relink.to.rsplit('/').next().unwrap_or("");
        if !bundled.iter().any(|name| name == target_name) {
            continue;
        }
        let file = match &relink.file {
            RelinkFile::Executable => paths.executable.clone(),
            RelinkFile::Dylib(name) if bundled.contains(name) => paths.frameworks.join(name),
            RelinkFile::Dylib(_) => continue,
        };
        if let Err(e) = run(Command::new("install_name_tool")
            .arg("-change")
            .arg(&relink.from)
            .arg(&relink.to)
            .arg(&file))
        {
            println!(
                "[bundle] WARN could not point {} at the bundled {target_name} ({e}); it keeps \
                 loading {}",
                file.display(),
                relink.from
            );
        }
    }

    // Ad-hoc signatures, inside out: every dylib, then the bundle (which
    // signs the main executable). Never --deep (deprecated for signing).
    for name in &bundled {
        run(Command::new("codesign")
            .args(["--force", "--sign", "-"])
            .arg(paths.frameworks.join(name)))?;
    }
    run(Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(&paths.app))?;

    if a.no_register {
        println!("[bundle] not registered (--no-register): open it once from Finder first");
    } else if let Err(e) = run(Command::new(LSREGISTER).arg("-f").arg(&paths.app)) {
        println!(
            "[bundle] WARN LaunchServices registration failed ({e}); opening the app once \
             from Finder registers it too"
        );
    }

    println!("[bundle] done: {}", paths.app.display());
    println!("[bundle]   run it:  open \"{}\"", paths.app.display());
    println!(
        "[bundle]   or:      \"{}\"   (logs stay in this terminal)",
        paths.executable.display()
    );
    println!(
        "[bundle]   a stale 'Don't Allow' is kept per bundle id: tccutil reset UserNotification {}",
        spec.bundle_id
    );
    Ok(())
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
            icon_file: None,
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

    // ---- the dylibs' own dependencies ----

    const TARGET: &str = "/Users/me/azul/target";

    /// `otool -L <file>` output listing `deps`.
    fn listing(file: &str, deps: &[&str]) -> String {
        let mut out = format!("{file}:\n");
        for dep in deps {
            out.push_str(&format!(
                "\t{dep} (compatibility version 1.0.0, current version 1.0.0)\n"
            ));
        }
        out
    }

    #[test]
    fn a_bundled_dylibs_own_dependencies_are_bundled_and_relinked_too() {
        let azul = "/Users/me/azul/target/release/build/azul-dll-a0e8/out/libazul.dylib";
        let exe = listing(
            "/Users/me/azul/target/release/AzWidgets",
            &[azul, "/usr/lib/libSystem.B.dylib"],
        );
        let mut asked: Vec<String> = Vec::new();
        let tree = plan_dylib_tree(&exe, Path::new(TARGET), DylibScope::Build, |d| {
            asked.push(d.name.clone());
            match d.name.as_str() {
                // `otool -L` on a dylib lists its own install name first.
                "libazul.dylib" => Some(listing(
                    azul,
                    &[azul, "@rpath/libextra.dylib", "/usr/lib/libSystem.B.dylib"],
                )),
                "libextra.dylib" => Some(listing("libextra.dylib", &["@rpath/libextra.dylib"])),
                _ => None,
            }
        });
        let names: Vec<&str> = tree.dylibs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["libazul.dylib", "libextra.dylib"]);
        assert_eq!(
            asked,
            vec!["libazul.dylib", "libextra.dylib"],
            "each dylib's own listing is read once"
        );
        assert_eq!(
            tree.relinks,
            vec![
                Relink {
                    file: RelinkFile::Executable,
                    from: azul.to_string(),
                    to: "@executable_path/../Frameworks/libazul.dylib".to_string(),
                },
                Relink {
                    file: RelinkFile::Dylib("libazul.dylib".to_string()),
                    from: "@rpath/libextra.dylib".to_string(),
                    to: "@executable_path/../Frameworks/libextra.dylib".to_string(),
                },
            ],
            "a dylib's own install name is no dependency to relink (install_name_tool -id sets it)"
        );
    }

    #[test]
    fn a_library_two_dylibs_share_is_bundled_once_and_a_cycle_ends() {
        let exe = listing("app", &["@rpath/liba.dylib", "@rpath/libb.dylib"]);
        let tree = plan_dylib_tree(&exe, Path::new(TARGET), DylibScope::Build, |d| {
            match d.name.as_str() {
                "liba.dylib" => Some(listing(
                    "liba.dylib",
                    &["@rpath/liba.dylib", "@rpath/libc.dylib"],
                )),
                "libb.dylib" => Some(listing("libb.dylib", &["@rpath/libc.dylib"])),
                "libc.dylib" => Some(listing("libc.dylib", &["@rpath/liba.dylib"])),
                _ => None,
            }
        });
        let names: Vec<&str> = tree.dylibs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["liba.dylib", "libb.dylib", "libc.dylib"]);
        let into_c = tree
            .relinks
            .iter()
            .filter(|r| r.to.ends_with("/libc.dylib"))
            .count();
        assert_eq!(
            into_c, 2,
            "both users of libc point at the one copy: {:?}",
            tree.relinks
        );
        assert!(
            tree.relinks.contains(&Relink {
                file: RelinkFile::Dylib("libc.dylib".to_string()),
                from: "@rpath/liba.dylib".to_string(),
                to: relinked_reference("liba.dylib"),
            }),
            "the reference back to liba is relinked, and the walk ends: {:?}",
            tree.relinks
        );
    }

    #[test]
    fn a_portable_bundle_also_carries_homebrews_libraries_but_never_the_systems() {
        let png = "/opt/homebrew/opt/libpng/lib/libpng16.16.dylib";
        let exe = listing(
            "app",
            &[
                png,
                "/usr/lib/libSystem.B.dylib",
                "/System/Library/Frameworks/AppKit.framework/Versions/C/AppKit",
            ],
        );
        let listing_of = |d: &BundledDylib| match d.name.as_str() {
            "libpng16.16.dylib" => Some(listing(
                png,
                &[
                    png,
                    "/opt/homebrew/opt/zlib/lib/libz.1.dylib",
                    "/usr/lib/libSystem.B.dylib",
                ],
            )),
            _ => Some(listing("x", &[])),
        };
        let portable = plan_dylib_tree(&exe, Path::new(TARGET), DylibScope::NonSystem, listing_of);
        let names: Vec<&str> = portable.dylibs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["libpng16.16.dylib", "libz.1.dylib"]);
        assert_eq!(
            portable.dylibs[1].source,
            Some(PathBuf::from("/opt/homebrew/opt/zlib/lib/libz.1.dylib"))
        );
        let build_only = plan_dylib_tree(&exe, Path::new(TARGET), DylibScope::Build, listing_of);
        assert!(
            build_only.dylibs.is_empty(),
            "the default bundles only the build's own: {build_only:?}"
        );
    }

    // ---- the icon ----

    /// The first bytes of a `w` x `h` PNG: the signature and the IHDR chunk
    /// (all `png_size` reads), and a stand-in for the rest.
    fn fake_png(w: u32, h: u32) -> Vec<u8> {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&w.to_be_bytes());
        png.extend_from_slice(&h.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0]);
        png.extend_from_slice(&[0, 0, 0, 0]);
        png.extend_from_slice(b"the-rest-of-the-file");
        png
    }

    #[test]
    fn the_configured_icons_are_read_from_the_bundle_metadata() {
        let toml = "[package]\nname = \"x\"\n\n[package.metadata.bundle]\nidentifier = \
                    \"com.x\"\nicon = [\"icons/32x32.png\", \"icons/icon.icns\"]\n\n\
                    [dependencies]\nicon = \"9\"\n";
        assert_eq!(
            configured_icons(toml),
            vec!["icons/32x32.png", "icons/icon.icns"]
        );
        let multi_line = "[package.metadata.bundle]\nicon = [\n    \"a.png\",\n    \
                          \"b@2x.png\",\n]\nname = \"X\"\n";
        assert_eq!(configured_icons(multi_line), vec!["a.png", "b@2x.png"]);
        let single = "[package.metadata.bundle]\nicon = \"app.icns\"\n";
        assert_eq!(configured_icons(single), vec!["app.icns"]);
        assert!(configured_icons("[package]\nname = \"x\"\n").is_empty());
        let elsewhere = "[package.metadata.other]\nicon = [\"no.png\"]\n";
        assert!(configured_icons(elsewhere).is_empty());
    }

    #[test]
    fn the_bundle_identifier_is_read_from_the_bundle_metadata() {
        // The build-time declaration of the id the app sets as
        // `AppConfig::app_id` - the `cargo-bundle` key, next to the icon.
        let toml = "[package]\nname = \"editor\"\n\n[package.metadata.bundle]\n\
                    identifier = \"org.example.Editor\"\nicon = [\"icons/128.png\"]\n\n\
                    [dependencies]\nidentifier = \"9\"\n";
        assert_eq!(
            configured_identifier(toml).as_deref(),
            Some("org.example.Editor")
        );
        assert_eq!(
            configured_icons(toml),
            vec!["icons/128.png"],
            "the same table, another key"
        );
        assert_eq!(configured_identifier("[package]\nname = \"x\"\n"), None);
        let elsewhere = "[package.metadata.other]\nidentifier = \"no.such.App\"\n";
        assert_eq!(configured_identifier(elsewhere), None);
        let another_key = "[package.metadata.bundle]\nidentifiers = \"x.y\"\n";
        assert_eq!(configured_identifier(another_key), None);
        let blank = "[package.metadata.bundle]\nidentifier = \"  \"\n";
        assert_eq!(configured_identifier(blank), None, "a blank id is none");
    }

    #[test]
    fn the_configured_identifier_is_the_default_bundle_id() {
        let toml = "[package.metadata.bundle]\nidentifier = \"org.example.Editor\"\n";
        assert_eq!(
            default_bundle_id(Some(toml), "Editor"),
            "org.example.Editor"
        );
        let underscore = "[package.metadata.bundle]\nidentifier = \"org.example.my_app\"\n";
        assert_eq!(
            default_bundle_id(Some(underscore), "my_app"),
            "org.example.my-app",
            "its Apple form: a CFBundleIdentifier has no underscore"
        );
        assert_eq!(
            default_bundle_id(Some("[package]\nname = \"x\"\n"), "AzWidgets"),
            "com.azul.azwidgets",
            "no identifier: the id derived from the binary, as before"
        );
        assert_eq!(default_bundle_id(None, "AzWidgets"), "com.azul.azwidgets");
    }

    #[test]
    fn a_png_is_read_for_its_size_and_only_a_png_is() {
        assert_eq!(png_size(&fake_png(128, 128)), Some((128, 128)));
        assert_eq!(png_size(&fake_png(512, 256)), Some((512, 256)));
        assert_eq!(png_size(b"GIF89a, not a PNG at all, long enough"), None);
        assert_eq!(png_size(&fake_png(16, 16)[..20]), None, "cut before the height");
    }

    #[test]
    fn each_square_size_an_icns_holds_has_its_element_type() {
        for (size, ty) in [
            (16, b"icp4"),
            (32, b"icp5"),
            (64, b"icp6"),
            (128, b"ic07"),
            (256, b"ic08"),
            (512, b"ic09"),
            (1024, b"ic10"),
        ] {
            assert_eq!(icns_type_for(size), Some(ty), "{size}");
        }
        assert_eq!(icns_type_for(100), None);
        assert_eq!(icns_type_for(2048), None);
    }

    #[test]
    fn a_png_is_wrapped_into_an_icns_as_it_is() {
        let png = fake_png(128, 128);
        let icns = icns_from_pngs(&[png.clone()]).expect("a 128 px PNG has an element type");
        assert_eq!(&icns[0..4], b"icns");
        assert_eq!(
            u32::from_be_bytes([icns[4], icns[5], icns[6], icns[7]]) as usize,
            icns.len(),
            "the header counts the whole file"
        );
        assert_eq!(&icns[8..12], b"ic07");
        assert_eq!(
            u32::from_be_bytes([icns[12], icns[13], icns[14], icns[15]]) as usize,
            8 + png.len(),
            "an element counts its own header"
        );
        assert_eq!(
            &icns[16..],
            &png[..],
            "macOS 10.7+ reads a PNG element's bytes as they are"
        );
    }

    #[test]
    fn several_pngs_share_one_icns_and_a_size_it_has_no_type_for_is_refused() {
        let small = fake_png(16, 16);
        let large = fake_png(1024, 1024);
        let icns = icns_from_pngs(&[small.clone(), large.clone(), fake_png(16, 16)])
            .expect("two sizes");
        assert_eq!(
            icns.len(),
            8 + (8 + small.len()) + (8 + large.len()),
            "the second 16 px PNG is dropped: one element per type"
        );
        assert_eq!(&icns[8..12], b"icp4");
        assert_eq!(&icns[16 + small.len()..20 + small.len()], b"ic10");
        assert!(icns_from_pngs(&[fake_png(128, 64)]).is_err(), "not square");
        assert!(
            icns_from_pngs(&[fake_png(100, 100)]).is_err(),
            "no element type"
        );
        assert!(icns_from_pngs(&[b"not a png".to_vec()]).is_err());
        assert!(icns_from_pngs(&[]).is_err(), "an icns with no icon");
    }

    #[test]
    fn the_info_plist_names_the_icon_only_when_the_bundle_has_one() {
        assert_eq!(plist_value(&info_plist(&spec()), "CFBundleIconFile"), None);
        let mut with_icon = spec();
        with_icon.icon_file = Some("AppIcon.icns".to_string());
        assert_eq!(
            plist_value(&info_plist(&with_icon), "CFBundleIconFile").as_deref(),
            Some("AppIcon.icns")
        );
    }
}
