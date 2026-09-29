//! AzBuilder PROJECTS: a folder on disk that the builder UI browses, edits and
//! saves into, driven end to end through the REAL op dispatch
//! (`process_debug_event` + the scenario runner) on a headless window.
//!
//! A project is a folder:
//!
//! ```text
//! <root>/azul-project.json              the manifest
//! <root>/document.json                  the builder document
//! <root>/components/<library>/<name>.json   one file per user component
//! <root>/styles/*.css                   stylesheets applied to the document
//! <root>/tests/*.json, snapshots/*.json, export/<language>/…
//! ```
//!
//! Every scenario checks the op's answer AND the disk (read back here with
//! `std::fs`), and the ones that apply a file (a stylesheet, a component, a
//! document) also check the NATIVE window (`assert_layout` / `assert_dom` read
//! the live `StyledDom`), because "saved" is only half of "saving a stylesheet
//! re-applies it".
//!
//! Path safety is the other half: every path an op takes is RELATIVE to the
//! project root, and nothing may reach outside it — not `..`, not an absolute
//! path, not a symlink that leads out, not a zip entry.

use std::path::{Path, PathBuf};

use super::{run_e2e_test, E2eTest, E2eTestResult};

/// Run `steps` as one headless scenario on a 400x300 window.
fn run(name: &str, continue_on_failure: bool, steps: Vec<serde_json::Value>) -> E2eTestResult {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": continue_on_failure },
        "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    run_e2e_test(&test)
}

