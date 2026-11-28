//! The open documents of a document app as azul's sidebar rows (feature `azul`): the rows of
//! [`crate::docs`] with their sync pie and collaboration icon, the "New or open" row, the section
//! that holds them, the status slot's pie for the shown document, and the decoding of what the
//! sidebar reports about them. AzWriter, AzSheets and AzShow call these and nothing else about
//! their documents list, so the three behave the same.

use azul::widgets::{
    PieTone, SidebarCollab, SidebarEvent, SidebarEventKind, SidebarRow, SidebarSection,
    SidebarStatus,
};

use crate::{
    docs::{self, DocRow, DocSync, Tone},
    l10n::{label, t},
};

/// azul's pie tone for a [`Tone`].
#[must_use]
pub const fn pie_tone(tone: Tone) -> PieTone {
    match tone {
        Tone::Accent => PieTone::Accent,
        Tone::Busy => PieTone::Busy,
        Tone::Good => PieTone::Good,
        Tone::Warning => PieTone::Warning,
        Tone::Danger => PieTone::Danger,
    }
}

/// The sidebar's rows of the open documents, in the order given: the document's icon (`icon`, a
/// Material name: `description`, `table_chart`, `slideshow`), its name (`untitled` for a blank
/// one), its sync pie and its collaboration icon; the shown one is selected. Every row is a
/// button of the collapsed rail (the pie sits at the corner of its icon there).
#[must_use]
pub fn doc_rows(rows: &[DocRow], icon: &str, untitled: &str) -> Vec<SidebarRow> {
    rows.iter()
        .take(docs::MAX_OPEN_DOCS)
        .enumerate()
        .map(|(index, row)| {
            let name = row.shown_name(untitled);
            SidebarRow::create(name.clone())
                .with_icon(icon)
                .with_tag(docs::doc_tag(index))
                .with_selected(row.selected)
                .with_pie(row.sync.fraction(), pie_tone(row.sync.tone()))
                .with_collab(if row.collab { SidebarCollab::On } else { SidebarCollab::Off })
                .with_tooltip(format!("{name}, {}", t(row.sync.key())))
        })
        .collect()
}

/// The "New or open" row that ends the list of open documents.
#[must_use]
pub fn new_row() -> SidebarRow {
    SidebarRow::create(label("kit-doc-new"))
        .with_icon("add")
        .with_tag(docs::NEW_TAG)
}

/// The section "Open documents": the rows of `rows`, then the "New or open" row.
#[must_use]
pub fn documents_section(rows: &[DocRow], icon: &str, untitled: &str) -> SidebarSection {
    let mut rows = doc_rows(rows, icon, untitled);
    rows.push(new_row());
    SidebarSection::create(label("kit-doc-section")).with_rows(rows)
}

/// The status slot's pie for the shown document: how far its save is, the state as the short
/// text beside it, and the sentence that says what the state means as the popover's detail.
#[must_use]
pub fn sync_status(sync: DocSync) -> SidebarStatus {
    SidebarStatus::create(label(sync.key()), sync.fraction(), pie_tone(sync.tone()))
        .with_text(t(sync.key()))
        .with_detail(t(sync.detail_key()))
}

/// What the sidebar reported about the open documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocEvent {
    /// The open document at this index was chosen: it takes the screen.
    Switch(usize),
    /// "New or open" was chosen.
    New,
    /// The collaboration icon of this document was pressed; `on` is the state asked for.
    Collab { index: usize, on: bool },
    /// The sync pie of this document was pressed: the app says what it means.
    Status(usize),
    /// This document's row was pressed and held (a phone's long press: its quick actions).
    Menu(usize),
}

/// What a report of the sidebar means for the open documents; `None` for everything else (the
/// toggle, a row of the app's own, a quick-access button: the app handles those).
#[must_use]
pub fn doc_event(event: &SidebarEvent) -> Option<DocEvent> {
    let index = docs::doc_of_tag(event.tag);
    match event.kind {
        SidebarEventKind::RowClicked if event.tag == docs::NEW_TAG => Some(DocEvent::New),
        SidebarEventKind::RowClicked => index.map(DocEvent::Switch),
        SidebarEventKind::RowCollabToggled => index.map(|index| DocEvent::Collab { index, on: event.expand }),
        SidebarEventKind::RowStatusClicked => index.map(DocEvent::Status),
        SidebarEventKind::RowLongPressed => index.map(DocEvent::Menu),
        _ => None,
    }
}
