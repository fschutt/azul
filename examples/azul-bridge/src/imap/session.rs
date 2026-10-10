//! One IMAP connection: reading commands with their literals inside the limits, the states
//! (not signed in, signed in, a mailbox selected), and every command's answer.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

use azmail_core::{folders::Role, mutf7};

use crate::{
    dates, mime,
    net::{self, looks_like_http, Conn, Input, RateLimiter, ReadError},
    sent,
    store::{self, Mark, MailboxInfo, Marks, StoreError, StoredMessage},
};

use super::{
    fetch,
    parse::{
        self, Command, CommandKind, FetchAtt, SearchKey, SequenceSet, StatusItem,
        StoreMode,
    },
    search::{self, Candidate, Largest, Need},
    Flags, Imap, CAPABILITIES, CAPABILITIES_BEFORE_LOGIN,
};

/// The first bytes of a message read for a header-only fetch or search.
const HEAD_BYTES: u64 = 64 * 1024;
/// Messages a session keeps read (a client fetches the header, then the body).
const CACHE_MESSAGES: usize = 8;
/// ...and their bytes at most.
const CACHE_BYTES: usize = 64 * 1024 * 1024;

/// One message of the selected mailbox.
#[derive(Debug, Clone)]
struct Msg {
    uid: u32,
    name: String,
    id: String,
    size: u64,
    arrived: u64,
    flags: Flags,
}

/// The selected mailbox: its messages by UID (the sequence number is the place + 1).
#[derive(Debug)]
struct Selected {
    path: String,
    read_only: bool,
    validity: u32,
    msgs: Vec<Msg>,
}

#[derive(Debug)]
enum State {
    NotAuthenticated,
    Authenticated,
    Selected(Selected),
    Logout,
}

/// What a command's handler says happens next.
enum Next {
    Continue,
    Close,
}

pub(super) struct Session<'s, C: Conn> {
    imap: &'s Imap,
    conn: C,
    input: Input,
    rate: RateLimiter,
    state: State,
    failures: u32,
    cache: Vec<(String, Arc<Vec<u8>>)>,
}

/// A mailbox's name as IMAP lists it: `INBOX`, else its path in modified UTF-7.
fn imap_name(info: &MailboxInfo) -> String {
    if info.role == Role::Inbox {
        String::from("INBOX")
    } else {
        mutf7::encode(&info.path)
    }
}

/// The special-use attribute of a role (RFC 6154).
fn special_use(role: Role) -> Option<&'static str> {
    match role {
        Role::Sent => Some("\\Sent"),
        Role::Drafts => Some("\\Drafts"),
        Role::Archive => Some("\\Archive"),
        Role::Spam => Some("\\Junk"),
        Role::Trash => Some("\\Trash"),
        Role::All => Some("\\All"),
        Role::Flagged => Some("\\Flagged"),
        Role::Inbox | Role::Other => None,
    }
}

/// LIST's wildcards: `*` matches anything, `%` anything but the hierarchy delimiter `/`.
fn wildcard(pattern: &[u8], name: &[u8]) -> bool {
    // Positions of `pattern` that can stand at each position of `name` (a small DP).
    let (p, n) = (pattern.len(), name.len());
    let mut row = vec![false; n + 1];
    row[0] = true;
    for i in 0..p {
        let mut next = vec![false; n + 1];
        match pattern[i] {
            b'*' | b'%' => {
                let stop_at_slash = pattern[i] == b'%';
                let mut reachable = false;
                for j in 0..=n {
                    reachable = reachable || row[j];
                    next[j] = reachable;
                    if j < n && stop_at_slash && name[j] == b'/' {
                        reachable = false;
                    }
                }
            }
            c => {
                for j in 0..n {
                    if row[j] && name[j] == c {
                        next[j + 1] = true;
                    }
                }
            }
        }
        row = next;
    }
    row[n]
}

impl<'s, C: Conn> Session<'s, C> {
    pub(super) fn new(imap: &'s Imap, conn: C) -> Session<'s, C> {
        Session {
            rate: RateLimiter::new(
                imap.limits.commands_per_second,
                imap.limits.command_burst,
            ),
            imap,
            conn,
            input: Input::new(),
            state: State::NotAuthenticated,
            failures: 0,
            cache: Vec::new(),
        }
    }

    fn send(&mut self, bytes: &[u8]) -> bool {
        net::send(&mut self.conn, bytes).is_ok()
    }

    fn line(&mut self, text: &str) -> bool {
        let mut bytes = Vec::with_capacity(text.len() + 2);
        bytes.extend_from_slice(text.as_bytes());
        bytes.extend_from_slice(b"\r\n");
        self.send(&bytes)
    }

    fn capabilities(&self) -> &'static str {
        if matches!(self.state, State::NotAuthenticated) {
            CAPABILITIES_BEFORE_LOGIN
        } else {
            CAPABILITIES
        }
    }

    /// Serves the connection until it is closed, LOGOUT, a fatal error or the idle timeout.
    pub(super) fn run(mut self) {
        let _ = self.conn.set_read_timeout(Some(self.imap.limits.imap_idle));
        let greeting = format!(
            "* OK [CAPABILITY {}] Azlin Bridge ready",
            CAPABILITIES_BEFORE_LOGIN
        );
        if !self.line(&greeting) {
            return;
        }
        let mut first = true;
        loop {
            let command = match self.read_command(first) {
                Ok(Some(command)) => command,
                Ok(None) => continue,
                Err(ReadError::Timeout) => {
                    let _ = self.line("* BYE Autologout: nothing was said for too long");
                    return;
                }
                Err(ReadError::TooLong) => {
                    let _ = self.line("* BYE A line was too long");
                    return;
                }
                Err(_) => return,
            };
            first = false;
            self.rate.take();
            let next = match parse::parse_command(&command) {
                Ok(command) => self.dispatch(command),
                Err(e) => {
                    let tag = e.tag.unwrap_or_else(|| String::from("*"));
                    let code = e.code.map(|c| format!("[{c}] ")).unwrap_or_default();
                    if self.line(&format!("{tag} BAD {code}{}", e.message)) {
                        Next::Continue
                    } else {
                        Next::Close
                    }
                }
            };
            if matches!(next, Next::Close) || matches!(self.state, State::Logout) {
                return;
            }
        }
    }

