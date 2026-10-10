//! AzContacts: the address book of the Azlin apps (the plan:
//! azul-apps/planning/core/contacts.md).
//!
//! The model is plain Rust, tested without a window, in azul-contacts-core (so the Azlin Bridge's
//! CardDAV runs the same code); it keeps its module names here (`crate::contact`):
//! - [`vcard`]: vCard 3.0 / 4.0 content lines - folding, escaping, parameters, multiple values;
//! - [`contact`]: the contact and its vCard 3.0 / 4.0 form;
//! - [`book`]: the list - sort by first or last name (diacritics folded), letter sections, the A-Z jump, initials, search, groups;
//! - [`dupes`]: possible duplicates (same email, phone or name) and merging two contacts;
//! - [`store`]: one contacts/<uid>.vcf file per contact, loading, the import preview, export;
//! - [`sample`]: the --sample address book (300 contacts, the plan's special cards and duplicate pairs).

pub use azcontacts_core::{book, contact, csv, dupes, sample, store, vcard};

pub mod ids;
pub mod photo;

/// The window (azul's PimShell, the list, the card, the edit form, import, merge, settings).
pub mod ui;

/// Starts AzContacts (the switches are read from the command line).
pub fn start() {
    ui::start();
}
