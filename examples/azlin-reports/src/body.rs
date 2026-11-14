//! The report body: what the reporter's app seals to the operator's report key and AzCtl opens.
//!
//! Implements planning/architecture.md section 3.9 ("What a report carries: the full link with its key,
//! attached automatically, because the reporter's app has it. Azlin can then open exactly what was reported and
//! nothing else. A report from a chat carries the reported records with their authors' signatures (5.13), so
//! their sender is proven"; "A reason from a fixed list (illegal content, CSAM, terrorism, malware, phishing,
//! copyright, other), optional text and optional contact (never required for CSAM, Art. 16(2))") and section 5.1
//! principle 6 (every format is versioned and extensible: deterministic CBOR, unknown fields ignored,
//! must-understand fields listed in `crit`).
//!
//! The wire form is an `azlin_cbor` map with unsigned-integer keys (key 0 = `crit`): 1 version, 2 id, 3 reason,
//! 4 text, 5 contact, 6 subject (a map: 1 kind and the fields of the kind), 7 reply. EVERY map of the body keeps
//! the keys it does not know in an [`Extras`] (rule R18 of the W1 common rules: a copy keeps `crit` and the
//! unknown fields, so a re-encoded body is the original bytes), and an unknown `crit` key refuses the body.
//!
//! The link in a `Subject::Link` carries the link's key, so the body is key material (rule R17): `decode`
//! reads with `decode_secret` and copies the leaves out at once, `encode` builds its tree inside a
//! `SecretValue`. The copies this crate owns (`Text`) are not wiped: drift X-DELTA-W1S46A-10.
//!
//! Caller: S46-INT-4 (AzDrive's report dialog builds, validates, encodes and seals a body), S46-INT-3 (AzCtl
//! opens it with the operator's report key and decodes it).

use core::fmt;

use azlin_cbor::{
    decode_secret, DecodeError, EncodeError, IntMap, IntMapBuilder, Limits, MapError, SecretValue,
    ShapeError, Value,
};

use crate::ids::{AccountRef, LinkRef, ReportId};

/// The most bytes a report body encodes to (planning/architecture.md section 1.4 rule 4). Twenty chat records of
/// 64 KiB cannot fit it: drift X-DELTA-W1S46A-13.
pub const MAX_BODY: usize = 128 * 1024;
/// The most bytes of the free text of a report (and of the note of a `Subject::File`).
pub const MAX_TEXT: usize = 2_000;
/// The most bytes of the reporter's optional contact.
pub const MAX_CONTACT: usize = 200;
/// The most bytes of a link url.
pub const MAX_URL: usize = 2_048;
/// The most chat records one report carries.
pub const MAX_RECORDS: usize = 20;
/// The most bytes of one chat record (a `log_append` record: 64 KiB, section 2.5).
pub const MAX_RECORD: usize = 64 * 1024;
/// The most bytes of the reporter's reply handle (provisional: drift X-DELTA-W1S46A-11).
pub const MAX_REPLY: usize = 4_096;
/// The version of the wire form of the body.
pub const BODY_VERSION: u64 = 1;

/// What `decode` and `encode` work under. `max_text` is the whole body: a string cannot be longer than the
/// input, and the field limits are checked by this crate so that the error names the field.
const BODY_LIMITS: Limits = Limits {
    max_input: 128 * 1024,
    max_depth: 8,
    max_items: 1_024,
    max_total_items: 4_096,
    max_text: 128 * 1024,
};

const K_VERSION: u64 = 1;
const K_ID: u64 = 2;
const K_REASON: u64 = 3;
const K_TEXT: u64 = 4;
const K_CONTACT: u64 = 5;
const K_SUBJECT: u64 = 6;
const K_REPLY: u64 = 7;
const BODY_KEYS: &[u64] = &[
    K_VERSION, K_ID, K_REASON, K_TEXT, K_CONTACT, K_SUBJECT, K_REPLY,
];

const S_KIND: u64 = 1;
const S_LINK_URL: u64 = 2;
const S_CHAT_RECORDS: u64 = 2;
const S_MEMBER_ID: u64 = 2;
const S_MEMBER_ACCOUNT: u64 = 3;
const S_FILE_LINK: u64 = 2;
const S_FILE_NOTE: u64 = 3;
const KIND_LINK: u64 = 1;
const KIND_CHAT: u64 = 2;
const KIND_MEMBER: u64 = 3;
const KIND_FILE: u64 = 4;
const LINK_KEYS: &[u64] = &[S_KIND, S_LINK_URL];
const CHAT_KEYS: &[u64] = &[S_KIND, S_CHAT_RECORDS];
const MEMBER_KEYS: &[u64] = &[S_KIND, S_MEMBER_ID, S_MEMBER_ACCOUNT];
const FILE_KEYS: &[u64] = &[S_KIND, S_FILE_LINK, S_FILE_NOTE];

