//! Writing mail, as plain data: the body model the compose window edits, what New / Reply /
//! Reply All / Forward start from, the address lines, the mail handed to sending
//! (`send::OutgoingMail`), and a draft as an `.eml` file in the Drafts folder.
//!
//! The body is a [`MailDoc`]: a flat list of blocks, each with a quote depth, a kind (paragraph,
//! bullet, numbered item) and runs of text with bold / italic / underline / a link (the
//! exploration's design, `scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md` 5.3). The compose
//! window renders it into its editor and reads it back (`editor.rs`); here it becomes the two
//! parts of the mail: `text/plain` with `> ` quoting and `text/html` with the quotes nested as
//! `<blockquote type="cite">` - the form Gmail and Thunderbird write and read.
//!
//! A reply starts with an empty line for the caret, then "On <date>, <sender> wrote:" and the
//! original one quote level deeper; a forward with the original's header block and its text.
//!
//! A draft is a whole message file in `mail/drafts/<yyyy>/<mm>/<uid>.eml` with its index line,
//! the same layout as the synced mail. Its UID is a LOCAL one, counted down from `u32::MAX`
//! ([`next_local_uid`]): a server counts its UIDs up from 1 and never reaches them, so a later
//! sync of the server's Drafts folder files its messages beside the local ones.

use crate::{
    message::MessageView,
    send::{Attachment, OutgoingMail},
    store::{self, FolderState, IndexEntry, LocalFolder},
};

/// The folder drafts are saved in (the `\Drafts` folder's fixed key).
pub const DRAFTS_FOLDER: &str = "drafts";
/// The first local UID; local UIDs count down from here.
pub const LOCAL_UID_TOP: u32 = u32::MAX;
/// UIDs above this are local ones (a draft saved here), never a server's.
pub const LOCAL_UID_FLOOR: u32 = 0xF000_0000;

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
        todo!()
    }
}

/// The compose window's title, as Outlook writes it: "<subject> - Message (HTML)", "Untitled -
/// Message (HTML)" before there is a subject.
pub fn window_title(subject: &str) -> String {
    todo!()
}

/// `Re: <subject>`, unless the subject already starts with a reply prefix (Re, RE, AW, Aw, SV,
/// Antw - any case, with the colon).
pub fn reply_subject(subject: &str) -> String {
    todo!()
}

/// `Fwd: <subject>`, unless the subject already starts with a forward prefix (Fwd, FW, WG, TR,
/// Doorst - any case, with the colon).
pub fn forward_subject(subject: &str) -> String {
    todo!()
}

/// The entries of an address line: split at commas and semicolons that are not inside quotes
/// or angle brackets, trimmed, empty ones left out.
pub fn split_addresses(line: &str) -> Vec<String> {
    todo!()
}

/// The address of an entry: `ada@example.org` from `Ada <ada@example.org>`, `"L, Ada"
/// <ada@example.org>` or `ada@example.org`; `None` when there is no address in it.
pub fn bare_address(entry: &str) -> Option<String> {
    todo!()
}

/// Whether two entries name the same mailbox (their addresses, ignoring case).
pub fn same_address(a: &str, b: &str) -> bool {
    todo!()
}

/// The header fields a reply or forward starts with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartFields {
    pub to: String,
    pub cc: String,
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
    todo!()
}

/// A forward of `original`: no recipients, the subject, the thread.
pub fn forward_fields(original: &MessageView) -> StartFields {
    todo!()
}

/// A run of text in one style.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// The link's address (`https:`, `http:`, `mailto:`).
    pub link: Option<String>,
}

impl Run {
    pub fn plain(text: &str) -> Run {
        todo!()
    }
}

/// What a block is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BlockKind {
    #[default]
    Paragraph,
    Bullet,
    Numbered,
}

/// One block of the body: its quote depth (0 = the writer's own text), its kind, its runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Block {
    pub quote: u8,
    pub kind: BlockKind,
    pub runs: Vec<Run>,
}

impl Block {
    /// A paragraph of plain `text` at quote depth `quote`.
    pub fn paragraph(quote: u8, text: &str) -> Block {
        todo!()
    }

    /// The block's text, every run's text joined.
    pub fn text(&self) -> String {
        todo!()
    }
}

/// The body of a mail being written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MailDoc {
    pub blocks: Vec<Block>,
}

impl MailDoc {
    /// One empty paragraph: where the caret starts in a new mail.
    pub fn empty() -> MailDoc {
        todo!()
    }

    /// Plain text as paragraphs, one per line, `>` quotes as depth (plus `extra_quote`).
    pub fn from_plain(text: &str, extra_quote: u8) -> MailDoc {
        todo!()
    }

