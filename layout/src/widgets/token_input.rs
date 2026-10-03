//! Token input widget - chips in a text field: the recipients of a mail
//! (To / Cc), a meeting's attendees, a task's tags, a file's labels. The
//! tokens are `Chip`s with a remove button, the text entry follows them,
//! and a list of suggestions under the field offers what the typed text
//! matches.
//!
//! THE APP OWNS THE STATE ([`TokenInputState`]: the tokens, the typed text,
//! the highlighted suggestion): every action reports a [`TokenInputEvent`]
//! whose `state` is the NEXT state - the app stores it and rebuilds (the
//! data table's view pattern). The kinds say what else happened:
//!
//! - `Add` (a token was committed: Enter, Tab, a `,` or `;` typed, a paste
//!   of a list, a suggestion picked; `token` is the last one added);
//! - `Remove` (a chip's "x", Backspace in an empty entry, Delete /
//!   Backspace on a focused chip; `index` / `token` say which);
//! - `Text` (the typed text changed: the app may fetch suggestions for it);
//! - `Navigate` (Up / Down moved the highlighted suggestion);
//! - `Refuse` (the app's validator refused a token: `message` says why, the
//!   text stays in the entry to be fixed);
//! - `Open` (a click on a chip's label: open the contact, the tag).
//!
//! VALIDATION: an optional `on_validate` callback is asked for every token
//! before it is added; it accepts (optionally with a normalised token:
//! "Bob <bob@x.org>" -> "bob@x.org") or refuses with a reason. Without one,
//! every non-empty token is taken. A token that is already there (case
//! folded) is not added twice unless [`TokenInput::with_allow_duplicates`].
//!
//! SUGGESTIONS: the app hands in candidates ([`TokenInput::with_suggestions`]
//! - all of them, or what it fetched for the typed text); the widget shows
//! the ones the typed text matches, those that START with it first, never
//! one that is a token already, at most [`TokenInput::max_suggestions`].
//! Nothing typed: no list.
//!
//! KEYBOARD: the field is ONE Tab stop, the entry. Enter commits the
//! highlighted suggestion or the typed text, Tab commits the typed text
//! (and leaves when there is none), `,` / `;` commit what is before them,
//! Down / Up move the highlight, Escape hides the list, Backspace in an
//! empty entry removes the last chip, Left in an empty entry goes to the
//! chips: Left / Right walk them (Right past the last returns to the
//! entry), Delete / Backspace remove the focused one, Enter / Space press
//! its "x".
//!
//! Key types: [`TokenInput`], [`TokenInputState`], [`TokenInputEvent`],
//! [`TokenInputVerdict`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
    callbacks::{FocusTarget, Update},
    dom::{Dom, DomNodeId, DomVec, EventFilter, HoverEventFilter, IdOrClass::Class, IdOrClassVec, TabIndex},
    events::FocusEventFilter,
    refany::RefAny,
    window::VirtualKeyCode,
};
use azul_css::{
    corety::OptionUsize,
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{length::FloatValue, pixel::PixelValue},
        layout::{
            LayoutAlignItems, LayoutBoxSizing, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutFlexWrap, LayoutLeft, LayoutMinWidth, LayoutPosition, LayoutTop,
            LayoutZIndex,
        },
        property::CssProperty,
        style::StyleCursor,
    },
    AzString, StringVec,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        chip::{Chip, ChipOnClickCallbackType, ChipOnRemoveCallbackType, ChipState},
        roving,
        text_input::{
            OnTextInputReturn, TextInput, TextInputOnTextInputCallbackType,
            TextInputOnVirtualKeyDownCallbackType, TextInputState, TextInputValid,
        },
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The widget's class.
pub(crate) const TOKEN_INPUT_CLASS: &str = "__azul-native-token-input";
/// The field: the chips and the entry, in a box that looks like a text field.
pub(crate) const FIELD_CLASS: &str = "__azul-native-token-input-field";
/// A token's chip (added to the chip's own class).
pub(crate) const CHIP_CLASS: &str = "__azul-native-token-input-chip";
/// The text entry after the chips (added to the text input's own class).
pub(crate) const ENTRY_CLASS: &str = "__azul-native-token-input-entry";
/// The list of suggestions under the field.
pub(crate) const LIST_CLASS: &str = "__azul-native-token-input-suggestions";
/// A suggestion.
pub(crate) const OPTION_CLASS: &str = "__azul-native-token-input-suggestion";
/// The highlighted suggestion (added to [`OPTION_CLASS`]).
pub(crate) const OPTION_ACTIVE_CLASS: &str = "__azul-native-token-input-suggestion-active";

/// What ends a token while typing (and splits a pasted list).
pub const TOKEN_INPUT_SEPARATORS: &[char] = &[',', ';', '\n', '\t'];

/// How many suggestions show when the app says nothing.
pub const TOKEN_INPUT_MAX_SUGGESTIONS: usize = 8;

// ==== Types ====

/// The state of a token input the APP keeps. Every [`TokenInputEvent`]
/// carries the next one; store it and rebuild.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenInputState {
    /// The tokens, in order.
    pub tokens: StringVec,
    /// What is typed in the entry (not yet a token).
    pub text: AzString,
    /// The highlighted suggestion: its place among the suggestions SHOWN.
    pub active: OptionUsize,
}

impl TokenInputState {
    /// `tokens`, nothing typed, nothing highlighted.
    #[must_use]
    pub const fn create(tokens: StringVec) -> Self {
        Self {
            tokens,
            text: AzString::from_const_str(""),
            active: OptionUsize::None,
        }
    }

    /// The typed text.
    pub fn set_text(&mut self, text: AzString) {
        self.text = text;
    }

    /// [`Self::set_text`] for the builder chain.
    #[must_use]
    pub fn with_text(mut self, text: AzString) -> Self {
        self.set_text(text);
        self
    }

    /// The highlighted suggestion (its place among the ones shown).
    pub fn set_active(&mut self, active: usize) {
        self.active = OptionUsize::Some(active);
    }

    /// [`Self::set_active`] for the builder chain.
    #[must_use]
    pub fn with_active(mut self, active: usize) -> Self {
        self.set_active(active);
        self
    }
}

impl Default for TokenInputState {
    fn default() -> Self {
        Self::create(StringVec::from_const_slice(&[]))
    }
}

/// What happened in the token input. Every event carries the next
/// [`TokenInputState`]; the kinds say what ELSE happened.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TokenInputEventKind {
    /// One or more tokens were added (`token`: the last one).
    Add,
    /// Token `index` (`token`) was removed.
    Remove,
    /// The typed text changed.
    Text,
    /// The highlighted suggestion moved.
    Navigate,
    /// The validator refused `token`: `message` says why.
    Refuse,
    /// Token `index` (`token`) was clicked: open it.
    Open,
}

/// One action in the token input.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenInputEvent {
    /// The state after the action: store it.
    pub state: TokenInputState,
    /// `Add`: the last token added; `Remove` / `Open`: the token;
    /// `Refuse`: the refused token.
    pub token: AzString,
    /// `Refuse`: why.
    pub message: AzString,
    /// `Remove` / `Open`: the token's index; `Add`: the first added token's.
    pub index: usize,
    /// What happened.
    pub kind: TokenInputEventKind,
}

impl TokenInputEvent {
    /// A `kind` event leaving `state`, nothing else set.
    #[must_use]
    pub const fn create(kind: TokenInputEventKind, state: TokenInputState) -> Self {
        Self {
            state,
            token: AzString::from_const_str(""),
            message: AzString::from_const_str(""),
            index: 0,
            kind,
        }
    }
}

