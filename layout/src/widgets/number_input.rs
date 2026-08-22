//! Numeric input widget that wraps `TextInput` with numeric validation.
//!
//! Exports `NumberInput`, `NumberInputState`, and callback types
//! (`NumberInputOnValueChangeCallbackType`, `NumberInputOnFocusLostCallbackType`).
//! Internally delegates to `TextInput` and validates that the entered text
//! parses as an `f32` within the configured `min`/`max` range.

use std::string::String;

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::Dom,
    refany::RefAny,
};
#[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
use azul_css::{OptionString, 
    dynamic_selector::CssPropertyWithConditionsVec,
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::text_input::{
        OnTextInputReturn, TextInput, TextInputOnFocusLostCallback,
        TextInputOnFocusLostCallbackType, TextInputOnTextInputCallback,
        TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallback,
        TextInputOnVirtualKeyDownCallbackType, TextInputState, TextInputValid,
    },
};

/// Callback type invoked when the numeric value changes.
pub type NumberInputOnValueChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, NumberInputState) -> Update;
impl_widget_callback!(
    NumberInputOnValueChange,
    OptionNumberInputOnValueChange,
    NumberInputOnValueChangeCallback,
    NumberInputOnValueChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        NumberInputOnValueChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: NUMBER_INPUT_ON_VALUE_CHANGE_INVOKER,
    invoker_ty:     AzNumberInputOnValueChangeCallbackInvoker,
    thunk_fn:       az_number_input_on_value_change_callback_thunk,
    setter_fn:      AzApp_setNumberInputOnValueChangeCallbackInvoker,
    from_handle_fn: AzNumberInputOnValueChangeCallback_createFromHostHandle,
    extra_args:     [ state: NumberInputState ],
}

/// Callback type invoked when the number input loses focus.
pub type NumberInputOnFocusLostCallbackType =
    extern "C" fn(RefAny, CallbackInfo, NumberInputState) -> Update;
impl_widget_callback!(
    NumberInputOnFocusLost,
    OptionNumberInputOnFocusLost,
    NumberInputOnFocusLostCallback,
    NumberInputOnFocusLostCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        NumberInputOnFocusLostCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: NUMBER_INPUT_ON_FOCUS_LOST_INVOKER,
    invoker_ty:     AzNumberInputOnFocusLostCallbackInvoker,
    thunk_fn:       az_number_input_on_focus_lost_callback_thunk,
    setter_fn:      AzApp_setNumberInputOnFocusLostCallbackInvoker,
    from_handle_fn: AzNumberInputOnFocusLostCallback_createFromHostHandle,
    extra_args:     [ state: NumberInputState ],
}

/// A numeric input widget that wraps `TextInput` with `f32` validation.
#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct NumberInput {
    pub number_input_state: NumberInputStateWrapper,
    pub text_input: TextInput,
    pub style: CssPropertyWithConditionsVec,
    /// What this control is CALLED, for assistive technology.
    ///
    /// Carried by the WIDGET so it knows at build time whether it was named;
    /// forwarded into the accessibility declaration it already builds.
    pub accessibility_name: OptionString,
}

/// Wraps `NumberInputState` together with its value-change and focus-lost callbacks.
#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct NumberInputStateWrapper {
    pub inner: NumberInputState,
    pub on_value_change: OptionNumberInputOnValueChange,
    pub on_focus_lost: OptionNumberInputOnFocusLost,
}

/// State of a `NumberInput`: the current and previous value, plus allowed range.
#[derive(Copy, Debug, Clone, PartialEq)]
#[repr(C)]
pub struct NumberInputState {
    /// The value before the most recent change.
    pub previous: f32,
    /// The current numeric value.
    pub number: f32,
    /// Minimum allowed value (inclusive).
    pub min: f32,
    /// Maximum allowed value (inclusive).
    pub max: f32,
}

impl Default for NumberInputState {
    fn default() -> Self {
        Self {
            previous: 0.0,
            number: 0.0,
            min: core::f32::MIN,
            max: core::f32::MAX,
        }
    }
}

impl NumberInput {
    /// Creates a new `NumberInput` with the given initial value.
    /// Name this control for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    #[must_use] pub fn create(input: f32) -> Self {
        Self {
            number_input_state: NumberInputStateWrapper {
                inner: NumberInputState {
                    number: input,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    pub fn set_on_text_input<C: Into<TextInputOnTextInputCallback>>(
        &mut self,
        refany: RefAny,
        callback: C,
    ) {
        self.text_input.set_on_text_input(refany, callback);
    }

    #[must_use]
    pub fn with_on_text_input<C: Into<TextInputOnTextInputCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_text_input(refany, callback);
        self
    }

    pub fn set_on_virtual_key_down<C: Into<TextInputOnVirtualKeyDownCallback>>(
        &mut self,
        refany: RefAny,
        callback: C,
    ) {
        self.text_input.set_on_virtual_key_down(refany, callback);
    }

    #[must_use]
    pub fn with_on_virtual_key_down<C: Into<TextInputOnVirtualKeyDownCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_virtual_key_down(refany, callback);
        self
    }

    pub fn set_placeholder_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.text_input.placeholder_style = style;
    }

    #[must_use] pub fn with_placeholder_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_placeholder_style(style);
        self
    }

    pub fn set_container_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.text_input.container_style = style;
    }

    #[must_use] pub fn with_container_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_container_style(style);
        self
    }

    pub fn set_label_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.text_input.label_style = style;
    }

    #[must_use] pub fn with_label_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_label_style(style);
        self
    }

    // Function called when the input has been parsed as a number
    pub fn set_on_value_change<C: Into<NumberInputOnValueChangeCallback>>(
        &mut self,
        refany: RefAny,
        callback: C,
    ) {
        self.number_input_state.on_value_change = Some(NumberInputOnValueChange {
            callback: callback.into(),
            refany,
        })
        .into();
    }

    #[must_use]
    pub fn with_on_value_change<C: Into<NumberInputOnValueChangeCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_value_change(refany, callback);
        self
    }

    pub fn set_on_focus_lost<C: Into<NumberInputOnFocusLostCallback>>(
        &mut self,
        refany: RefAny,
        callback: C,
    ) {
        self.number_input_state.on_focus_lost = Some(NumberInputOnFocusLost {
            callback: callback.into(),
            refany,
        })
        .into();
    }

    #[must_use]
    pub fn with_on_focus_lost<C: Into<NumberInputOnFocusLostCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_focus_lost(refany, callback);
        self
    }

    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(0.0);
        core::mem::swap(&mut s, self);
        s
    }

    #[must_use] pub fn dom(mut self) -> Dom {
        let number_string = format!("{}", self.number_input_state.inner.number);
        self.text_input.text_input_state.inner.text = number_string
            .chars()
            .map(|s| s as u32)
            .collect::<Vec<_>>()
            .into();

        let state = RefAny::new(self.number_input_state);

        let validate: TextInputOnTextInputCallbackType = validate_text_input;
        self.text_input.set_on_text_input(state.clone(), validate);
        let focus_lost: TextInputOnFocusLostCallbackType = on_focus_lost;
        self.text_input.set_on_focus_lost(state, focus_lost);
        self.text_input.dom()
    }
}

