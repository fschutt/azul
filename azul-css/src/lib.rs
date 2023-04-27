//! Provides datatypes used to describe an application's style using the Azul GUI framework.
#![allow(warnings)]

// #![no_std]

#[macro_use]
extern crate alloc;
extern crate core;

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

#[macro_export]
macro_rules! impl_vec {
    ($struct_type:ident, $struct_name:ident, $destructor_name:ident) => {
        #[repr(C)]
        pub struct $struct_name {
            ptr: *const $struct_type,
            len: usize,
            cap: usize,
            destructor: $destructor_name,
        }

        #[derive(Debug, Copy, Clone)]
        #[repr(C, u8)]
        pub enum $destructor_name {
            DefaultRust,
            NoDestructor,
            External(extern "C" fn(*mut $struct_name)),
        }

        unsafe impl Send for $struct_name {}
        unsafe impl Sync for $struct_name {}

        impl $struct_name {
            #[inline(always)]
            pub fn new() -> $struct_name {
                // lets hope the optimizer catches this
                Self::from_vec(alloc::vec::Vec::new())
            }

            #[inline]
            pub fn with_capacity(cap: usize) -> Self {
                Self::from_vec(alloc::vec::Vec::<$struct_type>::with_capacity(cap))
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
            pub fn from_vec(input: alloc::vec::Vec<$struct_type>) -> Self {
                let ptr = input.as_ptr();
                let len = input.len();
                let cap = input.capacity();

                let _ = ::core::mem::ManuallyDrop::new(input);

                Self {
                    ptr,
                    len,
                    cap,
                    destructor: $destructor_name::DefaultRust,
                }
            }

            #[inline]
            pub fn iter(&self) -> core::slice::Iter<$struct_type> {
                self.as_ref().iter()
            }

            #[inline(always)]
            pub fn ptr_as_usize(&self) -> usize {
                self.ptr as usize
            }

            #[inline(always)]
            pub const fn len(&self) -> usize {
                self.len
            }

            #[inline(always)]
            pub const fn capacity(&self) -> usize {
                self.cap
            }

            #[inline(always)]
            pub const fn is_empty(&self) -> bool {
                self.len == 0
            }

            #[inline(always)]
            pub fn get(&self, index: usize) -> Option<&$struct_type> {
                let v1: &[$struct_type] = self.as_ref();
                let res = v1.get(index);
                res
            }

            #[allow(dead_code)]
            #[inline(always)]
            unsafe fn get_unchecked(&self, index: usize) -> &$struct_type {
                let v1: &[$struct_type] = self.as_ref();
                let res = v1.get_unchecked(index);
                res
            }

            #[inline(always)]
            pub fn as_slice(&self) -> &[$struct_type] {
                self.as_ref()
            }
        }

        impl AsRef<[$struct_type]> for $struct_name {
            fn as_ref(&self) -> &[$struct_type] {
                unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
            }
        }

        impl Default for $struct_name {
            fn default() -> Self {
                Self::from_vec(alloc::vec::Vec::new())
            }
        }

        impl core::iter::FromIterator<$struct_type> for $struct_name {
            fn from_iter<T>(iter: T) -> Self
            where
                T: IntoIterator<Item = $struct_type>,
            {
                Self::from_vec(alloc::vec::Vec::from_iter(iter))
            }
        }

        impl From<alloc::vec::Vec<$struct_type>> for $struct_name {
            fn from(input: alloc::vec::Vec<$struct_type>) -> $struct_name {
                $struct_name::from_vec(input)
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
                    $destructor_name::DefaultRust => {
                        let _ = unsafe {
                            alloc::vec::Vec::from_raw_parts(
                                self.ptr as *mut $struct_type,
                                self.len,
                                self.cap,
                            )
                        };
                    }
                    $destructor_name::NoDestructor => {}
                    $destructor_name::External(f) => {
                        f(self);
                    }
                }
                // necessary so that double-frees are avoided
                self.destructor = $destructor_name::NoDestructor;
            }
        }
    };
}

#[macro_export]
macro_rules! impl_vec_as_hashmap {
    ($struct_type:ident, $struct_name:ident) => {
        impl $struct_name {
            pub fn insert_hm_item(&mut self, item: $struct_type) {
                if !self.contains_hm_item(&item) {
                    let mut vec = self.clone().into_library_owned_vec();
                    vec.push(item);
                    *self = Self::from_vec(vec);
                }
            }

            pub fn remove_hm_item(&mut self, remove_key: &$struct_type) {
                *self = Self::from_vec(
                    self.as_ref()
                        .iter()
                        .filter_map(|r| if *r == *remove_key { None } else { Some(*r) })
                        .collect::<Vec<_>>(),
                );
            }

            pub fn contains_hm_item(&self, searched: &$struct_type) -> bool {
                self.as_ref().iter().any(|i| i == searched)
            }
        }
    };
}

