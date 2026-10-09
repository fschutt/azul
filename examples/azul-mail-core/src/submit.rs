//! Submission: a mail handed to the account's own outgoing server, signed in (RFC 6409, RFC
//! 8314) - the optional route `send::SendRoute::Submission`; direct delivery with client-side
//! DKIM stays the default.
//!
//! The SMTP client is lettre's blocking `SmtpConnection` (implicit TLS or STARTTLS through
//! rustls with the RustCrypto provider and the Mozilla roots, EHLO, AUTH PLAIN / LOGIN /
//! XOAUTH2); the message is AzMail's own (micromail's builder, DKIM-signed by `send` when the
//! account signs) and goes as it is.

use std::{path::PathBuf, str::FromStr, time::Duration};

use lettre::{
    transport::smtp::{
        authentication::{Credentials, Mechanism},
        client::{Certificate, SmtpConnection, TlsParameters},
        commands::{Data, Mail, Rcpt},
        extension::{ClientId, Extension, MailBodyParameter, MailParameter},
        Error as SmtpError,
    },
    Address,
};
use micromail::{RecipientOutcome, RecipientStatus, Reply};

use crate::{
    account::{self, AuthKind, Secret},
    auth::{self, AuthMethod, ServerCaps},
    send::TlsPolicy,
};

/// How the connection to the outgoing server is protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitSecurity {
    /// TLS from the first byte (port 465, RFC 8314).
    ImplicitTls,
    /// STARTTLS, required: nothing is signed in before the connection is encrypted.
    StartTls,
    /// No encryption: only to a test server on this computer.
    Plain,
}

/// Where a submission goes and how it signs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitTarget {
    pub host: String,
    pub port: u16,
    pub security: SubmitSecurity,
    /// The account's user name (most providers want the address).
    pub username: String,
    pub auth: AuthKind,
    /// The password, app password or OAuth access token.
    pub secret: Secret,
    /// The name this computer gives in EHLO.
    pub helo: String,
    /// A PEM certificate to trust besides the Mozilla roots (a test server's).
    pub extra_ca_file: Option<PathBuf>,
}

/// Why nothing was handed over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitFailure {
    /// The mail waits for the user: the sign-in was refused, the server offers no way to sign
    /// in with this kind of secret, or the connection cannot be encrypted as it must be. No
    /// attempt is counted; the mail is due again at once (the next Send / Receive, once the
    /// password or the settings are new).
    Waits(String),
    /// The server could not be reached, or the connection broke: tried again after the
    /// backoff.
    Unreachable(String),
}

/// How long one step of the conversation may take.
const TIMEOUT: Duration = Duration::from_secs(60);

/// The protection for the outgoing server `host:port` (RFC 8314): TLS from the first byte on
/// port 465 (or when the settings say `implicit`), STARTTLS - required - on any other port, no
/// encryption only when STARTTLS is off AND the server is this computer (a test server). A
/// password never goes unencrypted to another computer.
pub fn submission_security(
    host: &str,
    port: u16,
    tls: TlsPolicy,
) -> Result<SubmitSecurity, String> {
    match tls {
        TlsPolicy::Off if account::is_loopback_host(host) => Ok(SubmitSecurity::Plain),
        TlsPolicy::Off => Err(format!(
            "{host}: AzMail signs in only over an encrypted connection, and STARTTLS is off (that \
             is for a test server on this computer): turn it on under Account Settings, Sending"
        )),
        TlsPolicy::Implicit => Ok(SubmitSecurity::ImplicitTls),
        _ if port == account::SMTPS_PORT => Ok(SubmitSecurity::ImplicitTls),
        TlsPolicy::Opportunistic | TlsPolicy::Required => Ok(SubmitSecurity::StartTls),
    }
}

