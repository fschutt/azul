//! Every DOM id and class AzMail names, defined ONCE, each with the app's prefix `__azmail_`
//! (the widgets' own names carry `__azul`). The scripts find the nodes by these names
//! (scripts/azmail_e2e.py, examples/azul-mail/scripts/sync_e2e.py); a mail's own classes are
//! renamed by the sanitizer to a per-message prefix that never starts with `__azmail_`
//! (`html.rs`), so a mail's style sheet cannot reach these.
//!
//! `AzString::from_const_str` borrows the static bytes: no allocation, no second copy.

use azul::str::String as AzString;

// ==== File > Add Account / Account Settings ====

/// The sender's name.
pub const ACCT_NAME: AzString = AzString::from_const_str("__azmail_acct_name");
/// The e-mail address.
pub const ACCT_EMAIL: AzString = AzString::from_const_str("__azmail_acct_email");
/// The password, app password or OAuth access token.
pub const ACCT_SECRET: AzString = AzString::from_const_str("__azmail_acct_secret");
/// The IMAP server.
pub const ACCT_IMAP_HOST: AzString = AzString::from_const_str("__azmail_acct_imap_host");
/// The IMAP port.
pub const ACCT_IMAP_PORT: AzString = AzString::from_const_str("__azmail_acct_imap_port");
/// The user name at the IMAP server.
pub const ACCT_USERNAME: AzString = AzString::from_const_str("__azmail_acct_username");
/// The local mail folder.
pub const ACCT_FOLDER: AzString = AzString::from_const_str("__azmail_acct_folder");
/// The SMTP server (sending through one).
pub const SEND_HOST: AzString = AzString::from_const_str("__azmail_send_host");
/// The SMTP port.
pub const SEND_PORT: AzString = AzString::from_const_str("__azmail_send_port");

// ==== A message window ====

pub const COMPOSE_TO: AzString = AzString::from_const_str("__azmail_compose_to");
pub const COMPOSE_CC: AzString = AzString::from_const_str("__azmail_compose_cc");
pub const COMPOSE_BCC: AzString = AzString::from_const_str("__azmail_compose_bcc");
pub const COMPOSE_SUBJECT: AzString = AzString::from_const_str("__azmail_compose_subject");
/// Insert Link's address field.
pub const COMPOSE_LINK: AzString = AzString::from_const_str("__azmail_compose_link");
/// The big Send button.
pub const COMPOSE_SEND: AzString = AzString::from_const_str("__azmail_compose_send");
/// The rich-text editor's host; its blocks are `__azmail_compose_body-<index>`.
pub const COMPOSE_BODY: AzString = AzString::from_const_str("__azmail_compose_body");

// ==== The reading pane ====

/// The class of the paper a mail's HTML part is read on (`html.rs`).
pub const PAPER: AzString = AzString::from_const_str("__azmail_paper");
/// The mail's own `<body>` inside the paper (`html.rs`): the mail's `body` rules and attributes
/// land here, and its 12 px padding is the margin around the mail.
pub const MAIL_BODY: AzString = AzString::from_const_str("__azmail_mail_body");
