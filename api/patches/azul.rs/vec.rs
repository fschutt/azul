    use core::iter;
    use core::fmt;

    use alloc::vec::{self, Vec};
    use alloc::slice;
    use alloc::string;

    use crate::gl::{
        GLint as AzGLint,
        GLuint as AzGLuint,
    };

    macro_rules! impl_vec {($struct_type:ident, $struct_name:ident) => (

        impl $struct_name {

            #[inline]
            pub fn iter(&self) -> slice::Iter<$struct_type> {
                self.as_ref().iter()
            }

            #[inline]
            pub fn into_iter(self) -> vec::IntoIter<$struct_type> {
                let v1: Vec<$struct_type> = self.into();
                v1.into_iter()
            }

            #[inline]
            pub fn iter_mut(&mut self) -> slice::IterMut<$struct_type> {
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
                unsafe { slice::from_raw_parts(self.ptr, self.len) }
            }
        }

        impl AsMut<[$struct_type]> for $struct_name {
            fn as_mut(&mut self) -> &mut [$struct_type] {
                unsafe { slice::from_raw_parts_mut (self.ptr, self.len) }
            }
        }

        impl iter::FromIterator<$struct_type> for $struct_name {
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
                unsafe { slice::from_raw_parts(input.as_mut_ptr(), input.len()) }.to_vec()
            }
        }

        // Drop, Debug + Clone already implemented by default
    )}

    impl_vec!(u8,  AzU8Vec);
    impl_vec!(u32, AzU32Vec);
    impl_vec!(u32, AzScanCodeVec);
    impl_vec!(u32, AzGLuintVec);
    impl_vec!(i32, AzGLintVec);
    impl_vec!(AzStyleTransform, AzStyleTransformVec);
    impl_vec!(AzContentGroup, AzContentGroupVec);
    impl_vec!(AzCssProperty, AzCssPropertyVec);
    impl_vec!(AzSvgMultiPolygon, AzSvgMultiPolygonVec);
    impl_vec!(AzSvgPath, AzSvgPathVec);
    impl_vec!(AzVertexAttribute, AzVertexAttributeVec);
    impl_vec!(AzSvgPathElement, AzSvgPathElementVec);
    impl_vec!(AzSvgVertex, AzSvgVertexVec);
    impl_vec!(AzXWindowType, AzXWindowTypeVec);
    impl_vec!(AzVirtualKeyCode, AzVirtualKeyCodeVec);
    impl_vec!(AzCascadeInfo, AzCascadeInfoVec);
    impl_vec!(AzCssDeclaration, AzCssDeclarationVec);
    impl_vec!(AzCssPathSelector, AzCssPathSelectorVec);
    impl_vec!(AzStylesheet, AzStylesheetVec);
    impl_vec!(AzCssRuleBlock, AzCssRuleBlockVec);
    impl_vec!(AzCallbackData, AzCallbackDataVec);
    impl_vec!(AzDebugMessage, AzDebugMessageVec);
    impl_vec!(AzDom, AzDomVec);
    impl_vec!(AzString, AzStringVec);
    impl_vec!(AzStringPair, AzStringPairVec);
    impl_vec!(AzLinearColorStop, AzLinearColorStopVec);
    impl_vec!(AzRadialColorStop, AzRadialColorStopVec);
    impl_vec!(AzCascadedCssPropertyWithSource, AzCascadedCssPropertyWithSourceVec);
    impl_vec!(AzNodeId, AzNodeIdVec);
    impl_vec!(AzNode, AzNodeVec);
    impl_vec!(AzStyledNode, AzStyledNodeVec);
    impl_vec!(AzTagIdToNodeIdMapping, AzTagIdsToNodeIdsMappingVec);
    impl_vec!(AzParentWithNodeDepth, AzParentWithNodeDepthVec);
    impl_vec!(AzNodeData, AzNodeDataVec);

    impl From<vec::Vec<string::String>> for crate::vec::StringVec {
        fn from(v: vec::Vec<string::String>) -> crate::vec::StringVec {
            let mut vec: Vec<AzString> = v.into_iter().map(Into::into).collect();
            unsafe { crate::dll::az_string_vec_copy_from(vec.as_mut_ptr(), vec.len()) }
        }
    }

    impl From<crate::vec::StringVec> for vec::Vec<string::String> {
        fn from(v: crate::vec::StringVec) -> vec::Vec<string::String> {
            v
            .as_ref()
            .iter()
            .cloned()
            .map(Into::into)
            .collect()

            // delete() not necessary because StringVec is stack-allocated
        }
    }