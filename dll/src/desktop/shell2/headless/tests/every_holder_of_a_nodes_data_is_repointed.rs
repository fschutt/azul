//! Every holder of a node's data is re-pointed by a rebuild's merge.
//!
//! A dataset merge keeps one of two allocations, and every clone of the other
//! must then point at the one kept, or what holds the clone writes where the
//! app no longer looks. One walk reaches the clones: a type that holds a
//! `RefAny`, or holds a type that does, implements `RefAnyHolder`, and its
//! walk names each such field. This test reads the sources of core, layout
//! and the dll and fails on a holder the walk does not reach, unless the list
//! below says why it need not.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

/// Fields that hold a binding's own callable, never an app's data.
const FOREIGN_CALLABLES: &[&str] = &["ctx", "callable", "callable_ptr"];

const PAYLOAD: &str = "a handler's payload: it sits inside a RefAny the walk cannot open";
const APP: &str = "the app's own data, which no rebuild merges";
const IN_FLIGHT: &str = "handed on within the event or frame that made it";
const BEING_BUILT: &str = "a DOM being built: it is merged where it is laid out";
const REFILLED: &str = "a native menu's items, taken over from the window's DOM after every layout";
const POPUP: &str = "a menu popup's own copy of the menu it shows, gone when it closes";
const BACKEND: &str = "the backend's own surface handles, never an app's data";

