//! How an account sends: the "Sending" section of the account settings, one JSON file next to
//! the account's `account.json`, `<AzMail folder>/<account id>/sending.json`. No secret is in
//! it (an SMTP server that wants a password gets the account's, from the OS keyring).
//!
//! The format, version 1:
//!
//! ```json
//! { "format": "azmail.sending", "version": 1, "route": "direct" }
//! { "format": "azmail.sending", "version": 1, "route": "smtp",
//!   "host": "localhost", "port": 2525, "starttls": false }
//! ```
//!
//! `direct` delivers to each receiver's mail server (MX) itself; `smtp` hands the mail to one
//! server (a provider's submission server, or a test server on this computer). What the values
//! mean to the sending code is `send.rs`'s (the SEND task); [`Sending::send_settings`] is the one
//! place that turns them into its `SendSettings`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The file in the account's folder.
pub const SENDING_FILE: &str = "sending.json";
/// The `format` of the file.
pub const FORMAT: &str = "azmail.sending";
/// The version this AzMail writes, and the newest it reads.
pub const VERSION: u64 = 1;
/// The port the form proposes for an SMTP server (submission with STARTTLS).
pub const SUBMISSION_PORT: u16 = 587;

/// Where mail goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// To each receiver's mail server (MX), directly.
    Direct,
    /// To one SMTP server.
    Smtp {
        host: String,
        port: u16,
        /// Upgrade the connection with STARTTLS (a test server on this computer may not offer
        /// it).
        starttls: bool,
    },
}

/// An account's sending settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sending {
    pub route: Route,
}

impl Default for Sending {
    fn default() -> Sending {
        Sending {
            route: Route::Direct,
        }
    }
}

/// The file on disk.
#[derive(Debug, Serialize, Deserialize)]
struct SendingFile {
    format: String,
    version: u64,
    route: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    starttls: Option<bool>,
}

impl Sending {
    /// The file's contents (pretty JSON, ending in a newline).
    pub fn to_json(&self) -> String {
        todo!()
    }

    /// Reads a sending file; `None` for anything that is not one this AzMail reads.
    pub fn from_json(text: &str) -> Option<Sending> {
        todo!()
    }

    /// `<AzMail folder>/<account id>/sending.json`
    pub fn path(root: &Path, account_id: &str) -> PathBuf {
        todo!()
    }

    /// The account's settings; the default (direct) when it has no file or one this AzMail
    /// cannot read.
    pub fn load(root: &Path, account_id: &str) -> Sending {
        todo!()
    }

    /// Writes the account's file (atomically).
    pub fn save(&self, root: &Path, account_id: &str) -> std::io::Result<PathBuf> {
        todo!()
    }

    /// One line for the status bar and the settings page: "Direct delivery" or "SMTP
    /// localhost:2525" (", STARTTLS").
    pub fn describe(&self) -> String {
        todo!()
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
}

impl SendingForm {
    /// The form showing `sending`.
    pub fn from_sending(sending: &Sending) -> SendingForm {
        todo!()
    }

    /// The settings the form says, or what is wrong with it.
    pub fn to_sending(&self) -> Result<Sending, String> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn smtp(host: &str, port: u16, starttls: bool) -> Sending {
        Sending {
            route: Route::Smtp {
                host: host.to_string(),
                port,
                starttls,
            },
        }
    }

    #[test]
    fn an_account_sends_directly_until_it_is_told_otherwise() {
        let dir = TempDir::new("sending");
        assert_eq!(Sending::load(&dir.0, "ada@example.org"), Sending::default());
        assert_eq!(Sending::default().route, Route::Direct);
        assert_eq!(Sending::default().describe(), "Direct delivery");
    }

    #[test]
    fn the_file_round_trips_next_to_the_account_and_holds_no_secret() {
        let dir = TempDir::new("sending");
        let s = smtp("localhost", 2525, false);
        let path = s.save(&dir.0, "ada@example.org").unwrap();
        assert_eq!(path, dir.0.join("ada@example.org").join("sending.json"));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"azmail.sending\"") && text.contains("\"smtp\""), "{text}");
        for word in ["password", "secret", "token"] {
            assert!(!text.contains(word), "{word} in {text}");
        }
        assert_eq!(Sending::load(&dir.0, "ada@example.org"), s);
        assert_eq!(s.describe(), "SMTP localhost:2525");
        assert_eq!(smtp("smtp.example.org", 587, true).describe(), "SMTP smtp.example.org:587, STARTTLS");
        let direct = Sending::default().to_json();
        assert_eq!(Sending::from_json(&direct), Some(Sending::default()));
        assert!(!direct.contains("host"), "{direct}");
    }

    #[test]
    fn a_file_this_azmail_cannot_read_is_left_alone() {
        assert_eq!(Sending::from_json("{}"), None);
        assert_eq!(Sending::from_json("nope"), None);
        assert_eq!(
            Sending::from_json(r#"{"format":"azmail.sending","version":2,"route":"direct"}"#),
            None
        );
        assert_eq!(
            Sending::from_json(r#"{"format":"azmail.sending","version":1,"route":"pigeon"}"#),
            None
        );
        assert_eq!(
            Sending::from_json(r#"{"format":"azmail.sending","version":1,"route":"smtp"}"#),
            None,
            "smtp without a host"
        );
    }

    #[test]
    fn the_form_checks_the_server_and_the_port() {
        let form = SendingForm::from_sending(&smtp("localhost", 2525, false));
        assert_eq!(
            form,
            SendingForm {
                smtp: true,
                host: String::from("localhost"),
                port: String::from("2525"),
                starttls: false,
            }
        );
        assert_eq!(form.to_sending(), Ok(smtp("localhost", 2525, false)));
        let direct = SendingForm::from_sending(&Sending::default());
        assert!(!direct.smtp);
        assert_eq!(direct.port, SUBMISSION_PORT.to_string(), "the form proposes 587");
        assert!(direct.starttls);
        assert_eq!(direct.to_sending(), Ok(Sending::default()));
        let no_host = SendingForm {
            host: String::from("  "),
            ..form.clone()
        };
        assert!(no_host.to_sending().is_err());
        let bad_port = SendingForm {
            port: String::from("70000"),
            ..form.clone()
        };
        assert!(bad_port.to_sending().is_err());
        let empty_port = SendingForm {
            port: String::new(),
            ..form
        };
        assert_eq!(
            empty_port.to_sending(),
            Ok(smtp("localhost", SUBMISSION_PORT, false)),
            "an empty port is the default"
        );
    }
}
