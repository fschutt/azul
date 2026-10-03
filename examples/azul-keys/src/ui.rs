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

// ==== The vault ====

fn vault_screen(s: &KeysApp, session: &Session, app: &RefAny) -> Dom {
    PimShell::create(
        navigation(s, session, app),
        list_pane(session, app),
        crate::ui_item::reading_pane(s, session, app),
    )
    .with_list_label("Items")
    .office_shell()
    .with_title_row(kit::title_row(&format!(
        "{} - {}",
        SPEC.name, session.open.vault.name
    )))
    .with_ribbon(toolbar(app))
    .with_status_bar(status_bar(s, session))
    .dom()
}

/// The navigation: the vault's views, the kinds, the tags (each with its count).
fn navigation(s: &KeysApp, session: &Session, app: &RefAny) -> Dom {
    let vault = &session.open.vault;
    let count = |scope: &Scope| vault.items.iter().filter(|i| scope.holds(i)).count();
    let all = TreeViewNode::create(format!("All items ({})", vault.items.len()))
        .with_icon("inventory_2")
        .with_expanded(true)
        .with_selected(session.scope == Scope::All)
        .with_child(
            TreeViewNode::create(format!("Favourites ({})", count(&Scope::Favorites)))
                .with_icon("star")
                .with_selected(session.scope == Scope::Favorites),
        )
        .with_child(
            TreeViewNode::create(format!("One-time codes ({})", count(&Scope::OneTimeCodes)))
                .with_icon("pin")
                .with_selected(session.scope == Scope::OneTimeCodes),
        );
    let counts = vault.kind_counts();
    let mut kinds = TreeViewNode::create("Categories")
        .with_icon("category")
        .with_expanded(true);
    for kind in Kind::ALL {
        kinds = kinds.with_child(
            TreeViewNode::create(format!("{} ({})", kind.plural(), counts[kind.index()]))
                .with_icon(kind.icon())
                .with_selected(session.scope == Scope::Kind(kind)),
        );
    }
    let tag_list = vault.tags();
    let mut tags = TreeViewNode::create("Tags")
        .with_icon("sell")
        .with_expanded(true);
    for (tag, n) in &tag_list {
        tags = tags.with_child(
            TreeViewNode::create(format!("{tag} ({n})"))
                .with_icon("label")
                .with_selected(session.scope == Scope::Tag(tag.clone())),
        );
    }
    ShellNavigationPane::create()
        .with_header(primary("New item", ids::NAV_NEW, app, on_new))
        .with_group(
            ShellNavigationGroup::create("Vault", all)
                .with_count(vault.items.len())
                .with_open(s.nav_open[0]),
        )
        .with_group(ShellNavigationGroup::create("Categories", kinds).with_open(s.nav_open[1]))
        .with_group(
            ShellNavigationGroup::create("Tags", tags)
                .with_count(tag_list.len())
                .with_open(s.nav_open[2]),
        )
        .with_label("Vault, categories and tags")
        .with_on_event(
            app.clone(),
            on_nav as ShellNavigationPaneOnEventCallbackType,
        )
        .dom()
}

extern "C" fn on_nav(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: ShellNavigationPaneEvent,
) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        if matches!(event.kind, ShellNavigationPaneEventKind::GroupToggled) {
            if event.group < s.nav_open.len() {
                s.nav_open[event.group] = event.expand;
            }
            return;
        }
        if !matches!(event.kind, ShellNavigationPaneEventKind::NodeClicked) {
            return;
        }
        let Some(session) = s.session.as_mut() else {
            return;
        };
        let scope = match (event.group, event.index) {
            (0, 1) => Scope::Favorites,
            (0, 2) => Scope::OneTimeCodes,
            (1, k) if (1..=Kind::ALL.len()).contains(&k) => Scope::Kind(Kind::ALL[k - 1]),
            (2, k) if k >= 1 => match session.open.vault.tags().get(k - 1) {
                Some((tag, _)) => Scope::Tag(tag.clone()),
                None => return,
            },
            _ => Scope::All,
        };
        session.scope = scope;
        session.keep_selection_in_view();
        if matches!(
            session.reading,
            Reading::Generator | Reading::Audit(_) | Reading::Import(_)
        ) {
            session.reading = Reading::Item;
        }
        println!("AZKEYS_VIEW {}", session.view().len());
    })
}

struct RowRef {
    app: RefAny,
    id: String,
}

