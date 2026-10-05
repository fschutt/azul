//! Submission: a mail handed to the account's own outgoing server, signed in (RFC 6409, RFC
//! 8314) - the optional route `send::SendRoute::Submission`; direct delivery with client-side
//! DKIM stays the default.
//!
//! The SMTP client is lettre's blocking `SmtpConnection` (implicit TLS or STARTTLS through
//! rustls with the RustCrypto provider and the Mozilla roots, EHLO, AUTH PLAIN / LOGIN /
//! XOAUTH2); the message is AzMail's own (micromail's builder, DKIM-signed by `send` when the
//! account signs) and goes as it is.

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
