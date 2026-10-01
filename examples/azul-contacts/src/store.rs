//! Where contacts live: one `.vcf` FILE per contact, `contacts/<uid>.vcf`
//! in the user's data folder - the key the user's S3 bucket will have.
//! The file is the contact's identity: a file whose card carries another
//! UID (or none) is the contact of the FILE NAME. Files are written as
//! vCard 4.0.
//!
//! Import reads any `.vcf` (3.0 or 4.0, one card or many) into a preview:
//! each card is new, an update of a contact with the same UID, or a
//! possible duplicate of one (dupes.rs); export writes the chosen contacts
//! as one `.vcf` in the version asked for.

use crate::contact::{parse_vcf, write_vcf, Contact};
use crate::dupes::{similarity, THRESHOLD};
use crate::vcard::Version;

/// The contacts' folder in the data root.
pub const APP_FOLDER: &str = "contacts";
/// A contact file's suffix.
pub const SUFFIX: &str = ".vcf";
/// The version contact files are written in.
pub const STORE_VERSION: Version = Version::V4;

/// Whether `uid` can be a file name on every system and a bucket key:
/// letters, digits, `-`, `_`, `.`, not starting with a dot, at most 100 long.
#[must_use]
pub fn is_safe_uid(uid: &str) -> bool {
    !uid.is_empty()
        && uid.len() <= 100
        && !uid.starts_with('.')
        && uid.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// `contacts/<uid>.vcf`.
#[must_use]
pub fn contact_key(uid: &str) -> String {
    format!("{APP_FOLDER}/{uid}{SUFFIX}")
}

/// The UID of a contact file's key (`contacts/<uid>.vcf`).
#[must_use]
pub fn uid_of_key(key: &str) -> Option<String> {
    let name = key.strip_prefix(APP_FOLDER)?.strip_prefix('/')?;
    if name.contains('/') {
        return None; // not directly in the folder
    }
    let uid = name.strip_suffix(SUFFIX)?;
    is_safe_uid(uid).then(|| uid.to_string())
}

/// Gives a contact a new UID if it has none that can name a file.
pub fn ensure_uid(c: &mut Contact) {
    if !is_safe_uid(&c.uid) {
        c.uid = azul_appkit::data::new_uuid();
    }
}

/// The file of a contact: its key and its bytes.
#[must_use]
pub fn file_of(c: &Contact) -> (String, Vec<u8>) {
    (contact_key(&c.uid), c.to_vcf(STORE_VERSION).into_bytes())
}

/// Reads the contact files (`(key, bytes)`, as the file jobs return them):
/// the contacts, and what could not be read.
#[must_use]
pub fn load(files: &[(String, Vec<u8>)]) -> (Vec<Contact>, Vec<String>) {
    let mut contacts = Vec::new();
    let mut problems = Vec::new();
    for (key, bytes) in files {
        let Some(uid) = uid_of_key(key) else {
            problems.push(format!("{key}: not a contact file name"));
            continue;
        };
        let text = String::from_utf8_lossy(bytes);
        let (mut cards, card_problems) = parse_vcf(&text);
        problems.extend(card_problems.into_iter().map(|p| format!("{key}: {p}")));
        if cards.is_empty() {
            problems.push(format!("{key}: no vCard in the file"));
            continue;
        }
        if cards.len() > 1 {
            problems.push(format!("{key}: {} cards, the first is used", cards.len()));
        }
        let mut c = cards.swap_remove(0);
        c.uid = uid;
        contacts.push(c);
    }
    (contacts, problems)
}

/// What an imported card is to the address book.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImportStatus {
    /// Nobody like it yet.
    New,
    /// A contact with the same UID exists (importing replaces it).
    Update(usize),
    /// Probably the same person as this contact (similarity).
    Duplicate(usize, f32),
}

/// One row of the import preview.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportRow {
    pub contact: Contact,
    pub status: ImportStatus,
    /// Imported when the user presses Import (new ones are, the rest not).
    pub selected: bool,
}

/// The import preview of a `.vcf` text against the address book.
#[must_use]
pub fn import_preview(text: &str, existing: &[Contact]) -> (Vec<ImportRow>, Vec<String>) {
    let (cards, problems) = parse_vcf(text);
    let mut rows: Vec<ImportRow> = Vec::new();
    for mut c in cards {
        let same_uid = (!c.uid.is_empty())
            .then(|| existing.iter().position(|e| e.uid == c.uid))
            .flatten();
        let status = match same_uid {
            Some(i) => ImportStatus::Update(i),
            None => {
                ensure_uid(&mut c);
                let best = existing
                    .iter()
                    .enumerate()
                    .map(|(i, e)| (i, similarity(&c, e).0))
                    .filter(|(_, s)| *s >= THRESHOLD)
                    .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                match best {
                    Some((i, s)) => ImportStatus::Duplicate(i, s),
                    None => ImportStatus::New,
                }
            }
        };
        // Two cards of one file with one UID: the second gets its own.
        if rows.iter().any(|r| r.contact.uid == c.uid) {
            c.uid = azul_appkit::data::new_uuid();
        }
        let selected = status == ImportStatus::New;
        rows.push(ImportRow {
            contact: c,
            status,
            selected,
        });
    }
    (rows, problems)
}