/// The list: the search, the heading, the items in letter sections.
fn list_pane(session: &Session, app: &RefAny) -> Dom {
    let list = session.view();
    let search = TextInput::create_search()
        .with_text(session.query.as_str())
        .with_placeholder("Search all items")
        .with_accessibility_name("Search all items")
        .with_on_text_input(app.clone(), on_search as TextInputOnTextInputCallbackType)
        .dom()
        .with_id(ids::LIST_SEARCH);
    let heading = block(
        "padding: 6px 8px 2px 8px; font-size: 12px; font-weight: 600;",
        text(format!("{} \u{b7} {}", session.scope.title(), list.len())),
    )
    .with_id(ids::LIST_HEADING);
    let body = if session.open.vault.items.is_empty() {
        ShellEmptyState::create("An empty vault")
            .with_icon("lock_open")
            .with_detail("Add a login, or import a browser's or a password manager's export.")
            .with_action_label("New item")
            .with_on_action(app.clone(), on_new as ButtonOnClickCallbackType)
            .dom()
    } else if list.is_empty() {
        ShellEmptyState::create("Nothing here")
            .with_icon("search")
            .with_detail(if session.query.trim().is_empty() {
                "No item is in this view.".to_string()
            } else {
                format!("No item matches \u{201c}{}\u{201d}.", session.query.trim())
            })
            .dom()
    } else {
        let mut rows = Dom::create_div().with_id(ids::LIST).with_css(
            "display: flex; flex-direction: column; flex-grow: 1; overflow-y: auto; min-height: 0px;",
        );
        let mut position = 0;
        for (letter, members) in sections(&session.open.vault, &list) {
            rows.add_child(block(
                "padding: 4px 8px; font-size: 11px; font-weight: 700; opacity: 0.8;",
                text(letter.to_string()),
            ));
            for i in members {
                rows.add_child(item_row(session, app, i, position));
                position += 1;
            }
        }
        rows
    };
    column(
        "flex-grow: 1; min-height: 0px;",
        vec![block("padding: 6px 8px;", search), heading, body],
    )
}

fn item_row(session: &Session, app: &RefAny, index: usize, position: usize) -> Dom {
    let item = &session.open.vault.items[index];
    let selected = session.selected.as_deref() == Some(item.id.as_str());
    let mut texts = vec![block("font-size: 13px;", text(item.title.as_str()))];
    let subtitle = item.subtitle();
    if !subtitle.is_empty() {
        texts.push(block("font-size: 11px; opacity: 0.7;", text(subtitle)));
    }
    let mut children = vec![
        Avatar::create(item.initials())
            .with_size(AvatarSize::Small)
            .dom(),
        column("flex-grow: 1; padding-left: 8px; min-width: 0px;", texts),
    ];
    if !item.totp.is_empty() {
        children.push(block(
            "padding: 0px 4px; font-size: 11px; opacity: 0.7;",
            text("2FA"),
        ));
    }
    if item.favorite {
        children.push(block(
            "padding: 0px 6px; font-size: 13px;",
            text("\u{2605}"),
        ));
    }
    row(
        &format!(
            "padding: 4px 8px; cursor: pointer; {}",
            if selected {
                "background-color: rgba(64, 128, 255, 0.18);"
            } else {
                ""
            }
        ),
        children,
    )
    .with_id(ids::row(position))
    .with_class(ids::ROW_CLASS)
    .with_accessibility_name(item.title.as_str())
    .with_callback(
        EventFilter::Hover(HoverEventFilter::MouseUp),
        RefAny::new(RowRef {
            app: app.clone(),
            id: item.id.clone(),
        }),
        on_row,
    )
}

/// Leaves an edit form for another item; `false` (and the question) when it has changes.
pub(crate) fn leave_form(session: &mut Session) -> bool {
    if let Reading::Edit(form) = &mut session.reading {
        if form.changed() {
            form.confirm_discard = true;
            return false;
        }
    }
    true
}

extern "C" fn on_row(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id)) = data
        .downcast_ref::<RowRef>()
        .map(|r| (r.app.clone(), r.id.clone()))
    else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        let Some(session) = s.session.as_mut() else {
            return;
        };
        if !leave_form(session) {
            return;
        }
        session.reading = Reading::Item;
        session.select(Some(id));
        if let Some(item) = session.selected_item() {
            println!("AZKEYS_SELECTED {}", item.title);
        }
    })
}

