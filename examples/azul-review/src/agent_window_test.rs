//! What the window keeps of its agents: the settings it starts from, the answers that wait for a file that is not open,
//! and what an answer does to a conversation. No window, no thread and no agent.

use super::*;
use crate::{
    agent_backend::{Body, Identity, Usage},
    agent_review::Suggestion,
    conversation::Phase,
    model::{Note, NoteState},
};

fn hub(values: &[(&str, &str)], backend: Option<BackendKind>) -> Hub {
    let values: BTreeMap<String, String> = values.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
    Hub::new(&values, vec![("PATH".to_string(), "/usr/bin".to_string())], Path::new("data-root"), backend, 10, true)
}

fn note() -> Note {
    Note {
        id: 1,
        page: 0,
        x: 10.0,
        y: 40.0,
        text: "rename this".to_string(),
        language: "en".to_string(),
        state: NoteState::Done,
        talk: crate::conversation::Conversation::default(),
    }
}

fn response(reply: Reply) -> Response {
    Response {
        body: Body::Review(reply),
        usage: Usage::default(),
        identity: Identity { kind: BackendKind::Mock, model: String::new() },
        diagnostics: Vec::new(),
    }
}

#[test]
fn a_window_asked_for_the_mock_has_the_mock_for_every_task() {
    let h = hub(&[], Some(BackendKind::Mock));
    for task in Task::all() {
        assert_eq!(h.settings.task(task).backend, BackendKind::Mock, "{task:?}");
    }
    assert!(h.auto_ask);
}

#[test]
fn a_window_that_asked_for_nothing_starts_from_the_settings_file_and_says_what_it_held() {
    let h = hub(&[("agent-max-concurrent", "40")], None);
    assert_eq!(h.pool.cap(), 8, "a number a person cannot mean is held to the most");
    assert!(!h.problems.is_empty(), "and the log is told");
    let plain = hub(&[], None);
    assert_eq!(plain.pool.cap(), crate::agent_pool::DEFAULT_CAP);
    assert!(plain.problems.is_empty());
    assert_eq!(plain.settings.task(Task::Editing).backend, BackendKind::ClaudePrint, "an agent with tools of its own by default");
}

#[test]
fn an_answer_for_a_file_that_is_not_open_waits_for_it_and_is_taken_once() {
    let mut h = hub(&[], None);
    h.keep_late(Late { file: "a.rs".to_string(), note: 1, outcome: Outcome::Failed("x".to_string()) });
    h.keep_late(Late { file: "b.rs".to_string(), note: 2, outcome: Outcome::Failed("y".to_string()) });
    h.keep_late(Late { file: "a.rs".to_string(), note: 3, outcome: Outcome::Failed("z".to_string()) });
    let mine = h.take_late("a.rs");
    assert_eq!(mine.iter().map(|l| l.note).collect::<Vec<_>>(), vec![1, 3]);
    assert!(h.take_late("a.rs").is_empty(), "taken once");
    assert_eq!(h.late.len(), 1, "the other file's answer still waits");
    assert_eq!(h.take_late("b.rs")[0].note, 2);
}

#[test]
fn the_answers_that_wait_are_bounded_and_the_oldest_goes() {
    let mut h = hub(&[], None);
    for i in 0..(MAX_LATE + 3) as u64 {
        h.keep_late(Late { file: "a.rs".to_string(), note: i, outcome: Outcome::Failed(String::new()) });
    }
    assert_eq!(h.late.len(), MAX_LATE);
    assert_eq!(h.late[0].note, 3, "the first three went");
}

#[test]
fn an_answer_of_words_lands_as_an_agent_turn_and_the_conversation_is_done() {
    let mut n = note();
    n.talk.asking();
    let state = land(&mut n, Outcome::Reply(Reply { answer: "Renamed.".to_string(), ..Reply::default() }), 5);
    assert_eq!(state, chips::State::Done);
    assert_eq!(n.talk.phase, Phase::Done);
    assert_eq!(n.talk.turns[0].text, "Renamed.");
}

#[test]
fn an_answer_with_suggestions_makes_the_conversation_wait_for_the_reviewer() {
    let mut n = note();
    n.talk.asking();
    let reply = Reply {
        answer: "One line.".to_string(),
        suggestions: vec![Suggestion::InsertLines { after_line: 1, lines: vec!["x".to_string()] }],
        ..Reply::default()
    };
    assert_eq!(land(&mut n, Outcome::Reply(reply), 5), chips::State::Suggested);
}

#[test]
fn an_agent_that_does_nothing_leaves_the_words_and_shows_nothing() {
    let mut n = note();
    n.talk.asking();
    let state = land(&mut n, Outcome::Reply(Reply::nothing("a test phrase")), 5);
    assert_eq!(state, chips::State::Draft);
    assert!(n.talk.turns.is_empty(), "nothing is added to what is shown");
    assert_eq!(n.text, "rename this", "the reviewer's words stay");
}

#[test]
fn a_failure_lands_with_its_words_and_can_be_asked_again() {
    let mut n = note();
    n.talk.asking();
    assert_eq!(land(&mut n, Outcome::Failed("claude is not installed".to_string()), 5), chips::State::Failed);
    assert_eq!(n.talk.problem.as_deref(), Some("claude is not installed"));
    assert!(n.talk.can_ask());
}

