    #![allow(dead_code, unused_imports)]
    //! Definition of azuls internal `String` wrappers
    use crate::dll::*;
    use core::ffi::c_void;

    use alloc::string;

    impl From<string::String> for crate::str::String {
        fn from(s: string::String) -> crate::str::String {
            crate::str::String::from_utf8_unchecked(s.as_ptr(), s.len()) // - copies s into a new String
            // - s is deallocated here
        }
    }

    impl From<&str> for crate::str::String {
        fn from(s: &str) -> crate::str::String {
            crate::str::String::from_utf8_unchecked(s.as_ptr(), s.len()) // - copies s into a new String
        }
    }

    impl From<crate::str::String> for string::String {
        fn from(s: crate::str::String) -> string::String {
            let s_bytes = s.into_bytes();
            unsafe { string::String::from_utf8_unchecked(s_bytes.into()) } // - copies s into a new String
            // - s_bytes is deallocated here
        }
    }

    impl AsRef<str> for crate::str::String {
        fn as_ref(&self) -> &str {
            self.as_str()
        }
    }

    impl core::fmt::Display for crate::str::String {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            self.as_str().fmt(f)
        }
    }

    impl crate::str::String {
        #[inline]
        pub fn into_string(self) -> String {
            self.into()
        }
    }

    /// `String` struct
    #[doc(inline)] pub use crate::dll::AzString as String;

    impl String {
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn from_utf8_unchecked(ptr: *const u8, len: usize) -> Self { unsafe { crate::dll::az_string_from_utf8_unchecked(ptr, len) } }
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn from_utf8_lossy(ptr: *const u8, len: usize) -> Self { unsafe { crate::dll::az_string_from_utf8_lossy(ptr, len) } }
        /// Returns the internal bytes of the String as a `U8Vec`
        pub fn into_bytes(self)  -> crate::vec::U8Vec { unsafe { crate::dll::az_string_into_bytes(self) } }
    }

    impl Clone for String { fn clone(&self) -> Self { unsafe { crate::dll::az_string_deep_copy(self) } } }
    impl Drop for String { fn drop(&mut self) { unsafe { crate::dll::az_string_delete(self) }; } }
