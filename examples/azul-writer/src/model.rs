//! An open document: azul's shared rich-text model (`RichTextDoc`, the one
//! AzNotes and AzMail's compose window edit too) in the editor's state - its
//! ONE undo history - plus what AzWriter knows about the file and about the
//! document's row in the sidebar (its pages, its save, its collaboration switch).
//!
//! A document is a Markdown file in the data tree, `writer/<uuid>.md`
//! (`crate::storage`), read with `RichTextDoc::create_from_markdown` and
//! written with `to_markdown` (headings, lists, check items, quotes, code,
//! tables, rules and page breaks round-trip; alignment has no Markdown form).

use azul::widgets::{RichBlockKind, RichTextDoc, RichTextEditorState};

use crate::{ids, paginate::Pages};

/// The name a document has before it has a title of its own.
pub const UNTITLED: &str = "Document1";

/// A document AzWriter has open.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentModel {
    /// The file's id in the data tree (`writer/<id>.md`); minted from
    /// `azul_storage::ids::random_seed`, so no two launches pick the same.
    pub id: String,
    /// The editor's state: the document and its one history.
    pub editor: RichTextEditorState,
    /// The Markdown the file holds on disk ("" before the first save).
    pub saved: String,
    /// Counts the changes of the document: the pagination of an older
    /// generation is stale. Every open document counts its own, from 1.
    pub generation: u64,
    /// Where the pages of the print layout start, for `pages.generation`.
    pub pages: Pages,
    /// The generation a pagination thread is working on for this document.
    pub paginating: Option<u64>,
    /// Collaboration is switched on for this document (the sidebar row's icon). UI only: the
    /// sync kit that would connect it is a TODO.
    pub collab: bool,
    /// The last write of this document failed; cleared by the next write that lands.
    pub save_failed: bool,
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
            generation: 1,
            pages: Pages::default(),
            paginating: None,
            collab: false,
            save_failed: false,
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

    /// The document changed: its pages are stale.
    pub fn touch(&mut self) {
        self.generation += 1;
    }

    /// The editor's new state (after typing, a command, an undo): kept,
    /// and the pagination starts over when the document changed.
    pub fn adopt(&mut self, state: RichTextEditorState) {
        let changed = state.doc != self.editor.doc;
        self.editor = state;
        if changed {
            self.touch();
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
#[path = "model_test.rs"]
mod model_test;
