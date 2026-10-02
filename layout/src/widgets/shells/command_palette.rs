//! ShellCommandPalette - Ctrl/Cmd+K (or Cmd+Shift+P) over the app's command
//! table, keyboard-first (04-app-shells.md, "Commands"): the menu, the
//! ribbon, a toolbar and the palette are all views of ONE list of commands.
//!
//! ```text
//! ┌──────────────────────────────────────────────┐
//! │ 🔍 new                                       │  a search field
//! ├──────────────────────────────────────────────┤
//! │ ✉  New message                     Ctrl+N    │  the matching commands,
//! │ 📁 New folder                                │  one Tab stop; Up / Down
//! │ 📅 New event                       Ctrl+E    │  move, Enter runs, Escape
//! └──────────────────────────────────────────────┘  closes
//! ```
//!
//! The palette is an overlay over the window (its root is positioned over
//! its nearest positioned ancestor - the shell root, or the body). It owns
//! no state: typing reports the query ([`ShellCommandPalette::on_query`]) and the
//! app rebuilds with [`ShellCommandPalette::with_query`]; the palette filters the
//! commands itself ([`palette_matches`]: every character of the query, in
//! order, case-insensitively - "nmsg" finds "New message"). Enter or a click
//! reports the command's index in the table ([`ShellCommandPalette::on_run`]);
//! Escape, or a click on the backdrop, reports a close.
//!
//! Key types: [`ShellCommandPalette`], [`ShellPaletteCommand`], [`palette_matches`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
    callbacks::{CoreCallback, CoreCallbackData, FocusTarget, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter},
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq,
    AzString,
};

use super::{
    inner_theme, look_for, part, root_classes, stack_state, state_classes, text, ShellLook,
    CHROME_ROW_BASE, GROW_LABEL_BASE, ITEM_BASE, LABEL_BASE, OVERLAY_BASE, SCROLL_COLUMN_BASE,
};
use crate::{
    callbacks::CallbackInfo,
    widgets::{
        button::{ButtonOnClick, ButtonOnClickCallback, OptionButtonOnClick},
        roving,
        text_input::{
            OnTextInputReturn, TextInput, TextInputOnTextInputCallbackType,
            TextInputOnVirtualKeyDownCallbackType, TextInputState, TextInputValid,
        },
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The backdrop's class (the palette's root while open).
pub const PALETTE_CLASS: &str = "__azul-native-command-palette";
/// The root's class while the palette is closed (an empty node).
pub const CLOSED_CLASS: &str = "__azul-native-command-palette-closed";
/// The panel's class.
pub const PANEL_CLASS: &str = "__azul-native-command-palette-panel";
/// The search row's class.
pub const INPUT_CLASS: &str = "__azul-native-command-palette-input";
/// The result list's class.
pub const LIST_CLASS: &str = "__azul-native-command-palette-list";
/// A result row's class.
pub const ROW_CLASS: &str = "__azul-native-command-palette-row";
/// Added to the selected row.
pub const ROW_SELECTED_CLASS: &str = "__azul-native-command-palette-row-selected";
/// A row's icon.
pub const ROW_ICON_CLASS: &str = "__azul-native-command-palette-row-icon";
/// A row's label.
pub const ROW_LABEL_CLASS: &str = "__azul-native-command-palette-row-label";
/// A row's shortcut.
pub const ROW_SHORTCUT_CLASS: &str = "__azul-native-command-palette-row-shortcut";
/// The "no matching commands" line.
pub const EMPTY_CLASS: &str = "__azul-native-command-palette-empty";

/// Callback invoked when a command is run: its index in the table.
pub type ShellCommandPaletteOnRunCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    ShellCommandPaletteOnRun,
    OptionShellCommandPaletteOnRun,
    ShellCommandPaletteOnRunCallback,
    ShellCommandPaletteOnRunCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ShellCommandPaletteOnRunCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SHELL_COMMAND_PALETTE_ON_RUN_INVOKER,
    invoker_ty:     AzShellCommandPaletteOnRunCallbackInvoker,
    thunk_fn:       az_shell_command_palette_on_run_callback_thunk,
    setter_fn:      AzApp_setShellCommandPaletteOnRunCallbackInvoker,
    from_handle_fn: AzShellCommandPaletteOnRunCallback_createFromHostHandle,
    from_handle_byref_fn: AzShellCommandPaletteOnRunCallback_createFromHostHandleByref,
    extra_args:     [ command_index: usize ],
}

