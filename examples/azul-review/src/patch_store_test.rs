//! What a resolved conversation writes: the patch of what the reviewer took, or a header alone. The pure part of
//! `patch_store`: notes and bytes in, text out. No disk, no window.

use super::*;
use crate::{
    agent_review::{Pos, Reply, Suggestion},
    conversation::{Conversation, Decision, Phase, Proposal, Said},
    model::{Note, NoteState},
};

const NOW: u64 = 1_764_135_612_000;

fn note(id: u64, text: &str) -> Note {
    Note { id, page: 0, x: 0.0, y: 0.0, text: text.to_string(), language: "en".to_string(), state: NoteState::Done, talk: Conversation::default() }
}

fn file_text() -> Vec<u8> {
    (1..=12).map(|i| format!("line {i}\n")).collect::<String>().into_bytes()
}

fn insert_after(line: usize, row: &str) -> Suggestion {
    Suggestion::InsertLines { after_line: line, lines: vec![row.to_string()] }
}

fn taken(n: &mut Note, suggestion: Suggestion) {
    n.talk.proposals.push(Proposal { suggestion, decision: Decision::Accepted });
    n.talk.phase = Phase::Done;
}

fn said(text: &str) -> Said {
    Said { text: text.to_string(), started_unix_ms: 1, words: Vec::new() }
}

#[test]
fn what_the_reviewer_took_is_a_patch_with_a_header_that_names_the_conversation() {
    let mut n = note(7, "rename step three");
    n.talk.hear_understood(said("rename step three"), Some("rename step_3".to_string()));
    n.talk.receive(&Reply { answer: "One line goes in.".to_string(), suggestions: vec![insert_after(4, "// the fourth")], ..Reply::default() }, 5);
    n.talk.accept_all();
    n.talk.base = Some("3833f10608241be94251152cd9c4da3de1b6a283".to_string());
    let m = made(&n, "src/lib.rs", Ok(&file_text()), NOW);
    assert_eq!(m.outcome, Outcome::Accepted);
    assert!(m.changed);
    assert_eq!(m.name, "20251126-054012-c7.patch");
    let head: Vec<&str> = m.text.lines().take(10).collect();
    assert_eq!(head[0], "AzReview-Patch: 1");
    assert_eq!(head[1], "Conversation: 7");
    assert_eq!(head[2], "Outcome: accepted");
    assert_eq!(head[3], "Base: 3833f10608241be94251152cd9c4da3de1b6a283");
    assert_eq!(head[5], "Said: rename step three");
    assert_eq!(head[6], "Understood: rename step_3");
    assert_eq!(head[7], "Rationale: One line goes in.");
    assert_eq!(head[8], "Files: src/lib.rs");
    assert!(m.text.contains("diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,6 +2,7 @@\n line 2\n line 3\n line 4\n+// the fourth\n line 5\n line 6\n line 7\n"), "{}", m.text);
}

#[test]
fn a_conversation_that_took_nothing_is_a_header_alone_with_how_it_ended() {
    let mut answered = note(1, "why is this read twice");
    answered.talk.receive(&Reply { answer: "It is read once more in the loop.".to_string(), ..Reply::default() }, 5);
    let m = made(&answered, "src/lib.rs", Ok(&file_text()), NOW);
    assert_eq!(m.outcome, Outcome::Answered);
    assert!(!m.changed);
    assert!(m.text.contains("Outcome: answered\n") && m.text.contains("Files: none\n") && m.text.contains("Base: none\n"), "{}", m.text);
    assert!(!m.text.contains("diff --git"));

    let mut refused = note(2, "drop it");
    refused.talk.receive(&Reply { suggestions: vec![insert_after(2, "x")], ..Reply::default() }, 5);
    refused.talk.reject_all();
    let m = made(&refused, "src/lib.rs", Ok(&file_text()), NOW);
    assert_eq!((m.outcome, m.changed), (Outcome::Rejected, false));

    let mut closed = note(3, "never mind");
    closed.talk.resolve();
    let m = made(&closed, "src/lib.rs", Ok(&file_text()), NOW);
    assert_eq!(m.outcome, Outcome::Closed);
    assert!(m.text.contains("Said: never mind\n"), "the note's words when nothing was dictated: {}", m.text);
}

