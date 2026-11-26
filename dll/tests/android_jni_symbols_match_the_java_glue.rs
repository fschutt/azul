//! Every `native` method of the Java glue has the Rust symbol the JNI looks it up by, and every Rust
//! `Java_*` entry point is exported.
//!
//! JNI binds a Java `native` method to the C symbol `Java_<package>_<Class>_<method>` THE FIRST TIME the
//! method is called, and throws `UnsatisfiedLinkError` (uncaught, on the UI thread: the app dies) when
//! the symbol is not exported. A Rust function is exported only with `#[no_mangle]`; without it the
//! name is mangled and nothing says so until a phone calls the method. The shell had exactly that:
//! `nativeGetTextBeforeCursor` carried a doc comment and `#[no_mangle]` on the function before it (the
//! attribute twice on `nativeGetInputHints`), so the first soft keyboard that asked for the text before
//! the caret (every IME does, when a field takes the focus) killed the app. Nobody had run a soft
//! keyboard on a device.
//!
//! This test reads `scripts/android/*.java` and every `.rs` file of `dll/src` and checks, for each native
//! method: a `#[no_mangle]` `extern "system"` function of the JNI name exists, and takes the two JNI
//! arguments (environment, class) plus the Java ones. It needs nothing built:
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_jni_symbols_match_the_java_glue.rs -o /tmp/t && /tmp/t

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// A `native` method the Java glue declares.
#[derive(Debug)]
struct JavaNative {
    /// `com.azul.text.NativeTextBridge.nativeCommitText`
    name: String,
    /// The JNI symbol.
    symbol: String,
    /// The number of Java parameters.
    params: usize,
}

/// `_` in a Java name is `_1` in the symbol, and a nested class `Outer$Inner` is `Outer_00024Inner`.
fn jni_escape(part: &str) -> String {
    let mut out = String::new();
    for c in part.chars() {
        match c {
            '_' => out.push_str("_1"),
            '$' => out.push_str("_00024"),
            c => out.push(c),
        }
    }
    out
}

/// The number of top-level parameters of a parameter list text (between the parentheses).
fn count_params(list: &str) -> usize {
    let list = list.trim();
    if list.is_empty() {
        return 0;
    }
    let mut depth = 0i32;
    let mut commas = 0;
    for c in list.chars() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => commas += 1,
            _ => {}
        }
    }
    // a trailing comma (Rust) is not another parameter
    if list.trim_end().ends_with(',') {
        commas
    } else {
        commas + 1
    }
}

/// Strip `//` comments and `/* */` blocks from Java source (the doc comments mention `native`).
fn strip_java_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

fn java_natives() -> Vec<JavaNative> {
    let dir = root().join("scripts/android");
    let mut found = Vec::new();
    let mut files: Vec<_> = fs::read_dir(&dir)
        .expect("scripts/android exists")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "java"))
        .collect();
    files.sort();
    for file in files {
        let src = strip_java_comments(&fs::read_to_string(&file).unwrap());
        let package = src
            .lines()
            .find_map(|l| l.trim().strip_prefix("package ").map(|p| p.trim_end_matches(';').trim().to_string()))
            .unwrap_or_else(|| panic!("{} declares no package", file.display()));
        let file_class = file.file_stem().unwrap().to_string_lossy().to_string();
        // the nesting of classes at each `native`: a stack of (name, brace depth)
        let mut classes: Vec<(String, i32)> = Vec::new();
        let mut depth = 0i32;
        let tokens: Vec<&str> = src.split_inclusive(['{', '}', ';']).collect();
        for token in tokens {
            // a class declaration opens a brace
            if token.ends_with('{') {
                let head = token.trim_end_matches('{');
                if let Some(at) = head.rfind(" class ").or_else(|| head.rfind("\nclass ")).or_else(|| head.find("class ").filter(|&i| i == 0)) {
                    let after = head[at..].trim_start().trim_start_matches("class").trim();
                    let name: String = after.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                    if !name.is_empty() {
                        classes.push((name, depth + 1));
                    }
                }
                depth += 1;
            } else if token.ends_with('}') {
                if classes.last().is_some_and(|(_, d)| *d == depth) {
                    classes.pop();
                }
                depth -= 1;
            } else if token.ends_with(';') && token.contains(" native ") {
                let decl = token.trim().trim_end_matches(';');
                let open = decl.find('(').expect("a native method has parameters");
                let close = decl.rfind(')').expect("and a closing parenthesis");
                let name = decl[..open].split_whitespace().last().unwrap().to_string();
                let params = count_params(&decl[open + 1..close]);
                let class = if classes.is_empty() {
                    file_class.clone()
                } else {
                    classes.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join("$")
                };
                let symbol = format!(
                    "Java_{}_{}_{}",
                    package.split('.').map(jni_escape).collect::<Vec<_>>().join("_"),
                    jni_escape(&class),
                    jni_escape(&name)
                );
                found.push(JavaNative {
                    name: format!("{package}.{class}.{name}"),
                    symbol,
                    params,
                });
            }
        }
    }
    found
}

