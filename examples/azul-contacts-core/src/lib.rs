//! azul-contacts-core: AzContacts' address book without azul types.
//!
//! AzContacts (examples/azul-contacts) links libazul for its window; these modules run without
//! it, so a headless process - the Azlin Bridge's CardDAV - reads and writes the same files with
//! the same code (and serves a vCard 4.0 file to a program that wants 3.0). AzContacts
//! re-exports them under the names it always had (`crate::contact`).
//!
//! - [`vcard`]: vCard 3.0 / 4.0 content lines - folding, escaping, parameters, multiple values;
//! - [`contact`]: the contact and its vCard 3.0 / 4.0 form;
//! - [`book`]: the list - sort by first or last name (diacritics folded), letter sections, the
//!   A-Z jump, initials, search, groups;
//! - [`dupes`]: possible duplicates (same email, phone or name) and merging two contacts;
//! - [`csv`]: a CSV file's columns mapped onto the contact's fields;
//! - [`store`]: one contacts/<uid>.vcf file per contact, loading, the import preview, export;
//! - [`sample`]: the --sample address book (300 contacts, the plan's special cards and duplicate
//!   pairs).

pub mod book;
pub mod contact;
pub mod csv;
pub mod dupes;
pub mod sample;
pub mod store;
pub mod vcard;
