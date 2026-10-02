//! Writing mail, as plain data: the body model the compose window edits, what New / Reply /
//! Reply All / Forward start from, the address lines, the mail handed to sending
//! (`send::OutgoingMail`), and a draft as an `.eml` file in the Drafts folder.
//!
//! The body is azul's `RichTextDoc`, the document of the shared rich-text editor the compose
//! window edits (the one AzNotes and AzWriter use): blocks with a quote depth, a kind and runs of
//! formatted text and links. Here it becomes the two parts of the mail: `text/plain` with `> `
//! quoting (`RichTextDoc::to_plain_text`) and `text/html` with the quotes nested as
//! `<blockquote type="cite">` (`RichTextDoc::to_html`) - the form Gmail and Thunderbird write and
//! read. A reopened draft reads its HTML part back (`RichTextDoc::create_from_html`), so its bold,
//! italic and links come back with it.
//!
//! A reply starts with an empty line for the caret, then "On <date>, <sender> wrote:" and the
//! original one quote level deeper; a forward with the original's header block and its text.
//!
//! A draft is SEND's message (`send::build_message`, micromail's MIME builder - the one
//! generator AzMail has) with the Bcc a draft keeps, filed by SEND's `send::file_message` in
//! `mail/drafts/<yyyy>/<mm>/<uid>.eml` with its index line: the same layout as the synced mail,
//! a local UID from `send::LOCAL_UID_FLOOR` up (far above any server's), and a state that says
//! the folder is local only until a sync adopts it (`sync::plan_folder`).

use std::path::Path;

use azul::{
    vec::RichBlockVec,
    widgets::{RichBlock, RichTextDoc},
};

use crate::{
    message::MessageView,
    send::{Attachment, OutgoingMail},
    store::{self, FolderState, IndexEntry, LocalFolder},
};

/// The folder drafts are saved in (the `\Drafts` folder's fixed key).
pub const DRAFTS_FOLDER: &str = "drafts";
/// What a compose window was opened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeKind {
    New,
    Reply,
    ReplyAll,
    Forward,
    /// A saved draft, opened again.
    Draft,
}

impl ComposeKind {
    /// The word the window's title and the stdout line use ("new", "reply", "reply-all",
    /// "forward", "draft").
    pub fn name(self) -> &'static str {
        match self {
            ComposeKind::New => "new",
            ComposeKind::Reply => "reply",
            ComposeKind::ReplyAll => "reply-all",
            ComposeKind::Forward => "forward",
            ComposeKind::Draft => "draft",
        }
    }
}

/// The compose window's title, as Outlook writes it: "<subject> - Message (HTML)", "Untitled -
/// Message (HTML)" before there is a subject.
pub fn window_title(subject: &str) -> String {
    let subject = subject.trim();
    if subject.is_empty() {
        String::from("Untitled - Message (HTML)")
    } else {
        format!("{subject} - Message (HTML)")
    }
}

/// `Re: <subject>`, unless the subject already starts with a reply prefix (Re, RE, AW, Aw, SV,
/// Antw - any case, with the colon).
pub fn reply_subject(subject: &str) -> String {
    if has_prefix(subject, REPLY_PREFIXES) {
        subject.to_string()
    } else {
        format!("Re: {subject}")
    }
}

/// `Fwd: <subject>`, unless the subject already starts with a forward prefix (Fwd, FW, WG, TR,
/// Doorst - any case, with the colon).
pub fn forward_subject(subject: &str) -> String {
    if has_prefix(subject, FORWARD_PREFIXES) {
        subject.to_string()
    } else {
        format!("Fwd: {subject}")
    }
}

