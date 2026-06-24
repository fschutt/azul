//! Button widget with Bootstrap-inspired type-based styling (`ButtonType`).

use std::vec::Vec;

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, NodeType, TabIndex},
    refany::RefAny,
    resources::{ImageRef, OptionImageRef},
};
#[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{
            color::ColorU,
            font::{StyleFontFamily, StyleFontFamilyVec},
            *,
        },
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    system::SystemFontType,
    *,
};

use crate::callbacks::{Callback, CallbackInfo};

/// The semantic type/role of a button.
/// 
/// Each type has distinct styling to indicate its purpose to the user.
/// Colors are based on Bootstrap's button variants for familiarity.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ButtonType {
    /// Default button style - neutral/gray appearance
    #[default]
    Default,
    /// Primary action button - blue, uses system accent color on macOS
    Primary,
    /// Secondary button - gray, less prominent than primary
    Secondary,
    /// Success/confirmation button - green with white text
    Success,
    /// Danger/destructive button - red with white text
    Danger,
    /// Warning button - yellow with BLACK text
    Warning,
    /// Informational button - teal/cyan with white text
    Info,
    /// Link-style button - appears as a hyperlink, no background
    Link,
}

impl ButtonType {
    /// Get the CSS class name for this button type
    #[must_use] pub const fn class_name(&self) -> &'static str {
        match self {
            Self::Default => "__azul-btn-default",
            Self::Primary => "__azul-btn-primary",
            Self::Secondary => "__azul-btn-secondary",
            Self::Success => "__azul-btn-success",
            Self::Danger => "__azul-btn-danger",
            Self::Warning => "__azul-btn-warning",
            Self::Info => "__azul-btn-info",
            Self::Link => "__azul-btn-link",
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
pub struct Button {
    /// Content (image or text) of this button, centered by default
    pub label: AzString,
    /// Optional image that is displayed next to the label
    pub image: OptionImageRef,
    /// The semantic type of this button (Primary, Success, Danger, etc.)
    pub button_type: ButtonType,
    /// Style for this button container
    pub container_style: CssPropertyWithConditionsVec,
    /// Style of the label
    pub label_style: CssPropertyWithConditionsVec,
    /// Style of the image
    pub image_style: CssPropertyWithConditionsVec,
    /// Optional: Function to call when the button is clicked
    pub on_click: OptionButtonOnClick,
}

pub type ButtonOnClickCallbackType = extern "C" fn(RefAny, CallbackInfo) -> Update;
impl_widget_callback!(
    ButtonOnClick,
    OptionButtonOnClick,
    ButtonOnClickCallback,
    ButtonOnClickCallbackType
);

// Host-invoker plumbing for managed-FFI bindings — see core/src/host_invoker.rs.
azul_core::impl_managed_callback! {
    wrapper:        ButtonOnClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: BUTTON_ON_CLICK_INVOKER,
    invoker_ty:     AzButtonOnClickCallbackInvoker,
    thunk_fn:       az_button_on_click_callback_thunk,
    setter_fn:      AzApp_setButtonOnClickCallbackInvoker,
    from_handle_fn: AzButtonOnClickCallback_createFromHostHandle,
}

// ButtonType-specific styling
// ============================================================

/// Get the background color for a button type
const fn get_button_colors(button_type: ButtonType) -> (ColorU, ColorU, ColorU) {
    // Returns (normal, hover, active) colors
    match button_type {
        ButtonType::Default => (
            ColorU::rgb(248, 249, 250), // Light gray
            ColorU::rgb(233, 236, 239), // Darker gray on hover
            ColorU::rgb(218, 222, 226), // Even darker on active
        ),
        ButtonType::Primary => (
            ColorU::bootstrap_primary(),
            ColorU::bootstrap_primary_hover(),
            ColorU::bootstrap_primary_active(),
        ),
        ButtonType::Secondary => (
            ColorU::bootstrap_secondary(),
            ColorU::bootstrap_secondary_hover(),
            ColorU::bootstrap_secondary_active(),
        ),
        ButtonType::Success => (
            ColorU::bootstrap_success(),
            ColorU::bootstrap_success_hover(),
            ColorU::bootstrap_success_active(),
        ),
        ButtonType::Danger => (
            ColorU::bootstrap_danger(),
            ColorU::bootstrap_danger_hover(),
            ColorU::bootstrap_danger_active(),
        ),
        ButtonType::Warning => (
            ColorU::bootstrap_warning(),
            ColorU::bootstrap_warning_hover(),
            ColorU::bootstrap_warning_active(),
        ),
        ButtonType::Info => (
            ColorU::bootstrap_info(),
            ColorU::bootstrap_info_hover(),
            ColorU::bootstrap_info_active(),
        ),
        ButtonType::Link => (
            ColorU::TRANSPARENT,
            ColorU::TRANSPARENT,
            ColorU::TRANSPARENT,
        ),
    }
}

/// Get the text color for a button type
const fn get_button_text_color(button_type: ButtonType) -> ColorU {
    match button_type {
        ButtonType::Default => ColorU::rgb(33, 37, 41),   // Dark text
        ButtonType::Warning => ColorU::BLACK,             // Black text on yellow
        ButtonType::Link => ColorU::bootstrap_link(),     // Blue link color
        _ => ColorU::WHITE,                               // White text on colored buttons
    }
}

/// Build container style properties for a button type
fn build_button_container_style(button_type: ButtonType) -> Vec<CssPropertyWithConditions> {
    // ⚠ BISECTION PROBE (2026-06-02, REVERT): return a MINIMAL container style — no const
    // background, no hover/active gradients, no conditions — to test whether the
    // container's COMPLEX props cause the web cascade OOB on AzButton. If web-button-nocb
    // RUNS with this → the complex container props are the root; if it still OOBs → the
    // label/structure is. Remove this `return` to restore the real button styling.
    // ⚠ BISECTION step 7 (REVERT): InlineFlex → Block. The cascade + inline style now work
    // (rules=3, disp correct), but layout returns InvalidTree + width=0 because InlineFlex
    // triggers the (deferred) taffy flex-algorithm lift gap. Block layout is known-good on web.
    // If this lays out (no InvalidTree, sized button) → confirms flex is the layout blocker.
    return alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(6))),
        CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(6))),
    ];
    #[allow(unreachable_code)]
    let (bg_normal, bg_hover, bg_active) = get_button_colors(button_type);
    let text_color = get_button_text_color(button_type);
    
    // Focus outline uses system accent color
    let focus_outline_color = ColorU::bootstrap_primary();
    
    let mut props = Vec::with_capacity(40);
    
    // Basic layout - use InlineFlex so flex properties (justify-content, align-items) work
    props.push(CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineFlex)));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_justify_content(LayoutJustifyContent::Center)));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)));
    // Prevent stretching when inside a flex column container
    props.push(CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))));
    
    // Text color
    props.push(CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor { inner: text_color })));
    
    // Padding (Bootstrap-like)
    props.push(CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(6))));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(6))));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(12))));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_padding_right(LayoutPaddingRight::const_px(12))));
    
    // Border radius
    props.push(CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(StyleBorderTopLeftRadius::const_px(4))));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(StyleBorderTopRightRadius::const_px(4))));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(StyleBorderBottomLeftRadius::const_px(4))));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(StyleBorderBottomRightRadius::const_px(4))));
    
    if button_type == ButtonType::Link {
        // Link buttons have no background or border
        props.push(CssPropertyWithConditions::simple(CssProperty::const_background_content(
            StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(ColorU::TRANSPARENT)]),
        )));
        
        // Underline on hover - use TextDecoration::Underline variant
        props.push(CssPropertyWithConditions::on_hover(CssProperty::TextDecoration(StyleTextDecoration::Underline.into())));
    } else {
        // Normal background
        props.push(CssPropertyWithConditions::simple(CssProperty::const_background_content(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(bg_normal)]),
        )));
        
        // Border (subtle for Default, transparent for others to maintain size)
        let border_color = if button_type == ButtonType::Default {
            ColorU::rgb(206, 212, 218)
        } else {
            bg_normal
        };
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(1))));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(1))));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(1))));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle { inner: BorderStyle::Solid })));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle { inner: BorderStyle::Solid })));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid })));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor { inner: border_color })));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: border_color })));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: border_color })));
        props.push(CssPropertyWithConditions::simple(CssProperty::const_border_right_color(StyleBorderRightColor { inner: border_color })));
        
        // Hover state
        props.push(CssPropertyWithConditions::on_hover(CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(bg_hover)]).into(),
        )));
        if button_type == ButtonType::Default {
            let hover_border = ColorU::rgb(173, 181, 189);
            props.push(CssPropertyWithConditions::on_hover(CssProperty::BorderTopColor(StyleBorderTopColor { inner: hover_border }.into())));
            props.push(CssPropertyWithConditions::on_hover(CssProperty::BorderBottomColor(StyleBorderBottomColor { inner: hover_border }.into())));
            props.push(CssPropertyWithConditions::on_hover(CssProperty::BorderLeftColor(StyleBorderLeftColor { inner: hover_border }.into())));
            props.push(CssPropertyWithConditions::on_hover(CssProperty::BorderRightColor(StyleBorderRightColor { inner: hover_border }.into())));
        }
        
        // Active (pressed) state
        props.push(CssPropertyWithConditions::on_active(CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(bg_active)]).into(),
        )));
        
        // Focus state - uses accent color for outline
        // This makes the button feel "native" as it uses the system accent
        props.push(CssPropertyWithConditions::on_focus(CssProperty::BorderTopColor(StyleBorderTopColor { inner: focus_outline_color }.into())));
        props.push(CssPropertyWithConditions::on_focus(CssProperty::BorderBottomColor(StyleBorderBottomColor { inner: focus_outline_color }.into())));
        props.push(CssPropertyWithConditions::on_focus(CssProperty::BorderLeftColor(StyleBorderLeftColor { inner: focus_outline_color }.into())));
        props.push(CssPropertyWithConditions::on_focus(CssProperty::BorderRightColor(StyleBorderRightColor { inner: focus_outline_color }.into())));
    }
    
    props
}

