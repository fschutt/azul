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
        unsafe impl Sync for $struct_name { }

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
                // necessary so that double-frees are avoided
                self.destructor = $destructor_name::NoDestructor;
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

    impl_vec!(u8,  AzU8Vec,  AzU8VecDestructor, az_u8_vec_destructor, AzU8Vec_delete);
    impl_vec_clone!(u8,  AzU8Vec,  AzU8VecDestructor);
    impl_vec!(u16, AzU16Vec, AzU16VecDestructor, az_u16_vec_destructor, AzU16Vec_delete);
    impl_vec_clone!(u16, AzU16Vec, AzU16VecDestructor);
    impl_vec!(u32, AzU32Vec, AzU32VecDestructor, az_u32_vec_destructor, AzU32Vec_delete);
    impl_vec_clone!(u32, AzU32Vec, AzU32VecDestructor);
    impl_vec!(u32, AzScanCodeVec, AzScanCodeVecDestructor, az_scan_code_vec_destructor, AzScanCodeVec_delete);
    impl_vec_clone!(u32, AzScanCodeVec, AzScanCodeVecDestructor);
    impl_vec!(u32, AzGLuintVec, AzGLuintVecDestructor, az_g_luint_vec_destructor, AzGLuintVec_delete);
    impl_vec_clone!(u32, AzGLuintVec, AzGLuintVecDestructor);
    impl_vec!(i32, AzGLintVec, AzGLintVecDestructor, az_g_lint_vec_destructor, AzGLintVec_delete);
    impl_vec_clone!(i32, AzGLintVec, AzGLintVecDestructor);
    impl_vec!(f32,  AzF32Vec,  AzF32VecDestructor, az_f32_vec_destructor, AzF32Vec_delete);
    impl_vec_clone!(f32,  AzF32Vec,  AzF32VecDestructor);
    impl_vec!(AzXmlNode,  AzXmlNodeVec,  AzXmlNodeVecDestructor, az_xml_node_vec_destructor, AzXmlNodeVec_delete);
    impl_vec_clone!(AzXmlNode,  AzXmlNodeVec,  AzXmlNodeVecDestructor);
    impl_vec!(AzInlineWord,  AzInlineWordVec,  AzInlineWordVecDestructor, az_inline_word_vec_destructor, AzInlineWordVec_delete);
    impl_vec_clone!(AzInlineWord,  AzInlineWordVec,  AzInlineWordVecDestructor);
    impl_vec!(AzInlineGlyph,  AzInlineGlyphVec,  AzInlineGlyphVecDestructor, az_inline_glyph_vec_destructor, AzInlineGlyphVec_delete);
    impl_vec_clone!(AzInlineGlyph,  AzInlineGlyphVec,  AzInlineGlyphVecDestructor);
    impl_vec!(AzInlineLine,  AzInlineLineVec,  AzInlineLineVecDestructor, az_inline_line_vec_destructor, AzInlineLineVec_delete);
    impl_vec_clone!(AzInlineLine,  AzInlineLineVec,  AzInlineLineVecDestructor);
    impl_vec!(AzFmtArg,  AzFmtArgVec,  AzFmtArgVecDestructor, az_fmt_arg_vec_destructor, AzFmtArgVec_delete);
    impl_vec_clone!(AzFmtArg,  AzFmtArgVec,  AzFmtArgVecDestructor);
    impl_vec!(AzInlineTextHit,  AzInlineTextHitVec,  AzInlineTextHitVecDestructor, az_inline_text_hit_vec_destructor, AzInlineTextHitVec_delete);
    impl_vec_clone!(AzInlineTextHit,  AzInlineTextHitVec,  AzInlineTextHitVecDestructor);
    impl_vec!(AzTessellatedSvgNode,  AzTessellatedSvgNodeVec,  AzTessellatedSvgNodeVecDestructor, az_tesselated_svg_node_vec_destructor, AzTessellatedSvgNodeVec_delete);
    impl_vec_clone!(AzTessellatedSvgNode,  AzTessellatedSvgNodeVec,  AzTessellatedSvgNodeVecDestructor);
    impl_vec!(AzNodeDataInlineCssProperty, AzNodeDataInlineCssPropertyVec, NodeDataInlineCssPropertyVecDestructor, az_node_data_inline_css_property_vec_destructor, AzNodeDataInlineCssPropertyVec_delete);
    impl_vec_clone!(AzNodeDataInlineCssProperty, AzNodeDataInlineCssPropertyVec, NodeDataInlineCssPropertyVecDestructor);
    impl_vec!(AzIdOrClass, AzIdOrClassVec, IdOrClassVecDestructor, az_id_or_class_vec_destructor, AzIdOrClassVec_delete);
    impl_vec_clone!(AzIdOrClass, AzIdOrClassVec, IdOrClassVecDestructor);
    impl_vec!(AzStyleTransform, AzStyleTransformVec, AzStyleTransformVecDestructor, az_style_transform_vec_destructor, AzStyleTransformVec_delete);
    impl_vec_clone!(AzStyleTransform, AzStyleTransformVec, AzStyleTransformVecDestructor);
    impl_vec!(AzCssProperty, AzCssPropertyVec, AzCssPropertyVecDestructor, az_css_property_vec_destructor, AzCssPropertyVec_delete);
    impl_vec_clone!(AzCssProperty, AzCssPropertyVec, AzCssPropertyVecDestructor);
    impl_vec!(AzSvgMultiPolygon, AzSvgMultiPolygonVec, AzSvgMultiPolygonVecDestructor, az_svg_multi_polygon_vec_destructor, AzSvgMultiPolygonVec_delete);
    impl_vec_clone!(AzSvgMultiPolygon, AzSvgMultiPolygonVec, AzSvgMultiPolygonVecDestructor);
    impl_vec!(AzSvgPath, AzSvgPathVec, AzSvgPathVecDestructor, az_svg_path_vec_destructor, AzSvgPathVec_delete);
    impl_vec_clone!(AzSvgPath, AzSvgPathVec, AzSvgPathVecDestructor);
    impl_vec!(AzVertexAttribute, AzVertexAttributeVec, AzVertexAttributeVecDestructor, az_vertex_attribute_vec_destructor, AzVertexAttributeVec_delete);
    impl_vec_clone!(AzVertexAttribute, AzVertexAttributeVec, AzVertexAttributeVecDestructor);
    impl_vec!(AzSvgPathElement, AzSvgPathElementVec, AzSvgPathElementVecDestructor, az_svg_path_element_vec_destructor, AzSvgPathElementVec_delete);
    impl_vec_clone!(AzSvgPathElement, AzSvgPathElementVec, AzSvgPathElementVecDestructor);
    impl_vec!(AzSvgVertex, AzSvgVertexVec, AzSvgVertexVecDestructor, az_svg_vertex_vec_destructor, AzSvgVertexVec_delete);
    impl_vec_clone!(AzSvgVertex, AzSvgVertexVec, AzSvgVertexVecDestructor);
    impl_vec!(AzXWindowType, AzXWindowTypeVec, AzXWindowTypeVecDestructor, az_x_window_type_vec_destructor, AzXWindowTypeVec_delete);
    impl_vec_clone!(AzXWindowType, AzXWindowTypeVec, AzXWindowTypeVecDestructor);
    impl_vec!(AzVirtualKeyCode, AzVirtualKeyCodeVec, AzVirtualKeyCodeVecDestructor, az_virtual_key_code_vec_destructor, AzVirtualKeyCodeVec_delete);
    impl_vec_clone!(AzVirtualKeyCode, AzVirtualKeyCodeVec, AzVirtualKeyCodeVecDestructor);
    impl_vec!(AzCascadeInfo, AzCascadeInfoVec, AzCascadeInfoVecDestructor, az_cascade_info_vec_destructor, AzCascadeInfoVec_delete);
    impl_vec_clone!(AzCascadeInfo, AzCascadeInfoVec, AzCascadeInfoVecDestructor);
    impl_vec!(AzCssDeclaration, AzCssDeclarationVec, AzCssDeclarationVecDestructor, az_css_declaration_vec_destructor, AzCssDeclarationVec_delete);
    impl_vec_clone!(AzCssDeclaration, AzCssDeclarationVec, AzCssDeclarationVecDestructor);
    impl_vec!(AzCssPathSelector, AzCssPathSelectorVec, AzCssPathSelectorVecDestructor, az_css_path_selector_vec_destructor, AzCssPathSelectorVec_delete);
    impl_vec_clone!(AzCssPathSelector, AzCssPathSelectorVec, AzCssPathSelectorVecDestructor);
    impl_vec!(AzStylesheet, AzStylesheetVec, AzStylesheetVecDestructor, az_stylesheet_vec_destructor, AzStylesheetVec_delete);
    impl_vec_clone!(AzStylesheet, AzStylesheetVec, AzStylesheetVecDestructor);
    impl_vec!(AzCssRuleBlock, AzCssRuleBlockVec, AzCssRuleBlockVecDestructor, az_css_rule_block_vec_destructor, AzCssRuleBlockVec_delete);
    impl_vec_clone!(AzCssRuleBlock, AzCssRuleBlockVec, AzCssRuleBlockVecDestructor);
    impl_vec!(AzCallbackData, AzCallbackDataVec, AzCallbackDataVecDestructor, az_callback_data_vec_destructor, AzCallbackDataVec_delete);
    impl_vec_clone!(AzCallbackData, AzCallbackDataVec, AzCallbackDataVecDestructor);
    impl_vec!(AzDebugMessage, AzDebugMessageVec, AzDebugMessageVecDestructor, az_debug_message_vec_destructor, AzDebugMessageVec_delete);
    impl_vec_clone!(AzDebugMessage, AzDebugMessageVec, AzDebugMessageVecDestructor);
    impl_vec!(AzDom, AzDomVec, AzDomVecDestructor, az_dom_vec_destructor, AzDomVec_delete);
    impl_vec_clone!(AzDom, AzDomVec, AzDomVecDestructor);
    impl_vec!(AzString, AzStringVec, AzStringVecDestructor, az_string_vec_destructor, AzStringVec_delete);
    impl_vec_clone!(AzString, AzStringVec, AzStringVecDestructor);
    impl_vec!(AzStringPair, AzStringPairVec, AzStringPairVecDestructor, az_string_pair_vec_destructor, AzStringPairVec_delete);
    impl_vec_clone!(AzStringPair, AzStringPairVec, AzStringPairVecDestructor);
    impl_vec!(AzNormalizedLinearColorStop, AzNormalizedLinearColorStopVec, AzNormalizedLinearColorStopVecDestructor, az_normalized_linear_color_stop_vec_destructor, AzNormalizedLinearColorStopVec_delete);
    impl_vec_clone!(AzNormalizedLinearColorStop, AzNormalizedLinearColorStopVec, AzNormalizedLinearColorStopVecDestructor);
    impl_vec!(AzNormalizedRadialColorStop, AzNormalizedRadialColorStopVec, AzNormalizedRadialColorStopVecDestructor, az_normalized_radial_color_stop_vec_destructor, AzNormalizedRadialColorStopVec_delete);
    impl_vec_clone!(AzNormalizedRadialColorStop, AzNormalizedRadialColorStopVec, AzNormalizedRadialColorStopVecDestructor);
    impl_vec!(AzNodeId, AzNodeIdVec, AzNodeIdVecDestructor, az_node_id_vec_destructor, AzNodeIdVec_delete);
    impl_vec_clone!(AzNodeId, AzNodeIdVec, AzNodeIdVecDestructor);
    impl_vec!(AzNodeHierarchyItem, AzNodeHierarchyItemVec, AzNodeHierarchyItemVecDestructor, az_node_hierarchy_item_vec_destructor, AzNodeHierarchyItemVec_delete);
    impl_vec_clone!(AzNodeHierarchyItem, AzNodeHierarchyItemVec, AzNodeHierarchyItemVecDestructor);
    impl_vec!(AzStyledNode, AzStyledNodeVec, AzStyledNodeVecDestructor, az_styled_node_vec_destructor, AzStyledNodeVec_delete);
    impl_vec_clone!(AzStyledNode, AzStyledNodeVec, AzStyledNodeVecDestructor);
    impl_vec!(AzTagIdToNodeIdMapping, AzTagIdToNodeIdMappingVec, AzTagIdToNodeIdMappingVecDestructor, az_tag_id_to_node_id_mapping_vec_destructor, AzTagIdToNodeIdMappingVec_delete);
    impl_vec_clone!(AzTagIdToNodeIdMapping, AzTagIdToNodeIdMappingVec, AzTagIdToNodeIdMappingVecDestructor);
    impl_vec!(AzParentWithNodeDepth, AzParentWithNodeDepthVec, AzParentWithNodeDepthVecDestructor, az_parent_with_node_depth_vec_destructor, AzParentWithNodeDepthVec_delete);
    impl_vec_clone!(AzParentWithNodeDepth, AzParentWithNodeDepthVec, AzParentWithNodeDepthVecDestructor);
    impl_vec!(AzNodeData, AzNodeDataVec, AzNodeDataVecDestructor, az_node_data_vec_destructor, AzNodeDataVec_delete);
    impl_vec_clone!(AzNodeData, AzNodeDataVec, AzNodeDataVecDestructor);
    impl_vec!(AzStyleBackgroundRepeat, AzStyleBackgroundRepeatVec, AzStyleBackgroundRepeatVecDestructor, az_style_background_repeat_vec_destructor, AzStyleBackgroundRepeatVec_delete);
    impl_vec_clone!(AzStyleBackgroundRepeat, AzStyleBackgroundRepeatVec, AzStyleBackgroundRepeatVecDestructor);
    impl_vec!(AzStyleBackgroundPosition, AzStyleBackgroundPositionVec, AzStyleBackgroundPositionVecDestructor, az_style_background_position_vec_destructor, AzStyleBackgroundPositionVec_delete);
    impl_vec_clone!(AzStyleBackgroundPosition, AzStyleBackgroundPositionVec, AzStyleBackgroundPositionVecDestructor);
    impl_vec!(AzStyleBackgroundSize, AzStyleBackgroundSizeVec, AzStyleBackgroundSizeVecDestructor, az_style_background_size_vec_destructor, AzStyleBackgroundSizeVec_delete);
    impl_vec_clone!(AzStyleBackgroundSize, AzStyleBackgroundSizeVec, AzStyleBackgroundSizeVecDestructor);
    impl_vec!(AzStyleBackgroundContent, AzStyleBackgroundContentVec, AzStyleBackgroundContentVecDestructor, az_style_background_content_vec_destructor, AzStyleBackgroundContentVec_delete);
    impl_vec_clone!(AzStyleBackgroundContent, AzStyleBackgroundContentVec, AzStyleBackgroundContentVecDestructor);
    impl_vec!(AzVideoMode, AzVideoModeVec, AzVideoModeVecDestructor, az_video_mode_vec_destructor, AzVideoModeVec_delete);
    impl_vec_clone!(AzVideoMode, AzVideoModeVec, AzVideoModeVecDestructor);
    impl_vec!(AzMonitor, AzMonitorVec, AzMonitorVecDestructor, az_monitor_vec_destructor, AzMonitorVec_delete);
    impl_vec_clone!(AzMonitor, AzMonitorVec, AzMonitorVecDestructor);
    impl_vec!(AzStyleFontFamily, AzStyleFontFamilyVec, AzStyleFontFamilyVecDestructor, az_style_font_family_vec_destructor, AzStyleFontFamilyVec_delete);
    impl_vec_clone!(AzStyleFontFamily, AzStyleFontFamilyVec, AzStyleFontFamilyVecDestructor);
    impl_vec!(AzTab, AzTabVec, AzTabVecDestructor, az_tab_vec_destructor, AzTabVec_delete);
    impl_vec_clone!(AzTab, AzTabVec, AzTabVecDestructor);
    impl_vec!(AzNodeTypeIdInfoMap, AzNodeTypeIdInfoMapVec, AzNodeTypeIdInfoMapVecDestructor, az_node_type_id_info_map_vec_destructor, AzNodeTypeIdInfoMapVec_delete);
    impl_vec_clone!(AzNodeTypeIdInfoMap, AzNodeTypeIdInfoMapVec, AzNodeTypeIdInfoMapVecDestructor);
    impl_vec!(AzInputOutputTypeIdInfoMap, AzInputOutputTypeIdInfoMapVec, AzInputOutputTypeIdInfoMapVecDestructor, az_input_output_type_id_info_map_vec_destructor, AzInputOutputTypeIdInfoMapVec_delete);
    impl_vec_clone!(AzInputOutputTypeIdInfoMap, AzInputOutputTypeIdInfoMapVec, AzInputOutputTypeIdInfoMapVecDestructor);
    impl_vec!(AzNodeIdNodeMap, AzNodeIdNodeMapVec, AzNodeIdNodeMapVecDestructor, az_node_id_node_map_vec_destructor, AzNodeIdNodeMapVec_delete);
    impl_vec_clone!(AzNodeIdNodeMap, AzNodeIdNodeMapVec, AzNodeIdNodeMapVecDestructor);
    impl_vec!(AzInputOutputTypeId, AzInputOutputTypeIdVec, AzInputOutputTypeIdVecDestructor, az_input_output_type_id_vec_destructor, AzInputOutputTypeIdVec_delete);
    impl_vec_clone!(AzInputOutputTypeId, AzInputOutputTypeIdVec, AzInputOutputTypeIdVecDestructor);
    impl_vec!(AzNodeTypeField, AzNodeTypeFieldVec, AzNodeTypeFieldVecDestructor, az_node_type_field_vec_destructor, AzNodeTypeFieldVec_delete);
    impl_vec_clone!(AzNodeTypeField, AzNodeTypeFieldVec, AzNodeTypeFieldVecDestructor);
    impl_vec!(AzInputConnection, AzInputConnectionVec, AzInputConnectionVecDestructor, az_input_connection_vec_destructor, AzInputConnectionVec_delete);
    impl_vec_clone!(AzInputConnection, AzInputConnectionVec, AzInputConnectionVecDestructor);
    impl_vec!(AzOutputNodeAndIndex, AzOutputNodeAndIndexVec, AzOutputNodeAndIndexVecDestructor, az_output_node_and_index_vec_destructor, AzOutputNodeAndIndexVec_delete);
    impl_vec_clone!(AzOutputNodeAndIndex, AzOutputNodeAndIndexVec, AzOutputNodeAndIndexVecDestructor);
    impl_vec!(AzOutputConnection, AzOutputConnectionVec, AzOutputConnectionVecDestructor, az_output_connection_vec_destructor, AzOutputConnectionVec_delete);
    impl_vec_clone!(AzOutputConnection, AzOutputConnectionVec, AzOutputConnectionVecDestructor);
    impl_vec!(AzInputNodeAndIndex, AzInputNodeAndIndexVec, AzInputNodeAndIndexVecDestructor, az_input_node_and_index_vec_destructor, AzInputNodeAndIndexVec_delete);
    impl_vec_clone!(AzInputNodeAndIndex, AzInputNodeAndIndexVec, AzInputNodeAndIndexVecDestructor);
    impl_vec!(AzLogicalRect, AzLogicalRectVec, AzLogicalRectVecDestructor, az_logical_rect_vec_destructor, AzLogicalRectVec_delete);
    impl_vec_clone!(AzLogicalRect, AzLogicalRectVec, AzLogicalRectVecDestructor);


    impl_vec!(AzAccessibilityState,  AzAccessibilityStateVec,  AzAccessibilityStateVecDestructor, az_accessibility_state_vec_destructor, AzAccessibilityStateVec_delete);
    impl_vec_clone!(AzAccessibilityState,  AzAccessibilityStateVec,  AzAccessibilityStateVecDestructor);

    impl_vec!(AzMenuItem,  AzMenuItemVec,  AzMenuItemVecDestructor, az_menu_item_vec_destructor, AzMenuItemVec_delete);
    impl_vec_clone!(AzMenuItem,  AzMenuItemVec,  AzMenuItemVecDestructor);

    impl From<vec::Vec<string::String>> for crate::vec::StringVec {
        fn from(v: vec::Vec<string::String>) -> crate::vec::StringVec {
            let vec: Vec<AzString> = v.into_iter().map(Into::into).collect();
            vec.into()
            // v dropped here
        }
    }