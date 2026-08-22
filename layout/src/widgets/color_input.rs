//! A colour swatch that opens a colour picker in a real popup window.
//!
//! The swatch is the control the app places in its layout; clicking it opens
//! a `<transient-window>` anchored below it (a real OS popup — see
//! `azul_core::transient`) holding a Chrome-style picker: a saturation/value
//! plane, a hue bar, a preview swatch with the hex value, and R/G/B fields.
//! Every change fires `on_value_change` with the new colour; the app stores
//! it and passes it back into `ColorInput::create`, like every other widget
//! here. The popup closes on an outside click, Escape, or a second click on
//! the swatch — the engine handles all three, so the app carries no flag.
//!
//! What persists across the app's rebuilds lives in the swatch's dataset
//! (`ColorPickerData`, kept by a merge callback): whether the picker is open,
//! the hue/saturation the user is on (so dragging through black does not
//! forget the hue), and an in-progress drag.

use alloc::{format, string::String, vec, vec::Vec};

use azul_core::{
    callbacks::Update,
    dom::{Dom, DomNodeId, NodeData, NodeType},
    refany::RefAny,
    transient::{TransientAnchor, TransientDismiss, TransientWindowConfig},
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
#[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
use azul_css::{OptionString, 
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::callbacks::{Callback, CallbackInfo};

/// Rectangular input that displays a color and opens a picker when clicked.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ColorInput {
    pub color_input_state: ColorInputStateWrapper,
    pub style: CssPropertyWithConditionsVec,
    /// What this control is CALLED, for assistive technology.
    ///
    /// Carried by the WIDGET so it knows at build time whether it was named;
    /// forwarded into the accessibility declaration it already builds.
    pub accessibility_name: OptionString,
}

/// Callback function type invoked when the color input value changes.
pub type ColorInputOnValueChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ColorInputState) -> Update;
impl_widget_callback!(
    ColorInputOnValueChange,
    OptionColorInputOnValueChange,
    ColorInputOnValueChangeCallback,
    ColorInputOnValueChangeCallbackType
);

/// Wrapper around [`ColorInputState`] that includes a title and an optional value-change callback.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct ColorInputStateWrapper {
    pub inner: ColorInputState,
    pub title: AzString,
    pub on_value_change: OptionColorInputOnValueChange,
}

impl Default for ColorInputStateWrapper {
    fn default() -> Self {
        Self {
            inner: ColorInputState::default(),
            title: AzString::from_const_str("Pick color"),
            on_value_change: None.into(),
        }
    }
}

/// Holds the current color value of a [`ColorInput`] widget.
#[derive(Copy, Debug, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub struct ColorInputState {
    pub color: ColorU,
}

impl Default for ColorInputState {
    fn default() -> Self {
        Self {
            color: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
        }
    }
}

static DEFAULT_COLOR_INPUT_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(14))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(14))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
];

/// Class on the swatch (the control itself).
pub const COLOR_INPUT_CLASS: &str = "__azul_native_color_input";
/// Class on the picker panel inside the popup.
pub const COLOR_PICKER_CLASS: &str = "__azul_native_color_picker";
/// Class on the saturation/value plane.
pub const COLOR_PICKER_PLANE_CLASS: &str = "__azul_native_color_picker_plane";
/// Class on the hue bar.
pub const COLOR_PICKER_HUE_CLASS: &str = "__azul_native_color_picker_hue";
/// Class on the alpha bar.
pub const COLOR_PICKER_ALPHA_CLASS: &str = "__azul_native_color_picker_alpha";
/// Class on the picker's grip strip: drag it to tear the picker off into a
/// floating palette, drag the palette back over the swatch to dock it.
pub const COLOR_PICKER_GRIP_CLASS: &str = "__azul_native_color_picker_grip";
/// Class on the eyedropper button (`pick_screen_color`).
pub const COLOR_PICKER_EYEDROPPER_CLASS: &str = "__azul_native_color_picker_eyedropper";

/// Width of the plane and hue bar, in px. The panel is this plus padding.
const PLANE_WIDTH: f32 = 216.0;
/// Height of the plane, in px.
const PLANE_HEIGHT: f32 = 150.0;

impl ColorInput {
    /// Name this control for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    /// Creates a new `ColorInput` displaying the given color.
    #[inline]
    #[must_use]
    pub fn create(color: ColorU) -> Self {
        Self {
            color_input_state: ColorInputStateWrapper {
                inner: ColorInputState { color },
                ..Default::default()
            },
            style: CssPropertyWithConditionsVec::from_const_slice(DEFAULT_COLOR_INPUT_STYLE),
            accessibility_name: OptionString::None,
        }
    }

    /// Sets the callback invoked when the color value changes.
    #[inline]
    pub fn set_on_value_change<I: Into<ColorInputOnValueChangeCallback>>(
        &mut self,
        data: RefAny,
        callback: I,
    ) {
        self.color_input_state.on_value_change = Some(ColorInputOnValueChange {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// Builder-style method to set the value-change callback.
    #[inline]
    #[must_use]
    pub fn with_on_value_change<C: Into<ColorInputOnValueChangeCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_value_change(data, callback);
        self
    }

    /// Replaces `self` with a default `ColorInput` and returns the previous value.
    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this `ColorInput` into a styled [`Dom`]: the swatch, with the
    /// picker popup attached as its (closed) transient child.
    #[inline]
    #[must_use]
    pub fn dom(self) -> Dom {
        use azul_core::{
            a11y::{AccessibilityInfo, AccessibilityRole},
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{ComponentEventFilter, EventFilter, HoverEventFilter, IdOrClass::Class},
        };

        let color = self.color_input_state.inner.color;
        let title = self.color_input_state.title.clone();
        let a11y_name = match self.accessibility_name {
            OptionString::Some(n) => n,
            OptionString::None => title,
        };

        let mut style = self.style.into_library_owned_vec();
        style.push(CssPropertyWithConditions::simple(
            CssProperty::const_background_content(
                vec![StyleBackgroundContent::Color(color)].into(),
            ),
        ));

        // The persistent half: hue/sat survive a pass through black, the
        // open flag survives the app's rebuilds, a drag survives a move.
        let data = RefAny::new(ColorPickerData {
            state: self.color_input_state,
            hsv: Hsv::from_color(color),
            open: false,
            drag: Drag::None,
        });

        let panel = picker_panel(&data, color);

        // `tearoff`: the grip strip at the top of the panel tears the picker
        // off into a floating palette (a real toplevel) and docks it back.
        let mut transient = NodeData::create_node(NodeType::TransientWindow(
            TransientWindowConfig::closed()
                .with_anchor(TransientAnchor::Bottom)
                .with_dismiss(TransientDismiss::Outside)
                .with_tearoff(azul_core::transient::TransientTearoff::Free),
        ));
        transient.set_attributes(
            vec![azul_core::dom::AttributeType::Title("Colour".into())].into(),
        );
        transient.add_callback(
            EventFilter::Component(ComponentEventFilter::Dismissed),
            data.clone(),
            Callback::from_ptr(on_picker_dismissed).to_core(),
        );
        let popup = Dom::create_from_data(transient).with_child(panel);

        // A translucent colour shows a checkerboard through it: the board and
        // a colour overlay go in as children, under the popup node. The
        // swatch's own background stays the colour (what an opaque swatch is).
        let translucent = color.a < 255;
        let mut swatch = Dom::create_div();
        if translucent {
            style.push(CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)));
            style.push(CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)));
            style.push(CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)));
            let (w, h) = swatch_size(&style);
            swatch = swatch
                .with_child(checkerboard(w, h, (w.min(h) / 2.0).max(1.0)))
                .with_child(Dom::create_div().with_css(&format!(
                    "position: absolute; left: 0px; top: 0px; width: 100%; height: 100%; background: {};",
                    css_rgba(ColorU { a: 255, ..color }, color.a)
                )));
        }

        swatch
            .with_ids_and_classes(vec![Class(COLOR_INPUT_CLASS.into())].into())
            .with_css_props(style.into())
            .with_tab_index(azul_core::dom::TabIndex::Auto)
            .with_accessibility_info(AccessibilityInfo {
                role: AccessibilityRole::PushButton,
                accessibility_name: Some(a11y_name).into(),
                accessibility_value: Some(AzString::from(color_to_hex(color))).into(),
                ..Default::default()
            })
            .with_dataset(Some(data.clone()).into())
            .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(merge_picker_data))
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    refany: data,
                    callback: CoreCallback {
                        cb: on_color_input_clicked as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                }]
                .into(),
            )
            .with_child(popup)
    }
}

/// The swatch's declared width/height (for the checkerboard behind a
/// translucent colour); the 14px default when the style does not say.
fn swatch_size(style: &[CssPropertyWithConditions]) -> (f32, f32) {
    let px = |ty: CssPropertyType, default: f32| -> f32 {
        style
            .iter()
            .rev()
            .find(|p| p.property.get_type() == ty)
            .and_then(|p| match &p.property {
                CssProperty::Width(v) => v.get_property().and_then(|w| match w {
                    LayoutWidth::Px(px) => Some(px.to_pixels_internal(default, 16.0, 16.0)),
                    _ => None,
                }),
                CssProperty::Height(v) => v.get_property().and_then(|h| match h {
                    LayoutHeight::Px(px) => Some(px.to_pixels_internal(default, 16.0, 16.0)),
                    _ => None,
                }),
                _ => None,
            })
            .unwrap_or(default)
    };
    (px(CssPropertyType::Width, 14.0), px(CssPropertyType::Height, 14.0))
}

impl From<ColorInput> for Dom {
    fn from(c: ColorInput) -> Self {
        c.dom()
    }
}

// ---------------------------------------------------------------------------
// Colour math — pure, so it can be tested without a DOM.
// ---------------------------------------------------------------------------

/// Hue in degrees `[0, 360)`, saturation and value in `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl Hsv {
    /// RGB → HSV. Grey has no hue; it reports 0.
    #[must_use]
    #[allow(clippy::many_single_char_names)] // r, g, b, h, s, v: the textbook formula
    pub fn from_color(c: ColorU) -> Self {
        let r = f32::from(c.r) / 255.0;
        let g = f32::from(c.g) / 255.0;
        let b = f32::from(c.b) / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let h = if delta <= f32::EPSILON {
            0.0
        } else if (max - r).abs() <= f32::EPSILON {
            60.0 * (((g - b) / delta) % 6.0)
        } else if (max - g).abs() <= f32::EPSILON {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        let h = if h < 0.0 { h + 360.0 } else { h };
        let s = if max <= f32::EPSILON { 0.0 } else { delta / max };
        Self { h, s, v: max }
    }

    /// HSV → RGB, alpha as given.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::many_single_char_names
    )] // 0..=255 by construction; r, g, b, h, s, v, c, x, m: the textbook formula
    pub fn to_color(self, a: u8) -> ColorU {
        let h = self.h.rem_euclid(360.0);
        let s = self.s.clamp(0.0, 1.0);
        let v = self.v.clamp(0.0, 1.0);
        let c = v * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = v - c;
        let (r, g, b) = match (h / 60.0) as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let ch = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        ColorU { r: ch(r), g: ch(g), b: ch(b), a }
    }
}

/// `#rrggbb`, or `#rrggbbaa` when the colour is not fully opaque.
#[must_use]
pub fn color_to_hex(c: ColorU) -> String {
    if c.a == 255 {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)
    }
}