    /// One command: its lines and literals. `Ok(None)`: a literal was refused (answered).
    fn read_command(&mut self, first: bool) -> Result<Option<Vec<u8>>, ReadError> {
        let limits = self.imap.limits.clone();
        let mut command: Vec<u8> = Vec::new();
        let mut literals = 0usize;
        // The command's own text, its literals aside.
        let mut text = 0usize;
        let mut append = false;
        loop {
            let line = self.input.read_line(&mut self.conn, limits.line_bytes)?;
            if command.is_empty() {
                if first && looks_like_http(&line) {
                    return Err(ReadError::Closed);
                }
                // APPEND's literal is the message: it may be as big as a message.
                append = line
                    .split(|b| *b == b' ')
                    .nth(1)
                    .is_some_and(|word| word.eq_ignore_ascii_case(b"APPEND"));
            }
            text += line.len();
            if text > limits.command_bytes {
                let _ = self.line("* BYE The command is too long");
                return Err(ReadError::Closed);
            }
            let Some((n, sync)) = parse::literal_at_end(&line) else {
                command.extend_from_slice(&line);
                return Ok(Some(command));
            };
            let limit = if append {
                limits.message_bytes
            } else {
                limits.literal_bytes
            };
            if n > limit || literals + n > limit + limits.command_bytes {
                let tag = parse::tag_of(if command.is_empty() { &line } else { &command })
                    .unwrap_or_else(|| String::from("*"));
                if sync {
                    // The client waits for "+": it sends nothing, the connection stays usable.
                    let _ = self.line(&format!("{tag} NO [TOOBIG] The literal is too big"));
                    return Ok(None);
                }
                let _ = self.line("* BYE [TOOBIG] The literal is too big");
                return Err(ReadError::Closed);
            }
            command.extend_from_slice(&line);
            command.extend_from_slice(b"\r\n");
            if sync && !self.line("+ Ready for the literal") {
                return Err(ReadError::Closed);
            }
            let bytes = self.input.read_bytes(&mut self.conn, n)?;
            literals += n;
            command.extend_from_slice(&bytes);
        }
    }

    fn ok(&mut self, tag: &str, text: &str) -> Next {
        if self.line(&format!("{tag} OK {text}")) {
            Next::Continue
        } else {
            Next::Close
        }
    }

    fn no(&mut self, tag: &str, text: &str) -> Next {
        if self.line(&format!("{tag} NO {text}")) {
            Next::Continue
        } else {
            Next::Close
        }
    }

    fn bad(&mut self, tag: &str, text: &str) -> Next {
        if self.line(&format!("{tag} BAD {text}")) {
            Next::Continue
        } else {
            Next::Close
        }
    }

    /// A store error as NO, with the response code a client acts on.
    fn store_no(&mut self, tag: &str, e: &StoreError) -> Next {
        let code = match e {
            StoreError::NotFound(_) => "[NONEXISTENT] ",
            StoreError::Exists(_) => "[ALREADYEXISTS] ",
            StoreError::Invalid(_) => "[CANNOT] ",
            StoreError::Failed(_) => "[UNAVAILABLE] ",
        };
        self.no(tag, &format!("{code}{e}"))
    }

    fn dispatch(&mut self, command: Command) -> Next {
        let tag = command.tag;
        let signed_in = !matches!(self.state, State::NotAuthenticated);
        let selected = matches!(self.state, State::Selected(_));
        match command.kind {
            CommandKind::Capability => {
                let caps = format!("* CAPABILITY {}", self.capabilities());
                if !self.line(&caps) {
                    return Next::Close;
                }
                self.ok(&tag, "CAPABILITY completed")
            }
            CommandKind::Noop | CommandKind::Check => {
                if selected {
                    let updates = self.refresh();
                    if !self.send(&updates) {
                        return Next::Close;
                    }
                }
                self.ok(&tag, "Done")
            }
            CommandKind::Logout => {
                let _ = self.line("* BYE Azlin Bridge signing off");
                let _ = self.ok(&tag, "LOGOUT completed");
                self.state = State::Logout;
                Next::Close
            }
            CommandKind::Id => {
                if !self.line("* ID (\"name\" \"Azlin Bridge\" \"version\" \"0.1.0\")") {
                    return Next::Close;
                }
                self.ok(&tag, "ID completed")
            }
            CommandKind::Enable => {
                if !self.line("* ENABLED") {
                    return Next::Close;
                }
                self.ok(&tag, "Nothing to enable")
            }
            CommandKind::StartTls => self.no(
                &tag,
                "TLS is not offered: the bridge only listens on this computer's own address",
            ),
            CommandKind::Login { user, password } => {
                if signed_in {
                    return self.bad(&tag, "Already signed in");
                }
                self.sign_in(&tag, &user, &password)
            }
            CommandKind::Authenticate { mechanism, initial } => {
                if signed_in {
                    return self.bad(&tag, "Already signed in");
                }
                self.authenticate(&tag, &mechanism, initial)
            }
            _ if !signed_in => self.bad(&tag, "Sign in first (LOGIN or AUTHENTICATE)"),
            CommandKind::Namespace => {
                if !self.line("* NAMESPACE ((\"\" \"/\")) NIL NIL") {
                    return Next::Close;
                }
                self.ok(&tag, "NAMESPACE completed")
            }
            CommandKind::Select(name) => self.select(&tag, &name, false),
            CommandKind::Examine(name) => self.select(&tag, &name, true),
            CommandKind::Create(name) => self.create(&tag, &name),
            CommandKind::Delete(name) => self.delete(&tag, &name),
            CommandKind::Rename(from, to) => self.rename(&tag, &from, &to),
            CommandKind::Subscribe(_) | CommandKind::Unsubscribe(_) => {
                self.ok(&tag, "Every mailbox is subscribed")
            }
            CommandKind::List {
                reference,
                pattern,
                subscribed,
            } => self.list(&tag, &reference, &pattern, subscribed),
            CommandKind::Status { mailbox, items } => self.status(&tag, &mailbox, &items),
            CommandKind::Append {
                mailbox,
                flags,
                date,
                message,
            } => self.append(&tag, &mailbox, &flags, date, &message),
            CommandKind::Idle => self.idle(&tag),
            _ if !selected => self.bad(&tag, "Select a mailbox first"),
            CommandKind::Close => {
                let read_only = self.selected().is_some_and(|s| s.read_only);
                if !read_only {
                    let _ = self.expunge_deleted(None, false);
                }
                self.state = State::Authenticated;
                self.ok(&tag, "CLOSE completed")
            }
            CommandKind::Unselect => {
                self.state = State::Authenticated;
                self.ok(&tag, "UNSELECT completed")
            }
            CommandKind::Expunge { uids } => {
                if self.selected().is_some_and(|s| s.read_only) {
                    return self.no(&tag, "[READ-ONLY] The mailbox is read-only");
                }
                match self.expunge_deleted(uids.as_ref(), true) {
                    Ok(()) => self.ok(&tag, "EXPUNGE completed"),
                    Err(Some(e)) => self.store_no(&tag, &e),
                    Err(None) => Next::Close,
                }
            }
            CommandKind::Search { uid, key } => self.search(&tag, uid, &key),
            CommandKind::Fetch { uid, set, atts } => self.fetch(&tag, uid, &set, atts),
            CommandKind::Store {
                uid,
                set,
                mode,
                silent,
                flags,
            } => self.store(&tag, uid, &set, mode, silent, &flags),
            CommandKind::Copy { uid, set, mailbox } => {
                self.copy_or_move(&tag, uid, &set, &mailbox, false)
            }
            CommandKind::Move { uid, set, mailbox } => {
                self.copy_or_move(&tag, uid, &set, &mailbox, true)
            }
        }
    }

