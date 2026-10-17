//! The customer's OWN Cloudflare account, through the customer's own API token (feature
//! `encryption`): the one call the apps make there - setting a variable of the customer's mail
//! Worker (examples/azlin-mail-worker), the drive's drop public key. Azlin is never in between:
//! the token goes from this computer to Cloudflare only and is kept nowhere by the kit.
//!
//! The request carries the account id, the Worker's name, the variable and its value (a public
//! key) and the token - nothing of the user (no name, no address).

use std::fmt;

use azul_storage::{
    crypto::Zeroizing,
    transport::{HttpCall, Method},
    Transport,
};
use serde_json::{json, Value};

use crate::error::{fail, CloudResult};

/// Cloudflare's API.
pub const API: &str = "https://api.cloudflare.com/client/v4";
/// The mail Worker's name in its `wrangler.toml`.
pub const DEFAULT_WORKER: &str = "azlin-mail-worker";
/// The Worker's variable that holds an encrypted drive's drop public key.
pub const DROP_KEY_VARIABLE: &str = "AZLIN_DROP_PUBLIC_KEY";

/// A Cloudflare account reached with its owner's API token.
pub struct Cloudflare {
    transport: Box<dyn Transport>,
    account_id: String,
    token: Zeroizing<String>,
    base: String,
}

impl fmt::Debug for Cloudflare {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cloudflare")
            .field("account_id", &self.account_id)
            .field("token", &"***")
            .finish_non_exhaustive()
    }
}

/// A name fit for a URL path segment: 1 to 63 of `a-z`, `0-9`, `-` and `_`.
fn check_name(what: &str, name: &str) -> CloudResult<()> {
    let fits = (1..=63).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    if !fits {
        fail!("{what} \"{name}\" is not a Cloudflare name (a-z, 0-9, - and _)");
    }
    Ok(())
}

impl Cloudflare {
    /// The account `account_id` (32 hex digits, the dashboard's "Account ID") with `token`.
    ///
    /// # Errors
    ///
    /// An account id that is not one, an empty token.
    pub fn new(transport: Box<dyn Transport>, account_id: &str, token: &str) -> CloudResult<Cloudflare> {
        let account_id = account_id.trim().to_ascii_lowercase();
        if account_id.len() != 32 || !account_id.bytes().all(|b| b.is_ascii_hexdigit()) {
            fail!("the Cloudflare account id is 32 hex digits (the dashboard's \"Account ID\")");
        }
        let token = Zeroizing::new(token.trim().to_string());
        if token.is_empty() {
            fail!("the Cloudflare API token is empty");
        }
        Ok(Cloudflare {
            transport,
            account_id,
            token,
            base: API.to_string(),
        })
    }

    /// Another API base (the tests' fake).
    #[must_use]
    pub fn with_base(mut self, base: &str) -> Cloudflare {
        self.base = base.trim_end_matches('/').to_string();
        self
    }

    /// Sets the Worker `script`'s secret variable `name` to `value` (Workers' "secrets" API:
    /// the Worker sees it at once, without a new deployment).
    ///
    /// # Errors
    ///
    /// No answer, or Cloudflare's refusal with its first error message (a token without
    /// "Workers Scripts: Edit", a Worker of another name).
    pub fn set_worker_secret(&self, script: &str, name: &str, value: &str) -> CloudResult<()> {
        check_name("the Worker", script)?;
        let call = HttpCall {
            method: Method::Put,
            url: format!(
                "{}/accounts/{}/workers/scripts/{script}/secrets",
                self.base, self.account_id
            ),
            headers: vec![
                (String::from("accept"), String::from("application/json")),
                (
                    String::from("authorization"),
                    format!("Bearer {}", self.token.as_str()),
                ),
            ],
            body: json!({ "name": name, "text": value, "type": "secret_text" })
                .to_string()
                .into_bytes(),
            content_type: String::from("application/json"),
        };
        let reply = match self.transport.send(&call) {
            Ok(reply) => reply,
            Err(why) => fail!("Cloudflare did not answer: {why}"),
        };
        let answer: Value = serde_json::from_slice(&reply.body).unwrap_or(Value::Null);
        if reply.is_success() && answer["success"].as_bool() != Some(false) {
            return Ok(());
        }
        let message = answer["errors"][0]["message"]
            .as_str()
            .map_or_else(|| format!("HTTP {}", reply.status), str::to_string);
        fail!("Cloudflare refused to set {name} on the Worker {script}: {message}")
    }
}
