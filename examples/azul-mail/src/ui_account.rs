//! The account wizard (File > Info > Add Account, or the empty message list's Add Account) and
//! Account Settings, on one account editor. Both are pages of the backstage under Info; the
//! wizard's Cancel goes back to where it was opened from (the mail window or File > Info) - with
//! or without an account, the window behind it is the real one.
//!
//! Outlook 2010's "Add New Account" is a wizard: who you are (name, address, password), the
//! incoming server, how mail leaves, and a last page that says what will happen. Account Settings
//! shows the same fields as the sections of a settings page (`ShellSettingsLayout`), with Save.
//! Both edit an [`AccountEditor`]; Finish / Save checks it, and the files are written on a thread
//! (`IoJob::SaveAccount`: `account.json` and SEND's `sending.json`), after which the secret goes
//! to the keyring and Send / Receive starts (`account_saved`).
//!
//! Field ids for scripts (`ids.rs`): `#__azmail_acct_name`, `_email`, `_secret`, `_imap_host`,
//! `_imap_port`, `_username`, `_folder`, `#__azmail_send_host`, `#__azmail_send_port`; an Azlin
//! account's `#__azmail_acct_kind` (the kind), `_token_url`, `_drive_id`, `_drive_token` and
//! `#__azmail_azlin_create_drive`.
//!
//! An Azlin account (the wizard's first page: "Azlin drive") has a token server, a drive id and
//! a drive token instead of the IMAP server and the password; "Create a new drive" makes one at
//! a development token server. Its sending is the same as an IMAP account's: from this computer.
//!
//! Account Settings' last category, "Other programs", shows the Azlin Bridge's settings to copy
//! into other mail, file and calendar programs ([`crate::ui_bridge`]).

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, SegmentedOnChangeCallbackType,
        ShellSettingsLayoutOnCategoryCallbackType, TextInputOnFocusLostCallbackType,
        TextInputOnTextInputCallbackType, WizardOnEventCallbackType,
    },
    prelude::*,
    shells::{ShellSettingsLayout, ShellSettingsSection},
    str::String as AzString,
    widgets::{
        ButtonType, CheckBoxState, OnTextInputReturn, Segmented, SegmentedState, TextInputState,
        TextInputValid, WizardEvent, WizardEventKind, WizardLayout,
    },
};

use azul_appkit::l10n::{self, t, t_args, Arg};

use crate::{
    account::{self, Account, AccountForm, Secret},
    azlin, dkim,
    send::SendSettings,
    sending::SendingForm,
    ids, ui_backstage, with_app, IoJob, MailApp,
};

// The pages' words are keys of the resources (appkit's label says them).

/// The wizard's steps.
pub(crate) const WIZARD_STEPS: [&str; 4] = [
    "azmail-acct-step-account",
    "azmail-acct-step-incoming",
    "azmail-acct-step-sending",
    "azmail-acct-step-finish",
];
/// Account Settings' categories (the same fields).
pub(crate) const SETTINGS_CATEGORIES: [&str; 4] = [
    "azmail-acct-category-account",
    "azmail-acct-step-incoming",
    "azmail-acct-step-sending",
    "azmail-acct-category-other",
];
/// The account kinds of the wizard's first page.
pub(crate) const KINDS: [&str; 2] = ["azmail-acct-kind-imap", "azmail-acct-kind-azlin"];
/// What an Azlin account is, on the wizard's pages.
const AZLIN_NOTE: &str = "azmail-acct-azlin-note";
/// How an Azlin account's mail leaves (Azlin itself never sends mail).
const AZLIN_SENDING_NOTE: &str = "azmail-acct-azlin-sending-note";

const NOTE: &str = "font-size: 12px; margin-top: 4px; opacity: 0.75; \
                    @theme(flora) { opacity: 1; color: system:secondary-text; }";
/// A field's label; under flora flora's label (`.fl-label`): capitals in the label ink.
const LABEL: &str = "font-size: 12px; margin-top: 12px; margin-bottom: 4px; \
                     @theme(flora) { font-size: 11px; font-weight: bold; text-transform: uppercase; \
                     letter-spacing: 0.1em; color: system:secondary-text; }";
/// An error line: Material red under flat; flora's clay stone by day, its glow at night.
const ERROR: &str = "font-size: 13px; color: #b3261e; margin-top: 12px; @theme(flora) { \
                     color: #7E4A42; @media (prefers-color-scheme: dark) { color: #B3837A; } }";
const PAGE: &str = "display: flex; flex-direction: column; max-width: 520px;";

/// An account being added or edited.
pub(crate) struct AccountEditor {
    pub(crate) form: AccountForm,
    pub(crate) secret: Secret,
    pub(crate) sending: SendingForm,
    /// The sending settings the form is applied over (the account's, or the defaults).
    pub(crate) settings: SendSettings,
    pub(crate) error: String,
    /// Editing an existing account (Account Settings): an empty secret keeps the saved one.
    pub(crate) editing: bool,
    /// The defaults the fields were drawn with (a new address redraws the placeholders).
    pub(crate) drawn: account::FormDefaults,
    /// The wizard's step, or the settings page's category.
    pub(crate) step: usize,
    /// Finish / Save was pressed and the files are being written.
    pub(crate) saving: bool,
    /// Where the wizard's Cancel goes: the backstage page it was opened from (File > Info), or
    /// `None` for the mail window.
    pub(crate) return_to: Option<usize>,
    /// A DKIM key made in this editor: its private half goes to the keyring when the account is
    /// saved, its public half into sending.json and the DNS record shown.
    pub(crate) dkim_new_key: Option<dkim::KeyPair>,
    /// A key is being made or DNS is being asked (on a thread).
    pub(crate) dkim_busy: bool,
    /// What the last "Check DNS" found, one line per record.
    pub(crate) dkim_report: Vec<String>,
    /// The session of a drive made with "Create a new drive" (its first credentials and drive
    /// token): the account's secret once it is saved.
    pub(crate) azlin_session: Option<Secret>,
    /// "Create a new drive" is asking the token server (on a thread).
    pub(crate) azlin_busy: bool,
    /// The recovery code of the drive "Create a new drive" made (encrypted as it was made):
    /// shown on the page this once. A secret.
    pub(crate) azlin_recovery: Option<Secret>,
    /// The Azlin Bridge's settings ("Other programs"), read when Account Settings opened.
    pub(crate) bridge: crate::ui_bridge::BridgeView,
}

