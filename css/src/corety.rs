use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use crate::props::basic::ColorU;

// Debug message severity/category
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutDebugMessageType {
    Info,
    Warning,
    Error,
    // Layout-specific categories for filtering
    BoxProps,
    CssGetter,
    BfcLayout,
    IfcLayout,
    TableLayout,
    DisplayType,
    PositionCalculation,
}

impl Default for LayoutDebugMessageType {
    fn default() -> Self {
        Self::Info
    }
}

// Define a struct for debug messages
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutDebugMessage {
    pub message_type: LayoutDebugMessageType,
    pub message: AzString,
    pub location: AzString,
}

impl LayoutDebugMessage {
    /// Create a new debug message with automatic caller location tracking
    #[track_caller]
    pub fn new(message_type: LayoutDebugMessageType, message: impl Into<String>) -> Self {
        let location = core::panic::Location::caller();
        Self {
            message_type,
            message: AzString::from_string(message.into()),
            location: AzString::from_string(format!("{}:{}:{}", 
                location.file(), 
                location.line(), 
                location.column()
            )),
        }
    }

    /// Helper for Info messages
    #[track_caller]
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(LayoutDebugMessageType::Info, message)
    }

    /// Helper for Warning messages
    #[track_caller]
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(LayoutDebugMessageType::Warning, message)
    }

    /// Helper for Error messages
    #[track_caller]
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(LayoutDebugMessageType::Error, message)
    }

    /// Helper for BoxProps debug messages
    #[track_caller]
    pub fn box_props(message: impl Into<String>) -> Self {
        Self::new(LayoutDebugMessageType::BoxProps, message)
    }

    /// Helper for CSS Getter debug messages
    #[track_caller]
    pub fn css_getter(message: impl Into<String>) -> Self {
        Self::new(LayoutDebugMessageType::CssGetter, message)
    }

    /// Helper for BFC Layout debug messages
    #[track_caller]
    pub fn bfc_layout(message: impl Into<String>) -> Self {
        Self::new(LayoutDebugMessageType::BfcLayout, message)
    }

    /// Helper for IFC Layout debug messages
    #[track_caller]
    pub fn ifc_layout(message: impl Into<String>) -> Self {
        Self::new(LayoutDebugMessageType::IfcLayout, message)
    }

    /// Helper for Table Layout debug messages
    #[track_caller]
    pub fn table_layout(message: impl Into<String>) -> Self {
        Self::new(LayoutDebugMessageType::TableLayout, message)
    }

    /// Helper for Display Type debug messages
    #[track_caller]
    pub fn display_type(message: impl Into<String>) -> Self {
        Self::new(LayoutDebugMessageType::DisplayType, message)
    }
}

#[repr(C)]
pub struct AzString {
    pub vec: U8Vec,
}

impl_option!(
    AzString,
    OptionAzString,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

static DEFAULT_STR: &str = "";

impl Default for AzString {
    fn default() -> Self {
        DEFAULT_STR.into()
    }
}

impl<'a> From<&'a str> for AzString {
    fn from(s: &'a str) -> Self {
        s.to_string().into()
    }
}

impl AsRef<str> for AzString {
    fn as_ref<'a>(&'a self) -> &'a str {
        self.as_str()
    }
}

impl core::fmt::Debug for AzString {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl core::fmt::Display for AzString {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl AzString {
    #[inline]
    pub const fn from_const_str(s: &'static str) -> Self {
        Self {
            vec: U8Vec::from_const_slice(s.as_bytes()),
        }
    }

    #[inline]
    pub fn from_string(s: String) -> Self {
        Self {
            vec: U8Vec::from_vec(s.into_bytes()),
        }
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(self.vec.as_ref()) }
    }

    /// NOTE: CLONES the memory if the memory is external or &'static
    /// Moves the memory out if the memory is library-allocated
    #[inline]
    pub fn clone_self(&self) -> Self {
        Self {
            vec: self.vec.clone_self(),
        }
    }

    #[inline]
    pub fn into_library_owned_string(self) -> String {
        match self.vec.destructor {
            U8VecDestructor::NoDestructor | U8VecDestructor::External(_) => {
                self.as_str().to_string()
            }
            U8VecDestructor::DefaultRust => {
                let m = core::mem::ManuallyDrop::new(self);
                unsafe { String::from_raw_parts(m.vec.ptr as *mut u8, m.vec.len, m.vec.cap) }
            }
        }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.vec.as_ref()
    }

