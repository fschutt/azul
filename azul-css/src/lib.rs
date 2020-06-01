//! Provides datatypes used to describe an application's style using the Azul GUI framework.

use std::fmt;

mod css;
mod css_properties;
mod hot_reload;

pub use crate::css::*;
pub use crate::css_properties::*;
pub use crate::hot_reload::*;

macro_rules! impl_vec {($struct_type:ident, $struct_name:ident) => (

    #[repr(C)]
    pub struct $struct_name {
        ptr: *mut $struct_type,
        len: usize,
        cap: usize,
    }

    impl $struct_name {

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

#[repr(C)]
pub struct AzString { vec: U8Vec }

impl fmt::Display for AzString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.vec.ptr, self.vec.len)) };
        let res = s.fmt(f);
        std::mem::forget(s);
        res
    }
}

impl AzString {
    pub fn as_str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.vec.ptr, self.vec.len)) }
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
        let _v1: String = unsafe { String::from_raw_parts(self.vec.ptr, self.vec.len, self.vec.cap) };
        std::mem::forget(self); // don't let the destructor of self.vec run
        // v1 drops here
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