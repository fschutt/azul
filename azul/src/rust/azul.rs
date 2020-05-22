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
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzStringPtr { let p = az_string_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

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
    }

    /// Wrapper over a Rust-allocated `Vec<u8>`
    pub struct U8Vec { pub(crate) ptr: AzU8VecPtr }

    impl U8Vec {
        /// Creates + allocates a Rust `Vec<u8>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u8, len: usize) -> Self { Self { ptr: az_u8_vec_copy_from(ptr, len) } }
       /// Prevents the destructor from running and returns the internal `AzU8VecPtr`
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzU8VecPtr { let p = az_u8_vec_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

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
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzPathBufPtr { let p = az_path_buf_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

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
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzAppConfigPtr { let p = az_app_config_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }



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
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzAppPtr { let p = az_app_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

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


pub type CallbackReturn = AzUpdateScreen;
/// Callback for responding to window events
pub type Callback = fn(AzCallbackInfoPtr) -> AzCallbackReturn;

    /// `CallbackInfo` struct
    pub struct CallbackInfo { pub(crate) ptr: AzCallbackInfoPtr }

    impl CallbackInfo {
       /// Prevents the destructor from running and returns the internal `AzCallbackInfoPtr`
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzCallbackInfoPtr { let p = az_callback_info_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }



    /// `UpdateScreen` struct
    pub struct UpdateScreen { pub(crate) object: AzUpdateScreen }

    impl<T> From<Option<T>> for UpdateScreen { fn from(o: Option<T>) -> Self { Self { object: match o { None => AzDontRedraw, Some(_) => AzRedraw }} } }


    /// `Redraw` struct
    pub static Redraw: AzUpdateScreen = AzRedraw;



    /// `DontRedraw` struct
    pub static DontRedraw: AzUpdateScreen = AzDontRedraw;



/// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
pub type IFrameCallback = fn(AzIFrameCallbackInfoPtr) -> AzIFrameCallbackReturnPtr;

    /// `IFrameCallbackInfo` struct
    pub struct IFrameCallbackInfo { pub(crate) ptr: AzIFrameCallbackInfoPtr }

    impl IFrameCallbackInfo {
       /// Prevents the destructor from running and returns the internal `AzIFrameCallbackInfoPtr`
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzIFrameCallbackInfoPtr { let p = az_i_frame_callback_info_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }



    /// `IFrameCallbackReturn` struct
    pub struct IFrameCallbackReturn { pub(crate) ptr: AzIFrameCallbackReturnPtr }

    impl IFrameCallbackReturn {
       /// Prevents the destructor from running and returns the internal `AzIFrameCallbackReturnPtr`
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzIFrameCallbackReturnPtr { let p = az_i_frame_callback_return_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }



/// Callback for rendering to an OpenGL texture
pub type GlCallback = fn(AzGlCallbackInfoPtr) -> AzGlCallbackReturnPtr;

    /// `GlCallbackInfo` struct
    pub struct GlCallbackInfo { pub(crate) ptr: AzGlCallbackInfoPtr }

    impl GlCallbackInfo {
       /// Prevents the destructor from running and returns the internal `AzGlCallbackInfoPtr`
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzGlCallbackInfoPtr { let p = az_gl_callback_info_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }



    /// `GlCallbackReturn` struct
    pub struct GlCallbackReturn { pub(crate) ptr: AzGlCallbackReturnPtr }

    impl GlCallbackReturn {
       /// Prevents the destructor from running and returns the internal `AzGlCallbackReturnPtr`
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzGlCallbackReturnPtr { let p = az_gl_callback_return_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }



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
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzLayoutInfoPtr { let p = az_layout_info_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

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
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzCssPtr { let p = az_css_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

}

/// `Dom` construction and configuration
#[allow(dead_code, unused_imports)]
pub mod dom {

    use azul_dll::*;
    use crate::str::String;


    /// `Dom` struct
    pub struct Dom { pub(crate) ptr: AzDomPtr }

    impl Dom {
        /// Creates a new `div` node
        pub fn div() -> Self { Self { ptr: az_dom_div() } }
        /// Creates a new `body` node
        pub fn body() -> Self { Self { ptr: az_dom_body() } }
        /// Creates a new `Dom` instance.
        pub fn label(text: String) -> Self { Self { ptr: az_dom_label(text.leak()) } }
        /// Reparents another `Dom` to be the child node of this `Dom`
        pub fn add_child(&mut self, child: Dom)  { az_dom_add_child(&mut self.ptr, child.leak()) }
        /// Same as [`Dom::add_child`](#method.add_child), but as a builder method
        pub fn with_child(self, child: Dom)  -> Dom { Dom { ptr: { az_dom_with_child(self.leak(), child.leak())} } }
       /// Prevents the destructor from running and returns the internal `AzDomPtr`
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzDomPtr { let p = az_dom_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

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
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzTextId { az_text_id_deep_copy(&self.object) }
    }



    /// `ImageId` struct
    pub struct ImageId { pub(crate) object: AzImageId }

    impl ImageId {
        /// Creates a new, unique `ImageId`
        pub fn new() -> Self { Self { object: az_image_id_new() } }
       /// Prevents the destructor from running and returns the internal `AzImageId`
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzImageId { az_image_id_deep_copy(&self.object) }
    }



    /// `FontId` struct
    pub struct FontId { pub(crate) object: AzFontId }

    impl FontId {
        /// Creates a new, unique `FontId`
        pub fn new() -> Self { Self { object: az_font_id_new() } }
       /// Prevents the destructor from running and returns the internal `AzFontId`
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzFontId { az_font_id_deep_copy(&self.object) }
    }



    /// `ImageSource` struct
    pub struct ImageSource { pub(crate) object: AzImageSource }

    impl ImageSource {
/// Bytes of the image, encoded in PNG / JPG / etc. format
/// References an (encoded!) image as a file from the file system that is loaded when necessary
/// References a decoded (!) `RawImage` as the image source
       /// Prevents the destructor from running and returns the internal `AzImageSource`
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzImageSource { az_image_source_deep_copy(&self.object) }
    }



    /// `FontSource` struct
    pub struct FontSource { pub(crate) object: AzFontSource }

    impl FontSource {
/// Bytes are the bytes of the font file
/// References a font from a file path, which is loaded when necessary
/// References a font from from a system font identifier, such as `"Arial"` or `"Helvetica"`
       /// Prevents the destructor from running and returns the internal `AzFontSource`
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzFontSource { az_font_source_deep_copy(&self.object) }
    }



    /// `RawImage` struct
    pub struct RawImage { pub(crate) ptr: AzRawImagePtr }

    impl RawImage {
        /// Creates a new `RawImage` by loading the decoded bytes
        pub fn new(decoded_pixels: U8Vec, width: usize, height: usize, data_format: RawImageFormat) -> Self { Self { ptr: az_raw_image_new(decoded_pixels.leak(), width, height, data_format.leak()) } }
       /// Prevents the destructor from running and returns the internal `AzRawImagePtr`
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzRawImagePtr { let p = az_raw_image_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }



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
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzRawImageFormat { az_raw_image_format_deep_copy(&self.object) }
    }

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
       #[allow(dead_code)]
       pub(crate) fn leak(self) -> AzWindowCreateOptionsPtr { let p = az_window_create_options_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

}

