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
    let none = scan_manifest(ManifestKind::Requirements, "requirements.txt", b"\n# nothing\n", &ScanCaps::default());
    assert_eq!((none.lang, none.lines, none.defs.len()), (None, 2, 0));
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

#[test]
fn a_go_mod_names_its_module_and_what_it_requires_by_the_last_part_of_each_path() {
    let got = rows(
        ManifestKind::GoMod,
        &src(&[
            "module github.com/acme/store",
            "",
            "go 1.21",
            "",
            "require github.com/single/dep v1.0.0",
            "require (",
            "\tgithub.com/pkg/errors v0.9.1",
            "\tgolang.org/x/sys v0.1.0 // indirect",
            ")",
            "replace github.com/old/thing => ../thing",
        ]),
    );
    let want = vec![
        row("store", Kind::Package, 1),
        row("dep", Kind::Dependency, 5),
        row("errors", Kind::Dependency, 7),
        row("sys", Kind::Dependency, 8),
    ];
    assert_eq!(got, want);
}

#[test]
fn a_requirements_file_names_each_package_before_its_version_and_leaves_options_out() {
    let got = rows(
        ManifestKind::Requirements,
        &src(&[
            "# the pins",
            "requests==2.31.0",
            "numpy>=1.20 ; python_version > '3.8'",
            "flask[async]~=2.0",
            "-r other.txt",
            "-e .",
            "https://example.com/pkg.tar.gz",
            "",
            "PyYAML",
        ]),
    );
    let want = vec![
        row("requests", Kind::Dependency, 2),
        row("numpy", Kind::Dependency, 3),
        row("flask", Kind::Dependency, 4),
        row("PyYAML", Kind::Dependency, 9),
    ];
    assert_eq!(got, want);
}

#[test]
fn a_pyproject_names_its_project_and_its_dependencies_in_both_layouts() {
    let got = rows(
        ManifestKind::Pyproject,
        &src(&[
            "[project]",
            "name = \"my-tool\"",
            "dependencies = [",
            "  \"click>=8\",",
            "  \"rich\",",
            "]",
            "",
            "[tool.poetry.dependencies]",
            "python = \"^3.9\"",
            "httpx = \"^0.25\"",
            "",
            "[tool.black]",
            "line-length = 100",
        ]),
    );
    let want = vec![
        row("my-tool", Kind::Package, 2),
        row("click", Kind::Dependency, 4),
        row("rich", Kind::Dependency, 5),
        row("httpx", Kind::Dependency, 10),
    ];
    assert_eq!(got, want);
}

#[test]
fn a_cmake_file_names_its_project_its_targets_and_the_packages_it_looks_for() {
    let got = rows(
        ManifestKind::CMake,
        &src(&[
            "cmake_minimum_required(VERSION 3.20)",
            "project(Viewer VERSION 1.0 LANGUAGES CXX)",
            "# add_library(Commented STATIC a.cpp)",
            "find_package(Qt6 REQUIRED COMPONENTS Core)",
            "add_library(viewer_core STATIC a.cpp)",
            "add_executable(viewer main.cpp)",
            "target_link_libraries(viewer PRIVATE viewer_core)",
        ]),
    );
    let want = vec![
        row("Viewer", Kind::Package, 2),
        row("Qt6", Kind::Dependency, 4),
        row("viewer_core", Kind::Module, 5),
        row("viewer", Kind::Module, 6),
    ];
    assert_eq!(got, want);
}

#[test]
fn hostile_manifests_of_the_other_kinds_never_panic() {
    for text in ["module", "require (", "require", "[project", "dependencies = [", "project(", "add_library(", "find_package", "(((", "\"", "[[[["] {
        for kind in [ManifestKind::GoMod, ManifestKind::Requirements, ManifestKind::Pyproject, ManifestKind::CMake] {
            let _ = scan_manifest(kind, "m", text.as_bytes(), &ScanCaps::default());
        }
    }
    let junk: Vec<u8> = (0..20_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 11) as u8).collect();
    for kind in [ManifestKind::GoMod, ManifestKind::Requirements, ManifestKind::Pyproject, ManifestKind::CMake] {
        let _ = scan_manifest(kind, "m", &junk, &ScanCaps::default());
        let _ = scan_manifest(kind, "m", "x\n".repeat(200_000).as_bytes(), &ScanCaps::default());
    }
}
