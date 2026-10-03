//! The reading pane of the vault: the selected item (copy, reveal, the one-time code with its
//! seconds, Edit, Delete), the edit form, the generator, the import preview and the audit.

use zeroize::Zeroizing;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ChipOnRemoveCallbackType,
        SegmentedOnChangeCallbackType, SliderOnValueChangeCallbackType, SwitchOnToggleCallbackType,
        TextAreaOnTextInputCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType,
    },
    dialog::{FileDialog, FileOpenResult},
    dom::VirtualKeyCode,
    option::OptionFileTypeList,
    prelude::*,
    shells::ShellEmptyState,
    str::String as AzString,
    widgets::{
        Avatar, AvatarSize, CheckBoxState, Chip, ChipState, OnTextInputReturn, Segmented,
        SegmentedState, Slider, SliderState, Switch, SwitchState, TextArea, TextAreaState,
        TextInputState, TextInputValid,
    },
};

use crate::app::{now, KeysApp};
use crate::audit::{self, Filter};
use crate::generator::{self, Mode};
use crate::ids;
use crate::jobs;
use crate::session::{Form, ImportView, Reading, Reveal, Session};
use crate::store::{self, Work};
use crate::totp::{group_code, Totp};
use crate::ui::{
    block, button, column, form_row, icon_button, keep, leave_form, note, primary, problem, row,
    strength_bar, strs, text, with_app,
};
use crate::vault::{Field, Item, Kind};

/// The pane's column style.
const PANE: &str = "flex-grow: 1; min-height: 0px; overflow-y: auto; padding: 12px 16px;";

/// `YYYY-MM-DD` of seconds since 1970 ("" for 0).
fn date(unix: u64) -> String {
    if unix == 0 {
        return String::new();
    }
    azul_storage::time::iso8601(unix)
        .get(..10)
        .unwrap_or_default()
        .to_string()
}

/// The reading pane for the session's reading.
pub fn reading_pane(s: &KeysApp, session: &Session, app: &RefAny) -> Dom {
    match &session.reading {
        Reading::Edit(form) => edit_view(form, app),
        Reading::Generator => generator_view(session, app),
        Reading::Import(view) => import_view(view, app),
        Reading::Audit(filter) => audit_view(session, *filter, app),
        Reading::Item => match session.selected_item() {
            Some(item) => item_view(s, session, item, app),
            None => ShellEmptyState::create("No item selected")
                .with_icon("key")
                .with_detail("Pick an item in the list, or add a new one.")
                .dom(),
        },
    }
}

// ==== The item ====

struct CopyRef {
    app: RefAny,
    field: String,
}

struct RevealRef {
    app: RefAny,
    reveal: Reveal,
}

fn copy_button(app: &RefAny, field: &str, id: AzString) -> Dom {
    Button::create("Copy")
        .with_icon("content_copy")
        .with_on_click(
            RefAny::new(CopyRef {
                app: app.clone(),
                field: field.to_string(),
            }),
            on_copy as ButtonOnClickCallbackType,
        )
        .dom()
        .with_id(id)
}

fn reveal_button(app: &RefAny, reveal: Reveal, shown: bool, id: AzString) -> Dom {
    Button::create(if shown { "Hide" } else { "Show" })
        .with_icon(if shown {
            "visibility_off"
        } else {
            "visibility"
        })
        .with_on_click(
            RefAny::new(RevealRef {
                app: app.clone(),
                reveal,
            }),
            on_reveal as ButtonOnClickCallbackType,
        )
        .dom()
        .with_id(id)
}

/// A field of the item: the label, the value, the buttons.
fn field_row(label: &str, value: Dom, buttons: Vec<Dom>) -> Dom {
    row(
        "align-items: flex-start; padding: 4px 0px;",
        vec![
            block(
                "width: 100px; flex-shrink: 0; font-size: 12px; opacity: 0.7; padding-top: 4px;",
                text(label),
            ),
            block(
                "flex-grow: 1; min-width: 0px; font-size: 13px; padding-top: 4px;",
                value,
            ),
            row("gap: 4px; flex-shrink: 0;", buttons),
        ],
    )
}

/// A secret's dots, or its text while revealed.
fn secret(value: &str, shown: bool) -> Dom {
    if shown {
        text(value)
    } else {
        text("\u{2022}".repeat(value.chars().count().clamp(8, 16)))
    }
}

