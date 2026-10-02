//! The shared rich-text model behind [`RichTextEditor`]: one document type
//! for AzNotes, AzMail's compose window and AzWriter (scripts/DEDUP_EDITORS
//! A2), its edits, its serializers and its undo history.
//!
//! - [`doc`]: [`RichTextDoc`] - a flat list of [`RichBlock`]s, each a list of
//!   [`RichRun`]s - and every edit the editor folds into it (typing, Enter's
//!   split, Backspace's merge, a paste across blocks, formats, block kinds,
//!   indents, links, check items, Markdown shortcuts).
//! - [`markdown`]: the document as Markdown, both ways (AzNotes' canonical
//!   writer; the reader needs the `rich_text_markdown` feature).
//! - [`html`]: the document as HTML (a mail's `text/html` part, a paste) and
//!   as plain text (a mail's `text/plain` part), and HTML back into blocks.
//! - [`history`]: the editor's ONE undo / redo history.
//!
//! [`RichTextEditor`]: crate::widgets::rich_text_editor::RichTextEditor

pub mod doc;
pub mod history;
pub mod html;
pub mod markdown;

pub use self::{
    doc::{
        RichAlign, RichBlock, RichBlockKind, RichBlockVec, RichCheck, RichFormat, RichFormats,
        RichImage, RichRun, RichRunVec, RichTable, RichTableRow, RichTableRowVec, RichTextDoc,
        RichTextDocVec,
    },
    history::RichTextHistory,
};