    // ---- signing in ----

    fn sign_in(&mut self, tag: &str, user: &[u8], password: &[u8]) -> Next {
        self.imap.gate.before_attempt();
        if self.imap.credentials.check(user, password) {
            self.state = State::Authenticated;
            let text = format!("[CAPABILITY {CAPABILITIES}] Signed in");
            return self.ok(tag, &text);
        }
        self.imap.gate.failed();
        self.failures += 1;
        if self.failures >= self.imap.limits.auth_failures_per_connection {
            let _ = self.line(&format!("{tag} NO [AUTHENTICATIONFAILED] Invalid credentials"));
            let _ = self.line("* BYE Too many failed sign-ins");
            return Next::Close;
        }
        self.no(tag, "[AUTHENTICATIONFAILED] Invalid credentials")
    }

    /// One SASL answer line: `None` when the client cancelled (`*`) or went away.
    fn sasl_line(&mut self, challenge: &str) -> Option<String> {
        if !self.line(&format!("+ {challenge}")) {
            return None;
        }
        let line = self
            .input
            .read_line(&mut self.conn, self.imap.limits.line_bytes)
            .ok()?;
        let text = String::from_utf8_lossy(&line).trim().to_string();
        (text != "*").then_some(text)
    }

    fn authenticate(&mut self, tag: &str, mechanism: &str, initial: Option<String>) -> Next {
        match mechanism {
            "PLAIN" => {
                let answer = match initial {
                    Some(text) => text,
                    None => match self.sasl_line("") {
                        Some(text) => text,
                        None => return self.bad(tag, "Sign-in cancelled"),
                    },
                };
                match crate::auth::decode_plain(&answer) {
                    Some((user, password)) => self.sign_in(tag, &user, &password),
                    None => self.sign_in(tag, b"", b""),
                }
            }
            "LOGIN" => {
                let user = match initial {
                    Some(text) => text,
                    // "Username:"
                    None => match self.sasl_line("VXNlcm5hbWU6") {
                        Some(text) => text,
                        None => return self.bad(tag, "Sign-in cancelled"),
                    },
                };
                // "Password:"
                let Some(password) = self.sasl_line("UGFzc3dvcmQ6") else {
                    return self.bad(tag, "Sign-in cancelled");
                };
                let user = crate::auth::decode_base64(&user).unwrap_or_default();
                let password = crate::auth::decode_base64(&password).unwrap_or_default();
                self.sign_in(tag, &user, &password)
            }
            _ => self.no(tag, "[CANNOT] Only PLAIN and LOGIN"),
        }
    }

    // ---- mailboxes ----

    fn selected(&self) -> Option<&Selected> {
        match &self.state {
            State::Selected(selected) => Some(selected),
            _ => None,
        }
    }

    /// The mailbox a client names (modified UTF-7; `INBOX` in any case).
    fn resolve(&self, raw: &[u8]) -> Result<Option<MailboxInfo>, StoreError> {
        let name = mutf7::decode(&String::from_utf8_lossy(raw));
        let name = name.trim_end_matches('/');
        let boxes = self.imap.store.mailboxes()?;
        if name.eq_ignore_ascii_case("INBOX") {
            return Ok(boxes.into_iter().find(|b| b.role == Role::Inbox));
        }
        Ok(boxes
            .iter()
            .find(|b| b.path == name)
            .or_else(|| boxes.iter().find(|b| b.path.eq_ignore_ascii_case(name)))
            .cloned())
    }

    /// A new mailbox's path from the name a client gives.
    fn new_path(raw: &[u8]) -> Result<String, StoreError> {
        let name = mutf7::decode(&String::from_utf8_lossy(raw));
        let name = name.trim_end_matches('/').to_string();
        if name.eq_ignore_ascii_case("INBOX") {
            return Err(StoreError::Exists(String::from("INBOX")));
        }
        store::check_mailbox_path(&name)?;
        Ok(name)
    }

    /// Whether `path` is the Drafts folder (its messages are `\Draft`).
    fn is_drafts(&self, path: &str) -> Result<bool, StoreError> {
        Ok(self
            .imap
            .store
            .mailboxes()?
            .iter()
            .any(|b| b.role == Role::Drafts && b.path == path))
    }