fn item_view(s: &KeysApp, session: &Session, item: &Item, app: &RefAny) -> Dom {
    let t = now();
    let revealed = |r: Reveal| session.reveal == Some(r);
    let mut subtitle = item.kind.singular().to_string();
    if !item.folder.is_empty() {
        subtitle.push_str(&format!(" \u{b7} {}", item.folder));
    }
    let header = row(
        "padding-bottom: 8px;",
        vec![
            Avatar::create(item.initials())
                .with_size(AvatarSize::Large)
                .dom(),
            column(
                "flex-grow: 1; padding-left: 12px; min-width: 0px;",
                vec![
                    block(
                        "font-size: 18px; font-weight: 600;",
                        text(item.title.as_str()),
                    )
                    .with_id(ids::ITEM_TITLE),
                    block("font-size: 12px; opacity: 0.7;", text(subtitle)),
                ],
            ),
            Button::create(if item.favorite {
                "\u{2605}"
            } else {
                "\u{2606}"
            })
            .with_toggled(item.favorite)
            .with_on_click(app.clone(), on_favorite as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::ITEM_FAVORITE),
        ],
    );
    let mut rows = vec![header];
    if !item.username.is_empty() {
        rows.push(
            field_row(
                "user name",
                text(item.username.as_str()),
                vec![copy_button(app, "username", ids::COPY_USERNAME)],
            )
            .with_id(ids::ITEM_USERNAME),
        );
    }
    if !item.password.is_empty() {
        let shown = revealed(Reveal::Password);
        rows.push(
            field_row(
                "password",
                secret(&item.password, shown),
                vec![
                    reveal_button(app, Reveal::Password, shown, ids::REVEAL_PASSWORD),
                    copy_button(app, "password", ids::COPY_PASSWORD),
                ],
            )
            .with_id(ids::ITEM_PASSWORD),
        );
        let bits = generator::estimate(&item.password);
        let mut words = generator::Strength::of_bits(bits).label().to_string();
        if item.password_changed > 0 {
            words.push_str(&format!(", changed {}", date(item.password_changed)));
        }
        rows.push(field_row("", note(&words), Vec::new()).with_id(ids::ITEM_STRENGTH));
    }
    if !item.totp.is_empty() {
        let value = match Totp::parse(&item.totp) {
            Ok(totp) => {
                let left = totp.remaining(t);
                let used = (totp.period - left) as f32 / totp.period as f32 * 100.0;
                row(
                    "gap: 8px;",
                    vec![
                        block(
                            "font-size: 18px; font-family: monospace;",
                            text(group_code(&totp.code_at(t))),
                        )
                        .with_id(ids::ITEM_TOTP_CODE),
                        // TODO(WIDGETS9B): Gauge - a draining ring in place of the bar.
                        block("width: 60px;", ProgressBar::create(100.0 - used).dom())
                            .with_id(ids::ITEM_TOTP_RING),
                        block("font-size: 12px; opacity: 0.7;", text(format!("{left} s"))),
                    ],
                )
            }
            Err(why) => block("font-size: 12px; color: #d13438;", text(why)),
        };
        rows.push(field_row(
            "one-time",
            value,
            vec![copy_button(app, "totp", ids::COPY_TOTP)],
        ));
    }
    if item.kind == Kind::Card {
        let card = &item.card;
        if !card.holder.is_empty() {
            rows.push(field_row("holder", text(card.holder.as_str()), Vec::new()));
        }
        if !card.brand.is_empty() {
            rows.push(field_row("brand", text(card.brand.as_str()), Vec::new()));
        }
        if !card.number.is_empty() {
            let shown = revealed(Reveal::CardNumber);
            let value = if shown {
                text(card.number.as_str())
            } else {
                text(card.masked_number())
            };
            rows.push(field_row(
                "number",
                value,
                vec![
                    reveal_button(app, Reveal::CardNumber, shown, ids::reveal_field(1000)),
                    copy_button(app, "card-number", ids::COPY_CARD_NUMBER),
                ],
            ));
        }
        if !card.expiry.is_empty() {
            rows.push(field_row("expires", text(card.expiry.as_str()), Vec::new()));
        }
        if !card.code.is_empty() {
            let shown = revealed(Reveal::CardCode);
            rows.push(field_row(
                "security code",
                secret(&card.code, shown),
                vec![
                    reveal_button(app, Reveal::CardCode, shown, ids::reveal_field(1001)),
                    copy_button(app, "card-code", ids::COPY_CARD_CODE),
                ],
            ));
        }
    }
    for (n, url) in item.urls.iter().enumerate() {
        rows.push(
            field_row(
                if n == 0 { "website" } else { "" },
                text(url.as_str()),
                Vec::new(),
            )
            .with_id(ids::ITEM_WEBSITE),
        );
    }
    for (n, f) in item.fields.iter().enumerate() {
        let shown = !f.hidden || revealed(Reveal::Field(n));
        let mut buttons = Vec::new();
        if f.hidden {
            buttons.push(reveal_button(
                app,
                Reveal::Field(n),
                shown,
                ids::reveal_field(n),
            ));
        }
        buttons.push(copy_button(app, &format!("field-{n}"), ids::copy_field(n)));
        rows.push(field_row(&f.name, secret(&f.value, shown), buttons));
    }
    if !item.tags.is_empty() {
        let chips: Vec<Dom> = item
            .tags
            .iter()
            .map(|t| Chip::create(t.as_str()).dom())
            .collect();
        rows.push(field_row(
            "tags",
            row("gap: 4px; flex-wrap: wrap;", chips),
            Vec::new(),
        ));
    }
    if !item.notes.is_empty() {
        let lines: Vec<Dom> = item
            .notes
            .lines()
            .map(|l| Dom::create_div().with_child(text(l)))
            .collect();
        rows.push(field_row("notes", column("", lines), Vec::new()).with_id(ids::ITEM_NOTES));
    }
    if !item.history.is_empty() {
        rows.push(
            field_row(
                "history",
                note(&format!(
                    "{} earlier password{} (the last until {})",
                    item.history.len(),
                    if item.history.len() == 1 { "" } else { "s" },
                    date(item.history[0].until)
                )),
                Vec::new(),
            )
            .with_id(ids::ITEM_HISTORY),
        );
    }
    rows.push(note(&format!(
        "Created {}, changed {}",
        date(item.created),
        date(item.modified)
    )));
    let actions = if session.confirm_delete {
        row(
            "gap: 8px; padding-top: 12px;",
            vec![
                block(
                    "font-size: 13px;",
                    text(format!("Delete \u{201c}{}\u{201d}?", item.title)),
                ),
                primary("Delete", ids::ITEM_DELETE_CONFIRM, app, on_delete_confirmed),
                button("Keep it", ids::ITEM_DELETE_CANCEL, app, on_delete_cancelled),
            ],
        )
    } else {
        row(
            "gap: 8px; padding-top: 12px;",
            vec![
                icon_button("Edit", "edit", ids::ITEM_EDIT, app, on_edit),
                icon_button("Delete", "delete", ids::ITEM_DELETE, app, on_delete),
            ],
        )
    };
    rows.push(actions);
    let _ = s;
    column(PANE, rows).with_id(ids::ITEM)
}

