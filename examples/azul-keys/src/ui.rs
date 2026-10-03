//! AzKeys' window: the app-drawn title row (`NoTitle` + `Titlebar`), then the unlock screen,
//! the new-vault form, or the unlocked vault on azul's S4 `PimShell` (navigation: all items,
//! favourites, one-time codes, the kinds, the tags; the searchable list; the reading pane from
//! `ui_item.rs`), a toolbar in the ribbon row and a status bar with the lock and clipboard
//! countdowns; or azul-appkit's settings page with AzKeys' Security and Vault sections. An item
//! being edited is guarded against a close (`CloseGuard`).

use std::path::Path;

use zeroize::Zeroizing;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CloseGuardDirtyCheckCallbackType, CloseGuardOnEventCallbackType,
        DropDownOnChoiceChangeCallbackType, ShellNavigationPaneOnEventCallbackType,
        SwitchOnToggleCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType,
    },
    dom::{DomId, VirtualKeyCode},
    prelude::*,
    shells::{
        PimShell, ShellEmptyState, ShellNavigationGroup, ShellNavigationPane,
        ShellNavigationPaneEvent, ShellNavigationPaneEventKind, ShellThemeAccent, ShellThemeScope,
    },
    str::String as AzString,
    vec::StringVec,
    widgets::{
        Avatar, AvatarSize, ButtonType, CloseGuard, CloseGuardDocumentState, CloseGuardEvent,
        CloseGuardEventKind, DropDown, OnTextInputReturn, StatusBar, StatusBarSegment, Switch,
        SwitchState, TextInputState, TextInputValid, TreeViewNode,
    },
};
use azul_appkit::ui::{self as kit, AppSection};

use crate::app::{
    minutes_label, now, seconds_label, KeysApp, Screen, CLEAR_CHOICES, IDLE_CHOICES, SPEC,
};
use crate::generator::{estimate, percent, Strength};
use crate::ids;
use crate::jobs;
use crate::session::{Form, Reading, Session};
use crate::store::Work;
use crate::vault::{sections, Kind, Scope, Vault};

// ==== Small pieces ====

pub(crate) fn strs(items: &[&str]) -> StringVec {
    StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect())
}

pub(crate) fn text<S: Into<AzString>>(content: S) -> Dom {
    Dom::create_span_with_text(content)
}

pub(crate) fn block(css: &str, child: Dom) -> Dom {
    Dom::create_div().with_css(css).with_child(child)
}

pub(crate) fn column(css: &str, children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css(format!("display: flex; flex-direction: column; {css}"))
        .with_children(DomVec::from_vec(children))
}

pub(crate) fn row(css: &str, children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; {css}"
        ))
        .with_children(DomVec::from_vec(children))
}

pub(crate) fn button(
    label: &str,
    id: AzString,
    app: &RefAny,
    cb: ButtonOnClickCallbackType,
) -> Dom {
    Button::create(label)
        .with_on_click(app.clone(), cb)
        .dom()
        .with_id(id)
}

pub(crate) fn icon_button(
    label: &str,
    icon: &str,
    id: AzString,
    app: &RefAny,
    cb: ButtonOnClickCallbackType,
) -> Dom {
    Button::create(label)
        .with_icon(icon)
        .with_on_click(app.clone(), cb)
        .dom()
        .with_id(id)
}

pub(crate) fn primary(
    label: &str,
    id: AzString,
    app: &RefAny,
    cb: ButtonOnClickCallbackType,
) -> Dom {
    Button::create(label)
        .with_button_type(ButtonType::Primary)
        .with_on_click(app.clone(), cb)
        .dom()
        .with_id(id)
}

/// A line of secondary text.
pub(crate) fn note(content: &str) -> Dom {
    block(
        "font-size: 12px; opacity: 0.75; padding: 2px 0px;",
        text(content),
    )
}

/// A line that reports a problem (red in both modes).
pub(crate) fn problem(content: &str, id: AzString) -> Dom {
    block(
        "font-size: 12px; color: #d13438; padding: 4px 0px;",
        text(content),
    )
    .with_id(id)
}

/// A labelled form row.
pub(crate) fn form_row(label: &str, control: Dom) -> Dom {
    row(
        "padding: 4px 0px;",
        vec![
            block(
                "width: 110px; flex-shrink: 0; font-size: 13px;",
                text(label),
            ),
            block("flex-grow: 1; min-width: 0px;", control),
        ],
    )
}

