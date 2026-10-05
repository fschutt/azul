//! Sending mail: the message, the outbox, the route, the per-domain policy and the Sent folder.
//!
//! [`send_mail`] (blocking: call it from an azul `Thread`, never from a callback):
//!
//! 1. builds the message with micromail's MIME builder (RFC 5322: Date, From, To, Cc,
//!    Subject, Message-ID, In-Reply-To, References; text, `multipart/alternative` with the HTML,
//!    `multipart/mixed` with the attachments; RFC 2047 headers; quoted-printable or base64;
//!    CRLF everywhere; Bcc only in the envelope);
//! 2. writes it, unsigned, to `<AzMail folder>/<account>/outbox/<id>.eml`, with its delivery
//!    state in `<id>.json` (every recipient: pending, sent or failed; attempts; the next
//!    attempt);
//! 3. signs it with DKIM for each attempt when the account signs (client-side DKIM, the key
//!    from the keyring - `crate::dkim` makes it and its DNS record); an account that signs but
//!    whose key is not in memory yet waits, due again at once, never unsigned;
//! 4. delivers it ([`SendRoute`]): `Direct` hands each recipient domain to its mail exchangers
//!    (MX lookup, by preference, port 25, STARTTLS when offered) - except domains the policy
//!    list ([`PolicyList`], `send_policy.json`) says take mail only from a trusted relay (a
//!    signed mail passes the shipped defaults, not a learned refusal), and except while this
//!    connection is known to block port 25 ([`Port25Check`]: when no exchanger of a domain can
//!    even be connected to, a probe of big providers' exchangers tells which); `Smtp` hands
//!    everything to one server (`host:port`, STARTTLS optional, no sign-in); `Submission`
//!    (optional, never the default) signs in to the account's own outgoing server from
//!    `account.json` with its user name and secret ([`SendSettings::sign_in`]) and hands
//!    everything over there (`crate::submit`, lettre's client: implicit TLS on 465, STARTTLS
//!    required elsewhere); a mail whose password is not in memory yet, or whose sign-in was
//!    refused, waits for the user, due again at once;
//! 5. files the result: when every recipient is done and at least one took it, the message as
//!    it went out (signed) moves into the account's Sent folder in MAIL1's layout
//!    (`mail/sent/<yyyy>/<mm>/<uid>.eml`, a line in `mail/sent/index.jsonl`, `state.json`), so
//!    it lists like a synced mail; a temporary failure (4xx, no connection) leaves it queued
//!    with its reason and a later attempt ([`retry_outbox`]) sends to the recipients still
//!    pending; a 5xx from a recipient's own server in direct mode puts that domain into the
//!    policy list as "needs a relay" with its cause ([`RefusalCause`]: a home address list such
//!    as Spamhaus PBL, no reverse DNS, SPF / DKIM / DMARC) - unless the 5xx names the address
//!    itself, 5.1.x / 5.2.x. The relay fallback later reads where it is needed from there.
//!
//! The MIME builder and DKIM are micromail's (the user's crate; the same client azul's crash
//! mail uses), and so is the SMTP client of direct delivery and the relay; submission's signed-in
//! client is lettre's (`crate::submit`). None of them is AzMail's own.
//!
//! Files next to `account.json` (`<AzMail folder>/<account>/`): `sending.json` (the
//! [`SendSettings`], never a secret), `send_policy.json`, `outbox/`. The DKIM private key is
//! either a file the settings name or the OS keyring entry [`dkim_keyring_key`], which the
//! caller reads in a callback and hands over in [`SendSettings::dkim_key`]; it is never
//! written, logged or printed by this module.

use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};

use micromail::{RecipientOutcome, RecipientStatus, Reply};
use serde::{Deserialize, Serialize};

use crate::{
    account::{self, Secret},
    folders::Role,
    message,
    store::{self, DriveFolder, FolderState, IndexEntry, MailStore},
    submit::{SubmitFailure, SubmitTarget},
};

// ==== the interface AzMail's windows code against ====

/// A file attached to an outgoing mail.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attachment {
    pub file_name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

/// A mail to send. Addresses are `Name <local@domain>` or `local@domain`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutgoingMail {
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    /// In the envelope only, never in the message.
    pub bcc: Vec<String>,
    pub subject: String,
    pub text_body: String,
    pub html_body: Option<String>,
    /// The Message-ID this mail answers.
    pub in_reply_to: Option<String>,
    /// The thread's Message-IDs, oldest first.
    pub references: Vec<String>,
    pub attachments: Vec<Attachment>,
}

/// How a mail leaves this computer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SendRoute {
    /// Straight to each recipient domain's mail exchangers (MX, port 25): no account and no
    /// server of our own. Domains in the policy list that need a trusted relay wait.
    #[default]
    Direct,
    /// Everything to one SMTP server (a relay; a test server on this computer).
    Smtp { host: String, port: u16 },
    /// Everything to the account's own outgoing server (`smtp` in `account.json`), signed in
    /// with the account's user name and secret ([`SendSettings::sign_in`]): an optional route
    /// for a connection that cannot deliver directly; never the default.
    Submission,
}

/// STARTTLS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TlsPolicy {
    /// Encrypt when the server offers it (certificate verified; a failed handshake is retried
    /// without TLS).
    #[default]
    Opportunistic,
    /// Never send over an unencrypted connection: a server without STARTTLS is skipped.
    Required,
    /// Never ask for STARTTLS (a test server without TLS).
    Off,
    /// TLS from the first byte on any port (submission; port 465 is always so). The routes
    /// through micromail, which speaks STARTTLS only, take it as `Required`.
    Implicit,
}

/// DKIM signing: the key's public half is published at `<selector>._domainkey.<domain>`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DkimSettings {
    /// The signing domain (`d=`), normally the From address's domain.
    pub domain: String,
    /// The selector (`s=`).
    pub selector: String,
    /// A PEM file with the RSA private key (PKCS#1 or PKCS#8). `None`: the key comes from the
    /// OS keyring ([`dkim_keyring_key`]) through [`SendSettings::dkim_key`].
    pub key_file: Option<PathBuf>,
    /// The public half (base64 SubjectPublicKeyInfo, `crate::dkim`): what the DNS record
    /// publishes, kept so the Sending page can show the record without the private key.
    /// Empty for a key AzMail did not make.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub public_key: String,
}

/// How AzMail sends for one account. `Default` is direct delivery with opportunistic TLS, no
/// DKIM. Stored as `sending.json` in the account's folder ([`SendSettings::load`] /
/// [`SendSettings::save`]); the runtime-only DKIM key is never stored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SendSettings {
    pub route: SendRoute,
    pub tls: TlsPolicy,
    /// The name this computer gives in `EHLO`; empty: the From address's domain.
    pub helo_name: String,
    /// A PEM certificate to trust besides the Mozilla roots (a local test server's).
    pub extra_ca_file: Option<PathBuf>,
    /// The port direct delivery connects to; 0 is 25. Only a test MX on this computer needs
    /// another one.
    pub direct_port: u16,
    /// Deliver directly even to domains the policy list marks as needing a relay.
    pub ignore_policy: bool,
    /// Sign with DKIM; `None`: unsigned.
    pub dkim: Option<DkimSettings>,
    /// The DKIM private key (PEM) read from the OS keyring by the caller. Never saved.
    #[serde(skip)]
    pub dkim_key: Option<Secret>,
    /// The account's password, app password or OAuth token, for [`SendRoute::Submission`]:
    /// read from the OS keyring by the caller (the secret the IMAP sync signs in with). Never
    /// saved.
    #[serde(skip)]
    pub sign_in: Option<Secret>,
    /// Where the port-25 probe knocks (`host:port`); empty: [`PORT25_PROBE_HOSTS`]. Only a
    /// test points it somewhere else.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub port25_probe: Vec<String>,
}

/// What became of a mail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendStatus {
    /// Every recipient's server took it; it is in the Sent folder. The Message-ID is without
    /// angle brackets, as `index.jsonl` has it.
    Sent { message_id: String },
    /// It waits in the outbox for another attempt (`reason`: why, for people).
    Queued { reason: String },
    /// At least one recipient will never get it (or nothing could be sent at all).
    Failed { reason: String },
}

/// Builds, stores, delivers and files `mail` for the account `account_id` under `root` (the
/// AzMail folder). Blocking (DNS and SMTP): call it from an azul `Thread`.
pub fn send_mail(
    root: &DriveFolder,
    account_id: &str,
    settings: &SendSettings,
    mail: &OutgoingMail,
) -> SendStatus {
    let mut transport = Network { settings };
    send_mail_with(
        root,
        account_id,
        settings,
        mail,
        now_secs(),
        &mut transport,
    )
}

/// Tries every queued outbox entry again whose time has come (all of them with `force`), and
/// returns each entry's id with its new status. Blocking, like [`send_mail`].
pub fn retry_outbox(
    root: &DriveFolder,
    account_id: &str,
    settings: &SendSettings,
    force: bool,
) -> Vec<(String, SendStatus)> {
    let mut transport = Network { settings };
    retry_outbox_with(
        root,
        account_id,
        settings,
        force,
        now_secs(),
        &mut transport,
    )
}

// ==== files ====

/// The settings file in the account's folder.
pub const SETTINGS_FILE: &str = "sending.json";
/// The policy list in the account's folder.
pub const POLICY_FILE: &str = "send_policy.json";
/// The outbox folder in the account's folder.
pub const OUTBOX_DIR: &str = "outbox";
/// The `format` of an outbox entry's state file.
pub const OUTBOX_FORMAT: &str = "azmail.outbox";
/// The `format` of the policy file.
pub const POLICY_FORMAT: &str = "azmail.send-policy";
/// Messages filed locally (sent from this computer) get UIDs from here up, far above any
/// server's, so a synced folder and local messages never share a UID.
pub const LOCAL_UID_FLOOR: u32 = 0xF000_0000;
/// A queued mail is given up after this long (RFC 5321 4.5.4.1 suggests 4-5 days).
pub const GIVE_UP_AFTER_SECS: i64 = 5 * 24 * 3600;

/// The OS keyring entry of the account's DKIM private key.
pub fn dkim_keyring_key(account_id: &str) -> String {
    format!("{}/{account_id}/dkim", account::APP_DIR)
}

impl SendSettings {
    /// The account's settings (`<AzMail folder>/<account>/sending.json`); the defaults when
    /// there is no such file or it cannot be read.
    pub fn load(root: &DriveFolder, account_id: &str) -> SendSettings {
        MailStore::new(account::account_dir(root, account_id))
            .get(SETTINGS_FILE)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Writes the settings file through the AzMail folder's drive and returns where it is. The
    /// DKIM key in memory is not part of it.
    pub fn save(&self, root: &DriveFolder, account_id: &str) -> std::io::Result<PathBuf> {
        let folder = account::account_dir(root, account_id);
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
        text.push('\n');
        MailStore::new(folder.clone()).put(SETTINGS_FILE, text.as_bytes())?;
        Ok(folder.path().join(SETTINGS_FILE))
    }

    /// The port direct delivery uses.
    pub fn direct_port(&self) -> u16 {
        if self.direct_port == 0 {
            25
        } else {
            self.direct_port
        }
    }
}

// ==== the policy list ====

/// How mail to a domain should leave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DomainRoute {
    /// Direct delivery to the domain's MX works.
    Direct,
    /// The domain takes mail only from a trusted relay (SPF / DKIM / reverse DNS checks a home
    /// connection does not pass).
    Relay,
}

/// Where a policy entry came from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicySource {
    /// Shipped with AzMail.
    #[default]
    Default,
    /// Learned from a 5xx answer.
    Learned,
    /// Set by the user: never overwritten by a learned answer.
    User,
}

/// One domain's entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainPolicy {
    pub route: DomainRoute,
    #[serde(default)]
    pub source: PolicySource,
    /// Why (for a learned entry: the server's answer).
    #[serde(default)]
    pub reason: String,
    /// When it was learned or set; RFC 3339, UTC.
    #[serde(default)]
    pub updated: String,
    /// What the refusal a learned entry comes from was about.
    #[serde(default)]
    pub cause: RefusalCause,
}

/// What a receiver's refusal of direct delivery was about: what the later relay fallback (or
/// the user) has to fix.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalCause {
    /// The answer does not say.
    #[default]
    Unknown,
    /// The sending address is on a list of home / dynamic addresses (Spamhaus PBL, a
    /// provider's own list): only a relay helps.
    HomeAddress,
    /// The sending address has no (matching) reverse DNS name, PTR: a relay, or a PTR from the
    /// Internet provider.
    NoReverseDns,
    /// SPF / DKIM / DMARC did not pass: the DNS records (`crate::dkim`) need fixing.
    NotAuthenticated,
}

/// Whether this connection reaches mail exchangers on port 25: the last probe's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Port25Check {
    pub open: bool,
    /// When it was checked, seconds since 1970.
    pub checked: i64,
    /// What the probe saw, for people.
    #[serde(default)]
    pub detail: String,
}

