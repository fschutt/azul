//! IMAP commands (RFC 3501's grammar, with the extensions the bridge offers): a whole command
//! - its lines and its literals, as [`super::session`] reads them - becomes a [`Command`]. The
//! parser is strict (anything outside the grammar is an error the session answers with BAD)
//! and never recurses without a bound (SEARCH's parentheses and NOT / OR nest at most
//! [`MAX_SEARCH_DEPTH`] deep).

use crate::dates;

/// SEARCH keys nest at most this deep.
pub const MAX_SEARCH_DEPTH: usize = 32;
/// A sequence set has at most this many ranges.
pub const MAX_SET_RANGES: usize = 10_000;

/// A number or `*` (the largest in use) in a sequence set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    Num(u32),
    Star,
}

/// `1:3,5,7:*`: ranges, each inclusive in either order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceSet(pub Vec<(Bound, Bound)>);

impl SequenceSet {
    /// Whether `n` is in the set when the largest number in use is `max`.
    #[must_use]
    pub fn contains(&self, n: u32, max: u32) -> bool {
        let value = |b: Bound| match b {
            Bound::Num(v) => v,
            Bound::Star => max,
        };
        self.0.iter().any(|(a, b)| {
            let (a, b) = (value(*a), value(*b));
            let (low, high) = if a <= b { (a, b) } else { (b, a) };
            (low..=high).contains(&n)
        })
    }

    /// Whether it names `*` (a UID set that does: the highest UID is in it, RFC 3501 6.4.8).
    #[must_use]
    pub fn has_star(&self) -> bool {
        self.0
            .iter()
            .any(|(a, b)| *a == Bound::Star || *b == Bound::Star)
    }
}

/// What a section's text part names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionText {
    Header,
    HeaderFields(Vec<String>),
    HeaderFieldsNot(Vec<String>),
    Text,
    Mime,
}

/// `BODY[<path>.<text>]`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Section {
    pub path: Vec<u32>,
    pub text: Option<SectionText>,
}

impl Section {
    /// The section as the response names it (`1.HEADER.FIELDS (FROM TO)`).
    #[must_use]
    pub fn spec(&self) -> String {
        let mut out = self
            .path
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".");
        if let Some(text) = &self.text {
            if !out.is_empty() {
                out.push('.');
            }
            match text {
                SectionText::Header => out.push_str("HEADER"),
                SectionText::Text => out.push_str("TEXT"),
                SectionText::Mime => out.push_str("MIME"),
                SectionText::HeaderFields(names) | SectionText::HeaderFieldsNot(names) => {
                    out.push_str(if matches!(text, SectionText::HeaderFields(_)) {
                        "HEADER.FIELDS ("
                    } else {
                        "HEADER.FIELDS.NOT ("
                    });
                    out.push_str(&names.join(" "));
                    out.push(')');
                }
            }
        }
        out
    }
}

/// One FETCH data item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchAtt {
    Flags,
    Uid,
    InternalDate,
    Rfc822Size,
    Envelope,
    /// `BODY` without a section: the non-extensible structure.
    Body,
    BodyStructure,
    /// `RFC822`: `BODY[]` named the old way (sets \Seen).
    Rfc822,
    /// `RFC822.HEADER`: `BODY.PEEK[HEADER]` named the old way.
    Rfc822Header,
    /// `RFC822.TEXT`: `BODY[TEXT]` named the old way (sets \Seen).
    Rfc822Text,
    /// `BODY[...]` / `BODY.PEEK[...]` with an optional `<start.length>`.
    Section {
        peek: bool,
        section: Section,
        partial: Option<(u64, u64)>,
    },
}

/// One SEARCH key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchKey {
    All,
    Answered,
    Unanswered,
    Deleted,
    Undeleted,
    Draft,
    Undraft,
    Flagged,
    Unflagged,
    Seen,
    Unseen,
    New,
    Old,
    Recent,
    Keyword(String),
    Unkeyword(String),
    Bcc(String),
    Cc(String),
    From(String),
    To(String),
    Subject(String),
    Body(String),
    Text(String),
    Header(String, String),
    /// Day numbers (since 1970-01-01) of the internal date...
    Before(i64),
    On(i64),
    Since(i64),
    /// ...and of the Date header.
    SentBefore(i64),
    SentOn(i64),
    SentSince(i64),
    Larger(u64),
    Smaller(u64),
    Uid(SequenceSet),
    Seq(SequenceSet),
    Not(Box<SearchKey>),
    Or(Box<SearchKey>, Box<SearchKey>),
    And(Vec<SearchKey>),
}

/// STORE's `FLAGS` / `+FLAGS` / `-FLAGS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreMode {
    Replace,
    Add,
    Remove,
}

/// STATUS's items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusItem {
    Messages,
    Recent,
    UidNext,
    UidValidity,
    Unseen,
}

/// What a command asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    Capability,
    Noop,
    Logout,
    Id,
    Namespace,
    Enable,
    StartTls,
    Login {
        user: Vec<u8>,
        password: Vec<u8>,
    },
    Authenticate {
        mechanism: String,
        initial: Option<String>,
    },
    Select(Vec<u8>),
    Examine(Vec<u8>),
    Create(Vec<u8>),
    Delete(Vec<u8>),
    Rename(Vec<u8>, Vec<u8>),
    Subscribe(Vec<u8>),
    Unsubscribe(Vec<u8>),
    List {
        reference: Vec<u8>,
        pattern: Vec<u8>,
        subscribed: bool,
    },
    Status {
        mailbox: Vec<u8>,
        items: Vec<StatusItem>,
    },
    Append {
        mailbox: Vec<u8>,
        flags: Vec<String>,
        date: Option<i64>,
        message: Vec<u8>,
    },
    Check,
    Close,
    Unselect,
    Expunge {
        uids: Option<SequenceSet>,
    },
    Search {
        uid: bool,
        key: SearchKey,
    },
    Fetch {
        uid: bool,
        set: SequenceSet,
        atts: Vec<FetchAtt>,
    },
    Store {
        uid: bool,
        set: SequenceSet,
        mode: StoreMode,
        silent: bool,
        flags: Vec<String>,
    },
    Copy {
        uid: bool,
        set: SequenceSet,
        mailbox: Vec<u8>,
    },
    Move {
        uid: bool,
        set: SequenceSet,
        mailbox: Vec<u8>,
    },
    Idle,
}