/// Every failing step, one line each — the assertion message a red run prints.
fn failures(result: &E2eTestResult) -> String {
    result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| {
            format!(
                "  step {} `{}`: {}",
                s.step_index,
                s.op,
                s.error.clone().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_passes(result: &E2eTestResult) {
    assert!(
        result.status == "pass" && result.steps_failed == 0,
        "scenario '{}' failed:\n{}",
        result.name,
        failures(result)
    );
}

/// `(status, error)` of step `i`.
fn step(result: &E2eTestResult, i: usize) -> (String, String) {
    result
        .steps
        .iter()
        .find(|s| s.step_index == i)
        .map(|s| (s.status.clone(), s.error.clone().unwrap_or_default()))
        .unwrap_or_else(|| ("missing".to_string(), String::new()))
}

/// The frame barrier every scenario in `e2e/` puts between a DOM change and
/// the assertions about it.
fn settle() -> [serde_json::Value; 2] {
    [
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
    ]
}

/// A fresh, not-yet-existing directory under the system temp dir. Removed
/// again when the guard drops (best effort: a failing test leaves it behind
/// for inspection only if the removal fails).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "azb-{tag}-{}-{}-{nanos}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn str(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

fn read_json(p: &Path) -> serde_json::Value {
    serde_json::from_str(&read(p)).unwrap_or_else(|e| panic!("{} is not JSON: {e}", p.display()))
}

fn write(p: &Path, s: &str) {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).expect("create the parent directory");
    }
    std::fs::write(p, s).expect("write the fixture file");
}

/// A zip archive of `(path, content)` entries, base64 — what the UI uploads.
fn zip_base64(entries: &[(&str, &str)]) -> String {
    let zip = crate::zip::ZipFile {
        entries: entries
            .iter()
            .map(|(p, c)| crate::zip::ZipFileEntry::file(*p, c.as_bytes().to_vec()))
            .collect(),
    };
    let bytes = zip
        .to_bytes(&crate::zip::ZipWriteConfig::default())
        .expect("build the zip fixture");
    crate::callbacks::base64_encode(&bytes)
}

/// Standard base64, padding and whitespace tolerated (for reading back a
/// `data:` URI the server answered).
fn base64_decode(s: &str) -> Vec<u8> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in s.bytes() {
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        let v = T.iter().position(|&t| t == b).expect("a base64 character") as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

#[test]
fn a_project_folder_is_created_listed_and_its_files_are_read_written_renamed_and_deleted() {
    let root = TempDir::new("project-files");
    let result = run(
        "project_files",
        false,
        vec![
            serde_json::json!({ "op": "project_open", "path": root.str(), "create": true }),
            serde_json::json!({ "op": "assert_response", "contains": "\"created\":true" }),
            // The skeleton of a new project, as the tree the UI renders.
            serde_json::json!({ "op": "assert_response", "contains": "\"path\":\"azul-project.json\"" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"path\":\"components\"" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"path\":\"styles\"" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"path\":\"tests\"" }),
            serde_json::json!({ "op": "project_write_file", "path": "styles/app.css",
                                "content": "body { color: #123456; }" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"written\":true" }),
            serde_json::json!({ "op": "project_read_file", "path": "styles/app.css" }),
            serde_json::json!({ "op": "assert_response", "contains": "#123456" }),
            // A folder, a file in it (parents are created on write).
            serde_json::json!({ "op": "project_create", "path": "notes", "directory": true }),
            serde_json::json!({ "op": "project_create", "path": "notes/a.txt", "content": "hello" }),
            serde_json::json!({ "op": "project_rename", "from": "notes/a.txt", "to": "notes/b.txt" }),
            serde_json::json!({ "op": "project_list" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"path\":\"notes/b.txt\"" }),
            serde_json::json!({ "op": "assert_response", "not_contains": "\"path\":\"notes/a.txt\"" }),
            serde_json::json!({ "op": "project_read_file", "path": "notes/b.txt" }),
            serde_json::json!({ "op": "assert_response", "contains": "hello" }),
            serde_json::json!({ "op": "project_delete", "path": "notes" }),
            serde_json::json!({ "op": "project_list" }),
            serde_json::json!({ "op": "assert_response", "not_contains": "\"path\":\"notes" }),
            serde_json::json!({ "op": "project_info" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"open\":true" }),
            serde_json::json!({ "op": "project_close" }),
            serde_json::json!({ "op": "project_info" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"open\":false" }),
        ],
    );
    assert_passes(&result);

    let manifest = read_json(&root.path().join("azul-project.json"));
    assert_eq!(manifest["format"], "azul-project", "the manifest names its format");
    assert!(root.path().join("components").is_dir());
    assert!(root.path().join("styles").is_dir());
    assert!(root.path().join("tests").is_dir());
    assert!(root.path().join("snapshots").is_dir());
    assert_eq!(read(&root.path().join("styles/app.css")), "body { color: #123456; }");
    assert!(!root.path().join("notes").exists(), "project_delete removes the folder");
}

/// The editor's "New file" and "Rename" must never clobber a file.
#[test]
fn creating_a_file_that_exists_is_refused_and_the_file_is_kept() {
    let root = TempDir::new("project-create");
    let result = run(
        "project_create_exists",
        true,
        vec![
            /* 0 */ serde_json::json!({ "op": "project_open", "path": root.str(), "create": true }),
            /* 1 */ serde_json::json!({ "op": "project_create", "path": "a.txt", "content": "one" }),
            /* 2 */ serde_json::json!({ "op": "project_create", "path": "a.txt", "content": "two" }),
            /* 3 */ serde_json::json!({ "op": "project_rename", "from": "a.txt", "to": "azul-project.json" }),
        ],
    );
    for i in [0, 1] {
        assert_eq!(step(&result, i).0, "pass", "step {i}:\n{}", failures(&result));
    }
    for i in [2, 3] {
        let (st, err) = step(&result, i);
        assert_eq!(st, "fail", "step {i} must be refused:\n{}", failures(&result));
        assert!(err.contains("exists"), "step {i} must say the target exists: {err}");
    }
    assert_eq!(read(&root.path().join("a.txt")), "one");
}

#[test]
fn every_path_that_leaves_the_project_root_is_refused_and_nothing_outside_is_touched() {
    let root = TempDir::new("project-root");
    let outside = TempDir::new("project-outside");
    write(&outside.path().join("secret.txt"), "secret");
    let outside_name = outside
        .path()
        .file_name()
        .expect("a temp dir has a name")
        .to_string_lossy()
        .into_owned();
    let absolute = outside.path().join("abs.txt").to_string_lossy().into_owned();

    let result = run(
        "project_traversal",
        true,
        vec![
            /* 0: nothing is open yet */
            serde_json::json!({ "op": "project_read_file", "path": "azul-project.json" }),
            /* 1 */
            serde_json::json!({ "op": "project_open", "path": root.str(), "create": true }),
            /* 2: up and over into the neighbour */
            serde_json::json!({ "op": "project_read_file",
                                "path": format!("../{outside_name}/secret.txt") }),
            /* 3: an absolute path */
            serde_json::json!({ "op": "project_write_file", "path": absolute, "content": "x" }),
            /* 4: `..` hidden in the middle */
            serde_json::json!({ "op": "project_write_file", "path": "styles/../../evil.txt",
                                "content": "x" }),
            /* 5: a rename target outside */
            serde_json::json!({ "op": "project_rename", "from": "azul-project.json",
                                "to": "../stolen.json" }),
            /* 6: the root itself */
            serde_json::json!({ "op": "project_delete", "path": "" }),
            /* 7 */
            serde_json::json!({ "op": "project_delete", "path": "." }),
            /* 8 */
            serde_json::json!({ "op": "project_create", "path": "..", "directory": true }),
            /* 9: a NUL byte */
            serde_json::json!({ "op": "project_write_file", "path": "a\u{0}b.txt", "content": "x" }),
            /* 10: backslashes are separators too */
            serde_json::json!({ "op": "project_write_file", "path": "styles\\..\\..\\evil2.txt",
                                "content": "x" }),
            /* 11: the project still works */
            serde_json::json!({ "op": "project_list" }),
        ],
    );
    let (st, err) = step(&result, 0);
    assert_eq!(st, "fail", "a file op before project_open must be refused");
    assert!(err.contains("no project is open"), "step 0 must say why: {err}");
    for i in [1, 11] {
        assert_eq!(step(&result, i).0, "pass", "step {i}:\n{}", failures(&result));
    }
    for (i, needle) in [
        (2, ".."),
        (3, "absolute"),
        (4, ".."),
        (5, ".."),
        (6, "root"),
        (7, "root"),
        (8, ".."),
        (9, "NUL"),
        (10, ".."),
    ] {
        let (st, err) = step(&result, i);
        assert_eq!(st, "fail", "step {i} must be refused:\n{}", failures(&result));
        assert!(
            err.contains(needle),
            "step {i}'s refusal must say why (expected '{needle}' in: {err})"
        );
    }

    assert_eq!(read(&outside.path().join("secret.txt")), "secret");
    assert!(!outside.path().join("abs.txt").exists());
    let parent = root.path().parent().expect("the temp dir has a parent");
    assert!(!parent.join("evil.txt").exists());
    assert!(!parent.join("evil2.txt").exists());
    assert!(!parent.join("stolen.json").exists());
    assert!(root.path().join("azul-project.json").is_file(), "the manifest survived");
}

/// A symlink INSIDE the project that points OUTSIDE is a way out, whatever
/// the path spells: the check is on where the path really leads.
#[cfg(unix)]
#[test]
fn a_symlink_that_leads_out_of_the_project_is_refused() {
    let root = TempDir::new("project-symlink");
    let outside = TempDir::new("project-symlink-out");
    write(&outside.path().join("secret.txt"), "secret");
    std::fs::create_dir_all(root.path()).expect("create the project folder");
    std::os::unix::fs::symlink(outside.path(), root.path().join("escape"))
        .expect("create the symlink fixture");
    std::os::unix::fs::symlink(
        outside.path().join("secret.txt"),
        root.path().join("secret-link.txt"),
    )
    .expect("create the file symlink fixture");

    let result = run(
        "project_symlink",
        true,
        vec![
            /* 0 */ serde_json::json!({ "op": "project_open", "path": root.str(), "create": true }),
            /* 1 */ serde_json::json!({ "op": "project_read_file", "path": "escape/secret.txt" }),
            /* 2 */ serde_json::json!({ "op": "project_write_file", "path": "escape/new.txt",
                                        "content": "x" }),
            /* 3 */ serde_json::json!({ "op": "project_read_file", "path": "secret-link.txt" }),
            /* 4 */ serde_json::json!({ "op": "project_write_file", "path": "secret-link.txt",
                                        "content": "overwritten" }),
            /* 5 */ serde_json::json!({ "op": "project_delete", "path": "escape/secret.txt" }),
            /* 6: the listing shows the links but never walks into them */
            serde_json::json!({ "op": "project_list" }),
            /* 7 */ serde_json::json!({ "op": "assert_response", "contains": "\"path\":\"escape\"" }),
            /* 8 */ serde_json::json!({ "op": "assert_response", "not_contains": "escape/secret.txt" }),
        ],
    );
    for i in [0, 6, 7, 8] {
        assert_eq!(step(&result, i).0, "pass", "step {i}:\n{}", failures(&result));
    }
    for i in 1..=5 {
        let (st, err) = step(&result, i);
        assert_eq!(st, "fail", "step {i} must be refused:\n{}", failures(&result));
        assert!(err.contains("outside"), "step {i} must say it leads outside: {err}");
    }
    assert_eq!(read(&outside.path().join("secret.txt")), "secret");
    assert!(!outside.path().join("new.txt").exists());
}

#[test]
fn saving_the_project_writes_the_builder_document_and_one_file_per_user_component() {
    let root = TempDir::new("project-save");
    let result = run(
        "project_save",
        false,
        vec![
            serde_json::json!({ "op": "project_open", "path": root.str(), "create": true }),
            // uid 1: the card, uid 2: its heading.
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "div",
                                "attrs": { "class": "card" } }),
            serde_json::json!({ "op": "builder_insert", "parent": 1, "component": "h1",
                                "attrs": { "text": "Title" } }),
            serde_json::json!({ "op": "builder_convert_to_component", "node": 1,
                                "library": "user", "name": "card" }),
            // uid 3.
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "p",
                                "attrs": { "id": "hello", "text": "Hello" } }),
            serde_json::json!({ "op": "project_save" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"document.json\"" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"components/user/card.json\"" }),
        ],
    );
    assert_passes(&result);

    let doc = read_json(&root.path().join("document.json"));
    assert_eq!(doc["format"], "azul-builder-document");
    let body = &doc["root"];
    assert_eq!(body["tag"], "body");
    let kids = body["children"].as_array().expect("the body has children");
    assert_eq!(kids.len(), 2, "the instance and the paragraph: {doc}");
    assert_eq!(kids[0]["kind"], "component");
    assert_eq!(kids[0]["library"], "user");
    assert_eq!(kids[0]["tag"], "card");
    assert_eq!(kids[1]["tag"], "p");
    assert_eq!(kids[1]["attrs"]["text"], "Hello");
    assert!(
        !read(&root.path().join("document.json")).contains("\"uid\""),
        "uids are the session's, not the file's"
    );

    let card = read_json(&root.path().join("components/user/card.json"));
    assert_eq!(card["format"], "azul-component");
    assert_eq!(card["library"], "user");
    assert_eq!(card["name"], "card");
    let template = card["template"].as_str().expect("a converted component has a template");
    assert!(template.contains("{text}"), "the template keeps its placeholder: {template}");
    let fields = card["fields"].as_array().expect("fields");
    assert!(
        fields.iter().any(|f| f["name"] == "text" && f["default"] == "Title"),
        "the inferred parameter and its default: {card}"
    );
}