const R_RECORD: u64 = 1;
const R_AUTHOR: u64 = 2;
const R_SIG: u64 = 3;
const RECORD_KEYS: &[u64] = &[R_RECORD, R_AUTHOR, R_SIG];

/// A field of a report, named in the errors that refuse it.
///
/// Caller: S46-INT-4 (the dialog shows which field is too long), S46-INT-3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// The free text of the report (limit [`MAX_TEXT`]).
    Text,
    /// The reporter's optional contact (limit [`MAX_CONTACT`]).
    Contact,
    /// The url of a reported link (limit [`MAX_URL`]).
    Url,
    /// The note of a reported file (limit [`MAX_TEXT`]).
    Note,
    /// The reporter's reply handle (limit [`MAX_REPLY`]).
    Reply,
    /// A chat record (limit [`MAX_RECORD`]).
    Record,
}

impl Field {
    /// The most bytes this field may hold.
    pub const fn max(self) -> usize {
        match self {
            Field::Text | Field::Note => MAX_TEXT,
            Field::Contact => MAX_CONTACT,
            Field::Url => MAX_URL,
            Field::Reply => MAX_REPLY,
            Field::Record => MAX_RECORD,
        }
    }

    /// Whether `c` may not stand in this field: a control character (a text shown in a terminal could carry an
    /// escape sequence), an explicit bidirectional override (it can disguise what the operator reads), and in a
    /// url a space or a line break as well. A line break and a tab are fine in the free text.
    fn refuses(self, c: char) -> bool {
        let bidi = matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}');
        let control = c.is_control();
        match self {
            Field::Url => control || bidi || c == ' ',
            _ => (control && c != '\n' && c != '\t') || bidi,
        }
    }
}

/// The reason of a report and of a decision: a FIXED list, the codes are the wire values (planning/architecture.md
/// section 3.9: "A reason from a fixed list (illegal content, CSAM, terrorism, malware, phishing, copyright,
/// other)"). One list in one place (decision X-D-37: owned by this crate). A "threat to life" is not on the plan's
/// list: drift X-DELTA-W1S46A-3.
///
/// Caller: S46-INT-4 (the dialog's reason picker), S46-INT-3 (the Reports page, `Decision`), S46-INT-2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Reason {
    /// Illegal content (wire value 1).
    IllegalContent = 1,
    /// Child sexual abuse material (wire value 2): urgent.
    Csam = 2,
    /// Terrorist content (wire value 3): urgent.
    Terrorism = 3,
    /// Malware (wire value 4).
    Malware = 4,
    /// Phishing (wire value 5).
    Phishing = 5,
    /// Copyright (wire value 6).
    Copyright = 6,
    /// Anything else (wire value 7): a decision with this reason needs text.
    Other = 7,
}

impl Reason {
    /// Every reason, in wire order.
    pub const ALL: [Reason; 7] = [
        Reason::IllegalContent,
        Reason::Csam,
        Reason::Terrorism,
        Reason::Malware,
        Reason::Phishing,
        Reason::Copyright,
        Reason::Other,
    ];

    /// The wire value.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// The reason of a wire value: `UnknownReason` for any other number.
    pub fn from_code(code: u64) -> Result<Reason, BodyError> {
        Reason::ALL
            .into_iter()
            .find(|r| u64::from(r.code()) == code)
            .ok_or(BodyError::UnknownReason(code))
    }

    /// Whether a report with this reason heads the Reports page and the daily report and is due at once
    /// (planning/architecture.md section 3.9: "Reports for CSAM, terrorism or a threat to life head the Reports
    /// page and the daily report").
    pub const fn is_urgent(self) -> bool {
        matches!(self, Reason::Csam | Reason::Terrorism)
    }
}

/// A free text of a report: valid UTF-8 without control characters (a line break and a tab are allowed outside
/// a url). The limit of the field it stands in is checked by `ReportBody::validate`. Its `Debug` prints only the
/// length: the text may be the reporter's own words or a link with its key.
///
/// Caller: S46-INT-4 (the dialog), S46-INT-3 (the Reports page shows it), `Decision` and the notices.
#[derive(Clone, PartialEq, Eq)]
pub struct Text(String);

impl Text {
    /// A free text (limit [`MAX_TEXT`]); the same as `Text::for_field(s, Field::Text)`.
    pub fn new(s: &str) -> Result<Text, BodyError> {
        Text::for_field(s, Field::Text)
    }

    /// A text for `field`: `TooLong(field)` when it is longer than the field allows, `ControlChar(field)` for a
    /// character the field refuses.
    pub fn for_field(s: &str, field: Field) -> Result<Text, BodyError> {
        if s.len() > field.max() {
            return Err(BodyError::TooLong(field));
        }
        if s.chars().any(|c| field.refuses(c)) {
            return Err(BodyError::ControlChar(field));
        }
        Ok(Text(s.to_owned()))
    }

    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The length in bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the text is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Text(<{} bytes>)", self.0.len())
    }
}