/// A tagged command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub tag: String,
    pub kind: CommandKind,
}

/// Why a command could not be read: what the BAD response says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// The tag, when one could be read.
    pub tag: Option<String>,
    pub message: String,
    /// `BADCHARSET` and the like: a response code for the BAD / NO.
    pub code: Option<String>,
}

type Result<T> = std::result::Result<T, String>;

/// The literal a line ends with: `{12}` (synchronizing) or `{12+}` (RFC 7888); its length and
/// whether the client waits for a continuation.
#[must_use]
pub fn literal_at_end(line: &[u8]) -> Option<(usize, bool)> {
    if line.last() != Some(&b'}') {
        return None;
    }
    let open = line.iter().rposition(|b| *b == b'{')?;
    let inner = &line[open + 1..line.len() - 1];
    let (digits, sync) = match inner.strip_suffix(b"+") {
        Some(digits) => (digits, false),
        None => (inner, true),
    };
    if digits.is_empty() || digits.len() > 10 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let n: usize = std::str::from_utf8(digits).ok()?.parse().ok()?;
    Some((n, sync))
}

/// The tag a command line starts with (for the answer to one that cannot be read).
#[must_use]
pub fn tag_of(line: &[u8]) -> Option<String> {
    let end = line.iter().position(|b| *b == b' ').unwrap_or(line.len());
    let tag = &line[..end];
    (!tag.is_empty() && tag.iter().all(|b| is_tag_char(*b)))
        .then(|| String::from_utf8_lossy(tag).into_owned())
}

fn is_atom_char(b: u8) -> bool {
    b > 0x20
        && b < 0x7f
        && !matches!(b, b'(' | b')' | b'{' | b'%' | b'*' | b'"' | b'\\' | b']')
}

fn is_astring_char(b: u8) -> bool {
    is_atom_char(b) || b == b']'
}

fn is_tag_char(b: u8) -> bool {
    is_astring_char(b) && b != b'+'
}

fn is_list_char(b: u8) -> bool {
    is_atom_char(b) || matches!(b, b'%' | b'*' | b']')
}