/// What a port-25 probe found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Port25Probe {
    /// A mail exchanger answered on port 25.
    Open,
    /// Every probed exchanger was found in DNS and none could be connected to: the connection
    /// blocks outgoing port 25.
    Blocked(String),
    /// Nothing can be said (DNS did not answer: offline).
    Unknown(String),
}

/// The per-receiver-domain policy list (`send_policy.json` in the account's folder).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyList {
    pub format: String,
    pub version: u64,
    /// By domain, lower case.
    pub domains: BTreeMap<String, DomainPolicy>,
    /// Whether this connection reaches port 25 at all (the last probe), for every domain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port25: Option<Port25Check>,
}

impl Default for PolicyList {
    fn default() -> PolicyList {
        let relay = "rejects direct mail from home connections (SPF, DKIM and reverse DNS \
                     checks): send through a trusted relay";
        let mut domains = BTreeMap::new();
        for domain in ["gmail.com", "googlemail.com", "yahoo.com"] {
            domains.insert(
                domain.to_string(),
                DomainPolicy {
                    route: DomainRoute::Relay,
                    source: PolicySource::Default,
                    reason: relay.to_string(),
                    updated: String::new(),
                    cause: RefusalCause::Unknown,
                },
            );
        }
        for domain in ["outlook.com", "hotmail.com"] {
            domains.insert(
                domain.to_string(),
                DomainPolicy {
                    route: DomainRoute::Direct,
                    source: PolicySource::Default,
                    reason: String::new(),
                    updated: String::new(),
                    cause: RefusalCause::Unknown,
                },
            );
        }
        PolicyList {
            format: POLICY_FORMAT.to_string(),
            version: 1,
            domains,
            port25: None,
        }
    }
}

impl PolicyList {
    /// The account's list; written with the defaults when there is none yet (or it cannot be
    /// read).
    pub fn load(account_dir: &DriveFolder) -> PolicyList {
        let read = MailStore::new(account_dir.clone())
            .get(POLICY_FILE)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PolicyList>(&bytes).ok())
            .filter(|list| list.format == POLICY_FORMAT);
        match read {
            Some(list) => list,
            None => {
                let list = PolicyList::default();
                let _ = list.save(account_dir);
                list
            }
        }
    }

    pub fn save(&self, account_dir: &DriveFolder) -> std::io::Result<()> {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
        text.push('\n');
        MailStore::new(account_dir.clone()).put(POLICY_FILE, text.as_bytes())
    }

    /// The entry for `domain` or the nearest parent domain that has one.
    pub fn entry_for(&self, domain: &str) -> Option<&DomainPolicy> {
        let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
        let mut rest = domain.as_str();
        loop {
            if let Some(entry) = self.domains.get(rest) {
                return Some(entry);
            }
            match rest.split_once('.') {
                Some((_, parent)) if parent.contains('.') => rest = parent,
                _ => return None,
            }
        }
    }

    /// How mail to `domain` should leave; `Direct` for a domain the list does not know.
    pub fn route_for(&self, domain: &str) -> DomainRoute {
        self.entry_for(domain)
            .map_or(DomainRoute::Direct, |entry| entry.route)
    }

    /// Records that `domain`'s server answered `reply` (5xx) to direct delivery. Returns
    /// whether the list changed: an entry the user set stays, and an answer about the address
    /// itself (5.1.x, 5.2.x, "no such user") says nothing about the route.
    pub fn learn(&mut self, domain: &str, reply: &Reply, now: i64) -> bool {
        if !says_relay_needed(reply) {
            return false;
        }
        let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
        if domain.is_empty() {
            return false;
        }
        if let Some(existing) = self.domains.get(&domain) {
            if existing.source == PolicySource::User {
                return false;
            }
        }
        let cause = refusal_cause(reply);
        let reason = format!("{} ({reply})", cause.explain(&domain));
        self.domains.insert(
            domain,
            DomainPolicy {
                route: DomainRoute::Relay,
                source: PolicySource::Learned,
                reason,
                updated: message::rfc3339_utc(now),
                cause,
            },
        );
        true
    }

    /// The entry that holds mail to `domain` back from direct delivery, if one does. A
    /// DKIM-`signed` mail is held back only by what was learned or set, not by a shipped
    /// default (those were about unsigned mail from home).
    pub fn holds_back(&self, domain: &str, signed: bool) -> Option<&DomainPolicy> {
        self.entry_for(domain).filter(|entry| {
            entry.route == DomainRoute::Relay && !(signed && entry.source == PolicySource::Default)
        })
    }

    /// Whether the last probe found port 25 blocked less than [`PORT25_RECHECK_SECS`] ago.
    pub fn port25_blocked(&self, now: i64) -> bool {
        self.port25
            .as_ref()
            .is_some_and(|check| !check.open && now - check.checked < PORT25_RECHECK_SECS)
    }

    /// Records that port 25 was found open at `now`; whether the list changed (an open finding
    /// is kept with its first date).
    fn note_port25_open(&mut self, now: i64) -> bool {
        if self.port25.as_ref().is_some_and(|check| check.open) {
            return false;
        }
        self.port25 = Some(Port25Check {
            open: true,
            checked: now,
            detail: String::new(),
        });
        true
    }
}

/// How long a port-25 probe's "blocked" stands before direct delivery is tried again (a laptop
/// moves to another network).
pub const PORT25_RECHECK_SECS: i64 = 3600;
/// Where the port-25 probe knocks: big providers' exchangers, which always listen.
pub const PORT25_PROBE_HOSTS: &[&str] = &[
    "gmail-smtp-in.l.google.com:25",
    "outlook-com.olc.protection.outlook.com:25",
];
/// What a recipient waits for while the connection blocks port 25.
pub const PORT25_BLOCKED: &str = "this Internet connection blocks outgoing mail on port 25 \
     (many home providers do): the mail waits in the Outbox until it can go through a relay";

/// How long the port-25 probe waits for each host.
const PORT25_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

/// What a refusal of direct delivery was about, from the receiver's answer (its enhanced code
/// and words: Gmail's 5.7.25 / 5.7.26, Spamhaus PBL in Microsoft's, Yahoo's and many smaller
/// receivers' 5.7.1, Postfix's "cannot find your reverse hostname").
pub fn refusal_cause(reply: &Reply) -> RefusalCause {
    let text = format!(
        "{} {}",
        reply.enhanced.as_deref().unwrap_or_default(),
        reply.text
    )
    .to_ascii_lowercase();
    let any = |needles: &[&str]| needles.iter().any(|needle| text.contains(needle));
    if any(&[
        "5.7.25",
        "ptr",
        "reverse dns",
        "reverse hostname",
        "reverse lookup",
        "reverse-dns",
        "rdns",
        "no reverse",
    ]) {
        return RefusalCause::NoReverseDns;
    }
    if any(&[
        "5.7.20",
        "5.7.21",
        "5.7.22",
        "5.7.23",
        "5.7.26",
        "5.7.27",
        "spf",
        "dkim",
        "dmarc",
        "unauthenticated",
        "not authenticated",
    ]) {
        return RefusalCause::NotAuthenticated;
    }
    if any(&[
        "spamhaus",
        "pbl",
        "dynamic",
        "residential",
        "dial-up",
        "dialup",
        "dsl",
        "home",
        "consumer",
        "blocked using",
        "blacklist",
        "blocklist",
        "block list",
        "dnsbl",
        "rbl",
        "[bl",
        "s3150",
        "5.7.606",
    ]) {
        return RefusalCause::HomeAddress;
    }
    RefusalCause::Unknown
}

impl RefusalCause {
    /// The cause in words, for `domain`'s refusal.
    pub fn explain(self, domain: &str) -> String {
        match self {
            RefusalCause::HomeAddress => format!(
                "{domain} refuses mail sent straight from a home Internet connection (its answer \
                 names a list of home addresses such as Spamhaus PBL): it needs a relay"
            ),
            RefusalCause::NoReverseDns => format!(
                "{domain} refuses mail from an address without a matching reverse DNS name \
                 (PTR): it needs a relay, or a PTR record for this connection from the Internet \
                 provider"
            ),
            RefusalCause::NotAuthenticated => format!(
                "{domain} did not accept the mail's SPF / DKIM / DMARC: check the DKIM record \
                 under Account Settings, Sending, and the domain's SPF record"
            ),
            RefusalCause::Unknown => format!(
                "{domain} refuses mail delivered directly from this computer: it needs a relay"
            ),
        }
    }
}

/// Knocks on `hosts` (`host:port`, port 25 for the real probe): `Open` at the first that takes
/// a TCP connection, `Blocked` when DNS knew some of them and none could be connected to,
/// `Unknown` when DNS knew none (offline). Blocking, at most `timeout` per address tried (the
/// first IPv4 and the first IPv6 address of each host).
pub fn probe_port_25(hosts: &[String], timeout: std::time::Duration) -> Port25Probe {
    use std::net::{TcpStream, ToSocketAddrs};
    let mut found = false;
    let mut seen = Vec::new();
    for host in hosts {
        let addresses: Vec<std::net::SocketAddr> = match host.as_str().to_socket_addrs() {
            Ok(addresses) => addresses.collect(),
            Err(e) => {
                seen.push(format!("{host}: {e}"));
                continue;
            }
        };
        let tried = addresses
            .iter()
            .find(|a| a.is_ipv4())
            .into_iter()
            .chain(addresses.iter().find(|a| a.is_ipv6()));
        for address in tried {
            found = true;
            match TcpStream::connect_timeout(address, timeout) {
                Ok(_) => return Port25Probe::Open,
                Err(e) => seen.push(format!("{host} ({address}): {e}")),
            }
        }
    }
    if found {
        Port25Probe::Blocked(seen.join("; "))
    } else {
        Port25Probe::Unknown(seen.join("; "))
    }
}

/// Whether a 5xx answer is about the sender (its IP, its authentication, its reputation)
/// rather than about the recipient's address.
pub fn says_relay_needed(reply: &Reply) -> bool {
    if !reply.is_permanent() {
        return false;
    }
    if let Some(enhanced) = &reply.enhanced {
        return !(enhanced.starts_with("5.1.") || enhanced.starts_with("5.2."));
    }
    let text = reply.text.to_ascii_lowercase();
    let about_the_address = [
        "no such user",
        "user unknown",
        "unknown user",
        "does not exist",
        "mailbox unavailable",
        "mailbox full",
        "over quota",
        "recipient address rejected",
    ];
    !about_the_address.iter().any(|needle| text.contains(needle))
}

// ==== the message ====

/// The message for `mail`, dated `now` (seconds since 1970): what is stored and sent, before
/// DKIM. Drafts can use it too.
pub fn build_message(mail: &OutgoingMail, now: i64) -> micromail::BuiltMessage {
    builder(mail, now).build()
}

fn builder(mail: &OutgoingMail, now: i64) -> micromail::MessageBuilder {
    micromail::MessageBuilder {
        from: mail.from.clone(),
        to: mail.to.clone(),
        cc: mail.cc.clone(),
        subject: mail.subject.clone(),
        text: mail.text_body.clone(),
        html: mail
            .html_body
            .clone()
            .filter(|html| !html.trim().is_empty()),
        in_reply_to: mail.in_reply_to.clone().filter(|id| !id.trim().is_empty()),
        references: mail.references.clone(),
        attachments: mail
            .attachments
            .iter()
            .map(|a| micromail::Attachment {
                file_name: a.file_name.clone(),
                mime_type: a.mime_type.clone(),
                bytes: a.bytes.clone(),
            })
            .collect(),
        headers: vec![("User-Agent".to_string(), "AzMail".to_string())],
        date: Some(now),
        ..micromail::MessageBuilder::default()
    }
}

/// The signed message, or why it cannot be signed. The key never appears in the error.
fn sign(bytes: Vec<u8>, settings: &SendSettings, account_id: &str) -> Result<Vec<u8>, String> {
    let Some(dkim) = &settings.dkim else {
        return Ok(bytes);
    };
    // Held as a Secret: overwritten when dropped.
    let pem = match (&settings.dkim_key, &dkim.key_file) {
        (Some(secret), _) if !secret.is_empty() => secret.clone(),
        (_, Some(path)) => Secret::new(
            std::fs::read_to_string(path)
                .map_err(|e| format!("DKIM: cannot read the key file {}: {e}", path.display()))?,
        ),
        _ => {
            return Err(format!(
                "DKIM: waiting for the signing key (the system keyring's entry {} is read at \
                 the next Send / Receive; without it, create a key under Account Settings, \
                 Sending)",
                dkim_keyring_key(account_id)
            ))
        }
    };
    let config = micromail::DkimConfig::from_pem(pem.expose(), &dkim.selector, &dkim.domain)
        .map_err(|e| format!("DKIM: {e}"))?;
    micromail::sign_message(&bytes, &config).map_err(|e| format!("DKIM: {e}"))
}

// ==== the outbox ====

/// Where one recipient stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecipientProgress {
    Pending,
    Sent,
    Failed,
}