    /// A reply's body: an empty paragraph (the caret's), the quote header, and the original's
    /// text one level deeper.
    pub fn reply_quote(original: &MessageView, header: &str) -> MailDoc {
        todo!()
    }

    /// A forward's body: an empty paragraph, the forwarded header block (From, Date, Subject,
    /// To, Cc), an empty line and the original's text.
    pub fn forward_quote(original: &MessageView, date: &str) -> MailDoc {
        todo!()
    }

    /// Whether there is no text at all.
    pub fn is_blank(&self) -> bool {
        todo!()
    }

    /// The `text/plain` part: a line per block, `> ` per quote level, `- ` before a bullet,
    /// `1. ` (counting) before a numbered item, a link as `text <address>` (just the address
    /// when the text is the address). Lines end in `\n`.
    pub fn to_plain(&self) -> String {
        todo!()
    }

    /// The `text/html` part: `<html><body>` with a `<div>` per paragraph (`<div><br></div>`
    /// for an empty one), bullets and numbered items in `<ul>` / `<ol>`, quote levels nested as
    /// `<blockquote type="cite">`, runs as `<b>` `<i>` `<u>` `<a href>`; text escaped.
    pub fn to_html(&self) -> String {
        todo!()
    }
}

/// The line before a reply's quote: `On Wed, 30 Sep 2026 at 10:42, Ada <ada@example.org>
/// wrote:` (`date` as the reader's zone shows it; without a date: `<sender> wrote:`).
pub fn quote_header(date: &str, from: &str) -> String {
    todo!()
}

/// An RFC 3339 date as the quote header writes it in `tz`: `Wed, 30 Sep 2026 at 10:42`; empty
/// when it is not a date.
pub fn header_date_in<Tz: chrono::TimeZone>(rfc3339: &str, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    todo!()
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComposeFields {
    /// The account's address (`Name <address>` or `address`).
    pub from: String,
    /// The address lines as typed.
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub body: MailDoc,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
}

/// The mail to send: every address line split and checked, the body as text and HTML.
pub fn outgoing(fields: &ComposeFields, attachments: Vec<Attachment>) -> Result<OutgoingMail, ComposeError> {
    todo!()
}

/// The media type of a file by its extension (`application/octet-stream` for anything unknown).
pub fn mime_type_for(file_name: &str) -> &'static str {
    todo!()
}

/// The next local UID for a draft in a folder whose index holds `existing`: one below the
/// lowest local UID there, `LOCAL_UID_TOP` for the first.
pub fn next_local_uid(existing: &[IndexEntry]) -> u32 {
    todo!()
}

/// Whether `uid` is a local one (a draft saved here).
pub fn is_local_uid(uid: u32) -> bool {
    todo!()
}

/// A Message-ID for a mail written here, bare (no angle brackets):
/// `azmail.<secs>.<salt>@<the sender's domain>` (`localhost` without one).
pub fn new_message_id(from: &str, now_secs: i64, salt: u64) -> String {
    todo!()
}

/// `secs` since 1970 as an RFC 5322 date in UTC (`Wed, 30 Sep 2026 08:42:00 +0000`).
pub fn rfc5322_date(secs: i64) -> String {
    todo!()
}

/// A header value as RFC 2047 encoded-words when it is not plain ASCII (`=?UTF-8?B?..?=`).
pub fn encode_header_value(value: &str) -> String {
    todo!()
}

/// The draft as a message file: the headers (From, To, Cc, Bcc - a draft keeps its Bcc -,
/// Subject, Date, Message-ID, In-Reply-To, References, MIME-Version, `X-AzMail-Draft: 1`),
/// `multipart/alternative` of the text and the HTML (base64, UTF-8), inside `multipart/mixed`
/// with the attachments (base64) when there are any. Lines end in CRLF.
pub fn draft_eml(mail: &OutgoingMail, date_secs: i64, message_id: &str) -> Vec<u8> {
    todo!()
}

/// Saves a draft's message file under `uid` in the Drafts folder of `store` with its index
/// line (replacing the draft's earlier file and line), and makes the folder known to the
/// window (a state file that says it is local only, see `sync::plan_folder`). Returns the line.
pub fn save_draft(store: &LocalFolder, uid: u32, eml: &[u8], now_secs: i64) -> std::io::Result<IndexEntry> {
    todo!()
}

