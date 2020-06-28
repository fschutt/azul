    #![allow(dead_code, unused_imports)]
    //! Definition of azuls internal `String` wrappers
    use crate::dll::*;
    use std::ffi::c_void;

    impl From<std::string::String> for crate::str::String {
        fn from(s: std::string::String) -> crate::str::String {
            crate::str::String::from_utf8_unchecked(s.as_ptr(), s.len()) // - copies s into a new String
            // - s is deallocated here
        }
    }

    impl From<&str> for crate::str::String {
        fn from(s: &str) -> crate::str::String {
            crate::str::String::from_utf8_unchecked(s.as_ptr(), s.len()) // - copies s into a new String
        }
    }

    impl From<crate::str::String> for std::string::String {
        fn from(s: crate::str::String) -> std::string::String {
            let s_bytes = s.into_bytes();
            unsafe { std::string::String::from_utf8_unchecked(s_bytes.into()) } // - copies s into a new String
            // - s_bytes is deallocated here
        }
    }

    impl AsRef<str> for crate::str::String {
        fn as_ref(&self) -> &str {
            self.as_str()
        }
    }

    impl std::fmt::Display for crate::str::String {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            self.as_str().fmt(f)
        }
    }

    impl crate::str::String {
        #[inline]
        pub fn as_str(&self) -> &str {
            unsafe { std::str::from_utf8_unchecked(self.as_bytes()) }
        }

        #[inline]
        pub fn as_bytes(&self) -> &[u8] {
            self.vec.as_ref()
        }

        #[inline]
        pub fn into_string(self) -> String {
            self.into()
        }
    }

    /// `String` struct
    pub use crate::dll::AzString as String;

    impl String {
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn from_utf8_unchecked(ptr: *const u8, len: usize) -> Self { (crate::dll::get_azul_dll().az_string_from_utf8_unchecked)(ptr, len) }
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn from_utf8_lossy(ptr: *const u8, len: usize) -> Self { (crate::dll::get_azul_dll().az_string_from_utf8_lossy)(ptr, len) }
        /// Returns the internal bytes of the String as a `U8Vec`
        pub fn into_bytes(self)  -> crate::vec::U8Vec { (crate::dll::get_azul_dll().az_string_into_bytes)(self) }
    }

    impl std::fmt::Debug for String { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_string_fmt_debug)(self)) } }
    impl Clone for String { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_string_deep_copy)(self) } }
    impl Drop for String { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_delete)(self); } }
