//! The Add drive dialog's pages (its data is `add_drive`, what its buttons start `add_flow`):
//!
//! ```text
//! Choose     [Buy storage]          Azlin cloud storage, the first month free
//!            [Connect data source]  S3, WebDAV, FTP, Google Drive, GitHub, databases, ...
//! Buy        < Back   the tiers (size, price a month or a year)   [ ] Pay yearly
//!            Name [Azlin Storage]   [Create test drive] (a development server)  [Buy ...]
//! Sources    < Back   the sources of this build in their groups (a tile each)
//! Form       < Back   the source, Name, one field per setting (text, password, a switch, a
//!            choice, a path with Choose...), the test's sentence  [Test connection] [Cancel]
//!            [Add drive]
//! ```
//!
//! The dialog is azul's modal `Dialog` - a `<transient-window>` over AzDrive's window (or the
//! sheet of `--dialogs inline`); its content is a subtree of the window's DOM, so every click
//! here asks EVERY window to rebuild (`RefreshDomAllWindows`): the window builds the new page,
//! the dialog's window shows it. The widgets are azul's (Tile, Button, TextInput, CheckBox,
//! DropDown): they follow the app theme.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType,
        DropDownOnChoiceChangeCallbackType, TextInputOnTextInputCallbackType,
        TileOnClickCallbackType,
    },
    dialog::{FileDialog, FileOpenResult},
    option::OptionFileTypeList,
    prelude::*,
    str::String as AzString,
    vec::StringVec,
    widgets::{
        ButtonType, CheckBoxState, DropDown, OnTextInputReturn, TextInputState, TextInputValid,
        Tile,
    },
};
use azul_storage::catalog::{FieldKind, FieldSpec, ServiceSpec};

use crate::{
    add_drive::{source_groups, AddDialog, AddPage, BuyStep, TiersState},
    add_flow::{self, AddEvent},
    ids, look, with_state, DriveState, Popup,
};

// ==== Pieces ====

/// Every rebuild the dialog asks for reaches the window that holds its content.
fn everywhere(update: Update) -> Update {
    match update {
        Update::RefreshDom => Update::RefreshDomAllWindows,
        other => other,
    }
}

/// A button or a tile's event.
struct EventRef {
    app: RefAny,
    event: AddEvent,
}

fn event_ref(app: &RefAny, event: AddEvent) -> RefAny {
    RefAny::new(EventRef {
        app: app.clone(),
        event,
    })
}

/// Where a text field writes.
#[derive(Clone, Copy)]
enum TextTarget {
    /// The drive's name (a form's).
    Name,
    /// Buy storage's name.
    BuyName,
    /// A form's field.
    Field(&'static str),
}

struct TextRef {
    app: RefAny,
    target: TextTarget,
}

/// A form's switch, choice or path field.
struct KeyRef {
    app: RefAny,
    key: &'static str,
    /// A path field: a folder (else a file).
    folder: bool,
}

/// The dialog kit's text size (azul's wizard pages, settings rows and standard dialogs: text and
/// field values 13px, hints 12px): the dialog's text and its fields' values are set in it.
const TEXT_SIZE: &str = "font-size: 13px;";

fn label(text: &str) -> Dom {
    Dom::create_span_with_text(AzString::from(text))
        .with_css("font-size: 12px; opacity: 0.75; margin-top: 10px; margin-bottom: 4px;")
}

fn note(text: &str) -> Dom {
    Dom::create_span_with_text(AzString::from(text))
        .with_css("font-size: 12px; opacity: 0.75; margin-top: 4px;")
}

fn line(text: &str) -> Dom {
    Dom::create_span_with_text(AzString::from(text)).with_css("margin-top: 8px;")
}

fn error_line(text: &str) -> Dom {
    line(text)
        .with_id(ids::ADD_ERROR)
        .with_css("color: #C42B1C;")
}

fn heading(text: &str) -> Dom {
    Dom::create_span_with_text(AzString::from(text)).with_css(
        "font-size: 11px; font-weight: bold; text-transform: uppercase; letter-spacing: 1px; \
         opacity: 0.7; margin: 12px 0px 6px 0px;",
    )
}

fn column(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_children(DomVec::from(children))
}

/// The buttons under a page, at its right.
fn buttons(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; justify-content: flex-end; flex-wrap: wrap; \
             margin-top: 16px;",
        )
        .with_children(DomVec::from(children))
}

