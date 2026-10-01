//! The "Sending" section of the account settings: the form over SEND's `send::SendSettings`.
//!
//! The settings themselves are SEND's (`send.rs`): `SendSettings::load` / `save` keep them in
//! `<AzMail folder>/<account id>/sending.json` next to the account's `account.json`, with no
//! secret in it. This module is only what the settings page shows and edits: the route (direct
//! delivery to each receiver's mail server, or one SMTP server `host:port`) and STARTTLS
//! (`TlsPolicy`: on - opportunistic, or required when the file says so - or off, for a test
//! server on this computer without TLS). Everything else in the settings (DKIM, the EHLO name,
//! a test CA, the direct port, the policy override) is kept as it is when the form is applied.

use crate::send::{SendRoute, SendSettings, TlsPolicy};

/// The port the form proposes for an SMTP server (submission with STARTTLS).
pub const SUBMISSION_PORT: u16 = 587;

/// One line for the status bar and the settings page: "Direct delivery" or "SMTP
/// localhost:2525" (", STARTTLS" / ", STARTTLS required").
pub fn describe(settings: &SendSettings) -> String {
    todo!()
}

/// The "Sending" section's fields as typed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SendingForm {
    /// "Through an SMTP server" is chosen (else direct delivery).
    pub smtp: bool,
    pub host: String,
    pub port: String,
    pub starttls: bool,
}

impl SendingForm {
    /// The form showing `settings`.
    pub fn from_settings(settings: &SendSettings) -> SendingForm {
        todo!()
    }

    /// `settings` with the form's route and STARTTLS choice (every other setting kept), or
    /// what is wrong with the form.
    pub fn apply(&self, settings: &SendSettings) -> Result<SendSettings, String> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn smtp(host: &str, port: u16, tls: TlsPolicy) -> SendSettings {
        SendSettings {
            route: SendRoute::Smtp {
                host: host.to_string(),
                port,
            },
            tls,
            ..SendSettings::default()
        }
    }

    #[test]
    fn an_account_sends_directly_until_it_is_told_otherwise() {
        let form = SendingForm::from_settings(&SendSettings::default());
        assert!(!form.smtp);
        assert_eq!(form.port, SUBMISSION_PORT.to_string(), "the form proposes 587");
        assert!(form.starttls);
        assert_eq!(form.apply(&SendSettings::default()), Ok(SendSettings::default()));
        assert_eq!(describe(&SendSettings::default()), "Direct delivery");
    }

    #[test]
    fn the_form_shows_and_edits_the_route_and_starttls() {
        let local = smtp("localhost", 2525, TlsPolicy::Off);
        let form = SendingForm::from_settings(&local);
        assert_eq!(
            form,
            SendingForm {
                smtp: true,
                host: String::from("localhost"),
                port: String::from("2525"),
                starttls: false,
            }
        );
        assert_eq!(form.apply(&SendSettings::default()), Ok(local.clone()));
        assert_eq!(describe(&local), "SMTP localhost:2525");
        let ticked = SendingForm {
            starttls: true,
            ..form.clone()
        };
        assert_eq!(
            ticked.apply(&SendSettings::default()),
            Ok(smtp("localhost", 2525, TlsPolicy::Opportunistic))
        );
        let required = smtp("smtp.example.org", 587, TlsPolicy::Required);
        assert_eq!(describe(&required), "SMTP smtp.example.org:587, STARTTLS required");
        assert_eq!(
            SendingForm::from_settings(&required).apply(&required),
            Ok(required.clone()),
            "a ticked box keeps STARTTLS required"
        );
        assert_eq!(
            describe(&smtp("smtp.example.org", 587, TlsPolicy::Opportunistic)),
            "SMTP smtp.example.org:587, STARTTLS"
        );
    }

    #[test]
    fn applying_the_form_keeps_every_other_setting() {
        let mut settings = SendSettings::default();
        settings.helo_name = String::from("mail.example.org");
        settings.direct_port = 2526;
        let form = SendingForm {
            smtp: true,
            host: String::from("localhost"),
            port: String::from("2525"),
            starttls: false,
        };
        let applied = form.apply(&settings).unwrap();
        assert_eq!(applied.helo_name, "mail.example.org");
        assert_eq!(applied.direct_port, 2526);
    }

    #[test]
    fn the_form_checks_the_server_and_the_port() {
        let form = SendingForm::from_settings(&smtp("localhost", 2525, TlsPolicy::Off));
        let no_host = SendingForm {
            host: String::from("  "),
            ..form.clone()
        };
        assert!(no_host.apply(&SendSettings::default()).is_err());
        let bad_port = SendingForm {
            port: String::from("70000"),
            ..form.clone()
        };
        assert!(bad_port.apply(&SendSettings::default()).is_err());
        let empty_port = SendingForm {
            port: String::new(),
            ..form
        };
        assert_eq!(
            empty_port.apply(&SendSettings::default()),
            Ok(smtp("localhost", SUBMISSION_PORT, TlsPolicy::Off)),
            "an empty port is the default"
        );
    }

    #[test]
    fn the_settings_go_into_sends_file_next_to_the_account() {
        let dir = TempDir::new("sending");
        let local = smtp("localhost", 2525, TlsPolicy::Off);
        let path = local.save(&dir.0, "ada@example.org").unwrap();
        assert_eq!(path, dir.0.join("ada@example.org").join("sending.json"));
        assert_eq!(SendSettings::load(&dir.0, "ada@example.org"), local);
    }
}