/// A cursor over one assembled command.
struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn new(b: &'a [u8]) -> Cursor<'a> {
        Cursor { b, i: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn at_end(&self) -> bool {
        self.i >= self.b.len()
    }

    fn expect(&mut self, byte: u8) -> Result<()> {
        if self.peek() == Some(byte) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected '{}'", char::from(byte)))
        }
    }

    fn sp(&mut self) -> Result<()> {
        self.expect(b' ')
    }

    fn end(&self) -> Result<()> {
        if self.at_end() {
            Ok(())
        } else {
            Err(String::from("unexpected text after the command"))
        }
    }

    fn take_while(&mut self, keep: impl Fn(u8) -> bool) -> &'a [u8] {
        let start = self.i;
        while self.peek().is_some_and(&keep) {
            self.i += 1;
        }
        &self.b[start..self.i]
    }

    fn atom(&mut self) -> Result<&'a [u8]> {
        let atom = self.take_while(is_atom_char);
        if atom.is_empty() {
            Err(String::from("expected an atom"))
        } else {
            Ok(atom)
        }
    }

    /// An atom, upper case (a command or item name).
    fn word(&mut self) -> Result<String> {
        let atom = self.take_while(|b| is_atom_char(b) && b != b'[' && b != b'<');
        if atom.is_empty() {
            return Err(String::from("expected a keyword"));
        }
        Ok(String::from_utf8_lossy(atom).to_ascii_uppercase())
    }

    fn number(&mut self) -> Result<u64> {
        let digits = self.take_while(|b| b.is_ascii_digit());
        if digits.is_empty() || digits.len() > 19 {
            return Err(String::from("expected a number"));
        }
        std::str::from_utf8(digits)
            .ok()
            .and_then(|d| d.parse().ok())
            .ok_or_else(|| String::from("expected a number"))
    }

    fn nz_number32(&mut self) -> Result<u32> {
        let n = self.number()?;
        u32::try_from(n)
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| String::from("expected a number from 1 to 4294967295"))
    }

    fn quoted(&mut self) -> Result<Vec<u8>> {
        self.expect(b'"')?;
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None | Some(b'\r' | b'\n') => return Err(String::from("unterminated string")),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    match self.peek() {
                        Some(c @ (b'"' | b'\\')) => {
                            out.push(c);
                            self.i += 1;
                        }
                        _ => return Err(String::from("a bad escape in a string")),
                    }
                }
                Some(c) => {
                    out.push(c);
                    self.i += 1;
                }
            }
        }
    }

    fn literal(&mut self) -> Result<Vec<u8>> {
        self.expect(b'{')?;
        let n = self.number()?;
        if self.peek() == Some(b'+') {
            self.i += 1;
        }
        self.expect(b'}')?;
        self.expect(b'\r')?;
        self.expect(b'\n')?;
        let n = usize::try_from(n).map_err(|_| String::from("a literal too big"))?;
        if self.b.len() - self.i < n {
            return Err(String::from("a literal shorter than announced"));
        }
        let out = self.b[self.i..self.i + n].to_vec();
        self.i += n;
        Ok(out)
    }

    fn string(&mut self) -> Result<Vec<u8>> {
        match self.peek() {
            Some(b'"') => self.quoted(),
            Some(b'{') => self.literal(),
            _ => Err(String::from("expected a string")),
        }
    }

    fn astring(&mut self) -> Result<Vec<u8>> {
        match self.peek() {
            Some(b'"' | b'{') => self.string(),
            _ => {
                let atom = self.take_while(is_astring_char);
                if atom.is_empty() {
                    Err(String::from("expected a string"))
                } else {
                    Ok(atom.to_vec())
                }
            }
        }
    }

    fn text_astring(&mut self) -> Result<String> {
        let bytes = self.astring()?;
        String::from_utf8(bytes).map_err(|_| String::from("a string that is not UTF-8"))
    }

    /// LIST's pattern: list-chars (`%` and `*` included) or a string.
    fn list_mailbox(&mut self) -> Result<Vec<u8>> {
        match self.peek() {
            Some(b'"' | b'{') => self.string(),
            _ => {
                let atom = self.take_while(is_list_char);
                if atom.is_empty() {
                    Err(String::from("expected a mailbox pattern"))
                } else {
                    Ok(atom.to_vec())
                }
            }
        }
    }

    /// `(a b c)`: the items `item` reads, separated by single spaces.
    fn list<T>(&mut self, mut item: impl FnMut(&mut Cursor<'a>) -> Result<T>) -> Result<Vec<T>> {
        self.expect(b'(')?;
        let mut out = Vec::new();
        if self.peek() == Some(b')') {
            self.i += 1;
            return Ok(out);
        }
        loop {
            out.push(item(self)?);
            match self.peek() {
                Some(b' ') => self.i += 1,
                Some(b')') => {
                    self.i += 1;
                    return Ok(out);
                }
                _ => return Err(String::from("expected ' ' or ')' in a list")),
            }
        }
    }

    /// A flag: `\Seen`, `$Forwarded`, a keyword.
    fn flag(&mut self) -> Result<String> {
        let backslash = self.peek() == Some(b'\\');
        if backslash {
            self.i += 1;
            if self.peek() == Some(b'*') {
                self.i += 1;
                return Ok(String::from("\\*"));
            }
        }
        let atom = self.atom()?;
        let name = String::from_utf8_lossy(atom).into_owned();
        Ok(if backslash { format!("\\{name}") } else { name })
    }

    fn flag_list(&mut self) -> Result<Vec<String>> {
        self.list(|c| c.flag())
    }

    fn sequence_set(&mut self) -> Result<SequenceSet> {
        let mut ranges = Vec::new();
        loop {
            let a = self.bound()?;
            let b = if self.peek() == Some(b':') {
                self.i += 1;
                self.bound()?
            } else {
                a
            };
            ranges.push((a, b));
            if ranges.len() > MAX_SET_RANGES {
                return Err(String::from("a sequence set with too many ranges"));
            }
            if self.peek() == Some(b',') {
                self.i += 1;
            } else {
                return Ok(SequenceSet(ranges));
            }
        }
    }

    fn bound(&mut self) -> Result<Bound> {
        if self.peek() == Some(b'*') {
            self.i += 1;
            return Ok(Bound::Star);
        }
        self.nz_number32().map(Bound::Num)
    }

    /// `"17-Jul-1996 02:44:25 -0700"`.
    fn date_time(&mut self) -> Result<i64> {
        let text = self.quoted()?;
        dates::parse_imap_datetime(&String::from_utf8_lossy(&text))
            .ok_or_else(|| String::from("a bad date-time"))
    }

    /// `1-Feb-1994`, quoted or not.
    fn date(&mut self) -> Result<i64> {
        let text = if self.peek() == Some(b'"') {
            self.quoted()?
        } else {
            self.take_while(|b| b.is_ascii_alphanumeric() || b == b'-')
                .to_vec()
        };
        dates::parse_imap_date(&String::from_utf8_lossy(&text))
            .ok_or_else(|| String::from("a bad date"))
    }

    fn section(&mut self) -> Result<Section> {
        self.expect(b'[')?;
        let mut section = Section::default();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(section);
        }
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            section.path.push(self.nz_number32()?);
            if self.peek() == Some(b'.') {
                self.i += 1;
            } else {
                break;
            }
        }
        if self.peek() != Some(b']') {
            let word = String::from_utf8_lossy(self.take_while(|b| b.is_ascii_alphabetic() || b == b'.'))
                .to_ascii_uppercase();
            section.text = Some(match word.as_str() {
                "HEADER" => SectionText::Header,
                "TEXT" => SectionText::Text,
                "MIME" if !section.path.is_empty() => SectionText::Mime,
                "HEADER.FIELDS" | "HEADER.FIELDS.NOT" => {
                    self.sp()?;
                    let names = self.list(|c| c.text_astring())?;
                    if names.is_empty() {
                        return Err(String::from("an empty header field list"));
                    }
                    let names: Vec<String> = names.iter().map(|n| n.to_ascii_uppercase()).collect();
                    if word == "HEADER.FIELDS" {
                        SectionText::HeaderFields(names)
                    } else {
                        SectionText::HeaderFieldsNot(names)
                    }
                }
                _ => return Err(format!("unknown section \"{word}\"")),
            });
        }
        self.expect(b']')?;
        Ok(section)
    }

    fn fetch_att(&mut self) -> Result<FetchAtt> {
        let name = self.word()?;
        Ok(match name.as_str() {
            "FLAGS" => FetchAtt::Flags,
            "UID" => FetchAtt::Uid,
            "INTERNALDATE" => FetchAtt::InternalDate,
            "RFC822.SIZE" => FetchAtt::Rfc822Size,
            "ENVELOPE" => FetchAtt::Envelope,
            "BODYSTRUCTURE" => FetchAtt::BodyStructure,
            "RFC822" => FetchAtt::Rfc822,
            "RFC822.HEADER" => FetchAtt::Rfc822Header,
            "RFC822.TEXT" => FetchAtt::Rfc822Text,
            "BODY" if self.peek() != Some(b'[') => FetchAtt::Body,
            "BODY" | "BODY.PEEK" => {
                let section = self.section()?;
                let partial = if self.peek() == Some(b'<') {
                    self.i += 1;
                    let start = self.number()?;
                    self.expect(b'.')?;
                    let length = self.number()?;
                    if length == 0 {
                        return Err(String::from("a partial of no octets"));
                    }
                    self.expect(b'>')?;
                    Some((start, length))
                } else {
                    None
                };
                FetchAtt::Section {
                    peek: name == "BODY.PEEK",
                    section,
                    partial,
                }
            }
            other => return Err(format!("unknown fetch item \"{other}\"")),
        })
    }

    fn fetch_atts(&mut self) -> Result<Vec<FetchAtt>> {
        if self.peek() == Some(b'(') {
            let atts = self.list(|c| c.fetch_att())?;
            if atts.is_empty() {
                return Err(String::from("no fetch items"));
            }
            return Ok(atts);
        }
        let start = self.i;
        let word = self.word()?;
        Ok(match word.as_str() {
            "ALL" => vec![
                FetchAtt::Flags,
                FetchAtt::InternalDate,
                FetchAtt::Rfc822Size,
                FetchAtt::Envelope,
            ],
            "FAST" => vec![FetchAtt::Flags, FetchAtt::InternalDate, FetchAtt::Rfc822Size],
            "FULL" => vec![
                FetchAtt::Flags,
                FetchAtt::InternalDate,
                FetchAtt::Rfc822Size,
                FetchAtt::Envelope,
                FetchAtt::Body,
            ],
            _ => {
                self.i = start;
                vec![self.fetch_att()?]
            }
        })
    }

    fn search_key(&mut self, depth: usize) -> Result<SearchKey> {
        if depth > MAX_SEARCH_DEPTH {
            return Err(String::from("search keys nested too deep"));
        }
        if self.peek() == Some(b'(') {
            let keys = self.list(|c| c.search_key(depth + 1))?;
            if keys.is_empty() {
                return Err(String::from("an empty search group"));
            }
            return Ok(SearchKey::And(keys));
        }
        if self.peek().is_some_and(|b| b.is_ascii_digit() || b == b'*') {
            return self.sequence_set().map(SearchKey::Seq);
        }
        let word = self.word()?;
        let string = |c: &mut Cursor<'a>| -> Result<String> {
            c.sp()?;
            let bytes = c.astring()?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        };
        Ok(match word.as_str() {
            "ALL" => SearchKey::All,
            "ANSWERED" => SearchKey::Answered,
            "UNANSWERED" => SearchKey::Unanswered,
            "DELETED" => SearchKey::Deleted,
            "UNDELETED" => SearchKey::Undeleted,
            "DRAFT" => SearchKey::Draft,
            "UNDRAFT" => SearchKey::Undraft,
            "FLAGGED" => SearchKey::Flagged,
            "UNFLAGGED" => SearchKey::Unflagged,
            "SEEN" => SearchKey::Seen,
            "UNSEEN" => SearchKey::Unseen,
            "NEW" => SearchKey::New,
            "OLD" => SearchKey::Old,
            "RECENT" => SearchKey::Recent,
            "KEYWORD" => {
                self.sp()?;
                SearchKey::Keyword(self.flag()?)
            }
            "UNKEYWORD" => {
                self.sp()?;
                SearchKey::Unkeyword(self.flag()?)
            }
            "BCC" => SearchKey::Bcc(string(self)?),
            "CC" => SearchKey::Cc(string(self)?),
            "FROM" => SearchKey::From(string(self)?),
            "TO" => SearchKey::To(string(self)?),
            "SUBJECT" => SearchKey::Subject(string(self)?),
            "BODY" => SearchKey::Body(string(self)?),
            "TEXT" => SearchKey::Text(string(self)?),
            "HEADER" => {
                let name = string(self)?;
                let value = string(self)?;
                SearchKey::Header(name, value)
            }
            "BEFORE" | "ON" | "SINCE" | "SENTBEFORE" | "SENTON" | "SENTSINCE" => {
                self.sp()?;
                let day = self.date()?;
                match word.as_str() {
                    "BEFORE" => SearchKey::Before(day),
                    "ON" => SearchKey::On(day),
                    "SINCE" => SearchKey::Since(day),
                    "SENTBEFORE" => SearchKey::SentBefore(day),
                    "SENTON" => SearchKey::SentOn(day),
                    _ => SearchKey::SentSince(day),
                }
            }
            "LARGER" => {
                self.sp()?;
                SearchKey::Larger(self.number()?)
            }
            "SMALLER" => {
                self.sp()?;
                SearchKey::Smaller(self.number()?)
            }
            "UID" => {
                self.sp()?;
                SearchKey::Uid(self.sequence_set()?)
            }
            "NOT" => {
                self.sp()?;
                SearchKey::Not(Box::new(self.search_key(depth + 1)?))
            }
            "OR" => {
                self.sp()?;
                let a = self.search_key(depth + 1)?;
                self.sp()?;
                let b = self.search_key(depth + 1)?;
                SearchKey::Or(Box::new(a), Box::new(b))
            }
            other => return Err(format!("unknown search key \"{other}\"")),
        })
    }

    /// Skips the rest of a command we accept without reading it (ID's parameters).
    fn rest(&mut self) {
        self.i = self.b.len();
    }
}

