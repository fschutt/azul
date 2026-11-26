//! The symbol index in the window: what the dictation listens for, how its words are resolved against the workspace, and
//! what an agent is told of them. A small workspace in memory; the scan is waited for, no timer and no window.

use std::time::Duration;

use super::*;
use crate::{dictation::HeardWord, symbols_source::MemSource};

fn workspace() -> Arc<MemSource> {
    let mut source = MemSource::new();
    source.put(
        "src/stale.rs",
        b"pub fn compute_stale_flag(x: u32) -> bool {\n    x > 3\n}\n\npub struct CssError {\n    pub line: u32,\n}\n".to_vec(),
    );
    source.put("src/other.rs", b"pub fn render_page() {}\n".to_vec());
    Arc::new(source)
}

fn started() -> Hub {
    let mut hub = Hub::none();
    hub.start(workspace(), Some("src/stale.rs"));
    assert!(hub.wait_idle(Duration::from_secs(20)), "the small workspace is scanned");
    hub
}

fn said(words: &[(&str, f32)]) -> Said {
    Said {
        text: words.iter().map(|(w, _)| *w).collect::<Vec<_>>().join(" "),
        started_unix_ms: 1_000,
        words: words
            .iter()
            .enumerate()
            .map(|(i, (w, p))| HeardWord { text: (*w).to_string(), start_ms: i as u32 * 300, end_ms: i as u32 * 300 + 250, probability: *p })
            .collect(),
    }
}

#[test]
fn a_window_that_made_no_index_listens_for_the_words_of_the_page_as_before() {
    let hub = Hub::none();
    assert!(!hub.is_running());
    let lines = vec!["pub fn compute_stale_flag() {}".to_string()];
    let words = hub.vocabulary(&lines, Some("a.rs"));
    assert!(words.iter().any(|w| w == "compute_stale_flag"), "{words:?}");
    assert!(hub.progress().is_none());
}

#[test]
fn what_the_index_knows_is_what_the_recogniser_is_told_to_listen_for() {
    let hub = started();
    assert!(hub.is_running());
    // the page shown is another file's: the index still knows the names of this one
    let lines = vec!["pub fn render_page() {}".to_string()];
    let words = hub.vocabulary(&lines, Some("src/other.rs"));
    assert!(words.iter().any(|w| w == "render_page"), "the page's own names: {words:?}");
    assert!(words.iter().any(|w| w == "compute_stale_flag" || w == "CssError"), "the workspace's names: {words:?}");
}

#[test]
fn words_that_say_a_name_are_written_as_the_name_beside_the_words_as_heard() {
    let mut hub = started();
    let heard = said(&[("compute", 0.9), ("stale", 0.8), ("flag", 0.9)]);
    let res = hub.resolve(&heard, Some("src/stale.rs"));
    assert_eq!(res.raw, "compute stale flag");
    assert_eq!(res.corrected, "compute_stale_flag");
    assert_eq!(res.certain.len(), 1);
    assert_eq!(res.certain[0].def.name, "compute_stale_flag");
}

#[test]
fn a_doubted_word_of_a_name_is_repaired_and_a_name_found_is_remembered_for_the_conversation() {
    let mut hub = started();
    let heard = said(&[("compute", 0.9), ("stale", 0.2), ("flag", 0.9)]);
    let res = hub.resolve(&heard, None);
    assert_eq!(res.corrected, "compute_stale_flag", "the word the recogniser doubted is read as the name's word");
    assert_eq!(hub.recent_len(), 1);
    // the same name again does not grow the list
    let _ = hub.resolve(&said(&[("compute", 0.9), ("stale", 0.9), ("flag", 0.9)]), None);
    assert_eq!(hub.recent_len(), 1);
}

#[test]
fn words_with_no_times_or_probabilities_are_still_resolved_as_words_the_recogniser_did_not_rate() {
    let mut hub = started();
    let typed = Said { text: "rename compute stale flag".to_string(), started_unix_ms: 0, words: Vec::new() };
    let res = hub.resolve(&typed, Some("src/stale.rs"));
    assert_eq!(res.corrected, "rename compute_stale_flag");
}

#[test]
fn nothing_said_resolves_to_nothing_and_a_window_with_no_index_resolves_nothing() {
    let mut hub = started();
    let none = hub.resolve(&Said::plain(""), None);
    assert!(none.certain.is_empty() && none.corrected.is_empty());
    let mut bare = Hub::none();
    let res = bare.resolve(&said(&[("compute", 0.9), ("stale", 0.9), ("flag", 0.9)]), None);
    assert!(res.certain.is_empty(), "no index, no definition");
    assert_eq!(res.corrected, res.raw, "nothing is changed");
}

#[test]
fn what_an_agent_is_told_of_the_names_is_where_they_are_defined() {
    let mut hub = started();
    let source = workspace();
    let res = hub.resolve(&said(&[("compute", 0.9), ("stale", 0.8), ("flag", 0.9)]), Some("src/stale.rs"));
    let section = hub.section(&res, &*source);
    assert!(section.contains("compute_stale_flag"), "{section}");
    assert!(section.contains("src/stale.rs"), "{section}");
    // words that name nothing add nothing to the prompt
    let nothing = hub.resolve(&said(&[("hello", 0.9), ("there", 0.9)]), None);
    assert_eq!(hub.section(&nothing, &*source), "");
}

#[test]
fn the_words_of_a_page_are_cut_to_what_a_recogniser_can_use() {
    let lines: Vec<String> = (0..10_000).map(|i| format!("pub fn name_{i}() {{}}")).collect();
    let text = page_text(&lines);
    assert!(text.len() <= MAX_PAGE_TEXT, "{} bytes", text.len());
    assert!(text.starts_with("pub fn name_0()"));
    assert_eq!(page_text(&[]), "");
}

#[test]
fn the_status_line_says_how_far_the_scan_is_and_nothing_once_it_is_done() {
    let walking = Progress { files_found: 10, files_done: 2, walking: true, ..Progress::default() };
    assert!(status_of(&walking).is_some());
    let indexing = Progress { files_found: 10, files_done: 2, ..Progress::default() };
    assert!(status_of(&indexing).is_some_and(|(key, args)| key == "azreview-symbols-status-indexing" && args.contains(&("done", "2".to_string()))));
    let ready = Progress { files_found: 10, files_done: 10, idle: true, ..Progress::default() };
    assert_eq!(status_of(&ready), None, "a finished scan says nothing: the tree's foot is the reviewer's");
}

#[test]
fn starting_another_scan_ends_the_one_before_and_a_hub_can_be_stopped() {
    let mut hub = Hub::none();
    hub.start(workspace(), None);
    hub.start(workspace(), Some("src/other.rs"));
    assert!(hub.wait_idle(Duration::from_secs(20)));
    hub.stop();
    assert!(!hub.is_running());
    hub.stop(); // twice is nothing
}

#[test]
fn the_pen_and_the_wheel_touch_the_gate_and_a_hub_with_no_index_takes_the_touch_quietly() {
    let hub = Hub::none();
    hub.touch();
    let hub = started();
    hub.touch();
    hub.set_focus("src/other.rs");
}