/// The strength bar and its words for a password.
pub(crate) fn strength_bar(password: &str, id: AzString) -> Dom {
    let bits = estimate(password);
    let words = if password.is_empty() {
        String::new()
    } else {
        format!("{} (~{:.0} bits)", Strength::of_bits(bits).label(), bits)
    };
    row(
        "padding: 2px 0px;",
        vec![
            block(
                "width: 160px; flex-shrink: 0;",
                ProgressBar::create(percent(bits)).dom(),
            ),
            block("padding-left: 8px; font-size: 12px;", text(words)),
        ],
    )
    .with_id(id)
}

/// A text field's answer that keeps the field as typed (no rebuild).
pub(crate) fn keep() -> OnTextInputReturn {
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Runs `f` on the app's state; the window is rebuilt afterwards. Input counts as activity.
pub(crate) fn with_app(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut KeysApp, &mut CallbackInfo, &RefAny),
) -> Update {
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<KeysApp>() else {
        return Update::DoNothing;
    };
    guard.touch(now());
    f(&mut guard, info, &handle);
    Update::RefreshDom
}

/// The key the window's key callback is about.
fn key_of(info: &CallbackInfo) -> Option<VirtualKeyCode> {
    info.get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()
}

// ==== The window ====

/// The window's layout.
pub extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<KeysApp>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let content = if kit::settings_open(&s.kit) {
        column(
            "flex-grow: 1; min-height: 0px;",
            vec![
                kit::title_row(SPEC.name),
                kit::settings_page(&s.kit, settings_sections(s, &app)),
            ],
        )
    } else {
        match (s.screen, s.session.as_ref()) {
            (Screen::Vault, Some(session)) => vault_screen(s, session, &app),
            (Screen::Create, _) => framed(create_screen(s, &app)),
            _ => framed(unlock_screen(s, &app)),
        }
    };
    let root = column("flex-grow: 1; min-height: 0px;", vec![content]);
    // An edited item is not lost to the close button: the guard asks "save it?".
    let guarded = CloseGuard::create(root, "the item")
        .with_dirty_check(app.clone(), dirty_check as CloseGuardDirtyCheckCallbackType)
        .with_asking(s.asking_close)
        .with_on_event(
            app.clone(),
            on_close_answer as CloseGuardOnEventCallbackType,
        )
        .dom();
    ShellThemeScope::create(guarded)
        .with_accent(ShellThemeAccent::Slate)
        .body()
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app.clone(),
            on_key,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::MouseDown),
            app.clone(),
            jobs::on_activity,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::KeyringResult),
            app.clone(),
            jobs::on_keyring_result,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::BiometricResult),
            app.clone(),
            jobs::on_biometric_result,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::CloseRequested),
            app,
            jobs::on_close_requested,
        )
}

/// A screen without the shell: the title row over a centred card.
fn framed(card: Dom) -> Dom {
    column(
        "flex-grow: 1; min-height: 0px;",
        vec![
            kit::title_row(SPEC.name),
            column(
                "flex-grow: 1; align-items: center; justify-content: center; padding: 24px;",
                vec![block("width: 440px; max-width: 100%;", card)],
            ),
        ],
    )
}

// ==== The unlock screen ====

