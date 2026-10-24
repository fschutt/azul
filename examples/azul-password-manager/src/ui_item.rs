//! The reading pane of the vault: the selected item (copy, reveal, the one-time code with its
//! seconds, Edit, Delete), the edit form, the generator, the import preview and the audit.

use zeroize::Zeroizing;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, SegmentedOnChangeCallbackType,
        SliderOnValueChangeCallbackType, SwitchOnToggleCallbackType,
        TextAreaOnTextInputCallbackType, TextInputOnTextInputCallbackType,
        TokenInputOnEventCallbackType,
    },
    dialog::{FileDialog, FileOpenResult},
    option::OptionFileTypeList,
    prelude::*,
    shells::ShellEmptyState,
    str::String as AzString,
    widgets::{
        Avatar, AvatarSize, CheckBoxState, Chip, OnTextInputReturn, Segmented, SegmentedState,
        Slider, SliderState, Switch, SwitchState, TextArea, TextAreaState, TextInputState,
        TextInputValid, TokenInput, TokenInputEvent, Gauge, GaugeBand, GaugeBandKind, GaugeKind,
    },
};

use azul_appkit::{
    l10n::{label, t, t_args, t_text, Arg, Text},
    pieces::Clickable,
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
    block, button, column, flex_row, form_row, icon_button, keep, leave_form, note, primary,
    problem, strength_bar, strs, text, with_app,
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
        Reading::Edit(form) => edit_view(form, session, app),
        Reading::Generator => generator_view(session, app),
        Reading::Import(view) => import_view(view, app),
        Reading::Audit(filter) => audit_view(session, *filter, app),
        Reading::Item => match session.selected_item() {
            Some(item) => item_view(s, session, item, app),
            None => ShellEmptyState::create(t("azpasswordmanager-no-item"))
                .with_icon("key")
                .with_detail(t("azpasswordmanager-no-item-detail"))
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
    Button::create(t("azpasswordmanager-copy"))
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
    Button::create(t(if shown { "azpasswordmanager-hide" } else { "azpasswordmanager-show" }))
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

/// A field of the item: the label (said already, or a custom field's own name), the value, the
/// buttons.
fn field_row(label: &str, value: Dom, buttons: Vec<Dom>) -> Dom {
    flex_row(
        "align-items: flex-start; padding: 4px 0px;",
        vec![
            block(
                &format!(
                    "width: 100px; flex-shrink: 0; font-size: 12px; opacity: 0.7; padding-top: \
                     4px; {}",
                    crate::ui::QUIET_FLORA
                ),
                text(label),
            ),
            block(
                "flex-grow: 1; min-width: 0px; font-size: 13px; padding-top: 4px;",
                value,
            ),
            flex_row("gap: 4px; flex-shrink: 0;", buttons),
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
    let at = now();
    let revealed = |r: Reveal| session.reveal == Some(r);
    let mut subtitle = crate::l10n::kind(item.kind);
    if !item.folder.is_empty() {
        subtitle.push_str(&format!(" \u{b7} {}", item.folder));
    }
    let header = flex_row(
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
                    block(
                        &format!("font-size: 12px; opacity: 0.7; {}", crate::ui::QUIET_FLORA),
                        text(subtitle),
                    ),
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
                &t("azpasswordmanager-field-username"),
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
                &t("azpasswordmanager-field-password"),
                secret(&item.password, shown),
                vec![
                    reveal_button(app, Reveal::Password, shown, ids::REVEAL_PASSWORD),
                    copy_button(app, "password", ids::COPY_PASSWORD),
                ],
            )
            .with_id(ids::ITEM_PASSWORD),
        );
        let bits = generator::estimate(&item.password);
        let strength = crate::l10n::strength(generator::Strength::of_bits(bits));
        let words = if item.password_changed > 0 {
            t_args(
                "azpasswordmanager-strength-changed",
                &[
                    ("strength", Arg::from(strength)),
                    ("date", Arg::from(date(item.password_changed))),
                ],
            )
        } else {
            strength
        };
        rows.push(field_row("", note(&words), Vec::new()).with_id(ids::ITEM_STRENGTH));
    }
    if !item.totp.is_empty() {
        let value = match Totp::parse(&item.totp) {
            Ok(totp) => {
                let left = totp.remaining(at);
                #[allow(clippy::cast_precision_loss)] // seconds
                let (left_s, period_s) = (left as f64, totp.period.max(1) as f64);
                flex_row(
                    "gap: 8px;",
                    vec![
                        block(
                            "font-size: 18px; font-family: monospace;",
                            text(group_code(&totp.code_at(at))),
                        )
                        .with_id(ids::ITEM_TOTP_CODE),
                        // A draining ring with the seconds left in it; the last five warn.
                        Gauge::create(left_s, 0.0, period_s)
                            .with_kind(GaugeKind::Ring)
                            .with_size(40.0)
                            .with_thickness(4.0)
                            .with_value_text(format!("{left}"))
                            .with_accessibility_name(t("azpasswordmanager-seconds-to-next-code"))
                            .with_band(GaugeBand::create(0.0, 5.0, GaugeBandKind::Warn))
                            .dom()
                            .with_id(ids::ITEM_TOTP_RING),
                    ],
                )
            }
            Err(why) => block(
                &format!("font-size: 12px; {}", crate::ui::PROBLEM_INK),
                text(t(&why)),
            ),
        };
        rows.push(field_row(
            &t("azpasswordmanager-field-one-time"),
            value,
            vec![copy_button(app, "totp", ids::COPY_TOTP)],
        ));
    }
    if item.kind == Kind::Card {
        let card = &item.card;
        if !card.holder.is_empty() {
            rows.push(field_row(
                &t("azpasswordmanager-field-holder"),
                text(card.holder.as_str()),
                Vec::new(),
            ));
        }
        if !card.brand.is_empty() {
            rows.push(field_row(
                &t("azpasswordmanager-field-brand"),
                text(card.brand.as_str()),
                Vec::new(),
            ));
        }
        if !card.number.is_empty() {
            let shown = revealed(Reveal::CardNumber);
            let value = if shown {
                text(card.number.as_str())
            } else {
                text(card.masked_number())
            };
            rows.push(field_row(
                &t("azpasswordmanager-field-number"),
                value,
                vec![
                    reveal_button(app, Reveal::CardNumber, shown, ids::reveal_field(1000)),
                    copy_button(app, "card-number", ids::COPY_CARD_NUMBER),
                ],
            ));
        }
        if !card.expiry.is_empty() {
            rows.push(field_row(
                &t("azpasswordmanager-field-expires"),
                text(card.expiry.as_str()),
                Vec::new(),
            ));
        }
        if !card.code.is_empty() {
            let shown = revealed(Reveal::CardCode);
            rows.push(field_row(
                &t("azpasswordmanager-field-security-code"),
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
                &if n == 0 {
                    t("azpasswordmanager-field-website")
                } else {
                    String::new()
                },
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
            &t("azpasswordmanager-field-tags"),
            flex_row("gap: 4px; flex-wrap: wrap;", chips),
            Vec::new(),
        ));
    }
    if !item.notes.is_empty() {
        let lines: Vec<Dom> = item
            .notes
            .lines()
            .map(|l| Dom::create_div().with_child(text(l)))
            .collect();
        rows.push(
            field_row(&t("azpasswordmanager-field-notes"), column("", lines), Vec::new())
                .with_id(ids::ITEM_NOTES),
        );
    }
    if !item.history.is_empty() {
        rows.push(
            field_row(
                &t("azpasswordmanager-field-history"),
                note(&t_args(
                    "azpasswordmanager-history",
                    &[
                        ("count", Arg::from(item.history.len())),
                        ("date", Arg::from(date(item.history[0].until))),
                    ],
                )),
                Vec::new(),
            )
            .with_id(ids::ITEM_HISTORY),
        );
    }
    rows.push(note(&t_args(
        "azpasswordmanager-created-changed",
        &[
            ("created", Arg::from(date(item.created))),
            ("changed", Arg::from(date(item.modified))),
        ],
    )));
    let actions = if session.confirm_delete {
        flex_row(
            "gap: 8px; padding-top: 12px;",
            vec![
                block(
                    "font-size: 13px;",
                    text(t_args(
                        "azpasswordmanager-delete-ask",
                        &[("title", Arg::from(&item.title))],
                    )),
                ),
                primary(
                    "azpasswordmanager-delete",
                    ids::ITEM_DELETE_CONFIRM,
                    app,
                    on_delete_confirmed,
                ),
                button(
                    "azpasswordmanager-keep-it",
                    ids::ITEM_DELETE_CANCEL,
                    app,
                    on_delete_cancelled,
                ),
            ],
        )
    } else {
        flex_row(
            "gap: 8px; padding-top: 12px;",
            vec![
                icon_button("azpasswordmanager-edit", "edit", ids::ITEM_EDIT, app, on_edit),
                icon_button("azpasswordmanager-delete", "delete", ids::ITEM_DELETE, app, on_delete),
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
                println!("AZPASSWORDMANAGER_DELETED");
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
    input
        .with_text(value)
        .with_placeholder(label(placeholder))
        .with_accessibility_name(label(placeholder))
        .with_on_text_input(
            RefAny::new(EditRef {
                app: app.clone(),
                field,
            }),
            on_edit_text as TextInputOnTextInputCallbackType,
        )
        .dom()
        .with_id(id)
}

fn edit_view(form: &Form, session: &Session, app: &RefAny) -> Dom {
    let d = &form.draft;
    let mut rows = vec![block(
        "font-size: 16px; font-weight: 600; padding-bottom: 8px;",
        text(crate::l10n::form_title(d.kind, form.original.is_none())),
    )];
    if form.original.is_none() {
        // The kinds a new item can be, in the order of the switch.
        let kinds: Vec<&str> = Kind::ALL
            .iter()
            .map(|k| crate::l10n::kind_choice(*k))
            .collect();
        rows.push(form_row(
            "azpasswordmanager-kind-label",
            Segmented::create(strs(&kinds))
                .with_selected_index(d.kind.index())
                .with_on_change(app.clone(), on_kind as SegmentedOnChangeCallbackType)
                .dom(),
        ));
    }
    rows.push(form_row(
        "azpasswordmanager-title",
        edit_input(
            app,
            EditField::Title,
            &d.title,
            "azpasswordmanager-title",
            false,
            ids::EDIT_TITLE,
        ),
    ));
    if matches!(d.kind, Kind::Login | Kind::Identity | Kind::SshKey) || !d.username.is_empty() {
        rows.push(form_row(
            "azpasswordmanager-username",
            edit_input(
                app,
                EditField::Username,
                &d.username,
                "azpasswordmanager-username-placeholder",
                false,
                ids::EDIT_USERNAME,
            ),
        ));
    }
    if d.kind == Kind::Login || !d.password.is_empty() {
        rows.push(form_row(
            "azpasswordmanager-password",
            flex_row(
                "gap: 4px;",
                vec![
                    block(
                        "flex-grow: 1; min-width: 0px;",
                        edit_input(
                            app,
                            EditField::Password,
                            &d.password,
                            "azpasswordmanager-password",
                            !form.reveal,
                            ids::EDIT_PASSWORD,
                        ),
                    ),
                    Button::create(t(if form.reveal {
                        "azpasswordmanager-hide"
                    } else {
                        "azpasswordmanager-show"
                    }))
                    .with_on_click(app.clone(), on_form_reveal as ButtonOnClickCallbackType)
                    .dom(),
                    icon_button(
                        "azpasswordmanager-generate",
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
            "azpasswordmanager-one-time-code",
            edit_input(
                app,
                EditField::Totp,
                &d.totp,
                "azpasswordmanager-totp-placeholder",
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
            "azpasswordmanager-website",
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
            "azpasswordmanager-holder",
            edit_input(
                app,
                EditField::CardHolder,
                &c.holder,
                "azpasswordmanager-holder-placeholder",
                false,
                ids::EDIT_CARD_HOLDER,
            ),
        ));
        rows.push(form_row(
            "azpasswordmanager-brand",
            edit_input(
                app,
                EditField::CardBrand,
                &c.brand,
                "azpasswordmanager-brand-placeholder",
                false,
                ids::EDIT_CARD_BRAND,
            ),
        ));
        rows.push(form_row(
            "azpasswordmanager-number",
            edit_input(
                app,
                EditField::CardNumber,
                &c.number,
                "azpasswordmanager-card-number",
                false,
                ids::EDIT_CARD_NUMBER,
            ),
        ));
        rows.push(form_row(
            "azpasswordmanager-expires",
            edit_input(
                app,
                EditField::CardExpiry,
                &c.expiry,
                "azpasswordmanager-expiry-placeholder",
                false,
                ids::EDIT_CARD_EXPIRY,
            ),
        ));
        rows.push(form_row(
            "azpasswordmanager-security-code",
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
            "azpasswordmanager-field",
            flex_row(
                "gap: 4px;",
                vec![
                    block(
                        "width: 120px; flex-shrink: 0;",
                        edit_input(
                            app,
                            EditField::FieldName(n),
                            &f.name,
                            "azpasswordmanager-name",
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
                            "azpasswordmanager-value",
                            f.hidden,
                            ids::edit_field_value(n),
                        ),
                    ),
                    Switch::create(f.hidden)
                        .with_accessibility_name(t("azpasswordmanager-hidden"))
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
        icon_button(
            "azpasswordmanager-add-field",
            "add",
            ids::EDIT_ADD_FIELD,
            app,
            on_field_add,
        ),
    ));
    // The tags: azul's TokenInput (the chips and the entry as one control), the vault's
    // other tags as suggestions.
    let tags: Vec<&str> = d.tags.iter().map(String::as_str).collect();
    let vault_tags: Vec<String> = session
        .open
        .vault
        .tags()
        .into_iter()
        .map(|(tag, _)| tag)
        .collect();
    let known: Vec<&str> = vault_tags.iter().map(String::as_str).collect();
    rows.push(form_row(
        "azpasswordmanager-tags",
        TokenInput::create(strs(&tags), t("azpasswordmanager-tags"))
            .with_text(form.tag.as_str())
            .with_placeholder(t("azpasswordmanager-add-tag"))
            .with_suggestions(strs(&known))
            .with_on_event(app.clone(), on_tags_event as TokenInputOnEventCallbackType)
            .dom()
            .with_id(ids::EDIT_TAG),
    ));
    rows.push(form_row(
        "azpasswordmanager-notes",
        TextArea::create()
            .with_text(d.notes.as_str())
            .with_placeholder(t("azpasswordmanager-notes"))
            .with_accessibility_name(t("azpasswordmanager-notes"))
            .with_on_text_input(app.clone(), on_notes as TextAreaOnTextInputCallbackType)
            .dom()
            .with_id(ids::EDIT_NOTES),
    ));
    rows.push(form_row(
        "azpasswordmanager-favourite",
        Switch::create(d.favorite)
            .with_accessibility_name(t("azpasswordmanager-favourite"))
            .with_on_toggle(app.clone(), on_form_favorite as SwitchOnToggleCallbackType)
            .dom()
            .with_id(ids::EDIT_FAVORITE),
    ));
    if form.confirm_discard {
        rows.push(flex_row(
            "gap: 8px; padding-top: 12px;",
            vec![
                block("font-size: 13px;", text(t("azpasswordmanager-discard-ask"))),
                primary("azpasswordmanager-discard", ids::EDIT_DISCARD, app, on_form_discard),
                button("azpasswordmanager-keep-editing", ids::EDIT_KEEP, app, on_form_keep),
            ],
        ));
    } else {
        rows.push(flex_row(
            "gap: 8px; padding: 12px 0px 0px 110px;",
            vec![
                primary("azpasswordmanager-save", ids::EDIT_SAVE, app, on_edit_save),
                button("kit-button-cancel", ids::EDIT_CANCEL, app, on_form_cancel),
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
    let rebuild = matches!(field, EditField::Password | EditField::Totp);
    let update = with_form(&mut app, &mut info, |form, _| {
        set_text(form, field, value.to_string())
    });
    OnTextInputReturn {
        update: if rebuild { update } else { Update::DoNothing },
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

/// The tag field: every event carries the field's next state - the draft takes its tokens and
/// the typed text (the window is rebuilt: the chips, the suggestions).
extern "C" fn on_tags_event(mut data: RefAny, mut info: CallbackInfo, event: TokenInputEvent) -> Update {
    let tokens: Vec<&str> = event.state.tokens.as_slice().iter().map(|t| t.as_str()).collect();
    let typed = Zeroizing::new(event.state.text.as_str().to_string());
    with_form(&mut data, &mut info, |form, _| {
        take_tag_tokens(form, &tokens, &typed);
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
                    println!("AZPASSWORDMANAGER_ITEM_SAVED {}", item.title);
                }
                s.notice.clear();
                jobs::save(s, info, app);
            }
            Err(problem) => s.notice = t_text(&problem),
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

// ==== The generator ====

/// A new secret by the generator's settings.
pub fn regenerate(session: &mut Session) {
    zeroize::Zeroize::zeroize(&mut session.generated);
    session.generated = generator::generate(&session.generator).unwrap_or_default();
}

/// A switch of the generator.
#[derive(Clone, Copy)]
enum GenOption {
    Upper,
    Lower,
    Digits,
    Symbols,
    AvoidSimilar,
    Capitalize,
    AddDigit,
}

struct GenRef {
    app: RefAny,
    option: GenOption,
}

/// A generator option's check box; `name` a key, or the characters it adds (`A-Z`).
fn gen_check(app: &RefAny, option: GenOption, checked: bool, name: &str, id: AzString) -> Dom {
    flex_row(
        "gap: 4px; padding-right: 12px;",
        vec![
            CheckBox::create(checked)
                .with_accessibility_name(label(name))
                .with_on_toggle(
                    RefAny::new(GenRef {
                        app: app.clone(),
                        option,
                    }),
                    on_gen_option as CheckBoxOnToggleCallbackType,
                )
                .dom()
                .with_id(id),
            block("font-size: 13px;", text(label(name))),
        ],
    )
}

fn generator_view(session: &Session, app: &RefAny) -> Dom {
    let o = &session.generator;
    let bits = generator::entropy(o);
    let mut rows = vec![
        block(
            "font-size: 16px; font-weight: 600; padding-bottom: 8px;",
            text(t("azpasswordmanager-generator-title")),
        ),
        Segmented::create(strs(&[
            crate::l10n::mode(Mode::Password),
            crate::l10n::mode(Mode::Passphrase),
            crate::l10n::mode(Mode::Pin),
        ]))
        .with_selected_index(Mode::ALL.iter().position(|m| *m == o.mode).unwrap_or(0))
        .with_on_change(app.clone(), on_gen_mode as SegmentedOnChangeCallbackType)
        .dom()
        .with_id(ids::GEN_MODE),
        flex_row(
            "gap: 8px; padding: 12px 0px;",
            vec![
                block(
                    "flex-grow: 1; min-width: 0px; font-family: monospace; font-size: 18px;",
                    text(session.generated.as_str()),
                )
                .with_id(ids::GEN_OUTPUT),
                icon_button(
                    "azpasswordmanager-regenerate",
                    "refresh",
                    ids::GEN_REFRESH,
                    app,
                    on_gen_refresh,
                ),
                icon_button(
                    "azpasswordmanager-copy",
                    "content_copy",
                    ids::GEN_COPY,
                    app,
                    on_gen_copy,
                ),
            ],
        ),
    ];
    let (value, min, max, name) = match o.mode {
        Mode::Password => (
            o.length,
            generator::PASSWORD_LENGTH.0,
            generator::PASSWORD_LENGTH.1,
            "azpasswordmanager-length",
        ),
        Mode::Pin => (
            o.length,
            generator::PIN_LENGTH.0,
            generator::PIN_LENGTH.1,
            "azpasswordmanager-digits",
        ),
        Mode::Passphrase => (
            o.words,
            generator::PASSPHRASE_WORDS.0,
            generator::PASSPHRASE_WORDS.1,
            "azpasswordmanager-words",
        ),
    };
    let value = value.clamp(min, max);
    rows.push(form_row(
        name,
        flex_row(
            "gap: 8px;",
            vec![
                block(
                    "flex-grow: 1;",
                    Slider::create(value as f32, min as f32, max as f32)
                        .with_accessibility_name(t(name))
                        .with_on_value_change(
                            app.clone(),
                            on_gen_length as SliderOnValueChangeCallbackType,
                        )
                        .dom()
                        .with_id(ids::GEN_LENGTH),
                ),
                block("width: 32px; font-size: 13px;", text(value.to_string())),
            ],
        ),
    ));
    match o.mode {
        Mode::Password => {
            rows.push(flex_row(
                "flex-wrap: wrap; padding: 4px 0px;",
                vec![
                    gen_check(app, GenOption::Upper, o.upper, "A-Z", ids::GEN_UPPER),
                    gen_check(app, GenOption::Lower, o.lower, "a-z", ids::GEN_LOWER),
                    gen_check(app, GenOption::Digits, o.digits, "0-9", ids::GEN_DIGITS),
                    gen_check(app, GenOption::Symbols, o.symbols, "!@#$", ids::GEN_SYMBOLS),
                    gen_check(
                        app,
                        GenOption::AvoidSimilar,
                        o.avoid_similar,
                        "azpasswordmanager-avoid-similar",
                        ids::GEN_SIMILAR,
                    ),
                ],
            ));
        }
        Mode::Passphrase => {
            rows.push(flex_row(
                "flex-wrap: wrap; padding: 4px 0px;",
                vec![
                    gen_check(
                        app,
                        GenOption::Capitalize,
                        o.capitalize,
                        "azpasswordmanager-capitals",
                        ids::GEN_CAPITALIZE,
                    ),
                    gen_check(
                        app,
                        GenOption::AddDigit,
                        o.add_digit,
                        "azpasswordmanager-digit-at-end",
                        ids::GEN_ADD_DIGIT,
                    ),
                ],
            ));
        }
        Mode::Pin => {}
    }
    rows.push(
        form_row(
            "azpasswordmanager-strength",
            flex_row(
                "gap: 8px;",
                vec![
                    block(
                        "width: 160px;",
                        ProgressBar::create(generator::percent(bits)).dom(),
                    ),
                    block("font-size: 12px;", text(crate::l10n::strength_bits(bits))),
                ],
            ),
        )
        .with_id(ids::GEN_STRENGTH),
    );
    rows.push(note("azpasswordmanager-generator-note"));
    rows.push(flex_row(
        "padding-top: 8px;",
        vec![button("azpasswordmanager-close", ids::GEN_CLOSE, app, on_panel_close)],
    ));
    column(PANE, rows).with_id(ids::GENERATOR)
}

/// Runs `f` on the generator's settings and draws a new secret.
fn with_generator(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut generator::Options),
) -> Update {
    with_app(data, info, |s, _info, _| {
        if let Some(session) = s.session.as_mut() {
            f(&mut session.generator);
            regenerate(session);
        }
    })
}

extern "C" fn on_gen_mode(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SegmentedState,
) -> Update {
    with_generator(&mut data, &mut info, |o| {
        o.mode = Mode::ALL[state.selected_index.min(Mode::ALL.len() - 1)];
        if o.mode == Mode::Pin {
            o.length = o.length.clamp(generator::PIN_LENGTH.0, 8);
        } else if o.mode == Mode::Password && o.length < 12 {
            o.length = 20;
        }
    })
}

extern "C" fn on_gen_length(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SliderState,
) -> Update {
    let value = state.value.round().max(0.0) as usize;
    with_generator(&mut data, &mut info, |o| match o.mode {
        Mode::Passphrase => o.words = value,
        _ => o.length = value,
    })
}

extern "C" fn on_gen_option(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: CheckBoxState,
) -> Update {
    let Some((mut app, option)) = data
        .downcast_ref::<GenRef>()
        .map(|r| (r.app.clone(), r.option))
    else {
        return Update::DoNothing;
    };
    with_generator(&mut app, &mut info, |o| {
        let flag = match option {
            GenOption::Upper => &mut o.upper,
            GenOption::Lower => &mut o.lower,
            GenOption::Digits => &mut o.digits,
            GenOption::Symbols => &mut o.symbols,
            GenOption::AvoidSimilar => &mut o.avoid_similar,
            GenOption::Capitalize => &mut o.capitalize,
            GenOption::AddDigit => &mut o.add_digit,
        };
        *flag = state.checked;
    })
}

extern "C" fn on_gen_refresh(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_generator(&mut data, &mut info, |_| {})
}

extern "C" fn on_gen_copy(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        let Some(secret) = s
            .session
            .as_ref()
            .map(|session| Zeroizing::new(session.generated.clone()))
        else {
            return;
        };
        if secret.is_empty() {
            return;
        }
        jobs::set_clipboard(info, &secret);
        s.clipboard.clear_after = s.settings.clear_seconds;
        s.clipboard
            .copied(&secret, &t("azpasswordmanager-generated-password"), now());
        s.notice = t("azpasswordmanager-copied-generated");
        println!("AZPASSWORDMANAGER_COPIED generated password");
    })
}

extern "C" fn on_panel_close(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        if let Some(session) = s.session.as_mut() {
            session.reading = Reading::Item;
        }
    })
}

// ==== Import ====

fn import_view(view: &ImportView, app: &RefAny) -> Dom {
    let mut rows = vec![
        block(
            "font-size: 16px; font-weight: 600; padding-bottom: 8px;",
            text(t("azpasswordmanager-import-title")),
        ),
        note("azpasswordmanager-import-help"),
        form_row(
            "azpasswordmanager-file",
            flex_row(
                "gap: 4px;",
                vec![
                    block(
                        "flex-grow: 1; min-width: 0px;",
                        TextInput::create()
                            .with_text(view.path.as_str())
                            .with_placeholder(t("azpasswordmanager-export-path"))
                            .with_accessibility_name(t("azpasswordmanager-export-path"))
                            .with_on_text_input(
                                app.clone(),
                                on_import_path as TextInputOnTextInputCallbackType,
                            )
                            .dom()
                            .with_id(ids::IMPORT_PATH),
                    ),
                    button("azpasswordmanager-choose", ids::IMPORT_CHOOSE, app, on_import_choose),
                    button("azpasswordmanager-read", ids::IMPORT_READ, app, on_import_read_path),
                ],
            ),
        ),
    ];
    if view.reading {
        rows.push(note("azpasswordmanager-reading-file"));
    }
    match &view.result {
        Some(Ok(imported)) => {
            let mut counts = [0usize; 5];
            for item in &imported.items {
                counts[item.kind.index()] += 1;
            }
            let kinds: Vec<String> = Kind::ALL
                .iter()
                .filter(|k| counts[k.index()] > 0)
                .map(|k| crate::l10n::kind_count(*k, counts[k.index()]))
                .collect();
            let summary = [
                ("format", Arg::from(imported.format.label())),
                ("count", Arg::from(imported.items.len())),
                ("kinds", Arg::from(kinds.join(", "))),
                ("skipped", Arg::from(imported.skipped.len())),
            ];
            let key = if imported.skipped.is_empty() {
                "azpasswordmanager-import-summary"
            } else {
                "azpasswordmanager-import-summary-skipped"
            };
            rows.push(
                block(
                    "font-size: 13px; padding: 8px 0px;",
                    text(t_args(key, &summary)),
                )
                .with_id(ids::IMPORT_SUMMARY),
            );
            for line in imported.skipped.iter().take(10) {
                rows.push(note(&t_text(line)));
            }
            if !imported.items.is_empty() {
                rows.push(flex_row(
                    "gap: 8px; padding-top: 8px;",
                    vec![
                        primary(
                            &t_args(
                                "azpasswordmanager-import-run",
                                &[("count", Arg::from(imported.items.len()))],
                            ),
                            ids::IMPORT_RUN,
                            app,
                            on_import_run,
                        ),
                        button("kit-button-cancel", ids::IMPORT_CANCEL, app, on_panel_close),
                    ],
                ));
            }
        }
        Some(Err(why)) => rows.push(problem(&t_text(why), ids::IMPORT_SUMMARY)),
        None => {}
    }
    if !matches!(view.result, Some(Ok(_))) {
        rows.push(flex_row(
            "padding-top: 8px;",
            vec![button(
                "azpasswordmanager-close",
                ids::IMPORT_CANCEL,
                app,
                on_panel_close,
            )],
        ));
    }
    column(PANE, rows).with_id(ids::IMPORT)
}

extern "C" fn on_import_path(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let path = state.get_text().as_str().to_string();
    let _ = with_app(&mut data, &mut info, |s, _info, _| {
        if let Some(Reading::Import(view)) = s.session.as_mut().map(|session| &mut session.reading)
        {
            view.path = path;
        }
    });
    keep()
}

extern "C" fn on_import_read_path(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, app| {
        let path = match s.session.as_ref().map(|session| &session.reading) {
            Some(Reading::Import(view)) => view.path.trim().to_string(),
            _ => return,
        };
        if path.is_empty() {
            if let Some(Reading::Import(view)) =
                s.session.as_mut().map(|session| &mut session.reading)
            {
                view.result = Some(Err(Text::key("azpasswordmanager-type-export-path")));
            }
            return;
        }
        crate::ui::read_import_file(s, info, app, std::path::Path::new(&path));
    })
}

extern "C" fn on_import_choose(mut data: RefAny, _info: CallbackInfo) -> Update {
    let app = data.clone();
    if data.downcast_ref::<KeysApp>().is_none() {
        return Update::DoNothing;
    }
    let _request = FileDialog::open_file(
        t("azpasswordmanager-import-title"),
        OptionString::None,
        OptionFileTypeList::None,
        app,
        on_import_picked,
    );
    Update::DoNothing
}

extern "C" fn on_import_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    let path = std::path::PathBuf::from(path.as_string().as_str());
    with_app(&mut data, &mut info, |s, info, app| {
        crate::ui::read_import_file(s, info, app, &path)
    })
}

/// "Import N items": into the vault (leaving out what it has), then saved.
extern "C" fn on_import_run(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, app| {
        let Some(session) = s.session.as_mut() else {
            return;
        };
        let Reading::Import(view) = &mut session.reading else {
            return;
        };
        let Some(Ok(imported)) = view.result.take() else {
            return;
        };
        let (added, known) = crate::import::merge(&mut session.open.vault, imported.items, now());
        session.dirty |= added > 0;
        session.reading = Reading::Item;
        session.keep_selection_in_view();
        s.notice = if known > 0 {
            t_args(
                "azpasswordmanager-imported-known",
                &[("count", Arg::from(added)), ("known", Arg::from(known))],
            )
        } else {
            t_args("azpasswordmanager-imported", &[("count", Arg::from(added))])
        };
        println!("AZPASSWORDMANAGER_IMPORTED {added} {known}");
        jobs::save(s, info, app);
    })
}

// ==== The audit ====

struct AuditRef {
    app: RefAny,
    id: String,
}

fn audit_view(session: &Session, filter: Filter, app: &RefAny) -> Dom {
    let vault = &session.open.vault;
    let a = audit::audit(vault, now());
    let labels: Vec<String> = Filter::ALL
        .iter()
        .map(|f| {
            let (key, count) = match f {
                Filter::All => ("azpasswordmanager-audit-all", a.findings.len()),
                Filter::Weak => ("azpasswordmanager-audit-weak", a.weak),
                Filter::Reused => ("azpasswordmanager-audit-reused", a.reused),
                Filter::Old => ("azpasswordmanager-audit-old", a.old),
                Filter::NoTwoFactor => ("azpasswordmanager-audit-no-two-factor", a.no_two_factor),
            };
            t_args(key, &[("count", Arg::from(count))])
        })
        .collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut rows = vec![
        block(
            "font-size: 16px; font-weight: 600; padding-bottom: 8px;",
            text(t("azpasswordmanager-audit-title")),
        ),
        Segmented::create(strs(&label_refs))
            .with_selected_index(Filter::ALL.iter().position(|f| *f == filter).unwrap_or(0))
            .with_on_change(
                app.clone(),
                on_audit_filter as SegmentedOnChangeCallbackType,
            )
            .dom()
            .with_id(ids::AUDIT_FILTER),
        note(&t_args(
            "azpasswordmanager-audit-summary",
            &[
                ("weak", Arg::from(a.weak)),
                ("reused", Arg::from(a.reused)),
                ("old", Arg::from(a.old)),
                ("none", Arg::from(a.no_two_factor)),
            ],
        ))
        .with_id(ids::AUDIT_SUMMARY),
    ];
    // TODO: azul's DataTable (sortable columns) in place of these rows.
    let mut shown = 0;
    for finding in a.findings.iter().filter(|f| filter.holds(&f.problems)) {
        let Some(item) = vault.items.get(finding.index) else {
            continue;
        };
        let problems: Vec<String> = finding
            .problems
            .iter()
            .map(|p| crate::l10n::problem(*p))
            .collect();
        rows.push(
            flex_row(
                "padding: 4px 0px; cursor: pointer;",
                vec![
                    block("width: 34%; font-size: 13px;", text(item.title.as_str())),
                    block("width: 34%; font-size: 12px;", text(problems.join(", "))),
                    block(
                        "width: 14%; font-size: 12px;",
                        text(crate::l10n::strength(finding.strength)),
                    ),
                    block(
                        &format!(
                            "width: 18%; font-size: 12px; opacity: 0.7; {}",
                            crate::ui::QUIET_FLORA
                        ),
                        text(date(finding.changed)),
                    ),
                ],
            )
            .with_id(ids::audit_row(shown))
            .clickable(
                item.title.as_str(),
                RefAny::new(AuditRef {
                    app: app.clone(),
                    id: item.id.clone(),
                }),
                on_audit_row,
            ),
        );
        shown += 1;
    }
    if shown == 0 {
        rows.push(note("azpasswordmanager-audit-nothing"));
    }
    rows.push(flex_row(
        "gap: 8px; padding-top: 12px;",
        vec![
            icon_button(
                "azpasswordmanager-export-report",
                "download",
                ids::AUDIT_EXPORT,
                app,
                on_audit_export,
            ),
            button("azpasswordmanager-close", ids::AUDIT_CLOSE, app, on_panel_close),
        ],
    ));
    column(PANE, rows).with_id(ids::AUDIT)
}

extern "C" fn on_audit_filter(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SegmentedState,
) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        if let Some(session) = s.session.as_mut() {
            session.reading =
                Reading::Audit(Filter::ALL[state.selected_index.min(Filter::ALL.len() - 1)]);
        }
    })
}

extern "C" fn on_audit_row(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id)) = data
        .downcast_ref::<AuditRef>()
        .map(|r| (r.app.clone(), r.id.clone()))
    else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        if let Some(session) = s.session.as_mut() {
            session.scope = crate::vault::Scope::All;
            session.query.clear();
            session.reading = Reading::Item;
            session.select(Some(id));
        }
    })
}

/// "Export report": the audit, without a secret, into keys/exports/ in the data tree.
extern "C" fn on_audit_export(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, app| {
        let Some(session) = s.session.as_ref() else {
            return;
        };
        let t = now();
        let report = audit::report(&audit::audit(&session.open.vault, t), &session.open.vault);
        let work = Work::Put {
            key: store::audit_key(&date(t)),
            bytes: report.into_bytes(),
        };
        jobs::spawn(info, app, &s.data_root, work);
    })
}

/// The tag field's state (its tokens and the typed text) into the draft: each token once, as
/// `Item::add_tag` keeps it (no `#`, no case-folded twin); a chip removed in the field leaves
/// the draft.
fn take_tag_tokens(form: &mut Form, tokens: &[&str], typed: &str) {
    form.draft.tags.clear();
    for token in tokens {
        form.draft.add_tag(token);
    }
    form.tag = typed.to_string();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tag_fields_tokens_become_the_drafts_tags_once_and_without_a_hash() {
        let mut form = Form::new_item(Kind::Login, 0);
        take_tag_tokens(&mut form, &["work", "#Work", " home "], "fi");
        assert_eq!(form.draft.tags, vec!["work", "home"]);
        assert_eq!(form.tag, "fi");
        take_tag_tokens(&mut form, &["home"], "");
        assert_eq!(
            form.draft.tags,
            vec!["home"],
            "a chip removed in the field leaves the draft"
        );
        assert!(form.tag.is_empty());
    }
}