/// A dialog button running `event`; disabled with `why_not`.
fn button(
    app: &RefAny,
    text: &str,
    kind: ButtonType,
    event: AddEvent,
    id: AzString,
    why_not: Option<&str>,
) -> Dom {
    let mut b = Button::with_type(AzString::from(text), kind)
        .with_on_click(event_ref(app, event), on_event as ButtonOnClickCallbackType);
    if let Some(why) = why_not {
        b = b.with_disabled(AzString::from(why));
    }
    b.dom().with_id(id).with_css("margin-left: 6px; margin-top: 4px;")
}

/// "< Back" at the top of a page.
fn back(app: &RefAny) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; margin-bottom: 6px;")
        .with_child(
            Button::create(AzString::from("Back"))
                .with_icon(AzString::from("arrow_back"))
                .with_on_click(event_ref(app, AddEvent::Back), on_event as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::ADD_BACK),
        )
}

/// A big choice or a source: azul's Tile (an icon beside a title over a detail line).
fn tile(
    app: &RefAny,
    icon: &str,
    title: &str,
    detail: &str,
    event: AddEvent,
    selected: bool,
    css: &str,
) -> Dom {
    Tile::create(AzString::from(title))
        .with_icon(AzString::from(icon))
        .with_detail(AzString::from(detail))
        .with_selected(selected)
        .with_on_click(event_ref(app, event), on_event as TileOnClickCallbackType)
        .dom()
        .with_css(format!(
            "{css} {}",
            if selected { look::TILE_SELECTED } else { "" }
        ))
}

/// A text field writing to `target`.
fn text_field(app: &RefAny, value: &str, placeholder: &str, secret: bool, target: TextTarget) -> Dom {
    let base = if secret {
        TextInput::create_password()
    } else {
        TextInput::create()
    };
    base.with_text(AzString::from(value))
        .with_placeholder(AzString::from(placeholder))
        .with_on_text_input(
            RefAny::new(TextRef {
                app: app.clone(),
                target,
            }),
            on_text as TextInputOnTextInputCallbackType,
        )
        .dom()
        // The value in the dialog's text size, not the field's own (smaller) default.
        .with_css(TEXT_SIZE)
}

// ==== The dialog ====

/// The dialog's title and content for `d`; `development`: the token server is a development one
/// (Buy storage offers a test drive).
pub(crate) fn dialog(d: &AddDialog, development: bool, app: &RefAny) -> (String, Dom) {
    let (title, page) = match d.page {
        AddPage::Choose => (String::from("Add a drive"), choose(app)),
        AddPage::Buy => (String::from("Buy storage"), buy(d, development, app)),
        AddPage::Sources => (String::from("Connect a data source"), sources(app)),
        AddPage::Form => {
            let title = match (&d.editing, d.spec()) {
                (Some(_), _) => format!("Enter the keys of \"{}\" again", d.name),
                (None, Some(spec)) => format!("Connect {}", spec.name),
                (None, None) => String::from("Connect a data source"),
            };
            (title, form(d, app))
        }
    };
    // Text that sets no size of its own (a check box's label, a status line) takes the dialog
    // kit's, not the dialog panel's larger one.
    let content = Dom::create_div()
        .with_id(ids::ADD_DRIVE)
        .with_css("display: flex; flex-direction: column; width: 460px; max-width: 100%;")
        .with_css(TEXT_SIZE)
        .with_child(page);
    (title, content)
}

/// The two choices.
fn choose(app: &RefAny) -> Dom {
    let css = "width: 100%; margin-bottom: 10px; padding: 10px;";
    column(vec![
        tile(
            app,
            "shopping_cart",
            "Buy storage",
            "Azlin cloud storage from 100 GB - the first month is free",
            AddEvent::ChooseBuy,
            false,
            css,
        )
        .with_id(ids::ADD_CHOICE_BUY),
        tile(
            app,
            "cable",
            "Connect data source",
            "S3, WebDAV, FTP, Google Drive, Dropbox, GitHub, databases ...",
            AddEvent::ChooseConnect,
            false,
            css,
        )
        .with_id(ids::ADD_CHOICE_CONNECT),
        buttons(vec![button(
            app,
            "Cancel",
            ButtonType::Default,
            AddEvent::Cancel,
            ids::ADD_CANCEL,
            None,
        )]),
    ])
    .with_id(ids::ADD_CHOOSE)
}

