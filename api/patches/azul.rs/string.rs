
    impl From<std::string::String> for crate::str::String {
        fn from(s: std::string::String) -> crate::str::String {
            crate::str::String::from_utf8_unchecked(s.as_ptr(), s.len()) // - copies s into a new String
            // - s is deallocated here
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

    impl std::fmt::Debug for crate::str::String {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            self.as_str().fmt(f)
        }
    }

    impl crate::str::String {
        #[inline]
        pub fn as_str(&self) -> &str {
            unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.vec.ptr, self.vec.len)) }
        }
        #[inline]
        pub fn as_bytes(&self) -> &[u8] {
            self.vec.as_ref()
        }
        #[inline]
        pub fn into_string(self) -> String {
            String::from(self)
        }
    }