/// Why a hand-built [`Extras`] cannot be written.
///
/// Caller: S46-INT-4 (through `BodyError::Extras`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExtrasError {
    /// `crit` is not a strictly ascending list of keys above 0.
    BadCrit,
    /// `crit` lists a key this crate does not know: no reader of this crate could understand the structure.
    CritUnknown(u64),
    /// A field key is 0, is a key this crate knows (the field would be written twice), or is not in strictly
    /// ascending order.
    Key(u64),
}

impl fmt::Display for ExtrasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtrasError::BadCrit => f.write_str("`crit` is not a strictly ascending list above 0"),
            ExtrasError::CritUnknown(k) => {
                write!(f, "`crit` names key {k}, which this reader does not know")
            }
            ExtrasError::Key(k) => write!(f, "extra key {k} is 0, known, or out of order"),
        }
    }
}

impl std::error::Error for ExtrasError {}

/// The keys of one map that this reader did not ask for, and its `crit` list, kept so that a copy of the
/// structure re-encodes to the same bytes (rule R18 of the W1 common rules; planning/architecture.md section 5.1
/// principle 6: "unknown fields ignored"). A writer that has nothing of the kind uses `Extras::default()`.
///
/// Caller: S46-INT-3 (a copy of a decoded body keeps them), S46-INT-4 (`Extras::default()` in every literal).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Extras {
    crit: Vec<u64>,
    fields: Vec<(u64, Value)>,
}

impl Extras {
    /// Extras with this `crit` list and these fields (both strictly ascending; checked when the body is
    /// validated or encoded, never trusted).
    pub fn new(crit: Vec<u64>, fields: Vec<(u64, Value)>) -> Extras {
        Extras { crit, fields }
    }

    /// Whether there is neither a `crit` list nor a field.
    pub fn is_empty(&self) -> bool {
        self.crit.is_empty() && self.fields.is_empty()
    }

    /// The keys the writer marked as must-understand.
    pub fn crit(&self) -> &[u64] {
        &self.crit
    }

    /// The unknown fields in key order.
    pub fn fields(&self) -> &[(u64, Value)] {
        &self.fields
    }

    /// Whether these extras can be written next to the fields `known` of their map.
    pub(crate) fn check(&self, known: &[u64]) -> Result<(), ExtrasError> {
        let mut previous: Option<u64> = None;
        for &k in &self.crit {
            if k == 0 || previous.is_some_and(|p| p >= k) {
                return Err(ExtrasError::BadCrit);
            }
            if !known.contains(&k) {
                return Err(ExtrasError::CritUnknown(k));
            }
            previous = Some(k);
        }
        let mut previous: Option<u64> = None;
        for (k, _) in &self.fields {
            if *k == 0 || known.contains(k) || previous.is_some_and(|p| p >= *k) {
                return Err(ExtrasError::Key(*k));
            }
            previous = Some(*k);
        }
        Ok(())
    }

    /// The builder with these fields and this `crit` list written into it.
    pub(crate) fn put_into(&self, mut builder: IntMapBuilder) -> IntMapBuilder {
        for (k, v) in &self.fields {
            builder = builder.put(*k, v.clone());
        }
        builder.crit(&self.crit)
    }
}

impl fmt::Debug for Extras {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Extras {{ crit: {}, fields: {} }}",
            self.crit.len(),
            self.fields.len()
        )
    }
}

/// Ends the reading of one map: applies `crit` (an unknown key in it refuses the map) and keeps the keys that
/// nobody asked for.
pub(crate) fn take_extras(map: IntMap<'_>) -> Result<Extras, ShapeError> {
    let crit = map.crit_keys()?;
    let unknown = map.finish()?;
    let mut fields = Vec::with_capacity(unknown.len());
    for (key, value) in unknown {
        let key = key.as_uint().ok_or(ShapeError::NotAnIntMap)?;
        fields.push((key, value.clone()));
    }
    Ok(Extras { crit, fields })
}

