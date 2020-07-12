    #![allow(dead_code, unused_imports)]
    //! Definition of azuls internal `Vec<*>` wrappers
    use crate::dll::*;
    use std::ffi::c_void;
    use std::fmt;
    use crate::gl::{
        GLint as AzGLint,
        GLuint as AzGLuint,
    };

    macro_rules! impl_vec {($struct_type:ident, $struct_name:ident) => (

        impl $struct_name {

            #[inline]
            pub fn iter(&self) -> std::slice::Iter<$struct_type> {
                self.as_ref().iter()
            }

            #[inline]
            pub fn into_iter(self) -> std::vec::IntoIter<$struct_type> {
                let v1: Vec<$struct_type> = self.into();
                v1.into_iter()
            }

            #[inline]
            pub fn iter_mut(&mut self) -> std::slice::IterMut<$struct_type> {
                self.as_mut().iter_mut()
            }

            #[inline]
            pub fn ptr_as_usize(&self) -> usize {
                self.ptr as usize
            }

            #[inline]
            pub fn as_mut_ptr(&mut self) -> *mut $struct_type {
                self.ptr
            }

            #[inline]
            pub fn len(&self) -> usize {
                self.len
            }

            #[inline]
            pub fn capacity(&self) -> usize {
                self.cap
            }

            #[inline]
            pub fn is_empty(&self) -> bool {
                self.len == 0
            }

            pub fn get(&self, index: usize) -> Option<&$struct_type> {
                self.as_ref().get(index)
            }

            #[inline]
            pub unsafe fn get_unchecked(&self, index: usize) -> &$struct_type {
                self.as_ref().get_unchecked(index)
            }
        }

        impl Default for $struct_name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl AsRef<[$struct_type]> for $struct_name {
            fn as_ref(&self) -> &[$struct_type] {
                unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
            }
        }

        impl AsMut<[$struct_type]> for $struct_name {
            fn as_mut(&mut self) -> &mut [$struct_type] {
                unsafe { std::slice::from_raw_parts_mut (self.ptr, self.len) }
            }
        }

        impl std::iter::FromIterator<$struct_type> for $struct_name {
            fn from_iter<T>(iter: T) -> Self where T: IntoIterator<Item = $struct_type> {
                let v: Vec<$struct_type> = Vec::from_iter(iter);
                v.into()
            }
        }

        impl From<Vec<$struct_type>> for $struct_name {
            fn from(input: Vec<$struct_type>) -> $struct_name {
                let s: &[$struct_type] = input.as_ref();
                s.into()
            }
        }

        impl From<&[$struct_type]> for $struct_name {
            fn from(input: &[$struct_type]) -> $struct_name {
                Self::copy_from(input.as_ptr(), input.len())
            }
        }

        impl From<$struct_name> for Vec<$struct_type> {
            fn from(mut input: $struct_name) -> Vec<$struct_type> {
                unsafe { std::slice::from_raw_parts(input.as_mut_ptr(), input.len()) }.to_vec()
            }
        }

        // Drop, Debug + Clone already implemented by default
    )}

    macro_rules! impl_vec_partialord {($struct_type:ident, $struct_name:ident) => (
        impl PartialOrd for $struct_name {
            fn partial_cmp(&self, rhs: &Self) -> Option<std::cmp::Ordering> {
                self.as_ref().partial_cmp(rhs.as_ref())
            }
        }
    )}

    macro_rules! impl_vec_ord {($struct_type:ident, $struct_name:ident) => (
        impl Ord for $struct_name {
            fn cmp(&self, rhs: &Self) -> std::cmp::Ordering {
                self.as_ref().cmp(rhs.as_ref())
            }
        }
    )}

    macro_rules! impl_vec_partialeq {($struct_type:ident, $struct_name:ident) => (
        impl PartialEq for $struct_name {
            fn eq(&self, rhs: &Self) -> bool {
                self.as_ref().eq(rhs.as_ref())
            }
        }
    )}

    macro_rules! impl_vec_eq {($struct_type:ident, $struct_name:ident) => (
        impl Eq for $struct_name { }
    )}

    macro_rules! impl_vec_hash {($struct_type:ident, $struct_name:ident) => (
        impl std::hash::Hash for $struct_name {
            fn hash<H>(&self, state: &mut H) where H: std::hash::Hasher {
                self.as_ref().hash(state);
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

    impl_vec!(AzCssProperty, AzCssPropertyVec);
    impl_vec_partialord!(AzCssProperty, AzCssPropertyVec);
    impl_vec_ord!(AzCssProperty, AzCssPropertyVec);
    impl_vec_partialeq!(AzCssProperty, AzCssPropertyVec);
    impl_vec_eq!(AzCssProperty, AzCssPropertyVec);
    impl_vec_hash!(AzCssProperty, AzCssPropertyVec);

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
            let mut vec: Vec<AzString> = v.into_iter().map(Into::into).collect();
            (crate::dll::get_azul_dll().az_string_vec_copy_from)(vec.as_mut_ptr(), vec.len())
        }
    }

    impl From<crate::vec::StringVec> for std::vec::Vec<std::string::String> {
        fn from(v: crate::vec::StringVec) -> std::vec::Vec<std::string::String> {
            v
            .as_ref()
            .iter()
            .cloned()
            .map(Into::into)
            .collect()

            // delete() not necessary because StringVec is stack-allocated
        }
    }    use crate::css::{CssDeclaration, CssPathSelector, CssProperty, CssRuleBlock, GradientStopPre, Stylesheet};
    use crate::svg::{SvgMultiPolygon, SvgPath, SvgPathElement, SvgVertex};
    use crate::gl::{DebugMessage, VertexAttribute};
    use crate::window::{StringPair, VirtualKeyCode, XWindowType};
    use crate::dom::{CallbackData, Dom};
    use crate::str::String;


    /// Wrapper over a Rust-allocated `Vec<CssProperty>`
    pub use crate::dll::AzCssPropertyVec as CssPropertyVec;

    impl CssPropertyVec {
        /// Creates a new, empty Rust `Vec<CssProperty>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_css_property_vec_new)() }
        /// Creates a new, empty Rust `Vec<CssProperty>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_css_property_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<CssProperty>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzCssProperty, len: usize) -> Self { (crate::dll::get_azul_dll().az_css_property_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for CssPropertyVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_property_vec_fmt_debug)(self)) } }
    impl Clone for CssPropertyVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_property_vec_deep_copy)(self) } }
    impl Drop for CssPropertyVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_property_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `Vec<SvgMultiPolygon>`
    pub use crate::dll::AzSvgMultiPolygonVec as SvgMultiPolygonVec;

    impl SvgMultiPolygonVec {
        /// Creates a new, empty Rust `Vec<SvgMultiPolygon>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_svg_multi_polygon_vec_new)() }
        /// Creates a new, empty Rust `Vec<SvgMultiPolygon>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_svg_multi_polygon_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<SvgMultiPolygon>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzSvgMultiPolygon, len: usize) -> Self { (crate::dll::get_azul_dll().az_svg_multi_polygon_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for SvgMultiPolygonVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_multi_polygon_vec_fmt_debug)(self)) } }
    impl Clone for SvgMultiPolygonVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_multi_polygon_vec_deep_copy)(self) } }
    impl Drop for SvgMultiPolygonVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_multi_polygon_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `Vec<SvgPath>`
    pub use crate::dll::AzSvgPathVec as SvgPathVec;

    impl SvgPathVec {
        /// Creates a new, empty Rust `Vec<SvgPath>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_svg_path_vec_new)() }
        /// Creates a new, empty Rust `Vec<SvgPath>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_svg_path_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<SvgPath>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzSvgPath, len: usize) -> Self { (crate::dll::get_azul_dll().az_svg_path_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for SvgPathVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_path_vec_fmt_debug)(self)) } }
    impl Clone for SvgPathVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_path_vec_deep_copy)(self) } }
    impl Drop for SvgPathVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_path_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `Vec<VertexAttribute>`
    pub use crate::dll::AzVertexAttributeVec as VertexAttributeVec;

    impl VertexAttributeVec {
        /// Creates a new, empty Rust `Vec<VertexAttribute>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_vertex_attribute_vec_new)() }
        /// Creates a new, empty Rust `Vec<VertexAttribute>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_vertex_attribute_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<VertexAttribute>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzVertexAttribute, len: usize) -> Self { (crate::dll::get_azul_dll().az_vertex_attribute_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for VertexAttributeVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_vertex_attribute_vec_fmt_debug)(self)) } }
    impl Clone for VertexAttributeVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_vertex_attribute_vec_deep_copy)(self) } }
    impl Drop for VertexAttributeVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_vertex_attribute_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `VertexAttribute`
    pub use crate::dll::AzSvgPathElementVec as SvgPathElementVec;

    impl SvgPathElementVec {
        /// Creates a new, empty Rust `Vec<SvgPathElement>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_svg_path_element_vec_new)() }
        /// Creates a new, empty Rust `Vec<SvgPathElement>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_svg_path_element_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<SvgPathElement>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzSvgPathElement, len: usize) -> Self { (crate::dll::get_azul_dll().az_svg_path_element_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for SvgPathElementVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_path_element_vec_fmt_debug)(self)) } }
    impl Clone for SvgPathElementVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_path_element_vec_deep_copy)(self) } }
    impl Drop for SvgPathElementVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_path_element_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `SvgVertex`
    pub use crate::dll::AzSvgVertexVec as SvgVertexVec;

    impl SvgVertexVec {
        /// Creates a new, empty Rust `Vec<SvgVertex>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_svg_vertex_vec_new)() }
        /// Creates a new, empty Rust `Vec<SvgVertex>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_svg_vertex_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<SvgVertex>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzSvgVertex, len: usize) -> Self { (crate::dll::get_azul_dll().az_svg_vertex_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for SvgVertexVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_vertex_vec_fmt_debug)(self)) } }
    impl Clone for SvgVertexVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_vertex_vec_deep_copy)(self) } }
    impl Drop for SvgVertexVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_vertex_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `Vec<u32>`
    pub use crate::dll::AzU32Vec as U32Vec;

    impl U32Vec {
        /// Creates a new, empty Rust `Vec<u32>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_u32_vec_new)() }
        /// Creates a new, empty Rust `Vec<u32>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_u32_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<u32>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u32, len: usize) -> Self { (crate::dll::get_azul_dll().az_u32_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for U32Vec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_u32_vec_fmt_debug)(self)) } }
    impl Clone for U32Vec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_u32_vec_deep_copy)(self) } }
    impl Drop for U32Vec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_u32_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `XWindowType`
    pub use crate::dll::AzXWindowTypeVec as XWindowTypeVec;

    impl XWindowTypeVec {
        /// Creates a new, empty Rust `Vec<XWindowType>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_x_window_type_vec_new)() }
        /// Creates a new, empty Rust `Vec<XWindowType>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_x_window_type_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<XWindowType>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzXWindowType, len: usize) -> Self { (crate::dll::get_azul_dll().az_x_window_type_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for XWindowTypeVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_x_window_type_vec_fmt_debug)(self)) } }
    impl Clone for XWindowTypeVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_x_window_type_vec_deep_copy)(self) } }
    impl Drop for XWindowTypeVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_x_window_type_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `VirtualKeyCode`
    pub use crate::dll::AzVirtualKeyCodeVec as VirtualKeyCodeVec;

    impl VirtualKeyCodeVec {
        /// Creates a new, empty Rust `Vec<VirtualKeyCode>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_virtual_key_code_vec_new)() }
        /// Creates a new, empty Rust `Vec<VirtualKeyCode>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_virtual_key_code_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<VirtualKeyCode>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzVirtualKeyCode, len: usize) -> Self { (crate::dll::get_azul_dll().az_virtual_key_code_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for VirtualKeyCodeVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_virtual_key_code_vec_fmt_debug)(self)) } }
    impl Clone for VirtualKeyCodeVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_virtual_key_code_vec_deep_copy)(self) } }
    impl Drop for VirtualKeyCodeVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_virtual_key_code_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `ScanCode`
    pub use crate::dll::AzScanCodeVec as ScanCodeVec;

    impl ScanCodeVec {
        /// Creates a new, empty Rust `Vec<ScanCode>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_scan_code_vec_new)() }
        /// Creates a new, empty Rust `Vec<ScanCode>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_scan_code_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<ScanCode>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u32, len: usize) -> Self { (crate::dll::get_azul_dll().az_scan_code_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for ScanCodeVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_scan_code_vec_fmt_debug)(self)) } }
    impl Clone for ScanCodeVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_scan_code_vec_deep_copy)(self) } }
    impl Drop for ScanCodeVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_scan_code_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CssDeclaration`
    pub use crate::dll::AzCssDeclarationVec as CssDeclarationVec;

    impl CssDeclarationVec {
        /// Creates a new, empty Rust `Vec<CssDeclaration>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_css_declaration_vec_new)() }
        /// Creates a new, empty Rust `Vec<CssDeclaration>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_css_declaration_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<CssDeclaration>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzCssDeclaration, len: usize) -> Self { (crate::dll::get_azul_dll().az_css_declaration_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for CssDeclarationVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_declaration_vec_fmt_debug)(self)) } }
    impl Clone for CssDeclarationVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_declaration_vec_deep_copy)(self) } }
    impl Drop for CssDeclarationVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_declaration_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CssPathSelector`
    pub use crate::dll::AzCssPathSelectorVec as CssPathSelectorVec;

    impl CssPathSelectorVec {
        /// Creates a new, empty Rust `Vec<CssPathSelector>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_css_path_selector_vec_new)() }
        /// Creates a new, empty Rust `Vec<CssPathSelector>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_css_path_selector_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<CssPathSelector>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzCssPathSelector, len: usize) -> Self { (crate::dll::get_azul_dll().az_css_path_selector_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for CssPathSelectorVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_path_selector_vec_fmt_debug)(self)) } }
    impl Clone for CssPathSelectorVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_path_selector_vec_deep_copy)(self) } }
    impl Drop for CssPathSelectorVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_path_selector_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `Stylesheet`
    pub use crate::dll::AzStylesheetVec as StylesheetVec;

    impl StylesheetVec {
        /// Creates a new, empty Rust `Vec<Stylesheet>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_stylesheet_vec_new)() }
        /// Creates a new, empty Rust `Vec<Stylesheet>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_stylesheet_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<Stylesheet>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzStylesheet, len: usize) -> Self { (crate::dll::get_azul_dll().az_stylesheet_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for StylesheetVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_stylesheet_vec_fmt_debug)(self)) } }
    impl Clone for StylesheetVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_stylesheet_vec_deep_copy)(self) } }
    impl Drop for StylesheetVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_stylesheet_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CssRuleBlock`
    pub use crate::dll::AzCssRuleBlockVec as CssRuleBlockVec;

    impl CssRuleBlockVec {
        /// Creates a new, empty Rust `Vec<CssRuleBlock>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_css_rule_block_vec_new)() }
        /// Creates a new, empty Rust `Vec<CssRuleBlock>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_css_rule_block_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<CssRuleBlock>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzCssRuleBlock, len: usize) -> Self { (crate::dll::get_azul_dll().az_css_rule_block_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for CssRuleBlockVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_rule_block_vec_fmt_debug)(self)) } }
    impl Clone for CssRuleBlockVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_rule_block_vec_deep_copy)(self) } }
    impl Drop for CssRuleBlockVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_rule_block_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `U8Vec`
    pub use crate::dll::AzU8Vec as U8Vec;

    impl U8Vec {
        /// Creates a new, empty Rust `Vec<u8>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_u8_vec_new)() }
        /// Creates a new, empty Rust `Vec<u8>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_u8_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<u8>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u8, len: usize) -> Self { (crate::dll::get_azul_dll().az_u8_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for U8Vec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_u8_vec_fmt_debug)(self)) } }
    impl Clone for U8Vec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_u8_vec_deep_copy)(self) } }
    impl Drop for U8Vec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_u8_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CallbackData`
    pub use crate::dll::AzCallbackDataVec as CallbackDataVec;

    impl CallbackDataVec {
        /// Creates a new, empty Rust `Vec<CallbackData>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_callback_data_vec_new)() }
        /// Creates a new, empty Rust `Vec<CallbackData>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_callback_data_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<CallbackData>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzCallbackData, len: usize) -> Self { (crate::dll::get_azul_dll().az_callback_data_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for CallbackDataVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_callback_data_vec_fmt_debug)(self)) } }
    impl Clone for CallbackDataVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_callback_data_vec_deep_copy)(self) } }
    impl Drop for CallbackDataVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_callback_data_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `Vec<DebugMessage>`
    pub use crate::dll::AzDebugMessageVec as DebugMessageVec;

    impl DebugMessageVec {
        /// Creates a new, empty Rust `Vec<DebugMessage>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_debug_message_vec_new)() }
        /// Creates a new, empty Rust `Vec<DebugMessage>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_debug_message_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<DebugMessage>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzDebugMessage, len: usize) -> Self { (crate::dll::get_azul_dll().az_debug_message_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for DebugMessageVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_debug_message_vec_fmt_debug)(self)) } }
    impl Clone for DebugMessageVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_debug_message_vec_deep_copy)(self) } }
    impl Drop for DebugMessageVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_debug_message_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `U32Vec`
    pub use crate::dll::AzGLuintVec as GLuintVec;

    impl GLuintVec {
        /// Creates a new, empty Rust `Vec<u32>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_g_luint_vec_new)() }
        /// Creates a new, empty Rust `Vec<u32>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_g_luint_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<u32>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u32, len: usize) -> Self { (crate::dll::get_azul_dll().az_g_luint_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for GLuintVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_g_luint_vec_fmt_debug)(self)) } }
    impl Clone for GLuintVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_g_luint_vec_deep_copy)(self) } }
    impl Drop for GLuintVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_luint_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `GLintVec`
    pub use crate::dll::AzGLintVec as GLintVec;

    impl GLintVec {
        /// Creates a new, empty Rust `Vec<GLint>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_g_lint_vec_new)() }
        /// Creates a new, empty Rust `Vec<GLint>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_g_lint_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<GLint>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const i32, len: usize) -> Self { (crate::dll::get_azul_dll().az_g_lint_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for GLintVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_g_lint_vec_fmt_debug)(self)) } }
    impl Clone for GLintVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_g_lint_vec_deep_copy)(self) } }
    impl Drop for GLintVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lint_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `DomVec`
    pub use crate::dll::AzDomVec as DomVec;

    impl DomVec {
        /// Creates a new, empty Rust `Vec<Dom>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_dom_vec_new)() }
        /// Creates a new, empty Rust `Vec<Dom>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_dom_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<Dom>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzDom, len: usize) -> Self { (crate::dll::get_azul_dll().az_dom_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for DomVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_dom_vec_fmt_debug)(self)) } }
    impl Clone for DomVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_dom_vec_deep_copy)(self) } }
    impl Drop for DomVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_dom_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `StringVec`
    pub use crate::dll::AzStringVec as StringVec;

    impl StringVec {
        /// Creates a new, empty Rust `Vec<String>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_string_vec_new)() }
        /// Creates a new, empty Rust `Vec<String>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_string_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<String>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzString, len: usize) -> Self { (crate::dll::get_azul_dll().az_string_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for StringVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_string_vec_fmt_debug)(self)) } }
    impl Clone for StringVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_string_vec_deep_copy)(self) } }
    impl Drop for StringVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `StringPairVec`
    pub use crate::dll::AzStringPairVec as StringPairVec;

    impl StringPairVec {
        /// Creates a new, empty Rust `Vec<StringPair>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_string_pair_vec_new)() }
        /// Creates a new, empty Rust `Vec<StringPair>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_string_pair_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<StringPair>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzStringPair, len: usize) -> Self { (crate::dll::get_azul_dll().az_string_pair_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for StringPairVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_string_pair_vec_fmt_debug)(self)) } }
    impl Clone for StringPairVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_string_pair_vec_deep_copy)(self) } }
    impl Drop for StringPairVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_pair_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `GradientStopPreVec`
    pub use crate::dll::AzGradientStopPreVec as GradientStopPreVec;

    impl GradientStopPreVec {
        /// Creates a new, empty Rust `Vec<GradientStopPre>`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_new)() }
        /// Creates a new, empty Rust `Vec<GradientStopPre>` with a given, pre-allocated capacity
        pub fn with_capacity(cap: usize) -> Self { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_with_capacity)(cap) }
        /// Creates + allocates a Rust `Vec<GradientStopPre>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzGradientStopPre, len: usize) -> Self { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_copy_from)(ptr, len) }
    }

    impl std::fmt::Debug for GradientStopPreVec { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_fmt_debug)(self)) } }
    impl Clone for GradientStopPreVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_deep_copy)(self) } }
    impl Drop for GradientStopPreVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_delete)(self); } }
