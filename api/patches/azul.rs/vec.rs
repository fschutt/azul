    #[macro_export]
    macro_rules! impl_vec {($struct_type:ident, $struct_name:ident) => (

        impl $struct_name {

            pub fn new() -> Self {
                Vec::<$struct_type>::new().into()
            }

            pub fn with_capacity(cap: usize) -> Self {
                Vec::<$struct_type>::with_capacity(cap).into()
            }

            pub fn push(&mut self, val: $struct_type) {
                let mut v: Vec<$struct_type> = unsafe { Vec::from_raw_parts(self.ptr as *mut $struct_type, self.len, self.cap) };
                v.push(val);
                let (ptr, len, cap) = Self::into_raw_parts(v);
                self.ptr = ptr;
                self.len = len;
                self.cap = cap;
            }

            pub fn iter(&self) -> std::slice::Iter<$struct_type> {
                let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                v1.iter()
            }

            pub fn iter_mut(&mut self) -> std::slice::IterMut<$struct_type> {
                let v1: &mut [$struct_type] = unsafe { std::slice::from_raw_parts_mut(self.ptr as *mut $struct_type, self.len) };
                v1.iter_mut()
            }

            pub fn into_iter(self) -> std::vec::IntoIter<$struct_type> {
                let v1: Vec<$struct_type> = unsafe { std::vec::Vec::from_raw_parts(self.ptr as *mut $struct_type, self.len, self.cap) };
                std::mem::forget(self); // do not run destructor of self
                v1.into_iter()
            }

            pub fn as_ptr(&self) -> *const $struct_type {
                self.ptr as *const $struct_type
            }

            pub fn len(&self) -> usize {
                self.len
            }

            pub fn is_empty(&self) -> bool {
                self.len == 0
            }

            pub fn cap(&self) -> usize {
                self.cap
            }

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

        impl std::iter::FromIterator<$struct_type> for $struct_name {
            fn from_iter<T>(iter: T) -> Self where T: IntoIterator<Item = $struct_type> {
                let v: Vec<$struct_type> = Vec::from_iter(iter);
                v.into()
            }
        }

        impl AsRef<[$struct_type]> for $struct_name {
            fn as_ref(&self) -> &[$struct_type] {
                unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
            }
        }

        impl From<Vec<$struct_type>> for $struct_name {
            fn from(v: Vec<$struct_type>) -> $struct_name {
                $struct_name::copy_from(v.as_ptr(), v.len())
            }
        }

        impl From<$struct_name> for Vec<$struct_type> {
            fn from(v: $struct_name) -> Vec<$struct_type> {
                unsafe { std::slice::from_raw_parts(v.as_ptr(), v.len()) }.to_vec()
            }
        }
    )}

    impl_vec!(u8, U8Vec);
    impl_vec!(CallbackData, CallbackDataVec);
    impl_vec!(OverrideProperty, OverridePropertyVec);
    impl_vec!(Dom, DomVec);
    impl_vec!(AzString, StringVec);
    impl_vec!(GradientStopPre, GradientStopPreVec);

    impl From<std::vec::Vec<std::string::String>> for crate::vec::StringVec {
        fn from(v: std::vec::Vec<std::string::String>) -> crate::vec::StringVec {
            let vec: Vec<AzString> = v.into_iter().map(|i| {
                let i: std::vec::Vec<u8> = i.into_bytes();
                (crate::dll::get_azul_dll().az_string_from_utf8_unchecked)(i.as_ptr(), i.len())
            }).collect();

            (crate::dll::get_azul_dll().az_string_vec_copy_from)(vec.as_ptr(), vec.len())
        }
    }

    impl From<crate::vec::StringVec> for std::vec::Vec<std::string::String> {
        fn from(v: crate::vec::StringVec) -> std::vec::Vec<std::string::String> {
            unsafe { std::slice::from_raw_parts(v.ptr, v.len) }
            .iter()
            .map(|s| unsafe {
                let s: AzString = (crate::dll::get_azul_dll().az_string_deep_copy)(s);
                let s_vec: std::vec::Vec<u8> = s.into_bytes().into();
                std::string::String::from_utf8_unchecked(s_vec)
            })
            .collect()

            // delete() not necessary because StringVec is stack-allocated
        }
    }