#[test]
fn what_an_answer_is_comes_from_its_reply_or_from_the_words_of_its_failure() {
    crate::l10n::in_english();
    let ok = Ok(response(Reply { answer: "Fine.".to_string(), ..Reply::default() }));
    assert_eq!(outcome_of(&ok), Outcome::Reply(Reply { answer: "Fine.".to_string(), ..Reply::default() }));
    let failed = Err(AgentError::NotInstalled { program: "claude".to_string(), searched: vec!["/usr/bin".to_string()] });
    match outcome_of(&failed) {
        Outcome::Failed(why) => assert!(why.contains("claude") && why.contains("not installed"), "{why}"),
        other => panic!("a failure, got {other:?}"),
    }
}

#[test]
fn an_answer_that_is_no_review_is_a_failure_and_not_a_silence() {
    crate::l10n::in_english();
    let export = Response {
        body: Body::Export(crate::agent_export::CutList { title: String::new(), segments: Vec::new(), chapters: Vec::new() }),
        usage: Usage::default(),
        identity: Identity { kind: BackendKind::Mock, model: String::new() },
        diagnostics: Vec::new(),
    };
    assert!(matches!(outcome_of(&Ok(export)), Outcome::Failed(_)));
}

// ==== Who is told when a conversation finishes ====

use std::sync::Mutex;

/// An `Events` that writes down what it was told.
#[derive(Default)]
struct Recorder(Mutex<Vec<Finished>>);

impl Events for Recorder {
    fn finished(&self, event: &Finished) {
        self.0.lock().expect("the lock").push(event.clone());
    }
}

impl Recorder {
    fn told(&self) -> Vec<Finished> {
        self.0.lock().expect("the lock").clone()
    }
}

fn flight(file: &str, note: u64) -> InFlight {
    InFlight { file: file.to_string(), note, task: Task::Editing, line: 12 }
}

fn hub_with(recorder: &Arc<Recorder>) -> Hub {
    let mut h = hub(&[], None);
    h.events = recorder.clone();
    h
}

#[test]
fn every_end_of_a_conversation_has_its_kind() {
    assert_eq!(Outcome::Reply(Reply { answer: "x".to_string(), ..Reply::default() }).finish(), Finish::Done);
    let two = Reply {
        suggestions: vec![
            Suggestion::InsertLines { after_line: 1, lines: vec!["a".to_string()] },
            Suggestion::InsertLines { after_line: 2, lines: vec!["b".to_string()] },
        ],
        ..Reply::default()
    };
    assert_eq!(Outcome::Reply(two).finish(), Finish::Suggested(2));
    assert_eq!(Outcome::Reply(Reply::nothing("a test phrase")).finish(), Finish::Quiet);
    assert_eq!(Outcome::Failed("no".to_string()).finish(), Finish::Failed);
}

#[test]
fn an_answer_for_the_open_file_lands_there_and_whoever_listens_is_told() {
    let recorder = Arc::new(Recorder::default());
    let mut h = hub_with(&recorder);
    let outcome = Outcome::Reply(Reply { answer: "Fine.".to_string(), ..Reply::default() });
    let answered = h.answered(Some("a.rs"), &flight("a.rs", 4), outcome.clone());
    assert_eq!(answered, Answered::Lands(outcome));
    assert_eq!(recorder.told(), vec![Finished { file: "a.rs".to_string(), conversation: 4, line: 12, finish: Finish::Done }]);
    assert!(h.late.is_empty());
}

#[test]
fn an_answer_for_a_file_that_is_not_open_waits_for_it_with_its_colour_and_whoever_listens_is_told_now() {
    let recorder = Arc::new(Recorder::default());
    let mut h = hub_with(&recorder);
    let suggested = Reply {
        answer: "One.".to_string(),
        suggestions: vec![Suggestion::InsertLines { after_line: 1, lines: vec!["a".to_string()] }],
        ..Reply::default()
    };
    let answered = h.answered(Some("b.rs"), &flight("a.rs", 4), Outcome::Reply(suggested));
    assert_eq!(answered, Answered::Waits(chips::State::Suggested));
    assert_eq!(h.late.len(), 1);
    assert_eq!(recorder.told(), vec![Finished { file: "a.rs".to_string(), conversation: 4, line: 12, finish: Finish::Suggested(1) }], "the phone is told at once, not when the file is opened");
    // no file open at all is another file
    let none = h.answered(None, &flight("a.rs", 5), Outcome::Failed("x".to_string()));
    assert_eq!(none, Answered::Waits(chips::State::Failed));
    let quiet = h.answered(Some("b.rs"), &flight("a.rs", 6), Outcome::Reply(Reply::nothing("nonsense")));
    assert_eq!(quiet, Answered::Waits(chips::State::Draft));
    assert_eq!(recorder.told().iter().map(|f| f.finish).collect::<Vec<_>>(), vec![Finish::Suggested(1), Finish::Failed, Finish::Quiet]);
}

#[test]
fn a_worker_that_never_reported_is_a_failure_in_words_and_is_told() {
    crate::l10n::in_english();
    let recorder = Arc::new(Recorder::default());
    let mut h = hub_with(&recorder);
    let answered = h.answered(Some("a.rs"), &flight("a.rs", 2), Outcome::Failed(t("azreview-agent-no-answer")));
    assert!(matches!(&answered, Answered::Lands(Outcome::Failed(why)) if why.contains("No answer")), "{answered:?}");
    assert_eq!(recorder.told()[0].finish, Finish::Failed);
}

#[test]
fn a_window_with_nobody_listening_tells_nobody() {
    let mut h = hub(&[], None);
    // the default listener does nothing, and a conversation still lands
    let _ = h.answered(Some("a.rs"), &flight("a.rs", 1), Outcome::Failed("x".to_string()));
    assert!(h.late.is_empty());
}