/// The holders the walk need not reach, and why: a path ending in `/`
/// covers a directory, and `*` every type of its path.
const NOT_WALKED: &[(&str, &str, &str)] = &[
    ("layout/src/widgets/", "*", PAYLOAD),
    ("layout/src/dialogs/", "*", PAYLOAD),
    ("layout/src/form_controls.rs", "CarriedFormHandlers", PAYLOAD),
    ("layout/src/e2e/", "*", "the end-to-end runner's own state, not an app's"),
    ("dll/src/web/", "*", APP),
    ("dll/src/desktop/app.rs", "AppInternal", APP),
    ("dll/src/desktop/app_events.rs", "AppEvents", APP),
    ("dll/src/desktop/shell2/linux/resources.rs", "AppResources", APP),
    (
        "dll/src/desktop/shell2/common/event.rs",
        "CommonWindowState",
        "the shell's hold on its window, which walks itself on each merge",
    ),
    (
        "dll/src/desktop/shell2/common/event.rs",
        "LayoutPassBorrows",
        "a borrow of the window for one layout pass",
    ),
    ("dll/src/desktop/shell2/common/event.rs", "PlannedInvocation", IN_FLIGHT),
    ("dll/src/desktop/shell2/common/transient.rs", "SyncOutcome", IN_FLIGHT),
    ("dll/src/desktop/shell2/common/transient.rs", "TransientWindowData", IN_FLIGHT),
    ("dll/src/desktop/tray/linux.rs", "MenuNode", APP),
    ("dll/src/desktop/menu.rs", "MenuPicks", IN_FLIGHT),
    ("dll/src/desktop/menu.rs", "MenuWindowData", POPUP),
    ("dll/src/desktop/menu_renderer.rs", "MenuItemCallbackData", POPUP),
    ("dll/src/desktop/menu_renderer.rs", "SubmenuCallbackData", POPUP),
    ("dll/src/desktop/shell2/macos/menu.rs", "MenuState", REFILLED),
    ("dll/src/desktop/shell2/windows/menu.rs", "WindowsMenuBar", REFILLED),
    (
        "dll/src/desktop/shell2/windows/mod.rs",
        "Win32Window",
        "a context menu's items while its modal loop runs, taken as it opened",
    ),
    ("dll/src/desktop/shell2/linux/gnome_menu/actions_protocol.rs", "DbusAction", REFILLED),
    (
        "dll/src/desktop/shell2/linux/gnome_menu/actions_protocol.rs",
        "PendingMenuCallback",
        IN_FLIGHT,
    ),
    (
        "dll/src/desktop/extra/sqlite/mod.rs",
        "Inner",
        "an open store: `repoint_open_dbs` walks it, registered with the runtime",
    ),
    (
        "core/src/dom.rs",
        "NodeDataExt",
        "the node's extra fields, walked row by row through the node's table",
    ),
    ("core/src/dom.rs", "Dom", BEING_BUILT),
    ("core/src/dom.rs", "FastDom", BEING_BUILT),
    ("core/src/styled_dom.rs", "CompactDom", BEING_BUILT),
    ("core/src/xml.rs", "CompactDomBuilder", BEING_BUILT),
    ("core/src/xml.rs", "DomXml", BEING_BUILT),
    ("core/src/xml.rs", "ComponentFieldValue", BEING_BUILT),
    ("core/src/xml_html_tree.rs", "Node", BEING_BUILT),
    ("layout/src/rich_blocks.rs", "ChainLink", BEING_BUILT),
    ("layout/src/rich_blocks.rs", "InlineNode", BEING_BUILT),
    (
        "core/src/resources.rs",
        "DecodedImage",
        "shared by the clones of its image: the node gets an image of the kept data",
    ),
    ("core/src/resources.rs", "SystemAnimations", APP),
    ("core/src/global_hotkey.rs", "GlobalHotkeyCallbackData", APP),
    ("core/src/notification.rs", "NotificationCallback", APP),
    ("core/src/icon.rs", "IconProviderInner", APP),
    (
        "core/src/icon.rs",
        "IconCacheEntry",
        "a cache keyed by the icon node as built, never handed to a handler",
    ),
    ("core/src/tray.rs", "TrayIconData", APP),
    ("core/src/task.rs", "ThreadSendMsg", IN_FLIGHT),
    ("core/src/host_invoker.rs", "Invocation", IN_FLIGHT),
    ("core/src/refany.rs", "Repointing", "the walk's own record of what replaces what"),
    (
        "core/src/diff.rs",
        "PreCascadeTransfers",
        "the fresh build's payloads on their way onto the kept tree, merged there",
    ),
    ("layout/src/managers/global_hotkey.rs", "AppSource", APP),
    ("layout/src/managers/global_hotkey.rs", "AppJob", APP),
    ("layout/src/managers/global_hotkey.rs", "HotkeyDelivery", APP),
    ("layout/src/managers/tray_event.rs", "TrayDelivery", APP),
    ("layout/src/callbacks.rs", "CallbackChange", IN_FLIGHT),
    ("layout/src/callbacks.rs", "MenuCallback", IN_FLIGHT),
    ("layout/src/thread.rs", "ThreadWriteBackMsg", IN_FLIGHT),
    ("layout/src/json.rs", "ResultRefAnyString", IN_FLIGHT),
    ("layout/src/window.rs", "CursorBlinkTimerAction", IN_FLIGHT),
    ("layout/src/window.rs", "TooltipTimerAction", IN_FLIGHT),
    (
        "layout/src/window.rs",
        "PendingReconciliation",
        "a rebuild's record: its old nodes are walked before it is made",
    ),
    ("layout/src/transient.rs", "OpenTransientWindow", BACKEND),
    ("layout/src/transient.rs", "TransientWindowManager", BACKEND),
    ("layout/src/solver3/pagination.rs", "MarginBoxCustom", APP),
];

/// Walks that do not name their fields, and why.
const WALKED_BY_TABLE: &[(&str, &str)] = &[(
    "NodeData",
    "walked row by row: `NodeField::for_each_refany` matches every row of the node's table",
)];

/// One `struct` or `enum` of the sources, with its fields as (name, type);
/// a tuple field or variant has an empty name.
struct Definition {
    path: String,
    name: String,
    fields: Vec<(String, String)>,
}

/// Every `.rs` file under `dir`, test files and directories left out.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        if path.is_dir() {
            if name != "tests" {
                rust_files(&path, out);
            }
        } else if name.ends_with(".rs")
            && !name.ends_with("_tests.rs")
            && !name.ends_with("_test.rs")
            && name != "autotest_generated.rs"
        {
            out.push(path);
        }
    }
}