/// Reads one command (its literals inside: `{n}\r\n` and the `n` bytes after it).
///
/// # Errors
///
/// [`ParseError`]: what the BAD says.
pub fn parse_command(bytes: &[u8]) -> std::result::Result<Command, ParseError> {
    let tag = tag_of(bytes);
    let bad = |message: String| ParseError {
        tag: tag.clone(),
        message,
        code: None,
    };
    let Some(tag_text) = tag.clone() else {
        return Err(bad(String::from("a command starts with its tag")));
    };
    let mut c = Cursor::new(bytes);
    c.i = tag_text.len();
    c.sp().map_err(bad)?;
    let mut name = c.word().map_err(bad)?;
    let mut uid = false;
    if name == "UID" {
        uid = true;
        c.sp().map_err(bad)?;
        name = c.word().map_err(bad)?;
        if !matches!(name.as_str(), "FETCH" | "STORE" | "COPY" | "MOVE" | "SEARCH" | "EXPUNGE") {
            return Err(bad(format!("UID {name} is no command")));
        }
    }
    let kind = parse_args(&mut c, &name, uid).map_err(|e| match e {
        ArgError::Bad(message) => bad(message),
        ArgError::Charset => ParseError {
            tag: tag.clone(),
            message: String::from("only US-ASCII and UTF-8 searches"),
            code: Some(String::from("BADCHARSET (US-ASCII UTF-8)")),
        },
    })?;
    Ok(Command {
        tag: tag_text,
        kind,
    })
}