/// The validator's answer for one token.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenInputVerdict {
    /// Accepted: the token to add (empty = as typed).
    pub token: AzString,
    /// Refused: why (shown to the user).
    pub message: AzString,
    /// The token is taken.
    pub accepted: bool,
}

impl TokenInputVerdict {
    /// The token is taken - as `token` (empty: as typed).
    #[must_use]
    pub const fn create_accepted(token: AzString) -> Self {
        Self {
            token,
            message: AzString::from_const_str(""),
            accepted: true,
        }
    }

    /// The token is refused, for `message`.
    #[must_use]
    pub const fn create_refused(message: AzString) -> Self {
        Self {
            token: AzString::from_const_str(""),
            message,
            accepted: false,
        }
    }
}

impl azul_core::host_invoker::HostOut for TokenInputVerdict {
    fn unwritten() -> Self {
        // A host that does not answer validates nothing: the token is taken
        // as typed.
        Self::create_accepted(AzString::from_const_str(""))
    }
}

/// Callback invoked for an action in the token input.
pub type TokenInputOnEventCallbackType = extern "C" fn(RefAny, CallbackInfo, TokenInputEvent) -> Update;
impl_widget_callback!(
    TokenInputOnEvent,
    OptionTokenInputOnEvent,
    TokenInputOnEventCallback,
    TokenInputOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TokenInputOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TOKEN_INPUT_ON_EVENT_INVOKER,
    invoker_ty:     AzTokenInputOnEventCallbackInvoker,
    thunk_fn:       az_token_input_on_event_callback_thunk,
    setter_fn:      AzApp_setTokenInputOnEventCallbackInvoker,
    from_handle_fn: AzTokenInputOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzTokenInputOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: TokenInputEvent ],
}

/// The VALIDATE callback: the app accepts (and may normalise) or refuses a
/// token before it is added.
pub type TokenInputOnValidateCallbackType =
    extern "C" fn(RefAny, CallbackInfo, AzString) -> TokenInputVerdict;
impl_widget_callback!(
    TokenInputOnValidate,
    OptionTokenInputOnValidate,
    TokenInputOnValidateCallback,
    TokenInputOnValidateCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TokenInputOnValidateCallback,
    info_ty:        CallbackInfo,
    return_ty:      TokenInputVerdict,
    default_ret:    TokenInputVerdict::create_accepted(AzString::from_const_str("")),
    invoker_static: TOKEN_INPUT_ON_VALIDATE_INVOKER,
    invoker_ty:     AzTokenInputOnValidateCallbackInvoker,
    thunk_fn:       az_token_input_on_validate_callback_thunk,
    setter_fn:      AzApp_setTokenInputOnValidateCallbackInvoker,
    from_handle_fn: AzTokenInputOnValidateCallback_createFromHostHandle,
    from_handle_byref_fn: AzTokenInputOnValidateCallback_createFromHostHandleByref,
    extra_args:     [ token: AzString ],
}

/// The token input (module docs).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct TokenInput {
    /// The app-owned state: tokens, typed text, highlighted suggestion.
    pub state: TokenInputState,
    /// The candidates the suggestions are picked from.
    pub suggestions: StringVec,
    /// The entry's prompt while nothing is typed ("Add people").
    pub placeholder: AzString,
    /// What a screen reader calls the field ("To").
    pub accessibility_name: AzString,
    /// Hears every action.
    pub on_event: OptionTokenInputOnEvent,
    /// Accepts or refuses each token; none = every token is taken.
    pub on_validate: OptionTokenInputOnValidate,
    /// How many suggestions show at most.
    pub max_suggestions: usize,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
    /// The same token may be added twice.
    pub allow_duplicates: bool,
}

impl TokenInput {
    /// A token input holding `tokens`, a screen reader calls it
    /// `accessibility_name`.
    #[must_use]
    pub const fn create(tokens: StringVec, accessibility_name: AzString) -> Self {
        Self {
            state: TokenInputState::create(tokens),
            suggestions: StringVec::from_const_slice(&[]),
            placeholder: AzString::from_const_str(""),
            accessibility_name,
            on_event: OptionTokenInputOnEvent::None,
            on_validate: OptionTokenInputOnValidate::None,
            max_suggestions: TOKEN_INPUT_MAX_SUGGESTIONS,
            theme: OptionUiTheme::None,
            allow_duplicates: false,
        }
    }

    /// The whole state (what the last event carried).
    pub fn set_state(&mut self, state: TokenInputState) {
        self.state = state;
    }

    /// [`Self::set_state`] for the builder chain.
    #[must_use]
    pub fn with_state(mut self, state: TokenInputState) -> Self {
        self.set_state(state);
        self
    }

    /// The typed text.
    pub fn set_text(&mut self, text: AzString) {
        self.state.text = text;
    }

    /// [`Self::set_text`] for the builder chain.
    #[must_use]
    pub fn with_text(mut self, text: AzString) -> Self {
        self.set_text(text);
        self
    }

    /// The candidates the suggestions are picked from.
    pub fn set_suggestions(&mut self, suggestions: StringVec) {
        self.suggestions = suggestions;
    }

    /// [`Self::set_suggestions`] for the builder chain.
    #[must_use]
    pub fn with_suggestions(mut self, suggestions: StringVec) -> Self {
        self.set_suggestions(suggestions);
        self
    }

    /// The entry's prompt.
    pub fn set_placeholder(&mut self, placeholder: AzString) {
        self.placeholder = placeholder;
    }

    /// [`Self::set_placeholder`] for the builder chain.
    #[must_use]
    pub fn with_placeholder(mut self, placeholder: AzString) -> Self {
        self.set_placeholder(placeholder);
        self
    }

    /// What a screen reader calls the field.
    pub fn set_accessibility_name(&mut self, name: AzString) {
        self.accessibility_name = name;
    }

    /// [`Self::set_accessibility_name`] for the builder chain.
    #[must_use]
    pub fn with_accessibility_name(mut self, name: AzString) -> Self {
        self.set_accessibility_name(name);
        self
    }

    /// Every action in the field.
    pub fn set_on_event<C: Into<TokenInputOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = OptionTokenInputOnEvent::Some(TokenInputOnEvent {
            refany: data,
            callback: callback.into(),
        });
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<TokenInputOnEventCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// The validator asked for every token before it is added.
    pub fn set_on_validate<C: Into<TokenInputOnValidateCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_validate = OptionTokenInputOnValidate::Some(TokenInputOnValidate {
            refany: data,
            callback: callback.into(),
        });
    }

    /// [`Self::set_on_validate`] for the builder chain.
    #[must_use]
    pub fn with_on_validate<C: Into<TokenInputOnValidateCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_validate(data, callback);
        self
    }

    /// How many suggestions show at most.
    pub const fn set_max_suggestions(&mut self, max: usize) {
        self.max_suggestions = max;
    }

    /// [`Self::set_max_suggestions`] for the builder chain.
    #[must_use]
    pub const fn with_max_suggestions(mut self, max: usize) -> Self {
        self.set_max_suggestions(max);
        self
    }

    /// Whether the same token may be added twice.
    pub const fn set_allow_duplicates(&mut self, allow: bool) {
        self.allow_duplicates = allow;
    }

    /// [`Self::set_allow_duplicates`] for the builder chain.
    #[must_use]
    pub const fn with_allow_duplicates(mut self, allow: bool) -> Self {
        self.set_allow_duplicates(allow);
        self
    }

    /// Pin the widget theme; unset, the field follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty field and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// The field's DOM, in the pinned theme's look or both looks merged
    /// (built once; the chips and the entry follow the app theme with it).
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        build(self, &look)
    }
}

