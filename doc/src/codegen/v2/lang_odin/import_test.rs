//! The import of the native library in the generated `azul.odin`, as each linker takes it.
//!
//! On Windows Odin passes the name of a `system:` library to the MSVC linker as it is. `foreign
//! import azul "system:azul"` reached `link.exe` as the bare name `azul`, which it reads as the
//! object file `azul.obj`: `LINK : fatal error LNK1181: cannot open input file 'azul.obj'` (the
//! Odin leg of the language board on windows-2022, website run 45738701245). With the import
//! library named `azul.lib` it builds and the counter runs (`test result: ok`, run 45754551758);
//! Linux and macOS keep the bare name, which their linkers extend (`-lazul`).

use std::sync::OnceLock;

use super::super::config::CodegenConfig;

fn generated() -> &'static str {
    static OUT: OnceLock<String> = OnceLock::new();
    OUT.get_or_init(|| {
        super::generate(crate::codegen::v2::bug_classes::ir(), &CodegenConfig::c_header()).expect("odin codegen")
    })
}

/// The lines of the file that import the library.
fn imports() -> Vec<&'static str> {
    generated().lines().filter(|l| l.trim_start().starts_with("foreign import ")).collect()
}

/// (the branch for Windows, the branch for the other systems) of the `when` that imports the library.
fn branches() -> (&'static str, &'static str) {
    let src = generated();
    let when = src.find("when ODIN_OS == .Windows {").expect("a branch for Windows");
    let other = when + src[when..].find("} else {").expect("a branch for the other systems");
    let end = other + src[other..].find("\n}\n").expect("the end of the `when`");
    (&src[when..other], &src[other..end])
}

#[test]
fn windows_imports_the_library_by_the_file_name_of_its_import_library() {
    let (windows, _) = branches();
    assert!(
        windows.contains("foreign import azul \"system:azul.lib\""),
        "link.exe takes a name without an extension for an object file (azul.obj); the Windows import is:\n{}",
        windows
    );
}

#[test]
fn the_other_systems_keep_the_bare_name() {
    let (_, other) = branches();
    assert!(other.contains("foreign import azul \"system:azul\""), "the other systems import:\n{}", other);
}

#[test]
fn the_file_imports_the_library_once_per_system_and_nowhere_else() {
    // One name for the foreign block that follows (`foreign azul { ... }`), two spellings of it.
    let lines = imports();
    assert_eq!(lines.len(), 2, "the imports of the file: {:?}", lines);
    assert!(lines.iter().all(|l| l.starts_with('\t')), "both imports sit inside their `when` branch: {:?}", lines);
}

fn repo_file(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// The commands of the Windows flow of the Odin install steps in api.json.
fn windows_install_steps() -> Vec<String> {
    fn odin(v: &serde_json::Value) -> Option<&serde_json::Value> {
        match v {
            serde_json::Value::Object(m) => {
                if m.get("displayName").and_then(|d| d.as_str()) == Some("Odin") && m.contains_key("install") {
                    return Some(v);
                }
                m.values().find_map(odin)
            }
            serde_json::Value::Array(a) => a.iter().find_map(odin),
            _ => None,
        }
    }
    let api: serde_json::Value = serde_json::from_str(&repo_file("api.json")).expect("api.json parses");
    let flows = odin(&api).expect("api.json has the Odin install steps")["install"].as_array().expect("flows");
    let windows = flows
        .iter()
        .find(|f| f["os"].as_array().is_some_and(|os| os.iter().any(|o| o == "windows")))
        .expect("a Windows flow of the Odin install steps");
    windows["steps"].as_array().expect("steps").iter().filter_map(|s| s["content"].as_str()).map(String::from).collect()
}

#[test]
fn the_windows_install_steps_download_the_import_library_as_azul_lib_and_build_plainly() {
    let steps = windows_install_steps();
    assert!(
        steps.iter().any(|c| c.starts_with("curl -o azul.lib ") && c.ends_with("/azul.dll.lib")),
        "the import library of the release (azul.dll.lib) must be saved as azul.lib, the name the generated import gives link.exe: {:?}",
        steps
    );
    let build = steps.iter().find(|c| c.starts_with("odin build")).expect("a build step");
    // `-extra-linker-flags:"-L."` is a unix linker's spelling; with any extra flag Odin also quotes its own library paths wrongly.
    assert_eq!(build, "odin build . -out:hello-world.exe", "the Windows build step");
}

#[test]
fn the_windows_leg_of_the_board_builds_the_way_the_install_steps_say() {
    let script = repo_file("scripts/e2e_language_matrix.sh");
    let from = script.find("\nlang_odin() {").expect("lang_odin in the language board");
    let leg = &script[from..from + script[from..].find("\n}\n").expect("end of lang_odin")];
    let windows = &leg[leg.find("elif [ \"$IS_WINDOWS\" = 1 ]").expect("a Windows branch")..];
    let windows = &windows[..windows.find("\n    else\n").expect("the end of the Windows branch")];
    assert!(windows.contains("azul.dll.lib\" ./azul.lib"), "the leg saves the import library as azul.lib:\n{}", windows);
    assert!(windows.contains("odin build . -out:hello-world-e2e.exe || exit 1"), "the leg builds plainly:\n{}", windows);
    let commands: Vec<&str> = windows.lines().filter(|l| !l.trim_start().starts_with('#')).collect();
    assert!(
        commands.iter().all(|l| !l.contains("-extra-linker-flags")),
        "no extra linker flags on Windows (odin-lang/Odin#2741):\n{}",
        windows
    );
    // The import library next to the program is all link.exe needs: no LIB variable to work around the bare name.
    assert!(
        commands.iter().all(|l| !l.contains("LIB=")),
        "the leg sets a LIB variable, the install steps do not:\n{}",
        windows
    );
}