/// Connects to the target, encrypts, signs in and hands `message` over for `recipients`
/// (envelope sender `from`): each recipient's outcome from the server's answers, or why
/// nothing was handed over. Blocking: call it from an azul `Thread`. The secret never appears
/// in a result.
pub fn submit(
    target: &SubmitTarget,
    from: &str,
    recipients: &[String],
    message: &[u8],
) -> Result<Vec<RecipientOutcome>, SubmitFailure> {
    let server = format!("{} port {}", target.host, target.port);
    let hello = ClientId::Domain(target.helo.clone());
    let tls = match target.security {
        SubmitSecurity::Plain => None,
        SubmitSecurity::ImplicitTls | SubmitSecurity::StartTls => {
            Some(tls_parameters(target).map_err(SubmitFailure::Waits)?)
        }
    };
    let implicit = match target.security {
        SubmitSecurity::ImplicitTls => tls.as_ref(),
        _ => None,
    };
    let mut connection = SmtpConnection::connect(
        (target.host.as_str(), target.port),
        Some(TIMEOUT),
        &hello,
        implicit,
        None,
    )
    .map_err(|e| {
        if e.is_tls() {
            SubmitFailure::Waits(format!(
                "{server}: the encrypted connection could not be set up ({e})"
            ))
        } else {
            SubmitFailure::Unreachable(format!("could not connect to {server} ({e})"))
        }
    })?;
    if target.security == SubmitSecurity::StartTls {
        let Some(tls) = tls.as_ref().filter(|_| connection.can_starttls()) else {
            let _ = connection.quit();
            return Err(SubmitFailure::Waits(format!(
                "{server} does not offer STARTTLS: AzMail sends the password only over an \
                 encrypted connection (port 465 encrypts from the start)"
            )));
        };
        connection.starttls(tls, &hello).map_err(|e| {
            if e.is_tls() || e.is_permanent() {
                SubmitFailure::Waits(format!(
                    "{server}: the encrypted connection could not be set up ({e})"
                ))
            } else {
                SubmitFailure::Unreachable(format!("{server}: STARTTLS failed ({e})"))
            }
        })?;
    }
    sign_in(&mut connection, target, &server)?;
    let outcomes = hand_over(&mut connection, &target.host, from, recipients, message);
    let _ = connection.quit();
    outcomes
}

/// Installs the pure-Rust RustCrypto provider as rustls's process default once (lettre's
/// `rustls-no-provider` asks for one; AzMail's IMAP connection uses the same provider).
fn install_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Another part of the process may have installed one first: that one is used.
        let _ = rustls_rustcrypto::provider().install_default();
    });
}

/// TLS for the target: the Mozilla roots, plus the extra certificate the settings name.
fn tls_parameters(target: &SubmitTarget) -> Result<TlsParameters, String> {
    install_crypto_provider();
    let mut builder = TlsParameters::builder(target.host.clone());
    if let Some(path) = &target.extra_ca_file {
        let pem = std::fs::read(path)
            .map_err(|e| format!("the certificate file {}: {e}", path.display()))?;
        let certificate = Certificate::from_pem(&pem)
            .map_err(|e| format!("the certificate file {}: {e}", path.display()))?;
        builder = builder.add_root_certificate(certificate);
    }
    builder
        .build_rustls()
        .map_err(|e| format!("TLS for {}: {e}", target.host))
}

/// Signs in with the one mechanism `auth::choose_submission` picks from the EHLO's AUTH line.
fn sign_in(
    connection: &mut SmtpConnection,
    target: &SubmitTarget,
    server: &str,
) -> Result<(), SubmitFailure> {
    let offered: Vec<String> = [Mechanism::Plain, Mechanism::Login, Mechanism::Xoauth2]
        .into_iter()
        .filter(|m| connection.server_info().supports_auth_mechanism(*m))
        .map(|m| m.to_string())
        .collect();
    let offered: Vec<&str> = offered.iter().map(String::as_str).collect();
    let method = match auth::choose_submission(target.auth, ServerCaps::from_smtp_auth(&offered)) {
        Ok(method) => method,
        Err(why) => {
            let _ = connection.quit();
            return Err(SubmitFailure::Waits(format!("{server}: {why}")));
        }
    };
    let mechanism = match method {
        AuthMethod::Plain => Mechanism::Plain,
        AuthMethod::Login => Mechanism::Login,
        AuthMethod::Xoauth2 => Mechanism::Xoauth2,
    };
    let credentials = Credentials::new(target.username.clone(), target.secret.expose().to_string());
    let refusal = match connection.auth(&[mechanism], &credentials) {
        Ok(_) => return Ok(()),
        // XOAUTH2's refusal starts with an error report sent as a challenge (Google's way);
        // lettre stops there. The empty line RFC 7628 asks for brings the server's answer.
        Err(e) if mechanism == Mechanism::Xoauth2 && e.is_client() => {
            connection.command("\r\n").err().unwrap_or(e)
        }
        Err(e) => e,
    };
    let _ = connection.quit();
    Err(match reply_of(&refusal) {
        Some(reply) if !reply.is_permanent() => SubmitFailure::Unreachable(format!(
            "{server} could not check the sign-in now ({reply})"
        )),
        Some(reply) => SubmitFailure::Waits(format!(
            "{server} refused the sign-in ({reply}): enter the password (an app password for \
             Gmail and iCloud) or a new token again under Account Settings"
        )),
        None if is_connection(&refusal) => SubmitFailure::Unreachable(format!(
            "the connection to {server} broke during the sign-in ({refusal})"
        )),
        None => SubmitFailure::Waits(format!("{server}: the sign-in failed ({refusal})")),
    })
}