impl Default for TokenInput {
    fn default() -> Self {
        Self::create(StringVec::from_const_slice(&[]), AzString::from_const_str("Tokens"))
    }
}

impl From<TokenInput> for Dom {
    fn from(t: TokenInput) -> Self {
        t.dom()
    }
}

// ==== The rules, without a window ====

/// `text` split at the separators: the tokens BEFORE the last separator
/// (trimmed, empty ones dropped) and what follows it (the text still being
/// typed, its leading blanks dropped).
#[must_use]
pub(crate) fn split_tokens(text: &str) -> (Vec<String>, String) {
    let mut parts: Vec<&str> = text.split(TOKEN_INPUT_SEPARATORS).collect();
    // `split` always yields at least one part: the text after the last
    // separator (all of it when there is none).
    let rest = parts.pop().unwrap_or("").trim_start();
    let tokens = parts
        .into_iter()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect();
    (tokens, String::from(rest))
}

/// `text` case folded and trimmed - what two tokens are compared by.
fn folded(text: &str) -> String {
    text.trim().to_lowercase()
}

/// Whether two tokens are the same (case folded, trimmed).
#[must_use]
pub(crate) fn same_token(a: &str, b: &str) -> bool {
    folded(a) == folded(b)
}

/// `state` with `tokens` added at the end (trimmed; empty ones, and - unless
/// `allow_duplicates` - ones already there, skipped), the text cleared and
/// nothing highlighted. Also returns how many were added.
#[must_use]
pub(crate) fn add_tokens(
    state: &TokenInputState,
    tokens: &[String],
    allow_duplicates: bool,
) -> (TokenInputState, usize) {
    let mut all: Vec<AzString> = state.tokens.as_ref().to_vec();
    let mut added = 0usize;
    for token in tokens {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if !allow_duplicates && all.iter().any(|t| same_token(t.as_str(), token)) {
            continue;
        }
        all.push(AzString::from(token));
        added += 1;
    }
    let next = TokenInputState {
        tokens: StringVec::from_vec(all),
        text: AzString::from_const_str(""),
        active: OptionUsize::None,
    };
    (next, added)
}

/// `state` without token `index` (out of range: unchanged).
#[must_use]
pub(crate) fn remove_token(state: &TokenInputState, index: usize) -> TokenInputState {
    if index >= state.tokens.len() {
        return state.clone();
    }
    let mut tokens: Vec<AzString> = state.tokens.as_ref().to_vec();
    tokens.remove(index);
    TokenInputState {
        tokens: StringVec::from_vec(tokens),
        text: state.text.clone(),
        active: state.active,
    }
}

/// The suggestions the typed `text` shows, as indices into `suggestions`:
/// the ones starting with it (case folded) first, then the ones containing
/// it, never one that is a token already, at most `max`. Nothing typed:
/// none.
#[must_use]
pub(crate) fn matching_suggestions(
    suggestions: &[AzString],
    text: &str,
    tokens: &[AzString],
    max: usize,
) -> Vec<usize> {
    let typed = folded(text);
    if typed.is_empty() || max == 0 {
        return Vec::new();
    }
    let candidates: Vec<(usize, String)> = suggestions
        .iter()
        .enumerate()
        .filter(|(_, s)| !tokens.iter().any(|t| same_token(t.as_str(), s.as_str())))
        .map(|(i, s)| (i, folded(s.as_str())))
        .collect();
    let starting = candidates.iter().filter(|(_, s)| s.starts_with(typed.as_str()));
    let containing = candidates
        .iter()
        .filter(|(_, s)| !s.starts_with(typed.as_str()) && s.contains(typed.as_str()));
    starting.chain(containing).map(|(i, _)| *i).take(max).collect()
}

/// The highlight after Down (`down`) or Up over `count` suggestions shown:
/// wrapping, from nothing to the first (Down) or the last (Up).
#[must_use]
pub(crate) fn step_active(active: Option<usize>, count: usize, down: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let last = count - 1;
    Some(match (active.filter(|a| *a <= last), down) {
        (None, true) => 0,
        (None, false) => last,
        (Some(a), true) => {
            if a == last {
                0
            } else {
                a + 1
            }
        }
        (Some(a), false) => {
            if a == 0 {
                last
            } else {
                a - 1
            }
        }
    })
}

/// What a key in the entry does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryKey {
    /// Not the token input's: the text input does what it does.
    Pass,
    /// Commit the typed text.
    CommitText,
    /// Commit the highlighted suggestion (its place among the shown ones).
    CommitSuggestion(usize),
    /// Remove the last token.
    RemoveLast,
    /// Move the highlight.
    Navigate(Option<usize>),
    /// Hide the suggestions.
    Dismiss,
    /// Go to the last chip.
    ToChips,
}

/// What `key` does in the entry: `text` is typed (`text_empty`: nothing
/// is), there are `tokens` tokens, `shown` suggestions show, `active` is
/// highlighted; `modified` = a modifier (Shift, Ctrl, Alt, Cmd) is held.
#[must_use]
pub(crate) fn entry_key(
    key: VirtualKeyCode,
    text_empty: bool,
    tokens: usize,
    shown: usize,
    active: Option<usize>,
    modified: bool,
) -> EntryKey {
    use VirtualKeyCode as K;
    if modified {
        return EntryKey::Pass;
    }
    match key {
        K::Return | K::NumpadEnter => match active.filter(|a| *a < shown) {
            Some(a) => EntryKey::CommitSuggestion(a),
            None if !text_empty => EntryKey::CommitText,
            None => EntryKey::Pass,
        },
        K::Tab if !text_empty => EntryKey::CommitText,
        K::Back if text_empty && tokens > 0 => EntryKey::RemoveLast,
        K::Down if shown > 0 => EntryKey::Navigate(step_active(active, shown, true)),
        K::Up if shown > 0 => EntryKey::Navigate(step_active(active, shown, false)),
        K::Escape if shown > 0 => EntryKey::Dismiss,
        K::Left if text_empty && tokens > 0 => EntryKey::ToChips,
        _ => EntryKey::Pass,
    }
}

// ==== The look and the DOM ====

/// What a theme decides about a token input: the SKIN of each part, laid
/// over the part's base by [`build`]. The chips are `Chip`s and the entry a
/// `TextInput`, each in its own look.
#[derive(Debug, Clone, Default)]
pub(crate) struct TokenInputLook {
    /// The whole widget (its font).
    pub root: Vec<CssPropertyWithConditions>,
    /// The field: the box that looks like a text field around the chips and
    /// the entry.
    pub field: Vec<CssPropertyWithConditions>,
    /// Added to the entry: its focus ring (the field cannot show one until
    /// the engine raises `:focus-within`).
    pub entry: Vec<CssPropertyWithConditions>,
    /// The suggestions' list.
    pub list: Vec<CssPropertyWithConditions>,
    /// A suggestion.
    pub option: Vec<CssPropertyWithConditions>,
    /// Stacked on the highlighted suggestion.
    pub option_active: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the widget, if it has one.
    pub marker: Option<&'static str>,
}