#[test]
fn the_words_of_the_header_are_the_dictation_and_the_follow_ups_and_every_answer_of_the_agent() {
    let mut n = note(4, "the note's own words");
    n.talk.hear(said("first words"));
    n.talk.hear(said("and then these"));
    n.talk.say("please also check the test", 3);
    n.talk.receive(&Reply { answer: "Done once.".to_string(), ..Reply::default() }, 4);
    n.talk.say("and again", 5);
    n.talk.receive(&Reply { answer: "Done twice.".to_string(), ..Reply::default() }, 6);
    let m = made(&n, "a.rs", Ok(&file_text()), NOW);
    assert!(m.text.contains("Said: first words and then these / please also check the test / and again\n"), "{}", m.text);
    assert!(m.text.contains("Rationale: Done once. / Done twice.\n"), "{}", m.text);
}

#[test]
fn a_file_that_cannot_be_read_is_a_header_that_says_so_and_still_says_what_was_taken() {
    let mut n = note(5, "add a line");
    taken(&mut n, insert_after(2, "x"));
    let m = made(&n, "src/lib.rs", Err("the file is gone"), NOW);
    assert_eq!(m.outcome, Outcome::Accepted);
    assert!(!m.changed);
    assert!(m.text.contains("Left-out: the file could not be read: the file is gone\n"), "{}", m.text);
}

#[test]
fn a_suggestion_the_file_no_longer_matches_is_left_out_and_said_in_the_header() {
    let mut n = note(6, "rename it");
    taken(&mut n, Suggestion::ReplaceRange { start: Pos { line: 3, col: 5 }, end: Pos { line: 3, col: 6 }, old: "9".to_string(), text: "nine".to_string() });
    taken(&mut n, insert_after(6, "kept"));
    let m = made(&n, "src/lib.rs", Ok(&file_text()), NOW);
    assert!(m.changed);
    assert!(m.text.contains("+kept\n") && !m.text.contains("nine"), "{}", m.text);
    assert!(m.text.contains("Left-out: line 3: the words \"9\" are not what the file says there any more\n"), "{}", m.text);
}

#[test]
fn a_file_with_windows_line_ends_and_one_that_ends_without_a_newline_are_read_as_they_are() {
    let crlf: Vec<u8> = b"one\r\ntwo\r\nthree\r\n".to_vec();
    assert_eq!(raw_lines(&crlf), vec!["one\r".to_string(), "two\r".to_string(), "three\r".to_string()]);
    assert_eq!(raw_lines(b"a\nb"), vec!["a".to_string(), "b".to_string()], "no newline at the end: the last line is a line");
    assert_eq!(raw_lines(b"a\nb\n"), vec!["a".to_string(), "b".to_string()]);
    assert_eq!(raw_lines(b"a\n\n"), vec!["a".to_string(), String::new()], "an empty last line is one");
    assert!(raw_lines(b"").is_empty());
    let mut n = note(8, "add");
    taken(&mut n, insert_after(1, "added"));
    let m = made(&n, "a.rs", Ok(&crlf), NOW);
    assert!(m.text.contains("+added\r\n"), "{:?}", m.text);
}

#[test]
fn a_path_of_a_windows_workspace_is_a_path_of_the_patch() {
    let mut n = note(9, "add");
    taken(&mut n, insert_after(1, "added"));
    let m = made(&n, "src\\lib.rs", Ok(&file_text()), NOW);
    assert!(m.text.contains("Files: src/lib.rs\n") && m.text.contains("diff --git a/src/lib.rs b/src/lib.rs\n"), "{}", m.text);
}

#[test]
fn the_patches_of_a_workspace_are_in_a_folder_of_their_own_next_to_its_progress() {
    let key = key_of("/work/azul", "20251126-054012-c7.patch");
    assert!(key.starts_with("review/patches-") && key.ends_with("/20251126-054012-c7.patch"), "{key}");
    assert_ne!(key_of("/work/azul", "a.patch"), key_of("/work/other", "a.patch"), "each workspace has its own folder");
    assert_eq!(key_of("/work/azul", "a.patch"), key_of("/work/azul", "a.patch"));
}

