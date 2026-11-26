//! What the manifests name: the package a folder is and the packages it depends on. No window, no
//! azul.

use super::*;
use crate::symbols::Kind;
use crate::symbols_scan::ScanCaps;
use crate::symbols_source::ManifestKind;

fn src(lines: &[&str]) -> String {
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

fn rows(kind: ManifestKind, text: &str) -> Vec<(String, Kind, u32)> {
    scan_manifest(kind, "x/manifest", text.as_bytes(), &ScanCaps::default())
        .defs
        .into_iter()
        .map(|d| (d.name, d.kind, d.line))
        .collect()
}

fn row(name: &str, kind: Kind, line: u32) -> (String, Kind, u32) {
    (name.to_string(), kind, line)
}

#[test]
fn a_cargo_manifest_names_its_package_and_every_dependency_with_the_line_of_each() {
    let got = rows(
        ManifestKind::Cargo,
        &src(&[
            "[package]",
            "name = \"azul-core\"",
            "version = \"0.1.0\"",
            "",
            "[dependencies]",
            "serde = { version = \"1\", features = [\"derive\"] }",
            "azul-css = { path = \"../css\" }",
            "renamed = { package = \"real-name\", version = \"2\" }",
            "multi = { version = \"1\",",
            "        features = [\"a\", \"b\"] }",
            "tiny = \"1\"",
            "quoted-key = \"1\"",
            "\"also-quoted\" = \"2\"",
            "",
            "[dev-dependencies]",
            "tempfile = \"3\"",
            "",
            "[dependencies.table-form]",
            "version = \"1\"",
            "",
            "[target.'cfg(unix)'.dependencies]",
            "libc = \"0.2\"",
            "",
            "[workspace.dependencies]",
            "shared = { version = \"1\" }",
            "",
            "[features]",
            "default = [\"serde\"]",
            "extra = []",
            "",
            "[[bin]]",
            "name = \"tool\"",
        ]),
    );
    let want = vec![
        row("azul-core", Kind::Package, 2),
        row("serde", Kind::Dependency, 6),
        row("azul-css", Kind::Dependency, 7),
        row("renamed", Kind::Dependency, 8),
        row("real-name", Kind::Dependency, 8),
        row("multi", Kind::Dependency, 9),
        row("tiny", Kind::Dependency, 11),
        row("quoted-key", Kind::Dependency, 12),
        row("also-quoted", Kind::Dependency, 13),
        row("tempfile", Kind::Dependency, 16),
        row("table-form", Kind::Dependency, 18),
        row("libc", Kind::Dependency, 22),
        row("shared", Kind::Dependency, 25),
    ];
    assert_eq!(got, want);
}

#[test]
fn a_cargo_comment_or_a_string_with_brackets_changes_nothing() {
    let got = rows(
        ManifestKind::Cargo,
        &src(&[
            "[package] # the package",
            "name = \"a]b[c\" # name = \"not\"",
            "description = \"[dependencies]\"",
            "[dependencies]",
            "# commented = \"1\"",
            "real = \"1\"",
        ]),
    );
    assert_eq!(got, vec![row("a]b[c", Kind::Package, 2), row("real", Kind::Dependency, 6)]);
}

#[test]
fn a_package_json_names_its_package_and_the_keys_of_its_dependency_objects() {
    let got = rows(
        ManifestKind::PackageJson,
        &src(&[
            "{",
            "  \"name\": \"@scope/web-app\",",
            "  \"version\": \"1.0.0\",",
            "  \"scripts\": { \"build\": \"tsc\" },",
            "  \"dependencies\": {",
            "    \"react\": \"^18\",",
            "    \"left-pad\": \"1.0.0\"",
            "  },",
            "  \"devDependencies\": { \"typescript\": \"^5\" }",
            "}",
        ]),
    );
    let want = vec![
        row("@scope/web-app", Kind::Package, 2),
        row("react", Kind::Dependency, 6),
        row("left-pad", Kind::Dependency, 7),
        row("typescript", Kind::Dependency, 9),
    ];
    assert_eq!(got, want);
}

#[test]
fn a_manifest_reports_its_size_lines_and_no_language() {
    let s = scan_manifest(ManifestKind::Cargo, "Cargo.toml", b"[package]\nname = \"a\"\n", &ScanCaps::default());
    assert_eq!((s.lang, s.size, s.lines), (None, 21, 2));
    assert!(!s.test_file);
    let other = scan_manifest(ManifestKind::GoMod, "go.mod", b"module x\n", &ScanCaps::default());
    assert_eq!((other.lang, other.lines), (None, 1));
}

#[test]
fn only_as_many_names_as_the_cap_are_kept() {
    let text: String = std::iter::once("[dependencies]\n".to_string()).chain((0..50).map(|i| format!("dep{i} = \"1\"\n"))).collect();
    let caps = ScanCaps { max_defs: 5, ..ScanCaps::default() };
    assert_eq!(scan_manifest(ManifestKind::Cargo, "Cargo.toml", text.as_bytes(), &caps).defs.len(), 5);
}

#[test]
fn hostile_manifests_never_panic() {
    for text in [
        "[".to_string(),
        "[[".to_string(),
        "[package".to_string(),
        "name = ".to_string(),
        "name = \"never closed".to_string(),
        "[dependencies]\n\"".to_string(),
        "{".repeat(100_000),
        "[".repeat(100_000),
        "{\"name\":".to_string(),
        "{\"dependencies\":{\"a\"".to_string(),
        "\"".repeat(1_001),
        "[dependencies]\n".to_string() + &"a = {\n".repeat(50_000),
    ] {
        for kind in [ManifestKind::Cargo, ManifestKind::PackageJson] {
            let _ = scan_manifest(kind, "m", text.as_bytes(), &ScanCaps::default());
        }
    }
    let junk: Vec<u8> = (0..20_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 11) as u8).collect();
    let _ = scan_manifest(ManifestKind::Cargo, "m", &junk, &ScanCaps::default());
    let _ = scan_manifest(ManifestKind::PackageJson, "m", &junk, &ScanCaps::default());
}
