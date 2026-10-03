//! Crash reports by EMAIL — the backup transport for deployments with no
//! collector.
//!
//! The intended RELEASE configuration is: telemetry tier `crashes` (metrics
//! off, crash capture on), no OTLP endpoint, and a support mailbox. The
//! panic hook persists a self-contained JSON crash dump per crash
//! ([`super::queue::PingKind::Crash`]); this module drains those dumps into
//! one email — the dump as a `.json` attachment, an optional USER MESSAGE as
//! the body — over SMTP via `micromail` (EHLO, STARTTLS when the MX offers
//! it, MAIL/RCPT/DATA, nothing more).
//!
//! This is MANUAL by design: sending mail from a panic hook would block a
//! dying process on the network, and mailing without the user seeing the
//! moment happen is not the consent posture this crate keeps. The built-in
//! reporter dialog (`dialogs::crash_reporter`) is the sender: it opens on
//! the crash itself and again for a dump still queued on the next launch,
//! and its Send mails the dump with whatever message the user typed.
//! `AppConfig.report_problem` arms the contact at startup; an app with its
//! own dialog can still call [`send_crash_reports`] directly.

use std::path::PathBuf;

use super::queue::PingKind;

/// Where crash mails go, and as whom the client identifies.
#[derive(Debug, Clone)]
pub struct CrashMailConfig {
    /// Recipient, e.g. `crashes@myapp.example`. The MX of this address's
    /// domain is where the mail is delivered.
    pub to: String,
    /// Sender identity, e.g. `crash-reporter@myapp.example`.
    pub from: String,
    /// HELO/EHLO domain the client announces (typically the app's domain).
    pub helo_domain: String,
    /// SMTP ports to try, in order. Default `[25, 587, 2525]`.
    pub ports: Vec<u16>,
    /// Upgrade to TLS via STARTTLS when the server offers it.
    pub use_tls: bool,
    /// Subject prefix; the app name + version are appended per mail.
    pub subject_prefix: String,
}

impl CrashMailConfig {
    /// A config with conventional defaults; `to`/`from`/`helo_domain` are the
    /// three the app must decide.
    #[must_use]
    pub fn new(
        to: impl Into<String>,
        from: impl Into<String>,
        helo_domain: impl Into<String>,
    ) -> Self {
        Self {
            to: to.into(),
            from: from.into(),
            helo_domain: helo_domain.into(),
            ports: vec![25, 587, 2525],
            use_tls: true,
            subject_prefix: "[crash]".to_owned(),
        }
    }

    /// Overrides the SMTP port list (e.g. `vec![2525]` against a local sink).
    #[must_use]
    pub fn with_ports(mut self, ports: Vec<u16>) -> Self {
        self.ports = ports;
        self
    }

    /// Disables the STARTTLS upgrade (local sinks, test rigs).
    #[must_use]
    pub const fn with_tls(mut self, use_tls: bool) -> Self {
        self.use_tls = use_tls;
        self
    }
}

/// The registered crash contact, read by the reporter process.
static CRASH_CONTACT: std::sync::OnceLock<CrashMailConfig> = std::sync::OnceLock::new();

/// Derive the crash-mail contact from the app's `report_problem` mailbox
/// (`AppConfig.report_problem` — the same address `SysDialogType::ReportProblem`
/// mails to): reports go TO that address, FROM `crash-reporter@<its domain>`,
/// announcing `<its domain>`. `None` when the address has no domain part.
#[must_use]
pub fn config_from_report_address(address: &str) -> Option<CrashMailConfig> {
    let (local, domain) = address.trim().rsplit_once('@')?;
    if local.is_empty() || domain.is_empty() {
        return None;
    }
    Some(CrashMailConfig::new(
        address.trim().to_owned(),
        format!("crash-reporter@{domain}"),
        domain.to_owned(),
    ))
}

