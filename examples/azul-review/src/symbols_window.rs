//! The symbol index in the window: what the dictation listens for, how what was said is resolved against the workspace, and
//! what an agent is told of the names the reviewer said.
//!
//! The index (`symbols_sched`) is built behind the window by a few low-priority threads, outwards from the file the
//! reviewer is looking at, and waits while the pen draws or the wheel scrolls (`touch`). Three things use it:
//!
//! - the recogniser's vocabulary at the start of a dictation: the page's names, the file's, the folder's and the project's,
//!   and the names found lately in this conversation, instead of the words of the page alone ([`Hub::vocabulary`]);
//! - each utterance, resolved against the index ([`Hub::resolve`]): words that say a name are written as the name BESIDE the
//!   words as heard (the heard words are never touched), a word the recogniser doubted is read as the name's word, and a
//!   definition found is remembered for the next utterance ([`MAX_RECENT`]);
//! - the request to the agent: where the names are defined and a snippet of each, as a section of its own
//!   ([`Hub::section`], `ReviewRequest::symbols`).
//!
//! No global state: the gate, the threads and what was found are fields of the [`Hub`], and stopping it (or dropping it)
//! joins the threads. A hub with no index ([`Hub::none`]: the tests, a window with no folder) falls back to the words of the
//! page and resolves nothing.

use std::{sync::Arc, time::Duration};

use crate::{
    dictation::Said,
    symbols::DefInfo,
    symbols_context::{build_section, ContextBudget},
    symbols_resolve::{Context, Resolution, Resolver},
    symbols_sched::{ActivityGate, Indexer, IndexerSettings, Progress},
    symbols_source::FileSource,
    symbols_spoken::HeardWord as SpokenWord,
    symbols_vocab::{build, VocabContext, VocabOptions},
};

/// The most text of a page the vocabulary reads names from, bytes.
pub const MAX_PAGE_TEXT: usize = 24 * 1024;
/// How many definitions found lately are kept for the next utterance.
pub const MAX_RECENT: usize = 40;
/// The words the recogniser is told to listen for when there is no index: the page's own.
const PAGE_WORDS_CAP: usize = 200;

/// The index of a window and what the conversation found in it.
pub struct Hub {
    indexer: Option<Indexer>,
    gate: ActivityGate,
    /// The definitions found lately, oldest first.
    recent: Vec<DefInfo>,
}

impl Hub {
    /// A hub with no index.
    #[must_use]
    pub fn none() -> Self {
        Self { indexer: None, gate: ActivityGate::new(), recent: Vec::new() }
    }

    /// Starts the scan of the workspace `source`, outwards from `open` (the file shown); the scan of an earlier workspace
    /// ends. Returns at once.
    pub fn start(&mut self, source: Arc<dyn FileSource>, open: Option<&str>) {
        self.stop();
        self.recent.clear();
        self.indexer = Some(Indexer::start(source, open, IndexerSettings::default(), self.gate.clone()));
    }

    /// Ends the scan (its threads are joined). The hub then has no index.
    pub fn stop(&mut self) {
        if let Some(mut indexer) = self.indexer.take() {
            indexer.stop();
        }
    }

    /// Whether an index is being built or has been.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.indexer.is_some()
    }

    /// The user draws or scrolls: the scan keeps to the neighbourhood of the file shown until it is quiet again.
    pub fn touch(&self) {
        self.gate.touch();
    }

    /// The reviewer looks at another file: the files near it are scanned next.
    pub fn set_focus(&self, rel: &str) {
        if let Some(indexer) = &self.indexer {
            indexer.set_focus(rel);
        }
    }

    /// How the scan stands (none: no index).
    #[must_use]
    pub fn progress(&self) -> Option<Progress> {
        self.indexer.as_ref().map(Indexer::progress)
    }

    /// Waits until the scan is idle, at most `timeout` (a hub with no index is idle).
    #[must_use]
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        self.indexer.as_ref().is_none_or(|i| i.wait_idle(timeout))
    }

    /// How many definitions found lately are kept.
    #[must_use]
    pub fn recent_len(&self) -> usize {
        self.recent.len()
    }

    /// The words the recogniser is told to listen for at the start of a dictation over the page `lines` of `file`: the
    /// page's names, the file's, what was found lately, the folder's and the project's; with no index, the page's words.
    #[must_use]
    pub fn vocabulary(&self, lines: &[String], file: Option<&str>) -> Vec<String> {
        let Some(indexer) = &self.indexer else {
            return crate::vocabulary::vocabulary(lines, file.unwrap_or(""), PAGE_WORDS_CAP);
        };
        let page = page_text(lines);
        let recent: Vec<String> = self.recent.iter().rev().map(|d| d.name.clone()).collect();
        let context = VocabContext { page_text: &page, file, recent: &recent };
        let index = indexer.index();
        build(&index, &context, &VocabOptions::default()).words
    }

    /// Resolves one utterance against the index: the words that say a name are written as the name beside the words as
    /// heard; a definition found is remembered. With no index nothing is resolved and nothing is changed.
    pub fn resolve(&mut self, said: &Said, file: Option<&str>) -> Resolution {
        let words = spoken_words(said);
        let Some(indexer) = &self.indexer else {
            let raw = words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
            return Resolution { corrected: raw.clone(), raw, ..Resolution::default() };
        };
        let resolution = {
            let index = indexer.index();
            Resolver::new(&index).resolve(&words, &Context { file, recent: &self.recent })
        };
        for certain in &resolution.certain {
            self.remember(certain.def.clone());
        }
        resolution
    }

    /// What an agent is told of the names in `resolution`: where each is defined and a snippet of it (read through
    /// `source`), as a section of the prompt; empty when the words named nothing.
    #[must_use]
    pub fn section(&self, resolution: &Resolution, source: &dyn FileSource) -> String {
        let section = build_section(resolution, source, &ContextBudget::default());
        if section.is_empty() {
            String::new()
        } else {
            section.render_prompt()
        }
    }

    /// Keeps a definition for the next utterance: the newest last, the same one once, at most [`MAX_RECENT`].
    fn remember(&mut self, def: DefInfo) {
        self.recent.retain(|d| !(d.file == def.file && d.name == def.name && d.line == def.line));
        if self.recent.len() >= MAX_RECENT {
            self.recent.remove(0);
        }
        self.recent.push(def);
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The words of an utterance as the resolver takes them: each with the probability the recogniser gave it (0 where it does
/// not say), or the words of the text where the recogniser gave none.
#[must_use]
pub fn spoken_words(said: &Said) -> Vec<SpokenWord> {
    if said.words.is_empty() {
        return said
            .text
            .split_whitespace()
            .map(|w| SpokenWord { text: w.to_string(), probability: 0.0, biased: false })
            .collect();
    }
    said.words
        .iter()
        .map(|w| SpokenWord { text: w.text.clone(), probability: w.probability, biased: false })
        .collect()
}

/// The text of a page for the vocabulary: its lines, cut to [`MAX_PAGE_TEXT`] bytes at a line.
#[must_use]
pub fn page_text(lines: &[String]) -> String {
    let mut out = String::new();
    for line in lines {
        if out.len() + line.len() + 1 > MAX_PAGE_TEXT {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// The status sentence of the scan (its key and arguments), while it is not done; none once it is.
#[must_use]
pub fn status_of(progress: &Progress) -> Option<(&'static str, Vec<(&'static str, String)>)> {
    if progress.idle {
        None
    } else {
        Some((progress.l10n_key(), progress.l10n_args()))
    }
}

#[cfg(test)]
#[path = "symbols_window_test.rs"]
mod symbols_window_test;
