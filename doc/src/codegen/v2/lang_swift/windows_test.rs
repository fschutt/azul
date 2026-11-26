//! The `Azul` module on Windows: a static library, not a DLL.
//!
//! The Swift compiler writes an export directive into the object of every `public` declaration of
//! a module and of the symbols behind it (the getter, the setter and the modify accessor of a
//! property, the metadata and the witness tables of a type, the forwarders of its closures), and
//! the linker puts each of them into the export table of a DLL, which holds 65,535 at most. The 49
//! objects of the generated module carry 76,781 of them for its 32,226 `public` lines (Swift 6.3.2
//! on windows-2022, run 45754551758), so `swiftc -emit-library ... -o AzulSwift.dll` ends in
//! `LINK : fatal error LNK1189: library limit of 65535 objects exceeded` - the Swift leg of the
//! language board, website run 45738701245, job 139680251909, and the same two commands stood in
//! the guide and in the install steps of api.json. Built with `-static` the objects carry none, and
//! an archive has no export table: the counter test passes against it on windows-2022.
//!
//! The driver archives with `link /LIB /NOLOGO /OUT:...`, found by name on `PATH`; in the Git
//! Bash that is Git's `link`, a coreutils command ("/usr/bin/link: extra operand
//! '/OUT:AzulSwift.lib'"). `-use-ld=lld-link` makes it the LLVM linker of the Swift toolchain,
//! which acts as `lib.exe` for `/LIB` (`-use-ld=llvm-lib` does not: it reads `/LIB` as a file).
//!
//! These tests fail while the module is past what a DLL exports and one of the places that tell a
//! user how to build it on Windows still builds a DLL.

use std::sync::OnceLock;

use super::super::config::CodegenConfig;

/// The name api.json gives the Windows flows of the install steps. (A constant: the emitter lint in
/// bug_classes.rs reads a literal `"windows"` that is compared as an API method name, and `windows` is one.)
const WINDOWS: &str = "windows";

/// What one DLL can export (its export ordinals are 16 bits).
const DLL_EXPORT_LIMIT: usize = 65_535;

/// Export directives per `public` line of the module's sources: 2.38 measured (76,781 for 32,226),
/// taken a little under so that a small change of the API does not switch the premise.
const EXPORTS_PER_PUBLIC_LINE: f64 = 2.3;

/// Every file the Swift binding writes, from the real api.json.
fn generated() -> &'static Vec<(String, String)> {
    static OUT: OnceLock<Vec<(String, String)>> = OnceLock::new();
    OUT.get_or_init(|| {
        super::generate_files(crate::codegen::v2::bug_classes::ir(), &CodegenConfig::c_header()).expect("swift codegen")
    })
}

fn generated_file(path: &str) -> &'static str {
    generated()
        .iter()
        .find(|(p, _)| p == path)
        .map(|(_, s)| s.as_str())
        .unwrap_or_else(|| panic!("the Swift binding writes no {}", path))
}

