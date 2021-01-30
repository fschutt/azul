    use core::iter;
    use core::fmt;
    use core::cmp;

    use alloc::vec::{self, Vec};
    use alloc::slice;
    use alloc::string;

    use crate::gl::{
        GLint as AzGLint,
        GLuint as AzGLuint,
    };

    macro_rules! impl_vec {($struct_type:ident, $struct_name:ident, $destructor_name:ident, $c_destructor_fn_name:ident, $crate_dll_delete_fn:ident) => (

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

        impl PartialEq for $destructor_name {
            fn eq(&self, rhs: &Self) -> bool {
                match (self, rhs) {
                    ($destructor_name::DefaultRust, $destructor_name::DefaultRust) => true,
                    ($destructor_name::NoDestructor, $destructor_name::NoDestructor) => true,
                    ($destructor_name::External(a), $destructor_name::External(b)) => (a as *const _ as usize).eq(&(b as *const _ as usize)),
                    _ => false,
                }
            }
        }

        impl PartialOrd for $destructor_name {
            fn partial_cmp(&self, _rhs: &Self) -> Option<cmp::Ordering> {
                None
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

            #[inline(always)]
            pub fn from_vec(input: Vec<$struct_type>) -> Self {

                extern "C" fn $c_destructor_fn_name(s: &mut $struct_name) {
                    let _ = unsafe { Vec::from_raw_parts(s.ptr as *mut $struct_type, s.len, s.cap) };
                }

                let ptr = input.as_ptr();
                let len = input.len();
                let cap = input.capacity();

                let _ = ::core::mem::ManuallyDrop::new(input);

                Self {
                    ptr,
                    len,
                    cap,
                    destructor: $destructor_name::External($c_destructor_fn_name),
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
                Self::from_vec(Vec::from_iter(iter))
            }
        }

        impl From<Vec<$struct_type>> for $struct_name {
            fn from(input: Vec<$struct_type>) -> $struct_name {
                Self::from_vec(input)
            }
        }

        impl From<&'static [$struct_type]> for $struct_name {
            fn from(input: &'static [$struct_type]) -> $struct_name {
                Self::from_const_slice(input)
            }
        }

        impl Drop for $struct_name {
            fn drop(&mut self) {
                match self.destructor {
                    $destructor_name::DefaultRust => { unsafe { crate::dll::$crate_dll_delete_fn(self); } },
                    $destructor_name::NoDestructor => { },
                    $destructor_name::External(f) => { f(self); }
                }
            }
        }

        impl fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                self.as_ref().fmt(f)
            }
        }

        impl PartialOrd for $struct_name {
            fn partial_cmp(&self, rhs: &Self) -> Option<cmp::Ordering> {
                self.as_ref().partial_cmp(rhs.as_ref())
            }
        }

        impl PartialEq for $struct_name {
            fn eq(&self, rhs: &Self) -> bool {
                self.as_ref().eq(rhs.as_ref())
            }
        }
    )}

    #[macro_export]
    macro_rules! impl_vec_clone {($struct_type:ident, $struct_name:ident, $destructor_name:ident) => (
        impl $struct_name {
            /// NOTE: CLONES the memory if the memory is external or &'static
            /// Moves the memory out if the memory is library-allocated
            #[inline(always)]
            pub fn clone_self(&self) -> Self {
                match self.destructor {
                    $destructor_name::NoDestructor => {
                        Self {
                            ptr: self.ptr,
                            len: self.len,
                            cap: self.cap,
                            destructor: $destructor_name::NoDestructor,
                        }
                    }
                    $destructor_name::External(_) | $destructor_name::DefaultRust => {
                        Self::from_vec(self.as_ref().to_vec())
                    }
                }
            }
        }

        impl Clone for $struct_name {
            fn clone(&self) -> Self {
                self.clone_self()
            }
        }
    )}

    impl_vec!(u8,  AzU8Vec,  AzU8VecDestructor, az_u8_vec_destructor, az_u8_vec_delete);
    impl_vec_clone!(u8,  AzU8Vec,  AzU8VecDestructor);
    impl_vec!(u32, AzU32Vec, AzU32VecDestructor, az_u32_vec_destructor, az_u32_vec_delete);
    impl_vec_clone!(u32, AzU32Vec, AzU32VecDestructor);
    impl_vec!(u32, AzScanCodeVec, AzScanCodeVecDestructor, az_scan_code_vec_destructor, az_scan_code_vec_delete);
    impl_vec_clone!(u32, AzScanCodeVec, AzScanCodeVecDestructor);
    impl_vec!(u32, AzGLuintVec, AzGLuintVecDestructor, az_g_luint_vec_destructor, az_g_luint_vec_delete);
    impl_vec_clone!(u32, AzGLuintVec, AzGLuintVecDestructor);
    impl_vec!(i32, AzGLintVec, AzGLintVecDestructor, az_g_lint_vec_destructor, az_g_lint_vec_delete);
    impl_vec_clone!(i32, AzGLintVec, AzGLintVecDestructor);
    impl_vec!(AzNodeDataInlineCssProperty, AzNodeDataInlineCssPropertyVec, NodeDataInlineCssPropertyVecDestructor, az_node_data_inline_css_property_vec_destructor, az_node_data_inline_css_property_vec_delete);
    impl_vec_clone!(AzNodeDataInlineCssProperty, AzNodeDataInlineCssPropertyVec, NodeDataInlineCssPropertyVecDestructor);
    impl_vec!(AzIdOrClass, AzIdOrClassVec, IdOrClassVecDestructor, az_id_or_class_vec_destructor, az_id_or_class_vec_delete);
    impl_vec_clone!(AzIdOrClass, AzIdOrClassVec, IdOrClassVecDestructor);
    impl_vec!(AzStyleTransform, AzStyleTransformVec, AzStyleTransformVecDestructor, az_style_transform_vec_destructor, az_style_transform_vec_delete);
    impl_vec_clone!(AzStyleTransform, AzStyleTransformVec, AzStyleTransformVecDestructor);
    impl_vec!(AzCssProperty, AzCssPropertyVec, AzCssPropertyVecDestructor, az_css_property_vec_destructor, az_css_property_vec_delete);
    impl_vec_clone!(AzCssProperty, AzCssPropertyVec, AzCssPropertyVecDestructor);
    impl_vec!(AzSvgMultiPolygon, AzSvgMultiPolygonVec, AzSvgMultiPolygonVecDestructor, az_svg_multi_polygon_vec_destructor, az_svg_multi_polygon_vec_delete);
    impl_vec_clone!(AzSvgMultiPolygon, AzSvgMultiPolygonVec, AzSvgMultiPolygonVecDestructor);
    impl_vec!(AzSvgPath, AzSvgPathVec, AzSvgPathVecDestructor, az_svg_path_vec_destructor, az_svg_path_vec_delete);
    impl_vec_clone!(AzSvgPath, AzSvgPathVec, AzSvgPathVecDestructor);
    impl_vec!(AzVertexAttribute, AzVertexAttributeVec, AzVertexAttributeVecDestructor, az_vertex_attribute_vec_destructor, az_vertex_attribute_vec_delete);
    impl_vec_clone!(AzVertexAttribute, AzVertexAttributeVec, AzVertexAttributeVecDestructor);
    impl_vec!(AzSvgPathElement, AzSvgPathElementVec, AzSvgPathElementVecDestructor, az_svg_path_element_vec_destructor, az_svg_path_element_vec_delete);
    impl_vec_clone!(AzSvgPathElement, AzSvgPathElementVec, AzSvgPathElementVecDestructor);
    impl_vec!(AzSvgVertex, AzSvgVertexVec, AzSvgVertexVecDestructor, az_svg_vertex_vec_destructor, az_svg_vertex_vec_delete);
    impl_vec_clone!(AzSvgVertex, AzSvgVertexVec, AzSvgVertexVecDestructor);
    impl_vec!(AzXWindowType, AzXWindowTypeVec, AzXWindowTypeVecDestructor, az_x_window_type_vec_destructor, az_x_window_type_vec_delete);
    impl_vec_clone!(AzXWindowType, AzXWindowTypeVec, AzXWindowTypeVecDestructor);
    impl_vec!(AzVirtualKeyCode, AzVirtualKeyCodeVec, AzVirtualKeyCodeVecDestructor, az_virtual_key_code_vec_destructor, az_virtual_key_code_vec_delete);
    impl_vec_clone!(AzVirtualKeyCode, AzVirtualKeyCodeVec, AzVirtualKeyCodeVecDestructor);
    impl_vec!(AzCascadeInfo, AzCascadeInfoVec, AzCascadeInfoVecDestructor, az_cascade_info_vec_destructor, az_cascade_info_vec_delete);
    impl_vec_clone!(AzCascadeInfo, AzCascadeInfoVec, AzCascadeInfoVecDestructor);
    impl_vec!(AzCssDeclaration, AzCssDeclarationVec, AzCssDeclarationVecDestructor, az_css_declaration_vec_destructor, az_css_declaration_vec_delete);
    impl_vec_clone!(AzCssDeclaration, AzCssDeclarationVec, AzCssDeclarationVecDestructor);
    impl_vec!(AzCssPathSelector, AzCssPathSelectorVec, AzCssPathSelectorVecDestructor, az_css_path_selector_vec_destructor, az_css_path_selector_vec_delete);
    impl_vec_clone!(AzCssPathSelector, AzCssPathSelectorVec, AzCssPathSelectorVecDestructor);
    impl_vec!(AzStylesheet, AzStylesheetVec, AzStylesheetVecDestructor, az_stylesheet_vec_destructor, az_stylesheet_vec_delete);
    impl_vec_clone!(AzStylesheet, AzStylesheetVec, AzStylesheetVecDestructor);
    impl_vec!(AzCssRuleBlock, AzCssRuleBlockVec, AzCssRuleBlockVecDestructor, az_css_rule_block_vec_destructor, az_css_rule_block_vec_delete);
    impl_vec_clone!(AzCssRuleBlock, AzCssRuleBlockVec, AzCssRuleBlockVecDestructor);
    impl_vec!(AzCallbackData, AzCallbackDataVec, AzCallbackDataVecDestructor, az_callback_data_vec_destructor, az_callback_data_vec_delete);
    impl_vec_clone!(AzCallbackData, AzCallbackDataVec, AzCallbackDataVecDestructor);
    impl_vec!(AzDebugMessage, AzDebugMessageVec, AzDebugMessageVecDestructor, az_debug_message_vec_destructor, az_debug_message_vec_delete);
    impl_vec_clone!(AzDebugMessage, AzDebugMessageVec, AzDebugMessageVecDestructor);
    impl_vec!(AzDom, AzDomVec, AzDomVecDestructor, az_dom_vec_destructor, az_dom_vec_delete);
    impl_vec_clone!(AzDom, AzDomVec, AzDomVecDestructor);
    impl_vec!(AzString, AzStringVec, AzStringVecDestructor, az_string_vec_destructor, az_string_vec_delete);
    impl_vec_clone!(AzString, AzStringVec, AzStringVecDestructor);
    impl_vec!(AzStringPair, AzStringPairVec, AzStringPairVecDestructor, az_string_pair_vec_destructor, az_string_pair_vec_delete);
    impl_vec_clone!(AzStringPair, AzStringPairVec, AzStringPairVecDestructor);
    impl_vec!(AzLinearColorStop, AzLinearColorStopVec, AzLinearColorStopVecDestructor, az_linear_color_stop_vec_destructor, az_linear_color_stop_vec_delete);
    impl_vec_clone!(AzLinearColorStop, AzLinearColorStopVec, AzLinearColorStopVecDestructor);
    impl_vec!(AzRadialColorStop, AzRadialColorStopVec, AzRadialColorStopVecDestructor, az_radial_color_stop_vec_destructor, az_radial_color_stop_vec_delete);
    impl_vec_clone!(AzRadialColorStop, AzRadialColorStopVec, AzRadialColorStopVecDestructor);
    impl_vec!(AzNodeId, AzNodeIdVec, AzNodeIdVecDestructor, az_node_id_vec_destructor, az_node_id_vec_delete);
    impl_vec_clone!(AzNodeId, AzNodeIdVec, AzNodeIdVecDestructor);
    impl_vec!(AzNode, AzNodeVec, AzNodeVecDestructor, az_node_vec_destructor, az_node_vec_delete);
    impl_vec_clone!(AzNode, AzNodeVec, AzNodeVecDestructor);
    impl_vec!(AzStyledNode, AzStyledNodeVec, AzStyledNodeVecDestructor, az_styled_node_vec_destructor, az_styled_node_vec_delete);
    impl_vec_clone!(AzStyledNode, AzStyledNodeVec, AzStyledNodeVecDestructor);
    impl_vec!(AzTagIdToNodeIdMapping, AzTagIdsToNodeIdsMappingVec, AzTagIdsToNodeIdsMappingVecDestructor, az_tag_ids_to_node_ids_mapping_vec_destructor, az_tag_ids_to_node_ids_mapping_vec_delete);
    impl_vec_clone!(AzTagIdToNodeIdMapping, AzTagIdsToNodeIdsMappingVec, AzTagIdsToNodeIdsMappingVecDestructor);
    impl_vec!(AzParentWithNodeDepth, AzParentWithNodeDepthVec, AzParentWithNodeDepthVecDestructor, az_parent_with_node_depth_vec_destructor, az_parent_with_node_depth_vec_delete);
    impl_vec_clone!(AzParentWithNodeDepth, AzParentWithNodeDepthVec, AzParentWithNodeDepthVecDestructor);
    impl_vec!(AzNodeData, AzNodeDataVec, AzNodeDataVecDestructor, az_node_data_vec_destructor, az_node_data_vec_delete);
    impl_vec_clone!(AzNodeData, AzNodeDataVec, AzNodeDataVecDestructor);
    impl_vec!(AzStyleBackgroundRepeat, AzStyleBackgroundRepeatVec, AzStyleBackgroundRepeatVecDestructor, az_style_background_repeat_vec_destructor, az_style_background_repeat_vec_delete);
    impl_vec_clone!(AzStyleBackgroundRepeat, AzStyleBackgroundRepeatVec, AzStyleBackgroundRepeatVecDestructor);
    impl_vec!(AzStyleBackgroundPosition, AzStyleBackgroundPositionVec, AzStyleBackgroundPositionVecDestructor, az_style_background_position_vec_destructor, az_style_background_position_vec_delete);
    impl_vec_clone!(AzStyleBackgroundPosition, AzStyleBackgroundPositionVec, AzStyleBackgroundPositionVecDestructor);
    impl_vec!(AzStyleBackgroundSize, AzStyleBackgroundSizeVec, AzStyleBackgroundSizeVecDestructor, az_style_background_size_vec_destructor, az_style_background_size_vec_delete);
    impl_vec_clone!(AzStyleBackgroundSize, AzStyleBackgroundSizeVec, AzStyleBackgroundSizeVecDestructor);
    impl_vec!(AzStyleBackgroundContent, AzStyleBackgroundContentVec, AzStyleBackgroundContentVecDestructor, az_style_background_content_vec_destructor, az_style_background_content_vec_delete);
    impl_vec_clone!(AzStyleBackgroundContent, AzStyleBackgroundContentVec, AzStyleBackgroundContentVecDestructor);

    impl From<vec::Vec<string::String>> for crate::vec::StringVec {
        fn from(v: vec::Vec<string::String>) -> crate::vec::StringVec {
            let vec: Vec<AzString> = v.into_iter().map(Into::into).collect();
            vec.into()
            // v dropped here
        }
    }