/// Buy storage: the tiers and their prices, monthly or yearly, the name, Create test drive
/// (a development token server), Buy.
fn buy(d: &AddDialog, development: bool, app: &RefAny) -> Dom {
    let mut children = vec![
        back(app),
        Dom::create_span_with_text(AzString::from("Azlin cloud storage"))
            .with_css("font-size: 15px; font-weight: bold;"),
        note(
            "Storage for AzDrive, AzMail and every Azlin app, paid monthly or yearly. The first \
             month is free.",
        ),
    ];
    match &d.tiers {
        TiersState::NotLoaded | TiersState::Loading => {
            children.push(line("Loading the storage tiers ...").with_id(ids::ADD_STATUS));
        }
        TiersState::Failed(why) => {
            children.push(error_line(&format!("The storage tiers could not be loaded: {why}")));
            children.push(buttons(vec![button(
                app,
                "Try again",
                ButtonType::Default,
                AddEvent::RetryTiers,
                ids::ADD_RETRY,
                None,
            )]));
        }
        TiersState::Loaded(tiers) => {
            let cells: Vec<Dom> = tiers
                .tiers
                .iter()
                .enumerate()
                .map(|(index, tier)| {
                    let price = tier
                        .price_text(d.yearly)
                        .unwrap_or_else(|| String::from("price on request"));
                    tile(
                        app,
                        "cloud",
                        &tier.quota_text(),
                        &price,
                        AddEvent::Tier(index),
                        index == d.tier,
                        // two to a line: the width holds the tile's padding
                        "width: 222px; box-sizing: border-box; margin: 0px 8px 8px 0px;",
                    )
                    .with_id(ids::add_tier(index))
                })
                .collect();
            children.push(
                Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: row; flex-wrap: wrap; margin-top: 10px; \
                         max-height: 260px; overflow-y: auto;",
                    )
                    .with_children(DomVec::from(cells)),
            );
            children.push(
                Dom::create_div()
                    .with_css("display: flex; flex-direction: row; align-items: center;")
                    .with_child(
                        CheckBox::create(d.yearly)
                            .with_accessibility_name(AzString::from("Pay yearly"))
                            .with_on_toggle(
                                event_ref(app, AddEvent::Yearly),
                                on_yearly as CheckBoxOnToggleCallbackType,
                            )
                            .dom()
                            .with_id(ids::ADD_YEARLY),
                    )
                    .with_child(
                        Dom::create_span_with_text(AzString::from(
                            "Pay yearly (two months for free)",
                        ))
                        .with_css("margin-left: 8px;")
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::Click),
                            event_ref(app, AddEvent::Yearly),
                            on_event,
                        ),
                    ),
            );
            if let Some(consent) = &tiers.withdrawal_consent {
                children.push(note(consent));
            }
            children.push(label("Name"));
            children.push(
                text_field(app, &d.buy_name, "Azlin Storage", false, TextTarget::BuyName)
                    .with_id(ids::ADD_NAME),
            );
        }
    }
    if !d.notice.is_empty() {
        children.push(line(&d.notice).with_id(ids::ADD_STATUS));
    }
    let loaded = matches!(d.tiers, TiersState::Loaded(_));
    let mut row = Vec::new();
    if d.paying() {
        row.push(button(
            app,
            "Stop waiting",
            ButtonType::Default,
            AddEvent::StopWaiting,
            ids::ADD_STOP,
            None,
        ));
    } else {
        let busy = d.busy().then_some("Wait for the step that runs.");
        let not_loaded = (!loaded).then_some("The storage tiers are not loaded yet.");
        if development {
            row.push(button(
                app,
                "Create test drive",
                ButtonType::Default,
                AddEvent::CreateTestDrive,
                ids::ADD_CREATE_TEST,
                busy.or(not_loaded),
            ));
        }
        row.push(button(
            app,
            "Cancel",
            ButtonType::Default,
            AddEvent::Cancel,
            ids::ADD_CANCEL,
            None,
        ));
        row.push(button(
            app,
            &d.buy_label(),
            ButtonType::Primary,
            AddEvent::Buy,
            ids::ADD_BUY_BUTTON,
            busy.or(not_loaded),
        ));
    }
    if matches!(d.step, BuyStep::Paying { .. }) {
        children.push(note(
            "Paying happens on the payment page in your browser; this window only waits for the \
             drive.",
        ));
    }
    children.push(buttons(row));
    column(children).with_id(ids::ADD_BUY)
}

