//! The kit reads the targets a manifest declares, not only the ones cargo finds by their place
//! (X-DELTA-AZDRIVE-KEYS-16: `azul-bridge/src/tray_main.rs`, a `[[bin]]` with its own `path`, was
//! read by no gate - a gate cannot vouch for code that is not in the module tree it reads).

use std::path::PathBuf;

use azul_srcgate::{files, workspace, Orphans, Root};

/// A tree on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "azul-srcgate-targets-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    for (path, text) in files {
        let full = base.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
    base
}

const MANIFEST: &str =
    "[package]\nname = \"pkg\"\n\n[lib]\nname = \"pkg\"\npath = \"src/lib.rs\"\n\n\
    [[bin]]\nname = \"pkg-tray\"\npath = \"src/tray_main.rs\" # beside the lib\n\
    required-features = [\"tray\"]\n\n[[test]]\nname = \"t\"\npath = \"tests/t.rs\"\n";

fn tree(name: &str) -> PathBuf {
    base_with(
        name,
        &[
            ("pkg/Cargo.toml", MANIFEST),
            ("pkg/src/lib.rs", "pub fn a() {}\n"),
            ("pkg/src/tray_main.rs", "mod menu;\nfn main() {}\n"),
            ("pkg/src/menu.rs", "pub fn m() {}\n"),
            ("pkg/tests/t.rs", "#[test]\nfn t() {}\n"),
        ],
    )
}

fn names(read: &[azul_srcgate::Code]) -> Vec<&str> {
    let mut names: Vec<&str> = read.iter().map(|f| f.rel.as_str()).collect();
    names.sort_unstable();
    names
}

#[test]
fn a_bin_that_stands_where_its_manifest_says_is_read_with_its_modules() {
    let base = tree("package");
    // the package as the root, and its `src` folder as the root: the same files
    for dir in ["pkg", "pkg/src"] {
        let roots = [Root { dir, min_files: 1 }];
        let read = files(&base, &roots, Orphans::Fail)
            .unwrap_or_else(|wrong| panic!("{dir}: the declared bin is a target: {wrong:#?}"));
        assert_eq!(
            names(&read),
            ["pkg/src/lib.rs", "pkg/src/menu.rs", "pkg/src/tray_main.rs"],
            "{dir}"
        );
    }
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn without_the_table_the_same_files_are_files_no_build_compiles() {
    let base = tree("undeclared");
    std::fs::write(
        base.join("pkg/Cargo.toml"),
        "[package]\nname = \"pkg\"\n\n[lib]\npath = \"src/lib.rs\"\n",
    )
    .unwrap();
    let roots = [Root {
        dir: "pkg",
        min_files: 1,
    }];
    let wrong =
        files(&base, &roots, Orphans::Fail).expect_err("tray_main.rs is compiled by nobody");
    assert!(
        wrong.iter().any(|w| w.contains("tray_main.rs")),
        "{wrong:?}"
    );
    // left alone, the gate reads what the build compiles by its place
    let read = files(&base, &roots, Orphans::Ignore).expect("readable");
    assert_eq!(names(&read), ["pkg/src/lib.rs"]);
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_table_whose_path_names_no_file_is_no_target() {
    let base = tree("missing");
    std::fs::write(
        base.join("pkg/Cargo.toml"),
        "[package]\nname = \"pkg\"\n\n[[bin]]\nname = \"gone\"\npath = \"src/gone.rs\"\n",
    )
    .unwrap();
    let roots = [Root {
        dir: "pkg",
        min_files: 1,
    }];
    let read = files(&base, &roots, Orphans::Ignore).expect("readable");
    assert_eq!(names(&read), ["pkg/src/lib.rs"]);
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn the_tray_binary_of_the_bridge_is_read_by_the_gates_that_read_the_bridge() {
    for dir in ["examples/azul-bridge/src", "examples"] {
        let min_files = if dir == "examples" { 500 } else { 15 };
        let roots = [Root { dir, min_files }];
        let read = files(&workspace(), &roots, Orphans::Ignore)
            .unwrap_or_else(|wrong| panic!("{wrong:#?}"));
        assert!(
            read.iter()
                .any(|f| f.rel == "examples/azul-bridge/src/tray_main.rs"),
            "{dir}: tray_main.rs (a [[bin]] path) is among the compiled files"
        );
    }
}