/// The entries of an address line: split at commas and semicolons that are not inside quotes
/// or angle brackets, trimmed, empty ones left out.
pub fn split_addresses(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut angle = 0usize;
    for c in line.chars() {
        if escaped {
            escaped = false;
            current.push(c);
            continue;
        }
        match c {
            '\\' if quoted => {
                escaped = true;
                current.push(c);
            }
            '"' => {
                quoted = !quoted;
                current.push(c);
            }
            '<' if !quoted => {
                angle += 1;
                current.push(c);
            }
            '>' if !quoted => {
                angle = angle.saturating_sub(1);
                current.push(c);
            }
            ',' | ';' if !quoted && angle == 0 => {
                let entry = current.trim();
                if !entry.is_empty() {
                    out.push(entry.to_string());
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }
    let entry = current.trim();
    if !entry.is_empty() {
        out.push(entry.to_string());
    }
    out
}

/// The address of an entry: `ada@example.org` from `Ada <ada@example.org>`, `"L, Ada"
/// <ada@example.org>` or `ada@example.org`; `None` when there is no address in it.
pub fn bare_address(entry: &str) -> Option<String> {
    let entry = entry.trim();
    let candidate = match (entry.rfind('<'), entry.rfind('>')) {
        (Some(open), Some(close)) if open < close => entry[open + 1..close].trim(),
        _ => entry,
    };
    crate::account::is_email(candidate).then(|| candidate.to_string())
}

/// Whether two entries name the same mailbox (their addresses, ignoring case).
pub fn same_address(a: &str, b: &str) -> bool {
    match (bare_address(a), bare_address(b)) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(&b),
        _ => false,
    }
}

/// The header fields a reply or forward starts with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartFields {
    pub to: String,
    pub cc: String,
    /// A reopened draft's Bcc; empty for a reply or a forward.
    pub bcc: String,
    pub subject: String,
    /// The original's Message-ID (bare, no angle brackets), for In-Reply-To.
    pub in_reply_to: Option<String>,
    /// The original's References and then its Message-ID (bare).
    pub references: Vec<String>,
}

/// A reply to `original` from `me` (the account's address). To: the Reply-To, else the sender;
/// Reply All adds the original's other recipients (To to To, Cc to Cc), never `me`, each
/// mailbox once. A reply to a message `me` sent goes to its recipients.
pub fn reply_fields(original: &MessageView, me: &str, all: bool) -> StartFields {
    let from_me = split_addresses(&original.from)
        .first()
        .is_some_and(|sender| same_address(sender, me));
    let mut to = if from_me {
        split_addresses(&original.to)
    } else if !original.reply_to.trim().is_empty() {
        split_addresses(&original.reply_to)
    } else {
        split_addresses(&original.from)
    };
    let mut cc = Vec::new();
    if all {
        if !from_me {
            to.extend(split_addresses(&original.to));
        }
        cc = split_addresses(&original.cc);
    }
    // Every mailbox once (To before Cc), never me, never an entry without an address.
    let mut seen: Vec<String> = Vec::new();
    let mut keep = |list: Vec<String>| -> Vec<String> {
        list.into_iter()
            .filter(|entry| match bare_address(entry) {
                Some(address) => {
                    let address = address.to_lowercase();
                    let fresh = !same_address(entry, me) && !seen.contains(&address);
                    if fresh {
                        seen.push(address);
                    }
                    fresh
                }
                None => false,
            })
            .collect()
    };
    let to = keep(to);
    let cc = keep(cc);
    let (in_reply_to, references) = thread_of(original);
    StartFields {
        to: to.join(", "),
        cc: cc.join(", "),
        bcc: String::new(),
        subject: reply_subject(&original.subject),
        in_reply_to,
        references,
    }
}

/// A forward of `original`: no recipients, the subject, the thread.
pub fn forward_fields(original: &MessageView) -> StartFields {
    let (in_reply_to, references) = thread_of(original);
    StartFields {
        to: String::new(),
        cc: String::new(),
        bcc: String::new(),
        subject: forward_subject(&original.subject),
        in_reply_to,
        references,
    }
}

/// A saved draft reopened: its own To / Cc / Bcc / Subject and the thread it continues (its
/// In-Reply-To and References as saved; its own Message-ID is not part of the thread).
pub fn draft_fields(draft: &MessageView) -> StartFields {
    StartFields {
        to: draft.to.clone(),
        cc: draft.cc.clone(),
        bcc: draft.bcc.clone(),
        subject: draft.subject.clone(),
        in_reply_to: Some(draft.in_reply_to.clone()).filter(|id| !id.is_empty()),
        references: draft.references.clone(),
    }
}

/// A paragraph of plain `text` at quote depth `depth`.
fn paragraph(depth: u8, text: &str) -> RichBlock {
    RichBlock::create_paragraph(text).with_quote_depth(depth)
}

/// The blocks of plain `text`, one paragraph per line, `>` marks as quote depth plus `extra`;
/// none for no text.
fn quoted_blocks(text: &str, extra: u8) -> Vec<RichBlock> {
    if text.strip_suffix('\n').unwrap_or(text).is_empty() {
        return Vec::new();
    }
    RichTextDoc::create_from_plain_text(text, extra).blocks.iter().cloned().collect()
}

/// A body of `blocks`.
fn body_of(blocks: Vec<RichBlock>) -> RichTextDoc {
    RichTextDoc::create_from_blocks(RichBlockVec::from_vec(blocks))
}

/// A reply's body: an empty paragraph (the caret's), the quote header, and the original's text
/// one level deeper.
pub fn reply_quote(original: &MessageView, header: &str) -> RichTextDoc {
    let mut blocks = vec![paragraph(0, ""), paragraph(0, header)];
    blocks.extend(quoted_blocks(&original.text, 1));
    body_of(blocks)
}

/// A forward's body: an empty paragraph, the forwarded header block (From, Date, Subject, To,
/// Cc), an empty line and the original's text.
pub fn forward_quote(original: &MessageView, date: &str) -> RichTextDoc {
    let mut blocks = vec![
        paragraph(0, ""),
        paragraph(0, "---------- Forwarded message ----------"),
        paragraph(0, &format!("From: {}", original.from)),
    ];
    if !date.is_empty() {
        blocks.push(paragraph(0, &format!("Date: {date}")));
    }
    blocks.push(paragraph(0, &format!("Subject: {}", original.subject)));
    if !original.to.is_empty() {
        blocks.push(paragraph(0, &format!("To: {}", original.to)));
    }
    if !original.cc.is_empty() {
        blocks.push(paragraph(0, &format!("Cc: {}", original.cc)));
    }
    blocks.push(paragraph(0, ""));
    blocks.extend(quoted_blocks(&original.text, 0));
    body_of(blocks)
}

/// A reopened draft's body: its HTML part (formats and links kept), else its text.
pub fn draft_body(draft: &MessageView) -> RichTextDoc {
    match draft.html.as_deref().filter(|h| !h.trim().is_empty()) {
        Some(html) => RichTextDoc::create_from_html(html),
        None => RichTextDoc::create_from_plain_text(draft.text.as_str(), 0),
    }
}

/// The `text/html` part of `body`: its HTML in `<html><body>`.
pub fn html_part(body: &RichTextDoc) -> String {
    format!("<html><body>{}</body></html>", body.to_html().as_str())
}

/// The line before a reply's quote: `On Wed, 30 Sep 2026 at 10:42, Ada <ada@example.org>
/// wrote:` (`date` as the reader's zone shows it; without a date: `<sender> wrote:`).
pub fn quote_header(date: &str, from: &str) -> String {
    if date.is_empty() {
        format!("{from} wrote:")
    } else {
        format!("On {date}, {from} wrote:")
    }
}

/// An RFC 3339 date as the quote header writes it in `tz`: `Wed, 30 Sep 2026 at 10:42`; empty
/// when it is not a date.
pub fn header_date_in<Tz: chrono::TimeZone>(rfc3339: &str, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    chrono::DateTime::parse_from_rfc3339(rfc3339.trim())
        .map(|date| date.with_timezone(tz).format("%a, %-d %b %Y at %H:%M").to_string())
        .unwrap_or_default()
}

/// Why a mail cannot be sent as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeError {
    /// No address in To, Cc or Bcc.
    NoRecipient,
    /// An entry that is not an address.
    BadAddress(String),
    /// The account has no address to send from.
    NoSender,
}