#[test]
fn a_conversation_resolved_again_replaces_its_patch_and_one_that_wrote_the_same_name_is_not_removed() {
    assert_eq!(superseded(Some("20251126-054012-c7.patch"), "20251126-055000-c7.patch"), Some("20251126-054012-c7.patch".to_string()));
    assert_eq!(superseded(Some("20251126-054012-c7.patch"), "20251126-054012-c7.patch"), None, "the same file is written over");
    assert_eq!(superseded(None, "20251126-054012-c7.patch"), None);
    assert_eq!(superseded(Some("../../etc/passwd"), "20251126-054012-c7.patch"), None, "what is not a patch's name is never removed");
}

// ---- the revision a conversation began at ----

const HASH: &str = "0123456789abcdef0123456789abcdef01234567";
const OTHER: &str = "fedcba9876543210fedcba9876543210fedcba98";

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("azreview-base-{tag}-{}-{:?}", std::process::id(), std::thread::current().id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

fn write(path: &std::path::Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
    std::fs::write(path, text).expect("a file");
}

#[test]
fn the_base_of_a_workspace_is_the_hash_its_branch_is_at() {
    let root = scratch("branch");
    write(&root.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(&root.join(".git/refs/heads/main"), &format!("{HASH}\n"));
    assert_eq!(base_of(&root).as_deref(), Some(HASH));
    // from a folder below the working tree too
    std::fs::create_dir_all(root.join("src/deep")).expect("a folder");
    assert_eq!(base_of(&root.join("src/deep")).as_deref(), Some(HASH));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn no_folder_at_all_is_no_repository_and_never_the_one_the_process_happens_to_run_in() {
    assert_eq!(base_of(std::path::Path::new("")), None);
}

#[test]
fn a_detached_head_is_the_hash_itself_and_a_branch_that_is_not_born_is_none() {
    let detached = scratch("detached");
    write(&detached.join(".git/HEAD"), &format!("{HASH}\n"));
    assert_eq!(base_of(&detached).as_deref(), Some(HASH));
    let unborn = scratch("unborn");
    write(&unborn.join(".git/HEAD"), "ref: refs/heads/main\n");
    assert_eq!(base_of(&unborn), None, "no commit yet: no revision");
    let odd = scratch("odd");
    write(&odd.join(".git/HEAD"), "not a hash\n");
    assert_eq!(base_of(&odd), None);
    for d in [detached, unborn, odd] {
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn a_worktree_whose_git_is_a_file_is_followed_and_a_folder_with_no_repository_has_no_base() {
    let root = scratch("worktree");
    let gitdir = root.join("gitdirs/wt");
    write(&gitdir.join("HEAD"), &format!("{OTHER}\n"));
    write(&root.join("tree/.git"), &format!("gitdir: {}\n", gitdir.display()));
    assert_eq!(base_of(&root.join("tree")).as_deref(), Some(OTHER));
    let _ = std::fs::remove_dir_all(&root);
    let none = scratch("none");
    // a folder under a temp folder that is no repository (a repository above the temp folder would be a repository)
    let found = base_of(&none);
    assert!(found.is_none() || std::env::temp_dir().ancestors().any(|a| a.join(".git").exists()), "{found:?}");
    let _ = std::fs::remove_dir_all(&none);
}

#[test]
fn the_header_says_the_revision_the_conversation_began_at_and_none_where_that_was_not_kept() {
    let mut began = note(5, "rename it");
    began.talk.base = Some(HASH.to_string());
    began.talk.receive(&Reply { answer: "Done.".to_string(), ..Reply::default() }, 5);
    let m = made(&began, "src/lib.rs", Ok(&file_text()), NOW);
    assert!(m.text.contains(&format!("Base: {HASH}\n")), "{}", m.text);
    let mut unknown = note(6, "and this");
    unknown.talk.receive(&Reply { answer: "Done.".to_string(), ..Reply::default() }, 5);
    let m = made(&unknown, "src/lib.rs", Ok(&file_text()), NOW);
    assert!(m.text.contains("Base: none\n"), "{}", m.text);
}