impl AccountEditor {
    fn create(form: AccountForm, editing: bool, settings: SendSettings) -> AccountEditor {
        let drawn = form.defaults();
        AccountEditor {
            sending: SendingForm::from_settings(&settings),
            form,
            secret: Secret::new(String::new()),
            settings,
            error: String::new(),
            editing,
            drawn,
            step: 0,
            saving: false,
            return_to: None,
            dkim_new_key: None,
            dkim_busy: false,
            dkim_report: Vec::new(),
            azlin_session: None,
            azlin_busy: false,
            azlin_recovery: None,
            bridge: crate::ui_bridge::BridgeView::default(),
        }
    }

    /// The secret the account is saved with: the typed one (an IMAP account's password or
    /// token; an Azlin account's drive token, as a session the first Send/Receive refreshes), a
    /// new drive's session; `None` when nothing was typed (editing: the saved one stays).
    fn secret_to_save(&self, account: &Account) -> Option<Secret> {
        let typed = Some(self.secret.clone()).filter(|secret| !secret.is_empty());
        let Some(link) = &account.azlin else {
            return typed;
        };
        match typed {
            Some(token) => Some(Secret::new(
                azlin::AzlinSession::with_token(&link.drive_id, token.expose()).to_secret(),
            )),
            None => self.azlin_session.clone(),
        }
    }

    /// The public half of the key the account signs with: one made here, else the saved one.
    pub(crate) fn dkim_public_key(&self) -> String {
        match &self.dkim_new_key {
            Some(pair) => pair.public_key.clone(),
            None => self
                .settings
                .dkim
                .as_ref()
                .map(|d| d.public_key.clone())
                .unwrap_or_default(),
        }
    }

    /// The sending settings the form describes: the route and STARTTLS, then DKIM.
    fn sending_settings(&self) -> Result<SendSettings, String> {
        let applied = self.sending.apply(&self.settings)?;
        // Submission signs in to the outgoing server of the Servers page: never unencrypted
        // to another computer.
        if let Ok(account) = self.form.to_account() {
            crate::sending::check_submission(&applied, &account.smtp.host, account.smtp.port)?;
        }
        let new_key = self
            .dkim_new_key
            .as_ref()
            .map(|pair| pair.public_key.as_str())
            .unwrap_or_default();
        self.sending
            .apply_dkim(applied, &self.form.email, new_key, crate::now_unix())
    }
}

/// A DKIM key was made (on a thread): kept in the editor until the account is saved.
pub(crate) fn dkim_key_made(s: &mut MailApp, result: Result<dkim::KeyPair, String>) {
    let Some(editor) = s.editor.as_mut() else {
        return;
    };
    editor.dkim_busy = false;
    match result {
        Ok(pair) => {
            println!("AZMAIL_DKIM_KEY_MADE");
            editor.dkim_new_key = Some(pair);
            editor.sending.dkim = true;
            editor.dkim_report.clear();
            editor.error.clear();
        }
        Err(e) => editor.error = e,
    }
}

/// "Check DNS" is done (on a thread).
pub(crate) fn dkim_checked(s: &mut MailApp, report: &dkim::DnsReport) {
    let Some(editor) = s.editor.as_mut() else {
        return;
    };
    editor.dkim_busy = false;
    editor.dkim_report = dkim_report_lines(report);
    println!(
        "AZMAIL_DKIM_CHECKED {}",
        if report.dkim == dkim::Published::Matches {
            "published"
        } else {
            "not-published"
        }
    );
}

/// File > Info > Add Account: the wizard, on an empty form (or `prefill`). Its Cancel returns to
/// File > Info when it was opened in the backstage, else to the mail window.
pub(crate) fn open_wizard(s: &mut MailApp, prefill: Option<AccountForm>) {
    let mut form = prefill.unwrap_or_default();
    // An Azlin account's empty token server field stands for the one this run was told.
    form.token_default = s.endpoints.token_url.clone().unwrap_or_default();
    let mut editor = AccountEditor::create(form, false, SendSettings::default());
    editor.return_to = s.backstage.map(|_| ui_backstage::PAGE_INFO);
    s.editor = Some(editor);
    s.backstage = Some(ui_backstage::PAGE_ADD_ACCOUNT);
}

/// File > Info > Account Settings for the current account - the first one while Local Folders
/// are shown - (the wizard when there is none).
pub(crate) fn open_settings(s: &mut MailApp) {
    match s
        .current_account()
        .or(s.accounts.first())
        .map(|a| a.id.clone())
    {
        Some(id) => open_settings_with_error(s, &id, String::new()),
        None => open_wizard(s, None),
    }
}

/// Account Settings for account `account_id`, saying `error` (a refused sign-in).
pub(crate) fn open_settings_with_error(s: &mut MailApp, account_id: &str, error: String) {
    let Some(account) = s.accounts.iter().find(|a| a.id == account_id).cloned() else {
        return;
    };
    let settings = SendSettings::load(&s.root, &account.id);
    let mut form = AccountForm::from_account(&account);
    form.token_default = s.endpoints.token_url.clone().unwrap_or_default();
    let mut editor = AccountEditor::create(form, true, settings);
    editor.error = error;
    editor.bridge = crate::ui_bridge::BridgeView::load();
    editor.return_to = s.backstage.map(|_| ui_backstage::PAGE_INFO);
    s.editor = Some(editor);
    s.backstage = Some(ui_backstage::PAGE_SETTINGS);
}

/// The files are written: the account joins the list (or replaces itself), its typed secret
/// goes to the keyring, the backstage closes and Send / Receive starts.
pub(crate) fn account_saved(
    s: &mut MailApp,
    info: &mut CallbackInfo,
    app: RefAny,
    account: Account,
    editing: bool,
) {
    let typed = s
        .editor
        .as_ref()
        .and_then(|e| e.secret_to_save(&account));
    // A DKIM key made in the editor: into the keyring and memory, now that sending.json names
    // its public half.
    let new_dkim_key = s
        .editor
        .as_mut()
        .and_then(|e| e.dkim_new_key.take())
        .map(|pair| pair.private_pem);
    if let Some(key) = new_dkim_key {
        crate::remember_dkim_key(s, info, &account.id, key);
    }
    let index = match s.accounts.iter().position(|a| a.id == account.id) {
        Some(i) => {
            s.accounts[i] = account.clone();
            i
        }
        None => {
            s.accounts.push(account.clone());
            s.accounts.len() - 1
        }
    };
    if let Some(secret) = typed {
        crate::remember_secret(s, info, &account, secret);
    }
    s.editor = None;
    s.backstage = None;
    s.notice = if editing {
        t("azmail-acct-saved")
    } else {
        t_args("azmail-acct-added", &[("address", Arg::from(account.email.as_str()))])
    };
    s.show_account(index);
    crate::start_sync(s, info, app);
}

