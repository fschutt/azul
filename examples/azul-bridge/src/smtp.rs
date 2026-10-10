//! SMTP submission (RFC 6409) on 127.0.0.1: a mail program hands over a message with its
//! envelope, signed in with the bridge's user and password, and the bridge hands it to a
//! [`Submitter`] - AzMail's own sending path ([`crate::sender`]) - and answers with what
//! became of it.
//!
//! Offered after EHLO: `SIZE`, `8BITMIME`, `SMTPUTF8`, `ENHANCEDSTATUSCODES`, `PIPELINING`,
//! `CHUNKING` (BDAT, RFC 3030), `AUTH PLAIN LOGIN`. MAIL needs a sign-in; the sender must be one of the account's
//! addresses (the DKIM key signs for that domain only); at most so many recipients; the
//! message at most so big. No STARTTLS: the connection never leaves this computer.

use std::{net::TcpListener, sync::Arc};

use crate::{
    auth::{self, Credentials, FailureGate},
    limits::Limits,
    net::{self, looks_like_http, Conn, Input, RateLimiter, ReadError},
};

/// A mail as the submission port took it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submission {
    /// `MAIL FROM`.
    pub from: String,
    /// Every `RCPT TO`.
    pub recipients: Vec<String>,
    /// The message, dot-stuffing undone, CRLF line ends.
    pub message: Vec<u8>,
}

/// What became of a submitted mail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Taken: sent, or queued to be tried again (the text says which).
    Accepted(String),
    /// Not taken now; the program may try again later.
    Temporary(String),
    /// Not taken, and trying again will not help.
    Refused(String),
}

/// Where submitted mail goes.
pub trait Submitter: Send + Sync {
    /// Delivers (or queues) `submission`. Blocking: the mail program waits for the answer.
    fn submit(&self, submission: &Submission) -> Verdict;
}

/// Unknown or out-of-order commands one connection may send before it is closed.
pub const MAX_ERRORS: u32 = 20;

/// The submission server: what every session shares.
pub struct Smtp {
    pub submitter: Arc<dyn Submitter>,
    pub credentials: Credentials,
    pub gate: Arc<FailureGate>,
    pub limits: Limits,
    /// The addresses MAIL FROM may name (any case); empty: any.
    pub senders: Vec<String>,
}

impl std::fmt::Debug for Smtp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Smtp")
            .field("credentials", &self.credentials)
            .field("senders", &self.senders)
            .finish_non_exhaustive()
    }
}

/// `<ada@example.org> SIZE=123` after `FROM:` / `TO:`: the address and the parameters.
fn path_and_params(argument: &str) -> Option<(String, Vec<(String, String)>)> {
    let argument = argument.trim_start();
    let (path, rest) = if let Some(inner) = argument.strip_prefix('<') {
        let end = inner.find('>')?;
        (inner[..end].to_string(), &inner[end + 1..])
    } else {
        // A path without brackets, as some programs send it.
        let end = argument.find(' ').unwrap_or(argument.len());
        (argument[..end].to_string(), &argument[end..])
    };
    let params = rest
        .split_whitespace()
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (k.to_ascii_uppercase(), v.to_string()),
            None => (p.to_ascii_uppercase(), String::new()),
        })
        .collect();
    // `@relay:user@host`: an old source route goes.
    let path = path.rsplit(':').next().unwrap_or_default().trim().to_string();
    Some((path, params))
}

/// Whether `address` looks like `local@domain` (no blanks, no brackets, one domain).
fn plausible(address: &str) -> bool {
    match address.rsplit_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && !domain.is_empty()
                && !address.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>'))
        }
        None => false,
    }
}

struct Session<'s, C: Conn> {
    smtp: &'s Smtp,
    conn: C,
    input: Input,
    rate: RateLimiter,
    signed_in: bool,
    failures: u32,
    errors: u32,
    from: Option<String>,
    recipients: Vec<String>,
    /// CHUNKING: the octets of the BDAT chunks so far (`Some` once a BDAT came).
    chunks: Option<Vec<u8>>,
}

impl<'s, C: Conn> Session<'s, C> {
    fn reply(&mut self, text: &str) -> bool {
        net::send(&mut self.conn, format!("{text}\r\n").as_bytes()).is_ok()
    }

