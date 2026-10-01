//! AzContacts: the address book of the Azlin apps (the plan:
//! azul-apps/planning/core/contacts.md).
//!
//! The model is plain Rust, tested without a window:
//! - [`vcard`]: vCard 3.0 / 4.0 content lines - folding, escaping, parameters, multiple values;
//! - [`contact`]: the contact and its vCard 3.0 / 4.0 form;
//! - [`book`]: the list - sort by first or last name (diacritics folded), letter sections, the A-Z jump, initials, search, groups.

pub mod book;
pub mod contact;
pub mod vcard;

/// The window (filled in by the UI commit).
pub fn start() {}
