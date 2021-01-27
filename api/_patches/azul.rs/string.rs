
    use alloc::string;


    impl From<&'static str> for crate::str::String {
        fn from(v: &'static str) -> crate::str::String {
            crate::str::String::from_const_str(v)
        }
    }

    impl From<string::String> for crate::str::String {
        fn from(s: string::String) -> crate::str::String {
            crate::str::String::from_utf8_unchecked(s.as_ptr(), s.len()) // - copies s into a new String
            // - s is deallocated here
        }
    }

    impl From<crate::str::String> for string::String {
        fn from(s: crate::str::String) -> string::String {
            s.as_str().into()
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

        #[inline(always)]
        pub const fn from_const_str(s: &'static str) -> Self {
            String {
                vec: crate::vec::U8Vec::from_const_slice(s.as_bytes())
            }
        }
    }