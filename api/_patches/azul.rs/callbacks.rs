
    #[cfg_attr(not(feature = "link_static"), derive(Debug))]
    #[repr(C)]
    pub struct Ref<'a, T> {
        ptr: &'a T,
        #[cfg(not(feature = "link_static"))]
        sharing_info: RefCount,
        #[cfg(feature = "link_static")]
        sharing_info: azul::AzRefCountTT,
    }

    impl<'a, T> Drop for Ref<'a, T> {
        fn drop(&mut self) {
            self.sharing_info.decrease_ref();
        }
    }

    impl<'a, T> core::ops::Deref for Ref<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            self.ptr
        }
    }

    #[cfg_attr(not(feature = "link_static"), derive(Debug))]
    #[repr(C)]
    pub struct RefMut<'a, T> {
        ptr: &'a mut T,
        #[cfg(not(feature = "link_static"))]
        sharing_info: RefCount,
        #[cfg(feature = "link_static")]
        sharing_info: azul::AzRefCountTT,
    }

    impl<'a, T> Drop for RefMut<'a, T> {
        fn drop(&mut self) {
            self.sharing_info.decrease_refmut();
        }
    }

    impl<'a, T> core::ops::Deref for RefMut<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &*self.ptr
        }
    }

    impl<'a, T> core::ops::DerefMut for RefMut<'a, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.ptr
        }
    }

    #[cfg(not(feature = "link_static"))]
    impl RefAny {

        /// Creates a new, type-erased pointer by casting the `T` value into a `Vec<u8>` and saving the length + type ID
        pub fn new<T: 'static>(value: T) -> Self {
            use crate::dll::*;

            extern "C" fn default_custom_destructor<U: 'static>(ptr: &mut c_void) {
                use core::{mem, ptr};

                // note: in the default constructor, we do not need to check whether U == T

                unsafe {
                    // copy the struct from the heap to the stack and
                    // call mem::drop on U to run the destructor
                    let mut stack_mem = mem::MaybeUninit::<U>::uninit();
                    ptr::copy_nonoverlapping((ptr as *mut c_void) as *const U, stack_mem.as_mut_ptr(), mem::size_of::<U>());
                    let stack_mem = stack_mem.assume_init();
                    mem::drop(stack_mem);
                }
            }

            let type_name_str = ::core::any::type_name::<T>();
            let st = crate::str::String::from_const_str(type_name_str);
            let s = unsafe { crate::dll::AzRefAny_newC(
                (&value as *const T) as *const c_void,
                ::core::mem::size_of::<T>(),
                Self::type_id::<T>(),
                st,
                default_custom_destructor::<T>,
            ) };
            ::core::mem::forget(value); // do not run the destructor of T here!
            s
        }

        /// Downcasts the type-erased pointer to a type `&U`, returns `None` if the types don't match
        #[inline]
        pub fn downcast_ref<'a, U: 'static>(&'a mut self) -> Option<Ref<'a, U>> {
            let is_same_type = self.get_type_id() == Self::type_id::<U>();
            if !is_same_type { return None; }

            let can_be_shared = self.sharing_info.can_be_shared();
            if !can_be_shared { return None; }

            self.sharing_info.increase_ref();
            Some(Ref {
                ptr: unsafe { &*(self._internal_ptr as *const U) },
                sharing_info: self.sharing_info.clone(),
            })
        }

        /// Downcasts the type-erased pointer to a type `&mut U`, returns `None` if the types don't match
        #[inline]
        pub fn downcast_mut<'a, U: 'static>(&'a mut self) -> Option<RefMut<'a, U>> {
            let is_same_type = self.get_type_id() == Self::type_id::<U>();
            if !is_same_type { return None; }

            let can_be_shared_mut = self.sharing_info.can_be_shared_mut();
            if !can_be_shared_mut { return None; }

            self.sharing_info.increase_refmut();

            Some(RefMut {
                ptr: unsafe { &mut *(self._internal_ptr as *mut U) },
                sharing_info: self.sharing_info.clone(),
            })
        }

        // Returns the typeid of `T` as a u64 (necessary because `core::any::TypeId` is not C-ABI compatible)
        #[inline]
        pub fn type_id<T: 'static>() -> u64 {
            use core::any::TypeId;
            use core::mem;

            // fast method to serialize the type id into a u64
            let t_id = TypeId::of::<T>();
            let struct_as_bytes = unsafe { ::core::slice::from_raw_parts((&t_id as *const TypeId) as *const u8, mem::size_of::<TypeId>()) };
            struct_as_bytes.into_iter().enumerate().map(|(s_pos, s)| ((*s as u64) << s_pos)).sum()
        }
    }