    fn reset(&mut self) {
        self.from = None;
        self.recipients.clear();
        self.chunks = None;
    }

    /// An out-of-order or unknown command: answered, counted; `false` ends the session.
    fn error(&mut self, text: &str) -> bool {
        self.errors += 1;
        if self.errors > MAX_ERRORS {
            let _ = self.reply("421 4.7.0 Too many errors, closing");
            return false;
        }
        self.reply(text)
    }

    fn run(mut self) {
        let _ = self.conn.set_read_timeout(Some(self.smtp.limits.smtp_idle));
        if !self.reply("220 localhost ESMTP Azlin Bridge ready") {
            return;
        }
        let mut first = true;
        loop {
            let line = match self.input.read_line(&mut self.conn, self.smtp.limits.line_bytes) {
                Ok(line) => line,
                Err(ReadError::Timeout) => {
                    let _ = self.reply("421 4.4.2 Idle for too long, closing");
                    return;
                }
                Err(ReadError::TooLong) => {
                    let _ = self.reply("500 5.5.6 Line too long, closing");
                    return;
                }
                Err(_) => return,
            };
            if first && looks_like_http(&line) {
                return;
            }
            first = false;
            self.rate.take();
            let text = String::from_utf8_lossy(&line).into_owned();
            let (verb, argument) = match text.split_once(' ') {
                Some((verb, argument)) => (verb.to_ascii_uppercase(), argument.to_string()),
                None => (text.trim().to_ascii_uppercase(), String::new()),
            };
            let go_on = match verb.as_str() {
                "EHLO" => self.ehlo(),
                "HELO" => self.reply("250 localhost"),
                "AUTH" => self.auth(&argument),
                "MAIL" => self.mail(&argument),
                "RCPT" => self.rcpt(&argument),
                "DATA" => self.data(),
                "BDAT" => self.bdat(&argument),
                "RSET" => {
                    self.reset();
                    self.reply("250 2.0.0 OK")
                }
                "NOOP" => self.reply("250 2.0.0 OK"),
                "VRFY" => self.reply("252 2.5.0 Cannot verify the address, send and see"),
                "HELP" => self.reply("214 2.0.0 EHLO AUTH MAIL RCPT DATA BDAT RSET NOOP QUIT"),
                "QUIT" => {
                    let _ = self.reply("221 2.0.0 Bye");
                    return;
                }
                "STARTTLS" => self.error(
                    "502 5.5.1 TLS is not offered: the bridge only listens on this computer's own address",
                ),
                _ => self.error("500 5.5.2 Unknown command"),
            };
            if !go_on {
                return;
            }
        }
    }

    fn ehlo(&mut self) -> bool {
        self.reset();
        let lines = format!(
            "250-localhost Hello\r\n\
             250-SIZE {}\r\n\
             250-8BITMIME\r\n\
             250-SMTPUTF8\r\n\
             250-ENHANCEDSTATUSCODES\r\n\
             250-PIPELINING\r\n\
             250-CHUNKING\r\n\
             250 AUTH PLAIN LOGIN",
            self.smtp.limits.message_bytes
        );
        self.reply(&lines)
    }

    /// One SASL answer: `None` when the client cancelled (`*`) or went away.
    fn sasl_line(&mut self, challenge: &str) -> Option<String> {
        if !self.reply(&format!("334 {challenge}")) {
            return None;
        }
        let line = self
            .input
            .read_line(&mut self.conn, self.smtp.limits.line_bytes)
            .ok()?;
        let text = String::from_utf8_lossy(&line).trim().to_string();
        (text != "*").then_some(text)
    }