/// Parse `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` (the `#` optional,
/// case-insensitive, surrounding whitespace ignored). A missing alpha is
/// opaque.
#[must_use]
#[allow(clippy::many_single_char_names)] // r, g, b, a: the channels
pub fn color_from_hex(text: &str) -> Option<ColorU> {
    let t = text.trim().trim_start_matches('#');
    let nib = |ch: u8| -> Option<u8> { char::from(ch).to_digit(16).map(|d| d as u8) };
    let bytes = t.as_bytes();
    let pair = |i: usize| -> Option<u8> { Some(nib(bytes[i])? * 16 + nib(bytes[i + 1])?) };
    let (r, g, b, a) = match bytes.len() {
        3 => (nib(bytes[0])? * 17, nib(bytes[1])? * 17, nib(bytes[2])? * 17, 255),
        4 => (nib(bytes[0])? * 17, nib(bytes[1])? * 17, nib(bytes[2])? * 17, nib(bytes[3])? * 17),
        6 => (pair(0)?, pair(2)?, pair(4)?, 255),
        8 => (pair(0)?, pair(2)?, pair(4)?, pair(6)?),
        _ => return None,
    };
    Some(ColorU { r, g, b, a })
}

// ---------------------------------------------------------------------------
// Persistent widget state
// ---------------------------------------------------------------------------

/// Which control is being dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drag {
    None,
    Plane,
    Hue,
    Alpha,
}

/// The swatch's dataset: what must outlive one build of the DOM.
#[derive(Debug)]
pub struct ColorPickerData {
    state: ColorInputStateWrapper,
    hsv: Hsv,
    open: bool,
    drag: Drag,
}

impl ColorPickerData {
    /// Whether the widget believes its picker is open (tests / diagnostics).
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// The colour the picker currently holds (tests / diagnostics).
    #[must_use]
    pub const fn current_color(&self) -> ColorU {
        self.state.inner.color
    }

    const fn color(&self) -> ColorU {
        self.state.inner.color
    }

    /// Adopt a colour the user picked: keeps the hue/sat the controls are on.
    fn set_hsv(&mut self, hsv: Hsv) {
        let a = self.state.inner.color.a;
        self.hsv = hsv;
        self.state.inner.color = hsv.to_color(a);
    }

    /// Adopt a colour that arrived as RGB (hex / number fields / the app).
    /// Hue and saturation are kept when the RGB is what they already
    /// describe — so a pass through black does not snap the hue to red.
    fn set_color(&mut self, c: ColorU) {
        if self.hsv.to_color(c.a) != c {
            self.hsv = Hsv::from_color(c);
        }
        self.state.inner.color = c;
    }
}

/// Reconcile: the old allocation survives (open flag, hsv, drag), adopting
/// the app's colour and callback from the new build — the app owns the
/// value, exactly like every other widget's `on_value_change` contract.
extern "C" fn merge_picker_data(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    let merged = {
        let new_guard = new_data.downcast_ref::<ColorPickerData>();
        let old_guard = old_data.downcast_mut::<ColorPickerData>();
        if let (Some(new_g), Some(mut old_g)) = (new_guard, old_guard) {
            let app_color = new_g.state.inner.color;
            old_g.state.on_value_change = new_g.state.on_value_change.clone();
            old_g.state.title = new_g.state.title.clone();
            old_g.set_color(app_color);
            true
        } else {
            false
        }
    };
    if merged {
        old_data
    } else {
        new_data
    }
}

// ---------------------------------------------------------------------------
// The panel
// ---------------------------------------------------------------------------

/// The plane's own background: white → the pure hue, left to right. The
/// darkening towards the bottom is a separate overlay child (see
/// [`SHADE_CSS`]) — stacking it as a second background layer painted only
/// one of the two, so the plane never went to black.
fn plane_background_css(hue: f32) -> String {
    format!("linear-gradient(to right, #ffffff, hsl({}, 100%, 50%))", hue.round())
}

/// The plane's shade overlay: transparent at the top, black at the bottom.
const SHADE_CSS: &str = "position: absolute; left: 0px; top: 0px; width: 100%; height: 100%; \
    border-radius: 4px; background: linear-gradient(to bottom, rgba(0, 0, 0, 0), #000000);";

/// `rgba(r, g, b, a)` for CSS, alpha as a fraction.
fn css_rgba(color: ColorU, alpha: u8) -> String {
    format!(
        "rgba({}, {}, {}, {:.3})",
        color.r,
        color.g,
        color.b,
        f32::from(alpha) / 255.0
    )
}

/// Class on a checkerboard (the thing behind a non-opaque colour).
pub const CHECKERBOARD_CLASS: &str = "__azul_native_checkerboard";
/// Class on a checkerboard's light cells.
pub const CHECKERBOARD_LIGHT_CLASS: &str = "__azul_native_checkerboard_light";
/// Class on a checkerboard's dark cells.
pub const CHECKERBOARD_DARK_CLASS: &str = "__azul_native_checkerboard_dark";

/// A checkerboard of `cell`-px squares filling `w`×`h`, built from DIVS so a
/// stylesheet can restyle it (`.__azul_native_checkerboard_dark { … }`) and
/// so it follows the theme: the cells carry a light-theme colour and a dark
/// one gated on `theme: dark`. What sits behind any colour that is not fully
/// opaque. (An image would tile faster, but could not be styled or themed.)
fn checkerboard(w: f32, h: f32, cell: f32) -> Dom {
    use azul_core::dom::IdOrClass::Class;
    use azul_css::dynamic_selector::{DynamicSelector, ThemeCondition};

    let cell_css = |dark: bool| -> CssPropertyWithConditionsVec {
        let (light_mode, dark_mode) = if dark {
            (ColorU { r: 0xcc, g: 0xcc, b: 0xcc, a: 255 }, ColorU { r: 0x33, g: 0x33, b: 0x33, a: 255 })
        } else {
            (ColorU { r: 0xff, g: 0xff, b: 0xff, a: 255 }, ColorU { r: 0x55, g: 0x55, b: 0x55, a: 255 })
        };
        let bg = |c: ColorU| CssProperty::const_background_content(vec![StyleBackgroundContent::Color(c)].into());
        CssPropertyWithConditionsVec::from_vec(vec![
            CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(cell as isize))),
            CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(cell as isize))),
            CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
            CssPropertyWithConditions::simple(bg(light_mode)),
            CssPropertyWithConditions {
                property: bg(dark_mode),
                apply_if: vec![DynamicSelector::Theme(ThemeCondition::Dark)].into(),
            },
        ])
    };
    let cols = (w / cell).ceil().max(1.0) as usize;
    let rows = (h / cell).ceil().max(1.0) as usize;
    let mut board = Dom::create_div()
        .with_ids_and_classes(vec![Class(CHECKERBOARD_CLASS.into())].into())
        .with_css(&format!(
            "position: absolute; left: 0px; top: 0px; width: {w}px; height: {h}px; \
             display: flex; flex-direction: column; overflow: hidden;"
        ));
    for y in 0..rows {
        let mut row = Dom::create_div().with_css("display: flex; flex-direction: row;");
        for x in 0..cols {
            let dark = (x + y) % 2 == 1;
            let class = if dark { CHECKERBOARD_DARK_CLASS } else { CHECKERBOARD_LIGHT_CLASS };
            row = row.with_child(
                Dom::create_div()
                    .with_ids_and_classes(vec![Class(class.into())].into())
                    .with_css_props(cell_css(dark)),
            );
        }
        board = board.with_child(row);
    }
    board
}

const HUE_BACKGROUND_CSS: &str = "linear-gradient(to right, #ff0000 0%, #ffff00 17%, \
    #00ff00 33%, #00ffff 50%, #0000ff 67%, #ff00ff 83%, #ff0000 100%)";

/// A property parsed from its CSS text; `None` if the text does not parse.
fn css_prop(ty: CssPropertyType, value: &str) -> Option<CssProperty> {
    parse_css_property(ty, value).ok()
}

/// The text/number inputs' default container style (border, padding, font)
/// with a fixed width in place of whatever width it declares — extended, not
/// replaced, so the fields inside the picker still look like fields.
fn field_container_style(width_px: isize, grow: bool) -> CssPropertyWithConditionsVec {
    let mut props: Vec<CssPropertyWithConditions> =
        crate::widgets::text_input::TextInput::default().container_style.as_ref().to_vec();
    props.retain(|p| {
        !matches!(
            p.property.get_type(),
            CssPropertyType::Width | CssPropertyType::FlexGrow | CssPropertyType::MinWidth
        )
    });
    props.push(CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(width_px))));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
        isize::from(grow),
    ))));
    props.into()
}