extern "C" fn on_copy(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, field)) = data
        .downcast_ref::<CopyRef>()
        .map(|r| (r.app.clone(), r.field.clone()))
    else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, _| {
        jobs::copy_secret(s, info, &field)
    })
}

extern "C" fn on_reveal(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, reveal)) = data
        .downcast_ref::<RevealRef>()
        .map(|r| (r.app.clone(), r.reveal))
    else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        if let Some(session) = s.session.as_mut() {
            session.reveal = if session.reveal == Some(reveal) {
                None
            } else {
                Some(reveal)
            };
        }
    })
}

extern "C" fn on_favorite(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, app| {
        if let Some(session) = s.session.as_mut() {
            session.toggle_favorite(now());
        }
        jobs::save(s, info, app);
    })
}

/// Edit the selected item.
pub extern "C" fn on_edit(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        let Some(session) = s.session.as_mut() else {
            return;
        };
        if matches!(session.reading, Reading::Edit(_)) {
            return;
        }
        if let Some(item) = session.selected_item() {
            let form = Form::edit(item);
            session.reading = Reading::Edit(form);
        }
    })
}

/// "Delete" (or the Delete key): asks first.
pub extern "C" fn on_delete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        if let Some(session) = s.session.as_mut() {
            if matches!(session.reading, Reading::Item) && session.selected.is_some() {
                session.confirm_delete = true;
            }
        }
    })
}