/// The sources this build can open, in their groups.
fn sources(app: &RefAny) -> Dom {
    let (groups, unavailable) = source_groups();
    let mut list = Vec::new();
    for (group, specs) in groups {
        list.push(heading(group.title()));
        let cells: Vec<Dom> = specs
            .iter()
            .map(|spec| {
                tile(
                    app,
                    spec.icon,
                    spec.name,
                    spec.summary,
                    AddEvent::Service(spec.id),
                    false,
                    // one to a line, so a source's summary is not cut off
                    "width: 100%; box-sizing: border-box; margin-bottom: 4px;",
                )
                .with_id(ids::add_service(spec.id))
            })
            .collect();
        list.push(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column;")
                .with_children(DomVec::from(cells)),
        );
    }
    let mut children = vec![
        back(app),
        note("Choose what to connect. Passwords, tokens and keys stay in the system keyring."),
        Dom::create_div()
            .with_css(
                "display: flex; flex-direction: column; max-height: 380px; overflow-y: auto; \
                 margin-top: 4px;",
            )
            .with_children(DomVec::from(list)),
    ];
    if unavailable > 0 {
        children.push(note(&format!(
            "{unavailable} more sources need a build of AzDrive with OpenDAL or the database \
             drivers."
        )));
    }
    children.push(buttons(vec![button(
        app,
        "Cancel",
        ButtonType::Default,
        AddEvent::Cancel,
        ids::ADD_CANCEL,
        None,
    )]));
    column(children).with_id(ids::ADD_SOURCES)
}

/// One field of a source's form.
fn field(d: &AddDialog, app: &RefAny, f: &'static FieldSpec) -> Dom {
    let title = if f.required {
        f.label.to_string()
    } else {
        format!("{} (optional)", f.label)
    };
    let value = d.value(f.key);
    let control = match f.kind {
        FieldKind::Bool => {
            return Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; margin-top: 10px;",
                )
                .with_child(
                    CheckBox::create(d.bool_value(f.key))
                        .with_accessibility_name(AzString::from(f.label))
                        .with_on_toggle(
                            RefAny::new(KeyRef {
                                app: app.clone(),
                                key: f.key,
                                folder: false,
                            }),
                            on_check as CheckBoxOnToggleCallbackType,
                        )
                        .dom()
                        .with_id(ids::add_field(f.key)),
                )
                .with_child(
                    Dom::create_span_with_text(AzString::from(f.label))
                        .with_css("margin-left: 8px;"),
                );
        }
        FieldKind::Choice(words) => {
            let selected = words.iter().position(|w| *w == value).unwrap_or(0);
            DropDown::create(StringVec::from(
                words
                    .iter()
                    .map(|w| AzString::from(*w))
                    .collect::<Vec<AzString>>(),
            ))
            .with_selected(selected)
            .with_accessibility_name(AzString::from(f.label))
            .with_on_choice_change(
                RefAny::new(KeyRef {
                    app: app.clone(),
                    key: f.key,
                    folder: false,
                }),
                on_choice as DropDownOnChoiceChangeCallbackType,
            )
            .dom()
            .with_id(ids::add_field(f.key))
        }
        FieldKind::Path => Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center;")
            .with_child(
                text_field(app, value, f.placeholder, false, TextTarget::Field(f.key))
                    .with_id(ids::add_field(f.key))
                    .with_css("flex-grow: 1; min-width: 0px;"),
            )
            .with_child(
                Button::create(AzString::from("Choose ..."))
                    .with_on_click(
                        RefAny::new(KeyRef {
                            app: app.clone(),
                            key: f.key,
                            // A database is a file; every other path a folder.
                            folder: f.key != "path",
                        }),
                        on_choose_path as ButtonOnClickCallbackType,
                    )
                    .dom()
                    .with_id(ids::add_choose(f.key))
                    .with_css("margin-left: 6px;"),
            ),
        FieldKind::Secret => {
            text_field(app, value, f.placeholder, true, TextTarget::Field(f.key))
                .with_id(ids::add_field(f.key))
        }
        FieldKind::Text | FieldKind::Url | FieldKind::Number => {
            text_field(app, value, f.placeholder, false, TextTarget::Field(f.key))
                .with_id(ids::add_field(f.key))
        }
    };
    let mut parts = vec![label(&title), control];
    if !f.help.is_empty() {
        parts.push(note(f.help));
    }
    column(parts)
}

