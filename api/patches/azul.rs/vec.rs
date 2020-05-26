
    impl From<std::vec::Vec<u8>> for crate::vec::U8Vec {
        fn from(v: std::vec::Vec<u8>) -> crate::vec::U8Vec {
            crate::vec::U8Vec::copy_from(v.as_ptr(), v.len())
        }
    }

    impl From<crate::vec::U8Vec> for std::vec::Vec<u8> {
        fn from(v: crate::vec::U8Vec) -> std::vec::Vec<u8> {
            unsafe { std::slice::from_raw_parts(v.object.object.as_ptr(), v.object.object.len()).to_vec() }
        }
    }

    impl From<std::vec::Vec<std::string::String>> for crate::vec::StringVec {
        fn from(v: std::vec::Vec<std::string::String>) -> crate::vec::StringVec {
            crate::vec::StringVec { object: v.into_iter().map(|i| azul_dll::AzString::copy_from(i.as_ptr(), i.len())).collect() }
        }
    }

    impl From<crate::vec::StringVec> for std::vec::Vec<std::string::String> {
        fn from(v: crate::vec::StringVec) -> std::vec::Vec<std::string::String> {
            v.object.object
            .into_iter()
            .map(|s| unsafe { std::string::String::from_utf8_unchecked(s.as_ptr(), s.len()) })
            .collect()
        }
    }