// ==== Checking ====

/// What is wrong with the editor's step `step` (the wizard's steps; Save checks them all).
fn check_step(s: &MailApp, editor: &AccountEditor, step: usize) -> Result<(), String> {
    match step {
        0 if editor.form.azlin => {
            if !account::is_email(&editor.form.email) {
                return Err(t("azmail-acct-enter-address"));
            }
            Ok(())
        }
        1 if editor.form.azlin => {
            editor.form.to_account().map_err(|e| form_error(&e))?;
            let id = account::account_id(&editor.form.email).unwrap_or_default();
            let have_token = !editor.secret.is_empty()
                || editor.azlin_session.is_some()
                || editor.editing
                || s.secrets.contains_key(&id);
            if !have_token {
                return Err(t("azmail-acct-enter-token"));
            }
            Ok(())
        }
        0 => {
            if !account::is_email(&editor.form.email) {
                return Err(t("azmail-acct-enter-address"));
            }
            let id = account::account_id(&editor.form.email).unwrap_or_default();
            let have_secret = !editor.secret.is_empty()
                || editor.editing
                || s.secrets.contains_key(&id)
                || crate::test_secret().is_some();
            if !have_secret {
                return Err(t(if editor.form.xoauth2 {
                    "azmail-acct-paste-token"
                } else {
                    "azmail-acct-enter-password"
                }));
            }
            Ok(())
        }
        1 => editor.form.to_account().map(|_| ()).map_err(|e| form_error(&e)),
        2 => editor.sending_settings().map(|_| ()),
        _ => Ok(()),
    }
}

/// What is wrong with the form, in the window's language (azul-mail-core's `FormError`).
fn form_error(e: &account::FormError) -> String {
    match e {
        account::FormError::BadEmail => t("azmail-form-bad-email"),
        account::FormError::NoImapHost => t("azmail-form-no-imap-host"),
        account::FormError::BadPort { field, value } => t_args(
            "azmail-form-bad-port",
            &[("field", Arg::from(*field)), ("value", Arg::from(value.as_str()))],
        ),
        account::FormError::PlainNotLocal(host) => {
            t_args("azmail-form-plain-not-local", &[("host", Arg::from(host.as_str()))])
        }
        account::FormError::NoTokenServer => t("azmail-form-no-token-server"),
        account::FormError::BadTokenServer(why) => why.clone(),
        account::FormError::NoDrive => t("azmail-form-no-drive"),
    }
}

/// Finish / Save: every step checked, then the files written on a thread.
fn save(s: &mut MailApp, info: &mut CallbackInfo, app: RefAny) {
    let checked = {
        let Some(editor) = s.editor.as_ref() else {
            return;
        };
        (0..3)
            .try_for_each(|step| check_step(s, editor, step).map_err(|e| (step, e)))
            .and_then(|()| {
                let account = editor.form.to_account().map_err(|e| (1, form_error(&e)))?;
                let settings = editor.sending_settings().map_err(|e| (2, e))?;
                Ok((account, settings, editor.editing))
            })
    };
    let Some(editor) = s.editor.as_mut() else {
        return;
    };
    match checked {
        Err((step, error)) => {
            editor.step = step;
            editor.error = error;
        }
        Ok((account, settings, editing)) => {
            editor.error.clear();
            editor.saving = true;
            crate::spawn_io(
                info,
                app,
                IoJob::SaveAccount {
                    root: s.root.clone(),
                    account,
                    settings,
                    editing,
                },
            );
        }
    }
}

// ==== The pages ====

/// File > Info > Add Account: the wizard.
pub(crate) fn wizard_page(s: &MailApp, app: &RefAny) -> Dom {
    let Some(editor) = s.editor.as_ref() else {
        return Dom::create_div();
    };
    let step = editor.step.min(WIZARD_STEPS.len() - 1);
    let mut page = Dom::create_div().with_css(PAGE);
    match step {
        0 => page.add_child(account_fields(editor, app)),
        1 => page.add_child(server_fields(s, editor, app)),
        2 => page.add_child(sending_fields(editor, app)),
        _ => page.add_child(finish_summary(editor)),
    }
    if !editor.error.is_empty() {
        page.add_child(Dom::create_span_with_text(editor.error.as_str()).with_css(ERROR));
    }
    if editor.saving {
        page.add_child(
            Dom::create_span_with_text(l10n::label("azmail-acct-saving")).with_css(NOTE),
        );
    }
    // Cancel always: the mail window is there with or without an account.
    WizardLayout::create(l10n::label("azmail-acct-add-title"), strings(&WIZARD_STEPS))
        .with_page(page)
        .with_current_step(step)
        .with_labels(
            l10n::label("azmail-acct-back"),
            l10n::label("azmail-acct-next"),
            l10n::label("azmail-acct-step-finish"),
            l10n::label("kit-button-cancel"),
        )
        .with_can_go_next(!editor.saving)
        .with_on_event(app.clone(), on_wizard_event as WizardOnEventCallbackType)
        .dom()
}