#[test]
fn loading_a_project_restores_its_components_stylesheets_and_document_in_the_native_window() {
    let root = TempDir::new("project-load");
    write(
        &root.path().join("azul-project.json"),
        r#"{ "format": "azul-project", "version": 1, "name": "Loaded" }"#,
    );
    write(
        &root.path().join("components/lib5/badge.json"),
        r#"{
            "format": "azul-component", "version": 1,
            "library": "lib5", "name": "badge", "display_name": "Badge",
            "description": "A badge",
            "fields": [ { "name": "text", "type": "String", "default": "New" } ],
            "css": ".badge { color: #ff0000; }",
            "template": "<span class=\"badge\">{text}</span>"
        }"#,
    );
    write(
        &root.path().join("styles/app.css"),
        "#box { width: 123px; height: 20px; }",
    );
    write(
        &root.path().join("document.json"),
        r#"{
            "format": "azul-builder-document", "version": 1,
            "root": { "kind": "element", "tag": "body", "attrs": {}, "children": [
                { "kind": "element", "tag": "div", "attrs": { "id": "box" }, "children": [] },
                { "kind": "element", "tag": "p", "attrs": { "id": "hello", "text": "Hi there" },
                  "children": [] },
                { "kind": "component", "library": "lib5", "tag": "badge",
                  "attrs": { "text": "Loaded" }, "children": [] }
            ] }
        }"#,
    );
    let [wf, w] = settle();
    let result = run(
        "project_load",
        false,
        vec![
            serde_json::json!({ "op": "project_open", "path": root.str() }),
            serde_json::json!({ "op": "assert_response", "contains": "\"name\":\"Loaded\"" }),
            serde_json::json!({ "op": "project_load" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"lib5:badge\"" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"styles/app.css\"" }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_exists", "selector": "#hello" }),
            serde_json::json!({ "op": "assert_dom", "contains": "Hi there" }),
            serde_json::json!({ "op": "assert_node_count", "selector": ".badge", "expected": 1 }),
            serde_json::json!({ "op": "assert_dom", "contains": "Loaded" }),
            // The project stylesheet reached the window.
            serde_json::json!({ "op": "assert_layout", "selector": "#box", "property": "width",
                                "expected": 123, "tolerance": 1 }),
            // The component is registered: the palette lists it.
            serde_json::json!({ "op": "get_library_components", "library": "lib5" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"tag\":\"badge\"" }),
            // The document is the builder's now: it edits on from here.
            serde_json::json!({ "op": "builder_get_document" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"active\":true" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"can_undo\":false" }),
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "p",
                                "attrs": { "id": "after", "text": "Added" } }),
            wf,
            w,
            serde_json::json!({ "op": "assert_exists", "selector": "#after" }),
            serde_json::json!({ "op": "assert_exists", "selector": "#hello" }),
        ],
    );
    assert_passes(&result);
}

