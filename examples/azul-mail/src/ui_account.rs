//! The account wizard (File > Add Account) and Account Settings, on one account editor.
//!
//! Outlook 2010's "Add New Account" is a wizard: who you are (name, address, password), the
//! incoming server, how mail leaves, and a last page that says what will happen. Account Settings
//! shows the same fields as the sections of a settings page (`ShellSettingsLayout`), with Save.
//! Both edit an [`AccountEditor`]; Finish / Save checks it, and the files are written on a thread
//! (`IoJob::SaveAccount`: `account.json` and SEND's `sending.json`), after which the secret goes
//! to the keyring and Send / Receive starts (`account_saved`).
//!
//! Field ids for scripts: `#acct-name`, `#acct-email`, `#acct-secret`, `#acct-imap-host`,
//! `#acct-imap-port`, `#acct-username`, `#acct-folder`, `#send-host`, `#send-port`.

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
    send::SendSettings,
    sending::SendingForm,
    ui_main, with_app, IoJob, MailApp,
};

/// The wizard's steps.
pub(crate) const WIZARD_STEPS: [&str; 4] = ["Your account", "Incoming mail", "Sending", "Finish"];
/// Account Settings' categories (the same fields).
pub(crate) const SETTINGS_CATEGORIES: [&str; 3] = ["Account", "Incoming mail", "Sending"];

const NOTE: &str = "font-size: 12px; margin-top: 4px; opacity: 0.75;";
const LABEL: &str = "font-size: 12px; margin-top: 12px; margin-bottom: 4px;";
const ERROR: &str = "font-size: 13px; color: #b3261e; margin-top: 12px;";
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
        }
    }
}

/// File > Add Account: the wizard, on an empty form (or `prefill`).
pub(crate) fn open_wizard(s: &mut MailApp, prefill: Option<AccountForm>) {
    s.editor = Some(AccountEditor::create(
        prefill.unwrap_or_default(),
        false,
        SendSettings::default(),
    ));
    s.backstage = Some(ui_main::PAGE_ADD_ACCOUNT);
}

/// File > Account Settings for the current account (the wizard when there is none).
pub(crate) fn open_settings(s: &mut MailApp) {
    match s.current_account().map(|a| a.id.clone()) {
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
    let mut editor = AccountEditor::create(AccountForm::from_account(&account), true, settings);
    editor.error = error;
    s.editor = Some(editor);
    s.backstage = Some(ui_main::PAGE_SETTINGS);
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
        .map(|e| e.secret.clone())
        .filter(|secret| !secret.is_empty());
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
        crate::remember_secret(s, info, &account.id, secret);
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
        2 => editor.sending.apply(&editor.settings).map(|_| ()),
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
                let settings = editor.sending.apply(&editor.settings).map_err(|e| (2, e))?;
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

/// File > Add Account: the wizard.
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
    let cancel = if s.accounts.is_empty() { "" } else { "Cancel" };
    WizardLayout::create("Add Account", strings(&WIZARD_STEPS))
        .with_page(page)
        .with_current_step(step)
        .with_labels("< Back", "Next >", "Finish", cancel)
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
                .with_css("flex-grow: 1; font-size: 13px; color: #b3261e;"),
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
}

/// A form check box.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flag {
    Plain,
    Xoauth2,
    StartTls,
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

fn input(app: &RefAny, kind: TextInput, field: Field, value: &str, placeholder: &str, id: &str) -> Dom {
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

/// Step 1: who you are.
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
    let mut page = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(
            Dom::create_span_with_text(
                "AzMail signs in over IMAP and keeps a copy of every folder on this computer. \
                 The password stays in the system keyring.",
            )
            .with_css(NOTE),
        )
        .with_child(label("Your Name:"))
        .with_child(input(
            app,
            TextInput::create(),
            Field::Name,
            &f.name,
            "Example: Ada Lovelace",
            "acct-name",
        ))
        .with_child(label("E-mail Address:"))
        .with_child(input(
            app,
            TextInput::create_email(),
            Field::Email,
            &f.email,
            "Example: ada@example.org",
            "acct-email",
        ))
        .with_child(label(secret_label))
        .with_child(input(
            app,
            TextInput::create_password(),
            Field::Secret,
            editor.secret.expose(),
            secret_placeholder,
            "acct-secret",
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

/// Step 2: the incoming server.
fn server_fields(s: &MailApp, editor: &AccountEditor, app: &RefAny) -> Dom {
    let f = &editor.form;
    let d = &editor.drawn;
    let default_folder = account::account_id(&f.email)
        .map(|id| account::account_dir(&s.root, &id).display().to_string())
        .unwrap_or_default();
    Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(label("Incoming mail server (IMAP) and port:"))
        .with_child(pair(
            input(app, TextInput::create(), Field::ImapHost, &f.imap_host, &d.imap_host, "acct-imap-host"),
            input(app, TextInput::create(), Field::ImapPort, &f.imap_port, &d.imap_port, "acct-imap-port"),
        ))
        .with_child(label("User Name:"))
        .with_child(input(
            app,
            TextInput::create(),
            Field::Username,
            &f.username,
            &d.username,
            "acct-username",
        ))
        .with_child(label("Local mail folder:"))
        .with_child(input(
            app,
            TextInput::create(),
            Field::Folder,
            &f.folder,
            &default_folder,
            "acct-folder",
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
    let mut page = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(label("Send mail:"))
        .with_child(
            Segmented::create(strings(&["Directly", "Through an SMTP server"]))
                .with_selected_index(usize::from(sending.smtp))
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
                "send-host",
            ),
            input(
                app,
                TextInput::create(),
                Field::SendPort,
                &sending.port,
                &crate::sending::SUBMISSION_PORT.to_string(),
                "send-port",
            ),
        ));
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
}

/// Step 4: what Finish does.
fn finish_summary(editor: &AccountEditor) -> Dom {
    let f = &editor.form;
    let d = &editor.drawn;
    let server = if f.imap_host.trim().is_empty() {
        d.imap_host.clone()
    } else {
        f.imap_host.trim().to_string()
    };
    let sending = editor
        .sending
        .apply(&editor.settings)
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

extern "C" fn on_route(mut data: RefAny, _info: CallbackInfo, state: SegmentedState) -> Update {
    with_app(&mut data, |s, _| {
        if let Some(editor) = s.editor.as_mut() {
            editor.sending.smtp = state.selected_index == 1;
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
                if !s.accounts.is_empty() {
                    s.editor = None;
                    s.backstage = None;
                }
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