/// Build label style properties
fn build_button_label_style() -> Vec<CssPropertyWithConditions> {
    // Use system UI font
    let font_family = StyleFontFamilyVec::from_vec(vec![
        StyleFontFamily::SystemType(SystemFontType::Ui),
    ]);
    
    vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
        CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(font_family)),
        CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    ]
}

impl Button {
    /// Create a button with `ButtonType::Default` styling.
    #[inline]
    #[must_use] pub fn create(label: AzString) -> Self {
        Self::with_type(label, ButtonType::Default)
    }
    
    /// Create a button with a specific type (Primary, Success, Danger, etc.)
    #[inline]
    #[must_use] pub fn with_type(label: AzString, button_type: ButtonType) -> Self {
        let container_style = build_button_container_style(button_type);
        let label_style = build_button_label_style();
        
        Self {
            label,
            image: None.into(),
            button_type,
            on_click: None.into(),
            container_style: CssPropertyWithConditionsVec::from_vec(container_style),
            label_style: CssPropertyWithConditionsVec::from_vec(label_style.clone()),
            image_style: CssPropertyWithConditionsVec::from_vec(label_style),
        }
    }
    
    /// Set the button type and update styling accordingly
    #[inline]
    pub fn set_button_type(&mut self, button_type: ButtonType) {
        self.button_type = button_type;
        self.container_style = CssPropertyWithConditionsVec::from_vec(build_button_container_style(button_type));
    }
    
    /// Builder method to set the button type
    #[inline]
    #[must_use] pub fn with_button_type(mut self, button_type: ButtonType) -> Self {
        self.set_button_type(button_type);
        self
    }

    #[inline]
    pub fn swap_with_default(&mut self) -> Self {
        let mut m = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut m, self);
        m
    }

    #[inline]
    pub fn set_image(&mut self, image: ImageRef) {
        self.image = Some(image).into();
    }

    #[inline]
    pub fn set_on_click<C: Into<ButtonOnClickCallback>>(&mut self, data: RefAny, on_click: C) {
        self.on_click = Some(ButtonOnClick {
            refany: data,
            callback: on_click.into(),
        })
        .into();
    }

    #[inline]
    pub fn with_on_click<C: Into<ButtonOnClickCallback>>(
        mut self,
        data: RefAny,
        on_click: C,
    ) -> Self {
        self.set_on_click(data, on_click);
        self
    }

    #[inline]
    #[must_use] pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{EventFilter, HoverEventFilter},
        };

        let callbacks = match self.on_click.into_option() {
            Some(ButtonOnClick {
                refany: data,
                callback,
            }) => vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                callback: CoreCallback {
                    cb: callback.cb as *const () as usize,
                    ctx: callback.ctx,
                },
                refany: data,
            }],
            None => Vec::new(),
        };

        // Add both the base class and the type-specific class
        // ⚠ BISECTION step 5 (REVERT): const-str classes → HEAP classes (AzString::from(&str)
        // = s.to_string().into()). Decisive test: if web-button-nocb RUNS now → the const-str
        // CLONE (s.clone() of a NoDestructor/borrowed AzString in set_ids_and_classes) mis-lifts
        // (deref of unmirrored .rodata); fix = transpiler const-str mirror OR heap classes here.
        // If it still OOBs → the AttributeTypeVec machinery (swap/into_library_owned_vec/retain/
        // push/set_attributes) is the lift bug, independent of const-str.
        let type_class = self.button_type.class_name();
        let classes: Vec<IdOrClass> = vec![
            Class(AzString::from("__azul-native-button")),
            Class(AzString::from(type_class)),
        ];

        // (2026-06-10: the June-02 bisection strips are REVERTED — the underlying corruption
        // was the alloc collect-machinery Leaf-stub in the web transpiler, fixed there. The
        // label keeps its inline css; the button carries its on_click callbacks + tab index
        // again — without them every Button click was a silent no-op on ALL backends, and the
        // web route-walk discovered 0 callbacks. The FIX-A ordering (container style before
        // ids/classes) is kept: builder-order is semantically neutral natively.)
        let label_dom = Dom::create_text(self.label)
            .with_css_props(self.label_style);

        let mut button = Dom::create_node(NodeType::Button);

        // If an image was set via `set_image`, render it as the first child
        // (left of the label, since the container is a horizontal flex row).
        if let Some(image) = self.image.into_option() {
            button = button.with_child(
                Dom::create_image(image).with_css_props(self.image_style),
            );
        }

        button
            .with_child(label_dom)
            .with_css_props(self.container_style)
            .with_ids_and_classes(IdOrClassVec::from_vec(classes))
            .with_callbacks(callbacks.into())
            .with_tab_index(TabIndex::Auto)
    }
}
