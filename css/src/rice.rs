//! The end user's stylesheets: the "rice".
//!
//! Design: `scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md`
//! (sections 4.1, 4.4, 7, 9, 9.2; pitfalls 7, 8, 12).
//!
//! # Where the files are
//!
//! The rice root is `~/.azul` (`%USERPROFILE%\.azul` on Windows). For every
//! entry `n` of the app's THEME CHAIN (most specific first, the app's default
//! theme last), the loader reads `<root>/css/<n>/*.css`, with `:` as a path
//! segment because `:` is illegal in NTFS file names: `xyz:pink` is
//! `css/xyz/pink/`. Files directly in `<root>/css/` are GLOBAL: their header's
//! `theme:` names the theme they belong to (`css/foo.css` with
//! `theme: abc-base` IS the base theme `abc-base`), and without one they are
//! live under every theme. The legacy per-app file
//! (`~/.config/azul/styles/<app>.css`; `%APPDATA%\azul\styles\` on Windows,
//! `~/Library/Application Support/azul/styles/` on macOS) keeps loading, as a
//! per-app, unthemed file.
//!
//! [`chain_directories`] is the one walk from a chain to its directories; the
//! icon remap loader walks `<root>/icons/<n>/` with it.
//!
//! # The header
//!
//! The first comment of a file, `// ...` lines or one `/* ... */` block:
//!
//! ```css
//! // theme: abc-base@1.4.0; priority: widgets; fallback: native; app: azwriter; azul: 0.2.*; requires: abc@^1.2
//! ```
//!
//! | key | meaning |
//! |---|---|
//! | `theme` | the theme (and `@version`) the file belongs to; implied by its directory |
//! | `priority` | `base` (default), `palette`, `app`, `widgets`, `force` or `off` |
//! | `fallback` | the themes below this one in the chain ([`fallback_of`]) |
//! | `app` | the file is inert for every other application |
//! | `azul` | the ONE gate: azul versions the file applies to; no match, the file is ignored |
//! | `requires` | theme-to-theme dependency, caret ranges; never gates, warns and attributes |
//!
//! Every rule of a file is wrapped in `@theme(<its theme>)` (so the chain's
//! rank applies to it) and stamped with its priority's slot
//! ([`RicePriority::rule_priority`]).
//!
//! # Precedence, hardening, attribution
//!
//! A per-app file (`app:` names this app, or the legacy file) beats the
//! global files of its theme both ways: its rules come after theirs, and an
//! explicit `priority:` in it replaces theirs for this app (`base` or `off`
//! turns a global `widgets` theme down; `widgets` turns a global `base` up).
//!
//! Rice files are untrusted input: a remote `url()` drops its declaration, an
//! `@import` is dropped, a file above [`MAX_RICE_FILE_BYTES`] is refused, and
//! a theme name or symlink that would leave the rice root is refused.
//!
//! [`RiceStatus`] lists the chain, every file with its priority, version,
//! `requires` check and live versus inert rules - the About panel and the
//! `AZ_DEBUG` log print it. `AZ_RICING=off` loads nothing: the self-check a
//! bug report states.

use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use std::path::{Path, PathBuf};

pub use crate::system::{ricing_mode, ricing_mode_from, RicingMode};
use crate::{
    corety::{AzString, StringVec},
    css::{rule_priority, Css, CssRuleBlock},
    dynamic_selector::{
        DynamicSelector, DynamicSelectorContext, ThemeCondition, DEFAULT_APP_THEME,
    },
    system::SystemStyle,
};

/// The largest rice file the loader reads; a larger one is refused.
pub const MAX_RICE_FILE_BYTES: u64 = 1024 * 1024;

/// At most this many `.css` files are read from one directory (sorted by
/// name; the rest are ignored).
pub const MAX_RICE_FILES_PER_DIR: usize = 256;

/// Theme names that are the COLOUR SCHEME, never a theme (pitfall 5): a
/// directory or `theme:` of this name is refused.
pub const RESERVED_THEME_NAMES: &[&str] = &["light", "dark"];

/// The line every rice report ends with: the support policy and the
/// self-check (pitfall 12, gap 5).
pub const RICE_SUPPORT_POLICY: &str = "A rice above `base` restyles this app beyond what it \
     was tested with: report visual issues to the theme's author, not the app's. Run with \
     AZ_RICING=off to check: a bug that reproduces without the rice is the app's.";

// ---------------------------------------------------------------------------------------------
// priorities
// ---------------------------------------------------------------------------------------------

/// The `priority:` of a rice file: which cascade slot its rules take.
///
/// Ordered from "does least" to "does most" (`Off < Palette < Base < App <
/// Widgets < Force`); the cascade slots are [`Self::rule_priority`].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum RicePriority {
    /// The file is inert. In a per-app file: the global files of its theme
    /// are off for this app too.
    Off,
    /// Custom properties only (`--name: value`), at the widgets slot. A
    /// header-less file of only custom properties is a palette by
    /// inspection; a declared palette that sets anything else is demoted to
    /// `Base`.
    Palette,
    /// The default: fills what nobody declared, cannot break the app
    /// (`rule_priority::SYSTEM`).
    #[default]
    Base,
    /// Re-skins the app's own DOM, widgets untouched (`rule_priority::APP`).
    App,
    /// A full theme, above the widgets' inline declarations
    /// (`rule_priority::WIDGETS`).
    Widgets,
    /// Above everything, the app's runtime overrides included
    /// (`rule_priority::FORCE`). Unsupported territory.
    Force,
}

impl RicePriority {
    /// The priority a header names (`base`, `palette`, `app`, `widgets`,
    /// `force`, `off`; case-insensitive).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "palette" => Some(Self::Palette),
            "base" => Some(Self::Base),
            "app" => Some(Self::App),
            "widgets" => Some(Self::Widgets),
            "force" => Some(Self::Force),
            _ => None,
        }
    }

    /// The header spelling of this priority.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Palette => "palette",
            Self::Base => "base",
            Self::App => "app",
            Self::Widgets => "widgets",
            Self::Force => "force",
        }
    }

    /// The `CssRuleBlock::priority` a file of this priority stamps on its
    /// rules; `None` for `Off`.
    #[must_use]
    pub const fn rule_priority(self) -> Option<u8> {
        match self {
            Self::Off => None,
            Self::Palette => Some(rule_priority::PALETTE),
            Self::Base => Some(rule_priority::SYSTEM),
            Self::App => Some(rule_priority::APP),
            Self::Widgets => Some(rule_priority::WIDGETS),
            Self::Force => Some(rule_priority::FORCE),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// the header
// ---------------------------------------------------------------------------------------------

/// One `requires:` entry: `abc-base@^1.2`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RiceRequirement {
    /// The theme required.
    pub theme: String,
    /// Its version range (Cargo-style: `^1.2`, `~1.2`, `=1.2.3`, `1.*`);
    /// `*` when the entry named no version.
    pub range: String,
}

/// A rice file's header meta-comment, parsed ([`parse_rice_header`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RiceHeader {
    /// `theme:` without its version.
    pub theme: Option<String>,
    /// The `@version` of `theme:`.
    pub version: Option<String>,
    /// `priority:`; `None` when absent (base, or palette by inspection).
    pub priority: Option<RicePriority>,
    /// `fallback:`, in order.
    pub fallback: Vec<String>,
    /// `app:` - the applications the file is for; empty: every app.
    pub apps: Vec<String>,
    /// `azul:` - the azul versions the file applies to; empty: every version.
    pub azul: Vec<String>,
    /// `requires:`.
    pub requires: Vec<RiceRequirement>,
    /// What the header got wrong: an unknown key or priority.
    pub warnings: Vec<String>,
}

const HEADER_KEYS: &str = "theme, priority, fallback, app, azul, requires";

