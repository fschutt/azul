//! Where AzWriter's files live in the data tree (user ruling, the S3
//! split): `writer/<uuid>.md` per document, `writer/exports/<title>.pdf`
//! (and `.md`) for exports. Every read and write is an azul-appkit file job
//! on an azul `Thread` against the data root's azul-storage drive (a
//! `LocalDrive` today, the user's bucket later) - never from a callback.

use azul_appkit::FileJob;

/// AzWriter's folder in the data root.
pub const APP_FOLDER: &str = "writer";

/// The write-back tags of AzWriter's file jobs.
pub mod tag {
    /// Every document's file (the backstage's list).
    pub const LIST: u64 = 1;
    /// One document's file, to open it.
    pub const OPEN: u64 = 2;
    /// The open document saved.
    pub const SAVE: u64 = 3;
    /// An export written.
    pub const EXPORT: u64 = 4;
    /// A document's file deleted.
    pub const DELETE: u64 = 5;
    /// The open document saved before the window closes.
    pub const SAVE_AND_CLOSE: u64 = 6;
}

/// The key of document `id`: `writer/<id>.md`.
#[must_use]
pub fn doc_key(id: &str) -> String {
    azul_appkit::data::app_key(APP_FOLDER, &format!("{id}.md"))
}

/// The document id a key names (`writer/<id>.md`, not an export).
#[must_use]
pub fn id_of_key(key: &str) -> Option<&str> {
    let name = key.strip_prefix(APP_FOLDER)?.strip_prefix('/')?;
    let id = name.strip_suffix(".md")?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}

/// `title` as a file name: letters, digits, blanks, `-` and `_` kept, the
/// rest dropped; "Document" when nothing is left.
#[must_use]
pub fn safe_file_name(title: &str) -> String {
    let kept: String = title
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect();
    let kept = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    if kept.is_empty() {
        "Document".to_string()
    } else {
        kept.chars().take(80).collect()
    }
}

/// The key of an export of `title` with `extension` (`pdf`, `md`):
/// `writer/exports/<title>.<extension>`.
#[must_use]
pub fn export_key(title: &str, extension: &str) -> String {
    azul_appkit::data::app_key(
        APP_FOLDER,
        &format!("exports/{}.{extension}", safe_file_name(title)),
    )
}

/// One document in the data tree, for the backstage's list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocEntry {
    pub id: String,
    pub title: String,
    pub words: usize,
}

/// The documents among a folder's files (`(key, bytes)`), by title.
#[must_use]
pub fn entries_from(files: &[(String, Vec<u8>)]) -> Vec<DocEntry> {
    let mut out: Vec<DocEntry> = files
        .iter()
        .filter_map(|(key, bytes)| {
            let id = id_of_key(key)?;
            let markdown = String::from_utf8_lossy(bytes);
            let doc = azul::widgets::RichTextDoc::create_from_markdown(markdown.as_ref());
            Some(DocEntry {
                id: id.to_string(),
                title: crate::model::title_of(&doc),
                words: doc.word_count(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()).then(a.id.cmp(&b.id)));
    out
}

/// The job that lists every document.
#[must_use]
pub fn list_job() -> FileJob {
    FileJob::GetAll {
        prefix: azul_appkit::data::app_prefix(APP_FOLDER),
        suffix: ".md".to_string(),
    }
}

/// The job that reads document `id`.
#[must_use]
pub fn open_job(id: &str) -> FileJob {
    FileJob::Get { key: doc_key(id) }
}

/// The job that writes `markdown` as document `id`.
#[must_use]
pub fn save_job(id: &str, markdown: &str) -> FileJob {
    FileJob::Put {
        key: doc_key(id),
        bytes: markdown.as_bytes().to_vec(),
    }
}

/// The job that removes document `id`.
#[must_use]
pub fn delete_job(id: &str) -> FileJob {
    FileJob::Delete { key: doc_key(id) }
}

/// The sample document of `--sample` (an empty data folder only).
pub const SAMPLE: &str = "# Welcome to AzWriter\n\nAzWriter keeps your documents as **Markdown files** in your data folder, one per document, and lays them out on *A4 pages* as you type.\n\n## What you can do\n\n- Format text: **bold**, *italic*, <u>underline</u>, ~~strike~~ and `code`\n- Headings, bulleted and numbered lists, check items, quotes, code blocks\n- Tables, horizontal rules and page breaks\n- Import Word and Markdown files, export PDF\n\n1. Write\n2. Save (Ctrl/Cmd+S)\n3. Export as PDF from the File tab\n\n- [x] Open AzWriter\n- [ ] Write the first page\n\n> Every change is one step of one undo history: Ctrl/Cmd+Z takes it back.\n\n| Feature | Shortcut |\n| --- | --- |\n| Bold | Ctrl/Cmd+B |\n| Undo | Ctrl/Cmd+Z |\n\n---\n\nThe end of the first page.\n\n<!-- pagebreak -->\n\n# A second page\n\nA page break starts a new sheet.\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_is_a_markdown_file_named_by_its_id() {
        assert_eq!(doc_key("abc"), "writer/abc.md");
        assert_eq!(id_of_key("writer/abc.md"), Some("abc"));
        assert_eq!(id_of_key("writer/exports/Report.md"), None, "an export is no document");
        assert_eq!(id_of_key("writer/settings.json"), None);
        assert_eq!(id_of_key("notes/abc.md"), None);
    }

    #[test]
    fn an_export_is_named_after_the_title_in_the_exports_folder() {
        assert_eq!(export_key("Quarterly report", "pdf"), "writer/exports/Quarterly report.pdf");
        assert_eq!(export_key("a/b: c?", "md"), "writer/exports/ab c.md");
        assert_eq!(export_key("???", "pdf"), "writer/exports/Document.pdf");
    }

    #[test]
    fn the_list_names_documents_by_title_and_skips_other_files() {
        let files = vec![
            ("writer/b.md".to_string(), b"# Zebra\n\ntext".to_vec()),
            ("writer/a.md".to_string(), b"Apple pie\n".to_vec()),
            ("writer/exports/x.md".to_string(), b"# Export\n".to_vec()),
        ];
        let entries = entries_from(&files);
        let titles: Vec<&str> = entries.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, vec!["Apple pie", "Zebra"]);
        assert_eq!(entries[1].id, "b");
    }

    #[test]
    fn the_sample_document_has_a_title_and_two_pages_of_blocks() {
        let doc = azul::widgets::RichTextDoc::create_from_markdown(SAMPLE);
        assert_eq!(crate::model::title_of(&doc), "Welcome to AzWriter");
        assert!(doc
            .blocks
            .as_slice()
            .iter()
            .any(|b| b.kind == azul::widgets::RichBlockKind::PageBreak));
    }
}