#[test]
fn saving_a_stylesheet_re_applies_it_to_the_native_window() {
    let root = TempDir::new("project-css");
    let [wf, w] = settle();
    let result = run(
        "project_stylesheet",
        false,
        vec![
            serde_json::json!({ "op": "project_open", "path": root.str(), "create": true }),
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "div",
                                "attrs": { "id": "box" } }),
            serde_json::json!({ "op": "project_write_file", "path": "styles/a-base.css",
                                "content": "#box { width: 50px; height: 20px; }" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"applied\":\"stylesheet\"" }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_layout", "selector": "#box", "property": "width",
                                "expected": 50, "tolerance": 1 }),
            // A later file (path order) wins on equal specificity.
            serde_json::json!({ "op": "project_write_file", "path": "styles/b-theme.css",
                                "content": "#box { width: 123px; }" }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_layout", "selector": "#box", "property": "width",
                                "expected": 123, "tolerance": 1 }),
            // Saving it again re-applies the new text.
            serde_json::json!({ "op": "project_write_file", "path": "styles/b-theme.css",
                                "content": "#box { width: 200px; }" }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_layout", "selector": "#box", "property": "width",
                                "expected": 200, "tolerance": 1 }),
            // Deleting it takes it off the window.
            serde_json::json!({ "op": "project_delete", "path": "styles/b-theme.css" }),
            wf,
            w,
            serde_json::json!({ "op": "assert_layout", "selector": "#box", "property": "width",
                                "expected": 50, "tolerance": 1 }),
        ],
    );
    assert_passes(&result);
}