    #[inline]
    pub fn into_bytes(self) -> U8Vec {
        let m = core::mem::ManuallyDrop::new(self);
        U8Vec {
            ptr: m.vec.ptr,
            len: m.vec.len,
            cap: m.vec.cap,
            destructor: m.vec.destructor,
        }
    }
}

impl From<String> for AzString {
    fn from(input: String) -> AzString {
        AzString::from_string(input)
    }
}

impl PartialOrd for AzString {
    fn partial_cmp(&self, rhs: &Self) -> Option<core::cmp::Ordering> {
        self.as_str().partial_cmp(rhs.as_str())
    }
}

impl Ord for AzString {
    fn cmp(&self, rhs: &Self) -> core::cmp::Ordering {
        self.as_str().cmp(rhs.as_str())
    }
}

impl Clone for AzString {
    fn clone(&self) -> Self {
        self.clone_self()
    }
}

impl PartialEq for AzString {
    fn eq(&self, rhs: &Self) -> bool {
        self.as_str().eq(rhs.as_str())
    }
}

impl Eq for AzString {}

impl core::hash::Hash for AzString {
    fn hash<H>(&self, state: &mut H)
    where
        H: core::hash::Hasher,
    {
        self.as_str().hash(state)
    }
}

impl core::ops::Deref for AzString {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl_vec!(u8, U8Vec, U8VecDestructor);
impl_vec_debug!(u8, U8Vec);
impl_vec_partialord!(u8, U8Vec);
impl_vec_ord!(u8, U8Vec);
impl_vec_clone!(u8, U8Vec, U8VecDestructor);
impl_vec_partialeq!(u8, U8Vec);
impl_vec_eq!(u8, U8Vec);
impl_vec_hash!(u8, U8Vec);

impl_option!(
    U8Vec,
    OptionU8Vec,
    copy = false,
    [Debug, Clone, PartialEq, Ord, PartialOrd, Eq, Hash]
);

impl_vec!(u16, U16Vec, U16VecDestructor);
impl_vec_debug!(u16, U16Vec);
impl_vec_partialord!(u16, U16Vec);
impl_vec_ord!(u16, U16Vec);
impl_vec_clone!(u16, U16Vec, U16VecDestructor);
impl_vec_partialeq!(u16, U16Vec);
impl_vec_eq!(u16, U16Vec);
impl_vec_hash!(u16, U16Vec);

impl_vec!(f32, F32Vec, F32VecDestructor);
impl_vec_debug!(f32, F32Vec);
impl_vec_partialord!(f32, F32Vec);
impl_vec_clone!(f32, F32Vec, F32VecDestructor);
impl_vec_partialeq!(f32, F32Vec);

// Vec<char>
impl_vec!(u32, U32Vec, U32VecDestructor);
impl_vec_mut!(u32, U32Vec);
impl_vec_debug!(u32, U32Vec);
impl_vec_partialord!(u32, U32Vec);
impl_vec_ord!(u32, U32Vec);
impl_vec_clone!(u32, U32Vec, U32VecDestructor);
impl_vec_partialeq!(u32, U32Vec);
impl_vec_eq!(u32, U32Vec);
impl_vec_hash!(u32, U32Vec);

impl_vec!(AzString, StringVec, StringVecDestructor);
impl_vec_debug!(AzString, StringVec);
impl_vec_partialord!(AzString, StringVec);
impl_vec_ord!(AzString, StringVec);
impl_vec_clone!(AzString, StringVec, StringVecDestructor);
impl_vec_partialeq!(AzString, StringVec);
impl_vec_eq!(AzString, StringVec);
impl_vec_hash!(AzString, StringVec);

impl From<Vec<String>> for StringVec {
    fn from(v: Vec<String>) -> StringVec {
        let new_v: Vec<AzString> = v.into_iter().map(|s| s.into()).collect();
        new_v.into()
    }
}

impl_option!(
    StringVec,
    OptionStringVec,
    copy = false,
    [Debug, Clone, PartialOrd, PartialEq, Ord, Eq, Hash]
);

impl_option!(
    u16,
    OptionU16,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_option!(
    u32,
    OptionU32,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_option!(
    i16,
    OptionI16,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_option!(
    i32,
    OptionI32,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_option!(f32, OptionF32, [Debug, Copy, Clone, PartialEq, PartialOrd]);
impl_option!(f64, OptionF64, [Debug, Copy, Clone, PartialEq, PartialOrd]);

// Manual implementations for Hash and Ord on OptionF32 (since f32 doesn't implement these traits)
impl core::hash::Hash for OptionF32 {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        match self {
            OptionF32::None => 0u8.hash(state),
            OptionF32::Some(v) => {
                1u8.hash(state);
                v.to_bits().hash(state);
            }
        }
    }
}

impl Eq for OptionF32 {}

impl Ord for OptionF32 {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (self, other) {
            (OptionF32::None, OptionF32::None) => core::cmp::Ordering::Equal,
            (OptionF32::None, OptionF32::Some(_)) => core::cmp::Ordering::Less,
            (OptionF32::Some(_), OptionF32::None) => core::cmp::Ordering::Greater,
            (OptionF32::Some(a), OptionF32::Some(b)) => {
                a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal)
            }
        }
    }
}
