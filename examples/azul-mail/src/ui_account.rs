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

use crate::{
    account::{self, Account, AccountForm, Secret},
    azlin, dkim,
    send::SendSettings,
    sending::SendingForm,
    ids, ui_backstage, with_app, IoJob, MailApp,
};

/// The wizard's steps.
pub(crate) const WIZARD_STEPS: [&str; 4] = ["Your account", "Incoming mail", "Sending", "Finish"];
/// Account Settings' categories (the same fields).
pub(crate) const SETTINGS_CATEGORIES: [&str; 4] = ["Account", "Incoming mail", "Sending", "Other programs"];
/// The account kinds of the wizard's first page.
pub(crate) const KINDS: [&str; 2] = ["IMAP server", "Azlin drive"];
/// What an Azlin account is, on the wizard's pages.
const AZLIN_NOTE: &str = "AzMail keeps your mail as files in your Azlin drive (one file per \
                          message, under mail/) and a copy on this computer. The drive token \
                          stays in the system keyring.";
/// How an Azlin account's mail leaves (Azlin itself never sends mail).
const AZLIN_SENDING_NOTE: &str = "Your Azlin drive stores your mail; it does not send it. AzMail \
                                  sends from this computer as chosen here, and the next \
                                  Send/Receive puts the copy from Sent Items into the drive.";

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
    editor.dkim_report = dkim::report_lines(report);
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
        String::from("The account settings are saved.")
    } else {
        format!("{} was added.", account.email)
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
                return Err(String::from("Enter your e-mail address."));
            }
            Ok(())
        }
        1 if editor.form.azlin => {
            editor.form.to_account().map_err(|e| e.to_string())?;
            let id = account::account_id(&editor.form.email).unwrap_or_default();
            let have_token = !editor.secret.is_empty()
                || editor.azlin_session.is_some()
                || editor.editing
                || s.secrets.contains_key(&id);
            if !have_token {
                return Err(String::from(
                    "Enter the drive token, or create a new drive.",
                ));
            }
            Ok(())
        }
        0 => {
            if !account::is_email(&editor.form.email) {
                return Err(String::from("Enter your e-mail address."));
            }
            let id = account::account_id(&editor.form.email).unwrap_or_default();
            let have_secret = !editor.secret.is_empty()
                || editor.editing
                || s.secrets.contains_key(&id)
                || crate::test_secret().is_some();
            if !have_secret {
                return Err(if editor.form.xoauth2 {
                    String::from("Paste your OAuth access token.")
                } else {
                    String::from("Enter your password or app password.")
                });
            }
            Ok(())
        }
        1 => editor.form.to_account().map(|_| ()).map_err(|e| e.to_string()),
        2 => editor.sending_settings().map(|_| ()),
        _ => Ok(()),
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
                let account = editor.form.to_account().map_err(|e| (1, e.to_string()))?;
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
            Dom::create_span_with_text("Saving the account and connecting...").with_css(NOTE),
        );
    }
    // Cancel always: the mail window is there with or without an account.
    WizardLayout::create("Add Account", strings(&WIZARD_STEPS))
        .with_page(page)
        .with_current_step(step)
        .with_labels("< Back", "Next >", "Finish", "Cancel")
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
        .with_section(ShellSettingsSection::create("Account", account_fields(editor, app)))
        .with_section(ShellSettingsSection::create(
            "Incoming mail",
            server_fields(s, editor, app),
        ))
        .with_section(ShellSettingsSection::create("Sending", sending_fields(editor, app)))
        .with_section(ShellSettingsSection::create(
            "Other programs",
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
        Button::with_type("Save", ButtonType::Primary)
            .with_on_click(app.clone(), on_settings_save as ButtonOnClickCallbackType)
            .dom(),
    );
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(layout)
        .with_child(footer)
}

fn strings(items: &[&str]) -> Vec<AzString> {
    items.iter().map(|s| AzString::from(*s)).collect()
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
    Dom::create_span_with_text(text).with_css(LABEL)
}