extern "C" fn on_delete_confirmed(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, app| {
        if let Some(session) = s.session.as_mut() {
            if session.delete_selected(now()) {
                println!("AZKEYS_DELETED");
            }
        }
        jobs::save(s, info, app);
    })
}

extern "C" fn on_delete_cancelled(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        if let Some(session) = s.session.as_mut() {
            session.confirm_delete = false;
        }
    })
}

/// Escape: a question goes, an unchanged form or a panel closes (a changed form asks).
pub extern "C" fn on_escape(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        let Some(session) = s.session.as_mut() else {
            return;
        };
        if session.confirm_delete {
            session.confirm_delete = false;
            return;
        }
        if session.reveal.is_some() {
            session.reveal = None;
            return;
        }
        let editing = match &mut session.reading {
            Reading::Edit(form) if form.confirm_discard => {
                form.confirm_discard = false;
                return;
            }
            Reading::Edit(_) => true,
            _ => false,
        };
        if !editing || leave_form(session) {
            session.reading = Reading::Item;
        }
    })
}

// ==== The edit form ====

/// A text field of the edit form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditField {
    Title,
    Username,
    Password,
    Totp,
    Website,
    CardHolder,
    CardBrand,
    CardNumber,
    CardExpiry,
    CardCode,
    Tag,
    FieldName(usize),
    FieldValue(usize),
}

struct EditRef {
    app: RefAny,
    field: EditField,
}

struct IndexRef {
    app: RefAny,
    index: usize,
}

fn edit_input(
    app: &RefAny,
    field: EditField,
    value: &str,
    placeholder: &str,
    password: bool,
    id: AzString,
) -> Dom {
    let input = if password {
        TextInput::create_password()
    } else {
        TextInput::create()
    };
    let mut input = input
        .with_text(value)
        .with_placeholder(placeholder)
        .with_accessibility_name(placeholder)
        .with_on_text_input(
            RefAny::new(EditRef {
                app: app.clone(),
                field,
            }),
            on_edit_text as TextInputOnTextInputCallbackType,
        );
    if field == EditField::Tag {
        input = input.with_on_virtual_key_down(
            RefAny::new(EditRef {
                app: app.clone(),
                field,
            }),
            on_tag_key as TextInputOnVirtualKeyDownCallbackType,
        );
    }
    input.dom().with_id(id)
}

/// The kinds a new item can be, in the order of the switch.
const KIND_LABELS: [&str; 5] = ["Login", "Card", "Secure note", "Identity", "SSH key"];

