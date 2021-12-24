
    use alloc::string;

    #[cfg(all(feature = "serde-support", not(feature = "link_static")))]
    use serde::{Serialize, Deserialize, Serializer, Deserializer};

    #[cfg(not(feature = "link_static"))]
    #[cfg(feature = "serde-support")]
    impl Serialize for crate::str::String {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer,
        {
            serializer.serialize_str(self.as_str())
        }
    }

    #[cfg(not(feature = "link_static"))]
    #[cfg(feature = "serde-support")]
    impl<'de> Deserialize<'de> for crate::str::String {
        fn deserialize<D>(deserializer: D) -> Result<crate::str::String, D::Error>
        where D: Deserializer<'de>,
        {
            let s = string::String::deserialize(deserializer)?;
            Ok(s.into())
        }
    }


    #[cfg(not(feature = "link_static"))]
    impl From<&'static str> for crate::str::String {
        fn from(v: &'static str) -> crate::str::String {
            crate::str::String::from_const_str(v)
        }
    }

    #[cfg(not(feature = "link_static"))]
    impl From<string::String> for crate::str::String {
        fn from(s: string::String) -> crate::str::String {
            crate::str::String::from_string(s)
        }
    }

    #[cfg(not(feature = "link_static"))]
    impl AsRef<str> for crate::str::String {
        fn as_ref(&self) -> &str {
            self.as_str()
        }
    }

    #[cfg(not(feature = "link_static"))]
    impl core::fmt::Debug for crate::str::String {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            self.as_str().fmt(f)
        }
    }

    #[cfg(not(feature = "link_static"))]
    impl core::fmt::Display for crate::str::String {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            self.as_str().fmt(f)
        }
    }

    #[cfg(not(feature = "link_static"))]
    impl crate::str::String {

        #[inline(always)]
        pub fn from_string(s: string::String) -> crate::str::String {
            crate::str::String {
                vec: crate::vec::U8Vec::from_vec(s.into_bytes())
            }
        }

        #[inline(always)]
        pub const fn from_const_str(s: &'static str) -> crate::str::String {
            crate::str::String {
                vec: crate::vec::U8Vec::from_const_slice(s.as_bytes())
            }
        }
    }