/// MAIL, one RCPT per recipient, DATA: each recipient's outcome, in the recipients' order.
fn hand_over(
    connection: &mut SmtpConnection,
    host: &str,
    from: &str,
    recipients: &[String],
    message: &[u8],
) -> Result<Vec<RecipientOutcome>, SubmitFailure> {
    let everyone = |status: RecipientStatus| {
        recipients
            .iter()
            .map(|address| RecipientOutcome {
                address: address.clone(),
                status: status.clone(),
            })
            .collect::<Vec<_>>()
    };
    let Ok(sender) = Address::from_str(from) else {
        return Ok(everyone(RecipientStatus::Rejected {
            reply: Reply::local(
                553,
                "5.1.7",
                &format!("the sender {from} is not an address"),
            ),
            server: String::new(),
        }));
    };
    let mut parameters = Vec::new();
    if !message.is_ascii()
        && connection
            .server_info()
            .supports_feature(Extension::EightBitMime)
    {
        parameters.push(MailParameter::Body(MailBodyParameter::EightBitMime));
    }
    if let Err(e) = connection.command(Mail::new(Some(sender), parameters)) {
        return Ok(everyone(answer(&e, host)?));
    }
    // `None`: taken at RCPT, its outcome is the data's.
    let mut statuses: Vec<Option<RecipientStatus>> = Vec::with_capacity(recipients.len());
    for recipient in recipients {
        statuses.push(match Address::from_str(recipient) {
            Err(_) => Some(RecipientStatus::Rejected {
                reply: Reply::local(553, "5.1.3", &format!("{recipient} is not an address")),
                server: String::new(),
            }),
            Ok(address) => match connection.command(Rcpt::new(address, Vec::new())) {
                Ok(_) => None,
                Err(e) => Some(answer(&e, host)?),
            },
        });
    }
    if statuses.iter().any(Option::is_none) {
        let data = match connection.command(Data) {
            Err(e) => answer(&e, host)?,
            Ok(_) => {
                // lettre ends the data with CRLF . CRLF itself: the message's own last line
                // end is not sent twice, so the server stores the bytes as they are.
                let body = message.strip_suffix(b"\r\n").unwrap_or(message);
                match connection.message(body) {
                    Ok(_) => RecipientStatus::Accepted {
                        server: host.to_string(),
                    },
                    Err(e) => answer(&e, host)?,
                }
            }
        };
        for status in statuses.iter_mut().filter(|s| s.is_none()) {
            *status = Some(data.clone());
        }
    }
    Ok(recipients
        .iter()
        .zip(statuses)
        .map(|(address, status)| RecipientOutcome {
            address: address.clone(),
            status: status.unwrap_or(RecipientStatus::Accepted {
                server: host.to_string(),
            }),
        })
        .collect())
}

