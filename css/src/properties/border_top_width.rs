//! `border-top-width` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{impl_pixel_value, PixelValue};
use crate::{impl_option}; // Assuming impl_option is a macro accessible from crate root

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBorderTopWidth {
    pub inner: PixelValue,
}

impl_pixel_value!(LayoutBorderTopWidth);

impl CssPropertyValue<LayoutBorderTopWidth> {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        if let CssPropertyValue::Exact(s) = self {
            s.inner.scale_for_dpi(scale_factor);
        }
    }
}

// It seems LayoutBorderTopWidthValue did not have its own impl_option in css_properties.rs
// It was just a type alias. We'll define it here.
// If an OptionLayoutBorderTopWidthValue is needed, it would be:
// impl_option!(LayoutBorderTopWidthValue, OptionLayoutBorderTopWidthValue, ...);
// For now, just the type alias as it was in the original file.
pub type LayoutBorderTopWidthValue = CssPropertyValue<LayoutBorderTopWidth>;

// If OptionLayoutBorderTopWidthValue is used elsewhere and needs an impl_option,
// it should be added here or where OptionLayoutBorderTopWidthValue is defined/used.
// For now, sticking to what was directly in css_properties.rs for the Value type.
impl_option!(
    LayoutBorderTopWidth,
    OptionLayoutBorderTopWidth, // This would be for Option<LayoutBorderTopWidth>, not Option<LayoutBorderTopWidthValue>
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

// Let's define OptionLayoutBorderTopWidthValue if it's a separate type that needs `impl_option`
// However, the original code has:
// pub type LayoutBorderTopWidthValue = CssPropertyValue<LayoutBorderTopWidth>;
// And then CssProperty::BorderTopWidth(LayoutBorderTopWidthValue)
// This implies LayoutBorderTopWidthValue itself is what might be an Option if CssPropertyValue can be an Option.
// Let's assume CssPropertyValue handles the optionality if needed, or the specific Option type is defined elsewhere.
// The task asks to move the "LayoutBorderTopWidthValue type alias and its impl_option! call".
// Since there was no direct impl_option!(LayoutBorderTopWidthValue, ...) in css_properties.rs,
// I will only include the type alias here.
// The impl_option for the base type LayoutBorderTopWidth itself might be what's intended if an Option version is used.
// Re-checking the original file structure:
// pub type LayoutBorderTopWidthValue = CssPropertyValue<LayoutBorderTopWidth>; (this is what we move)
// The impl_option! calls are usually for the XxxValue type itself if it's meant to be optional directly.
// Example: impl_option!(LayoutMarginRightValue, OptionLayoutMarginRightValue, ...);
// For border widths, such an `impl_option` for the `Value` type was NOT present.
// So, only the type alias is needed here.
// The `impl_option` for the raw struct `LayoutBorderTopWidth` is standard if that struct can be optional.
// I'll keep the `impl_option` for `LayoutBorderTopWidth` as it's good practice if the raw struct can be optional.
// And the type alias as requested.

// Corrected: The task is to move the `LayoutBorderTopWidthValue` type alias.
// If there was an `impl_option` for `LayoutBorderTopWidthValue` itself, it would be moved too.
// The `impl_option` for `LayoutBorderTopWidth` (the struct) is fine to keep here with the struct.
// The `css_properties.rs` file had:
// pub type LayoutBorderTopWidthValue = CssPropertyValue<LayoutBorderTopWidth>;
// (No impl_option for LayoutBorderTopWidthValue specifically)
// So, the type alias is correct.
// The impl_option for the struct `LayoutBorderTopWidth` (if it's intended to be used as `Option<LayoutBorderTopWidth>`)
// is separate from the `LayoutBorderTopWidthValue` alias.
// Let's ensure the `impl_option` for the struct itself is present, as that's good practice.
// And the type alias `LayoutBorderTopWidthValue` as requested.

// Final structure for this file:
// Struct, impl_pixel_value for struct, impl_option for struct, type alias for XxxValue.
// This seems consistent with other properties.
// The scale_for_dpi was on CssPropertyValue<LayoutBorderTopWidth>, so that's also included.