/// The text of a source's FIRST comment, if the source starts with one: the
/// consecutive `//` lines, or the one `/* ... */` block.
fn header_comment(source: &str) -> Option<String> {
    let s = source.trim_start_matches('\u{feff}').trim_start();
    if let Some(rest) = s.strip_prefix("/*") {
        let end = rest.find("*/").unwrap_or(rest.len());
        return Some(rest[..end].to_string());
    }
    if !s.starts_with("//") {
        return None;
    }
    let mut out = String::new();
    for line in s.lines() {
        let Some(body) = line.trim_start().strip_prefix("//") else {
            break;
        };
        out.push_str(body.trim_start_matches('/'));
        out.push('\n');
    }
    Some(out)
}

fn split_list(value: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in value.split(',') {
        let item = item.trim();
        if !item.is_empty() && !out.iter().any(|o| o == item) {
            out.push(item.to_string());
        }
    }
    out
}

/// `name@version` into its halves.
fn split_version(value: &str) -> (&str, Option<&str>) {
    match value.split_once('@') {
        Some((name, version)) => (name.trim(), Some(version.trim())),
        None => (value.trim(), None),
    }
}

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// Parse the header meta-comment of a rice file: its first comment, `key:
/// value` items separated by `;` or line breaks.
///
/// Free text in the comment is ignored. A key the loader does not know is a
/// warning only when the comment has at least one key it does know (so an
/// ordinary leading comment stays silent, and a typo in a real header does
/// not).
#[must_use]
pub fn parse_rice_header(source: &str) -> RiceHeader {
    let mut header = RiceHeader::default();
    let Some(text) = header_comment(source) else {
        return header;
    };
    let mut unknown: Vec<String> = Vec::new();
    let mut known = false;
    for item in text.split([';', '\n']) {
        let item = item.trim().trim_start_matches('*').trim();
        let Some((key, value)) = item.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "theme" => {
                known = true;
                let (name, version) = split_version(value);
                header.theme = non_empty(name);
                header.version = version.and_then(non_empty);
            }
            "priority" => {
                known = true;
                match RicePriority::from_name(value) {
                    Some(p) => header.priority = Some(p),
                    None => header.warnings.push(format!(
                        "unknown priority `{value}` (base, palette, app, widgets, force or off)"
                    )),
                }
            }
            "fallback" => {
                known = true;
                header.fallback = split_list(value);
            }
            "app" => {
                known = true;
                header.apps = split_list(value);
            }
            "azul" => {
                known = true;
                header.azul = split_list(value);
            }
            "requires" => {
                known = true;
                header.requires = split_list(value)
                    .iter()
                    .map(|entry| {
                        let (theme, range) = split_version(entry);
                        RiceRequirement {
                            theme: theme.to_string(),
                            range: range.filter(|r| !r.is_empty()).unwrap_or("*").to_string(),
                        }
                    })
                    .collect();
            }
            other => {
                if !other.is_empty() && !other.contains(char::is_whitespace) {
                    unknown.push(other.to_string());
                }
            }
        }
    }
    if known {
        for key in unknown {
            header
                .warnings
                .push(format!("unknown header key `{key}` ({HEADER_KEYS})"));
        }
    }
    header
}

// ---------------------------------------------------------------------------------------------
// versions
// ---------------------------------------------------------------------------------------------

/// A version component without its pre-release / build suffix (`0-beta` is `0`).
fn numeric_part(component: &str) -> &str {
    component
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .trim()
}

/// Up to three numeric components of `v` (`v1.2`, `1.2.3-beta`); `None` if
/// one is not a number.
fn version_parts(v: &str) -> Option<Vec<u64>> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    if v.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for component in v.split('.').take(3) {
        parts.push(numeric_part(component).parse::<u64>().ok()?);
    }
    Some(parts)
}

fn full_version(parts: &[u64]) -> [u64; 3] {
    [
        parts.first().copied().unwrap_or(0),
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
    ]
}

/// One `azul:` / wildcard pattern (`0.2.*`, `0.2`, `0.2.0`, `*`) against a
/// version: component by component, `*` (or `x`) matches the rest, a
/// shorter pattern is a prefix.
fn wildcard_matches(pattern: &str, version: &str) -> bool {
    let pattern = pattern
        .trim()
        .trim_start_matches(['=', 'v', 'V'])
        .trim();
    if pattern.is_empty() {
        return false;
    }
    let actual: Vec<&str> = version.trim().split('.').collect();
    for (i, component) in pattern.split('.').enumerate() {
        let component = component.trim();
        if component == "*" || component.eq_ignore_ascii_case("x") {
            return true;
        }
        let have = actual.get(i).map_or("0", |a| numeric_part(a));
        match (numeric_part(component).parse::<u64>(), have.parse::<u64>()) {
            (Ok(want), Ok(have)) if want == have => {}
            _ => return false,
        }
    }
    true
}

/// Does the running azul `running` match the `azul:` list `patterns`? An
/// OR list; empty (the key is absent) matches everywhere. Each entry is a
/// version with `*` as wildcard (`0.2.*`), or a plain version that matches
/// as a prefix (`0.2` is every `0.2.x`).
#[must_use]
pub fn azul_version_matches(patterns: &[String], running: &str) -> bool {
    patterns.is_empty() || patterns.iter().any(|p| wildcard_matches(p, running))
}

/// Does `version` satisfy the Cargo-style `range`? `^1.2` (and a bare
/// `1.2`): same left-most non-zero component; `~1.2`: same major.minor;
/// `=1.2.3`, `>=`, `>`, `<=`, `<`; `1.*` wildcards; `*` or empty: any.
#[must_use]
pub fn version_satisfies(range: &str, version: &str) -> bool {
    let Some(have) = version_parts(version) else {
        return false;
    };
    let have = full_version(&have);
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return true;
    }
    if range.contains('*') {
        return wildcard_matches(range, version);
    }
    let (op, rest) = [">=", "<=", ">", "<", "=", "~", "^"]
        .iter()
        .find_map(|op| range.strip_prefix(*op).map(|rest| (*op, rest)))
        .unwrap_or(("^", range));
    let Some(given) = version_parts(rest) else {
        return false;
    };
    let base = full_version(&given);
    match op {
        ">=" => have >= base,
        "<=" => have <= base,
        ">" => have > base,
        "<" => have < base,
        "=" => given.iter().enumerate().all(|(i, g)| have[i] == *g),
        "~" => have >= base && have[0] == base[0] && (given.len() < 2 || have[1] == base[1]),
        _ => {
            // Caret: the left-most non-zero GIVEN component may not change.
            if have < base {
                return false;
            }
            let pivot = given
                .iter()
                .position(|&c| c != 0)
                .unwrap_or(given.len().saturating_sub(1));
            let mut upper = base;
            upper[pivot] = upper[pivot].saturating_add(1);
            for slot in upper.iter_mut().skip(pivot + 1) {
                *slot = 0;
            }
            have < upper
        }
    }
}

// ---------------------------------------------------------------------------------------------
// hardening
// ---------------------------------------------------------------------------------------------

/// A rice source after [`sanitize_rice_source`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SanitizedRice {
    /// The source the CSS parser gets.
    pub source: String,
    /// What was dropped, one line each.
    pub warnings: Vec<String>,
}

/// Overwrite `bytes[start..end]` with spaces, keeping line breaks (so a
/// parse warning's line number still points at the right line).
fn blank(bytes: &mut [u8], start: usize, end: usize) {
    let end = end.min(bytes.len());
    for b in bytes.iter_mut().take(end).skip(start) {
        if *b != b'\n' {
            *b = b' ';
        }
    }
}

