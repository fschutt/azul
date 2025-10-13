use alloc::boxed::Box;
use core::{
    alloc::Layout,
    ffi::c_void,
    fmt,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};

use azul_css::AzString;

#[derive(Debug)]
#[repr(C)]
pub struct RefCountInner {
    pub num_copies: AtomicUsize,
    pub num_refs: AtomicUsize,
    pub num_mutable_refs: AtomicUsize,
    pub _internal_len: usize,
    pub _internal_layout_size: usize,
    pub _internal_layout_align: usize,
    pub type_id: u64,
    pub type_name: AzString,
    pub custom_destructor: extern "C" fn(*mut c_void),
}

#[derive(Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(C)]
pub struct RefCount {
    pub ptr: *const RefCountInner,
    pub run_destructor: bool,
}

impl fmt::Debug for RefCount {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.downcast().fmt(f)
    }
}

impl Clone for RefCount {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            run_destructor: true,
        }
    }
}

impl Drop for RefCount {
    fn drop(&mut self) {
        self.run_destructor = false;
        // note: the owning struct of the RefCount has to do the dropping!
    }
}

#[derive(Debug, Clone)]
pub struct RefCountInnerDebug {
    pub num_copies: usize,
    pub num_refs: usize,
    pub num_mutable_refs: usize,
    pub _internal_len: usize,
    pub _internal_layout_size: usize,
    pub _internal_layout_align: usize,
    pub type_id: u64,
    pub type_name: AzString,
    pub custom_destructor: usize,
}

impl RefCount {
    fn new(ref_count: RefCountInner) -> Self {
        RefCount {
            ptr: Box::into_raw(Box::new(ref_count)),
            run_destructor: true,
        }
    }
    fn downcast(&self) -> &RefCountInner {
        unsafe { &*self.ptr }
    }

    pub fn debug_get_refcount_copied(&self) -> RefCountInnerDebug {
        let dc = self.downcast();
        RefCountInnerDebug {
            num_copies: dc.num_copies.load(AtomicOrdering::SeqCst),
            num_refs: dc.num_refs.load(AtomicOrdering::SeqCst),
            num_mutable_refs: dc.num_mutable_refs.load(AtomicOrdering::SeqCst),
            _internal_len: dc._internal_len,
            _internal_layout_size: dc._internal_layout_size,
            _internal_layout_align: dc._internal_layout_align,
            type_id: dc.type_id,
            type_name: dc.type_name.clone(),
            custom_destructor: dc.custom_destructor as usize,
        }
    }

    /// Runtime check to check whether this `RefAny` can be borrowed
    pub fn can_be_shared(&self) -> bool {
        self.downcast()
            .num_mutable_refs
            .load(AtomicOrdering::SeqCst)
            == 0
    }

    /// Runtime check to check whether this `RefAny` can be borrowed mutably
    pub fn can_be_shared_mut(&self) -> bool {
        let info = self.downcast();
        info.num_mutable_refs.load(AtomicOrdering::SeqCst) == 0
            && info.num_refs.load(AtomicOrdering::SeqCst) == 0
    }

    pub fn increase_ref(&self) {
        self.downcast()
            .num_refs
            .fetch_add(1, AtomicOrdering::SeqCst);
    }

    pub fn decrease_ref(&self) {
        self.downcast()
            .num_refs
            .fetch_sub(1, AtomicOrdering::SeqCst);
    }

    pub fn increase_refmut(&self) {
        self.downcast()
            .num_mutable_refs
            .fetch_add(1, AtomicOrdering::SeqCst);
    }