/// NOTE: impl_vec_mut can only exist for vectors that are known to be library-allocated!
#[macro_export]
macro_rules! impl_vec_mut {
    ($struct_type:ident, $struct_name:ident) => {
        impl AsMut<[$struct_type]> for $struct_name {
            fn as_mut(&mut self) -> &mut [$struct_type] {
                unsafe { core::slice::from_raw_parts_mut(self.ptr as *mut $struct_type, self.len) }
            }
        }

        impl From<$struct_name> for alloc::vec::Vec<$struct_type> {
            #[allow(unused_mut)]
            fn from(mut input: $struct_name) -> alloc::vec::Vec<$struct_type> {
                input.into_library_owned_vec()
            }
        }

        impl core::iter::Extend<$struct_type> for $struct_name {
            fn extend<T: core::iter::IntoIterator<Item = $struct_type>>(&mut self, iter: T) {
                for elem in iter {
                    self.push(elem);
                }
            }
        }

        impl $struct_name {
            // <'a> has to live longer thant &'self
            pub fn as_mut_slice_extended<'a>(&mut self) -> &'a mut [$struct_type] {
                unsafe { core::slice::from_raw_parts_mut(self.ptr as *mut $struct_type, self.len) }
            }

            #[inline]
            pub fn as_mut_ptr(&mut self) -> *mut $struct_type {
                self.ptr as *mut $struct_type
            }

            #[inline]
            pub fn sort_by<F: FnMut(&$struct_type, &$struct_type) -> core::cmp::Ordering>(
                &mut self,
                compare: F,
            ) {
                self.as_mut().sort_by(compare);
            }

            #[inline]
            pub fn push(&mut self, value: $struct_type) {
                // code is copied from the rust stdlib, since it's not possible to
                // create a temporary Vec here. Doing that would create two
                if self.len == self.capacity() {
                    self.buf_reserve(self.len, 1);
                }
                unsafe {
                    let end = self.as_mut_ptr().add(self.len);
                    core::ptr::write(end, value);
                    self.len += 1;
                }
            }

            pub fn insert(&mut self, index: usize, element: $struct_type) {
                let len = self.len();
                if index > len {
                    return;
                }

                // space for the new element
                if len == self.capacity() {
                    self.reserve(1);
                }

                unsafe {
                    // infallible
                    // The spot to put the new value
                    {
                        let p = self.as_mut_ptr().add(index);
                        // Shift everything over to make space. (Duplicating the
                        // `index`th element into two consecutive places.)
                        core::ptr::copy(p, p.offset(1), len - index);
                        // Write it in, overwriting the first copy of the `index`th
                        // element.
                        core::ptr::write(p, element);
                    }
                    self.set_len(len + 1);
                }
            }

            pub fn remove(&mut self, index: usize) {
                let len = self.len();
                if index >= len {
                    return;
                }

                unsafe {
                    // infallible
                    let ret;
                    {
                        // the place we are taking from.
                        let ptr = self.as_mut_ptr().add(index);
                        // copy it out, unsafely having a copy of the value on
                        // the stack and in the vector at the same time.
                        ret = core::ptr::read(ptr);

                        // Shift everything down to fill in that spot.
                        core::ptr::copy(ptr.offset(1), ptr, len - index - 1);
                    }
                    self.set_len(len - 1);
                    let _ = ret;
                }
            }

            #[inline]
            pub fn pop(&mut self) -> Option<$struct_type> {
                if self.len == 0 {
                    None
                } else {
                    unsafe {
                        self.len -= 1;
                        Some(core::ptr::read(self.ptr.add(self.len())))
                    }
                }
            }

            #[inline]
            pub fn iter_mut(&mut self) -> core::slice::IterMut<$struct_type> {
                self.as_mut().iter_mut()
            }

            #[inline]
            pub fn into_iter(self) -> alloc::vec::IntoIter<$struct_type> {
                let v1: alloc::vec::Vec<$struct_type> = self.into();
                v1.into_iter()
            }

            #[inline]
            fn amortized_new_size(
                &self,
                used_cap: usize,
                needed_extra_cap: usize,
            ) -> Result<usize, bool> {
                // Nothing we can really do about these checks :(
                let required_cap = used_cap.checked_add(needed_extra_cap).ok_or(true)?;
                // Cannot overflow, because `cap <= isize::MAX`, and type of `cap` is `usize`.
                let double_cap = self.cap * 2;
                // `double_cap` guarantees exponential growth.
                Ok(core::cmp::max(double_cap, required_cap))
            }

            #[inline]
            fn current_layout(&self) -> Option<core::alloc::Layout> {
                if self.cap == 0 {
                    None
                } else {
                    // We have an allocated chunk of memory, so we can bypass runtime
                    // checks to get our current layout.
                    unsafe {
                        let align = core::mem::align_of::<$struct_type>();
                        let size = core::mem::size_of::<$struct_type>() * self.cap;
                        Some(core::alloc::Layout::from_size_align_unchecked(size, align))
                    }
                }
            }

            #[inline]
            fn alloc_guard(alloc_size: usize) -> Result<(), bool> {
                if core::mem::size_of::<usize>() < 8 && alloc_size > ::core::isize::MAX as usize {
                    Err(true)
                } else {
                    Ok(())
                }
            }

            #[inline]
            fn try_reserve(
                &mut self,
                used_cap: usize,
                needed_extra_cap: usize,
            ) -> Result<(), bool> {
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
                let new_layout =
                    alloc::alloc::Layout::array::<$struct_type>(new_cap).map_err(|_| true)?;

                // FIXME: may crash and burn on over-reserve
                $struct_name::alloc_guard(new_layout.size())?;

                let res = unsafe {
                    match self.current_layout() {
                        Some(layout) => {
                            alloc::alloc::realloc(self.ptr as *mut u8, layout, new_layout.size())
                        }
                        None => alloc::alloc::alloc(new_layout),
                    }
                };

                if res == core::ptr::null_mut() {
                    return Err(false);
                }

                self.ptr = res as *mut $struct_type;
                self.cap = new_cap;

                Ok(())
            }

            fn buf_reserve(&mut self, used_cap: usize, needed_extra_cap: usize) {
                match self.try_reserve(used_cap, needed_extra_cap) {
                    Err(true /* Overflow */) => {
                        panic!("memory allocation failed: overflow");
                    }
                    Err(false /* AllocError(_) */) => {
                        panic!("memory allocation failed: error allocating new memory");
                    }
                    Ok(()) => { /* yay */ }
                }
            }

            pub fn append(&mut self, other: &mut Self) {
                unsafe {
                    self.append_elements(other.as_slice() as _);
                    other.set_len(0);
                }
            }

            unsafe fn set_len(&mut self, new_len: usize) {
                debug_assert!(new_len <= self.capacity());
                self.len = new_len;
            }

            pub fn reserve(&mut self, additional: usize) {
                self.buf_reserve(self.len, additional);
            }

            /// Appends elements to `Self` from other buffer.
            #[inline]
            unsafe fn append_elements(&mut self, other: *const [$struct_type]) {
                let count = (*other).len();
                self.reserve(count);
                let len = self.len();
                core::ptr::copy_nonoverlapping(
                    other as *const $struct_type,
                    self.as_mut_ptr().add(len),
                    count,
                );
                self.len += count;
            }

            pub fn truncate(&mut self, len: usize) {
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
                    let s = core::ptr::slice_from_raw_parts_mut(
                        self.as_mut_ptr().add(len),
                        remaining_len,
                    );
                    self.len = len;
                    core::ptr::drop_in_place(s);
                }
            }

            pub fn retain<F>(&mut self, mut f: F)
            where
                F: FnMut(&$struct_type) -> bool,
            {
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
    };
}

#[macro_export]
macro_rules! impl_vec_debug {
    ($struct_type:ident, $struct_name:ident) => {
        impl core::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                self.as_ref().fmt(f)
            }
        }
    };
}

