//! The "Sending" section of the account settings: the form over SEND's `send::SendSettings`.
//!
//! The settings themselves are SEND's (`send.rs`): `SendSettings::load` / `save` keep them in
//! `<AzMail folder>/<account id>/sending.json` next to the account's `account.json`, with no
//! secret in it. This module is only what the settings page shows and edits: the route (direct
//! delivery to each receiver's mail server, or one SMTP server `host:port`) and STARTTLS
//! (`TlsPolicy`: on - opportunistic, or required when the file says so - or off, for a test
//! server on this computer without TLS), and client-side DKIM ([`SendingForm::apply_dkim`]:
//! on or off, the signing domain, the selector, the public half of the key `crate::dkim`
//! made). Everything else in the settings (the EHLO name, a test CA, the direct port, the
//! policy override, a key file named by hand) is kept as it is when the form is applied.

use crate::{
    dkim,
    send::{DkimSettings, SendRoute, SendSettings, TlsPolicy},
};

/// The port the form proposes for an SMTP server (submission with STARTTLS).
pub const SUBMISSION_PORT: u16 = 587;

/// One line for the status bar and the settings page: "Direct delivery" or "SMTP
/// localhost:2525" (", STARTTLS" / ", STARTTLS required"), and ", DKIM-signed (<domain>)" for
/// an account that signs.
pub fn describe(settings: &SendSettings) -> String {
    let route = match &settings.route {
        SendRoute::Direct => String::from("Direct delivery"),
        SendRoute::Submission => String::from("Through my provider's server, signed in"),
        SendRoute::Smtp { host, port } => {
            let tls = match settings.tls {
                TlsPolicy::Opportunistic => ", STARTTLS",
                TlsPolicy::Required => ", STARTTLS required",
                TlsPolicy::Implicit => ", TLS",
                TlsPolicy::Off => "",
            };
            format!("SMTP {host}:{port}{tls}")
        }
    };
    match &settings.dkim {
        Some(dkim) => format!("{route}, DKIM-signed ({})", dkim.domain),
        None => route,
    }
}

/// The "Sending" section's fields as typed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SendingForm {
    /// "Through an SMTP server" is chosen (else direct delivery).
    pub smtp: bool,
    pub host: String,
    pub port: String,
    pub starttls: bool,
    /// "Sign my mail with DKIM" is ticked.
    pub dkim: bool,
    /// The signing domain as typed; empty: the address's domain.
    pub dkim_domain: String,
    /// The selector as typed; empty: the saved one, else `dkim::default_selector`.
    pub dkim_selector: String,
}

impl SendingForm {
    /// The form showing `settings`.
    pub fn from_settings(settings: &SendSettings) -> SendingForm {
        let starttls = settings.tls != TlsPolicy::Off;
        let mut form = match &settings.route {
            SendRoute::Direct | SendRoute::Submission => SendingForm {
                smtp: false,
                host: String::new(),
                port: SUBMISSION_PORT.to_string(),
                starttls,
                ..SendingForm::default()
            },
            SendRoute::Smtp { host, port } => SendingForm {
                smtp: true,
                host: host.clone(),
                port: port.to_string(),
                starttls,
                ..SendingForm::default()
            },
        };
        if let Some(dkim) = &settings.dkim {
            form.dkim = true;
            form.dkim_domain = dkim.domain.clone();
            form.dkim_selector = dkim.selector.clone();
        }
        form
    }

    /// `settings` with the form's DKIM choice: unticked, unsigned; ticked, signed as the typed
    /// domain (else `email`'s) with the typed selector (else the saved one, else the one for a
    /// key made at `now`) and `public_key` (a key just created; empty: the saved one), or what
    /// is wrong - a provider's domain, a selector that is no DNS label, no key at all.
    pub fn apply_dkim(
        &self,
        settings: SendSettings,
        email: &str,
        public_key: &str,
        now: i64,
    ) -> Result<SendSettings, String> {
        if !self.dkim {
            return Ok(SendSettings {
                dkim: None,
                ..settings
            });
        }
        let saved = settings.dkim.clone().unwrap_or_default();
        let domain = match self.dkim_domain.trim() {
            "" => crate::account::email_domain(email).unwrap_or_default(),
            typed => typed.trim_end_matches('.').to_ascii_lowercase(),
        };
        dkim::can_sign_for(&domain)?;
        let selector = match self.dkim_selector.trim() {
            "" if !saved.selector.is_empty() => saved.selector.clone(),
            "" => dkim::default_selector(now),
            typed => typed.to_string(),
        };
        if !dkim::is_selector(&selector) {
            return Err(format!(
                "The selector {selector:?} cannot be a DNS name: letters, digits and -, at most \
                 63."
            ));
        }
        let public_key = match public_key.trim() {
            "" => saved.public_key.clone(),
            new => new.to_string(),
        };
        if public_key.is_empty() && saved.key_file.is_none() {
            return Err(String::from(
                "Create a key first: AzMail signs with a key of its own, whose public half goes \
                 into your domain's DNS.",
            ));
        }
        Ok(SendSettings {
            dkim: Some(DkimSettings {
                domain,
                selector,
                key_file: saved.key_file,
                public_key,
            }),
            ..settings
        })
    }

