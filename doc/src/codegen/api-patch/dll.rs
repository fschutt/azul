
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

    unsafe impl Send for AzThreadSender { }

    // NOTE: Callback trait implementations (Debug, Clone, Hash, PartialEq, PartialOrd, Ord, Eq)
    // are now automatically generated in struct_gen.rs for all structs with a `cb` field.
    // This eliminates the need for ~240 lines of manual implementations that were here previously.