#[macro_export]
macro_rules! impl_vec_partialord {
    ($struct_type:ident, $struct_name:ident) => {
        impl PartialOrd for $struct_name {
            fn partial_cmp(&self, rhs: &Self) -> Option<core::cmp::Ordering> {
                self.as_ref().partial_cmp(rhs.as_ref())
            }
        }
    };
}

#[macro_export]
macro_rules! impl_vec_ord {
    ($struct_type:ident, $struct_name:ident) => {
        impl Ord for $struct_name {
            fn cmp(&self, rhs: &Self) -> core::cmp::Ordering {
                self.as_ref().cmp(rhs.as_ref())
            }
        }
    };
}

#[macro_export]
macro_rules! impl_vec_clone {
    ($struct_type:ident, $struct_name:ident, $destructor_name:ident) => {
        impl $struct_name {
            // Creates a `Vec` from a `Cow<'static, [T]>` - useful to avoid allocating in the case of &'static memory
            #[inline(always)]
            pub fn from_copy_on_write(
                input: alloc::borrow::Cow<'static, [$struct_type]>,
            ) -> $struct_name {
                match input {
                    alloc::borrow::Cow::Borrowed(static_array) => {
                        Self::from_const_slice(static_array)
                    }
                    alloc::borrow::Cow::Owned(owned_vec) => Self::from_vec(owned_vec),
                }
            }

            /// NOTE: CLONES the memory if the memory is external or &'static
            /// Moves the memory out if the memory is library-allocated
            #[inline(always)]
            pub fn clone_self(&self) -> Self {
                match self.destructor {
                    $destructor_name::NoDestructor => Self {
                        ptr: self.ptr,
                        len: self.len,
                        cap: self.cap,
                        destructor: $destructor_name::NoDestructor,
                    },
                    $destructor_name::External(_) | $destructor_name::DefaultRust => {
                        Self::from_vec(self.as_ref().to_vec())
                    }
                }
            }

            /// NOTE: CLONES the memory if the memory is external or &'static
            /// Moves the memory out if the memory is library-allocated
            #[inline(always)]
            pub fn into_library_owned_vec(self) -> alloc::vec::Vec<$struct_type> {
                match self.destructor {
                    $destructor_name::NoDestructor | $destructor_name::External(_) => {
                        self.as_ref().to_vec()
                    }
                    $destructor_name::DefaultRust => {
                        let v = unsafe {
                            alloc::vec::Vec::from_raw_parts(
                                self.ptr as *mut $struct_type,
                                self.len,
                                self.cap,
                            )
                        };
                        core::mem::forget(self);
                        v
                    }
                }
            }
        }
        impl Clone for $struct_name {
            fn clone(&self) -> Self {
                self.clone_self()
            }
        }
    };
}