fn edit_view(form: &Form, app: &RefAny) -> Dom {
    let d = &form.draft;
    let mut rows = vec![block(
        "font-size: 16px; font-weight: 600; padding-bottom: 8px;",
        text(if form.original.is_some() {
            format!("Edit {}", d.kind.singular())
        } else {
            format!("New {}", d.kind.singular())
        }),
    )];
    if form.original.is_none() {
        rows.push(form_row(
            "Kind",
            Segmented::create(strs(&KIND_LABELS))
                .with_selected_index(d.kind.index())
                .with_on_change(app.clone(), on_kind as SegmentedOnChangeCallbackType)
                .dom(),
        ));
    }
    rows.push(form_row(
        "Title",
        edit_input(
            app,
            EditField::Title,
            &d.title,
            "Title",
            false,
            ids::EDIT_TITLE,
        ),
    ));
    if matches!(d.kind, Kind::Login | Kind::Identity | Kind::SshKey) || !d.username.is_empty() {
        rows.push(form_row(
            "User name",
            edit_input(
                app,
                EditField::Username,
                &d.username,
                "User name or email",
                false,
                ids::EDIT_USERNAME,
            ),
        ));
    }
    if d.kind == Kind::Login || !d.password.is_empty() {
        rows.push(form_row(
            "Password",
            row(
                "gap: 4px;",
                vec![
                    block(
                        "flex-grow: 1; min-width: 0px;",
                        edit_input(
                            app,
                            EditField::Password,
                            &d.password,
                            "Password",
                            !form.reveal,
                            ids::EDIT_PASSWORD,
                        ),
                    ),
                    Button::create(if form.reveal { "Hide" } else { "Show" })
                        .with_on_click(app.clone(), on_form_reveal as ButtonOnClickCallbackType)
                        .dom(),
                    icon_button(
                        "Generate",
                        "casino",
                        ids::EDIT_GENERATE,
                        app,
                        on_form_generate,
                    ),
                ],
            ),
        ));
        rows.push(form_row(
            "",
            strength_bar(&d.password, ids::EDIT_PASSWORD_STRENGTH),
        ));
    }
    if d.kind == Kind::Login || !d.totp.is_empty() {
        rows.push(form_row(
            "One-time code",
            edit_input(
                app,
                EditField::Totp,
                &d.totp,
                "otpauth://totp/... or the base32 secret",
                false,
                ids::EDIT_TOTP,
            ),
        ));
        if !form.totp_problem.is_empty() {
            rows.push(form_row(
                "",
                problem(&form.totp_problem, ids::EDIT_TOTP_PROBLEM),
            ));
        }
    }
    if d.kind == Kind::Login || !d.urls.is_empty() {
        let website = d.urls.first().map(String::as_str).unwrap_or_default();
        rows.push(form_row(
            "Website",
            edit_input(
                app,
                EditField::Website,
                website,
                "https://",
                false,
                ids::EDIT_WEBSITE,
            ),
        ));
    }
    if d.kind == Kind::Card {
        let c = &d.card;
        rows.push(form_row(
            "Holder",
            edit_input(
                app,
                EditField::CardHolder,
                &c.holder,
                "Name on the card",
                false,
                ids::EDIT_CARD_HOLDER,
            ),
        ));
        rows.push(form_row(
            "Brand",
            edit_input(
                app,
                EditField::CardBrand,
                &c.brand,
                "Visa, Mastercard, ...",
                false,
                ids::EDIT_CARD_BRAND,
            ),
        ));
        rows.push(form_row(
            "Number",
            edit_input(
                app,
                EditField::CardNumber,
                &c.number,
                "Card number",
                false,
                ids::EDIT_CARD_NUMBER,
            ),
        ));
        rows.push(form_row(
            "Expires",
            edit_input(
                app,
                EditField::CardExpiry,
                &c.expiry,
                "MM/YYYY",
                false,
                ids::EDIT_CARD_EXPIRY,
            ),
        ));
        rows.push(form_row(
            "Security code",
            edit_input(
                app,
                EditField::CardCode,
                &c.code,
                "CVC",
                true,
                ids::EDIT_CARD_CODE,
            ),
        ));
    }
    for (n, f) in d.fields.iter().enumerate() {
        rows.push(form_row(
            "Field",
            row(
                "gap: 4px;",
                vec![
                    block(
                        "width: 120px; flex-shrink: 0;",
                        edit_input(
                            app,
                            EditField::FieldName(n),
                            &f.name,
                            "Name",
                            false,
                            ids::edit_field_name(n),
                        ),
                    ),
                    block(
                        "flex-grow: 1; min-width: 0px;",
                        edit_input(
                            app,
                            EditField::FieldValue(n),
                            &f.value,
                            "Value",
                            f.hidden,
                            ids::edit_field_value(n),
                        ),
                    ),
                    Switch::create(f.hidden)
                        .with_accessibility_name("Hidden")
                        .with_on_toggle(
                            RefAny::new(IndexRef {
                                app: app.clone(),
                                index: n,
                            }),
                            on_field_hidden as SwitchOnToggleCallbackType,
                        )
                        .dom(),
                    Button::create("")
                        .with_icon("remove_circle_outline")
                        .with_on_click(
                            RefAny::new(IndexRef {
                                app: app.clone(),
                                index: n,
                            }),
                            on_field_remove as ButtonOnClickCallbackType,
                        )
                        .dom()
                        .with_id(ids::edit_field_remove(n)),
                ],
            ),
        ));
    }
    rows.push(form_row(
        "",
        icon_button("Add a field", "add", ids::EDIT_ADD_FIELD, app, on_field_add),
    ));
    let mut chips: Vec<Dom> = d
        .tags
        .iter()
        .enumerate()
        .map(|(n, t)| {
            Chip::create(t.as_str())
                .with_removable(true)
                .with_on_remove(
                    RefAny::new(IndexRef {
                        app: app.clone(),
                        index: n,
                    }),
                    on_tag_remove as ChipOnRemoveCallbackType,
                )
                .dom()
                .with_id(ids::edit_tag_chip(n))
        })
        .collect();
    // TODO(WIDGETS9A): TokenInput - the chips and the field as one control.
    chips.push(block(
        "flex-grow: 1; min-width: 120px;",
        edit_input(
            app,
            EditField::Tag,
            &form.tag,
            "Add a tag (Enter)",
            false,
            ids::EDIT_TAG,
        ),
    ));
    rows.push(form_row("Tags", row("gap: 4px; flex-wrap: wrap;", chips)));
    rows.push(form_row(
        "Notes",
        TextArea::create()
            .with_text(d.notes.as_str())
            .with_placeholder("Notes")
            .with_accessibility_name("Notes")
            .with_on_text_input(app.clone(), on_notes as TextAreaOnTextInputCallbackType)
            .dom()
            .with_id(ids::EDIT_NOTES),
    ));
    rows.push(form_row(
        "Favourite",
        Switch::create(d.favorite)
            .with_accessibility_name("Favourite")
            .with_on_toggle(app.clone(), on_form_favorite as SwitchOnToggleCallbackType)
            .dom()
            .with_id(ids::EDIT_FAVORITE),
    ));
    if form.confirm_discard {
        rows.push(row(
            "gap: 8px; padding-top: 12px;",
            vec![
                block("font-size: 13px;", text("Discard your changes?")),
                primary("Discard", ids::EDIT_DISCARD, app, on_form_discard),
                button("Keep editing", ids::EDIT_KEEP, app, on_form_keep),
            ],
        ));
    } else {
        rows.push(row(
            "gap: 8px; padding: 12px 0px 0px 110px;",
            vec![
                primary("Save", ids::EDIT_SAVE, app, on_edit_save),
                button("Cancel", ids::EDIT_CANCEL, app, on_form_cancel),
            ],
        ));
    }
    column(PANE, rows).with_id(ids::EDIT)
}