/// The picker panel that lives inside the popup.
fn picker_panel(data: &RefAny, color: ColorU) -> Dom {
    use azul_core::{
        a11y::{AccessibilityInfo, AccessibilityRole},
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{EventFilter, HoverEventFilter, IdOrClass::Class},
    };
    use crate::widgets::{label::Label, number_input::NumberInput, text_input::TextInput};

    let hsv = Hsv::from_color(color);
    let hex = color_to_hex(color);

    let drag_callbacks = |down: usize, over: usize, up: usize| {
        let mk = |event: EventFilter, cb: usize| CoreCallbackData {
            event,
            refany: data.clone(),
            callback: CoreCallback { cb, ctx: azul_core::refany::OptionRefAny::None },
        };
        // The press captures the pointer (see `capture_pointer`), so the
        // moves and the release reach this node wherever the cursor goes —
        // no `MouseLeave` ends the drag the moment the cursor slips off.
        vec![
            mk(EventFilter::Hover(HoverEventFilter::MouseDown), down),
            mk(EventFilter::Hover(HoverEventFilter::MouseOver), over),
            mk(EventFilter::Hover(HoverEventFilter::MouseUp), up),
        ]
    };

    // Saturation/value plane with its ring marker.
    let plane_marker = Dom::create_div().with_css(&format!(
            "position: absolute; left: {:.1}%; top: {:.1}%; width: 12px; height: 12px; \
             margin-left: -6px; margin-top: -6px; border: 2px solid #ffffff; \
             border-radius: 6px; box-shadow: 0px 0px 2px rgba(0, 0, 0, 0.6);",
            hsv.s * 100.0,
            (1.0 - hsv.v) * 100.0
        ),
    );
    // Children: [shade overlay, marker] — the marker stays on top.
    let plane = Dom::create_div()
        .with_ids_and_classes(vec![Class(COLOR_PICKER_PLANE_CLASS.into())].into())
        .with_css(&format!(
                "position: relative; width: {PLANE_WIDTH}px; height: {PLANE_HEIGHT}px; \
                 border-radius: 4px; cursor: crosshair; background: {};",
                plane_background_css(hsv.h)
            )
        )
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Slider,
            accessibility_name: Some("Saturation and brightness".into()).into(),
            accessibility_value: Some(AzString::from(sv_a11y_value(hsv))).into(),
            ..Default::default()
        })
        .with_callbacks(
            drag_callbacks(
                on_plane_down as usize,
                on_plane_move as usize,
                on_plane_up as usize,
            )
            .into(),
        )
        .with_child(Dom::create_div().with_css(SHADE_CSS))
        .with_child(plane_marker);

    // Hue bar with its marker.
    let hue_marker = Dom::create_div().with_css(&format!(
            "position: absolute; left: {:.1}%; top: 0px; width: 12px; height: 12px; \
             margin-left: -6px; border: 2px solid #ffffff; border-radius: 6px; \
             box-shadow: 0px 0px 2px rgba(0, 0, 0, 0.6);",
            hsv.h / 360.0 * 100.0
        ),
    );
    let hue = Dom::create_div()
        .with_ids_and_classes(vec![Class(COLOR_PICKER_HUE_CLASS.into())].into())
        .with_css(&format!(
                "position: relative; width: {PLANE_WIDTH}px; height: 12px; border-radius: 6px; \
                 cursor: pointer; background: {HUE_BACKGROUND_CSS};"
            )
        )
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Slider,
            accessibility_name: Some("Hue".into()).into(),
            accessibility_value: Some(AzString::from(format!("{}°", hsv.h.round()))).into(),
            ..Default::default()
        })
        .with_callbacks(
            drag_callbacks(on_hue_down as usize, on_hue_move as usize, on_hue_up as usize)
                .into(),
        )
        .with_child(hue_marker);

    // Alpha bar: checkerboard, then transparent→colour over it, then the marker.
    let opaque = ColorU { a: 255, ..color };
    let alpha_marker = Dom::create_div().with_css(&format!(
        "position: absolute; left: {:.1}%; top: 0px; width: 12px; height: 12px; \
         margin-left: -6px; border: 2px solid #ffffff; border-radius: 6px; \
         box-shadow: 0px 0px 2px rgba(0, 0, 0, 0.6);",
        f32::from(color.a) / 255.0 * 100.0
    ));
    let alpha_fill = Dom::create_div().with_css(&format!(
        "position: absolute; left: 0px; top: 0px; width: 100%; height: 100%; border-radius: 6px; \
         background: linear-gradient(to right, {}, {});",
        css_rgba(opaque, 0),
        css_rgba(opaque, 255)
    ));
    let mut alpha = Dom::create_div()
        .with_ids_and_classes(vec![Class(COLOR_PICKER_ALPHA_CLASS.into())].into())
        .with_css(&format!(
            "position: relative; width: {PLANE_WIDTH}px; height: 12px; border-radius: 6px; \
             cursor: pointer; overflow: hidden;"
        ))
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Slider,
            accessibility_name: Some("Opacity".into()).into(),
            accessibility_value: Some(AzString::from(alpha_a11y_value(color.a))).into(),
            ..Default::default()
        })
        .with_callbacks(
            drag_callbacks(on_alpha_down as usize, on_alpha_move as usize, on_alpha_up as usize)
                .into(),
        );
    alpha = alpha.with_child(checkerboard(PLANE_WIDTH, 12.0, 6.0));
    let alpha = alpha.with_child(alpha_fill).with_child(alpha_marker);

    // Preview + hex. A translucent colour shows the checkerboard through it.
    let mut preview = Dom::create_div()
        .with_css(
            "position: relative; width: 28px; height: 28px; border-radius: 4px; \
             border: 1px solid #c8c8c8; overflow: hidden;",
        )
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Graphic,
            accessibility_name: Some("Current colour".into()).into(),
            accessibility_value: Some(AzString::from(hex.clone())).into(),
            ..Default::default()
        });
    preview = preview.with_child(checkerboard(26.0, 26.0, 6.5));
    let preview = preview.with_child(Dom::create_div().with_css(&format!(
        "position: absolute; left: 0px; top: 0px; width: 100%; height: 100%; background: {};",
        css_rgba(opaque, color.a)
    )));
    let hex_input = TextInput::create()
        .with_text(hex.into())
        .with_accessibility_name("Hex colour")
        .with_container_style(field_container_style(96, true))
        .with_on_focus_lost(data.clone(), {
            let cb: crate::widgets::text_input::TextInputOnFocusLostCallbackType = on_hex_committed;
            cb
        })
        .dom();
    // The eyedropper: `pick_screen_color` runs the platform's sampler (the
    // system loupe on macOS; a screenshot in a fullscreen loupe elsewhere -
    // Wayland asks the user through the portal first). The answer comes
    // back as the window-level `ScreenColorPicked`, registered on this
    // very node so it reaches the picker's data.
    let eyedropper = Dom::create_div()
        .with_ids_and_classes(vec![Class(COLOR_PICKER_EYEDROPPER_CLASS.into())].into())
        .with_css(
            "display: flex; align-items: center; justify-content: center; width: 28px; \
             height: 28px; border: 1px solid #c8c8c8; border-radius: 4px; cursor: pointer; \
             background: #f4f4f4; color: #404040; font-size: 18px;",
        )
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::PushButton,
            accessibility_name: Some("Pick a colour from the screen".into()).into(),
            ..Default::default()
        })
        .with_callbacks(
            vec![
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    refany: data.clone(),
                    callback: CoreCallback { cb: on_eyedropper_clicked as usize, ctx: azul_core::refany::OptionRefAny::None },
                },
                CoreCallbackData {
                    event: EventFilter::Window(azul_core::events::WindowEventFilter::ScreenColorPicked),
                    refany: data.clone(),
                    callback: CoreCallback { cb: on_screen_color_picked as usize, ctx: azul_core::refany::OptionRefAny::None },
                },
            ]
            .into(),
        )
        .with_child(Dom::create_icon("colorize"));

    let preview_row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; gap: 8px;")
        .with_child(preview)
        .with_child(hex_input)
        .with_child(eyedropper);

    // R / G / B.
    let channel = |name: &str, short: &str, value: u8, cb: crate::widgets::number_input::NumberInputOnValueChangeCallbackType| {
        let field = NumberInput::create(f32::from(value))
            .with_accessibility_name(name)
            .with_container_style(field_container_style(44, false))
            .with_on_value_change(data.clone(), cb)
            .dom();
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; gap: 4px;")
            .with_child(Label::create(short.into()).dom())
            .with_child(field)
    };
    let rgb_row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; gap: 8px;")
        .with_child(channel("Red", "R", color.r, on_red_changed))
        .with_child(channel("Green", "G", color.g, on_green_changed))
        .with_child(channel("Blue", "B", color.b, on_blue_changed))
        .with_child(channel("Opacity", "A", color.a, on_alpha_changed));

    // The grip: a drag region (`-azul-app-region: drag`). In the popup the
    // engine runs the tear-off drag on it; in the torn-off palette, a drag
    // back over the swatch docks it.
    let grip = Dom::create_div()
        .with_ids_and_classes(vec![Class(COLOR_PICKER_GRIP_CLASS.into())].into())
        .with_css(
            "display: flex; flex-direction: row; justify-content: center; align-items: center; \
             height: 10px; margin-top: -4px; margin-bottom: -2px; cursor: grab; \
             -azul-app-region: drag;",
        )
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Separator,
            accessibility_name: Some("Drag to tear off".into()).into(),
            ..Default::default()
        })
        .with_child(Dom::create_div().with_css(
            "width: 36px; height: 4px; border-radius: 2px; background: #c8c8c8;",
        ));

    Dom::create_div()
        .with_ids_and_classes(vec![Class(COLOR_PICKER_CLASS.into())].into())
        .with_css(
            "display: flex; flex-direction: column; gap: 8px; padding: 8px; \
             background: #ffffff; border: 1px solid #c8c8c8; border-radius: 6px; \
             box-shadow: 0px 4px 16px rgba(0, 0, 0, 0.25); font-size: 12px; color: #202020;",
        )
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Dialog,
            accessibility_name: Some("Colour picker".into()).into(),
            ..Default::default()
        })
        .with_child(grip)
        .with_child(plane)
        .with_child(hue)
        .with_child(alpha)
        .with_child(preview_row)
        .with_child(rgb_row)
}

fn alpha_a11y_value(a: u8) -> String {
    format!("{}%", (f32::from(a) / 255.0 * 100.0).round())
}

fn sv_a11y_value(hsv: Hsv) -> String {
    format!(
        "saturation {}%, brightness {}%",
        (hsv.s * 100.0).round(),
        (hsv.v * 100.0).round()
    )
}

// ---------------------------------------------------------------------------
// Callbacks
// ---------------------------------------------------------------------------

/// The swatch was clicked: toggle the picker. The engine holds the popup
/// open across rebuilds (`set_transient_window_open`), closes it on an
/// outside click / Escape, and tells us through `Dismissed`.
extern "C" fn on_color_input_clicked(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut picker) = data.downcast_mut::<ColorPickerData>() else {
        return Update::DoNothing;
    };
    let swatch = info.get_hit_node();
    let Some(transient) = info.get_first_child(swatch) else {
        return Update::DoNothing;
    };
    picker.open = !picker.open;
    info.set_transient_window_open(transient, picker.open);
    Update::DoNothing
}

/// The engine closed the popup for the user (outside click, Escape).
extern "C" fn on_picker_dismissed(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut picker) = data.downcast_mut::<ColorPickerData>() {
        picker.open = false;
        picker.drag = Drag::None;
    }
    Update::DoNothing
}

/// Where the cursor is inside the hit node, as fractions of its size.
fn cursor_fraction(info: &CallbackInfo) -> Option<(f32, f32)> {
    let pos = info.get_cursor_relative_to_node().into_option()?;
    // No laid-out rect (a headless test, a node mid-mutation): the nominal
    // size is the right denominator, the same way the slider falls back.
    let rect = info.get_hit_node_rect();
    let w = rect.map(|r| r.size.width).filter(|w| *w > 0.0).unwrap_or(PLANE_WIDTH);
    let h = rect.map(|r| r.size.height).filter(|h| *h > 0.0).unwrap_or(PLANE_HEIGHT);
    Some(((pos.x / w).clamp(0.0, 1.0), (pos.y / h).clamp(0.0, 1.0)))
}

/// Push the picked colour to the popup's own controls (instant, no relayout)
/// and to the app. `panel` is the picker panel node (the parent of the three
/// bars; the grandparent of the eyedropper button).
fn publish(picker: &mut ColorPickerData, info: &mut CallbackInfo, panel: Option<DomNodeId>) -> Update {
    let hsv = picker.hsv;
    let color = picker.color();
    let hex = color_to_hex(color);

    // The panel holds [grip, plane, hue, alpha, preview_row, rgb_row].
    // Children: plane = [shade, marker]; hue = [marker]; alpha = [board?,
    // fill, marker] (the board is the last child of alpha-less builds, so
    // walk from the end).
    if let Some(panel) = panel {
        if let Some(plane) = info.get_first_child(panel).and_then(|grip| info.get_next_sibling(grip)) {
            if let Some(prop) = css_prop(CssPropertyType::BackgroundContent, &plane_background_css(hsv.h)) {
                info.set_css_property(plane, prop);
            }
            info.set_accessibility_value(plane, sv_a11y_value(hsv).into());
            if let Some(marker) = info.get_last_child(plane) {
                info.set_css_property(
                    marker,
                    CssProperty::const_left(LayoutLeft { inner: PixelValue::percent(hsv.s * 100.0) }),
                );
                info.set_css_property(
                    marker,
                    CssProperty::const_top(LayoutTop { inner: PixelValue::percent((1.0 - hsv.v) * 100.0) }),
                );
            }
            if let Some(hue) = info.get_next_sibling(plane) {
                info.set_accessibility_value(hue, format!("{}°", hsv.h.round()).into());
                if let Some(marker) = info.get_last_child(hue) {
                    info.set_css_property(
                        marker,
                        CssProperty::const_left(LayoutLeft { inner: PixelValue::percent(hsv.h / 360.0 * 100.0) }),
                    );
                }
                if let Some(alpha) = info.get_next_sibling(hue) {
                    info.set_accessibility_value(alpha, alpha_a11y_value(color.a).into());
                    let opaque = ColorU { a: 255, ..color };
                    if let Some(marker) = info.get_last_child(alpha) {
                        info.set_css_property(
                            marker,
                            CssProperty::const_left(LayoutLeft {
                                inner: PixelValue::percent(f32::from(color.a) / 255.0 * 100.0),
                            }),
                        );
                        if let Some(fill) = info.get_previous_sibling(marker) {
                            if let Some(prop) = css_prop(
                                CssPropertyType::BackgroundContent,
                                &format!(
                                    "linear-gradient(to right, {}, {})",
                                    css_rgba(opaque, 0),
                                    css_rgba(opaque, 255)
                                ),
                            ) {
                                info.set_css_property(fill, prop);
                            }
                        }
                    }
                    if let Some(preview) = info.get_next_sibling(alpha).and_then(|row| info.get_first_child(row)) {
                        info.set_accessibility_value(preview, hex.into());
                        if let Some(overlay) = info.get_last_child(preview) {
                            info.set_css_property(
                                overlay,
                                CssProperty::const_background_content(
                                    vec![StyleBackgroundContent::Color(color)].into(),
                                ),
                            );
                        }
                    }
                }
            }
        }
    }

    let inner = picker.state.inner;
    match picker.state.on_value_change.as_mut() {
        Some(ColorInputOnValueChange { callback, refany }) => (callback.cb)(refany.clone(), *info, inner),
        None => Update::DoNothing,
    }
}