#[test]
fn saving_a_component_file_re_registers_the_component_and_its_instances_update() {
    let root = TempDir::new("project-component");
    let changed = serde_json::json!({
        "format": "azul-component", "version": 1,
        "library": "user", "name": "card", "display_name": "Card", "description": "",
        "fields": [ { "name": "text", "type": "String", "default": "Changed" } ],
        "css": "",
        "template": "<div class=\"card\"><h2>{text}</h2></div>",
    })
    .to_string();
    let [wf, w] = settle();
    let result = run(
        "project_component_file",
        false,
        vec![
            serde_json::json!({ "op": "project_open", "path": root.str(), "create": true }),
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "div",
                                "attrs": { "class": "card" } }),
            serde_json::json!({ "op": "builder_insert", "parent": 1, "component": "h1",
                                "attrs": { "text": "Title" } }),
            serde_json::json!({ "op": "builder_convert_to_component", "node": 1,
                                "library": "user", "name": "card" }),
            serde_json::json!({ "op": "project_save" }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_node_count", "selector": "h1", "expected": 1 }),
            // The editor saves the component file: the component changes, and
            // so does every instance on screen.
            serde_json::json!({ "op": "project_write_file", "path": "components/user/card.json",
                                "content": changed }),
            serde_json::json!({ "op": "assert_response", "contains": "\"applied\":\"component\"" }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_node_count", "selector": "h1", "expected": 0 }),
            serde_json::json!({ "op": "assert_node_count", "selector": "h2", "expected": 1 }),
            // The instance has no argument: the new default shows.
            serde_json::json!({ "op": "assert_dom", "contains": "Changed" }),
            serde_json::json!({ "op": "get_component_render_tree", "library": "user",
                                "name": "card" }),
            serde_json::json!({ "op": "assert_response", "contains": "h2" }),
            // A broken file is still SAVED (it is the user's text), but not
            // applied, and the answer says why; the window keeps the last
            // good version.
            serde_json::json!({ "op": "project_write_file", "path": "components/user/card.json",
                                "content": "{ not json" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"written\":true" }),
            serde_json::json!({ "op": "assert_response", "contains": "apply_error" }),
            wf,
            w,
            serde_json::json!({ "op": "assert_node_count", "selector": "h2", "expected": 1 }),
        ],
    );
    assert_passes(&result);
    assert_eq!(
        read(&root.path().join("components/user/card.json")),
        "{ not json",
        "the file on disk is exactly what the editor saved"
    );
}