/// Blank every `/* ... */` comment, and every line whose first non-blank
/// characters are `//` (CSS has no line comments; the header uses them, and
/// the parser does not know them).
fn blank_comments(source: &str) -> Vec<u8> {
    let mut bytes = source.as_bytes().to_vec();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'/' && bytes[i + 1] == b'*' {
            let end = source[i + 2..]
                .find("*/")
                .map_or(bytes.len(), |rel| i + 2 + rel + 2);
            blank(&mut bytes, i, end);
            i = end;
        } else {
            i += 1;
        }
    }
    let mut line_start = 0;
    while line_start < bytes.len() {
        let line_end = bytes[line_start..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(bytes.len(), |rel| line_start + rel);
        let first = bytes[line_start..line_end]
            .iter()
            .position(|b| !b.is_ascii_whitespace())
            .map(|rel| line_start + rel);
        if let Some(first) = first {
            if bytes.get(first) == Some(&b'/') && bytes.get(first + 1) == Some(&b'/') {
                blank(&mut bytes, first, line_end);
            }
        }
        line_start = line_end + 1;
    }
    bytes
}

/// Is `url` a network fetch? A protocol-relative `//host` or a UNC `\\host`,
/// any scheme but `data:`, and `file:` with a host.
fn is_remote_url(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    if u.starts_with("//") || u.starts_with("\\\\") {
        return true;
    }
    let Some((scheme, rest)) = u.split_once(':') else {
        return false;
    };
    let is_scheme = scheme.len() > 1
        && scheme.as_bytes()[0].is_ascii_alphabetic()
        && scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.');
    if !is_scheme {
        // `C:\x.png`: a drive letter, not a scheme.
        return false;
    }
    match scheme {
        "data" => false,
        "file" => rest.starts_with("//") && !rest.starts_with("///"),
        _ => true,
    }
}

/// The index of the `)` closing a `url(` whose argument starts at `from`
/// (a quoted argument may contain `)`), or the end of `text`.
fn url_close(text: &[u8], from: usize) -> usize {
    let mut i = from;
    while i < text.len() && text[i].is_ascii_whitespace() {
        i += 1;
    }
    if let Some(&quote) = text.get(i).filter(|q| **q == b'"' || **q == b'\'') {
        i += 1;
        while i < text.len() && text[i] != quote {
            i += 1;
        }
        i += 1;
    }
    while i < text.len() && text[i] != b')' {
        i += 1;
    }
    i.min(text.len())
}

/// The byte range of the declaration around `at`: from after the previous
/// `{`, `}` or `;` to the next `;` (included) or up to the next `}`.
fn declaration_around(text: &[u8], at: usize, after: usize) -> (usize, usize) {
    let start = text[..at]
        .iter()
        .rposition(|&b| b == b'{' || b == b'}' || b == b';')
        .map_or(0, |p| p + 1);
    let mut end = after.min(text.len());
    while end < text.len() {
        match text[end] {
            b';' => return (start, end + 1),
            b'}' => return (start, end),
            _ => end += 1,
        }
    }
    (start, text.len())
}

/// Blank every `@import` statement (to its `;`).
fn drop_imports(bytes: &mut [u8], warnings: &mut Vec<String>) {
    let lower = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower.get(from..).and_then(|s| s.find("@import")) {
        let at = from + rel;
        let end = lower[at..]
            .find(';')
            .map_or(lower.len(), |rel| at + rel + 1);
        let statement = String::from_utf8_lossy(&bytes[at..end]).trim().to_string();
        blank(bytes, at, end);
        warnings.push(format!(
            "dropped `{statement}`: a rice file cannot load other files"
        ));
        from = end;
    }
}

/// Blank every declaration whose `url()` is a network fetch.
fn drop_remote_urls(bytes: &mut [u8], warnings: &mut Vec<String>) {
    let lower = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower.get(from..).and_then(|s| s.find("url(")) {
        let at = from + rel;
        let arg_start = at + 4;
        let close = url_close(lower.as_bytes(), arg_start);
        let arg = String::from_utf8_lossy(&bytes[arg_start..close])
            .trim()
            .trim_matches(|c: char| c == '"' || c == '\'')
            .trim()
            .to_string();
        if is_remote_url(&arg) {
            let (start, end) = declaration_around(bytes, at, close);
            blank(bytes, start, end);
            warnings.push(format!(
                "dropped a declaration with `url({arg})`: a rice file may not fetch from the \
                 network"
            ));
            from = end.max(close);
        } else {
            from = close.max(arg_start);
        }
        if from >= lower.len() {
            break;
        }
    }
}

/// Make a rice source safe to parse: comments blanked (`//` lines included,
/// which CSS does not have), `@import` statements dropped, and every
/// declaration with a REMOTE `url()` dropped (a rice cannot make the app
/// fetch from the network, pitfall 8). A relative, `file:///` or `data:`
/// url stays. One warning per drop.
#[must_use]
pub fn sanitize_rice_source(source: &str) -> SanitizedRice {
    let mut bytes = blank_comments(source);
    let mut warnings = Vec::new();
    drop_imports(&mut bytes, &mut warnings);
    drop_remote_urls(&mut bytes, &mut warnings);
    // Every blanked range starts and ends at an ASCII delimiter or covers
    // whole characters, so this is valid UTF-8. Fail CLOSED if it is not:
    // handing back the original would hand back the remote url.
    let source = String::from_utf8(bytes).unwrap_or_default();
    SanitizedRice { source, warnings }
}

/// Does the (sanitized) source declare custom properties and nothing else?
/// A declaration is a `key: value` segment ended by `;` or `}` inside a
/// block; a segment ended by `{` is a selector or an at-rule prelude.
fn is_palette_source(source: &str) -> bool {
    let mut declarations = 0usize;
    let mut depth = 0usize;
    let mut segment = String::new();
    for c in source.chars() {
        match c {
            '{' => {
                depth += 1;
                segment.clear();
            }
            ';' | '}' => {
                let text = segment.trim();
                if depth > 0 && !text.is_empty() {
                    if let Some((key, _)) = text.split_once(':') {
                        if !key.trim().starts_with("--") {
                            return false;
                        }
                        declarations += 1;
                    }
                }
                if c == '}' {
                    depth = depth.saturating_sub(1);
                }
                segment.clear();
            }
            _ => segment.push(c),
        }
    }
    declarations > 0
}

/// The on-disk path segments of the theme `name` (`xyz:pink` is
/// `["xyz", "pink"]`), or `None` when the name is refused: a segment must be
/// ASCII letters, digits, `-` and `_` (so no `..`, no separator, nothing
/// absolute), no segment may be empty, and the name may not start with a
/// colour scheme ([`RESERVED_THEME_NAMES`]).
#[must_use]
pub fn theme_dir_segments(name: &str) -> Option<Vec<&str>> {
    let segments: Vec<&str> = name.split(':').collect();
    let valid = segments.iter().all(|s| {
        !s.is_empty()
            && s.len() <= 64
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    });
    let reserved = segments.first().is_some_and(|first| {
        RESERVED_THEME_NAMES
            .iter()
            .any(|r| r.eq_ignore_ascii_case(first))
    });
    (valid && !reserved).then_some(segments)
}

