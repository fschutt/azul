    #![allow(dead_code, unused_imports)]
    //! Definition of azuls internal `Vec<*>` wrappers
    use crate::dll::*;
    use std::ffi::c_void;
    use std::fmt;
    use crate::gl::{
        GLint as AzGLint,
        GLuint as AzGLuint,
    };

    macro_rules! define_vec {($struct_type:ident, $struct_name:ident) => (
        #[repr(C)]
        pub struct $struct_name {
            ptr: *mut $struct_type,
            len: usize,
            cap: usize,
        }
    )}

    macro_rules! impl_vec {($struct_type:ident, $struct_name:ident) => (

        impl $struct_name {

            pub fn new() -> Self {
                Vec::<$struct_type>::new().into()
            }

            pub fn clear(&mut self) {
                let mut v: Vec<$struct_type> = unsafe { Vec::from_raw_parts(self.ptr, self.len, self.cap) };
                v.clear();
                std::mem::forget(v);
            }

            pub fn sort_by<F: FnMut(&$struct_type, &$struct_type) -> std::cmp::Ordering>(&mut self, compare: F) {
                let v1: &mut [$struct_type] = unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) };
                v1.sort_by(compare);
            }

            pub fn with_capacity(cap: usize) -> Self {
                Vec::<$struct_type>::with_capacity(cap).into()
            }

            pub fn push(&mut self, val: $struct_type) {
                let mut v: Vec<$struct_type> = unsafe { Vec::from_raw_parts(self.ptr, self.len, self.cap) };
                v.push(val);
                std::mem::forget(v);
            }

            pub fn iter(&self) -> std::slice::Iter<$struct_type> {
                let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                v1.iter()
            }

            pub fn iter_mut(&mut self) -> std::slice::IterMut<$struct_type> {
                let v1: &mut [$struct_type] = unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) };
                v1.iter_mut()
            }

            pub fn into_iter(self) -> std::vec::IntoIter<$struct_type> {
                let v1: Vec<$struct_type> = unsafe { std::vec::Vec::from_raw_parts(self.ptr, self.len, self.cap) };
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
                res
            }

            pub fn foreach<U, F: FnMut(&$struct_type) -> Result<(), U>>(&self, mut closure: F) -> Result<(), U> {
                let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                for i in v1.iter() { closure(i)?; }
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
            fn from(input: Vec<$struct_type>) -> $struct_name {
                let (ptr, len, cap) = $struct_name::into_raw_parts(input);
                $struct_name { ptr, len, cap }
            }
        }

        impl From<$struct_name> for Vec<$struct_type> {
            fn from(input: $struct_name) -> Vec<$struct_type> {
                let v = unsafe { Vec::from_raw_parts(input.ptr, input.len, input.cap) };
                std::mem::forget(input); // don't run the destructor of "input"
                v
            }
        }
    )}

    macro_rules! impl_vec_as_hashmap {($struct_type:ident, $struct_name:ident) => (
        impl $struct_name {
            pub fn insert_hm_item(&mut self, item: $struct_type) {
                if !self.contains_hm_item(&item) {
                    self.push(item);
                }
            }

            pub fn contains_hm_item(&self, searched: &$struct_type) -> bool {
                let v1: &mut [$struct_type] = unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) };
                v1.iter().any(|i| i == searched)
            }

            pub fn remove_hm_item(&mut self, remove_key: &$struct_type) {
                let mut v: Vec<$struct_type> = unsafe { Vec::from_raw_parts(self.ptr, self.len, self.cap) };
                v.retain(|v| v == remove_key);
                std::mem::forget(v);
            }
        }
    )}

    macro_rules! impl_vec_debug {($struct_type:ident, $struct_name:ident) => (
        impl std::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                let res = v1.fmt(f);
                res
            }
        }
    )}

    macro_rules! impl_vec_partialord {($struct_type:ident, $struct_name:ident) => (
        impl PartialOrd for $struct_name {
            fn partial_cmp(&self, rhs: &Self) -> Option<std::cmp::Ordering> {
                let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                let v2: &[$struct_type] = unsafe { std::slice::from_raw_parts(rhs.ptr, rhs.len) };
                v1.partial_cmp(&v2)
            }
        }
    )}

    macro_rules! impl_vec_ord {($struct_type:ident, $struct_name:ident) => (
        impl Ord for $struct_name {
            fn cmp(&self, rhs: &Self) -> std::cmp::Ordering {
                let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                let v2: &[$struct_type] = unsafe { std::slice::from_raw_parts(rhs.ptr, rhs.len) };
                v1.cmp(&v2)
            }
        }
    )}

    macro_rules! impl_vec_clone {($struct_type:ident, $struct_name:ident) => (
        impl Clone for $struct_name {
            fn clone(&self) -> Self {
                let v: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                let v2 = v.to_vec();
                let (ptr, len, cap) = $struct_name::into_raw_parts(v2);
                $struct_name { ptr, len, cap }
            }
        }
    )}

    macro_rules! impl_vec_partialeq {($struct_type:ident, $struct_name:ident) => (
        impl PartialEq for $struct_name {
            fn eq(&self, other: &Self) -> bool {
                let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                let v2: &[$struct_type] = unsafe { std::slice::from_raw_parts(other.ptr, other.len) };
                v1.eq(v2)
            }
        }
    )}

    macro_rules! impl_vec_eq {($struct_type:ident, $struct_name:ident) => (
        impl Eq for $struct_name { }
    )}

    macro_rules! impl_vec_hash {($struct_type:ident, $struct_name:ident) => (
        impl std::hash::Hash for $struct_name {
            fn hash<H>(&self, state: &mut H) where H: std::hash::Hasher {
                let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                v1.hash(state);
            }
        }
    )}

    impl_vec!(u8, AzU8Vec);
    impl_vec_partialord!(u8, AzU8Vec);
    impl_vec_ord!(u8, AzU8Vec);
    impl_vec_partialeq!(u8, AzU8Vec);
    impl_vec_eq!(u8, AzU8Vec);
    impl_vec_hash!(u8, AzU8Vec);

    impl_vec!(AzCallbackData, AzCallbackDataVec);
    impl_vec_partialord!(AzCallbackData, AzCallbackDataVec);
    impl_vec_ord!(AzCallbackData, AzCallbackDataVec);
    impl_vec_partialeq!(AzCallbackData, AzCallbackDataVec);
    impl_vec_eq!(AzCallbackData, AzCallbackDataVec);
    impl_vec_hash!(AzCallbackData, AzCallbackDataVec);

    impl_vec!(AzOverrideProperty, AzOverridePropertyVec);
    impl_vec_partialord!(AzOverrideProperty, AzOverridePropertyVec);
    impl_vec_ord!(AzOverrideProperty, AzOverridePropertyVec);
    impl_vec_partialeq!(AzOverrideProperty, AzOverridePropertyVec);
    impl_vec_eq!(AzOverrideProperty, AzOverridePropertyVec);
    impl_vec_hash!(AzOverrideProperty, AzOverridePropertyVec);

    impl_vec!(AzDom, AzDomVec);
    impl_vec_partialord!(AzDom, AzDomVec);
    impl_vec_ord!(AzDom, AzDomVec);
    impl_vec_partialeq!(AzDom, AzDomVec);
    impl_vec_eq!(AzDom, AzDomVec);
    impl_vec_hash!(AzDom, AzDomVec);

    impl_vec!(AzString, AzStringVec);
    impl_vec_partialord!(AzString, AzStringVec);
    impl_vec_ord!(AzString, AzStringVec);
    impl_vec_partialeq!(AzString, AzStringVec);
    impl_vec_eq!(AzString, AzStringVec);
    impl_vec_hash!(AzString, AzStringVec);

    impl_vec!(AzGradientStopPre, AzGradientStopPreVec);
    impl_vec_partialord!(AzGradientStopPre, AzGradientStopPreVec);
    impl_vec_ord!(AzGradientStopPre, AzGradientStopPreVec);
    impl_vec_partialeq!(AzGradientStopPre, AzGradientStopPreVec);
    impl_vec_eq!(AzGradientStopPre, AzGradientStopPreVec);
    impl_vec_hash!(AzGradientStopPre, AzGradientStopPreVec);

    impl_vec!(AzDebugMessage, AzDebugMessageVec);
    impl_vec_partialord!(AzDebugMessage, AzDebugMessageVec);
    impl_vec_ord!(AzDebugMessage, AzDebugMessageVec);
    impl_vec_partialeq!(AzDebugMessage, AzDebugMessageVec);
    impl_vec_eq!(AzDebugMessage, AzDebugMessageVec);
    impl_vec_hash!(AzDebugMessage, AzDebugMessageVec);

    impl_vec!(AzGLint, AzGLintVec);
    impl_vec_partialord!(AzGLint, AzGLintVec);
    impl_vec_ord!(AzGLint, AzGLintVec);
    impl_vec_partialeq!(AzGLint, AzGLintVec);
    impl_vec_eq!(AzGLint, AzGLintVec);
    impl_vec_hash!(AzGLint, AzGLintVec);

    impl_vec!(AzGLuint, AzGLuintVec);
    impl_vec_partialord!(AzGLuint, AzGLuintVec);
    impl_vec_ord!(AzGLuint, AzGLuintVec);
    impl_vec_partialeq!(AzGLuint, AzGLuintVec);
    impl_vec_eq!(AzGLuint, AzGLuintVec);
    impl_vec_hash!(AzGLuint, AzGLuintVec);

    impl From<std::vec::Vec<std::string::String>> for crate::vec::StringVec {
        fn from(v: std::vec::Vec<std::string::String>) -> crate::vec::StringVec {
            let mut vec: Vec<AzString> = v.into_iter().map(|i| {
                let i: std::vec::Vec<u8> = i.into_bytes();
                (crate::dll::get_azul_dll().az_string_from_utf8_unchecked)(i.as_ptr(), i.len())
            }).collect();

            (crate::dll::get_azul_dll().az_string_vec_copy_from)(vec.as_mut_ptr(), vec.len())
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
        pub fn copy_from(ptr: *mut AzXWindowType, len: usize) -> Self { (crate::dll::get_azul_dll().az_x_window_type_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for XWindowTypeVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_x_window_type_vec_fmt_debug)(self)) } }
    impl Clone for XWindowTypeVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_x_window_type_vec_deep_copy)(self) } }
    impl Drop for XWindowTypeVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_x_window_type_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `VirtualKeyCode`
    pub use crate::dll::AzVirtualKeyCodeVec as VirtualKeyCodeVec;

    impl VirtualKeyCodeVec {
        /// Creates + allocates a Rust `Vec<VirtualKeyCode>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut AzVirtualKeyCode, len: usize) -> Self { (crate::dll::get_azul_dll().az_virtual_key_code_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for VirtualKeyCodeVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_virtual_key_code_vec_fmt_debug)(self)) } }
    impl Clone for VirtualKeyCodeVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_virtual_key_code_vec_deep_copy)(self) } }
    impl Drop for VirtualKeyCodeVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_virtual_key_code_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `ScanCode`
    pub use crate::dll::AzScanCodeVec as ScanCodeVec;

    impl ScanCodeVec {
        /// Creates + allocates a Rust `Vec<ScanCode>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut u32, len: usize) -> Self { (crate::dll::get_azul_dll().az_scan_code_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for ScanCodeVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_scan_code_vec_fmt_debug)(self)) } }
    impl Clone for ScanCodeVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_scan_code_vec_deep_copy)(self) } }
    impl Drop for ScanCodeVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_scan_code_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CssDeclaration`
    pub use crate::dll::AzCssDeclarationVec as CssDeclarationVec;

    impl CssDeclarationVec {
        /// Creates + allocates a Rust `Vec<CssDeclaration>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut AzCssDeclaration, len: usize) -> Self { (crate::dll::get_azul_dll().az_css_declaration_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for CssDeclarationVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_declaration_vec_fmt_debug)(self)) } }
    impl Clone for CssDeclarationVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_declaration_vec_deep_copy)(self) } }
    impl Drop for CssDeclarationVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_declaration_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CssPathSelector`
    pub use crate::dll::AzCssPathSelectorVec as CssPathSelectorVec;

    impl CssPathSelectorVec {
        /// Creates + allocates a Rust `Vec<CssPathSelector>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut AzCssPathSelector, len: usize) -> Self { (crate::dll::get_azul_dll().az_css_path_selector_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for CssPathSelectorVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_path_selector_vec_fmt_debug)(self)) } }
    impl Clone for CssPathSelectorVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_path_selector_vec_deep_copy)(self) } }
    impl Drop for CssPathSelectorVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_path_selector_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `Stylesheet`
    pub use crate::dll::AzStylesheetVec as StylesheetVec;

    impl StylesheetVec {
        /// Creates + allocates a Rust `Vec<Stylesheet>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut AzStylesheet, len: usize) -> Self { (crate::dll::get_azul_dll().az_stylesheet_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for StylesheetVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_stylesheet_vec_fmt_debug)(self)) } }
    impl Clone for StylesheetVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_stylesheet_vec_deep_copy)(self) } }
    impl Drop for StylesheetVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_stylesheet_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CssRuleBlock`
    pub use crate::dll::AzCssRuleBlockVec as CssRuleBlockVec;

    impl CssRuleBlockVec {
        /// Creates + allocates a Rust `Vec<CssRuleBlock>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut AzCssRuleBlock, len: usize) -> Self { (crate::dll::get_azul_dll().az_css_rule_block_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for CssRuleBlockVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_rule_block_vec_fmt_debug)(self)) } }
    impl Clone for CssRuleBlockVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_rule_block_vec_deep_copy)(self) } }
    impl Drop for CssRuleBlockVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_rule_block_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `U8Vec`
    pub use crate::dll::AzU8Vec as U8Vec;

    impl U8Vec {
        /// Creates + allocates a Rust `Vec<u8>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut u8, len: usize) -> Self { (crate::dll::get_azul_dll().az_u8_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for U8Vec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_u8_vec_fmt_debug)(self)) } }
    impl Clone for U8Vec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_u8_vec_deep_copy)(self) } }
    impl Drop for U8Vec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_u8_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CallbackData`
    pub use crate::dll::AzCallbackDataVec as CallbackDataVec;

    impl CallbackDataVec {
        /// Creates + allocates a Rust `Vec<CallbackData>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut AzCallbackData, len: usize) -> Self { (crate::dll::get_azul_dll().az_callback_data_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for CallbackDataVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_callback_data_vec_fmt_debug)(self)) } }
    impl Clone for CallbackDataVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_callback_data_vec_deep_copy)(self) } }
    impl Drop for CallbackDataVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_callback_data_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `Vec<DebugMessage>`
    pub use crate::dll::AzDebugMessageVec as DebugMessageVec;

    impl DebugMessageVec {
        /// Creates + allocates a Rust `Vec<DebugMessage>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut AzDebugMessage, len: usize) -> Self { (crate::dll::get_azul_dll().az_debug_message_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for DebugMessageVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_debug_message_vec_fmt_debug)(self)) } }
    impl Clone for DebugMessageVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_debug_message_vec_deep_copy)(self) } }
    impl Drop for DebugMessageVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_debug_message_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `U32Vec`
    pub use crate::dll::AzGLuintVec as GLuintVec;

    impl GLuintVec {
        /// Creates + allocates a Rust `Vec<u32>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut u32, len: usize) -> Self { (crate::dll::get_azul_dll().az_g_luint_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for GLuintVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_g_luint_vec_fmt_debug)(self)) } }
    impl Clone for GLuintVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_g_luint_vec_deep_copy)(self) } }
    impl Drop for GLuintVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_luint_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `GLintVec`
    pub use crate::dll::AzGLintVec as GLintVec;

    impl GLintVec {
        /// Creates + allocates a Rust `Vec<u32>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut i32, len: usize) -> Self { (crate::dll::get_azul_dll().az_g_lint_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for GLintVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_g_lint_vec_fmt_debug)(self)) } }
    impl Clone for GLintVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_g_lint_vec_deep_copy)(self) } }
    impl Drop for GLintVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lint_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `OverridePropertyVec`
    pub use crate::dll::AzOverridePropertyVec as OverridePropertyVec;

    impl OverridePropertyVec {
        /// Creates + allocates a Rust `Vec<OverrideProperty>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut AzOverrideProperty, len: usize) -> Self { (crate::dll::get_azul_dll().az_override_property_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for OverridePropertyVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_override_property_vec_fmt_debug)(self)) } }
    impl Clone for OverridePropertyVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_override_property_vec_deep_copy)(self) } }
    impl Drop for OverridePropertyVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_override_property_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `DomVec`
    pub use crate::dll::AzDomVec as DomVec;

    impl DomVec {
        /// Creates + allocates a Rust `Vec<Dom>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut AzDom, len: usize) -> Self { (crate::dll::get_azul_dll().az_dom_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for DomVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_dom_vec_fmt_debug)(self)) } }
    impl Clone for DomVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_dom_vec_deep_copy)(self) } }
    impl Drop for DomVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_dom_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `StringVec`
    pub use crate::dll::AzStringVec as StringVec;

    impl StringVec {
        /// Creates + allocates a Rust `Vec<String>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut AzString, len: usize) -> Self { (crate::dll::get_azul_dll().az_string_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for StringVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_string_vec_fmt_debug)(self)) } }
    impl Clone for StringVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_string_vec_deep_copy)(self) } }
    impl Drop for StringVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `StringPairVec`
    pub use crate::dll::AzStringPairVec as StringPairVec;

    impl StringPairVec {
        /// Creates + allocates a Rust `Vec<StringPair>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut AzStringPair, len: usize) -> Self { (crate::dll::get_azul_dll().az_string_pair_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for StringPairVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_string_pair_vec_fmt_debug)(self)) } }
    impl Clone for StringPairVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_string_pair_vec_deep_copy)(self) } }
    impl Drop for StringPairVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_pair_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `GradientStopPreVec`
    pub use crate::dll::AzGradientStopPreVec as GradientStopPreVec;

    impl GradientStopPreVec {
        /// Creates + allocates a Rust `Vec<GradientStopPre>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *mut AzGradientStopPre, len: usize) -> Self { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for GradientStopPreVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_fmt_debug)(self)) } }
    impl Clone for GradientStopPreVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_deep_copy)(self) } }
    impl Drop for GradientStopPreVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_delete)(self); } }
