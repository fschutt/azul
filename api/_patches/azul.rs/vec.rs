    use core::iter;
    use core::fmt;

    use alloc::vec::{self, Vec};
    use alloc::slice;
    use alloc::string;

    use crate::gl::{
        GLint as AzGLint,
        GLuint as AzGLuint,
    };

    macro_rules! impl_vec {($struct_type:ident, $struct_name:ident, $destructor_name:ident, $c_destructor_fn_name:ident) => (

        unsafe impl Send for $struct_name { }

        impl fmt::Debug for $destructor_name {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                match self {
                    $destructor_name::DefaultRust => write!(f, "DefaultRust"),
                    $destructor_name::NoDestructor => write!(f, "NoDestructor"),
                    $destructor_name::External(_) => write!(f, "External"),
                }
            }
        }

        impl $struct_name {

            #[inline]
            pub fn iter(&self) -> slice::Iter<$struct_type> {
                self.as_ref().iter()
            }

            #[inline]
            pub fn ptr_as_usize(&self) -> usize {
                self.ptr as usize
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
                let v1: &[$struct_type] = self.as_ref();
                let res = v1.get(index);
                res
            }

            #[inline]
            unsafe fn get_unchecked(&self, index: usize) -> &$struct_type {
                let v1: &[$struct_type] = self.as_ref();
                let res = v1.get_unchecked(index);
                res
            }

            pub fn as_slice(&self) -> &[$struct_type] {
                self.as_ref()
            }

            #[inline(always)]
            pub const fn from_const_slice(input: &'static [$struct_type]) -> Self {
                Self {
                    ptr: input.as_ptr(),
                    len: input.len(),
                    cap: input.len(),
                    destructor: $destructor_name::NoDestructor, // because of &'static
                }
            }
        }

        impl AsRef<[$struct_type]> for $struct_name {
            fn as_ref(&self) -> &[$struct_type] {
                unsafe { slice::from_raw_parts(self.ptr, self.len) }
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

                extern "C" fn $c_destructor_fn_name(s: &mut $struct_name) {
                    let _ = unsafe { Vec::from_raw_parts(s.ptr as *mut $struct_type, s.len, s.cap) };
                }

                Self {
                    ptr: input.as_ptr(),
                    len: input.len(),
                    cap: input.len(),
                    destructor: $destructor_name::External($c_destructor_fn_name),
                }
            }
        }

        impl From<&'static [$struct_type]> for $struct_name {
            fn from(input: &'static [$struct_type]) -> $struct_name {
                Self::from_const_slice(input)
            }
        }

        impl From<$struct_name> for Vec<$struct_type> {
            fn from(input: $struct_name) -> Vec<$struct_type> {
                input.as_ref().to_vec()
            }
        }

        impl Drop for $struct_name {
            fn drop(&mut self) {
                match self.destructor {
                    $destructor_name::DefaultRust => { /* no destructor because this is library code */ },
                    $destructor_name::NoDestructor => { },
                    $destructor_name::External(f) => { f(self); }
                }
            }
        }
    )}

    impl_vec!(u8,  AzU8Vec,  AzU8VecDestructor, u8_vec_destructor);
    impl_vec!(u32, AzU32Vec, AzU32VecDestructor, u32_vec_destructor);
    impl_vec!(u32, AzScanCodeVec, AzScanCodeVecDestructor, u32_vec_destructor);
    impl_vec!(u32, AzGLuintVec, AzGLuintVecDestructor, u32_vec_destructor);
    impl_vec!(i32, AzGLintVec, AzGLintVecDestructor, i32_vec_destructor);
    impl_vec!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec, NodeDataInlineCssPropertyVecDestructor, node_data_inline_css_property_vec_destructor);
    impl_vec!(IdOrClass, IdOrClassVec, IdOrClassVecDestructor, id_or_class_vec_destructor);
    impl_vec!(AzStyleTransform, AzStyleTransformVec, AzStyleTransformVecDestructor, az_style_transform_vec_destructor);
    impl_vec!(AzCssProperty, AzCssPropertyVec, AzCssPropertyVecDestructor, az_css_property_vec_destructor);
    impl_vec!(AzSvgMultiPolygon, AzSvgMultiPolygonVec, AzSvgMultiPolygonVecDestructor, az_svg_multi_polygon_vec_destructor);
    impl_vec!(AzSvgPath, AzSvgPathVec, AzSvgPathVecDestructor, az_svg_path_vec_destructor);
    impl_vec!(AzVertexAttribute, AzVertexAttributeVec, AzVertexAttributeVecDestructor, az_vertex_attribute_vec_destructor);
    impl_vec!(AzSvgPathElement, AzSvgPathElementVec, AzSvgPathElementVecDestructor, az_svg_path_element_vec_destructor);
    impl_vec!(AzSvgVertex, AzSvgVertexVec, AzSvgVertexVecDestructor, az_svg_vertex_vec_destructor);
    impl_vec!(AzXWindowType, AzXWindowTypeVec, AzXWindowTypeVecDestructor, az_x_window_type_vec_destructor);
    impl_vec!(AzVirtualKeyCode, AzVirtualKeyCodeVec, AzVirtualKeyCodeVecDestructor, az_virtual_key_code_vec_destructor);
    impl_vec!(AzCascadeInfo, AzCascadeInfoVec, AzCascadeInfoVecDestructor, az_cascade_info_vec_destructor);
    impl_vec!(AzCssDeclaration, AzCssDeclarationVec, AzCssDeclarationVecDestructor, az_css_declaration_vec_destructor);
    impl_vec!(AzCssPathSelector, AzCssPathSelectorVec, AzCssPathSelectorVecDestructor, az_css_path_selector_vec_destructor);
    impl_vec!(AzStylesheet, AzStylesheetVec, AzStylesheetVecDestructor, az_stylesheet_vec_destructor);
    impl_vec!(AzCssRuleBlock, AzCssRuleBlockVec, AzCssRuleBlockVecDestructor, az_css_rule_block_vec_destructor);
    impl_vec!(AzCallbackData, AzCallbackDataVec, AzCallbackDataVecDestructor, az_callback_data_vec_destructor);
    impl_vec!(AzDebugMessage, AzDebugMessageVec, AzDebugMessageVecDestructor, az_debug_message_vec_destructor);
    impl_vec!(AzDom, AzDomVec, AzDomVecDestructor, az_dom_vec_destructor);
    impl_vec!(AzString, AzStringVec, AzStringVecDestructor, az_string_vec_destructor);
    impl_vec!(AzStringPair, AzStringPairVec, AzStringPairVecDestructor, az_string_pair_vec_destructor);
    impl_vec!(AzLinearColorStop, AzLinearColorStopVec, AzLinearColorStopVecDestructor, az_linear_color_stop_vec_destructor);
    impl_vec!(AzRadialColorStop, AzRadialColorStopVec, AzRadialColorStopVecDestructor, az_radial_color_stop_vec_destructor);
    impl_vec!(AzNodeId, AzNodeIdVec, AzNodeIdVecDestructor, az_node_id_vec_destructor);
    impl_vec!(AzNode, AzNodeVec, AzNodeVecDestructor, az_node_vec_destructor);
    impl_vec!(AzStyledNode, AzStyledNodeVec, AzStyledNodeVecDestructor, az_styled_node_vec_destructor);
    impl_vec!(AzTagIdToNodeIdMapping, AzTagIdsToNodeIdsMappingVec, AzTagIdsToNodeIdsMappingVecDestructor, az_tag_id_to_node_id_mapping_vec_destructor);
    impl_vec!(AzParentWithNodeDepth, AzParentWithNodeDepthVec, AzParentWithNodeDepthVecDestructor, az_parent_with_node_depth_vec_destructor);
    impl_vec!(AzNodeData, AzNodeDataVec, AzNodeDataVecDestructor, az_node_data_vec_destructor);
    impl_vec!(AzStyleBackgroundRepeat, AzStyleBackgroundRepeatVec, AzStyleBackgroundRepeatVecDestructor, az_style_background_repeat_vec_destructor);
    impl_vec!(AzStyleBackgroundPosition, AzStyleBackgroundPositionVec, AzStyleBackgroundPositionVecDestructor, az_style_background_position_vec_destructor);
    impl_vec!(AzStyleBackgroundSize, AzStyleBackgroundSizeVec, AzStyleBackgroundSizeVecDestructor, az_style_background_size_vec_destructor);
    impl_vec!(AzStyleBackgroundContent, AzStyleBackgroundContentVec, AzStyleBackgroundContentVecDestructor, az_style_background_content_vec_destructor);

    impl From<vec::Vec<string::String>> for crate::vec::StringVec {
        fn from(v: vec::Vec<string::String>) -> crate::vec::StringVec {
            let vec: Vec<AzString> = v.into_iter().map(Into::into).collect();
            vec.into()
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