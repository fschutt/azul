    use core::ffi::c_void;

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
            unsafe { core::str::from_utf8_unchecked(self.as_bytes()) }
        }
        #[inline]
        pub fn as_bytes(&self) -> &[u8] {
            unsafe { core::slice::from_raw_parts(self.vec.ptr, self.vec.len) }
        }
    }

    impl ::core::fmt::Debug for AzCallback                   { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzLayoutCallback             { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzGlCallback                 { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzIFrameCallback             { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzTimerCallback              { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzWriteBackCallback          { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzRefAny                     {
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            write!(f, "RefAny {{\r\n")?;
            write!(f, "    _internal_ptr: 0x{:x}\r\n", self._internal_ptr as usize)?;
            write!(f, "    _internal_len: {}\r\n", self._internal_len)?;
            write!(f, "    _internal_layout_size: {}\r\n", self._internal_layout_size)?;
            write!(f, "    _internal_layout_align: {}\r\n", self._internal_layout_align)?;
            write!(f, "    type_name: \"{}\"\r\n", self.type_name.as_str())?;
            write!(f, "    type_id: {}\r\n", self.type_id)?;
            write!(f, "    sharing_info: {:#?}\r\n", self.sharing_info)?;
            write!(f, "    custom_destructor: 0x{:x}\r\n", self.custom_destructor as usize)?;
            write!(f, "}}\r\n")?;
            Ok(())
        }
    }    /// Re-export of rust-allocated (stack based) `String` struct
    #[repr(C)] #[derive(Debug)] pub struct AzString {
        pub vec: AzU8Vec,
    }
    /// Wrapper over a Rust-allocated `Vec<IdOrClass>`
    #[repr(C)] #[derive(Debug)] pub struct AzIdOrClassVec {
        pub(crate) ptr: *mut AzIdOrClass,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<NodeDataInlineCssProperty>`
    #[repr(C)] #[derive(Debug)] pub struct AzNodeDataInlineCssPropertyVec {
        pub(crate) ptr: *mut AzNodeDataInlineCssProperty,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<StyleBackgroundContent>`
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBackgroundContentVec {
        pub(crate) ptr: *mut AzStyleBackgroundContent,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<StyleBackgroundPosition>`
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBackgroundPositionVec {
        pub(crate) ptr: *mut AzStyleBackgroundPosition,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<StyleBackgroundRepeat>`
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBackgroundRepeatVec {
        pub(crate) ptr: *mut AzStyleBackgroundRepeat,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<StyleBackgroundSize>`
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBackgroundSizeVec {
        pub(crate) ptr: *mut AzStyleBackgroundSize,
        pub len: usize,
        pub cap: usize,
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
    /// Wrapper over a Rust-allocated `LinearColorStopVec`
    #[repr(C)] #[derive(Debug)] pub struct AzLinearColorStopVec {
        pub(crate) ptr: *mut AzLinearColorStop,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `RadialColorStopVec`
    #[repr(C)] #[derive(Debug)] pub struct AzRadialColorStopVec {
        pub(crate) ptr: *mut AzRadialColorStop,
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
    /// Re-export of rust-allocated (stack based) `OptionPercentageValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionPercentageValue {
        None,
        Some(AzPercentageValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionAngleValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionAngleValue {
        None,
        Some(AzAngleValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionRendererOptions` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionRendererOptions {
        None,
        Some(AzRendererOptions),
    }
    /// Re-export of rust-allocated (stack based) `OptionCallback` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzOptionCallback {
        None,
        Some(AzCallback),
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
    #[repr(C)]  pub struct AzAtomicRefCount {
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
        pub sharing_info: AzAtomicRefCount,
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
        ScrollbarStyle,
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
    /// Re-export of rust-allocated (stack based) `StyleBoxShadow` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyleBoxShadow {
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
    /// Re-export of rust-allocated (stack based) `LayoutFlexWrap` struct
    #[repr(C)] #[derive(Debug)] pub enum AzLayoutFlexWrap {
        Wrap,
        NoWrap,
    }
    /// Re-export of rust-allocated (stack based) `LayoutOverflow` struct
    #[repr(C)] #[derive(Debug)] pub enum AzLayoutOverflow {
        Scroll,
        Auto,
        Hidden,
        Visible,
    }
    /// Re-export of rust-allocated (stack based) `PercentageValue` struct
    #[repr(C)] #[derive(Debug)] pub struct AzPercentageValue {
        pub number: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `AngleMetric` struct
    #[repr(C)] #[derive(Debug)] pub enum AzAngleMetric {
        Degree,
        Radians,
        Grad,
        Turn,
        Percent,
    }
    /// Re-export of rust-allocated (stack based) `AngleValue` struct
    #[repr(C)] #[derive(Debug)] pub struct AzAngleValue {
        pub metric: AzAngleMetric,
        pub number: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `LinearColorStop` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLinearColorStop {
        pub offset: AzOptionPercentageValue,
        pub color: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `RadialColorStop` struct
    #[repr(C)] #[derive(Debug)] pub struct AzRadialColorStop {
        pub offset: AzOptionAngleValue,
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
        pub stops: AzLinearColorStopVec,
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
        pub stops: AzLinearColorStopVec,
    }
    /// Re-export of rust-allocated (stack based) `ConicGradient` struct
    #[repr(C)] #[derive(Debug)] pub struct AzConicGradient {
        pub extend_mode: AzExtendMode,
        pub center: AzStyleBackgroundPosition,
        pub angle: AzAngleValue,
        pub stops: AzRadialColorStopVec,
    }
    /// Re-export of rust-allocated (stack based) `CssImageId` struct
    #[repr(C)] #[derive(Debug)] pub struct AzCssImageId {
        pub inner: AzString,
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundContent` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBackgroundContent {
        LinearGradient(AzLinearGradient),
        RadialGradient(AzRadialGradient),
        ConicGradient(AzConicGradient),
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
    /// Re-export of rust-allocated (stack based) `LayoutBorderBottomWidth` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutBorderBottomWidth {
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
    /// Re-export of rust-allocated (stack based) `LayoutBorderLeftWidth` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutBorderLeftWidth {
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
    /// Re-export of rust-allocated (stack based) `LayoutBorderRightWidth` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutBorderRightWidth {
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
    /// Re-export of rust-allocated (stack based) `LayoutBorderTopWidth` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLayoutBorderTopWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `ScrollbarInfo` struct
    #[repr(C)] #[derive(Debug)] pub struct AzScrollbarInfo {
        pub width: AzLayoutWidth,
        pub padding_left: AzLayoutPaddingLeft,
        pub padding_right: AzLayoutPaddingRight,
        pub track: AzStyleBackgroundContent,
        pub thumb: AzStyleBackgroundContent,
        pub button: AzStyleBackgroundContent,
        pub corner: AzStyleBackgroundContent,
        pub resizer: AzStyleBackgroundContent,
    }
    /// Re-export of rust-allocated (stack based) `ScrollbarStyle` struct
    #[repr(C)] #[derive(Debug)] pub struct AzScrollbarStyle {
        pub horizontal: AzScrollbarInfo,
        pub vertical: AzScrollbarInfo,
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
    /// Re-export of rust-allocated (stack based) `StyleBoxShadowValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBoxShadowValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBoxShadow),
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
    /// Re-export of rust-allocated (stack based) `LayoutFlexWrapValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutFlexWrapValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFlexWrap),
    }
    /// Re-export of rust-allocated (stack based) `LayoutOverflowValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutOverflowValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutOverflow),
    }
    /// Re-export of rust-allocated (stack based) `ScrollbarStyleValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzScrollbarStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzScrollbarStyle),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundContentVecValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBackgroundContentVecValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundContentVec),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundPositionVecValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBackgroundPositionVecValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundPositionVec),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundRepeatVecValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBackgroundRepeatVecValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundRepeatVec),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundSizeVecValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzStyleBackgroundSizeVecValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundSizeVec),
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
    /// Re-export of rust-allocated (stack based) `LayoutBorderBottomWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutBorderBottomWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBorderBottomWidth),
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
    /// Re-export of rust-allocated (stack based) `LayoutBorderLeftWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutBorderLeftWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBorderLeftWidth),
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
    /// Re-export of rust-allocated (stack based) `LayoutBorderRightWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutBorderRightWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBorderRightWidth),
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
    /// Re-export of rust-allocated (stack based) `LayoutBorderTopWidthValue` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzLayoutBorderTopWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBorderTopWidth),
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
        FlexWrap(AzLayoutFlexWrapValue),
        FlexDirection(AzLayoutFlexDirectionValue),
        FlexGrow(AzLayoutFlexGrowValue),
        FlexShrink(AzLayoutFlexShrinkValue),
        JustifyContent(AzLayoutJustifyContentValue),
        AlignItems(AzLayoutAlignItemsValue),
        AlignContent(AzLayoutAlignContentValue),
        BackgroundContent(AzStyleBackgroundContentVecValue),
        BackgroundPosition(AzStyleBackgroundPositionVecValue),
        BackgroundSize(AzStyleBackgroundSizeVecValue),
        BackgroundRepeat(AzStyleBackgroundRepeatVecValue),
        OverflowX(AzLayoutOverflowValue),
        OverflowY(AzLayoutOverflowValue),
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
        BorderTopWidth(AzLayoutBorderTopWidthValue),
        BorderRightWidth(AzLayoutBorderRightWidthValue),
        BorderLeftWidth(AzLayoutBorderLeftWidthValue),
        BorderBottomWidth(AzLayoutBorderBottomWidthValue),
        BoxShadowLeft(AzStyleBoxShadowValue),
        BoxShadowRight(AzStyleBoxShadowValue),
        BoxShadowTop(AzStyleBoxShadowValue),
        BoxShadowBottom(AzStyleBoxShadowValue),
        ScrollbarStyle(AzScrollbarStyleValue),
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
    /// Re-export of rust-allocated (stack based) `CssPropertySource` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzCssPropertySource {
        Css(AzCssPath),
        Inline,
    }
    /// Re-export of rust-allocated (stack based) `StyledNodeState` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyledNodeState {
        pub normal: bool,
        pub hover: bool,
        pub active: bool,
        pub focused: bool,
    }
    /// Re-export of rust-allocated (stack based) `StyledNode` struct
    #[repr(C)] #[derive(Debug)] pub struct AzStyledNode {
        pub state: AzStyledNodeState,
        pub tag_id: AzOptionTagId,
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
    /// Re-export of rust-allocated (stack based) `IdOrClass` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzIdOrClass {
        Id(AzString),
        Class(AzString),
    }
    /// Re-export of rust-allocated (stack based) `NodeDataInlineCssProperty` struct
    #[repr(C, u8)] #[derive(Debug)] pub enum AzNodeDataInlineCssProperty {
        Normal(AzCssProperty),
        Active(AzCssProperty),
        Focus(AzCssProperty),
        Hover(AzCssProperty),
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
        pub ids_and_classes: AzIdOrClassVec,
        pub callbacks: AzCallbackDataVec,
        pub inline_css_props: AzCssPropertyVec,
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
        pub renderer_type: AzRendererType,
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
    /// Re-export of rust-allocated (stack based) `RendererOptions` struct
    #[repr(C)] #[derive(Debug)] pub struct AzRendererOptions {
        pub vsync: AzVsync,
        pub srgb: AzSrgb,
        pub hw_accel: AzHwAcceleration,
    }
    /// Re-export of rust-allocated (stack based) `Vsync` struct
    #[repr(C)] #[derive(Debug)] pub enum AzVsync {
        Enabled,
        Disabled,
    }
    /// Re-export of rust-allocated (stack based) `Srgb` struct
    #[repr(C)] #[derive(Debug)] pub enum AzSrgb {
        Enabled,
        Disabled,
    }
    /// Re-export of rust-allocated (stack based) `HwAcceleration` struct
    #[repr(C)] #[derive(Debug)] pub enum AzHwAcceleration {
        Enabled,
        Disabled,
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
        pub echo_driver_messages: bool,
        pub show_overdraw: bool,
        pub gpu_cache_dbg: bool,
        pub texture_cache_dbg_clear_evicted: bool,
        pub picture_caching_dbg: bool,
        pub primitive_dbg: bool,
        pub zoom_dbg: bool,
        pub small_screen: bool,
        pub disable_opaque_pass: bool,
        pub disable_alpha_pass: bool,
        pub disable_clip_masks: bool,
        pub disable_text_prims: bool,
        pub disable_gradient_prims: bool,
        pub obscure_images: bool,
        pub glyph_flashing: bool,
        pub smart_profiler: bool,
        pub invalidation_dbg: bool,
        pub tile_cache_logging_dbg: bool,
        pub profiler_capture: bool,
        pub force_picture_invalidation: bool,
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
    #[repr(C)] #[derive(Debug)] pub enum AzRendererType {
        Hardware,
        Software,
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
        pub renderer_options: AzRendererOptions,
        pub background_color: AzColorU,
        pub layout_callback: AzLayoutCallback,
        pub close_callback: AzOptionCallback,
    }
    /// Re-export of rust-allocated (stack based) `LogicalSize` struct
    #[repr(C)] #[derive(Debug)] pub struct AzLogicalSize {
        pub width: f32,
        pub height: f32,
    }
    /// Re-export of rust-allocated (stack based) `WindowCreateOptions` struct
    #[repr(C)] #[derive(Debug)] pub struct AzWindowCreateOptions {
        pub state: AzWindowState,
        pub renderer_type: AzOptionRendererOptions,
        pub theme: AzOptionWindowTheme,
        pub create_callback: AzOptionCallback,
    }


    #[link(name="azul")]
    extern "C" {
        pub(crate) fn az_string_from_utf8_unchecked(_:  *const u8, _:  usize) -> AzString;
        pub(crate) fn az_string_from_utf8_lossy(_:  *const u8, _:  usize) -> AzString;
        pub(crate) fn az_string_into_bytes(_:  AzString) -> AzU8Vec;
        pub(crate) fn az_string_delete(_:  &mut AzString);
        pub(crate) fn az_string_deep_copy(_:  &AzString) -> AzString;
        pub(crate) fn az_id_or_class_vec_new() -> AzIdOrClassVec;
        pub(crate) fn az_id_or_class_vec_with_capacity(_:  usize) -> AzIdOrClassVec;
        pub(crate) fn az_id_or_class_vec_copy_from(_:  *const AzIdOrClass, _:  usize) -> AzIdOrClassVec;
        pub(crate) fn az_id_or_class_vec_delete(_:  &mut AzIdOrClassVec);
        pub(crate) fn az_id_or_class_vec_deep_copy(_:  &AzIdOrClassVec) -> AzIdOrClassVec;
        pub(crate) fn az_node_data_inline_css_property_vec_new() -> AzNodeDataInlineCssPropertyVec;
        pub(crate) fn az_node_data_inline_css_property_vec_with_capacity(_:  usize) -> AzNodeDataInlineCssPropertyVec;
        pub(crate) fn az_node_data_inline_css_property_vec_copy_from(_:  *const AzNodeDataInlineCssProperty, _:  usize) -> AzNodeDataInlineCssPropertyVec;
        pub(crate) fn az_node_data_inline_css_property_vec_delete(_:  &mut AzNodeDataInlineCssPropertyVec);
        pub(crate) fn az_node_data_inline_css_property_vec_deep_copy(_:  &AzNodeDataInlineCssPropertyVec) -> AzNodeDataInlineCssPropertyVec;
        pub(crate) fn az_style_background_content_vec_new() -> AzStyleBackgroundContentVec;
        pub(crate) fn az_style_background_content_vec_with_capacity(_:  usize) -> AzStyleBackgroundContentVec;
        pub(crate) fn az_style_background_content_vec_copy_from(_:  *const AzStyleBackgroundContent, _:  usize) -> AzStyleBackgroundContentVec;
        pub(crate) fn az_style_background_content_vec_delete(_:  &mut AzStyleBackgroundContentVec);
        pub(crate) fn az_style_background_content_vec_deep_copy(_:  &AzStyleBackgroundContentVec) -> AzStyleBackgroundContentVec;
        pub(crate) fn az_style_background_position_vec_new() -> AzStyleBackgroundPositionVec;
        pub(crate) fn az_style_background_position_vec_with_capacity(_:  usize) -> AzStyleBackgroundPositionVec;
        pub(crate) fn az_style_background_position_vec_copy_from(_:  *const AzStyleBackgroundPosition, _:  usize) -> AzStyleBackgroundPositionVec;
        pub(crate) fn az_style_background_position_vec_delete(_:  &mut AzStyleBackgroundPositionVec);
        pub(crate) fn az_style_background_position_vec_deep_copy(_:  &AzStyleBackgroundPositionVec) -> AzStyleBackgroundPositionVec;
        pub(crate) fn az_style_background_repeat_vec_new() -> AzStyleBackgroundRepeatVec;
        pub(crate) fn az_style_background_repeat_vec_with_capacity(_:  usize) -> AzStyleBackgroundRepeatVec;
        pub(crate) fn az_style_background_repeat_vec_copy_from(_:  *const AzStyleBackgroundRepeat, _:  usize) -> AzStyleBackgroundRepeatVec;
        pub(crate) fn az_style_background_repeat_vec_delete(_:  &mut AzStyleBackgroundRepeatVec);
        pub(crate) fn az_style_background_repeat_vec_deep_copy(_:  &AzStyleBackgroundRepeatVec) -> AzStyleBackgroundRepeatVec;
        pub(crate) fn az_style_background_size_vec_new() -> AzStyleBackgroundSizeVec;
        pub(crate) fn az_style_background_size_vec_with_capacity(_:  usize) -> AzStyleBackgroundSizeVec;
        pub(crate) fn az_style_background_size_vec_copy_from(_:  *const AzStyleBackgroundSize, _:  usize) -> AzStyleBackgroundSizeVec;
        pub(crate) fn az_style_background_size_vec_delete(_:  &mut AzStyleBackgroundSizeVec);
        pub(crate) fn az_style_background_size_vec_deep_copy(_:  &AzStyleBackgroundSizeVec) -> AzStyleBackgroundSizeVec;
        pub(crate) fn az_style_transform_vec_new() -> AzStyleTransformVec;
        pub(crate) fn az_style_transform_vec_with_capacity(_:  usize) -> AzStyleTransformVec;
        pub(crate) fn az_style_transform_vec_copy_from(_:  *const AzStyleTransform, _:  usize) -> AzStyleTransformVec;
        pub(crate) fn az_style_transform_vec_delete(_:  &mut AzStyleTransformVec);
        pub(crate) fn az_style_transform_vec_deep_copy(_:  &AzStyleTransformVec) -> AzStyleTransformVec;
        pub(crate) fn az_content_group_vec_new() -> AzContentGroupVec;
        pub(crate) fn az_content_group_vec_with_capacity(_:  usize) -> AzContentGroupVec;
        pub(crate) fn az_content_group_vec_copy_from(_:  *const AzContentGroup, _:  usize) -> AzContentGroupVec;
        pub(crate) fn az_content_group_vec_delete(_:  &mut AzContentGroupVec);
        pub(crate) fn az_content_group_vec_deep_copy(_:  &AzContentGroupVec) -> AzContentGroupVec;
        pub(crate) fn az_css_property_vec_new() -> AzCssPropertyVec;
        pub(crate) fn az_css_property_vec_with_capacity(_:  usize) -> AzCssPropertyVec;
        pub(crate) fn az_css_property_vec_copy_from(_:  *const AzCssProperty, _:  usize) -> AzCssPropertyVec;
        pub(crate) fn az_css_property_vec_delete(_:  &mut AzCssPropertyVec);
        pub(crate) fn az_css_property_vec_deep_copy(_:  &AzCssPropertyVec) -> AzCssPropertyVec;
        pub(crate) fn az_svg_multi_polygon_vec_new() -> AzSvgMultiPolygonVec;
        pub(crate) fn az_svg_multi_polygon_vec_with_capacity(_:  usize) -> AzSvgMultiPolygonVec;
        pub(crate) fn az_svg_multi_polygon_vec_copy_from(_:  *const AzSvgMultiPolygon, _:  usize) -> AzSvgMultiPolygonVec;
        pub(crate) fn az_svg_multi_polygon_vec_delete(_:  &mut AzSvgMultiPolygonVec);
        pub(crate) fn az_svg_multi_polygon_vec_deep_copy(_:  &AzSvgMultiPolygonVec) -> AzSvgMultiPolygonVec;
        pub(crate) fn az_svg_path_vec_new() -> AzSvgPathVec;
        pub(crate) fn az_svg_path_vec_with_capacity(_:  usize) -> AzSvgPathVec;
        pub(crate) fn az_svg_path_vec_copy_from(_:  *const AzSvgPath, _:  usize) -> AzSvgPathVec;
        pub(crate) fn az_svg_path_vec_delete(_:  &mut AzSvgPathVec);
        pub(crate) fn az_svg_path_vec_deep_copy(_:  &AzSvgPathVec) -> AzSvgPathVec;
        pub(crate) fn az_vertex_attribute_vec_new() -> AzVertexAttributeVec;
        pub(crate) fn az_vertex_attribute_vec_with_capacity(_:  usize) -> AzVertexAttributeVec;
        pub(crate) fn az_vertex_attribute_vec_copy_from(_:  *const AzVertexAttribute, _:  usize) -> AzVertexAttributeVec;
        pub(crate) fn az_vertex_attribute_vec_delete(_:  &mut AzVertexAttributeVec);
        pub(crate) fn az_vertex_attribute_vec_deep_copy(_:  &AzVertexAttributeVec) -> AzVertexAttributeVec;
        pub(crate) fn az_svg_path_element_vec_new() -> AzSvgPathElementVec;
        pub(crate) fn az_svg_path_element_vec_with_capacity(_:  usize) -> AzSvgPathElementVec;
        pub(crate) fn az_svg_path_element_vec_copy_from(_:  *const AzSvgPathElement, _:  usize) -> AzSvgPathElementVec;
        pub(crate) fn az_svg_path_element_vec_delete(_:  &mut AzSvgPathElementVec);
        pub(crate) fn az_svg_path_element_vec_deep_copy(_:  &AzSvgPathElementVec) -> AzSvgPathElementVec;
        pub(crate) fn az_svg_vertex_vec_new() -> AzSvgVertexVec;
        pub(crate) fn az_svg_vertex_vec_with_capacity(_:  usize) -> AzSvgVertexVec;
        pub(crate) fn az_svg_vertex_vec_copy_from(_:  *const AzSvgVertex, _:  usize) -> AzSvgVertexVec;
        pub(crate) fn az_svg_vertex_vec_delete(_:  &mut AzSvgVertexVec);
        pub(crate) fn az_svg_vertex_vec_deep_copy(_:  &AzSvgVertexVec) -> AzSvgVertexVec;
        pub(crate) fn az_u32_vec_new() -> AzU32Vec;
        pub(crate) fn az_u32_vec_with_capacity(_:  usize) -> AzU32Vec;
        pub(crate) fn az_u32_vec_copy_from(_:  *const u32, _:  usize) -> AzU32Vec;
        pub(crate) fn az_u32_vec_delete(_:  &mut AzU32Vec);
        pub(crate) fn az_u32_vec_deep_copy(_:  &AzU32Vec) -> AzU32Vec;
        pub(crate) fn az_x_window_type_vec_new() -> AzXWindowTypeVec;
        pub(crate) fn az_x_window_type_vec_with_capacity(_:  usize) -> AzXWindowTypeVec;
        pub(crate) fn az_x_window_type_vec_copy_from(_:  *const AzXWindowType, _:  usize) -> AzXWindowTypeVec;
        pub(crate) fn az_x_window_type_vec_delete(_:  &mut AzXWindowTypeVec);
        pub(crate) fn az_x_window_type_vec_deep_copy(_:  &AzXWindowTypeVec) -> AzXWindowTypeVec;
        pub(crate) fn az_virtual_key_code_vec_new() -> AzVirtualKeyCodeVec;
        pub(crate) fn az_virtual_key_code_vec_with_capacity(_:  usize) -> AzVirtualKeyCodeVec;
        pub(crate) fn az_virtual_key_code_vec_copy_from(_:  *const AzVirtualKeyCode, _:  usize) -> AzVirtualKeyCodeVec;
        pub(crate) fn az_virtual_key_code_vec_delete(_:  &mut AzVirtualKeyCodeVec);
        pub(crate) fn az_virtual_key_code_vec_deep_copy(_:  &AzVirtualKeyCodeVec) -> AzVirtualKeyCodeVec;
        pub(crate) fn az_cascade_info_vec_new() -> AzCascadeInfoVec;
        pub(crate) fn az_cascade_info_vec_with_capacity(_:  usize) -> AzCascadeInfoVec;
        pub(crate) fn az_cascade_info_vec_copy_from(_:  *const AzCascadeInfo, _:  usize) -> AzCascadeInfoVec;
        pub(crate) fn az_cascade_info_vec_delete(_:  &mut AzCascadeInfoVec);
        pub(crate) fn az_cascade_info_vec_deep_copy(_:  &AzCascadeInfoVec) -> AzCascadeInfoVec;
        pub(crate) fn az_scan_code_vec_new() -> AzScanCodeVec;
        pub(crate) fn az_scan_code_vec_with_capacity(_:  usize) -> AzScanCodeVec;
        pub(crate) fn az_scan_code_vec_copy_from(_:  *const u32, _:  usize) -> AzScanCodeVec;
        pub(crate) fn az_scan_code_vec_delete(_:  &mut AzScanCodeVec);
        pub(crate) fn az_scan_code_vec_deep_copy(_:  &AzScanCodeVec) -> AzScanCodeVec;
        pub(crate) fn az_css_declaration_vec_new() -> AzCssDeclarationVec;
        pub(crate) fn az_css_declaration_vec_with_capacity(_:  usize) -> AzCssDeclarationVec;
        pub(crate) fn az_css_declaration_vec_copy_from(_:  *const AzCssDeclaration, _:  usize) -> AzCssDeclarationVec;
        pub(crate) fn az_css_declaration_vec_delete(_:  &mut AzCssDeclarationVec);
        pub(crate) fn az_css_declaration_vec_deep_copy(_:  &AzCssDeclarationVec) -> AzCssDeclarationVec;
        pub(crate) fn az_css_path_selector_vec_new() -> AzCssPathSelectorVec;
        pub(crate) fn az_css_path_selector_vec_with_capacity(_:  usize) -> AzCssPathSelectorVec;
        pub(crate) fn az_css_path_selector_vec_copy_from(_:  *const AzCssPathSelector, _:  usize) -> AzCssPathSelectorVec;
        pub(crate) fn az_css_path_selector_vec_delete(_:  &mut AzCssPathSelectorVec);
        pub(crate) fn az_css_path_selector_vec_deep_copy(_:  &AzCssPathSelectorVec) -> AzCssPathSelectorVec;
        pub(crate) fn az_stylesheet_vec_new() -> AzStylesheetVec;
        pub(crate) fn az_stylesheet_vec_with_capacity(_:  usize) -> AzStylesheetVec;
        pub(crate) fn az_stylesheet_vec_copy_from(_:  *const AzStylesheet, _:  usize) -> AzStylesheetVec;
        pub(crate) fn az_stylesheet_vec_delete(_:  &mut AzStylesheetVec);
        pub(crate) fn az_stylesheet_vec_deep_copy(_:  &AzStylesheetVec) -> AzStylesheetVec;
        pub(crate) fn az_css_rule_block_vec_new() -> AzCssRuleBlockVec;
        pub(crate) fn az_css_rule_block_vec_with_capacity(_:  usize) -> AzCssRuleBlockVec;
        pub(crate) fn az_css_rule_block_vec_copy_from(_:  *const AzCssRuleBlock, _:  usize) -> AzCssRuleBlockVec;
        pub(crate) fn az_css_rule_block_vec_delete(_:  &mut AzCssRuleBlockVec);
        pub(crate) fn az_css_rule_block_vec_deep_copy(_:  &AzCssRuleBlockVec) -> AzCssRuleBlockVec;
        pub(crate) fn az_u8_vec_new() -> AzU8Vec;
        pub(crate) fn az_u8_vec_with_capacity(_:  usize) -> AzU8Vec;
        pub(crate) fn az_u8_vec_copy_from(_:  *const u8, _:  usize) -> AzU8Vec;
        pub(crate) fn az_u8_vec_delete(_:  &mut AzU8Vec);
        pub(crate) fn az_u8_vec_deep_copy(_:  &AzU8Vec) -> AzU8Vec;
        pub(crate) fn az_callback_data_vec_new() -> AzCallbackDataVec;
        pub(crate) fn az_callback_data_vec_with_capacity(_:  usize) -> AzCallbackDataVec;
        pub(crate) fn az_callback_data_vec_copy_from(_:  *const AzCallbackData, _:  usize) -> AzCallbackDataVec;
        pub(crate) fn az_callback_data_vec_delete(_:  &mut AzCallbackDataVec);
        pub(crate) fn az_callback_data_vec_deep_copy(_:  &AzCallbackDataVec) -> AzCallbackDataVec;
        pub(crate) fn az_debug_message_vec_new() -> AzDebugMessageVec;
        pub(crate) fn az_debug_message_vec_with_capacity(_:  usize) -> AzDebugMessageVec;
        pub(crate) fn az_debug_message_vec_copy_from(_:  *const AzDebugMessage, _:  usize) -> AzDebugMessageVec;
        pub(crate) fn az_debug_message_vec_delete(_:  &mut AzDebugMessageVec);
        pub(crate) fn az_debug_message_vec_deep_copy(_:  &AzDebugMessageVec) -> AzDebugMessageVec;
        pub(crate) fn az_g_luint_vec_new() -> AzGLuintVec;
        pub(crate) fn az_g_luint_vec_with_capacity(_:  usize) -> AzGLuintVec;
        pub(crate) fn az_g_luint_vec_copy_from(_:  *const u32, _:  usize) -> AzGLuintVec;
        pub(crate) fn az_g_luint_vec_delete(_:  &mut AzGLuintVec);
        pub(crate) fn az_g_luint_vec_deep_copy(_:  &AzGLuintVec) -> AzGLuintVec;
        pub(crate) fn az_g_lint_vec_new() -> AzGLintVec;
        pub(crate) fn az_g_lint_vec_with_capacity(_:  usize) -> AzGLintVec;
        pub(crate) fn az_g_lint_vec_copy_from(_:  *const i32, _:  usize) -> AzGLintVec;
        pub(crate) fn az_g_lint_vec_delete(_:  &mut AzGLintVec);
        pub(crate) fn az_g_lint_vec_deep_copy(_:  &AzGLintVec) -> AzGLintVec;
        pub(crate) fn az_dom_vec_new() -> AzDomVec;
        pub(crate) fn az_dom_vec_with_capacity(_:  usize) -> AzDomVec;
        pub(crate) fn az_dom_vec_copy_from(_:  *const AzDom, _:  usize) -> AzDomVec;
        pub(crate) fn az_dom_vec_delete(_:  &mut AzDomVec);
        pub(crate) fn az_dom_vec_deep_copy(_:  &AzDomVec) -> AzDomVec;
        pub(crate) fn az_string_vec_new() -> AzStringVec;
        pub(crate) fn az_string_vec_with_capacity(_:  usize) -> AzStringVec;
        pub(crate) fn az_string_vec_copy_from(_:  *const AzString, _:  usize) -> AzStringVec;
        pub(crate) fn az_string_vec_delete(_:  &mut AzStringVec);
        pub(crate) fn az_string_vec_deep_copy(_:  &AzStringVec) -> AzStringVec;
        pub(crate) fn az_string_pair_vec_new() -> AzStringPairVec;
        pub(crate) fn az_string_pair_vec_with_capacity(_:  usize) -> AzStringPairVec;
        pub(crate) fn az_string_pair_vec_copy_from(_:  *const AzStringPair, _:  usize) -> AzStringPairVec;
        pub(crate) fn az_string_pair_vec_delete(_:  &mut AzStringPairVec);
        pub(crate) fn az_string_pair_vec_deep_copy(_:  &AzStringPairVec) -> AzStringPairVec;
        pub(crate) fn az_linear_color_stop_vec_new() -> AzLinearColorStopVec;
        pub(crate) fn az_linear_color_stop_vec_with_capacity(_:  usize) -> AzLinearColorStopVec;
        pub(crate) fn az_linear_color_stop_vec_copy_from(_:  *const AzLinearColorStop, _:  usize) -> AzLinearColorStopVec;
        pub(crate) fn az_linear_color_stop_vec_delete(_:  &mut AzLinearColorStopVec);
        pub(crate) fn az_linear_color_stop_vec_deep_copy(_:  &AzLinearColorStopVec) -> AzLinearColorStopVec;
        pub(crate) fn az_radial_color_stop_vec_new() -> AzRadialColorStopVec;
        pub(crate) fn az_radial_color_stop_vec_with_capacity(_:  usize) -> AzRadialColorStopVec;
        pub(crate) fn az_radial_color_stop_vec_copy_from(_:  *const AzRadialColorStop, _:  usize) -> AzRadialColorStopVec;
        pub(crate) fn az_radial_color_stop_vec_delete(_:  &mut AzRadialColorStopVec);
        pub(crate) fn az_radial_color_stop_vec_deep_copy(_:  &AzRadialColorStopVec) -> AzRadialColorStopVec;
        pub(crate) fn az_node_id_vec_new() -> AzNodeIdVec;
        pub(crate) fn az_node_id_vec_with_capacity(_:  usize) -> AzNodeIdVec;
        pub(crate) fn az_node_id_vec_copy_from(_:  *const AzNodeId, _:  usize) -> AzNodeIdVec;
        pub(crate) fn az_node_id_vec_delete(_:  &mut AzNodeIdVec);
        pub(crate) fn az_node_id_vec_deep_copy(_:  &AzNodeIdVec) -> AzNodeIdVec;
        pub(crate) fn az_node_vec_new() -> AzNodeVec;
        pub(crate) fn az_node_vec_with_capacity(_:  usize) -> AzNodeVec;
        pub(crate) fn az_node_vec_copy_from(_:  *const AzNode, _:  usize) -> AzNodeVec;
        pub(crate) fn az_node_vec_delete(_:  &mut AzNodeVec);
        pub(crate) fn az_node_vec_deep_copy(_:  &AzNodeVec) -> AzNodeVec;
        pub(crate) fn az_styled_node_vec_new() -> AzStyledNodeVec;
        pub(crate) fn az_styled_node_vec_with_capacity(_:  usize) -> AzStyledNodeVec;
        pub(crate) fn az_styled_node_vec_copy_from(_:  *const AzStyledNode, _:  usize) -> AzStyledNodeVec;
        pub(crate) fn az_styled_node_vec_delete(_:  &mut AzStyledNodeVec);
        pub(crate) fn az_styled_node_vec_deep_copy(_:  &AzStyledNodeVec) -> AzStyledNodeVec;
        pub(crate) fn az_tag_ids_to_node_ids_mapping_vec_new() -> AzTagIdsToNodeIdsMappingVec;
        pub(crate) fn az_tag_ids_to_node_ids_mapping_vec_with_capacity(_:  usize) -> AzTagIdsToNodeIdsMappingVec;
        pub(crate) fn az_tag_ids_to_node_ids_mapping_vec_copy_from(_:  *const AzTagIdToNodeIdMapping, _:  usize) -> AzTagIdsToNodeIdsMappingVec;
        pub(crate) fn az_tag_ids_to_node_ids_mapping_vec_delete(_:  &mut AzTagIdsToNodeIdsMappingVec);
        pub(crate) fn az_tag_ids_to_node_ids_mapping_vec_deep_copy(_:  &AzTagIdsToNodeIdsMappingVec) -> AzTagIdsToNodeIdsMappingVec;
        pub(crate) fn az_parent_with_node_depth_vec_new() -> AzParentWithNodeDepthVec;
        pub(crate) fn az_parent_with_node_depth_vec_with_capacity(_:  usize) -> AzParentWithNodeDepthVec;
        pub(crate) fn az_parent_with_node_depth_vec_copy_from(_:  *const AzParentWithNodeDepth, _:  usize) -> AzParentWithNodeDepthVec;
        pub(crate) fn az_parent_with_node_depth_vec_delete(_:  &mut AzParentWithNodeDepthVec);
        pub(crate) fn az_parent_with_node_depth_vec_deep_copy(_:  &AzParentWithNodeDepthVec) -> AzParentWithNodeDepthVec;
        pub(crate) fn az_node_data_vec_new() -> AzNodeDataVec;
        pub(crate) fn az_node_data_vec_with_capacity(_:  usize) -> AzNodeDataVec;
        pub(crate) fn az_node_data_vec_copy_from(_:  *const AzNodeData, _:  usize) -> AzNodeDataVec;
        pub(crate) fn az_node_data_vec_delete(_:  &mut AzNodeDataVec);
        pub(crate) fn az_node_data_vec_deep_copy(_:  &AzNodeDataVec) -> AzNodeDataVec;
        pub(crate) fn az_option_ref_any_delete(_:  &mut AzOptionRefAny);
        pub(crate) fn az_option_ref_any_deep_copy(_:  &AzOptionRefAny) -> AzOptionRefAny;
        pub(crate) fn az_option_raw_image_delete(_:  &mut AzOptionRawImage);
        pub(crate) fn az_option_raw_image_deep_copy(_:  &AzOptionRawImage) -> AzOptionRawImage;
        pub(crate) fn az_option_wayland_theme_delete(_:  &mut AzOptionWaylandTheme);
        pub(crate) fn az_option_wayland_theme_deep_copy(_:  &AzOptionWaylandTheme) -> AzOptionWaylandTheme;
        pub(crate) fn az_option_task_bar_icon_delete(_:  &mut AzOptionTaskBarIcon);
        pub(crate) fn az_option_task_bar_icon_deep_copy(_:  &AzOptionTaskBarIcon) -> AzOptionTaskBarIcon;
        pub(crate) fn az_option_window_icon_delete(_:  &mut AzOptionWindowIcon);
        pub(crate) fn az_option_window_icon_deep_copy(_:  &AzOptionWindowIcon) -> AzOptionWindowIcon;
        pub(crate) fn az_option_string_delete(_:  &mut AzOptionString);
        pub(crate) fn az_option_string_deep_copy(_:  &AzOptionString) -> AzOptionString;
        pub(crate) fn az_option_dom_delete(_:  &mut AzOptionDom);
        pub(crate) fn az_option_dom_deep_copy(_:  &AzOptionDom) -> AzOptionDom;
        pub(crate) fn az_option_texture_delete(_:  &mut AzOptionTexture);
        pub(crate) fn az_option_image_mask_delete(_:  &mut AzOptionImageMask);
        pub(crate) fn az_option_image_mask_deep_copy(_:  &AzOptionImageMask) -> AzOptionImageMask;
        pub(crate) fn az_option_instant_ptr_delete(_:  &mut AzOptionInstantPtr);
        pub(crate) fn az_option_instant_ptr_deep_copy(_:  &AzOptionInstantPtr) -> AzOptionInstantPtr;
        pub(crate) fn az_option_u8_vec_ref_delete(_:  &mut AzOptionU8VecRef);
        pub(crate) fn az_result_svg_svg_parse_error_delete(_:  &mut AzResultSvgSvgParseError);
        pub(crate) fn az_result_svg_svg_parse_error_deep_copy(_:  &AzResultSvgSvgParseError) -> AzResultSvgSvgParseError;
        pub(crate) fn az_svg_parse_error_delete(_:  &mut AzSvgParseError);
        pub(crate) fn az_svg_parse_error_deep_copy(_:  &AzSvgParseError) -> AzSvgParseError;
        pub(crate) fn az_xml_error_delete(_:  &mut AzXmlError);
        pub(crate) fn az_xml_error_deep_copy(_:  &AzXmlError) -> AzXmlError;
        pub(crate) fn az_duplicated_namespace_error_delete(_:  &mut AzDuplicatedNamespaceError);
        pub(crate) fn az_duplicated_namespace_error_deep_copy(_:  &AzDuplicatedNamespaceError) -> AzDuplicatedNamespaceError;
        pub(crate) fn az_unknown_namespace_error_delete(_:  &mut AzUnknownNamespaceError);
        pub(crate) fn az_unknown_namespace_error_deep_copy(_:  &AzUnknownNamespaceError) -> AzUnknownNamespaceError;
        pub(crate) fn az_unexpected_close_tag_error_delete(_:  &mut AzUnexpectedCloseTagError);
        pub(crate) fn az_unexpected_close_tag_error_deep_copy(_:  &AzUnexpectedCloseTagError) -> AzUnexpectedCloseTagError;
        pub(crate) fn az_unknown_entity_reference_error_delete(_:  &mut AzUnknownEntityReferenceError);
        pub(crate) fn az_unknown_entity_reference_error_deep_copy(_:  &AzUnknownEntityReferenceError) -> AzUnknownEntityReferenceError;
        pub(crate) fn az_duplicated_attribute_error_delete(_:  &mut AzDuplicatedAttributeError);
        pub(crate) fn az_duplicated_attribute_error_deep_copy(_:  &AzDuplicatedAttributeError) -> AzDuplicatedAttributeError;
        pub(crate) fn az_xml_parse_error_delete(_:  &mut AzXmlParseError);
        pub(crate) fn az_xml_parse_error_deep_copy(_:  &AzXmlParseError) -> AzXmlParseError;
        pub(crate) fn az_xml_text_error_delete(_:  &mut AzXmlTextError);
        pub(crate) fn az_xml_text_error_deep_copy(_:  &AzXmlTextError) -> AzXmlTextError;
        pub(crate) fn az_xml_stream_error_delete(_:  &mut AzXmlStreamError);
        pub(crate) fn az_xml_stream_error_deep_copy(_:  &AzXmlStreamError) -> AzXmlStreamError;
        pub(crate) fn az_invalid_char_multiple_error_delete(_:  &mut AzInvalidCharMultipleError);
        pub(crate) fn az_invalid_char_multiple_error_deep_copy(_:  &AzInvalidCharMultipleError) -> AzInvalidCharMultipleError;
        pub(crate) fn az_invalid_string_error_delete(_:  &mut AzInvalidStringError);
        pub(crate) fn az_invalid_string_error_deep_copy(_:  &AzInvalidStringError) -> AzInvalidStringError;
        pub(crate) fn az_instant_ptr_now() -> AzInstantPtr;
        pub(crate) fn az_instant_ptr_delete(_:  &mut AzInstantPtr);
        pub(crate) fn az_duration_milliseconds(_:  usize) -> AzDuration;
        pub(crate) fn az_duration_seconds(_:  usize) -> AzDuration;
        pub(crate) fn az_app_config_default() -> AzAppConfig;
        pub(crate) fn az_app_config_delete(_:  &mut AzAppConfig);
        pub(crate) fn az_app_config_deep_copy(_:  &AzAppConfig) -> AzAppConfig;
        pub(crate) fn az_app_ptr_new(_:  AzRefAny, _:  AzAppConfig) -> AzAppPtr;
        pub(crate) fn az_app_ptr_add_window(_:  &mut AzAppPtr, _:  AzWindowCreateOptions);
        pub(crate) fn az_app_ptr_run(_:  AzAppPtr, _:  AzWindowCreateOptions);
        pub(crate) fn az_app_ptr_delete(_:  &mut AzAppPtr);
        pub(crate) fn az_hidpi_adjusted_bounds_get_logical_size(_:  &AzHidpiAdjustedBounds) -> AzLogicalSize;
        pub(crate) fn az_hidpi_adjusted_bounds_get_physical_size(_:  &AzHidpiAdjustedBounds) -> AzPhysicalSizeU32;
        pub(crate) fn az_hidpi_adjusted_bounds_get_hidpi_factor(_:  &AzHidpiAdjustedBounds) -> f32;
        pub(crate) fn az_focus_target_delete(_:  &mut AzFocusTarget);
        pub(crate) fn az_focus_target_deep_copy(_:  &AzFocusTarget) -> AzFocusTarget;
        pub(crate) fn az_focus_target_path_delete(_:  &mut AzFocusTargetPath);
        pub(crate) fn az_focus_target_path_deep_copy(_:  &AzFocusTargetPath) -> AzFocusTargetPath;
        pub(crate) fn az_callback_info_get_hit_node(_:  &AzCallbackInfo) -> AzDomNodeId;
        pub(crate) fn az_callback_info_get_cursor_relative_to_viewport(_:  &AzCallbackInfo) -> AzOptionLayoutPoint;
        pub(crate) fn az_callback_info_get_cursor_relative_to_node(_:  &AzCallbackInfo) -> AzOptionLayoutPoint;
        pub(crate) fn az_callback_info_get_parent(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn az_callback_info_get_previous_sibling(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn az_callback_info_get_next_sibling(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn az_callback_info_get_first_child(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn az_callback_info_get_last_child(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn az_callback_info_get_dataset(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionRefAny;
        pub(crate) fn az_callback_info_get_window_state(_:  &AzCallbackInfo) -> AzWindowState;
        pub(crate) fn az_callback_info_get_keyboard_state(_:  &AzCallbackInfo) -> AzKeyboardState;
        pub(crate) fn az_callback_info_get_mouse_state(_:  &AzCallbackInfo) -> AzMouseState;
        pub(crate) fn az_callback_info_get_current_window_handle(_:  &AzCallbackInfo) -> AzRawWindowHandle;
        pub(crate) fn az_callback_info_get_gl_context(_:  &AzCallbackInfo) -> AzGlContextPtr;
        pub(crate) fn az_callback_info_set_window_state(_:  &mut AzCallbackInfo, _:  AzWindowState);
        pub(crate) fn az_callback_info_set_focus(_:  &mut AzCallbackInfo, _:  AzFocusTarget);
        pub(crate) fn az_callback_info_set_css_property(_:  &mut AzCallbackInfo, _:  AzDomNodeId, _:  AzCssProperty);
        pub(crate) fn az_callback_info_stop_propagation(_:  &mut AzCallbackInfo);
        pub(crate) fn az_callback_info_create_window(_:  &mut AzCallbackInfo, _:  AzWindowCreateOptions);
        pub(crate) fn az_callback_info_start_thread(_:  &mut AzCallbackInfo, _:  AzThreadId, _:  AzRefAny, _:  AzRefAny, _:  AzThreadCallbackType);
        pub(crate) fn az_callback_info_start_timer(_:  &mut AzCallbackInfo, _:  AzTimerId, _:  AzTimer);
        pub(crate) fn az_callback_info_delete(_:  &mut AzCallbackInfo);
        pub(crate) fn az_i_frame_callback_info_get_bounds(_:  &AzIFrameCallbackInfo) -> AzHidpiAdjustedBounds;
        pub(crate) fn az_i_frame_callback_info_delete(_:  &mut AzIFrameCallbackInfo);
        pub(crate) fn az_i_frame_callback_return_delete(_:  &mut AzIFrameCallbackReturn);
        pub(crate) fn az_gl_callback_info_get_gl_context(_:  &AzGlCallbackInfo) -> AzGlContextPtr;
        pub(crate) fn az_gl_callback_info_delete(_:  &mut AzGlCallbackInfo);
        pub(crate) fn az_gl_callback_return_delete(_:  &mut AzGlCallbackReturn);
        pub(crate) fn az_timer_callback_info_delete(_:  &mut AzTimerCallbackInfo);
        pub(crate) fn az_timer_callback_return_delete(_:  &mut AzTimerCallbackReturn);
        pub(crate) fn az_timer_callback_return_deep_copy(_:  &AzTimerCallbackReturn) -> AzTimerCallbackReturn;
        pub(crate) fn az_write_back_callback_delete(_:  &mut AzWriteBackCallback);
        pub(crate) fn az_write_back_callback_deep_copy(_:  &AzWriteBackCallback) -> AzWriteBackCallback;
        pub(crate) fn az_atomic_ref_count_can_be_shared(_:  &AzAtomicRefCount) -> bool;
        pub(crate) fn az_atomic_ref_count_can_be_shared_mut(_:  &AzAtomicRefCount) -> bool;
        pub(crate) fn az_atomic_ref_count_increase_ref(_:  &AzAtomicRefCount);
        pub(crate) fn az_atomic_ref_count_decrease_ref(_:  &AzAtomicRefCount);
        pub(crate) fn az_atomic_ref_count_increase_refmut(_:  &AzAtomicRefCount);
        pub(crate) fn az_atomic_ref_count_decrease_refmut(_:  &AzAtomicRefCount);
        pub(crate) fn az_atomic_ref_count_delete(_:  &mut AzAtomicRefCount);
        pub(crate) fn az_atomic_ref_count_deep_copy(_:  &AzAtomicRefCount) -> AzAtomicRefCount;
        pub(crate) fn az_atomic_ref_count_fmt_debug(_:  &AzAtomicRefCount) -> AzString;
        pub(crate) fn az_ref_any_new_c(_:  *const c_void, _:  usize, _:  u64, _:  AzString, _:  AzRefAnyDestructorType) -> AzRefAny;
        pub(crate) fn az_ref_any_is_type(_:  &AzRefAny, _:  u64) -> bool;
        pub(crate) fn az_ref_any_get_type_name(_:  &AzRefAny) -> AzString;
        pub(crate) fn az_ref_any_can_be_shared(_:  &AzRefAny) -> bool;
        pub(crate) fn az_ref_any_can_be_shared_mut(_:  &AzRefAny) -> bool;
        pub(crate) fn az_ref_any_increase_ref(_:  &AzRefAny);
        pub(crate) fn az_ref_any_decrease_ref(_:  &AzRefAny);
        pub(crate) fn az_ref_any_increase_refmut(_:  &AzRefAny);
        pub(crate) fn az_ref_any_decrease_refmut(_:  &AzRefAny);
        pub(crate) fn az_ref_any_delete(_:  &mut AzRefAny);
        pub(crate) fn az_ref_any_deep_copy(_:  &AzRefAny) -> AzRefAny;
        pub(crate) fn az_layout_info_window_width_larger_than(_:  &mut AzLayoutInfo, _:  f32) -> bool;
        pub(crate) fn az_layout_info_window_width_smaller_than(_:  &mut AzLayoutInfo, _:  f32) -> bool;
        pub(crate) fn az_layout_info_window_height_larger_than(_:  &mut AzLayoutInfo, _:  f32) -> bool;
        pub(crate) fn az_layout_info_window_height_smaller_than(_:  &mut AzLayoutInfo, _:  f32) -> bool;
        pub(crate) fn az_layout_info_delete(_:  &mut AzLayoutInfo);
        pub(crate) fn az_css_rule_block_delete(_:  &mut AzCssRuleBlock);
        pub(crate) fn az_css_rule_block_deep_copy(_:  &AzCssRuleBlock) -> AzCssRuleBlock;
        pub(crate) fn az_css_declaration_delete(_:  &mut AzCssDeclaration);
        pub(crate) fn az_css_declaration_deep_copy(_:  &AzCssDeclaration) -> AzCssDeclaration;
        pub(crate) fn az_dynamic_css_property_delete(_:  &mut AzDynamicCssProperty);
        pub(crate) fn az_dynamic_css_property_deep_copy(_:  &AzDynamicCssProperty) -> AzDynamicCssProperty;
        pub(crate) fn az_css_path_delete(_:  &mut AzCssPath);
        pub(crate) fn az_css_path_deep_copy(_:  &AzCssPath) -> AzCssPath;
        pub(crate) fn az_css_path_selector_delete(_:  &mut AzCssPathSelector);
        pub(crate) fn az_css_path_selector_deep_copy(_:  &AzCssPathSelector) -> AzCssPathSelector;
        pub(crate) fn az_stylesheet_delete(_:  &mut AzStylesheet);
        pub(crate) fn az_stylesheet_deep_copy(_:  &AzStylesheet) -> AzStylesheet;
        pub(crate) fn az_css_empty() -> AzCss;
        pub(crate) fn az_css_from_string(_:  AzString) -> AzCss;
        pub(crate) fn az_css_delete(_:  &mut AzCss);
        pub(crate) fn az_css_deep_copy(_:  &AzCss) -> AzCss;
        pub(crate) fn az_color_u_from_str(_:  AzString) -> AzColorU;
        pub(crate) fn az_color_u_to_hash(_:  &AzColorU) -> AzString;
        pub(crate) fn az_linear_gradient_delete(_:  &mut AzLinearGradient);
        pub(crate) fn az_linear_gradient_deep_copy(_:  &AzLinearGradient) -> AzLinearGradient;
        pub(crate) fn az_radial_gradient_delete(_:  &mut AzRadialGradient);
        pub(crate) fn az_radial_gradient_deep_copy(_:  &AzRadialGradient) -> AzRadialGradient;
        pub(crate) fn az_conic_gradient_delete(_:  &mut AzConicGradient);
        pub(crate) fn az_conic_gradient_deep_copy(_:  &AzConicGradient) -> AzConicGradient;
        pub(crate) fn az_css_image_id_delete(_:  &mut AzCssImageId);
        pub(crate) fn az_css_image_id_deep_copy(_:  &AzCssImageId) -> AzCssImageId;
        pub(crate) fn az_style_background_content_delete(_:  &mut AzStyleBackgroundContent);
        pub(crate) fn az_style_background_content_deep_copy(_:  &AzStyleBackgroundContent) -> AzStyleBackgroundContent;
        pub(crate) fn az_style_font_family_delete(_:  &mut AzStyleFontFamily);
        pub(crate) fn az_style_font_family_deep_copy(_:  &AzStyleFontFamily) -> AzStyleFontFamily;
        pub(crate) fn az_scrollbar_style_value_delete(_:  &mut AzScrollbarStyleValue);
        pub(crate) fn az_scrollbar_style_value_deep_copy(_:  &AzScrollbarStyleValue) -> AzScrollbarStyleValue;
        pub(crate) fn az_style_background_content_vec_value_delete(_:  &mut AzStyleBackgroundContentVecValue);
        pub(crate) fn az_style_background_content_vec_value_deep_copy(_:  &AzStyleBackgroundContentVecValue) -> AzStyleBackgroundContentVecValue;
        pub(crate) fn az_style_font_family_value_delete(_:  &mut AzStyleFontFamilyValue);
        pub(crate) fn az_style_font_family_value_deep_copy(_:  &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue;
        pub(crate) fn az_style_transform_vec_value_delete(_:  &mut AzStyleTransformVecValue);
        pub(crate) fn az_style_transform_vec_value_deep_copy(_:  &AzStyleTransformVecValue) -> AzStyleTransformVecValue;
        pub(crate) fn az_css_property_delete(_:  &mut AzCssProperty);
        pub(crate) fn az_css_property_deep_copy(_:  &AzCssProperty) -> AzCssProperty;
        pub(crate) fn az_css_property_source_delete(_:  &mut AzCssPropertySource);
        pub(crate) fn az_css_property_source_deep_copy(_:  &AzCssPropertySource) -> AzCssPropertySource;
        pub(crate) fn az_styled_node_delete(_:  &mut AzStyledNode);
        pub(crate) fn az_styled_node_deep_copy(_:  &AzStyledNode) -> AzStyledNode;
        pub(crate) fn az_content_group_delete(_:  &mut AzContentGroup);
        pub(crate) fn az_content_group_deep_copy(_:  &AzContentGroup) -> AzContentGroup;
        pub(crate) fn az_styled_dom_new(_:  AzDom, _:  AzCss) -> AzStyledDom;
        pub(crate) fn az_styled_dom_append(_:  &mut AzStyledDom, _:  AzStyledDom);
        pub(crate) fn az_styled_dom_delete(_:  &mut AzStyledDom);
        pub(crate) fn az_id_or_class_delete(_:  &mut AzIdOrClass);
        pub(crate) fn az_id_or_class_deep_copy(_:  &AzIdOrClass) -> AzIdOrClass;
        pub(crate) fn az_node_data_inline_css_property_delete(_:  &mut AzNodeDataInlineCssProperty);
        pub(crate) fn az_node_data_inline_css_property_deep_copy(_:  &AzNodeDataInlineCssProperty) -> AzNodeDataInlineCssProperty;
        pub(crate) fn az_dom_new(_:  AzNodeType) -> AzDom;
        pub(crate) fn az_dom_div() -> AzDom;
        pub(crate) fn az_dom_body() -> AzDom;
        pub(crate) fn az_dom_label(_:  AzString) -> AzDom;
        pub(crate) fn az_dom_text(_:  AzTextId) -> AzDom;
        pub(crate) fn az_dom_image(_:  AzImageId) -> AzDom;
        pub(crate) fn az_dom_gl_texture(_:  AzRefAny, _:  AzGlCallbackType) -> AzDom;
        pub(crate) fn az_dom_iframe(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzDom;
        pub(crate) fn az_dom_add_id(_:  &mut AzDom, _:  AzString);
        pub(crate) fn az_dom_with_id(_:  AzDom, _:  AzString) -> AzDom;
        pub(crate) fn az_dom_add_class(_:  &mut AzDom, _:  AzString);
        pub(crate) fn az_dom_with_class(_:  AzDom, _:  AzString) -> AzDom;
        pub(crate) fn az_dom_add_callback(_:  &mut AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType);
        pub(crate) fn az_dom_with_callback(_:  AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzDom;
        pub(crate) fn az_dom_set_dataset(_:  &mut AzDom, _:  AzRefAny);
        pub(crate) fn az_dom_with_dataset(_:  AzDom, _:  AzRefAny) -> AzDom;
        pub(crate) fn az_dom_add_inline_css(_:  &mut AzDom, _:  AzCssProperty);
        pub(crate) fn az_dom_with_inline_css(_:  AzDom, _:  AzCssProperty) -> AzDom;
        pub(crate) fn az_dom_set_inline_css_props(_:  &mut AzDom, _:  AzNodeDataInlineCssPropertyVec);
        pub(crate) fn az_dom_with_inline_css_props(_:  AzDom, _:  AzNodeDataInlineCssPropertyVec) -> AzDom;
        pub(crate) fn az_dom_add_inline_hover_css(_:  &mut AzDom, _:  AzCssProperty);
        pub(crate) fn az_dom_with_inline_hover_css(_:  AzDom, _:  AzCssProperty) -> AzDom;
        pub(crate) fn az_dom_add_inline_active_css(_:  &mut AzDom, _:  AzCssProperty);
        pub(crate) fn az_dom_with_inline_active_css(_:  AzDom, _:  AzCssProperty) -> AzDom;
        pub(crate) fn az_dom_add_inline_focus_css(_:  &mut AzDom, _:  AzCssProperty);
        pub(crate) fn az_dom_with_inline_focus_css(_:  AzDom, _:  AzCssProperty) -> AzDom;
        pub(crate) fn az_dom_set_is_draggable(_:  &mut AzDom, _:  bool);
        pub(crate) fn az_dom_with_clip_mask(_:  AzDom, _:  AzOptionImageMask) -> AzDom;
        pub(crate) fn az_dom_set_clip_mask(_:  &mut AzDom, _:  AzOptionImageMask);
        pub(crate) fn az_dom_is_draggable(_:  AzDom, _:  bool) -> AzDom;
        pub(crate) fn az_dom_set_tab_index(_:  &mut AzDom, _:  AzOptionTabIndex);
        pub(crate) fn az_dom_with_tab_index(_:  AzDom, _:  AzOptionTabIndex) -> AzDom;
        pub(crate) fn az_dom_add_child(_:  &mut AzDom, _:  AzDom);
        pub(crate) fn az_dom_with_child(_:  AzDom, _:  AzDom) -> AzDom;
        pub(crate) fn az_dom_get_html_string(_:  &AzDom) -> AzString;
        pub(crate) fn az_dom_style(_:  AzDom, _:  AzCss) -> AzStyledDom;
        pub(crate) fn az_dom_delete(_:  &mut AzDom);
        pub(crate) fn az_dom_deep_copy(_:  &AzDom) -> AzDom;
        pub(crate) fn az_gl_texture_node_delete(_:  &mut AzGlTextureNode);
        pub(crate) fn az_gl_texture_node_deep_copy(_:  &AzGlTextureNode) -> AzGlTextureNode;
        pub(crate) fn az_i_frame_node_delete(_:  &mut AzIFrameNode);
        pub(crate) fn az_i_frame_node_deep_copy(_:  &AzIFrameNode) -> AzIFrameNode;
        pub(crate) fn az_callback_data_delete(_:  &mut AzCallbackData);
        pub(crate) fn az_callback_data_deep_copy(_:  &AzCallbackData) -> AzCallbackData;
        pub(crate) fn az_image_mask_delete(_:  &mut AzImageMask);
        pub(crate) fn az_image_mask_deep_copy(_:  &AzImageMask) -> AzImageMask;
        pub(crate) fn az_node_data_new(_:  AzNodeType) -> AzNodeData;
        pub(crate) fn az_node_data_div() -> AzNodeData;
        pub(crate) fn az_node_data_body() -> AzNodeData;
        pub(crate) fn az_node_data_label(_:  AzString) -> AzNodeData;
        pub(crate) fn az_node_data_text(_:  AzTextId) -> AzNodeData;
        pub(crate) fn az_node_data_image(_:  AzImageId) -> AzNodeData;
        pub(crate) fn az_node_data_gl_texture(_:  AzRefAny, _:  AzGlCallbackType) -> AzNodeData;
        pub(crate) fn az_node_data_iframe(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzNodeData;
        pub(crate) fn az_node_data_default() -> AzNodeData;
        pub(crate) fn az_node_data_add_id(_:  &mut AzNodeData, _:  AzString);
        pub(crate) fn az_node_data_with_id(_:  AzNodeData, _:  AzString) -> AzNodeData;
        pub(crate) fn az_node_data_add_class(_:  &mut AzNodeData, _:  AzString);
        pub(crate) fn az_node_data_with_class(_:  AzNodeData, _:  AzString) -> AzNodeData;
        pub(crate) fn az_node_data_set_ids_and_classes(_:  &mut AzNodeData, _:  AzIdOrClassVec);
        pub(crate) fn az_node_data_with_ids_and_classes(_:  AzNodeData, _:  AzIdOrClassVec) -> AzNodeData;
        pub(crate) fn az_node_data_add_dataset(_:  &mut AzNodeData, _:  AzRefAny);
        pub(crate) fn az_node_data_with_dataset(_:  AzNodeData, _:  AzRefAny) -> AzNodeData;
        pub(crate) fn az_node_data_add_callback(_:  &mut AzNodeData, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType);
        pub(crate) fn az_node_data_with_callback(_:  AzNodeData, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzNodeData;
        pub(crate) fn az_node_data_add_inline_css(_:  &mut AzNodeData, _:  AzCssProperty);
        pub(crate) fn az_node_data_with_inline_css(_:  AzNodeData, _:  AzCssProperty) -> AzNodeData;
        pub(crate) fn az_node_data_add_inline_hover_css(_:  &mut AzNodeData, _:  AzCssProperty);
        pub(crate) fn az_node_data_with_inline_hover_css(_:  AzNodeData, _:  AzCssProperty) -> AzNodeData;
        pub(crate) fn az_node_data_add_inline_active_css(_:  &mut AzNodeData, _:  AzCssProperty);
        pub(crate) fn az_node_data_with_inline_active_css(_:  AzNodeData, _:  AzCssProperty) -> AzNodeData;
        pub(crate) fn az_node_data_add_inline_focus_css(_:  &mut AzNodeData, _:  AzCssProperty);
        pub(crate) fn az_node_data_with_inline_focus_css(_:  AzNodeData, _:  AzCssProperty) -> AzNodeData;
        pub(crate) fn az_node_data_with_clip_mask(_:  AzNodeData, _:  AzOptionImageMask) -> AzNodeData;
        pub(crate) fn az_node_data_set_clip_mask(_:  &mut AzNodeData, _:  AzOptionImageMask);
        pub(crate) fn az_node_data_set_is_draggable(_:  &mut AzNodeData, _:  bool);
        pub(crate) fn az_node_data_is_draggable(_:  AzNodeData, _:  bool) -> AzNodeData;
        pub(crate) fn az_node_data_set_tab_index(_:  &mut AzNodeData, _:  AzOptionTabIndex);
        pub(crate) fn az_node_data_with_tab_index(_:  AzNodeData, _:  AzOptionTabIndex) -> AzNodeData;
        pub(crate) fn az_node_data_delete(_:  &mut AzNodeData);
        pub(crate) fn az_node_data_deep_copy(_:  &AzNodeData) -> AzNodeData;
        pub(crate) fn az_node_type_delete(_:  &mut AzNodeType);
        pub(crate) fn az_node_type_deep_copy(_:  &AzNodeType) -> AzNodeType;
        pub(crate) fn az_on_into_event_filter(_:  AzOn) -> AzEventFilter;
        pub(crate) fn az_vertex_attribute_delete(_:  &mut AzVertexAttribute);
        pub(crate) fn az_vertex_attribute_deep_copy(_:  &AzVertexAttribute) -> AzVertexAttribute;
        pub(crate) fn az_vertex_layout_delete(_:  &mut AzVertexLayout);
        pub(crate) fn az_vertex_layout_deep_copy(_:  &AzVertexLayout) -> AzVertexLayout;
        pub(crate) fn az_vertex_array_object_delete(_:  &mut AzVertexArrayObject);
        pub(crate) fn az_vertex_buffer_delete(_:  &mut AzVertexBuffer);
        pub(crate) fn az_debug_message_delete(_:  &mut AzDebugMessage);
        pub(crate) fn az_debug_message_deep_copy(_:  &AzDebugMessage) -> AzDebugMessage;
        pub(crate) fn az_u8_vec_ref_delete(_:  &mut AzU8VecRef);
        pub(crate) fn az_u8_vec_ref_mut_delete(_:  &mut AzU8VecRefMut);
        pub(crate) fn az_f32_vec_ref_delete(_:  &mut AzF32VecRef);
        pub(crate) fn az_i32_vec_ref_delete(_:  &mut AzI32VecRef);
        pub(crate) fn az_g_luint_vec_ref_delete(_:  &mut AzGLuintVecRef);
        pub(crate) fn az_g_lenum_vec_ref_delete(_:  &mut AzGLenumVecRef);
        pub(crate) fn az_g_lint_vec_ref_mut_delete(_:  &mut AzGLintVecRefMut);
        pub(crate) fn az_g_lint64_vec_ref_mut_delete(_:  &mut AzGLint64VecRefMut);
        pub(crate) fn az_g_lboolean_vec_ref_mut_delete(_:  &mut AzGLbooleanVecRefMut);
        pub(crate) fn az_g_lfloat_vec_ref_mut_delete(_:  &mut AzGLfloatVecRefMut);
        pub(crate) fn az_refstr_vec_ref_delete(_:  &mut AzRefstrVecRef);
        pub(crate) fn az_refstr_delete(_:  &mut AzRefstr);
        pub(crate) fn az_get_program_binary_return_delete(_:  &mut AzGetProgramBinaryReturn);
        pub(crate) fn az_get_program_binary_return_deep_copy(_:  &AzGetProgramBinaryReturn) -> AzGetProgramBinaryReturn;
        pub(crate) fn az_get_active_attrib_return_delete(_:  &mut AzGetActiveAttribReturn);
        pub(crate) fn az_get_active_attrib_return_deep_copy(_:  &AzGetActiveAttribReturn) -> AzGetActiveAttribReturn;
        pub(crate) fn az_g_lsync_ptr_delete(_:  &mut AzGLsyncPtr);
        pub(crate) fn az_get_active_uniform_return_delete(_:  &mut AzGetActiveUniformReturn);
        pub(crate) fn az_get_active_uniform_return_deep_copy(_:  &AzGetActiveUniformReturn) -> AzGetActiveUniformReturn;
        pub(crate) fn az_gl_context_ptr_get_type(_:  &AzGlContextPtr) -> AzGlType;
        pub(crate) fn az_gl_context_ptr_buffer_data_untyped(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  *const c_void, _:  u32);
        pub(crate) fn az_gl_context_ptr_buffer_sub_data_untyped(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  *const c_void);
        pub(crate) fn az_gl_context_ptr_map_buffer(_:  &AzGlContextPtr, _:  u32, _:  u32) -> *mut c_void;
        pub(crate) fn az_gl_context_ptr_map_buffer_range(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  u32) -> *mut c_void;
        pub(crate) fn az_gl_context_ptr_unmap_buffer(_:  &AzGlContextPtr, _:  u32) -> u8;
        pub(crate) fn az_gl_context_ptr_tex_buffer(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_shader_source(_:  &AzGlContextPtr, _:  u32, _:  AzStringVec);
        pub(crate) fn az_gl_context_ptr_read_buffer(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_read_pixels_into_buffer(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut);
        pub(crate) fn az_gl_context_ptr_read_pixels(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32) -> AzU8Vec;
        pub(crate) fn az_gl_context_ptr_read_pixels_into_pbo(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_sample_coverage(_:  &AzGlContextPtr, _:  f32, _:  bool);
        pub(crate) fn az_gl_context_ptr_polygon_offset(_:  &AzGlContextPtr, _:  f32, _:  f32);
        pub(crate) fn az_gl_context_ptr_pixel_store_i(_:  &AzGlContextPtr, _:  u32, _:  i32);
        pub(crate) fn az_gl_context_ptr_gen_buffers(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec;
        pub(crate) fn az_gl_context_ptr_gen_renderbuffers(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec;
        pub(crate) fn az_gl_context_ptr_gen_framebuffers(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec;
        pub(crate) fn az_gl_context_ptr_gen_textures(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec;
        pub(crate) fn az_gl_context_ptr_gen_vertex_arrays(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec;
        pub(crate) fn az_gl_context_ptr_gen_queries(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec;
        pub(crate) fn az_gl_context_ptr_begin_query(_:  &AzGlContextPtr, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_end_query(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_query_counter(_:  &AzGlContextPtr, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_get_query_object_iv(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32;
        pub(crate) fn az_gl_context_ptr_get_query_object_uiv(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u32;
        pub(crate) fn az_gl_context_ptr_get_query_object_i64v(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i64;
        pub(crate) fn az_gl_context_ptr_get_query_object_ui64v(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u64;
        pub(crate) fn az_gl_context_ptr_delete_queries(_:  &AzGlContextPtr, _:  AzGLuintVecRef);
        pub(crate) fn az_gl_context_ptr_delete_vertex_arrays(_:  &AzGlContextPtr, _:  AzGLuintVecRef);
        pub(crate) fn az_gl_context_ptr_delete_buffers(_:  &AzGlContextPtr, _:  AzGLuintVecRef);
        pub(crate) fn az_gl_context_ptr_delete_renderbuffers(_:  &AzGlContextPtr, _:  AzGLuintVecRef);
        pub(crate) fn az_gl_context_ptr_delete_framebuffers(_:  &AzGlContextPtr, _:  AzGLuintVecRef);
        pub(crate) fn az_gl_context_ptr_delete_textures(_:  &AzGlContextPtr, _:  AzGLuintVecRef);
        pub(crate) fn az_gl_context_ptr_framebuffer_renderbuffer(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_renderbuffer_storage(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_depth_func(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_active_texture(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_attach_shader(_:  &AzGlContextPtr, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_bind_attrib_location(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr);
        pub(crate) fn az_gl_context_ptr_get_uniform_iv(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLintVecRefMut);
        pub(crate) fn az_gl_context_ptr_get_uniform_fv(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLfloatVecRefMut);
        pub(crate) fn az_gl_context_ptr_get_uniform_block_index(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> u32;
        pub(crate) fn az_gl_context_ptr_get_uniform_indices(_:  &AzGlContextPtr, _:  u32, _:  AzRefstrVecRef) -> AzGLuintVec;
        pub(crate) fn az_gl_context_ptr_bind_buffer_base(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_bind_buffer_range(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  isize, _:  isize);
        pub(crate) fn az_gl_context_ptr_uniform_block_binding(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_bind_buffer(_:  &AzGlContextPtr, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_bind_vertex_array(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_bind_renderbuffer(_:  &AzGlContextPtr, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_bind_framebuffer(_:  &AzGlContextPtr, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_bind_texture(_:  &AzGlContextPtr, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_draw_buffers(_:  &AzGlContextPtr, _:  AzGLenumVecRef);
        pub(crate) fn az_gl_context_ptr_tex_image_2d(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef);
        pub(crate) fn az_gl_context_ptr_compressed_tex_image_2d(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  AzU8VecRef);
        pub(crate) fn az_gl_context_ptr_compressed_tex_sub_image_2d(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  AzU8VecRef);
        pub(crate) fn az_gl_context_ptr_tex_image_3d(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef);
        pub(crate) fn az_gl_context_ptr_copy_tex_image_2d(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_copy_tex_sub_image_2d(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_copy_tex_sub_image_3d(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_tex_sub_image_2d(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef);
        pub(crate) fn az_gl_context_ptr_tex_sub_image_2d_pbo(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize);
        pub(crate) fn az_gl_context_ptr_tex_sub_image_3d(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef);
        pub(crate) fn az_gl_context_ptr_tex_sub_image_3d_pbo(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize);
        pub(crate) fn az_gl_context_ptr_tex_storage_2d(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_tex_storage_3d(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_get_tex_image_into_buffer(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut);
        pub(crate) fn az_gl_context_ptr_copy_image_sub_data(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_invalidate_framebuffer(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef);
        pub(crate) fn az_gl_context_ptr_invalidate_sub_framebuffer(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_get_integer_v(_:  &AzGlContextPtr, _:  u32, _:  AzGLintVecRefMut);
        pub(crate) fn az_gl_context_ptr_get_integer_64v(_:  &AzGlContextPtr, _:  u32, _:  AzGLint64VecRefMut);
        pub(crate) fn az_gl_context_ptr_get_integer_iv(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut);
        pub(crate) fn az_gl_context_ptr_get_integer_64iv(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLint64VecRefMut);
        pub(crate) fn az_gl_context_ptr_get_boolean_v(_:  &AzGlContextPtr, _:  u32, _:  AzGLbooleanVecRefMut);
        pub(crate) fn az_gl_context_ptr_get_float_v(_:  &AzGlContextPtr, _:  u32, _:  AzGLfloatVecRefMut);
        pub(crate) fn az_gl_context_ptr_get_framebuffer_attachment_parameter_iv(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32;
        pub(crate) fn az_gl_context_ptr_get_renderbuffer_parameter_iv(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32;
        pub(crate) fn az_gl_context_ptr_get_tex_parameter_iv(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32;
        pub(crate) fn az_gl_context_ptr_get_tex_parameter_fv(_:  &AzGlContextPtr, _:  u32, _:  u32) -> f32;
        pub(crate) fn az_gl_context_ptr_tex_parameter_i(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32);
        pub(crate) fn az_gl_context_ptr_tex_parameter_f(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  f32);
        pub(crate) fn az_gl_context_ptr_framebuffer_texture_2d(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  i32);
        pub(crate) fn az_gl_context_ptr_framebuffer_texture_layer(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_blit_framebuffer(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_vertex_attrib_4f(_:  &AzGlContextPtr, _:  u32, _:  f32, _:  f32, _:  f32, _:  f32);
        pub(crate) fn az_gl_context_ptr_vertex_attrib_pointer_f32(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  bool, _:  i32, _:  u32);
        pub(crate) fn az_gl_context_ptr_vertex_attrib_pointer(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  bool, _:  i32, _:  u32);
        pub(crate) fn az_gl_context_ptr_vertex_attrib_i_pointer(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  u32);
        pub(crate) fn az_gl_context_ptr_vertex_attrib_divisor(_:  &AzGlContextPtr, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_viewport(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_scissor(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_line_width(_:  &AzGlContextPtr, _:  f32);
        pub(crate) fn az_gl_context_ptr_use_program(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_validate_program(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_draw_arrays(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_draw_arrays_instanced(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_draw_elements(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_draw_elements_instanced(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32);
        pub(crate) fn az_gl_context_ptr_blend_color(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32);
        pub(crate) fn az_gl_context_ptr_blend_func(_:  &AzGlContextPtr, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_blend_func_separate(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_blend_equation(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_blend_equation_separate(_:  &AzGlContextPtr, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_color_mask(_:  &AzGlContextPtr, _:  bool, _:  bool, _:  bool, _:  bool);
        pub(crate) fn az_gl_context_ptr_cull_face(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_front_face(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_enable(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_disable(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_hint(_:  &AzGlContextPtr, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_is_enabled(_:  &AzGlContextPtr, _:  u32) -> u8;
        pub(crate) fn az_gl_context_ptr_is_shader(_:  &AzGlContextPtr, _:  u32) -> u8;
        pub(crate) fn az_gl_context_ptr_is_texture(_:  &AzGlContextPtr, _:  u32) -> u8;
        pub(crate) fn az_gl_context_ptr_is_framebuffer(_:  &AzGlContextPtr, _:  u32) -> u8;
        pub(crate) fn az_gl_context_ptr_is_renderbuffer(_:  &AzGlContextPtr, _:  u32) -> u8;
        pub(crate) fn az_gl_context_ptr_check_frame_buffer_status(_:  &AzGlContextPtr, _:  u32) -> u32;
        pub(crate) fn az_gl_context_ptr_enable_vertex_attrib_array(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_disable_vertex_attrib_array(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_uniform_1f(_:  &AzGlContextPtr, _:  i32, _:  f32);
        pub(crate) fn az_gl_context_ptr_uniform_1fv(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef);
        pub(crate) fn az_gl_context_ptr_uniform_1i(_:  &AzGlContextPtr, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_uniform_1iv(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef);
        pub(crate) fn az_gl_context_ptr_uniform_1ui(_:  &AzGlContextPtr, _:  i32, _:  u32);
        pub(crate) fn az_gl_context_ptr_uniform_2f(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32);
        pub(crate) fn az_gl_context_ptr_uniform_2fv(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef);
        pub(crate) fn az_gl_context_ptr_uniform_2i(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_uniform_2iv(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef);
        pub(crate) fn az_gl_context_ptr_uniform_2ui(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_uniform_3f(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32);
        pub(crate) fn az_gl_context_ptr_uniform_3fv(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef);
        pub(crate) fn az_gl_context_ptr_uniform_3i(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_uniform_3iv(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef);
        pub(crate) fn az_gl_context_ptr_uniform_3ui(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_uniform_4f(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32, _:  f32);
        pub(crate) fn az_gl_context_ptr_uniform_4i(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn az_gl_context_ptr_uniform_4iv(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef);
        pub(crate) fn az_gl_context_ptr_uniform_4ui(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_uniform_4fv(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef);
        pub(crate) fn az_gl_context_ptr_uniform_matrix_2fv(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef);
        pub(crate) fn az_gl_context_ptr_uniform_matrix_3fv(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef);
        pub(crate) fn az_gl_context_ptr_uniform_matrix_4fv(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef);
        pub(crate) fn az_gl_context_ptr_depth_mask(_:  &AzGlContextPtr, _:  bool);
        pub(crate) fn az_gl_context_ptr_depth_range(_:  &AzGlContextPtr, _:  f64, _:  f64);
        pub(crate) fn az_gl_context_ptr_get_active_attrib(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveAttribReturn;
        pub(crate) fn az_gl_context_ptr_get_active_uniform(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveUniformReturn;
        pub(crate) fn az_gl_context_ptr_get_active_uniforms_iv(_:  &AzGlContextPtr, _:  u32, _:  AzGLuintVec, _:  u32) -> AzGLintVec;
        pub(crate) fn az_gl_context_ptr_get_active_uniform_block_i(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32;
        pub(crate) fn az_gl_context_ptr_get_active_uniform_block_iv(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> AzGLintVec;
        pub(crate) fn az_gl_context_ptr_get_active_uniform_block_name(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString;
        pub(crate) fn az_gl_context_ptr_get_attrib_location(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32;
        pub(crate) fn az_gl_context_ptr_get_frag_data_location(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32;
        pub(crate) fn az_gl_context_ptr_get_uniform_location(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32;
        pub(crate) fn az_gl_context_ptr_get_program_info_log(_:  &AzGlContextPtr, _:  u32) -> AzString;
        pub(crate) fn az_gl_context_ptr_get_program_iv(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut);
        pub(crate) fn az_gl_context_ptr_get_program_binary(_:  &AzGlContextPtr, _:  u32) -> AzGetProgramBinaryReturn;
        pub(crate) fn az_gl_context_ptr_program_binary(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzU8VecRef);
        pub(crate) fn az_gl_context_ptr_program_parameter_i(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32);
        pub(crate) fn az_gl_context_ptr_get_vertex_attrib_iv(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut);
        pub(crate) fn az_gl_context_ptr_get_vertex_attrib_fv(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLfloatVecRefMut);
        pub(crate) fn az_gl_context_ptr_get_vertex_attrib_pointer_v(_:  &AzGlContextPtr, _:  u32, _:  u32) -> isize;
        pub(crate) fn az_gl_context_ptr_get_buffer_parameter_iv(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32;
        pub(crate) fn az_gl_context_ptr_get_shader_info_log(_:  &AzGlContextPtr, _:  u32) -> AzString;
        pub(crate) fn az_gl_context_ptr_get_string(_:  &AzGlContextPtr, _:  u32) -> AzString;
        pub(crate) fn az_gl_context_ptr_get_string_i(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString;
        pub(crate) fn az_gl_context_ptr_get_shader_iv(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut);
        pub(crate) fn az_gl_context_ptr_get_shader_precision_format(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGlShaderPrecisionFormatReturn;
        pub(crate) fn az_gl_context_ptr_compile_shader(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_create_program(_:  &AzGlContextPtr) -> u32;
        pub(crate) fn az_gl_context_ptr_delete_program(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_create_shader(_:  &AzGlContextPtr, _:  u32) -> u32;
        pub(crate) fn az_gl_context_ptr_delete_shader(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_detach_shader(_:  &AzGlContextPtr, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_link_program(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_clear_color(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32);
        pub(crate) fn az_gl_context_ptr_clear(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_clear_depth(_:  &AzGlContextPtr, _:  f64);
        pub(crate) fn az_gl_context_ptr_clear_stencil(_:  &AzGlContextPtr, _:  i32);
        pub(crate) fn az_gl_context_ptr_flush(_:  &AzGlContextPtr);
        pub(crate) fn az_gl_context_ptr_finish(_:  &AzGlContextPtr);
        pub(crate) fn az_gl_context_ptr_get_error(_:  &AzGlContextPtr) -> u32;
        pub(crate) fn az_gl_context_ptr_stencil_mask(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_stencil_mask_separate(_:  &AzGlContextPtr, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_stencil_func(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32);
        pub(crate) fn az_gl_context_ptr_stencil_func_separate(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  u32);
        pub(crate) fn az_gl_context_ptr_stencil_op(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_stencil_op_separate(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_egl_image_target_texture2d_oes(_:  &AzGlContextPtr, _:  u32, _:  *const c_void);
        pub(crate) fn az_gl_context_ptr_generate_mipmap(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_insert_event_marker_ext(_:  &AzGlContextPtr, _:  AzRefstr);
        pub(crate) fn az_gl_context_ptr_push_group_marker_ext(_:  &AzGlContextPtr, _:  AzRefstr);
        pub(crate) fn az_gl_context_ptr_pop_group_marker_ext(_:  &AzGlContextPtr);
        pub(crate) fn az_gl_context_ptr_debug_message_insert_khr(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  AzRefstr);
        pub(crate) fn az_gl_context_ptr_push_debug_group_khr(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr);
        pub(crate) fn az_gl_context_ptr_pop_debug_group_khr(_:  &AzGlContextPtr);
        pub(crate) fn az_gl_context_ptr_fence_sync(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGLsyncPtr;
        pub(crate) fn az_gl_context_ptr_client_wait_sync(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64) -> u32;
        pub(crate) fn az_gl_context_ptr_wait_sync(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64);
        pub(crate) fn az_gl_context_ptr_delete_sync(_:  &AzGlContextPtr, _:  AzGLsyncPtr);
        pub(crate) fn az_gl_context_ptr_texture_range_apple(_:  &AzGlContextPtr, _:  u32, _:  AzU8VecRef);
        pub(crate) fn az_gl_context_ptr_gen_fences_apple(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec;
        pub(crate) fn az_gl_context_ptr_delete_fences_apple(_:  &AzGlContextPtr, _:  AzGLuintVecRef);
        pub(crate) fn az_gl_context_ptr_set_fence_apple(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_finish_fence_apple(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_test_fence_apple(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_test_object_apple(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u8;
        pub(crate) fn az_gl_context_ptr_finish_object_apple(_:  &AzGlContextPtr, _:  u32, _:  u32);
        pub(crate) fn az_gl_context_ptr_get_frag_data_index(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32;
        pub(crate) fn az_gl_context_ptr_blend_barrier_khr(_:  &AzGlContextPtr);
        pub(crate) fn az_gl_context_ptr_bind_frag_data_location_indexed(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  AzRefstr);
        pub(crate) fn az_gl_context_ptr_get_debug_messages(_:  &AzGlContextPtr) -> AzDebugMessageVec;
        pub(crate) fn az_gl_context_ptr_provoking_vertex_angle(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_gen_vertex_arrays_apple(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec;
        pub(crate) fn az_gl_context_ptr_bind_vertex_array_apple(_:  &AzGlContextPtr, _:  u32);
        pub(crate) fn az_gl_context_ptr_delete_vertex_arrays_apple(_:  &AzGlContextPtr, _:  AzGLuintVecRef);
        pub(crate) fn az_gl_context_ptr_copy_texture_chromium(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8);
        pub(crate) fn az_gl_context_ptr_copy_sub_texture_chromium(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8);
        pub(crate) fn az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes(_:  &AzGlContextPtr, _:  u32, _:  *const c_void);
        pub(crate) fn az_gl_context_ptr_copy_texture_3d_angle(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8);
        pub(crate) fn az_gl_context_ptr_copy_sub_texture_3d_angle(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8);
        pub(crate) fn az_gl_context_ptr_buffer_storage(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  *const c_void, _:  u32);
        pub(crate) fn az_gl_context_ptr_flush_mapped_buffer_range(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize);
        pub(crate) fn az_gl_context_ptr_delete(_:  &mut AzGlContextPtr);
        pub(crate) fn az_gl_context_ptr_deep_copy(_:  &AzGlContextPtr) -> AzGlContextPtr;
        pub(crate) fn az_texture_delete(_:  &mut AzTexture);
        pub(crate) fn az_texture_flags_default() -> AzTextureFlags;
        pub(crate) fn az_raw_image_format_delete(_:  &mut AzRawImageFormat);
        pub(crate) fn az_raw_image_format_deep_copy(_:  &AzRawImageFormat) -> AzRawImageFormat;
        pub(crate) fn az_text_id_new() -> AzTextId;
        pub(crate) fn az_image_id_new() -> AzImageId;
        pub(crate) fn az_font_id_new() -> AzFontId;
        pub(crate) fn az_image_source_delete(_:  &mut AzImageSource);
        pub(crate) fn az_image_source_deep_copy(_:  &AzImageSource) -> AzImageSource;
        pub(crate) fn az_font_source_delete(_:  &mut AzFontSource);
        pub(crate) fn az_font_source_deep_copy(_:  &AzFontSource) -> AzFontSource;
        pub(crate) fn az_raw_image_new(_:  AzU8Vec, _:  usize, _:  usize, _:  AzRawImageFormat) -> AzRawImage;
        pub(crate) fn az_raw_image_delete(_:  &mut AzRawImage);
        pub(crate) fn az_raw_image_deep_copy(_:  &AzRawImage) -> AzRawImage;
        pub(crate) fn az_svg_multi_polygon_delete(_:  &mut AzSvgMultiPolygon);
        pub(crate) fn az_svg_multi_polygon_deep_copy(_:  &AzSvgMultiPolygon) -> AzSvgMultiPolygon;
        pub(crate) fn az_svg_node_delete(_:  &mut AzSvgNode);
        pub(crate) fn az_svg_node_deep_copy(_:  &AzSvgNode) -> AzSvgNode;
        pub(crate) fn az_svg_styled_node_delete(_:  &mut AzSvgStyledNode);
        pub(crate) fn az_svg_styled_node_deep_copy(_:  &AzSvgStyledNode) -> AzSvgStyledNode;
        pub(crate) fn az_svg_path_delete(_:  &mut AzSvgPath);
        pub(crate) fn az_svg_path_deep_copy(_:  &AzSvgPath) -> AzSvgPath;
        pub(crate) fn az_tesselated_cpu_svg_node_delete(_:  &mut AzTesselatedCPUSvgNode);
        pub(crate) fn az_tesselated_cpu_svg_node_deep_copy(_:  &AzTesselatedCPUSvgNode) -> AzTesselatedCPUSvgNode;
        pub(crate) fn az_svg_parse_options_default() -> AzSvgParseOptions;
        pub(crate) fn az_svg_parse_options_delete(_:  &mut AzSvgParseOptions);
        pub(crate) fn az_svg_parse_options_deep_copy(_:  &AzSvgParseOptions) -> AzSvgParseOptions;
        pub(crate) fn az_svg_render_options_default() -> AzSvgRenderOptions;
        pub(crate) fn az_svg_render_options_delete(_:  &mut AzSvgRenderOptions);
        pub(crate) fn az_svg_render_options_deep_copy(_:  &AzSvgRenderOptions) -> AzSvgRenderOptions;
        pub(crate) fn az_svg_parse(_:  AzU8VecRef, _:  AzSvgParseOptions) -> AzResultSvgSvgParseError;
        pub(crate) fn az_svg_delete(_:  &mut AzSvg);
        pub(crate) fn az_svg_deep_copy(_:  &AzSvg) -> AzSvg;
        pub(crate) fn az_svg_xml_node_delete(_:  &mut AzSvgXmlNode);
        pub(crate) fn az_svg_xml_node_deep_copy(_:  &AzSvgXmlNode) -> AzSvgXmlNode;
        pub(crate) fn az_timer_id_unique() -> AzTimerId;
        pub(crate) fn az_timer_new(_:  AzRefAny, _:  AzTimerCallbackType) -> AzTimer;
        pub(crate) fn az_timer_with_delay(_:  AzTimer, _:  AzDuration) -> AzTimer;
        pub(crate) fn az_timer_with_interval(_:  AzTimer, _:  AzDuration) -> AzTimer;
        pub(crate) fn az_timer_with_timeout(_:  AzTimer, _:  AzDuration) -> AzTimer;
        pub(crate) fn az_timer_delete(_:  &mut AzTimer);
        pub(crate) fn az_timer_deep_copy(_:  &AzTimer) -> AzTimer;
        pub(crate) fn az_thread_sender_send(_:  &mut AzThreadSender, _:  AzThreadReceiveMsg) -> bool;
        pub(crate) fn az_thread_sender_delete(_:  &mut AzThreadSender);
        pub(crate) fn az_thread_receiver_receive(_:  &mut AzThreadReceiver) -> AzOptionThreadSendMsg;
        pub(crate) fn az_thread_receiver_delete(_:  &mut AzThreadReceiver);
        pub(crate) fn az_thread_receive_msg_delete(_:  &mut AzThreadReceiveMsg);
        pub(crate) fn az_thread_write_back_msg_delete(_:  &mut AzThreadWriteBackMsg);
        pub(crate) fn az_task_bar_icon_delete(_:  &mut AzTaskBarIcon);
        pub(crate) fn az_task_bar_icon_deep_copy(_:  &AzTaskBarIcon) -> AzTaskBarIcon;
        pub(crate) fn az_small_window_icon_bytes_delete(_:  &mut AzSmallWindowIconBytes);
        pub(crate) fn az_small_window_icon_bytes_deep_copy(_:  &AzSmallWindowIconBytes) -> AzSmallWindowIconBytes;
        pub(crate) fn az_large_window_icon_bytes_delete(_:  &mut AzLargeWindowIconBytes);
        pub(crate) fn az_large_window_icon_bytes_deep_copy(_:  &AzLargeWindowIconBytes) -> AzLargeWindowIconBytes;
        pub(crate) fn az_window_icon_delete(_:  &mut AzWindowIcon);
        pub(crate) fn az_window_icon_deep_copy(_:  &AzWindowIcon) -> AzWindowIcon;
        pub(crate) fn az_debug_state_delete(_:  &mut AzDebugState);
        pub(crate) fn az_debug_state_deep_copy(_:  &AzDebugState) -> AzDebugState;
        pub(crate) fn az_keyboard_state_delete(_:  &mut AzKeyboardState);
        pub(crate) fn az_keyboard_state_deep_copy(_:  &AzKeyboardState) -> AzKeyboardState;
        pub(crate) fn az_mouse_state_delete(_:  &mut AzMouseState);
        pub(crate) fn az_mouse_state_deep_copy(_:  &AzMouseState) -> AzMouseState;
        pub(crate) fn az_platform_specific_options_delete(_:  &mut AzPlatformSpecificOptions);
        pub(crate) fn az_platform_specific_options_deep_copy(_:  &AzPlatformSpecificOptions) -> AzPlatformSpecificOptions;
        pub(crate) fn az_windows_window_options_delete(_:  &mut AzWindowsWindowOptions);
        pub(crate) fn az_windows_window_options_deep_copy(_:  &AzWindowsWindowOptions) -> AzWindowsWindowOptions;
        pub(crate) fn az_wayland_theme_delete(_:  &mut AzWaylandTheme);
        pub(crate) fn az_wayland_theme_deep_copy(_:  &AzWaylandTheme) -> AzWaylandTheme;
        pub(crate) fn az_string_pair_delete(_:  &mut AzStringPair);
        pub(crate) fn az_string_pair_deep_copy(_:  &AzStringPair) -> AzStringPair;
        pub(crate) fn az_linux_window_options_delete(_:  &mut AzLinuxWindowOptions);
        pub(crate) fn az_linux_window_options_deep_copy(_:  &AzLinuxWindowOptions) -> AzLinuxWindowOptions;
        pub(crate) fn az_window_state_new(_:  AzLayoutCallbackType) -> AzWindowState;
        pub(crate) fn az_window_state_default() -> AzWindowState;
        pub(crate) fn az_window_state_delete(_:  &mut AzWindowState);
        pub(crate) fn az_window_state_deep_copy(_:  &AzWindowState) -> AzWindowState;
        pub(crate) fn az_window_create_options_new(_:  AzLayoutCallbackType) -> AzWindowCreateOptions;
        pub(crate) fn az_window_create_options_default() -> AzWindowCreateOptions;
        pub(crate) fn az_window_create_options_delete(_:  &mut AzWindowCreateOptions);
        pub(crate) fn az_window_create_options_deep_copy(_:  &AzWindowCreateOptions) -> AzWindowCreateOptions;
    }