fn input(app: &RefAny, kind: TextInput, field: Field, value: &str, placeholder: &str, id: AzString) -> Dom {
    let mut input = kind
        .with_text(value)
        .with_placeholder(placeholder)
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
            Dom::create_span_with_text(text)
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
        "OAuth access token (XOAUTH2):"
    } else {
        "Password:"
    };
    let secret_placeholder = if editor.editing {
        "Leave empty to keep the saved one"
    } else {
        ""
    };
    let mut page = Dom::create_div().with_css("display: flex; flex-direction: column;");
    if !editor.editing {
        // The kind first: an IMAP server's mail, or the user's own Azlin drive.
        page.add_child(label("Account type:"));
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
        "AzMail signs in over IMAP and keeps a copy of every folder on this computer. The \
         password stays in the system keyring."
    };
    let page = page
        .with_child(Dom::create_span_with_text(intro).with_css(NOTE))
        .with_child(label("Your Name:"))
        .with_child(input(
            app,
            TextInput::create(),
            Field::Name,
            &f.name,
            "Example: Ada Lovelace",
            ids::ACCT_NAME,
        ))
        .with_child(label("E-mail Address:"))
        .with_child(input(
            app,
            TextInput::create_email(),
            Field::Email,
            &f.email,
            "Example: ada@example.org",
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
        .with_child(Dom::create_span_with_text(account::APP_PASSWORD_NOTE).with_css(NOTE));
    if !editor.drawn.note.is_empty() {
        page.add_child(Dom::create_span_with_text(editor.drawn.note.as_str()).with_css(NOTE));
    }
    page.with_child(check(
        app,
        f.xoauth2,
        Flag::Xoauth2,
        "Sign in with an OAuth access token (XOAUTH2) instead of a password",
    ))
}

/// Step 2 of an Azlin account: the token server, the drive and its token - or a new drive.
fn azlin_fields(s: &MailApp, editor: &AccountEditor, app: &RefAny) -> Dom {
    let f = &editor.form;
    let default_folder = account::account_id(&f.email)
        .map(|id| account::account_dir(&s.root, &id).path().display().to_string())
        .unwrap_or_default();
    let token_placeholder: &str = if f.token_default.is_empty() {
        "https://... (your Azlin provider's token server)"
    } else {
        &f.token_default
    };
    let secret_placeholder = if editor.editing || editor.azlin_session.is_some() {
        "Leave empty to keep the one AzMail has"
    } else {
        "dt_..."
    };
    let mut page = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(label("Azlin token server:"))
        .with_child(input(
            app,
            TextInput::create(),
            Field::TokenUrl,
            &f.token_url,
            token_placeholder,
            ids::ACCT_TOKEN_URL,
        ))
        .with_child(label("Drive id:"))
        .with_child(input(
            app,
            TextInput::create(),
            Field::DriveId,
            &f.drive_id,
            "d_...",
            ids::ACCT_DRIVE_ID,
        ))
        .with_child(label("Drive token:"))
        .with_child(input(
            app,
            TextInput::create_password(),
            Field::Secret,
            editor.secret.expose(),
            secret_placeholder,
            ids::ACCT_DRIVE_TOKEN,
        ))
        .with_child(
            Dom::create_span_with_text(
                "A drive token for this computer, from your Azlin provider or AzDrive's devices. \
                 Every sign-in replaces it with a new one: give AzMail a token of its own, not \
                 one AzDrive uses.",
            )
            .with_css(NOTE),
        );
    if !editor.editing {
        page.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; margin-top: 12px;")
                .with_child(
                    Button::create("Create a new drive")
                        .with_on_click(app.clone(), on_create_drive as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::AZLIN_CREATE_DRIVE),
                ),
        );
        let note = if editor.azlin_busy {
            String::from("Asking the token server for a new drive...")
        } else if editor.azlin_session.is_some() {
            new_drive_note(
                f.drive_id.trim(),
                editor.azlin_recovery.as_ref().map(Secret::expose),
            )
        } else {
            String::from(
                "A new, empty drive at this token server (a development token server's: a real \
                 one comes from your Azlin provider).",
            )
        };
        page.add_child(Dom::create_span_with_text(note).with_css(NOTE));
    }
    page.with_child(label("Local mail folder:")).with_child(input(
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
        .with_child(label("Incoming mail server (IMAP) and port:"))
        .with_child(pair(
            input(app, TextInput::create(), Field::ImapHost, &f.imap_host, &d.imap_host, ids::ACCT_IMAP_HOST),
            input(app, TextInput::create(), Field::ImapPort, &f.imap_port, &d.imap_port, ids::ACCT_IMAP_PORT),
        ))
        .with_child(label("User Name:"))
        .with_child(input(
            app,
            TextInput::create(),
            Field::Username,
            &f.username,
            &d.username,
            ids::ACCT_USERNAME,
        ))
        .with_child(label("Local mail folder:"))
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
            "Unencrypted connection (only for a test server on this computer)",
        ))
}