    /// The messages of `path` now: UIDs and flags (the drive's markers; `\Draft` in Drafts).
    fn snapshot(&self, path: &str) -> Result<(u32, u32, Vec<Msg>), StoreError> {
        let in_drafts = self.is_drafts(path)?;
        let messages: Vec<StoredMessage> = self.imap.store.messages(path)?;
        let names: Vec<String> = messages.iter().map(|m| m.name.clone()).collect();
        let numbered = self.imap.uids.number(path, &names);
        let marks = self.imap.marks()?;
        let by_name: HashMap<&str, &StoredMessage> =
            messages.iter().map(|m| (m.name.as_str(), m)).collect();
        let msgs = numbered
            .uids
            .iter()
            .filter_map(|(name, uid)| {
                let message = by_name.get(name.as_str())?;
                let flags = match marks.get(&message.id) {
                    Some(marks) => Flags::from_marks(marks, in_drafts),
                    None => Flags::from_marks(&Marks::default(), in_drafts),
                };
                Some(Msg {
                    uid: *uid,
                    name: name.clone(),
                    id: message.id.clone(),
                    size: message.size,
                    arrived: message.arrived,
                    flags,
                })
            })
            .collect();
        Ok((numbered.validity, numbered.next, msgs))
    }

    fn select(&mut self, tag: &str, raw: &[u8], read_only: bool) -> Next {
        // A failed SELECT leaves no mailbox selected (RFC 3501 6.3.1).
        self.state = State::Authenticated;
        let info = match self.resolve(raw) {
            Ok(Some(info)) => info,
            Ok(None) => return self.no(tag, "[NONEXISTENT] No such mailbox"),
            Err(e) => return self.store_no(tag, &e),
        };
        let (validity, next, msgs) = match self.snapshot(&info.path) {
            Ok(snapshot) => snapshot,
            Err(e) => return self.store_no(tag, &e),
        };
        let mut out = String::new();
        out.push_str("* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)\r\n");
        if read_only {
            out.push_str("* OK [PERMANENTFLAGS ()] Read-only\r\n");
        } else {
            // \Draft follows from the folder; keywords are kept (label markers).
            out.push_str("* OK [PERMANENTFLAGS (\\Answered \\Flagged \\Deleted \\Seen \\*)] Kept in the drive\r\n");
        }
        out.push_str(&format!("* {} EXISTS\r\n* 0 RECENT\r\n", msgs.len()));
        if let Some(first) = msgs.iter().position(|m| !m.flags.seen) {
            out.push_str(&format!("* OK [UNSEEN {}] First unseen\r\n", first + 1));
        }
        out.push_str(&format!("* OK [UIDVALIDITY {validity}] UIDs valid\r\n"));
        out.push_str(&format!("* OK [UIDNEXT {next}] Predicted next UID\r\n"));
        if !self.send(out.as_bytes()) {
            return Next::Close;
        }
        self.state = State::Selected(Selected {
            path: info.path,
            read_only,
            validity,
            msgs,
        });
        let code = if read_only { "[READ-ONLY]" } else { "[READ-WRITE]" };
        self.ok(tag, &format!("{code} Selected"))
    }

    fn create(&mut self, tag: &str, raw: &[u8]) -> Next {
        let path = match Self::new_path(raw) {
            Ok(path) => path,
            Err(e) => return self.store_no(tag, &e),
        };
        match self.imap.store.create_mailbox(&path) {
            Ok(()) => self.ok(tag, "CREATE completed"),
            Err(e) => self.store_no(tag, &e),
        }
    }

    fn delete(&mut self, tag: &str, raw: &[u8]) -> Next {
        let info = match self.resolve(raw) {
            Ok(Some(info)) => info,
            Ok(None) => return self.no(tag, "[NONEXISTENT] No such mailbox"),
            Err(e) => return self.store_no(tag, &e),
        };
        if info.role == Role::Inbox {
            return self.no(tag, "[CANNOT] The inbox cannot be deleted");
        }
        match self.imap.store.delete_mailbox(&info.path) {
            Ok(()) => self.ok(tag, "DELETE completed"),
            Err(e) => self.store_no(tag, &e),
        }
    }

    fn rename(&mut self, tag: &str, from: &[u8], to: &[u8]) -> Next {
        let info = match self.resolve(from) {
            Ok(Some(info)) => info,
            Ok(None) => return self.no(tag, "[NONEXISTENT] No such mailbox"),
            Err(e) => return self.store_no(tag, &e),
        };
        let path = match Self::new_path(to) {
            Ok(path) => path,
            Err(e) => return self.store_no(tag, &e),
        };
        match self.imap.store.rename_mailbox(&info.path, &path) {
            Ok(()) => self.ok(tag, "RENAME completed"),
            Err(e) => self.store_no(tag, &e),
        }
    }

    fn list(&mut self, tag: &str, reference: &[u8], pattern: &[u8], subscribed: bool) -> Next {
        let word = if subscribed { "LSUB" } else { "LIST" };
        if pattern.is_empty() {
            // The hierarchy delimiter and root.
            if !self.line(&format!("* {word} (\\Noselect) \"/\" \"\"")) {
                return Next::Close;
            }
            return self.ok(tag, &format!("{word} completed"));
        }
        let boxes = match self.imap.store.mailboxes() {
            Ok(boxes) => boxes,
            Err(e) => return self.store_no(tag, &e),
        };
        let mut full = reference.to_vec();
        full.extend_from_slice(pattern);
        let names: Vec<(String, &MailboxInfo)> = boxes.iter().map(|b| (imap_name(b), b)).collect();
        let mut out = Vec::new();
        for (name, info) in &names {
            let matched = if name == "INBOX" {
                wildcard(&full.to_ascii_uppercase(), b"INBOX")
            } else {
                wildcard(&full, name.as_bytes())
            };
            if !matched {
                continue;
            }
            let below = format!("{}/", info.path);
            let has_children = boxes.iter().any(|b| b.path.starts_with(&below));
            let mut attributes = vec![if has_children {
                "\\HasChildren"
            } else {
                "\\HasNoChildren"
            }];
            if let Some(attribute) = special_use(info.role) {
                attributes.push(attribute);
            }
            out.extend_from_slice(format!("* {word} ({}) \"/\" ", attributes.join(" ")).as_bytes());
            fetch::string(&mut out, name.as_bytes());
            out.extend_from_slice(b"\r\n");
        }
        if !self.send(&out) {
            return Next::Close;
        }
        self.ok(tag, &format!("{word} completed"))
    }