impl std::fmt::Display for ComposeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComposeError::NoRecipient => write!(f, "Add at least one recipient."),
            ComposeError::BadAddress(a) => write!(f, "\"{a}\" is not an e-mail address."),
            ComposeError::NoSender => write!(f, "The account has no address to send from."),
        }
    }
}

/// What the compose window holds when Send or Save is pressed.
#[derive(Debug, Clone, PartialEq)]
pub struct ComposeFields {
    /// The account's address (`Name <address>` or `address`).
    pub from: String,
    /// The address lines as typed.
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub body: RichTextDoc,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
}

/// The mail to send: every address line split and checked, the body as text and HTML.
pub fn outgoing(fields: &ComposeFields, attachments: Vec<Attachment>) -> Result<OutgoingMail, ComposeError> {
    if bare_address(&fields.from).is_none() {
        return Err(ComposeError::NoSender);
    }
    let to = checked_line(&fields.to)?;
    let cc = checked_line(&fields.cc)?;
    let bcc = checked_line(&fields.bcc)?;
    if to.is_empty() && cc.is_empty() && bcc.is_empty() {
        return Err(ComposeError::NoRecipient);
    }
    Ok(OutgoingMail {
        from: fields.from.trim().to_string(),
        to,
        cc,
        bcc,
        subject: fields.subject.clone(),
        text_body: fields.body.to_plain_text().as_str().to_string(),
        html_body: Some(html_part(&fields.body)),
        in_reply_to: fields.in_reply_to.clone(),
        references: fields.references.clone(),
        attachments,
    })
}