/// Registers the support mailbox crash reports go to — AND arms the
/// reinvoke-reporter flow: from now on a panic in a process with NO OTLP
/// endpoint writes its dump to a temp file and respawns this executable
/// with [`super::CRASH_DUMP_ENV`] pointing at it. The reinvoked process
/// (`AzApp::run` checks the env var first) shows the dump and offers to
/// mail it. With an endpoint configured nothing respawns — the automatic
/// pipeline already owns the crash.
pub fn set_crash_contact(config: CrashMailConfig) {
    super::mark_crash_contact(true);
    drop(CRASH_CONTACT.set(config));
}

/// The registered contact, if any.
#[must_use]
pub fn crash_contact() -> Option<&'static CrashMailConfig> {
    CRASH_CONTACT.get()
}

/// Mails ONE dump file (the reporter process's path: the dump came in via
/// [`super::CRASH_DUMP_ENV`]) with the user's message; deletes the file on
/// success.
///
/// # Errors
///
/// Returns the SMTP error as text; the file stays for a retry.
pub fn send_dump_file(
    config: &CrashMailConfig,
    path: &std::path::Path,
    user_message: &str,
) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let name = path.file_name().map_or_else(
        || "crash.json".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    send_attachments(config, user_message, &[(name, bytes)])?;
    drop(std::fs::remove_file(path));
    Ok(())
}

/// What one [`send_crash_reports`] call did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrashMailOutcome {
    /// Crash dumps attached and successfully mailed (files removed).
    pub mailed: usize,
    /// Dumps left on disk (no dumps pending is `mailed == 0` with
    /// `retained == 0`; a send failure retains everything).
    pub retained: usize,
    /// The transport error, if sending failed.
    pub last_error: Option<String>,
}

/// The pending crash-dump files, oldest first. Empty when the app never
/// crashed (or the queue was drained/GC'd).
#[must_use]
pub fn pending_crash_dumps() -> Vec<PathBuf> {
    let Some(queue) = super::ping_queue() else {
        return Vec::new();
    };
    queue
        .pending()
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .and_then(PingKind::from_file_name)
                == Some(PingKind::Crash)
        })
        .collect()
}