/// File > Account Settings: the same fields as sections, and Save.
pub(crate) fn settings_page(s: &MailApp, app: &RefAny) -> Dom {
    let Some(editor) = s.editor.as_ref() else {
        return Dom::create_div();
    };
    let layout = ShellSettingsLayout::create(strings(&SETTINGS_CATEGORIES))
        .with_section(ShellSettingsSection::create(
            l10n::label(SETTINGS_CATEGORIES[0]),
            account_fields(editor, app),
        ))
        .with_section(ShellSettingsSection::create(
            l10n::label(SETTINGS_CATEGORIES[1]),
            server_fields(s, editor, app),
        ))
        .with_section(ShellSettingsSection::create(
            l10n::label(SETTINGS_CATEGORIES[2]),
            sending_fields(editor, app),
        ))
        .with_section(ShellSettingsSection::create(
            l10n::label(SETTINGS_CATEGORIES[3]),
            crate::ui_bridge::section(&editor.bridge, app),
        ))
        .with_active_category(editor.step.min(SETTINGS_CATEGORIES.len() - 1))
        .with_on_category(
            app.clone(),
            on_settings_category as ShellSettingsLayoutOnCategoryCallbackType,
        )
        .dom();
    let mut footer = Dom::create_div().with_css(
        "display: flex; flex-direction: row; align-items: center; padding: 10px 16px; \
         flex-shrink: 0;",
    );
    if !editor.error.is_empty() {
        footer.add_child(
            Dom::create_span_with_text(editor.error.as_str())
                .with_css(
                    "flex-grow: 1; font-size: 13px; color: #b3261e; @theme(flora) { color: \
                     #7E4A42; @media (prefers-color-scheme: dark) { color: #B3837A; } }",
                ),
        );
    } else {
        footer.add_child(Dom::create_div().with_css("flex-grow: 1;"));
    }
    footer.add_child(
        Button::with_type(l10n::label("azmail-acct-save"), ButtonType::Primary)
            .with_on_click(app.clone(), on_settings_save as ButtonOnClickCallbackType)
            .dom(),
    );
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(layout)
        .with_child(footer)
}

/// Keys of the resources (or plain words) in the window's language.
fn strings(items: &[&str]) -> Vec<AzString> {
    items.iter().map(|s| l10n::label(s)).collect()
}

/// A form field.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    Email,
    Secret,
    Username,
    ImapHost,
    ImapPort,
    Folder,
    SendHost,
    SendPort,
    DkimDomain,
    DkimSelector,
    TokenUrl,
    DriveId,
}

/// A form check box.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flag {
    Plain,
    Xoauth2,
    StartTls,
    Dkim,
}

struct FieldRef {
    app: RefAny,
    field: Field,
}

struct FlagRef {
    app: RefAny,
    flag: Flag,
}

fn label(text: &str) -> Dom {
    Dom::create_span_with_text(l10n::label(text)).with_css(LABEL)
}

/// A note line: a key of the resources, or words as they are.
fn note(text: &str) -> Dom {
    Dom::create_span_with_text(l10n::label(text)).with_css(NOTE)
}

fn input(app: &RefAny, kind: TextInput, field: Field, value: &str, placeholder: &str, id: AzString) -> Dom {
    let mut input = kind
        .with_text(value)
        .with_placeholder(l10n::label(placeholder))
        .with_on_text_input(
            RefAny::new(FieldRef {
                app: app.clone(),
                field,
            }),
            on_field as TextInputOnTextInputCallbackType,
        );
    if field == Field::Email {
        input = input.with_on_focus_lost(
            RefAny::new(FieldRef {
                app: app.clone(),
                field,
            }),
            on_email_done as TextInputOnFocusLostCallbackType,
        );
    }
    input.dom().with_id(id)
}

fn check(app: &RefAny, checked: bool, flag: Flag, text: &str) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 12px;")
        .with_child(
            CheckBox::create(checked)
                .with_on_toggle(
                    RefAny::new(FlagRef {
                        app: app.clone(),
                        flag,
                    }),
                    on_flag as CheckBoxOnToggleCallbackType,
                )
                .dom(),
        )
        .with_child(
            Dom::create_span_with_text(l10n::label(text))
                .with_css("margin-left: 8px; cursor: pointer;")
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(FlagRef {
                        app: app.clone(),
                        flag,
                    }),
                    on_flag_label,
                ),
        )
}

fn pair(left: Dom, right: Dom) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row;")
        .with_child(Dom::create_div().with_css("flex-grow: 1;").with_child(left))
        .with_child(Dom::create_div().with_css("width: 90px; margin-left: 8px;").with_child(right))
}

/// Step 1: who you are - and, in the wizard, the kind of account.
fn account_fields(editor: &AccountEditor, app: &RefAny) -> Dom {
    let f = &editor.form;
    let secret_label = if f.xoauth2 {
        "azmail-acct-oauth-token"
    } else {
        "azmail-acct-password"
    };
    let secret_placeholder = if editor.editing {
        "azmail-acct-keep-saved"
    } else {
        ""
    };
    let mut page = Dom::create_div().with_css("display: flex; flex-direction: column;");
    if !editor.editing {
        // The kind first: an IMAP server's mail, or the user's own Azlin drive.
        page.add_child(label("azmail-acct-type"));
        page.add_child(
            Segmented::create(strings(&KINDS))
                .with_selected_index(usize::from(f.azlin))
                .with_on_change(app.clone(), on_kind as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::ACCT_KIND),
        );
    }
    let intro = if f.azlin {
        AZLIN_NOTE
    } else {
        "azmail-acct-imap-note"
    };
    let page = page
        .with_child(note(intro))
        .with_child(label("azmail-acct-your-name"))
        .with_child(input(
            app,
            TextInput::create(),
            Field::Name,
            &f.name,
            "azmail-acct-name-example",
            ids::ACCT_NAME,
        ))
        .with_child(label("azmail-acct-address"))
        .with_child(input(
            app,
            TextInput::create_email(),
            Field::Email,
            &f.email,
            "azmail-acct-address-example",
            ids::ACCT_EMAIL,
        ));
    if f.azlin {
        // The drive and its token are the next page's.
        return page;
    }
    let mut page = page
        .with_child(label(secret_label))
        .with_child(input(
            app,
            TextInput::create_password(),
            Field::Secret,
            editor.secret.expose(),
            secret_placeholder,
            ids::ACCT_SECRET,
        ))
        .with_child(note("azmail-acct-app-password-note"));
    if !editor.drawn.note.is_empty() {
        // The provider's note in the window's language (azul-mail-core's English without it).
        let shown = match account::provider_for(&f.email) {
            Some(provider) => l10n::app_word(
                "AzMail",
                &format!("provider-{}", provider.name.to_ascii_lowercase()),
                &editor.drawn.note,
            ),
            None => editor.drawn.note.clone(),
        };
        page.add_child(note(&shown));
    }
    page.with_child(check(
        app,
        f.xoauth2,
        Flag::Xoauth2,
        "azmail-acct-use-oauth",
    ))
}