/// Why a body, a notice or a decision was refused. One variant per way, so a caller (and a test) can tell
/// them apart; `Copy` and without text.
///
/// Caller: S46-INT-3 (a body that cannot be opened is shown with its error), S46-INT-4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BodyError {
    /// The body or notice is longer than its limit ([`MAX_BODY`] for a body).
    TooLarge,
    /// The bytes are not the strict deterministic CBOR of `azlin-cbor`.
    Cbor(DecodeError),
    /// A map has the wrong shape: a missing key, a wrong type or length, an invalid or unknown `crit`.
    Shape(ShapeError),
    /// A chat record has the wrong shape (the index of the record).
    RecordShape {
        /// The position of the record in the list.
        index: usize,
        /// What is wrong with it.
        why: ShapeError,
    },
    /// The tree could not be written under the body's limits.
    Encode(EncodeError),
    /// The tree's map could not be built.
    Map(MapError),
    /// The extras of a map cannot be written.
    Extras(ExtrasError),
    /// The version is not [`BODY_VERSION`].
    Version(u64),
    /// The reason code is not on the fixed list.
    UnknownReason(u64),
    /// The subject kind is not one of the four.
    UnknownSubject(u64),
    /// A field is longer than its limit.
    TooLong(Field),
    /// An optional field is present and empty (the writer leaves the key out), or a note that is the only name
    /// of a file is empty.
    Empty(Field),
    /// A field holds a character it refuses.
    ControlChar(Field),
    /// The link url has no fragment: the link's key travels in it, and without it Azlin cannot open the link.
    UrlNoFragment,
    /// A chat report carries no record.
    NoRecords,
    /// A chat report carries more than [`MAX_RECORDS`] records.
    TooManyRecords,
    /// A chat record is empty (the index).
    EmptyRecord(usize),
    /// A chat record is longer than [`MAX_RECORD`] (the index).
    RecordTooLarge(usize),
    /// The signature of a chat record does not verify (the index of the first bad one).
    BadSignature(usize),
    /// The bytes decode, but re-encoding what they say gives other bytes: not the canonical form.
    NotCanonical,
    /// The sealing seam refused.
    Seal(SealError),
    /// A notice has another kind than the one asked for.
    WrongNotice(u64),
    /// The restriction code of a statement is not known.
    UnknownRestriction(u64),
    /// The kind of the facts of a statement is not known.
    UnknownFacts(u64),
    /// The ground code of a statement is not known.
    UnknownGround(u64),
    /// The redress word of a statement has no path or a bit that is not defined.
    BadRedress(u64),
    /// The outcome code of a notifier notice is not known.
    UnknownOutcome(u64),
}

impl fmt::Display for BodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BodyError::TooLarge => f.write_str("larger than the limit"),
            BodyError::Cbor(e) => write!(f, "not strict CBOR: {e}"),
            BodyError::Shape(e) => write!(f, "wrong shape: {e}"),
            BodyError::RecordShape { index, why } => write!(f, "chat record {index}: {why}"),
            BodyError::Encode(e) => write!(f, "cannot be written: {e}"),
            BodyError::Map(e) => write!(f, "cannot be built: {e}"),
            BodyError::Extras(e) => write!(f, "extras: {e}"),
            BodyError::Version(v) => write!(f, "version {v} is not understood"),
            BodyError::UnknownReason(c) => write!(f, "reason code {c} is not on the list"),
            BodyError::UnknownSubject(c) => write!(f, "subject kind {c} is not known"),
            BodyError::TooLong(field) => {
                write!(f, "{field:?} is longer than {} bytes", field.max())
            }
            BodyError::Empty(field) => write!(f, "{field:?} is empty"),
            BodyError::ControlChar(field) => write!(f, "{field:?} holds a character it refuses"),
            BodyError::UrlNoFragment => f.write_str("the link url has no fragment (no key)"),
            BodyError::NoRecords => f.write_str("a chat report carries no record"),
            BodyError::TooManyRecords => write!(f, "more than {MAX_RECORDS} chat records"),
            BodyError::EmptyRecord(i) => write!(f, "chat record {i} is empty"),
            BodyError::RecordTooLarge(i) => {
                write!(f, "chat record {i} is larger than {MAX_RECORD} bytes")
            }
            BodyError::BadSignature(i) => {
                write!(f, "the signature of chat record {i} does not verify")
            }
            BodyError::NotCanonical => f.write_str("not the canonical encoding of what it says"),
            BodyError::Seal(e) => write!(f, "sealing: {e}"),
            BodyError::WrongNotice(k) => write!(f, "notice kind {k} is not the one asked for"),
            BodyError::UnknownRestriction(c) => write!(f, "restriction code {c} is not known"),
            BodyError::UnknownFacts(c) => write!(f, "facts kind {c} is not known"),
            BodyError::UnknownGround(c) => write!(f, "ground code {c} is not known"),
            BodyError::BadRedress(c) => write!(f, "redress word {c} is not valid"),
            BodyError::UnknownOutcome(c) => write!(f, "outcome code {c} is not known"),
        }
    }
}

impl std::error::Error for BodyError {}

impl From<DecodeError> for BodyError {
    fn from(e: DecodeError) -> BodyError {
        BodyError::Cbor(e)
    }
}

impl From<ShapeError> for BodyError {
    fn from(e: ShapeError) -> BodyError {
        BodyError::Shape(e)
    }
}