/// The look a token input with the theme option `theme` is built with: the
/// pinned theme's own look, or both looks merged part by part (the DOM is
/// built once).
pub(crate) fn look_for(theme: OptionUiTheme) -> TokenInputLook {
    use crate::widgets::themes::{flat, flora, theme_blocks::follow_props};
    match theme.into_option() {
        Some(UiTheme::Flat) => flat::token_input_look(),
        Some(UiTheme::Flora) => flora::token_input_look(),
        None => {
            let (a, b) = (flat::token_input_look(), flora::token_input_look());
            let both = |x: &[CssPropertyWithConditions], y: &[CssPropertyWithConditions]| {
                follow_props(x, y).into_library_owned_vec()
            };
            TokenInputLook {
                root: both(&a.root, &b.root),
                field: both(&a.field, &b.field),
                entry: both(&a.entry, &b.entry),
                list: both(&a.list, &b.list),
                option: both(&a.option, &b.option),
                option_active: both(&a.option_active, &b.option_active),
                marker: match UiTheme::current() {
                    UiTheme::Flat => a.marker,
                    UiTheme::Flora => b.marker,
                },
            }
        }
    }
}

// ---- the base: the widget's structure, in every theme ----

/// A row or column gap, px.
fn gap(px: isize, column: bool) -> CssPropertyWithConditions {
    use azul_css::props::{
        layout::{LayoutColumnGap, LayoutRowGap},
        property::{LayoutColumnGapValue, LayoutRowGapValue},
    };
    let inner = PixelValue::const_px(px);
    CssPropertyWithConditions::simple(if column {
        CssProperty::ColumnGap(LayoutColumnGapValue::Exact(LayoutColumnGap { inner }))
    } else {
        CssProperty::RowGap(LayoutRowGapValue::Exact(LayoutRowGap { inner }))
    })
}

/// The widget: the field over the (floating) list, the list's positioning
/// context.
pub(crate) static TOKEN_INPUT_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// The field: the chips and the entry in rows that wrap, centred on each
/// line, a gap apart; a text cursor over it.
pub(crate) fn token_field_base() -> Vec<CssPropertyWithConditions> {
    alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_wrap(LayoutFlexWrap::Wrap)),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
        CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Text)),
        CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
        gap(4, true),
        gap(2, false),
    ]
}

/// What the entry (a `TextInput`) gets on top of its own container style:
/// no border and no face of its own (the field is the box), the rest of
/// the line to grow into, never narrower than a short address.
pub(crate) fn token_entry_overrides() -> Vec<CssPropertyWithConditions> {
    use azul_css::props::{
        basic::color::ColorU,
        style::{LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, LayoutBorderTopWidth},
    };
    alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(0))),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(0))),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(0))),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(0))),
        CssPropertyWithConditions::simple(crate::widgets::themes::decl::fill(ColorU::TRANSPARENT)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
        CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(1),
        })),
        CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(80))),
    ]
}

/// The list: floating under the field, as wide as it, over what follows.
pub(crate) static TOKEN_LIST_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
    CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop {
        inner: PixelValue::const_percent(100),
    })),
    CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft {
        inner: PixelValue::const_px(0),
    })),
    CssPropertyWithConditions::simple(CssProperty::const_width(azul_css::props::layout::LayoutWidth::Px(
        PixelValue::const_percent(100),
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    CssPropertyWithConditions::simple(CssProperty::const_z_index(LayoutZIndex::Integer(10))),
];

/// A suggestion: one line under the pointer.
pub(crate) static TOKEN_OPTION_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Default)),
];

/// A part's declarations: its base (the structure), then the look's skin.
fn part(base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
}

/// What every handler of one token input shares: the app's hooks, the
/// state the DOM was built from and the suggestions shown (in order).
struct TokenShared {
    on_event: OptionTokenInputOnEvent,
    on_validate: OptionTokenInputOnValidate,
    state: TokenInputState,
    shown: Vec<AzString>,
    allow_duplicates: bool,
}

/// A chip's (or a suggestion's) payload: its index (among the tokens, or
/// among the suggestions shown) and the shared part.
struct TokenData {
    index: usize,
    shared: RefAny,
}

/// The token input's DOM in `look`: root [field [chip.., entry], list?].
pub(crate) fn build(input: TokenInput, look: &TokenInputLook) -> Dom {
    let TokenInput {
        state,
        suggestions,
        placeholder,
        accessibility_name,
        on_event,
        on_validate,
        max_suggestions,
        theme,
        allow_duplicates,
    } = input;
    let theme = theme.into_option();
    let shown_indices = matching_suggestions(
        suggestions.as_ref(),
        state.text.as_str(),
        state.tokens.as_ref(),
        max_suggestions,
    );
    let shown: Vec<AzString> = shown_indices
        .iter()
        .filter_map(|i| suggestions.as_ref().get(*i).cloned())
        .collect();
    let active = state.active.into_option().filter(|a| *a < shown.len());
    let shared = RefAny::new(TokenShared {
        on_event,
        on_validate,
        state: state.clone(),
        shown: shown.clone(),
        allow_duplicates,
    });

    // The field: a chip per token, then the entry.
    let mut in_field: Vec<Dom> = Vec::with_capacity(state.tokens.len() + 1);
    for (index, token) in state.tokens.as_ref().iter().enumerate() {
        let data = RefAny::new(TokenData {
            index,
            shared: shared.clone(),
        });
        let mut chip = Chip::create(token.clone())
            .with_removable(true)
            .with_on_remove(data.clone(), on_chip_remove as ChipOnRemoveCallbackType)
            .with_on_click(data.clone(), on_chip_click as ChipOnClickCallbackType);
        if let Some(t) = theme {
            chip = chip.with_theme(t);
        }
        let mut chip = chip.dom();
        chip.add_class(AzString::from_const_str(CHIP_CLASS));
        // One Tab stop for the field: the chip's label and "x" are reached
        // by the arrows from the entry, not by Tab.
        let parts: &mut [Dom] = chip.children.as_mut();
        if let Some(label) = parts.first_mut() {
            label.root.set_tab_index(TabIndex::NoKeyboardFocus);
        }
        if let Some(x) = parts.get_mut(1) {
            x.root.set_tab_index(TabIndex::NoKeyboardFocus);
            x.root.add_callback(
                EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                data,
                on_chip_key as usize,
            );
        }
        in_field.push(chip);
    }

    let mut entry = TextInput::create()
        .with_text(state.text.clone())
        .with_accessibility_name(accessibility_name.clone())
        .with_on_text_input(shared.clone(), on_entry_text as TextInputOnTextInputCallbackType)
        .with_on_virtual_key_down(shared.clone(), on_entry_key as TextInputOnVirtualKeyDownCallbackType);
    if !placeholder.as_str().is_empty() {
        entry = entry.with_placeholder(placeholder);
    }
    let mut entry_style = entry.resolved_container_style().into_library_owned_vec();
    entry_style.extend(token_entry_overrides());
    entry_style.extend(look.entry.iter().cloned());
    entry = entry.with_container_style(CssPropertyWithConditionsVec::from_vec(entry_style));
    if let Some(t) = theme {
        entry = entry.with_theme(t);
    }
    let mut entry = entry.dom();
    entry.add_class(AzString::from_const_str(ENTRY_CLASS));
    in_field.push(entry);

    let field = Dom::create_div()
        .with_class(AzString::from_const_str(FIELD_CLASS))
        .with_css_props(part(&token_field_base(), &look.field))
        .with_children(DomVec::from_vec(in_field));

    let mut children = alloc::vec![field];
    if !shown.is_empty() {
        let options: Vec<Dom> = shown
            .iter()
            .enumerate()
            .map(|(position, text)| {
                let is_active = active == Some(position);
                let mut style = part(TOKEN_OPTION_BASE, &look.option);
                let mut classes = alloc::vec![Class(AzString::from_const_str(OPTION_CLASS))];
                if is_active {
                    style = crate::widgets::themes::theme_blocks::stack_parts(
                        &style,
                        &CssPropertyWithConditionsVec::from_vec(look.option_active.clone()),
                    );
                    classes.push(Class(AzString::from_const_str(OPTION_ACTIVE_CLASS)));
                }
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_vec(classes))
                    .with_css_props(style)
                    .with_accessibility_info(AccessibilityInfo {
                        role: AccessibilityRole::ListItem,
                        accessibility_name: Some(text.clone()).into(),
                        states: if is_active {
                            AccessibilityStateVec::from_vec(alloc::vec![AccessibilityState::Selected])
                        } else {
                            AccessibilityStateVec::from_const_slice(&[])
                        },
                        ..Default::default()
                    })
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::Click),
                        RefAny::new(TokenData {
                            index: position,
                            shared: shared.clone(),
                        }),
                        on_option_click as usize,
                    )
                    .with_child(crate::widgets::widget_p_with_text(text.clone()))
            })
            .collect();
        children.push(
            Dom::create_div()
                .with_class(AzString::from_const_str(LIST_CLASS))
                .with_css_props(part(TOKEN_LIST_BASE, &look.list))
                .with_accessibility_info(AccessibilityInfo {
                    role: AccessibilityRole::List,
                    accessibility_name: Some(AzString::from_const_str("Suggestions")).into(),
                    ..Default::default()
                })
                .with_children(DomVec::from_vec(options)),
        );
    }

    let mut classes = alloc::vec![Class(AzString::from_const_str(TOKEN_INPUT_CLASS))];
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(TOKEN_INPUT_BASE, &look.root))
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Grouping,
            accessibility_name: Some(accessibility_name).into(),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(children))
}