#[test]
fn a_project_exports_as_a_zip_and_a_zip_imports_back_without_leaving_the_root() {
    let root = TempDir::new("project-zip");
    let good = zip_base64(&[
        ("styles/imported.css", "#imported { width: 1px; }"),
        ("tests/t1.json", r#"{ "name": "t1", "steps": [] }"#),
    ]);
    // One bad entry spoils the whole archive: nothing of it is written.
    let evil = zip_base64(&[("styles/ok.css", "ok"), ("../evil.txt", "evil")]);
    let evil_abs = zip_base64(&[("/tmp/azb-evil-abs.txt", "evil")]);
    let result = run(
        "project_zip",
        true,
        vec![
            /* 0 */ serde_json::json!({ "op": "project_open", "path": root.str(), "create": true }),
            /* 1 */ serde_json::json!({ "op": "project_write_file", "path": "styles/x.css",
                                        "content": "#x { width: 2px; }" }),
            /* 2 */ serde_json::json!({ "op": "project_export_zip" }),
            /* 3 */ serde_json::json!({ "op": "assert_response",
                                        "contains": "data:application/zip;base64,UEsDB" }),
            /* 4 */ serde_json::json!({ "op": "project_import_zip",
                                        "data": format!("data:application/zip;base64,{good}") }),
            /* 5 */ serde_json::json!({ "op": "assert_response", "contains": "styles/imported.css" }),
            /* 6 */ serde_json::json!({ "op": "project_import_zip", "data": evil }),
            /* 7 */ serde_json::json!({ "op": "project_import_zip", "data": evil_abs }),
            /* 8 */ serde_json::json!({ "op": "project_list" }),
            /* 9 */ serde_json::json!({ "op": "assert_response", "not_contains": "styles/ok.css" }),
        ],
    );
    for i in [0, 1, 2, 3, 4, 5, 8, 9] {
        assert_eq!(step(&result, i).0, "pass", "step {i}:\n{}", failures(&result));
    }
    for (i, needle) in [(6, ".."), (7, "absolute")] {
        let (st, err) = step(&result, i);
        assert_eq!(st, "fail", "step {i} must be refused:\n{}", failures(&result));
        assert!(err.contains(needle), "step {i} must say why ({needle}): {err}");
    }

    assert_eq!(
        read(&root.path().join("styles/imported.css")),
        "#imported { width: 1px; }"
    );
    assert!(root.path().join("tests/t1.json").is_file());
    assert!(!root.path().join("styles/ok.css").exists(), "an archive is all or nothing");
    let parent = root.path().parent().expect("the temp dir has a parent");
    assert!(!parent.join("evil.txt").exists());
    assert!(!Path::new("/tmp/azb-evil-abs.txt").exists());

    // The export is a real zip of the project.
    let export = result
        .steps
        .iter()
        .find(|s| s.step_index == 2)
        .and_then(|s| s.response.clone())
        .expect("project_export_zip answered");
    let url = export["value"]["download_url"]
        .as_str()
        .expect("a download_url")
        .to_string();
    let b64 = url.split_once(',').map(|(_, b)| b).expect("a data: URI");
    let zip = crate::zip::ZipFile::from_bytes(
        &base64_decode(b64),
        &crate::zip::ZipReadConfig::default(),
    )
    .expect("the export is a zip");
    let paths = zip.paths();
    assert!(paths.contains(&"azul-project.json"), "{paths:?}");
    assert!(paths.contains(&"styles/x.css"), "{paths:?}");
}
