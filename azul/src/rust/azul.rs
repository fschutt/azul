//! Auto-generated public Rust API for the Azul GUI toolkit version 0.1.0
//!
// Copyright 2017 Maps4Print Einzelunternehmung
// 
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
// 
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
// 
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.


extern crate azul_dll;

extern crate libloading;

pub mod dll {

    use std::ffi::c_void;

    #[repr(C)] pub struct AzString { object: std::string::String, }
    #[repr(C)] pub struct AzU8Vec { object: std::vec::Vec::<u8>, }
    #[repr(C)] pub struct AzStringVec { object: std::vec::Vec::<String>, }
    #[repr(C)] pub struct AzPathBufPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzAppConfigPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzAppPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzCallbackInfoPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzIFrameCallbackInfoPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzIFrameCallbackReturnPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzGlCallbackInfoPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzGlCallbackReturnPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutInfoPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzCssPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzBoxShadowPreDisplayItemPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutAlignContentPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutAlignItemsPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutBottomPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutBoxSizingPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutDirectionPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutDisplayPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutFlexGrowPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutFlexShrinkPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutFloatPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutHeightPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutJustifyContentPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutLeftPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutMarginBottomPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutMarginLeftPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutMarginRightPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutMarginTopPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutMaxHeightPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutMaxWidthPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutMinHeightPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutMinWidthPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutPaddingBottomPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutPaddingLeftPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutPaddingRightPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutPaddingTopPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutPositionPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutRightPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutTopPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutWidthPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzLayoutWrapPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzOverflowPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBackgroundContentPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBackgroundPositionPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBackgroundRepeatPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBackgroundSizePtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderBottomColorPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderBottomLeftRadiusPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderBottomRightRadiusPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderBottomStylePtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderBottomWidthPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderLeftColorPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderLeftStylePtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderLeftWidthPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderRightColorPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderRightStylePtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderRightWidthPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderTopColorPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderTopLeftRadiusPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderTopRightRadiusPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderTopStylePtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleBorderTopWidthPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleCursorPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleFontFamilyPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleFontSizePtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleLetterSpacingPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleLineHeightPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleTabWidthPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleTextAlignmentHorzPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleTextColorPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzStyleWordSpacingPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzBoxShadowPreDisplayItemValue { object: azul_css::CssPropertyValue::<BoxShadowPreDisplayItem>, }
    #[repr(C)] pub struct AzLayoutAlignContentValue { object: azul_css::CssPropertyValue::<LayoutAlignContent>, }
    #[repr(C)] pub struct AzLayoutAlignItemsValue { object: azul_css::CssPropertyValue::<LayoutAlignItems>, }
    #[repr(C)] pub struct AzLayoutBottomValue { object: azul_css::CssPropertyValue::<LayoutBottom>, }
    #[repr(C)] pub struct AzLayoutBoxSizingValue { object: azul_css::CssPropertyValue::<LayoutBoxSizing>, }
    #[repr(C)] pub struct AzLayoutDirectionValue { object: azul_css::CssPropertyValue::<LayoutDirection>, }
    #[repr(C)] pub struct AzLayoutDisplayValue { object: azul_css::CssPropertyValue::<LayoutDisplay>, }
    #[repr(C)] pub struct AzLayoutFlexGrowValue { object: azul_css::CssPropertyValue::<LayoutFlexGrow>, }
    #[repr(C)] pub struct AzLayoutFlexShrinkValue { object: azul_css::CssPropertyValue::<LayoutFlexShrink>, }
    #[repr(C)] pub struct AzLayoutFloatValue { object: azul_css::CssPropertyValue::<LayoutFloat>, }
    #[repr(C)] pub struct AzLayoutHeightValue { object: azul_css::CssPropertyValue::<LayoutHeight>, }
    #[repr(C)] pub struct AzLayoutJustifyContentValue { object: azul_css::CssPropertyValue::<LayoutJustifyContent>, }
    #[repr(C)] pub struct AzLayoutLeftValue { object: azul_css::CssPropertyValue::<LayoutLeft>, }
    #[repr(C)] pub struct AzLayoutMarginBottomValue { object: azul_css::CssPropertyValue::<LayoutMarginBottom>, }
    #[repr(C)] pub struct AzLayoutMarginLeftValue { object: azul_css::CssPropertyValue::<LayoutMarginLeft>, }
    #[repr(C)] pub struct AzLayoutMarginRightValue { object: azul_css::CssPropertyValue::<LayoutMarginRight>, }
    #[repr(C)] pub struct AzLayoutMarginTopValue { object: azul_css::CssPropertyValue::<LayoutMarginTop>, }
    #[repr(C)] pub struct AzLayoutMaxHeightValue { object: azul_css::CssPropertyValue::<LayoutMaxHeight>, }
    #[repr(C)] pub struct AzLayoutMaxWidthValue { object: azul_css::CssPropertyValue::<LayoutMaxWidth>, }
    #[repr(C)] pub struct AzLayoutMinHeightValue { object: azul_css::CssPropertyValue::<LayoutMinHeight>, }
    #[repr(C)] pub struct AzLayoutMinWidthValue { object: azul_css::CssPropertyValue::<LayoutMinWidth>, }
    #[repr(C)] pub struct AzLayoutPaddingBottomValue { object: azul_css::CssPropertyValue::<LayoutPaddingBottom>, }
    #[repr(C)] pub struct AzLayoutPaddingLeftValue { object: azul_css::CssPropertyValue::<LayoutPaddingLeft>, }
    #[repr(C)] pub struct AzLayoutPaddingRightValue { object: azul_css::CssPropertyValue::<LayoutPaddingRight>, }
    #[repr(C)] pub struct AzLayoutPaddingTopValue { object: azul_css::CssPropertyValue::<LayoutPaddingTop>, }
    #[repr(C)] pub struct AzLayoutPositionValue { object: azul_css::CssPropertyValue::<LayoutPosition>, }
    #[repr(C)] pub struct AzLayoutRightValue { object: azul_css::CssPropertyValue::<LayoutRight>, }
    #[repr(C)] pub struct AzLayoutTopValue { object: azul_css::CssPropertyValue::<LayoutTop>, }
    #[repr(C)] pub struct AzLayoutWidthValue { object: azul_css::CssPropertyValue::<LayoutWidth>, }
    #[repr(C)] pub struct AzLayoutWrapValue { object: azul_css::CssPropertyValue::<LayoutWrap>, }
    #[repr(C)] pub struct AzOverflowValue { object: azul_css::CssPropertyValue::<Overflow>, }
    #[repr(C)] pub struct AzStyleBackgroundContentValue { object: azul_css::CssPropertyValue::<StyleBackgroundContent>, }
    #[repr(C)] pub struct AzStyleBackgroundPositionValue { object: azul_css::CssPropertyValue::<StyleBackgroundPosition>, }
    #[repr(C)] pub struct AzStyleBackgroundRepeatValue { object: azul_css::CssPropertyValue::<StyleBackgroundRepeat>, }
    #[repr(C)] pub struct AzStyleBackgroundSizeValue { object: azul_css::CssPropertyValue::<StyleBackgroundSize>, }
    #[repr(C)] pub struct AzStyleBorderBottomColorValue { object: azul_css::CssPropertyValue::<StyleBorderBottomColor>, }
    #[repr(C)] pub struct AzStyleBorderBottomLeftRadiusValue { object: azul_css::CssPropertyValue::<StyleBorderBottomLeftRadius>, }
    #[repr(C)] pub struct AzStyleBorderBottomRightRadiusValue { object: azul_css::CssPropertyValue::<StyleBorderBottomRightRadius>, }
    #[repr(C)] pub struct AzStyleBorderBottomStyleValue { object: azul_css::CssPropertyValue::<StyleBorderBottomStyle>, }
    #[repr(C)] pub struct AzStyleBorderBottomWidthValue { object: azul_css::CssPropertyValue::<StyleBorderBottomWidth>, }
    #[repr(C)] pub struct AzStyleBorderLeftColorValue { object: azul_css::CssPropertyValue::<StyleBorderLeftColor>, }
    #[repr(C)] pub struct AzStyleBorderLeftStyleValue { object: azul_css::CssPropertyValue::<StyleBorderLeftStyle>, }
    #[repr(C)] pub struct AzStyleBorderLeftWidthValue { object: azul_css::CssPropertyValue::<StyleBorderLeftWidth>, }
    #[repr(C)] pub struct AzStyleBorderRightColorValue { object: azul_css::CssPropertyValue::<StyleBorderRightColor>, }
    #[repr(C)] pub struct AzStyleBorderRightStyleValue { object: azul_css::CssPropertyValue::<StyleBorderRightStyle>, }
    #[repr(C)] pub struct AzStyleBorderRightWidthValue { object: azul_css::CssPropertyValue::<StyleBorderRightWidth>, }
    #[repr(C)] pub struct AzStyleBorderTopColorValue { object: azul_css::CssPropertyValue::<StyleBorderTopColor>, }
    #[repr(C)] pub struct AzStyleBorderTopLeftRadiusValue { object: azul_css::CssPropertyValue::<StyleBorderTopLeftRadius>, }
    #[repr(C)] pub struct AzStyleBorderTopRightRadiusValue { object: azul_css::CssPropertyValue::<StyleBorderTopRightRadius>, }
    #[repr(C)] pub struct AzStyleBorderTopStyleValue { object: azul_css::CssPropertyValue::<StyleBorderTopStyle>, }
    #[repr(C)] pub struct AzStyleBorderTopWidthValue { object: azul_css::CssPropertyValue::<StyleBorderTopWidth>, }
    #[repr(C)] pub struct AzStyleCursorValue { object: azul_css::CssPropertyValue::<StyleCursor>, }
    #[repr(C)] pub struct AzStyleFontFamilyValue { object: azul_css::CssPropertyValue::<StyleFontFamily>, }
    #[repr(C)] pub struct AzStyleFontSizeValue { object: azul_css::CssPropertyValue::<StyleFontSize>, }
    #[repr(C)] pub struct AzStyleLetterSpacingValue { object: azul_css::CssPropertyValue::<StyleLetterSpacing>, }
    #[repr(C)] pub struct AzStyleLineHeightValue { object: azul_css::CssPropertyValue::<StyleLineHeight>, }
    #[repr(C)] pub struct AzStyleTabWidthValue { object: azul_css::CssPropertyValue::<StyleTabWidth>, }
    #[repr(C)] pub struct AzStyleTextAlignmentHorzValue { object: azul_css::CssPropertyValue::<StyleTextAlignmentHorz>, }
    #[repr(C)] pub struct AzStyleTextColorValue { object: azul_css::CssPropertyValue::<StyleTextColor>, }
    #[repr(C)] pub struct AzStyleWordSpacingValue { object: azul_css::CssPropertyValue::<StyleWordSpacing>, }
    #[repr(C)] pub struct AzCssProperty { object: azul_css::CssProperty, }
    #[repr(C)] pub struct AzDomPtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzEventFilter { object: azul_core::dom::EventFilter, }
    #[repr(C)] pub struct AzHoverEventFilter { object: azul_core::dom::HoverEventFilter, }
    #[repr(C)] pub struct AzFocusEventFilter { object: azul_core::dom::FocusEventFilter, }
    #[repr(C)] pub struct AzNotEventFilter { object: azul_core::dom::NotEventFilter, }
    #[repr(C)] pub struct AzWindowEventFilter { object: azul_core::dom::WindowEventFilter, }
    #[repr(C)] pub struct AzTabIndex { object: azul_core::dom::TabIndex, }
    #[repr(C)] pub struct AzTextId { object: azul_core::app_resources::TextId, }
    #[repr(C)] pub struct AzImageId { object: azul_core::app_resources::ImageId, }
    #[repr(C)] pub struct AzFontId { object: azul_core::app_resources::FontId, }
    #[repr(C)] pub struct AzImageSource { object: azul_core::app_resources::ImageSource, }
    #[repr(C)] pub struct AzFontSource { object: azul_core::app_resources::FontSource, }
    #[repr(C)] pub struct AzRawImagePtr { ptr: *mut c_void, }
    #[repr(C)] pub struct AzRawImageFormat { object: azul_core::app_resources::RawImageFormat, }
    #[repr(C)] pub struct AzWindowCreateOptionsPtr { ptr: *mut c_void, }


    #[cfg(unix)]
    use libloading::os::unix::{Library, Symbol};
    #[cfg(windows)]
    use libloading::os::windows::{Library, Symbol};

    pub struct AzulDll {
        lib: Box<Library>,
        az_string_from_utf8_unchecked: Symbol<extern fn(_: usize) -> AzString>,
        az_string_from_utf8_lossy: Symbol<extern fn(_: usize) -> AzString>,
        az_string_into_bytes: Symbol<extern fn(_: AzString) -> AzU8Vec>,
        az_string_delete: Symbol<extern fn(_: &mut AzString)>,
        az_string_deep_copy: Symbol<extern fn(_: &AzString) -> AzString>,
        az_u8_vec_copy_from: Symbol<extern fn(_: usize) -> AzU8Vec>,
        az_u8_vec_as_ptr: Symbol<extern fn(_: &AzU8Vec) -> *const u8>,
        az_u8_vec_len: Symbol<extern fn(_: &AzU8Vec) -> usize>,
        az_u8_vec_delete: Symbol<extern fn(_: &mut AzU8Vec)>,
        az_u8_vec_deep_copy: Symbol<extern fn(_: &AzU8Vec) -> AzU8Vec>,
        az_string_vec_copy_from: Symbol<extern fn(_: usize) -> AzStringVec>,
        az_string_vec_delete: Symbol<extern fn(_: &mut AzStringVec)>,
        az_string_vec_deep_copy: Symbol<extern fn(_: &AzStringVec) -> AzStringVec>,
        az_path_buf_new: Symbol<extern fn(_: AzString) -> AzPathBufPtr>,
        az_path_buf_delete: Symbol<extern fn(_: &mut AzPathBufPtr)>,
        az_path_buf_shallow_copy: Symbol<extern fn(_: &AzPathBufPtr) -> AzPathBufPtr>,
        az_app_config_default: Symbol<extern fn() -> AzAppConfigPtr>,
        az_app_config_delete: Symbol<extern fn(_: &mut AzAppConfigPtr)>,
        az_app_config_shallow_copy: Symbol<extern fn(_: &AzAppConfigPtr) -> AzAppConfigPtr>,
        az_app_new: Symbol<extern fn(_: AzLayoutCallback) -> AzAppPtr>,
        az_app_run: Symbol<extern fn(_: AzWindowCreateOptionsPtr)>,
        az_app_delete: Symbol<extern fn(_: &mut AzAppPtr)>,
        az_app_shallow_copy: Symbol<extern fn(_: &AzAppPtr) -> AzAppPtr>,
        az_callback_info_delete: Symbol<extern fn(_: &mut AzCallbackInfoPtr)>,
        az_callback_info_shallow_copy: Symbol<extern fn(_: &AzCallbackInfoPtr) -> AzCallbackInfoPtr>,
        az_i_frame_callback_info_delete: Symbol<extern fn(_: &mut AzIFrameCallbackInfoPtr)>,
        az_i_frame_callback_info_shallow_copy: Symbol<extern fn(_: &AzIFrameCallbackInfoPtr) -> AzIFrameCallbackInfoPtr>,
        az_i_frame_callback_return_delete: Symbol<extern fn(_: &mut AzIFrameCallbackReturnPtr)>,
        az_i_frame_callback_return_shallow_copy: Symbol<extern fn(_: &AzIFrameCallbackReturnPtr) -> AzIFrameCallbackReturnPtr>,
        az_gl_callback_info_delete: Symbol<extern fn(_: &mut AzGlCallbackInfoPtr)>,
        az_gl_callback_info_shallow_copy: Symbol<extern fn(_: &AzGlCallbackInfoPtr) -> AzGlCallbackInfoPtr>,
        az_gl_callback_return_delete: Symbol<extern fn(_: &mut AzGlCallbackReturnPtr)>,
        az_gl_callback_return_shallow_copy: Symbol<extern fn(_: &AzGlCallbackReturnPtr) -> AzGlCallbackReturnPtr>,
        az_layout_info_delete: Symbol<extern fn(_: &mut AzLayoutInfoPtr)>,
        az_layout_info_shallow_copy: Symbol<extern fn(_: &AzLayoutInfoPtr) -> AzLayoutInfoPtr>,
        az_css_native: Symbol<extern fn() -> AzCssPtr>,
        az_css_empty: Symbol<extern fn() -> AzCssPtr>,
        az_css_delete: Symbol<extern fn(_: &mut AzCssPtr)>,
        az_css_shallow_copy: Symbol<extern fn(_: &AzCssPtr) -> AzCssPtr>,
        az_box_shadow_pre_display_item_delete: Symbol<extern fn(_: &mut AzBoxShadowPreDisplayItemPtr)>,
        az_box_shadow_pre_display_item_shallow_copy: Symbol<extern fn(_: &AzBoxShadowPreDisplayItemPtr) -> AzBoxShadowPreDisplayItemPtr>,
        az_layout_align_content_delete: Symbol<extern fn(_: &mut AzLayoutAlignContentPtr)>,
        az_layout_align_content_shallow_copy: Symbol<extern fn(_: &AzLayoutAlignContentPtr) -> AzLayoutAlignContentPtr>,
        az_layout_align_items_delete: Symbol<extern fn(_: &mut AzLayoutAlignItemsPtr)>,
        az_layout_align_items_shallow_copy: Symbol<extern fn(_: &AzLayoutAlignItemsPtr) -> AzLayoutAlignItemsPtr>,
        az_layout_bottom_delete: Symbol<extern fn(_: &mut AzLayoutBottomPtr)>,
        az_layout_bottom_shallow_copy: Symbol<extern fn(_: &AzLayoutBottomPtr) -> AzLayoutBottomPtr>,
        az_layout_box_sizing_delete: Symbol<extern fn(_: &mut AzLayoutBoxSizingPtr)>,
        az_layout_box_sizing_shallow_copy: Symbol<extern fn(_: &AzLayoutBoxSizingPtr) -> AzLayoutBoxSizingPtr>,
        az_layout_direction_delete: Symbol<extern fn(_: &mut AzLayoutDirectionPtr)>,
        az_layout_direction_shallow_copy: Symbol<extern fn(_: &AzLayoutDirectionPtr) -> AzLayoutDirectionPtr>,
        az_layout_display_delete: Symbol<extern fn(_: &mut AzLayoutDisplayPtr)>,
        az_layout_display_shallow_copy: Symbol<extern fn(_: &AzLayoutDisplayPtr) -> AzLayoutDisplayPtr>,
        az_layout_flex_grow_delete: Symbol<extern fn(_: &mut AzLayoutFlexGrowPtr)>,
        az_layout_flex_grow_shallow_copy: Symbol<extern fn(_: &AzLayoutFlexGrowPtr) -> AzLayoutFlexGrowPtr>,
        az_layout_flex_shrink_delete: Symbol<extern fn(_: &mut AzLayoutFlexShrinkPtr)>,
        az_layout_flex_shrink_shallow_copy: Symbol<extern fn(_: &AzLayoutFlexShrinkPtr) -> AzLayoutFlexShrinkPtr>,
        az_layout_float_delete: Symbol<extern fn(_: &mut AzLayoutFloatPtr)>,
        az_layout_float_shallow_copy: Symbol<extern fn(_: &AzLayoutFloatPtr) -> AzLayoutFloatPtr>,
        az_layout_height_delete: Symbol<extern fn(_: &mut AzLayoutHeightPtr)>,
        az_layout_height_shallow_copy: Symbol<extern fn(_: &AzLayoutHeightPtr) -> AzLayoutHeightPtr>,
        az_layout_justify_content_delete: Symbol<extern fn(_: &mut AzLayoutJustifyContentPtr)>,
        az_layout_justify_content_shallow_copy: Symbol<extern fn(_: &AzLayoutJustifyContentPtr) -> AzLayoutJustifyContentPtr>,
        az_layout_left_delete: Symbol<extern fn(_: &mut AzLayoutLeftPtr)>,
        az_layout_left_shallow_copy: Symbol<extern fn(_: &AzLayoutLeftPtr) -> AzLayoutLeftPtr>,
        az_layout_margin_bottom_delete: Symbol<extern fn(_: &mut AzLayoutMarginBottomPtr)>,
        az_layout_margin_bottom_shallow_copy: Symbol<extern fn(_: &AzLayoutMarginBottomPtr) -> AzLayoutMarginBottomPtr>,
        az_layout_margin_left_delete: Symbol<extern fn(_: &mut AzLayoutMarginLeftPtr)>,
        az_layout_margin_left_shallow_copy: Symbol<extern fn(_: &AzLayoutMarginLeftPtr) -> AzLayoutMarginLeftPtr>,
        az_layout_margin_right_delete: Symbol<extern fn(_: &mut AzLayoutMarginRightPtr)>,
        az_layout_margin_right_shallow_copy: Symbol<extern fn(_: &AzLayoutMarginRightPtr) -> AzLayoutMarginRightPtr>,
        az_layout_margin_top_delete: Symbol<extern fn(_: &mut AzLayoutMarginTopPtr)>,
        az_layout_margin_top_shallow_copy: Symbol<extern fn(_: &AzLayoutMarginTopPtr) -> AzLayoutMarginTopPtr>,
        az_layout_max_height_delete: Symbol<extern fn(_: &mut AzLayoutMaxHeightPtr)>,
        az_layout_max_height_shallow_copy: Symbol<extern fn(_: &AzLayoutMaxHeightPtr) -> AzLayoutMaxHeightPtr>,
        az_layout_max_width_delete: Symbol<extern fn(_: &mut AzLayoutMaxWidthPtr)>,
        az_layout_max_width_shallow_copy: Symbol<extern fn(_: &AzLayoutMaxWidthPtr) -> AzLayoutMaxWidthPtr>,
        az_layout_min_height_delete: Symbol<extern fn(_: &mut AzLayoutMinHeightPtr)>,
        az_layout_min_height_shallow_copy: Symbol<extern fn(_: &AzLayoutMinHeightPtr) -> AzLayoutMinHeightPtr>,
        az_layout_min_width_delete: Symbol<extern fn(_: &mut AzLayoutMinWidthPtr)>,
        az_layout_min_width_shallow_copy: Symbol<extern fn(_: &AzLayoutMinWidthPtr) -> AzLayoutMinWidthPtr>,
        az_layout_padding_bottom_delete: Symbol<extern fn(_: &mut AzLayoutPaddingBottomPtr)>,
        az_layout_padding_bottom_shallow_copy: Symbol<extern fn(_: &AzLayoutPaddingBottomPtr) -> AzLayoutPaddingBottomPtr>,
        az_layout_padding_left_delete: Symbol<extern fn(_: &mut AzLayoutPaddingLeftPtr)>,
        az_layout_padding_left_shallow_copy: Symbol<extern fn(_: &AzLayoutPaddingLeftPtr) -> AzLayoutPaddingLeftPtr>,
        az_layout_padding_right_delete: Symbol<extern fn(_: &mut AzLayoutPaddingRightPtr)>,
        az_layout_padding_right_shallow_copy: Symbol<extern fn(_: &AzLayoutPaddingRightPtr) -> AzLayoutPaddingRightPtr>,
        az_layout_padding_top_delete: Symbol<extern fn(_: &mut AzLayoutPaddingTopPtr)>,
        az_layout_padding_top_shallow_copy: Symbol<extern fn(_: &AzLayoutPaddingTopPtr) -> AzLayoutPaddingTopPtr>,
        az_layout_position_delete: Symbol<extern fn(_: &mut AzLayoutPositionPtr)>,
        az_layout_position_shallow_copy: Symbol<extern fn(_: &AzLayoutPositionPtr) -> AzLayoutPositionPtr>,
        az_layout_right_delete: Symbol<extern fn(_: &mut AzLayoutRightPtr)>,
        az_layout_right_shallow_copy: Symbol<extern fn(_: &AzLayoutRightPtr) -> AzLayoutRightPtr>,
        az_layout_top_delete: Symbol<extern fn(_: &mut AzLayoutTopPtr)>,
        az_layout_top_shallow_copy: Symbol<extern fn(_: &AzLayoutTopPtr) -> AzLayoutTopPtr>,
        az_layout_width_delete: Symbol<extern fn(_: &mut AzLayoutWidthPtr)>,
        az_layout_width_shallow_copy: Symbol<extern fn(_: &AzLayoutWidthPtr) -> AzLayoutWidthPtr>,
        az_layout_wrap_delete: Symbol<extern fn(_: &mut AzLayoutWrapPtr)>,
        az_layout_wrap_shallow_copy: Symbol<extern fn(_: &AzLayoutWrapPtr) -> AzLayoutWrapPtr>,
        az_overflow_delete: Symbol<extern fn(_: &mut AzOverflowPtr)>,
        az_overflow_shallow_copy: Symbol<extern fn(_: &AzOverflowPtr) -> AzOverflowPtr>,
        az_style_background_content_delete: Symbol<extern fn(_: &mut AzStyleBackgroundContentPtr)>,
        az_style_background_content_shallow_copy: Symbol<extern fn(_: &AzStyleBackgroundContentPtr) -> AzStyleBackgroundContentPtr>,
        az_style_background_position_delete: Symbol<extern fn(_: &mut AzStyleBackgroundPositionPtr)>,
        az_style_background_position_shallow_copy: Symbol<extern fn(_: &AzStyleBackgroundPositionPtr) -> AzStyleBackgroundPositionPtr>,
        az_style_background_repeat_delete: Symbol<extern fn(_: &mut AzStyleBackgroundRepeatPtr)>,
        az_style_background_repeat_shallow_copy: Symbol<extern fn(_: &AzStyleBackgroundRepeatPtr) -> AzStyleBackgroundRepeatPtr>,
        az_style_background_size_delete: Symbol<extern fn(_: &mut AzStyleBackgroundSizePtr)>,
        az_style_background_size_shallow_copy: Symbol<extern fn(_: &AzStyleBackgroundSizePtr) -> AzStyleBackgroundSizePtr>,
        az_style_border_bottom_color_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomColorPtr)>,
        az_style_border_bottom_color_shallow_copy: Symbol<extern fn(_: &AzStyleBorderBottomColorPtr) -> AzStyleBorderBottomColorPtr>,
        az_style_border_bottom_left_radius_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomLeftRadiusPtr)>,
        az_style_border_bottom_left_radius_shallow_copy: Symbol<extern fn(_: &AzStyleBorderBottomLeftRadiusPtr) -> AzStyleBorderBottomLeftRadiusPtr>,
        az_style_border_bottom_right_radius_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomRightRadiusPtr)>,
        az_style_border_bottom_right_radius_shallow_copy: Symbol<extern fn(_: &AzStyleBorderBottomRightRadiusPtr) -> AzStyleBorderBottomRightRadiusPtr>,
        az_style_border_bottom_style_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomStylePtr)>,
        az_style_border_bottom_style_shallow_copy: Symbol<extern fn(_: &AzStyleBorderBottomStylePtr) -> AzStyleBorderBottomStylePtr>,
        az_style_border_bottom_width_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomWidthPtr)>,
        az_style_border_bottom_width_shallow_copy: Symbol<extern fn(_: &AzStyleBorderBottomWidthPtr) -> AzStyleBorderBottomWidthPtr>,
        az_style_border_left_color_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftColorPtr)>,
        az_style_border_left_color_shallow_copy: Symbol<extern fn(_: &AzStyleBorderLeftColorPtr) -> AzStyleBorderLeftColorPtr>,
        az_style_border_left_style_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftStylePtr)>,
        az_style_border_left_style_shallow_copy: Symbol<extern fn(_: &AzStyleBorderLeftStylePtr) -> AzStyleBorderLeftStylePtr>,
        az_style_border_left_width_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftWidthPtr)>,
        az_style_border_left_width_shallow_copy: Symbol<extern fn(_: &AzStyleBorderLeftWidthPtr) -> AzStyleBorderLeftWidthPtr>,
        az_style_border_right_color_delete: Symbol<extern fn(_: &mut AzStyleBorderRightColorPtr)>,
        az_style_border_right_color_shallow_copy: Symbol<extern fn(_: &AzStyleBorderRightColorPtr) -> AzStyleBorderRightColorPtr>,
        az_style_border_right_style_delete: Symbol<extern fn(_: &mut AzStyleBorderRightStylePtr)>,
        az_style_border_right_style_shallow_copy: Symbol<extern fn(_: &AzStyleBorderRightStylePtr) -> AzStyleBorderRightStylePtr>,
        az_style_border_right_width_delete: Symbol<extern fn(_: &mut AzStyleBorderRightWidthPtr)>,
        az_style_border_right_width_shallow_copy: Symbol<extern fn(_: &AzStyleBorderRightWidthPtr) -> AzStyleBorderRightWidthPtr>,
        az_style_border_top_color_delete: Symbol<extern fn(_: &mut AzStyleBorderTopColorPtr)>,
        az_style_border_top_color_shallow_copy: Symbol<extern fn(_: &AzStyleBorderTopColorPtr) -> AzStyleBorderTopColorPtr>,
        az_style_border_top_left_radius_delete: Symbol<extern fn(_: &mut AzStyleBorderTopLeftRadiusPtr)>,
        az_style_border_top_left_radius_shallow_copy: Symbol<extern fn(_: &AzStyleBorderTopLeftRadiusPtr) -> AzStyleBorderTopLeftRadiusPtr>,
        az_style_border_top_right_radius_delete: Symbol<extern fn(_: &mut AzStyleBorderTopRightRadiusPtr)>,
        az_style_border_top_right_radius_shallow_copy: Symbol<extern fn(_: &AzStyleBorderTopRightRadiusPtr) -> AzStyleBorderTopRightRadiusPtr>,
        az_style_border_top_style_delete: Symbol<extern fn(_: &mut AzStyleBorderTopStylePtr)>,
        az_style_border_top_style_shallow_copy: Symbol<extern fn(_: &AzStyleBorderTopStylePtr) -> AzStyleBorderTopStylePtr>,
        az_style_border_top_width_delete: Symbol<extern fn(_: &mut AzStyleBorderTopWidthPtr)>,
        az_style_border_top_width_shallow_copy: Symbol<extern fn(_: &AzStyleBorderTopWidthPtr) -> AzStyleBorderTopWidthPtr>,
        az_style_cursor_delete: Symbol<extern fn(_: &mut AzStyleCursorPtr)>,
        az_style_cursor_shallow_copy: Symbol<extern fn(_: &AzStyleCursorPtr) -> AzStyleCursorPtr>,
        az_style_font_family_delete: Symbol<extern fn(_: &mut AzStyleFontFamilyPtr)>,
        az_style_font_family_shallow_copy: Symbol<extern fn(_: &AzStyleFontFamilyPtr) -> AzStyleFontFamilyPtr>,
        az_style_font_size_delete: Symbol<extern fn(_: &mut AzStyleFontSizePtr)>,
        az_style_font_size_shallow_copy: Symbol<extern fn(_: &AzStyleFontSizePtr) -> AzStyleFontSizePtr>,
        az_style_letter_spacing_delete: Symbol<extern fn(_: &mut AzStyleLetterSpacingPtr)>,
        az_style_letter_spacing_shallow_copy: Symbol<extern fn(_: &AzStyleLetterSpacingPtr) -> AzStyleLetterSpacingPtr>,
        az_style_line_height_delete: Symbol<extern fn(_: &mut AzStyleLineHeightPtr)>,
        az_style_line_height_shallow_copy: Symbol<extern fn(_: &AzStyleLineHeightPtr) -> AzStyleLineHeightPtr>,
        az_style_tab_width_delete: Symbol<extern fn(_: &mut AzStyleTabWidthPtr)>,
        az_style_tab_width_shallow_copy: Symbol<extern fn(_: &AzStyleTabWidthPtr) -> AzStyleTabWidthPtr>,
        az_style_text_alignment_horz_delete: Symbol<extern fn(_: &mut AzStyleTextAlignmentHorzPtr)>,
        az_style_text_alignment_horz_shallow_copy: Symbol<extern fn(_: &AzStyleTextAlignmentHorzPtr) -> AzStyleTextAlignmentHorzPtr>,
        az_style_text_color_delete: Symbol<extern fn(_: &mut AzStyleTextColorPtr)>,
        az_style_text_color_shallow_copy: Symbol<extern fn(_: &AzStyleTextColorPtr) -> AzStyleTextColorPtr>,
        az_style_word_spacing_delete: Symbol<extern fn(_: &mut AzStyleWordSpacingPtr)>,
        az_style_word_spacing_shallow_copy: Symbol<extern fn(_: &AzStyleWordSpacingPtr) -> AzStyleWordSpacingPtr>,
        az_box_shadow_pre_display_item_value_auto: Symbol<extern fn() -> AzBoxShadowPreDisplayItemValue>,
        az_box_shadow_pre_display_item_value_none: Symbol<extern fn() -> AzBoxShadowPreDisplayItemValue>,
        az_box_shadow_pre_display_item_value_inherit: Symbol<extern fn() -> AzBoxShadowPreDisplayItemValue>,
        az_box_shadow_pre_display_item_value_initial: Symbol<extern fn() -> AzBoxShadowPreDisplayItemValue>,
        az_box_shadow_pre_display_item_value_exact: Symbol<extern fn(_: AzBoxShadowPreDisplayItemPtr) -> AzBoxShadowPreDisplayItemValue>,
        az_box_shadow_pre_display_item_value_delete: Symbol<extern fn(_: &mut AzBoxShadowPreDisplayItemValue)>,
        az_box_shadow_pre_display_item_value_deep_copy: Symbol<extern fn(_: &AzBoxShadowPreDisplayItemValue) -> AzBoxShadowPreDisplayItemValue>,
        az_layout_align_content_value_auto: Symbol<extern fn() -> AzLayoutAlignContentValue>,
        az_layout_align_content_value_none: Symbol<extern fn() -> AzLayoutAlignContentValue>,
        az_layout_align_content_value_inherit: Symbol<extern fn() -> AzLayoutAlignContentValue>,
        az_layout_align_content_value_initial: Symbol<extern fn() -> AzLayoutAlignContentValue>,
        az_layout_align_content_value_exact: Symbol<extern fn(_: AzLayoutAlignContentPtr) -> AzLayoutAlignContentValue>,
        az_layout_align_content_value_delete: Symbol<extern fn(_: &mut AzLayoutAlignContentValue)>,
        az_layout_align_content_value_deep_copy: Symbol<extern fn(_: &AzLayoutAlignContentValue) -> AzLayoutAlignContentValue>,
        az_layout_align_items_value_auto: Symbol<extern fn() -> AzLayoutAlignItemsValue>,
        az_layout_align_items_value_none: Symbol<extern fn() -> AzLayoutAlignItemsValue>,
        az_layout_align_items_value_inherit: Symbol<extern fn() -> AzLayoutAlignItemsValue>,
        az_layout_align_items_value_initial: Symbol<extern fn() -> AzLayoutAlignItemsValue>,
        az_layout_align_items_value_exact: Symbol<extern fn(_: AzLayoutAlignItemsPtr) -> AzLayoutAlignItemsValue>,
        az_layout_align_items_value_delete: Symbol<extern fn(_: &mut AzLayoutAlignItemsValue)>,
        az_layout_align_items_value_deep_copy: Symbol<extern fn(_: &AzLayoutAlignItemsValue) -> AzLayoutAlignItemsValue>,
        az_layout_bottom_value_auto: Symbol<extern fn() -> AzLayoutBottomValue>,
        az_layout_bottom_value_none: Symbol<extern fn() -> AzLayoutBottomValue>,
        az_layout_bottom_value_inherit: Symbol<extern fn() -> AzLayoutBottomValue>,
        az_layout_bottom_value_initial: Symbol<extern fn() -> AzLayoutBottomValue>,
        az_layout_bottom_value_exact: Symbol<extern fn(_: AzLayoutBottomPtr) -> AzLayoutBottomValue>,
        az_layout_bottom_value_delete: Symbol<extern fn(_: &mut AzLayoutBottomValue)>,
        az_layout_bottom_value_deep_copy: Symbol<extern fn(_: &AzLayoutBottomValue) -> AzLayoutBottomValue>,
        az_layout_box_sizing_value_auto: Symbol<extern fn() -> AzLayoutBoxSizingValue>,
        az_layout_box_sizing_value_none: Symbol<extern fn() -> AzLayoutBoxSizingValue>,
        az_layout_box_sizing_value_inherit: Symbol<extern fn() -> AzLayoutBoxSizingValue>,
        az_layout_box_sizing_value_initial: Symbol<extern fn() -> AzLayoutBoxSizingValue>,
        az_layout_box_sizing_value_exact: Symbol<extern fn(_: AzLayoutBoxSizingPtr) -> AzLayoutBoxSizingValue>,
        az_layout_box_sizing_value_delete: Symbol<extern fn(_: &mut AzLayoutBoxSizingValue)>,
        az_layout_box_sizing_value_deep_copy: Symbol<extern fn(_: &AzLayoutBoxSizingValue) -> AzLayoutBoxSizingValue>,
        az_layout_direction_value_auto: Symbol<extern fn() -> AzLayoutDirectionValue>,
        az_layout_direction_value_none: Symbol<extern fn() -> AzLayoutDirectionValue>,
        az_layout_direction_value_inherit: Symbol<extern fn() -> AzLayoutDirectionValue>,
        az_layout_direction_value_initial: Symbol<extern fn() -> AzLayoutDirectionValue>,
        az_layout_direction_value_exact: Symbol<extern fn(_: AzLayoutDirectionPtr) -> AzLayoutDirectionValue>,
        az_layout_direction_value_delete: Symbol<extern fn(_: &mut AzLayoutDirectionValue)>,
        az_layout_direction_value_deep_copy: Symbol<extern fn(_: &AzLayoutDirectionValue) -> AzLayoutDirectionValue>,
        az_layout_display_value_auto: Symbol<extern fn() -> AzLayoutDisplayValue>,
        az_layout_display_value_none: Symbol<extern fn() -> AzLayoutDisplayValue>,
        az_layout_display_value_inherit: Symbol<extern fn() -> AzLayoutDisplayValue>,
        az_layout_display_value_initial: Symbol<extern fn() -> AzLayoutDisplayValue>,
        az_layout_display_value_exact: Symbol<extern fn(_: AzLayoutDisplayPtr) -> AzLayoutDisplayValue>,
        az_layout_display_value_delete: Symbol<extern fn(_: &mut AzLayoutDisplayValue)>,
        az_layout_display_value_deep_copy: Symbol<extern fn(_: &AzLayoutDisplayValue) -> AzLayoutDisplayValue>,
        az_layout_flex_grow_value_auto: Symbol<extern fn() -> AzLayoutFlexGrowValue>,
        az_layout_flex_grow_value_none: Symbol<extern fn() -> AzLayoutFlexGrowValue>,
        az_layout_flex_grow_value_inherit: Symbol<extern fn() -> AzLayoutFlexGrowValue>,
        az_layout_flex_grow_value_initial: Symbol<extern fn() -> AzLayoutFlexGrowValue>,
        az_layout_flex_grow_value_exact: Symbol<extern fn(_: AzLayoutFlexGrowPtr) -> AzLayoutFlexGrowValue>,
        az_layout_flex_grow_value_delete: Symbol<extern fn(_: &mut AzLayoutFlexGrowValue)>,
        az_layout_flex_grow_value_deep_copy: Symbol<extern fn(_: &AzLayoutFlexGrowValue) -> AzLayoutFlexGrowValue>,
        az_layout_flex_shrink_value_auto: Symbol<extern fn() -> AzLayoutFlexShrinkValue>,
        az_layout_flex_shrink_value_none: Symbol<extern fn() -> AzLayoutFlexShrinkValue>,
        az_layout_flex_shrink_value_inherit: Symbol<extern fn() -> AzLayoutFlexShrinkValue>,
        az_layout_flex_shrink_value_initial: Symbol<extern fn() -> AzLayoutFlexShrinkValue>,
        az_layout_flex_shrink_value_exact: Symbol<extern fn(_: AzLayoutFlexShrinkPtr) -> AzLayoutFlexShrinkValue>,
        az_layout_flex_shrink_value_delete: Symbol<extern fn(_: &mut AzLayoutFlexShrinkValue)>,
        az_layout_flex_shrink_value_deep_copy: Symbol<extern fn(_: &AzLayoutFlexShrinkValue) -> AzLayoutFlexShrinkValue>,
        az_layout_float_value_auto: Symbol<extern fn() -> AzLayoutFloatValue>,
        az_layout_float_value_none: Symbol<extern fn() -> AzLayoutFloatValue>,
        az_layout_float_value_inherit: Symbol<extern fn() -> AzLayoutFloatValue>,
        az_layout_float_value_initial: Symbol<extern fn() -> AzLayoutFloatValue>,
        az_layout_float_value_exact: Symbol<extern fn(_: AzLayoutFloatPtr) -> AzLayoutFloatValue>,
        az_layout_float_value_delete: Symbol<extern fn(_: &mut AzLayoutFloatValue)>,
        az_layout_float_value_deep_copy: Symbol<extern fn(_: &AzLayoutFloatValue) -> AzLayoutFloatValue>,
        az_layout_height_value_auto: Symbol<extern fn() -> AzLayoutHeightValue>,
        az_layout_height_value_none: Symbol<extern fn() -> AzLayoutHeightValue>,
        az_layout_height_value_inherit: Symbol<extern fn() -> AzLayoutHeightValue>,
        az_layout_height_value_initial: Symbol<extern fn() -> AzLayoutHeightValue>,
        az_layout_height_value_exact: Symbol<extern fn(_: AzLayoutHeightPtr) -> AzLayoutHeightValue>,
        az_layout_height_value_delete: Symbol<extern fn(_: &mut AzLayoutHeightValue)>,
        az_layout_height_value_deep_copy: Symbol<extern fn(_: &AzLayoutHeightValue) -> AzLayoutHeightValue>,
        az_layout_justify_content_value_auto: Symbol<extern fn() -> AzLayoutJustifyContentValue>,
        az_layout_justify_content_value_none: Symbol<extern fn() -> AzLayoutJustifyContentValue>,
        az_layout_justify_content_value_inherit: Symbol<extern fn() -> AzLayoutJustifyContentValue>,
        az_layout_justify_content_value_initial: Symbol<extern fn() -> AzLayoutJustifyContentValue>,
        az_layout_justify_content_value_exact: Symbol<extern fn(_: AzLayoutJustifyContentPtr) -> AzLayoutJustifyContentValue>,
        az_layout_justify_content_value_delete: Symbol<extern fn(_: &mut AzLayoutJustifyContentValue)>,
        az_layout_justify_content_value_deep_copy: Symbol<extern fn(_: &AzLayoutJustifyContentValue) -> AzLayoutJustifyContentValue>,
        az_layout_left_value_auto: Symbol<extern fn() -> AzLayoutLeftValue>,
        az_layout_left_value_none: Symbol<extern fn() -> AzLayoutLeftValue>,
        az_layout_left_value_inherit: Symbol<extern fn() -> AzLayoutLeftValue>,
        az_layout_left_value_initial: Symbol<extern fn() -> AzLayoutLeftValue>,
        az_layout_left_value_exact: Symbol<extern fn(_: AzLayoutLeftPtr) -> AzLayoutLeftValue>,
        az_layout_left_value_delete: Symbol<extern fn(_: &mut AzLayoutLeftValue)>,
        az_layout_left_value_deep_copy: Symbol<extern fn(_: &AzLayoutLeftValue) -> AzLayoutLeftValue>,
        az_layout_margin_bottom_value_auto: Symbol<extern fn() -> AzLayoutMarginBottomValue>,
        az_layout_margin_bottom_value_none: Symbol<extern fn() -> AzLayoutMarginBottomValue>,
        az_layout_margin_bottom_value_inherit: Symbol<extern fn() -> AzLayoutMarginBottomValue>,
        az_layout_margin_bottom_value_initial: Symbol<extern fn() -> AzLayoutMarginBottomValue>,
        az_layout_margin_bottom_value_exact: Symbol<extern fn(_: AzLayoutMarginBottomPtr) -> AzLayoutMarginBottomValue>,
        az_layout_margin_bottom_value_delete: Symbol<extern fn(_: &mut AzLayoutMarginBottomValue)>,
        az_layout_margin_bottom_value_deep_copy: Symbol<extern fn(_: &AzLayoutMarginBottomValue) -> AzLayoutMarginBottomValue>,
        az_layout_margin_left_value_auto: Symbol<extern fn() -> AzLayoutMarginLeftValue>,
        az_layout_margin_left_value_none: Symbol<extern fn() -> AzLayoutMarginLeftValue>,
        az_layout_margin_left_value_inherit: Symbol<extern fn() -> AzLayoutMarginLeftValue>,
        az_layout_margin_left_value_initial: Symbol<extern fn() -> AzLayoutMarginLeftValue>,
        az_layout_margin_left_value_exact: Symbol<extern fn(_: AzLayoutMarginLeftPtr) -> AzLayoutMarginLeftValue>,
        az_layout_margin_left_value_delete: Symbol<extern fn(_: &mut AzLayoutMarginLeftValue)>,
        az_layout_margin_left_value_deep_copy: Symbol<extern fn(_: &AzLayoutMarginLeftValue) -> AzLayoutMarginLeftValue>,
        az_layout_margin_right_value_auto: Symbol<extern fn() -> AzLayoutMarginRightValue>,
        az_layout_margin_right_value_none: Symbol<extern fn() -> AzLayoutMarginRightValue>,
        az_layout_margin_right_value_inherit: Symbol<extern fn() -> AzLayoutMarginRightValue>,
        az_layout_margin_right_value_initial: Symbol<extern fn() -> AzLayoutMarginRightValue>,
        az_layout_margin_right_value_exact: Symbol<extern fn(_: AzLayoutMarginRightPtr) -> AzLayoutMarginRightValue>,
        az_layout_margin_right_value_delete: Symbol<extern fn(_: &mut AzLayoutMarginRightValue)>,
        az_layout_margin_right_value_deep_copy: Symbol<extern fn(_: &AzLayoutMarginRightValue) -> AzLayoutMarginRightValue>,
        az_layout_margin_top_value_auto: Symbol<extern fn() -> AzLayoutMarginTopValue>,
        az_layout_margin_top_value_none: Symbol<extern fn() -> AzLayoutMarginTopValue>,
        az_layout_margin_top_value_inherit: Symbol<extern fn() -> AzLayoutMarginTopValue>,
        az_layout_margin_top_value_initial: Symbol<extern fn() -> AzLayoutMarginTopValue>,
        az_layout_margin_top_value_exact: Symbol<extern fn(_: AzLayoutMarginTopPtr) -> AzLayoutMarginTopValue>,
        az_layout_margin_top_value_delete: Symbol<extern fn(_: &mut AzLayoutMarginTopValue)>,
        az_layout_margin_top_value_deep_copy: Symbol<extern fn(_: &AzLayoutMarginTopValue) -> AzLayoutMarginTopValue>,
        az_layout_max_height_value_auto: Symbol<extern fn() -> AzLayoutMaxHeightValue>,
        az_layout_max_height_value_none: Symbol<extern fn() -> AzLayoutMaxHeightValue>,
        az_layout_max_height_value_inherit: Symbol<extern fn() -> AzLayoutMaxHeightValue>,
        az_layout_max_height_value_initial: Symbol<extern fn() -> AzLayoutMaxHeightValue>,
        az_layout_max_height_value_exact: Symbol<extern fn(_: AzLayoutMaxHeightPtr) -> AzLayoutMaxHeightValue>,
        az_layout_max_height_value_delete: Symbol<extern fn(_: &mut AzLayoutMaxHeightValue)>,
        az_layout_max_height_value_deep_copy: Symbol<extern fn(_: &AzLayoutMaxHeightValue) -> AzLayoutMaxHeightValue>,
        az_layout_max_width_value_auto: Symbol<extern fn() -> AzLayoutMaxWidthValue>,
        az_layout_max_width_value_none: Symbol<extern fn() -> AzLayoutMaxWidthValue>,
        az_layout_max_width_value_inherit: Symbol<extern fn() -> AzLayoutMaxWidthValue>,
        az_layout_max_width_value_initial: Symbol<extern fn() -> AzLayoutMaxWidthValue>,
        az_layout_max_width_value_exact: Symbol<extern fn(_: AzLayoutMaxWidthPtr) -> AzLayoutMaxWidthValue>,
        az_layout_max_width_value_delete: Symbol<extern fn(_: &mut AzLayoutMaxWidthValue)>,
        az_layout_max_width_value_deep_copy: Symbol<extern fn(_: &AzLayoutMaxWidthValue) -> AzLayoutMaxWidthValue>,
        az_layout_min_height_value_auto: Symbol<extern fn() -> AzLayoutMinHeightValue>,
        az_layout_min_height_value_none: Symbol<extern fn() -> AzLayoutMinHeightValue>,
        az_layout_min_height_value_inherit: Symbol<extern fn() -> AzLayoutMinHeightValue>,
        az_layout_min_height_value_initial: Symbol<extern fn() -> AzLayoutMinHeightValue>,
        az_layout_min_height_value_exact: Symbol<extern fn(_: AzLayoutMinHeightPtr) -> AzLayoutMinHeightValue>,
        az_layout_min_height_value_delete: Symbol<extern fn(_: &mut AzLayoutMinHeightValue)>,
        az_layout_min_height_value_deep_copy: Symbol<extern fn(_: &AzLayoutMinHeightValue) -> AzLayoutMinHeightValue>,
        az_layout_min_width_value_auto: Symbol<extern fn() -> AzLayoutMinWidthValue>,
        az_layout_min_width_value_none: Symbol<extern fn() -> AzLayoutMinWidthValue>,
        az_layout_min_width_value_inherit: Symbol<extern fn() -> AzLayoutMinWidthValue>,
        az_layout_min_width_value_initial: Symbol<extern fn() -> AzLayoutMinWidthValue>,
        az_layout_min_width_value_exact: Symbol<extern fn(_: AzLayoutMinWidthPtr) -> AzLayoutMinWidthValue>,
        az_layout_min_width_value_delete: Symbol<extern fn(_: &mut AzLayoutMinWidthValue)>,
        az_layout_min_width_value_deep_copy: Symbol<extern fn(_: &AzLayoutMinWidthValue) -> AzLayoutMinWidthValue>,
        az_layout_padding_bottom_value_auto: Symbol<extern fn() -> AzLayoutPaddingBottomValue>,
        az_layout_padding_bottom_value_none: Symbol<extern fn() -> AzLayoutPaddingBottomValue>,
        az_layout_padding_bottom_value_inherit: Symbol<extern fn() -> AzLayoutPaddingBottomValue>,
        az_layout_padding_bottom_value_initial: Symbol<extern fn() -> AzLayoutPaddingBottomValue>,
        az_layout_padding_bottom_value_exact: Symbol<extern fn(_: AzLayoutPaddingBottomPtr) -> AzLayoutPaddingBottomValue>,
        az_layout_padding_bottom_value_delete: Symbol<extern fn(_: &mut AzLayoutPaddingBottomValue)>,
        az_layout_padding_bottom_value_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingBottomValue) -> AzLayoutPaddingBottomValue>,
        az_layout_padding_left_value_auto: Symbol<extern fn() -> AzLayoutPaddingLeftValue>,
        az_layout_padding_left_value_none: Symbol<extern fn() -> AzLayoutPaddingLeftValue>,
        az_layout_padding_left_value_inherit: Symbol<extern fn() -> AzLayoutPaddingLeftValue>,
        az_layout_padding_left_value_initial: Symbol<extern fn() -> AzLayoutPaddingLeftValue>,
        az_layout_padding_left_value_exact: Symbol<extern fn(_: AzLayoutPaddingLeftPtr) -> AzLayoutPaddingLeftValue>,
        az_layout_padding_left_value_delete: Symbol<extern fn(_: &mut AzLayoutPaddingLeftValue)>,
        az_layout_padding_left_value_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingLeftValue) -> AzLayoutPaddingLeftValue>,
        az_layout_padding_right_value_auto: Symbol<extern fn() -> AzLayoutPaddingRightValue>,
        az_layout_padding_right_value_none: Symbol<extern fn() -> AzLayoutPaddingRightValue>,
        az_layout_padding_right_value_inherit: Symbol<extern fn() -> AzLayoutPaddingRightValue>,
        az_layout_padding_right_value_initial: Symbol<extern fn() -> AzLayoutPaddingRightValue>,
        az_layout_padding_right_value_exact: Symbol<extern fn(_: AzLayoutPaddingRightPtr) -> AzLayoutPaddingRightValue>,
        az_layout_padding_right_value_delete: Symbol<extern fn(_: &mut AzLayoutPaddingRightValue)>,
        az_layout_padding_right_value_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingRightValue) -> AzLayoutPaddingRightValue>,
        az_layout_padding_top_value_auto: Symbol<extern fn() -> AzLayoutPaddingTopValue>,
        az_layout_padding_top_value_none: Symbol<extern fn() -> AzLayoutPaddingTopValue>,
        az_layout_padding_top_value_inherit: Symbol<extern fn() -> AzLayoutPaddingTopValue>,
        az_layout_padding_top_value_initial: Symbol<extern fn() -> AzLayoutPaddingTopValue>,
        az_layout_padding_top_value_exact: Symbol<extern fn(_: AzLayoutPaddingTopPtr) -> AzLayoutPaddingTopValue>,
        az_layout_padding_top_value_delete: Symbol<extern fn(_: &mut AzLayoutPaddingTopValue)>,
        az_layout_padding_top_value_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingTopValue) -> AzLayoutPaddingTopValue>,
        az_layout_position_value_auto: Symbol<extern fn() -> AzLayoutPositionValue>,
        az_layout_position_value_none: Symbol<extern fn() -> AzLayoutPositionValue>,
        az_layout_position_value_inherit: Symbol<extern fn() -> AzLayoutPositionValue>,
        az_layout_position_value_initial: Symbol<extern fn() -> AzLayoutPositionValue>,
        az_layout_position_value_exact: Symbol<extern fn(_: AzLayoutPositionPtr) -> AzLayoutPositionValue>,
        az_layout_position_value_delete: Symbol<extern fn(_: &mut AzLayoutPositionValue)>,
        az_layout_position_value_deep_copy: Symbol<extern fn(_: &AzLayoutPositionValue) -> AzLayoutPositionValue>,
        az_layout_right_value_auto: Symbol<extern fn() -> AzLayoutRightValue>,
        az_layout_right_value_none: Symbol<extern fn() -> AzLayoutRightValue>,
        az_layout_right_value_inherit: Symbol<extern fn() -> AzLayoutRightValue>,
        az_layout_right_value_initial: Symbol<extern fn() -> AzLayoutRightValue>,
        az_layout_right_value_exact: Symbol<extern fn(_: AzLayoutRightPtr) -> AzLayoutRightValue>,
        az_layout_right_value_delete: Symbol<extern fn(_: &mut AzLayoutRightValue)>,
        az_layout_right_value_deep_copy: Symbol<extern fn(_: &AzLayoutRightValue) -> AzLayoutRightValue>,
        az_layout_top_value_auto: Symbol<extern fn() -> AzLayoutTopValue>,
        az_layout_top_value_none: Symbol<extern fn() -> AzLayoutTopValue>,
        az_layout_top_value_inherit: Symbol<extern fn() -> AzLayoutTopValue>,
        az_layout_top_value_initial: Symbol<extern fn() -> AzLayoutTopValue>,
        az_layout_top_value_exact: Symbol<extern fn(_: AzLayoutTopPtr) -> AzLayoutTopValue>,
        az_layout_top_value_delete: Symbol<extern fn(_: &mut AzLayoutTopValue)>,
        az_layout_top_value_deep_copy: Symbol<extern fn(_: &AzLayoutTopValue) -> AzLayoutTopValue>,
        az_layout_width_value_auto: Symbol<extern fn() -> AzLayoutWidthValue>,
        az_layout_width_value_none: Symbol<extern fn() -> AzLayoutWidthValue>,
        az_layout_width_value_inherit: Symbol<extern fn() -> AzLayoutWidthValue>,
        az_layout_width_value_initial: Symbol<extern fn() -> AzLayoutWidthValue>,
        az_layout_width_value_exact: Symbol<extern fn(_: AzLayoutWidthPtr) -> AzLayoutWidthValue>,
        az_layout_width_value_delete: Symbol<extern fn(_: &mut AzLayoutWidthValue)>,
        az_layout_width_value_deep_copy: Symbol<extern fn(_: &AzLayoutWidthValue) -> AzLayoutWidthValue>,
        az_layout_wrap_value_auto: Symbol<extern fn() -> AzLayoutWrapValue>,
        az_layout_wrap_value_none: Symbol<extern fn() -> AzLayoutWrapValue>,
        az_layout_wrap_value_inherit: Symbol<extern fn() -> AzLayoutWrapValue>,
        az_layout_wrap_value_initial: Symbol<extern fn() -> AzLayoutWrapValue>,
        az_layout_wrap_value_exact: Symbol<extern fn(_: AzLayoutWrapPtr) -> AzLayoutWrapValue>,
        az_layout_wrap_value_delete: Symbol<extern fn(_: &mut AzLayoutWrapValue)>,
        az_layout_wrap_value_deep_copy: Symbol<extern fn(_: &AzLayoutWrapValue) -> AzLayoutWrapValue>,
        az_overflow_value_auto: Symbol<extern fn() -> AzOverflowValue>,
        az_overflow_value_none: Symbol<extern fn() -> AzOverflowValue>,
        az_overflow_value_inherit: Symbol<extern fn() -> AzOverflowValue>,
        az_overflow_value_initial: Symbol<extern fn() -> AzOverflowValue>,
        az_overflow_value_exact: Symbol<extern fn(_: AzOverflowPtr) -> AzOverflowValue>,
        az_overflow_value_delete: Symbol<extern fn(_: &mut AzOverflowValue)>,
        az_overflow_value_deep_copy: Symbol<extern fn(_: &AzOverflowValue) -> AzOverflowValue>,
        az_style_background_content_value_auto: Symbol<extern fn() -> AzStyleBackgroundContentValue>,
        az_style_background_content_value_none: Symbol<extern fn() -> AzStyleBackgroundContentValue>,
        az_style_background_content_value_inherit: Symbol<extern fn() -> AzStyleBackgroundContentValue>,
        az_style_background_content_value_initial: Symbol<extern fn() -> AzStyleBackgroundContentValue>,
        az_style_background_content_value_exact: Symbol<extern fn(_: AzStyleBackgroundContentPtr) -> AzStyleBackgroundContentValue>,
        az_style_background_content_value_delete: Symbol<extern fn(_: &mut AzStyleBackgroundContentValue)>,
        az_style_background_content_value_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue>,
        az_style_background_position_value_auto: Symbol<extern fn() -> AzStyleBackgroundPositionValue>,
        az_style_background_position_value_none: Symbol<extern fn() -> AzStyleBackgroundPositionValue>,
        az_style_background_position_value_inherit: Symbol<extern fn() -> AzStyleBackgroundPositionValue>,
        az_style_background_position_value_initial: Symbol<extern fn() -> AzStyleBackgroundPositionValue>,
        az_style_background_position_value_exact: Symbol<extern fn(_: AzStyleBackgroundPositionPtr) -> AzStyleBackgroundPositionValue>,
        az_style_background_position_value_delete: Symbol<extern fn(_: &mut AzStyleBackgroundPositionValue)>,
        az_style_background_position_value_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundPositionValue) -> AzStyleBackgroundPositionValue>,
        az_style_background_repeat_value_auto: Symbol<extern fn() -> AzStyleBackgroundRepeatValue>,
        az_style_background_repeat_value_none: Symbol<extern fn() -> AzStyleBackgroundRepeatValue>,
        az_style_background_repeat_value_inherit: Symbol<extern fn() -> AzStyleBackgroundRepeatValue>,
        az_style_background_repeat_value_initial: Symbol<extern fn() -> AzStyleBackgroundRepeatValue>,
        az_style_background_repeat_value_exact: Symbol<extern fn(_: AzStyleBackgroundRepeatPtr) -> AzStyleBackgroundRepeatValue>,
        az_style_background_repeat_value_delete: Symbol<extern fn(_: &mut AzStyleBackgroundRepeatValue)>,
        az_style_background_repeat_value_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundRepeatValue) -> AzStyleBackgroundRepeatValue>,
        az_style_background_size_value_auto: Symbol<extern fn() -> AzStyleBackgroundSizeValue>,
        az_style_background_size_value_none: Symbol<extern fn() -> AzStyleBackgroundSizeValue>,
        az_style_background_size_value_inherit: Symbol<extern fn() -> AzStyleBackgroundSizeValue>,
        az_style_background_size_value_initial: Symbol<extern fn() -> AzStyleBackgroundSizeValue>,
        az_style_background_size_value_exact: Symbol<extern fn(_: AzStyleBackgroundSizePtr) -> AzStyleBackgroundSizeValue>,
        az_style_background_size_value_delete: Symbol<extern fn(_: &mut AzStyleBackgroundSizeValue)>,
        az_style_background_size_value_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundSizeValue) -> AzStyleBackgroundSizeValue>,
        az_style_border_bottom_color_value_auto: Symbol<extern fn() -> AzStyleBorderBottomColorValue>,
        az_style_border_bottom_color_value_none: Symbol<extern fn() -> AzStyleBorderBottomColorValue>,
        az_style_border_bottom_color_value_inherit: Symbol<extern fn() -> AzStyleBorderBottomColorValue>,
        az_style_border_bottom_color_value_initial: Symbol<extern fn() -> AzStyleBorderBottomColorValue>,
        az_style_border_bottom_color_value_exact: Symbol<extern fn(_: AzStyleBorderBottomColorPtr) -> AzStyleBorderBottomColorValue>,
        az_style_border_bottom_color_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomColorValue)>,
        az_style_border_bottom_color_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomColorValue) -> AzStyleBorderBottomColorValue>,
        az_style_border_bottom_left_radius_value_auto: Symbol<extern fn() -> AzStyleBorderBottomLeftRadiusValue>,
        az_style_border_bottom_left_radius_value_none: Symbol<extern fn() -> AzStyleBorderBottomLeftRadiusValue>,
        az_style_border_bottom_left_radius_value_inherit: Symbol<extern fn() -> AzStyleBorderBottomLeftRadiusValue>,
        az_style_border_bottom_left_radius_value_initial: Symbol<extern fn() -> AzStyleBorderBottomLeftRadiusValue>,
        az_style_border_bottom_left_radius_value_exact: Symbol<extern fn(_: AzStyleBorderBottomLeftRadiusPtr) -> AzStyleBorderBottomLeftRadiusValue>,
        az_style_border_bottom_left_radius_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomLeftRadiusValue)>,
        az_style_border_bottom_left_radius_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomLeftRadiusValue) -> AzStyleBorderBottomLeftRadiusValue>,
        az_style_border_bottom_right_radius_value_auto: Symbol<extern fn() -> AzStyleBorderBottomRightRadiusValue>,
        az_style_border_bottom_right_radius_value_none: Symbol<extern fn() -> AzStyleBorderBottomRightRadiusValue>,
        az_style_border_bottom_right_radius_value_inherit: Symbol<extern fn() -> AzStyleBorderBottomRightRadiusValue>,
        az_style_border_bottom_right_radius_value_initial: Symbol<extern fn() -> AzStyleBorderBottomRightRadiusValue>,
        az_style_border_bottom_right_radius_value_exact: Symbol<extern fn(_: AzStyleBorderBottomRightRadiusPtr) -> AzStyleBorderBottomRightRadiusValue>,
        az_style_border_bottom_right_radius_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomRightRadiusValue)>,
        az_style_border_bottom_right_radius_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomRightRadiusValue) -> AzStyleBorderBottomRightRadiusValue>,
        az_style_border_bottom_style_value_auto: Symbol<extern fn() -> AzStyleBorderBottomStyleValue>,
        az_style_border_bottom_style_value_none: Symbol<extern fn() -> AzStyleBorderBottomStyleValue>,
        az_style_border_bottom_style_value_inherit: Symbol<extern fn() -> AzStyleBorderBottomStyleValue>,
        az_style_border_bottom_style_value_initial: Symbol<extern fn() -> AzStyleBorderBottomStyleValue>,
        az_style_border_bottom_style_value_exact: Symbol<extern fn(_: AzStyleBorderBottomStylePtr) -> AzStyleBorderBottomStyleValue>,
        az_style_border_bottom_style_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomStyleValue)>,
        az_style_border_bottom_style_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomStyleValue) -> AzStyleBorderBottomStyleValue>,
        az_style_border_bottom_width_value_auto: Symbol<extern fn() -> AzStyleBorderBottomWidthValue>,
        az_style_border_bottom_width_value_none: Symbol<extern fn() -> AzStyleBorderBottomWidthValue>,
        az_style_border_bottom_width_value_inherit: Symbol<extern fn() -> AzStyleBorderBottomWidthValue>,
        az_style_border_bottom_width_value_initial: Symbol<extern fn() -> AzStyleBorderBottomWidthValue>,
        az_style_border_bottom_width_value_exact: Symbol<extern fn(_: AzStyleBorderBottomWidthPtr) -> AzStyleBorderBottomWidthValue>,
        az_style_border_bottom_width_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomWidthValue)>,
        az_style_border_bottom_width_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomWidthValue) -> AzStyleBorderBottomWidthValue>,
        az_style_border_left_color_value_auto: Symbol<extern fn() -> AzStyleBorderLeftColorValue>,
        az_style_border_left_color_value_none: Symbol<extern fn() -> AzStyleBorderLeftColorValue>,
        az_style_border_left_color_value_inherit: Symbol<extern fn() -> AzStyleBorderLeftColorValue>,
        az_style_border_left_color_value_initial: Symbol<extern fn() -> AzStyleBorderLeftColorValue>,
        az_style_border_left_color_value_exact: Symbol<extern fn(_: AzStyleBorderLeftColorPtr) -> AzStyleBorderLeftColorValue>,
        az_style_border_left_color_value_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftColorValue)>,
        az_style_border_left_color_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderLeftColorValue) -> AzStyleBorderLeftColorValue>,
        az_style_border_left_style_value_auto: Symbol<extern fn() -> AzStyleBorderLeftStyleValue>,
        az_style_border_left_style_value_none: Symbol<extern fn() -> AzStyleBorderLeftStyleValue>,
        az_style_border_left_style_value_inherit: Symbol<extern fn() -> AzStyleBorderLeftStyleValue>,
        az_style_border_left_style_value_initial: Symbol<extern fn() -> AzStyleBorderLeftStyleValue>,
        az_style_border_left_style_value_exact: Symbol<extern fn(_: AzStyleBorderLeftStylePtr) -> AzStyleBorderLeftStyleValue>,
        az_style_border_left_style_value_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftStyleValue)>,
        az_style_border_left_style_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderLeftStyleValue) -> AzStyleBorderLeftStyleValue>,
        az_style_border_left_width_value_auto: Symbol<extern fn() -> AzStyleBorderLeftWidthValue>,
        az_style_border_left_width_value_none: Symbol<extern fn() -> AzStyleBorderLeftWidthValue>,
        az_style_border_left_width_value_inherit: Symbol<extern fn() -> AzStyleBorderLeftWidthValue>,
        az_style_border_left_width_value_initial: Symbol<extern fn() -> AzStyleBorderLeftWidthValue>,
        az_style_border_left_width_value_exact: Symbol<extern fn(_: AzStyleBorderLeftWidthPtr) -> AzStyleBorderLeftWidthValue>,
        az_style_border_left_width_value_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftWidthValue)>,
        az_style_border_left_width_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderLeftWidthValue) -> AzStyleBorderLeftWidthValue>,
        az_style_border_right_color_value_auto: Symbol<extern fn() -> AzStyleBorderRightColorValue>,
        az_style_border_right_color_value_none: Symbol<extern fn() -> AzStyleBorderRightColorValue>,
        az_style_border_right_color_value_inherit: Symbol<extern fn() -> AzStyleBorderRightColorValue>,
        az_style_border_right_color_value_initial: Symbol<extern fn() -> AzStyleBorderRightColorValue>,
        az_style_border_right_color_value_exact: Symbol<extern fn(_: AzStyleBorderRightColorPtr) -> AzStyleBorderRightColorValue>,
        az_style_border_right_color_value_delete: Symbol<extern fn(_: &mut AzStyleBorderRightColorValue)>,
        az_style_border_right_color_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderRightColorValue) -> AzStyleBorderRightColorValue>,
        az_style_border_right_style_value_auto: Symbol<extern fn() -> AzStyleBorderRightStyleValue>,
        az_style_border_right_style_value_none: Symbol<extern fn() -> AzStyleBorderRightStyleValue>,
        az_style_border_right_style_value_inherit: Symbol<extern fn() -> AzStyleBorderRightStyleValue>,
        az_style_border_right_style_value_initial: Symbol<extern fn() -> AzStyleBorderRightStyleValue>,
        az_style_border_right_style_value_exact: Symbol<extern fn(_: AzStyleBorderRightStylePtr) -> AzStyleBorderRightStyleValue>,
        az_style_border_right_style_value_delete: Symbol<extern fn(_: &mut AzStyleBorderRightStyleValue)>,
        az_style_border_right_style_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderRightStyleValue) -> AzStyleBorderRightStyleValue>,
        az_style_border_right_width_value_auto: Symbol<extern fn() -> AzStyleBorderRightWidthValue>,
        az_style_border_right_width_value_none: Symbol<extern fn() -> AzStyleBorderRightWidthValue>,
        az_style_border_right_width_value_inherit: Symbol<extern fn() -> AzStyleBorderRightWidthValue>,
        az_style_border_right_width_value_initial: Symbol<extern fn() -> AzStyleBorderRightWidthValue>,
        az_style_border_right_width_value_exact: Symbol<extern fn(_: AzStyleBorderRightWidthPtr) -> AzStyleBorderRightWidthValue>,
        az_style_border_right_width_value_delete: Symbol<extern fn(_: &mut AzStyleBorderRightWidthValue)>,
        az_style_border_right_width_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderRightWidthValue) -> AzStyleBorderRightWidthValue>,
        az_style_border_top_color_value_auto: Symbol<extern fn() -> AzStyleBorderTopColorValue>,
        az_style_border_top_color_value_none: Symbol<extern fn() -> AzStyleBorderTopColorValue>,
        az_style_border_top_color_value_inherit: Symbol<extern fn() -> AzStyleBorderTopColorValue>,
        az_style_border_top_color_value_initial: Symbol<extern fn() -> AzStyleBorderTopColorValue>,
        az_style_border_top_color_value_exact: Symbol<extern fn(_: AzStyleBorderTopColorPtr) -> AzStyleBorderTopColorValue>,
        az_style_border_top_color_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopColorValue)>,
        az_style_border_top_color_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopColorValue) -> AzStyleBorderTopColorValue>,
        az_style_border_top_left_radius_value_auto: Symbol<extern fn() -> AzStyleBorderTopLeftRadiusValue>,
        az_style_border_top_left_radius_value_none: Symbol<extern fn() -> AzStyleBorderTopLeftRadiusValue>,
        az_style_border_top_left_radius_value_inherit: Symbol<extern fn() -> AzStyleBorderTopLeftRadiusValue>,
        az_style_border_top_left_radius_value_initial: Symbol<extern fn() -> AzStyleBorderTopLeftRadiusValue>,
        az_style_border_top_left_radius_value_exact: Symbol<extern fn(_: AzStyleBorderTopLeftRadiusPtr) -> AzStyleBorderTopLeftRadiusValue>,
        az_style_border_top_left_radius_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopLeftRadiusValue)>,
        az_style_border_top_left_radius_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopLeftRadiusValue) -> AzStyleBorderTopLeftRadiusValue>,
        az_style_border_top_right_radius_value_auto: Symbol<extern fn() -> AzStyleBorderTopRightRadiusValue>,
        az_style_border_top_right_radius_value_none: Symbol<extern fn() -> AzStyleBorderTopRightRadiusValue>,
        az_style_border_top_right_radius_value_inherit: Symbol<extern fn() -> AzStyleBorderTopRightRadiusValue>,
        az_style_border_top_right_radius_value_initial: Symbol<extern fn() -> AzStyleBorderTopRightRadiusValue>,
        az_style_border_top_right_radius_value_exact: Symbol<extern fn(_: AzStyleBorderTopRightRadiusPtr) -> AzStyleBorderTopRightRadiusValue>,
        az_style_border_top_right_radius_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopRightRadiusValue)>,
        az_style_border_top_right_radius_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopRightRadiusValue) -> AzStyleBorderTopRightRadiusValue>,
        az_style_border_top_style_value_auto: Symbol<extern fn() -> AzStyleBorderTopStyleValue>,
        az_style_border_top_style_value_none: Symbol<extern fn() -> AzStyleBorderTopStyleValue>,
        az_style_border_top_style_value_inherit: Symbol<extern fn() -> AzStyleBorderTopStyleValue>,
        az_style_border_top_style_value_initial: Symbol<extern fn() -> AzStyleBorderTopStyleValue>,
        az_style_border_top_style_value_exact: Symbol<extern fn(_: AzStyleBorderTopStylePtr) -> AzStyleBorderTopStyleValue>,
        az_style_border_top_style_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopStyleValue)>,
        az_style_border_top_style_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopStyleValue) -> AzStyleBorderTopStyleValue>,
        az_style_border_top_width_value_auto: Symbol<extern fn() -> AzStyleBorderTopWidthValue>,
        az_style_border_top_width_value_none: Symbol<extern fn() -> AzStyleBorderTopWidthValue>,
        az_style_border_top_width_value_inherit: Symbol<extern fn() -> AzStyleBorderTopWidthValue>,
        az_style_border_top_width_value_initial: Symbol<extern fn() -> AzStyleBorderTopWidthValue>,
        az_style_border_top_width_value_exact: Symbol<extern fn(_: AzStyleBorderTopWidthPtr) -> AzStyleBorderTopWidthValue>,
        az_style_border_top_width_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopWidthValue)>,
        az_style_border_top_width_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopWidthValue) -> AzStyleBorderTopWidthValue>,
        az_style_cursor_value_auto: Symbol<extern fn() -> AzStyleCursorValue>,
        az_style_cursor_value_none: Symbol<extern fn() -> AzStyleCursorValue>,
        az_style_cursor_value_inherit: Symbol<extern fn() -> AzStyleCursorValue>,
        az_style_cursor_value_initial: Symbol<extern fn() -> AzStyleCursorValue>,
        az_style_cursor_value_exact: Symbol<extern fn(_: AzStyleCursorPtr) -> AzStyleCursorValue>,
        az_style_cursor_value_delete: Symbol<extern fn(_: &mut AzStyleCursorValue)>,
        az_style_cursor_value_deep_copy: Symbol<extern fn(_: &AzStyleCursorValue) -> AzStyleCursorValue>,
        az_style_font_family_value_auto: Symbol<extern fn() -> AzStyleFontFamilyValue>,
        az_style_font_family_value_none: Symbol<extern fn() -> AzStyleFontFamilyValue>,
        az_style_font_family_value_inherit: Symbol<extern fn() -> AzStyleFontFamilyValue>,
        az_style_font_family_value_initial: Symbol<extern fn() -> AzStyleFontFamilyValue>,
        az_style_font_family_value_exact: Symbol<extern fn(_: AzStyleFontFamilyPtr) -> AzStyleFontFamilyValue>,
        az_style_font_family_value_delete: Symbol<extern fn(_: &mut AzStyleFontFamilyValue)>,
        az_style_font_family_value_deep_copy: Symbol<extern fn(_: &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue>,
        az_style_font_size_value_auto: Symbol<extern fn() -> AzStyleFontSizeValue>,
        az_style_font_size_value_none: Symbol<extern fn() -> AzStyleFontSizeValue>,
        az_style_font_size_value_inherit: Symbol<extern fn() -> AzStyleFontSizeValue>,
        az_style_font_size_value_initial: Symbol<extern fn() -> AzStyleFontSizeValue>,
        az_style_font_size_value_exact: Symbol<extern fn(_: AzStyleFontSizePtr) -> AzStyleFontSizeValue>,
        az_style_font_size_value_delete: Symbol<extern fn(_: &mut AzStyleFontSizeValue)>,
        az_style_font_size_value_deep_copy: Symbol<extern fn(_: &AzStyleFontSizeValue) -> AzStyleFontSizeValue>,
        az_style_letter_spacing_value_auto: Symbol<extern fn() -> AzStyleLetterSpacingValue>,
        az_style_letter_spacing_value_none: Symbol<extern fn() -> AzStyleLetterSpacingValue>,
        az_style_letter_spacing_value_inherit: Symbol<extern fn() -> AzStyleLetterSpacingValue>,
        az_style_letter_spacing_value_initial: Symbol<extern fn() -> AzStyleLetterSpacingValue>,
        az_style_letter_spacing_value_exact: Symbol<extern fn(_: AzStyleLetterSpacingPtr) -> AzStyleLetterSpacingValue>,
        az_style_letter_spacing_value_delete: Symbol<extern fn(_: &mut AzStyleLetterSpacingValue)>,
        az_style_letter_spacing_value_deep_copy: Symbol<extern fn(_: &AzStyleLetterSpacingValue) -> AzStyleLetterSpacingValue>,
        az_style_line_height_value_auto: Symbol<extern fn() -> AzStyleLineHeightValue>,
        az_style_line_height_value_none: Symbol<extern fn() -> AzStyleLineHeightValue>,
        az_style_line_height_value_inherit: Symbol<extern fn() -> AzStyleLineHeightValue>,
        az_style_line_height_value_initial: Symbol<extern fn() -> AzStyleLineHeightValue>,
        az_style_line_height_value_exact: Symbol<extern fn(_: AzStyleLineHeightPtr) -> AzStyleLineHeightValue>,
        az_style_line_height_value_delete: Symbol<extern fn(_: &mut AzStyleLineHeightValue)>,
        az_style_line_height_value_deep_copy: Symbol<extern fn(_: &AzStyleLineHeightValue) -> AzStyleLineHeightValue>,
        az_style_tab_width_value_auto: Symbol<extern fn() -> AzStyleTabWidthValue>,
        az_style_tab_width_value_none: Symbol<extern fn() -> AzStyleTabWidthValue>,
        az_style_tab_width_value_inherit: Symbol<extern fn() -> AzStyleTabWidthValue>,
        az_style_tab_width_value_initial: Symbol<extern fn() -> AzStyleTabWidthValue>,
        az_style_tab_width_value_exact: Symbol<extern fn(_: AzStyleTabWidthPtr) -> AzStyleTabWidthValue>,
        az_style_tab_width_value_delete: Symbol<extern fn(_: &mut AzStyleTabWidthValue)>,
        az_style_tab_width_value_deep_copy: Symbol<extern fn(_: &AzStyleTabWidthValue) -> AzStyleTabWidthValue>,
        az_style_text_alignment_horz_value_auto: Symbol<extern fn() -> AzStyleTextAlignmentHorzValue>,
        az_style_text_alignment_horz_value_none: Symbol<extern fn() -> AzStyleTextAlignmentHorzValue>,
        az_style_text_alignment_horz_value_inherit: Symbol<extern fn() -> AzStyleTextAlignmentHorzValue>,
        az_style_text_alignment_horz_value_initial: Symbol<extern fn() -> AzStyleTextAlignmentHorzValue>,
        az_style_text_alignment_horz_value_exact: Symbol<extern fn(_: AzStyleTextAlignmentHorzPtr) -> AzStyleTextAlignmentHorzValue>,
        az_style_text_alignment_horz_value_delete: Symbol<extern fn(_: &mut AzStyleTextAlignmentHorzValue)>,
        az_style_text_alignment_horz_value_deep_copy: Symbol<extern fn(_: &AzStyleTextAlignmentHorzValue) -> AzStyleTextAlignmentHorzValue>,
        az_style_text_color_value_auto: Symbol<extern fn() -> AzStyleTextColorValue>,
        az_style_text_color_value_none: Symbol<extern fn() -> AzStyleTextColorValue>,
        az_style_text_color_value_inherit: Symbol<extern fn() -> AzStyleTextColorValue>,
        az_style_text_color_value_initial: Symbol<extern fn() -> AzStyleTextColorValue>,
        az_style_text_color_value_exact: Symbol<extern fn(_: AzStyleTextColorPtr) -> AzStyleTextColorValue>,
        az_style_text_color_value_delete: Symbol<extern fn(_: &mut AzStyleTextColorValue)>,
        az_style_text_color_value_deep_copy: Symbol<extern fn(_: &AzStyleTextColorValue) -> AzStyleTextColorValue>,
        az_style_word_spacing_value_auto: Symbol<extern fn() -> AzStyleWordSpacingValue>,
        az_style_word_spacing_value_none: Symbol<extern fn() -> AzStyleWordSpacingValue>,
        az_style_word_spacing_value_inherit: Symbol<extern fn() -> AzStyleWordSpacingValue>,
        az_style_word_spacing_value_initial: Symbol<extern fn() -> AzStyleWordSpacingValue>,
        az_style_word_spacing_value_exact: Symbol<extern fn(_: AzStyleWordSpacingPtr) -> AzStyleWordSpacingValue>,
        az_style_word_spacing_value_delete: Symbol<extern fn(_: &mut AzStyleWordSpacingValue)>,
        az_style_word_spacing_value_deep_copy: Symbol<extern fn(_: &AzStyleWordSpacingValue) -> AzStyleWordSpacingValue>,
        az_css_property_text_color: Symbol<extern fn(_: AzStyleTextColorValue) -> AzCssProperty>,
        az_css_property_font_size: Symbol<extern fn(_: AzStyleFontSizeValue) -> AzCssProperty>,
        az_css_property_font_family: Symbol<extern fn(_: AzStyleFontFamilyValue) -> AzCssProperty>,
        az_css_property_text_align: Symbol<extern fn(_: AzStyleTextAlignmentHorzValue) -> AzCssProperty>,
        az_css_property_letter_spacing: Symbol<extern fn(_: AzStyleLetterSpacingValue) -> AzCssProperty>,
        az_css_property_line_height: Symbol<extern fn(_: AzStyleLineHeightValue) -> AzCssProperty>,
        az_css_property_word_spacing: Symbol<extern fn(_: AzStyleWordSpacingValue) -> AzCssProperty>,
        az_css_property_tab_width: Symbol<extern fn(_: AzStyleTabWidthValue) -> AzCssProperty>,
        az_css_property_cursor: Symbol<extern fn(_: AzStyleCursorValue) -> AzCssProperty>,
        az_css_property_display: Symbol<extern fn(_: AzLayoutDisplayValue) -> AzCssProperty>,
        az_css_property_float: Symbol<extern fn(_: AzLayoutFloatValue) -> AzCssProperty>,
        az_css_property_box_sizing: Symbol<extern fn(_: AzLayoutBoxSizingValue) -> AzCssProperty>,
        az_css_property_width: Symbol<extern fn(_: AzLayoutWidthValue) -> AzCssProperty>,
        az_css_property_height: Symbol<extern fn(_: AzLayoutHeightValue) -> AzCssProperty>,
        az_css_property_min_width: Symbol<extern fn(_: AzLayoutMinWidthValue) -> AzCssProperty>,
        az_css_property_min_height: Symbol<extern fn(_: AzLayoutMinHeightValue) -> AzCssProperty>,
        az_css_property_max_width: Symbol<extern fn(_: AzLayoutMaxWidthValue) -> AzCssProperty>,
        az_css_property_max_height: Symbol<extern fn(_: AzLayoutMaxHeightValue) -> AzCssProperty>,
        az_css_property_position: Symbol<extern fn(_: AzLayoutPositionValue) -> AzCssProperty>,
        az_css_property_top: Symbol<extern fn(_: AzLayoutTopValue) -> AzCssProperty>,
        az_css_property_right: Symbol<extern fn(_: AzLayoutRightValue) -> AzCssProperty>,
        az_css_property_left: Symbol<extern fn(_: AzLayoutLeftValue) -> AzCssProperty>,
        az_css_property_bottom: Symbol<extern fn(_: AzLayoutBottomValue) -> AzCssProperty>,
        az_css_property_flex_wrap: Symbol<extern fn(_: AzLayoutWrapValue) -> AzCssProperty>,
        az_css_property_flex_direction: Symbol<extern fn(_: AzLayoutDirectionValue) -> AzCssProperty>,
        az_css_property_flex_grow: Symbol<extern fn(_: AzLayoutFlexGrowValue) -> AzCssProperty>,
        az_css_property_flex_shrink: Symbol<extern fn(_: AzLayoutFlexShrinkValue) -> AzCssProperty>,
        az_css_property_justify_content: Symbol<extern fn(_: AzLayoutJustifyContentValue) -> AzCssProperty>,
        az_css_property_align_items: Symbol<extern fn(_: AzLayoutAlignItemsValue) -> AzCssProperty>,
        az_css_property_align_content: Symbol<extern fn(_: AzLayoutAlignContentValue) -> AzCssProperty>,
        az_css_property_background_content: Symbol<extern fn(_: AzStyleBackgroundContentValue) -> AzCssProperty>,
        az_css_property_background_position: Symbol<extern fn(_: AzStyleBackgroundPositionValue) -> AzCssProperty>,
        az_css_property_background_size: Symbol<extern fn(_: AzStyleBackgroundSizeValue) -> AzCssProperty>,
        az_css_property_background_repeat: Symbol<extern fn(_: AzStyleBackgroundRepeatValue) -> AzCssProperty>,
        az_css_property_overflow_x: Symbol<extern fn(_: AzOverflowValue) -> AzCssProperty>,
        az_css_property_overflow_y: Symbol<extern fn(_: AzOverflowValue) -> AzCssProperty>,
        az_css_property_padding_top: Symbol<extern fn(_: AzLayoutPaddingTopValue) -> AzCssProperty>,
        az_css_property_padding_left: Symbol<extern fn(_: AzLayoutPaddingLeftValue) -> AzCssProperty>,
        az_css_property_padding_right: Symbol<extern fn(_: AzLayoutPaddingRightValue) -> AzCssProperty>,
        az_css_property_padding_bottom: Symbol<extern fn(_: AzLayoutPaddingBottomValue) -> AzCssProperty>,
        az_css_property_margin_top: Symbol<extern fn(_: AzLayoutMarginTopValue) -> AzCssProperty>,
        az_css_property_margin_left: Symbol<extern fn(_: AzLayoutMarginLeftValue) -> AzCssProperty>,
        az_css_property_margin_right: Symbol<extern fn(_: AzLayoutMarginRightValue) -> AzCssProperty>,
        az_css_property_margin_bottom: Symbol<extern fn(_: AzLayoutMarginBottomValue) -> AzCssProperty>,
        az_css_property_border_top_left_radius: Symbol<extern fn(_: AzStyleBorderTopLeftRadiusValue) -> AzCssProperty>,
        az_css_property_border_top_right_radius: Symbol<extern fn(_: AzStyleBorderTopRightRadiusValue) -> AzCssProperty>,
        az_css_property_border_bottom_left_radius: Symbol<extern fn(_: AzStyleBorderBottomLeftRadiusValue) -> AzCssProperty>,
        az_css_property_border_bottom_right_radius: Symbol<extern fn(_: AzStyleBorderBottomRightRadiusValue) -> AzCssProperty>,
        az_css_property_border_top_color: Symbol<extern fn(_: AzStyleBorderTopColorValue) -> AzCssProperty>,
        az_css_property_border_right_color: Symbol<extern fn(_: AzStyleBorderRightColorValue) -> AzCssProperty>,
        az_css_property_border_left_color: Symbol<extern fn(_: AzStyleBorderLeftColorValue) -> AzCssProperty>,
        az_css_property_border_bottom_color: Symbol<extern fn(_: AzStyleBorderBottomColorValue) -> AzCssProperty>,
        az_css_property_border_top_style: Symbol<extern fn(_: AzStyleBorderTopStyleValue) -> AzCssProperty>,
        az_css_property_border_right_style: Symbol<extern fn(_: AzStyleBorderRightStyleValue) -> AzCssProperty>,
        az_css_property_border_left_style: Symbol<extern fn(_: AzStyleBorderLeftStyleValue) -> AzCssProperty>,
        az_css_property_border_bottom_style: Symbol<extern fn(_: AzStyleBorderBottomStyleValue) -> AzCssProperty>,
        az_css_property_border_top_width: Symbol<extern fn(_: AzStyleBorderTopWidthValue) -> AzCssProperty>,
        az_css_property_border_right_width: Symbol<extern fn(_: AzStyleBorderRightWidthValue) -> AzCssProperty>,
        az_css_property_border_left_width: Symbol<extern fn(_: AzStyleBorderLeftWidthValue) -> AzCssProperty>,
        az_css_property_border_bottom_width: Symbol<extern fn(_: AzStyleBorderBottomWidthValue) -> AzCssProperty>,
        az_css_property_box_shadow_left: Symbol<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>,
        az_css_property_box_shadow_right: Symbol<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>,
        az_css_property_box_shadow_top: Symbol<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>,
        az_css_property_box_shadow_bottom: Symbol<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>,
        az_css_property_delete: Symbol<extern fn(_: &mut AzCssProperty)>,
        az_css_property_deep_copy: Symbol<extern fn(_: &AzCssProperty) -> AzCssProperty>,
        az_dom_div: Symbol<extern fn() -> AzDomPtr>,
        az_dom_body: Symbol<extern fn() -> AzDomPtr>,
        az_dom_label: Symbol<extern fn(_: AzString) -> AzDomPtr>,
        az_dom_text: Symbol<extern fn(_: AzTextId) -> AzDomPtr>,
        az_dom_image: Symbol<extern fn(_: AzImageId) -> AzDomPtr>,
        az_dom_gl_texture: Symbol<extern fn(_: AzGlCallback) -> AzDomPtr>,
        az_dom_iframe_callback: Symbol<extern fn(_: AzIFrameCallback) -> AzDomPtr>,
        az_dom_add_id: Symbol<extern fn(_: AzString)>,
        az_dom_with_id: Symbol<extern fn(_: AzString) -> AzDomPtr>,
        az_dom_set_ids: Symbol<extern fn(_: AzStringVec)>,
        az_dom_with_ids: Symbol<extern fn(_: AzStringVec) -> AzDomPtr>,
        az_dom_add_class: Symbol<extern fn(_: AzString)>,
        az_dom_with_class: Symbol<extern fn(_: AzString) -> AzDomPtr>,
        az_dom_set_classes: Symbol<extern fn(_: AzStringVec)>,
        az_dom_with_classes: Symbol<extern fn(_: AzStringVec) -> AzDomPtr>,
        az_dom_add_callback: Symbol<extern fn(_: AzCallback)>,
        az_dom_with_callback: Symbol<extern fn(_: AzCallback) -> AzDomPtr>,
        az_dom_add_css_override: Symbol<extern fn(_: AzCssProperty)>,
        az_dom_with_css_override: Symbol<extern fn(_: AzCssProperty) -> AzDomPtr>,
        az_dom_set_is_draggable: Symbol<extern fn(_: bool)>,
        az_dom_is_draggable: Symbol<extern fn(_: bool) -> AzDomPtr>,
        az_dom_set_tab_index: Symbol<extern fn(_: AzTabIndex)>,
        az_dom_with_tab_index: Symbol<extern fn(_: AzTabIndex) -> AzDomPtr>,
        az_dom_add_child: Symbol<extern fn(_: AzDomPtr)>,
        az_dom_with_child: Symbol<extern fn(_: AzDomPtr) -> AzDomPtr>,
        az_dom_has_id: Symbol<extern fn(_: AzString) -> bool>,
        az_dom_has_class: Symbol<extern fn(_: AzString) -> bool>,
        az_dom_get_html_string: Symbol<extern fn(_: &mut AzDomPtr) -> AzString>,
        az_dom_delete: Symbol<extern fn(_: &mut AzDomPtr)>,
        az_dom_shallow_copy: Symbol<extern fn(_: &AzDomPtr) -> AzDomPtr>,
        az_event_filter_hover: Symbol<extern fn(_: AzHoverEventFilter) -> AzEventFilter>,
        az_event_filter_not: Symbol<extern fn(_: AzNotEventFilter) -> AzEventFilter>,
        az_event_filter_focus: Symbol<extern fn(_: AzFocusEventFilter) -> AzEventFilter>,
        az_event_filter_window: Symbol<extern fn(_: AzWindowEventFilter) -> AzEventFilter>,
        az_event_filter_delete: Symbol<extern fn(_: &mut AzEventFilter)>,
        az_event_filter_deep_copy: Symbol<extern fn(_: &AzEventFilter) -> AzEventFilter>,
        az_hover_event_filter_mouse_over: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_mouse_down: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_left_mouse_down: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_right_mouse_down: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_middle_mouse_down: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_mouse_up: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_left_mouse_up: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_right_mouse_up: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_middle_mouse_up: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_mouse_enter: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_mouse_leave: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_scroll: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_scroll_start: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_scroll_end: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_text_input: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_virtual_key_down: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_virtual_key_up: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_hovered_file: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_dropped_file: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_hovered_file_cancelled: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_delete: Symbol<extern fn(_: &mut AzHoverEventFilter)>,
        az_hover_event_filter_deep_copy: Symbol<extern fn(_: &AzHoverEventFilter) -> AzHoverEventFilter>,
        az_focus_event_filter_mouse_over: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_mouse_down: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_left_mouse_down: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_right_mouse_down: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_middle_mouse_down: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_mouse_up: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_left_mouse_up: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_right_mouse_up: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_middle_mouse_up: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_mouse_enter: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_mouse_leave: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_scroll: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_scroll_start: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_scroll_end: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_text_input: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_virtual_key_down: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_virtual_key_up: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_focus_received: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_focus_lost: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_delete: Symbol<extern fn(_: &mut AzFocusEventFilter)>,
        az_focus_event_filter_deep_copy: Symbol<extern fn(_: &AzFocusEventFilter) -> AzFocusEventFilter>,
        az_not_event_filter_hover: Symbol<extern fn(_: AzHoverEventFilter) -> AzNotEventFilter>,
        az_not_event_filter_focus: Symbol<extern fn(_: AzFocusEventFilter) -> AzNotEventFilter>,
        az_not_event_filter_delete: Symbol<extern fn(_: &mut AzNotEventFilter)>,
        az_not_event_filter_deep_copy: Symbol<extern fn(_: &AzNotEventFilter) -> AzNotEventFilter>,
        az_window_event_filter_mouse_over: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_mouse_down: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_left_mouse_down: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_right_mouse_down: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_middle_mouse_down: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_mouse_up: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_left_mouse_up: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_right_mouse_up: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_middle_mouse_up: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_mouse_enter: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_mouse_leave: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_scroll: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_scroll_start: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_scroll_end: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_text_input: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_virtual_key_down: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_virtual_key_up: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_hovered_file: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_dropped_file: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_hovered_file_cancelled: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_delete: Symbol<extern fn(_: &mut AzWindowEventFilter)>,
        az_window_event_filter_deep_copy: Symbol<extern fn(_: &AzWindowEventFilter) -> AzWindowEventFilter>,
        az_tab_index_auto: Symbol<extern fn() -> AzTabIndex>,
        az_tab_index_override_in_parent: Symbol<extern fn(_: usize) -> AzTabIndex>,
        az_tab_index_no_keyboard_focus: Symbol<extern fn() -> AzTabIndex>,
        az_tab_index_delete: Symbol<extern fn(_: &mut AzTabIndex)>,
        az_tab_index_deep_copy: Symbol<extern fn(_: &AzTabIndex) -> AzTabIndex>,
        az_text_id_new: Symbol<extern fn() -> AzTextId>,
        az_text_id_delete: Symbol<extern fn(_: &mut AzTextId)>,
        az_text_id_deep_copy: Symbol<extern fn(_: &AzTextId) -> AzTextId>,
        az_image_id_new: Symbol<extern fn() -> AzImageId>,
        az_image_id_delete: Symbol<extern fn(_: &mut AzImageId)>,
        az_image_id_deep_copy: Symbol<extern fn(_: &AzImageId) -> AzImageId>,
        az_font_id_new: Symbol<extern fn() -> AzFontId>,
        az_font_id_delete: Symbol<extern fn(_: &mut AzFontId)>,
        az_font_id_deep_copy: Symbol<extern fn(_: &AzFontId) -> AzFontId>,
        az_image_source_embedded: Symbol<extern fn(_: AzU8Vec) -> AzImageSource>,
        az_image_source_file: Symbol<extern fn(_: AzPathBufPtr) -> AzImageSource>,
        az_image_source_raw: Symbol<extern fn(_: AzRawImagePtr) -> AzImageSource>,
        az_image_source_delete: Symbol<extern fn(_: &mut AzImageSource)>,
        az_image_source_deep_copy: Symbol<extern fn(_: &AzImageSource) -> AzImageSource>,
        az_font_source_embedded: Symbol<extern fn(_: AzU8Vec) -> AzFontSource>,
        az_font_source_file: Symbol<extern fn(_: AzPathBufPtr) -> AzFontSource>,
        az_font_source_system: Symbol<extern fn(_: AzString) -> AzFontSource>,
        az_font_source_delete: Symbol<extern fn(_: &mut AzFontSource)>,
        az_font_source_deep_copy: Symbol<extern fn(_: &AzFontSource) -> AzFontSource>,
        az_raw_image_new: Symbol<extern fn(_: AzRawImageFormat) -> AzRawImagePtr>,
        az_raw_image_delete: Symbol<extern fn(_: &mut AzRawImagePtr)>,
        az_raw_image_shallow_copy: Symbol<extern fn(_: &AzRawImagePtr) -> AzRawImagePtr>,
        az_raw_image_format_r8: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_r16: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_rg16: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_bgra8: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_rgbaf32: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_rg8: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_rgbai32: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_rgba8: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_delete: Symbol<extern fn(_: &mut AzRawImageFormat)>,
        az_raw_image_format_deep_copy: Symbol<extern fn(_: &AzRawImageFormat) -> AzRawImageFormat>,
        az_window_create_options_new: Symbol<extern fn(_: AzCssPtr) -> AzWindowCreateOptionsPtr>,
        az_window_create_options_delete: Symbol<extern fn(_: &mut AzWindowCreateOptionsPtr)>,
        az_window_create_options_shallow_copy: Symbol<extern fn(_: &AzWindowCreateOptionsPtr) -> AzWindowCreateOptionsPtr>,
    }

    pub fn initialize_library(path: &str) -> Option<AzulDll> {
        let lib = Library::new(path).ok()?;
        let az_string_from_utf8_unchecked = unsafe { lib.get::<extern fn(_: usize) -> AzString>(b"az_string_from_utf8_unchecked").ok()? };
        let az_string_from_utf8_lossy = unsafe { lib.get::<extern fn(_: usize) -> AzString>(b"az_string_from_utf8_lossy").ok()? };
        let az_string_into_bytes = unsafe { lib.get::<extern fn(_: AzString) -> AzU8Vec>(b"az_string_into_bytes").ok()? };
        let az_string_delete = unsafe { lib.get::<extern fn(_: &mut AzString)>(b"az_string_delete").ok()? };
        let az_string_deep_copy = unsafe { lib.get::<extern fn(_: &AzString) -> AzString>(b"az_string_deep_copy").ok()? };
        let az_u8_vec_copy_from = unsafe { lib.get::<extern fn(_: usize) -> AzU8Vec>(b"az_u8_vec_copy_from").ok()? };
        let az_u8_vec_as_ptr = unsafe { lib.get::<extern fn(_: &AzU8Vec) -> *const u8>(b"az_u8_vec_as_ptr").ok()? };
        let az_u8_vec_len = unsafe { lib.get::<extern fn(_: &AzU8Vec) -> usize>(b"az_u8_vec_len").ok()? };
        let az_u8_vec_delete = unsafe { lib.get::<extern fn(_: &mut AzU8Vec)>(b"az_u8_vec_delete").ok()? };
        let az_u8_vec_deep_copy = unsafe { lib.get::<extern fn(_: &AzU8Vec) -> AzU8Vec>(b"az_u8_vec_deep_copy").ok()? };
        let az_string_vec_copy_from = unsafe { lib.get::<extern fn(_: usize) -> AzStringVec>(b"az_string_vec_copy_from").ok()? };
        let az_string_vec_delete = unsafe { lib.get::<extern fn(_: &mut AzStringVec)>(b"az_string_vec_delete").ok()? };
        let az_string_vec_deep_copy = unsafe { lib.get::<extern fn(_: &AzStringVec) -> AzStringVec>(b"az_string_vec_deep_copy").ok()? };
        let az_path_buf_new = unsafe { lib.get::<extern fn(_: AzString) -> AzPathBufPtr>(b"az_path_buf_new").ok()? };
        let az_path_buf_delete = unsafe { lib.get::<extern fn(_: &mut AzPathBufPtr)>(b"az_path_buf_delete").ok()? };
        let az_path_buf_shallow_copy = unsafe { lib.get::<extern fn(_: &AzPathBufPtr) -> AzPathBufPtr>(b"az_path_buf_shallow_copy").ok()? };
        let az_app_config_default = unsafe { lib.get::<extern fn() -> AzAppConfigPtr>(b"az_app_config_default").ok()? };
        let az_app_config_delete = unsafe { lib.get::<extern fn(_: &mut AzAppConfigPtr)>(b"az_app_config_delete").ok()? };
        let az_app_config_shallow_copy = unsafe { lib.get::<extern fn(_: &AzAppConfigPtr) -> AzAppConfigPtr>(b"az_app_config_shallow_copy").ok()? };
        let az_app_new = unsafe { lib.get::<extern fn(_: AzLayoutCallback) -> AzAppPtr>(b"az_app_new").ok()? };
        let az_app_run = unsafe { lib.get::<extern fn(_: AzWindowCreateOptionsPtr)>(b"az_app_run").ok()? };
        let az_app_delete = unsafe { lib.get::<extern fn(_: &mut AzAppPtr)>(b"az_app_delete").ok()? };
        let az_app_shallow_copy = unsafe { lib.get::<extern fn(_: &AzAppPtr) -> AzAppPtr>(b"az_app_shallow_copy").ok()? };
        let az_callback_info_delete = unsafe { lib.get::<extern fn(_: &mut AzCallbackInfoPtr)>(b"az_callback_info_delete").ok()? };
        let az_callback_info_shallow_copy = unsafe { lib.get::<extern fn(_: &AzCallbackInfoPtr) -> AzCallbackInfoPtr>(b"az_callback_info_shallow_copy").ok()? };
        let az_i_frame_callback_info_delete = unsafe { lib.get::<extern fn(_: &mut AzIFrameCallbackInfoPtr)>(b"az_i_frame_callback_info_delete").ok()? };
        let az_i_frame_callback_info_shallow_copy = unsafe { lib.get::<extern fn(_: &AzIFrameCallbackInfoPtr) -> AzIFrameCallbackInfoPtr>(b"az_i_frame_callback_info_shallow_copy").ok()? };
        let az_i_frame_callback_return_delete = unsafe { lib.get::<extern fn(_: &mut AzIFrameCallbackReturnPtr)>(b"az_i_frame_callback_return_delete").ok()? };
        let az_i_frame_callback_return_shallow_copy = unsafe { lib.get::<extern fn(_: &AzIFrameCallbackReturnPtr) -> AzIFrameCallbackReturnPtr>(b"az_i_frame_callback_return_shallow_copy").ok()? };
        let az_gl_callback_info_delete = unsafe { lib.get::<extern fn(_: &mut AzGlCallbackInfoPtr)>(b"az_gl_callback_info_delete").ok()? };
        let az_gl_callback_info_shallow_copy = unsafe { lib.get::<extern fn(_: &AzGlCallbackInfoPtr) -> AzGlCallbackInfoPtr>(b"az_gl_callback_info_shallow_copy").ok()? };
        let az_gl_callback_return_delete = unsafe { lib.get::<extern fn(_: &mut AzGlCallbackReturnPtr)>(b"az_gl_callback_return_delete").ok()? };
        let az_gl_callback_return_shallow_copy = unsafe { lib.get::<extern fn(_: &AzGlCallbackReturnPtr) -> AzGlCallbackReturnPtr>(b"az_gl_callback_return_shallow_copy").ok()? };
        let az_layout_info_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutInfoPtr)>(b"az_layout_info_delete").ok()? };
        let az_layout_info_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutInfoPtr) -> AzLayoutInfoPtr>(b"az_layout_info_shallow_copy").ok()? };
        let az_css_native = unsafe { lib.get::<extern fn() -> AzCssPtr>(b"az_css_native").ok()? };
        let az_css_empty = unsafe { lib.get::<extern fn() -> AzCssPtr>(b"az_css_empty").ok()? };
        let az_css_delete = unsafe { lib.get::<extern fn(_: &mut AzCssPtr)>(b"az_css_delete").ok()? };
        let az_css_shallow_copy = unsafe { lib.get::<extern fn(_: &AzCssPtr) -> AzCssPtr>(b"az_css_shallow_copy").ok()? };
        let az_box_shadow_pre_display_item_delete = unsafe { lib.get::<extern fn(_: &mut AzBoxShadowPreDisplayItemPtr)>(b"az_box_shadow_pre_display_item_delete").ok()? };
        let az_box_shadow_pre_display_item_shallow_copy = unsafe { lib.get::<extern fn(_: &AzBoxShadowPreDisplayItemPtr) -> AzBoxShadowPreDisplayItemPtr>(b"az_box_shadow_pre_display_item_shallow_copy").ok()? };
        let az_layout_align_content_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutAlignContentPtr)>(b"az_layout_align_content_delete").ok()? };
        let az_layout_align_content_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutAlignContentPtr) -> AzLayoutAlignContentPtr>(b"az_layout_align_content_shallow_copy").ok()? };
        let az_layout_align_items_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutAlignItemsPtr)>(b"az_layout_align_items_delete").ok()? };
        let az_layout_align_items_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutAlignItemsPtr) -> AzLayoutAlignItemsPtr>(b"az_layout_align_items_shallow_copy").ok()? };
        let az_layout_bottom_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutBottomPtr)>(b"az_layout_bottom_delete").ok()? };
        let az_layout_bottom_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutBottomPtr) -> AzLayoutBottomPtr>(b"az_layout_bottom_shallow_copy").ok()? };
        let az_layout_box_sizing_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutBoxSizingPtr)>(b"az_layout_box_sizing_delete").ok()? };
        let az_layout_box_sizing_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutBoxSizingPtr) -> AzLayoutBoxSizingPtr>(b"az_layout_box_sizing_shallow_copy").ok()? };
        let az_layout_direction_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutDirectionPtr)>(b"az_layout_direction_delete").ok()? };
        let az_layout_direction_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutDirectionPtr) -> AzLayoutDirectionPtr>(b"az_layout_direction_shallow_copy").ok()? };
        let az_layout_display_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutDisplayPtr)>(b"az_layout_display_delete").ok()? };
        let az_layout_display_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutDisplayPtr) -> AzLayoutDisplayPtr>(b"az_layout_display_shallow_copy").ok()? };
        let az_layout_flex_grow_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFlexGrowPtr)>(b"az_layout_flex_grow_delete").ok()? };
        let az_layout_flex_grow_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFlexGrowPtr) -> AzLayoutFlexGrowPtr>(b"az_layout_flex_grow_shallow_copy").ok()? };
        let az_layout_flex_shrink_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFlexShrinkPtr)>(b"az_layout_flex_shrink_delete").ok()? };
        let az_layout_flex_shrink_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFlexShrinkPtr) -> AzLayoutFlexShrinkPtr>(b"az_layout_flex_shrink_shallow_copy").ok()? };
        let az_layout_float_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFloatPtr)>(b"az_layout_float_delete").ok()? };
        let az_layout_float_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFloatPtr) -> AzLayoutFloatPtr>(b"az_layout_float_shallow_copy").ok()? };
        let az_layout_height_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutHeightPtr)>(b"az_layout_height_delete").ok()? };
        let az_layout_height_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutHeightPtr) -> AzLayoutHeightPtr>(b"az_layout_height_shallow_copy").ok()? };
        let az_layout_justify_content_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutJustifyContentPtr)>(b"az_layout_justify_content_delete").ok()? };
        let az_layout_justify_content_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutJustifyContentPtr) -> AzLayoutJustifyContentPtr>(b"az_layout_justify_content_shallow_copy").ok()? };
        let az_layout_left_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutLeftPtr)>(b"az_layout_left_delete").ok()? };
        let az_layout_left_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutLeftPtr) -> AzLayoutLeftPtr>(b"az_layout_left_shallow_copy").ok()? };
        let az_layout_margin_bottom_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginBottomPtr)>(b"az_layout_margin_bottom_delete").ok()? };
        let az_layout_margin_bottom_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginBottomPtr) -> AzLayoutMarginBottomPtr>(b"az_layout_margin_bottom_shallow_copy").ok()? };
        let az_layout_margin_left_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginLeftPtr)>(b"az_layout_margin_left_delete").ok()? };
        let az_layout_margin_left_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginLeftPtr) -> AzLayoutMarginLeftPtr>(b"az_layout_margin_left_shallow_copy").ok()? };
        let az_layout_margin_right_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginRightPtr)>(b"az_layout_margin_right_delete").ok()? };
        let az_layout_margin_right_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginRightPtr) -> AzLayoutMarginRightPtr>(b"az_layout_margin_right_shallow_copy").ok()? };
        let az_layout_margin_top_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginTopPtr)>(b"az_layout_margin_top_delete").ok()? };
        let az_layout_margin_top_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginTopPtr) -> AzLayoutMarginTopPtr>(b"az_layout_margin_top_shallow_copy").ok()? };
        let az_layout_max_height_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMaxHeightPtr)>(b"az_layout_max_height_delete").ok()? };
        let az_layout_max_height_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMaxHeightPtr) -> AzLayoutMaxHeightPtr>(b"az_layout_max_height_shallow_copy").ok()? };
        let az_layout_max_width_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMaxWidthPtr)>(b"az_layout_max_width_delete").ok()? };
        let az_layout_max_width_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMaxWidthPtr) -> AzLayoutMaxWidthPtr>(b"az_layout_max_width_shallow_copy").ok()? };
        let az_layout_min_height_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMinHeightPtr)>(b"az_layout_min_height_delete").ok()? };
        let az_layout_min_height_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMinHeightPtr) -> AzLayoutMinHeightPtr>(b"az_layout_min_height_shallow_copy").ok()? };
        let az_layout_min_width_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMinWidthPtr)>(b"az_layout_min_width_delete").ok()? };
        let az_layout_min_width_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMinWidthPtr) -> AzLayoutMinWidthPtr>(b"az_layout_min_width_shallow_copy").ok()? };
        let az_layout_padding_bottom_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingBottomPtr)>(b"az_layout_padding_bottom_delete").ok()? };
        let az_layout_padding_bottom_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingBottomPtr) -> AzLayoutPaddingBottomPtr>(b"az_layout_padding_bottom_shallow_copy").ok()? };
        let az_layout_padding_left_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingLeftPtr)>(b"az_layout_padding_left_delete").ok()? };
        let az_layout_padding_left_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingLeftPtr) -> AzLayoutPaddingLeftPtr>(b"az_layout_padding_left_shallow_copy").ok()? };
        let az_layout_padding_right_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingRightPtr)>(b"az_layout_padding_right_delete").ok()? };
        let az_layout_padding_right_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingRightPtr) -> AzLayoutPaddingRightPtr>(b"az_layout_padding_right_shallow_copy").ok()? };
        let az_layout_padding_top_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingTopPtr)>(b"az_layout_padding_top_delete").ok()? };
        let az_layout_padding_top_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingTopPtr) -> AzLayoutPaddingTopPtr>(b"az_layout_padding_top_shallow_copy").ok()? };
        let az_layout_position_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPositionPtr)>(b"az_layout_position_delete").ok()? };
        let az_layout_position_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPositionPtr) -> AzLayoutPositionPtr>(b"az_layout_position_shallow_copy").ok()? };
        let az_layout_right_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutRightPtr)>(b"az_layout_right_delete").ok()? };
        let az_layout_right_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutRightPtr) -> AzLayoutRightPtr>(b"az_layout_right_shallow_copy").ok()? };
        let az_layout_top_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutTopPtr)>(b"az_layout_top_delete").ok()? };
        let az_layout_top_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutTopPtr) -> AzLayoutTopPtr>(b"az_layout_top_shallow_copy").ok()? };
        let az_layout_width_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutWidthPtr)>(b"az_layout_width_delete").ok()? };
        let az_layout_width_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutWidthPtr) -> AzLayoutWidthPtr>(b"az_layout_width_shallow_copy").ok()? };
        let az_layout_wrap_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutWrapPtr)>(b"az_layout_wrap_delete").ok()? };
        let az_layout_wrap_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutWrapPtr) -> AzLayoutWrapPtr>(b"az_layout_wrap_shallow_copy").ok()? };
        let az_overflow_delete = unsafe { lib.get::<extern fn(_: &mut AzOverflowPtr)>(b"az_overflow_delete").ok()? };
        let az_overflow_shallow_copy = unsafe { lib.get::<extern fn(_: &AzOverflowPtr) -> AzOverflowPtr>(b"az_overflow_shallow_copy").ok()? };
        let az_style_background_content_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundContentPtr)>(b"az_style_background_content_delete").ok()? };
        let az_style_background_content_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundContentPtr) -> AzStyleBackgroundContentPtr>(b"az_style_background_content_shallow_copy").ok()? };
        let az_style_background_position_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundPositionPtr)>(b"az_style_background_position_delete").ok()? };
        let az_style_background_position_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundPositionPtr) -> AzStyleBackgroundPositionPtr>(b"az_style_background_position_shallow_copy").ok()? };
        let az_style_background_repeat_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundRepeatPtr)>(b"az_style_background_repeat_delete").ok()? };
        let az_style_background_repeat_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundRepeatPtr) -> AzStyleBackgroundRepeatPtr>(b"az_style_background_repeat_shallow_copy").ok()? };
        let az_style_background_size_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundSizePtr)>(b"az_style_background_size_delete").ok()? };
        let az_style_background_size_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundSizePtr) -> AzStyleBackgroundSizePtr>(b"az_style_background_size_shallow_copy").ok()? };
        let az_style_border_bottom_color_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomColorPtr)>(b"az_style_border_bottom_color_delete").ok()? };
        let az_style_border_bottom_color_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomColorPtr) -> AzStyleBorderBottomColorPtr>(b"az_style_border_bottom_color_shallow_copy").ok()? };
        let az_style_border_bottom_left_radius_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomLeftRadiusPtr)>(b"az_style_border_bottom_left_radius_delete").ok()? };
        let az_style_border_bottom_left_radius_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomLeftRadiusPtr) -> AzStyleBorderBottomLeftRadiusPtr>(b"az_style_border_bottom_left_radius_shallow_copy").ok()? };
        let az_style_border_bottom_right_radius_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomRightRadiusPtr)>(b"az_style_border_bottom_right_radius_delete").ok()? };
        let az_style_border_bottom_right_radius_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomRightRadiusPtr) -> AzStyleBorderBottomRightRadiusPtr>(b"az_style_border_bottom_right_radius_shallow_copy").ok()? };
        let az_style_border_bottom_style_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomStylePtr)>(b"az_style_border_bottom_style_delete").ok()? };
        let az_style_border_bottom_style_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomStylePtr) -> AzStyleBorderBottomStylePtr>(b"az_style_border_bottom_style_shallow_copy").ok()? };
        let az_style_border_bottom_width_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomWidthPtr)>(b"az_style_border_bottom_width_delete").ok()? };
        let az_style_border_bottom_width_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomWidthPtr) -> AzStyleBorderBottomWidthPtr>(b"az_style_border_bottom_width_shallow_copy").ok()? };
        let az_style_border_left_color_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftColorPtr)>(b"az_style_border_left_color_delete").ok()? };
        let az_style_border_left_color_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftColorPtr) -> AzStyleBorderLeftColorPtr>(b"az_style_border_left_color_shallow_copy").ok()? };
        let az_style_border_left_style_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftStylePtr)>(b"az_style_border_left_style_delete").ok()? };
        let az_style_border_left_style_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftStylePtr) -> AzStyleBorderLeftStylePtr>(b"az_style_border_left_style_shallow_copy").ok()? };
        let az_style_border_left_width_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftWidthPtr)>(b"az_style_border_left_width_delete").ok()? };
        let az_style_border_left_width_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftWidthPtr) -> AzStyleBorderLeftWidthPtr>(b"az_style_border_left_width_shallow_copy").ok()? };
        let az_style_border_right_color_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightColorPtr)>(b"az_style_border_right_color_delete").ok()? };
        let az_style_border_right_color_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightColorPtr) -> AzStyleBorderRightColorPtr>(b"az_style_border_right_color_shallow_copy").ok()? };
        let az_style_border_right_style_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightStylePtr)>(b"az_style_border_right_style_delete").ok()? };
        let az_style_border_right_style_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightStylePtr) -> AzStyleBorderRightStylePtr>(b"az_style_border_right_style_shallow_copy").ok()? };
        let az_style_border_right_width_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightWidthPtr)>(b"az_style_border_right_width_delete").ok()? };
        let az_style_border_right_width_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightWidthPtr) -> AzStyleBorderRightWidthPtr>(b"az_style_border_right_width_shallow_copy").ok()? };
        let az_style_border_top_color_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopColorPtr)>(b"az_style_border_top_color_delete").ok()? };
        let az_style_border_top_color_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopColorPtr) -> AzStyleBorderTopColorPtr>(b"az_style_border_top_color_shallow_copy").ok()? };
        let az_style_border_top_left_radius_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopLeftRadiusPtr)>(b"az_style_border_top_left_radius_delete").ok()? };
        let az_style_border_top_left_radius_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopLeftRadiusPtr) -> AzStyleBorderTopLeftRadiusPtr>(b"az_style_border_top_left_radius_shallow_copy").ok()? };
        let az_style_border_top_right_radius_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopRightRadiusPtr)>(b"az_style_border_top_right_radius_delete").ok()? };
        let az_style_border_top_right_radius_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopRightRadiusPtr) -> AzStyleBorderTopRightRadiusPtr>(b"az_style_border_top_right_radius_shallow_copy").ok()? };
        let az_style_border_top_style_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopStylePtr)>(b"az_style_border_top_style_delete").ok()? };
        let az_style_border_top_style_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopStylePtr) -> AzStyleBorderTopStylePtr>(b"az_style_border_top_style_shallow_copy").ok()? };
        let az_style_border_top_width_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopWidthPtr)>(b"az_style_border_top_width_delete").ok()? };
        let az_style_border_top_width_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopWidthPtr) -> AzStyleBorderTopWidthPtr>(b"az_style_border_top_width_shallow_copy").ok()? };
        let az_style_cursor_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleCursorPtr)>(b"az_style_cursor_delete").ok()? };
        let az_style_cursor_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleCursorPtr) -> AzStyleCursorPtr>(b"az_style_cursor_shallow_copy").ok()? };
        let az_style_font_family_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleFontFamilyPtr)>(b"az_style_font_family_delete").ok()? };
        let az_style_font_family_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleFontFamilyPtr) -> AzStyleFontFamilyPtr>(b"az_style_font_family_shallow_copy").ok()? };
        let az_style_font_size_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleFontSizePtr)>(b"az_style_font_size_delete").ok()? };
        let az_style_font_size_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleFontSizePtr) -> AzStyleFontSizePtr>(b"az_style_font_size_shallow_copy").ok()? };
        let az_style_letter_spacing_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleLetterSpacingPtr)>(b"az_style_letter_spacing_delete").ok()? };
        let az_style_letter_spacing_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleLetterSpacingPtr) -> AzStyleLetterSpacingPtr>(b"az_style_letter_spacing_shallow_copy").ok()? };
        let az_style_line_height_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleLineHeightPtr)>(b"az_style_line_height_delete").ok()? };
        let az_style_line_height_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleLineHeightPtr) -> AzStyleLineHeightPtr>(b"az_style_line_height_shallow_copy").ok()? };
        let az_style_tab_width_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTabWidthPtr)>(b"az_style_tab_width_delete").ok()? };
        let az_style_tab_width_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleTabWidthPtr) -> AzStyleTabWidthPtr>(b"az_style_tab_width_shallow_copy").ok()? };
        let az_style_text_alignment_horz_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTextAlignmentHorzPtr)>(b"az_style_text_alignment_horz_delete").ok()? };
        let az_style_text_alignment_horz_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleTextAlignmentHorzPtr) -> AzStyleTextAlignmentHorzPtr>(b"az_style_text_alignment_horz_shallow_copy").ok()? };
        let az_style_text_color_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTextColorPtr)>(b"az_style_text_color_delete").ok()? };
        let az_style_text_color_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleTextColorPtr) -> AzStyleTextColorPtr>(b"az_style_text_color_shallow_copy").ok()? };
        let az_style_word_spacing_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleWordSpacingPtr)>(b"az_style_word_spacing_delete").ok()? };
        let az_style_word_spacing_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleWordSpacingPtr) -> AzStyleWordSpacingPtr>(b"az_style_word_spacing_shallow_copy").ok()? };
        let az_box_shadow_pre_display_item_value_auto = unsafe { lib.get::<extern fn() -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_auto").ok()? };
        let az_box_shadow_pre_display_item_value_none = unsafe { lib.get::<extern fn() -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_none").ok()? };
        let az_box_shadow_pre_display_item_value_inherit = unsafe { lib.get::<extern fn() -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_inherit").ok()? };
        let az_box_shadow_pre_display_item_value_initial = unsafe { lib.get::<extern fn() -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_initial").ok()? };
        let az_box_shadow_pre_display_item_value_exact = unsafe { lib.get::<extern fn(_: AzBoxShadowPreDisplayItemPtr) -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_exact").ok()? };
        let az_box_shadow_pre_display_item_value_delete = unsafe { lib.get::<extern fn(_: &mut AzBoxShadowPreDisplayItemValue)>(b"az_box_shadow_pre_display_item_value_delete").ok()? };
        let az_box_shadow_pre_display_item_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzBoxShadowPreDisplayItemValue) -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_deep_copy").ok()? };
        let az_layout_align_content_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_auto").ok()? };
        let az_layout_align_content_value_none = unsafe { lib.get::<extern fn() -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_none").ok()? };
        let az_layout_align_content_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_inherit").ok()? };
        let az_layout_align_content_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_initial").ok()? };
        let az_layout_align_content_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutAlignContentPtr) -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_exact").ok()? };
        let az_layout_align_content_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutAlignContentValue)>(b"az_layout_align_content_value_delete").ok()? };
        let az_layout_align_content_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutAlignContentValue) -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_deep_copy").ok()? };
        let az_layout_align_items_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_auto").ok()? };
        let az_layout_align_items_value_none = unsafe { lib.get::<extern fn() -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_none").ok()? };
        let az_layout_align_items_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_inherit").ok()? };
        let az_layout_align_items_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_initial").ok()? };
        let az_layout_align_items_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutAlignItemsPtr) -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_exact").ok()? };
        let az_layout_align_items_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutAlignItemsValue)>(b"az_layout_align_items_value_delete").ok()? };
        let az_layout_align_items_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutAlignItemsValue) -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_deep_copy").ok()? };
        let az_layout_bottom_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutBottomValue>(b"az_layout_bottom_value_auto").ok()? };
        let az_layout_bottom_value_none = unsafe { lib.get::<extern fn() -> AzLayoutBottomValue>(b"az_layout_bottom_value_none").ok()? };
        let az_layout_bottom_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutBottomValue>(b"az_layout_bottom_value_inherit").ok()? };
        let az_layout_bottom_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutBottomValue>(b"az_layout_bottom_value_initial").ok()? };
        let az_layout_bottom_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutBottomPtr) -> AzLayoutBottomValue>(b"az_layout_bottom_value_exact").ok()? };
        let az_layout_bottom_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutBottomValue)>(b"az_layout_bottom_value_delete").ok()? };
        let az_layout_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutBottomValue) -> AzLayoutBottomValue>(b"az_layout_bottom_value_deep_copy").ok()? };
        let az_layout_box_sizing_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_auto").ok()? };
        let az_layout_box_sizing_value_none = unsafe { lib.get::<extern fn() -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_none").ok()? };
        let az_layout_box_sizing_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_inherit").ok()? };
        let az_layout_box_sizing_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_initial").ok()? };
        let az_layout_box_sizing_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutBoxSizingPtr) -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_exact").ok()? };
        let az_layout_box_sizing_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutBoxSizingValue)>(b"az_layout_box_sizing_value_delete").ok()? };
        let az_layout_box_sizing_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutBoxSizingValue) -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_deep_copy").ok()? };
        let az_layout_direction_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutDirectionValue>(b"az_layout_direction_value_auto").ok()? };
        let az_layout_direction_value_none = unsafe { lib.get::<extern fn() -> AzLayoutDirectionValue>(b"az_layout_direction_value_none").ok()? };
        let az_layout_direction_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutDirectionValue>(b"az_layout_direction_value_inherit").ok()? };
        let az_layout_direction_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutDirectionValue>(b"az_layout_direction_value_initial").ok()? };
        let az_layout_direction_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutDirectionPtr) -> AzLayoutDirectionValue>(b"az_layout_direction_value_exact").ok()? };
        let az_layout_direction_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutDirectionValue)>(b"az_layout_direction_value_delete").ok()? };
        let az_layout_direction_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutDirectionValue) -> AzLayoutDirectionValue>(b"az_layout_direction_value_deep_copy").ok()? };
        let az_layout_display_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutDisplayValue>(b"az_layout_display_value_auto").ok()? };
        let az_layout_display_value_none = unsafe { lib.get::<extern fn() -> AzLayoutDisplayValue>(b"az_layout_display_value_none").ok()? };
        let az_layout_display_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutDisplayValue>(b"az_layout_display_value_inherit").ok()? };
        let az_layout_display_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutDisplayValue>(b"az_layout_display_value_initial").ok()? };
        let az_layout_display_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutDisplayPtr) -> AzLayoutDisplayValue>(b"az_layout_display_value_exact").ok()? };
        let az_layout_display_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutDisplayValue)>(b"az_layout_display_value_delete").ok()? };
        let az_layout_display_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutDisplayValue) -> AzLayoutDisplayValue>(b"az_layout_display_value_deep_copy").ok()? };
        let az_layout_flex_grow_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_auto").ok()? };
        let az_layout_flex_grow_value_none = unsafe { lib.get::<extern fn() -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_none").ok()? };
        let az_layout_flex_grow_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_inherit").ok()? };
        let az_layout_flex_grow_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_initial").ok()? };
        let az_layout_flex_grow_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutFlexGrowPtr) -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_exact").ok()? };
        let az_layout_flex_grow_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFlexGrowValue)>(b"az_layout_flex_grow_value_delete").ok()? };
        let az_layout_flex_grow_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFlexGrowValue) -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_deep_copy").ok()? };
        let az_layout_flex_shrink_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_auto").ok()? };
        let az_layout_flex_shrink_value_none = unsafe { lib.get::<extern fn() -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_none").ok()? };
        let az_layout_flex_shrink_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_inherit").ok()? };
        let az_layout_flex_shrink_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_initial").ok()? };
        let az_layout_flex_shrink_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutFlexShrinkPtr) -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_exact").ok()? };
        let az_layout_flex_shrink_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFlexShrinkValue)>(b"az_layout_flex_shrink_value_delete").ok()? };
        let az_layout_flex_shrink_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFlexShrinkValue) -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_deep_copy").ok()? };
        let az_layout_float_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutFloatValue>(b"az_layout_float_value_auto").ok()? };
        let az_layout_float_value_none = unsafe { lib.get::<extern fn() -> AzLayoutFloatValue>(b"az_layout_float_value_none").ok()? };
        let az_layout_float_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutFloatValue>(b"az_layout_float_value_inherit").ok()? };
        let az_layout_float_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutFloatValue>(b"az_layout_float_value_initial").ok()? };
        let az_layout_float_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutFloatPtr) -> AzLayoutFloatValue>(b"az_layout_float_value_exact").ok()? };
        let az_layout_float_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFloatValue)>(b"az_layout_float_value_delete").ok()? };
        let az_layout_float_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFloatValue) -> AzLayoutFloatValue>(b"az_layout_float_value_deep_copy").ok()? };
        let az_layout_height_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutHeightValue>(b"az_layout_height_value_auto").ok()? };
        let az_layout_height_value_none = unsafe { lib.get::<extern fn() -> AzLayoutHeightValue>(b"az_layout_height_value_none").ok()? };
        let az_layout_height_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutHeightValue>(b"az_layout_height_value_inherit").ok()? };
        let az_layout_height_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutHeightValue>(b"az_layout_height_value_initial").ok()? };
        let az_layout_height_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutHeightPtr) -> AzLayoutHeightValue>(b"az_layout_height_value_exact").ok()? };
        let az_layout_height_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutHeightValue)>(b"az_layout_height_value_delete").ok()? };
        let az_layout_height_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutHeightValue) -> AzLayoutHeightValue>(b"az_layout_height_value_deep_copy").ok()? };
        let az_layout_justify_content_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_auto").ok()? };
        let az_layout_justify_content_value_none = unsafe { lib.get::<extern fn() -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_none").ok()? };
        let az_layout_justify_content_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_inherit").ok()? };
        let az_layout_justify_content_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_initial").ok()? };
        let az_layout_justify_content_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutJustifyContentPtr) -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_exact").ok()? };
        let az_layout_justify_content_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutJustifyContentValue)>(b"az_layout_justify_content_value_delete").ok()? };
        let az_layout_justify_content_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutJustifyContentValue) -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_deep_copy").ok()? };
        let az_layout_left_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutLeftValue>(b"az_layout_left_value_auto").ok()? };
        let az_layout_left_value_none = unsafe { lib.get::<extern fn() -> AzLayoutLeftValue>(b"az_layout_left_value_none").ok()? };
        let az_layout_left_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutLeftValue>(b"az_layout_left_value_inherit").ok()? };
        let az_layout_left_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutLeftValue>(b"az_layout_left_value_initial").ok()? };
        let az_layout_left_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutLeftPtr) -> AzLayoutLeftValue>(b"az_layout_left_value_exact").ok()? };
        let az_layout_left_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutLeftValue)>(b"az_layout_left_value_delete").ok()? };
        let az_layout_left_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutLeftValue) -> AzLayoutLeftValue>(b"az_layout_left_value_deep_copy").ok()? };
        let az_layout_margin_bottom_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_auto").ok()? };
        let az_layout_margin_bottom_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_none").ok()? };
        let az_layout_margin_bottom_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_inherit").ok()? };
        let az_layout_margin_bottom_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_initial").ok()? };
        let az_layout_margin_bottom_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMarginBottomPtr) -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_exact").ok()? };
        let az_layout_margin_bottom_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginBottomValue)>(b"az_layout_margin_bottom_value_delete").ok()? };
        let az_layout_margin_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginBottomValue) -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_deep_copy").ok()? };
        let az_layout_margin_left_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_auto").ok()? };
        let az_layout_margin_left_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_none").ok()? };
        let az_layout_margin_left_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_inherit").ok()? };
        let az_layout_margin_left_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_initial").ok()? };
        let az_layout_margin_left_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMarginLeftPtr) -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_exact").ok()? };
        let az_layout_margin_left_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginLeftValue)>(b"az_layout_margin_left_value_delete").ok()? };
        let az_layout_margin_left_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginLeftValue) -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_deep_copy").ok()? };
        let az_layout_margin_right_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_auto").ok()? };
        let az_layout_margin_right_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_none").ok()? };
        let az_layout_margin_right_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_inherit").ok()? };
        let az_layout_margin_right_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_initial").ok()? };
        let az_layout_margin_right_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMarginRightPtr) -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_exact").ok()? };
        let az_layout_margin_right_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginRightValue)>(b"az_layout_margin_right_value_delete").ok()? };
        let az_layout_margin_right_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginRightValue) -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_deep_copy").ok()? };
        let az_layout_margin_top_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_auto").ok()? };
        let az_layout_margin_top_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_none").ok()? };
        let az_layout_margin_top_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_inherit").ok()? };
        let az_layout_margin_top_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_initial").ok()? };
        let az_layout_margin_top_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMarginTopPtr) -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_exact").ok()? };
        let az_layout_margin_top_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginTopValue)>(b"az_layout_margin_top_value_delete").ok()? };
        let az_layout_margin_top_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginTopValue) -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_deep_copy").ok()? };
        let az_layout_max_height_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_auto").ok()? };
        let az_layout_max_height_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_none").ok()? };
        let az_layout_max_height_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_inherit").ok()? };
        let az_layout_max_height_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_initial").ok()? };
        let az_layout_max_height_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMaxHeightPtr) -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_exact").ok()? };
        let az_layout_max_height_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMaxHeightValue)>(b"az_layout_max_height_value_delete").ok()? };
        let az_layout_max_height_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMaxHeightValue) -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_deep_copy").ok()? };
        let az_layout_max_width_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_auto").ok()? };
        let az_layout_max_width_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_none").ok()? };
        let az_layout_max_width_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_inherit").ok()? };
        let az_layout_max_width_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_initial").ok()? };
        let az_layout_max_width_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMaxWidthPtr) -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_exact").ok()? };
        let az_layout_max_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMaxWidthValue)>(b"az_layout_max_width_value_delete").ok()? };
        let az_layout_max_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMaxWidthValue) -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_deep_copy").ok()? };
        let az_layout_min_height_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_auto").ok()? };
        let az_layout_min_height_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_none").ok()? };
        let az_layout_min_height_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_inherit").ok()? };
        let az_layout_min_height_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_initial").ok()? };
        let az_layout_min_height_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMinHeightPtr) -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_exact").ok()? };
        let az_layout_min_height_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMinHeightValue)>(b"az_layout_min_height_value_delete").ok()? };
        let az_layout_min_height_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMinHeightValue) -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_deep_copy").ok()? };
        let az_layout_min_width_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_auto").ok()? };
        let az_layout_min_width_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_none").ok()? };
        let az_layout_min_width_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_inherit").ok()? };
        let az_layout_min_width_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_initial").ok()? };
        let az_layout_min_width_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMinWidthPtr) -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_exact").ok()? };
        let az_layout_min_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMinWidthValue)>(b"az_layout_min_width_value_delete").ok()? };
        let az_layout_min_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMinWidthValue) -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_deep_copy").ok()? };
        let az_layout_padding_bottom_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_auto").ok()? };
        let az_layout_padding_bottom_value_none = unsafe { lib.get::<extern fn() -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_none").ok()? };
        let az_layout_padding_bottom_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_inherit").ok()? };
        let az_layout_padding_bottom_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_initial").ok()? };
        let az_layout_padding_bottom_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutPaddingBottomPtr) -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_exact").ok()? };
        let az_layout_padding_bottom_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingBottomValue)>(b"az_layout_padding_bottom_value_delete").ok()? };
        let az_layout_padding_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingBottomValue) -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_deep_copy").ok()? };
        let az_layout_padding_left_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_auto").ok()? };
        let az_layout_padding_left_value_none = unsafe { lib.get::<extern fn() -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_none").ok()? };
        let az_layout_padding_left_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_inherit").ok()? };
        let az_layout_padding_left_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_initial").ok()? };
        let az_layout_padding_left_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutPaddingLeftPtr) -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_exact").ok()? };
        let az_layout_padding_left_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingLeftValue)>(b"az_layout_padding_left_value_delete").ok()? };
        let az_layout_padding_left_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingLeftValue) -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_deep_copy").ok()? };
        let az_layout_padding_right_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_auto").ok()? };
        let az_layout_padding_right_value_none = unsafe { lib.get::<extern fn() -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_none").ok()? };
        let az_layout_padding_right_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_inherit").ok()? };
        let az_layout_padding_right_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_initial").ok()? };
        let az_layout_padding_right_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutPaddingRightPtr) -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_exact").ok()? };
        let az_layout_padding_right_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingRightValue)>(b"az_layout_padding_right_value_delete").ok()? };
        let az_layout_padding_right_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingRightValue) -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_deep_copy").ok()? };
        let az_layout_padding_top_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_auto").ok()? };
        let az_layout_padding_top_value_none = unsafe { lib.get::<extern fn() -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_none").ok()? };
        let az_layout_padding_top_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_inherit").ok()? };
        let az_layout_padding_top_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_initial").ok()? };
        let az_layout_padding_top_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutPaddingTopPtr) -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_exact").ok()? };
        let az_layout_padding_top_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingTopValue)>(b"az_layout_padding_top_value_delete").ok()? };
        let az_layout_padding_top_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingTopValue) -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_deep_copy").ok()? };
        let az_layout_position_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutPositionValue>(b"az_layout_position_value_auto").ok()? };
        let az_layout_position_value_none = unsafe { lib.get::<extern fn() -> AzLayoutPositionValue>(b"az_layout_position_value_none").ok()? };
        let az_layout_position_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutPositionValue>(b"az_layout_position_value_inherit").ok()? };
        let az_layout_position_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutPositionValue>(b"az_layout_position_value_initial").ok()? };
        let az_layout_position_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutPositionPtr) -> AzLayoutPositionValue>(b"az_layout_position_value_exact").ok()? };
        let az_layout_position_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPositionValue)>(b"az_layout_position_value_delete").ok()? };
        let az_layout_position_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPositionValue) -> AzLayoutPositionValue>(b"az_layout_position_value_deep_copy").ok()? };
        let az_layout_right_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutRightValue>(b"az_layout_right_value_auto").ok()? };
        let az_layout_right_value_none = unsafe { lib.get::<extern fn() -> AzLayoutRightValue>(b"az_layout_right_value_none").ok()? };
        let az_layout_right_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutRightValue>(b"az_layout_right_value_inherit").ok()? };
        let az_layout_right_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutRightValue>(b"az_layout_right_value_initial").ok()? };
        let az_layout_right_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutRightPtr) -> AzLayoutRightValue>(b"az_layout_right_value_exact").ok()? };
        let az_layout_right_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutRightValue)>(b"az_layout_right_value_delete").ok()? };
        let az_layout_right_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutRightValue) -> AzLayoutRightValue>(b"az_layout_right_value_deep_copy").ok()? };
        let az_layout_top_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutTopValue>(b"az_layout_top_value_auto").ok()? };
        let az_layout_top_value_none = unsafe { lib.get::<extern fn() -> AzLayoutTopValue>(b"az_layout_top_value_none").ok()? };
        let az_layout_top_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutTopValue>(b"az_layout_top_value_inherit").ok()? };
        let az_layout_top_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutTopValue>(b"az_layout_top_value_initial").ok()? };
        let az_layout_top_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutTopPtr) -> AzLayoutTopValue>(b"az_layout_top_value_exact").ok()? };
        let az_layout_top_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutTopValue)>(b"az_layout_top_value_delete").ok()? };
        let az_layout_top_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutTopValue) -> AzLayoutTopValue>(b"az_layout_top_value_deep_copy").ok()? };
        let az_layout_width_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutWidthValue>(b"az_layout_width_value_auto").ok()? };
        let az_layout_width_value_none = unsafe { lib.get::<extern fn() -> AzLayoutWidthValue>(b"az_layout_width_value_none").ok()? };
        let az_layout_width_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutWidthValue>(b"az_layout_width_value_inherit").ok()? };
        let az_layout_width_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutWidthValue>(b"az_layout_width_value_initial").ok()? };
        let az_layout_width_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutWidthPtr) -> AzLayoutWidthValue>(b"az_layout_width_value_exact").ok()? };
        let az_layout_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutWidthValue)>(b"az_layout_width_value_delete").ok()? };
        let az_layout_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutWidthValue) -> AzLayoutWidthValue>(b"az_layout_width_value_deep_copy").ok()? };
        let az_layout_wrap_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutWrapValue>(b"az_layout_wrap_value_auto").ok()? };
        let az_layout_wrap_value_none = unsafe { lib.get::<extern fn() -> AzLayoutWrapValue>(b"az_layout_wrap_value_none").ok()? };
        let az_layout_wrap_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutWrapValue>(b"az_layout_wrap_value_inherit").ok()? };
        let az_layout_wrap_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutWrapValue>(b"az_layout_wrap_value_initial").ok()? };
        let az_layout_wrap_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutWrapPtr) -> AzLayoutWrapValue>(b"az_layout_wrap_value_exact").ok()? };
        let az_layout_wrap_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutWrapValue)>(b"az_layout_wrap_value_delete").ok()? };
        let az_layout_wrap_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutWrapValue) -> AzLayoutWrapValue>(b"az_layout_wrap_value_deep_copy").ok()? };
        let az_overflow_value_auto = unsafe { lib.get::<extern fn() -> AzOverflowValue>(b"az_overflow_value_auto").ok()? };
        let az_overflow_value_none = unsafe { lib.get::<extern fn() -> AzOverflowValue>(b"az_overflow_value_none").ok()? };
        let az_overflow_value_inherit = unsafe { lib.get::<extern fn() -> AzOverflowValue>(b"az_overflow_value_inherit").ok()? };
        let az_overflow_value_initial = unsafe { lib.get::<extern fn() -> AzOverflowValue>(b"az_overflow_value_initial").ok()? };
        let az_overflow_value_exact = unsafe { lib.get::<extern fn(_: AzOverflowPtr) -> AzOverflowValue>(b"az_overflow_value_exact").ok()? };
        let az_overflow_value_delete = unsafe { lib.get::<extern fn(_: &mut AzOverflowValue)>(b"az_overflow_value_delete").ok()? };
        let az_overflow_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzOverflowValue) -> AzOverflowValue>(b"az_overflow_value_deep_copy").ok()? };
        let az_style_background_content_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_auto").ok()? };
        let az_style_background_content_value_none = unsafe { lib.get::<extern fn() -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_none").ok()? };
        let az_style_background_content_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_inherit").ok()? };
        let az_style_background_content_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_initial").ok()? };
        let az_style_background_content_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBackgroundContentPtr) -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_exact").ok()? };
        let az_style_background_content_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundContentValue)>(b"az_style_background_content_value_delete").ok()? };
        let az_style_background_content_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_deep_copy").ok()? };
        let az_style_background_position_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_auto").ok()? };
        let az_style_background_position_value_none = unsafe { lib.get::<extern fn() -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_none").ok()? };
        let az_style_background_position_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_inherit").ok()? };
        let az_style_background_position_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_initial").ok()? };
        let az_style_background_position_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBackgroundPositionPtr) -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_exact").ok()? };
        let az_style_background_position_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundPositionValue)>(b"az_style_background_position_value_delete").ok()? };
        let az_style_background_position_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundPositionValue) -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_deep_copy").ok()? };
        let az_style_background_repeat_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_auto").ok()? };
        let az_style_background_repeat_value_none = unsafe { lib.get::<extern fn() -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_none").ok()? };
        let az_style_background_repeat_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_inherit").ok()? };
        let az_style_background_repeat_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_initial").ok()? };
        let az_style_background_repeat_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBackgroundRepeatPtr) -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_exact").ok()? };
        let az_style_background_repeat_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundRepeatValue)>(b"az_style_background_repeat_value_delete").ok()? };
        let az_style_background_repeat_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundRepeatValue) -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_deep_copy").ok()? };
        let az_style_background_size_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_auto").ok()? };
        let az_style_background_size_value_none = unsafe { lib.get::<extern fn() -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_none").ok()? };
        let az_style_background_size_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_inherit").ok()? };
        let az_style_background_size_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_initial").ok()? };
        let az_style_background_size_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBackgroundSizePtr) -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_exact").ok()? };
        let az_style_background_size_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundSizeValue)>(b"az_style_background_size_value_delete").ok()? };
        let az_style_background_size_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundSizeValue) -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_deep_copy").ok()? };
        let az_style_border_bottom_color_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_auto").ok()? };
        let az_style_border_bottom_color_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_none").ok()? };
        let az_style_border_bottom_color_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_inherit").ok()? };
        let az_style_border_bottom_color_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_initial").ok()? };
        let az_style_border_bottom_color_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomColorPtr) -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_exact").ok()? };
        let az_style_border_bottom_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomColorValue)>(b"az_style_border_bottom_color_value_delete").ok()? };
        let az_style_border_bottom_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomColorValue) -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_deep_copy").ok()? };
        let az_style_border_bottom_left_radius_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_auto").ok()? };
        let az_style_border_bottom_left_radius_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_none").ok()? };
        let az_style_border_bottom_left_radius_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_inherit").ok()? };
        let az_style_border_bottom_left_radius_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_initial").ok()? };
        let az_style_border_bottom_left_radius_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomLeftRadiusPtr) -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_exact").ok()? };
        let az_style_border_bottom_left_radius_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomLeftRadiusValue)>(b"az_style_border_bottom_left_radius_value_delete").ok()? };
        let az_style_border_bottom_left_radius_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomLeftRadiusValue) -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_deep_copy").ok()? };
        let az_style_border_bottom_right_radius_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_auto").ok()? };
        let az_style_border_bottom_right_radius_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_none").ok()? };
        let az_style_border_bottom_right_radius_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_inherit").ok()? };
        let az_style_border_bottom_right_radius_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_initial").ok()? };
        let az_style_border_bottom_right_radius_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomRightRadiusPtr) -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_exact").ok()? };
        let az_style_border_bottom_right_radius_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomRightRadiusValue)>(b"az_style_border_bottom_right_radius_value_delete").ok()? };
        let az_style_border_bottom_right_radius_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomRightRadiusValue) -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_deep_copy").ok()? };
        let az_style_border_bottom_style_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_auto").ok()? };
        let az_style_border_bottom_style_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_none").ok()? };
        let az_style_border_bottom_style_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_inherit").ok()? };
        let az_style_border_bottom_style_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_initial").ok()? };
        let az_style_border_bottom_style_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomStylePtr) -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_exact").ok()? };
        let az_style_border_bottom_style_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomStyleValue)>(b"az_style_border_bottom_style_value_delete").ok()? };
        let az_style_border_bottom_style_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomStyleValue) -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_deep_copy").ok()? };
        let az_style_border_bottom_width_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_auto").ok()? };
        let az_style_border_bottom_width_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_none").ok()? };
        let az_style_border_bottom_width_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_inherit").ok()? };
        let az_style_border_bottom_width_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_initial").ok()? };
        let az_style_border_bottom_width_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomWidthPtr) -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_exact").ok()? };
        let az_style_border_bottom_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomWidthValue)>(b"az_style_border_bottom_width_value_delete").ok()? };
        let az_style_border_bottom_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomWidthValue) -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_deep_copy").ok()? };
        let az_style_border_left_color_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_auto").ok()? };
        let az_style_border_left_color_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_none").ok()? };
        let az_style_border_left_color_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_inherit").ok()? };
        let az_style_border_left_color_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_initial").ok()? };
        let az_style_border_left_color_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderLeftColorPtr) -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_exact").ok()? };
        let az_style_border_left_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftColorValue)>(b"az_style_border_left_color_value_delete").ok()? };
        let az_style_border_left_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftColorValue) -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_deep_copy").ok()? };
        let az_style_border_left_style_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_auto").ok()? };
        let az_style_border_left_style_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_none").ok()? };
        let az_style_border_left_style_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_inherit").ok()? };
        let az_style_border_left_style_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_initial").ok()? };
        let az_style_border_left_style_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderLeftStylePtr) -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_exact").ok()? };
        let az_style_border_left_style_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftStyleValue)>(b"az_style_border_left_style_value_delete").ok()? };
        let az_style_border_left_style_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftStyleValue) -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_deep_copy").ok()? };
        let az_style_border_left_width_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_auto").ok()? };
        let az_style_border_left_width_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_none").ok()? };
        let az_style_border_left_width_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_inherit").ok()? };
        let az_style_border_left_width_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_initial").ok()? };
        let az_style_border_left_width_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderLeftWidthPtr) -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_exact").ok()? };
        let az_style_border_left_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftWidthValue)>(b"az_style_border_left_width_value_delete").ok()? };
        let az_style_border_left_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftWidthValue) -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_deep_copy").ok()? };
        let az_style_border_right_color_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_auto").ok()? };
        let az_style_border_right_color_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_none").ok()? };
        let az_style_border_right_color_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_inherit").ok()? };
        let az_style_border_right_color_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_initial").ok()? };
        let az_style_border_right_color_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderRightColorPtr) -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_exact").ok()? };
        let az_style_border_right_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightColorValue)>(b"az_style_border_right_color_value_delete").ok()? };
        let az_style_border_right_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightColorValue) -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_deep_copy").ok()? };
        let az_style_border_right_style_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_auto").ok()? };
        let az_style_border_right_style_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_none").ok()? };
        let az_style_border_right_style_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_inherit").ok()? };
        let az_style_border_right_style_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_initial").ok()? };
        let az_style_border_right_style_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderRightStylePtr) -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_exact").ok()? };
        let az_style_border_right_style_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightStyleValue)>(b"az_style_border_right_style_value_delete").ok()? };
        let az_style_border_right_style_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightStyleValue) -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_deep_copy").ok()? };
        let az_style_border_right_width_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_auto").ok()? };
        let az_style_border_right_width_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_none").ok()? };
        let az_style_border_right_width_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_inherit").ok()? };
        let az_style_border_right_width_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_initial").ok()? };
        let az_style_border_right_width_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderRightWidthPtr) -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_exact").ok()? };
        let az_style_border_right_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightWidthValue)>(b"az_style_border_right_width_value_delete").ok()? };
        let az_style_border_right_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightWidthValue) -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_deep_copy").ok()? };
        let az_style_border_top_color_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_auto").ok()? };
        let az_style_border_top_color_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_none").ok()? };
        let az_style_border_top_color_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_inherit").ok()? };
        let az_style_border_top_color_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_initial").ok()? };
        let az_style_border_top_color_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderTopColorPtr) -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_exact").ok()? };
        let az_style_border_top_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopColorValue)>(b"az_style_border_top_color_value_delete").ok()? };
        let az_style_border_top_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopColorValue) -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_deep_copy").ok()? };
        let az_style_border_top_left_radius_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_auto").ok()? };
        let az_style_border_top_left_radius_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_none").ok()? };
        let az_style_border_top_left_radius_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_inherit").ok()? };
        let az_style_border_top_left_radius_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_initial").ok()? };
        let az_style_border_top_left_radius_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderTopLeftRadiusPtr) -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_exact").ok()? };
        let az_style_border_top_left_radius_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopLeftRadiusValue)>(b"az_style_border_top_left_radius_value_delete").ok()? };
        let az_style_border_top_left_radius_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopLeftRadiusValue) -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_deep_copy").ok()? };
        let az_style_border_top_right_radius_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_auto").ok()? };
        let az_style_border_top_right_radius_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_none").ok()? };
        let az_style_border_top_right_radius_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_inherit").ok()? };
        let az_style_border_top_right_radius_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_initial").ok()? };
        let az_style_border_top_right_radius_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderTopRightRadiusPtr) -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_exact").ok()? };
        let az_style_border_top_right_radius_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopRightRadiusValue)>(b"az_style_border_top_right_radius_value_delete").ok()? };
        let az_style_border_top_right_radius_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopRightRadiusValue) -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_deep_copy").ok()? };
        let az_style_border_top_style_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_auto").ok()? };
        let az_style_border_top_style_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_none").ok()? };
        let az_style_border_top_style_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_inherit").ok()? };
        let az_style_border_top_style_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_initial").ok()? };
        let az_style_border_top_style_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderTopStylePtr) -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_exact").ok()? };
        let az_style_border_top_style_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopStyleValue)>(b"az_style_border_top_style_value_delete").ok()? };
        let az_style_border_top_style_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopStyleValue) -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_deep_copy").ok()? };
        let az_style_border_top_width_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_auto").ok()? };
        let az_style_border_top_width_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_none").ok()? };
        let az_style_border_top_width_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_inherit").ok()? };
        let az_style_border_top_width_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_initial").ok()? };
        let az_style_border_top_width_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderTopWidthPtr) -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_exact").ok()? };
        let az_style_border_top_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopWidthValue)>(b"az_style_border_top_width_value_delete").ok()? };
        let az_style_border_top_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopWidthValue) -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_deep_copy").ok()? };
        let az_style_cursor_value_auto = unsafe { lib.get::<extern fn() -> AzStyleCursorValue>(b"az_style_cursor_value_auto").ok()? };
        let az_style_cursor_value_none = unsafe { lib.get::<extern fn() -> AzStyleCursorValue>(b"az_style_cursor_value_none").ok()? };
        let az_style_cursor_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleCursorValue>(b"az_style_cursor_value_inherit").ok()? };
        let az_style_cursor_value_initial = unsafe { lib.get::<extern fn() -> AzStyleCursorValue>(b"az_style_cursor_value_initial").ok()? };
        let az_style_cursor_value_exact = unsafe { lib.get::<extern fn(_: AzStyleCursorPtr) -> AzStyleCursorValue>(b"az_style_cursor_value_exact").ok()? };
        let az_style_cursor_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleCursorValue)>(b"az_style_cursor_value_delete").ok()? };
        let az_style_cursor_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleCursorValue) -> AzStyleCursorValue>(b"az_style_cursor_value_deep_copy").ok()? };
        let az_style_font_family_value_auto = unsafe { lib.get::<extern fn() -> AzStyleFontFamilyValue>(b"az_style_font_family_value_auto").ok()? };
        let az_style_font_family_value_none = unsafe { lib.get::<extern fn() -> AzStyleFontFamilyValue>(b"az_style_font_family_value_none").ok()? };
        let az_style_font_family_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleFontFamilyValue>(b"az_style_font_family_value_inherit").ok()? };
        let az_style_font_family_value_initial = unsafe { lib.get::<extern fn() -> AzStyleFontFamilyValue>(b"az_style_font_family_value_initial").ok()? };
        let az_style_font_family_value_exact = unsafe { lib.get::<extern fn(_: AzStyleFontFamilyPtr) -> AzStyleFontFamilyValue>(b"az_style_font_family_value_exact").ok()? };
        let az_style_font_family_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleFontFamilyValue)>(b"az_style_font_family_value_delete").ok()? };
        let az_style_font_family_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue>(b"az_style_font_family_value_deep_copy").ok()? };
        let az_style_font_size_value_auto = unsafe { lib.get::<extern fn() -> AzStyleFontSizeValue>(b"az_style_font_size_value_auto").ok()? };
        let az_style_font_size_value_none = unsafe { lib.get::<extern fn() -> AzStyleFontSizeValue>(b"az_style_font_size_value_none").ok()? };
        let az_style_font_size_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleFontSizeValue>(b"az_style_font_size_value_inherit").ok()? };
        let az_style_font_size_value_initial = unsafe { lib.get::<extern fn() -> AzStyleFontSizeValue>(b"az_style_font_size_value_initial").ok()? };
        let az_style_font_size_value_exact = unsafe { lib.get::<extern fn(_: AzStyleFontSizePtr) -> AzStyleFontSizeValue>(b"az_style_font_size_value_exact").ok()? };
        let az_style_font_size_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleFontSizeValue)>(b"az_style_font_size_value_delete").ok()? };
        let az_style_font_size_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleFontSizeValue) -> AzStyleFontSizeValue>(b"az_style_font_size_value_deep_copy").ok()? };
        let az_style_letter_spacing_value_auto = unsafe { lib.get::<extern fn() -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_auto").ok()? };
        let az_style_letter_spacing_value_none = unsafe { lib.get::<extern fn() -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_none").ok()? };
        let az_style_letter_spacing_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_inherit").ok()? };
        let az_style_letter_spacing_value_initial = unsafe { lib.get::<extern fn() -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_initial").ok()? };
        let az_style_letter_spacing_value_exact = unsafe { lib.get::<extern fn(_: AzStyleLetterSpacingPtr) -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_exact").ok()? };
        let az_style_letter_spacing_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleLetterSpacingValue)>(b"az_style_letter_spacing_value_delete").ok()? };
        let az_style_letter_spacing_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleLetterSpacingValue) -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_deep_copy").ok()? };
        let az_style_line_height_value_auto = unsafe { lib.get::<extern fn() -> AzStyleLineHeightValue>(b"az_style_line_height_value_auto").ok()? };
        let az_style_line_height_value_none = unsafe { lib.get::<extern fn() -> AzStyleLineHeightValue>(b"az_style_line_height_value_none").ok()? };
        let az_style_line_height_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleLineHeightValue>(b"az_style_line_height_value_inherit").ok()? };
        let az_style_line_height_value_initial = unsafe { lib.get::<extern fn() -> AzStyleLineHeightValue>(b"az_style_line_height_value_initial").ok()? };
        let az_style_line_height_value_exact = unsafe { lib.get::<extern fn(_: AzStyleLineHeightPtr) -> AzStyleLineHeightValue>(b"az_style_line_height_value_exact").ok()? };
        let az_style_line_height_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleLineHeightValue)>(b"az_style_line_height_value_delete").ok()? };
        let az_style_line_height_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleLineHeightValue) -> AzStyleLineHeightValue>(b"az_style_line_height_value_deep_copy").ok()? };
        let az_style_tab_width_value_auto = unsafe { lib.get::<extern fn() -> AzStyleTabWidthValue>(b"az_style_tab_width_value_auto").ok()? };
        let az_style_tab_width_value_none = unsafe { lib.get::<extern fn() -> AzStyleTabWidthValue>(b"az_style_tab_width_value_none").ok()? };
        let az_style_tab_width_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleTabWidthValue>(b"az_style_tab_width_value_inherit").ok()? };
        let az_style_tab_width_value_initial = unsafe { lib.get::<extern fn() -> AzStyleTabWidthValue>(b"az_style_tab_width_value_initial").ok()? };
        let az_style_tab_width_value_exact = unsafe { lib.get::<extern fn(_: AzStyleTabWidthPtr) -> AzStyleTabWidthValue>(b"az_style_tab_width_value_exact").ok()? };
        let az_style_tab_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTabWidthValue)>(b"az_style_tab_width_value_delete").ok()? };
        let az_style_tab_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleTabWidthValue) -> AzStyleTabWidthValue>(b"az_style_tab_width_value_deep_copy").ok()? };
        let az_style_text_alignment_horz_value_auto = unsafe { lib.get::<extern fn() -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_auto").ok()? };
        let az_style_text_alignment_horz_value_none = unsafe { lib.get::<extern fn() -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_none").ok()? };
        let az_style_text_alignment_horz_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_inherit").ok()? };
        let az_style_text_alignment_horz_value_initial = unsafe { lib.get::<extern fn() -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_initial").ok()? };
        let az_style_text_alignment_horz_value_exact = unsafe { lib.get::<extern fn(_: AzStyleTextAlignmentHorzPtr) -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_exact").ok()? };
        let az_style_text_alignment_horz_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTextAlignmentHorzValue)>(b"az_style_text_alignment_horz_value_delete").ok()? };
        let az_style_text_alignment_horz_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleTextAlignmentHorzValue) -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_deep_copy").ok()? };
        let az_style_text_color_value_auto = unsafe { lib.get::<extern fn() -> AzStyleTextColorValue>(b"az_style_text_color_value_auto").ok()? };
        let az_style_text_color_value_none = unsafe { lib.get::<extern fn() -> AzStyleTextColorValue>(b"az_style_text_color_value_none").ok()? };
        let az_style_text_color_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleTextColorValue>(b"az_style_text_color_value_inherit").ok()? };
        let az_style_text_color_value_initial = unsafe { lib.get::<extern fn() -> AzStyleTextColorValue>(b"az_style_text_color_value_initial").ok()? };
        let az_style_text_color_value_exact = unsafe { lib.get::<extern fn(_: AzStyleTextColorPtr) -> AzStyleTextColorValue>(b"az_style_text_color_value_exact").ok()? };
        let az_style_text_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTextColorValue)>(b"az_style_text_color_value_delete").ok()? };
        let az_style_text_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleTextColorValue) -> AzStyleTextColorValue>(b"az_style_text_color_value_deep_copy").ok()? };
        let az_style_word_spacing_value_auto = unsafe { lib.get::<extern fn() -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_auto").ok()? };
        let az_style_word_spacing_value_none = unsafe { lib.get::<extern fn() -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_none").ok()? };
        let az_style_word_spacing_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_inherit").ok()? };
        let az_style_word_spacing_value_initial = unsafe { lib.get::<extern fn() -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_initial").ok()? };
        let az_style_word_spacing_value_exact = unsafe { lib.get::<extern fn(_: AzStyleWordSpacingPtr) -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_exact").ok()? };
        let az_style_word_spacing_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleWordSpacingValue)>(b"az_style_word_spacing_value_delete").ok()? };
        let az_style_word_spacing_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleWordSpacingValue) -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_deep_copy").ok()? };
        let az_css_property_text_color = unsafe { lib.get::<extern fn(_: AzStyleTextColorValue) -> AzCssProperty>(b"az_css_property_text_color").ok()? };
        let az_css_property_font_size = unsafe { lib.get::<extern fn(_: AzStyleFontSizeValue) -> AzCssProperty>(b"az_css_property_font_size").ok()? };
        let az_css_property_font_family = unsafe { lib.get::<extern fn(_: AzStyleFontFamilyValue) -> AzCssProperty>(b"az_css_property_font_family").ok()? };
        let az_css_property_text_align = unsafe { lib.get::<extern fn(_: AzStyleTextAlignmentHorzValue) -> AzCssProperty>(b"az_css_property_text_align").ok()? };
        let az_css_property_letter_spacing = unsafe { lib.get::<extern fn(_: AzStyleLetterSpacingValue) -> AzCssProperty>(b"az_css_property_letter_spacing").ok()? };
        let az_css_property_line_height = unsafe { lib.get::<extern fn(_: AzStyleLineHeightValue) -> AzCssProperty>(b"az_css_property_line_height").ok()? };
        let az_css_property_word_spacing = unsafe { lib.get::<extern fn(_: AzStyleWordSpacingValue) -> AzCssProperty>(b"az_css_property_word_spacing").ok()? };
        let az_css_property_tab_width = unsafe { lib.get::<extern fn(_: AzStyleTabWidthValue) -> AzCssProperty>(b"az_css_property_tab_width").ok()? };
        let az_css_property_cursor = unsafe { lib.get::<extern fn(_: AzStyleCursorValue) -> AzCssProperty>(b"az_css_property_cursor").ok()? };
        let az_css_property_display = unsafe { lib.get::<extern fn(_: AzLayoutDisplayValue) -> AzCssProperty>(b"az_css_property_display").ok()? };
        let az_css_property_float = unsafe { lib.get::<extern fn(_: AzLayoutFloatValue) -> AzCssProperty>(b"az_css_property_float").ok()? };
        let az_css_property_box_sizing = unsafe { lib.get::<extern fn(_: AzLayoutBoxSizingValue) -> AzCssProperty>(b"az_css_property_box_sizing").ok()? };
        let az_css_property_width = unsafe { lib.get::<extern fn(_: AzLayoutWidthValue) -> AzCssProperty>(b"az_css_property_width").ok()? };
        let az_css_property_height = unsafe { lib.get::<extern fn(_: AzLayoutHeightValue) -> AzCssProperty>(b"az_css_property_height").ok()? };
        let az_css_property_min_width = unsafe { lib.get::<extern fn(_: AzLayoutMinWidthValue) -> AzCssProperty>(b"az_css_property_min_width").ok()? };
        let az_css_property_min_height = unsafe { lib.get::<extern fn(_: AzLayoutMinHeightValue) -> AzCssProperty>(b"az_css_property_min_height").ok()? };
        let az_css_property_max_width = unsafe { lib.get::<extern fn(_: AzLayoutMaxWidthValue) -> AzCssProperty>(b"az_css_property_max_width").ok()? };
        let az_css_property_max_height = unsafe { lib.get::<extern fn(_: AzLayoutMaxHeightValue) -> AzCssProperty>(b"az_css_property_max_height").ok()? };
        let az_css_property_position = unsafe { lib.get::<extern fn(_: AzLayoutPositionValue) -> AzCssProperty>(b"az_css_property_position").ok()? };
        let az_css_property_top = unsafe { lib.get::<extern fn(_: AzLayoutTopValue) -> AzCssProperty>(b"az_css_property_top").ok()? };
        let az_css_property_right = unsafe { lib.get::<extern fn(_: AzLayoutRightValue) -> AzCssProperty>(b"az_css_property_right").ok()? };
        let az_css_property_left = unsafe { lib.get::<extern fn(_: AzLayoutLeftValue) -> AzCssProperty>(b"az_css_property_left").ok()? };
        let az_css_property_bottom = unsafe { lib.get::<extern fn(_: AzLayoutBottomValue) -> AzCssProperty>(b"az_css_property_bottom").ok()? };
        let az_css_property_flex_wrap = unsafe { lib.get::<extern fn(_: AzLayoutWrapValue) -> AzCssProperty>(b"az_css_property_flex_wrap").ok()? };
        let az_css_property_flex_direction = unsafe { lib.get::<extern fn(_: AzLayoutDirectionValue) -> AzCssProperty>(b"az_css_property_flex_direction").ok()? };
        let az_css_property_flex_grow = unsafe { lib.get::<extern fn(_: AzLayoutFlexGrowValue) -> AzCssProperty>(b"az_css_property_flex_grow").ok()? };
        let az_css_property_flex_shrink = unsafe { lib.get::<extern fn(_: AzLayoutFlexShrinkValue) -> AzCssProperty>(b"az_css_property_flex_shrink").ok()? };
        let az_css_property_justify_content = unsafe { lib.get::<extern fn(_: AzLayoutJustifyContentValue) -> AzCssProperty>(b"az_css_property_justify_content").ok()? };
        let az_css_property_align_items = unsafe { lib.get::<extern fn(_: AzLayoutAlignItemsValue) -> AzCssProperty>(b"az_css_property_align_items").ok()? };
        let az_css_property_align_content = unsafe { lib.get::<extern fn(_: AzLayoutAlignContentValue) -> AzCssProperty>(b"az_css_property_align_content").ok()? };
        let az_css_property_background_content = unsafe { lib.get::<extern fn(_: AzStyleBackgroundContentValue) -> AzCssProperty>(b"az_css_property_background_content").ok()? };
        let az_css_property_background_position = unsafe { lib.get::<extern fn(_: AzStyleBackgroundPositionValue) -> AzCssProperty>(b"az_css_property_background_position").ok()? };
        let az_css_property_background_size = unsafe { lib.get::<extern fn(_: AzStyleBackgroundSizeValue) -> AzCssProperty>(b"az_css_property_background_size").ok()? };
        let az_css_property_background_repeat = unsafe { lib.get::<extern fn(_: AzStyleBackgroundRepeatValue) -> AzCssProperty>(b"az_css_property_background_repeat").ok()? };
        let az_css_property_overflow_x = unsafe { lib.get::<extern fn(_: AzOverflowValue) -> AzCssProperty>(b"az_css_property_overflow_x").ok()? };
        let az_css_property_overflow_y = unsafe { lib.get::<extern fn(_: AzOverflowValue) -> AzCssProperty>(b"az_css_property_overflow_y").ok()? };
        let az_css_property_padding_top = unsafe { lib.get::<extern fn(_: AzLayoutPaddingTopValue) -> AzCssProperty>(b"az_css_property_padding_top").ok()? };
        let az_css_property_padding_left = unsafe { lib.get::<extern fn(_: AzLayoutPaddingLeftValue) -> AzCssProperty>(b"az_css_property_padding_left").ok()? };
        let az_css_property_padding_right = unsafe { lib.get::<extern fn(_: AzLayoutPaddingRightValue) -> AzCssProperty>(b"az_css_property_padding_right").ok()? };
        let az_css_property_padding_bottom = unsafe { lib.get::<extern fn(_: AzLayoutPaddingBottomValue) -> AzCssProperty>(b"az_css_property_padding_bottom").ok()? };
        let az_css_property_margin_top = unsafe { lib.get::<extern fn(_: AzLayoutMarginTopValue) -> AzCssProperty>(b"az_css_property_margin_top").ok()? };
        let az_css_property_margin_left = unsafe { lib.get::<extern fn(_: AzLayoutMarginLeftValue) -> AzCssProperty>(b"az_css_property_margin_left").ok()? };
        let az_css_property_margin_right = unsafe { lib.get::<extern fn(_: AzLayoutMarginRightValue) -> AzCssProperty>(b"az_css_property_margin_right").ok()? };
        let az_css_property_margin_bottom = unsafe { lib.get::<extern fn(_: AzLayoutMarginBottomValue) -> AzCssProperty>(b"az_css_property_margin_bottom").ok()? };
        let az_css_property_border_top_left_radius = unsafe { lib.get::<extern fn(_: AzStyleBorderTopLeftRadiusValue) -> AzCssProperty>(b"az_css_property_border_top_left_radius").ok()? };
        let az_css_property_border_top_right_radius = unsafe { lib.get::<extern fn(_: AzStyleBorderTopRightRadiusValue) -> AzCssProperty>(b"az_css_property_border_top_right_radius").ok()? };
        let az_css_property_border_bottom_left_radius = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomLeftRadiusValue) -> AzCssProperty>(b"az_css_property_border_bottom_left_radius").ok()? };
        let az_css_property_border_bottom_right_radius = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomRightRadiusValue) -> AzCssProperty>(b"az_css_property_border_bottom_right_radius").ok()? };
        let az_css_property_border_top_color = unsafe { lib.get::<extern fn(_: AzStyleBorderTopColorValue) -> AzCssProperty>(b"az_css_property_border_top_color").ok()? };
        let az_css_property_border_right_color = unsafe { lib.get::<extern fn(_: AzStyleBorderRightColorValue) -> AzCssProperty>(b"az_css_property_border_right_color").ok()? };
        let az_css_property_border_left_color = unsafe { lib.get::<extern fn(_: AzStyleBorderLeftColorValue) -> AzCssProperty>(b"az_css_property_border_left_color").ok()? };
        let az_css_property_border_bottom_color = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomColorValue) -> AzCssProperty>(b"az_css_property_border_bottom_color").ok()? };
        let az_css_property_border_top_style = unsafe { lib.get::<extern fn(_: AzStyleBorderTopStyleValue) -> AzCssProperty>(b"az_css_property_border_top_style").ok()? };
        let az_css_property_border_right_style = unsafe { lib.get::<extern fn(_: AzStyleBorderRightStyleValue) -> AzCssProperty>(b"az_css_property_border_right_style").ok()? };
        let az_css_property_border_left_style = unsafe { lib.get::<extern fn(_: AzStyleBorderLeftStyleValue) -> AzCssProperty>(b"az_css_property_border_left_style").ok()? };
        let az_css_property_border_bottom_style = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomStyleValue) -> AzCssProperty>(b"az_css_property_border_bottom_style").ok()? };
        let az_css_property_border_top_width = unsafe { lib.get::<extern fn(_: AzStyleBorderTopWidthValue) -> AzCssProperty>(b"az_css_property_border_top_width").ok()? };
        let az_css_property_border_right_width = unsafe { lib.get::<extern fn(_: AzStyleBorderRightWidthValue) -> AzCssProperty>(b"az_css_property_border_right_width").ok()? };
        let az_css_property_border_left_width = unsafe { lib.get::<extern fn(_: AzStyleBorderLeftWidthValue) -> AzCssProperty>(b"az_css_property_border_left_width").ok()? };
        let az_css_property_border_bottom_width = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomWidthValue) -> AzCssProperty>(b"az_css_property_border_bottom_width").ok()? };
        let az_css_property_box_shadow_left = unsafe { lib.get::<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>(b"az_css_property_box_shadow_left").ok()? };
        let az_css_property_box_shadow_right = unsafe { lib.get::<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>(b"az_css_property_box_shadow_right").ok()? };
        let az_css_property_box_shadow_top = unsafe { lib.get::<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>(b"az_css_property_box_shadow_top").ok()? };
        let az_css_property_box_shadow_bottom = unsafe { lib.get::<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>(b"az_css_property_box_shadow_bottom").ok()? };
        let az_css_property_delete = unsafe { lib.get::<extern fn(_: &mut AzCssProperty)>(b"az_css_property_delete").ok()? };
        let az_css_property_deep_copy = unsafe { lib.get::<extern fn(_: &AzCssProperty) -> AzCssProperty>(b"az_css_property_deep_copy").ok()? };
        let az_dom_div = unsafe { lib.get::<extern fn() -> AzDomPtr>(b"az_dom_div").ok()? };
        let az_dom_body = unsafe { lib.get::<extern fn() -> AzDomPtr>(b"az_dom_body").ok()? };
        let az_dom_label = unsafe { lib.get::<extern fn(_: AzString) -> AzDomPtr>(b"az_dom_label").ok()? };
        let az_dom_text = unsafe { lib.get::<extern fn(_: AzTextId) -> AzDomPtr>(b"az_dom_text").ok()? };
        let az_dom_image = unsafe { lib.get::<extern fn(_: AzImageId) -> AzDomPtr>(b"az_dom_image").ok()? };
        let az_dom_gl_texture = unsafe { lib.get::<extern fn(_: AzGlCallback) -> AzDomPtr>(b"az_dom_gl_texture").ok()? };
        let az_dom_iframe_callback = unsafe { lib.get::<extern fn(_: AzIFrameCallback) -> AzDomPtr>(b"az_dom_iframe_callback").ok()? };
        let az_dom_add_id = unsafe { lib.get::<extern fn(_: AzString)>(b"az_dom_add_id").ok()? };
        let az_dom_with_id = unsafe { lib.get::<extern fn(_: AzString) -> AzDomPtr>(b"az_dom_with_id").ok()? };
        let az_dom_set_ids = unsafe { lib.get::<extern fn(_: AzStringVec)>(b"az_dom_set_ids").ok()? };
        let az_dom_with_ids = unsafe { lib.get::<extern fn(_: AzStringVec) -> AzDomPtr>(b"az_dom_with_ids").ok()? };
        let az_dom_add_class = unsafe { lib.get::<extern fn(_: AzString)>(b"az_dom_add_class").ok()? };
        let az_dom_with_class = unsafe { lib.get::<extern fn(_: AzString) -> AzDomPtr>(b"az_dom_with_class").ok()? };
        let az_dom_set_classes = unsafe { lib.get::<extern fn(_: AzStringVec)>(b"az_dom_set_classes").ok()? };
        let az_dom_with_classes = unsafe { lib.get::<extern fn(_: AzStringVec) -> AzDomPtr>(b"az_dom_with_classes").ok()? };
        let az_dom_add_callback = unsafe { lib.get::<extern fn(_: AzCallback)>(b"az_dom_add_callback").ok()? };
        let az_dom_with_callback = unsafe { lib.get::<extern fn(_: AzCallback) -> AzDomPtr>(b"az_dom_with_callback").ok()? };
        let az_dom_add_css_override = unsafe { lib.get::<extern fn(_: AzCssProperty)>(b"az_dom_add_css_override").ok()? };
        let az_dom_with_css_override = unsafe { lib.get::<extern fn(_: AzCssProperty) -> AzDomPtr>(b"az_dom_with_css_override").ok()? };
        let az_dom_set_is_draggable = unsafe { lib.get::<extern fn(_: bool)>(b"az_dom_set_is_draggable").ok()? };
        let az_dom_is_draggable = unsafe { lib.get::<extern fn(_: bool) -> AzDomPtr>(b"az_dom_is_draggable").ok()? };
        let az_dom_set_tab_index = unsafe { lib.get::<extern fn(_: AzTabIndex)>(b"az_dom_set_tab_index").ok()? };
        let az_dom_with_tab_index = unsafe { lib.get::<extern fn(_: AzTabIndex) -> AzDomPtr>(b"az_dom_with_tab_index").ok()? };
        let az_dom_add_child = unsafe { lib.get::<extern fn(_: AzDomPtr)>(b"az_dom_add_child").ok()? };
        let az_dom_with_child = unsafe { lib.get::<extern fn(_: AzDomPtr) -> AzDomPtr>(b"az_dom_with_child").ok()? };
        let az_dom_has_id = unsafe { lib.get::<extern fn(_: AzString) -> bool>(b"az_dom_has_id").ok()? };
        let az_dom_has_class = unsafe { lib.get::<extern fn(_: AzString) -> bool>(b"az_dom_has_class").ok()? };
        let az_dom_get_html_string = unsafe { lib.get::<extern fn(_: &mut AzDomPtr) -> AzString>(b"az_dom_get_html_string").ok()? };
        let az_dom_delete = unsafe { lib.get::<extern fn(_: &mut AzDomPtr)>(b"az_dom_delete").ok()? };
        let az_dom_shallow_copy = unsafe { lib.get::<extern fn(_: &AzDomPtr) -> AzDomPtr>(b"az_dom_shallow_copy").ok()? };
        let az_event_filter_hover = unsafe { lib.get::<extern fn(_: AzHoverEventFilter) -> AzEventFilter>(b"az_event_filter_hover").ok()? };
        let az_event_filter_not = unsafe { lib.get::<extern fn(_: AzNotEventFilter) -> AzEventFilter>(b"az_event_filter_not").ok()? };
        let az_event_filter_focus = unsafe { lib.get::<extern fn(_: AzFocusEventFilter) -> AzEventFilter>(b"az_event_filter_focus").ok()? };
        let az_event_filter_window = unsafe { lib.get::<extern fn(_: AzWindowEventFilter) -> AzEventFilter>(b"az_event_filter_window").ok()? };
        let az_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzEventFilter)>(b"az_event_filter_delete").ok()? };
        let az_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzEventFilter) -> AzEventFilter>(b"az_event_filter_deep_copy").ok()? };
        let az_hover_event_filter_mouse_over = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_mouse_over").ok()? };
        let az_hover_event_filter_mouse_down = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_mouse_down").ok()? };
        let az_hover_event_filter_left_mouse_down = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_left_mouse_down").ok()? };
        let az_hover_event_filter_right_mouse_down = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_right_mouse_down").ok()? };
        let az_hover_event_filter_middle_mouse_down = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_middle_mouse_down").ok()? };
        let az_hover_event_filter_mouse_up = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_mouse_up").ok()? };
        let az_hover_event_filter_left_mouse_up = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_left_mouse_up").ok()? };
        let az_hover_event_filter_right_mouse_up = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_right_mouse_up").ok()? };
        let az_hover_event_filter_middle_mouse_up = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_middle_mouse_up").ok()? };
        let az_hover_event_filter_mouse_enter = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_mouse_enter").ok()? };
        let az_hover_event_filter_mouse_leave = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_mouse_leave").ok()? };
        let az_hover_event_filter_scroll = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_scroll").ok()? };
        let az_hover_event_filter_scroll_start = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_scroll_start").ok()? };
        let az_hover_event_filter_scroll_end = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_scroll_end").ok()? };
        let az_hover_event_filter_text_input = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_text_input").ok()? };
        let az_hover_event_filter_virtual_key_down = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_virtual_key_down").ok()? };
        let az_hover_event_filter_virtual_key_up = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_virtual_key_up").ok()? };
        let az_hover_event_filter_hovered_file = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_hovered_file").ok()? };
        let az_hover_event_filter_dropped_file = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_dropped_file").ok()? };
        let az_hover_event_filter_hovered_file_cancelled = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_hovered_file_cancelled").ok()? };
        let az_hover_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzHoverEventFilter)>(b"az_hover_event_filter_delete").ok()? };
        let az_hover_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzHoverEventFilter) -> AzHoverEventFilter>(b"az_hover_event_filter_deep_copy").ok()? };
        let az_focus_event_filter_mouse_over = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_mouse_over").ok()? };
        let az_focus_event_filter_mouse_down = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_mouse_down").ok()? };
        let az_focus_event_filter_left_mouse_down = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_left_mouse_down").ok()? };
        let az_focus_event_filter_right_mouse_down = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_right_mouse_down").ok()? };
        let az_focus_event_filter_middle_mouse_down = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_middle_mouse_down").ok()? };
        let az_focus_event_filter_mouse_up = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_mouse_up").ok()? };
        let az_focus_event_filter_left_mouse_up = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_left_mouse_up").ok()? };
        let az_focus_event_filter_right_mouse_up = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_right_mouse_up").ok()? };
        let az_focus_event_filter_middle_mouse_up = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_middle_mouse_up").ok()? };
        let az_focus_event_filter_mouse_enter = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_mouse_enter").ok()? };
        let az_focus_event_filter_mouse_leave = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_mouse_leave").ok()? };
        let az_focus_event_filter_scroll = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_scroll").ok()? };
        let az_focus_event_filter_scroll_start = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_scroll_start").ok()? };
        let az_focus_event_filter_scroll_end = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_scroll_end").ok()? };
        let az_focus_event_filter_text_input = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_text_input").ok()? };
        let az_focus_event_filter_virtual_key_down = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_virtual_key_down").ok()? };
        let az_focus_event_filter_virtual_key_up = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_virtual_key_up").ok()? };
        let az_focus_event_filter_focus_received = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_focus_received").ok()? };
        let az_focus_event_filter_focus_lost = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_focus_lost").ok()? };
        let az_focus_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzFocusEventFilter)>(b"az_focus_event_filter_delete").ok()? };
        let az_focus_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzFocusEventFilter) -> AzFocusEventFilter>(b"az_focus_event_filter_deep_copy").ok()? };
        let az_not_event_filter_hover = unsafe { lib.get::<extern fn(_: AzHoverEventFilter) -> AzNotEventFilter>(b"az_not_event_filter_hover").ok()? };
        let az_not_event_filter_focus = unsafe { lib.get::<extern fn(_: AzFocusEventFilter) -> AzNotEventFilter>(b"az_not_event_filter_focus").ok()? };
        let az_not_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzNotEventFilter)>(b"az_not_event_filter_delete").ok()? };
        let az_not_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzNotEventFilter) -> AzNotEventFilter>(b"az_not_event_filter_deep_copy").ok()? };
        let az_window_event_filter_mouse_over = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_mouse_over").ok()? };
        let az_window_event_filter_mouse_down = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_mouse_down").ok()? };
        let az_window_event_filter_left_mouse_down = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_left_mouse_down").ok()? };
        let az_window_event_filter_right_mouse_down = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_right_mouse_down").ok()? };
        let az_window_event_filter_middle_mouse_down = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_middle_mouse_down").ok()? };
        let az_window_event_filter_mouse_up = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_mouse_up").ok()? };
        let az_window_event_filter_left_mouse_up = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_left_mouse_up").ok()? };
        let az_window_event_filter_right_mouse_up = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_right_mouse_up").ok()? };
        let az_window_event_filter_middle_mouse_up = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_middle_mouse_up").ok()? };
        let az_window_event_filter_mouse_enter = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_mouse_enter").ok()? };
        let az_window_event_filter_mouse_leave = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_mouse_leave").ok()? };
        let az_window_event_filter_scroll = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_scroll").ok()? };
        let az_window_event_filter_scroll_start = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_scroll_start").ok()? };
        let az_window_event_filter_scroll_end = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_scroll_end").ok()? };
        let az_window_event_filter_text_input = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_text_input").ok()? };
        let az_window_event_filter_virtual_key_down = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_virtual_key_down").ok()? };
        let az_window_event_filter_virtual_key_up = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_virtual_key_up").ok()? };
        let az_window_event_filter_hovered_file = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_hovered_file").ok()? };
        let az_window_event_filter_dropped_file = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_dropped_file").ok()? };
        let az_window_event_filter_hovered_file_cancelled = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_hovered_file_cancelled").ok()? };
        let az_window_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzWindowEventFilter)>(b"az_window_event_filter_delete").ok()? };
        let az_window_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzWindowEventFilter) -> AzWindowEventFilter>(b"az_window_event_filter_deep_copy").ok()? };
        let az_tab_index_auto = unsafe { lib.get::<extern fn() -> AzTabIndex>(b"az_tab_index_auto").ok()? };
        let az_tab_index_override_in_parent = unsafe { lib.get::<extern fn(_: usize) -> AzTabIndex>(b"az_tab_index_override_in_parent").ok()? };
        let az_tab_index_no_keyboard_focus = unsafe { lib.get::<extern fn() -> AzTabIndex>(b"az_tab_index_no_keyboard_focus").ok()? };
        let az_tab_index_delete = unsafe { lib.get::<extern fn(_: &mut AzTabIndex)>(b"az_tab_index_delete").ok()? };
        let az_tab_index_deep_copy = unsafe { lib.get::<extern fn(_: &AzTabIndex) -> AzTabIndex>(b"az_tab_index_deep_copy").ok()? };
        let az_text_id_new = unsafe { lib.get::<extern fn() -> AzTextId>(b"az_text_id_new").ok()? };
        let az_text_id_delete = unsafe { lib.get::<extern fn(_: &mut AzTextId)>(b"az_text_id_delete").ok()? };
        let az_text_id_deep_copy = unsafe { lib.get::<extern fn(_: &AzTextId) -> AzTextId>(b"az_text_id_deep_copy").ok()? };
        let az_image_id_new = unsafe { lib.get::<extern fn() -> AzImageId>(b"az_image_id_new").ok()? };
        let az_image_id_delete = unsafe { lib.get::<extern fn(_: &mut AzImageId)>(b"az_image_id_delete").ok()? };
        let az_image_id_deep_copy = unsafe { lib.get::<extern fn(_: &AzImageId) -> AzImageId>(b"az_image_id_deep_copy").ok()? };
        let az_font_id_new = unsafe { lib.get::<extern fn() -> AzFontId>(b"az_font_id_new").ok()? };
        let az_font_id_delete = unsafe { lib.get::<extern fn(_: &mut AzFontId)>(b"az_font_id_delete").ok()? };
        let az_font_id_deep_copy = unsafe { lib.get::<extern fn(_: &AzFontId) -> AzFontId>(b"az_font_id_deep_copy").ok()? };
        let az_image_source_embedded = unsafe { lib.get::<extern fn(_: AzU8Vec) -> AzImageSource>(b"az_image_source_embedded").ok()? };
        let az_image_source_file = unsafe { lib.get::<extern fn(_: AzPathBufPtr) -> AzImageSource>(b"az_image_source_file").ok()? };
        let az_image_source_raw = unsafe { lib.get::<extern fn(_: AzRawImagePtr) -> AzImageSource>(b"az_image_source_raw").ok()? };
        let az_image_source_delete = unsafe { lib.get::<extern fn(_: &mut AzImageSource)>(b"az_image_source_delete").ok()? };
        let az_image_source_deep_copy = unsafe { lib.get::<extern fn(_: &AzImageSource) -> AzImageSource>(b"az_image_source_deep_copy").ok()? };
        let az_font_source_embedded = unsafe { lib.get::<extern fn(_: AzU8Vec) -> AzFontSource>(b"az_font_source_embedded").ok()? };
        let az_font_source_file = unsafe { lib.get::<extern fn(_: AzPathBufPtr) -> AzFontSource>(b"az_font_source_file").ok()? };
        let az_font_source_system = unsafe { lib.get::<extern fn(_: AzString) -> AzFontSource>(b"az_font_source_system").ok()? };
        let az_font_source_delete = unsafe { lib.get::<extern fn(_: &mut AzFontSource)>(b"az_font_source_delete").ok()? };
        let az_font_source_deep_copy = unsafe { lib.get::<extern fn(_: &AzFontSource) -> AzFontSource>(b"az_font_source_deep_copy").ok()? };
        let az_raw_image_new = unsafe { lib.get::<extern fn(_: AzRawImageFormat) -> AzRawImagePtr>(b"az_raw_image_new").ok()? };
        let az_raw_image_delete = unsafe { lib.get::<extern fn(_: &mut AzRawImagePtr)>(b"az_raw_image_delete").ok()? };
        let az_raw_image_shallow_copy = unsafe { lib.get::<extern fn(_: &AzRawImagePtr) -> AzRawImagePtr>(b"az_raw_image_shallow_copy").ok()? };
        let az_raw_image_format_r8 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_r8").ok()? };
        let az_raw_image_format_r16 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_r16").ok()? };
        let az_raw_image_format_rg16 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_rg16").ok()? };
        let az_raw_image_format_bgra8 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_bgra8").ok()? };
        let az_raw_image_format_rgbaf32 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_rgbaf32").ok()? };
        let az_raw_image_format_rg8 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_rg8").ok()? };
        let az_raw_image_format_rgbai32 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_rgbai32").ok()? };
        let az_raw_image_format_rgba8 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_rgba8").ok()? };
        let az_raw_image_format_delete = unsafe { lib.get::<extern fn(_: &mut AzRawImageFormat)>(b"az_raw_image_format_delete").ok()? };
        let az_raw_image_format_deep_copy = unsafe { lib.get::<extern fn(_: &AzRawImageFormat) -> AzRawImageFormat>(b"az_raw_image_format_deep_copy").ok()? };
        let az_window_create_options_new = unsafe { lib.get::<extern fn(_: AzCssPtr) -> AzWindowCreateOptionsPtr>(b"az_window_create_options_new").ok()? };
        let az_window_create_options_delete = unsafe { lib.get::<extern fn(_: &mut AzWindowCreateOptionsPtr)>(b"az_window_create_options_delete").ok()? };
        let az_window_create_options_shallow_copy = unsafe { lib.get::<extern fn(_: &AzWindowCreateOptionsPtr) -> AzWindowCreateOptionsPtr>(b"az_window_create_options_shallow_copy").ok()? };
        Some(AzulDll {
            lib: Box::new(lib),
            az_string_from_utf8_unchecked,
            az_string_from_utf8_lossy,
            az_string_into_bytes,
            az_string_delete,
            az_string_deep_copy,
            az_u8_vec_copy_from,
            az_u8_vec_as_ptr,
            az_u8_vec_len,
            az_u8_vec_delete,
            az_u8_vec_deep_copy,
            az_string_vec_copy_from,
            az_string_vec_delete,
            az_string_vec_deep_copy,
            az_path_buf_new,
            az_path_buf_delete,
            az_path_buf_shallow_copy,
            az_app_config_default,
            az_app_config_delete,
            az_app_config_shallow_copy,
            az_app_new,
            az_app_run,
            az_app_delete,
            az_app_shallow_copy,
            az_callback_info_delete,
            az_callback_info_shallow_copy,
            az_i_frame_callback_info_delete,
            az_i_frame_callback_info_shallow_copy,
            az_i_frame_callback_return_delete,
            az_i_frame_callback_return_shallow_copy,
            az_gl_callback_info_delete,
            az_gl_callback_info_shallow_copy,
            az_gl_callback_return_delete,
            az_gl_callback_return_shallow_copy,
            az_layout_info_delete,
            az_layout_info_shallow_copy,
            az_css_native,
            az_css_empty,
            az_css_delete,
            az_css_shallow_copy,
            az_box_shadow_pre_display_item_delete,
            az_box_shadow_pre_display_item_shallow_copy,
            az_layout_align_content_delete,
            az_layout_align_content_shallow_copy,
            az_layout_align_items_delete,
            az_layout_align_items_shallow_copy,
            az_layout_bottom_delete,
            az_layout_bottom_shallow_copy,
            az_layout_box_sizing_delete,
            az_layout_box_sizing_shallow_copy,
            az_layout_direction_delete,
            az_layout_direction_shallow_copy,
            az_layout_display_delete,
            az_layout_display_shallow_copy,
            az_layout_flex_grow_delete,
            az_layout_flex_grow_shallow_copy,
            az_layout_flex_shrink_delete,
            az_layout_flex_shrink_shallow_copy,
            az_layout_float_delete,
            az_layout_float_shallow_copy,
            az_layout_height_delete,
            az_layout_height_shallow_copy,
            az_layout_justify_content_delete,
            az_layout_justify_content_shallow_copy,
            az_layout_left_delete,
            az_layout_left_shallow_copy,
            az_layout_margin_bottom_delete,
            az_layout_margin_bottom_shallow_copy,
            az_layout_margin_left_delete,
            az_layout_margin_left_shallow_copy,
            az_layout_margin_right_delete,
            az_layout_margin_right_shallow_copy,
            az_layout_margin_top_delete,
            az_layout_margin_top_shallow_copy,
            az_layout_max_height_delete,
            az_layout_max_height_shallow_copy,
            az_layout_max_width_delete,
            az_layout_max_width_shallow_copy,
            az_layout_min_height_delete,
            az_layout_min_height_shallow_copy,
            az_layout_min_width_delete,
            az_layout_min_width_shallow_copy,
            az_layout_padding_bottom_delete,
            az_layout_padding_bottom_shallow_copy,
            az_layout_padding_left_delete,
            az_layout_padding_left_shallow_copy,
            az_layout_padding_right_delete,
            az_layout_padding_right_shallow_copy,
            az_layout_padding_top_delete,
            az_layout_padding_top_shallow_copy,
            az_layout_position_delete,
            az_layout_position_shallow_copy,
            az_layout_right_delete,
            az_layout_right_shallow_copy,
            az_layout_top_delete,
            az_layout_top_shallow_copy,
            az_layout_width_delete,
            az_layout_width_shallow_copy,
            az_layout_wrap_delete,
            az_layout_wrap_shallow_copy,
            az_overflow_delete,
            az_overflow_shallow_copy,
            az_style_background_content_delete,
            az_style_background_content_shallow_copy,
            az_style_background_position_delete,
            az_style_background_position_shallow_copy,
            az_style_background_repeat_delete,
            az_style_background_repeat_shallow_copy,
            az_style_background_size_delete,
            az_style_background_size_shallow_copy,
            az_style_border_bottom_color_delete,
            az_style_border_bottom_color_shallow_copy,
            az_style_border_bottom_left_radius_delete,
            az_style_border_bottom_left_radius_shallow_copy,
            az_style_border_bottom_right_radius_delete,
            az_style_border_bottom_right_radius_shallow_copy,
            az_style_border_bottom_style_delete,
            az_style_border_bottom_style_shallow_copy,
            az_style_border_bottom_width_delete,
            az_style_border_bottom_width_shallow_copy,
            az_style_border_left_color_delete,
            az_style_border_left_color_shallow_copy,
            az_style_border_left_style_delete,
            az_style_border_left_style_shallow_copy,
            az_style_border_left_width_delete,
            az_style_border_left_width_shallow_copy,
            az_style_border_right_color_delete,
            az_style_border_right_color_shallow_copy,
            az_style_border_right_style_delete,
            az_style_border_right_style_shallow_copy,
            az_style_border_right_width_delete,
            az_style_border_right_width_shallow_copy,
            az_style_border_top_color_delete,
            az_style_border_top_color_shallow_copy,
            az_style_border_top_left_radius_delete,
            az_style_border_top_left_radius_shallow_copy,
            az_style_border_top_right_radius_delete,
            az_style_border_top_right_radius_shallow_copy,
            az_style_border_top_style_delete,
            az_style_border_top_style_shallow_copy,
            az_style_border_top_width_delete,
            az_style_border_top_width_shallow_copy,
            az_style_cursor_delete,
            az_style_cursor_shallow_copy,
            az_style_font_family_delete,
            az_style_font_family_shallow_copy,
            az_style_font_size_delete,
            az_style_font_size_shallow_copy,
            az_style_letter_spacing_delete,
            az_style_letter_spacing_shallow_copy,
            az_style_line_height_delete,
            az_style_line_height_shallow_copy,
            az_style_tab_width_delete,
            az_style_tab_width_shallow_copy,
            az_style_text_alignment_horz_delete,
            az_style_text_alignment_horz_shallow_copy,
            az_style_text_color_delete,
            az_style_text_color_shallow_copy,
            az_style_word_spacing_delete,
            az_style_word_spacing_shallow_copy,
            az_box_shadow_pre_display_item_value_auto,
            az_box_shadow_pre_display_item_value_none,
            az_box_shadow_pre_display_item_value_inherit,
            az_box_shadow_pre_display_item_value_initial,
            az_box_shadow_pre_display_item_value_exact,
            az_box_shadow_pre_display_item_value_delete,
            az_box_shadow_pre_display_item_value_deep_copy,
            az_layout_align_content_value_auto,
            az_layout_align_content_value_none,
            az_layout_align_content_value_inherit,
            az_layout_align_content_value_initial,
            az_layout_align_content_value_exact,
            az_layout_align_content_value_delete,
            az_layout_align_content_value_deep_copy,
            az_layout_align_items_value_auto,
            az_layout_align_items_value_none,
            az_layout_align_items_value_inherit,
            az_layout_align_items_value_initial,
            az_layout_align_items_value_exact,
            az_layout_align_items_value_delete,
            az_layout_align_items_value_deep_copy,
            az_layout_bottom_value_auto,
            az_layout_bottom_value_none,
            az_layout_bottom_value_inherit,
            az_layout_bottom_value_initial,
            az_layout_bottom_value_exact,
            az_layout_bottom_value_delete,
            az_layout_bottom_value_deep_copy,
            az_layout_box_sizing_value_auto,
            az_layout_box_sizing_value_none,
            az_layout_box_sizing_value_inherit,
            az_layout_box_sizing_value_initial,
            az_layout_box_sizing_value_exact,
            az_layout_box_sizing_value_delete,
            az_layout_box_sizing_value_deep_copy,
            az_layout_direction_value_auto,
            az_layout_direction_value_none,
            az_layout_direction_value_inherit,
            az_layout_direction_value_initial,
            az_layout_direction_value_exact,
            az_layout_direction_value_delete,
            az_layout_direction_value_deep_copy,
            az_layout_display_value_auto,
            az_layout_display_value_none,
            az_layout_display_value_inherit,
            az_layout_display_value_initial,
            az_layout_display_value_exact,
            az_layout_display_value_delete,
            az_layout_display_value_deep_copy,
            az_layout_flex_grow_value_auto,
            az_layout_flex_grow_value_none,
            az_layout_flex_grow_value_inherit,
            az_layout_flex_grow_value_initial,
            az_layout_flex_grow_value_exact,
            az_layout_flex_grow_value_delete,
            az_layout_flex_grow_value_deep_copy,
            az_layout_flex_shrink_value_auto,
            az_layout_flex_shrink_value_none,
            az_layout_flex_shrink_value_inherit,
            az_layout_flex_shrink_value_initial,
            az_layout_flex_shrink_value_exact,
            az_layout_flex_shrink_value_delete,
            az_layout_flex_shrink_value_deep_copy,
            az_layout_float_value_auto,
            az_layout_float_value_none,
            az_layout_float_value_inherit,
            az_layout_float_value_initial,
            az_layout_float_value_exact,
            az_layout_float_value_delete,
            az_layout_float_value_deep_copy,
            az_layout_height_value_auto,
            az_layout_height_value_none,
            az_layout_height_value_inherit,
            az_layout_height_value_initial,
            az_layout_height_value_exact,
            az_layout_height_value_delete,
            az_layout_height_value_deep_copy,
            az_layout_justify_content_value_auto,
            az_layout_justify_content_value_none,
            az_layout_justify_content_value_inherit,
            az_layout_justify_content_value_initial,
            az_layout_justify_content_value_exact,
            az_layout_justify_content_value_delete,
            az_layout_justify_content_value_deep_copy,
            az_layout_left_value_auto,
            az_layout_left_value_none,
            az_layout_left_value_inherit,
            az_layout_left_value_initial,
            az_layout_left_value_exact,
            az_layout_left_value_delete,
            az_layout_left_value_deep_copy,
            az_layout_margin_bottom_value_auto,
            az_layout_margin_bottom_value_none,
            az_layout_margin_bottom_value_inherit,
            az_layout_margin_bottom_value_initial,
            az_layout_margin_bottom_value_exact,
            az_layout_margin_bottom_value_delete,
            az_layout_margin_bottom_value_deep_copy,
            az_layout_margin_left_value_auto,
            az_layout_margin_left_value_none,
            az_layout_margin_left_value_inherit,
            az_layout_margin_left_value_initial,
            az_layout_margin_left_value_exact,
            az_layout_margin_left_value_delete,
            az_layout_margin_left_value_deep_copy,
            az_layout_margin_right_value_auto,
            az_layout_margin_right_value_none,
            az_layout_margin_right_value_inherit,
            az_layout_margin_right_value_initial,
            az_layout_margin_right_value_exact,
            az_layout_margin_right_value_delete,
            az_layout_margin_right_value_deep_copy,
            az_layout_margin_top_value_auto,
            az_layout_margin_top_value_none,
            az_layout_margin_top_value_inherit,
            az_layout_margin_top_value_initial,
            az_layout_margin_top_value_exact,
            az_layout_margin_top_value_delete,
            az_layout_margin_top_value_deep_copy,
            az_layout_max_height_value_auto,
            az_layout_max_height_value_none,
            az_layout_max_height_value_inherit,
            az_layout_max_height_value_initial,
            az_layout_max_height_value_exact,
            az_layout_max_height_value_delete,
            az_layout_max_height_value_deep_copy,
            az_layout_max_width_value_auto,
            az_layout_max_width_value_none,
            az_layout_max_width_value_inherit,
            az_layout_max_width_value_initial,
            az_layout_max_width_value_exact,
            az_layout_max_width_value_delete,
            az_layout_max_width_value_deep_copy,
            az_layout_min_height_value_auto,
            az_layout_min_height_value_none,
            az_layout_min_height_value_inherit,
            az_layout_min_height_value_initial,
            az_layout_min_height_value_exact,
            az_layout_min_height_value_delete,
            az_layout_min_height_value_deep_copy,
            az_layout_min_width_value_auto,
            az_layout_min_width_value_none,
            az_layout_min_width_value_inherit,
            az_layout_min_width_value_initial,
            az_layout_min_width_value_exact,
            az_layout_min_width_value_delete,
            az_layout_min_width_value_deep_copy,
            az_layout_padding_bottom_value_auto,
            az_layout_padding_bottom_value_none,
            az_layout_padding_bottom_value_inherit,
            az_layout_padding_bottom_value_initial,
            az_layout_padding_bottom_value_exact,
            az_layout_padding_bottom_value_delete,
            az_layout_padding_bottom_value_deep_copy,
            az_layout_padding_left_value_auto,
            az_layout_padding_left_value_none,
            az_layout_padding_left_value_inherit,
            az_layout_padding_left_value_initial,
            az_layout_padding_left_value_exact,
            az_layout_padding_left_value_delete,
            az_layout_padding_left_value_deep_copy,
            az_layout_padding_right_value_auto,
            az_layout_padding_right_value_none,
            az_layout_padding_right_value_inherit,
            az_layout_padding_right_value_initial,
            az_layout_padding_right_value_exact,
            az_layout_padding_right_value_delete,
            az_layout_padding_right_value_deep_copy,
            az_layout_padding_top_value_auto,
            az_layout_padding_top_value_none,
            az_layout_padding_top_value_inherit,
            az_layout_padding_top_value_initial,
            az_layout_padding_top_value_exact,
            az_layout_padding_top_value_delete,
            az_layout_padding_top_value_deep_copy,
            az_layout_position_value_auto,
            az_layout_position_value_none,
            az_layout_position_value_inherit,
            az_layout_position_value_initial,
            az_layout_position_value_exact,
            az_layout_position_value_delete,
            az_layout_position_value_deep_copy,
            az_layout_right_value_auto,
            az_layout_right_value_none,
            az_layout_right_value_inherit,
            az_layout_right_value_initial,
            az_layout_right_value_exact,
            az_layout_right_value_delete,
            az_layout_right_value_deep_copy,
            az_layout_top_value_auto,
            az_layout_top_value_none,
            az_layout_top_value_inherit,
            az_layout_top_value_initial,
            az_layout_top_value_exact,
            az_layout_top_value_delete,
            az_layout_top_value_deep_copy,
            az_layout_width_value_auto,
            az_layout_width_value_none,
            az_layout_width_value_inherit,
            az_layout_width_value_initial,
            az_layout_width_value_exact,
            az_layout_width_value_delete,
            az_layout_width_value_deep_copy,
            az_layout_wrap_value_auto,
            az_layout_wrap_value_none,
            az_layout_wrap_value_inherit,
            az_layout_wrap_value_initial,
            az_layout_wrap_value_exact,
            az_layout_wrap_value_delete,
            az_layout_wrap_value_deep_copy,
            az_overflow_value_auto,
            az_overflow_value_none,
            az_overflow_value_inherit,
            az_overflow_value_initial,
            az_overflow_value_exact,
            az_overflow_value_delete,
            az_overflow_value_deep_copy,
            az_style_background_content_value_auto,
            az_style_background_content_value_none,
            az_style_background_content_value_inherit,
            az_style_background_content_value_initial,
            az_style_background_content_value_exact,
            az_style_background_content_value_delete,
            az_style_background_content_value_deep_copy,
            az_style_background_position_value_auto,
            az_style_background_position_value_none,
            az_style_background_position_value_inherit,
            az_style_background_position_value_initial,
            az_style_background_position_value_exact,
            az_style_background_position_value_delete,
            az_style_background_position_value_deep_copy,
            az_style_background_repeat_value_auto,
            az_style_background_repeat_value_none,
            az_style_background_repeat_value_inherit,
            az_style_background_repeat_value_initial,
            az_style_background_repeat_value_exact,
            az_style_background_repeat_value_delete,
            az_style_background_repeat_value_deep_copy,
            az_style_background_size_value_auto,
            az_style_background_size_value_none,
            az_style_background_size_value_inherit,
            az_style_background_size_value_initial,
            az_style_background_size_value_exact,
            az_style_background_size_value_delete,
            az_style_background_size_value_deep_copy,
            az_style_border_bottom_color_value_auto,
            az_style_border_bottom_color_value_none,
            az_style_border_bottom_color_value_inherit,
            az_style_border_bottom_color_value_initial,
            az_style_border_bottom_color_value_exact,
            az_style_border_bottom_color_value_delete,
            az_style_border_bottom_color_value_deep_copy,
            az_style_border_bottom_left_radius_value_auto,
            az_style_border_bottom_left_radius_value_none,
            az_style_border_bottom_left_radius_value_inherit,
            az_style_border_bottom_left_radius_value_initial,
            az_style_border_bottom_left_radius_value_exact,
            az_style_border_bottom_left_radius_value_delete,
            az_style_border_bottom_left_radius_value_deep_copy,
            az_style_border_bottom_right_radius_value_auto,
            az_style_border_bottom_right_radius_value_none,
            az_style_border_bottom_right_radius_value_inherit,
            az_style_border_bottom_right_radius_value_initial,
            az_style_border_bottom_right_radius_value_exact,
            az_style_border_bottom_right_radius_value_delete,
            az_style_border_bottom_right_radius_value_deep_copy,
            az_style_border_bottom_style_value_auto,
            az_style_border_bottom_style_value_none,
            az_style_border_bottom_style_value_inherit,
            az_style_border_bottom_style_value_initial,
            az_style_border_bottom_style_value_exact,
            az_style_border_bottom_style_value_delete,
            az_style_border_bottom_style_value_deep_copy,
            az_style_border_bottom_width_value_auto,
            az_style_border_bottom_width_value_none,
            az_style_border_bottom_width_value_inherit,
            az_style_border_bottom_width_value_initial,
            az_style_border_bottom_width_value_exact,
            az_style_border_bottom_width_value_delete,
            az_style_border_bottom_width_value_deep_copy,
            az_style_border_left_color_value_auto,
            az_style_border_left_color_value_none,
            az_style_border_left_color_value_inherit,
            az_style_border_left_color_value_initial,
            az_style_border_left_color_value_exact,
            az_style_border_left_color_value_delete,
            az_style_border_left_color_value_deep_copy,
            az_style_border_left_style_value_auto,
            az_style_border_left_style_value_none,
            az_style_border_left_style_value_inherit,
            az_style_border_left_style_value_initial,
            az_style_border_left_style_value_exact,
            az_style_border_left_style_value_delete,
            az_style_border_left_style_value_deep_copy,
            az_style_border_left_width_value_auto,
            az_style_border_left_width_value_none,
            az_style_border_left_width_value_inherit,
            az_style_border_left_width_value_initial,
            az_style_border_left_width_value_exact,
            az_style_border_left_width_value_delete,
            az_style_border_left_width_value_deep_copy,
            az_style_border_right_color_value_auto,
            az_style_border_right_color_value_none,
            az_style_border_right_color_value_inherit,
            az_style_border_right_color_value_initial,
            az_style_border_right_color_value_exact,
            az_style_border_right_color_value_delete,
            az_style_border_right_color_value_deep_copy,
            az_style_border_right_style_value_auto,
            az_style_border_right_style_value_none,
            az_style_border_right_style_value_inherit,
            az_style_border_right_style_value_initial,
            az_style_border_right_style_value_exact,
            az_style_border_right_style_value_delete,
            az_style_border_right_style_value_deep_copy,
            az_style_border_right_width_value_auto,
            az_style_border_right_width_value_none,
            az_style_border_right_width_value_inherit,
            az_style_border_right_width_value_initial,
            az_style_border_right_width_value_exact,
            az_style_border_right_width_value_delete,
            az_style_border_right_width_value_deep_copy,
            az_style_border_top_color_value_auto,
            az_style_border_top_color_value_none,
            az_style_border_top_color_value_inherit,
            az_style_border_top_color_value_initial,
            az_style_border_top_color_value_exact,
            az_style_border_top_color_value_delete,
            az_style_border_top_color_value_deep_copy,
            az_style_border_top_left_radius_value_auto,
            az_style_border_top_left_radius_value_none,
            az_style_border_top_left_radius_value_inherit,
            az_style_border_top_left_radius_value_initial,
            az_style_border_top_left_radius_value_exact,
            az_style_border_top_left_radius_value_delete,
            az_style_border_top_left_radius_value_deep_copy,
            az_style_border_top_right_radius_value_auto,
            az_style_border_top_right_radius_value_none,
            az_style_border_top_right_radius_value_inherit,
            az_style_border_top_right_radius_value_initial,
            az_style_border_top_right_radius_value_exact,
            az_style_border_top_right_radius_value_delete,
            az_style_border_top_right_radius_value_deep_copy,
            az_style_border_top_style_value_auto,
            az_style_border_top_style_value_none,
            az_style_border_top_style_value_inherit,
            az_style_border_top_style_value_initial,
            az_style_border_top_style_value_exact,
            az_style_border_top_style_value_delete,
            az_style_border_top_style_value_deep_copy,
            az_style_border_top_width_value_auto,
            az_style_border_top_width_value_none,
            az_style_border_top_width_value_inherit,
            az_style_border_top_width_value_initial,
            az_style_border_top_width_value_exact,
            az_style_border_top_width_value_delete,
            az_style_border_top_width_value_deep_copy,
            az_style_cursor_value_auto,
            az_style_cursor_value_none,
            az_style_cursor_value_inherit,
            az_style_cursor_value_initial,
            az_style_cursor_value_exact,
            az_style_cursor_value_delete,
            az_style_cursor_value_deep_copy,
            az_style_font_family_value_auto,
            az_style_font_family_value_none,
            az_style_font_family_value_inherit,
            az_style_font_family_value_initial,
            az_style_font_family_value_exact,
            az_style_font_family_value_delete,
            az_style_font_family_value_deep_copy,
            az_style_font_size_value_auto,
            az_style_font_size_value_none,
            az_style_font_size_value_inherit,
            az_style_font_size_value_initial,
            az_style_font_size_value_exact,
            az_style_font_size_value_delete,
            az_style_font_size_value_deep_copy,
            az_style_letter_spacing_value_auto,
            az_style_letter_spacing_value_none,
            az_style_letter_spacing_value_inherit,
            az_style_letter_spacing_value_initial,
            az_style_letter_spacing_value_exact,
            az_style_letter_spacing_value_delete,
            az_style_letter_spacing_value_deep_copy,
            az_style_line_height_value_auto,
            az_style_line_height_value_none,
            az_style_line_height_value_inherit,
            az_style_line_height_value_initial,
            az_style_line_height_value_exact,
            az_style_line_height_value_delete,
            az_style_line_height_value_deep_copy,
            az_style_tab_width_value_auto,
            az_style_tab_width_value_none,
            az_style_tab_width_value_inherit,
            az_style_tab_width_value_initial,
            az_style_tab_width_value_exact,
            az_style_tab_width_value_delete,
            az_style_tab_width_value_deep_copy,
            az_style_text_alignment_horz_value_auto,
            az_style_text_alignment_horz_value_none,
            az_style_text_alignment_horz_value_inherit,
            az_style_text_alignment_horz_value_initial,
            az_style_text_alignment_horz_value_exact,
            az_style_text_alignment_horz_value_delete,
            az_style_text_alignment_horz_value_deep_copy,
            az_style_text_color_value_auto,
            az_style_text_color_value_none,
            az_style_text_color_value_inherit,
            az_style_text_color_value_initial,
            az_style_text_color_value_exact,
            az_style_text_color_value_delete,
            az_style_text_color_value_deep_copy,
            az_style_word_spacing_value_auto,
            az_style_word_spacing_value_none,
            az_style_word_spacing_value_inherit,
            az_style_word_spacing_value_initial,
            az_style_word_spacing_value_exact,
            az_style_word_spacing_value_delete,
            az_style_word_spacing_value_deep_copy,
            az_css_property_text_color,
            az_css_property_font_size,
            az_css_property_font_family,
            az_css_property_text_align,
            az_css_property_letter_spacing,
            az_css_property_line_height,
            az_css_property_word_spacing,
            az_css_property_tab_width,
            az_css_property_cursor,
            az_css_property_display,
            az_css_property_float,
            az_css_property_box_sizing,
            az_css_property_width,
            az_css_property_height,
            az_css_property_min_width,
            az_css_property_min_height,
            az_css_property_max_width,
            az_css_property_max_height,
            az_css_property_position,
            az_css_property_top,
            az_css_property_right,
            az_css_property_left,
            az_css_property_bottom,
            az_css_property_flex_wrap,
            az_css_property_flex_direction,
            az_css_property_flex_grow,
            az_css_property_flex_shrink,
            az_css_property_justify_content,
            az_css_property_align_items,
            az_css_property_align_content,
            az_css_property_background_content,
            az_css_property_background_position,
            az_css_property_background_size,
            az_css_property_background_repeat,
            az_css_property_overflow_x,
            az_css_property_overflow_y,
            az_css_property_padding_top,
            az_css_property_padding_left,
            az_css_property_padding_right,
            az_css_property_padding_bottom,
            az_css_property_margin_top,
            az_css_property_margin_left,
            az_css_property_margin_right,
            az_css_property_margin_bottom,
            az_css_property_border_top_left_radius,
            az_css_property_border_top_right_radius,
            az_css_property_border_bottom_left_radius,
            az_css_property_border_bottom_right_radius,
            az_css_property_border_top_color,
            az_css_property_border_right_color,
            az_css_property_border_left_color,
            az_css_property_border_bottom_color,
            az_css_property_border_top_style,
            az_css_property_border_right_style,
            az_css_property_border_left_style,
            az_css_property_border_bottom_style,
            az_css_property_border_top_width,
            az_css_property_border_right_width,
            az_css_property_border_left_width,
            az_css_property_border_bottom_width,
            az_css_property_box_shadow_left,
            az_css_property_box_shadow_right,
            az_css_property_box_shadow_top,
            az_css_property_box_shadow_bottom,
            az_css_property_delete,
            az_css_property_deep_copy,
            az_dom_div,
            az_dom_body,
            az_dom_label,
            az_dom_text,
            az_dom_image,
            az_dom_gl_texture,
            az_dom_iframe_callback,
            az_dom_add_id,
            az_dom_with_id,
            az_dom_set_ids,
            az_dom_with_ids,
            az_dom_add_class,
            az_dom_with_class,
            az_dom_set_classes,
            az_dom_with_classes,
            az_dom_add_callback,
            az_dom_with_callback,
            az_dom_add_css_override,
            az_dom_with_css_override,
            az_dom_set_is_draggable,
            az_dom_is_draggable,
            az_dom_set_tab_index,
            az_dom_with_tab_index,
            az_dom_add_child,
            az_dom_with_child,
            az_dom_has_id,
            az_dom_has_class,
            az_dom_get_html_string,
            az_dom_delete,
            az_dom_shallow_copy,
            az_event_filter_hover,
            az_event_filter_not,
            az_event_filter_focus,
            az_event_filter_window,
            az_event_filter_delete,
            az_event_filter_deep_copy,
            az_hover_event_filter_mouse_over,
            az_hover_event_filter_mouse_down,
            az_hover_event_filter_left_mouse_down,
            az_hover_event_filter_right_mouse_down,
            az_hover_event_filter_middle_mouse_down,
            az_hover_event_filter_mouse_up,
            az_hover_event_filter_left_mouse_up,
            az_hover_event_filter_right_mouse_up,
            az_hover_event_filter_middle_mouse_up,
            az_hover_event_filter_mouse_enter,
            az_hover_event_filter_mouse_leave,
            az_hover_event_filter_scroll,
            az_hover_event_filter_scroll_start,
            az_hover_event_filter_scroll_end,
            az_hover_event_filter_text_input,
            az_hover_event_filter_virtual_key_down,
            az_hover_event_filter_virtual_key_up,
            az_hover_event_filter_hovered_file,
            az_hover_event_filter_dropped_file,
            az_hover_event_filter_hovered_file_cancelled,
            az_hover_event_filter_delete,
            az_hover_event_filter_deep_copy,
            az_focus_event_filter_mouse_over,
            az_focus_event_filter_mouse_down,
            az_focus_event_filter_left_mouse_down,
            az_focus_event_filter_right_mouse_down,
            az_focus_event_filter_middle_mouse_down,
            az_focus_event_filter_mouse_up,
            az_focus_event_filter_left_mouse_up,
            az_focus_event_filter_right_mouse_up,
            az_focus_event_filter_middle_mouse_up,
            az_focus_event_filter_mouse_enter,
            az_focus_event_filter_mouse_leave,
            az_focus_event_filter_scroll,
            az_focus_event_filter_scroll_start,
            az_focus_event_filter_scroll_end,
            az_focus_event_filter_text_input,
            az_focus_event_filter_virtual_key_down,
            az_focus_event_filter_virtual_key_up,
            az_focus_event_filter_focus_received,
            az_focus_event_filter_focus_lost,
            az_focus_event_filter_delete,
            az_focus_event_filter_deep_copy,
            az_not_event_filter_hover,
            az_not_event_filter_focus,
            az_not_event_filter_delete,
            az_not_event_filter_deep_copy,
            az_window_event_filter_mouse_over,
            az_window_event_filter_mouse_down,
            az_window_event_filter_left_mouse_down,
            az_window_event_filter_right_mouse_down,
            az_window_event_filter_middle_mouse_down,
            az_window_event_filter_mouse_up,
            az_window_event_filter_left_mouse_up,
            az_window_event_filter_right_mouse_up,
            az_window_event_filter_middle_mouse_up,
            az_window_event_filter_mouse_enter,
            az_window_event_filter_mouse_leave,
            az_window_event_filter_scroll,
            az_window_event_filter_scroll_start,
            az_window_event_filter_scroll_end,
            az_window_event_filter_text_input,
            az_window_event_filter_virtual_key_down,
            az_window_event_filter_virtual_key_up,
            az_window_event_filter_hovered_file,
            az_window_event_filter_dropped_file,
            az_window_event_filter_hovered_file_cancelled,
            az_window_event_filter_delete,
            az_window_event_filter_deep_copy,
            az_tab_index_auto,
            az_tab_index_override_in_parent,
            az_tab_index_no_keyboard_focus,
            az_tab_index_delete,
            az_tab_index_deep_copy,
            az_text_id_new,
            az_text_id_delete,
            az_text_id_deep_copy,
            az_image_id_new,
            az_image_id_delete,
            az_image_id_deep_copy,
            az_font_id_new,
            az_font_id_delete,
            az_font_id_deep_copy,
            az_image_source_embedded,
            az_image_source_file,
            az_image_source_raw,
            az_image_source_delete,
            az_image_source_deep_copy,
            az_font_source_embedded,
            az_font_source_file,
            az_font_source_system,
            az_font_source_delete,
            az_font_source_deep_copy,
            az_raw_image_new,
            az_raw_image_delete,
            az_raw_image_shallow_copy,
            az_raw_image_format_r8,
            az_raw_image_format_r16,
            az_raw_image_format_rg16,
            az_raw_image_format_bgra8,
            az_raw_image_format_rgbaf32,
            az_raw_image_format_rg8,
            az_raw_image_format_rgbai32,
            az_raw_image_format_rgba8,
            az_raw_image_format_delete,
            az_raw_image_format_deep_copy,
            az_window_create_options_new,
            az_window_create_options_delete,
            az_window_create_options_shallow_copy,
        })
    }
}

/// Module to re-export common structs (`App`, `AppConfig`, `Css`, `Dom`, `WindowCreateOptions`, `RefAny`, `LayoutInfo`)
pub mod prelude {
    pub use crate::{
        app::{App, AppConfig},
        css::Css,
        dom::Dom,
        window::WindowCreateOptions,
        callbacks::{RefAny, LayoutInfo},
    };
}

/// Definition of azuls internal String type + functions for conversion from `std::String`
#[allow(dead_code, unused_imports)]
pub mod str {

    use azul_dll::*;

    impl From<std::string::String> for crate::str::String {
        fn from(s: std::string::String) -> crate::str::String {
            crate::str::String::from_utf8_unchecked(s.as_ptr(), s.len()) // - copies s into a new String
            // - s is deallocated here
        }
    }

    /// `String` struct
    pub struct String { pub(crate) object: AzString }

    impl String {
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn from_utf8_unchecked(ptr: *const u8, len: usize) -> Self { Self { object: az_string_from_utf8_unchecked(ptr, len) } }
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn from_utf8_lossy(ptr: *const u8, len: usize) -> Self { Self { object: az_string_from_utf8_lossy(ptr, len) } }
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn into_bytes(self)  -> crate::vec::U8Vec { crate::vec::U8Vec { object: { az_string_into_bytes(self.leak())} } }
       /// Prevents the destructor from running and returns the internal `AzString`
       pub fn leak(self) -> AzString { az_string_deep_copy(&self.object) }
    }

    impl Drop for String { fn drop(&mut self) { az_string_delete(&mut self.object); } }
}

/// Definition of azuls internal `U8Vec` type + functions for conversion from `std::Vec`
#[allow(dead_code, unused_imports)]
pub mod vec {

    use azul_dll::*;

    impl From<std::vec::Vec<u8>> for crate::vec::U8Vec {
        fn from(v: std::vec::Vec<u8>) -> crate::vec::U8Vec {
            crate::vec::U8Vec::copy_from(v.as_ptr(), v.len())
        }
    }

    impl From<crate::vec::U8Vec> for std::vec::Vec<u8> {
        fn from(v: crate::vec::U8Vec) -> std::vec::Vec<u8> {
            unsafe { std::slice::from_raw_parts(v.object.object.as_ptr(), v.object.object.len()).to_vec() }
        }
    }

    impl From<std::vec::Vec<std::string::String>> for crate::vec::StringVec {
        fn from(v: std::vec::Vec<std::string::String>) -> crate::vec::StringVec {
            let vec: Vec<AzString> = v.into_iter().map(|i| {
                let i: std::vec::Vec<u8> = i.into_bytes();
                az_string_from_utf8_unchecked(i.as_ptr(), i.len())
            }).collect();

            crate::vec::StringVec { object: az_string_vec_copy_from(vec.as_ptr(), vec.len()) }
        }
    }

    impl From<crate::vec::StringVec> for std::vec::Vec<std::string::String> {
        fn from(v: crate::vec::StringVec) -> std::vec::Vec<std::string::String> {
            v.leak().object
            .into_iter()
            .map(|s| unsafe {
                let s_vec: std::vec::Vec<u8> = s.into_bytes().into();
                std::string::String::from_utf8_unchecked(s_vec)
            })
            .collect()

            // delete() not necessary because StringVec is stack-allocated
        }
    }    use crate::str::String;


    /// Wrapper over a Rust-allocated `Vec<u8>`
    pub struct U8Vec { pub(crate) object: AzU8Vec }

    impl U8Vec {
        /// Creates + allocates a Rust `Vec<u8>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u8, len: usize) -> Self { Self { object: az_u8_vec_copy_from(ptr, len) } }
        /// Returns the internal pointer to the start of the heap-allocated `[u8]`
        pub fn as_ptr(&self)  -> *const u8 { az_u8_vec_as_ptr(&self.object) }
        /// Returns the length of bytes in the heap-allocated `[u8]`
        pub fn len(&self)  -> usize { az_u8_vec_len(&self.object) }
       /// Prevents the destructor from running and returns the internal `AzU8Vec`
       pub fn leak(self) -> AzU8Vec { az_u8_vec_deep_copy(&self.object) }
    }

    impl Drop for U8Vec { fn drop(&mut self) { az_u8_vec_delete(&mut self.object); } }


    /// Wrapper over a Rust-allocated `Vec<String>`
    pub struct StringVec { pub(crate) object: AzStringVec }

    impl StringVec {
        /// Creates + allocates a Rust `Vec<String>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzString, len: usize) -> Self { Self { object: az_string_vec_copy_from(ptr, len) } }
       /// Prevents the destructor from running and returns the internal `AzStringVec`
       pub fn leak(self) -> AzStringVec { az_string_vec_deep_copy(&self.object) }
    }

    impl Drop for StringVec { fn drop(&mut self) { az_string_vec_delete(&mut self.object); } }
}

/// Definition of azuls internal `PathBuf` type + functions for conversion from `std::PathBuf`
#[allow(dead_code, unused_imports)]
pub mod path {

    use azul_dll::*;
    use crate::str::String;


    /// Wrapper over a Rust-allocated `PathBuf`
    pub struct PathBuf { pub(crate) ptr: AzPathBufPtr }

    impl PathBuf {
        /// Creates a new PathBuf from a String
        pub fn new(path: String) -> Self { Self { ptr: az_path_buf_new(path.leak()) } }
       /// Prevents the destructor from running and returns the internal `AzPathBufPtr`
       pub fn leak(self) -> AzPathBufPtr { let p = az_path_buf_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for PathBuf { fn drop(&mut self) { az_path_buf_delete(&mut self.ptr); } }
}

/// `App` construction and configuration
#[allow(dead_code, unused_imports)]
pub mod app {

    use azul_dll::*;
    use crate::callbacks::{LayoutCallback, RefAny};
    use crate::window::WindowCreateOptions;


    /// `AppConfig` struct
    pub struct AppConfig { pub(crate) ptr: AzAppConfigPtr }

    impl AppConfig {
        /// Creates a new AppConfig with default values
        pub fn default() -> Self { Self { ptr: az_app_config_default() } }
       /// Prevents the destructor from running and returns the internal `AzAppConfigPtr`
       pub fn leak(self) -> AzAppConfigPtr { let p = az_app_config_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for AppConfig { fn drop(&mut self) { az_app_config_delete(&mut self.ptr); } }


    /// `App` struct
    pub struct App { pub(crate) ptr: AzAppPtr }

    impl App {
        /// Creates a new App instance from the given `AppConfig`
        pub fn new(data: RefAny, config: AppConfig, callback: LayoutCallback) -> Self { 
            unsafe { crate::callbacks::CALLBACK = callback };
            Self {
                ptr: az_app_new(data.leak(), config.leak(), crate::callbacks::translate_callback)
            }
 }
        /// Runs the application. Due to platform restrictions (specifically `WinMain` on Windows), this function never returns.
        pub fn run(self, window: WindowCreateOptions)  { az_app_run(self.leak(), window.leak()) }
       /// Prevents the destructor from running and returns the internal `AzAppPtr`
       pub fn leak(self) -> AzAppPtr { let p = az_app_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for App { fn drop(&mut self) { az_app_delete(&mut self.ptr); } }
}

/// Callback type definitions + struct definitions of `CallbackInfo`s
#[allow(dead_code, unused_imports)]
pub mod callbacks {

    use azul_dll::*;


    use crate::dom::Dom;

    /// Callback fn that returns the layout
    pub type LayoutCallback = fn(RefAny, LayoutInfo) -> Dom;

    fn default_callback(_: RefAny, _: LayoutInfo) -> Dom {
        Dom::div()
    }

    pub(crate) static mut CALLBACK: LayoutCallback = default_callback;

    pub(crate) fn translate_callback(data: azul_dll::AzRefAny, layout: azul_dll::AzLayoutInfoPtr) -> azul_dll::AzDomPtr {
        unsafe { CALLBACK(RefAny(data), LayoutInfo { ptr: layout }) }.leak()
    }


/// Return type of a regular callback - currently `AzUpdateScreen`
pub type CallbackReturn = AzUpdateScreen;
/// Callback for responding to window events
pub type Callback = fn(AzCallbackInfoPtr) -> AzCallbackReturn;

    /// `CallbackInfo` struct
    pub struct CallbackInfo { pub(crate) ptr: AzCallbackInfoPtr }

    impl CallbackInfo {
       /// Prevents the destructor from running and returns the internal `AzCallbackInfoPtr`
       pub fn leak(self) -> AzCallbackInfoPtr { let p = az_callback_info_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for CallbackInfo { fn drop(&mut self) { az_callback_info_delete(&mut self.ptr); } }


    /// `UpdateScreen` struct
    pub struct UpdateScreen { pub(crate) object: AzUpdateScreen }

    impl<T> From<Option<T>> for UpdateScreen { fn from(o: Option<T>) -> Self { Self { object: match o { None => AzDontRedraw, Some(_) => AzRedraw }} } }


    /// `Redraw` struct
    pub static REDRAW: AzUpdateScreen = AzRedraw;



    /// `DontRedraw` struct
    pub static DONT_REDRAW: AzUpdateScreen = AzDontRedraw;



/// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
pub type IFrameCallback = fn(AzIFrameCallbackInfoPtr) -> AzIFrameCallbackReturnPtr;

    /// `IFrameCallbackInfo` struct
    pub struct IFrameCallbackInfo { pub(crate) ptr: AzIFrameCallbackInfoPtr }

    impl IFrameCallbackInfo {
       /// Prevents the destructor from running and returns the internal `AzIFrameCallbackInfoPtr`
       pub fn leak(self) -> AzIFrameCallbackInfoPtr { let p = az_i_frame_callback_info_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for IFrameCallbackInfo { fn drop(&mut self) { az_i_frame_callback_info_delete(&mut self.ptr); } }


    /// `IFrameCallbackReturn` struct
    pub struct IFrameCallbackReturn { pub(crate) ptr: AzIFrameCallbackReturnPtr }

    impl IFrameCallbackReturn {
       /// Prevents the destructor from running and returns the internal `AzIFrameCallbackReturnPtr`
       pub fn leak(self) -> AzIFrameCallbackReturnPtr { let p = az_i_frame_callback_return_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for IFrameCallbackReturn { fn drop(&mut self) { az_i_frame_callback_return_delete(&mut self.ptr); } }


/// Callback for rendering to an OpenGL texture
pub type GlCallback = fn(AzGlCallbackInfoPtr) -> AzGlCallbackReturnPtr;

    /// `GlCallbackInfo` struct
    pub struct GlCallbackInfo { pub(crate) ptr: AzGlCallbackInfoPtr }

    impl GlCallbackInfo {
       /// Prevents the destructor from running and returns the internal `AzGlCallbackInfoPtr`
       pub fn leak(self) -> AzGlCallbackInfoPtr { let p = az_gl_callback_info_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for GlCallbackInfo { fn drop(&mut self) { az_gl_callback_info_delete(&mut self.ptr); } }


    /// `GlCallbackReturn` struct
    pub struct GlCallbackReturn { pub(crate) ptr: AzGlCallbackReturnPtr }

    impl GlCallbackReturn {
       /// Prevents the destructor from running and returns the internal `AzGlCallbackReturnPtr`
       pub fn leak(self) -> AzGlCallbackReturnPtr { let p = az_gl_callback_return_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for GlCallbackReturn { fn drop(&mut self) { az_gl_callback_return_delete(&mut self.ptr); } }


    use azul_dll::AzRefAny as AzRefAnyCore;

    /// `RefAny` struct
    #[repr(transparent)]
    pub struct RefAny(pub(crate) AzRefAnyCore);

    impl Clone for RefAny {
        fn clone(&self) -> Self {
            RefAny(az_ref_any_shallow_copy(&self.0))
        }
    }

    impl RefAny {

        /// Creates a new, type-erased pointer by casting the `T` value into a `Vec<u8>` and saving the length + type ID
        pub fn new<T: 'static>(value: T) -> Self {
            use azul_dll::*;

            fn default_custom_destructor<U: 'static>(ptr: AzRefAnyCore) {
                use std::{mem, ptr};

                // note: in the default constructor, we do not need to check whether U == T

                unsafe {
                    // copy the struct from the heap to the stack and call mem::drop on U to run the destructor
                    let mut stack_mem = mem::MaybeUninit::<U>::uninit().assume_init();
                    ptr::copy_nonoverlapping(ptr._internal_ptr as *const u8, &mut stack_mem as *mut U as *mut u8, mem::size_of::<U>().min(ptr._internal_len));
                    mem::drop(stack_mem);
                }
            }

            let type_name_str = ::std::any::type_name::<T>();
            let s = az_ref_any_new(
                (&value as *const T) as *const u8,
                ::std::mem::size_of::<T>(),
                Self::get_type_id::<T>() as u64,
                crate::str::String::from_utf8_unchecked(type_name_str.as_ptr(), type_name_str.len()).leak(),
                default_custom_destructor::<T>,
            );
            ::std::mem::forget(value); // do not run the destructor of T here!
            Self(s)
        }

        /// Returns the inner `AzRefAnyCore`
        pub fn leak(self) -> AzRefAnyCore {
            use std::mem;
            let s = az_ref_any_core_copy(&self.0);
            mem::forget(self); // do not run destructor
            s
        }

        /// Downcasts the type-erased pointer to a type `&U`, returns `None` if the types don't match
        #[inline]
        pub fn downcast_ref<'a, U: 'static>(&'a self) -> Option<&'a U> {
            use std::ptr;
            let ptr = az_ref_any_get_ptr(&self.0, self.0._internal_len, Self::get_type_id::<U>());
            if ptr == ptr::null() { None } else { Some(unsafe { &*(self.0._internal_ptr as *const U) as &'a U }) }
        }

        /// Downcasts the type-erased pointer to a type `&mut U`, returns `None` if the types don't match
        #[inline]
        pub fn downcast_mut<'a, U: 'static>(&'a mut self) -> Option<&'a mut U> {
            use std::ptr;
            let ptr = az_ref_any_get_mut_ptr(&self.0, self.0._internal_len, Self::get_type_id::<U>());
            if ptr == ptr::null_mut() { None } else { Some(unsafe { &mut *(self.0._internal_ptr as *mut U) as &'a mut U }) }
        }

        #[inline]
        fn get_type_id<T: 'static>() -> u64 {
            use std::any::TypeId;
            use std::mem;

            // fast method to serialize the type id into a u64
            let t_id = TypeId::of::<T>();
            let struct_as_bytes = unsafe { ::std::slice::from_raw_parts((&t_id as *const TypeId) as *const u8, mem::size_of::<TypeId>()) };
            struct_as_bytes.into_iter().enumerate().map(|(s_pos, s)| ((*s as u64) << s_pos)).sum()
        }
    }

    impl Drop for RefAny {
        fn drop(&mut self) {
            az_ref_any_delete(&mut self.0);
        }
    }


    /// `LayoutInfo` struct
    pub struct LayoutInfo { pub(crate) ptr: AzLayoutInfoPtr }

    impl LayoutInfo {
       /// Prevents the destructor from running and returns the internal `AzLayoutInfoPtr`
       pub fn leak(self) -> AzLayoutInfoPtr { let p = az_layout_info_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutInfo { fn drop(&mut self) { az_layout_info_delete(&mut self.ptr); } }
}

/// `Css` parsing module
#[allow(dead_code, unused_imports)]
pub mod css {

    use azul_dll::*;


    /// `Css` struct
    pub struct Css { pub(crate) ptr: AzCssPtr }

    impl Css {
        /// Loads the native style for the given operating system
        pub fn native() -> Self { Self { ptr: az_css_native() } }
        /// Returns an empty CSS style
        pub fn empty() -> Self { Self { ptr: az_css_empty() } }
       /// Prevents the destructor from running and returns the internal `AzCssPtr`
       pub fn leak(self) -> AzCssPtr { let p = az_css_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for Css { fn drop(&mut self) { az_css_delete(&mut self.ptr); } }


    /// `BoxShadowPreDisplayItem` struct
    pub struct BoxShadowPreDisplayItem { pub(crate) ptr: AzBoxShadowPreDisplayItemPtr }

    impl BoxShadowPreDisplayItem {
       /// Prevents the destructor from running and returns the internal `AzBoxShadowPreDisplayItemPtr`
       pub fn leak(self) -> AzBoxShadowPreDisplayItemPtr { let p = az_box_shadow_pre_display_item_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for BoxShadowPreDisplayItem { fn drop(&mut self) { az_box_shadow_pre_display_item_delete(&mut self.ptr); } }


    /// `LayoutAlignContent` struct
    pub struct LayoutAlignContent { pub(crate) ptr: AzLayoutAlignContentPtr }

    impl LayoutAlignContent {
       /// Prevents the destructor from running and returns the internal `AzLayoutAlignContentPtr`
       pub fn leak(self) -> AzLayoutAlignContentPtr { let p = az_layout_align_content_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutAlignContent { fn drop(&mut self) { az_layout_align_content_delete(&mut self.ptr); } }


    /// `LayoutAlignItems` struct
    pub struct LayoutAlignItems { pub(crate) ptr: AzLayoutAlignItemsPtr }

    impl LayoutAlignItems {
       /// Prevents the destructor from running and returns the internal `AzLayoutAlignItemsPtr`
       pub fn leak(self) -> AzLayoutAlignItemsPtr { let p = az_layout_align_items_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutAlignItems { fn drop(&mut self) { az_layout_align_items_delete(&mut self.ptr); } }


    /// `LayoutBottom` struct
    pub struct LayoutBottom { pub(crate) ptr: AzLayoutBottomPtr }

    impl LayoutBottom {
       /// Prevents the destructor from running and returns the internal `AzLayoutBottomPtr`
       pub fn leak(self) -> AzLayoutBottomPtr { let p = az_layout_bottom_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutBottom { fn drop(&mut self) { az_layout_bottom_delete(&mut self.ptr); } }


    /// `LayoutBoxSizing` struct
    pub struct LayoutBoxSizing { pub(crate) ptr: AzLayoutBoxSizingPtr }

    impl LayoutBoxSizing {
       /// Prevents the destructor from running and returns the internal `AzLayoutBoxSizingPtr`
       pub fn leak(self) -> AzLayoutBoxSizingPtr { let p = az_layout_box_sizing_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutBoxSizing { fn drop(&mut self) { az_layout_box_sizing_delete(&mut self.ptr); } }


    /// `LayoutDirection` struct
    pub struct LayoutDirection { pub(crate) ptr: AzLayoutDirectionPtr }

    impl LayoutDirection {
       /// Prevents the destructor from running and returns the internal `AzLayoutDirectionPtr`
       pub fn leak(self) -> AzLayoutDirectionPtr { let p = az_layout_direction_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutDirection { fn drop(&mut self) { az_layout_direction_delete(&mut self.ptr); } }


    /// `LayoutDisplay` struct
    pub struct LayoutDisplay { pub(crate) ptr: AzLayoutDisplayPtr }

    impl LayoutDisplay {
       /// Prevents the destructor from running and returns the internal `AzLayoutDisplayPtr`
       pub fn leak(self) -> AzLayoutDisplayPtr { let p = az_layout_display_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutDisplay { fn drop(&mut self) { az_layout_display_delete(&mut self.ptr); } }


    /// `LayoutFlexGrow` struct
    pub struct LayoutFlexGrow { pub(crate) ptr: AzLayoutFlexGrowPtr }

    impl LayoutFlexGrow {
       /// Prevents the destructor from running and returns the internal `AzLayoutFlexGrowPtr`
       pub fn leak(self) -> AzLayoutFlexGrowPtr { let p = az_layout_flex_grow_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutFlexGrow { fn drop(&mut self) { az_layout_flex_grow_delete(&mut self.ptr); } }


    /// `LayoutFlexShrink` struct
    pub struct LayoutFlexShrink { pub(crate) ptr: AzLayoutFlexShrinkPtr }

    impl LayoutFlexShrink {
       /// Prevents the destructor from running and returns the internal `AzLayoutFlexShrinkPtr`
       pub fn leak(self) -> AzLayoutFlexShrinkPtr { let p = az_layout_flex_shrink_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutFlexShrink { fn drop(&mut self) { az_layout_flex_shrink_delete(&mut self.ptr); } }


    /// `LayoutFloat` struct
    pub struct LayoutFloat { pub(crate) ptr: AzLayoutFloatPtr }

    impl LayoutFloat {
       /// Prevents the destructor from running and returns the internal `AzLayoutFloatPtr`
       pub fn leak(self) -> AzLayoutFloatPtr { let p = az_layout_float_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutFloat { fn drop(&mut self) { az_layout_float_delete(&mut self.ptr); } }


    /// `LayoutHeight` struct
    pub struct LayoutHeight { pub(crate) ptr: AzLayoutHeightPtr }

    impl LayoutHeight {
       /// Prevents the destructor from running and returns the internal `AzLayoutHeightPtr`
       pub fn leak(self) -> AzLayoutHeightPtr { let p = az_layout_height_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutHeight { fn drop(&mut self) { az_layout_height_delete(&mut self.ptr); } }


    /// `LayoutJustifyContent` struct
    pub struct LayoutJustifyContent { pub(crate) ptr: AzLayoutJustifyContentPtr }

    impl LayoutJustifyContent {
       /// Prevents the destructor from running and returns the internal `AzLayoutJustifyContentPtr`
       pub fn leak(self) -> AzLayoutJustifyContentPtr { let p = az_layout_justify_content_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutJustifyContent { fn drop(&mut self) { az_layout_justify_content_delete(&mut self.ptr); } }


    /// `LayoutLeft` struct
    pub struct LayoutLeft { pub(crate) ptr: AzLayoutLeftPtr }

    impl LayoutLeft {
       /// Prevents the destructor from running and returns the internal `AzLayoutLeftPtr`
       pub fn leak(self) -> AzLayoutLeftPtr { let p = az_layout_left_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutLeft { fn drop(&mut self) { az_layout_left_delete(&mut self.ptr); } }


    /// `LayoutMarginBottom` struct
    pub struct LayoutMarginBottom { pub(crate) ptr: AzLayoutMarginBottomPtr }

    impl LayoutMarginBottom {
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginBottomPtr`
       pub fn leak(self) -> AzLayoutMarginBottomPtr { let p = az_layout_margin_bottom_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutMarginBottom { fn drop(&mut self) { az_layout_margin_bottom_delete(&mut self.ptr); } }


    /// `LayoutMarginLeft` struct
    pub struct LayoutMarginLeft { pub(crate) ptr: AzLayoutMarginLeftPtr }

    impl LayoutMarginLeft {
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginLeftPtr`
       pub fn leak(self) -> AzLayoutMarginLeftPtr { let p = az_layout_margin_left_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutMarginLeft { fn drop(&mut self) { az_layout_margin_left_delete(&mut self.ptr); } }


    /// `LayoutMarginRight` struct
    pub struct LayoutMarginRight { pub(crate) ptr: AzLayoutMarginRightPtr }

    impl LayoutMarginRight {
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginRightPtr`
       pub fn leak(self) -> AzLayoutMarginRightPtr { let p = az_layout_margin_right_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutMarginRight { fn drop(&mut self) { az_layout_margin_right_delete(&mut self.ptr); } }


    /// `LayoutMarginTop` struct
    pub struct LayoutMarginTop { pub(crate) ptr: AzLayoutMarginTopPtr }

    impl LayoutMarginTop {
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginTopPtr`
       pub fn leak(self) -> AzLayoutMarginTopPtr { let p = az_layout_margin_top_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutMarginTop { fn drop(&mut self) { az_layout_margin_top_delete(&mut self.ptr); } }


    /// `LayoutMaxHeight` struct
    pub struct LayoutMaxHeight { pub(crate) ptr: AzLayoutMaxHeightPtr }

    impl LayoutMaxHeight {
       /// Prevents the destructor from running and returns the internal `AzLayoutMaxHeightPtr`
       pub fn leak(self) -> AzLayoutMaxHeightPtr { let p = az_layout_max_height_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutMaxHeight { fn drop(&mut self) { az_layout_max_height_delete(&mut self.ptr); } }


    /// `LayoutMaxWidth` struct
    pub struct LayoutMaxWidth { pub(crate) ptr: AzLayoutMaxWidthPtr }

    impl LayoutMaxWidth {
       /// Prevents the destructor from running and returns the internal `AzLayoutMaxWidthPtr`
       pub fn leak(self) -> AzLayoutMaxWidthPtr { let p = az_layout_max_width_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutMaxWidth { fn drop(&mut self) { az_layout_max_width_delete(&mut self.ptr); } }


    /// `LayoutMinHeight` struct
    pub struct LayoutMinHeight { pub(crate) ptr: AzLayoutMinHeightPtr }

    impl LayoutMinHeight {
       /// Prevents the destructor from running and returns the internal `AzLayoutMinHeightPtr`
       pub fn leak(self) -> AzLayoutMinHeightPtr { let p = az_layout_min_height_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutMinHeight { fn drop(&mut self) { az_layout_min_height_delete(&mut self.ptr); } }


    /// `LayoutMinWidth` struct
    pub struct LayoutMinWidth { pub(crate) ptr: AzLayoutMinWidthPtr }

    impl LayoutMinWidth {
       /// Prevents the destructor from running and returns the internal `AzLayoutMinWidthPtr`
       pub fn leak(self) -> AzLayoutMinWidthPtr { let p = az_layout_min_width_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutMinWidth { fn drop(&mut self) { az_layout_min_width_delete(&mut self.ptr); } }


    /// `LayoutPaddingBottom` struct
    pub struct LayoutPaddingBottom { pub(crate) ptr: AzLayoutPaddingBottomPtr }

    impl LayoutPaddingBottom {
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingBottomPtr`
       pub fn leak(self) -> AzLayoutPaddingBottomPtr { let p = az_layout_padding_bottom_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutPaddingBottom { fn drop(&mut self) { az_layout_padding_bottom_delete(&mut self.ptr); } }


    /// `LayoutPaddingLeft` struct
    pub struct LayoutPaddingLeft { pub(crate) ptr: AzLayoutPaddingLeftPtr }

    impl LayoutPaddingLeft {
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingLeftPtr`
       pub fn leak(self) -> AzLayoutPaddingLeftPtr { let p = az_layout_padding_left_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutPaddingLeft { fn drop(&mut self) { az_layout_padding_left_delete(&mut self.ptr); } }


    /// `LayoutPaddingRight` struct
    pub struct LayoutPaddingRight { pub(crate) ptr: AzLayoutPaddingRightPtr }

    impl LayoutPaddingRight {
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingRightPtr`
       pub fn leak(self) -> AzLayoutPaddingRightPtr { let p = az_layout_padding_right_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutPaddingRight { fn drop(&mut self) { az_layout_padding_right_delete(&mut self.ptr); } }


    /// `LayoutPaddingTop` struct
    pub struct LayoutPaddingTop { pub(crate) ptr: AzLayoutPaddingTopPtr }

    impl LayoutPaddingTop {
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingTopPtr`
       pub fn leak(self) -> AzLayoutPaddingTopPtr { let p = az_layout_padding_top_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutPaddingTop { fn drop(&mut self) { az_layout_padding_top_delete(&mut self.ptr); } }


    /// `LayoutPosition` struct
    pub struct LayoutPosition { pub(crate) ptr: AzLayoutPositionPtr }

    impl LayoutPosition {
       /// Prevents the destructor from running and returns the internal `AzLayoutPositionPtr`
       pub fn leak(self) -> AzLayoutPositionPtr { let p = az_layout_position_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutPosition { fn drop(&mut self) { az_layout_position_delete(&mut self.ptr); } }


    /// `LayoutRight` struct
    pub struct LayoutRight { pub(crate) ptr: AzLayoutRightPtr }

    impl LayoutRight {
       /// Prevents the destructor from running and returns the internal `AzLayoutRightPtr`
       pub fn leak(self) -> AzLayoutRightPtr { let p = az_layout_right_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutRight { fn drop(&mut self) { az_layout_right_delete(&mut self.ptr); } }


    /// `LayoutTop` struct
    pub struct LayoutTop { pub(crate) ptr: AzLayoutTopPtr }

    impl LayoutTop {
       /// Prevents the destructor from running and returns the internal `AzLayoutTopPtr`
       pub fn leak(self) -> AzLayoutTopPtr { let p = az_layout_top_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutTop { fn drop(&mut self) { az_layout_top_delete(&mut self.ptr); } }


    /// `LayoutWidth` struct
    pub struct LayoutWidth { pub(crate) ptr: AzLayoutWidthPtr }

    impl LayoutWidth {
       /// Prevents the destructor from running and returns the internal `AzLayoutWidthPtr`
       pub fn leak(self) -> AzLayoutWidthPtr { let p = az_layout_width_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutWidth { fn drop(&mut self) { az_layout_width_delete(&mut self.ptr); } }


    /// `LayoutWrap` struct
    pub struct LayoutWrap { pub(crate) ptr: AzLayoutWrapPtr }

    impl LayoutWrap {
       /// Prevents the destructor from running and returns the internal `AzLayoutWrapPtr`
       pub fn leak(self) -> AzLayoutWrapPtr { let p = az_layout_wrap_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutWrap { fn drop(&mut self) { az_layout_wrap_delete(&mut self.ptr); } }


    /// `Overflow` struct
    pub struct Overflow { pub(crate) ptr: AzOverflowPtr }

    impl Overflow {
       /// Prevents the destructor from running and returns the internal `AzOverflowPtr`
       pub fn leak(self) -> AzOverflowPtr { let p = az_overflow_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for Overflow { fn drop(&mut self) { az_overflow_delete(&mut self.ptr); } }


    /// `StyleBackgroundContent` struct
    pub struct StyleBackgroundContent { pub(crate) ptr: AzStyleBackgroundContentPtr }

    impl StyleBackgroundContent {
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundContentPtr`
       pub fn leak(self) -> AzStyleBackgroundContentPtr { let p = az_style_background_content_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBackgroundContent { fn drop(&mut self) { az_style_background_content_delete(&mut self.ptr); } }


    /// `StyleBackgroundPosition` struct
    pub struct StyleBackgroundPosition { pub(crate) ptr: AzStyleBackgroundPositionPtr }

    impl StyleBackgroundPosition {
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundPositionPtr`
       pub fn leak(self) -> AzStyleBackgroundPositionPtr { let p = az_style_background_position_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBackgroundPosition { fn drop(&mut self) { az_style_background_position_delete(&mut self.ptr); } }


    /// `StyleBackgroundRepeat` struct
    pub struct StyleBackgroundRepeat { pub(crate) ptr: AzStyleBackgroundRepeatPtr }

    impl StyleBackgroundRepeat {
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundRepeatPtr`
       pub fn leak(self) -> AzStyleBackgroundRepeatPtr { let p = az_style_background_repeat_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBackgroundRepeat { fn drop(&mut self) { az_style_background_repeat_delete(&mut self.ptr); } }


    /// `StyleBackgroundSize` struct
    pub struct StyleBackgroundSize { pub(crate) ptr: AzStyleBackgroundSizePtr }

    impl StyleBackgroundSize {
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundSizePtr`
       pub fn leak(self) -> AzStyleBackgroundSizePtr { let p = az_style_background_size_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBackgroundSize { fn drop(&mut self) { az_style_background_size_delete(&mut self.ptr); } }


    /// `StyleBorderBottomColor` struct
    pub struct StyleBorderBottomColor { pub(crate) ptr: AzStyleBorderBottomColorPtr }

    impl StyleBorderBottomColor {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomColorPtr`
       pub fn leak(self) -> AzStyleBorderBottomColorPtr { let p = az_style_border_bottom_color_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderBottomColor { fn drop(&mut self) { az_style_border_bottom_color_delete(&mut self.ptr); } }


    /// `StyleBorderBottomLeftRadius` struct
    pub struct StyleBorderBottomLeftRadius { pub(crate) ptr: AzStyleBorderBottomLeftRadiusPtr }

    impl StyleBorderBottomLeftRadius {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomLeftRadiusPtr`
       pub fn leak(self) -> AzStyleBorderBottomLeftRadiusPtr { let p = az_style_border_bottom_left_radius_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderBottomLeftRadius { fn drop(&mut self) { az_style_border_bottom_left_radius_delete(&mut self.ptr); } }


    /// `StyleBorderBottomRightRadius` struct
    pub struct StyleBorderBottomRightRadius { pub(crate) ptr: AzStyleBorderBottomRightRadiusPtr }

    impl StyleBorderBottomRightRadius {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomRightRadiusPtr`
       pub fn leak(self) -> AzStyleBorderBottomRightRadiusPtr { let p = az_style_border_bottom_right_radius_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderBottomRightRadius { fn drop(&mut self) { az_style_border_bottom_right_radius_delete(&mut self.ptr); } }


    /// `StyleBorderBottomStyle` struct
    pub struct StyleBorderBottomStyle { pub(crate) ptr: AzStyleBorderBottomStylePtr }

    impl StyleBorderBottomStyle {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomStylePtr`
       pub fn leak(self) -> AzStyleBorderBottomStylePtr { let p = az_style_border_bottom_style_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderBottomStyle { fn drop(&mut self) { az_style_border_bottom_style_delete(&mut self.ptr); } }


    /// `StyleBorderBottomWidth` struct
    pub struct StyleBorderBottomWidth { pub(crate) ptr: AzStyleBorderBottomWidthPtr }

    impl StyleBorderBottomWidth {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomWidthPtr`
       pub fn leak(self) -> AzStyleBorderBottomWidthPtr { let p = az_style_border_bottom_width_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderBottomWidth { fn drop(&mut self) { az_style_border_bottom_width_delete(&mut self.ptr); } }


    /// `StyleBorderLeftColor` struct
    pub struct StyleBorderLeftColor { pub(crate) ptr: AzStyleBorderLeftColorPtr }

    impl StyleBorderLeftColor {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderLeftColorPtr`
       pub fn leak(self) -> AzStyleBorderLeftColorPtr { let p = az_style_border_left_color_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderLeftColor { fn drop(&mut self) { az_style_border_left_color_delete(&mut self.ptr); } }


    /// `StyleBorderLeftStyle` struct
    pub struct StyleBorderLeftStyle { pub(crate) ptr: AzStyleBorderLeftStylePtr }

    impl StyleBorderLeftStyle {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderLeftStylePtr`
       pub fn leak(self) -> AzStyleBorderLeftStylePtr { let p = az_style_border_left_style_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderLeftStyle { fn drop(&mut self) { az_style_border_left_style_delete(&mut self.ptr); } }


    /// `StyleBorderLeftWidth` struct
    pub struct StyleBorderLeftWidth { pub(crate) ptr: AzStyleBorderLeftWidthPtr }

    impl StyleBorderLeftWidth {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderLeftWidthPtr`
       pub fn leak(self) -> AzStyleBorderLeftWidthPtr { let p = az_style_border_left_width_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderLeftWidth { fn drop(&mut self) { az_style_border_left_width_delete(&mut self.ptr); } }


    /// `StyleBorderRightColor` struct
    pub struct StyleBorderRightColor { pub(crate) ptr: AzStyleBorderRightColorPtr }

    impl StyleBorderRightColor {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderRightColorPtr`
       pub fn leak(self) -> AzStyleBorderRightColorPtr { let p = az_style_border_right_color_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderRightColor { fn drop(&mut self) { az_style_border_right_color_delete(&mut self.ptr); } }


    /// `StyleBorderRightStyle` struct
    pub struct StyleBorderRightStyle { pub(crate) ptr: AzStyleBorderRightStylePtr }

    impl StyleBorderRightStyle {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderRightStylePtr`
       pub fn leak(self) -> AzStyleBorderRightStylePtr { let p = az_style_border_right_style_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderRightStyle { fn drop(&mut self) { az_style_border_right_style_delete(&mut self.ptr); } }


    /// `StyleBorderRightWidth` struct
    pub struct StyleBorderRightWidth { pub(crate) ptr: AzStyleBorderRightWidthPtr }

    impl StyleBorderRightWidth {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderRightWidthPtr`
       pub fn leak(self) -> AzStyleBorderRightWidthPtr { let p = az_style_border_right_width_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderRightWidth { fn drop(&mut self) { az_style_border_right_width_delete(&mut self.ptr); } }


    /// `StyleBorderTopColor` struct
    pub struct StyleBorderTopColor { pub(crate) ptr: AzStyleBorderTopColorPtr }

    impl StyleBorderTopColor {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopColorPtr`
       pub fn leak(self) -> AzStyleBorderTopColorPtr { let p = az_style_border_top_color_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderTopColor { fn drop(&mut self) { az_style_border_top_color_delete(&mut self.ptr); } }


    /// `StyleBorderTopLeftRadius` struct
    pub struct StyleBorderTopLeftRadius { pub(crate) ptr: AzStyleBorderTopLeftRadiusPtr }

    impl StyleBorderTopLeftRadius {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopLeftRadiusPtr`
       pub fn leak(self) -> AzStyleBorderTopLeftRadiusPtr { let p = az_style_border_top_left_radius_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderTopLeftRadius { fn drop(&mut self) { az_style_border_top_left_radius_delete(&mut self.ptr); } }


    /// `StyleBorderTopRightRadius` struct
    pub struct StyleBorderTopRightRadius { pub(crate) ptr: AzStyleBorderTopRightRadiusPtr }

    impl StyleBorderTopRightRadius {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopRightRadiusPtr`
       pub fn leak(self) -> AzStyleBorderTopRightRadiusPtr { let p = az_style_border_top_right_radius_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderTopRightRadius { fn drop(&mut self) { az_style_border_top_right_radius_delete(&mut self.ptr); } }


    /// `StyleBorderTopStyle` struct
    pub struct StyleBorderTopStyle { pub(crate) ptr: AzStyleBorderTopStylePtr }

    impl StyleBorderTopStyle {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopStylePtr`
       pub fn leak(self) -> AzStyleBorderTopStylePtr { let p = az_style_border_top_style_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderTopStyle { fn drop(&mut self) { az_style_border_top_style_delete(&mut self.ptr); } }


    /// `StyleBorderTopWidth` struct
    pub struct StyleBorderTopWidth { pub(crate) ptr: AzStyleBorderTopWidthPtr }

    impl StyleBorderTopWidth {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopWidthPtr`
       pub fn leak(self) -> AzStyleBorderTopWidthPtr { let p = az_style_border_top_width_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderTopWidth { fn drop(&mut self) { az_style_border_top_width_delete(&mut self.ptr); } }


    /// `StyleCursor` struct
    pub struct StyleCursor { pub(crate) ptr: AzStyleCursorPtr }

    impl StyleCursor {
       /// Prevents the destructor from running and returns the internal `AzStyleCursorPtr`
       pub fn leak(self) -> AzStyleCursorPtr { let p = az_style_cursor_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleCursor { fn drop(&mut self) { az_style_cursor_delete(&mut self.ptr); } }


    /// `StyleFontFamily` struct
    pub struct StyleFontFamily { pub(crate) ptr: AzStyleFontFamilyPtr }

    impl StyleFontFamily {
       /// Prevents the destructor from running and returns the internal `AzStyleFontFamilyPtr`
       pub fn leak(self) -> AzStyleFontFamilyPtr { let p = az_style_font_family_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleFontFamily { fn drop(&mut self) { az_style_font_family_delete(&mut self.ptr); } }


    /// `StyleFontSize` struct
    pub struct StyleFontSize { pub(crate) ptr: AzStyleFontSizePtr }

    impl StyleFontSize {
       /// Prevents the destructor from running and returns the internal `AzStyleFontSizePtr`
       pub fn leak(self) -> AzStyleFontSizePtr { let p = az_style_font_size_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleFontSize { fn drop(&mut self) { az_style_font_size_delete(&mut self.ptr); } }


    /// `StyleLetterSpacing` struct
    pub struct StyleLetterSpacing { pub(crate) ptr: AzStyleLetterSpacingPtr }

    impl StyleLetterSpacing {
       /// Prevents the destructor from running and returns the internal `AzStyleLetterSpacingPtr`
       pub fn leak(self) -> AzStyleLetterSpacingPtr { let p = az_style_letter_spacing_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleLetterSpacing { fn drop(&mut self) { az_style_letter_spacing_delete(&mut self.ptr); } }


    /// `StyleLineHeight` struct
    pub struct StyleLineHeight { pub(crate) ptr: AzStyleLineHeightPtr }

    impl StyleLineHeight {
       /// Prevents the destructor from running and returns the internal `AzStyleLineHeightPtr`
       pub fn leak(self) -> AzStyleLineHeightPtr { let p = az_style_line_height_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleLineHeight { fn drop(&mut self) { az_style_line_height_delete(&mut self.ptr); } }


    /// `StyleTabWidth` struct
    pub struct StyleTabWidth { pub(crate) ptr: AzStyleTabWidthPtr }

    impl StyleTabWidth {
       /// Prevents the destructor from running and returns the internal `AzStyleTabWidthPtr`
       pub fn leak(self) -> AzStyleTabWidthPtr { let p = az_style_tab_width_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleTabWidth { fn drop(&mut self) { az_style_tab_width_delete(&mut self.ptr); } }


    /// `StyleTextAlignmentHorz` struct
    pub struct StyleTextAlignmentHorz { pub(crate) ptr: AzStyleTextAlignmentHorzPtr }

    impl StyleTextAlignmentHorz {
       /// Prevents the destructor from running and returns the internal `AzStyleTextAlignmentHorzPtr`
       pub fn leak(self) -> AzStyleTextAlignmentHorzPtr { let p = az_style_text_alignment_horz_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleTextAlignmentHorz { fn drop(&mut self) { az_style_text_alignment_horz_delete(&mut self.ptr); } }


    /// `StyleTextColor` struct
    pub struct StyleTextColor { pub(crate) ptr: AzStyleTextColorPtr }

    impl StyleTextColor {
       /// Prevents the destructor from running and returns the internal `AzStyleTextColorPtr`
       pub fn leak(self) -> AzStyleTextColorPtr { let p = az_style_text_color_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleTextColor { fn drop(&mut self) { az_style_text_color_delete(&mut self.ptr); } }


    /// `StyleWordSpacing` struct
    pub struct StyleWordSpacing { pub(crate) ptr: AzStyleWordSpacingPtr }

    impl StyleWordSpacing {
       /// Prevents the destructor from running and returns the internal `AzStyleWordSpacingPtr`
       pub fn leak(self) -> AzStyleWordSpacingPtr { let p = az_style_word_spacing_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleWordSpacing { fn drop(&mut self) { az_style_word_spacing_delete(&mut self.ptr); } }


    /// `BoxShadowPreDisplayItemValue` struct
    pub struct BoxShadowPreDisplayItemValue { pub(crate) object: AzBoxShadowPreDisplayItemValue }

    impl BoxShadowPreDisplayItemValue {
        pub fn auto() -> Self { Self { object: az_box_shadow_pre_display_item_value_auto() }  }
        pub fn none() -> Self { Self { object: az_box_shadow_pre_display_item_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_box_shadow_pre_display_item_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_box_shadow_pre_display_item_value_initial() }  }
        pub fn exact(variant_data: crate::css::BoxShadowPreDisplayItem) -> Self { Self { object: az_box_shadow_pre_display_item_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzBoxShadowPreDisplayItemValue`
       pub fn leak(self) -> AzBoxShadowPreDisplayItemValue { az_box_shadow_pre_display_item_value_deep_copy(&self.object) }
    }

    impl Drop for BoxShadowPreDisplayItemValue { fn drop(&mut self) { az_box_shadow_pre_display_item_value_delete(&mut self.object); } }


    /// `LayoutAlignContentValue` struct
    pub struct LayoutAlignContentValue { pub(crate) object: AzLayoutAlignContentValue }

    impl LayoutAlignContentValue {
        pub fn auto() -> Self { Self { object: az_layout_align_content_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_align_content_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_align_content_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_align_content_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutAlignContent) -> Self { Self { object: az_layout_align_content_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutAlignContentValue`
       pub fn leak(self) -> AzLayoutAlignContentValue { az_layout_align_content_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutAlignContentValue { fn drop(&mut self) { az_layout_align_content_value_delete(&mut self.object); } }


    /// `LayoutAlignItemsValue` struct
    pub struct LayoutAlignItemsValue { pub(crate) object: AzLayoutAlignItemsValue }

    impl LayoutAlignItemsValue {
        pub fn auto() -> Self { Self { object: az_layout_align_items_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_align_items_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_align_items_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_align_items_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutAlignItems) -> Self { Self { object: az_layout_align_items_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutAlignItemsValue`
       pub fn leak(self) -> AzLayoutAlignItemsValue { az_layout_align_items_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutAlignItemsValue { fn drop(&mut self) { az_layout_align_items_value_delete(&mut self.object); } }


    /// `LayoutBottomValue` struct
    pub struct LayoutBottomValue { pub(crate) object: AzLayoutBottomValue }

    impl LayoutBottomValue {
        pub fn auto() -> Self { Self { object: az_layout_bottom_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_bottom_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_bottom_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_bottom_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutBottom) -> Self { Self { object: az_layout_bottom_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutBottomValue`
       pub fn leak(self) -> AzLayoutBottomValue { az_layout_bottom_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutBottomValue { fn drop(&mut self) { az_layout_bottom_value_delete(&mut self.object); } }


    /// `LayoutBoxSizingValue` struct
    pub struct LayoutBoxSizingValue { pub(crate) object: AzLayoutBoxSizingValue }

    impl LayoutBoxSizingValue {
        pub fn auto() -> Self { Self { object: az_layout_box_sizing_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_box_sizing_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_box_sizing_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_box_sizing_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutBoxSizing) -> Self { Self { object: az_layout_box_sizing_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutBoxSizingValue`
       pub fn leak(self) -> AzLayoutBoxSizingValue { az_layout_box_sizing_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutBoxSizingValue { fn drop(&mut self) { az_layout_box_sizing_value_delete(&mut self.object); } }


    /// `LayoutDirectionValue` struct
    pub struct LayoutDirectionValue { pub(crate) object: AzLayoutDirectionValue }

    impl LayoutDirectionValue {
        pub fn auto() -> Self { Self { object: az_layout_direction_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_direction_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_direction_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_direction_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutDirection) -> Self { Self { object: az_layout_direction_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutDirectionValue`
       pub fn leak(self) -> AzLayoutDirectionValue { az_layout_direction_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutDirectionValue { fn drop(&mut self) { az_layout_direction_value_delete(&mut self.object); } }


    /// `LayoutDisplayValue` struct
    pub struct LayoutDisplayValue { pub(crate) object: AzLayoutDisplayValue }

    impl LayoutDisplayValue {
        pub fn auto() -> Self { Self { object: az_layout_display_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_display_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_display_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_display_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutDisplay) -> Self { Self { object: az_layout_display_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutDisplayValue`
       pub fn leak(self) -> AzLayoutDisplayValue { az_layout_display_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutDisplayValue { fn drop(&mut self) { az_layout_display_value_delete(&mut self.object); } }


    /// `LayoutFlexGrowValue` struct
    pub struct LayoutFlexGrowValue { pub(crate) object: AzLayoutFlexGrowValue }

    impl LayoutFlexGrowValue {
        pub fn auto() -> Self { Self { object: az_layout_flex_grow_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_flex_grow_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_flex_grow_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_flex_grow_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutFlexGrow) -> Self { Self { object: az_layout_flex_grow_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutFlexGrowValue`
       pub fn leak(self) -> AzLayoutFlexGrowValue { az_layout_flex_grow_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutFlexGrowValue { fn drop(&mut self) { az_layout_flex_grow_value_delete(&mut self.object); } }


    /// `LayoutFlexShrinkValue` struct
    pub struct LayoutFlexShrinkValue { pub(crate) object: AzLayoutFlexShrinkValue }

    impl LayoutFlexShrinkValue {
        pub fn auto() -> Self { Self { object: az_layout_flex_shrink_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_flex_shrink_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_flex_shrink_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_flex_shrink_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutFlexShrink) -> Self { Self { object: az_layout_flex_shrink_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutFlexShrinkValue`
       pub fn leak(self) -> AzLayoutFlexShrinkValue { az_layout_flex_shrink_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutFlexShrinkValue { fn drop(&mut self) { az_layout_flex_shrink_value_delete(&mut self.object); } }


    /// `LayoutFloatValue` struct
    pub struct LayoutFloatValue { pub(crate) object: AzLayoutFloatValue }

    impl LayoutFloatValue {
        pub fn auto() -> Self { Self { object: az_layout_float_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_float_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_float_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_float_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutFloat) -> Self { Self { object: az_layout_float_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutFloatValue`
       pub fn leak(self) -> AzLayoutFloatValue { az_layout_float_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutFloatValue { fn drop(&mut self) { az_layout_float_value_delete(&mut self.object); } }


    /// `LayoutHeightValue` struct
    pub struct LayoutHeightValue { pub(crate) object: AzLayoutHeightValue }

    impl LayoutHeightValue {
        pub fn auto() -> Self { Self { object: az_layout_height_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_height_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_height_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_height_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutHeight) -> Self { Self { object: az_layout_height_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutHeightValue`
       pub fn leak(self) -> AzLayoutHeightValue { az_layout_height_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutHeightValue { fn drop(&mut self) { az_layout_height_value_delete(&mut self.object); } }


    /// `LayoutJustifyContentValue` struct
    pub struct LayoutJustifyContentValue { pub(crate) object: AzLayoutJustifyContentValue }

    impl LayoutJustifyContentValue {
        pub fn auto() -> Self { Self { object: az_layout_justify_content_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_justify_content_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_justify_content_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_justify_content_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutJustifyContent) -> Self { Self { object: az_layout_justify_content_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutJustifyContentValue`
       pub fn leak(self) -> AzLayoutJustifyContentValue { az_layout_justify_content_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutJustifyContentValue { fn drop(&mut self) { az_layout_justify_content_value_delete(&mut self.object); } }


    /// `LayoutLeftValue` struct
    pub struct LayoutLeftValue { pub(crate) object: AzLayoutLeftValue }

    impl LayoutLeftValue {
        pub fn auto() -> Self { Self { object: az_layout_left_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_left_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_left_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_left_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutLeft) -> Self { Self { object: az_layout_left_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutLeftValue`
       pub fn leak(self) -> AzLayoutLeftValue { az_layout_left_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutLeftValue { fn drop(&mut self) { az_layout_left_value_delete(&mut self.object); } }


    /// `LayoutMarginBottomValue` struct
    pub struct LayoutMarginBottomValue { pub(crate) object: AzLayoutMarginBottomValue }

    impl LayoutMarginBottomValue {
        pub fn auto() -> Self { Self { object: az_layout_margin_bottom_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_margin_bottom_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_margin_bottom_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_margin_bottom_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMarginBottom) -> Self { Self { object: az_layout_margin_bottom_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginBottomValue`
       pub fn leak(self) -> AzLayoutMarginBottomValue { az_layout_margin_bottom_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMarginBottomValue { fn drop(&mut self) { az_layout_margin_bottom_value_delete(&mut self.object); } }


    /// `LayoutMarginLeftValue` struct
    pub struct LayoutMarginLeftValue { pub(crate) object: AzLayoutMarginLeftValue }

    impl LayoutMarginLeftValue {
        pub fn auto() -> Self { Self { object: az_layout_margin_left_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_margin_left_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_margin_left_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_margin_left_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMarginLeft) -> Self { Self { object: az_layout_margin_left_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginLeftValue`
       pub fn leak(self) -> AzLayoutMarginLeftValue { az_layout_margin_left_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMarginLeftValue { fn drop(&mut self) { az_layout_margin_left_value_delete(&mut self.object); } }


    /// `LayoutMarginRightValue` struct
    pub struct LayoutMarginRightValue { pub(crate) object: AzLayoutMarginRightValue }

    impl LayoutMarginRightValue {
        pub fn auto() -> Self { Self { object: az_layout_margin_right_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_margin_right_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_margin_right_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_margin_right_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMarginRight) -> Self { Self { object: az_layout_margin_right_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginRightValue`
       pub fn leak(self) -> AzLayoutMarginRightValue { az_layout_margin_right_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMarginRightValue { fn drop(&mut self) { az_layout_margin_right_value_delete(&mut self.object); } }


    /// `LayoutMarginTopValue` struct
    pub struct LayoutMarginTopValue { pub(crate) object: AzLayoutMarginTopValue }

    impl LayoutMarginTopValue {
        pub fn auto() -> Self { Self { object: az_layout_margin_top_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_margin_top_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_margin_top_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_margin_top_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMarginTop) -> Self { Self { object: az_layout_margin_top_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginTopValue`
       pub fn leak(self) -> AzLayoutMarginTopValue { az_layout_margin_top_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMarginTopValue { fn drop(&mut self) { az_layout_margin_top_value_delete(&mut self.object); } }


    /// `LayoutMaxHeightValue` struct
    pub struct LayoutMaxHeightValue { pub(crate) object: AzLayoutMaxHeightValue }

    impl LayoutMaxHeightValue {
        pub fn auto() -> Self { Self { object: az_layout_max_height_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_max_height_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_max_height_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_max_height_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMaxHeight) -> Self { Self { object: az_layout_max_height_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMaxHeightValue`
       pub fn leak(self) -> AzLayoutMaxHeightValue { az_layout_max_height_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMaxHeightValue { fn drop(&mut self) { az_layout_max_height_value_delete(&mut self.object); } }


    /// `LayoutMaxWidthValue` struct
    pub struct LayoutMaxWidthValue { pub(crate) object: AzLayoutMaxWidthValue }

    impl LayoutMaxWidthValue {
        pub fn auto() -> Self { Self { object: az_layout_max_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_max_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_max_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_max_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMaxWidth) -> Self { Self { object: az_layout_max_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMaxWidthValue`
       pub fn leak(self) -> AzLayoutMaxWidthValue { az_layout_max_width_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMaxWidthValue { fn drop(&mut self) { az_layout_max_width_value_delete(&mut self.object); } }


    /// `LayoutMinHeightValue` struct
    pub struct LayoutMinHeightValue { pub(crate) object: AzLayoutMinHeightValue }

    impl LayoutMinHeightValue {
        pub fn auto() -> Self { Self { object: az_layout_min_height_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_min_height_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_min_height_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_min_height_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMinHeight) -> Self { Self { object: az_layout_min_height_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMinHeightValue`
       pub fn leak(self) -> AzLayoutMinHeightValue { az_layout_min_height_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMinHeightValue { fn drop(&mut self) { az_layout_min_height_value_delete(&mut self.object); } }


    /// `LayoutMinWidthValue` struct
    pub struct LayoutMinWidthValue { pub(crate) object: AzLayoutMinWidthValue }

    impl LayoutMinWidthValue {
        pub fn auto() -> Self { Self { object: az_layout_min_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_min_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_min_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_min_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMinWidth) -> Self { Self { object: az_layout_min_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMinWidthValue`
       pub fn leak(self) -> AzLayoutMinWidthValue { az_layout_min_width_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMinWidthValue { fn drop(&mut self) { az_layout_min_width_value_delete(&mut self.object); } }


    /// `LayoutPaddingBottomValue` struct
    pub struct LayoutPaddingBottomValue { pub(crate) object: AzLayoutPaddingBottomValue }

    impl LayoutPaddingBottomValue {
        pub fn auto() -> Self { Self { object: az_layout_padding_bottom_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_padding_bottom_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_padding_bottom_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_padding_bottom_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutPaddingBottom) -> Self { Self { object: az_layout_padding_bottom_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingBottomValue`
       pub fn leak(self) -> AzLayoutPaddingBottomValue { az_layout_padding_bottom_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutPaddingBottomValue { fn drop(&mut self) { az_layout_padding_bottom_value_delete(&mut self.object); } }


    /// `LayoutPaddingLeftValue` struct
    pub struct LayoutPaddingLeftValue { pub(crate) object: AzLayoutPaddingLeftValue }

    impl LayoutPaddingLeftValue {
        pub fn auto() -> Self { Self { object: az_layout_padding_left_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_padding_left_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_padding_left_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_padding_left_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutPaddingLeft) -> Self { Self { object: az_layout_padding_left_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingLeftValue`
       pub fn leak(self) -> AzLayoutPaddingLeftValue { az_layout_padding_left_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutPaddingLeftValue { fn drop(&mut self) { az_layout_padding_left_value_delete(&mut self.object); } }


    /// `LayoutPaddingRightValue` struct
    pub struct LayoutPaddingRightValue { pub(crate) object: AzLayoutPaddingRightValue }

    impl LayoutPaddingRightValue {
        pub fn auto() -> Self { Self { object: az_layout_padding_right_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_padding_right_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_padding_right_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_padding_right_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutPaddingRight) -> Self { Self { object: az_layout_padding_right_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingRightValue`
       pub fn leak(self) -> AzLayoutPaddingRightValue { az_layout_padding_right_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutPaddingRightValue { fn drop(&mut self) { az_layout_padding_right_value_delete(&mut self.object); } }


    /// `LayoutPaddingTopValue` struct
    pub struct LayoutPaddingTopValue { pub(crate) object: AzLayoutPaddingTopValue }

    impl LayoutPaddingTopValue {
        pub fn auto() -> Self { Self { object: az_layout_padding_top_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_padding_top_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_padding_top_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_padding_top_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutPaddingTop) -> Self { Self { object: az_layout_padding_top_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingTopValue`
       pub fn leak(self) -> AzLayoutPaddingTopValue { az_layout_padding_top_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutPaddingTopValue { fn drop(&mut self) { az_layout_padding_top_value_delete(&mut self.object); } }


    /// `LayoutPositionValue` struct
    pub struct LayoutPositionValue { pub(crate) object: AzLayoutPositionValue }

    impl LayoutPositionValue {
        pub fn auto() -> Self { Self { object: az_layout_position_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_position_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_position_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_position_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutPosition) -> Self { Self { object: az_layout_position_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutPositionValue`
       pub fn leak(self) -> AzLayoutPositionValue { az_layout_position_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutPositionValue { fn drop(&mut self) { az_layout_position_value_delete(&mut self.object); } }


    /// `LayoutRightValue` struct
    pub struct LayoutRightValue { pub(crate) object: AzLayoutRightValue }

    impl LayoutRightValue {
        pub fn auto() -> Self { Self { object: az_layout_right_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_right_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_right_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_right_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutRight) -> Self { Self { object: az_layout_right_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutRightValue`
       pub fn leak(self) -> AzLayoutRightValue { az_layout_right_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutRightValue { fn drop(&mut self) { az_layout_right_value_delete(&mut self.object); } }


    /// `LayoutTopValue` struct
    pub struct LayoutTopValue { pub(crate) object: AzLayoutTopValue }

    impl LayoutTopValue {
        pub fn auto() -> Self { Self { object: az_layout_top_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_top_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_top_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_top_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutTop) -> Self { Self { object: az_layout_top_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutTopValue`
       pub fn leak(self) -> AzLayoutTopValue { az_layout_top_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutTopValue { fn drop(&mut self) { az_layout_top_value_delete(&mut self.object); } }


    /// `LayoutWidthValue` struct
    pub struct LayoutWidthValue { pub(crate) object: AzLayoutWidthValue }

    impl LayoutWidthValue {
        pub fn auto() -> Self { Self { object: az_layout_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutWidth) -> Self { Self { object: az_layout_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutWidthValue`
       pub fn leak(self) -> AzLayoutWidthValue { az_layout_width_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutWidthValue { fn drop(&mut self) { az_layout_width_value_delete(&mut self.object); } }


    /// `LayoutWrapValue` struct
    pub struct LayoutWrapValue { pub(crate) object: AzLayoutWrapValue }

    impl LayoutWrapValue {
        pub fn auto() -> Self { Self { object: az_layout_wrap_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_wrap_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_wrap_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_wrap_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutWrap) -> Self { Self { object: az_layout_wrap_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutWrapValue`
       pub fn leak(self) -> AzLayoutWrapValue { az_layout_wrap_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutWrapValue { fn drop(&mut self) { az_layout_wrap_value_delete(&mut self.object); } }


    /// `OverflowValue` struct
    pub struct OverflowValue { pub(crate) object: AzOverflowValue }

    impl OverflowValue {
        pub fn auto() -> Self { Self { object: az_overflow_value_auto() }  }
        pub fn none() -> Self { Self { object: az_overflow_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_overflow_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_overflow_value_initial() }  }
        pub fn exact(variant_data: crate::css::Overflow) -> Self { Self { object: az_overflow_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzOverflowValue`
       pub fn leak(self) -> AzOverflowValue { az_overflow_value_deep_copy(&self.object) }
    }

    impl Drop for OverflowValue { fn drop(&mut self) { az_overflow_value_delete(&mut self.object); } }


    /// `StyleBackgroundContentValue` struct
    pub struct StyleBackgroundContentValue { pub(crate) object: AzStyleBackgroundContentValue }

    impl StyleBackgroundContentValue {
        pub fn auto() -> Self { Self { object: az_style_background_content_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_background_content_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_background_content_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_background_content_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBackgroundContent) -> Self { Self { object: az_style_background_content_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundContentValue`
       pub fn leak(self) -> AzStyleBackgroundContentValue { az_style_background_content_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBackgroundContentValue { fn drop(&mut self) { az_style_background_content_value_delete(&mut self.object); } }


    /// `StyleBackgroundPositionValue` struct
    pub struct StyleBackgroundPositionValue { pub(crate) object: AzStyleBackgroundPositionValue }

    impl StyleBackgroundPositionValue {
        pub fn auto() -> Self { Self { object: az_style_background_position_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_background_position_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_background_position_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_background_position_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBackgroundPosition) -> Self { Self { object: az_style_background_position_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundPositionValue`
       pub fn leak(self) -> AzStyleBackgroundPositionValue { az_style_background_position_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBackgroundPositionValue { fn drop(&mut self) { az_style_background_position_value_delete(&mut self.object); } }


    /// `StyleBackgroundRepeatValue` struct
    pub struct StyleBackgroundRepeatValue { pub(crate) object: AzStyleBackgroundRepeatValue }

    impl StyleBackgroundRepeatValue {
        pub fn auto() -> Self { Self { object: az_style_background_repeat_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_background_repeat_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_background_repeat_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_background_repeat_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBackgroundRepeat) -> Self { Self { object: az_style_background_repeat_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundRepeatValue`
       pub fn leak(self) -> AzStyleBackgroundRepeatValue { az_style_background_repeat_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBackgroundRepeatValue { fn drop(&mut self) { az_style_background_repeat_value_delete(&mut self.object); } }


    /// `StyleBackgroundSizeValue` struct
    pub struct StyleBackgroundSizeValue { pub(crate) object: AzStyleBackgroundSizeValue }

    impl StyleBackgroundSizeValue {
        pub fn auto() -> Self { Self { object: az_style_background_size_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_background_size_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_background_size_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_background_size_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBackgroundSize) -> Self { Self { object: az_style_background_size_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundSizeValue`
       pub fn leak(self) -> AzStyleBackgroundSizeValue { az_style_background_size_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBackgroundSizeValue { fn drop(&mut self) { az_style_background_size_value_delete(&mut self.object); } }


    /// `StyleBorderBottomColorValue` struct
    pub struct StyleBorderBottomColorValue { pub(crate) object: AzStyleBorderBottomColorValue }

    impl StyleBorderBottomColorValue {
        pub fn auto() -> Self { Self { object: az_style_border_bottom_color_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_bottom_color_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_bottom_color_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_bottom_color_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomColor) -> Self { Self { object: az_style_border_bottom_color_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomColorValue`
       pub fn leak(self) -> AzStyleBorderBottomColorValue { az_style_border_bottom_color_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomColorValue { fn drop(&mut self) { az_style_border_bottom_color_value_delete(&mut self.object); } }


    /// `StyleBorderBottomLeftRadiusValue` struct
    pub struct StyleBorderBottomLeftRadiusValue { pub(crate) object: AzStyleBorderBottomLeftRadiusValue }

    impl StyleBorderBottomLeftRadiusValue {
        pub fn auto() -> Self { Self { object: az_style_border_bottom_left_radius_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_bottom_left_radius_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_bottom_left_radius_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_bottom_left_radius_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomLeftRadius) -> Self { Self { object: az_style_border_bottom_left_radius_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomLeftRadiusValue`
       pub fn leak(self) -> AzStyleBorderBottomLeftRadiusValue { az_style_border_bottom_left_radius_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomLeftRadiusValue { fn drop(&mut self) { az_style_border_bottom_left_radius_value_delete(&mut self.object); } }


    /// `StyleBorderBottomRightRadiusValue` struct
    pub struct StyleBorderBottomRightRadiusValue { pub(crate) object: AzStyleBorderBottomRightRadiusValue }

    impl StyleBorderBottomRightRadiusValue {
        pub fn auto() -> Self { Self { object: az_style_border_bottom_right_radius_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_bottom_right_radius_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_bottom_right_radius_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_bottom_right_radius_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomRightRadius) -> Self { Self { object: az_style_border_bottom_right_radius_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomRightRadiusValue`
       pub fn leak(self) -> AzStyleBorderBottomRightRadiusValue { az_style_border_bottom_right_radius_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomRightRadiusValue { fn drop(&mut self) { az_style_border_bottom_right_radius_value_delete(&mut self.object); } }


    /// `StyleBorderBottomStyleValue` struct
    pub struct StyleBorderBottomStyleValue { pub(crate) object: AzStyleBorderBottomStyleValue }

    impl StyleBorderBottomStyleValue {
        pub fn auto() -> Self { Self { object: az_style_border_bottom_style_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_bottom_style_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_bottom_style_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_bottom_style_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomStyle) -> Self { Self { object: az_style_border_bottom_style_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomStyleValue`
       pub fn leak(self) -> AzStyleBorderBottomStyleValue { az_style_border_bottom_style_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomStyleValue { fn drop(&mut self) { az_style_border_bottom_style_value_delete(&mut self.object); } }


    /// `StyleBorderBottomWidthValue` struct
    pub struct StyleBorderBottomWidthValue { pub(crate) object: AzStyleBorderBottomWidthValue }

    impl StyleBorderBottomWidthValue {
        pub fn auto() -> Self { Self { object: az_style_border_bottom_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_bottom_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_bottom_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_bottom_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomWidth) -> Self { Self { object: az_style_border_bottom_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomWidthValue`
       pub fn leak(self) -> AzStyleBorderBottomWidthValue { az_style_border_bottom_width_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomWidthValue { fn drop(&mut self) { az_style_border_bottom_width_value_delete(&mut self.object); } }


    /// `StyleBorderLeftColorValue` struct
    pub struct StyleBorderLeftColorValue { pub(crate) object: AzStyleBorderLeftColorValue }

    impl StyleBorderLeftColorValue {
        pub fn auto() -> Self { Self { object: az_style_border_left_color_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_left_color_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_left_color_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_left_color_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderLeftColor) -> Self { Self { object: az_style_border_left_color_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderLeftColorValue`
       pub fn leak(self) -> AzStyleBorderLeftColorValue { az_style_border_left_color_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderLeftColorValue { fn drop(&mut self) { az_style_border_left_color_value_delete(&mut self.object); } }


    /// `StyleBorderLeftStyleValue` struct
    pub struct StyleBorderLeftStyleValue { pub(crate) object: AzStyleBorderLeftStyleValue }

    impl StyleBorderLeftStyleValue {
        pub fn auto() -> Self { Self { object: az_style_border_left_style_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_left_style_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_left_style_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_left_style_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderLeftStyle) -> Self { Self { object: az_style_border_left_style_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderLeftStyleValue`
       pub fn leak(self) -> AzStyleBorderLeftStyleValue { az_style_border_left_style_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderLeftStyleValue { fn drop(&mut self) { az_style_border_left_style_value_delete(&mut self.object); } }


    /// `StyleBorderLeftWidthValue` struct
    pub struct StyleBorderLeftWidthValue { pub(crate) object: AzStyleBorderLeftWidthValue }

    impl StyleBorderLeftWidthValue {
        pub fn auto() -> Self { Self { object: az_style_border_left_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_left_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_left_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_left_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderLeftWidth) -> Self { Self { object: az_style_border_left_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderLeftWidthValue`
       pub fn leak(self) -> AzStyleBorderLeftWidthValue { az_style_border_left_width_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderLeftWidthValue { fn drop(&mut self) { az_style_border_left_width_value_delete(&mut self.object); } }


    /// `StyleBorderRightColorValue` struct
    pub struct StyleBorderRightColorValue { pub(crate) object: AzStyleBorderRightColorValue }

    impl StyleBorderRightColorValue {
        pub fn auto() -> Self { Self { object: az_style_border_right_color_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_right_color_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_right_color_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_right_color_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderRightColor) -> Self { Self { object: az_style_border_right_color_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderRightColorValue`
       pub fn leak(self) -> AzStyleBorderRightColorValue { az_style_border_right_color_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderRightColorValue { fn drop(&mut self) { az_style_border_right_color_value_delete(&mut self.object); } }


    /// `StyleBorderRightStyleValue` struct
    pub struct StyleBorderRightStyleValue { pub(crate) object: AzStyleBorderRightStyleValue }

    impl StyleBorderRightStyleValue {
        pub fn auto() -> Self { Self { object: az_style_border_right_style_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_right_style_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_right_style_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_right_style_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderRightStyle) -> Self { Self { object: az_style_border_right_style_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderRightStyleValue`
       pub fn leak(self) -> AzStyleBorderRightStyleValue { az_style_border_right_style_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderRightStyleValue { fn drop(&mut self) { az_style_border_right_style_value_delete(&mut self.object); } }


    /// `StyleBorderRightWidthValue` struct
    pub struct StyleBorderRightWidthValue { pub(crate) object: AzStyleBorderRightWidthValue }

    impl StyleBorderRightWidthValue {
        pub fn auto() -> Self { Self { object: az_style_border_right_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_right_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_right_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_right_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderRightWidth) -> Self { Self { object: az_style_border_right_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderRightWidthValue`
       pub fn leak(self) -> AzStyleBorderRightWidthValue { az_style_border_right_width_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderRightWidthValue { fn drop(&mut self) { az_style_border_right_width_value_delete(&mut self.object); } }


    /// `StyleBorderTopColorValue` struct
    pub struct StyleBorderTopColorValue { pub(crate) object: AzStyleBorderTopColorValue }

    impl StyleBorderTopColorValue {
        pub fn auto() -> Self { Self { object: az_style_border_top_color_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_top_color_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_top_color_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_top_color_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderTopColor) -> Self { Self { object: az_style_border_top_color_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopColorValue`
       pub fn leak(self) -> AzStyleBorderTopColorValue { az_style_border_top_color_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopColorValue { fn drop(&mut self) { az_style_border_top_color_value_delete(&mut self.object); } }


    /// `StyleBorderTopLeftRadiusValue` struct
    pub struct StyleBorderTopLeftRadiusValue { pub(crate) object: AzStyleBorderTopLeftRadiusValue }

    impl StyleBorderTopLeftRadiusValue {
        pub fn auto() -> Self { Self { object: az_style_border_top_left_radius_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_top_left_radius_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_top_left_radius_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_top_left_radius_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderTopLeftRadius) -> Self { Self { object: az_style_border_top_left_radius_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopLeftRadiusValue`
       pub fn leak(self) -> AzStyleBorderTopLeftRadiusValue { az_style_border_top_left_radius_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopLeftRadiusValue { fn drop(&mut self) { az_style_border_top_left_radius_value_delete(&mut self.object); } }


    /// `StyleBorderTopRightRadiusValue` struct
    pub struct StyleBorderTopRightRadiusValue { pub(crate) object: AzStyleBorderTopRightRadiusValue }

    impl StyleBorderTopRightRadiusValue {
        pub fn auto() -> Self { Self { object: az_style_border_top_right_radius_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_top_right_radius_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_top_right_radius_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_top_right_radius_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderTopRightRadius) -> Self { Self { object: az_style_border_top_right_radius_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopRightRadiusValue`
       pub fn leak(self) -> AzStyleBorderTopRightRadiusValue { az_style_border_top_right_radius_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopRightRadiusValue { fn drop(&mut self) { az_style_border_top_right_radius_value_delete(&mut self.object); } }


    /// `StyleBorderTopStyleValue` struct
    pub struct StyleBorderTopStyleValue { pub(crate) object: AzStyleBorderTopStyleValue }

    impl StyleBorderTopStyleValue {
        pub fn auto() -> Self { Self { object: az_style_border_top_style_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_top_style_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_top_style_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_top_style_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderTopStyle) -> Self { Self { object: az_style_border_top_style_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopStyleValue`
       pub fn leak(self) -> AzStyleBorderTopStyleValue { az_style_border_top_style_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopStyleValue { fn drop(&mut self) { az_style_border_top_style_value_delete(&mut self.object); } }


    /// `StyleBorderTopWidthValue` struct
    pub struct StyleBorderTopWidthValue { pub(crate) object: AzStyleBorderTopWidthValue }

    impl StyleBorderTopWidthValue {
        pub fn auto() -> Self { Self { object: az_style_border_top_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_top_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_top_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_top_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderTopWidth) -> Self { Self { object: az_style_border_top_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopWidthValue`
       pub fn leak(self) -> AzStyleBorderTopWidthValue { az_style_border_top_width_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopWidthValue { fn drop(&mut self) { az_style_border_top_width_value_delete(&mut self.object); } }


    /// `StyleCursorValue` struct
    pub struct StyleCursorValue { pub(crate) object: AzStyleCursorValue }

    impl StyleCursorValue {
        pub fn auto() -> Self { Self { object: az_style_cursor_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_cursor_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_cursor_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_cursor_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleCursor) -> Self { Self { object: az_style_cursor_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleCursorValue`
       pub fn leak(self) -> AzStyleCursorValue { az_style_cursor_value_deep_copy(&self.object) }
    }

    impl Drop for StyleCursorValue { fn drop(&mut self) { az_style_cursor_value_delete(&mut self.object); } }


    /// `StyleFontFamilyValue` struct
    pub struct StyleFontFamilyValue { pub(crate) object: AzStyleFontFamilyValue }

    impl StyleFontFamilyValue {
        pub fn auto() -> Self { Self { object: az_style_font_family_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_font_family_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_font_family_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_font_family_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleFontFamily) -> Self { Self { object: az_style_font_family_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleFontFamilyValue`
       pub fn leak(self) -> AzStyleFontFamilyValue { az_style_font_family_value_deep_copy(&self.object) }
    }

    impl Drop for StyleFontFamilyValue { fn drop(&mut self) { az_style_font_family_value_delete(&mut self.object); } }


    /// `StyleFontSizeValue` struct
    pub struct StyleFontSizeValue { pub(crate) object: AzStyleFontSizeValue }

    impl StyleFontSizeValue {
        pub fn auto() -> Self { Self { object: az_style_font_size_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_font_size_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_font_size_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_font_size_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleFontSize) -> Self { Self { object: az_style_font_size_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleFontSizeValue`
       pub fn leak(self) -> AzStyleFontSizeValue { az_style_font_size_value_deep_copy(&self.object) }
    }

    impl Drop for StyleFontSizeValue { fn drop(&mut self) { az_style_font_size_value_delete(&mut self.object); } }


    /// `StyleLetterSpacingValue` struct
    pub struct StyleLetterSpacingValue { pub(crate) object: AzStyleLetterSpacingValue }

    impl StyleLetterSpacingValue {
        pub fn auto() -> Self { Self { object: az_style_letter_spacing_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_letter_spacing_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_letter_spacing_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_letter_spacing_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleLetterSpacing) -> Self { Self { object: az_style_letter_spacing_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleLetterSpacingValue`
       pub fn leak(self) -> AzStyleLetterSpacingValue { az_style_letter_spacing_value_deep_copy(&self.object) }
    }

    impl Drop for StyleLetterSpacingValue { fn drop(&mut self) { az_style_letter_spacing_value_delete(&mut self.object); } }


    /// `StyleLineHeightValue` struct
    pub struct StyleLineHeightValue { pub(crate) object: AzStyleLineHeightValue }

    impl StyleLineHeightValue {
        pub fn auto() -> Self { Self { object: az_style_line_height_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_line_height_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_line_height_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_line_height_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleLineHeight) -> Self { Self { object: az_style_line_height_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleLineHeightValue`
       pub fn leak(self) -> AzStyleLineHeightValue { az_style_line_height_value_deep_copy(&self.object) }
    }

    impl Drop for StyleLineHeightValue { fn drop(&mut self) { az_style_line_height_value_delete(&mut self.object); } }


    /// `StyleTabWidthValue` struct
    pub struct StyleTabWidthValue { pub(crate) object: AzStyleTabWidthValue }

    impl StyleTabWidthValue {
        pub fn auto() -> Self { Self { object: az_style_tab_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_tab_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_tab_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_tab_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleTabWidth) -> Self { Self { object: az_style_tab_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleTabWidthValue`
       pub fn leak(self) -> AzStyleTabWidthValue { az_style_tab_width_value_deep_copy(&self.object) }
    }

    impl Drop for StyleTabWidthValue { fn drop(&mut self) { az_style_tab_width_value_delete(&mut self.object); } }


    /// `StyleTextAlignmentHorzValue` struct
    pub struct StyleTextAlignmentHorzValue { pub(crate) object: AzStyleTextAlignmentHorzValue }

    impl StyleTextAlignmentHorzValue {
        pub fn auto() -> Self { Self { object: az_style_text_alignment_horz_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_text_alignment_horz_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_text_alignment_horz_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_text_alignment_horz_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleTextAlignmentHorz) -> Self { Self { object: az_style_text_alignment_horz_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleTextAlignmentHorzValue`
       pub fn leak(self) -> AzStyleTextAlignmentHorzValue { az_style_text_alignment_horz_value_deep_copy(&self.object) }
    }

    impl Drop for StyleTextAlignmentHorzValue { fn drop(&mut self) { az_style_text_alignment_horz_value_delete(&mut self.object); } }


    /// `StyleTextColorValue` struct
    pub struct StyleTextColorValue { pub(crate) object: AzStyleTextColorValue }

    impl StyleTextColorValue {
        pub fn auto() -> Self { Self { object: az_style_text_color_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_text_color_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_text_color_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_text_color_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleTextColor) -> Self { Self { object: az_style_text_color_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleTextColorValue`
       pub fn leak(self) -> AzStyleTextColorValue { az_style_text_color_value_deep_copy(&self.object) }
    }

    impl Drop for StyleTextColorValue { fn drop(&mut self) { az_style_text_color_value_delete(&mut self.object); } }


    /// `StyleWordSpacingValue` struct
    pub struct StyleWordSpacingValue { pub(crate) object: AzStyleWordSpacingValue }

    impl StyleWordSpacingValue {
        pub fn auto() -> Self { Self { object: az_style_word_spacing_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_word_spacing_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_word_spacing_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_word_spacing_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleWordSpacing) -> Self { Self { object: az_style_word_spacing_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleWordSpacingValue`
       pub fn leak(self) -> AzStyleWordSpacingValue { az_style_word_spacing_value_deep_copy(&self.object) }
    }

    impl Drop for StyleWordSpacingValue { fn drop(&mut self) { az_style_word_spacing_value_delete(&mut self.object); } }


    /// Parsed CSS key-value pair
    pub struct CssProperty { pub(crate) object: AzCssProperty }

    impl CssProperty {
        pub fn text_color(variant_data: crate::css::StyleTextColorValue) -> Self { Self { object: az_css_property_text_color(variant_data.leak()) }}
        pub fn font_size(variant_data: crate::css::StyleFontSizeValue) -> Self { Self { object: az_css_property_font_size(variant_data.leak()) }}
        pub fn font_family(variant_data: crate::css::StyleFontFamilyValue) -> Self { Self { object: az_css_property_font_family(variant_data.leak()) }}
        pub fn text_align(variant_data: crate::css::StyleTextAlignmentHorzValue) -> Self { Self { object: az_css_property_text_align(variant_data.leak()) }}
        pub fn letter_spacing(variant_data: crate::css::StyleLetterSpacingValue) -> Self { Self { object: az_css_property_letter_spacing(variant_data.leak()) }}
        pub fn line_height(variant_data: crate::css::StyleLineHeightValue) -> Self { Self { object: az_css_property_line_height(variant_data.leak()) }}
        pub fn word_spacing(variant_data: crate::css::StyleWordSpacingValue) -> Self { Self { object: az_css_property_word_spacing(variant_data.leak()) }}
        pub fn tab_width(variant_data: crate::css::StyleTabWidthValue) -> Self { Self { object: az_css_property_tab_width(variant_data.leak()) }}
        pub fn cursor(variant_data: crate::css::StyleCursorValue) -> Self { Self { object: az_css_property_cursor(variant_data.leak()) }}
        pub fn display(variant_data: crate::css::LayoutDisplayValue) -> Self { Self { object: az_css_property_display(variant_data.leak()) }}
        pub fn float(variant_data: crate::css::LayoutFloatValue) -> Self { Self { object: az_css_property_float(variant_data.leak()) }}
        pub fn box_sizing(variant_data: crate::css::LayoutBoxSizingValue) -> Self { Self { object: az_css_property_box_sizing(variant_data.leak()) }}
        pub fn width(variant_data: crate::css::LayoutWidthValue) -> Self { Self { object: az_css_property_width(variant_data.leak()) }}
        pub fn height(variant_data: crate::css::LayoutHeightValue) -> Self { Self { object: az_css_property_height(variant_data.leak()) }}
        pub fn min_width(variant_data: crate::css::LayoutMinWidthValue) -> Self { Self { object: az_css_property_min_width(variant_data.leak()) }}
        pub fn min_height(variant_data: crate::css::LayoutMinHeightValue) -> Self { Self { object: az_css_property_min_height(variant_data.leak()) }}
        pub fn max_width(variant_data: crate::css::LayoutMaxWidthValue) -> Self { Self { object: az_css_property_max_width(variant_data.leak()) }}
        pub fn max_height(variant_data: crate::css::LayoutMaxHeightValue) -> Self { Self { object: az_css_property_max_height(variant_data.leak()) }}
        pub fn position(variant_data: crate::css::LayoutPositionValue) -> Self { Self { object: az_css_property_position(variant_data.leak()) }}
        pub fn top(variant_data: crate::css::LayoutTopValue) -> Self { Self { object: az_css_property_top(variant_data.leak()) }}
        pub fn right(variant_data: crate::css::LayoutRightValue) -> Self { Self { object: az_css_property_right(variant_data.leak()) }}
        pub fn left(variant_data: crate::css::LayoutLeftValue) -> Self { Self { object: az_css_property_left(variant_data.leak()) }}
        pub fn bottom(variant_data: crate::css::LayoutBottomValue) -> Self { Self { object: az_css_property_bottom(variant_data.leak()) }}
        pub fn flex_wrap(variant_data: crate::css::LayoutWrapValue) -> Self { Self { object: az_css_property_flex_wrap(variant_data.leak()) }}
        pub fn flex_direction(variant_data: crate::css::LayoutDirectionValue) -> Self { Self { object: az_css_property_flex_direction(variant_data.leak()) }}
        pub fn flex_grow(variant_data: crate::css::LayoutFlexGrowValue) -> Self { Self { object: az_css_property_flex_grow(variant_data.leak()) }}
        pub fn flex_shrink(variant_data: crate::css::LayoutFlexShrinkValue) -> Self { Self { object: az_css_property_flex_shrink(variant_data.leak()) }}
        pub fn justify_content(variant_data: crate::css::LayoutJustifyContentValue) -> Self { Self { object: az_css_property_justify_content(variant_data.leak()) }}
        pub fn align_items(variant_data: crate::css::LayoutAlignItemsValue) -> Self { Self { object: az_css_property_align_items(variant_data.leak()) }}
        pub fn align_content(variant_data: crate::css::LayoutAlignContentValue) -> Self { Self { object: az_css_property_align_content(variant_data.leak()) }}
        pub fn background_content(variant_data: crate::css::StyleBackgroundContentValue) -> Self { Self { object: az_css_property_background_content(variant_data.leak()) }}
        pub fn background_position(variant_data: crate::css::StyleBackgroundPositionValue) -> Self { Self { object: az_css_property_background_position(variant_data.leak()) }}
        pub fn background_size(variant_data: crate::css::StyleBackgroundSizeValue) -> Self { Self { object: az_css_property_background_size(variant_data.leak()) }}
        pub fn background_repeat(variant_data: crate::css::StyleBackgroundRepeatValue) -> Self { Self { object: az_css_property_background_repeat(variant_data.leak()) }}
        pub fn overflow_x(variant_data: crate::css::OverflowValue) -> Self { Self { object: az_css_property_overflow_x(variant_data.leak()) }}
        pub fn overflow_y(variant_data: crate::css::OverflowValue) -> Self { Self { object: az_css_property_overflow_y(variant_data.leak()) }}
        pub fn padding_top(variant_data: crate::css::LayoutPaddingTopValue) -> Self { Self { object: az_css_property_padding_top(variant_data.leak()) }}
        pub fn padding_left(variant_data: crate::css::LayoutPaddingLeftValue) -> Self { Self { object: az_css_property_padding_left(variant_data.leak()) }}
        pub fn padding_right(variant_data: crate::css::LayoutPaddingRightValue) -> Self { Self { object: az_css_property_padding_right(variant_data.leak()) }}
        pub fn padding_bottom(variant_data: crate::css::LayoutPaddingBottomValue) -> Self { Self { object: az_css_property_padding_bottom(variant_data.leak()) }}
        pub fn margin_top(variant_data: crate::css::LayoutMarginTopValue) -> Self { Self { object: az_css_property_margin_top(variant_data.leak()) }}
        pub fn margin_left(variant_data: crate::css::LayoutMarginLeftValue) -> Self { Self { object: az_css_property_margin_left(variant_data.leak()) }}
        pub fn margin_right(variant_data: crate::css::LayoutMarginRightValue) -> Self { Self { object: az_css_property_margin_right(variant_data.leak()) }}
        pub fn margin_bottom(variant_data: crate::css::LayoutMarginBottomValue) -> Self { Self { object: az_css_property_margin_bottom(variant_data.leak()) }}
        pub fn border_top_left_radius(variant_data: crate::css::StyleBorderTopLeftRadiusValue) -> Self { Self { object: az_css_property_border_top_left_radius(variant_data.leak()) }}
        pub fn border_top_right_radius(variant_data: crate::css::StyleBorderTopRightRadiusValue) -> Self { Self { object: az_css_property_border_top_right_radius(variant_data.leak()) }}
        pub fn border_bottom_left_radius(variant_data: crate::css::StyleBorderBottomLeftRadiusValue) -> Self { Self { object: az_css_property_border_bottom_left_radius(variant_data.leak()) }}
        pub fn border_bottom_right_radius(variant_data: crate::css::StyleBorderBottomRightRadiusValue) -> Self { Self { object: az_css_property_border_bottom_right_radius(variant_data.leak()) }}
        pub fn border_top_color(variant_data: crate::css::StyleBorderTopColorValue) -> Self { Self { object: az_css_property_border_top_color(variant_data.leak()) }}
        pub fn border_right_color(variant_data: crate::css::StyleBorderRightColorValue) -> Self { Self { object: az_css_property_border_right_color(variant_data.leak()) }}
        pub fn border_left_color(variant_data: crate::css::StyleBorderLeftColorValue) -> Self { Self { object: az_css_property_border_left_color(variant_data.leak()) }}
        pub fn border_bottom_color(variant_data: crate::css::StyleBorderBottomColorValue) -> Self { Self { object: az_css_property_border_bottom_color(variant_data.leak()) }}
        pub fn border_top_style(variant_data: crate::css::StyleBorderTopStyleValue) -> Self { Self { object: az_css_property_border_top_style(variant_data.leak()) }}
        pub fn border_right_style(variant_data: crate::css::StyleBorderRightStyleValue) -> Self { Self { object: az_css_property_border_right_style(variant_data.leak()) }}
        pub fn border_left_style(variant_data: crate::css::StyleBorderLeftStyleValue) -> Self { Self { object: az_css_property_border_left_style(variant_data.leak()) }}
        pub fn border_bottom_style(variant_data: crate::css::StyleBorderBottomStyleValue) -> Self { Self { object: az_css_property_border_bottom_style(variant_data.leak()) }}
        pub fn border_top_width(variant_data: crate::css::StyleBorderTopWidthValue) -> Self { Self { object: az_css_property_border_top_width(variant_data.leak()) }}
        pub fn border_right_width(variant_data: crate::css::StyleBorderRightWidthValue) -> Self { Self { object: az_css_property_border_right_width(variant_data.leak()) }}
        pub fn border_left_width(variant_data: crate::css::StyleBorderLeftWidthValue) -> Self { Self { object: az_css_property_border_left_width(variant_data.leak()) }}
        pub fn border_bottom_width(variant_data: crate::css::StyleBorderBottomWidthValue) -> Self { Self { object: az_css_property_border_bottom_width(variant_data.leak()) }}
        pub fn box_shadow_left(variant_data: crate::css::BoxShadowPreDisplayItemValue) -> Self { Self { object: az_css_property_box_shadow_left(variant_data.leak()) }}
        pub fn box_shadow_right(variant_data: crate::css::BoxShadowPreDisplayItemValue) -> Self { Self { object: az_css_property_box_shadow_right(variant_data.leak()) }}
        pub fn box_shadow_top(variant_data: crate::css::BoxShadowPreDisplayItemValue) -> Self { Self { object: az_css_property_box_shadow_top(variant_data.leak()) }}
        pub fn box_shadow_bottom(variant_data: crate::css::BoxShadowPreDisplayItemValue) -> Self { Self { object: az_css_property_box_shadow_bottom(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzCssProperty`
       pub fn leak(self) -> AzCssProperty { az_css_property_deep_copy(&self.object) }
    }

    impl Drop for CssProperty { fn drop(&mut self) { az_css_property_delete(&mut self.object); } }
}

/// `Dom` construction and configuration
#[allow(dead_code, unused_imports)]
pub mod dom {

    use azul_dll::*;
    use crate::str::String;
    use crate::resources::{ImageId, TextId};
    use crate::callbacks::{IFrameCallback, Callback, RefAny, GlCallback};
    use crate::vec::StringVec;
    use crate::css::CssProperty;


    /// `Dom` struct
    pub struct Dom { pub(crate) ptr: AzDomPtr }

    impl Dom {
        /// Creates a new `div` node
        pub fn div() -> Self { Self { ptr: az_dom_div() } }
        /// Creates a new `body` node
        pub fn body() -> Self { Self { ptr: az_dom_body() } }
        /// Creates a new `p` node with a given `String` as the text contents
        pub fn label(text: String) -> Self { Self { ptr: az_dom_label(text.leak()) } }
        /// Creates a new `p` node from a (cached) text referenced by a `TextId`
        pub fn text(text_id: TextId) -> Self { Self { ptr: az_dom_text(text_id.leak()) } }
        /// Creates a new `img` node from a (cached) text referenced by a `ImageId`
        pub fn image(image_id: ImageId) -> Self { Self { ptr: az_dom_image(image_id.leak()) } }
        /// Creates a new node which will render an OpenGL texture after the layout step is finished. See the documentation for [GlCallback]() for more info about OpenGL rendering callbacks.
        pub fn gl_texture(data: RefAny, callback: GlCallback) -> Self { Self { ptr: az_dom_gl_texture(data.leak(), callback) } }
        /// Creates a new node with a callback that will return a `Dom` after being layouted. See the documentation for [IFrameCallback]() for more info about iframe callbacks.
        pub fn iframe_callback(data: RefAny, callback: IFrameCallback) -> Self { Self { ptr: az_dom_iframe_callback(data.leak(), callback) } }
        /// Adds a CSS ID (`#something`) to the DOM node
        pub fn add_id(&mut self, id: String)  { az_dom_add_id(&mut self.ptr, id.leak()) }
        /// Same as [`Dom::add_id`](#method.add_id), but as a builder method
        pub fn with_id(self, id: String)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_id(self.leak(), id.leak()) } } }
        /// Same as calling [`Dom::add_id`](#method.add_id) for each CSS ID, but this function **replaces** all current CSS IDs
        pub fn set_ids(&mut self, ids: StringVec)  { az_dom_set_ids(&mut self.ptr, ids.leak()) }
        /// Same as [`Dom::set_ids`](#method.set_ids), but as a builder method
        pub fn with_ids(self, ids: StringVec)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_ids(self.leak(), ids.leak()) } } }
        /// Adds a CSS class (`.something`) to the DOM node
        pub fn add_class(&mut self, class: String)  { az_dom_add_class(&mut self.ptr, class.leak()) }
        /// Same as [`Dom::add_class`](#method.add_class), but as a builder method
        pub fn with_class(self, class: String)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_class(self.leak(), class.leak()) } } }
        /// Same as calling [`Dom::add_class`](#method.add_class) for each class, but this function **replaces** all current classes
        pub fn set_classes(&mut self, classes: StringVec)  { az_dom_set_classes(&mut self.ptr, classes.leak()) }
        /// Same as [`Dom::set_classes`](#method.set_classes), but as a builder method
        pub fn with_classes(self, classes: StringVec)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_classes(self.leak(), classes.leak()) } } }
        /// Adds a [`Callback`](callbacks/type.Callback) that acts on the `data` the `event` happens
        pub fn add_callback(&mut self, event: EventFilter, data: RefAny, callback: Callback)  { az_dom_add_callback(&mut self.ptr, event.leak(), data.leak(), callback) }
        /// Same as [`Dom::add_callback`](#method.add_callback), but as a builder method
        pub fn with_callback(self, event: EventFilter, data: RefAny, callback: Callback)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_callback(self.leak(), event.leak(), data.leak(), callback) } } }
        /// Overrides the CSS property of this DOM node with a value (for example `"width = 200px"`)
        pub fn add_css_override(&mut self, id: String, prop: CssProperty)  { az_dom_add_css_override(&mut self.ptr, id.leak(), prop.leak()) }
        /// Same as [`Dom::add_css_override`](#method.add_css_override), but as a builder method
        pub fn with_css_override(self, id: String, prop: CssProperty)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_css_override(self.leak(), id.leak(), prop.leak()) } } }
        /// Sets the `is_draggable` attribute of this DOM node (default: false)
        pub fn set_is_draggable(&mut self, is_draggable: bool)  { az_dom_set_is_draggable(&mut self.ptr, is_draggable) }
        /// Same as [`Dom::set_is_draggable`](#method.set_is_draggable), but as a builder method
        pub fn is_draggable(self, is_draggable: bool)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_is_draggable(self.leak(), is_draggable) } } }
        /// Sets the `tabindex` attribute of this DOM node (makes an element focusable - default: None)
        pub fn set_tab_index(&mut self, tab_index: TabIndex)  { az_dom_set_tab_index(&mut self.ptr, tab_index.leak()) }
        /// Same as [`Dom::set_tab_index`](#method.set_tab_index), but as a builder method
        pub fn with_tab_index(self, tab_index: TabIndex)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_tab_index(self.leak(), tab_index.leak()) } } }
        /// Reparents another `Dom` to be the child node of this `Dom`
        pub fn add_child(&mut self, child: Dom)  { az_dom_add_child(&mut self.ptr, child.leak()) }
        /// Same as [`Dom::add_child`](#method.add_child), but as a builder method
        pub fn with_child(self, child: Dom)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_child(self.leak(), child.leak()) } } }
        /// Returns if the DOM node has a certain CSS ID
        pub fn has_id(&mut self, id: String)  -> bool { az_dom_has_id(&mut self.ptr, id.leak()) }
        /// Returns if the DOM node has a certain CSS class
        pub fn has_class(&mut self, class: String)  -> bool { az_dom_has_class(&mut self.ptr, class.leak()) }
        /// Returns the HTML String for this DOM
        pub fn get_html_string(&mut self)  -> crate::str::String { crate::str::String { object: { az_dom_get_html_string(&mut self.ptr)} } }
       /// Prevents the destructor from running and returns the internal `AzDomPtr`
       pub fn leak(self) -> AzDomPtr { let p = az_dom_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for Dom { fn drop(&mut self) { az_dom_delete(&mut self.ptr); } }


    /// `EventFilter` struct
    pub struct EventFilter { pub(crate) object: AzEventFilter }

    impl EventFilter {
        pub fn hover(variant_data: crate::dom::HoverEventFilter) -> Self { Self { object: az_event_filter_hover(variant_data.leak()) }}
        pub fn not(variant_data: crate::dom::NotEventFilter) -> Self { Self { object: az_event_filter_not(variant_data.leak()) }}
        pub fn focus(variant_data: crate::dom::FocusEventFilter) -> Self { Self { object: az_event_filter_focus(variant_data.leak()) }}
        pub fn window(variant_data: crate::dom::WindowEventFilter) -> Self { Self { object: az_event_filter_window(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzEventFilter`
       pub fn leak(self) -> AzEventFilter { az_event_filter_deep_copy(&self.object) }
    }

    impl Drop for EventFilter { fn drop(&mut self) { az_event_filter_delete(&mut self.object); } }


    /// `HoverEventFilter` struct
    pub struct HoverEventFilter { pub(crate) object: AzHoverEventFilter }

    impl HoverEventFilter {
        pub fn mouse_over() -> Self { Self { object: az_hover_event_filter_mouse_over() }  }
        pub fn mouse_down() -> Self { Self { object: az_hover_event_filter_mouse_down() }  }
        pub fn left_mouse_down() -> Self { Self { object: az_hover_event_filter_left_mouse_down() }  }
        pub fn right_mouse_down() -> Self { Self { object: az_hover_event_filter_right_mouse_down() }  }
        pub fn middle_mouse_down() -> Self { Self { object: az_hover_event_filter_middle_mouse_down() }  }
        pub fn mouse_up() -> Self { Self { object: az_hover_event_filter_mouse_up() }  }
        pub fn left_mouse_up() -> Self { Self { object: az_hover_event_filter_left_mouse_up() }  }
        pub fn right_mouse_up() -> Self { Self { object: az_hover_event_filter_right_mouse_up() }  }
        pub fn middle_mouse_up() -> Self { Self { object: az_hover_event_filter_middle_mouse_up() }  }
        pub fn mouse_enter() -> Self { Self { object: az_hover_event_filter_mouse_enter() }  }
        pub fn mouse_leave() -> Self { Self { object: az_hover_event_filter_mouse_leave() }  }
        pub fn scroll() -> Self { Self { object: az_hover_event_filter_scroll() }  }
        pub fn scroll_start() -> Self { Self { object: az_hover_event_filter_scroll_start() }  }
        pub fn scroll_end() -> Self { Self { object: az_hover_event_filter_scroll_end() }  }
        pub fn text_input() -> Self { Self { object: az_hover_event_filter_text_input() }  }
        pub fn virtual_key_down() -> Self { Self { object: az_hover_event_filter_virtual_key_down() }  }
        pub fn virtual_key_up() -> Self { Self { object: az_hover_event_filter_virtual_key_up() }  }
        pub fn hovered_file() -> Self { Self { object: az_hover_event_filter_hovered_file() }  }
        pub fn dropped_file() -> Self { Self { object: az_hover_event_filter_dropped_file() }  }
        pub fn hovered_file_cancelled() -> Self { Self { object: az_hover_event_filter_hovered_file_cancelled() }  }
       /// Prevents the destructor from running and returns the internal `AzHoverEventFilter`
       pub fn leak(self) -> AzHoverEventFilter { az_hover_event_filter_deep_copy(&self.object) }
    }

    impl Drop for HoverEventFilter { fn drop(&mut self) { az_hover_event_filter_delete(&mut self.object); } }


    /// `FocusEventFilter` struct
    pub struct FocusEventFilter { pub(crate) object: AzFocusEventFilter }

    impl FocusEventFilter {
        pub fn mouse_over() -> Self { Self { object: az_focus_event_filter_mouse_over() }  }
        pub fn mouse_down() -> Self { Self { object: az_focus_event_filter_mouse_down() }  }
        pub fn left_mouse_down() -> Self { Self { object: az_focus_event_filter_left_mouse_down() }  }
        pub fn right_mouse_down() -> Self { Self { object: az_focus_event_filter_right_mouse_down() }  }
        pub fn middle_mouse_down() -> Self { Self { object: az_focus_event_filter_middle_mouse_down() }  }
        pub fn mouse_up() -> Self { Self { object: az_focus_event_filter_mouse_up() }  }
        pub fn left_mouse_up() -> Self { Self { object: az_focus_event_filter_left_mouse_up() }  }
        pub fn right_mouse_up() -> Self { Self { object: az_focus_event_filter_right_mouse_up() }  }
        pub fn middle_mouse_up() -> Self { Self { object: az_focus_event_filter_middle_mouse_up() }  }
        pub fn mouse_enter() -> Self { Self { object: az_focus_event_filter_mouse_enter() }  }
        pub fn mouse_leave() -> Self { Self { object: az_focus_event_filter_mouse_leave() }  }
        pub fn scroll() -> Self { Self { object: az_focus_event_filter_scroll() }  }
        pub fn scroll_start() -> Self { Self { object: az_focus_event_filter_scroll_start() }  }
        pub fn scroll_end() -> Self { Self { object: az_focus_event_filter_scroll_end() }  }
        pub fn text_input() -> Self { Self { object: az_focus_event_filter_text_input() }  }
        pub fn virtual_key_down() -> Self { Self { object: az_focus_event_filter_virtual_key_down() }  }
        pub fn virtual_key_up() -> Self { Self { object: az_focus_event_filter_virtual_key_up() }  }
        pub fn focus_received() -> Self { Self { object: az_focus_event_filter_focus_received() }  }
        pub fn focus_lost() -> Self { Self { object: az_focus_event_filter_focus_lost() }  }
       /// Prevents the destructor from running and returns the internal `AzFocusEventFilter`
       pub fn leak(self) -> AzFocusEventFilter { az_focus_event_filter_deep_copy(&self.object) }
    }

    impl Drop for FocusEventFilter { fn drop(&mut self) { az_focus_event_filter_delete(&mut self.object); } }


    /// `NotEventFilter` struct
    pub struct NotEventFilter { pub(crate) object: AzNotEventFilter }

    impl NotEventFilter {
        pub fn hover(variant_data: crate::dom::HoverEventFilter) -> Self { Self { object: az_not_event_filter_hover(variant_data.leak()) }}
        pub fn focus(variant_data: crate::dom::FocusEventFilter) -> Self { Self { object: az_not_event_filter_focus(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzNotEventFilter`
       pub fn leak(self) -> AzNotEventFilter { az_not_event_filter_deep_copy(&self.object) }
    }

    impl Drop for NotEventFilter { fn drop(&mut self) { az_not_event_filter_delete(&mut self.object); } }


    /// `WindowEventFilter` struct
    pub struct WindowEventFilter { pub(crate) object: AzWindowEventFilter }

    impl WindowEventFilter {
        pub fn mouse_over() -> Self { Self { object: az_window_event_filter_mouse_over() }  }
        pub fn mouse_down() -> Self { Self { object: az_window_event_filter_mouse_down() }  }
        pub fn left_mouse_down() -> Self { Self { object: az_window_event_filter_left_mouse_down() }  }
        pub fn right_mouse_down() -> Self { Self { object: az_window_event_filter_right_mouse_down() }  }
        pub fn middle_mouse_down() -> Self { Self { object: az_window_event_filter_middle_mouse_down() }  }
        pub fn mouse_up() -> Self { Self { object: az_window_event_filter_mouse_up() }  }
        pub fn left_mouse_up() -> Self { Self { object: az_window_event_filter_left_mouse_up() }  }
        pub fn right_mouse_up() -> Self { Self { object: az_window_event_filter_right_mouse_up() }  }
        pub fn middle_mouse_up() -> Self { Self { object: az_window_event_filter_middle_mouse_up() }  }
        pub fn mouse_enter() -> Self { Self { object: az_window_event_filter_mouse_enter() }  }
        pub fn mouse_leave() -> Self { Self { object: az_window_event_filter_mouse_leave() }  }
        pub fn scroll() -> Self { Self { object: az_window_event_filter_scroll() }  }
        pub fn scroll_start() -> Self { Self { object: az_window_event_filter_scroll_start() }  }
        pub fn scroll_end() -> Self { Self { object: az_window_event_filter_scroll_end() }  }
        pub fn text_input() -> Self { Self { object: az_window_event_filter_text_input() }  }
        pub fn virtual_key_down() -> Self { Self { object: az_window_event_filter_virtual_key_down() }  }
        pub fn virtual_key_up() -> Self { Self { object: az_window_event_filter_virtual_key_up() }  }
        pub fn hovered_file() -> Self { Self { object: az_window_event_filter_hovered_file() }  }
        pub fn dropped_file() -> Self { Self { object: az_window_event_filter_dropped_file() }  }
        pub fn hovered_file_cancelled() -> Self { Self { object: az_window_event_filter_hovered_file_cancelled() }  }
       /// Prevents the destructor from running and returns the internal `AzWindowEventFilter`
       pub fn leak(self) -> AzWindowEventFilter { az_window_event_filter_deep_copy(&self.object) }
    }

    impl Drop for WindowEventFilter { fn drop(&mut self) { az_window_event_filter_delete(&mut self.object); } }


    /// `TabIndex` struct
    pub struct TabIndex { pub(crate) object: AzTabIndex }

    impl TabIndex {
        /// Automatic tab index, similar to simply setting `focusable = "true"` or `tabindex = 0`, (both have the effect of making the element focusable)
        pub fn auto() -> Self { Self { object: az_tab_index_auto() }  }
        ///  Set the tab index in relation to its parent element (`tabindex = n`)
        pub fn override_in_parent(variant_data: usize) -> Self { Self { object: az_tab_index_override_in_parent(variant_data) }}
        /// Elements can be focused in callbacks, but are not accessible via keyboard / tab navigation (`tabindex = -1`)
        pub fn no_keyboard_focus() -> Self { Self { object: az_tab_index_no_keyboard_focus() }  }
       /// Prevents the destructor from running and returns the internal `AzTabIndex`
       pub fn leak(self) -> AzTabIndex { az_tab_index_deep_copy(&self.object) }
    }

    impl Drop for TabIndex { fn drop(&mut self) { az_tab_index_delete(&mut self.object); } }
}

/// Struct definition for image / font / text IDs
#[allow(dead_code, unused_imports)]
pub mod resources {

    use azul_dll::*;
    use crate::vec::U8Vec;


    /// `TextId` struct
    pub struct TextId { pub(crate) object: AzTextId }

    impl TextId {
        /// Creates a new, unique `TextId`
        pub fn new() -> Self { Self { object: az_text_id_new() } }
       /// Prevents the destructor from running and returns the internal `AzTextId`
       pub fn leak(self) -> AzTextId { az_text_id_deep_copy(&self.object) }
    }

    impl Drop for TextId { fn drop(&mut self) { az_text_id_delete(&mut self.object); } }


    /// `ImageId` struct
    pub struct ImageId { pub(crate) object: AzImageId }

    impl ImageId {
        /// Creates a new, unique `ImageId`
        pub fn new() -> Self { Self { object: az_image_id_new() } }
       /// Prevents the destructor from running and returns the internal `AzImageId`
       pub fn leak(self) -> AzImageId { az_image_id_deep_copy(&self.object) }
    }

    impl Drop for ImageId { fn drop(&mut self) { az_image_id_delete(&mut self.object); } }


    /// `FontId` struct
    pub struct FontId { pub(crate) object: AzFontId }

    impl FontId {
        /// Creates a new, unique `FontId`
        pub fn new() -> Self { Self { object: az_font_id_new() } }
       /// Prevents the destructor from running and returns the internal `AzFontId`
       pub fn leak(self) -> AzFontId { az_font_id_deep_copy(&self.object) }
    }

    impl Drop for FontId { fn drop(&mut self) { az_font_id_delete(&mut self.object); } }


    /// `ImageSource` struct
    pub struct ImageSource { pub(crate) object: AzImageSource }

    impl ImageSource {
        /// Bytes of the image, encoded in PNG / JPG / etc. format
        pub fn embedded(variant_data: crate::vec::U8Vec) -> Self { Self { object: az_image_source_embedded(variant_data.leak()) }}
        /// References an (encoded!) image as a file from the file system that is loaded when necessary
        pub fn file(variant_data: crate::path::PathBuf) -> Self { Self { object: az_image_source_file(variant_data.leak()) }}
        /// References a decoded (!) `RawImage` as the image source
        pub fn raw(variant_data: crate::resources::RawImage) -> Self { Self { object: az_image_source_raw(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzImageSource`
       pub fn leak(self) -> AzImageSource { az_image_source_deep_copy(&self.object) }
    }

    impl Drop for ImageSource { fn drop(&mut self) { az_image_source_delete(&mut self.object); } }


    /// `FontSource` struct
    pub struct FontSource { pub(crate) object: AzFontSource }

    impl FontSource {
        /// Bytes are the bytes of the font file
        pub fn embedded(variant_data: crate::vec::U8Vec) -> Self { Self { object: az_font_source_embedded(variant_data.leak()) }}
        /// References a font from a file path, which is loaded when necessary
        pub fn file(variant_data: crate::path::PathBuf) -> Self { Self { object: az_font_source_file(variant_data.leak()) }}
        /// References a font from from a system font identifier, such as `"Arial"` or `"Helvetica"`
        pub fn system(variant_data: crate::str::String) -> Self { Self { object: az_font_source_system(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzFontSource`
       pub fn leak(self) -> AzFontSource { az_font_source_deep_copy(&self.object) }
    }

    impl Drop for FontSource { fn drop(&mut self) { az_font_source_delete(&mut self.object); } }


    /// `RawImage` struct
    pub struct RawImage { pub(crate) ptr: AzRawImagePtr }

    impl RawImage {
        /// Creates a new `RawImage` by loading the decoded bytes
        pub fn new(decoded_pixels: U8Vec, width: usize, height: usize, data_format: RawImageFormat) -> Self { Self { ptr: az_raw_image_new(decoded_pixels.leak(), width, height, data_format.leak()) } }
       /// Prevents the destructor from running and returns the internal `AzRawImagePtr`
       pub fn leak(self) -> AzRawImagePtr { let p = az_raw_image_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for RawImage { fn drop(&mut self) { az_raw_image_delete(&mut self.ptr); } }


    /// `RawImageFormat` struct
    pub struct RawImageFormat { pub(crate) object: AzRawImageFormat }

    impl RawImageFormat {
        /// Bytes are in the R-unsinged-8bit format
        pub fn r8() -> Self { Self { object: az_raw_image_format_r8() }  }
        /// Bytes are in the R-unsinged-16bit format
        pub fn r16() -> Self { Self { object: az_raw_image_format_r16() }  }
        /// Bytes are in the RG-unsinged-16bit format
        pub fn rg16() -> Self { Self { object: az_raw_image_format_rg16() }  }
        /// Bytes are in the BRGA-unsigned-8bit format
        pub fn bgra8() -> Self { Self { object: az_raw_image_format_bgra8() }  }
        /// Bytes are in the RGBA-floating-point-32bit format
        pub fn rgbaf32() -> Self { Self { object: az_raw_image_format_rgbaf32() }  }
        /// Bytes are in the RG-unsigned-8bit format
        pub fn rg8() -> Self { Self { object: az_raw_image_format_rg8() }  }
        /// Bytes are in the RGBA-signed-32bit format
        pub fn rgbai32() -> Self { Self { object: az_raw_image_format_rgbai32() }  }
        /// Bytes are in the RGBA-unsigned-8bit format
        pub fn rgba8() -> Self { Self { object: az_raw_image_format_rgba8() }  }
       /// Prevents the destructor from running and returns the internal `AzRawImageFormat`
       pub fn leak(self) -> AzRawImageFormat { az_raw_image_format_deep_copy(&self.object) }
    }

    impl Drop for RawImageFormat { fn drop(&mut self) { az_raw_image_format_delete(&mut self.object); } }
}

/// Window creation / startup configuration
#[allow(dead_code, unused_imports)]
pub mod window {

    use azul_dll::*;
    use crate::css::Css;


    /// `WindowCreateOptions` struct
    pub struct WindowCreateOptions { pub(crate) ptr: AzWindowCreateOptionsPtr }

    impl WindowCreateOptions {
        /// Creates a new `WindowCreateOptions` instance.
        pub fn new(css: Css) -> Self { Self { ptr: az_window_create_options_new(css.leak()) } }
       /// Prevents the destructor from running and returns the internal `AzWindowCreateOptionsPtr`
       pub fn leak(self) -> AzWindowCreateOptionsPtr { let p = az_window_create_options_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for WindowCreateOptions { fn drop(&mut self) { az_window_create_options_delete(&mut self.ptr); } }
}