/// Step 2 of an Azlin account: the token server, the drive and its token - or a new drive.
fn azlin_fields(s: &MailApp, editor: &AccountEditor, app: &RefAny) -> Dom {
    let f = &editor.form;
    let default_folder = account::account_id(&f.email)
        .map(|id| account::account_dir(&s.root, &id).path().display().to_string())
        .unwrap_or_default();
    let token_placeholder: &str = if f.token_default.is_empty() {
        "azmail-acct-token-server-example"
    } else {
        &f.token_default
    };
    let secret_placeholder = if editor.editing || editor.azlin_session.is_some() {
        "azmail-acct-keep-token"
    } else {
        "dt_..."
    };
    let mut page = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(label("azmail-acct-token-server"))
        .with_child(input(
            app,
            TextInput::create(),
            Field::TokenUrl,
            &f.token_url,
            token_placeholder,
            ids::ACCT_TOKEN_URL,
        ))
        .with_child(label("azmail-acct-drive-id"))
        .with_child(input(
            app,
            TextInput::create(),
            Field::DriveId,
            &f.drive_id,
            "d_...",
            ids::ACCT_DRIVE_ID,
        ))
        .with_child(label("azmail-acct-drive-token"))
        .with_child(input(
            app,
            TextInput::create_password(),
            Field::Secret,
            editor.secret.expose(),
            secret_placeholder,
            ids::ACCT_DRIVE_TOKEN,
        ))
        .with_child(note("azmail-acct-drive-token-note"));
    if !editor.editing {
        page.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; margin-top: 12px;")
                .with_child(
                    Button::create(l10n::label("azmail-acct-create-drive"))
                        .with_on_click(app.clone(), on_create_drive as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::AZLIN_CREATE_DRIVE),
                ),
        );
        let text = if editor.azlin_busy {
            t("azmail-acct-asking-drive")
        } else if editor.azlin_session.is_some() {
            new_drive_note(
                f.drive_id.trim(),
                editor.azlin_recovery.as_ref().map(Secret::expose),
            )
        } else {
            t("azmail-acct-new-drive-what")
        };
        page.add_child(note(&text));
    }
    page.with_child(label("azmail-acct-local-folder")).with_child(input(
        app,
        TextInput::create(),
        Field::Folder,
        &f.folder,
        &default_folder,
        ids::ACCT_FOLDER,
    ))
}

/// Step 2: the incoming server (an Azlin account's drive: [`azlin_fields`]).
fn server_fields(s: &MailApp, editor: &AccountEditor, app: &RefAny) -> Dom {
    if editor.form.azlin {
        return azlin_fields(s, editor, app);
    }
    let f = &editor.form;
    let d = &editor.drawn;
    let default_folder = account::account_id(&f.email)
        .map(|id| account::account_dir(&s.root, &id).path().display().to_string())
        .unwrap_or_default();
    Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(label("azmail-acct-imap-server"))
        .with_child(pair(
            input(app, TextInput::create(), Field::ImapHost, &f.imap_host, &d.imap_host, ids::ACCT_IMAP_HOST),
            input(app, TextInput::create(), Field::ImapPort, &f.imap_port, &d.imap_port, ids::ACCT_IMAP_PORT),
        ))
        .with_child(label("azmail-acct-user-name"))
        .with_child(input(
            app,
            TextInput::create(),
            Field::Username,
            &f.username,
            &d.username,
            ids::ACCT_USERNAME,
        ))
        .with_child(label("azmail-acct-local-folder"))
        .with_child(input(
            app,
            TextInput::create(),
            Field::Folder,
            &f.folder,
            &default_folder,
            ids::ACCT_FOLDER,
        ))
        .with_child(check(
            app,
            f.plain,
            Flag::Plain,
            "azmail-acct-unencrypted",
        ))
}

/// Step 3: how mail leaves.
fn sending_fields(editor: &AccountEditor, app: &RefAny) -> Dom {
    let sending = &editor.sending;
    let mut page = Dom::create_div().with_css("display: flex; flex-direction: column;");
    if editor.form.azlin {
        page.add_child(note(AZLIN_SENDING_NOTE));
    }
    let mut page = page
        .with_child(label("azmail-acct-send-mail"))
        .with_child(
            Segmented::create(strings(&crate::sending::ROUTE_CHOICES))
                .with_selected_index(sending.route_index())
                .with_on_change(app.clone(), on_route as SegmentedOnChangeCallbackType)
                .dom(),
        );
    if sending.smtp {
        page.add_child(label("azmail-acct-smtp-server"));
        page.add_child(pair(
            input(
                app,
                TextInput::create(),
                Field::SendHost,
                &sending.host,
                &editor.drawn.smtp_host,
                ids::SEND_HOST,
            ),
            input(
                app,
                TextInput::create(),
                Field::SendPort,
                &sending.port,
                &crate::sending::SUBMISSION_PORT.to_string(),
                ids::SEND_PORT,
            ),
        ));
    } else if sending.submission {
        // The account's own outgoing server (the Servers page), signed in.
        let (host, port) = match editor.form.to_account() {
            Ok(account) => (account.smtp.host, account.smtp.port),
            Err(_) => (
                editor.drawn.smtp_host.clone(),
                editor.drawn.smtp_port.parse().unwrap_or(account::SMTPS_PORT),
            ),
        };
        let protection = t(if port == account::SMTPS_PORT {
            "azmail-acct-tls-implicit"
        } else {
            "azmail-acct-tls-starttls"
        });
        let text = t_args(
            "azmail-acct-submission-note",
            &[
                ("host", Arg::from(host.as_str())),
                ("port", Arg::from(u32::from(port))),
                ("protection", Arg::from(protection)),
            ],
        );
        page.add_child(note(&text));
    } else {
        page.add_child(note("azmail-acct-direct-note"));
    }
    page.with_child(check(
        app,
        sending.starttls,
        Flag::StartTls,
        "azmail-acct-starttls",
    ))
    .with_child(dkim_fields(editor, app))
}