extern "C" fn on_focus_lost(
    mut refany: RefAny,
    info: CallbackInfo,
    _state: TextInputState,
) -> Update {
    let Some(mut refany) = refany.downcast_mut::<NumberInputStateWrapper>() else {
        return Update::DoNothing;
    };

    let number_input = &mut *refany;
    let onfocuslost = &mut number_input.on_focus_lost;
    let inner = number_input.inner;

    match onfocuslost.as_mut() {
        Some(NumberInputOnFocusLost { callback, refany }) => {
            (callback.cb)(refany.clone(), info, inner)
        }
        None => Update::DoNothing,
    }
}

/// Is `s` a PREFIX of some string `<f32 as FromStr>` accepts — i.e.
///
/// can it
/// still become a number with more characters? Grammar:
/// `[+-]? digits* ('.' digits*)? ([eE] [+-]? digits*)?`, with the exponent
/// allowed only after at least one mantissa digit (`e`, `.e` can never
/// become numbers, `1e` can). The empty buffer is a prefix of everything.
/// Deliberately NOT `str::parse` (which rejects every prefix) and
/// deliberately not accepting `inf` / `nan`, which the widget never wants.
fn is_float_prefix(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let mut mantissa_digits = 0usize;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
        mantissa_digits += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
            mantissa_digits += 1;
        }
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        if mantissa_digits == 0 {
            return false;
        }
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    i == b.len()
}

/// Clamps `value` into `[min, max]`, tolerating the degenerate bounds that
/// `f32::clamp` panics on: an inverted range (`min > max`) is swapped and a NaN
/// bound is dropped (both NaN → value untouched). `min`/`max` are `pub` fields on
/// a `#[repr(C)]` `NumberInputState` reachable across the C/FFI boundary, so a
/// caller can invert or NaN them; a panic here would unwind across that boundary.
fn clamp_to_range(value: f32, min: f32, max: f32) -> f32 {
    let (lo, hi) = match (min.is_nan(), max.is_nan()) {
        (true, true) => return value,
        (true, false) => (max, max),
        (false, true) => (min, min),
        (false, false) if min <= max => (min, max),
        (false, false) => (max, min),
    };
    value.clamp(lo, hi)
}

extern "C" fn validate_text_input(
    mut refany: RefAny,
    info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let Some(mut refany) = refany.downcast_mut::<NumberInputStateWrapper>() else {
        return OnTextInputReturn {
            update: Update::DoNothing,
            valid: TextInputValid::Yes,
        };
    };

    let validated_input: String = state
        .text
        .iter()
        .filter_map(|c| core::char::from_u32(*c))
        .map(|c| if c == ',' { '.' } else { c })
        .collect();

    let Ok(validated_f32) = validated_input.parse::<f32>() else {
        // A buffer that is not a number YET but can become one — `-`, `.`,
        // `-.`, `1e-`, the empty buffer — must be allowed to exist in the
        // field: a number is typed one character at a time, and `-5` or `.5`
        // starts with a character that does not parse. Vetoing those
        // keystrokes made it impossible to enter a negative or fractional
        // number from scratch. The value stays what it was; the hook waits
        // for a complete number. A buffer that can never become a number
        // (`abc`, `1.2.3`, ` 1`) is still vetoed.
        if is_float_prefix(&validated_input) {
            return OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            };
        }
        // do not re-layout the entire screen,
        // but don't handle the character
        return OnTextInputReturn {
            update: Update::DoNothing,
            valid: TextInputValid::No,
        };
    };

    let number_input = &mut *refany;
    let onvaluechange = &mut number_input.on_value_change;
    let inner = &mut number_input.inner;

    inner.previous = inner.number;
    let clamped = clamp_to_range(validated_f32, inner.min, inner.max);
    inner.number = clamped;
    let inner_clone = *inner;

    let update = match onvaluechange.as_mut() {
        Some(NumberInputOnValueChange { callback, refany }) => {
            (callback.cb)(refany.clone(), info, inner_clone)
        }
        None => Update::DoNothing,
    };

    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