/// THE walk from a theme chain to its directories under the rice root:
/// `<root>/<kind>/<segment>/<segment>/` for every entry, in chain order,
/// refused names ([`theme_dir_segments`]) left out. `kind` is `"css"` for
/// the stylesheets and `"icons"` for the icon remap tables.
#[must_use]
pub fn chain_directories(root: &Path, kind: &str, chain: &[String]) -> Vec<(String, PathBuf)> {
    chain
        .iter()
        .filter_map(|name| {
            let segments = theme_dir_segments(name)?;
            let mut dir = root.join(kind);
            for segment in segments {
                dir.push(segment);
            }
            Some((name.clone(), dir))
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// where the files are
// ---------------------------------------------------------------------------------------------

/// Where the rice loader looks, and for whom. [`Self::from_process`] for the
/// real home directory; [`Self::with_root`] for anything else (every test).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiceEnv {
    /// The rice root (`~/.azul`), holding `css/<theme>/` and `icons/<theme>/`.
    /// `None`: no theme directories.
    pub root: Option<PathBuf>,
    /// The legacy per-app stylesheet. `None`: none.
    pub legacy_file: Option<PathBuf>,
    /// This application's name, what `app:` is matched against
    /// (case-insensitive): the executable's file stem.
    pub app: String,
    /// The running azul, what `azul:` is matched against.
    pub azul_version: String,
}

impl RiceEnv {
    /// A rice root at `root`, for the application `app`, with no legacy file.
    #[must_use]
    pub fn with_root(root: impl Into<PathBuf>, app: &str) -> Self {
        Self {
            root: Some(root.into()),
            legacy_file: None,
            app: app.to_string(),
            azul_version: crate::AZUL_VERSION.to_string(),
        }
    }

    /// The real one: `~/.azul`, the legacy file of this executable, its name.
    #[must_use]
    pub fn from_process() -> Self {
        let app = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_default();
        let legacy_file = if app.is_empty() {
            None
        } else {
            legacy_stylesheet_path(&app)
        };
        Self {
            root: default_rice_root(),
            legacy_file,
            app,
            azul_version: crate::AZUL_VERSION.to_string(),
        }
    }
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn home_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        env_path("USERPROFILE")
    } else {
        env_path("HOME")
    }
}

/// `~/.azul` (`%USERPROFILE%\.azul` on Windows).
#[must_use]
pub fn default_rice_root() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".azul"))
}

/// The legacy per-app stylesheet of `app`: `$XDG_CONFIG_HOME` (else
/// `~/.config`) `/azul/styles/<app>.css` on Linux and the BSDs,
/// `~/Library/Application Support/azul/styles/<app>.css` on macOS,
/// `%APPDATA%\azul\styles\<app>.css` on Windows.
#[must_use]
pub fn legacy_stylesheet_path(app: &str) -> Option<PathBuf> {
    let config = if cfg!(target_os = "windows") {
        env_path("APPDATA")?
    } else if cfg!(target_os = "macos") {
        home_dir()?.join("Library").join("Application Support")
    } else {
        env_path("XDG_CONFIG_HOME").or_else(|| home_dir().map(|h| h.join(".config")))?
    };
    Some(
        config
            .join("azul")
            .join("styles")
            .join(format!("{app}.css")),
    )
}

// ---------------------------------------------------------------------------------------------
// the status listing (FFI)
// ---------------------------------------------------------------------------------------------

/// What became of one rice file.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiceFileState {
    /// Its rules are in the cascade (live or inert per their conditions).
    Applied,
    /// Its `app:` names another application.
    OtherApp,
    /// Its `azul:` names no version matching the running azul.
    AzulVersionMismatch,
    /// `priority: off`, or a per-app file turned its theme off for this app.
    Off,
    /// Not read: too large, unreadable, not UTF-8, or outside the rice root.
    Refused,
}

impl RiceFileState {
    /// A short label for the report.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::OtherApp => "other app",
            Self::AzulVersionMismatch => "azul version",
            Self::Off => "off",
            Self::Refused => "refused",
        }
    }
}

/// One rice file in the [`RiceStatus`].
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiceFileStatus {
    /// The file.
    pub path: AzString,
    /// The theme it belongs to; empty: unthemed (live under every theme).
    pub theme: AzString,
    /// Its header's `@version`; empty: none.
    pub version: AzString,
    /// The application(s) it is for; empty: every app.
    pub app: AzString,
    /// Its `requires:`, as written; empty: none.
    pub requires: AzString,
    /// `ok`, or why a requirement failed (it never gates); empty: none.
    pub requires_check: AzString,
    /// How the priority came about (inferred, clamped, set by a per-app
    /// file) and anything else worth saying about the file.
    pub note: AzString,
    /// Index of its first rule in the loaded stylesheet (applied files).
    pub first_rule: usize,
    /// How many rules it has.
    pub rule_count: usize,
    /// Rules whose conditions hold under the context of the listing.
    pub live_rules: usize,
    /// Rules whose conditions do not (all of them, unless applied).
    pub inert_rules: usize,
    /// What became of it.
    pub state: RiceFileState,
    /// The priority it cascades at.
    pub priority: RicePriority,
    /// The `CssRuleBlock::priority` its rules carry.
    pub rule_priority: u8,
    /// A per-app file (`app:` names this app, or the legacy file).
    pub per_app: bool,
    /// No `requires:`, or every one satisfied.
    pub requires_ok: bool,
}

crate::impl_option!(
    RiceFileStatus,
    OptionRiceFileStatus,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);
crate::impl_vec!(
    RiceFileStatus,
    RiceFileStatusVec,
    RiceFileStatusVecDestructor,
    RiceFileStatusVecDestructorType,
    RiceFileStatusVecSlice,
    OptionRiceFileStatus
);
crate::impl_vec_mut!(RiceFileStatus, RiceFileStatusVec);
crate::impl_vec_debug!(RiceFileStatus, RiceFileStatusVec);
crate::impl_vec_clone!(
    RiceFileStatus,
    RiceFileStatusVec,
    RiceFileStatusVecDestructor
);
crate::impl_vec_partialeq!(RiceFileStatus, RiceFileStatusVec);
crate::impl_vec_eq!(RiceFileStatus, RiceFileStatusVec);

/// What the rice loader did: the chain, every file, every warning. The About
/// panel and the `AZ_DEBUG` log show it ([`Self::to_report`]); a bug report
/// carries it.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RiceStatus {
    /// The rice root; empty: none.
    pub root: AzString,
    /// The application `app:` keys were matched against.
    pub app: AzString,
    /// The running azul `azul:` keys were matched against.
    pub azul_version: AzString,
    /// The theme chain the files were loaded for, most specific first.
    pub chain: StringVec,
    /// Every file found, in cascade order (the later wins at equal rank).
    pub files: RiceFileStatusVec,
    /// Everything the loader dropped, ignored, clamped or doubted.
    pub warnings: StringVec,
    /// The `AZ_RICING` mode.
    pub mode: RicingMode,
}

impl RiceStatus {
    /// Rules of applied files whose conditions hold, over every file.
    #[must_use]
    pub fn live_rule_count(&self) -> usize {
        self.files.iter().map(|f| f.live_rules).sum()
    }

    /// Does any applied file cascade above `base`? (What the support policy
    /// is about.)
    #[must_use]
    pub fn is_beyond_base(&self) -> bool {
        self.files.iter().any(|f| {
            f.state == RiceFileState::Applied
                && matches!(
                    f.priority,
                    RicePriority::App | RicePriority::Widgets | RicePriority::Force
                )
        })
    }

    /// The listing as text: the chain, one line per file, the warnings, and
    /// the support policy with the `AZ_RICING=off` self-check.
    #[must_use]
    pub fn to_report(&self) -> String {
        let mode = match self.mode {
            RicingMode::Off => "off",
            RicingMode::Default => "default",
            RicingMode::Force => "force",
            RicingMode::Watch => "watch",
        };
        let mut out = format!(
            "rice (AZ_RICING={mode}; root {}; app {}; azul {})\n",
            or_none(self.root.as_str()),
            or_none(self.app.as_str()),
            self.azul_version.as_str()
        );
        let chain: Vec<&str> = self.chain.iter().map(AzString::as_str).collect();
        out.push_str(&format!("chain: {}\n", or_none(&chain.join(" > "))));
        if self.files.is_empty() {
            out.push_str("files: none\n");
        }
        for f in &self.files {
            out.push_str(&format!(
                "  [{}] {} {}{} {}: {} rules, {} live, {} inert",
                f.state.label(),
                f.priority.name(),
                if f.theme.as_str().is_empty() {
                    "(every theme)"
                } else {
                    f.theme.as_str()
                },
                if f.version.as_str().is_empty() {
                    String::new()
                } else {
                    format!(" {}", f.version.as_str())
                },
                f.path.as_str(),
                f.rule_count,
                f.live_rules,
                f.inert_rules,
            ));
            if !f.app.as_str().is_empty() {
                out.push_str(&format!("; app {}", f.app.as_str()));
            }
            if !f.requires.as_str().is_empty() {
                out.push_str(&format!(
                    "; requires {}: {}",
                    f.requires.as_str(),
                    f.requires_check.as_str()
                ));
            }
            if !f.note.as_str().is_empty() {
                out.push_str(&format!("; {}", f.note.as_str()));
            }
            out.push('\n');
        }
        if !self.warnings.is_empty() {
            out.push_str("warnings:\n");
            for w in &self.warnings {
                out.push_str(&format!("  - {}\n", w.as_str()));
            }
        }
        out.push_str(RICE_SUPPORT_POLICY);
        out
    }
}