extern "C" fn on_search(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let query = state.get_text().as_str().to_string();
    let update = with_app(&mut data, &mut info, |s, _info, _| {
        if let Some(session) = s.session.as_mut() {
            session.query = query;
            session.keep_selection_in_view();
            println!("AZKEYS_VIEW {}", session.view().len());
        }
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

fn toolbar(app: &RefAny) -> Dom {
    row(
        "gap: 4px; padding: 4px 8px;",
        vec![
            icon_button("New", "add", ids::TOOLBAR_NEW, app, on_new),
            icon_button(
                "Generator",
                "password",
                ids::TOOLBAR_GENERATOR,
                app,
                on_generator,
            ),
            icon_button(
                "Audit",
                "health_and_safety",
                ids::TOOLBAR_AUDIT,
                app,
                on_audit,
            ),
            icon_button("Import", "file_upload", ids::TOOLBAR_IMPORT, app, on_import),
            block("flex-grow: 1;", Dom::create_div()),
            icon_button("Lock", "lock", ids::TOOLBAR_LOCK, app, on_lock),
            icon_button(
                "Settings",
                "settings",
                ids::TOOLBAR_SETTINGS,
                app,
                on_open_settings,
            ),
        ],
    )
}

fn status_bar(s: &KeysApp, session: &Session) -> Dom {
    let t = now();
    let n = session.open.vault.items.len();
    let mut segments = vec![StatusBarSegment::create(format!(
        "{n} item{}",
        if n == 1 { "" } else { "s" }
    ))];
    if s.saving {
        segments.push(StatusBarSegment::create("Saving\u{2026}"));
    }
    segments.push(StatusBarSegment::create(match s.auto_lock.remaining(t) {
        None => "Locks only by hand".to_string(),
        Some(r) if r <= 60 => format!("Locks in {}", crate::lock::clock(r)),
        Some(_) => format!(
            "Locks after {} idle",
            minutes_label(s.auto_lock.idle_minutes)
        ),
    }));
    if let Some(r) = s.clipboard.remaining(t) {
        segments.push(StatusBarSegment::create(format!(
            "Clipboard clears in {r} s"
        )));
    }
    if !s.notice.is_empty() {
        segments.push(StatusBarSegment::create(s.notice.as_str()));
    }
    StatusBar::create(segments).dom().with_id(ids::STATUS)
}

// ==== The toolbar's commands ====

/// A new item of `kind` in the reading pane (asks first when an edit has changes).
pub(crate) fn new_item(s: &mut KeysApp, kind: Kind) {
    let Some(session) = s.session.as_mut() else {
        return;
    };
    if !leave_form(session) {
        return;
    }
    session.reading = Reading::Edit(Form::new_item(kind, now()));
}

extern "C" fn on_new(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| new_item(s, Kind::Login))
}

extern "C" fn on_generator(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        if let Some(session) = s.session.as_mut() {
            if leave_form(session) {
                session.reading = Reading::Generator;
                crate::ui_item::regenerate(session);
            }
        }
    })
}

extern "C" fn on_audit(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        if let Some(session) = s.session.as_mut() {
            if leave_form(session) {
                session.reading = Reading::Audit(crate::audit::Filter::All);
            }
        }
    })
}

extern "C" fn on_import(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        if let Some(session) = s.session.as_mut() {
            if leave_form(session) {
                session.reading = Reading::Import(Default::default());
            }
        }
    })
}

extern "C" fn on_lock(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, app| {
        jobs::lock(s, info, app)
    })
}

extern "C" fn on_open_settings(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        kit::open_settings(&s.kit, None)
    })
}

// ==== Import files ====

/// The write-back tag of an import file's read.
const TAG_IMPORT: u64 = 1;

/// Reads a file to import (outside the data tree) on a Thread; the preview shows its items.
pub fn read_import_file(s: &mut KeysApp, info: &mut CallbackInfo, app: &RefAny, path: &Path) {
    let Some(session) = s.session.as_mut() else {
        return;
    };
    session.reading = Reading::Import(crate::session::ImportView {
        path: path.display().to_string(),
        reading: true,
        result: None,
    });
    if !kit::spawn_outside_read(info, path, app.clone(), TAG_IMPORT, on_import_read) {
        if let Reading::Import(view) = &mut session.reading {
            view.reading = false;
            view.result = Some(Err("This is not a file name AzKeys can read.".to_string()));
        }
    }
}