/// Runs `f` on the edit form (when one is open); the window is rebuilt.
fn with_form(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut Form, &generator::Options),
) -> Update {
    with_app(data, info, |s, _info, _| {
        if let Some(session) = s.session.as_mut() {
            let options = session.generator.clone();
            if let Reading::Edit(form) = &mut session.reading {
                f(form, &options);
            }
        }
    })
}

fn set_text(form: &mut Form, field: EditField, value: String) {
    let d = &mut form.draft;
    match field {
        EditField::Title => d.title = value,
        EditField::Username => d.username = value,
        EditField::Password => d.password = value,
        EditField::Totp => form.set_totp(&value),
        EditField::Website => match d.urls.first_mut() {
            Some(first) => *first = value,
            None if !value.trim().is_empty() => d.urls.push(value),
            None => {}
        },
        EditField::CardHolder => d.card.holder = value,
        EditField::CardBrand => d.card.brand = value,
        EditField::CardNumber => d.card.number = value,
        EditField::CardExpiry => d.card.expiry = value,
        EditField::CardCode => d.card.code = value,
        EditField::Tag => {
            form.tag = value;
            if form.tag.contains(',') {
                form.commit_tag();
            }
        }
        EditField::FieldName(n) => {
            if let Some(f) = d.fields.get_mut(n) {
                f.name = value;
            }
        }
        EditField::FieldValue(n) => {
            if let Some(f) = d.fields.get_mut(n) {
                f.value = value;
            }
        }
    }
}

/// A field of the form was typed in. The draft takes the text; the window is rebuilt only for
/// what shows it elsewhere (the strength bar, the one-time field's problem, a committed tag).
extern "C" fn on_edit_text(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let Some((mut app, field)) = data
        .downcast_ref::<EditRef>()
        .map(|r| (r.app.clone(), r.field))
    else {
        return keep();
    };
    let value = Zeroizing::new(state.get_text().as_str().to_string());
    let rebuild = matches!(field, EditField::Password | EditField::Totp)
        || (field == EditField::Tag && value.contains(','));
    let update = with_form(&mut app, &mut info, |form, _| {
        set_text(form, field, value.to_string())
    });
    OnTextInputReturn {
        update: if rebuild { update } else { Update::DoNothing },
        valid: TextInputValid::Yes,
    }
}