/// A source's form: its name, its fields, the test's sentence, Test connection / Add drive.
fn form(d: &AddDialog, app: &RefAny) -> Dom {
    let Some(spec) = d.spec() else {
        return column(vec![back(app), error_line("Choose a source first.")]);
    };
    let mut children = Vec::new();
    if d.editing.is_none() {
        children.push(back(app));
    }
    children.push(header(spec));
    let mut fields = vec![
        label("Name"),
        text_field(app, &d.name, spec.name, false, TextTarget::Name).with_id(ids::ADD_NAME),
    ];
    fields.extend(spec.fields.iter().map(|f| field(d, app, f)));
    if spec.read_only {
        fields.push(note(
            "AzDrive browses this source; it does not write to it.",
        ));
    }
    children.push(
        Dom::create_div()
            .with_css(
                "display: flex; flex-direction: column; max-height: 400px; overflow-y: auto; \
                 padding-right: 4px;",
            )
            .with_children(DomVec::from(fields)),
    );
    let status = if d.testing {
        Some((String::from("Testing the connection ..."), false))
    } else {
        match &d.tested {
            Some(Ok(text)) => Some((text.clone(), false)),
            Some(Err(text)) => Some((format!("The connection failed: {text}"), true)),
            None => None,
        }
    };
    if let Some((text, failed)) = status {
        let mut status = line(&text).with_id(ids::ADD_STATUS);
        if failed {
            status = status.with_css("color: #C42B1C;");
        }
        children.push(status);
    }
    if !d.error.is_empty() {
        children.push(error_line(&d.error));
    }
    let busy = d.testing.then_some("The connection test runs.");
    children.push(buttons(vec![
        button(
            app,
            "Test connection",
            ButtonType::Default,
            AddEvent::Test,
            ids::ADD_TEST,
            busy,
        ),
        button(
            app,
            "Cancel",
            ButtonType::Default,
            AddEvent::Cancel,
            ids::ADD_CANCEL,
            None,
        ),
        button(
            app,
            if d.editing.is_some() {
                "Save keys"
            } else {
                "Add drive"
            },
            ButtonType::Primary,
            AddEvent::Save,
            ids::ADD_SAVE,
            None,
        ),
    ]));
    column(children).with_id(ids::ADD_FORM)
}

/// The form's head: the source's icon, name and line.
fn header(spec: &ServiceSpec) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin-bottom: 4px;")
        .with_child(
            Dom::create_icon(AzString::from(spec.icon))
                .with_css("font-size: 28px; margin-right: 10px;"),
        )
        .with_child(column(vec![
            Dom::create_span_with_text(AzString::from(spec.name))
                .with_css("font-size: 15px; font-weight: bold;"),
            note(spec.summary),
        ]))
}

// ==== The callbacks ====

extern "C" fn on_event(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, event)) = data
        .downcast_ref::<EventRef>()
        .map(|r| (r.app.clone(), r.event.clone()))
    else {
        return Update::DoNothing;
    };
    everywhere(with_state(&mut app, &mut info, |info, app, s| {
        add_flow::event(info, app, s, event)
    }))
}

extern "C" fn on_yearly(data: RefAny, info: CallbackInfo, _state: CheckBoxState) -> Update {
    on_event(data, info)
}

/// A text field: the dialog keeps what it says (no rebuild: the field shows it already).
extern "C" fn on_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let Some((mut app, target)) = data
        .downcast_ref::<TextRef>()
        .map(|r| (r.app.clone(), r.target))
    else {
        return keep;
    };
    let Some(mut s) = app.downcast_mut::<DriveState>() else {
        return keep;
    };
    let text = state.get_text().as_str().to_string();
    if let Some(Popup::AddDrive(d)) = s.popup.as_mut() {
        match target {
            TextTarget::Name => d.set_name(&text),
            TextTarget::BuyName => d.buy_name = text,
            TextTarget::Field(key) => d.set_value(key, &text),
        }
    }
    keep
}

/// A switch of the form.
extern "C" fn on_check(mut data: RefAny, mut info: CallbackInfo, _state: CheckBoxState) -> Update {
    let Some((mut app, key)) = data
        .downcast_ref::<KeyRef>()
        .map(|r| (r.app.clone(), r.key))
    else {
        return Update::DoNothing;
    };
    everywhere(with_state(&mut app, &mut info, |_info, _app, s| {
        if let Some(Popup::AddDrive(d)) = s.popup.as_mut() {
            d.toggle(key);
        }
    }))
}

/// A choice of the form.
extern "C" fn on_choice(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some((mut app, key)) = data
        .downcast_ref::<KeyRef>()
        .map(|r| (r.app.clone(), r.key))
    else {
        return Update::DoNothing;
    };
    everywhere(with_state(&mut app, &mut info, |_info, _app, s| {
        if let Some(Popup::AddDrive(d)) = s.popup.as_mut() {
            d.choose(key, index);
        }
    }))
}