/// One recipient of an outbox entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipientState {
    /// `local@domain`.
    pub address: String,
    pub state: RecipientProgress,
    /// The last answer or problem, for people.
    #[serde(default)]
    pub reason: String,
    /// The server's reply code, when there was one.
    #[serde(default)]
    pub code: Option<u16>,
    /// The server that took it or refused it.
    #[serde(default)]
    pub server: String,
}

/// Whether an outbox entry is still being tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutboxState {
    /// Some recipients are pending: tried again at `next_attempt`.
    Queued,
    /// Nobody got it and nothing is pending: it stays until the user deletes or edits it.
    Failed,
}

/// An outbox entry's state file, `outbox/<id>.json` next to `outbox/<id>.eml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub format: String,
    pub version: u64,
    pub id: String,
    /// Without angle brackets.
    pub message_id: String,
    /// The envelope sender (`local@domain`).
    pub from: String,
    pub subject: String,
    /// Seconds since 1970.
    pub created: i64,
    pub attempts: u32,
    /// Seconds since 1970; 0: never.
    pub last_attempt: i64,
    /// Seconds since 1970.
    pub next_attempt: i64,
    pub state: OutboxState,
    pub recipients: Vec<RecipientState>,
}

impl OutboxEntry {
    fn count(&self, progress: RecipientProgress) -> usize {
        self.recipients
            .iter()
            .filter(|r| r.state == progress)
            .count()
    }

    fn pending(&self) -> Vec<String> {
        self.recipients
            .iter()
            .filter(|r| r.state == RecipientProgress::Pending)
            .map(|r| r.address.clone())
            .collect()
    }
}

/// The outbox folder of an account.
pub fn outbox_dir(root: &DriveFolder, account_id: &str) -> DriveFolder {
    account::account_dir(root, account_id).child(OUTBOX_DIR)
}

/// The account's outbox entries, oldest first.
pub fn outbox_entries(root: &DriveFolder, account_id: &str) -> Vec<OutboxEntry> {
    let account = MailStore::new(account::account_dir(root, account_id));
    let mut entries: Vec<OutboxEntry> = account
        .keys(OUTBOX_DIR)
        .into_iter()
        .filter(|key| key.ends_with(".json"))
        .filter_map(|key| account.get(&key).ok())
        .filter_map(|bytes| serde_json::from_slice::<OutboxEntry>(&bytes).ok())
        .filter(|entry| entry.format == OUTBOX_FORMAT)
        .collect();
    entries.sort_by(|a, b| (a.created, &a.id).cmp(&(b.created, &b.id)));
    entries
}

/// One send or retry at a time per process: an entry, the policy file and the Sent index are
/// never written by two threads at once, and an entry is never delivered twice in parallel.
static OUTBOX_LOCK: Mutex<()> = Mutex::new(());

/// One delivery attempt. `relay`: `None` is the recipients' MX hosts.
pub(crate) trait Transport {
    fn deliver(
        &mut self,
        relay: Option<(&str, u16)>,
        from: &str,
        recipients: &[String],
        message: &[u8],
    ) -> Vec<RecipientOutcome>;

    /// Whether this connection reaches mail exchangers on port 25 at all.
    fn probe_port_25(&mut self) -> Port25Probe;

    /// Signs in to the submission server and hands the message over: each recipient's
    /// outcome, or why nothing was handed over.
    fn submit(
        &mut self,
        target: &SubmitTarget,
        from: &str,
        recipients: &[String],
        message: &[u8],
    ) -> Result<Vec<RecipientOutcome>, SubmitFailure>;
}

/// The real transport: micromail for direct delivery and the relay, lettre
/// (`crate::submit`) for submission.
struct Network<'a> {
    settings: &'a SendSettings,
}

impl Transport for Network<'_> {
    fn deliver(
        &mut self,
        relay: Option<(&str, u16)>,
        from: &str,
        recipients: &[String],
        message: &[u8],
    ) -> Vec<RecipientOutcome> {
        match micromail_config(self.settings, from, relay) {
            Ok(config) => micromail::Mailer::new(config).send_raw(from, recipients, message),
            Err(why) => {
                // A setting that cannot work: retrying will not help until it is changed.
                let reply = Reply::local(554, "5.3.5", &why);
                recipients
                    .iter()
                    .map(|address| RecipientOutcome {
                        address: micromail::message::address_spec(address),
                        status: RecipientStatus::Rejected {
                            reply: reply.clone(),
                            server: String::new(),
                        },
                    })
                    .collect()
            }
        }
    }

    fn probe_port_25(&mut self) -> Port25Probe {
        let hosts: Vec<String> = if self.settings.port25_probe.is_empty() {
            PORT25_PROBE_HOSTS.iter().map(|h| h.to_string()).collect()
        } else {
            self.settings.port25_probe.clone()
        };
        probe_port_25(&hosts, PORT25_PROBE_TIMEOUT)
    }

    fn submit(
        &mut self,
        target: &SubmitTarget,
        from: &str,
        recipients: &[String],
        message: &[u8],
    ) -> Result<Vec<RecipientOutcome>, SubmitFailure> {
        crate::submit::submit(target, from, recipients, message)
    }
}

/// The name this computer gives in `EHLO`: the settings' name, else the From address's domain.
fn helo_name(settings: &SendSettings, from: &str) -> String {
    if settings.helo_name.trim().is_empty() {
        micromail::message::address_domain(from).unwrap_or_else(|| "localhost".to_string())
    } else {
        settings.helo_name.trim().to_string()
    }
}

/// Where submission goes for the account: its outgoing server from `account.json`, its user
/// name and kind of secret, the secret in memory, the protection by port
/// ([`crate::submit::submission_security`]); or why the mail waits for the user.
fn submit_target(
    root: &DriveFolder,
    account_id: &str,
    settings: &SendSettings,
    from: &str,
) -> Result<SubmitTarget, String> {
    let account = match account::load(root, account_id) {
        Some(Ok(account)) => account,
        Some(Err(why)) => {
            return Err(format!(
                "the account's settings ({}) cannot be read: {why}",
                account::ACCOUNT_FILE
            ))
        }
        None => {
            return Err(format!(
                "there are no account settings ({}) to sign in with",
                account::ACCOUNT_FILE
            ))
        }
    };
    let host = account.smtp.host.trim().to_string();
    let port = account.smtp.port;
    if host.is_empty() || port == 0 {
        return Err(String::from(
            "the account has no outgoing (SMTP) server: enter it under Account Settings",
        ));
    }
    let security = crate::submit::submission_security(&host, port, settings.tls)?;
    let Some(secret) = settings.sign_in.clone().filter(|s| !s.is_empty()) else {
        return Err(format!(
            "waiting for the password to sign in to {host} (the system keyring's entry {} is \
             read at the next Send / Receive)",
            account::keyring_key(account_id)
        ));
    };
    let username = if account.username.trim().is_empty() {
        account.email.clone()
    } else {
        account.username.clone()
    };
    Ok(SubmitTarget {
        host,
        port,
        security,
        username,
        auth: account.auth,
        secret,
        helo: helo_name(settings, from),
        extra_ca_file: settings.extra_ca_file.clone(),
    })
}

/// micromail's configuration for these settings.
fn micromail_config(
    settings: &SendSettings,
    from: &str,
    relay: Option<(&str, u16)>,
) -> Result<micromail::Config, String> {
    let mut config = micromail::Config::new(helo_name(settings, from));
    config = match settings.tls {
        TlsPolicy::Opportunistic => config.use_tls(true),
        // micromail speaks STARTTLS only: implicit TLS (submission's) asks for encryption all
        // the same.
        TlsPolicy::Required | TlsPolicy::Implicit => config.use_tls(true).require_tls(true),
        TlsPolicy::Off => config.use_tls(false),
    };
    config = match relay {
        Some((host, port)) => config.relay(host, port),
        None => config.ports(vec![settings.direct_port()]),
    };
    let tls = match &settings.extra_ca_file {
        Some(path) => {
            let pem = std::fs::read(path)
                .map_err(|e| format!("the certificate file {}: {e}", path.display()))?;
            Some(
                micromail::tls::client_config_with_roots(&pem)
                    .map_err(|e| format!("the certificate file {}: {e}", path.display()))?,
            )
        }
        None => micromail::tls::default_client_config(),
    };
    if let Some(tls) = tls {
        config = config.tls_config(tls);
    }
    Ok(config)
}

pub(crate) fn send_mail_with(
    root: &DriveFolder,
    account_id: &str,
    settings: &SendSettings,
    mail: &OutgoingMail,
    now: i64,
    transport: &mut dyn Transport,
) -> SendStatus {
    let _lock = OUTBOX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let from = micromail::message::address_spec(&mail.from);
    if !from.contains('@') {
        return SendStatus::Failed {
            reason: "There is no sender address.".to_string(),
        };
    }
    let mut recipients: Vec<RecipientState> = Vec::new();
    for address in mail.to.iter().chain(&mail.cc).chain(&mail.bcc) {
        let address = micromail::message::address_spec(address);
        if address.is_empty()
            || recipients
                .iter()
                .any(|r| r.address.eq_ignore_ascii_case(&address))
        {
            continue;
        }
        recipients.push(RecipientState {
            address,
            state: RecipientProgress::Pending,
            reason: String::new(),
            code: None,
            server: String::new(),
        });
    }
    if recipients.is_empty() {
        return SendStatus::Failed {
            reason: "There is no recipient.".to_string(),
        };
    }
    // The outbox keeps the message unsigned: each attempt signs it with the key it has then
    // (`attempt`), so a mail written before the key came from the keyring waits rather than
    // going out unsigned.
    let built = build_message(mail, now);
    let bytes = built.bytes;
    let message_id = built
        .message_id
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_string();
    let mut entry = OutboxEntry {
        format: OUTBOX_FORMAT.to_string(),
        version: 1,
        id: outbox_id(now, &message_id),
        message_id,
        from,
        subject: mail.subject.clone(),
        created: now,
        attempts: 0,
        last_attempt: 0,
        next_attempt: now,
        state: OutboxState::Queued,
        recipients,
    };
    let outbox = MailStore::new(outbox_dir(root, account_id));
    if let Err(e) = outbox
        .put(&format!("{}.eml", entry.id), &bytes)
        .and_then(|()| write_entry(&outbox, &entry))
    {
        return SendStatus::Failed {
            reason: format!("The outbox cannot be written: {e}"),
        };
    }
    let attempted = attempt(
        root, account_id, settings, &mut entry, &bytes, now, transport,
    );
    let sent = attempted.sent.as_deref().unwrap_or(&bytes);
    finish(root, account_id, &mut entry, sent, now, attempted.waiting)
}

pub(crate) fn retry_outbox_with(
    root: &DriveFolder,
    account_id: &str,
    settings: &SendSettings,
    force: bool,
    now: i64,
    transport: &mut dyn Transport,
) -> Vec<(String, SendStatus)> {
    let _lock = OUTBOX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let outbox = MailStore::new(outbox_dir(root, account_id));
    let mut results = Vec::new();
    for mut entry in outbox_entries(root, account_id) {
        if entry.state != OutboxState::Queued || (!force && entry.next_attempt > now) {
            continue;
        }
        let Ok(bytes) = outbox.get(&format!("{}.eml", entry.id)) else {
            // The message is gone (deleted by hand): so is the entry.
            let _ = outbox.delete(&format!("{}.json", entry.id));
            continue;
        };
        let attempted = attempt(
            root, account_id, settings, &mut entry, &bytes, now, transport,
        );
        let sent = attempted.sent.as_deref().unwrap_or(&bytes);
        let status = finish(root, account_id, &mut entry, sent, now, attempted.waiting);
        results.push((entry.id.clone(), status));
    }
    results
}

/// `<yyyymmddThhmmssZ>-<8 hex digits of the Message-ID>`: sorts by time, unique enough.
fn outbox_id(now: i64, message_id: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    message_id.hash(&mut hasher);
    now.hash(&mut hasher);
    let stamp = message::rfc3339_utc(now).replace(|c: char| c == '-' || c == ':', "");
    format!("{stamp}-{:08x}", hasher.finish() as u32)
}

fn write_entry(outbox: &MailStore, entry: &OutboxEntry) -> std::io::Result<()> {
    let mut text = serde_json::to_string_pretty(entry).unwrap_or_default();
    text.push('\n');
    outbox.put(&format!("{}.json", entry.id), text.as_bytes())
}

/// What one attempt did.
#[derive(Debug, Default)]
struct Attempted {
    /// The message as it went out (signed when the account signs): what Sent keeps.
    sent: Option<Vec<u8>>,
    /// Nothing was tried because the mail waits for the user (the DKIM key is not in memory
    /// yet): no attempt is counted, and the mail is due again at once.
    waiting: bool,
}