const fn or_none(s: &str) -> &str {
    if s.is_empty() {
        "(none)"
    } else {
        s
    }
}

// ---------------------------------------------------------------------------------------------
// loading
// ---------------------------------------------------------------------------------------------

/// The rice of one theme chain: every applied file's rules, stamped and
/// wrapped, in one stylesheet, and the listing of what was found.
#[derive(Debug, Clone, Default)]
pub struct LoadedRice {
    /// The user-origin stylesheet: every applied rule, least specific theme
    /// first, each wrapped in `@theme(<its theme>)` and stamped with its
    /// file's priority. Unscoped: it addresses the whole window
    /// (`StyledDom::create_from_dom_with_user_sheets`).
    pub css: Css,
    /// The listing. Its live / inert counts are filled in by
    /// [`Self::status_under`]; here every rule of an applied file counts as
    /// live.
    pub status: RiceStatus,
}

impl LoadedRice {
    /// The listing with each file's live versus inert rules under `ctx`:
    /// "rule 12 of pink.css is inert because `@os(linux:kde)` is false".
    /// Per-node conditions (pseudo-states, container queries) count as live.
    #[must_use]
    pub fn status_under(&self, ctx: &DynamicSelectorContext) -> RiceStatus {
        let rules = self.css.rules.as_ref();
        let mut status = self.status.clone();
        let mut files = status.files.clone().into_library_owned_vec();
        for f in &mut files {
            if f.state == RiceFileState::Applied {
                let end = f.first_rule.saturating_add(f.rule_count).min(rules.len());
                let slice = rules.get(f.first_rule..end).unwrap_or(&[]);
                let live = slice.iter().filter(|r| rule_is_live(r, ctx)).count();
                f.live_rules = live;
                f.inert_rules = slice.len() - live;
            } else {
                f.live_rules = 0;
                f.inert_rules = f.rule_count;
            }
        }
        status.files = files.into();
        status
    }
}

fn rule_is_live(rule: &CssRuleBlock, ctx: &DynamicSelectorContext) -> bool {
    rule.conditions.as_ref().iter().all(|c| match c {
        DynamicSelector::PseudoState(_)
        | DynamicSelector::ContainerWidth(_)
        | DynamicSelector::ContainerHeight(_)
        | DynamicSelector::ContainerName(_) => true,
        other => other.matches(ctx),
    })
}

/// One file as read from disk, before it is resolved against the others.
#[derive(Debug)]
struct RawFile {
    path: PathBuf,
    /// The theme its directory implies.
    dir_theme: Option<String>,
    legacy: bool,
    /// Why it was not read.
    refused: Option<String>,
    header: RiceHeader,
    /// The sanitized source.
    source: String,
    sanitize_warnings: Vec<String>,
}

/// Read one file: a regular UTF-8 file, at most [`MAX_RICE_FILE_BYTES`], and
/// - when `root` is given - resolving (through symlinks) inside it.
fn read_rice_file(path: &Path, root: Option<&Path>) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("unreadable: {e}"))?;
    if !meta.is_file() {
        return Err("not a file".to_string());
    }
    if meta.len() > MAX_RICE_FILE_BYTES {
        return Err(format!(
            "{} bytes, over the {MAX_RICE_FILE_BYTES} byte limit",
            meta.len()
        ));
    }
    if let Some(root) = root {
        let resolved = std::fs::canonicalize(path).map_err(|e| format!("unreadable: {e}"))?;
        if !resolved.starts_with(root) {
            return Err(format!(
                "resolves to {}, outside the rice root",
                resolved.display()
            ));
        }
    }
    let bytes = std::fs::read(path).map_err(|e| format!("unreadable: {e}"))?;
    if bytes.len() as u64 > MAX_RICE_FILE_BYTES {
        return Err(format!("over the {MAX_RICE_FILE_BYTES} byte limit"));
    }
    String::from_utf8(bytes).map_err(|_| "not UTF-8".to_string())
}

fn load_raw(
    path: PathBuf,
    dir_theme: Option<String>,
    legacy: bool,
    root: Option<&Path>,
) -> RawFile {
    match read_rice_file(&path, root) {
        Ok(text) => {
            let header = parse_rice_header(&text);
            let sanitized = sanitize_rice_source(&text);
            RawFile {
                path,
                dir_theme,
                legacy,
                refused: None,
                header,
                source: sanitized.source,
                sanitize_warnings: sanitized.warnings,
            }
        }
        Err(why) => RawFile {
            path,
            dir_theme,
            legacy,
            refused: Some(why),
            header: RiceHeader::default(),
            source: String::new(),
            sanitize_warnings: Vec::new(),
        },
    }
}

/// The `.css` files directly in `dir`, sorted, at most
/// [`MAX_RICE_FILES_PER_DIR`].
fn css_files_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("css"))
        })
        .collect();
    files.sort();
    files.truncate(MAX_RICE_FILES_PER_DIR);
    files
}

fn applies_to_app(header: &RiceHeader, app: &str) -> bool {
    header.apps.is_empty() || header.apps.iter().any(|a| a.eq_ignore_ascii_case(app))
}

/// Can this file speak for its theme (read, for this app, for this azul)?
fn is_usable(raw: &RawFile, env: &RiceEnv) -> bool {
    raw.refused.is_none()
        && applies_to_app(&raw.header, &env.app)
        && azul_version_matches(&raw.header.azul, &env.azul_version)
}

fn push_unique(out: &mut Vec<String>, items: &[String]) {
    for item in items {
        if !out.iter().any(|o| o == item) {
            out.push(item.clone());
        }
    }
}

/// The `fallback:` list of the theme `name`, from the headers of its files
/// (its directory's, and the global files whose `theme:` names it), in file
/// order, de-duplicated. Only files that apply to this app and this azul
/// speak. What the theme chain's expansion asks ([`rice_chain`]).
#[must_use]
pub fn fallback_of(env: &RiceEnv, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Some(root) = env.root.as_ref() else {
        return out;
    };
    let canon = std::fs::canonicalize(root).ok();
    let canon = canon.as_deref();
    for (theme, dir) in chain_directories(root, "css", &[name.to_string()]) {
        for path in css_files_in(&dir) {
            let raw = load_raw(path, Some(theme.clone()), false, canon);
            if is_usable(&raw, env) {
                push_unique(&mut out, &raw.header.fallback);
            }
        }
    }
    for path in css_files_in(&root.join("css")) {
        let raw = load_raw(path, None, false, canon);
        if raw.header.theme.as_deref() == Some(name) && is_usable(&raw, env) {
            push_unique(&mut out, &raw.header.fallback);
        }
    }
    out
}

/// The theme chain the rice of `head` is loaded for: the theme chain's
/// expansion (`crate::theme_chain::expand_chain`: `:` prefixes, `fallback:`
/// headers transitively, `app_default` as the floor), fed by the headers
/// this loader parses. Returns the chain and the expansion's warnings.
#[must_use]
pub fn rice_chain(env: &RiceEnv, head: &str, app_default: &str) -> (Vec<String>, Vec<String>) {
    let fallback = |name: &str| fallback_of(env, name);
    let expanded = crate::theme_chain::expand_chain(head, &fallback, app_default);
    let names = expanded
        .names
        .iter()
        .map(|n| n.as_str().to_string())
        .collect();
    let warnings = expanded
        .warnings
        .iter()
        .map(|w| w.as_str().to_string())
        .collect();
    (names, warnings)
}