/// Client-side DKIM: on or off, the domain and selector, the key, the DNS record to publish,
/// what DNS has now, and the notes on DMARC, SPF, reverse DNS and port 25.
fn dkim_fields(editor: &AccountEditor, app: &RefAny) -> Dom {
    let sending = &editor.sending;
    let mut page = Dom::create_div()
        .with_css("display: flex; flex-direction: column; margin-top: 16px;")
        .with_child(check(
            app,
            sending.dkim,
            Flag::Dkim,
            "azmail-acct-dkim",
        ));
    if !sending.dkim {
        return page;
    }
    let address_domain = account::email_domain(&editor.form.email).unwrap_or_default();
    let saved_selector = editor
        .settings
        .dkim
        .as_ref()
        .map(|d| d.selector.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| dkim::default_selector(crate::now_unix()));
    page.add_child(label("azmail-acct-domain-selector"));
    page.add_child(pair(
        input(
            app,
            TextInput::create(),
            Field::DkimDomain,
            &sending.dkim_domain,
            &address_domain,
            ids::DKIM_DOMAIN,
        ),
        input(
            app,
            TextInput::create(),
            Field::DkimSelector,
            &sending.dkim_selector,
            &saved_selector,
            ids::DKIM_SELECTOR,
        ),
    ));
    let public_key = editor.dkim_public_key();
    let create_label = if public_key.is_empty() {
        "azmail-acct-create-key"
    } else {
        "azmail-acct-create-new-key"
    };
    let check_dns = if public_key.is_empty() {
        None
    } else {
        Some(
            Button::create(l10n::label("azmail-acct-check-dns"))
                .with_on_click(app.clone(), on_dkim_check as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::DKIM_CHECK)
                .with_css("margin-left: 8px;"),
        )
    };
    let mut buttons = Dom::create_div()
        .with_css("display: flex; flex-direction: row; margin-top: 12px;")
        .with_child(
            Button::create(l10n::label(create_label))
                .with_on_click(app.clone(), on_dkim_create as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::DKIM_CREATE),
        );
    if let Some(check_dns) = check_dns {
        buttons.add_child(check_dns);
    }
    page.add_child(buttons);
    if editor.dkim_busy {
        page.add_child(note("azmail-acct-working"));
    }
    if public_key.is_empty() {
        page.add_child(note("azmail-acct-dkim-what"));
        return page;
    }
    let domain = match sending.dkim_domain.trim() {
        "" => address_domain.clone(),
        typed => typed.to_string(),
    };
    let selector = match sending.dkim_selector.trim() {
        "" => saved_selector,
        typed => typed.to_string(),
    };
    let record_css = "font-family: monospace; font-size: 12px; overflow-wrap: anywhere; \
                      margin-top: 4px;";
    page.add_child(label("azmail-acct-publish-txt"));
    page.add_child(
        Dom::create_span_with_text(dkim::record_name(&selector, &domain))
            .with_css(record_css)
            .with_id(ids::DKIM_NAME),
    );
    page.add_child(
        Dom::create_span_with_text(dkim::record_value(&public_key))
            .with_css(record_css)
            .with_id(ids::DKIM_VALUE),
    );
    page.add_child(label("azmail-acct-zone-line"));
    page.add_child(
        Dom::create_span_with_text(dkim::zone_line(&selector, &domain, &public_key))
            .with_css(record_css),
    );
    if editor.dkim_new_key.is_some() {
        page.add_child(note("azmail-acct-new-key-note"));
    }
    for line in &editor.dkim_report {
        page.add_child(note(line));
    }
    for line in dkim_notes(&domain, editor.form.email.trim()) {
        page.add_child(note(&line));
    }
    page
}

/// What the Sending page says under the DKIM record: DMARC, SPF, reverse DNS and port 25, for
/// `domain` and the sender `address`. One paragraph per entry.
fn dkim_notes(domain: &str, address: &str) -> Vec<String> {
    let domain = dkim::domain_name(domain);
    let (dmarc_name, dmarc_value) = dkim::dmarc_record(&domain, address);
    vec![
        t_args(
            "azmail-dkim-note-dmarc",
            &[
                ("name", Arg::from(dmarc_name)),
                ("value", Arg::from(dmarc_value)),
                ("domain", Arg::from(domain.as_str())),
                ("address", Arg::from(address.trim())),
            ],
        ),
        t_args("azmail-dkim-note-spf", &[("domain", Arg::from(domain.as_str()))]),
        t("azmail-dkim-note-ptr"),
        t("azmail-dkim-note-port"),
    ]
}

/// "Check DNS"'s report as the Sending page shows it: one line for DKIM, DMARC and SPF each.
fn dkim_report_lines(report: &dkim::DnsReport) -> Vec<String> {
    let dkim = match &report.dkim {
        dkim::Published::Matches => t("azmail-dkim-published"),
        dkim::Published::Different(key) if key.is_empty() => t("azmail-dkim-revoked"),
        dkim::Published::Different(key) => {
            t_args("azmail-dkim-other-key", &[("key", Arg::from(key.as_str()))])
        }
        dkim::Published::Missing => t("azmail-dkim-missing"),
        dkim::Published::Unknown(why) => {
            t_args("azmail-dkim-unknown", &[("why", Arg::from(why.as_str()))])
        }
    };
    let dmarc = match &report.dmarc {
        Some(record) => t_args("azmail-dkim-dmarc", &[("record", Arg::from(record.as_str()))]),
        None => t("azmail-dkim-no-dmarc"),
    };
    let spf = match &report.spf {
        Some(record) if record.to_ascii_lowercase().contains("-all") => {
            t_args("azmail-dkim-spf-hard", &[("record", Arg::from(record.as_str()))])
        }
        Some(record) => t_args("azmail-dkim-spf", &[("record", Arg::from(record.as_str()))]),
        None => t("azmail-dkim-no-spf"),
    };
    vec![dkim, dmarc, spf]
}