/// The mail a DRAFT is saved as: like [`outgoing`], but nothing is checked - a draft may have
/// no recipient yet, or half an address.
pub fn draft_mail(fields: &ComposeFields, attachments: Vec<Attachment>) -> OutgoingMail {
    OutgoingMail {
        from: fields.from.trim().to_string(),
        to: split_addresses(&fields.to),
        cc: split_addresses(&fields.cc),
        bcc: split_addresses(&fields.bcc),
        subject: fields.subject.clone(),
        text_body: fields.body.to_plain_text().as_str().to_string(),
        html_body: Some(html_part(&fields.body)),
        in_reply_to: fields.in_reply_to.clone(),
        references: fields.references.clone(),
        attachments,
    }
}

/// The media type of a file by its extension (`application/octet-stream` for anything unknown).
pub fn mime_type_for(file_name: &str) -> &'static str {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "pdf" => "application/pdf",
        "txt" | "text" | "log" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "htm" | "html" => "text/html",
        "ics" => "text/calendar",
        "vcf" => "text/vcard",
        "eml" => "message/rfc822",
        "json" => "application/json",
        "xml" => "application/xml",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "heic" => "image/heic",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        _ => "application/octet-stream",
    }
}

/// The draft as a message file: SEND's message for `mail` (`send::build_message`, dated
/// `now_secs`) with `Bcc` (a draft keeps it; the sent message has none - bare addresses, which
/// need no encoding) and `X-AzMail-Draft: 1` in front of its header.
pub fn draft_bytes(mail: &OutgoingMail, now_secs: i64) -> Vec<u8> {
    let built = crate::send::build_message(mail, now_secs);
    let bcc: Vec<String> = mail.bcc.iter().filter_map(|entry| bare_address(entry)).collect();
    let mut out = Vec::with_capacity(built.bytes.len() + 64);
    if !bcc.is_empty() {
        out.extend_from_slice(format!("Bcc: {}\r\n", bcc.join(", ")).as_bytes());
    }
    out.extend_from_slice(b"X-AzMail-Draft: 1\r\n");
    out.extend_from_slice(&built.bytes);
    out
}

/// Files a draft's message file in the Drafts folder under `store_root` through SEND's
/// `send::file_message` (a new local UID, its index line, a local-only state for a folder never
/// synced), after removing `replaces` - the same draft saved before - so it is there once.
/// Returns the new index line.
pub fn save_draft(
    store_root: &Path,
    replaces: Option<u32>,
    bytes: &[u8],
    now_secs: i64,
) -> std::io::Result<IndexEntry> {
    if let Some(old) = replaces {
        delete_draft(&LocalFolder::new(store_root.to_path_buf()), old)?;
    }
    let flags = [String::from("\\Seen"), String::from("\\Draft")];
    crate::send::file_message(store_root, DRAFTS_FOLDER, bytes, &flags, now_secs)
}

/// Removes a draft (once it is sent): its file and its index line.
pub fn delete_draft(store: &LocalFolder, uid: u32) -> std::io::Result<()> {
    let mut index = read_drafts_index(store);
    let Some(at) = index.iter().position(|e| e.uid == uid) else {
        return Ok(());
    };
    let entry = index.remove(at);
    store.delete(&entry.path)?;
    write_drafts_index(store, &index)
}

// ==== Helpers ====

/// Reply prefixes: English, German (AW, Antw), Scandinavian (SV, VS), French (Ref).
const REPLY_PREFIXES: &[&str] = &["re", "aw", "antw", "sv", "vs", "ref"];
/// Forward prefixes: English (Fwd, FW), German (WG), French (TR), Dutch (Doorst).
const FORWARD_PREFIXES: &[&str] = &["fwd", "fw", "wg", "tr", "doorst"];

/// Whether `subject` starts with one of `prefixes` and a colon (`Re:`, `RE[2]:`), any case.
fn has_prefix(subject: &str, prefixes: &[&str]) -> bool {
    let Some((word, _)) = subject.trim_start().split_once(':') else {
        return false;
    };
    let base = word.split('[').next().unwrap_or("").trim();
    prefixes.iter().any(|p| base.eq_ignore_ascii_case(p))
}

/// The original's Message-ID (for In-Reply-To) and the References a reply carries: the
/// original's, then its Message-ID.
fn thread_of(original: &MessageView) -> (Option<String>, Vec<String>) {
    let id = original.message_id.trim();
    let mut references: Vec<String> = original
        .references
        .iter()
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty())
        .collect();
    if id.is_empty() {
        return (None, references);
    }
    if references.last().map(String::as_str) != Some(id) {
        references.push(id.to_string());
    }
    (Some(id.to_string()), references)
}

/// An address line split, every entry checked.
fn checked_line(line: &str) -> Result<Vec<String>, ComposeError> {
    let entries = split_addresses(line);
    match entries.iter().find(|e| bare_address(e).is_none()) {
        Some(bad) => Err(ComposeError::BadAddress(bad.clone())),
        None => Ok(entries),
    }
}

