//! Every DOM id and class AzMail names, defined ONCE, each with the app's prefix `__azmail_`
//! (the widgets' own names carry `__azul`). The scripts find the nodes by these names
//! (scripts/azmail_e2e.py, examples/azul-mail/scripts/sync_e2e.py); a mail's own classes are
//! renamed by the sanitizer to a per-message prefix that never starts with `__azmail_`
//! (`html.rs`), so a mail's style sheet cannot reach these.
//!
//! `AzString::from_const_str` borrows the static bytes: no allocation, no second copy.

use azul::str::String as AzString;

// ==== The main window ====

/// The ribbon (in the mail view; with File open, its tab row over the backstage).
pub const RIBBON: AzString = AzString::from_const_str("__azmail_ribbon");
/// The folder pane (Favorites, every account's folder tree, Mail / Calendar / Contacts / Tasks).
pub const FOLDER_PANE: AzString = AzString::from_const_str("__azmail_folder_pane");
/// The message list.
pub const MESSAGE_LIST: AzString = AzString::from_const_str("__azmail_message_list");
/// The list's empty state while there is no account ("No account yet", Add Account...).
pub const NO_ACCOUNT: AzString = AzString::from_const_str("__azmail_no_account");

// ==== File (the backstage) ====

/// The File tab: the ribbon's tab row over the backstage.
pub const BACKSTAGE: AzString = AzString::from_const_str("__azmail_backstage");
/// File > Info.
pub const PAGE_INFO: AzString = AzString::from_const_str("__azmail_page_info");
/// File > Print.
pub const PAGE_PRINT: AzString = AzString::from_const_str("__azmail_page_print");
/// File > Help.
pub const PAGE_HELP: AzString = AzString::from_const_str("__azmail_page_help");
/// Info's "Add Account" button.
pub const ADD_ACCOUNT: AzString = AzString::from_const_str("__azmail_add_account");
/// Info's "Account Settings" button.
pub const ACCOUNT_SETTINGS: AzString = AzString::from_const_str("__azmail_account_settings");
/// Info's "Send/Receive" button.
pub const INFO_SEND_RECEIVE: AzString = AzString::from_const_str("__azmail_info_send_receive");
/// Print's "Print" button (to a PDF file).
pub const PRINT: AzString = AzString::from_const_str("__azmail_print");
/// Print's preview of the PDF's first page.
pub const PRINT_PREVIEW: AzString = AzString::from_const_str("__azmail_print_preview");
/// Help's "Keyboard Shortcuts" button.
pub const HELP_SHORTCUTS: AzString = AzString::from_const_str("__azmail_help_shortcuts");
/// Help's "Options" button.
pub const HELP_OPTIONS: AzString = AzString::from_const_str("__azmail_help_options");
/// Help's "About AzMail" button.
pub const HELP_ABOUT: AzString = AzString::from_const_str("__azmail_help_about");

// ==== File > Info > Add Account / Account Settings ====

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
/// Client-side DKIM: the signing domain.
pub const DKIM_DOMAIN: AzString = AzString::from_const_str("__azmail_dkim_domain");
/// Client-side DKIM: the selector.
pub const DKIM_SELECTOR: AzString = AzString::from_const_str("__azmail_dkim_selector");
/// The "Create a key" button.
pub const DKIM_CREATE: AzString = AzString::from_const_str("__azmail_dkim_create");
/// The "Check DNS" button.
pub const DKIM_CHECK: AzString = AzString::from_const_str("__azmail_dkim_check");
/// The DNS record's name (`<selector>._domainkey.<domain>`).
pub const DKIM_NAME: AzString = AzString::from_const_str("__azmail_dkim_name");
/// The DNS record's value (`v=DKIM1; k=rsa; p=...`).
pub const DKIM_VALUE: AzString = AzString::from_const_str("__azmail_dkim_value");

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