extern "C" fn on_import_read(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        let Some(session) = s.session.as_mut() else {
            return;
        };
        let Reading::Import(view) = &mut session.reading else {
            return;
        };
        view.reading = false;
        let got = reply.outcomes.into_iter().find_map(|o| match o {
            azul_appkit::files::FileOutcome::Got { result, .. } => Some(result),
            _ => None,
        });
        let result = match got {
            Some(Ok(Some(bytes))) => {
                // The export holds plain passwords: its bytes are overwritten once parsed.
                let bytes = Zeroizing::new(bytes);
                crate::import::import_file(&bytes, now())
            }
            Some(Ok(None)) => Err("The file does not exist.".to_string()),
            Some(Err(e)) => Err(format!("The file cannot be read: {e}")),
            None => Err("The file was not read.".to_string()),
        };
        match &result {
            Ok(imported) => println!(
                "AZKEYS_IMPORT_PREVIEW {} {} {}",
                imported.items.len(),
                imported.skipped.len(),
                imported.format.label()
            ),
            Err(_) => println!("AZKEYS_IMPORT_PREVIEW 0 0 none"),
        }
        view.result = Some(result);
    })
}

// ==== The close guard ====

/// The guard asks when a close arrives: does an edited item hold changes now?
extern "C" fn dirty_check(mut data: RefAny, _info: CallbackInfo) -> CloseGuardDocumentState {
    match data.downcast_ref::<KeysApp>() {
        Some(s) if s.has_unsaved_form() => CloseGuardDocumentState::Unsaved,
        _ => CloseGuardDocumentState::Saved,
    }
}

/// The answer to "save the item?": Save keeps it (the window closes once the vault is written),
/// Don't Save drops the form (the guard closes the window), Cancel keeps the window.
extern "C" fn on_close_answer(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: CloseGuardEvent,
) -> Update {
    with_app(&mut data, &mut info, |s, info, app| match event.kind {
        CloseGuardEventKind::Ask => s.asking_close = true,
        CloseGuardEventKind::Cancel => s.asking_close = false,
        CloseGuardEventKind::Discard => {
            s.asking_close = false;
            if let Some(session) = s.session.as_mut() {
                session.reading = Reading::Item;
            }
        }
        CloseGuardEventKind::Save => {
            s.asking_close = false;
            let saved = s.session.as_mut().map(|session| session.save_form(now()));
            match saved {
                Some(Ok(_)) => {
                    jobs::save(s, info, app);
                    if s.saving {
                        s.close_after_save = true;
                    } else {
                        info.close_window();
                    }
                }
                Some(Err(problem)) => s.notice = problem,
                None => {}
            }
        }
    })
}

// ==== Keys ====

extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((kit_ref, in_vault)) = data.downcast_ref::<KeysApp>().map(|s| {
        (
            s.kit.clone(),
            s.screen == Screen::Vault && s.session.is_some(),
        )
    }) else {
        return Update::DoNothing;
    };
    if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
        return update;
    }
    if kit::settings_open(&kit_ref) || !in_vault {
        return Update::DoNothing;
    }
    let Some(key) = key_of(&info) else {
        return Update::DoNothing;
    };
    let m = info.get_key_modifiers();
    let command = m.primary_down();
    // A text field or a selection keeps its own copy and arrows.
    let text_focus =
        info.get_focused_node().into_option().is_some() || info.has_selection(DomId { inner: 0 });
    use VirtualKeyCode as K;
    match (key, command, m.shift) {
        (K::N, true, _) => {
            info.prevent_default();
            on_new(data, info)
        }
        (K::L, true, _) => {
            info.prevent_default();
            on_lock(data, info)
        }
        (K::G, true, _) => {
            info.prevent_default();
            on_generator(data, info)
        }
        (K::E, true, _) => {
            info.prevent_default();
            crate::ui_item::on_edit(data, info)
        }
        (K::S, true, _) => {
            info.prevent_default();
            crate::ui_item::on_edit_save(data, info)
        }
        (K::C, true, shift) if !text_focus => {
            info.prevent_default();
            let field = if shift { "totp" } else { "password" };
            with_app(&mut data, &mut info, |s, info, _| {
                jobs::copy_secret(s, info, field)
            })
        }
        (K::B, true, _) if !text_focus => {
            info.prevent_default();
            with_app(&mut data, &mut info, |s, info, _| {
                jobs::copy_secret(s, info, "username")
            })
        }
        (K::Escape, false, _) => crate::ui_item::on_escape(data, info),
        (K::Delete, false, _) if !text_focus => crate::ui_item::on_delete(data, info),
        (K::Up | K::Down, false, _) if !text_focus => {
            with_app(&mut data, &mut info, |s, _info, _| {
                let Some(session) = s.session.as_mut() else {
                    return;
                };
                if !matches!(session.reading, Reading::Item) {
                    return;
                }
                let list = session.view();
                if list.is_empty() {
                    return;
                }
                let at = session.selected.as_deref().and_then(|id| {
                    list.iter()
                        .position(|&i| session.open.vault.items[i].id == id)
                });
                let next = match (at, key == K::Down) {
                    (None, _) => 0,
                    (Some(p), true) => (p + 1).min(list.len() - 1),
                    (Some(p), false) => p.saturating_sub(1),
                };
                let id = session.open.vault.items[list[next]].id.clone();
                session.select(Some(id));
            })
        }
        _ => {
            if let Some(mut s) = data.downcast_mut::<KeysApp>() {
                s.touch(now());
            }
            Update::DoNothing
        }
    }
}

