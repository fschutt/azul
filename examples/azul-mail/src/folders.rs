//! Which local folder each server mailbox is synced to.
//!
//! Every selectable mailbox the server lists becomes one folder, `mail/<key>/`, where the key is
//! ONE path segment (so a folder's `<yyyy>/<mm>` subfolders can never be mistaken for a child
//! folder). Mailboxes with a special use get a fixed key: the inbox is `inbox`; `\Junk` - or,
//! when the server marks none, a folder called Spam, Junk, Junk E-mail or Bulk Mail - is `spam`;
//! `\Sent`, `\Drafts`, `\Trash`, `\Archive`, `\All` and `\Flagged` are `sent`, `drafts`,
//! `trash`, `archive`, `all` and `flagged`. Any other mailbox is its decoded name with the
//! hierarchy delimiter shown as `.` (`Work/Projects` is `Work.Projects`), with the characters a
//! file system or an object key could trip on replaced; a key that another folder already has
//! (case-insensitively, as macOS compares names) gets `-2`, `-3`, ...

use crate::mutf7;

/// A mailbox as the server lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMailbox {
    /// The server's name, in modified UTF-7, exactly as listed: what `SELECT` needs.
    pub name: String,
    /// The hierarchy delimiter, if the server has one.
    pub delimiter: Option<String>,
    /// The LIST attributes (`\Noselect`, `\Junk`, `\HasChildren`, ...), as listed.
    pub attributes: Vec<String>,
}

/// What a folder is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Inbox,
    Sent,
    Drafts,
    Archive,
    Spam,
    Trash,
    All,
    Flagged,
    Other,
}

impl Role {
    /// The fixed key of a special folder; `None` for `Other`.
    pub fn key(self) -> Option<&'static str> {
        todo!()
    }

    /// The folder's name in the sidebar.
    pub fn label(self) -> Option<&'static str> {
        todo!()
    }

    /// The role of a folder synced under `key` (what the sidebar sorts by).
    pub fn of_key(key: &str) -> Role {
        todo!()
    }
}

/// A server mailbox and the local folder it is synced to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalMailbox {
    /// The server's name, for `SELECT`.
    pub server_name: String,
    /// The folder: `mail/<key>/`.
    pub key: String,
    /// The folder's name in the sidebar: the decoded name, the hierarchy shown with `/`.
    pub display: String,
    pub role: Role,
}

/// One path segment made safe for a file system and an object key: `/ \ : * ? " < > |` and
/// control characters become `_`, surrounding spaces and trailing dots go, a leading dot becomes
/// `_`, nothing becomes `_`, and it is cut to 120 bytes.
pub fn safe_segment(name: &str) -> String {
    todo!()
}

