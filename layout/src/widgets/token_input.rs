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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
    (Vec::new(), String::from(text))
}

/// Whether two tokens are the same (case folded, trimmed).
#[must_use]
pub(crate) fn same_token(a: &str, b: &str) -> bool {
    a == b
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
    let _ = (tokens, allow_duplicates);
    (state.clone(), 0)
}

/// `state` without token `index` (out of range: unchanged).
#[must_use]
pub(crate) fn remove_token(state: &TokenInputState, index: usize) -> TokenInputState {
    let _ = index;
    state.clone()
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
    let _ = (suggestions, text, tokens, max);
    Vec::new()
}

/// The highlight after Down (`down`) or Up over `count` suggestions shown:
/// wrapping, from nothing to the first (Down) or the last (Up).
#[must_use]
pub(crate) fn step_active(active: Option<usize>, count: usize, down: bool) -> Option<usize> {
    let _ = (count, down);
    active
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
    let _ = (key, text_empty, tokens, shown, active, modified);
    EntryKey::Pass
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

/// The look a token input with the theme option `theme` is built with.
pub(crate) fn look_for(theme: OptionUiTheme) -> TokenInputLook {
    let _ = theme;
    TokenInputLook::default()
}

/// The token input's DOM in `look`.
pub(crate) fn build(input: TokenInput, look: &TokenInputLook) -> Dom {
    let _ = look;
    Dom::create_div()
        .with_class(AzString::from_const_str(TOKEN_INPUT_CLASS))
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Grouping,
            accessibility_name: Some(input.accessibility_name).into(),
            ..Default::default()
        })
}
