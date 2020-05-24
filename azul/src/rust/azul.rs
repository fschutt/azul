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
    pub struct String { pub(crate) ptr: AzStringPtr }

    impl String {
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn from_utf8_unchecked(ptr: *const u8, len: usize) -> Self { Self { ptr: az_string_from_utf8_unchecked(ptr, len) } }
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn from_utf8_lossy(ptr: *const u8, len: usize) -> Self { Self { ptr: az_string_from_utf8_lossy(ptr, len) } }
       /// Prevents the destructor from running and returns the internal `AzStringPtr`
       pub fn leak(self) -> AzStringPtr { let p = az_string_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for String { fn drop(&mut self) { az_string_delete(&mut self.ptr); } }
}

/// Definition of azuls internal `U8Vec` type + functions for conversion from `std::Vec`
#[allow(dead_code, unused_imports)]
pub mod vec {

    use azul_dll::*;

    impl From<std::vec::Vec<u8>> for crate::vec::U8Vec {
        fn from(v: std::vec::Vec<u8>) -> crate::vec::U8Vec {
            crate::vec::U8Vec::copy_from(v.as_ptr(), v.len()) // - copies v into a new Vec<u8>
            // - v is deallocated here
        }
    }    use crate::str::String;


    /// Wrapper over a Rust-allocated `Vec<u8>`
    pub struct U8Vec { pub(crate) ptr: AzU8VecPtr }

    impl U8Vec {
        /// Creates + allocates a Rust `Vec<u8>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u8, len: usize) -> Self { Self { ptr: az_u8_vec_copy_from(ptr, len) } }
       /// Prevents the destructor from running and returns the internal `AzU8VecPtr`
       pub fn leak(self) -> AzU8VecPtr { let p = az_u8_vec_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for U8Vec { fn drop(&mut self) { az_u8_vec_delete(&mut self.ptr); } }


    /// Wrapper over a Rust-allocated `Vec<String>`
    pub struct StringVec { pub(crate) ptr: AzStringVecPtr }

    impl StringVec {
        /// Creates + allocates a Rust `Vec<String>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const String, len: usize) -> Self { Self { ptr: az_string_vec_copy_from(ptr.leak(), len) } }
       /// Prevents the destructor from running and returns the internal `AzStringVecPtr`
       pub fn leak(self) -> AzStringVecPtr { let p = az_string_vec_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StringVec { fn drop(&mut self) { az_string_vec_delete(&mut self.ptr); } }
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

/// Definition of azuls internal `Duration` type + functions for conversion from `std::time::Duration`
#[allow(dead_code, unused_imports)]
pub mod time {

    use azul_dll::*;


    /// Wrapper over a Rust-allocated `Duration`
    pub struct Duration { pub(crate) ptr: AzDurationPtr }

    impl Duration {
        /// Creates a new `Duration` from milliseconds
        pub fn from_millis(millis: u64) -> Self { Self { ptr: az_duration_from_millis(millis) } }
       /// Prevents the destructor from running and returns the internal `AzDurationPtr`
       pub fn leak(self) -> AzDurationPtr { let p = az_duration_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for Duration { fn drop(&mut self) { az_duration_delete(&mut self.ptr); } }
}

/// `App` construction and configuration
#[allow(dead_code, unused_imports)]
pub mod app {

    use azul_dll::*;
    use crate::callbacks::{RefAny, LayoutCallback};
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
    use crate::str::String;


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


    /// `CssPropertyKey` struct
    pub struct CssPropertyKey { pub(crate) ptr: AzCssPropertyKeyPtr }

    impl CssPropertyKey {
       /// Prevents the destructor from running and returns the internal `AzCssPropertyKeyPtr`
       pub fn leak(self) -> AzCssPropertyKeyPtr { let p = az_css_property_key_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for CssPropertyKey { fn drop(&mut self) { az_css_property_key_delete(&mut self.ptr); } }


    /// Parsed CSS key-value pair
    pub struct CssProperty { pub(crate) ptr: AzCssPropertyPtr }

    impl CssProperty {
        /// Parses a new CssProperty from a string
        pub fn parse_from_string(key: CssPropertyKey, value: String) -> Self { Self { ptr: az_css_property_parse_from_string(key.leak(), value.leak()) } }
       /// Prevents the destructor from running and returns the internal `AzCssPropertyPtr`
       pub fn leak(self) -> AzCssPropertyPtr { let p = az_css_property_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for CssProperty { fn drop(&mut self) { az_css_property_delete(&mut self.ptr); } }
}

/// `Dom` construction and configuration
#[allow(dead_code, unused_imports)]
pub mod dom {

    use azul_dll::*;
    use crate::str::{String, String, String, String, String, String, String};
    use crate::resources::{TextId, ImageId};
    use crate::callbacks::{RefAny, GlCallback, RefAny, GlCallback, RefAny, Callback, RefAny, Callback};
    use crate::vec::{Vec<String>, Vec<String>, Vec<String>, Vec<String>};
    use crate::css::{CssProperty, CssProperty};


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
        pub fn gl_callback(data: RefAny, callback: GlCallback) -> Self { Self { ptr: az_dom_gl_callback(data.leak(), callback.leak()) } }
        /// Creates a new node with a callback that will return a `Dom` after being layouted. See the documentation for [IFrameCallback]() for more info about iframe callbacks.
        pub fn iframe_callback(data: RefAny, callback: GlCallback) -> Self { Self { ptr: az_dom_iframe_callback(data.leak(), callback.leak()) } }
        /// Adds a CSS ID (`#something`) to the DOM node
        pub fn add_id(&mut self, id: String)  { az_dom_add_id(&mut self.ptr, id.leak()) }
        /// Same as [`Dom::add_id`](#method.add_id), but as a builder method
        pub fn with_id(self, id: String)  -> Dom { Dom { ptr: { az_dom_with_id(self.leak(), id.leak())} } }
        /// Same as calling [`Dom::add_id`](#method.add_id) for each CSS ID, but this function **replaces** all current CSS IDs
        pub fn set_ids(&mut self, ids: Vec<String>)  { az_dom_set_ids(&mut self.ptr, ids.leak()) }
        /// Same as [`Dom::set_ids`](#method.set_ids), but as a builder method
        pub fn with_ids(self, ids: Vec<String>)  -> Dom { Dom { ptr: { az_dom_with_ids(self.leak(), ids.leak())} } }
        /// Adds a CSS class (`.something`) to the DOM node
        pub fn add_class(&mut self, class: String)  { az_dom_add_class(&mut self.ptr, class.leak()) }
        /// Same as [`Dom::add_class`](#method.add_class), but as a builder method
        pub fn with_class(self, class: String)  -> Dom { Dom { ptr: { az_dom_with_class(self.leak(), class.leak())} } }
        /// Same as calling [`Dom::add_class`](#method.add_class) for each class, but this function **replaces** all current classes
        pub fn set_classes(&mut self, classes: Vec<String>)  { az_dom_set_classes(&mut self.ptr, classes.leak()) }
        /// Same as [`Dom::set_classes`](#method.set_classes), but as a builder method
        pub fn with_classes(self, classes: Vec<String>)  -> Dom { Dom { ptr: { az_dom_with_classes(self.leak(), classes.leak())} } }
        /// Adds a [`Callback`](callbacks/type.Callback) that acts on the `data` the `event` happens
        pub fn add_callback(&mut self, event: EventFilter, data: RefAny, callback: Callback)  { az_dom_add_callback(&mut self.ptr, event.leak(), data.leak(), callback.leak()) }
        /// Same as [`Dom::add_callback`](#method.add_callback), but as a builder method
        pub fn with_callback(&mut self, event: EventFilter, data: RefAny, callback: Callback)  { az_dom_with_callback(&mut self.ptr, event.leak(), data.leak(), callback.leak()) }
        /// Overrides the CSS property of this DOM node with a value (for example `"width = 200px"`)
        pub fn add_dynamic_css_override(&mut self, prop: CssProperty)  { az_dom_add_dynamic_css_override(&mut self.ptr, prop.leak()) }
        /// Same as [`Dom::add_dynamic_css_override`](#method.add_dynamic_css_override), but as a builder method
        pub fn with_dynamic_css_override(&mut self, prop: CssProperty)  { az_dom_with_dynamic_css_override(&mut self.ptr, prop.leak()) }
        /// Sets the `is_draggable` attribute of this DOM node (default: false)
        pub fn set_is_draggable(&mut self, is_draggable: bool)  { az_dom_set_is_draggable(&mut self.ptr, is_draggable) }
        /// Same as [`Dom::set_is_draggable`](#method.set_is_draggable), but as a builder method
        pub fn is_draggable(&mut self, is_draggable: bool)  { az_dom_is_draggable(&mut self.ptr, is_draggable) }
        /// Sets the `tabindex` attribute of this DOM node (makes an element focusable - default: None)
        pub fn set_tab_index(&mut self, tab_index: TabIndex)  { az_dom_set_tab_index(&mut self.ptr, tab_index.leak()) }
        /// Same as [`Dom::set_tab_index`](#method.set_tab_index), but as a builder method
        pub fn with_tab_index(&mut self, tab_index: TabIndex)  { az_dom_with_tab_index(&mut self.ptr, tab_index.leak()) }
        /// Reparents another `Dom` to be the child node of this `Dom`
        pub fn add_child(&mut self, child: Dom)  { az_dom_add_child(&mut self.ptr, child.leak()) }
        /// Same as [`Dom::add_child`](#method.add_child), but as a builder method
        pub fn with_child(self, child: Dom)  -> Dom { Dom { ptr: { az_dom_with_child(self.leak(), child.leak())} } }
        /// Returns if the DOM node has a certain CSS ID
        pub fn has_id(&self, id: String)  -> bool { bool { ptr: { az_dom_has_id(&self.ptr, id.leak())} } }
        /// Returns if the DOM node has a certain CSS class
        pub fn has_class(&self, class: String)  -> bool { bool { ptr: { az_dom_has_class(&self.ptr, class.leak())} } }
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

