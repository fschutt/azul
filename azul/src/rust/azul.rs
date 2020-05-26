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

/// Module to re-export common structs (`App`, `AppConfig`, `Css`, `Dom`, `WindowCreateOptions`, `RefAny`, `LayoutInfo`)
pub mod prelude {
    pub use crate::{
        app::{App, AppConfig},
        css::Css,
        dom::Dom,
        window::WindowCreateOptions,
        callbacks::{RefAny, LayoutInfo},
    };
}/// Definition of azuls internal String type + functions for conversion from `std::String`
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
            crate::vec::StringVec { object: v.into_iter().map(|i| azul_dll::AzString::copy_from(i.as_ptr(), i.len())).collect() }
        }
    }

    impl From<crate::vec::StringVec> for std::vec::Vec<std::string::String> {
        fn from(v: crate::vec::StringVec) -> std::vec::Vec<std::string::String> {
            v.object.object
            .into_iter()
            .map(|s| unsafe { std::string::String::from_utf8_unchecked(s.as_ptr(), s.len()) })
            .collect()
        }
    }

    /// Wrapper over a Rust-allocated `Vec<u8>`
    pub struct U8Vec { pub(crate) object: AzU8Vec }

    impl U8Vec {
        /// Creates + allocates a Rust `Vec<u8>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u8, len: usize) -> Self { Self { object: az_u8_vec_copy_from(ptr, len) } }
       /// Prevents the destructor from running and returns the internal `AzU8Vec`
       pub fn leak(self) -> AzU8Vec { az_u8_vec_deep_copy(&self.object) }
    }

    impl Drop for U8Vec { fn drop(&mut self) { az_u8_vec_delete(&mut self.object); } }


    /// Wrapper over a Rust-allocated `Vec<String>`
    pub struct StringVec { pub(crate) object: AzStringVec }

    impl StringVec {
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
        pub fn new(path: String) -> Self { Self { ptr: az_path_buf_new(path.object) } }
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
    use crate::callbacks::{RefAny, IFrameCallback, GlCallback, Callback};
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
        pub fn label(text: String) -> Self { Self { ptr: az_dom_label(text.object) } }
        /// Creates a new `p` node from a (cached) text referenced by a `TextId`
        pub fn text(text_id: TextId) -> Self { Self { ptr: az_dom_text(text_id.object) } }
        /// Creates a new `img` node from a (cached) text referenced by a `ImageId`
        pub fn image(image_id: ImageId) -> Self { Self { ptr: az_dom_image(image_id.object) } }
        /// Creates a new node which will render an OpenGL texture after the layout step is finished. See the documentation for [GlCallback]() for more info about OpenGL rendering callbacks.
        pub fn gl_texture(data: RefAny, callback: GlCallback) -> Self { Self { ptr: az_dom_gl_texture(data.leak(), callback) } }
        /// Creates a new node with a callback that will return a `Dom` after being layouted. See the documentation for [IFrameCallback]() for more info about iframe callbacks.
        pub fn iframe_callback(data: RefAny, callback: IFrameCallback) -> Self { Self { ptr: az_dom_iframe_callback(data.leak(), callback) } }
        /// Adds a CSS ID (`#something`) to the DOM node
        pub fn add_id(&mut self, id: String)  { az_dom_add_id(&mut self.ptr, id.object) }
        /// Same as [`Dom::add_id`](#method.add_id), but as a builder method
        pub fn with_id(self, id: String)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_id(self.leak(), id.object) } } }
        /// Same as calling [`Dom::add_id`](#method.add_id) for each CSS ID, but this function **replaces** all current CSS IDs
        pub fn set_ids(&mut self, ids: StringVec)  { az_dom_set_ids(&mut self.ptr, ids.object) }
        /// Same as [`Dom::set_ids`](#method.set_ids), but as a builder method
        pub fn with_ids(self, ids: StringVec)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_ids(self.leak(), ids.object) } } }
        /// Adds a CSS class (`.something`) to the DOM node
        pub fn add_class(&mut self, class: String)  { az_dom_add_class(&mut self.ptr, class.object) }
        /// Same as [`Dom::add_class`](#method.add_class), but as a builder method
        pub fn with_class(self, class: String)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_class(self.leak(), class.object) } } }
        /// Same as calling [`Dom::add_class`](#method.add_class) for each class, but this function **replaces** all current classes
        pub fn set_classes(&mut self, classes: StringVec)  { az_dom_set_classes(&mut self.ptr, classes.object) }
        /// Same as [`Dom::set_classes`](#method.set_classes), but as a builder method
        pub fn with_classes(self, classes: StringVec)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_classes(self.leak(), classes.object) } } }
        /// Adds a [`Callback`](callbacks/type.Callback) that acts on the `data` the `event` happens
        pub fn add_callback(&mut self, event: EventFilter, data: RefAny, callback: Callback)  { az_dom_add_callback(&mut self.ptr, event.object, data.leak(), callback) }
        /// Same as [`Dom::add_callback`](#method.add_callback), but as a builder method
        pub fn with_callback(self, event: EventFilter, data: RefAny, callback: Callback)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_callback(self.leak(), event.object, data.leak(), callback) } } }
        /// Overrides the CSS property of this DOM node with a value (for example `"width = 200px"`)
        pub fn add_css_override(&mut self, id: String, prop: CssProperty)  { az_dom_add_css_override(&mut self.ptr, id.object, prop.object) }
        /// Same as [`Dom::add_css_override`](#method.add_css_override), but as a builder method
        pub fn with_css_override(self, id: String, prop: CssProperty)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_css_override(self.leak(), id.object, prop.object) } } }
        /// Sets the `is_draggable` attribute of this DOM node (default: false)
        pub fn set_is_draggable(&mut self, is_draggable: bool)  { az_dom_set_is_draggable(&mut self.ptr, is_draggable) }
        /// Same as [`Dom::set_is_draggable`](#method.set_is_draggable), but as a builder method
        pub fn is_draggable(self, is_draggable: bool)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_is_draggable(self.leak(), is_draggable) } } }
        /// Sets the `tabindex` attribute of this DOM node (makes an element focusable - default: None)
        pub fn set_tab_index(&mut self, tab_index: TabIndex)  { az_dom_set_tab_index(&mut self.ptr, tab_index.object) }
        /// Same as [`Dom::set_tab_index`](#method.set_tab_index), but as a builder method
        pub fn with_tab_index(self, tab_index: TabIndex)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_tab_index(self.leak(), tab_index.object) } } }
        /// Reparents another `Dom` to be the child node of this `Dom`
        pub fn add_child(&mut self, child: Dom)  { az_dom_add_child(&mut self.ptr, child.leak()) }
        /// Same as [`Dom::add_child`](#method.add_child), but as a builder method
        pub fn with_child(self, child: Dom)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_child(self.leak(), child.leak()) } } }
        /// Returns if the DOM node has a certain CSS ID
        pub fn has_id(&mut self, id: String)  -> bool { az_dom_has_id(&mut self.ptr, id.object) }
        /// Returns if the DOM node has a certain CSS class
        pub fn has_class(&mut self, class: String)  -> bool { az_dom_has_class(&mut self.ptr, class.object) }
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
        pub fn new(decoded_pixels: U8Vec, width: usize, height: usize, data_format: RawImageFormat) -> Self { Self { ptr: az_raw_image_new(decoded_pixels.object, width, height, data_format.object) } }
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