// ==== The callbacks ====

/// The payload's index and the shared part.
fn token_of(data: &mut RefAny) -> Option<(usize, RefAny)> {
    let d = data.downcast_ref::<TokenData>()?;
    Some((d.index, d.shared.clone()))
}

/// Hands `event` to the app.
fn emit(shared: &mut RefAny, info: CallbackInfo, event: TokenInputEvent) -> Update {
    let hook = match shared.downcast_ref::<TokenShared>() {
        Some(s) => s.on_event.clone(),
        None => return Update::DoNothing,
    };
    match hook.as_ref() {
        Some(TokenInputOnEvent { refany, callback }) => callback.invoke(refany.clone(), info, event),
        None => Update::DoNothing,
    }
}

/// The app's verdict on `token` (no validator: taken as typed).
fn validate(shared: &mut RefAny, info: CallbackInfo, token: &str) -> TokenInputVerdict {
    let hook = shared.downcast_ref::<TokenShared>().map(|s| s.on_validate.clone());
    match hook.as_ref().and_then(|h| h.as_ref()) {
        Some(TokenInputOnValidate { refany, callback }) => {
            callback.invoke(refany.clone(), info, AzString::from(token))
        }
        None => TokenInputVerdict::create_accepted(AzString::from_const_str("")),
    }
}

/// Validates and adds `parts` to the built state, `rest` staying typed:
/// `Add` (or `Text` when nothing new was added), or `Refuse` for the first
/// refused part - which then stays typed, before `rest`, to be fixed.
fn commit(shared: &mut RefAny, info: CallbackInfo, parts: Vec<String>, rest: String) -> Option<TokenInputEvent> {
    let (state, allow) = {
        let s = shared.downcast_ref::<TokenShared>()?;
        (s.state.clone(), s.allow_duplicates)
    };
    let mut accepted: Vec<String> = Vec::with_capacity(parts.len());
    let mut refused: Option<(String, AzString)> = None;
    for part in parts {
        let part = String::from(part.trim());
        if part.is_empty() {
            continue;
        }
        let verdict = validate(shared, info, &part);
        if verdict.accepted {
            let normalised = verdict.token.as_str().trim();
            accepted.push(if normalised.is_empty() {
                part
            } else {
                String::from(normalised)
            });
        } else if refused.is_none() {
            refused = Some((part, verdict.message));
        }
    }
    let first = state.tokens.len();
    let (mut next, added) = add_tokens(&state, &accepted, allow);
    if let Some((token, message)) = refused {
        next.text = AzString::from(if rest.is_empty() {
            token.clone()
        } else {
            alloc::format!("{token}, {rest}")
        });
        let mut event = TokenInputEvent::create(TokenInputEventKind::Refuse, next);
        event.token = AzString::from(token);
        event.message = message;
        return Some(event);
    }
    next.text = AzString::from(rest);
    if added == 0 {
        return Some(TokenInputEvent::create(TokenInputEventKind::Text, next));
    }
    let last = next.tokens.as_ref().last().cloned().unwrap_or_default();
    let mut event = TokenInputEvent::create(TokenInputEventKind::Add, next);
    event.index = first;
    event.token = last;
    Some(event)
}

/// The `Remove` of token `index` from the built state.
fn remove_event(shared: &mut RefAny, index: usize) -> Option<TokenInputEvent> {
    let state = shared.downcast_ref::<TokenShared>()?.state.clone();
    let token = state.tokens.as_ref().get(index)?.clone();
    let mut event = TokenInputEvent::create(TokenInputEventKind::Remove, remove_token(&state, index));
    event.index = index;
    event.token = token;
    Some(event)
}

/// The entry of the field `field` (its child with [`ENTRY_CLASS`]).
fn entry_in(info: &CallbackInfo, field: DomNodeId) -> Option<DomNodeId> {
    roving::items_of(info, field, ENTRY_CLASS).first().copied()
}

/// Text typed into the entry: a separator in it commits what is before it
/// (a pasted list: all of it); otherwise the app hears the new text.
extern "C" fn on_entry_text(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let keep = |update: Update| OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    };
    let text = state.get_text();
    if !text.contains(TOKEN_INPUT_SEPARATORS) {
        let next = match data.downcast_ref::<TokenShared>() {
            Some(s) => {
                let mut next = s.state.clone();
                next.text = AzString::from(text.as_str());
                next.active = OptionUsize::None;
                next
            }
            None => return keep(Update::DoNothing),
        };
        let update = emit(&mut data, info, TokenInputEvent::create(TokenInputEventKind::Text, next));
        return keep(update);
    }
    let pasted = info
        .get_text_changeset()
        .is_some_and(|c| c.inserted_text.as_str().chars().count() > 1);
    let (mut parts, mut rest) = split_tokens(&text);
    if pasted && !rest.trim().is_empty() {
        parts.push(String::from(rest.trim()));
        rest = String::new();
    }
    let container = info.get_hit_node();
    let Some(event) = commit(&mut data, info, parts, rest) else {
        return keep(Update::DoNothing);
    };
    TextInput::set_text_in(&mut info, container, event.state.text.clone());
    let update = emit(&mut data, info, event);
    // The separator never reaches the line: the line is what is left typed.
    OnTextInputReturn {
        update,
        valid: TextInputValid::No,
    }
}

