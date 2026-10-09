//! The Azlin Bridge: other people's programs on this computer reach the user's Azlin drive
//! through protocols they already speak.
//!
//! Azlin's servers speak S3 and the token API only; every other protocol is a translation on
//! the user's own device, the way Proton Mail Bridge works. The bridge is a background process
//! that listens on 127.0.0.1 and serves:
//!
//! - **IMAP4rev1** (RFC 3501, with IDLE, UIDPLUS, MOVE, SPECIAL-USE, NAMESPACE, ID, UNSELECT,
//!   LITERAL+, SASL-IR) for Apple Mail, Outlook and Thunderbird: the drive's mailbox as AzMail
//!   keeps it (`examples/azul-mail/AZLIN_MAIL.md`).
//! - **SMTP submission** (RFC 6409): a mail program's message goes out through AzMail's own
//!   sending path (azul-mail-core's `send::send_prepared`: the outbox, a DKIM signature per
//!   attempt, direct delivery or the relay) and the copy that went out is filed in Sent.
//! - **WebDAV** class 1 and the minimal class 2 Finder and Windows Explorer need (LOCK /
//!   UNLOCK) over the drive's files.
//!
//! Where mail and files live is behind two small seams, so the servers do not change when the
//! storage does: [`store::MailStore`] (today [`store::DriveMailStore`], the plain `.eml` layout;
//! an encrypted store later) and azul-storage's `Drive` for WebDAV (today the bucket; the
//! encrypted drive later).
//!
//! Security: every listener is bound to 127.0.0.1 and a peer that is not this computer is
//! dropped ([`net`]); the one user signs in with the account's address and a per-install
//! random password compared in constant time, wrong attempts are counted per connection and
//! across the bridge ([`auth`]); lines, literals, messages, XML bodies, connections and the
//! command rate are limited ([`limits`]); WebDAV paths are normalised and `..`, absolute paths,
//! backslashes and NUL are refused, requests from browsers (an `Origin` header, a foreign
//! `Host`) are refused; there is no TLS on the loopback interface.
//!
//! UIDs ([`uids`]): a mailbox's UIDs are numbered in the order of its messages' names - the
//! drive's names start with the arrival stamp, so that is arrival order - the first time the
//! bridge sees the mailbox, and every name that appears later gets the next number. The map
//! (name to UID, UIDNEXT, UIDVALIDITY) is kept in the bridge's state folder, so a UID never
//! changes while the map lives; UIDVALIDITY is the time the map was made, so a lost map shows
//! the mail programs a new UIDVALIDITY (they fetch the mailbox again) instead of wrong UIDs.
//!
//! The drive ([`account`]): the bridge is a device of its own (azcloud-kit's account in its
//! state folder, signed up or joined with a code), with its own token family, credentials
//! renewed before they run out; requests go out through ureq with rustls ([`transport`]).
//! The password lives in the secret store ([`secrets`]: azcloud-kit's 0600 secrets file until
//! a process without libazul can reach the OS keyring). Sending ([`sender`]) is AzMail's own
//! path, through azul-mail-core. Running it: [`cli`].
//!
//! Modules: [`imap`], [`smtp`], [`dav`] (+ [`http`]) are the servers; [`store`], [`uids`],
//! [`mime`], [`sent`] serve mail; [`net`], [`auth`], [`limits`] keep the doors; [`config`],
//! [`dates`], [`memory`] (a drive in memory for tests and `serve --memory`).

pub mod account;
pub mod auth;
pub mod cli;
pub mod config;
pub mod dates;
pub mod dav;
pub mod http;
pub mod imap;
pub mod limits;
pub mod memory;
pub mod mime;
pub mod net;
pub mod secrets;
pub mod sender;
pub mod sent;
pub mod smtp;
pub mod store;
pub mod transport;
pub mod uids;
