//! Provides datatypes used to describe an application's style using the Azul GUI framework.

use std::fmt;

#[macro_export]
macro_rules! impl_vec {($struct_type:ident, $struct_name:ident) => (

    #[repr(C)]
    pub struct $struct_name {
        ptr: *mut $struct_type,
        len: usize,
        cap: usize,
    }

    impl $struct_name {

        pub fn new() -> Self {
            Vec::<$struct_type>::new().into()
        }

        pub fn with_capacity(cap: usize) -> Self {
            Vec::<$struct_type>::with_capacity(cap).into()
        }

        pub fn push(&mut self, val: $struct_type) {
            let mut v: Vec<$struct_type> = unsafe { Vec::from_raw_parts(self.ptr, self.len, self.cap) };
            v.push(val);
            let (ptr, len, cap) = Self::into_raw_parts(v);
            self.ptr = ptr;
            self.len = len;
            self.cap = cap;
        }

        pub fn iter(&self) -> std::slice::Iter<$struct_type> {
            let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
            v1.iter()
        }

        pub fn iter_mut(&mut self) -> std::slice::IterMut<$struct_type> {
            let v1: &mut [$struct_type] = unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) };
            v1.iter_mut()
        }

        pub fn into_iter(self) -> std::vec::IntoIter<$struct_type> {
            let v1: Vec<$struct_type> = unsafe { std::vec::Vec::from_raw_parts(self.ptr, self.len, self.cap) };
            std::mem::forget(self); // do not run destructor of self
            v1.into_iter()
        }

        pub fn as_ptr(&self) -> *const $struct_type {
            self.ptr as *const $struct_type
        }

        pub fn len(&self) -> usize {
            self.len
        }

        pub fn is_empty(&self) -> bool {
            self.len == 0
        }

        pub fn cap(&self) -> usize {
            self.cap
        }

        pub fn get(&self, index: usize) -> Option<&$struct_type> {
            let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
            let res = v1.get(index);
            std::mem::forget(v1);
            res
        }

        pub fn foreach<U, F: FnMut(&$struct_type) -> Result<(), U>>(&self, mut closure: F) -> Result<(), U> {
            let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
            for i in v1.iter() { closure(i)?; }
            std::mem::forget(v1);
            Ok(())
        }

        /// Same as Vec::into_raw_parts(self), prevents destructor from running
        fn into_raw_parts(mut v: Vec<$struct_type>) -> (*mut $struct_type, usize, usize) {
            let ptr = v.as_mut_ptr();
            let len = v.len();
            let cap = v.capacity();
            std::mem::forget(v);
            (ptr, len, cap)
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

    impl fmt::Debug for $struct_name {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
            let res = v1.fmt(f);
            std::mem::forget(v1);
            res
        }
    }

    impl From<Vec<$struct_type>> for $struct_name {
        fn from(input: Vec<$struct_type>) -> $struct_name {
            let (ptr, len, cap) = $struct_name::into_raw_parts(input);
            $struct_name { ptr, len, cap }
        }
    }

    impl From<$struct_name> for Vec<$struct_type> {
        fn from(input: $struct_name) -> Vec<$struct_type> {
            let v = unsafe { Vec::from_raw_parts(input.ptr, input.len, input.cap) };
            std::mem::forget(input); // don't run the destructor of "input"
            v
        }
    }

    impl PartialOrd for $struct_name {
        fn partial_cmp(&self, rhs: &Self) -> Option<std::cmp::Ordering> {
            let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
            let v2: &[$struct_type] = unsafe { std::slice::from_raw_parts(rhs.ptr, rhs.len) };
            let result = v1.partial_cmp(&v2);
            std::mem::forget(v1);
            std::mem::forget(v2);
            result
        }
    }

    impl Ord for $struct_name {
        fn cmp(&self, rhs: &Self) -> std::cmp::Ordering {
            let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
            let v2: &[$struct_type] = unsafe { std::slice::from_raw_parts(rhs.ptr, rhs.len) };
            let result = v1.cmp(&v2);
            std::mem::forget(v1);
            std::mem::forget(v2);
            result
        }
    }

    impl Clone for $struct_name {
        fn clone(&self) -> Self {
            let v: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
            let v2 = v.to_vec();
            std::mem::forget(v);
            let (ptr, len, cap) = $struct_name::into_raw_parts(v2);
            $struct_name { ptr, len, cap }
        }
    }

    impl PartialEq for $struct_name {
        fn eq(&self, other: &Self) -> bool {
            let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
            let v2: &[$struct_type] = unsafe { std::slice::from_raw_parts(other.ptr, other.len) };
            let is_eq = v1.eq(v2);
            std::mem::forget(v1);
            std::mem::forget(v2);
            is_eq
        }
    }

    impl Eq for $struct_name { }

    impl std::hash::Hash for $struct_name {
        fn hash<H>(&self, state: &mut H) where H: std::hash::Hasher {
            let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
            v1.hash(state);
            std::mem::forget(v1);
        }
    }

    impl Drop for $struct_name {
        fn drop(&mut self) {
            let _v: Vec<$struct_type> = unsafe { Vec::from_raw_parts(self.ptr, self.len, self.cap) };
            // let v drop here
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

    impl $struct_name {
        pub fn as_option(&self) -> Option<&$struct_type> {
            match self {
                $struct_name::None => None,
                $struct_name::Some(t) => Some(t),
            }
        }
    }
)}

#[macro_export]
macro_rules! impl_option {
    ($struct_type:ident, $struct_name:ident, copy = false, clone = false) => (
        #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(C, u8)]
        pub enum $struct_name {
            None,
            Some($struct_type)
        }

        impl_option_inner!($struct_type, $struct_name);
    );
    ($struct_type:ident, $struct_name:ident, copy = false) => (
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(C, u8)]
        pub enum $struct_name {
            None,
            Some($struct_type)
        }

        impl_option_inner!($struct_type, $struct_name);
    );
    ($struct_type:ident, $struct_name:ident) => (
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(C, u8)]
        pub enum $struct_name {
            None,
            Some($struct_type)
        }

        impl_option_inner!($struct_type, $struct_name);
    );
}