    fn status(&mut self, tag: &str, raw: &[u8], items: &[StatusItem]) -> Next {
        let info = match self.resolve(raw) {
            Ok(Some(info)) => info,
            Ok(None) => return self.no(tag, "[NONEXISTENT] No such mailbox"),
            Err(e) => return self.store_no(tag, &e),
        };
        let (validity, next, msgs) = match self.snapshot(&info.path) {
            Ok(snapshot) => snapshot,
            Err(e) => return self.store_no(tag, &e),
        };
        let values: Vec<String> = items
            .iter()
            .map(|item| match item {
                StatusItem::Messages => format!("MESSAGES {}", msgs.len()),
                StatusItem::Recent => String::from("RECENT 0"),
                StatusItem::UidNext => format!("UIDNEXT {next}"),
                StatusItem::UidValidity => format!("UIDVALIDITY {validity}"),
                StatusItem::Unseen => {
                    format!("UNSEEN {}", msgs.iter().filter(|m| !m.flags.seen).count())
                }
            })
            .collect();
        let mut out = b"* STATUS ".to_vec();
        fetch::string(&mut out, raw);
        out.extend_from_slice(format!(" ({})\r\n", values.join(" ")).as_bytes());
        if !self.send(&out) {
            return Next::Close;
        }
        self.ok(tag, "STATUS completed")
    }

    fn append(
        &mut self,
        tag: &str,
        raw: &[u8],
        flags: &[String],
        date: Option<i64>,
        message: &[u8],
    ) -> Next {
        let info = match self.resolve(raw) {
            Ok(Some(info)) => info,
            Ok(None) => return self.no(tag, "[TRYCREATE] No such mailbox"),
            Err(e) => return self.store_no(tag, &e),
        };
        // A mail program files its own copy of what it just sent: the bridge has filed the
        // copy that went out already.
        if info.role == Role::Sent {
            let filed = sent::message_id_of(message)
                .and_then(|id| self.imap.sent.find(&id))
                .filter(|(mailbox, _)| *mailbox == info.path);
            if let Some((_, name)) = filed {
                let code = self.uid_of(&info.path, &name).map_or(String::new(), |(validity, uid)| {
                    format!("[APPENDUID {validity} {uid}] ")
                });
                return self.ok(tag, &format!("{code}Already filed by the bridge"));
            }
        }
        let mut wanted = Flags::default();
        for flag in flags {
            wanted.set(flag, true);
        }
        let arrived = date
            .and_then(|d| u64::try_from(d).ok())
            .unwrap_or_else(store::now);
        let stored = match self.imap.store.append(&info.path, message, arrived, wanted.marks()) {
            Ok(stored) => stored,
            Err(e) => return self.store_no(tag, &e),
        };
        self.imap.marks_changed();
        let code = self
            .uid_of(&info.path, &stored.name)
            .map_or(String::new(), |(validity, uid)| format!("[APPENDUID {validity} {uid}] "));
        if self.selected().is_some_and(|s| s.path == info.path) {
            let updates = self.refresh();
            if !self.send(&updates) {
                return Next::Close;
            }
        }
        self.ok(tag, &format!("{code}APPEND completed"))
    }

