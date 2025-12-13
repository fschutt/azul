
    use alloc::string;

    #[cfg(all(feature = "serde-support"))]
    use serde::{Serialize, Deserialize, Serializer, Deserializer};

    #[cfg(feature = "serde-support")]
    impl Serialize for crate::str::String {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer,
        {
            serializer.serialize_str(self.as_str())
        }
    }

    #[cfg(feature = "serde-support")]
    impl<'de> Deserialize<'de> for crate::str::String {
        fn deserialize<D>(deserializer: D) -> Result<crate::str::String, D::Error>
        where D: Deserializer<'de>,
        {
            let s = string::String::deserialize(deserializer)?;
            Ok(s.into())
        }
    }

    impl From<&'static str> for crate::str::String {
        fn from(v: &'static str) -> crate::str::String {
            crate::str::String::from_const_str(v)
        }
    }

    impl From<string::String> for crate::str::String {
        fn from(s: string::String) -> crate::str::String {
            crate::str::String::from_string(s)
        }
    }

    impl AsRef<str> for crate::str::String {
        fn as_ref(&self) -> &str {
            self.as_str()
        }
    }

    impl core::fmt::Debug for crate::str::String {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            self.as_str().fmt(f)
        }
    }

    impl core::fmt::Display for crate::str::String {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            self.as_str().fmt(f)
        }
    }

    impl Clone for crate::str::String {
        fn clone(&self) -> Self {
            Self { vec: self.vec.clone() }
        }
    }

    impl PartialEq for crate::str::String {
        fn eq(&self, other: &Self) -> bool {
            self.as_str() == other.as_str()
        }
    }

    impl PartialOrd for crate::str::String {
        fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
            self.as_str().partial_cmp(other.as_str())
        }
    }

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