/// "Create a key": a new RSA key on a thread ([`dkim_key_made`] takes it).
extern "C" fn on_dkim_create(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, |s, app| {
        let Some(editor) = s.editor.as_mut() else {
            return Update::DoNothing;
        };
        if editor.dkim_busy {
            return Update::DoNothing;
        }
        editor.dkim_busy = true;
        editor.error.clear();
        crate::spawn_io(&mut info, app, IoJob::DkimKey);
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// "Check DNS": the DKIM, DMARC and SPF records on a thread ([`dkim_checked`] shows them).
extern "C" fn on_dkim_check(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, |s, app| {
        let Some(editor) = s.editor.as_mut() else {
            return Update::DoNothing;
        };
        if editor.dkim_busy {
            return Update::DoNothing;
        }
        let settings = match editor.sending_settings() {
            Ok(settings) => settings,
            Err(e) => {
                editor.error = e;
                return Update::RefreshDom;
            }
        };
        let Some(dkim) = settings.dkim else {
            return Update::DoNothing;
        };
        editor.dkim_busy = true;
        editor.dkim_report.clear();
        crate::spawn_io(
            &mut info,
            app,
            IoJob::DkimCheck {
                selector: dkim.selector,
                domain: dkim.domain,
                public_key: dkim.public_key,
            },
        );
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// Step 4: what Finish does.
fn finish_summary(editor: &AccountEditor) -> Dom {
    let f = &editor.form;
    let d = &editor.drawn;
    let server = if f.azlin {
        let token = if f.token_url.trim().is_empty() {
            f.token_default.trim()
        } else {
            f.token_url.trim()
        };
        t_args(
            "azmail-acct-azlin-drive-at",
            &[("drive", Arg::from(f.drive_id.trim())), ("server", Arg::from(token))],
        )
    } else if f.imap_host.trim().is_empty() {
        d.imap_host.clone()
    } else {
        f.imap_host.trim().to_string()
    };
    let sending = editor
        .sending_settings()
        .map(|settings| crate::sending::describe(&settings))
        .unwrap_or_default();
    let line = |text: String| Dom::create_span_with_text(text).with_css("margin-top: 6px;");
    Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(
            Dom::create_span_with_text(l10n::label("azmail-acct-finish-what"))
                .with_css("font-size: 14px;"),
        )
        .with_child(line(t_args(
            "azmail-acct-summary-account",
            &[("address", Arg::from(f.email.trim()))],
        )))
        .with_child(line(t_args(
            "azmail-acct-summary-incoming",
            &[("server", Arg::from(server))],
        )))
        .with_child(line(t_args(
            "azmail-acct-summary-sending",
            &[("how", Arg::from(sending))],
        )))
}

// ==== Callbacks ====

extern "C" fn on_field(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let target = data
        .downcast_ref::<FieldRef>()
        .map(|r| (r.app.clone(), r.field));
    if let Some((mut app, field)) = target {
        let text = state.get_text().as_str().to_string();
        if let Some(mut s) = app.downcast_mut::<MailApp>() {
            if let Some(editor) = s.editor.as_mut() {
                let f = &mut editor.form;
                match field {
                    Field::Name => f.name = text,
                    Field::Email => f.email = text,
                    Field::Secret => editor.secret = Secret::new(text),
                    Field::Username => f.username = text,
                    Field::ImapHost => f.imap_host = text,
                    Field::ImapPort => f.imap_port = text,
                    Field::Folder => f.folder = text,
                    Field::SendHost => editor.sending.host = text,
                    Field::SendPort => editor.sending.port = text,
                    Field::DkimDomain => editor.sending.dkim_domain = text,
                    Field::DkimSelector => editor.sending.dkim_selector = text,
                    Field::TokenUrl => f.token_url = text,
                    Field::DriveId => f.drive_id = text,
                }
            }
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Leaving the address field redraws the page when the address changed what the empty fields
/// stand for (the provider's servers and note).
extern "C" fn on_email_done(mut data: RefAny, _info: CallbackInfo, _state: TextInputState) -> Update {
    let Some(mut app) = data.downcast_ref::<FieldRef>().map(|r| r.app.clone()) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    let Some(editor) = s.editor.as_mut() else {
        return Update::DoNothing;
    };
    let now = editor.form.defaults();
    if now == editor.drawn {
        return Update::DoNothing;
    }
    editor.drawn = now;
    Update::RefreshDom
}

fn set_flag(data: &mut RefAny, checked: Option<bool>) -> Update {
    let Some((mut app, flag)) = data
        .downcast_ref::<FlagRef>()
        .map(|r| (r.app.clone(), r.flag))
    else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    let Some(editor) = s.editor.as_mut() else {
        return Update::DoNothing;
    };
    let value = match flag {
        Flag::Plain => &mut editor.form.plain,
        Flag::Xoauth2 => &mut editor.form.xoauth2,
        Flag::StartTls => &mut editor.sending.starttls,
        Flag::Dkim => &mut editor.sending.dkim,
    };
    *value = checked.unwrap_or(!*value);
    editor.error.clear();
    Update::RefreshDom
}

extern "C" fn on_flag(mut data: RefAny, _info: CallbackInfo, state: CheckBoxState) -> Update {
    set_flag(&mut data, Some(state.checked))
}

extern "C" fn on_flag_label(mut data: RefAny, _info: CallbackInfo) -> Update {
    set_flag(&mut data, None)
}

/// The wizard's account type: an IMAP server (0) or an Azlin drive (1).
extern "C" fn on_kind(mut data: RefAny, _info: CallbackInfo, state: SegmentedState) -> Update {
    with_app(&mut data, |s, _| {
        if let Some(editor) = s.editor.as_mut() {
            editor.form.azlin = state.selected_index == 1;
            editor.error.clear();
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// "Create a new drive": a drive at the token server, on a thread ([`drive_created`] takes
/// it). The token server is the typed one, else the one this run was told.
extern "C" fn on_create_drive(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, |s, app| {
        let Some(editor) = s.editor.as_mut() else {
            return Update::DoNothing;
        };
        if editor.azlin_busy {
            return Update::DoNothing;
        }
        let typed = editor.form.token_url.trim();
        let url = if typed.is_empty() {
            editor.form.token_default.trim().to_string()
        } else {
            typed.to_string()
        };
        if url.is_empty() {
            editor.error = form_error(&account::FormError::NoTokenServer);
            return Update::RefreshDom;
        }
        if let Err(e) = azlin::check_token_url(&url) {
            editor.error = e.to_string();
            return Update::RefreshDom;
        }
        editor.azlin_busy = true;
        editor.error.clear();
        crate::spawn_io(
            &mut info,
            app,
            IoJob::CreateDrive {
                token_url: url,
                name: String::from("AzMail"),
            },
        );
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// "Create a new drive" is answered: the drive's id goes into its field, and its session (the
/// first credentials, the drive token) is the account's secret once Finish saves it.
/// What the account page says of a drive "Create a new drive" made: that Finish adds it, and
/// - "we always encrypt" - its recovery code, shown this once.
pub(crate) fn new_drive_note(drive_id: &str, recovery_code: Option<&str>) -> String {
    match recovery_code {
        Some(code) => t_args(
            "azmail-acct-new-drive-encrypted",
            &[("drive", Arg::from(drive_id)), ("code", Arg::from(code))],
        ),
        None => t_args("azmail-acct-new-drive", &[("drive", Arg::from(drive_id))]),
    }
}

pub(crate) fn drive_created(
    s: &mut MailApp,
    result: Result<(azlin::AzlinSession, Option<String>), String>,
) {
    let Some(editor) = s.editor.as_mut() else {
        return;
    };
    editor.azlin_busy = false;
    match result {
        Ok((session, recovery)) => {
            println!(
                "AZMAIL_AZLIN_DRIVE_CREATED {}{}",
                session.drive_id,
                if recovery.is_some() { " encrypted" } else { "" }
            );
            editor.form.drive_id = session.drive_id.clone();
            editor.secret = Secret::new(String::new());
            editor.azlin_session = Some(Secret::new(session.to_secret()));
            editor.azlin_recovery = recovery.map(Secret::new);
            editor.error.clear();
        }
        Err(e) => {
            editor.error = t_args("azmail-acct-no-drive", &[("why", Arg::from(l10n::t_label(&e)))]);
        }
    }
}

extern "C" fn on_route(mut data: RefAny, _info: CallbackInfo, state: SegmentedState) -> Update {
    with_app(&mut data, |s, _| {
        if let Some(editor) = s.editor.as_mut() {
            editor.sending.choose_route(state.selected_index);
            editor.error.clear();
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

extern "C" fn on_wizard_event(mut data: RefAny, mut info: CallbackInfo, event: WizardEvent) -> Update {
    with_app(&mut data, |s, app| {
        let Some(step) = s.editor.as_ref().map(|e| e.step) else {
            return Update::DoNothing;
        };
        match event.kind {
            WizardEventKind::Back => {
                if let Some(editor) = s.editor.as_mut() {
                    editor.step = step.saturating_sub(1);
                    editor.error.clear();
                }
            }
            WizardEventKind::Next => {
                let checked = s
                    .editor
                    .as_ref()
                    .map(|editor| check_step(s, editor, step))
                    .unwrap_or(Ok(()));
                if let Some(editor) = s.editor.as_mut() {
                    match checked {
                        Ok(()) => {
                            editor.step = (step + 1).min(WIZARD_STEPS.len() - 1);
                            editor.error.clear();
                        }
                        Err(error) => editor.error = error,
                    }
                }
            }
            WizardEventKind::Step => {
                if let Some(editor) = s.editor.as_mut() {
                    if event.step < step {
                        editor.step = event.step;
                        editor.error.clear();
                    }
                }
            }
            WizardEventKind::Finish => save(s, &mut info, app),
            WizardEventKind::Cancel => {
                if s.editor.as_ref().is_some_and(|e| e.saving) {
                    return Update::DoNothing;
                }
                s.backstage = s.editor.take().and_then(|e| e.return_to);
            }
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

extern "C" fn on_settings_category(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, |s, _| {
        if let Some(editor) = s.editor.as_mut() {
            editor.step = index;
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

extern "C" fn on_settings_save(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, |s, app| {
        save(s, &mut info, app);
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

#[cfg(test)]
mod tests {
    use super::{dkim_notes, dkim_report_lines, new_drive_note};
    use crate::dkim::{DnsReport, Published};

    #[test]
    fn the_notes_say_what_dmarc_spf_reverse_dns_and_port_25_need() {
        crate::l10n::in_english();
        let notes = dkim_notes("example.org", "ada@example.org").join("\n");
        assert!(notes.contains("_dmarc.example.org"), "{notes}");
        assert!(
            notes.contains("v=DMARC1; p=none; rua=mailto:ada@example.org"),
            "{notes}"
        );
        assert!(notes.contains("SPF"), "{notes}");
        assert!(notes.contains("~all"), "{notes}");
        assert!(notes.contains("PTR"), "{notes}");
        assert!(notes.contains("port 25"), "{notes}");
    }

    /// "We always encrypt": a drive AzMail made is encrypted as it was made, and its page shows
    /// the recovery code (shown once: it is stored nowhere).
    #[test]
    fn a_new_drives_note_shows_its_recovery_code_once() {
        crate::l10n::in_english();
        let code = "0123A-4567B-89CDE-FGHJK-MNPQRS";
        let note = new_drive_note("d_1", Some(code));
        assert!(note.contains(code), "{note}");
        assert!(note.contains("encrypted"), "{note}");
        assert!(!new_drive_note("d_1", None).contains("RECOVERY"));
    }

    #[test]
    fn the_dns_check_reads_as_one_line_per_record() {
        crate::l10n::in_english();
        let lines = dkim_report_lines(&DnsReport {
            dkim: Published::Matches,
            dmarc: Some(String::from("v=DMARC1; p=none")),
            spf: Some(String::from("v=spf1 mx -all")),
        });
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(lines[0].contains("published"), "{lines:?}");
        assert_eq!(lines[1], "DMARC: v=DMARC1; p=none");
        assert!(lines[2].starts_with("SPF: v=spf1 mx -all"), "{lines:?}");
        assert!(
            lines[2].contains("~all"),
            "a hard -all is pointed out: {lines:?}"
        );
        let missing = dkim_report_lines(&DnsReport {
            dkim: Published::Missing,
            dmarc: None,
            spf: None,
        });
        assert!(missing[0].contains("not found"), "{missing:?}");
        assert!(missing[1].contains("no record"), "{missing:?}");
        assert!(missing[2].contains("no record"), "{missing:?}");
        let other = dkim_report_lines(&DnsReport {
            dkim: Published::Different(String::from("MIIBother")),
            dmarc: None,
            spf: Some(String::from("v=spf1 ~all")),
        });
        assert!(other[0].contains("MIIBother"), "{other:?}");
        assert!(!other[2].contains("-all"), "{other:?}");
        let offline = dkim_report_lines(&DnsReport {
            dkim: Published::Unknown(String::from("timed out")),
            dmarc: None,
            spf: None,
        });
        assert!(offline[0].contains("timed out"), "{offline:?}");
    }
}
