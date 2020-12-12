    use std::ffi::c_void;

    /// Return type of a regular callback - currently `AzUpdateScreen`
    pub type AzCallbackReturn = AzUpdateScreen;
    /// Callback for responding to window events
    pub type AzCallbackType = extern "C" fn(AzCallbackInfoPtr) -> AzCallbackReturn;
    /// Callback fn that returns the DOM of the app
    pub type AzLayoutCallbackType = extern "C" fn(AzRefAny, AzLayoutInfoPtr) -> AzDom;
    /// Callback for rendering to an OpenGL texture
    pub type AzGlCallbackType = extern "C" fn(AzGlCallbackInfoPtr) -> AzGlCallbackReturn;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzIFrameCallbackType = extern "C" fn(AzIFrameCallbackInfoPtr) -> AzIFrameCallbackReturn;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzRefAnyDestructorType = extern "C" fn(*const c_void);
    /// Callback for the `Timer` class
    pub type AzTimerCallbackType = extern "C" fn(AzTimerCallbackInfoPtr) -> AzTimerCallbackReturn;
    /// Callback for the `Thread` class
    pub type AzThreadCallbackType = extern "C" fn(AzRefAny) -> AzRefAny;
    /// Callback for the `Task` class
    pub type AzTaskCallbackType= extern "C" fn(AzArcMutexRefAnyPtr, AzDropCheckPtr) -> AzUpdateScreen;
    /// Re-export of rust-allocated (stack based) `String` struct
    #[repr(C)] pub struct AzString {
        pub vec: AzU8Vec,
    }

    impl PartialEq for AzString { fn eq(&self, rhs: &AzString) -> bool { (crate::dll::get_azul_dll().az_string_partial_eq)(self, rhs) } }

    impl Eq for AzString { }

    impl PartialOrd for AzString { fn partial_cmp(&self, rhs: &AzString) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_string_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzString { fn cmp(&self, rhs: &AzString) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_string_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzString { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_string_hash)(self)).hash(state) } }
    /// Wrapper over a Rust-allocated `Vec<CssProperty>`
    #[repr(C)] pub struct AzCssPropertyVec {
        pub(crate) ptr: *mut AzCssProperty,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<SvgMultiPolygon>`
    #[repr(C)] pub struct AzSvgMultiPolygonVec {
        pub(crate) ptr: *mut AzSvgMultiPolygon,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<SvgPath>`
    #[repr(C)] pub struct AzSvgPathVec {
        pub(crate) ptr: *mut AzSvgPath,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<VertexAttribute>`
    #[repr(C)] pub struct AzVertexAttributeVec {
        pub(crate) ptr: *mut AzVertexAttribute,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `VertexAttribute`
    #[repr(C)] pub struct AzSvgPathElementVec {
        pub(crate) ptr: *mut AzSvgPathElement,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `SvgVertex`
    #[repr(C)] pub struct AzSvgVertexVec {
        pub(crate) ptr: *mut AzSvgVertex,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<u32>`
    #[repr(C)] pub struct AzU32Vec {
        pub(crate) ptr: *mut u32,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `XWindowType`
    #[repr(C)] pub struct AzXWindowTypeVec {
        pub(crate) ptr: *mut AzXWindowType,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `VirtualKeyCode`
    #[repr(C)] pub struct AzVirtualKeyCodeVec {
        pub(crate) ptr: *mut AzVirtualKeyCode,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `ScanCode`
    #[repr(C)] pub struct AzScanCodeVec {
        pub(crate) ptr: *mut u32,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CssDeclaration`
    #[repr(C)] pub struct AzCssDeclarationVec {
        pub(crate) ptr: *mut AzCssDeclaration,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CssPathSelector`
    #[repr(C)] pub struct AzCssPathSelectorVec {
        pub(crate) ptr: *mut AzCssPathSelector,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Stylesheet`
    #[repr(C)] pub struct AzStylesheetVec {
        pub(crate) ptr: *mut AzStylesheet,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CssRuleBlock`
    #[repr(C)] pub struct AzCssRuleBlockVec {
        pub(crate) ptr: *mut AzCssRuleBlock,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `U8Vec`
    #[repr(C)] pub struct AzU8Vec {
        pub(crate) ptr: *mut u8,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CallbackData`
    #[repr(C)] pub struct AzCallbackDataVec {
        pub(crate) ptr: *mut AzCallbackData,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<DebugMessage>`
    #[repr(C)] pub struct AzDebugMessageVec {
        pub(crate) ptr: *mut AzDebugMessage,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `U32Vec`
    #[repr(C)] pub struct AzGLuintVec {
        pub(crate) ptr: *mut u32,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `GLintVec`
    #[repr(C)] pub struct AzGLintVec {
        pub(crate) ptr: *mut i32,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `DomVec`
    #[repr(C)] pub struct AzDomVec {
        pub(crate) ptr: *mut AzDom,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `StringVec`
    #[repr(C)] pub struct AzStringVec {
        pub(crate) ptr: *mut AzString,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `StringPairVec`
    #[repr(C)] pub struct AzStringPairVec {
        pub(crate) ptr: *mut AzStringPair,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `GradientStopPreVec`
    #[repr(C)] pub struct AzGradientStopPreVec {
        pub(crate) ptr: *mut AzGradientStopPre,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CascadedCssPropertyWithSourceVec`
    #[repr(C)] pub struct AzCascadedCssPropertyWithSourceVec {
        pub(crate) ptr: *mut AzCascadedCssPropertyWithSource,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `NodeIdVec`
    #[repr(C)] pub struct AzNodeIdVec {
        pub(crate) ptr: *mut AzNodeId,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `NodeVec`
    #[repr(C)] pub struct AzNodeVec {
        pub(crate) ptr: *mut AzNode,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `StyledNodeVec`
    #[repr(C)] pub struct AzStyledNodeVec {
        pub(crate) ptr: *mut AzStyledNode,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `TagIdsToNodeIdsMappingVec`
    #[repr(C)] pub struct AzTagIdsToNodeIdsMappingVec {
        pub(crate) ptr: *mut AzTagIdToNodeIdMapping,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `ParentWithNodeDepthVec`
    #[repr(C)] pub struct AzParentWithNodeDepthVec {
        pub(crate) ptr: *mut AzParentWithNodeDepth,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `NodeDataVec`
    #[repr(C)] pub struct AzNodeDataVec {
        pub(crate) ptr: *mut AzNodeData,
        pub len: usize,
        pub cap: usize,
    }
    /// Re-export of rust-allocated (stack based) `OptionNodeId` struct
    #[repr(C, u8)] pub enum AzOptionNodeId {
        None,
        Some(AzNodeId),
    }
    /// Re-export of rust-allocated (stack based) `OptionDomNodeId` struct
    #[repr(C, u8)] pub enum AzOptionDomNodeId {
        None,
        Some(AzDomNodeId),
    }
    /// Re-export of rust-allocated (stack based) `OptionColorU` struct
    #[repr(C, u8)] pub enum AzOptionColorU {
        None,
        Some(AzColorU),
    }
    /// Re-export of rust-allocated (stack based) `OptionRawImage` struct
    #[repr(C, u8)] pub enum AzOptionRawImage {
        None,
        Some(AzRawImage),
    }
    /// Re-export of rust-allocated (stack based) `OptionSvgDashPattern` struct
    #[repr(C, u8)] pub enum AzOptionSvgDashPattern {
        None,
        Some(AzSvgDashPattern),
    }
    /// Re-export of rust-allocated (stack based) `OptionWaylandTheme` struct
    #[repr(C, u8)] pub enum AzOptionWaylandTheme {
        None,
        Some(AzWaylandTheme),
    }
    /// Re-export of rust-allocated (stack based) `OptionTaskBarIcon` struct
    #[repr(C, u8)] pub enum AzOptionTaskBarIcon {
        None,
        Some(AzTaskBarIcon),
    }
    /// Re-export of rust-allocated (stack based) `OptionHwndHandle` struct
    #[repr(C, u8)] pub enum AzOptionHwndHandle {
        None,
        Some(*mut c_void),
    }
    /// Re-export of rust-allocated (stack based) `OptionLogicalPosition` struct
    #[repr(C, u8)] pub enum AzOptionLogicalPosition {
        None,
        Some(AzLogicalPosition),
    }
    /// Re-export of rust-allocated (stack based) `OptionHotReloadOptions` struct
    #[repr(C, u8)] pub enum AzOptionHotReloadOptions {
        None,
        Some(AzHotReloadOptions),
    }
    /// Re-export of rust-allocated (stack based) `OptionPhysicalPositionI32` struct
    #[repr(C, u8)] pub enum AzOptionPhysicalPositionI32 {
        None,
        Some(AzPhysicalPositionI32),
    }
    /// Re-export of rust-allocated (stack based) `OptionWindowIcon` struct
    #[repr(C, u8)] pub enum AzOptionWindowIcon {
        None,
        Some(AzWindowIcon),
    }
    /// Re-export of rust-allocated (stack based) `OptionString` struct
    #[repr(C, u8)] pub enum AzOptionString {
        None,
        Some(AzString),
    }
    /// Re-export of rust-allocated (stack based) `OptionX11Visual` struct
    #[repr(C, u8)] pub enum AzOptionX11Visual {
        None,
        Some(*const c_void),
    }
    /// Re-export of rust-allocated (stack based) `OptionI32` struct
    #[repr(C, u8)] pub enum AzOptionI32 {
        None,
        Some(i32),
    }
    /// Re-export of rust-allocated (stack based) `OptionF32` struct
    #[repr(C, u8)] pub enum AzOptionF32 {
        None,
        Some(f32),
    }
    /// Re-export of rust-allocated (stack based) `OptionMouseCursorType` struct
    #[repr(C, u8)] pub enum AzOptionMouseCursorType {
        None,
        Some(AzMouseCursorType),
    }
    /// Re-export of rust-allocated (stack based) `OptionLogicalSize` struct
    #[repr(C, u8)] pub enum AzOptionLogicalSize {
        None,
        Some(AzLogicalSize),
    }
    /// Option<char> but the char is a u32, for C FFI stability reasons
    #[repr(C, u8)] pub enum AzOptionChar {
        None,
        Some(u32),
    }
    /// Re-export of rust-allocated (stack based) `OptionVirtualKeyCode` struct
    #[repr(C, u8)] pub enum AzOptionVirtualKeyCode {
        None,
        Some(AzVirtualKeyCode),
    }
    /// Re-export of rust-allocated (stack based) `OptionPercentageValue` struct
    #[repr(C, u8)] pub enum AzOptionPercentageValue {
        None,
        Some(AzPercentageValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionDom` struct
    #[repr(C, u8)] pub enum AzOptionDom {
        None,
        Some(AzDom),
    }
    /// Re-export of rust-allocated (stack based) `OptionTexture` struct
    #[repr(C, u8)] pub enum AzOptionTexture {
        None,
        Some(AzTexture),
    }
    /// Re-export of rust-allocated (stack based) `OptionImageMask` struct
    #[repr(C, u8)] pub enum AzOptionImageMask {
        None,
        Some(AzImageMask),
    }
    /// Re-export of rust-allocated (stack based) `OptionTabIndex` struct
    #[repr(C, u8)] pub enum AzOptionTabIndex {
        None,
        Some(AzTabIndex),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBackgroundContentValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBackgroundContentValue {
        None,
        Some(AzStyleBackgroundContent),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBackgroundPositionValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBackgroundPositionValue {
        None,
        Some(AzStyleBackgroundPosition),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBackgroundSizeValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBackgroundSizeValue {
        None,
        Some(AzStyleBackgroundSize),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBackgroundRepeatValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBackgroundRepeatValue {
        None,
        Some(AzStyleBackgroundRepeat),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleFontSizeValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleFontSizeValue {
        None,
        Some(AzStyleFontSize),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleFontFamilyValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleFontFamilyValue {
        None,
        Some(AzStyleFontFamily),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleTextColorValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleTextColorValue {
        None,
        Some(AzStyleTextColor),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleTextAlignmentHorzValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleTextAlignmentHorzValue {
        None,
        Some(AzStyleTextAlignmentHorz),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleLineHeightValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleLineHeightValue {
        None,
        Some(AzStyleLineHeight),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleLetterSpacingValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleLetterSpacingValue {
        None,
        Some(AzStyleLetterSpacing),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleWordSpacingValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleWordSpacingValue {
        None,
        Some(AzStyleWordSpacing),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleTabWidthValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleTabWidthValue {
        None,
        Some(AzStyleTabWidth),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleCursorValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleCursorValue {
        None,
        Some(AzStyleCursor),
    }
    /// Re-export of rust-allocated (stack based) `OptionBoxShadowPreDisplayItemValue` struct
    #[repr(C, u8)] pub enum AzOptionBoxShadowPreDisplayItemValue {
        None,
        Some(AzBoxShadowPreDisplayItem),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderTopColorValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderTopColorValue {
        None,
        Some(AzStyleBorderTopColor),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderLeftColorValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderLeftColorValue {
        None,
        Some(AzStyleBorderLeftColor),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderRightColorValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderRightColorValue {
        None,
        Some(AzStyleBorderRightColor),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderBottomColorValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderBottomColorValue {
        None,
        Some(AzStyleBorderBottomColor),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderTopStyleValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderTopStyleValue {
        None,
        Some(AzStyleBorderTopStyle),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderLeftStyleValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderLeftStyleValue {
        None,
        Some(AzStyleBorderLeftStyle),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderRightStyleValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderRightStyleValue {
        None,
        Some(AzStyleBorderRightStyle),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderBottomStyleValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderBottomStyleValue {
        None,
        Some(AzStyleBorderBottomStyle),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderTopLeftRadiusValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderTopLeftRadiusValue {
        None,
        Some(AzStyleBorderTopLeftRadius),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderTopRightRadiusValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderTopRightRadiusValue {
        None,
        Some(AzStyleBorderTopRightRadius),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderBottomLeftRadiusValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderBottomLeftRadiusValue {
        None,
        Some(AzStyleBorderBottomLeftRadius),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderBottomRightRadiusValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderBottomRightRadiusValue {
        None,
        Some(AzStyleBorderBottomRightRadius),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutDisplayValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutDisplayValue {
        None,
        Some(AzLayoutDisplay),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutFloatValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutFloatValue {
        None,
        Some(AzLayoutFloat),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutBoxSizingValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutBoxSizingValue {
        None,
        Some(AzLayoutBoxSizing),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutWidthValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutWidthValue {
        None,
        Some(AzLayoutWidth),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutHeightValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutHeightValue {
        None,
        Some(AzLayoutHeight),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMinWidthValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutMinWidthValue {
        None,
        Some(AzLayoutMinWidth),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMinHeightValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutMinHeightValue {
        None,
        Some(AzLayoutMinHeight),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMaxWidthValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutMaxWidthValue {
        None,
        Some(AzLayoutMaxWidth),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMaxHeightValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutMaxHeightValue {
        None,
        Some(AzLayoutMaxHeight),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutPositionValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutPositionValue {
        None,
        Some(AzLayoutPosition),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutTopValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutTopValue {
        None,
        Some(AzLayoutTop),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutBottomValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutBottomValue {
        None,
        Some(AzLayoutBottom),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutRightValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutRightValue {
        None,
        Some(AzLayoutRight),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutLeftValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutLeftValue {
        None,
        Some(AzLayoutLeft),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutPaddingTopValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutPaddingTopValue {
        None,
        Some(AzLayoutPaddingTop),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutPaddingBottomValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutPaddingBottomValue {
        None,
        Some(AzLayoutPaddingBottom),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutPaddingLeftValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutPaddingLeftValue {
        None,
        Some(AzLayoutPaddingLeft),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutPaddingRightValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutPaddingRightValue {
        None,
        Some(AzLayoutPaddingRight),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMarginTopValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutMarginTopValue {
        None,
        Some(AzLayoutMarginTop),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMarginBottomValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutMarginBottomValue {
        None,
        Some(AzLayoutMarginBottom),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMarginLeftValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutMarginLeftValue {
        None,
        Some(AzLayoutMarginLeft),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutMarginRightValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutMarginRightValue {
        None,
        Some(AzLayoutMarginRight),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderTopWidthValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderTopWidthValue {
        None,
        Some(AzStyleBorderTopWidth),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderLeftWidthValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderLeftWidthValue {
        None,
        Some(AzStyleBorderLeftWidth),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderRightWidthValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderRightWidthValue {
        None,
        Some(AzStyleBorderRightWidth),
    }
    /// Re-export of rust-allocated (stack based) `OptionStyleBorderBottomWidthValue` struct
    #[repr(C, u8)] pub enum AzOptionStyleBorderBottomWidthValue {
        None,
        Some(AzStyleBorderBottomWidth),
    }
    /// Re-export of rust-allocated (stack based) `OptionOverflowValue` struct
    #[repr(C, u8)] pub enum AzOptionOverflowValue {
        None,
        Some(AzOverflow),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutDirectionValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutDirectionValue {
        None,
        Some(AzLayoutDirection),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutWrapValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutWrapValue {
        None,
        Some(AzLayoutWrap),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutFlexGrowValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutFlexGrowValue {
        None,
        Some(AzLayoutFlexGrow),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutFlexShrinkValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutFlexShrinkValue {
        None,
        Some(AzLayoutFlexShrink),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutJustifyContentValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutJustifyContentValue {
        None,
        Some(AzLayoutJustifyContent),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutAlignItemsValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutAlignItemsValue {
        None,
        Some(AzLayoutAlignItems),
    }
    /// Re-export of rust-allocated (stack based) `OptionLayoutAlignContentValue` struct
    #[repr(C, u8)] pub enum AzOptionLayoutAlignContentValue {
        None,
        Some(AzLayoutAlignContent),
    }
    /// Re-export of rust-allocated (stack based) `OptionHoverGroup` struct
    #[repr(C, u8)] pub enum AzOptionHoverGroup {
        None,
        Some(AzHoverGroup),
    }
    /// Re-export of rust-allocated (stack based) `OptionTagId` struct
    #[repr(C, u8)] pub enum AzOptionTagId {
        None,
        Some(AzTagId),
    }
    /// Re-export of rust-allocated (stack based) `OptionDuration` struct
    #[repr(C, u8)] pub enum AzOptionDuration {
        None,
        Some(AzDuration),
    }
    /// Re-export of rust-allocated (stack based) `OptionInstantPtr` struct
    #[repr(C, u8)] pub enum AzOptionInstantPtr {
        None,
        Some(AzInstantPtr),
    }
    /// Re-export of rust-allocated (stack based) `OptionUsize` struct
    #[repr(C, u8)] pub enum AzOptionUsize {
        None,
        Some(usize),
    }
    /// Re-export of rust-allocated (stack based) `OptionU8VecRef` struct
    #[repr(C, u8)] pub enum AzOptionU8VecRef {
        None,
        Some(AzU8VecRef),
    }
    /// Re-export of rust-allocated (stack based) `ResultSvgSvgParseError` struct
    #[repr(C, u8)] pub enum AzResultSvgSvgParseError {
        Ok(AzSvg),
        Err(AzSvgParseError),
    }
    /// Re-export of rust-allocated (stack based) `ResultRefAnyBlockError` struct
    #[repr(C, u8)] pub enum AzResultRefAnyBlockError {
        Ok(AzRefAny),
        Err(AzBlockError),
    }
    /// Re-export of rust-allocated (stack based) `SvgParseError` struct
    #[repr(C, u8)] pub enum AzSvgParseError {
        InvalidFileSuffix,
        FileOpenFailed,
        NotAnUtf8Str,
        MalformedGZip,
        InvalidSize,
        ParsingFailed(AzXmlError),
    }
    /// Re-export of rust-allocated (stack based) `XmlError` struct
    #[repr(C, u8)] pub enum AzXmlError {
        InvalidXmlPrefixUri(AzXmlTextPos),
        UnexpectedXmlUri(AzXmlTextPos),
        UnexpectedXmlnsUri(AzXmlTextPos),
        InvalidElementNamePrefix(AzXmlTextPos),
        DuplicatedNamespace(AzDuplicatedNamespaceError),
        UnknownNamespace(AzUnknownNamespaceError),
        UnexpectedCloseTag(AzUnexpectedCloseTagError),
        UnexpectedEntityCloseTag(AzXmlTextPos),
        UnknownEntityReference(AzUnknownEntityReferenceError),
        MalformedEntityReference(AzXmlTextPos),
        EntityReferenceLoop(AzXmlTextPos),
        InvalidAttributeValue(AzXmlTextPos),
        DuplicatedAttribute(AzDuplicatedAttributeError),
        NoRootNode,
        SizeLimit,
        ParserError(AzXmlParseError),
    }
    /// Re-export of rust-allocated (stack based) `DuplicatedNamespaceError` struct
    #[repr(C)] pub struct AzDuplicatedNamespaceError {
        pub ns: AzString,
        pub pos: AzXmlTextPos,
    }
    /// Re-export of rust-allocated (stack based) `UnknownNamespaceError` struct
    #[repr(C)] pub struct AzUnknownNamespaceError {
        pub ns: AzString,
        pub pos: AzXmlTextPos,
    }
    /// Re-export of rust-allocated (stack based) `UnexpectedCloseTagError` struct
    #[repr(C)] pub struct AzUnexpectedCloseTagError {
        pub expected: AzString,
        pub actual: AzString,
        pub pos: AzXmlTextPos,
    }
    /// Re-export of rust-allocated (stack based) `UnknownEntityReferenceError` struct
    #[repr(C)] pub struct AzUnknownEntityReferenceError {
        pub entity: AzString,
        pub pos: AzXmlTextPos,
    }
    /// Re-export of rust-allocated (stack based) `DuplicatedAttributeError` struct
    #[repr(C)] pub struct AzDuplicatedAttributeError {
        pub attribute: AzString,
        pub pos: AzXmlTextPos,
    }
    /// Re-export of rust-allocated (stack based) `XmlParseError` struct
    #[repr(C, u8)] pub enum AzXmlParseError {
        InvalidDeclaration(AzXmlTextError),
        InvalidComment(AzXmlTextError),
        InvalidPI(AzXmlTextError),
        InvalidDoctype(AzXmlTextError),
        InvalidEntity(AzXmlTextError),
        InvalidElement(AzXmlTextError),
        InvalidAttribute(AzXmlTextError),
        InvalidCdata(AzXmlTextError),
        InvalidCharData(AzXmlTextError),
        UnknownToken(AzXmlTextPos),
    }
    /// Re-export of rust-allocated (stack based) `XmlTextError` struct
    #[repr(C)] pub struct AzXmlTextError {
        pub stream_error: AzXmlStreamError,
        pub pos: AzXmlTextPos,
    }
    /// Re-export of rust-allocated (stack based) `XmlStreamError` struct
    #[repr(C, u8)] pub enum AzXmlStreamError {
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
    #[repr(C)] pub struct AzNonXmlCharError {
        pub ch: char,
        pub pos: AzXmlTextPos,
    }
    /// Re-export of rust-allocated (stack based) `InvalidCharError` struct
    #[repr(C)] pub struct AzInvalidCharError {
        pub expected: u8,
        pub got: u8,
        pub pos: AzXmlTextPos,
    }
    /// Re-export of rust-allocated (stack based) `InvalidCharMultipleError` struct
    #[repr(C)] pub struct AzInvalidCharMultipleError {
        pub expected: u8,
        pub got: AzU8Vec,
        pub pos: AzXmlTextPos,
    }
    /// Re-export of rust-allocated (stack based) `InvalidQuoteError` struct
    #[repr(C)] pub struct AzInvalidQuoteError {
        pub got: u8,
        pub pos: AzXmlTextPos,
    }
    /// Re-export of rust-allocated (stack based) `InvalidSpaceError` struct
    #[repr(C)] pub struct AzInvalidSpaceError {
        pub got: u8,
        pub pos: AzXmlTextPos,
    }
    /// Re-export of rust-allocated (stack based) `InvalidStringError` struct
    #[repr(C)] pub struct AzInvalidStringError {
        pub got: AzString,
        pub pos: AzXmlTextPos,
    }
    /// Re-export of rust-allocated (stack based) `XmlTextPos` struct
    #[repr(C)] pub struct AzXmlTextPos {
        pub row: u32,
        pub col: u32,
    }
    /// Pointer to rust-allocated `Box<Instant>` struct
    #[repr(C)] pub struct AzInstantPtr {
        pub(crate) ptr: *mut c_void,
    }

    impl PartialEq for AzInstantPtr { fn eq(&self, rhs: &AzInstantPtr) -> bool { (crate::dll::get_azul_dll().az_instant_ptr_partial_eq)(self, rhs) } }

    impl Eq for AzInstantPtr { }

    impl PartialOrd for AzInstantPtr { fn partial_cmp(&self, rhs: &AzInstantPtr) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_instant_ptr_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzInstantPtr { fn cmp(&self, rhs: &AzInstantPtr) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_instant_ptr_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }
    /// Re-export of rust-allocated (stack based) `Duration` struct
    #[repr(C)] pub struct AzDuration {
        pub secs: u64,
        pub nanos: u32,
    }
    /// Pointer to rust-allocated `Box<AppConfig>` struct
    #[repr(C)] pub struct AzAppConfigPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Pointer to rust-allocated `Box<App>` struct
    #[repr(C)] pub struct AzAppPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `NodeId` struct
    #[repr(C)] pub struct AzNodeId {
        pub inner: u32,
    }
    /// Re-export of rust-allocated (stack based) `DomId` struct
    #[repr(C)] pub struct AzDomId {
        pub inner: u32,
    }
    /// Re-export of rust-allocated (stack based) `DomNodeId` struct
    #[repr(C)] pub struct AzDomNodeId {
        pub dom: AzDomId,
        pub node: AzNodeId,
    }
    /// Re-export of rust-allocated (stack based) `HidpiAdjustedBounds` struct
    #[repr(C)] pub struct AzHidpiAdjustedBounds {
        pub logical_size: AzLogicalSize,
        pub hidpi_factor: f32,
    }
    /// Re-export of rust-allocated (stack based) `LayoutCallback` struct
    #[repr(C)] pub struct AzLayoutCallback {
        pub cb: AzLayoutCallbackType,
    }
    /// Re-export of rust-allocated (stack based) `Callback` struct
    #[repr(C)] pub struct AzCallback {
        pub cb: AzCallbackType,
    }

    impl PartialEq for AzCallback { fn eq(&self, rhs: &AzCallback) -> bool { (crate::dll::get_azul_dll().az_callback_partial_eq)(self, rhs) } }

    impl Eq for AzCallback { }

    impl PartialOrd for AzCallback { fn partial_cmp(&self, rhs: &AzCallback) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_callback_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzCallback { fn cmp(&self, rhs: &AzCallback) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_callback_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzCallback { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_callback_hash)(self)).hash(state) } }
    /// Pointer to rust-allocated `Box<CallbackInfo>` struct
    #[repr(C)] pub struct AzCallbackInfoPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Specifies if the screen should be updated after the callback function has returned
    #[repr(C)] pub enum AzUpdateScreen {
        Redraw,
        DontRedraw,
        UpdateScrollStates,
        UpdateTransforms,
    }
    /// Re-export of rust-allocated (stack based) `IFrameCallback` struct
    #[repr(C)] pub struct AzIFrameCallback {
        pub cb: AzIFrameCallbackType,
    }
    /// Pointer to rust-allocated `Box<IFrameCallbackInfo>` struct
    #[repr(C)] pub struct AzIFrameCallbackInfoPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `IFrameCallbackReturn` struct
    #[repr(C)] pub struct AzIFrameCallbackReturn {
        pub dom: AzOptionDom,
    }
    /// Re-export of rust-allocated (stack based) `GlCallback` struct
    #[repr(C)] pub struct AzGlCallback {
        pub cb: AzGlCallbackType,
    }
    /// Pointer to rust-allocated `Box<GlCallbackInfo>` struct
    #[repr(C)] pub struct AzGlCallbackInfoPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `GlCallbackReturn` struct
    #[repr(C)] pub struct AzGlCallbackReturn {
        pub texture: AzOptionTexture,
    }
    /// Re-export of rust-allocated (stack based) `TimerCallback` struct
    #[repr(C)] pub struct AzTimerCallback {
        pub cb: AzTimerCallbackType,
    }
    /// Pointer to rust-allocated `Box<TimerCallbackType>` struct
    #[repr(C)] pub struct AzTimerCallbackTypePtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Pointer to rust-allocated `Box<TimerCallbackInfo>` struct
    #[repr(C)] pub struct AzTimerCallbackInfoPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `TimerCallbackReturn` struct
    #[repr(C)] pub struct AzTimerCallbackReturn {
        pub should_update: AzUpdateScreen,
        pub should_terminate: AzTerminateTimer,
    }
    /// Re-export of rust-allocated (stack based) `RefAnySharingInfo` struct
    #[repr(C)] pub struct AzRefAnySharingInfo {
        pub(crate) ptr: *const c_void,
    }
    /// RefAny is a reference-counted, type-erased pointer, which stores a reference to a struct. `RefAny` can be up- and downcasted (this usually done via generics and can't be expressed in the Rust API)
    #[repr(C)] pub struct AzRefAny {
        pub _internal_ptr: *const c_void,
        pub _internal_len: usize,
        pub _internal_layout_size: usize,
        pub _internal_layout_align: usize,
        pub type_id: u64,
        pub type_name: AzString,
        pub _sharing_info_ptr: *const AzRefAnySharingInfo,
        pub custom_destructor: AzRefAnyDestructorType,
    }

    impl PartialEq for AzRefAny { fn eq(&self, rhs: &AzRefAny) -> bool { (crate::dll::get_azul_dll().az_ref_any_partial_eq)(self, rhs) } }

    impl Eq for AzRefAny { }

    impl PartialOrd for AzRefAny { fn partial_cmp(&self, rhs: &AzRefAny) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_ref_any_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzRefAny { fn cmp(&self, rhs: &AzRefAny) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_ref_any_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzRefAny { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_ref_any_hash)(self)).hash(state) } }
    /// Pointer to rust-allocated `Box<LayoutInfo>` struct
    #[repr(C)] pub struct AzLayoutInfoPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `CssRuleBlock` struct
    #[repr(C)] pub struct AzCssRuleBlock {
        pub path: AzCssPath,
        pub declarations: AzCssDeclarationVec,
    }
    /// Re-export of rust-allocated (stack based) `CssDeclaration` struct
    #[repr(C, u8)] pub enum AzCssDeclaration {
        Static(AzCssProperty),
        Dynamic(AzDynamicCssProperty),
    }
    /// Re-export of rust-allocated (stack based) `DynamicCssProperty` struct
    #[repr(C)] pub struct AzDynamicCssProperty {
        pub dynamic_id: AzString,
        pub default_value: AzCssProperty,
    }
    /// Re-export of rust-allocated (stack based) `CssPath` struct
    #[repr(C)] pub struct AzCssPath {
        pub selectors: AzCssPathSelectorVec,
    }
    /// Re-export of rust-allocated (stack based) `CssPathSelector` struct
    #[repr(C, u8)] pub enum AzCssPathSelector {
        Global,
        Type(AzNodeTypePath),
        Class(AzString),
        Id(AzString),
        PseudoSelector(AzCssPathPseudoSelector),
        DirectChildren,
        Children,
    }
    /// Re-export of rust-allocated (stack based) `NodeTypePath` struct
    #[repr(C)] pub enum AzNodeTypePath {
        Body,
        Div,
        P,
        Img,
        Texture,
        IFrame,
    }
    /// Re-export of rust-allocated (stack based) `CssPathPseudoSelector` struct
    #[repr(C, u8)] pub enum AzCssPathPseudoSelector {
        First,
        Last,
        NthChild(AzCssNthChildSelector),
        Hover,
        Active,
        Focus,
    }
    /// Re-export of rust-allocated (stack based) `CssNthChildSelector` struct
    #[repr(C, u8)] pub enum AzCssNthChildSelector {
        Number(u32),
        Even,
        Odd,
        Pattern(AzCssNthChildPattern),
    }
    /// Re-export of rust-allocated (stack based) `CssNthChildPattern` struct
    #[repr(C)] pub struct AzCssNthChildPattern {
        pub repeat: u32,
        pub offset: u32,
    }
    /// Re-export of rust-allocated (stack based) `Stylesheet` struct
    #[repr(C)] pub struct AzStylesheet {
        pub rules: AzCssRuleBlockVec,
    }
    /// Re-export of rust-allocated (stack based) `Css` struct
    #[repr(C)] pub struct AzCss {
        pub stylesheets: AzStylesheetVec,
    }
    /// Re-export of rust-allocated (stack based) `ColorU` struct
    #[repr(C)] pub struct AzColorU {
        pub r: u8,
        pub g: u8,
        pub b: u8,
        pub a: u8,
    }
    /// Re-export of rust-allocated (stack based) `SizeMetric` struct
    #[repr(C)] pub enum AzSizeMetric {
        Px,
        Pt,
        Em,
        Percent,
    }
    /// Re-export of rust-allocated (stack based) `FloatValue` struct
    #[repr(C)] pub struct AzFloatValue {
        pub number: isize,
    }
    /// Re-export of rust-allocated (stack based) `PixelValue` struct
    #[repr(C)] pub struct AzPixelValue {
        pub metric: AzSizeMetric,
        pub number: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `PixelValueNoPercent` struct
    #[repr(C)] pub struct AzPixelValueNoPercent {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `BoxShadowClipMode` struct
    #[repr(C)] pub enum AzBoxShadowClipMode {
        Outset,
        Inset,
    }
    /// Re-export of rust-allocated (stack based) `BoxShadowPreDisplayItem` struct
    #[repr(C)] pub struct AzBoxShadowPreDisplayItem {
        pub offset: [AzPixelValueNoPercent;2],
        pub color: AzColorU,
        pub blur_radius: AzPixelValueNoPercent,
        pub spread_radius: AzPixelValueNoPercent,
        pub clip_mode: AzBoxShadowClipMode,
    }
    /// Re-export of rust-allocated (stack based) `LayoutAlignContent` struct
    #[repr(C)] pub enum AzLayoutAlignContent {
        Stretch,
        Center,
        Start,
        End,
        SpaceBetween,
        SpaceAround,
    }
    /// Re-export of rust-allocated (stack based) `LayoutAlignItems` struct
    #[repr(C)] pub enum AzLayoutAlignItems {
        Stretch,
        Center,
        Start,
        End,
    }
    /// Re-export of rust-allocated (stack based) `LayoutBottom` struct
    #[repr(C)] pub struct AzLayoutBottom {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutBoxSizing` struct
    #[repr(C)] pub enum AzLayoutBoxSizing {
        ContentBox,
        BorderBox,
    }
    /// Re-export of rust-allocated (stack based) `LayoutDirection` struct
    #[repr(C)] pub enum AzLayoutDirection {
        Row,
        RowReverse,
        Column,
        ColumnReverse,
    }
    /// Re-export of rust-allocated (stack based) `LayoutDisplay` struct
    #[repr(C)] pub enum AzLayoutDisplay {
        Flex,
        Block,
        InlineBlock,
    }
    /// Re-export of rust-allocated (stack based) `LayoutFlexGrow` struct
    #[repr(C)] pub struct AzLayoutFlexGrow {
        pub inner: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutFlexShrink` struct
    #[repr(C)] pub struct AzLayoutFlexShrink {
        pub inner: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutFloat` struct
    #[repr(C)] pub enum AzLayoutFloat {
        Left,
        Right,
    }
    /// Re-export of rust-allocated (stack based) `LayoutHeight` struct
    #[repr(C)] pub struct AzLayoutHeight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutJustifyContent` struct
    #[repr(C)] pub enum AzLayoutJustifyContent {
        Start,
        End,
        Center,
        SpaceBetween,
        SpaceAround,
        SpaceEvenly,
    }
    /// Re-export of rust-allocated (stack based) `LayoutLeft` struct
    #[repr(C)] pub struct AzLayoutLeft {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginBottom` struct
    #[repr(C)] pub struct AzLayoutMarginBottom {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginLeft` struct
    #[repr(C)] pub struct AzLayoutMarginLeft {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginRight` struct
    #[repr(C)] pub struct AzLayoutMarginRight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginTop` struct
    #[repr(C)] pub struct AzLayoutMarginTop {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMaxHeight` struct
    #[repr(C)] pub struct AzLayoutMaxHeight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMaxWidth` struct
    #[repr(C)] pub struct AzLayoutMaxWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMinHeight` struct
    #[repr(C)] pub struct AzLayoutMinHeight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMinWidth` struct
    #[repr(C)] pub struct AzLayoutMinWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingBottom` struct
    #[repr(C)] pub struct AzLayoutPaddingBottom {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingLeft` struct
    #[repr(C)] pub struct AzLayoutPaddingLeft {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingRight` struct
    #[repr(C)] pub struct AzLayoutPaddingRight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingTop` struct
    #[repr(C)] pub struct AzLayoutPaddingTop {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPosition` struct
    #[repr(C)] pub enum AzLayoutPosition {
        Static,
        Relative,
        Absolute,
        Fixed,
    }
    /// Re-export of rust-allocated (stack based) `LayoutRight` struct
    #[repr(C)] pub struct AzLayoutRight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutTop` struct
    #[repr(C)] pub struct AzLayoutTop {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutWidth` struct
    #[repr(C)] pub struct AzLayoutWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutWrap` struct
    #[repr(C)] pub enum AzLayoutWrap {
        Wrap,
        NoWrap,
    }
    /// Re-export of rust-allocated (stack based) `Overflow` struct
    #[repr(C)] pub enum AzOverflow {
        Scroll,
        Auto,
        Hidden,
        Visible,
    }
    /// Re-export of rust-allocated (stack based) `PercentageValue` struct
    #[repr(C)] pub struct AzPercentageValue {
        pub number: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `GradientStopPre` struct
    #[repr(C)] pub struct AzGradientStopPre {
        pub offset: AzOptionPercentageValue,
        pub color: AzColorU,
    }

    impl PartialEq for AzGradientStopPre { fn eq(&self, rhs: &AzGradientStopPre) -> bool { (crate::dll::get_azul_dll().az_gradient_stop_pre_partial_eq)(self, rhs) } }

    impl Eq for AzGradientStopPre { }

    impl PartialOrd for AzGradientStopPre { fn partial_cmp(&self, rhs: &AzGradientStopPre) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_gradient_stop_pre_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzGradientStopPre { fn cmp(&self, rhs: &AzGradientStopPre) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_gradient_stop_pre_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzGradientStopPre { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_gradient_stop_pre_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `DirectionCorner` struct
    #[repr(C)] pub enum AzDirectionCorner {
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
    #[repr(C)] pub struct AzDirectionCorners {
        pub from: AzDirectionCorner,
        pub to: AzDirectionCorner,
    }
    /// Re-export of rust-allocated (stack based) `Direction` struct
    #[repr(C, u8)] pub enum AzDirection {
        Angle(AzFloatValue),
        FromTo(AzDirectionCorners),
    }
    /// Re-export of rust-allocated (stack based) `ExtendMode` struct
    #[repr(C)] pub enum AzExtendMode {
        Clamp,
        Repeat,
    }
    /// Re-export of rust-allocated (stack based) `LinearGradient` struct
    #[repr(C)] pub struct AzLinearGradient {
        pub direction: AzDirection,
        pub extend_mode: AzExtendMode,
        pub stops: AzGradientStopPreVec,
    }
    /// Re-export of rust-allocated (stack based) `Shape` struct
    #[repr(C)] pub enum AzShape {
        Ellipse,
        Circle,
    }
    /// Re-export of rust-allocated (stack based) `RadialGradient` struct
    #[repr(C)] pub struct AzRadialGradient {
        pub shape: AzShape,
        pub extend_mode: AzExtendMode,
        pub stops: AzGradientStopPreVec,
    }
    /// Re-export of rust-allocated (stack based) `CssImageId` struct
    #[repr(C)] pub struct AzCssImageId {
        pub inner: AzString,
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundContent` struct
    #[repr(C, u8)] pub enum AzStyleBackgroundContent {
        LinearGradient(AzLinearGradient),
        RadialGradient(AzRadialGradient),
        Image(AzCssImageId),
        Color(AzColorU),
    }
    /// Re-export of rust-allocated (stack based) `BackgroundPositionHorizontal` struct
    #[repr(C, u8)] pub enum AzBackgroundPositionHorizontal {
        Left,
        Center,
        Right,
        Exact(AzPixelValue),
    }
    /// Re-export of rust-allocated (stack based) `BackgroundPositionVertical` struct
    #[repr(C, u8)] pub enum AzBackgroundPositionVertical {
        Top,
        Center,
        Bottom,
        Exact(AzPixelValue),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundPosition` struct
    #[repr(C)] pub struct AzStyleBackgroundPosition {
        pub horizontal: AzBackgroundPositionHorizontal,
        pub vertical: AzBackgroundPositionVertical,
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundRepeat` struct
    #[repr(C)] pub enum AzStyleBackgroundRepeat {
        NoRepeat,
        Repeat,
        RepeatX,
        RepeatY,
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundSize` struct
    #[repr(C, u8)] pub enum AzStyleBackgroundSize {
        ExactSize([AzPixelValue;2]),
        Contain,
        Cover,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomColor` struct
    #[repr(C)] pub struct AzStyleBorderBottomColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomLeftRadius` struct
    #[repr(C)] pub struct AzStyleBorderBottomLeftRadius {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomRightRadius` struct
    #[repr(C)] pub struct AzStyleBorderBottomRightRadius {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `BorderStyle` struct
    #[repr(C)] pub enum AzBorderStyle {
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
    #[repr(C)] pub struct AzStyleBorderBottomStyle {
        pub inner: AzBorderStyle,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomWidth` struct
    #[repr(C)] pub struct AzStyleBorderBottomWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftColor` struct
    #[repr(C)] pub struct AzStyleBorderLeftColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftStyle` struct
    #[repr(C)] pub struct AzStyleBorderLeftStyle {
        pub inner: AzBorderStyle,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftWidth` struct
    #[repr(C)] pub struct AzStyleBorderLeftWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightColor` struct
    #[repr(C)] pub struct AzStyleBorderRightColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightStyle` struct
    #[repr(C)] pub struct AzStyleBorderRightStyle {
        pub inner: AzBorderStyle,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightWidth` struct
    #[repr(C)] pub struct AzStyleBorderRightWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopColor` struct
    #[repr(C)] pub struct AzStyleBorderTopColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopLeftRadius` struct
    #[repr(C)] pub struct AzStyleBorderTopLeftRadius {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopRightRadius` struct
    #[repr(C)] pub struct AzStyleBorderTopRightRadius {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopStyle` struct
    #[repr(C)] pub struct AzStyleBorderTopStyle {
        pub inner: AzBorderStyle,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopWidth` struct
    #[repr(C)] pub struct AzStyleBorderTopWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleCursor` struct
    #[repr(C)] pub enum AzStyleCursor {
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
    #[repr(C)] pub struct AzStyleFontFamily {
        pub fonts: AzStringVec,
    }
    /// Re-export of rust-allocated (stack based) `StyleFontSize` struct
    #[repr(C)] pub struct AzStyleFontSize {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleLetterSpacing` struct
    #[repr(C)] pub struct AzStyleLetterSpacing {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleLineHeight` struct
    #[repr(C)] pub struct AzStyleLineHeight {
        pub inner: AzPercentageValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleTabWidth` struct
    #[repr(C)] pub struct AzStyleTabWidth {
        pub inner: AzPercentageValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleTextAlignmentHorz` struct
    #[repr(C)] pub enum AzStyleTextAlignmentHorz {
        Left,
        Center,
        Right,
    }
    /// Re-export of rust-allocated (stack based) `StyleTextColor` struct
    #[repr(C)] pub struct AzStyleTextColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleWordSpacing` struct
    #[repr(C)] pub struct AzStyleWordSpacing {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `BoxShadowPreDisplayItemValue` struct
    #[repr(C, u8)] pub enum AzBoxShadowPreDisplayItemValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzBoxShadowPreDisplayItem),
    }
    /// Re-export of rust-allocated (stack based) `LayoutAlignContentValue` struct
    #[repr(C, u8)] pub enum AzLayoutAlignContentValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutAlignContent),
    }
    /// Re-export of rust-allocated (stack based) `LayoutAlignItemsValue` struct
    #[repr(C, u8)] pub enum AzLayoutAlignItemsValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutAlignItems),
    }
    /// Re-export of rust-allocated (stack based) `LayoutBottomValue` struct
    #[repr(C, u8)] pub enum AzLayoutBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBottom),
    }
    /// Re-export of rust-allocated (stack based) `LayoutBoxSizingValue` struct
    #[repr(C, u8)] pub enum AzLayoutBoxSizingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBoxSizing),
    }
    /// Re-export of rust-allocated (stack based) `LayoutDirectionValue` struct
    #[repr(C, u8)] pub enum AzLayoutDirectionValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutDirection),
    }
    /// Re-export of rust-allocated (stack based) `LayoutDisplayValue` struct
    #[repr(C, u8)] pub enum AzLayoutDisplayValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutDisplay),
    }
    /// Re-export of rust-allocated (stack based) `LayoutFlexGrowValue` struct
    #[repr(C, u8)] pub enum AzLayoutFlexGrowValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFlexGrow),
    }
    /// Re-export of rust-allocated (stack based) `LayoutFlexShrinkValue` struct
    #[repr(C, u8)] pub enum AzLayoutFlexShrinkValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFlexShrink),
    }
    /// Re-export of rust-allocated (stack based) `LayoutFloatValue` struct
    #[repr(C, u8)] pub enum AzLayoutFloatValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFloat),
    }
    /// Re-export of rust-allocated (stack based) `LayoutHeightValue` struct
    #[repr(C, u8)] pub enum AzLayoutHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutHeight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutJustifyContentValue` struct
    #[repr(C, u8)] pub enum AzLayoutJustifyContentValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutJustifyContent),
    }
    /// Re-export of rust-allocated (stack based) `LayoutLeftValue` struct
    #[repr(C, u8)] pub enum AzLayoutLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutLeft),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginBottomValue` struct
    #[repr(C, u8)] pub enum AzLayoutMarginBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginBottom),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginLeftValue` struct
    #[repr(C, u8)] pub enum AzLayoutMarginLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginLeft),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginRightValue` struct
    #[repr(C, u8)] pub enum AzLayoutMarginRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginRight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginTopValue` struct
    #[repr(C, u8)] pub enum AzLayoutMarginTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginTop),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMaxHeightValue` struct
    #[repr(C, u8)] pub enum AzLayoutMaxHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMaxHeight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMaxWidthValue` struct
    #[repr(C, u8)] pub enum AzLayoutMaxWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMaxWidth),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMinHeightValue` struct
    #[repr(C, u8)] pub enum AzLayoutMinHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMinHeight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMinWidthValue` struct
    #[repr(C, u8)] pub enum AzLayoutMinWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMinWidth),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingBottomValue` struct
    #[repr(C, u8)] pub enum AzLayoutPaddingBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingBottom),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingLeftValue` struct
    #[repr(C, u8)] pub enum AzLayoutPaddingLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingLeft),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingRightValue` struct
    #[repr(C, u8)] pub enum AzLayoutPaddingRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingRight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingTopValue` struct
    #[repr(C, u8)] pub enum AzLayoutPaddingTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingTop),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPositionValue` struct
    #[repr(C, u8)] pub enum AzLayoutPositionValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPosition),
    }
    /// Re-export of rust-allocated (stack based) `LayoutRightValue` struct
    #[repr(C, u8)] pub enum AzLayoutRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutRight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutTopValue` struct
    #[repr(C, u8)] pub enum AzLayoutTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutTop),
    }
    /// Re-export of rust-allocated (stack based) `LayoutWidthValue` struct
    #[repr(C, u8)] pub enum AzLayoutWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutWidth),
    }
    /// Re-export of rust-allocated (stack based) `LayoutWrapValue` struct
    #[repr(C, u8)] pub enum AzLayoutWrapValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutWrap),
    }
    /// Re-export of rust-allocated (stack based) `OverflowValue` struct
    #[repr(C, u8)] pub enum AzOverflowValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzOverflow),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundContentValue` struct
    #[repr(C, u8)] pub enum AzStyleBackgroundContentValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundContent),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundPositionValue` struct
    #[repr(C, u8)] pub enum AzStyleBackgroundPositionValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundPosition),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundRepeatValue` struct
    #[repr(C, u8)] pub enum AzStyleBackgroundRepeatValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundRepeat),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundSizeValue` struct
    #[repr(C, u8)] pub enum AzStyleBackgroundSizeValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundSize),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomColorValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderBottomColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomLeftRadiusValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderBottomLeftRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomLeftRadius),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomRightRadiusValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderBottomRightRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomRightRadius),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomStyleValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderBottomStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomStyle),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomWidthValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderBottomWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftColorValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderLeftColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderLeftColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftStyleValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderLeftStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderLeftStyle),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftWidthValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderLeftWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderLeftWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightColorValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderRightColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderRightColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightStyleValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderRightStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderRightStyle),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightWidthValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderRightWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderRightWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopColorValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderTopColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopLeftRadiusValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderTopLeftRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopLeftRadius),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopRightRadiusValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderTopRightRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopRightRadius),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopStyleValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderTopStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopStyle),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopWidthValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderTopWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleCursorValue` struct
    #[repr(C, u8)] pub enum AzStyleCursorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleCursor),
    }
    /// Re-export of rust-allocated (stack based) `StyleFontFamilyValue` struct
    #[repr(C, u8)] pub enum AzStyleFontFamilyValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleFontFamily),
    }
    /// Re-export of rust-allocated (stack based) `StyleFontSizeValue` struct
    #[repr(C, u8)] pub enum AzStyleFontSizeValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleFontSize),
    }
    /// Re-export of rust-allocated (stack based) `StyleLetterSpacingValue` struct
    #[repr(C, u8)] pub enum AzStyleLetterSpacingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleLetterSpacing),
    }
    /// Re-export of rust-allocated (stack based) `StyleLineHeightValue` struct
    #[repr(C, u8)] pub enum AzStyleLineHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleLineHeight),
    }
    /// Re-export of rust-allocated (stack based) `StyleTabWidthValue` struct
    #[repr(C, u8)] pub enum AzStyleTabWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTabWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleTextAlignmentHorzValue` struct
    #[repr(C, u8)] pub enum AzStyleTextAlignmentHorzValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTextAlignmentHorz),
    }
    /// Re-export of rust-allocated (stack based) `StyleTextColorValue` struct
    #[repr(C, u8)] pub enum AzStyleTextColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTextColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleWordSpacingValue` struct
    #[repr(C, u8)] pub enum AzStyleWordSpacingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleWordSpacing),
    }
    /// Parsed CSS key-value pair
    #[repr(C, u8)] pub enum AzCssProperty {
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
        FlexDirection(AzLayoutDirectionValue),
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
    }

    impl PartialEq for AzCssProperty { fn eq(&self, rhs: &AzCssProperty) -> bool { (crate::dll::get_azul_dll().az_css_property_partial_eq)(self, rhs) } }

    impl Eq for AzCssProperty { }

    impl PartialOrd for AzCssProperty { fn partial_cmp(&self, rhs: &AzCssProperty) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_css_property_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzCssProperty { fn cmp(&self, rhs: &AzCssProperty) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_css_property_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzCssProperty { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_css_property_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `Node` struct
    #[repr(C)] pub struct AzNode {
        pub parent: u32,
        pub previous_sibling: u32,
        pub next_sibling: u32,
        pub first_child: u32,
        pub last_child: u32,
    }

    impl Copy for AzNode { }

    impl PartialEq for AzNode { fn eq(&self, rhs: &AzNode) -> bool { (crate::dll::get_azul_dll().az_node_partial_eq)(self, rhs) } }

    impl Eq for AzNode { }

    impl PartialOrd for AzNode { fn partial_cmp(&self, rhs: &AzNode) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_node_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzNode { fn cmp(&self, rhs: &AzNode) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_node_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzNode { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_node_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `CascadeInfo` struct
    #[repr(C)] pub struct AzCascadeInfo {
        pub index_in_parent: u32,
        pub is_last_child: bool,
        pub is_hovered_over: bool,
        pub is_focused: bool,
        pub is_active: bool,
    }

    impl Copy for AzCascadeInfo { }

    impl PartialEq for AzCascadeInfo { fn eq(&self, rhs: &AzCascadeInfo) -> bool { (crate::dll::get_azul_dll().az_cascade_info_partial_eq)(self, rhs) } }

    impl Eq for AzCascadeInfo { }

    impl PartialOrd for AzCascadeInfo { fn partial_cmp(&self, rhs: &AzCascadeInfo) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_cascade_info_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzCascadeInfo { fn cmp(&self, rhs: &AzCascadeInfo) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_cascade_info_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzCascadeInfo { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_cascade_info_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `RectStyle` struct
    #[repr(C)] pub struct AzRectStyle {
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
    }

    impl PartialEq for AzRectStyle { fn eq(&self, rhs: &AzRectStyle) -> bool { (crate::dll::get_azul_dll().az_rect_style_partial_eq)(self, rhs) } }

    impl Eq for AzRectStyle { }

    impl PartialOrd for AzRectStyle { fn partial_cmp(&self, rhs: &AzRectStyle) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_rect_style_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzRectStyle { fn cmp(&self, rhs: &AzRectStyle) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_rect_style_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzRectStyle { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_rect_style_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `RectLayout` struct
    #[repr(C)] pub struct AzRectLayout {
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
        pub direction: AzOptionLayoutDirectionValue,
        pub wrap: AzOptionLayoutWrapValue,
        pub flex_grow: AzOptionLayoutFlexGrowValue,
        pub flex_shrink: AzOptionLayoutFlexShrinkValue,
        pub justify_content: AzOptionLayoutJustifyContentValue,
        pub align_items: AzOptionLayoutAlignItemsValue,
        pub align_content: AzOptionLayoutAlignContentValue,
    }

    impl PartialEq for AzRectLayout { fn eq(&self, rhs: &AzRectLayout) -> bool { (crate::dll::get_azul_dll().az_rect_layout_partial_eq)(self, rhs) } }

    impl Eq for AzRectLayout { }

    impl PartialOrd for AzRectLayout { fn partial_cmp(&self, rhs: &AzRectLayout) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_rect_layout_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzRectLayout { fn cmp(&self, rhs: &AzRectLayout) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_rect_layout_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzRectLayout { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_rect_layout_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `CascadedCssPropertyWithSource` struct
    #[repr(C)] pub struct AzCascadedCssPropertyWithSource {
        pub prop: AzCssProperty,
        pub source: AzCssPropertySource,
    }

    impl PartialEq for AzCascadedCssPropertyWithSource { fn eq(&self, rhs: &AzCascadedCssPropertyWithSource) -> bool { (crate::dll::get_azul_dll().az_cascaded_css_property_with_source_partial_eq)(self, rhs) } }

    impl Eq for AzCascadedCssPropertyWithSource { }

    impl PartialOrd for AzCascadedCssPropertyWithSource { fn partial_cmp(&self, rhs: &AzCascadedCssPropertyWithSource) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_cascaded_css_property_with_source_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzCascadedCssPropertyWithSource { fn cmp(&self, rhs: &AzCascadedCssPropertyWithSource) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_cascaded_css_property_with_source_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzCascadedCssPropertyWithSource { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_cascaded_css_property_with_source_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `CssPropertySource` struct
    #[repr(C, u8)] pub enum AzCssPropertySource {
        Css(AzCssPath),
        Inline,
    }

    impl PartialEq for AzCssPropertySource { fn eq(&self, rhs: &AzCssPropertySource) -> bool { (crate::dll::get_azul_dll().az_css_property_source_partial_eq)(self, rhs) } }

    impl Eq for AzCssPropertySource { }

    impl PartialOrd for AzCssPropertySource { fn partial_cmp(&self, rhs: &AzCssPropertySource) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_css_property_source_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzCssPropertySource { fn cmp(&self, rhs: &AzCssPropertySource) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_css_property_source_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzCssPropertySource { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_css_property_source_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `StyledNode` struct
    #[repr(C)] pub struct AzStyledNode {
        pub css_constraints: AzCascadedCssPropertyWithSourceVec,
        pub cascade_info: AzCascadeInfo,
        pub hover_group: AzOptionHoverGroup,
        pub tag_id: AzOptionTagId,
        pub style: AzRectStyle,
        pub layout: AzRectLayout,
    }

    impl PartialEq for AzStyledNode { fn eq(&self, rhs: &AzStyledNode) -> bool { (crate::dll::get_azul_dll().az_styled_node_partial_eq)(self, rhs) } }

    impl Eq for AzStyledNode { }

    impl PartialOrd for AzStyledNode { fn partial_cmp(&self, rhs: &AzStyledNode) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_styled_node_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzStyledNode { fn cmp(&self, rhs: &AzStyledNode) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_styled_node_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzStyledNode { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_styled_node_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `TagId` struct
    #[repr(C)] pub struct AzTagId {
        pub inner: u32,
    }

    impl Copy for AzTagId { }

    impl PartialEq for AzTagId { fn eq(&self, rhs: &AzTagId) -> bool { (crate::dll::get_azul_dll().az_tag_id_partial_eq)(self, rhs) } }

    impl Eq for AzTagId { }

    impl PartialOrd for AzTagId { fn partial_cmp(&self, rhs: &AzTagId) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_tag_id_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzTagId { fn cmp(&self, rhs: &AzTagId) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_tag_id_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzTagId { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_tag_id_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `TagIdToNodeIdMapping` struct
    #[repr(C)] pub struct AzTagIdToNodeIdMapping {
        pub tag_id: AzTagId,
        pub node_id: AzNodeId,
        pub tab_index: AzOptionTabIndex,
        pub hover_group: AzOptionHoverGroup,
    }

    impl Copy for AzTagIdToNodeIdMapping { }

    impl PartialEq for AzTagIdToNodeIdMapping { fn eq(&self, rhs: &AzTagIdToNodeIdMapping) -> bool { (crate::dll::get_azul_dll().az_tag_id_to_node_id_mapping_partial_eq)(self, rhs) } }

    impl Eq for AzTagIdToNodeIdMapping { }

    impl PartialOrd for AzTagIdToNodeIdMapping { fn partial_cmp(&self, rhs: &AzTagIdToNodeIdMapping) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_tag_id_to_node_id_mapping_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzTagIdToNodeIdMapping { fn cmp(&self, rhs: &AzTagIdToNodeIdMapping) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_tag_id_to_node_id_mapping_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzTagIdToNodeIdMapping { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_tag_id_to_node_id_mapping_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `HoverGroup` struct
    #[repr(C)] pub struct AzHoverGroup {
        pub affects_layout: bool,
        pub active_or_hover: AzActiveHover,
    }

    impl Copy for AzHoverGroup { }

    impl PartialEq for AzHoverGroup { fn eq(&self, rhs: &AzHoverGroup) -> bool { (crate::dll::get_azul_dll().az_hover_group_partial_eq)(self, rhs) } }

    impl Eq for AzHoverGroup { }

    impl PartialOrd for AzHoverGroup { fn partial_cmp(&self, rhs: &AzHoverGroup) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_hover_group_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzHoverGroup { fn cmp(&self, rhs: &AzHoverGroup) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_hover_group_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzHoverGroup { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_hover_group_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `ActiveHover` struct
    #[repr(C)] pub enum AzActiveHover {
        Active,
        Hover,
    }

    impl Copy for AzActiveHover { }

    impl PartialEq for AzActiveHover { fn eq(&self, rhs: &AzActiveHover) -> bool { (crate::dll::get_azul_dll().az_active_hover_partial_eq)(self, rhs) } }

    impl Eq for AzActiveHover { }

    impl PartialOrd for AzActiveHover { fn partial_cmp(&self, rhs: &AzActiveHover) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_active_hover_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzActiveHover { fn cmp(&self, rhs: &AzActiveHover) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_active_hover_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzActiveHover { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_active_hover_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `ParentWithNodeDepth` struct
    #[repr(C)] pub struct AzParentWithNodeDepth {
        pub depth: u32,
        pub node_id: AzNodeId,
    }

    impl Copy for AzParentWithNodeDepth { }

    impl PartialEq for AzParentWithNodeDepth { fn eq(&self, rhs: &AzParentWithNodeDepth) -> bool { (crate::dll::get_azul_dll().az_parent_with_node_depth_partial_eq)(self, rhs) } }

    impl Eq for AzParentWithNodeDepth { }

    impl PartialOrd for AzParentWithNodeDepth { fn partial_cmp(&self, rhs: &AzParentWithNodeDepth) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_parent_with_node_depth_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzParentWithNodeDepth { fn cmp(&self, rhs: &AzParentWithNodeDepth) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_parent_with_node_depth_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzParentWithNodeDepth { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_parent_with_node_depth_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `StyleOptions` struct
    #[repr(C)] pub struct AzStyleOptions {
        pub focused_node: AzOptionNodeId,
        pub hovered_nodes: AzNodeIdVec,
        pub is_mouse_down: bool,
    }

    impl PartialEq for AzStyleOptions { fn eq(&self, rhs: &AzStyleOptions) -> bool { (crate::dll::get_azul_dll().az_style_options_partial_eq)(self, rhs) } }

    impl Eq for AzStyleOptions { }

    impl PartialOrd for AzStyleOptions { fn partial_cmp(&self, rhs: &AzStyleOptions) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_style_options_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzStyleOptions { fn cmp(&self, rhs: &AzStyleOptions) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_style_options_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzStyleOptions { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_style_options_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `StyledDom` struct
    #[repr(C)] pub struct AzStyledDom {
        pub root: AzNodeId,
        pub node_hierarchy: AzNodeVec,
        pub node_data: AzNodeDataVec,
        pub styled_nodes: AzStyledNodeVec,
        pub tag_ids_to_node_ids: AzTagIdsToNodeIdsMappingVec,
        pub non_leaf_nodes: AzParentWithNodeDepthVec,
    }

    impl PartialEq for AzStyledDom { fn eq(&self, rhs: &AzStyledDom) -> bool { (crate::dll::get_azul_dll().az_styled_dom_partial_eq)(self, rhs) } }

    impl Eq for AzStyledDom { }

    impl PartialOrd for AzStyledDom { fn partial_cmp(&self, rhs: &AzStyledDom) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_styled_dom_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzStyledDom { fn cmp(&self, rhs: &AzStyledDom) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_styled_dom_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzStyledDom { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_styled_dom_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `Dom` struct
    #[repr(C)] pub struct AzDom {
        pub root: AzNodeData,
        pub children: AzDomVec,
        pub estimated_total_children: usize,
    }

    impl PartialEq for AzDom { fn eq(&self, rhs: &AzDom) -> bool { (crate::dll::get_azul_dll().az_dom_partial_eq)(self, rhs) } }

    impl Eq for AzDom { }

    impl PartialOrd for AzDom { fn partial_cmp(&self, rhs: &AzDom) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_dom_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzDom { fn cmp(&self, rhs: &AzDom) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_dom_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzDom { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_dom_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `GlTextureNode` struct
    #[repr(C)] pub struct AzGlTextureNode {
        pub callback: AzGlCallback,
        pub data: AzRefAny,
    }
    /// Re-export of rust-allocated (stack based) `IFrameNode` struct
    #[repr(C)] pub struct AzIFrameNode {
        pub callback: AzIFrameCallback,
        pub data: AzRefAny,
    }
    /// Re-export of rust-allocated (stack based) `CallbackData` struct
    #[repr(C)] pub struct AzCallbackData {
        pub event: AzEventFilter,
        pub callback: AzCallback,
        pub data: AzRefAny,
    }

    impl PartialEq for AzCallbackData { fn eq(&self, rhs: &AzCallbackData) -> bool { (crate::dll::get_azul_dll().az_callback_data_partial_eq)(self, rhs) } }

    impl Eq for AzCallbackData { }

    impl PartialOrd for AzCallbackData { fn partial_cmp(&self, rhs: &AzCallbackData) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_callback_data_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzCallbackData { fn cmp(&self, rhs: &AzCallbackData) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_callback_data_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzCallbackData { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_callback_data_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `ImageMask` struct
    #[repr(C)] pub struct AzImageMask {
        pub image: AzImageId,
        pub rect: AzLogicalRect,
        pub repeat: bool,
    }

    impl PartialEq for AzImageMask { fn eq(&self, rhs: &AzImageMask) -> bool { (crate::dll::get_azul_dll().az_image_mask_partial_eq)(self, rhs) } }

    impl Eq for AzImageMask { }

    impl PartialOrd for AzImageMask { fn partial_cmp(&self, rhs: &AzImageMask) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_image_mask_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzImageMask { fn cmp(&self, rhs: &AzImageMask) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_image_mask_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzImageMask { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_image_mask_hash)(self)).hash(state) } }
    /// Represents one single DOM node (node type, classes, ids and callbacks are stored here)
    #[repr(C)] pub struct AzNodeData {
        pub node_type: AzNodeType,
        pub ids: AzStringVec,
        pub classes: AzStringVec,
        pub callbacks: AzCallbackDataVec,
        pub inline_css_props: AzCssPropertyVec,
        pub clip_mask: AzOptionImageMask,
        pub is_draggable: bool,
        pub tab_index: AzOptionTabIndex,
    }
    /// List of core DOM node types built-into by `azul`
    #[repr(C, u8)] pub enum AzNodeType {
        Div,
        Body,
        Label(AzString),
        Text(AzTextId),
        Image(AzImageId),
        GlTexture(AzGlTextureNode),
        IFrame(AzIFrameNode),
    }
    /// When to call a callback action - `On::MouseOver`, `On::MouseOut`, etc.
    #[repr(C)] pub enum AzOn {
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
    #[repr(C, u8)] pub enum AzEventFilter {
        Hover(AzHoverEventFilter),
        Not(AzNotEventFilter),
        Focus(AzFocusEventFilter),
        Window(AzWindowEventFilter),
    }
    /// Re-export of rust-allocated (stack based) `HoverEventFilter` struct
    #[repr(C)] pub enum AzHoverEventFilter {
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
    }
    /// Re-export of rust-allocated (stack based) `FocusEventFilter` struct
    #[repr(C)] pub enum AzFocusEventFilter {
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
    #[repr(C, u8)] pub enum AzNotEventFilter {
        Hover(AzHoverEventFilter),
        Focus(AzFocusEventFilter),
    }
    /// Re-export of rust-allocated (stack based) `WindowEventFilter` struct
    #[repr(C)] pub enum AzWindowEventFilter {
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
    }
    /// Re-export of rust-allocated (stack based) `TabIndex` struct
    #[repr(C, u8)] pub enum AzTabIndex {
        Auto,
        OverrideInParent(usize),
        NoKeyboardFocus,
    }
    /// Re-export of rust-allocated (stack based) `GlShaderPrecisionFormatReturn` struct
    #[repr(C)] pub struct AzGlShaderPrecisionFormatReturn {
        pub _0: i32,
        pub _1: i32,
        pub _2: i32,
    }
    /// Re-export of rust-allocated (stack based) `VertexAttributeType` struct
    #[repr(C)] pub enum AzVertexAttributeType {
        Float,
        Double,
        UnsignedByte,
        UnsignedShort,
        UnsignedInt,
    }
    /// Re-export of rust-allocated (stack based) `VertexAttribute` struct
    #[repr(C)] pub struct AzVertexAttribute {
        pub name: AzString,
        pub layout_location: AzOptionUsize,
        pub attribute_type: AzVertexAttributeType,
        pub item_count: usize,
    }
    /// Re-export of rust-allocated (stack based) `VertexLayout` struct
    #[repr(C)] pub struct AzVertexLayout {
        pub fields: AzVertexAttributeVec,
    }
    /// Re-export of rust-allocated (stack based) `VertexArrayObject` struct
    #[repr(C)] pub struct AzVertexArrayObject {
        pub vertex_layout: AzVertexLayout,
        pub vao_id: u32,
        pub gl_context: AzGlContextPtr,
    }
    /// Re-export of rust-allocated (stack based) `IndexBufferFormat` struct
    #[repr(C)] pub enum AzIndexBufferFormat {
        Points,
        Lines,
        LineStrip,
        Triangles,
        TriangleStrip,
        TriangleFan,
    }
    /// Re-export of rust-allocated (stack based) `VertexBuffer` struct
    #[repr(C)] pub struct AzVertexBuffer {
        pub vertex_buffer_id: u32,
        pub vertex_buffer_len: usize,
        pub vao: AzVertexArrayObject,
        pub index_buffer_id: u32,
        pub index_buffer_len: usize,
        pub index_buffer_format: AzIndexBufferFormat,
    }
    /// Re-export of rust-allocated (stack based) `GlType` struct
    #[repr(C)] pub enum AzGlType {
        Gl,
        Gles,
    }
    /// Re-export of rust-allocated (stack based) `DebugMessage` struct
    #[repr(C)] pub struct AzDebugMessage {
        pub message: AzString,
        pub source: u32,
        pub ty: u32,
        pub id: u32,
        pub severity: u32,
    }

    impl PartialEq for AzDebugMessage { fn eq(&self, rhs: &AzDebugMessage) -> bool { (crate::dll::get_azul_dll().az_debug_message_partial_eq)(self, rhs) } }

    impl Eq for AzDebugMessage { }

    impl PartialOrd for AzDebugMessage { fn partial_cmp(&self, rhs: &AzDebugMessage) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_debug_message_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzDebugMessage { fn cmp(&self, rhs: &AzDebugMessage) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_debug_message_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzDebugMessage { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_debug_message_hash)(self)).hash(state) } }
    /// C-ABI stable reexport of `&[u8]`
    #[repr(C)] pub struct AzU8VecRef {
        pub(crate) ptr: *const u8,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [u8]`
    #[repr(C)] pub struct AzU8VecRefMut {
        pub(crate) ptr: *mut u8,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[f32]`
    #[repr(C)] pub struct AzF32VecRef {
        pub(crate) ptr: *const f32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[i32]`
    #[repr(C)] pub struct AzI32VecRef {
        pub(crate) ptr: *const i32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[GLuint]` aka `&[u32]`
    #[repr(C)] pub struct AzGLuintVecRef {
        pub(crate) ptr: *const u32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[GLenum]` aka `&[u32]`
    #[repr(C)] pub struct AzGLenumVecRef {
        pub(crate) ptr: *const u32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [GLint]` aka `&mut [i32]`
    #[repr(C)] pub struct AzGLintVecRefMut {
        pub(crate) ptr: *mut i32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [GLint64]` aka `&mut [i64]`
    #[repr(C)] pub struct AzGLint64VecRefMut {
        pub(crate) ptr: *mut i64,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [GLboolean]` aka `&mut [u8]`
    #[repr(C)] pub struct AzGLbooleanVecRefMut {
        pub(crate) ptr: *mut u8,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [GLfloat]` aka `&mut [f32]`
    #[repr(C)] pub struct AzGLfloatVecRefMut {
        pub(crate) ptr: *mut f32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[Refstr]` aka `&mut [&str]`
    #[repr(C)] pub struct AzRefstrVecRef {
        pub(crate) ptr: *const AzRefstr,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&str`
    #[repr(C)] pub struct AzRefstr {
        pub(crate) ptr: *const u8,
        pub len: usize,
    }
    /// C-ABI stable reexport of `(U8Vec, u32)`
    #[repr(C)] pub struct AzGetProgramBinaryReturn {
        pub _0: AzU8Vec,
        pub _1: u32,
    }
    /// C-ABI stable reexport of `(i32, u32, AzString)`
    #[repr(C)] pub struct AzGetActiveAttribReturn {
        pub _0: i32,
        pub _1: u32,
        pub _2: AzString,
    }
    /// C-ABI stable reexport of `*const gleam::gl::GLsync`
    #[repr(C)] pub struct AzGLsyncPtr {
        pub(crate) ptr: *const c_void,
    }
    /// C-ABI stable reexport of `(i32, u32, AzString)`
    #[repr(C)] pub struct AzGetActiveUniformReturn {
        pub _0: i32,
        pub _1: u32,
        pub _2: AzString,
    }
    /// Re-export of rust-allocated (stack based) `GlContextPtr` struct
    #[repr(C)] pub struct AzGlContextPtr {
        pub(crate) ptr: *const c_void,
    }
    /// Re-export of rust-allocated (stack based) `Texture` struct
    #[repr(C)] pub struct AzTexture {
        pub texture_id: u32,
        pub flags: AzTextureFlags,
        pub size: AzPhysicalSizeU32,
        pub gl_context: AzGlContextPtr,
    }
    /// Re-export of rust-allocated (stack based) `TextureFlags` struct
    #[repr(C)] pub struct AzTextureFlags {
        pub is_opaque: bool,
        pub is_video_texture: bool,
    }
    /// Re-export of rust-allocated (stack based) `TextId` struct
    #[repr(C)] pub struct AzTextId {
        pub id: usize,
    }
    /// Re-export of rust-allocated (stack based) `ImageId` struct
    #[repr(C)] pub struct AzImageId {
        pub id: usize,
    }

    impl PartialEq for AzImageId { fn eq(&self, rhs: &AzImageId) -> bool { (crate::dll::get_azul_dll().az_image_id_partial_eq)(self, rhs) } }

    impl Eq for AzImageId { }

    impl PartialOrd for AzImageId { fn partial_cmp(&self, rhs: &AzImageId) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_image_id_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzImageId { fn cmp(&self, rhs: &AzImageId) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_image_id_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzImageId { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_image_id_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `FontId` struct
    #[repr(C)] pub struct AzFontId {
        pub id: usize,
    }
    /// Re-export of rust-allocated (stack based) `ImageSource` struct
    #[repr(C, u8)] pub enum AzImageSource {
        Embedded(AzU8Vec),
        File(AzString),
        Raw(AzRawImage),
    }
    /// Re-export of rust-allocated (stack based) `FontSource` struct
    #[repr(C, u8)] pub enum AzFontSource {
        Embedded(AzU8Vec),
        File(AzString),
        System(AzString),
    }
    /// Re-export of rust-allocated (stack based) `RawImage` struct
    #[repr(C)] pub struct AzRawImage {
        pub pixels: AzU8Vec,
        pub width: usize,
        pub height: usize,
        pub data_format: AzRawImageFormat,
    }
    /// Re-export of rust-allocated (stack based) `RawImageFormat` struct
    #[repr(C)] pub enum AzRawImageFormat {
        R8,
        R16,
        RG16,
        BGRA8,
        RGBAF32,
        RG8,
        RGBAI32,
        RGBA8,
    }
    /// Re-export of rust-allocated (stack based) `SvgMultiPolygon` struct
    #[repr(C)] pub struct AzSvgMultiPolygon {
        pub rings: AzSvgPathVec,
    }
    /// Re-export of rust-allocated (stack based) `SvgNode` struct
    #[repr(C, u8)] pub enum AzSvgNode {
        MultiPolygonCollection(AzSvgMultiPolygonVec),
        MultiPolygon(AzSvgMultiPolygon),
        Path(AzSvgPath),
        Circle(AzSvgCircle),
        Rect(AzSvgRect),
    }
    /// Re-export of rust-allocated (stack based) `SvgStyledNode` struct
    #[repr(C)] pub struct AzSvgStyledNode {
        pub geometry: AzSvgNode,
        pub style: AzSvgStyle,
    }
    /// Re-export of rust-allocated (stack based) `SvgCircle` struct
    #[repr(C)] pub struct AzSvgCircle {
        pub center_x: f32,
        pub center_y: f32,
        pub radius: f32,
    }
    /// Re-export of rust-allocated (stack based) `SvgPath` struct
    #[repr(C)] pub struct AzSvgPath {
        pub items: AzSvgPathElementVec,
    }
    /// Re-export of rust-allocated (stack based) `SvgPathElement` struct
    #[repr(C, u8)] pub enum AzSvgPathElement {
        Line(AzSvgLine),
        QuadraticCurve(AzSvgQuadraticCurve),
        CubicCurve(AzSvgCubicCurve),
    }
    /// Re-export of rust-allocated (stack based) `SvgLine` struct
    #[repr(C)] pub struct AzSvgLine {
        pub start: AzSvgPoint,
        pub end: AzSvgPoint,
    }
    /// Re-export of rust-allocated (stack based) `SvgPoint` struct
    #[repr(C)] pub struct AzSvgPoint {
        pub x: f32,
        pub y: f32,
    }
    /// Re-export of rust-allocated (stack based) `SvgVertex` struct
    #[repr(C)] pub struct AzSvgVertex {
        pub x: f32,
        pub y: f32,
    }
    /// Re-export of rust-allocated (stack based) `SvgQuadraticCurve` struct
    #[repr(C)] pub struct AzSvgQuadraticCurve {
        pub start: AzSvgPoint,
        pub ctrl: AzSvgPoint,
        pub end: AzSvgPoint,
    }
    /// Re-export of rust-allocated (stack based) `SvgCubicCurve` struct
    #[repr(C)] pub struct AzSvgCubicCurve {
        pub start: AzSvgPoint,
        pub ctrl_1: AzSvgPoint,
        pub ctrl_2: AzSvgPoint,
        pub end: AzSvgPoint,
    }
    /// Re-export of rust-allocated (stack based) `SvgRect` struct
    #[repr(C)] pub struct AzSvgRect {
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
    #[repr(C)] pub struct AzTesselatedCPUSvgNode {
        pub vertices: AzSvgVertexVec,
        pub indices: AzU32Vec,
    }
    /// Re-export of rust-allocated (stack based) `TesselatedGPUSvgNode` struct
    #[repr(C)] pub struct AzTesselatedGPUSvgNode {
        pub vertex_index_buffer: AzVertexBuffer,
    }
    /// Re-export of rust-allocated (stack based) `SvgLineCap` struct
    #[repr(C)] pub enum AzSvgLineCap {
        Butt,
        Square,
        Round,
    }
    /// Re-export of rust-allocated (stack based) `SvgParseOptions` struct
    #[repr(C)] pub struct AzSvgParseOptions {
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
    #[repr(C)] pub enum AzShapeRendering {
        OptimizeSpeed,
        CrispEdges,
        GeometricPrecision,
    }
    /// Re-export of rust-allocated (stack based) `TextRendering` struct
    #[repr(C)] pub enum AzTextRendering {
        OptimizeSpeed,
        OptimizeLegibility,
        GeometricPrecision,
    }
    /// Re-export of rust-allocated (stack based) `ImageRendering` struct
    #[repr(C)] pub enum AzImageRendering {
        OptimizeQuality,
        OptimizeSpeed,
    }
    /// Re-export of rust-allocated (stack based) `FontDatabase` struct
    #[repr(C)] pub enum AzFontDatabase {
        Empty,
        System,
    }
    /// Re-export of rust-allocated (stack based) `SvgRenderOptions` struct
    #[repr(C)] pub struct AzSvgRenderOptions {
        pub background_color: AzOptionColorU,
        pub fit: AzSvgFitTo,
    }
    /// Re-export of rust-allocated (stack based) `SvgFitTo` struct
    #[repr(C, u8)] pub enum AzSvgFitTo {
        Original,
        Width(u32),
        Height(u32),
        Zoom(f32),
    }
    /// Re-export of rust-allocated (stack based) `Svg` struct
    #[repr(C)] pub struct AzSvg {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `SvgXmlNode` struct
    #[repr(C)] pub struct AzSvgXmlNode {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `SvgLineJoin` struct
    #[repr(C)] pub enum AzSvgLineJoin {
        Miter,
        MiterClip,
        Round,
        Bevel,
    }
    /// Re-export of rust-allocated (stack based) `SvgDashPattern` struct
    #[repr(C)] pub struct AzSvgDashPattern {
        pub offset: f32,
        pub length_1: f32,
        pub gap_1: f32,
        pub length_2: f32,
        pub gap_2: f32,
        pub length_3: f32,
        pub gap_3: f32,
    }
    /// Re-export of rust-allocated (stack based) `SvgStyle` struct
    #[repr(C, u8)] pub enum AzSvgStyle {
        Fill(AzSvgFillStyle),
        Stroke(AzSvgStrokeStyle),
    }
    /// Re-export of rust-allocated (stack based) `SvgFillStyle` struct
    #[repr(C)] pub struct AzSvgFillStyle {
        pub line_join: AzSvgLineJoin,
        pub miter_limit: usize,
        pub tolerance: usize,
    }
    /// Re-export of rust-allocated (stack based) `SvgStrokeStyle` struct
    #[repr(C)] pub struct AzSvgStrokeStyle {
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
    #[repr(C)] pub struct AzSvgNodeId {
        pub id: usize,
    }
    /// Pointer to rust-allocated `Box<DropCheckPtr>` struct
    #[repr(C)] pub struct AzDropCheckPtrPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Pointer to rust-allocated `Box<ArcMutexRefAny>` struct
    #[repr(C)] pub struct AzArcMutexRefAnyPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `Timer` struct
    #[repr(C)] pub struct AzTimer {
        pub created: AzInstantPtr,
        pub last_run: AzOptionInstantPtr,
        pub delay: AzOptionInstantPtr,
        pub interval: AzOptionDuration,
        pub timeout: AzOptionDuration,
        pub callback: AzTimerCallback,
    }
    /// Pointer to rust-allocated `Box<Task>` struct
    #[repr(C)] pub struct AzTaskPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Pointer to rust-allocated `Box<Thread>` struct
    #[repr(C)] pub struct AzThreadPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Pointer to rust-allocated `Box<DropCheck>` struct
    #[repr(C)] pub struct AzDropCheckPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `TimerId` struct
    #[repr(C)] pub struct AzTimerId {
        pub id: usize,
    }
    /// Should a timer terminate or not - used to remove active timers
    #[repr(C)] pub enum AzTerminateTimer {
        Terminate,
        Continue,
    }
    /// Re-export of rust-allocated (stack based) `BlockError` struct
    #[repr(C)] pub enum AzBlockError {
        ArcUnlockError,
        ThreadJoinError,
        MutexIntoInnerError,
    }
    /// Re-export of rust-allocated (stack based) `TaskBarIcon` struct
    #[repr(C)] pub struct AzTaskBarIcon {
        pub key: AzIconKey,
        pub rgba_bytes: AzU8Vec,
    }
    /// Re-export of rust-allocated (stack based) `XWindowType` struct
    #[repr(C)] pub enum AzXWindowType {
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
    #[repr(C)] pub struct AzPhysicalPositionI32 {
        pub x: i32,
        pub y: i32,
    }

    impl PartialEq for AzPhysicalPositionI32 { fn eq(&self, rhs: &AzPhysicalPositionI32) -> bool { (crate::dll::get_azul_dll().az_physical_position_i32_partial_eq)(self, rhs) } }

    impl Eq for AzPhysicalPositionI32 { }

    impl PartialOrd for AzPhysicalPositionI32 { fn partial_cmp(&self, rhs: &AzPhysicalPositionI32) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_physical_position_i32_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzPhysicalPositionI32 { fn cmp(&self, rhs: &AzPhysicalPositionI32) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_physical_position_i32_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzPhysicalPositionI32 { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_physical_position_i32_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `PhysicalSizeU32` struct
    #[repr(C)] pub struct AzPhysicalSizeU32 {
        pub width: u32,
        pub height: u32,
    }

    impl PartialEq for AzPhysicalSizeU32 { fn eq(&self, rhs: &AzPhysicalSizeU32) -> bool { (crate::dll::get_azul_dll().az_physical_size_u32_partial_eq)(self, rhs) } }

    impl Eq for AzPhysicalSizeU32 { }

    impl PartialOrd for AzPhysicalSizeU32 { fn partial_cmp(&self, rhs: &AzPhysicalSizeU32) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_physical_size_u32_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzPhysicalSizeU32 { fn cmp(&self, rhs: &AzPhysicalSizeU32) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_physical_size_u32_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzPhysicalSizeU32 { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_physical_size_u32_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `LogicalPosition` struct
    #[repr(C)] pub struct AzLogicalPosition {
        pub x: f32,
        pub y: f32,
    }
    /// Re-export of rust-allocated (stack based) `LogicalRect` struct
    #[repr(C)] pub struct AzLogicalRect {
        pub origin: AzLogicalPosition,
        pub size: AzLogicalSize,
    }
    /// Re-export of rust-allocated (stack based) `IconKey` struct
    #[repr(C)] pub struct AzIconKey {
        pub id: usize,
    }
    /// Re-export of rust-allocated (stack based) `SmallWindowIconBytes` struct
    #[repr(C)] pub struct AzSmallWindowIconBytes {
        pub key: AzIconKey,
        pub rgba_bytes: AzU8Vec,
    }
    /// Re-export of rust-allocated (stack based) `LargeWindowIconBytes` struct
    #[repr(C)] pub struct AzLargeWindowIconBytes {
        pub key: AzIconKey,
        pub rgba_bytes: AzU8Vec,
    }
    /// Re-export of rust-allocated (stack based) `WindowIcon` struct
    #[repr(C, u8)] pub enum AzWindowIcon {
        Small(AzSmallWindowIconBytes),
        Large(AzLargeWindowIconBytes),
    }
    /// Re-export of rust-allocated (stack based) `VirtualKeyCode` struct
    #[repr(C)] pub enum AzVirtualKeyCode {
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
    #[repr(C, u8)] pub enum AzAcceleratorKey {
        Ctrl,
        Alt,
        Shift,
        Key(AzVirtualKeyCode),
    }
    /// Re-export of rust-allocated (stack based) `WindowSize` struct
    #[repr(C)] pub struct AzWindowSize {
        pub dimensions: AzLogicalSize,
        pub hidpi_factor: f32,
        pub winit_hidpi_factor: f32,
        pub min_dimensions: AzOptionLogicalSize,
        pub max_dimensions: AzOptionLogicalSize,
    }
    /// Re-export of rust-allocated (stack based) `WindowFlags` struct
    #[repr(C)] pub struct AzWindowFlags {
        pub is_maximized: bool,
        pub is_fullscreen: bool,
        pub has_decorations: bool,
        pub is_visible: bool,
        pub is_always_on_top: bool,
        pub is_resizable: bool,
    }
    /// Re-export of rust-allocated (stack based) `DebugState` struct
    #[repr(C)] pub struct AzDebugState {
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
    #[repr(C)] pub struct AzKeyboardState {
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
    #[repr(C)] pub enum AzMouseCursorType {
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
    #[repr(C, u8)] pub enum AzCursorPosition {
        OutOfWindow,
        Uninitialized,
        InWindow(AzLogicalPosition),
    }
    /// Re-export of rust-allocated (stack based) `MouseState` struct
    #[repr(C)] pub struct AzMouseState {
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
    #[repr(C)] pub struct AzPlatformSpecificOptions {
        pub windows_options: AzWindowsWindowOptions,
        pub linux_options: AzLinuxWindowOptions,
        pub mac_options: AzMacWindowOptions,
        pub wasm_options: AzWasmWindowOptions,
    }
    /// Re-export of rust-allocated (stack based) `WindowsWindowOptions` struct
    #[repr(C)] pub struct AzWindowsWindowOptions {
        pub no_redirection_bitmap: bool,
        pub window_icon: AzOptionWindowIcon,
        pub taskbar_icon: AzOptionTaskBarIcon,
        pub parent_window: AzOptionHwndHandle,
    }
    /// Re-export of rust-allocated (stack based) `WaylandTheme` struct
    #[repr(C)] pub struct AzWaylandTheme {
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
    #[repr(C, u8)] pub enum AzRendererType {
        Default,
        ForceHardware,
        ForceSoftware,
        Custom(AzGlContextPtr),
    }
    /// Re-export of rust-allocated (stack based) `StringPair` struct
    #[repr(C)] pub struct AzStringPair {
        pub key: AzString,
        pub value: AzString,
    }
    /// Re-export of rust-allocated (stack based) `LinuxWindowOptions` struct
    #[repr(C)] pub struct AzLinuxWindowOptions {
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
    #[repr(C)] pub struct AzMacWindowOptions {
        pub request_user_attention: bool,
    }
    /// Re-export of rust-allocated (stack based) `WasmWindowOptions` struct
    #[repr(C)] pub struct AzWasmWindowOptions {
    }
    /// Re-export of rust-allocated (stack based) `FullScreenMode` struct
    #[repr(C)] pub enum AzFullScreenMode {
        SlowFullScreen,
        FastFullScreen,
        SlowWindowed,
        FastWindowed,
    }
    /// Re-export of rust-allocated (stack based) `WindowState` struct
    #[repr(C)] pub struct AzWindowState {
        pub title: AzString,
        pub size: AzWindowSize,
        pub position: AzOptionPhysicalPositionI32,
        pub flags: AzWindowFlags,
        pub debug_state: AzDebugState,
        pub keyboard_state: AzKeyboardState,
        pub mouse_state: AzMouseState,
        pub ime_position: AzOptionLogicalPosition,
        pub platform_specific_options: AzPlatformSpecificOptions,
        pub css: AzCss,
        pub layout_callback: AzLayoutCallback,
    }
    /// Re-export of rust-allocated (stack based) `LogicalSize` struct
    #[repr(C)] pub struct AzLogicalSize {
        pub width: f32,
        pub height: f32,
    }
    /// Re-export of rust-allocated (stack based) `HotReloadOptions` struct
    #[repr(C)] pub struct AzHotReloadOptions {
        pub path: AzString,
        pub reload_interval: AzDuration,
        pub apply_native_css: bool,
    }
    /// Re-export of rust-allocated (stack based) `WindowCreateOptions` struct
    #[repr(C)] pub struct AzWindowCreateOptions {
        pub state: AzWindowState,
        pub renderer_type: AzRendererType,
        pub hot_reload: AzOptionHotReloadOptions,
    }


    use libloading_mini::Library;
    pub struct AzulDll {
        pub lib: Library,
        pub az_string_from_utf8_unchecked: extern "C" fn(_:  *const u8, _:  usize) -> AzString,
        pub az_string_from_utf8_lossy: extern "C" fn(_:  *const u8, _:  usize) -> AzString,
        pub az_string_into_bytes: extern "C" fn(_:  AzString) -> AzU8Vec,
        pub az_string_delete: extern "C" fn(_:  &mut AzString),
        pub az_string_deep_copy: extern "C" fn(_:  &AzString) -> AzString,
        pub az_string_fmt_debug: extern "C" fn(_:  &AzString) -> AzString,
        pub az_string_partial_eq: extern "C" fn(_:  &AzString, _:  &AzString) -> bool,
        pub az_string_partial_cmp: extern "C" fn(_:  &AzString, _:  &AzString) -> u8,
        pub az_string_cmp: extern "C" fn(_:  &AzString, _:  &AzString) -> u8,
        pub az_string_hash: extern "C" fn(_:  &AzString) -> u64,
        pub az_css_property_vec_new: extern "C" fn() -> AzCssPropertyVec,
        pub az_css_property_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssPropertyVec,
        pub az_css_property_vec_copy_from: extern "C" fn(_:  *const AzCssProperty, _:  usize) -> AzCssPropertyVec,
        pub az_css_property_vec_delete: extern "C" fn(_:  &mut AzCssPropertyVec),
        pub az_css_property_vec_deep_copy: extern "C" fn(_:  &AzCssPropertyVec) -> AzCssPropertyVec,
        pub az_css_property_vec_fmt_debug: extern "C" fn(_:  &AzCssPropertyVec) -> AzString,
        pub az_svg_multi_polygon_vec_new: extern "C" fn() -> AzSvgMultiPolygonVec,
        pub az_svg_multi_polygon_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgMultiPolygonVec,
        pub az_svg_multi_polygon_vec_copy_from: extern "C" fn(_:  *const AzSvgMultiPolygon, _:  usize) -> AzSvgMultiPolygonVec,
        pub az_svg_multi_polygon_vec_delete: extern "C" fn(_:  &mut AzSvgMultiPolygonVec),
        pub az_svg_multi_polygon_vec_deep_copy: extern "C" fn(_:  &AzSvgMultiPolygonVec) -> AzSvgMultiPolygonVec,
        pub az_svg_multi_polygon_vec_fmt_debug: extern "C" fn(_:  &AzSvgMultiPolygonVec) -> AzString,
        pub az_svg_path_vec_new: extern "C" fn() -> AzSvgPathVec,
        pub az_svg_path_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgPathVec,
        pub az_svg_path_vec_copy_from: extern "C" fn(_:  *const AzSvgPath, _:  usize) -> AzSvgPathVec,
        pub az_svg_path_vec_delete: extern "C" fn(_:  &mut AzSvgPathVec),
        pub az_svg_path_vec_deep_copy: extern "C" fn(_:  &AzSvgPathVec) -> AzSvgPathVec,
        pub az_svg_path_vec_fmt_debug: extern "C" fn(_:  &AzSvgPathVec) -> AzString,
        pub az_vertex_attribute_vec_new: extern "C" fn() -> AzVertexAttributeVec,
        pub az_vertex_attribute_vec_with_capacity: extern "C" fn(_:  usize) -> AzVertexAttributeVec,
        pub az_vertex_attribute_vec_copy_from: extern "C" fn(_:  *const AzVertexAttribute, _:  usize) -> AzVertexAttributeVec,
        pub az_vertex_attribute_vec_delete: extern "C" fn(_:  &mut AzVertexAttributeVec),
        pub az_vertex_attribute_vec_deep_copy: extern "C" fn(_:  &AzVertexAttributeVec) -> AzVertexAttributeVec,
        pub az_vertex_attribute_vec_fmt_debug: extern "C" fn(_:  &AzVertexAttributeVec) -> AzString,
        pub az_svg_path_element_vec_new: extern "C" fn() -> AzSvgPathElementVec,
        pub az_svg_path_element_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgPathElementVec,
        pub az_svg_path_element_vec_copy_from: extern "C" fn(_:  *const AzSvgPathElement, _:  usize) -> AzSvgPathElementVec,
        pub az_svg_path_element_vec_delete: extern "C" fn(_:  &mut AzSvgPathElementVec),
        pub az_svg_path_element_vec_deep_copy: extern "C" fn(_:  &AzSvgPathElementVec) -> AzSvgPathElementVec,
        pub az_svg_path_element_vec_fmt_debug: extern "C" fn(_:  &AzSvgPathElementVec) -> AzString,
        pub az_svg_vertex_vec_new: extern "C" fn() -> AzSvgVertexVec,
        pub az_svg_vertex_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgVertexVec,
        pub az_svg_vertex_vec_copy_from: extern "C" fn(_:  *const AzSvgVertex, _:  usize) -> AzSvgVertexVec,
        pub az_svg_vertex_vec_delete: extern "C" fn(_:  &mut AzSvgVertexVec),
        pub az_svg_vertex_vec_deep_copy: extern "C" fn(_:  &AzSvgVertexVec) -> AzSvgVertexVec,
        pub az_svg_vertex_vec_fmt_debug: extern "C" fn(_:  &AzSvgVertexVec) -> AzString,
        pub az_u32_vec_new: extern "C" fn() -> AzU32Vec,
        pub az_u32_vec_with_capacity: extern "C" fn(_:  usize) -> AzU32Vec,
        pub az_u32_vec_copy_from: extern "C" fn(_:  *const u32, _:  usize) -> AzU32Vec,
        pub az_u32_vec_delete: extern "C" fn(_:  &mut AzU32Vec),
        pub az_u32_vec_deep_copy: extern "C" fn(_:  &AzU32Vec) -> AzU32Vec,
        pub az_u32_vec_fmt_debug: extern "C" fn(_:  &AzU32Vec) -> AzString,
        pub az_x_window_type_vec_new: extern "C" fn() -> AzXWindowTypeVec,
        pub az_x_window_type_vec_with_capacity: extern "C" fn(_:  usize) -> AzXWindowTypeVec,
        pub az_x_window_type_vec_copy_from: extern "C" fn(_:  *const AzXWindowType, _:  usize) -> AzXWindowTypeVec,
        pub az_x_window_type_vec_delete: extern "C" fn(_:  &mut AzXWindowTypeVec),
        pub az_x_window_type_vec_deep_copy: extern "C" fn(_:  &AzXWindowTypeVec) -> AzXWindowTypeVec,
        pub az_x_window_type_vec_fmt_debug: extern "C" fn(_:  &AzXWindowTypeVec) -> AzString,
        pub az_virtual_key_code_vec_new: extern "C" fn() -> AzVirtualKeyCodeVec,
        pub az_virtual_key_code_vec_with_capacity: extern "C" fn(_:  usize) -> AzVirtualKeyCodeVec,
        pub az_virtual_key_code_vec_copy_from: extern "C" fn(_:  *const AzVirtualKeyCode, _:  usize) -> AzVirtualKeyCodeVec,
        pub az_virtual_key_code_vec_delete: extern "C" fn(_:  &mut AzVirtualKeyCodeVec),
        pub az_virtual_key_code_vec_deep_copy: extern "C" fn(_:  &AzVirtualKeyCodeVec) -> AzVirtualKeyCodeVec,
        pub az_virtual_key_code_vec_fmt_debug: extern "C" fn(_:  &AzVirtualKeyCodeVec) -> AzString,
        pub az_scan_code_vec_new: extern "C" fn() -> AzScanCodeVec,
        pub az_scan_code_vec_with_capacity: extern "C" fn(_:  usize) -> AzScanCodeVec,
        pub az_scan_code_vec_copy_from: extern "C" fn(_:  *const u32, _:  usize) -> AzScanCodeVec,
        pub az_scan_code_vec_delete: extern "C" fn(_:  &mut AzScanCodeVec),
        pub az_scan_code_vec_deep_copy: extern "C" fn(_:  &AzScanCodeVec) -> AzScanCodeVec,
        pub az_scan_code_vec_fmt_debug: extern "C" fn(_:  &AzScanCodeVec) -> AzString,
        pub az_css_declaration_vec_new: extern "C" fn() -> AzCssDeclarationVec,
        pub az_css_declaration_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssDeclarationVec,
        pub az_css_declaration_vec_copy_from: extern "C" fn(_:  *const AzCssDeclaration, _:  usize) -> AzCssDeclarationVec,
        pub az_css_declaration_vec_delete: extern "C" fn(_:  &mut AzCssDeclarationVec),
        pub az_css_declaration_vec_deep_copy: extern "C" fn(_:  &AzCssDeclarationVec) -> AzCssDeclarationVec,
        pub az_css_declaration_vec_fmt_debug: extern "C" fn(_:  &AzCssDeclarationVec) -> AzString,
        pub az_css_path_selector_vec_new: extern "C" fn() -> AzCssPathSelectorVec,
        pub az_css_path_selector_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssPathSelectorVec,
        pub az_css_path_selector_vec_copy_from: extern "C" fn(_:  *const AzCssPathSelector, _:  usize) -> AzCssPathSelectorVec,
        pub az_css_path_selector_vec_delete: extern "C" fn(_:  &mut AzCssPathSelectorVec),
        pub az_css_path_selector_vec_deep_copy: extern "C" fn(_:  &AzCssPathSelectorVec) -> AzCssPathSelectorVec,
        pub az_css_path_selector_vec_fmt_debug: extern "C" fn(_:  &AzCssPathSelectorVec) -> AzString,
        pub az_stylesheet_vec_new: extern "C" fn() -> AzStylesheetVec,
        pub az_stylesheet_vec_with_capacity: extern "C" fn(_:  usize) -> AzStylesheetVec,
        pub az_stylesheet_vec_copy_from: extern "C" fn(_:  *const AzStylesheet, _:  usize) -> AzStylesheetVec,
        pub az_stylesheet_vec_delete: extern "C" fn(_:  &mut AzStylesheetVec),
        pub az_stylesheet_vec_deep_copy: extern "C" fn(_:  &AzStylesheetVec) -> AzStylesheetVec,
        pub az_stylesheet_vec_fmt_debug: extern "C" fn(_:  &AzStylesheetVec) -> AzString,
        pub az_css_rule_block_vec_new: extern "C" fn() -> AzCssRuleBlockVec,
        pub az_css_rule_block_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssRuleBlockVec,
        pub az_css_rule_block_vec_copy_from: extern "C" fn(_:  *const AzCssRuleBlock, _:  usize) -> AzCssRuleBlockVec,
        pub az_css_rule_block_vec_delete: extern "C" fn(_:  &mut AzCssRuleBlockVec),
        pub az_css_rule_block_vec_deep_copy: extern "C" fn(_:  &AzCssRuleBlockVec) -> AzCssRuleBlockVec,
        pub az_css_rule_block_vec_fmt_debug: extern "C" fn(_:  &AzCssRuleBlockVec) -> AzString,
        pub az_u8_vec_new: extern "C" fn() -> AzU8Vec,
        pub az_u8_vec_with_capacity: extern "C" fn(_:  usize) -> AzU8Vec,
        pub az_u8_vec_copy_from: extern "C" fn(_:  *const u8, _:  usize) -> AzU8Vec,
        pub az_u8_vec_delete: extern "C" fn(_:  &mut AzU8Vec),
        pub az_u8_vec_deep_copy: extern "C" fn(_:  &AzU8Vec) -> AzU8Vec,
        pub az_u8_vec_fmt_debug: extern "C" fn(_:  &AzU8Vec) -> AzString,
        pub az_callback_data_vec_new: extern "C" fn() -> AzCallbackDataVec,
        pub az_callback_data_vec_with_capacity: extern "C" fn(_:  usize) -> AzCallbackDataVec,
        pub az_callback_data_vec_copy_from: extern "C" fn(_:  *const AzCallbackData, _:  usize) -> AzCallbackDataVec,
        pub az_callback_data_vec_delete: extern "C" fn(_:  &mut AzCallbackDataVec),
        pub az_callback_data_vec_deep_copy: extern "C" fn(_:  &AzCallbackDataVec) -> AzCallbackDataVec,
        pub az_callback_data_vec_fmt_debug: extern "C" fn(_:  &AzCallbackDataVec) -> AzString,
        pub az_debug_message_vec_new: extern "C" fn() -> AzDebugMessageVec,
        pub az_debug_message_vec_with_capacity: extern "C" fn(_:  usize) -> AzDebugMessageVec,
        pub az_debug_message_vec_copy_from: extern "C" fn(_:  *const AzDebugMessage, _:  usize) -> AzDebugMessageVec,
        pub az_debug_message_vec_delete: extern "C" fn(_:  &mut AzDebugMessageVec),
        pub az_debug_message_vec_deep_copy: extern "C" fn(_:  &AzDebugMessageVec) -> AzDebugMessageVec,
        pub az_debug_message_vec_fmt_debug: extern "C" fn(_:  &AzDebugMessageVec) -> AzString,
        pub az_g_luint_vec_new: extern "C" fn() -> AzGLuintVec,
        pub az_g_luint_vec_with_capacity: extern "C" fn(_:  usize) -> AzGLuintVec,
        pub az_g_luint_vec_copy_from: extern "C" fn(_:  *const u32, _:  usize) -> AzGLuintVec,
        pub az_g_luint_vec_delete: extern "C" fn(_:  &mut AzGLuintVec),
        pub az_g_luint_vec_deep_copy: extern "C" fn(_:  &AzGLuintVec) -> AzGLuintVec,
        pub az_g_luint_vec_fmt_debug: extern "C" fn(_:  &AzGLuintVec) -> AzString,
        pub az_g_lint_vec_new: extern "C" fn() -> AzGLintVec,
        pub az_g_lint_vec_with_capacity: extern "C" fn(_:  usize) -> AzGLintVec,
        pub az_g_lint_vec_copy_from: extern "C" fn(_:  *const i32, _:  usize) -> AzGLintVec,
        pub az_g_lint_vec_delete: extern "C" fn(_:  &mut AzGLintVec),
        pub az_g_lint_vec_deep_copy: extern "C" fn(_:  &AzGLintVec) -> AzGLintVec,
        pub az_g_lint_vec_fmt_debug: extern "C" fn(_:  &AzGLintVec) -> AzString,
        pub az_dom_vec_new: extern "C" fn() -> AzDomVec,
        pub az_dom_vec_with_capacity: extern "C" fn(_:  usize) -> AzDomVec,
        pub az_dom_vec_copy_from: extern "C" fn(_:  *const AzDom, _:  usize) -> AzDomVec,
        pub az_dom_vec_delete: extern "C" fn(_:  &mut AzDomVec),
        pub az_dom_vec_deep_copy: extern "C" fn(_:  &AzDomVec) -> AzDomVec,
        pub az_dom_vec_fmt_debug: extern "C" fn(_:  &AzDomVec) -> AzString,
        pub az_string_vec_new: extern "C" fn() -> AzStringVec,
        pub az_string_vec_with_capacity: extern "C" fn(_:  usize) -> AzStringVec,
        pub az_string_vec_copy_from: extern "C" fn(_:  *const AzString, _:  usize) -> AzStringVec,
        pub az_string_vec_delete: extern "C" fn(_:  &mut AzStringVec),
        pub az_string_vec_deep_copy: extern "C" fn(_:  &AzStringVec) -> AzStringVec,
        pub az_string_vec_fmt_debug: extern "C" fn(_:  &AzStringVec) -> AzString,
        pub az_string_pair_vec_new: extern "C" fn() -> AzStringPairVec,
        pub az_string_pair_vec_with_capacity: extern "C" fn(_:  usize) -> AzStringPairVec,
        pub az_string_pair_vec_copy_from: extern "C" fn(_:  *const AzStringPair, _:  usize) -> AzStringPairVec,
        pub az_string_pair_vec_delete: extern "C" fn(_:  &mut AzStringPairVec),
        pub az_string_pair_vec_deep_copy: extern "C" fn(_:  &AzStringPairVec) -> AzStringPairVec,
        pub az_string_pair_vec_fmt_debug: extern "C" fn(_:  &AzStringPairVec) -> AzString,
        pub az_gradient_stop_pre_vec_new: extern "C" fn() -> AzGradientStopPreVec,
        pub az_gradient_stop_pre_vec_with_capacity: extern "C" fn(_:  usize) -> AzGradientStopPreVec,
        pub az_gradient_stop_pre_vec_copy_from: extern "C" fn(_:  *const AzGradientStopPre, _:  usize) -> AzGradientStopPreVec,
        pub az_gradient_stop_pre_vec_delete: extern "C" fn(_:  &mut AzGradientStopPreVec),
        pub az_gradient_stop_pre_vec_deep_copy: extern "C" fn(_:  &AzGradientStopPreVec) -> AzGradientStopPreVec,
        pub az_gradient_stop_pre_vec_fmt_debug: extern "C" fn(_:  &AzGradientStopPreVec) -> AzString,
        pub az_cascaded_css_property_with_source_vec_new: extern "C" fn() -> AzCascadedCssPropertyWithSourceVec,
        pub az_cascaded_css_property_with_source_vec_with_capacity: extern "C" fn(_:  usize) -> AzCascadedCssPropertyWithSourceVec,
        pub az_cascaded_css_property_with_source_vec_copy_from: extern "C" fn(_:  *const AzCascadedCssPropertyWithSource, _:  usize) -> AzCascadedCssPropertyWithSourceVec,
        pub az_cascaded_css_property_with_source_vec_delete: extern "C" fn(_:  &mut AzCascadedCssPropertyWithSourceVec),
        pub az_cascaded_css_property_with_source_vec_deep_copy: extern "C" fn(_:  &AzCascadedCssPropertyWithSourceVec) -> AzCascadedCssPropertyWithSourceVec,
        pub az_cascaded_css_property_with_source_vec_fmt_debug: extern "C" fn(_:  &AzCascadedCssPropertyWithSourceVec) -> AzString,
        pub az_node_id_vec_new: extern "C" fn() -> AzNodeIdVec,
        pub az_node_id_vec_with_capacity: extern "C" fn(_:  usize) -> AzNodeIdVec,
        pub az_node_id_vec_copy_from: extern "C" fn(_:  *const AzNodeId, _:  usize) -> AzNodeIdVec,
        pub az_node_id_vec_delete: extern "C" fn(_:  &mut AzNodeIdVec),
        pub az_node_id_vec_deep_copy: extern "C" fn(_:  &AzNodeIdVec) -> AzNodeIdVec,
        pub az_node_id_vec_fmt_debug: extern "C" fn(_:  &AzNodeIdVec) -> AzString,
        pub az_node_vec_new: extern "C" fn() -> AzNodeVec,
        pub az_node_vec_with_capacity: extern "C" fn(_:  usize) -> AzNodeVec,
        pub az_node_vec_copy_from: extern "C" fn(_:  *const AzNode, _:  usize) -> AzNodeVec,
        pub az_node_vec_delete: extern "C" fn(_:  &mut AzNodeVec),
        pub az_node_vec_deep_copy: extern "C" fn(_:  &AzNodeVec) -> AzNodeVec,
        pub az_node_vec_fmt_debug: extern "C" fn(_:  &AzNodeVec) -> AzString,
        pub az_styled_node_vec_new: extern "C" fn() -> AzStyledNodeVec,
        pub az_styled_node_vec_with_capacity: extern "C" fn(_:  usize) -> AzStyledNodeVec,
        pub az_styled_node_vec_copy_from: extern "C" fn(_:  *const AzStyledNode, _:  usize) -> AzStyledNodeVec,
        pub az_styled_node_vec_delete: extern "C" fn(_:  &mut AzStyledNodeVec),
        pub az_styled_node_vec_deep_copy: extern "C" fn(_:  &AzStyledNodeVec) -> AzStyledNodeVec,
        pub az_styled_node_vec_fmt_debug: extern "C" fn(_:  &AzStyledNodeVec) -> AzString,
        pub az_tag_ids_to_node_ids_mapping_vec_new: extern "C" fn() -> AzTagIdsToNodeIdsMappingVec,
        pub az_tag_ids_to_node_ids_mapping_vec_with_capacity: extern "C" fn(_:  usize) -> AzTagIdsToNodeIdsMappingVec,
        pub az_tag_ids_to_node_ids_mapping_vec_copy_from: extern "C" fn(_:  *const AzTagIdToNodeIdMapping, _:  usize) -> AzTagIdsToNodeIdsMappingVec,
        pub az_tag_ids_to_node_ids_mapping_vec_delete: extern "C" fn(_:  &mut AzTagIdsToNodeIdsMappingVec),
        pub az_tag_ids_to_node_ids_mapping_vec_deep_copy: extern "C" fn(_:  &AzTagIdsToNodeIdsMappingVec) -> AzTagIdsToNodeIdsMappingVec,
        pub az_tag_ids_to_node_ids_mapping_vec_fmt_debug: extern "C" fn(_:  &AzTagIdsToNodeIdsMappingVec) -> AzString,
        pub az_parent_with_node_depth_vec_new: extern "C" fn() -> AzParentWithNodeDepthVec,
        pub az_parent_with_node_depth_vec_with_capacity: extern "C" fn(_:  usize) -> AzParentWithNodeDepthVec,
        pub az_parent_with_node_depth_vec_copy_from: extern "C" fn(_:  *const AzParentWithNodeDepth, _:  usize) -> AzParentWithNodeDepthVec,
        pub az_parent_with_node_depth_vec_delete: extern "C" fn(_:  &mut AzParentWithNodeDepthVec),
        pub az_parent_with_node_depth_vec_deep_copy: extern "C" fn(_:  &AzParentWithNodeDepthVec) -> AzParentWithNodeDepthVec,
        pub az_parent_with_node_depth_vec_fmt_debug: extern "C" fn(_:  &AzParentWithNodeDepthVec) -> AzString,
        pub az_node_data_vec_new: extern "C" fn() -> AzNodeDataVec,
        pub az_node_data_vec_with_capacity: extern "C" fn(_:  usize) -> AzNodeDataVec,
        pub az_node_data_vec_copy_from: extern "C" fn(_:  *const AzNodeData, _:  usize) -> AzNodeDataVec,
        pub az_node_data_vec_delete: extern "C" fn(_:  &mut AzNodeDataVec),
        pub az_node_data_vec_deep_copy: extern "C" fn(_:  &AzNodeDataVec) -> AzNodeDataVec,
        pub az_node_data_vec_fmt_debug: extern "C" fn(_:  &AzNodeDataVec) -> AzString,
        pub az_option_node_id_delete: extern "C" fn(_:  &mut AzOptionNodeId),
        pub az_option_node_id_deep_copy: extern "C" fn(_:  &AzOptionNodeId) -> AzOptionNodeId,
        pub az_option_node_id_fmt_debug: extern "C" fn(_:  &AzOptionNodeId) -> AzString,
        pub az_option_dom_node_id_delete: extern "C" fn(_:  &mut AzOptionDomNodeId),
        pub az_option_dom_node_id_deep_copy: extern "C" fn(_:  &AzOptionDomNodeId) -> AzOptionDomNodeId,
        pub az_option_dom_node_id_fmt_debug: extern "C" fn(_:  &AzOptionDomNodeId) -> AzString,
        pub az_option_color_u_delete: extern "C" fn(_:  &mut AzOptionColorU),
        pub az_option_color_u_deep_copy: extern "C" fn(_:  &AzOptionColorU) -> AzOptionColorU,
        pub az_option_color_u_fmt_debug: extern "C" fn(_:  &AzOptionColorU) -> AzString,
        pub az_option_raw_image_delete: extern "C" fn(_:  &mut AzOptionRawImage),
        pub az_option_raw_image_deep_copy: extern "C" fn(_:  &AzOptionRawImage) -> AzOptionRawImage,
        pub az_option_raw_image_fmt_debug: extern "C" fn(_:  &AzOptionRawImage) -> AzString,
        pub az_option_svg_dash_pattern_delete: extern "C" fn(_:  &mut AzOptionSvgDashPattern),
        pub az_option_svg_dash_pattern_deep_copy: extern "C" fn(_:  &AzOptionSvgDashPattern) -> AzOptionSvgDashPattern,
        pub az_option_svg_dash_pattern_fmt_debug: extern "C" fn(_:  &AzOptionSvgDashPattern) -> AzString,
        pub az_option_wayland_theme_delete: extern "C" fn(_:  &mut AzOptionWaylandTheme),
        pub az_option_wayland_theme_deep_copy: extern "C" fn(_:  &AzOptionWaylandTheme) -> AzOptionWaylandTheme,
        pub az_option_wayland_theme_fmt_debug: extern "C" fn(_:  &AzOptionWaylandTheme) -> AzString,
        pub az_option_task_bar_icon_delete: extern "C" fn(_:  &mut AzOptionTaskBarIcon),
        pub az_option_task_bar_icon_deep_copy: extern "C" fn(_:  &AzOptionTaskBarIcon) -> AzOptionTaskBarIcon,
        pub az_option_task_bar_icon_fmt_debug: extern "C" fn(_:  &AzOptionTaskBarIcon) -> AzString,
        pub az_option_hwnd_handle_delete: extern "C" fn(_:  &mut AzOptionHwndHandle),
        pub az_option_hwnd_handle_deep_copy: extern "C" fn(_:  &AzOptionHwndHandle) -> AzOptionHwndHandle,
        pub az_option_hwnd_handle_fmt_debug: extern "C" fn(_:  &AzOptionHwndHandle) -> AzString,
        pub az_option_logical_position_delete: extern "C" fn(_:  &mut AzOptionLogicalPosition),
        pub az_option_logical_position_deep_copy: extern "C" fn(_:  &AzOptionLogicalPosition) -> AzOptionLogicalPosition,
        pub az_option_logical_position_fmt_debug: extern "C" fn(_:  &AzOptionLogicalPosition) -> AzString,
        pub az_option_hot_reload_options_delete: extern "C" fn(_:  &mut AzOptionHotReloadOptions),
        pub az_option_hot_reload_options_deep_copy: extern "C" fn(_:  &AzOptionHotReloadOptions) -> AzOptionHotReloadOptions,
        pub az_option_hot_reload_options_fmt_debug: extern "C" fn(_:  &AzOptionHotReloadOptions) -> AzString,
        pub az_option_physical_position_i32_delete: extern "C" fn(_:  &mut AzOptionPhysicalPositionI32),
        pub az_option_physical_position_i32_deep_copy: extern "C" fn(_:  &AzOptionPhysicalPositionI32) -> AzOptionPhysicalPositionI32,
        pub az_option_physical_position_i32_fmt_debug: extern "C" fn(_:  &AzOptionPhysicalPositionI32) -> AzString,
        pub az_option_window_icon_delete: extern "C" fn(_:  &mut AzOptionWindowIcon),
        pub az_option_window_icon_deep_copy: extern "C" fn(_:  &AzOptionWindowIcon) -> AzOptionWindowIcon,
        pub az_option_window_icon_fmt_debug: extern "C" fn(_:  &AzOptionWindowIcon) -> AzString,
        pub az_option_string_delete: extern "C" fn(_:  &mut AzOptionString),
        pub az_option_string_deep_copy: extern "C" fn(_:  &AzOptionString) -> AzOptionString,
        pub az_option_string_fmt_debug: extern "C" fn(_:  &AzOptionString) -> AzString,
        pub az_option_x11_visual_delete: extern "C" fn(_:  &mut AzOptionX11Visual),
        pub az_option_x11_visual_deep_copy: extern "C" fn(_:  &AzOptionX11Visual) -> AzOptionX11Visual,
        pub az_option_x11_visual_fmt_debug: extern "C" fn(_:  &AzOptionX11Visual) -> AzString,
        pub az_option_i32_delete: extern "C" fn(_:  &mut AzOptionI32),
        pub az_option_i32_deep_copy: extern "C" fn(_:  &AzOptionI32) -> AzOptionI32,
        pub az_option_i32_fmt_debug: extern "C" fn(_:  &AzOptionI32) -> AzString,
        pub az_option_f32_delete: extern "C" fn(_:  &mut AzOptionF32),
        pub az_option_f32_deep_copy: extern "C" fn(_:  &AzOptionF32) -> AzOptionF32,
        pub az_option_f32_fmt_debug: extern "C" fn(_:  &AzOptionF32) -> AzString,
        pub az_option_mouse_cursor_type_delete: extern "C" fn(_:  &mut AzOptionMouseCursorType),
        pub az_option_mouse_cursor_type_deep_copy: extern "C" fn(_:  &AzOptionMouseCursorType) -> AzOptionMouseCursorType,
        pub az_option_mouse_cursor_type_fmt_debug: extern "C" fn(_:  &AzOptionMouseCursorType) -> AzString,
        pub az_option_logical_size_delete: extern "C" fn(_:  &mut AzOptionLogicalSize),
        pub az_option_logical_size_deep_copy: extern "C" fn(_:  &AzOptionLogicalSize) -> AzOptionLogicalSize,
        pub az_option_logical_size_fmt_debug: extern "C" fn(_:  &AzOptionLogicalSize) -> AzString,
        pub az_option_char_delete: extern "C" fn(_:  &mut AzOptionChar),
        pub az_option_char_deep_copy: extern "C" fn(_:  &AzOptionChar) -> AzOptionChar,
        pub az_option_char_fmt_debug: extern "C" fn(_:  &AzOptionChar) -> AzString,
        pub az_option_virtual_key_code_delete: extern "C" fn(_:  &mut AzOptionVirtualKeyCode),
        pub az_option_virtual_key_code_deep_copy: extern "C" fn(_:  &AzOptionVirtualKeyCode) -> AzOptionVirtualKeyCode,
        pub az_option_virtual_key_code_fmt_debug: extern "C" fn(_:  &AzOptionVirtualKeyCode) -> AzString,
        pub az_option_percentage_value_delete: extern "C" fn(_:  &mut AzOptionPercentageValue),
        pub az_option_percentage_value_deep_copy: extern "C" fn(_:  &AzOptionPercentageValue) -> AzOptionPercentageValue,
        pub az_option_percentage_value_fmt_debug: extern "C" fn(_:  &AzOptionPercentageValue) -> AzString,
        pub az_option_dom_delete: extern "C" fn(_:  &mut AzOptionDom),
        pub az_option_dom_deep_copy: extern "C" fn(_:  &AzOptionDom) -> AzOptionDom,
        pub az_option_dom_fmt_debug: extern "C" fn(_:  &AzOptionDom) -> AzString,
        pub az_option_texture_delete: extern "C" fn(_:  &mut AzOptionTexture),
        pub az_option_texture_fmt_debug: extern "C" fn(_:  &AzOptionTexture) -> AzString,
        pub az_option_image_mask_delete: extern "C" fn(_:  &mut AzOptionImageMask),
        pub az_option_image_mask_deep_copy: extern "C" fn(_:  &AzOptionImageMask) -> AzOptionImageMask,
        pub az_option_image_mask_fmt_debug: extern "C" fn(_:  &AzOptionImageMask) -> AzString,
        pub az_option_tab_index_delete: extern "C" fn(_:  &mut AzOptionTabIndex),
        pub az_option_tab_index_deep_copy: extern "C" fn(_:  &AzOptionTabIndex) -> AzOptionTabIndex,
        pub az_option_tab_index_fmt_debug: extern "C" fn(_:  &AzOptionTabIndex) -> AzString,
        pub az_option_style_background_content_value_delete: extern "C" fn(_:  &mut AzOptionStyleBackgroundContentValue),
        pub az_option_style_background_content_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBackgroundContentValue) -> AzOptionStyleBackgroundContentValue,
        pub az_option_style_background_content_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBackgroundContentValue) -> AzString,
        pub az_option_style_background_position_value_delete: extern "C" fn(_:  &mut AzOptionStyleBackgroundPositionValue),
        pub az_option_style_background_position_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBackgroundPositionValue) -> AzOptionStyleBackgroundPositionValue,
        pub az_option_style_background_position_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBackgroundPositionValue) -> AzString,
        pub az_option_style_background_size_value_delete: extern "C" fn(_:  &mut AzOptionStyleBackgroundSizeValue),
        pub az_option_style_background_size_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBackgroundSizeValue) -> AzOptionStyleBackgroundSizeValue,
        pub az_option_style_background_size_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBackgroundSizeValue) -> AzString,
        pub az_option_style_background_repeat_value_delete: extern "C" fn(_:  &mut AzOptionStyleBackgroundRepeatValue),
        pub az_option_style_background_repeat_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBackgroundRepeatValue) -> AzOptionStyleBackgroundRepeatValue,
        pub az_option_style_background_repeat_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBackgroundRepeatValue) -> AzString,
        pub az_option_style_font_size_value_delete: extern "C" fn(_:  &mut AzOptionStyleFontSizeValue),
        pub az_option_style_font_size_value_deep_copy: extern "C" fn(_:  &AzOptionStyleFontSizeValue) -> AzOptionStyleFontSizeValue,
        pub az_option_style_font_size_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleFontSizeValue) -> AzString,
        pub az_option_style_font_family_value_delete: extern "C" fn(_:  &mut AzOptionStyleFontFamilyValue),
        pub az_option_style_font_family_value_deep_copy: extern "C" fn(_:  &AzOptionStyleFontFamilyValue) -> AzOptionStyleFontFamilyValue,
        pub az_option_style_font_family_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleFontFamilyValue) -> AzString,
        pub az_option_style_text_color_value_delete: extern "C" fn(_:  &mut AzOptionStyleTextColorValue),
        pub az_option_style_text_color_value_deep_copy: extern "C" fn(_:  &AzOptionStyleTextColorValue) -> AzOptionStyleTextColorValue,
        pub az_option_style_text_color_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleTextColorValue) -> AzString,
        pub az_option_style_text_alignment_horz_value_delete: extern "C" fn(_:  &mut AzOptionStyleTextAlignmentHorzValue),
        pub az_option_style_text_alignment_horz_value_deep_copy: extern "C" fn(_:  &AzOptionStyleTextAlignmentHorzValue) -> AzOptionStyleTextAlignmentHorzValue,
        pub az_option_style_text_alignment_horz_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleTextAlignmentHorzValue) -> AzString,
        pub az_option_style_line_height_value_delete: extern "C" fn(_:  &mut AzOptionStyleLineHeightValue),
        pub az_option_style_line_height_value_deep_copy: extern "C" fn(_:  &AzOptionStyleLineHeightValue) -> AzOptionStyleLineHeightValue,
        pub az_option_style_line_height_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleLineHeightValue) -> AzString,
        pub az_option_style_letter_spacing_value_delete: extern "C" fn(_:  &mut AzOptionStyleLetterSpacingValue),
        pub az_option_style_letter_spacing_value_deep_copy: extern "C" fn(_:  &AzOptionStyleLetterSpacingValue) -> AzOptionStyleLetterSpacingValue,
        pub az_option_style_letter_spacing_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleLetterSpacingValue) -> AzString,
        pub az_option_style_word_spacing_value_delete: extern "C" fn(_:  &mut AzOptionStyleWordSpacingValue),
        pub az_option_style_word_spacing_value_deep_copy: extern "C" fn(_:  &AzOptionStyleWordSpacingValue) -> AzOptionStyleWordSpacingValue,
        pub az_option_style_word_spacing_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleWordSpacingValue) -> AzString,
        pub az_option_style_tab_width_value_delete: extern "C" fn(_:  &mut AzOptionStyleTabWidthValue),
        pub az_option_style_tab_width_value_deep_copy: extern "C" fn(_:  &AzOptionStyleTabWidthValue) -> AzOptionStyleTabWidthValue,
        pub az_option_style_tab_width_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleTabWidthValue) -> AzString,
        pub az_option_style_cursor_value_delete: extern "C" fn(_:  &mut AzOptionStyleCursorValue),
        pub az_option_style_cursor_value_deep_copy: extern "C" fn(_:  &AzOptionStyleCursorValue) -> AzOptionStyleCursorValue,
        pub az_option_style_cursor_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleCursorValue) -> AzString,
        pub az_option_box_shadow_pre_display_item_value_delete: extern "C" fn(_:  &mut AzOptionBoxShadowPreDisplayItemValue),
        pub az_option_box_shadow_pre_display_item_value_deep_copy: extern "C" fn(_:  &AzOptionBoxShadowPreDisplayItemValue) -> AzOptionBoxShadowPreDisplayItemValue,
        pub az_option_box_shadow_pre_display_item_value_fmt_debug: extern "C" fn(_:  &AzOptionBoxShadowPreDisplayItemValue) -> AzString,
        pub az_option_style_border_top_color_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderTopColorValue),
        pub az_option_style_border_top_color_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderTopColorValue) -> AzOptionStyleBorderTopColorValue,
        pub az_option_style_border_top_color_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderTopColorValue) -> AzString,
        pub az_option_style_border_left_color_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderLeftColorValue),
        pub az_option_style_border_left_color_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderLeftColorValue) -> AzOptionStyleBorderLeftColorValue,
        pub az_option_style_border_left_color_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderLeftColorValue) -> AzString,
        pub az_option_style_border_right_color_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderRightColorValue),
        pub az_option_style_border_right_color_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderRightColorValue) -> AzOptionStyleBorderRightColorValue,
        pub az_option_style_border_right_color_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderRightColorValue) -> AzString,
        pub az_option_style_border_bottom_color_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderBottomColorValue),
        pub az_option_style_border_bottom_color_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderBottomColorValue) -> AzOptionStyleBorderBottomColorValue,
        pub az_option_style_border_bottom_color_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderBottomColorValue) -> AzString,
        pub az_option_style_border_top_style_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderTopStyleValue),
        pub az_option_style_border_top_style_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderTopStyleValue) -> AzOptionStyleBorderTopStyleValue,
        pub az_option_style_border_top_style_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderTopStyleValue) -> AzString,
        pub az_option_style_border_left_style_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderLeftStyleValue),
        pub az_option_style_border_left_style_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderLeftStyleValue) -> AzOptionStyleBorderLeftStyleValue,
        pub az_option_style_border_left_style_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderLeftStyleValue) -> AzString,
        pub az_option_style_border_right_style_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderRightStyleValue),
        pub az_option_style_border_right_style_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderRightStyleValue) -> AzOptionStyleBorderRightStyleValue,
        pub az_option_style_border_right_style_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderRightStyleValue) -> AzString,
        pub az_option_style_border_bottom_style_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderBottomStyleValue),
        pub az_option_style_border_bottom_style_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderBottomStyleValue) -> AzOptionStyleBorderBottomStyleValue,
        pub az_option_style_border_bottom_style_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderBottomStyleValue) -> AzString,
        pub az_option_style_border_top_left_radius_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderTopLeftRadiusValue),
        pub az_option_style_border_top_left_radius_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderTopLeftRadiusValue) -> AzOptionStyleBorderTopLeftRadiusValue,
        pub az_option_style_border_top_left_radius_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderTopLeftRadiusValue) -> AzString,
        pub az_option_style_border_top_right_radius_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderTopRightRadiusValue),
        pub az_option_style_border_top_right_radius_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderTopRightRadiusValue) -> AzOptionStyleBorderTopRightRadiusValue,
        pub az_option_style_border_top_right_radius_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderTopRightRadiusValue) -> AzString,
        pub az_option_style_border_bottom_left_radius_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderBottomLeftRadiusValue),
        pub az_option_style_border_bottom_left_radius_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderBottomLeftRadiusValue) -> AzOptionStyleBorderBottomLeftRadiusValue,
        pub az_option_style_border_bottom_left_radius_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderBottomLeftRadiusValue) -> AzString,
        pub az_option_style_border_bottom_right_radius_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderBottomRightRadiusValue),
        pub az_option_style_border_bottom_right_radius_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderBottomRightRadiusValue) -> AzOptionStyleBorderBottomRightRadiusValue,
        pub az_option_style_border_bottom_right_radius_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderBottomRightRadiusValue) -> AzString,
        pub az_option_layout_display_value_delete: extern "C" fn(_:  &mut AzOptionLayoutDisplayValue),
        pub az_option_layout_display_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutDisplayValue) -> AzOptionLayoutDisplayValue,
        pub az_option_layout_display_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutDisplayValue) -> AzString,
        pub az_option_layout_float_value_delete: extern "C" fn(_:  &mut AzOptionLayoutFloatValue),
        pub az_option_layout_float_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutFloatValue) -> AzOptionLayoutFloatValue,
        pub az_option_layout_float_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutFloatValue) -> AzString,
        pub az_option_layout_box_sizing_value_delete: extern "C" fn(_:  &mut AzOptionLayoutBoxSizingValue),
        pub az_option_layout_box_sizing_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutBoxSizingValue) -> AzOptionLayoutBoxSizingValue,
        pub az_option_layout_box_sizing_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutBoxSizingValue) -> AzString,
        pub az_option_layout_width_value_delete: extern "C" fn(_:  &mut AzOptionLayoutWidthValue),
        pub az_option_layout_width_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutWidthValue) -> AzOptionLayoutWidthValue,
        pub az_option_layout_width_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutWidthValue) -> AzString,
        pub az_option_layout_height_value_delete: extern "C" fn(_:  &mut AzOptionLayoutHeightValue),
        pub az_option_layout_height_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutHeightValue) -> AzOptionLayoutHeightValue,
        pub az_option_layout_height_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutHeightValue) -> AzString,
        pub az_option_layout_min_width_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMinWidthValue),
        pub az_option_layout_min_width_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMinWidthValue) -> AzOptionLayoutMinWidthValue,
        pub az_option_layout_min_width_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMinWidthValue) -> AzString,
        pub az_option_layout_min_height_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMinHeightValue),
        pub az_option_layout_min_height_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMinHeightValue) -> AzOptionLayoutMinHeightValue,
        pub az_option_layout_min_height_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMinHeightValue) -> AzString,
        pub az_option_layout_max_width_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMaxWidthValue),
        pub az_option_layout_max_width_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMaxWidthValue) -> AzOptionLayoutMaxWidthValue,
        pub az_option_layout_max_width_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMaxWidthValue) -> AzString,
        pub az_option_layout_max_height_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMaxHeightValue),
        pub az_option_layout_max_height_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMaxHeightValue) -> AzOptionLayoutMaxHeightValue,
        pub az_option_layout_max_height_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMaxHeightValue) -> AzString,
        pub az_option_layout_position_value_delete: extern "C" fn(_:  &mut AzOptionLayoutPositionValue),
        pub az_option_layout_position_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutPositionValue) -> AzOptionLayoutPositionValue,
        pub az_option_layout_position_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutPositionValue) -> AzString,
        pub az_option_layout_top_value_delete: extern "C" fn(_:  &mut AzOptionLayoutTopValue),
        pub az_option_layout_top_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutTopValue) -> AzOptionLayoutTopValue,
        pub az_option_layout_top_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutTopValue) -> AzString,
        pub az_option_layout_bottom_value_delete: extern "C" fn(_:  &mut AzOptionLayoutBottomValue),
        pub az_option_layout_bottom_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutBottomValue) -> AzOptionLayoutBottomValue,
        pub az_option_layout_bottom_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutBottomValue) -> AzString,
        pub az_option_layout_right_value_delete: extern "C" fn(_:  &mut AzOptionLayoutRightValue),
        pub az_option_layout_right_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutRightValue) -> AzOptionLayoutRightValue,
        pub az_option_layout_right_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutRightValue) -> AzString,
        pub az_option_layout_left_value_delete: extern "C" fn(_:  &mut AzOptionLayoutLeftValue),
        pub az_option_layout_left_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutLeftValue) -> AzOptionLayoutLeftValue,
        pub az_option_layout_left_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutLeftValue) -> AzString,
        pub az_option_layout_padding_top_value_delete: extern "C" fn(_:  &mut AzOptionLayoutPaddingTopValue),
        pub az_option_layout_padding_top_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutPaddingTopValue) -> AzOptionLayoutPaddingTopValue,
        pub az_option_layout_padding_top_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutPaddingTopValue) -> AzString,
        pub az_option_layout_padding_bottom_value_delete: extern "C" fn(_:  &mut AzOptionLayoutPaddingBottomValue),
        pub az_option_layout_padding_bottom_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutPaddingBottomValue) -> AzOptionLayoutPaddingBottomValue,
        pub az_option_layout_padding_bottom_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutPaddingBottomValue) -> AzString,
        pub az_option_layout_padding_left_value_delete: extern "C" fn(_:  &mut AzOptionLayoutPaddingLeftValue),
        pub az_option_layout_padding_left_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutPaddingLeftValue) -> AzOptionLayoutPaddingLeftValue,
        pub az_option_layout_padding_left_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutPaddingLeftValue) -> AzString,
        pub az_option_layout_padding_right_value_delete: extern "C" fn(_:  &mut AzOptionLayoutPaddingRightValue),
        pub az_option_layout_padding_right_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutPaddingRightValue) -> AzOptionLayoutPaddingRightValue,
        pub az_option_layout_padding_right_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutPaddingRightValue) -> AzString,
        pub az_option_layout_margin_top_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMarginTopValue),
        pub az_option_layout_margin_top_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMarginTopValue) -> AzOptionLayoutMarginTopValue,
        pub az_option_layout_margin_top_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMarginTopValue) -> AzString,
        pub az_option_layout_margin_bottom_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMarginBottomValue),
        pub az_option_layout_margin_bottom_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMarginBottomValue) -> AzOptionLayoutMarginBottomValue,
        pub az_option_layout_margin_bottom_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMarginBottomValue) -> AzString,
        pub az_option_layout_margin_left_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMarginLeftValue),
        pub az_option_layout_margin_left_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMarginLeftValue) -> AzOptionLayoutMarginLeftValue,
        pub az_option_layout_margin_left_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMarginLeftValue) -> AzString,
        pub az_option_layout_margin_right_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMarginRightValue),
        pub az_option_layout_margin_right_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMarginRightValue) -> AzOptionLayoutMarginRightValue,
        pub az_option_layout_margin_right_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMarginRightValue) -> AzString,
        pub az_option_style_border_top_width_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderTopWidthValue),
        pub az_option_style_border_top_width_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderTopWidthValue) -> AzOptionStyleBorderTopWidthValue,
        pub az_option_style_border_top_width_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderTopWidthValue) -> AzString,
        pub az_option_style_border_left_width_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderLeftWidthValue),
        pub az_option_style_border_left_width_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderLeftWidthValue) -> AzOptionStyleBorderLeftWidthValue,
        pub az_option_style_border_left_width_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderLeftWidthValue) -> AzString,
        pub az_option_style_border_right_width_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderRightWidthValue),
        pub az_option_style_border_right_width_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderRightWidthValue) -> AzOptionStyleBorderRightWidthValue,
        pub az_option_style_border_right_width_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderRightWidthValue) -> AzString,
        pub az_option_style_border_bottom_width_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderBottomWidthValue),
        pub az_option_style_border_bottom_width_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderBottomWidthValue) -> AzOptionStyleBorderBottomWidthValue,
        pub az_option_style_border_bottom_width_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderBottomWidthValue) -> AzString,
        pub az_option_overflow_value_delete: extern "C" fn(_:  &mut AzOptionOverflowValue),
        pub az_option_overflow_value_deep_copy: extern "C" fn(_:  &AzOptionOverflowValue) -> AzOptionOverflowValue,
        pub az_option_overflow_value_fmt_debug: extern "C" fn(_:  &AzOptionOverflowValue) -> AzString,
        pub az_option_layout_direction_value_delete: extern "C" fn(_:  &mut AzOptionLayoutDirectionValue),
        pub az_option_layout_direction_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutDirectionValue) -> AzOptionLayoutDirectionValue,
        pub az_option_layout_direction_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutDirectionValue) -> AzString,
        pub az_option_layout_wrap_value_delete: extern "C" fn(_:  &mut AzOptionLayoutWrapValue),
        pub az_option_layout_wrap_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutWrapValue) -> AzOptionLayoutWrapValue,
        pub az_option_layout_wrap_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutWrapValue) -> AzString,
        pub az_option_layout_flex_grow_value_delete: extern "C" fn(_:  &mut AzOptionLayoutFlexGrowValue),
        pub az_option_layout_flex_grow_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutFlexGrowValue) -> AzOptionLayoutFlexGrowValue,
        pub az_option_layout_flex_grow_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutFlexGrowValue) -> AzString,
        pub az_option_layout_flex_shrink_value_delete: extern "C" fn(_:  &mut AzOptionLayoutFlexShrinkValue),
        pub az_option_layout_flex_shrink_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutFlexShrinkValue) -> AzOptionLayoutFlexShrinkValue,
        pub az_option_layout_flex_shrink_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutFlexShrinkValue) -> AzString,
        pub az_option_layout_justify_content_value_delete: extern "C" fn(_:  &mut AzOptionLayoutJustifyContentValue),
        pub az_option_layout_justify_content_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutJustifyContentValue) -> AzOptionLayoutJustifyContentValue,
        pub az_option_layout_justify_content_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutJustifyContentValue) -> AzString,
        pub az_option_layout_align_items_value_delete: extern "C" fn(_:  &mut AzOptionLayoutAlignItemsValue),
        pub az_option_layout_align_items_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutAlignItemsValue) -> AzOptionLayoutAlignItemsValue,
        pub az_option_layout_align_items_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutAlignItemsValue) -> AzString,
        pub az_option_layout_align_content_value_delete: extern "C" fn(_:  &mut AzOptionLayoutAlignContentValue),
        pub az_option_layout_align_content_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutAlignContentValue) -> AzOptionLayoutAlignContentValue,
        pub az_option_layout_align_content_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutAlignContentValue) -> AzString,
        pub az_option_hover_group_delete: extern "C" fn(_:  &mut AzOptionHoverGroup),
        pub az_option_hover_group_deep_copy: extern "C" fn(_:  &AzOptionHoverGroup) -> AzOptionHoverGroup,
        pub az_option_hover_group_fmt_debug: extern "C" fn(_:  &AzOptionHoverGroup) -> AzString,
        pub az_option_tag_id_delete: extern "C" fn(_:  &mut AzOptionTagId),
        pub az_option_tag_id_deep_copy: extern "C" fn(_:  &AzOptionTagId) -> AzOptionTagId,
        pub az_option_tag_id_fmt_debug: extern "C" fn(_:  &AzOptionTagId) -> AzString,
        pub az_option_duration_delete: extern "C" fn(_:  &mut AzOptionDuration),
        pub az_option_duration_deep_copy: extern "C" fn(_:  &AzOptionDuration) -> AzOptionDuration,
        pub az_option_duration_fmt_debug: extern "C" fn(_:  &AzOptionDuration) -> AzString,
        pub az_option_instant_ptr_delete: extern "C" fn(_:  &mut AzOptionInstantPtr),
        pub az_option_instant_ptr_deep_copy: extern "C" fn(_:  &AzOptionInstantPtr) -> AzOptionInstantPtr,
        pub az_option_instant_ptr_fmt_debug: extern "C" fn(_:  &AzOptionInstantPtr) -> AzString,
        pub az_option_usize_delete: extern "C" fn(_:  &mut AzOptionUsize),
        pub az_option_usize_deep_copy: extern "C" fn(_:  &AzOptionUsize) -> AzOptionUsize,
        pub az_option_usize_fmt_debug: extern "C" fn(_:  &AzOptionUsize) -> AzString,
        pub az_option_u8_vec_ref_delete: extern "C" fn(_:  &mut AzOptionU8VecRef),
        pub az_option_u8_vec_ref_fmt_debug: extern "C" fn(_:  &AzOptionU8VecRef) -> AzString,
        pub az_result_svg_svg_parse_error_delete: extern "C" fn(_:  &mut AzResultSvgSvgParseError),
        pub az_result_svg_svg_parse_error_deep_copy: extern "C" fn(_:  &AzResultSvgSvgParseError) -> AzResultSvgSvgParseError,
        pub az_result_svg_svg_parse_error_fmt_debug: extern "C" fn(_:  &AzResultSvgSvgParseError) -> AzString,
        pub az_result_ref_any_block_error_delete: extern "C" fn(_:  &mut AzResultRefAnyBlockError),
        pub az_result_ref_any_block_error_deep_copy: extern "C" fn(_:  &AzResultRefAnyBlockError) -> AzResultRefAnyBlockError,
        pub az_result_ref_any_block_error_fmt_debug: extern "C" fn(_:  &AzResultRefAnyBlockError) -> AzString,
        pub az_svg_parse_error_delete: extern "C" fn(_:  &mut AzSvgParseError),
        pub az_svg_parse_error_deep_copy: extern "C" fn(_:  &AzSvgParseError) -> AzSvgParseError,
        pub az_svg_parse_error_fmt_debug: extern "C" fn(_:  &AzSvgParseError) -> AzString,
        pub az_xml_error_delete: extern "C" fn(_:  &mut AzXmlError),
        pub az_xml_error_deep_copy: extern "C" fn(_:  &AzXmlError) -> AzXmlError,
        pub az_xml_error_fmt_debug: extern "C" fn(_:  &AzXmlError) -> AzString,
        pub az_duplicated_namespace_error_delete: extern "C" fn(_:  &mut AzDuplicatedNamespaceError),
        pub az_duplicated_namespace_error_deep_copy: extern "C" fn(_:  &AzDuplicatedNamespaceError) -> AzDuplicatedNamespaceError,
        pub az_duplicated_namespace_error_fmt_debug: extern "C" fn(_:  &AzDuplicatedNamespaceError) -> AzString,
        pub az_unknown_namespace_error_delete: extern "C" fn(_:  &mut AzUnknownNamespaceError),
        pub az_unknown_namespace_error_deep_copy: extern "C" fn(_:  &AzUnknownNamespaceError) -> AzUnknownNamespaceError,
        pub az_unknown_namespace_error_fmt_debug: extern "C" fn(_:  &AzUnknownNamespaceError) -> AzString,
        pub az_unexpected_close_tag_error_delete: extern "C" fn(_:  &mut AzUnexpectedCloseTagError),
        pub az_unexpected_close_tag_error_deep_copy: extern "C" fn(_:  &AzUnexpectedCloseTagError) -> AzUnexpectedCloseTagError,
        pub az_unexpected_close_tag_error_fmt_debug: extern "C" fn(_:  &AzUnexpectedCloseTagError) -> AzString,
        pub az_unknown_entity_reference_error_delete: extern "C" fn(_:  &mut AzUnknownEntityReferenceError),
        pub az_unknown_entity_reference_error_deep_copy: extern "C" fn(_:  &AzUnknownEntityReferenceError) -> AzUnknownEntityReferenceError,
        pub az_unknown_entity_reference_error_fmt_debug: extern "C" fn(_:  &AzUnknownEntityReferenceError) -> AzString,
        pub az_duplicated_attribute_error_delete: extern "C" fn(_:  &mut AzDuplicatedAttributeError),
        pub az_duplicated_attribute_error_deep_copy: extern "C" fn(_:  &AzDuplicatedAttributeError) -> AzDuplicatedAttributeError,
        pub az_duplicated_attribute_error_fmt_debug: extern "C" fn(_:  &AzDuplicatedAttributeError) -> AzString,
        pub az_xml_parse_error_delete: extern "C" fn(_:  &mut AzXmlParseError),
        pub az_xml_parse_error_deep_copy: extern "C" fn(_:  &AzXmlParseError) -> AzXmlParseError,
        pub az_xml_parse_error_fmt_debug: extern "C" fn(_:  &AzXmlParseError) -> AzString,
        pub az_xml_text_error_delete: extern "C" fn(_:  &mut AzXmlTextError),
        pub az_xml_text_error_deep_copy: extern "C" fn(_:  &AzXmlTextError) -> AzXmlTextError,
        pub az_xml_text_error_fmt_debug: extern "C" fn(_:  &AzXmlTextError) -> AzString,
        pub az_xml_stream_error_delete: extern "C" fn(_:  &mut AzXmlStreamError),
        pub az_xml_stream_error_deep_copy: extern "C" fn(_:  &AzXmlStreamError) -> AzXmlStreamError,
        pub az_xml_stream_error_fmt_debug: extern "C" fn(_:  &AzXmlStreamError) -> AzString,
        pub az_non_xml_char_error_delete: extern "C" fn(_:  &mut AzNonXmlCharError),
        pub az_non_xml_char_error_deep_copy: extern "C" fn(_:  &AzNonXmlCharError) -> AzNonXmlCharError,
        pub az_non_xml_char_error_fmt_debug: extern "C" fn(_:  &AzNonXmlCharError) -> AzString,
        pub az_invalid_char_error_delete: extern "C" fn(_:  &mut AzInvalidCharError),
        pub az_invalid_char_error_deep_copy: extern "C" fn(_:  &AzInvalidCharError) -> AzInvalidCharError,
        pub az_invalid_char_error_fmt_debug: extern "C" fn(_:  &AzInvalidCharError) -> AzString,
        pub az_invalid_char_multiple_error_delete: extern "C" fn(_:  &mut AzInvalidCharMultipleError),
        pub az_invalid_char_multiple_error_deep_copy: extern "C" fn(_:  &AzInvalidCharMultipleError) -> AzInvalidCharMultipleError,
        pub az_invalid_char_multiple_error_fmt_debug: extern "C" fn(_:  &AzInvalidCharMultipleError) -> AzString,
        pub az_invalid_quote_error_delete: extern "C" fn(_:  &mut AzInvalidQuoteError),
        pub az_invalid_quote_error_deep_copy: extern "C" fn(_:  &AzInvalidQuoteError) -> AzInvalidQuoteError,
        pub az_invalid_quote_error_fmt_debug: extern "C" fn(_:  &AzInvalidQuoteError) -> AzString,
        pub az_invalid_space_error_delete: extern "C" fn(_:  &mut AzInvalidSpaceError),
        pub az_invalid_space_error_deep_copy: extern "C" fn(_:  &AzInvalidSpaceError) -> AzInvalidSpaceError,
        pub az_invalid_space_error_fmt_debug: extern "C" fn(_:  &AzInvalidSpaceError) -> AzString,
        pub az_invalid_string_error_delete: extern "C" fn(_:  &mut AzInvalidStringError),
        pub az_invalid_string_error_deep_copy: extern "C" fn(_:  &AzInvalidStringError) -> AzInvalidStringError,
        pub az_invalid_string_error_fmt_debug: extern "C" fn(_:  &AzInvalidStringError) -> AzString,
        pub az_xml_text_pos_delete: extern "C" fn(_:  &mut AzXmlTextPos),
        pub az_xml_text_pos_deep_copy: extern "C" fn(_:  &AzXmlTextPos) -> AzXmlTextPos,
        pub az_xml_text_pos_fmt_debug: extern "C" fn(_:  &AzXmlTextPos) -> AzString,
        pub az_instant_ptr_now: extern "C" fn() -> AzInstantPtr,
        pub az_instant_ptr_delete: extern "C" fn(_:  &mut AzInstantPtr),
        pub az_instant_ptr_fmt_debug: extern "C" fn(_:  &AzInstantPtr) -> AzString,
        pub az_instant_ptr_partial_eq: extern "C" fn(_:  &AzInstantPtr, _:  &AzInstantPtr) -> bool,
        pub az_instant_ptr_partial_cmp: extern "C" fn(_:  &AzInstantPtr, _:  &AzInstantPtr) -> u8,
        pub az_instant_ptr_cmp: extern "C" fn(_:  &AzInstantPtr, _:  &AzInstantPtr) -> u8,
        pub az_duration_delete: extern "C" fn(_:  &mut AzDuration),
        pub az_duration_deep_copy: extern "C" fn(_:  &AzDuration) -> AzDuration,
        pub az_duration_fmt_debug: extern "C" fn(_:  &AzDuration) -> AzString,
        pub az_app_config_ptr_default: extern "C" fn() -> AzAppConfigPtr,
        pub az_app_config_ptr_delete: extern "C" fn(_:  &mut AzAppConfigPtr),
        pub az_app_config_ptr_fmt_debug: extern "C" fn(_:  &AzAppConfigPtr) -> AzString,
        pub az_app_ptr_new: extern "C" fn(_:  AzRefAny, _:  AzAppConfigPtr) -> AzAppPtr,
        pub az_app_ptr_run: extern "C" fn(_:  AzAppPtr, _:  AzWindowCreateOptions),
        pub az_app_ptr_delete: extern "C" fn(_:  &mut AzAppPtr),
        pub az_app_ptr_fmt_debug: extern "C" fn(_:  &AzAppPtr) -> AzString,
        pub az_node_id_delete: extern "C" fn(_:  &mut AzNodeId),
        pub az_node_id_deep_copy: extern "C" fn(_:  &AzNodeId) -> AzNodeId,
        pub az_node_id_fmt_debug: extern "C" fn(_:  &AzNodeId) -> AzString,
        pub az_dom_id_delete: extern "C" fn(_:  &mut AzDomId),
        pub az_dom_id_deep_copy: extern "C" fn(_:  &AzDomId) -> AzDomId,
        pub az_dom_id_fmt_debug: extern "C" fn(_:  &AzDomId) -> AzString,
        pub az_dom_node_id_delete: extern "C" fn(_:  &mut AzDomNodeId),
        pub az_dom_node_id_deep_copy: extern "C" fn(_:  &AzDomNodeId) -> AzDomNodeId,
        pub az_dom_node_id_fmt_debug: extern "C" fn(_:  &AzDomNodeId) -> AzString,
        pub az_hidpi_adjusted_bounds_get_logical_size: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzLogicalSize,
        pub az_hidpi_adjusted_bounds_get_physical_size: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzPhysicalSizeU32,
        pub az_hidpi_adjusted_bounds_get_hidpi_factor: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> f32,
        pub az_hidpi_adjusted_bounds_delete: extern "C" fn(_:  &mut AzHidpiAdjustedBounds),
        pub az_hidpi_adjusted_bounds_deep_copy: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzHidpiAdjustedBounds,
        pub az_hidpi_adjusted_bounds_fmt_debug: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzString,
        pub az_layout_callback_delete: extern "C" fn(_:  &mut AzLayoutCallback),
        pub az_layout_callback_deep_copy: extern "C" fn(_:  &AzLayoutCallback) -> AzLayoutCallback,
        pub az_layout_callback_fmt_debug: extern "C" fn(_:  &AzLayoutCallback) -> AzString,
        pub az_callback_delete: extern "C" fn(_:  &mut AzCallback),
        pub az_callback_deep_copy: extern "C" fn(_:  &AzCallback) -> AzCallback,
        pub az_callback_fmt_debug: extern "C" fn(_:  &AzCallback) -> AzString,
        pub az_callback_partial_eq: extern "C" fn(_:  &AzCallback, _:  &AzCallback) -> bool,
        pub az_callback_partial_cmp: extern "C" fn(_:  &AzCallback, _:  &AzCallback) -> u8,
        pub az_callback_cmp: extern "C" fn(_:  &AzCallback, _:  &AzCallback) -> u8,
        pub az_callback_hash: extern "C" fn(_:  &AzCallback) -> u64,
        pub az_callback_info_ptr_get_state: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzRefAny,
        pub az_callback_info_ptr_get_keyboard_state: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzKeyboardState,
        pub az_callback_info_ptr_get_mouse_state: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzMouseState,
        pub az_callback_info_ptr_set_window_state: extern "C" fn(_:  &mut AzCallbackInfoPtr, _:  AzWindowState),
        pub az_callback_info_ptr_delete: extern "C" fn(_:  &mut AzCallbackInfoPtr),
        pub az_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzString,
        pub az_update_screen_delete: extern "C" fn(_:  &mut AzUpdateScreen),
        pub az_update_screen_deep_copy: extern "C" fn(_:  &AzUpdateScreen) -> AzUpdateScreen,
        pub az_update_screen_fmt_debug: extern "C" fn(_:  &AzUpdateScreen) -> AzString,
        pub az_i_frame_callback_delete: extern "C" fn(_:  &mut AzIFrameCallback),
        pub az_i_frame_callback_deep_copy: extern "C" fn(_:  &AzIFrameCallback) -> AzIFrameCallback,
        pub az_i_frame_callback_fmt_debug: extern "C" fn(_:  &AzIFrameCallback) -> AzString,
        pub az_i_frame_callback_info_ptr_get_state: extern "C" fn(_:  &AzIFrameCallbackInfoPtr) -> AzRefAny,
        pub az_i_frame_callback_info_ptr_get_bounds: extern "C" fn(_:  &AzIFrameCallbackInfoPtr) -> AzHidpiAdjustedBounds,
        pub az_i_frame_callback_info_ptr_delete: extern "C" fn(_:  &mut AzIFrameCallbackInfoPtr),
        pub az_i_frame_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzIFrameCallbackInfoPtr) -> AzString,
        pub az_i_frame_callback_return_delete: extern "C" fn(_:  &mut AzIFrameCallbackReturn),
        pub az_i_frame_callback_return_deep_copy: extern "C" fn(_:  &AzIFrameCallbackReturn) -> AzIFrameCallbackReturn,
        pub az_i_frame_callback_return_fmt_debug: extern "C" fn(_:  &AzIFrameCallbackReturn) -> AzString,
        pub az_gl_callback_delete: extern "C" fn(_:  &mut AzGlCallback),
        pub az_gl_callback_deep_copy: extern "C" fn(_:  &AzGlCallback) -> AzGlCallback,
        pub az_gl_callback_fmt_debug: extern "C" fn(_:  &AzGlCallback) -> AzString,
        pub az_gl_callback_info_ptr_get_state: extern "C" fn(_:  &AzGlCallbackInfoPtr) -> AzRefAny,
        pub az_gl_callback_info_ptr_get_gl_context: extern "C" fn(_:  &AzGlCallbackInfoPtr) -> AzGlContextPtr,
        pub az_gl_callback_info_ptr_get_bounds: extern "C" fn(_:  &AzGlCallbackInfoPtr) -> AzHidpiAdjustedBounds,
        pub az_gl_callback_info_ptr_delete: extern "C" fn(_:  &mut AzGlCallbackInfoPtr),
        pub az_gl_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzGlCallbackInfoPtr) -> AzString,
        pub az_gl_callback_return_delete: extern "C" fn(_:  &mut AzGlCallbackReturn),
        pub az_gl_callback_return_fmt_debug: extern "C" fn(_:  &AzGlCallbackReturn) -> AzString,
        pub az_timer_callback_delete: extern "C" fn(_:  &mut AzTimerCallback),
        pub az_timer_callback_deep_copy: extern "C" fn(_:  &AzTimerCallback) -> AzTimerCallback,
        pub az_timer_callback_fmt_debug: extern "C" fn(_:  &AzTimerCallback) -> AzString,
        pub az_timer_callback_type_ptr_delete: extern "C" fn(_:  &mut AzTimerCallbackTypePtr),
        pub az_timer_callback_type_ptr_fmt_debug: extern "C" fn(_:  &AzTimerCallbackTypePtr) -> AzString,
        pub az_timer_callback_info_ptr_get_state: extern "C" fn(_:  &AzTimerCallbackInfoPtr) -> AzRefAny,
        pub az_timer_callback_info_ptr_delete: extern "C" fn(_:  &mut AzTimerCallbackInfoPtr),
        pub az_timer_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzTimerCallbackInfoPtr) -> AzString,
        pub az_timer_callback_return_delete: extern "C" fn(_:  &mut AzTimerCallbackReturn),
        pub az_timer_callback_return_deep_copy: extern "C" fn(_:  &AzTimerCallbackReturn) -> AzTimerCallbackReturn,
        pub az_timer_callback_return_fmt_debug: extern "C" fn(_:  &AzTimerCallbackReturn) -> AzString,
        pub az_ref_any_sharing_info_can_be_shared: extern "C" fn(_:  &AzRefAnySharingInfo) -> bool,
        pub az_ref_any_sharing_info_can_be_shared_mut: extern "C" fn(_:  &AzRefAnySharingInfo) -> bool,
        pub az_ref_any_sharing_info_increase_ref: extern "C" fn(_:  &mut AzRefAnySharingInfo),
        pub az_ref_any_sharing_info_decrease_ref: extern "C" fn(_:  &mut AzRefAnySharingInfo),
        pub az_ref_any_sharing_info_increase_refmut: extern "C" fn(_:  &mut AzRefAnySharingInfo),
        pub az_ref_any_sharing_info_decrease_refmut: extern "C" fn(_:  &mut AzRefAnySharingInfo),
        pub az_ref_any_sharing_info_delete: extern "C" fn(_:  &mut AzRefAnySharingInfo),
        pub az_ref_any_sharing_info_fmt_debug: extern "C" fn(_:  &AzRefAnySharingInfo) -> AzString,
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
        pub az_ref_any_fmt_debug: extern "C" fn(_:  &AzRefAny) -> AzString,
        pub az_ref_any_partial_eq: extern "C" fn(_:  &AzRefAny, _:  &AzRefAny) -> bool,
        pub az_ref_any_partial_cmp: extern "C" fn(_:  &AzRefAny, _:  &AzRefAny) -> u8,
        pub az_ref_any_cmp: extern "C" fn(_:  &AzRefAny, _:  &AzRefAny) -> u8,
        pub az_ref_any_hash: extern "C" fn(_:  &AzRefAny) -> u64,
        pub az_layout_info_ptr_get_gl_context: extern "C" fn(_:  &AzLayoutInfoPtr) -> AzGlContextPtr,
        pub az_layout_info_ptr_delete: extern "C" fn(_:  &mut AzLayoutInfoPtr),
        pub az_layout_info_ptr_fmt_debug: extern "C" fn(_:  &AzLayoutInfoPtr) -> AzString,
        pub az_css_rule_block_delete: extern "C" fn(_:  &mut AzCssRuleBlock),
        pub az_css_rule_block_deep_copy: extern "C" fn(_:  &AzCssRuleBlock) -> AzCssRuleBlock,
        pub az_css_rule_block_fmt_debug: extern "C" fn(_:  &AzCssRuleBlock) -> AzString,
        pub az_css_declaration_delete: extern "C" fn(_:  &mut AzCssDeclaration),
        pub az_css_declaration_deep_copy: extern "C" fn(_:  &AzCssDeclaration) -> AzCssDeclaration,
        pub az_css_declaration_fmt_debug: extern "C" fn(_:  &AzCssDeclaration) -> AzString,
        pub az_dynamic_css_property_delete: extern "C" fn(_:  &mut AzDynamicCssProperty),
        pub az_dynamic_css_property_deep_copy: extern "C" fn(_:  &AzDynamicCssProperty) -> AzDynamicCssProperty,
        pub az_dynamic_css_property_fmt_debug: extern "C" fn(_:  &AzDynamicCssProperty) -> AzString,
        pub az_css_path_delete: extern "C" fn(_:  &mut AzCssPath),
        pub az_css_path_deep_copy: extern "C" fn(_:  &AzCssPath) -> AzCssPath,
        pub az_css_path_fmt_debug: extern "C" fn(_:  &AzCssPath) -> AzString,
        pub az_css_path_selector_delete: extern "C" fn(_:  &mut AzCssPathSelector),
        pub az_css_path_selector_deep_copy: extern "C" fn(_:  &AzCssPathSelector) -> AzCssPathSelector,
        pub az_css_path_selector_fmt_debug: extern "C" fn(_:  &AzCssPathSelector) -> AzString,
        pub az_node_type_path_delete: extern "C" fn(_:  &mut AzNodeTypePath),
        pub az_node_type_path_deep_copy: extern "C" fn(_:  &AzNodeTypePath) -> AzNodeTypePath,
        pub az_node_type_path_fmt_debug: extern "C" fn(_:  &AzNodeTypePath) -> AzString,
        pub az_css_path_pseudo_selector_delete: extern "C" fn(_:  &mut AzCssPathPseudoSelector),
        pub az_css_path_pseudo_selector_deep_copy: extern "C" fn(_:  &AzCssPathPseudoSelector) -> AzCssPathPseudoSelector,
        pub az_css_path_pseudo_selector_fmt_debug: extern "C" fn(_:  &AzCssPathPseudoSelector) -> AzString,
        pub az_css_nth_child_selector_delete: extern "C" fn(_:  &mut AzCssNthChildSelector),
        pub az_css_nth_child_selector_deep_copy: extern "C" fn(_:  &AzCssNthChildSelector) -> AzCssNthChildSelector,
        pub az_css_nth_child_selector_fmt_debug: extern "C" fn(_:  &AzCssNthChildSelector) -> AzString,
        pub az_css_nth_child_pattern_delete: extern "C" fn(_:  &mut AzCssNthChildPattern),
        pub az_css_nth_child_pattern_deep_copy: extern "C" fn(_:  &AzCssNthChildPattern) -> AzCssNthChildPattern,
        pub az_css_nth_child_pattern_fmt_debug: extern "C" fn(_:  &AzCssNthChildPattern) -> AzString,
        pub az_stylesheet_delete: extern "C" fn(_:  &mut AzStylesheet),
        pub az_stylesheet_deep_copy: extern "C" fn(_:  &AzStylesheet) -> AzStylesheet,
        pub az_stylesheet_fmt_debug: extern "C" fn(_:  &AzStylesheet) -> AzString,
        pub az_css_native: extern "C" fn() -> AzCss,
        pub az_css_empty: extern "C" fn() -> AzCss,
        pub az_css_from_string: extern "C" fn(_:  AzString) -> AzCss,
        pub az_css_override_native: extern "C" fn(_:  AzString) -> AzCss,
        pub az_css_delete: extern "C" fn(_:  &mut AzCss),
        pub az_css_deep_copy: extern "C" fn(_:  &AzCss) -> AzCss,
        pub az_css_fmt_debug: extern "C" fn(_:  &AzCss) -> AzString,
        pub az_color_u_delete: extern "C" fn(_:  &mut AzColorU),
        pub az_color_u_deep_copy: extern "C" fn(_:  &AzColorU) -> AzColorU,
        pub az_color_u_fmt_debug: extern "C" fn(_:  &AzColorU) -> AzString,
        pub az_size_metric_delete: extern "C" fn(_:  &mut AzSizeMetric),
        pub az_size_metric_deep_copy: extern "C" fn(_:  &AzSizeMetric) -> AzSizeMetric,
        pub az_size_metric_fmt_debug: extern "C" fn(_:  &AzSizeMetric) -> AzString,
        pub az_float_value_delete: extern "C" fn(_:  &mut AzFloatValue),
        pub az_float_value_deep_copy: extern "C" fn(_:  &AzFloatValue) -> AzFloatValue,
        pub az_float_value_fmt_debug: extern "C" fn(_:  &AzFloatValue) -> AzString,
        pub az_pixel_value_delete: extern "C" fn(_:  &mut AzPixelValue),
        pub az_pixel_value_deep_copy: extern "C" fn(_:  &AzPixelValue) -> AzPixelValue,
        pub az_pixel_value_fmt_debug: extern "C" fn(_:  &AzPixelValue) -> AzString,
        pub az_pixel_value_no_percent_delete: extern "C" fn(_:  &mut AzPixelValueNoPercent),
        pub az_pixel_value_no_percent_deep_copy: extern "C" fn(_:  &AzPixelValueNoPercent) -> AzPixelValueNoPercent,
        pub az_pixel_value_no_percent_fmt_debug: extern "C" fn(_:  &AzPixelValueNoPercent) -> AzString,
        pub az_box_shadow_clip_mode_delete: extern "C" fn(_:  &mut AzBoxShadowClipMode),
        pub az_box_shadow_clip_mode_deep_copy: extern "C" fn(_:  &AzBoxShadowClipMode) -> AzBoxShadowClipMode,
        pub az_box_shadow_clip_mode_fmt_debug: extern "C" fn(_:  &AzBoxShadowClipMode) -> AzString,
        pub az_box_shadow_pre_display_item_delete: extern "C" fn(_:  &mut AzBoxShadowPreDisplayItem),
        pub az_box_shadow_pre_display_item_deep_copy: extern "C" fn(_:  &AzBoxShadowPreDisplayItem) -> AzBoxShadowPreDisplayItem,
        pub az_box_shadow_pre_display_item_fmt_debug: extern "C" fn(_:  &AzBoxShadowPreDisplayItem) -> AzString,
        pub az_layout_align_content_delete: extern "C" fn(_:  &mut AzLayoutAlignContent),
        pub az_layout_align_content_deep_copy: extern "C" fn(_:  &AzLayoutAlignContent) -> AzLayoutAlignContent,
        pub az_layout_align_content_fmt_debug: extern "C" fn(_:  &AzLayoutAlignContent) -> AzString,
        pub az_layout_align_items_delete: extern "C" fn(_:  &mut AzLayoutAlignItems),
        pub az_layout_align_items_deep_copy: extern "C" fn(_:  &AzLayoutAlignItems) -> AzLayoutAlignItems,
        pub az_layout_align_items_fmt_debug: extern "C" fn(_:  &AzLayoutAlignItems) -> AzString,
        pub az_layout_bottom_delete: extern "C" fn(_:  &mut AzLayoutBottom),
        pub az_layout_bottom_deep_copy: extern "C" fn(_:  &AzLayoutBottom) -> AzLayoutBottom,
        pub az_layout_bottom_fmt_debug: extern "C" fn(_:  &AzLayoutBottom) -> AzString,
        pub az_layout_box_sizing_delete: extern "C" fn(_:  &mut AzLayoutBoxSizing),
        pub az_layout_box_sizing_deep_copy: extern "C" fn(_:  &AzLayoutBoxSizing) -> AzLayoutBoxSizing,
        pub az_layout_box_sizing_fmt_debug: extern "C" fn(_:  &AzLayoutBoxSizing) -> AzString,
        pub az_layout_direction_delete: extern "C" fn(_:  &mut AzLayoutDirection),
        pub az_layout_direction_deep_copy: extern "C" fn(_:  &AzLayoutDirection) -> AzLayoutDirection,
        pub az_layout_direction_fmt_debug: extern "C" fn(_:  &AzLayoutDirection) -> AzString,
        pub az_layout_display_delete: extern "C" fn(_:  &mut AzLayoutDisplay),
        pub az_layout_display_deep_copy: extern "C" fn(_:  &AzLayoutDisplay) -> AzLayoutDisplay,
        pub az_layout_display_fmt_debug: extern "C" fn(_:  &AzLayoutDisplay) -> AzString,
        pub az_layout_flex_grow_delete: extern "C" fn(_:  &mut AzLayoutFlexGrow),
        pub az_layout_flex_grow_deep_copy: extern "C" fn(_:  &AzLayoutFlexGrow) -> AzLayoutFlexGrow,
        pub az_layout_flex_grow_fmt_debug: extern "C" fn(_:  &AzLayoutFlexGrow) -> AzString,
        pub az_layout_flex_shrink_delete: extern "C" fn(_:  &mut AzLayoutFlexShrink),
        pub az_layout_flex_shrink_deep_copy: extern "C" fn(_:  &AzLayoutFlexShrink) -> AzLayoutFlexShrink,
        pub az_layout_flex_shrink_fmt_debug: extern "C" fn(_:  &AzLayoutFlexShrink) -> AzString,
        pub az_layout_float_delete: extern "C" fn(_:  &mut AzLayoutFloat),
        pub az_layout_float_deep_copy: extern "C" fn(_:  &AzLayoutFloat) -> AzLayoutFloat,
        pub az_layout_float_fmt_debug: extern "C" fn(_:  &AzLayoutFloat) -> AzString,
        pub az_layout_height_delete: extern "C" fn(_:  &mut AzLayoutHeight),
        pub az_layout_height_deep_copy: extern "C" fn(_:  &AzLayoutHeight) -> AzLayoutHeight,
        pub az_layout_height_fmt_debug: extern "C" fn(_:  &AzLayoutHeight) -> AzString,
        pub az_layout_justify_content_delete: extern "C" fn(_:  &mut AzLayoutJustifyContent),
        pub az_layout_justify_content_deep_copy: extern "C" fn(_:  &AzLayoutJustifyContent) -> AzLayoutJustifyContent,
        pub az_layout_justify_content_fmt_debug: extern "C" fn(_:  &AzLayoutJustifyContent) -> AzString,
        pub az_layout_left_delete: extern "C" fn(_:  &mut AzLayoutLeft),
        pub az_layout_left_deep_copy: extern "C" fn(_:  &AzLayoutLeft) -> AzLayoutLeft,
        pub az_layout_left_fmt_debug: extern "C" fn(_:  &AzLayoutLeft) -> AzString,
        pub az_layout_margin_bottom_delete: extern "C" fn(_:  &mut AzLayoutMarginBottom),
        pub az_layout_margin_bottom_deep_copy: extern "C" fn(_:  &AzLayoutMarginBottom) -> AzLayoutMarginBottom,
        pub az_layout_margin_bottom_fmt_debug: extern "C" fn(_:  &AzLayoutMarginBottom) -> AzString,
        pub az_layout_margin_left_delete: extern "C" fn(_:  &mut AzLayoutMarginLeft),
        pub az_layout_margin_left_deep_copy: extern "C" fn(_:  &AzLayoutMarginLeft) -> AzLayoutMarginLeft,
        pub az_layout_margin_left_fmt_debug: extern "C" fn(_:  &AzLayoutMarginLeft) -> AzString,
        pub az_layout_margin_right_delete: extern "C" fn(_:  &mut AzLayoutMarginRight),
        pub az_layout_margin_right_deep_copy: extern "C" fn(_:  &AzLayoutMarginRight) -> AzLayoutMarginRight,
        pub az_layout_margin_right_fmt_debug: extern "C" fn(_:  &AzLayoutMarginRight) -> AzString,
        pub az_layout_margin_top_delete: extern "C" fn(_:  &mut AzLayoutMarginTop),
        pub az_layout_margin_top_deep_copy: extern "C" fn(_:  &AzLayoutMarginTop) -> AzLayoutMarginTop,
        pub az_layout_margin_top_fmt_debug: extern "C" fn(_:  &AzLayoutMarginTop) -> AzString,
        pub az_layout_max_height_delete: extern "C" fn(_:  &mut AzLayoutMaxHeight),
        pub az_layout_max_height_deep_copy: extern "C" fn(_:  &AzLayoutMaxHeight) -> AzLayoutMaxHeight,
        pub az_layout_max_height_fmt_debug: extern "C" fn(_:  &AzLayoutMaxHeight) -> AzString,
        pub az_layout_max_width_delete: extern "C" fn(_:  &mut AzLayoutMaxWidth),
        pub az_layout_max_width_deep_copy: extern "C" fn(_:  &AzLayoutMaxWidth) -> AzLayoutMaxWidth,
        pub az_layout_max_width_fmt_debug: extern "C" fn(_:  &AzLayoutMaxWidth) -> AzString,
        pub az_layout_min_height_delete: extern "C" fn(_:  &mut AzLayoutMinHeight),
        pub az_layout_min_height_deep_copy: extern "C" fn(_:  &AzLayoutMinHeight) -> AzLayoutMinHeight,
        pub az_layout_min_height_fmt_debug: extern "C" fn(_:  &AzLayoutMinHeight) -> AzString,
        pub az_layout_min_width_delete: extern "C" fn(_:  &mut AzLayoutMinWidth),
        pub az_layout_min_width_deep_copy: extern "C" fn(_:  &AzLayoutMinWidth) -> AzLayoutMinWidth,
        pub az_layout_min_width_fmt_debug: extern "C" fn(_:  &AzLayoutMinWidth) -> AzString,
        pub az_layout_padding_bottom_delete: extern "C" fn(_:  &mut AzLayoutPaddingBottom),
        pub az_layout_padding_bottom_deep_copy: extern "C" fn(_:  &AzLayoutPaddingBottom) -> AzLayoutPaddingBottom,
        pub az_layout_padding_bottom_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingBottom) -> AzString,
        pub az_layout_padding_left_delete: extern "C" fn(_:  &mut AzLayoutPaddingLeft),
        pub az_layout_padding_left_deep_copy: extern "C" fn(_:  &AzLayoutPaddingLeft) -> AzLayoutPaddingLeft,
        pub az_layout_padding_left_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingLeft) -> AzString,
        pub az_layout_padding_right_delete: extern "C" fn(_:  &mut AzLayoutPaddingRight),
        pub az_layout_padding_right_deep_copy: extern "C" fn(_:  &AzLayoutPaddingRight) -> AzLayoutPaddingRight,
        pub az_layout_padding_right_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingRight) -> AzString,
        pub az_layout_padding_top_delete: extern "C" fn(_:  &mut AzLayoutPaddingTop),
        pub az_layout_padding_top_deep_copy: extern "C" fn(_:  &AzLayoutPaddingTop) -> AzLayoutPaddingTop,
        pub az_layout_padding_top_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingTop) -> AzString,
        pub az_layout_position_delete: extern "C" fn(_:  &mut AzLayoutPosition),
        pub az_layout_position_deep_copy: extern "C" fn(_:  &AzLayoutPosition) -> AzLayoutPosition,
        pub az_layout_position_fmt_debug: extern "C" fn(_:  &AzLayoutPosition) -> AzString,
        pub az_layout_right_delete: extern "C" fn(_:  &mut AzLayoutRight),
        pub az_layout_right_deep_copy: extern "C" fn(_:  &AzLayoutRight) -> AzLayoutRight,
        pub az_layout_right_fmt_debug: extern "C" fn(_:  &AzLayoutRight) -> AzString,
        pub az_layout_top_delete: extern "C" fn(_:  &mut AzLayoutTop),
        pub az_layout_top_deep_copy: extern "C" fn(_:  &AzLayoutTop) -> AzLayoutTop,
        pub az_layout_top_fmt_debug: extern "C" fn(_:  &AzLayoutTop) -> AzString,
        pub az_layout_width_delete: extern "C" fn(_:  &mut AzLayoutWidth),
        pub az_layout_width_deep_copy: extern "C" fn(_:  &AzLayoutWidth) -> AzLayoutWidth,
        pub az_layout_width_fmt_debug: extern "C" fn(_:  &AzLayoutWidth) -> AzString,
        pub az_layout_wrap_delete: extern "C" fn(_:  &mut AzLayoutWrap),
        pub az_layout_wrap_deep_copy: extern "C" fn(_:  &AzLayoutWrap) -> AzLayoutWrap,
        pub az_layout_wrap_fmt_debug: extern "C" fn(_:  &AzLayoutWrap) -> AzString,
        pub az_overflow_delete: extern "C" fn(_:  &mut AzOverflow),
        pub az_overflow_deep_copy: extern "C" fn(_:  &AzOverflow) -> AzOverflow,
        pub az_overflow_fmt_debug: extern "C" fn(_:  &AzOverflow) -> AzString,
        pub az_percentage_value_delete: extern "C" fn(_:  &mut AzPercentageValue),
        pub az_percentage_value_deep_copy: extern "C" fn(_:  &AzPercentageValue) -> AzPercentageValue,
        pub az_percentage_value_fmt_debug: extern "C" fn(_:  &AzPercentageValue) -> AzString,
        pub az_gradient_stop_pre_delete: extern "C" fn(_:  &mut AzGradientStopPre),
        pub az_gradient_stop_pre_deep_copy: extern "C" fn(_:  &AzGradientStopPre) -> AzGradientStopPre,
        pub az_gradient_stop_pre_fmt_debug: extern "C" fn(_:  &AzGradientStopPre) -> AzString,
        pub az_gradient_stop_pre_partial_eq: extern "C" fn(_:  &AzGradientStopPre, _:  &AzGradientStopPre) -> bool,
        pub az_gradient_stop_pre_partial_cmp: extern "C" fn(_:  &AzGradientStopPre, _:  &AzGradientStopPre) -> u8,
        pub az_gradient_stop_pre_cmp: extern "C" fn(_:  &AzGradientStopPre, _:  &AzGradientStopPre) -> u8,
        pub az_gradient_stop_pre_hash: extern "C" fn(_:  &AzGradientStopPre) -> u64,
        pub az_direction_corner_delete: extern "C" fn(_:  &mut AzDirectionCorner),
        pub az_direction_corner_deep_copy: extern "C" fn(_:  &AzDirectionCorner) -> AzDirectionCorner,
        pub az_direction_corner_fmt_debug: extern "C" fn(_:  &AzDirectionCorner) -> AzString,
        pub az_direction_corners_delete: extern "C" fn(_:  &mut AzDirectionCorners),
        pub az_direction_corners_deep_copy: extern "C" fn(_:  &AzDirectionCorners) -> AzDirectionCorners,
        pub az_direction_corners_fmt_debug: extern "C" fn(_:  &AzDirectionCorners) -> AzString,
        pub az_direction_delete: extern "C" fn(_:  &mut AzDirection),
        pub az_direction_deep_copy: extern "C" fn(_:  &AzDirection) -> AzDirection,
        pub az_direction_fmt_debug: extern "C" fn(_:  &AzDirection) -> AzString,
        pub az_extend_mode_delete: extern "C" fn(_:  &mut AzExtendMode),
        pub az_extend_mode_deep_copy: extern "C" fn(_:  &AzExtendMode) -> AzExtendMode,
        pub az_extend_mode_fmt_debug: extern "C" fn(_:  &AzExtendMode) -> AzString,
        pub az_linear_gradient_delete: extern "C" fn(_:  &mut AzLinearGradient),
        pub az_linear_gradient_deep_copy: extern "C" fn(_:  &AzLinearGradient) -> AzLinearGradient,
        pub az_linear_gradient_fmt_debug: extern "C" fn(_:  &AzLinearGradient) -> AzString,
        pub az_shape_delete: extern "C" fn(_:  &mut AzShape),
        pub az_shape_deep_copy: extern "C" fn(_:  &AzShape) -> AzShape,
        pub az_shape_fmt_debug: extern "C" fn(_:  &AzShape) -> AzString,
        pub az_radial_gradient_delete: extern "C" fn(_:  &mut AzRadialGradient),
        pub az_radial_gradient_deep_copy: extern "C" fn(_:  &AzRadialGradient) -> AzRadialGradient,
        pub az_radial_gradient_fmt_debug: extern "C" fn(_:  &AzRadialGradient) -> AzString,
        pub az_css_image_id_delete: extern "C" fn(_:  &mut AzCssImageId),
        pub az_css_image_id_deep_copy: extern "C" fn(_:  &AzCssImageId) -> AzCssImageId,
        pub az_css_image_id_fmt_debug: extern "C" fn(_:  &AzCssImageId) -> AzString,
        pub az_style_background_content_delete: extern "C" fn(_:  &mut AzStyleBackgroundContent),
        pub az_style_background_content_deep_copy: extern "C" fn(_:  &AzStyleBackgroundContent) -> AzStyleBackgroundContent,
        pub az_style_background_content_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundContent) -> AzString,
        pub az_background_position_horizontal_delete: extern "C" fn(_:  &mut AzBackgroundPositionHorizontal),
        pub az_background_position_horizontal_deep_copy: extern "C" fn(_:  &AzBackgroundPositionHorizontal) -> AzBackgroundPositionHorizontal,
        pub az_background_position_horizontal_fmt_debug: extern "C" fn(_:  &AzBackgroundPositionHorizontal) -> AzString,
        pub az_background_position_vertical_delete: extern "C" fn(_:  &mut AzBackgroundPositionVertical),
        pub az_background_position_vertical_deep_copy: extern "C" fn(_:  &AzBackgroundPositionVertical) -> AzBackgroundPositionVertical,
        pub az_background_position_vertical_fmt_debug: extern "C" fn(_:  &AzBackgroundPositionVertical) -> AzString,
        pub az_style_background_position_delete: extern "C" fn(_:  &mut AzStyleBackgroundPosition),
        pub az_style_background_position_deep_copy: extern "C" fn(_:  &AzStyleBackgroundPosition) -> AzStyleBackgroundPosition,
        pub az_style_background_position_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundPosition) -> AzString,
        pub az_style_background_repeat_delete: extern "C" fn(_:  &mut AzStyleBackgroundRepeat),
        pub az_style_background_repeat_deep_copy: extern "C" fn(_:  &AzStyleBackgroundRepeat) -> AzStyleBackgroundRepeat,
        pub az_style_background_repeat_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundRepeat) -> AzString,
        pub az_style_background_size_delete: extern "C" fn(_:  &mut AzStyleBackgroundSize),
        pub az_style_background_size_deep_copy: extern "C" fn(_:  &AzStyleBackgroundSize) -> AzStyleBackgroundSize,
        pub az_style_background_size_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundSize) -> AzString,
        pub az_style_border_bottom_color_delete: extern "C" fn(_:  &mut AzStyleBorderBottomColor),
        pub az_style_border_bottom_color_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomColor) -> AzStyleBorderBottomColor,
        pub az_style_border_bottom_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomColor) -> AzString,
        pub az_style_border_bottom_left_radius_delete: extern "C" fn(_:  &mut AzStyleBorderBottomLeftRadius),
        pub az_style_border_bottom_left_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomLeftRadius) -> AzStyleBorderBottomLeftRadius,
        pub az_style_border_bottom_left_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomLeftRadius) -> AzString,
        pub az_style_border_bottom_right_radius_delete: extern "C" fn(_:  &mut AzStyleBorderBottomRightRadius),
        pub az_style_border_bottom_right_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomRightRadius) -> AzStyleBorderBottomRightRadius,
        pub az_style_border_bottom_right_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomRightRadius) -> AzString,
        pub az_border_style_delete: extern "C" fn(_:  &mut AzBorderStyle),
        pub az_border_style_deep_copy: extern "C" fn(_:  &AzBorderStyle) -> AzBorderStyle,
        pub az_border_style_fmt_debug: extern "C" fn(_:  &AzBorderStyle) -> AzString,
        pub az_style_border_bottom_style_delete: extern "C" fn(_:  &mut AzStyleBorderBottomStyle),
        pub az_style_border_bottom_style_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomStyle) -> AzStyleBorderBottomStyle,
        pub az_style_border_bottom_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomStyle) -> AzString,
        pub az_style_border_bottom_width_delete: extern "C" fn(_:  &mut AzStyleBorderBottomWidth),
        pub az_style_border_bottom_width_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomWidth) -> AzStyleBorderBottomWidth,
        pub az_style_border_bottom_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomWidth) -> AzString,
        pub az_style_border_left_color_delete: extern "C" fn(_:  &mut AzStyleBorderLeftColor),
        pub az_style_border_left_color_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftColor) -> AzStyleBorderLeftColor,
        pub az_style_border_left_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftColor) -> AzString,
        pub az_style_border_left_style_delete: extern "C" fn(_:  &mut AzStyleBorderLeftStyle),
        pub az_style_border_left_style_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftStyle) -> AzStyleBorderLeftStyle,
        pub az_style_border_left_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftStyle) -> AzString,
        pub az_style_border_left_width_delete: extern "C" fn(_:  &mut AzStyleBorderLeftWidth),
        pub az_style_border_left_width_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftWidth) -> AzStyleBorderLeftWidth,
        pub az_style_border_left_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftWidth) -> AzString,
        pub az_style_border_right_color_delete: extern "C" fn(_:  &mut AzStyleBorderRightColor),
        pub az_style_border_right_color_deep_copy: extern "C" fn(_:  &AzStyleBorderRightColor) -> AzStyleBorderRightColor,
        pub az_style_border_right_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightColor) -> AzString,
        pub az_style_border_right_style_delete: extern "C" fn(_:  &mut AzStyleBorderRightStyle),
        pub az_style_border_right_style_deep_copy: extern "C" fn(_:  &AzStyleBorderRightStyle) -> AzStyleBorderRightStyle,
        pub az_style_border_right_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightStyle) -> AzString,
        pub az_style_border_right_width_delete: extern "C" fn(_:  &mut AzStyleBorderRightWidth),
        pub az_style_border_right_width_deep_copy: extern "C" fn(_:  &AzStyleBorderRightWidth) -> AzStyleBorderRightWidth,
        pub az_style_border_right_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightWidth) -> AzString,
        pub az_style_border_top_color_delete: extern "C" fn(_:  &mut AzStyleBorderTopColor),
        pub az_style_border_top_color_deep_copy: extern "C" fn(_:  &AzStyleBorderTopColor) -> AzStyleBorderTopColor,
        pub az_style_border_top_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopColor) -> AzString,
        pub az_style_border_top_left_radius_delete: extern "C" fn(_:  &mut AzStyleBorderTopLeftRadius),
        pub az_style_border_top_left_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderTopLeftRadius) -> AzStyleBorderTopLeftRadius,
        pub az_style_border_top_left_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopLeftRadius) -> AzString,
        pub az_style_border_top_right_radius_delete: extern "C" fn(_:  &mut AzStyleBorderTopRightRadius),
        pub az_style_border_top_right_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderTopRightRadius) -> AzStyleBorderTopRightRadius,
        pub az_style_border_top_right_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopRightRadius) -> AzString,
        pub az_style_border_top_style_delete: extern "C" fn(_:  &mut AzStyleBorderTopStyle),
        pub az_style_border_top_style_deep_copy: extern "C" fn(_:  &AzStyleBorderTopStyle) -> AzStyleBorderTopStyle,
        pub az_style_border_top_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopStyle) -> AzString,
        pub az_style_border_top_width_delete: extern "C" fn(_:  &mut AzStyleBorderTopWidth),
        pub az_style_border_top_width_deep_copy: extern "C" fn(_:  &AzStyleBorderTopWidth) -> AzStyleBorderTopWidth,
        pub az_style_border_top_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopWidth) -> AzString,
        pub az_style_cursor_delete: extern "C" fn(_:  &mut AzStyleCursor),
        pub az_style_cursor_deep_copy: extern "C" fn(_:  &AzStyleCursor) -> AzStyleCursor,
        pub az_style_cursor_fmt_debug: extern "C" fn(_:  &AzStyleCursor) -> AzString,
        pub az_style_font_family_delete: extern "C" fn(_:  &mut AzStyleFontFamily),
        pub az_style_font_family_deep_copy: extern "C" fn(_:  &AzStyleFontFamily) -> AzStyleFontFamily,
        pub az_style_font_family_fmt_debug: extern "C" fn(_:  &AzStyleFontFamily) -> AzString,
        pub az_style_font_size_delete: extern "C" fn(_:  &mut AzStyleFontSize),
        pub az_style_font_size_deep_copy: extern "C" fn(_:  &AzStyleFontSize) -> AzStyleFontSize,
        pub az_style_font_size_fmt_debug: extern "C" fn(_:  &AzStyleFontSize) -> AzString,
        pub az_style_letter_spacing_delete: extern "C" fn(_:  &mut AzStyleLetterSpacing),
        pub az_style_letter_spacing_deep_copy: extern "C" fn(_:  &AzStyleLetterSpacing) -> AzStyleLetterSpacing,
        pub az_style_letter_spacing_fmt_debug: extern "C" fn(_:  &AzStyleLetterSpacing) -> AzString,
        pub az_style_line_height_delete: extern "C" fn(_:  &mut AzStyleLineHeight),
        pub az_style_line_height_deep_copy: extern "C" fn(_:  &AzStyleLineHeight) -> AzStyleLineHeight,
        pub az_style_line_height_fmt_debug: extern "C" fn(_:  &AzStyleLineHeight) -> AzString,
        pub az_style_tab_width_delete: extern "C" fn(_:  &mut AzStyleTabWidth),
        pub az_style_tab_width_deep_copy: extern "C" fn(_:  &AzStyleTabWidth) -> AzStyleTabWidth,
        pub az_style_tab_width_fmt_debug: extern "C" fn(_:  &AzStyleTabWidth) -> AzString,
        pub az_style_text_alignment_horz_delete: extern "C" fn(_:  &mut AzStyleTextAlignmentHorz),
        pub az_style_text_alignment_horz_deep_copy: extern "C" fn(_:  &AzStyleTextAlignmentHorz) -> AzStyleTextAlignmentHorz,
        pub az_style_text_alignment_horz_fmt_debug: extern "C" fn(_:  &AzStyleTextAlignmentHorz) -> AzString,
        pub az_style_text_color_delete: extern "C" fn(_:  &mut AzStyleTextColor),
        pub az_style_text_color_deep_copy: extern "C" fn(_:  &AzStyleTextColor) -> AzStyleTextColor,
        pub az_style_text_color_fmt_debug: extern "C" fn(_:  &AzStyleTextColor) -> AzString,
        pub az_style_word_spacing_delete: extern "C" fn(_:  &mut AzStyleWordSpacing),
        pub az_style_word_spacing_deep_copy: extern "C" fn(_:  &AzStyleWordSpacing) -> AzStyleWordSpacing,
        pub az_style_word_spacing_fmt_debug: extern "C" fn(_:  &AzStyleWordSpacing) -> AzString,
        pub az_box_shadow_pre_display_item_value_delete: extern "C" fn(_:  &mut AzBoxShadowPreDisplayItemValue),
        pub az_box_shadow_pre_display_item_value_deep_copy: extern "C" fn(_:  &AzBoxShadowPreDisplayItemValue) -> AzBoxShadowPreDisplayItemValue,
        pub az_box_shadow_pre_display_item_value_fmt_debug: extern "C" fn(_:  &AzBoxShadowPreDisplayItemValue) -> AzString,
        pub az_layout_align_content_value_delete: extern "C" fn(_:  &mut AzLayoutAlignContentValue),
        pub az_layout_align_content_value_deep_copy: extern "C" fn(_:  &AzLayoutAlignContentValue) -> AzLayoutAlignContentValue,
        pub az_layout_align_content_value_fmt_debug: extern "C" fn(_:  &AzLayoutAlignContentValue) -> AzString,
        pub az_layout_align_items_value_delete: extern "C" fn(_:  &mut AzLayoutAlignItemsValue),
        pub az_layout_align_items_value_deep_copy: extern "C" fn(_:  &AzLayoutAlignItemsValue) -> AzLayoutAlignItemsValue,
        pub az_layout_align_items_value_fmt_debug: extern "C" fn(_:  &AzLayoutAlignItemsValue) -> AzString,
        pub az_layout_bottom_value_delete: extern "C" fn(_:  &mut AzLayoutBottomValue),
        pub az_layout_bottom_value_deep_copy: extern "C" fn(_:  &AzLayoutBottomValue) -> AzLayoutBottomValue,
        pub az_layout_bottom_value_fmt_debug: extern "C" fn(_:  &AzLayoutBottomValue) -> AzString,
        pub az_layout_box_sizing_value_delete: extern "C" fn(_:  &mut AzLayoutBoxSizingValue),
        pub az_layout_box_sizing_value_deep_copy: extern "C" fn(_:  &AzLayoutBoxSizingValue) -> AzLayoutBoxSizingValue,
        pub az_layout_box_sizing_value_fmt_debug: extern "C" fn(_:  &AzLayoutBoxSizingValue) -> AzString,
        pub az_layout_direction_value_delete: extern "C" fn(_:  &mut AzLayoutDirectionValue),
        pub az_layout_direction_value_deep_copy: extern "C" fn(_:  &AzLayoutDirectionValue) -> AzLayoutDirectionValue,
        pub az_layout_direction_value_fmt_debug: extern "C" fn(_:  &AzLayoutDirectionValue) -> AzString,
        pub az_layout_display_value_delete: extern "C" fn(_:  &mut AzLayoutDisplayValue),
        pub az_layout_display_value_deep_copy: extern "C" fn(_:  &AzLayoutDisplayValue) -> AzLayoutDisplayValue,
        pub az_layout_display_value_fmt_debug: extern "C" fn(_:  &AzLayoutDisplayValue) -> AzString,
        pub az_layout_flex_grow_value_delete: extern "C" fn(_:  &mut AzLayoutFlexGrowValue),
        pub az_layout_flex_grow_value_deep_copy: extern "C" fn(_:  &AzLayoutFlexGrowValue) -> AzLayoutFlexGrowValue,
        pub az_layout_flex_grow_value_fmt_debug: extern "C" fn(_:  &AzLayoutFlexGrowValue) -> AzString,
        pub az_layout_flex_shrink_value_delete: extern "C" fn(_:  &mut AzLayoutFlexShrinkValue),
        pub az_layout_flex_shrink_value_deep_copy: extern "C" fn(_:  &AzLayoutFlexShrinkValue) -> AzLayoutFlexShrinkValue,
        pub az_layout_flex_shrink_value_fmt_debug: extern "C" fn(_:  &AzLayoutFlexShrinkValue) -> AzString,
        pub az_layout_float_value_delete: extern "C" fn(_:  &mut AzLayoutFloatValue),
        pub az_layout_float_value_deep_copy: extern "C" fn(_:  &AzLayoutFloatValue) -> AzLayoutFloatValue,
        pub az_layout_float_value_fmt_debug: extern "C" fn(_:  &AzLayoutFloatValue) -> AzString,
        pub az_layout_height_value_delete: extern "C" fn(_:  &mut AzLayoutHeightValue),
        pub az_layout_height_value_deep_copy: extern "C" fn(_:  &AzLayoutHeightValue) -> AzLayoutHeightValue,
        pub az_layout_height_value_fmt_debug: extern "C" fn(_:  &AzLayoutHeightValue) -> AzString,
        pub az_layout_justify_content_value_delete: extern "C" fn(_:  &mut AzLayoutJustifyContentValue),
        pub az_layout_justify_content_value_deep_copy: extern "C" fn(_:  &AzLayoutJustifyContentValue) -> AzLayoutJustifyContentValue,
        pub az_layout_justify_content_value_fmt_debug: extern "C" fn(_:  &AzLayoutJustifyContentValue) -> AzString,
        pub az_layout_left_value_delete: extern "C" fn(_:  &mut AzLayoutLeftValue),
        pub az_layout_left_value_deep_copy: extern "C" fn(_:  &AzLayoutLeftValue) -> AzLayoutLeftValue,
        pub az_layout_left_value_fmt_debug: extern "C" fn(_:  &AzLayoutLeftValue) -> AzString,
        pub az_layout_margin_bottom_value_delete: extern "C" fn(_:  &mut AzLayoutMarginBottomValue),
        pub az_layout_margin_bottom_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginBottomValue) -> AzLayoutMarginBottomValue,
        pub az_layout_margin_bottom_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginBottomValue) -> AzString,
        pub az_layout_margin_left_value_delete: extern "C" fn(_:  &mut AzLayoutMarginLeftValue),
        pub az_layout_margin_left_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginLeftValue) -> AzLayoutMarginLeftValue,
        pub az_layout_margin_left_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginLeftValue) -> AzString,
        pub az_layout_margin_right_value_delete: extern "C" fn(_:  &mut AzLayoutMarginRightValue),
        pub az_layout_margin_right_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginRightValue) -> AzLayoutMarginRightValue,
        pub az_layout_margin_right_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginRightValue) -> AzString,
        pub az_layout_margin_top_value_delete: extern "C" fn(_:  &mut AzLayoutMarginTopValue),
        pub az_layout_margin_top_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginTopValue) -> AzLayoutMarginTopValue,
        pub az_layout_margin_top_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginTopValue) -> AzString,
        pub az_layout_max_height_value_delete: extern "C" fn(_:  &mut AzLayoutMaxHeightValue),
        pub az_layout_max_height_value_deep_copy: extern "C" fn(_:  &AzLayoutMaxHeightValue) -> AzLayoutMaxHeightValue,
        pub az_layout_max_height_value_fmt_debug: extern "C" fn(_:  &AzLayoutMaxHeightValue) -> AzString,
        pub az_layout_max_width_value_delete: extern "C" fn(_:  &mut AzLayoutMaxWidthValue),
        pub az_layout_max_width_value_deep_copy: extern "C" fn(_:  &AzLayoutMaxWidthValue) -> AzLayoutMaxWidthValue,
        pub az_layout_max_width_value_fmt_debug: extern "C" fn(_:  &AzLayoutMaxWidthValue) -> AzString,
        pub az_layout_min_height_value_delete: extern "C" fn(_:  &mut AzLayoutMinHeightValue),
        pub az_layout_min_height_value_deep_copy: extern "C" fn(_:  &AzLayoutMinHeightValue) -> AzLayoutMinHeightValue,
        pub az_layout_min_height_value_fmt_debug: extern "C" fn(_:  &AzLayoutMinHeightValue) -> AzString,
        pub az_layout_min_width_value_delete: extern "C" fn(_:  &mut AzLayoutMinWidthValue),
        pub az_layout_min_width_value_deep_copy: extern "C" fn(_:  &AzLayoutMinWidthValue) -> AzLayoutMinWidthValue,
        pub az_layout_min_width_value_fmt_debug: extern "C" fn(_:  &AzLayoutMinWidthValue) -> AzString,
        pub az_layout_padding_bottom_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingBottomValue),
        pub az_layout_padding_bottom_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingBottomValue) -> AzLayoutPaddingBottomValue,
        pub az_layout_padding_bottom_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingBottomValue) -> AzString,
        pub az_layout_padding_left_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingLeftValue),
        pub az_layout_padding_left_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingLeftValue) -> AzLayoutPaddingLeftValue,
        pub az_layout_padding_left_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingLeftValue) -> AzString,
        pub az_layout_padding_right_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingRightValue),
        pub az_layout_padding_right_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingRightValue) -> AzLayoutPaddingRightValue,
        pub az_layout_padding_right_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingRightValue) -> AzString,
        pub az_layout_padding_top_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingTopValue),
        pub az_layout_padding_top_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingTopValue) -> AzLayoutPaddingTopValue,
        pub az_layout_padding_top_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingTopValue) -> AzString,
        pub az_layout_position_value_delete: extern "C" fn(_:  &mut AzLayoutPositionValue),
        pub az_layout_position_value_deep_copy: extern "C" fn(_:  &AzLayoutPositionValue) -> AzLayoutPositionValue,
        pub az_layout_position_value_fmt_debug: extern "C" fn(_:  &AzLayoutPositionValue) -> AzString,
        pub az_layout_right_value_delete: extern "C" fn(_:  &mut AzLayoutRightValue),
        pub az_layout_right_value_deep_copy: extern "C" fn(_:  &AzLayoutRightValue) -> AzLayoutRightValue,
        pub az_layout_right_value_fmt_debug: extern "C" fn(_:  &AzLayoutRightValue) -> AzString,
        pub az_layout_top_value_delete: extern "C" fn(_:  &mut AzLayoutTopValue),
        pub az_layout_top_value_deep_copy: extern "C" fn(_:  &AzLayoutTopValue) -> AzLayoutTopValue,
        pub az_layout_top_value_fmt_debug: extern "C" fn(_:  &AzLayoutTopValue) -> AzString,
        pub az_layout_width_value_delete: extern "C" fn(_:  &mut AzLayoutWidthValue),
        pub az_layout_width_value_deep_copy: extern "C" fn(_:  &AzLayoutWidthValue) -> AzLayoutWidthValue,
        pub az_layout_width_value_fmt_debug: extern "C" fn(_:  &AzLayoutWidthValue) -> AzString,
        pub az_layout_wrap_value_delete: extern "C" fn(_:  &mut AzLayoutWrapValue),
        pub az_layout_wrap_value_deep_copy: extern "C" fn(_:  &AzLayoutWrapValue) -> AzLayoutWrapValue,
        pub az_layout_wrap_value_fmt_debug: extern "C" fn(_:  &AzLayoutWrapValue) -> AzString,
        pub az_overflow_value_delete: extern "C" fn(_:  &mut AzOverflowValue),
        pub az_overflow_value_deep_copy: extern "C" fn(_:  &AzOverflowValue) -> AzOverflowValue,
        pub az_overflow_value_fmt_debug: extern "C" fn(_:  &AzOverflowValue) -> AzString,
        pub az_style_background_content_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundContentValue),
        pub az_style_background_content_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue,
        pub az_style_background_content_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundContentValue) -> AzString,
        pub az_style_background_position_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundPositionValue),
        pub az_style_background_position_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundPositionValue) -> AzStyleBackgroundPositionValue,
        pub az_style_background_position_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundPositionValue) -> AzString,
        pub az_style_background_repeat_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundRepeatValue),
        pub az_style_background_repeat_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundRepeatValue) -> AzStyleBackgroundRepeatValue,
        pub az_style_background_repeat_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundRepeatValue) -> AzString,
        pub az_style_background_size_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundSizeValue),
        pub az_style_background_size_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundSizeValue) -> AzStyleBackgroundSizeValue,
        pub az_style_background_size_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundSizeValue) -> AzString,
        pub az_style_border_bottom_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomColorValue),
        pub az_style_border_bottom_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomColorValue) -> AzStyleBorderBottomColorValue,
        pub az_style_border_bottom_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomColorValue) -> AzString,
        pub az_style_border_bottom_left_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomLeftRadiusValue),
        pub az_style_border_bottom_left_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomLeftRadiusValue) -> AzStyleBorderBottomLeftRadiusValue,
        pub az_style_border_bottom_left_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomLeftRadiusValue) -> AzString,
        pub az_style_border_bottom_right_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomRightRadiusValue),
        pub az_style_border_bottom_right_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomRightRadiusValue) -> AzStyleBorderBottomRightRadiusValue,
        pub az_style_border_bottom_right_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomRightRadiusValue) -> AzString,
        pub az_style_border_bottom_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomStyleValue),
        pub az_style_border_bottom_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomStyleValue) -> AzStyleBorderBottomStyleValue,
        pub az_style_border_bottom_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomStyleValue) -> AzString,
        pub az_style_border_bottom_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomWidthValue),
        pub az_style_border_bottom_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomWidthValue) -> AzStyleBorderBottomWidthValue,
        pub az_style_border_bottom_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomWidthValue) -> AzString,
        pub az_style_border_left_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderLeftColorValue),
        pub az_style_border_left_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftColorValue) -> AzStyleBorderLeftColorValue,
        pub az_style_border_left_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftColorValue) -> AzString,
        pub az_style_border_left_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderLeftStyleValue),
        pub az_style_border_left_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftStyleValue) -> AzStyleBorderLeftStyleValue,
        pub az_style_border_left_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftStyleValue) -> AzString,
        pub az_style_border_left_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderLeftWidthValue),
        pub az_style_border_left_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftWidthValue) -> AzStyleBorderLeftWidthValue,
        pub az_style_border_left_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftWidthValue) -> AzString,
        pub az_style_border_right_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderRightColorValue),
        pub az_style_border_right_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderRightColorValue) -> AzStyleBorderRightColorValue,
        pub az_style_border_right_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightColorValue) -> AzString,
        pub az_style_border_right_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderRightStyleValue),
        pub az_style_border_right_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderRightStyleValue) -> AzStyleBorderRightStyleValue,
        pub az_style_border_right_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightStyleValue) -> AzString,
        pub az_style_border_right_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderRightWidthValue),
        pub az_style_border_right_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderRightWidthValue) -> AzStyleBorderRightWidthValue,
        pub az_style_border_right_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightWidthValue) -> AzString,
        pub az_style_border_top_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopColorValue),
        pub az_style_border_top_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopColorValue) -> AzStyleBorderTopColorValue,
        pub az_style_border_top_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopColorValue) -> AzString,
        pub az_style_border_top_left_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopLeftRadiusValue),
        pub az_style_border_top_left_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopLeftRadiusValue) -> AzStyleBorderTopLeftRadiusValue,
        pub az_style_border_top_left_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopLeftRadiusValue) -> AzString,
        pub az_style_border_top_right_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopRightRadiusValue),
        pub az_style_border_top_right_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopRightRadiusValue) -> AzStyleBorderTopRightRadiusValue,
        pub az_style_border_top_right_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopRightRadiusValue) -> AzString,
        pub az_style_border_top_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopStyleValue),
        pub az_style_border_top_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopStyleValue) -> AzStyleBorderTopStyleValue,
        pub az_style_border_top_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopStyleValue) -> AzString,
        pub az_style_border_top_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopWidthValue),
        pub az_style_border_top_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopWidthValue) -> AzStyleBorderTopWidthValue,
        pub az_style_border_top_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopWidthValue) -> AzString,
        pub az_style_cursor_value_delete: extern "C" fn(_:  &mut AzStyleCursorValue),
        pub az_style_cursor_value_deep_copy: extern "C" fn(_:  &AzStyleCursorValue) -> AzStyleCursorValue,
        pub az_style_cursor_value_fmt_debug: extern "C" fn(_:  &AzStyleCursorValue) -> AzString,
        pub az_style_font_family_value_delete: extern "C" fn(_:  &mut AzStyleFontFamilyValue),
        pub az_style_font_family_value_deep_copy: extern "C" fn(_:  &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue,
        pub az_style_font_family_value_fmt_debug: extern "C" fn(_:  &AzStyleFontFamilyValue) -> AzString,
        pub az_style_font_size_value_delete: extern "C" fn(_:  &mut AzStyleFontSizeValue),
        pub az_style_font_size_value_deep_copy: extern "C" fn(_:  &AzStyleFontSizeValue) -> AzStyleFontSizeValue,
        pub az_style_font_size_value_fmt_debug: extern "C" fn(_:  &AzStyleFontSizeValue) -> AzString,
        pub az_style_letter_spacing_value_delete: extern "C" fn(_:  &mut AzStyleLetterSpacingValue),
        pub az_style_letter_spacing_value_deep_copy: extern "C" fn(_:  &AzStyleLetterSpacingValue) -> AzStyleLetterSpacingValue,
        pub az_style_letter_spacing_value_fmt_debug: extern "C" fn(_:  &AzStyleLetterSpacingValue) -> AzString,
        pub az_style_line_height_value_delete: extern "C" fn(_:  &mut AzStyleLineHeightValue),
        pub az_style_line_height_value_deep_copy: extern "C" fn(_:  &AzStyleLineHeightValue) -> AzStyleLineHeightValue,
        pub az_style_line_height_value_fmt_debug: extern "C" fn(_:  &AzStyleLineHeightValue) -> AzString,
        pub az_style_tab_width_value_delete: extern "C" fn(_:  &mut AzStyleTabWidthValue),
        pub az_style_tab_width_value_deep_copy: extern "C" fn(_:  &AzStyleTabWidthValue) -> AzStyleTabWidthValue,
        pub az_style_tab_width_value_fmt_debug: extern "C" fn(_:  &AzStyleTabWidthValue) -> AzString,
        pub az_style_text_alignment_horz_value_delete: extern "C" fn(_:  &mut AzStyleTextAlignmentHorzValue),
        pub az_style_text_alignment_horz_value_deep_copy: extern "C" fn(_:  &AzStyleTextAlignmentHorzValue) -> AzStyleTextAlignmentHorzValue,
        pub az_style_text_alignment_horz_value_fmt_debug: extern "C" fn(_:  &AzStyleTextAlignmentHorzValue) -> AzString,
        pub az_style_text_color_value_delete: extern "C" fn(_:  &mut AzStyleTextColorValue),
        pub az_style_text_color_value_deep_copy: extern "C" fn(_:  &AzStyleTextColorValue) -> AzStyleTextColorValue,
        pub az_style_text_color_value_fmt_debug: extern "C" fn(_:  &AzStyleTextColorValue) -> AzString,
        pub az_style_word_spacing_value_delete: extern "C" fn(_:  &mut AzStyleWordSpacingValue),
        pub az_style_word_spacing_value_deep_copy: extern "C" fn(_:  &AzStyleWordSpacingValue) -> AzStyleWordSpacingValue,
        pub az_style_word_spacing_value_fmt_debug: extern "C" fn(_:  &AzStyleWordSpacingValue) -> AzString,
        pub az_css_property_delete: extern "C" fn(_:  &mut AzCssProperty),
        pub az_css_property_deep_copy: extern "C" fn(_:  &AzCssProperty) -> AzCssProperty,
        pub az_css_property_fmt_debug: extern "C" fn(_:  &AzCssProperty) -> AzString,
        pub az_css_property_partial_eq: extern "C" fn(_:  &AzCssProperty, _:  &AzCssProperty) -> bool,
        pub az_css_property_partial_cmp: extern "C" fn(_:  &AzCssProperty, _:  &AzCssProperty) -> u8,
        pub az_css_property_cmp: extern "C" fn(_:  &AzCssProperty, _:  &AzCssProperty) -> u8,
        pub az_css_property_hash: extern "C" fn(_:  &AzCssProperty) -> u64,
        pub az_node_delete: extern "C" fn(_:  &mut AzNode),
        pub az_node_deep_copy: extern "C" fn(_:  &AzNode) -> AzNode,
        pub az_node_fmt_debug: extern "C" fn(_:  &AzNode) -> AzString,
        pub az_node_partial_eq: extern "C" fn(_:  &AzNode, _:  &AzNode) -> bool,
        pub az_node_partial_cmp: extern "C" fn(_:  &AzNode, _:  &AzNode) -> u8,
        pub az_node_cmp: extern "C" fn(_:  &AzNode, _:  &AzNode) -> u8,
        pub az_node_hash: extern "C" fn(_:  &AzNode) -> u64,
        pub az_cascade_info_delete: extern "C" fn(_:  &mut AzCascadeInfo),
        pub az_cascade_info_deep_copy: extern "C" fn(_:  &AzCascadeInfo) -> AzCascadeInfo,
        pub az_cascade_info_fmt_debug: extern "C" fn(_:  &AzCascadeInfo) -> AzString,
        pub az_cascade_info_partial_eq: extern "C" fn(_:  &AzCascadeInfo, _:  &AzCascadeInfo) -> bool,
        pub az_cascade_info_partial_cmp: extern "C" fn(_:  &AzCascadeInfo, _:  &AzCascadeInfo) -> u8,
        pub az_cascade_info_cmp: extern "C" fn(_:  &AzCascadeInfo, _:  &AzCascadeInfo) -> u8,
        pub az_cascade_info_hash: extern "C" fn(_:  &AzCascadeInfo) -> u64,
        pub az_rect_style_delete: extern "C" fn(_:  &mut AzRectStyle),
        pub az_rect_style_deep_copy: extern "C" fn(_:  &AzRectStyle) -> AzRectStyle,
        pub az_rect_style_fmt_debug: extern "C" fn(_:  &AzRectStyle) -> AzString,
        pub az_rect_style_partial_eq: extern "C" fn(_:  &AzRectStyle, _:  &AzRectStyle) -> bool,
        pub az_rect_style_partial_cmp: extern "C" fn(_:  &AzRectStyle, _:  &AzRectStyle) -> u8,
        pub az_rect_style_cmp: extern "C" fn(_:  &AzRectStyle, _:  &AzRectStyle) -> u8,
        pub az_rect_style_hash: extern "C" fn(_:  &AzRectStyle) -> u64,
        pub az_rect_layout_delete: extern "C" fn(_:  &mut AzRectLayout),
        pub az_rect_layout_deep_copy: extern "C" fn(_:  &AzRectLayout) -> AzRectLayout,
        pub az_rect_layout_fmt_debug: extern "C" fn(_:  &AzRectLayout) -> AzString,
        pub az_rect_layout_partial_eq: extern "C" fn(_:  &AzRectLayout, _:  &AzRectLayout) -> bool,
        pub az_rect_layout_partial_cmp: extern "C" fn(_:  &AzRectLayout, _:  &AzRectLayout) -> u8,
        pub az_rect_layout_cmp: extern "C" fn(_:  &AzRectLayout, _:  &AzRectLayout) -> u8,
        pub az_rect_layout_hash: extern "C" fn(_:  &AzRectLayout) -> u64,
        pub az_cascaded_css_property_with_source_delete: extern "C" fn(_:  &mut AzCascadedCssPropertyWithSource),
        pub az_cascaded_css_property_with_source_deep_copy: extern "C" fn(_:  &AzCascadedCssPropertyWithSource) -> AzCascadedCssPropertyWithSource,
        pub az_cascaded_css_property_with_source_fmt_debug: extern "C" fn(_:  &AzCascadedCssPropertyWithSource) -> AzString,
        pub az_cascaded_css_property_with_source_partial_eq: extern "C" fn(_:  &AzCascadedCssPropertyWithSource, _:  &AzCascadedCssPropertyWithSource) -> bool,
        pub az_cascaded_css_property_with_source_partial_cmp: extern "C" fn(_:  &AzCascadedCssPropertyWithSource, _:  &AzCascadedCssPropertyWithSource) -> u8,
        pub az_cascaded_css_property_with_source_cmp: extern "C" fn(_:  &AzCascadedCssPropertyWithSource, _:  &AzCascadedCssPropertyWithSource) -> u8,
        pub az_cascaded_css_property_with_source_hash: extern "C" fn(_:  &AzCascadedCssPropertyWithSource) -> u64,
        pub az_css_property_source_delete: extern "C" fn(_:  &mut AzCssPropertySource),
        pub az_css_property_source_deep_copy: extern "C" fn(_:  &AzCssPropertySource) -> AzCssPropertySource,
        pub az_css_property_source_fmt_debug: extern "C" fn(_:  &AzCssPropertySource) -> AzString,
        pub az_css_property_source_partial_eq: extern "C" fn(_:  &AzCssPropertySource, _:  &AzCssPropertySource) -> bool,
        pub az_css_property_source_partial_cmp: extern "C" fn(_:  &AzCssPropertySource, _:  &AzCssPropertySource) -> u8,
        pub az_css_property_source_cmp: extern "C" fn(_:  &AzCssPropertySource, _:  &AzCssPropertySource) -> u8,
        pub az_css_property_source_hash: extern "C" fn(_:  &AzCssPropertySource) -> u64,
        pub az_styled_node_delete: extern "C" fn(_:  &mut AzStyledNode),
        pub az_styled_node_deep_copy: extern "C" fn(_:  &AzStyledNode) -> AzStyledNode,
        pub az_styled_node_fmt_debug: extern "C" fn(_:  &AzStyledNode) -> AzString,
        pub az_styled_node_partial_eq: extern "C" fn(_:  &AzStyledNode, _:  &AzStyledNode) -> bool,
        pub az_styled_node_partial_cmp: extern "C" fn(_:  &AzStyledNode, _:  &AzStyledNode) -> u8,
        pub az_styled_node_cmp: extern "C" fn(_:  &AzStyledNode, _:  &AzStyledNode) -> u8,
        pub az_styled_node_hash: extern "C" fn(_:  &AzStyledNode) -> u64,
        pub az_tag_id_delete: extern "C" fn(_:  &mut AzTagId),
        pub az_tag_id_deep_copy: extern "C" fn(_:  &AzTagId) -> AzTagId,
        pub az_tag_id_fmt_debug: extern "C" fn(_:  &AzTagId) -> AzString,
        pub az_tag_id_partial_eq: extern "C" fn(_:  &AzTagId, _:  &AzTagId) -> bool,
        pub az_tag_id_partial_cmp: extern "C" fn(_:  &AzTagId, _:  &AzTagId) -> u8,
        pub az_tag_id_cmp: extern "C" fn(_:  &AzTagId, _:  &AzTagId) -> u8,
        pub az_tag_id_hash: extern "C" fn(_:  &AzTagId) -> u64,
        pub az_tag_id_to_node_id_mapping_delete: extern "C" fn(_:  &mut AzTagIdToNodeIdMapping),
        pub az_tag_id_to_node_id_mapping_deep_copy: extern "C" fn(_:  &AzTagIdToNodeIdMapping) -> AzTagIdToNodeIdMapping,
        pub az_tag_id_to_node_id_mapping_fmt_debug: extern "C" fn(_:  &AzTagIdToNodeIdMapping) -> AzString,
        pub az_tag_id_to_node_id_mapping_partial_eq: extern "C" fn(_:  &AzTagIdToNodeIdMapping, _:  &AzTagIdToNodeIdMapping) -> bool,
        pub az_tag_id_to_node_id_mapping_partial_cmp: extern "C" fn(_:  &AzTagIdToNodeIdMapping, _:  &AzTagIdToNodeIdMapping) -> u8,
        pub az_tag_id_to_node_id_mapping_cmp: extern "C" fn(_:  &AzTagIdToNodeIdMapping, _:  &AzTagIdToNodeIdMapping) -> u8,
        pub az_tag_id_to_node_id_mapping_hash: extern "C" fn(_:  &AzTagIdToNodeIdMapping) -> u64,
        pub az_hover_group_delete: extern "C" fn(_:  &mut AzHoverGroup),
        pub az_hover_group_deep_copy: extern "C" fn(_:  &AzHoverGroup) -> AzHoverGroup,
        pub az_hover_group_fmt_debug: extern "C" fn(_:  &AzHoverGroup) -> AzString,
        pub az_hover_group_partial_eq: extern "C" fn(_:  &AzHoverGroup, _:  &AzHoverGroup) -> bool,
        pub az_hover_group_partial_cmp: extern "C" fn(_:  &AzHoverGroup, _:  &AzHoverGroup) -> u8,
        pub az_hover_group_cmp: extern "C" fn(_:  &AzHoverGroup, _:  &AzHoverGroup) -> u8,
        pub az_hover_group_hash: extern "C" fn(_:  &AzHoverGroup) -> u64,
        pub az_active_hover_delete: extern "C" fn(_:  &mut AzActiveHover),
        pub az_active_hover_deep_copy: extern "C" fn(_:  &AzActiveHover) -> AzActiveHover,
        pub az_active_hover_fmt_debug: extern "C" fn(_:  &AzActiveHover) -> AzString,
        pub az_active_hover_partial_eq: extern "C" fn(_:  &AzActiveHover, _:  &AzActiveHover) -> bool,
        pub az_active_hover_partial_cmp: extern "C" fn(_:  &AzActiveHover, _:  &AzActiveHover) -> u8,
        pub az_active_hover_cmp: extern "C" fn(_:  &AzActiveHover, _:  &AzActiveHover) -> u8,
        pub az_active_hover_hash: extern "C" fn(_:  &AzActiveHover) -> u64,
        pub az_parent_with_node_depth_delete: extern "C" fn(_:  &mut AzParentWithNodeDepth),
        pub az_parent_with_node_depth_deep_copy: extern "C" fn(_:  &AzParentWithNodeDepth) -> AzParentWithNodeDepth,
        pub az_parent_with_node_depth_fmt_debug: extern "C" fn(_:  &AzParentWithNodeDepth) -> AzString,
        pub az_parent_with_node_depth_partial_eq: extern "C" fn(_:  &AzParentWithNodeDepth, _:  &AzParentWithNodeDepth) -> bool,
        pub az_parent_with_node_depth_partial_cmp: extern "C" fn(_:  &AzParentWithNodeDepth, _:  &AzParentWithNodeDepth) -> u8,
        pub az_parent_with_node_depth_cmp: extern "C" fn(_:  &AzParentWithNodeDepth, _:  &AzParentWithNodeDepth) -> u8,
        pub az_parent_with_node_depth_hash: extern "C" fn(_:  &AzParentWithNodeDepth) -> u64,
        pub az_style_options_delete: extern "C" fn(_:  &mut AzStyleOptions),
        pub az_style_options_deep_copy: extern "C" fn(_:  &AzStyleOptions) -> AzStyleOptions,
        pub az_style_options_fmt_debug: extern "C" fn(_:  &AzStyleOptions) -> AzString,
        pub az_style_options_partial_eq: extern "C" fn(_:  &AzStyleOptions, _:  &AzStyleOptions) -> bool,
        pub az_style_options_partial_cmp: extern "C" fn(_:  &AzStyleOptions, _:  &AzStyleOptions) -> u8,
        pub az_style_options_cmp: extern "C" fn(_:  &AzStyleOptions, _:  &AzStyleOptions) -> u8,
        pub az_style_options_hash: extern "C" fn(_:  &AzStyleOptions) -> u64,
        pub az_styled_dom_new: extern "C" fn(_:  AzDom, _:  AzCss, _:  AzStyleOptions) -> AzStyledDom,
        pub az_styled_dom_append: extern "C" fn(_:  &mut AzStyledDom, _:  AzStyledDom),
        pub az_styled_dom_delete: extern "C" fn(_:  &mut AzStyledDom),
        pub az_styled_dom_deep_copy: extern "C" fn(_:  &AzStyledDom) -> AzStyledDom,
        pub az_styled_dom_fmt_debug: extern "C" fn(_:  &AzStyledDom) -> AzString,
        pub az_styled_dom_partial_eq: extern "C" fn(_:  &AzStyledDom, _:  &AzStyledDom) -> bool,
        pub az_styled_dom_partial_cmp: extern "C" fn(_:  &AzStyledDom, _:  &AzStyledDom) -> u8,
        pub az_styled_dom_cmp: extern "C" fn(_:  &AzStyledDom, _:  &AzStyledDom) -> u8,
        pub az_styled_dom_hash: extern "C" fn(_:  &AzStyledDom) -> u64,
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
        pub az_dom_add_inline_css: extern "C" fn(_:  &mut AzDom, _:  AzCssProperty),
        pub az_dom_with_inline_css: extern "C" fn(_:  AzDom, _:  AzCssProperty) -> AzDom,
        pub az_dom_set_is_draggable: extern "C" fn(_:  &mut AzDom, _:  bool),
        pub az_dom_with_clip_mask: extern "C" fn(_:  AzDom, _:  AzOptionImageMask) -> AzDom,
        pub az_dom_set_clip_mask: extern "C" fn(_:  &mut AzDom, _:  AzOptionImageMask),
        pub az_dom_is_draggable: extern "C" fn(_:  AzDom, _:  bool) -> AzDom,
        pub az_dom_set_tab_index: extern "C" fn(_:  &mut AzDom, _:  AzOptionTabIndex),
        pub az_dom_with_tab_index: extern "C" fn(_:  AzDom, _:  AzOptionTabIndex) -> AzDom,
        pub az_dom_has_id: extern "C" fn(_:  &mut AzDom, _:  AzString) -> bool,
        pub az_dom_has_class: extern "C" fn(_:  &mut AzDom, _:  AzString) -> bool,
        pub az_dom_add_child: extern "C" fn(_:  &mut AzDom, _:  AzDom),
        pub az_dom_with_child: extern "C" fn(_:  AzDom, _:  AzDom) -> AzDom,
        pub az_dom_get_html_string: extern "C" fn(_:  &AzDom) -> AzString,
        pub az_dom_delete: extern "C" fn(_:  &mut AzDom),
        pub az_dom_deep_copy: extern "C" fn(_:  &AzDom) -> AzDom,
        pub az_dom_fmt_debug: extern "C" fn(_:  &AzDom) -> AzString,
        pub az_dom_partial_eq: extern "C" fn(_:  &AzDom, _:  &AzDom) -> bool,
        pub az_dom_partial_cmp: extern "C" fn(_:  &AzDom, _:  &AzDom) -> u8,
        pub az_dom_cmp: extern "C" fn(_:  &AzDom, _:  &AzDom) -> u8,
        pub az_dom_hash: extern "C" fn(_:  &AzDom) -> u64,
        pub az_gl_texture_node_delete: extern "C" fn(_:  &mut AzGlTextureNode),
        pub az_gl_texture_node_deep_copy: extern "C" fn(_:  &AzGlTextureNode) -> AzGlTextureNode,
        pub az_gl_texture_node_fmt_debug: extern "C" fn(_:  &AzGlTextureNode) -> AzString,
        pub az_i_frame_node_delete: extern "C" fn(_:  &mut AzIFrameNode),
        pub az_i_frame_node_deep_copy: extern "C" fn(_:  &AzIFrameNode) -> AzIFrameNode,
        pub az_i_frame_node_fmt_debug: extern "C" fn(_:  &AzIFrameNode) -> AzString,
        pub az_callback_data_delete: extern "C" fn(_:  &mut AzCallbackData),
        pub az_callback_data_deep_copy: extern "C" fn(_:  &AzCallbackData) -> AzCallbackData,
        pub az_callback_data_fmt_debug: extern "C" fn(_:  &AzCallbackData) -> AzString,
        pub az_callback_data_partial_eq: extern "C" fn(_:  &AzCallbackData, _:  &AzCallbackData) -> bool,
        pub az_callback_data_partial_cmp: extern "C" fn(_:  &AzCallbackData, _:  &AzCallbackData) -> u8,
        pub az_callback_data_cmp: extern "C" fn(_:  &AzCallbackData, _:  &AzCallbackData) -> u8,
        pub az_callback_data_hash: extern "C" fn(_:  &AzCallbackData) -> u64,
        pub az_image_mask_delete: extern "C" fn(_:  &mut AzImageMask),
        pub az_image_mask_deep_copy: extern "C" fn(_:  &AzImageMask) -> AzImageMask,
        pub az_image_mask_fmt_debug: extern "C" fn(_:  &AzImageMask) -> AzString,
        pub az_image_mask_partial_eq: extern "C" fn(_:  &AzImageMask, _:  &AzImageMask) -> bool,
        pub az_image_mask_partial_cmp: extern "C" fn(_:  &AzImageMask, _:  &AzImageMask) -> u8,
        pub az_image_mask_cmp: extern "C" fn(_:  &AzImageMask, _:  &AzImageMask) -> u8,
        pub az_image_mask_hash: extern "C" fn(_:  &AzImageMask) -> u64,
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
        pub az_node_data_add_callback: extern "C" fn(_:  &mut AzNodeData, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType),
        pub az_node_data_with_callback: extern "C" fn(_:  AzNodeData, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzNodeData,
        pub az_node_data_add_inline_css: extern "C" fn(_:  &mut AzNodeData, _:  AzCssProperty),
        pub az_node_data_with_inline_css: extern "C" fn(_:  AzNodeData, _:  AzCssProperty) -> AzNodeData,
        pub az_node_data_with_clip_mask: extern "C" fn(_:  AzNodeData, _:  AzOptionImageMask) -> AzNodeData,
        pub az_node_data_set_clip_mask: extern "C" fn(_:  &mut AzNodeData, _:  AzOptionImageMask),
        pub az_node_data_set_is_draggable: extern "C" fn(_:  &mut AzNodeData, _:  bool),
        pub az_node_data_is_draggable: extern "C" fn(_:  AzNodeData, _:  bool) -> AzNodeData,
        pub az_node_data_set_tab_index: extern "C" fn(_:  &mut AzNodeData, _:  AzOptionTabIndex),
        pub az_node_data_with_tab_index: extern "C" fn(_:  AzNodeData, _:  AzOptionTabIndex) -> AzNodeData,
        pub az_node_data_has_id: extern "C" fn(_:  &mut AzNodeData, _:  AzString) -> bool,
        pub az_node_data_has_class: extern "C" fn(_:  &mut AzNodeData, _:  AzString) -> bool,
        pub az_node_data_delete: extern "C" fn(_:  &mut AzNodeData),
        pub az_node_data_deep_copy: extern "C" fn(_:  &AzNodeData) -> AzNodeData,
        pub az_node_data_fmt_debug: extern "C" fn(_:  &AzNodeData) -> AzString,
        pub az_node_type_delete: extern "C" fn(_:  &mut AzNodeType),
        pub az_node_type_deep_copy: extern "C" fn(_:  &AzNodeType) -> AzNodeType,
        pub az_node_type_fmt_debug: extern "C" fn(_:  &AzNodeType) -> AzString,
        pub az_on_into_event_filter: extern "C" fn(_:  AzOn) -> AzEventFilter,
        pub az_on_delete: extern "C" fn(_:  &mut AzOn),
        pub az_on_deep_copy: extern "C" fn(_:  &AzOn) -> AzOn,
        pub az_on_fmt_debug: extern "C" fn(_:  &AzOn) -> AzString,
        pub az_event_filter_delete: extern "C" fn(_:  &mut AzEventFilter),
        pub az_event_filter_deep_copy: extern "C" fn(_:  &AzEventFilter) -> AzEventFilter,
        pub az_event_filter_fmt_debug: extern "C" fn(_:  &AzEventFilter) -> AzString,
        pub az_hover_event_filter_delete: extern "C" fn(_:  &mut AzHoverEventFilter),
        pub az_hover_event_filter_deep_copy: extern "C" fn(_:  &AzHoverEventFilter) -> AzHoverEventFilter,
        pub az_hover_event_filter_fmt_debug: extern "C" fn(_:  &AzHoverEventFilter) -> AzString,
        pub az_focus_event_filter_delete: extern "C" fn(_:  &mut AzFocusEventFilter),
        pub az_focus_event_filter_deep_copy: extern "C" fn(_:  &AzFocusEventFilter) -> AzFocusEventFilter,
        pub az_focus_event_filter_fmt_debug: extern "C" fn(_:  &AzFocusEventFilter) -> AzString,
        pub az_not_event_filter_delete: extern "C" fn(_:  &mut AzNotEventFilter),
        pub az_not_event_filter_deep_copy: extern "C" fn(_:  &AzNotEventFilter) -> AzNotEventFilter,
        pub az_not_event_filter_fmt_debug: extern "C" fn(_:  &AzNotEventFilter) -> AzString,
        pub az_window_event_filter_delete: extern "C" fn(_:  &mut AzWindowEventFilter),
        pub az_window_event_filter_deep_copy: extern "C" fn(_:  &AzWindowEventFilter) -> AzWindowEventFilter,
        pub az_window_event_filter_fmt_debug: extern "C" fn(_:  &AzWindowEventFilter) -> AzString,
        pub az_tab_index_delete: extern "C" fn(_:  &mut AzTabIndex),
        pub az_tab_index_deep_copy: extern "C" fn(_:  &AzTabIndex) -> AzTabIndex,
        pub az_tab_index_fmt_debug: extern "C" fn(_:  &AzTabIndex) -> AzString,
        pub az_gl_shader_precision_format_return_delete: extern "C" fn(_:  &mut AzGlShaderPrecisionFormatReturn),
        pub az_gl_shader_precision_format_return_deep_copy: extern "C" fn(_:  &AzGlShaderPrecisionFormatReturn) -> AzGlShaderPrecisionFormatReturn,
        pub az_gl_shader_precision_format_return_fmt_debug: extern "C" fn(_:  &AzGlShaderPrecisionFormatReturn) -> AzString,
        pub az_vertex_attribute_type_delete: extern "C" fn(_:  &mut AzVertexAttributeType),
        pub az_vertex_attribute_type_deep_copy: extern "C" fn(_:  &AzVertexAttributeType) -> AzVertexAttributeType,
        pub az_vertex_attribute_type_fmt_debug: extern "C" fn(_:  &AzVertexAttributeType) -> AzString,
        pub az_vertex_attribute_delete: extern "C" fn(_:  &mut AzVertexAttribute),
        pub az_vertex_attribute_deep_copy: extern "C" fn(_:  &AzVertexAttribute) -> AzVertexAttribute,
        pub az_vertex_attribute_fmt_debug: extern "C" fn(_:  &AzVertexAttribute) -> AzString,
        pub az_vertex_layout_delete: extern "C" fn(_:  &mut AzVertexLayout),
        pub az_vertex_layout_deep_copy: extern "C" fn(_:  &AzVertexLayout) -> AzVertexLayout,
        pub az_vertex_layout_fmt_debug: extern "C" fn(_:  &AzVertexLayout) -> AzString,
        pub az_vertex_array_object_delete: extern "C" fn(_:  &mut AzVertexArrayObject),
        pub az_vertex_array_object_fmt_debug: extern "C" fn(_:  &AzVertexArrayObject) -> AzString,
        pub az_index_buffer_format_delete: extern "C" fn(_:  &mut AzIndexBufferFormat),
        pub az_index_buffer_format_deep_copy: extern "C" fn(_:  &AzIndexBufferFormat) -> AzIndexBufferFormat,
        pub az_index_buffer_format_fmt_debug: extern "C" fn(_:  &AzIndexBufferFormat) -> AzString,
        pub az_vertex_buffer_delete: extern "C" fn(_:  &mut AzVertexBuffer),
        pub az_vertex_buffer_fmt_debug: extern "C" fn(_:  &AzVertexBuffer) -> AzString,
        pub az_gl_type_delete: extern "C" fn(_:  &mut AzGlType),
        pub az_gl_type_deep_copy: extern "C" fn(_:  &AzGlType) -> AzGlType,
        pub az_gl_type_fmt_debug: extern "C" fn(_:  &AzGlType) -> AzString,
        pub az_debug_message_delete: extern "C" fn(_:  &mut AzDebugMessage),
        pub az_debug_message_deep_copy: extern "C" fn(_:  &AzDebugMessage) -> AzDebugMessage,
        pub az_debug_message_fmt_debug: extern "C" fn(_:  &AzDebugMessage) -> AzString,
        pub az_debug_message_partial_eq: extern "C" fn(_:  &AzDebugMessage, _:  &AzDebugMessage) -> bool,
        pub az_debug_message_partial_cmp: extern "C" fn(_:  &AzDebugMessage, _:  &AzDebugMessage) -> u8,
        pub az_debug_message_cmp: extern "C" fn(_:  &AzDebugMessage, _:  &AzDebugMessage) -> u8,
        pub az_debug_message_hash: extern "C" fn(_:  &AzDebugMessage) -> u64,
        pub az_u8_vec_ref_delete: extern "C" fn(_:  &mut AzU8VecRef),
        pub az_u8_vec_ref_fmt_debug: extern "C" fn(_:  &AzU8VecRef) -> AzString,
        pub az_u8_vec_ref_mut_delete: extern "C" fn(_:  &mut AzU8VecRefMut),
        pub az_u8_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzU8VecRefMut) -> AzString,
        pub az_f32_vec_ref_delete: extern "C" fn(_:  &mut AzF32VecRef),
        pub az_f32_vec_ref_fmt_debug: extern "C" fn(_:  &AzF32VecRef) -> AzString,
        pub az_i32_vec_ref_delete: extern "C" fn(_:  &mut AzI32VecRef),
        pub az_i32_vec_ref_fmt_debug: extern "C" fn(_:  &AzI32VecRef) -> AzString,
        pub az_g_luint_vec_ref_delete: extern "C" fn(_:  &mut AzGLuintVecRef),
        pub az_g_luint_vec_ref_fmt_debug: extern "C" fn(_:  &AzGLuintVecRef) -> AzString,
        pub az_g_lenum_vec_ref_delete: extern "C" fn(_:  &mut AzGLenumVecRef),
        pub az_g_lenum_vec_ref_fmt_debug: extern "C" fn(_:  &AzGLenumVecRef) -> AzString,
        pub az_g_lint_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLintVecRefMut),
        pub az_g_lint_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLintVecRefMut) -> AzString,
        pub az_g_lint64_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLint64VecRefMut),
        pub az_g_lint64_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLint64VecRefMut) -> AzString,
        pub az_g_lboolean_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLbooleanVecRefMut),
        pub az_g_lboolean_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLbooleanVecRefMut) -> AzString,
        pub az_g_lfloat_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLfloatVecRefMut),
        pub az_g_lfloat_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLfloatVecRefMut) -> AzString,
        pub az_refstr_vec_ref_delete: extern "C" fn(_:  &mut AzRefstrVecRef),
        pub az_refstr_vec_ref_fmt_debug: extern "C" fn(_:  &AzRefstrVecRef) -> AzString,
        pub az_refstr_delete: extern "C" fn(_:  &mut AzRefstr),
        pub az_refstr_fmt_debug: extern "C" fn(_:  &AzRefstr) -> AzString,
        pub az_get_program_binary_return_delete: extern "C" fn(_:  &mut AzGetProgramBinaryReturn),
        pub az_get_program_binary_return_deep_copy: extern "C" fn(_:  &AzGetProgramBinaryReturn) -> AzGetProgramBinaryReturn,
        pub az_get_program_binary_return_fmt_debug: extern "C" fn(_:  &AzGetProgramBinaryReturn) -> AzString,
        pub az_get_active_attrib_return_delete: extern "C" fn(_:  &mut AzGetActiveAttribReturn),
        pub az_get_active_attrib_return_deep_copy: extern "C" fn(_:  &AzGetActiveAttribReturn) -> AzGetActiveAttribReturn,
        pub az_get_active_attrib_return_fmt_debug: extern "C" fn(_:  &AzGetActiveAttribReturn) -> AzString,
        pub az_g_lsync_ptr_delete: extern "C" fn(_:  &mut AzGLsyncPtr),
        pub az_g_lsync_ptr_fmt_debug: extern "C" fn(_:  &AzGLsyncPtr) -> AzString,
        pub az_get_active_uniform_return_delete: extern "C" fn(_:  &mut AzGetActiveUniformReturn),
        pub az_get_active_uniform_return_deep_copy: extern "C" fn(_:  &AzGetActiveUniformReturn) -> AzGetActiveUniformReturn,
        pub az_get_active_uniform_return_fmt_debug: extern "C" fn(_:  &AzGetActiveUniformReturn) -> AzString,
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
        pub az_gl_context_ptr_fmt_debug: extern "C" fn(_:  &AzGlContextPtr) -> AzString,
        pub az_texture_delete: extern "C" fn(_:  &mut AzTexture),
        pub az_texture_fmt_debug: extern "C" fn(_:  &AzTexture) -> AzString,
        pub az_texture_flags_default: extern "C" fn() -> AzTextureFlags,
        pub az_texture_flags_delete: extern "C" fn(_:  &mut AzTextureFlags),
        pub az_texture_flags_deep_copy: extern "C" fn(_:  &AzTextureFlags) -> AzTextureFlags,
        pub az_texture_flags_fmt_debug: extern "C" fn(_:  &AzTextureFlags) -> AzString,
        pub az_text_id_new: extern "C" fn() -> AzTextId,
        pub az_text_id_delete: extern "C" fn(_:  &mut AzTextId),
        pub az_text_id_deep_copy: extern "C" fn(_:  &AzTextId) -> AzTextId,
        pub az_text_id_fmt_debug: extern "C" fn(_:  &AzTextId) -> AzString,
        pub az_image_id_new: extern "C" fn() -> AzImageId,
        pub az_image_id_delete: extern "C" fn(_:  &mut AzImageId),
        pub az_image_id_deep_copy: extern "C" fn(_:  &AzImageId) -> AzImageId,
        pub az_image_id_fmt_debug: extern "C" fn(_:  &AzImageId) -> AzString,
        pub az_image_id_partial_eq: extern "C" fn(_:  &AzImageId, _:  &AzImageId) -> bool,
        pub az_image_id_partial_cmp: extern "C" fn(_:  &AzImageId, _:  &AzImageId) -> u8,
        pub az_image_id_cmp: extern "C" fn(_:  &AzImageId, _:  &AzImageId) -> u8,
        pub az_image_id_hash: extern "C" fn(_:  &AzImageId) -> u64,
        pub az_font_id_new: extern "C" fn() -> AzFontId,
        pub az_font_id_delete: extern "C" fn(_:  &mut AzFontId),
        pub az_font_id_deep_copy: extern "C" fn(_:  &AzFontId) -> AzFontId,
        pub az_font_id_fmt_debug: extern "C" fn(_:  &AzFontId) -> AzString,
        pub az_image_source_delete: extern "C" fn(_:  &mut AzImageSource),
        pub az_image_source_deep_copy: extern "C" fn(_:  &AzImageSource) -> AzImageSource,
        pub az_image_source_fmt_debug: extern "C" fn(_:  &AzImageSource) -> AzString,
        pub az_font_source_delete: extern "C" fn(_:  &mut AzFontSource),
        pub az_font_source_deep_copy: extern "C" fn(_:  &AzFontSource) -> AzFontSource,
        pub az_font_source_fmt_debug: extern "C" fn(_:  &AzFontSource) -> AzString,
        pub az_raw_image_new: extern "C" fn(_:  AzU8Vec, _:  usize, _:  usize, _:  AzRawImageFormat) -> AzRawImage,
        pub az_raw_image_delete: extern "C" fn(_:  &mut AzRawImage),
        pub az_raw_image_deep_copy: extern "C" fn(_:  &AzRawImage) -> AzRawImage,
        pub az_raw_image_fmt_debug: extern "C" fn(_:  &AzRawImage) -> AzString,
        pub az_raw_image_format_delete: extern "C" fn(_:  &mut AzRawImageFormat),
        pub az_raw_image_format_deep_copy: extern "C" fn(_:  &AzRawImageFormat) -> AzRawImageFormat,
        pub az_raw_image_format_fmt_debug: extern "C" fn(_:  &AzRawImageFormat) -> AzString,
        pub az_svg_multi_polygon_delete: extern "C" fn(_:  &mut AzSvgMultiPolygon),
        pub az_svg_multi_polygon_deep_copy: extern "C" fn(_:  &AzSvgMultiPolygon) -> AzSvgMultiPolygon,
        pub az_svg_multi_polygon_fmt_debug: extern "C" fn(_:  &AzSvgMultiPolygon) -> AzString,
        pub az_svg_node_delete: extern "C" fn(_:  &mut AzSvgNode),
        pub az_svg_node_deep_copy: extern "C" fn(_:  &AzSvgNode) -> AzSvgNode,
        pub az_svg_node_fmt_debug: extern "C" fn(_:  &AzSvgNode) -> AzString,
        pub az_svg_styled_node_delete: extern "C" fn(_:  &mut AzSvgStyledNode),
        pub az_svg_styled_node_deep_copy: extern "C" fn(_:  &AzSvgStyledNode) -> AzSvgStyledNode,
        pub az_svg_styled_node_fmt_debug: extern "C" fn(_:  &AzSvgStyledNode) -> AzString,
        pub az_svg_circle_delete: extern "C" fn(_:  &mut AzSvgCircle),
        pub az_svg_circle_deep_copy: extern "C" fn(_:  &AzSvgCircle) -> AzSvgCircle,
        pub az_svg_circle_fmt_debug: extern "C" fn(_:  &AzSvgCircle) -> AzString,
        pub az_svg_path_delete: extern "C" fn(_:  &mut AzSvgPath),
        pub az_svg_path_deep_copy: extern "C" fn(_:  &AzSvgPath) -> AzSvgPath,
        pub az_svg_path_fmt_debug: extern "C" fn(_:  &AzSvgPath) -> AzString,
        pub az_svg_path_element_delete: extern "C" fn(_:  &mut AzSvgPathElement),
        pub az_svg_path_element_deep_copy: extern "C" fn(_:  &AzSvgPathElement) -> AzSvgPathElement,
        pub az_svg_path_element_fmt_debug: extern "C" fn(_:  &AzSvgPathElement) -> AzString,
        pub az_svg_line_delete: extern "C" fn(_:  &mut AzSvgLine),
        pub az_svg_line_deep_copy: extern "C" fn(_:  &AzSvgLine) -> AzSvgLine,
        pub az_svg_line_fmt_debug: extern "C" fn(_:  &AzSvgLine) -> AzString,
        pub az_svg_point_delete: extern "C" fn(_:  &mut AzSvgPoint),
        pub az_svg_point_deep_copy: extern "C" fn(_:  &AzSvgPoint) -> AzSvgPoint,
        pub az_svg_point_fmt_debug: extern "C" fn(_:  &AzSvgPoint) -> AzString,
        pub az_svg_vertex_delete: extern "C" fn(_:  &mut AzSvgVertex),
        pub az_svg_vertex_deep_copy: extern "C" fn(_:  &AzSvgVertex) -> AzSvgVertex,
        pub az_svg_vertex_fmt_debug: extern "C" fn(_:  &AzSvgVertex) -> AzString,
        pub az_svg_quadratic_curve_delete: extern "C" fn(_:  &mut AzSvgQuadraticCurve),
        pub az_svg_quadratic_curve_deep_copy: extern "C" fn(_:  &AzSvgQuadraticCurve) -> AzSvgQuadraticCurve,
        pub az_svg_quadratic_curve_fmt_debug: extern "C" fn(_:  &AzSvgQuadraticCurve) -> AzString,
        pub az_svg_cubic_curve_delete: extern "C" fn(_:  &mut AzSvgCubicCurve),
        pub az_svg_cubic_curve_deep_copy: extern "C" fn(_:  &AzSvgCubicCurve) -> AzSvgCubicCurve,
        pub az_svg_cubic_curve_fmt_debug: extern "C" fn(_:  &AzSvgCubicCurve) -> AzString,
        pub az_svg_rect_delete: extern "C" fn(_:  &mut AzSvgRect),
        pub az_svg_rect_deep_copy: extern "C" fn(_:  &AzSvgRect) -> AzSvgRect,
        pub az_svg_rect_fmt_debug: extern "C" fn(_:  &AzSvgRect) -> AzString,
        pub az_tesselated_cpu_svg_node_delete: extern "C" fn(_:  &mut AzTesselatedCPUSvgNode),
        pub az_tesselated_cpu_svg_node_deep_copy: extern "C" fn(_:  &AzTesselatedCPUSvgNode) -> AzTesselatedCPUSvgNode,
        pub az_tesselated_cpu_svg_node_fmt_debug: extern "C" fn(_:  &AzTesselatedCPUSvgNode) -> AzString,
        pub az_tesselated_gpu_svg_node_delete: extern "C" fn(_:  &mut AzTesselatedGPUSvgNode),
        pub az_tesselated_gpu_svg_node_fmt_debug: extern "C" fn(_:  &AzTesselatedGPUSvgNode) -> AzString,
        pub az_svg_line_cap_delete: extern "C" fn(_:  &mut AzSvgLineCap),
        pub az_svg_line_cap_deep_copy: extern "C" fn(_:  &AzSvgLineCap) -> AzSvgLineCap,
        pub az_svg_line_cap_fmt_debug: extern "C" fn(_:  &AzSvgLineCap) -> AzString,
        pub az_svg_parse_options_default: extern "C" fn() -> AzSvgParseOptions,
        pub az_svg_parse_options_delete: extern "C" fn(_:  &mut AzSvgParseOptions),
        pub az_svg_parse_options_deep_copy: extern "C" fn(_:  &AzSvgParseOptions) -> AzSvgParseOptions,
        pub az_svg_parse_options_fmt_debug: extern "C" fn(_:  &AzSvgParseOptions) -> AzString,
        pub az_shape_rendering_delete: extern "C" fn(_:  &mut AzShapeRendering),
        pub az_shape_rendering_deep_copy: extern "C" fn(_:  &AzShapeRendering) -> AzShapeRendering,
        pub az_shape_rendering_fmt_debug: extern "C" fn(_:  &AzShapeRendering) -> AzString,
        pub az_text_rendering_delete: extern "C" fn(_:  &mut AzTextRendering),
        pub az_text_rendering_deep_copy: extern "C" fn(_:  &AzTextRendering) -> AzTextRendering,
        pub az_text_rendering_fmt_debug: extern "C" fn(_:  &AzTextRendering) -> AzString,
        pub az_image_rendering_delete: extern "C" fn(_:  &mut AzImageRendering),
        pub az_image_rendering_deep_copy: extern "C" fn(_:  &AzImageRendering) -> AzImageRendering,
        pub az_image_rendering_fmt_debug: extern "C" fn(_:  &AzImageRendering) -> AzString,
        pub az_font_database_delete: extern "C" fn(_:  &mut AzFontDatabase),
        pub az_font_database_deep_copy: extern "C" fn(_:  &AzFontDatabase) -> AzFontDatabase,
        pub az_font_database_fmt_debug: extern "C" fn(_:  &AzFontDatabase) -> AzString,
        pub az_svg_render_options_default: extern "C" fn() -> AzSvgRenderOptions,
        pub az_svg_render_options_delete: extern "C" fn(_:  &mut AzSvgRenderOptions),
        pub az_svg_render_options_deep_copy: extern "C" fn(_:  &AzSvgRenderOptions) -> AzSvgRenderOptions,
        pub az_svg_render_options_fmt_debug: extern "C" fn(_:  &AzSvgRenderOptions) -> AzString,
        pub az_svg_fit_to_delete: extern "C" fn(_:  &mut AzSvgFitTo),
        pub az_svg_fit_to_deep_copy: extern "C" fn(_:  &AzSvgFitTo) -> AzSvgFitTo,
        pub az_svg_fit_to_fmt_debug: extern "C" fn(_:  &AzSvgFitTo) -> AzString,
        pub az_svg_parse: extern "C" fn(_:  AzU8VecRef, _:  AzSvgParseOptions) -> AzResultSvgSvgParseError,
        pub az_svg_delete: extern "C" fn(_:  &mut AzSvg),
        pub az_svg_deep_copy: extern "C" fn(_:  &AzSvg) -> AzSvg,
        pub az_svg_fmt_debug: extern "C" fn(_:  &AzSvg) -> AzString,
        pub az_svg_xml_node_delete: extern "C" fn(_:  &mut AzSvgXmlNode),
        pub az_svg_xml_node_deep_copy: extern "C" fn(_:  &AzSvgXmlNode) -> AzSvgXmlNode,
        pub az_svg_xml_node_fmt_debug: extern "C" fn(_:  &AzSvgXmlNode) -> AzString,
        pub az_svg_line_join_delete: extern "C" fn(_:  &mut AzSvgLineJoin),
        pub az_svg_line_join_deep_copy: extern "C" fn(_:  &AzSvgLineJoin) -> AzSvgLineJoin,
        pub az_svg_line_join_fmt_debug: extern "C" fn(_:  &AzSvgLineJoin) -> AzString,
        pub az_svg_dash_pattern_delete: extern "C" fn(_:  &mut AzSvgDashPattern),
        pub az_svg_dash_pattern_deep_copy: extern "C" fn(_:  &AzSvgDashPattern) -> AzSvgDashPattern,
        pub az_svg_dash_pattern_fmt_debug: extern "C" fn(_:  &AzSvgDashPattern) -> AzString,
        pub az_svg_style_delete: extern "C" fn(_:  &mut AzSvgStyle),
        pub az_svg_style_deep_copy: extern "C" fn(_:  &AzSvgStyle) -> AzSvgStyle,
        pub az_svg_style_fmt_debug: extern "C" fn(_:  &AzSvgStyle) -> AzString,
        pub az_svg_fill_style_delete: extern "C" fn(_:  &mut AzSvgFillStyle),
        pub az_svg_fill_style_deep_copy: extern "C" fn(_:  &AzSvgFillStyle) -> AzSvgFillStyle,
        pub az_svg_fill_style_fmt_debug: extern "C" fn(_:  &AzSvgFillStyle) -> AzString,
        pub az_svg_stroke_style_delete: extern "C" fn(_:  &mut AzSvgStrokeStyle),
        pub az_svg_stroke_style_deep_copy: extern "C" fn(_:  &AzSvgStrokeStyle) -> AzSvgStrokeStyle,
        pub az_svg_stroke_style_fmt_debug: extern "C" fn(_:  &AzSvgStrokeStyle) -> AzString,
        pub az_svg_node_id_delete: extern "C" fn(_:  &mut AzSvgNodeId),
        pub az_svg_node_id_deep_copy: extern "C" fn(_:  &AzSvgNodeId) -> AzSvgNodeId,
        pub az_svg_node_id_fmt_debug: extern "C" fn(_:  &AzSvgNodeId) -> AzString,
        pub az_drop_check_ptr_ptr_delete: extern "C" fn(_:  &mut AzDropCheckPtrPtr),
        pub az_drop_check_ptr_ptr_fmt_debug: extern "C" fn(_:  &AzDropCheckPtrPtr) -> AzString,
        pub az_arc_mutex_ref_any_ptr_delete: extern "C" fn(_:  &mut AzArcMutexRefAnyPtr),
        pub az_arc_mutex_ref_any_ptr_fmt_debug: extern "C" fn(_:  &AzArcMutexRefAnyPtr) -> AzString,
        pub az_timer_delete: extern "C" fn(_:  &mut AzTimer),
        pub az_timer_deep_copy: extern "C" fn(_:  &AzTimer) -> AzTimer,
        pub az_timer_fmt_debug: extern "C" fn(_:  &AzTimer) -> AzString,
        pub az_task_ptr_new: extern "C" fn(_:  AzArcMutexRefAnyPtr, _:  AzTaskCallbackType) -> AzTaskPtr,
        pub az_task_ptr_then: extern "C" fn(_:  AzTaskPtr, _:  AzTimer) -> AzTaskPtr,
        pub az_task_ptr_delete: extern "C" fn(_:  &mut AzTaskPtr),
        pub az_task_ptr_fmt_debug: extern "C" fn(_:  &AzTaskPtr) -> AzString,
        pub az_thread_ptr_new: extern "C" fn(_:  AzRefAny, _:  AzThreadCallbackType) -> AzThreadPtr,
        pub az_thread_ptr_block: extern "C" fn(_:  AzThreadPtr) -> AzResultRefAnyBlockError,
        pub az_thread_ptr_delete: extern "C" fn(_:  &mut AzThreadPtr),
        pub az_thread_ptr_fmt_debug: extern "C" fn(_:  &AzThreadPtr) -> AzString,
        pub az_drop_check_ptr_delete: extern "C" fn(_:  &mut AzDropCheckPtr),
        pub az_drop_check_ptr_fmt_debug: extern "C" fn(_:  &AzDropCheckPtr) -> AzString,
        pub az_timer_id_delete: extern "C" fn(_:  &mut AzTimerId),
        pub az_timer_id_deep_copy: extern "C" fn(_:  &AzTimerId) -> AzTimerId,
        pub az_timer_id_fmt_debug: extern "C" fn(_:  &AzTimerId) -> AzString,
        pub az_terminate_timer_delete: extern "C" fn(_:  &mut AzTerminateTimer),
        pub az_terminate_timer_deep_copy: extern "C" fn(_:  &AzTerminateTimer) -> AzTerminateTimer,
        pub az_terminate_timer_fmt_debug: extern "C" fn(_:  &AzTerminateTimer) -> AzString,
        pub az_block_error_delete: extern "C" fn(_:  &mut AzBlockError),
        pub az_block_error_deep_copy: extern "C" fn(_:  &AzBlockError) -> AzBlockError,
        pub az_block_error_fmt_debug: extern "C" fn(_:  &AzBlockError) -> AzString,
        pub az_task_bar_icon_delete: extern "C" fn(_:  &mut AzTaskBarIcon),
        pub az_task_bar_icon_deep_copy: extern "C" fn(_:  &AzTaskBarIcon) -> AzTaskBarIcon,
        pub az_task_bar_icon_fmt_debug: extern "C" fn(_:  &AzTaskBarIcon) -> AzString,
        pub az_x_window_type_delete: extern "C" fn(_:  &mut AzXWindowType),
        pub az_x_window_type_deep_copy: extern "C" fn(_:  &AzXWindowType) -> AzXWindowType,
        pub az_x_window_type_fmt_debug: extern "C" fn(_:  &AzXWindowType) -> AzString,
        pub az_physical_position_i32_delete: extern "C" fn(_:  &mut AzPhysicalPositionI32),
        pub az_physical_position_i32_deep_copy: extern "C" fn(_:  &AzPhysicalPositionI32) -> AzPhysicalPositionI32,
        pub az_physical_position_i32_fmt_debug: extern "C" fn(_:  &AzPhysicalPositionI32) -> AzString,
        pub az_physical_position_i32_partial_eq: extern "C" fn(_:  &AzPhysicalPositionI32, _:  &AzPhysicalPositionI32) -> bool,
        pub az_physical_position_i32_partial_cmp: extern "C" fn(_:  &AzPhysicalPositionI32, _:  &AzPhysicalPositionI32) -> u8,
        pub az_physical_position_i32_cmp: extern "C" fn(_:  &AzPhysicalPositionI32, _:  &AzPhysicalPositionI32) -> u8,
        pub az_physical_position_i32_hash: extern "C" fn(_:  &AzPhysicalPositionI32) -> u64,
        pub az_physical_size_u32_delete: extern "C" fn(_:  &mut AzPhysicalSizeU32),
        pub az_physical_size_u32_deep_copy: extern "C" fn(_:  &AzPhysicalSizeU32) -> AzPhysicalSizeU32,
        pub az_physical_size_u32_fmt_debug: extern "C" fn(_:  &AzPhysicalSizeU32) -> AzString,
        pub az_physical_size_u32_partial_eq: extern "C" fn(_:  &AzPhysicalSizeU32, _:  &AzPhysicalSizeU32) -> bool,
        pub az_physical_size_u32_partial_cmp: extern "C" fn(_:  &AzPhysicalSizeU32, _:  &AzPhysicalSizeU32) -> u8,
        pub az_physical_size_u32_cmp: extern "C" fn(_:  &AzPhysicalSizeU32, _:  &AzPhysicalSizeU32) -> u8,
        pub az_physical_size_u32_hash: extern "C" fn(_:  &AzPhysicalSizeU32) -> u64,
        pub az_logical_position_delete: extern "C" fn(_:  &mut AzLogicalPosition),
        pub az_logical_position_deep_copy: extern "C" fn(_:  &AzLogicalPosition) -> AzLogicalPosition,
        pub az_logical_position_fmt_debug: extern "C" fn(_:  &AzLogicalPosition) -> AzString,
        pub az_logical_rect_delete: extern "C" fn(_:  &mut AzLogicalRect),
        pub az_logical_rect_deep_copy: extern "C" fn(_:  &AzLogicalRect) -> AzLogicalRect,
        pub az_logical_rect_fmt_debug: extern "C" fn(_:  &AzLogicalRect) -> AzString,
        pub az_icon_key_delete: extern "C" fn(_:  &mut AzIconKey),
        pub az_icon_key_deep_copy: extern "C" fn(_:  &AzIconKey) -> AzIconKey,
        pub az_icon_key_fmt_debug: extern "C" fn(_:  &AzIconKey) -> AzString,
        pub az_small_window_icon_bytes_delete: extern "C" fn(_:  &mut AzSmallWindowIconBytes),
        pub az_small_window_icon_bytes_deep_copy: extern "C" fn(_:  &AzSmallWindowIconBytes) -> AzSmallWindowIconBytes,
        pub az_small_window_icon_bytes_fmt_debug: extern "C" fn(_:  &AzSmallWindowIconBytes) -> AzString,
        pub az_large_window_icon_bytes_delete: extern "C" fn(_:  &mut AzLargeWindowIconBytes),
        pub az_large_window_icon_bytes_deep_copy: extern "C" fn(_:  &AzLargeWindowIconBytes) -> AzLargeWindowIconBytes,
        pub az_large_window_icon_bytes_fmt_debug: extern "C" fn(_:  &AzLargeWindowIconBytes) -> AzString,
        pub az_window_icon_delete: extern "C" fn(_:  &mut AzWindowIcon),
        pub az_window_icon_deep_copy: extern "C" fn(_:  &AzWindowIcon) -> AzWindowIcon,
        pub az_window_icon_fmt_debug: extern "C" fn(_:  &AzWindowIcon) -> AzString,
        pub az_virtual_key_code_delete: extern "C" fn(_:  &mut AzVirtualKeyCode),
        pub az_virtual_key_code_deep_copy: extern "C" fn(_:  &AzVirtualKeyCode) -> AzVirtualKeyCode,
        pub az_virtual_key_code_fmt_debug: extern "C" fn(_:  &AzVirtualKeyCode) -> AzString,
        pub az_accelerator_key_delete: extern "C" fn(_:  &mut AzAcceleratorKey),
        pub az_accelerator_key_deep_copy: extern "C" fn(_:  &AzAcceleratorKey) -> AzAcceleratorKey,
        pub az_accelerator_key_fmt_debug: extern "C" fn(_:  &AzAcceleratorKey) -> AzString,
        pub az_window_size_delete: extern "C" fn(_:  &mut AzWindowSize),
        pub az_window_size_deep_copy: extern "C" fn(_:  &AzWindowSize) -> AzWindowSize,
        pub az_window_size_fmt_debug: extern "C" fn(_:  &AzWindowSize) -> AzString,
        pub az_window_flags_delete: extern "C" fn(_:  &mut AzWindowFlags),
        pub az_window_flags_deep_copy: extern "C" fn(_:  &AzWindowFlags) -> AzWindowFlags,
        pub az_window_flags_fmt_debug: extern "C" fn(_:  &AzWindowFlags) -> AzString,
        pub az_debug_state_delete: extern "C" fn(_:  &mut AzDebugState),
        pub az_debug_state_deep_copy: extern "C" fn(_:  &AzDebugState) -> AzDebugState,
        pub az_debug_state_fmt_debug: extern "C" fn(_:  &AzDebugState) -> AzString,
        pub az_keyboard_state_delete: extern "C" fn(_:  &mut AzKeyboardState),
        pub az_keyboard_state_deep_copy: extern "C" fn(_:  &AzKeyboardState) -> AzKeyboardState,
        pub az_keyboard_state_fmt_debug: extern "C" fn(_:  &AzKeyboardState) -> AzString,
        pub az_mouse_cursor_type_delete: extern "C" fn(_:  &mut AzMouseCursorType),
        pub az_mouse_cursor_type_deep_copy: extern "C" fn(_:  &AzMouseCursorType) -> AzMouseCursorType,
        pub az_mouse_cursor_type_fmt_debug: extern "C" fn(_:  &AzMouseCursorType) -> AzString,
        pub az_cursor_position_delete: extern "C" fn(_:  &mut AzCursorPosition),
        pub az_cursor_position_deep_copy: extern "C" fn(_:  &AzCursorPosition) -> AzCursorPosition,
        pub az_cursor_position_fmt_debug: extern "C" fn(_:  &AzCursorPosition) -> AzString,
        pub az_mouse_state_delete: extern "C" fn(_:  &mut AzMouseState),
        pub az_mouse_state_deep_copy: extern "C" fn(_:  &AzMouseState) -> AzMouseState,
        pub az_mouse_state_fmt_debug: extern "C" fn(_:  &AzMouseState) -> AzString,
        pub az_platform_specific_options_delete: extern "C" fn(_:  &mut AzPlatformSpecificOptions),
        pub az_platform_specific_options_deep_copy: extern "C" fn(_:  &AzPlatformSpecificOptions) -> AzPlatformSpecificOptions,
        pub az_platform_specific_options_fmt_debug: extern "C" fn(_:  &AzPlatformSpecificOptions) -> AzString,
        pub az_windows_window_options_delete: extern "C" fn(_:  &mut AzWindowsWindowOptions),
        pub az_windows_window_options_deep_copy: extern "C" fn(_:  &AzWindowsWindowOptions) -> AzWindowsWindowOptions,
        pub az_windows_window_options_fmt_debug: extern "C" fn(_:  &AzWindowsWindowOptions) -> AzString,
        pub az_wayland_theme_delete: extern "C" fn(_:  &mut AzWaylandTheme),
        pub az_wayland_theme_deep_copy: extern "C" fn(_:  &AzWaylandTheme) -> AzWaylandTheme,
        pub az_wayland_theme_fmt_debug: extern "C" fn(_:  &AzWaylandTheme) -> AzString,
        pub az_renderer_type_delete: extern "C" fn(_:  &mut AzRendererType),
        pub az_renderer_type_deep_copy: extern "C" fn(_:  &AzRendererType) -> AzRendererType,
        pub az_renderer_type_fmt_debug: extern "C" fn(_:  &AzRendererType) -> AzString,
        pub az_string_pair_delete: extern "C" fn(_:  &mut AzStringPair),
        pub az_string_pair_deep_copy: extern "C" fn(_:  &AzStringPair) -> AzStringPair,
        pub az_string_pair_fmt_debug: extern "C" fn(_:  &AzStringPair) -> AzString,
        pub az_linux_window_options_delete: extern "C" fn(_:  &mut AzLinuxWindowOptions),
        pub az_linux_window_options_deep_copy: extern "C" fn(_:  &AzLinuxWindowOptions) -> AzLinuxWindowOptions,
        pub az_linux_window_options_fmt_debug: extern "C" fn(_:  &AzLinuxWindowOptions) -> AzString,
        pub az_mac_window_options_delete: extern "C" fn(_:  &mut AzMacWindowOptions),
        pub az_mac_window_options_deep_copy: extern "C" fn(_:  &AzMacWindowOptions) -> AzMacWindowOptions,
        pub az_mac_window_options_fmt_debug: extern "C" fn(_:  &AzMacWindowOptions) -> AzString,
        pub az_wasm_window_options_delete: extern "C" fn(_:  &mut AzWasmWindowOptions),
        pub az_wasm_window_options_deep_copy: extern "C" fn(_:  &AzWasmWindowOptions) -> AzWasmWindowOptions,
        pub az_wasm_window_options_fmt_debug: extern "C" fn(_:  &AzWasmWindowOptions) -> AzString,
        pub az_full_screen_mode_delete: extern "C" fn(_:  &mut AzFullScreenMode),
        pub az_full_screen_mode_deep_copy: extern "C" fn(_:  &AzFullScreenMode) -> AzFullScreenMode,
        pub az_full_screen_mode_fmt_debug: extern "C" fn(_:  &AzFullScreenMode) -> AzString,
        pub az_window_state_new: extern "C" fn(_:  AzLayoutCallbackType, _:  AzCss) -> AzWindowState,
        pub az_window_state_delete: extern "C" fn(_:  &mut AzWindowState),
        pub az_window_state_deep_copy: extern "C" fn(_:  &AzWindowState) -> AzWindowState,
        pub az_window_state_fmt_debug: extern "C" fn(_:  &AzWindowState) -> AzString,
        pub az_logical_size_delete: extern "C" fn(_:  &mut AzLogicalSize),
        pub az_logical_size_deep_copy: extern "C" fn(_:  &AzLogicalSize) -> AzLogicalSize,
        pub az_logical_size_fmt_debug: extern "C" fn(_:  &AzLogicalSize) -> AzString,
        pub az_hot_reload_options_delete: extern "C" fn(_:  &mut AzHotReloadOptions),
        pub az_hot_reload_options_deep_copy: extern "C" fn(_:  &AzHotReloadOptions) -> AzHotReloadOptions,
        pub az_hot_reload_options_fmt_debug: extern "C" fn(_:  &AzHotReloadOptions) -> AzString,
        pub az_window_create_options_new: extern "C" fn(_:  AzLayoutCallbackType, _:  AzCss) -> AzWindowCreateOptions,
        pub az_window_create_options_new_hot_reload: extern "C" fn(_:  AzLayoutCallbackType, _:  AzHotReloadOptions) -> AzWindowCreateOptions,
        pub az_window_create_options_delete: extern "C" fn(_:  &mut AzWindowCreateOptions),
        pub az_window_create_options_deep_copy: extern "C" fn(_:  &AzWindowCreateOptions) -> AzWindowCreateOptions,
        pub az_window_create_options_fmt_debug: extern "C" fn(_:  &AzWindowCreateOptions) -> AzString,
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
            let az_string_fmt_debug: extern "C" fn(_:  &AzString) -> AzString = transmute(lib.get(b"az_string_fmt_debug")?);
            let az_string_partial_eq: extern "C" fn(_:  &AzString, _:  &AzString) -> bool = transmute(lib.get(b"az_string_partial_eq")?);
            let az_string_partial_cmp: extern "C" fn(_:  &AzString, _:  &AzString) -> u8 = transmute(lib.get(b"az_string_partial_cmp")?);
            let az_string_cmp: extern "C" fn(_:  &AzString, _:  &AzString) -> u8 = transmute(lib.get(b"az_string_cmp")?);
            let az_string_hash: extern "C" fn(_:  &AzString) -> u64 = transmute(lib.get(b"az_string_hash")?);
            let az_css_property_vec_new: extern "C" fn() -> AzCssPropertyVec = transmute(lib.get(b"az_css_property_vec_new")?);
            let az_css_property_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssPropertyVec = transmute(lib.get(b"az_css_property_vec_with_capacity")?);
            let az_css_property_vec_copy_from: extern "C" fn(_:  *const AzCssProperty, _:  usize) -> AzCssPropertyVec = transmute(lib.get(b"az_css_property_vec_copy_from")?);
            let az_css_property_vec_delete: extern "C" fn(_:  &mut AzCssPropertyVec) = transmute(lib.get(b"az_css_property_vec_delete")?);
            let az_css_property_vec_deep_copy: extern "C" fn(_:  &AzCssPropertyVec) -> AzCssPropertyVec = transmute(lib.get(b"az_css_property_vec_deep_copy")?);
            let az_css_property_vec_fmt_debug: extern "C" fn(_:  &AzCssPropertyVec) -> AzString = transmute(lib.get(b"az_css_property_vec_fmt_debug")?);
            let az_svg_multi_polygon_vec_new: extern "C" fn() -> AzSvgMultiPolygonVec = transmute(lib.get(b"az_svg_multi_polygon_vec_new")?);
            let az_svg_multi_polygon_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgMultiPolygonVec = transmute(lib.get(b"az_svg_multi_polygon_vec_with_capacity")?);
            let az_svg_multi_polygon_vec_copy_from: extern "C" fn(_:  *const AzSvgMultiPolygon, _:  usize) -> AzSvgMultiPolygonVec = transmute(lib.get(b"az_svg_multi_polygon_vec_copy_from")?);
            let az_svg_multi_polygon_vec_delete: extern "C" fn(_:  &mut AzSvgMultiPolygonVec) = transmute(lib.get(b"az_svg_multi_polygon_vec_delete")?);
            let az_svg_multi_polygon_vec_deep_copy: extern "C" fn(_:  &AzSvgMultiPolygonVec) -> AzSvgMultiPolygonVec = transmute(lib.get(b"az_svg_multi_polygon_vec_deep_copy")?);
            let az_svg_multi_polygon_vec_fmt_debug: extern "C" fn(_:  &AzSvgMultiPolygonVec) -> AzString = transmute(lib.get(b"az_svg_multi_polygon_vec_fmt_debug")?);
            let az_svg_path_vec_new: extern "C" fn() -> AzSvgPathVec = transmute(lib.get(b"az_svg_path_vec_new")?);
            let az_svg_path_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgPathVec = transmute(lib.get(b"az_svg_path_vec_with_capacity")?);
            let az_svg_path_vec_copy_from: extern "C" fn(_:  *const AzSvgPath, _:  usize) -> AzSvgPathVec = transmute(lib.get(b"az_svg_path_vec_copy_from")?);
            let az_svg_path_vec_delete: extern "C" fn(_:  &mut AzSvgPathVec) = transmute(lib.get(b"az_svg_path_vec_delete")?);
            let az_svg_path_vec_deep_copy: extern "C" fn(_:  &AzSvgPathVec) -> AzSvgPathVec = transmute(lib.get(b"az_svg_path_vec_deep_copy")?);
            let az_svg_path_vec_fmt_debug: extern "C" fn(_:  &AzSvgPathVec) -> AzString = transmute(lib.get(b"az_svg_path_vec_fmt_debug")?);
            let az_vertex_attribute_vec_new: extern "C" fn() -> AzVertexAttributeVec = transmute(lib.get(b"az_vertex_attribute_vec_new")?);
            let az_vertex_attribute_vec_with_capacity: extern "C" fn(_:  usize) -> AzVertexAttributeVec = transmute(lib.get(b"az_vertex_attribute_vec_with_capacity")?);
            let az_vertex_attribute_vec_copy_from: extern "C" fn(_:  *const AzVertexAttribute, _:  usize) -> AzVertexAttributeVec = transmute(lib.get(b"az_vertex_attribute_vec_copy_from")?);
            let az_vertex_attribute_vec_delete: extern "C" fn(_:  &mut AzVertexAttributeVec) = transmute(lib.get(b"az_vertex_attribute_vec_delete")?);
            let az_vertex_attribute_vec_deep_copy: extern "C" fn(_:  &AzVertexAttributeVec) -> AzVertexAttributeVec = transmute(lib.get(b"az_vertex_attribute_vec_deep_copy")?);
            let az_vertex_attribute_vec_fmt_debug: extern "C" fn(_:  &AzVertexAttributeVec) -> AzString = transmute(lib.get(b"az_vertex_attribute_vec_fmt_debug")?);
            let az_svg_path_element_vec_new: extern "C" fn() -> AzSvgPathElementVec = transmute(lib.get(b"az_svg_path_element_vec_new")?);
            let az_svg_path_element_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgPathElementVec = transmute(lib.get(b"az_svg_path_element_vec_with_capacity")?);
            let az_svg_path_element_vec_copy_from: extern "C" fn(_:  *const AzSvgPathElement, _:  usize) -> AzSvgPathElementVec = transmute(lib.get(b"az_svg_path_element_vec_copy_from")?);
            let az_svg_path_element_vec_delete: extern "C" fn(_:  &mut AzSvgPathElementVec) = transmute(lib.get(b"az_svg_path_element_vec_delete")?);
            let az_svg_path_element_vec_deep_copy: extern "C" fn(_:  &AzSvgPathElementVec) -> AzSvgPathElementVec = transmute(lib.get(b"az_svg_path_element_vec_deep_copy")?);
            let az_svg_path_element_vec_fmt_debug: extern "C" fn(_:  &AzSvgPathElementVec) -> AzString = transmute(lib.get(b"az_svg_path_element_vec_fmt_debug")?);
            let az_svg_vertex_vec_new: extern "C" fn() -> AzSvgVertexVec = transmute(lib.get(b"az_svg_vertex_vec_new")?);
            let az_svg_vertex_vec_with_capacity: extern "C" fn(_:  usize) -> AzSvgVertexVec = transmute(lib.get(b"az_svg_vertex_vec_with_capacity")?);
            let az_svg_vertex_vec_copy_from: extern "C" fn(_:  *const AzSvgVertex, _:  usize) -> AzSvgVertexVec = transmute(lib.get(b"az_svg_vertex_vec_copy_from")?);
            let az_svg_vertex_vec_delete: extern "C" fn(_:  &mut AzSvgVertexVec) = transmute(lib.get(b"az_svg_vertex_vec_delete")?);
            let az_svg_vertex_vec_deep_copy: extern "C" fn(_:  &AzSvgVertexVec) -> AzSvgVertexVec = transmute(lib.get(b"az_svg_vertex_vec_deep_copy")?);
            let az_svg_vertex_vec_fmt_debug: extern "C" fn(_:  &AzSvgVertexVec) -> AzString = transmute(lib.get(b"az_svg_vertex_vec_fmt_debug")?);
            let az_u32_vec_new: extern "C" fn() -> AzU32Vec = transmute(lib.get(b"az_u32_vec_new")?);
            let az_u32_vec_with_capacity: extern "C" fn(_:  usize) -> AzU32Vec = transmute(lib.get(b"az_u32_vec_with_capacity")?);
            let az_u32_vec_copy_from: extern "C" fn(_:  *const u32, _:  usize) -> AzU32Vec = transmute(lib.get(b"az_u32_vec_copy_from")?);
            let az_u32_vec_delete: extern "C" fn(_:  &mut AzU32Vec) = transmute(lib.get(b"az_u32_vec_delete")?);
            let az_u32_vec_deep_copy: extern "C" fn(_:  &AzU32Vec) -> AzU32Vec = transmute(lib.get(b"az_u32_vec_deep_copy")?);
            let az_u32_vec_fmt_debug: extern "C" fn(_:  &AzU32Vec) -> AzString = transmute(lib.get(b"az_u32_vec_fmt_debug")?);
            let az_x_window_type_vec_new: extern "C" fn() -> AzXWindowTypeVec = transmute(lib.get(b"az_x_window_type_vec_new")?);
            let az_x_window_type_vec_with_capacity: extern "C" fn(_:  usize) -> AzXWindowTypeVec = transmute(lib.get(b"az_x_window_type_vec_with_capacity")?);
            let az_x_window_type_vec_copy_from: extern "C" fn(_:  *const AzXWindowType, _:  usize) -> AzXWindowTypeVec = transmute(lib.get(b"az_x_window_type_vec_copy_from")?);
            let az_x_window_type_vec_delete: extern "C" fn(_:  &mut AzXWindowTypeVec) = transmute(lib.get(b"az_x_window_type_vec_delete")?);
            let az_x_window_type_vec_deep_copy: extern "C" fn(_:  &AzXWindowTypeVec) -> AzXWindowTypeVec = transmute(lib.get(b"az_x_window_type_vec_deep_copy")?);
            let az_x_window_type_vec_fmt_debug: extern "C" fn(_:  &AzXWindowTypeVec) -> AzString = transmute(lib.get(b"az_x_window_type_vec_fmt_debug")?);
            let az_virtual_key_code_vec_new: extern "C" fn() -> AzVirtualKeyCodeVec = transmute(lib.get(b"az_virtual_key_code_vec_new")?);
            let az_virtual_key_code_vec_with_capacity: extern "C" fn(_:  usize) -> AzVirtualKeyCodeVec = transmute(lib.get(b"az_virtual_key_code_vec_with_capacity")?);
            let az_virtual_key_code_vec_copy_from: extern "C" fn(_:  *const AzVirtualKeyCode, _:  usize) -> AzVirtualKeyCodeVec = transmute(lib.get(b"az_virtual_key_code_vec_copy_from")?);
            let az_virtual_key_code_vec_delete: extern "C" fn(_:  &mut AzVirtualKeyCodeVec) = transmute(lib.get(b"az_virtual_key_code_vec_delete")?);
            let az_virtual_key_code_vec_deep_copy: extern "C" fn(_:  &AzVirtualKeyCodeVec) -> AzVirtualKeyCodeVec = transmute(lib.get(b"az_virtual_key_code_vec_deep_copy")?);
            let az_virtual_key_code_vec_fmt_debug: extern "C" fn(_:  &AzVirtualKeyCodeVec) -> AzString = transmute(lib.get(b"az_virtual_key_code_vec_fmt_debug")?);
            let az_scan_code_vec_new: extern "C" fn() -> AzScanCodeVec = transmute(lib.get(b"az_scan_code_vec_new")?);
            let az_scan_code_vec_with_capacity: extern "C" fn(_:  usize) -> AzScanCodeVec = transmute(lib.get(b"az_scan_code_vec_with_capacity")?);
            let az_scan_code_vec_copy_from: extern "C" fn(_:  *const u32, _:  usize) -> AzScanCodeVec = transmute(lib.get(b"az_scan_code_vec_copy_from")?);
            let az_scan_code_vec_delete: extern "C" fn(_:  &mut AzScanCodeVec) = transmute(lib.get(b"az_scan_code_vec_delete")?);
            let az_scan_code_vec_deep_copy: extern "C" fn(_:  &AzScanCodeVec) -> AzScanCodeVec = transmute(lib.get(b"az_scan_code_vec_deep_copy")?);
            let az_scan_code_vec_fmt_debug: extern "C" fn(_:  &AzScanCodeVec) -> AzString = transmute(lib.get(b"az_scan_code_vec_fmt_debug")?);
            let az_css_declaration_vec_new: extern "C" fn() -> AzCssDeclarationVec = transmute(lib.get(b"az_css_declaration_vec_new")?);
            let az_css_declaration_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssDeclarationVec = transmute(lib.get(b"az_css_declaration_vec_with_capacity")?);
            let az_css_declaration_vec_copy_from: extern "C" fn(_:  *const AzCssDeclaration, _:  usize) -> AzCssDeclarationVec = transmute(lib.get(b"az_css_declaration_vec_copy_from")?);
            let az_css_declaration_vec_delete: extern "C" fn(_:  &mut AzCssDeclarationVec) = transmute(lib.get(b"az_css_declaration_vec_delete")?);
            let az_css_declaration_vec_deep_copy: extern "C" fn(_:  &AzCssDeclarationVec) -> AzCssDeclarationVec = transmute(lib.get(b"az_css_declaration_vec_deep_copy")?);
            let az_css_declaration_vec_fmt_debug: extern "C" fn(_:  &AzCssDeclarationVec) -> AzString = transmute(lib.get(b"az_css_declaration_vec_fmt_debug")?);
            let az_css_path_selector_vec_new: extern "C" fn() -> AzCssPathSelectorVec = transmute(lib.get(b"az_css_path_selector_vec_new")?);
            let az_css_path_selector_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssPathSelectorVec = transmute(lib.get(b"az_css_path_selector_vec_with_capacity")?);
            let az_css_path_selector_vec_copy_from: extern "C" fn(_:  *const AzCssPathSelector, _:  usize) -> AzCssPathSelectorVec = transmute(lib.get(b"az_css_path_selector_vec_copy_from")?);
            let az_css_path_selector_vec_delete: extern "C" fn(_:  &mut AzCssPathSelectorVec) = transmute(lib.get(b"az_css_path_selector_vec_delete")?);
            let az_css_path_selector_vec_deep_copy: extern "C" fn(_:  &AzCssPathSelectorVec) -> AzCssPathSelectorVec = transmute(lib.get(b"az_css_path_selector_vec_deep_copy")?);
            let az_css_path_selector_vec_fmt_debug: extern "C" fn(_:  &AzCssPathSelectorVec) -> AzString = transmute(lib.get(b"az_css_path_selector_vec_fmt_debug")?);
            let az_stylesheet_vec_new: extern "C" fn() -> AzStylesheetVec = transmute(lib.get(b"az_stylesheet_vec_new")?);
            let az_stylesheet_vec_with_capacity: extern "C" fn(_:  usize) -> AzStylesheetVec = transmute(lib.get(b"az_stylesheet_vec_with_capacity")?);
            let az_stylesheet_vec_copy_from: extern "C" fn(_:  *const AzStylesheet, _:  usize) -> AzStylesheetVec = transmute(lib.get(b"az_stylesheet_vec_copy_from")?);
            let az_stylesheet_vec_delete: extern "C" fn(_:  &mut AzStylesheetVec) = transmute(lib.get(b"az_stylesheet_vec_delete")?);
            let az_stylesheet_vec_deep_copy: extern "C" fn(_:  &AzStylesheetVec) -> AzStylesheetVec = transmute(lib.get(b"az_stylesheet_vec_deep_copy")?);
            let az_stylesheet_vec_fmt_debug: extern "C" fn(_:  &AzStylesheetVec) -> AzString = transmute(lib.get(b"az_stylesheet_vec_fmt_debug")?);
            let az_css_rule_block_vec_new: extern "C" fn() -> AzCssRuleBlockVec = transmute(lib.get(b"az_css_rule_block_vec_new")?);
            let az_css_rule_block_vec_with_capacity: extern "C" fn(_:  usize) -> AzCssRuleBlockVec = transmute(lib.get(b"az_css_rule_block_vec_with_capacity")?);
            let az_css_rule_block_vec_copy_from: extern "C" fn(_:  *const AzCssRuleBlock, _:  usize) -> AzCssRuleBlockVec = transmute(lib.get(b"az_css_rule_block_vec_copy_from")?);
            let az_css_rule_block_vec_delete: extern "C" fn(_:  &mut AzCssRuleBlockVec) = transmute(lib.get(b"az_css_rule_block_vec_delete")?);
            let az_css_rule_block_vec_deep_copy: extern "C" fn(_:  &AzCssRuleBlockVec) -> AzCssRuleBlockVec = transmute(lib.get(b"az_css_rule_block_vec_deep_copy")?);
            let az_css_rule_block_vec_fmt_debug: extern "C" fn(_:  &AzCssRuleBlockVec) -> AzString = transmute(lib.get(b"az_css_rule_block_vec_fmt_debug")?);
            let az_u8_vec_new: extern "C" fn() -> AzU8Vec = transmute(lib.get(b"az_u8_vec_new")?);
            let az_u8_vec_with_capacity: extern "C" fn(_:  usize) -> AzU8Vec = transmute(lib.get(b"az_u8_vec_with_capacity")?);
            let az_u8_vec_copy_from: extern "C" fn(_:  *const u8, _:  usize) -> AzU8Vec = transmute(lib.get(b"az_u8_vec_copy_from")?);
            let az_u8_vec_delete: extern "C" fn(_:  &mut AzU8Vec) = transmute(lib.get(b"az_u8_vec_delete")?);
            let az_u8_vec_deep_copy: extern "C" fn(_:  &AzU8Vec) -> AzU8Vec = transmute(lib.get(b"az_u8_vec_deep_copy")?);
            let az_u8_vec_fmt_debug: extern "C" fn(_:  &AzU8Vec) -> AzString = transmute(lib.get(b"az_u8_vec_fmt_debug")?);
            let az_callback_data_vec_new: extern "C" fn() -> AzCallbackDataVec = transmute(lib.get(b"az_callback_data_vec_new")?);
            let az_callback_data_vec_with_capacity: extern "C" fn(_:  usize) -> AzCallbackDataVec = transmute(lib.get(b"az_callback_data_vec_with_capacity")?);
            let az_callback_data_vec_copy_from: extern "C" fn(_:  *const AzCallbackData, _:  usize) -> AzCallbackDataVec = transmute(lib.get(b"az_callback_data_vec_copy_from")?);
            let az_callback_data_vec_delete: extern "C" fn(_:  &mut AzCallbackDataVec) = transmute(lib.get(b"az_callback_data_vec_delete")?);
            let az_callback_data_vec_deep_copy: extern "C" fn(_:  &AzCallbackDataVec) -> AzCallbackDataVec = transmute(lib.get(b"az_callback_data_vec_deep_copy")?);
            let az_callback_data_vec_fmt_debug: extern "C" fn(_:  &AzCallbackDataVec) -> AzString = transmute(lib.get(b"az_callback_data_vec_fmt_debug")?);
            let az_debug_message_vec_new: extern "C" fn() -> AzDebugMessageVec = transmute(lib.get(b"az_debug_message_vec_new")?);
            let az_debug_message_vec_with_capacity: extern "C" fn(_:  usize) -> AzDebugMessageVec = transmute(lib.get(b"az_debug_message_vec_with_capacity")?);
            let az_debug_message_vec_copy_from: extern "C" fn(_:  *const AzDebugMessage, _:  usize) -> AzDebugMessageVec = transmute(lib.get(b"az_debug_message_vec_copy_from")?);
            let az_debug_message_vec_delete: extern "C" fn(_:  &mut AzDebugMessageVec) = transmute(lib.get(b"az_debug_message_vec_delete")?);
            let az_debug_message_vec_deep_copy: extern "C" fn(_:  &AzDebugMessageVec) -> AzDebugMessageVec = transmute(lib.get(b"az_debug_message_vec_deep_copy")?);
            let az_debug_message_vec_fmt_debug: extern "C" fn(_:  &AzDebugMessageVec) -> AzString = transmute(lib.get(b"az_debug_message_vec_fmt_debug")?);
            let az_g_luint_vec_new: extern "C" fn() -> AzGLuintVec = transmute(lib.get(b"az_g_luint_vec_new")?);
            let az_g_luint_vec_with_capacity: extern "C" fn(_:  usize) -> AzGLuintVec = transmute(lib.get(b"az_g_luint_vec_with_capacity")?);
            let az_g_luint_vec_copy_from: extern "C" fn(_:  *const u32, _:  usize) -> AzGLuintVec = transmute(lib.get(b"az_g_luint_vec_copy_from")?);
            let az_g_luint_vec_delete: extern "C" fn(_:  &mut AzGLuintVec) = transmute(lib.get(b"az_g_luint_vec_delete")?);
            let az_g_luint_vec_deep_copy: extern "C" fn(_:  &AzGLuintVec) -> AzGLuintVec = transmute(lib.get(b"az_g_luint_vec_deep_copy")?);
            let az_g_luint_vec_fmt_debug: extern "C" fn(_:  &AzGLuintVec) -> AzString = transmute(lib.get(b"az_g_luint_vec_fmt_debug")?);
            let az_g_lint_vec_new: extern "C" fn() -> AzGLintVec = transmute(lib.get(b"az_g_lint_vec_new")?);
            let az_g_lint_vec_with_capacity: extern "C" fn(_:  usize) -> AzGLintVec = transmute(lib.get(b"az_g_lint_vec_with_capacity")?);
            let az_g_lint_vec_copy_from: extern "C" fn(_:  *const i32, _:  usize) -> AzGLintVec = transmute(lib.get(b"az_g_lint_vec_copy_from")?);
            let az_g_lint_vec_delete: extern "C" fn(_:  &mut AzGLintVec) = transmute(lib.get(b"az_g_lint_vec_delete")?);
            let az_g_lint_vec_deep_copy: extern "C" fn(_:  &AzGLintVec) -> AzGLintVec = transmute(lib.get(b"az_g_lint_vec_deep_copy")?);
            let az_g_lint_vec_fmt_debug: extern "C" fn(_:  &AzGLintVec) -> AzString = transmute(lib.get(b"az_g_lint_vec_fmt_debug")?);
            let az_dom_vec_new: extern "C" fn() -> AzDomVec = transmute(lib.get(b"az_dom_vec_new")?);
            let az_dom_vec_with_capacity: extern "C" fn(_:  usize) -> AzDomVec = transmute(lib.get(b"az_dom_vec_with_capacity")?);
            let az_dom_vec_copy_from: extern "C" fn(_:  *const AzDom, _:  usize) -> AzDomVec = transmute(lib.get(b"az_dom_vec_copy_from")?);
            let az_dom_vec_delete: extern "C" fn(_:  &mut AzDomVec) = transmute(lib.get(b"az_dom_vec_delete")?);
            let az_dom_vec_deep_copy: extern "C" fn(_:  &AzDomVec) -> AzDomVec = transmute(lib.get(b"az_dom_vec_deep_copy")?);
            let az_dom_vec_fmt_debug: extern "C" fn(_:  &AzDomVec) -> AzString = transmute(lib.get(b"az_dom_vec_fmt_debug")?);
            let az_string_vec_new: extern "C" fn() -> AzStringVec = transmute(lib.get(b"az_string_vec_new")?);
            let az_string_vec_with_capacity: extern "C" fn(_:  usize) -> AzStringVec = transmute(lib.get(b"az_string_vec_with_capacity")?);
            let az_string_vec_copy_from: extern "C" fn(_:  *const AzString, _:  usize) -> AzStringVec = transmute(lib.get(b"az_string_vec_copy_from")?);
            let az_string_vec_delete: extern "C" fn(_:  &mut AzStringVec) = transmute(lib.get(b"az_string_vec_delete")?);
            let az_string_vec_deep_copy: extern "C" fn(_:  &AzStringVec) -> AzStringVec = transmute(lib.get(b"az_string_vec_deep_copy")?);
            let az_string_vec_fmt_debug: extern "C" fn(_:  &AzStringVec) -> AzString = transmute(lib.get(b"az_string_vec_fmt_debug")?);
            let az_string_pair_vec_new: extern "C" fn() -> AzStringPairVec = transmute(lib.get(b"az_string_pair_vec_new")?);
            let az_string_pair_vec_with_capacity: extern "C" fn(_:  usize) -> AzStringPairVec = transmute(lib.get(b"az_string_pair_vec_with_capacity")?);
            let az_string_pair_vec_copy_from: extern "C" fn(_:  *const AzStringPair, _:  usize) -> AzStringPairVec = transmute(lib.get(b"az_string_pair_vec_copy_from")?);
            let az_string_pair_vec_delete: extern "C" fn(_:  &mut AzStringPairVec) = transmute(lib.get(b"az_string_pair_vec_delete")?);
            let az_string_pair_vec_deep_copy: extern "C" fn(_:  &AzStringPairVec) -> AzStringPairVec = transmute(lib.get(b"az_string_pair_vec_deep_copy")?);
            let az_string_pair_vec_fmt_debug: extern "C" fn(_:  &AzStringPairVec) -> AzString = transmute(lib.get(b"az_string_pair_vec_fmt_debug")?);
            let az_gradient_stop_pre_vec_new: extern "C" fn() -> AzGradientStopPreVec = transmute(lib.get(b"az_gradient_stop_pre_vec_new")?);
            let az_gradient_stop_pre_vec_with_capacity: extern "C" fn(_:  usize) -> AzGradientStopPreVec = transmute(lib.get(b"az_gradient_stop_pre_vec_with_capacity")?);
            let az_gradient_stop_pre_vec_copy_from: extern "C" fn(_:  *const AzGradientStopPre, _:  usize) -> AzGradientStopPreVec = transmute(lib.get(b"az_gradient_stop_pre_vec_copy_from")?);
            let az_gradient_stop_pre_vec_delete: extern "C" fn(_:  &mut AzGradientStopPreVec) = transmute(lib.get(b"az_gradient_stop_pre_vec_delete")?);
            let az_gradient_stop_pre_vec_deep_copy: extern "C" fn(_:  &AzGradientStopPreVec) -> AzGradientStopPreVec = transmute(lib.get(b"az_gradient_stop_pre_vec_deep_copy")?);
            let az_gradient_stop_pre_vec_fmt_debug: extern "C" fn(_:  &AzGradientStopPreVec) -> AzString = transmute(lib.get(b"az_gradient_stop_pre_vec_fmt_debug")?);
            let az_cascaded_css_property_with_source_vec_new: extern "C" fn() -> AzCascadedCssPropertyWithSourceVec = transmute(lib.get(b"az_cascaded_css_property_with_source_vec_new")?);
            let az_cascaded_css_property_with_source_vec_with_capacity: extern "C" fn(_:  usize) -> AzCascadedCssPropertyWithSourceVec = transmute(lib.get(b"az_cascaded_css_property_with_source_vec_with_capacity")?);
            let az_cascaded_css_property_with_source_vec_copy_from: extern "C" fn(_:  *const AzCascadedCssPropertyWithSource, _:  usize) -> AzCascadedCssPropertyWithSourceVec = transmute(lib.get(b"az_cascaded_css_property_with_source_vec_copy_from")?);
            let az_cascaded_css_property_with_source_vec_delete: extern "C" fn(_:  &mut AzCascadedCssPropertyWithSourceVec) = transmute(lib.get(b"az_cascaded_css_property_with_source_vec_delete")?);
            let az_cascaded_css_property_with_source_vec_deep_copy: extern "C" fn(_:  &AzCascadedCssPropertyWithSourceVec) -> AzCascadedCssPropertyWithSourceVec = transmute(lib.get(b"az_cascaded_css_property_with_source_vec_deep_copy")?);
            let az_cascaded_css_property_with_source_vec_fmt_debug: extern "C" fn(_:  &AzCascadedCssPropertyWithSourceVec) -> AzString = transmute(lib.get(b"az_cascaded_css_property_with_source_vec_fmt_debug")?);
            let az_node_id_vec_new: extern "C" fn() -> AzNodeIdVec = transmute(lib.get(b"az_node_id_vec_new")?);
            let az_node_id_vec_with_capacity: extern "C" fn(_:  usize) -> AzNodeIdVec = transmute(lib.get(b"az_node_id_vec_with_capacity")?);
            let az_node_id_vec_copy_from: extern "C" fn(_:  *const AzNodeId, _:  usize) -> AzNodeIdVec = transmute(lib.get(b"az_node_id_vec_copy_from")?);
            let az_node_id_vec_delete: extern "C" fn(_:  &mut AzNodeIdVec) = transmute(lib.get(b"az_node_id_vec_delete")?);
            let az_node_id_vec_deep_copy: extern "C" fn(_:  &AzNodeIdVec) -> AzNodeIdVec = transmute(lib.get(b"az_node_id_vec_deep_copy")?);
            let az_node_id_vec_fmt_debug: extern "C" fn(_:  &AzNodeIdVec) -> AzString = transmute(lib.get(b"az_node_id_vec_fmt_debug")?);
            let az_node_vec_new: extern "C" fn() -> AzNodeVec = transmute(lib.get(b"az_node_vec_new")?);
            let az_node_vec_with_capacity: extern "C" fn(_:  usize) -> AzNodeVec = transmute(lib.get(b"az_node_vec_with_capacity")?);
            let az_node_vec_copy_from: extern "C" fn(_:  *const AzNode, _:  usize) -> AzNodeVec = transmute(lib.get(b"az_node_vec_copy_from")?);
            let az_node_vec_delete: extern "C" fn(_:  &mut AzNodeVec) = transmute(lib.get(b"az_node_vec_delete")?);
            let az_node_vec_deep_copy: extern "C" fn(_:  &AzNodeVec) -> AzNodeVec = transmute(lib.get(b"az_node_vec_deep_copy")?);
            let az_node_vec_fmt_debug: extern "C" fn(_:  &AzNodeVec) -> AzString = transmute(lib.get(b"az_node_vec_fmt_debug")?);
            let az_styled_node_vec_new: extern "C" fn() -> AzStyledNodeVec = transmute(lib.get(b"az_styled_node_vec_new")?);
            let az_styled_node_vec_with_capacity: extern "C" fn(_:  usize) -> AzStyledNodeVec = transmute(lib.get(b"az_styled_node_vec_with_capacity")?);
            let az_styled_node_vec_copy_from: extern "C" fn(_:  *const AzStyledNode, _:  usize) -> AzStyledNodeVec = transmute(lib.get(b"az_styled_node_vec_copy_from")?);
            let az_styled_node_vec_delete: extern "C" fn(_:  &mut AzStyledNodeVec) = transmute(lib.get(b"az_styled_node_vec_delete")?);
            let az_styled_node_vec_deep_copy: extern "C" fn(_:  &AzStyledNodeVec) -> AzStyledNodeVec = transmute(lib.get(b"az_styled_node_vec_deep_copy")?);
            let az_styled_node_vec_fmt_debug: extern "C" fn(_:  &AzStyledNodeVec) -> AzString = transmute(lib.get(b"az_styled_node_vec_fmt_debug")?);
            let az_tag_ids_to_node_ids_mapping_vec_new: extern "C" fn() -> AzTagIdsToNodeIdsMappingVec = transmute(lib.get(b"az_tag_ids_to_node_ids_mapping_vec_new")?);
            let az_tag_ids_to_node_ids_mapping_vec_with_capacity: extern "C" fn(_:  usize) -> AzTagIdsToNodeIdsMappingVec = transmute(lib.get(b"az_tag_ids_to_node_ids_mapping_vec_with_capacity")?);
            let az_tag_ids_to_node_ids_mapping_vec_copy_from: extern "C" fn(_:  *const AzTagIdToNodeIdMapping, _:  usize) -> AzTagIdsToNodeIdsMappingVec = transmute(lib.get(b"az_tag_ids_to_node_ids_mapping_vec_copy_from")?);
            let az_tag_ids_to_node_ids_mapping_vec_delete: extern "C" fn(_:  &mut AzTagIdsToNodeIdsMappingVec) = transmute(lib.get(b"az_tag_ids_to_node_ids_mapping_vec_delete")?);
            let az_tag_ids_to_node_ids_mapping_vec_deep_copy: extern "C" fn(_:  &AzTagIdsToNodeIdsMappingVec) -> AzTagIdsToNodeIdsMappingVec = transmute(lib.get(b"az_tag_ids_to_node_ids_mapping_vec_deep_copy")?);
            let az_tag_ids_to_node_ids_mapping_vec_fmt_debug: extern "C" fn(_:  &AzTagIdsToNodeIdsMappingVec) -> AzString = transmute(lib.get(b"az_tag_ids_to_node_ids_mapping_vec_fmt_debug")?);
            let az_parent_with_node_depth_vec_new: extern "C" fn() -> AzParentWithNodeDepthVec = transmute(lib.get(b"az_parent_with_node_depth_vec_new")?);
            let az_parent_with_node_depth_vec_with_capacity: extern "C" fn(_:  usize) -> AzParentWithNodeDepthVec = transmute(lib.get(b"az_parent_with_node_depth_vec_with_capacity")?);
            let az_parent_with_node_depth_vec_copy_from: extern "C" fn(_:  *const AzParentWithNodeDepth, _:  usize) -> AzParentWithNodeDepthVec = transmute(lib.get(b"az_parent_with_node_depth_vec_copy_from")?);
            let az_parent_with_node_depth_vec_delete: extern "C" fn(_:  &mut AzParentWithNodeDepthVec) = transmute(lib.get(b"az_parent_with_node_depth_vec_delete")?);
            let az_parent_with_node_depth_vec_deep_copy: extern "C" fn(_:  &AzParentWithNodeDepthVec) -> AzParentWithNodeDepthVec = transmute(lib.get(b"az_parent_with_node_depth_vec_deep_copy")?);
            let az_parent_with_node_depth_vec_fmt_debug: extern "C" fn(_:  &AzParentWithNodeDepthVec) -> AzString = transmute(lib.get(b"az_parent_with_node_depth_vec_fmt_debug")?);
            let az_node_data_vec_new: extern "C" fn() -> AzNodeDataVec = transmute(lib.get(b"az_node_data_vec_new")?);
            let az_node_data_vec_with_capacity: extern "C" fn(_:  usize) -> AzNodeDataVec = transmute(lib.get(b"az_node_data_vec_with_capacity")?);
            let az_node_data_vec_copy_from: extern "C" fn(_:  *const AzNodeData, _:  usize) -> AzNodeDataVec = transmute(lib.get(b"az_node_data_vec_copy_from")?);
            let az_node_data_vec_delete: extern "C" fn(_:  &mut AzNodeDataVec) = transmute(lib.get(b"az_node_data_vec_delete")?);
            let az_node_data_vec_deep_copy: extern "C" fn(_:  &AzNodeDataVec) -> AzNodeDataVec = transmute(lib.get(b"az_node_data_vec_deep_copy")?);
            let az_node_data_vec_fmt_debug: extern "C" fn(_:  &AzNodeDataVec) -> AzString = transmute(lib.get(b"az_node_data_vec_fmt_debug")?);
            let az_option_node_id_delete: extern "C" fn(_:  &mut AzOptionNodeId) = transmute(lib.get(b"az_option_node_id_delete")?);
            let az_option_node_id_deep_copy: extern "C" fn(_:  &AzOptionNodeId) -> AzOptionNodeId = transmute(lib.get(b"az_option_node_id_deep_copy")?);
            let az_option_node_id_fmt_debug: extern "C" fn(_:  &AzOptionNodeId) -> AzString = transmute(lib.get(b"az_option_node_id_fmt_debug")?);
            let az_option_dom_node_id_delete: extern "C" fn(_:  &mut AzOptionDomNodeId) = transmute(lib.get(b"az_option_dom_node_id_delete")?);
            let az_option_dom_node_id_deep_copy: extern "C" fn(_:  &AzOptionDomNodeId) -> AzOptionDomNodeId = transmute(lib.get(b"az_option_dom_node_id_deep_copy")?);
            let az_option_dom_node_id_fmt_debug: extern "C" fn(_:  &AzOptionDomNodeId) -> AzString = transmute(lib.get(b"az_option_dom_node_id_fmt_debug")?);
            let az_option_color_u_delete: extern "C" fn(_:  &mut AzOptionColorU) = transmute(lib.get(b"az_option_color_u_delete")?);
            let az_option_color_u_deep_copy: extern "C" fn(_:  &AzOptionColorU) -> AzOptionColorU = transmute(lib.get(b"az_option_color_u_deep_copy")?);
            let az_option_color_u_fmt_debug: extern "C" fn(_:  &AzOptionColorU) -> AzString = transmute(lib.get(b"az_option_color_u_fmt_debug")?);
            let az_option_raw_image_delete: extern "C" fn(_:  &mut AzOptionRawImage) = transmute(lib.get(b"az_option_raw_image_delete")?);
            let az_option_raw_image_deep_copy: extern "C" fn(_:  &AzOptionRawImage) -> AzOptionRawImage = transmute(lib.get(b"az_option_raw_image_deep_copy")?);
            let az_option_raw_image_fmt_debug: extern "C" fn(_:  &AzOptionRawImage) -> AzString = transmute(lib.get(b"az_option_raw_image_fmt_debug")?);
            let az_option_svg_dash_pattern_delete: extern "C" fn(_:  &mut AzOptionSvgDashPattern) = transmute(lib.get(b"az_option_svg_dash_pattern_delete")?);
            let az_option_svg_dash_pattern_deep_copy: extern "C" fn(_:  &AzOptionSvgDashPattern) -> AzOptionSvgDashPattern = transmute(lib.get(b"az_option_svg_dash_pattern_deep_copy")?);
            let az_option_svg_dash_pattern_fmt_debug: extern "C" fn(_:  &AzOptionSvgDashPattern) -> AzString = transmute(lib.get(b"az_option_svg_dash_pattern_fmt_debug")?);
            let az_option_wayland_theme_delete: extern "C" fn(_:  &mut AzOptionWaylandTheme) = transmute(lib.get(b"az_option_wayland_theme_delete")?);
            let az_option_wayland_theme_deep_copy: extern "C" fn(_:  &AzOptionWaylandTheme) -> AzOptionWaylandTheme = transmute(lib.get(b"az_option_wayland_theme_deep_copy")?);
            let az_option_wayland_theme_fmt_debug: extern "C" fn(_:  &AzOptionWaylandTheme) -> AzString = transmute(lib.get(b"az_option_wayland_theme_fmt_debug")?);
            let az_option_task_bar_icon_delete: extern "C" fn(_:  &mut AzOptionTaskBarIcon) = transmute(lib.get(b"az_option_task_bar_icon_delete")?);
            let az_option_task_bar_icon_deep_copy: extern "C" fn(_:  &AzOptionTaskBarIcon) -> AzOptionTaskBarIcon = transmute(lib.get(b"az_option_task_bar_icon_deep_copy")?);
            let az_option_task_bar_icon_fmt_debug: extern "C" fn(_:  &AzOptionTaskBarIcon) -> AzString = transmute(lib.get(b"az_option_task_bar_icon_fmt_debug")?);
            let az_option_hwnd_handle_delete: extern "C" fn(_:  &mut AzOptionHwndHandle) = transmute(lib.get(b"az_option_hwnd_handle_delete")?);
            let az_option_hwnd_handle_deep_copy: extern "C" fn(_:  &AzOptionHwndHandle) -> AzOptionHwndHandle = transmute(lib.get(b"az_option_hwnd_handle_deep_copy")?);
            let az_option_hwnd_handle_fmt_debug: extern "C" fn(_:  &AzOptionHwndHandle) -> AzString = transmute(lib.get(b"az_option_hwnd_handle_fmt_debug")?);
            let az_option_logical_position_delete: extern "C" fn(_:  &mut AzOptionLogicalPosition) = transmute(lib.get(b"az_option_logical_position_delete")?);
            let az_option_logical_position_deep_copy: extern "C" fn(_:  &AzOptionLogicalPosition) -> AzOptionLogicalPosition = transmute(lib.get(b"az_option_logical_position_deep_copy")?);
            let az_option_logical_position_fmt_debug: extern "C" fn(_:  &AzOptionLogicalPosition) -> AzString = transmute(lib.get(b"az_option_logical_position_fmt_debug")?);
            let az_option_hot_reload_options_delete: extern "C" fn(_:  &mut AzOptionHotReloadOptions) = transmute(lib.get(b"az_option_hot_reload_options_delete")?);
            let az_option_hot_reload_options_deep_copy: extern "C" fn(_:  &AzOptionHotReloadOptions) -> AzOptionHotReloadOptions = transmute(lib.get(b"az_option_hot_reload_options_deep_copy")?);
            let az_option_hot_reload_options_fmt_debug: extern "C" fn(_:  &AzOptionHotReloadOptions) -> AzString = transmute(lib.get(b"az_option_hot_reload_options_fmt_debug")?);
            let az_option_physical_position_i32_delete: extern "C" fn(_:  &mut AzOptionPhysicalPositionI32) = transmute(lib.get(b"az_option_physical_position_i32_delete")?);
            let az_option_physical_position_i32_deep_copy: extern "C" fn(_:  &AzOptionPhysicalPositionI32) -> AzOptionPhysicalPositionI32 = transmute(lib.get(b"az_option_physical_position_i32_deep_copy")?);
            let az_option_physical_position_i32_fmt_debug: extern "C" fn(_:  &AzOptionPhysicalPositionI32) -> AzString = transmute(lib.get(b"az_option_physical_position_i32_fmt_debug")?);
            let az_option_window_icon_delete: extern "C" fn(_:  &mut AzOptionWindowIcon) = transmute(lib.get(b"az_option_window_icon_delete")?);
            let az_option_window_icon_deep_copy: extern "C" fn(_:  &AzOptionWindowIcon) -> AzOptionWindowIcon = transmute(lib.get(b"az_option_window_icon_deep_copy")?);
            let az_option_window_icon_fmt_debug: extern "C" fn(_:  &AzOptionWindowIcon) -> AzString = transmute(lib.get(b"az_option_window_icon_fmt_debug")?);
            let az_option_string_delete: extern "C" fn(_:  &mut AzOptionString) = transmute(lib.get(b"az_option_string_delete")?);
            let az_option_string_deep_copy: extern "C" fn(_:  &AzOptionString) -> AzOptionString = transmute(lib.get(b"az_option_string_deep_copy")?);
            let az_option_string_fmt_debug: extern "C" fn(_:  &AzOptionString) -> AzString = transmute(lib.get(b"az_option_string_fmt_debug")?);
            let az_option_x11_visual_delete: extern "C" fn(_:  &mut AzOptionX11Visual) = transmute(lib.get(b"az_option_x11_visual_delete")?);
            let az_option_x11_visual_deep_copy: extern "C" fn(_:  &AzOptionX11Visual) -> AzOptionX11Visual = transmute(lib.get(b"az_option_x11_visual_deep_copy")?);
            let az_option_x11_visual_fmt_debug: extern "C" fn(_:  &AzOptionX11Visual) -> AzString = transmute(lib.get(b"az_option_x11_visual_fmt_debug")?);
            let az_option_i32_delete: extern "C" fn(_:  &mut AzOptionI32) = transmute(lib.get(b"az_option_i32_delete")?);
            let az_option_i32_deep_copy: extern "C" fn(_:  &AzOptionI32) -> AzOptionI32 = transmute(lib.get(b"az_option_i32_deep_copy")?);
            let az_option_i32_fmt_debug: extern "C" fn(_:  &AzOptionI32) -> AzString = transmute(lib.get(b"az_option_i32_fmt_debug")?);
            let az_option_f32_delete: extern "C" fn(_:  &mut AzOptionF32) = transmute(lib.get(b"az_option_f32_delete")?);
            let az_option_f32_deep_copy: extern "C" fn(_:  &AzOptionF32) -> AzOptionF32 = transmute(lib.get(b"az_option_f32_deep_copy")?);
            let az_option_f32_fmt_debug: extern "C" fn(_:  &AzOptionF32) -> AzString = transmute(lib.get(b"az_option_f32_fmt_debug")?);
            let az_option_mouse_cursor_type_delete: extern "C" fn(_:  &mut AzOptionMouseCursorType) = transmute(lib.get(b"az_option_mouse_cursor_type_delete")?);
            let az_option_mouse_cursor_type_deep_copy: extern "C" fn(_:  &AzOptionMouseCursorType) -> AzOptionMouseCursorType = transmute(lib.get(b"az_option_mouse_cursor_type_deep_copy")?);
            let az_option_mouse_cursor_type_fmt_debug: extern "C" fn(_:  &AzOptionMouseCursorType) -> AzString = transmute(lib.get(b"az_option_mouse_cursor_type_fmt_debug")?);
            let az_option_logical_size_delete: extern "C" fn(_:  &mut AzOptionLogicalSize) = transmute(lib.get(b"az_option_logical_size_delete")?);
            let az_option_logical_size_deep_copy: extern "C" fn(_:  &AzOptionLogicalSize) -> AzOptionLogicalSize = transmute(lib.get(b"az_option_logical_size_deep_copy")?);
            let az_option_logical_size_fmt_debug: extern "C" fn(_:  &AzOptionLogicalSize) -> AzString = transmute(lib.get(b"az_option_logical_size_fmt_debug")?);
            let az_option_char_delete: extern "C" fn(_:  &mut AzOptionChar) = transmute(lib.get(b"az_option_char_delete")?);
            let az_option_char_deep_copy: extern "C" fn(_:  &AzOptionChar) -> AzOptionChar = transmute(lib.get(b"az_option_char_deep_copy")?);
            let az_option_char_fmt_debug: extern "C" fn(_:  &AzOptionChar) -> AzString = transmute(lib.get(b"az_option_char_fmt_debug")?);
            let az_option_virtual_key_code_delete: extern "C" fn(_:  &mut AzOptionVirtualKeyCode) = transmute(lib.get(b"az_option_virtual_key_code_delete")?);
            let az_option_virtual_key_code_deep_copy: extern "C" fn(_:  &AzOptionVirtualKeyCode) -> AzOptionVirtualKeyCode = transmute(lib.get(b"az_option_virtual_key_code_deep_copy")?);
            let az_option_virtual_key_code_fmt_debug: extern "C" fn(_:  &AzOptionVirtualKeyCode) -> AzString = transmute(lib.get(b"az_option_virtual_key_code_fmt_debug")?);
            let az_option_percentage_value_delete: extern "C" fn(_:  &mut AzOptionPercentageValue) = transmute(lib.get(b"az_option_percentage_value_delete")?);
            let az_option_percentage_value_deep_copy: extern "C" fn(_:  &AzOptionPercentageValue) -> AzOptionPercentageValue = transmute(lib.get(b"az_option_percentage_value_deep_copy")?);
            let az_option_percentage_value_fmt_debug: extern "C" fn(_:  &AzOptionPercentageValue) -> AzString = transmute(lib.get(b"az_option_percentage_value_fmt_debug")?);
            let az_option_dom_delete: extern "C" fn(_:  &mut AzOptionDom) = transmute(lib.get(b"az_option_dom_delete")?);
            let az_option_dom_deep_copy: extern "C" fn(_:  &AzOptionDom) -> AzOptionDom = transmute(lib.get(b"az_option_dom_deep_copy")?);
            let az_option_dom_fmt_debug: extern "C" fn(_:  &AzOptionDom) -> AzString = transmute(lib.get(b"az_option_dom_fmt_debug")?);
            let az_option_texture_delete: extern "C" fn(_:  &mut AzOptionTexture) = transmute(lib.get(b"az_option_texture_delete")?);
            let az_option_texture_fmt_debug: extern "C" fn(_:  &AzOptionTexture) -> AzString = transmute(lib.get(b"az_option_texture_fmt_debug")?);
            let az_option_image_mask_delete: extern "C" fn(_:  &mut AzOptionImageMask) = transmute(lib.get(b"az_option_image_mask_delete")?);
            let az_option_image_mask_deep_copy: extern "C" fn(_:  &AzOptionImageMask) -> AzOptionImageMask = transmute(lib.get(b"az_option_image_mask_deep_copy")?);
            let az_option_image_mask_fmt_debug: extern "C" fn(_:  &AzOptionImageMask) -> AzString = transmute(lib.get(b"az_option_image_mask_fmt_debug")?);
            let az_option_tab_index_delete: extern "C" fn(_:  &mut AzOptionTabIndex) = transmute(lib.get(b"az_option_tab_index_delete")?);
            let az_option_tab_index_deep_copy: extern "C" fn(_:  &AzOptionTabIndex) -> AzOptionTabIndex = transmute(lib.get(b"az_option_tab_index_deep_copy")?);
            let az_option_tab_index_fmt_debug: extern "C" fn(_:  &AzOptionTabIndex) -> AzString = transmute(lib.get(b"az_option_tab_index_fmt_debug")?);
            let az_option_style_background_content_value_delete: extern "C" fn(_:  &mut AzOptionStyleBackgroundContentValue) = transmute(lib.get(b"az_option_style_background_content_value_delete")?);
            let az_option_style_background_content_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBackgroundContentValue) -> AzOptionStyleBackgroundContentValue = transmute(lib.get(b"az_option_style_background_content_value_deep_copy")?);
            let az_option_style_background_content_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBackgroundContentValue) -> AzString = transmute(lib.get(b"az_option_style_background_content_value_fmt_debug")?);
            let az_option_style_background_position_value_delete: extern "C" fn(_:  &mut AzOptionStyleBackgroundPositionValue) = transmute(lib.get(b"az_option_style_background_position_value_delete")?);
            let az_option_style_background_position_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBackgroundPositionValue) -> AzOptionStyleBackgroundPositionValue = transmute(lib.get(b"az_option_style_background_position_value_deep_copy")?);
            let az_option_style_background_position_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBackgroundPositionValue) -> AzString = transmute(lib.get(b"az_option_style_background_position_value_fmt_debug")?);
            let az_option_style_background_size_value_delete: extern "C" fn(_:  &mut AzOptionStyleBackgroundSizeValue) = transmute(lib.get(b"az_option_style_background_size_value_delete")?);
            let az_option_style_background_size_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBackgroundSizeValue) -> AzOptionStyleBackgroundSizeValue = transmute(lib.get(b"az_option_style_background_size_value_deep_copy")?);
            let az_option_style_background_size_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBackgroundSizeValue) -> AzString = transmute(lib.get(b"az_option_style_background_size_value_fmt_debug")?);
            let az_option_style_background_repeat_value_delete: extern "C" fn(_:  &mut AzOptionStyleBackgroundRepeatValue) = transmute(lib.get(b"az_option_style_background_repeat_value_delete")?);
            let az_option_style_background_repeat_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBackgroundRepeatValue) -> AzOptionStyleBackgroundRepeatValue = transmute(lib.get(b"az_option_style_background_repeat_value_deep_copy")?);
            let az_option_style_background_repeat_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBackgroundRepeatValue) -> AzString = transmute(lib.get(b"az_option_style_background_repeat_value_fmt_debug")?);
            let az_option_style_font_size_value_delete: extern "C" fn(_:  &mut AzOptionStyleFontSizeValue) = transmute(lib.get(b"az_option_style_font_size_value_delete")?);
            let az_option_style_font_size_value_deep_copy: extern "C" fn(_:  &AzOptionStyleFontSizeValue) -> AzOptionStyleFontSizeValue = transmute(lib.get(b"az_option_style_font_size_value_deep_copy")?);
            let az_option_style_font_size_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleFontSizeValue) -> AzString = transmute(lib.get(b"az_option_style_font_size_value_fmt_debug")?);
            let az_option_style_font_family_value_delete: extern "C" fn(_:  &mut AzOptionStyleFontFamilyValue) = transmute(lib.get(b"az_option_style_font_family_value_delete")?);
            let az_option_style_font_family_value_deep_copy: extern "C" fn(_:  &AzOptionStyleFontFamilyValue) -> AzOptionStyleFontFamilyValue = transmute(lib.get(b"az_option_style_font_family_value_deep_copy")?);
            let az_option_style_font_family_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleFontFamilyValue) -> AzString = transmute(lib.get(b"az_option_style_font_family_value_fmt_debug")?);
            let az_option_style_text_color_value_delete: extern "C" fn(_:  &mut AzOptionStyleTextColorValue) = transmute(lib.get(b"az_option_style_text_color_value_delete")?);
            let az_option_style_text_color_value_deep_copy: extern "C" fn(_:  &AzOptionStyleTextColorValue) -> AzOptionStyleTextColorValue = transmute(lib.get(b"az_option_style_text_color_value_deep_copy")?);
            let az_option_style_text_color_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleTextColorValue) -> AzString = transmute(lib.get(b"az_option_style_text_color_value_fmt_debug")?);
            let az_option_style_text_alignment_horz_value_delete: extern "C" fn(_:  &mut AzOptionStyleTextAlignmentHorzValue) = transmute(lib.get(b"az_option_style_text_alignment_horz_value_delete")?);
            let az_option_style_text_alignment_horz_value_deep_copy: extern "C" fn(_:  &AzOptionStyleTextAlignmentHorzValue) -> AzOptionStyleTextAlignmentHorzValue = transmute(lib.get(b"az_option_style_text_alignment_horz_value_deep_copy")?);
            let az_option_style_text_alignment_horz_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleTextAlignmentHorzValue) -> AzString = transmute(lib.get(b"az_option_style_text_alignment_horz_value_fmt_debug")?);
            let az_option_style_line_height_value_delete: extern "C" fn(_:  &mut AzOptionStyleLineHeightValue) = transmute(lib.get(b"az_option_style_line_height_value_delete")?);
            let az_option_style_line_height_value_deep_copy: extern "C" fn(_:  &AzOptionStyleLineHeightValue) -> AzOptionStyleLineHeightValue = transmute(lib.get(b"az_option_style_line_height_value_deep_copy")?);
            let az_option_style_line_height_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleLineHeightValue) -> AzString = transmute(lib.get(b"az_option_style_line_height_value_fmt_debug")?);
            let az_option_style_letter_spacing_value_delete: extern "C" fn(_:  &mut AzOptionStyleLetterSpacingValue) = transmute(lib.get(b"az_option_style_letter_spacing_value_delete")?);
            let az_option_style_letter_spacing_value_deep_copy: extern "C" fn(_:  &AzOptionStyleLetterSpacingValue) -> AzOptionStyleLetterSpacingValue = transmute(lib.get(b"az_option_style_letter_spacing_value_deep_copy")?);
            let az_option_style_letter_spacing_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleLetterSpacingValue) -> AzString = transmute(lib.get(b"az_option_style_letter_spacing_value_fmt_debug")?);
            let az_option_style_word_spacing_value_delete: extern "C" fn(_:  &mut AzOptionStyleWordSpacingValue) = transmute(lib.get(b"az_option_style_word_spacing_value_delete")?);
            let az_option_style_word_spacing_value_deep_copy: extern "C" fn(_:  &AzOptionStyleWordSpacingValue) -> AzOptionStyleWordSpacingValue = transmute(lib.get(b"az_option_style_word_spacing_value_deep_copy")?);
            let az_option_style_word_spacing_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleWordSpacingValue) -> AzString = transmute(lib.get(b"az_option_style_word_spacing_value_fmt_debug")?);
            let az_option_style_tab_width_value_delete: extern "C" fn(_:  &mut AzOptionStyleTabWidthValue) = transmute(lib.get(b"az_option_style_tab_width_value_delete")?);
            let az_option_style_tab_width_value_deep_copy: extern "C" fn(_:  &AzOptionStyleTabWidthValue) -> AzOptionStyleTabWidthValue = transmute(lib.get(b"az_option_style_tab_width_value_deep_copy")?);
            let az_option_style_tab_width_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleTabWidthValue) -> AzString = transmute(lib.get(b"az_option_style_tab_width_value_fmt_debug")?);
            let az_option_style_cursor_value_delete: extern "C" fn(_:  &mut AzOptionStyleCursorValue) = transmute(lib.get(b"az_option_style_cursor_value_delete")?);
            let az_option_style_cursor_value_deep_copy: extern "C" fn(_:  &AzOptionStyleCursorValue) -> AzOptionStyleCursorValue = transmute(lib.get(b"az_option_style_cursor_value_deep_copy")?);
            let az_option_style_cursor_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleCursorValue) -> AzString = transmute(lib.get(b"az_option_style_cursor_value_fmt_debug")?);
            let az_option_box_shadow_pre_display_item_value_delete: extern "C" fn(_:  &mut AzOptionBoxShadowPreDisplayItemValue) = transmute(lib.get(b"az_option_box_shadow_pre_display_item_value_delete")?);
            let az_option_box_shadow_pre_display_item_value_deep_copy: extern "C" fn(_:  &AzOptionBoxShadowPreDisplayItemValue) -> AzOptionBoxShadowPreDisplayItemValue = transmute(lib.get(b"az_option_box_shadow_pre_display_item_value_deep_copy")?);
            let az_option_box_shadow_pre_display_item_value_fmt_debug: extern "C" fn(_:  &AzOptionBoxShadowPreDisplayItemValue) -> AzString = transmute(lib.get(b"az_option_box_shadow_pre_display_item_value_fmt_debug")?);
            let az_option_style_border_top_color_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderTopColorValue) = transmute(lib.get(b"az_option_style_border_top_color_value_delete")?);
            let az_option_style_border_top_color_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderTopColorValue) -> AzOptionStyleBorderTopColorValue = transmute(lib.get(b"az_option_style_border_top_color_value_deep_copy")?);
            let az_option_style_border_top_color_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderTopColorValue) -> AzString = transmute(lib.get(b"az_option_style_border_top_color_value_fmt_debug")?);
            let az_option_style_border_left_color_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderLeftColorValue) = transmute(lib.get(b"az_option_style_border_left_color_value_delete")?);
            let az_option_style_border_left_color_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderLeftColorValue) -> AzOptionStyleBorderLeftColorValue = transmute(lib.get(b"az_option_style_border_left_color_value_deep_copy")?);
            let az_option_style_border_left_color_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderLeftColorValue) -> AzString = transmute(lib.get(b"az_option_style_border_left_color_value_fmt_debug")?);
            let az_option_style_border_right_color_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderRightColorValue) = transmute(lib.get(b"az_option_style_border_right_color_value_delete")?);
            let az_option_style_border_right_color_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderRightColorValue) -> AzOptionStyleBorderRightColorValue = transmute(lib.get(b"az_option_style_border_right_color_value_deep_copy")?);
            let az_option_style_border_right_color_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderRightColorValue) -> AzString = transmute(lib.get(b"az_option_style_border_right_color_value_fmt_debug")?);
            let az_option_style_border_bottom_color_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderBottomColorValue) = transmute(lib.get(b"az_option_style_border_bottom_color_value_delete")?);
            let az_option_style_border_bottom_color_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderBottomColorValue) -> AzOptionStyleBorderBottomColorValue = transmute(lib.get(b"az_option_style_border_bottom_color_value_deep_copy")?);
            let az_option_style_border_bottom_color_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderBottomColorValue) -> AzString = transmute(lib.get(b"az_option_style_border_bottom_color_value_fmt_debug")?);
            let az_option_style_border_top_style_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderTopStyleValue) = transmute(lib.get(b"az_option_style_border_top_style_value_delete")?);
            let az_option_style_border_top_style_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderTopStyleValue) -> AzOptionStyleBorderTopStyleValue = transmute(lib.get(b"az_option_style_border_top_style_value_deep_copy")?);
            let az_option_style_border_top_style_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderTopStyleValue) -> AzString = transmute(lib.get(b"az_option_style_border_top_style_value_fmt_debug")?);
            let az_option_style_border_left_style_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderLeftStyleValue) = transmute(lib.get(b"az_option_style_border_left_style_value_delete")?);
            let az_option_style_border_left_style_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderLeftStyleValue) -> AzOptionStyleBorderLeftStyleValue = transmute(lib.get(b"az_option_style_border_left_style_value_deep_copy")?);
            let az_option_style_border_left_style_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderLeftStyleValue) -> AzString = transmute(lib.get(b"az_option_style_border_left_style_value_fmt_debug")?);
            let az_option_style_border_right_style_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderRightStyleValue) = transmute(lib.get(b"az_option_style_border_right_style_value_delete")?);
            let az_option_style_border_right_style_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderRightStyleValue) -> AzOptionStyleBorderRightStyleValue = transmute(lib.get(b"az_option_style_border_right_style_value_deep_copy")?);
            let az_option_style_border_right_style_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderRightStyleValue) -> AzString = transmute(lib.get(b"az_option_style_border_right_style_value_fmt_debug")?);
            let az_option_style_border_bottom_style_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderBottomStyleValue) = transmute(lib.get(b"az_option_style_border_bottom_style_value_delete")?);
            let az_option_style_border_bottom_style_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderBottomStyleValue) -> AzOptionStyleBorderBottomStyleValue = transmute(lib.get(b"az_option_style_border_bottom_style_value_deep_copy")?);
            let az_option_style_border_bottom_style_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderBottomStyleValue) -> AzString = transmute(lib.get(b"az_option_style_border_bottom_style_value_fmt_debug")?);
            let az_option_style_border_top_left_radius_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderTopLeftRadiusValue) = transmute(lib.get(b"az_option_style_border_top_left_radius_value_delete")?);
            let az_option_style_border_top_left_radius_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderTopLeftRadiusValue) -> AzOptionStyleBorderTopLeftRadiusValue = transmute(lib.get(b"az_option_style_border_top_left_radius_value_deep_copy")?);
            let az_option_style_border_top_left_radius_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderTopLeftRadiusValue) -> AzString = transmute(lib.get(b"az_option_style_border_top_left_radius_value_fmt_debug")?);
            let az_option_style_border_top_right_radius_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderTopRightRadiusValue) = transmute(lib.get(b"az_option_style_border_top_right_radius_value_delete")?);
            let az_option_style_border_top_right_radius_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderTopRightRadiusValue) -> AzOptionStyleBorderTopRightRadiusValue = transmute(lib.get(b"az_option_style_border_top_right_radius_value_deep_copy")?);
            let az_option_style_border_top_right_radius_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderTopRightRadiusValue) -> AzString = transmute(lib.get(b"az_option_style_border_top_right_radius_value_fmt_debug")?);
            let az_option_style_border_bottom_left_radius_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderBottomLeftRadiusValue) = transmute(lib.get(b"az_option_style_border_bottom_left_radius_value_delete")?);
            let az_option_style_border_bottom_left_radius_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderBottomLeftRadiusValue) -> AzOptionStyleBorderBottomLeftRadiusValue = transmute(lib.get(b"az_option_style_border_bottom_left_radius_value_deep_copy")?);
            let az_option_style_border_bottom_left_radius_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderBottomLeftRadiusValue) -> AzString = transmute(lib.get(b"az_option_style_border_bottom_left_radius_value_fmt_debug")?);
            let az_option_style_border_bottom_right_radius_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderBottomRightRadiusValue) = transmute(lib.get(b"az_option_style_border_bottom_right_radius_value_delete")?);
            let az_option_style_border_bottom_right_radius_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderBottomRightRadiusValue) -> AzOptionStyleBorderBottomRightRadiusValue = transmute(lib.get(b"az_option_style_border_bottom_right_radius_value_deep_copy")?);
            let az_option_style_border_bottom_right_radius_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderBottomRightRadiusValue) -> AzString = transmute(lib.get(b"az_option_style_border_bottom_right_radius_value_fmt_debug")?);
            let az_option_layout_display_value_delete: extern "C" fn(_:  &mut AzOptionLayoutDisplayValue) = transmute(lib.get(b"az_option_layout_display_value_delete")?);
            let az_option_layout_display_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutDisplayValue) -> AzOptionLayoutDisplayValue = transmute(lib.get(b"az_option_layout_display_value_deep_copy")?);
            let az_option_layout_display_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutDisplayValue) -> AzString = transmute(lib.get(b"az_option_layout_display_value_fmt_debug")?);
            let az_option_layout_float_value_delete: extern "C" fn(_:  &mut AzOptionLayoutFloatValue) = transmute(lib.get(b"az_option_layout_float_value_delete")?);
            let az_option_layout_float_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutFloatValue) -> AzOptionLayoutFloatValue = transmute(lib.get(b"az_option_layout_float_value_deep_copy")?);
            let az_option_layout_float_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutFloatValue) -> AzString = transmute(lib.get(b"az_option_layout_float_value_fmt_debug")?);
            let az_option_layout_box_sizing_value_delete: extern "C" fn(_:  &mut AzOptionLayoutBoxSizingValue) = transmute(lib.get(b"az_option_layout_box_sizing_value_delete")?);
            let az_option_layout_box_sizing_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutBoxSizingValue) -> AzOptionLayoutBoxSizingValue = transmute(lib.get(b"az_option_layout_box_sizing_value_deep_copy")?);
            let az_option_layout_box_sizing_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutBoxSizingValue) -> AzString = transmute(lib.get(b"az_option_layout_box_sizing_value_fmt_debug")?);
            let az_option_layout_width_value_delete: extern "C" fn(_:  &mut AzOptionLayoutWidthValue) = transmute(lib.get(b"az_option_layout_width_value_delete")?);
            let az_option_layout_width_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutWidthValue) -> AzOptionLayoutWidthValue = transmute(lib.get(b"az_option_layout_width_value_deep_copy")?);
            let az_option_layout_width_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutWidthValue) -> AzString = transmute(lib.get(b"az_option_layout_width_value_fmt_debug")?);
            let az_option_layout_height_value_delete: extern "C" fn(_:  &mut AzOptionLayoutHeightValue) = transmute(lib.get(b"az_option_layout_height_value_delete")?);
            let az_option_layout_height_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutHeightValue) -> AzOptionLayoutHeightValue = transmute(lib.get(b"az_option_layout_height_value_deep_copy")?);
            let az_option_layout_height_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutHeightValue) -> AzString = transmute(lib.get(b"az_option_layout_height_value_fmt_debug")?);
            let az_option_layout_min_width_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMinWidthValue) = transmute(lib.get(b"az_option_layout_min_width_value_delete")?);
            let az_option_layout_min_width_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMinWidthValue) -> AzOptionLayoutMinWidthValue = transmute(lib.get(b"az_option_layout_min_width_value_deep_copy")?);
            let az_option_layout_min_width_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMinWidthValue) -> AzString = transmute(lib.get(b"az_option_layout_min_width_value_fmt_debug")?);
            let az_option_layout_min_height_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMinHeightValue) = transmute(lib.get(b"az_option_layout_min_height_value_delete")?);
            let az_option_layout_min_height_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMinHeightValue) -> AzOptionLayoutMinHeightValue = transmute(lib.get(b"az_option_layout_min_height_value_deep_copy")?);
            let az_option_layout_min_height_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMinHeightValue) -> AzString = transmute(lib.get(b"az_option_layout_min_height_value_fmt_debug")?);
            let az_option_layout_max_width_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMaxWidthValue) = transmute(lib.get(b"az_option_layout_max_width_value_delete")?);
            let az_option_layout_max_width_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMaxWidthValue) -> AzOptionLayoutMaxWidthValue = transmute(lib.get(b"az_option_layout_max_width_value_deep_copy")?);
            let az_option_layout_max_width_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMaxWidthValue) -> AzString = transmute(lib.get(b"az_option_layout_max_width_value_fmt_debug")?);
            let az_option_layout_max_height_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMaxHeightValue) = transmute(lib.get(b"az_option_layout_max_height_value_delete")?);
            let az_option_layout_max_height_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMaxHeightValue) -> AzOptionLayoutMaxHeightValue = transmute(lib.get(b"az_option_layout_max_height_value_deep_copy")?);
            let az_option_layout_max_height_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMaxHeightValue) -> AzString = transmute(lib.get(b"az_option_layout_max_height_value_fmt_debug")?);
            let az_option_layout_position_value_delete: extern "C" fn(_:  &mut AzOptionLayoutPositionValue) = transmute(lib.get(b"az_option_layout_position_value_delete")?);
            let az_option_layout_position_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutPositionValue) -> AzOptionLayoutPositionValue = transmute(lib.get(b"az_option_layout_position_value_deep_copy")?);
            let az_option_layout_position_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutPositionValue) -> AzString = transmute(lib.get(b"az_option_layout_position_value_fmt_debug")?);
            let az_option_layout_top_value_delete: extern "C" fn(_:  &mut AzOptionLayoutTopValue) = transmute(lib.get(b"az_option_layout_top_value_delete")?);
            let az_option_layout_top_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutTopValue) -> AzOptionLayoutTopValue = transmute(lib.get(b"az_option_layout_top_value_deep_copy")?);
            let az_option_layout_top_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutTopValue) -> AzString = transmute(lib.get(b"az_option_layout_top_value_fmt_debug")?);
            let az_option_layout_bottom_value_delete: extern "C" fn(_:  &mut AzOptionLayoutBottomValue) = transmute(lib.get(b"az_option_layout_bottom_value_delete")?);
            let az_option_layout_bottom_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutBottomValue) -> AzOptionLayoutBottomValue = transmute(lib.get(b"az_option_layout_bottom_value_deep_copy")?);
            let az_option_layout_bottom_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutBottomValue) -> AzString = transmute(lib.get(b"az_option_layout_bottom_value_fmt_debug")?);
            let az_option_layout_right_value_delete: extern "C" fn(_:  &mut AzOptionLayoutRightValue) = transmute(lib.get(b"az_option_layout_right_value_delete")?);
            let az_option_layout_right_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutRightValue) -> AzOptionLayoutRightValue = transmute(lib.get(b"az_option_layout_right_value_deep_copy")?);
            let az_option_layout_right_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutRightValue) -> AzString = transmute(lib.get(b"az_option_layout_right_value_fmt_debug")?);
            let az_option_layout_left_value_delete: extern "C" fn(_:  &mut AzOptionLayoutLeftValue) = transmute(lib.get(b"az_option_layout_left_value_delete")?);
            let az_option_layout_left_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutLeftValue) -> AzOptionLayoutLeftValue = transmute(lib.get(b"az_option_layout_left_value_deep_copy")?);
            let az_option_layout_left_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutLeftValue) -> AzString = transmute(lib.get(b"az_option_layout_left_value_fmt_debug")?);
            let az_option_layout_padding_top_value_delete: extern "C" fn(_:  &mut AzOptionLayoutPaddingTopValue) = transmute(lib.get(b"az_option_layout_padding_top_value_delete")?);
            let az_option_layout_padding_top_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutPaddingTopValue) -> AzOptionLayoutPaddingTopValue = transmute(lib.get(b"az_option_layout_padding_top_value_deep_copy")?);
            let az_option_layout_padding_top_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutPaddingTopValue) -> AzString = transmute(lib.get(b"az_option_layout_padding_top_value_fmt_debug")?);
            let az_option_layout_padding_bottom_value_delete: extern "C" fn(_:  &mut AzOptionLayoutPaddingBottomValue) = transmute(lib.get(b"az_option_layout_padding_bottom_value_delete")?);
            let az_option_layout_padding_bottom_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutPaddingBottomValue) -> AzOptionLayoutPaddingBottomValue = transmute(lib.get(b"az_option_layout_padding_bottom_value_deep_copy")?);
            let az_option_layout_padding_bottom_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutPaddingBottomValue) -> AzString = transmute(lib.get(b"az_option_layout_padding_bottom_value_fmt_debug")?);
            let az_option_layout_padding_left_value_delete: extern "C" fn(_:  &mut AzOptionLayoutPaddingLeftValue) = transmute(lib.get(b"az_option_layout_padding_left_value_delete")?);
            let az_option_layout_padding_left_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutPaddingLeftValue) -> AzOptionLayoutPaddingLeftValue = transmute(lib.get(b"az_option_layout_padding_left_value_deep_copy")?);
            let az_option_layout_padding_left_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutPaddingLeftValue) -> AzString = transmute(lib.get(b"az_option_layout_padding_left_value_fmt_debug")?);
            let az_option_layout_padding_right_value_delete: extern "C" fn(_:  &mut AzOptionLayoutPaddingRightValue) = transmute(lib.get(b"az_option_layout_padding_right_value_delete")?);
            let az_option_layout_padding_right_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutPaddingRightValue) -> AzOptionLayoutPaddingRightValue = transmute(lib.get(b"az_option_layout_padding_right_value_deep_copy")?);
            let az_option_layout_padding_right_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutPaddingRightValue) -> AzString = transmute(lib.get(b"az_option_layout_padding_right_value_fmt_debug")?);
            let az_option_layout_margin_top_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMarginTopValue) = transmute(lib.get(b"az_option_layout_margin_top_value_delete")?);
            let az_option_layout_margin_top_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMarginTopValue) -> AzOptionLayoutMarginTopValue = transmute(lib.get(b"az_option_layout_margin_top_value_deep_copy")?);
            let az_option_layout_margin_top_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMarginTopValue) -> AzString = transmute(lib.get(b"az_option_layout_margin_top_value_fmt_debug")?);
            let az_option_layout_margin_bottom_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMarginBottomValue) = transmute(lib.get(b"az_option_layout_margin_bottom_value_delete")?);
            let az_option_layout_margin_bottom_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMarginBottomValue) -> AzOptionLayoutMarginBottomValue = transmute(lib.get(b"az_option_layout_margin_bottom_value_deep_copy")?);
            let az_option_layout_margin_bottom_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMarginBottomValue) -> AzString = transmute(lib.get(b"az_option_layout_margin_bottom_value_fmt_debug")?);
            let az_option_layout_margin_left_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMarginLeftValue) = transmute(lib.get(b"az_option_layout_margin_left_value_delete")?);
            let az_option_layout_margin_left_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMarginLeftValue) -> AzOptionLayoutMarginLeftValue = transmute(lib.get(b"az_option_layout_margin_left_value_deep_copy")?);
            let az_option_layout_margin_left_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMarginLeftValue) -> AzString = transmute(lib.get(b"az_option_layout_margin_left_value_fmt_debug")?);
            let az_option_layout_margin_right_value_delete: extern "C" fn(_:  &mut AzOptionLayoutMarginRightValue) = transmute(lib.get(b"az_option_layout_margin_right_value_delete")?);
            let az_option_layout_margin_right_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutMarginRightValue) -> AzOptionLayoutMarginRightValue = transmute(lib.get(b"az_option_layout_margin_right_value_deep_copy")?);
            let az_option_layout_margin_right_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutMarginRightValue) -> AzString = transmute(lib.get(b"az_option_layout_margin_right_value_fmt_debug")?);
            let az_option_style_border_top_width_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderTopWidthValue) = transmute(lib.get(b"az_option_style_border_top_width_value_delete")?);
            let az_option_style_border_top_width_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderTopWidthValue) -> AzOptionStyleBorderTopWidthValue = transmute(lib.get(b"az_option_style_border_top_width_value_deep_copy")?);
            let az_option_style_border_top_width_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderTopWidthValue) -> AzString = transmute(lib.get(b"az_option_style_border_top_width_value_fmt_debug")?);
            let az_option_style_border_left_width_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderLeftWidthValue) = transmute(lib.get(b"az_option_style_border_left_width_value_delete")?);
            let az_option_style_border_left_width_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderLeftWidthValue) -> AzOptionStyleBorderLeftWidthValue = transmute(lib.get(b"az_option_style_border_left_width_value_deep_copy")?);
            let az_option_style_border_left_width_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderLeftWidthValue) -> AzString = transmute(lib.get(b"az_option_style_border_left_width_value_fmt_debug")?);
            let az_option_style_border_right_width_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderRightWidthValue) = transmute(lib.get(b"az_option_style_border_right_width_value_delete")?);
            let az_option_style_border_right_width_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderRightWidthValue) -> AzOptionStyleBorderRightWidthValue = transmute(lib.get(b"az_option_style_border_right_width_value_deep_copy")?);
            let az_option_style_border_right_width_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderRightWidthValue) -> AzString = transmute(lib.get(b"az_option_style_border_right_width_value_fmt_debug")?);
            let az_option_style_border_bottom_width_value_delete: extern "C" fn(_:  &mut AzOptionStyleBorderBottomWidthValue) = transmute(lib.get(b"az_option_style_border_bottom_width_value_delete")?);
            let az_option_style_border_bottom_width_value_deep_copy: extern "C" fn(_:  &AzOptionStyleBorderBottomWidthValue) -> AzOptionStyleBorderBottomWidthValue = transmute(lib.get(b"az_option_style_border_bottom_width_value_deep_copy")?);
            let az_option_style_border_bottom_width_value_fmt_debug: extern "C" fn(_:  &AzOptionStyleBorderBottomWidthValue) -> AzString = transmute(lib.get(b"az_option_style_border_bottom_width_value_fmt_debug")?);
            let az_option_overflow_value_delete: extern "C" fn(_:  &mut AzOptionOverflowValue) = transmute(lib.get(b"az_option_overflow_value_delete")?);
            let az_option_overflow_value_deep_copy: extern "C" fn(_:  &AzOptionOverflowValue) -> AzOptionOverflowValue = transmute(lib.get(b"az_option_overflow_value_deep_copy")?);
            let az_option_overflow_value_fmt_debug: extern "C" fn(_:  &AzOptionOverflowValue) -> AzString = transmute(lib.get(b"az_option_overflow_value_fmt_debug")?);
            let az_option_layout_direction_value_delete: extern "C" fn(_:  &mut AzOptionLayoutDirectionValue) = transmute(lib.get(b"az_option_layout_direction_value_delete")?);
            let az_option_layout_direction_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutDirectionValue) -> AzOptionLayoutDirectionValue = transmute(lib.get(b"az_option_layout_direction_value_deep_copy")?);
            let az_option_layout_direction_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutDirectionValue) -> AzString = transmute(lib.get(b"az_option_layout_direction_value_fmt_debug")?);
            let az_option_layout_wrap_value_delete: extern "C" fn(_:  &mut AzOptionLayoutWrapValue) = transmute(lib.get(b"az_option_layout_wrap_value_delete")?);
            let az_option_layout_wrap_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutWrapValue) -> AzOptionLayoutWrapValue = transmute(lib.get(b"az_option_layout_wrap_value_deep_copy")?);
            let az_option_layout_wrap_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutWrapValue) -> AzString = transmute(lib.get(b"az_option_layout_wrap_value_fmt_debug")?);
            let az_option_layout_flex_grow_value_delete: extern "C" fn(_:  &mut AzOptionLayoutFlexGrowValue) = transmute(lib.get(b"az_option_layout_flex_grow_value_delete")?);
            let az_option_layout_flex_grow_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutFlexGrowValue) -> AzOptionLayoutFlexGrowValue = transmute(lib.get(b"az_option_layout_flex_grow_value_deep_copy")?);
            let az_option_layout_flex_grow_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutFlexGrowValue) -> AzString = transmute(lib.get(b"az_option_layout_flex_grow_value_fmt_debug")?);
            let az_option_layout_flex_shrink_value_delete: extern "C" fn(_:  &mut AzOptionLayoutFlexShrinkValue) = transmute(lib.get(b"az_option_layout_flex_shrink_value_delete")?);
            let az_option_layout_flex_shrink_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutFlexShrinkValue) -> AzOptionLayoutFlexShrinkValue = transmute(lib.get(b"az_option_layout_flex_shrink_value_deep_copy")?);
            let az_option_layout_flex_shrink_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutFlexShrinkValue) -> AzString = transmute(lib.get(b"az_option_layout_flex_shrink_value_fmt_debug")?);
            let az_option_layout_justify_content_value_delete: extern "C" fn(_:  &mut AzOptionLayoutJustifyContentValue) = transmute(lib.get(b"az_option_layout_justify_content_value_delete")?);
            let az_option_layout_justify_content_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutJustifyContentValue) -> AzOptionLayoutJustifyContentValue = transmute(lib.get(b"az_option_layout_justify_content_value_deep_copy")?);
            let az_option_layout_justify_content_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutJustifyContentValue) -> AzString = transmute(lib.get(b"az_option_layout_justify_content_value_fmt_debug")?);
            let az_option_layout_align_items_value_delete: extern "C" fn(_:  &mut AzOptionLayoutAlignItemsValue) = transmute(lib.get(b"az_option_layout_align_items_value_delete")?);
            let az_option_layout_align_items_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutAlignItemsValue) -> AzOptionLayoutAlignItemsValue = transmute(lib.get(b"az_option_layout_align_items_value_deep_copy")?);
            let az_option_layout_align_items_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutAlignItemsValue) -> AzString = transmute(lib.get(b"az_option_layout_align_items_value_fmt_debug")?);
            let az_option_layout_align_content_value_delete: extern "C" fn(_:  &mut AzOptionLayoutAlignContentValue) = transmute(lib.get(b"az_option_layout_align_content_value_delete")?);
            let az_option_layout_align_content_value_deep_copy: extern "C" fn(_:  &AzOptionLayoutAlignContentValue) -> AzOptionLayoutAlignContentValue = transmute(lib.get(b"az_option_layout_align_content_value_deep_copy")?);
            let az_option_layout_align_content_value_fmt_debug: extern "C" fn(_:  &AzOptionLayoutAlignContentValue) -> AzString = transmute(lib.get(b"az_option_layout_align_content_value_fmt_debug")?);
            let az_option_hover_group_delete: extern "C" fn(_:  &mut AzOptionHoverGroup) = transmute(lib.get(b"az_option_hover_group_delete")?);
            let az_option_hover_group_deep_copy: extern "C" fn(_:  &AzOptionHoverGroup) -> AzOptionHoverGroup = transmute(lib.get(b"az_option_hover_group_deep_copy")?);
            let az_option_hover_group_fmt_debug: extern "C" fn(_:  &AzOptionHoverGroup) -> AzString = transmute(lib.get(b"az_option_hover_group_fmt_debug")?);
            let az_option_tag_id_delete: extern "C" fn(_:  &mut AzOptionTagId) = transmute(lib.get(b"az_option_tag_id_delete")?);
            let az_option_tag_id_deep_copy: extern "C" fn(_:  &AzOptionTagId) -> AzOptionTagId = transmute(lib.get(b"az_option_tag_id_deep_copy")?);
            let az_option_tag_id_fmt_debug: extern "C" fn(_:  &AzOptionTagId) -> AzString = transmute(lib.get(b"az_option_tag_id_fmt_debug")?);
            let az_option_duration_delete: extern "C" fn(_:  &mut AzOptionDuration) = transmute(lib.get(b"az_option_duration_delete")?);
            let az_option_duration_deep_copy: extern "C" fn(_:  &AzOptionDuration) -> AzOptionDuration = transmute(lib.get(b"az_option_duration_deep_copy")?);
            let az_option_duration_fmt_debug: extern "C" fn(_:  &AzOptionDuration) -> AzString = transmute(lib.get(b"az_option_duration_fmt_debug")?);
            let az_option_instant_ptr_delete: extern "C" fn(_:  &mut AzOptionInstantPtr) = transmute(lib.get(b"az_option_instant_ptr_delete")?);
            let az_option_instant_ptr_deep_copy: extern "C" fn(_:  &AzOptionInstantPtr) -> AzOptionInstantPtr = transmute(lib.get(b"az_option_instant_ptr_deep_copy")?);
            let az_option_instant_ptr_fmt_debug: extern "C" fn(_:  &AzOptionInstantPtr) -> AzString = transmute(lib.get(b"az_option_instant_ptr_fmt_debug")?);
            let az_option_usize_delete: extern "C" fn(_:  &mut AzOptionUsize) = transmute(lib.get(b"az_option_usize_delete")?);
            let az_option_usize_deep_copy: extern "C" fn(_:  &AzOptionUsize) -> AzOptionUsize = transmute(lib.get(b"az_option_usize_deep_copy")?);
            let az_option_usize_fmt_debug: extern "C" fn(_:  &AzOptionUsize) -> AzString = transmute(lib.get(b"az_option_usize_fmt_debug")?);
            let az_option_u8_vec_ref_delete: extern "C" fn(_:  &mut AzOptionU8VecRef) = transmute(lib.get(b"az_option_u8_vec_ref_delete")?);
            let az_option_u8_vec_ref_fmt_debug: extern "C" fn(_:  &AzOptionU8VecRef) -> AzString = transmute(lib.get(b"az_option_u8_vec_ref_fmt_debug")?);
            let az_result_svg_svg_parse_error_delete: extern "C" fn(_:  &mut AzResultSvgSvgParseError) = transmute(lib.get(b"az_result_svg_svg_parse_error_delete")?);
            let az_result_svg_svg_parse_error_deep_copy: extern "C" fn(_:  &AzResultSvgSvgParseError) -> AzResultSvgSvgParseError = transmute(lib.get(b"az_result_svg_svg_parse_error_deep_copy")?);
            let az_result_svg_svg_parse_error_fmt_debug: extern "C" fn(_:  &AzResultSvgSvgParseError) -> AzString = transmute(lib.get(b"az_result_svg_svg_parse_error_fmt_debug")?);
            let az_result_ref_any_block_error_delete: extern "C" fn(_:  &mut AzResultRefAnyBlockError) = transmute(lib.get(b"az_result_ref_any_block_error_delete")?);
            let az_result_ref_any_block_error_deep_copy: extern "C" fn(_:  &AzResultRefAnyBlockError) -> AzResultRefAnyBlockError = transmute(lib.get(b"az_result_ref_any_block_error_deep_copy")?);
            let az_result_ref_any_block_error_fmt_debug: extern "C" fn(_:  &AzResultRefAnyBlockError) -> AzString = transmute(lib.get(b"az_result_ref_any_block_error_fmt_debug")?);
            let az_svg_parse_error_delete: extern "C" fn(_:  &mut AzSvgParseError) = transmute(lib.get(b"az_svg_parse_error_delete")?);
            let az_svg_parse_error_deep_copy: extern "C" fn(_:  &AzSvgParseError) -> AzSvgParseError = transmute(lib.get(b"az_svg_parse_error_deep_copy")?);
            let az_svg_parse_error_fmt_debug: extern "C" fn(_:  &AzSvgParseError) -> AzString = transmute(lib.get(b"az_svg_parse_error_fmt_debug")?);
            let az_xml_error_delete: extern "C" fn(_:  &mut AzXmlError) = transmute(lib.get(b"az_xml_error_delete")?);
            let az_xml_error_deep_copy: extern "C" fn(_:  &AzXmlError) -> AzXmlError = transmute(lib.get(b"az_xml_error_deep_copy")?);
            let az_xml_error_fmt_debug: extern "C" fn(_:  &AzXmlError) -> AzString = transmute(lib.get(b"az_xml_error_fmt_debug")?);
            let az_duplicated_namespace_error_delete: extern "C" fn(_:  &mut AzDuplicatedNamespaceError) = transmute(lib.get(b"az_duplicated_namespace_error_delete")?);
            let az_duplicated_namespace_error_deep_copy: extern "C" fn(_:  &AzDuplicatedNamespaceError) -> AzDuplicatedNamespaceError = transmute(lib.get(b"az_duplicated_namespace_error_deep_copy")?);
            let az_duplicated_namespace_error_fmt_debug: extern "C" fn(_:  &AzDuplicatedNamespaceError) -> AzString = transmute(lib.get(b"az_duplicated_namespace_error_fmt_debug")?);
            let az_unknown_namespace_error_delete: extern "C" fn(_:  &mut AzUnknownNamespaceError) = transmute(lib.get(b"az_unknown_namespace_error_delete")?);
            let az_unknown_namespace_error_deep_copy: extern "C" fn(_:  &AzUnknownNamespaceError) -> AzUnknownNamespaceError = transmute(lib.get(b"az_unknown_namespace_error_deep_copy")?);
            let az_unknown_namespace_error_fmt_debug: extern "C" fn(_:  &AzUnknownNamespaceError) -> AzString = transmute(lib.get(b"az_unknown_namespace_error_fmt_debug")?);
            let az_unexpected_close_tag_error_delete: extern "C" fn(_:  &mut AzUnexpectedCloseTagError) = transmute(lib.get(b"az_unexpected_close_tag_error_delete")?);
            let az_unexpected_close_tag_error_deep_copy: extern "C" fn(_:  &AzUnexpectedCloseTagError) -> AzUnexpectedCloseTagError = transmute(lib.get(b"az_unexpected_close_tag_error_deep_copy")?);
            let az_unexpected_close_tag_error_fmt_debug: extern "C" fn(_:  &AzUnexpectedCloseTagError) -> AzString = transmute(lib.get(b"az_unexpected_close_tag_error_fmt_debug")?);
            let az_unknown_entity_reference_error_delete: extern "C" fn(_:  &mut AzUnknownEntityReferenceError) = transmute(lib.get(b"az_unknown_entity_reference_error_delete")?);
            let az_unknown_entity_reference_error_deep_copy: extern "C" fn(_:  &AzUnknownEntityReferenceError) -> AzUnknownEntityReferenceError = transmute(lib.get(b"az_unknown_entity_reference_error_deep_copy")?);
            let az_unknown_entity_reference_error_fmt_debug: extern "C" fn(_:  &AzUnknownEntityReferenceError) -> AzString = transmute(lib.get(b"az_unknown_entity_reference_error_fmt_debug")?);
            let az_duplicated_attribute_error_delete: extern "C" fn(_:  &mut AzDuplicatedAttributeError) = transmute(lib.get(b"az_duplicated_attribute_error_delete")?);
            let az_duplicated_attribute_error_deep_copy: extern "C" fn(_:  &AzDuplicatedAttributeError) -> AzDuplicatedAttributeError = transmute(lib.get(b"az_duplicated_attribute_error_deep_copy")?);
            let az_duplicated_attribute_error_fmt_debug: extern "C" fn(_:  &AzDuplicatedAttributeError) -> AzString = transmute(lib.get(b"az_duplicated_attribute_error_fmt_debug")?);
            let az_xml_parse_error_delete: extern "C" fn(_:  &mut AzXmlParseError) = transmute(lib.get(b"az_xml_parse_error_delete")?);
            let az_xml_parse_error_deep_copy: extern "C" fn(_:  &AzXmlParseError) -> AzXmlParseError = transmute(lib.get(b"az_xml_parse_error_deep_copy")?);
            let az_xml_parse_error_fmt_debug: extern "C" fn(_:  &AzXmlParseError) -> AzString = transmute(lib.get(b"az_xml_parse_error_fmt_debug")?);
            let az_xml_text_error_delete: extern "C" fn(_:  &mut AzXmlTextError) = transmute(lib.get(b"az_xml_text_error_delete")?);
            let az_xml_text_error_deep_copy: extern "C" fn(_:  &AzXmlTextError) -> AzXmlTextError = transmute(lib.get(b"az_xml_text_error_deep_copy")?);
            let az_xml_text_error_fmt_debug: extern "C" fn(_:  &AzXmlTextError) -> AzString = transmute(lib.get(b"az_xml_text_error_fmt_debug")?);
            let az_xml_stream_error_delete: extern "C" fn(_:  &mut AzXmlStreamError) = transmute(lib.get(b"az_xml_stream_error_delete")?);
            let az_xml_stream_error_deep_copy: extern "C" fn(_:  &AzXmlStreamError) -> AzXmlStreamError = transmute(lib.get(b"az_xml_stream_error_deep_copy")?);
            let az_xml_stream_error_fmt_debug: extern "C" fn(_:  &AzXmlStreamError) -> AzString = transmute(lib.get(b"az_xml_stream_error_fmt_debug")?);
            let az_non_xml_char_error_delete: extern "C" fn(_:  &mut AzNonXmlCharError) = transmute(lib.get(b"az_non_xml_char_error_delete")?);
            let az_non_xml_char_error_deep_copy: extern "C" fn(_:  &AzNonXmlCharError) -> AzNonXmlCharError = transmute(lib.get(b"az_non_xml_char_error_deep_copy")?);
            let az_non_xml_char_error_fmt_debug: extern "C" fn(_:  &AzNonXmlCharError) -> AzString = transmute(lib.get(b"az_non_xml_char_error_fmt_debug")?);
            let az_invalid_char_error_delete: extern "C" fn(_:  &mut AzInvalidCharError) = transmute(lib.get(b"az_invalid_char_error_delete")?);
            let az_invalid_char_error_deep_copy: extern "C" fn(_:  &AzInvalidCharError) -> AzInvalidCharError = transmute(lib.get(b"az_invalid_char_error_deep_copy")?);
            let az_invalid_char_error_fmt_debug: extern "C" fn(_:  &AzInvalidCharError) -> AzString = transmute(lib.get(b"az_invalid_char_error_fmt_debug")?);
            let az_invalid_char_multiple_error_delete: extern "C" fn(_:  &mut AzInvalidCharMultipleError) = transmute(lib.get(b"az_invalid_char_multiple_error_delete")?);
            let az_invalid_char_multiple_error_deep_copy: extern "C" fn(_:  &AzInvalidCharMultipleError) -> AzInvalidCharMultipleError = transmute(lib.get(b"az_invalid_char_multiple_error_deep_copy")?);
            let az_invalid_char_multiple_error_fmt_debug: extern "C" fn(_:  &AzInvalidCharMultipleError) -> AzString = transmute(lib.get(b"az_invalid_char_multiple_error_fmt_debug")?);
            let az_invalid_quote_error_delete: extern "C" fn(_:  &mut AzInvalidQuoteError) = transmute(lib.get(b"az_invalid_quote_error_delete")?);
            let az_invalid_quote_error_deep_copy: extern "C" fn(_:  &AzInvalidQuoteError) -> AzInvalidQuoteError = transmute(lib.get(b"az_invalid_quote_error_deep_copy")?);
            let az_invalid_quote_error_fmt_debug: extern "C" fn(_:  &AzInvalidQuoteError) -> AzString = transmute(lib.get(b"az_invalid_quote_error_fmt_debug")?);
            let az_invalid_space_error_delete: extern "C" fn(_:  &mut AzInvalidSpaceError) = transmute(lib.get(b"az_invalid_space_error_delete")?);
            let az_invalid_space_error_deep_copy: extern "C" fn(_:  &AzInvalidSpaceError) -> AzInvalidSpaceError = transmute(lib.get(b"az_invalid_space_error_deep_copy")?);
            let az_invalid_space_error_fmt_debug: extern "C" fn(_:  &AzInvalidSpaceError) -> AzString = transmute(lib.get(b"az_invalid_space_error_fmt_debug")?);
            let az_invalid_string_error_delete: extern "C" fn(_:  &mut AzInvalidStringError) = transmute(lib.get(b"az_invalid_string_error_delete")?);
            let az_invalid_string_error_deep_copy: extern "C" fn(_:  &AzInvalidStringError) -> AzInvalidStringError = transmute(lib.get(b"az_invalid_string_error_deep_copy")?);
            let az_invalid_string_error_fmt_debug: extern "C" fn(_:  &AzInvalidStringError) -> AzString = transmute(lib.get(b"az_invalid_string_error_fmt_debug")?);
            let az_xml_text_pos_delete: extern "C" fn(_:  &mut AzXmlTextPos) = transmute(lib.get(b"az_xml_text_pos_delete")?);
            let az_xml_text_pos_deep_copy: extern "C" fn(_:  &AzXmlTextPos) -> AzXmlTextPos = transmute(lib.get(b"az_xml_text_pos_deep_copy")?);
            let az_xml_text_pos_fmt_debug: extern "C" fn(_:  &AzXmlTextPos) -> AzString = transmute(lib.get(b"az_xml_text_pos_fmt_debug")?);
            let az_instant_ptr_now: extern "C" fn() -> AzInstantPtr = transmute(lib.get(b"az_instant_ptr_now")?);
            let az_instant_ptr_delete: extern "C" fn(_:  &mut AzInstantPtr) = transmute(lib.get(b"az_instant_ptr_delete")?);
            let az_instant_ptr_fmt_debug: extern "C" fn(_:  &AzInstantPtr) -> AzString = transmute(lib.get(b"az_instant_ptr_fmt_debug")?);
            let az_instant_ptr_partial_eq: extern "C" fn(_:  &AzInstantPtr, _:  &AzInstantPtr) -> bool = transmute(lib.get(b"az_instant_ptr_partial_eq")?);
            let az_instant_ptr_partial_cmp: extern "C" fn(_:  &AzInstantPtr, _:  &AzInstantPtr) -> u8 = transmute(lib.get(b"az_instant_ptr_partial_cmp")?);
            let az_instant_ptr_cmp: extern "C" fn(_:  &AzInstantPtr, _:  &AzInstantPtr) -> u8 = transmute(lib.get(b"az_instant_ptr_cmp")?);
            let az_duration_delete: extern "C" fn(_:  &mut AzDuration) = transmute(lib.get(b"az_duration_delete")?);
            let az_duration_deep_copy: extern "C" fn(_:  &AzDuration) -> AzDuration = transmute(lib.get(b"az_duration_deep_copy")?);
            let az_duration_fmt_debug: extern "C" fn(_:  &AzDuration) -> AzString = transmute(lib.get(b"az_duration_fmt_debug")?);
            let az_app_config_ptr_default: extern "C" fn() -> AzAppConfigPtr = transmute(lib.get(b"az_app_config_ptr_default")?);
            let az_app_config_ptr_delete: extern "C" fn(_:  &mut AzAppConfigPtr) = transmute(lib.get(b"az_app_config_ptr_delete")?);
            let az_app_config_ptr_fmt_debug: extern "C" fn(_:  &AzAppConfigPtr) -> AzString = transmute(lib.get(b"az_app_config_ptr_fmt_debug")?);
            let az_app_ptr_new: extern "C" fn(_:  AzRefAny, _:  AzAppConfigPtr) -> AzAppPtr = transmute(lib.get(b"az_app_ptr_new")?);
            let az_app_ptr_run: extern "C" fn(_:  AzAppPtr, _:  AzWindowCreateOptions) = transmute(lib.get(b"az_app_ptr_run")?);
            let az_app_ptr_delete: extern "C" fn(_:  &mut AzAppPtr) = transmute(lib.get(b"az_app_ptr_delete")?);
            let az_app_ptr_fmt_debug: extern "C" fn(_:  &AzAppPtr) -> AzString = transmute(lib.get(b"az_app_ptr_fmt_debug")?);
            let az_node_id_delete: extern "C" fn(_:  &mut AzNodeId) = transmute(lib.get(b"az_node_id_delete")?);
            let az_node_id_deep_copy: extern "C" fn(_:  &AzNodeId) -> AzNodeId = transmute(lib.get(b"az_node_id_deep_copy")?);
            let az_node_id_fmt_debug: extern "C" fn(_:  &AzNodeId) -> AzString = transmute(lib.get(b"az_node_id_fmt_debug")?);
            let az_dom_id_delete: extern "C" fn(_:  &mut AzDomId) = transmute(lib.get(b"az_dom_id_delete")?);
            let az_dom_id_deep_copy: extern "C" fn(_:  &AzDomId) -> AzDomId = transmute(lib.get(b"az_dom_id_deep_copy")?);
            let az_dom_id_fmt_debug: extern "C" fn(_:  &AzDomId) -> AzString = transmute(lib.get(b"az_dom_id_fmt_debug")?);
            let az_dom_node_id_delete: extern "C" fn(_:  &mut AzDomNodeId) = transmute(lib.get(b"az_dom_node_id_delete")?);
            let az_dom_node_id_deep_copy: extern "C" fn(_:  &AzDomNodeId) -> AzDomNodeId = transmute(lib.get(b"az_dom_node_id_deep_copy")?);
            let az_dom_node_id_fmt_debug: extern "C" fn(_:  &AzDomNodeId) -> AzString = transmute(lib.get(b"az_dom_node_id_fmt_debug")?);
            let az_hidpi_adjusted_bounds_get_logical_size: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzLogicalSize = transmute(lib.get(b"az_hidpi_adjusted_bounds_get_logical_size")?);
            let az_hidpi_adjusted_bounds_get_physical_size: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzPhysicalSizeU32 = transmute(lib.get(b"az_hidpi_adjusted_bounds_get_physical_size")?);
            let az_hidpi_adjusted_bounds_get_hidpi_factor: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> f32 = transmute(lib.get(b"az_hidpi_adjusted_bounds_get_hidpi_factor")?);
            let az_hidpi_adjusted_bounds_delete: extern "C" fn(_:  &mut AzHidpiAdjustedBounds) = transmute(lib.get(b"az_hidpi_adjusted_bounds_delete")?);
            let az_hidpi_adjusted_bounds_deep_copy: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzHidpiAdjustedBounds = transmute(lib.get(b"az_hidpi_adjusted_bounds_deep_copy")?);
            let az_hidpi_adjusted_bounds_fmt_debug: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzString = transmute(lib.get(b"az_hidpi_adjusted_bounds_fmt_debug")?);
            let az_layout_callback_delete: extern "C" fn(_:  &mut AzLayoutCallback) = transmute(lib.get(b"az_layout_callback_delete")?);
            let az_layout_callback_deep_copy: extern "C" fn(_:  &AzLayoutCallback) -> AzLayoutCallback = transmute(lib.get(b"az_layout_callback_deep_copy")?);
            let az_layout_callback_fmt_debug: extern "C" fn(_:  &AzLayoutCallback) -> AzString = transmute(lib.get(b"az_layout_callback_fmt_debug")?);
            let az_callback_delete: extern "C" fn(_:  &mut AzCallback) = transmute(lib.get(b"az_callback_delete")?);
            let az_callback_deep_copy: extern "C" fn(_:  &AzCallback) -> AzCallback = transmute(lib.get(b"az_callback_deep_copy")?);
            let az_callback_fmt_debug: extern "C" fn(_:  &AzCallback) -> AzString = transmute(lib.get(b"az_callback_fmt_debug")?);
            let az_callback_partial_eq: extern "C" fn(_:  &AzCallback, _:  &AzCallback) -> bool = transmute(lib.get(b"az_callback_partial_eq")?);
            let az_callback_partial_cmp: extern "C" fn(_:  &AzCallback, _:  &AzCallback) -> u8 = transmute(lib.get(b"az_callback_partial_cmp")?);
            let az_callback_cmp: extern "C" fn(_:  &AzCallback, _:  &AzCallback) -> u8 = transmute(lib.get(b"az_callback_cmp")?);
            let az_callback_hash: extern "C" fn(_:  &AzCallback) -> u64 = transmute(lib.get(b"az_callback_hash")?);
            let az_callback_info_ptr_get_state: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzRefAny = transmute(lib.get(b"az_callback_info_ptr_get_state")?);
            let az_callback_info_ptr_get_keyboard_state: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzKeyboardState = transmute(lib.get(b"az_callback_info_ptr_get_keyboard_state")?);
            let az_callback_info_ptr_get_mouse_state: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzMouseState = transmute(lib.get(b"az_callback_info_ptr_get_mouse_state")?);
            let az_callback_info_ptr_set_window_state: extern "C" fn(_:  &mut AzCallbackInfoPtr, _:  AzWindowState) = transmute(lib.get(b"az_callback_info_ptr_set_window_state")?);
            let az_callback_info_ptr_delete: extern "C" fn(_:  &mut AzCallbackInfoPtr) = transmute(lib.get(b"az_callback_info_ptr_delete")?);
            let az_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzString = transmute(lib.get(b"az_callback_info_ptr_fmt_debug")?);
            let az_update_screen_delete: extern "C" fn(_:  &mut AzUpdateScreen) = transmute(lib.get(b"az_update_screen_delete")?);
            let az_update_screen_deep_copy: extern "C" fn(_:  &AzUpdateScreen) -> AzUpdateScreen = transmute(lib.get(b"az_update_screen_deep_copy")?);
            let az_update_screen_fmt_debug: extern "C" fn(_:  &AzUpdateScreen) -> AzString = transmute(lib.get(b"az_update_screen_fmt_debug")?);
            let az_i_frame_callback_delete: extern "C" fn(_:  &mut AzIFrameCallback) = transmute(lib.get(b"az_i_frame_callback_delete")?);
            let az_i_frame_callback_deep_copy: extern "C" fn(_:  &AzIFrameCallback) -> AzIFrameCallback = transmute(lib.get(b"az_i_frame_callback_deep_copy")?);
            let az_i_frame_callback_fmt_debug: extern "C" fn(_:  &AzIFrameCallback) -> AzString = transmute(lib.get(b"az_i_frame_callback_fmt_debug")?);
            let az_i_frame_callback_info_ptr_get_state: extern "C" fn(_:  &AzIFrameCallbackInfoPtr) -> AzRefAny = transmute(lib.get(b"az_i_frame_callback_info_ptr_get_state")?);
            let az_i_frame_callback_info_ptr_get_bounds: extern "C" fn(_:  &AzIFrameCallbackInfoPtr) -> AzHidpiAdjustedBounds = transmute(lib.get(b"az_i_frame_callback_info_ptr_get_bounds")?);
            let az_i_frame_callback_info_ptr_delete: extern "C" fn(_:  &mut AzIFrameCallbackInfoPtr) = transmute(lib.get(b"az_i_frame_callback_info_ptr_delete")?);
            let az_i_frame_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzIFrameCallbackInfoPtr) -> AzString = transmute(lib.get(b"az_i_frame_callback_info_ptr_fmt_debug")?);
            let az_i_frame_callback_return_delete: extern "C" fn(_:  &mut AzIFrameCallbackReturn) = transmute(lib.get(b"az_i_frame_callback_return_delete")?);
            let az_i_frame_callback_return_deep_copy: extern "C" fn(_:  &AzIFrameCallbackReturn) -> AzIFrameCallbackReturn = transmute(lib.get(b"az_i_frame_callback_return_deep_copy")?);
            let az_i_frame_callback_return_fmt_debug: extern "C" fn(_:  &AzIFrameCallbackReturn) -> AzString = transmute(lib.get(b"az_i_frame_callback_return_fmt_debug")?);
            let az_gl_callback_delete: extern "C" fn(_:  &mut AzGlCallback) = transmute(lib.get(b"az_gl_callback_delete")?);
            let az_gl_callback_deep_copy: extern "C" fn(_:  &AzGlCallback) -> AzGlCallback = transmute(lib.get(b"az_gl_callback_deep_copy")?);
            let az_gl_callback_fmt_debug: extern "C" fn(_:  &AzGlCallback) -> AzString = transmute(lib.get(b"az_gl_callback_fmt_debug")?);
            let az_gl_callback_info_ptr_get_state: extern "C" fn(_:  &AzGlCallbackInfoPtr) -> AzRefAny = transmute(lib.get(b"az_gl_callback_info_ptr_get_state")?);
            let az_gl_callback_info_ptr_get_gl_context: extern "C" fn(_:  &AzGlCallbackInfoPtr) -> AzGlContextPtr = transmute(lib.get(b"az_gl_callback_info_ptr_get_gl_context")?);
            let az_gl_callback_info_ptr_get_bounds: extern "C" fn(_:  &AzGlCallbackInfoPtr) -> AzHidpiAdjustedBounds = transmute(lib.get(b"az_gl_callback_info_ptr_get_bounds")?);
            let az_gl_callback_info_ptr_delete: extern "C" fn(_:  &mut AzGlCallbackInfoPtr) = transmute(lib.get(b"az_gl_callback_info_ptr_delete")?);
            let az_gl_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzGlCallbackInfoPtr) -> AzString = transmute(lib.get(b"az_gl_callback_info_ptr_fmt_debug")?);
            let az_gl_callback_return_delete: extern "C" fn(_:  &mut AzGlCallbackReturn) = transmute(lib.get(b"az_gl_callback_return_delete")?);
            let az_gl_callback_return_fmt_debug: extern "C" fn(_:  &AzGlCallbackReturn) -> AzString = transmute(lib.get(b"az_gl_callback_return_fmt_debug")?);
            let az_timer_callback_delete: extern "C" fn(_:  &mut AzTimerCallback) = transmute(lib.get(b"az_timer_callback_delete")?);
            let az_timer_callback_deep_copy: extern "C" fn(_:  &AzTimerCallback) -> AzTimerCallback = transmute(lib.get(b"az_timer_callback_deep_copy")?);
            let az_timer_callback_fmt_debug: extern "C" fn(_:  &AzTimerCallback) -> AzString = transmute(lib.get(b"az_timer_callback_fmt_debug")?);
            let az_timer_callback_type_ptr_delete: extern "C" fn(_:  &mut AzTimerCallbackTypePtr) = transmute(lib.get(b"az_timer_callback_type_ptr_delete")?);
            let az_timer_callback_type_ptr_fmt_debug: extern "C" fn(_:  &AzTimerCallbackTypePtr) -> AzString = transmute(lib.get(b"az_timer_callback_type_ptr_fmt_debug")?);
            let az_timer_callback_info_ptr_get_state: extern "C" fn(_:  &AzTimerCallbackInfoPtr) -> AzRefAny = transmute(lib.get(b"az_timer_callback_info_ptr_get_state")?);
            let az_timer_callback_info_ptr_delete: extern "C" fn(_:  &mut AzTimerCallbackInfoPtr) = transmute(lib.get(b"az_timer_callback_info_ptr_delete")?);
            let az_timer_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzTimerCallbackInfoPtr) -> AzString = transmute(lib.get(b"az_timer_callback_info_ptr_fmt_debug")?);
            let az_timer_callback_return_delete: extern "C" fn(_:  &mut AzTimerCallbackReturn) = transmute(lib.get(b"az_timer_callback_return_delete")?);
            let az_timer_callback_return_deep_copy: extern "C" fn(_:  &AzTimerCallbackReturn) -> AzTimerCallbackReturn = transmute(lib.get(b"az_timer_callback_return_deep_copy")?);
            let az_timer_callback_return_fmt_debug: extern "C" fn(_:  &AzTimerCallbackReturn) -> AzString = transmute(lib.get(b"az_timer_callback_return_fmt_debug")?);
            let az_ref_any_sharing_info_can_be_shared: extern "C" fn(_:  &AzRefAnySharingInfo) -> bool = transmute(lib.get(b"az_ref_any_sharing_info_can_be_shared")?);
            let az_ref_any_sharing_info_can_be_shared_mut: extern "C" fn(_:  &AzRefAnySharingInfo) -> bool = transmute(lib.get(b"az_ref_any_sharing_info_can_be_shared_mut")?);
            let az_ref_any_sharing_info_increase_ref: extern "C" fn(_:  &mut AzRefAnySharingInfo) = transmute(lib.get(b"az_ref_any_sharing_info_increase_ref")?);
            let az_ref_any_sharing_info_decrease_ref: extern "C" fn(_:  &mut AzRefAnySharingInfo) = transmute(lib.get(b"az_ref_any_sharing_info_decrease_ref")?);
            let az_ref_any_sharing_info_increase_refmut: extern "C" fn(_:  &mut AzRefAnySharingInfo) = transmute(lib.get(b"az_ref_any_sharing_info_increase_refmut")?);
            let az_ref_any_sharing_info_decrease_refmut: extern "C" fn(_:  &mut AzRefAnySharingInfo) = transmute(lib.get(b"az_ref_any_sharing_info_decrease_refmut")?);
            let az_ref_any_sharing_info_delete: extern "C" fn(_:  &mut AzRefAnySharingInfo) = transmute(lib.get(b"az_ref_any_sharing_info_delete")?);
            let az_ref_any_sharing_info_fmt_debug: extern "C" fn(_:  &AzRefAnySharingInfo) -> AzString = transmute(lib.get(b"az_ref_any_sharing_info_fmt_debug")?);
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
            let az_ref_any_fmt_debug: extern "C" fn(_:  &AzRefAny) -> AzString = transmute(lib.get(b"az_ref_any_fmt_debug")?);
            let az_ref_any_partial_eq: extern "C" fn(_:  &AzRefAny, _:  &AzRefAny) -> bool = transmute(lib.get(b"az_ref_any_partial_eq")?);
            let az_ref_any_partial_cmp: extern "C" fn(_:  &AzRefAny, _:  &AzRefAny) -> u8 = transmute(lib.get(b"az_ref_any_partial_cmp")?);
            let az_ref_any_cmp: extern "C" fn(_:  &AzRefAny, _:  &AzRefAny) -> u8 = transmute(lib.get(b"az_ref_any_cmp")?);
            let az_ref_any_hash: extern "C" fn(_:  &AzRefAny) -> u64 = transmute(lib.get(b"az_ref_any_hash")?);
            let az_layout_info_ptr_get_gl_context: extern "C" fn(_:  &AzLayoutInfoPtr) -> AzGlContextPtr = transmute(lib.get(b"az_layout_info_ptr_get_gl_context")?);
            let az_layout_info_ptr_delete: extern "C" fn(_:  &mut AzLayoutInfoPtr) = transmute(lib.get(b"az_layout_info_ptr_delete")?);
            let az_layout_info_ptr_fmt_debug: extern "C" fn(_:  &AzLayoutInfoPtr) -> AzString = transmute(lib.get(b"az_layout_info_ptr_fmt_debug")?);
            let az_css_rule_block_delete: extern "C" fn(_:  &mut AzCssRuleBlock) = transmute(lib.get(b"az_css_rule_block_delete")?);
            let az_css_rule_block_deep_copy: extern "C" fn(_:  &AzCssRuleBlock) -> AzCssRuleBlock = transmute(lib.get(b"az_css_rule_block_deep_copy")?);
            let az_css_rule_block_fmt_debug: extern "C" fn(_:  &AzCssRuleBlock) -> AzString = transmute(lib.get(b"az_css_rule_block_fmt_debug")?);
            let az_css_declaration_delete: extern "C" fn(_:  &mut AzCssDeclaration) = transmute(lib.get(b"az_css_declaration_delete")?);
            let az_css_declaration_deep_copy: extern "C" fn(_:  &AzCssDeclaration) -> AzCssDeclaration = transmute(lib.get(b"az_css_declaration_deep_copy")?);
            let az_css_declaration_fmt_debug: extern "C" fn(_:  &AzCssDeclaration) -> AzString = transmute(lib.get(b"az_css_declaration_fmt_debug")?);
            let az_dynamic_css_property_delete: extern "C" fn(_:  &mut AzDynamicCssProperty) = transmute(lib.get(b"az_dynamic_css_property_delete")?);
            let az_dynamic_css_property_deep_copy: extern "C" fn(_:  &AzDynamicCssProperty) -> AzDynamicCssProperty = transmute(lib.get(b"az_dynamic_css_property_deep_copy")?);
            let az_dynamic_css_property_fmt_debug: extern "C" fn(_:  &AzDynamicCssProperty) -> AzString = transmute(lib.get(b"az_dynamic_css_property_fmt_debug")?);
            let az_css_path_delete: extern "C" fn(_:  &mut AzCssPath) = transmute(lib.get(b"az_css_path_delete")?);
            let az_css_path_deep_copy: extern "C" fn(_:  &AzCssPath) -> AzCssPath = transmute(lib.get(b"az_css_path_deep_copy")?);
            let az_css_path_fmt_debug: extern "C" fn(_:  &AzCssPath) -> AzString = transmute(lib.get(b"az_css_path_fmt_debug")?);
            let az_css_path_selector_delete: extern "C" fn(_:  &mut AzCssPathSelector) = transmute(lib.get(b"az_css_path_selector_delete")?);
            let az_css_path_selector_deep_copy: extern "C" fn(_:  &AzCssPathSelector) -> AzCssPathSelector = transmute(lib.get(b"az_css_path_selector_deep_copy")?);
            let az_css_path_selector_fmt_debug: extern "C" fn(_:  &AzCssPathSelector) -> AzString = transmute(lib.get(b"az_css_path_selector_fmt_debug")?);
            let az_node_type_path_delete: extern "C" fn(_:  &mut AzNodeTypePath) = transmute(lib.get(b"az_node_type_path_delete")?);
            let az_node_type_path_deep_copy: extern "C" fn(_:  &AzNodeTypePath) -> AzNodeTypePath = transmute(lib.get(b"az_node_type_path_deep_copy")?);
            let az_node_type_path_fmt_debug: extern "C" fn(_:  &AzNodeTypePath) -> AzString = transmute(lib.get(b"az_node_type_path_fmt_debug")?);
            let az_css_path_pseudo_selector_delete: extern "C" fn(_:  &mut AzCssPathPseudoSelector) = transmute(lib.get(b"az_css_path_pseudo_selector_delete")?);
            let az_css_path_pseudo_selector_deep_copy: extern "C" fn(_:  &AzCssPathPseudoSelector) -> AzCssPathPseudoSelector = transmute(lib.get(b"az_css_path_pseudo_selector_deep_copy")?);
            let az_css_path_pseudo_selector_fmt_debug: extern "C" fn(_:  &AzCssPathPseudoSelector) -> AzString = transmute(lib.get(b"az_css_path_pseudo_selector_fmt_debug")?);
            let az_css_nth_child_selector_delete: extern "C" fn(_:  &mut AzCssNthChildSelector) = transmute(lib.get(b"az_css_nth_child_selector_delete")?);
            let az_css_nth_child_selector_deep_copy: extern "C" fn(_:  &AzCssNthChildSelector) -> AzCssNthChildSelector = transmute(lib.get(b"az_css_nth_child_selector_deep_copy")?);
            let az_css_nth_child_selector_fmt_debug: extern "C" fn(_:  &AzCssNthChildSelector) -> AzString = transmute(lib.get(b"az_css_nth_child_selector_fmt_debug")?);
            let az_css_nth_child_pattern_delete: extern "C" fn(_:  &mut AzCssNthChildPattern) = transmute(lib.get(b"az_css_nth_child_pattern_delete")?);
            let az_css_nth_child_pattern_deep_copy: extern "C" fn(_:  &AzCssNthChildPattern) -> AzCssNthChildPattern = transmute(lib.get(b"az_css_nth_child_pattern_deep_copy")?);
            let az_css_nth_child_pattern_fmt_debug: extern "C" fn(_:  &AzCssNthChildPattern) -> AzString = transmute(lib.get(b"az_css_nth_child_pattern_fmt_debug")?);
            let az_stylesheet_delete: extern "C" fn(_:  &mut AzStylesheet) = transmute(lib.get(b"az_stylesheet_delete")?);
            let az_stylesheet_deep_copy: extern "C" fn(_:  &AzStylesheet) -> AzStylesheet = transmute(lib.get(b"az_stylesheet_deep_copy")?);
            let az_stylesheet_fmt_debug: extern "C" fn(_:  &AzStylesheet) -> AzString = transmute(lib.get(b"az_stylesheet_fmt_debug")?);
            let az_css_native: extern "C" fn() -> AzCss = transmute(lib.get(b"az_css_native")?);
            let az_css_empty: extern "C" fn() -> AzCss = transmute(lib.get(b"az_css_empty")?);
            let az_css_from_string: extern "C" fn(_:  AzString) -> AzCss = transmute(lib.get(b"az_css_from_string")?);
            let az_css_override_native: extern "C" fn(_:  AzString) -> AzCss = transmute(lib.get(b"az_css_override_native")?);
            let az_css_delete: extern "C" fn(_:  &mut AzCss) = transmute(lib.get(b"az_css_delete")?);
            let az_css_deep_copy: extern "C" fn(_:  &AzCss) -> AzCss = transmute(lib.get(b"az_css_deep_copy")?);
            let az_css_fmt_debug: extern "C" fn(_:  &AzCss) -> AzString = transmute(lib.get(b"az_css_fmt_debug")?);
            let az_color_u_delete: extern "C" fn(_:  &mut AzColorU) = transmute(lib.get(b"az_color_u_delete")?);
            let az_color_u_deep_copy: extern "C" fn(_:  &AzColorU) -> AzColorU = transmute(lib.get(b"az_color_u_deep_copy")?);
            let az_color_u_fmt_debug: extern "C" fn(_:  &AzColorU) -> AzString = transmute(lib.get(b"az_color_u_fmt_debug")?);
            let az_size_metric_delete: extern "C" fn(_:  &mut AzSizeMetric) = transmute(lib.get(b"az_size_metric_delete")?);
            let az_size_metric_deep_copy: extern "C" fn(_:  &AzSizeMetric) -> AzSizeMetric = transmute(lib.get(b"az_size_metric_deep_copy")?);
            let az_size_metric_fmt_debug: extern "C" fn(_:  &AzSizeMetric) -> AzString = transmute(lib.get(b"az_size_metric_fmt_debug")?);
            let az_float_value_delete: extern "C" fn(_:  &mut AzFloatValue) = transmute(lib.get(b"az_float_value_delete")?);
            let az_float_value_deep_copy: extern "C" fn(_:  &AzFloatValue) -> AzFloatValue = transmute(lib.get(b"az_float_value_deep_copy")?);
            let az_float_value_fmt_debug: extern "C" fn(_:  &AzFloatValue) -> AzString = transmute(lib.get(b"az_float_value_fmt_debug")?);
            let az_pixel_value_delete: extern "C" fn(_:  &mut AzPixelValue) = transmute(lib.get(b"az_pixel_value_delete")?);
            let az_pixel_value_deep_copy: extern "C" fn(_:  &AzPixelValue) -> AzPixelValue = transmute(lib.get(b"az_pixel_value_deep_copy")?);
            let az_pixel_value_fmt_debug: extern "C" fn(_:  &AzPixelValue) -> AzString = transmute(lib.get(b"az_pixel_value_fmt_debug")?);
            let az_pixel_value_no_percent_delete: extern "C" fn(_:  &mut AzPixelValueNoPercent) = transmute(lib.get(b"az_pixel_value_no_percent_delete")?);
            let az_pixel_value_no_percent_deep_copy: extern "C" fn(_:  &AzPixelValueNoPercent) -> AzPixelValueNoPercent = transmute(lib.get(b"az_pixel_value_no_percent_deep_copy")?);
            let az_pixel_value_no_percent_fmt_debug: extern "C" fn(_:  &AzPixelValueNoPercent) -> AzString = transmute(lib.get(b"az_pixel_value_no_percent_fmt_debug")?);
            let az_box_shadow_clip_mode_delete: extern "C" fn(_:  &mut AzBoxShadowClipMode) = transmute(lib.get(b"az_box_shadow_clip_mode_delete")?);
            let az_box_shadow_clip_mode_deep_copy: extern "C" fn(_:  &AzBoxShadowClipMode) -> AzBoxShadowClipMode = transmute(lib.get(b"az_box_shadow_clip_mode_deep_copy")?);
            let az_box_shadow_clip_mode_fmt_debug: extern "C" fn(_:  &AzBoxShadowClipMode) -> AzString = transmute(lib.get(b"az_box_shadow_clip_mode_fmt_debug")?);
            let az_box_shadow_pre_display_item_delete: extern "C" fn(_:  &mut AzBoxShadowPreDisplayItem) = transmute(lib.get(b"az_box_shadow_pre_display_item_delete")?);
            let az_box_shadow_pre_display_item_deep_copy: extern "C" fn(_:  &AzBoxShadowPreDisplayItem) -> AzBoxShadowPreDisplayItem = transmute(lib.get(b"az_box_shadow_pre_display_item_deep_copy")?);
            let az_box_shadow_pre_display_item_fmt_debug: extern "C" fn(_:  &AzBoxShadowPreDisplayItem) -> AzString = transmute(lib.get(b"az_box_shadow_pre_display_item_fmt_debug")?);
            let az_layout_align_content_delete: extern "C" fn(_:  &mut AzLayoutAlignContent) = transmute(lib.get(b"az_layout_align_content_delete")?);
            let az_layout_align_content_deep_copy: extern "C" fn(_:  &AzLayoutAlignContent) -> AzLayoutAlignContent = transmute(lib.get(b"az_layout_align_content_deep_copy")?);
            let az_layout_align_content_fmt_debug: extern "C" fn(_:  &AzLayoutAlignContent) -> AzString = transmute(lib.get(b"az_layout_align_content_fmt_debug")?);
            let az_layout_align_items_delete: extern "C" fn(_:  &mut AzLayoutAlignItems) = transmute(lib.get(b"az_layout_align_items_delete")?);
            let az_layout_align_items_deep_copy: extern "C" fn(_:  &AzLayoutAlignItems) -> AzLayoutAlignItems = transmute(lib.get(b"az_layout_align_items_deep_copy")?);
            let az_layout_align_items_fmt_debug: extern "C" fn(_:  &AzLayoutAlignItems) -> AzString = transmute(lib.get(b"az_layout_align_items_fmt_debug")?);
            let az_layout_bottom_delete: extern "C" fn(_:  &mut AzLayoutBottom) = transmute(lib.get(b"az_layout_bottom_delete")?);
            let az_layout_bottom_deep_copy: extern "C" fn(_:  &AzLayoutBottom) -> AzLayoutBottom = transmute(lib.get(b"az_layout_bottom_deep_copy")?);
            let az_layout_bottom_fmt_debug: extern "C" fn(_:  &AzLayoutBottom) -> AzString = transmute(lib.get(b"az_layout_bottom_fmt_debug")?);
            let az_layout_box_sizing_delete: extern "C" fn(_:  &mut AzLayoutBoxSizing) = transmute(lib.get(b"az_layout_box_sizing_delete")?);
            let az_layout_box_sizing_deep_copy: extern "C" fn(_:  &AzLayoutBoxSizing) -> AzLayoutBoxSizing = transmute(lib.get(b"az_layout_box_sizing_deep_copy")?);
            let az_layout_box_sizing_fmt_debug: extern "C" fn(_:  &AzLayoutBoxSizing) -> AzString = transmute(lib.get(b"az_layout_box_sizing_fmt_debug")?);
            let az_layout_direction_delete: extern "C" fn(_:  &mut AzLayoutDirection) = transmute(lib.get(b"az_layout_direction_delete")?);
            let az_layout_direction_deep_copy: extern "C" fn(_:  &AzLayoutDirection) -> AzLayoutDirection = transmute(lib.get(b"az_layout_direction_deep_copy")?);
            let az_layout_direction_fmt_debug: extern "C" fn(_:  &AzLayoutDirection) -> AzString = transmute(lib.get(b"az_layout_direction_fmt_debug")?);
            let az_layout_display_delete: extern "C" fn(_:  &mut AzLayoutDisplay) = transmute(lib.get(b"az_layout_display_delete")?);
            let az_layout_display_deep_copy: extern "C" fn(_:  &AzLayoutDisplay) -> AzLayoutDisplay = transmute(lib.get(b"az_layout_display_deep_copy")?);
            let az_layout_display_fmt_debug: extern "C" fn(_:  &AzLayoutDisplay) -> AzString = transmute(lib.get(b"az_layout_display_fmt_debug")?);
            let az_layout_flex_grow_delete: extern "C" fn(_:  &mut AzLayoutFlexGrow) = transmute(lib.get(b"az_layout_flex_grow_delete")?);
            let az_layout_flex_grow_deep_copy: extern "C" fn(_:  &AzLayoutFlexGrow) -> AzLayoutFlexGrow = transmute(lib.get(b"az_layout_flex_grow_deep_copy")?);
            let az_layout_flex_grow_fmt_debug: extern "C" fn(_:  &AzLayoutFlexGrow) -> AzString = transmute(lib.get(b"az_layout_flex_grow_fmt_debug")?);
            let az_layout_flex_shrink_delete: extern "C" fn(_:  &mut AzLayoutFlexShrink) = transmute(lib.get(b"az_layout_flex_shrink_delete")?);
            let az_layout_flex_shrink_deep_copy: extern "C" fn(_:  &AzLayoutFlexShrink) -> AzLayoutFlexShrink = transmute(lib.get(b"az_layout_flex_shrink_deep_copy")?);
            let az_layout_flex_shrink_fmt_debug: extern "C" fn(_:  &AzLayoutFlexShrink) -> AzString = transmute(lib.get(b"az_layout_flex_shrink_fmt_debug")?);
            let az_layout_float_delete: extern "C" fn(_:  &mut AzLayoutFloat) = transmute(lib.get(b"az_layout_float_delete")?);
            let az_layout_float_deep_copy: extern "C" fn(_:  &AzLayoutFloat) -> AzLayoutFloat = transmute(lib.get(b"az_layout_float_deep_copy")?);
            let az_layout_float_fmt_debug: extern "C" fn(_:  &AzLayoutFloat) -> AzString = transmute(lib.get(b"az_layout_float_fmt_debug")?);
            let az_layout_height_delete: extern "C" fn(_:  &mut AzLayoutHeight) = transmute(lib.get(b"az_layout_height_delete")?);
            let az_layout_height_deep_copy: extern "C" fn(_:  &AzLayoutHeight) -> AzLayoutHeight = transmute(lib.get(b"az_layout_height_deep_copy")?);
            let az_layout_height_fmt_debug: extern "C" fn(_:  &AzLayoutHeight) -> AzString = transmute(lib.get(b"az_layout_height_fmt_debug")?);
            let az_layout_justify_content_delete: extern "C" fn(_:  &mut AzLayoutJustifyContent) = transmute(lib.get(b"az_layout_justify_content_delete")?);
            let az_layout_justify_content_deep_copy: extern "C" fn(_:  &AzLayoutJustifyContent) -> AzLayoutJustifyContent = transmute(lib.get(b"az_layout_justify_content_deep_copy")?);
            let az_layout_justify_content_fmt_debug: extern "C" fn(_:  &AzLayoutJustifyContent) -> AzString = transmute(lib.get(b"az_layout_justify_content_fmt_debug")?);
            let az_layout_left_delete: extern "C" fn(_:  &mut AzLayoutLeft) = transmute(lib.get(b"az_layout_left_delete")?);
            let az_layout_left_deep_copy: extern "C" fn(_:  &AzLayoutLeft) -> AzLayoutLeft = transmute(lib.get(b"az_layout_left_deep_copy")?);
            let az_layout_left_fmt_debug: extern "C" fn(_:  &AzLayoutLeft) -> AzString = transmute(lib.get(b"az_layout_left_fmt_debug")?);
            let az_layout_margin_bottom_delete: extern "C" fn(_:  &mut AzLayoutMarginBottom) = transmute(lib.get(b"az_layout_margin_bottom_delete")?);
            let az_layout_margin_bottom_deep_copy: extern "C" fn(_:  &AzLayoutMarginBottom) -> AzLayoutMarginBottom = transmute(lib.get(b"az_layout_margin_bottom_deep_copy")?);
            let az_layout_margin_bottom_fmt_debug: extern "C" fn(_:  &AzLayoutMarginBottom) -> AzString = transmute(lib.get(b"az_layout_margin_bottom_fmt_debug")?);
            let az_layout_margin_left_delete: extern "C" fn(_:  &mut AzLayoutMarginLeft) = transmute(lib.get(b"az_layout_margin_left_delete")?);
            let az_layout_margin_left_deep_copy: extern "C" fn(_:  &AzLayoutMarginLeft) -> AzLayoutMarginLeft = transmute(lib.get(b"az_layout_margin_left_deep_copy")?);
            let az_layout_margin_left_fmt_debug: extern "C" fn(_:  &AzLayoutMarginLeft) -> AzString = transmute(lib.get(b"az_layout_margin_left_fmt_debug")?);
            let az_layout_margin_right_delete: extern "C" fn(_:  &mut AzLayoutMarginRight) = transmute(lib.get(b"az_layout_margin_right_delete")?);
            let az_layout_margin_right_deep_copy: extern "C" fn(_:  &AzLayoutMarginRight) -> AzLayoutMarginRight = transmute(lib.get(b"az_layout_margin_right_deep_copy")?);
            let az_layout_margin_right_fmt_debug: extern "C" fn(_:  &AzLayoutMarginRight) -> AzString = transmute(lib.get(b"az_layout_margin_right_fmt_debug")?);
            let az_layout_margin_top_delete: extern "C" fn(_:  &mut AzLayoutMarginTop) = transmute(lib.get(b"az_layout_margin_top_delete")?);
            let az_layout_margin_top_deep_copy: extern "C" fn(_:  &AzLayoutMarginTop) -> AzLayoutMarginTop = transmute(lib.get(b"az_layout_margin_top_deep_copy")?);
            let az_layout_margin_top_fmt_debug: extern "C" fn(_:  &AzLayoutMarginTop) -> AzString = transmute(lib.get(b"az_layout_margin_top_fmt_debug")?);
            let az_layout_max_height_delete: extern "C" fn(_:  &mut AzLayoutMaxHeight) = transmute(lib.get(b"az_layout_max_height_delete")?);
            let az_layout_max_height_deep_copy: extern "C" fn(_:  &AzLayoutMaxHeight) -> AzLayoutMaxHeight = transmute(lib.get(b"az_layout_max_height_deep_copy")?);
            let az_layout_max_height_fmt_debug: extern "C" fn(_:  &AzLayoutMaxHeight) -> AzString = transmute(lib.get(b"az_layout_max_height_fmt_debug")?);
            let az_layout_max_width_delete: extern "C" fn(_:  &mut AzLayoutMaxWidth) = transmute(lib.get(b"az_layout_max_width_delete")?);
            let az_layout_max_width_deep_copy: extern "C" fn(_:  &AzLayoutMaxWidth) -> AzLayoutMaxWidth = transmute(lib.get(b"az_layout_max_width_deep_copy")?);
            let az_layout_max_width_fmt_debug: extern "C" fn(_:  &AzLayoutMaxWidth) -> AzString = transmute(lib.get(b"az_layout_max_width_fmt_debug")?);
            let az_layout_min_height_delete: extern "C" fn(_:  &mut AzLayoutMinHeight) = transmute(lib.get(b"az_layout_min_height_delete")?);
            let az_layout_min_height_deep_copy: extern "C" fn(_:  &AzLayoutMinHeight) -> AzLayoutMinHeight = transmute(lib.get(b"az_layout_min_height_deep_copy")?);
            let az_layout_min_height_fmt_debug: extern "C" fn(_:  &AzLayoutMinHeight) -> AzString = transmute(lib.get(b"az_layout_min_height_fmt_debug")?);
            let az_layout_min_width_delete: extern "C" fn(_:  &mut AzLayoutMinWidth) = transmute(lib.get(b"az_layout_min_width_delete")?);
            let az_layout_min_width_deep_copy: extern "C" fn(_:  &AzLayoutMinWidth) -> AzLayoutMinWidth = transmute(lib.get(b"az_layout_min_width_deep_copy")?);
            let az_layout_min_width_fmt_debug: extern "C" fn(_:  &AzLayoutMinWidth) -> AzString = transmute(lib.get(b"az_layout_min_width_fmt_debug")?);
            let az_layout_padding_bottom_delete: extern "C" fn(_:  &mut AzLayoutPaddingBottom) = transmute(lib.get(b"az_layout_padding_bottom_delete")?);
            let az_layout_padding_bottom_deep_copy: extern "C" fn(_:  &AzLayoutPaddingBottom) -> AzLayoutPaddingBottom = transmute(lib.get(b"az_layout_padding_bottom_deep_copy")?);
            let az_layout_padding_bottom_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingBottom) -> AzString = transmute(lib.get(b"az_layout_padding_bottom_fmt_debug")?);
            let az_layout_padding_left_delete: extern "C" fn(_:  &mut AzLayoutPaddingLeft) = transmute(lib.get(b"az_layout_padding_left_delete")?);
            let az_layout_padding_left_deep_copy: extern "C" fn(_:  &AzLayoutPaddingLeft) -> AzLayoutPaddingLeft = transmute(lib.get(b"az_layout_padding_left_deep_copy")?);
            let az_layout_padding_left_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingLeft) -> AzString = transmute(lib.get(b"az_layout_padding_left_fmt_debug")?);
            let az_layout_padding_right_delete: extern "C" fn(_:  &mut AzLayoutPaddingRight) = transmute(lib.get(b"az_layout_padding_right_delete")?);
            let az_layout_padding_right_deep_copy: extern "C" fn(_:  &AzLayoutPaddingRight) -> AzLayoutPaddingRight = transmute(lib.get(b"az_layout_padding_right_deep_copy")?);
            let az_layout_padding_right_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingRight) -> AzString = transmute(lib.get(b"az_layout_padding_right_fmt_debug")?);
            let az_layout_padding_top_delete: extern "C" fn(_:  &mut AzLayoutPaddingTop) = transmute(lib.get(b"az_layout_padding_top_delete")?);
            let az_layout_padding_top_deep_copy: extern "C" fn(_:  &AzLayoutPaddingTop) -> AzLayoutPaddingTop = transmute(lib.get(b"az_layout_padding_top_deep_copy")?);
            let az_layout_padding_top_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingTop) -> AzString = transmute(lib.get(b"az_layout_padding_top_fmt_debug")?);
            let az_layout_position_delete: extern "C" fn(_:  &mut AzLayoutPosition) = transmute(lib.get(b"az_layout_position_delete")?);
            let az_layout_position_deep_copy: extern "C" fn(_:  &AzLayoutPosition) -> AzLayoutPosition = transmute(lib.get(b"az_layout_position_deep_copy")?);
            let az_layout_position_fmt_debug: extern "C" fn(_:  &AzLayoutPosition) -> AzString = transmute(lib.get(b"az_layout_position_fmt_debug")?);
            let az_layout_right_delete: extern "C" fn(_:  &mut AzLayoutRight) = transmute(lib.get(b"az_layout_right_delete")?);
            let az_layout_right_deep_copy: extern "C" fn(_:  &AzLayoutRight) -> AzLayoutRight = transmute(lib.get(b"az_layout_right_deep_copy")?);
            let az_layout_right_fmt_debug: extern "C" fn(_:  &AzLayoutRight) -> AzString = transmute(lib.get(b"az_layout_right_fmt_debug")?);
            let az_layout_top_delete: extern "C" fn(_:  &mut AzLayoutTop) = transmute(lib.get(b"az_layout_top_delete")?);
            let az_layout_top_deep_copy: extern "C" fn(_:  &AzLayoutTop) -> AzLayoutTop = transmute(lib.get(b"az_layout_top_deep_copy")?);
            let az_layout_top_fmt_debug: extern "C" fn(_:  &AzLayoutTop) -> AzString = transmute(lib.get(b"az_layout_top_fmt_debug")?);
            let az_layout_width_delete: extern "C" fn(_:  &mut AzLayoutWidth) = transmute(lib.get(b"az_layout_width_delete")?);
            let az_layout_width_deep_copy: extern "C" fn(_:  &AzLayoutWidth) -> AzLayoutWidth = transmute(lib.get(b"az_layout_width_deep_copy")?);
            let az_layout_width_fmt_debug: extern "C" fn(_:  &AzLayoutWidth) -> AzString = transmute(lib.get(b"az_layout_width_fmt_debug")?);
            let az_layout_wrap_delete: extern "C" fn(_:  &mut AzLayoutWrap) = transmute(lib.get(b"az_layout_wrap_delete")?);
            let az_layout_wrap_deep_copy: extern "C" fn(_:  &AzLayoutWrap) -> AzLayoutWrap = transmute(lib.get(b"az_layout_wrap_deep_copy")?);
            let az_layout_wrap_fmt_debug: extern "C" fn(_:  &AzLayoutWrap) -> AzString = transmute(lib.get(b"az_layout_wrap_fmt_debug")?);
            let az_overflow_delete: extern "C" fn(_:  &mut AzOverflow) = transmute(lib.get(b"az_overflow_delete")?);
            let az_overflow_deep_copy: extern "C" fn(_:  &AzOverflow) -> AzOverflow = transmute(lib.get(b"az_overflow_deep_copy")?);
            let az_overflow_fmt_debug: extern "C" fn(_:  &AzOverflow) -> AzString = transmute(lib.get(b"az_overflow_fmt_debug")?);
            let az_percentage_value_delete: extern "C" fn(_:  &mut AzPercentageValue) = transmute(lib.get(b"az_percentage_value_delete")?);
            let az_percentage_value_deep_copy: extern "C" fn(_:  &AzPercentageValue) -> AzPercentageValue = transmute(lib.get(b"az_percentage_value_deep_copy")?);
            let az_percentage_value_fmt_debug: extern "C" fn(_:  &AzPercentageValue) -> AzString = transmute(lib.get(b"az_percentage_value_fmt_debug")?);
            let az_gradient_stop_pre_delete: extern "C" fn(_:  &mut AzGradientStopPre) = transmute(lib.get(b"az_gradient_stop_pre_delete")?);
            let az_gradient_stop_pre_deep_copy: extern "C" fn(_:  &AzGradientStopPre) -> AzGradientStopPre = transmute(lib.get(b"az_gradient_stop_pre_deep_copy")?);
            let az_gradient_stop_pre_fmt_debug: extern "C" fn(_:  &AzGradientStopPre) -> AzString = transmute(lib.get(b"az_gradient_stop_pre_fmt_debug")?);
            let az_gradient_stop_pre_partial_eq: extern "C" fn(_:  &AzGradientStopPre, _:  &AzGradientStopPre) -> bool = transmute(lib.get(b"az_gradient_stop_pre_partial_eq")?);
            let az_gradient_stop_pre_partial_cmp: extern "C" fn(_:  &AzGradientStopPre, _:  &AzGradientStopPre) -> u8 = transmute(lib.get(b"az_gradient_stop_pre_partial_cmp")?);
            let az_gradient_stop_pre_cmp: extern "C" fn(_:  &AzGradientStopPre, _:  &AzGradientStopPre) -> u8 = transmute(lib.get(b"az_gradient_stop_pre_cmp")?);
            let az_gradient_stop_pre_hash: extern "C" fn(_:  &AzGradientStopPre) -> u64 = transmute(lib.get(b"az_gradient_stop_pre_hash")?);
            let az_direction_corner_delete: extern "C" fn(_:  &mut AzDirectionCorner) = transmute(lib.get(b"az_direction_corner_delete")?);
            let az_direction_corner_deep_copy: extern "C" fn(_:  &AzDirectionCorner) -> AzDirectionCorner = transmute(lib.get(b"az_direction_corner_deep_copy")?);
            let az_direction_corner_fmt_debug: extern "C" fn(_:  &AzDirectionCorner) -> AzString = transmute(lib.get(b"az_direction_corner_fmt_debug")?);
            let az_direction_corners_delete: extern "C" fn(_:  &mut AzDirectionCorners) = transmute(lib.get(b"az_direction_corners_delete")?);
            let az_direction_corners_deep_copy: extern "C" fn(_:  &AzDirectionCorners) -> AzDirectionCorners = transmute(lib.get(b"az_direction_corners_deep_copy")?);
            let az_direction_corners_fmt_debug: extern "C" fn(_:  &AzDirectionCorners) -> AzString = transmute(lib.get(b"az_direction_corners_fmt_debug")?);
            let az_direction_delete: extern "C" fn(_:  &mut AzDirection) = transmute(lib.get(b"az_direction_delete")?);
            let az_direction_deep_copy: extern "C" fn(_:  &AzDirection) -> AzDirection = transmute(lib.get(b"az_direction_deep_copy")?);
            let az_direction_fmt_debug: extern "C" fn(_:  &AzDirection) -> AzString = transmute(lib.get(b"az_direction_fmt_debug")?);
            let az_extend_mode_delete: extern "C" fn(_:  &mut AzExtendMode) = transmute(lib.get(b"az_extend_mode_delete")?);
            let az_extend_mode_deep_copy: extern "C" fn(_:  &AzExtendMode) -> AzExtendMode = transmute(lib.get(b"az_extend_mode_deep_copy")?);
            let az_extend_mode_fmt_debug: extern "C" fn(_:  &AzExtendMode) -> AzString = transmute(lib.get(b"az_extend_mode_fmt_debug")?);
            let az_linear_gradient_delete: extern "C" fn(_:  &mut AzLinearGradient) = transmute(lib.get(b"az_linear_gradient_delete")?);
            let az_linear_gradient_deep_copy: extern "C" fn(_:  &AzLinearGradient) -> AzLinearGradient = transmute(lib.get(b"az_linear_gradient_deep_copy")?);
            let az_linear_gradient_fmt_debug: extern "C" fn(_:  &AzLinearGradient) -> AzString = transmute(lib.get(b"az_linear_gradient_fmt_debug")?);
            let az_shape_delete: extern "C" fn(_:  &mut AzShape) = transmute(lib.get(b"az_shape_delete")?);
            let az_shape_deep_copy: extern "C" fn(_:  &AzShape) -> AzShape = transmute(lib.get(b"az_shape_deep_copy")?);
            let az_shape_fmt_debug: extern "C" fn(_:  &AzShape) -> AzString = transmute(lib.get(b"az_shape_fmt_debug")?);
            let az_radial_gradient_delete: extern "C" fn(_:  &mut AzRadialGradient) = transmute(lib.get(b"az_radial_gradient_delete")?);
            let az_radial_gradient_deep_copy: extern "C" fn(_:  &AzRadialGradient) -> AzRadialGradient = transmute(lib.get(b"az_radial_gradient_deep_copy")?);
            let az_radial_gradient_fmt_debug: extern "C" fn(_:  &AzRadialGradient) -> AzString = transmute(lib.get(b"az_radial_gradient_fmt_debug")?);
            let az_css_image_id_delete: extern "C" fn(_:  &mut AzCssImageId) = transmute(lib.get(b"az_css_image_id_delete")?);
            let az_css_image_id_deep_copy: extern "C" fn(_:  &AzCssImageId) -> AzCssImageId = transmute(lib.get(b"az_css_image_id_deep_copy")?);
            let az_css_image_id_fmt_debug: extern "C" fn(_:  &AzCssImageId) -> AzString = transmute(lib.get(b"az_css_image_id_fmt_debug")?);
            let az_style_background_content_delete: extern "C" fn(_:  &mut AzStyleBackgroundContent) = transmute(lib.get(b"az_style_background_content_delete")?);
            let az_style_background_content_deep_copy: extern "C" fn(_:  &AzStyleBackgroundContent) -> AzStyleBackgroundContent = transmute(lib.get(b"az_style_background_content_deep_copy")?);
            let az_style_background_content_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundContent) -> AzString = transmute(lib.get(b"az_style_background_content_fmt_debug")?);
            let az_background_position_horizontal_delete: extern "C" fn(_:  &mut AzBackgroundPositionHorizontal) = transmute(lib.get(b"az_background_position_horizontal_delete")?);
            let az_background_position_horizontal_deep_copy: extern "C" fn(_:  &AzBackgroundPositionHorizontal) -> AzBackgroundPositionHorizontal = transmute(lib.get(b"az_background_position_horizontal_deep_copy")?);
            let az_background_position_horizontal_fmt_debug: extern "C" fn(_:  &AzBackgroundPositionHorizontal) -> AzString = transmute(lib.get(b"az_background_position_horizontal_fmt_debug")?);
            let az_background_position_vertical_delete: extern "C" fn(_:  &mut AzBackgroundPositionVertical) = transmute(lib.get(b"az_background_position_vertical_delete")?);
            let az_background_position_vertical_deep_copy: extern "C" fn(_:  &AzBackgroundPositionVertical) -> AzBackgroundPositionVertical = transmute(lib.get(b"az_background_position_vertical_deep_copy")?);
            let az_background_position_vertical_fmt_debug: extern "C" fn(_:  &AzBackgroundPositionVertical) -> AzString = transmute(lib.get(b"az_background_position_vertical_fmt_debug")?);
            let az_style_background_position_delete: extern "C" fn(_:  &mut AzStyleBackgroundPosition) = transmute(lib.get(b"az_style_background_position_delete")?);
            let az_style_background_position_deep_copy: extern "C" fn(_:  &AzStyleBackgroundPosition) -> AzStyleBackgroundPosition = transmute(lib.get(b"az_style_background_position_deep_copy")?);
            let az_style_background_position_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundPosition) -> AzString = transmute(lib.get(b"az_style_background_position_fmt_debug")?);
            let az_style_background_repeat_delete: extern "C" fn(_:  &mut AzStyleBackgroundRepeat) = transmute(lib.get(b"az_style_background_repeat_delete")?);
            let az_style_background_repeat_deep_copy: extern "C" fn(_:  &AzStyleBackgroundRepeat) -> AzStyleBackgroundRepeat = transmute(lib.get(b"az_style_background_repeat_deep_copy")?);
            let az_style_background_repeat_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundRepeat) -> AzString = transmute(lib.get(b"az_style_background_repeat_fmt_debug")?);
            let az_style_background_size_delete: extern "C" fn(_:  &mut AzStyleBackgroundSize) = transmute(lib.get(b"az_style_background_size_delete")?);
            let az_style_background_size_deep_copy: extern "C" fn(_:  &AzStyleBackgroundSize) -> AzStyleBackgroundSize = transmute(lib.get(b"az_style_background_size_deep_copy")?);
            let az_style_background_size_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundSize) -> AzString = transmute(lib.get(b"az_style_background_size_fmt_debug")?);
            let az_style_border_bottom_color_delete: extern "C" fn(_:  &mut AzStyleBorderBottomColor) = transmute(lib.get(b"az_style_border_bottom_color_delete")?);
            let az_style_border_bottom_color_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomColor) -> AzStyleBorderBottomColor = transmute(lib.get(b"az_style_border_bottom_color_deep_copy")?);
            let az_style_border_bottom_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomColor) -> AzString = transmute(lib.get(b"az_style_border_bottom_color_fmt_debug")?);
            let az_style_border_bottom_left_radius_delete: extern "C" fn(_:  &mut AzStyleBorderBottomLeftRadius) = transmute(lib.get(b"az_style_border_bottom_left_radius_delete")?);
            let az_style_border_bottom_left_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomLeftRadius) -> AzStyleBorderBottomLeftRadius = transmute(lib.get(b"az_style_border_bottom_left_radius_deep_copy")?);
            let az_style_border_bottom_left_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomLeftRadius) -> AzString = transmute(lib.get(b"az_style_border_bottom_left_radius_fmt_debug")?);
            let az_style_border_bottom_right_radius_delete: extern "C" fn(_:  &mut AzStyleBorderBottomRightRadius) = transmute(lib.get(b"az_style_border_bottom_right_radius_delete")?);
            let az_style_border_bottom_right_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomRightRadius) -> AzStyleBorderBottomRightRadius = transmute(lib.get(b"az_style_border_bottom_right_radius_deep_copy")?);
            let az_style_border_bottom_right_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomRightRadius) -> AzString = transmute(lib.get(b"az_style_border_bottom_right_radius_fmt_debug")?);
            let az_border_style_delete: extern "C" fn(_:  &mut AzBorderStyle) = transmute(lib.get(b"az_border_style_delete")?);
            let az_border_style_deep_copy: extern "C" fn(_:  &AzBorderStyle) -> AzBorderStyle = transmute(lib.get(b"az_border_style_deep_copy")?);
            let az_border_style_fmt_debug: extern "C" fn(_:  &AzBorderStyle) -> AzString = transmute(lib.get(b"az_border_style_fmt_debug")?);
            let az_style_border_bottom_style_delete: extern "C" fn(_:  &mut AzStyleBorderBottomStyle) = transmute(lib.get(b"az_style_border_bottom_style_delete")?);
            let az_style_border_bottom_style_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomStyle) -> AzStyleBorderBottomStyle = transmute(lib.get(b"az_style_border_bottom_style_deep_copy")?);
            let az_style_border_bottom_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomStyle) -> AzString = transmute(lib.get(b"az_style_border_bottom_style_fmt_debug")?);
            let az_style_border_bottom_width_delete: extern "C" fn(_:  &mut AzStyleBorderBottomWidth) = transmute(lib.get(b"az_style_border_bottom_width_delete")?);
            let az_style_border_bottom_width_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomWidth) -> AzStyleBorderBottomWidth = transmute(lib.get(b"az_style_border_bottom_width_deep_copy")?);
            let az_style_border_bottom_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomWidth) -> AzString = transmute(lib.get(b"az_style_border_bottom_width_fmt_debug")?);
            let az_style_border_left_color_delete: extern "C" fn(_:  &mut AzStyleBorderLeftColor) = transmute(lib.get(b"az_style_border_left_color_delete")?);
            let az_style_border_left_color_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftColor) -> AzStyleBorderLeftColor = transmute(lib.get(b"az_style_border_left_color_deep_copy")?);
            let az_style_border_left_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftColor) -> AzString = transmute(lib.get(b"az_style_border_left_color_fmt_debug")?);
            let az_style_border_left_style_delete: extern "C" fn(_:  &mut AzStyleBorderLeftStyle) = transmute(lib.get(b"az_style_border_left_style_delete")?);
            let az_style_border_left_style_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftStyle) -> AzStyleBorderLeftStyle = transmute(lib.get(b"az_style_border_left_style_deep_copy")?);
            let az_style_border_left_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftStyle) -> AzString = transmute(lib.get(b"az_style_border_left_style_fmt_debug")?);
            let az_style_border_left_width_delete: extern "C" fn(_:  &mut AzStyleBorderLeftWidth) = transmute(lib.get(b"az_style_border_left_width_delete")?);
            let az_style_border_left_width_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftWidth) -> AzStyleBorderLeftWidth = transmute(lib.get(b"az_style_border_left_width_deep_copy")?);
            let az_style_border_left_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftWidth) -> AzString = transmute(lib.get(b"az_style_border_left_width_fmt_debug")?);
            let az_style_border_right_color_delete: extern "C" fn(_:  &mut AzStyleBorderRightColor) = transmute(lib.get(b"az_style_border_right_color_delete")?);
            let az_style_border_right_color_deep_copy: extern "C" fn(_:  &AzStyleBorderRightColor) -> AzStyleBorderRightColor = transmute(lib.get(b"az_style_border_right_color_deep_copy")?);
            let az_style_border_right_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightColor) -> AzString = transmute(lib.get(b"az_style_border_right_color_fmt_debug")?);
            let az_style_border_right_style_delete: extern "C" fn(_:  &mut AzStyleBorderRightStyle) = transmute(lib.get(b"az_style_border_right_style_delete")?);
            let az_style_border_right_style_deep_copy: extern "C" fn(_:  &AzStyleBorderRightStyle) -> AzStyleBorderRightStyle = transmute(lib.get(b"az_style_border_right_style_deep_copy")?);
            let az_style_border_right_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightStyle) -> AzString = transmute(lib.get(b"az_style_border_right_style_fmt_debug")?);
            let az_style_border_right_width_delete: extern "C" fn(_:  &mut AzStyleBorderRightWidth) = transmute(lib.get(b"az_style_border_right_width_delete")?);
            let az_style_border_right_width_deep_copy: extern "C" fn(_:  &AzStyleBorderRightWidth) -> AzStyleBorderRightWidth = transmute(lib.get(b"az_style_border_right_width_deep_copy")?);
            let az_style_border_right_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightWidth) -> AzString = transmute(lib.get(b"az_style_border_right_width_fmt_debug")?);
            let az_style_border_top_color_delete: extern "C" fn(_:  &mut AzStyleBorderTopColor) = transmute(lib.get(b"az_style_border_top_color_delete")?);
            let az_style_border_top_color_deep_copy: extern "C" fn(_:  &AzStyleBorderTopColor) -> AzStyleBorderTopColor = transmute(lib.get(b"az_style_border_top_color_deep_copy")?);
            let az_style_border_top_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopColor) -> AzString = transmute(lib.get(b"az_style_border_top_color_fmt_debug")?);
            let az_style_border_top_left_radius_delete: extern "C" fn(_:  &mut AzStyleBorderTopLeftRadius) = transmute(lib.get(b"az_style_border_top_left_radius_delete")?);
            let az_style_border_top_left_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderTopLeftRadius) -> AzStyleBorderTopLeftRadius = transmute(lib.get(b"az_style_border_top_left_radius_deep_copy")?);
            let az_style_border_top_left_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopLeftRadius) -> AzString = transmute(lib.get(b"az_style_border_top_left_radius_fmt_debug")?);
            let az_style_border_top_right_radius_delete: extern "C" fn(_:  &mut AzStyleBorderTopRightRadius) = transmute(lib.get(b"az_style_border_top_right_radius_delete")?);
            let az_style_border_top_right_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderTopRightRadius) -> AzStyleBorderTopRightRadius = transmute(lib.get(b"az_style_border_top_right_radius_deep_copy")?);
            let az_style_border_top_right_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopRightRadius) -> AzString = transmute(lib.get(b"az_style_border_top_right_radius_fmt_debug")?);
            let az_style_border_top_style_delete: extern "C" fn(_:  &mut AzStyleBorderTopStyle) = transmute(lib.get(b"az_style_border_top_style_delete")?);
            let az_style_border_top_style_deep_copy: extern "C" fn(_:  &AzStyleBorderTopStyle) -> AzStyleBorderTopStyle = transmute(lib.get(b"az_style_border_top_style_deep_copy")?);
            let az_style_border_top_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopStyle) -> AzString = transmute(lib.get(b"az_style_border_top_style_fmt_debug")?);
            let az_style_border_top_width_delete: extern "C" fn(_:  &mut AzStyleBorderTopWidth) = transmute(lib.get(b"az_style_border_top_width_delete")?);
            let az_style_border_top_width_deep_copy: extern "C" fn(_:  &AzStyleBorderTopWidth) -> AzStyleBorderTopWidth = transmute(lib.get(b"az_style_border_top_width_deep_copy")?);
            let az_style_border_top_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopWidth) -> AzString = transmute(lib.get(b"az_style_border_top_width_fmt_debug")?);
            let az_style_cursor_delete: extern "C" fn(_:  &mut AzStyleCursor) = transmute(lib.get(b"az_style_cursor_delete")?);
            let az_style_cursor_deep_copy: extern "C" fn(_:  &AzStyleCursor) -> AzStyleCursor = transmute(lib.get(b"az_style_cursor_deep_copy")?);
            let az_style_cursor_fmt_debug: extern "C" fn(_:  &AzStyleCursor) -> AzString = transmute(lib.get(b"az_style_cursor_fmt_debug")?);
            let az_style_font_family_delete: extern "C" fn(_:  &mut AzStyleFontFamily) = transmute(lib.get(b"az_style_font_family_delete")?);
            let az_style_font_family_deep_copy: extern "C" fn(_:  &AzStyleFontFamily) -> AzStyleFontFamily = transmute(lib.get(b"az_style_font_family_deep_copy")?);
            let az_style_font_family_fmt_debug: extern "C" fn(_:  &AzStyleFontFamily) -> AzString = transmute(lib.get(b"az_style_font_family_fmt_debug")?);
            let az_style_font_size_delete: extern "C" fn(_:  &mut AzStyleFontSize) = transmute(lib.get(b"az_style_font_size_delete")?);
            let az_style_font_size_deep_copy: extern "C" fn(_:  &AzStyleFontSize) -> AzStyleFontSize = transmute(lib.get(b"az_style_font_size_deep_copy")?);
            let az_style_font_size_fmt_debug: extern "C" fn(_:  &AzStyleFontSize) -> AzString = transmute(lib.get(b"az_style_font_size_fmt_debug")?);
            let az_style_letter_spacing_delete: extern "C" fn(_:  &mut AzStyleLetterSpacing) = transmute(lib.get(b"az_style_letter_spacing_delete")?);
            let az_style_letter_spacing_deep_copy: extern "C" fn(_:  &AzStyleLetterSpacing) -> AzStyleLetterSpacing = transmute(lib.get(b"az_style_letter_spacing_deep_copy")?);
            let az_style_letter_spacing_fmt_debug: extern "C" fn(_:  &AzStyleLetterSpacing) -> AzString = transmute(lib.get(b"az_style_letter_spacing_fmt_debug")?);
            let az_style_line_height_delete: extern "C" fn(_:  &mut AzStyleLineHeight) = transmute(lib.get(b"az_style_line_height_delete")?);
            let az_style_line_height_deep_copy: extern "C" fn(_:  &AzStyleLineHeight) -> AzStyleLineHeight = transmute(lib.get(b"az_style_line_height_deep_copy")?);
            let az_style_line_height_fmt_debug: extern "C" fn(_:  &AzStyleLineHeight) -> AzString = transmute(lib.get(b"az_style_line_height_fmt_debug")?);
            let az_style_tab_width_delete: extern "C" fn(_:  &mut AzStyleTabWidth) = transmute(lib.get(b"az_style_tab_width_delete")?);
            let az_style_tab_width_deep_copy: extern "C" fn(_:  &AzStyleTabWidth) -> AzStyleTabWidth = transmute(lib.get(b"az_style_tab_width_deep_copy")?);
            let az_style_tab_width_fmt_debug: extern "C" fn(_:  &AzStyleTabWidth) -> AzString = transmute(lib.get(b"az_style_tab_width_fmt_debug")?);
            let az_style_text_alignment_horz_delete: extern "C" fn(_:  &mut AzStyleTextAlignmentHorz) = transmute(lib.get(b"az_style_text_alignment_horz_delete")?);
            let az_style_text_alignment_horz_deep_copy: extern "C" fn(_:  &AzStyleTextAlignmentHorz) -> AzStyleTextAlignmentHorz = transmute(lib.get(b"az_style_text_alignment_horz_deep_copy")?);
            let az_style_text_alignment_horz_fmt_debug: extern "C" fn(_:  &AzStyleTextAlignmentHorz) -> AzString = transmute(lib.get(b"az_style_text_alignment_horz_fmt_debug")?);
            let az_style_text_color_delete: extern "C" fn(_:  &mut AzStyleTextColor) = transmute(lib.get(b"az_style_text_color_delete")?);
            let az_style_text_color_deep_copy: extern "C" fn(_:  &AzStyleTextColor) -> AzStyleTextColor = transmute(lib.get(b"az_style_text_color_deep_copy")?);
            let az_style_text_color_fmt_debug: extern "C" fn(_:  &AzStyleTextColor) -> AzString = transmute(lib.get(b"az_style_text_color_fmt_debug")?);
            let az_style_word_spacing_delete: extern "C" fn(_:  &mut AzStyleWordSpacing) = transmute(lib.get(b"az_style_word_spacing_delete")?);
            let az_style_word_spacing_deep_copy: extern "C" fn(_:  &AzStyleWordSpacing) -> AzStyleWordSpacing = transmute(lib.get(b"az_style_word_spacing_deep_copy")?);
            let az_style_word_spacing_fmt_debug: extern "C" fn(_:  &AzStyleWordSpacing) -> AzString = transmute(lib.get(b"az_style_word_spacing_fmt_debug")?);
            let az_box_shadow_pre_display_item_value_delete: extern "C" fn(_:  &mut AzBoxShadowPreDisplayItemValue) = transmute(lib.get(b"az_box_shadow_pre_display_item_value_delete")?);
            let az_box_shadow_pre_display_item_value_deep_copy: extern "C" fn(_:  &AzBoxShadowPreDisplayItemValue) -> AzBoxShadowPreDisplayItemValue = transmute(lib.get(b"az_box_shadow_pre_display_item_value_deep_copy")?);
            let az_box_shadow_pre_display_item_value_fmt_debug: extern "C" fn(_:  &AzBoxShadowPreDisplayItemValue) -> AzString = transmute(lib.get(b"az_box_shadow_pre_display_item_value_fmt_debug")?);
            let az_layout_align_content_value_delete: extern "C" fn(_:  &mut AzLayoutAlignContentValue) = transmute(lib.get(b"az_layout_align_content_value_delete")?);
            let az_layout_align_content_value_deep_copy: extern "C" fn(_:  &AzLayoutAlignContentValue) -> AzLayoutAlignContentValue = transmute(lib.get(b"az_layout_align_content_value_deep_copy")?);
            let az_layout_align_content_value_fmt_debug: extern "C" fn(_:  &AzLayoutAlignContentValue) -> AzString = transmute(lib.get(b"az_layout_align_content_value_fmt_debug")?);
            let az_layout_align_items_value_delete: extern "C" fn(_:  &mut AzLayoutAlignItemsValue) = transmute(lib.get(b"az_layout_align_items_value_delete")?);
            let az_layout_align_items_value_deep_copy: extern "C" fn(_:  &AzLayoutAlignItemsValue) -> AzLayoutAlignItemsValue = transmute(lib.get(b"az_layout_align_items_value_deep_copy")?);
            let az_layout_align_items_value_fmt_debug: extern "C" fn(_:  &AzLayoutAlignItemsValue) -> AzString = transmute(lib.get(b"az_layout_align_items_value_fmt_debug")?);
            let az_layout_bottom_value_delete: extern "C" fn(_:  &mut AzLayoutBottomValue) = transmute(lib.get(b"az_layout_bottom_value_delete")?);
            let az_layout_bottom_value_deep_copy: extern "C" fn(_:  &AzLayoutBottomValue) -> AzLayoutBottomValue = transmute(lib.get(b"az_layout_bottom_value_deep_copy")?);
            let az_layout_bottom_value_fmt_debug: extern "C" fn(_:  &AzLayoutBottomValue) -> AzString = transmute(lib.get(b"az_layout_bottom_value_fmt_debug")?);
            let az_layout_box_sizing_value_delete: extern "C" fn(_:  &mut AzLayoutBoxSizingValue) = transmute(lib.get(b"az_layout_box_sizing_value_delete")?);
            let az_layout_box_sizing_value_deep_copy: extern "C" fn(_:  &AzLayoutBoxSizingValue) -> AzLayoutBoxSizingValue = transmute(lib.get(b"az_layout_box_sizing_value_deep_copy")?);
            let az_layout_box_sizing_value_fmt_debug: extern "C" fn(_:  &AzLayoutBoxSizingValue) -> AzString = transmute(lib.get(b"az_layout_box_sizing_value_fmt_debug")?);
            let az_layout_direction_value_delete: extern "C" fn(_:  &mut AzLayoutDirectionValue) = transmute(lib.get(b"az_layout_direction_value_delete")?);
            let az_layout_direction_value_deep_copy: extern "C" fn(_:  &AzLayoutDirectionValue) -> AzLayoutDirectionValue = transmute(lib.get(b"az_layout_direction_value_deep_copy")?);
            let az_layout_direction_value_fmt_debug: extern "C" fn(_:  &AzLayoutDirectionValue) -> AzString = transmute(lib.get(b"az_layout_direction_value_fmt_debug")?);
            let az_layout_display_value_delete: extern "C" fn(_:  &mut AzLayoutDisplayValue) = transmute(lib.get(b"az_layout_display_value_delete")?);
            let az_layout_display_value_deep_copy: extern "C" fn(_:  &AzLayoutDisplayValue) -> AzLayoutDisplayValue = transmute(lib.get(b"az_layout_display_value_deep_copy")?);
            let az_layout_display_value_fmt_debug: extern "C" fn(_:  &AzLayoutDisplayValue) -> AzString = transmute(lib.get(b"az_layout_display_value_fmt_debug")?);
            let az_layout_flex_grow_value_delete: extern "C" fn(_:  &mut AzLayoutFlexGrowValue) = transmute(lib.get(b"az_layout_flex_grow_value_delete")?);
            let az_layout_flex_grow_value_deep_copy: extern "C" fn(_:  &AzLayoutFlexGrowValue) -> AzLayoutFlexGrowValue = transmute(lib.get(b"az_layout_flex_grow_value_deep_copy")?);
            let az_layout_flex_grow_value_fmt_debug: extern "C" fn(_:  &AzLayoutFlexGrowValue) -> AzString = transmute(lib.get(b"az_layout_flex_grow_value_fmt_debug")?);
            let az_layout_flex_shrink_value_delete: extern "C" fn(_:  &mut AzLayoutFlexShrinkValue) = transmute(lib.get(b"az_layout_flex_shrink_value_delete")?);
            let az_layout_flex_shrink_value_deep_copy: extern "C" fn(_:  &AzLayoutFlexShrinkValue) -> AzLayoutFlexShrinkValue = transmute(lib.get(b"az_layout_flex_shrink_value_deep_copy")?);
            let az_layout_flex_shrink_value_fmt_debug: extern "C" fn(_:  &AzLayoutFlexShrinkValue) -> AzString = transmute(lib.get(b"az_layout_flex_shrink_value_fmt_debug")?);
            let az_layout_float_value_delete: extern "C" fn(_:  &mut AzLayoutFloatValue) = transmute(lib.get(b"az_layout_float_value_delete")?);
            let az_layout_float_value_deep_copy: extern "C" fn(_:  &AzLayoutFloatValue) -> AzLayoutFloatValue = transmute(lib.get(b"az_layout_float_value_deep_copy")?);
            let az_layout_float_value_fmt_debug: extern "C" fn(_:  &AzLayoutFloatValue) -> AzString = transmute(lib.get(b"az_layout_float_value_fmt_debug")?);
            let az_layout_height_value_delete: extern "C" fn(_:  &mut AzLayoutHeightValue) = transmute(lib.get(b"az_layout_height_value_delete")?);
            let az_layout_height_value_deep_copy: extern "C" fn(_:  &AzLayoutHeightValue) -> AzLayoutHeightValue = transmute(lib.get(b"az_layout_height_value_deep_copy")?);
            let az_layout_height_value_fmt_debug: extern "C" fn(_:  &AzLayoutHeightValue) -> AzString = transmute(lib.get(b"az_layout_height_value_fmt_debug")?);
            let az_layout_justify_content_value_delete: extern "C" fn(_:  &mut AzLayoutJustifyContentValue) = transmute(lib.get(b"az_layout_justify_content_value_delete")?);
            let az_layout_justify_content_value_deep_copy: extern "C" fn(_:  &AzLayoutJustifyContentValue) -> AzLayoutJustifyContentValue = transmute(lib.get(b"az_layout_justify_content_value_deep_copy")?);
            let az_layout_justify_content_value_fmt_debug: extern "C" fn(_:  &AzLayoutJustifyContentValue) -> AzString = transmute(lib.get(b"az_layout_justify_content_value_fmt_debug")?);
            let az_layout_left_value_delete: extern "C" fn(_:  &mut AzLayoutLeftValue) = transmute(lib.get(b"az_layout_left_value_delete")?);
            let az_layout_left_value_deep_copy: extern "C" fn(_:  &AzLayoutLeftValue) -> AzLayoutLeftValue = transmute(lib.get(b"az_layout_left_value_deep_copy")?);
            let az_layout_left_value_fmt_debug: extern "C" fn(_:  &AzLayoutLeftValue) -> AzString = transmute(lib.get(b"az_layout_left_value_fmt_debug")?);
            let az_layout_margin_bottom_value_delete: extern "C" fn(_:  &mut AzLayoutMarginBottomValue) = transmute(lib.get(b"az_layout_margin_bottom_value_delete")?);
            let az_layout_margin_bottom_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginBottomValue) -> AzLayoutMarginBottomValue = transmute(lib.get(b"az_layout_margin_bottom_value_deep_copy")?);
            let az_layout_margin_bottom_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginBottomValue) -> AzString = transmute(lib.get(b"az_layout_margin_bottom_value_fmt_debug")?);
            let az_layout_margin_left_value_delete: extern "C" fn(_:  &mut AzLayoutMarginLeftValue) = transmute(lib.get(b"az_layout_margin_left_value_delete")?);
            let az_layout_margin_left_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginLeftValue) -> AzLayoutMarginLeftValue = transmute(lib.get(b"az_layout_margin_left_value_deep_copy")?);
            let az_layout_margin_left_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginLeftValue) -> AzString = transmute(lib.get(b"az_layout_margin_left_value_fmt_debug")?);
            let az_layout_margin_right_value_delete: extern "C" fn(_:  &mut AzLayoutMarginRightValue) = transmute(lib.get(b"az_layout_margin_right_value_delete")?);
            let az_layout_margin_right_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginRightValue) -> AzLayoutMarginRightValue = transmute(lib.get(b"az_layout_margin_right_value_deep_copy")?);
            let az_layout_margin_right_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginRightValue) -> AzString = transmute(lib.get(b"az_layout_margin_right_value_fmt_debug")?);
            let az_layout_margin_top_value_delete: extern "C" fn(_:  &mut AzLayoutMarginTopValue) = transmute(lib.get(b"az_layout_margin_top_value_delete")?);
            let az_layout_margin_top_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginTopValue) -> AzLayoutMarginTopValue = transmute(lib.get(b"az_layout_margin_top_value_deep_copy")?);
            let az_layout_margin_top_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginTopValue) -> AzString = transmute(lib.get(b"az_layout_margin_top_value_fmt_debug")?);
            let az_layout_max_height_value_delete: extern "C" fn(_:  &mut AzLayoutMaxHeightValue) = transmute(lib.get(b"az_layout_max_height_value_delete")?);
            let az_layout_max_height_value_deep_copy: extern "C" fn(_:  &AzLayoutMaxHeightValue) -> AzLayoutMaxHeightValue = transmute(lib.get(b"az_layout_max_height_value_deep_copy")?);
            let az_layout_max_height_value_fmt_debug: extern "C" fn(_:  &AzLayoutMaxHeightValue) -> AzString = transmute(lib.get(b"az_layout_max_height_value_fmt_debug")?);
            let az_layout_max_width_value_delete: extern "C" fn(_:  &mut AzLayoutMaxWidthValue) = transmute(lib.get(b"az_layout_max_width_value_delete")?);
            let az_layout_max_width_value_deep_copy: extern "C" fn(_:  &AzLayoutMaxWidthValue) -> AzLayoutMaxWidthValue = transmute(lib.get(b"az_layout_max_width_value_deep_copy")?);
            let az_layout_max_width_value_fmt_debug: extern "C" fn(_:  &AzLayoutMaxWidthValue) -> AzString = transmute(lib.get(b"az_layout_max_width_value_fmt_debug")?);
            let az_layout_min_height_value_delete: extern "C" fn(_:  &mut AzLayoutMinHeightValue) = transmute(lib.get(b"az_layout_min_height_value_delete")?);
            let az_layout_min_height_value_deep_copy: extern "C" fn(_:  &AzLayoutMinHeightValue) -> AzLayoutMinHeightValue = transmute(lib.get(b"az_layout_min_height_value_deep_copy")?);
            let az_layout_min_height_value_fmt_debug: extern "C" fn(_:  &AzLayoutMinHeightValue) -> AzString = transmute(lib.get(b"az_layout_min_height_value_fmt_debug")?);
            let az_layout_min_width_value_delete: extern "C" fn(_:  &mut AzLayoutMinWidthValue) = transmute(lib.get(b"az_layout_min_width_value_delete")?);
            let az_layout_min_width_value_deep_copy: extern "C" fn(_:  &AzLayoutMinWidthValue) -> AzLayoutMinWidthValue = transmute(lib.get(b"az_layout_min_width_value_deep_copy")?);
            let az_layout_min_width_value_fmt_debug: extern "C" fn(_:  &AzLayoutMinWidthValue) -> AzString = transmute(lib.get(b"az_layout_min_width_value_fmt_debug")?);
            let az_layout_padding_bottom_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingBottomValue) = transmute(lib.get(b"az_layout_padding_bottom_value_delete")?);
            let az_layout_padding_bottom_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingBottomValue) -> AzLayoutPaddingBottomValue = transmute(lib.get(b"az_layout_padding_bottom_value_deep_copy")?);
            let az_layout_padding_bottom_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingBottomValue) -> AzString = transmute(lib.get(b"az_layout_padding_bottom_value_fmt_debug")?);
            let az_layout_padding_left_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingLeftValue) = transmute(lib.get(b"az_layout_padding_left_value_delete")?);
            let az_layout_padding_left_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingLeftValue) -> AzLayoutPaddingLeftValue = transmute(lib.get(b"az_layout_padding_left_value_deep_copy")?);
            let az_layout_padding_left_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingLeftValue) -> AzString = transmute(lib.get(b"az_layout_padding_left_value_fmt_debug")?);
            let az_layout_padding_right_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingRightValue) = transmute(lib.get(b"az_layout_padding_right_value_delete")?);
            let az_layout_padding_right_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingRightValue) -> AzLayoutPaddingRightValue = transmute(lib.get(b"az_layout_padding_right_value_deep_copy")?);
            let az_layout_padding_right_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingRightValue) -> AzString = transmute(lib.get(b"az_layout_padding_right_value_fmt_debug")?);
            let az_layout_padding_top_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingTopValue) = transmute(lib.get(b"az_layout_padding_top_value_delete")?);
            let az_layout_padding_top_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingTopValue) -> AzLayoutPaddingTopValue = transmute(lib.get(b"az_layout_padding_top_value_deep_copy")?);
            let az_layout_padding_top_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingTopValue) -> AzString = transmute(lib.get(b"az_layout_padding_top_value_fmt_debug")?);
            let az_layout_position_value_delete: extern "C" fn(_:  &mut AzLayoutPositionValue) = transmute(lib.get(b"az_layout_position_value_delete")?);
            let az_layout_position_value_deep_copy: extern "C" fn(_:  &AzLayoutPositionValue) -> AzLayoutPositionValue = transmute(lib.get(b"az_layout_position_value_deep_copy")?);
            let az_layout_position_value_fmt_debug: extern "C" fn(_:  &AzLayoutPositionValue) -> AzString = transmute(lib.get(b"az_layout_position_value_fmt_debug")?);
            let az_layout_right_value_delete: extern "C" fn(_:  &mut AzLayoutRightValue) = transmute(lib.get(b"az_layout_right_value_delete")?);
            let az_layout_right_value_deep_copy: extern "C" fn(_:  &AzLayoutRightValue) -> AzLayoutRightValue = transmute(lib.get(b"az_layout_right_value_deep_copy")?);
            let az_layout_right_value_fmt_debug: extern "C" fn(_:  &AzLayoutRightValue) -> AzString = transmute(lib.get(b"az_layout_right_value_fmt_debug")?);
            let az_layout_top_value_delete: extern "C" fn(_:  &mut AzLayoutTopValue) = transmute(lib.get(b"az_layout_top_value_delete")?);
            let az_layout_top_value_deep_copy: extern "C" fn(_:  &AzLayoutTopValue) -> AzLayoutTopValue = transmute(lib.get(b"az_layout_top_value_deep_copy")?);
            let az_layout_top_value_fmt_debug: extern "C" fn(_:  &AzLayoutTopValue) -> AzString = transmute(lib.get(b"az_layout_top_value_fmt_debug")?);
            let az_layout_width_value_delete: extern "C" fn(_:  &mut AzLayoutWidthValue) = transmute(lib.get(b"az_layout_width_value_delete")?);
            let az_layout_width_value_deep_copy: extern "C" fn(_:  &AzLayoutWidthValue) -> AzLayoutWidthValue = transmute(lib.get(b"az_layout_width_value_deep_copy")?);
            let az_layout_width_value_fmt_debug: extern "C" fn(_:  &AzLayoutWidthValue) -> AzString = transmute(lib.get(b"az_layout_width_value_fmt_debug")?);
            let az_layout_wrap_value_delete: extern "C" fn(_:  &mut AzLayoutWrapValue) = transmute(lib.get(b"az_layout_wrap_value_delete")?);
            let az_layout_wrap_value_deep_copy: extern "C" fn(_:  &AzLayoutWrapValue) -> AzLayoutWrapValue = transmute(lib.get(b"az_layout_wrap_value_deep_copy")?);
            let az_layout_wrap_value_fmt_debug: extern "C" fn(_:  &AzLayoutWrapValue) -> AzString = transmute(lib.get(b"az_layout_wrap_value_fmt_debug")?);
            let az_overflow_value_delete: extern "C" fn(_:  &mut AzOverflowValue) = transmute(lib.get(b"az_overflow_value_delete")?);
            let az_overflow_value_deep_copy: extern "C" fn(_:  &AzOverflowValue) -> AzOverflowValue = transmute(lib.get(b"az_overflow_value_deep_copy")?);
            let az_overflow_value_fmt_debug: extern "C" fn(_:  &AzOverflowValue) -> AzString = transmute(lib.get(b"az_overflow_value_fmt_debug")?);
            let az_style_background_content_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundContentValue) = transmute(lib.get(b"az_style_background_content_value_delete")?);
            let az_style_background_content_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue = transmute(lib.get(b"az_style_background_content_value_deep_copy")?);
            let az_style_background_content_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundContentValue) -> AzString = transmute(lib.get(b"az_style_background_content_value_fmt_debug")?);
            let az_style_background_position_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundPositionValue) = transmute(lib.get(b"az_style_background_position_value_delete")?);
            let az_style_background_position_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundPositionValue) -> AzStyleBackgroundPositionValue = transmute(lib.get(b"az_style_background_position_value_deep_copy")?);
            let az_style_background_position_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundPositionValue) -> AzString = transmute(lib.get(b"az_style_background_position_value_fmt_debug")?);
            let az_style_background_repeat_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundRepeatValue) = transmute(lib.get(b"az_style_background_repeat_value_delete")?);
            let az_style_background_repeat_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundRepeatValue) -> AzStyleBackgroundRepeatValue = transmute(lib.get(b"az_style_background_repeat_value_deep_copy")?);
            let az_style_background_repeat_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundRepeatValue) -> AzString = transmute(lib.get(b"az_style_background_repeat_value_fmt_debug")?);
            let az_style_background_size_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundSizeValue) = transmute(lib.get(b"az_style_background_size_value_delete")?);
            let az_style_background_size_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundSizeValue) -> AzStyleBackgroundSizeValue = transmute(lib.get(b"az_style_background_size_value_deep_copy")?);
            let az_style_background_size_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundSizeValue) -> AzString = transmute(lib.get(b"az_style_background_size_value_fmt_debug")?);
            let az_style_border_bottom_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomColorValue) = transmute(lib.get(b"az_style_border_bottom_color_value_delete")?);
            let az_style_border_bottom_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomColorValue) -> AzStyleBorderBottomColorValue = transmute(lib.get(b"az_style_border_bottom_color_value_deep_copy")?);
            let az_style_border_bottom_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomColorValue) -> AzString = transmute(lib.get(b"az_style_border_bottom_color_value_fmt_debug")?);
            let az_style_border_bottom_left_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomLeftRadiusValue) = transmute(lib.get(b"az_style_border_bottom_left_radius_value_delete")?);
            let az_style_border_bottom_left_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomLeftRadiusValue) -> AzStyleBorderBottomLeftRadiusValue = transmute(lib.get(b"az_style_border_bottom_left_radius_value_deep_copy")?);
            let az_style_border_bottom_left_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomLeftRadiusValue) -> AzString = transmute(lib.get(b"az_style_border_bottom_left_radius_value_fmt_debug")?);
            let az_style_border_bottom_right_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomRightRadiusValue) = transmute(lib.get(b"az_style_border_bottom_right_radius_value_delete")?);
            let az_style_border_bottom_right_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomRightRadiusValue) -> AzStyleBorderBottomRightRadiusValue = transmute(lib.get(b"az_style_border_bottom_right_radius_value_deep_copy")?);
            let az_style_border_bottom_right_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomRightRadiusValue) -> AzString = transmute(lib.get(b"az_style_border_bottom_right_radius_value_fmt_debug")?);
            let az_style_border_bottom_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomStyleValue) = transmute(lib.get(b"az_style_border_bottom_style_value_delete")?);
            let az_style_border_bottom_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomStyleValue) -> AzStyleBorderBottomStyleValue = transmute(lib.get(b"az_style_border_bottom_style_value_deep_copy")?);
            let az_style_border_bottom_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomStyleValue) -> AzString = transmute(lib.get(b"az_style_border_bottom_style_value_fmt_debug")?);
            let az_style_border_bottom_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomWidthValue) = transmute(lib.get(b"az_style_border_bottom_width_value_delete")?);
            let az_style_border_bottom_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomWidthValue) -> AzStyleBorderBottomWidthValue = transmute(lib.get(b"az_style_border_bottom_width_value_deep_copy")?);
            let az_style_border_bottom_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomWidthValue) -> AzString = transmute(lib.get(b"az_style_border_bottom_width_value_fmt_debug")?);
            let az_style_border_left_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderLeftColorValue) = transmute(lib.get(b"az_style_border_left_color_value_delete")?);
            let az_style_border_left_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftColorValue) -> AzStyleBorderLeftColorValue = transmute(lib.get(b"az_style_border_left_color_value_deep_copy")?);
            let az_style_border_left_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftColorValue) -> AzString = transmute(lib.get(b"az_style_border_left_color_value_fmt_debug")?);
            let az_style_border_left_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderLeftStyleValue) = transmute(lib.get(b"az_style_border_left_style_value_delete")?);
            let az_style_border_left_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftStyleValue) -> AzStyleBorderLeftStyleValue = transmute(lib.get(b"az_style_border_left_style_value_deep_copy")?);
            let az_style_border_left_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftStyleValue) -> AzString = transmute(lib.get(b"az_style_border_left_style_value_fmt_debug")?);
            let az_style_border_left_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderLeftWidthValue) = transmute(lib.get(b"az_style_border_left_width_value_delete")?);
            let az_style_border_left_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftWidthValue) -> AzStyleBorderLeftWidthValue = transmute(lib.get(b"az_style_border_left_width_value_deep_copy")?);
            let az_style_border_left_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftWidthValue) -> AzString = transmute(lib.get(b"az_style_border_left_width_value_fmt_debug")?);
            let az_style_border_right_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderRightColorValue) = transmute(lib.get(b"az_style_border_right_color_value_delete")?);
            let az_style_border_right_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderRightColorValue) -> AzStyleBorderRightColorValue = transmute(lib.get(b"az_style_border_right_color_value_deep_copy")?);
            let az_style_border_right_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightColorValue) -> AzString = transmute(lib.get(b"az_style_border_right_color_value_fmt_debug")?);
            let az_style_border_right_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderRightStyleValue) = transmute(lib.get(b"az_style_border_right_style_value_delete")?);
            let az_style_border_right_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderRightStyleValue) -> AzStyleBorderRightStyleValue = transmute(lib.get(b"az_style_border_right_style_value_deep_copy")?);
            let az_style_border_right_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightStyleValue) -> AzString = transmute(lib.get(b"az_style_border_right_style_value_fmt_debug")?);
            let az_style_border_right_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderRightWidthValue) = transmute(lib.get(b"az_style_border_right_width_value_delete")?);
            let az_style_border_right_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderRightWidthValue) -> AzStyleBorderRightWidthValue = transmute(lib.get(b"az_style_border_right_width_value_deep_copy")?);
            let az_style_border_right_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightWidthValue) -> AzString = transmute(lib.get(b"az_style_border_right_width_value_fmt_debug")?);
            let az_style_border_top_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopColorValue) = transmute(lib.get(b"az_style_border_top_color_value_delete")?);
            let az_style_border_top_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopColorValue) -> AzStyleBorderTopColorValue = transmute(lib.get(b"az_style_border_top_color_value_deep_copy")?);
            let az_style_border_top_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopColorValue) -> AzString = transmute(lib.get(b"az_style_border_top_color_value_fmt_debug")?);
            let az_style_border_top_left_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopLeftRadiusValue) = transmute(lib.get(b"az_style_border_top_left_radius_value_delete")?);
            let az_style_border_top_left_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopLeftRadiusValue) -> AzStyleBorderTopLeftRadiusValue = transmute(lib.get(b"az_style_border_top_left_radius_value_deep_copy")?);
            let az_style_border_top_left_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopLeftRadiusValue) -> AzString = transmute(lib.get(b"az_style_border_top_left_radius_value_fmt_debug")?);
            let az_style_border_top_right_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopRightRadiusValue) = transmute(lib.get(b"az_style_border_top_right_radius_value_delete")?);
            let az_style_border_top_right_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopRightRadiusValue) -> AzStyleBorderTopRightRadiusValue = transmute(lib.get(b"az_style_border_top_right_radius_value_deep_copy")?);
            let az_style_border_top_right_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopRightRadiusValue) -> AzString = transmute(lib.get(b"az_style_border_top_right_radius_value_fmt_debug")?);
            let az_style_border_top_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopStyleValue) = transmute(lib.get(b"az_style_border_top_style_value_delete")?);
            let az_style_border_top_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopStyleValue) -> AzStyleBorderTopStyleValue = transmute(lib.get(b"az_style_border_top_style_value_deep_copy")?);
            let az_style_border_top_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopStyleValue) -> AzString = transmute(lib.get(b"az_style_border_top_style_value_fmt_debug")?);
            let az_style_border_top_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopWidthValue) = transmute(lib.get(b"az_style_border_top_width_value_delete")?);
            let az_style_border_top_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopWidthValue) -> AzStyleBorderTopWidthValue = transmute(lib.get(b"az_style_border_top_width_value_deep_copy")?);
            let az_style_border_top_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopWidthValue) -> AzString = transmute(lib.get(b"az_style_border_top_width_value_fmt_debug")?);
            let az_style_cursor_value_delete: extern "C" fn(_:  &mut AzStyleCursorValue) = transmute(lib.get(b"az_style_cursor_value_delete")?);
            let az_style_cursor_value_deep_copy: extern "C" fn(_:  &AzStyleCursorValue) -> AzStyleCursorValue = transmute(lib.get(b"az_style_cursor_value_deep_copy")?);
            let az_style_cursor_value_fmt_debug: extern "C" fn(_:  &AzStyleCursorValue) -> AzString = transmute(lib.get(b"az_style_cursor_value_fmt_debug")?);
            let az_style_font_family_value_delete: extern "C" fn(_:  &mut AzStyleFontFamilyValue) = transmute(lib.get(b"az_style_font_family_value_delete")?);
            let az_style_font_family_value_deep_copy: extern "C" fn(_:  &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue = transmute(lib.get(b"az_style_font_family_value_deep_copy")?);
            let az_style_font_family_value_fmt_debug: extern "C" fn(_:  &AzStyleFontFamilyValue) -> AzString = transmute(lib.get(b"az_style_font_family_value_fmt_debug")?);
            let az_style_font_size_value_delete: extern "C" fn(_:  &mut AzStyleFontSizeValue) = transmute(lib.get(b"az_style_font_size_value_delete")?);
            let az_style_font_size_value_deep_copy: extern "C" fn(_:  &AzStyleFontSizeValue) -> AzStyleFontSizeValue = transmute(lib.get(b"az_style_font_size_value_deep_copy")?);
            let az_style_font_size_value_fmt_debug: extern "C" fn(_:  &AzStyleFontSizeValue) -> AzString = transmute(lib.get(b"az_style_font_size_value_fmt_debug")?);
            let az_style_letter_spacing_value_delete: extern "C" fn(_:  &mut AzStyleLetterSpacingValue) = transmute(lib.get(b"az_style_letter_spacing_value_delete")?);
            let az_style_letter_spacing_value_deep_copy: extern "C" fn(_:  &AzStyleLetterSpacingValue) -> AzStyleLetterSpacingValue = transmute(lib.get(b"az_style_letter_spacing_value_deep_copy")?);
            let az_style_letter_spacing_value_fmt_debug: extern "C" fn(_:  &AzStyleLetterSpacingValue) -> AzString = transmute(lib.get(b"az_style_letter_spacing_value_fmt_debug")?);
            let az_style_line_height_value_delete: extern "C" fn(_:  &mut AzStyleLineHeightValue) = transmute(lib.get(b"az_style_line_height_value_delete")?);
            let az_style_line_height_value_deep_copy: extern "C" fn(_:  &AzStyleLineHeightValue) -> AzStyleLineHeightValue = transmute(lib.get(b"az_style_line_height_value_deep_copy")?);
            let az_style_line_height_value_fmt_debug: extern "C" fn(_:  &AzStyleLineHeightValue) -> AzString = transmute(lib.get(b"az_style_line_height_value_fmt_debug")?);
            let az_style_tab_width_value_delete: extern "C" fn(_:  &mut AzStyleTabWidthValue) = transmute(lib.get(b"az_style_tab_width_value_delete")?);
            let az_style_tab_width_value_deep_copy: extern "C" fn(_:  &AzStyleTabWidthValue) -> AzStyleTabWidthValue = transmute(lib.get(b"az_style_tab_width_value_deep_copy")?);
            let az_style_tab_width_value_fmt_debug: extern "C" fn(_:  &AzStyleTabWidthValue) -> AzString = transmute(lib.get(b"az_style_tab_width_value_fmt_debug")?);
            let az_style_text_alignment_horz_value_delete: extern "C" fn(_:  &mut AzStyleTextAlignmentHorzValue) = transmute(lib.get(b"az_style_text_alignment_horz_value_delete")?);
            let az_style_text_alignment_horz_value_deep_copy: extern "C" fn(_:  &AzStyleTextAlignmentHorzValue) -> AzStyleTextAlignmentHorzValue = transmute(lib.get(b"az_style_text_alignment_horz_value_deep_copy")?);
            let az_style_text_alignment_horz_value_fmt_debug: extern "C" fn(_:  &AzStyleTextAlignmentHorzValue) -> AzString = transmute(lib.get(b"az_style_text_alignment_horz_value_fmt_debug")?);
            let az_style_text_color_value_delete: extern "C" fn(_:  &mut AzStyleTextColorValue) = transmute(lib.get(b"az_style_text_color_value_delete")?);
            let az_style_text_color_value_deep_copy: extern "C" fn(_:  &AzStyleTextColorValue) -> AzStyleTextColorValue = transmute(lib.get(b"az_style_text_color_value_deep_copy")?);
            let az_style_text_color_value_fmt_debug: extern "C" fn(_:  &AzStyleTextColorValue) -> AzString = transmute(lib.get(b"az_style_text_color_value_fmt_debug")?);
            let az_style_word_spacing_value_delete: extern "C" fn(_:  &mut AzStyleWordSpacingValue) = transmute(lib.get(b"az_style_word_spacing_value_delete")?);
            let az_style_word_spacing_value_deep_copy: extern "C" fn(_:  &AzStyleWordSpacingValue) -> AzStyleWordSpacingValue = transmute(lib.get(b"az_style_word_spacing_value_deep_copy")?);
            let az_style_word_spacing_value_fmt_debug: extern "C" fn(_:  &AzStyleWordSpacingValue) -> AzString = transmute(lib.get(b"az_style_word_spacing_value_fmt_debug")?);
            let az_css_property_delete: extern "C" fn(_:  &mut AzCssProperty) = transmute(lib.get(b"az_css_property_delete")?);
            let az_css_property_deep_copy: extern "C" fn(_:  &AzCssProperty) -> AzCssProperty = transmute(lib.get(b"az_css_property_deep_copy")?);
            let az_css_property_fmt_debug: extern "C" fn(_:  &AzCssProperty) -> AzString = transmute(lib.get(b"az_css_property_fmt_debug")?);
            let az_css_property_partial_eq: extern "C" fn(_:  &AzCssProperty, _:  &AzCssProperty) -> bool = transmute(lib.get(b"az_css_property_partial_eq")?);
            let az_css_property_partial_cmp: extern "C" fn(_:  &AzCssProperty, _:  &AzCssProperty) -> u8 = transmute(lib.get(b"az_css_property_partial_cmp")?);
            let az_css_property_cmp: extern "C" fn(_:  &AzCssProperty, _:  &AzCssProperty) -> u8 = transmute(lib.get(b"az_css_property_cmp")?);
            let az_css_property_hash: extern "C" fn(_:  &AzCssProperty) -> u64 = transmute(lib.get(b"az_css_property_hash")?);
            let az_node_delete: extern "C" fn(_:  &mut AzNode) = transmute(lib.get(b"az_node_delete")?);
            let az_node_deep_copy: extern "C" fn(_:  &AzNode) -> AzNode = transmute(lib.get(b"az_node_deep_copy")?);
            let az_node_fmt_debug: extern "C" fn(_:  &AzNode) -> AzString = transmute(lib.get(b"az_node_fmt_debug")?);
            let az_node_partial_eq: extern "C" fn(_:  &AzNode, _:  &AzNode) -> bool = transmute(lib.get(b"az_node_partial_eq")?);
            let az_node_partial_cmp: extern "C" fn(_:  &AzNode, _:  &AzNode) -> u8 = transmute(lib.get(b"az_node_partial_cmp")?);
            let az_node_cmp: extern "C" fn(_:  &AzNode, _:  &AzNode) -> u8 = transmute(lib.get(b"az_node_cmp")?);
            let az_node_hash: extern "C" fn(_:  &AzNode) -> u64 = transmute(lib.get(b"az_node_hash")?);
            let az_cascade_info_delete: extern "C" fn(_:  &mut AzCascadeInfo) = transmute(lib.get(b"az_cascade_info_delete")?);
            let az_cascade_info_deep_copy: extern "C" fn(_:  &AzCascadeInfo) -> AzCascadeInfo = transmute(lib.get(b"az_cascade_info_deep_copy")?);
            let az_cascade_info_fmt_debug: extern "C" fn(_:  &AzCascadeInfo) -> AzString = transmute(lib.get(b"az_cascade_info_fmt_debug")?);
            let az_cascade_info_partial_eq: extern "C" fn(_:  &AzCascadeInfo, _:  &AzCascadeInfo) -> bool = transmute(lib.get(b"az_cascade_info_partial_eq")?);
            let az_cascade_info_partial_cmp: extern "C" fn(_:  &AzCascadeInfo, _:  &AzCascadeInfo) -> u8 = transmute(lib.get(b"az_cascade_info_partial_cmp")?);
            let az_cascade_info_cmp: extern "C" fn(_:  &AzCascadeInfo, _:  &AzCascadeInfo) -> u8 = transmute(lib.get(b"az_cascade_info_cmp")?);
            let az_cascade_info_hash: extern "C" fn(_:  &AzCascadeInfo) -> u64 = transmute(lib.get(b"az_cascade_info_hash")?);
            let az_rect_style_delete: extern "C" fn(_:  &mut AzRectStyle) = transmute(lib.get(b"az_rect_style_delete")?);
            let az_rect_style_deep_copy: extern "C" fn(_:  &AzRectStyle) -> AzRectStyle = transmute(lib.get(b"az_rect_style_deep_copy")?);
            let az_rect_style_fmt_debug: extern "C" fn(_:  &AzRectStyle) -> AzString = transmute(lib.get(b"az_rect_style_fmt_debug")?);
            let az_rect_style_partial_eq: extern "C" fn(_:  &AzRectStyle, _:  &AzRectStyle) -> bool = transmute(lib.get(b"az_rect_style_partial_eq")?);
            let az_rect_style_partial_cmp: extern "C" fn(_:  &AzRectStyle, _:  &AzRectStyle) -> u8 = transmute(lib.get(b"az_rect_style_partial_cmp")?);
            let az_rect_style_cmp: extern "C" fn(_:  &AzRectStyle, _:  &AzRectStyle) -> u8 = transmute(lib.get(b"az_rect_style_cmp")?);
            let az_rect_style_hash: extern "C" fn(_:  &AzRectStyle) -> u64 = transmute(lib.get(b"az_rect_style_hash")?);
            let az_rect_layout_delete: extern "C" fn(_:  &mut AzRectLayout) = transmute(lib.get(b"az_rect_layout_delete")?);
            let az_rect_layout_deep_copy: extern "C" fn(_:  &AzRectLayout) -> AzRectLayout = transmute(lib.get(b"az_rect_layout_deep_copy")?);
            let az_rect_layout_fmt_debug: extern "C" fn(_:  &AzRectLayout) -> AzString = transmute(lib.get(b"az_rect_layout_fmt_debug")?);
            let az_rect_layout_partial_eq: extern "C" fn(_:  &AzRectLayout, _:  &AzRectLayout) -> bool = transmute(lib.get(b"az_rect_layout_partial_eq")?);
            let az_rect_layout_partial_cmp: extern "C" fn(_:  &AzRectLayout, _:  &AzRectLayout) -> u8 = transmute(lib.get(b"az_rect_layout_partial_cmp")?);
            let az_rect_layout_cmp: extern "C" fn(_:  &AzRectLayout, _:  &AzRectLayout) -> u8 = transmute(lib.get(b"az_rect_layout_cmp")?);
            let az_rect_layout_hash: extern "C" fn(_:  &AzRectLayout) -> u64 = transmute(lib.get(b"az_rect_layout_hash")?);
            let az_cascaded_css_property_with_source_delete: extern "C" fn(_:  &mut AzCascadedCssPropertyWithSource) = transmute(lib.get(b"az_cascaded_css_property_with_source_delete")?);
            let az_cascaded_css_property_with_source_deep_copy: extern "C" fn(_:  &AzCascadedCssPropertyWithSource) -> AzCascadedCssPropertyWithSource = transmute(lib.get(b"az_cascaded_css_property_with_source_deep_copy")?);
            let az_cascaded_css_property_with_source_fmt_debug: extern "C" fn(_:  &AzCascadedCssPropertyWithSource) -> AzString = transmute(lib.get(b"az_cascaded_css_property_with_source_fmt_debug")?);
            let az_cascaded_css_property_with_source_partial_eq: extern "C" fn(_:  &AzCascadedCssPropertyWithSource, _:  &AzCascadedCssPropertyWithSource) -> bool = transmute(lib.get(b"az_cascaded_css_property_with_source_partial_eq")?);
            let az_cascaded_css_property_with_source_partial_cmp: extern "C" fn(_:  &AzCascadedCssPropertyWithSource, _:  &AzCascadedCssPropertyWithSource) -> u8 = transmute(lib.get(b"az_cascaded_css_property_with_source_partial_cmp")?);
            let az_cascaded_css_property_with_source_cmp: extern "C" fn(_:  &AzCascadedCssPropertyWithSource, _:  &AzCascadedCssPropertyWithSource) -> u8 = transmute(lib.get(b"az_cascaded_css_property_with_source_cmp")?);
            let az_cascaded_css_property_with_source_hash: extern "C" fn(_:  &AzCascadedCssPropertyWithSource) -> u64 = transmute(lib.get(b"az_cascaded_css_property_with_source_hash")?);
            let az_css_property_source_delete: extern "C" fn(_:  &mut AzCssPropertySource) = transmute(lib.get(b"az_css_property_source_delete")?);
            let az_css_property_source_deep_copy: extern "C" fn(_:  &AzCssPropertySource) -> AzCssPropertySource = transmute(lib.get(b"az_css_property_source_deep_copy")?);
            let az_css_property_source_fmt_debug: extern "C" fn(_:  &AzCssPropertySource) -> AzString = transmute(lib.get(b"az_css_property_source_fmt_debug")?);
            let az_css_property_source_partial_eq: extern "C" fn(_:  &AzCssPropertySource, _:  &AzCssPropertySource) -> bool = transmute(lib.get(b"az_css_property_source_partial_eq")?);
            let az_css_property_source_partial_cmp: extern "C" fn(_:  &AzCssPropertySource, _:  &AzCssPropertySource) -> u8 = transmute(lib.get(b"az_css_property_source_partial_cmp")?);
            let az_css_property_source_cmp: extern "C" fn(_:  &AzCssPropertySource, _:  &AzCssPropertySource) -> u8 = transmute(lib.get(b"az_css_property_source_cmp")?);
            let az_css_property_source_hash: extern "C" fn(_:  &AzCssPropertySource) -> u64 = transmute(lib.get(b"az_css_property_source_hash")?);
            let az_styled_node_delete: extern "C" fn(_:  &mut AzStyledNode) = transmute(lib.get(b"az_styled_node_delete")?);
            let az_styled_node_deep_copy: extern "C" fn(_:  &AzStyledNode) -> AzStyledNode = transmute(lib.get(b"az_styled_node_deep_copy")?);
            let az_styled_node_fmt_debug: extern "C" fn(_:  &AzStyledNode) -> AzString = transmute(lib.get(b"az_styled_node_fmt_debug")?);
            let az_styled_node_partial_eq: extern "C" fn(_:  &AzStyledNode, _:  &AzStyledNode) -> bool = transmute(lib.get(b"az_styled_node_partial_eq")?);
            let az_styled_node_partial_cmp: extern "C" fn(_:  &AzStyledNode, _:  &AzStyledNode) -> u8 = transmute(lib.get(b"az_styled_node_partial_cmp")?);
            let az_styled_node_cmp: extern "C" fn(_:  &AzStyledNode, _:  &AzStyledNode) -> u8 = transmute(lib.get(b"az_styled_node_cmp")?);
            let az_styled_node_hash: extern "C" fn(_:  &AzStyledNode) -> u64 = transmute(lib.get(b"az_styled_node_hash")?);
            let az_tag_id_delete: extern "C" fn(_:  &mut AzTagId) = transmute(lib.get(b"az_tag_id_delete")?);
            let az_tag_id_deep_copy: extern "C" fn(_:  &AzTagId) -> AzTagId = transmute(lib.get(b"az_tag_id_deep_copy")?);
            let az_tag_id_fmt_debug: extern "C" fn(_:  &AzTagId) -> AzString = transmute(lib.get(b"az_tag_id_fmt_debug")?);
            let az_tag_id_partial_eq: extern "C" fn(_:  &AzTagId, _:  &AzTagId) -> bool = transmute(lib.get(b"az_tag_id_partial_eq")?);
            let az_tag_id_partial_cmp: extern "C" fn(_:  &AzTagId, _:  &AzTagId) -> u8 = transmute(lib.get(b"az_tag_id_partial_cmp")?);
            let az_tag_id_cmp: extern "C" fn(_:  &AzTagId, _:  &AzTagId) -> u8 = transmute(lib.get(b"az_tag_id_cmp")?);
            let az_tag_id_hash: extern "C" fn(_:  &AzTagId) -> u64 = transmute(lib.get(b"az_tag_id_hash")?);
            let az_tag_id_to_node_id_mapping_delete: extern "C" fn(_:  &mut AzTagIdToNodeIdMapping) = transmute(lib.get(b"az_tag_id_to_node_id_mapping_delete")?);
            let az_tag_id_to_node_id_mapping_deep_copy: extern "C" fn(_:  &AzTagIdToNodeIdMapping) -> AzTagIdToNodeIdMapping = transmute(lib.get(b"az_tag_id_to_node_id_mapping_deep_copy")?);
            let az_tag_id_to_node_id_mapping_fmt_debug: extern "C" fn(_:  &AzTagIdToNodeIdMapping) -> AzString = transmute(lib.get(b"az_tag_id_to_node_id_mapping_fmt_debug")?);
            let az_tag_id_to_node_id_mapping_partial_eq: extern "C" fn(_:  &AzTagIdToNodeIdMapping, _:  &AzTagIdToNodeIdMapping) -> bool = transmute(lib.get(b"az_tag_id_to_node_id_mapping_partial_eq")?);
            let az_tag_id_to_node_id_mapping_partial_cmp: extern "C" fn(_:  &AzTagIdToNodeIdMapping, _:  &AzTagIdToNodeIdMapping) -> u8 = transmute(lib.get(b"az_tag_id_to_node_id_mapping_partial_cmp")?);
            let az_tag_id_to_node_id_mapping_cmp: extern "C" fn(_:  &AzTagIdToNodeIdMapping, _:  &AzTagIdToNodeIdMapping) -> u8 = transmute(lib.get(b"az_tag_id_to_node_id_mapping_cmp")?);
            let az_tag_id_to_node_id_mapping_hash: extern "C" fn(_:  &AzTagIdToNodeIdMapping) -> u64 = transmute(lib.get(b"az_tag_id_to_node_id_mapping_hash")?);
            let az_hover_group_delete: extern "C" fn(_:  &mut AzHoverGroup) = transmute(lib.get(b"az_hover_group_delete")?);
            let az_hover_group_deep_copy: extern "C" fn(_:  &AzHoverGroup) -> AzHoverGroup = transmute(lib.get(b"az_hover_group_deep_copy")?);
            let az_hover_group_fmt_debug: extern "C" fn(_:  &AzHoverGroup) -> AzString = transmute(lib.get(b"az_hover_group_fmt_debug")?);
            let az_hover_group_partial_eq: extern "C" fn(_:  &AzHoverGroup, _:  &AzHoverGroup) -> bool = transmute(lib.get(b"az_hover_group_partial_eq")?);
            let az_hover_group_partial_cmp: extern "C" fn(_:  &AzHoverGroup, _:  &AzHoverGroup) -> u8 = transmute(lib.get(b"az_hover_group_partial_cmp")?);
            let az_hover_group_cmp: extern "C" fn(_:  &AzHoverGroup, _:  &AzHoverGroup) -> u8 = transmute(lib.get(b"az_hover_group_cmp")?);
            let az_hover_group_hash: extern "C" fn(_:  &AzHoverGroup) -> u64 = transmute(lib.get(b"az_hover_group_hash")?);
            let az_active_hover_delete: extern "C" fn(_:  &mut AzActiveHover) = transmute(lib.get(b"az_active_hover_delete")?);
            let az_active_hover_deep_copy: extern "C" fn(_:  &AzActiveHover) -> AzActiveHover = transmute(lib.get(b"az_active_hover_deep_copy")?);
            let az_active_hover_fmt_debug: extern "C" fn(_:  &AzActiveHover) -> AzString = transmute(lib.get(b"az_active_hover_fmt_debug")?);
            let az_active_hover_partial_eq: extern "C" fn(_:  &AzActiveHover, _:  &AzActiveHover) -> bool = transmute(lib.get(b"az_active_hover_partial_eq")?);
            let az_active_hover_partial_cmp: extern "C" fn(_:  &AzActiveHover, _:  &AzActiveHover) -> u8 = transmute(lib.get(b"az_active_hover_partial_cmp")?);
            let az_active_hover_cmp: extern "C" fn(_:  &AzActiveHover, _:  &AzActiveHover) -> u8 = transmute(lib.get(b"az_active_hover_cmp")?);
            let az_active_hover_hash: extern "C" fn(_:  &AzActiveHover) -> u64 = transmute(lib.get(b"az_active_hover_hash")?);
            let az_parent_with_node_depth_delete: extern "C" fn(_:  &mut AzParentWithNodeDepth) = transmute(lib.get(b"az_parent_with_node_depth_delete")?);
            let az_parent_with_node_depth_deep_copy: extern "C" fn(_:  &AzParentWithNodeDepth) -> AzParentWithNodeDepth = transmute(lib.get(b"az_parent_with_node_depth_deep_copy")?);
            let az_parent_with_node_depth_fmt_debug: extern "C" fn(_:  &AzParentWithNodeDepth) -> AzString = transmute(lib.get(b"az_parent_with_node_depth_fmt_debug")?);
            let az_parent_with_node_depth_partial_eq: extern "C" fn(_:  &AzParentWithNodeDepth, _:  &AzParentWithNodeDepth) -> bool = transmute(lib.get(b"az_parent_with_node_depth_partial_eq")?);
            let az_parent_with_node_depth_partial_cmp: extern "C" fn(_:  &AzParentWithNodeDepth, _:  &AzParentWithNodeDepth) -> u8 = transmute(lib.get(b"az_parent_with_node_depth_partial_cmp")?);
            let az_parent_with_node_depth_cmp: extern "C" fn(_:  &AzParentWithNodeDepth, _:  &AzParentWithNodeDepth) -> u8 = transmute(lib.get(b"az_parent_with_node_depth_cmp")?);
            let az_parent_with_node_depth_hash: extern "C" fn(_:  &AzParentWithNodeDepth) -> u64 = transmute(lib.get(b"az_parent_with_node_depth_hash")?);
            let az_style_options_delete: extern "C" fn(_:  &mut AzStyleOptions) = transmute(lib.get(b"az_style_options_delete")?);
            let az_style_options_deep_copy: extern "C" fn(_:  &AzStyleOptions) -> AzStyleOptions = transmute(lib.get(b"az_style_options_deep_copy")?);
            let az_style_options_fmt_debug: extern "C" fn(_:  &AzStyleOptions) -> AzString = transmute(lib.get(b"az_style_options_fmt_debug")?);
            let az_style_options_partial_eq: extern "C" fn(_:  &AzStyleOptions, _:  &AzStyleOptions) -> bool = transmute(lib.get(b"az_style_options_partial_eq")?);
            let az_style_options_partial_cmp: extern "C" fn(_:  &AzStyleOptions, _:  &AzStyleOptions) -> u8 = transmute(lib.get(b"az_style_options_partial_cmp")?);
            let az_style_options_cmp: extern "C" fn(_:  &AzStyleOptions, _:  &AzStyleOptions) -> u8 = transmute(lib.get(b"az_style_options_cmp")?);
            let az_style_options_hash: extern "C" fn(_:  &AzStyleOptions) -> u64 = transmute(lib.get(b"az_style_options_hash")?);
            let az_styled_dom_new: extern "C" fn(_:  AzDom, _:  AzCss, _:  AzStyleOptions) -> AzStyledDom = transmute(lib.get(b"az_styled_dom_new")?);
            let az_styled_dom_append: extern "C" fn(_:  &mut AzStyledDom, _:  AzStyledDom) = transmute(lib.get(b"az_styled_dom_append")?);
            let az_styled_dom_delete: extern "C" fn(_:  &mut AzStyledDom) = transmute(lib.get(b"az_styled_dom_delete")?);
            let az_styled_dom_deep_copy: extern "C" fn(_:  &AzStyledDom) -> AzStyledDom = transmute(lib.get(b"az_styled_dom_deep_copy")?);
            let az_styled_dom_fmt_debug: extern "C" fn(_:  &AzStyledDom) -> AzString = transmute(lib.get(b"az_styled_dom_fmt_debug")?);
            let az_styled_dom_partial_eq: extern "C" fn(_:  &AzStyledDom, _:  &AzStyledDom) -> bool = transmute(lib.get(b"az_styled_dom_partial_eq")?);
            let az_styled_dom_partial_cmp: extern "C" fn(_:  &AzStyledDom, _:  &AzStyledDom) -> u8 = transmute(lib.get(b"az_styled_dom_partial_cmp")?);
            let az_styled_dom_cmp: extern "C" fn(_:  &AzStyledDom, _:  &AzStyledDom) -> u8 = transmute(lib.get(b"az_styled_dom_cmp")?);
            let az_styled_dom_hash: extern "C" fn(_:  &AzStyledDom) -> u64 = transmute(lib.get(b"az_styled_dom_hash")?);
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
            let az_dom_add_inline_css: extern "C" fn(_:  &mut AzDom, _:  AzCssProperty) = transmute(lib.get(b"az_dom_add_inline_css")?);
            let az_dom_with_inline_css: extern "C" fn(_:  AzDom, _:  AzCssProperty) -> AzDom = transmute(lib.get(b"az_dom_with_inline_css")?);
            let az_dom_set_is_draggable: extern "C" fn(_:  &mut AzDom, _:  bool) = transmute(lib.get(b"az_dom_set_is_draggable")?);
            let az_dom_with_clip_mask: extern "C" fn(_:  AzDom, _:  AzOptionImageMask) -> AzDom = transmute(lib.get(b"az_dom_with_clip_mask")?);
            let az_dom_set_clip_mask: extern "C" fn(_:  &mut AzDom, _:  AzOptionImageMask) = transmute(lib.get(b"az_dom_set_clip_mask")?);
            let az_dom_is_draggable: extern "C" fn(_:  AzDom, _:  bool) -> AzDom = transmute(lib.get(b"az_dom_is_draggable")?);
            let az_dom_set_tab_index: extern "C" fn(_:  &mut AzDom, _:  AzOptionTabIndex) = transmute(lib.get(b"az_dom_set_tab_index")?);
            let az_dom_with_tab_index: extern "C" fn(_:  AzDom, _:  AzOptionTabIndex) -> AzDom = transmute(lib.get(b"az_dom_with_tab_index")?);
            let az_dom_has_id: extern "C" fn(_:  &mut AzDom, _:  AzString) -> bool = transmute(lib.get(b"az_dom_has_id")?);
            let az_dom_has_class: extern "C" fn(_:  &mut AzDom, _:  AzString) -> bool = transmute(lib.get(b"az_dom_has_class")?);
            let az_dom_add_child: extern "C" fn(_:  &mut AzDom, _:  AzDom) = transmute(lib.get(b"az_dom_add_child")?);
            let az_dom_with_child: extern "C" fn(_:  AzDom, _:  AzDom) -> AzDom = transmute(lib.get(b"az_dom_with_child")?);
            let az_dom_get_html_string: extern "C" fn(_:  &AzDom) -> AzString = transmute(lib.get(b"az_dom_get_html_string")?);
            let az_dom_delete: extern "C" fn(_:  &mut AzDom) = transmute(lib.get(b"az_dom_delete")?);
            let az_dom_deep_copy: extern "C" fn(_:  &AzDom) -> AzDom = transmute(lib.get(b"az_dom_deep_copy")?);
            let az_dom_fmt_debug: extern "C" fn(_:  &AzDom) -> AzString = transmute(lib.get(b"az_dom_fmt_debug")?);
            let az_dom_partial_eq: extern "C" fn(_:  &AzDom, _:  &AzDom) -> bool = transmute(lib.get(b"az_dom_partial_eq")?);
            let az_dom_partial_cmp: extern "C" fn(_:  &AzDom, _:  &AzDom) -> u8 = transmute(lib.get(b"az_dom_partial_cmp")?);
            let az_dom_cmp: extern "C" fn(_:  &AzDom, _:  &AzDom) -> u8 = transmute(lib.get(b"az_dom_cmp")?);
            let az_dom_hash: extern "C" fn(_:  &AzDom) -> u64 = transmute(lib.get(b"az_dom_hash")?);
            let az_gl_texture_node_delete: extern "C" fn(_:  &mut AzGlTextureNode) = transmute(lib.get(b"az_gl_texture_node_delete")?);
            let az_gl_texture_node_deep_copy: extern "C" fn(_:  &AzGlTextureNode) -> AzGlTextureNode = transmute(lib.get(b"az_gl_texture_node_deep_copy")?);
            let az_gl_texture_node_fmt_debug: extern "C" fn(_:  &AzGlTextureNode) -> AzString = transmute(lib.get(b"az_gl_texture_node_fmt_debug")?);
            let az_i_frame_node_delete: extern "C" fn(_:  &mut AzIFrameNode) = transmute(lib.get(b"az_i_frame_node_delete")?);
            let az_i_frame_node_deep_copy: extern "C" fn(_:  &AzIFrameNode) -> AzIFrameNode = transmute(lib.get(b"az_i_frame_node_deep_copy")?);
            let az_i_frame_node_fmt_debug: extern "C" fn(_:  &AzIFrameNode) -> AzString = transmute(lib.get(b"az_i_frame_node_fmt_debug")?);
            let az_callback_data_delete: extern "C" fn(_:  &mut AzCallbackData) = transmute(lib.get(b"az_callback_data_delete")?);
            let az_callback_data_deep_copy: extern "C" fn(_:  &AzCallbackData) -> AzCallbackData = transmute(lib.get(b"az_callback_data_deep_copy")?);
            let az_callback_data_fmt_debug: extern "C" fn(_:  &AzCallbackData) -> AzString = transmute(lib.get(b"az_callback_data_fmt_debug")?);
            let az_callback_data_partial_eq: extern "C" fn(_:  &AzCallbackData, _:  &AzCallbackData) -> bool = transmute(lib.get(b"az_callback_data_partial_eq")?);
            let az_callback_data_partial_cmp: extern "C" fn(_:  &AzCallbackData, _:  &AzCallbackData) -> u8 = transmute(lib.get(b"az_callback_data_partial_cmp")?);
            let az_callback_data_cmp: extern "C" fn(_:  &AzCallbackData, _:  &AzCallbackData) -> u8 = transmute(lib.get(b"az_callback_data_cmp")?);
            let az_callback_data_hash: extern "C" fn(_:  &AzCallbackData) -> u64 = transmute(lib.get(b"az_callback_data_hash")?);
            let az_image_mask_delete: extern "C" fn(_:  &mut AzImageMask) = transmute(lib.get(b"az_image_mask_delete")?);
            let az_image_mask_deep_copy: extern "C" fn(_:  &AzImageMask) -> AzImageMask = transmute(lib.get(b"az_image_mask_deep_copy")?);
            let az_image_mask_fmt_debug: extern "C" fn(_:  &AzImageMask) -> AzString = transmute(lib.get(b"az_image_mask_fmt_debug")?);
            let az_image_mask_partial_eq: extern "C" fn(_:  &AzImageMask, _:  &AzImageMask) -> bool = transmute(lib.get(b"az_image_mask_partial_eq")?);
            let az_image_mask_partial_cmp: extern "C" fn(_:  &AzImageMask, _:  &AzImageMask) -> u8 = transmute(lib.get(b"az_image_mask_partial_cmp")?);
            let az_image_mask_cmp: extern "C" fn(_:  &AzImageMask, _:  &AzImageMask) -> u8 = transmute(lib.get(b"az_image_mask_cmp")?);
            let az_image_mask_hash: extern "C" fn(_:  &AzImageMask) -> u64 = transmute(lib.get(b"az_image_mask_hash")?);
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
            let az_node_data_add_callback: extern "C" fn(_:  &mut AzNodeData, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) = transmute(lib.get(b"az_node_data_add_callback")?);
            let az_node_data_with_callback: extern "C" fn(_:  AzNodeData, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzNodeData = transmute(lib.get(b"az_node_data_with_callback")?);
            let az_node_data_add_inline_css: extern "C" fn(_:  &mut AzNodeData, _:  AzCssProperty) = transmute(lib.get(b"az_node_data_add_inline_css")?);
            let az_node_data_with_inline_css: extern "C" fn(_:  AzNodeData, _:  AzCssProperty) -> AzNodeData = transmute(lib.get(b"az_node_data_with_inline_css")?);
            let az_node_data_with_clip_mask: extern "C" fn(_:  AzNodeData, _:  AzOptionImageMask) -> AzNodeData = transmute(lib.get(b"az_node_data_with_clip_mask")?);
            let az_node_data_set_clip_mask: extern "C" fn(_:  &mut AzNodeData, _:  AzOptionImageMask) = transmute(lib.get(b"az_node_data_set_clip_mask")?);
            let az_node_data_set_is_draggable: extern "C" fn(_:  &mut AzNodeData, _:  bool) = transmute(lib.get(b"az_node_data_set_is_draggable")?);
            let az_node_data_is_draggable: extern "C" fn(_:  AzNodeData, _:  bool) -> AzNodeData = transmute(lib.get(b"az_node_data_is_draggable")?);
            let az_node_data_set_tab_index: extern "C" fn(_:  &mut AzNodeData, _:  AzOptionTabIndex) = transmute(lib.get(b"az_node_data_set_tab_index")?);
            let az_node_data_with_tab_index: extern "C" fn(_:  AzNodeData, _:  AzOptionTabIndex) -> AzNodeData = transmute(lib.get(b"az_node_data_with_tab_index")?);
            let az_node_data_has_id: extern "C" fn(_:  &mut AzNodeData, _:  AzString) -> bool = transmute(lib.get(b"az_node_data_has_id")?);
            let az_node_data_has_class: extern "C" fn(_:  &mut AzNodeData, _:  AzString) -> bool = transmute(lib.get(b"az_node_data_has_class")?);
            let az_node_data_delete: extern "C" fn(_:  &mut AzNodeData) = transmute(lib.get(b"az_node_data_delete")?);
            let az_node_data_deep_copy: extern "C" fn(_:  &AzNodeData) -> AzNodeData = transmute(lib.get(b"az_node_data_deep_copy")?);
            let az_node_data_fmt_debug: extern "C" fn(_:  &AzNodeData) -> AzString = transmute(lib.get(b"az_node_data_fmt_debug")?);
            let az_node_type_delete: extern "C" fn(_:  &mut AzNodeType) = transmute(lib.get(b"az_node_type_delete")?);
            let az_node_type_deep_copy: extern "C" fn(_:  &AzNodeType) -> AzNodeType = transmute(lib.get(b"az_node_type_deep_copy")?);
            let az_node_type_fmt_debug: extern "C" fn(_:  &AzNodeType) -> AzString = transmute(lib.get(b"az_node_type_fmt_debug")?);
            let az_on_into_event_filter: extern "C" fn(_:  AzOn) -> AzEventFilter = transmute(lib.get(b"az_on_into_event_filter")?);
            let az_on_delete: extern "C" fn(_:  &mut AzOn) = transmute(lib.get(b"az_on_delete")?);
            let az_on_deep_copy: extern "C" fn(_:  &AzOn) -> AzOn = transmute(lib.get(b"az_on_deep_copy")?);
            let az_on_fmt_debug: extern "C" fn(_:  &AzOn) -> AzString = transmute(lib.get(b"az_on_fmt_debug")?);
            let az_event_filter_delete: extern "C" fn(_:  &mut AzEventFilter) = transmute(lib.get(b"az_event_filter_delete")?);
            let az_event_filter_deep_copy: extern "C" fn(_:  &AzEventFilter) -> AzEventFilter = transmute(lib.get(b"az_event_filter_deep_copy")?);
            let az_event_filter_fmt_debug: extern "C" fn(_:  &AzEventFilter) -> AzString = transmute(lib.get(b"az_event_filter_fmt_debug")?);
            let az_hover_event_filter_delete: extern "C" fn(_:  &mut AzHoverEventFilter) = transmute(lib.get(b"az_hover_event_filter_delete")?);
            let az_hover_event_filter_deep_copy: extern "C" fn(_:  &AzHoverEventFilter) -> AzHoverEventFilter = transmute(lib.get(b"az_hover_event_filter_deep_copy")?);
            let az_hover_event_filter_fmt_debug: extern "C" fn(_:  &AzHoverEventFilter) -> AzString = transmute(lib.get(b"az_hover_event_filter_fmt_debug")?);
            let az_focus_event_filter_delete: extern "C" fn(_:  &mut AzFocusEventFilter) = transmute(lib.get(b"az_focus_event_filter_delete")?);
            let az_focus_event_filter_deep_copy: extern "C" fn(_:  &AzFocusEventFilter) -> AzFocusEventFilter = transmute(lib.get(b"az_focus_event_filter_deep_copy")?);
            let az_focus_event_filter_fmt_debug: extern "C" fn(_:  &AzFocusEventFilter) -> AzString = transmute(lib.get(b"az_focus_event_filter_fmt_debug")?);
            let az_not_event_filter_delete: extern "C" fn(_:  &mut AzNotEventFilter) = transmute(lib.get(b"az_not_event_filter_delete")?);
            let az_not_event_filter_deep_copy: extern "C" fn(_:  &AzNotEventFilter) -> AzNotEventFilter = transmute(lib.get(b"az_not_event_filter_deep_copy")?);
            let az_not_event_filter_fmt_debug: extern "C" fn(_:  &AzNotEventFilter) -> AzString = transmute(lib.get(b"az_not_event_filter_fmt_debug")?);
            let az_window_event_filter_delete: extern "C" fn(_:  &mut AzWindowEventFilter) = transmute(lib.get(b"az_window_event_filter_delete")?);
            let az_window_event_filter_deep_copy: extern "C" fn(_:  &AzWindowEventFilter) -> AzWindowEventFilter = transmute(lib.get(b"az_window_event_filter_deep_copy")?);
            let az_window_event_filter_fmt_debug: extern "C" fn(_:  &AzWindowEventFilter) -> AzString = transmute(lib.get(b"az_window_event_filter_fmt_debug")?);
            let az_tab_index_delete: extern "C" fn(_:  &mut AzTabIndex) = transmute(lib.get(b"az_tab_index_delete")?);
            let az_tab_index_deep_copy: extern "C" fn(_:  &AzTabIndex) -> AzTabIndex = transmute(lib.get(b"az_tab_index_deep_copy")?);
            let az_tab_index_fmt_debug: extern "C" fn(_:  &AzTabIndex) -> AzString = transmute(lib.get(b"az_tab_index_fmt_debug")?);
            let az_gl_shader_precision_format_return_delete: extern "C" fn(_:  &mut AzGlShaderPrecisionFormatReturn) = transmute(lib.get(b"az_gl_shader_precision_format_return_delete")?);
            let az_gl_shader_precision_format_return_deep_copy: extern "C" fn(_:  &AzGlShaderPrecisionFormatReturn) -> AzGlShaderPrecisionFormatReturn = transmute(lib.get(b"az_gl_shader_precision_format_return_deep_copy")?);
            let az_gl_shader_precision_format_return_fmt_debug: extern "C" fn(_:  &AzGlShaderPrecisionFormatReturn) -> AzString = transmute(lib.get(b"az_gl_shader_precision_format_return_fmt_debug")?);
            let az_vertex_attribute_type_delete: extern "C" fn(_:  &mut AzVertexAttributeType) = transmute(lib.get(b"az_vertex_attribute_type_delete")?);
            let az_vertex_attribute_type_deep_copy: extern "C" fn(_:  &AzVertexAttributeType) -> AzVertexAttributeType = transmute(lib.get(b"az_vertex_attribute_type_deep_copy")?);
            let az_vertex_attribute_type_fmt_debug: extern "C" fn(_:  &AzVertexAttributeType) -> AzString = transmute(lib.get(b"az_vertex_attribute_type_fmt_debug")?);
            let az_vertex_attribute_delete: extern "C" fn(_:  &mut AzVertexAttribute) = transmute(lib.get(b"az_vertex_attribute_delete")?);
            let az_vertex_attribute_deep_copy: extern "C" fn(_:  &AzVertexAttribute) -> AzVertexAttribute = transmute(lib.get(b"az_vertex_attribute_deep_copy")?);
            let az_vertex_attribute_fmt_debug: extern "C" fn(_:  &AzVertexAttribute) -> AzString = transmute(lib.get(b"az_vertex_attribute_fmt_debug")?);
            let az_vertex_layout_delete: extern "C" fn(_:  &mut AzVertexLayout) = transmute(lib.get(b"az_vertex_layout_delete")?);
            let az_vertex_layout_deep_copy: extern "C" fn(_:  &AzVertexLayout) -> AzVertexLayout = transmute(lib.get(b"az_vertex_layout_deep_copy")?);
            let az_vertex_layout_fmt_debug: extern "C" fn(_:  &AzVertexLayout) -> AzString = transmute(lib.get(b"az_vertex_layout_fmt_debug")?);
            let az_vertex_array_object_delete: extern "C" fn(_:  &mut AzVertexArrayObject) = transmute(lib.get(b"az_vertex_array_object_delete")?);
            let az_vertex_array_object_fmt_debug: extern "C" fn(_:  &AzVertexArrayObject) -> AzString = transmute(lib.get(b"az_vertex_array_object_fmt_debug")?);
            let az_index_buffer_format_delete: extern "C" fn(_:  &mut AzIndexBufferFormat) = transmute(lib.get(b"az_index_buffer_format_delete")?);
            let az_index_buffer_format_deep_copy: extern "C" fn(_:  &AzIndexBufferFormat) -> AzIndexBufferFormat = transmute(lib.get(b"az_index_buffer_format_deep_copy")?);
            let az_index_buffer_format_fmt_debug: extern "C" fn(_:  &AzIndexBufferFormat) -> AzString = transmute(lib.get(b"az_index_buffer_format_fmt_debug")?);
            let az_vertex_buffer_delete: extern "C" fn(_:  &mut AzVertexBuffer) = transmute(lib.get(b"az_vertex_buffer_delete")?);
            let az_vertex_buffer_fmt_debug: extern "C" fn(_:  &AzVertexBuffer) -> AzString = transmute(lib.get(b"az_vertex_buffer_fmt_debug")?);
            let az_gl_type_delete: extern "C" fn(_:  &mut AzGlType) = transmute(lib.get(b"az_gl_type_delete")?);
            let az_gl_type_deep_copy: extern "C" fn(_:  &AzGlType) -> AzGlType = transmute(lib.get(b"az_gl_type_deep_copy")?);
            let az_gl_type_fmt_debug: extern "C" fn(_:  &AzGlType) -> AzString = transmute(lib.get(b"az_gl_type_fmt_debug")?);
            let az_debug_message_delete: extern "C" fn(_:  &mut AzDebugMessage) = transmute(lib.get(b"az_debug_message_delete")?);
            let az_debug_message_deep_copy: extern "C" fn(_:  &AzDebugMessage) -> AzDebugMessage = transmute(lib.get(b"az_debug_message_deep_copy")?);
            let az_debug_message_fmt_debug: extern "C" fn(_:  &AzDebugMessage) -> AzString = transmute(lib.get(b"az_debug_message_fmt_debug")?);
            let az_debug_message_partial_eq: extern "C" fn(_:  &AzDebugMessage, _:  &AzDebugMessage) -> bool = transmute(lib.get(b"az_debug_message_partial_eq")?);
            let az_debug_message_partial_cmp: extern "C" fn(_:  &AzDebugMessage, _:  &AzDebugMessage) -> u8 = transmute(lib.get(b"az_debug_message_partial_cmp")?);
            let az_debug_message_cmp: extern "C" fn(_:  &AzDebugMessage, _:  &AzDebugMessage) -> u8 = transmute(lib.get(b"az_debug_message_cmp")?);
            let az_debug_message_hash: extern "C" fn(_:  &AzDebugMessage) -> u64 = transmute(lib.get(b"az_debug_message_hash")?);
            let az_u8_vec_ref_delete: extern "C" fn(_:  &mut AzU8VecRef) = transmute(lib.get(b"az_u8_vec_ref_delete")?);
            let az_u8_vec_ref_fmt_debug: extern "C" fn(_:  &AzU8VecRef) -> AzString = transmute(lib.get(b"az_u8_vec_ref_fmt_debug")?);
            let az_u8_vec_ref_mut_delete: extern "C" fn(_:  &mut AzU8VecRefMut) = transmute(lib.get(b"az_u8_vec_ref_mut_delete")?);
            let az_u8_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzU8VecRefMut) -> AzString = transmute(lib.get(b"az_u8_vec_ref_mut_fmt_debug")?);
            let az_f32_vec_ref_delete: extern "C" fn(_:  &mut AzF32VecRef) = transmute(lib.get(b"az_f32_vec_ref_delete")?);
            let az_f32_vec_ref_fmt_debug: extern "C" fn(_:  &AzF32VecRef) -> AzString = transmute(lib.get(b"az_f32_vec_ref_fmt_debug")?);
            let az_i32_vec_ref_delete: extern "C" fn(_:  &mut AzI32VecRef) = transmute(lib.get(b"az_i32_vec_ref_delete")?);
            let az_i32_vec_ref_fmt_debug: extern "C" fn(_:  &AzI32VecRef) -> AzString = transmute(lib.get(b"az_i32_vec_ref_fmt_debug")?);
            let az_g_luint_vec_ref_delete: extern "C" fn(_:  &mut AzGLuintVecRef) = transmute(lib.get(b"az_g_luint_vec_ref_delete")?);
            let az_g_luint_vec_ref_fmt_debug: extern "C" fn(_:  &AzGLuintVecRef) -> AzString = transmute(lib.get(b"az_g_luint_vec_ref_fmt_debug")?);
            let az_g_lenum_vec_ref_delete: extern "C" fn(_:  &mut AzGLenumVecRef) = transmute(lib.get(b"az_g_lenum_vec_ref_delete")?);
            let az_g_lenum_vec_ref_fmt_debug: extern "C" fn(_:  &AzGLenumVecRef) -> AzString = transmute(lib.get(b"az_g_lenum_vec_ref_fmt_debug")?);
            let az_g_lint_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLintVecRefMut) = transmute(lib.get(b"az_g_lint_vec_ref_mut_delete")?);
            let az_g_lint_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLintVecRefMut) -> AzString = transmute(lib.get(b"az_g_lint_vec_ref_mut_fmt_debug")?);
            let az_g_lint64_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLint64VecRefMut) = transmute(lib.get(b"az_g_lint64_vec_ref_mut_delete")?);
            let az_g_lint64_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLint64VecRefMut) -> AzString = transmute(lib.get(b"az_g_lint64_vec_ref_mut_fmt_debug")?);
            let az_g_lboolean_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLbooleanVecRefMut) = transmute(lib.get(b"az_g_lboolean_vec_ref_mut_delete")?);
            let az_g_lboolean_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLbooleanVecRefMut) -> AzString = transmute(lib.get(b"az_g_lboolean_vec_ref_mut_fmt_debug")?);
            let az_g_lfloat_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLfloatVecRefMut) = transmute(lib.get(b"az_g_lfloat_vec_ref_mut_delete")?);
            let az_g_lfloat_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLfloatVecRefMut) -> AzString = transmute(lib.get(b"az_g_lfloat_vec_ref_mut_fmt_debug")?);
            let az_refstr_vec_ref_delete: extern "C" fn(_:  &mut AzRefstrVecRef) = transmute(lib.get(b"az_refstr_vec_ref_delete")?);
            let az_refstr_vec_ref_fmt_debug: extern "C" fn(_:  &AzRefstrVecRef) -> AzString = transmute(lib.get(b"az_refstr_vec_ref_fmt_debug")?);
            let az_refstr_delete: extern "C" fn(_:  &mut AzRefstr) = transmute(lib.get(b"az_refstr_delete")?);
            let az_refstr_fmt_debug: extern "C" fn(_:  &AzRefstr) -> AzString = transmute(lib.get(b"az_refstr_fmt_debug")?);
            let az_get_program_binary_return_delete: extern "C" fn(_:  &mut AzGetProgramBinaryReturn) = transmute(lib.get(b"az_get_program_binary_return_delete")?);
            let az_get_program_binary_return_deep_copy: extern "C" fn(_:  &AzGetProgramBinaryReturn) -> AzGetProgramBinaryReturn = transmute(lib.get(b"az_get_program_binary_return_deep_copy")?);
            let az_get_program_binary_return_fmt_debug: extern "C" fn(_:  &AzGetProgramBinaryReturn) -> AzString = transmute(lib.get(b"az_get_program_binary_return_fmt_debug")?);
            let az_get_active_attrib_return_delete: extern "C" fn(_:  &mut AzGetActiveAttribReturn) = transmute(lib.get(b"az_get_active_attrib_return_delete")?);
            let az_get_active_attrib_return_deep_copy: extern "C" fn(_:  &AzGetActiveAttribReturn) -> AzGetActiveAttribReturn = transmute(lib.get(b"az_get_active_attrib_return_deep_copy")?);
            let az_get_active_attrib_return_fmt_debug: extern "C" fn(_:  &AzGetActiveAttribReturn) -> AzString = transmute(lib.get(b"az_get_active_attrib_return_fmt_debug")?);
            let az_g_lsync_ptr_delete: extern "C" fn(_:  &mut AzGLsyncPtr) = transmute(lib.get(b"az_g_lsync_ptr_delete")?);
            let az_g_lsync_ptr_fmt_debug: extern "C" fn(_:  &AzGLsyncPtr) -> AzString = transmute(lib.get(b"az_g_lsync_ptr_fmt_debug")?);
            let az_get_active_uniform_return_delete: extern "C" fn(_:  &mut AzGetActiveUniformReturn) = transmute(lib.get(b"az_get_active_uniform_return_delete")?);
            let az_get_active_uniform_return_deep_copy: extern "C" fn(_:  &AzGetActiveUniformReturn) -> AzGetActiveUniformReturn = transmute(lib.get(b"az_get_active_uniform_return_deep_copy")?);
            let az_get_active_uniform_return_fmt_debug: extern "C" fn(_:  &AzGetActiveUniformReturn) -> AzString = transmute(lib.get(b"az_get_active_uniform_return_fmt_debug")?);
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
            let az_gl_context_ptr_fmt_debug: extern "C" fn(_:  &AzGlContextPtr) -> AzString = transmute(lib.get(b"az_gl_context_ptr_fmt_debug")?);
            let az_texture_delete: extern "C" fn(_:  &mut AzTexture) = transmute(lib.get(b"az_texture_delete")?);
            let az_texture_fmt_debug: extern "C" fn(_:  &AzTexture) -> AzString = transmute(lib.get(b"az_texture_fmt_debug")?);
            let az_texture_flags_default: extern "C" fn() -> AzTextureFlags = transmute(lib.get(b"az_texture_flags_default")?);
            let az_texture_flags_delete: extern "C" fn(_:  &mut AzTextureFlags) = transmute(lib.get(b"az_texture_flags_delete")?);
            let az_texture_flags_deep_copy: extern "C" fn(_:  &AzTextureFlags) -> AzTextureFlags = transmute(lib.get(b"az_texture_flags_deep_copy")?);
            let az_texture_flags_fmt_debug: extern "C" fn(_:  &AzTextureFlags) -> AzString = transmute(lib.get(b"az_texture_flags_fmt_debug")?);
            let az_text_id_new: extern "C" fn() -> AzTextId = transmute(lib.get(b"az_text_id_new")?);
            let az_text_id_delete: extern "C" fn(_:  &mut AzTextId) = transmute(lib.get(b"az_text_id_delete")?);
            let az_text_id_deep_copy: extern "C" fn(_:  &AzTextId) -> AzTextId = transmute(lib.get(b"az_text_id_deep_copy")?);
            let az_text_id_fmt_debug: extern "C" fn(_:  &AzTextId) -> AzString = transmute(lib.get(b"az_text_id_fmt_debug")?);
            let az_image_id_new: extern "C" fn() -> AzImageId = transmute(lib.get(b"az_image_id_new")?);
            let az_image_id_delete: extern "C" fn(_:  &mut AzImageId) = transmute(lib.get(b"az_image_id_delete")?);
            let az_image_id_deep_copy: extern "C" fn(_:  &AzImageId) -> AzImageId = transmute(lib.get(b"az_image_id_deep_copy")?);
            let az_image_id_fmt_debug: extern "C" fn(_:  &AzImageId) -> AzString = transmute(lib.get(b"az_image_id_fmt_debug")?);
            let az_image_id_partial_eq: extern "C" fn(_:  &AzImageId, _:  &AzImageId) -> bool = transmute(lib.get(b"az_image_id_partial_eq")?);
            let az_image_id_partial_cmp: extern "C" fn(_:  &AzImageId, _:  &AzImageId) -> u8 = transmute(lib.get(b"az_image_id_partial_cmp")?);
            let az_image_id_cmp: extern "C" fn(_:  &AzImageId, _:  &AzImageId) -> u8 = transmute(lib.get(b"az_image_id_cmp")?);
            let az_image_id_hash: extern "C" fn(_:  &AzImageId) -> u64 = transmute(lib.get(b"az_image_id_hash")?);
            let az_font_id_new: extern "C" fn() -> AzFontId = transmute(lib.get(b"az_font_id_new")?);
            let az_font_id_delete: extern "C" fn(_:  &mut AzFontId) = transmute(lib.get(b"az_font_id_delete")?);
            let az_font_id_deep_copy: extern "C" fn(_:  &AzFontId) -> AzFontId = transmute(lib.get(b"az_font_id_deep_copy")?);
            let az_font_id_fmt_debug: extern "C" fn(_:  &AzFontId) -> AzString = transmute(lib.get(b"az_font_id_fmt_debug")?);
            let az_image_source_delete: extern "C" fn(_:  &mut AzImageSource) = transmute(lib.get(b"az_image_source_delete")?);
            let az_image_source_deep_copy: extern "C" fn(_:  &AzImageSource) -> AzImageSource = transmute(lib.get(b"az_image_source_deep_copy")?);
            let az_image_source_fmt_debug: extern "C" fn(_:  &AzImageSource) -> AzString = transmute(lib.get(b"az_image_source_fmt_debug")?);
            let az_font_source_delete: extern "C" fn(_:  &mut AzFontSource) = transmute(lib.get(b"az_font_source_delete")?);
            let az_font_source_deep_copy: extern "C" fn(_:  &AzFontSource) -> AzFontSource = transmute(lib.get(b"az_font_source_deep_copy")?);
            let az_font_source_fmt_debug: extern "C" fn(_:  &AzFontSource) -> AzString = transmute(lib.get(b"az_font_source_fmt_debug")?);
            let az_raw_image_new: extern "C" fn(_:  AzU8Vec, _:  usize, _:  usize, _:  AzRawImageFormat) -> AzRawImage = transmute(lib.get(b"az_raw_image_new")?);
            let az_raw_image_delete: extern "C" fn(_:  &mut AzRawImage) = transmute(lib.get(b"az_raw_image_delete")?);
            let az_raw_image_deep_copy: extern "C" fn(_:  &AzRawImage) -> AzRawImage = transmute(lib.get(b"az_raw_image_deep_copy")?);
            let az_raw_image_fmt_debug: extern "C" fn(_:  &AzRawImage) -> AzString = transmute(lib.get(b"az_raw_image_fmt_debug")?);
            let az_raw_image_format_delete: extern "C" fn(_:  &mut AzRawImageFormat) = transmute(lib.get(b"az_raw_image_format_delete")?);
            let az_raw_image_format_deep_copy: extern "C" fn(_:  &AzRawImageFormat) -> AzRawImageFormat = transmute(lib.get(b"az_raw_image_format_deep_copy")?);
            let az_raw_image_format_fmt_debug: extern "C" fn(_:  &AzRawImageFormat) -> AzString = transmute(lib.get(b"az_raw_image_format_fmt_debug")?);
            let az_svg_multi_polygon_delete: extern "C" fn(_:  &mut AzSvgMultiPolygon) = transmute(lib.get(b"az_svg_multi_polygon_delete")?);
            let az_svg_multi_polygon_deep_copy: extern "C" fn(_:  &AzSvgMultiPolygon) -> AzSvgMultiPolygon = transmute(lib.get(b"az_svg_multi_polygon_deep_copy")?);
            let az_svg_multi_polygon_fmt_debug: extern "C" fn(_:  &AzSvgMultiPolygon) -> AzString = transmute(lib.get(b"az_svg_multi_polygon_fmt_debug")?);
            let az_svg_node_delete: extern "C" fn(_:  &mut AzSvgNode) = transmute(lib.get(b"az_svg_node_delete")?);
            let az_svg_node_deep_copy: extern "C" fn(_:  &AzSvgNode) -> AzSvgNode = transmute(lib.get(b"az_svg_node_deep_copy")?);
            let az_svg_node_fmt_debug: extern "C" fn(_:  &AzSvgNode) -> AzString = transmute(lib.get(b"az_svg_node_fmt_debug")?);
            let az_svg_styled_node_delete: extern "C" fn(_:  &mut AzSvgStyledNode) = transmute(lib.get(b"az_svg_styled_node_delete")?);
            let az_svg_styled_node_deep_copy: extern "C" fn(_:  &AzSvgStyledNode) -> AzSvgStyledNode = transmute(lib.get(b"az_svg_styled_node_deep_copy")?);
            let az_svg_styled_node_fmt_debug: extern "C" fn(_:  &AzSvgStyledNode) -> AzString = transmute(lib.get(b"az_svg_styled_node_fmt_debug")?);
            let az_svg_circle_delete: extern "C" fn(_:  &mut AzSvgCircle) = transmute(lib.get(b"az_svg_circle_delete")?);
            let az_svg_circle_deep_copy: extern "C" fn(_:  &AzSvgCircle) -> AzSvgCircle = transmute(lib.get(b"az_svg_circle_deep_copy")?);
            let az_svg_circle_fmt_debug: extern "C" fn(_:  &AzSvgCircle) -> AzString = transmute(lib.get(b"az_svg_circle_fmt_debug")?);
            let az_svg_path_delete: extern "C" fn(_:  &mut AzSvgPath) = transmute(lib.get(b"az_svg_path_delete")?);
            let az_svg_path_deep_copy: extern "C" fn(_:  &AzSvgPath) -> AzSvgPath = transmute(lib.get(b"az_svg_path_deep_copy")?);
            let az_svg_path_fmt_debug: extern "C" fn(_:  &AzSvgPath) -> AzString = transmute(lib.get(b"az_svg_path_fmt_debug")?);
            let az_svg_path_element_delete: extern "C" fn(_:  &mut AzSvgPathElement) = transmute(lib.get(b"az_svg_path_element_delete")?);
            let az_svg_path_element_deep_copy: extern "C" fn(_:  &AzSvgPathElement) -> AzSvgPathElement = transmute(lib.get(b"az_svg_path_element_deep_copy")?);
            let az_svg_path_element_fmt_debug: extern "C" fn(_:  &AzSvgPathElement) -> AzString = transmute(lib.get(b"az_svg_path_element_fmt_debug")?);
            let az_svg_line_delete: extern "C" fn(_:  &mut AzSvgLine) = transmute(lib.get(b"az_svg_line_delete")?);
            let az_svg_line_deep_copy: extern "C" fn(_:  &AzSvgLine) -> AzSvgLine = transmute(lib.get(b"az_svg_line_deep_copy")?);
            let az_svg_line_fmt_debug: extern "C" fn(_:  &AzSvgLine) -> AzString = transmute(lib.get(b"az_svg_line_fmt_debug")?);
            let az_svg_point_delete: extern "C" fn(_:  &mut AzSvgPoint) = transmute(lib.get(b"az_svg_point_delete")?);
            let az_svg_point_deep_copy: extern "C" fn(_:  &AzSvgPoint) -> AzSvgPoint = transmute(lib.get(b"az_svg_point_deep_copy")?);
            let az_svg_point_fmt_debug: extern "C" fn(_:  &AzSvgPoint) -> AzString = transmute(lib.get(b"az_svg_point_fmt_debug")?);
            let az_svg_vertex_delete: extern "C" fn(_:  &mut AzSvgVertex) = transmute(lib.get(b"az_svg_vertex_delete")?);
            let az_svg_vertex_deep_copy: extern "C" fn(_:  &AzSvgVertex) -> AzSvgVertex = transmute(lib.get(b"az_svg_vertex_deep_copy")?);
            let az_svg_vertex_fmt_debug: extern "C" fn(_:  &AzSvgVertex) -> AzString = transmute(lib.get(b"az_svg_vertex_fmt_debug")?);
            let az_svg_quadratic_curve_delete: extern "C" fn(_:  &mut AzSvgQuadraticCurve) = transmute(lib.get(b"az_svg_quadratic_curve_delete")?);
            let az_svg_quadratic_curve_deep_copy: extern "C" fn(_:  &AzSvgQuadraticCurve) -> AzSvgQuadraticCurve = transmute(lib.get(b"az_svg_quadratic_curve_deep_copy")?);
            let az_svg_quadratic_curve_fmt_debug: extern "C" fn(_:  &AzSvgQuadraticCurve) -> AzString = transmute(lib.get(b"az_svg_quadratic_curve_fmt_debug")?);
            let az_svg_cubic_curve_delete: extern "C" fn(_:  &mut AzSvgCubicCurve) = transmute(lib.get(b"az_svg_cubic_curve_delete")?);
            let az_svg_cubic_curve_deep_copy: extern "C" fn(_:  &AzSvgCubicCurve) -> AzSvgCubicCurve = transmute(lib.get(b"az_svg_cubic_curve_deep_copy")?);
            let az_svg_cubic_curve_fmt_debug: extern "C" fn(_:  &AzSvgCubicCurve) -> AzString = transmute(lib.get(b"az_svg_cubic_curve_fmt_debug")?);
            let az_svg_rect_delete: extern "C" fn(_:  &mut AzSvgRect) = transmute(lib.get(b"az_svg_rect_delete")?);
            let az_svg_rect_deep_copy: extern "C" fn(_:  &AzSvgRect) -> AzSvgRect = transmute(lib.get(b"az_svg_rect_deep_copy")?);
            let az_svg_rect_fmt_debug: extern "C" fn(_:  &AzSvgRect) -> AzString = transmute(lib.get(b"az_svg_rect_fmt_debug")?);
            let az_tesselated_cpu_svg_node_delete: extern "C" fn(_:  &mut AzTesselatedCPUSvgNode) = transmute(lib.get(b"az_tesselated_cpu_svg_node_delete")?);
            let az_tesselated_cpu_svg_node_deep_copy: extern "C" fn(_:  &AzTesselatedCPUSvgNode) -> AzTesselatedCPUSvgNode = transmute(lib.get(b"az_tesselated_cpu_svg_node_deep_copy")?);
            let az_tesselated_cpu_svg_node_fmt_debug: extern "C" fn(_:  &AzTesselatedCPUSvgNode) -> AzString = transmute(lib.get(b"az_tesselated_cpu_svg_node_fmt_debug")?);
            let az_tesselated_gpu_svg_node_delete: extern "C" fn(_:  &mut AzTesselatedGPUSvgNode) = transmute(lib.get(b"az_tesselated_gpu_svg_node_delete")?);
            let az_tesselated_gpu_svg_node_fmt_debug: extern "C" fn(_:  &AzTesselatedGPUSvgNode) -> AzString = transmute(lib.get(b"az_tesselated_gpu_svg_node_fmt_debug")?);
            let az_svg_line_cap_delete: extern "C" fn(_:  &mut AzSvgLineCap) = transmute(lib.get(b"az_svg_line_cap_delete")?);
            let az_svg_line_cap_deep_copy: extern "C" fn(_:  &AzSvgLineCap) -> AzSvgLineCap = transmute(lib.get(b"az_svg_line_cap_deep_copy")?);
            let az_svg_line_cap_fmt_debug: extern "C" fn(_:  &AzSvgLineCap) -> AzString = transmute(lib.get(b"az_svg_line_cap_fmt_debug")?);
            let az_svg_parse_options_default: extern "C" fn() -> AzSvgParseOptions = transmute(lib.get(b"az_svg_parse_options_default")?);
            let az_svg_parse_options_delete: extern "C" fn(_:  &mut AzSvgParseOptions) = transmute(lib.get(b"az_svg_parse_options_delete")?);
            let az_svg_parse_options_deep_copy: extern "C" fn(_:  &AzSvgParseOptions) -> AzSvgParseOptions = transmute(lib.get(b"az_svg_parse_options_deep_copy")?);
            let az_svg_parse_options_fmt_debug: extern "C" fn(_:  &AzSvgParseOptions) -> AzString = transmute(lib.get(b"az_svg_parse_options_fmt_debug")?);
            let az_shape_rendering_delete: extern "C" fn(_:  &mut AzShapeRendering) = transmute(lib.get(b"az_shape_rendering_delete")?);
            let az_shape_rendering_deep_copy: extern "C" fn(_:  &AzShapeRendering) -> AzShapeRendering = transmute(lib.get(b"az_shape_rendering_deep_copy")?);
            let az_shape_rendering_fmt_debug: extern "C" fn(_:  &AzShapeRendering) -> AzString = transmute(lib.get(b"az_shape_rendering_fmt_debug")?);
            let az_text_rendering_delete: extern "C" fn(_:  &mut AzTextRendering) = transmute(lib.get(b"az_text_rendering_delete")?);
            let az_text_rendering_deep_copy: extern "C" fn(_:  &AzTextRendering) -> AzTextRendering = transmute(lib.get(b"az_text_rendering_deep_copy")?);
            let az_text_rendering_fmt_debug: extern "C" fn(_:  &AzTextRendering) -> AzString = transmute(lib.get(b"az_text_rendering_fmt_debug")?);
            let az_image_rendering_delete: extern "C" fn(_:  &mut AzImageRendering) = transmute(lib.get(b"az_image_rendering_delete")?);
            let az_image_rendering_deep_copy: extern "C" fn(_:  &AzImageRendering) -> AzImageRendering = transmute(lib.get(b"az_image_rendering_deep_copy")?);
            let az_image_rendering_fmt_debug: extern "C" fn(_:  &AzImageRendering) -> AzString = transmute(lib.get(b"az_image_rendering_fmt_debug")?);
            let az_font_database_delete: extern "C" fn(_:  &mut AzFontDatabase) = transmute(lib.get(b"az_font_database_delete")?);
            let az_font_database_deep_copy: extern "C" fn(_:  &AzFontDatabase) -> AzFontDatabase = transmute(lib.get(b"az_font_database_deep_copy")?);
            let az_font_database_fmt_debug: extern "C" fn(_:  &AzFontDatabase) -> AzString = transmute(lib.get(b"az_font_database_fmt_debug")?);
            let az_svg_render_options_default: extern "C" fn() -> AzSvgRenderOptions = transmute(lib.get(b"az_svg_render_options_default")?);
            let az_svg_render_options_delete: extern "C" fn(_:  &mut AzSvgRenderOptions) = transmute(lib.get(b"az_svg_render_options_delete")?);
            let az_svg_render_options_deep_copy: extern "C" fn(_:  &AzSvgRenderOptions) -> AzSvgRenderOptions = transmute(lib.get(b"az_svg_render_options_deep_copy")?);
            let az_svg_render_options_fmt_debug: extern "C" fn(_:  &AzSvgRenderOptions) -> AzString = transmute(lib.get(b"az_svg_render_options_fmt_debug")?);
            let az_svg_fit_to_delete: extern "C" fn(_:  &mut AzSvgFitTo) = transmute(lib.get(b"az_svg_fit_to_delete")?);
            let az_svg_fit_to_deep_copy: extern "C" fn(_:  &AzSvgFitTo) -> AzSvgFitTo = transmute(lib.get(b"az_svg_fit_to_deep_copy")?);
            let az_svg_fit_to_fmt_debug: extern "C" fn(_:  &AzSvgFitTo) -> AzString = transmute(lib.get(b"az_svg_fit_to_fmt_debug")?);
            let az_svg_parse: extern "C" fn(_:  AzU8VecRef, _:  AzSvgParseOptions) -> AzResultSvgSvgParseError = transmute(lib.get(b"az_svg_parse")?);
            let az_svg_delete: extern "C" fn(_:  &mut AzSvg) = transmute(lib.get(b"az_svg_delete")?);
            let az_svg_deep_copy: extern "C" fn(_:  &AzSvg) -> AzSvg = transmute(lib.get(b"az_svg_deep_copy")?);
            let az_svg_fmt_debug: extern "C" fn(_:  &AzSvg) -> AzString = transmute(lib.get(b"az_svg_fmt_debug")?);
            let az_svg_xml_node_delete: extern "C" fn(_:  &mut AzSvgXmlNode) = transmute(lib.get(b"az_svg_xml_node_delete")?);
            let az_svg_xml_node_deep_copy: extern "C" fn(_:  &AzSvgXmlNode) -> AzSvgXmlNode = transmute(lib.get(b"az_svg_xml_node_deep_copy")?);
            let az_svg_xml_node_fmt_debug: extern "C" fn(_:  &AzSvgXmlNode) -> AzString = transmute(lib.get(b"az_svg_xml_node_fmt_debug")?);
            let az_svg_line_join_delete: extern "C" fn(_:  &mut AzSvgLineJoin) = transmute(lib.get(b"az_svg_line_join_delete")?);
            let az_svg_line_join_deep_copy: extern "C" fn(_:  &AzSvgLineJoin) -> AzSvgLineJoin = transmute(lib.get(b"az_svg_line_join_deep_copy")?);
            let az_svg_line_join_fmt_debug: extern "C" fn(_:  &AzSvgLineJoin) -> AzString = transmute(lib.get(b"az_svg_line_join_fmt_debug")?);
            let az_svg_dash_pattern_delete: extern "C" fn(_:  &mut AzSvgDashPattern) = transmute(lib.get(b"az_svg_dash_pattern_delete")?);
            let az_svg_dash_pattern_deep_copy: extern "C" fn(_:  &AzSvgDashPattern) -> AzSvgDashPattern = transmute(lib.get(b"az_svg_dash_pattern_deep_copy")?);
            let az_svg_dash_pattern_fmt_debug: extern "C" fn(_:  &AzSvgDashPattern) -> AzString = transmute(lib.get(b"az_svg_dash_pattern_fmt_debug")?);
            let az_svg_style_delete: extern "C" fn(_:  &mut AzSvgStyle) = transmute(lib.get(b"az_svg_style_delete")?);
            let az_svg_style_deep_copy: extern "C" fn(_:  &AzSvgStyle) -> AzSvgStyle = transmute(lib.get(b"az_svg_style_deep_copy")?);
            let az_svg_style_fmt_debug: extern "C" fn(_:  &AzSvgStyle) -> AzString = transmute(lib.get(b"az_svg_style_fmt_debug")?);
            let az_svg_fill_style_delete: extern "C" fn(_:  &mut AzSvgFillStyle) = transmute(lib.get(b"az_svg_fill_style_delete")?);
            let az_svg_fill_style_deep_copy: extern "C" fn(_:  &AzSvgFillStyle) -> AzSvgFillStyle = transmute(lib.get(b"az_svg_fill_style_deep_copy")?);
            let az_svg_fill_style_fmt_debug: extern "C" fn(_:  &AzSvgFillStyle) -> AzString = transmute(lib.get(b"az_svg_fill_style_fmt_debug")?);
            let az_svg_stroke_style_delete: extern "C" fn(_:  &mut AzSvgStrokeStyle) = transmute(lib.get(b"az_svg_stroke_style_delete")?);
            let az_svg_stroke_style_deep_copy: extern "C" fn(_:  &AzSvgStrokeStyle) -> AzSvgStrokeStyle = transmute(lib.get(b"az_svg_stroke_style_deep_copy")?);
            let az_svg_stroke_style_fmt_debug: extern "C" fn(_:  &AzSvgStrokeStyle) -> AzString = transmute(lib.get(b"az_svg_stroke_style_fmt_debug")?);
            let az_svg_node_id_delete: extern "C" fn(_:  &mut AzSvgNodeId) = transmute(lib.get(b"az_svg_node_id_delete")?);
            let az_svg_node_id_deep_copy: extern "C" fn(_:  &AzSvgNodeId) -> AzSvgNodeId = transmute(lib.get(b"az_svg_node_id_deep_copy")?);
            let az_svg_node_id_fmt_debug: extern "C" fn(_:  &AzSvgNodeId) -> AzString = transmute(lib.get(b"az_svg_node_id_fmt_debug")?);
            let az_drop_check_ptr_ptr_delete: extern "C" fn(_:  &mut AzDropCheckPtrPtr) = transmute(lib.get(b"az_drop_check_ptr_ptr_delete")?);
            let az_drop_check_ptr_ptr_fmt_debug: extern "C" fn(_:  &AzDropCheckPtrPtr) -> AzString = transmute(lib.get(b"az_drop_check_ptr_ptr_fmt_debug")?);
            let az_arc_mutex_ref_any_ptr_delete: extern "C" fn(_:  &mut AzArcMutexRefAnyPtr) = transmute(lib.get(b"az_arc_mutex_ref_any_ptr_delete")?);
            let az_arc_mutex_ref_any_ptr_fmt_debug: extern "C" fn(_:  &AzArcMutexRefAnyPtr) -> AzString = transmute(lib.get(b"az_arc_mutex_ref_any_ptr_fmt_debug")?);
            let az_timer_delete: extern "C" fn(_:  &mut AzTimer) = transmute(lib.get(b"az_timer_delete")?);
            let az_timer_deep_copy: extern "C" fn(_:  &AzTimer) -> AzTimer = transmute(lib.get(b"az_timer_deep_copy")?);
            let az_timer_fmt_debug: extern "C" fn(_:  &AzTimer) -> AzString = transmute(lib.get(b"az_timer_fmt_debug")?);
            let az_task_ptr_new: extern "C" fn(_:  AzArcMutexRefAnyPtr, _:  AzTaskCallbackType) -> AzTaskPtr = transmute(lib.get(b"az_task_ptr_new")?);
            let az_task_ptr_then: extern "C" fn(_:  AzTaskPtr, _:  AzTimer) -> AzTaskPtr = transmute(lib.get(b"az_task_ptr_then")?);
            let az_task_ptr_delete: extern "C" fn(_:  &mut AzTaskPtr) = transmute(lib.get(b"az_task_ptr_delete")?);
            let az_task_ptr_fmt_debug: extern "C" fn(_:  &AzTaskPtr) -> AzString = transmute(lib.get(b"az_task_ptr_fmt_debug")?);
            let az_thread_ptr_new: extern "C" fn(_:  AzRefAny, _:  AzThreadCallbackType) -> AzThreadPtr = transmute(lib.get(b"az_thread_ptr_new")?);
            let az_thread_ptr_block: extern "C" fn(_:  AzThreadPtr) -> AzResultRefAnyBlockError = transmute(lib.get(b"az_thread_ptr_block")?);
            let az_thread_ptr_delete: extern "C" fn(_:  &mut AzThreadPtr) = transmute(lib.get(b"az_thread_ptr_delete")?);
            let az_thread_ptr_fmt_debug: extern "C" fn(_:  &AzThreadPtr) -> AzString = transmute(lib.get(b"az_thread_ptr_fmt_debug")?);
            let az_drop_check_ptr_delete: extern "C" fn(_:  &mut AzDropCheckPtr) = transmute(lib.get(b"az_drop_check_ptr_delete")?);
            let az_drop_check_ptr_fmt_debug: extern "C" fn(_:  &AzDropCheckPtr) -> AzString = transmute(lib.get(b"az_drop_check_ptr_fmt_debug")?);
            let az_timer_id_delete: extern "C" fn(_:  &mut AzTimerId) = transmute(lib.get(b"az_timer_id_delete")?);
            let az_timer_id_deep_copy: extern "C" fn(_:  &AzTimerId) -> AzTimerId = transmute(lib.get(b"az_timer_id_deep_copy")?);
            let az_timer_id_fmt_debug: extern "C" fn(_:  &AzTimerId) -> AzString = transmute(lib.get(b"az_timer_id_fmt_debug")?);
            let az_terminate_timer_delete: extern "C" fn(_:  &mut AzTerminateTimer) = transmute(lib.get(b"az_terminate_timer_delete")?);
            let az_terminate_timer_deep_copy: extern "C" fn(_:  &AzTerminateTimer) -> AzTerminateTimer = transmute(lib.get(b"az_terminate_timer_deep_copy")?);
            let az_terminate_timer_fmt_debug: extern "C" fn(_:  &AzTerminateTimer) -> AzString = transmute(lib.get(b"az_terminate_timer_fmt_debug")?);
            let az_block_error_delete: extern "C" fn(_:  &mut AzBlockError) = transmute(lib.get(b"az_block_error_delete")?);
            let az_block_error_deep_copy: extern "C" fn(_:  &AzBlockError) -> AzBlockError = transmute(lib.get(b"az_block_error_deep_copy")?);
            let az_block_error_fmt_debug: extern "C" fn(_:  &AzBlockError) -> AzString = transmute(lib.get(b"az_block_error_fmt_debug")?);
            let az_task_bar_icon_delete: extern "C" fn(_:  &mut AzTaskBarIcon) = transmute(lib.get(b"az_task_bar_icon_delete")?);
            let az_task_bar_icon_deep_copy: extern "C" fn(_:  &AzTaskBarIcon) -> AzTaskBarIcon = transmute(lib.get(b"az_task_bar_icon_deep_copy")?);
            let az_task_bar_icon_fmt_debug: extern "C" fn(_:  &AzTaskBarIcon) -> AzString = transmute(lib.get(b"az_task_bar_icon_fmt_debug")?);
            let az_x_window_type_delete: extern "C" fn(_:  &mut AzXWindowType) = transmute(lib.get(b"az_x_window_type_delete")?);
            let az_x_window_type_deep_copy: extern "C" fn(_:  &AzXWindowType) -> AzXWindowType = transmute(lib.get(b"az_x_window_type_deep_copy")?);
            let az_x_window_type_fmt_debug: extern "C" fn(_:  &AzXWindowType) -> AzString = transmute(lib.get(b"az_x_window_type_fmt_debug")?);
            let az_physical_position_i32_delete: extern "C" fn(_:  &mut AzPhysicalPositionI32) = transmute(lib.get(b"az_physical_position_i32_delete")?);
            let az_physical_position_i32_deep_copy: extern "C" fn(_:  &AzPhysicalPositionI32) -> AzPhysicalPositionI32 = transmute(lib.get(b"az_physical_position_i32_deep_copy")?);
            let az_physical_position_i32_fmt_debug: extern "C" fn(_:  &AzPhysicalPositionI32) -> AzString = transmute(lib.get(b"az_physical_position_i32_fmt_debug")?);
            let az_physical_position_i32_partial_eq: extern "C" fn(_:  &AzPhysicalPositionI32, _:  &AzPhysicalPositionI32) -> bool = transmute(lib.get(b"az_physical_position_i32_partial_eq")?);
            let az_physical_position_i32_partial_cmp: extern "C" fn(_:  &AzPhysicalPositionI32, _:  &AzPhysicalPositionI32) -> u8 = transmute(lib.get(b"az_physical_position_i32_partial_cmp")?);
            let az_physical_position_i32_cmp: extern "C" fn(_:  &AzPhysicalPositionI32, _:  &AzPhysicalPositionI32) -> u8 = transmute(lib.get(b"az_physical_position_i32_cmp")?);
            let az_physical_position_i32_hash: extern "C" fn(_:  &AzPhysicalPositionI32) -> u64 = transmute(lib.get(b"az_physical_position_i32_hash")?);
            let az_physical_size_u32_delete: extern "C" fn(_:  &mut AzPhysicalSizeU32) = transmute(lib.get(b"az_physical_size_u32_delete")?);
            let az_physical_size_u32_deep_copy: extern "C" fn(_:  &AzPhysicalSizeU32) -> AzPhysicalSizeU32 = transmute(lib.get(b"az_physical_size_u32_deep_copy")?);
            let az_physical_size_u32_fmt_debug: extern "C" fn(_:  &AzPhysicalSizeU32) -> AzString = transmute(lib.get(b"az_physical_size_u32_fmt_debug")?);
            let az_physical_size_u32_partial_eq: extern "C" fn(_:  &AzPhysicalSizeU32, _:  &AzPhysicalSizeU32) -> bool = transmute(lib.get(b"az_physical_size_u32_partial_eq")?);
            let az_physical_size_u32_partial_cmp: extern "C" fn(_:  &AzPhysicalSizeU32, _:  &AzPhysicalSizeU32) -> u8 = transmute(lib.get(b"az_physical_size_u32_partial_cmp")?);
            let az_physical_size_u32_cmp: extern "C" fn(_:  &AzPhysicalSizeU32, _:  &AzPhysicalSizeU32) -> u8 = transmute(lib.get(b"az_physical_size_u32_cmp")?);
            let az_physical_size_u32_hash: extern "C" fn(_:  &AzPhysicalSizeU32) -> u64 = transmute(lib.get(b"az_physical_size_u32_hash")?);
            let az_logical_position_delete: extern "C" fn(_:  &mut AzLogicalPosition) = transmute(lib.get(b"az_logical_position_delete")?);
            let az_logical_position_deep_copy: extern "C" fn(_:  &AzLogicalPosition) -> AzLogicalPosition = transmute(lib.get(b"az_logical_position_deep_copy")?);
            let az_logical_position_fmt_debug: extern "C" fn(_:  &AzLogicalPosition) -> AzString = transmute(lib.get(b"az_logical_position_fmt_debug")?);
            let az_logical_rect_delete: extern "C" fn(_:  &mut AzLogicalRect) = transmute(lib.get(b"az_logical_rect_delete")?);
            let az_logical_rect_deep_copy: extern "C" fn(_:  &AzLogicalRect) -> AzLogicalRect = transmute(lib.get(b"az_logical_rect_deep_copy")?);
            let az_logical_rect_fmt_debug: extern "C" fn(_:  &AzLogicalRect) -> AzString = transmute(lib.get(b"az_logical_rect_fmt_debug")?);
            let az_icon_key_delete: extern "C" fn(_:  &mut AzIconKey) = transmute(lib.get(b"az_icon_key_delete")?);
            let az_icon_key_deep_copy: extern "C" fn(_:  &AzIconKey) -> AzIconKey = transmute(lib.get(b"az_icon_key_deep_copy")?);
            let az_icon_key_fmt_debug: extern "C" fn(_:  &AzIconKey) -> AzString = transmute(lib.get(b"az_icon_key_fmt_debug")?);
            let az_small_window_icon_bytes_delete: extern "C" fn(_:  &mut AzSmallWindowIconBytes) = transmute(lib.get(b"az_small_window_icon_bytes_delete")?);
            let az_small_window_icon_bytes_deep_copy: extern "C" fn(_:  &AzSmallWindowIconBytes) -> AzSmallWindowIconBytes = transmute(lib.get(b"az_small_window_icon_bytes_deep_copy")?);
            let az_small_window_icon_bytes_fmt_debug: extern "C" fn(_:  &AzSmallWindowIconBytes) -> AzString = transmute(lib.get(b"az_small_window_icon_bytes_fmt_debug")?);
            let az_large_window_icon_bytes_delete: extern "C" fn(_:  &mut AzLargeWindowIconBytes) = transmute(lib.get(b"az_large_window_icon_bytes_delete")?);
            let az_large_window_icon_bytes_deep_copy: extern "C" fn(_:  &AzLargeWindowIconBytes) -> AzLargeWindowIconBytes = transmute(lib.get(b"az_large_window_icon_bytes_deep_copy")?);
            let az_large_window_icon_bytes_fmt_debug: extern "C" fn(_:  &AzLargeWindowIconBytes) -> AzString = transmute(lib.get(b"az_large_window_icon_bytes_fmt_debug")?);
            let az_window_icon_delete: extern "C" fn(_:  &mut AzWindowIcon) = transmute(lib.get(b"az_window_icon_delete")?);
            let az_window_icon_deep_copy: extern "C" fn(_:  &AzWindowIcon) -> AzWindowIcon = transmute(lib.get(b"az_window_icon_deep_copy")?);
            let az_window_icon_fmt_debug: extern "C" fn(_:  &AzWindowIcon) -> AzString = transmute(lib.get(b"az_window_icon_fmt_debug")?);
            let az_virtual_key_code_delete: extern "C" fn(_:  &mut AzVirtualKeyCode) = transmute(lib.get(b"az_virtual_key_code_delete")?);
            let az_virtual_key_code_deep_copy: extern "C" fn(_:  &AzVirtualKeyCode) -> AzVirtualKeyCode = transmute(lib.get(b"az_virtual_key_code_deep_copy")?);
            let az_virtual_key_code_fmt_debug: extern "C" fn(_:  &AzVirtualKeyCode) -> AzString = transmute(lib.get(b"az_virtual_key_code_fmt_debug")?);
            let az_accelerator_key_delete: extern "C" fn(_:  &mut AzAcceleratorKey) = transmute(lib.get(b"az_accelerator_key_delete")?);
            let az_accelerator_key_deep_copy: extern "C" fn(_:  &AzAcceleratorKey) -> AzAcceleratorKey = transmute(lib.get(b"az_accelerator_key_deep_copy")?);
            let az_accelerator_key_fmt_debug: extern "C" fn(_:  &AzAcceleratorKey) -> AzString = transmute(lib.get(b"az_accelerator_key_fmt_debug")?);
            let az_window_size_delete: extern "C" fn(_:  &mut AzWindowSize) = transmute(lib.get(b"az_window_size_delete")?);
            let az_window_size_deep_copy: extern "C" fn(_:  &AzWindowSize) -> AzWindowSize = transmute(lib.get(b"az_window_size_deep_copy")?);
            let az_window_size_fmt_debug: extern "C" fn(_:  &AzWindowSize) -> AzString = transmute(lib.get(b"az_window_size_fmt_debug")?);
            let az_window_flags_delete: extern "C" fn(_:  &mut AzWindowFlags) = transmute(lib.get(b"az_window_flags_delete")?);
            let az_window_flags_deep_copy: extern "C" fn(_:  &AzWindowFlags) -> AzWindowFlags = transmute(lib.get(b"az_window_flags_deep_copy")?);
            let az_window_flags_fmt_debug: extern "C" fn(_:  &AzWindowFlags) -> AzString = transmute(lib.get(b"az_window_flags_fmt_debug")?);
            let az_debug_state_delete: extern "C" fn(_:  &mut AzDebugState) = transmute(lib.get(b"az_debug_state_delete")?);
            let az_debug_state_deep_copy: extern "C" fn(_:  &AzDebugState) -> AzDebugState = transmute(lib.get(b"az_debug_state_deep_copy")?);
            let az_debug_state_fmt_debug: extern "C" fn(_:  &AzDebugState) -> AzString = transmute(lib.get(b"az_debug_state_fmt_debug")?);
            let az_keyboard_state_delete: extern "C" fn(_:  &mut AzKeyboardState) = transmute(lib.get(b"az_keyboard_state_delete")?);
            let az_keyboard_state_deep_copy: extern "C" fn(_:  &AzKeyboardState) -> AzKeyboardState = transmute(lib.get(b"az_keyboard_state_deep_copy")?);
            let az_keyboard_state_fmt_debug: extern "C" fn(_:  &AzKeyboardState) -> AzString = transmute(lib.get(b"az_keyboard_state_fmt_debug")?);
            let az_mouse_cursor_type_delete: extern "C" fn(_:  &mut AzMouseCursorType) = transmute(lib.get(b"az_mouse_cursor_type_delete")?);
            let az_mouse_cursor_type_deep_copy: extern "C" fn(_:  &AzMouseCursorType) -> AzMouseCursorType = transmute(lib.get(b"az_mouse_cursor_type_deep_copy")?);
            let az_mouse_cursor_type_fmt_debug: extern "C" fn(_:  &AzMouseCursorType) -> AzString = transmute(lib.get(b"az_mouse_cursor_type_fmt_debug")?);
            let az_cursor_position_delete: extern "C" fn(_:  &mut AzCursorPosition) = transmute(lib.get(b"az_cursor_position_delete")?);
            let az_cursor_position_deep_copy: extern "C" fn(_:  &AzCursorPosition) -> AzCursorPosition = transmute(lib.get(b"az_cursor_position_deep_copy")?);
            let az_cursor_position_fmt_debug: extern "C" fn(_:  &AzCursorPosition) -> AzString = transmute(lib.get(b"az_cursor_position_fmt_debug")?);
            let az_mouse_state_delete: extern "C" fn(_:  &mut AzMouseState) = transmute(lib.get(b"az_mouse_state_delete")?);
            let az_mouse_state_deep_copy: extern "C" fn(_:  &AzMouseState) -> AzMouseState = transmute(lib.get(b"az_mouse_state_deep_copy")?);
            let az_mouse_state_fmt_debug: extern "C" fn(_:  &AzMouseState) -> AzString = transmute(lib.get(b"az_mouse_state_fmt_debug")?);
            let az_platform_specific_options_delete: extern "C" fn(_:  &mut AzPlatformSpecificOptions) = transmute(lib.get(b"az_platform_specific_options_delete")?);
            let az_platform_specific_options_deep_copy: extern "C" fn(_:  &AzPlatformSpecificOptions) -> AzPlatformSpecificOptions = transmute(lib.get(b"az_platform_specific_options_deep_copy")?);
            let az_platform_specific_options_fmt_debug: extern "C" fn(_:  &AzPlatformSpecificOptions) -> AzString = transmute(lib.get(b"az_platform_specific_options_fmt_debug")?);
            let az_windows_window_options_delete: extern "C" fn(_:  &mut AzWindowsWindowOptions) = transmute(lib.get(b"az_windows_window_options_delete")?);
            let az_windows_window_options_deep_copy: extern "C" fn(_:  &AzWindowsWindowOptions) -> AzWindowsWindowOptions = transmute(lib.get(b"az_windows_window_options_deep_copy")?);
            let az_windows_window_options_fmt_debug: extern "C" fn(_:  &AzWindowsWindowOptions) -> AzString = transmute(lib.get(b"az_windows_window_options_fmt_debug")?);
            let az_wayland_theme_delete: extern "C" fn(_:  &mut AzWaylandTheme) = transmute(lib.get(b"az_wayland_theme_delete")?);
            let az_wayland_theme_deep_copy: extern "C" fn(_:  &AzWaylandTheme) -> AzWaylandTheme = transmute(lib.get(b"az_wayland_theme_deep_copy")?);
            let az_wayland_theme_fmt_debug: extern "C" fn(_:  &AzWaylandTheme) -> AzString = transmute(lib.get(b"az_wayland_theme_fmt_debug")?);
            let az_renderer_type_delete: extern "C" fn(_:  &mut AzRendererType) = transmute(lib.get(b"az_renderer_type_delete")?);
            let az_renderer_type_deep_copy: extern "C" fn(_:  &AzRendererType) -> AzRendererType = transmute(lib.get(b"az_renderer_type_deep_copy")?);
            let az_renderer_type_fmt_debug: extern "C" fn(_:  &AzRendererType) -> AzString = transmute(lib.get(b"az_renderer_type_fmt_debug")?);
            let az_string_pair_delete: extern "C" fn(_:  &mut AzStringPair) = transmute(lib.get(b"az_string_pair_delete")?);
            let az_string_pair_deep_copy: extern "C" fn(_:  &AzStringPair) -> AzStringPair = transmute(lib.get(b"az_string_pair_deep_copy")?);
            let az_string_pair_fmt_debug: extern "C" fn(_:  &AzStringPair) -> AzString = transmute(lib.get(b"az_string_pair_fmt_debug")?);
            let az_linux_window_options_delete: extern "C" fn(_:  &mut AzLinuxWindowOptions) = transmute(lib.get(b"az_linux_window_options_delete")?);
            let az_linux_window_options_deep_copy: extern "C" fn(_:  &AzLinuxWindowOptions) -> AzLinuxWindowOptions = transmute(lib.get(b"az_linux_window_options_deep_copy")?);
            let az_linux_window_options_fmt_debug: extern "C" fn(_:  &AzLinuxWindowOptions) -> AzString = transmute(lib.get(b"az_linux_window_options_fmt_debug")?);
            let az_mac_window_options_delete: extern "C" fn(_:  &mut AzMacWindowOptions) = transmute(lib.get(b"az_mac_window_options_delete")?);
            let az_mac_window_options_deep_copy: extern "C" fn(_:  &AzMacWindowOptions) -> AzMacWindowOptions = transmute(lib.get(b"az_mac_window_options_deep_copy")?);
            let az_mac_window_options_fmt_debug: extern "C" fn(_:  &AzMacWindowOptions) -> AzString = transmute(lib.get(b"az_mac_window_options_fmt_debug")?);
            let az_wasm_window_options_delete: extern "C" fn(_:  &mut AzWasmWindowOptions) = transmute(lib.get(b"az_wasm_window_options_delete")?);
            let az_wasm_window_options_deep_copy: extern "C" fn(_:  &AzWasmWindowOptions) -> AzWasmWindowOptions = transmute(lib.get(b"az_wasm_window_options_deep_copy")?);
            let az_wasm_window_options_fmt_debug: extern "C" fn(_:  &AzWasmWindowOptions) -> AzString = transmute(lib.get(b"az_wasm_window_options_fmt_debug")?);
            let az_full_screen_mode_delete: extern "C" fn(_:  &mut AzFullScreenMode) = transmute(lib.get(b"az_full_screen_mode_delete")?);
            let az_full_screen_mode_deep_copy: extern "C" fn(_:  &AzFullScreenMode) -> AzFullScreenMode = transmute(lib.get(b"az_full_screen_mode_deep_copy")?);
            let az_full_screen_mode_fmt_debug: extern "C" fn(_:  &AzFullScreenMode) -> AzString = transmute(lib.get(b"az_full_screen_mode_fmt_debug")?);
            let az_window_state_new: extern "C" fn(_:  AzLayoutCallbackType, _:  AzCss) -> AzWindowState = transmute(lib.get(b"az_window_state_new")?);
            let az_window_state_delete: extern "C" fn(_:  &mut AzWindowState) = transmute(lib.get(b"az_window_state_delete")?);
            let az_window_state_deep_copy: extern "C" fn(_:  &AzWindowState) -> AzWindowState = transmute(lib.get(b"az_window_state_deep_copy")?);
            let az_window_state_fmt_debug: extern "C" fn(_:  &AzWindowState) -> AzString = transmute(lib.get(b"az_window_state_fmt_debug")?);
            let az_logical_size_delete: extern "C" fn(_:  &mut AzLogicalSize) = transmute(lib.get(b"az_logical_size_delete")?);
            let az_logical_size_deep_copy: extern "C" fn(_:  &AzLogicalSize) -> AzLogicalSize = transmute(lib.get(b"az_logical_size_deep_copy")?);
            let az_logical_size_fmt_debug: extern "C" fn(_:  &AzLogicalSize) -> AzString = transmute(lib.get(b"az_logical_size_fmt_debug")?);
            let az_hot_reload_options_delete: extern "C" fn(_:  &mut AzHotReloadOptions) = transmute(lib.get(b"az_hot_reload_options_delete")?);
            let az_hot_reload_options_deep_copy: extern "C" fn(_:  &AzHotReloadOptions) -> AzHotReloadOptions = transmute(lib.get(b"az_hot_reload_options_deep_copy")?);
            let az_hot_reload_options_fmt_debug: extern "C" fn(_:  &AzHotReloadOptions) -> AzString = transmute(lib.get(b"az_hot_reload_options_fmt_debug")?);
            let az_window_create_options_new: extern "C" fn(_:  AzLayoutCallbackType, _:  AzCss) -> AzWindowCreateOptions = transmute(lib.get(b"az_window_create_options_new")?);
            let az_window_create_options_new_hot_reload: extern "C" fn(_:  AzLayoutCallbackType, _:  AzHotReloadOptions) -> AzWindowCreateOptions = transmute(lib.get(b"az_window_create_options_new_hot_reload")?);
            let az_window_create_options_delete: extern "C" fn(_:  &mut AzWindowCreateOptions) = transmute(lib.get(b"az_window_create_options_delete")?);
            let az_window_create_options_deep_copy: extern "C" fn(_:  &AzWindowCreateOptions) -> AzWindowCreateOptions = transmute(lib.get(b"az_window_create_options_deep_copy")?);
            let az_window_create_options_fmt_debug: extern "C" fn(_:  &AzWindowCreateOptions) -> AzString = transmute(lib.get(b"az_window_create_options_fmt_debug")?);
            Some(AzulDll {
                lib: lib,
                az_string_from_utf8_unchecked,
                az_string_from_utf8_lossy,
                az_string_into_bytes,
                az_string_delete,
                az_string_deep_copy,
                az_string_fmt_debug,
                az_string_partial_eq,
                az_string_partial_cmp,
                az_string_cmp,
                az_string_hash,
                az_css_property_vec_new,
                az_css_property_vec_with_capacity,
                az_css_property_vec_copy_from,
                az_css_property_vec_delete,
                az_css_property_vec_deep_copy,
                az_css_property_vec_fmt_debug,
                az_svg_multi_polygon_vec_new,
                az_svg_multi_polygon_vec_with_capacity,
                az_svg_multi_polygon_vec_copy_from,
                az_svg_multi_polygon_vec_delete,
                az_svg_multi_polygon_vec_deep_copy,
                az_svg_multi_polygon_vec_fmt_debug,
                az_svg_path_vec_new,
                az_svg_path_vec_with_capacity,
                az_svg_path_vec_copy_from,
                az_svg_path_vec_delete,
                az_svg_path_vec_deep_copy,
                az_svg_path_vec_fmt_debug,
                az_vertex_attribute_vec_new,
                az_vertex_attribute_vec_with_capacity,
                az_vertex_attribute_vec_copy_from,
                az_vertex_attribute_vec_delete,
                az_vertex_attribute_vec_deep_copy,
                az_vertex_attribute_vec_fmt_debug,
                az_svg_path_element_vec_new,
                az_svg_path_element_vec_with_capacity,
                az_svg_path_element_vec_copy_from,
                az_svg_path_element_vec_delete,
                az_svg_path_element_vec_deep_copy,
                az_svg_path_element_vec_fmt_debug,
                az_svg_vertex_vec_new,
                az_svg_vertex_vec_with_capacity,
                az_svg_vertex_vec_copy_from,
                az_svg_vertex_vec_delete,
                az_svg_vertex_vec_deep_copy,
                az_svg_vertex_vec_fmt_debug,
                az_u32_vec_new,
                az_u32_vec_with_capacity,
                az_u32_vec_copy_from,
                az_u32_vec_delete,
                az_u32_vec_deep_copy,
                az_u32_vec_fmt_debug,
                az_x_window_type_vec_new,
                az_x_window_type_vec_with_capacity,
                az_x_window_type_vec_copy_from,
                az_x_window_type_vec_delete,
                az_x_window_type_vec_deep_copy,
                az_x_window_type_vec_fmt_debug,
                az_virtual_key_code_vec_new,
                az_virtual_key_code_vec_with_capacity,
                az_virtual_key_code_vec_copy_from,
                az_virtual_key_code_vec_delete,
                az_virtual_key_code_vec_deep_copy,
                az_virtual_key_code_vec_fmt_debug,
                az_scan_code_vec_new,
                az_scan_code_vec_with_capacity,
                az_scan_code_vec_copy_from,
                az_scan_code_vec_delete,
                az_scan_code_vec_deep_copy,
                az_scan_code_vec_fmt_debug,
                az_css_declaration_vec_new,
                az_css_declaration_vec_with_capacity,
                az_css_declaration_vec_copy_from,
                az_css_declaration_vec_delete,
                az_css_declaration_vec_deep_copy,
                az_css_declaration_vec_fmt_debug,
                az_css_path_selector_vec_new,
                az_css_path_selector_vec_with_capacity,
                az_css_path_selector_vec_copy_from,
                az_css_path_selector_vec_delete,
                az_css_path_selector_vec_deep_copy,
                az_css_path_selector_vec_fmt_debug,
                az_stylesheet_vec_new,
                az_stylesheet_vec_with_capacity,
                az_stylesheet_vec_copy_from,
                az_stylesheet_vec_delete,
                az_stylesheet_vec_deep_copy,
                az_stylesheet_vec_fmt_debug,
                az_css_rule_block_vec_new,
                az_css_rule_block_vec_with_capacity,
                az_css_rule_block_vec_copy_from,
                az_css_rule_block_vec_delete,
                az_css_rule_block_vec_deep_copy,
                az_css_rule_block_vec_fmt_debug,
                az_u8_vec_new,
                az_u8_vec_with_capacity,
                az_u8_vec_copy_from,
                az_u8_vec_delete,
                az_u8_vec_deep_copy,
                az_u8_vec_fmt_debug,
                az_callback_data_vec_new,
                az_callback_data_vec_with_capacity,
                az_callback_data_vec_copy_from,
                az_callback_data_vec_delete,
                az_callback_data_vec_deep_copy,
                az_callback_data_vec_fmt_debug,
                az_debug_message_vec_new,
                az_debug_message_vec_with_capacity,
                az_debug_message_vec_copy_from,
                az_debug_message_vec_delete,
                az_debug_message_vec_deep_copy,
                az_debug_message_vec_fmt_debug,
                az_g_luint_vec_new,
                az_g_luint_vec_with_capacity,
                az_g_luint_vec_copy_from,
                az_g_luint_vec_delete,
                az_g_luint_vec_deep_copy,
                az_g_luint_vec_fmt_debug,
                az_g_lint_vec_new,
                az_g_lint_vec_with_capacity,
                az_g_lint_vec_copy_from,
                az_g_lint_vec_delete,
                az_g_lint_vec_deep_copy,
                az_g_lint_vec_fmt_debug,
                az_dom_vec_new,
                az_dom_vec_with_capacity,
                az_dom_vec_copy_from,
                az_dom_vec_delete,
                az_dom_vec_deep_copy,
                az_dom_vec_fmt_debug,
                az_string_vec_new,
                az_string_vec_with_capacity,
                az_string_vec_copy_from,
                az_string_vec_delete,
                az_string_vec_deep_copy,
                az_string_vec_fmt_debug,
                az_string_pair_vec_new,
                az_string_pair_vec_with_capacity,
                az_string_pair_vec_copy_from,
                az_string_pair_vec_delete,
                az_string_pair_vec_deep_copy,
                az_string_pair_vec_fmt_debug,
                az_gradient_stop_pre_vec_new,
                az_gradient_stop_pre_vec_with_capacity,
                az_gradient_stop_pre_vec_copy_from,
                az_gradient_stop_pre_vec_delete,
                az_gradient_stop_pre_vec_deep_copy,
                az_gradient_stop_pre_vec_fmt_debug,
                az_cascaded_css_property_with_source_vec_new,
                az_cascaded_css_property_with_source_vec_with_capacity,
                az_cascaded_css_property_with_source_vec_copy_from,
                az_cascaded_css_property_with_source_vec_delete,
                az_cascaded_css_property_with_source_vec_deep_copy,
                az_cascaded_css_property_with_source_vec_fmt_debug,
                az_node_id_vec_new,
                az_node_id_vec_with_capacity,
                az_node_id_vec_copy_from,
                az_node_id_vec_delete,
                az_node_id_vec_deep_copy,
                az_node_id_vec_fmt_debug,
                az_node_vec_new,
                az_node_vec_with_capacity,
                az_node_vec_copy_from,
                az_node_vec_delete,
                az_node_vec_deep_copy,
                az_node_vec_fmt_debug,
                az_styled_node_vec_new,
                az_styled_node_vec_with_capacity,
                az_styled_node_vec_copy_from,
                az_styled_node_vec_delete,
                az_styled_node_vec_deep_copy,
                az_styled_node_vec_fmt_debug,
                az_tag_ids_to_node_ids_mapping_vec_new,
                az_tag_ids_to_node_ids_mapping_vec_with_capacity,
                az_tag_ids_to_node_ids_mapping_vec_copy_from,
                az_tag_ids_to_node_ids_mapping_vec_delete,
                az_tag_ids_to_node_ids_mapping_vec_deep_copy,
                az_tag_ids_to_node_ids_mapping_vec_fmt_debug,
                az_parent_with_node_depth_vec_new,
                az_parent_with_node_depth_vec_with_capacity,
                az_parent_with_node_depth_vec_copy_from,
                az_parent_with_node_depth_vec_delete,
                az_parent_with_node_depth_vec_deep_copy,
                az_parent_with_node_depth_vec_fmt_debug,
                az_node_data_vec_new,
                az_node_data_vec_with_capacity,
                az_node_data_vec_copy_from,
                az_node_data_vec_delete,
                az_node_data_vec_deep_copy,
                az_node_data_vec_fmt_debug,
                az_option_node_id_delete,
                az_option_node_id_deep_copy,
                az_option_node_id_fmt_debug,
                az_option_dom_node_id_delete,
                az_option_dom_node_id_deep_copy,
                az_option_dom_node_id_fmt_debug,
                az_option_color_u_delete,
                az_option_color_u_deep_copy,
                az_option_color_u_fmt_debug,
                az_option_raw_image_delete,
                az_option_raw_image_deep_copy,
                az_option_raw_image_fmt_debug,
                az_option_svg_dash_pattern_delete,
                az_option_svg_dash_pattern_deep_copy,
                az_option_svg_dash_pattern_fmt_debug,
                az_option_wayland_theme_delete,
                az_option_wayland_theme_deep_copy,
                az_option_wayland_theme_fmt_debug,
                az_option_task_bar_icon_delete,
                az_option_task_bar_icon_deep_copy,
                az_option_task_bar_icon_fmt_debug,
                az_option_hwnd_handle_delete,
                az_option_hwnd_handle_deep_copy,
                az_option_hwnd_handle_fmt_debug,
                az_option_logical_position_delete,
                az_option_logical_position_deep_copy,
                az_option_logical_position_fmt_debug,
                az_option_hot_reload_options_delete,
                az_option_hot_reload_options_deep_copy,
                az_option_hot_reload_options_fmt_debug,
                az_option_physical_position_i32_delete,
                az_option_physical_position_i32_deep_copy,
                az_option_physical_position_i32_fmt_debug,
                az_option_window_icon_delete,
                az_option_window_icon_deep_copy,
                az_option_window_icon_fmt_debug,
                az_option_string_delete,
                az_option_string_deep_copy,
                az_option_string_fmt_debug,
                az_option_x11_visual_delete,
                az_option_x11_visual_deep_copy,
                az_option_x11_visual_fmt_debug,
                az_option_i32_delete,
                az_option_i32_deep_copy,
                az_option_i32_fmt_debug,
                az_option_f32_delete,
                az_option_f32_deep_copy,
                az_option_f32_fmt_debug,
                az_option_mouse_cursor_type_delete,
                az_option_mouse_cursor_type_deep_copy,
                az_option_mouse_cursor_type_fmt_debug,
                az_option_logical_size_delete,
                az_option_logical_size_deep_copy,
                az_option_logical_size_fmt_debug,
                az_option_char_delete,
                az_option_char_deep_copy,
                az_option_char_fmt_debug,
                az_option_virtual_key_code_delete,
                az_option_virtual_key_code_deep_copy,
                az_option_virtual_key_code_fmt_debug,
                az_option_percentage_value_delete,
                az_option_percentage_value_deep_copy,
                az_option_percentage_value_fmt_debug,
                az_option_dom_delete,
                az_option_dom_deep_copy,
                az_option_dom_fmt_debug,
                az_option_texture_delete,
                az_option_texture_fmt_debug,
                az_option_image_mask_delete,
                az_option_image_mask_deep_copy,
                az_option_image_mask_fmt_debug,
                az_option_tab_index_delete,
                az_option_tab_index_deep_copy,
                az_option_tab_index_fmt_debug,
                az_option_style_background_content_value_delete,
                az_option_style_background_content_value_deep_copy,
                az_option_style_background_content_value_fmt_debug,
                az_option_style_background_position_value_delete,
                az_option_style_background_position_value_deep_copy,
                az_option_style_background_position_value_fmt_debug,
                az_option_style_background_size_value_delete,
                az_option_style_background_size_value_deep_copy,
                az_option_style_background_size_value_fmt_debug,
                az_option_style_background_repeat_value_delete,
                az_option_style_background_repeat_value_deep_copy,
                az_option_style_background_repeat_value_fmt_debug,
                az_option_style_font_size_value_delete,
                az_option_style_font_size_value_deep_copy,
                az_option_style_font_size_value_fmt_debug,
                az_option_style_font_family_value_delete,
                az_option_style_font_family_value_deep_copy,
                az_option_style_font_family_value_fmt_debug,
                az_option_style_text_color_value_delete,
                az_option_style_text_color_value_deep_copy,
                az_option_style_text_color_value_fmt_debug,
                az_option_style_text_alignment_horz_value_delete,
                az_option_style_text_alignment_horz_value_deep_copy,
                az_option_style_text_alignment_horz_value_fmt_debug,
                az_option_style_line_height_value_delete,
                az_option_style_line_height_value_deep_copy,
                az_option_style_line_height_value_fmt_debug,
                az_option_style_letter_spacing_value_delete,
                az_option_style_letter_spacing_value_deep_copy,
                az_option_style_letter_spacing_value_fmt_debug,
                az_option_style_word_spacing_value_delete,
                az_option_style_word_spacing_value_deep_copy,
                az_option_style_word_spacing_value_fmt_debug,
                az_option_style_tab_width_value_delete,
                az_option_style_tab_width_value_deep_copy,
                az_option_style_tab_width_value_fmt_debug,
                az_option_style_cursor_value_delete,
                az_option_style_cursor_value_deep_copy,
                az_option_style_cursor_value_fmt_debug,
                az_option_box_shadow_pre_display_item_value_delete,
                az_option_box_shadow_pre_display_item_value_deep_copy,
                az_option_box_shadow_pre_display_item_value_fmt_debug,
                az_option_style_border_top_color_value_delete,
                az_option_style_border_top_color_value_deep_copy,
                az_option_style_border_top_color_value_fmt_debug,
                az_option_style_border_left_color_value_delete,
                az_option_style_border_left_color_value_deep_copy,
                az_option_style_border_left_color_value_fmt_debug,
                az_option_style_border_right_color_value_delete,
                az_option_style_border_right_color_value_deep_copy,
                az_option_style_border_right_color_value_fmt_debug,
                az_option_style_border_bottom_color_value_delete,
                az_option_style_border_bottom_color_value_deep_copy,
                az_option_style_border_bottom_color_value_fmt_debug,
                az_option_style_border_top_style_value_delete,
                az_option_style_border_top_style_value_deep_copy,
                az_option_style_border_top_style_value_fmt_debug,
                az_option_style_border_left_style_value_delete,
                az_option_style_border_left_style_value_deep_copy,
                az_option_style_border_left_style_value_fmt_debug,
                az_option_style_border_right_style_value_delete,
                az_option_style_border_right_style_value_deep_copy,
                az_option_style_border_right_style_value_fmt_debug,
                az_option_style_border_bottom_style_value_delete,
                az_option_style_border_bottom_style_value_deep_copy,
                az_option_style_border_bottom_style_value_fmt_debug,
                az_option_style_border_top_left_radius_value_delete,
                az_option_style_border_top_left_radius_value_deep_copy,
                az_option_style_border_top_left_radius_value_fmt_debug,
                az_option_style_border_top_right_radius_value_delete,
                az_option_style_border_top_right_radius_value_deep_copy,
                az_option_style_border_top_right_radius_value_fmt_debug,
                az_option_style_border_bottom_left_radius_value_delete,
                az_option_style_border_bottom_left_radius_value_deep_copy,
                az_option_style_border_bottom_left_radius_value_fmt_debug,
                az_option_style_border_bottom_right_radius_value_delete,
                az_option_style_border_bottom_right_radius_value_deep_copy,
                az_option_style_border_bottom_right_radius_value_fmt_debug,
                az_option_layout_display_value_delete,
                az_option_layout_display_value_deep_copy,
                az_option_layout_display_value_fmt_debug,
                az_option_layout_float_value_delete,
                az_option_layout_float_value_deep_copy,
                az_option_layout_float_value_fmt_debug,
                az_option_layout_box_sizing_value_delete,
                az_option_layout_box_sizing_value_deep_copy,
                az_option_layout_box_sizing_value_fmt_debug,
                az_option_layout_width_value_delete,
                az_option_layout_width_value_deep_copy,
                az_option_layout_width_value_fmt_debug,
                az_option_layout_height_value_delete,
                az_option_layout_height_value_deep_copy,
                az_option_layout_height_value_fmt_debug,
                az_option_layout_min_width_value_delete,
                az_option_layout_min_width_value_deep_copy,
                az_option_layout_min_width_value_fmt_debug,
                az_option_layout_min_height_value_delete,
                az_option_layout_min_height_value_deep_copy,
                az_option_layout_min_height_value_fmt_debug,
                az_option_layout_max_width_value_delete,
                az_option_layout_max_width_value_deep_copy,
                az_option_layout_max_width_value_fmt_debug,
                az_option_layout_max_height_value_delete,
                az_option_layout_max_height_value_deep_copy,
                az_option_layout_max_height_value_fmt_debug,
                az_option_layout_position_value_delete,
                az_option_layout_position_value_deep_copy,
                az_option_layout_position_value_fmt_debug,
                az_option_layout_top_value_delete,
                az_option_layout_top_value_deep_copy,
                az_option_layout_top_value_fmt_debug,
                az_option_layout_bottom_value_delete,
                az_option_layout_bottom_value_deep_copy,
                az_option_layout_bottom_value_fmt_debug,
                az_option_layout_right_value_delete,
                az_option_layout_right_value_deep_copy,
                az_option_layout_right_value_fmt_debug,
                az_option_layout_left_value_delete,
                az_option_layout_left_value_deep_copy,
                az_option_layout_left_value_fmt_debug,
                az_option_layout_padding_top_value_delete,
                az_option_layout_padding_top_value_deep_copy,
                az_option_layout_padding_top_value_fmt_debug,
                az_option_layout_padding_bottom_value_delete,
                az_option_layout_padding_bottom_value_deep_copy,
                az_option_layout_padding_bottom_value_fmt_debug,
                az_option_layout_padding_left_value_delete,
                az_option_layout_padding_left_value_deep_copy,
                az_option_layout_padding_left_value_fmt_debug,
                az_option_layout_padding_right_value_delete,
                az_option_layout_padding_right_value_deep_copy,
                az_option_layout_padding_right_value_fmt_debug,
                az_option_layout_margin_top_value_delete,
                az_option_layout_margin_top_value_deep_copy,
                az_option_layout_margin_top_value_fmt_debug,
                az_option_layout_margin_bottom_value_delete,
                az_option_layout_margin_bottom_value_deep_copy,
                az_option_layout_margin_bottom_value_fmt_debug,
                az_option_layout_margin_left_value_delete,
                az_option_layout_margin_left_value_deep_copy,
                az_option_layout_margin_left_value_fmt_debug,
                az_option_layout_margin_right_value_delete,
                az_option_layout_margin_right_value_deep_copy,
                az_option_layout_margin_right_value_fmt_debug,
                az_option_style_border_top_width_value_delete,
                az_option_style_border_top_width_value_deep_copy,
                az_option_style_border_top_width_value_fmt_debug,
                az_option_style_border_left_width_value_delete,
                az_option_style_border_left_width_value_deep_copy,
                az_option_style_border_left_width_value_fmt_debug,
                az_option_style_border_right_width_value_delete,
                az_option_style_border_right_width_value_deep_copy,
                az_option_style_border_right_width_value_fmt_debug,
                az_option_style_border_bottom_width_value_delete,
                az_option_style_border_bottom_width_value_deep_copy,
                az_option_style_border_bottom_width_value_fmt_debug,
                az_option_overflow_value_delete,
                az_option_overflow_value_deep_copy,
                az_option_overflow_value_fmt_debug,
                az_option_layout_direction_value_delete,
                az_option_layout_direction_value_deep_copy,
                az_option_layout_direction_value_fmt_debug,
                az_option_layout_wrap_value_delete,
                az_option_layout_wrap_value_deep_copy,
                az_option_layout_wrap_value_fmt_debug,
                az_option_layout_flex_grow_value_delete,
                az_option_layout_flex_grow_value_deep_copy,
                az_option_layout_flex_grow_value_fmt_debug,
                az_option_layout_flex_shrink_value_delete,
                az_option_layout_flex_shrink_value_deep_copy,
                az_option_layout_flex_shrink_value_fmt_debug,
                az_option_layout_justify_content_value_delete,
                az_option_layout_justify_content_value_deep_copy,
                az_option_layout_justify_content_value_fmt_debug,
                az_option_layout_align_items_value_delete,
                az_option_layout_align_items_value_deep_copy,
                az_option_layout_align_items_value_fmt_debug,
                az_option_layout_align_content_value_delete,
                az_option_layout_align_content_value_deep_copy,
                az_option_layout_align_content_value_fmt_debug,
                az_option_hover_group_delete,
                az_option_hover_group_deep_copy,
                az_option_hover_group_fmt_debug,
                az_option_tag_id_delete,
                az_option_tag_id_deep_copy,
                az_option_tag_id_fmt_debug,
                az_option_duration_delete,
                az_option_duration_deep_copy,
                az_option_duration_fmt_debug,
                az_option_instant_ptr_delete,
                az_option_instant_ptr_deep_copy,
                az_option_instant_ptr_fmt_debug,
                az_option_usize_delete,
                az_option_usize_deep_copy,
                az_option_usize_fmt_debug,
                az_option_u8_vec_ref_delete,
                az_option_u8_vec_ref_fmt_debug,
                az_result_svg_svg_parse_error_delete,
                az_result_svg_svg_parse_error_deep_copy,
                az_result_svg_svg_parse_error_fmt_debug,
                az_result_ref_any_block_error_delete,
                az_result_ref_any_block_error_deep_copy,
                az_result_ref_any_block_error_fmt_debug,
                az_svg_parse_error_delete,
                az_svg_parse_error_deep_copy,
                az_svg_parse_error_fmt_debug,
                az_xml_error_delete,
                az_xml_error_deep_copy,
                az_xml_error_fmt_debug,
                az_duplicated_namespace_error_delete,
                az_duplicated_namespace_error_deep_copy,
                az_duplicated_namespace_error_fmt_debug,
                az_unknown_namespace_error_delete,
                az_unknown_namespace_error_deep_copy,
                az_unknown_namespace_error_fmt_debug,
                az_unexpected_close_tag_error_delete,
                az_unexpected_close_tag_error_deep_copy,
                az_unexpected_close_tag_error_fmt_debug,
                az_unknown_entity_reference_error_delete,
                az_unknown_entity_reference_error_deep_copy,
                az_unknown_entity_reference_error_fmt_debug,
                az_duplicated_attribute_error_delete,
                az_duplicated_attribute_error_deep_copy,
                az_duplicated_attribute_error_fmt_debug,
                az_xml_parse_error_delete,
                az_xml_parse_error_deep_copy,
                az_xml_parse_error_fmt_debug,
                az_xml_text_error_delete,
                az_xml_text_error_deep_copy,
                az_xml_text_error_fmt_debug,
                az_xml_stream_error_delete,
                az_xml_stream_error_deep_copy,
                az_xml_stream_error_fmt_debug,
                az_non_xml_char_error_delete,
                az_non_xml_char_error_deep_copy,
                az_non_xml_char_error_fmt_debug,
                az_invalid_char_error_delete,
                az_invalid_char_error_deep_copy,
                az_invalid_char_error_fmt_debug,
                az_invalid_char_multiple_error_delete,
                az_invalid_char_multiple_error_deep_copy,
                az_invalid_char_multiple_error_fmt_debug,
                az_invalid_quote_error_delete,
                az_invalid_quote_error_deep_copy,
                az_invalid_quote_error_fmt_debug,
                az_invalid_space_error_delete,
                az_invalid_space_error_deep_copy,
                az_invalid_space_error_fmt_debug,
                az_invalid_string_error_delete,
                az_invalid_string_error_deep_copy,
                az_invalid_string_error_fmt_debug,
                az_xml_text_pos_delete,
                az_xml_text_pos_deep_copy,
                az_xml_text_pos_fmt_debug,
                az_instant_ptr_now,
                az_instant_ptr_delete,
                az_instant_ptr_fmt_debug,
                az_instant_ptr_partial_eq,
                az_instant_ptr_partial_cmp,
                az_instant_ptr_cmp,
                az_duration_delete,
                az_duration_deep_copy,
                az_duration_fmt_debug,
                az_app_config_ptr_default,
                az_app_config_ptr_delete,
                az_app_config_ptr_fmt_debug,
                az_app_ptr_new,
                az_app_ptr_run,
                az_app_ptr_delete,
                az_app_ptr_fmt_debug,
                az_node_id_delete,
                az_node_id_deep_copy,
                az_node_id_fmt_debug,
                az_dom_id_delete,
                az_dom_id_deep_copy,
                az_dom_id_fmt_debug,
                az_dom_node_id_delete,
                az_dom_node_id_deep_copy,
                az_dom_node_id_fmt_debug,
                az_hidpi_adjusted_bounds_get_logical_size,
                az_hidpi_adjusted_bounds_get_physical_size,
                az_hidpi_adjusted_bounds_get_hidpi_factor,
                az_hidpi_adjusted_bounds_delete,
                az_hidpi_adjusted_bounds_deep_copy,
                az_hidpi_adjusted_bounds_fmt_debug,
                az_layout_callback_delete,
                az_layout_callback_deep_copy,
                az_layout_callback_fmt_debug,
                az_callback_delete,
                az_callback_deep_copy,
                az_callback_fmt_debug,
                az_callback_partial_eq,
                az_callback_partial_cmp,
                az_callback_cmp,
                az_callback_hash,
                az_callback_info_ptr_get_state,
                az_callback_info_ptr_get_keyboard_state,
                az_callback_info_ptr_get_mouse_state,
                az_callback_info_ptr_set_window_state,
                az_callback_info_ptr_delete,
                az_callback_info_ptr_fmt_debug,
                az_update_screen_delete,
                az_update_screen_deep_copy,
                az_update_screen_fmt_debug,
                az_i_frame_callback_delete,
                az_i_frame_callback_deep_copy,
                az_i_frame_callback_fmt_debug,
                az_i_frame_callback_info_ptr_get_state,
                az_i_frame_callback_info_ptr_get_bounds,
                az_i_frame_callback_info_ptr_delete,
                az_i_frame_callback_info_ptr_fmt_debug,
                az_i_frame_callback_return_delete,
                az_i_frame_callback_return_deep_copy,
                az_i_frame_callback_return_fmt_debug,
                az_gl_callback_delete,
                az_gl_callback_deep_copy,
                az_gl_callback_fmt_debug,
                az_gl_callback_info_ptr_get_state,
                az_gl_callback_info_ptr_get_gl_context,
                az_gl_callback_info_ptr_get_bounds,
                az_gl_callback_info_ptr_delete,
                az_gl_callback_info_ptr_fmt_debug,
                az_gl_callback_return_delete,
                az_gl_callback_return_fmt_debug,
                az_timer_callback_delete,
                az_timer_callback_deep_copy,
                az_timer_callback_fmt_debug,
                az_timer_callback_type_ptr_delete,
                az_timer_callback_type_ptr_fmt_debug,
                az_timer_callback_info_ptr_get_state,
                az_timer_callback_info_ptr_delete,
                az_timer_callback_info_ptr_fmt_debug,
                az_timer_callback_return_delete,
                az_timer_callback_return_deep_copy,
                az_timer_callback_return_fmt_debug,
                az_ref_any_sharing_info_can_be_shared,
                az_ref_any_sharing_info_can_be_shared_mut,
                az_ref_any_sharing_info_increase_ref,
                az_ref_any_sharing_info_decrease_ref,
                az_ref_any_sharing_info_increase_refmut,
                az_ref_any_sharing_info_decrease_refmut,
                az_ref_any_sharing_info_delete,
                az_ref_any_sharing_info_fmt_debug,
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
                az_ref_any_fmt_debug,
                az_ref_any_partial_eq,
                az_ref_any_partial_cmp,
                az_ref_any_cmp,
                az_ref_any_hash,
                az_layout_info_ptr_get_gl_context,
                az_layout_info_ptr_delete,
                az_layout_info_ptr_fmt_debug,
                az_css_rule_block_delete,
                az_css_rule_block_deep_copy,
                az_css_rule_block_fmt_debug,
                az_css_declaration_delete,
                az_css_declaration_deep_copy,
                az_css_declaration_fmt_debug,
                az_dynamic_css_property_delete,
                az_dynamic_css_property_deep_copy,
                az_dynamic_css_property_fmt_debug,
                az_css_path_delete,
                az_css_path_deep_copy,
                az_css_path_fmt_debug,
                az_css_path_selector_delete,
                az_css_path_selector_deep_copy,
                az_css_path_selector_fmt_debug,
                az_node_type_path_delete,
                az_node_type_path_deep_copy,
                az_node_type_path_fmt_debug,
                az_css_path_pseudo_selector_delete,
                az_css_path_pseudo_selector_deep_copy,
                az_css_path_pseudo_selector_fmt_debug,
                az_css_nth_child_selector_delete,
                az_css_nth_child_selector_deep_copy,
                az_css_nth_child_selector_fmt_debug,
                az_css_nth_child_pattern_delete,
                az_css_nth_child_pattern_deep_copy,
                az_css_nth_child_pattern_fmt_debug,
                az_stylesheet_delete,
                az_stylesheet_deep_copy,
                az_stylesheet_fmt_debug,
                az_css_native,
                az_css_empty,
                az_css_from_string,
                az_css_override_native,
                az_css_delete,
                az_css_deep_copy,
                az_css_fmt_debug,
                az_color_u_delete,
                az_color_u_deep_copy,
                az_color_u_fmt_debug,
                az_size_metric_delete,
                az_size_metric_deep_copy,
                az_size_metric_fmt_debug,
                az_float_value_delete,
                az_float_value_deep_copy,
                az_float_value_fmt_debug,
                az_pixel_value_delete,
                az_pixel_value_deep_copy,
                az_pixel_value_fmt_debug,
                az_pixel_value_no_percent_delete,
                az_pixel_value_no_percent_deep_copy,
                az_pixel_value_no_percent_fmt_debug,
                az_box_shadow_clip_mode_delete,
                az_box_shadow_clip_mode_deep_copy,
                az_box_shadow_clip_mode_fmt_debug,
                az_box_shadow_pre_display_item_delete,
                az_box_shadow_pre_display_item_deep_copy,
                az_box_shadow_pre_display_item_fmt_debug,
                az_layout_align_content_delete,
                az_layout_align_content_deep_copy,
                az_layout_align_content_fmt_debug,
                az_layout_align_items_delete,
                az_layout_align_items_deep_copy,
                az_layout_align_items_fmt_debug,
                az_layout_bottom_delete,
                az_layout_bottom_deep_copy,
                az_layout_bottom_fmt_debug,
                az_layout_box_sizing_delete,
                az_layout_box_sizing_deep_copy,
                az_layout_box_sizing_fmt_debug,
                az_layout_direction_delete,
                az_layout_direction_deep_copy,
                az_layout_direction_fmt_debug,
                az_layout_display_delete,
                az_layout_display_deep_copy,
                az_layout_display_fmt_debug,
                az_layout_flex_grow_delete,
                az_layout_flex_grow_deep_copy,
                az_layout_flex_grow_fmt_debug,
                az_layout_flex_shrink_delete,
                az_layout_flex_shrink_deep_copy,
                az_layout_flex_shrink_fmt_debug,
                az_layout_float_delete,
                az_layout_float_deep_copy,
                az_layout_float_fmt_debug,
                az_layout_height_delete,
                az_layout_height_deep_copy,
                az_layout_height_fmt_debug,
                az_layout_justify_content_delete,
                az_layout_justify_content_deep_copy,
                az_layout_justify_content_fmt_debug,
                az_layout_left_delete,
                az_layout_left_deep_copy,
                az_layout_left_fmt_debug,
                az_layout_margin_bottom_delete,
                az_layout_margin_bottom_deep_copy,
                az_layout_margin_bottom_fmt_debug,
                az_layout_margin_left_delete,
                az_layout_margin_left_deep_copy,
                az_layout_margin_left_fmt_debug,
                az_layout_margin_right_delete,
                az_layout_margin_right_deep_copy,
                az_layout_margin_right_fmt_debug,
                az_layout_margin_top_delete,
                az_layout_margin_top_deep_copy,
                az_layout_margin_top_fmt_debug,
                az_layout_max_height_delete,
                az_layout_max_height_deep_copy,
                az_layout_max_height_fmt_debug,
                az_layout_max_width_delete,
                az_layout_max_width_deep_copy,
                az_layout_max_width_fmt_debug,
                az_layout_min_height_delete,
                az_layout_min_height_deep_copy,
                az_layout_min_height_fmt_debug,
                az_layout_min_width_delete,
                az_layout_min_width_deep_copy,
                az_layout_min_width_fmt_debug,
                az_layout_padding_bottom_delete,
                az_layout_padding_bottom_deep_copy,
                az_layout_padding_bottom_fmt_debug,
                az_layout_padding_left_delete,
                az_layout_padding_left_deep_copy,
                az_layout_padding_left_fmt_debug,
                az_layout_padding_right_delete,
                az_layout_padding_right_deep_copy,
                az_layout_padding_right_fmt_debug,
                az_layout_padding_top_delete,
                az_layout_padding_top_deep_copy,
                az_layout_padding_top_fmt_debug,
                az_layout_position_delete,
                az_layout_position_deep_copy,
                az_layout_position_fmt_debug,
                az_layout_right_delete,
                az_layout_right_deep_copy,
                az_layout_right_fmt_debug,
                az_layout_top_delete,
                az_layout_top_deep_copy,
                az_layout_top_fmt_debug,
                az_layout_width_delete,
                az_layout_width_deep_copy,
                az_layout_width_fmt_debug,
                az_layout_wrap_delete,
                az_layout_wrap_deep_copy,
                az_layout_wrap_fmt_debug,
                az_overflow_delete,
                az_overflow_deep_copy,
                az_overflow_fmt_debug,
                az_percentage_value_delete,
                az_percentage_value_deep_copy,
                az_percentage_value_fmt_debug,
                az_gradient_stop_pre_delete,
                az_gradient_stop_pre_deep_copy,
                az_gradient_stop_pre_fmt_debug,
                az_gradient_stop_pre_partial_eq,
                az_gradient_stop_pre_partial_cmp,
                az_gradient_stop_pre_cmp,
                az_gradient_stop_pre_hash,
                az_direction_corner_delete,
                az_direction_corner_deep_copy,
                az_direction_corner_fmt_debug,
                az_direction_corners_delete,
                az_direction_corners_deep_copy,
                az_direction_corners_fmt_debug,
                az_direction_delete,
                az_direction_deep_copy,
                az_direction_fmt_debug,
                az_extend_mode_delete,
                az_extend_mode_deep_copy,
                az_extend_mode_fmt_debug,
                az_linear_gradient_delete,
                az_linear_gradient_deep_copy,
                az_linear_gradient_fmt_debug,
                az_shape_delete,
                az_shape_deep_copy,
                az_shape_fmt_debug,
                az_radial_gradient_delete,
                az_radial_gradient_deep_copy,
                az_radial_gradient_fmt_debug,
                az_css_image_id_delete,
                az_css_image_id_deep_copy,
                az_css_image_id_fmt_debug,
                az_style_background_content_delete,
                az_style_background_content_deep_copy,
                az_style_background_content_fmt_debug,
                az_background_position_horizontal_delete,
                az_background_position_horizontal_deep_copy,
                az_background_position_horizontal_fmt_debug,
                az_background_position_vertical_delete,
                az_background_position_vertical_deep_copy,
                az_background_position_vertical_fmt_debug,
                az_style_background_position_delete,
                az_style_background_position_deep_copy,
                az_style_background_position_fmt_debug,
                az_style_background_repeat_delete,
                az_style_background_repeat_deep_copy,
                az_style_background_repeat_fmt_debug,
                az_style_background_size_delete,
                az_style_background_size_deep_copy,
                az_style_background_size_fmt_debug,
                az_style_border_bottom_color_delete,
                az_style_border_bottom_color_deep_copy,
                az_style_border_bottom_color_fmt_debug,
                az_style_border_bottom_left_radius_delete,
                az_style_border_bottom_left_radius_deep_copy,
                az_style_border_bottom_left_radius_fmt_debug,
                az_style_border_bottom_right_radius_delete,
                az_style_border_bottom_right_radius_deep_copy,
                az_style_border_bottom_right_radius_fmt_debug,
                az_border_style_delete,
                az_border_style_deep_copy,
                az_border_style_fmt_debug,
                az_style_border_bottom_style_delete,
                az_style_border_bottom_style_deep_copy,
                az_style_border_bottom_style_fmt_debug,
                az_style_border_bottom_width_delete,
                az_style_border_bottom_width_deep_copy,
                az_style_border_bottom_width_fmt_debug,
                az_style_border_left_color_delete,
                az_style_border_left_color_deep_copy,
                az_style_border_left_color_fmt_debug,
                az_style_border_left_style_delete,
                az_style_border_left_style_deep_copy,
                az_style_border_left_style_fmt_debug,
                az_style_border_left_width_delete,
                az_style_border_left_width_deep_copy,
                az_style_border_left_width_fmt_debug,
                az_style_border_right_color_delete,
                az_style_border_right_color_deep_copy,
                az_style_border_right_color_fmt_debug,
                az_style_border_right_style_delete,
                az_style_border_right_style_deep_copy,
                az_style_border_right_style_fmt_debug,
                az_style_border_right_width_delete,
                az_style_border_right_width_deep_copy,
                az_style_border_right_width_fmt_debug,
                az_style_border_top_color_delete,
                az_style_border_top_color_deep_copy,
                az_style_border_top_color_fmt_debug,
                az_style_border_top_left_radius_delete,
                az_style_border_top_left_radius_deep_copy,
                az_style_border_top_left_radius_fmt_debug,
                az_style_border_top_right_radius_delete,
                az_style_border_top_right_radius_deep_copy,
                az_style_border_top_right_radius_fmt_debug,
                az_style_border_top_style_delete,
                az_style_border_top_style_deep_copy,
                az_style_border_top_style_fmt_debug,
                az_style_border_top_width_delete,
                az_style_border_top_width_deep_copy,
                az_style_border_top_width_fmt_debug,
                az_style_cursor_delete,
                az_style_cursor_deep_copy,
                az_style_cursor_fmt_debug,
                az_style_font_family_delete,
                az_style_font_family_deep_copy,
                az_style_font_family_fmt_debug,
                az_style_font_size_delete,
                az_style_font_size_deep_copy,
                az_style_font_size_fmt_debug,
                az_style_letter_spacing_delete,
                az_style_letter_spacing_deep_copy,
                az_style_letter_spacing_fmt_debug,
                az_style_line_height_delete,
                az_style_line_height_deep_copy,
                az_style_line_height_fmt_debug,
                az_style_tab_width_delete,
                az_style_tab_width_deep_copy,
                az_style_tab_width_fmt_debug,
                az_style_text_alignment_horz_delete,
                az_style_text_alignment_horz_deep_copy,
                az_style_text_alignment_horz_fmt_debug,
                az_style_text_color_delete,
                az_style_text_color_deep_copy,
                az_style_text_color_fmt_debug,
                az_style_word_spacing_delete,
                az_style_word_spacing_deep_copy,
                az_style_word_spacing_fmt_debug,
                az_box_shadow_pre_display_item_value_delete,
                az_box_shadow_pre_display_item_value_deep_copy,
                az_box_shadow_pre_display_item_value_fmt_debug,
                az_layout_align_content_value_delete,
                az_layout_align_content_value_deep_copy,
                az_layout_align_content_value_fmt_debug,
                az_layout_align_items_value_delete,
                az_layout_align_items_value_deep_copy,
                az_layout_align_items_value_fmt_debug,
                az_layout_bottom_value_delete,
                az_layout_bottom_value_deep_copy,
                az_layout_bottom_value_fmt_debug,
                az_layout_box_sizing_value_delete,
                az_layout_box_sizing_value_deep_copy,
                az_layout_box_sizing_value_fmt_debug,
                az_layout_direction_value_delete,
                az_layout_direction_value_deep_copy,
                az_layout_direction_value_fmt_debug,
                az_layout_display_value_delete,
                az_layout_display_value_deep_copy,
                az_layout_display_value_fmt_debug,
                az_layout_flex_grow_value_delete,
                az_layout_flex_grow_value_deep_copy,
                az_layout_flex_grow_value_fmt_debug,
                az_layout_flex_shrink_value_delete,
                az_layout_flex_shrink_value_deep_copy,
                az_layout_flex_shrink_value_fmt_debug,
                az_layout_float_value_delete,
                az_layout_float_value_deep_copy,
                az_layout_float_value_fmt_debug,
                az_layout_height_value_delete,
                az_layout_height_value_deep_copy,
                az_layout_height_value_fmt_debug,
                az_layout_justify_content_value_delete,
                az_layout_justify_content_value_deep_copy,
                az_layout_justify_content_value_fmt_debug,
                az_layout_left_value_delete,
                az_layout_left_value_deep_copy,
                az_layout_left_value_fmt_debug,
                az_layout_margin_bottom_value_delete,
                az_layout_margin_bottom_value_deep_copy,
                az_layout_margin_bottom_value_fmt_debug,
                az_layout_margin_left_value_delete,
                az_layout_margin_left_value_deep_copy,
                az_layout_margin_left_value_fmt_debug,
                az_layout_margin_right_value_delete,
                az_layout_margin_right_value_deep_copy,
                az_layout_margin_right_value_fmt_debug,
                az_layout_margin_top_value_delete,
                az_layout_margin_top_value_deep_copy,
                az_layout_margin_top_value_fmt_debug,
                az_layout_max_height_value_delete,
                az_layout_max_height_value_deep_copy,
                az_layout_max_height_value_fmt_debug,
                az_layout_max_width_value_delete,
                az_layout_max_width_value_deep_copy,
                az_layout_max_width_value_fmt_debug,
                az_layout_min_height_value_delete,
                az_layout_min_height_value_deep_copy,
                az_layout_min_height_value_fmt_debug,
                az_layout_min_width_value_delete,
                az_layout_min_width_value_deep_copy,
                az_layout_min_width_value_fmt_debug,
                az_layout_padding_bottom_value_delete,
                az_layout_padding_bottom_value_deep_copy,
                az_layout_padding_bottom_value_fmt_debug,
                az_layout_padding_left_value_delete,
                az_layout_padding_left_value_deep_copy,
                az_layout_padding_left_value_fmt_debug,
                az_layout_padding_right_value_delete,
                az_layout_padding_right_value_deep_copy,
                az_layout_padding_right_value_fmt_debug,
                az_layout_padding_top_value_delete,
                az_layout_padding_top_value_deep_copy,
                az_layout_padding_top_value_fmt_debug,
                az_layout_position_value_delete,
                az_layout_position_value_deep_copy,
                az_layout_position_value_fmt_debug,
                az_layout_right_value_delete,
                az_layout_right_value_deep_copy,
                az_layout_right_value_fmt_debug,
                az_layout_top_value_delete,
                az_layout_top_value_deep_copy,
                az_layout_top_value_fmt_debug,
                az_layout_width_value_delete,
                az_layout_width_value_deep_copy,
                az_layout_width_value_fmt_debug,
                az_layout_wrap_value_delete,
                az_layout_wrap_value_deep_copy,
                az_layout_wrap_value_fmt_debug,
                az_overflow_value_delete,
                az_overflow_value_deep_copy,
                az_overflow_value_fmt_debug,
                az_style_background_content_value_delete,
                az_style_background_content_value_deep_copy,
                az_style_background_content_value_fmt_debug,
                az_style_background_position_value_delete,
                az_style_background_position_value_deep_copy,
                az_style_background_position_value_fmt_debug,
                az_style_background_repeat_value_delete,
                az_style_background_repeat_value_deep_copy,
                az_style_background_repeat_value_fmt_debug,
                az_style_background_size_value_delete,
                az_style_background_size_value_deep_copy,
                az_style_background_size_value_fmt_debug,
                az_style_border_bottom_color_value_delete,
                az_style_border_bottom_color_value_deep_copy,
                az_style_border_bottom_color_value_fmt_debug,
                az_style_border_bottom_left_radius_value_delete,
                az_style_border_bottom_left_radius_value_deep_copy,
                az_style_border_bottom_left_radius_value_fmt_debug,
                az_style_border_bottom_right_radius_value_delete,
                az_style_border_bottom_right_radius_value_deep_copy,
                az_style_border_bottom_right_radius_value_fmt_debug,
                az_style_border_bottom_style_value_delete,
                az_style_border_bottom_style_value_deep_copy,
                az_style_border_bottom_style_value_fmt_debug,
                az_style_border_bottom_width_value_delete,
                az_style_border_bottom_width_value_deep_copy,
                az_style_border_bottom_width_value_fmt_debug,
                az_style_border_left_color_value_delete,
                az_style_border_left_color_value_deep_copy,
                az_style_border_left_color_value_fmt_debug,
                az_style_border_left_style_value_delete,
                az_style_border_left_style_value_deep_copy,
                az_style_border_left_style_value_fmt_debug,
                az_style_border_left_width_value_delete,
                az_style_border_left_width_value_deep_copy,
                az_style_border_left_width_value_fmt_debug,
                az_style_border_right_color_value_delete,
                az_style_border_right_color_value_deep_copy,
                az_style_border_right_color_value_fmt_debug,
                az_style_border_right_style_value_delete,
                az_style_border_right_style_value_deep_copy,
                az_style_border_right_style_value_fmt_debug,
                az_style_border_right_width_value_delete,
                az_style_border_right_width_value_deep_copy,
                az_style_border_right_width_value_fmt_debug,
                az_style_border_top_color_value_delete,
                az_style_border_top_color_value_deep_copy,
                az_style_border_top_color_value_fmt_debug,
                az_style_border_top_left_radius_value_delete,
                az_style_border_top_left_radius_value_deep_copy,
                az_style_border_top_left_radius_value_fmt_debug,
                az_style_border_top_right_radius_value_delete,
                az_style_border_top_right_radius_value_deep_copy,
                az_style_border_top_right_radius_value_fmt_debug,
                az_style_border_top_style_value_delete,
                az_style_border_top_style_value_deep_copy,
                az_style_border_top_style_value_fmt_debug,
                az_style_border_top_width_value_delete,
                az_style_border_top_width_value_deep_copy,
                az_style_border_top_width_value_fmt_debug,
                az_style_cursor_value_delete,
                az_style_cursor_value_deep_copy,
                az_style_cursor_value_fmt_debug,
                az_style_font_family_value_delete,
                az_style_font_family_value_deep_copy,
                az_style_font_family_value_fmt_debug,
                az_style_font_size_value_delete,
                az_style_font_size_value_deep_copy,
                az_style_font_size_value_fmt_debug,
                az_style_letter_spacing_value_delete,
                az_style_letter_spacing_value_deep_copy,
                az_style_letter_spacing_value_fmt_debug,
                az_style_line_height_value_delete,
                az_style_line_height_value_deep_copy,
                az_style_line_height_value_fmt_debug,
                az_style_tab_width_value_delete,
                az_style_tab_width_value_deep_copy,
                az_style_tab_width_value_fmt_debug,
                az_style_text_alignment_horz_value_delete,
                az_style_text_alignment_horz_value_deep_copy,
                az_style_text_alignment_horz_value_fmt_debug,
                az_style_text_color_value_delete,
                az_style_text_color_value_deep_copy,
                az_style_text_color_value_fmt_debug,
                az_style_word_spacing_value_delete,
                az_style_word_spacing_value_deep_copy,
                az_style_word_spacing_value_fmt_debug,
                az_css_property_delete,
                az_css_property_deep_copy,
                az_css_property_fmt_debug,
                az_css_property_partial_eq,
                az_css_property_partial_cmp,
                az_css_property_cmp,
                az_css_property_hash,
                az_node_delete,
                az_node_deep_copy,
                az_node_fmt_debug,
                az_node_partial_eq,
                az_node_partial_cmp,
                az_node_cmp,
                az_node_hash,
                az_cascade_info_delete,
                az_cascade_info_deep_copy,
                az_cascade_info_fmt_debug,
                az_cascade_info_partial_eq,
                az_cascade_info_partial_cmp,
                az_cascade_info_cmp,
                az_cascade_info_hash,
                az_rect_style_delete,
                az_rect_style_deep_copy,
                az_rect_style_fmt_debug,
                az_rect_style_partial_eq,
                az_rect_style_partial_cmp,
                az_rect_style_cmp,
                az_rect_style_hash,
                az_rect_layout_delete,
                az_rect_layout_deep_copy,
                az_rect_layout_fmt_debug,
                az_rect_layout_partial_eq,
                az_rect_layout_partial_cmp,
                az_rect_layout_cmp,
                az_rect_layout_hash,
                az_cascaded_css_property_with_source_delete,
                az_cascaded_css_property_with_source_deep_copy,
                az_cascaded_css_property_with_source_fmt_debug,
                az_cascaded_css_property_with_source_partial_eq,
                az_cascaded_css_property_with_source_partial_cmp,
                az_cascaded_css_property_with_source_cmp,
                az_cascaded_css_property_with_source_hash,
                az_css_property_source_delete,
                az_css_property_source_deep_copy,
                az_css_property_source_fmt_debug,
                az_css_property_source_partial_eq,
                az_css_property_source_partial_cmp,
                az_css_property_source_cmp,
                az_css_property_source_hash,
                az_styled_node_delete,
                az_styled_node_deep_copy,
                az_styled_node_fmt_debug,
                az_styled_node_partial_eq,
                az_styled_node_partial_cmp,
                az_styled_node_cmp,
                az_styled_node_hash,
                az_tag_id_delete,
                az_tag_id_deep_copy,
                az_tag_id_fmt_debug,
                az_tag_id_partial_eq,
                az_tag_id_partial_cmp,
                az_tag_id_cmp,
                az_tag_id_hash,
                az_tag_id_to_node_id_mapping_delete,
                az_tag_id_to_node_id_mapping_deep_copy,
                az_tag_id_to_node_id_mapping_fmt_debug,
                az_tag_id_to_node_id_mapping_partial_eq,
                az_tag_id_to_node_id_mapping_partial_cmp,
                az_tag_id_to_node_id_mapping_cmp,
                az_tag_id_to_node_id_mapping_hash,
                az_hover_group_delete,
                az_hover_group_deep_copy,
                az_hover_group_fmt_debug,
                az_hover_group_partial_eq,
                az_hover_group_partial_cmp,
                az_hover_group_cmp,
                az_hover_group_hash,
                az_active_hover_delete,
                az_active_hover_deep_copy,
                az_active_hover_fmt_debug,
                az_active_hover_partial_eq,
                az_active_hover_partial_cmp,
                az_active_hover_cmp,
                az_active_hover_hash,
                az_parent_with_node_depth_delete,
                az_parent_with_node_depth_deep_copy,
                az_parent_with_node_depth_fmt_debug,
                az_parent_with_node_depth_partial_eq,
                az_parent_with_node_depth_partial_cmp,
                az_parent_with_node_depth_cmp,
                az_parent_with_node_depth_hash,
                az_style_options_delete,
                az_style_options_deep_copy,
                az_style_options_fmt_debug,
                az_style_options_partial_eq,
                az_style_options_partial_cmp,
                az_style_options_cmp,
                az_style_options_hash,
                az_styled_dom_new,
                az_styled_dom_append,
                az_styled_dom_delete,
                az_styled_dom_deep_copy,
                az_styled_dom_fmt_debug,
                az_styled_dom_partial_eq,
                az_styled_dom_partial_cmp,
                az_styled_dom_cmp,
                az_styled_dom_hash,
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
                az_dom_add_inline_css,
                az_dom_with_inline_css,
                az_dom_set_is_draggable,
                az_dom_with_clip_mask,
                az_dom_set_clip_mask,
                az_dom_is_draggable,
                az_dom_set_tab_index,
                az_dom_with_tab_index,
                az_dom_has_id,
                az_dom_has_class,
                az_dom_add_child,
                az_dom_with_child,
                az_dom_get_html_string,
                az_dom_delete,
                az_dom_deep_copy,
                az_dom_fmt_debug,
                az_dom_partial_eq,
                az_dom_partial_cmp,
                az_dom_cmp,
                az_dom_hash,
                az_gl_texture_node_delete,
                az_gl_texture_node_deep_copy,
                az_gl_texture_node_fmt_debug,
                az_i_frame_node_delete,
                az_i_frame_node_deep_copy,
                az_i_frame_node_fmt_debug,
                az_callback_data_delete,
                az_callback_data_deep_copy,
                az_callback_data_fmt_debug,
                az_callback_data_partial_eq,
                az_callback_data_partial_cmp,
                az_callback_data_cmp,
                az_callback_data_hash,
                az_image_mask_delete,
                az_image_mask_deep_copy,
                az_image_mask_fmt_debug,
                az_image_mask_partial_eq,
                az_image_mask_partial_cmp,
                az_image_mask_cmp,
                az_image_mask_hash,
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
                az_node_data_add_callback,
                az_node_data_with_callback,
                az_node_data_add_inline_css,
                az_node_data_with_inline_css,
                az_node_data_with_clip_mask,
                az_node_data_set_clip_mask,
                az_node_data_set_is_draggable,
                az_node_data_is_draggable,
                az_node_data_set_tab_index,
                az_node_data_with_tab_index,
                az_node_data_has_id,
                az_node_data_has_class,
                az_node_data_delete,
                az_node_data_deep_copy,
                az_node_data_fmt_debug,
                az_node_type_delete,
                az_node_type_deep_copy,
                az_node_type_fmt_debug,
                az_on_into_event_filter,
                az_on_delete,
                az_on_deep_copy,
                az_on_fmt_debug,
                az_event_filter_delete,
                az_event_filter_deep_copy,
                az_event_filter_fmt_debug,
                az_hover_event_filter_delete,
                az_hover_event_filter_deep_copy,
                az_hover_event_filter_fmt_debug,
                az_focus_event_filter_delete,
                az_focus_event_filter_deep_copy,
                az_focus_event_filter_fmt_debug,
                az_not_event_filter_delete,
                az_not_event_filter_deep_copy,
                az_not_event_filter_fmt_debug,
                az_window_event_filter_delete,
                az_window_event_filter_deep_copy,
                az_window_event_filter_fmt_debug,
                az_tab_index_delete,
                az_tab_index_deep_copy,
                az_tab_index_fmt_debug,
                az_gl_shader_precision_format_return_delete,
                az_gl_shader_precision_format_return_deep_copy,
                az_gl_shader_precision_format_return_fmt_debug,
                az_vertex_attribute_type_delete,
                az_vertex_attribute_type_deep_copy,
                az_vertex_attribute_type_fmt_debug,
                az_vertex_attribute_delete,
                az_vertex_attribute_deep_copy,
                az_vertex_attribute_fmt_debug,
                az_vertex_layout_delete,
                az_vertex_layout_deep_copy,
                az_vertex_layout_fmt_debug,
                az_vertex_array_object_delete,
                az_vertex_array_object_fmt_debug,
                az_index_buffer_format_delete,
                az_index_buffer_format_deep_copy,
                az_index_buffer_format_fmt_debug,
                az_vertex_buffer_delete,
                az_vertex_buffer_fmt_debug,
                az_gl_type_delete,
                az_gl_type_deep_copy,
                az_gl_type_fmt_debug,
                az_debug_message_delete,
                az_debug_message_deep_copy,
                az_debug_message_fmt_debug,
                az_debug_message_partial_eq,
                az_debug_message_partial_cmp,
                az_debug_message_cmp,
                az_debug_message_hash,
                az_u8_vec_ref_delete,
                az_u8_vec_ref_fmt_debug,
                az_u8_vec_ref_mut_delete,
                az_u8_vec_ref_mut_fmt_debug,
                az_f32_vec_ref_delete,
                az_f32_vec_ref_fmt_debug,
                az_i32_vec_ref_delete,
                az_i32_vec_ref_fmt_debug,
                az_g_luint_vec_ref_delete,
                az_g_luint_vec_ref_fmt_debug,
                az_g_lenum_vec_ref_delete,
                az_g_lenum_vec_ref_fmt_debug,
                az_g_lint_vec_ref_mut_delete,
                az_g_lint_vec_ref_mut_fmt_debug,
                az_g_lint64_vec_ref_mut_delete,
                az_g_lint64_vec_ref_mut_fmt_debug,
                az_g_lboolean_vec_ref_mut_delete,
                az_g_lboolean_vec_ref_mut_fmt_debug,
                az_g_lfloat_vec_ref_mut_delete,
                az_g_lfloat_vec_ref_mut_fmt_debug,
                az_refstr_vec_ref_delete,
                az_refstr_vec_ref_fmt_debug,
                az_refstr_delete,
                az_refstr_fmt_debug,
                az_get_program_binary_return_delete,
                az_get_program_binary_return_deep_copy,
                az_get_program_binary_return_fmt_debug,
                az_get_active_attrib_return_delete,
                az_get_active_attrib_return_deep_copy,
                az_get_active_attrib_return_fmt_debug,
                az_g_lsync_ptr_delete,
                az_g_lsync_ptr_fmt_debug,
                az_get_active_uniform_return_delete,
                az_get_active_uniform_return_deep_copy,
                az_get_active_uniform_return_fmt_debug,
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
                az_gl_context_ptr_fmt_debug,
                az_texture_delete,
                az_texture_fmt_debug,
                az_texture_flags_default,
                az_texture_flags_delete,
                az_texture_flags_deep_copy,
                az_texture_flags_fmt_debug,
                az_text_id_new,
                az_text_id_delete,
                az_text_id_deep_copy,
                az_text_id_fmt_debug,
                az_image_id_new,
                az_image_id_delete,
                az_image_id_deep_copy,
                az_image_id_fmt_debug,
                az_image_id_partial_eq,
                az_image_id_partial_cmp,
                az_image_id_cmp,
                az_image_id_hash,
                az_font_id_new,
                az_font_id_delete,
                az_font_id_deep_copy,
                az_font_id_fmt_debug,
                az_image_source_delete,
                az_image_source_deep_copy,
                az_image_source_fmt_debug,
                az_font_source_delete,
                az_font_source_deep_copy,
                az_font_source_fmt_debug,
                az_raw_image_new,
                az_raw_image_delete,
                az_raw_image_deep_copy,
                az_raw_image_fmt_debug,
                az_raw_image_format_delete,
                az_raw_image_format_deep_copy,
                az_raw_image_format_fmt_debug,
                az_svg_multi_polygon_delete,
                az_svg_multi_polygon_deep_copy,
                az_svg_multi_polygon_fmt_debug,
                az_svg_node_delete,
                az_svg_node_deep_copy,
                az_svg_node_fmt_debug,
                az_svg_styled_node_delete,
                az_svg_styled_node_deep_copy,
                az_svg_styled_node_fmt_debug,
                az_svg_circle_delete,
                az_svg_circle_deep_copy,
                az_svg_circle_fmt_debug,
                az_svg_path_delete,
                az_svg_path_deep_copy,
                az_svg_path_fmt_debug,
                az_svg_path_element_delete,
                az_svg_path_element_deep_copy,
                az_svg_path_element_fmt_debug,
                az_svg_line_delete,
                az_svg_line_deep_copy,
                az_svg_line_fmt_debug,
                az_svg_point_delete,
                az_svg_point_deep_copy,
                az_svg_point_fmt_debug,
                az_svg_vertex_delete,
                az_svg_vertex_deep_copy,
                az_svg_vertex_fmt_debug,
                az_svg_quadratic_curve_delete,
                az_svg_quadratic_curve_deep_copy,
                az_svg_quadratic_curve_fmt_debug,
                az_svg_cubic_curve_delete,
                az_svg_cubic_curve_deep_copy,
                az_svg_cubic_curve_fmt_debug,
                az_svg_rect_delete,
                az_svg_rect_deep_copy,
                az_svg_rect_fmt_debug,
                az_tesselated_cpu_svg_node_delete,
                az_tesselated_cpu_svg_node_deep_copy,
                az_tesselated_cpu_svg_node_fmt_debug,
                az_tesselated_gpu_svg_node_delete,
                az_tesselated_gpu_svg_node_fmt_debug,
                az_svg_line_cap_delete,
                az_svg_line_cap_deep_copy,
                az_svg_line_cap_fmt_debug,
                az_svg_parse_options_default,
                az_svg_parse_options_delete,
                az_svg_parse_options_deep_copy,
                az_svg_parse_options_fmt_debug,
                az_shape_rendering_delete,
                az_shape_rendering_deep_copy,
                az_shape_rendering_fmt_debug,
                az_text_rendering_delete,
                az_text_rendering_deep_copy,
                az_text_rendering_fmt_debug,
                az_image_rendering_delete,
                az_image_rendering_deep_copy,
                az_image_rendering_fmt_debug,
                az_font_database_delete,
                az_font_database_deep_copy,
                az_font_database_fmt_debug,
                az_svg_render_options_default,
                az_svg_render_options_delete,
                az_svg_render_options_deep_copy,
                az_svg_render_options_fmt_debug,
                az_svg_fit_to_delete,
                az_svg_fit_to_deep_copy,
                az_svg_fit_to_fmt_debug,
                az_svg_parse,
                az_svg_delete,
                az_svg_deep_copy,
                az_svg_fmt_debug,
                az_svg_xml_node_delete,
                az_svg_xml_node_deep_copy,
                az_svg_xml_node_fmt_debug,
                az_svg_line_join_delete,
                az_svg_line_join_deep_copy,
                az_svg_line_join_fmt_debug,
                az_svg_dash_pattern_delete,
                az_svg_dash_pattern_deep_copy,
                az_svg_dash_pattern_fmt_debug,
                az_svg_style_delete,
                az_svg_style_deep_copy,
                az_svg_style_fmt_debug,
                az_svg_fill_style_delete,
                az_svg_fill_style_deep_copy,
                az_svg_fill_style_fmt_debug,
                az_svg_stroke_style_delete,
                az_svg_stroke_style_deep_copy,
                az_svg_stroke_style_fmt_debug,
                az_svg_node_id_delete,
                az_svg_node_id_deep_copy,
                az_svg_node_id_fmt_debug,
                az_drop_check_ptr_ptr_delete,
                az_drop_check_ptr_ptr_fmt_debug,
                az_arc_mutex_ref_any_ptr_delete,
                az_arc_mutex_ref_any_ptr_fmt_debug,
                az_timer_delete,
                az_timer_deep_copy,
                az_timer_fmt_debug,
                az_task_ptr_new,
                az_task_ptr_then,
                az_task_ptr_delete,
                az_task_ptr_fmt_debug,
                az_thread_ptr_new,
                az_thread_ptr_block,
                az_thread_ptr_delete,
                az_thread_ptr_fmt_debug,
                az_drop_check_ptr_delete,
                az_drop_check_ptr_fmt_debug,
                az_timer_id_delete,
                az_timer_id_deep_copy,
                az_timer_id_fmt_debug,
                az_terminate_timer_delete,
                az_terminate_timer_deep_copy,
                az_terminate_timer_fmt_debug,
                az_block_error_delete,
                az_block_error_deep_copy,
                az_block_error_fmt_debug,
                az_task_bar_icon_delete,
                az_task_bar_icon_deep_copy,
                az_task_bar_icon_fmt_debug,
                az_x_window_type_delete,
                az_x_window_type_deep_copy,
                az_x_window_type_fmt_debug,
                az_physical_position_i32_delete,
                az_physical_position_i32_deep_copy,
                az_physical_position_i32_fmt_debug,
                az_physical_position_i32_partial_eq,
                az_physical_position_i32_partial_cmp,
                az_physical_position_i32_cmp,
                az_physical_position_i32_hash,
                az_physical_size_u32_delete,
                az_physical_size_u32_deep_copy,
                az_physical_size_u32_fmt_debug,
                az_physical_size_u32_partial_eq,
                az_physical_size_u32_partial_cmp,
                az_physical_size_u32_cmp,
                az_physical_size_u32_hash,
                az_logical_position_delete,
                az_logical_position_deep_copy,
                az_logical_position_fmt_debug,
                az_logical_rect_delete,
                az_logical_rect_deep_copy,
                az_logical_rect_fmt_debug,
                az_icon_key_delete,
                az_icon_key_deep_copy,
                az_icon_key_fmt_debug,
                az_small_window_icon_bytes_delete,
                az_small_window_icon_bytes_deep_copy,
                az_small_window_icon_bytes_fmt_debug,
                az_large_window_icon_bytes_delete,
                az_large_window_icon_bytes_deep_copy,
                az_large_window_icon_bytes_fmt_debug,
                az_window_icon_delete,
                az_window_icon_deep_copy,
                az_window_icon_fmt_debug,
                az_virtual_key_code_delete,
                az_virtual_key_code_deep_copy,
                az_virtual_key_code_fmt_debug,
                az_accelerator_key_delete,
                az_accelerator_key_deep_copy,
                az_accelerator_key_fmt_debug,
                az_window_size_delete,
                az_window_size_deep_copy,
                az_window_size_fmt_debug,
                az_window_flags_delete,
                az_window_flags_deep_copy,
                az_window_flags_fmt_debug,
                az_debug_state_delete,
                az_debug_state_deep_copy,
                az_debug_state_fmt_debug,
                az_keyboard_state_delete,
                az_keyboard_state_deep_copy,
                az_keyboard_state_fmt_debug,
                az_mouse_cursor_type_delete,
                az_mouse_cursor_type_deep_copy,
                az_mouse_cursor_type_fmt_debug,
                az_cursor_position_delete,
                az_cursor_position_deep_copy,
                az_cursor_position_fmt_debug,
                az_mouse_state_delete,
                az_mouse_state_deep_copy,
                az_mouse_state_fmt_debug,
                az_platform_specific_options_delete,
                az_platform_specific_options_deep_copy,
                az_platform_specific_options_fmt_debug,
                az_windows_window_options_delete,
                az_windows_window_options_deep_copy,
                az_windows_window_options_fmt_debug,
                az_wayland_theme_delete,
                az_wayland_theme_deep_copy,
                az_wayland_theme_fmt_debug,
                az_renderer_type_delete,
                az_renderer_type_deep_copy,
                az_renderer_type_fmt_debug,
                az_string_pair_delete,
                az_string_pair_deep_copy,
                az_string_pair_fmt_debug,
                az_linux_window_options_delete,
                az_linux_window_options_deep_copy,
                az_linux_window_options_fmt_debug,
                az_mac_window_options_delete,
                az_mac_window_options_deep_copy,
                az_mac_window_options_fmt_debug,
                az_wasm_window_options_delete,
                az_wasm_window_options_deep_copy,
                az_wasm_window_options_fmt_debug,
                az_full_screen_mode_delete,
                az_full_screen_mode_deep_copy,
                az_full_screen_mode_fmt_debug,
                az_window_state_new,
                az_window_state_delete,
                az_window_state_deep_copy,
                az_window_state_fmt_debug,
                az_logical_size_delete,
                az_logical_size_deep_copy,
                az_logical_size_fmt_debug,
                az_hot_reload_options_delete,
                az_hot_reload_options_deep_copy,
                az_hot_reload_options_fmt_debug,
                az_window_create_options_new,
                az_window_create_options_new_hot_reload,
                az_window_create_options_delete,
                az_window_create_options_deep_copy,
                az_window_create_options_fmt_debug,
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