/// Callback invoked when the query changed: the app stores it and rebuilds
/// with [`ShellCommandPalette::with_query`].
pub type ShellCommandPaletteOnQueryCallbackType =
    extern "C" fn(RefAny, CallbackInfo, AzString) -> Update;
impl_widget_callback!(
    ShellCommandPaletteOnQuery,
    OptionShellCommandPaletteOnQuery,
    ShellCommandPaletteOnQueryCallback,
    ShellCommandPaletteOnQueryCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ShellCommandPaletteOnQueryCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SHELL_COMMAND_PALETTE_ON_QUERY_INVOKER,
    invoker_ty:     AzShellCommandPaletteOnQueryCallbackInvoker,
    thunk_fn:       az_shell_command_palette_on_query_callback_thunk,
    setter_fn:      AzApp_setShellCommandPaletteOnQueryCallbackInvoker,
    from_handle_fn: AzShellCommandPaletteOnQueryCallback_createFromHostHandle,
    from_handle_byref_fn: AzShellCommandPaletteOnQueryCallback_createFromHostHandleByref,
    extra_args:     [ query: AzString ],
}

/// One command of the table.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ShellPaletteCommand {
    /// The label ("New message").
    pub label: AzString,
    /// The shortcut, shown set right ("Ctrl+N"), or empty.
    pub shortcut: AzString,
    /// The icon (a `Dom::create_icon` name), or empty.
    pub icon: AzString,
    /// The category ("Mail"), matched together with the label.
    pub category: AzString,
}

impl_option!(
    ShellPaletteCommand,
    OptionShellPaletteCommand,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    ShellPaletteCommand,
    ShellPaletteCommandVec,
    ShellPaletteCommandVecDestructor,
    ShellPaletteCommandVecDestructorType,
    ShellPaletteCommandVecSlice,
    OptionShellPaletteCommand
);
impl_vec_clone!(ShellPaletteCommand, ShellPaletteCommandVec, ShellPaletteCommandVecDestructor);
impl_vec_debug!(ShellPaletteCommand, ShellPaletteCommandVec);
impl_vec_partialeq!(ShellPaletteCommand, ShellPaletteCommandVec);
impl_vec_mut!(ShellPaletteCommand, ShellPaletteCommandVec);

impl ShellPaletteCommand {
    /// A command with `label`, no shortcut, no icon, no category.
    #[must_use]
    pub fn create(label: AzString) -> Self {
        Self {
            label,
            shortcut: AzString::from_const_str(""),
            icon: AzString::from_const_str(""),
            category: AzString::from_const_str(""),
        }
    }

    /// The shortcut shown set right.
    pub fn set_shortcut(&mut self, shortcut: AzString) {
        self.shortcut = shortcut;
    }

    /// [`Self::set_shortcut`] for the builder chain.
    #[must_use]
    pub fn with_shortcut(mut self, shortcut: AzString) -> Self {
        self.set_shortcut(shortcut);
        self
    }

    /// The icon.
    pub fn set_icon(&mut self, icon: AzString) {
        self.icon = icon;
    }

    /// [`Self::set_icon`] for the builder chain.
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.set_icon(icon);
        self
    }

    /// The category.
    pub fn set_category(&mut self, category: AzString) {
        self.category = category;
    }

    /// [`Self::set_category`] for the builder chain.
    #[must_use]
    pub fn with_category(mut self, category: AzString) -> Self {
        self.set_category(category);
        self
    }

    /// What the palette matches the query against: "Category: Label".
    #[must_use]
    pub fn search_text(&self) -> String {
        if self.category.as_str().is_empty() {
            String::from(self.label.as_str())
        } else {
            alloc::format!("{}: {}", self.category.as_str(), self.label.as_str())
        }
    }
}