    fn auth(&mut self, argument: &str) -> bool {
        if self.signed_in {
            return self.error("503 5.5.1 Already signed in");
        }
        let mut words = argument.split_whitespace();
        let mechanism = words.next().unwrap_or_default().to_ascii_uppercase();
        let initial = words.next().map(str::to_string);
        let credentials = match mechanism.as_str() {
            "PLAIN" => {
                let answer = match initial.filter(|i| i != "=") {
                    Some(text) => Some(text),
                    None => self.sasl_line(""),
                };
                match answer {
                    Some(text) => auth::decode_plain(&text).unwrap_or_default(),
                    None => return self.reply("501 5.0.0 Sign-in cancelled"),
                }
            }
            "LOGIN" => {
                let user = match initial {
                    Some(text) => Some(text),
                    None => self.sasl_line("VXNlcm5hbWU6"),
                };
                let Some(user) = user else {
                    return self.reply("501 5.0.0 Sign-in cancelled");
                };
                let Some(password) = self.sasl_line("UGFzc3dvcmQ6") else {
                    return self.reply("501 5.0.0 Sign-in cancelled");
                };
                (
                    auth::decode_base64(&user).unwrap_or_default(),
                    auth::decode_base64(&password).unwrap_or_default(),
                )
            }
            _ => return self.error("504 5.5.4 Only PLAIN and LOGIN"),
        };
        self.smtp.gate.before_attempt();
        if self.smtp.credentials.check(&credentials.0, &credentials.1) {
            self.signed_in = true;
            return self.reply("235 2.7.0 Signed in");
        }
        self.smtp.gate.failed();
        self.failures += 1;
        if self.failures >= self.smtp.limits.auth_failures_per_connection {
            let _ = self.reply("535 5.7.8 Invalid credentials");
            let _ = self.reply("421 4.7.0 Too many failed sign-ins, closing");
            return false;
        }
        self.reply("535 5.7.8 Invalid credentials")
    }

    fn mail(&mut self, argument: &str) -> bool {
        if !self.signed_in {
            return self.error("530 5.7.0 Sign in first (AUTH)");
        }
        if self.from.is_some() {
            return self.error("503 5.5.1 MAIL was given already");
        }
        let Some(rest) = argument
            .get(..5)
            .filter(|p| p.eq_ignore_ascii_case("FROM:"))
            .map(|_| &argument[5..])
        else {
            return self.error("501 5.5.4 Syntax: MAIL FROM:<address>");
        };
        let Some((from, params)) = path_and_params(rest) else {
            return self.error("501 5.5.4 Syntax: MAIL FROM:<address>");
        };
        if !plausible(&from) {
            return self.reply("550 5.1.7 The sender's address is not an address");
        }
        let allowed = self.smtp.senders.is_empty()
            || self.smtp.senders.iter().any(|s| s.eq_ignore_ascii_case(&from));
        if !allowed {
            return self.reply(&format!(
                "550 5.7.1 This account does not send as <{from}>"
            ));
        }
        if let Some((_, size)) = params.iter().find(|(k, _)| k == "SIZE") {
            let too_big = size
                .parse::<usize>()
                .map_or(false, |n| n > self.smtp.limits.message_bytes);
            if too_big {
                return self.reply("552 5.3.4 The message is bigger than the bridge takes");
            }
        }
        self.from = Some(from);
        self.reply("250 2.1.0 OK")
    }

    fn rcpt(&mut self, argument: &str) -> bool {
        if self.from.is_none() {
            return self.error("503 5.5.1 MAIL first");
        }
        let Some(rest) = argument
            .get(..3)
            .filter(|p| p.eq_ignore_ascii_case("TO:"))
            .map(|_| &argument[3..])
        else {
            return self.error("501 5.5.4 Syntax: RCPT TO:<address>");
        };
        let Some((to, _)) = path_and_params(rest) else {
            return self.error("501 5.5.4 Syntax: RCPT TO:<address>");
        };
        if !plausible(&to) {
            return self.reply("501 5.1.3 The recipient's address is not an address");
        }
        if self.recipients.len() >= self.smtp.limits.recipients {
            return self.reply("452 4.5.3 Too many recipients");
        }
        self.recipients.push(to);
        self.reply("250 2.1.5 OK")
    }