impl From<MapError> for BodyError {
    fn from(e: MapError) -> BodyError {
        BodyError::Map(e)
    }
}

impl From<ExtrasError> for BodyError {
    fn from(e: ExtrasError) -> BodyError {
        BodyError::Extras(e)
    }
}

impl From<EncodeError> for BodyError {
    fn from(e: EncodeError) -> BodyError {
        match e {
            EncodeError::InputTooLarge => BodyError::TooLarge,
            other => BodyError::Encode(other),
        }
    }
}

/// Why the sealing seam refused.
///
/// Seam: S46-INT-4 (the AZD2 adapter that wraps `azlin-requests::Sealer` maps its own errors to these).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SealError {
    /// The operator's report key is not known to this app (no recipient to seal to).
    NoOperatorKey,
    /// The plaintext is larger than a drop may be.
    TooLarge,
    /// The sealing failed.
    Failed,
}

impl fmt::Display for SealError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SealError::NoOperatorKey => "the operator's report key is not known",
            SealError::TooLarge => "the plaintext is too large to seal",
            SealError::Failed => "sealing failed",
        })
    }
}

impl std::error::Error for SealError {}

/// Sealing a plaintext body to the operator's report key (planning/architecture.md section 3.9: "the app seals
/// each report to the operator's report key and drops it into the operator's 'reports' space through a request
/// (AZD2, 5.12)"). This crate never seals: the envelope is AZD2's.
///
/// Seam: first caller is S46-INT-4. `azlin-requests::Sealer` (lane S16f) is the lower-level trait that the
/// AZD2-WIRE adapter wraps (answer P-S46a-1 / C-16 of the reconciler).
pub trait ReportSealer {
    /// The sealed form of `plaintext`, for the operator's report key.
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, SealError>;
}

/// One reported chat record with its author's signature (planning/architecture.md section 5.13: "A report from a
/// chat carries the reported records with their authors' signatures, so their sender is proven").
///
/// Caller: S46-INT-4 (the dialog copies the records of the message), S46-INT-3 (`ReportBody::verify_authors`).
#[derive(Clone, PartialEq, Eq)]
pub struct SignedRecord {
    /// The record, as it stands in the chat's log (at most [`MAX_RECORD`] bytes).
    pub record: Vec<u8>,
    /// The author's public key.
    pub author: [u8; 32],
    /// The author's signature over the record.
    pub sig: [u8; 64],
    /// Keys of a newer writer (R18).
    pub extras: Extras,
}

impl fmt::Debug for SignedRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SignedRecord {{ record: <{} bytes>, .. }}",
            self.record.len()
        )
    }
}

impl SignedRecord {
    fn read(item: &Value, index: usize) -> Result<SignedRecord, BodyError> {
        let wrap = |why: ShapeError| BodyError::RecordShape { index, why };
        let mut map = IntMap::new(item).map_err(wrap)?;
        let record = map.req_bytes(R_RECORD).map_err(wrap)?.to_vec();
        let author = map.req_bytes_n::<32>(R_AUTHOR).map_err(wrap)?;
        let sig = map.req_bytes_n::<64>(R_SIG).map_err(wrap)?;
        let extras = take_extras(map).map_err(wrap)?;
        Ok(SignedRecord {
            record,
            author,
            sig,
            extras,
        })
    }

    fn to_value(&self) -> Result<Value, BodyError> {
        let builder = IntMapBuilder::new()
            .put(R_RECORD, Value::Bytes(self.record.clone()))
            .put(R_AUTHOR, Value::Bytes(self.author.to_vec()))
            .put(R_SIG, Value::Bytes(self.sig.to_vec()));
        Ok(self.extras.put_into(builder).build()?)
    }
}

/// Whether a chat record was signed by the author it names (drift X-DELTA-W1S46A-20; the log's signature rule: azlin-keys' `verify`
/// over `domain || 0x00 || bytes`, section 5.13).
///
/// Seam: first caller is S46-INT-3 (AzCtl opens a chat report and proves the senders before it decides). The
/// implementation over `azlin_keys` is that unit's.
pub trait RecordVerifier {
    /// Whether `record.sig` is `record.author`'s signature over `record.record`.
    fn verifies(&self, record: &SignedRecord) -> bool;
}

