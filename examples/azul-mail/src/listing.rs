//! The message list and the folder pane as plain data, the way Outlook 2010 arranges them:
//! messages newest first under date groups ("Today", "Yesterday", "Monday", "Last Week", ...),
//! read / unread and flags from the server's flags plus AzMail's own marks, and the folders as a
//! tree in Outlook's order with their unread counts. No azul types here: `lib.rs` turns these
//! into the `MessageList` and the navigation pane's trees.
//!
//! AzMail never writes to the server (the sync is read-only, `EXAMINE` + `BODY.PEEK[]`), so what
//! the user does to a message - opening it marks it read, the flag column flags it - is kept in a
//! file of its own next to the folder's index, `mail/<folder>/flags.json` ([`LocalFlags`]): a
//! sync rewrites `index.jsonl` whole and never touches it.

use std::collections::BTreeMap;

use chrono::{NaiveDate, TimeZone, Weekday};
use serde::{Deserialize, Serialize};

use crate::{folders::Role, store::IndexEntry};

/// The `format` of a folder's local flags file.
pub const FLAGS_FORMAT: &str = "azmail.flags";
/// The local flags file version this AzMail writes, and the newest it reads.
pub const FLAGS_VERSION: u64 = 1;
/// The local flags file in a folder.
pub const FLAGS_FILE: &str = "flags.json";

/// `mail/<folder>/flags.json`
pub fn flags_key(folder: &str) -> String {
    format!("{}/{folder}/{FLAGS_FILE}", crate::store::MAIL_PREFIX)
}

/// Outlook's date groups of a list arranged by date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateGroup {
    Today,
    Yesterday,
    /// An earlier day of this week (weeks start on Monday).
    Weekday(Weekday),
    LastWeek,
    TwoWeeksAgo,
    ThreeWeeksAgo,
    /// Before that, but in the previous calendar month.
    LastMonth,
    Older,
}

impl DateGroup {
    /// The group header ("Today", "Monday", "Last Week").
    pub fn label(self) -> String {
        todo!()
    }
}

/// The group of a message received on `day` when it is `today` (a day after today - a sender's
/// clock running ahead - is Today).
pub fn date_group(day: NaiveDate, today: NaiveDate) -> DateGroup {
    todo!()
}

/// The calendar day of an RFC 3339 date in `tz`; `None` when it is not a date.
pub fn local_day<Tz: TimeZone>(rfc3339: &str, tz: &Tz) -> Option<NaiveDate> {
    todo!()
}

/// The list's date column in `tz`: the time for today ("21:12"), the weekday and time for this
/// week ("Wed 21:12"), the date before ("2026-09-28"); the text as it is when it is not a date.
pub fn list_date<Tz: TimeZone>(rfc3339: &str, today: NaiveDate, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    todo!()
}

/// What AzMail keeps of a folder's messages besides the server's flags: the read marks and
/// flags the user set here, by UID. A mark here wins over the server's flag.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalFlags {
    pub format: String,
    pub version: u64,
    /// UID -> read (`true`) or unread (`false`).
    #[serde(default)]
    pub read: BTreeMap<u32, bool>,
    /// UID -> flagged for follow-up or not.
    #[serde(default)]
    pub flagged: BTreeMap<u32, bool>,
}

impl LocalFlags {
    pub fn create() -> LocalFlags {
        todo!()
    }

    /// The file's contents (pretty JSON, ending in a newline).
    pub fn to_json(&self) -> String {
        todo!()
    }

    /// Reads a flags file; `None` for anything that is not one this AzMail reads.
    pub fn from_json(text: &str) -> Option<LocalFlags> {
        todo!()
    }

    /// Whether `entry` is read: AzMail's mark, else the server's `\Seen`.
    pub fn is_read(&self, entry: &IndexEntry) -> bool {
        todo!()
    }

    /// Whether `entry` is flagged: AzMail's mark, else the server's `\Flagged`.
    pub fn is_flagged(&self, entry: &IndexEntry) -> bool {
        todo!()
    }
}

/// The messages of `entries` that are not read.
pub fn unread_count(entries: &[IndexEntry], flags: &LocalFlags) -> usize {
    todo!()
}

/// Whether `entry` matches the search box: every word of `query` is in its sender, recipients
/// or subject, ignoring case. An empty query matches everything.
pub fn matches_search(entry: &IndexEntry, query: &str) -> bool {
    todo!()
}

/// `entries` by date (RFC 3339 sorts as text), newest first unless `newest_first` is false;
/// equal dates by UID the same way.
pub fn sort_by_date(entries: &mut [IndexEntry], newest_first: bool) {
    todo!()
}

