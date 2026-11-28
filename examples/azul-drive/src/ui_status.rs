//! What the command line says at rest, in place of a status bar: how the place stands - the
//! items it holds (or the drives, or the pinned folders), the selection and its size, what waits
//! on the clipboard, the transfer running, or - while a search is open - how far the search got.
//! The facts are plain data ([`Facts`]) so that the line's words can be read without a window;
//! [`facts_of`] reads them off the state.

use azul_appkit::l10n::{grouped, t_text, Phrase, Text};

use crate::{browse::{self, Place}, listing, DriveState};

/// Where the window is, as far as the status line cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Where {
    Folder,
    ThisPc,
    QuickAccess,
}

/// What the status line is made of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Facts {
    pub place: Where,
    /// Nothing of the folder has arrived yet.
    pub loading: bool,
    /// The scan of the folder handed over its last batch.
    pub done: bool,
    /// The items the view shows.
    pub shown: usize,
    pub selected: usize,
    /// The bytes of the selected files, when they are known.
    pub selected_bytes: Option<u64>,
    pub drives: usize,
    pub pins: usize,
    /// How far an open search got, in the window's words.
    pub search: Option<String>,
    /// Items on the clipboard.
    pub clipboard: usize,
    /// The running transfer's line, in the window's words; empty for none.
    pub transfers: String,
}

/// The parts of the line, in their order.
#[must_use]
pub(crate) fn parts(f: &Facts) -> Vec<Text> {
    let mut parts: Vec<Text> = Vec::new();
    match (f.place, &f.search) {
        (_, Some(search)) => parts.push(Text::plain(search.clone())),
        (Where::ThisPc, None) => parts.push(
            Phrase::new("azdrive-status-drives")
                .arg("count", f.drives)
                .into(),
        ),
        (Where::QuickAccess, None) => parts.push(
            Phrase::new("azdrive-status-pins")
                .arg("count", f.pins)
                .into(),
        ),
        (Where::Folder, None) if f.loading => parts.push(Text::key("azdrive-status-loading")),
        (Where::Folder, None) => parts.push(listing::count_text(f.shown, f.done).into()),
    }
    if f.selected > 0 {
        parts.push(
            Phrase::new("azdrive-status-selected-of")
                .arg("n", grouped(f.selected as u64))
                .arg("shown", grouped(f.shown as u64))
                .into(),
        );
        if let Some(bytes) = f.selected_bytes.filter(|b| *b > 0) {
            parts.push(Text::plain(browse::format_size(Some(bytes))));
        }
    }
    if f.clipboard > 0 {
        parts.push(
            Phrase::new("azdrive-status-clipboard")
                .arg("count", f.clipboard)
                .into(),
        );
    }
    if !f.transfers.is_empty() {
        parts.push(Text::plain(f.transfers.clone()));
    }
    parts
}

/// The line as one string: the parts a dot apart.
#[must_use]
pub(crate) fn line_of(f: &Facts) -> String {
    parts(f)
        .iter()
        .map(t_text)
        .collect::<Vec<_>>()
        .join("  \u{00B7}  ")
}

/// The facts of the window now.
#[must_use]
pub(crate) fn facts_of(s: &DriveState) -> Facts {
    let selected = s.selected_entries();
    let sizes: Vec<u64> = selected
        .iter()
        .filter(|e| !e.is_folder)
        .filter_map(|e| e.size)
        .collect();
    Facts {
        place: match s.place {
            Place::ThisPc => Where::ThisPc,
            Place::QuickAccess => Where::QuickAccess,
            Place::Folder { .. } => Where::Folder,
        },
        loading: s.loading,
        done: s.listing_done,
        shown: s.visible_entries().len(),
        selected: selected.len(),
        selected_bytes: (!sizes.is_empty()).then(|| sizes.iter().sum()),
        drives: s.slots.len(),
        pins: s.settings.pinned.len(),
        search: s.find.as_ref().map(|find| t_text(&find.status_text().into())),
        clipboard: s.clipboard.as_ref().map_or(0, |c| c.items.len()),
        transfers: t_text(&s.queue.status_text()),
    }
}

/// The line of the window now.
#[must_use]
pub(crate) fn status_line(s: &DriveState) -> String {
    line_of(&facts_of(s))
}

#[cfg(test)]
#[path = "ui_status_test.rs"]
mod ui_status_test;