/// Removes a draft (once it is sent): its file and its index line.
pub fn delete_draft(store: &LocalFolder, uid: u32) -> std::io::Result<()> {
    todo!()
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
        }
    }

    fn runs(doc: &MailDoc) -> Vec<(u8, String)> {
        doc.blocks.iter().map(|b| (b.quote, b.text())).collect()
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
        let doc = MailDoc::reply_quote(&original(), &header);
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
        let doc = MailDoc::forward_quote(&original(), "Wed, 30 Sep 2026 at 10:42");
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

    fn sample_doc() -> MailDoc {
        MailDoc {
            blocks: vec![
                Block {
                    quote: 0,
                    kind: BlockKind::Paragraph,
                    runs: vec![
                        Run::plain("Thanks, "),
                        Run {
                            text: String::from("bold"),
                            bold: true,
                            ..Run::default()
                        },
                        Run::plain(" & "),
                        Run {
                            text: String::from("the list"),
                            link: Some(String::from("https://example.org/a?b=1&c=2")),
                            ..Run::default()
                        },
                    ],
                },
                Block {
                    quote: 0,
                    kind: BlockKind::Bullet,
                    runs: vec![Run::plain("bulbs")],
                },
                Block {
                    quote: 0,
                    kind: BlockKind::Bullet,
                    runs: vec![Run {
                        text: String::from("gloves"),
                        italic: true,
                        underline: true,
                        ..Run::default()
                    }],
                },
                Block::paragraph(0, ""),
                Block::paragraph(1, "Bring <gloves>."),
                Block::paragraph(2, "Deeper."),
                Block::paragraph(1, "Back up."),
                Block {
                    quote: 0,
                    kind: BlockKind::Numbered,
                    runs: vec![Run::plain("one")],
                },
                Block {
                    quote: 0,
                    kind: BlockKind::Numbered,
                    runs: vec![Run::plain("two")],
                },
            ],
        }
    }

    #[test]
    fn the_text_part_quotes_with_marks_and_writes_lists_and_links_readably() {
        assert_eq!(
            sample_doc().to_plain(),
            "Thanks, bold & the list <https://example.org/a?b=1&c=2>\n\
             - bulbs\n\
             - gloves\n\
             \n\
             > Bring <gloves>.\n\
             > > Deeper.\n\
             > Back up.\n\
             1. one\n\
             2. two\n"
        );
        let link_is_text = MailDoc {
            blocks: vec![Block {
                runs: vec![Run {
                    text: String::from("https://example.org"),
                    link: Some(String::from("https://example.org")),
                    ..Run::default()
                }],
                ..Block::default()
            }],
        };
        assert_eq!(link_is_text.to_plain(), "https://example.org\n");
    }

    #[test]
    fn the_html_part_nests_quotes_and_lists_and_escapes_text() {
        let html = sample_doc().to_html();
        assert!(html.starts_with("<html><body>"), "{html}");
        assert!(html.ends_with("</body></html>"), "{html}");
        assert!(
            html.contains(
                "<div>Thanks, <b>bold</b> &amp; <a href=\"https://example.org/a?b=1&amp;c=2\">the list</a></div>"
            ),
            "{html}"
        );
        assert!(html.contains("<ul><li>bulbs</li><li><i><u>gloves</u></i></li></ul>"), "{html}");
        assert!(html.contains("<div><br></div>"), "an empty line: {html}");
        assert!(
            html.contains(
                "<blockquote type=\"cite\"><div>Bring &lt;gloves&gt;.</div><blockquote type=\"cite\"><div>Deeper.</div></blockquote><div>Back up.</div></blockquote>"
            ),
            "{html}"
        );
        assert!(html.contains("<ol><li>one</li><li>two</li></ol>"), "{html}");
    }

    #[test]
    fn plain_text_becomes_paragraphs_with_quote_depths() {
        let doc = MailDoc::from_plain("Hi\n> quoted\n>> deeper\n", 1);
        assert_eq!(
            runs(&doc),
            vec![(1, String::from("Hi")), (2, String::from("quoted")), (3, String::from("deeper"))]
        );
        assert!(MailDoc::empty().is_blank());
        assert_eq!(MailDoc::empty().blocks.len(), 1);
        assert!(!doc.is_blank());
        assert_eq!(MailDoc::from_plain("", 0), MailDoc::empty(), "no text is one empty line");
    }

    fn fields() -> ComposeFields {
        ComposeFields {
            from: String::from("Ada <ada@example.org>"),
            to: String::from("Ben <ben@example.org>, cleo@example.org"),
            cc: String::new(),
            bcc: String::from("dan@example.org"),
            subject: String::from("Re: Garden plan"),
            body: MailDoc::from_plain("See you.", 0),
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
    fn attachments_get_their_type_from_the_extension() {
        assert_eq!(mime_type_for("plan.PDF"), "application/pdf");
        assert_eq!(mime_type_for("photo.jpeg"), "image/jpeg");
        assert_eq!(mime_type_for("notes.txt"), "text/plain");
        assert_eq!(mime_type_for("sheet.xlsx"), "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet");
        assert_eq!(mime_type_for("archive"), "application/octet-stream");
        assert_eq!(mime_type_for("x.weird"), "application/octet-stream");
    }

    #[test]
    fn local_uids_count_down_from_the_top_and_never_meet_the_servers() {
        let server = IndexEntry {
            uid: 41,
            ..IndexEntry::default()
        };
        assert_eq!(next_local_uid(&[server.clone()]), LOCAL_UID_TOP);
        let local = IndexEntry {
            uid: LOCAL_UID_TOP,
            ..IndexEntry::default()
        };
        assert_eq!(next_local_uid(&[server, local]), LOCAL_UID_TOP - 1);
        assert!(is_local_uid(LOCAL_UID_TOP) && !is_local_uid(41));
    }

    #[test]
    fn ids_dates_and_header_words_are_rfc_5322() {
        assert_eq!(new_message_id("Ada <ada@example.org>", 1_790_757_720, 7), "azmail.1790757720.7@example.org");
        assert_eq!(new_message_id("", 1, 2), "azmail.1.2@localhost");
        assert_eq!(rfc5322_date(1_790_757_720), "Wed, 30 Sep 2026 08:42:00 +0000");
        assert_eq!(encode_header_value("Garden plan"), "Garden plan");
        assert_eq!(encode_header_value("Grüße"), "=?UTF-8?B?R3LDvMOfZQ==?=");
    }

    #[test]
    fn a_draft_is_a_message_file_mail_parser_reads_back() {
        let attachment = Attachment {
            file_name: String::from("plan.txt"),
            mime_type: String::from("text/plain"),
            bytes: b"tulips\n".to_vec(),
        };
        let mail = outgoing(&fields(), vec![attachment]).unwrap();
        let eml = draft_eml(&mail, 1_790_757_720, "azmail.1.2@example.org");
        let text = String::from_utf8(eml.clone()).unwrap();
        assert!(text.contains("\r\nBcc: dan@example.org\r\n"), "a draft keeps its Bcc: {text}");
        assert!(text.contains("\r\nX-AzMail-Draft: 1\r\n"), "{text}");
        assert!(text.contains("\r\nIn-Reply-To: <garden-1@example.org>\r\n"), "{text}");
        assert!(text.contains("multipart/mixed") && text.contains("multipart/alternative"), "{text}");
        let view = crate::message::parse_view(&eml).unwrap();
        assert_eq!(view.subject, "Re: Garden plan");
        assert_eq!(view.from, "Ada <ada@example.org>");
        assert_eq!(view.to, "Ben <ben@example.org>, cleo@example.org");
        assert_eq!(view.date, "2026-09-30T08:42:00Z");
        assert_eq!(view.text.trim(), "See you.");
        assert!(view.html.as_deref().unwrap_or("").contains("See you."));
        assert_eq!(view.attachments.len(), 1);
        assert_eq!(view.attachments[0].name, "plan.txt");
        assert_eq!(view.message_id, "azmail.1.2@example.org");
    }

    #[test]
    fn a_saved_draft_is_filed_like_synced_mail_and_replaced_when_saved_again() {
        let dir = TempDir::new("drafts");
        let store = LocalFolder::new(dir.0.clone());
        let mail = outgoing(&fields(), Vec::new()).unwrap();
        let eml = draft_eml(&mail, 1_790_757_720, "azmail.1.2@example.org");
        let entry = save_draft(&store, LOCAL_UID_TOP, &eml, 1_790_757_720).unwrap();
        assert_eq!(entry.path, "mail/drafts/2026/09/4294967295.eml");
        assert_eq!(entry.subject, "Re: Garden plan");
        assert_eq!(store.get(&entry.path).unwrap(), eml);
        assert_eq!(store.folders(), vec![String::from("drafts")], "the window lists the folder");
        let state = FolderState::from_json(&String::from_utf8(store.get(&store::state_key("drafts")).unwrap()).unwrap()).unwrap();
        assert_eq!(state.uidvalidity, 0, "local only until a sync adopts it");
        let again = save_draft(&store, LOCAL_UID_TOP, &eml, 1_790_757_720).unwrap();
        let index = store::index_from_jsonl(&String::from_utf8(store.get(&store::index_key("drafts")).unwrap()).unwrap());
        assert_eq!(index, vec![again], "one line per draft");
        delete_draft(&store, LOCAL_UID_TOP).unwrap();
        assert!(store.get(&entry.path).is_err());
        let index = store::index_from_jsonl(&String::from_utf8(store.get(&store::index_key("drafts")).unwrap()).unwrap());
        assert!(index.is_empty());
    }
}
