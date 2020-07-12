//! Provides datatypes used to describe an application's style using the Azul GUI framework.

#[macro_export]
macro_rules! impl_vec {($struct_type:ident, $struct_name:ident) => (

    #[repr(C)]
    pub struct $struct_name {
        ptr: *mut $struct_type,
        len: usize,
        cap: usize,
    }

    impl $struct_name {

        #[inline]
        pub fn new() -> Self {
            Vec::<$struct_type>::new().into()
        }

        #[inline]
        pub fn clear(&mut self) {
            *self = Self::new();
        }

        #[inline]
        pub fn sort_by<F: FnMut(&$struct_type, &$struct_type) -> std::cmp::Ordering>(&mut self, compare: F) {
            self.as_mut().sort_by(compare);
        }

        #[inline]
        pub fn with_capacity(cap: usize) -> Self {
            Vec::<$struct_type>::with_capacity(cap).into()
        }

        #[inline]
        pub fn push(&mut self, value: $struct_type) {
            // code is copied from the rust stdlib, since it's not possible to
            // create a temporary Vec here. Doing that would create two
            if self.len == self.capacity() {
                self.reserve(self.len, 1);
            }
            unsafe {
                let end = self.as_mut_ptr().add(self.len);
                std::ptr::write(end, value);
                self.len += 1;
            }
        }

        #[inline]
        pub fn iter(&self) -> std::slice::Iter<$struct_type> {
            self.as_ref().iter()
        }

        #[inline]
        pub fn iter_mut(&mut self) -> std::slice::IterMut<$struct_type> {
            self.as_mut().iter_mut()
        }

        #[inline]
        pub fn into_iter(self) -> std::vec::IntoIter<$struct_type> {
            let v1: Vec<$struct_type> = self.into();
            v1.into_iter()
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

        #[inline]
        fn amortized_new_size(&self, used_cap: usize, needed_extra_cap: usize) -> Result<usize, bool> {
            // Nothing we can really do about these checks :(
            let required_cap = used_cap.checked_add(needed_extra_cap).ok_or(true)?;
            // Cannot overflow, because `cap <= isize::MAX`, and type of `cap` is `usize`.
            let double_cap = self.cap * 2;
            // `double_cap` guarantees exponential growth.
            Ok(std::cmp::max(double_cap, required_cap))
        }

        #[inline]
        fn current_layout(&self) -> Option<std::alloc::Layout> {
            if self.cap == 0 {
                None
            } else {
                // We have an allocated chunk of memory, so we can bypass runtime
                // checks to get our current layout.
                unsafe {
                    let align = std::mem::align_of::<$struct_type>();
                    let size = std::mem::size_of::<$struct_type>() * self.cap;
                    Some(std::alloc::Layout::from_size_align_unchecked(size, align))
                }
            }
        }

        #[inline]
        fn alloc_guard(alloc_size: usize) -> Result<(), bool> {
            if std::mem::size_of::<usize>() < 8 && alloc_size > ::core::isize::MAX as usize {
                Err(true)
            } else {
                Ok(())
            }
        }

        #[inline]
        fn try_reserve(&mut self, used_cap: usize, needed_extra_cap: usize) -> Result<(), bool> {
            // NOTE: we don't early branch on ZSTs here because we want this
            // to actually catch "asking for more than usize::MAX" in that case.
            // If we make it past the first branch then we are guaranteed to
            // panic.

            // Don't actually need any more capacity.
            // Wrapping in case they give a bad `used_cap`
            if self.capacity().wrapping_sub(used_cap) >= needed_extra_cap {
               return Ok(());
            }

            let new_cap = self.amortized_new_size(used_cap, needed_extra_cap)?;
            let new_layout = std::alloc::Layout::array::<$struct_type>(new_cap).map_err(|_| true)?;

            // FIXME: may crash and burn on over-reserve
            $struct_name::alloc_guard(new_layout.size())?;

            let res = unsafe {
                match self.current_layout() {
                    Some(layout) => std::alloc::realloc(self.ptr as *mut u8, layout, new_layout.size()),
                    None => std::alloc::alloc(new_layout),
                }
            };

            if res == std::ptr::null_mut() {
                return Err(false);
            }

            self.ptr = res as *mut $struct_type;
            self.cap = new_cap;

            println!("allocating {} bytes for ptr {}", new_layout.size(), self.ptr as usize);

            Ok(())
        }

        fn reserve(&mut self, used_cap: usize, needed_extra_cap: usize) {
            match self.try_reserve(used_cap, needed_extra_cap) {
                Err(true /* Overflow */) => { std::process::exit(-1) },
                Err(false /* AllocError(_) */) => { std::process::exit(-2); },
                Ok(()) => { /* yay */ }
            }
        }

        fn truncate(&mut self, len: usize) {
            // This is safe because:
            //
            // * the slice passed to `drop_in_place` is valid; the `len > self.len`
            //   case avoids creating an invalid slice, and
            // * the `len` of the vector is shrunk before calling `drop_in_place`,
            //   such that no value will be dropped twice in case `drop_in_place`
            //   were to panic once (if it panics twice, the program aborts).
            unsafe {
                if len > self.len {
                    return;
                }
                let remaining_len = self.len - len;
                let s = std::ptr::slice_from_raw_parts_mut(self.as_mut_ptr().add(len), remaining_len);
                self.len = len;
                std::ptr::drop_in_place(s);
            }
        }

        pub fn retain<F>(&mut self, mut f: F) where F: FnMut(&$struct_type) -> bool {
            let len = self.len();
            let mut del = 0;

            {
                for i in 0..len {
                    if unsafe { !f(self.get_unchecked(i)) } {
                        del += 1;
                    } else if del > 0 {
                        self.as_mut().swap(i - del, i);
                    }
                }
            }

            if del > 0 {
                self.truncate(len - del);
            }
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

    impl AsMut<[$struct_type]> for $struct_name {
        fn as_mut(&mut self) -> &mut [$struct_type] {
            unsafe { std::slice::from_raw_parts_mut (self.ptr, self.len) }
        }
    }

    impl From<Vec<$struct_type>> for $struct_name {
        fn from(input: Vec<$struct_type>) -> $struct_name {
            use std::mem::ManuallyDrop;
            let mut me = ManuallyDrop::new(input);
            $struct_name { ptr: me.as_mut_ptr(), len: me.len(), cap: me.capacity() }
        }
    }

    impl From<$struct_name> for Vec<$struct_type> {
        fn from(input: $struct_name) -> Vec<$struct_type> {
            use std::mem::ManuallyDrop;
            let mut me = ManuallyDrop::new(input);
            unsafe { Vec::from_raw_parts(me.as_mut_ptr(), me.len(), me.capacity()) }
        }
    }

    impl Drop for $struct_name {
        fn drop(&mut self) {
            let _ = unsafe { Vec::from_raw_parts(self.ptr, self.len, self.cap) };
        }
    }
)}

#[macro_export]
macro_rules! impl_vec_as_hashmap {($struct_type:ident, $struct_name:ident) => (
    impl $struct_name {
        pub fn insert_hm_item(&mut self, item: $struct_type) {
            if !self.contains_hm_item(&item) {
                self.push(item);
            }
        }

        pub fn contains_hm_item(&self, searched: &$struct_type) -> bool {
            self.as_ref().iter().any(|i| i == searched)
        }

        pub fn remove_hm_item(&mut self, remove_key: &$struct_type) {
            self.retain(|v| v == remove_key);
        }
    }
)}

#[macro_export]
macro_rules! impl_vec_debug {($struct_type:ident, $struct_name:ident) => (
    impl std::fmt::Debug for $struct_name {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            self.as_ref().fmt(f)
        }
    }
)}

#[macro_export]
macro_rules! impl_vec_partialord {($struct_type:ident, $struct_name:ident) => (
    impl PartialOrd for $struct_name {
        fn partial_cmp(&self, rhs: &Self) -> Option<std::cmp::Ordering> {
            self.as_ref().partial_cmp(rhs.as_ref())
        }
    }
)}

#[macro_export]
macro_rules! impl_vec_ord {($struct_type:ident, $struct_name:ident) => (
    impl Ord for $struct_name {
        fn cmp(&self, rhs: &Self) -> std::cmp::Ordering {
            self.as_ref().cmp(rhs.as_ref())
        }
    }
)}

#[macro_export]
macro_rules! impl_vec_clone {($struct_type:ident, $struct_name:ident) => (
    impl Clone for $struct_name {
        fn clone(&self) -> Self {
            self.as_ref().to_vec().into()
        }
    }
)}

#[macro_export]
macro_rules! impl_vec_partialeq {($struct_type:ident, $struct_name:ident) => (
    impl PartialEq for $struct_name {
        fn eq(&self, rhs: &Self) -> bool {
            self.as_ref().eq(rhs.as_ref())
        }
    }
)}

#[macro_export]
macro_rules! impl_vec_eq {($struct_type:ident, $struct_name:ident) => (
    impl Eq for $struct_name { }
)}

#[macro_export]
macro_rules! impl_vec_hash {($struct_type:ident, $struct_name:ident) => (
    impl std::hash::Hash for $struct_name {
        fn hash<H>(&self, state: &mut H) where H: std::hash::Hasher {
            self.as_ref().hash(state);
        }
    }
)}

#[macro_export]
macro_rules! impl_option_inner {
    ($struct_type:ident, $struct_name:ident) => (

    impl From<$struct_name> for Option<$struct_type> {
        fn from(o: $struct_name) -> Option<$struct_type> {
            match o {
                $struct_name::None => None,
                $struct_name::Some(t) => Some(t),
            }
        }
    }

    impl From<Option<$struct_type>> for $struct_name {
        fn from(o: Option<$struct_type>) -> $struct_name {
            match o {
                None => $struct_name::None,
                Some(t) => $struct_name::Some(t),
            }
        }
    }

    impl Default for $struct_name {
        fn default() -> $struct_name { $struct_name::None }
    }

    impl $struct_name {
        pub fn as_option(&self) -> Option<&$struct_type> {
            match self {
                $struct_name::None => None,
                $struct_name::Some(t) => Some(t),
            }
        }
        pub fn is_some(&self) -> bool {
            match self {
                $struct_name::None => false,
                $struct_name::Some(_) => true,
            }
        }
        pub fn is_none(&self) -> bool {
            !self.is_some()
        }
    }
)}

#[macro_export]
macro_rules! impl_option {
    ($struct_type:ident, $struct_name:ident, copy = false, clone = false, [$($derive:meta),* ]) => (
        $(#[derive($derive)])*
        #[repr(C, u8)]
        pub enum $struct_name {
            None,
            Some($struct_type)
        }

        impl $struct_name {
            pub fn into_option(self) -> Option<$struct_type> {
                match self {
                    $struct_name::None => None,
                    $struct_name::Some(t) => Some(t),
                }
            }
        }

        impl_option_inner!($struct_type, $struct_name);
    );
    ($struct_type:ident, $struct_name:ident, copy = false, [$($derive:meta),* ]) => (
        $(#[derive($derive)])*
        #[repr(C, u8)]
        pub enum $struct_name {
            None,
            Some($struct_type)
        }

        impl $struct_name {
            pub fn into_option(&self) -> Option<$struct_type> {
                match self {
                    $struct_name::None => None,
                    $struct_name::Some(t) => Some(t.clone()),
                }
            }
        }

        impl_option_inner!($struct_type, $struct_name);
    );
    ($struct_type:ident, $struct_name:ident, [$($derive:meta),* ]) => (
        $(#[derive($derive)])*
        #[repr(C, u8)]
        pub enum $struct_name {
            None,
            Some($struct_type)
        }

        impl $struct_name {
            pub fn into_option(&self) -> Option<$struct_type> {
                match self {
                    $struct_name::None => None,
                    $struct_name::Some(t) => Some(*t),
                }
            }
        }

        impl_option_inner!($struct_type, $struct_name);
    );
}

#[macro_export]
macro_rules! impl_result_inner {
    ($ok_struct_type:ident, $err_struct_type:ident, $struct_name:ident) => (

    impl From<$struct_name> for Result<$ok_struct_type, $err_struct_type> {
        fn from(o: $struct_name) -> Result<$ok_struct_type, $err_struct_type> {
            match o {
                $struct_name::Ok(o) => Ok(o),
                $struct_name::Err(e) => Err(e),
            }
        }
    }

    impl From<Result<$ok_struct_type, $err_struct_type>> for $struct_name {
        fn from(o: Result<$ok_struct_type, $err_struct_type>) -> $struct_name {
            match o {
                Ok(o) => $struct_name::Ok(o),
                Err(e) => $struct_name::Err(e),
            }
        }
    }

    impl $struct_name {
        pub fn as_result(&self) -> Result<&$ok_struct_type, &$err_struct_type> {
            match self {
                $struct_name::Ok(o) => Ok(o),
                $struct_name::Err(e) => Err(e),
            }
        }
        pub fn is_ok(&self) -> bool {
            match self {
                $struct_name::Ok(_) => true,
                $struct_name::Err(_) => false,
            }
        }
        pub fn is_err(&self) -> bool {
            !self.is_ok()
        }
    }
)}

#[macro_export]
macro_rules! impl_result {
    ($ok_struct_type:ident, $err_struct_type:ident, $struct_name:ident, copy = false, clone = false, [$($derive:meta),* ]) => (
        $(#[derive($derive)])*
        #[repr(C, u8)]
        pub enum $struct_name {
            Ok($ok_struct_type),
            Err($err_struct_type)
        }

        impl $struct_name {
            pub fn into_result(self) -> Result<$ok_struct_type, $err_struct_type> {
                match self {
                    $struct_name::Ok(o) => Ok(o),
                    $struct_name::Err(e) => Err(e),
                }
            }
        }

        impl_result_inner!($ok_struct_type, $err_struct_type, $struct_name);
    );
    ($ok_struct_type:ident, $err_struct_type:ident, $struct_name:ident, copy = false, [$($derive:meta),* ]) => (
        $(#[derive($derive)])*
        #[repr(C, u8)]
        pub enum $struct_name {
            Ok($ok_struct_type),
            Err($err_struct_type)
        }
        impl $struct_name {
            pub fn into_result(&self) -> Result<$ok_struct_type, $err_struct_type> {
                match self {
                    $struct_name::Ok(o) => Ok(o.clone()),
                    $struct_name::Err(e) => Err(e.clone()),
                }
            }
        }

        impl_result_inner!($ok_struct_type, $err_struct_type, $struct_name);
    );
    ($ok_struct_type:ident, $err_struct_type:ident,  $struct_name:ident, [$($derive:meta),* ]) => (
        $(#[derive($derive)])*
        #[repr(C, u8)]
        pub enum $struct_name {
            Ok($ok_struct_type),
            Err($err_struct_type)
        }

        impl $struct_name {
            pub fn into_result(&self) -> Result<$ok_struct_type, $err_struct_type> {
                match self {
                    $struct_name::Ok(o) => Ok(*o),
                    $struct_name::Err(e) => Err(*e),
                }
            }
        }

        impl_result_inner!($ok_struct_type, $err_struct_type, $struct_name);
    );
}

#[derive(Debug)]
#[repr(C)]
pub struct AzString { pub vec: U8Vec }

impl AsRef<str> for AzString {
    fn as_ref<'a>(&'a self) -> &'a str {
        self.as_str()
    }
}

impl Default for AzString {
    fn default() -> Self {
        String::new().into()
    }
}

impl std::fmt::Display for AzString {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl AzString {

    #[inline]
    pub fn new(vec: U8Vec) -> Self {
        Self { vec }
    }

    #[inline]
    pub fn from_utf8_unchecked(ptr: *const u8, len: usize) -> Self {
        let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
        Self { vec: slice.to_vec().into() }
    }

    #[inline]
    pub fn from_utf8_lossy(ptr: *const u8, len: usize) -> Self {
        let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
        Self { vec: String::from_utf8_lossy(slice).into_owned().into_bytes().into() }
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(self.vec.as_ref()) }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.vec.as_ref()
    }

    #[inline]
    pub fn into_string(self) -> String {
        String::from(self)
    }

    #[inline]
    pub fn into_bytes(self) -> U8Vec {
        let mut m = std::mem::ManuallyDrop::new(self);
        U8Vec { ptr: m.vec.as_mut_ptr(), len: m.vec.len(), cap: m.vec.capacity() }
    }
}

impl From<AzString> for String {
    fn from(input: AzString) -> String {
        unsafe { String::from_utf8_unchecked(input.into_bytes().into()) }
    }
}

impl From<String> for AzString {
    fn from(input: String) -> AzString {
        AzString::new(input.into_bytes().into())
    }
}

impl PartialOrd for AzString {
    fn partial_cmp(&self, rhs: &Self) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(rhs.as_str())
    }
}

impl Ord for AzString {
    fn cmp(&self, rhs: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(rhs.as_str())
    }
}

impl Clone for AzString {
    fn clone(&self) -> Self {
        self.as_str().to_owned().into()
    }
}

impl PartialEq for AzString {
    fn eq(&self, rhs: &Self) -> bool {
        self.as_str().eq(rhs.as_str())
    }
}

impl Eq for AzString { }

impl std::hash::Hash for AzString {
    fn hash<H>(&self, state: &mut H) where H: std::hash::Hasher {
        self.as_str().hash(state)
    }
}

impl Drop for AzString {
    fn drop(&mut self) {
        // NOTE: dropping self.vec would lead to a double-free,
        // since U8Vec::drop() is automatically called here
    }
}

impl_option!(ColorU, OptionColorU, [Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash]);

impl_vec!(u8, U8Vec);
impl_vec_debug!(u8, U8Vec);
impl_vec_partialord!(u8, U8Vec);
impl_vec_ord!(u8, U8Vec);
impl_vec_clone!(u8, U8Vec);
impl_vec_partialeq!(u8, U8Vec);
impl_vec_eq!(u8, U8Vec);
impl_vec_hash!(u8, U8Vec);

impl_vec!(AzString, StringVec);
impl_vec_debug!(AzString, StringVec);
impl_vec_partialord!(AzString, StringVec);
impl_vec_ord!(AzString, StringVec);
impl_vec_clone!(AzString, StringVec);
impl_vec_partialeq!(AzString, StringVec);
impl_vec_eq!(AzString, StringVec);
impl_vec_hash!(AzString, StringVec);

impl_vec!(GradientStopPre, GradientStopPreVec);
impl_vec_debug!(GradientStopPre, GradientStopPreVec);
impl_vec_partialord!(GradientStopPre, GradientStopPreVec);
impl_vec_ord!(GradientStopPre, GradientStopPreVec);
impl_vec_clone!(GradientStopPre, GradientStopPreVec);
impl_vec_partialeq!(GradientStopPre, GradientStopPreVec);
impl_vec_eq!(GradientStopPre, GradientStopPreVec);
impl_vec_hash!(GradientStopPre, GradientStopPreVec);

impl From<Vec<String>> for StringVec {
    fn from(v: Vec<String>) -> StringVec {
        let new_v: Vec<AzString> = v.into_iter().map(|s| s.into()).collect();
        new_v.into()
    }
}

impl From<StringVec> for Vec<String> {
    fn from(v: StringVec) -> Vec<String> {
        let v: Vec<AzString> = v.into();
        v.into_iter().map(|s| s.into()).collect()
    }
}

mod css;
mod css_properties;

pub use crate::css::*;
pub use crate::css_properties::*;