/// How many more of `open` than of `close` `text` has.
fn balance(text: &str, open: char, close: char) -> i64 {
    text.chars().fold(0, |depth, c| {
        if c == open {
            depth + 1
        } else if c == close {
            depth - 1
        } else {
            depth
        }
    })
}

/// Whether `line` makes the item after it a test-only one.
fn is_test_cfg(line: &str) -> bool {
    line == "#[cfg(test)]" || line.starts_with("#[cfg(all(test,")
}

/// The lines of `source` with their comments cut off and the test modules
/// blanked: their types are no holders a running app has.
fn code_lines(source: &str) -> Vec<String> {
    let mut lines: Vec<String> = source
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default().to_owned())
        .collect();
    let mut start = 0;
    while start < lines.len() {
        if !is_test_cfg(lines[start].trim()) {
            start += 1;
            continue;
        }
        // The item after the attributes that follow the `cfg`.
        let mut item = start + 1;
        let mut brackets = 0;
        while item < lines.len() {
            let text = lines[item].trim();
            if brackets > 0 {
                brackets += balance(text, '[', ']');
            } else if text.starts_with("#[") {
                brackets = balance(text, '[', ']');
            } else if !text.is_empty() {
                break;
            }
            item += 1;
        }
        let head = lines.get(item).map_or("", |line| line.trim());
        let head = head
            .strip_prefix("pub(crate) ")
            .or_else(|| head.strip_prefix("pub "))
            .unwrap_or(head);
        if !(head.starts_with("mod ") && head.ends_with('{')) {
            start += 1;
            continue;
        }
        let mut end = item;
        let mut depth = 0;
        while end < lines.len() {
            depth += balance(&lines[end], '{', '}');
            end += 1;
            if depth <= 0 {
                break;
            }
        }
        for line in &mut lines[start..end] {
            line.clear();
        }
        start = end;
    }
    lines
}

/// `text` without a leading `pub`, `pub(crate)` or the like.
fn without_visibility(text: &str) -> &str {
    let Some(after) = text.strip_prefix("pub") else {
        return text;
    };
    let after = after.strip_prefix('(').map_or(Some(after), |scoped| {
        scoped.find(')').map(|close| &scoped[close + 1..])
    });
    after
        .filter(|after| after.starts_with(char::is_whitespace))
        .map_or(text, str::trim_start)
}

/// The end of the identifier `text` starts with.
fn identifier_end(text: &str) -> usize {
    text.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(text.len())
}

/// The kind, name and rest of a line that opens a `struct` or an `enum`.
fn definition_head(line: &str) -> Option<(&str, &str, &str)> {
    let rest = without_visibility(line.trim_start());
    let (kind, after) = rest.split_once(char::is_whitespace)?;
    if kind != "struct" && kind != "enum" {
        return None;
    }
    let after = after.trim_start();
    let end = identifier_end(after);
    let name = &after[..end];
    name.starts_with(|c: char| c.is_ascii_uppercase())
        .then_some((kind, name, &after[end..]))
}

/// The name and type of a line that declares a named field.
fn field_head(line: &str) -> Option<(&str, &str)> {
    let rest = without_visibility(line.trim_start());
    let end = rest
        .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
        .unwrap_or(rest.len());
    let name = &rest[..end];
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let ty = rest[end..].trim_start().strip_prefix(':')?;
    (!ty.starts_with(':')).then_some((name, ty))
}

/// The tuple fields of a line that declares an enum variant: what follows
/// its `(`, or nothing for a variant of another shape.
fn variant_head(line: &str) -> Option<&str> {
    let rest = line.trim_start();
    if !rest.starts_with(|c: char| c.is_ascii_uppercase()) {
        return None;
    }
    let after = rest[identifier_end(rest)..].trim_start();
    match after.chars().next() {
        Some('(') => Some(&after[1..]),
        None | Some('{' | ',' | '=') => Some(""),
        Some(_) => None,
    }
}

