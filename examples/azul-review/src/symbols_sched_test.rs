//! The background scan: how far a file is from the one looked at, the order the files are read in,
//! the gate that holds the far ones back while the user draws, and the thread's priority. The
//! integration tests run the real worker threads on a memory workspace. No window, no azul.

use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use super::*;
use crate::{
    symbols::{Kind, Layout},
    symbols_source::{FileSource, MemSource, SkipReason},
};

fn layout() -> Layout {
    let mut l = Layout::default();
    l.note_manifest_dir("crates/core");
    l.note_manifest_dir("crates/css");
    l.note_manifest_dir("app");
    l.set_package("azul-core", "crates/core");
    l.set_package("azul-css", "crates/css");
    l
}

fn imports(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

// ---- distance ----

#[test]
fn the_distance_between_folders_counts_the_steps_up_and_down() {
    assert_eq!(path_distance("a/b", "a/b"), 0);
    assert_eq!(path_distance("a/b", "a/c"), 2);
    assert_eq!(path_distance("a/b/c", "a"), 2);
    assert_eq!(path_distance("", "x/y"), 2);
    assert_eq!(path_distance("a", ""), 1);
    assert_eq!(path_distance("a/b", "x/y"), 4);
}

#[test]
fn the_classes_go_from_the_file_itself_out_to_the_rest_and_a_nearer_file_comes_first_within_one() {
    let order = [Class::Focus, Class::SameDir, Class::Module, Class::SameCrate, Class::ImportedCrate, Class::Rest];
    for pair in order.windows(2) {
        assert!(pair[0] < pair[1]);
        assert!(rank_key(Rank { class: pair[0], distance: 900 }, 7) < rank_key(Rank { class: pair[1], distance: 0 }, 0));
    }
    assert!(rank_key(Rank { class: Class::Rest, distance: 2 }, 9) < rank_key(Rank { class: Class::Rest, distance: 3 }, 0));
    assert!(rank_key(Rank { class: Class::Rest, distance: 2 }, 1) < rank_key(Rank { class: Class::Rest, distance: 2 }, 2));
}

#[test]
fn a_rust_file_ranks_the_files_around_it_by_module_crate_and_import() {
    let l = layout();
    let focus = focus_of(
        "crates/core/src/ui/view.rs",
        &imports(&["crate::dom::Node", "super::super::layout::Rect", "self::widgets::Button", "azul_css::props::Color", "std::fmt::Display"]),
        &l,
    );
    let class = |p: &str| rank(&focus, p, &l).class;
    assert_eq!(class("crates/core/src/ui/view.rs"), Class::Focus);
    assert_eq!(class("crates/core/src/ui/other.rs"), Class::SameDir);
    // `crate::dom::Node`: the module file, its folder form, and the file named for the item.
    assert_eq!(class("crates/core/src/dom.rs"), Class::Module);
    assert_eq!(class("crates/core/src/dom/mod.rs"), Class::Module);
    assert_eq!(class("crates/core/src/dom/node.rs"), Class::Module);
    // `super::super::layout::Rect`: `view.rs` is a module of `ui`, whose parent is the crate's root.
    assert_eq!(class("crates/core/src/layout.rs"), Class::Module);
    // `self::widgets::Button`: a module below view.rs.
    assert_eq!(class("crates/core/src/ui/view/widgets.rs"), Class::Module);
    assert_eq!(class("crates/core/src/ui/view/widgets/button.rs"), Class::Module);
    // The parent module, and the crate's root.
    assert_eq!(class("crates/core/src/ui.rs"), Class::Module);
    assert_eq!(class("crates/core/src/lib.rs"), Class::Module);
    // Another crate of the workspace named in an import: its module, then the rest of it.
    assert_eq!(class("crates/css/src/props.rs"), Class::Module);
    assert_eq!(class("crates/css/src/props/color.rs"), Class::Module);
    assert_eq!(class("crates/css/src/other.rs"), Class::ImportedCrate);
    // The same crate, and everything else.
    assert_eq!(class("crates/core/src/misc/util.rs"), Class::SameCrate);
    assert_eq!(class("app/src/main.rs"), Class::Rest);
    assert_eq!(class("README.md"), Class::Rest);
}

#[test]
fn a_file_below_the_focus_folder_is_not_the_same_folder_and_the_distance_tells_how_far() {
    let l = layout();
    let focus = focus_of("crates/core/src/ui/view.rs", &[], &l);
    assert_eq!(rank(&focus, "crates/core/src/ui/deep/er/x.rs", &l), Rank { class: Class::SameCrate, distance: 2 });
    assert_eq!(rank(&focus, "crates/core/src/ui/other.rs", &l), Rank { class: Class::SameDir, distance: 0 });
    assert_eq!(rank(&focus, "app/src/main.rs", &l), Rank { class: Class::Rest, distance: 6 });
}

#[test]
fn a_module_file_is_its_own_folders_module_and_a_crate_root_has_the_src_folder_as_its_module_folder() {
    let l = layout();
    let focus = focus_of("crates/core/src/lib.rs", &imports(&["self::ui::view", "crate::dom"]), &l);
    let class = |p: &str| rank(&focus, p, &l).class;
    assert_eq!(class("crates/core/src/ui/view.rs"), Class::Module);
    assert_eq!(class("crates/core/src/dom.rs"), Class::SameDir, "a module in the same folder is in the same folder first");
    assert_eq!(class("crates/core/src/other.rs"), Class::SameDir);
    let focus = focus_of("crates/core/src/ui/mod.rs", &imports(&["self::view", "super::dom"]), &l);
    let class = |p: &str| rank(&focus, p, &l).class;
    assert_eq!(class("crates/core/src/ui/view.rs"), Class::SameDir);
    assert_eq!(class("crates/core/src/dom.rs"), Class::Module);
    assert_eq!(class("crates/core/src/lib.rs"), Class::Module);
}

#[test]
fn python_javascript_and_c_imports_that_name_a_file_next_to_this_one_rank_as_modules() {
    let l = layout();
    let py = focus_of("app/pkg/main.py", &imports(&[".util", "..shared.tools", "pkg.helpers"]), &l);
    // A module in the same folder is in the same folder first.
    assert_eq!(rank(&py, "app/pkg/util.py", &l).class, Class::SameDir);
    assert_eq!(rank(&py, "app/shared/tools.py", &l).class, Class::Module);
    assert_eq!(rank(&py, "app/pkg/helpers.py", &l).class, Class::SameDir);
    assert_eq!(rank(&py, "app/shared/other.py", &l).class, Class::SameCrate);

    let js = focus_of("app/web/main.ts", &imports(&["./util", "../lib/store", "react"]), &l);
    assert_eq!(rank(&js, "app/web/util.ts", &l).class, Class::SameDir);
    assert_eq!(rank(&js, "app/web/util/index.js", &l).class, Class::Module);
    assert_eq!(rank(&js, "app/lib/store.tsx", &l).class, Class::Module);

    let c = focus_of("app/src/a.c", &imports(&["b.h", "<stdio.h>", "sub/c.h"]), &l);
    assert_eq!(rank(&c, "app/src/b.h", &l).class, Class::SameDir);
    assert_eq!(rank(&c, "app/src/sub/c.h", &l).class, Class::Module);
}

#[test]
fn a_hostile_import_list_is_bounded() {
    let l = layout();
    let many: Vec<String> = (0..50_000).map(|i| format!("crate::m{i}::n{i}::Item{i}")).collect();
    let focus = focus_of("crates/core/src/a.rs", &many, &l);
    assert!(focus.module_files.len() <= MAX_FOCUS_CANDIDATES);
    let deep = vec![format!("{}::x", "super::".repeat(500))];
    let _ = focus_of("crates/core/src/a/b/c.rs", &deep, &l);
    let _ = focus_of("", &[], &l);
    let _ = focus_of("x", &imports(&["", "::", "a::::b"]), &l);
}

// ---- the gate ----

#[test]
fn the_gate_is_busy_for_a_while_after_a_touch_and_never_before_one() {
    let gate = ActivityGate::new();
    assert!(!gate.busy_at(5_000, 250));
    gate.touch_at(1_000);
    assert!(gate.busy_at(1_100, 250));
    assert!(gate.busy_at(1_249, 250));
    assert!(!gate.busy_at(1_250, 250));
    assert!(!gate.busy_at(9_000, 250));
    let clone = gate.clone();
    clone.touch_at(8_900);
    assert!(gate.busy_at(9_000, 250), "a clone shares the gate");
}

#[test]
fn a_touch_with_the_real_clock_makes_the_gate_busy_now() {
    let gate = ActivityGate::new();
    assert!(!gate.busy_within(10_000));
    gate.touch();
    assert!(gate.busy_within(10_000));
    thread::sleep(Duration::from_millis(30));
    assert!(!gate.busy_within(5));
}

// ---- the priority ----

#[test]
fn the_normal_priority_is_always_there_and_a_lower_one_is_the_systems_where_it_has_one() {
    let normal = thread::spawn(|| apply_thread_priority(ThreadPriority::Normal)).join().unwrap();
    assert!(normal);
    let low = thread::spawn(|| apply_thread_priority(ThreadPriority::Low)).join().unwrap();
    let idle = thread::spawn(|| apply_thread_priority(ThreadPriority::Idle)).join().unwrap();
    if cfg!(any(target_os = "macos", target_os = "linux", windows)) {
        assert!(low && idle, "this system lowers a thread's priority");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn on_macos_the_scan_thread_runs_at_the_utility_class_and_the_idle_one_at_background() {
    let (low, idle, normal) = thread::spawn(|| {
        let before = current_qos();
        assert!(apply_thread_priority(ThreadPriority::Low));
        let low = current_qos();
        assert!(apply_thread_priority(ThreadPriority::Idle));
        (low, current_qos(), before)
    })
    .join()
    .unwrap();
    assert_eq!(low, Some(QOS_UTILITY));
    assert_eq!(idle, Some(QOS_BACKGROUND));
    assert_ne!(normal, Some(QOS_UTILITY));
}

// ---- the scan itself ----

/// A workspace of Rust crates on which the order can be read.
fn workspace() -> MemSource {
    let mut m = MemSource::new();
    let mut put = |path: &str, text: &str| m.put(path, text.as_bytes().to_vec());
    put("Cargo.toml", "[workspace]\nmembers = [\"crates/*\", \"app\"]\n");
    put("crates/core/Cargo.toml", "[package]\nname = \"azul-core\"\n");
    put("crates/core/src/lib.rs", "pub mod ui;\npub mod dom;\npub struct CoreRoot;\n");
    put("crates/core/src/ui/mod.rs", "pub mod view;\npub mod other;\npub struct UiMod;\n");
    put("crates/core/src/ui/view.rs", "use crate::dom::Node;\nuse azul_css::props::Color;\npub struct View;\n");
    put("crates/core/src/ui/other.rs", "pub struct Other;\n");
    put("crates/core/src/dom.rs", "pub struct Node;\n");
    put("crates/core/src/misc/util.rs", "pub fn util() {}\n");
    put("crates/css/Cargo.toml", "[package]\nname = \"azul-css\"\n");
    put("crates/css/src/lib.rs", "pub struct CssRoot;\n");
    put("crates/css/src/props.rs", "pub struct Color;\n");
    put("app/Cargo.toml", "[package]\nname = \"app\"\n");
    put("app/src/main.rs", "fn main() {}\n");
    put("docs/readme.md", "# readme\n");
    put("node_modules/dep/index.js", "function hidden() {}\n");
    put(".git/hooks/pre-commit.sh", "echo hi\n");
    m
}

fn settings(workers: usize) -> IndexerSettings {
    IndexerSettings { workers, quiet_ms: 60, poll_ms: 5, ..IndexerSettings::default() }
}

fn rs_reads(m: &MemSource) -> Vec<String> {
    m.reads().into_iter().filter(|p| p.ends_with(".rs")).collect()
}

const WAIT: Duration = Duration::from_secs(30);

#[test]
fn the_files_are_read_outwards_from_the_one_looked_at() {
    let source = Arc::new(workspace());
    let indexer = Indexer::start(source.clone(), Some("crates/core/src/ui/view.rs"), settings(1), ActivityGate::new());
    assert!(indexer.wait_idle(WAIT));
    let reads = rs_reads(&source);
    let want = [
        // the file itself, then the folder
        "crates/core/src/ui/view.rs",
        "crates/core/src/ui/mod.rs",
        "crates/core/src/ui/other.rs",
        // the module neighbours, nearest first
        "crates/core/src/dom.rs",
        "crates/core/src/lib.rs",
        "crates/css/src/props.rs",
        // the same crate, the crate it imports, the rest
        "crates/core/src/misc/util.rs",
        "crates/css/src/lib.rs",
        "app/src/main.rs",
    ];
    assert_eq!(reads, want);
}

#[test]
fn what_was_scanned_can_be_asked_and_nothing_hidden_or_excluded_was_read() {
    let source = Arc::new(workspace());
    let indexer = Indexer::start(source.clone(), Some("crates/core/src/ui/view.rs"), settings(2), ActivityGate::new());
    assert!(indexer.wait_idle(WAIT));
    {
        let idx = indexer.index();
        // `pub struct View` and `pub mod view;` both say "view".
        assert_eq!(idx.lookup(&["view"]).iter().filter(|d| d.kind == Kind::Struct).count(), 1);
        assert_eq!(idx.lookup(&["view"]).len(), 2);
        assert_eq!(idx.lookup(&["css", "root"])[0].file, "crates/css/src/lib.rs");
        assert_eq!(idx.lookup(&["color"])[0].kind, Kind::Struct);
        assert!(idx.lookup(&["hidden"]).is_empty());
        assert_eq!(idx.package_dir("azul_core"), Some("crates/core"));
        assert_eq!(idx.crate_dir_of("crates/css/src/props.rs"), Some("crates/css"));
        assert_eq!(idx.imports_of("crates/core/src/ui/view.rs"), ["crate::dom::Node", "azul_css::props::Color"]);
    }
    let reads = source.reads();
    assert!(reads.iter().all(|p| !p.contains("node_modules") && !p.contains(".git")), "{reads:?}");
    assert!(!reads.contains(&"docs/readme.md".to_string()));
    let progress = indexer.progress();
    assert!(progress.idle);
    assert!(!progress.walking);
    assert_eq!(progress.files_done, progress.files_found);
    // 9 Rust files and 4 manifests are what there is to read.
    assert_eq!(progress.files_found, 13);
}

#[test]
fn a_workspace_with_no_focus_is_scanned_all_the_same() {
    let source = Arc::new(workspace());
    let indexer = Indexer::start(source, None, settings(3), ActivityGate::new());
    assert!(indexer.wait_idle(WAIT));
    assert_eq!(indexer.index().lookup(&["main"]).len(), 1);
    assert_eq!(indexer.progress().files_found, 13);
}

#[test]
fn files_not_worth_reading_are_skipped_with_their_reason_and_count_as_done() {
    let mut m = workspace();
    m.put("crates/core/src/blob.rs", vec![b'x', 0, 0, 0]);
    m.put("crates/core/src/gen.rs", b"// Code generated by tool. DO NOT EDIT.\npub struct Generated;\n".to_vec());
    m.put("crates/core/src/min.rs", format!("fn a() {{ {} }}", "x(); ".repeat(2_000)).into_bytes());
    m.put("crates/core/src/huge.rs", vec![b' '; 30_000]);
    let mut s = settings(2);
    s.exclusions.max_file_bytes = 20_000;
    let indexer = Indexer::start(Arc::new(m), None, s, ActivityGate::new());
    assert!(indexer.wait_idle(WAIT));
    let idx = indexer.index();
    let stats = idx.stats();
    assert_eq!(stats.skipped[SkipReason::Binary.index()], 1);
    assert_eq!(stats.skipped[SkipReason::Generated.index()], 1);
    assert_eq!(stats.skipped[SkipReason::Minified.index()], 1);
    assert_eq!(stats.skipped[SkipReason::TooLarge.index()], 1);
    assert!(idx.lookup(&["generated"]).is_empty());
    let p = indexer.progress();
    assert_eq!(p.files_done, p.files_found);
}

#[test]
fn a_file_that_cannot_be_read_is_counted_and_the_scan_goes_on() {
    struct Flaky(MemSource);
    impl FileSource for Flaky {
        fn list(&self, dir: &str) -> Result<Vec<crate::symbols_source::DirEntry>, crate::symbols_source::ReadError> {
            self.0.list(dir)
        }
        fn read(&self, rel: &str, max: usize) -> Result<Vec<u8>, crate::symbols_source::ReadError> {
            if rel.ends_with("dom.rs") {
                return Err(crate::symbols_source::ReadError::Io("disk on fire".into()));
            }
            self.0.read(rel, max)
        }
        fn stat(&self, rel: &str) -> Option<crate::symbols_source::FileMeta> {
            self.0.stat(rel)
        }
    }
    let indexer = Indexer::start(Arc::new(Flaky(workspace())), None, settings(2), ActivityGate::new());
    assert!(indexer.wait_idle(WAIT));
    let idx = indexer.index();
    assert_eq!(idx.stats().skipped[SkipReason::Unreadable.index()], 1);
    assert_eq!(idx.lookup(&["main"]).len(), 1);
}

/// A source whose bytes for one path can be changed while the index runs.
struct Live {
    base: MemSource,
    changed: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
}

impl FileSource for Live {
    fn list(&self, d: &str) -> Result<Vec<crate::symbols_source::DirEntry>, crate::symbols_source::ReadError> {
        self.base.list(d)
    }
    fn read(&self, r: &str, m: usize) -> Result<Vec<u8>, crate::symbols_source::ReadError> {
        if let Some(bytes) = self.changed.lock().unwrap().get(r) {
            return Ok(bytes.clone());
        }
        self.base.read(r, m)
    }
    fn stat(&self, r: &str) -> Option<crate::symbols_source::FileMeta> {
        self.base.stat(r)
    }
}

#[test]
fn saving_a_file_while_the_index_runs_replaces_its_definitions_without_a_new_scan_of_the_rest() {
    let source = Arc::new(Live { base: workspace(), changed: std::sync::Mutex::new(Default::default()) });
    let indexer = Indexer::start(source.clone(), None, settings(2), ActivityGate::new());
    assert!(indexer.wait_idle(WAIT));
    assert_eq!(indexer.index().lookup(&["node"]).len(), 1);
    source.changed.lock().unwrap().insert("crates/core/src/dom.rs".into(), b"pub struct Leaf;\n".to_vec());
    indexer.file_saved("crates/core/src/dom.rs");
    assert!(indexer.wait_idle(WAIT));
    let idx = indexer.index();
    assert!(idx.lookup(&["node"]).is_empty());
    assert_eq!(idx.lookup(&["leaf"]).len(), 1);
    // A file the walk never saw is put in by a save too.
    drop(idx);
    source.changed.lock().unwrap().insert("crates/core/src/fresh.rs".into(), b"pub fn brand_new() {}\n".to_vec());
    indexer.file_saved("crates/core/src/fresh.rs");
    assert!(indexer.wait_idle(WAIT));
    assert_eq!(indexer.index().lookup(&["brand", "new"]).len(), 1);
}

#[test]
fn while_the_user_draws_only_the_neighbourhood_is_read_and_the_rest_waits_for_quiet() {
    let source = Arc::new(workspace());
    let gate = ActivityGate::new();
    gate.touch();
    let mut s = settings(2);
    s.quiet_ms = 400;
    let indexer = Indexer::start(source.clone(), Some("crates/core/src/ui/view.rs"), s, gate.clone());
    // The neighbourhood is read although the gate is busy.
    let near_files = ["crates/core/src/ui/view.rs", "crates/core/src/ui/mod.rs", "crates/core/src/ui/other.rs", "crates/core/src/dom.rs", "crates/core/src/lib.rs"];
    let deadline = Instant::now() + WAIT;
    while !near_files.iter().all(|f| rs_reads(&source).contains(&(*f).to_string())) && Instant::now() < deadline {
        gate.touch();
        thread::sleep(Duration::from_millis(5));
    }
    let early = rs_reads(&source);
    for near in near_files {
        assert!(early.contains(&near.to_string()), "{near} in {early:?}");
    }
    // The far ones are not, as long as the user keeps drawing.
    for _ in 0..30 {
        gate.touch();
        thread::sleep(Duration::from_millis(5));
    }
    let held = rs_reads(&source);
    assert!(!held.contains(&"app/src/main.rs".to_string()), "{held:?}");
    assert!(!indexer.progress().idle);
    // Quiet: everything is read.
    assert!(indexer.wait_idle(WAIT));
    assert!(rs_reads(&source).contains(&"app/src/main.rs".to_string()));
}

#[test]
fn looking_at_another_file_moves_its_neighbourhood_to_the_front_of_what_is_left() {
    let source = Arc::new(workspace());
    let gate = ActivityGate::new();
    gate.touch();
    let mut s = settings(1);
    s.quiet_ms = 500;
    let indexer = Indexer::start(source.clone(), Some("crates/core/src/ui/view.rs"), s, gate.clone());
    let deadline = Instant::now() + WAIT;
    while !rs_reads(&source).contains(&"crates/core/src/dom.rs".to_string()) && Instant::now() < deadline {
        gate.touch();
        thread::sleep(Duration::from_millis(5));
    }
    // Now the user opens a file of the other crate; the far files are still held back.
    indexer.set_focus("app/src/main.rs");
    let deadline = Instant::now() + WAIT;
    while !rs_reads(&source).contains(&"app/src/main.rs".to_string()) && Instant::now() < deadline {
        gate.touch();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(rs_reads(&source).contains(&"app/src/main.rs".to_string()), "the new focus is read although the user draws");
    assert!(indexer.wait_idle(WAIT));
    assert_eq!(indexer.index().lookup(&["main"]).len(), 1);
}

#[test]
fn the_index_is_bounded_by_its_caps_and_says_so_when_a_workspace_is_larger() {
    let mut m = MemSource::new();
    for i in 0..300 {
        m.put(&format!("src/m{}/f{i}.rs", i % 10), format!("pub fn item_{i}() {{}}\npub struct Item{i};\n").into_bytes());
    }
    let mut s = settings(3);
    s.caps.max_files = 100;
    let indexer = Indexer::start(Arc::new(m), None, s, ActivityGate::new());
    assert!(indexer.wait_idle(WAIT));
    let idx = indexer.index();
    let stats = idx.stats();
    assert!(stats.files <= 100, "{}", stats.files);
    assert!(stats.full);
    assert!(!idx.notes().is_empty());
}

#[test]
fn stopping_ends_the_threads_even_while_far_files_wait_and_a_drop_does_the_same() {
    let source = Arc::new(workspace());
    let gate = ActivityGate::new();
    gate.touch();
    let mut s = settings(2);
    s.quiet_ms = 60_000;
    let mut indexer = Indexer::start(source.clone(), Some("crates/core/src/ui/view.rs"), s.clone(), gate.clone());
    thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    indexer.stop();
    assert!(started.elapsed() < Duration::from_secs(5));
    let again = Indexer::start(source, None, s, gate);
    thread::sleep(Duration::from_millis(50));
    let started = Instant::now();
    drop(again);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn the_number_of_workers_is_half_the_cores_between_one_and_two_unless_it_is_asked() {
    assert_eq!(AUTO_WORKERS_MAX, 2);
    assert_eq!(workers_for(0, 8), 2);
    assert_eq!(workers_for(0, 4), 2);
    assert_eq!(workers_for(0, 2), 1);
    assert_eq!(workers_for(0, 1), 1);
    assert_eq!(workers_for(0, 64), 2);
    assert_eq!(workers_for(3, 8), 3);
    assert_eq!(workers_for(99, 8), 8);
    assert_eq!(workers_for(1, 1), 1);
}

#[test]
fn wait_idle_gives_up_when_the_scan_does_not_finish_in_time() {
    let source = Arc::new(workspace());
    let gate = ActivityGate::new();
    gate.touch();
    let mut s = settings(1);
    s.quiet_ms = 60_000;
    let indexer = Indexer::start(source, Some("crates/core/src/ui/view.rs"), s, gate);
    let started = Instant::now();
    assert!(!indexer.wait_idle(Duration::from_millis(200)));
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn the_status_sentence_is_ready_when_idle_walking_while_folders_wait_and_indexing_otherwise() {
    let busy = Progress { files_found: 10, files_done: 4, walking: false, idle: false, ..Progress::default() };
    assert_eq!(busy.l10n_key(), "azreview-symbols-status-indexing");
    assert_eq!(Progress { walking: true, ..busy }.l10n_key(), "azreview-symbols-status-walking");
    assert_eq!(Progress { idle: true, ..busy }.l10n_key(), "azreview-symbols-status-ready");
    assert_eq!(busy.l10n_args(), [("found", "10".to_string()), ("done", "4".to_string())]);
}

#[test]
fn every_reason_to_skip_a_file_has_its_own_key() {
    let mut keys: Vec<&str> = SkipReason::ALL.iter().map(|r| r.l10n_key()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), SkipReason::ALL.len());
}

#[test]
#[ignore = "X-EDGE-C-2: a measurement on a real tree, by hand: cargo test --release -p AzReview --lib -- --ignored --nocapture a_scan_of_a_real_tree_is_timed (the folder is SYMBOLS_TREE, else the repository)"]
fn a_scan_of_a_real_tree_is_timed() {
    let root = std::env::var("SYMBOLS_TREE")
        .map_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."), std::path::PathBuf::from);
    let focus = std::env::var("SYMBOLS_FOCUS").ok();
    let source = Arc::new(crate::symbols_source::DiskSource::new(root.clone()));
    let started = Instant::now();
    let indexer = Indexer::start(source, focus.as_deref(), IndexerSettings::default(), ActivityGate::new());
    assert!(indexer.wait_idle(Duration::from_secs(300)), "the scan of {} did not end in 300 s", root.display());
    let total = started.elapsed();
    let stats = indexer.index().stats();
    let p = indexer.progress();
    println!(
        "{}: {} files read of {} found ({} ignored), {} definitions, {} names, {} lines, {:.1} MB read, about {:.1} MB held, {:?}",
        root.display(),
        p.files_done,
        p.files_found,
        p.files_ignored,
        stats.defs,
        stats.names,
        stats.lines,
        stats.bytes_read as f64 / 1e6,
        stats.est_bytes as f64 / 1e6,
        total
    );
    assert!(stats.defs > 0 && p.idle);
}
