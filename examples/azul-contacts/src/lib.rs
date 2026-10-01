//! AzContacts: the address book of the Azlin apps (the plan:
//! azul-apps/planning/core/contacts.md).
//!
//! The model is plain Rust, tested without a window:
//! - [`vcard`]: vCard 3.0 / 4.0 content lines - folding, escaping, parameters, multiple values;
//! - [`contact`]: the contact and its vCard 3.0 / 4.0 form;
//! - [`book`]: the list - sort by first or last name (diacritics folded), letter sections, the A-Z jump, initials, search, groups;
//! - [`dupes`]: possible duplicates (same email, phone or name) and merging two contacts;
//! - [`store`]: one contacts/<uid>.vcf file per contact, loading, the import preview, export;
//! - [`sample`]: the --sample address book (300 contacts, the plan's special cards and duplicate pairs).

pub mod book;
pub mod contact;
pub mod dupes;
pub mod sample;
pub mod store;
pub mod vcard;

/// The window (filled in by the UI commit).
pub fn start() {}