    fn data(&mut self) -> bool {
        let Some(from) = self.from.clone() else {
            return self.error("503 5.5.1 MAIL first");
        };
        if self.recipients.is_empty() {
            return self.error("503 5.5.1 RCPT first");
        }
        if self.chunks.is_some() {
            return self.error("503 5.5.1 BDAT began this message: end it with BDAT ... LAST");
        }
        if !self.reply("354 Send the message, end it with <CRLF>.<CRLF>") {
            return false;
        }
        let max = self.smtp.limits.message_bytes;
        let line_max = self.smtp.limits.line_bytes.max(1024 * 1024);
        let mut message: Vec<u8> = Vec::new();
        let mut too_big = false;
        loop {
            let line = match self.input.read_line(&mut self.conn, line_max) {
                Ok(line) => line,
                Err(_) => return false,
            };
            if line == b"." {
                break;
            }
            let line = line.strip_prefix(b".").unwrap_or(&line);
            if message.len() + line.len() + 2 > max {
                // Read to the end all the same, so the answer comes in step.
                too_big = true;
                continue;
            }
            message.extend_from_slice(line);
            message.extend_from_slice(b"\r\n");
        }
        let recipients = std::mem::take(&mut self.recipients);
        self.reset();
        if too_big {
            return self.reply("552 5.3.4 The message is bigger than the bridge takes");
        }
        self.submit(from, recipients, message)
    }

    /// Hands the message over and answers with what became of it.
    fn submit(&mut self, from: String, recipients: Vec<String>, message: Vec<u8>) -> bool {
        let submission = Submission {
            from,
            recipients,
            message,
        };
        match self.smtp.submitter.submit(&submission) {
            Verdict::Accepted(text) => self.reply(&format!("250 2.0.0 {}", one_line(&text))),
            Verdict::Temporary(text) => self.reply(&format!("451 4.3.0 {}", one_line(&text))),
            Verdict::Refused(text) => self.reply(&format!("554 5.0.0 {}", one_line(&text))),
        }
    }

    /// `BDAT <size> [LAST]` (RFC 3030): exactly `size` octets of the message, as they are; the
    /// last chunk sends it. The octets are read whatever the answer, so the next command comes
    /// in step; a message over the limit ends the transaction (552).
    fn bdat(&mut self, argument: &str) -> bool {
        let mut words = argument.split_whitespace();
        let Some(size) = words.next().and_then(|n| n.parse::<u64>().ok()) else {
            return self.error("501 5.5.4 BDAT takes the chunk's size in octets");
        };
        let last = match words.next() {
            None => false,
            Some(word) if word.eq_ignore_ascii_case("LAST") && words.next().is_none() => true,
            Some(_) => return self.error("501 5.5.4 BDAT takes a size and LAST"),
        };
        let max = self.smtp.limits.message_bytes as u64;
        let ready = self.from.is_some() && !self.recipients.is_empty();
        let so_far = self.chunks.as_ref().map_or(0, Vec::len) as u64;
        let fits = ready && so_far.saturating_add(size) <= max;
        if fits {
            let Ok(bytes) = self.input.read_bytes(&mut self.conn, size as usize) else {
                return false;
            };
            self.chunks.get_or_insert_with(Vec::new).extend_from_slice(&bytes);
        } else {
            let mut left = size;
            while left > 0 {
                let n = left.min(64 * 1024) as usize;
                if self.input.read_bytes(&mut self.conn, n).is_err() {
                    return false;
                }
                left -= n as u64;
            }
        }
        let Some(from) = self.from.clone() else {
            return self.error("503 5.5.1 MAIL first");
        };
        if self.recipients.is_empty() {
            return self.error("503 5.5.1 RCPT first");
        }
        if !fits {
            self.reset();
            return self.reply("552 5.3.4 The message is bigger than the bridge takes");
        }
        if !last {
            return self.reply(&format!("250 2.0.0 {size} octets taken"));
        }
        let message = self.chunks.take().unwrap_or_default();
        let recipients = std::mem::take(&mut self.recipients);
        self.reset();
        self.submit(from, recipients, message)
    }
}

/// A reply's text on one line (a reason may carry line ends).
fn one_line(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = joined.as_str();
    if trimmed.len() > 400 {
        let mut cut = 400;
        while !trimmed.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}...", &trimmed[..cut])
    } else {
        trimmed.to_string()
    }
}

impl Smtp {
    /// Serves one connection until it ends.
    pub fn handle<C: Conn>(&self, conn: C) {
        Session {
            rate: RateLimiter::new(
                self.limits.commands_per_second,
                self.limits.command_burst,
            ),
            smtp: self,
            conn,
            input: Input::new(),
            signed_in: false,
            failures: 0,
            errors: 0,
            from: None,
            recipients: Vec::new(),
            chunks: None,
        }
        .run();
    }
}