fn unlock_screen(s: &KeysApp, app: &RefAny) -> Dom {
    if !s.listed {
        return ShellEmptyState::create("Looking for your vaults")
            .with_icon("hourglass_empty")
            .with_detail(
                azul_appkit::data::local_path(&s.data_root, crate::store::VAULTS)
                    .display()
                    .to_string(),
            )
            .dom()
            .with_id(ids::UNLOCK_SCREEN);
    }
    let Some(chosen) = s.chosen_vault() else {
        return ShellEmptyState::create("No vault yet")
            .with_icon("lock")
            .with_detail("A vault keeps your passwords in one encrypted file in your data folder.")
            .with_action_label("Create a vault")
            .with_on_action(app.clone(), on_show_create as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::UNLOCK_SCREEN);
    };
    let t = now();
    let wait = s.attempts.wait(t);
    let mut children = vec![
        block(
            "font-size: 40px; text-align: center; padding-bottom: 4px;",
            text("\u{1f511}"),
        ),
        block(
            "font-size: 18px; font-weight: 600; text-align: center; padding-bottom: 12px;",
            text(format!("Unlock \u{201c}{}\u{201d}", chosen.name())),
        ),
    ];
    if s.vaults.len() > 1 {
        let names: Vec<&str> = s.vaults.iter().map(|v| v.name()).collect();
        children.push(form_row(
            "Vault",
            DropDown::create(strs(&names))
                .with_selected(s.unlock.chosen)
                .with_accessibility_name("Vault")
                .with_on_choice_change(
                    app.clone(),
                    on_choose_vault as DropDownOnChoiceChangeCallbackType,
                )
                .dom()
                .with_id(ids::UNLOCK_VAULT),
        ));
    }
    children.push(form_row(
        "Password",
        TextInput::create_password()
            .with_text(s.unlock.password.as_str())
            .with_placeholder("Master password")
            .with_accessibility_name("Master password")
            .with_on_text_input(
                app.clone(),
                on_unlock_text as TextInputOnTextInputCallbackType,
            )
            .with_on_virtual_key_down(
                app.clone(),
                on_unlock_key as TextInputOnVirtualKeyDownCallbackType,
            )
            .dom()
            .with_id(ids::UNLOCK_PASSWORD),
    ));
    let mut buttons = vec![];
    let mut unlock = Button::create(if wait > 0 {
        format!("Wait {wait} s")
    } else {
        "Unlock".to_string()
    })
    .with_button_type(ButtonType::Primary)
    .with_on_click(app.clone(), on_unlock as ButtonOnClickCallbackType);
    if wait > 0 || s.busy.is_some() {
        unlock = unlock.with_disabled("Wait for the answer");
    }
    buttons.push(unlock.dom().with_id(ids::UNLOCK_BUTTON));
    if let Some(mode) = s.device_unlock(chosen.id()) {
        let label = match mode {
            crate::app::DeviceUnlock::Keyring => "Unlock from the keyring",
            _ => "Use biometrics",
        };
        buttons.push(icon_button(
            label,
            "fingerprint",
            ids::UNLOCK_DEVICE,
            app,
            on_unlock_device,
        ));
    }
    children.push(row("gap: 8px; padding: 8px 0px 4px 110px;", buttons));
    let message = if wait > 0 {
        s.attempts.message(t)
    } else {
        s.unlock.message.clone()
    };
    if !message.is_empty() {
        children.push(problem(&message, ids::UNLOCK_MESSAGE));
    }
    if let Some(busy) = &s.busy {
        children.push(note(busy));
    }
    if jobs::setting(s, "sample_vault").as_deref() == Some(chosen.id()) {
        children.push(
            note(&format!(
                "This is the sample vault: its master password is \u{201c}{}\u{201d}.",
                crate::sample::SAMPLE_PASSWORD
            ))
            .with_id(ids::UNLOCK_HINT),
        );
    }
    children.push(row(
        "padding-top: 16px;",
        vec![icon_button(
            "New vault",
            "add",
            ids::UNLOCK_NEW,
            app,
            on_show_create,
        )],
    ));
    column("", children).with_id(ids::UNLOCK_SCREEN)
}

extern "C" fn on_choose_vault(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.unlock.chosen = index.min(s.vaults.len().saturating_sub(1));
        s.unlock.message.clear();
    })
}

extern "C" fn on_unlock_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let typed = Zeroizing::new(state.get_text().as_str().to_string());
    if let Some(mut s) = data.downcast_mut::<KeysApp>() {
        s.unlock.password = typed;
    }
    keep()
}

extern "C" fn on_unlock_key(
    data: RefAny,
    info: CallbackInfo,
    _state: TextInputState,
) -> OnTextInputReturn {
    if matches!(
        key_of(&info),
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter)
    ) {
        return OnTextInputReturn {
            update: on_unlock(data, info),
            valid: TextInputValid::Yes,
        };
    }
    keep()
}

/// "Unlock": the picked vault, the typed password, on the vault thread.
extern "C" fn on_unlock(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, app| {
        let t = now();
        if s.attempts.wait(t) > 0 || s.busy.is_some() {
            return;
        }
        let Some(key) = s.chosen_vault().map(|v| v.key.clone()) else {
            return;
        };
        if s.unlock.password.is_empty() {
            s.unlock.message = "Type the master password.".to_string();
            return;
        }
        let password = Zeroizing::new(s.unlock.password.as_str().to_string());
        s.unlock.message.clear();
        s.busy = Some("Unlocking\u{2026}".to_string());
        jobs::spawn(info, app, &s.data_root, Work::Unlock { key, password });
    })
}