fn apply_plane(picker: &mut ColorPickerData, info: &mut CallbackInfo) -> Update {
    let Some((x, y)) = cursor_fraction(info) else {
        return Update::DoNothing;
    };
    let hsv = Hsv { h: picker.hsv.h, s: x, v: 1.0 - y };
    picker.set_hsv(hsv);
    let panel = info.get_parent(info.get_hit_node());
    publish(picker, info, panel)
}

fn apply_hue(picker: &mut ColorPickerData, info: &mut CallbackInfo) -> Update {
    let Some((x, _)) = cursor_fraction(info) else {
        return Update::DoNothing;
    };
    let hsv = Hsv { h: (x * 360.0).min(359.9), s: picker.hsv.s, v: picker.hsv.v };
    picker.set_hsv(hsv);
    let panel = info.get_parent(info.get_hit_node());
    publish(picker, info, panel)
}

/// The eyedropper button: ask the platform to sample a screen pixel. The
/// answer arrives in `on_screen_color_picked`.
extern "C" fn on_eyedropper_clicked(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.pick_screen_color();
    Update::DoNothing
}

/// `ScreenColorPicked` (window-level) in the picker's window: adopt the
/// sampled colour - RGB from the screen, the alpha the user had set - and
/// publish it like any other change. A cancelled pick changes nothing.
extern "C" fn on_screen_color_picked(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let azul_css::props::basic::color::OptionColorU::Some(picked) = info.get_picked_screen_color() else {
        return Update::DoNothing;
    };
    let Some(mut picker) = data.downcast_mut::<ColorPickerData>() else {
        return Update::DoNothing;
    };
    let a = picker.color().a;
    picker.set_color(ColorU { a, ..picked });
    // The button sits in the preview row: the panel is two levels up.
    let panel = info.get_parent(info.get_hit_node()).and_then(|row| info.get_parent(row));
    publish(&mut picker, &mut info, panel)
}

extern "C" fn on_plane_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut picker) = data.downcast_mut::<ColorPickerData>() else {
        return Update::DoNothing;
    };
    picker.drag = Drag::Plane;
    info.capture_pointer(info.get_hit_node());
    apply_plane(&mut picker, &mut info)
}

extern "C" fn on_plane_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut picker) = data.downcast_mut::<ColorPickerData>() else {
        return Update::DoNothing;
    };
    if picker.drag != Drag::Plane {
        return Update::DoNothing;
    }
    apply_plane(&mut picker, &mut info)
}

extern "C" fn on_plane_up(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut picker) = data.downcast_mut::<ColorPickerData>() {
        if picker.drag == Drag::Plane {
            picker.drag = Drag::None;
        }
    }
    Update::DoNothing
}

extern "C" fn on_hue_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut picker) = data.downcast_mut::<ColorPickerData>() else {
        return Update::DoNothing;
    };
    picker.drag = Drag::Hue;
    info.capture_pointer(info.get_hit_node());
    apply_hue(&mut picker, &mut info)
}

extern "C" fn on_hue_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut picker) = data.downcast_mut::<ColorPickerData>() else {
        return Update::DoNothing;
    };
    if picker.drag != Drag::Hue {
        return Update::DoNothing;
    }
    apply_hue(&mut picker, &mut info)
}

extern "C" fn on_hue_up(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut picker) = data.downcast_mut::<ColorPickerData>() {
        if picker.drag == Drag::Hue {
            picker.drag = Drag::None;
        }
    }
    Update::DoNothing
}

fn apply_alpha(picker: &mut ColorPickerData, info: &mut CallbackInfo) -> Update {
    let Some((x, _)) = cursor_fraction(info) else {
        return Update::DoNothing;
    };
    let mut c = picker.color();
    c.a = channel_value(x * 255.0);
    picker.set_color(c);
    let panel = info.get_parent(info.get_hit_node());
    publish(picker, info, panel)
}

extern "C" fn on_alpha_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut picker) = data.downcast_mut::<ColorPickerData>() else {
        return Update::DoNothing;
    };
    picker.drag = Drag::Alpha;
    info.capture_pointer(info.get_hit_node());
    apply_alpha(&mut picker, &mut info)
}

extern "C" fn on_alpha_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut picker) = data.downcast_mut::<ColorPickerData>() else {
        return Update::DoNothing;
    };
    if picker.drag != Drag::Alpha {
        return Update::DoNothing;
    }
    apply_alpha(&mut picker, &mut info)
}

extern "C" fn on_alpha_up(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut picker) = data.downcast_mut::<ColorPickerData>() {
        if picker.drag == Drag::Alpha {
            picker.drag = Drag::None;
        }
    }
    Update::DoNothing
}

