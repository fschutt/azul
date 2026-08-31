//! Crash reports by EMAIL — the backup transport for deployments with no
//! collector.
//!
//! The intended RELEASE configuration is: telemetry tier `crashes` (metrics
//! off, crash capture on), no OTLP endpoint, and a support mailbox. The
//! panic hook persists a self-contained JSON crash dump per crash
//! ([`super::queue::PingKind::Crash`]); this module drains those dumps into
//! one email — the dump as a `.json` attachment, an optional USER MESSAGE as
//! the body — over plain SMTP via `micromail` (EHLO/MAIL/RCPT/DATA, nothing
//! more).
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

/// Registers the support mailbox crash reports go to — AND arms the
/// reinvoke-reporter flow: from now on a panic in a process with NO OTLP
/// endpoint writes its dump to a temp file and respawns this executable
/// with [`super::CRASH_DUMP_ENV`] pointing at it. The reinvoked process
/// (`AzApp::run` checks the env var first) shows the dump and offers to
/// mail it. With an endpoint configured nothing respawns — the automatic
/// pipeline already owns the crash.
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
            "--{MIME_BOUNDARY}\r\nContent-Type: application/json; name=\"{name}\"\r\n\
             Content-Disposition: attachment; filename=\"{name}\"\r\n\
             Content-Transfer-Encoding: base64\r\n\r\n"
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