/// What a report is about.
///
/// Caller: S46-INT-4 (the dialog builds one), S46-INT-3 (`Decision::validate` reads it).
#[derive(Clone, PartialEq, Eq)]
pub enum Subject {
    /// A link, WITH its key: the full url `https://drive.azlin.io/l/<link id>#<share key>` (planning/architecture.md
    /// section 3.9: "attached automatically": the only way Azlin can open what was reported).
    Link {
        /// The full url, fragment included (at most [`MAX_URL`] bytes).
        url: Text,
        /// Keys of a newer writer (R18).
        extras: Extras,
    },
    /// Records of a chat, with their authors' signatures.
    ChatRecords {
        /// One to [`MAX_RECORDS`] records.
        records: Vec<SignedRecord>,
        /// Keys of a newer writer (R18).
        extras: Extras,
    },
    /// A member (an AzChat or AzDrive member who sends abuse).
    Member {
        /// The member's id.
        member: [u8; 16],
        /// The account the member belongs to.
        account: AccountRef,
        /// Keys of a newer writer (R18).
        extras: Extras,
    },
    /// A file, by its link when it has one.
    File {
        /// The link the file was shared by.
        link: Option<LinkRef>,
        /// The reporter's words about the file (at most [`MAX_TEXT`] bytes; not empty when there is no link).
        note: Text,
        /// Keys of a newer writer (R18).
        extras: Extras,
    },
}

impl fmt::Debug for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Subject::Link { .. } => f.write_str("Subject::Link { .. }"),
            Subject::ChatRecords { records, .. } => {
                write!(f, "Subject::ChatRecords {{ records: {} }}", records.len())
            }
            Subject::Member { .. } => f.write_str("Subject::Member { .. }"),
            Subject::File { link, .. } => {
                write!(f, "Subject::File {{ linked: {} }}", link.is_some())
            }
        }
    }
}

impl Subject {
    /// The rules of the subject (planning/architecture.md section 3.9, section 1.4 rule 4).
    fn validate(&self) -> Result<(), BodyError> {
        match self {
            Subject::Link { url, extras } => {
                validate_url(url)?;
                extras.check(LINK_KEYS)?;
            }
            Subject::ChatRecords { records, extras } => {
                if records.is_empty() {
                    return Err(BodyError::NoRecords);
                }
                if records.len() > MAX_RECORDS {
                    return Err(BodyError::TooManyRecords);
                }
                for (index, record) in records.iter().enumerate() {
                    if record.record.is_empty() {
                        return Err(BodyError::EmptyRecord(index));
                    }
                    if record.record.len() > MAX_RECORD {
                        return Err(BodyError::RecordTooLarge(index));
                    }
                    record.extras.check(RECORD_KEYS)?;
                }
                extras.check(CHAT_KEYS)?;
            }
            Subject::Member { extras, .. } => extras.check(MEMBER_KEYS)?,
            Subject::File { link, note, extras } => {
                if note.len() > Field::Note.max() {
                    return Err(BodyError::TooLong(Field::Note));
                }
                if link.is_none() && note.is_empty() {
                    return Err(BodyError::Empty(Field::Note));
                }
                extras.check(FILE_KEYS)?;
            }
        }
        Ok(())
    }

    fn read(value: &Value) -> Result<Subject, BodyError> {
        let mut map = IntMap::new(value)?;
        match map.req_u64(S_KIND)? {
            KIND_LINK => {
                let url = Text::for_field(map.req_text(S_LINK_URL)?, Field::Url)?;
                Ok(Subject::Link {
                    url,
                    extras: take_extras(map)?,
                })
            }
            KIND_CHAT => {
                let items = map.req_array(S_CHAT_RECORDS)?;
                if items.len() > MAX_RECORDS {
                    return Err(BodyError::TooManyRecords);
                }
                let mut records = Vec::with_capacity(items.len());
                for (index, item) in items.iter().enumerate() {
                    records.push(SignedRecord::read(item, index)?);
                }
                Ok(Subject::ChatRecords {
                    records,
                    extras: take_extras(map)?,
                })
            }
            KIND_MEMBER => {
                let member = map.req_bytes_n::<16>(S_MEMBER_ID)?;
                let account = AccountRef::from_bytes(map.req_bytes_n::<16>(S_MEMBER_ACCOUNT)?);
                Ok(Subject::Member {
                    member,
                    account,
                    extras: take_extras(map)?,
                })
            }
            KIND_FILE => {
                let link = map.opt_bytes_n::<16>(S_FILE_LINK)?.map(LinkRef::from_bytes);
                let note = Text::for_field(map.req_text(S_FILE_NOTE)?, Field::Note)?;
                Ok(Subject::File {
                    link,
                    note,
                    extras: take_extras(map)?,
                })
            }
            other => Err(BodyError::UnknownSubject(other)),
        }
    }