/// Enter in the tag field adds the tag.
extern "C" fn on_tag_key(
    mut data: RefAny,
    mut info: CallbackInfo,
    _state: TextInputState,
) -> OnTextInputReturn {
    let enter = matches!(
        info.get_current_keyboard_state()
            .current_virtual_keycode
            .into_option(),
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter)
    );
    if !enter {
        return keep();
    }
    let Some(mut app) = data.downcast_ref::<EditRef>().map(|r| r.app.clone()) else {
        return keep();
    };
    OnTextInputReturn {
        update: with_form(&mut app, &mut info, |form, _| form.commit_tag()),
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_notes(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextAreaState,
) -> OnTextInputReturn {
    let notes = state.get_text().as_str().to_string();
    let _ = with_form(&mut data, &mut info, |form, _| form.draft.notes = notes);
    keep()
}

extern "C" fn on_kind(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    with_form(&mut data, &mut info, |form, _| {
        if form.original.is_none() {
            form.draft.kind = Kind::ALL[state.selected_index.min(Kind::ALL.len() - 1)];
        }
    })
}

extern "C" fn on_form_reveal(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_form(&mut data, &mut info, |form, _| form.reveal = !form.reveal)
}

/// "Generate": a password by the generator's settings goes into the field (shown).
extern "C" fn on_form_generate(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_form(&mut data, &mut info, |form, options| {
        let mut options = options.clone();
        if options.mode == Mode::Passphrase && form.draft.kind == Kind::Card {
            options.mode = Mode::Pin;
        }
        if let Ok(password) = generator::generate(&options) {
            form.draft.password = password;
            form.reveal = true;
        }
    })
}

extern "C" fn on_form_favorite(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SwitchState,
) -> Update {
    with_form(&mut data, &mut info, |form, _| {
        form.draft.favorite = state.checked
    })
}

extern "C" fn on_field_add(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_form(&mut data, &mut info, |form, _| {
        form.draft.fields.push(Field::default())
    })
}

extern "C" fn on_field_remove(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data
        .downcast_ref::<IndexRef>()
        .map(|r| (r.app.clone(), r.index))
    else {
        return Update::DoNothing;
    };
    with_form(&mut app, &mut info, |form, _| {
        if index < form.draft.fields.len() {
            form.draft.fields.remove(index);
        }
    })
}

extern "C" fn on_field_hidden(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SwitchState,
) -> Update {
    let Some((mut app, index)) = data
        .downcast_ref::<IndexRef>()
        .map(|r| (r.app.clone(), r.index))
    else {
        return Update::DoNothing;
    };
    with_form(&mut app, &mut info, |form, _| {
        if let Some(f) = form.draft.fields.get_mut(index) {
            f.hidden = state.checked;
        }
    })
}

extern "C" fn on_tag_remove(mut data: RefAny, mut info: CallbackInfo, _state: ChipState) -> Update {
    let Some((mut app, index)) = data
        .downcast_ref::<IndexRef>()
        .map(|r| (r.app.clone(), r.index))
    else {
        return Update::DoNothing;
    };
    with_form(&mut app, &mut info, |form, _| {
        if index < form.draft.tags.len() {
            form.draft.tags.remove(index);
        }
    })
}

/// "Save" (or Mod+S): the draft goes into the vault and the vault is saved.
pub extern "C" fn on_edit_save(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, app| {
        let Some(session) = s.session.as_mut() else {
            return;
        };
        if !matches!(session.reading, Reading::Edit(_)) {
            return;
        }
        match session.save_form(now()) {
            Ok(_) => {
                if let Some(item) = session.selected_item() {
                    println!("AZKEYS_ITEM_SAVED {}", item.title);
                }
                s.notice.clear();
                jobs::save(s, info, app);
            }
            Err(problem) => s.notice = problem,
        }
    })
}

extern "C" fn on_form_cancel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        if let Some(session) = s.session.as_mut() {
            if leave_form(session) {
                session.reading = Reading::Item;
            }
        }
    })
}

extern "C" fn on_form_discard(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        if let Some(session) = s.session.as_mut() {
            session.reading = Reading::Item;
        }
    })
}

extern "C" fn on_form_keep(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_form(&mut data, &mut info, |form, _| form.confirm_discard = false)
}