fn busy(stream: &mut std::net::TcpStream) {
    let _ = net::send(stream, b"421 4.3.2 Too many connections to the Azlin Bridge\r\n");
}

/// Serves SMTP submission on `listener` until it fails.
pub fn serve(smtp: Arc<Smtp>, listener: TcpListener) {
    let max = smtp.limits.max_connections;
    let handler: net::Handler = Arc::new(move |stream| smtp.handle(stream));
    net::serve(listener, max, handler, busy);
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        net::TcpStream,
        sync::Mutex,
        time::Duration,
    };

    use super::*;
    use crate::net::bind_loopback;

    const USER: &str = "ada@example.org";
    const PASSWORD: &str = "k7m2p-9qxat-4ds8w-hb3zn-e6r1v";

    /// Takes every mail and answers with `verdict`.
    struct Fake {
        verdict: Verdict,
        seen: Mutex<Vec<Submission>>,
    }

    impl Submitter for Fake {
        fn submit(&self, submission: &Submission) -> Verdict {
            self.seen.lock().unwrap().push(submission.clone());
            self.verdict.clone()
        }
    }

    fn server(verdict: Verdict, limits: Limits) -> (Arc<Smtp>, Arc<Fake>) {
        let fake = Arc::new(Fake {
            verdict,
            seen: Mutex::new(Vec::new()),
        });
        let smtp = Arc::new(Smtp {
            submitter: fake.clone(),
            credentials: Credentials::new(USER, PASSWORD),
            gate: Arc::new(FailureGate::new(100, Duration::from_secs(60), Duration::ZERO)),
            limits,
            senders: vec![USER.to_string()],
        });
        (smtp, fake)
    }

    struct Client {
        stream: TcpStream,
        input: Input,
    }

    impl Client {
        fn connect(smtp: Arc<Smtp>) -> Client {
            let listener = bind_loopback(0).unwrap();
            let addr = listener.local_addr().unwrap();
            std::thread::spawn(move || {
                if let Ok((stream, _)) = listener.accept() {
                    smtp.handle(stream);
                }
            });
            let stream = TcpStream::connect(addr).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            let mut client = Client {
                stream,
                input: Input::new(),
            };
            assert!(client.reply().starts_with("220 "));
            client
        }

        /// One reply, its continuation lines joined with `\n`.
        fn reply(&mut self) -> String {
            let mut lines = Vec::new();
            loop {
                let line = self.input.read_line(&mut self.stream, 4096).expect("a reply");
                let line = String::from_utf8_lossy(&line).into_owned();
                let more = line.as_bytes().get(3) == Some(&b'-');
                lines.push(line);
                if !more {
                    return lines.join("\n");
                }
            }
        }

        fn say(&mut self, line: &str) -> String {
            self.stream.write_all(format!("{line}\r\n").as_bytes()).unwrap();
            self.reply()
        }

        fn signed_in(smtp: Arc<Smtp>) -> Client {
            let mut client = Client::connect(smtp);
            assert!(client.say("EHLO mail.local").contains("AUTH PLAIN LOGIN"));
            let plain = auth::encode_base64(format!("\0{USER}\0{PASSWORD}").as_bytes());
            assert!(client.say(&format!("AUTH PLAIN {plain}")).starts_with("235 "));
            client
        }
    }

    const MESSAGE: &str = "From: Ada <ada@example.org>\r\nTo: ben@example.net\r\nSubject: Hi\r\n\r\nLine one\r\n..dotted\r\n";

    #[test]
    fn a_signed_in_program_submits_and_the_message_arrives_unstuffed() {
        let (smtp, fake) = server(Verdict::Accepted(String::from("sent")), Limits::default());
        let mut client = Client::signed_in(smtp);
        assert!(client.say("MAIL FROM:<Ada@Example.org> SIZE=120 BODY=8BITMIME").starts_with("250 "));
        assert!(client.say("RCPT TO:<ben@example.net>").starts_with("250 "));
        assert!(client.say("RCPT TO:<hidden@example.com>").starts_with("250 "));
        assert!(client.say("DATA").starts_with("354 "));
        let reply = client.say(&format!("{MESSAGE}."));
        assert_eq!(reply, "250 2.0.0 sent");
        let seen = fake.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].from, "Ada@Example.org");
        assert_eq!(seen[0].recipients, vec!["ben@example.net", "hidden@example.com"]);
        assert_eq!(
            String::from_utf8_lossy(&seen[0].message),
            MESSAGE.replace("\r\n..dotted", "\r\n.dotted")
        );
        drop(seen);
        assert!(client.say("QUIT").starts_with("221 "));
    }

    #[test]
    fn the_submitters_verdict_becomes_the_reply() {
        for (verdict, code) in [
            (Verdict::Temporary(String::from("no connection\r\nnow")), "451 4.3.0 no connection now"),
            (Verdict::Refused(String::from("nobody took it")), "554 5.0.0 nobody took it"),
        ] {
            let (smtp, _) = server(verdict, Limits::default());
            let mut client = Client::signed_in(smtp);
            client.say("MAIL FROM:<ada@example.org>");
            client.say("RCPT TO:<ben@example.net>");
            client.say("DATA");
            assert_eq!(client.say("Subject: x\r\n\r\nbody\r\n."), code);
        }
    }

    #[test]
    fn nothing_is_taken_before_signing_in_and_wrong_passwords_end_the_connection() {
        let (smtp, fake) = server(Verdict::Accepted(String::new()), Limits::default());
        let mut client = Client::connect(smtp.clone());
        assert!(client.say("EHLO x").contains("250 AUTH PLAIN LOGIN"));
        assert!(client.say("MAIL FROM:<ada@example.org>").starts_with("530 "));
        assert!(client.say("STARTTLS").starts_with("502 "));
        let wrong = auth::encode_base64(format!("\0{USER}\0nope").as_bytes());
        assert!(client.say(&format!("AUTH PLAIN {wrong}")).starts_with("535 "));
        assert!(client.say(&format!("AUTH PLAIN {wrong}")).starts_with("535 "));
        let last = client.say(&format!("AUTH PLAIN {wrong}"));
        assert!(last.starts_with("535 "), "{last}");
        assert!(client.reply().starts_with("421 "));
        assert!(fake.seen.lock().unwrap().is_empty());

        // AUTH LOGIN, the challenges answered one by one.
        let mut client = Client::connect(smtp);
        assert_eq!(client.say("AUTH LOGIN"), "334 VXNlcm5hbWU6");
        assert_eq!(client.say(&auth::encode_base64(USER.as_bytes())), "334 UGFzc3dvcmQ6");
        assert!(client.say(&auth::encode_base64(PASSWORD.as_bytes())).starts_with("235 "));
    }

    #[test]
    fn a_foreign_sender_a_bad_recipient_and_commands_out_of_order_are_refused() {
        let limits = Limits {
            recipients: 1,
            ..Limits::default()
        };
        let (smtp, fake) = server(Verdict::Accepted(String::new()), limits);
        let mut client = Client::signed_in(smtp);
        assert!(client.say("RCPT TO:<ben@example.net>").starts_with("503 "));
        assert!(client.say("DATA").starts_with("503 "));
        assert!(client.say("MAIL FROM:<boss@example.com>").starts_with("550 5.7.1"));
        assert!(client.say("MAIL FROM:<>").starts_with("550 "));
        assert!(client.say("MAIL FROM:<ada@example.org>").starts_with("250 "));
        assert!(client.say("RCPT TO:<not an address>").starts_with("501 "));
        assert!(client.say("RCPT TO:<ben@example.net>").starts_with("250 "));
        assert!(client.say("RCPT TO:<cy@example.net>").starts_with("452 "));
        assert!(client.say("RSET").starts_with("250 "));
        assert!(client.say("DATA").starts_with("503 "));
        assert!(fake.seen.lock().unwrap().is_empty());
    }

    #[test]
    fn a_message_over_the_limit_is_read_to_its_end_and_refused() {
        let limits = Limits {
            message_bytes: 64,
            ..Limits::default()
        };
        let (smtp, fake) = server(Verdict::Accepted(String::new()), limits);
        let mut client = Client::signed_in(smtp);
        assert!(client.say("MAIL FROM:<ada@example.org> SIZE=1000").starts_with("552 "));
        client.say("MAIL FROM:<ada@example.org>");
        client.say("RCPT TO:<ben@example.net>");
        client.say("DATA");
        let body = "x".repeat(50);
        let reply = client.say(&format!("Subject: big\r\n\r\n{body}\r\n{body}\r\n."));
        assert!(reply.starts_with("552 "), "{reply}");
        assert!(client.say("NOOP").starts_with("250 "), "still in step");
        assert!(fake.seen.lock().unwrap().is_empty());
    }

    #[test]
    fn an_http_request_on_the_submission_port_is_hung_up_on() {
        let (smtp, _) = server(Verdict::Accepted(String::new()), Limits::default());
        let mut client = Client::connect(smtp);
        client
            .stream
            .write_all(b"POST / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\nMAIL FROM:<x@y>\r\n")
            .unwrap();
        let started = std::time::Instant::now();
        let mut rest = Vec::new();
        let _ = std::io::Read::read_to_end(&mut client.stream, &mut rest);
        assert!(rest.is_empty(), "{}", String::from_utf8_lossy(&rest));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn paths_are_read_with_or_without_brackets_and_with_parameters() {
        assert_eq!(
            path_and_params(" <ada@example.org> SIZE=10 body=8bitmime"),
            Some((
                String::from("ada@example.org"),
                vec![
                    (String::from("SIZE"), String::from("10")),
                    (String::from("BODY"), String::from("8bitmime"))
                ]
            ))
        );
        assert_eq!(
            path_and_params("ben@example.net"),
            Some((String::from("ben@example.net"), vec![]))
        );
        assert_eq!(
            path_and_params("<@relay.example:ben@example.net>").map(|p| p.0),
            Some(String::from("ben@example.net"))
        );
        assert_eq!(path_and_params("<unclosed"), None);
        assert!(plausible("ada@example.org"));
        assert!(!plausible(""));
        assert!(!plausible("no-at"));
        assert!(!plausible("a b@c"));
        assert_eq!(one_line("a\r\nb"), "a b");
        assert_eq!(one_line(&"é".repeat(300)).chars().count(), 203);
    }

    /// CHUNKING (RFC 3030): BDAT takes the message in chunks of exactly so many octets, as they
    /// are (no dot-stuffing); the last one sends it. A BDAT out of order is refused after its
    /// octets are read, so the next command comes in step; DATA does not mix with BDAT.
    #[test]
    fn bdat_takes_the_message_in_chunks_of_exact_octets() {
        let (smtp, fake) = server(Verdict::Accepted(String::from("sent")), Limits::default());
        let mut client = Client::signed_in(smtp);
        assert!(client.say("EHLO mail.local").contains("CHUNKING"));
        let head = "From: Ada <ada@example.org>\r\nTo: ben@example.net\r\nSubject: Hi\r\n\r\n";
        let body = ".a line that starts with a dot\r\n";

        client.stream.write_all(b"BDAT 5\r\nhello").unwrap();
        assert!(client.reply().starts_with("503 "), "no MAIL yet");
        assert!(client.say("NOOP").starts_with("250 "), "still in step");

        assert!(client.say("MAIL FROM:<ada@example.org>").starts_with("250 "));
        assert!(client.say("RCPT TO:<ben@example.net>").starts_with("250 "));
        client
            .stream
            .write_all(format!("BDAT {}\r\n{head}", head.len()).as_bytes())
            .unwrap();
        assert!(client.reply().starts_with("250 "));
        assert!(client.say("DATA").starts_with("503 "), "DATA does not follow BDAT");
        client
            .stream
            .write_all(format!("BDAT {} LAST\r\n{body}", body.len()).as_bytes())
            .unwrap();
        assert_eq!(client.reply(), "250 2.0.0 sent");
        let seen = fake.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(String::from_utf8_lossy(&seen[0].message), format!("{head}{body}"));
        drop(seen);
        assert!(client.say("QUIT").starts_with("221 "));
    }
}
