//! The manifests of a project, read for the two things the index needs of them: the NAME of the
//! package a folder is (so `azul_core::dom::Node` can be traced to the folder of the crate, and "the
//! azul core" to a package) and the names of the packages it DEPENDS on (words a reviewer says about
//! the libraries the code uses: "serde", "left pad").
//!
//! Rough and line-based, as the code scanners are: no TOML or JSON parser, only enough of the
//! shape to find the keys that matter: the `name` of `[package]`, the keys of the dependency tables
//! (`[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`, `[workspace.dependencies]`,
//! `[target.'cfg(..)'.dependencies]`, `[dependencies.name]`, and a `package = "real-name"` rename),
//! the `name` and the dependency objects of a `package.json`. A name is a [`Kind::Package`] or a
//! [`Kind::Dependency`] with the line it stands on. Total on any input; bounded by [`ScanCaps`].

use crate::{
    symbols::{Kind, RawDef, Scanned, Vis},
    symbols_scan::{count_lines, ScanCaps},
    symbols_source::ManifestKind,
};

/// Reads manifest `kind`. A kind that is not read yet gives a file with no names.
#[must_use]
pub fn scan_manifest(kind: ManifestKind, _rel: &str, bytes: &[u8], caps: &ScanCaps) -> Scanned {
    let defs = match kind {
        ManifestKind::Cargo => scan_cargo(&String::from_utf8_lossy(bytes), caps),
        ManifestKind::PackageJson => scan_package_json(bytes, caps),
        ManifestKind::GoMod | ManifestKind::Requirements | ManifestKind::Pyproject | ManifestKind::CMake => Vec::new(),
    };
    Scanned {
        lang: None,
        size: bytes.len() as u64,
        mtime_ms: 0,
        lines: count_lines(bytes),
        defs,
        imports: Vec::new(),
        test_file: false,
    }
}

fn def(name: &str, kind: Kind, line: u32) -> RawDef {
    RawDef {
        name: name.to_string(),
        kind,
        vis: Vis::Public,
        test: false,
        line,
        lines: 1,
        parent: None,
    }
}

// ---- Cargo.toml ----

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Package,
    Deps,
    /// `[dependencies.name]`: the header named the dependency, the keys below describe it.
    DepTable,
    Other,
}

/// The string literal that starts at `s[at]` (a `"` or a `'`): its content and the index past it.
fn quoted(s: &[u8], at: usize) -> Option<(String, usize)> {
    let q = *s.get(at)?;
    if q != b'"' && q != b'\'' {
        return None;
    }
    let mut j = at + 1;
    let mut out = Vec::new();
    while j < s.len() {
        let b = s[j];
        if b == q {
            return Some((String::from_utf8_lossy(&out).into_owned(), j + 1));
        }
        if b == b'\\' && q == b'"' && j + 1 < s.len() {
            out.push(s[j + 1]);
            j += 2;
            continue;
        }
        out.push(b);
        j += 1;
    }
    None
}

/// How much deeper the brackets of `line` leave a value that continues on the next line; strings,
/// comments and the opening of a multi-line string are not counted. `triple` is the quote of a
/// multi-line string that is open at the start of the line (and, afterwards, at its end).
fn bracket_delta(line: &str, triple: &mut Option<u8>) -> i32 {
    let b = line.as_bytes();
    let mut delta = 0i32;
    let mut i = 0usize;
    if let Some(q) = *triple {
        match find_triple(b, 0, q) {
            Some(end) => {
                *triple = None;
                i = end;
            }
            None => return 0,
        }
    }
    while i < b.len() {
        match b[i] {
            b'#' => break,
            q @ (b'"' | b'\'') => {
                if b.get(i + 1) == Some(&q) && b.get(i + 2) == Some(&q) {
                    match find_triple(b, i + 3, q) {
                        Some(end) => i = end,
                        None => {
                            *triple = Some(q);
                            break;
                        }
                    }
                    continue;
                }
                match quoted(b, i) {
                    Some((_, end)) => {
                        i = end;
                        continue;
                    }
                    None => break,
                }
            }
            b'{' | b'[' => delta += 1,
            b'}' | b']' => delta -= 1,
            _ => {}
        }
        i += 1;
    }
    delta
}

fn find_triple(b: &[u8], from: usize, q: u8) -> Option<usize> {
    let mut k = from;
    while k + 3 <= b.len() {
        if b[k] == q && b[k + 1] == q && b[k + 2] == q {
            return Some(k + 3);
        }
        k += 1;
    }
    None
}

/// The text of a table header (`[a.b]`, `[[a]]`) without its brackets, the first `]` outside quotes
/// ending it.
fn header_text(t: &str) -> Option<String> {
    let b = t.as_bytes();
    let mut i = 0;
    while b.get(i) == Some(&b'[') {
        i += 1;
    }
    let start = i;
    let mut quote = 0u8;
    while i < b.len() {
        let c = b[i];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b']' {
            return Some(t[start..i].trim().to_string());
        }
        i += 1;
    }
    None
}

/// The dot-separated parts of a table name, a quoted part kept whole and without its quotes.
fn header_parts(h: &str) -> Vec<String> {
    let b = h.as_bytes();
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => match quoted(b, i) {
                Some((text, end)) => {
                    cur.push_str(&text);
                    i = end;
                    continue;
                }
                None => break,
            },
            b'.' => parts.push(std::mem::take(&mut cur)),
            c if c.is_ascii_whitespace() => {}
            _ => {
                let start = i;
                while i < b.len() && !matches!(b[i], b'.' | b'"' | b'\'') && !b[i].is_ascii_whitespace() {
                    i += 1;
                }
                cur.push_str(&h[start..i]);
                continue;
            }
        }
        i += 1;
    }
    parts.push(cur);
    parts
}