/// The import summary: `55 new · 3 duplicates · 1 update`.
#[must_use]
pub fn import_summary(rows: &[ImportRow]) -> String {
    let new = rows.iter().filter(|r| r.status == ImportStatus::New).count();
    let dup = rows.iter().filter(|r| matches!(r.status, ImportStatus::Duplicate(..))).count();
    let upd = rows.iter().filter(|r| matches!(r.status, ImportStatus::Update(_))).count();
    let mut parts = vec![format!("{new} new")];
    if dup > 0 {
        parts.push(format!("{dup} possible duplicate{}", if dup == 1 { "" } else { "s" }));
    }
    if upd > 0 {
        parts.push(format!("{upd} update{}", if upd == 1 { "" } else { "s" }));
    }
    parts.join(" \u{b7} ")
}

/// The chosen contacts as one `.vcf` text.
#[must_use]
pub fn export(contacts: &[Contact], indices: &[usize], version: Version) -> String {
    let chosen: Vec<Contact> = indices
        .iter()
        .filter_map(|&i| contacts.get(i).cloned())
        .collect();
    write_vcf(&chosen, version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contact::Labeled;

    fn person(uid: &str, given: &str, family: &str, email: &str) -> Contact {
        Contact {
            uid: uid.to_string(),
            given: given.to_string(),
            family: family.to_string(),
            emails: vec![Labeled::new("home", email)],
            ..Contact::default()
        }
    }

    #[test]
    fn a_contact_is_one_file_named_by_its_uid() {
        let c = person("0b8f8a4e-6f0e-4c39-9a51-6c0f1e2d3a4b", "Robin", "Weber", "robin@example.org");
        let (key, bytes) = file_of(&c);
        assert_eq!(key, "contacts/0b8f8a4e-6f0e-4c39-9a51-6c0f1e2d3a4b.vcf");
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with("BEGIN:VCARD\r\nVERSION:4.0\r\n"));
        assert_eq!(uid_of_key(&key).as_deref(), Some(c.uid.as_str()));
        assert_eq!(uid_of_key("contacts/notes.txt"), None);
        assert_eq!(uid_of_key("contacts/../x.vcf"), None);
    }

    #[test]
    fn loading_reads_every_file_and_the_file_name_wins() {
        let a = person("aaaa", "Anna", "Berg", "anna@example.org");
        let b = person("bbbb", "Ben", "Krüger", "ben@example.org");
        let (ka, va) = file_of(&a);
        let mut files = vec![(ka, va), ("contacts/cccc.vcf".to_string(), file_of(&b).1)];
        files.push(("contacts/broken.vcf".to_string(), b"hello".to_vec()));
        let (contacts, problems) = load(&files);
        assert_eq!(contacts.len(), 2);
        assert_eq!(contacts[0], a);
        assert_eq!(contacts[1].uid, "cccc", "the file's name is the contact's identity");
        assert_eq!(contacts[1].given, "Ben");
        assert_eq!(problems, vec!["contacts/broken.vcf: no vCard in the file"]);
    }

    #[test]
    fn a_missing_or_unsafe_uid_gets_a_new_one() {
        let mut c = person("", "A", "B", "a@example.org");
        ensure_uid(&mut c);
        assert!(azul_appkit::data::is_uuid(&c.uid));
        let mut d = person("../../etc/passwd", "A", "B", "a@example.org");
        ensure_uid(&mut d);
        assert!(is_safe_uid(&d.uid));
        let mut e = person("keep-me_1.2", "A", "B", "a@example.org");
        ensure_uid(&mut e);
        assert_eq!(e.uid, "keep-me_1.2");
    }

    #[test]
    fn the_import_preview_marks_new_updates_and_duplicates() {
        let book = vec![person("aaaa", "Anna", "Berg", "anna@example.org")];
        let text = [
            person("", "Mara", "Schulz", "mara@example.org").to_vcf(Version::V3),
            person("aaaa", "Anna", "Berg", "anna@example.org").to_vcf(Version::V3),
            person("zzzz", "Anna", "Berg", "anna.berg@example.net").to_vcf(Version::V4),
        ]
        .concat();
        let (rows, problems) = import_preview(&text, &book);
        assert!(problems.is_empty());
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].status, ImportStatus::New);
        assert!(rows[0].selected && is_safe_uid(&rows[0].contact.uid));
        assert_eq!(rows[1].status, ImportStatus::Update(0));
        assert!(!rows[1].selected);
        assert!(matches!(rows[2].status, ImportStatus::Duplicate(0, s) if (s - 0.93).abs() < 1e-6));
        assert!(!rows[2].selected);
        assert_eq!(import_summary(&rows), "1 new \u{b7} 1 possible duplicate \u{b7} 1 update");
    }

    #[test]
    fn export_writes_the_chosen_contacts_in_the_version_asked_for() {
        let book = vec![
            person("aaaa", "Anna", "Berg", "anna@example.org"),
            person("bbbb", "Ben", "Krüger", "ben@example.org"),
        ];
        let text = export(&book, &[1], Version::V3);
        assert_eq!(text.matches("BEGIN:VCARD").count(), 1);
        assert!(text.contains("VERSION:3.0") && text.contains("Krüger"));
        let both = export(&book, &[0, 1], Version::V4);
        let (back, _) = parse_vcf(&both);
        assert_eq!(back, book);
    }
}
