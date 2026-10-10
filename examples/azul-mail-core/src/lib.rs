//! azul-mail-core: AzMail's mail logic without azul types.
//!
//! AzMail (examples/azul-mail) links libazul for its windows; everything here runs without it,
//! so a headless process - the Azlin Bridge (examples/azul-bridge), the command-line tools - uses
//! the same code instead of a copy. AzMail re-exports every module under the name it always had
//! (`azmail::send`, `crate::folders` in its own modules), so nothing there changed but where the
//! files are.
//!
//! - [`account`]: an account's files (`account.json`), the data folder, the keyring's entry names.
//! - [`args`]: AzMail's command line and its facts for azul-appkit.
//! - [`auth`]: which sign-in method an account's kind of secret and a server's offer give.
//! - [`azlin`]: the Azlin drive's mailbox (`AZLIN_MAIL.md`): names, state markers, the session,
//!   the drive's folders.
//! - [`dkim`]: the DKIM key pair and its DNS record, the DNS checks.
//! - [`folders`]: which folder a mailbox is and what it is for (inbox, sent, spam, ...).
//! - [`message`]: MIME parsing for the index lines and the reading pane's model.
//! - [`mutf7`]: IMAP's modified UTF-7 mailbox names.
//! - [`send`]: sending: the outbox, the route, DKIM, the per-domain policy, the Sent folder.
//! - [`store`]: the mail store's files (`mail/<folder>/<yyyy>/<mm>/<uid>.eml`, `index.jsonl`).
//! - [`submit`]: signed-in submission to an account's own outgoing server.
//!
//! Every call that touches the network or the disk blocks: call it from a worker thread (an azul
//! `Thread` in the app), never from a UI callback. Nothing here prints or `Debug`s a secret.

pub mod account;
pub mod args;
pub mod auth;
pub mod azlin;
pub mod dkim;
pub mod folders;
pub mod message;
pub mod mutf7;
pub mod send;
pub mod store;
pub mod submit;

/// Test helpers: this crate's tests, and AzMail's through the `testing` feature.
#[cfg(any(test, feature = "testing"))]
pub mod testutil;