/// The Drafts folder's index; empty when there is none.
fn read_drafts_index(store: &LocalFolder) -> Vec<IndexEntry> {
    store
        .get(&store::index_key(DRAFTS_FOLDER))
        .map(|bytes| store::index_from_jsonl(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default()
}

/// Writes the Drafts folder's index and its state: the synced state when the server's Drafts
/// is synced, else one that says the folder is local only (UIDVALIDITY 0).
fn write_drafts_index(store: &LocalFolder, index: &[IndexEntry]) -> std::io::Result<()> {
    store.put(
        &store::index_key(DRAFTS_FOLDER),
        store::index_to_jsonl(index).as_bytes(),
        true,
    )?;
    let mut state = store
        .get(&store::state_key(DRAFTS_FOLDER))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| FolderState::from_json(&text))
        .unwrap_or_else(|| FolderState::create("", "Drafts", 0));
    state.messages = index.len() as u64;
    store.put(
        &store::state_key(DRAFTS_FOLDER),
        state.to_json().as_bytes(),
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn original() -> MessageView {
        MessageView {
            subject: String::from("Garden plan for October"),
            from: String::from("Ben Okafor <ben@example.org>"),
            to: String::from("Ada Lovelace <ada@example.org>, cleo@example.org"),
            cc: String::from("Dan <dan@example.org>, ADA@example.org"),
            date: String::from("2026-09-30T08:42:00Z"),
            text: String::from("Hi Ada,\n\nthe bulbs are here.\n> Last year it came early.\n"),
            html: None,
            attachments: Vec::new(),
            message_id: String::from("garden-1@example.org"),
            reply_to: String::new(),
            references: vec![String::from("root-0@example.org")],
            bcc: String::new(),
            in_reply_to: String::new(),
        }
    }

    #[test]
    fn a_reopened_draft_starts_with_all_its_fields_and_the_thread_it_continues() {
        let draft = MessageView {
            subject: String::from("Re: Garden plan for October"),
            from: String::from("Ada Lovelace <ada@example.org>"),
            to: String::from("Ben Okafor <ben@example.org>"),
            cc: String::from("dan@example.org"),
            bcc: String::from("Eve <eve@example.org>"),
            message_id: String::from("draft-1@example.org"),
            in_reply_to: String::from("garden-1@example.org"),
            references: vec![
                String::from("root-0@example.org"),
                String::from("garden-1@example.org"),
            ],
            ..original()
        };
        let fields = draft_fields(&draft);
        assert_eq!(fields.to, "Ben Okafor <ben@example.org>");
        assert_eq!(fields.cc, "dan@example.org");
        assert_eq!(fields.bcc, "Eve <eve@example.org>");
        assert_eq!(fields.subject, "Re: Garden plan for October", "no second prefix");
        assert_eq!(fields.in_reply_to.as_deref(), Some("garden-1@example.org"));
        assert_eq!(fields.references, draft.references, "the draft's own id is not added");
        let fresh = MessageView {
            in_reply_to: String::new(),
            references: Vec::new(),
            ..draft
        };
        assert_eq!(draft_fields(&fresh).in_reply_to, None, "a new mail answers nothing");
    }

    fn runs(doc: &RichTextDoc) -> Vec<(u8, String)> {
        doc.blocks
            .iter()
            .map(|b| (b.quote_depth, b.get_text().as_str().to_string()))
            .collect()
    }

    #[test]
    fn subjects_get_one_prefix_whatever_language_the_last_one_was_in() {
        assert_eq!(reply_subject("Garden plan"), "Re: Garden plan");
        assert_eq!(reply_subject("Re: Garden plan"), "Re: Garden plan");
        assert_eq!(reply_subject("RE: Garden plan"), "RE: Garden plan");
        assert_eq!(reply_subject("AW: Gartenplan"), "AW: Gartenplan");
        assert_eq!(reply_subject(""), "Re: ");
        assert_eq!(reply_subject("Rest of the plan"), "Re: Rest of the plan", "a word, no prefix");
        assert_eq!(forward_subject("Garden plan"), "Fwd: Garden plan");
        assert_eq!(forward_subject("FW: Garden plan"), "FW: Garden plan");
        assert_eq!(forward_subject("WG: Gartenplan"), "WG: Gartenplan");
        assert_eq!(forward_subject("Re: Garden plan"), "Fwd: Re: Garden plan");
        assert_eq!(window_title(""), "Untitled - Message (HTML)");
        assert_eq!(window_title("Re: Garden plan"), "Re: Garden plan - Message (HTML)");
        assert_eq!(ComposeKind::ReplyAll.name(), "reply-all");
    }

    #[test]
    fn address_lines_split_outside_quotes_and_brackets() {
        assert_eq!(
            split_addresses(r#"Ada <ada@example.org>, "Okafor, Ben" <ben@example.org>; cleo@example.org,,"#),
            vec![
                String::from("Ada <ada@example.org>"),
                String::from(r#""Okafor, Ben" <ben@example.org>"#),
                String::from("cleo@example.org"),
            ]
        );
        assert!(split_addresses("  ").is_empty());
        assert_eq!(bare_address("Ada <ada@example.org>").as_deref(), Some("ada@example.org"));
        assert_eq!(bare_address(" ada@example.org ").as_deref(), Some("ada@example.org"));
        assert_eq!(bare_address(r#""L, Ada" <ada@example.org>"#).as_deref(), Some("ada@example.org"));
        assert_eq!(bare_address("Ada Lovelace"), None);
        assert_eq!(bare_address("<>"), None);
        assert!(same_address("ADA@Example.org", "Ada <ada@example.org>"));
        assert!(!same_address("ada@example.org", "ben@example.org"));
    }

    #[test]
    fn a_reply_goes_to_the_sender_and_reply_all_to_everyone_but_me() {
        let me = "ada@example.org";
        let reply = reply_fields(&original(), me, false);
        assert_eq!(reply.to, "Ben Okafor <ben@example.org>");
        assert_eq!(reply.cc, "");
        assert_eq!(reply.subject, "Re: Garden plan for October");
        assert_eq!(reply.in_reply_to.as_deref(), Some("garden-1@example.org"));
        assert_eq!(
            reply.references,
            vec![String::from("root-0@example.org"), String::from("garden-1@example.org")]
        );
        let all = reply_fields(&original(), me, true);
        assert_eq!(all.to, "Ben Okafor <ben@example.org>, cleo@example.org");
        assert_eq!(all.cc, "Dan <dan@example.org>", "never me, in any case");
        let with_reply_to = MessageView {
            reply_to: String::from("Garden List <list@example.org>"),
            ..original()
        };
        assert_eq!(reply_fields(&with_reply_to, me, false).to, "Garden List <list@example.org>");
        let mine = MessageView {
            from: String::from("Ada <ada@example.org>"),
            to: String::from("ben@example.org"),
            cc: String::new(),
            ..original()
        };
        assert_eq!(reply_fields(&mine, me, false).to, "ben@example.org", "a reply to my own mail");
    }

    #[test]
    fn a_forward_has_no_recipients_and_keeps_the_thread() {
        let f = forward_fields(&original());
        assert_eq!((f.to.as_str(), f.cc.as_str()), ("", ""));
        assert_eq!(f.subject, "Fwd: Garden plan for October");
        assert_eq!(f.in_reply_to.as_deref(), Some("garden-1@example.org"));
    }

    #[test]
    fn a_reply_starts_with_an_empty_line_then_the_header_then_the_quote_one_level_deeper() {
        let header = quote_header("Wed, 30 Sep 2026 at 10:42", "Ben Okafor <ben@example.org>");
        assert_eq!(header, "On Wed, 30 Sep 2026 at 10:42, Ben Okafor <ben@example.org> wrote:");
        assert_eq!(quote_header("", "Ben"), "Ben wrote:");
        let doc = reply_quote(&original(), &header);
        assert_eq!(
            runs(&doc),
            vec![
                (0, String::new()),
                (0, header.clone()),
                (1, String::from("Hi Ada,")),
                (1, String::new()),
                (1, String::from("the bulbs are here.")),
                (2, String::from("Last year it came early.")),
            ]
        );
    }

    #[test]
    fn the_quote_headers_date_is_the_readers() {
        let berlin = chrono::FixedOffset::east_opt(2 * 3600).unwrap();
        assert_eq!(header_date_in("2026-09-30T08:42:00Z", &berlin), "Wed, 30 Sep 2026 at 10:42");
        assert_eq!(header_date_in("", &berlin), "");
        assert_eq!(header_date_in("later", &berlin), "");
    }

    #[test]
    fn a_forward_carries_the_header_block_and_the_text_unquoted() {
        let doc = forward_quote(&original(), "Wed, 30 Sep 2026 at 10:42");
        let lines = runs(&doc);
        assert_eq!(lines[0], (0, String::new()));
        assert_eq!(lines[1], (0, String::from("---------- Forwarded message ----------")));
        assert!(lines.contains(&(0, String::from("From: Ben Okafor <ben@example.org>"))));
        assert!(lines.contains(&(0, String::from("Date: Wed, 30 Sep 2026 at 10:42"))));
        assert!(lines.contains(&(0, String::from("Subject: Garden plan for October"))));
        assert!(lines.contains(&(0, String::from("Cc: Dan <dan@example.org>, ADA@example.org"))));
        assert!(lines.contains(&(0, String::from("the bulbs are here."))));
        assert!(lines.contains(&(1, String::from("Last year it came early."))));
    }

    /// A body with bold, a link, a list and a quote.
    fn formatted_body() -> RichTextDoc {
        RichTextDoc::create_from_html(
            "<div>Thanks, <b>bold</b> &amp; <a href=\"https://example.org/plan\">the plan</a></div>\
             <ul><li>bulbs</li></ul><blockquote type=\"cite\"><div>Bring gloves.</div></blockquote>",
        )
    }

    #[test]
    fn the_mail_parts_are_the_shared_writers_text_and_html() {
        let body = formatted_body();
        let mail = outgoing(
            &ComposeFields {
                body: body.clone(),
                ..fields()
            },
            Vec::new(),
        )
        .unwrap();
        assert_eq!(mail.text_body, body.to_plain_text().as_str());
        assert_eq!(
            mail.text_body,
            "Thanks, bold & the plan <https://example.org/plan>\n- bulbs\n> Bring gloves.\n"
        );
        let html = mail.html_body.unwrap_or_default();
        assert!(html.starts_with("<html><body>") && html.ends_with("</body></html>"), "{html}");
        assert!(
            html.contains("<div>Thanks, <b>bold</b> &amp; <a href=\"https://example.org/plan\">the plan</a></div>"),
            "{html}"
        );
        assert!(
            html.contains("<blockquote type=\"cite\"><div>Bring gloves.</div></blockquote>"),
            "{html}"
        );
    }

    #[test]
    fn a_reopened_draft_keeps_its_bold_its_link_and_its_quote() {
        // A draft came back from its text/plain part and lost its formats
        // (DEDUP_EDITORS F3).
        let body = formatted_body();
        let mail = draft_mail(
            &ComposeFields {
                body: body.clone(),
                ..fields()
            },
            Vec::new(),
        );
        let bytes = draft_bytes(&mail, 1_790_757_720);
        let view = crate::message::parse_view(&bytes).unwrap();
        assert_eq!(draft_body(&view), body);
        let plain_only = MessageView {
            html: None,
            ..view
        };
        assert_eq!(
            runs(&draft_body(&plain_only))[0].1,
            "Thanks, bold & the plan <https://example.org/plan>",
            "without an HTML part: the text"
        );
    }

    fn fields() -> ComposeFields {
        ComposeFields {
            from: String::from("Ada <ada@example.org>"),
            to: String::from("Ben <ben@example.org>, cleo@example.org"),
            cc: String::new(),
            bcc: String::from("dan@example.org"),
            subject: String::from("Re: Garden plan"),
            body: RichTextDoc::create_from_plain_text("See you.", 0),
            in_reply_to: Some(String::from("garden-1@example.org")),
            references: vec![String::from("garden-1@example.org")],
        }
    }

    #[test]
    fn the_outgoing_mail_has_every_address_split_and_both_parts() {
        let mail = outgoing(&fields(), Vec::new()).unwrap();
        assert_eq!(mail.from, "Ada <ada@example.org>");
        assert_eq!(mail.to, vec![String::from("Ben <ben@example.org>"), String::from("cleo@example.org")]);
        assert!(mail.cc.is_empty());
        assert_eq!(mail.bcc, vec![String::from("dan@example.org")]);
        assert_eq!(mail.subject, "Re: Garden plan");
        assert_eq!(mail.text_body, "See you.\n");
        assert!(mail.html_body.as_deref().unwrap_or("").contains("<div>See you.</div>"));
        assert_eq!(mail.in_reply_to.as_deref(), Some("garden-1@example.org"));
        assert_eq!(mail.references, vec![String::from("garden-1@example.org")]);
    }

    #[test]
    fn a_mail_without_recipients_or_with_a_bad_address_is_refused() {
        let none = ComposeFields {
            to: String::from(" , "),
            bcc: String::new(),
            ..fields()
        };
        assert_eq!(outgoing(&none, Vec::new()).unwrap_err(), ComposeError::NoRecipient);
        let bad = ComposeFields {
            cc: String::from("Ben Okafor"),
            ..fields()
        };
        assert_eq!(
            outgoing(&bad, Vec::new()).unwrap_err(),
            ComposeError::BadAddress(String::from("Ben Okafor"))
        );
        let no_from = ComposeFields {
            from: String::new(),
            ..fields()
        };
        assert_eq!(outgoing(&no_from, Vec::new()).unwrap_err(), ComposeError::NoSender);
    }

    #[test]
    fn a_draft_is_saved_as_typed_even_without_recipients() {
        let half = ComposeFields {
            to: String::from("Ben Okaf"),
            cc: String::new(),
            bcc: String::new(),
            ..fields()
        };
        let mail = draft_mail(&half, Vec::new());
        assert_eq!(mail.to, vec![String::from("Ben Okaf")]);
        assert!(mail.cc.is_empty() && mail.bcc.is_empty());
        assert_eq!(mail.subject, "Re: Garden plan");
        assert_eq!(mail.text_body, "See you.\n");
        assert_eq!(mail.in_reply_to.as_deref(), Some("garden-1@example.org"));
        let full = outgoing(&fields(), Vec::new()).unwrap();
        assert_eq!(draft_mail(&fields(), Vec::new()), full, "a complete draft is the mail itself");
    }

    #[test]
    fn attachments_get_their_type_from_the_extension() {
        assert_eq!(mime_type_for("plan.PDF"), "application/pdf");
        assert_eq!(mime_type_for("photo.jpeg"), "image/jpeg");
        assert_eq!(mime_type_for("notes.txt"), "text/plain");
        assert_eq!(mime_type_for("sheet.xlsx"), "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet");
        assert_eq!(mime_type_for("archive"), "application/octet-stream");
        assert_eq!(mime_type_for("x.weird"), "application/octet-stream");
    }

    #[test]
    fn a_draft_is_sends_message_with_its_bcc_and_mail_parser_reads_it_back() {
        let attachment = Attachment {
            file_name: String::from("plan.txt"),
            mime_type: String::from("text/plain"),
            bytes: b"tulips\n".to_vec(),
        };
        let mail = outgoing(&fields(), vec![attachment]).unwrap();
        let bytes = draft_bytes(&mail, 1_790_757_720);
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.starts_with("Bcc: dan@example.org\r\nX-AzMail-Draft: 1\r\n"), "{text}");
        let view = crate::message::parse_view(&bytes).unwrap();
        assert_eq!(view.subject, "Re: Garden plan");
        assert_eq!(view.from, "Ada <ada@example.org>");
        assert_eq!(view.to, "Ben <ben@example.org>, cleo@example.org");
        assert_eq!(view.date, "2026-09-30T08:42:00Z");
        assert_eq!(view.text.trim(), "See you.");
        assert!(view.html.as_deref().unwrap_or("").contains("See you."));
        assert_eq!(view.attachments.len(), 1);
        assert_eq!(view.attachments[0].name, "plan.txt");
        assert!(!view.message_id.is_empty());
        let no_bcc = OutgoingMail {
            bcc: Vec::new(),
            ..mail
        };
        let text = String::from_utf8(draft_bytes(&no_bcc, 1_790_757_720)).unwrap();
        assert!(text.starts_with("X-AzMail-Draft: 1\r\n"), "{text}");
    }

    #[test]
    fn a_saved_draft_is_filed_like_synced_mail_and_replaced_when_saved_again() {
        let dir = TempDir::new("drafts");
        let store = LocalFolder::new(dir.0.clone());
        let mail = outgoing(&fields(), Vec::new()).unwrap();
        let bytes = draft_bytes(&mail, 1_790_757_720);
        let first = save_draft(&dir.0, None, &bytes, 1_790_757_720).unwrap();
        let floor = crate::send::LOCAL_UID_FLOOR;
        assert_eq!(first.uid, floor, "a local UID, far above any server's");
        assert_eq!(first.path, format!("mail/drafts/2026/09/{floor}.eml"));
        assert_eq!(first.subject, "Re: Garden plan");
        assert!(first.flags.iter().any(|f| f == "\\Draft"), "{:?}", first.flags);
        assert_eq!(store.get(&first.path).unwrap(), bytes);
        assert_eq!(store.folders(), vec![String::from("drafts")], "the window lists the folder");
        let state = FolderState::from_json(
            &String::from_utf8(store.get(&store::state_key(DRAFTS_FOLDER)).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(state.uidvalidity, 0, "local only until a sync adopts it");
        let again = save_draft(&dir.0, Some(first.uid), &bytes, 1_790_757_720).unwrap();
        let index = store::index_from_jsonl(
            &String::from_utf8(store.get(&store::index_key(DRAFTS_FOLDER)).unwrap()).unwrap(),
        );
        assert_eq!(index, vec![again.clone()], "one line per draft");
        delete_draft(&store, again.uid).unwrap();
        assert!(store.get(&again.path).is_err());
        let index = store::index_from_jsonl(
            &String::from_utf8(store.get(&store::index_key(DRAFTS_FOLDER)).unwrap()).unwrap(),
        );
        assert!(index.is_empty());
    }
}