fn repo_file(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// The `public` lines of the sources of the `Azul` target.
fn public_lines() -> usize {
    generated()
        .iter()
        .filter(|(p, _)| p.starts_with("swift/Sources/Azul/"))
        .flat_map(|(_, src)| src.lines())
        .filter(|l| l.trim_start().starts_with("public "))
        .count()
}

/// Does the module export more than a DLL can hold?
fn past_the_dll_limit() -> bool {
    (public_lines() as f64 * EXPORTS_PER_PUBLIC_LINE) as usize >= DLL_EXPORT_LIMIT
}

/// The first fenced block after the line that contains `marker`.
fn fenced_block_after(text: &str, marker: &str) -> String {
    let from = text.find(marker).unwrap_or_else(|| panic!("no `{}` in:\n{}", marker, text));
    let rest = &text[from..];
    let open = rest.find("```").unwrap_or_else(|| panic!("no code block after `{}`", marker));
    let body = &rest[open + 3..];
    let body = &body[body.find('\n').expect("a line after the fence") + 1..];
    body[..body.find("```").expect("the end of the code block")].to_string()
}

/// A place that tells the user how to build the module on Windows must build a static library,
/// archived by the toolchain's own linker.
fn assert_static_recipe(place: &str, commands: &str) {
    assert!(
        !commands.contains("-o AzulSwift.dll"),
        "{} builds AzulSwift.dll: the module exports {:.0} symbols, one DLL holds {} (LNK1189):\n{}",
        place,
        public_lines() as f64 * EXPORTS_PER_PUBLIC_LINE,
        DLL_EXPORT_LIMIT,
        commands
    );
    for needle in ["-static", "-use-ld=lld-link", "AzulSwift.lib"] {
        assert!(commands.contains(needle), "{} does not say `{}` on Windows:\n{}", place, needle, commands);
    }
}

#[test]
fn the_estimate_counts_the_public_lines_of_the_module() {
    let n = public_lines();
    // 32,226 on 2026-11-26. An API this size is never a third of that.
    assert!(n > 10_000, "only {} `public` lines found: the count does not read the module", n);
    assert!(
        (32_226f64 * EXPORTS_PER_PUBLIC_LINE) as usize >= DLL_EXPORT_LIMIT,
        "the premise of these tests does not hold at the measured size"
    );
}

#[test]
fn the_guide_builds_a_static_library_on_windows() {
    if !past_the_dll_limit() {
        return;
    }
    let guide = repo_file("doc/guide/en/hello-world/swift.md");
    let block = fenced_block_after(&guide, "\nWindows (");
    assert_static_recipe("doc/guide/en/hello-world/swift.md", &block);
}

#[test]
fn the_windows_install_steps_build_a_static_library() {
    if !past_the_dll_limit() {
        return;
    }
    fn swift(v: &serde_json::Value) -> Option<&serde_json::Value> {
        match v {
            serde_json::Value::Object(m) => {
                if m.get("displayName").and_then(|d| d.as_str()) == Some("Swift") && m.contains_key("install") {
                    return Some(v);
                }
                m.values().find_map(swift)
            }
            serde_json::Value::Array(a) => a.iter().find_map(swift),
            _ => None,
        }
    }
    let api: serde_json::Value = serde_json::from_str(&repo_file("api.json")).expect("api.json parses");
    let flows = swift(&api).expect("api.json has the Swift install steps")["install"].as_array().expect("flows");
    let windows = flows
        .iter()
        .find(|f| f["os"].as_array().is_some_and(|os| os.iter().any(|o| o == WINDOWS)))
        .expect("a Windows flow of the Swift install steps");
    let steps: Vec<&str> = windows["steps"].as_array().expect("steps").iter().filter_map(|s| s["content"].as_str()).collect();
    let module = steps
        .iter()
        .find(|c| c.starts_with("swiftc -emit-library"))
        .unwrap_or_else(|| panic!("the Windows flow has no step that builds the module: {:?}", steps));
    assert_static_recipe("the Windows install steps of api.json", module);
}

#[test]
fn the_windows_leg_of_the_board_builds_a_static_library() {
    if !past_the_dll_limit() {
        return;
    }
    let script = repo_file("scripts/e2e_language_matrix.sh");
    let from = script.find("\nlang_swift() {").expect("lang_swift in the language board");
    let leg = &script[from..from + script[from..].find("\n}\n").expect("end of lang_swift")];
    let windows = &leg[leg.find("elif [ \"$IS_WINDOWS\" = 1 ]").expect("a Windows branch in lang_swift")..];
    let windows = &windows[..windows.find("\n    else\n").expect("the end of the Windows branch")];
    assert_static_recipe("the Windows branch of lang_swift in scripts/e2e_language_matrix.sh", windows);
}

#[test]
fn the_package_readme_says_how_to_build_it_on_windows() {
    if !past_the_dll_limit() {
        return;
    }
    let readme = generated_file("swift/README.md");
    let block = fenced_block_after(readme, "On Windows build the module as a static library");
    assert_static_recipe("the README of the Swift package", &block);
}

#[test]
fn the_header_of_the_one_file_module_says_how_to_build_it_on_windows() {
    if !past_the_dll_limit() {
        return;
    }
    let src = generated_file("azul.swift");
    let header = &src[..src.find("\nimport CAzul").expect("the import of CAzul")];
    let from = header.find("// On Windows").expect("a Windows build in the header of azul.swift");
    let commands: String = header[from..].lines().filter(|l| l.starts_with("//   ")).collect::<Vec<_>>().join("\n");
    assert_static_recipe("the header of azul.swift", &commands);
}