    /// The UIDVALIDITY of `path` and the UID of its message `name`, numbering it now.
    fn uid_of(&self, path: &str, name: &str) -> Option<(u32, u32)> {
        let names: Vec<String> = self
            .imap
            .store
            .messages(path)
            .ok()?
            .into_iter()
            .map(|m| m.name)
            .collect();
        let numbered = self.imap.uids.number(path, &names);
        numbered
            .uids
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, uid)| (numbered.validity, *uid))
    }

    // ---- the selected mailbox ----

    /// What changed in the selected mailbox since the session last looked, as untagged
    /// responses (EXPUNGE, FETCH FLAGS, EXISTS); the session's view takes the changes.
    fn refresh(&mut self) -> Vec<u8> {
        let Some(path) = self.selected().map(|s| s.path.clone()) else {
            return Vec::new();
        };
        let Ok((validity, _next, fresh)) = self.snapshot(&path) else {
            return Vec::new();
        };
        let State::Selected(selected) = &mut self.state else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if validity != selected.validity {
            // The mailbox's UIDs were made anew: the client must select it again.
            out.extend_from_slice(b"* BYE The mailbox's UIDs changed; select it again\r\n");
            self.state = State::Logout;
            return out;
        }
        let present: HashSet<u32> = fresh.iter().map(|m| m.uid).collect();
        for i in (0..selected.msgs.len()).rev() {
            if !present.contains(&selected.msgs[i].uid) {
                out.extend_from_slice(format!("* {} EXPUNGE\r\n", i + 1).as_bytes());
                selected.msgs.remove(i);
            }
        }
        let place: HashMap<u32, usize> = selected
            .msgs
            .iter()
            .enumerate()
            .map(|(i, m)| (m.uid, i))
            .collect();
        let mut added = Vec::new();
        for msg in fresh {
            match place.get(&msg.uid) {
                Some(&i) => {
                    if selected.msgs[i].flags != msg.flags {
                        out.extend_from_slice(
                            format!(
                                "* {} FETCH (UID {} FLAGS {})\r\n",
                                i + 1,
                                msg.uid,
                                msg.flags.render()
                            )
                            .as_bytes(),
                        );
                        selected.msgs[i].flags = msg.flags;
                    }
                }
                None => added.push(msg),
            }
        }
        if !added.is_empty() {
            selected.msgs.extend(added);
            selected.msgs.sort_by_key(|m| m.uid);
            out.extend_from_slice(
                format!("* {} EXISTS\r\n* 0 RECENT\r\n", selected.msgs.len()).as_bytes(),
            );
        }
        out
    }

    /// The places of the selected messages a set names (by UID or by sequence number).
    fn targets(&self, uid: bool, set: &SequenceSet) -> Vec<usize> {
        let Some(selected) = self.selected() else {
            return Vec::new();
        };
        let count = selected.msgs.len();
        if count == 0 {
            return Vec::new();
        }
        let largest_uid = selected.msgs[count - 1].uid;
        (0..count)
            .filter(|&i| {
                if uid {
                    set.contains(selected.msgs[i].uid, largest_uid)
                } else {
                    set.contains(i as u32 + 1, count as u32)
                }
            })
            .collect()
    }

    fn cached(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        self.cache
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, bytes)| bytes.clone())
    }

    /// The whole message (kept for the next commands).
    fn read_whole(&mut self, path: &str, name: &str) -> Result<Arc<Vec<u8>>, StoreError> {
        let key = format!("{path}\0{name}");
        if let Some(bytes) = self.cached(&key) {
            return Ok(bytes);
        }
        let bytes = Arc::new(self.imap.store.read(path, name)?);
        self.cache.push((key, bytes.clone()));
        while self.cache.len() > CACHE_MESSAGES
            || self.cache.iter().map(|(_, b)| b.len()).sum::<usize>() > CACHE_BYTES
        {
            if self.cache.len() <= 1 {
                break;
            }
            self.cache.remove(0);
        }
        Ok(bytes)
    }

    /// Enough of the message for its header: the whole one when it is small or cached, else
    /// its first bytes (the whole one when the header is longer than those).
    fn read_header(&mut self, path: &str, name: &str, size: u64) -> Result<Arc<Vec<u8>>, StoreError> {
        let key = format!("{path}\0{name}");
        if let Some(bytes) = self.cached(&key) {
            return Ok(bytes);
        }
        if size <= HEAD_BYTES {
            return self.read_whole(path, name);
        }
        let head = self.imap.store.read_head(path, name, HEAD_BYTES)?;
        let (end, _) = mime::split(&head);
        if end < head.len() {
            Ok(Arc::new(head))
        } else {
            self.read_whole(path, name)
        }
    }

    fn fetch(&mut self, tag: &str, uid: bool, set: &SequenceSet, mut atts: Vec<FetchAtt>) -> Next {
        if uid && !atts.contains(&FetchAtt::Uid) {
            atts.insert(0, FetchAtt::Uid);
        }
        let needs_whole = atts.iter().any(|att| match att {
            FetchAtt::Body | FetchAtt::BodyStructure | FetchAtt::Rfc822 | FetchAtt::Rfc822Text => true,
            FetchAtt::Section { section, .. } => !fetch::header_only(section),
            _ => false,
        });
        let needs_header = atts
            .iter()
            .any(|att| matches!(att, FetchAtt::Envelope | FetchAtt::Rfc822Header | FetchAtt::Section { .. }));
        let marks_seen = atts.iter().any(|att| match att {
            FetchAtt::Rfc822 | FetchAtt::Rfc822Text => true,
            FetchAtt::Section { peek, .. } => !peek,
            _ => false,
        });
        let Some((path, read_only)) = self.selected().map(|s| (s.path.clone(), s.read_only)) else {
            return self.bad(tag, "Select a mailbox first");
        };
        for i in self.targets(uid, set) {
            let Some(msg) = self.selected().and_then(|s| s.msgs.get(i)).cloned() else {
                continue;
            };
            let bytes = if needs_whole {
                self.read_whole(&path, &msg.name).ok()
            } else if needs_header {
                self.read_header(&path, &msg.name, msg.size).ok()
            } else {
                None
            };
            if (needs_whole || needs_header) && bytes.is_none() {
                // Gone from the drive meanwhile: the next NOOP reports it expunged.
                continue;
            }
            let mut flags = msg.flags.clone();
            let mut show_flags = atts.contains(&FetchAtt::Flags);
            if marks_seen && !read_only && !flags.seen {
                if self.imap.store.set_mark(&msg.id, Mark::Seen, true).is_ok() {
                    self.imap.marks_changed();
                    flags.seen = true;
                    show_flags = true;
                    if let State::Selected(selected) = &mut self.state {
                        if let Some(m) = selected.msgs.get_mut(i) {
                            m.flags.seen = true;
                        }
                    }
                }
            }
            let response = self.fetch_response(i + 1, &msg, &flags, show_flags, &atts, bytes.as_deref());
            if !self.send(&response) {
                return Next::Close;
            }
        }
        self.ok(tag, "FETCH completed")
    }

    fn fetch_response(
        &self,
        seq: usize,
        msg: &Msg,
        flags: &Flags,
        show_flags: bool,
        atts: &[FetchAtt],
        bytes: Option<&Vec<u8>>,
    ) -> Vec<u8> {
        let empty = Vec::new();
        let bytes: &[u8] = bytes.map_or(&empty[..], |b| &b[..]);
        let root = mime::parse(bytes);
        let (header_end, body_start) = mime::split(bytes);
        let mut out = format!("* {seq} FETCH (").into_bytes();
        let mut items: Vec<Vec<u8>> = Vec::new();
        if show_flags && !atts.contains(&FetchAtt::Flags) {
            items.push(format!("FLAGS {}", flags.render()).into_bytes());
        }
        for att in atts {
            let mut item = Vec::new();
            match att {
                FetchAtt::Flags => item.extend_from_slice(format!("FLAGS {}", flags.render()).as_bytes()),
                FetchAtt::Uid => item.extend_from_slice(format!("UID {}", msg.uid).as_bytes()),
                FetchAtt::InternalDate => item.extend_from_slice(
                    format!(
                        "INTERNALDATE \"{}\"",
                        dates::imap_datetime(i64::try_from(msg.arrived).unwrap_or(0))
                    )
                    .as_bytes(),
                ),
                FetchAtt::Rfc822Size => {
                    item.extend_from_slice(format!("RFC822.SIZE {}", msg.size).as_bytes());
                }
                FetchAtt::Envelope => {
                    item.extend_from_slice(b"ENVELOPE ");
                    fetch::envelope(&mut item, &bytes[..header_end]);
                }
                FetchAtt::Body => {
                    item.extend_from_slice(b"BODY ");
                    fetch::body_structure(&mut item, bytes, &root, false);
                }
                FetchAtt::BodyStructure => {
                    item.extend_from_slice(b"BODYSTRUCTURE ");
                    fetch::body_structure(&mut item, bytes, &root, true);
                }
                FetchAtt::Rfc822 => {
                    item.extend_from_slice(b"RFC822 ");
                    fetch::literal(&mut item, bytes);
                }
                FetchAtt::Rfc822Header => {
                    item.extend_from_slice(b"RFC822.HEADER ");
                    fetch::literal(&mut item, &bytes[..body_start.min(bytes.len())]);
                }
                FetchAtt::Rfc822Text => {
                    item.extend_from_slice(b"RFC822.TEXT ");
                    fetch::literal(&mut item, &bytes[body_start.min(bytes.len())..]);
                }
                FetchAtt::Section {
                    section, partial, ..
                } => {
                    let data = fetch::section_bytes(bytes, &root, section).unwrap_or_default();
                    item.extend_from_slice(format!("BODY[{}]", section.spec()).as_bytes());
                    let data: &[u8] = match partial {
                        Some((start, length)) => {
                            item.extend_from_slice(format!("<{start}>").as_bytes());
                            fetch::partial(&data, *start, *length)
                        }
                        None => &data,
                    };
                    item.push(b' ');
                    fetch::literal(&mut item, data);
                }
            }
            items.push(item);
        }
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                out.push(b' ');
            }
            out.extend_from_slice(item);
        }
        out.extend_from_slice(b")\r\n");
        out
    }

    fn store(
        &mut self,
        tag: &str,
        uid: bool,
        set: &SequenceSet,
        mode: StoreMode,
        silent: bool,
        flags: &[String],
    ) -> Next {
        let Some(read_only) = self.selected().map(|s| s.read_only) else {
            return self.bad(tag, "Select a mailbox first");
        };
        if read_only {
            return self.no(tag, "[READ-ONLY] The mailbox is read-only");
        }
        let mut out = Vec::new();
        for i in self.targets(uid, set) {
            let Some(msg) = self.selected().and_then(|s| s.msgs.get(i)).cloned() else {
                continue;
            };
            let mut new = match mode {
                StoreMode::Replace => Flags::default(),
                StoreMode::Add | StoreMode::Remove => msg.flags.clone(),
            };
            for flag in flags {
                new.set(flag, mode != StoreMode::Remove);
            }
            // \Draft follows from the folder: a STORE does not change it.
            new.draft = msg.flags.draft;
            if let Err(e) = self.imap.write_marks(&msg.id, &msg.flags, &new) {
                let _ = self.send(&out);
                return self.store_no(tag, &e);
            }
            if !silent {
                let uid_part = if uid { format!(" UID {}", msg.uid) } else { String::new() };
                out.extend_from_slice(
                    format!("* {} FETCH (FLAGS {}{uid_part})\r\n", i + 1, new.render()).as_bytes(),
                );
            }
            if let State::Selected(selected) = &mut self.state {
                if let Some(m) = selected.msgs.get_mut(i) {
                    m.flags = new;
                }
            }
        }
        if !self.send(&out) {
            return Next::Close;
        }
        self.ok(tag, "STORE completed")
    }

    fn search(&mut self, tag: &str, uid: bool, key: &SearchKey) -> Next {
        let Some((path, msgs)) = self.selected().map(|s| (s.path.clone(), s.msgs.clone())) else {
            return self.bad(tag, "Select a mailbox first");
        };
        let need = search::need(key);
        let largest = Largest {
            seq: msgs.len() as u32,
            uid: msgs.last().map_or(0, |m| m.uid),
        };
        let mut found = Vec::new();
        for (i, msg) in msgs.iter().enumerate() {
            let bytes = match need {
                Need::Nothing => None,
                Need::Header => self.read_header(&path, &msg.name, msg.size).ok(),
                Need::Whole => self.read_whole(&path, &msg.name).ok(),
            };
            let candidate = Candidate {
                seq: i as u32 + 1,
                uid: msg.uid,
                size: msg.size,
                arrived: i64::try_from(msg.arrived).unwrap_or(0),
                flags: &msg.flags,
                bytes: bytes.as_deref().map(|b| &b[..]),
            };
            if search::matches(key, &candidate, largest) {
                found.push(if uid { msg.uid } else { i as u32 + 1 });
            }
        }
        let mut line = String::from("* SEARCH");
        for n in found {
            line.push(' ');
            line.push_str(&n.to_string());
        }
        if !self.line(&line) {
            return Next::Close;
        }
        self.ok(tag, "SEARCH completed")
    }

    /// Removes the selected mailbox's `\Deleted` messages (those `uids` names, for UID
    /// EXPUNGE), answering with EXPUNGE responses when `tell`. `Err(None)`: the connection
    /// failed.
    fn expunge_deleted(&mut self, uids: Option<&SequenceSet>, tell: bool) -> Result<(), Option<StoreError>> {
        let Some(selected) = self.selected() else {
            return Ok(());
        };
        let path = selected.path.clone();
        let largest = selected.msgs.last().map_or(0, |m| m.uid);
        let doomed: Vec<usize> = selected
            .msgs
            .iter()
            .enumerate()
            .filter(|(_, m)| m.flags.deleted && uids.is_none_or(|set| set.contains(m.uid, largest)))
            .map(|(i, _)| i)
            .collect();
        if doomed.is_empty() {
            return Ok(());
        }
        let names: Vec<String> = doomed
            .iter()
            .filter_map(|&i| self.selected().map(|s| s.msgs[i].name.clone()))
            .collect();
        self.imap.store.expunge(&path, &names).map_err(Some)?;
        self.imap.marks_changed();
        let mut out = Vec::new();
        if let State::Selected(selected) = &mut self.state {
            for &i in doomed.iter().rev() {
                selected.msgs.remove(i);
                out.extend_from_slice(format!("* {} EXPUNGE\r\n", i + 1).as_bytes());
            }
        }
        if tell && !self.send(&out) {
            return Err(None);
        }
        Ok(())
    }

    fn copy_or_move(
        &mut self,
        tag: &str,
        uid: bool,
        set: &SequenceSet,
        raw: &[u8],
        moving: bool,
    ) -> Next {
        let Some((source, read_only)) = self.selected().map(|s| (s.path.clone(), s.read_only)) else {
            return self.bad(tag, "Select a mailbox first");
        };
        if moving && read_only {
            return self.no(tag, "[READ-ONLY] The mailbox is read-only");
        }
        let target = match self.resolve(raw) {
            Ok(Some(info)) => info,
            Ok(None) => return self.no(tag, "[TRYCREATE] No such mailbox"),
            Err(e) => return self.store_no(tag, &e),
        };
        let places = self.targets(uid, set);
        let msgs: Vec<Msg> = places
            .iter()
            .filter_map(|&i| self.selected().and_then(|s| s.msgs.get(i)).cloned())
            .collect();
        // Each message's source UID and its name in the target.
        let mut copied: Vec<(u32, String)> = Vec::new();
        for msg in &msgs {
            let name = if target.path == source {
                if moving {
                    // A move into the mailbox it is in: nothing to do.
                    msg.name.clone()
                } else {
                    // A copy into the same mailbox is a new message under a new name, never the
                    // object copied onto itself (S3 refuses that, and it would add nothing).
                    let copy = self
                        .imap
                        .store
                        .read(&source, &msg.name)
                        .and_then(|bytes| {
                            self.imap
                                .store
                                .append(&source, &bytes, store::now(), msg.flags.marks())
                        });
                    match copy {
                        Ok(stored) => {
                            self.imap.marks_changed();
                            stored.name
                        }
                        Err(e) => return self.store_no(tag, &e),
                    }
                }
            } else {
                let done = if moving {
                    self.imap.store.move_to(&source, &msg.name, &target.path)
                } else {
                    self.imap.store.copy(&source, &msg.name, &target.path)
                };
                if let Err(e) = done {
                    return self.store_no(tag, &e);
                }
                msg.name.clone()
            };
            copied.push((msg.uid, name));
        }
        // COPYUID: the source UIDs and the ones the messages have in the target now.
        let mut code = String::new();
        if !copied.is_empty() {
            let names: Vec<String> = match self.imap.store.messages(&target.path) {
                Ok(listed) => listed.into_iter().map(|m| m.name).collect(),
                Err(_) => Vec::new(),
            };
            let numbered = self.imap.uids.number(&target.path, &names);
            let target_uid: HashMap<&str, u32> =
                numbered.uids.iter().map(|(n, u)| (n.as_str(), *u)).collect();
            let pairs: Vec<(u32, u32)> = copied
                .iter()
                .filter_map(|(uid, name)| target_uid.get(name.as_str()).map(|t| (*uid, *t)))
                .collect();
            if !pairs.is_empty() {
                let from: Vec<String> = pairs.iter().map(|(s, _)| s.to_string()).collect();
                let to: Vec<String> = pairs.iter().map(|(_, t)| t.to_string()).collect();
                code = format!(
                    "[COPYUID {} {} {}] ",
                    numbered.validity,
                    from.join(","),
                    to.join(",")
                );
            }
        }
        if !moving || target.path == source {
            if self.selected().is_some_and(|s| s.path == target.path) {
                let updates = self.refresh();
                if !self.send(&updates) {
                    return Next::Close;
                }
            }
            let word = if moving { "MOVE" } else { "COPY" };
            return self.ok(tag, &format!("{code}{word} completed"));
        }
        // MOVE: the COPYUID first, then the moved messages leave this mailbox.
        let mut out = Vec::new();
        if !code.is_empty() {
            out.extend_from_slice(format!("* OK {}Moved\r\n", code).as_bytes());
        }
        if let State::Selected(selected) = &mut self.state {
            let mut places = places;
            places.sort_unstable();
            for &i in places.iter().rev() {
                if i < selected.msgs.len() {
                    selected.msgs.remove(i);
                    out.extend_from_slice(format!("* {} EXPUNGE\r\n", i + 1).as_bytes());
                }
            }
        }
        if !self.send(&out) {
            return Next::Close;
        }
        self.ok(tag, "MOVE completed")
    }

    // ---- IDLE ----

    fn idle(&mut self, tag: &str) -> Next {
        if !self.line("+ idling") {
            return Next::Close;
        }
        let poll = self.imap.limits.idle_poll;
        let limit = self.imap.limits.imap_idle;
        let _ = self.conn.set_read_timeout(Some(poll));
        let started = Instant::now();
        let next = loop {
            match self.input.read_line(&mut self.conn, self.imap.limits.line_bytes) {
                Ok(line) => {
                    let _ = self.conn.set_read_timeout(Some(limit));
                    if line.eq_ignore_ascii_case(b"DONE") {
                        break self.ok(tag, "IDLE terminated");
                    }
                    break self.bad(tag, "Expected DONE");
                }
                Err(ReadError::Timeout) => {
                    if started.elapsed() > limit {
                        let _ = self.line("* BYE Autologout: idle for too long");
                        break Next::Close;
                    }
                    let updates = self.refresh();
                    if !updates.is_empty() && !self.send(&updates) {
                        break Next::Close;
                    }
                    if matches!(self.state, State::Logout) {
                        break Next::Close;
                    }
                }
                Err(_) => break Next::Close,
            }
        };
        let _ = self.conn.set_read_timeout(Some(limit));
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_wildcards_match_across_or_within_one_level() {
        assert!(wildcard(b"*", b"Work/Projects"));
        assert!(wildcard(b"%", b"Work"));
        assert!(!wildcard(b"%", b"Work/Projects"));
        assert!(wildcard(b"Work/%", b"Work/Projects"));
        assert!(wildcard(b"W*s", b"Work/Projects"));
        assert!(!wildcard(b"W%s", b"Work/Projects"));
        assert!(wildcard(b"INBOX", b"INBOX"));
        assert!(!wildcard(b"INBOX", b"INBOX2"));
        assert!(wildcard(b"", b""));
        assert!(!wildcard(b"", b"x"));
    }

    #[test]
    fn mailbox_names_are_inbox_or_the_path_in_modified_utf_7() {
        let info = |path: &str, role| MailboxInfo {
            path: path.to_string(),
            role,
        };
        assert_eq!(imap_name(&info("Inbox", Role::Inbox)), "INBOX");
        assert_eq!(imap_name(&info("Entwürfe", Role::Other)), "Entw&APw-rfe");
        assert_eq!(imap_name(&info("Work/Projects", Role::Other)), "Work/Projects");
        assert_eq!(special_use(Role::Spam), Some("\\Junk"));
        assert_eq!(special_use(Role::Other), None);
    }
}