/// One file, resolved.
#[derive(Debug)]
struct Entry {
    raw: RawFile,
    theme: Option<String>,
    per_app: bool,
    state: RiceFileState,
    priority: RicePriority,
    notes: Vec<String>,
    requires_ok: bool,
    requires_check: String,
    /// The parsed rules (stamped only if applied).
    css: Css,
}

impl Entry {
    fn label(&self) -> String {
        self.raw.path.display().to_string()
    }
}

/// Resolve a raw file on its own: theme, app, the `azul:` gate, its priority.
fn resolve(raw: RawFile, env: &RiceEnv, warnings: &mut Vec<String>) -> Entry {
    let label = raw.path.display().to_string();
    let mut notes = Vec::new();
    for w in raw
        .header
        .warnings
        .iter()
        .chain(raw.sanitize_warnings.iter())
    {
        warnings.push(format!("{label}: {w}"));
    }

    // The theme: the directory's, else the header's.
    let mut theme = raw.dir_theme.clone();
    let mut refused = raw.refused.clone();
    match (&raw.dir_theme, &raw.header.theme) {
        (Some(dir), Some(named)) if dir != named => {
            warnings.push(format!(
                "{label}: the header says theme `{named}`, the directory says `{dir}`: the \
                 directory wins"
            ));
        }
        (None, Some(named)) => {
            if theme_dir_segments(named).is_some() {
                theme = Some(named.clone());
            } else if refused.is_none() {
                refused = Some(format!("the theme name `{named}` is refused"));
            }
        }
        _ => {}
    }

    let per_app = raw.legacy || !raw.header.apps.is_empty();
    let mut state = RiceFileState::Applied;
    if let Some(why) = &refused {
        state = RiceFileState::Refused;
        warnings.push(format!("{label} refused: {why}"));
    } else if !applies_to_app(&raw.header, &env.app) {
        state = RiceFileState::OtherApp;
    } else if !azul_version_matches(&raw.header.azul, &env.azul_version) {
        state = RiceFileState::AzulVersionMismatch;
        warnings.push(format!(
            "{label} ignored: written for azul {}, this is azul {}",
            raw.header.azul.join(", "),
            env.azul_version
        ));
    }

    let priority = match raw.header.priority {
        Some(RicePriority::Palette) => {
            if is_palette_source(&raw.source) {
                RicePriority::Palette
            } else {
                notes.push("declared palette, sets other properties: applied at base".to_string());
                if state == RiceFileState::Applied {
                    warnings.push(format!(
                        "{label}: declared `priority: palette` but sets properties other than \
                         custom properties (--name): applied at base"
                    ));
                }
                RicePriority::Base
            }
        }
        Some(p) => p,
        None if refused.is_none() && is_palette_source(&raw.source) => {
            notes.push("palette by inspection".to_string());
            RicePriority::Palette
        }
        None => RicePriority::Base,
    };
    if priority == RicePriority::Off && state == RiceFileState::Applied {
        state = RiceFileState::Off;
    }

    let css = if refused.is_none() {
        let (css, parse_warnings) = crate::parser2::new_from_str(&raw.source);
        if !parse_warnings.is_empty() && state == RiceFileState::Applied {
            warnings.push(format!(
                "{label}: {} declaration(s) or rule(s) could not be parsed",
                parse_warnings.len()
            ));
        }
        css
    } else {
        Css::empty()
    };

    Entry {
        raw,
        theme,
        per_app,
        state,
        priority,
        notes,
        requires_ok: true,
        requires_check: String::new(),
        css,
    }
}

/// Pitfall 7: an explicit `priority:` in a per-app file replaces the
/// priority of the global files of the same theme for this app - both ways.
fn apply_per_app_priorities(entries: &mut [Entry], env: &RiceEnv, warnings: &mut Vec<String>) {
    let mut scopes: Vec<Option<String>> = Vec::new();
    for e in entries.iter() {
        if !scopes.contains(&e.theme) {
            scopes.push(e.theme.clone());
        }
    }
    for scope in scopes {
        let overrides: Vec<(RicePriority, String)> = entries
            .iter()
            .filter(|e| {
                e.theme == scope
                    && e.per_app
                    && matches!(e.state, RiceFileState::Applied | RiceFileState::Off)
            })
            .filter_map(|e| e.raw.header.priority.map(|p| (p, e.label())))
            .collect();
        let Some((lowest, by)) = overrides.iter().min_by_key(|(p, _)| *p).cloned() else {
            continue;
        };
        if overrides.iter().any(|(p, _)| *p != lowest) {
            warnings.push(format!(
                "the per-app files of theme `{}` disagree on its priority: using the lowest, {}",
                scope.as_deref().unwrap_or("(every theme)"),
                lowest.name()
            ));
        }
        for e in entries.iter_mut() {
            if e.theme != scope || e.per_app || e.state != RiceFileState::Applied {
                continue;
            }
            if lowest == RicePriority::Off {
                e.state = RiceFileState::Off;
                e.notes.push(format!("turned off for {} by {by}", env.app));
            } else if e.priority != lowest {
                e.notes.push(format!(
                    "{} for {} by {by} (was {})",
                    lowest.name(),
                    env.app,
                    e.priority.name()
                ));
                e.priority = lowest;
            }
        }
    }
}

/// A file without a theme is live under every theme: above `base` it is
/// clamped to `base` (a palette stays a palette).
fn clamp_unthemed(entries: &mut [Entry], warnings: &mut Vec<String>) {
    for e in entries.iter_mut() {
        if e.theme.is_none()
            && e.state == RiceFileState::Applied
            && matches!(
                e.priority,
                RicePriority::App | RicePriority::Widgets | RicePriority::Force
            )
        {
            warnings.push(format!(
                "{}: priority {} clamped to base: a file without a theme is live under every \
                 theme (give it `theme:` or put it in css/<theme>/)",
                e.label(),
                e.priority.name()
            ));
            e.notes
                .push(format!("clamped to base (was {})", e.priority.name()));
            e.priority = RicePriority::Base;
        }
    }
}

/// Check every `requires:` against the versions the files' headers declare.
/// Never gates: a mismatch is a warning and a line in the status.
fn check_requirements(entries: &mut [Entry], warnings: &mut Vec<String>) {
    let mut versions: Vec<(String, String)> = Vec::new();
    for e in entries.iter() {
        if let (Some(theme), Some(version)) = (&e.theme, &e.raw.header.version) {
            if e.raw.refused.is_none() && !versions.iter().any(|(t, _)| t == theme) {
                versions.push((theme.clone(), version.clone()));
            }
        }
    }
    for e in entries.iter_mut() {
        if e.raw.header.requires.is_empty() || e.state == RiceFileState::Refused {
            continue;
        }
        let subject = match (&e.theme, &e.raw.header.version) {
            (Some(t), Some(v)) => format!("{t} {v}"),
            (Some(t), None) => t.clone(),
            _ => e.label(),
        };
        let mut problems = Vec::new();
        for req in &e.raw.header.requires {
            match versions.iter().find(|(t, _)| *t == req.theme) {
                Some((_, found)) if version_satisfies(&req.range, found) => {}
                Some((_, found)) => problems.push(format!(
                    "{subject} requires {} {}, found {found}",
                    req.theme, req.range
                )),
                None => problems.push(format!(
                    "{subject} requires {} {}, which declares no version here",
                    req.theme, req.range
                )),
            }
        }
        e.requires_ok = problems.is_empty();
        e.requires_check = if problems.is_empty() {
            "ok".to_string()
        } else {
            problems.join("; ")
        };
        if e.state == RiceFileState::Applied {
            for p in problems {
                warnings.push(format!("{}: {p} (applied anyway)", e.label()));
            }
        }
    }
}