/// A Rust entry point: its parameter count and whether `#[no_mangle]` stands on it.
#[derive(Debug)]
struct RustEntry {
    file: String,
    params: usize,
    no_mangle: bool,
    extern_system: bool,
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn rust_entries() -> BTreeMap<String, Vec<RustEntry>> {
    let mut files = Vec::new();
    rust_files(&root().join("dll/src"), &mut files);
    files.sort();
    let mut entries: BTreeMap<String, Vec<RustEntry>> = BTreeMap::new();
    for file in files {
        let src = fs::read_to_string(&file).unwrap();
        let lines: Vec<&str> = src.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let Some(at) = line.find("fn Java_") else {
                continue;
            };
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            let name: String = line[at + 3..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            // the attributes: the lines above, up to the previous item, doc comments skipped
            let mut no_mangle = false;
            let mut j = i;
            while j > 0 {
                j -= 1;
                let above = lines[j].trim();
                if above.is_empty() || above.starts_with("///") || above.starts_with("//") {
                    continue;
                }
                if above.starts_with("#[") {
                    no_mangle |= above.contains("no_mangle");
                    continue;
                }
                break;
            }
            // the parameter list: from the first `(` after the name to its match
            let mut text = String::new();
            for l in &lines[i..] {
                // a comment inside the parameter list may hold commas
                let code = l.split("//").next().unwrap_or(l);
                text.push_str(code);
                text.push('\n');
                if code.contains('{') || code.trim_end().ends_with(';') {
                    break;
                }
            }
            let open = text.find('(').unwrap();
            let mut depth = 0;
            let mut close = open;
            for (k, c) in text[open..].char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = open + k;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            entries.entry(name).or_default().push(RustEntry {
                file: file.strip_prefix(root()).unwrap_or(&file).display().to_string(),
                params: count_params(&text[open + 1..close]),
                no_mangle,
                extern_system: text[..open].contains("extern \"system\""),
            });
        }
    }
    entries
}

#[test]
fn every_native_method_of_the_java_glue_has_an_exported_rust_symbol_with_the_right_arity() {
    let natives = java_natives();
    assert!(
        natives.len() > 40,
        "the Java glue declares dozens of native methods; found {} (is the parser reading them?)",
        natives.len()
    );
    let entries = rust_entries();
    let mut problems = Vec::new();
    for native in &natives {
        match entries.get(&native.symbol) {
            None => problems.push(format!(
                "{}: no Rust function {} (the JNI throws UnsatisfiedLinkError at the first call)",
                native.name, native.symbol
            )),
            Some(found) => {
                if !found.iter().any(|e| e.no_mangle && e.extern_system) {
                    problems.push(format!(
                        "{}: {} in {} is not `#[no_mangle] pub unsafe extern \"system\"`: its name is mangled and the JNI cannot find it",
                        native.name, native.symbol, found[0].file
                    ));
                }
                if !found.iter().any(|e| e.params == native.params + 2) {
                    problems.push(format!(
                        "{}: Java passes {} arguments, so {} must take {} (environment, class and those); it takes {:?}",
                        native.name,
                        native.params,
                        native.symbol,
                        native.params + 2,
                        found.iter().map(|e| e.params).collect::<Vec<_>>()
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

/// A Rust `Java_*` function that is not `#[no_mangle]` is dead whatever Java declares: it is exported by
/// nothing. (A function for a Java class an app's APK may not ship is fine; one that cannot be found is
/// not.)
#[test]
fn every_rust_jni_entry_point_is_exported() {
    let mut problems = Vec::new();
    for (name, found) in rust_entries() {
        for entry in found {
            if !entry.no_mangle {
                problems.push(format!("{name} in {} lacks #[no_mangle]", entry.file));
            }
            if !entry.extern_system {
                problems.push(format!("{name} in {} is not extern \"system\"", entry.file));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

#[test]
fn the_parser_reads_a_known_declaration() {
    assert_eq!(jni_escape("com.azul.text".replace('.', "_").as_str()), "com_1azul_1text");
    assert_eq!(count_params("long nativePtr, String text, int cursorPos"), 3);
    assert_eq!(count_params(""), 0);
    assert_eq!(count_params("a: Option<(u32, u32)>, b: i32,"), 2);
    let natives = java_natives();
    let n = natives
        .iter()
        .find(|n| n.name.ends_with("NativeTextBridge.nativeSetComposingText"))
        .expect("the IME bridge is read");
    assert_eq!(n.symbol, "Java_com_azul_text_NativeTextBridge_nativeSetComposingText");
    assert_eq!(n.params, 3);
    let nested = natives
        .iter()
        .find(|n| n.name.contains("nativeOnPermissionResult"))
        .expect("the permissions bridge is read");
    assert_eq!(nested.symbol, "Java_com_azul_permission_AzulPermissions_nativeOnPermissionResult");
}

