    use std::ffi::c_void;

    /// Return type of a regular callback - currently `AzUpdateScreen`
    pub type AzCallbackReturn = AzUpdateScreen;
    /// Callback for responding to window events
    pub type AzCallbackType = extern "C" fn(&mut AzRefAny, AzCallbackInfo) -> AzCallbackReturn;
    /// Callback fn that returns the DOM of the app
    pub type AzLayoutCallbackType = extern "C" fn(&AzRefAny, AzLayoutInfo) -> AzStyledDom;
    /// Callback for rendering to an OpenGL texture
    pub type AzGlCallbackType = extern "C" fn(&AzRefAny, AzGlCallbackInfo) -> AzGlCallbackReturn;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzIFrameCallbackType = extern "C" fn(&AzRefAny, AzIFrameCallbackInfo) -> AzIFrameCallbackReturn;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzRefAnyDestructorType = extern "C" fn(*const c_void);
    /// Callback for the `Timer` class
    pub type AzTimerCallbackType = extern "C" fn(&mut AzRefAny, &mut AzRefAny, AzTimerCallbackInfo) -> AzTimerCallbackReturn;
    /// Callback for the `Thread` class
    pub type AzThreadCallbackType= extern "C" fn(AzRefAny, AzThreadSender, AzThreadReceiver);
    /// Callback for the `WriteBack` class
    pub type AzWriteBackCallbackType =  extern "C" fn(&mut AzRefAny, AzRefAny, AzCallbackInfo) -> AzUpdateScreen;

    impl AzString {
        #[inline]
        pub fn as_str(&self) -> &str {
            unsafe { std::str::from_utf8_unchecked(self.as_bytes()) }
        }
        #[inline]
        pub fn as_bytes(&self) -> &[u8] {
            unsafe { std::slice::from_raw_parts(self.vec.ptr, self.vec.len) }
        }
    }

    impl ::std::fmt::Debug for AzCallback                   { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzLayoutCallback             { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzGlCallback                 { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzIFrameCallback             { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzTimerCallback              { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzWriteBackCallback          { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzRefAny                     {
        fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            write!(f, "RefAny {{")?;
            write!(f, "_internal_ptr: {:x}", self._internal_ptr as usize)?;
            write!(f, "_internal_len: {}", self._internal_len)?;
            write!(f, "_internal_layout_size: {}", self._internal_layout_size)?;
            write!(f, "_internal_layout_align: {}", self._internal_layout_align)?;
            write!(f, "type_id: {}", self.type_id)?;
            write!(f, "type_name: \"{}\"", self.type_name.as_str())?;
            write!(f, "_sharing_info_ptr: \"{}\"", self._sharing_info_ptr as usize)?;
            write!(f, "custom_destructor: \"{}\"", self.custom_destructor as usize)?;
            write!(f, "}}")?;
            Ok(())
        }
    }    /// Re-export of rust-allocated (stack based) `String` struct
    #[repr(C)] #[derive(Debug)] pub struct AzString {
        pub vec: AzU8Vec,
    }
    /// Wrapper over a Rust-allocated `Vec<StyleTransform>`
    #[repr(C)] #[derive(Debug)] pub struct AzStyleTransformVec {
        pub(crate) ptr: *mut AzStyleTransform,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<ContentGroup>`
    #[repr(C)] #[derive(Debug)] pub struct AzContentGroupVec {
        pub(crate) ptr: *mut AzContentGroup,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<CssProperty>`
    #[repr(C)] #[derive(Debug)] pub struct AzCssPropertyVec {
        pub(crate) ptr: *mut AzCssProperty,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<SvgMultiPolygon>`
    #[repr(C)] #[derive(Debug)] pub struct AzSvgMultiPolygonVec {
        pub(crate) ptr: *mut AzSvgMultiPolygon,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<SvgPath>`
    #[repr(C)] #[derive(Debug)] pub struct AzSvgPathVec {
        pub(crate) ptr: *mut AzSvgPath,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<VertexAttribute>`
    #[repr(C)] #[derive(Debug)] pub struct AzVertexAttributeVec {
        pub(crate) ptr: *mut AzVertexAttribute,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `VertexAttribute`
    #[repr(C)] #[derive(Debug)] pub struct AzSvgPathElementVec {
        pub(crate) ptr: *mut AzSvgPathElement,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `SvgVertex`
    #[repr(C)] #[derive(Debug)] pub struct AzSvgVertexVec {
        pub(crate) ptr: *mut AzSvgVertex,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<u32>`
    #[repr(C)] #[derive(Debug)] pub struct AzU32Vec {
        pub(crate) ptr: *mut u32,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `XWindowType`
    #[repr(C)] #[derive(Debug)] pub struct AzXWindowTypeVec {
        pub(crate) ptr: *mut AzXWindowType,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `VirtualKeyCode`
    #[repr(C)] #[derive(Debug)] pub struct AzVirtualKeyCodeVec {
        pub(crate) ptr: *mut AzVirtualKeyCode,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CascadeInfo`
    #[repr(C)] #[derive(Debug)] pub struct AzCascadeInfoVec {
        pub(crate) ptr: *mut AzCascadeInfo,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `ScanCode`
    #[repr(C)] #[derive(Debug)] pub struct AzScanCodeVec {
        pub(crate) ptr: *mut u32,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CssDeclaration`
    #[repr(C)] #[derive(Debug)] pub struct AzCssDeclarationVec {
        pub(crate) ptr: *mut AzCssDeclaration,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CssPathSelector`
    #[repr(C)] #[derive(Debug)] pub struct AzCssPathSelectorVec {
        pub(crate) ptr: *mut AzCssPathSelector,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Stylesheet`
    #[repr(C)] #[derive(Debug)] pub struct AzStylesheetVec {
        pub(crate) ptr: *mut AzStylesheet,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CssRuleBlock`
    #[repr(C)] #[derive(Debug)] pub struct AzCssRuleBlockVec {
        pub(crate) ptr: *mut AzCssRuleBlock,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `U8Vec`
    #[repr(C)] #[derive(Debug)] pub struct AzU8Vec {
        pub(crate) ptr: *mut u8,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CallbackData`
    #[repr(C)] #[derive(Debug)] pub struct AzCallbackDataVec {
        pub(crate) ptr: *mut AzCallbackData,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<DebugMessage>`
    #[repr(C)] #[derive(Debug)] pub struct AzDebugMessageVec {
        pub(crate) ptr: *mut AzDebugMessage,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `U32Vec`
    #[repr(C)] #[derive(Debug)] pub struct AzGLuintVec {
        pub(crate) ptr: *mut u32,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `GLintVec`
    #[repr(C)] #[derive(Debug)] pub struct AzGLintVec {
        pub(crate) ptr: *mut i32,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `DomVec`
    #[repr(C)] #[derive(Debug)] pub struct AzDomVec {
        pub(crate) ptr: *mut AzDom,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `StringVec`
    #[repr(C)] #[derive(Debug)] pub struct AzStringVec {
        pub(crate) ptr: *mut AzString,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `StringPairVec`
    #[repr(C)] #[derive(Debug)] pub struct AzStringPairVec {
        pub(crate) ptr: *mut AzStringPair,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `GradientStopPreVec`
    #[repr(C)] #[derive(Debug)] pub struct AzGradientStopPreVec {
        pub(crate) ptr: *mut AzGradientStopPre,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CascadedCssPropertyWithSourceVec`
    #[repr(C)] #[derive(Debug)] pub struct AzCascadedCssPropertyWithSourceVec {
        pub(crate) ptr: *mut AzCascadedCssPropertyWithSource,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `NodeIdVec`
    #[repr(C)] #[derive(Debug)] pub struct AzNodeIdVec {
        pub(crate) ptr: *mut AzNodeId,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `NodeVec`
    #[repr(C)] #[derive(Debug)] pub struct AzNodeVec {
        pub(crate) ptr: *mut AzNode,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `StyledNodeVec`
    #[repr(C)] #[derive(Debug)] pub struct AzStyledNodeVec {
        pub(crate) ptr: *mut AzStyledNode,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `TagIdsToNodeIdsMappingVec`
    #[repr(C)] #[derive(Debug)] pub struct AzTagIdsToNodeIdsMappingVec {
        pub(crate) ptr: *mut AzTagIdToNodeIdMapping,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `ParentWithNodeDepthVec`
    #[repr(C)] #[derive(Debug)] pub struct AzParentWithNodeDepthVec {
        pub(crate) ptr: *mut AzParentWithNodeDepth,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `NodeDataVec`
    #[repr(C)] #[derive(Debug)] pub struct AzNodeDataVec {
        pub(crate) ptr: *mut AzNodeData,
        pub len: usize,
        pub cap: usize,
    }
    /// Re-export of rust-allocated (stack based) `OptionThreadSendMsg` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionThreadSendMsg {
        None,
        Some(AzThreadSendMsg),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutRect` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutRect {
        None,
        Some(AzLayoutRect),
    }
    /// Re-export of rust-allocated (stack based) `OptionRefAny` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionRefAny {
        None,
        Some(AzRefAny),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleOpacityValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleOpacityValue {
        None,
        Some(AzStyleOpacityValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleTransformVecValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleTransformVecValue {
        None,
        Some(AzStyleTransformVecValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleTransformOriginValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleTransformOriginValue {
        None,
        Some(AzStyleTransformOriginValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStylePerspectiveOriginValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStylePerspectiveOriginValue {
        None,
        Some(AzStylePerspectiveOriginValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBackfaceVisibilityValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBackfaceVisibilityValue {
        None,
        Some(AzStyleBackfaceVisibilityValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutPoint` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutPoint {
        None,
        Some(AzLayoutPoint),
    }
    /// Re-export of rust-allocated (stack based) `OptionWindowTheme` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionWindowTheme {
        None,
        Some(AzWindowTheme),
    }
    /// Re-export of rust-allocated (stack based) `OptionNodeId` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionNodeId {
        None,
        Some(AzNodeId),
    }
    /// Re-export of rust-allocated (stack based) `OptionDomNodeId` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionDomNodeId {
        None,
        Some(AzDomNodeId),
    }
    /// Re-export of rust-allocated (stack based) `OptionColorU` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionColorU {
        None,
        Some(AzColorU),
    }
    /// Re-export of rust-allocated (stack based) `OptionRawImage` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionRawImage {
        None,
        Some(AzRawImage),
    }
    /// Re-export of rust-allocated (stack based) `OptionSvgDashPattern` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionSvgDashPattern {
        None,
        Some(AzSvgDashPattern),
    }
    /// Re-export of rust-allocated (stack based) `OptionWaylandTheme` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionWaylandTheme {
        None,
        Some(AzWaylandTheme),
    }
    /// Re-export of rust-allocated (stack based) `OptionTaskBarIcon` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionTaskBarIcon {
        None,
        Some(AzTaskBarIcon),
    }
    /// Re-export of rust-allocated (stack based) `OptionHwndHandle` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionHwndHandle {
        None,
        Some(*mut c_void),
    }
    /// Re-export of rust-allocated (stack based) `OptionLogicalPosition` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLogicalPosition {
        None,
        Some(AzLogicalPosition),
    }
    /// Re-export of rust-allocated (stack based) `OptionPhysicalPositionI32` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionPhysicalPositionI32 {
        None,
        Some(AzPhysicalPositionI32),
    }
    /// Re-export of rust-allocated (stack based) `OptionWindowIcon` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionWindowIcon {
        None,
        Some(AzWindowIcon),
    }
    /// Re-export of rust-allocated (stack based) `OptionString` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionString {
        None,
        Some(AzString),
    }
    /// Re-export of rust-allocated (stack based) `OptionX11Visual` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionX11Visual {
        None,
        Some(*const c_void),
    }
    /// Re-export of rust-allocated (stack based) `OptionI32` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionI32 {
        None,
        Some(i32),
    }
    /// Re-export of rust-allocated (stack based) `OptionF32` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionF32 {
        None,
        Some(f32),
    }
    /// Re-export of rust-allocated (stack based) `OptionMouseCursorType` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionMouseCursorType {
        None,
        Some(AzMouseCursorType),
    }
    /// Re-export of rust-allocated (stack based) `OptionLogicalSize` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLogicalSize {
        None,
        Some(AzLogicalSize),
    }
    /// Option<char> but the char is a u32, for C FFI stability reasons
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionChar {
        None,
        Some(u32),
    }
    /// Re-export of rust-allocated (stack based) `OptionVirtualKeyCode` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionVirtualKeyCode {
        None,
        Some(AzVirtualKeyCode),
    }
    /// Re-export of rust-allocated (stack based) `OptionPercentageValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionPercentageValue {
        None,
        Some(AzPercentageValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionDom` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionDom {
        None,
        Some(AzDom),
    }
    /// Re-export of rust-allocated (stack based) `OptionTexture` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionTexture {
        None,
        Some(AzTexture),
    }
    /// Re-export of rust-allocated (stack based) `OptionImageMask` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionImageMask {
        None,
        Some(AzImageMask),
    }
    /// Re-export of rust-allocated (stack based) `OptionTabIndex` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionTabIndex {
        None,
        Some(AzTabIndex),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBackgroundContentValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBackgroundContentValue {
        None,
        Some(AzStyleBackgroundContentValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBackgroundPositionValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBackgroundPositionValue {
        None,
        Some(AzStyleBackgroundPositionValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBackgroundSizeValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBackgroundSizeValue {
        None,
        Some(AzStyleBackgroundSizeValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBackgroundRepeatValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBackgroundRepeatValue {
        None,
        Some(AzStyleBackgroundRepeatValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleFontSizeValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleFontSizeValue {
        None,
        Some(AzStyleFontSizeValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleFontFamilyValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleFontFamilyValue {
        None,
        Some(AzStyleFontFamilyValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleTextColorValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleTextColorValue {
        None,
        Some(AzStyleTextColorValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleTextAlignmentHorzValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleTextAlignmentHorzValue {
        None,
        Some(AzStyleTextAlignmentHorzValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleLineHeightValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleLineHeightValue {
        None,
        Some(AzStyleLineHeightValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleLetterSpacingValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleLetterSpacingValue {
        None,
        Some(AzStyleLetterSpacingValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleWordSpacingValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleWordSpacingValue {
        None,
        Some(AzStyleWordSpacingValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleTabWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleTabWidthValue {
        None,
        Some(AzStyleTabWidthValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleCursorValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleCursorValue {
        None,
        Some(AzStyleCursorValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionBoxShadowPreDisplayItemValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionBoxShadowPreDisplayItemValue {
        None,
        Some(AzBoxShadowPreDisplayItemValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderTopColorValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderTopColorValue {
        None,
        Some(AzStyleBorderTopColorValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderLeftColorValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderLeftColorValue {
        None,
        Some(AzStyleBorderLeftColorValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderRightColorValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderRightColorValue {
        None,
        Some(AzStyleBorderRightColorValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderBottomColorValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderBottomColorValue {
        None,
        Some(AzStyleBorderBottomColorValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderTopStyleValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderTopStyleValue {
        None,
        Some(AzStyleBorderTopStyleValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderLeftStyleValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderLeftStyleValue {
        None,
        Some(AzStyleBorderLeftStyleValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderRightStyleValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderRightStyleValue {
        None,
        Some(AzStyleBorderRightStyleValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderBottomStyleValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderBottomStyleValue {
        None,
        Some(AzStyleBorderBottomStyleValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderTopLeftRadiusValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderTopLeftRadiusValue {
        None,
        Some(AzStyleBorderTopLeftRadiusValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderTopRightRadiusValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderTopRightRadiusValue {
        None,
        Some(AzStyleBorderTopRightRadiusValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderBottomLeftRadiusValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderBottomLeftRadiusValue {
        None,
        Some(AzStyleBorderBottomLeftRadiusValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderBottomRightRadiusValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderBottomRightRadiusValue {
        None,
        Some(AzStyleBorderBottomRightRadiusValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutDisplayValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutDisplayValue {
        None,
        Some(AzLayoutDisplayValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutFloatValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutFloatValue {
        None,
        Some(AzLayoutFloatValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutBoxSizingValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutBoxSizingValue {
        None,
        Some(AzLayoutBoxSizingValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutWidthValue {
        None,
        Some(AzLayoutWidthValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutHeightValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutHeightValue {
        None,
        Some(AzLayoutHeightValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMinWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutMinWidthValue {
        None,
        Some(AzLayoutMinWidthValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMinHeightValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutMinHeightValue {
        None,
        Some(AzLayoutMinHeightValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMaxWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutMaxWidthValue {
        None,
        Some(AzLayoutMaxWidthValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMaxHeightValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutMaxHeightValue {
        None,
        Some(AzLayoutMaxHeightValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutPositionValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutPositionValue {
        None,
        Some(AzLayoutPositionValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutTopValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutTopValue {
        None,
        Some(AzLayoutTopValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutBottomValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutBottomValue {
        None,
        Some(AzLayoutBottomValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutRightValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutRightValue {
        None,
        Some(AzLayoutRightValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutLeftValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutLeftValue {
        None,
        Some(AzLayoutLeftValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutPaddingTopValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutPaddingTopValue {
        None,
        Some(AzLayoutPaddingTopValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutPaddingBottomValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutPaddingBottomValue {
        None,
        Some(AzLayoutPaddingBottomValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutPaddingLeftValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutPaddingLeftValue {
        None,
        Some(AzLayoutPaddingLeftValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutPaddingRightValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutPaddingRightValue {
        None,
        Some(AzLayoutPaddingRightValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMarginTopValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutMarginTopValue {
        None,
        Some(AzLayoutMarginTopValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMarginBottomValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutMarginBottomValue {
        None,
        Some(AzLayoutMarginBottomValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMarginLeftValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutMarginLeftValue {
        None,
        Some(AzLayoutMarginLeftValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMarginRightValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutMarginRightValue {
        None,
        Some(AzLayoutMarginRightValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderTopWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderTopWidthValue {
        None,
        Some(AzStyleBorderTopWidthValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderLeftWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderLeftWidthValue {
        None,
        Some(AzStyleBorderLeftWidthValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderRightWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderRightWidthValue {
        None,
        Some(AzStyleBorderRightWidthValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderBottomWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionStyleBorderBottomWidthValue {
        None,
        Some(AzStyleBorderBottomWidthValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionOverflowValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionOverflowValue {
        None,
        Some(AzOverflowValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutFlexDirectionValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutFlexDirectionValue {
        None,
        Some(AzLayoutFlexDirectionValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutWrapValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutWrapValue {
        None,
        Some(AzLayoutWrapValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutFlexGrowValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutFlexGrowValue {
        None,
        Some(AzLayoutFlexGrowValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutFlexShrinkValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutFlexShrinkValue {
        None,
        Some(AzLayoutFlexShrinkValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutJustifyContentValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutJustifyContentValue {
        None,
        Some(AzLayoutJustifyContentValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutAlignItemsValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutAlignItemsValue {
        None,
        Some(AzLayoutAlignItemsValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutAlignContentValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionLayoutAlignContentValue {
        None,
        Some(AzLayoutAlignContentValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionTagId` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionTagId {
        None,
        Some(AzTagId),
    }
    /// Re-export of rust-allocated (stack based) `OptionDuration` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionDuration {
        None,
        Some(AzDuration),
    }
    /// Re-export of rust-allocated (stack based) `OptionInstantPtr` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionInstantPtr {
        None,
        Some(AzInstantPtr),
    }
    /// Re-export of rust-allocated (stack based) `OptionUsize` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionUsize {
        None,
        Some(usize),
    }
    /// Re-export of rust-allocated (stack based) `OptionU8VecRef` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionU8VecRef {
        None,
        Some(AzU8VecRef),
    }
    /// Re-export of rust-allocated (stack based) `ResultSvgSvgParseError` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzResultSvgSvgParseError {
        Ok(AzSvg),
        Err(AzSvgParseError),
    }
    /// Re-export of rust-allocated (stack based) `SvgParseError` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzSvgParseError {
        InvalidFileSuffix,
        FileOpenFailed,
        NotAnUtf8Str,
        MalformedGZip,
        InvalidSize,
        ParsingFailed(AzXmlError),
    }
    /// Re-export of rust-allocated (stack based) `XmlError` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzXmlError {
        InvalidXmlPrefixUri(AzSvgParseErrorPosition),
        UnexpectedXmlUri(AzSvgParseErrorPosition),
        UnexpectedXmlnsUri(AzSvgParseErrorPosition),
        InvalidElementNamePrefix(AzSvgParseErrorPosition),
        DuplicatedNamespace(AzDuplicatedNamespaceError),
        UnknownNamespace(AzUnknownNamespaceError),
        UnexpectedCloseTag(AzUnexpectedCloseTagError),
        UnexpectedEntityCloseTag(AzSvgParseErrorPosition),
        UnknownEntityReference(AzUnknownEntityReferenceError),
        MalformedEntityReference(AzSvgParseErrorPosition),
        EntityReferenceLoop(AzSvgParseErrorPosition),
        InvalidAttributeValue(AzSvgParseErrorPosition),
        DuplicatedAttribute(AzDuplicatedAttributeError),
        NoRootNode,
        SizeLimit,
        ParserError(AzXmlParseError),
    }
    /// Re-export of rust-allocated (stack based) `DuplicatedNamespaceError` struct
    #[repr(C)] #[derive(Debug)] pub struct AzDuplicatedNamespaceError {
        pub ns: AzString,
        pub pos: AzSvgParseErrorPosition,
    }
    /// Re-export of rust-allocated (stack based) `UnknownNamespaceError` struct
    #[repr(C)] #[derive(Debug)] pub struct AzUnknownNamespaceError {
        pub ns: AzString,
        pub pos: AzSvgParseErrorPosition,
    }
    /// Re-export of rust-allocated (stack based) `UnexpectedCloseTagError` struct
    #[repr(C)] #[derive(Debug)] pub struct AzUnexpectedCloseTagError {
        pub expected: AzString,
        pub actual: AzString,
        pub pos: AzSvgParseErrorPosition,
    }
    /// Re-export of rust-allocated (stack based) `UnknownEntityReferenceError` struct
    #[repr(C)] #[derive(Debug)] pub struct AzUnknownEntityReferenceError {
        pub entity: AzString,
        pub pos: AzSvgParseErrorPosition,
    }
    /// Re-export of rust-allocated (stack based) `DuplicatedAttributeError` struct
    #[repr(C)] #[derive(Debug)] pub struct AzDuplicatedAttributeError {
        pub attribute: AzString,
        pub pos: AzSvgParseErrorPosition,
    }
    /// Re-export of rust-allocated (stack based) `XmlParseError` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzXmlParseError {
        InvalidDeclaration(AzXmlTextError),
        InvalidComment(AzXmlTextError),
        InvalidPI(AzXmlTextError),
        InvalidDoctype(AzXmlTextError),
        InvalidEntity(AzXmlTextError),
        InvalidElement(AzXmlTextError),
        InvalidAttribute(AzXmlTextError),
        InvalidCdata(AzXmlTextError),
        InvalidCharData(AzXmlTextError),
        UnknownToken(AzSvgParseErrorPosition),
    }
    /// Re-export of rust-allocated (stack based) `XmlTextError` struct
    #[repr(C)] #[derive(Debug)] pub struct AzXmlTextError {
        pub stream_error: AzXmlStreamError,
        pub pos: AzSvgParseErrorPosition,
    }
    /// Re-export of rust-allocated (stack based) `XmlStreamError` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzXmlStreamError {
        UnexpectedEndOfStream,
        InvalidName,
        NonXmlChar(AzNonXmlCharError),
        InvalidChar(AzInvalidCharError),
        InvalidCharMultiple(AzInvalidCharMultipleError),
        InvalidQuote(AzInvalidQuoteError),
        InvalidSpace(AzInvalidSpaceError),
        InvalidString(AzInvalidStringError),
        InvalidReference,
        InvalidExternalID,
        InvalidCommentData,
        InvalidCommentEnd,
        InvalidCharacterData,
    }
    /// Re-export of rust-allocated (stack based) `NonXmlCharError` struct
    #[repr(C)] #[derive(Debug)] pub struct AzNonXmlCharError {
        pub ch: char,
        pub pos: AzSvgParseErrorPosition,
    }
    /// Re-export of rust-allocated (stack based) `InvalidCharError` struct
    #[repr(C)] #[derive(Debug)] pub struct AzInvalidCharError {
        pub expected: u8,
        pub got: u8,
        pub pos: AzSvgParseErrorPosition,
    }
    /// Re-export of rust-allocated (stack based) `InvalidCharMultipleError` struct
    #[repr(C)] #[derive(Debug)] pub struct AzInvalidCharMultipleError {
        pub expected: u8,
        pub got: AzU8Vec,
        pub pos: AzSvgParseErrorPosition,
    }
    /// Re-export of rust-allocated (stack based) `InvalidQuoteError` struct
    #[repr(C)] #[derive(Debug)] pub struct AzInvalidQuoteError {
        pub got: u8,
        pub pos: AzSvgParseErrorPosition,
    }
    /// Re-export of rust-allocated (stack based) `InvalidSpaceError` struct
    #[repr(C)] #[derive(Debug)] pub struct AzInvalidSpaceError {
        pub got: u8,
        pub pos: AzSvgParseErrorPosition,
    }
    /// Re-export of rust-allocated (stack based) `InvalidStringError` struct
    #[repr(C)] #[derive(Debug)] pub struct AzInvalidStringError {
        pub got: AzString,
        pub pos: AzSvgParseErrorPosition,
    }
    /// Re-export of rust-allocated (stack based) `SvgParseErrorPosition` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgParseErrorPosition {
        pub row: u32,
        pub col: u32,
    }
    /// Pointer to rust-allocated `Box<Instant>` struct
    #[repr(C)] #[derive(Debug)] pub struct AzInstantPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `Duration` struct
    #[repr(C)] #[derive(Debug)] pub struct AzDuration {
        pub secs: u64,
        pub nanos: u32,
    }
    /// Re-export of rust-allocated (stack based) `AppLogLevel` struct
    #[repr(C)] #[derive(Debug)] pub enum AzAppLogLevel {
        Off,
        Error,
        Warn,
        Info,
        Debug,
        Trace,
    }
    /// Configuration for optional features, such as whether to enable logging or panic hooks
    #[repr(C)] #[derive(Debug)] pub struct AzAppConfig {
        pub log_level: AzAppLogLevel,
        pub enable_visual_panic_hook: bool,
        pub enable_logging_on_panic: bool,
        pub enable_tab_navigation: bool,
        pub renderer_type: AzRendererType,
        pub debug_state: AzDebugState,
    }
    /// Pointer to rust-allocated `Box<App>` struct
    #[repr(C)] #[derive(Debug)] pub struct AzAppPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `NodeId` struct
    #[repr(C)] #[derive(Debug)] pub struct AzNodeId {
        pub inner: usize,
    }
    /// Re-export of rust-allocated (stack based) `DomId` struct
    #[repr(C)] #[derive(Debug)] pub struct AzDomId {
        pub inner: usize,
    }
    /// Re-export of rust-allocated (stack based) `DomNodeId` struct
    #[repr(C)] #[derive(Debug)] pub struct AzDomNodeId {
        pub dom: AzDomId,
        pub node: AzNodeId,
    }
    /// Re-export of rust-allocated (stack based) `HidpiAdjustedBounds` struct
    #[repr(C)] #[derive(Debug)] pub struct AzHidpiAdjustedBounds {
        pub logical_size: AzLogicalSize,
        pub hidpi_factor: f32,
    }
    /// Re-export of rust-allocated (stack based) `LayoutCallback` struct
    #[repr(C)]  pub struct AzLayoutCallback {
        pub cb: AzLayoutCallbackType,
    }
    /// Re-export of rust-allocated (stack based) `Callback` struct
    #[repr(C)]  pub struct AzCallback {
        pub cb: AzCallbackType,
    }
    /// Defines the focus target for the next frame
    #[repr(C, u8)] #[derive(Debug)] pub enum AzFocusTarget {
        Id(AzDomNodeId),
        Path(AzFocusTargetPath),
        PreviousFocusItem,
        NextFocusItem,
        NoFocus,
    }
    /// Re-export of rust-allocated (stack based) `FocusTargetPath` struct
    #[repr(C)] #[derive(Debug)] pub struct AzFocusTargetPath {
        pub dom: AzDomId,
        pub css_path: AzCssPath,
    }
    /// Re-export of rust-allocated (stack based) `CallbackInfo` struct
    #[repr(C)] #[derive(Debug)] pub struct AzCallbackInfo {
        pub current_window_state: *const c_void,
        pub modifiable_window_state: *mut AzWindowState,
        pub gl_context: *const AzGlContextPtr,
        pub resources: *mut c_void,
        pub timers: *mut c_void,
        pub threads: *mut c_void,
        pub new_windows: *mut c_void,
        pub current_window_handle: *const AzRawWindowHandle,
        pub layout_results: *const c_void,
        pub stop_propagation: *mut bool,
        pub focus_target: *const c_void,
        pub current_scroll_states: *const c_void,
        pub css_properties_changed_in_callbacks: *const c_void,
        pub nodes_scrolled_in_callback: *const c_void,
        pub hit_dom_node: AzDomNodeId,
        pub cursor_relative_to_item: AzOptionLayoutPoint,
        pub cursor_in_viewport: AzOptionLayoutPoint,
    }
    /// Specifies if the screen should be updated after the callback function has returned
    #[repr(C)] #[derive(Debug)] pub enum AzUpdateScreen {
        DoNothing,
        RegenerateStyledDomForCurrentWindow,
        RegenerateStyledDomForAllWindows,
    }
    /// Re-export of rust-allocated (stack based) `IFrameCallback` struct
    #[repr(C)]  pub struct AzIFrameCallback {
        pub cb: AzIFrameCallbackType,
    }
    /// Re-export of rust-allocated (stack based) `IFrameCallbackInfo` struct
    #[repr(C)] #[derive(Debug)] pub struct AzIFrameCallbackInfo {
        pub resources: *const c_void,
        pub bounds: AzHidpiAdjustedBounds,
    }
    /// Re-export of rust-allocated (stack based) `IFrameCallbackReturn` struct
    #[repr(C)] #[derive(Debug)] pub struct AzIFrameCallbackReturn {
        pub dom: AzStyledDom,
        pub size: AzLayoutRect,
        pub virtual_size: AzOptionLayoutRect,
    }
    /// Re-export of rust-allocated (stack based) `GlCallback` struct
    #[repr(C)]  pub struct AzGlCallback {
        pub cb: AzGlCallbackType,
    }
    /// Re-export of rust-allocated (stack based) `GlCallbackInfo` struct
    #[repr(C)] #[derive(Debug)] pub struct AzGlCallbackInfo {
        pub gl_context: *const AzGlContextPtr,
        pub resources: *const c_void,
        pub bounds: AzHidpiAdjustedBounds,
    }
    /// Re-export of rust-allocated (stack based) `GlCallbackReturn` struct
    #[repr(C)] #[derive(Debug)] pub struct AzGlCallbackReturn {
        pub texture: AzOptionTexture,
    }
    /// Re-export of rust-allocated (stack based) `TimerCallback` struct
    #[repr(C)]  pub struct AzTimerCallback {
        pub cb: AzTimerCallbackType,
    }
    /// Re-export of rust-allocated (stack based) `TimerCallbackInfo` struct
    #[repr(C)] #[derive(Debug)] pub struct AzTimerCallbackInfo {
        pub callback_info: AzCallbackInfo,
        pub frame_start: AzInstantPtr,
        pub call_count: usize,
    }
    /// Re-export of rust-allocated (stack based) `TimerCallbackReturn` struct
    #[repr(C)] #[derive(Debug)] pub struct AzTimerCallbackReturn {
        pub should_update: AzUpdateScreen,
        pub should_terminate: AzTerminateTimer,
    }
    /// Re-export of rust-allocated (stack based) `WriteBackCallback` struct
    #[repr(C)]  pub struct AzWriteBackCallback {
        pub cb: AzWriteBackCallbackType,
    }
    /// Re-export of rust-allocated (stack based) `AtomicRefCount` struct
    #[repr(C)] #[derive(Debug)] pub struct AzAtomicRefCount {
        pub(crate) ptr: *const c_void,
    }
    /// RefAny is a reference-counted, type-erased pointer, which stores a reference to a struct. `RefAny` can be up- and downcasted (this usually done via generics and can't be expressed in the Rust API)
    #[repr(C)]  pub struct AzRefAny {
        pub _internal_ptr: *const c_void,
        pub _internal_len: usize,
        pub _internal_layout_size: usize,
        pub _internal_layout_align: usize,
        pub type_id: u64,
        pub type_name: AzString,
        pub _sharing_info_ptr: *const AzAtomicRefCount,
        pub custom_destructor: AzRefAnyDestructorType,
    }
    /// Re-export of rust-allocated (stack based) `LayoutInfo` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutInfo {
        pub window_size: *const AzWindowSize,
        pub window_size_width_stops: *mut c_void,
        pub window_size_height_stops: *mut c_void,
        pub resources: *const c_void,
    }
    /// Re-export of rust-allocated (stack based) `CssRuleBlock` struct
    #[repr(C)] #[derive(Debug)] pub struct AzCssRuleBlock {
        pub path: AzCssPath,
        pub declarations: AzCssDeclarationVec,
    }
    /// Re-export of rust-allocated (stack based) `CssDeclaration` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzCssDeclaration {
        Static(AzCssProperty),
        Dynamic(AzDynamicCssProperty),
    }
    /// Re-export of rust-allocated (stack based) `DynamicCssProperty` struct
    #[repr(C)] #[derive(Debug)] pub struct AzDynamicCssProperty {
        pub dynamic_id: AzString,
        pub default_value: AzCssProperty,
    }
    /// Re-export of rust-allocated (stack based) `CssPath` struct
    #[repr(C)] #[derive(Debug)] pub struct AzCssPath {
        pub selectors: AzCssPathSelectorVec,
    }
    /// Re-export of rust-allocated (stack based) `CssPathSelector` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzCssPathSelector {
        Global,
        Type(AzNodeTypePath),
        Class(AzString),
        Id(AzString),
        PseudoSelector(AzCssPathPseudoSelector),
        DirectChildren,
        Children,
    }
    /// Re-export of rust-allocated (stack based) `NodeTypePath` struct
    #[repr(C)] #[derive(Debug)] pub enum AzNodeTypePath {
        Body,
        Div,
        P,
        Img,
        Texture,
        IFrame,
    }
    /// Re-export of rust-allocated (stack based) `CssPathPseudoSelector` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzCssPathPseudoSelector {
        First,
        Last,
        NthChild(AzCssNthChildSelector),
        Hover,
        Active,
        Focus,
    }
    /// Re-export of rust-allocated (stack based) `CssNthChildSelector` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzCssNthChildSelector {
        Number(u32),
        Even,
        Odd,
        Pattern(AzCssNthChildPattern),
    }
    /// Re-export of rust-allocated (stack based) `CssNthChildPattern` struct
    #[repr(C)] #[derive(Debug)] pub struct AzCssNthChildPattern {
        pub repeat: u32,
        pub offset: u32,
    }
    /// Re-export of rust-allocated (stack based) `Stylesheet` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStylesheet {
        pub rules: AzCssRuleBlockVec,
    }
    /// Re-export of rust-allocated (stack based) `Css` struct
    #[repr(C)] #[derive(Debug)] pub struct AzCss {
        pub stylesheets: AzStylesheetVec,
    }
    /// Re-export of rust-allocated (stack based) `CssPropertyType` struct
    #[repr(C)] #[derive(Debug)] pub enum AzCssPropertyType {
        TextColor,
        FontSize,
        FontFamily,
        TextAlign,
        LetterSpacing,
        LineHeight,
        WordSpacing,
        TabWidth,
        Cursor,
        Display,
        Float,
        BoxSizing,
        Width,
        Height,
        MinWidth,
        MinHeight,
        MaxWidth,
        MaxHeight,
        Position,
        Top,
        Right,
        Left,
        Bottom,
        FlexWrap,
        FlexDirection,
        FlexGrow,
        FlexShrink,
        JustifyContent,
        AlignItems,
        AlignContent,
        OverflowX,
        OverflowY,
        PaddingTop,
        PaddingLeft,
        PaddingRight,
        PaddingBottom,
        MarginTop,
        MarginLeft,
        MarginRight,
        MarginBottom,
        Background,
        BackgroundImage,
        BackgroundColor,
        BackgroundPosition,
        BackgroundSize,
        BackgroundRepeat,
        BorderTopLeftRadius,
        BorderTopRightRadius,
        BorderBottomLeftRadius,
        BorderBottomRightRadius,
        BorderTopColor,
        BorderRightColor,
        BorderLeftColor,
        BorderBottomColor,
        BorderTopStyle,
        BorderRightStyle,
        BorderLeftStyle,
        BorderBottomStyle,
        BorderTopWidth,
        BorderRightWidth,
        BorderLeftWidth,
        BorderBottomWidth,
        BoxShadowLeft,
        BoxShadowRight,
        BoxShadowTop,
        BoxShadowBottom,
        Opacity,
        Transform,
        PerspectiveOrigin,
        TransformOrigin,
        BackfaceVisibility,
    }
    /// Re-export of rust-allocated (stack based) `ColorU` struct
    #[repr(C)] #[derive(Debug)] pub struct AzColorU {
        pub r: u8,
        pub g: u8,
        pub b: u8,
        pub a: u8,
    }
    /// Re-export of rust-allocated (stack based) `SizeMetric` struct
    #[repr(C)] #[derive(Debug)] pub enum AzSizeMetric {
        Px,
        Pt,
        Em,
        Percent,
    }
    /// Re-export of rust-allocated (stack based) `FloatValue` struct
    #[repr(C)] #[derive(Debug)] pub struct AzFloatValue {
        pub number: isize,
    }
    /// Re-export of rust-allocated (stack based) `PixelValue` struct
    #[repr(C)] #[derive(Debug)] pub struct AzPixelValue {
        pub metric: AzSizeMetric,
        pub number: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `PixelValueNoPercent` struct
    #[repr(C)] #[derive(Debug)] pub struct AzPixelValueNoPercent {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `BoxShadowClipMode` struct
    #[repr(C)] #[derive(Debug)] pub enum AzBoxShadowClipMode {
        Outset,
        Inset,
    }
    /// Re-export of rust-allocated (stack based) `BoxShadowPreDisplayItem` struct
    #[repr(C)] #[derive(Debug)] pub struct AzBoxShadowPreDisplayItem {
        pub offset: [AzPixelValueNoPercent;2],
        pub color: AzColorU,
        pub blur_radius: AzPixelValueNoPercent,
        pub spread_radius: AzPixelValueNoPercent,
        pub clip_mode: AzBoxShadowClipMode,
    }
    /// Re-export of rust-allocated (stack based) `LayoutAlignContent` struct
    #[repr(C)] #[derive(Debug)] pub enum AzLayoutAlignContent {
        Stretch,
        Center,
        Start,
        End,
        SpaceBetween,
        SpaceAround,
    }
    /// Re-export of rust-allocated (stack based) `LayoutAlignItems` struct
    #[repr(C)] #[derive(Debug)] pub enum AzLayoutAlignItems {
        Stretch,
        Center,
        FlexStart,
        FlexEnd,
    }
    /// Re-export of rust-allocated (stack based) `LayoutBottom` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutBottom {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutBoxSizing` struct
    #[repr(C)] #[derive(Debug)] pub enum AzLayoutBoxSizing {
        ContentBox,
        BorderBox,
    }
    /// Re-export of rust-allocated (stack based) `LayoutFlexDirection` struct
    #[repr(C)] #[derive(Debug)] pub enum AzLayoutFlexDirection {
        Row,
        RowReverse,
        Column,
        ColumnReverse,
    }
    /// Re-export of rust-allocated (stack based) `LayoutDisplay` struct
    #[repr(C)] #[derive(Debug)] pub enum AzLayoutDisplay {
        Flex,
        Block,
        InlineBlock,
    }
    /// Re-export of rust-allocated (stack based) `LayoutFlexGrow` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutFlexGrow {
        pub inner: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutFlexShrink` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutFlexShrink {
        pub inner: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutFloat` struct
    #[repr(C)] #[derive(Debug)] pub enum AzLayoutFloat {
        Left,
        Right,
    }
    /// Re-export of rust-allocated (stack based) `LayoutHeight` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutHeight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutJustifyContent` struct
    #[repr(C)] #[derive(Debug)] pub enum AzLayoutJustifyContent {
        Start,
        End,
        Center,
        SpaceBetween,
        SpaceAround,
        SpaceEvenly,
    }
    /// Re-export of rust-allocated (stack based) `LayoutLeft` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutLeft {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginBottom` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutMarginBottom {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginLeft` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutMarginLeft {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginRight` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutMarginRight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginTop` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutMarginTop {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMaxHeight` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutMaxHeight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMaxWidth` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutMaxWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMinHeight` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutMinHeight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMinWidth` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutMinWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingBottom` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutPaddingBottom {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingLeft` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutPaddingLeft {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingRight` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutPaddingRight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingTop` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutPaddingTop {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPosition` struct
    #[repr(C)] #[derive(Debug)] pub enum AzLayoutPosition {
        Static,
        Relative,
        Absolute,
        Fixed,
    }
    /// Re-export of rust-allocated (stack based) `LayoutRight` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutRight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutTop` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutTop {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutWidth` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutWrap` struct
    #[repr(C)] #[derive(Debug)] pub enum AzLayoutWrap {
        Wrap,
        NoWrap,
    }
    /// Re-export of rust-allocated (stack based) `Overflow` struct
    #[repr(C)] #[derive(Debug)] pub enum AzOverflow {
        Scroll,
        Auto,
        Hidden,
        Visible,
    }
    /// Re-export of rust-allocated (stack based) `PercentageValue` struct
    #[repr(C)] #[derive(Debug)] pub struct AzPercentageValue {
        pub number: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `GradientStopPre` struct
    #[repr(C)] #[derive(Debug)] pub struct AzGradientStopPre {
        pub offset: AzOptionPercentageValue,
        pub color: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `DirectionCorner` struct
    #[repr(C)] #[derive(Debug)] pub enum AzDirectionCorner {
        Right,
        Left,
        Top,
        Bottom,
        TopRight,
        TopLeft,
        BottomRight,
        BottomLeft,
    }
    /// Re-export of rust-allocated (stack based) `DirectionCorners` struct
    #[repr(C)] #[derive(Debug)] pub struct AzDirectionCorners {
        pub from: AzDirectionCorner,
        pub to: AzDirectionCorner,
    }
    /// Re-export of rust-allocated (stack based) `Direction` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzDirection {
        Angle(AzFloatValue),
        FromTo(AzDirectionCorners),
    }
    /// Re-export of rust-allocated (stack based) `ExtendMode` struct
    #[repr(C)] #[derive(Debug)] pub enum AzExtendMode {
        Clamp,
        Repeat,
    }
    /// Re-export of rust-allocated (stack based) `LinearGradient` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLinearGradient {
        pub direction: AzDirection,
        pub extend_mode: AzExtendMode,
        pub stops: AzGradientStopPreVec,
    }
    /// Re-export of rust-allocated (stack based) `Shape` struct
    #[repr(C)] #[derive(Debug)] pub enum AzShape {
        Ellipse,
        Circle,
    }
    /// Re-export of rust-allocated (stack based) `RadialGradient` struct
    #[repr(C)] #[derive(Debug)] pub struct AzRadialGradient {
        pub shape: AzShape,
        pub extend_mode: AzExtendMode,
        pub stops: AzGradientStopPreVec,
    }
    /// Re-export of rust-allocated (stack based) `CssImageId` struct
    #[repr(C)] #[derive(Debug)] pub struct AzCssImageId {
        pub inner: AzString,
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundContent` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBackgroundContent {
        LinearGradient(AzLinearGradient),
        RadialGradient(AzRadialGradient),
        Image(AzCssImageId),
        Color(AzColorU),
    }
    /// Re-export of rust-allocated (stack based) `BackgroundPositionHorizontal` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzBackgroundPositionHorizontal {
        Left,
        Center,
        Right,
        Exact(AzPixelValue),
    }
    /// Re-export of rust-allocated (stack based) `BackgroundPositionVertical` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzBackgroundPositionVertical {
        Top,
        Center,
        Bottom,
        Exact(AzPixelValue),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundPosition` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBackgroundPosition {
        pub horizontal: AzBackgroundPositionHorizontal,
        pub vertical: AzBackgroundPositionVertical,
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundRepeat` struct
    #[repr(C)] #[derive(Debug)] pub enum AzStyleBackgroundRepeat {
        NoRepeat,
        Repeat,
        RepeatX,
        RepeatY,
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundSize` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBackgroundSize {
        ExactSize([AzPixelValue;2]),
        Contain,
        Cover,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomColor` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderBottomColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomLeftRadius` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderBottomLeftRadius {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomRightRadius` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderBottomRightRadius {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `BorderStyle` struct
    #[repr(C)] #[derive(Debug)] pub enum AzBorderStyle {
        None,
        Solid,
        Double,
        Dotted,
        Dashed,
        Hidden,
        Groove,
        Ridge,
        Inset,
        Outset,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomStyle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderBottomStyle {
        pub inner: AzBorderStyle,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomWidth` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderBottomWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftColor` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderLeftColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftStyle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderLeftStyle {
        pub inner: AzBorderStyle,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftWidth` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderLeftWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightColor` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderRightColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightStyle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderRightStyle {
        pub inner: AzBorderStyle,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightWidth` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderRightWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopColor` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderTopColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopLeftRadius` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderTopLeftRadius {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopRightRadius` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderTopRightRadius {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopStyle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderTopStyle {
        pub inner: AzBorderStyle,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopWidth` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBorderTopWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleCursor` struct
    #[repr(C)] #[derive(Debug)] pub enum AzStyleCursor {
        Alias,
        AllScroll,
        Cell,
        ColResize,
        ContextMenu,
        Copy,
        Crosshair,
        Default,
        EResize,
        EwResize,
        Grab,
        Grabbing,
        Help,
        Move,
        NResize,
        NsResize,
        NeswResize,
        NwseResize,
        Pointer,
        Progress,
        RowResize,
        SResize,
        SeResize,
        Text,
        Unset,
        VerticalText,
        WResize,
        Wait,
        ZoomIn,
        ZoomOut,
    }
    /// Re-export of rust-allocated (stack based) `StyleFontFamily` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleFontFamily {
        pub fonts: AzStringVec,
    }
    /// Re-export of rust-allocated (stack based) `StyleFontSize` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleFontSize {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleLetterSpacing` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleLetterSpacing {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleLineHeight` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleLineHeight {
        pub inner: AzPercentageValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleTabWidth` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleTabWidth {
        pub inner: AzPercentageValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleOpacity` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleOpacity {
        pub inner: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleTransformOrigin` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleTransformOrigin {
        pub x: AzPixelValue,
        pub y: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StylePerspectiveOrigin` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStylePerspectiveOrigin {
        pub x: AzPixelValue,
        pub y: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBackfaceVisibility` struct
    #[repr(C)] #[derive(Debug)] pub enum AzStyleBackfaceVisibility {
        Hidden,
        Visible,
    }
    /// Re-export of rust-allocated (stack based) `StyleTransform` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleTransform {
        Matrix(AzStyleTransformMatrix2D),
        Matrix3D(AzStyleTransformMatrix3D),
        Translate(AzStyleTransformTranslate2D),
        Translate3D(AzStyleTransformTranslate3D),
        TranslateX(AzPixelValue),
        TranslateY(AzPixelValue),
        TranslateZ(AzPixelValue),
        Rotate(AzPercentageValue),
        Rotate3D(AzStyleTransformRotate3D),
        RotateX(AzPercentageValue),
        RotateY(AzPercentageValue),
        RotateZ(AzPercentageValue),
        Scale(AzStyleTransformScale2D),
        Scale3D(AzStyleTransformScale3D),
        ScaleX(AzPercentageValue),
        ScaleY(AzPercentageValue),
        ScaleZ(AzPercentageValue),
        Skew(AzStyleTransformSkew2D),
        SkewX(AzPercentageValue),
        SkewY(AzPercentageValue),
        Perspective(AzPixelValue),
    }
    /// Re-export of rust-allocated (stack based) `StyleTransformMatrix2D` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleTransformMatrix2D {
        pub a: AzPixelValue,
        pub b: AzPixelValue,
        pub c: AzPixelValue,
        pub d: AzPixelValue,
        pub tx: AzPixelValue,
        pub ty: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleTransformMatrix3D` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleTransformMatrix3D {
        pub m11: AzPixelValue,
        pub m12: AzPixelValue,
        pub m13: AzPixelValue,
        pub m14: AzPixelValue,
        pub m21: AzPixelValue,
        pub m22: AzPixelValue,
        pub m23: AzPixelValue,
        pub m24: AzPixelValue,
        pub m31: AzPixelValue,
        pub m32: AzPixelValue,
        pub m33: AzPixelValue,
        pub m34: AzPixelValue,
        pub m41: AzPixelValue,
        pub m42: AzPixelValue,
        pub m43: AzPixelValue,
        pub m44: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleTransformTranslate2D` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleTransformTranslate2D {
        pub x: AzPixelValue,
        pub y: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleTransformTranslate3D` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleTransformTranslate3D {
        pub x: AzPixelValue,
        pub y: AzPixelValue,
        pub z: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleTransformRotate3D` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleTransformRotate3D {
        pub x: AzPercentageValue,
        pub y: AzPercentageValue,
        pub z: AzPercentageValue,
        pub angle: AzPercentageValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleTransformScale2D` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleTransformScale2D {
        pub x: AzPercentageValue,
        pub y: AzPercentageValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleTransformScale3D` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleTransformScale3D {
        pub x: AzPercentageValue,
        pub y: AzPercentageValue,
        pub z: AzPercentageValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleTransformSkew2D` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleTransformSkew2D {
        pub x: AzPercentageValue,
        pub y: AzPercentageValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleTextAlignmentHorz` struct
    #[repr(C)] #[derive(Debug)] pub enum AzStyleTextAlignmentHorz {
        Left,
        Center,
        Right,
    }
    /// Re-export of rust-allocated (stack based) `StyleTextColor` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleTextColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleWordSpacing` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleWordSpacing {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `BoxShadowPreDisplayItemValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzBoxShadowPreDisplayItemValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzBoxShadowPreDisplayItem),
    }
    /// Re-export of rust-allocated (stack based) `LayoutAlignContentValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutAlignContentValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutAlignContent),
    }
    /// Re-export of rust-allocated (stack based) `LayoutAlignItemsValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutAlignItemsValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutAlignItems),
    }
    /// Re-export of rust-allocated (stack based) `LayoutBottomValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBottom),
    }
    /// Re-export of rust-allocated (stack based) `LayoutBoxSizingValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutBoxSizingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBoxSizing),
    }
    /// Re-export of rust-allocated (stack based) `LayoutFlexDirectionValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutFlexDirectionValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFlexDirection),
    }
    /// Re-export of rust-allocated (stack based) `LayoutDisplayValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutDisplayValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutDisplay),
    }
    /// Re-export of rust-allocated (stack based) `LayoutFlexGrowValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutFlexGrowValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFlexGrow),
    }
    /// Re-export of rust-allocated (stack based) `LayoutFlexShrinkValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutFlexShrinkValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFlexShrink),
    }
    /// Re-export of rust-allocated (stack based) `LayoutFloatValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutFloatValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFloat),
    }
    /// Re-export of rust-allocated (stack based) `LayoutHeightValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutHeight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutJustifyContentValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutJustifyContentValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutJustifyContent),
    }
    /// Re-export of rust-allocated (stack based) `LayoutLeftValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutLeft),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginBottomValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutMarginBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginBottom),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginLeftValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutMarginLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginLeft),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginRightValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutMarginRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginRight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginTopValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutMarginTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginTop),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMaxHeightValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutMaxHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMaxHeight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMaxWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutMaxWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMaxWidth),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMinHeightValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutMinHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMinHeight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMinWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutMinWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMinWidth),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingBottomValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutPaddingBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingBottom),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingLeftValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutPaddingLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingLeft),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingRightValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutPaddingRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingRight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingTopValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutPaddingTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingTop),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPositionValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutPositionValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPosition),
    }
    /// Re-export of rust-allocated (stack based) `LayoutRightValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutRight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutTopValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutTop),
    }
    /// Re-export of rust-allocated (stack based) `LayoutWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutWidth),
    }
    /// Re-export of rust-allocated (stack based) `LayoutWrapValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutWrapValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutWrap),
    }
    /// Re-export of rust-allocated (stack based) `OverflowValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOverflowValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzOverflow),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundContentValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBackgroundContentValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundContent),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundPositionValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBackgroundPositionValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundPosition),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundRepeatValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBackgroundRepeatValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundRepeat),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundSizeValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBackgroundSizeValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundSize),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomColorValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderBottomColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomLeftRadiusValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderBottomLeftRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomLeftRadius),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomRightRadiusValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderBottomRightRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomRightRadius),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomStyleValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderBottomStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomStyle),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderBottomWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftColorValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderLeftColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderLeftColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftStyleValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderLeftStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderLeftStyle),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderLeftWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderLeftWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightColorValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderRightColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderRightColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightStyleValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderRightStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderRightStyle),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderRightWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderRightWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopColorValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderTopColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopLeftRadiusValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderTopLeftRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopLeftRadius),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopRightRadiusValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderTopRightRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopRightRadius),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopStyleValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderTopStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopStyle),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBorderTopWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleCursorValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleCursorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleCursor),
    }
    /// Re-export of rust-allocated (stack based) `StyleFontFamilyValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleFontFamilyValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleFontFamily),
    }
    /// Re-export of rust-allocated (stack based) `StyleFontSizeValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleFontSizeValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleFontSize),
    }
    /// Re-export of rust-allocated (stack based) `StyleLetterSpacingValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleLetterSpacingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleLetterSpacing),
    }
    /// Re-export of rust-allocated (stack based) `StyleLineHeightValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleLineHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleLineHeight),
    }
    /// Re-export of rust-allocated (stack based) `StyleTabWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleTabWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTabWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleTextAlignmentHorzValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleTextAlignmentHorzValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTextAlignmentHorz),
    }
    /// Re-export of rust-allocated (stack based) `StyleTextColorValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleTextColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTextColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleWordSpacingValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleWordSpacingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleWordSpacing),
    }
    /// Re-export of rust-allocated (stack based) `StyleOpacityValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleOpacityValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleOpacity),
    }
    /// Re-export of rust-allocated (stack based) `StyleTransformVecValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleTransformVecValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTransformVec),
    }
    /// Re-export of rust-allocated (stack based) `StyleTransformOriginValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleTransformOriginValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTransformOrigin),
    }
    /// Re-export of rust-allocated (stack based) `StylePerspectiveOriginValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStylePerspectiveOriginValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStylePerspectiveOrigin),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackfaceVisibilityValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBackfaceVisibilityValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackfaceVisibility),
    }
    /// Parsed CSS key-value pair
    #[repr(C, u8)] #[derive(Debug)] pub enum AzCssProperty {
        TextColor(AzStyleTextColorValue),
        FontSize(AzStyleFontSizeValue),
        FontFamily(AzStyleFontFamilyValue),
        TextAlign(AzStyleTextAlignmentHorzValue),
        LetterSpacing(AzStyleLetterSpacingValue),
        LineHeight(AzStyleLineHeightValue),
        WordSpacing(AzStyleWordSpacingValue),
        TabWidth(AzStyleTabWidthValue),
        Cursor(AzStyleCursorValue),
        Display(AzLayoutDisplayValue),
        Float(AzLayoutFloatValue),
        BoxSizing(AzLayoutBoxSizingValue),
        Width(AzLayoutWidthValue),
        Height(AzLayoutHeightValue),
        MinWidth(AzLayoutMinWidthValue),
        MinHeight(AzLayoutMinHeightValue),
        MaxWidth(AzLayoutMaxWidthValue),
        MaxHeight(AzLayoutMaxHeightValue),
        Position(AzLayoutPositionValue),
        Top(AzLayoutTopValue),
        Right(AzLayoutRightValue),
        Left(AzLayoutLeftValue),
        Bottom(AzLayoutBottomValue),
        FlexWrap(AzLayoutWrapValue),
        FlexDirection(AzLayoutFlexDirectionValue),
        FlexGrow(AzLayoutFlexGrowValue),
        FlexShrink(AzLayoutFlexShrinkValue),
        JustifyContent(AzLayoutJustifyContentValue),
        AlignItems(AzLayoutAlignItemsValue),
        AlignContent(AzLayoutAlignContentValue),
        BackgroundContent(AzStyleBackgroundContentValue),
        BackgroundPosition(AzStyleBackgroundPositionValue),
        BackgroundSize(AzStyleBackgroundSizeValue),
        BackgroundRepeat(AzStyleBackgroundRepeatValue),
        OverflowX(AzOverflowValue),
        OverflowY(AzOverflowValue),
        PaddingTop(AzLayoutPaddingTopValue),
        PaddingLeft(AzLayoutPaddingLeftValue),
        PaddingRight(AzLayoutPaddingRightValue),
        PaddingBottom(AzLayoutPaddingBottomValue),
        MarginTop(AzLayoutMarginTopValue),
        MarginLeft(AzLayoutMarginLeftValue),
        MarginRight(AzLayoutMarginRightValue),
        MarginBottom(AzLayoutMarginBottomValue),
        BorderTopLeftRadius(AzStyleBorderTopLeftRadiusValue),
        BorderTopRightRadius(AzStyleBorderTopRightRadiusValue),
        BorderBottomLeftRadius(AzStyleBorderBottomLeftRadiusValue),
        BorderBottomRightRadius(AzStyleBorderBottomRightRadiusValue),
        BorderTopColor(AzStyleBorderTopColorValue),
        BorderRightColor(AzStyleBorderRightColorValue),
        BorderLeftColor(AzStyleBorderLeftColorValue),
        BorderBottomColor(AzStyleBorderBottomColorValue),
        BorderTopStyle(AzStyleBorderTopStyleValue),
        BorderRightStyle(AzStyleBorderRightStyleValue),
        BorderLeftStyle(AzStyleBorderLeftStyleValue),
        BorderBottomStyle(AzStyleBorderBottomStyleValue),
        BorderTopWidth(AzStyleBorderTopWidthValue),
        BorderRightWidth(AzStyleBorderRightWidthValue),
        BorderLeftWidth(AzStyleBorderLeftWidthValue),
        BorderBottomWidth(AzStyleBorderBottomWidthValue),
        BoxShadowLeft(AzBoxShadowPreDisplayItemValue),
        BoxShadowRight(AzBoxShadowPreDisplayItemValue),
        BoxShadowTop(AzBoxShadowPreDisplayItemValue),
        BoxShadowBottom(AzBoxShadowPreDisplayItemValue),
        Opacity(AzStyleOpacityValue),
        Transform(AzStyleTransformVecValue),
        TransformOrigin(AzStyleTransformOriginValue),
        PerspectiveOrigin(AzStylePerspectiveOriginValue),
        BackfaceVisibility(AzStyleBackfaceVisibilityValue),
    }
    /// Re-export of rust-allocated (stack based) `Node` struct
    #[repr(C)] #[derive(Debug)] pub struct AzNode {
        pub parent: usize,
        pub previous_sibling: usize,
        pub next_sibling: usize,
        pub first_child: usize,
        pub last_child: usize,
    }
    /// Re-export of rust-allocated (stack based) `CascadeInfo` struct
    #[repr(C)] #[derive(Debug)] pub struct AzCascadeInfo {
        pub index_in_parent: u32,
        pub is_last_child: bool,
    }
    /// Re-export of rust-allocated (stack based) `RectStyle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzRectStyle {
        pub background: AzOptionStyleBackgroundContentValue,
        pub background_position: AzOptionStyleBackgroundPositionValue,
        pub background_size: AzOptionStyleBackgroundSizeValue,
        pub background_repeat: AzOptionStyleBackgroundRepeatValue,
        pub font_size: AzOptionStyleFontSizeValue,
        pub font_family: AzOptionStyleFontFamilyValue,
        pub text_color: AzOptionStyleTextColorValue,
        pub text_align: AzOptionStyleTextAlignmentHorzValue,
        pub line_height: AzOptionStyleLineHeightValue,
        pub letter_spacing: AzOptionStyleLetterSpacingValue,
        pub word_spacing: AzOptionStyleWordSpacingValue,
        pub tab_width: AzOptionStyleTabWidthValue,
        pub cursor: AzOptionStyleCursorValue,
        pub box_shadow_left: AzOptionBoxShadowPreDisplayItemValue,
        pub box_shadow_right: AzOptionBoxShadowPreDisplayItemValue,
        pub box_shadow_top: AzOptionBoxShadowPreDisplayItemValue,
        pub box_shadow_bottom: AzOptionBoxShadowPreDisplayItemValue,
        pub border_top_color: AzOptionStyleBorderTopColorValue,
        pub border_left_color: AzOptionStyleBorderLeftColorValue,
        pub border_right_color: AzOptionStyleBorderRightColorValue,
        pub border_bottom_color: AzOptionStyleBorderBottomColorValue,
        pub border_top_style: AzOptionStyleBorderTopStyleValue,
        pub border_left_style: AzOptionStyleBorderLeftStyleValue,
        pub border_right_style: AzOptionStyleBorderRightStyleValue,
        pub border_bottom_style: AzOptionStyleBorderBottomStyleValue,
        pub border_top_left_radius: AzOptionStyleBorderTopLeftRadiusValue,
        pub border_top_right_radius: AzOptionStyleBorderTopRightRadiusValue,
        pub border_bottom_left_radius: AzOptionStyleBorderBottomLeftRadiusValue,
        pub border_bottom_right_radius: AzOptionStyleBorderBottomRightRadiusValue,
        pub opacity: AzOptionStyleOpacityValue,
        pub transform: AzOptionStyleTransformVecValue,
        pub transform_origin: AzOptionStyleTransformOriginValue,
        pub perspective_origin: AzOptionStylePerspectiveOriginValue,
        pub backface_visibility: AzOptionStyleBackfaceVisibilityValue,
    }
    /// Re-export of rust-allocated (stack based) `RectLayout` struct
    #[repr(C)] #[derive(Debug)] pub struct AzRectLayout {
        pub display: AzOptionLayoutDisplayValue,
        pub float: AzOptionLayoutFloatValue,
        pub box_sizing: AzOptionLayoutBoxSizingValue,
        pub width: AzOptionLayoutWidthValue,
        pub height: AzOptionLayoutHeightValue,
        pub min_width: AzOptionLayoutMinWidthValue,
        pub min_height: AzOptionLayoutMinHeightValue,
        pub max_width: AzOptionLayoutMaxWidthValue,
        pub max_height: AzOptionLayoutMaxHeightValue,
        pub position: AzOptionLayoutPositionValue,
        pub top: AzOptionLayoutTopValue,
        pub bottom: AzOptionLayoutBottomValue,
        pub right: AzOptionLayoutRightValue,
        pub left: AzOptionLayoutLeftValue,
        pub padding_top: AzOptionLayoutPaddingTopValue,
        pub padding_bottom: AzOptionLayoutPaddingBottomValue,
        pub padding_left: AzOptionLayoutPaddingLeftValue,
        pub padding_right: AzOptionLayoutPaddingRightValue,
        pub margin_top: AzOptionLayoutMarginTopValue,
        pub margin_bottom: AzOptionLayoutMarginBottomValue,
        pub margin_left: AzOptionLayoutMarginLeftValue,
        pub margin_right: AzOptionLayoutMarginRightValue,
        pub border_top_width: AzOptionStyleBorderTopWidthValue,
        pub border_left_width: AzOptionStyleBorderLeftWidthValue,
        pub border_right_width: AzOptionStyleBorderRightWidthValue,
        pub border_bottom_width: AzOptionStyleBorderBottomWidthValue,
        pub overflow_x: AzOptionOverflowValue,
        pub overflow_y: AzOptionOverflowValue,
        pub direction: AzOptionLayoutFlexDirectionValue,
        pub wrap: AzOptionLayoutWrapValue,
        pub flex_grow: AzOptionLayoutFlexGrowValue,
        pub flex_shrink: AzOptionLayoutFlexShrinkValue,
        pub justify_content: AzOptionLayoutJustifyContentValue,
        pub align_items: AzOptionLayoutAlignItemsValue,
        pub align_content: AzOptionLayoutAlignContentValue,
    }
    /// Re-export of rust-allocated (stack based) `CascadedCssPropertyWithSource` struct
    #[repr(C)] #[derive(Debug)] pub struct AzCascadedCssPropertyWithSource {
        pub prop: AzCssProperty,
        pub source: AzCssPropertySource,
    }
    /// Re-export of rust-allocated (stack based) `CssPropertySource` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzCssPropertySource {
        Css(AzCssPath),
        Inline,
    }
    /// Re-export of rust-allocated (stack based) `StyledNodeState` struct
    #[repr(C)] #[derive(Debug)] pub enum AzStyledNodeState {
        Uninitialized,
        Normal,
        Hover,
        Active,
        Focused,
    }
    /// Re-export of rust-allocated (stack based) `StyledNode` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyledNode {
        pub css_constraints: AzCascadedCssPropertyWithSourceVec,
        pub hover_css_constraints: AzCascadedCssPropertyWithSourceVec,
        pub active_css_constraints: AzCascadedCssPropertyWithSourceVec,
        pub focus_css_constraints: AzCascadedCssPropertyWithSourceVec,
        pub state: AzStyledNodeState,
        pub tag_id: AzOptionTagId,
        pub style: AzRectStyle,
        pub layout: AzRectLayout,
    }
    /// Re-export of rust-allocated (stack based) `TagId` struct
    #[repr(C)] #[derive(Debug)] pub struct AzTagId {
        pub inner: u64,
    }
    /// Re-export of rust-allocated (stack based) `TagIdToNodeIdMapping` struct
    #[repr(C)] #[derive(Debug)] pub struct AzTagIdToNodeIdMapping {
        pub tag_id: AzTagId,
        pub node_id: AzNodeId,
        pub tab_index: AzOptionTabIndex,
    }
    /// Re-export of rust-allocated (stack based) `ParentWithNodeDepth` struct
    #[repr(C)] #[derive(Debug)] pub struct AzParentWithNodeDepth {
        pub depth: usize,
        pub node_id: AzNodeId,
    }
    /// Re-export of rust-allocated (stack based) `ContentGroup` struct
    #[repr(C)] #[derive(Debug)] pub struct AzContentGroup {
        pub root: AzNodeId,
        pub children: AzContentGroupVec,
    }
    /// Re-export of rust-allocated (stack based) `StyledDom` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyledDom {
        pub root: AzNodeId,
        pub node_hierarchy: AzNodeVec,
        pub node_data: AzNodeDataVec,
        pub styled_nodes: AzStyledNodeVec,
        pub cascade_info: AzCascadeInfoVec,
        pub tag_ids_to_node_ids: AzTagIdsToNodeIdsMappingVec,
        pub non_leaf_nodes: AzParentWithNodeDepthVec,
        pub rects_in_rendering_order: AzContentGroup,
    }
    /// Re-export of rust-allocated (stack based) `Dom` struct
    #[repr(C)] #[derive(Debug)] pub struct AzDom {
        pub root: AzNodeData,
        pub children: AzDomVec,
        pub estimated_total_children: usize,
    }
    /// Re-export of rust-allocated (stack based) `GlTextureNode` struct
    #[repr(C)] #[derive(Debug)] pub struct AzGlTextureNode {
        pub callback: AzGlCallback,
        pub data: AzRefAny,
    }
    /// Re-export of rust-allocated (stack based) `IFrameNode` struct
    #[repr(C)] #[derive(Debug)] pub struct AzIFrameNode {
        pub callback: AzIFrameCallback,
        pub data: AzRefAny,
    }
    /// Re-export of rust-allocated (stack based) `CallbackData` struct
    #[repr(C)] #[derive(Debug)] pub struct AzCallbackData {
        pub event: AzEventFilter,
        pub callback: AzCallback,
        pub data: AzRefAny,
    }
    /// Re-export of rust-allocated (stack based) `ImageMask` struct
    #[repr(C)] #[derive(Debug)] pub struct AzImageMask {
        pub image: AzImageId,
        pub rect: AzLogicalRect,
        pub repeat: bool,
    }
    /// Represents one single DOM node (node type, classes, ids and callbacks are stored here)
    #[repr(C)] #[derive(Debug)] pub struct AzNodeData {
        pub node_type: AzNodeType,
        pub dataset: AzOptionRefAny,
        pub ids: AzStringVec,
        pub classes: AzStringVec,
        pub callbacks: AzCallbackDataVec,
        pub inline_css_props: AzCssPropertyVec,
        pub inline_hover_css_props: AzCssPropertyVec,
        pub inline_active_css_props: AzCssPropertyVec,
        pub inline_focus_css_props: AzCssPropertyVec,
        pub clip_mask: AzOptionImageMask,
        pub is_draggable: bool,
        pub tab_index: AzOptionTabIndex,
    }
    /// List of core DOM node types built-into by `azul`
    #[repr(C, u8)] #[derive(Debug)] pub enum AzNodeType {
        Div,
        Body,
        Label(AzString),
        Text(AzTextId),
        Image(AzImageId),
        GlTexture(AzGlTextureNode),
        IFrame(AzIFrameNode),
    }
    /// When to call a callback action - `On::MouseOver`, `On::MouseOut`, etc.
    #[repr(C)] #[derive(Debug)] pub enum AzOn {
        MouseOver,
        MouseDown,
        LeftMouseDown,
        MiddleMouseDown,
        RightMouseDown,
        MouseUp,
        LeftMouseUp,
        MiddleMouseUp,
        RightMouseUp,
        MouseEnter,
        MouseLeave,
        Scroll,
        TextInput,
        VirtualKeyDown,
        VirtualKeyUp,
        HoveredFile,
        DroppedFile,
        HoveredFileCancelled,
        FocusReceived,
        FocusLost,
    }
    /// Re-export of rust-allocated (stack based) `EventFilter` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzEventFilter {
        Hover(AzHoverEventFilter),
        Not(AzNotEventFilter),
        Focus(AzFocusEventFilter),
        Window(AzWindowEventFilter),
        Component(AzComponentEventFilter),
        Application(AzApplicationEventFilter),
    }
    /// Re-export of rust-allocated (stack based) `HoverEventFilter` struct
    #[repr(C)] #[derive(Debug)] pub enum AzHoverEventFilter {
        MouseOver,
        MouseDown,
        LeftMouseDown,
        RightMouseDown,
        MiddleMouseDown,
        MouseUp,
        LeftMouseUp,
        RightMouseUp,
        MiddleMouseUp,
        MouseEnter,
        MouseLeave,
        Scroll,
        ScrollStart,
        ScrollEnd,
        TextInput,
        VirtualKeyDown,
        VirtualKeyUp,
        HoveredFile,
        DroppedFile,
        HoveredFileCancelled,
        TouchStart,
        TouchMove,
        TouchEnd,
        TouchCancel,
    }
    /// Re-export of rust-allocated (stack based) `FocusEventFilter` struct
    #[repr(C)] #[derive(Debug)] pub enum AzFocusEventFilter {
        MouseOver,
        MouseDown,
        LeftMouseDown,
        RightMouseDown,
        MiddleMouseDown,
        MouseUp,
        LeftMouseUp,
        RightMouseUp,
        MiddleMouseUp,
        MouseEnter,
        MouseLeave,
        Scroll,
        ScrollStart,
        ScrollEnd,
        TextInput,
        VirtualKeyDown,
        VirtualKeyUp,
        FocusReceived,
        FocusLost,
    }
    /// Re-export of rust-allocated (stack based) `NotEventFilter` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzNotEventFilter {
        Hover(AzHoverEventFilter),
        Focus(AzFocusEventFilter),
    }
    /// Re-export of rust-allocated (stack based) `WindowEventFilter` struct
    #[repr(C)] #[derive(Debug)] pub enum AzWindowEventFilter {
        MouseOver,
        MouseDown,
        LeftMouseDown,
        RightMouseDown,
        MiddleMouseDown,
        MouseUp,
        LeftMouseUp,
        RightMouseUp,
        MiddleMouseUp,
        MouseEnter,
        MouseLeave,
        Scroll,
        ScrollStart,
        ScrollEnd,
        TextInput,
        VirtualKeyDown,
        VirtualKeyUp,
        HoveredFile,
        DroppedFile,
        HoveredFileCancelled,
        Resized,
        Moved,
        TouchStart,
        TouchMove,
        TouchEnd,
        TouchCancel,
        FocusReceived,
        FocusLost,
        CloseRequested,
        ThemeChanged,
    }
    /// Re-export of rust-allocated (stack based) `ComponentEventFilter` struct
    #[repr(C)] #[derive(Debug)] pub enum AzComponentEventFilter {
        AfterMount,
        BeforeUnmount,
        NodeResized,
    }
    /// Re-export of rust-allocated (stack based) `ApplicationEventFilter` struct
    #[repr(C)] #[derive(Debug)] pub enum AzApplicationEventFilter {
        DeviceConnected,
        DeviceDisconnected,
    }
    /// Re-export of rust-allocated (stack based) `TabIndex` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzTabIndex {
        Auto,
        OverrideInParent(u32),
        NoKeyboardFocus,
    }
    /// Re-export of rust-allocated (stack based) `GlShaderPrecisionFormatReturn` struct
    #[repr(C)] #[derive(Debug)] pub struct AzGlShaderPrecisionFormatReturn {
        pub _0: i32,
        pub _1: i32,
        pub _2: i32,
    }
    /// Re-export of rust-allocated (stack based) `VertexAttributeType` struct
    #[repr(C)] #[derive(Debug)] pub enum AzVertexAttributeType {
        Float,
        Double,
        UnsignedByte,
        UnsignedShort,
        UnsignedInt,
    }
    /// Re-export of rust-allocated (stack based) `VertexAttribute` struct
    #[repr(C)] #[derive(Debug)] pub struct AzVertexAttribute {
        pub name: AzString,
        pub layout_location: AzOptionUsize,
        pub attribute_type: AzVertexAttributeType,
        pub item_count: usize,
    }
    /// Re-export of rust-allocated (stack based) `VertexLayout` struct
    #[repr(C)] #[derive(Debug)] pub struct AzVertexLayout {
        pub fields: AzVertexAttributeVec,
    }
    /// Re-export of rust-allocated (stack based) `VertexArrayObject` struct
    #[repr(C)] #[derive(Debug)] pub struct AzVertexArrayObject {
        pub vertex_layout: AzVertexLayout,
        pub vao_id: u32,
        pub gl_context: AzGlContextPtr,
    }
    /// Re-export of rust-allocated (stack based) `IndexBufferFormat` struct
    #[repr(C)] #[derive(Debug)] pub enum AzIndexBufferFormat {
        Points,
        Lines,
        LineStrip,
        Triangles,
        TriangleStrip,
        TriangleFan,
    }
    /// Re-export of rust-allocated (stack based) `VertexBuffer` struct
    #[repr(C)] #[derive(Debug)] pub struct AzVertexBuffer {
        pub vertex_buffer_id: u32,
        pub vertex_buffer_len: usize,
        pub vao: AzVertexArrayObject,
        pub index_buffer_id: u32,
        pub index_buffer_len: usize,
        pub index_buffer_format: AzIndexBufferFormat,
    }
    /// Re-export of rust-allocated (stack based) `GlType` struct
    #[repr(C)] #[derive(Debug)] pub enum AzGlType {
        Gl,
        Gles,
    }
    /// Re-export of rust-allocated (stack based) `DebugMessage` struct
    #[repr(C)] #[derive(Debug)] pub struct AzDebugMessage {
        pub message: AzString,
        pub source: u32,
        pub ty: u32,
        pub id: u32,
        pub severity: u32,
    }
    /// C-ABI stable reexport of `&[u8]`
    #[repr(C)] #[derive(Debug)] pub struct AzU8VecRef {
        pub(crate) ptr: *const u8,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [u8]`
    #[repr(C)] #[derive(Debug)] pub struct AzU8VecRefMut {
        pub(crate) ptr: *mut u8,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[f32]`
    #[repr(C)] #[derive(Debug)] pub struct AzF32VecRef {
        pub(crate) ptr: *const f32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[i32]`
    #[repr(C)] #[derive(Debug)] pub struct AzI32VecRef {
        pub(crate) ptr: *const i32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[GLuint]` aka `&[u32]`
    #[repr(C)] #[derive(Debug)] pub struct AzGLuintVecRef {
        pub(crate) ptr: *const u32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[GLenum]` aka `&[u32]`
    #[repr(C)] #[derive(Debug)] pub struct AzGLenumVecRef {
        pub(crate) ptr: *const u32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [GLint]` aka `&mut [i32]`
    #[repr(C)] #[derive(Debug)] pub struct AzGLintVecRefMut {
        pub(crate) ptr: *mut i32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [GLint64]` aka `&mut [i64]`
    #[repr(C)] #[derive(Debug)] pub struct AzGLint64VecRefMut {
        pub(crate) ptr: *mut i64,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [GLboolean]` aka `&mut [u8]`
    #[repr(C)] #[derive(Debug)] pub struct AzGLbooleanVecRefMut {
        pub(crate) ptr: *mut u8,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [GLfloat]` aka `&mut [f32]`
    #[repr(C)] #[derive(Debug)] pub struct AzGLfloatVecRefMut {
        pub(crate) ptr: *mut f32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[Refstr]` aka `&mut [&str]`
    #[repr(C)] #[derive(Debug)] pub struct AzRefstrVecRef {
        pub(crate) ptr: *const AzRefstr,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&str`
    #[repr(C)] #[derive(Debug)] pub struct AzRefstr {
        pub(crate) ptr: *const u8,
        pub len: usize,
    }
    /// C-ABI stable reexport of `(U8Vec, u32)`
    #[repr(C)] #[derive(Debug)] pub struct AzGetProgramBinaryReturn {
        pub _0: AzU8Vec,
        pub _1: u32,
    }
    /// C-ABI stable reexport of `(i32, u32, AzString)`
    #[repr(C)] #[derive(Debug)] pub struct AzGetActiveAttribReturn {
        pub _0: i32,
        pub _1: u32,
        pub _2: AzString,
    }
    /// C-ABI stable reexport of `*const gleam::gl::GLsync`
    #[repr(C)] #[derive(Debug)] pub struct AzGLsyncPtr {
        pub(crate) ptr: *const c_void,
    }
    /// C-ABI stable reexport of `(i32, u32, AzString)`
    #[repr(C)] #[derive(Debug)] pub struct AzGetActiveUniformReturn {
        pub _0: i32,
        pub _1: u32,
        pub _2: AzString,
    }
    /// Re-export of rust-allocated (stack based) `GlContextPtr` struct
    #[repr(C)] #[derive(Debug)] pub struct AzGlContextPtr {
        pub(crate) ptr: *const c_void,
    }
    /// Re-export of rust-allocated (stack based) `Texture` struct
    #[repr(C)] #[derive(Debug)] pub struct AzTexture {
        pub texture_id: u32,
        pub format: AzRawImageFormat,
        pub flags: AzTextureFlags,
        pub size: AzPhysicalSizeU32,
        pub gl_context: AzGlContextPtr,
    }
    /// Re-export of rust-allocated (stack based) `TextureFlags` struct
    #[repr(C)] #[derive(Debug)] pub struct AzTextureFlags {
        pub is_opaque: bool,
        pub is_video_texture: bool,
    }
    /// Re-export of rust-allocated (stack based) `RawImageFormat` struct
    #[repr(C)] #[derive(Debug)] pub enum AzRawImageFormat {
        R8,
        R16,
        RG16,
        BGRA8,
        RGBAF32,
        RG8,
        RGBAI32,
        RGBA8,
    }
    /// Re-export of rust-allocated (stack based) `TextId` struct
    #[repr(C)] #[derive(Debug)] pub struct AzTextId {
        pub id: usize,
    }
    /// Re-export of rust-allocated (stack based) `ImageId` struct
    #[repr(C)] #[derive(Debug)] pub struct AzImageId {
        pub id: usize,
    }
    /// Re-export of rust-allocated (stack based) `FontId` struct
    #[repr(C)] #[derive(Debug)] pub struct AzFontId {
        pub id: usize,
    }
    /// Re-export of rust-allocated (stack based) `ImageSource` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzImageSource {
        Embedded(AzU8Vec),
        File(AzString),
        Raw(AzRawImage),
    }
    /// Re-export of rust-allocated (stack based) `FontSource` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzFontSource {
        Embedded(AzU8Vec),
        File(AzString),
        System(AzString),
    }
    /// Re-export of rust-allocated (stack based) `RawImage` struct
    #[repr(C)] #[derive(Debug)] pub struct AzRawImage {
        pub pixels: AzU8Vec,
        pub width: usize,
        pub height: usize,
        pub data_format: AzRawImageFormat,
    }
    /// Re-export of rust-allocated (stack based) `SvgMultiPolygon` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgMultiPolygon {
        pub rings: AzSvgPathVec,
    }
    /// Re-export of rust-allocated (stack based) `SvgNode` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzSvgNode {
        MultiPolygonCollection(AzSvgMultiPolygonVec),
        MultiPolygon(AzSvgMultiPolygon),
        Path(AzSvgPath),
        Circle(AzSvgCircle),
        Rect(AzSvgRect),
    }
    /// Re-export of rust-allocated (stack based) `SvgStyledNode` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgStyledNode {
        pub geometry: AzSvgNode,
        pub style: AzSvgStyle,
    }
    /// Re-export of rust-allocated (stack based) `SvgCircle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgCircle {
        pub center_x: f32,
        pub center_y: f32,
        pub radius: f32,
    }
    /// Re-export of rust-allocated (stack based) `SvgPath` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgPath {
        pub items: AzSvgPathElementVec,
    }
    /// Re-export of rust-allocated (stack based) `SvgPathElement` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzSvgPathElement {
        Line(AzSvgLine),
        QuadraticCurve(AzSvgQuadraticCurve),
        CubicCurve(AzSvgCubicCurve),
    }
    /// Re-export of rust-allocated (stack based) `SvgLine` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgLine {
        pub start: AzSvgPoint,
        pub end: AzSvgPoint,
    }
    /// Re-export of rust-allocated (stack based) `SvgPoint` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgPoint {
        pub x: f32,
        pub y: f32,
    }
    /// Re-export of rust-allocated (stack based) `SvgVertex` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgVertex {
        pub x: f32,
        pub y: f32,
    }
    /// Re-export of rust-allocated (stack based) `SvgQuadraticCurve` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgQuadraticCurve {
        pub start: AzSvgPoint,
        pub ctrl: AzSvgPoint,
        pub end: AzSvgPoint,
    }
    /// Re-export of rust-allocated (stack based) `SvgCubicCurve` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgCubicCurve {
        pub start: AzSvgPoint,
        pub ctrl_1: AzSvgPoint,
        pub ctrl_2: AzSvgPoint,
        pub end: AzSvgPoint,
    }
    /// Re-export of rust-allocated (stack based) `SvgRect` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgRect {
        pub width: f32,
        pub height: f32,
        pub x: f32,
        pub y: f32,
        pub radius_top_left: f32,
        pub radius_top_right: f32,
        pub radius_bottom_left: f32,
        pub radius_bottom_right: f32,
    }
    /// Re-export of rust-allocated (stack based) `TesselatedCPUSvgNode` struct
    #[repr(C)] #[derive(Debug)] pub struct AzTesselatedCPUSvgNode {
        pub vertices: AzSvgVertexVec,
        pub indices: AzU32Vec,
    }
    /// Re-export of rust-allocated (stack based) `SvgLineCap` struct
    #[repr(C)] #[derive(Debug)] pub enum AzSvgLineCap {
        Butt,
        Square,
        Round,
    }
    /// Re-export of rust-allocated (stack based) `SvgParseOptions` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgParseOptions {
        pub relative_image_path: AzOptionString,
        pub dpi: f32,
        pub default_font_family: AzString,
        pub font_size: f32,
        pub languages: AzStringVec,
        pub shape_rendering: AzShapeRendering,
        pub text_rendering: AzTextRendering,
        pub image_rendering: AzImageRendering,
        pub keep_named_groups: bool,
        pub fontdb: AzFontDatabase,
    }
    /// Re-export of rust-allocated (stack based) `ShapeRendering` struct
    #[repr(C)] #[derive(Debug)] pub enum AzShapeRendering {
        OptimizeSpeed,
        CrispEdges,
        GeometricPrecision,
    }
    /// Re-export of rust-allocated (stack based) `TextRendering` struct
    #[repr(C)] #[derive(Debug)] pub enum AzTextRendering {
        OptimizeSpeed,
        OptimizeLegibility,
        GeometricPrecision,
    }
    /// Re-export of rust-allocated (stack based) `ImageRendering` struct
    #[repr(C)] #[derive(Debug)] pub enum AzImageRendering {
        OptimizeQuality,
        OptimizeSpeed,
    }
    /// Re-export of rust-allocated (stack based) `FontDatabase` struct
    #[repr(C)] #[derive(Debug)] pub enum AzFontDatabase {
        Empty,
        System,
    }
    /// Re-export of rust-allocated (stack based) `SvgRenderOptions` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgRenderOptions {
        pub background_color: AzOptionColorU,
        pub fit: AzSvgFitTo,
    }
    /// Re-export of rust-allocated (stack based) `SvgFitTo` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzSvgFitTo {
        Original,
        Width(u32),
        Height(u32),
        Zoom(f32),
    }
    /// Re-export of rust-allocated (stack based) `Svg` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvg {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `SvgXmlNode` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgXmlNode {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `SvgLineJoin` struct
    #[repr(C)] #[derive(Debug)] pub enum AzSvgLineJoin {
        Miter,
        MiterClip,
        Round,
        Bevel,
    }
    /// Re-export of rust-allocated (stack based) `SvgDashPattern` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgDashPattern {
        pub offset: usize,
        pub length_1: usize,
        pub gap_1: usize,
        pub length_2: usize,
        pub gap_2: usize,
        pub length_3: usize,
        pub gap_3: usize,
    }
    /// Re-export of rust-allocated (stack based) `SvgStyle` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzSvgStyle {
        Fill(AzSvgFillStyle),
        Stroke(AzSvgStrokeStyle),
    }
    /// Re-export of rust-allocated (stack based) `SvgFillStyle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgFillStyle {
        pub line_join: AzSvgLineJoin,
        pub miter_limit: usize,
        pub tolerance: usize,
    }
    /// Re-export of rust-allocated (stack based) `SvgStrokeStyle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgStrokeStyle {
        pub start_cap: AzSvgLineCap,
        pub end_cap: AzSvgLineCap,
        pub line_join: AzSvgLineJoin,
        pub dash_pattern: AzOptionSvgDashPattern,
        pub line_width: usize,
        pub miter_limit: usize,
        pub tolerance: usize,
        pub apply_line_width: bool,
    }
    /// Re-export of rust-allocated (stack based) `SvgNodeId` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSvgNodeId {
        pub id: usize,
    }
    /// Re-export of rust-allocated (stack based) `TimerId` struct
    #[repr(C)] #[derive(Debug)] pub struct AzTimerId {
        pub id: usize,
    }
    /// Re-export of rust-allocated (stack based) `Timer` struct
    #[repr(C)] #[derive(Debug)] pub struct AzTimer {
        pub data: AzRefAny,
        pub created: AzInstantPtr,
        pub last_run: AzOptionInstantPtr,
        pub run_count: usize,
        pub delay: AzOptionDuration,
        pub interval: AzOptionDuration,
        pub timeout: AzOptionDuration,
        pub callback: AzTimerCallback,
    }
    /// Should a timer terminate or not - used to remove active timers
    #[repr(C)] #[derive(Debug)] pub enum AzTerminateTimer {
        Terminate,
        Continue,
    }
    /// Re-export of rust-allocated (stack based) `ThreadSender` struct
    #[repr(C)] #[derive(Debug)] pub struct AzThreadSender {
        pub(crate) ptr: *const c_void,
    }
    /// Re-export of rust-allocated (stack based) `ThreadReceiver` struct
    #[repr(C)] #[derive(Debug)] pub struct AzThreadReceiver {
        pub(crate) ptr: *const c_void,
    }
    /// Re-export of rust-allocated (stack based) `ThreadSendMsg` struct
    #[repr(C)] #[derive(Debug)] pub enum AzThreadSendMsg {
        TerminateThread,
        Tick,
    }
    /// Re-export of rust-allocated (stack based) `ThreadReceiveMsg` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzThreadReceiveMsg {
        WriteBack(AzThreadWriteBackMsg),
        Update(AzUpdateScreen),
    }
    /// Re-export of rust-allocated (stack based) `ThreadWriteBackMsg` struct
    #[repr(C)] #[derive(Debug)] pub struct AzThreadWriteBackMsg {
        pub data: AzRefAny,
        pub callback: AzWriteBackCallback,
    }
    /// Re-export of rust-allocated (stack based) `ThreadId` struct
    #[repr(C)] #[derive(Debug)] pub struct AzThreadId {
        pub id: usize,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPoint` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutPoint {
        pub x: isize,
        pub y: isize,
    }
    /// Re-export of rust-allocated (stack based) `LayoutSize` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutSize {
        pub width: isize,
        pub height: isize,
    }
    /// Re-export of rust-allocated (stack based) `LayoutRect` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutRect {
        pub origin: AzLayoutPoint,
        pub size: AzLayoutSize,
    }
    /// Re-export of rust-allocated (stack based) `RawWindowHandle` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzRawWindowHandle {
        IOS(AzIOSHandle),
        MacOS(AzMacOSHandle),
        Xlib(AzXlibHandle),
        Xcb(AzXcbHandle),
        Wayland(AzWaylandHandle),
        Windows(AzWindowsHandle),
        Web(AzWebHandle),
        Android(AzAndroidHandle),
        Unsupported,
    }
    /// Re-export of rust-allocated (stack based) `IOSHandle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzIOSHandle {
        pub ui_window: *mut c_void,
        pub ui_view: *mut c_void,
        pub ui_view_controller: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `MacOSHandle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzMacOSHandle {
        pub ns_window: *mut c_void,
        pub ns_view: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `XlibHandle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzXlibHandle {
        pub window: u64,
        pub display: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `XcbHandle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzXcbHandle {
        pub window: u32,
        pub connection: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `WaylandHandle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzWaylandHandle {
        pub surface: *mut c_void,
        pub display: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `WindowsHandle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzWindowsHandle {
        pub hwnd: *mut c_void,
        pub hinstance: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `WebHandle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzWebHandle {
        pub id: u32,
    }
    /// Re-export of rust-allocated (stack based) `AndroidHandle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzAndroidHandle {
        pub a_native_window: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `TaskBarIcon` struct
    #[repr(C)] #[derive(Debug)] pub struct AzTaskBarIcon {
        pub key: AzIconKey,
        pub rgba_bytes: AzU8Vec,
    }
    /// Re-export of rust-allocated (stack based) `XWindowType` struct
    #[repr(C)] #[derive(Debug)] pub enum AzXWindowType {
        Desktop,
        Dock,
        Toolbar,
        Menu,
        Utility,
        Splash,
        Dialog,
        DropdownMenu,
        PopupMenu,
        Tooltip,
        Notification,
        Combo,
        Dnd,
        Normal,
    }
    /// Re-export of rust-allocated (stack based) `PhysicalPositionI32` struct
    #[repr(C)] #[derive(Debug)] pub struct AzPhysicalPositionI32 {
        pub x: i32,
        pub y: i32,
    }
    /// Re-export of rust-allocated (stack based) `PhysicalSizeU32` struct
    #[repr(C)] #[derive(Debug)] pub struct AzPhysicalSizeU32 {
        pub width: u32,
        pub height: u32,
    }
    /// Re-export of rust-allocated (stack based) `LogicalPosition` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLogicalPosition {
        pub x: f32,
        pub y: f32,
    }
    /// Re-export of rust-allocated (stack based) `LogicalRect` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLogicalRect {
        pub origin: AzLogicalPosition,
        pub size: AzLogicalSize,
    }
    /// Re-export of rust-allocated (stack based) `IconKey` struct
    #[repr(C)] #[derive(Debug)] pub struct AzIconKey {
        pub id: usize,
    }
    /// Re-export of rust-allocated (stack based) `SmallWindowIconBytes` struct
    #[repr(C)] #[derive(Debug)] pub struct AzSmallWindowIconBytes {
        pub key: AzIconKey,
        pub rgba_bytes: AzU8Vec,
    }
    /// Re-export of rust-allocated (stack based) `LargeWindowIconBytes` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLargeWindowIconBytes {
        pub key: AzIconKey,
        pub rgba_bytes: AzU8Vec,
    }
    /// Re-export of rust-allocated (stack based) `WindowIcon` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzWindowIcon {
        Small(AzSmallWindowIconBytes),
        Large(AzLargeWindowIconBytes),
    }
    /// Re-export of rust-allocated (stack based) `VirtualKeyCode` struct
    #[repr(C)] #[derive(Debug)] pub enum AzVirtualKeyCode {
        Key1,
        Key2,
        Key3,
        Key4,
        Key5,
        Key6,
        Key7,
        Key8,
        Key9,
        Key0,
        A,
        B,
        C,
        D,
        E,
        F,
        G,
        H,
        I,
        J,
        K,
        L,
        M,
        N,
        O,
        P,
        Q,
        R,
        S,
        T,
        U,
        V,
        W,
        X,
        Y,
        Z,
        Escape,
        F1,
        F2,
        F3,
        F4,
        F5,
        F6,
        F7,
        F8,
        F9,
        F10,
        F11,
        F12,
        F13,
        F14,
        F15,
        F16,
        F17,
        F18,
        F19,
        F20,
        F21,
        F22,
        F23,
        F24,
        Snapshot,
        Scroll,
        Pause,
        Insert,
        Home,
        Delete,
        End,
        PageDown,
        PageUp,
        Left,
        Up,
        Right,
        Down,
        Back,
        Return,
        Space,
        Compose,
        Caret,
        Numlock,
        Numpad0,
        Numpad1,
        Numpad2,
        Numpad3,
        Numpad4,
        Numpad5,
        Numpad6,
        Numpad7,
        Numpad8,
        Numpad9,
        NumpadAdd,
        NumpadDivide,
        NumpadDecimal,
        NumpadComma,
        NumpadEnter,
        NumpadEquals,
        NumpadMultiply,
        NumpadSubtract,
        AbntC1,
        AbntC2,
        Apostrophe,
        Apps,
        Asterisk,
        At,
        Ax,
        Backslash,
        Calculator,
        Capital,
        Colon,
        Comma,
        Convert,
        Equals,
        Grave,
        Kana,
        Kanji,
        LAlt,
        LBracket,
        LControl,
        LShift,
        LWin,
        Mail,
        MediaSelect,
        MediaStop,
        Minus,
        Mute,
        MyComputer,
        NavigateForward,
        NavigateBackward,
        NextTrack,
        NoConvert,
        OEM102,
        Period,
        PlayPause,
        Plus,
        Power,
        PrevTrack,
        RAlt,
        RBracket,
        RControl,
        RShift,
        RWin,
        Semicolon,
        Slash,
        Sleep,
        Stop,
        Sysrq,
        Tab,
        Underline,
        Unlabeled,
        VolumeDown,
        VolumeUp,
        Wake,
        WebBack,
        WebFavorites,
        WebForward,
        WebHome,
        WebRefresh,
        WebSearch,
        WebStop,
        Yen,
        Copy,
        Paste,
        Cut,
    }
    /// Re-export of rust-allocated (stack based) `AcceleratorKey` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzAcceleratorKey {
        Ctrl,
        Alt,
        Shift,
        Key(AzVirtualKeyCode),
    }
    /// Re-export of rust-allocated (stack based) `WindowSize` struct
    #[repr(C)] #[derive(Debug)] pub struct AzWindowSize {
        pub dimensions: AzLogicalSize,
        pub hidpi_factor: f32,
        pub system_hidpi_factor: f32,
        pub min_dimensions: AzOptionLogicalSize,
        pub max_dimensions: AzOptionLogicalSize,
    }
    /// Re-export of rust-allocated (stack based) `WindowFlags` struct
    #[repr(C)] #[derive(Debug)] pub struct AzWindowFlags {
        pub is_maximized: bool,
        pub is_minimized: bool,
        pub is_about_to_close: bool,
        pub is_fullscreen: bool,
        pub has_decorations: bool,
        pub is_visible: bool,
        pub is_always_on_top: bool,
        pub is_resizable: bool,
        pub has_focus: bool,
        pub has_blur_behind_window: bool,
    }
    /// Re-export of rust-allocated (stack based) `DebugState` struct
    #[repr(C)] #[derive(Debug)] pub struct AzDebugState {
        pub profiler_dbg: bool,
        pub render_target_dbg: bool,
        pub texture_cache_dbg: bool,
        pub gpu_time_queries: bool,
        pub gpu_sample_queries: bool,
        pub disable_batching: bool,
        pub epochs: bool,
        pub compact_profiler: bool,
        pub echo_driver_messages: bool,
        pub new_frame_indicator: bool,
        pub new_scene_indicator: bool,
        pub show_overdraw: bool,
        pub gpu_cache_dbg: bool,
    }
    /// Re-export of rust-allocated (stack based) `KeyboardState` struct
    #[repr(C)] #[derive(Debug)] pub struct AzKeyboardState {
        pub shift_down: bool,
        pub ctrl_down: bool,
        pub alt_down: bool,
        pub super_down: bool,
        pub current_char: AzOptionChar,
        pub current_virtual_keycode: AzOptionVirtualKeyCode,
        pub pressed_virtual_keycodes: AzVirtualKeyCodeVec,
        pub pressed_scancodes: AzScanCodeVec,
    }
    /// Re-export of rust-allocated (stack based) `MouseCursorType` struct
    #[repr(C)] #[derive(Debug)] pub enum AzMouseCursorType {
        Default,
        Crosshair,
        Hand,
        Arrow,
        Move,
        Text,
        Wait,
        Help,
        Progress,
        NotAllowed,
        ContextMenu,
        Cell,
        VerticalText,
        Alias,
        Copy,
        NoDrop,
        Grab,
        Grabbing,
        AllScroll,
        ZoomIn,
        ZoomOut,
        EResize,
        NResize,
        NeResize,
        NwResize,
        SResize,
        SeResize,
        SwResize,
        WResize,
        EwResize,
        NsResize,
        NeswResize,
        NwseResize,
        ColResize,
        RowResize,
    }
    /// Re-export of rust-allocated (stack based) `CursorPosition` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzCursorPosition {
        OutOfWindow,
        Uninitialized,
        InWindow(AzLogicalPosition),
    }
    /// Re-export of rust-allocated (stack based) `MouseState` struct
    #[repr(C)] #[derive(Debug)] pub struct AzMouseState {
        pub mouse_cursor_type: AzOptionMouseCursorType,
        pub cursor_position: AzCursorPosition,
        pub is_cursor_locked: bool,
        pub left_down: bool,
        pub right_down: bool,
        pub middle_down: bool,
        pub scroll_x: AzOptionF32,
        pub scroll_y: AzOptionF32,
    }
    /// Re-export of rust-allocated (stack based) `PlatformSpecificOptions` struct
    #[repr(C)] #[derive(Debug)] pub struct AzPlatformSpecificOptions {
        pub windows_options: AzWindowsWindowOptions,
        pub linux_options: AzLinuxWindowOptions,
        pub mac_options: AzMacWindowOptions,
        pub wasm_options: AzWasmWindowOptions,
    }
    /// Re-export of rust-allocated (stack based) `WindowsWindowOptions` struct
    #[repr(C)] #[derive(Debug)] pub struct AzWindowsWindowOptions {
        pub allow_drag_drop: bool,
        pub no_redirection_bitmap: bool,
        pub window_icon: AzOptionWindowIcon,
        pub taskbar_icon: AzOptionTaskBarIcon,
        pub parent_window: AzOptionHwndHandle,
    }
    /// Re-export of rust-allocated (stack based) `WaylandTheme` struct
    #[repr(C)] #[derive(Debug)] pub struct AzWaylandTheme {
        pub primary_color_active: [u8;4],
        pub primary_color_inactive: [u8;4],
        pub secondary_color_active: [u8;4],
        pub secondary_color_inactive: [u8;4],
        pub close_button_color_idle: [u8;4],
        pub close_button_color_hovered: [u8;4],
        pub close_button_color_disabled: [u8;4],
        pub maximize_button_color_idle: [u8;4],
        pub maximize_button_color_hovered: [u8;4],
        pub maximize_button_color_disabled: [u8;4],
        pub minimize_button_color_idle: [u8;4],
        pub minimize_button_color_hovered: [u8;4],
        pub minimize_button_color_disabled: [u8;4],
    }
    /// Re-export of rust-allocated (stack based) `RendererType` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzRendererType {
        Default,
        ForceHardware,
        ForceSoftware,
        Custom(AzGlContextPtr),
    }
    /// Re-export of rust-allocated (stack based) `StringPair` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStringPair {
        pub key: AzString,
        pub value: AzString,
    }
    /// Re-export of rust-allocated (stack based) `LinuxWindowOptions` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLinuxWindowOptions {
        pub x11_visual: AzOptionX11Visual,
        pub x11_screen: AzOptionI32,
        pub x11_wm_classes: AzStringPairVec,
        pub x11_override_redirect: bool,
        pub x11_window_types: AzXWindowTypeVec,
        pub x11_gtk_theme_variant: AzOptionString,
        pub x11_resize_increments: AzOptionLogicalSize,
        pub x11_base_size: AzOptionLogicalSize,
        pub wayland_app_id: AzOptionString,
        pub wayland_theme: AzOptionWaylandTheme,
        pub request_user_attention: bool,
        pub window_icon: AzOptionWindowIcon,
    }
    /// Re-export of rust-allocated (stack based) `MacWindowOptions` struct
    #[repr(C)] #[derive(Debug)] pub struct AzMacWindowOptions {
        pub _reserved: u8,
    }
    /// Re-export of rust-allocated (stack based) `WasmWindowOptions` struct
    #[repr(C)] #[derive(Debug)] pub struct AzWasmWindowOptions {
        pub _reserved: u8,
    }
    /// Re-export of rust-allocated (stack based) `FullScreenMode` struct
    #[repr(C)] #[derive(Debug)] pub enum AzFullScreenMode {
        SlowFullScreen,
        FastFullScreen,
        SlowWindowed,
        FastWindowed,
    }
    /// Re-export of rust-allocated (stack based) `WindowTheme` struct
    #[repr(C)] #[derive(Debug)] pub enum AzWindowTheme {
        DarkMode,
        LightMode,
    }
    /// Re-export of rust-allocated (stack based) `WindowPosition` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzWindowPosition {
        Uninitialized,
        Initialized(AzPhysicalPositionI32),
    }
    /// Re-export of rust-allocated (stack based) `ImePosition` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzImePosition {
        Uninitialized,
        Initialized(AzLogicalPosition),
    }
    /// Re-export of rust-allocated (stack based) `TouchState` struct
    #[repr(C)] #[derive(Debug)] pub struct AzTouchState {
        pub unused: u8,
    }
    /// Re-export of rust-allocated (stack based) `WindowState` struct
    #[repr(C)] #[derive(Debug)] pub struct AzWindowState {
        pub theme: AzWindowTheme,
        pub title: AzString,
        pub size: AzWindowSize,
        pub position: AzWindowPosition,
        pub flags: AzWindowFlags,
        pub debug_state: AzDebugState,
        pub keyboard_state: AzKeyboardState,
        pub mouse_state: AzMouseState,
        pub touch_state: AzTouchState,
        pub ime_position: AzImePosition,
        pub platform_specific_options: AzPlatformSpecificOptions,
        pub background_color: AzColorU,
        pub layout_callback: AzLayoutCallback,
    }
    /// Re-export of rust-allocated (stack based) `LogicalSize` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLogicalSize {
        pub width: f32,
        pub height: f32,
    }
    /// Re-export of rust-allocated (stack based) `WindowCreateOptions` struct
    #[repr(C)] #[derive(Debug)] pub struct AzWindowCreateOptions {
        pub state: AzWindowState,
        pub renderer_type: AzRendererType,
        pub theme: AzOptionWindowTheme,
    }


    use libloading_mini::Library;
    pub struct AzulDll {
        pub lib: Library,
        pub az_string_from_utf8_unchecked: extern "C" fn(_:  *const u8, _:  usize) -> AzString,
        pub az_string_from_utf8_lossy: extern "C" fn(_:  *const u8, _:  usize) -> AzString,
        pub az_string_into_bytes: extern "C" fn(_:  AzString) -> AzU8Vec,
        pub az_string_delete: extern "C" fn(_:  &mut AzString),
        pub az_string_deep_copy: extern "C" fn(_:  &AzString) -> AzString,
        pub az_style_transform_vec_new: extern "C" fn() -> AzStyleTransformVec,
        pub az_style_transform_vec_with_capacity: extern "C" fn(_:  usize) -> AzStyleTransformVec,
        pub az_style_transform_vec_copy_from: extern "C" fn(_:  *const AzStyleTransform, _:  usize) -> AzStyleTransformVec,
        pub az_style_transform_vec_delete: extern "C" fn(_:  &mut AzStyleTransformVec),
        pub az_style_transform_vec_deep_copy: extern "C" fn(_:  &AzStyleTransformVec) -> AzStyleTransformVec,
        pub az_content_group_vec_new: extern "C" fn() -> AzContentGroupVec,
        pub az_content_group_vec_with_capacity: extern "C" fn(_:  usize) -> AzContentGroupVec,
        pub az_content_group_vec_copy_from: extern "C" fn(_:  *const AzContentGroup, _:  usize) -> AzContentGroupVec,
        pub az_content_group_vec_delete: extern "C" fn(_:  &mut AzContentGroupVec),
        pub az_content_group_vec_deep_copy: extern "C" fn(_:  &AzContentGroupVec) -> AzContentGroupVec,
        pub az_css_property_vec_new: extern "C" fn() -> AzCssPropertyVec,
        pub az_css_property_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssPropertyVec,
        pub az_css_property_vec_copy_from: extern "C" fn(_:  *const AzCssProperty, _:  usize) -> AzCssPropertyVec,
        pub az_css_property_vec_delete: extern "C" fn(_:  &mut AzCssPropertyVec),
        pub az_css_property_vec_deep_copy: extern "C" fn(_:  &AzCssPropertyVec) -> AzCssPropertyVec,
        pub az_svg_multi_polygon_vec_new: extern "C" fn() -> AzSvgMultiPolygonVec,
        pub az_svg_multi_polygon_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgMultiPolygonVec,
        pub az_svg_multi_polygon_vec_copy_from: extern "C" fn(_:  *const AzSvgMultiPolygon, _:  usize) -> AzSvgMultiPolygonVec,
        pub az_svg_multi_polygon_vec_delete: extern "C" fn(_:  &mut AzSvgMultiPolygonVec),
        pub az_svg_multi_polygon_vec_deep_copy: extern "C" fn(_:  &AzSvgMultiPolygonVec) -> AzSvgMultiPolygonVec,
        pub az_svg_path_vec_new: extern "C" fn() -> AzSvgPathVec,
        pub az_svg_path_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgPathVec,
        pub az_svg_path_vec_copy_from: extern "C" fn(_:  *const AzSvgPath, _:  usize) -> AzSvgPathVec,
        pub az_svg_path_vec_delete: extern "C" fn(_:  &mut AzSvgPathVec),
        pub az_svg_path_vec_deep_copy: extern "C" fn(_:  &AzSvgPathVec) -> AzSvgPathVec,
        pub az_vertex_attribute_vec_new: extern "C" fn() -> AzVertexAttributeVec,
        pub az_vertex_attribute_vec_with_capacity: extern "C" fn(_:  usize) -> AzVertexAttributeVec,
        pub az_vertex_attribute_vec_copy_from: extern "C" fn(_:  *const AzVertexAttribute, _:  usize) -> AzVertexAttributeVec,
        pub az_vertex_attribute_vec_delete: extern "C" fn(_:  &mut AzVertexAttributeVec),
        pub az_vertex_attribute_vec_deep_copy: extern "C" fn(_:  &AzVertexAttributeVec) -> AzVertexAttributeVec,
        pub az_svg_path_element_vec_new: extern "C" fn() -> AzSvgPathElementVec,
        pub az_svg_path_element_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgPathElementVec,
        pub az_svg_path_element_vec_copy_from: extern "C" fn(_:  *const AzSvgPathElement, _:  usize) -> AzSvgPathElementVec,
        pub az_svg_path_element_vec_delete: extern "C" fn(_:  &mut AzSvgPathElementVec),
        pub az_svg_path_element_vec_deep_copy: extern "C" fn(_:  &AzSvgPathElementVec) -> AzSvgPathElementVec,
        pub az_svg_vertex_vec_new: extern "C" fn() -> AzSvgVertexVec,
        pub az_svg_vertex_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgVertexVec,
        pub az_svg_vertex_vec_copy_from: extern "C" fn(_:  *const AzSvgVertex, _:  usize) -> AzSvgVertexVec,
        pub az_svg_vertex_vec_delete: extern "C" fn(_:  &mut AzSvgVertexVec),
        pub az_svg_vertex_vec_deep_copy: extern "C" fn(_:  &AzSvgVertexVec) -> AzSvgVertexVec,
        pub az_u32_vec_new: extern "C" fn() -> AzU32Vec,
        pub az_u32_vec_with_capacity: extern "C" fn(_:  usize) -> AzU32Vec,
        pub az_u32_vec_copy_from: extern "C" fn(_:  *const u32, _:  usize) -> AzU32Vec,
        pub az_u32_vec_delete: extern "C" fn(_:  &mut AzU32Vec),
        pub az_u32_vec_deep_copy: extern "C" fn(_:  &AzU32Vec) -> AzU32Vec,
        pub az_x_window_type_vec_new: extern "C" fn() -> AzXWindowTypeVec,
        pub az_x_window_type_vec_with_capacity: extern "C" fn(_:  usize) -> AzXWindowTypeVec,
        pub az_x_window_type_vec_copy_from: extern "C" fn(_:  *const AzXWindowType, _:  usize) -> AzXWindowTypeVec,
        pub az_x_window_type_vec_delete: extern "C" fn(_:  &mut AzXWindowTypeVec),
        pub az_x_window_type_vec_deep_copy: extern "C" fn(_:  &AzXWindowTypeVec) -> AzXWindowTypeVec,
        pub az_virtual_key_code_vec_new: extern "C" fn() -> AzVirtualKeyCodeVec,
        pub az_virtual_key_code_vec_with_capacity: extern "C" fn(_:  usize) -> AzVirtualKeyCodeVec,
        pub az_virtual_key_code_vec_copy_from: extern "C" fn(_:  *const AzVirtualKeyCode, _:  usize) -> AzVirtualKeyCodeVec,
        pub az_virtual_key_code_vec_delete: extern "C" fn(_:  &mut AzVirtualKeyCodeVec),
        pub az_virtual_key_code_vec_deep_copy: extern "C" fn(_:  &AzVirtualKeyCodeVec) -> AzVirtualKeyCodeVec,
        pub az_cascade_info_vec_new: extern "C" fn() -> AzCascadeInfoVec,
        pub az_cascade_info_vec_with_capacity: extern "C" fn(_:  usize) -> AzCascadeInfoVec,
        pub az_cascade_info_vec_copy_from: extern "C" fn(_:  *const AzCascadeInfo, _:  usize) -> AzCascadeInfoVec,
        pub az_cascade_info_vec_delete: extern "C" fn(_:  &mut AzCascadeInfoVec),
        pub az_cascade_info_vec_deep_copy: extern "C" fn(_:  &AzCascadeInfoVec) -> AzCascadeInfoVec,
        pub az_scan_code_vec_new: extern "C" fn() -> AzScanCodeVec,
        pub az_scan_code_vec_with_capacity: extern "C" fn(_:  usize) -> AzScanCodeVec,
        pub az_scan_code_vec_copy_from: extern "C" fn(_:  *const u32, _:  usize) -> AzScanCodeVec,
        pub az_scan_code_vec_delete: extern "C" fn(_:  &mut AzScanCodeVec),
        pub az_scan_code_vec_deep_copy: extern "C" fn(_:  &AzScanCodeVec) -> AzScanCodeVec,
        pub az_css_declaration_vec_new: extern "C" fn() -> AzCssDeclarationVec,
        pub az_css_declaration_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssDeclarationVec,
        pub az_css_declaration_vec_copy_from: extern "C" fn(_:  *const AzCssDeclaration, _:  usize) -> AzCssDeclarationVec,
        pub az_css_declaration_vec_delete: extern "C" fn(_:  &mut AzCssDeclarationVec),
        pub az_css_declaration_vec_deep_copy: extern "C" fn(_:  &AzCssDeclarationVec) -> AzCssDeclarationVec,
        pub az_css_path_selector_vec_new: extern "C" fn() -> AzCssPathSelectorVec,
        pub az_css_path_selector_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssPathSelectorVec,
        pub az_css_path_selector_vec_copy_from: extern "C" fn(_:  *const AzCssPathSelector, _:  usize) -> AzCssPathSelectorVec,
        pub az_css_path_selector_vec_delete: extern "C" fn(_:  &mut AzCssPathSelectorVec),
        pub az_css_path_selector_vec_deep_copy: extern "C" fn(_:  &AzCssPathSelectorVec) -> AzCssPathSelectorVec,
        pub az_stylesheet_vec_new: extern "C" fn() -> AzStylesheetVec,
        pub az_stylesheet_vec_with_capacity: extern "C" fn(_:  usize) -> AzStylesheetVec,
        pub az_stylesheet_vec_copy_from: extern "C" fn(_:  *const AzStylesheet, _:  usize) -> AzStylesheetVec,
        pub az_stylesheet_vec_delete: extern "C" fn(_:  &mut AzStylesheetVec),
        pub az_stylesheet_vec_deep_copy: extern "C" fn(_:  &AzStylesheetVec) -> AzStylesheetVec,
        pub az_css_rule_block_vec_new: extern "C" fn() -> AzCssRuleBlockVec,
        pub az_css_rule_block_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssRuleBlockVec,
        pub az_css_rule_block_vec_copy_from: extern "C" fn(_:  *const AzCssRuleBlock, _:  usize) -> AzCssRuleBlockVec,
        pub az_css_rule_block_vec_delete: extern "C" fn(_:  &mut AzCssRuleBlockVec),
        pub az_css_rule_block_vec_deep_copy: extern "C" fn(_:  &AzCssRuleBlockVec) -> AzCssRuleBlockVec,
        pub az_u8_vec_new: extern "C" fn() -> AzU8Vec,
        pub az_u8_vec_with_capacity: extern "C" fn(_:  usize) -> AzU8Vec,
        pub az_u8_vec_copy_from: extern "C" fn(_:  *const u8, _:  usize) -> AzU8Vec,
        pub az_u8_vec_delete: extern "C" fn(_:  &mut AzU8Vec),
        pub az_u8_vec_deep_copy: extern "C" fn(_:  &AzU8Vec) -> AzU8Vec,
        pub az_callback_data_vec_new: extern "C" fn() -> AzCallbackDataVec,
        pub az_callback_data_vec_with_capacity: extern "C" fn(_:  usize) -> AzCallbackDataVec,
        pub az_callback_data_vec_copy_from: extern "C" fn(_:  *const AzCallbackData, _:  usize) -> AzCallbackDataVec,
        pub az_callback_data_vec_delete: extern "C" fn(_:  &mut AzCallbackDataVec),
        pub az_callback_data_vec_deep_copy: extern "C" fn(_:  &AzCallbackDataVec) -> AzCallbackDataVec,
        pub az_debug_message_vec_new: extern "C" fn() -> AzDebugMessageVec,
        pub az_debug_message_vec_with_capacity: extern "C" fn(_:  usize) -> AzDebugMessageVec,
        pub az_debug_message_vec_copy_from: extern "C" fn(_:  *const AzDebugMessage, _:  usize) -> AzDebugMessageVec,
        pub az_debug_message_vec_delete: extern "C" fn(_:  &mut AzDebugMessageVec),
        pub az_debug_message_vec_deep_copy: extern "C" fn(_:  &AzDebugMessageVec) -> AzDebugMessageVec,
        pub az_g_luint_vec_new: extern "C" fn() -> AzGLuintVec,
        pub az_g_luint_vec_with_capacity: extern "C" fn(_:  usize) -> AzGLuintVec,
        pub az_g_luint_vec_copy_from: extern "C" fn(_:  *const u32, _:  usize) -> AzGLuintVec,
        pub az_g_luint_vec_delete: extern "C" fn(_:  &mut AzGLuintVec),
        pub az_g_luint_vec_deep_copy: extern "C" fn(_:  &AzGLuintVec) -> AzGLuintVec,
        pub az_g_lint_vec_new: extern "C" fn() -> AzGLintVec,
        pub az_g_lint_vec_with_capacity: extern "C" fn(_:  usize) -> AzGLintVec,
        pub az_g_lint_vec_copy_from: extern "C" fn(_:  *const i32, _:  usize) -> AzGLintVec,
        pub az_g_lint_vec_delete: extern "C" fn(_:  &mut AzGLintVec),
        pub az_g_lint_vec_deep_copy: extern "C" fn(_:  &AzGLintVec) -> AzGLintVec,
        pub az_dom_vec_new: extern "C" fn() -> AzDomVec,
        pub az_dom_vec_with_capacity: extern "C" fn(_:  usize) -> AzDomVec,
        pub az_dom_vec_copy_from: extern "C" fn(_:  *const AzDom, _:  usize) -> AzDomVec,
        pub az_dom_vec_delete: extern "C" fn(_:  &mut AzDomVec),
        pub az_dom_vec_deep_copy: extern "C" fn(_:  &AzDomVec) -> AzDomVec,
        pub az_string_vec_new: extern "C" fn() -> AzStringVec,
        pub az_string_vec_with_capacity: extern "C" fn(_:  usize) -> AzStringVec,
        pub az_string_vec_copy_from: extern "C" fn(_:  *const AzString, _:  usize) -> AzStringVec,
        pub az_string_vec_delete: extern "C" fn(_:  &mut AzStringVec),
        pub az_string_vec_deep_copy: extern "C" fn(_:  &AzStringVec) -> AzStringVec,
        pub az_string_pair_vec_new: extern "C" fn() -> AzStringPairVec,
        pub az_string_pair_vec_with_capacity: extern "C" fn(_:  usize) -> AzStringPairVec,
        pub az_string_pair_vec_copy_from: extern "C" fn(_:  *const AzStringPair, _:  usize) -> AzStringPairVec,
        pub az_string_pair_vec_delete: extern "C" fn(_:  &mut AzStringPairVec),
        pub az_string_pair_vec_deep_copy: extern "C" fn(_:  &AzStringPairVec) -> AzStringPairVec,
        pub az_gradient_stop_pre_vec_new: extern "C" fn() -> AzGradientStopPreVec,
        pub az_gradient_stop_pre_vec_with_capacity: extern "C" fn(_:  usize) -> AzGradientStopPreVec,
        pub az_gradient_stop_pre_vec_copy_from: extern "C" fn(_:  *const AzGradientStopPre, _:  usize) -> AzGradientStopPreVec,
        pub az_gradient_stop_pre_vec_delete: extern "C" fn(_:  &mut AzGradientStopPreVec),
        pub az_gradient_stop_pre_vec_deep_copy: extern "C" fn(_:  &AzGradientStopPreVec) -> AzGradientStopPreVec,
        pub az_cascaded_css_property_with_source_vec_new: extern "C" fn() -> AzCascadedCssPropertyWithSourceVec,
        pub az_cascaded_css_property_with_source_vec_with_capacity: extern "C" fn(_:  usize) -> AzCascadedCssPropertyWithSourceVec,
        pub az_cascaded_css_property_with_source_vec_copy_from: extern "C" fn(_:  *const AzCascadedCssPropertyWithSource, _:  usize) -> AzCascadedCssPropertyWithSourceVec,
        pub az_cascaded_css_property_with_source_vec_delete: extern "C" fn(_:  &mut AzCascadedCssPropertyWithSourceVec),
        pub az_cascaded_css_property_with_source_vec_deep_copy: extern "C" fn(_:  &AzCascadedCssPropertyWithSourceVec) -> AzCascadedCssPropertyWithSourceVec,
        pub az_node_id_vec_new: extern "C" fn() -> AzNodeIdVec,
        pub az_node_id_vec_with_capacity: extern "C" fn(_:  usize) -> AzNodeIdVec,
        pub az_node_id_vec_copy_from: extern "C" fn(_:  *const AzNodeId, _:  usize) -> AzNodeIdVec,
        pub az_node_id_vec_delete: extern "C" fn(_:  &mut AzNodeIdVec),
        pub az_node_id_vec_deep_copy: extern "C" fn(_:  &AzNodeIdVec) -> AzNodeIdVec,
        pub az_node_vec_new: extern "C" fn() -> AzNodeVec,
        pub az_node_vec_with_capacity: extern "C" fn(_:  usize) -> AzNodeVec,
        pub az_node_vec_copy_from: extern "C" fn(_:  *const AzNode, _:  usize) -> AzNodeVec,
        pub az_node_vec_delete: extern "C" fn(_:  &mut AzNodeVec),
        pub az_node_vec_deep_copy: extern "C" fn(_:  &AzNodeVec) -> AzNodeVec,
        pub az_styled_node_vec_new: extern "C" fn() -> AzStyledNodeVec,
        pub az_styled_node_vec_with_capacity: extern "C" fn(_:  usize) -> AzStyledNodeVec,
        pub az_styled_node_vec_copy_from: extern "C" fn(_:  *const AzStyledNode, _:  usize) -> AzStyledNodeVec,
        pub az_styled_node_vec_delete: extern "C" fn(_:  &mut AzStyledNodeVec),
        pub az_styled_node_vec_deep_copy: extern "C" fn(_:  &AzStyledNodeVec) -> AzStyledNodeVec,
        pub az_tag_ids_to_node_ids_mapping_vec_new: extern "C" fn() -> AzTagIdsToNodeIdsMappingVec,
        pub az_tag_ids_to_node_ids_mapping_vec_with_capacity: extern "C" fn(_:  usize) -> AzTagIdsToNodeIdsMappingVec,
        pub az_tag_ids_to_node_ids_mapping_vec_copy_from: extern "C" fn(_:  *const AzTagIdToNodeIdMapping, _:  usize) -> AzTagIdsToNodeIdsMappingVec,
        pub az_tag_ids_to_node_ids_mapping_vec_delete: extern "C" fn(_:  &mut AzTagIdsToNodeIdsMappingVec),
        pub az_tag_ids_to_node_ids_mapping_vec_deep_copy: extern "C" fn(_:  &AzTagIdsToNodeIdsMappingVec) -> AzTagIdsToNodeIdsMappingVec,
        pub az_parent_with_node_depth_vec_new: extern "C" fn() -> AzParentWithNodeDepthVec,
        pub az_parent_with_node_depth_vec_with_capacity: extern "C" fn(_:  usize) -> AzParentWithNodeDepthVec,
        pub az_parent_with_node_depth_vec_copy_from: extern "C" fn(_:  *const AzParentWithNodeDepth, _:  usize) -> AzParentWithNodeDepthVec,
        pub az_parent_with_node_depth_vec_delete: extern "C" fn(_:  &mut AzParentWithNodeDepthVec),
        pub az_parent_with_node_depth_vec_deep_copy: extern "C" fn(_:  &AzParentWithNodeDepthVec) -> AzParentWithNodeDepthVec,
        pub az_node_data_vec_new: extern "C" fn() -> AzNodeDataVec,
        pub az_node_data_vec_with_capacity: extern "C" fn(_:  usize) -> AzNodeDataVec,
        pub az_node_data_vec_copy_from: extern "C" fn(_:  *const AzNodeData, _:  usize) -> AzNodeDataVec,
        pub az_node_data_vec_delete: extern "C" fn(_:  &mut AzNodeDataVec),
        pub az_node_data_vec_deep_copy: extern "C" fn(_:  &AzNodeDataVec) -> AzNodeDataVec,
        pub az_option_ref_any_delete: extern "C" fn(_:  &mut AzOptionRefAny),
        pub az_option_ref_any_deep_copy: extern "C" fn(_:  &AzOptionRefAny) -> AzOptionRefAny,
        pub az_option_style_transform_vec_value_delete: extern "C" fn(_:  &mut AzOptionStyleTransformVecValue),
        pub az_option_style_transform_vec_value_deep_copy: extern "C" fn(_:  &AzOptionStyleTransformVecValue) -> AzOptionStyleTransformVecValue,
        pub az_option_raw_image_delete: extern "C" fn(_:  &mut AzOptionRawImage),
        pub az_option_raw_image_deep_copy: extern "C" fn(_:  &AzOptionRawImage) -> AzOptionRawImage,
        pub az_option_wayland_theme_delete: extern "C" fn(_:  &mut AzOptionWaylandTheme),
        pub az_option_wayland_theme_deep_copy: extern "C" fn(_:  &AzOptionWaylandTheme) -> AzOptionWaylandTheme,
        pub az_option_task_bar_icon_delete: extern "C" fn(_:  &mut AzOptionTaskBarIcon),
        pub az_option_task_bar_icon_deep_copy: extern "C" fn(_:  &AzOptionTaskBarIcon) -> AzOptionTaskBarIcon,
        pub az_option_window_icon_delete: extern "C" fn(_:  &mut AzOptionWindowIcon),
        pub az_option_window_icon_deep_copy: extern "C" fn(_:  &AzOptionWindowIcon) -> AzOptionWindowIcon,
        pub az_option_string_delete: extern "C" fn(_:  &mut AzOptionString),
        pub az_option_string_deep_copy: extern "C" fn(_:  &AzOptionString) -> AzOptionString,
        pub az_option_dom_delete: extern "C" fn(_:  &mut AzOptionDom),
        pub az_option_dom_deep_copy: extern "C" fn(_:  &AzOptionDom) -> AzOptionDom,
        pub az_option_texture_delete: extern "C" fn(_:  &mut AzOptionTexture),
        pub az_option_image_mask_delete: extern "C" fn(_:  &mut AzOptionImageMask),
        pub az_option_image_mask_deep_copy: extern "C" fn(_:  &AzOptionImageMask) -> AzOptionImageMask,
        pub az_option_style_background_content_value_delete: extern "C" fn(_:  &mut AzOptionStyleBackgroundContentValue),
        pub az_option_style_background_content_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBackgroundContentValue) -> AzOptionStyleBackgroundContentValue,
        pub az_option_style_font_family_value_delete: extern "C" fn(_:  &mut AzOptionStyleFontFamilyValue),
        pub az_option_style_font_family_value_deep_copy: extern "C" fn(_:  &AzOptionStyleFontFamilyValue) -> AzOptionStyleFontFamilyValue,
        pub az_option_box_shadow_pre_display_item_value_delete: extern "C" fn(_:  &mut AzOptionBoxShadowPreDisplayItemValue),
        pub az_option_box_shadow_pre_display_item_value_deep_copy: extern "C" fn(_:  &AzOptionBoxShadowPreDisplayItemValue) -> AzOptionBoxShadowPreDisplayItemValue,
        pub az_option_instant_ptr_delete: extern "C" fn(_:  &mut AzOptionInstantPtr),
        pub az_option_instant_ptr_deep_copy: extern "C" fn(_:  &AzOptionInstantPtr) -> AzOptionInstantPtr,
        pub az_option_u8_vec_ref_delete: extern "C" fn(_:  &mut AzOptionU8VecRef),
        pub az_result_svg_svg_parse_error_delete: extern "C" fn(_:  &mut AzResultSvgSvgParseError),
        pub az_result_svg_svg_parse_error_deep_copy: extern "C" fn(_:  &AzResultSvgSvgParseError) -> AzResultSvgSvgParseError,
        pub az_svg_parse_error_delete: extern "C" fn(_:  &mut AzSvgParseError),
        pub az_svg_parse_error_deep_copy: extern "C" fn(_:  &AzSvgParseError) -> AzSvgParseError,
        pub az_xml_error_delete: extern "C" fn(_:  &mut AzXmlError),
        pub az_xml_error_deep_copy: extern "C" fn(_:  &AzXmlError) -> AzXmlError,
        pub az_duplicated_namespace_error_delete: extern "C" fn(_:  &mut AzDuplicatedNamespaceError),
        pub az_duplicated_namespace_error_deep_copy: extern "C" fn(_:  &AzDuplicatedNamespaceError) -> AzDuplicatedNamespaceError,
        pub az_unknown_namespace_error_delete: extern "C" fn(_:  &mut AzUnknownNamespaceError),
        pub az_unknown_namespace_error_deep_copy: extern "C" fn(_:  &AzUnknownNamespaceError) -> AzUnknownNamespaceError,
        pub az_unexpected_close_tag_error_delete: extern "C" fn(_:  &mut AzUnexpectedCloseTagError),
        pub az_unexpected_close_tag_error_deep_copy: extern "C" fn(_:  &AzUnexpectedCloseTagError) -> AzUnexpectedCloseTagError,
        pub az_unknown_entity_reference_error_delete: extern "C" fn(_:  &mut AzUnknownEntityReferenceError),
        pub az_unknown_entity_reference_error_deep_copy: extern "C" fn(_:  &AzUnknownEntityReferenceError) -> AzUnknownEntityReferenceError,
        pub az_duplicated_attribute_error_delete: extern "C" fn(_:  &mut AzDuplicatedAttributeError),
        pub az_duplicated_attribute_error_deep_copy: extern "C" fn(_:  &AzDuplicatedAttributeError) -> AzDuplicatedAttributeError,
        pub az_xml_parse_error_delete: extern "C" fn(_:  &mut AzXmlParseError),
        pub az_xml_parse_error_deep_copy: extern "C" fn(_:  &AzXmlParseError) -> AzXmlParseError,
        pub az_xml_text_error_delete: extern "C" fn(_:  &mut AzXmlTextError),
        pub az_xml_text_error_deep_copy: extern "C" fn(_:  &AzXmlTextError) -> AzXmlTextError,
        pub az_xml_stream_error_delete: extern "C" fn(_:  &mut AzXmlStreamError),
        pub az_xml_stream_error_deep_copy: extern "C" fn(_:  &AzXmlStreamError) -> AzXmlStreamError,
        pub az_invalid_char_multiple_error_delete: extern "C" fn(_:  &mut AzInvalidCharMultipleError),
        pub az_invalid_char_multiple_error_deep_copy: extern "C" fn(_:  &AzInvalidCharMultipleError) -> AzInvalidCharMultipleError,
        pub az_invalid_string_error_delete: extern "C" fn(_:  &mut AzInvalidStringError),
        pub az_invalid_string_error_deep_copy: extern "C" fn(_:  &AzInvalidStringError) -> AzInvalidStringError,
        pub az_instant_ptr_now: extern "C" fn() -> AzInstantPtr,
        pub az_instant_ptr_delete: extern "C" fn(_:  &mut AzInstantPtr),
        pub az_app_config_default: extern "C" fn() -> AzAppConfig,
        pub az_app_config_delete: extern "C" fn(_:  &mut AzAppConfig),
        pub az_app_config_deep_copy: extern "C" fn(_:  &AzAppConfig) -> AzAppConfig,
        pub az_app_ptr_new: extern "C" fn(_:  AzRefAny, _:  AzAppConfig) -> AzAppPtr,
        pub az_app_ptr_add_window: extern "C" fn(_:  &mut AzAppPtr, _:  AzWindowCreateOptions),
        pub az_app_ptr_run: extern "C" fn(_:  AzAppPtr, _:  AzWindowCreateOptions),
        pub az_app_ptr_delete: extern "C" fn(_:  &mut AzAppPtr),
        pub az_hidpi_adjusted_bounds_get_logical_size: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzLogicalSize,
        pub az_hidpi_adjusted_bounds_get_physical_size: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzPhysicalSizeU32,
        pub az_hidpi_adjusted_bounds_get_hidpi_factor: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> f32,
        pub az_focus_target_delete: extern "C" fn(_:  &mut AzFocusTarget),
        pub az_focus_target_deep_copy: extern "C" fn(_:  &AzFocusTarget) -> AzFocusTarget,
        pub az_focus_target_path_delete: extern "C" fn(_:  &mut AzFocusTargetPath),
        pub az_focus_target_path_deep_copy: extern "C" fn(_:  &AzFocusTargetPath) -> AzFocusTargetPath,
        pub az_callback_info_get_hit_node: extern "C" fn(_:  &AzCallbackInfo) -> AzDomNodeId,
        pub az_callback_info_get_cursor_relative_to_viewport: extern "C" fn(_:  &AzCallbackInfo) -> AzOptionLayoutPoint,
        pub az_callback_info_get_cursor_relative_to_node: extern "C" fn(_:  &AzCallbackInfo) -> AzOptionLayoutPoint,
        pub az_callback_info_get_parent: extern "C" fn(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId,
        pub az_callback_info_get_previous_sibling: extern "C" fn(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId,
        pub az_callback_info_get_next_sibling: extern "C" fn(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId,
        pub az_callback_info_get_first_child: extern "C" fn(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId,
        pub az_callback_info_get_last_child: extern "C" fn(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId,
        pub az_callback_info_get_dataset: extern "C" fn(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionRefAny,
        pub az_callback_info_get_window_state: extern "C" fn(_:  &AzCallbackInfo) -> AzWindowState,
        pub az_callback_info_get_keyboard_state: extern "C" fn(_:  &AzCallbackInfo) -> AzKeyboardState,
        pub az_callback_info_get_mouse_state: extern "C" fn(_:  &AzCallbackInfo) -> AzMouseState,
        pub az_callback_info_get_current_window_handle: extern "C" fn(_:  &AzCallbackInfo) -> AzRawWindowHandle,
        pub az_callback_info_get_gl_context: extern "C" fn(_:  &AzCallbackInfo) -> AzGlContextPtr,
        pub az_callback_info_set_window_state: extern "C" fn(_:  &mut AzCallbackInfo, _:  AzWindowState),
        pub az_callback_info_set_focus: extern "C" fn(_:  &mut AzCallbackInfo, _:  AzFocusTarget),
        pub az_callback_info_set_css_property: extern "C" fn(_:  &mut AzCallbackInfo, _:  AzDomNodeId, _:  AzCssProperty),
        pub az_callback_info_stop_propagation: extern "C" fn(_:  &mut AzCallbackInfo),
        pub az_callback_info_create_window: extern "C" fn(_:  &mut AzCallbackInfo, _:  AzWindowCreateOptions),
        pub az_callback_info_start_thread: extern "C" fn(_:  &mut AzCallbackInfo, _:  AzThreadId, _:  AzRefAny, _:  AzRefAny, _:  AzThreadCallbackType),
        pub az_callback_info_start_timer: extern "C" fn(_:  &mut AzCallbackInfo, _:  AzTimerId, _:  AzTimer),
        pub az_callback_info_delete: extern "C" fn(_:  &mut AzCallbackInfo),
        pub az_i_frame_callback_info_get_bounds: extern "C" fn(_:  &AzIFrameCallbackInfo) -> AzHidpiAdjustedBounds,
        pub az_i_frame_callback_info_delete: extern "C" fn(_:  &mut AzIFrameCallbackInfo),
        pub az_i_frame_callback_return_delete: extern "C" fn(_:  &mut AzIFrameCallbackReturn),
        pub az_i_frame_callback_return_deep_copy: extern "C" fn(_:  &AzIFrameCallbackReturn) -> AzIFrameCallbackReturn,
        pub az_gl_callback_info_get_gl_context: extern "C" fn(_:  &AzGlCallbackInfo) -> AzGlContextPtr,
        pub az_gl_callback_info_delete: extern "C" fn(_:  &mut AzGlCallbackInfo),
        pub az_gl_callback_return_delete: extern "C" fn(_:  &mut AzGlCallbackReturn),
        pub az_timer_callback_info_delete: extern "C" fn(_:  &mut AzTimerCallbackInfo),
        pub az_timer_callback_return_delete: extern "C" fn(_:  &mut AzTimerCallbackReturn),
        pub az_timer_callback_return_deep_copy: extern "C" fn(_:  &AzTimerCallbackReturn) -> AzTimerCallbackReturn,
        pub az_write_back_callback_delete: extern "C" fn(_:  &mut AzWriteBackCallback),
        pub az_write_back_callback_deep_copy: extern "C" fn(_:  &AzWriteBackCallback) -> AzWriteBackCallback,
        pub az_atomic_ref_count_can_be_shared: extern "C" fn(_:  &AzAtomicRefCount) -> bool,
        pub az_atomic_ref_count_can_be_shared_mut: extern "C" fn(_:  &AzAtomicRefCount) -> bool,
        pub az_atomic_ref_count_increase_ref: extern "C" fn(_:  &mut AzAtomicRefCount),
        pub az_atomic_ref_count_decrease_ref: extern "C" fn(_:  &mut AzAtomicRefCount),
        pub az_atomic_ref_count_increase_refmut: extern "C" fn(_:  &mut AzAtomicRefCount),
        pub az_atomic_ref_count_decrease_refmut: extern "C" fn(_:  &mut AzAtomicRefCount),
        pub az_atomic_ref_count_delete: extern "C" fn(_:  &mut AzAtomicRefCount),
        pub az_ref_any_new_c: extern "C" fn(_:  *const c_void, _:  usize, _:  u64, _:  AzString, _:  AzRefAnyDestructorType) -> AzRefAny,
        pub az_ref_any_is_type: extern "C" fn(_:  &AzRefAny, _:  u64) -> bool,
        pub az_ref_any_get_type_name: extern "C" fn(_:  &AzRefAny) -> AzString,
        pub az_ref_any_can_be_shared: extern "C" fn(_:  &AzRefAny) -> bool,
        pub az_ref_any_can_be_shared_mut: extern "C" fn(_:  &AzRefAny) -> bool,
        pub az_ref_any_increase_ref: extern "C" fn(_:  &AzRefAny),
        pub az_ref_any_decrease_ref: extern "C" fn(_:  &AzRefAny),
        pub az_ref_any_increase_refmut: extern "C" fn(_:  &AzRefAny),
        pub az_ref_any_decrease_refmut: extern "C" fn(_:  &AzRefAny),
        pub az_ref_any_delete: extern "C" fn(_:  &mut AzRefAny),
        pub az_ref_any_deep_copy: extern "C" fn(_:  &AzRefAny) -> AzRefAny,
        pub az_layout_info_window_width_larger_than: extern "C" fn(_:  &mut AzLayoutInfo, _:  f32) -> bool,
        pub az_layout_info_window_width_smaller_than: extern "C" fn(_:  &mut AzLayoutInfo, _:  f32) -> bool,
        pub az_layout_info_window_height_larger_than: extern "C" fn(_:  &mut AzLayoutInfo, _:  f32) -> bool,
        pub az_layout_info_window_height_smaller_than: extern "C" fn(_:  &mut AzLayoutInfo, _:  f32) -> bool,
        pub az_layout_info_delete: extern "C" fn(_:  &mut AzLayoutInfo),
        pub az_css_rule_block_delete: extern "C" fn(_:  &mut AzCssRuleBlock),
        pub az_css_rule_block_deep_copy: extern "C" fn(_:  &AzCssRuleBlock) -> AzCssRuleBlock,
        pub az_css_declaration_delete: extern "C" fn(_:  &mut AzCssDeclaration),
        pub az_css_declaration_deep_copy: extern "C" fn(_:  &AzCssDeclaration) -> AzCssDeclaration,
        pub az_dynamic_css_property_delete: extern "C" fn(_:  &mut AzDynamicCssProperty),
        pub az_dynamic_css_property_deep_copy: extern "C" fn(_:  &AzDynamicCssProperty) -> AzDynamicCssProperty,
        pub az_css_path_delete: extern "C" fn(_:  &mut AzCssPath),
        pub az_css_path_deep_copy: extern "C" fn(_:  &AzCssPath) -> AzCssPath,
        pub az_css_path_selector_delete: extern "C" fn(_:  &mut AzCssPathSelector),
        pub az_css_path_selector_deep_copy: extern "C" fn(_:  &AzCssPathSelector) -> AzCssPathSelector,
        pub az_stylesheet_delete: extern "C" fn(_:  &mut AzStylesheet),
        pub az_stylesheet_deep_copy: extern "C" fn(_:  &AzStylesheet) -> AzStylesheet,
        pub az_css_empty: extern "C" fn() -> AzCss,
        pub az_css_from_string: extern "C" fn(_:  AzString) -> AzCss,
        pub az_css_delete: extern "C" fn(_:  &mut AzCss),
        pub az_css_deep_copy: extern "C" fn(_:  &AzCss) -> AzCss,
        pub az_color_u_from_str: extern "C" fn(_:  AzString) -> AzColorU,
        pub az_color_u_to_hash: extern "C" fn(_:  &AzColorU) -> AzString,
        pub az_linear_gradient_delete: extern "C" fn(_:  &mut AzLinearGradient),
        pub az_linear_gradient_deep_copy: extern "C" fn(_:  &AzLinearGradient) -> AzLinearGradient,
        pub az_radial_gradient_delete: extern "C" fn(_:  &mut AzRadialGradient),
        pub az_radial_gradient_deep_copy: extern "C" fn(_:  &AzRadialGradient) -> AzRadialGradient,
        pub az_css_image_id_delete: extern "C" fn(_:  &mut AzCssImageId),
        pub az_css_image_id_deep_copy: extern "C" fn(_:  &AzCssImageId) -> AzCssImageId,
        pub az_style_background_content_delete: extern "C" fn(_:  &mut AzStyleBackgroundContent),
        pub az_style_background_content_deep_copy: extern "C" fn(_:  &AzStyleBackgroundContent) -> AzStyleBackgroundContent,
        pub az_style_font_family_delete: extern "C" fn(_:  &mut AzStyleFontFamily),
        pub az_style_font_family_deep_copy: extern "C" fn(_:  &AzStyleFontFamily) -> AzStyleFontFamily,
        pub az_style_background_content_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundContentValue),
        pub az_style_background_content_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue,
        pub az_style_font_family_value_delete: extern "C" fn(_:  &mut AzStyleFontFamilyValue),
        pub az_style_font_family_value_deep_copy: extern "C" fn(_:  &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue,
        pub az_style_transform_vec_value_delete: extern "C" fn(_:  &mut AzStyleTransformVecValue),
        pub az_style_transform_vec_value_deep_copy: extern "C" fn(_:  &AzStyleTransformVecValue) -> AzStyleTransformVecValue,
        pub az_css_property_delete: extern "C" fn(_:  &mut AzCssProperty),
        pub az_css_property_deep_copy: extern "C" fn(_:  &AzCssProperty) -> AzCssProperty,
        pub az_rect_style_delete: extern "C" fn(_:  &mut AzRectStyle),
        pub az_rect_style_deep_copy: extern "C" fn(_:  &AzRectStyle) -> AzRectStyle,
        pub az_rect_layout_delete: extern "C" fn(_:  &mut AzRectLayout),
        pub az_rect_layout_deep_copy: extern "C" fn(_:  &AzRectLayout) -> AzRectLayout,
        pub az_cascaded_css_property_with_source_delete: extern "C" fn(_:  &mut AzCascadedCssPropertyWithSource),
        pub az_cascaded_css_property_with_source_deep_copy: extern "C" fn(_:  &AzCascadedCssPropertyWithSource) -> AzCascadedCssPropertyWithSource,
        pub az_css_property_source_delete: extern "C" fn(_:  &mut AzCssPropertySource),
        pub az_css_property_source_deep_copy: extern "C" fn(_:  &AzCssPropertySource) -> AzCssPropertySource,
        pub az_styled_node_delete: extern "C" fn(_:  &mut AzStyledNode),
        pub az_styled_node_deep_copy: extern "C" fn(_:  &AzStyledNode) -> AzStyledNode,
        pub az_content_group_delete: extern "C" fn(_:  &mut AzContentGroup),
        pub az_content_group_deep_copy: extern "C" fn(_:  &AzContentGroup) -> AzContentGroup,
        pub az_styled_dom_new: extern "C" fn(_:  AzDom, _:  AzCss) -> AzStyledDom,
        pub az_styled_dom_append: extern "C" fn(_:  &mut AzStyledDom, _:  AzStyledDom),
        pub az_styled_dom_delete: extern "C" fn(_:  &mut AzStyledDom),
        pub az_styled_dom_deep_copy: extern "C" fn(_:  &AzStyledDom) -> AzStyledDom,
        pub az_dom_new: extern "C" fn(_:  AzNodeType) -> AzDom,
        pub az_dom_div: extern "C" fn() -> AzDom,
        pub az_dom_body: extern "C" fn() -> AzDom,
        pub az_dom_label: extern "C" fn(_:  AzString) -> AzDom,
        pub az_dom_text: extern "C" fn(_:  AzTextId) -> AzDom,
        pub az_dom_image: extern "C" fn(_:  AzImageId) -> AzDom,
        pub az_dom_gl_texture: extern "C" fn(_:  AzRefAny, _:  AzGlCallbackType) -> AzDom,
        pub az_dom_iframe: extern "C" fn(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzDom,
        pub az_dom_add_id: extern "C" fn(_:  &mut AzDom, _:  AzString),
        pub az_dom_with_id: extern "C" fn(_:  AzDom, _:  AzString) -> AzDom,
        pub az_dom_set_ids: extern "C" fn(_:  &mut AzDom, _:  AzStringVec),
        pub az_dom_with_ids: extern "C" fn(_:  AzDom, _:  AzStringVec) -> AzDom,
        pub az_dom_add_class: extern "C" fn(_:  &mut AzDom, _:  AzString),
        pub az_dom_with_class: extern "C" fn(_:  AzDom, _:  AzString) -> AzDom,
        pub az_dom_set_classes: extern "C" fn(_:  &mut AzDom, _:  AzStringVec),
        pub az_dom_with_classes: extern "C" fn(_:  AzDom, _:  AzStringVec) -> AzDom,
        pub az_dom_add_callback: extern "C" fn(_:  &mut AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType),
        pub az_dom_with_callback: extern "C" fn(_:  AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzDom,
        pub az_dom_set_dataset: extern "C" fn(_:  &mut AzDom, _:  AzRefAny),
        pub az_dom_with_dataset: extern "C" fn(_:  AzDom, _:  AzRefAny) -> AzDom,
        pub az_dom_add_inline_css: extern "C" fn(_:  &mut AzDom, _:  AzCssProperty),
        pub az_dom_with_inline_css: extern "C" fn(_:  AzDom, _:  AzCssProperty) -> AzDom,
        pub az_dom_add_inline_hover_css: extern "C" fn(_:  &mut AzDom, _:  AzCssProperty),
        pub az_dom_with_inline_hover_css: extern "C" fn(_:  AzDom, _:  AzCssProperty) -> AzDom,
        pub az_dom_add_inline_active_css: extern "C" fn(_:  &mut AzDom, _:  AzCssProperty),
        pub az_dom_with_inline_active_css: extern "C" fn(_:  AzDom, _:  AzCssProperty) -> AzDom,
        pub az_dom_add_inline_focus_css: extern "C" fn(_:  &mut AzDom, _:  AzCssProperty),
        pub az_dom_with_inline_focus_css: extern "C" fn(_:  AzDom, _:  AzCssProperty) -> AzDom,
        pub az_dom_set_is_draggable: extern "C" fn(_:  &mut AzDom, _:  bool),
        pub az_dom_with_clip_mask: extern "C" fn(_:  AzDom, _:  AzOptionImageMask) -> AzDom,
        pub az_dom_set_clip_mask: extern "C" fn(_:  &mut AzDom, _:  AzOptionImageMask),
        pub az_dom_is_draggable: extern "C" fn(_:  AzDom, _:  bool) -> AzDom,
        pub az_dom_set_tab_index: extern "C" fn(_:  &mut AzDom, _:  AzOptionTabIndex),
        pub az_dom_with_tab_index: extern "C" fn(_:  AzDom, _:  AzOptionTabIndex) -> AzDom,
        pub az_dom_add_child: extern "C" fn(_:  &mut AzDom, _:  AzDom),
        pub az_dom_with_child: extern "C" fn(_:  AzDom, _:  AzDom) -> AzDom,
        pub az_dom_get_html_string: extern "C" fn(_:  &AzDom) -> AzString,
        pub az_dom_style: extern "C" fn(_:  AzDom, _:  AzCss) -> AzStyledDom,
        pub az_dom_delete: extern "C" fn(_:  &mut AzDom),
        pub az_dom_deep_copy: extern "C" fn(_:  &AzDom) -> AzDom,
        pub az_gl_texture_node_delete: extern "C" fn(_:  &mut AzGlTextureNode),
        pub az_gl_texture_node_deep_copy: extern "C" fn(_:  &AzGlTextureNode) -> AzGlTextureNode,
        pub az_i_frame_node_delete: extern "C" fn(_:  &mut AzIFrameNode),
        pub az_i_frame_node_deep_copy: extern "C" fn(_:  &AzIFrameNode) -> AzIFrameNode,
        pub az_callback_data_delete: extern "C" fn(_:  &mut AzCallbackData),
        pub az_callback_data_deep_copy: extern "C" fn(_:  &AzCallbackData) -> AzCallbackData,
        pub az_image_mask_delete: extern "C" fn(_:  &mut AzImageMask),
        pub az_image_mask_deep_copy: extern "C" fn(_:  &AzImageMask) -> AzImageMask,
        pub az_node_data_new: extern "C" fn(_:  AzNodeType) -> AzNodeData,
        pub az_node_data_div: extern "C" fn() -> AzNodeData,
        pub az_node_data_body: extern "C" fn() -> AzNodeData,
        pub az_node_data_label: extern "C" fn(_:  AzString) -> AzNodeData,
        pub az_node_data_text: extern "C" fn(_:  AzTextId) -> AzNodeData,
        pub az_node_data_image: extern "C" fn(_:  AzImageId) -> AzNodeData,
        pub az_node_data_gl_texture: extern "C" fn(_:  AzRefAny, _:  AzGlCallbackType) -> AzNodeData,
        pub az_node_data_iframe: extern "C" fn(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzNodeData,
        pub az_node_data_default: extern "C" fn() -> AzNodeData,
        pub az_node_data_add_id: extern "C" fn(_:  &mut AzNodeData, _:  AzString),
        pub az_node_data_with_id: extern "C" fn(_:  AzNodeData, _:  AzString) -> AzNodeData,
        pub az_node_data_set_ids: extern "C" fn(_:  &mut AzNodeData, _:  AzStringVec),
        pub az_node_data_with_ids: extern "C" fn(_:  AzNodeData, _:  AzStringVec) -> AzNodeData,
        pub az_node_data_add_class: extern "C" fn(_:  &mut AzNodeData, _:  AzString),
        pub az_node_data_with_class: extern "C" fn(_:  AzNodeData, _:  AzString) -> AzNodeData,
        pub az_node_data_set_classes: extern "C" fn(_:  &mut AzNodeData, _:  AzStringVec),
        pub az_node_data_with_classes: extern "C" fn(_:  AzNodeData, _:  AzStringVec) -> AzNodeData,
        pub az_node_data_add_dataset: extern "C" fn(_:  &mut AzNodeData, _:  AzRefAny),
        pub az_node_data_with_dataset: extern "C" fn(_:  AzNodeData, _:  AzRefAny) -> AzNodeData,
        pub az_node_data_add_callback: extern "C" fn(_:  &mut AzNodeData, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType),
        pub az_node_data_with_callback: extern "C" fn(_:  AzNodeData, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzNodeData,
        pub az_node_data_add_inline_css: extern "C" fn(_:  &mut AzNodeData, _:  AzCssProperty),
        pub az_node_data_with_inline_css: extern "C" fn(_:  AzNodeData, _:  AzCssProperty) -> AzNodeData,
        pub az_node_data_add_inline_hover_css: extern "C" fn(_:  &mut AzNodeData, _:  AzCssProperty),
        pub az_node_data_add_inline_active_css: extern "C" fn(_:  &mut AzNodeData, _:  AzCssProperty),
        pub az_node_data_add_inline_focus_css: extern "C" fn(_:  &mut AzNodeData, _:  AzCssProperty),
        pub az_node_data_with_clip_mask: extern "C" fn(_:  AzNodeData, _:  AzOptionImageMask) -> AzNodeData,
        pub az_node_data_set_clip_mask: extern "C" fn(_:  &mut AzNodeData, _:  AzOptionImageMask),
        pub az_node_data_set_is_draggable: extern "C" fn(_:  &mut AzNodeData, _:  bool),
        pub az_node_data_is_draggable: extern "C" fn(_:  AzNodeData, _:  bool) -> AzNodeData,
        pub az_node_data_set_tab_index: extern "C" fn(_:  &mut AzNodeData, _:  AzOptionTabIndex),
        pub az_node_data_with_tab_index: extern "C" fn(_:  AzNodeData, _:  AzOptionTabIndex) -> AzNodeData,
        pub az_node_data_delete: extern "C" fn(_:  &mut AzNodeData),
        pub az_node_data_deep_copy: extern "C" fn(_:  &AzNodeData) -> AzNodeData,
        pub az_node_type_delete: extern "C" fn(_:  &mut AzNodeType),
        pub az_node_type_deep_copy: extern "C" fn(_:  &AzNodeType) -> AzNodeType,
        pub az_on_into_event_filter: extern "C" fn(_:  AzOn) -> AzEventFilter,
        pub az_vertex_attribute_delete: extern "C" fn(_:  &mut AzVertexAttribute),
        pub az_vertex_attribute_deep_copy: extern "C" fn(_:  &AzVertexAttribute) -> AzVertexAttribute,
        pub az_vertex_layout_delete: extern "C" fn(_:  &mut AzVertexLayout),
        pub az_vertex_layout_deep_copy: extern "C" fn(_:  &AzVertexLayout) -> AzVertexLayout,
        pub az_vertex_array_object_delete: extern "C" fn(_:  &mut AzVertexArrayObject),
        pub az_vertex_buffer_delete: extern "C" fn(_:  &mut AzVertexBuffer),
        pub az_debug_message_delete: extern "C" fn(_:  &mut AzDebugMessage),
        pub az_debug_message_deep_copy: extern "C" fn(_:  &AzDebugMessage) -> AzDebugMessage,
        pub az_u8_vec_ref_delete: extern "C" fn(_:  &mut AzU8VecRef),
        pub az_u8_vec_ref_mut_delete: extern "C" fn(_:  &mut AzU8VecRefMut),
        pub az_f32_vec_ref_delete: extern "C" fn(_:  &mut AzF32VecRef),
        pub az_i32_vec_ref_delete: extern "C" fn(_:  &mut AzI32VecRef),
        pub az_g_luint_vec_ref_delete: extern "C" fn(_:  &mut AzGLuintVecRef),
        pub az_g_lenum_vec_ref_delete: extern "C" fn(_:  &mut AzGLenumVecRef),
        pub az_g_lint_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLintVecRefMut),
        pub az_g_lint64_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLint64VecRefMut),
        pub az_g_lboolean_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLbooleanVecRefMut),
        pub az_g_lfloat_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLfloatVecRefMut),
        pub az_refstr_vec_ref_delete: extern "C" fn(_:  &mut AzRefstrVecRef),
        pub az_refstr_delete: extern "C" fn(_:  &mut AzRefstr),
        pub az_get_program_binary_return_delete: extern "C" fn(_:  &mut AzGetProgramBinaryReturn),
        pub az_get_program_binary_return_deep_copy: extern "C" fn(_:  &AzGetProgramBinaryReturn) -> AzGetProgramBinaryReturn,
        pub az_get_active_attrib_return_delete: extern "C" fn(_:  &mut AzGetActiveAttribReturn),
        pub az_get_active_attrib_return_deep_copy: extern "C" fn(_:  &AzGetActiveAttribReturn) -> AzGetActiveAttribReturn,
        pub az_g_lsync_ptr_delete: extern "C" fn(_:  &mut AzGLsyncPtr),
        pub az_get_active_uniform_return_delete: extern "C" fn(_:  &mut AzGetActiveUniformReturn),
        pub az_get_active_uniform_return_deep_copy: extern "C" fn(_:  &AzGetActiveUniformReturn) -> AzGetActiveUniformReturn,
        pub az_gl_context_ptr_get_type: extern "C" fn(_:  &AzGlContextPtr) -> AzGlType,
        pub az_gl_context_ptr_buffer_data_untyped: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  *const c_void, _:  u32),
        pub az_gl_context_ptr_buffer_sub_data_untyped: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  *const c_void),
        pub az_gl_context_ptr_map_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> *mut c_void,
        pub az_gl_context_ptr_map_buffer_range: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  u32) -> *mut c_void,
        pub az_gl_context_ptr_unmap_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8,
        pub az_gl_context_ptr_tex_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_shader_source: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzStringVec),
        pub az_gl_context_ptr_read_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_read_pixels_into_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut),
        pub az_gl_context_ptr_read_pixels: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32) -> AzU8Vec,
        pub az_gl_context_ptr_read_pixels_into_pbo: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32),
        pub az_gl_context_ptr_sample_coverage: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  bool),
        pub az_gl_context_ptr_polygon_offset: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  f32),
        pub az_gl_context_ptr_pixel_store_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32),
        pub az_gl_context_ptr_gen_buffers: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_gen_renderbuffers: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_gen_framebuffers: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_gen_textures: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_gen_vertex_arrays: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_gen_queries: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_begin_query: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_end_query: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_query_counter: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_get_query_object_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32,
        pub az_gl_context_ptr_get_query_object_uiv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u32,
        pub az_gl_context_ptr_get_query_object_i64v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i64,
        pub az_gl_context_ptr_get_query_object_ui64v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u64,
        pub az_gl_context_ptr_delete_queries: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_delete_vertex_arrays: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_delete_buffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_delete_renderbuffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_delete_framebuffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_delete_textures: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_framebuffer_renderbuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_renderbuffer_storage: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32),
        pub az_gl_context_ptr_depth_func: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_active_texture: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_attach_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_bind_attrib_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr),
        pub az_gl_context_ptr_get_uniform_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLintVecRefMut),
        pub az_gl_context_ptr_get_uniform_fv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLfloatVecRefMut),
        pub az_gl_context_ptr_get_uniform_block_index: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> u32,
        pub az_gl_context_ptr_get_uniform_indices: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstrVecRef) -> AzGLuintVec,
        pub az_gl_context_ptr_bind_buffer_base: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_bind_buffer_range: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  isize, _:  isize),
        pub az_gl_context_ptr_uniform_block_binding: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_bind_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_bind_vertex_array: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_bind_renderbuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_bind_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_bind_texture: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_draw_buffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLenumVecRef),
        pub az_gl_context_ptr_tex_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef),
        pub az_gl_context_ptr_compressed_tex_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  AzU8VecRef),
        pub az_gl_context_ptr_compressed_tex_sub_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  AzU8VecRef),
        pub az_gl_context_ptr_tex_image_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef),
        pub az_gl_context_ptr_copy_tex_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_copy_tex_sub_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_copy_tex_sub_image_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_tex_sub_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef),
        pub az_gl_context_ptr_tex_sub_image_2d_pbo: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize),
        pub az_gl_context_ptr_tex_sub_image_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef),
        pub az_gl_context_ptr_tex_sub_image_3d_pbo: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize),
        pub az_gl_context_ptr_tex_storage_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32),
        pub az_gl_context_ptr_tex_storage_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_get_tex_image_into_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut),
        pub az_gl_context_ptr_copy_image_sub_data: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_invalidate_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef),
        pub az_gl_context_ptr_invalidate_sub_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_get_integer_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLintVecRefMut),
        pub az_gl_context_ptr_get_integer_64v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLint64VecRefMut),
        pub az_gl_context_ptr_get_integer_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut),
        pub az_gl_context_ptr_get_integer_64iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLint64VecRefMut),
        pub az_gl_context_ptr_get_boolean_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLbooleanVecRefMut),
        pub az_gl_context_ptr_get_float_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLfloatVecRefMut),
        pub az_gl_context_ptr_get_framebuffer_attachment_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32,
        pub az_gl_context_ptr_get_renderbuffer_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32,
        pub az_gl_context_ptr_get_tex_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32,
        pub az_gl_context_ptr_get_tex_parameter_fv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> f32,
        pub az_gl_context_ptr_tex_parameter_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32),
        pub az_gl_context_ptr_tex_parameter_f: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  f32),
        pub az_gl_context_ptr_framebuffer_texture_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  i32),
        pub az_gl_context_ptr_framebuffer_texture_layer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  i32, _:  i32),
        pub az_gl_context_ptr_blit_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32),
        pub az_gl_context_ptr_vertex_attrib_4f: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  f32, _:  f32, _:  f32, _:  f32),
        pub az_gl_context_ptr_vertex_attrib_pointer_f32: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  bool, _:  i32, _:  u32),
        pub az_gl_context_ptr_vertex_attrib_pointer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  bool, _:  i32, _:  u32),
        pub az_gl_context_ptr_vertex_attrib_i_pointer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  u32),
        pub az_gl_context_ptr_vertex_attrib_divisor: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_viewport: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_scissor: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_line_width: extern "C" fn(_:  &AzGlContextPtr, _:  f32),
        pub az_gl_context_ptr_use_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_validate_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_draw_arrays: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32),
        pub az_gl_context_ptr_draw_arrays_instanced: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_draw_elements: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32),
        pub az_gl_context_ptr_draw_elements_instanced: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32),
        pub az_gl_context_ptr_blend_color: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32),
        pub az_gl_context_ptr_blend_func: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_blend_func_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_blend_equation: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_blend_equation_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_color_mask: extern "C" fn(_:  &AzGlContextPtr, _:  bool, _:  bool, _:  bool, _:  bool),
        pub az_gl_context_ptr_cull_face: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_front_face: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_enable: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_disable: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_hint: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_is_enabled: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8,
        pub az_gl_context_ptr_is_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8,
        pub az_gl_context_ptr_is_texture: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8,
        pub az_gl_context_ptr_is_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8,
        pub az_gl_context_ptr_is_renderbuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8,
        pub az_gl_context_ptr_check_frame_buffer_status: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u32,
        pub az_gl_context_ptr_enable_vertex_attrib_array: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_disable_vertex_attrib_array: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_uniform_1f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32),
        pub az_gl_context_ptr_uniform_1fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef),
        pub az_gl_context_ptr_uniform_1i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32),
        pub az_gl_context_ptr_uniform_1iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef),
        pub az_gl_context_ptr_uniform_1ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32),
        pub az_gl_context_ptr_uniform_2f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32),
        pub az_gl_context_ptr_uniform_2fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef),
        pub az_gl_context_ptr_uniform_2i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_uniform_2iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef),
        pub az_gl_context_ptr_uniform_2ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32),
        pub az_gl_context_ptr_uniform_3f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32),
        pub az_gl_context_ptr_uniform_3fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef),
        pub az_gl_context_ptr_uniform_3i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_uniform_3iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef),
        pub az_gl_context_ptr_uniform_3ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_uniform_4f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32, _:  f32),
        pub az_gl_context_ptr_uniform_4i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_uniform_4iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef),
        pub az_gl_context_ptr_uniform_4ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_uniform_4fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef),
        pub az_gl_context_ptr_uniform_matrix_2fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef),
        pub az_gl_context_ptr_uniform_matrix_3fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef),
        pub az_gl_context_ptr_uniform_matrix_4fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef),
        pub az_gl_context_ptr_depth_mask: extern "C" fn(_:  &AzGlContextPtr, _:  bool),
        pub az_gl_context_ptr_depth_range: extern "C" fn(_:  &AzGlContextPtr, _:  f64, _:  f64),
        pub az_gl_context_ptr_get_active_attrib: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveAttribReturn,
        pub az_gl_context_ptr_get_active_uniform: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveUniformReturn,
        pub az_gl_context_ptr_get_active_uniforms_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLuintVec, _:  u32) -> AzGLintVec,
        pub az_gl_context_ptr_get_active_uniform_block_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32,
        pub az_gl_context_ptr_get_active_uniform_block_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> AzGLintVec,
        pub az_gl_context_ptr_get_active_uniform_block_name: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString,
        pub az_gl_context_ptr_get_attrib_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32,
        pub az_gl_context_ptr_get_frag_data_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32,
        pub az_gl_context_ptr_get_uniform_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32,
        pub az_gl_context_ptr_get_program_info_log: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzString,
        pub az_gl_context_ptr_get_program_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut),
        pub az_gl_context_ptr_get_program_binary: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzGetProgramBinaryReturn,
        pub az_gl_context_ptr_program_binary: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzU8VecRef),
        pub az_gl_context_ptr_program_parameter_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32),
        pub az_gl_context_ptr_get_vertex_attrib_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut),
        pub az_gl_context_ptr_get_vertex_attrib_fv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLfloatVecRefMut),
        pub az_gl_context_ptr_get_vertex_attrib_pointer_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> isize,
        pub az_gl_context_ptr_get_buffer_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32,
        pub az_gl_context_ptr_get_shader_info_log: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzString,
        pub az_gl_context_ptr_get_string: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzString,
        pub az_gl_context_ptr_get_string_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString,
        pub az_gl_context_ptr_get_shader_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut),
        pub az_gl_context_ptr_get_shader_precision_format: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGlShaderPrecisionFormatReturn,
        pub az_gl_context_ptr_compile_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_create_program: extern "C" fn(_:  &AzGlContextPtr) -> u32,
        pub az_gl_context_ptr_delete_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_create_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u32,
        pub az_gl_context_ptr_delete_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_detach_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_link_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_clear_color: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32),
        pub az_gl_context_ptr_clear: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_clear_depth: extern "C" fn(_:  &AzGlContextPtr, _:  f64),
        pub az_gl_context_ptr_clear_stencil: extern "C" fn(_:  &AzGlContextPtr, _:  i32),
        pub az_gl_context_ptr_flush: extern "C" fn(_:  &AzGlContextPtr),
        pub az_gl_context_ptr_finish: extern "C" fn(_:  &AzGlContextPtr),
        pub az_gl_context_ptr_get_error: extern "C" fn(_:  &AzGlContextPtr) -> u32,
        pub az_gl_context_ptr_stencil_mask: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_stencil_mask_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_stencil_func: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32),
        pub az_gl_context_ptr_stencil_func_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  u32),
        pub az_gl_context_ptr_stencil_op: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_stencil_op_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_egl_image_target_texture2d_oes: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  *const c_void),
        pub az_gl_context_ptr_generate_mipmap: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_insert_event_marker_ext: extern "C" fn(_:  &AzGlContextPtr, _:  AzRefstr),
        pub az_gl_context_ptr_push_group_marker_ext: extern "C" fn(_:  &AzGlContextPtr, _:  AzRefstr),
        pub az_gl_context_ptr_pop_group_marker_ext: extern "C" fn(_:  &AzGlContextPtr),
        pub az_gl_context_ptr_debug_message_insert_khr: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  AzRefstr),
        pub az_gl_context_ptr_push_debug_group_khr: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr),
        pub az_gl_context_ptr_pop_debug_group_khr: extern "C" fn(_:  &AzGlContextPtr),
        pub az_gl_context_ptr_fence_sync: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGLsyncPtr,
        pub az_gl_context_ptr_client_wait_sync: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64),
        pub az_gl_context_ptr_wait_sync: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64),
        pub az_gl_context_ptr_delete_sync: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr),
        pub az_gl_context_ptr_texture_range_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzU8VecRef),
        pub az_gl_context_ptr_gen_fences_apple: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_delete_fences_apple: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_set_fence_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_finish_fence_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_test_fence_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_test_object_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u8,
        pub az_gl_context_ptr_finish_object_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_get_frag_data_index: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32,
        pub az_gl_context_ptr_blend_barrier_khr: extern "C" fn(_:  &AzGlContextPtr),
        pub az_gl_context_ptr_bind_frag_data_location_indexed: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  AzRefstr),
        pub az_gl_context_ptr_get_debug_messages: extern "C" fn(_:  &AzGlContextPtr) -> AzDebugMessageVec,
        pub az_gl_context_ptr_provoking_vertex_angle: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_gen_vertex_arrays_apple: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_bind_vertex_array_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_delete_vertex_arrays_apple: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_copy_texture_chromium: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8),
        pub az_gl_context_ptr_copy_sub_texture_chromium: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8),
        pub az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  *const c_void),
        pub az_gl_context_ptr_copy_texture_3d_angle: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8),
        pub az_gl_context_ptr_copy_sub_texture_3d_angle: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8),
        pub az_gl_context_ptr_delete: extern "C" fn(_:  &mut AzGlContextPtr),
        pub az_gl_context_ptr_deep_copy: extern "C" fn(_:  &AzGlContextPtr) -> AzGlContextPtr,
        pub az_texture_delete: extern "C" fn(_:  &mut AzTexture),
        pub az_texture_flags_default: extern "C" fn() -> AzTextureFlags,
        pub az_raw_image_format_delete: extern "C" fn(_:  &mut AzRawImageFormat),
        pub az_raw_image_format_deep_copy: extern "C" fn(_:  &AzRawImageFormat) -> AzRawImageFormat,
        pub az_text_id_new: extern "C" fn() -> AzTextId,
        pub az_image_id_new: extern "C" fn() -> AzImageId,
        pub az_font_id_new: extern "C" fn() -> AzFontId,
        pub az_image_source_delete: extern "C" fn(_:  &mut AzImageSource),
        pub az_image_source_deep_copy: extern "C" fn(_:  &AzImageSource) -> AzImageSource,
        pub az_font_source_delete: extern "C" fn(_:  &mut AzFontSource),
        pub az_font_source_deep_copy: extern "C" fn(_:  &AzFontSource) -> AzFontSource,
        pub az_raw_image_new: extern "C" fn(_:  AzU8Vec, _:  usize, _:  usize, _:  AzRawImageFormat) -> AzRawImage,
        pub az_raw_image_delete: extern "C" fn(_:  &mut AzRawImage),
        pub az_raw_image_deep_copy: extern "C" fn(_:  &AzRawImage) -> AzRawImage,
        pub az_svg_multi_polygon_delete: extern "C" fn(_:  &mut AzSvgMultiPolygon),
        pub az_svg_multi_polygon_deep_copy: extern "C" fn(_:  &AzSvgMultiPolygon) -> AzSvgMultiPolygon,
        pub az_svg_node_delete: extern "C" fn(_:  &mut AzSvgNode),
        pub az_svg_node_deep_copy: extern "C" fn(_:  &AzSvgNode) -> AzSvgNode,
        pub az_svg_styled_node_delete: extern "C" fn(_:  &mut AzSvgStyledNode),
        pub az_svg_styled_node_deep_copy: extern "C" fn(_:  &AzSvgStyledNode) -> AzSvgStyledNode,
        pub az_svg_path_delete: extern "C" fn(_:  &mut AzSvgPath),
        pub az_svg_path_deep_copy: extern "C" fn(_:  &AzSvgPath) -> AzSvgPath,
        pub az_tesselated_cpu_svg_node_delete: extern "C" fn(_:  &mut AzTesselatedCPUSvgNode),
        pub az_tesselated_cpu_svg_node_deep_copy: extern "C" fn(_:  &AzTesselatedCPUSvgNode) -> AzTesselatedCPUSvgNode,
        pub az_svg_parse_options_default: extern "C" fn() -> AzSvgParseOptions,
        pub az_svg_parse_options_delete: extern "C" fn(_:  &mut AzSvgParseOptions),
        pub az_svg_parse_options_deep_copy: extern "C" fn(_:  &AzSvgParseOptions) -> AzSvgParseOptions,
        pub az_svg_render_options_default: extern "C" fn() -> AzSvgRenderOptions,
        pub az_svg_render_options_delete: extern "C" fn(_:  &mut AzSvgRenderOptions),
        pub az_svg_render_options_deep_copy: extern "C" fn(_:  &AzSvgRenderOptions) -> AzSvgRenderOptions,
        pub az_svg_parse: extern "C" fn(_:  AzU8VecRef, _:  AzSvgParseOptions) -> AzResultSvgSvgParseError,
        pub az_svg_delete: extern "C" fn(_:  &mut AzSvg),
        pub az_svg_deep_copy: extern "C" fn(_:  &AzSvg) -> AzSvg,
        pub az_svg_xml_node_delete: extern "C" fn(_:  &mut AzSvgXmlNode),
        pub az_svg_xml_node_deep_copy: extern "C" fn(_:  &AzSvgXmlNode) -> AzSvgXmlNode,
        pub az_timer_delete: extern "C" fn(_:  &mut AzTimer),
        pub az_timer_deep_copy: extern "C" fn(_:  &AzTimer) -> AzTimer,
        pub az_thread_sender_send: extern "C" fn(_:  &mut AzThreadSender, _:  AzThreadReceiveMsg) -> bool,
        pub az_thread_sender_delete: extern "C" fn(_:  &mut AzThreadSender),
        pub az_thread_receiver_receive: extern "C" fn(_:  &mut AzThreadReceiver) -> AzOptionThreadSendMsg,
        pub az_thread_receiver_delete: extern "C" fn(_:  &mut AzThreadReceiver),
        pub az_thread_receive_msg_delete: extern "C" fn(_:  &mut AzThreadReceiveMsg),
        pub az_thread_write_back_msg_delete: extern "C" fn(_:  &mut AzThreadWriteBackMsg),
        pub az_task_bar_icon_delete: extern "C" fn(_:  &mut AzTaskBarIcon),
        pub az_task_bar_icon_deep_copy: extern "C" fn(_:  &AzTaskBarIcon) -> AzTaskBarIcon,
        pub az_small_window_icon_bytes_delete: extern "C" fn(_:  &mut AzSmallWindowIconBytes),
        pub az_small_window_icon_bytes_deep_copy: extern "C" fn(_:  &AzSmallWindowIconBytes) -> AzSmallWindowIconBytes,
        pub az_large_window_icon_bytes_delete: extern "C" fn(_:  &mut AzLargeWindowIconBytes),
        pub az_large_window_icon_bytes_deep_copy: extern "C" fn(_:  &AzLargeWindowIconBytes) -> AzLargeWindowIconBytes,
        pub az_window_icon_delete: extern "C" fn(_:  &mut AzWindowIcon),
        pub az_window_icon_deep_copy: extern "C" fn(_:  &AzWindowIcon) -> AzWindowIcon,
        pub az_debug_state_delete: extern "C" fn(_:  &mut AzDebugState),
        pub az_debug_state_deep_copy: extern "C" fn(_:  &AzDebugState) -> AzDebugState,
        pub az_keyboard_state_delete: extern "C" fn(_:  &mut AzKeyboardState),
        pub az_keyboard_state_deep_copy: extern "C" fn(_:  &AzKeyboardState) -> AzKeyboardState,
        pub az_mouse_state_delete: extern "C" fn(_:  &mut AzMouseState),
        pub az_mouse_state_deep_copy: extern "C" fn(_:  &AzMouseState) -> AzMouseState,
        pub az_platform_specific_options_delete: extern "C" fn(_:  &mut AzPlatformSpecificOptions),
        pub az_platform_specific_options_deep_copy: extern "C" fn(_:  &AzPlatformSpecificOptions) -> AzPlatformSpecificOptions,
        pub az_windows_window_options_delete: extern "C" fn(_:  &mut AzWindowsWindowOptions),
        pub az_windows_window_options_deep_copy: extern "C" fn(_:  &AzWindowsWindowOptions) -> AzWindowsWindowOptions,
        pub az_wayland_theme_delete: extern "C" fn(_:  &mut AzWaylandTheme),
        pub az_wayland_theme_deep_copy: extern "C" fn(_:  &AzWaylandTheme) -> AzWaylandTheme,
        pub az_renderer_type_delete: extern "C" fn(_:  &mut AzRendererType),
        pub az_renderer_type_deep_copy: extern "C" fn(_:  &AzRendererType) -> AzRendererType,
        pub az_string_pair_delete: extern "C" fn(_:  &mut AzStringPair),
        pub az_string_pair_deep_copy: extern "C" fn(_:  &AzStringPair) -> AzStringPair,
        pub az_linux_window_options_delete: extern "C" fn(_:  &mut AzLinuxWindowOptions),
        pub az_linux_window_options_deep_copy: extern "C" fn(_:  &AzLinuxWindowOptions) -> AzLinuxWindowOptions,
        pub az_window_state_new: extern "C" fn(_:  AzLayoutCallbackType) -> AzWindowState,
        pub az_window_state_delete: extern "C" fn(_:  &mut AzWindowState),
        pub az_window_state_deep_copy: extern "C" fn(_:  &AzWindowState) -> AzWindowState,
        pub az_window_create_options_new: extern "C" fn(_:  AzLayoutCallbackType) -> AzWindowCreateOptions,
        pub az_window_create_options_delete: extern "C" fn(_:  &mut AzWindowCreateOptions),
        pub az_window_create_options_deep_copy: extern "C" fn(_:  &AzWindowCreateOptions) -> AzWindowCreateOptions,
    }

    pub fn initialize_library(path: &std::path::Path) -> Option<AzulDll> {
        use std::mem::transmute;
        let lib = Library::new(path)?;
        unsafe {
            let az_string_from_utf8_unchecked: extern "C" fn(_:  *const u8, _:  usize) -> AzString = transmute(lib.get(b"az_string_from_utf8_unchecked")?);
            let az_string_from_utf8_lossy: extern "C" fn(_:  *const u8, _:  usize) -> AzString = transmute(lib.get(b"az_string_from_utf8_lossy")?);
            let az_string_into_bytes: extern "C" fn(_:  AzString) -> AzU8Vec = transmute(lib.get(b"az_string_into_bytes")?);
            let az_string_delete: extern "C" fn(_:  &mut AzString) = transmute(lib.get(b"az_string_delete")?);
            let az_string_deep_copy: extern "C" fn(_:  &AzString) -> AzString = transmute(lib.get(b"az_string_deep_copy")?);
            let az_style_transform_vec_new: extern "C" fn() -> AzStyleTransformVec = transmute(lib.get(b"az_style_transform_vec_new")?);
            let az_style_transform_vec_with_capacity: extern "C" fn(_:  usize) -> AzStyleTransformVec = transmute(lib.get(b"az_style_transform_vec_with_capacity")?);
            let az_style_transform_vec_copy_from: extern "C" fn(_:  *const AzStyleTransform, _:  usize) -> AzStyleTransformVec = transmute(lib.get(b"az_style_transform_vec_copy_from")?);
            let az_style_transform_vec_delete: extern "C" fn(_:  &mut AzStyleTransformVec) = transmute(lib.get(b"az_style_transform_vec_delete")?);
            let az_style_transform_vec_deep_copy: extern "C" fn(_:  &AzStyleTransformVec) -> AzStyleTransformVec = transmute(lib.get(b"az_style_transform_vec_deep_copy")?);
            let az_content_group_vec_new: extern "C" fn() -> AzContentGroupVec = transmute(lib.get(b"az_content_group_vec_new")?);
            let az_content_group_vec_with_capacity: extern "C" fn(_:  usize) -> AzContentGroupVec = transmute(lib.get(b"az_content_group_vec_with_capacity")?);
            let az_content_group_vec_copy_from: extern "C" fn(_:  *const AzContentGroup, _:  usize) -> AzContentGroupVec = transmute(lib.get(b"az_content_group_vec_copy_from")?);
            let az_content_group_vec_delete: extern "C" fn(_:  &mut AzContentGroupVec) = transmute(lib.get(b"az_content_group_vec_delete")?);
            let az_content_group_vec_deep_copy: extern "C" fn(_:  &AzContentGroupVec) -> AzContentGroupVec = transmute(lib.get(b"az_content_group_vec_deep_copy")?);
            let az_css_property_vec_new: extern "C" fn() -> AzCssPropertyVec = transmute(lib.get(b"az_css_property_vec_new")?);
            let az_css_property_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssPropertyVec = transmute(lib.get(b"az_css_property_vec_with_capacity")?);
            let az_css_property_vec_copy_from: extern "C" fn(_:  *const AzCssProperty, _:  usize) -> AzCssPropertyVec = transmute(lib.get(b"az_css_property_vec_copy_from")?);
            let az_css_property_vec_delete: extern "C" fn(_:  &mut AzCssPropertyVec) = transmute(lib.get(b"az_css_property_vec_delete")?);
            let az_css_property_vec_deep_copy: extern "C" fn(_:  &AzCssPropertyVec) -> AzCssPropertyVec = transmute(lib.get(b"az_css_property_vec_deep_copy")?);
            let az_svg_multi_polygon_vec_new: extern "C" fn() -> AzSvgMultiPolygonVec = transmute(lib.get(b"az_svg_multi_polygon_vec_new")?);
            let az_svg_multi_polygon_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgMultiPolygonVec = transmute(lib.get(b"az_svg_multi_polygon_vec_with_capacity")?);
            let az_svg_multi_polygon_vec_copy_from: extern "C" fn(_:  *const AzSvgMultiPolygon, _:  usize) -> AzSvgMultiPolygonVec = transmute(lib.get(b"az_svg_multi_polygon_vec_copy_from")?);
            let az_svg_multi_polygon_vec_delete: extern "C" fn(_:  &mut AzSvgMultiPolygonVec) = transmute(lib.get(b"az_svg_multi_polygon_vec_delete")?);
            let az_svg_multi_polygon_vec_deep_copy: extern "C" fn(_:  &AzSvgMultiPolygonVec) -> AzSvgMultiPolygonVec = transmute(lib.get(b"az_svg_multi_polygon_vec_deep_copy")?);
            let az_svg_path_vec_new: extern "C" fn() -> AzSvgPathVec = transmute(lib.get(b"az_svg_path_vec_new")?);
            let az_svg_path_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgPathVec = transmute(lib.get(b"az_svg_path_vec_with_capacity")?);
            let az_svg_path_vec_copy_from: extern "C" fn(_:  *const AzSvgPath, _:  usize) -> AzSvgPathVec = transmute(lib.get(b"az_svg_path_vec_copy_from")?);
            let az_svg_path_vec_delete: extern "C" fn(_:  &mut AzSvgPathVec) = transmute(lib.get(b"az_svg_path_vec_delete")?);
            let az_svg_path_vec_deep_copy: extern "C" fn(_:  &AzSvgPathVec) -> AzSvgPathVec = transmute(lib.get(b"az_svg_path_vec_deep_copy")?);
            let az_vertex_attribute_vec_new: extern "C" fn() -> AzVertexAttributeVec = transmute(lib.get(b"az_vertex_attribute_vec_new")?);
            let az_vertex_attribute_vec_with_capacity: extern "C" fn(_:  usize) -> AzVertexAttributeVec = transmute(lib.get(b"az_vertex_attribute_vec_with_capacity")?);
            let az_vertex_attribute_vec_copy_from: extern "C" fn(_:  *const AzVertexAttribute, _:  usize) -> AzVertexAttributeVec = transmute(lib.get(b"az_vertex_attribute_vec_copy_from")?);
            let az_vertex_attribute_vec_delete: extern "C" fn(_:  &mut AzVertexAttributeVec) = transmute(lib.get(b"az_vertex_attribute_vec_delete")?);
            let az_vertex_attribute_vec_deep_copy: extern "C" fn(_:  &AzVertexAttributeVec) -> AzVertexAttributeVec = transmute(lib.get(b"az_vertex_attribute_vec_deep_copy")?);
            let az_svg_path_element_vec_new: extern "C" fn() -> AzSvgPathElementVec = transmute(lib.get(b"az_svg_path_element_vec_new")?);
            let az_svg_path_element_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgPathElementVec = transmute(lib.get(b"az_svg_path_element_vec_with_capacity")?);
            let az_svg_path_element_vec_copy_from: extern "C" fn(_:  *const AzSvgPathElement, _:  usize) -> AzSvgPathElementVec = transmute(lib.get(b"az_svg_path_element_vec_copy_from")?);
            let az_svg_path_element_vec_delete: extern "C" fn(_:  &mut AzSvgPathElementVec) = transmute(lib.get(b"az_svg_path_element_vec_delete")?);
            let az_svg_path_element_vec_deep_copy: extern "C" fn(_:  &AzSvgPathElementVec) -> AzSvgPathElementVec = transmute(lib.get(b"az_svg_path_element_vec_deep_copy")?);
            let az_svg_vertex_vec_new: extern "C" fn() -> AzSvgVertexVec = transmute(lib.get(b"az_svg_vertex_vec_new")?);
            let az_svg_vertex_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgVertexVec = transmute(lib.get(b"az_svg_vertex_vec_with_capacity")?);
            let az_svg_vertex_vec_copy_from: extern "C" fn(_:  *const AzSvgVertex, _:  usize) -> AzSvgVertexVec = transmute(lib.get(b"az_svg_vertex_vec_copy_from")?);
            let az_svg_vertex_vec_delete: extern "C" fn(_:  &mut AzSvgVertexVec) = transmute(lib.get(b"az_svg_vertex_vec_delete")?);
            let az_svg_vertex_vec_deep_copy: extern "C" fn(_:  &AzSvgVertexVec) -> AzSvgVertexVec = transmute(lib.get(b"az_svg_vertex_vec_deep_copy")?);
            let az_u32_vec_new: extern "C" fn() -> AzU32Vec = transmute(lib.get(b"az_u32_vec_new")?);
            let az_u32_vec_with_capacity: extern "C" fn(_:  usize) -> AzU32Vec = transmute(lib.get(b"az_u32_vec_with_capacity")?);
            let az_u32_vec_copy_from: extern "C" fn(_:  *const u32, _:  usize) -> AzU32Vec = transmute(lib.get(b"az_u32_vec_copy_from")?);
            let az_u32_vec_delete: extern "C" fn(_:  &mut AzU32Vec) = transmute(lib.get(b"az_u32_vec_delete")?);
            let az_u32_vec_deep_copy: extern "C" fn(_:  &AzU32Vec) -> AzU32Vec = transmute(lib.get(b"az_u32_vec_deep_copy")?);
            let az_x_window_type_vec_new: extern "C" fn() -> AzXWindowTypeVec = transmute(lib.get(b"az_x_window_type_vec_new")?);
            let az_x_window_type_vec_with_capacity: extern "C" fn(_:  usize) -> AzXWindowTypeVec = transmute(lib.get(b"az_x_window_type_vec_with_capacity")?);
            let az_x_window_type_vec_copy_from: extern "C" fn(_:  *const AzXWindowType, _:  usize) -> AzXWindowTypeVec = transmute(lib.get(b"az_x_window_type_vec_copy_from")?);
            let az_x_window_type_vec_delete: extern "C" fn(_:  &mut AzXWindowTypeVec) = transmute(lib.get(b"az_x_window_type_vec_delete")?);
            let az_x_window_type_vec_deep_copy: extern "C" fn(_:  &AzXWindowTypeVec) -> AzXWindowTypeVec = transmute(lib.get(b"az_x_window_type_vec_deep_copy")?);
            let az_virtual_key_code_vec_new: extern "C" fn() -> AzVirtualKeyCodeVec = transmute(lib.get(b"az_virtual_key_code_vec_new")?);
            let az_virtual_key_code_vec_with_capacity: extern "C" fn(_:  usize) -> AzVirtualKeyCodeVec = transmute(lib.get(b"az_virtual_key_code_vec_with_capacity")?);
            let az_virtual_key_code_vec_copy_from: extern "C" fn(_:  *const AzVirtualKeyCode, _:  usize) -> AzVirtualKeyCodeVec = transmute(lib.get(b"az_virtual_key_code_vec_copy_from")?);
            let az_virtual_key_code_vec_delete: extern "C" fn(_:  &mut AzVirtualKeyCodeVec) = transmute(lib.get(b"az_virtual_key_code_vec_delete")?);
            let az_virtual_key_code_vec_deep_copy: extern "C" fn(_:  &AzVirtualKeyCodeVec) -> AzVirtualKeyCodeVec = transmute(lib.get(b"az_virtual_key_code_vec_deep_copy")?);
            let az_cascade_info_vec_new: extern "C" fn() -> AzCascadeInfoVec = transmute(lib.get(b"az_cascade_info_vec_new")?);
            let az_cascade_info_vec_with_capacity: extern "C" fn(_:  usize) -> AzCascadeInfoVec = transmute(lib.get(b"az_cascade_info_vec_with_capacity")?);
            let az_cascade_info_vec_copy_from: extern "C" fn(_:  *const AzCascadeInfo, _:  usize) -> AzCascadeInfoVec = transmute(lib.get(b"az_cascade_info_vec_copy_from")?);
            let az_cascade_info_vec_delete: extern "C" fn(_:  &mut AzCascadeInfoVec) = transmute(lib.get(b"az_cascade_info_vec_delete")?);
            let az_cascade_info_vec_deep_copy: extern "C" fn(_:  &AzCascadeInfoVec) -> AzCascadeInfoVec = transmute(lib.get(b"az_cascade_info_vec_deep_copy")?);
            let az_scan_code_vec_new: extern "C" fn() -> AzScanCodeVec = transmute(lib.get(b"az_scan_code_vec_new")?);
            let az_scan_code_vec_with_capacity: extern "C" fn(_:  usize) -> AzScanCodeVec = transmute(lib.get(b"az_scan_code_vec_with_capacity")?);
            let az_scan_code_vec_copy_from: extern "C" fn(_:  *const u32, _:  usize) -> AzScanCodeVec = transmute(lib.get(b"az_scan_code_vec_copy_from")?);
            let az_scan_code_vec_delete: extern "C" fn(_:  &mut AzScanCodeVec) = transmute(lib.get(b"az_scan_code_vec_delete")?);
            let az_scan_code_vec_deep_copy: extern "C" fn(_:  &AzScanCodeVec) -> AzScanCodeVec = transmute(lib.get(b"az_scan_code_vec_deep_copy")?);
            let az_css_declaration_vec_new: extern "C" fn() -> AzCssDeclarationVec = transmute(lib.get(b"az_css_declaration_vec_new")?);
            let az_css_declaration_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssDeclarationVec = transmute(lib.get(b"az_css_declaration_vec_with_capacity")?);
            let az_css_declaration_vec_copy_from: extern "C" fn(_:  *const AzCssDeclaration, _:  usize) -> AzCssDeclarationVec = transmute(lib.get(b"az_css_declaration_vec_copy_from")?);
            let az_css_declaration_vec_delete: extern "C" fn(_:  &mut AzCssDeclarationVec) = transmute(lib.get(b"az_css_declaration_vec_delete")?);
            let az_css_declaration_vec_deep_copy: extern "C" fn(_:  &AzCssDeclarationVec) -> AzCssDeclarationVec = transmute(lib.get(b"az_css_declaration_vec_deep_copy")?);
            let az_css_path_selector_vec_new: extern "C" fn() -> AzCssPathSelectorVec = transmute(lib.get(b"az_css_path_selector_vec_new")?);
            let az_css_path_selector_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssPathSelectorVec = transmute(lib.get(b"az_css_path_selector_vec_with_capacity")?);
            let az_css_path_selector_vec_copy_from: extern "C" fn(_:  *const AzCssPathSelector, _:  usize) -> AzCssPathSelectorVec = transmute(lib.get(b"az_css_path_selector_vec_copy_from")?);
            let az_css_path_selector_vec_delete: extern "C" fn(_:  &mut AzCssPathSelectorVec) = transmute(lib.get(b"az_css_path_selector_vec_delete")?);
            let az_css_path_selector_vec_deep_copy: extern "C" fn(_:  &AzCssPathSelectorVec) -> AzCssPathSelectorVec = transmute(lib.get(b"az_css_path_selector_vec_deep_copy")?);
            let az_stylesheet_vec_new: extern "C" fn() -> AzStylesheetVec = transmute(lib.get(b"az_stylesheet_vec_new")?);
            let az_stylesheet_vec_with_capacity: extern "C" fn(_:  usize) -> AzStylesheetVec = transmute(lib.get(b"az_stylesheet_vec_with_capacity")?);
            let az_stylesheet_vec_copy_from: extern "C" fn(_:  *const AzStylesheet, _:  usize) -> AzStylesheetVec = transmute(lib.get(b"az_stylesheet_vec_copy_from")?);
            let az_stylesheet_vec_delete: extern "C" fn(_:  &mut AzStylesheetVec) = transmute(lib.get(b"az_stylesheet_vec_delete")?);
            let az_stylesheet_vec_deep_copy: extern "C" fn(_:  &AzStylesheetVec) -> AzStylesheetVec = transmute(lib.get(b"az_stylesheet_vec_deep_copy")?);
            let az_css_rule_block_vec_new: extern "C" fn() -> AzCssRuleBlockVec = transmute(lib.get(b"az_css_rule_block_vec_new")?);
            let az_css_rule_block_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssRuleBlockVec = transmute(lib.get(b"az_css_rule_block_vec_with_capacity")?);
            let az_css_rule_block_vec_copy_from: extern "C" fn(_:  *const AzCssRuleBlock, _:  usize) -> AzCssRuleBlockVec = transmute(lib.get(b"az_css_rule_block_vec_copy_from")?);
            let az_css_rule_block_vec_delete: extern "C" fn(_:  &mut AzCssRuleBlockVec) = transmute(lib.get(b"az_css_rule_block_vec_delete")?);
            let az_css_rule_block_vec_deep_copy: extern "C" fn(_:  &AzCssRuleBlockVec) -> AzCssRuleBlockVec = transmute(lib.get(b"az_css_rule_block_vec_deep_copy")?);
            let az_u8_vec_new: extern "C" fn() -> AzU8Vec = transmute(lib.get(b"az_u8_vec_new")?);
            let az_u8_vec_with_capacity: extern "C" fn(_:  usize) -> AzU8Vec = transmute(lib.get(b"az_u8_vec_with_capacity")?);
            let az_u8_vec_copy_from: extern "C" fn(_:  *const u8, _:  usize) -> AzU8Vec = transmute(lib.get(b"az_u8_vec_copy_from")?);
            let az_u8_vec_delete: extern "C" fn(_:  &mut AzU8Vec) = transmute(lib.get(b"az_u8_vec_delete")?);
            let az_u8_vec_deep_copy: extern "C" fn(_:  &AzU8Vec) -> AzU8Vec = transmute(lib.get(b"az_u8_vec_deep_copy")?);
            let az_callback_data_vec_new: extern "C" fn() -> AzCallbackDataVec = transmute(lib.get(b"az_callback_data_vec_new")?);
            let az_callback_data_vec_with_capacity: extern "C" fn(_:  usize) -> AzCallbackDataVec = transmute(lib.get(b"az_callback_data_vec_with_capacity")?);
            let az_callback_data_vec_copy_from: extern "C" fn(_:  *const AzCallbackData, _:  usize) -> AzCallbackDataVec = transmute(lib.get(b"az_callback_data_vec_copy_from")?);
            let az_callback_data_vec_delete: extern "C" fn(_:  &mut AzCallbackDataVec) = transmute(lib.get(b"az_callback_data_vec_delete")?);
            let az_callback_data_vec_deep_copy: extern "C" fn(_:  &AzCallbackDataVec) -> AzCallbackDataVec = transmute(lib.get(b"az_callback_data_vec_deep_copy")?);
            let az_debug_message_vec_new: extern "C" fn() -> AzDebugMessageVec = transmute(lib.get(b"az_debug_message_vec_new")?);
            let az_debug_message_vec_with_capacity: extern "C" fn(_:  usize) -> AzDebugMessageVec = transmute(lib.get(b"az_debug_message_vec_with_capacity")?);
            let az_debug_message_vec_copy_from: extern "C" fn(_:  *const AzDebugMessage, _:  usize) -> AzDebugMessageVec = transmute(lib.get(b"az_debug_message_vec_copy_from")?);
            let az_debug_message_vec_delete: extern "C" fn(_:  &mut AzDebugMessageVec) = transmute(lib.get(b"az_debug_message_vec_delete")?);
            let az_debug_message_vec_deep_copy: extern "C" fn(_:  &AzDebugMessageVec) -> AzDebugMessageVec = transmute(lib.get(b"az_debug_message_vec_deep_copy")?);
            let az_g_luint_vec_new: extern "C" fn() -> AzGLuintVec = transmute(lib.get(b"az_g_luint_vec_new")?);
            let az_g_luint_vec_with_capacity: extern "C" fn(_:  usize) -> AzGLuintVec = transmute(lib.get(b"az_g_luint_vec_with_capacity")?);
            let az_g_luint_vec_copy_from: extern "C" fn(_:  *const u32, _:  usize) -> AzGLuintVec = transmute(lib.get(b"az_g_luint_vec_copy_from")?);
            let az_g_luint_vec_delete: extern "C" fn(_:  &mut AzGLuintVec) = transmute(lib.get(b"az_g_luint_vec_delete")?);
            let az_g_luint_vec_deep_copy: extern "C" fn(_:  &AzGLuintVec) -> AzGLuintVec = transmute(lib.get(b"az_g_luint_vec_deep_copy")?);
            let az_g_lint_vec_new: extern "C" fn() -> AzGLintVec = transmute(lib.get(b"az_g_lint_vec_new")?);
            let az_g_lint_vec_with_capacity: extern "C" fn(_:  usize) -> AzGLintVec = transmute(lib.get(b"az_g_lint_vec_with_capacity")?);
            let az_g_lint_vec_copy_from: extern "C" fn(_:  *const i32, _:  usize) -> AzGLintVec = transmute(lib.get(b"az_g_lint_vec_copy_from")?);
            let az_g_lint_vec_delete: extern "C" fn(_:  &mut AzGLintVec) = transmute(lib.get(b"az_g_lint_vec_delete")?);
            let az_g_lint_vec_deep_copy: extern "C" fn(_:  &AzGLintVec) -> AzGLintVec = transmute(lib.get(b"az_g_lint_vec_deep_copy")?);
            let az_dom_vec_new: extern "C" fn() -> AzDomVec = transmute(lib.get(b"az_dom_vec_new")?);
            let az_dom_vec_with_capacity: extern "C" fn(_:  usize) -> AzDomVec = transmute(lib.get(b"az_dom_vec_with_capacity")?);
            let az_dom_vec_copy_from: extern "C" fn(_:  *const AzDom, _:  usize) -> AzDomVec = transmute(lib.get(b"az_dom_vec_copy_from")?);
            let az_dom_vec_delete: extern "C" fn(_:  &mut AzDomVec) = transmute(lib.get(b"az_dom_vec_delete")?);
            let az_dom_vec_deep_copy: extern "C" fn(_:  &AzDomVec) -> AzDomVec = transmute(lib.get(b"az_dom_vec_deep_copy")?);
            let az_string_vec_new: extern "C" fn() -> AzStringVec = transmute(lib.get(b"az_string_vec_new")?);
            let az_string_vec_with_capacity: extern "C" fn(_:  usize) -> AzStringVec = transmute(lib.get(b"az_string_vec_with_capacity")?);
            let az_string_vec_copy_from: extern "C" fn(_:  *const AzString, _:  usize) -> AzStringVec = transmute(lib.get(b"az_string_vec_copy_from")?);
            let az_string_vec_delete: extern "C" fn(_:  &mut AzStringVec) = transmute(lib.get(b"az_string_vec_delete")?);
            let az_string_vec_deep_copy: extern "C" fn(_:  &AzStringVec) -> AzStringVec = transmute(lib.get(b"az_string_vec_deep_copy")?);
            let az_string_pair_vec_new: extern "C" fn() -> AzStringPairVec = transmute(lib.get(b"az_string_pair_vec_new")?);
            let az_string_pair_vec_with_capacity: extern "C" fn(_:  usize) -> AzStringPairVec = transmute(lib.get(b"az_string_pair_vec_with_capacity")?);
            let az_string_pair_vec_copy_from: extern "C" fn(_:  *const AzStringPair, _:  usize) -> AzStringPairVec = transmute(lib.get(b"az_string_pair_vec_copy_from")?);
            let az_string_pair_vec_delete: extern "C" fn(_:  &mut AzStringPairVec) = transmute(lib.get(b"az_string_pair_vec_delete")?);
            let az_string_pair_vec_deep_copy: extern "C" fn(_:  &AzStringPairVec) -> AzStringPairVec = transmute(lib.get(b"az_string_pair_vec_deep_copy")?);
            let az_gradient_stop_pre_vec_new: extern "C" fn() -> AzGradientStopPreVec = transmute(lib.get(b"az_gradient_stop_pre_vec_new")?);
            let az_gradient_stop_pre_vec_with_capacity: extern "C" fn(_:  usize) -> AzGradientStopPreVec = transmute(lib.get(b"az_gradient_stop_pre_vec_with_capacity")?);
            let az_gradient_stop_pre_vec_copy_from: extern "C" fn(_:  *const AzGradientStopPre, _:  usize) -> AzGradientStopPreVec = transmute(lib.get(b"az_gradient_stop_pre_vec_copy_from")?);
            let az_gradient_stop_pre_vec_delete: extern "C" fn(_:  &mut AzGradientStopPreVec) = transmute(lib.get(b"az_gradient_stop_pre_vec_delete")?);
            let az_gradient_stop_pre_vec_deep_copy: extern "C" fn(_:  &AzGradientStopPreVec) -> AzGradientStopPreVec = transmute(lib.get(b"az_gradient_stop_pre_vec_deep_copy")?);
            let az_cascaded_css_property_with_source_vec_new: extern "C" fn() -> AzCascadedCssPropertyWithSourceVec = transmute(lib.get(b"az_cascaded_css_property_with_source_vec_new")?);
            let az_cascaded_css_property_with_source_vec_with_capacity: extern "C" fn(_:  usize) -> AzCascadedCssPropertyWithSourceVec = transmute(lib.get(b"az_cascaded_css_property_with_source_vec_with_capacity")?);
            let az_cascaded_css_property_with_source_vec_copy_from: extern "C" fn(_:  *const AzCascadedCssPropertyWithSource, _:  usize) -> AzCascadedCssPropertyWithSourceVec = transmute(lib.get(b"az_cascaded_css_property_with_source_vec_copy_from")?);
            let az_cascaded_css_property_with_source_vec_delete: extern "C" fn(_:  &mut AzCascadedCssPropertyWithSourceVec) = transmute(lib.get(b"az_cascaded_css_property_with_source_vec_delete")?);
            let az_cascaded_css_property_with_source_vec_deep_copy: extern "C" fn(_:  &AzCascadedCssPropertyWithSourceVec) -> AzCascadedCssPropertyWithSourceVec = transmute(lib.get(b"az_cascaded_css_property_with_source_vec_deep_copy")?);
            let az_node_id_vec_new: extern "C" fn() -> AzNodeIdVec = transmute(lib.get(b"az_node_id_vec_new")?);
            let az_node_id_vec_with_capacity: extern "C" fn(_:  usize) -> AzNodeIdVec = transmute(lib.get(b"az_node_id_vec_with_capacity")?);
            let az_node_id_vec_copy_from: extern "C" fn(_:  *const AzNodeId, _:  usize) -> AzNodeIdVec = transmute(lib.get(b"az_node_id_vec_copy_from")?);
            let az_node_id_vec_delete: extern "C" fn(_:  &mut AzNodeIdVec) = transmute(lib.get(b"az_node_id_vec_delete")?);
            let az_node_id_vec_deep_copy: extern "C" fn(_:  &AzNodeIdVec) -> AzNodeIdVec = transmute(lib.get(b"az_node_id_vec_deep_copy")?);
            let az_node_vec_new: extern "C" fn() -> AzNodeVec = transmute(lib.get(b"az_node_vec_new")?);
            let az_node_vec_with_capacity: extern "C" fn(_:  usize) -> AzNodeVec = transmute(lib.get(b"az_node_vec_with_capacity")?);
            let az_node_vec_copy_from: extern "C" fn(_:  *const AzNode, _:  usize) -> AzNodeVec = transmute(lib.get(b"az_node_vec_copy_from")?);
            let az_node_vec_delete: extern "C" fn(_:  &mut AzNodeVec) = transmute(lib.get(b"az_node_vec_delete")?);
            let az_node_vec_deep_copy: extern "C" fn(_:  &AzNodeVec) -> AzNodeVec = transmute(lib.get(b"az_node_vec_deep_copy")?);
            let az_styled_node_vec_new: extern "C" fn() -> AzStyledNodeVec = transmute(lib.get(b"az_styled_node_vec_new")?);
            let az_styled_node_vec_with_capacity: extern "C" fn(_:  usize) -> AzStyledNodeVec = transmute(lib.get(b"az_styled_node_vec_with_capacity")?);
            let az_styled_node_vec_copy_from: extern "C" fn(_:  *const AzStyledNode, _:  usize) -> AzStyledNodeVec = transmute(lib.get(b"az_styled_node_vec_copy_from")?);
            let az_styled_node_vec_delete: extern "C" fn(_:  &mut AzStyledNodeVec) = transmute(lib.get(b"az_styled_node_vec_delete")?);
            let az_styled_node_vec_deep_copy: extern "C" fn(_:  &AzStyledNodeVec) -> AzStyledNodeVec = transmute(lib.get(b"az_styled_node_vec_deep_copy")?);
            let az_tag_ids_to_node_ids_mapping_vec_new: extern "C" fn() -> AzTagIdsToNodeIdsMappingVec = transmute(lib.get(b"az_tag_ids_to_node_ids_mapping_vec_new")?);
            let az_tag_ids_to_node_ids_mapping_vec_with_capacity: extern "C" fn(_:  usize) -> AzTagIdsToNodeIdsMappingVec = transmute(lib.get(b"az_tag_ids_to_node_ids_mapping_vec_with_capacity")?);
            let az_tag_ids_to_node_ids_mapping_vec_copy_from: extern "C" fn(_:  *const AzTagIdToNodeIdMapping, _:  usize) -> AzTagIdsToNodeIdsMappingVec = transmute(lib.get(b"az_tag_ids_to_node_ids_mapping_vec_copy_from")?);
            let az_tag_ids_to_node_ids_mapping_vec_delete: extern "C" fn(_:  &mut AzTagIdsToNodeIdsMappingVec) = transmute(lib.get(b"az_tag_ids_to_node_ids_mapping_vec_delete")?);
            let az_tag_ids_to_node_ids_mapping_vec_deep_copy: extern "C" fn(_:  &AzTagIdsToNodeIdsMappingVec) -> AzTagIdsToNodeIdsMappingVec = transmute(lib.get(b"az_tag_ids_to_node_ids_mapping_vec_deep_copy")?);
            let az_parent_with_node_depth_vec_new: extern "C" fn() -> AzParentWithNodeDepthVec = transmute(lib.get(b"az_parent_with_node_depth_vec_new")?);
            let az_parent_with_node_depth_vec_with_capacity: extern "C" fn(_:  usize) -> AzParentWithNodeDepthVec = transmute(lib.get(b"az_parent_with_node_depth_vec_with_capacity")?);
            let az_parent_with_node_depth_vec_copy_from: extern "C" fn(_:  *const AzParentWithNodeDepth, _:  usize) -> AzParentWithNodeDepthVec = transmute(lib.get(b"az_parent_with_node_depth_vec_copy_from")?);
            let az_parent_with_node_depth_vec_delete: extern "C" fn(_:  &mut AzParentWithNodeDepthVec) = transmute(lib.get(b"az_parent_with_node_depth_vec_delete")?);
            let az_parent_with_node_depth_vec_deep_copy: extern "C" fn(_:  &AzParentWithNodeDepthVec) -> AzParentWithNodeDepthVec = transmute(lib.get(b"az_parent_with_node_depth_vec_deep_copy")?);
            let az_node_data_vec_new: extern "C" fn() -> AzNodeDataVec = transmute(lib.get(b"az_node_data_vec_new")?);
            let az_node_data_vec_with_capacity: extern "C" fn(_:  usize) -> AzNodeDataVec = transmute(lib.get(b"az_node_data_vec_with_capacity")?);
            let az_node_data_vec_copy_from: extern "C" fn(_:  *const AzNodeData, _:  usize) -> AzNodeDataVec = transmute(lib.get(b"az_node_data_vec_copy_from")?);
            let az_node_data_vec_delete: extern "C" fn(_:  &mut AzNodeDataVec) = transmute(lib.get(b"az_node_data_vec_delete")?);
            let az_node_data_vec_deep_copy: extern "C" fn(_:  &AzNodeDataVec) -> AzNodeDataVec = transmute(lib.get(b"az_node_data_vec_deep_copy")?);
            let az_option_ref_any_delete: extern "C" fn(_:  &mut AzOptionRefAny) = transmute(lib.get(b"az_option_ref_any_delete")?);
            let az_option_ref_any_deep_copy: extern "C" fn(_:  &AzOptionRefAny) -> AzOptionRefAny = transmute(lib.get(b"az_option_ref_any_deep_copy")?);
            let az_option_style_transform_vec_value_delete: extern "C" fn(_:  &mut AzOptionStyleTransformVecValue) = transmute(lib.get(b"az_option_style_transform_vec_value_delete")?);
            let az_option_style_transform_vec_value_deep_copy: extern "C" fn(_:  &AzOptionStyleTransformVecValue) -> AzOptionStyleTransformVecValue = transmute(lib.get(b"az_option_style_transform_vec_value_deep_copy")?);
            let az_option_raw_image_delete: extern "C" fn(_:  &mut AzOptionRawImage) = transmute(lib.get(b"az_option_raw_image_delete")?);
            let az_option_raw_image_deep_copy: extern "C" fn(_:  &AzOptionRawImage) -> AzOptionRawImage = transmute(lib.get(b"az_option_raw_image_deep_copy")?);
            let az_option_wayland_theme_delete: extern "C" fn(_:  &mut AzOptionWaylandTheme) = transmute(lib.get(b"az_option_wayland_theme_delete")?);
            let az_option_wayland_theme_deep_copy: extern "C" fn(_:  &AzOptionWaylandTheme) -> AzOptionWaylandTheme = transmute(lib.get(b"az_option_wayland_theme_deep_copy")?);
            let az_option_task_bar_icon_delete: extern "C" fn(_:  &mut AzOptionTaskBarIcon) = transmute(lib.get(b"az_option_task_bar_icon_delete")?);
            let az_option_task_bar_icon_deep_copy: extern "C" fn(_:  &AzOptionTaskBarIcon) -> AzOptionTaskBarIcon = transmute(lib.get(b"az_option_task_bar_icon_deep_copy")?);
            let az_option_window_icon_delete: extern "C" fn(_:  &mut AzOptionWindowIcon) = transmute(lib.get(b"az_option_window_icon_delete")?);
            let az_option_window_icon_deep_copy: extern "C" fn(_:  &AzOptionWindowIcon) -> AzOptionWindowIcon = transmute(lib.get(b"az_option_window_icon_deep_copy")?);
            let az_option_string_delete: extern "C" fn(_:  &mut AzOptionString) = transmute(lib.get(b"az_option_string_delete")?);
            let az_option_string_deep_copy: extern "C" fn(_:  &AzOptionString) -> AzOptionString = transmute(lib.get(b"az_option_string_deep_copy")?);
            let az_option_dom_delete: extern "C" fn(_:  &mut AzOptionDom) = transmute(lib.get(b"az_option_dom_delete")?);
            let az_option_dom_deep_copy: extern "C" fn(_:  &AzOptionDom) -> AzOptionDom = transmute(lib.get(b"az_option_dom_deep_copy")?);
            let az_option_texture_delete: extern "C" fn(_:  &mut AzOptionTexture) = transmute(lib.get(b"az_option_texture_delete")?);
            let az_option_image_mask_delete: extern "C" fn(_:  &mut AzOptionImageMask) = transmute(lib.get(b"az_option_image_mask_delete")?);
            let az_option_image_mask_deep_copy: extern "C" fn(_:  &AzOptionImageMask) -> AzOptionImageMask = transmute(lib.get(b"az_option_image_mask_deep_copy")?);
            let az_option_style_background_content_value_delete: extern "C" fn(_:  &mut AzOptionStyleBackgroundContentValue) = transmute(lib.get(b"az_option_style_background_content_value_delete")?);
            let az_option_style_background_content_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBackgroundContentValue) -> AzOptionStyleBackgroundContentValue = transmute(lib.get(b"az_option_style_background_content_value_deep_copy")?);
            let az_option_style_font_family_value_delete: extern "C" fn(_:  &mut AzOptionStyleFontFamilyValue) = transmute(lib.get(b"az_option_style_font_family_value_delete")?);
            let az_option_style_font_family_value_deep_copy: extern "C" fn(_:  &AzOptionStyleFontFamilyValue) -> AzOptionStyleFontFamilyValue = transmute(lib.get(b"az_option_style_font_family_value_deep_copy")?);
            let az_option_box_shadow_pre_display_item_value_delete: extern "C" fn(_:  &mut AzOptionBoxShadowPreDisplayItemValue) = transmute(lib.get(b"az_option_box_shadow_pre_display_item_value_delete")?);
            let az_option_box_shadow_pre_display_item_value_deep_copy: extern "C" fn(_:  &AzOptionBoxShadowPreDisplayItemValue) -> AzOptionBoxShadowPreDisplayItemValue = transmute(lib.get(b"az_option_box_shadow_pre_display_item_value_deep_copy")?);
            let az_option_instant_ptr_delete: extern "C" fn(_:  &mut AzOptionInstantPtr) = transmute(lib.get(b"az_option_instant_ptr_delete")?);
            let az_option_instant_ptr_deep_copy: extern "C" fn(_:  &AzOptionInstantPtr) -> AzOptionInstantPtr = transmute(lib.get(b"az_option_instant_ptr_deep_copy")?);
            let az_option_u8_vec_ref_delete: extern "C" fn(_:  &mut AzOptionU8VecRef) = transmute(lib.get(b"az_option_u8_vec_ref_delete")?);
            let az_result_svg_svg_parse_error_delete: extern "C" fn(_:  &mut AzResultSvgSvgParseError) = transmute(lib.get(b"az_result_svg_svg_parse_error_delete")?);
            let az_result_svg_svg_parse_error_deep_copy: extern "C" fn(_:  &AzResultSvgSvgParseError) -> AzResultSvgSvgParseError = transmute(lib.get(b"az_result_svg_svg_parse_error_deep_copy")?);
            let az_svg_parse_error_delete: extern "C" fn(_:  &mut AzSvgParseError) = transmute(lib.get(b"az_svg_parse_error_delete")?);
            let az_svg_parse_error_deep_copy: extern "C" fn(_:  &AzSvgParseError) -> AzSvgParseError = transmute(lib.get(b"az_svg_parse_error_deep_copy")?);
            let az_xml_error_delete: extern "C" fn(_:  &mut AzXmlError) = transmute(lib.get(b"az_xml_error_delete")?);
            let az_xml_error_deep_copy: extern "C" fn(_:  &AzXmlError) -> AzXmlError = transmute(lib.get(b"az_xml_error_deep_copy")?);
            let az_duplicated_namespace_error_delete: extern "C" fn(_:  &mut AzDuplicatedNamespaceError) = transmute(lib.get(b"az_duplicated_namespace_error_delete")?);
            let az_duplicated_namespace_error_deep_copy: extern "C" fn(_:  &AzDuplicatedNamespaceError) -> AzDuplicatedNamespaceError = transmute(lib.get(b"az_duplicated_namespace_error_deep_copy")?);
            let az_unknown_namespace_error_delete: extern "C" fn(_:  &mut AzUnknownNamespaceError) = transmute(lib.get(b"az_unknown_namespace_error_delete")?);
            let az_unknown_namespace_error_deep_copy: extern "C" fn(_:  &AzUnknownNamespaceError) -> AzUnknownNamespaceError = transmute(lib.get(b"az_unknown_namespace_error_deep_copy")?);
            let az_unexpected_close_tag_error_delete: extern "C" fn(_:  &mut AzUnexpectedCloseTagError) = transmute(lib.get(b"az_unexpected_close_tag_error_delete")?);
            let az_unexpected_close_tag_error_deep_copy: extern "C" fn(_:  &AzUnexpectedCloseTagError) -> AzUnexpectedCloseTagError = transmute(lib.get(b"az_unexpected_close_tag_error_deep_copy")?);
            let az_unknown_entity_reference_error_delete: extern "C" fn(_:  &mut AzUnknownEntityReferenceError) = transmute(lib.get(b"az_unknown_entity_reference_error_delete")?);
            let az_unknown_entity_reference_error_deep_copy: extern "C" fn(_:  &AzUnknownEntityReferenceError) -> AzUnknownEntityReferenceError = transmute(lib.get(b"az_unknown_entity_reference_error_deep_copy")?);
            let az_duplicated_attribute_error_delete: extern "C" fn(_:  &mut AzDuplicatedAttributeError) = transmute(lib.get(b"az_duplicated_attribute_error_delete")?);
            let az_duplicated_attribute_error_deep_copy: extern "C" fn(_:  &AzDuplicatedAttributeError) -> AzDuplicatedAttributeError = transmute(lib.get(b"az_duplicated_attribute_error_deep_copy")?);
            let az_xml_parse_error_delete: extern "C" fn(_:  &mut AzXmlParseError) = transmute(lib.get(b"az_xml_parse_error_delete")?);
            let az_xml_parse_error_deep_copy: extern "C" fn(_:  &AzXmlParseError) -> AzXmlParseError = transmute(lib.get(b"az_xml_parse_error_deep_copy")?);
            let az_xml_text_error_delete: extern "C" fn(_:  &mut AzXmlTextError) = transmute(lib.get(b"az_xml_text_error_delete")?);
            let az_xml_text_error_deep_copy: extern "C" fn(_:  &AzXmlTextError) -> AzXmlTextError = transmute(lib.get(b"az_xml_text_error_deep_copy")?);
            let az_xml_stream_error_delete: extern "C" fn(_:  &mut AzXmlStreamError) = transmute(lib.get(b"az_xml_stream_error_delete")?);
            let az_xml_stream_error_deep_copy: extern "C" fn(_:  &AzXmlStreamError) -> AzXmlStreamError = transmute(lib.get(b"az_xml_stream_error_deep_copy")?);
            let az_invalid_char_multiple_error_delete: extern "C" fn(_:  &mut AzInvalidCharMultipleError) = transmute(lib.get(b"az_invalid_char_multiple_error_delete")?);
            let az_invalid_char_multiple_error_deep_copy: extern "C" fn(_:  &AzInvalidCharMultipleError) -> AzInvalidCharMultipleError = transmute(lib.get(b"az_invalid_char_multiple_error_deep_copy")?);
            let az_invalid_string_error_delete: extern "C" fn(_:  &mut AzInvalidStringError) = transmute(lib.get(b"az_invalid_string_error_delete")?);
            let az_invalid_string_error_deep_copy: extern "C" fn(_:  &AzInvalidStringError) -> AzInvalidStringError = transmute(lib.get(b"az_invalid_string_error_deep_copy")?);
            let az_instant_ptr_now: extern "C" fn() -> AzInstantPtr = transmute(lib.get(b"az_instant_ptr_now")?);
            let az_instant_ptr_delete: extern "C" fn(_:  &mut AzInstantPtr) = transmute(lib.get(b"az_instant_ptr_delete")?);
            let az_app_config_default: extern "C" fn() -> AzAppConfig = transmute(lib.get(b"az_app_config_default")?);
            let az_app_config_delete: extern "C" fn(_:  &mut AzAppConfig) = transmute(lib.get(b"az_app_config_delete")?);
            let az_app_config_deep_copy: extern "C" fn(_:  &AzAppConfig) -> AzAppConfig = transmute(lib.get(b"az_app_config_deep_copy")?);
            let az_app_ptr_new: extern "C" fn(_:  AzRefAny, _:  AzAppConfig) -> AzAppPtr = transmute(lib.get(b"az_app_ptr_new")?);
            let az_app_ptr_add_window: extern "C" fn(_:  &mut AzAppPtr, _:  AzWindowCreateOptions) = transmute(lib.get(b"az_app_ptr_add_window")?);
            let az_app_ptr_run: extern "C" fn(_:  AzAppPtr, _:  AzWindowCreateOptions) = transmute(lib.get(b"az_app_ptr_run")?);
            let az_app_ptr_delete: extern "C" fn(_:  &mut AzAppPtr) = transmute(lib.get(b"az_app_ptr_delete")?);
            let az_hidpi_adjusted_bounds_get_logical_size: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzLogicalSize = transmute(lib.get(b"az_hidpi_adjusted_bounds_get_logical_size")?);
            let az_hidpi_adjusted_bounds_get_physical_size: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzPhysicalSizeU32 = transmute(lib.get(b"az_hidpi_adjusted_bounds_get_physical_size")?);
            let az_hidpi_adjusted_bounds_get_hidpi_factor: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> f32 = transmute(lib.get(b"az_hidpi_adjusted_bounds_get_hidpi_factor")?);
            let az_focus_target_delete: extern "C" fn(_:  &mut AzFocusTarget) = transmute(lib.get(b"az_focus_target_delete")?);
            let az_focus_target_deep_copy: extern "C" fn(_:  &AzFocusTarget) -> AzFocusTarget = transmute(lib.get(b"az_focus_target_deep_copy")?);
            let az_focus_target_path_delete: extern "C" fn(_:  &mut AzFocusTargetPath) = transmute(lib.get(b"az_focus_target_path_delete")?);
            let az_focus_target_path_deep_copy: extern "C" fn(_:  &AzFocusTargetPath) -> AzFocusTargetPath = transmute(lib.get(b"az_focus_target_path_deep_copy")?);
            let az_callback_info_get_hit_node: extern "C" fn(_:  &AzCallbackInfo) -> AzDomNodeId = transmute(lib.get(b"az_callback_info_get_hit_node")?);
            let az_callback_info_get_cursor_relative_to_viewport: extern "C" fn(_:  &AzCallbackInfo) -> AzOptionLayoutPoint = transmute(lib.get(b"az_callback_info_get_cursor_relative_to_viewport")?);
            let az_callback_info_get_cursor_relative_to_node: extern "C" fn(_:  &AzCallbackInfo) -> AzOptionLayoutPoint = transmute(lib.get(b"az_callback_info_get_cursor_relative_to_node")?);
            let az_callback_info_get_parent: extern "C" fn(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId = transmute(lib.get(b"az_callback_info_get_parent")?);
            let az_callback_info_get_previous_sibling: extern "C" fn(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId = transmute(lib.get(b"az_callback_info_get_previous_sibling")?);
            let az_callback_info_get_next_sibling: extern "C" fn(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId = transmute(lib.get(b"az_callback_info_get_next_sibling")?);
            let az_callback_info_get_first_child: extern "C" fn(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId = transmute(lib.get(b"az_callback_info_get_first_child")?);
            let az_callback_info_get_last_child: extern "C" fn(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId = transmute(lib.get(b"az_callback_info_get_last_child")?);
            let az_callback_info_get_dataset: extern "C" fn(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionRefAny = transmute(lib.get(b"az_callback_info_get_dataset")?);
            let az_callback_info_get_window_state: extern "C" fn(_:  &AzCallbackInfo) -> AzWindowState = transmute(lib.get(b"az_callback_info_get_window_state")?);
            let az_callback_info_get_keyboard_state: extern "C" fn(_:  &AzCallbackInfo) -> AzKeyboardState = transmute(lib.get(b"az_callback_info_get_keyboard_state")?);
            let az_callback_info_get_mouse_state: extern "C" fn(_:  &AzCallbackInfo) -> AzMouseState = transmute(lib.get(b"az_callback_info_get_mouse_state")?);
            let az_callback_info_get_current_window_handle: extern "C" fn(_:  &AzCallbackInfo) -> AzRawWindowHandle = transmute(lib.get(b"az_callback_info_get_current_window_handle")?);
            let az_callback_info_get_gl_context: extern "C" fn(_:  &AzCallbackInfo) -> AzGlContextPtr = transmute(lib.get(b"az_callback_info_get_gl_context")?);
            let az_callback_info_set_window_state: extern "C" fn(_:  &mut AzCallbackInfo, _:  AzWindowState) = transmute(lib.get(b"az_callback_info_set_window_state")?);
            let az_callback_info_set_focus: extern "C" fn(_:  &mut AzCallbackInfo, _:  AzFocusTarget) = transmute(lib.get(b"az_callback_info_set_focus")?);
            let az_callback_info_set_css_property: extern "C" fn(_:  &mut AzCallbackInfo, _:  AzDomNodeId, _:  AzCssProperty) = transmute(lib.get(b"az_callback_info_set_css_property")?);
            let az_callback_info_stop_propagation: extern "C" fn(_:  &mut AzCallbackInfo) = transmute(lib.get(b"az_callback_info_stop_propagation")?);
            let az_callback_info_create_window: extern "C" fn(_:  &mut AzCallbackInfo, _:  AzWindowCreateOptions) = transmute(lib.get(b"az_callback_info_create_window")?);
            let az_callback_info_start_thread: extern "C" fn(_:  &mut AzCallbackInfo, _:  AzThreadId, _:  AzRefAny, _:  AzRefAny, _:  AzThreadCallbackType) = transmute(lib.get(b"az_callback_info_start_thread")?);
            let az_callback_info_start_timer: extern "C" fn(_:  &mut AzCallbackInfo, _:  AzTimerId, _:  AzTimer) = transmute(lib.get(b"az_callback_info_start_timer")?);
            let az_callback_info_delete: extern "C" fn(_:  &mut AzCallbackInfo) = transmute(lib.get(b"az_callback_info_delete")?);
            let az_i_frame_callback_info_get_bounds: extern "C" fn(_:  &AzIFrameCallbackInfo) -> AzHidpiAdjustedBounds = transmute(lib.get(b"az_i_frame_callback_info_get_bounds")?);
            let az_i_frame_callback_info_delete: extern "C" fn(_:  &mut AzIFrameCallbackInfo) = transmute(lib.get(b"az_i_frame_callback_info_delete")?);
            let az_i_frame_callback_return_delete: extern "C" fn(_:  &mut AzIFrameCallbackReturn) = transmute(lib.get(b"az_i_frame_callback_return_delete")?);
            let az_i_frame_callback_return_deep_copy: extern "C" fn(_:  &AzIFrameCallbackReturn) -> AzIFrameCallbackReturn = transmute(lib.get(b"az_i_frame_callback_return_deep_copy")?);
            let az_gl_callback_info_get_gl_context: extern "C" fn(_:  &AzGlCallbackInfo) -> AzGlContextPtr = transmute(lib.get(b"az_gl_callback_info_get_gl_context")?);
            let az_gl_callback_info_delete: extern "C" fn(_:  &mut AzGlCallbackInfo) = transmute(lib.get(b"az_gl_callback_info_delete")?);
            let az_gl_callback_return_delete: extern "C" fn(_:  &mut AzGlCallbackReturn) = transmute(lib.get(b"az_gl_callback_return_delete")?);
            let az_timer_callback_info_delete: extern "C" fn(_:  &mut AzTimerCallbackInfo) = transmute(lib.get(b"az_timer_callback_info_delete")?);
            let az_timer_callback_return_delete: extern "C" fn(_:  &mut AzTimerCallbackReturn) = transmute(lib.get(b"az_timer_callback_return_delete")?);
            let az_timer_callback_return_deep_copy: extern "C" fn(_:  &AzTimerCallbackReturn) -> AzTimerCallbackReturn = transmute(lib.get(b"az_timer_callback_return_deep_copy")?);
            let az_write_back_callback_delete: extern "C" fn(_:  &mut AzWriteBackCallback) = transmute(lib.get(b"az_write_back_callback_delete")?);
            let az_write_back_callback_deep_copy: extern "C" fn(_:  &AzWriteBackCallback) -> AzWriteBackCallback = transmute(lib.get(b"az_write_back_callback_deep_copy")?);
            let az_atomic_ref_count_can_be_shared: extern "C" fn(_:  &AzAtomicRefCount) -> bool = transmute(lib.get(b"az_atomic_ref_count_can_be_shared")?);
            let az_atomic_ref_count_can_be_shared_mut: extern "C" fn(_:  &AzAtomicRefCount) -> bool = transmute(lib.get(b"az_atomic_ref_count_can_be_shared_mut")?);
            let az_atomic_ref_count_increase_ref: extern "C" fn(_:  &mut AzAtomicRefCount) = transmute(lib.get(b"az_atomic_ref_count_increase_ref")?);
            let az_atomic_ref_count_decrease_ref: extern "C" fn(_:  &mut AzAtomicRefCount) = transmute(lib.get(b"az_atomic_ref_count_decrease_ref")?);
            let az_atomic_ref_count_increase_refmut: extern "C" fn(_:  &mut AzAtomicRefCount) = transmute(lib.get(b"az_atomic_ref_count_increase_refmut")?);
            let az_atomic_ref_count_decrease_refmut: extern "C" fn(_:  &mut AzAtomicRefCount) = transmute(lib.get(b"az_atomic_ref_count_decrease_refmut")?);
            let az_atomic_ref_count_delete: extern "C" fn(_:  &mut AzAtomicRefCount) = transmute(lib.get(b"az_atomic_ref_count_delete")?);
            let az_ref_any_new_c: extern "C" fn(_:  *const c_void, _:  usize, _:  u64, _:  AzString, _:  AzRefAnyDestructorType) -> AzRefAny = transmute(lib.get(b"az_ref_any_new_c")?);
            let az_ref_any_is_type: extern "C" fn(_:  &AzRefAny, _:  u64) -> bool = transmute(lib.get(b"az_ref_any_is_type")?);
            let az_ref_any_get_type_name: extern "C" fn(_:  &AzRefAny) -> AzString = transmute(lib.get(b"az_ref_any_get_type_name")?);
            let az_ref_any_can_be_shared: extern "C" fn(_:  &AzRefAny) -> bool = transmute(lib.get(b"az_ref_any_can_be_shared")?);
            let az_ref_any_can_be_shared_mut: extern "C" fn(_:  &AzRefAny) -> bool = transmute(lib.get(b"az_ref_any_can_be_shared_mut")?);
            let az_ref_any_increase_ref: extern "C" fn(_:  &AzRefAny) = transmute(lib.get(b"az_ref_any_increase_ref")?);
            let az_ref_any_decrease_ref: extern "C" fn(_:  &AzRefAny) = transmute(lib.get(b"az_ref_any_decrease_ref")?);
            let az_ref_any_increase_refmut: extern "C" fn(_:  &AzRefAny) = transmute(lib.get(b"az_ref_any_increase_refmut")?);
            let az_ref_any_decrease_refmut: extern "C" fn(_:  &AzRefAny) = transmute(lib.get(b"az_ref_any_decrease_refmut")?);
            let az_ref_any_delete: extern "C" fn(_:  &mut AzRefAny) = transmute(lib.get(b"az_ref_any_delete")?);
            let az_ref_any_deep_copy: extern "C" fn(_:  &AzRefAny) -> AzRefAny = transmute(lib.get(b"az_ref_any_deep_copy")?);
            let az_layout_info_window_width_larger_than: extern "C" fn(_:  &mut AzLayoutInfo, _:  f32) -> bool = transmute(lib.get(b"az_layout_info_window_width_larger_than")?);
            let az_layout_info_window_width_smaller_than: extern "C" fn(_:  &mut AzLayoutInfo, _:  f32) -> bool = transmute(lib.get(b"az_layout_info_window_width_smaller_than")?);
            let az_layout_info_window_height_larger_than: extern "C" fn(_:  &mut AzLayoutInfo, _:  f32) -> bool = transmute(lib.get(b"az_layout_info_window_height_larger_than")?);
            let az_layout_info_window_height_smaller_than: extern "C" fn(_:  &mut AzLayoutInfo, _:  f32) -> bool = transmute(lib.get(b"az_layout_info_window_height_smaller_than")?);
            let az_layout_info_delete: extern "C" fn(_:  &mut AzLayoutInfo) = transmute(lib.get(b"az_layout_info_delete")?);
            let az_css_rule_block_delete: extern "C" fn(_:  &mut AzCssRuleBlock) = transmute(lib.get(b"az_css_rule_block_delete")?);
            let az_css_rule_block_deep_copy: extern "C" fn(_:  &AzCssRuleBlock) -> AzCssRuleBlock = transmute(lib.get(b"az_css_rule_block_deep_copy")?);
            let az_css_declaration_delete: extern "C" fn(_:  &mut AzCssDeclaration) = transmute(lib.get(b"az_css_declaration_delete")?);
            let az_css_declaration_deep_copy: extern "C" fn(_:  &AzCssDeclaration) -> AzCssDeclaration = transmute(lib.get(b"az_css_declaration_deep_copy")?);
            let az_dynamic_css_property_delete: extern "C" fn(_:  &mut AzDynamicCssProperty) = transmute(lib.get(b"az_dynamic_css_property_delete")?);
            let az_dynamic_css_property_deep_copy: extern "C" fn(_:  &AzDynamicCssProperty) -> AzDynamicCssProperty = transmute(lib.get(b"az_dynamic_css_property_deep_copy")?);
            let az_css_path_delete: extern "C" fn(_:  &mut AzCssPath) = transmute(lib.get(b"az_css_path_delete")?);
            let az_css_path_deep_copy: extern "C" fn(_:  &AzCssPath) -> AzCssPath = transmute(lib.get(b"az_css_path_deep_copy")?);
            let az_css_path_selector_delete: extern "C" fn(_:  &mut AzCssPathSelector) = transmute(lib.get(b"az_css_path_selector_delete")?);
            let az_css_path_selector_deep_copy: extern "C" fn(_:  &AzCssPathSelector) -> AzCssPathSelector = transmute(lib.get(b"az_css_path_selector_deep_copy")?);
            let az_stylesheet_delete: extern "C" fn(_:  &mut AzStylesheet) = transmute(lib.get(b"az_stylesheet_delete")?);
            let az_stylesheet_deep_copy: extern "C" fn(_:  &AzStylesheet) -> AzStylesheet = transmute(lib.get(b"az_stylesheet_deep_copy")?);
            let az_css_empty: extern "C" fn() -> AzCss = transmute(lib.get(b"az_css_empty")?);
            let az_css_from_string: extern "C" fn(_:  AzString) -> AzCss = transmute(lib.get(b"az_css_from_string")?);
            let az_css_delete: extern "C" fn(_:  &mut AzCss) = transmute(lib.get(b"az_css_delete")?);
            let az_css_deep_copy: extern "C" fn(_:  &AzCss) -> AzCss = transmute(lib.get(b"az_css_deep_copy")?);
            let az_color_u_from_str: extern "C" fn(_:  AzString) -> AzColorU = transmute(lib.get(b"az_color_u_from_str")?);
            let az_color_u_to_hash: extern "C" fn(_:  &AzColorU) -> AzString = transmute(lib.get(b"az_color_u_to_hash")?);
            let az_linear_gradient_delete: extern "C" fn(_:  &mut AzLinearGradient) = transmute(lib.get(b"az_linear_gradient_delete")?);
            let az_linear_gradient_deep_copy: extern "C" fn(_:  &AzLinearGradient) -> AzLinearGradient = transmute(lib.get(b"az_linear_gradient_deep_copy")?);
            let az_radial_gradient_delete: extern "C" fn(_:  &mut AzRadialGradient) = transmute(lib.get(b"az_radial_gradient_delete")?);
            let az_radial_gradient_deep_copy: extern "C" fn(_:  &AzRadialGradient) -> AzRadialGradient = transmute(lib.get(b"az_radial_gradient_deep_copy")?);
            let az_css_image_id_delete: extern "C" fn(_:  &mut AzCssImageId) = transmute(lib.get(b"az_css_image_id_delete")?);
            let az_css_image_id_deep_copy: extern "C" fn(_:  &AzCssImageId) -> AzCssImageId = transmute(lib.get(b"az_css_image_id_deep_copy")?);
            let az_style_background_content_delete: extern "C" fn(_:  &mut AzStyleBackgroundContent) = transmute(lib.get(b"az_style_background_content_delete")?);
            let az_style_background_content_deep_copy: extern "C" fn(_:  &AzStyleBackgroundContent) -> AzStyleBackgroundContent = transmute(lib.get(b"az_style_background_content_deep_copy")?);
            let az_style_font_family_delete: extern "C" fn(_:  &mut AzStyleFontFamily) = transmute(lib.get(b"az_style_font_family_delete")?);
            let az_style_font_family_deep_copy: extern "C" fn(_:  &AzStyleFontFamily) -> AzStyleFontFamily = transmute(lib.get(b"az_style_font_family_deep_copy")?);
            let az_style_background_content_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundContentValue) = transmute(lib.get(b"az_style_background_content_value_delete")?);
            let az_style_background_content_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue = transmute(lib.get(b"az_style_background_content_value_deep_copy")?);
            let az_style_font_family_value_delete: extern "C" fn(_:  &mut AzStyleFontFamilyValue) = transmute(lib.get(b"az_style_font_family_value_delete")?);
            let az_style_font_family_value_deep_copy: extern "C" fn(_:  &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue = transmute(lib.get(b"az_style_font_family_value_deep_copy")?);
            let az_style_transform_vec_value_delete: extern "C" fn(_:  &mut AzStyleTransformVecValue) = transmute(lib.get(b"az_style_transform_vec_value_delete")?);
            let az_style_transform_vec_value_deep_copy: extern "C" fn(_:  &AzStyleTransformVecValue) -> AzStyleTransformVecValue = transmute(lib.get(b"az_style_transform_vec_value_deep_copy")?);
            let az_css_property_delete: extern "C" fn(_:  &mut AzCssProperty) = transmute(lib.get(b"az_css_property_delete")?);
            let az_css_property_deep_copy: extern "C" fn(_:  &AzCssProperty) -> AzCssProperty = transmute(lib.get(b"az_css_property_deep_copy")?);
            let az_rect_style_delete: extern "C" fn(_:  &mut AzRectStyle) = transmute(lib.get(b"az_rect_style_delete")?);
            let az_rect_style_deep_copy: extern "C" fn(_:  &AzRectStyle) -> AzRectStyle = transmute(lib.get(b"az_rect_style_deep_copy")?);
            let az_rect_layout_delete: extern "C" fn(_:  &mut AzRectLayout) = transmute(lib.get(b"az_rect_layout_delete")?);
            let az_rect_layout_deep_copy: extern "C" fn(_:  &AzRectLayout) -> AzRectLayout = transmute(lib.get(b"az_rect_layout_deep_copy")?);
            let az_cascaded_css_property_with_source_delete: extern "C" fn(_:  &mut AzCascadedCssPropertyWithSource) = transmute(lib.get(b"az_cascaded_css_property_with_source_delete")?);
            let az_cascaded_css_property_with_source_deep_copy: extern "C" fn(_:  &AzCascadedCssPropertyWithSource) -> AzCascadedCssPropertyWithSource = transmute(lib.get(b"az_cascaded_css_property_with_source_deep_copy")?);
            let az_css_property_source_delete: extern "C" fn(_:  &mut AzCssPropertySource) = transmute(lib.get(b"az_css_property_source_delete")?);
            let az_css_property_source_deep_copy: extern "C" fn(_:  &AzCssPropertySource) -> AzCssPropertySource = transmute(lib.get(b"az_css_property_source_deep_copy")?);
            let az_styled_node_delete: extern "C" fn(_:  &mut AzStyledNode) = transmute(lib.get(b"az_styled_node_delete")?);
            let az_styled_node_deep_copy: extern "C" fn(_:  &AzStyledNode) -> AzStyledNode = transmute(lib.get(b"az_styled_node_deep_copy")?);
            let az_content_group_delete: extern "C" fn(_:  &mut AzContentGroup) = transmute(lib.get(b"az_content_group_delete")?);
            let az_content_group_deep_copy: extern "C" fn(_:  &AzContentGroup) -> AzContentGroup = transmute(lib.get(b"az_content_group_deep_copy")?);
            let az_styled_dom_new: extern "C" fn(_:  AzDom, _:  AzCss) -> AzStyledDom = transmute(lib.get(b"az_styled_dom_new")?);
            let az_styled_dom_append: extern "C" fn(_:  &mut AzStyledDom, _:  AzStyledDom) = transmute(lib.get(b"az_styled_dom_append")?);
            let az_styled_dom_delete: extern "C" fn(_:  &mut AzStyledDom) = transmute(lib.get(b"az_styled_dom_delete")?);
            let az_styled_dom_deep_copy: extern "C" fn(_:  &AzStyledDom) -> AzStyledDom = transmute(lib.get(b"az_styled_dom_deep_copy")?);
            let az_dom_new: extern "C" fn(_:  AzNodeType) -> AzDom = transmute(lib.get(b"az_dom_new")?);
            let az_dom_div: extern "C" fn() -> AzDom = transmute(lib.get(b"az_dom_div")?);
            let az_dom_body: extern "C" fn() -> AzDom = transmute(lib.get(b"az_dom_body")?);
            let az_dom_label: extern "C" fn(_:  AzString) -> AzDom = transmute(lib.get(b"az_dom_label")?);
            let az_dom_text: extern "C" fn(_:  AzTextId) -> AzDom = transmute(lib.get(b"az_dom_text")?);
            let az_dom_image: extern "C" fn(_:  AzImageId) -> AzDom = transmute(lib.get(b"az_dom_image")?);
            let az_dom_gl_texture: extern "C" fn(_:  AzRefAny, _:  AzGlCallbackType) -> AzDom = transmute(lib.get(b"az_dom_gl_texture")?);
            let az_dom_iframe: extern "C" fn(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzDom = transmute(lib.get(b"az_dom_iframe")?);
            let az_dom_add_id: extern "C" fn(_:  &mut AzDom, _:  AzString) = transmute(lib.get(b"az_dom_add_id")?);
            let az_dom_with_id: extern "C" fn(_:  AzDom, _:  AzString) -> AzDom = transmute(lib.get(b"az_dom_with_id")?);
            let az_dom_set_ids: extern "C" fn(_:  &mut AzDom, _:  AzStringVec) = transmute(lib.get(b"az_dom_set_ids")?);
            let az_dom_with_ids: extern "C" fn(_:  AzDom, _:  AzStringVec) -> AzDom = transmute(lib.get(b"az_dom_with_ids")?);
            let az_dom_add_class: extern "C" fn(_:  &mut AzDom, _:  AzString) = transmute(lib.get(b"az_dom_add_class")?);
            let az_dom_with_class: extern "C" fn(_:  AzDom, _:  AzString) -> AzDom = transmute(lib.get(b"az_dom_with_class")?);
            let az_dom_set_classes: extern "C" fn(_:  &mut AzDom, _:  AzStringVec) = transmute(lib.get(b"az_dom_set_classes")?);
            let az_dom_with_classes: extern "C" fn(_:  AzDom, _:  AzStringVec) -> AzDom = transmute(lib.get(b"az_dom_with_classes")?);
            let az_dom_add_callback: extern "C" fn(_:  &mut AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) = transmute(lib.get(b"az_dom_add_callback")?);
            let az_dom_with_callback: extern "C" fn(_:  AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzDom = transmute(lib.get(b"az_dom_with_callback")?);
            let az_dom_set_dataset: extern "C" fn(_:  &mut AzDom, _:  AzRefAny) = transmute(lib.get(b"az_dom_set_dataset")?);
            let az_dom_with_dataset: extern "C" fn(_:  AzDom, _:  AzRefAny) -> AzDom = transmute(lib.get(b"az_dom_with_dataset")?);
            let az_dom_add_inline_css: extern "C" fn(_:  &mut AzDom, _:  AzCssProperty) = transmute(lib.get(b"az_dom_add_inline_css")?);
            let az_dom_with_inline_css: extern "C" fn(_:  AzDom, _:  AzCssProperty) -> AzDom = transmute(lib.get(b"az_dom_with_inline_css")?);
            let az_dom_add_inline_hover_css: extern "C" fn(_:  &mut AzDom, _:  AzCssProperty) = transmute(lib.get(b"az_dom_add_inline_hover_css")?);
            let az_dom_with_inline_hover_css: extern "C" fn(_:  AzDom, _:  AzCssProperty) -> AzDom = transmute(lib.get(b"az_dom_with_inline_hover_css")?);
            let az_dom_add_inline_active_css: extern "C" fn(_:  &mut AzDom, _:  AzCssProperty) = transmute(lib.get(b"az_dom_add_inline_active_css")?);
            let az_dom_with_inline_active_css: extern "C" fn(_:  AzDom, _:  AzCssProperty) -> AzDom = transmute(lib.get(b"az_dom_with_inline_active_css")?);
            let az_dom_add_inline_focus_css: extern "C" fn(_:  &mut AzDom, _:  AzCssProperty) = transmute(lib.get(b"az_dom_add_inline_focus_css")?);
            let az_dom_with_inline_focus_css: extern "C" fn(_:  AzDom, _:  AzCssProperty) -> AzDom = transmute(lib.get(b"az_dom_with_inline_focus_css")?);
            let az_dom_set_is_draggable: extern "C" fn(_:  &mut AzDom, _:  bool) = transmute(lib.get(b"az_dom_set_is_draggable")?);
            let az_dom_with_clip_mask: extern "C" fn(_:  AzDom, _:  AzOptionImageMask) -> AzDom = transmute(lib.get(b"az_dom_with_clip_mask")?);
            let az_dom_set_clip_mask: extern "C" fn(_:  &mut AzDom, _:  AzOptionImageMask) = transmute(lib.get(b"az_dom_set_clip_mask")?);
            let az_dom_is_draggable: extern "C" fn(_:  AzDom, _:  bool) -> AzDom = transmute(lib.get(b"az_dom_is_draggable")?);
            let az_dom_set_tab_index: extern "C" fn(_:  &mut AzDom, _:  AzOptionTabIndex) = transmute(lib.get(b"az_dom_set_tab_index")?);
            let az_dom_with_tab_index: extern "C" fn(_:  AzDom, _:  AzOptionTabIndex) -> AzDom = transmute(lib.get(b"az_dom_with_tab_index")?);
            let az_dom_add_child: extern "C" fn(_:  &mut AzDom, _:  AzDom) = transmute(lib.get(b"az_dom_add_child")?);
            let az_dom_with_child: extern "C" fn(_:  AzDom, _:  AzDom) -> AzDom = transmute(lib.get(b"az_dom_with_child")?);
            let az_dom_get_html_string: extern "C" fn(_:  &AzDom) -> AzString = transmute(lib.get(b"az_dom_get_html_string")?);
            let az_dom_style: extern "C" fn(_:  AzDom, _:  AzCss) -> AzStyledDom = transmute(lib.get(b"az_dom_style")?);
            let az_dom_delete: extern "C" fn(_:  &mut AzDom) = transmute(lib.get(b"az_dom_delete")?);
            let az_dom_deep_copy: extern "C" fn(_:  &AzDom) -> AzDom = transmute(lib.get(b"az_dom_deep_copy")?);
            let az_gl_texture_node_delete: extern "C" fn(_:  &mut AzGlTextureNode) = transmute(lib.get(b"az_gl_texture_node_delete")?);
            let az_gl_texture_node_deep_copy: extern "C" fn(_:  &AzGlTextureNode) -> AzGlTextureNode = transmute(lib.get(b"az_gl_texture_node_deep_copy")?);
            let az_i_frame_node_delete: extern "C" fn(_:  &mut AzIFrameNode) = transmute(lib.get(b"az_i_frame_node_delete")?);
            let az_i_frame_node_deep_copy: extern "C" fn(_:  &AzIFrameNode) -> AzIFrameNode = transmute(lib.get(b"az_i_frame_node_deep_copy")?);
            let az_callback_data_delete: extern "C" fn(_:  &mut AzCallbackData) = transmute(lib.get(b"az_callback_data_delete")?);
            let az_callback_data_deep_copy: extern "C" fn(_:  &AzCallbackData) -> AzCallbackData = transmute(lib.get(b"az_callback_data_deep_copy")?);
            let az_image_mask_delete: extern "C" fn(_:  &mut AzImageMask) = transmute(lib.get(b"az_image_mask_delete")?);
            let az_image_mask_deep_copy: extern "C" fn(_:  &AzImageMask) -> AzImageMask = transmute(lib.get(b"az_image_mask_deep_copy")?);
            let az_node_data_new: extern "C" fn(_:  AzNodeType) -> AzNodeData = transmute(lib.get(b"az_node_data_new")?);
            let az_node_data_div: extern "C" fn() -> AzNodeData = transmute(lib.get(b"az_node_data_div")?);
            let az_node_data_body: extern "C" fn() -> AzNodeData = transmute(lib.get(b"az_node_data_body")?);
            let az_node_data_label: extern "C" fn(_:  AzString) -> AzNodeData = transmute(lib.get(b"az_node_data_label")?);
            let az_node_data_text: extern "C" fn(_:  AzTextId) -> AzNodeData = transmute(lib.get(b"az_node_data_text")?);
            let az_node_data_image: extern "C" fn(_:  AzImageId) -> AzNodeData = transmute(lib.get(b"az_node_data_image")?);
            let az_node_data_gl_texture: extern "C" fn(_:  AzRefAny, _:  AzGlCallbackType) -> AzNodeData = transmute(lib.get(b"az_node_data_gl_texture")?);
            let az_node_data_iframe: extern "C" fn(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzNodeData = transmute(lib.get(b"az_node_data_iframe")?);
            let az_node_data_default: extern "C" fn() -> AzNodeData = transmute(lib.get(b"az_node_data_default")?);
            let az_node_data_add_id: extern "C" fn(_:  &mut AzNodeData, _:  AzString) = transmute(lib.get(b"az_node_data_add_id")?);
            let az_node_data_with_id: extern "C" fn(_:  AzNodeData, _:  AzString) -> AzNodeData = transmute(lib.get(b"az_node_data_with_id")?);
            let az_node_data_set_ids: extern "C" fn(_:  &mut AzNodeData, _:  AzStringVec) = transmute(lib.get(b"az_node_data_set_ids")?);
            let az_node_data_with_ids: extern "C" fn(_:  AzNodeData, _:  AzStringVec) -> AzNodeData = transmute(lib.get(b"az_node_data_with_ids")?);
            let az_node_data_add_class: extern "C" fn(_:  &mut AzNodeData, _:  AzString) = transmute(lib.get(b"az_node_data_add_class")?);
            let az_node_data_with_class: extern "C" fn(_:  AzNodeData, _:  AzString) -> AzNodeData = transmute(lib.get(b"az_node_data_with_class")?);
            let az_node_data_set_classes: extern "C" fn(_:  &mut AzNodeData, _:  AzStringVec) = transmute(lib.get(b"az_node_data_set_classes")?);
            let az_node_data_with_classes: extern "C" fn(_:  AzNodeData, _:  AzStringVec) -> AzNodeData = transmute(lib.get(b"az_node_data_with_classes")?);
            let az_node_data_add_dataset: extern "C" fn(_:  &mut AzNodeData, _:  AzRefAny) = transmute(lib.get(b"az_node_data_add_dataset")?);
            let az_node_data_with_dataset: extern "C" fn(_:  AzNodeData, _:  AzRefAny) -> AzNodeData = transmute(lib.get(b"az_node_data_with_dataset")?);
            let az_node_data_add_callback: extern "C" fn(_:  &mut AzNodeData, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) = transmute(lib.get(b"az_node_data_add_callback")?);
            let az_node_data_with_callback: extern "C" fn(_:  AzNodeData, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzNodeData = transmute(lib.get(b"az_node_data_with_callback")?);
            let az_node_data_add_inline_css: extern "C" fn(_:  &mut AzNodeData, _:  AzCssProperty) = transmute(lib.get(b"az_node_data_add_inline_css")?);
            let az_node_data_with_inline_css: extern "C" fn(_:  AzNodeData, _:  AzCssProperty) -> AzNodeData = transmute(lib.get(b"az_node_data_with_inline_css")?);
            let az_node_data_add_inline_hover_css: extern "C" fn(_:  &mut AzNodeData, _:  AzCssProperty) = transmute(lib.get(b"az_node_data_add_inline_hover_css")?);
            let az_node_data_add_inline_active_css: extern "C" fn(_:  &mut AzNodeData, _:  AzCssProperty) = transmute(lib.get(b"az_node_data_add_inline_active_css")?);
            let az_node_data_add_inline_focus_css: extern "C" fn(_:  &mut AzNodeData, _:  AzCssProperty) = transmute(lib.get(b"az_node_data_add_inline_focus_css")?);
            let az_node_data_with_clip_mask: extern "C" fn(_:  AzNodeData, _:  AzOptionImageMask) -> AzNodeData = transmute(lib.get(b"az_node_data_with_clip_mask")?);
            let az_node_data_set_clip_mask: extern "C" fn(_:  &mut AzNodeData, _:  AzOptionImageMask) = transmute(lib.get(b"az_node_data_set_clip_mask")?);
            let az_node_data_set_is_draggable: extern "C" fn(_:  &mut AzNodeData, _:  bool) = transmute(lib.get(b"az_node_data_set_is_draggable")?);
            let az_node_data_is_draggable: extern "C" fn(_:  AzNodeData, _:  bool) -> AzNodeData = transmute(lib.get(b"az_node_data_is_draggable")?);
            let az_node_data_set_tab_index: extern "C" fn(_:  &mut AzNodeData, _:  AzOptionTabIndex) = transmute(lib.get(b"az_node_data_set_tab_index")?);
            let az_node_data_with_tab_index: extern "C" fn(_:  AzNodeData, _:  AzOptionTabIndex) -> AzNodeData = transmute(lib.get(b"az_node_data_with_tab_index")?);
            let az_node_data_delete: extern "C" fn(_:  &mut AzNodeData) = transmute(lib.get(b"az_node_data_delete")?);
            let az_node_data_deep_copy: extern "C" fn(_:  &AzNodeData) -> AzNodeData = transmute(lib.get(b"az_node_data_deep_copy")?);
            let az_node_type_delete: extern "C" fn(_:  &mut AzNodeType) = transmute(lib.get(b"az_node_type_delete")?);
            let az_node_type_deep_copy: extern "C" fn(_:  &AzNodeType) -> AzNodeType = transmute(lib.get(b"az_node_type_deep_copy")?);
            let az_on_into_event_filter: extern "C" fn(_:  AzOn) -> AzEventFilter = transmute(lib.get(b"az_on_into_event_filter")?);
            let az_vertex_attribute_delete: extern "C" fn(_:  &mut AzVertexAttribute) = transmute(lib.get(b"az_vertex_attribute_delete")?);
            let az_vertex_attribute_deep_copy: extern "C" fn(_:  &AzVertexAttribute) -> AzVertexAttribute = transmute(lib.get(b"az_vertex_attribute_deep_copy")?);
            let az_vertex_layout_delete: extern "C" fn(_:  &mut AzVertexLayout) = transmute(lib.get(b"az_vertex_layout_delete")?);
            let az_vertex_layout_deep_copy: extern "C" fn(_:  &AzVertexLayout) -> AzVertexLayout = transmute(lib.get(b"az_vertex_layout_deep_copy")?);
            let az_vertex_array_object_delete: extern "C" fn(_:  &mut AzVertexArrayObject) = transmute(lib.get(b"az_vertex_array_object_delete")?);
            let az_vertex_buffer_delete: extern "C" fn(_:  &mut AzVertexBuffer) = transmute(lib.get(b"az_vertex_buffer_delete")?);
            let az_debug_message_delete: extern "C" fn(_:  &mut AzDebugMessage) = transmute(lib.get(b"az_debug_message_delete")?);
            let az_debug_message_deep_copy: extern "C" fn(_:  &AzDebugMessage) -> AzDebugMessage = transmute(lib.get(b"az_debug_message_deep_copy")?);
            let az_u8_vec_ref_delete: extern "C" fn(_:  &mut AzU8VecRef) = transmute(lib.get(b"az_u8_vec_ref_delete")?);
            let az_u8_vec_ref_mut_delete: extern "C" fn(_:  &mut AzU8VecRefMut) = transmute(lib.get(b"az_u8_vec_ref_mut_delete")?);
            let az_f32_vec_ref_delete: extern "C" fn(_:  &mut AzF32VecRef) = transmute(lib.get(b"az_f32_vec_ref_delete")?);
            let az_i32_vec_ref_delete: extern "C" fn(_:  &mut AzI32VecRef) = transmute(lib.get(b"az_i32_vec_ref_delete")?);
            let az_g_luint_vec_ref_delete: extern "C" fn(_:  &mut AzGLuintVecRef) = transmute(lib.get(b"az_g_luint_vec_ref_delete")?);
            let az_g_lenum_vec_ref_delete: extern "C" fn(_:  &mut AzGLenumVecRef) = transmute(lib.get(b"az_g_lenum_vec_ref_delete")?);
            let az_g_lint_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLintVecRefMut) = transmute(lib.get(b"az_g_lint_vec_ref_mut_delete")?);
            let az_g_lint64_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLint64VecRefMut) = transmute(lib.get(b"az_g_lint64_vec_ref_mut_delete")?);
            let az_g_lboolean_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLbooleanVecRefMut) = transmute(lib.get(b"az_g_lboolean_vec_ref_mut_delete")?);
            let az_g_lfloat_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLfloatVecRefMut) = transmute(lib.get(b"az_g_lfloat_vec_ref_mut_delete")?);
            let az_refstr_vec_ref_delete: extern "C" fn(_:  &mut AzRefstrVecRef) = transmute(lib.get(b"az_refstr_vec_ref_delete")?);
            let az_refstr_delete: extern "C" fn(_:  &mut AzRefstr) = transmute(lib.get(b"az_refstr_delete")?);
            let az_get_program_binary_return_delete: extern "C" fn(_:  &mut AzGetProgramBinaryReturn) = transmute(lib.get(b"az_get_program_binary_return_delete")?);
            let az_get_program_binary_return_deep_copy: extern "C" fn(_:  &AzGetProgramBinaryReturn) -> AzGetProgramBinaryReturn = transmute(lib.get(b"az_get_program_binary_return_deep_copy")?);
            let az_get_active_attrib_return_delete: extern "C" fn(_:  &mut AzGetActiveAttribReturn) = transmute(lib.get(b"az_get_active_attrib_return_delete")?);
            let az_get_active_attrib_return_deep_copy: extern "C" fn(_:  &AzGetActiveAttribReturn) -> AzGetActiveAttribReturn = transmute(lib.get(b"az_get_active_attrib_return_deep_copy")?);
            let az_g_lsync_ptr_delete: extern "C" fn(_:  &mut AzGLsyncPtr) = transmute(lib.get(b"az_g_lsync_ptr_delete")?);
            let az_get_active_uniform_return_delete: extern "C" fn(_:  &mut AzGetActiveUniformReturn) = transmute(lib.get(b"az_get_active_uniform_return_delete")?);
            let az_get_active_uniform_return_deep_copy: extern "C" fn(_:  &AzGetActiveUniformReturn) -> AzGetActiveUniformReturn = transmute(lib.get(b"az_get_active_uniform_return_deep_copy")?);
            let az_gl_context_ptr_get_type: extern "C" fn(_:  &AzGlContextPtr) -> AzGlType = transmute(lib.get(b"az_gl_context_ptr_get_type")?);
            let az_gl_context_ptr_buffer_data_untyped: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  *const c_void, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_buffer_data_untyped")?);
            let az_gl_context_ptr_buffer_sub_data_untyped: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  *const c_void) = transmute(lib.get(b"az_gl_context_ptr_buffer_sub_data_untyped")?);
            let az_gl_context_ptr_map_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> *mut c_void = transmute(lib.get(b"az_gl_context_ptr_map_buffer")?);
            let az_gl_context_ptr_map_buffer_range: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  u32) -> *mut c_void = transmute(lib.get(b"az_gl_context_ptr_map_buffer_range")?);
            let az_gl_context_ptr_unmap_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8 = transmute(lib.get(b"az_gl_context_ptr_unmap_buffer")?);
            let az_gl_context_ptr_tex_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_tex_buffer")?);
            let az_gl_context_ptr_shader_source: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzStringVec) = transmute(lib.get(b"az_gl_context_ptr_shader_source")?);
            let az_gl_context_ptr_read_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_read_buffer")?);
            let az_gl_context_ptr_read_pixels_into_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut) = transmute(lib.get(b"az_gl_context_ptr_read_pixels_into_buffer")?);
            let az_gl_context_ptr_read_pixels: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32) -> AzU8Vec = transmute(lib.get(b"az_gl_context_ptr_read_pixels")?);
            let az_gl_context_ptr_read_pixels_into_pbo: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_read_pixels_into_pbo")?);
            let az_gl_context_ptr_sample_coverage: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  bool) = transmute(lib.get(b"az_gl_context_ptr_sample_coverage")?);
            let az_gl_context_ptr_polygon_offset: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_polygon_offset")?);
            let az_gl_context_ptr_pixel_store_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_pixel_store_i")?);
            let az_gl_context_ptr_gen_buffers: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_buffers")?);
            let az_gl_context_ptr_gen_renderbuffers: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_renderbuffers")?);
            let az_gl_context_ptr_gen_framebuffers: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_framebuffers")?);
            let az_gl_context_ptr_gen_textures: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_textures")?);
            let az_gl_context_ptr_gen_vertex_arrays: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_vertex_arrays")?);
            let az_gl_context_ptr_gen_queries: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_queries")?);
            let az_gl_context_ptr_begin_query: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_begin_query")?);
            let az_gl_context_ptr_end_query: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_end_query")?);
            let az_gl_context_ptr_query_counter: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_query_counter")?);
            let az_gl_context_ptr_get_query_object_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_query_object_iv")?);
            let az_gl_context_ptr_get_query_object_uiv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u32 = transmute(lib.get(b"az_gl_context_ptr_get_query_object_uiv")?);
            let az_gl_context_ptr_get_query_object_i64v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i64 = transmute(lib.get(b"az_gl_context_ptr_get_query_object_i64v")?);
            let az_gl_context_ptr_get_query_object_ui64v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u64 = transmute(lib.get(b"az_gl_context_ptr_get_query_object_ui64v")?);
            let az_gl_context_ptr_delete_queries: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_queries")?);
            let az_gl_context_ptr_delete_vertex_arrays: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_vertex_arrays")?);
            let az_gl_context_ptr_delete_buffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_buffers")?);
            let az_gl_context_ptr_delete_renderbuffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_renderbuffers")?);
            let az_gl_context_ptr_delete_framebuffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_framebuffers")?);
            let az_gl_context_ptr_delete_textures: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_textures")?);
            let az_gl_context_ptr_framebuffer_renderbuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_framebuffer_renderbuffer")?);
            let az_gl_context_ptr_renderbuffer_storage: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_renderbuffer_storage")?);
            let az_gl_context_ptr_depth_func: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_depth_func")?);
            let az_gl_context_ptr_active_texture: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_active_texture")?);
            let az_gl_context_ptr_attach_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_attach_shader")?);
            let az_gl_context_ptr_bind_attrib_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr) = transmute(lib.get(b"az_gl_context_ptr_bind_attrib_location")?);
            let az_gl_context_ptr_get_uniform_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLintVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_uniform_iv")?);
            let az_gl_context_ptr_get_uniform_fv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLfloatVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_uniform_fv")?);
            let az_gl_context_ptr_get_uniform_block_index: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> u32 = transmute(lib.get(b"az_gl_context_ptr_get_uniform_block_index")?);
            let az_gl_context_ptr_get_uniform_indices: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstrVecRef) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_get_uniform_indices")?);
            let az_gl_context_ptr_bind_buffer_base: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_bind_buffer_base")?);
            let az_gl_context_ptr_bind_buffer_range: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  isize, _:  isize) = transmute(lib.get(b"az_gl_context_ptr_bind_buffer_range")?);
            let az_gl_context_ptr_uniform_block_binding: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_uniform_block_binding")?);
            let az_gl_context_ptr_bind_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_bind_buffer")?);
            let az_gl_context_ptr_bind_vertex_array: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_bind_vertex_array")?);
            let az_gl_context_ptr_bind_renderbuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_bind_renderbuffer")?);
            let az_gl_context_ptr_bind_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_bind_framebuffer")?);
            let az_gl_context_ptr_bind_texture: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_bind_texture")?);
            let az_gl_context_ptr_draw_buffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLenumVecRef) = transmute(lib.get(b"az_gl_context_ptr_draw_buffers")?);
            let az_gl_context_ptr_tex_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_tex_image_2d")?);
            let az_gl_context_ptr_compressed_tex_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  AzU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_compressed_tex_image_2d")?);
            let az_gl_context_ptr_compressed_tex_sub_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  AzU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_compressed_tex_sub_image_2d")?);
            let az_gl_context_ptr_tex_image_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_tex_image_3d")?);
            let az_gl_context_ptr_copy_tex_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_copy_tex_image_2d")?);
            let az_gl_context_ptr_copy_tex_sub_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_copy_tex_sub_image_2d")?);
            let az_gl_context_ptr_copy_tex_sub_image_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_copy_tex_sub_image_3d")?);
            let az_gl_context_ptr_tex_sub_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_tex_sub_image_2d")?);
            let az_gl_context_ptr_tex_sub_image_2d_pbo: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize) = transmute(lib.get(b"az_gl_context_ptr_tex_sub_image_2d_pbo")?);
            let az_gl_context_ptr_tex_sub_image_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_tex_sub_image_3d")?);
            let az_gl_context_ptr_tex_sub_image_3d_pbo: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize) = transmute(lib.get(b"az_gl_context_ptr_tex_sub_image_3d_pbo")?);
            let az_gl_context_ptr_tex_storage_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_tex_storage_2d")?);
            let az_gl_context_ptr_tex_storage_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_tex_storage_3d")?);
            let az_gl_context_ptr_get_tex_image_into_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_tex_image_into_buffer")?);
            let az_gl_context_ptr_copy_image_sub_data: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_copy_image_sub_data")?);
            let az_gl_context_ptr_invalidate_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef) = transmute(lib.get(b"az_gl_context_ptr_invalidate_framebuffer")?);
            let az_gl_context_ptr_invalidate_sub_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_invalidate_sub_framebuffer")?);
            let az_gl_context_ptr_get_integer_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLintVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_integer_v")?);
            let az_gl_context_ptr_get_integer_64v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLint64VecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_integer_64v")?);
            let az_gl_context_ptr_get_integer_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_integer_iv")?);
            let az_gl_context_ptr_get_integer_64iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLint64VecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_integer_64iv")?);
            let az_gl_context_ptr_get_boolean_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLbooleanVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_boolean_v")?);
            let az_gl_context_ptr_get_float_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLfloatVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_float_v")?);
            let az_gl_context_ptr_get_framebuffer_attachment_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_framebuffer_attachment_parameter_iv")?);
            let az_gl_context_ptr_get_renderbuffer_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_renderbuffer_parameter_iv")?);
            let az_gl_context_ptr_get_tex_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_tex_parameter_iv")?);
            let az_gl_context_ptr_get_tex_parameter_fv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> f32 = transmute(lib.get(b"az_gl_context_ptr_get_tex_parameter_fv")?);
            let az_gl_context_ptr_tex_parameter_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_tex_parameter_i")?);
            let az_gl_context_ptr_tex_parameter_f: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_tex_parameter_f")?);
            let az_gl_context_ptr_framebuffer_texture_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_framebuffer_texture_2d")?);
            let az_gl_context_ptr_framebuffer_texture_layer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_framebuffer_texture_layer")?);
            let az_gl_context_ptr_blit_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_blit_framebuffer")?);
            let az_gl_context_ptr_vertex_attrib_4f: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  f32, _:  f32, _:  f32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_vertex_attrib_4f")?);
            let az_gl_context_ptr_vertex_attrib_pointer_f32: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  bool, _:  i32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_vertex_attrib_pointer_f32")?);
            let az_gl_context_ptr_vertex_attrib_pointer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  bool, _:  i32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_vertex_attrib_pointer")?);
            let az_gl_context_ptr_vertex_attrib_i_pointer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_vertex_attrib_i_pointer")?);
            let az_gl_context_ptr_vertex_attrib_divisor: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_vertex_attrib_divisor")?);
            let az_gl_context_ptr_viewport: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_viewport")?);
            let az_gl_context_ptr_scissor: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_scissor")?);
            let az_gl_context_ptr_line_width: extern "C" fn(_:  &AzGlContextPtr, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_line_width")?);
            let az_gl_context_ptr_use_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_use_program")?);
            let az_gl_context_ptr_validate_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_validate_program")?);
            let az_gl_context_ptr_draw_arrays: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_draw_arrays")?);
            let az_gl_context_ptr_draw_arrays_instanced: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_draw_arrays_instanced")?);
            let az_gl_context_ptr_draw_elements: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_draw_elements")?);
            let az_gl_context_ptr_draw_elements_instanced: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_draw_elements_instanced")?);
            let az_gl_context_ptr_blend_color: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_blend_color")?);
            let az_gl_context_ptr_blend_func: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_blend_func")?);
            let az_gl_context_ptr_blend_func_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_blend_func_separate")?);
            let az_gl_context_ptr_blend_equation: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_blend_equation")?);
            let az_gl_context_ptr_blend_equation_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_blend_equation_separate")?);
            let az_gl_context_ptr_color_mask: extern "C" fn(_:  &AzGlContextPtr, _:  bool, _:  bool, _:  bool, _:  bool) = transmute(lib.get(b"az_gl_context_ptr_color_mask")?);
            let az_gl_context_ptr_cull_face: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_cull_face")?);
            let az_gl_context_ptr_front_face: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_front_face")?);
            let az_gl_context_ptr_enable: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_enable")?);
            let az_gl_context_ptr_disable: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_disable")?);
            let az_gl_context_ptr_hint: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_hint")?);
            let az_gl_context_ptr_is_enabled: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8 = transmute(lib.get(b"az_gl_context_ptr_is_enabled")?);
            let az_gl_context_ptr_is_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8 = transmute(lib.get(b"az_gl_context_ptr_is_shader")?);
            let az_gl_context_ptr_is_texture: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8 = transmute(lib.get(b"az_gl_context_ptr_is_texture")?);
            let az_gl_context_ptr_is_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8 = transmute(lib.get(b"az_gl_context_ptr_is_framebuffer")?);
            let az_gl_context_ptr_is_renderbuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8 = transmute(lib.get(b"az_gl_context_ptr_is_renderbuffer")?);
            let az_gl_context_ptr_check_frame_buffer_status: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u32 = transmute(lib.get(b"az_gl_context_ptr_check_frame_buffer_status")?);
            let az_gl_context_ptr_enable_vertex_attrib_array: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_enable_vertex_attrib_array")?);
            let az_gl_context_ptr_disable_vertex_attrib_array: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_disable_vertex_attrib_array")?);
            let az_gl_context_ptr_uniform_1f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_uniform_1f")?);
            let az_gl_context_ptr_uniform_1fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_1fv")?);
            let az_gl_context_ptr_uniform_1i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_uniform_1i")?);
            let az_gl_context_ptr_uniform_1iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_1iv")?);
            let az_gl_context_ptr_uniform_1ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_uniform_1ui")?);
            let az_gl_context_ptr_uniform_2f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_uniform_2f")?);
            let az_gl_context_ptr_uniform_2fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_2fv")?);
            let az_gl_context_ptr_uniform_2i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_uniform_2i")?);
            let az_gl_context_ptr_uniform_2iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_2iv")?);
            let az_gl_context_ptr_uniform_2ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_uniform_2ui")?);
            let az_gl_context_ptr_uniform_3f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_uniform_3f")?);
            let az_gl_context_ptr_uniform_3fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_3fv")?);
            let az_gl_context_ptr_uniform_3i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_uniform_3i")?);
            let az_gl_context_ptr_uniform_3iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_3iv")?);
            let az_gl_context_ptr_uniform_3ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_uniform_3ui")?);
            let az_gl_context_ptr_uniform_4f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_uniform_4f")?);
            let az_gl_context_ptr_uniform_4i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_uniform_4i")?);
            let az_gl_context_ptr_uniform_4iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_4iv")?);
            let az_gl_context_ptr_uniform_4ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_uniform_4ui")?);
            let az_gl_context_ptr_uniform_4fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_4fv")?);
            let az_gl_context_ptr_uniform_matrix_2fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_matrix_2fv")?);
            let az_gl_context_ptr_uniform_matrix_3fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_matrix_3fv")?);
            let az_gl_context_ptr_uniform_matrix_4fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_matrix_4fv")?);
            let az_gl_context_ptr_depth_mask: extern "C" fn(_:  &AzGlContextPtr, _:  bool) = transmute(lib.get(b"az_gl_context_ptr_depth_mask")?);
            let az_gl_context_ptr_depth_range: extern "C" fn(_:  &AzGlContextPtr, _:  f64, _:  f64) = transmute(lib.get(b"az_gl_context_ptr_depth_range")?);
            let az_gl_context_ptr_get_active_attrib: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveAttribReturn = transmute(lib.get(b"az_gl_context_ptr_get_active_attrib")?);
            let az_gl_context_ptr_get_active_uniform: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveUniformReturn = transmute(lib.get(b"az_gl_context_ptr_get_active_uniform")?);
            let az_gl_context_ptr_get_active_uniforms_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLuintVec, _:  u32) -> AzGLintVec = transmute(lib.get(b"az_gl_context_ptr_get_active_uniforms_iv")?);
            let az_gl_context_ptr_get_active_uniform_block_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_active_uniform_block_i")?);
            let az_gl_context_ptr_get_active_uniform_block_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> AzGLintVec = transmute(lib.get(b"az_gl_context_ptr_get_active_uniform_block_iv")?);
            let az_gl_context_ptr_get_active_uniform_block_name: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString = transmute(lib.get(b"az_gl_context_ptr_get_active_uniform_block_name")?);
            let az_gl_context_ptr_get_attrib_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_attrib_location")?);
            let az_gl_context_ptr_get_frag_data_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_frag_data_location")?);
            let az_gl_context_ptr_get_uniform_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_uniform_location")?);
            let az_gl_context_ptr_get_program_info_log: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzString = transmute(lib.get(b"az_gl_context_ptr_get_program_info_log")?);
            let az_gl_context_ptr_get_program_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_program_iv")?);
            let az_gl_context_ptr_get_program_binary: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzGetProgramBinaryReturn = transmute(lib.get(b"az_gl_context_ptr_get_program_binary")?);
            let az_gl_context_ptr_program_binary: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_program_binary")?);
            let az_gl_context_ptr_program_parameter_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_program_parameter_i")?);
            let az_gl_context_ptr_get_vertex_attrib_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_vertex_attrib_iv")?);
            let az_gl_context_ptr_get_vertex_attrib_fv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLfloatVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_vertex_attrib_fv")?);
            let az_gl_context_ptr_get_vertex_attrib_pointer_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> isize = transmute(lib.get(b"az_gl_context_ptr_get_vertex_attrib_pointer_v")?);
            let az_gl_context_ptr_get_buffer_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_buffer_parameter_iv")?);
            let az_gl_context_ptr_get_shader_info_log: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzString = transmute(lib.get(b"az_gl_context_ptr_get_shader_info_log")?);
            let az_gl_context_ptr_get_string: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzString = transmute(lib.get(b"az_gl_context_ptr_get_string")?);
            let az_gl_context_ptr_get_string_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString = transmute(lib.get(b"az_gl_context_ptr_get_string_i")?);
            let az_gl_context_ptr_get_shader_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_shader_iv")?);
            let az_gl_context_ptr_get_shader_precision_format: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGlShaderPrecisionFormatReturn = transmute(lib.get(b"az_gl_context_ptr_get_shader_precision_format")?);
            let az_gl_context_ptr_compile_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_compile_shader")?);
            let az_gl_context_ptr_create_program: extern "C" fn(_:  &AzGlContextPtr) -> u32 = transmute(lib.get(b"az_gl_context_ptr_create_program")?);
            let az_gl_context_ptr_delete_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_delete_program")?);
            let az_gl_context_ptr_create_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u32 = transmute(lib.get(b"az_gl_context_ptr_create_shader")?);
            let az_gl_context_ptr_delete_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_delete_shader")?);
            let az_gl_context_ptr_detach_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_detach_shader")?);
            let az_gl_context_ptr_link_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_link_program")?);
            let az_gl_context_ptr_clear_color: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_clear_color")?);
            let az_gl_context_ptr_clear: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_clear")?);
            let az_gl_context_ptr_clear_depth: extern "C" fn(_:  &AzGlContextPtr, _:  f64) = transmute(lib.get(b"az_gl_context_ptr_clear_depth")?);
            let az_gl_context_ptr_clear_stencil: extern "C" fn(_:  &AzGlContextPtr, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_clear_stencil")?);
            let az_gl_context_ptr_flush: extern "C" fn(_:  &AzGlContextPtr) = transmute(lib.get(b"az_gl_context_ptr_flush")?);
            let az_gl_context_ptr_finish: extern "C" fn(_:  &AzGlContextPtr) = transmute(lib.get(b"az_gl_context_ptr_finish")?);
            let az_gl_context_ptr_get_error: extern "C" fn(_:  &AzGlContextPtr) -> u32 = transmute(lib.get(b"az_gl_context_ptr_get_error")?);
            let az_gl_context_ptr_stencil_mask: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_stencil_mask")?);
            let az_gl_context_ptr_stencil_mask_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_stencil_mask_separate")?);
            let az_gl_context_ptr_stencil_func: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_stencil_func")?);
            let az_gl_context_ptr_stencil_func_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_stencil_func_separate")?);
            let az_gl_context_ptr_stencil_op: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_stencil_op")?);
            let az_gl_context_ptr_stencil_op_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_stencil_op_separate")?);
            let az_gl_context_ptr_egl_image_target_texture2d_oes: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  *const c_void) = transmute(lib.get(b"az_gl_context_ptr_egl_image_target_texture2d_oes")?);
            let az_gl_context_ptr_generate_mipmap: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_generate_mipmap")?);
            let az_gl_context_ptr_insert_event_marker_ext: extern "C" fn(_:  &AzGlContextPtr, _:  AzRefstr) = transmute(lib.get(b"az_gl_context_ptr_insert_event_marker_ext")?);
            let az_gl_context_ptr_push_group_marker_ext: extern "C" fn(_:  &AzGlContextPtr, _:  AzRefstr) = transmute(lib.get(b"az_gl_context_ptr_push_group_marker_ext")?);
            let az_gl_context_ptr_pop_group_marker_ext: extern "C" fn(_:  &AzGlContextPtr) = transmute(lib.get(b"az_gl_context_ptr_pop_group_marker_ext")?);
            let az_gl_context_ptr_debug_message_insert_khr: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  AzRefstr) = transmute(lib.get(b"az_gl_context_ptr_debug_message_insert_khr")?);
            let az_gl_context_ptr_push_debug_group_khr: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr) = transmute(lib.get(b"az_gl_context_ptr_push_debug_group_khr")?);
            let az_gl_context_ptr_pop_debug_group_khr: extern "C" fn(_:  &AzGlContextPtr) = transmute(lib.get(b"az_gl_context_ptr_pop_debug_group_khr")?);
            let az_gl_context_ptr_fence_sync: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGLsyncPtr = transmute(lib.get(b"az_gl_context_ptr_fence_sync")?);
            let az_gl_context_ptr_client_wait_sync: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64) = transmute(lib.get(b"az_gl_context_ptr_client_wait_sync")?);
            let az_gl_context_ptr_wait_sync: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64) = transmute(lib.get(b"az_gl_context_ptr_wait_sync")?);
            let az_gl_context_ptr_delete_sync: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr) = transmute(lib.get(b"az_gl_context_ptr_delete_sync")?);
            let az_gl_context_ptr_texture_range_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_texture_range_apple")?);
            let az_gl_context_ptr_gen_fences_apple: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_fences_apple")?);
            let az_gl_context_ptr_delete_fences_apple: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_fences_apple")?);
            let az_gl_context_ptr_set_fence_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_set_fence_apple")?);
            let az_gl_context_ptr_finish_fence_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_finish_fence_apple")?);
            let az_gl_context_ptr_test_fence_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_test_fence_apple")?);
            let az_gl_context_ptr_test_object_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u8 = transmute(lib.get(b"az_gl_context_ptr_test_object_apple")?);
            let az_gl_context_ptr_finish_object_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_finish_object_apple")?);
            let az_gl_context_ptr_get_frag_data_index: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_frag_data_index")?);
            let az_gl_context_ptr_blend_barrier_khr: extern "C" fn(_:  &AzGlContextPtr) = transmute(lib.get(b"az_gl_context_ptr_blend_barrier_khr")?);
            let az_gl_context_ptr_bind_frag_data_location_indexed: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  AzRefstr) = transmute(lib.get(b"az_gl_context_ptr_bind_frag_data_location_indexed")?);
            let az_gl_context_ptr_get_debug_messages: extern "C" fn(_:  &AzGlContextPtr) -> AzDebugMessageVec = transmute(lib.get(b"az_gl_context_ptr_get_debug_messages")?);
            let az_gl_context_ptr_provoking_vertex_angle: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_provoking_vertex_angle")?);
            let az_gl_context_ptr_gen_vertex_arrays_apple: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_vertex_arrays_apple")?);
            let az_gl_context_ptr_bind_vertex_array_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_bind_vertex_array_apple")?);
            let az_gl_context_ptr_delete_vertex_arrays_apple: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_vertex_arrays_apple")?);
            let az_gl_context_ptr_copy_texture_chromium: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8) = transmute(lib.get(b"az_gl_context_ptr_copy_texture_chromium")?);
            let az_gl_context_ptr_copy_sub_texture_chromium: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8) = transmute(lib.get(b"az_gl_context_ptr_copy_sub_texture_chromium")?);
            let az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  *const c_void) = transmute(lib.get(b"az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes")?);
            let az_gl_context_ptr_copy_texture_3d_angle: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8) = transmute(lib.get(b"az_gl_context_ptr_copy_texture_3d_angle")?);
            let az_gl_context_ptr_copy_sub_texture_3d_angle: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8) = transmute(lib.get(b"az_gl_context_ptr_copy_sub_texture_3d_angle")?);
            let az_gl_context_ptr_delete: extern "C" fn(_:  &mut AzGlContextPtr) = transmute(lib.get(b"az_gl_context_ptr_delete")?);
            let az_gl_context_ptr_deep_copy: extern "C" fn(_:  &AzGlContextPtr) -> AzGlContextPtr = transmute(lib.get(b"az_gl_context_ptr_deep_copy")?);
            let az_texture_delete: extern "C" fn(_:  &mut AzTexture) = transmute(lib.get(b"az_texture_delete")?);
            let az_texture_flags_default: extern "C" fn() -> AzTextureFlags = transmute(lib.get(b"az_texture_flags_default")?);
            let az_raw_image_format_delete: extern "C" fn(_:  &mut AzRawImageFormat) = transmute(lib.get(b"az_raw_image_format_delete")?);
            let az_raw_image_format_deep_copy: extern "C" fn(_:  &AzRawImageFormat) -> AzRawImageFormat = transmute(lib.get(b"az_raw_image_format_deep_copy")?);
            let az_text_id_new: extern "C" fn() -> AzTextId = transmute(lib.get(b"az_text_id_new")?);
            let az_image_id_new: extern "C" fn() -> AzImageId = transmute(lib.get(b"az_image_id_new")?);
            let az_font_id_new: extern "C" fn() -> AzFontId = transmute(lib.get(b"az_font_id_new")?);
            let az_image_source_delete: extern "C" fn(_:  &mut AzImageSource) = transmute(lib.get(b"az_image_source_delete")?);
            let az_image_source_deep_copy: extern "C" fn(_:  &AzImageSource) -> AzImageSource = transmute(lib.get(b"az_image_source_deep_copy")?);
            let az_font_source_delete: extern "C" fn(_:  &mut AzFontSource) = transmute(lib.get(b"az_font_source_delete")?);
            let az_font_source_deep_copy: extern "C" fn(_:  &AzFontSource) -> AzFontSource = transmute(lib.get(b"az_font_source_deep_copy")?);
            let az_raw_image_new: extern "C" fn(_:  AzU8Vec, _:  usize, _:  usize, _:  AzRawImageFormat) -> AzRawImage = transmute(lib.get(b"az_raw_image_new")?);
            let az_raw_image_delete: extern "C" fn(_:  &mut AzRawImage) = transmute(lib.get(b"az_raw_image_delete")?);
            let az_raw_image_deep_copy: extern "C" fn(_:  &AzRawImage) -> AzRawImage = transmute(lib.get(b"az_raw_image_deep_copy")?);
            let az_svg_multi_polygon_delete: extern "C" fn(_:  &mut AzSvgMultiPolygon) = transmute(lib.get(b"az_svg_multi_polygon_delete")?);
            let az_svg_multi_polygon_deep_copy: extern "C" fn(_:  &AzSvgMultiPolygon) -> AzSvgMultiPolygon = transmute(lib.get(b"az_svg_multi_polygon_deep_copy")?);
            let az_svg_node_delete: extern "C" fn(_:  &mut AzSvgNode) = transmute(lib.get(b"az_svg_node_delete")?);
            let az_svg_node_deep_copy: extern "C" fn(_:  &AzSvgNode) -> AzSvgNode = transmute(lib.get(b"az_svg_node_deep_copy")?);
            let az_svg_styled_node_delete: extern "C" fn(_:  &mut AzSvgStyledNode) = transmute(lib.get(b"az_svg_styled_node_delete")?);
            let az_svg_styled_node_deep_copy: extern "C" fn(_:  &AzSvgStyledNode) -> AzSvgStyledNode = transmute(lib.get(b"az_svg_styled_node_deep_copy")?);
            let az_svg_path_delete: extern "C" fn(_:  &mut AzSvgPath) = transmute(lib.get(b"az_svg_path_delete")?);
            let az_svg_path_deep_copy: extern "C" fn(_:  &AzSvgPath) -> AzSvgPath = transmute(lib.get(b"az_svg_path_deep_copy")?);
            let az_tesselated_cpu_svg_node_delete: extern "C" fn(_:  &mut AzTesselatedCPUSvgNode) = transmute(lib.get(b"az_tesselated_cpu_svg_node_delete")?);
            let az_tesselated_cpu_svg_node_deep_copy: extern "C" fn(_:  &AzTesselatedCPUSvgNode) -> AzTesselatedCPUSvgNode = transmute(lib.get(b"az_tesselated_cpu_svg_node_deep_copy")?);
            let az_svg_parse_options_default: extern "C" fn() -> AzSvgParseOptions = transmute(lib.get(b"az_svg_parse_options_default")?);
            let az_svg_parse_options_delete: extern "C" fn(_:  &mut AzSvgParseOptions) = transmute(lib.get(b"az_svg_parse_options_delete")?);
            let az_svg_parse_options_deep_copy: extern "C" fn(_:  &AzSvgParseOptions) -> AzSvgParseOptions = transmute(lib.get(b"az_svg_parse_options_deep_copy")?);
            let az_svg_render_options_default: extern "C" fn() -> AzSvgRenderOptions = transmute(lib.get(b"az_svg_render_options_default")?);
            let az_svg_render_options_delete: extern "C" fn(_:  &mut AzSvgRenderOptions) = transmute(lib.get(b"az_svg_render_options_delete")?);
            let az_svg_render_options_deep_copy: extern "C" fn(_:  &AzSvgRenderOptions) -> AzSvgRenderOptions = transmute(lib.get(b"az_svg_render_options_deep_copy")?);
            let az_svg_parse: extern "C" fn(_:  AzU8VecRef, _:  AzSvgParseOptions) -> AzResultSvgSvgParseError = transmute(lib.get(b"az_svg_parse")?);
            let az_svg_delete: extern "C" fn(_:  &mut AzSvg) = transmute(lib.get(b"az_svg_delete")?);
            let az_svg_deep_copy: extern "C" fn(_:  &AzSvg) -> AzSvg = transmute(lib.get(b"az_svg_deep_copy")?);
            let az_svg_xml_node_delete: extern "C" fn(_:  &mut AzSvgXmlNode) = transmute(lib.get(b"az_svg_xml_node_delete")?);
            let az_svg_xml_node_deep_copy: extern "C" fn(_:  &AzSvgXmlNode) -> AzSvgXmlNode = transmute(lib.get(b"az_svg_xml_node_deep_copy")?);
            let az_timer_delete: extern "C" fn(_:  &mut AzTimer) = transmute(lib.get(b"az_timer_delete")?);
            let az_timer_deep_copy: extern "C" fn(_:  &AzTimer) -> AzTimer = transmute(lib.get(b"az_timer_deep_copy")?);
            let az_thread_sender_send: extern "C" fn(_:  &mut AzThreadSender, _:  AzThreadReceiveMsg) -> bool = transmute(lib.get(b"az_thread_sender_send")?);
            let az_thread_sender_delete: extern "C" fn(_:  &mut AzThreadSender) = transmute(lib.get(b"az_thread_sender_delete")?);
            let az_thread_receiver_receive: extern "C" fn(_:  &mut AzThreadReceiver) -> AzOptionThreadSendMsg = transmute(lib.get(b"az_thread_receiver_receive")?);
            let az_thread_receiver_delete: extern "C" fn(_:  &mut AzThreadReceiver) = transmute(lib.get(b"az_thread_receiver_delete")?);
            let az_thread_receive_msg_delete: extern "C" fn(_:  &mut AzThreadReceiveMsg) = transmute(lib.get(b"az_thread_receive_msg_delete")?);
            let az_thread_write_back_msg_delete: extern "C" fn(_:  &mut AzThreadWriteBackMsg) = transmute(lib.get(b"az_thread_write_back_msg_delete")?);
            let az_task_bar_icon_delete: extern "C" fn(_:  &mut AzTaskBarIcon) = transmute(lib.get(b"az_task_bar_icon_delete")?);
            let az_task_bar_icon_deep_copy: extern "C" fn(_:  &AzTaskBarIcon) -> AzTaskBarIcon = transmute(lib.get(b"az_task_bar_icon_deep_copy")?);
            let az_small_window_icon_bytes_delete: extern "C" fn(_:  &mut AzSmallWindowIconBytes) = transmute(lib.get(b"az_small_window_icon_bytes_delete")?);
            let az_small_window_icon_bytes_deep_copy: extern "C" fn(_:  &AzSmallWindowIconBytes) -> AzSmallWindowIconBytes = transmute(lib.get(b"az_small_window_icon_bytes_deep_copy")?);
            let az_large_window_icon_bytes_delete: extern "C" fn(_:  &mut AzLargeWindowIconBytes) = transmute(lib.get(b"az_large_window_icon_bytes_delete")?);
            let az_large_window_icon_bytes_deep_copy: extern "C" fn(_:  &AzLargeWindowIconBytes) -> AzLargeWindowIconBytes = transmute(lib.get(b"az_large_window_icon_bytes_deep_copy")?);
            let az_window_icon_delete: extern "C" fn(_:  &mut AzWindowIcon) = transmute(lib.get(b"az_window_icon_delete")?);
            let az_window_icon_deep_copy: extern "C" fn(_:  &AzWindowIcon) -> AzWindowIcon = transmute(lib.get(b"az_window_icon_deep_copy")?);
            let az_debug_state_delete: extern "C" fn(_:  &mut AzDebugState) = transmute(lib.get(b"az_debug_state_delete")?);
            let az_debug_state_deep_copy: extern "C" fn(_:  &AzDebugState) -> AzDebugState = transmute(lib.get(b"az_debug_state_deep_copy")?);
            let az_keyboard_state_delete: extern "C" fn(_:  &mut AzKeyboardState) = transmute(lib.get(b"az_keyboard_state_delete")?);
            let az_keyboard_state_deep_copy: extern "C" fn(_:  &AzKeyboardState) -> AzKeyboardState = transmute(lib.get(b"az_keyboard_state_deep_copy")?);
            let az_mouse_state_delete: extern "C" fn(_:  &mut AzMouseState) = transmute(lib.get(b"az_mouse_state_delete")?);
            let az_mouse_state_deep_copy: extern "C" fn(_:  &AzMouseState) -> AzMouseState = transmute(lib.get(b"az_mouse_state_deep_copy")?);
            let az_platform_specific_options_delete: extern "C" fn(_:  &mut AzPlatformSpecificOptions) = transmute(lib.get(b"az_platform_specific_options_delete")?);
            let az_platform_specific_options_deep_copy: extern "C" fn(_:  &AzPlatformSpecificOptions) -> AzPlatformSpecificOptions = transmute(lib.get(b"az_platform_specific_options_deep_copy")?);
            let az_windows_window_options_delete: extern "C" fn(_:  &mut AzWindowsWindowOptions) = transmute(lib.get(b"az_windows_window_options_delete")?);
            let az_windows_window_options_deep_copy: extern "C" fn(_:  &AzWindowsWindowOptions) -> AzWindowsWindowOptions = transmute(lib.get(b"az_windows_window_options_deep_copy")?);
            let az_wayland_theme_delete: extern "C" fn(_:  &mut AzWaylandTheme) = transmute(lib.get(b"az_wayland_theme_delete")?);
            let az_wayland_theme_deep_copy: extern "C" fn(_:  &AzWaylandTheme) -> AzWaylandTheme = transmute(lib.get(b"az_wayland_theme_deep_copy")?);
            let az_renderer_type_delete: extern "C" fn(_:  &mut AzRendererType) = transmute(lib.get(b"az_renderer_type_delete")?);
            let az_renderer_type_deep_copy: extern "C" fn(_:  &AzRendererType) -> AzRendererType = transmute(lib.get(b"az_renderer_type_deep_copy")?);
            let az_string_pair_delete: extern "C" fn(_:  &mut AzStringPair) = transmute(lib.get(b"az_string_pair_delete")?);
            let az_string_pair_deep_copy: extern "C" fn(_:  &AzStringPair) -> AzStringPair = transmute(lib.get(b"az_string_pair_deep_copy")?);
            let az_linux_window_options_delete: extern "C" fn(_:  &mut AzLinuxWindowOptions) = transmute(lib.get(b"az_linux_window_options_delete")?);
            let az_linux_window_options_deep_copy: extern "C" fn(_:  &AzLinuxWindowOptions) -> AzLinuxWindowOptions = transmute(lib.get(b"az_linux_window_options_deep_copy")?);
            let az_window_state_new: extern "C" fn(_:  AzLayoutCallbackType) -> AzWindowState = transmute(lib.get(b"az_window_state_new")?);
            let az_window_state_delete: extern "C" fn(_:  &mut AzWindowState) = transmute(lib.get(b"az_window_state_delete")?);
            let az_window_state_deep_copy: extern "C" fn(_:  &AzWindowState) -> AzWindowState = transmute(lib.get(b"az_window_state_deep_copy")?);
            let az_window_create_options_new: extern "C" fn(_:  AzLayoutCallbackType) -> AzWindowCreateOptions = transmute(lib.get(b"az_window_create_options_new")?);
            let az_window_create_options_delete: extern "C" fn(_:  &mut AzWindowCreateOptions) = transmute(lib.get(b"az_window_create_options_delete")?);
            let az_window_create_options_deep_copy: extern "C" fn(_:  &AzWindowCreateOptions) -> AzWindowCreateOptions = transmute(lib.get(b"az_window_create_options_deep_copy")?);
            Some(AzulDll {
                lib: lib,
                az_string_from_utf8_unchecked,
                az_string_from_utf8_lossy,
                az_string_into_bytes,
                az_string_delete,
                az_string_deep_copy,
                az_style_transform_vec_new,
                az_style_transform_vec_with_capacity,
                az_style_transform_vec_copy_from,
                az_style_transform_vec_delete,
                az_style_transform_vec_deep_copy,
                az_content_group_vec_new,
                az_content_group_vec_with_capacity,
                az_content_group_vec_copy_from,
                az_content_group_vec_delete,
                az_content_group_vec_deep_copy,
                az_css_property_vec_new,
                az_css_property_vec_with_capacity,
                az_css_property_vec_copy_from,
                az_css_property_vec_delete,
                az_css_property_vec_deep_copy,
                az_svg_multi_polygon_vec_new,
                az_svg_multi_polygon_vec_with_capacity,
                az_svg_multi_polygon_vec_copy_from,
                az_svg_multi_polygon_vec_delete,
                az_svg_multi_polygon_vec_deep_copy,
                az_svg_path_vec_new,
                az_svg_path_vec_with_capacity,
                az_svg_path_vec_copy_from,
                az_svg_path_vec_delete,
                az_svg_path_vec_deep_copy,
                az_vertex_attribute_vec_new,
                az_vertex_attribute_vec_with_capacity,
                az_vertex_attribute_vec_copy_from,
                az_vertex_attribute_vec_delete,
                az_vertex_attribute_vec_deep_copy,
                az_svg_path_element_vec_new,
                az_svg_path_element_vec_with_capacity,
                az_svg_path_element_vec_copy_from,
                az_svg_path_element_vec_delete,
                az_svg_path_element_vec_deep_copy,
                az_svg_vertex_vec_new,
                az_svg_vertex_vec_with_capacity,
                az_svg_vertex_vec_copy_from,
                az_svg_vertex_vec_delete,
                az_svg_vertex_vec_deep_copy,
                az_u32_vec_new,
                az_u32_vec_with_capacity,
                az_u32_vec_copy_from,
                az_u32_vec_delete,
                az_u32_vec_deep_copy,
                az_x_window_type_vec_new,
                az_x_window_type_vec_with_capacity,
                az_x_window_type_vec_copy_from,
                az_x_window_type_vec_delete,
                az_x_window_type_vec_deep_copy,
                az_virtual_key_code_vec_new,
                az_virtual_key_code_vec_with_capacity,
                az_virtual_key_code_vec_copy_from,
                az_virtual_key_code_vec_delete,
                az_virtual_key_code_vec_deep_copy,
                az_cascade_info_vec_new,
                az_cascade_info_vec_with_capacity,
                az_cascade_info_vec_copy_from,
                az_cascade_info_vec_delete,
                az_cascade_info_vec_deep_copy,
                az_scan_code_vec_new,
                az_scan_code_vec_with_capacity,
                az_scan_code_vec_copy_from,
                az_scan_code_vec_delete,
                az_scan_code_vec_deep_copy,
                az_css_declaration_vec_new,
                az_css_declaration_vec_with_capacity,
                az_css_declaration_vec_copy_from,
                az_css_declaration_vec_delete,
                az_css_declaration_vec_deep_copy,
                az_css_path_selector_vec_new,
                az_css_path_selector_vec_with_capacity,
                az_css_path_selector_vec_copy_from,
                az_css_path_selector_vec_delete,
                az_css_path_selector_vec_deep_copy,
                az_stylesheet_vec_new,
                az_stylesheet_vec_with_capacity,
                az_stylesheet_vec_copy_from,
                az_stylesheet_vec_delete,
                az_stylesheet_vec_deep_copy,
                az_css_rule_block_vec_new,
                az_css_rule_block_vec_with_capacity,
                az_css_rule_block_vec_copy_from,
                az_css_rule_block_vec_delete,
                az_css_rule_block_vec_deep_copy,
                az_u8_vec_new,
                az_u8_vec_with_capacity,
                az_u8_vec_copy_from,
                az_u8_vec_delete,
                az_u8_vec_deep_copy,
                az_callback_data_vec_new,
                az_callback_data_vec_with_capacity,
                az_callback_data_vec_copy_from,
                az_callback_data_vec_delete,
                az_callback_data_vec_deep_copy,
                az_debug_message_vec_new,
                az_debug_message_vec_with_capacity,
                az_debug_message_vec_copy_from,
                az_debug_message_vec_delete,
                az_debug_message_vec_deep_copy,
                az_g_luint_vec_new,
                az_g_luint_vec_with_capacity,
                az_g_luint_vec_copy_from,
                az_g_luint_vec_delete,
                az_g_luint_vec_deep_copy,
                az_g_lint_vec_new,
                az_g_lint_vec_with_capacity,
                az_g_lint_vec_copy_from,
                az_g_lint_vec_delete,
                az_g_lint_vec_deep_copy,
                az_dom_vec_new,
                az_dom_vec_with_capacity,
                az_dom_vec_copy_from,
                az_dom_vec_delete,
                az_dom_vec_deep_copy,
                az_string_vec_new,
                az_string_vec_with_capacity,
                az_string_vec_copy_from,
                az_string_vec_delete,
                az_string_vec_deep_copy,
                az_string_pair_vec_new,
                az_string_pair_vec_with_capacity,
                az_string_pair_vec_copy_from,
                az_string_pair_vec_delete,
                az_string_pair_vec_deep_copy,
                az_gradient_stop_pre_vec_new,
                az_gradient_stop_pre_vec_with_capacity,
                az_gradient_stop_pre_vec_copy_from,
                az_gradient_stop_pre_vec_delete,
                az_gradient_stop_pre_vec_deep_copy,
                az_cascaded_css_property_with_source_vec_new,
                az_cascaded_css_property_with_source_vec_with_capacity,
                az_cascaded_css_property_with_source_vec_copy_from,
                az_cascaded_css_property_with_source_vec_delete,
                az_cascaded_css_property_with_source_vec_deep_copy,
                az_node_id_vec_new,
                az_node_id_vec_with_capacity,
                az_node_id_vec_copy_from,
                az_node_id_vec_delete,
                az_node_id_vec_deep_copy,
                az_node_vec_new,
                az_node_vec_with_capacity,
                az_node_vec_copy_from,
                az_node_vec_delete,
                az_node_vec_deep_copy,
                az_styled_node_vec_new,
                az_styled_node_vec_with_capacity,
                az_styled_node_vec_copy_from,
                az_styled_node_vec_delete,
                az_styled_node_vec_deep_copy,
                az_tag_ids_to_node_ids_mapping_vec_new,
                az_tag_ids_to_node_ids_mapping_vec_with_capacity,
                az_tag_ids_to_node_ids_mapping_vec_copy_from,
                az_tag_ids_to_node_ids_mapping_vec_delete,
                az_tag_ids_to_node_ids_mapping_vec_deep_copy,
                az_parent_with_node_depth_vec_new,
                az_parent_with_node_depth_vec_with_capacity,
                az_parent_with_node_depth_vec_copy_from,
                az_parent_with_node_depth_vec_delete,
                az_parent_with_node_depth_vec_deep_copy,
                az_node_data_vec_new,
                az_node_data_vec_with_capacity,
                az_node_data_vec_copy_from,
                az_node_data_vec_delete,
                az_node_data_vec_deep_copy,
                az_option_ref_any_delete,
                az_option_ref_any_deep_copy,
                az_option_style_transform_vec_value_delete,
                az_option_style_transform_vec_value_deep_copy,
                az_option_raw_image_delete,
                az_option_raw_image_deep_copy,
                az_option_wayland_theme_delete,
                az_option_wayland_theme_deep_copy,
                az_option_task_bar_icon_delete,
                az_option_task_bar_icon_deep_copy,
                az_option_window_icon_delete,
                az_option_window_icon_deep_copy,
                az_option_string_delete,
                az_option_string_deep_copy,
                az_option_dom_delete,
                az_option_dom_deep_copy,
                az_option_texture_delete,
                az_option_image_mask_delete,
                az_option_image_mask_deep_copy,
                az_option_style_background_content_value_delete,
                az_option_style_background_content_value_deep_copy,
                az_option_style_font_family_value_delete,
                az_option_style_font_family_value_deep_copy,
                az_option_box_shadow_pre_display_item_value_delete,
                az_option_box_shadow_pre_display_item_value_deep_copy,
                az_option_instant_ptr_delete,
                az_option_instant_ptr_deep_copy,
                az_option_u8_vec_ref_delete,
                az_result_svg_svg_parse_error_delete,
                az_result_svg_svg_parse_error_deep_copy,
                az_svg_parse_error_delete,
                az_svg_parse_error_deep_copy,
                az_xml_error_delete,
                az_xml_error_deep_copy,
                az_duplicated_namespace_error_delete,
                az_duplicated_namespace_error_deep_copy,
                az_unknown_namespace_error_delete,
                az_unknown_namespace_error_deep_copy,
                az_unexpected_close_tag_error_delete,
                az_unexpected_close_tag_error_deep_copy,
                az_unknown_entity_reference_error_delete,
                az_unknown_entity_reference_error_deep_copy,
                az_duplicated_attribute_error_delete,
                az_duplicated_attribute_error_deep_copy,
                az_xml_parse_error_delete,
                az_xml_parse_error_deep_copy,
                az_xml_text_error_delete,
                az_xml_text_error_deep_copy,
                az_xml_stream_error_delete,
                az_xml_stream_error_deep_copy,
                az_invalid_char_multiple_error_delete,
                az_invalid_char_multiple_error_deep_copy,
                az_invalid_string_error_delete,
                az_invalid_string_error_deep_copy,
                az_instant_ptr_now,
                az_instant_ptr_delete,
                az_app_config_default,
                az_app_config_delete,
                az_app_config_deep_copy,
                az_app_ptr_new,
                az_app_ptr_add_window,
                az_app_ptr_run,
                az_app_ptr_delete,
                az_hidpi_adjusted_bounds_get_logical_size,
                az_hidpi_adjusted_bounds_get_physical_size,
                az_hidpi_adjusted_bounds_get_hidpi_factor,
                az_focus_target_delete,
                az_focus_target_deep_copy,
                az_focus_target_path_delete,
                az_focus_target_path_deep_copy,
                az_callback_info_get_hit_node,
                az_callback_info_get_cursor_relative_to_viewport,
                az_callback_info_get_cursor_relative_to_node,
                az_callback_info_get_parent,
                az_callback_info_get_previous_sibling,
                az_callback_info_get_next_sibling,
                az_callback_info_get_first_child,
                az_callback_info_get_last_child,
                az_callback_info_get_dataset,
                az_callback_info_get_window_state,
                az_callback_info_get_keyboard_state,
                az_callback_info_get_mouse_state,
                az_callback_info_get_current_window_handle,
                az_callback_info_get_gl_context,
                az_callback_info_set_window_state,
                az_callback_info_set_focus,
                az_callback_info_set_css_property,
                az_callback_info_stop_propagation,
                az_callback_info_create_window,
                az_callback_info_start_thread,
                az_callback_info_start_timer,
                az_callback_info_delete,
                az_i_frame_callback_info_get_bounds,
                az_i_frame_callback_info_delete,
                az_i_frame_callback_return_delete,
                az_i_frame_callback_return_deep_copy,
                az_gl_callback_info_get_gl_context,
                az_gl_callback_info_delete,
                az_gl_callback_return_delete,
                az_timer_callback_info_delete,
                az_timer_callback_return_delete,
                az_timer_callback_return_deep_copy,
                az_write_back_callback_delete,
                az_write_back_callback_deep_copy,
                az_atomic_ref_count_can_be_shared,
                az_atomic_ref_count_can_be_shared_mut,
                az_atomic_ref_count_increase_ref,
                az_atomic_ref_count_decrease_ref,
                az_atomic_ref_count_increase_refmut,
                az_atomic_ref_count_decrease_refmut,
                az_atomic_ref_count_delete,
                az_ref_any_new_c,
                az_ref_any_is_type,
                az_ref_any_get_type_name,
                az_ref_any_can_be_shared,
                az_ref_any_can_be_shared_mut,
                az_ref_any_increase_ref,
                az_ref_any_decrease_ref,
                az_ref_any_increase_refmut,
                az_ref_any_decrease_refmut,
                az_ref_any_delete,
                az_ref_any_deep_copy,
                az_layout_info_window_width_larger_than,
                az_layout_info_window_width_smaller_than,
                az_layout_info_window_height_larger_than,
                az_layout_info_window_height_smaller_than,
                az_layout_info_delete,
                az_css_rule_block_delete,
                az_css_rule_block_deep_copy,
                az_css_declaration_delete,
                az_css_declaration_deep_copy,
                az_dynamic_css_property_delete,
                az_dynamic_css_property_deep_copy,
                az_css_path_delete,
                az_css_path_deep_copy,
                az_css_path_selector_delete,
                az_css_path_selector_deep_copy,
                az_stylesheet_delete,
                az_stylesheet_deep_copy,
                az_css_empty,
                az_css_from_string,
                az_css_delete,
                az_css_deep_copy,
                az_color_u_from_str,
                az_color_u_to_hash,
                az_linear_gradient_delete,
                az_linear_gradient_deep_copy,
                az_radial_gradient_delete,
                az_radial_gradient_deep_copy,
                az_css_image_id_delete,
                az_css_image_id_deep_copy,
                az_style_background_content_delete,
                az_style_background_content_deep_copy,
                az_style_font_family_delete,
                az_style_font_family_deep_copy,
                az_style_background_content_value_delete,
                az_style_background_content_value_deep_copy,
                az_style_font_family_value_delete,
                az_style_font_family_value_deep_copy,
                az_style_transform_vec_value_delete,
                az_style_transform_vec_value_deep_copy,
                az_css_property_delete,
                az_css_property_deep_copy,
                az_rect_style_delete,
                az_rect_style_deep_copy,
                az_rect_layout_delete,
                az_rect_layout_deep_copy,
                az_cascaded_css_property_with_source_delete,
                az_cascaded_css_property_with_source_deep_copy,
                az_css_property_source_delete,
                az_css_property_source_deep_copy,
                az_styled_node_delete,
                az_styled_node_deep_copy,
                az_content_group_delete,
                az_content_group_deep_copy,
                az_styled_dom_new,
                az_styled_dom_append,
                az_styled_dom_delete,
                az_styled_dom_deep_copy,
                az_dom_new,
                az_dom_div,
                az_dom_body,
                az_dom_label,
                az_dom_text,
                az_dom_image,
                az_dom_gl_texture,
                az_dom_iframe,
                az_dom_add_id,
                az_dom_with_id,
                az_dom_set_ids,
                az_dom_with_ids,
                az_dom_add_class,
                az_dom_with_class,
                az_dom_set_classes,
                az_dom_with_classes,
                az_dom_add_callback,
                az_dom_with_callback,
                az_dom_set_dataset,
                az_dom_with_dataset,
                az_dom_add_inline_css,
                az_dom_with_inline_css,
                az_dom_add_inline_hover_css,
                az_dom_with_inline_hover_css,
                az_dom_add_inline_active_css,
                az_dom_with_inline_active_css,
                az_dom_add_inline_focus_css,
                az_dom_with_inline_focus_css,
                az_dom_set_is_draggable,
                az_dom_with_clip_mask,
                az_dom_set_clip_mask,
                az_dom_is_draggable,
                az_dom_set_tab_index,
                az_dom_with_tab_index,
                az_dom_add_child,
                az_dom_with_child,
                az_dom_get_html_string,
                az_dom_style,
                az_dom_delete,
                az_dom_deep_copy,
                az_gl_texture_node_delete,
                az_gl_texture_node_deep_copy,
                az_i_frame_node_delete,
                az_i_frame_node_deep_copy,
                az_callback_data_delete,
                az_callback_data_deep_copy,
                az_image_mask_delete,
                az_image_mask_deep_copy,
                az_node_data_new,
                az_node_data_div,
                az_node_data_body,
                az_node_data_label,
                az_node_data_text,
                az_node_data_image,
                az_node_data_gl_texture,
                az_node_data_iframe,
                az_node_data_default,
                az_node_data_add_id,
                az_node_data_with_id,
                az_node_data_set_ids,
                az_node_data_with_ids,
                az_node_data_add_class,
                az_node_data_with_class,
                az_node_data_set_classes,
                az_node_data_with_classes,
                az_node_data_add_dataset,
                az_node_data_with_dataset,
                az_node_data_add_callback,
                az_node_data_with_callback,
                az_node_data_add_inline_css,
                az_node_data_with_inline_css,
                az_node_data_add_inline_hover_css,
                az_node_data_add_inline_active_css,
                az_node_data_add_inline_focus_css,
                az_node_data_with_clip_mask,
                az_node_data_set_clip_mask,
                az_node_data_set_is_draggable,
                az_node_data_is_draggable,
                az_node_data_set_tab_index,
                az_node_data_with_tab_index,
                az_node_data_delete,
                az_node_data_deep_copy,
                az_node_type_delete,
                az_node_type_deep_copy,
                az_on_into_event_filter,
                az_vertex_attribute_delete,
                az_vertex_attribute_deep_copy,
                az_vertex_layout_delete,
                az_vertex_layout_deep_copy,
                az_vertex_array_object_delete,
                az_vertex_buffer_delete,
                az_debug_message_delete,
                az_debug_message_deep_copy,
                az_u8_vec_ref_delete,
                az_u8_vec_ref_mut_delete,
                az_f32_vec_ref_delete,
                az_i32_vec_ref_delete,
                az_g_luint_vec_ref_delete,
                az_g_lenum_vec_ref_delete,
                az_g_lint_vec_ref_mut_delete,
                az_g_lint64_vec_ref_mut_delete,
                az_g_lboolean_vec_ref_mut_delete,
                az_g_lfloat_vec_ref_mut_delete,
                az_refstr_vec_ref_delete,
                az_refstr_delete,
                az_get_program_binary_return_delete,
                az_get_program_binary_return_deep_copy,
                az_get_active_attrib_return_delete,
                az_get_active_attrib_return_deep_copy,
                az_g_lsync_ptr_delete,
                az_get_active_uniform_return_delete,
                az_get_active_uniform_return_deep_copy,
                az_gl_context_ptr_get_type,
                az_gl_context_ptr_buffer_data_untyped,
                az_gl_context_ptr_buffer_sub_data_untyped,
                az_gl_context_ptr_map_buffer,
                az_gl_context_ptr_map_buffer_range,
                az_gl_context_ptr_unmap_buffer,
                az_gl_context_ptr_tex_buffer,
                az_gl_context_ptr_shader_source,
                az_gl_context_ptr_read_buffer,
                az_gl_context_ptr_read_pixels_into_buffer,
                az_gl_context_ptr_read_pixels,
                az_gl_context_ptr_read_pixels_into_pbo,
                az_gl_context_ptr_sample_coverage,
                az_gl_context_ptr_polygon_offset,
                az_gl_context_ptr_pixel_store_i,
                az_gl_context_ptr_gen_buffers,
                az_gl_context_ptr_gen_renderbuffers,
                az_gl_context_ptr_gen_framebuffers,
                az_gl_context_ptr_gen_textures,
                az_gl_context_ptr_gen_vertex_arrays,
                az_gl_context_ptr_gen_queries,
                az_gl_context_ptr_begin_query,
                az_gl_context_ptr_end_query,
                az_gl_context_ptr_query_counter,
                az_gl_context_ptr_get_query_object_iv,
                az_gl_context_ptr_get_query_object_uiv,
                az_gl_context_ptr_get_query_object_i64v,
                az_gl_context_ptr_get_query_object_ui64v,
                az_gl_context_ptr_delete_queries,
                az_gl_context_ptr_delete_vertex_arrays,
                az_gl_context_ptr_delete_buffers,
                az_gl_context_ptr_delete_renderbuffers,
                az_gl_context_ptr_delete_framebuffers,
                az_gl_context_ptr_delete_textures,
                az_gl_context_ptr_framebuffer_renderbuffer,
                az_gl_context_ptr_renderbuffer_storage,
                az_gl_context_ptr_depth_func,
                az_gl_context_ptr_active_texture,
                az_gl_context_ptr_attach_shader,
                az_gl_context_ptr_bind_attrib_location,
                az_gl_context_ptr_get_uniform_iv,
                az_gl_context_ptr_get_uniform_fv,
                az_gl_context_ptr_get_uniform_block_index,
                az_gl_context_ptr_get_uniform_indices,
                az_gl_context_ptr_bind_buffer_base,
                az_gl_context_ptr_bind_buffer_range,
                az_gl_context_ptr_uniform_block_binding,
                az_gl_context_ptr_bind_buffer,
                az_gl_context_ptr_bind_vertex_array,
                az_gl_context_ptr_bind_renderbuffer,
                az_gl_context_ptr_bind_framebuffer,
                az_gl_context_ptr_bind_texture,
                az_gl_context_ptr_draw_buffers,
                az_gl_context_ptr_tex_image_2d,
                az_gl_context_ptr_compressed_tex_image_2d,
                az_gl_context_ptr_compressed_tex_sub_image_2d,
                az_gl_context_ptr_tex_image_3d,
                az_gl_context_ptr_copy_tex_image_2d,
                az_gl_context_ptr_copy_tex_sub_image_2d,
                az_gl_context_ptr_copy_tex_sub_image_3d,
                az_gl_context_ptr_tex_sub_image_2d,
                az_gl_context_ptr_tex_sub_image_2d_pbo,
                az_gl_context_ptr_tex_sub_image_3d,
                az_gl_context_ptr_tex_sub_image_3d_pbo,
                az_gl_context_ptr_tex_storage_2d,
                az_gl_context_ptr_tex_storage_3d,
                az_gl_context_ptr_get_tex_image_into_buffer,
                az_gl_context_ptr_copy_image_sub_data,
                az_gl_context_ptr_invalidate_framebuffer,
                az_gl_context_ptr_invalidate_sub_framebuffer,
                az_gl_context_ptr_get_integer_v,
                az_gl_context_ptr_get_integer_64v,
                az_gl_context_ptr_get_integer_iv,
                az_gl_context_ptr_get_integer_64iv,
                az_gl_context_ptr_get_boolean_v,
                az_gl_context_ptr_get_float_v,
                az_gl_context_ptr_get_framebuffer_attachment_parameter_iv,
                az_gl_context_ptr_get_renderbuffer_parameter_iv,
                az_gl_context_ptr_get_tex_parameter_iv,
                az_gl_context_ptr_get_tex_parameter_fv,
                az_gl_context_ptr_tex_parameter_i,
                az_gl_context_ptr_tex_parameter_f,
                az_gl_context_ptr_framebuffer_texture_2d,
                az_gl_context_ptr_framebuffer_texture_layer,
                az_gl_context_ptr_blit_framebuffer,
                az_gl_context_ptr_vertex_attrib_4f,
                az_gl_context_ptr_vertex_attrib_pointer_f32,
                az_gl_context_ptr_vertex_attrib_pointer,
                az_gl_context_ptr_vertex_attrib_i_pointer,
                az_gl_context_ptr_vertex_attrib_divisor,
                az_gl_context_ptr_viewport,
                az_gl_context_ptr_scissor,
                az_gl_context_ptr_line_width,
                az_gl_context_ptr_use_program,
                az_gl_context_ptr_validate_program,
                az_gl_context_ptr_draw_arrays,
                az_gl_context_ptr_draw_arrays_instanced,
                az_gl_context_ptr_draw_elements,
                az_gl_context_ptr_draw_elements_instanced,
                az_gl_context_ptr_blend_color,
                az_gl_context_ptr_blend_func,
                az_gl_context_ptr_blend_func_separate,
                az_gl_context_ptr_blend_equation,
                az_gl_context_ptr_blend_equation_separate,
                az_gl_context_ptr_color_mask,
                az_gl_context_ptr_cull_face,
                az_gl_context_ptr_front_face,
                az_gl_context_ptr_enable,
                az_gl_context_ptr_disable,
                az_gl_context_ptr_hint,
                az_gl_context_ptr_is_enabled,
                az_gl_context_ptr_is_shader,
                az_gl_context_ptr_is_texture,
                az_gl_context_ptr_is_framebuffer,
                az_gl_context_ptr_is_renderbuffer,
                az_gl_context_ptr_check_frame_buffer_status,
                az_gl_context_ptr_enable_vertex_attrib_array,
                az_gl_context_ptr_disable_vertex_attrib_array,
                az_gl_context_ptr_uniform_1f,
                az_gl_context_ptr_uniform_1fv,
                az_gl_context_ptr_uniform_1i,
                az_gl_context_ptr_uniform_1iv,
                az_gl_context_ptr_uniform_1ui,
                az_gl_context_ptr_uniform_2f,
                az_gl_context_ptr_uniform_2fv,
                az_gl_context_ptr_uniform_2i,
                az_gl_context_ptr_uniform_2iv,
                az_gl_context_ptr_uniform_2ui,
                az_gl_context_ptr_uniform_3f,
                az_gl_context_ptr_uniform_3fv,
                az_gl_context_ptr_uniform_3i,
                az_gl_context_ptr_uniform_3iv,
                az_gl_context_ptr_uniform_3ui,
                az_gl_context_ptr_uniform_4f,
                az_gl_context_ptr_uniform_4i,
                az_gl_context_ptr_uniform_4iv,
                az_gl_context_ptr_uniform_4ui,
                az_gl_context_ptr_uniform_4fv,
                az_gl_context_ptr_uniform_matrix_2fv,
                az_gl_context_ptr_uniform_matrix_3fv,
                az_gl_context_ptr_uniform_matrix_4fv,
                az_gl_context_ptr_depth_mask,
                az_gl_context_ptr_depth_range,
                az_gl_context_ptr_get_active_attrib,
                az_gl_context_ptr_get_active_uniform,
                az_gl_context_ptr_get_active_uniforms_iv,
                az_gl_context_ptr_get_active_uniform_block_i,
                az_gl_context_ptr_get_active_uniform_block_iv,
                az_gl_context_ptr_get_active_uniform_block_name,
                az_gl_context_ptr_get_attrib_location,
                az_gl_context_ptr_get_frag_data_location,
                az_gl_context_ptr_get_uniform_location,
                az_gl_context_ptr_get_program_info_log,
                az_gl_context_ptr_get_program_iv,
                az_gl_context_ptr_get_program_binary,
                az_gl_context_ptr_program_binary,
                az_gl_context_ptr_program_parameter_i,
                az_gl_context_ptr_get_vertex_attrib_iv,
                az_gl_context_ptr_get_vertex_attrib_fv,
                az_gl_context_ptr_get_vertex_attrib_pointer_v,
                az_gl_context_ptr_get_buffer_parameter_iv,
                az_gl_context_ptr_get_shader_info_log,
                az_gl_context_ptr_get_string,
                az_gl_context_ptr_get_string_i,
                az_gl_context_ptr_get_shader_iv,
                az_gl_context_ptr_get_shader_precision_format,
                az_gl_context_ptr_compile_shader,
                az_gl_context_ptr_create_program,
                az_gl_context_ptr_delete_program,
                az_gl_context_ptr_create_shader,
                az_gl_context_ptr_delete_shader,
                az_gl_context_ptr_detach_shader,
                az_gl_context_ptr_link_program,
                az_gl_context_ptr_clear_color,
                az_gl_context_ptr_clear,
                az_gl_context_ptr_clear_depth,
                az_gl_context_ptr_clear_stencil,
                az_gl_context_ptr_flush,
                az_gl_context_ptr_finish,
                az_gl_context_ptr_get_error,
                az_gl_context_ptr_stencil_mask,
                az_gl_context_ptr_stencil_mask_separate,
                az_gl_context_ptr_stencil_func,
                az_gl_context_ptr_stencil_func_separate,
                az_gl_context_ptr_stencil_op,
                az_gl_context_ptr_stencil_op_separate,
                az_gl_context_ptr_egl_image_target_texture2d_oes,
                az_gl_context_ptr_generate_mipmap,
                az_gl_context_ptr_insert_event_marker_ext,
                az_gl_context_ptr_push_group_marker_ext,
                az_gl_context_ptr_pop_group_marker_ext,
                az_gl_context_ptr_debug_message_insert_khr,
                az_gl_context_ptr_push_debug_group_khr,
                az_gl_context_ptr_pop_debug_group_khr,
                az_gl_context_ptr_fence_sync,
                az_gl_context_ptr_client_wait_sync,
                az_gl_context_ptr_wait_sync,
                az_gl_context_ptr_delete_sync,
                az_gl_context_ptr_texture_range_apple,
                az_gl_context_ptr_gen_fences_apple,
                az_gl_context_ptr_delete_fences_apple,
                az_gl_context_ptr_set_fence_apple,
                az_gl_context_ptr_finish_fence_apple,
                az_gl_context_ptr_test_fence_apple,
                az_gl_context_ptr_test_object_apple,
                az_gl_context_ptr_finish_object_apple,
                az_gl_context_ptr_get_frag_data_index,
                az_gl_context_ptr_blend_barrier_khr,
                az_gl_context_ptr_bind_frag_data_location_indexed,
                az_gl_context_ptr_get_debug_messages,
                az_gl_context_ptr_provoking_vertex_angle,
                az_gl_context_ptr_gen_vertex_arrays_apple,
                az_gl_context_ptr_bind_vertex_array_apple,
                az_gl_context_ptr_delete_vertex_arrays_apple,
                az_gl_context_ptr_copy_texture_chromium,
                az_gl_context_ptr_copy_sub_texture_chromium,
                az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes,
                az_gl_context_ptr_copy_texture_3d_angle,
                az_gl_context_ptr_copy_sub_texture_3d_angle,
                az_gl_context_ptr_delete,
                az_gl_context_ptr_deep_copy,
                az_texture_delete,
                az_texture_flags_default,
                az_raw_image_format_delete,
                az_raw_image_format_deep_copy,
                az_text_id_new,
                az_image_id_new,
                az_font_id_new,
                az_image_source_delete,
                az_image_source_deep_copy,
                az_font_source_delete,
                az_font_source_deep_copy,
                az_raw_image_new,
                az_raw_image_delete,
                az_raw_image_deep_copy,
                az_svg_multi_polygon_delete,
                az_svg_multi_polygon_deep_copy,
                az_svg_node_delete,
                az_svg_node_deep_copy,
                az_svg_styled_node_delete,
                az_svg_styled_node_deep_copy,
                az_svg_path_delete,
                az_svg_path_deep_copy,
                az_tesselated_cpu_svg_node_delete,
                az_tesselated_cpu_svg_node_deep_copy,
                az_svg_parse_options_default,
                az_svg_parse_options_delete,
                az_svg_parse_options_deep_copy,
                az_svg_render_options_default,
                az_svg_render_options_delete,
                az_svg_render_options_deep_copy,
                az_svg_parse,
                az_svg_delete,
                az_svg_deep_copy,
                az_svg_xml_node_delete,
                az_svg_xml_node_deep_copy,
                az_timer_delete,
                az_timer_deep_copy,
                az_thread_sender_send,
                az_thread_sender_delete,
                az_thread_receiver_receive,
                az_thread_receiver_delete,
                az_thread_receive_msg_delete,
                az_thread_write_back_msg_delete,
                az_task_bar_icon_delete,
                az_task_bar_icon_deep_copy,
                az_small_window_icon_bytes_delete,
                az_small_window_icon_bytes_deep_copy,
                az_large_window_icon_bytes_delete,
                az_large_window_icon_bytes_deep_copy,
                az_window_icon_delete,
                az_window_icon_deep_copy,
                az_debug_state_delete,
                az_debug_state_deep_copy,
                az_keyboard_state_delete,
                az_keyboard_state_deep_copy,
                az_mouse_state_delete,
                az_mouse_state_deep_copy,
                az_platform_specific_options_delete,
                az_platform_specific_options_deep_copy,
                az_windows_window_options_delete,
                az_windows_window_options_deep_copy,
                az_wayland_theme_delete,
                az_wayland_theme_deep_copy,
                az_renderer_type_delete,
                az_renderer_type_deep_copy,
                az_string_pair_delete,
                az_string_pair_deep_copy,
                az_linux_window_options_delete,
                az_linux_window_options_deep_copy,
                az_window_state_new,
                az_window_state_delete,
                az_window_state_deep_copy,
                az_window_create_options_new,
                az_window_create_options_delete,
                az_window_create_options_deep_copy,
            })
        }

    }

    #[cfg(target_os="linux")]
    const LIB_BYTES: &[u8] = include_bytes!(concat!(env!("CARGO_HOME"), "/lib/", "azul-dll-", env!("CARGO_PKG_VERSION"), "/target/release/libazul.so")); /* !!! IF THIS LINE SHOWS AN ERROR, IT MEANS YOU FORGOT TO RUN "cargo install --version 0.1.0 azul-dll" */
    #[cfg(target_os="windows")]
    const LIB_BYTES: &[u8] = include_bytes!(concat!(env!("CARGO_HOME"), "/lib/", "azul-dll-", env!("CARGO_PKG_VERSION"), "/target/release/azul.dll")); /* !!! IF THIS LINE SHOWS AN ERROR, IT MEANS YOU FORGOT TO RUN "cargo install --version 0.1.0 azul-dll" */
    #[cfg(target_os="macos")]
    const LIB_BYTES: &[u8] = include_bytes!(concat!(env!("CARGO_HOME"), "/lib/", "azul-dll-", env!("CARGO_PKG_VERSION"), "/target/release/libazul.dylib")); /* !!! IF THIS LINE SHOWS AN ERROR, IT MEANS YOU FORGOT TO RUN "cargo install --version 0.1.0 azul-dll" */

    use std::{mem::MaybeUninit, sync::atomic::{AtomicBool, Ordering}};

    static LIBRARY_IS_INITIALIZED: AtomicBool = AtomicBool::new(false);
    static mut AZUL_DLL: MaybeUninit<AzulDll> = MaybeUninit::<AzulDll>::uninit();

    #[cfg(target_os="linux")]
    const DLL_FILE_NAME: &str = "azul.so";
    #[cfg(target_os="windows")]
    const DLL_FILE_NAME: &str = "azul.dll";
    #[cfg(target_os="macos")]
    const DLL_FILE_NAME: &str = "libazul.dynlib";

    fn load_library_inner() -> Result<AzulDll, &'static str> {

        let current_exe_path = std::env::current_exe().map_err(|_| "current exe has no current dir (?!)")?;
        let mut library_path = current_exe_path.parent().ok_or("current exe has no parent (?!)")?.to_path_buf();
        library_path.push(DLL_FILE_NAME);

        if !library_path.exists() {
           std::fs::write(&library_path, LIB_BYTES).map_err(|_| "could not unpack DLL")?;
        }

        initialize_library(&library_path).ok_or("could not initialize library")
    }

    pub(crate) fn get_azul_dll() -> &'static AzulDll { 
        if !LIBRARY_IS_INITIALIZED.load(Ordering::SeqCst) {
           match load_library_inner() {
               Ok(s) => {
                   unsafe { AZUL_DLL = MaybeUninit::new(s) };
                   LIBRARY_IS_INITIALIZED.store(true, Ordering::SeqCst);
               },
               Err(e) => { println!("failed to initialize libazul dll: missing function {}", e); std::process::exit(-1); }
           }
        }

        unsafe { &*AZUL_DLL.as_ptr() }
    }