#[cfg(all(test, feature = "std"))]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use std::{
        collections::BTreeMap,
        panic::{catch_unwind, AssertUnwindSafe},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId},
        geom::OptionLogicalPosition,
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::NodeHierarchyItemId,
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::dynamic_selector::CssPropertyWithConditions;
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        widgets::text_input::TextInputStateWrapper,
        window::LayoutWindow,
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Sample values
    // ------------------------------------------------------------------

    /// Every finite `f32` the widget has to survive a format/parse round-trip on:
    /// both zeros (the sign of `-0.0` is the classic casualty), both ends of the
    /// range, the smallest normal, the smallest and largest subnormals, and `2^24`
    /// — the point where `f32` stops being able to count.
    fn finite_samples() -> [f32; 16] {
        [
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.5,
            -0.5,
            42.25,
            -2.5,
            0.1,
            16_777_216.0,
            f32::MIN,
            f32::MAX,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            f32::EPSILON,
            // smallest positive subnormal (~1.4e-45); written as bits so no float
            // literal in this file is ever out of range for `f32`.
            f32::from_bits(1),
        ]
    }

    /// Strings `<f32 as FromStr>` rejects outright AND that can never become a
    /// number however the user continues typing. None of them contains a
    /// comma, so the widget's `,` -> `.` rewrite cannot rescue any of them
    /// either. (Buffers that are merely incomplete live in `TRANSIENT_PREFIXES`.)
    const MALFORMED: [&str; 20] = [
        " ",            // `from_str` does not trim
        " 1",
        "1 ",
        "\t1",
        "\n",
        "abc",
        "e",
        "e5",
        "E",
        "..",
        "--1",
        "1.2.3",
        "0x10",         // hex is not float syntax
        "0b1",
        "1_000",        // Rust *literal* syntax is not *parse* syntax
        "1/2",
        "1%",
        "½",            // vulgar fraction
        "∞",            // the symbol is not the word "inf"
        "1\u{200b}0",   // zero-width space wedged between two digits
    ];

    /// Buffers `from_str` rejects that are PREFIXES of a number: the states a
    /// field passes through while `-5`, `.5`, `1e-3` or a replacement are
    /// typed. The widget must let them exist (keystroke accepted, value
    /// unchanged, hook silent) or a negative / fractional number can never be
    /// entered from scratch.
    const TRANSIENT_PREFIXES: [&str; 12] = [
        "",             // the empty buffer: select-all + delete, then type
        "+",
        "-",
        ".",
        "-.",
        "+.",
        "1.",
        "-1.",
        "1e",
        "1e+",
        "1e-",
        "1.5e-",
    ];

    /// Digits that are digits to a human but not to `from_str`.
    const NON_ASCII_DIGITS: [&str; 6] = [
        "١٢٣",        // Arabic-Indic
        "١٫٥",        // Arabic-Indic + the Arabic decimal separator
        "１２３",      // fullwidth
        "𝟏",          // MATHEMATICAL BOLD DIGIT ONE
        "٣.5",        // mixed script
        "Ⅻ",          // roman numeral twelve
    ];

    /// Every spelling the Rust float parser accepts, paired with the value the widget
    /// must end up storing under the default range. `inf` / `-inf` are listed with
    /// their *clamped* results — saturating them is the widget's job, not the parser's.
    const ACCEPTED: [(&str, f32); 17] = [
        ("0", 0.0),
        ("-0", -0.0),
        ("+1", 1.0),
        ("1.", 1.0),
        (".5", 0.5),
        ("-.5", -0.5),
        ("1e3", 1000.0),
        ("1E3", 1000.0),
        ("1e+3", 1000.0),
        ("1e-3", 0.001),
        ("00042.2500", 42.25),
        ("inf", f32::MAX),
        ("infinity", f32::MAX),
        ("-inf", f32::MIN),
        ("nan", f32::NAN),
        ("NaN", f32::NAN),
        ("NAN", f32::NAN),
    ];

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    /// Bit-exact float comparison — `-0.0 != 0.0` here, because losing the sign of a
    /// zero is exactly the kind of round-trip damage these tests are looking for.
    /// NaNs compare equal to each other: the widget renders every NaN as `"NaN"`, so
    /// the payload and sign cannot survive anyway.
    fn same(a: f32, b: f32) -> bool {
        if a.is_nan() || b.is_nan() {
            a.is_nan() && b.is_nan()
        } else {
            a.to_bits() == b.to_bits()
        }
    }

    /// A `NumberInputStateWrapper` with no hooks: `previous` starts at `0.0` so any
    /// write to it is visible.
    fn wrapper(number: f32, min: f32, max: f32) -> NumberInputStateWrapper {
        NumberInputStateWrapper {
            inner: NumberInputState {
                previous: 0.0,
                number,
                min,
                max,
            },
            on_value_change: OptionNumberInputOnValueChange::None,
            on_focus_lost: OptionNumberInputOnFocusLost::None,
        }
    }

    /// The widget's edit buffer, built from a `&str` the way `TextInput` builds it.
    fn text_state(text: &str) -> TextInputState {
        TextInputState {
            text: text.chars().map(|c| c as u32).collect::<Vec<_>>().into(),
            ..TextInputState::default()
        }
    }

    /// An edit buffer built from *raw* `u32` code units — the buffer is a `U32Vec`,
    /// so it can hold values that are not Unicode scalars at all.
    fn raw_text_state(units: &[u32]) -> TextInputState {
        TextInputState {
            text: units.to_vec().into(),
            ..TextInputState::default()
        }
    }

    /// The state currently stored behind a `NumberInputStateWrapper` payload.
    fn read(state: &RefAny) -> NumberInputState {
        let mut state = state.clone();
        let wrapper = state
            .downcast_ref::<NumberInputStateWrapper>()
            .expect("the payload must still be a NumberInputStateWrapper");
        wrapper.inner
    }

    /// Overwrites the stored value and clears the history, so one `LayoutWindow` can
    /// serve a whole table of cases.
    fn reset(state: &RefAny, number: f32) {
        let mut state = state.clone();
        let mut wrapper = state
            .downcast_mut::<NumberInputStateWrapper>()
            .expect("the payload must still be a NumberInputStateWrapper");
        wrapper.inner.number = number;
        wrapper.inner.previous = 0.0;
    }

    /// `n` properties lifted off the default container style — an easy way to mint
    /// style vectors that are pairwise distinct without hard-coding CSS.
    fn style(n: usize) -> CssPropertyWithConditionsVec {
        let all: Vec<CssPropertyWithConditions> =
            TextInput::default().container_style.as_ref().to_vec();
        assert!(n <= all.len(), "not enough default properties to slice");
        CssPropertyWithConditionsVec::from_vec(all.into_iter().take(n).collect())
    }

    // ---- recording hooks --------------------------------------------------

    /// Records every `NumberInputState` a hook is handed, and answers with `ret`.
    struct Recorder {
        seen: Vec<NumberInputState>,
        ret: Update,
    }

    impl Recorder {
        fn new(ret: Update) -> Self {
            Self {
                seen: Vec::new(),
                ret,
            }
        }
    }

    extern "C" fn record_value_change(
        mut data: RefAny,
        _: CallbackInfo,
        state: NumberInputState,
    ) -> Update {
        let Some(mut log) = data.downcast_mut::<Recorder>() else {
            return Update::DoNothing;
        };
        log.seen.push(state);
        log.ret
    }

    // Deliberately *not* the same body as `record_value_change`: two hooks with
    // identical bodies can be folded onto one symbol, and these two have to stay
    // distinguishable.
    extern "C" fn record_focus_lost(
        mut data: RefAny,
        _: CallbackInfo,
        state: NumberInputState,
    ) -> Update {
        match data.downcast_mut::<Recorder>() {
            Some(mut log) => {
                log.seen.push(state);
                log.ret
            }
            None => Update::DoNothing,
        }
    }

    /// A user-supplied text-input hook that accepts *everything*: if `dom()` kept it,
    /// `"abc"` would come back as `TextInputValid::Yes`.
    extern "C" fn accept_everything(
        _: RefAny,
        _: CallbackInfo,
        _: TextInputState,
    ) -> OnTextInputReturn {
        OnTextInputReturn {
            update: Update::RefreshDomAllWindows,
            valid: TextInputValid::Yes,
        }
    }

    /// A user-supplied virtual-key hook with a signature no other hook here returns.
    extern "C" fn reject_everything(
        _: RefAny,
        _: CallbackInfo,
        _: TextInputState,
    ) -> OnTextInputReturn {
        OnTextInputReturn {
            update: Update::RefreshDomAllWindows,
            valid: TextInputValid::No,
        }
    }

    fn recorded(recorder: &RefAny) -> Vec<NumberInputState> {
        let mut recorder = recorder.clone();
        let log = recorder
            .downcast_ref::<Recorder>()
            .expect("the payload must still be a Recorder");
        log.seen.clone()
    }

    fn wrapper_with_value_hook(
        number: f32,
        min: f32,
        max: f32,
        recorder: &RefAny,
    ) -> NumberInputStateWrapper {
        NumberInputStateWrapper {
            on_value_change: Some(NumberInputOnValueChange {
                refany: recorder.clone(),
                callback: (record_value_change as NumberInputOnValueChangeCallbackType).into(),
            })
            .into(),
            ..wrapper(number, min, max)
        }
    }

    fn wrapper_with_focus_hook(
        number: f32,
        min: f32,
        max: f32,
        recorder: &RefAny,
    ) -> NumberInputStateWrapper {
        NumberInputStateWrapper {
            on_focus_lost: Some(NumberInputOnFocusLost {
                refany: recorder.clone(),
                callback: (record_focus_lost as NumberInputOnFocusLostCallbackType).into(),
            })
            .into(),
            ..wrapper(number, min, max)
        }
    }

    // ---- CallbackInfo harness --------------------------------------------

    /// Runs `f` with a real `CallbackInfo` over an empty `LayoutWindow`. Neither
    /// `validate_text_input` nor `on_focus_lost` queries the DOM through it — they
    /// only hand it to the user's hook — so an empty window is enough. `CallbackInfo`
    /// is `Copy`, so a whole table of cases can share one window (building one per
    /// case would dominate the runtime).
    fn with_info<R>(f: impl FnOnce(CallbackInfo) -> R) -> R {
        let layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(system::SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::NONE,
            },
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        f(info)
    }

    /// One edit delivered to `validate_text_input`; returns its answer plus the state
    /// it left behind.
    fn validate_one(state: &RefAny, text: &str) -> (OnTextInputReturn, NumberInputState) {
        with_info(|info| {
            let r = validate_text_input(state.clone(), info, text_state(text));
            (r, read(state))
        })
    }

    /// One edit made of raw code units (which need not be Unicode scalars).
    fn validate_raw(state: &RefAny, units: &[u32]) -> (OnTextInputReturn, NumberInputState) {
        with_info(|info| {
            let r = validate_text_input(state.clone(), info, raw_text_state(units));
            (r, read(state))
        })
    }

    fn focus_lost(state: &RefAny, text: &str) -> Update {
        with_info(|info| on_focus_lost(state.clone(), info, text_state(text)))
    }

    // ---- DOM probes -------------------------------------------------------

    /// Flattened child indices of `TextInput::dom()`.
    const PLACEHOLDER: usize = 0;
    const LABEL: usize = 1;

    fn dataset_of(dom: &Dom) -> RefAny {
        dom.root
            .get_dataset()
            .cloned()
            .expect("TextInput::dom must attach its state as the node's dataset")
    }

    /// The text sitting in the widget's *edit buffer*.
    fn buffer_text(dom: &Dom) -> String {
        let mut dataset = dataset_of(dom);
        let wrapper = dataset
            .downcast_ref::<TextInputStateWrapper>()
            .expect("the dataset must be a TextInputStateWrapper");
        wrapper.inner.get_text()
    }

    /// The text actually *rendered* into the label node (a styled `<p>`
    /// wrapping its bare text leaf per the label convention).
    fn displayed_text(dom: &Dom) -> String {
        let label = &dom.children.as_ref()[LABEL];
        label
            .root
            .get_node_type()
            .format()
            .or_else(|| {
                label.children.as_ref().first().and_then(|c| c.root.get_node_type().format())
            })
            .expect("the label child must wrap a text node")
    }

    fn cursor_pos(dom: &Dom) -> usize {
        let mut dataset = dataset_of(dom);
        let wrapper = dataset
            .downcast_ref::<TextInputStateWrapper>()
            .expect("the dataset must be a TextInputStateWrapper");
        wrapper.inner.cursor_pos
    }

    /// The `NumberInputStateWrapper` the rendered widget actually validates against —
    /// pulled out of the hook `dom()` installed, so nothing about the wiring is
    /// re-created by hand.
    fn number_state_of(dom: &Dom) -> RefAny {
        let mut dataset = dataset_of(dom);
        let wrapper = dataset
            .downcast_ref::<TextInputStateWrapper>()
            .expect("the dataset must be a TextInputStateWrapper");
        wrapper
            .on_text_input
            .as_ref()
            .expect("NumberInput::dom must install a text-input hook")
            .refany
            .clone()
    }

    /// Delivers `text` to whichever text-input hook the rendered widget registered.
    fn drive_text_input(dom: &Dom, text: &str) -> OnTextInputReturn {
        let mut dataset = dataset_of(dom);
        let hook = dataset
            .downcast_ref::<TextInputStateWrapper>()
            .expect("the dataset must be a TextInputStateWrapper")
            .on_text_input
            .as_ref()
            .expect("NumberInput::dom must install a text-input hook")
            .clone();
        with_info(|info| (hook.callback.cb)(hook.refany.clone(), info, text_state(text)))
    }

    /// Delivers a key-down to whichever virtual-key hook survived rendering, if any.
    fn drive_virtual_key_down(dom: &Dom) -> Option<OnTextInputReturn> {
        let mut dataset = dataset_of(dom);
        let hook = dataset
            .downcast_ref::<TextInputStateWrapper>()
            .expect("the dataset must be a TextInputStateWrapper")
            .on_virtual_key_down
            .as_ref()
            .cloned()?;
        Some(with_info(|info| {
            (hook.callback.cb)(hook.refany.clone(), info, text_state(""))
        }))
    }

    // ==================================================================
    // NumberInput::create — numeric limits
    // ==================================================================

    #[test]
    fn create_zero_is_exactly_the_default_widget() {
        assert_eq!(
            NumberInput::create(0.0),
            NumberInput::default(),
            "create(0.0) must not perturb anything Default already set",
        );
    }

    #[test]
    fn create_preserves_every_sample_value_bit_exactly() {
        for v in finite_samples() {
            let state = NumberInput::create(v).number_input_state.inner;
            assert!(
                same(state.number, v),
                "create({v:?}) stored {:?}",
                state.number,
            );
            assert!(
                same(state.previous, 0.0),
                "create({v:?}) must start with no history, got previous = {:?}",
                state.previous,
            );
            assert!(
                same(state.min, f32::MIN) && same(state.max, f32::MAX),
                "create({v:?}) must leave the range wide open, got [{}, {}]",
                state.min,
                state.max,
            );
        }
    }

    #[test]
    fn create_accepts_nan_and_infinities_without_panicking() {
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let state = NumberInput::create(v).number_input_state.inner;
            assert!(
                same(state.number, v),
                "create({v:?}) stored {:?}",
                state.number,
            );
        }
        assert!(NumberInput::create(f32::NAN)
            .number_input_state
            .inner
            .number
            .is_nan());
    }

    #[test]
    fn create_never_clamps_its_argument() {
        // `create` is documented as "the given initial value" — it does not consult
        // min/max, so `+inf` survives even though the default `max` is `f32::MAX`.
        // Clamping is the *input* path's job (see `validate_clamps_into_range`).
        let state = NumberInput::create(f32::INFINITY).number_input_state.inner;
        assert!(
            state.number.is_infinite(),
            "create must store the value verbatim, got {}",
            state.number,
        );
        assert!(state.number > state.max, "…even outside its own range");
    }

    #[test]
    fn the_default_range_leaves_every_finite_value_untouched() {
        let d = NumberInputState::default();
        assert!(
            d.min <= d.max,
            "the default range must be non-empty — f32::clamp panics otherwise",
        );
        assert!(same(d.min, f32::MIN) && same(d.max, f32::MAX));
        assert!(same(d.number, 0.0) && same(d.previous, 0.0));
        for v in finite_samples() {
            assert!(
                same(v.clamp(d.min, d.max), v),
                "{v:?} must pass through the default range untouched",
            );
        }
    }

    #[test]
    fn number_input_state_is_a_value_type() {
        let a = NumberInputState::default();
        let mut b = a;
        b.number = 5.0;
        assert!(
            same(a.number, 0.0),
            "NumberInputState is Copy — mutating a copy must not alias the original",
        );
        assert_ne!(a, b);
    }

    #[test]
    fn a_fresh_wrapper_has_no_hooks() {
        let w = NumberInputStateWrapper::default();
        assert!(w.on_value_change.as_ref().is_none());
        assert!(w.on_focus_lost.as_ref().is_none());
        assert_eq!(w.inner, NumberInputState::default());
    }

    // ==================================================================
    // Builders / setters — invariants
    // ==================================================================

    #[test]
    fn with_and_set_style_pairs_are_equivalent() {
        for n in 0..4 {
            let s = style(n);

            let a = NumberInput::create(1.0).with_placeholder_style(s.clone());
            let mut b = NumberInput::create(1.0);
            b.set_placeholder_style(s.clone());
            assert_eq!(a, b, "with_placeholder_style != set_placeholder_style ({n})");

            let a = NumberInput::create(1.0).with_container_style(s.clone());
            let mut b = NumberInput::create(1.0);
            b.set_container_style(s.clone());
            assert_eq!(a, b, "with_container_style != set_container_style ({n})");

            let a = NumberInput::create(1.0).with_label_style(s.clone());
            let mut b = NumberInput::create(1.0);
            b.set_label_style(s);
            assert_eq!(a, b, "with_label_style != set_label_style ({n})");
        }
    }

    #[test]
    fn style_setters_write_to_disjoint_fields() {
        let placeholder = style(1);
        let container = style(2);
        let label = style(3);
        assert_ne!(placeholder, container, "the fixture must be distinguishable");
        assert_ne!(container, label, "the fixture must be distinguishable");

        let input = NumberInput::create(0.0)
            .with_placeholder_style(placeholder.clone())
            .with_container_style(container.clone())
            .with_label_style(label.clone());

        assert_eq!(input.text_input.placeholder_style, placeholder);
        assert_eq!(input.text_input.container_style, container);
        assert_eq!(input.text_input.label_style, label);
        assert_eq!(
            input.style,
            NumberInput::default().style,
            "NumberInput::style is not a dumping ground for the TextInput styles",
        );
    }

    #[test]
    fn with_and_set_callback_pairs_are_equivalent() {
        // The same `RefAny` handle on both sides: `RefAny` equality is identity of the
        // shared allocation, so two independent `RefAny::new(0u32)` would never match.
        let data = RefAny::new(0u32);

        let a = NumberInput::create(1.0).with_on_value_change(
            data.clone(),
            record_value_change as NumberInputOnValueChangeCallbackType,
        );
        let mut b = NumberInput::create(1.0);
        b.set_on_value_change(
            data.clone(),
            record_value_change as NumberInputOnValueChangeCallbackType,
        );
        assert_eq!(a, b, "with_on_value_change != set_on_value_change");

        let a = NumberInput::create(1.0).with_on_focus_lost(
            data.clone(),
            record_focus_lost as NumberInputOnFocusLostCallbackType,
        );
        let mut b = NumberInput::create(1.0);
        b.set_on_focus_lost(
            data.clone(),
            record_focus_lost as NumberInputOnFocusLostCallbackType,
        );
        assert_eq!(a, b, "with_on_focus_lost != set_on_focus_lost");

        let a = NumberInput::create(1.0).with_on_text_input(
            data.clone(),
            accept_everything as TextInputOnTextInputCallbackType,
        );
        let mut b = NumberInput::create(1.0);
        b.set_on_text_input(
            data.clone(),
            accept_everything as TextInputOnTextInputCallbackType,
        );
        assert_eq!(a, b, "with_on_text_input != set_on_text_input");

        let a = NumberInput::create(1.0).with_on_virtual_key_down(
            data.clone(),
            reject_everything as TextInputOnVirtualKeyDownCallbackType,
        );
        let mut b = NumberInput::create(1.0);
        b.set_on_virtual_key_down(data, reject_everything as TextInputOnVirtualKeyDownCallbackType);
        assert_eq!(a, b, "with_on_virtual_key_down != set_on_virtual_key_down");
    }

    #[test]
    fn setting_a_hook_twice_keeps_the_last_one() {
        let first = RefAny::new(1u32);
        let second = RefAny::new(2u32);

        let mut input = NumberInput::create(0.0);
        input.set_on_value_change(
            first.clone(),
            record_value_change as NumberInputOnValueChangeCallbackType,
        );
        input.set_on_value_change(
            second.clone(),
            record_value_change as NumberInputOnValueChangeCallbackType,
        );

        let stored = input
            .number_input_state
            .on_value_change
            .as_ref()
            .expect("the hook must be set");
        assert_eq!(stored.refany, second, "the last hook must win");
        assert_ne!(stored.refany, first, "the first hook must be released");
    }

    #[test]
    fn swap_with_default_hands_back_the_original_and_leaves_a_fresh_widget() {
        let data = RefAny::new(0u32);
        let mut input = NumberInput::create(7.5)
            .with_on_value_change(
                data,
                record_value_change as NumberInputOnValueChangeCallbackType,
            )
            .with_label_style(style(2));
        let original = input.clone();

        let taken = input.swap_with_default();
        assert_eq!(taken, original, "swap_with_default must return the original");
        assert_eq!(
            input,
            NumberInput::create(0.0),
            "the receiver must be left as a fresh 0.0 widget",
        );
        assert_eq!(
            input,
            NumberInput::default(),
            "…which is also exactly the Default widget",
        );

        // Idempotent on an already-defaulted receiver.
        let second = input.swap_with_default();
        assert_eq!(second, NumberInput::default());
        assert_eq!(input, NumberInput::default());
    }

    // ==================================================================
    // NumberInput::dom — encode / decode round-trip
    // ==================================================================

    #[test]
    fn dom_wires_the_text_input_and_parks_the_cursor_at_the_end() {
        let dom = NumberInput::create(-12.5).dom();
        assert_eq!(
            dom.children.as_ref().len(),
            2,
            "TextInput renders a placeholder node and a label node",
        );
        assert_eq!(
            dom.root.callbacks.as_ref().len(),
            5,
            "focus received/lost, text input, virtual key down, hover",
        );
        assert!(
            dom.children.as_ref()[PLACEHOLDER]
                .children
                .as_ref()
                .first()
                .and_then(|c| c.root.get_node_type().format())
                .is_some(),
            "the placeholder child must wrap a text node",
        );
        assert_eq!(buffer_text(&dom), "-12.5");
        assert_eq!(displayed_text(&dom), "-12.5");
        assert_eq!(
            cursor_pos(&dom),
            "-12.5".chars().count(),
            "the cursor must sit at the end of the rendered number",
        );
    }

    #[test]
    fn dom_text_round_trips_back_to_the_same_f32() {
        for v in finite_samples() {
            let dom = NumberInput::create(v).dom();
            let text = buffer_text(&dom);
            let parsed: f32 = text.parse().unwrap_or_else(|e| {
                panic!("the widget rendered {v:?} as {text:?}, which is not a float: {e}")
            });
            assert!(
                same(parsed, v),
                "{v:?} was rendered as {text:?} and read back as {parsed:?}",
            );
            assert_eq!(
                displayed_text(&dom),
                text,
                "the visible label and the edit buffer must agree for {v:?}",
            );
        }
    }

    #[test]
    fn dom_renders_the_shortest_round_trip_form() {
        for (value, expected) in [
            (0.0f32, "0"),
            (1.0, "1"),
            (-1.5, "-1.5"),
            (42.25, "42.25"),
            (0.5, "0.5"),
        ] {
            assert_eq!(buffer_text(&NumberInput::create(value).dom()), expected);
        }
    }

    #[test]
    fn dom_renders_non_finite_values_as_inf_and_nan() {
        assert_eq!(buffer_text(&NumberInput::create(f32::INFINITY).dom()), "inf");
        assert_eq!(
            buffer_text(&NumberInput::create(f32::NEG_INFINITY).dom()),
            "-inf",
        );
        assert_eq!(buffer_text(&NumberInput::create(f32::NAN).dom()), "NaN");
        assert_eq!(
            buffer_text(&NumberInput::create(-f32::NAN).dom()),
            "NaN",
            "the sign of a NaN is not rendered, so it cannot round-trip",
        );
    }

    #[test]
    fn every_string_the_widget_renders_is_accepted_by_its_own_validator() {
        let mut values = finite_samples().to_vec();
        values.extend_from_slice(&[f32::INFINITY, f32::NEG_INFINITY, f32::NAN]);

        for v in values {
            let text = buffer_text(&NumberInput::create(v).dom());
            let state = RefAny::new(wrapper(0.0, f32::MIN, f32::MAX));
            let (r, _) = validate_one(&state, &text);
            assert_eq!(
                r.valid,
                TextInputValid::Yes,
                "the widget renders {v:?} as {text:?} but then refuses to parse it back",
            );
        }
    }

    #[test]
    fn dom_renders_a_value_that_is_outside_its_own_range() {
        // Neither `create` nor `dom` consults min/max — only typed input is clamped.
        // A widget constructed out of range therefore *shows* a number it would never
        // accept from the keyboard.
        let mut input = NumberInput::create(1000.0);
        input.number_input_state.inner.min = 0.0;
        input.number_input_state.inner.max = 10.0;
        assert_eq!(buffer_text(&input.dom()), "1000");
    }

    #[test]
    fn dom_replaces_a_user_supplied_text_input_hook_with_the_numeric_validator() {
        // `dom()` unconditionally overwrites `on_text_input`, so a hook installed via
        // `with_on_text_input` never fires. `accept_everything` would answer `Yes` to
        // "abc"; the numeric validator answers `No`.
        let dom = NumberInput::create(1.0)
            .with_on_text_input(
                RefAny::new(0u32),
                accept_everything as TextInputOnTextInputCallbackType,
            )
            .dom();

        let r = drive_text_input(&dom, "abc");
        assert_eq!(
            r.valid,
            TextInputValid::No,
            "the numeric validator must own the text-input hook after dom()",
        );
        assert_eq!(r.update, Update::DoNothing);
    }

    #[test]
    fn dom_keeps_a_user_supplied_virtual_key_hook() {
        let dom = NumberInput::create(1.0)
            .with_on_virtual_key_down(
                RefAny::new(0u32),
                reject_everything as TextInputOnVirtualKeyDownCallbackType,
            )
            .dom();

        let r = drive_virtual_key_down(&dom)
            .expect("with_on_virtual_key_down must survive rendering");
        assert_eq!(r.update, Update::RefreshDomAllWindows);
        assert_eq!(r.valid, TextInputValid::No);
    }

    #[test]
    fn dom_wires_the_value_change_hook_through_the_rendered_widget() {
        let recorder = RefAny::new(Recorder::new(Update::RefreshDom));
        let dom = NumberInput::create(0.0)
            .with_on_value_change(
                recorder.clone(),
                record_value_change as NumberInputOnValueChangeCallbackType,
            )
            .dom();

        let r = drive_text_input(&dom, "12,5");
        assert_eq!(r.valid, TextInputValid::Yes);
        assert_eq!(
            r.update,
            Update::RefreshDom,
            "validate must return whatever the user's hook returned",
        );

        let seen = recorded(&recorder);
        assert_eq!(seen.len(), 1, "the hook must fire exactly once per edit");
        assert!(same(seen[0].number, 12.5));
        assert!(same(seen[0].previous, 0.0));

        assert!(
            same(read(&number_state_of(&dom)).number, 12.5),
            "the state behind the rendered DOM must have been updated too",
        );
    }

    // ==================================================================
    // validate_text_input — the parser
    // ==================================================================

    #[test]
    fn validate_rejects_malformed_input_without_touching_the_state() {
        // One state for the whole table: a rejected edit must not accumulate either.
        let state = RefAny::new(wrapper(7.5, -100.0, 100.0));
        with_info(|info| {
            for text in MALFORMED {
                let r = validate_text_input(state.clone(), info, text_state(text));
                assert_eq!(r.valid, TextInputValid::No, "{text:?} must be rejected");
                assert_eq!(
                    r.update,
                    Update::DoNothing,
                    "a rejected edit must not trigger a relayout ({text:?})",
                );
                let after = read(&state);
                assert!(
                    same(after.number, 7.5),
                    "{text:?} changed the value to {}",
                    after.number,
                );
                assert!(
                    same(after.previous, 0.0),
                    "{text:?} touched `previous` ({})",
                    after.previous,
                );
            }
        });
    }

    #[test]
    fn validate_lets_a_transient_prefix_exist_without_touching_the_value() {
        let state = RefAny::new(wrapper(7.5, -100.0, 100.0));
        with_info(|info| {
            for text in TRANSIENT_PREFIXES {
                assert!(is_float_prefix(text), "{text:?} is a prefix of a number");
                let r = validate_text_input(state.clone(), info, text_state(text));
                assert_eq!(
                    r.valid,
                    TextInputValid::Yes,
                    "{text:?} must be allowed to exist in the field — `-5` starts with `-`",
                );
                assert_eq!(r.update, Update::DoNothing, "{text:?}: nothing to relayout yet");
                let after = read(&state);
                assert!(same(after.number, 7.5), "{text:?} changed the value to {}", after.number);
                assert!(same(after.previous, 0.0), "{text:?} touched `previous`");
            }
            for text in MALFORMED {
                assert!(!is_float_prefix(text), "{text:?} can never become a number");
            }
            // And the prefix completes into the number it was heading for.
            let r = validate_text_input(state.clone(), info, text_state("-5"));
            assert_eq!(r.valid, TextInputValid::Yes);
            assert!(same(read(&state).number, -5.0));
            let r = validate_text_input(state.clone(), info, text_state(".5"));
            assert_eq!(r.valid, TextInputValid::Yes);
            assert!(same(read(&state).number, 0.5));
        });
    }

    #[test]
    fn a_transient_prefix_does_not_reach_the_users_hook() {
        let recorder = RefAny::new(Recorder::new(Update::RefreshDom));
        let state = RefAny::new(wrapper_with_value_hook(1.0, f32::MIN, f32::MAX, &recorder));
        with_info(|info| {
            for text in TRANSIENT_PREFIXES {
                let _ = validate_text_input(state.clone(), info, text_state(text));
            }
        });
        assert!(recorded(&recorder).is_empty(), "an incomplete number is not a value change");
    }

    #[test]
    fn validate_rejects_digits_that_are_not_ascii_digits() {
        let state = RefAny::new(wrapper(3.0, f32::MIN, f32::MAX));
        with_info(|info| {
            for text in NON_ASCII_DIGITS {
                let r = validate_text_input(state.clone(), info, text_state(text));
                assert_eq!(r.valid, TextInputValid::No, "{text:?} must be rejected");
                assert!(
                    same(read(&state).number, 3.0),
                    "{text:?} must not change the value",
                );
            }
        });
    }

    #[test]
    fn validate_accepts_every_form_the_rust_float_parser_accepts() {
        let state = RefAny::new(wrapper(-1.0, f32::MIN, f32::MAX));
        with_info(|info| {
            for (text, expected) in ACCEPTED {
                reset(&state, -1.0);
                let r = validate_text_input(state.clone(), info, text_state(text));
                assert_eq!(r.valid, TextInputValid::Yes, "{text:?} must be accepted");
                assert_eq!(
                    r.update,
                    Update::DoNothing,
                    "no hook is installed, so there is nothing to redraw ({text:?})",
                );
                let after = read(&state);
                assert!(
                    same(after.number, expected),
                    "{text:?} stored {} (expected {expected})",
                    after.number,
                );
                assert!(
                    same(after.previous, -1.0),
                    "{text:?} must push the old value into `previous`, got {}",
                    after.previous,
                );
            }
        });
    }

    #[test]
    fn validate_reads_a_comma_as_a_decimal_point() {
        with_info(|info| {
            for (text, expected) in [
                ("1,5", 1.5f32),
                ("-1,5", -1.5),
                (",5", 0.5),
                ("1,", 1.0),
                ("1,25e2", 125.0),
            ] {
                let state = RefAny::new(wrapper(0.0, f32::MIN, f32::MAX));
                let r = validate_text_input(state.clone(), info, text_state(text));
                assert_eq!(r.valid, TextInputValid::Yes, "{text:?} must be accepted");
                assert!(
                    same(read(&state).number, expected),
                    "{text:?} stored {} (expected {expected})",
                    read(&state).number,
                );
            }

            // A comma is rewritten, not deleted: a second one is still a parse error.
            for text in [",", ",,", "1,,5", "1,5,5"] {
                let state = RefAny::new(wrapper(0.0, f32::MIN, f32::MAX));
                let r = validate_text_input(state.clone(), info, text_state(text));
                assert_eq!(r.valid, TextInputValid::No, "{text:?} must be rejected");
            }
        });
    }

    #[test]
    fn validate_reads_a_thousands_separator_as_a_decimal_point() {
        // The `,` -> `.` rewrite is unconditional, so "1,000" (US grouping for one
        // thousand) is silently read as *one*. That is the price of supporting the
        // European decimal comma, and it is worth pinning down: the value a user
        // typed changes by three orders of magnitude with no rejection.
        let state = RefAny::new(wrapper(0.0, f32::MIN, f32::MAX));
        let (r, after) = validate_one(&state, "1,000");
        assert_eq!(r.valid, TextInputValid::Yes);
        assert!(
            same(after.number, 1.0),
            "\"1,000\" is read as {} (the comma is a decimal point here)",
            after.number,
        );

        let (r, _) = validate_one(&state, "1,000,000");
        assert_eq!(
            r.valid,
            TextInputValid::No,
            "a second group makes it un-parseable rather than ambiguous",
        );
    }

    #[test]
    fn validate_silently_drops_code_units_that_are_not_unicode_scalars() {
        // The edit buffer is a `U32Vec`, so it can hold unpaired surrogates and
        // out-of-range code units. `char::from_u32` returns None for those and the
        // filter *drops* them, so "1<D800>5" is read as the number 15 rather than
        // being rejected as malformed.
        let state = RefAny::new(wrapper(0.0, f32::MIN, f32::MAX));

        let (r, after) = validate_raw(&state, &[0x31, 0xD800, 0x35]);
        assert_eq!(r.valid, TextInputValid::Yes);
        assert!(
            same(after.number, 15.0),
            "a non-scalar code unit between two digits is dropped, got {}",
            after.number,
        );

        // …but a buffer made *only* of non-scalars collapses to the empty string,
        // which is rejected rather than read as zero.
        let (r, after) = validate_raw(&state, &[0xD800, 0xDFFF, 0x0011_0000]);
        assert_eq!(r.valid, TextInputValid::No);
        assert!(
            same(after.number, 15.0),
            "a rejected edit must not change the value",
        );
    }

    #[test]
    fn validate_survives_pathologically_long_input() {
        let state = RefAny::new(wrapper(0.0, f32::MIN, f32::MAX));

        for (text, expected) in [
            ("9".repeat(10_000), f32::MAX),          // overflows to +inf, saturates
            (format!("-{}", "9".repeat(10_000)), f32::MIN),
            (format!("{}1", "0".repeat(10_000)), 1.0), // leading zeros
            (format!("0.{}", "0".repeat(10_000)), 0.0),
            ("1e999999999".to_string(), f32::MAX),   // exponent overflow
            ("1e-999999999".to_string(), 0.0),       // exponent underflow
        ] {
            let (r, after) = validate_one(&state, &text);
            assert_eq!(
                r.valid,
                TextInputValid::Yes,
                "a {}-char input must parse, not error",
                text.len(),
            );
            assert!(
                same(after.number, expected),
                "a {}-char input stored {} (expected {expected})",
                text.len(),
                after.number,
            );
        }
    }

    // ==================================================================
    // validate_text_input — numeric limits
    // ==================================================================

    #[test]
    fn validate_saturates_overflow_at_the_configured_bounds() {
        let state = RefAny::new(wrapper(0.0, f32::MIN, f32::MAX));

        // "1e39" is well past f32::MAX, so the parser returns +inf rather than an
        // error — the saturation has to happen here, in the clamp.
        let (r, after) = validate_one(&state, "1e39");
        assert_eq!(r.valid, TextInputValid::Yes);
        assert!(
            same(after.number, f32::MAX),
            "an overflowing value must saturate at `max`, got {}",
            after.number,
        );

        let (_, after) = validate_one(&state, "-1e39");
        assert!(
            same(after.number, f32::MIN),
            "…and at `min` on the negative side, got {}",
            after.number,
        );
    }

    #[test]
    fn validate_underflow_keeps_the_sign_of_zero() {
        let state = RefAny::new(wrapper(1.0, f32::MIN, f32::MAX));

        let (_, after) = validate_one(&state, "1e-46");
        assert!(
            same(after.number, 0.0),
            "a positive underflow must land on +0.0, got {}",
            after.number,
        );

        let (_, after) = validate_one(&state, "-1e-46");
        assert!(
            same(after.number, -0.0),
            "a negative underflow must land on -0.0, got {}",
            after.number,
        );
    }

    #[test]
    fn validate_clamps_into_range() {
        with_info(|info| {
            for (min, max, text, expected) in [
                (-10.0f32, 10.0f32, "1000", 10.0f32),
                (-10.0, 10.0, "-1000", -10.0),
                (-10.0, 10.0, "inf", 10.0),
                (-10.0, 10.0, "-inf", -10.0),
                (-10.0, 10.0, "10", 10.0),          // exactly on the bound
                (-10.0, 10.0, "-10", -10.0),
                (-10.0, 10.0, "10.000001", 10.0),   // one ulp past the bound
                (-10.0, 10.0, "0", 0.0),
                (0.0, 0.0, "5", 0.0),               // a single-point range is legal
                (0.0, 0.0, "-5", 0.0),
                (5.0, 5.0, "0", 5.0),
            ] {
                let state = RefAny::new(wrapper(0.0, min, max));
                let r = validate_text_input(state.clone(), info, text_state(text));
                assert_eq!(r.valid, TextInputValid::Yes, "{text:?} must be accepted");
                let after = read(&state);
                assert!(
                    same(after.number, expected),
                    "{text:?} in [{min}, {max}] stored {} (expected {expected})",
                    after.number,
                );
                assert!(
                    after.number >= min && after.number <= max,
                    "{text:?} escaped [{min}, {max}] as {}",
                    after.number,
                );
            }
        });
    }

    #[test]
    fn validate_stores_nan_unclamped() {
        // `f32::clamp` compares, and every comparison against NaN is false, so a NaN
        // walks straight through the range check. The widget's "number is always in
        // [min, max]" invariant therefore has exactly one hole, and it is reachable
        // by typing "nan" into the field.
        let state = RefAny::new(wrapper(0.0, -1.0, 1.0));
        let (r, after) = validate_one(&state, "NaN");
        assert_eq!(
            r.valid,
            TextInputValid::Yes,
            "the Rust float parser accepts \"NaN\", so the widget does too",
        );
        assert!(
            after.number.is_nan(),
            "NaN survives the clamp untouched, got {}",
            after.number,
        );
    }

    #[test]
    fn validate_tracks_previous_as_the_last_accepted_value() {
        let state = RefAny::new(wrapper(0.0, 0.0, 10.0));
        with_info(|info| {
            for (text, previous, number) in [
                ("1", 0.0f32, 1.0f32),
                ("2", 1.0, 2.0),
                ("100", 2.0, 10.0),   // clamped
                ("200", 10.0, 10.0),  // `previous` is the *clamped* old value
                ("abc", 10.0, 10.0),  // rejected: neither field moves
                ("-5", 10.0, 0.0),
            ] {
                let _ = validate_text_input(state.clone(), info, text_state(text));
                let after = read(&state);
                assert!(
                    same(after.previous, previous),
                    "after {text:?}: previous = {} (expected {previous})",
                    after.previous,
                );
                assert!(
                    same(after.number, number),
                    "after {text:?}: number = {} (expected {number})",
                    after.number,
                );
            }
        });
    }

    /// `validate_text_input` runs `f32::clamp(min, max)` on every value it parses,
    /// and `f32::clamp` **panics** unless `min <= max` — which a NaN bound also
    /// fails. `min`/`max` are `pub` fields on a `#[repr(C)]` struct that crosses the
    /// C/FFI boundary, so nothing stops a caller from handing the widget an inverted
    /// or NaN-bounded range, and a panic inside a UI callback takes the app with it.
    /// Rejecting the edit (`TextInputValid::No`) or normalising the range would both
    /// be safe; unwinding is not.
    #[test]
    fn validate_with_a_degenerate_range_must_not_panic() {
        let degenerate: [(f32, f32); 5] = [
            (10.0, 5.0),
            (1.0, -1.0),
            (f32::NAN, 10.0),
            (0.0, f32::NAN),
            (f32::NAN, f32::NAN),
        ];

        let panicked: Vec<(f32, f32)> = degenerate
            .iter()
            .copied()
            .filter(|&(min, max)| {
                let state = RefAny::new(wrapper(0.0, min, max));
                catch_unwind(AssertUnwindSafe(|| {
                    let _ = validate_one(&state, "1");
                }))
                .is_err()
            })
            .collect();

        assert!(
            panicked.is_empty(),
            "typing a digit into a NumberInput whose [min, max] range is inverted or \
             NaN-bounded panics (f32::clamp asserts min <= max) instead of rejecting \
             the input; offending ranges: {panicked:?}",
        );
    }

    // ==================================================================
    // validate_text_input — hooks and payload handling
    // ==================================================================

    #[test]
    fn validate_with_a_foreign_payload_accepts_the_edit_unchanged() {
        // The downcast guard bails out *before* parsing, so a mis-wired NumberInput
        // reports arbitrary text as valid instead of rejecting it.
        let state = RefAny::new(0u32);
        let r = with_info(|info| {
            validate_text_input(state.clone(), info, text_state("not a number"))
        });
        assert_eq!(r.update, Update::DoNothing);
        assert_eq!(r.valid, TextInputValid::Yes);

        let mut state = state;
        assert_eq!(
            *state
                .downcast_ref::<u32>()
                .expect("the foreign payload must be left alone"),
            0,
        );
    }

    #[test]
    fn validate_does_not_invoke_the_value_change_hook_for_rejected_input() {
        let recorder = RefAny::new(Recorder::new(Update::RefreshDom));
        let state = RefAny::new(wrapper_with_value_hook(
            1.0,
            f32::MIN,
            f32::MAX,
            &recorder,
        ));

        with_info(|info| {
            for text in MALFORMED {
                let _ = validate_text_input(state.clone(), info, text_state(text));
            }
        });
        assert!(
            recorded(&recorder).is_empty(),
            "a rejected edit must not reach the user's hook",
        );

        // Sanity: the hook *is* wired up and does fire for a well-formed edit.
        let (r, _) = validate_one(&state, "2");
        assert_eq!(r.update, Update::RefreshDom);
        assert_eq!(recorded(&recorder).len(), 1);
    }

    #[test]
    fn validate_hands_the_hook_the_clamped_state_and_returns_its_update() {
        let recorder = RefAny::new(Recorder::new(Update::RefreshDomAllWindows));
        let state = RefAny::new(wrapper_with_value_hook(4.0, 0.0, 10.0, &recorder));

        let (r, after) = validate_one(&state, "1000");
        assert_eq!(
            r.update,
            Update::RefreshDomAllWindows,
            "validate must forward the hook's Update verbatim",
        );
        assert_eq!(r.valid, TextInputValid::Yes);

        let seen = recorded(&recorder);
        assert_eq!(seen.len(), 1);
        assert!(
            same(seen[0].number, 10.0),
            "the hook must see the clamped value, not the raw 1000, got {}",
            seen[0].number,
        );
        assert!(same(seen[0].previous, 4.0), "…and the previous value");
        assert_eq!(
            seen[0], after,
            "the hook's copy and the stored state must agree",
        );
    }

    // ==================================================================
    // on_focus_lost
    // ==================================================================

    #[test]
    fn focus_lost_with_a_foreign_payload_does_nothing() {
        let state = RefAny::new(0u32);
        assert_eq!(focus_lost(&state, "123"), Update::DoNothing);

        let mut state = state;
        assert_eq!(
            *state
                .downcast_ref::<u32>()
                .expect("the foreign payload must be left alone"),
            0,
        );
    }

    #[test]
    fn focus_lost_without_a_hook_does_nothing() {
        let state = RefAny::new(wrapper(1.5, 0.0, 10.0));
        assert_eq!(focus_lost(&state, "123"), Update::DoNothing);
        let after = read(&state);
        assert!(same(after.number, 1.5) && same(after.previous, 0.0));
    }

    #[test]
    fn focus_lost_reports_the_stored_number_and_ignores_the_text_buffer() {
        // `on_focus_lost` never looks at the `TextInputState` it is handed: the value
        // it reports is the one the *validator* accepted, not whatever happens to be
        // sitting in the buffer.
        let recorder = RefAny::new(Recorder::new(Update::RefreshDom));
        let state = RefAny::new(wrapper_with_focus_hook(1.5, 0.0, 10.0, &recorder));

        assert_eq!(
            focus_lost(&state, "999"),
            Update::RefreshDom,
            "the hook's Update must be forwarded verbatim",
        );

        let seen = recorded(&recorder);
        assert_eq!(seen.len(), 1, "the hook must fire exactly once");
        assert!(
            same(seen[0].number, 1.5),
            "the hook saw {} — the text buffer must not be re-parsed",
            seen[0].number,
        );
        assert!(same(seen[0].previous, 0.0));
    }

    #[test]
    fn focus_lost_neither_mutates_nor_clamps() {
        // A state built out of range (see `dom_renders_a_value_that_is_outside_its_own_range`)
        // is reported verbatim: focus-lost is a read-only notification.
        let recorder = RefAny::new(Recorder::new(Update::DoNothing));
        let state = RefAny::new(wrapper_with_focus_hook(1000.0, 0.0, 10.0, &recorder));

        assert_eq!(focus_lost(&state, ""), Update::DoNothing);

        let seen = recorded(&recorder);
        assert_eq!(seen.len(), 1);
        assert!(
            same(seen[0].number, 1000.0),
            "focus-lost must not clamp, got {}",
            seen[0].number,
        );

        let after = read(&state);
        assert!(
            same(after.number, 1000.0) && same(after.previous, 0.0),
            "focus-lost must not mutate the state",
        );
    }

    #[test]
    fn focus_lost_is_repeatable() {
        let recorder = RefAny::new(Recorder::new(Update::DoNothing));
        let state = RefAny::new(wrapper_with_focus_hook(2.5, 0.0, 10.0, &recorder));

        for _ in 0..8 {
            assert_eq!(focus_lost(&state, "2.5"), Update::DoNothing);
        }

        let seen = recorded(&recorder);
        assert_eq!(seen.len(), 8, "every focus loss must reach the hook");
        assert!(
            seen.iter().all(|s| same(s.number, 2.5)),
            "repeated focus losses must keep reporting the same value",
        );
    }
}