    fn to_value(&self) -> Result<Value, BodyError> {
        let builder = IntMapBuilder::new();
        let builder = match self {
            Subject::Link { url, extras } => extras.put_into(
                builder
                    .put(S_KIND, Value::Uint(KIND_LINK))
                    .put(S_LINK_URL, Value::Text(url.as_str().to_owned())),
            ),
            Subject::ChatRecords { records, extras } => {
                let items = records
                    .iter()
                    .map(SignedRecord::to_value)
                    .collect::<Result<Vec<Value>, BodyError>>()?;
                extras.put_into(
                    builder
                        .put(S_KIND, Value::Uint(KIND_CHAT))
                        .put(S_CHAT_RECORDS, Value::Array(items)),
                )
            }
            Subject::Member {
                member,
                account,
                extras,
            } => extras.put_into(
                builder
                    .put(S_KIND, Value::Uint(KIND_MEMBER))
                    .put(S_MEMBER_ID, Value::Bytes(member.to_vec()))
                    .put(S_MEMBER_ACCOUNT, Value::Bytes(account.as_bytes().to_vec())),
            ),
            Subject::File { link, note, extras } => {
                let mut builder = builder.put(S_KIND, Value::Uint(KIND_FILE));
                if let Some(link) = link {
                    builder = builder.put(S_FILE_LINK, Value::Bytes(link.as_bytes().to_vec()));
                }
                extras.put_into(builder.put(S_FILE_NOTE, Value::Text(note.as_str().to_owned())))
            }
        };
        Ok(builder.build()?)
    }
}

/// A link url must carry its key: a non-empty fragment (planning/architecture.md section 3.9: "the full link with
/// its key"; section 5.4: the key is in the fragment). It may not hold what the url field refuses.
fn validate_url(url: &Text) -> Result<(), BodyError> {
    let s = url.as_str();
    if s.is_empty() {
        return Err(BodyError::Empty(Field::Url));
    }
    if s.len() > MAX_URL {
        return Err(BodyError::TooLong(Field::Url));
    }
    if s.chars().any(|c| Field::Url.refuses(c)) {
        return Err(BodyError::ControlChar(Field::Url));
    }
    match s.split_once('#') {
        Some((_, fragment)) if !fragment.is_empty() => Ok(()),
        _ => Err(BodyError::UrlNoFragment),
    }
}

/// An optional text: absent, or present, not empty, and within the limit of its field.
fn validate_optional(text: Option<&Text>, field: Field) -> Result<(), BodyError> {
    match text {
        None => Ok(()),
        Some(t) if t.is_empty() => Err(BodyError::Empty(field)),
        Some(t) if t.len() > field.max() => Err(BodyError::TooLong(field)),
        Some(_) => Ok(()),
    }
}

/// What the app seals and AzCtl opens: one report.
///
/// The `Debug` form prints the receipt, the reason and the kind of the subject: no url (it holds a key), no
/// contact, no text, no record.
///
/// Caller: S46-INT-4 (AzDrive's report dialog), S46-INT-3 (AzCtl).
#[derive(Clone, PartialEq, Eq)]
pub struct ReportBody {
    /// The receipt number the reporter's app made (Art. 16(4) DSA).
    pub id: ReportId,
    /// Why the reporter reports.
    pub reason: Reason,
    /// The reporter's words (optional; at most [`MAX_TEXT`] bytes).
    pub text: Option<Text>,
    /// The reporter's own contact, if they want an answer outside the app (optional for every reason, never
    /// required: Art. 16(2) DSA; at most [`MAX_CONTACT`] bytes).
    pub contact: Option<Text>,
    /// What is reported.
    pub subject: Subject,
    /// The reporter's inbox request handle, opaque (lane S16): where the notifier notice and the decision are
    /// dropped. At most [`MAX_REPLY`] bytes.
    pub reply: Option<Vec<u8>>,
    /// Keys of a newer writer (R18).
    pub extras: Extras,
}

impl fmt::Debug for ReportBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReportBody")
            .field("id", &self.id)
            .field("reason", &self.reason)
            .field("subject", &self.subject)
            .field("has_text", &self.text.is_some())
            .field("has_contact", &self.contact.is_some())
            .field("has_reply", &self.reply.is_some())
            .field("extras", &self.extras)
            .finish()
    }
}

impl ReportBody {
    /// The rules of a body: the limits of every field, the link's fragment, the chat records' count and size, the
    /// extras. The size of the whole encoding is checked by `encode` ([`MAX_BODY`]); the signatures of chat
    /// records by `verify_authors` (they need a verifier).
    ///
    /// Caller: S46-INT-4 (before sealing), S46-INT-3.
    pub fn validate(&self) -> Result<(), BodyError> {
        validate_optional(self.text.as_ref(), Field::Text)?;
        // Art. 16(2) DSA: the contact is optional for every reason, the CSAM reports included.
        validate_optional(self.contact.as_ref(), Field::Contact)?;
        if let Some(reply) = &self.reply {
            if reply.is_empty() {
                return Err(BodyError::Empty(Field::Reply));
            }
            if reply.len() > MAX_REPLY {
                return Err(BodyError::TooLong(Field::Reply));
            }
        }
        self.subject.validate()?;
        self.extras.check(BODY_KEYS)?;
        Ok(())
    }