// ==== The settings page's sections ====

fn settings_sections(s: &KeysApp, app: &RefAny) -> Vec<AppSection> {
    let idle_labels: Vec<String> = IDLE_CHOICES.iter().map(|m| minutes_label(*m)).collect();
    let clear_labels: Vec<String> = CLEAR_CHOICES.iter().map(|c| seconds_label(*c)).collect();
    let idle_refs: Vec<&str> = idle_labels.iter().map(String::as_str).collect();
    let clear_refs: Vec<&str> = clear_labels.iter().map(String::as_str).collect();
    let idle_at = IDLE_CHOICES
        .iter()
        .position(|m| *m == s.settings.idle_minutes)
        .unwrap_or(2);
    let clear_at = CLEAR_CHOICES
        .iter()
        .position(|c| *c == s.settings.clear_seconds)
        .unwrap_or(1);
    let mut security = vec![
        kit::row(
            "Lock after",
            DropDown::create(strs(&idle_refs))
                .with_selected(idle_at)
                .with_accessibility_name("Lock after")
                .with_on_choice_change(
                    app.clone(),
                    on_idle_choice as DropDownOnChoiceChangeCallbackType,
                )
                .dom()
                .with_id(ids::SET_IDLE),
        ),
        kit::row(
            "Clear the clipboard",
            DropDown::create(strs(&clear_refs))
                .with_selected(clear_at)
                .with_accessibility_name("Clear the clipboard after")
                .with_on_choice_change(
                    app.clone(),
                    on_clear_choice as DropDownOnChoiceChangeCallbackType,
                )
                .dom()
                .with_id(ids::SET_CLEAR),
        ),
    ];
    match s.session.as_ref() {
        Some(session) => {
            let on = s.device_unlock(&session.open.vault.id).is_some();
            security.push(kit::row(
                "Device unlock",
                Switch::create(on)
                    .with_accessibility_name(
                        "Unlock this vault on this device without the password",
                    )
                    .with_on_toggle(app.clone(), on_device_unlock as SwitchOnToggleCallbackType)
                    .dom()
                    .with_id(ids::SET_DEVICE_UNLOCK),
            ));
            security.push(kit::note(
                "Keeps this vault's key in the system keyring, behind biometrics where the system \
                 has them (Touch ID, Windows Hello, fprintd), so this device unlocks the vault \
                 without the master password.",
            ));
        }
        None => security.push(kit::note(
            "Unlock a vault to let this device unlock it with biometrics.",
        )),
    }
    security.push(kit::note(
        "AzKeys makes no network requests: no breach checks, no website icons. A copied secret is \
         cleared from the clipboard when the countdown ends, even if something else was copied \
         since (azul cannot read the clipboard outside a paste).",
    ));
    let mut vault = Vec::new();
    match s.session.as_ref() {
        Some(session) => {
            let kdf = &session.open.envelope.kdf;
            vault.push(kit::row("Vault", text(session.open.vault.name.as_str())));
            vault.push(kit::row(
                "File",
                text(
                    azul_appkit::data::local_path(&s.data_root, &session.open.key)
                        .display()
                        .to_string(),
                ),
            ));
            vault.push(kit::row(
                "Key derivation",
                text(format!(
                    "Argon2id, {} MiB, {} passes, {} lane(s); XChaCha20-Poly1305",
                    kdf.memory_kib / 1024,
                    kdf.iterations,
                    kdf.parallelism
                )),
            ));
            vault.push(kit::row(
                "New password",
                change_field(
                    app,
                    "New master password",
                    s.change.password.as_str(),
                    ChangeField::Password,
                    ids::SET_NEW_PASSWORD,
                ),
            ));
            vault.push(kit::row(
                "Again",
                change_field(
                    app,
                    "The same password again",
                    s.change.confirm.as_str(),
                    ChangeField::Confirm,
                    ids::SET_NEW_CONFIRM,
                ),
            ));
            vault.push(kit::row(
                "",
                button(
                    "Change the master password",
                    ids::SET_CHANGE,
                    app,
                    on_change_password,
                ),
            ));
            if !s.change.message.is_empty() {
                vault.push(kit::note(&s.change.message).with_id(ids::SET_CHANGE_MESSAGE));
            }
            vault.push(kit::note(
                "The vault key stays the same: a device that unlocks the vault with biometrics \
                 keeps doing so. The file before each save is kept in keys/backups/.",
            ));
        }
        None => vault.push(kit::note(
            "Unlock a vault to see its file and change its master password.",
        )),
    }
    vault.push(kit::note(
        "Import: CSV exports of Chrome, Edge, Firefox, Safari, Bitwarden, 1Password, KeePassXC and \
         LastPass, and Bitwarden's unencrypted JSON export (the toolbar's Import). OpenPGP keys \
         are a later step.",
    ));
    vec![
        AppSection {
            category: 0,
            title: "Security".to_string(),
            content: column("", security),
        },
        AppSection {
            category: 1,
            title: "Vault".to_string(),
            content: column("", vault),
        },
    ]
}

