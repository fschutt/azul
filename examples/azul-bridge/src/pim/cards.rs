//! CardDAV's contacts: AzContacts' files, one `contacts/<uid>.vcf` per contact in the drive,
//! read with AzContacts' own code (azul-contacts-core). A program's card is kept byte for byte
//! (vCard 3.0 or 4.0: AzContacts reads both and takes the file name as the contact's UID), so a
//! PUT's answer carries the new ETag. A card is served in vCard 3.0 - what every CardDAV program
//! reads, Apple's Contacts above all - unless the program asks for 4.0: a 4.0 file (AzContacts
//! writes 4.0) is then written in 3.0 by AzContacts' contact model ([`card_as`]).

use std::collections::HashSet;

use azul_storage::{ops, DriveError, ObjectInfo};

use super::{dav_error, precondition, version_of, Item, Kind, Pim};
use crate::{
    dates,
    http::{Head, Response, Status},
};

// AzContacts' file rules, its own code: the folder (`contacts`), whether a UID can name a file,
// `contacts/<uid>.vcf` and back.
pub use azcontacts_core::store::{
    contact_key as card_key, is_safe_uid as is_contact_uid, uid_of_key, APP_FOLDER as CONTACTS_DIR,
};
pub use azcontacts_core::vcard::Version;
use azcontacts_core::{contact, vcard};

/// What a card is served as.
pub const CONTENT_TYPE: &str = "text/vcard; charset=utf-8";

/// The vCard version a GET asks for: 4.0 when its `Accept` says `version=4.0`, else 3.0.
#[must_use]
pub fn wanted_of_accept(accept: Option<&str>) -> Version {
    match accept {
        Some(value) if value.to_ascii_lowercase().replace(' ', "").contains("version=4.0") => Version::V4,
        _ => Version::V3,
    }
}

/// A card's bytes in the version `wanted`: the file as it is when it is that version already;
/// else the contact - AzContacts' model of it - written in that version, with the file's UID. A
/// file that holds no card is served as it is.
#[must_use]
pub fn card_as(bytes: &[u8], uid: &str, wanted: Version) -> Vec<u8> {
    let text = String::from_utf8_lossy(bytes);
    let (cards, _) = vcard::parse(&text);
    if cards.first().is_none_or(|card| card.version == wanted) {
        return bytes.to_vec();
    }
    let (contacts, _) = contact::parse_vcf(&text);
    let Some(mut contact) = contacts.into_iter().next() else {
        return bytes.to_vec();
    };
    contact.uid = uid.to_string();
    contact.to_vcf(wanted).into_bytes()
}

/// Whether `text` is a vCard: it has a `BEGIN:VCARD` line.
fn is_vcard(text: &str) -> bool {
    text.lines()
        .any(|line| line.trim().eq_ignore_ascii_case("BEGIN:VCARD"))
}

/// A contact file and the name its href has.
#[derive(Debug, Clone)]
pub(crate) struct CardFile {
    pub name: String,
    pub uid: String,
    pub info: ObjectInfo,
}

impl Pim {
    /// Every contact file, under the names the programs gave them (else their UIDs).
    pub(crate) fn cards(&self) -> Result<Vec<CardFile>, DriveError> {
        let prefix = format!("{CONTACTS_DIR}/");
        let mut out = Vec::new();
        for info in ops::list_all(&*self.contacts, &prefix)? {
            let Some(uid) = uid_of_key(&info.key) else {
                continue;
            };
            let name = self
                .names
                .name_of(Kind::Contact, &uid)
                .unwrap_or_else(|| uid.clone());
            out.push(CardFile { name, uid, info });
        }
        let present: HashSet<String> = out.iter().map(|card| card.uid.clone()).collect();
        self.names.keep_only(Kind::Contact, &present);
        Ok(out)
    }

    /// The address book, with its CTag.
    pub(crate) fn book_item(&self) -> Result<Item, DriveError> {
        let mut text = String::new();
        for card in self.cards()? {
            text.push_str(&format!("{}\n{}\n", card.name, version_of(&card.info)));
        }
        Ok(Item::Book {
            ctag: super::ctag_of(&text),
        })
    }

    /// The UID the name `name` stands for: the one kept for it, else the name itself when it can
    /// name a file.
    fn card_uid(&self, name: &str) -> Option<String> {
        self.names
            .id_of(Kind::Contact, name)
            .or_else(|| is_contact_uid(name).then(|| name.to_string()))
    }

    /// The contact `name` names, if it is there: its UID and its file.
    pub(crate) fn card(&self, name: &str) -> Result<Option<(String, ObjectInfo)>, DriveError> {
        let Some(uid) = self.card_uid(name) else {
            return Ok(None);
        };
        match self.contacts.head(&card_key(&uid)) {
            Ok(info) => Ok(Some((uid, info))),
            Err(DriveError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The card `name` in the version the program's `Accept` asks for (3.0 unless it says 4.0).
    pub(crate) fn get_card(&self, name: &str, accept: Option<&str>) -> Result<Response, DriveError> {
        let Some((uid, info)) = self.card(name)? else {
            return Ok(Response::text(Status::NOT_FOUND, "Not there."));
        };
        let bytes = card_as(&self.contacts.get(&info.key)?, &uid, wanted_of_accept(accept));
        let mut response = Response::new(Status::OK)
            .with_header("ETag", format!("\"{}\"", version_of(&info)))
            .with_body(CONTENT_TYPE, bytes);
        if let Some(modified) = info.modified {
            response = response.with_header("Last-Modified", dates::http_date(i64::try_from(modified).unwrap_or(0)));
        }
        Ok(response)
    }

    pub(crate) fn put_card(&self, head: &Head, name: &str, body: &[u8]) -> Result<Response, DriveError> {
        if !std::str::from_utf8(body).is_ok_and(is_vcard) {
            return Ok(dav_error(Status::FORBIDDEN, "<CR:valid-address-data/>"));
        }
        // A name that cannot be a file's gets a UID of its own, kept for the name.
        let uid = self
            .card_uid(name)
            .unwrap_or_else(azul_storage::ids::new_uuid);
        let key = card_key(&uid);
        let current = match self.contacts.head(&key) {
            Ok(info) => Some(info),
            Err(DriveError::NotFound { .. }) => None,
            Err(e) => return Err(e),
        };
        if let Some(refusal) = precondition(head, current.as_ref().map(version_of).as_deref()) {
            return Ok(refusal);
        }
        self.contacts.put(&key, body)?;
        if uid != name {
            self.names.set(Kind::Contact, name, &uid);
        }
        let mut response = Response::new(if current.is_some() { Status::NO_CONTENT } else { Status::CREATED });
        // Kept as it came: the version is what the program has.
        if let Ok(info) = self.contacts.head(&key) {
            response = response.with_header("ETag", format!("\"{}\"", version_of(&info)));
        }
        Ok(response)
    }

    pub(crate) fn delete_card(&self, head: &Head, name: &str) -> Result<Response, DriveError> {
        let Some((_, info)) = self.card(name)? else {
            return Ok(Response::text(Status::NOT_FOUND, "Not there."));
        };
        if let Some(refusal) = precondition(head, Some(version_of(&info).as_str())) {
            return Ok(refusal);
        }
        self.contacts.delete(&info.key)?;
        self.names.forget(Kind::Contact, name);
        Ok(Response::new(Status::NO_CONTENT))
    }
}