/// Step 3: how mail leaves.
fn sending_fields(editor: &AccountEditor, app: &RefAny) -> Dom {
    let sending = &editor.sending;
    let mut page = Dom::create_div().with_css("display: flex; flex-direction: column;");
    if editor.form.azlin {
        page.add_child(Dom::create_span_with_text(AZLIN_SENDING_NOTE).with_css(NOTE));
    }
    let mut page = page
        .with_child(label("Send mail:"))
        .with_child(
            Segmented::create(strings(&crate::sending::ROUTE_CHOICES))
                .with_selected_index(sending.route_index())
                .with_on_change(app.clone(), on_route as SegmentedOnChangeCallbackType)
                .dom(),
        );
    if sending.smtp {
        page.add_child(label("Outgoing mail server (SMTP) and port:"));
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
        let protection = if port == account::SMTPS_PORT {
            "encrypted from the first byte"
        } else {
            "encrypted with STARTTLS before the sign-in"
        };
        let text = format!(
            "AzMail signs in to {host} port {port} (the outgoing server on the Servers page) \
             with this account's password or token, {protection}, and hands every mail to it. \
             Gmail, iCloud and Fastmail want an app password. For a connection that cannot \
             deliver directly; DKIM below still signs as your own domain."
        );
        page.add_child(Dom::create_span_with_text(text.as_str()).with_css(NOTE));
    } else {
        page.add_child(
            Dom::create_span_with_text(
                "AzMail hands each mail to the receivers' own mail servers. Some providers take \
                 mail only from a trusted server; AzMail remembers those and keeps such mail in \
                 the Outbox.",
            )
            .with_css(NOTE),
        );
    }
    page.with_child(check(
        app,
        sending.starttls,
        Flag::StartTls,
        "Use STARTTLS when the server offers it",
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
            "Sign my mail with DKIM (needs a domain of your own whose DNS you can edit)",
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
    page.add_child(label("Domain and selector:"));
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
        "Create a key"
    } else {
        "Create a new key"
    };
    let check_dns = if public_key.is_empty() {
        None
    } else {
        Some(
            Button::create("Check DNS")
                .with_on_click(app.clone(), on_dkim_check as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::DKIM_CHECK)
                .with_css("margin-left: 8px;"),
        )
    };
    let mut buttons = Dom::create_div()
        .with_css("display: flex; flex-direction: row; margin-top: 12px;")
        .with_child(
            Button::create(create_label)
                .with_on_click(app.clone(), on_dkim_create as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::DKIM_CREATE),
        );
    if let Some(check_dns) = check_dns {
        buttons.add_child(check_dns);
    }
    page.add_child(buttons);
    if editor.dkim_busy {
        page.add_child(Dom::create_span_with_text("Working...").with_css(NOTE));
    }
    if public_key.is_empty() {
        page.add_child(
            Dom::create_span_with_text(
                "AzMail makes the key on this computer and keeps its private half in the system \
                 keyring; you publish the public half in your domain's DNS.",
            )
            .with_css(NOTE),
        );
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
    page.add_child(label("Publish this TXT record in your domain's DNS:"));
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
    page.add_child(label("As a line of a zone file:"));
    page.add_child(
        Dom::create_span_with_text(dkim::zone_line(&selector, &domain, &public_key))
            .with_css(record_css),
    );
    if editor.dkim_new_key.is_some() {
        page.add_child(
            Dom::create_span_with_text(
                "A new key: Save puts it into the system keyring. Until its record is \
                 published, receivers cannot check the signature.",
            )
            .with_css(NOTE),
        );
    }
    for line in &editor.dkim_report {
        page.add_child(Dom::create_span_with_text(line.as_str()).with_css(NOTE));
    }
    for note in dkim::setup_notes(&domain, editor.form.email.trim()) {
        page.add_child(Dom::create_span_with_text(note).with_css(NOTE));
    }
    page
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
        format!("the Azlin drive {} at {token}", f.drive_id.trim())
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
            Dom::create_span_with_text("Finish adds the account and receives its mail.")
                .with_css("font-size: 14px;"),
        )
        .with_child(line(format!("Account: {}", f.email.trim())))
        .with_child(line(format!("Incoming: {server}")))
        .with_child(line(format!("Sending: {sending}")))
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
            editor.error = account::FormError::NoTokenServer.to_string();
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
        Some(code) => format!(
            "The new drive {drive_id} is ready and encrypted: Finish adds it as this account. Its \
             RECOVERY CODE, shown this once and stored nowhere - write it down and keep it apart \
             from this computer (Azlin cannot reset it): {code}"
        ),
        None => format!("The new drive {drive_id} is ready: Finish adds it as this account."),
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
        Err(e) => editor.error = format!("No drive was made: {e}"),
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
    use super::new_drive_note;

    /// "We always encrypt": a drive AzMail made is encrypted as it was made, and its page shows
    /// the recovery code (shown once: it is stored nowhere).
    #[test]
    fn a_new_drives_note_shows_its_recovery_code_once() {
        let code = "0123A-4567B-89CDE-FGHJK-MNPQRS";
        let note = new_drive_note("d_1", Some(code));
        assert!(note.contains(code), "{note}");
        assert!(note.contains("encrypted"), "{note}");
        assert!(!new_drive_note("d_1", None).contains("RECOVERY"));
    }
}
