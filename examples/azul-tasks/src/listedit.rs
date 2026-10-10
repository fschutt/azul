//! Two more faces of the reading pane: a list's settings (name, group, colour, "default
//! list", delete) and the commands for several selected tasks (complete, flag, move,
//! delete). Element ids for scripts: `#list-name`, `#list-group`, `#list-default`,
//! `#list-delete`, `#list-done`, `#bulk`, `#bulk-complete`, `#bulk-flag`, `#bulk-move`,
//! `#bulk-delete`.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DropDownOnChoiceChangeCallbackType, SwitchOnToggleCallbackType,
        TextInputOnFocusLostCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType,
    },
    dom::VirtualKeyCode,
    prelude::*,
    str::String as AzString,
    vec::StringVec,
    widgets::{ButtonType, DropDown, OnTextInputReturn, Switch, SwitchState, TextInputState, TextInputValid},
};

use azul_appkit::l10n::{self, t_args, Arg};

use crate::{
    ids,
    model::ListColor,
    state::{self, Confirm, Tasks},
    views,
};

const PANE: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                    overflow-y: auto; padding: 12px 16px; gap: 12px;";
const FIELD: &str = "display: flex; flex-direction: row; align-items: center; gap: 8px; \
                     flex-wrap: wrap;";
const LABEL: &str = "font-size: 12px; color: system:secondary-text; min-width: 72px;";

/// A field's label: a key of the resources, or words as they are.
fn label(text: &str) -> Dom {
    Dom::create_span_with_text(l10n::label(text)).with_css(LABEL)
}

const KEEP: OnTextInputReturn = OnTextInputReturn {
    update: Update::DoNothing,
    valid: TextInputValid::Yes,
};

/// What a colour button carries.
struct ColorRef {
    app: RefAny,
    color: ListColor,
}

/// List `list`'s settings.
pub fn pane(s: &Tasks, app: &RefAny, list: &str) -> Dom {
    let Some(li) = s.list_index(list) else {
        return Dom::create_div();
    };
    let l = &s.lists[li];
    let (name, group) = if s.drafts.list == l.id {
        (s.drafts.list_name.as_str(), s.drafts.list_group.as_str())
    } else {
        (l.name.as_str(), l.group.as_str())
    };
    let mut colors = Dom::create_div().with_css(FIELD).with_child(label("aztasks-colour"));
    for color in ListColor::ALL {
        let on = color == l.color;
        colors.add_child(
            Button::with_type(l10n::label(color.message_id()), if on { ButtonType::Primary } else { ButtonType::Default })
                .with_on_click(
                    RefAny::new(ColorRef {
                        app: app.clone(),
                        color,
                    }),
                    on_color as ButtonOnClickCallbackType,
                )
                .dom()
                .with_css(format!(
                    "border-left: 6px solid {}; @media (prefers-color-scheme: dark) {{ border-left: 6px solid {}; }}",
                    color.hex(false),
                    color.hex(true)
                )),
        );
    }
    let is_default = s.default_list().as_deref() == Some(l.id.as_str());
    Dom::create_div()
        .with_id(ids::LIST_EDIT)
        .with_css(PANE)
        .with_child(Dom::create_h2_with_text(l10n::label("aztasks-list-settings")).with_css("font-size: 18px; font-weight: bold;"))
        .with_child(
            Dom::create_div().with_css(FIELD).with_child(label("aztasks-name")).with_child(
                TextInput::create()
                    .with_text(name)
                    .with_placeholder(l10n::label("aztasks-list-name"))
                    .with_accessibility_name(l10n::label("aztasks-list-name"))
                    .with_on_text_input(app.clone(), on_name_text as TextInputOnTextInputCallbackType)
                    .with_on_virtual_key_down(app.clone(), on_field_key as TextInputOnVirtualKeyDownCallbackType)
                    .with_on_focus_lost(app.clone(), on_field_blur as TextInputOnFocusLostCallbackType)
                    .dom()
                    .with_id(ids::LIST_NAME)
                    .with_css("flex-grow: 1;"),
            ),
        )
        .with_child(
            Dom::create_div().with_css(FIELD).with_child(label("aztasks-group")).with_child(
                TextInput::create()
                    .with_text(group)
                    .with_placeholder(l10n::label("aztasks-no-group-type-one"))
                    .with_accessibility_name(l10n::label("aztasks-group"))
                    .with_on_text_input(app.clone(), on_group_text as TextInputOnTextInputCallbackType)
                    .with_on_virtual_key_down(app.clone(), on_field_key as TextInputOnVirtualKeyDownCallbackType)
                    .with_on_focus_lost(app.clone(), on_field_blur as TextInputOnFocusLostCallbackType)
                    .dom()
                    .with_id(ids::LIST_GROUP)
                    .with_css("flex-grow: 1;"),
            ),
        )
        .with_child(colors)
        .with_child(
            Dom::create_div()
                .with_css(FIELD)
                .with_child(label("aztasks-default"))
                .with_child(
                    Switch::create(is_default)
                        .with_accessibility_name(l10n::label("aztasks-new-tasks-outside-list"))
                        .with_on_toggle(app.clone(), on_default as SwitchOnToggleCallbackType)
                        .dom()
                        .with_id(ids::LIST_DEFAULT),
                )
                .with_child(Dom::create_span_with_text(l10n::label("aztasks-new-tasks-outside-list")).with_css(LABEL)),
        )
        .with_child(
            Dom::create_div()
                .with_css(FIELD)
                .with_child(
                    Button::with_type(l10n::label("aztasks-column-done"), ButtonType::Primary)
                        .with_on_click(app.clone(), on_done as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::LIST_DONE),
                )
                .with_child(
                    Button::with_type(l10n::label("aztasks-delete-list"), ButtonType::Danger)
                        .with_icon("delete")
                        .with_on_click(app.clone(), on_delete as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::LIST_DELETE),
                ),
        )
}