#[repr(C)]
pub struct AzString { vec: U8Vec }

impl AsRef<str> for AzString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for AzString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.vec.ptr, self.vec.len)) };
        let res = s.fmt(f);
        std::mem::forget(s);
        res
    }
}

impl AzString {
    #[inline]
    pub fn as_str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.vec.ptr, self.vec.len)) }
    }
    #[inline]
    pub fn into_string(self) -> String {
        String::from(self)
    }
    #[inline]
    pub fn into_bytes(self) -> U8Vec {
        let self_vec = U8Vec { ptr: self.vec.ptr, len: self.vec.len, cap: self.vec.cap };
        std::mem::forget(self); // don't run destructor
        self_vec
    }
}

impl fmt::Debug for AzString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.vec.ptr, self.vec.len)) };
        let res = s.fmt(f);
        std::mem::forget(s);
        res
    }
}

impl From<AzString> for String {
    fn from(input: AzString) -> String {
        let s = unsafe { String::from_raw_parts(input.vec.ptr, input.vec.len, input.vec.cap) };
        std::mem::forget(input);
        s
    }
}

impl From<String> for AzString {
    fn from(mut input: String) -> AzString {
        let ptr = input.as_mut_ptr();
        let len = input.len();
        let cap = input.capacity();
        std::mem::forget(input);
        AzString { vec: U8Vec { ptr, len, cap } }
    }
}

impl PartialOrd for AzString {
    fn partial_cmp(&self, rhs: &Self) -> Option<std::cmp::Ordering> {
        let v1: &str = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.vec.ptr, self.vec.len)) };
        let v2: &str = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(rhs.vec.ptr, rhs.vec.len)) };
        let result = v1.partial_cmp(&v2);
        std::mem::forget(v1);
        std::mem::forget(v2);
        result
    }
}

impl Ord for AzString {
    fn cmp(&self, rhs: &Self) -> std::cmp::Ordering {
        let v1: &str = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.vec.ptr, self.vec.len)) };
        let v2: &str = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(rhs.vec.ptr, rhs.vec.len)) };
        let result = v1.cmp(&v2);
        std::mem::forget(v1);
        std::mem::forget(v2);
        result
    }
}

impl Clone for AzString {
    fn clone(&self) -> Self {
        let v: &str = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.vec.ptr, self.vec.len)) };
        let mut v2 = v.to_owned();
        std::mem::forget(v);
        let ptr = v2.as_mut_ptr();
        let len = v2.len();
        let cap = v2.capacity();
        std::mem::forget(v2);
        AzString { vec: U8Vec { ptr, len, cap } }
    }
}

impl PartialEq for AzString {
    fn eq(&self, other: &Self) -> bool {
        let v1: &str = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.vec.ptr, self.vec.len)) };
        let v2: &str = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(other.vec.ptr, other.vec.len)) };
        let is_eq = v1.eq(v2);
        std::mem::forget(v1);
        std::mem::forget(v2);
        is_eq
    }
}

impl Eq for AzString { }

impl std::hash::Hash for AzString {
    fn hash<H>(&self, state: &mut H) where H: std::hash::Hasher {
        let v1: &str = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.vec.ptr, self.vec.len)) };
        v1.hash(state);
        std::mem::forget(v1);
    }
}

impl Drop for AzString {
    fn drop(&mut self) {
        // NOTE: dropping self.vec would lead to a double-free,
        // since U8Vec::drop() is automatically called here
    }
}

impl_vec!(u8, U8Vec);
impl_vec!(AzString, StringVec);
impl_vec!(GradientStopPre, GradientStopPreVec);

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
mod hot_reload;

pub use crate::css::*;
pub use crate::css_properties::*;
pub use crate::hot_reload::*;
