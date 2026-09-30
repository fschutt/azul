//! How AzMail signs in: the method a server's capabilities allow, and the SASL responses.
//!
//! A password goes as `AUTHENTICATE PLAIN` when the server offers it (the password is sent as
//! UTF-8, base64-encoded, so any character works) and as `LOGIN` otherwise; a server that says
//! `LOGINDISABLED` and offers no PLAIN is refused before anything is sent. An OAuth access token
//! goes as `AUTHENTICATE XOAUTH2`. Only ever over TLS, or to a test server on this computer.

use crate::account::AuthKind;

/// What the server's `CAPABILITY` answer says about signing in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerCaps {
    /// `AUTH=PLAIN`
    pub auth_plain: bool,
    /// `AUTH=XOAUTH2`
    pub auth_xoauth2: bool,
    /// `LOGINDISABLED`
    pub login_disabled: bool,
}

/// The sign-in command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    Plain,
    Login,
    Xoauth2,
}

/// The method for the account's kind of secret, or why there is none.
pub fn choose(kind: AuthKind, caps: ServerCaps) -> Result<AuthMethod, String> {
    match kind {
        AuthKind::Xoauth2 => Ok(AuthMethod::Xoauth2),
        AuthKind::Password if caps.auth_plain => Ok(AuthMethod::Plain),
        AuthKind::Password if !caps.login_disabled => Ok(AuthMethod::Login),
        AuthKind::Password => Err(String::from(
            "the server offers neither AUTHENTICATE PLAIN nor LOGIN (it says LOGINDISABLED): it \
             may only take OAuth, or want STARTTLS on another port",
        )),
    }
}

/// The `AUTHENTICATE PLAIN` response before base64: `\0<user>\0<password>`.
pub fn plain_response(user: &str, password: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(user.len() + password.len() + 2);
    out.push(0);
    out.extend_from_slice(user.as_bytes());
    out.push(0);
    out.extend_from_slice(password.as_bytes());
    out
}

/// The `AUTHENTICATE XOAUTH2` response before base64:
/// `user=<user>\x01auth=Bearer <token>\x01\x01`.
pub fn xoauth2_response(user: &str, token: &str) -> Vec<u8> {
    format!("user={user}\x01auth=Bearer {token}\x01\x01").into_bytes()
}

/// What to answer a SASL challenge: the response to the server's first (empty) challenge; a
/// non-empty challenge is the server's error report (XOAUTH2 sends one before it says NO), which
/// is answered with an empty line so the server ends the exchange.
pub fn answer(challenge: &[u8], response: &[u8]) -> Vec<u8> {
    if challenge.is_empty() {
        response.to_vec()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(auth_plain: bool, auth_xoauth2: bool, login_disabled: bool) -> ServerCaps {
        ServerCaps {
            auth_plain,
            auth_xoauth2,
            login_disabled,
        }
    }

    #[test]
    fn a_password_goes_as_plain_when_offered_else_as_login() {
        assert_eq!(
            choose(AuthKind::Password, caps(true, false, false)),
            Ok(AuthMethod::Plain)
        );
        assert_eq!(
            choose(AuthKind::Password, caps(true, false, true)),
            Ok(AuthMethod::Plain)
        );
        assert_eq!(
            choose(AuthKind::Password, caps(false, false, false)),
            Ok(AuthMethod::Login)
        );
        let refused = choose(AuthKind::Password, caps(false, true, true)).unwrap_err();
        assert!(refused.contains("LOGINDISABLED"), "{refused}");
    }

    #[test]
    fn a_token_goes_as_xoauth2() {
        assert_eq!(
            choose(AuthKind::Xoauth2, caps(true, true, false)),
            Ok(AuthMethod::Xoauth2)
        );
        // Some servers only list their SASL methods after STARTTLS or not at all: try anyway.
        assert_eq!(
            choose(AuthKind::Xoauth2, caps(false, false, true)),
            Ok(AuthMethod::Xoauth2)
        );
    }

    #[test]
    fn the_sasl_responses_have_their_documented_shape() {
        assert_eq!(
            plain_response("ada@example.org", "pässword"),
            "\0ada@example.org\0pässword".as_bytes()
        );
        assert_eq!(
            xoauth2_response("ada@example.org", "ya29.token"),
            b"user=ada@example.org\x01auth=Bearer ya29.token\x01\x01"
        );
    }

    #[test]
    fn an_error_challenge_is_answered_with_an_empty_line() {
        assert_eq!(answer(b"", b"response"), b"response");
        assert_eq!(answer(b"{\"status\":\"401\"}", b"response"), b"");
    }
}