extern "C" fn on_unlock_device(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        jobs::unlock_with_device(s, info)
    })
}

extern "C" fn on_show_create(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.screen = Screen::Create;
        s.create.message.clear();
    })
}

// ==== A new vault ====

fn create_screen(s: &KeysApp, app: &RefAny) -> Dom {
    let field =
        |placeholder: &str, value: &str, password: bool, which: CreateField, id: AzString| {
            let input = if password {
                TextInput::create_password()
            } else {
                TextInput::create()
            };
            input
                .with_text(value)
                .with_placeholder(placeholder)
                .with_accessibility_name(placeholder)
                .with_on_text_input(
                    RefAny::new(CreateRef {
                        app: app.clone(),
                        field: which,
                    }),
                    on_create_text as TextInputOnTextInputCallbackType,
                )
                .dom()
                .with_id(id)
        };
    let mut children = vec![
        block(
            "font-size: 18px; font-weight: 600; padding-bottom: 8px;",
            text("A new vault"),
        ),
        note(
            "One encrypted file in your data folder. Nobody - not even AzKeys - can open it \
              without the master password: choose one you will remember, and keep it written \
              down somewhere safe.",
        ),
        form_row(
            "Name",
            field(
                "Personal",
                s.create.name.as_str(),
                false,
                CreateField::Name,
                ids::CREATE_NAME,
            ),
        ),
        form_row(
            "Password",
            field(
                "Master password",
                s.create.password.as_str(),
                true,
                CreateField::Password,
                ids::CREATE_PASSWORD,
            ),
        ),
        form_row("", strength_bar(&s.create.password, ids::CREATE_STRENGTH)),
        form_row(
            "Again",
            field(
                "The same password again",
                s.create.confirm.as_str(),
                true,
                CreateField::Confirm,
                ids::CREATE_CONFIRM,
            ),
        ),
    ];
    let mut buttons = vec![primary("Create vault", ids::CREATE_BUTTON, app, on_create)];
    if !s.vaults.is_empty() {
        buttons.push(button("Cancel", ids::CREATE_CANCEL, app, on_create_cancel));
    }
    children.push(row("gap: 8px; padding: 8px 0px 4px 110px;", buttons));
    if !s.create.message.is_empty() {
        children.push(problem(&s.create.message, ids::CREATE_MESSAGE));
    }
    if let Some(busy) = &s.busy {
        children.push(note(busy));
    }
    children.push(note(
        "The master password derives the key with Argon2id (64 MiB, 3 passes); the vault is \
         sealed with XChaCha20-Poly1305.",
    ));
    column("", children)
}

#[derive(Clone, Copy)]
enum CreateField {
    Name,
    Password,
    Confirm,
}

struct CreateRef {
    app: RefAny,
    field: CreateField,
}

extern "C" fn on_create_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let Some((mut app, field)) = data
        .downcast_ref::<CreateRef>()
        .map(|r| (r.app.clone(), r.field))
    else {
        return keep();
    };
    let typed = Zeroizing::new(state.get_text().as_str().to_string());
    let Some(mut s) = app.downcast_mut::<KeysApp>() else {
        return keep();
    };
    match field {
        CreateField::Name => s.create.name = typed.to_string(),
        CreateField::Password => s.create.password = typed,
        CreateField::Confirm => s.create.confirm = typed,
    }
    // The strength bar follows the password.
    OnTextInputReturn {
        update: if matches!(field, CreateField::Password) {
            Update::RefreshDom
        } else {
            Update::DoNothing
        },
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_create(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, app| {
        if s.busy.is_some() {
            return;
        }
        let problem = s.create.problem();
        if !problem.is_empty() {
            s.create.message = problem;
            return;
        }
        s.create.message.clear();
        s.busy = Some("Making the vault\u{2026}".to_string());
        let vault = Vault::new(&s.create.name, now());
        let password = Zeroizing::new(s.create.password.as_str().to_string());
        jobs::spawn(
            info,
            app,
            &s.data_root,
            Work::Create {
                vault,
                password,
                kdf: None,
            },
        );
    })
}

extern "C" fn on_create_cancel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.create = Default::default();
        s.screen = Screen::Unlock;
    })
}