extern "C" fn on_idle_choice(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        let minutes = IDLE_CHOICES[index.min(IDLE_CHOICES.len() - 1)];
        s.settings.idle_minutes = minutes;
        s.auto_lock.idle_minutes = minutes;
        kit::set_value(&s.kit, info, "idle_minutes", &minutes.to_string());
    })
}

extern "C" fn on_clear_choice(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        let seconds = CLEAR_CHOICES[index.min(CLEAR_CHOICES.len() - 1)];
        s.settings.clear_seconds = seconds;
        kit::set_value(&s.kit, info, "clear_seconds", &seconds.to_string());
    })
}

extern "C" fn on_device_unlock(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SwitchState,
) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        if state.checked {
            jobs::enable_device_unlock(s, info);
        } else {
            jobs::disable_device_unlock(s, info);
        }
    })
}

#[derive(Clone, Copy)]
enum ChangeField {
    Password,
    Confirm,
}

struct ChangeRef {
    app: RefAny,
    field: ChangeField,
}

fn change_field(
    app: &RefAny,
    placeholder: &str,
    value: &str,
    field: ChangeField,
    id: AzString,
) -> Dom {
    TextInput::create_password()
        .with_text(value)
        .with_placeholder(placeholder)
        .with_accessibility_name(placeholder)
        .with_on_text_input(
            RefAny::new(ChangeRef {
                app: app.clone(),
                field,
            }),
            on_change_text as TextInputOnTextInputCallbackType,
        )
        .dom()
        .with_id(id)
}

extern "C" fn on_change_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let Some((mut app, field)) = data
        .downcast_ref::<ChangeRef>()
        .map(|r| (r.app.clone(), r.field))
    else {
        return keep();
    };
    let typed = Zeroizing::new(state.get_text().as_str().to_string());
    if let Some(mut s) = app.downcast_mut::<KeysApp>() {
        match field {
            ChangeField::Password => s.change.password = typed,
            ChangeField::Confirm => s.change.confirm = typed,
        }
    }
    keep()
}

extern "C" fn on_change_password(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, app| {
        if s.saving {
            s.change.message = "Wait for the save on the way, then try again.".to_string();
            return;
        }
        if s.change.password.chars().count() < 8 {
            s.change.message = "The master password needs 8 characters or more.".to_string();
            return;
        }
        if *s.change.password != *s.change.confirm {
            s.change.message = "The two passwords differ.".to_string();
            return;
        }
        let Some(session) = s.session.as_ref() else {
            return;
        };
        let work = Work::ChangePassword {
            key: session.open.key.clone(),
            envelope: session.open.envelope.clone(),
            vault_key: session.open.vault_key.clone(),
            password: Zeroizing::new(s.change.password.as_str().to_string()),
            kdf: None,
        };
        s.saving = true;
        s.change.message = "Changing the master password\u{2026}".to_string();
        jobs::spawn(info, app, &s.data_root, work);
    })
}