fn is_dependency_table(part: &str) -> bool {
    matches!(part, "dependencies" | "dev-dependencies" | "build-dependencies")
}

fn scan_cargo(text: &str, caps: &ScanCaps) -> Vec<RawDef> {
    let mut out: Vec<RawDef> = Vec::new();
    let mut section = Section::None;
    let mut depth = 0i32;
    let mut triple: Option<u8> = None;
    let mut seen_in_section: Vec<String> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if out.len() >= caps.max_defs {
            break;
        }
        let line_no = u32::try_from(n + 1).unwrap_or(u32::MAX);
        let t = line.trim();
        if depth == 0 && triple.is_none() && !t.is_empty() && !t.starts_with('#') {
            if t.starts_with('[') {
                seen_in_section.clear();
                section = Section::Other;
                if let Some(h) = header_text(t) {
                    let parts = header_parts(&h);
                    let last = parts.last().map_or("", String::as_str);
                    if parts.len() == 1 && last == "package" {
                        section = Section::Package;
                    } else if is_dependency_table(last) || (last == "dependencies" && parts.len() >= 2) {
                        section = Section::Deps;
                    } else if parts.len() >= 2 && parts.get(parts.len() - 2).is_some_and(|p| is_dependency_table(p)) && !last.is_empty() {
                        section = Section::DepTable;
                        out.push(def(last, Kind::Dependency, line_no));
                    }
                }
            } else if matches!(section, Section::Package | Section::Deps) {
                let b = t.as_bytes();
                let key = match b.first() {
                    Some(b'"' | b'\'') => quoted(b, 0),
                    _ => {
                        let end = b.iter().position(|c| c.is_ascii_whitespace() || *c == b'=' || *c == b'.').unwrap_or(b.len());
                        (end > 0).then(|| (t[..end].to_string(), end))
                    }
                };
                if let Some((key, after)) = key {
                    let rest = t[after..].trim_start();
                    if rest.starts_with('=') || rest.starts_with('.') {
                        let value = rest.split_once('=').map_or("", |(_, v)| v.trim_start());
                        if section == Section::Package {
                            if key == "name" {
                                if let Some((name, _)) = quoted(value.as_bytes(), 0) {
                                    out.push(def(&name, Kind::Package, line_no));
                                }
                            }
                        } else if !seen_in_section.contains(&key) {
                            out.push(def(&key, Kind::Dependency, line_no));
                            seen_in_section.push(key);
                            // A rename: `name = { package = "real-name", .. }` is also the real name.
                            if value.starts_with('{') {
                                if let Some(at) = value.find("package") {
                                    let after = value[at + "package".len()..].trim_start();
                                    if let Some(rest) = after.strip_prefix('=') {
                                        if let Some((real, _)) = quoted(rest.trim_start().as_bytes(), 0) {
                                            out.push(def(&real, Kind::Dependency, line_no));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        depth = (depth + bracket_delta(line, &mut triple)).max(0);
    }
    out.truncate(caps.max_defs);
    out
}

// ---- package.json ----

fn scan_package_json(bytes: &[u8], caps: &ScanCaps) -> Vec<RawDef> {
    let mut out: Vec<RawDef> = Vec::new();
    let mut depth = 0i32;
    let mut line = 1u32;
    let mut expect_name = false;
    let mut expect_deps = false;
    let mut deps_depth: Option<i32> = None;
    let mut i = 0usize;
    while i < bytes.len() && out.len() < caps.max_defs {
        match bytes[i] {
            b'\n' => {
                line = line.saturating_add(1);
                i += 1;
            }
            b'"' => {
                // A string; its newlines (not allowed in JSON, but possible in a broken file) count.
                let start = i;
                let mut j = i + 1;
                let mut text = Vec::new();
                while j < bytes.len() && bytes[j] != b'"' {
                    if bytes[j] == b'\\' && j + 1 < bytes.len() {
                        text.push(bytes[j + 1]);
                        j += 2;
                        continue;
                    }
                    text.push(bytes[j]);
                    j += 1;
                }
                let string_line = line;
                line = line.saturating_add(u32::try_from(bytes[start..j.min(bytes.len())].iter().filter(|&&b| b == b'\n').count()).unwrap_or(0));
                i = (j + 1).min(bytes.len());
                let mut k = i;
                while k < bytes.len() && matches!(bytes[k], b' ' | b'\t' | b'\r' | b'\n') {
                    k += 1;
                }
                let is_key = bytes.get(k) == Some(&b':');
                let text = String::from_utf8_lossy(&text).into_owned();
                if is_key {
                    if depth == 1 && text == "name" {
                        expect_name = true;
                    } else if depth == 1 && matches!(text.as_str(), "dependencies" | "devDependencies" | "peerDependencies" | "optionalDependencies") {
                        expect_deps = true;
                    } else if deps_depth == Some(depth) && !text.is_empty() {
                        out.push(def(&text, Kind::Dependency, string_line));
                    }
                } else if expect_name && depth == 1 {
                    if !text.is_empty() {
                        out.push(def(&text, Kind::Package, string_line));
                    }
                    expect_name = false;
                }
            }
            c @ (b'{' | b'[') => {
                if c == b'{' && expect_deps && depth == 1 {
                    deps_depth = Some(2);
                }
                expect_deps = false;
                expect_name = false;
                depth += 1;
                i += 1;
            }
            b'}' | b']' => {
                if deps_depth == Some(depth) {
                    deps_depth = None;
                }
                depth = (depth - 1).max(0);
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

#[cfg(test)]
#[path = "symbols_manifest_test.rs"]
mod symbols_manifest_test;