/// Mails every pending crash dump to the configured address as ONE message
/// (dumps as `.json` attachments), with `user_message` as the body. Sent
/// dumps are deleted; on failure everything stays for a retry.
///
/// Blocking (network). Call it from a background thread or an azul `Thread`,
/// never from a UI callback.
///
/// # Errors
///
/// Returns the SMTP error string when the transport fails; the outcome's
/// `retained` count then equals the number of dumps still on disk.
pub fn send_crash_reports(
    config: &CrashMailConfig,
    user_message: &str,
) -> Result<CrashMailOutcome, String> {
    let mut outcome = CrashMailOutcome::default();
    let dumps = pending_crash_dumps();
    if dumps.is_empty() {
        return Ok(outcome);
    }

    let mut attachments: Vec<(String, Vec<u8>)> = Vec::new();
    for path in &dumps {
        if let Ok(bytes) = std::fs::read(path) {
            let name = path.file_name().map_or_else(
                || "crash.json".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
            attachments.push((name, bytes));
        }
    }
    if attachments.is_empty() {
        outcome.retained = dumps.len();
        return Ok(outcome);
    }

    match send_attachments(config, user_message, &attachments) {
        Ok(()) => {
            for path in &dumps {
                drop(std::fs::remove_file(path));
            }
            outcome.mailed = dumps.len();
            Ok(outcome)
        }
        Err(e) => {
            outcome.retained = dumps.len();
            outcome.last_error = Some(e.clone());
            Err(e)
        }
    }
}

/// Shared transport: one mail, `user_message` body, attachments as base64
/// MIME parts. Also used by the `ReportProblem` dialog (`report.txt` +
/// screenshot.png ride the same pipe as crash dumps).
///
/// # Errors
///
/// Returns the SMTP error as text.
pub fn send_attachments(
    config: &CrashMailConfig,
    user_message: &str,
    attachments: &[(String, Vec<u8>)],
) -> Result<(), String> {
    let (app, version) = super::inner()
        .read()
        .ok()
        .and_then(|slot| {
            slot.as_ref().map(|state| {
                (
                    state.resource.service_name.clone(),
                    state.resource.service_version.clone(),
                )
            })
        })
        .unwrap_or_else(|| ("azul-app".to_owned(), "unknown".to_owned()));

    let subject = format!(
        "{} {app} {version}: {} crash report(s)",
        config.subject_prefix,
        attachments.len()
    );
    let body_text = if user_message.trim().is_empty() {
        "(no user message)".to_owned()
    } else {
        user_message.to_owned()
    };
    let mime = build_mime_body(&body_text, attachments);

    let mail_config = micromail::Config::new(config.helo_domain.clone())
        .ports(config.ports.clone())
        .use_tls(config.use_tls);
    let mut mailer = micromail::Mailer::new(mail_config);
    let mail = micromail::Mail::new()
        .from(config.from.clone())
        .to(config.to.clone())
        .subject(subject)
        .content_type(format!("multipart/mixed; boundary=\"{MIME_BOUNDARY}\""))
        .body(mime);

    mailer
        .send_sync(mail)
        .map_err(|e| format!("crash mail failed: {e}"))
}

/// Fixed multipart boundary — the payload is JSON we generate ourselves, so
/// collision with content is not a concern the way it is for arbitrary MIME.
const MIME_BOUNDARY: &str = "azul-crash-report-boundary";

/// `multipart/mixed` body: one `text/plain` part (the user's message), then
/// each dump as an `application/json` base64 attachment.
fn build_mime_body(text: &str, attachments: &[(String, Vec<u8>)]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = write!(
        out,
        "--{MIME_BOUNDARY}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{text}\r\n"
    );
    for (name, bytes) in attachments {
        let _ = write!(
            out,
            "--{MIME_BOUNDARY}\r\nContent-Type: application/json; \
             name=\"{name}\"\r\nContent-Disposition: attachment; \
             filename=\"{name}\"\r\nContent-Transfer-Encoding: base64\r\n\r\n"
        );
        // 76-char lines per RFC 2045.
        let encoded = base64_encode(bytes);
        for chunk in encoded.as_bytes().chunks(76) {
            out.push_str(std::str::from_utf8(chunk).unwrap_or_default());
            out.push_str("\r\n");
        }
    }
    let _ = write!(out, "--{MIME_BOUNDARY}--\r\n");
    out
}

/// Standard-alphabet base64 with `=` padding. ~20 lines beats a dependency
/// for the one place this crate needs an encoder.
fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        // RFC 4648 test vectors.
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn mime_body_carries_message_and_attachment() {
        let body = build_mime_body(
            "it crashed while I scrolled",
            &[(
                "0-1-crash.json".to_owned(),
                br#"{"kind":"azul-crash-dump"}"#.to_vec(),
            )],
        );
        assert!(body.contains("it crashed while I scrolled"));
        assert!(body.contains("filename=\"0-1-crash.json\""));
        assert!(body.contains("Content-Transfer-Encoding: base64"));
        // The attachment decodes back to the dump.
        assert!(body.contains(&base64_encode(br#"{"kind":"azul-crash-dump"}"#)));
        assert!(body.ends_with(&format!("--{MIME_BOUNDARY}--\r\n")));
    }
}

#[cfg(test)]
mod report_address_tests {
    use super::config_from_report_address;

    #[test]
    fn a_support_mailbox_becomes_a_full_crash_mail_contact() {
        let c = config_from_report_address(" crashes@myapp.example ").expect("valid address");
        assert_eq!(c.to, "crashes@myapp.example");
        assert_eq!(c.from, "crash-reporter@myapp.example");
        assert_eq!(c.helo_domain, "myapp.example");
        assert!(config_from_report_address("nodomain").is_none());
        assert!(config_from_report_address("@x").is_none());
        assert!(config_from_report_address("x@").is_none());
    }
}

/// The crash mail against a local SMTP sink: what actually goes over the wire.
///
/// `micromail` delivers a `...@localhost` recipient to 127.0.0.1 on the
/// configured ports (its `dns::get_mx_records`), so a sink on an ephemeral
/// port sees the exact session the reporter dialog would run against a real
/// MX. Found by the AzMail exploration
/// (`scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md`, section 4).
#[cfg(test)]
mod smtp_sink_tests {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::{TcpListener, TcpStream},
        sync::mpsc,
        time::Duration,
    };

    use super::{send_attachments, CrashMailConfig};

    /// What one SMTP session delivered to the sink.
    #[derive(Debug, Default)]
    struct Session {
        /// The bytes the client sent right after the sink answered `220` to
        /// `STARTTLS` (`None`: the client never asked for STARTTLS).
        after_starttls: Option<Vec<u8>>,
        /// The message as a server stores it: RFC 5321 4.5.2 transparency
        /// applied (one leading dot of a line removed), terminator line dropped.
        message: Option<String>,
        /// A DATA line ended in a bare LF instead of CRLF.
        bare_lf: bool,
    }

    /// A one-shot SMTP sink on 127.0.0.1: its port and the session it saw.
    fn spawn_sink(offer_starttls: bool) -> (u16, mpsc::Receiver<Session>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local port");
        let port = listener.local_addr().expect("the sink's address").port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            drop(stream.set_read_timeout(Some(Duration::from_secs(10))));
            drop(tx.send(serve(stream, offer_starttls)));
        });
        (port, rx)
    }

    fn say(out: &mut TcpStream, line: &str) {
        drop(out.write_all(line.as_bytes()));
    }

    fn serve(stream: TcpStream, offer_starttls: bool) -> Session {
        let mut session = Session::default();
        let mut out = stream.try_clone().expect("clone the sink socket");
        let mut reader = BufReader::new(stream);
        say(&mut out, "220 sink ESMTP\r\n");
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let upper = line.trim_end().to_ascii_uppercase();
            if upper.starts_with("EHLO") {
                if offer_starttls {
                    say(&mut out, "250-sink\r\n250-STARTTLS\r\n250 OK\r\n");
                } else {
                    say(&mut out, "250-sink\r\n250 OK\r\n");
                }
            } else if upper.starts_with("HELO") {
                say(&mut out, "250 sink\r\n");
            } else if upper == "STARTTLS" {
                say(&mut out, "220 ready to start TLS\r\n");
                // A TLS client now sends its ClientHello. This sink speaks no
                // TLS: what arrives first is the whole proof.
                let mut buf = [0_u8; 64];
                let n = reader.read(&mut buf).unwrap_or(0);
                session.after_starttls = Some(buf[..n].to_vec());
                break;
            } else if upper.starts_with("MAIL FROM") || upper.starts_with("RCPT TO") {
                say(&mut out, "250 OK\r\n");
            } else if upper == "DATA" {
                say(&mut out, "354 end with <CRLF>.<CRLF>\r\n");
                let mut message = String::new();
                loop {
                    let mut l = String::new();
                    match reader.read_line(&mut l) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    if l == ".\r\n" || l == ".\n" {
                        break;
                    }
                    if l.ends_with('\n') && !l.ends_with("\r\n") {
                        session.bare_lf = true;
                    }
                    let stored = if l.starts_with('.') {
                        l[1..].to_owned()
                    } else {
                        l
                    };
                    message.push_str(&stored);
                }
                session.message = Some(message);
                say(&mut out, "250 queued\r\n");
            } else if upper == "QUIT" {
                say(&mut out, "221 bye\r\n");
                break;
            } else {
                say(&mut out, "500 unrecognised\r\n");
            }
        }
        session
    }

    /// The contact `config_from_report_address` would build (STARTTLS on by
    /// default), pointed at the sink.
    fn contact(port: u16) -> CrashMailConfig {
        CrashMailConfig::new("crashes@localhost", "crash-reporter@localhost", "localhost")
            .with_ports(vec![port])
    }

    fn dump() -> Vec<(String, Vec<u8>)> {
        vec![(
            "0-1-crash.json".to_owned(),
            br#"{"kind":"azul-crash-dump"}"#.to_vec(),
        )]
    }

    fn session_of(rx: &mpsc::Receiver<Session>) -> Session {
        rx.recv_timeout(Duration::from_secs(30))
            .expect("the crash mail reached the sink")
    }

    /// Every real MX offers STARTTLS and the reporter asks for it by default.
    /// micromail 0.1, built without its `tls` feature, answered the server's
    /// `220` by carrying on in PLAINTEXT (`EHLO ...`), which a server that is
    /// waiting for a ClientHello drops: the report was never delivered. 0.2
    /// with `tls-rustcrypto` (layout/Cargo.toml) sends the handshake.
    #[test]
    fn a_crash_mail_never_speaks_plaintext_after_the_server_agreed_to_starttls() {
        let (port, rx) = spawn_sink(true);
        drop(send_attachments(&contact(port), "it crashed", &dump()));
        let session = session_of(&rx);
        if let Some(first) = session.after_starttls {
            assert!(
                first.is_empty() || first[0] == 0x16,
                "after `220 ready to start TLS` the client must send a TLS handshake record \
                 (0x16) or hang up, not plaintext SMTP: got {:?}",
                String::from_utf8_lossy(&first)
            );
        }
    }

    /// Without `MIME-Version: 1.0` (RFC 2045 section 4) a mail client may show the
    /// multipart body as raw text instead of a message with a `.json`
    /// attachment. micromail 0.1's `Mail::format` wrote From, To, Subject,
    /// Date, Message-ID and Content-Type only; 0.2 adds the version.
    #[test]
    fn a_crash_mail_declares_mime_version_1_0() {
        let (port, rx) = spawn_sink(false);
        send_attachments(&contact(port), "it crashed", &dump()).expect("the sink accepts the mail");
        let message = session_of(&rx)
            .message
            .expect("the sink received a message");
        let headers = message.split("\r\n\r\n").next().unwrap_or_default();
        assert!(
            headers
                .lines()
                .any(|h| h.to_ascii_lowercase().replace(' ', "") == "mime-version:1.0"),
            "the header block must carry `MIME-Version: 1.0`:\n{headers}"
        );
    }

    /// RFC 5321 section 4.5.2: a client doubles the dot of every line that
    /// starts with one, because the server removes it. micromail 0.1 sent the
    /// body as is, so a user message line `.config/azul was missing` arrived
    /// as `config/azul was missing` (and a line holding only `.` ended DATA
    /// early); 0.2 dot-stuffs.
    #[test]
    fn a_user_message_line_that_starts_with_a_dot_arrives_intact() {
        let (port, rx) = spawn_sink(false);
        let user_message = "steps:\r\n.config/azul was missing\r\nthen it crashed";
        send_attachments(&contact(port), user_message, &dump()).expect("the sink accepts the mail");
        let message = session_of(&rx)
            .message
            .expect("the sink received a message");
        assert!(
            message.contains("\r\n.config/azul was missing\r\n"),
            "the dot-led line must survive the SMTP transparency rule:\n{message}"
        );
    }

    /// The reporter dialog's text box yields `\n` line ends. micromail 0.1's
    /// `ensure_crlf` converted only a body that contains NO `\r\n` at all, and
    /// crash_mail's MIME framing always contains some, so the user's lines
    /// went out with bare LFs, which strict servers reject (bare-LF / SMTP
    /// smuggling defences); 0.2 normalizes every line end.
    #[test]
    fn a_multi_line_user_message_goes_out_with_crlf_line_ends() {
        let (port, rx) = spawn_sink(false);
        send_attachments(
            &contact(port),
            "first line\nsecond line\nthird line",
            &dump(),
        )
        .expect("the sink accepts the mail");
        let session = session_of(&rx);
        assert!(
            !session.bare_lf,
            "every DATA line must end in CRLF:\n{}",
            session.message.unwrap_or_default()
        );
    }
}
