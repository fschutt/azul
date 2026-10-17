//! The open document: azul's shared rich-text model (`RichTextDoc`, the one
//! AzNotes and AzMail's compose window edit too) in the editor's state - its
//! ONE undo history - plus what AzWriter knows about the file.
//!
//! A document is a Markdown file in the data tree, `writer/<uuid>.md`
//! (`crate::storage`), read with `RichTextDoc::create_from_markdown` and
//! written with `to_markdown` (headings, lists, check items, quotes, code,
//! tables, rules and page breaks round-trip; alignment has no Markdown form).

use azul::widgets::{RichBlockKind, RichTextDoc, RichTextEditorState};

use crate::ids;

/// The name a document has before it has a title of its own.
pub const UNTITLED: &str = "Document1";

/// The document AzWriter has open.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentModel {
    /// The file's id in the data tree (`writer/<id>.md`); minted from
    /// `azul_storage::ids::random_seed`, so no two launches pick the same.
    pub id: String,
    /// The editor's state: the document and its one history.
    pub editor: RichTextEditorState,
    /// The Markdown the file holds on disk ("" before the first save).
    pub saved: String,
    /// Bumped on every change of the document: the pagination of an older
    /// generation is stale.
    pub generation: u64,
}

impl DocumentModel {
    /// A new, empty document (one empty paragraph to type into).
    #[must_use]
    pub fn untitled(id: String) -> Self {
        Self::from_doc(id, RichTextDoc::create(), String::new())
    }

    /// A document read from its file's Markdown.
    #[must_use]
    pub fn from_markdown(id: String, markdown: &str) -> Self {
        Self::from_doc(
            id,
            RichTextDoc::create_from_markdown(markdown),
            markdown.to_string(),
        )
    }

    /// A document showing `doc`; `saved` is what its file holds.
    #[must_use]
    pub fn from_doc(id: String, doc: RichTextDoc, saved: String) -> Self {
        let mut editor = RichTextEditorState::create(doc);
        editor.host_id = ids::DOC_HOST;
        Self {
            id,
            editor,
            saved,
            generation: next_generation(),
        }
    }

    /// The document (the editor's).
    #[must_use]
    pub fn doc(&self) -> &RichTextDoc {
        &self.editor.doc
    }

    /// The Markdown the document is now.
    #[must_use]
    pub fn markdown(&self) -> String {
        self.editor.doc.to_markdown().as_str().to_string()
    }

    /// Whether the document differs from its file (a new document counts
    /// once it has text).
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        if self.saved.is_empty() {
            return !self.editor.doc.is_blank();
        }
        self.markdown() != self.saved
    }

    /// The editor's new state (after typing, a command, an undo): kept,
    /// and the pagination starts over when the document changed.
    pub fn adopt(&mut self, state: RichTextEditorState) {
        let changed = state.doc != self.editor.doc;
        self.editor = state;
        if changed {
            self.generation = next_generation();
        }
    }

    /// The title: the first heading's text, else the first line of text,
    /// else [`UNTITLED`].
    #[must_use]
    pub fn title(&self) -> String {
        title_of(&self.editor.doc)
    }

    /// Words in the document.
    #[must_use]
    pub fn word_count(&self) -> usize {
        self.editor.doc.word_count()
    }

    /// The file's key in the data tree.
    #[must_use]
    pub fn key(&self) -> String {
        crate::storage::doc_key(&self.id)
    }
}

/// The title of `doc`: its first heading, else its first line of text, else
/// [`UNTITLED`] - cut to 60 characters.
#[must_use]
pub fn title_of(doc: &RichTextDoc) -> String {
    let blocks = doc.blocks.as_slice();
    let heading = blocks
        .iter()
        .find(|b| matches!(b.kind, RichBlockKind::Heading(_)))
        .map(|b| b.get_text().as_str().trim().to_string())
        .filter(|t| !t.is_empty());
    let first = || {
        blocks
            .iter()
            .map(|b| b.get_text().as_str().trim().to_string())
            .find(|t| !t.is_empty())
    };
    let title = heading.or_else(first).unwrap_or_else(|| UNTITLED.to_string());
    let line = title.lines().next().unwrap_or(UNTITLED).trim().to_string();
    if line.chars().count() > 60 {
        let cut: String = line.chars().take(59).collect();
        format!("{cut}\u{2026}")
    } else {
        line
    }
}

/// A new pagination generation (process-wide, never repeats).
pub fn next_generation() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static G: AtomicU64 = AtomicU64::new(1);
    G.fetch_add(1, Ordering::Relaxed)
}

/// A new document id: a v4 UUID from a seed that differs in every process
/// (`azul::uuid::Uuid::v4` is a process-local sequence - the first id of
/// every launch would be the same, and the new file would overwrite the
/// old; DEDUP_EDITORS B1).
#[must_use]
pub fn new_document_id() -> String {
    azul::uuid::Uuid::from_seed(azul_storage::ids::random_seed())
        .as_str()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Quarterly report\n\nSales grew **12%** in the *third* quarter.\n\n- North\n- South\n\n| Region | Sales |\n| --- | --- |\n| North | 120 |\n\n---\n\nThe end.\n";

    #[test]
    fn a_document_reads_its_markdown_and_writes_the_same_back() {
        let doc = DocumentModel::from_markdown("id".to_string(), SAMPLE);
        assert_eq!(doc.title(), "Quarterly report");
        let again = DocumentModel::from_markdown("id".to_string(), &doc.markdown());
        assert_eq!(again.doc(), doc.doc(), "Markdown -> document -> Markdown -> the same document");
        assert!(doc.word_count() >= 12);
    }

    #[test]
    fn a_document_is_dirty_when_it_differs_from_its_file() {
        let fresh = DocumentModel::untitled("a".to_string());
        assert!(!fresh.is_dirty(), "an empty new document has nothing to save");
        assert_eq!(fresh.title(), UNTITLED);
        let mut doc = DocumentModel::from_markdown("b".to_string(), SAMPLE);
        let saved = doc.markdown();
        doc.saved = saved;
        assert!(!doc.is_dirty());
        let mut edited = doc.editor.clone();
        edited.doc = RichTextDoc::create_from_markdown("# Other\n");
        let before = doc.generation;
        doc.adopt(edited);
        assert!(doc.is_dirty());
        assert_ne!(doc.generation, before, "a change re-paginates");
        assert_eq!(doc.title(), "Other");
    }

    #[test]
    fn the_editor_of_a_document_is_named_with_the_apps_prefix() {
        let doc = DocumentModel::untitled("c".to_string());
        assert!(doc.editor.host_id.as_str().starts_with("__azwriter_"));
    }

    #[test]
    fn a_new_document_id_is_a_uuid() {
        let id = new_document_id();
        assert_eq!(id.len(), 36, "{id}");
        assert_eq!(id.matches('-').count(), 4);
    }
}