/// Whether `text` matches `query`: every character of the query occurs in
/// the text, in order, case-insensitively (a subsequence: "nmsg" finds "New
/// message"); spaces in the query are ignored; an empty query matches
/// everything.
#[must_use]
pub fn palette_matches(query: &str, text: &str) -> bool {
    let mut haystack = text.chars().flat_map(char::to_lowercase);
    for wanted in query.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase) {
        if !haystack.any(|c| c == wanted) {
            return false;
        }
    }
    true
}

/// The command palette: a search field over the app's command table.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct ShellCommandPalette {
    /// The command table.
    pub commands: ShellPaletteCommandVec,
    /// The query in the field.
    pub query: AzString,
    /// The field's placeholder.
    pub placeholder: AzString,
    /// The query changed.
    pub on_query: OptionShellCommandPaletteOnQuery,
    /// A command was run.
    pub on_run: OptionShellCommandPaletteOnRun,
    /// Escape or the backdrop: the app closes the palette.
    pub on_close: OptionButtonOnClick,
    /// The widget theme this palette is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
    /// Whether the palette is shown.
    pub is_open: bool,
}

impl ShellCommandPalette {
    /// A closed palette with no commands.
    #[must_use]
    pub fn create() -> Self {
        Self {
            commands: ShellPaletteCommandVec::from_const_slice(&[]),
            query: AzString::from_const_str(""),
            placeholder: AzString::from_const_str("Type a command"),
            on_query: None.into(),
            on_run: None.into(),
            on_close: None.into(),
            theme: OptionUiTheme::None,
            is_open: false,
        }
    }

    /// Appends a command to the table.
    pub fn add_command(&mut self, command: ShellPaletteCommand) {
        let mut v = self.commands.clone().into_library_owned_vec();
        v.push(command);
        self.commands = ShellPaletteCommandVec::from_vec(v);
    }

    /// [`Self::add_command`] for the builder chain.
    #[must_use]
    pub fn with_command(mut self, command: ShellPaletteCommand) -> Self {
        self.add_command(command);
        self
    }

    /// Replaces the command table.
    pub fn set_commands(&mut self, commands: ShellPaletteCommandVec) {
        self.commands = commands;
    }

    /// [`Self::set_commands`] for the builder chain.
    #[must_use]
    pub fn with_commands(mut self, commands: ShellPaletteCommandVec) -> Self {
        self.set_commands(commands);
        self
    }

    /// The query in the field.
    pub fn set_query(&mut self, query: AzString) {
        self.query = query;
    }

    /// [`Self::set_query`] for the builder chain.
    #[must_use]
    pub fn with_query(mut self, query: AzString) -> Self {
        self.set_query(query);
        self
    }

    /// The field's placeholder.
    pub fn set_placeholder(&mut self, placeholder: AzString) {
        self.placeholder = placeholder;
    }

    /// [`Self::set_placeholder`] for the builder chain.
    #[must_use]
    pub fn with_placeholder(mut self, placeholder: AzString) -> Self {
        self.set_placeholder(placeholder);
        self
    }

    /// Show or hide the palette.
    pub const fn set_open(&mut self, open: bool) {
        self.is_open = open;
    }

    /// [`Self::set_open`] for the builder chain.
    #[must_use]
    pub const fn with_open(mut self, open: bool) -> Self {
        self.set_open(open);
        self
    }

    /// The query changed.
    pub fn set_on_query<C: Into<ShellCommandPaletteOnQueryCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_query = Some(ShellCommandPaletteOnQuery {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_query`] for the builder chain.
    #[must_use]
    pub fn with_on_query<C: Into<ShellCommandPaletteOnQueryCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_query(data, callback);
        self
    }

    /// A command was run.
    pub fn set_on_run<C: Into<ShellCommandPaletteOnRunCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_run = Some(ShellCommandPaletteOnRun {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_run`] for the builder chain.
    #[must_use]
    pub fn with_on_run<C: Into<ShellCommandPaletteOnRunCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_run(data, callback);
        self
    }

