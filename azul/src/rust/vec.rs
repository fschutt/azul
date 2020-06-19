    #![allow(dead_code, unused_imports)]
    //! Definition of azuls internal `Vec<*>` wrappers
    use crate::dll::*;
    use std::ffi::c_void;
    macro_rules! impl_vec {($struct_type:ident, $struct_name:ident) => (

        impl $struct_name {

            pub fn new() -> Self {
                Vec::<$struct_type>::new().into()
            }

            pub fn sort_by<F: FnMut(&$struct_type, &$struct_type) -> std::cmp::Ordering>(&mut self, compare: F) {
                let v1: &mut [$struct_type] = unsafe { std::slice::from_raw_parts_mut(self.ptr as *mut $struct_type, self.len) };
                v1.sort_by(compare);
            }

            pub fn with_capacity(cap: usize) -> Self {
                Vec::<$struct_type>::with_capacity(cap).into()
            }

            pub fn push(&mut self, val: $struct_type) {
                let mut v: Vec<$struct_type> = unsafe { Vec::from_raw_parts(self.ptr as *mut $struct_type, self.len, self.cap) };
                v.push(val);
                let (ptr, len, cap) = Self::into_raw_parts(v);
                self.ptr = ptr;
                self.len = len;
                self.cap = cap;
            }

            pub fn iter(&self) -> std::slice::Iter<$struct_type> {
                let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                v1.iter()
            }

            pub fn iter_mut(&mut self) -> std::slice::IterMut<$struct_type> {
                let v1: &mut [$struct_type] = unsafe { std::slice::from_raw_parts_mut(self.ptr as *mut $struct_type, self.len) };
                v1.iter_mut()
            }

            pub fn into_iter(self) -> std::vec::IntoIter<$struct_type> {
                let v1: Vec<$struct_type> = unsafe { std::vec::Vec::from_raw_parts(self.ptr as *mut $struct_type, self.len, self.cap) };
                std::mem::forget(self); // do not run destructor of self
                v1.into_iter()
            }

            pub fn as_ptr(&self) -> *const $struct_type {
                self.ptr as *const $struct_type
            }

            pub fn len(&self) -> usize {
                self.len
            }

            pub fn is_empty(&self) -> bool {
                self.len == 0
            }

            pub fn cap(&self) -> usize {
                self.cap
            }

            pub fn get(&self, index: usize) -> Option<&$struct_type> {
                let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                let res = v1.get(index);
                std::mem::forget(v1);
                res
            }

            pub fn foreach<U, F: FnMut(&$struct_type) -> Result<(), U>>(&self, mut closure: F) -> Result<(), U> {
                let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                for i in v1.iter() { closure(i)?; }
                std::mem::forget(v1);
                Ok(())
            }

            /// Same as Vec::into_raw_parts(self), prevents destructor from running
            fn into_raw_parts(mut v: Vec<$struct_type>) -> (*mut $struct_type, usize, usize) {
                let ptr = v.as_mut_ptr();
                let len = v.len();
                let cap = v.capacity();
                std::mem::forget(v);
                (ptr, len, cap)
            }
        }

        impl Default for $struct_name {
            fn default() -> Self {
                Vec::<$struct_type>::default().into()
            }
        }

        impl std::iter::FromIterator<$struct_type> for $struct_name {
            fn from_iter<T>(iter: T) -> Self where T: IntoIterator<Item = $struct_type> {
                let v: Vec<$struct_type> = Vec::from_iter(iter);
                v.into()
            }
        }

        impl AsRef<[$struct_type]> for $struct_name {
            fn as_ref(&self) -> &[$struct_type] {
                unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
            }
        }

        impl From<Vec<$struct_type>> for $struct_name {
            fn from(v: Vec<$struct_type>) -> $struct_name {
                $struct_name::copy_from(v.as_ptr(), v.len())
            }
        }

        impl From<$struct_name> for Vec<$struct_type> {
            fn from(v: $struct_name) -> Vec<$struct_type> {
                unsafe { std::slice::from_raw_parts(v.as_ptr(), v.len()) }.to_vec()
            }
        }
    )}

    impl_vec!(u8, U8Vec);
    impl_vec!(CallbackData, CallbackDataVec);
    impl_vec!(OverrideProperty, OverridePropertyVec);
    impl_vec!(Dom, DomVec);
    impl_vec!(AzString, StringVec);
    impl_vec!(GradientStopPre, GradientStopPreVec);
    impl_vec!(DebugMessage, DebugMessageVec);

    impl From<std::vec::Vec<std::string::String>> for crate::vec::StringVec {
        fn from(v: std::vec::Vec<std::string::String>) -> crate::vec::StringVec {
            let vec: Vec<AzString> = v.into_iter().map(|i| {
                let i: std::vec::Vec<u8> = i.into_bytes();
                (crate::dll::get_azul_dll().az_string_from_utf8_unchecked)(i.as_ptr(), i.len())
            }).collect();

            (crate::dll::get_azul_dll().az_string_vec_copy_from)(vec.as_ptr(), vec.len())
        }
    }

    impl From<crate::vec::StringVec> for std::vec::Vec<std::string::String> {
        fn from(v: crate::vec::StringVec) -> std::vec::Vec<std::string::String> {
            unsafe { std::slice::from_raw_parts(v.ptr, v.len) }
            .iter()
            .map(|s| unsafe {
                let s: AzString = (crate::dll::get_azul_dll().az_string_deep_copy)(s);
                let s_vec: std::vec::Vec<u8> = s.into_bytes().into();
                std::string::String::from_utf8_unchecked(s_vec)
            })
            .collect()

            // delete() not necessary because StringVec is stack-allocated
        }
    }    use crate::window::{StringPair, VirtualKeyCode, XWindowType};
    use crate::css::{CssDeclaration, CssPathSelector, CssRuleBlock, GradientStopPre, Stylesheet};
    use crate::dom::{CallbackData, Dom, OverrideProperty};
    use crate::gl::DebugMessage;
    use crate::str::String;


    /// Wrapper over a Rust-allocated `XWindowType`
    pub use crate::dll::AzXWindowTypeVec as XWindowTypeVec;

    impl XWindowTypeVec {
        /// Creates + allocates a Rust `Vec<XWindowType>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzXWindowType, len: usize) -> Self { (crate::dll::get_azul_dll().az_x_window_type_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for XWindowTypeVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_x_window_type_vec_fmt_debug)(self)) } }
    impl Clone for XWindowTypeVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_x_window_type_vec_deep_copy)(self) } }
    impl Drop for XWindowTypeVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_x_window_type_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `VirtualKeyCode`
    pub use crate::dll::AzVirtualKeyCodeVec as VirtualKeyCodeVec;

    impl VirtualKeyCodeVec {
        /// Creates + allocates a Rust `Vec<VirtualKeyCode>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzVirtualKeyCode, len: usize) -> Self { (crate::dll::get_azul_dll().az_virtual_key_code_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for VirtualKeyCodeVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_virtual_key_code_vec_fmt_debug)(self)) } }
    impl Clone for VirtualKeyCodeVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_virtual_key_code_vec_deep_copy)(self) } }
    impl Drop for VirtualKeyCodeVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_virtual_key_code_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `ScanCode`
    pub use crate::dll::AzScanCodeVec as ScanCodeVec;

    impl ScanCodeVec {
        /// Creates + allocates a Rust `Vec<ScanCode>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u32, len: usize) -> Self { (crate::dll::get_azul_dll().az_scan_code_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for ScanCodeVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_scan_code_vec_fmt_debug)(self)) } }
    impl Clone for ScanCodeVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_scan_code_vec_deep_copy)(self) } }
    impl Drop for ScanCodeVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_scan_code_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CssDeclaration`
    pub use crate::dll::AzCssDeclarationVec as CssDeclarationVec;

    impl CssDeclarationVec {
        /// Creates + allocates a Rust `Vec<CssDeclaration>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzCssDeclaration, len: usize) -> Self { (crate::dll::get_azul_dll().az_css_declaration_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for CssDeclarationVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_declaration_vec_fmt_debug)(self)) } }
    impl Clone for CssDeclarationVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_declaration_vec_deep_copy)(self) } }
    impl Drop for CssDeclarationVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_declaration_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CssPathSelector`
    pub use crate::dll::AzCssPathSelectorVec as CssPathSelectorVec;

    impl CssPathSelectorVec {
        /// Creates + allocates a Rust `Vec<CssPathSelector>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzCssPathSelector, len: usize) -> Self { (crate::dll::get_azul_dll().az_css_path_selector_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for CssPathSelectorVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_path_selector_vec_fmt_debug)(self)) } }
    impl Clone for CssPathSelectorVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_path_selector_vec_deep_copy)(self) } }
    impl Drop for CssPathSelectorVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_path_selector_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `Stylesheet`
    pub use crate::dll::AzStylesheetVec as StylesheetVec;

    impl StylesheetVec {
        /// Creates + allocates a Rust `Vec<Stylesheet>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzStylesheet, len: usize) -> Self { (crate::dll::get_azul_dll().az_stylesheet_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for StylesheetVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_stylesheet_vec_fmt_debug)(self)) } }
    impl Clone for StylesheetVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_stylesheet_vec_deep_copy)(self) } }
    impl Drop for StylesheetVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_stylesheet_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CssRuleBlock`
    pub use crate::dll::AzCssRuleBlockVec as CssRuleBlockVec;

    impl CssRuleBlockVec {
        /// Creates + allocates a Rust `Vec<CssRuleBlock>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzCssRuleBlock, len: usize) -> Self { (crate::dll::get_azul_dll().az_css_rule_block_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for CssRuleBlockVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_rule_block_vec_fmt_debug)(self)) } }
    impl Clone for CssRuleBlockVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_rule_block_vec_deep_copy)(self) } }
    impl Drop for CssRuleBlockVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_rule_block_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `U8Vec`
    pub use crate::dll::AzU8Vec as U8Vec;

    impl U8Vec {
        /// Creates + allocates a Rust `Vec<u8>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u8, len: usize) -> Self { (crate::dll::get_azul_dll().az_u8_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for U8Vec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_u8_vec_fmt_debug)(self)) } }
    impl Clone for U8Vec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_u8_vec_deep_copy)(self) } }
    impl Drop for U8Vec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_u8_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CallbackData`
    pub use crate::dll::AzCallbackDataVec as CallbackDataVec;

    impl CallbackDataVec {
        /// Creates + allocates a Rust `Vec<CallbackData>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzCallbackData, len: usize) -> Self { (crate::dll::get_azul_dll().az_callback_data_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for CallbackDataVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_callback_data_vec_fmt_debug)(self)) } }
    impl Clone for CallbackDataVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_callback_data_vec_deep_copy)(self) } }
    impl Drop for CallbackDataVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_callback_data_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `Vec<DebugMessage>`
    pub use crate::dll::AzDebugMessageVec as DebugMessageVec;

    impl DebugMessageVec {
        /// Creates + allocates a Rust `Vec<DebugMessage>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzDebugMessage, len: usize) -> Self { (crate::dll::get_azul_dll().az_debug_message_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for DebugMessageVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_debug_message_vec_fmt_debug)(self)) } }
    impl Clone for DebugMessageVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_debug_message_vec_deep_copy)(self) } }
    impl Drop for DebugMessageVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_debug_message_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `U32Vec`
    pub use crate::dll::AzGLuintVec as GLuintVec;

    impl GLuintVec {
        /// Creates + allocates a Rust `Vec<u32>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u32, len: usize) -> Self { (crate::dll::get_azul_dll().az_g_luint_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for GLuintVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_g_luint_vec_fmt_debug)(self)) } }
    impl Clone for GLuintVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_g_luint_vec_deep_copy)(self) } }
    impl Drop for GLuintVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_luint_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `GLintVec`
    pub use crate::dll::AzGLintVec as GLintVec;

    impl GLintVec {
        /// Creates + allocates a Rust `Vec<u32>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const i32, len: usize) -> Self { (crate::dll::get_azul_dll().az_g_lint_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for GLintVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_g_lint_vec_fmt_debug)(self)) } }
    impl Clone for GLintVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_g_lint_vec_deep_copy)(self) } }
    impl Drop for GLintVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lint_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `OverridePropertyVec`
    pub use crate::dll::AzOverridePropertyVec as OverridePropertyVec;

    impl OverridePropertyVec {
        /// Creates + allocates a Rust `Vec<OverrideProperty>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzOverrideProperty, len: usize) -> Self { (crate::dll::get_azul_dll().az_override_property_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for OverridePropertyVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_override_property_vec_fmt_debug)(self)) } }
    impl Clone for OverridePropertyVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_override_property_vec_deep_copy)(self) } }
    impl Drop for OverridePropertyVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_override_property_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `DomVec`
    pub use crate::dll::AzDomVec as DomVec;

    impl DomVec {
        /// Creates + allocates a Rust `Vec<Dom>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzDom, len: usize) -> Self { (crate::dll::get_azul_dll().az_dom_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for DomVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_dom_vec_fmt_debug)(self)) } }
    impl Clone for DomVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_dom_vec_deep_copy)(self) } }
    impl Drop for DomVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_dom_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `StringVec`
    pub use crate::dll::AzStringVec as StringVec;

    impl StringVec {
        /// Creates + allocates a Rust `Vec<String>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzString, len: usize) -> Self { (crate::dll::get_azul_dll().az_string_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for StringVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_string_vec_fmt_debug)(self)) } }
    impl Clone for StringVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_string_vec_deep_copy)(self) } }
    impl Drop for StringVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `StringPairVec`
    pub use crate::dll::AzStringPairVec as StringPairVec;

    impl StringPairVec {
        /// Creates + allocates a Rust `Vec<StringPair>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzStringPair, len: usize) -> Self { (crate::dll::get_azul_dll().az_string_pair_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for StringPairVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_string_pair_vec_fmt_debug)(self)) } }
    impl Clone for StringPairVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_string_pair_vec_deep_copy)(self) } }
    impl Drop for StringPairVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_pair_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `GradientStopPreVec`
    pub use crate::dll::AzGradientStopPreVec as GradientStopPreVec;

    impl GradientStopPreVec {
        /// Creates + allocates a Rust `Vec<GradientStopPre>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzGradientStopPre, len: usize) -> Self { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for GradientStopPreVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_fmt_debug)(self)) } }
    impl Clone for GradientStopPreVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_deep_copy)(self) } }
    impl Drop for GradientStopPreVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_delete)(self); } }