/// The type a line's `impl RefAnyHolder for` names, if the line opens one.
fn walk_head(line: &str) -> Option<&str> {
    const MARKER: &str = "RefAnyHolder for ";
    let line = line.trim_start();
    if !line.starts_with("impl") {
        return None;
    }
    let at = line.find(MARKER)?;
    if line[..at].contains(['{', ';']) {
        return None;
    }
    let target = &line[at + MARKER.len()..];
    let end = target
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
        .unwrap_or(target.len());
    target[..end].rsplit("::").next().filter(|name| !name.is_empty())
}

/// The braced body of the item that opens on the first of `lines`.
fn walk_body(lines: &[String]) -> String {
    let mut body = String::new();
    let mut depth = 0;
    let mut opened = false;
    for line in lines {
        let mut text = line.as_str();
        if !opened {
            let Some(brace) = text.find('{') else {
                continue;
            };
            opened = true;
            text = &text[brace..];
        }
        depth += balance(text, '{', '}');
        body.push_str(text);
        body.push('\n');
        if depth <= 0 {
            break;
        }
    }
    body
}

/// Reads the definitions and the walks of the file at `path`.
fn read_file(
    path: &str,
    lines: &[String],
    definitions: &mut Vec<Definition>,
    walks: &mut BTreeMap<String, String>,
) {
    let mut at = 0;
    while at < lines.len() {
        let line = &lines[at];
        if let Some(name) = walk_head(line) {
            walks
                .entry(name.to_owned())
                .or_default()
                .push_str(&walk_body(&lines[at..]));
        }
        let Some((kind, name, rest)) = definition_head(line) else {
            at += 1;
            continue;
        };
        let mut fields: Vec<(String, String)> = Vec::new();
        if rest.contains('{') && !rest.trim_end().ends_with(';') {
            let mut depth = balance(rest, '{', '}');
            at += 1;
            while at < lines.len() && depth > 0 {
                let body_line = &lines[at];
                depth += balance(body_line, '{', '}');
                at += 1;
                let text = body_line.trim();
                if text.is_empty() || text.starts_with('#') {
                    continue;
                }
                if let Some((field, ty)) = field_head(body_line) {
                    fields.push((field.to_owned(), ty.to_owned()));
                } else if let Some(tuple) = variant_head(body_line).filter(|_| kind == "enum") {
                    fields.push((String::new(), tuple.to_owned()));
                } else if let Some(last) = fields.last_mut() {
                    last.1.push(' ');
                    last.1.push_str(text);
                }
            }
        } else if rest.contains('(') {
            let mut text = rest.to_owned();
            while !text.contains(';') && at + 1 < lines.len() {
                at += 1;
                text.push(' ');
                text.push_str(lines[at].trim());
            }
            fields.push((String::new(), text));
            at += 1;
        } else {
            at += 1;
            continue;
        }
        definitions.push(Definition {
            path: path.to_owned(),
            name: name.to_owned(),
            fields,
        });
    }
}

/// The type names `ty` mentions, each with the names it wraps: an
/// `OptionFooVec` stands for `FooVec` and `Foo` as well.
fn names_in(ty: &str) -> Vec<&str> {
    let mut names = Vec::new();
    for word in ty.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if !word.starts_with(|c: char| c.is_ascii_uppercase()) {
            continue;
        }
        let mut name = word;
        names.push(name);
        loop {
            if let Some(inner) = name.strip_prefix("Option").filter(|inner| !inner.is_empty()) {
                name = inner;
            } else if let Some(inner) = name.strip_suffix("Vec").filter(|inner| !inner.is_empty()) {
                name = inner;
            } else {
                break;
            }
            names.push(name);
        }
    }
    names
}

/// Whether a field `name` of type `ty` holds a `RefAny` or a walked type:
/// a borrow or a pointer holds nothing.
fn holds(name: &str, ty: &str, walked: &BTreeSet<String>) -> bool {
    !FOREIGN_CALLABLES.contains(&name)
        && !ty.trim_start().starts_with(['&', '*'])
        && names_in(ty)
            .into_iter()
            .any(|held| held == "RefAny" || walked.contains(held))
}