/// A key in the entry (module docs, KEYBOARD).
extern "C" fn on_entry_key(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let pass = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let ks = info.get_current_keyboard_state();
    let Some(key) = ks.current_virtual_keycode.into_option() else {
        return pass;
    };
    let modified = ks.shift_down() || ks.ctrl_down() || ks.alt_down() || ks.super_down();
    let text = state.get_text();
    let Some((tokens, shown, active)) = data
        .downcast_ref::<TokenShared>()
        .map(|s| (s.state.tokens.len(), s.shown.len(), s.state.active.into_option()))
    else {
        return pass;
    };
    let container = info.get_hit_node();
    let taken = |update: Update| OnTextInputReturn {
        update,
        valid: TextInputValid::No,
    };
    let event = match entry_key(key, text.trim().is_empty(), tokens, shown, active, modified) {
        EntryKey::Pass => return pass,
        EntryKey::CommitText => {
            let (mut parts, rest) = split_tokens(&text);
            parts.push(rest);
            commit(&mut data, info, parts, String::new())
        }
        EntryKey::CommitSuggestion(position) => {
            let pick = data
                .downcast_ref::<TokenShared>()
                .and_then(|s| s.shown.get(position).map(|t| String::from(t.as_str())));
            match pick {
                Some(pick) => commit(&mut data, info, alloc::vec![pick], String::new()),
                None => None,
            }
        }
        EntryKey::RemoveLast => remove_event(&mut data, tokens.saturating_sub(1)),
        EntryKey::Navigate(to) => data.downcast_ref::<TokenShared>().map(|s| {
            let mut next = s.state.clone();
            next.text = AzString::from(text.as_str());
            next.active = to.into();
            TokenInputEvent::create(TokenInputEventKind::Navigate, next)
        }),
        EntryKey::Dismiss => {
            // The list goes until the next build (the next character).
            let list = info
                .get_parent(container)
                .and_then(|field| info.get_parent(field))
                .and_then(|root| roving::items_of(&info, root, LIST_CLASS).first().copied());
            if let Some(list) = list {
                info.set_css_property(list, CssProperty::const_display(LayoutDisplay::None));
            }
            return taken(Update::DoNothing);
        }
        EntryKey::ToChips => {
            let last_x = info.get_parent(container).and_then(|field| {
                roving::items_of(&info, field, CHIP_CLASS)
                    .last()
                    .copied()
                    .and_then(|chip| info.get_last_child(chip))
            });
            if let Some(x) = last_x {
                info.set_focus(FocusTarget::Id(x));
            }
            return taken(Update::DoNothing);
        }
    };
    let Some(event) = event else {
        return taken(Update::DoNothing);
    };
    if event.state.text.as_str() != text.as_str() {
        TextInput::set_text_in(&mut info, container, event.state.text.clone());
    }
    taken(emit(&mut data, info, event))
}

/// A chip's "x": remove the token; the entry keeps the focus.
extern "C" fn on_chip_remove(mut data: RefAny, mut info: CallbackInfo, _state: ChipState) -> Update {
    let Some((index, mut shared)) = token_of(&mut data) else {
        return Update::DoNothing;
    };
    let x = info.get_hit_node();
    let entry = info
        .get_parent(x)
        .and_then(|chip| info.get_parent(chip))
        .and_then(|field| entry_in(&info, field));
    if let Some(entry) = entry {
        info.set_focus(FocusTarget::Id(entry));
    }
    match remove_event(&mut shared, index) {
        Some(event) => emit(&mut shared, info, event),
        None => Update::DoNothing,
    }
}

/// A click on a chip's label: open the token.
extern "C" fn on_chip_click(mut data: RefAny, info: CallbackInfo, _state: ChipState) -> Update {
    let Some((index, mut shared)) = token_of(&mut data) else {
        return Update::DoNothing;
    };
    let state = match shared.downcast_ref::<TokenShared>() {
        Some(s) => s.state.clone(),
        None => return Update::DoNothing,
    };
    let Some(token) = state.tokens.as_ref().get(index).cloned() else {
        return Update::DoNothing;
    };
    let mut event = TokenInputEvent::create(TokenInputEventKind::Open, state);
    event.index = index;
    event.token = token;
    emit(&mut shared, info, event)
}

/// A key on a chip's "x": Left / Right walk the chips (Right past the last
/// is the entry), Delete / Backspace remove the chip.
extern "C" fn on_chip_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use VirtualKeyCode as K;
    let ks = info.get_current_keyboard_state();
    let Some(key) = roving::plain_key(&ks) else {
        return Update::DoNothing;
    };
    if !matches!(key, K::Left | K::Right | K::Back | K::Delete) {
        return Update::DoNothing;
    }
    let Some((index, mut shared)) = token_of(&mut data) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    let x = info.get_hit_node();
    let Some(field) = info.get_parent(x).and_then(|chip| info.get_parent(chip)) else {
        return Update::DoNothing;
    };
    let chips = roving::items_of(&info, field, CHIP_CLASS);
    let Some(position) = info
        .get_parent(x)
        .and_then(|chip| chips.iter().position(|c| *c == chip))
    else {
        return Update::DoNothing;
    };
    let x_of = |info: &CallbackInfo, chip: DomNodeId| info.get_last_child(chip);
    match key {
        K::Left => {
            if position > 0 {
                if let Some(target) = x_of(&info, chips[position - 1]) {
                    info.set_focus(FocusTarget::Id(target));
                }
            }
            Update::DoNothing
        }
        K::Right => {
            let target = if position + 1 < chips.len() {
                x_of(&info, chips[position + 1])
            } else {
                entry_in(&info, field)
            };
            if let Some(target) = target {
                info.set_focus(FocusTarget::Id(target));
            }
            Update::DoNothing
        }
        _ => {
            if let Some(entry) = entry_in(&info, field) {
                info.set_focus(FocusTarget::Id(entry));
            }
            match remove_event(&mut shared, index) {
                Some(event) => emit(&mut shared, info, event),
                None => Update::DoNothing,
            }
        }
    }
}

/// A click on a suggestion: it becomes a token, the entry empties and keeps
/// the focus.
extern "C" fn on_option_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((position, mut shared)) = token_of(&mut data) else {
        return Update::DoNothing;
    };
    let pick = shared
        .downcast_ref::<TokenShared>()
        .and_then(|s| s.shown.get(position).map(|t| String::from(t.as_str())));
    let Some(pick) = pick else {
        return Update::DoNothing;
    };
    let option = info.get_hit_node();
    let entry = info
        .get_parent(option)
        .and_then(|list| info.get_parent(list))
        .and_then(|root| info.get_first_child(root))
        .and_then(|field| entry_in(&info, field));
    if let Some(entry) = entry {
        info.set_focus(FocusTarget::Id(entry));
        TextInput::set_text_in(&mut info, entry, AzString::from_const_str(""));
    }
    match commit(&mut shared, info, alloc::vec![pick], String::new()) {
        Some(event) => emit(&mut shared, info, event),
        None => Update::DoNothing,
    }
}

// ==== fixtures (the widget manifest's sample) ====

/// The token input the widget manifest builds (`widgets::label_convention`):
/// two recipients, "al" typed and three suggestions showing, the second one
/// highlighted.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    fn s(text: &str) -> AzString {
        AzString::from(text)
    }

    /// The sample field.
    pub(crate) fn sample() -> TokenInput {
        TokenInput::create(
            StringVec::from_vec(alloc::vec![s("alice@example.org"), s("bob@example.org")]),
            s("To"),
        )
        .with_state(
            TokenInputState::create(StringVec::from_vec(alloc::vec![
                s("alice@example.org"),
                s("bob@example.org"),
            ]))
            .with_text(s("al"))
            .with_active(1),
        )
        .with_suggestions(StringVec::from_vec(alloc::vec![
            s("Alan Turing <alan@example.org>"),
            s("Malcolm X <malcolm@example.org>"),
            s("Albert Camus <albert@example.org>"),
        ]))
        .with_placeholder(s("Add people"))
    }
}