/// One row of the list: a date group's header, or a message (its UID).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListRow {
    Group(DateGroup),
    Message(u32),
}

/// The list's rows: `entries` (already sorted) under their date groups in `tz`, a header before
/// the first message of each group. A message without a date is in Older.
pub fn grouped_rows<Tz: TimeZone>(entries: &[IndexEntry], today: NaiveDate, tz: &Tz) -> Vec<ListRow> {
    todo!()
}

/// A synced folder as the navigation pane needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderInfo {
    /// The folder's key (`inbox`, `Work.Projects`).
    pub key: String,
    /// Its decoded name, the hierarchy shown with `/` (`Work/Projects`).
    pub display: String,
    pub role: Role,
    pub unread: usize,
}

/// A folder in the tree: its key, its label, its unread count and the folders under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderNode {
    pub key: String,
    pub label: String,
    pub unread: usize,
    pub children: Vec<FolderNode>,
}

/// The name Outlook gives a special folder ("Sent Items", "Deleted Items", "Junk E-mail"), or the
/// last segment of the folder's own name.
pub fn folder_label(role: Role, display: &str) -> String {
    todo!()
}

/// Outlook's order for the special folders: Inbox, Drafts, Sent Items, Deleted Items, Junk
/// E-mail, Archive, All Mail, Flagged; every other folder after them, by name.
pub fn role_rank(role: Role) -> u8 {
    todo!()
}

/// The folders as a tree: `A/B` (key `A.B`) under `A` when `A` is synced too, else at the top
/// level under its full name; siblings in Outlook's order ([`role_rank`], then by label).
pub fn folder_tree(folders: &[FolderInfo]) -> Vec<FolderNode> {
    todo!()
}

/// The keys of `nodes` in depth-first order: the tree view numbers its rows that way, the
/// account's own row (the root) first, so row `i` is `keys[i - 1]`.
pub fn preorder_keys(nodes: &[FolderNode]) -> Vec<String> {
    todo!()
}