/// Signs the message for this attempt, sends to the entry's pending recipients and records
/// each outcome.
#[allow(clippy::too_many_arguments)]
fn attempt(
    root: &DriveFolder,
    account_id: &str,
    settings: &SendSettings,
    entry: &mut OutboxEntry,
    bytes: &[u8],
    now: i64,
    transport: &mut dyn Transport,
) -> Attempted {
    let pending = entry.pending();
    if pending.is_empty() {
        return Attempted::default();
    }
    let wire = match sign(bytes.to_vec(), settings, account_id) {
        Ok(wire) => wire,
        Err(why) => {
            // Never unsigned when the account signs: the mail waits for the key.
            hold_back(entry, &pending, &why);
            return Attempted {
                sent: None,
                waiting: true,
            };
        }
    };
    let before = (entry.attempts, entry.last_attempt);
    entry.attempts += 1;
    entry.last_attempt = now;
    match &settings.route {
        SendRoute::Submission => {
            let failure = match submit_target(root, account_id, settings, &entry.from) {
                Err(why) => Some(SubmitFailure::Waits(why)),
                Ok(target) => match transport.submit(&target, &entry.from, &pending, &wire) {
                    Ok(outcomes) => {
                        record(entry, &outcomes);
                        None
                    }
                    Err(failure) => Some(failure),
                },
            };
            match failure {
                None => {}
                Some(SubmitFailure::Unreachable(why)) => hold_back(entry, &pending, &why),
                Some(SubmitFailure::Waits(why)) => {
                    // Waits for the user (the password, a refused sign-in, a setting): no
                    // attempt, due again at once.
                    (entry.attempts, entry.last_attempt) = before;
                    hold_back(entry, &pending, &why);
                    return Attempted {
                        sent: None,
                        waiting: true,
                    };
                }
            }
        }
        SendRoute::Smtp { host, port } => {
            let outcomes =
                transport.deliver(Some((host.as_str(), *port)), &entry.from, &pending, &wire);
            record(entry, &outcomes);
        }
        SendRoute::Direct => {
            let signed = settings.dkim.is_some();
            deliver_direct(
                root, account_id, settings, entry, pending, &wire, signed, now, transport,
            );
        }
    }
    Attempted {
        sent: Some(wire),
        waiting: false,
    }
}

/// Puts `why` on the pending recipients among `group` (they stay pending).
fn hold_back(entry: &mut OutboxEntry, group: &[String], why: &str) {
    for recipient in entry.recipients.iter_mut().filter(|r| {
        r.state == RecipientProgress::Pending && group.iter().any(|g| g.eq_ignore_ascii_case(&r.address))
    }) {
        recipient.reason = why.to_string();
    }
}

/// Direct delivery of `wire` to `pending`, each domain to its own mail exchangers, with the
/// policy list: a connection whose port 25 was found blocked waits (and is probed again after
/// [`PORT25_RECHECK_SECS`]); a domain that needs a relay waits; a refusal that says something
/// about this sender is learned with its cause; when no exchanger of a domain could even be
/// connected to, the port-25 probe tells a dead domain from a blocked connection.
#[allow(clippy::too_many_arguments)]
fn deliver_direct(
    root: &DriveFolder,
    account_id: &str,
    settings: &SendSettings,
    entry: &mut OutboxEntry,
    pending: Vec<String>,
    wire: &[u8],
    signed: bool,
    now: i64,
    transport: &mut dyn Transport,
) {
    let account_dir = account::account_dir(root, account_id);
    let mut policy = PolicyList::load(&account_dir);
    let mut changed = false;

    // The connection first.
    if !settings.ignore_policy {
        if policy.port25_blocked(now) {
            hold_back(entry, &pending, PORT25_BLOCKED);
            return;
        }
        if policy.port25.as_ref().is_some_and(|check| !check.open) {
            // Found blocked a while ago: this may be another network now.
            match transport.probe_port_25() {
                Port25Probe::Open => changed |= policy.note_port25_open(now),
                Port25Probe::Blocked(detail) => {
                    policy.port25 = Some(Port25Check {
                        open: false,
                        checked: now,
                        detail,
                    });
                    let _ = policy.save(&account_dir);
                    hold_back(entry, &pending, PORT25_BLOCKED);
                    return;
                }
                Port25Probe::Unknown(_) => {}
            }
        }
    }

    let mut domains: Vec<(String, Vec<String>)> = Vec::new();
    for address in pending {
        let domain = micromail::message::address_domain(&address).unwrap_or_default();
        match domains.iter_mut().find(|(d, _)| *d == domain) {
            Some((_, group)) => group.push(address),
            None => domains.push((domain, vec![address])),
        }
    }
    // Recipients whose domain's exchangers could not even be connected to.
    let mut unreached: Vec<String> = Vec::new();
    // Some exchanger answered: port 25 is open.
    let mut reached = false;
    for (domain, group) in domains {
        if !settings.ignore_policy {
            if let Some(held) = policy.holds_back(&domain, signed) {
                let why = if held.source == PolicySource::Default {
                    format!(
                        "{domain} takes mail only from a trusted relay: choose an SMTP server \
                         under Settings, Sending"
                    )
                } else {
                    held.reason.clone()
                };
                hold_back(entry, &group, &why);
                continue;
            }
        }
        let outcomes = transport.deliver(None, &entry.from, &group, wire);
        let mut no_answer = !outcomes.is_empty();
        for outcome in &outcomes {
            let answered = match &outcome.status {
                RecipientStatus::Deferred { reply, .. } => reply.is_some(),
                RecipientStatus::Accepted { server } | RecipientStatus::Rejected { server, .. } => {
                    !server.is_empty()
                }
            };
            reached |= answered;
            if !matches!(outcome.status, RecipientStatus::Deferred { reply: None, .. }) {
                no_answer = false;
            }
        }
        record(entry, &outcomes);
        for outcome in &outcomes {
            let RecipientStatus::Rejected { reply, server } = &outcome.status else {
                continue;
            };
            // Only the domain's own server's answer about the sender says something about the
            // route.
            if server.is_empty() || !says_relay_needed(reply) {
                continue;
            }
            changed |= policy.learn(&domain, reply, now);
            let why = format!(
                "{} ({server}: {reply})",
                refusal_cause(reply).explain(&domain)
            );
            if let Some(recipient) = entry
                .recipients
                .iter_mut()
                .find(|r| r.address.eq_ignore_ascii_case(&outcome.address))
            {
                recipient.reason = why;
            }
        }
        if no_answer {
            unreached.extend(group);
        }
    }

    if !unreached.is_empty() && !reached {
        // Is it those domains, or this connection's port 25?
        match transport.probe_port_25() {
            Port25Probe::Blocked(detail) => {
                policy.port25 = Some(Port25Check {
                    open: false,
                    checked: now,
                    detail,
                });
                changed = true;
                hold_back(entry, &unreached, PORT25_BLOCKED);
            }
            Port25Probe::Open => changed |= policy.note_port25_open(now),
            Port25Probe::Unknown(_) => {}
        }
    }
    if reached {
        changed |= policy.note_port25_open(now);
    }
    if changed {
        let _ = policy.save(&account_dir);
    }
}

fn record(entry: &mut OutboxEntry, outcomes: &[RecipientOutcome]) {
    for outcome in outcomes {
        let Some(recipient) = entry
            .recipients
            .iter_mut()
            .find(|r| r.address.eq_ignore_ascii_case(&outcome.address))
        else {
            continue;
        };
        match &outcome.status {
            RecipientStatus::Accepted { server } => {
                recipient.state = RecipientProgress::Sent;
                recipient.server = server.clone();
                recipient.code = None;
                recipient.reason = String::new();
            }
            RecipientStatus::Deferred { reply, reason } => {
                recipient.state = RecipientProgress::Pending;
                recipient.code = reply.as_ref().map(|r| r.code);
                recipient.reason = reason.clone();
            }
            RecipientStatus::Rejected { reply, server } => {
                recipient.state = RecipientProgress::Failed;
                recipient.server = server.clone();
                recipient.code = Some(reply.code);
                recipient.reason = if server.is_empty() {
                    reply.to_string()
                } else {
                    format!("{server}: {reply}")
                };
            }
        }
    }
}

/// The wait before attempt `attempts + 1`: 5 minutes, doubling, at most 4 hours.
fn backoff_secs(attempts: u32) -> i64 {
    let shift = attempts.saturating_sub(1).min(6);
    (300_i64 << shift).min(4 * 3600)
}

/// Files the entry by its recipients' states and says what became of the mail.
/// `bytes` is the message as it went out (what Sent keeps); `waiting`: the attempt waited for
/// the user, so the mail is due again at once instead of after a backoff.
fn finish(
    root: &DriveFolder,
    account_id: &str,
    entry: &mut OutboxEntry,
    bytes: &[u8],
    now: i64,
    waiting: bool,
) -> SendStatus {
    let outbox = MailStore::new(outbox_dir(root, account_id));
    if entry.count(RecipientProgress::Pending) > 0 && now - entry.created >= GIVE_UP_AFTER_SECS {
        for recipient in entry
            .recipients
            .iter_mut()
            .filter(|r| r.state == RecipientProgress::Pending)
        {
            recipient.state = RecipientProgress::Failed;
            recipient.reason = format!("given up after 5 days ({})", recipient.reason);
        }
    }
    let sent = entry.count(RecipientProgress::Sent);
    let failed = entry.count(RecipientProgress::Failed);
    let pending = entry.count(RecipientProgress::Pending);
    let failures: Vec<String> = entry
        .recipients
        .iter()
        .filter(|r| r.state == RecipientProgress::Failed)
        .map(|r| format!("{} ({})", r.address, r.reason))
        .collect();

    if pending == 0 && sent > 0 {
        // Out of the outbox, into Sent.
        let filed = file_message(
            &sent_store_root(root, account_id),
            Role::Sent.key().unwrap_or("sent"),
            bytes,
            &[String::from("\\Seen")],
            now,
        );
        if let Err(e) = filed {
            // Delivered, but not filed: keep it in the outbox (not queued) rather than lose it.
            entry.state = OutboxState::Failed;
            let _ = write_entry(&outbox, entry);
            return SendStatus::Failed {
                reason: format!("Sent, but it could not be put into the Sent folder: {e}"),
            };
        }
        let _ = outbox.delete(&format!("{}.eml", entry.id));
        let _ = outbox.delete(&format!("{}.json", entry.id));
        if failed == 0 {
            return SendStatus::Sent {
                message_id: entry.message_id.clone(),
            };
        }
        return SendStatus::Failed {
            reason: format!(
                "Not delivered to {}. The other recipients got it.",
                failures.join(", ")
            ),
        };
    }

    if pending == 0 {
        entry.state = OutboxState::Failed;
        let _ = write_entry(&outbox, entry);
        return SendStatus::Failed {
            reason: format!("Not delivered to {}.", failures.join(", ")),
        };
    }

    entry.state = OutboxState::Queued;
    entry.next_attempt = if waiting {
        now
    } else {
        now + backoff_secs(entry.attempts)
    };
    let _ = write_entry(&outbox, entry);
    let waiting: Vec<String> = entry
        .recipients
        .iter()
        .filter(|r| r.state == RecipientProgress::Pending)
        .map(|r| {
            if r.reason.is_empty() {
                r.address.clone()
            } else {
                format!("{} ({})", r.address, r.reason)
            }
        })
        .collect();
    if failed > 0 {
        return SendStatus::Failed {
            reason: format!(
                "Not delivered to {}. Still trying: {}.",
                failures.join(", "),
                waiting.join(", ")
            ),
        };
    }
    SendStatus::Queued {
        reason: format!("Waiting to try again: {}.", waiting.join(", ")),
    }
}

// ==== the Sent folder ====

/// Where the account's mail folders are (its `folder` in `account.json`, else its own folder).
fn sent_store_root(root: &DriveFolder, account_id: &str) -> DriveFolder {
    match account::load(root, account_id) {
        Some(Ok(account)) => account::mail_root(root, &account),
        _ => account::account_dir(root, account_id),
    }
}