/// The hex field lost focus: adopt its text if it is a colour.
extern "C" fn on_hex_committed(
    mut data: RefAny,
    mut info: CallbackInfo,
    text: crate::widgets::text_input::TextInputState,
) -> Update {
    let Some(mut picker) = data.downcast_mut::<ColorPickerData>() else {
        return Update::DoNothing;
    };
    let Some(c) = color_from_hex(&text.get_text()) else {
        return Update::DoNothing;
    };
    if c == picker.color() {
        return Update::DoNothing;
    }
    picker.set_color(c);
    let inner = picker.state.inner;
    match picker.state.on_value_change.as_mut() {
        Some(ColorInputOnValueChange { callback, refany }) => (callback.cb)(refany.clone(), info, inner),
        None => Update::DoNothing,
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 0..=255 first
fn channel_value(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

fn on_channel_changed(data: &mut RefAny, info: CallbackInfo, value: f32, set: fn(&mut ColorU, u8)) -> Update {
    let Some(mut picker) = data.downcast_mut::<ColorPickerData>() else {
        return Update::DoNothing;
    };
    let mut c = picker.color();
    set(&mut c, channel_value(value));
    if c == picker.color() {
        return Update::DoNothing;
    }
    picker.set_color(c);
    let inner = picker.state.inner;
    match picker.state.on_value_change.as_mut() {
        Some(ColorInputOnValueChange { callback, refany }) => (callback.cb)(refany.clone(), info, inner),
        None => Update::DoNothing,
    }
}

extern "C" fn on_red_changed(
    mut data: RefAny,
    info: CallbackInfo,
    n: crate::widgets::number_input::NumberInputState,
) -> Update {
    on_channel_changed(&mut data, info, n.number, |c, v| c.r = v)
}

extern "C" fn on_green_changed(
    mut data: RefAny,
    info: CallbackInfo,
    n: crate::widgets::number_input::NumberInputState,
) -> Update {
    on_channel_changed(&mut data, info, n.number, |c, v| c.g = v)
}

extern "C" fn on_alpha_changed(
    mut data: RefAny,
    info: CallbackInfo,
    n: crate::widgets::number_input::NumberInputState,
) -> Update {
    on_channel_changed(&mut data, info, n.number, |c, v| c.a = v)
}

extern "C" fn on_blue_changed(
    mut data: RefAny,
    info: CallbackInfo,
    n: crate::widgets::number_input::NumberInputState,
) -> Update {
    on_channel_changed(&mut data, info, n.number, |c, v| c.b = v)
}

#[cfg(all(test, feature = "std"))]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use std::{
        collections::{hash_map::DefaultHasher, BTreeMap, HashMap},
        hash::{Hash, Hasher},
        mem::discriminant,
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, IdOrClass, NodeId, NodeType},
        geom::{LogicalPosition, LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::props::basic::{length::SizeMetric, pixel::PixelValue};
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    /// The swatch is a fixed 14x14 box — the entire geometry of the widget.
    const SIDE: f32 = 14.0;

    /// The widget's default title, as promised by `ColorInputStateWrapper::default`.
    const DEFAULT_TITLE: &str = "Pick color";

    /// The color a freshly-defaulted `ColorInputState` holds: **opaque white**, which is
    /// deliberately *not* `ColorU::default()` (that one is opaque black). A swatch that
    /// silently defaulted to black would be indistinguishable from a "real" black pick.
    const DEFAULT_COLOR: ColorU = ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    /// Adversarial `ColorU` inputs. `create`/`dom` must move all four channels through
    /// verbatim, so the set covers both alpha extremes, the two off-by-one alphas, and
    /// `{1,2,3,4}` — four distinct small values that catch any channel reordering (an
    /// r/b swap is invisible for greys and for anything symmetric).
    const SAMPLE_COLORS: [ColorU; 8] = [
        ColorU { r: 0, g: 0, b: 0, a: 0 },
        ColorU { r: 0, g: 0, b: 0, a: 255 },
        ColorU { r: 255, g: 255, b: 255, a: 255 },
        ColorU { r: 255, g: 255, b: 255, a: 0 },
        ColorU { r: 255, g: 0, b: 0, a: 1 },
        ColorU { r: 0, g: 255, b: 0, a: 254 },
        ColorU { r: 1, g: 2, b: 3, a: 4 },
        ColorU { r: 128, g: 64, b: 32, a: 16 },
    ];

    // ------------------------------------------------------------------
    // Style-vec / DOM probes
    // ------------------------------------------------------------------

    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    fn find<T>(v: &CssPropertyWithConditionsVec, f: impl Fn(&CssProperty) -> Option<T>) -> Option<T> {
        v.as_ref().iter().find_map(|p| f(&p.property))
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length. An `em` or
    /// `%` slipping into the swatch geometry would resolve against the parent font/box,
    /// so the "14px" swatch could render at any size at all.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "color-input geometry must be absolute px, got {:?}",
            pv.metric,
        );
        pv.number.get()
    }

    fn width_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        find(v, |p| match p {
            CssProperty::Width(w) => match w.get_property() {
                Some(LayoutWidth::Px(pv)) => Some(px(pv)),
                _ => None,
            },
            _ => None,
        })
    }

    fn height_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        find(v, |p| match p {
            CssProperty::Height(h) => match h.get_property() {
                Some(LayoutHeight::Px(pv)) => Some(px(pv)),
                _ => None,
            },
            _ => None,
        })
    }

    /// The `background-color` of a style vec (first background layer only).
    fn background_color(v: &CssPropertyWithConditionsVec) -> Option<ColorU> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::BackgroundContent(b) => match b.get_property()?.as_ref().first()? {
                StyleBackgroundContent::Color(c) => Some(*c),
                _ => None,
            },
            _ => None,
        })
    }

    fn classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                IdOrClass::Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    /// The properties of a rendered node's *inline* style, in declaration order.
    fn inline_properties(dom: &Dom) -> Vec<CssProperty> {
        dom.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// The `background-color` actually declared on the rendered node.
    fn dom_background(dom: &Dom) -> Option<ColorU> {
        inline_properties(dom).into_iter().find_map(|p| match p {
            CssProperty::BackgroundContent(b) => match b.get_property()?.as_ref().first()? {
                StyleBackgroundContent::Color(c) => Some(*c),
                _ => None,
            },
            _ => None,
        })
    }

    /// The exact property `dom()` is expected to append for `c`.
    fn expected_background(c: ColorU) -> CssProperty {
        CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(vec![
            StyleBackgroundContent::Color(c),
        ]))
    }

    fn hash_of<T: Hash>(t: &T) -> u64 {
        let mut h = DefaultHasher::new();
        t.hash(&mut h);
        h.finish()
    }

    // ------------------------------------------------------------------
    // Callback harness
    // ------------------------------------------------------------------

    /// A `DomNodeId` in the root DOM pointing at flattened node `idx`.
    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    /// A `DomNodeId` whose node component is `None` — the "no concrete node was hit" case.
    fn node_none() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::NONE,
        }
    }

    /// A `DomLayoutResult` carrying only a `styled_dom`. `on_color_input_clicked` never
    /// queries the layout at all, so no real layout (and no font) is needed.
    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(DisplayList::default()),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// Runs `f` with a `CallbackInfo` whose window holds `styled_dom` as the root DOM and
    /// whose hit node is `hit`. Returns `f`'s value plus every change the callback pushed
    /// onto the transaction log.
    fn with_info<R>(
        styled_dom: StyledDom,
        hit: DomNodeId,
        f: impl FnOnce(&mut CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        with_info_cursor(styled_dom, hit, OptionLogicalPosition::None, f)
    }

    fn with_info_cursor<R>(
        styled_dom: StyledDom,
        hit: DomNodeId,
        cursor: OptionLogicalPosition,
        f: impl FnOnce(&mut CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window
            .layout_results
            .insert(DomId::ROOT_ID, layout_result(styled_dom));

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

        let mut info = CallbackInfo::new(
            &ref_data,
            &changes,
            hit,
            cursor,
            OptionLogicalPosition::None,
        );

        let r = f(&mut info);
        let pushed = info.take_changes();
        (r, pushed)
    }

    /// Renders `color_input`, then hands back both the laid-out DOM *and* the very `RefAny`
    /// the widget registered on its own mouse-up callback. Driving the handler with these
    /// two is the real wiring — nothing is re-created by hand, so a mismatch between what
    /// `dom()` stores and what the handler expects cannot hide behind the fixture.
    fn laid_out(color_input: ColorInput) -> (StyledDom, RefAny) {
        let dom = color_input.dom();
        let state = dom.root.callbacks.as_ref()[0].refany.clone();
        (StyledDom::create_from_dom(dom), state)
    }

    /// One "mouse-up on `hit`" delivered to the widget's own registered handler.
    fn click(styled_dom: StyledDom, state: &RefAny, hit: DomNodeId) -> (Update, Vec<CallbackChange>) {
        with_info(styled_dom, hit, |info| {
            on_color_input_clicked(state.clone(), *info)
        })
    }

    fn state_color(state: &RefAny) -> ColorU {
        let mut state = state.clone();
        let picker = state
            .downcast_ref::<ColorPickerData>()
            .expect("the widget state changed type");
        picker.state.inner.color
    }

    /// Like `with_info`, with the cursor at `cursor` relative to the hit node.
    fn with_info_at<R>(
        styled_dom: StyledDom,
        hit: DomNodeId,
        cursor: LogicalPosition,
        f: impl FnOnce(&mut CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        with_info_cursor(styled_dom, hit, OptionLogicalPosition::Some(cursor), f)
    }

    /// A payload the value-change callback writes into. It arrives as the `data: RefAny`
    /// argument — a *shared* clone of what the test still holds — so the test can read back
    /// exactly what the widget passed, without any global state.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ColorLog {
        seen: Vec<ColorU>,
        payload: u32,
    }

    extern "C" fn record_value(
        mut data: RefAny,
        _info: CallbackInfo,
        state: ColorInputState,
    ) -> Update {
        if let Some(mut log) = data.downcast_mut::<ColorLog>() {
            log.seen.push(state.color);
        }
        Update::RefreshDom
    }

    extern "C" fn value_do_nothing(
        _data: RefAny,
        _info: CallbackInfo,
        _state: ColorInputState,
    ) -> Update {
        Update::DoNothing
    }

    extern "C" fn value_refresh_all(
        _data: RefAny,
        _info: CallbackInfo,
        _state: ColorInputState,
    ) -> Update {
        Update::RefreshDomAllWindows
    }

    /// A `Callback`-shaped (2-arg) function — the shape FFI bindings hand in, which the
    /// `From<Callback>` arm *transmutes* into the 3-arg color-input slot. Never called.
    extern "C" fn generic_shaped(_data: RefAny, _info: CallbackInfo) -> Update {
        Update::DoNothing
    }

    fn log_refany() -> RefAny {
        RefAny::new(ColorLog {
            seen: Vec::new(),
            payload: 0xDEAD_BEEF,
        })
    }

    fn read_log(probe: &RefAny) -> ColorLog {
        let mut probe = probe.clone();
        let log = probe
            .downcast_ref::<ColorLog>()
            .expect("the user payload changed type");
        log.clone()
    }

    // ==================================================================
    // ColorInput::create
    // ==================================================================

    #[test]
    fn create_stores_every_channel_verbatim() {
        // A channel swap (r/b) or a dropped alpha still type-checks and still renders
        // *a* color — only an asymmetric fixture catches it.
        for c in SAMPLE_COLORS {
            let w = ColorInput::create(c);
            assert_eq!(
                w.color_input_state.inner.color, c,
                "create({c:?}) did not store the color it was given",
            );
        }
    }

    #[test]
    fn create_installs_no_callback_and_the_default_title() {
        for c in SAMPLE_COLORS {
            let w = ColorInput::create(c);
            assert!(
                w.color_input_state.on_value_change.as_ref().is_none(),
                "create({c:?}) invented a value-change callback out of nowhere",
            );
            assert_eq!(
                w.color_input_state.title.as_str(),
                DEFAULT_TITLE,
                "create({c:?}) did not keep the default title",
            );
        }
    }

    #[test]
    fn create_is_pure_and_distinguishes_every_sample_color() {
        for c in SAMPLE_COLORS {
            assert_eq!(
                ColorInput::create(c),
                ColorInput::create(c),
                "create({c:?}) is not deterministic",
            );
        }
        for (i, a) in SAMPLE_COLORS.iter().enumerate() {
            for b in &SAMPLE_COLORS[i + 1..] {
                assert_ne!(
                    ColorInput::create(*a),
                    ColorInput::create(*b),
                    "the widgets for {a:?} and {b:?} are indistinguishable",
                );
            }
        }
    }

    #[test]
    fn create_treats_alpha_as_significant() {
        // `{255,0,0,0}` and `{255,0,0,255}` differ only in alpha: an invisible swatch and
        // an opaque red one. Comparing on rgb alone would fuse the two.
        let opaque = ColorU { r: 255, g: 0, b: 0, a: 255 };
        let clear = ColorU { r: 255, g: 0, b: 0, a: 0 };
        assert_ne!(
            ColorInput::create(opaque),
            ColorInput::create(clear),
            "a transparent swatch compares equal to an opaque one",
        );
    }

    #[test]
    fn create_geometry_is_absolute_14px_for_every_color() {
        // `px()` asserts SizeMetric::Px — an em/% here would scale with the parent.
        for c in SAMPLE_COLORS {
            let w = ColorInput::create(c);
            assert_eq!(width_px(&w.style), Some(SIDE), "{c:?}: wrong swatch width");
            assert_eq!(height_px(&w.style), Some(SIDE), "{c:?}: wrong swatch height");
        }
    }

    #[test]
    fn create_marks_the_swatch_as_clickable() {
        // Without `cursor: pointer` the swatch looks inert even though it is the node
        // that carries the mouse-up handler.
        let props = properties(&ColorInput::create(DEFAULT_COLOR).style);
        assert!(
            props.contains(&CssProperty::const_cursor(StyleCursor::Pointer)),
            "the color input does not present as clickable: {props:?}",
        );
    }

    #[test]
    fn create_is_a_non_growing_block() {
        // A swatch with flex-grow != 0 would stretch to fill its row and stop being a
        // 14px square, silently defeating the width/height declarations above.
        let props = properties(&ColorInput::create(DEFAULT_COLOR).style);
        assert!(
            props.contains(&CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
            "the swatch is allowed to flex-grow: {props:?}",
        );
        assert!(
            props.contains(&CssProperty::const_display(LayoutDisplay::Block)),
            "the swatch is not a block box: {props:?}",
        );
    }

    #[test]
    fn create_declares_no_property_twice() {
        // A duplicate declaration means the later one silently wins — a latent
        // "why is my override ignored" bug that never surfaces as an error.
        let props = properties(&ColorInput::create(DEFAULT_COLOR).style);
        let mut seen = Vec::new();
        for p in &props {
            let d = discriminant(p);
            assert!(!seen.contains(&d), "the base style declares {p:?} twice");
            seen.push(d);
        }
    }

    #[test]
    fn create_keeps_the_color_out_of_the_base_style() {
        // The color lives in the *state* and is only turned into a background by `dom()`.
        // A background baked into the shared const table would make every swatch on screen
        // render the same color (and `dom()` would then declare it twice).
        for c in SAMPLE_COLORS {
            assert_eq!(
                background_color(&ColorInput::create(c).style),
                None,
                "create({c:?}) leaked the color into the base style",
            );
        }
    }

    #[test]
    fn create_style_does_not_depend_on_the_color() {
        let reference = properties(&ColorInput::create(SAMPLE_COLORS[0]).style);
        for c in SAMPLE_COLORS {
            assert_eq!(
                properties(&ColorInput::create(c).style),
                reference,
                "create({c:?}) produced a different style than create({:?})",
                SAMPLE_COLORS[0],
            );
        }
    }

    // ==================================================================
    // Default state invariants
    // ==================================================================

    #[test]
    fn the_default_color_is_opaque_white_not_colorus_own_default() {
        // `ColorU::default()` is opaque *black*. If `ColorInputState` ever fell back to the
        // derived default, every un-set swatch would render black — and a user who really
        // picked black would be indistinguishable from one who picked nothing.
        assert_eq!(ColorInputState::default().color, DEFAULT_COLOR);
        assert_ne!(
            ColorInputState::default().color,
            ColorU::default(),
            "the color input's default silently became ColorU::default()",
        );
        assert_eq!(ColorInputStateWrapper::default().inner.color, DEFAULT_COLOR);
        assert_eq!(
            ColorInputStateWrapper::default().title.as_str(),
            DEFAULT_TITLE,
        );
        assert!(ColorInputStateWrapper::default()
            .on_value_change
            .as_ref()
            .is_none());
    }

    #[test]
    fn color_input_state_ord_and_partial_ord_agree() {
        // `ColorInputState` derives both. A hand-written impl drifting from the other would
        // make sorted containers of states behave inconsistently with `<`.
        for a in SAMPLE_COLORS {
            for b in SAMPLE_COLORS {
                let (x, y) = (ColorInputState { color: a }, ColorInputState { color: b });
                assert_eq!(
                    x.partial_cmp(&y),
                    Some(x.cmp(&y)),
                    "PartialOrd and Ord disagree for {a:?} vs {b:?}",
                );
                assert_eq!(
                    x == y,
                    x.cmp(&y) == core::cmp::Ordering::Equal,
                    "Eq and Ord disagree for {a:?} vs {b:?}",
                );
            }
        }
    }

    #[test]
    fn equal_color_input_states_hash_equal() {
        // The Hash/Eq contract: `a == b` must imply `hash(a) == hash(b)`, or a
        // `HashMap<ColorInputState, _>` loses entries.
        for c in SAMPLE_COLORS {
            let a = ColorInputState { color: c };
            let b = ColorInputState { color: c };
            assert_eq!(a, b);
            assert_eq!(hash_of(&a), hash_of(&b), "equal states hash differently ({c:?})");
        }
    }

    #[test]
    fn color_input_state_equality_is_channel_exact() {
        // One channel bumped by one must break equality — in all four channels.
        let base = ColorU { r: 10, g: 20, b: 30, a: 40 };
        let variants = [
            ColorU { r: 11, ..base },
            ColorU { g: 21, ..base },
            ColorU { b: 31, ..base },
            ColorU { a: 41, ..base },
        ];
        for v in variants {
            assert_ne!(
                ColorInputState { color: base },
                ColorInputState { color: v },
                "a one-channel difference ({base:?} vs {v:?}) was swallowed",
            );
        }
    }

    // ==================================================================
    // ColorInput::set_on_value_change / with_on_value_change
    // ==================================================================

    #[test]
    fn set_on_value_change_stores_the_function_pointer_and_the_payload_verbatim() {
        let mut w = ColorInput::create(DEFAULT_COLOR);
        w.set_on_value_change(
            RefAny::new(0xDEAD_BEEF_u32),
            value_do_nothing as ColorInputOnValueChangeCallbackType,
        );

        let t = w
            .color_input_state
            .on_value_change
            .as_ref()
            .expect("set_on_value_change did not store anything");
        assert_eq!(
            t.callback.cb as *const () as usize,
            value_do_nothing as ColorInputOnValueChangeCallbackType as *const () as usize,
            "the fn pointer was corrupted on the way in",
        );

        let mut data = t.refany.clone();
        assert_eq!(
            *data.downcast_ref::<u32>().expect("the payload changed type"),
            0xDEAD_BEEF,
            "the payload was corrupted",
        );
        assert!(
            data.downcast_ref::<u64>().is_none(),
            "downcasting to the wrong type must fail, not reinterpret the bytes",
        );
    }

    #[test]
    fn set_on_value_change_replaces_rather_than_accumulates() {
        // `OptionColorInputOnValueChange` is a single slot; setting twice must leave the
        // *second* callback installed (and must not leak or free the first one's RefAny).
        let first = log_refany();
        let mut w = ColorInput::create(DEFAULT_COLOR);
        w.set_on_value_change(
            first.clone(),
            value_do_nothing as ColorInputOnValueChangeCallbackType,
        );
        w.set_on_value_change(
            RefAny::new(1u8),
            value_refresh_all as ColorInputOnValueChangeCallbackType,
        );

        let t = w
            .color_input_state
            .on_value_change
            .as_ref()
            .expect("the callback vanished");
        assert_eq!(
            t.callback.cb as *const () as usize,
            value_refresh_all as ColorInputOnValueChangeCallbackType as *const () as usize,
            "the second set_on_value_change did not win",
        );
        // The displaced payload is still a valid, readable RefAny (not freed twice).
        assert_eq!(read_log(&first).payload, 0xDEAD_BEEF);
    }

    #[test]
    fn set_on_value_change_does_not_disturb_the_color_or_the_style() {
        for c in SAMPLE_COLORS {
            let pristine = ColorInput::create(c);
            let mut w = ColorInput::create(c);
            w.set_on_value_change(
                RefAny::new(0u8),
                value_do_nothing as ColorInputOnValueChangeCallbackType,
            );

            assert_eq!(
                w.color_input_state.inner.color, c,
                "installing a callback rewrote the color",
            );
            assert_eq!(
                properties(&w.style),
                properties(&pristine.style),
                "installing a callback rewrote the style",
            );
            assert_eq!(
                w.color_input_state.title.as_str(),
                pristine.color_input_state.title.as_str(),
                "installing a callback rewrote the title",
            );
        }
    }

    #[test]
    fn with_on_value_change_is_exactly_set_on_value_change_in_builder_form() {
        let by_builder = ColorInput::create(SAMPLE_COLORS[6]).with_on_value_change(
            RefAny::new(7u32),
            value_do_nothing as ColorInputOnValueChangeCallbackType,
        );

        let mut by_setter = ColorInput::create(SAMPLE_COLORS[6]);
        by_setter.set_on_value_change(
            RefAny::new(7u32),
            value_do_nothing as ColorInputOnValueChangeCallbackType,
        );

        assert_eq!(by_builder.color_input_state.inner, by_setter.color_input_state.inner);
        assert_eq!(properties(&by_builder.style), properties(&by_setter.style));

        let a = by_builder
            .color_input_state
            .on_value_change
            .as_ref()
            .expect("builder lost the callback");
        let b = by_setter
            .color_input_state
            .on_value_change
            .as_ref()
            .expect("setter lost the callback");
        assert_eq!(
            a.callback.cb as *const () as usize,
            b.callback.cb as *const () as usize,
        );

        let (mut a, mut b) = (a.refany.clone(), b.refany.clone());
        assert_eq!(
            *a.downcast_ref::<u32>().expect("builder payload changed type"),
            *b.downcast_ref::<u32>().expect("setter payload changed type"),
        );
    }

    #[test]
    fn with_on_value_change_accepts_a_generic_callback_without_mangling_the_pointer() {
        // The `From<Callback>` arm *transmutes* a 2-arg fn pointer into the 3-arg
        // color-input slot — this is the FFI (Python/C) path. The pointer must come out
        // bit-identical; a mangled one would be called as a wild jump on the first click.
        let generic = Callback {
            cb: generic_shaped,
            ctx: OptionRefAny::None,
        };
        let expected = generic_shaped as *const () as usize;

        let w = ColorInput::create(DEFAULT_COLOR).with_on_value_change(RefAny::new(0u8), generic);
        let t = w
            .color_input_state
            .on_value_change
            .as_ref()
            .expect("the generic callback was dropped");
        assert_eq!(
            t.callback.cb as *const () as usize,
            expected,
            "the Callback -> ColorInputOnValueChangeCallback transmute mangled the pointer",
        );
    }

    // ==================================================================
    // ColorInput::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_returns_the_old_widget_and_leaves_a_default_behind() {
        for c in SAMPLE_COLORS {
            let mut w = ColorInput::create(c);
            let old = w.swap_with_default();

            assert_eq!(old, ColorInput::create(c), "{c:?}: the old widget was not returned intact");
            assert_eq!(w, ColorInput::default(), "{c:?}: what was left behind is not a default widget");
        }
    }

    #[test]
    fn swap_with_default_leaves_an_unstyled_widget_behind() {
        // `ColorInput::default()` is *derived*, so its `style` is an empty vec — unlike
        // `create()`, which installs the 14x14 + cursor table. The two therefore differ
        // even though their state is identical. Documented here so a change in either
        // direction is loud rather than silent.
        assert_eq!(
            ColorInput::default().color_input_state,
            ColorInput::create(DEFAULT_COLOR).color_input_state,
            "default() and create(white) no longer agree on the state",
        );
        assert!(
            ColorInput::default().style.as_ref().is_empty(),
            "ColorInput::default() gained a style",
        );
        assert_ne!(
            ColorInput::default(),
            ColorInput::create(DEFAULT_COLOR),
            "default() and create(white) became interchangeable",
        );

        let mut w = ColorInput::create(SAMPLE_COLORS[6]);
        let _ = w.swap_with_default();
        assert_eq!(width_px(&w.style), None, "the swapped-in widget unexpectedly has a width");
        assert_eq!(height_px(&w.style), None, "the swapped-in widget unexpectedly has a height");
    }

    #[test]
    fn swap_with_default_moves_the_callback_out_rather_than_copying_or_dropping_it() {
        let probe = log_refany();
        let mut w = ColorInput::create(SAMPLE_COLORS[4]).with_on_value_change(
            probe.clone(),
            record_value as ColorInputOnValueChangeCallbackType,
        );

        let old = w.swap_with_default();

        // The callback (and its payload) left with the returned value ...
        let moved = old
            .color_input_state
            .on_value_change
            .as_ref()
            .expect("the value-change callback vanished during the swap");
        assert_eq!(
            moved.callback.cb as *const () as usize,
            record_value as ColorInputOnValueChangeCallbackType as *const () as usize,
            "the fn pointer was mangled by the swap",
        );

        // ... and did NOT stay behind: a duplicated callback would fire twice, and a
        // duplicated RefAny would double-free its payload.
        assert!(
            w.color_input_state.on_value_change.as_ref().is_none(),
            "the callback was copied instead of moved",
        );

        // The payload is still alive and unchanged after the move.
        assert_eq!(read_log(&probe).payload, 0xDEAD_BEEF);
    }

    #[test]
    fn swapping_twice_round_trips_the_original_widget() {
        let mut a = ColorInput::create(SAMPLE_COLORS[6]);
        let mut b = a.swap_with_default(); // a = default, b = the original
        let c = b.swap_with_default(); // b = default, c = the original

        assert_eq!(c, ColorInput::create(SAMPLE_COLORS[6]));
        assert_eq!(a, ColorInput::default());
        assert_eq!(b, ColorInput::default());
    }

    // ==================================================================
    // ColorInput::dom
    // ==================================================================

    #[test]
    fn dom_is_a_div_with_the_native_class_and_a_closed_popup_child() {
        for c in SAMPLE_COLORS {
            let dom = ColorInput::create(c).dom();
            assert!(
                matches!(dom.root.get_node_type(), NodeType::Div),
                "{c:?}: the color input is not a div",
            );
            assert_eq!(
                classes(&dom),
                vec!["__azul_native_color_input".to_string()],
                "{c:?}: wrong class list",
            );
            // The LAST child is the picker's transient window, CLOSED by
            // attribute (a click opens it through the engine), anchored below
            // the swatch, dismissed by an outside click. An opaque swatch has
            // nothing else; a translucent one carries a checkerboard and a
            // colour overlay before it.
            let kids = dom.children.as_ref();
            let expected = if c.a < 255 { 3 } else { 1 };
            assert_eq!(kids.len(), expected, "{c:?}: wrong children for the swatch");
            if c.a < 255 {
                assert_eq!(classes(&kids[0]), vec![CHECKERBOARD_CLASS.to_string()], "{c:?}: board first");
            }
            let NodeType::TransientWindow(cfg) = kids[expected - 1].root.get_node_type() else {
                panic!("{c:?}: the last child is not a <transient-window>");
            };
            assert!(!cfg.open, "{c:?}: the popup must start closed");
            assert_eq!(cfg.anchor, TransientAnchor::Bottom);
            assert_eq!(cfg.dismiss, TransientDismiss::Outside);
            // ...holding the panel.
            let panel = &kids[expected - 1].children.as_ref()[0];
            assert_eq!(classes(panel), vec![COLOR_PICKER_CLASS.to_string()]);
            assert_eq!(panel.children.as_ref().len(), 6, "grip, plane, hue, alpha, preview row, rgb row");
            assert_eq!(cfg.tearoff, azul_core::transient::TransientTearoff::Free, "{c:?}: the picker tears off");
            assert_eq!(
                classes(&panel.children.as_ref()[0]),
                vec![COLOR_PICKER_GRIP_CLASS.to_string()],
                "{c:?}: the grip comes first"
            );
        }
    }
    #[test]
    fn dom_appends_the_color_as_the_last_background_and_keeps_the_base_style() {
        // The round trip: the color goes in through `create` and must come back out of the
        // rendered node's background, byte-identical, with the base style untouched and the
        // background appended *after* it (so a user override earlier in the table can't win).
        for c in SAMPLE_COLORS {
            let base = properties(&ColorInput::create(c).style);
            let rendered = inline_properties(&ColorInput::create(c).dom());

            // The colour, plus — for a translucent swatch only — the three
            // props that let the checkerboard sit under it (relative
            // positioning, overflow hidden x/y).
            let extra = if c.a < 255 { 4 } else { 1 };
            assert_eq!(
                rendered.len(),
                base.len() + extra,
                "{c:?}: dom() added {} properties instead of {extra}",
                rendered.len() as i64 - base.len() as i64,
            );
            assert_eq!(&rendered[..base.len()], &base[..], "{c:?}: dom() rewrote the base style");
            assert_eq!(
                rendered[base.len()],
                expected_background(c),
                "{c:?}: the appended background is not this widget's color",
            );
        }
    }

    #[test]
    fn dom_round_trips_every_channel_of_every_sample_color() {
        for c in SAMPLE_COLORS {
            assert_eq!(
                dom_background(&ColorInput::create(c).dom()),
                Some(c),
                "create({c:?}).dom() does not paint {c:?}",
            );
        }
    }

    #[test]
    fn dom_declares_exactly_one_background_and_no_property_twice() {
        for c in SAMPLE_COLORS {
            let props = inline_properties(&ColorInput::create(c).dom());
            let backgrounds = props
                .iter()
                .filter(|p| matches!(p, CssProperty::BackgroundContent(_)))
                .count();
            assert_eq!(backgrounds, 1, "{c:?}: expected exactly one background declaration");

            let mut seen = Vec::new();
            for p in &props {
                let d = discriminant(p);
                assert!(!seen.contains(&d), "{c:?}: the rendered node declares {p:?} twice");
                seen.push(d);
            }
        }
    }

    #[test]
    fn dom_preserves_the_swatch_geometry() {
        // The geometry has to survive the const-slice -> owned-vec -> vec round trip that
        // `dom()` performs; losing it would leave a background-only, zero-sized node.
        for c in SAMPLE_COLORS {
            let rendered: CssPropertyWithConditionsVec = inline_properties(&ColorInput::create(c).dom())
                .into_iter()
                .map(CssPropertyWithConditions::simple)
                .collect();
            assert_eq!(width_px(&rendered), Some(SIDE), "{c:?}: the rendered swatch lost its width");
            assert_eq!(height_px(&rendered), Some(SIDE), "{c:?}: the rendered swatch lost its height");
        }
    }

    #[test]
    fn dom_registers_exactly_one_mouse_up_handler_and_it_is_the_widgets_own() {
        for c in SAMPLE_COLORS {
            let dom = ColorInput::create(c).dom();
            let callbacks = dom.root.callbacks.as_ref();

            assert_eq!(callbacks.len(), 1, "{c:?}: expected exactly one callback");
            assert_eq!(
                callbacks[0].event,
                EventFilter::Hover(HoverEventFilter::MouseUp),
                "{c:?}: the color input must fire on mouse-up",
            );
            assert_eq!(
                callbacks[0].callback.cb,
                on_color_input_clicked as usize,
                "{c:?}: the registered handler is not on_color_input_clicked",
            );
            assert_eq!(
                callbacks[0].callback.ctx,
                OptionRefAny::None,
                "{c:?}: a native handler must not carry an FFI context",
            );
        }
    }

    #[test]
    fn dom_hands_the_widget_state_to_the_handler_not_the_user_payload() {
        // `dom()` moves `color_input_state` (state + on_value_change + user RefAny) into the
        // persistent picker data the handler downcasts to. If it stored the *user's* payload
        // instead, every click would be a silent no-op.
        for c in SAMPLE_COLORS {
            let dom = ColorInput::create(c)
                .with_on_value_change(
                    RefAny::new(9u32),
                    value_do_nothing as ColorInputOnValueChangeCallbackType,
                )
                .dom();

            let mut state = dom.root.callbacks.as_ref()[0].refany.clone();
            let picker = state
                .downcast_ref::<ColorPickerData>()
                .expect("the handler's RefAny is not the picker data");

            assert_eq!(picker.state.inner.color, c, "the color was lost on the way into the DOM");
            assert_eq!(picker.state.title.as_str(), DEFAULT_TITLE, "the title was lost");
            assert!(
                picker.state.on_value_change.as_ref().is_some(),
                "the user's value-change callback was lost on the way into the DOM",
            );
            assert!(!picker.open, "a freshly built picker is closed");
            // The same allocation is the node's dataset, so it survives rebuilds.
            let mut ds = dom.root.get_dataset().cloned().expect("dataset");
            assert!(ds.downcast_ref::<ColorPickerData>().is_some());
        }
    }
    #[test]
    fn dom_of_a_callback_less_color_input_still_registers_the_click_handler() {
        // The handler must always be installed: it is what opens the picker.
        let dom = ColorInput::create(DEFAULT_COLOR).dom();
        assert_eq!(dom.root.callbacks.as_ref().len(), 1);

        let mut state = dom.root.callbacks.as_ref()[0].refany.clone();
        let picker = state
            .downcast_ref::<ColorPickerData>()
            .expect("wrong RefAny type");
        assert!(picker.state.on_value_change.as_ref().is_none());
    }
    #[test]
    fn dom_of_an_unstyled_default_widget_still_carries_its_background() {
        // `ColorInput::default()` has an empty style vec — pushing onto it must still work
        // and must produce exactly the one background property.
        let dom = ColorInput::default().dom();
        assert_eq!(
            inline_properties(&dom),
            vec![expected_background(DEFAULT_COLOR)],
            "a default color input did not render its background alone",
        );
    }

    #[test]
    fn the_rendered_dom_flattens_with_exactly_one_transient_window() {
        // `Dom::estimated_total_children` is a *cached* count; if it under-reports, the
        // flatten under-allocates its arenas. The picker panel is a real subtree.
        let styled = StyledDom::create_from_dom(ColorInput::create(SAMPLE_COLORS[6]).dom());
        let nodes = styled.node_data.as_ref();
        assert!(nodes.len() > 10, "the picker panel did not flatten: {} nodes", nodes.len());
        let transient = nodes
            .iter()
            .filter(|n| matches!(n.get_node_type(), NodeType::TransientWindow(_)))
            .count();
        assert_eq!(transient, 1, "exactly one popup per swatch");
        assert!(
            matches!(nodes[0].get_node_type(), NodeType::Div),
            "the swatch stays the root",
        );
    }
    // ==================================================================
    // on_color_input_clicked
    // ==================================================================

    #[test]
    fn clicking_toggles_the_picker_through_the_engine_and_never_touches_the_color() {
        for c in SAMPLE_COLORS {
            let (styled, state) = laid_out(ColorInput::create(c));
            // Open: the only thing a click does is ask the engine to hold the
            // popup (the swatch's first child) open.
            let (update, changes) = click(styled.clone(), &state, node(0));
            assert_eq!(update, Update::DoNothing, "{c:?}: a click asked for a redraw itself");
            assert_eq!(changes.len(), 1, "{c:?}: a click must push exactly one change");
            let popup_node = node(1);
            assert!(
                matches!(
                    &changes[0],
                    CallbackChange::SetTransientWindowOpen { node: n, open: true } if *n == popup_node
                ),
                "{c:?}: expected SetTransientWindowOpen(node 1, true), got {:?}",
                changes[0]
            );
            assert_eq!(state_color(&state), c, "{c:?}: the click changed the stored color");
            // Close: the second click asks for it closed.
            let (_, changes) = click(styled, &state, node(0));
            assert!(
                matches!(&changes[0], CallbackChange::SetTransientWindowOpen { open: false, .. }),
                "{c:?}: the second click must close"
            );
        }
    }
    #[test]
    fn clicking_with_a_refany_of_the_wrong_type_is_a_silent_no_op() {
        // The handler downcasts blind; a foreign RefAny must bail out, not reinterpret the
        // bytes as a ColorInputStateWrapper.
        let (styled, _) = laid_out(ColorInput::create(DEFAULT_COLOR));
        let foreign = RefAny::new(0xDEAD_BEEF_u32);

        let (update, changes) = click(styled, &foreign, node(0));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "the handler wrote to the DOM through a foreign RefAny");

        let mut foreign = foreign;
        assert_eq!(
            *foreign
                .downcast_ref::<u32>()
                .expect("the foreign payload was reinterpreted"),
            0xDEAD_BEEF,
            "the handler corrupted a RefAny it did not understand",
        );
    }

    #[test]
    fn clicking_never_fires_on_value_change() {
        // A click opens the picker; it is not a value change. The user callback must not
        // run, and the event loop sees the click's own (empty) verdict.
        let cases: [ColorInputOnValueChangeCallbackType; 3] =
            [value_do_nothing, record_value, value_refresh_all];
        for cb in cases {
            let probe = log_refany();
            let (styled, state) = laid_out(
                ColorInput::create(SAMPLE_COLORS[6]).with_on_value_change(probe.clone(), cb),
            );
            let (update, _) = click(styled, &state, node(0));
            assert_eq!(update, Update::DoNothing);
            assert!(read_log(&probe).seen.is_empty(), "a click must not report a value");
        }
    }

    /// Press on the plane at (x, y) as fractions of its size, through the
    /// widget's own handler.
    fn press_plane(styled: StyledDom, state: &RefAny, fx: f32, fy: f32) -> (Update, Vec<CallbackChange>) {
        // The panel is node 2 (swatch=0, transient=1, panel=2), the plane node 3.
        with_info_at(styled, node(3), LogicalPosition::new(fx * PLANE_WIDTH, fy * PLANE_HEIGHT), |info| {
            on_plane_down(state.clone(), *info)
        })
    }

    /// Press on the hue bar at `fx` of its width.
    fn press_hue(styled: StyledDom, state: &RefAny, fx: f32) -> (Update, Vec<CallbackChange>) {
        // The hue bar is the plane's next sibling: plane=3, marker=4, hue=5.
        with_info_at(styled, node(5), LogicalPosition::new(fx * PLANE_WIDTH, 6.0), |info| {
            on_hue_down(state.clone(), *info)
        })
    }

    #[test]
    fn pressing_the_plane_picks_saturation_and_brightness_and_reports_it() {
        // Start from pure red (hue 0): bottom-left of the plane is black, top-right is
        // the full hue, top-left is white.
        let probe = log_refany();
        let (styled, state) = laid_out(
            ColorInput::create(ColorU { r: 255, g: 0, b: 0, a: 255 })
                .with_on_value_change(probe.clone(), record_value as ColorInputOnValueChangeCallbackType),
        );
        let (update, changes) = press_plane(styled.clone(), &state, 1.0, 0.0);
        assert_eq!(update, Update::RefreshDom, "the user's verdict is forwarded");
        assert_eq!(state_color(&state), ColorU { r: 255, g: 0, b: 0, a: 255 });
        assert!(
            changes.iter().any(|c| matches!(c, CallbackChange::ChangeNodeCssProperties { .. })),
            "the marker/preview must move without a relayout; got {changes:?}"
        );

        let _ = press_plane(styled.clone(), &state, 0.0, 0.0);
        assert_eq!(state_color(&state), ColorU { r: 255, g: 255, b: 255, a: 255 }, "top-left is white");
        let _ = press_plane(styled.clone(), &state, 0.5, 1.0);
        assert_eq!(state_color(&state), ColorU { r: 0, g: 0, b: 0, a: 255 }, "the bottom edge is black");
        // Going through black must not forget the hue: half saturation, full value
        // afterwards is still a RED, not grey.
        let _ = press_plane(styled, &state, 0.5, 0.0);
        assert_eq!(state_color(&state), ColorU { r: 255, g: 128, b: 128, a: 255 });
        assert_eq!(read_log(&probe).seen.len(), 4, "every pick reported");
    }

    #[test]
    fn pressing_the_hue_bar_rotates_the_hue_and_keeps_saturation_and_brightness() {
        let (styled, state) = laid_out(ColorInput::create(ColorU { r: 255, g: 128, b: 128, a: 255 }));
        let _ = press_hue(styled.clone(), &state, 1.0 / 3.0); // 120° = green
        assert_eq!(state_color(&state), ColorU { r: 128, g: 255, b: 128, a: 255 });
        let _ = press_hue(styled.clone(), &state, 2.0 / 3.0); // 240° = blue
        assert_eq!(state_color(&state), ColorU { r: 128, g: 128, b: 255, a: 255 });
        let _ = press_hue(styled, &state, 0.0);
        assert_eq!(state_color(&state), ColorU { r: 255, g: 128, b: 128, a: 255 });
    }

    #[test]
    fn moving_without_a_press_does_nothing() {
        let (styled, state) = laid_out(ColorInput::create(SAMPLE_COLORS[6]));
        let (update, changes) = with_info_at(styled, node(3), LogicalPosition::new(10.0, 10.0), |info| {
            on_plane_move(state.clone(), *info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(state_color(&state), SAMPLE_COLORS[6]);
    }
    #[test]
    fn the_callback_sees_the_picked_color_not_the_default() {
        // The handler reads the picker's current colour and passes it on. Passing
        // `ColorInputState::default()` (opaque white) instead would type-check.
        let probe = log_refany();
        let (styled, state) = laid_out(
            ColorInput::create(ColorU { r: 0, g: 0, b: 255, a: 255 }).with_on_value_change(
                probe.clone(),
                record_value as ColorInputOnValueChangeCallbackType,
            ),
        );
        let (update, _) = press_plane(styled, &state, 1.0, 0.0);
        assert_eq!(update, Update::RefreshDom);
        assert_eq!(read_log(&probe).seen, vec![ColorU { r: 0, g: 0, b: 255, a: 255 }]);
    }
    #[test]
    fn the_callback_receives_the_user_payload_not_the_widget_state() {
        let probe = log_refany();
        let (styled, state) = laid_out(
            ColorInput::create(SAMPLE_COLORS[6]).with_on_value_change(
                probe.clone(),
                record_value as ColorInputOnValueChangeCallbackType,
            ),
        );
        press_plane(styled, &state, 0.25, 0.25);

        // It wrote into the ColorLog, so it got the user's payload ...
        assert_eq!(read_log(&probe).seen.len(), 1);
        assert_eq!(read_log(&probe).payload, 0xDEAD_BEEF);

        // ... and that payload is emphatically not the widget state.
        let mut probe = probe;
        assert!(
            probe.downcast_ref::<ColorPickerData>().is_none(),
            "the user payload and the widget state got confused",
        );
    }
    #[test]
    fn clicking_never_mutates_the_stored_color() {
        // The swatch click only toggles the popup; the colour changes through the picker.
        let probe = log_refany();
        let c = SAMPLE_COLORS[6];
        let (_, state) = laid_out(
            ColorInput::create(c).with_on_value_change(
                probe.clone(),
                record_value as ColorInputOnValueChangeCallbackType,
            ),
        );

        for i in 0..8 {
            let (styled, _) = laid_out(ColorInput::create(c));
            let _ = click(styled, &state, node(0));
            assert_eq!(state_color(&state), c, "click {i} altered the stored color");
        }
        assert!(read_log(&probe).seen.is_empty(), "a click reported a value");
    }
    #[test]
    fn clicking_a_stale_or_missing_hit_node_does_not_panic() {
        // Stale hit ids reach callbacks after a DOM mutation, and `node_none()` is the
        // "nothing concrete was hit" case. With no popup child to find, the click is a
        // no-op — never a panic, never a stray change.
        let c = SAMPLE_COLORS[4];
        for hit in [node(9_999), node(usize::MAX - 1), node_none()] {
            let (styled, state) = laid_out(ColorInput::create(c));
            let (update, changes) = click(styled, &state, hit);
            assert_eq!(update, Update::DoNothing, "{hit:?}: wrong verdict");
            assert!(changes.is_empty(), "{hit:?}: a change was pushed for a missing node");
            assert_eq!(state_color(&state), c);
        }
    }
    #[test]
    fn two_widgets_built_from_the_same_color_do_not_share_state() {
        // `dom()` allocates a fresh `RefAny` per widget. If two swatches aliased one state,
        // picking in one would report through the other's callback as well.
        let a_probe = log_refany();
        let b_probe = log_refany();
        let (a_styled, a_state) = laid_out(ColorInput::create(SAMPLE_COLORS[1]).with_on_value_change(
            a_probe.clone(),
            record_value as ColorInputOnValueChangeCallbackType,
        ));
        let (_b_styled, b_state) = laid_out(ColorInput::create(SAMPLE_COLORS[1]).with_on_value_change(
            b_probe.clone(),
            record_value as ColorInputOnValueChangeCallbackType,
        ));

        press_plane(a_styled, &a_state, 1.0, 0.0);

        assert_eq!(read_log(&a_probe).seen.len(), 1, "the picked widget did not report");
        assert!(read_log(&b_probe).seen.is_empty(), "the other widget reported too");
        assert_eq!(state_color(&b_state), SAMPLE_COLORS[1], "the other widget's colour moved");
    }

    // ==================================================================
    // Colour math, hex, persistence
    // ==================================================================

    #[test]
    fn hsv_round_trips_every_sample_color() {
        for c in SAMPLE_COLORS {
            let back = Hsv::from_color(c).to_color(c.a);
            assert_eq!(back, c, "{c:?} did not survive rgb→hsv→rgb");
        }
        assert_eq!(Hsv::from_color(ColorU { r: 0, g: 255, b: 0, a: 255 }).h, 120.0);
        assert_eq!(Hsv::from_color(ColorU { r: 0, g: 0, b: 255, a: 255 }).h, 240.0);
        let grey = Hsv::from_color(ColorU { r: 90, g: 90, b: 90, a: 255 });
        assert_eq!((grey.h, grey.s), (0.0, 0.0), "grey has no hue and no saturation");
    }

    #[test]
    fn hex_parses_both_lengths_and_rejects_junk() {
        assert_eq!(color_to_hex(ColorU { r: 255, g: 87, b: 51, a: 255 }), "#ff5733");
        assert_eq!(color_from_hex("#ff5733"), Some(ColorU { r: 255, g: 87, b: 51, a: 255 }));
        assert_eq!(color_from_hex("  FF5733 "), Some(ColorU { r: 255, g: 87, b: 51, a: 255 }));
        assert_eq!(color_from_hex("#f53"), Some(ColorU { r: 255, g: 85, b: 51, a: 255 }));
        assert_eq!(color_from_hex("#ff573"), None);
        assert_eq!(color_from_hex("#gg5733"), None);
        assert_eq!(color_from_hex(""), None);
    }

    #[test]
    fn committing_a_hex_value_adopts_it_and_reports_once() {
        use crate::widgets::text_input::TextInputState;
        let probe = log_refany();
        let (styled, state) = laid_out(
            ColorInput::create(SAMPLE_COLORS[6]).with_on_value_change(
                probe.clone(),
                record_value as ColorInputOnValueChangeCallbackType,
            ),
        );
        let text = TextInputState {
            text: "#00ff00".chars().map(|c| c as u32).collect::<Vec<_>>().into(),
            ..Default::default()
        };
        let (update, _) = with_info(styled.clone(), node(0), |info| {
            on_hex_committed(state.clone(), *info, text.clone())
        });
        assert_eq!(update, Update::RefreshDom);
        assert_eq!(state_color(&state), ColorU { r: 0, g: 255, b: 0, a: 255 });
        // Committing the SAME value again is not a change.
        let (update, _) = with_info(styled.clone(), node(0), |info| {
            on_hex_committed(state.clone(), *info, text.clone())
        });
        assert_eq!(update, Update::DoNothing);
        // Junk is ignored.
        let junk = TextInputState {
            text: "nope".chars().map(|c| c as u32).collect::<Vec<_>>().into(),
            ..Default::default()
        };
        let (update, _) = with_info(styled, node(0), |info| on_hex_committed(state.clone(), *info, junk));
        assert_eq!(update, Update::DoNothing);
        assert_eq!(read_log(&probe).seen, vec![ColorU { r: 0, g: 255, b: 0, a: 255 }]);
    }

    #[test]
    fn a_channel_field_changes_only_its_channel() {
        use crate::widgets::number_input::NumberInputState;
        let (styled, state) = laid_out(ColorInput::create(ColorU { r: 10, g: 20, b: 30, a: 255 }));
        let n = |v: f32| NumberInputState { number: v, ..Default::default() };
        let _ = with_info(styled.clone(), node(0), |info| on_red_changed(state.clone(), *info, n(200.0)));
        assert_eq!(state_color(&state), ColorU { r: 200, g: 20, b: 30, a: 255 });
        let _ = with_info(styled.clone(), node(0), |info| on_green_changed(state.clone(), *info, n(999.0)));
        assert_eq!(state_color(&state), ColorU { r: 200, g: 255, b: 30, a: 255 }, "clamped");
        let _ = with_info(styled, node(0), |info| on_blue_changed(state.clone(), *info, n(-5.0)));
        assert_eq!(state_color(&state), ColorU { r: 200, g: 255, b: 0, a: 255 }, "clamped");
    }

    #[test]
    fn the_merge_keeps_the_old_allocation_and_adopts_the_apps_color() {
        // The old data (open, mid-drag, a hue the user is on) survives a rebuild; the
        // colour and callback come from the new build, because the app owns the value.
        let old_dom = ColorInput::create(ColorU { r: 255, g: 0, b: 0, a: 255 }).dom();
        let old = old_dom.root.get_dataset().cloned().unwrap();
        {
            let mut o = old.clone();
            let mut d = o.downcast_mut::<ColorPickerData>().unwrap();
            d.open = true;
            d.drag = Drag::Hue;
            d.set_hsv(Hsv { h: 200.0, s: 0.0, v: 0.0 }); // black, but "on" hue 200
        }
        // The app rebuilds with the SAME colour (black): hue must be kept.
        let probe = log_refany();
        let new_dom = ColorInput::create(ColorU { r: 0, g: 0, b: 0, a: 255 })
            .with_on_value_change(probe, record_value as ColorInputOnValueChangeCallbackType)
            .dom();
        let new = new_dom.root.get_dataset().cloned().unwrap();
        let mut merged = merge_picker_data(new, old.clone());
        let d = merged.downcast_ref::<ColorPickerData>().unwrap();
        assert!(d.open && d.drag == Drag::Hue, "runtime state survived");
        assert_eq!(d.hsv.h, 200.0, "the hue the user is on survived a pass through black");
        assert!(d.state.on_value_change.as_ref().is_some(), "the new callback was adopted");
        drop(d);
        // The app rebuilds with a DIFFERENT colour: adopted, hue recomputed.
        let new_dom = ColorInput::create(ColorU { r: 0, g: 255, b: 0, a: 255 }).dom();
        let new = new_dom.root.get_dataset().cloned().unwrap();
        let mut merged = merge_picker_data(new, old);
        let d = merged.downcast_ref::<ColorPickerData>().unwrap();
        assert_eq!(d.state.inner.color, ColorU { r: 0, g: 255, b: 0, a: 255 });
        assert_eq!(d.hsv.h, 120.0);
        // A foreign old payload is not merged into.
        let new_dom = ColorInput::create(DEFAULT_COLOR).dom();
        let new = new_dom.root.get_dataset().cloned().unwrap();
        let mut out = merge_picker_data(new, RefAny::new(7u8));
        assert!(out.downcast_ref::<ColorPickerData>().is_some());
    }

    #[test]
    fn dismissal_closes_the_picker_state() {
        let (styled, state) = laid_out(ColorInput::create(SAMPLE_COLORS[2]));
        let _ = click(styled.clone(), &state, node(0));
        {
            let mut s = state.clone();
            assert!(s.downcast_ref::<ColorPickerData>().unwrap().open);
        }
        let _ = with_info(styled.clone(), node(1), |info| on_picker_dismissed(state.clone(), *info));
        {
            let mut s = state.clone();
            assert!(!s.downcast_ref::<ColorPickerData>().unwrap().open, "dismiss closed it");
        }
        // The next click OPENS again (not a stale "close").
        let (_, changes) = click(styled, &state, node(0));
        assert!(matches!(&changes[0], CallbackChange::SetTransientWindowOpen { open: true, .. }));
    }
}