/// Load the rice of the theme chain `chain` (most specific first) from
/// `env`: the global files, every chain entry's directory, the legacy file.
///
/// The returned stylesheet orders the rules for the cascade: files of themes
/// outside the chain (inert), unthemed files, then the chain from its last
/// entry to its head; per-app files after the global files of their theme.
/// At equal priority the chain's rank decides, then this order.
#[must_use]
pub fn load_rice(env: &RiceEnv, chain: &[String]) -> LoadedRice {
    let mut warnings: Vec<String> = Vec::new();

    // Collect: global files, the chain's directories, the legacy file.
    let mut raws: Vec<RawFile> = Vec::new();
    if let Some(root) = env.root.as_ref() {
        let canon = std::fs::canonicalize(root).ok();
        if let Some(canon) = canon.as_deref() {
            for path in css_files_in(&root.join("css")) {
                raws.push(load_raw(path, None, false, Some(canon)));
            }
            for name in chain {
                if theme_dir_segments(name).is_none() {
                    warnings.push(format!(
                        "theme `{name}` refused: a theme name is ASCII letters, digits, `-` and \
                         `_`, `:` between the parts of a spin-off, and not `light` or `dark`"
                    ));
                }
            }
            let mut seen: Vec<String> = Vec::new();
            for (name, dir) in chain_directories(root, "css", chain) {
                if seen.contains(&name) {
                    continue;
                }
                for path in css_files_in(&dir) {
                    raws.push(load_raw(path, Some(name.clone()), false, Some(canon)));
                }
                seen.push(name);
            }
        }
    }
    if let Some(legacy) = env.legacy_file.as_ref().filter(|p| p.exists()) {
        raws.push(load_raw(legacy.clone(), None, true, None));
    }

    let mut entries: Vec<Entry> = raws
        .into_iter()
        .map(|raw| resolve(raw, env, &mut warnings))
        .collect();
    apply_per_app_priorities(&mut entries, env, &mut warnings);
    clamp_unthemed(&mut entries, &mut warnings);
    check_requirements(&mut entries, &mut warnings);

    // Cascade order: themes outside the chain, unthemed, then the chain from
    // its floor to its head; within a group global before per-app, then the
    // collection order (global files, directory files, legacy; by name).
    let group = |e: &Entry| -> usize {
        match &e.theme {
            None => 1,
            Some(t) => match chain.iter().position(|c| c == t) {
                Some(rank) => 2 + (chain.len() - 1 - rank),
                None => 0,
            },
        }
    };
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by_key(|&i| (group(&entries[i]), entries[i].per_app));

    let mut rules: Vec<CssRuleBlock> = Vec::new();
    let mut keyframes = Vec::new();
    let mut files: Vec<RiceFileStatus> = Vec::new();
    for i in order {
        let e = &mut entries[i];
        let parsed = core::mem::take(&mut e.css);
        let mut own_rules = parsed.rules.into_library_owned_vec();
        let rule_count = own_rules.len();
        let first_rule = rules.len();
        let slot = e.priority.rule_priority().unwrap_or(rule_priority::SYSTEM);
        if e.state == RiceFileState::Applied {
            for rule in &mut own_rules {
                rule.priority = slot;
                if let Some(theme) = &e.theme {
                    let mut conditions = Vec::with_capacity(rule.conditions.as_ref().len() + 1);
                    conditions.push(DynamicSelector::Theme(ThemeCondition::Custom(
                        AzString::from(theme.clone()),
                    )));
                    conditions.extend(rule.conditions.as_ref().iter().cloned());
                    rule.conditions = conditions.into();
                }
            }
            rules.extend(own_rules);
            keyframes.extend(parsed.keyframes.into_library_owned_vec());
        }
        let applied = e.state == RiceFileState::Applied;
        let app = if e.raw.legacy {
            env.app.clone()
        } else {
            e.raw.header.apps.join(", ")
        };
        let requires = e
            .raw
            .header
            .requires
            .iter()
            .map(|r| format!("{}@{}", r.theme, r.range))
            .collect::<Vec<_>>()
            .join(", ");
        files.push(RiceFileStatus {
            path: AzString::from(e.label()),
            theme: AzString::from(e.theme.clone().unwrap_or_default()),
            version: AzString::from(e.raw.header.version.clone().unwrap_or_default()),
            app: AzString::from(app),
            requires: AzString::from(requires),
            requires_check: AzString::from(e.requires_check.clone()),
            note: AzString::from(e.notes.join("; ")),
            first_rule: if applied { first_rule } else { 0 },
            rule_count,
            live_rules: if applied { rule_count } else { 0 },
            inert_rules: if applied { 0 } else { rule_count },
            state: e.state,
            priority: e.priority,
            rule_priority: if applied { slot } else { 0 },
            per_app: e.per_app,
            requires_ok: e.requires_ok,
        });
    }

    let mut css = Css::new(rules);
    css.keyframes = keyframes.into();
    LoadedRice {
        css,
        status: RiceStatus {
            root: AzString::from(
                env.root
                    .as_ref()
                    .map(|r| r.display().to_string())
                    .unwrap_or_default(),
            ),
            app: AzString::from(env.app.clone()),
            azul_version: AzString::from(env.azul_version.clone()),
            chain: StringVec::from(chain.to_vec()),
            files: files.into(),
            warnings: StringVec::from(warnings),
            mode: RicingMode::Default,
        },
    }
}

// ---------------------------------------------------------------------------------------------
// the process's rice
// ---------------------------------------------------------------------------------------------

/// The rice of this process: where it is read from, what was loaded for
/// which chain head, and the watch state. `None` until [`install`]: a
/// process that never calls it (a test, a headless tool) never reads the
/// home directory.
#[derive(Debug)]
struct ProcessRice {
    env: RiceEnv,
    mode: RicingMode,
    /// Bumped by every reload a watch triggered; a window built under an
    /// older generation owes a rebuild.
    generation: u64,
    loaded: Option<(String, Arc<LoadedRice>)>,
    fingerprint: Option<u64>,
    /// A watch reload no window has been asked to adopt yet.
    reload_signal: bool,
    /// A load whose listing nobody printed yet.
    report_pending: bool,
    /// [`installed_fallback_of`]'s answers: the chain builder runs whenever
    /// a context is built (every restyle, every widget's structure pick),
    /// and the headers only change with the rice tree - a watch reload
    /// clears this.
    fallbacks: alloc::collections::BTreeMap<String, Vec<String>>,
}

static PROCESS: std::sync::Mutex<Option<ProcessRice>> = std::sync::Mutex::new(None);

fn with_process<R>(f: impl FnOnce(&mut Option<ProcessRice>) -> R) -> R {
    let mut guard = PROCESS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    f(&mut guard)
}

/// Start this process's rice: read from `env`, in the `AZ_RICING` mode.
/// `App::create` calls it with [`RiceEnv::from_process`]. Nothing is read
/// until a window asks ([`rice_for_theme`]).
pub fn install(env: RiceEnv) {
    install_with_mode(env, ricing_mode());
}

/// [`install`] with an explicit mode.
pub fn install_with_mode(env: RiceEnv, mode: RicingMode) {
    with_process(|p| {
        *p = Some(ProcessRice {
            env,
            mode,
            generation: 0,
            loaded: None,
            fingerprint: None,
            reload_signal: false,
            report_pending: false,
            fallbacks: alloc::collections::BTreeMap::new(),
        });
    });
}

/// The installed mode; `None` before [`install`].
#[must_use]
pub fn installed_mode() -> Option<RicingMode> {
    with_process(|p| p.as_ref().map(|p| p.mode))
}