/// "Choose ...": the system's folder (or file) dialog fills a path field.
extern "C" fn on_choose_path(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((app, key, folder)) = data
        .downcast_ref::<KeyRef>()
        .map(|r| (r.app.clone(), r.key, r.folder))
    else {
        return Update::DoNothing;
    };
    let picker = RefAny::new(KeyRef { app, key, folder });
    if folder {
        let _request = FileDialog::open_directory(
            AzString::from("Choose a folder"),
            OptionString::None,
            picker,
            on_path_picked,
        );
    } else {
        let _request = FileDialog::open_file(
            AzString::from("Choose a database file"),
            OptionString::None,
            OptionFileTypeList::None,
            picker,
            on_path_picked,
        );
    }
    Update::DoNothing
}

/// The system's dialog answered: the path goes into its field.
extern "C" fn on_path_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let text = path.inner.as_str().to_string();
    let Some((mut app, key)) = data
        .downcast_ref::<KeyRef>()
        .map(|r| (r.app.clone(), r.key))
    else {
        return Update::DoNothing;
    };
    everywhere(with_state(&mut app, &mut info, |_info, _app, s| {
        if let Some(Popup::AddDrive(d)) = s.popup.as_mut() {
            d.set_value(key, &text);
        }
    }))
}

#[cfg(test)]
mod tests {
    use azcloud_kit::{Tier, Tiers};
    use azul::css::{CssDeclaration, CssProperty, StyleFontSizeValue};

    use super::*;

    /// The font size (px) `dom`'s own sheets give it - the app's `with_css`, never a widget's
    /// UA default (rule priority 0).
    fn own_font_size(dom: &Dom) -> Option<f32> {
        dom.css
            .as_slice()
            .iter()
            .flat_map(|css| css.rules.as_slice().iter())
            .filter(|rule| rule.priority > 0)
            .flat_map(|rule| rule.declarations.as_slice().iter())
            .filter_map(|declaration| match declaration {
                CssDeclaration::Static(CssProperty::FontSize(StyleFontSizeValue::Exact(size))) => {
                    Some(size.inner.number.get())
                }
                _ => None,
            })
            .next_back()
    }

    /// The size the text `text` is set in inside `dom`: the nearest of its boxes that sets one
    /// (`None`: nothing inside `dom` does, the text takes what is around the dialog). `None`
    /// outside: no such text.
    fn size_of_text(dom: &Dom, text: &str, around: Option<f32>) -> Option<Option<f32>> {
        let here = own_font_size(dom).or(around);
        if dom.root.node_type.get_text().into_option().is_some_and(|t| t.as_str() == text) {
            return Some(here);
        }
        dom.children.as_slice().iter().find_map(|child| size_of_text(child, text, here))
    }

    /// The box with the id `id` in `dom`.
    fn with_id<'a>(dom: &'a Dom, id: &AzString) -> Option<&'a Dom> {
        if dom.root.has_id(id.clone()) {
            return Some(dom);
        }
        dom.children.as_slice().iter().find_map(|child| with_id(child, id))
    }

    /// Buy storage with two tiers loaded.
    fn buy_page() -> Dom {
        let tier = |id: &str, gb: u64| Tier {
            id: id.to_string(),
            quota_bytes: gb * 1_000_000_000,
            price_cents_month: Some(99),
            price_cents_year: Some(990),
            currency: "EUR".to_string(),
            first_month_free: true,
        };
        let mut d = AddDialog::new(1);
        d.choose_buy();
        d.tiers = TiersState::Loaded(Tiers {
            tiers: vec![tier("100GB", 100), tier("1TB", 1000)],
            methods: vec!["sepa".to_string()],
            withdrawal_consent: None,
        });
        dialog(&d, false, &RefAny::new(())).1
    }

    /// The dialog kit's sizes (azul's wizard pages, settings rows and standard dialogs): text and
    /// field values 13px, hints 12px. The yearly label took the dialog panel's 14px and the name
    /// field its own 11px default.
    #[test]
    fn the_buy_pages_yearly_label_and_name_field_are_set_in_the_dialog_kits_text_size() {
        let page = buy_page();
        assert_eq!(
            size_of_text(&page, "Pay yearly (two months for free)", None),
            Some(Some(13.0)),
            "the yearly label: the dialog's text size"
        );
        let name = with_id(&page, &ids::ADD_NAME).expect("the name field");
        assert_eq!(own_font_size(name), Some(13.0), "the name: the dialog's text size");
    }
}
