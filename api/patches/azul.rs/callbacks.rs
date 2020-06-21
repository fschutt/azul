
    #[derive(Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
    #[repr(C)]
    pub struct Ref<'a, T> {
        ptr: &'a T,
        _sharing_info_ptr: *const RefAnySharingInfo,
    }

    impl<'a, T> Drop for Ref<'a, T> {
        fn drop(&mut self) {
            (crate::dll::get_azul_dll().az_ref_any_sharing_info_decrease_ref)(unsafe { &mut *(self._sharing_info_ptr as *mut RefAnySharingInfo) });
        }
    }

    impl<'a, T> std::ops::Deref for Ref<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            self.ptr
        }
    }

    #[derive(Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
    #[repr(C)]
    pub struct RefMut<'a, T> {
        ptr: &'a mut T,
        _sharing_info_ptr: *const RefAnySharingInfo,
    }

    impl<'a, T> Drop for RefMut<'a, T> {
        fn drop(&mut self) {
            (crate::dll::get_azul_dll().az_ref_any_sharing_info_decrease_refmut)(unsafe { &mut *(self._sharing_info_ptr as *mut RefAnySharingInfo) });
        }
    }

    impl<'a, T> std::ops::Deref for RefMut<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &*self.ptr
        }
    }

    impl<'a, T> std::ops::DerefMut for RefMut<'a, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.ptr
        }
    }

    impl RefAny {

        /// Creates a new, type-erased pointer by casting the `T` value into a `Vec<u8>` and saving the length + type ID
        pub fn new<T: 'static>(value: T) -> Self {
            use crate::dll::*;

            extern "C" fn default_custom_destructor<U: 'static>(ptr: *const c_void) {
                use std::{mem, ptr};

                // note: in the default constructor, we do not need to check whether U == T

                unsafe {
                    // copy the struct from the heap to the stack and call mem::drop on U to run the destructor
                    let mut stack_mem = mem::MaybeUninit::<U>::uninit().assume_init();
                    ptr::copy_nonoverlapping(ptr as *const U, &mut stack_mem as *mut U, mem::size_of::<U>());
                    mem::drop(stack_mem);
                }
            }

            let type_name_str = ::std::any::type_name::<T>();
            let st = crate::str::String::from_utf8_unchecked(type_name_str.as_ptr(), type_name_str.len());
            let s = (crate::dll::get_azul_dll().az_ref_any_new_c)(
                (&value as *const T) as *const c_void,
                ::std::mem::size_of::<T>(),
                Self::get_type_id::<T>(),
                st,
                default_custom_destructor::<T>,
            );
            ::std::mem::forget(value); // do not run the destructor of T here!
            s
        }

        /// Downcasts the type-erased pointer to a type `&U`, returns `None` if the types don't match
        #[inline]
        pub fn borrow<'a, U: 'static>(&'a self) -> Option<Ref<'a, U>> {
            let is_same_type = (crate::dll::get_azul_dll().az_ref_any_is_type)(self, Self::get_type_id::<U>());
            if !is_same_type { return None; }

            let can_be_shared = (crate::dll::get_azul_dll().az_ref_any_can_be_shared)(self);
            if !can_be_shared { return None; }

            Some(Ref {
                ptr: unsafe { &*(self._internal_ptr as *const U) },
                _sharing_info_ptr: self._sharing_info_ptr,
            })
        }

        /// Downcasts the type-erased pointer to a type `&mut U`, returns `None` if the types don't match
        #[inline]
        pub fn borrow_mut<'a, U: 'static>(&'a mut self) -> Option<RefMut<'a, U>> {
            let is_same_type = (crate::dll::get_azul_dll().az_ref_any_is_type)(self, Self::get_type_id::<U>());
            if !is_same_type { return None; }

            let can_be_shared_mut = (crate::dll::get_azul_dll().az_ref_any_can_be_shared_mut)(self);
            if !can_be_shared_mut { return None; }

            Some(RefMut {
                ptr: unsafe { &mut *(self._internal_ptr as *mut U) },
                _sharing_info_ptr: self._sharing_info_ptr,
            })
        }

        // Returns the typeid of `T` as a u64 (necessary because `std::any::TypeId` is not C-ABI compatible)
        #[inline]
        pub fn get_type_id<T: 'static>() -> u64 {
            use std::any::TypeId;
            use std::mem;

            // fast method to serialize the type id into a u64
            let t_id = TypeId::of::<T>();
            let struct_as_bytes = unsafe { ::std::slice::from_raw_parts((&t_id as *const TypeId) as *const u8, mem::size_of::<TypeId>()) };
            struct_as_bytes.into_iter().enumerate().map(|(s_pos, s)| ((*s as u64) << s_pos)).sum()
        }
    }