/// Whether `name` is an `Option` or a `Vec` of a walked type: what holds
/// one holds what it wraps.
fn is_wrapper(name: &str, walked: &BTreeSet<String>) -> bool {
    names_in(name)
        .into_iter()
        .skip(1)
        .any(|held| held == "RefAny" || walked.contains(held))
}

/// Whether `body` names `word` as a whole identifier.
fn names_word(body: &str, word: &str) -> bool {
    body.match_indices(word).any(|(at, _)| {
        let before = body[..at].chars().next_back();
        let after = body[at + word.len()..].chars().next();
        !before
            .into_iter()
            .chain(after)
            .any(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

#[test]
fn every_holder_of_a_nodes_data_is_reached_by_the_repointing_walk() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the dll sits in its workspace");
    let mut files = Vec::new();
    for root in ["core/src", "layout/src", "dll/src"] {
        rust_files(&workspace.join(root), &mut files);
    }
    files.sort();
    let mut definitions = Vec::new();
    let mut walks = BTreeMap::new();
    for file in &files {
        let source = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        let path = file
            .strip_prefix(workspace)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        read_file(&path, &code_lines(&source), &mut definitions, &mut walks);
    }
    assert!(
        files.len() > 300 && definitions.len() > 1000,
        "harness: the sources are read ({} files, {} types)",
        files.len(),
        definitions.len()
    );
    for holder in ["NodeData", "Menu", "CoreCallbackData", "Timer", "Thread", "LayoutWindow"] {
        assert!(walks.contains_key(holder), "harness: the walk reaches {holder}");
    }
    let walked: BTreeSet<String> = walks.keys().cloned().collect();

    let mut unwalked = Vec::new();
    let mut listed = BTreeSet::new();
    for definition in &definitions {
        if walked.contains(&definition.name) || is_wrapper(&definition.name, &walked) {
            continue;
        }
        let held: Vec<&str> = definition
            .fields
            .iter()
            .filter(|(name, ty)| holds(name, ty, &walked))
            .map(|(name, _)| if name.is_empty() { "a tuple field" } else { name.as_str() })
            .collect();
        if held.is_empty() {
            continue;
        }
        let entry = NOT_WALKED.iter().find(|(path, name, _)| {
            definition.path.starts_with(path) && (*name == "*" || *name == definition.name)
        });
        if let Some((path, name, _)) = entry {
            listed.insert((*path, *name));
        } else {
            unwalked.push(format!(
                "{} {}: {}",
                definition.path,
                definition.name,
                held.join(", ")
            ));
        }
    }
    assert!(
        unwalked.is_empty(),
        "these types hold a RefAny, or a type that does, out of the repointing walk's \
         reach (implement `RefAnyHolder` for them and walk them from what holds them, \
         or list them in NOT_WALKED with the reason the walk need not reach them):\n{}",
        unwalked.join("\n")
    );

    let mut unnamed = Vec::new();
    for definition in &definitions {
        let Some(body) = walks.get(&definition.name) else {
            continue;
        };
        if WALKED_BY_TABLE.iter().any(|(name, _)| *name == definition.name) {
            continue;
        }
        for (name, ty) in &definition.fields {
            if !name.is_empty() && holds(name, ty, &walked) && !names_word(body, name) {
                unnamed.push(format!("{} {}.{name}", definition.path, definition.name));
            }
        }
    }
    assert!(
        unnamed.is_empty(),
        "these fields hold a RefAny, or a type that does, but the walk of their type \
         passes them by (hand them to `visit` in its `RefAnyHolder` impl):\n{}",
        unnamed.join("\n")
    );

    let stale: Vec<String> = NOT_WALKED
        .iter()
        .filter(|(path, name, _)| *name != "*" && !listed.contains(&(*path, *name)))
        .map(|(path, name, _)| format!("{path} {name}"))
        .collect();
    assert!(
        stale.is_empty(),
        "these NOT_WALKED entries name a type that holds nothing any more (drop them):\n{}",
        stale.join("\n")
    );
}