/// A recipient's status from a negative answer: 5xx refused, 4xx later; a broken connection
/// stops the whole submission (tried again later).
fn answer(e: &SmtpError, host: &str) -> Result<RecipientStatus, SubmitFailure> {
    match reply_of(e) {
        Some(reply) if reply.is_permanent() => Ok(RecipientStatus::Rejected {
            reply,
            server: host.to_string(),
        }),
        Some(reply) => Ok(RecipientStatus::Deferred {
            reason: format!("{host}: {reply}"),
            reply: Some(reply),
        }),
        None => Err(SubmitFailure::Unreachable(format!(
            "the connection to {host} broke ({e})"
        ))),
    }
}

/// The server's answer inside a lettre error (4xx / 5xx), as micromail's `Reply` (the type
/// the outbox records for every route).
fn reply_of(e: &SmtpError) -> Option<Reply> {
    use std::error::Error as _;
    let code = e.status()?;
    let text = e.source().map(|s| s.to_string()).unwrap_or_default();
    let enhanced = text
        .split_whitespace()
        .next()
        .filter(|word| {
            let parts: Vec<&str> = word.split('.').collect();
            parts.len() == 3
                && matches!(parts[0], "2" | "4" | "5")
                && parts[1..]
                    .iter()
                    .all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(str::to_string);
    Some(Reply {
        code: u16::from(code),
        enhanced,
        text,
    })
}

/// Whether the connection itself failed (refused, timed out, broken), not an answer.
fn is_connection(e: &SmtpError) -> bool {
    !(e.is_response() || e.is_client() || e.is_transient() || e.is_permanent() || e.is_tls())
}

#[cfg(test)]
mod tests {
    use micromail::{RecipientOutcome, RecipientStatus, Reply};

    use super::*;
    use crate::{
        account::{AuthKind, Secret},
        send::TlsPolicy,
        testutil::{spawn_smtp_sink, SinkScript},
    };

    const MESSAGE: &[u8] =
        b"From: ada@example.org\r\nTo: ben@example.net\r\nSubject: Lunch\r\n\r\nnoon?\r\n.dotted\r\n";

    fn target(port: u16, security: SubmitSecurity, auth: AuthKind, secret: &str) -> SubmitTarget {
        SubmitTarget {
            host: String::from("127.0.0.1"),
            port,
            security,
            username: String::from("ada"),
            auth,
            secret: Secret::new(secret.to_string()),
            helo: String::from("example.org"),
            extra_ca_file: None,
        }
    }

    fn offers(ehlo: &[&str]) -> SinkScript {
        SinkScript {
            ehlo: ehlo.iter().map(|l| l.to_string()).collect(),
            accept: Some((String::from("ada"), String::from("app-password-1234"))),
            ..SinkScript::default()
        }
    }

    fn session(
        rx: std::sync::mpsc::Receiver<crate::testutil::SinkSession>,
    ) -> crate::testutil::SinkSession {
        rx.recv_timeout(std::time::Duration::from_secs(30))
            .expect("the sink saw a session")
    }

    #[test]
    fn the_connection_is_implicit_tls_on_465_starttls_elsewhere_and_plain_only_to_this_computer() {
        use SubmitSecurity::*;
        let cases = [
            (
                "smtp.gmail.com",
                465,
                TlsPolicy::Opportunistic,
                Ok(ImplicitTls),
            ),
            ("smtp.gmail.com", 465, TlsPolicy::Required, Ok(ImplicitTls)),
            (
                "smtp.office365.com",
                587,
                TlsPolicy::Opportunistic,
                Ok(StartTls),
            ),
            ("smtp.office365.com", 587, TlsPolicy::Required, Ok(StartTls)),
            (
                "smtp.example.org",
                2525,
                TlsPolicy::Implicit,
                Ok(ImplicitTls),
            ),
            ("127.0.0.1", 2525, TlsPolicy::Off, Ok(Plain)),
            ("localhost", 2525, TlsPolicy::Off, Ok(Plain)),
            ("[::1]", 2525, TlsPolicy::Off, Ok(Plain)),
            ("127.0.0.1", 2526, TlsPolicy::Implicit, Ok(ImplicitTls)),
        ];
        for (host, port, tls, want) in cases {
            assert_eq!(
                submission_security(host, port, tls),
                want,
                "{host}:{port} {tls:?}"
            );
        }
        for (host, port) in [
            ("smtp.example.org", 587),
            ("smtp.example.org", 465),
            ("192.0.2.7", 25),
        ] {
            let refused = submission_security(host, port, TlsPolicy::Off).unwrap_err();
            assert!(refused.contains("encrypt"), "{refused}");
        }
    }

    #[test]
    fn a_password_signs_in_with_plain_and_the_message_arrives_as_it_was() {
        let (port, rx) = spawn_smtp_sink(SinkScript {
            rcpt: vec![(
                String::from("cy@example.com"),
                String::from("550 5.1.1 no such user"),
            )],
            ..offers(&["8BITMIME", "AUTH LOGIN PLAIN"])
        });
        let outcomes = submit(
            &target(
                port,
                SubmitSecurity::Plain,
                AuthKind::Password,
                "app-password-1234",
            ),
            "ada@example.org",
            &[
                String::from("ben@example.net"),
                String::from("cy@example.com"),
            ],
            MESSAGE,
        )
        .expect("signed in and handed over");
        assert_eq!(
            outcomes,
            vec![
                RecipientOutcome {
                    address: String::from("ben@example.net"),
                    status: RecipientStatus::Accepted {
                        server: String::from("127.0.0.1"),
                    },
                },
                RecipientOutcome {
                    address: String::from("cy@example.com"),
                    status: RecipientStatus::Rejected {
                        reply: Reply {
                            code: 550,
                            enhanced: Some(String::from("5.1.1")),
                            text: String::from("5.1.1 no such user"),
                        },
                        server: String::from("127.0.0.1"),
                    },
                },
            ]
        );
        let seen = session(rx);
        assert_eq!(
            seen.sign_ins,
            vec![(
                String::from("PLAIN"),
                String::from("ada"),
                String::from("app-password-1234")
            )]
        );
        assert!(
            seen.commands.iter().any(|c| c == "EHLO example.org"),
            "{:?}",
            seen.commands
        );
        assert!(
            seen.commands
                .iter()
                .any(|c| c == "MAIL FROM:<ada@example.org>"),
            "{:?}",
            seen.commands
        );
        assert_eq!(
            seen.message.as_bytes(),
            MESSAGE,
            "byte for byte, dots and line ends kept"
        );
    }

    #[test]
    fn a_password_signs_in_with_login_when_plain_is_not_offered() {
        let (port, rx) = spawn_smtp_sink(offers(&["AUTH LOGIN"]));
        let outcomes = submit(
            &target(
                port,
                SubmitSecurity::Plain,
                AuthKind::Password,
                "app-password-1234",
            ),
            "ada@example.org",
            &[String::from("ben@example.net")],
            MESSAGE,
        )
        .expect("signed in");
        assert!(
            matches!(outcomes[0].status, RecipientStatus::Accepted { .. }),
            "{outcomes:?}"
        );
        assert_eq!(session(rx).sign_ins[0].0, "LOGIN");
    }

    #[test]
    fn a_token_goes_as_xoauth2_and_a_refused_one_waits_with_the_servers_words() {
        let (port, rx) = spawn_smtp_sink(SinkScript {
            accept: Some((String::from("ada"), String::from("ya29.token"))),
            ..offers(&["AUTH PLAIN LOGIN XOAUTH2"])
        });
        submit(
            &target(port, SubmitSecurity::Plain, AuthKind::Xoauth2, "ya29.token"),
            "ada@example.org",
            &[String::from("ben@example.net")],
            MESSAGE,
        )
        .expect("signed in with the token");
        assert_eq!(
            session(rx).sign_ins,
            vec![(
                String::from("XOAUTH2"),
                String::from("ada"),
                String::from("ya29.token")
            )]
        );
        // An expired token: the error report comes as a challenge, then the refusal.
        let (port, rx) = spawn_smtp_sink(offers(&["AUTH PLAIN LOGIN XOAUTH2"]));
        let failure = submit(
            &target(
                port,
                SubmitSecurity::Plain,
                AuthKind::Xoauth2,
                "ya29.expired",
            ),
            "ada@example.org",
            &[String::from("ben@example.net")],
            MESSAGE,
        )
        .unwrap_err();
        let SubmitFailure::Waits(why) = failure else {
            panic!("{failure:?}");
        };
        assert!(why.contains("535") && why.contains("5.7.8"), "{why}");
        assert!(!why.contains("ya29"), "the token never shows: {why}");
        let seen = session(rx);
        assert!(
            !seen.commands.iter().any(|c| c.starts_with("MAIL")),
            "{:?}",
            seen.commands
        );
    }

    #[test]
    fn a_refused_password_waits_for_the_user_and_nothing_is_sent() {
        let (port, rx) = spawn_smtp_sink(offers(&["AUTH PLAIN LOGIN"]));
        let failure = submit(
            &target(
                port,
                SubmitSecurity::Plain,
                AuthKind::Password,
                "wrong-password",
            ),
            "ada@example.org",
            &[String::from("ben@example.net")],
            MESSAGE,
        )
        .unwrap_err();
        let SubmitFailure::Waits(why) = failure else {
            panic!("{failure:?}");
        };
        assert!(
            why.contains("535 5.7.8 Username and Password not accepted"),
            "{why}"
        );
        assert!(why.contains("127.0.0.1"), "{why}");
        assert!(
            !why.contains("wrong-password"),
            "the password never shows: {why}"
        );
        let seen = session(rx);
        assert!(
            !seen.commands.iter().any(|c| c.starts_with("MAIL")),
            "{:?}",
            seen.commands
        );
    }

    #[test]
    fn no_secret_goes_to_a_server_that_offers_no_way_for_it() {
        // A token, but no XOAUTH2 on offer.
        let (port, rx) = spawn_smtp_sink(offers(&["AUTH PLAIN LOGIN"]));
        let failure = submit(
            &target(port, SubmitSecurity::Plain, AuthKind::Xoauth2, "ya29.token"),
            "ada@example.org",
            &[String::from("ben@example.net")],
            MESSAGE,
        )
        .unwrap_err();
        assert!(
            matches!(&failure, SubmitFailure::Waits(why) if why.contains("XOAUTH2")),
            "{failure:?}"
        );
        assert!(session(rx).sign_ins.is_empty());
        // STARTTLS required, and the server does not offer it: no password in the clear.
        let (port, rx) = spawn_smtp_sink(offers(&["AUTH PLAIN LOGIN"]));
        let failure = submit(
            &target(
                port,
                SubmitSecurity::StartTls,
                AuthKind::Password,
                "app-password-1234",
            ),
            "ada@example.org",
            &[String::from("ben@example.net")],
            MESSAGE,
        )
        .unwrap_err();
        assert!(
            matches!(&failure, SubmitFailure::Waits(why) if why.contains("STARTTLS")),
            "{failure:?}"
        );
        assert!(session(rx).sign_ins.is_empty());
    }

    #[test]
    fn a_server_nobody_answers_on_is_unreachable_and_tried_again_later() {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let failure = submit(
            &target(
                port,
                SubmitSecurity::Plain,
                AuthKind::Password,
                "app-password-1234",
            ),
            "ada@example.org",
            &[String::from("ben@example.net")],
            MESSAGE,
        )
        .unwrap_err();
        assert!(
            matches!(&failure, SubmitFailure::Unreachable(why) if why.contains("127.0.0.1") && why.contains(&port.to_string())),
            "{failure:?}"
        );
    }

    #[test]
    fn a_temporary_answer_to_the_data_defers_every_recipient() {
        let (port, _rx) = spawn_smtp_sink(SinkScript {
            data_reply: Some(String::from("451 4.3.0 try again later")),
            ..offers(&["AUTH PLAIN"])
        });
        let outcomes = submit(
            &target(
                port,
                SubmitSecurity::Plain,
                AuthKind::Password,
                "app-password-1234",
            ),
            "ada@example.org",
            &[
                String::from("ben@example.net"),
                String::from("cy@example.com"),
            ],
            MESSAGE,
        )
        .expect("signed in");
        assert_eq!(outcomes.len(), 2);
        for outcome in &outcomes {
            let RecipientStatus::Deferred {
                reply: Some(reply), ..
            } = &outcome.status
            else {
                panic!("{outcomes:?}");
            };
            assert_eq!(reply.code, 451);
            assert_eq!(reply.enhanced.as_deref(), Some("4.3.0"));
        }
    }
}