#[cfg(test)]
mod token_input_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, NodeId},
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::VirtualKeyCode as K,
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks},
    };

    type Log = Arc<Mutex<Vec<String>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, e: TokenInputEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            let tokens: Vec<String> = e.state.tokens.as_ref().iter().map(|t| t.as_str().to_string()).collect();
            log.lock().expect("log").push(format!(
                "{:?} {} {} | {} | {} | {:?}",
                e.kind,
                e.index,
                e.token.as_str(),
                tokens.join(","),
                e.state.text.as_str(),
                e.state.active.into_option(),
            ));
        }
        Update::RefreshDom
    }

    fn log() -> Log {
        Arc::new(Mutex::new(Vec::new()))
    }

    fn logged(log: &Log) -> Vec<String> {
        log.lock().expect("log").clone()
    }

    fn s(text: &str) -> AzString {
        AzString::from(text)
    }

    fn sv(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|t| AzString::from(*t)).collect())
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|t| String::from(*t)).collect()
    }

    /// Two recipients, "al" typed, five candidates (one of them a recipient
    /// already).
    fn field(log: &Log) -> TokenInput {
        TokenInput::create(sv(&["alice@x.org", "bob@y.org"]), s("To"))
            .with_text(s("al"))
            .with_suggestions(sv(&[
                "Alan <alan@z.org>",
                "Malcolm <m@q.org>",
                "alice@x.org",
                "Albert <al@b.org>",
                "Zed <z@z.org>",
            ]))
            .with_placeholder(s("Add people"))
            .with_on_event(RefAny::new(log.clone()), record as TokenInputOnEventCallbackType)
    }

    fn id(n: NodeId) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(n)),
        }
    }

    /// The children of `parent`, in order.
    fn kids(styled: &StyledDom, parent: NodeId) -> Vec<NodeId> {
        let hierarchy = styled.node_hierarchy.as_ref();
        let mut out = Vec::new();
        let mut next = hierarchy[parent.index()].first_child_id(parent);
        while let Some(n) = next {
            out.push(n);
            next = hierarchy[n.index()].next_sibling_id();
        }
        out
    }

    fn has_class_at(styled: &StyledDom, node: NodeId, class: &str) -> bool {
        styled.node_data.as_ref()[node.index()]
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(name) if name.as_str() == class))
    }

    fn texts(node: &Dom, out: &mut Vec<String>) {
        if let azul_core::dom::NodeType::Text(t) = node.root.get_node_type() {
            if !t.as_ref().as_str().is_empty() {
                out.push(t.as_ref().as_str().to_string());
            }
        }
        for c in node.children.as_ref() {
            texts(c, out);
        }
    }

    // ---- the rules ----

    #[test]
    fn typed_text_splits_at_the_separators_and_keeps_what_follows_the_last() {
        assert_eq!(split_tokens("a, b; c"), (strings(&["a", "b"]), String::from("c")));
        assert_eq!(
            split_tokens("alice@x.org, bob@y.org,"),
            (strings(&["alice@x.org", "bob@y.org"]), String::new())
        );
        assert_eq!(split_tokens(" , ,x"), (Vec::new(), String::from("x")), "empty tokens drop");
        assert_eq!(split_tokens("no separator"), (Vec::new(), String::from("no separator")));
        assert_eq!(split_tokens("a\tb\nc"), (strings(&["a", "b"]), String::from("c")));
    }

    #[test]
    fn tokens_are_added_trimmed_once_each_and_the_text_is_cleared() {
        let state = TokenInputState::create(sv(&["Rust"])).with_text(s("go")).with_active(1);
        let (next, added) = add_tokens(&state, &strings(&[" Go ", "rust", "", "go", "Zig"]), false);
        let tokens: Vec<&str> = next.tokens.as_ref().iter().map(|t| t.as_str()).collect();
        assert_eq!(tokens, vec!["Rust", "Go", "Zig"], "trimmed, case-folded duplicates dropped");
        assert_eq!(added, 2);
        assert_eq!(next.text.as_str(), "");
        assert_eq!(next.active.into_option(), None);
        let (next, added) = add_tokens(&state, &strings(&["rust"]), true);
        assert_eq!(next.tokens.len(), 2, "duplicates allowed");
        assert_eq!(added, 1);
        assert!(same_token(" Rust", "rust "));
        assert!(!same_token("Rust", "Rusty"));
    }

    #[test]
    fn a_token_is_removed_by_its_index() {
        let state = TokenInputState::create(sv(&["a", "b", "c"]));
        let next = remove_token(&state, 1);
        let tokens: Vec<&str> = next.tokens.as_ref().iter().map(|t| t.as_str()).collect();
        assert_eq!(tokens, vec!["a", "c"]);
        assert_eq!(remove_token(&state, 9), state, "out of range: unchanged");
    }

    #[test]
    fn the_suggestions_that_start_with_the_text_come_first_and_tokens_never_show() {
        let candidates: Vec<AzString> = [
            "Alan <alan@z.org>",
            "Malcolm <m@q.org>",
            "alice@x.org",
            "Albert <al@b.org>",
            "Zed <z@z.org>",
        ]
        .iter()
        .map(|c| AzString::from(*c))
        .collect();
        let tokens = [AzString::from("ALICE@x.org")];
        assert_eq!(matching_suggestions(&candidates, "al", &tokens, 8), vec![0, 3, 1]);
        assert_eq!(matching_suggestions(&candidates, "AL", &tokens, 2), vec![0, 3], "capped");
        assert!(matching_suggestions(&candidates, "", &tokens, 8).is_empty(), "nothing typed");
        assert!(matching_suggestions(&candidates, "  ", &tokens, 8).is_empty(), "blanks typed");
        assert!(matching_suggestions(&candidates, "qq", &tokens, 8).is_empty());
    }

    #[test]
    fn the_highlight_wraps_around_the_shown_suggestions() {
        assert_eq!(step_active(None, 3, true), Some(0));
        assert_eq!(step_active(None, 3, false), Some(2));
        assert_eq!(step_active(Some(2), 3, true), Some(0));
        assert_eq!(step_active(Some(0), 3, false), Some(2));
        assert_eq!(step_active(Some(1), 3, true), Some(2));
        assert_eq!(step_active(Some(1), 0, true), None, "nothing shown");
    }

    #[test]
    fn the_keys_in_the_entry_commit_remove_navigate_and_dismiss() {
        use EntryKey as E;
        // key, text empty, tokens, shown, active, modified
        assert_eq!(entry_key(K::Return, false, 2, 3, Some(1), false), E::CommitSuggestion(1));
        assert_eq!(entry_key(K::Return, false, 2, 3, None, false), E::CommitText);
        assert_eq!(entry_key(K::NumpadEnter, false, 2, 0, None, false), E::CommitText);
        assert_eq!(entry_key(K::Return, true, 2, 0, None, false), E::Pass, "nothing to commit");
        assert_eq!(entry_key(K::Tab, false, 2, 0, None, false), E::CommitText);
        assert_eq!(entry_key(K::Tab, true, 2, 0, None, false), E::Pass, "Tab leaves an empty entry");
        assert_eq!(entry_key(K::Tab, false, 2, 0, None, true), E::Pass, "Shift+Tab leaves");
        assert_eq!(entry_key(K::Back, true, 2, 0, None, false), E::RemoveLast);
        assert_eq!(entry_key(K::Back, false, 2, 0, None, false), E::Pass, "Backspace edits the text");
        assert_eq!(entry_key(K::Back, true, 0, 0, None, false), E::Pass, "no chip to remove");
        assert_eq!(entry_key(K::Down, false, 2, 3, None, false), E::Navigate(Some(0)));
        assert_eq!(entry_key(K::Up, false, 2, 3, None, false), E::Navigate(Some(2)));
        assert_eq!(entry_key(K::Down, false, 2, 0, None, false), E::Pass, "no list");
        assert_eq!(entry_key(K::Escape, false, 2, 3, Some(0), false), E::Dismiss);
        assert_eq!(entry_key(K::Escape, false, 2, 0, None, false), E::Pass);
        assert_eq!(entry_key(K::Left, true, 2, 0, None, false), E::ToChips);
        assert_eq!(entry_key(K::Left, false, 2, 0, None, false), E::Pass, "Left moves the caret");
        assert_eq!(entry_key(K::A, false, 2, 3, None, false), E::Pass);
    }

    // ---- the DOM ----

    #[test]
    fn the_field_holds_a_chip_per_token_and_the_entry_and_the_list_shows_the_matches() {
        let log = log();
        for theme in checks::BOTH {
            let dom = field(&log).with_theme(theme).dom();
            assert!(theme_checks::has_class(&dom, TOKEN_INPUT_CLASS), "{}", theme.name());
            let info = dom.root.get_accessibility_info().cloned().unwrap_or_default();
            assert_eq!(info.role, AccessibilityRole::Grouping);
            assert_eq!(
                info.accessibility_name.as_ref().map(|n| n.as_str().to_string()),
                Some(String::from("To"))
            );
            let parts = dom.children.as_ref();
            assert_eq!(parts.len(), 2, "{}: the field and the list", theme.name());
            assert!(theme_checks::has_class(&parts[0], FIELD_CLASS));
            let in_field = parts[0].children.as_ref();
            assert_eq!(in_field.len(), 3, "two chips and the entry");
            for (n, token) in ["alice@x.org", "bob@y.org"].iter().enumerate() {
                assert!(theme_checks::has_class(&in_field[n], CHIP_CLASS));
                let mut words = Vec::new();
                texts(&in_field[n], &mut words);
                assert!(words.iter().any(|w| w == token), "{words:?}");
            }
            assert!(theme_checks::has_class(&in_field[2], ENTRY_CLASS));
            let list = &parts[1];
            assert!(theme_checks::has_class(list, LIST_CLASS));
            assert_eq!(
                list.root.get_accessibility_info().map(|i| i.role),
                Some(AccessibilityRole::List)
            );
            let options: Vec<Vec<String>> = list
                .children
                .as_ref()
                .iter()
                .map(|o| {
                    let mut w = Vec::new();
                    texts(o, &mut w);
                    w
                })
                .collect();
            assert_eq!(
                options,
                vec![
                    vec![String::from("Alan <alan@z.org>")],
                    vec![String::from("Albert <al@b.org>")],
                    vec![String::from("Malcolm <m@q.org>")],
                ]
            );
        }
    }

    #[test]
    fn nothing_typed_shows_no_list() {
        let log = log();
        let dom = field(&log).with_text(s("")).with_theme(UiTheme::Flat).dom();
        assert_eq!(dom.children.as_ref().len(), 1, "the field alone");
        assert!(theme_checks::find(&dom, LIST_CLASS).is_none());
    }

    #[test]
    fn the_highlighted_suggestion_is_marked_and_announced_selected() {
        let log = log();
        let mut input = field(&log).with_theme(UiTheme::Flat);
        input.state.active = OptionUsize::Some(1);
        let dom = input.dom();
        let list = &dom.children.as_ref()[1];
        let options = list.children.as_ref();
        assert!(theme_checks::has_class(&options[1], OPTION_ACTIVE_CLASS));
        assert!(!theme_checks::has_class(&options[0], OPTION_ACTIVE_CLASS));
        let states = options[1]
            .root
            .get_accessibility_info()
            .map(|i| i.states.as_ref().to_vec())
            .unwrap_or_default();
        assert!(states.contains(&AccessibilityState::Selected));
        assert_ne!(options[0].root.get_style(), options[1].root.get_style(), "the highlight shows");
    }

    #[test]
    fn the_entry_is_the_one_tab_stop_and_the_chips_are_reached_by_the_arrows() {
        let log = log();
        let styled = StyledDom::create_from_dom(field(&log).with_theme(UiTheme::Flat).dom());
        let field_node = kids(&styled, NodeId::new(0))[0];
        let parts = kids(&styled, field_node);
        let entry = parts[2];
        assert!(has_class_at(&styled, entry, ENTRY_CLASS));
        assert_eq!(styled.node_data.as_ref()[entry.index()].get_tab_index(), Some(TabIndex::Auto));
        let x0 = kids(&styled, parts[0])[1];
        let x1 = kids(&styled, parts[1])[1];
        for x in [x0, x1] {
            assert_eq!(
                styled.node_data.as_ref()[x.index()].get_tab_index(),
                Some(TabIndex::NoKeyboardFocus),
                "a chip's x is not a Tab stop of its own"
            );
        }
        let focus = |from: NodeId, key: K| {
            let (_, changes) = rv::press(&styled, id(from), key, &[]).expect("a key handler");
            assert!(rv::prevented(&changes), "{key:?}");
            rv::focus_request(&changes)
        };
        assert_eq!(focus(x0, K::Right), Some(id(x1)));
        assert_eq!(focus(x1, K::Left), Some(id(x0)));
        assert_eq!(focus(x1, K::Right), Some(id(entry)), "past the last chip: the entry");
        assert_eq!(focus(x0, K::Left), None, "the first chip holds");
    }

    #[test]
    fn a_chips_x_or_backspace_on_it_removes_it() {
        let log = log();
        let styled = StyledDom::create_from_dom(field(&log).with_theme(UiTheme::Flat).dom());
        let field_node = kids(&styled, NodeId::new(0))[0];
        let parts = kids(&styled, field_node);
        let x0 = kids(&styled, parts[0])[1];
        let x1 = kids(&styled, parts[1])[1];
        rv::fire(&styled, id(x0), EventFilter::Hover(HoverEventFilter::Click)).expect("a click target");
        rv::press(&styled, id(x1), K::Back, &[]).expect("a key handler");
        assert_eq!(
            logged(&log),
            vec![
                "Remove 0 alice@x.org | bob@y.org | al | None",
                "Remove 1 bob@y.org | alice@x.org | al | None",
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_click_on_a_suggestion_adds_it_and_clears_the_text() {
        let log = log();
        let styled = StyledDom::create_from_dom(field(&log).with_theme(UiTheme::Flat).dom());
        let list = kids(&styled, NodeId::new(0))[1];
        let options = kids(&styled, list);
        assert_eq!(options.len(), 3);
        rv::fire(&styled, id(options[1]), EventFilter::Hover(HoverEventFilter::Click)).expect("a click target");
        assert_eq!(
            logged(&log),
            vec![String::from(
                "Add 2 Albert <al@b.org> | alice@x.org,bob@y.org,Albert <al@b.org> |  | None"
            )]
        );
    }

    #[test]
    fn a_token_input_without_a_theme_follows_the_app_theme_and_declares_its_structure_once() {
        let log = log();
        for text in ["al", ""] {
            checks::assert_follows_the_app_theme(
                "token_input",
                || field(&log).with_text(s(text)).dom(),
                |t: UiTheme| field(&log).with_text(s(text)).with_theme(t).dom(),
            );
            for theme in checks::BOTH {
                let dom = checks::under(theme, || field(&log).with_text(s(text)).dom());
                theme_checks::assert_structure_is_shared(&format!("token_input built for {}", theme.name()), &dom, &[]);
                theme_checks::assert_theme_invariants(&format!("token_input ({})", theme.name()), &dom);
            }
        }
    }
}