enum ArgError {
    Bad(String),
    Charset,
}

impl From<String> for ArgError {
    fn from(message: String) -> ArgError {
        ArgError::Bad(message)
    }
}

fn parse_args(c: &mut Cursor<'_>, name: &str, uid: bool) -> std::result::Result<CommandKind, ArgError> {
    let kind = match name {
        "CAPABILITY" => CommandKind::Capability,
        "NOOP" => CommandKind::Noop,
        "LOGOUT" => CommandKind::Logout,
        "STARTTLS" => CommandKind::StartTls,
        "NAMESPACE" => CommandKind::Namespace,
        "CHECK" => CommandKind::Check,
        "CLOSE" => CommandKind::Close,
        "UNSELECT" => CommandKind::Unselect,
        "IDLE" => CommandKind::Idle,
        "ID" => {
            c.sp()?;
            c.rest();
            CommandKind::Id
        }
        "ENABLE" => {
            c.sp()?;
            c.rest();
            CommandKind::Enable
        }
        "LOGIN" => {
            c.sp()?;
            let user = c.astring()?;
            c.sp()?;
            let password = c.astring()?;
            CommandKind::Login { user, password }
        }
        "AUTHENTICATE" => {
            c.sp()?;
            let mechanism = String::from_utf8_lossy(c.atom()?).to_ascii_uppercase();
            let initial = if c.peek() == Some(b' ') {
                c.i += 1;
                let text = c.take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='));
                if text.is_empty() {
                    return Err(ArgError::Bad(String::from("expected a base64 initial response")));
                }
                Some(String::from_utf8_lossy(text).into_owned())
            } else {
                None
            };
            CommandKind::Authenticate { mechanism, initial }
        }
        "SELECT" | "EXAMINE" | "CREATE" | "DELETE" | "SUBSCRIBE" | "UNSUBSCRIBE" => {
            c.sp()?;
            let mailbox = c.astring()?;
            // CREATE and SELECT may carry parameters (RFC 4466); none are offered: refused.
            match name {
                "SELECT" => CommandKind::Select(mailbox),
                "EXAMINE" => CommandKind::Examine(mailbox),
                "CREATE" => CommandKind::Create(mailbox),
                "DELETE" => CommandKind::Delete(mailbox),
                "SUBSCRIBE" => CommandKind::Subscribe(mailbox),
                _ => CommandKind::Unsubscribe(mailbox),
            }
        }
        "RENAME" => {
            c.sp()?;
            let from = c.astring()?;
            c.sp()?;
            let to = c.astring()?;
            CommandKind::Rename(from, to)
        }
        "LIST" | "LSUB" => {
            c.sp()?;
            // LIST-EXTENDED's selection options are read and ignored.
            if c.peek() == Some(b'(') {
                let _ = c.list(|c| c.atom().map(<[u8]>::to_vec))?;
                c.sp()?;
            }
            let reference = c.astring()?;
            c.sp()?;
            let pattern = if c.peek() == Some(b'(') {
                let patterns = c.list(|c| c.list_mailbox())?;
                patterns.into_iter().next().unwrap_or_default()
            } else {
                c.list_mailbox()?
            };
            // ... and its RETURN options.
            c.rest();
            CommandKind::List {
                reference,
                pattern,
                subscribed: name == "LSUB",
            }
        }
        "STATUS" => {
            c.sp()?;
            let mailbox = c.astring()?;
            c.sp()?;
            let items = c.list(|c| {
                let word = c.word()?;
                Ok(match word.as_str() {
                    "MESSAGES" => StatusItem::Messages,
                    "RECENT" => StatusItem::Recent,
                    "UIDNEXT" => StatusItem::UidNext,
                    "UIDVALIDITY" => StatusItem::UidValidity,
                    "UNSEEN" => StatusItem::Unseen,
                    other => return Err(format!("unknown status item \"{other}\"")),
                })
            })?;
            CommandKind::Status { mailbox, items }
        }
        "APPEND" => {
            c.sp()?;
            let mailbox = c.astring()?;
            c.sp()?;
            let flags = if c.peek() == Some(b'(') {
                let flags = c.flag_list()?;
                c.sp()?;
                flags
            } else {
                Vec::new()
            };
            let date = if c.peek() == Some(b'"') {
                let date = c.date_time()?;
                c.sp()?;
                Some(date)
            } else {
                None
            };
            let message = c.literal()?;
            CommandKind::Append {
                mailbox,
                flags,
                date,
                message,
            }
        }
        "EXPUNGE" => {
            let uids = if uid {
                c.sp()?;
                Some(c.sequence_set()?)
            } else {
                None
            };
            CommandKind::Expunge { uids }
        }
        "SEARCH" => {
            c.sp()?;
            let mut keys = Vec::new();
            if c.b[c.i..].len() >= 7 && c.b[c.i..c.i + 7].eq_ignore_ascii_case(b"CHARSET") {
                c.word()?;
                c.sp()?;
                let charset = c.text_astring()?.to_ascii_uppercase();
                if !matches!(charset.as_str(), "US-ASCII" | "UTF-8") {
                    return Err(ArgError::Charset);
                }
                c.sp()?;
            }
            loop {
                keys.push(c.search_key(0)?);
                if c.peek() == Some(b' ') {
                    c.i += 1;
                } else {
                    break;
                }
            }
            let key = if keys.len() == 1 {
                keys.remove(0)
            } else {
                SearchKey::And(keys)
            };
            CommandKind::Search { uid, key }
        }
        "FETCH" => {
            c.sp()?;
            let set = c.sequence_set()?;
            c.sp()?;
            let atts = c.fetch_atts()?;
            CommandKind::Fetch { uid, set, atts }
        }
        "STORE" => {
            c.sp()?;
            let set = c.sequence_set()?;
            c.sp()?;
            let mode = match c.peek() {
                Some(b'+') => {
                    c.i += 1;
                    StoreMode::Add
                }
                Some(b'-') => {
                    c.i += 1;
                    StoreMode::Remove
                }
                _ => StoreMode::Replace,
            };
            let word = c.word()?;
            let silent = match word.as_str() {
                "FLAGS" => false,
                "FLAGS.SILENT" => true,
                other => return Err(ArgError::Bad(format!("cannot store \"{other}\""))),
            };
            c.sp()?;
            let flags = if c.peek() == Some(b'(') {
                c.flag_list()?
            } else {
                let mut flags = vec![c.flag()?];
                while c.peek() == Some(b' ') {
                    c.i += 1;
                    flags.push(c.flag()?);
                }
                flags
            };
            CommandKind::Store {
                uid,
                set,
                mode,
                silent,
                flags,
            }
        }
        "COPY" | "MOVE" => {
            c.sp()?;
            let set = c.sequence_set()?;
            c.sp()?;
            let mailbox = c.astring()?;
            if name == "COPY" {
                CommandKind::Copy { uid, set, mailbox }
            } else {
                CommandKind::Move { uid, set, mailbox }
            }
        }
        other => return Err(ArgError::Bad(format!("unknown command \"{other}\""))),
    };
    c.end()?;
    Ok(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> CommandKind {
        match parse_command(text.as_bytes()) {
            Ok(command) => command.kind,
            Err(e) => panic!("{text}: {e:?}"),
        }
    }

    fn bad(text: &[u8]) -> ParseError {
        parse_command(text).expect_err(&String::from_utf8_lossy(text))
    }

    fn set(ranges: &[(Bound, Bound)]) -> SequenceSet {
        SequenceSet(ranges.to_vec())
    }

    #[test]
    fn literals_at_the_end_of_a_line_are_found_with_or_without_plus() {
        assert_eq!(literal_at_end(b"A1 LOGIN {3}"), Some((3, true)));
        assert_eq!(literal_at_end(b"A1 APPEND INBOX {120+}"), Some((120, false)));
        assert_eq!(literal_at_end(b"A1 LOGIN \"{3}\" x"), None);
        assert_eq!(literal_at_end(b"A1 X {}"), None);
        assert_eq!(literal_at_end(b"A1 X {99999999999}"), None);
    }

    #[test]
    fn login_takes_atoms_quoted_strings_and_literals() {
        assert_eq!(
            parse("a1 login ada@example.org secret"),
            CommandKind::Login {
                user: b"ada@example.org".to_vec(),
                password: b"secret".to_vec()
            }
        );
        assert_eq!(
            parse("a2 LOGIN \"ada@example.org\" \"pa\\\"ss\\\\\""),
            CommandKind::Login {
                user: b"ada@example.org".to_vec(),
                password: b"pa\"ss\\".to_vec()
            }
        );
        assert_eq!(
            parse("a3 LOGIN {15}\r\nada@example.org {6+}\r\nsec et"),
            CommandKind::Login {
                user: b"ada@example.org".to_vec(),
                password: b"sec et".to_vec()
            }
        );
        assert!(bad(b"a4 LOGIN ada").message.contains("expected"));
        assert!(bad(b"a5 LOGIN \"open").message.contains("unterminated"));
    }

    #[test]
    fn fetch_reads_macros_sections_and_partials() {
        assert_eq!(
            parse("A1 FETCH 1:* FAST"),
            CommandKind::Fetch {
                uid: false,
                set: set(&[(Bound::Num(1), Bound::Star)]),
                atts: vec![FetchAtt::Flags, FetchAtt::InternalDate, FetchAtt::Rfc822Size],
            }
        );
        let CommandKind::Fetch { uid, set: s, atts } = parse(
            "A2 UID FETCH 4,7:9 (UID FLAGS BODY.PEEK[HEADER.FIELDS (From \"Subject\")] BODY[1.2.MIME] BODY[]<0.1024> BODYSTRUCTURE BODY)",
        ) else {
            panic!();
        };
        assert!(uid);
        assert_eq!(s, set(&[(Bound::Num(4), Bound::Num(4)), (Bound::Num(7), Bound::Num(9))]));
        assert_eq!(
            atts,
            vec![
                FetchAtt::Uid,
                FetchAtt::Flags,
                FetchAtt::Section {
                    peek: true,
                    section: Section {
                        path: vec![],
                        text: Some(SectionText::HeaderFields(vec![
                            "FROM".to_string(),
                            "SUBJECT".to_string()
                        ]))
                    },
                    partial: None
                },
                FetchAtt::Section {
                    peek: false,
                    section: Section {
                        path: vec![1, 2],
                        text: Some(SectionText::Mime)
                    },
                    partial: None
                },
                FetchAtt::Section {
                    peek: false,
                    section: Section::default(),
                    partial: Some((0, 1024))
                },
                FetchAtt::BodyStructure,
                FetchAtt::Body,
            ]
        );
        assert!(bad(b"A3 FETCH 1 (FLAGS").message.contains("list"));
        assert!(bad(b"A4 FETCH 1 BODY[MIME]").message.contains("section"));
        assert!(bad(b"A5 FETCH 0 FLAGS").message.contains("number"));
        assert!(bad(b"A6 FETCH 1 BINARY[1]").message.contains("BINARY"));
    }

    #[test]
    fn a_sequence_set_counts_star_as_the_largest_and_ranges_either_way() {
        let s = set(&[(Bound::Num(5), Bound::Star), (Bound::Num(2), Bound::Num(2))]);
        assert!(s.contains(2, 9));
        assert!(!s.contains(3, 9));
        assert!(s.contains(9, 9));
        // `5:*` when the largest is 3 is `3:5`.
        assert!(s.contains(3, 3));
        assert!(s.has_star());
        let reversed = set(&[(Bound::Num(9), Bound::Num(7))]);
        assert!(reversed.contains(8, 100));
    }

    #[test]
    fn store_reads_modes_silence_and_flags_with_or_without_parentheses() {
        assert_eq!(
            parse("A1 STORE 1:3 +FLAGS.SILENT (\\Seen \\Deleted)"),
            CommandKind::Store {
                uid: false,
                set: set(&[(Bound::Num(1), Bound::Num(3))]),
                mode: StoreMode::Add,
                silent: true,
                flags: vec!["\\Seen".to_string(), "\\Deleted".to_string()],
            }
        );
        assert_eq!(
            parse("A2 UID STORE 7 -FLAGS \\Flagged $Junk"),
            CommandKind::Store {
                uid: true,
                set: set(&[(Bound::Num(7), Bound::Num(7))]),
                mode: StoreMode::Remove,
                silent: false,
                flags: vec!["\\Flagged".to_string(), "$Junk".to_string()],
            }
        );
        assert!(bad(b"A3 STORE 1 X-GM-LABELS (a)").message.contains("cannot store"));
    }

    #[test]
    fn search_reads_nested_keys_dates_charsets_and_sets() {
        assert_eq!(
            parse("A1 UID SEARCH CHARSET UTF-8 UNSEEN OR FROM ada (SINCE 1-Oct-2026 NOT DELETED) UID 3:*"),
            CommandKind::Search {
                uid: true,
                key: SearchKey::And(vec![
                    SearchKey::Unseen,
                    SearchKey::Or(
                        Box::new(SearchKey::From("ada".to_string())),
                        Box::new(SearchKey::And(vec![
                            SearchKey::Since(crate::dates::days_from_civil(2026, 10, 1)),
                            SearchKey::Not(Box::new(SearchKey::Deleted)),
                        ]))
                    ),
                    SearchKey::Uid(set(&[(Bound::Num(3), Bound::Star)])),
                ])
            }
        );
        assert_eq!(
            parse("A2 SEARCH 1:5 HEADER Message-ID \"<m1@x>\" LARGER 100"),
            CommandKind::Search {
                uid: false,
                key: SearchKey::And(vec![
                    SearchKey::Seq(set(&[(Bound::Num(1), Bound::Num(5))])),
                    SearchKey::Header("Message-ID".to_string(), "<m1@x>".to_string()),
                    SearchKey::Larger(100),
                ])
            }
        );
        let error = bad(b"A3 SEARCH CHARSET KOI8-R TEXT x");
        assert_eq!(error.code.as_deref(), Some("BADCHARSET (US-ASCII UTF-8)"));
        let deep = format!("A4 SEARCH {}ALL", "NOT ".repeat(100));
        assert!(bad(deep.as_bytes()).message.contains("deep"));
    }

    #[test]
    fn append_takes_flags_a_date_and_the_message_literal() {
        assert_eq!(
            parse("A1 APPEND Sent (\\Seen) \" 1-Oct-2026 08:30:00 +0000\" {5}\r\nhello"),
            CommandKind::Append {
                mailbox: b"Sent".to_vec(),
                flags: vec!["\\Seen".to_string()],
                date: Some(1_790_843_400),
                message: b"hello".to_vec(),
            }
        );
        assert_eq!(
            parse("A2 APPEND \"Drafts\" {2+}\r\nhi"),
            CommandKind::Append {
                mailbox: b"Drafts".to_vec(),
                flags: vec![],
                date: None,
                message: b"hi".to_vec(),
            }
        );
        assert!(bad(b"A3 APPEND INBOX {9}\r\nshort").message.contains("shorter"));
    }

    #[test]
    fn list_status_and_the_mailbox_commands_read_their_names() {
        assert_eq!(
            parse("A1 LIST \"\" \"*\""),
            CommandKind::List {
                reference: vec![],
                pattern: b"*".to_vec(),
                subscribed: false
            }
        );
        assert_eq!(
            parse("A2 LIST (SPECIAL-USE) \"\" % RETURN (SPECIAL-USE)"),
            CommandKind::List {
                reference: vec![],
                pattern: b"%".to_vec(),
                subscribed: false
            }
        );
        assert_eq!(
            parse("A3 STATUS \"Entw&APw-rfe\" (MESSAGES UIDNEXT UNSEEN)"),
            CommandKind::Status {
                mailbox: b"Entw&APw-rfe".to_vec(),
                items: vec![StatusItem::Messages, StatusItem::UidNext, StatusItem::Unseen],
            }
        );
        assert_eq!(parse("A4 select inbox"), CommandKind::Select(b"inbox".to_vec()));
        assert_eq!(
            parse("A5 RENAME Work \"Work Old\""),
            CommandKind::Rename(b"Work".to_vec(), b"Work Old".to_vec())
        );
        assert_eq!(
            parse("A6 UID MOVE 1:2 Trash"),
            CommandKind::Move {
                uid: true,
                set: set(&[(Bound::Num(1), Bound::Num(2))]),
                mailbox: b"Trash".to_vec()
            }
        );
        assert_eq!(
            parse("A7 UID EXPUNGE 4"),
            CommandKind::Expunge {
                uids: Some(set(&[(Bound::Num(4), Bound::Num(4))]))
            }
        );
        assert_eq!(
            parse("A8 AUTHENTICATE PLAIN AGFkYQBwdw=="),
            CommandKind::Authenticate {
                mechanism: "PLAIN".to_string(),
                initial: Some("AGFkYQBwdw==".to_string())
            }
        );
        assert_eq!(parse("A9 ID (\"name\" \"Mail\")"), CommandKind::Id);
    }

    #[test]
    fn a_line_without_a_tag_or_with_junk_after_the_command_is_bad() {
        assert_eq!(bad(b"").tag, None);
        assert_eq!(bad(b"+ x").tag, None);
        let e = bad(b"A1 NOOP extra");
        assert_eq!(e.tag.as_deref(), Some("A1"));
        assert!(bad(b"A1 FROBNICATE").message.contains("unknown command"));
        assert!(bad(b"A1 UID NOOP").message.contains("no command"));
    }

    #[test]
    fn condstore_qresync_and_esearch_modifiers_and_enable_are_read() {
        let command =
            parse_command(b"A1 SELECT INBOX (QRESYNC (67890007 20050715194045000 41,43:211,214:541))").unwrap();
        assert_eq!(command.kind, CommandKind::Select(b"INBOX".to_vec()));
        let qresync = command.modifiers.qresync.expect("QRESYNC's parameters");
        assert_eq!((qresync.validity, qresync.modseq), (67_890_007, 20_050_715_194_045_000));
        assert!(qresync.known.is_some());
        assert!(parse_command(b"A2 EXAMINE INBOX (CONDSTORE)").unwrap().modifiers.condstore);
        let fetch = parse_command(b"A3 UID FETCH 1:* (FLAGS) (CHANGEDSINCE 12345 VANISHED)").unwrap();
        assert_eq!((fetch.modifiers.changed_since, fetch.modifiers.vanished), (Some(12345), true));
        let store = parse_command(b"A4 STORE 1:3 (UNCHANGEDSINCE 320162338) +FLAGS.SILENT (\\Deleted)").unwrap();
        assert_eq!(store.modifiers.unchanged_since, Some(320_162_338));
        assert!(matches!(store.kind, CommandKind::Store { silent: true, .. }));
        let enable = parse_command(b"A5 ENABLE CONDSTORE qresync").unwrap();
        assert_eq!(enable.modifiers.enable, vec!["CONDSTORE", "QRESYNC"]);
        assert_eq!(
            parse("A6 SEARCH MODSEQ \"/flags/\\\\draft\" all 620162338"),
            CommandKind::Search {
                uid: false,
                key: SearchKey::Modseq(620_162_338),
            }
        );
        assert_eq!(
            parse("A7 FETCH 1 (UID MODSEQ)"),
            CommandKind::Fetch {
                uid: false,
                set: set(&[(Bound::Num(1), Bound::Num(1))]),
                atts: vec![FetchAtt::Uid, FetchAtt::Modseq],
            }
        );
        assert_eq!(
            parse("A8 STATUS INBOX (HIGHESTMODSEQ SIZE DELETED)"),
            CommandKind::Status {
                mailbox: b"INBOX".to_vec(),
                items: vec![StatusItem::HighestModseq, StatusItem::Size, StatusItem::Deleted],
            }
        );
        let search = parse_command(b"A9 UID SEARCH RETURN (MIN COUNT) UNSEEN").unwrap();
        assert_eq!(search.modifiers.search_return, Some(vec![String::from("MIN"), String::from("COUNT")]));
        assert_eq!(search.kind, CommandKind::Search { uid: true, key: SearchKey::Unseen });
    }
}