/// Outlook's Favorites: the Inbox and Sent Items, when they are synced.
pub fn favorites(folders: &[FolderInfo]) -> Vec<FolderNode> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn entry(uid: u32, date: &str, flags: &[&str]) -> IndexEntry {
        IndexEntry {
            uid,
            message_id: format!("m{uid}@example.org"),
            date: date.to_string(),
            from: format!("Sender {uid} <s{uid}@example.org>"),
            to: String::from("ada@example.org"),
            subject: format!("Subject {uid}"),
            flags: flags.iter().map(|f| f.to_string()).collect(),
            size: 100,
            path: format!("mail/inbox/2026/09/{uid}.eml"),
        }
    }

    fn info(key: &str, display: &str, unread: usize) -> FolderInfo {
        FolderInfo {
            key: key.to_string(),
            display: display.to_string(),
            role: Role::of_key(key),
            unread,
        }
    }

    // Thursday, 1 October 2026.
    const TODAY: (i32, u32, u32) = (2026, 10, 1);

    fn today() -> NaiveDate {
        day(TODAY.0, TODAY.1, TODAY.2)
    }

    #[test]
    fn the_date_groups_follow_outlook_from_today_back() {
        let t = today();
        assert_eq!(date_group(t, t), DateGroup::Today);
        assert_eq!(date_group(day(2026, 10, 2), t), DateGroup::Today, "a clock ahead");
        assert_eq!(date_group(day(2026, 9, 30), t), DateGroup::Yesterday);
        // This week started on Monday 28 September.
        assert_eq!(date_group(day(2026, 9, 29), t), DateGroup::Weekday(Weekday::Tue));
        assert_eq!(date_group(day(2026, 9, 28), t), DateGroup::Weekday(Weekday::Mon));
        assert_eq!(date_group(day(2026, 9, 27), t), DateGroup::LastWeek);
        assert_eq!(date_group(day(2026, 9, 21), t), DateGroup::LastWeek);
        assert_eq!(date_group(day(2026, 9, 20), t), DateGroup::TwoWeeksAgo);
        assert_eq!(date_group(day(2026, 9, 14), t), DateGroup::TwoWeeksAgo);
        assert_eq!(date_group(day(2026, 9, 13), t), DateGroup::ThreeWeeksAgo);
        assert_eq!(date_group(day(2026, 9, 7), t), DateGroup::ThreeWeeksAgo);
        assert_eq!(date_group(day(2026, 9, 6), t), DateGroup::LastMonth);
        assert_eq!(date_group(day(2026, 9, 1), t), DateGroup::LastMonth);
        assert_eq!(date_group(day(2026, 8, 31), t), DateGroup::Older);
        assert_eq!(date_group(day(1999, 1, 1), t), DateGroup::Older);
    }

    #[test]
    fn on_a_monday_yesterday_is_last_week_s_sunday_and_no_weekday_group_exists() {
        let monday = day(2026, 9, 28);
        assert_eq!(date_group(day(2026, 9, 27), monday), DateGroup::Yesterday);
        assert_eq!(date_group(day(2026, 9, 26), monday), DateGroup::LastWeek);
    }

    #[test]
    fn the_group_labels_are_outlooks() {
        assert_eq!(DateGroup::Today.label(), "Today");
        assert_eq!(DateGroup::Yesterday.label(), "Yesterday");
        assert_eq!(DateGroup::Weekday(Weekday::Mon).label(), "Monday");
        assert_eq!(DateGroup::Weekday(Weekday::Wed).label(), "Wednesday");
        assert_eq!(DateGroup::LastWeek.label(), "Last Week");
        assert_eq!(DateGroup::TwoWeeksAgo.label(), "Two Weeks Ago");
        assert_eq!(DateGroup::ThreeWeeksAgo.label(), "Three Weeks Ago");
        assert_eq!(DateGroup::LastMonth.label(), "Last Month");
        assert_eq!(DateGroup::Older.label(), "Older");
    }

    #[test]
    fn the_date_column_shows_the_time_today_the_weekday_this_week_and_the_date_before() {
        let berlin = chrono::FixedOffset::east_opt(2 * 3600).unwrap();
        let t = today();
        assert_eq!(list_date("2026-10-01T08:42:00Z", t, &berlin), "10:42");
        assert_eq!(list_date("2026-09-30T19:12:00Z", t, &berlin), "Wed 21:12");
        assert_eq!(list_date("2026-09-28T06:00:00Z", t, &berlin), "Mon 08:00");
        assert_eq!(list_date("2026-09-27T06:00:00Z", t, &berlin), "2026-09-27");
        // 23:30 UTC on the 30th is already the 1st in Berlin: today.
        assert_eq!(list_date("2026-09-30T23:30:00Z", t, &berlin), "01:30");
        assert_eq!(list_date("", t, &berlin), "");
        assert_eq!(list_date("soon", t, &berlin), "soon");
        assert_eq!(local_day("2026-09-30T23:30:00Z", &berlin), Some(day(2026, 10, 1)));
        assert_eq!(local_day("nope", &berlin), None);
    }

    #[test]
    fn the_rows_are_the_messages_under_a_header_per_date_group() {
        let utc = chrono::Utc;
        let mut entries = vec![
            entry(1, "2026-09-01T10:00:00Z", &[]),
            entry(2, "2026-10-01T09:00:00Z", &[]),
            entry(3, "2026-09-30T09:00:00Z", &[]),
            entry(4, "2026-10-01T07:00:00Z", &[]),
            entry(5, "", &[]),
        ];
        sort_by_date(&mut entries, true);
        assert_eq!(
            entries.iter().map(|e| e.uid).collect::<Vec<_>>(),
            vec![2, 4, 3, 1, 5],
            "newest first, a message without a date last"
        );
        assert_eq!(
            grouped_rows(&entries, today(), &utc),
            vec![
                ListRow::Group(DateGroup::Today),
                ListRow::Message(2),
                ListRow::Message(4),
                ListRow::Group(DateGroup::Yesterday),
                ListRow::Message(3),
                ListRow::Group(DateGroup::LastMonth),
                ListRow::Message(1),
                ListRow::Group(DateGroup::Older),
                ListRow::Message(5),
            ]
        );
        sort_by_date(&mut entries, false);
        assert_eq!(
            entries.iter().map(|e| e.uid).collect::<Vec<_>>(),
            vec![5, 1, 3, 4, 2],
            "oldest first"
        );
        assert!(grouped_rows(&[], today(), &utc).is_empty());
    }

    #[test]
    fn a_local_mark_wins_over_the_servers_flag() {
        let seen = entry(1, "", &["\\Seen"]);
        let unseen = entry(2, "", &[]);
        let flagged = entry(3, "", &["\\Flagged", "\\Seen"]);
        let mut flags = LocalFlags::create();
        assert!(flags.is_read(&seen) && !flags.is_read(&unseen));
        assert!(flags.is_flagged(&flagged) && !flags.is_flagged(&seen));
        assert_eq!(unread_count(&[seen.clone(), unseen.clone(), flagged.clone()], &flags), 1);
        flags.read.insert(2, true);
        flags.read.insert(1, false);
        flags.flagged.insert(3, false);
        flags.flagged.insert(1, true);
        assert!(flags.is_read(&unseen), "opened here");
        assert!(!flags.is_read(&seen), "marked unread here");
        assert!(!flags.is_flagged(&flagged) && flags.is_flagged(&seen));
        assert_eq!(unread_count(&[seen, unseen, flagged], &flags), 1);
    }

    #[test]
    fn the_local_flags_file_round_trips_and_rejects_other_files() {
        let mut flags = LocalFlags::create();
        flags.read.insert(7, true);
        flags.flagged.insert(9, true);
        let text = flags.to_json();
        assert!(text.contains("\"azmail.flags\"") && text.ends_with('\n'), "{text}");
        assert_eq!(LocalFlags::from_json(&text), Some(flags));
        assert_eq!(LocalFlags::from_json("{}"), None);
        assert_eq!(LocalFlags::from_json("not json"), None);
        assert_eq!(
            LocalFlags::from_json(r#"{"format":"azmail.flags","version":99}"#),
            None,
            "a newer version is not read"
        );
        assert_eq!(flags_key("inbox"), "mail/inbox/flags.json");
    }

    #[test]
    fn the_search_box_matches_every_word_in_sender_recipients_or_subject() {
        let e = IndexEntry {
            from: String::from("Ben Okafor <ben@example.org>"),
            to: String::from("ada@example.org"),
            subject: String::from("Re: Garden plan for October"),
            ..entry(1, "", &[])
        };
        assert!(matches_search(&e, ""));
        assert!(matches_search(&e, "garden"));
        assert!(matches_search(&e, "OKAFOR october"));
        assert!(matches_search(&e, "ada@"));
        assert!(!matches_search(&e, "garden november"));
    }

    #[test]
    fn special_folders_take_outlooks_names_and_order() {
        assert_eq!(folder_label(Role::Inbox, "INBOX"), "Inbox");
        assert_eq!(folder_label(Role::Sent, "Sent"), "Sent Items");
        assert_eq!(folder_label(Role::Trash, "Trash"), "Deleted Items");
        assert_eq!(folder_label(Role::Spam, "Spam"), "Junk E-mail");
        assert_eq!(folder_label(Role::Drafts, "Entwürfe"), "Drafts");
        assert_eq!(folder_label(Role::Other, "Work/Projects"), "Projects");
        assert_eq!(folder_label(Role::Other, "Receipts"), "Receipts");
        assert!(role_rank(Role::Inbox) < role_rank(Role::Drafts));
        assert!(role_rank(Role::Drafts) < role_rank(Role::Sent));
        assert!(role_rank(Role::Sent) < role_rank(Role::Trash));
        assert!(role_rank(Role::Trash) < role_rank(Role::Spam));
        assert!(role_rank(Role::Flagged) < role_rank(Role::Other));
    }

    #[test]
    fn the_folder_tree_nests_children_and_orders_like_outlook() {
        let folders = vec![
            info("Work.Projects", "Work/Projects", 1),
            info("spam", "Spam", 4),
            info("Receipts", "Receipts", 0),
            info("sent", "Sent", 0),
            info("Work", "Work", 2),
            info("inbox", "INBOX", 3),
            info("Lonely.Child", "Lonely/Child", 0),
            info("drafts", "Drafts", 0),
        ];
        let tree = folder_tree(&folders);
        let top: Vec<(&str, &str, usize)> = tree
            .iter()
            .map(|n| (n.key.as_str(), n.label.as_str(), n.unread))
            .collect();
        assert_eq!(
            top,
            vec![
                ("inbox", "Inbox", 3),
                ("drafts", "Drafts", 0),
                ("sent", "Sent Items", 0),
                ("spam", "Junk E-mail", 4),
                ("Lonely.Child", "Lonely/Child", 0),
                ("Receipts", "Receipts", 0),
                ("Work", "Work", 2),
            ]
        );
        let work = tree.iter().find(|n| n.key == "Work").unwrap();
        assert_eq!(
            work.children,
            vec![FolderNode {
                key: String::from("Work.Projects"),
                label: String::from("Projects"),
                unread: 1,
                children: Vec::new(),
            }]
        );
        assert_eq!(
            preorder_keys(&tree),
            vec!["inbox", "drafts", "sent", "spam", "Lonely.Child", "Receipts", "Work", "Work.Projects"]
        );
    }

    #[test]
    fn the_favorites_are_the_inbox_and_sent_items() {
        let folders = vec![info("sent", "Sent", 0), info("Work", "Work", 2), info("inbox", "INBOX", 5)];
        let fav = favorites(&folders);
        assert_eq!(
            fav.iter().map(|n| (n.key.as_str(), n.label.as_str(), n.unread)).collect::<Vec<_>>(),
            vec![("inbox", "Inbox", 5), ("sent", "Sent Items", 0)]
        );
        assert!(favorites(&[info("Work", "Work", 0)]).is_empty());
    }
}