    /// Escape or the backdrop.
    pub fn set_on_close<C: Into<ButtonOnClickCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_close = Some(ButtonOnClick {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_close`] for the builder chain.
    #[must_use]
    pub fn with_on_close<C: Into<ButtonOnClickCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_close(data, callback);
        self
    }

    /// Pin the widget theme; unset, the palette follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with a closed, empty palette and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }

    /// The indices of the commands the query matches, in table order.
    #[must_use]
    pub(crate) fn matching(&self) -> Vec<usize> {
        self.commands
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, c)| palette_matches(self.query.as_str(), &c.search_text()))
            .map(|(i, _)| i)
            .collect()
    }

    /// The palette's DOM: the backdrop and the panel while open, an empty
    /// node while closed.
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        build(self, &look)
    }
}

impl Default for ShellCommandPalette {
    fn default() -> Self {
        Self::create()
    }
}

impl From<ShellCommandPalette> for Dom {
    fn from(p: ShellCommandPalette) -> Self {
        p.dom()
    }
}

// ---------------------------------------------------------------------------
// The handlers
// ---------------------------------------------------------------------------

/// What every part of one palette shares.
struct PaletteShared {
    on_query: OptionShellCommandPaletteOnQuery,
    on_run: OptionShellCommandPaletteOnRun,
    on_close: OptionButtonOnClick,
    /// The command index of every row, in row order.
    rows: Vec<usize>,
}

/// One row: the shared state and the row's command.
struct RowRef {
    shared: RefAny,
    command: usize,
}

fn run(shared: &mut RefAny, info: CallbackInfo, command: usize) -> Update {
    let Some(s) = shared.downcast_ref::<PaletteShared>() else {
        return Update::DoNothing;
    };
    match s.on_run.as_ref() {
        Some(ShellCommandPaletteOnRun { callback, refany }) => callback.invoke(refany.clone(), info, command),
        None => Update::DoNothing,
    }
}

fn close(shared: &mut RefAny, info: CallbackInfo) -> Update {
    let Some(s) = shared.downcast_ref::<PaletteShared>() else {
        return Update::DoNothing;
    };
    match s.on_close.as_ref() {
        Some(ButtonOnClick { callback, refany }) => callback.invoke(refany.clone(), info),
        None => Update::DoNothing,
    }
}

fn first_row_command(shared: &mut RefAny) -> Option<usize> {
    shared
        .downcast_ref::<PaletteShared>()
        .and_then(|s| s.rows.first().copied())
}

/// The backdrop: a click outside the panel closes.
extern "C" fn on_backdrop_click(mut data: RefAny, info: CallbackInfo) -> Update {
    close(&mut data, info)
}

/// The panel swallows its clicks, so they never reach the backdrop.
extern "C" fn on_panel_click(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    Update::DoNothing
}

/// Typing: the app stores the query and rebuilds.
extern "C" fn on_query_text(mut data: RefAny, info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let update = {
        let Some(s) = data.downcast_ref::<PaletteShared>() else {
            return OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            };
        };
        match s.on_query.as_ref() {
            Some(ShellCommandPaletteOnQuery { callback, refany }) => {
                callback.invoke(refany.clone(), info, AzString::from(state.get_text()))
            }
            None => Update::DoNothing,
        }
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// In the field: Escape closes, Enter runs the first match, Down moves to
/// the first row.
extern "C" fn on_query_key(mut data: RefAny, mut info: CallbackInfo, _state: TextInputState) -> OnTextInputReturn {
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let update = match key {
        Some(VirtualKeyCode::Escape) => close(&mut data, info),
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => match first_row_command(&mut data) {
            Some(command) => run(&mut data, info, command),
            None => Update::DoNothing,
        },
        Some(VirtualKeyCode::Down) => {
            // The field sits somewhere inside the input row; the list is
            // the row's next sibling; its first child is the first row.
            let mut input_row = Some(info.get_hit_node());
            while let Some(n) = input_row {
                if roving::has_class(&info, n, INPUT_CLASS) {
                    break;
                }
                input_row = info.get_parent(n);
            }
            let first = input_row
                .and_then(|r| info.get_next_sibling(r))
                .and_then(|list| info.get_first_child(list))
                .filter(|row| roving::has_class(&info, *row, ROW_CLASS));
            if let Some(row) = first {
                info.prevent_default();
                info.set_focus(FocusTarget::Id(row));
            }
            Update::DoNothing
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// A row click runs its command.
extern "C" fn on_row_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let (mut shared, command) = {
        let Some(row) = data.downcast_ref::<RowRef>() else {
            return Update::DoNothing;
        };
        (row.shared.clone(), row.command)
    };
    run(&mut shared, info, command)
}

/// On a row: Up / Down / Home / End move the stop, Enter runs, Escape
/// closes.
extern "C" fn on_row_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(key) = roving::plain_key(&info.get_current_keyboard_state()) else {
        return Update::DoNothing;
    };
    let (mut shared, command) = {
        let Some(row) = data.downcast_ref::<RowRef>() else {
            return Update::DoNothing;
        };
        (row.shared.clone(), row.command)
    };
    let step = match key {
        VirtualKeyCode::Up => roving::Step::Previous,
        VirtualKeyCode::Down => roving::Step::Next,
        VirtualKeyCode::Home => roving::Step::First,
        VirtualKeyCode::End => roving::Step::Last,
        VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => {
            info.prevent_default();
            return run(&mut shared, info, command);
        }
        VirtualKeyCode::Escape => {
            info.prevent_default();
            return close(&mut shared, info);
        }
        _ => return Update::DoNothing,
    };
    let me = info.get_hit_node();
    let Some(parent) = info.get_parent(me) else {
        return Update::DoNothing;
    };
    let items = roving::items_of(&info, parent, ROW_CLASS);
    let Some(current) = items.iter().position(|n| *n == me) else {
        return Update::DoNothing;
    };
    let Some(target) = roving::step_target(current, items.len(), step, false) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    roving::move_stop(&mut info, &items, target);
    Update::DoNothing
}

// ---------------------------------------------------------------------------
// The build
// ---------------------------------------------------------------------------

fn hook(event: EventFilter, cb: usize, refany: RefAny) -> CoreCallbackData {
    CoreCallbackData {
        event,
        callback: CoreCallback {
            cb,
            ctx: OptionRefAny::None,
        },
        refany,
    }
}

/// One result row: row `row_index` of the list, for command `index` of the
/// table; the first row is the selected one and the list's one Tab stop.
fn row(
    command: &ShellPaletteCommand,
    index: usize,
    row_index: usize,
    shared: &RefAny,
    look: &ShellLook,
) -> Dom {
    let selected = row_index == 0;
    let base = part(ITEM_BASE, &look.palette_row);
    let css = if selected {
        stack_state(&base, &look.palette_row_selected)
    } else {
        base
    };
    let icon = if command.icon.as_str().is_empty() {
        Dom::create_div()
    } else {
        Dom::create_icon(command.icon.clone())
    };
    let mut children: Vec<Dom> = alloc::vec![
        icon.with_class(AzString::from_const_str(ROW_ICON_CLASS))
            .with_css_props(part(LABEL_BASE, &look.palette_row_icon)),
        text(command.label.clone())
            .with_class(AzString::from_const_str(ROW_LABEL_CLASS))
            .with_css_props(part(GROW_LABEL_BASE, &look.palette_row_label)),
    ];
    if !command.shortcut.as_str().is_empty() {
        children.push(
            text(command.shortcut.clone())
                .with_class(AzString::from_const_str(ROW_SHORTCUT_CLASS))
                .with_css_props(part(LABEL_BASE, &look.palette_row_shortcut)),
        );
    }
    let row_ref = RefAny::new(RowRef {
        shared: shared.clone(),
        command: index,
    });
    Dom::create_div()
        .with_ids_and_classes(state_classes(ROW_CLASS, selected, ROW_SELECTED_CLASS))
        .with_css_props(css)
        .with_tab_index(roving::item_tab_index(row_index, 0))
        .with_accessibility_info(AccessibilityInfo {
            states: if selected {
                AccessibilityStateVec::from_vec(alloc::vec![AccessibilityState::Selected])
            } else {
                AccessibilityStateVec::from_const_slice(&[])
            },
            ..AccessibilityInfo::named(command.label.clone(), AccessibilityRole::ListItem)
        })
        .with_callbacks(
            alloc::vec![
                hook(
                    EventFilter::Hover(HoverEventFilter::Click),
                    on_row_click as usize,
                    row_ref.clone()
                ),
                hook(
                    EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    on_row_key as usize,
                    row_ref
                ),
            ]
            .into(),
        )
        .with_children(DomVec::from_vec(children))
}

/// The palette's DOM in `look`: backdrop [panel [input row, list [rows] |
/// empty line]], or an empty closed node.
pub(crate) fn build(palette: ShellCommandPalette, look: &ShellLook) -> Dom {
    if !palette.is_open {
        return Dom::create_div().with_class(AzString::from_const_str(CLOSED_CLASS));
    }
    let matching = palette.matching();
    let ShellCommandPalette {
        commands,
        query,
        placeholder,
        on_query,
        on_run,
        on_close,
        theme,
        is_open: _,
    } = palette;
    let inner = inner_theme(theme);
    let shared = RefAny::new(PaletteShared {
        on_query,
        on_run,
        on_close,
        rows: matching.clone(),
    });

    let mut field = TextInput::create_search()
        .with_text(query)
        .with_placeholder(placeholder)
        .with_accessibility_name("Command")
        .with_on_text_input(shared.clone(), on_query_text as TextInputOnTextInputCallbackType)
        .with_on_virtual_key_down(shared.clone(), on_query_key as TextInputOnVirtualKeyDownCallbackType);
    if let Some(t) = inner {
        field = field.with_theme(t);
    }
    let input_row = Dom::create_div()
        .with_class(AzString::from_const_str(INPUT_CLASS))
        .with_css_props(part(CHROME_ROW_BASE, &look.palette_input))
        .with_child(field.dom());

    let commands = commands.as_ref();
    let rows: Vec<Dom> = matching
        .iter()
        .enumerate()
        .map(|(row_index, command_index)| {
            row(&commands[*command_index], *command_index, row_index, &shared, look)
        })
        .collect();
    let list = if rows.is_empty() {
        Dom::create_div()
            .with_class(AzString::from_const_str(LIST_CLASS))
            .with_css_props(part(SCROLL_COLUMN_BASE, &look.palette_list))
            .with_child(
                text(AzString::from_const_str("No matching commands"))
                    .with_class(AzString::from_const_str(EMPTY_CLASS))
                    .with_css_props(part(LABEL_BASE, &look.palette_empty)),
            )
    } else {
        Dom::create_div()
            .with_class(AzString::from_const_str(LIST_CLASS))
            .with_css_props(part(SCROLL_COLUMN_BASE, &look.palette_list))
            .with_accessibility_info(AccessibilityInfo::named("Commands", AccessibilityRole::List))
            .with_children(DomVec::from_vec(rows))
    };

    let panel = Dom::create_div()
        .with_class(AzString::from_const_str(PANEL_CLASS))
        .with_css_props(part(super::COLUMN_BASE, &look.palette_panel))
        .with_accessibility_info(AccessibilityInfo::named("Command palette", AccessibilityRole::Dialog))
        .with_callbacks(
            alloc::vec![hook(
                EventFilter::Hover(HoverEventFilter::Click),
                on_panel_click as usize,
                shared.clone()
            )]
            .into(),
        )
        .with_children(DomVec::from_vec(alloc::vec![input_row, list]));

    Dom::create_div()
        .with_ids_and_classes(root_classes(PALETTE_CLASS, look))
        .with_css_props(part(OVERLAY_BASE, &look.palette_backdrop))
        .with_callbacks(
            alloc::vec![hook(
                EventFilter::Hover(HoverEventFilter::Click),
                on_backdrop_click as usize,
                shared
            )]
            .into(),
        )
        .with_child(panel)
}

#[cfg(test)]
mod command_palette_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, NodeType, TabIndex},
        id::NodeId,
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        shells::fixtures::command_palette,
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    fn indices_of(dom: &Dom, class: &str) -> Vec<usize> {
        tc::nodes(dom)
            .iter()
            .enumerate()
            .filter(|(_, (_, n))| tc::has_class(n, class))
            .map(|(i, _)| i)
            .collect()
    }

    fn node(index: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }
    }

    #[test]
    fn a_query_matches_as_a_subsequence_ignoring_case_and_spaces() {
        assert!(palette_matches("", "New message"));
        assert!(palette_matches("new", "New message"));
        assert!(palette_matches("nmsg", "New message"));
        assert!(palette_matches("NEW MSG", "New message"));
        assert!(!palette_matches("newx", "New message"));
        assert!(!palette_matches("gn", "New message"), "order matters");
        let c = ShellPaletteCommand::create(AzString::from("Archive")).with_category(AzString::from("Mail"));
        assert_eq!(c.search_text(), "Mail: Archive");
        assert!(palette_matches("mail arch", &c.search_text()));
    }

    #[test]
    fn a_closed_palette_is_an_empty_node_and_an_open_one_a_backdrop_over_a_panel() {
        let closed = ShellCommandPalette::create().with_theme(UiTheme::Flat).dom();
        assert!(tc::has_class(&closed, CLOSED_CLASS));
        assert!(closed.children.as_ref().is_empty());
        let dom = command_palette().with_theme(UiTheme::Flat).dom();
        assert!(tc::has_class(&dom, PALETTE_CLASS));
        let panel = &dom.children.as_ref()[0];
        assert!(tc::has_class(panel, PANEL_CLASS));
        assert_eq!(
            panel.root.get_accessibility_info().map(|i| i.role),
            Some(AccessibilityRole::Dialog)
        );
        let kids = panel.children.as_ref();
        assert_eq!(kids.len(), 2, "the input row, the list");
        assert!(tc::has_class(&kids[0], INPUT_CLASS));
        assert!(tc::has_class(&kids[1], LIST_CLASS));
        assert_eq!(tc::find_all(&dom, ROW_CLASS).len(), 3);
    }

    #[test]
    fn the_query_filters_the_rows_and_the_first_row_is_the_stop() {
        let dom = command_palette()
            .with_query(AzString::from("arch"))
            .with_theme(UiTheme::Flat)
            .dom();
        let rows = tc::find_all(&dom, ROW_CLASS);
        assert_eq!(rows.len(), 1);
        assert!(tc::has_class(rows[0], ROW_SELECTED_CLASS));
        assert_eq!(rows[0].root.get_tab_index(), Some(TabIndex::Auto));
        let label = tc::find(rows[0], ROW_LABEL_CLASS).expect("label");
        assert!(matches!(
            label.children.as_ref()[0].root.get_node_type(),
            NodeType::Text(s) if s.as_ref().as_str() == "Archive"
        ));
        let none = command_palette()
            .with_query(AzString::from("zzz"))
            .with_theme(UiTheme::Flat)
            .dom();
        assert!(tc::find_all(&none, ROW_CLASS).is_empty());
        assert!(tc::find(&none, EMPTY_CLASS).is_some());
    }

    #[test]
    fn a_row_shows_its_icon_label_and_shortcut_and_only_the_first_is_a_tab_stop() {
        let dom = command_palette().with_theme(UiTheme::Flat).dom();
        let rows = tc::find_all(&dom, ROW_CLASS);
        assert_eq!(rows[0].children.as_ref().len(), 3, "icon, label, shortcut");
        assert!(matches!(rows[0].children.as_ref()[0].root.get_node_type(), NodeType::Icon(_)));
        assert_eq!(rows[1].children.as_ref().len(), 2, "no shortcut");
        assert_eq!(rows[0].root.get_tab_index(), Some(TabIndex::Auto));
        assert_eq!(rows[1].root.get_tab_index(), Some(TabIndex::NoKeyboardFocus));
        assert_eq!(rows[2].root.get_tab_index(), Some(TabIndex::NoKeyboardFocus));
    }

    type Log = Arc<Mutex<Vec<String>>>;

    extern "C" fn record_run(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(alloc::format!("run {index}"));
        }
        Update::DoNothing
    }

    extern "C" fn record_close(mut data: RefAny, _info: CallbackInfo) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(String::from("close"));
        }
        Update::DoNothing
    }

    fn logged(query: &str) -> (Dom, Log) {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = command_palette()
            .with_query(AzString::from(query))
            .with_on_run(RefAny::new(log.clone()), record_run as ShellCommandPaletteOnRunCallbackType)
            .with_on_close(RefAny::new(log.clone()), record_close as crate::widgets::button::ButtonOnClickCallbackType)
            .with_theme(UiTheme::Flat)
            .dom();
        (dom, log)
    }

    #[test]
    fn enter_on_a_row_runs_its_command_by_table_index_and_escape_closes() {
        let (dom, log) = logged("arch");
        let styled = StyledDom::create_from_dom(dom.clone());
        let rows = indices_of(&dom, ROW_CLASS);
        let (_, changes) = rv::press(&styled, node(rows[0]), VirtualKeyCode::Return, &[]).expect("keys");
        assert!(rv::prevented(&changes));
        rv::press(&styled, node(rows[0]), VirtualKeyCode::Escape, &[]).expect("keys");
        rv::fire(&styled, node(rows[0]), EventFilter::Hover(HoverEventFilter::Click)).expect("click");
        assert_eq!(*log.lock().expect("log"), vec!["run 1", "close", "run 1"], "Archive is command 1");
    }

    #[test]
    fn down_and_up_move_the_stop_between_the_rows() {
        let (dom, _) = logged("");
        let styled = StyledDom::create_from_dom(dom.clone());
        let rows = indices_of(&dom, ROW_CLASS);
        let (_, changes) = rv::press(&styled, node(rows[0]), VirtualKeyCode::Down, &[]).expect("keys");
        assert_eq!(rv::focus_request(&changes), Some(node(rows[1])));
        let (_, changes) = rv::press(&styled, node(rows[2]), VirtualKeyCode::Up, &[]).expect("keys");
        assert_eq!(rv::focus_request(&changes), Some(node(rows[1])));
        let (_, changes) = rv::press(&styled, node(rows[1]), VirtualKeyCode::End, &[]).expect("keys");
        assert_eq!(rv::focus_request(&changes), Some(node(rows[2])));
    }

    #[test]
    fn a_backdrop_click_closes_and_a_panel_click_does_not() {
        let (dom, log) = logged("");
        let styled = StyledDom::create_from_dom(dom.clone());
        let panel = indices_of(&dom, PANEL_CLASS)[0];
        rv::fire(&styled, node(panel), EventFilter::Hover(HoverEventFilter::Click)).expect("panel");
        assert!(log.lock().expect("log").is_empty());
        rv::fire(&styled, node(0), EventFilter::Hover(HoverEventFilter::Click)).expect("backdrop");
        assert_eq!(*log.lock().expect("log"), vec!["close"]);
    }

    #[test]
    fn a_palette_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "command_palette",
            || command_palette().dom(),
            |t: UiTheme| command_palette().with_theme(t).dom(),
        );
    }
}