    /// `settings` with the form's route and STARTTLS choice (every other setting kept), or
    /// what is wrong with the form.
    pub fn apply(&self, settings: &SendSettings) -> Result<SendSettings, String> {
        let mut applied = settings.clone();
        applied.tls = match (self.starttls, settings.tls) {
            (false, _) => TlsPolicy::Off,
            // Ticked: STARTTLS when offered, or still required when the file said so.
            (true, TlsPolicy::Off) => TlsPolicy::Opportunistic,
            (true, kept) => kept,
        };
        if !self.smtp {
            applied.route = SendRoute::Direct;
            return Ok(applied);
        }
        let host = self.host.trim();
        if host.is_empty() || host.contains(char::is_whitespace) {
            return Err(String::from("Enter the SMTP server's name, e.g. smtp.example.org."));
        }
        let port = match self.port.trim() {
            "" => SUBMISSION_PORT,
            text => match text.parse::<u16>() {
                Ok(port) if port > 0 => port,
                _ => return Err(String::from("The port is a number from 1 to 65535.")),
            },
        };
        applied.route = SendRoute::Smtp {
            host: host.to_string(),
            port,
        };
        Ok(applied)
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
                ..SendingForm::default()
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
            ..SendingForm::default()
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
        let path = local.save(&dir.folder(), "ada@example.org").unwrap();
        assert_eq!(path, dir.0.join("ada@example.org").join("sending.json"));
        assert_eq!(SendSettings::load(&dir.folder(), "ada@example.org"), local);
    }

    // ---- client-side DKIM ----

    /// 2026-10-01T08:30:00Z
    const NOW: i64 = 1_790_843_400;

    fn signing_settings() -> SendSettings {
        SendSettings {
            dkim: Some(DkimSettings {
                domain: String::from("example.org"),
                selector: String::from("azmail202609"),
                key_file: None,
                public_key: String::from("MIIBsaved"),
            }),
            ..SendSettings::default()
        }
    }

    #[test]
    fn dkim_is_off_until_ticked_and_then_signs_as_the_address_domain_with_the_created_key() {
        let form = SendingForm::from_settings(&SendSettings::default());
        assert!(!form.dkim);
        let off = form
            .apply_dkim(signing_settings(), "ada@example.org", "", NOW)
            .unwrap();
        assert_eq!(off.dkim, None, "unticked: unsigned");
        let ticked = SendingForm {
            dkim: true,
            ..form
        };
        let on = ticked
            .apply_dkim(SendSettings::default(), "Ada@Example.org", "MIIBnew", NOW)
            .unwrap();
        assert_eq!(
            on.dkim,
            Some(DkimSettings {
                domain: String::from("example.org"),
                selector: String::from("azmail202610"),
                key_file: None,
                public_key: String::from("MIIBnew"),
            })
        );
        assert_eq!(describe(&on), "Direct delivery, DKIM-signed (example.org)");
        // A typed domain and selector win.
        let typed = SendingForm {
            dkim_domain: String::from(" mail.example.org "),
            dkim_selector: String::from("s1"),
            ..ticked.clone()
        };
        let on = typed
            .apply_dkim(SendSettings::default(), "ada@example.org", "MIIBnew", NOW)
            .unwrap();
        assert_eq!(
            on.dkim
                .as_ref()
                .map(|d| (d.domain.as_str(), d.selector.as_str())),
            Some(("mail.example.org", "s1"))
        );
    }

    #[test]
    fn the_saved_dkim_settings_show_in_the_form_and_survive_a_save_without_a_new_key() {
        let saved = signing_settings();
        let form = SendingForm::from_settings(&saved);
        assert!(form.dkim);
        assert_eq!(form.dkim_domain, "example.org");
        assert_eq!(form.dkim_selector, "azmail202609");
        assert_eq!(
            form.apply_dkim(saved.clone(), "ada@example.org", "", NOW),
            Ok(saved)
        );
    }

    #[test]
    fn dkim_refuses_a_providers_domain_a_bad_selector_and_a_missing_key() {
        let ticked = SendingForm {
            dkim: true,
            ..SendingForm::default()
        };
        let gmail = ticked
            .apply_dkim(SendSettings::default(), "ada@gmail.com", "MIIBnew", NOW)
            .unwrap_err();
        assert!(gmail.contains("gmail.com"), "{gmail}");
        let bad = SendingForm {
            dkim_selector: String::from("a b"),
            ..ticked.clone()
        };
        assert!(bad
            .apply_dkim(SendSettings::default(), "ada@example.org", "MIIBnew", NOW)
            .is_err());
        let no_key = ticked
            .apply_dkim(SendSettings::default(), "ada@example.org", "", NOW)
            .unwrap_err();
        assert!(no_key.contains("Create a key"), "{no_key}");
        // A key file named by hand (azmail-send --dkim-key) is a key too.
        let with_file = SendSettings {
            dkim: Some(DkimSettings {
                domain: String::from("example.org"),
                selector: String::from("s1"),
                key_file: Some(std::path::PathBuf::from("/keys/dkim.pem")),
                public_key: String::new(),
            }),
            ..SendSettings::default()
        };
        assert_eq!(
            SendingForm::from_settings(&with_file).apply_dkim(
                with_file.clone(),
                "ada@example.org",
                "",
                NOW
            ),
            Ok(with_file)
        );
    }
}