/// The commands for the selected tasks.
pub fn bulk(s: &Tasks, app: &RefAny) -> Dom {
    let picked = s.selected();
    let all_done = !picked.is_empty() && picked.iter().all(|&i| s.tasks[i].is_done());
    let all_flagged = !picked.is_empty() && picked.iter().all(|&i| s.tasks[i].flagged);
    let order = views::lists_in_nav_order(&s.lists);
    let mut names = vec![l10n::label("aztasks-move-to")];
    names.extend(order.iter().map(|&i| AzString::from(s.lists[i].name.as_str())));
    Dom::create_div()
        .with_id(ids::BULK)
        .with_css(PANE)
        .with_child(
            Dom::create_h2_with_text(t_args("aztasks-selected", &[("count", Arg::from(picked.len()))]))
                .with_css("font-size: 18px; font-weight: bold;"),
        )
        .with_child(
            Dom::create_div()
                .with_css(FIELD)
                .with_child(
                    Button::create(l10n::label(if all_done { "aztasks-open-again" } else { "aztasks-cmd-complete" }))
                        .with_icon("task_alt")
                        .with_on_click(app.clone(), on_bulk_complete as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::BULK_COMPLETE),
                )
                .with_child(
                    Button::create(l10n::label(if all_flagged { "aztasks-unflag" } else { "aztasks-cmd-flag" }))
                        .with_icon("flag")
                        .with_on_click(app.clone(), on_bulk_flag as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::BULK_FLAG),
                )
                .with_child(
                    DropDown::create(StringVec::from(names))
                        .with_selected(0)
                        .with_accessibility_name(l10n::label("aztasks-move"))
                        .with_on_choice_change(app.clone(), on_bulk_move as DropDownOnChoiceChangeCallbackType)
                        .dom()
                        .with_id(ids::BULK_MOVE),
                )
                .with_child(
                    Button::with_type(l10n::label("aztasks-cmd-delete"), ButtonType::Danger)
                        .with_icon("delete")
                        .with_on_click(app.clone(), on_bulk_delete as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::BULK_DELETE),
                ),
        )
        .with_child(
            Dom::create_span_with_text(l10n::label("aztasks-ctrl-cmd-click-adds"))
                .with_css(LABEL),
        )
}

// ==== Callbacks ====

extern "C" fn on_name_text(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<Tasks>() {
        s.drafts.list_name = state.get_text().as_str().to_string();
    }
    KEEP
}

extern "C" fn on_group_text(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<Tasks>() {
        s.drafts.list_group = state.get_text().as_str().to_string();
    }
    KEEP
}

extern "C" fn on_field_key(mut data: RefAny, mut info: CallbackInfo, _state: TextInputState) -> OnTextInputReturn {
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    if !matches!(key, Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter)) {
        return KEEP;
    }
    let update = crate::with_tasks(&mut data, &mut info, |_info, _app, s| s.commit_list_drafts());
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_field_blur(mut data: RefAny, mut info: CallbackInfo, _state: TextInputState) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| s.commit_list_drafts())
}

extern "C" fn on_color(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, color)) = data
        .downcast_ref::<ColorRef>()
        .map(|r| (r.app.clone(), r.color))
    else {
        return Update::DoNothing;
    };
    crate::with_tasks(&mut app, &mut info, |_info, _app, s| {
        let Some(id) = s.editing_list.clone() else {
            return;
        };
        if let Some(li) = s.list_index(&id) {
            s.lists[li].color = color;
            s.save_list(li);
        }
    })
}

extern "C" fn on_default(mut data: RefAny, mut info: CallbackInfo, state: SwitchState) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        let Some(id) = s.editing_list.clone() else {
            return;
        };
        if state.checked {
            s.settings.default_list = id;
        } else if s.settings.default_list == id {
            s.settings.default_list.clear();
        }
        s.save_settings();
    })
}

extern "C" fn on_done(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        s.commit_list_drafts();
        s.editing_list = None;
    })
}

extern "C" fn on_delete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        if let Some(id) = s.editing_list.clone() {
            s.confirm = Some(Confirm::DeleteList(id));
        }
    })
}

extern "C" fn on_bulk_complete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| s.toggle_selected(state::now()))
}

extern "C" fn on_bulk_flag(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        let picked = s.selected();
        let flag = !picked.iter().all(|&i| s.tasks[i].flagged);
        for i in picked {
            s.tasks[i].flagged = flag;
            s.save_task(i);
        }
    })
}

extern "C" fn on_bulk_move(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    crate::with_tasks(&mut data, &mut info, |info, app, s| {
        let order = views::lists_in_nav_order(&s.lists);
        let Some(&li) = index.checked_sub(1).and_then(|n| order.get(n)) else {
            return;
        };
        let list = s.lists[li].id.clone();
        let ids = s.selected_ids();
        let moves = s.move_tasks(&ids, &list);
        crate::jobs::move_files(info, app, s, moves);
    })
}

extern "C" fn on_bulk_delete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |info, app, s| {
        let ids = s.selected_ids();
        let gone = s.delete_tasks(&ids);
        crate::jobs::delete_files(info, app, s, gone);
    })
}