/// The rice of the app theme `head` (loaded on first ask, cached until the
/// head changes or a watch reload): what every window's DOM is styled with.
/// `None` before [`install`] and under `AZ_RICING=off`.
#[must_use]
pub fn rice_for_theme(head: &str) -> Option<Arc<LoadedRice>> {
    with_process(|p| {
        let p = p.as_mut()?;
        if p.mode == RicingMode::Off {
            return None;
        }
        if let Some((loaded_head, loaded)) = &p.loaded {
            if loaded_head == head {
                return Some(loaded.clone());
            }
        }
        let (chain, chain_warnings) = rice_chain(&p.env, head, DEFAULT_APP_THEME);
        let mut loaded = load_rice(&p.env, &chain);
        loaded.status.mode = p.mode;
        if !chain_warnings.is_empty() {
            let mut warnings = chain_warnings;
            warnings.extend(
                loaded
                    .status
                    .warnings
                    .iter()
                    .map(|w| w.as_str().to_string()),
            );
            loaded.status.warnings = StringVec::from(warnings);
        }
        if p.mode == RicingMode::Watch && p.fingerprint.is_none() {
            p.fingerprint = Some(fingerprint(&p.env));
        }
        let loaded = Arc::new(loaded);
        p.loaded = Some((head.to_string(), loaded.clone()));
        p.report_pending = true;
        Some(loaded)
    })
}

/// The reload generation: bumped by every change a watch saw. A window
/// records the generation it was built under; a newer one owes a rebuild.
#[must_use]
pub fn generation() -> u64 {
    with_process(|p| p.as_ref().map_or(0, |p| p.generation))
}

/// `AZ_RICING=watch`: has a rice file changed since the last poll? If so the
/// loaded rice is dropped (the next [`rice_for_theme`] reloads), the
/// [`generation`] moves on, and [`take_reload_signal`] answers `true` once.
/// Reads the file tree (names, sizes, modification times) - call it off the
/// event loop, a few times a second. `false` in any other mode.
#[must_use]
pub fn poll_watch() -> bool {
    let Some(env) = with_process(|p| {
        p.as_ref()
            .filter(|p| p.mode == RicingMode::Watch)
            .map(|p| p.env.clone())
    }) else {
        return false;
    };
    let now = fingerprint(&env);
    with_process(|p| {
        let Some(p) = p.as_mut() else {
            return false;
        };
        match p.fingerprint {
            Some(before) if before != now => {
                p.fingerprint = Some(now);
                p.loaded = None;
                p.fallbacks.clear();
                p.generation = p.generation.wrapping_add(1);
                p.reload_signal = true;
                true
            }
            Some(_) => false,
            None => {
                p.fingerprint = Some(now);
                false
            }
        }
    })
}

/// Has a watch reload happened that no window was asked to adopt? Answers
/// `true` once per reload.
#[must_use]
pub fn take_reload_signal() -> bool {
    with_process(|p| {
        p.as_mut()
            .is_some_and(|p| core::mem::take(&mut p.reload_signal))
    })
}

/// The listing of a load nobody printed yet, under `ctx` - what the shell
/// logs after a window was styled (visible under `AZ_DEBUG`). `None` when
/// there is nothing new.
#[must_use]
pub fn take_pending_status(ctx: &DynamicSelectorContext) -> Option<RiceStatus> {
    let loaded = with_process(|p| {
        let p = p.as_mut()?;
        if !core::mem::take(&mut p.report_pending) {
            return None;
        }
        p.loaded.as_ref().map(|(_, l)| l.clone())
    })?;
    Some(loaded.status_under(ctx))
}

/// The listing of this process's rice under `ctx` (its chain replaced by
/// the rice's). Before anything was loaded: the mode, root and app, and why
/// there is nothing.
#[must_use]
pub fn rice_status(ctx: Option<&DynamicSelectorContext>) -> RiceStatus {
    let (loaded, mode, env) = with_process(|p| match p.as_ref() {
        Some(p) => (
            p.loaded.as_ref().map(|(_, l)| l.clone()),
            p.mode,
            Some(p.env.clone()),
        ),
        None => (None, ricing_mode(), None),
    });
    if let Some(loaded) = loaded {
        let mut ctx = ctx
            .cloned()
            .unwrap_or_else(|| DynamicSelectorContext::from_system_style(&SystemStyle::default()));
        ctx.theme_chain = loaded.status.chain.clone();
        return loaded.status_under(&ctx);
    }
    let why = match (&env, mode) {
        (_, RicingMode::Off) => "AZ_RICING=off: no rice is loaded",
        (None, _) => "the rice loader is not installed in this process",
        _ => "no window has been styled yet",
    };
    RiceStatus {
        root: AzString::from(
            env.as_ref()
                .and_then(|e| e.root.as_ref())
                .map(|r| r.display().to_string())
                .unwrap_or_default(),
        ),
        app: AzString::from(env.as_ref().map(|e| e.app.clone()).unwrap_or_default()),
        azul_version: AzString::from(crate::AZUL_VERSION),
        warnings: StringVec::from(vec![why.to_string()]),
        mode,
        ..RiceStatus::default()
    }
}

/// [`fallback_of`] against this process's rice root: what the theme chain
/// of the cascade (`DynamicSelectorContext::theme_chain`) asks, so the
/// window's chain and the rice's chain are one chain. Empty before
/// [`install`] and under `AZ_RICING=off`.
#[must_use]
pub fn installed_fallback_of(name: &str) -> Vec<String> {
    // No rice (headless, tests, `AZ_RICING=off`): nothing, without a read.
    let cached = with_process(|p| match p.as_ref() {
        Some(p) if p.mode != RicingMode::Off => Some(p.fallbacks.get(name).cloned()),
        _ => None,
    });
    match cached {
        None => return Vec::new(),
        Some(Some(hit)) => return hit,
        Some(None) => {}
    }
    let Some(env) = with_process(|p| p.as_ref().map(|p| p.env.clone())) else {
        return Vec::new();
    };
    // Read outside the lock; the first answer per name is kept.
    let found = fallback_of(&env, name);
    with_process(|p| {
        if let Some(p) = p.as_mut() {
            p.fallbacks.insert(name.to_string(), found.clone());
        }
    });
    found
}

/// A hash of the rice tree's file names, sizes and modification times (and
/// the legacy file's). Bounded: 8 directory levels, 4096 entries.
fn fingerprint(env: &RiceEnv) -> u64 {
    use core::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut budget = 4096usize;
    if let Some(root) = env.root.as_ref() {
        fingerprint_dir(&root.join("css"), 0, &mut hasher, &mut budget);
    }
    if let Some(legacy) = env.legacy_file.as_ref() {
        fingerprint_file(legacy, &mut hasher);
    }
    budget.hash(&mut hasher);
    hasher.finish()
}

fn fingerprint_file(path: &Path, hasher: &mut impl core::hash::Hasher) {
    use core::hash::Hash;
    path.hash(hasher);
    if let Ok(meta) = std::fs::metadata(path) {
        meta.len().hash(hasher);
        meta.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .hash(hasher);
    }
}

fn fingerprint_dir(
    dir: &Path,
    depth: usize,
    hasher: &mut impl core::hash::Hasher,
    budget: &mut usize,
) {
    if depth > 8 || *budget == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        if path.is_dir() {
            fingerprint_dir(&path, depth + 1, hasher, budget);
        } else {
            fingerprint_file(&path, hasher);
        }
    }
}

impl SystemStyle {
    /// What the rice loader loaded - the theme chain, every file with its
    /// priority, version, `requires` check and live versus inert rules under
    /// the context this system style describes - and the warnings. For an
    /// About panel and a bug report; [`RiceStatus::to_report`] prints it.
    #[must_use]
    pub fn get_rice_status(&self) -> RiceStatus {
        let ctx = DynamicSelectorContext::from_system_style(self);
        rice_status(Some(&ctx))
    }
}