#[macro_export]
macro_rules! impl_vec_partialeq {
    ($struct_type:ident, $struct_name:ident) => {
        impl PartialEq for $struct_name {
            fn eq(&self, rhs: &Self) -> bool {
                self.as_ref().eq(rhs.as_ref())
            }
        }
    };
}

#[macro_export]
macro_rules! impl_vec_eq {
    ($struct_type:ident, $struct_name:ident) => {
        impl Eq for $struct_name {}
    };
}

#[macro_export]
macro_rules! impl_vec_hash {
    ($struct_type:ident, $struct_name:ident) => {
        impl core::hash::Hash for $struct_name {
            fn hash<H>(&self, state: &mut H)
            where
                H: core::hash::Hasher,
            {
                self.as_ref().hash(state);
            }
        }
    };
}

#[macro_export]
macro_rules! impl_option_inner {
    ($struct_type:ident, $struct_name:ident) => {
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
            fn default() -> $struct_name {
                $struct_name::None
            }
        }

        impl $struct_name {
            pub fn as_option(&self) -> Option<&$struct_type> {
                match self {
                    $struct_name::None => None,
                    $struct_name::Some(t) => Some(t),
                }
            }
            pub fn replace(&mut self, value: $struct_type) -> $struct_name {
                ::core::mem::replace(self, $struct_name::Some(value))
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
            pub const fn as_ref(&self) -> Option<&$struct_type> {
                match *self {
                    $struct_name::Some(ref x) => Some(x),
                    $struct_name::None => None,
                }
            }
            pub fn as_mut(&mut self) -> Option<&mut $struct_type> {
                match self {
                    $struct_name::Some(x) => Some(x),
                    $struct_name::None => None,
                }
            }
            pub fn map<U, F: FnOnce($struct_type) -> U>(self, f: F) -> Option<U> {
                match self {
                    $struct_name::Some(x) => Some(f(x)),
                    $struct_name::None => None,
                }
            }
            pub fn and_then<U, F>(self, f: F) -> Option<U>
            where
                F: FnOnce($struct_type) -> Option<U>,
            {
                match self {
                    $struct_name::None => None,
                    $struct_name::Some(x) => f(x),
                }
            }
        }
    };
}

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
    ($ok_struct_type:ident, $err_struct_type:ident, $struct_name:ident) => {
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
    };
}

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

impl_option!(
    ColorU,
    OptionColorU,
    [Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash]
);

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

mod css;
mod css_properties;
mod print_css;

pub use crate::css::*;
pub use crate::css_properties::*;
pub use crate::print_css::*;