/// The local folders for a server's mailboxes, inbox first, then the other special folders,
/// then the rest by name. Mailboxes that cannot be selected are left out.
pub fn local_mailboxes(listed: &[ServerMailbox]) -> Vec<LocalMailbox> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mailbox(name: &str, attributes: &[&str]) -> ServerMailbox {
        ServerMailbox {
            name: name.to_string(),
            delimiter: Some(String::from("/")),
            attributes: attributes.iter().map(|a| a.to_string()).collect(),
        }
    }

    fn keys(listed: &[ServerMailbox]) -> Vec<(String, String)> {
        local_mailboxes(listed)
            .into_iter()
            .map(|m| (m.server_name, m.key))
            .collect()
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn gmail_special_use_folders_get_fixed_keys_and_spam_is_spam() {
        let listed = [
            mailbox("[Gmail]", &["\\HasChildren", "\\Noselect"]),
            mailbox("[Gmail]/All Mail", &["\\All", "\\HasNoChildren"]),
            mailbox("[Gmail]/Drafts", &["\\Drafts", "\\HasNoChildren"]),
            mailbox("[Gmail]/Sent Mail", &["\\HasNoChildren", "\\Sent"]),
            mailbox("[Gmail]/Spam", &["\\HasNoChildren", "\\Junk"]),
            mailbox("[Gmail]/Starred", &["\\Flagged", "\\HasNoChildren"]),
            mailbox("[Gmail]/Trash", &["\\HasNoChildren", "\\Trash"]),
            mailbox("INBOX", &["\\HasNoChildren"]),
            mailbox("Receipts", &["\\HasNoChildren"]),
        ];
        assert_eq!(
            keys(&listed),
            pairs(&[
                ("INBOX", "inbox"),
                ("[Gmail]/Sent Mail", "sent"),
                ("[Gmail]/Drafts", "drafts"),
                ("[Gmail]/Spam", "spam"),
                ("[Gmail]/Trash", "trash"),
                ("[Gmail]/All Mail", "all"),
                ("[Gmail]/Starred", "flagged"),
                ("Receipts", "Receipts"),
            ])
        );
        let spam = local_mailboxes(&listed)
            .into_iter()
            .find(|m| m.key == "spam")
            .unwrap();
        assert_eq!(spam.role, Role::Spam);
        assert_eq!(spam.display, "Spam");
    }

    #[test]
    fn without_a_junk_attribute_a_folder_called_junk_is_spam() {
        for name in ["Junk", "Spam", "Junk E-mail", "Bulk Mail", "INBOX/Junk"] {
            let listed = [mailbox("INBOX", &[]), mailbox(name, &[])];
            assert_eq!(
                keys(&listed),
                pairs(&[("INBOX", "inbox"), (name, "spam")]),
                "{name}"
            );
        }
    }

    #[test]
    fn a_junk_attribute_wins_over_a_folder_merely_called_spam() {
        let listed = [
            mailbox("INBOX", &[]),
            mailbox("Spam", &[]),
            mailbox("Unwanted", &["\\Junk"]),
        ];
        assert_eq!(
            keys(&listed),
            pairs(&[("INBOX", "inbox"), ("Unwanted", "spam"), ("Spam", "Spam-2")])
        );
    }

    #[test]
    fn the_inbox_is_found_in_any_case() {
        assert_eq!(keys(&[mailbox("Inbox", &[])]), pairs(&[("Inbox", "inbox")]));
    }

    #[test]
    fn nested_and_non_ascii_names_become_one_decoded_segment() {
        let listed = [
            mailbox("Work", &["\\HasChildren"]),
            mailbox("Work/Projects", &[]),
            mailbox("Entw&APw-rfe", &[]),
        ];
        let local = local_mailboxes(&listed);
        let got: Vec<(&str, &str, &str)> = local
            .iter()
            .map(|m| (m.server_name.as_str(), m.key.as_str(), m.display.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("Entw&APw-rfe", "Entwürfe", "Entwürfe"),
                ("Work", "Work", "Work"),
                ("Work/Projects", "Work.Projects", "Work/Projects"),
            ]
        );
        let dotted = [ServerMailbox {
            name: String::from("INBOX.Receipts"),
            delimiter: Some(String::from(".")),
            attributes: Vec::new(),
        }];
        assert_eq!(
            keys(&dotted),
            pairs(&[("INBOX.Receipts", "INBOX.Receipts")])
        );
    }

    #[test]
    fn mailboxes_that_cannot_be_selected_are_left_out() {
        let listed = [
            mailbox("INBOX", &[]),
            mailbox("Archive-root", &["\\NoSelect"]),
            mailbox("Gone", &["\\NonExistent"]),
        ];
        assert_eq!(keys(&listed), pairs(&[("INBOX", "inbox")]));
    }

    #[test]
    fn two_folders_never_share_a_key_even_by_case() {
        let listed = [
            mailbox("inbox-archive", &[]),
            mailbox("Notes", &[]),
            mailbox("notes", &[]),
            mailbox("NOTES", &[]),
            mailbox("sent", &[]),
            mailbox("Posted", &["\\Sent"]),
        ];
        let got = keys(&listed);
        assert_eq!(
            got,
            pairs(&[
                ("Posted", "sent"),
                ("NOTES", "NOTES"),
                ("Notes", "Notes-2"),
                ("inbox-archive", "inbox-archive"),
                ("notes", "notes-3"),
                ("sent", "sent-2"),
            ])
        );
    }

    #[test]
    fn a_segment_is_safe_for_files_and_keys() {
        assert_eq!(safe_segment("Work"), "Work");
        assert_eq!(safe_segment("a/b\\c:d*e?f\"g<h>i|j"), "a_b_c_d_e_f_g_h_i_j");
        assert_eq!(safe_segment("tab\there"), "tab_here");
        assert_eq!(safe_segment(".hidden"), "_hidden");
        assert_eq!(safe_segment(".."), "_");
        assert_eq!(safe_segment("  name. . "), "name");
        assert_eq!(safe_segment(""), "_");
        assert_eq!(safe_segment("[Gmail]"), "[Gmail]");
        let long = "é".repeat(100);
        let cut = safe_segment(&long);
        assert!(cut.len() <= 120 && cut.chars().all(|c| c == 'é'), "{cut}");
    }

    #[test]
    fn a_folders_role_follows_from_its_key() {
        assert_eq!(Role::of_key("inbox"), Role::Inbox);
        assert_eq!(Role::of_key("spam"), Role::Spam);
        assert_eq!(Role::of_key("Receipts"), Role::Other);
        assert_eq!(Role::Spam.key(), Some("spam"));
        assert_eq!(Role::Inbox.label(), Some("Inbox"));
        assert_eq!(Role::Other.key(), None);
    }
}