/// Files a message written on this computer into the folder `folder_key` of the mail store in
/// `store_root`, as a synced message is: `mail/<folder>/<yyyy>/<mm>/<uid>.eml` (the month of
/// `now`), its line in `index.jsonl`, and a `state.json` (created for a folder never synced;
/// a synced folder's UIDVALIDITY and last UID stay as they are). The UID comes from
/// [`LOCAL_UID_FLOOR`] up. AzMail files sent mail this way; drafts can be filed the same way.
pub fn file_message(
    store_root: &DriveFolder,
    folder_key: &str,
    bytes: &[u8],
    flags: &[String],
    now: i64,
) -> std::io::Result<IndexEntry> {
    let store = MailStore::new(store_root.clone());
    let index_key = store::index_key(folder_key);
    let mut entries: Vec<IndexEntry> = store
        .get(&index_key)
        .map(|b| store::index_from_jsonl(&String::from_utf8_lossy(&b)))
        .unwrap_or_default();
    let uid = entries
        .iter()
        .map(|e| e.uid)
        .filter(|&uid| uid >= LOCAL_UID_FLOOR)
        .max()
        .map_or(LOCAL_UID_FLOOR, |uid| uid.saturating_add(1));
    let (year, month) = message::year_month(now);
    let path = store::message_key(folder_key, year, month, uid);
    store.put(&path, bytes)?;
    let entry = message::index_entry(uid, bytes, flags, Some(now), &path);
    entries.push(entry.clone());
    store.put(&index_key, store::index_to_jsonl(&entries).as_bytes())?;
    let state_key = store::state_key(folder_key);
    let mut state = store
        .get(&state_key)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|text| FolderState::from_json(&text))
        .unwrap_or_else(|| {
            let display = Role::of_key(folder_key).label().unwrap_or(folder_key);
            FolderState::create(display, display, 0)
        });
    state.messages = entries.len() as u64;
    store.put(&state_key, state.to_json().as_bytes())?;
    Ok(entry)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::{submit::SubmitSecurity, testutil::{MailFolder, TempDir}};

    /// 2026-10-01T08:30:00Z
    const OCT_1: i64 = 1_790_843_400;
    const ACCOUNT: &str = "ada@example.org";

    /// A throwaway 1024-bit RSA key, PKCS#8, made for these tests only (the PEM markers are
    /// split so no scanner takes the source for a leaked key).
    const TEST_KEY: &str = concat!(
        "-----BEGIN ",
        "PRIVATE KEY-----\n",
        "MIICdgIBADANBgkqhkiG9w0BAQEFAASCAmAwggJcAgEAAoGBAK9svAXG/GucB2/R\n",
        "OA7Fq+awV6fCCVbobJ2uNlhQIqTmJ3v/T/2SwKh9lhKDirXj9PwcqxJdOnqZDErY\n",
        "MsiDu+ix3dmW5mxcG3FFg7U/FsMJkANFkSvRrQiA5LQq/xdYWjD89JsvIHmicXlz\n",
        "aVsx657iz9PeyYtaWdxL/i12rfqvAgMBAAECgYB7oJuZTrSRebJcAQwKjRAqUVhU\n",
        "55ABaWcycJXoAwGHSJPG9RUAVS3lECx0+7MDoJUEH4gINx+BSt642Ehhu0Tu+xut\n",
        "pgiefm1SwH2NNh3teXHtvhRskK78sj8NGkvwb9mtTaUlp0aUHZNzgHGO/QnHqeyY\n",
        "GBwnvsPj6SClDubpEQJBAOKtrbgw/yJEEg9aaF6Lb0RSsZAmsjU4SJYAIxjwuhWx\n",
        "lQQFj2PRs0EsWnm46yhzNFCTZU8NZmx9qlyBFCj6SlUCQQDGHdILe2P8X2LFZZXj\n",
        "h/LJM01indeZk4baY0SKAfpEXXydbj8KZQbxEy94qG0HjF3TDVy+NFXSmka5+bBh\n",
        "d7zzAkEAjBBDClAEJfknq6LyYJEJtI7gNrEiZm4bs8vr4+pDIUp0SGLjIgueFoRA\n",
        "d3wCmiDtT2h0Le+avSi9DqGXgmZ9bQJAKZJoWPBzcqmxWCqQ4UXNtFqHioIEk71Z\n",
        "NspNv4fatC3J0F8p60x3wG5+L5toBYV2yqqrI15oA+FLpgq28Dzn8QJATS90urxx\n",
        "KtSKrA6f2G0TOmX3GHlzNdIgtMTA45wuSrK95iln2T0aue37BVtCuUq22UPobkZb\n",
        "Uvy4lwqlqL/aoA==\n",
        "-----END ",
        "PRIVATE KEY-----\n",
    );

    fn mail() -> OutgoingMail {
        OutgoingMail {
            from: "Ada Lovelace <ada@example.org>".to_string(),
            to: vec!["ben@example.net".to_string()],
            cc: vec![],
            bcc: vec![],
            subject: "Lunch".to_string(),
            text_body: "See you at noon.\n".to_string(),
            html_body: None,
            in_reply_to: None,
            references: vec![],
            attachments: vec![],
        }
    }

    fn accepted(address: &str) -> RecipientOutcome {
        RecipientOutcome {
            address: address.to_string(),
            status: RecipientStatus::Accepted {
                server: "mx.test".to_string(),
            },
        }
    }

    fn reply(code: u16, text: &str) -> Reply {
        let enhanced = text
            .split_whitespace()
            .next()
            .filter(|w| w.matches('.').count() == 2)
            .map(str::to_string);
        Reply {
            code,
            enhanced,
            text: text.to_string(),
        }
    }

    /// A transport that answers every recipient with what `answer` says and records the calls,
    /// the messages and the port-25 probes (answered with `probe`). Submission (`submit`)
    /// answers the same way, or fails with `submit_failure`, and records each target.
    struct Fake {
        answer: Box<dyn FnMut(&str) -> RecipientStatus>,
        calls: Vec<(Option<(String, u16)>, Vec<String>)>,
        messages: Vec<Vec<u8>>,
        probe: Port25Probe,
        probes: usize,
        submit_failure: Option<SubmitFailure>,
        submits: Vec<(SubmitTarget, Vec<String>)>,
    }

    impl Fake {
        fn new(answer: impl FnMut(&str) -> RecipientStatus + 'static) -> Fake {
            Fake {
                answer: Box::new(answer),
                calls: Vec::new(),
                messages: Vec::new(),
                probe: Port25Probe::Unknown(String::from("this test does not probe")),
                probes: 0,
                submit_failure: None,
                submits: Vec::new(),
            }
        }
    }

    impl Transport for Fake {
        fn deliver(
            &mut self,
            relay: Option<(&str, u16)>,
            _from: &str,
            recipients: &[String],
            message: &[u8],
        ) -> Vec<RecipientOutcome> {
            self.calls
                .push((relay.map(|(h, p)| (h.to_string(), p)), recipients.to_vec()));
            self.messages.push(message.to_vec());
            recipients
                .iter()
                .map(|r| RecipientOutcome {
                    address: r.clone(),
                    status: (self.answer)(r),
                })
                .collect()
        }

        fn probe_port_25(&mut self) -> Port25Probe {
            self.probes += 1;
            self.probe.clone()
        }

        fn submit(
            &mut self,
            target: &SubmitTarget,
            _from: &str,
            recipients: &[String],
            message: &[u8],
        ) -> Result<Vec<RecipientOutcome>, SubmitFailure> {
            self.submits.push((target.clone(), recipients.to_vec()));
            if let Some(failure) = &self.submit_failure {
                return Err(failure.clone());
            }
            self.messages.push(message.to_vec());
            Ok(recipients
                .iter()
                .map(|r| RecipientOutcome {
                    address: r.clone(),
                    status: (self.answer)(r),
                })
                .collect())
        }
    }

    fn take_all() -> Fake {
        Fake::new(|_| RecipientStatus::Accepted {
            server: "mx.test".to_string(),
        })
    }

    fn sent_index(root: &Path) -> Vec<IndexEntry> {
        let text = std::fs::read_to_string(root.join(ACCOUNT).join("mail/sent/index.jsonl"))
        .unwrap_or_default();
        store::index_from_jsonl(&text)
    }

    fn outbox_files(root: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(root.join(ACCOUNT).join(OUTBOX_DIR))
            .map(|dir| {
                dir.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    // ---- the message ----

    #[test]
    fn a_mail_is_built_as_rfc_5322_with_crlf_and_bcc_only_in_the_envelope() {
        let mut mail = mail();
        mail.cc = vec!["Cy <cy@example.com>".to_string()];
        mail.bcc = vec!["secret@example.com".to_string()];
        mail.in_reply_to = Some("<m0@example.net>".to_string());
        mail.references = vec!["<m0@example.net>".to_string()];
        let mut builder = builder(&mail, OCT_1);
        builder.message_id = Some("m1@example.org".to_string());
        let built = builder.build();
        assert_eq!(
            String::from_utf8(built.bytes).unwrap(),
            "Date: Thu, 01 Oct 2026 08:30:00 +0000\r\n\
             From: Ada Lovelace <ada@example.org>\r\n\
             To: ben@example.net\r\n\
             Cc: Cy <cy@example.com>\r\n\
             Subject: Lunch\r\n\
             Message-ID: <m1@example.org>\r\n\
             In-Reply-To: <m0@example.net>\r\n\
             References: <m0@example.net>\r\n\
             User-Agent: AzMail\r\n\
             MIME-Version: 1.0\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\
             Content-Transfer-Encoding: 7bit\r\n\
             \r\n\
             See you at noon.\r\n"
        );
    }

    #[test]
    fn html_and_attachments_nest_alternative_inside_mixed_and_parse_back() {
        let mut mail = mail();
        mail.subject = "Grüße aus Köln".to_string();
        mail.text_body = "Hallo Ben,\n\nder Plan hängt an.".to_string();
        mail.html_body = Some("<p>Hallo Ben,</p><p>der Plan h&auml;ngt an.</p>".to_string());
        mail.attachments = vec![Attachment {
            file_name: "Plan für Montag.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            bytes: b"%PDF-1.4 tiny".to_vec(),
        }];
        let built = build_message(&mail, OCT_1);
        let text = String::from_utf8(built.bytes.clone()).unwrap();
        assert!(
            text.contains("Subject: =?utf-8?B?R3LDvMOfZSBhdXMgS8O2bG4=?=\r\n"),
            "{text}"
        );
        assert!(text.contains("Content-Type: multipart/mixed;"), "{text}");
        assert!(
            text.contains("Content-Type: multipart/alternative;"),
            "{text}"
        );
        assert!(
            text.contains("filename*=utf-8''Plan%20f%C3%BCr%20Montag.pdf"),
            "{text}"
        );
        // What a reader sees: mail-parser (the parser AzMail reads synced mail with).
        let parsed = mail_parser::MessageParser::default()
            .parse(&built.bytes[..])
            .expect("the message parses");
        assert_eq!(parsed.subject(), Some("Grüße aus Köln"));
        assert_eq!(
            parsed
                .body_text(0)
                .map(|text| text.replace("\r\n", "\n").trim_end().to_string()),
            Some("Hallo Ben,\n\nder Plan hängt an.".to_string())
        );
        assert!(parsed.body_html(0).unwrap().contains("Hallo Ben"));
        let attachment = parsed.attachment(0).expect("one attachment");
        use mail_parser::MimeHeaders;
        assert_eq!(attachment.attachment_name(), Some("Plan für Montag.pdf"));
        assert_eq!(attachment.contents(), b"%PDF-1.4 tiny");
    }

    #[test]
    fn a_line_that_starts_with_a_dot_is_doubled_on_the_wire_only() {
        let mut mail = mail();
        mail.text_body = "steps:\n.config was missing\n.\nend".to_string();
        let built = build_message(&mail, OCT_1);
        let text = String::from_utf8(built.bytes.clone()).unwrap();
        assert!(
            text.contains("\r\n.config was missing\r\n.\r\nend\r\n"),
            "{text}"
        );
        let wire = String::from_utf8(micromail::message::wire_data(&built.bytes)).unwrap();
        assert!(
            wire.contains("\r\n..config was missing\r\n..\r\nend\r\n.\r\n"),
            "{wire}"
        );
        assert!(wire.ends_with("end\r\n.\r\n"), "{wire}");
    }

    #[test]
    fn a_dkim_signature_goes_in_front_and_the_key_never_into_a_file() {
        let dir = TempDir::new("send");
        let key_file = dir.0.join("dkim.pem");
        std::fs::write(&key_file, TEST_KEY).unwrap();
        let settings = SendSettings {
            dkim: Some(DkimSettings {
                domain: "example.org".to_string(),
                selector: "azmail".to_string(),
                key_file: Some(key_file),
                public_key: String::new(),
            }),
            ..SendSettings::default()
        };
        let signed = sign(build_message(&mail(), OCT_1).bytes, &settings, ACCOUNT).unwrap();
        let text = String::from_utf8(signed).unwrap();
        assert!(
            text.starts_with("DKIM-Signature: v=1; a=rsa-sha256; s=azmail; d=example.org;"),
            "{text}"
        );
        // From the keyring (in memory) instead of a file.
        let from_keyring = SendSettings {
            dkim: Some(DkimSettings {
                domain: "example.org".to_string(),
                selector: "azmail".to_string(),
                key_file: None,
                public_key: String::new(),
            }),
            dkim_key: Some(Secret::new(TEST_KEY.to_string())),
            ..SendSettings::default()
        };
        assert!(sign(
            b"From: a@example.org\r\n\r\nx\r\n".to_vec(),
            &from_keyring,
            ACCOUNT
        )
        .unwrap()
        .starts_with(b"DKIM-Signature:"));
        let saved = from_keyring.save(&dir.folder(), ACCOUNT).unwrap();
        let json = std::fs::read_to_string(saved).unwrap();
        assert!(
            !json.contains("PRIVATE KEY") && !json.contains("MIIC"),
            "{json}"
        );
        assert_eq!(
            SendSettings::load(&dir.folder(), ACCOUNT),
            SendSettings {
                dkim_key: None,
                ..from_keyring.clone()
            }
        );
        // No key at all: an error that names where the key was looked for, not a panic.
        let missing = SendSettings {
            dkim_key: None,
            ..from_keyring
        };
        let err = sign(b"x".to_vec(), &missing, ACCOUNT).unwrap_err();
        assert!(err.contains("AzMail/ada@example.org/dkim"), "{err}");
    }

    #[test]
    fn the_settings_default_to_direct_delivery_and_round_trip_through_json() {
        let settings = SendSettings::default();
        assert_eq!(settings.route, SendRoute::Direct);
        assert_eq!(settings.tls, TlsPolicy::Opportunistic);
        assert_eq!(settings.direct_port(), 25);
        let smtp = SendSettings {
            route: SendRoute::Smtp {
                host: "127.0.0.1".to_string(),
                port: 2525,
            },
            tls: TlsPolicy::Off,
            ..SendSettings::default()
        };
        let json = serde_json::to_string(&smtp).unwrap();
        assert!(json.contains("\"kind\":\"smtp\""), "{json}");
        assert!(json.contains("\"tls\":\"off\""), "{json}");
        assert_eq!(serde_json::from_str::<SendSettings>(&json).unwrap(), smtp);
        // Old or partial files read with defaults for what they lack.
        assert_eq!(
            serde_json::from_str::<SendSettings>("{}").unwrap(),
            SendSettings::default()
        );
        assert_eq!(
            SendSettings::load(
                &DriveFolder::outside(PathBuf::from("/nonexistent/azmail")),
                ACCOUNT
            ),
            SendSettings::default()
        );
    }

    // ---- the policy list ----

    #[test]
    fn the_policy_list_ships_with_defaults_in_the_account_folder() {
        let dir = TempDir::new("send");
        let list = PolicyList::load(&dir.folder());
        assert_eq!(list.route_for("gmail.com"), DomainRoute::Relay);
        assert_eq!(list.route_for("GoogleMail.com."), DomainRoute::Relay);
        assert_eq!(list.route_for("yahoo.com"), DomainRoute::Relay);
        assert_eq!(list.route_for("outlook.com"), DomainRoute::Direct);
        assert_eq!(list.route_for("hotmail.com"), DomainRoute::Direct);
        assert_eq!(list.route_for("example.org"), DomainRoute::Direct);
        // A subdomain follows its parent.
        assert_eq!(list.route_for("eu.yahoo.com"), DomainRoute::Relay);
        let json = std::fs::read_to_string(dir.0.join(POLICY_FILE)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["format"], "azmail.send-policy");
        assert_eq!(value["domains"]["gmail.com"]["route"], "relay");
        assert_eq!(value["domains"]["outlook.com"]["route"], "direct");
        assert_eq!(PolicyList::load(&dir.folder()), list);
    }

    #[test]
    fn a_5xx_about_the_sender_teaches_the_list_and_one_about_the_address_does_not() {
        let mut list = PolicyList::default();
        assert!(!list.learn("example.org", &reply(550, "5.1.1 no such user"), OCT_1));
        assert!(!list.learn("example.org", &reply(552, "5.2.2 mailbox full"), OCT_1));
        assert!(!list.learn(
            "example.org",
            &reply(550, "Requested action not taken: mailbox unavailable"),
            OCT_1
        ));
        assert!(!list.learn("example.org", &reply(451, "4.7.1 greylisted"), OCT_1));
        assert_eq!(list.route_for("example.org"), DomainRoute::Direct);
        assert!(list.learn(
            "Example.org",
            &reply(
                550,
                "5.7.1 Client host [203.0.113.7] blocked using Spamhaus"
            ),
            OCT_1
        ));
        let entry = list.entry_for("example.org").unwrap();
        assert_eq!(entry.route, DomainRoute::Relay);
        assert_eq!(entry.source, PolicySource::Learned);
        assert!(entry.reason.contains("Spamhaus"), "{}", entry.reason);
        assert_eq!(entry.updated, "2026-10-01T08:30:00Z");
        // A choice the user made stays.
        list.domains.insert(
            "mine.example".to_string(),
            DomainPolicy {
                route: DomainRoute::Direct,
                source: PolicySource::User,
                reason: String::new(),
                updated: String::new(),
                cause: RefusalCause::Unknown,
            },
        );
        assert!(!list.learn("mine.example", &reply(554, "5.7.1 no"), OCT_1));
        assert_eq!(list.route_for("mine.example"), DomainRoute::Direct);
    }

    // ---- the outbox state machine ----

    #[test]
    fn a_mail_every_server_takes_moves_from_the_outbox_into_sent() {
        let dir = TempDir::new("send");
        let mut fake = take_all();
        let mut mail = mail();
        mail.cc = vec!["cy@example.com".to_string()];
        mail.bcc = vec![
            "Ben <BEN@example.net>".to_string(),
            "dee@example.com".to_string(),
        ];
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            &mail,
            OCT_1,
            &mut fake,
        );
        let SendStatus::Sent { message_id } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(message_id.ends_with("@example.org") && !message_id.starts_with('<'));
        // One attempt per domain, the duplicate Bcc folded into the To.
        assert_eq!(fake.calls.len(), 2);
        assert_eq!(fake.calls[0], (None, vec!["ben@example.net".to_string()]));
        assert_eq!(
            fake.calls[1],
            (
                None,
                vec!["cy@example.com".to_string(), "dee@example.com".to_string()]
            )
        );
        assert!(
            outbox_files(&dir.0).is_empty(),
            "{:?}",
            outbox_files(&dir.0)
        );
        let index = sent_index(&dir.0);
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].uid, LOCAL_UID_FLOOR);
        assert_eq!(index[0].message_id, message_id);
        assert_eq!(index[0].subject, "Lunch");
        assert_eq!(index[0].flags, vec!["\\Seen".to_string()]);
        assert_eq!(
            index[0].path,
            format!("mail/sent/2026/10/{LOCAL_UID_FLOOR}.eml")
        );
        let eml =
            std::fs::read(dir.0.join(ACCOUNT).join(&index[0].path)).unwrap();
        assert!(
            !String::from_utf8_lossy(&eml).contains("dee@example.com"),
            "Bcc leaked"
        );
        let state = std::fs::read_to_string(
            dir.0.join(ACCOUNT).join("mail/sent/state.json"),
        )
        .unwrap();
        let state = FolderState::from_json(&state).expect("a state file MAIL1 reads");
        assert_eq!(state.messages, 1);
        assert_eq!(state.display, "Sent");
        // A second mail gets the next local UID.
        send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            &mail,
            OCT_1 + 60,
            &mut take_all(),
        );
        let uids: Vec<u32> = sent_index(&dir.0).iter().map(|e| e.uid).collect();
        assert_eq!(uids, vec![LOCAL_UID_FLOOR, LOCAL_UID_FLOOR + 1]);
    }

    #[test]
    fn a_temporary_failure_queues_the_mail_and_a_retry_sends_only_to_who_is_left() {
        let dir = TempDir::new("send");
        let mut mail = mail();
        mail.to.push("cy@example.com".to_string());
        let mut first = Fake::new(|address| {
            if address.ends_with("example.com") {
                RecipientStatus::Deferred {
                    reply: Some(Reply {
                        code: 451,
                        enhanced: Some("4.7.1".to_string()),
                        text: "4.7.1 greylisted, try again in 5 minutes".to_string(),
                    }),
                    reason: "mx.example.com: 451 4.7.1 greylisted".to_string(),
                }
            } else {
                RecipientStatus::Accepted {
                    server: "mx.example.net".to_string(),
                }
            }
        });
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            &mail,
            OCT_1,
            &mut first,
        );
        let SendStatus::Queued { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(
            reason.contains("cy@example.com") && reason.contains("greylisted"),
            "{reason}"
        );
        let files = outbox_files(&dir.0);
        assert_eq!(files.len(), 2, "{files:?}");
        assert!(files[0].ends_with(".eml") && files[1].ends_with(".json"));
        let entries = outbox_entries(&dir.folder(), ACCOUNT);
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.state, OutboxState::Queued);
        assert_eq!(entry.attempts, 1);
        assert_eq!(entry.next_attempt, OCT_1 + 300);
        assert_eq!(entry.recipients[0].state, RecipientProgress::Sent);
        assert_eq!(entry.recipients[1].state, RecipientProgress::Pending);
        assert_eq!(entry.recipients[1].code, Some(451));
        assert!(sent_index(&dir.0).is_empty());

        // Not due yet: nothing happens.
        let mut second = take_all();
        assert!(retry_outbox_with(
            &dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            false,
            OCT_1 + 60,
            &mut second
        )
        .is_empty());
        assert!(second.calls.is_empty());
        // Due: only the one still pending is sent to, and the mail lands in Sent.
        let results = retry_outbox_with(
            &dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            false,
            OCT_1 + 300,
            &mut second,
        );
        assert_eq!(results.len(), 1);
        assert!(
            matches!(results[0].1, SendStatus::Sent { .. }),
            "{results:?}"
        );
        assert_eq!(
            second.calls,
            vec![(None, vec!["cy@example.com".to_string()])]
        );
        assert!(outbox_files(&dir.0).is_empty());
        assert_eq!(sent_index(&dir.0).len(), 1);
    }

    #[test]
    fn a_5xx_in_direct_mode_fails_and_puts_the_domain_into_the_policy_list() {
        let dir = TempDir::new("send");
        let mut fake = Fake::new(|_| RecipientStatus::Rejected {
            reply: Reply {
                code: 550,
                enhanced: Some("5.7.26".to_string()),
                text: "5.7.26 Unauthenticated email is not accepted from this domain".to_string(),
            },
            server: "mx.example.net".to_string(),
        });
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            &mail(),
            OCT_1,
            &mut fake,
        );
        let SendStatus::Failed { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(
            reason.contains("ben@example.net") && reason.contains("5.7.26"),
            "{reason}"
        );
        let policy = PolicyList::load(&account::account_dir(&dir.folder(), ACCOUNT));
        assert_eq!(policy.route_for("example.net"), DomainRoute::Relay);
        // Nobody got it: it stays in the outbox, failed, for the user to fix or delete.
        let entries = outbox_entries(&dir.folder(), ACCOUNT);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].state, OutboxState::Failed);
        assert!(sent_index(&dir.0).is_empty());
        // A failed entry is not retried.
        let mut again = take_all();
        assert!(retry_outbox_with(
            &dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            true,
            OCT_1 + 9999,
            &mut again
        )
        .is_empty());
        assert!(again.calls.is_empty());
    }

    #[test]
    fn a_mail_some_got_is_filed_in_sent_and_reported_as_failed_for_the_rest() {
        let dir = TempDir::new("send");
        let mut mail = mail();
        mail.to.push("nobody@example.net".to_string());
        let mut fake = Fake::new(|address| {
            if address.starts_with("nobody") {
                RecipientStatus::Rejected {
                    reply: Reply {
                        code: 550,
                        enhanced: Some("5.1.1".to_string()),
                        text: "5.1.1 no such user".to_string(),
                    },
                    server: "mx.example.net".to_string(),
                }
            } else {
                RecipientStatus::Accepted {
                    server: "mx.example.net".to_string(),
                }
            }
        });
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            &mail,
            OCT_1,
            &mut fake,
        );
        let SendStatus::Failed { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(
            reason.contains("nobody@example.net") && reason.contains("other recipients got it"),
            "{reason}"
        );
        assert_eq!(sent_index(&dir.0).len(), 1);
        assert!(outbox_files(&dir.0).is_empty());
        // "no such user" says nothing about the route.
        assert_eq!(
            PolicyList::load(&account::account_dir(&dir.folder(), ACCOUNT)).route_for("example.net"),
            DomainRoute::Direct
        );
    }

    #[test]
    fn a_domain_that_needs_a_relay_waits_in_direct_mode_unless_the_policy_is_ignored() {
        let dir = TempDir::new("send");
        let mut mail = mail();
        mail.to = vec!["someone@gmail.com".to_string()];
        let mut fake = take_all();
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            &mail,
            OCT_1,
            &mut fake,
        );
        let SendStatus::Queued { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(
            reason.contains("gmail.com takes mail only from a trusted relay"),
            "{reason}"
        );
        assert!(fake.calls.is_empty(), "{:?}", fake.calls);
        // Nerd mode: try anyway.
        let nerd = SendSettings {
            ignore_policy: true,
            ..SendSettings::default()
        };
        let results = retry_outbox_with(&dir.folder(), ACCOUNT, &nerd, true, OCT_1 + 1, &mut fake);
        assert!(
            matches!(results[0].1, SendStatus::Sent { .. }),
            "{results:?}"
        );
        assert_eq!(fake.calls.len(), 1);
    }

    #[test]
    fn the_smtp_route_sends_everyone_through_one_server_and_learns_nothing() {
        let dir = TempDir::new("send");
        let mut mail = mail();
        mail.to.push("someone@gmail.com".to_string());
        let settings = SendSettings {
            route: SendRoute::Smtp {
                host: "127.0.0.1".to_string(),
                port: 2525,
            },
            ..SendSettings::default()
        };
        let mut fake = Fake::new(|address| {
            if address.starts_with("ben") {
                RecipientStatus::Rejected {
                    reply: Reply {
                        code: 554,
                        enhanced: Some("5.7.1".to_string()),
                        text: "5.7.1 relay denied".to_string(),
                    },
                    server: "127.0.0.1".to_string(),
                }
            } else {
                RecipientStatus::Accepted {
                    server: "127.0.0.1".to_string(),
                }
            }
        });
        let status = send_mail_with(&dir.folder(), ACCOUNT, &settings, &mail, OCT_1, &mut fake);
        assert!(matches!(status, SendStatus::Failed { .. }), "{status:?}");
        assert_eq!(
            fake.calls,
            vec![(
                Some(("127.0.0.1".to_string(), 2525)),
                vec![
                    "ben@example.net".to_string(),
                    "someone@gmail.com".to_string()
                ]
            )]
        );
        // The relay's 5xx says nothing about example.net.
        assert_eq!(
            PolicyList::load(&account::account_dir(&dir.folder(), ACCOUNT)).route_for("example.net"),
            DomainRoute::Direct
        );
    }

    #[test]
    fn a_mail_still_queued_after_five_days_is_given_up() {
        let dir = TempDir::new("send");
        let mut fake = Fake::new(|_| RecipientStatus::Deferred {
            reply: None,
            reason: "could not connect to mx.example.net port 25".to_string(),
        });
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            &mail(),
            OCT_1,
            &mut fake,
        );
        assert!(matches!(status, SendStatus::Queued { .. }), "{status:?}");
        let results = retry_outbox_with(
            &dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            false,
            OCT_1 + GIVE_UP_AFTER_SECS,
            &mut fake,
        );
        let SendStatus::Failed { reason } = &results[0].1 else {
            panic!("{results:?}");
        };
        assert!(reason.contains("given up after 5 days"), "{reason}");
        assert_eq!(
            outbox_entries(&dir.folder(), ACCOUNT)[0].state,
            OutboxState::Failed
        );
    }

    #[test]
    fn no_sender_or_no_recipient_fails_before_anything_is_written() {
        let dir = TempDir::new("send");
        let mut fake = take_all();
        let mut no_from = mail();
        no_from.from = String::new();
        assert!(matches!(
            send_mail_with(
                &dir.folder(),
                ACCOUNT,
                &SendSettings::default(),
                &no_from,
                OCT_1,
                &mut fake
            ),
            SendStatus::Failed { .. }
        ));
        let mut no_to = mail();
        no_to.to.clear();
        assert!(matches!(
            send_mail_with(
                &dir.folder(),
                ACCOUNT,
                &SendSettings::default(),
                &no_to,
                OCT_1,
                &mut fake
            ),
            SendStatus::Failed { .. }
        ));
        assert!(fake.calls.is_empty());
        assert!(outbox_files(&dir.0).is_empty());
    }

    #[test]
    fn the_backoff_doubles_from_five_minutes_to_four_hours() {
        assert_eq!(backoff_secs(1), 300);
        assert_eq!(backoff_secs(2), 600);
        assert_eq!(backoff_secs(3), 1200);
        assert_eq!(backoff_secs(7), 4 * 3600);
        assert_eq!(backoff_secs(40), 4 * 3600);
    }

    // ---- client-side DKIM and direct delivery ----

    /// Settings that sign as example.org, with the test key in memory (as the keyring hands it
    /// over) or without it (`None`: not read yet).
    fn signing(key: Option<&str>) -> SendSettings {
        SendSettings {
            dkim: Some(DkimSettings {
                domain: "example.org".to_string(),
                selector: "azmail202610".to_string(),
                key_file: None,
                public_key: String::new(),
            }),
            dkim_key: key.map(|k| Secret::new(k.to_string())),
            ..SendSettings::default()
        }
    }

    /// The bytes of the first mail in Sent.
    fn sent_bytes(root: &Path) -> Vec<u8> {
        let index = sent_index(root);
        std::fs::read(root.join(ACCOUNT).join(&index[0].path)).unwrap()
    }

    fn unreachable(_: &str) -> RecipientStatus {
        RecipientStatus::Deferred {
            reply: None,
            reason: "could not connect to mx.example.net port 25".to_string(),
        }
    }

    #[test]
    fn a_signed_mail_is_tried_directly_where_only_a_shipped_default_asks_for_a_relay() {
        let dir = TempDir::new("send");
        let mut mail = mail();
        mail.to = vec!["someone@gmail.com".to_string()];
        let mut fake = take_all();
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &signing(Some(TEST_KEY)),
            &mail,
            OCT_1,
            &mut fake,
        );
        assert!(matches!(status, SendStatus::Sent { .. }), "{status:?}");
        assert_eq!(
            fake.calls,
            vec![(None, vec!["someone@gmail.com".to_string()])]
        );
        // What went out is signed, and the Sent copy is what went out.
        let wire = String::from_utf8_lossy(&fake.messages[0]).into_owned();
        assert!(wire.starts_with("DKIM-Signature: v=1; a=rsa-sha256;"), "{wire}");
        assert!(wire.contains("s=azmail202610; d=example.org;"), "{wire}");
        assert_eq!(sent_bytes(&dir.0), fake.messages[0]);
        // The shipped defaults hold back unsigned mail only.
        let defaults = PolicyList::default();
        assert!(defaults.holds_back("gmail.com", false).is_some());
        assert!(defaults.holds_back("gmail.com", true).is_none());
        assert!(defaults.holds_back("example.org", false).is_none());
        // A domain that refused before still waits for a relay, signed or not.
        let account_dir = account::account_dir(&dir.folder(), ACCOUNT);
        let mut policy = PolicyList::load(&account_dir);
        assert!(policy.learn(
            "gmail.com",
            &reply(
                550,
                "5.7.25 [203.0.113.7] The IP address sending this message does not have a PTR \
                 record setup"
            ),
            OCT_1
        ));
        policy.save(&account_dir).unwrap();
        let mut again = take_all();
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &signing(Some(TEST_KEY)),
            &mail,
            OCT_1 + 60,
            &mut again,
        );
        let SendStatus::Queued { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(reason.contains("PTR"), "{reason}");
        assert!(again.calls.is_empty(), "{:?}", again.calls);
    }

    #[test]
    fn the_outbox_keeps_the_mail_unsigned_until_the_key_is_there_and_then_signs_it() {
        let dir = TempDir::new("send");
        let mut fake = take_all();
        // DKIM is on, but the key has not come from the keyring yet.
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &signing(None),
            &mail(),
            OCT_1,
            &mut fake,
        );
        let SendStatus::Queued { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(reason.contains("AzMail/ada@example.org/dkim"), "{reason}");
        assert!(fake.calls.is_empty(), "nothing goes out unsigned");
        let entries = outbox_entries(&dir.folder(), ACCOUNT);
        assert_eq!(entries[0].attempts, 0, "waiting for the key is no attempt");
        assert_eq!(entries[0].next_attempt, OCT_1, "due as soon as the key is there");
        let eml = std::fs::read(
            dir.0
                .join(ACCOUNT)
                .join(OUTBOX_DIR)
                .join(format!("{}.eml", entries[0].id)),
        )
        .unwrap();
        assert!(eml.starts_with(b"Date: "), "the outbox keeps the unsigned message");
        // The next Send / Receive has the key: signed, sent, filed as it went out.
        let results = retry_outbox_with(
            &dir.folder(),
            ACCOUNT,
            &signing(Some(TEST_KEY)),
            false,
            OCT_1,
            &mut fake,
        );
        assert!(
            matches!(results[0].1, SendStatus::Sent { .. }),
            "{results:?}"
        );
        assert!(fake.messages[0].starts_with(b"DKIM-Signature: "));
        assert_eq!(sent_bytes(&dir.0), fake.messages[0]);
        // An account that does not sign sends the message as it is.
        let plain_dir = TempDir::new("send");
        let mut plain = take_all();
        send_mail_with(
            &plain_dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            &mail(),
            OCT_1,
            &mut plain,
        );
        assert!(plain.messages[0].starts_with(b"Date: "));
    }

    #[test]
    fn a_refusal_of_direct_mail_is_recorded_with_what_it_was_about() {
        let cases = [
            (
                550,
                "5.7.1 Service unavailable, Client host [203.0.113.7] blocked using Spamhaus. \
                 To request removal from this list see https://www.spamhaus.org/query/ip/203.0.113.7",
                RefusalCause::HomeAddress,
            ),
            (
                553,
                "5.7.1 [BL21] Connections not accepted from IP addresses on Spamhaus PBL",
                RefusalCause::HomeAddress,
            ),
            (
                554,
                "5.7.1 Dynamic IP addresses may not send mail directly",
                RefusalCause::HomeAddress,
            ),
            (
                550,
                "5.7.25 [203.0.113.7] The IP address sending this message does not have a PTR \
                 record setup",
                RefusalCause::NoReverseDns,
            ),
            (
                554,
                "5.7.1 Client host rejected: cannot find your reverse hostname, [203.0.113.7]",
                RefusalCause::NoReverseDns,
            ),
            (
                550,
                "5.7.26 This mail has been blocked because the sender is unauthenticated",
                RefusalCause::NotAuthenticated,
            ),
            (550, "5.7.1 SPF check failed", RefusalCause::NotAuthenticated),
            (554, "5.7.1 rejected", RefusalCause::Unknown),
        ];
        for (code, text, cause) in cases {
            assert_eq!(refusal_cause(&reply(code, text)), cause, "{text}");
        }
        // Direct delivery refused for the home address: failed, recorded with its cause and
        // explained in words.
        let dir = TempDir::new("send");
        let mut fake = Fake::new(|_| RecipientStatus::Rejected {
            reply: Reply {
                code: 550,
                enhanced: Some("5.7.1".to_string()),
                text: "5.7.1 Client host [203.0.113.7] blocked using Spamhaus PBL".to_string(),
            },
            server: "mx.example.net".to_string(),
        });
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &signing(Some(TEST_KEY)),
            &mail(),
            OCT_1,
            &mut fake,
        );
        let SendStatus::Failed { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(reason.contains("home"), "{reason}");
        assert!(reason.contains("Spamhaus"), "{reason}");
        let policy = PolicyList::load(&account::account_dir(&dir.folder(), ACCOUNT));
        let entry = policy.entry_for("example.net").unwrap();
        assert_eq!(entry.cause, RefusalCause::HomeAddress);
        assert_eq!(entry.source, PolicySource::Learned);
        assert!(entry.reason.contains("relay"), "{}", entry.reason);
        assert!(entry.reason.contains("Spamhaus"), "{}", entry.reason);
        let json = std::fs::read_to_string(dir.0.join(ACCOUNT).join(POLICY_FILE)).unwrap();
        assert!(json.contains("\"cause\": \"home_address\""), "{json}");
    }

    #[test]
    fn when_no_exchanger_answers_the_port_25_probe_decides_and_the_connection_is_remembered() {
        let dir = TempDir::new("send");
        let mut fake = Fake::new(unreachable);
        fake.probe = Port25Probe::Blocked(String::from(
            "gmail-smtp-in.l.google.com:25: timed out",
        ));
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &signing(Some(TEST_KEY)),
            &mail(),
            OCT_1,
            &mut fake,
        );
        let SendStatus::Queued { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(reason.contains("port 25"), "{reason}");
        assert_eq!(fake.probes, 1);
        let account_dir = account::account_dir(&dir.folder(), ACCOUNT);
        let policy = PolicyList::load(&account_dir);
        let check = policy.port25.clone().expect("the probe is recorded");
        assert!(!check.open);
        assert_eq!(check.checked, OCT_1);
        assert!(check.detail.contains("timed out"), "{}", check.detail);
        assert!(policy.port25_blocked(OCT_1 + 60));
        assert!(!policy.port25_blocked(OCT_1 + PORT25_RECHECK_SECS));
        // Within the hour another mail waits without knocking anywhere.
        let mut second = take_all();
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &signing(Some(TEST_KEY)),
            &mail(),
            OCT_1 + 60,
            &mut second,
        );
        let SendStatus::Queued { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(reason.contains("port 25"), "{reason}");
        assert!(second.calls.is_empty());
        assert_eq!(second.probes, 0);
        // An hour later (another network?): probed again; open, so both mails go out.
        let mut later = take_all();
        later.probe = Port25Probe::Open;
        let results = retry_outbox_with(
            &dir.folder(),
            ACCOUNT,
            &signing(Some(TEST_KEY)),
            true,
            OCT_1 + PORT25_RECHECK_SECS,
            &mut later,
        );
        assert_eq!(later.probes, 1);
        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .all(|(_, s)| matches!(s, SendStatus::Sent { .. })),
            "{results:?}"
        );
        assert!(PolicyList::load(&account_dir).port25.unwrap().open);
    }

    #[test]
    fn one_unreachable_exchanger_on_an_open_connection_is_only_a_temporary_failure() {
        let dir = TempDir::new("send");
        let mut fake = Fake::new(unreachable);
        fake.probe = Port25Probe::Open;
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            &mail(),
            OCT_1,
            &mut fake,
        );
        let SendStatus::Queued { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(reason.contains("could not connect to mx.example.net"), "{reason}");
        assert!(!reason.contains("blocks"), "{reason}");
        assert_eq!(fake.probes, 1);
        let policy = PolicyList::load(&account::account_dir(&dir.folder(), ACCOUNT));
        assert!(policy.port25.as_ref().is_some_and(|c| c.open));
        assert_eq!(policy.route_for("example.net"), DomainRoute::Direct);
        assert_eq!(
            outbox_entries(&dir.folder(), ACCOUNT)[0].next_attempt,
            OCT_1 + 300,
            "an ordinary backoff"
        );
        // Offline (the probe's DNS does not answer either): nothing is recorded.
        let offline_dir = TempDir::new("send");
        let mut offline = Fake::new(unreachable);
        offline.probe = Port25Probe::Unknown(String::from("no DNS"));
        send_mail_with(
            &offline_dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            &mail(),
            OCT_1,
            &mut offline,
        );
        assert!(PolicyList::load(&account::account_dir(&offline_dir.folder(), ACCOUNT))
            .port25
            .is_none());
        // A server that answers at all shows port 25 is open.
        let open_dir = TempDir::new("send");
        send_mail_with(
            &open_dir.folder(),
            ACCOUNT,
            &SendSettings::default(),
            &mail(),
            OCT_1,
            &mut take_all(),
        );
        assert!(PolicyList::load(&account::account_dir(&open_dir.folder(), ACCOUNT))
            .port25
            .is_some_and(|c| c.open));
    }

    // ---- submission: the account's own outgoing server, signed in (optional route) ----

    /// Saves the account file with `host:port` as its outgoing server, user name `ada`.
    fn submission_account(root: &DriveFolder, host: &str, port: u16, auth: account::AuthKind) {
        account::save(
            root,
            &account::Account {
                id: ACCOUNT.to_string(),
                email: ACCOUNT.to_string(),
                name: "Ada Lovelace".to_string(),
                username: "ada".to_string(),
                imap: account::Server {
                    host: "imap.example.org".to_string(),
                    port: 993,
                },
                smtp: account::Server {
                    host: host.to_string(),
                    port,
                },
                security: account::Security::Tls,
                auth,
                folder: None,
            },
        )
        .unwrap();
    }

    /// Settings that submit through the account's server, with the sign-in secret in memory
    /// (as the keyring hands it over) or without it (`None`: not read yet).
    fn submission(secret: Option<&str>) -> SendSettings {
        SendSettings {
            route: SendRoute::Submission,
            sign_in: secret.map(|s| Secret::new(s.to_string())),
            ..SendSettings::default()
        }
    }

    #[test]
    fn submission_is_a_route_of_its_own_and_direct_stays_the_default() {
        assert_eq!(SendSettings::default().route, SendRoute::Direct);
        let settings = submission(Some("app-password-1234"));
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"route\":{\"kind\":\"submission\"}"), "{json}");
        assert!(!json.contains("app-password-1234"), "the secret is never saved: {json}");
        assert_eq!(
            serde_json::from_str::<SendSettings>(&json).unwrap(),
            SendSettings {
                sign_in: None,
                ..settings.clone()
            }
        );
        let implicit = SendSettings {
            tls: TlsPolicy::Implicit,
            ..settings
        };
        let json = serde_json::to_string(&implicit).unwrap();
        assert!(json.contains("\"tls\":\"implicit\""), "{json}");
    }

    #[test]
    fn submission_signs_in_to_the_accounts_own_server_with_its_user_name_and_secret() {
        let dir = TempDir::new("send");
        submission_account(&dir.folder(), "smtp.example.org", 465, account::AuthKind::Password);
        let mut fake = take_all();
        let mut mail = mail();
        mail.cc = vec!["cy@example.com".to_string()];
        mail.bcc = vec!["dee@gmail.com".to_string()];
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &submission(Some("app-password-1234")),
            &mail,
            OCT_1,
            &mut fake,
        );
        assert!(matches!(status, SendStatus::Sent { .. }), "{status:?}");
        // One submission for everyone, no MX delivery, no port-25 probe.
        assert!(fake.calls.is_empty(), "{:?}", fake.calls);
        assert_eq!(fake.probes, 0);
        assert_eq!(fake.submits.len(), 1);
        let (target, recipients) = &fake.submits[0];
        assert_eq!(target.host, "smtp.example.org");
        assert_eq!(target.port, 465);
        assert_eq!(target.security, SubmitSecurity::ImplicitTls);
        assert_eq!(target.username, "ada");
        assert_eq!(target.auth, account::AuthKind::Password);
        assert_eq!(target.secret.expose(), "app-password-1234");
        assert_eq!(target.helo, "example.org");
        assert_eq!(
            recipients,
            &vec![
                "ben@example.net".to_string(),
                "cy@example.com".to_string(),
                "dee@gmail.com".to_string()
            ]
        );
        // gmail.com's shipped "needs a relay" is about direct delivery: submission is one.
        assert_eq!(sent_bytes(&dir.0), fake.messages[0]);
        // A token account signs in with its token, the same way.
        let oauth_dir = TempDir::new("send");
        submission_account(&oauth_dir.folder(), "smtp.office365.com", 587, account::AuthKind::Xoauth2);
        let mut oauth = take_all();
        send_mail_with(
            &oauth_dir.folder(),
            ACCOUNT,
            &submission(Some("ya29.token")),
            &mail,
            OCT_1,
            &mut oauth,
        );
        let (target, _) = &oauth.submits[0];
        assert_eq!(target.security, SubmitSecurity::StartTls);
        assert_eq!(target.auth, account::AuthKind::Xoauth2);
        assert_eq!(target.secret.expose(), "ya29.token");
    }

    #[test]
    fn submission_without_the_password_in_memory_waits_and_is_no_attempt() {
        let dir = TempDir::new("send");
        submission_account(&dir.folder(), "smtp.example.org", 587, account::AuthKind::Password);
        let mut fake = take_all();
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &submission(None),
            &mail(),
            OCT_1,
            &mut fake,
        );
        let SendStatus::Queued { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(reason.contains("AzMail/ada@example.org/imap"), "{reason}");
        assert!(fake.submits.is_empty(), "no sign-in without a secret");
        let entries = outbox_entries(&dir.folder(), ACCOUNT);
        assert_eq!(entries[0].attempts, 0, "waiting for the password is no attempt");
        assert_eq!(entries[0].next_attempt, OCT_1, "due as soon as the password is there");
        // The next Send / Receive has the password: sent.
        let results = retry_outbox_with(
            &dir.folder(),
            ACCOUNT,
            &submission(Some("app-password-1234")),
            false,
            OCT_1,
            &mut fake,
        );
        assert!(matches!(results[0].1, SendStatus::Sent { .. }), "{results:?}");
        // No account file: waits too, and says so.
        let lost = TempDir::new("send");
        let status = send_mail_with(
            &lost.folder(),
            ACCOUNT,
            &submission(Some("app-password-1234")),
            &mail(),
            OCT_1,
            &mut take_all(),
        );
        let SendStatus::Queued { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(reason.contains("account.json"), "{reason}");
    }

    #[test]
    fn a_refused_sign_in_waits_for_the_user_and_an_unreachable_server_is_tried_again_later() {
        let dir = TempDir::new("send");
        submission_account(&dir.folder(), "smtp.example.org", 587, account::AuthKind::Password);
        let mut refused = take_all();
        refused.submit_failure = Some(SubmitFailure::Waits(String::from(
            "smtp.example.org refused the sign-in (535 5.7.8 Username and Password not accepted)",
        )));
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &submission(Some("wrong")),
            &mail(),
            OCT_1,
            &mut refused,
        );
        let SendStatus::Queued { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(reason.contains("535 5.7.8"), "{reason}");
        let entries = outbox_entries(&dir.folder(), ACCOUNT);
        assert_eq!(entries[0].attempts, 0, "a refused sign-in is no attempt");
        assert_eq!(entries[0].next_attempt, OCT_1, "due again once the password is new");
        // The server cannot be reached: an ordinary attempt with its backoff.
        let other = TempDir::new("send");
        submission_account(&other.folder(), "smtp.example.org", 587, account::AuthKind::Password);
        let mut down = take_all();
        down.submit_failure = Some(SubmitFailure::Unreachable(String::from(
            "could not connect to smtp.example.org port 587",
        )));
        let status = send_mail_with(
            &other.folder(),
            ACCOUNT,
            &submission(Some("app-password-1234")),
            &mail(),
            OCT_1,
            &mut down,
        );
        let SendStatus::Queued { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(reason.contains("could not connect to smtp.example.org"), "{reason}");
        let entries = outbox_entries(&other.folder(), ACCOUNT);
        assert_eq!(entries[0].attempts, 1);
        assert_eq!(entries[0].next_attempt, OCT_1 + 300, "an ordinary backoff");
    }

    #[test]
    fn the_submission_servers_answer_per_recipient_counts_and_teaches_the_policy_list_nothing() {
        let dir = TempDir::new("send");
        submission_account(&dir.folder(), "smtp.example.org", 587, account::AuthKind::Password);
        let mut fake = Fake::new(|address| {
            if address.starts_with("cy@") {
                RecipientStatus::Rejected {
                    reply: Reply {
                        code: 550,
                        enhanced: Some("5.7.1".to_string()),
                        text: "5.7.1 Client host [203.0.113.7] blocked using Spamhaus".to_string(),
                    },
                    server: "smtp.example.org".to_string(),
                }
            } else {
                RecipientStatus::Accepted {
                    server: "smtp.example.org".to_string(),
                }
            }
        });
        let mut mail = mail();
        mail.cc = vec!["cy@example.com".to_string()];
        let status = send_mail_with(
            &dir.folder(),
            ACCOUNT,
            &submission(Some("app-password-1234")),
            &mail,
            OCT_1,
            &mut fake,
        );
        let SendStatus::Failed { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(reason.contains("cy@example.com"), "{reason}");
        assert!(reason.contains("other recipients got it"), "{reason}");
        // The provider's server answered, not example.com's exchanger: nothing is learned.
        let policy = PolicyList::load(&account::account_dir(&dir.folder(), ACCOUNT));
        assert_eq!(policy.route_for("example.com"), DomainRoute::Direct);
        assert!(policy.port25.is_none());
    }

    #[test]
    fn submission_never_sends_a_password_unencrypted_to_another_computer() {
        let dir = TempDir::new("send");
        submission_account(&dir.folder(), "smtp.example.org", 587, account::AuthKind::Password);
        let mut fake = take_all();
        let plain = SendSettings {
            tls: TlsPolicy::Off,
            ..submission(Some("app-password-1234"))
        };
        let status = send_mail_with(&dir.folder(), ACCOUNT, &plain, &mail(), OCT_1, &mut fake);
        let SendStatus::Queued { reason } = status.clone() else {
            panic!("{status:?}");
        };
        assert!(reason.contains("encrypt"), "{reason}");
        assert!(fake.submits.is_empty(), "nothing went out");
        assert_eq!(outbox_entries(&dir.folder(), ACCOUNT)[0].attempts, 0);
        // To a test server on this computer it may.
        let local = TempDir::new("send");
        submission_account(&local.folder(), "127.0.0.1", 2525, account::AuthKind::Password);
        let mut fake = take_all();
        let status = send_mail_with(&local.folder(), ACCOUNT, &plain, &mail(), OCT_1, &mut fake);
        assert!(matches!(status, SendStatus::Sent { .. }), "{status:?}");
        assert_eq!(fake.submits[0].0.security, SubmitSecurity::Plain);
    }

    // ---- the real client against a sink on this computer ----

    #[test]
    fn send_mail_delivers_through_a_local_smtp_server_and_files_the_mail_in_sent() {
        let dir = TempDir::new("send");
        let (port, rx) = crate::testutil::spawn_smtp_sink(crate::testutil::SinkScript {
            ehlo: vec![String::from("8BITMIME")],
            ..crate::testutil::SinkScript::default()
        });
        let settings = SendSettings {
            route: SendRoute::Smtp {
                host: "127.0.0.1".to_string(),
                port,
            },
            tls: TlsPolicy::Off,
            ..SendSettings::default()
        };
        let mut mail = mail();
        mail.bcc = vec!["hidden@example.com".to_string()];
        mail.text_body = "line one\n.dotted\nline three".to_string();
        let status = send_mail(&dir.folder(), ACCOUNT, &settings, &mail);
        assert!(matches!(status, SendStatus::Sent { .. }), "{status:?}");
        let session = rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("the sink saw a session");
        assert!(session
            .commands
            .iter()
            .any(|c| c == "MAIL FROM:<ada@example.org>"));
        assert!(session
            .commands
            .iter()
            .any(|c| c == "RCPT TO:<ben@example.net>"));
        assert!(session
            .commands
            .iter()
            .any(|c| c == "RCPT TO:<hidden@example.com>"));
        assert!(!session.message.contains("hidden@example.com"));
        assert!(
            session.message.contains("\r\n.dotted\r\n"),
            "{}",
            session.message
        );
        assert!(session.message.contains("MIME-Version: 1.0\r\n"));
        let index = sent_index(&dir.0);
        assert_eq!(index.len(), 1);
        let filed =
            std::fs::read(dir.0.join(ACCOUNT).join(&index[0].path)).unwrap();
        assert_eq!(String::from_utf8(filed).unwrap(), session.message);
    }
}