    pub fn decrease_refmut(&self) {
        self.downcast()
            .num_mutable_refs
            .fetch_sub(1, AtomicOrdering::SeqCst);
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct Ref<'a, T> {
    ptr: &'a T,
    sharing_info: RefCount,
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

#[derive(Debug)]
#[repr(C)]
pub struct RefMut<'a, T> {
    ptr: &'a mut T,
    sharing_info: RefCount,
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

#[derive(Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(C)]
pub struct RefAny {
    /// void* to a boxed struct or enum of type "T". RefCount stores the RTTI
    /// for this opaque type (can be downcasted by the user)
    pub _internal_ptr: *const c_void,
    /// All the metadata information is set on the refcount, so that the metadata
    /// has to only be created once per object, not once per copy
    pub sharing_info: RefCount,
    /// Instance of this copy (root = 0th copy).
    ///
    /// Necessary to distinguish between the original copy and all other clones
    pub instance_id: u64,
    pub run_destructor: bool,
}

impl_option!(
    RefAny,
    OptionRefAny,
    copy = false,
    [Debug, Hash, Clone, PartialEq, PartialOrd, Ord, Eq]
);

// the refcount of RefAny is atomic, therefore `RefAny` is not `Sync`, but it is `Send`
unsafe impl Send for RefAny {}
// library-internal only - RefAny is not Sync outside of this library!
unsafe impl Sync for RefAny {} // necessary for rayon to work

impl RefAny {
    /// Creates a new, type-erased pointer by casting the `T` value into a
    /// `Vec<u8>` and saving the length + type ID
    pub fn new<T: 'static>(value: T) -> Self {
        extern "C" fn default_custom_destructor<U: 'static>(ptr: &mut c_void) {
            use core::{mem, ptr};

            // note: in the default constructor, we do not need to check whether U == T

            unsafe {
                // copy the struct from the heap to the stack and
                // call mem::drop on U to run the destructor
                let mut stack_mem = mem::MaybeUninit::<U>::uninit();
                ptr::copy_nonoverlapping(
                    (ptr as *mut c_void) as *const U,
                    stack_mem.as_mut_ptr(),
                    mem::size_of::<U>(),
                );
                let stack_mem = stack_mem.assume_init();
                mem::drop(stack_mem);
            }
        }

        let type_name = ::core::any::type_name::<T>();
        let st = AzString::from_const_str(type_name);
        let s = Self::new_c(
            (&value as *const T) as *const c_void,
            ::core::mem::size_of::<T>(),
            Self::get_type_id_static::<T>(),
            st,
            default_custom_destructor::<T>,
        );
        ::core::mem::forget(value); // do not run the destructor of T here!
        s
    }

    /// C-ABI compatible function to create a `RefAny` across the C boundary
    pub fn new_c(
        // *const T
        ptr: *const c_void,
        // sizeof(T)
        len: usize,
        // unique ID of the type (used for type comparison when downcasting)
        type_id: u64,
        // name of the class such as "app::MyData", usually compiler- or macro-generated
        type_name: AzString,
        custom_destructor: extern "C" fn(&mut c_void),
    ) -> Self {
        use core::ptr;

        // special case: calling alloc() with 0 bytes would be undefined behaviour
        //
        // In order to invoke the destructor correctly, we need a 0-sized allocation
        // on the heap (NOT nullptr, as this would lead to UB when calling the destructor)
        let (_internal_ptr, layout) = if len == 0 {
            let _dummy: [u8; 0] = [];
            (ptr::null_mut(), Layout::for_value(&_dummy))
        } else {
            // cast the struct as bytes
            let struct_as_bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
            let layout = Layout::for_value(&*struct_as_bytes);

            // allocate + copy the struct to the heap
            let heap_struct_as_bytes = unsafe { alloc::alloc::alloc(layout) };
            unsafe {
                ptr::copy_nonoverlapping(
                    struct_as_bytes.as_ptr(),
                    heap_struct_as_bytes,
                    struct_as_bytes.len(),
                )
            };
            (heap_struct_as_bytes, layout)
        };

        let ref_count_inner = RefCountInner {
            num_copies: AtomicUsize::new(1),
            num_refs: AtomicUsize::new(0),
            num_mutable_refs: AtomicUsize::new(0),
            _internal_len: len,
            _internal_layout_size: layout.size(),
            _internal_layout_align: layout.align(),
            type_id,
            type_name,
            // fn(&mut c_void) and fn(*mut c_void) are the same, so transmute is safe
            custom_destructor: unsafe { core::mem::transmute(custom_destructor) },
        };

        Self {
            _internal_ptr: _internal_ptr as *const c_void,
            sharing_info: RefCount::new(ref_count_inner),
            instance_id: 0,
            run_destructor: true,
        }
    }

    /// Returns whether this RefAny is the only instance
    pub fn has_no_copies(&self) -> bool {
        self.sharing_info
            .downcast()
            .num_copies
            .load(AtomicOrdering::SeqCst)
            == 1
            && self
                .sharing_info
                .downcast()
                .num_refs
                .load(AtomicOrdering::SeqCst)
                == 0
            && self
                .sharing_info
                .downcast()
                .num_mutable_refs
                .load(AtomicOrdering::SeqCst)
                == 0
    }