    /// The one deterministic encoding of the body (validated first). The tree that holds the link's key is
    /// wiped when it drops (rule R17); the returned bytes are the caller's to seal and wipe.
    ///
    /// Caller: S46-INT-4 (through `seal`), S46-INT-3 (a copy that keeps its unknown fields).
    pub fn encode(&self) -> Result<Vec<u8>, BodyError> {
        self.validate()?;
        let tree = SecretValue::new(self.to_value()?);
        Ok(azlin_cbor::encode(tree.as_value(), &BODY_LIMITS)?)
    }

    /// The body of `bytes`, which must be the strict canonical encoding of a valid body: shortest integers,
    /// sorted unique keys, no float, no indefinite length, nothing after the item; version 1; every key this
    /// reader needs; no unknown `crit` key. Unknown keys that are not in `crit` are ignored and KEPT in the
    /// extras of their map. The decoded tree is wiped before this returns (rule R17).
    ///
    /// Caller: S46-INT-3 (AzCtl opens a sealed report).
    pub fn decode(bytes: &[u8]) -> Result<ReportBody, BodyError> {
        if bytes.len() > MAX_BODY {
            return Err(BodyError::TooLarge);
        }
        let tree = decode_secret(bytes, &BODY_LIMITS)?;
        let mut map = IntMap::new(tree.as_value())?;
        let version = map.req_u64(K_VERSION)?;
        if version != BODY_VERSION {
            return Err(BodyError::Version(version));
        }
        let id = ReportId::from_bytes(map.req_bytes_n::<16>(K_ID)?);
        let reason = Reason::from_code(map.req_u64(K_REASON)?)?;
        let text = map
            .opt_text(K_TEXT)?
            .map(|s| Text::for_field(s, Field::Text))
            .transpose()?;
        let contact = map
            .opt_text(K_CONTACT)?
            .map(|s| Text::for_field(s, Field::Contact))
            .transpose()?;
        let subject = Subject::read(map.req_value(K_SUBJECT)?)?;
        let reply = map.opt_bytes(K_REPLY)?.map(<[u8]>::to_vec);
        let extras = take_extras(map)?;
        let body = ReportBody {
            id,
            reason,
            text,
            contact,
            subject,
            reply,
            extras,
        };
        body.validate()?;
        // The signature rule: what was accepted is exactly what this crate would write.
        if body.encode()? != bytes {
            return Err(BodyError::NotCanonical);
        }
        Ok(body)
    }

    /// Validates, encodes and seals the body for the operator (drift X-DELTA-W1S46A-21; planning/architecture.md section 3.9: "the app
    /// seals each report to the operator's report key"). The plaintext, which holds the link's key, is wiped
    /// after the sealer has read it.
    ///
    /// Caller: S46-INT-4 (the report dialog), with the AZD2 adapter as the sealer.
    pub fn seal(&self, sealer: &dyn ReportSealer) -> Result<Vec<u8>, BodyError> {
        let plain = SecretValue::new(Value::Bytes(self.encode()?));
        let bytes = plain
            .as_value()
            .as_bytes()
            .ok_or(BodyError::Seal(SealError::Failed))?;
        sealer.seal(bytes).map_err(BodyError::Seal)
    }

    /// Whether every chat record was signed by its author: the first one that does not verify fails the whole
    /// body with its index (planning/architecture.md section 3.9, 5.13: "so their sender is proven"). A body of
    /// another subject has no record to verify.
    ///
    /// Caller: S46-INT-3 (AzCtl proves the senders before it decides).
    pub fn verify_authors(&self, verifier: &dyn RecordVerifier) -> Result<(), BodyError> {
        if let Subject::ChatRecords { records, .. } = &self.subject {
            for (index, record) in records.iter().enumerate() {
                if !verifier.verifies(record) {
                    return Err(BodyError::BadSignature(index));
                }
            }
        }
        Ok(())
    }

    fn to_value(&self) -> Result<Value, BodyError> {
        let mut builder = IntMapBuilder::new()
            .put(K_VERSION, Value::Uint(BODY_VERSION))
            .put(K_ID, Value::Bytes(self.id.as_bytes().to_vec()))
            .put(K_REASON, Value::Uint(u64::from(self.reason.code())));
        if let Some(text) = &self.text {
            builder = builder.put(K_TEXT, Value::Text(text.as_str().to_owned()));
        }
        if let Some(contact) = &self.contact {
            builder = builder.put(K_CONTACT, Value::Text(contact.as_str().to_owned()));
        }
        builder = builder.put(K_SUBJECT, self.subject.to_value()?);
        if let Some(reply) = &self.reply {
            builder = builder.put(K_REPLY, Value::Bytes(reply.clone()));
        }
        Ok(self.extras.put_into(builder).build()?)
    }
}