    /// Downcasts the type-erased pointer to a type `&U`, returns `None` if the types don't match
    #[inline]
    pub fn downcast_ref<'a, U: 'static>(&'a mut self) -> Option<Ref<'a, U>> {
        let is_same_type = self.get_type_id() == Self::get_type_id_static::<U>();
        if !is_same_type {
            return None;
        }

        let can_be_shared = self.sharing_info.can_be_shared();
        if !can_be_shared {
            return None;
        }

        if self._internal_ptr.is_null() {
            return None;
        }
        self.sharing_info.increase_ref();
        Some(Ref {
            ptr: unsafe { &*(self._internal_ptr as *const U) },
            sharing_info: self.sharing_info.clone(),
        })
    }

    /// Downcasts the type-erased pointer to a type `&mut U`, returns `None` if the types don't
    /// match
    #[inline]
    pub fn downcast_mut<'a, U: 'static>(&'a mut self) -> Option<RefMut<'a, U>> {
        let is_same_type = self.get_type_id() == Self::get_type_id_static::<U>();
        if !is_same_type {
            return None;
        }

        let can_be_shared_mut = self.sharing_info.can_be_shared_mut();
        if !can_be_shared_mut {
            return None;
        }

        if self._internal_ptr.is_null() {
            return None;
        }
        self.sharing_info.increase_refmut();

        Some(RefMut {
            ptr: unsafe { &mut *(self._internal_ptr as *mut U) },
            sharing_info: self.sharing_info.clone(),
        })
    }

    // Returns the typeid of `T` as a u64 (necessary because
    // `core::any::TypeId` is not C-ABI compatible)
    #[inline]
    fn get_type_id_static<T: 'static>() -> u64 {
        use core::{any::TypeId, mem};

        // fast method to serialize the type id into a u64
        let t_id = TypeId::of::<T>();
        let struct_as_bytes = unsafe {
            core::slice::from_raw_parts(
                (&t_id as *const TypeId) as *const u8,
                mem::size_of::<TypeId>(),
            )
        };

        struct_as_bytes
            .into_iter()
            .enumerate()
            .map(|(s_pos, s)| ((*s as u64) << s_pos))
            .sum()
    }

    /// Checks whether the typeids match
    pub fn is_type(&self, type_id: u64) -> bool {
        self.sharing_info.downcast().type_id == type_id
    }

    // Returns the internal type ID
    pub fn get_type_id(&self) -> u64 {
        self.sharing_info.downcast().type_id
    }

    // Returns the type name
    pub fn get_type_name(&self) -> AzString {
        self.sharing_info.downcast().type_name.clone()
    }
}

impl Clone for RefAny {
    fn clone(&self) -> Self {
        self.sharing_info
            .downcast()
            .num_copies
            .fetch_add(1, AtomicOrdering::SeqCst);
        Self {
            _internal_ptr: self._internal_ptr,
            sharing_info: RefCount {
                ptr: self.sharing_info.ptr,
                run_destructor: true,
            },
            instance_id: self
                .sharing_info
                .downcast()
                .num_copies
                .load(AtomicOrdering::SeqCst) as u64,
            run_destructor: true,
        }
    }
}

impl Drop for RefAny {
    fn drop(&mut self) {
        use core::ptr;

        self.run_destructor = false;

        let current_copies = self
            .sharing_info
            .downcast()
            .num_copies
            .fetch_sub(1, AtomicOrdering::SeqCst);

        if current_copies != 1 {
            return;
        }

        let sharing_info = unsafe { Box::from_raw(self.sharing_info.ptr as *mut RefCountInner) };
        let sharing_info = *sharing_info; // sharing_info itself deallocates here

        if sharing_info._internal_len == 0
            || sharing_info._internal_layout_size == 0
            || self._internal_ptr.is_null()
        {
            let mut _dummy: [u8; 0] = [];
            (sharing_info.custom_destructor)(_dummy.as_ptr() as *mut c_void);
        } else {
            let layout = unsafe {
                Layout::from_size_align_unchecked(
                    sharing_info._internal_layout_size,
                    sharing_info._internal_layout_align,
                )
            };

            (sharing_info.custom_destructor)(self._internal_ptr as *mut c_void);
            unsafe {
                alloc::alloc::dealloc(self._internal_ptr as *mut u8, layout);
            }
        }
    }
}
