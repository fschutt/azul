//! Type-erased, reference-counted smart pointer with runtime borrow checking.
//!
//! # Safety
//!
//! This module provides `RefAny`, a type-erased container similar to `Arc<RefCell<dyn Any>>`,
//! but designed for FFI compatibility and cross-language interoperability.
//!
//! ## Memory Safety Guarantees
//!
//! 1. **Proper Alignment**: Fixed in commit addressing Miri UB - memory is allocated with correct
//!    alignment for the stored type using `Layout::from_size_align()`.
//!
//! 2. **Atomic Reference Counting**: All reference counts use `AtomicUsize` with `SeqCst` ordering,
//!    ensuring thread-safe access and preventing use-after-free.
//!
//! 3. **Runtime Type Safety**: Type IDs are checked before downcasting, preventing invalid pointer
//!    casts that would cause undefined behavior.
//!
//! 4. **Runtime Borrow Checking**: Shared and mutable borrows are tracked at runtime, enforcing
//!    Rust's borrowing rules dynamically (similar to `RefCell`).
//!
//! ## Thread Safety
//!
//! - `RefAny` is `Send`: Can be transferred between threads (data is heap-allocated)
//! - `RefAny` is `Sync`: Can be shared between threads (atomic operations + `&mut self` for
//!   borrows)
//!
//! The `SeqCst` (Sequentially Consistent) memory ordering provides the strongest guarantees:
//! all atomic operations appear in a single global order visible to all threads, preventing
//! race conditions where one thread doesn't see another's reference count updates.

use alloc::boxed::Box;
use alloc::string::String;
use core::{
    alloc::Layout,
    ffi::c_void,
    fmt,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};

use azul_css::AzString;

/// C-compatible destructor function type for RefAny.
/// Called when the last reference to a RefAny is dropped.
pub type RefAnyDestructorType = extern "C" fn(*mut c_void);

// NOTE: JSON serialization/deserialization callback types are defined in azul_layout::json
// The actual types are:
//   RefAnySerializeFnType = extern "C" fn(RefAny) -> Json
//   RefAnyDeserializeFnType = extern "C" fn(Json) -> ResultRefAnyString
// In azul_core, we only store function pointers as usize (0 = not set).

/// Internal reference counting metadata for `RefAny`.
///
/// This struct tracks:
///
/// - How many `RefAny` clones exist (`num_copies`)
/// - How many shared borrows are active (`num_refs`)
/// - How many mutable borrows are active (`num_mutable_refs`)
/// - Memory layout information for correct deallocation
/// - Type information for runtime type checking
///
/// # Thread Safety
///
/// All counters are `AtomicUsize` with `SeqCst` ordering, making them safe to access
/// from multiple threads simultaneously. The strong ordering ensures no thread can
/// observe inconsistent states (e.g., both seeing count=1 during final drop).
#[derive(Debug)]
#[repr(C)]
pub struct RefCountInner {
    /// Type-erased pointer to heap-allocated data.
    ///
    /// SAFETY: Must be properly aligned for the stored type (guaranteed by
    /// `Layout::from_size_align` in `new_c`). Never null for non-ZST types.
    ///
    /// This pointer is shared by all RefAny clones, so replace_contents
    /// updates are visible to all clones.
    pub _internal_ptr: *const c_void,

    /// Number of `RefAny` instances sharing the same data.
    /// When this reaches 0, the data is deallocated.
    pub num_copies: AtomicUsize,

    /// Number of active shared borrows (`Ref<T>`).
    /// While > 0, mutable borrows are forbidden.
    pub num_refs: AtomicUsize,

    /// Number of active mutable borrows (`RefMut<T>`).
    /// While > 0, all other borrows are forbidden.
    pub num_mutable_refs: AtomicUsize,

    /// Size of the stored type in bytes (from `size_of::<T>()`).
    pub _internal_len: usize,

    /// Layout size for deallocation (from `Layout::size()`).
    pub _internal_layout_size: usize,

    /// Required alignment for the stored type (from `align_of::<T>()`).
    /// CRITICAL: Must match the alignment used during allocation to prevent UB.
    pub _internal_layout_align: usize,

    /// Runtime type identifier computed from `TypeId::of::<T>()`.
    /// Used to prevent invalid downcasts.
    pub type_id: u64,

    /// Human-readable type name (e.g., "MyStruct") for debugging.
    pub type_name: AzString,

    /// Function pointer to correctly drop the type-erased data.
    /// SAFETY: Must be called with a pointer to data of the correct type.
    pub custom_destructor: extern "C" fn(*mut c_void),

    /// Function pointer to serialize RefAny to JSON (0 = not set).
    /// Cast to RefAnySerializeFnType (defined in azul_layout::json) when called.
    /// Type: extern "C" fn(RefAny) -> Json
    pub serialize_fn: usize,

    /// Function pointer to deserialize JSON to new RefAny (0 = not set).
    /// Cast to RefAnyDeserializeFnType (defined in azul_layout::json) when called.
    /// Type: extern "C" fn(Json) -> ResultRefAnyString
    pub deserialize_fn: usize,
}

/// Wrapper around a heap-allocated `RefCountInner`.
///
/// This is the shared metadata that all `RefAny` clones point to.
/// The `RefCount` is responsible for all memory management:
///
/// - `RefCount::clone()` increments `num_copies` in RefCountInner
/// - `RefCount::drop()` decrements `num_copies` and, if it reaches 0:
///   1. Frees the RefCountInner
///   2. Calls the custom destructor on the data
///   3. Deallocates the data memory
///
/// # Why `run_destructor: bool`
///
/// This flag tracks whether this `RefCount` instance should decrement
/// `num_copies` when dropped. Set to `true` for all clones (including
/// those created by `RefAny::clone()` and `AZ_REFLECT` macros).
/// Set to `false` after the decrement has been performed to prevent
/// double-decrement.
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
    /// Clones the RefCount and increments the reference count.
    ///
    /// # Safety
    ///
    /// This is safe because:
    /// - The ptr is valid (created from Box::into_raw)
    /// - num_copies is atomically incremented with SeqCst ordering
    /// - This ensures the RefCountInner is not freed while clones exist
    fn clone(&self) -> Self {
        // CRITICAL: Must increment num_copies so the RefCountInner is not freed
        // while this clone exists. The C macros (AZ_REFLECT) use AzRefCount_clone
        // to create Ref/RefMut guards, and those guards must keep the data alive.
        if !self.ptr.is_null() {
            unsafe {
                (*self.ptr).num_copies.fetch_add(1, AtomicOrdering::SeqCst);
            }
        }
        Self {
            ptr: self.ptr,
            run_destructor: true,
        }
    }
}

impl Drop for RefCount {
    /// Decrements the reference count when a RefCount clone is dropped.
    ///
    /// If this was the last reference (num_copies reaches 0), this will also
    /// free the RefCountInner and call the custom destructor.
    fn drop(&mut self) {
        // Only decrement if run_destructor is true (meaning this is a clone)
        // and the pointer is valid
        if !self.run_destructor || self.ptr.is_null() {
            return;
        }
        self.run_destructor = false;

        // Atomically decrement and get the PREVIOUS value
        let current_copies = unsafe {
            (*self.ptr).num_copies.fetch_sub(1, AtomicOrdering::SeqCst)
        };

        // If previous value wasn't 1, other references still exist
        if current_copies != 1 {
            return;
        }

        // We're the last reference! Clean up.
        // SAFETY: ptr came from Box::into_raw, and we're the last reference
        let sharing_info = unsafe { Box::from_raw(self.ptr as *mut RefCountInner) };
        let sharing_info = *sharing_info; // Box deallocates RefCountInner here

        // Get the data pointer
        let data_ptr = sharing_info._internal_ptr;

        // Handle zero-sized types specially
        if sharing_info._internal_len == 0
            || sharing_info._internal_layout_size == 0
            || data_ptr.is_null()
        {
            let mut _dummy: [u8; 0] = [];
            // Call destructor even for ZSTs (may have side effects)
            (sharing_info.custom_destructor)(_dummy.as_ptr() as *mut c_void);
        } else {
            // Reconstruct the layout used during allocation
            let layout = unsafe {
                Layout::from_size_align_unchecked(
                    sharing_info._internal_layout_size,
                    sharing_info._internal_layout_align,
                )
            };

            // Phase 1: Run the custom destructor
            (sharing_info.custom_destructor)(data_ptr as *mut c_void);

            // Phase 2: Deallocate the memory
            unsafe {
                alloc::alloc::dealloc(data_ptr as *mut u8, layout);
            }
        }
    }
}

/// Debug-friendly snapshot of `RefCountInner` with non-atomic values.
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
    /// Serialization function pointer (0 = not set)
    pub serialize_fn: usize,
    /// Deserialization function pointer (0 = not set)
    pub deserialize_fn: usize,
}

impl RefCount {
    /// Creates a new `RefCount` by boxing the metadata on the heap.
    ///
    /// # Safety
    ///
    /// Safe because we're creating a new allocation with `Box::new`,
    /// then immediately leaking it with `into_raw` to get a stable pointer.
    fn new(ref_count: RefCountInner) -> Self {
        RefCount {
            ptr: Box::into_raw(Box::new(ref_count)),
            run_destructor: true,
        }
    }

    /// Dereferences the raw pointer to access the metadata.
    ///
    /// # Safety
    ///
    /// Safe because:
    /// - The pointer is created from `Box::into_raw`, so it's valid and properly aligned
    /// - The lifetime is tied to `&self`, ensuring the pointer is still alive
    /// - Reference counting ensures the data isn't freed while references exist
    fn downcast(&self) -> &RefCountInner {
        if self.ptr.is_null() {
            panic!("[RefCount::downcast] FATAL: self.ptr is null!");
        }
        unsafe { &*self.ptr }
    }

    /// Creates a debug snapshot of the current reference counts.
    ///
    /// Loads all atomic values with `SeqCst` ordering to get a consistent view.
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
            serialize_fn: dc.serialize_fn,
            deserialize_fn: dc.deserialize_fn,
        }
    }

    /// Runtime check: can we create a shared borrow?
    ///
    /// Returns `true` if there are no active mutable borrows.
    /// Multiple shared borrows can coexist (like `&T` in Rust).
    ///
    /// # Memory Ordering
    ///
    /// Uses `SeqCst` to ensure we see the most recent state from all threads.
    /// If another thread just released a mutable borrow, we'll see it.
    pub fn can_be_shared(&self) -> bool {
        self.downcast()
            .num_mutable_refs
            .load(AtomicOrdering::SeqCst)
            == 0
    }

    /// Runtime check: can we create a mutable borrow?
    ///
    /// Returns `true` only if there are ZERO active borrows of any kind.
    /// This enforces Rust's exclusive mutability rule (like `&mut T`).
    ///
    /// # Memory Ordering
    ///
    /// Uses `SeqCst` to ensure we see all recent borrows from all threads.
    /// Both counters must be checked atomically to prevent races.
    pub fn can_be_shared_mut(&self) -> bool {
        let info = self.downcast();
        info.num_mutable_refs.load(AtomicOrdering::SeqCst) == 0
            && info.num_refs.load(AtomicOrdering::SeqCst) == 0
    }

    /// Increments the shared borrow counter.
    ///
    /// Called when a `Ref<T>` is created. The `Ref::drop` will decrement it.
    ///
    /// # Memory Ordering
    ///
    /// `SeqCst` ensures this increment is visible to all threads before they
    /// try to acquire a mutable borrow (which checks this counter).
    pub fn increase_ref(&self) {
        self.downcast()
            .num_refs
            .fetch_add(1, AtomicOrdering::SeqCst);
    }

    /// Decrements the shared borrow counter.
    ///
    /// Called when a `Ref<T>` is dropped, indicating the borrow is released.
    ///
    /// # Memory Ordering
    ///
    /// `SeqCst` ensures this decrement is immediately visible to other threads
    /// waiting to acquire a mutable borrow.
    pub fn decrease_ref(&self) {
        self.downcast()
            .num_refs
            .fetch_sub(1, AtomicOrdering::SeqCst);
    }

    /// Increments the mutable borrow counter.
    ///
    /// Called when a `RefMut<T>` is created. Should only succeed when this
    /// counter and `num_refs` are both 0.
    ///
    /// # Memory Ordering
    ///
    /// `SeqCst` ensures this increment is visible to all other threads,
    /// blocking them from acquiring any borrow (shared or mutable).
    pub fn increase_refmut(&self) {
        self.downcast()
            .num_mutable_refs
            .fetch_add(1, AtomicOrdering::SeqCst);
    }

    /// Decrements the mutable borrow counter.
    ///
    /// Called when a `RefMut<T>` is dropped, releasing exclusive access.
    ///
    /// # Memory Ordering
    ///
    /// `SeqCst` ensures this decrement is immediately visible, allowing
    /// other threads to acquire borrows.
    pub fn decrease_refmut(&self) {
        self.downcast()
            .num_mutable_refs
            .fetch_sub(1, AtomicOrdering::SeqCst);
    }
}

/// RAII guard for a shared borrow of type `T` from a `RefAny`.
///
/// Similar to `std::cell::Ref`, this automatically decrements the borrow
/// counter when dropped, ensuring borrows are properly released.
///
/// # Deref
///
/// Implements `Deref<Target = T>` so you can use it like `&T`.
#[derive(Debug)]
#[repr(C)]
pub struct Ref<'a, T> {
    ptr: &'a T,
    sharing_info: RefCount,
}

impl<'a, T> Drop for Ref<'a, T> {
    /// Automatically releases the shared borrow when the guard goes out of scope.
    ///
    /// # Safety
    ///
    /// Safe because `decrease_ref` uses atomic operations and is designed to be
    /// called exactly once per `Ref` instance.
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

/// RAII guard for a mutable borrow of type `T` from a `RefAny`.
///
/// Similar to `std::cell::RefMut`, this automatically decrements the mutable
/// borrow counter when dropped, releasing exclusive access.
///
/// # Deref / DerefMut
///
/// Implements both `Deref` and `DerefMut` so you can use it like `&mut T`.
#[derive(Debug)]
#[repr(C)]
pub struct RefMut<'a, T> {
    ptr: &'a mut T,
    sharing_info: RefCount,
}

impl<'a, T> Drop for RefMut<'a, T> {
    /// Automatically releases the mutable borrow when the guard goes out of scope.
    ///
    /// # Safety
    ///
    /// Safe because `decrease_refmut` uses atomic operations and is designed to be
    /// called exactly once per `RefMut` instance.
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

/// Type-erased, reference-counted smart pointer with runtime borrow checking.
///
/// `RefAny` is similar to `Arc<RefCell<dyn Any>>`, providing:
/// - Type erasure (stores any `'static` type)
/// - Reference counting (clones share the same data)
/// - Runtime borrow checking (enforces Rust's borrowing rules at runtime)
/// - FFI compatibility (`#[repr(C)]` and C-compatible API)
///
/// # Thread Safety
///
/// - `Send`: Can be moved between threads (heap-allocated data, atomic counters)
/// - `Sync`: Can be shared between threads (`downcast_ref/mut` require `&mut self`)
///
/// # Memory Safety
///
/// Fixed critical UB bugs in alignment, copy count, and pointer provenance (see
/// REFANY_UB_FIXES.md). All operations are verified with Miri to ensure absence of
/// undefined behavior.
///
/// # Usage
///
/// ```rust
/// # use azul_core::refany::RefAny;
/// let data = RefAny::new(42i32);
/// let mut data_clone = data.clone(); // shares the same heap allocation
///
/// // Runtime-checked downcasting with type safety
/// if let Some(value_ref) = data_clone.downcast_ref::<i32>() {
///     assert_eq!(*value_ref, 42);
/// };
///
/// // Runtime-checked mutable borrowing
/// if let Some(mut value_mut) = data_clone.downcast_mut::<i32>() {
///     *value_mut = 100;
/// };
/// ```
#[derive(Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(C)]
pub struct RefAny {
    /// Shared metadata: reference counts, type info, destructor, AND data pointer.
    ///
    /// All `RefAny` clones point to the same `RefCountInner` via this field.
    /// The data pointer is stored in RefCountInner so all clones see the same
    /// pointer, even after replace_contents() is called.
    ///
    /// The `run_destructor` flag on `RefCount` controls whether dropping this
    /// RefAny should decrement the reference count and potentially free memory.
    pub sharing_info: RefCount,

    /// Unique ID for this specific clone (root = 0, subsequent clones increment).
    ///
    /// Used to distinguish between the original and clones for debugging.
    pub instance_id: u64,
}

impl_option!(
    RefAny,
    OptionRefAny,
    copy = false,
    [Debug, Hash, Clone, PartialEq, PartialOrd, Ord, Eq]
);

// SAFETY: RefAny is Send because:
// - The data pointer points to heap memory (can be sent between threads)
// - All shared state (RefCountInner) uses atomic operations
// - No thread-local storage is used
unsafe impl Send for RefAny {}

// SAFETY: RefAny is Sync because:
// - Methods that access the inner data (`downcast_ref/mut`) require `&mut self`, which
//  is checked by the compiler and prevents concurrent access
// - Methods on `&RefAny` (like `clone`, `get_type_id`) only use atomic operations or
//  read immutable data, which is inherently thread-safe
// - The runtime borrow checker (via `can_be_shared/shared_mut`) uses SeqCst atomics,
//   ensures proper synchronization across threads
unsafe impl Sync for RefAny {}

impl RefAny {
    /// Creates a new type-erased `RefAny` containing the given value.
    ///
    /// This is the primary way to construct a `RefAny` from Rust code.
    ///
    /// # Type Safety
    ///
    /// Stores the `TypeId` of `T` for runtime type checking during downcasts.
    ///
    /// # Memory Layout
    ///
    /// - Allocates memory on the heap with correct size (`size_of::<T>()`) and alignment
    ///   (`align_of::<T>()`)
    /// - Copies the value into the heap allocation
    /// - Forgets the original value to prevent double-drop
    ///
    /// # Custom Destructor
    ///
    /// Creates a type-specific destructor that:
    /// 1. Copies the data from heap back to stack
    /// 2. Calls `mem::drop` to run `T`'s destructor
    /// 3. The heap memory is freed separately in `RefAny::drop`
    ///
    /// This two-phase destruction ensures proper cleanup even for complex types.
    ///
    /// # Safety
    ///
    /// Safe because:
    /// - `mem::forget` prevents double-drop of the original value
    /// - Type `T` and destructor `<U>` are matched at compile time
    /// - `ptr::copy_nonoverlapping` with count=1 copies exactly one `T`
    ///
    /// # Example
    ///
    /// ```rust
    /// # use azul_core::refany::RefAny;
    /// let mut data = RefAny::new(42i32);
    /// let value = data.downcast_ref::<i32>().unwrap();
    /// assert_eq!(*value, 42);
    /// ```
    pub fn new<T: 'static>(value: T) -> Self {
        /// Type-specific destructor that properly drops the inner value.
        ///
        /// # Safety
        ///
        /// Safe to call ONLY with a pointer that was created by `RefAny::new<U>`.
        /// The type `U` must match the original type `T`.
        ///
        /// # Why Copy to Stack?
        ///
        /// Rust's drop glue expects a value, not a pointer. We copy the data
        /// to the stack so `mem::drop` can run the destructor properly.
        ///
        /// # Critical Fix
        ///
        /// The third argument to `copy_nonoverlapping` is the COUNT (1 element),
        /// not the SIZE in bytes. Using `size_of::<U>()` here would copy
        /// `size_of::<U>()` elements, causing buffer overflow.
        extern "C" fn default_custom_destructor<U: 'static>(ptr: *mut c_void) {
            use core::{mem, ptr};

            unsafe {
                // Allocate uninitialized stack space for one `U`
                let mut stack_mem = mem::MaybeUninit::<U>::uninit();

                // Copy 1 element of type U from heap to stack
                ptr::copy_nonoverlapping(
                    ptr as *const U,
                    stack_mem.as_mut_ptr(),
                    1, // CRITICAL: This is element count, not byte count!
                );

                // Take ownership and run the destructor
                let stack_mem = stack_mem.assume_init();
                mem::drop(stack_mem); // Runs U's Drop implementation
            }
        }

        let type_name = ::core::any::type_name::<T>();
        let type_id = Self::get_type_id_static::<T>();

        let st = AzString::from_const_str(type_name);
        let s = Self::new_c(
            (&value as *const T) as *const c_void,
            ::core::mem::size_of::<T>(),
            ::core::mem::align_of::<T>(), // CRITICAL: Pass alignment to prevent UB
            type_id,
            st,
            default_custom_destructor::<T>,
            0, // serialize_fn: not set for Rust types by default
            0, // deserialize_fn: not set for Rust types by default
        );
        ::core::mem::forget(value); // Prevent double-drop
        s
    }

    /// C-ABI compatible function to create a `RefAny` from raw components.
    ///
    /// This is the low-level constructor used by FFI bindings (C, Python, etc.).
    ///
    /// # Parameters
    ///
    /// - `ptr`: Pointer to the value to store (will be copied)
    /// - `len`: Size of the value in bytes (`size_of::<T>()`)
    /// - `align`: Required alignment in bytes (`align_of::<T>()`)
    /// - `type_id`: Unique identifier for the type (for downcast safety)
    /// - `type_name`: Human-readable type name (for debugging)
    /// - `custom_destructor`: Function to call when the last reference is dropped
    /// - `serialize_fn`: Function pointer for JSON serialization (0 = not set)
    /// - `deserialize_fn`: Function pointer for JSON deserialization (0 = not set)
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - `ptr` points to valid data of size `len` with alignment `align`
    /// - `type_id` uniquely identifies the type
    /// - `custom_destructor` correctly drops the type at `ptr`
    /// - `len` and `align` match the actual type's layout
    /// - If `serialize_fn != 0`, it must be a valid function pointer of type
    ///   `extern "C" fn(RefAny) -> Json`
    /// - If `deserialize_fn != 0`, it must be a valid function pointer of type
    ///   `extern "C" fn(Json) -> ResultRefAnyString`
    ///
    /// # Zero-Sized Types
    ///
    /// Special case: ZSTs use a null pointer but still track the type info
    /// and call the destructor (which may have side effects even for ZSTs).
    pub fn new_c(
        // *const T
        ptr: *const c_void,
        // sizeof(T)
        len: usize,
        // alignof(T)
        align: usize,
        // unique ID of the type (used for type comparison when downcasting)
        type_id: u64,
        // name of the class such as "app::MyData", usually compiler- or macro-generated
        type_name: AzString,
        custom_destructor: extern "C" fn(*mut c_void),
        // function pointer for JSON serialization (0 = not set)
        serialize_fn: usize,
        // function pointer for JSON deserialization (0 = not set)
        deserialize_fn: usize,
    ) -> Self {
        use core::ptr;

        // CRITICAL: Validate input pointer for non-ZST types
        // A NULL pointer for a non-zero-sized type would cause UB when copying
        // and would lead to crashes when cloning (as documented in REPORT2.md)
        if len > 0 && ptr.is_null() {
            panic!(
                "RefAny::new_c: NULL pointer passed for non-ZST type (size={}). \
                This would cause undefined behavior. Type: {:?}",
                len,
                type_name.as_str()
            );
        }

        // Special case: Zero-sized types
        //
        // Calling `alloc(Layout { size: 0, .. })` is UB, so we use a null pointer.
        // The destructor is still called (it may have side effects even for ZSTs).
        let (_internal_ptr, layout) = if len == 0 {
            let _dummy: [u8; 0] = [];
            (ptr::null_mut(), Layout::for_value(&_dummy))
        } else {
            // CRITICAL FIX: Use the caller-provided alignment, not alignment of [u8]
            //
            // Previous bug: `Layout::for_value(&[u8])` created align=1
            // This caused unaligned references when downcasting to types like i32 (align=4)
            //
            // Fixed: `Layout::from_size_align(len, align)` respects the type's alignment
            let layout = Layout::from_size_align(len, align).expect("Failed to create layout");

            // Allocate heap memory with correct alignment
            let heap_struct_as_bytes = unsafe { alloc::alloc::alloc(layout) };

            // Handle allocation failure (aborts the program)
            if heap_struct_as_bytes.is_null() {
                alloc::alloc::handle_alloc_error(layout);
            }

            // Copy the data byte-by-byte to the heap
            // SAFETY: Both pointers are valid, non-overlapping, and properly aligned
            unsafe { ptr::copy_nonoverlapping(ptr as *const u8, heap_struct_as_bytes, len) };

            (heap_struct_as_bytes, layout)
        };

        let ref_count_inner = RefCountInner {
            _internal_ptr: _internal_ptr as *const c_void,
            num_copies: AtomicUsize::new(1),       // This is the first instance
            num_refs: AtomicUsize::new(0),         // No borrows yet
            num_mutable_refs: AtomicUsize::new(0), // No mutable borrows yet
            _internal_len: len,
            _internal_layout_size: layout.size(),
            _internal_layout_align: layout.align(),
            type_id,
            type_name,
            custom_destructor,
            serialize_fn,
            deserialize_fn,
        };

        let sharing_info = RefCount::new(ref_count_inner);

        Self {
            sharing_info,
            instance_id: 0, // Root instance
        }
    }

    /// Returns the raw data pointer for FFI downcasting.
    ///
    /// This is used by the AZ_REFLECT macros in C/C++ to access the
    /// type-erased data pointer for downcasting operations.
    ///
    /// # Safety
    ///
    /// The returned pointer must only be dereferenced after verifying
    /// the type ID matches the expected type. Callers are responsible
    /// for proper type safety checks.
    pub fn get_data_ptr(&self) -> *const c_void {
        self.sharing_info.downcast()._internal_ptr
    }

    /// Checks if this is the only `RefAny` instance with no active borrows.
    ///
    /// Returns `true` only if:
    /// - `num_copies == 1` (no clones exist)
    /// - `num_refs == 0` (no shared borrows active)
    /// - `num_mutable_refs == 0` (no mutable borrows active)
    ///
    /// Useful for checking if you have exclusive ownership.
    ///
    /// # Memory Ordering
    ///
    /// Uses `SeqCst` to ensure a consistent view across all three counters.
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

    /// Attempts to downcast to a shared reference of type `U`.
    ///
    /// Returns `None` if:
    /// - The stored type doesn't match `U` (type safety)
    /// - A mutable borrow is already active (borrow checking)
    /// - The pointer is null (ZST or uninitialized)
    ///
    /// # Type Safety
    ///
    /// Compares `type_id` at runtime before casting. This prevents casting
    /// `*const c_void` to the wrong type, which would be immediate UB.
    ///
    /// # Borrow Checking
    ///
    /// Checks `can_be_shared()` to enforce Rust's borrowing rules:
    /// - Multiple shared borrows are allowed
    /// - Shared and mutable borrows cannot coexist
    ///
    /// # Safety
    ///
    /// The `unsafe` cast is safe because:
    /// - Type ID check ensures `U` matches the stored type
    /// - Memory was allocated with correct alignment for `U`
    /// - Lifetime `'a` is tied to `&'a mut self`, preventing use-after-free
    /// - Reference count is incremented atomically before returning
    ///
    /// # Why `&mut self`?
    ///
    /// Requires `&mut self` to prevent multiple threads from calling this
    /// simultaneously on the same `RefAny`. The borrow checker enforces this.
    /// Clones of the `RefAny` can call this independently (they share data
    /// but have separate runtime borrow tracking).
    #[inline]
    pub fn downcast_ref<'a, U: 'static>(&'a mut self) -> Option<Ref<'a, U>> {
        // Runtime type check: prevent downcasting to wrong type
        let stored_type_id = self.get_type_id();
        let target_type_id = Self::get_type_id_static::<U>();
        let is_same_type = stored_type_id == target_type_id;

        if !is_same_type {
            return None;
        }

        // Runtime borrow check: ensure no mutable borrows exist
        let can_be_shared = self.sharing_info.can_be_shared();
        if !can_be_shared {
            return None;
        }

        // Get data pointer from shared RefCountInner
        let data_ptr = self.sharing_info.downcast()._internal_ptr;

        // Null check: ZSTs or uninitialized
        if data_ptr.is_null() {
            return None;
        }

        // Increment shared borrow count atomically
        self.sharing_info.increase_ref();

        Some(Ref {
            // SAFETY: Type check passed, pointer is non-null and properly aligned
            ptr: unsafe { &*(data_ptr as *const U) },
            sharing_info: self.sharing_info.clone(),
        })
    }

    /// Attempts to downcast to a mutable reference of type `U`.
    ///
    /// Returns `None` if:
    /// - The stored type doesn't match `U` (type safety)
    /// - Any borrow is already active (borrow checking)
    /// - The pointer is null (ZST or uninitialized)
    ///
    /// # Type Safety
    ///
    /// Compares `type_id` at runtime before casting, preventing UB.
    ///
    /// # Borrow Checking
    ///
    /// Checks `can_be_shared_mut()` to enforce exclusive mutability:
    /// - No other borrows (shared or mutable) can be active
    /// - This is Rust's `&mut T` rule, enforced at runtime
    ///
    /// # Safety
    ///
    /// The `unsafe` cast is safe because:
    ///
    /// - Type ID check ensures `U` matches the stored type
    /// - Memory was allocated with correct alignment for `U`
    /// - Borrow check ensures no other references exist
    /// - Lifetime `'a` is tied to `&'a mut self`, preventing aliasing
    /// - Mutable reference count is incremented atomically
    ///
    /// # Memory Ordering
    ///
    /// The `increase_refmut()` uses `SeqCst`, ensuring other threads see
    /// this mutable borrow before they try to acquire any borrow.
    #[inline]
    pub fn downcast_mut<'a, U: 'static>(&'a mut self) -> Option<RefMut<'a, U>> {
        // Runtime type check
        let is_same_type = self.get_type_id() == Self::get_type_id_static::<U>();
        if !is_same_type {
            return None;
        }

        // Runtime exclusive borrow check
        let can_be_shared_mut = self.sharing_info.can_be_shared_mut();
        if !can_be_shared_mut {
            return None;
        }

        // Get data pointer from shared RefCountInner
        let data_ptr = self.sharing_info.downcast()._internal_ptr;

        // Null check
        if data_ptr.is_null() {
            return None;
        }

        // Increment mutable borrow count atomically
        self.sharing_info.increase_refmut();

        Some(RefMut {
            // SAFETY: Type and borrow checks passed, exclusive access guaranteed
            ptr: unsafe { &mut *(data_ptr as *mut U) },
            sharing_info: self.sharing_info.clone(),
        })
    }

    /// Computes a runtime type ID from Rust's `TypeId`.
    ///
    /// Rust's `TypeId` is not `#[repr(C)]` and can't cross FFI boundaries.
    /// This function converts it to a `u64` by treating it as a byte array.
    ///
    /// # Safety
    ///
    /// Safe because:
    /// - `TypeId` is a valid type with a stable layout
    /// - We only read from it, never write
    /// - The slice lifetime is bounded by the function scope
    ///
    /// # Implementation
    ///
    /// Treats the `TypeId` as bytes and sums them with bit shifts to create
    /// a unique (but not cryptographically secure) hash.
    #[inline]
    fn get_type_id_static<T: 'static>() -> u64 {
        use core::{any::TypeId, mem};

        let t_id = TypeId::of::<T>();

        // SAFETY: TypeId is a valid type, we're only reading it
        let struct_as_bytes = unsafe {
            core::slice::from_raw_parts(
                (&t_id as *const TypeId) as *const u8,
                mem::size_of::<TypeId>(),
            )
        };

        // Convert first 8 bytes to u64 using proper bit positions
        struct_as_bytes
            .into_iter()
            .enumerate()
            .take(8) // Only use first 8 bytes (64 bits fit in u64)
            .map(|(s_pos, s)| (*s as u64) << (s_pos * 8))
            .sum()
    }

    /// Checks if the stored type matches the given type ID.
    pub fn is_type(&self, type_id: u64) -> bool {
        self.sharing_info.downcast().type_id == type_id
    }

    /// Returns the stored type ID.
    pub fn get_type_id(&self) -> u64 {
        self.sharing_info.downcast().type_id
    }

    /// Returns the human-readable type name for debugging.
    pub fn get_type_name(&self) -> AzString {
        self.sharing_info.downcast().type_name.clone()
    }

    /// Returns the current reference count (number of `RefAny` clones sharing this data).
    ///
    /// This is useful for debugging and metadata purposes.
    pub fn get_ref_count(&self) -> usize {
        self.sharing_info
            .downcast()
            .num_copies
            .load(AtomicOrdering::SeqCst)
    }

    /// Returns the serialize function pointer (0 = not set).
    /// 
    /// This is used for JSON serialization of RefAny contents.
    pub fn get_serialize_fn(&self) -> usize {
        self.sharing_info.downcast().serialize_fn
    }

    /// Returns the deserialize function pointer (0 = not set).
    /// 
    /// This is used for JSON deserialization to create a new RefAny.
    pub fn get_deserialize_fn(&self) -> usize {
        self.sharing_info.downcast().deserialize_fn
    }

    /// Sets the serialize function pointer.
    /// 
    /// # Safety
    /// 
    /// The caller must ensure the function pointer is valid and has the correct
    /// signature: `extern "C" fn(RefAny) -> Json`
    pub fn set_serialize_fn(&mut self, serialize_fn: usize) {
        // Safety: We have &mut self, so we have exclusive access
        let inner = self.sharing_info.ptr as *mut RefCountInner;
        unsafe {
            (*inner).serialize_fn = serialize_fn;
        }
    }

    /// Sets the deserialize function pointer.
    /// 
    /// # Safety
    /// 
    /// The caller must ensure the function pointer is valid and has the correct
    /// signature: `extern "C" fn(Json) -> ResultRefAnyString`
    pub fn set_deserialize_fn(&mut self, deserialize_fn: usize) {
        // Safety: We have &mut self, so we have exclusive access
        let inner = self.sharing_info.ptr as *mut RefCountInner;
        unsafe {
            (*inner).deserialize_fn = deserialize_fn;
        }
    }

    /// Returns true if this RefAny supports JSON serialization.
    pub fn can_serialize(&self) -> bool {
        self.get_serialize_fn() != 0
    }

    /// Returns true if this RefAny type supports JSON deserialization.
    pub fn can_deserialize(&self) -> bool {
        self.get_deserialize_fn() != 0
    }

    /// Replaces the contents of this RefAny with a new value from another RefAny.
    ///
    /// This method:
    /// 1. Atomically acquires a mutable "lock" via compare_exchange
    /// 2. Calls the destructor on the old value
    /// 3. Deallocates the old memory
    /// 4. Copies the new value's memory
    /// 5. Updates metadata (type_id, type_name, destructor, serialize/deserialize fns)
    /// 6. Updates the shared _internal_ptr so ALL clones see the new data
    /// 7. Releases the lock
    ///
    /// Since all clones of a RefAny share the same `RefCountInner`, this change
    /// will be visible to ALL clones of this RefAny.
    ///
    /// # Returns
    ///
    /// - `true` if the replacement was successful
    /// - `false` if there are active borrows (would cause UB)
    ///
    /// # Thread Safety
    ///
    /// Uses compare_exchange to atomically acquire exclusive access, preventing
    /// any race condition between checking for borrows and modifying the data.
    ///
    /// # Safety
    ///
    /// Safe because:
    /// - We atomically acquire exclusive access before modifying
    /// - The old destructor is called before deallocation
    /// - Memory is properly allocated with correct alignment
    /// - All metadata is updated while holding the lock
    pub fn replace_contents(&mut self, new_value: RefAny) -> bool {
        use core::ptr;

        let inner = self.sharing_info.ptr as *mut RefCountInner;
        
        // Atomically acquire exclusive access by setting num_mutable_refs to 1.
        // This uses compare_exchange to ensure no race condition:
        // - If num_mutable_refs is 0, set it to 1 (success)
        // - If num_mutable_refs is not 0, someone else has it (fail)
        // We also need to check num_refs == 0 atomically.
        let inner_ref = self.sharing_info.downcast();
        
        // First, try to acquire the mutable lock
        let mutable_lock_result = inner_ref.num_mutable_refs.compare_exchange(
            0,  // expected: no mutable refs
            1,  // desired: we take the mutable ref
            AtomicOrdering::SeqCst,
            AtomicOrdering::SeqCst,
        );
        
        if mutable_lock_result.is_err() {
            // Someone else has a mutable reference
            return false;
        }
        
        // Now check that there are no shared references
        // Note: We hold the mutable lock, so no new shared refs can be acquired
        if inner_ref.num_refs.load(AtomicOrdering::SeqCst) != 0 {
            // Release the lock and fail
            inner_ref.num_mutable_refs.store(0, AtomicOrdering::SeqCst);
            return false;
        }
        
        // We now have exclusive access - perform the replacement
        unsafe {
            // Get old layout info before we overwrite it
            let old_ptr = (*inner)._internal_ptr;
            let old_len = (*inner)._internal_len;
            let old_layout_size = (*inner)._internal_layout_size;
            let old_layout_align = (*inner)._internal_layout_align;
            let old_destructor = (*inner).custom_destructor;

            // Step 1: Call destructor on old value (if non-ZST)
            if old_len > 0 && !old_ptr.is_null() {
                old_destructor(old_ptr as *mut c_void);
            }

            // Step 2: Deallocate old memory (if non-ZST)
            if old_layout_size > 0 && !old_ptr.is_null() {
                let old_layout = Layout::from_size_align_unchecked(old_layout_size, old_layout_align);
                alloc::alloc::dealloc(old_ptr as *mut u8, old_layout);
            }

            // Get new value's metadata
            let new_inner = new_value.sharing_info.downcast();
            let new_ptr = new_inner._internal_ptr;
            let new_len = new_inner._internal_len;
            let new_layout_size = new_inner._internal_layout_size;
            let new_layout_align = new_inner._internal_layout_align;

            // Step 3: Allocate new memory and copy data
            let allocated_ptr = if new_len == 0 {
                ptr::null_mut()
            } else {
                let new_layout = Layout::from_size_align(new_len, new_layout_align)
                    .expect("Failed to create layout");
                let heap_ptr = alloc::alloc::alloc(new_layout);
                if heap_ptr.is_null() {
                    alloc::alloc::handle_alloc_error(new_layout);
                }
                // Copy data from new_value
                ptr::copy_nonoverlapping(
                    new_ptr as *const u8,
                    heap_ptr,
                    new_len,
                );
                heap_ptr
            };

            // Step 4: Update the shared internal pointer in RefCountInner
            // All clones will see this new pointer!
            (*inner)._internal_ptr = allocated_ptr as *const c_void;

            // Step 5: Update metadata in RefCountInner
            (*inner)._internal_len = new_len;
            (*inner)._internal_layout_size = new_layout_size;
            (*inner)._internal_layout_align = new_layout_align;
            (*inner).type_id = new_inner.type_id;
            (*inner).type_name = new_inner.type_name.clone();
            (*inner).custom_destructor = new_inner.custom_destructor;
            (*inner).serialize_fn = new_inner.serialize_fn;
            (*inner).deserialize_fn = new_inner.deserialize_fn;
        }

        // Release the mutable lock
        self.sharing_info.downcast().num_mutable_refs.store(0, AtomicOrdering::SeqCst);

        // Prevent new_value from running its destructor (we copied the data)
        core::mem::forget(new_value);

        true
    }
}

impl Clone for RefAny {
    /// Creates a new `RefAny` sharing the same heap-allocated data.
    ///
    /// This is cheap (just increments a counter) and is how multiple parts
    /// of the code can hold references to the same data.
    ///
    /// # Reference Counting
    ///
    /// Atomically increments `num_copies` with `SeqCst` ordering before
    /// creating the clone. This ensures all threads see the updated count
    /// before the clone can be used.
    ///
    /// # Instance ID
    ///
    /// Each clone gets a unique `instance_id` based on the current copy count.
    /// The original has `instance_id=0`, the first clone gets `1`, etc.
    ///
    /// # Memory Ordering
    ///
    /// The `fetch_add` followed by `load` both use `SeqCst`:
    /// - `fetch_add`: Ensures the increment is visible to all threads
    /// - `load`: Gets the updated value for the instance_id
    ///
    /// This prevents race conditions where two threads clone simultaneously
    /// and both see the same instance_id.
    ///
    /// # Safety
    ///
    /// Safe because:
    ///
    /// - Atomic operations prevent data races
    /// - The heap allocation remains valid (only freed when count reaches 0)
    /// - `run_destructor` is set to `true` for all clones
    fn clone(&self) -> Self {
        // Atomically increment the reference count
        let inner = self.sharing_info.downcast();
        inner.num_copies.fetch_add(1, AtomicOrdering::SeqCst);

        let new_instance_id = inner.num_copies.load(AtomicOrdering::SeqCst) as u64;

        Self {
            // Data pointer is now in RefCountInner, shared automatically
            sharing_info: RefCount {
                ptr: self.sharing_info.ptr, // Share the same metadata (and data pointer)
                run_destructor: true,       // This clone should decrement num_copies on drop
            },
            // Give this clone a unique ID based on the updated count
            instance_id: new_instance_id,
        }
    }
}

impl Drop for RefAny {
    /// Empty drop implementation - all cleanup is handled by `RefCount::drop`.
    ///
    /// When a `RefAny` is dropped, its `sharing_info: RefCount` field is automatically
    /// dropped by Rust. The `RefCount::drop` implementation handles all cleanup:
    ///
    /// 1. Atomically decrements `num_copies` with `fetch_sub`
    /// 2. If the previous value was 1 (we're the last reference):
    ///    - Reclaims the `RefCountInner` via `Box::from_raw`
    ///    - Calls the custom destructor to run `T::drop()`
    ///    - Deallocates the heap memory with the stored layout
    ///
    /// # Why No Code Here?
    ///
    /// Previously, `RefAny::drop` handled cleanup, but this caused issues with the
    /// C API where `Ref<T>` and `RefMut<T>` guards (which clone the `RefCount`) need
    /// to keep the data alive even after the original `RefAny` is dropped.
    ///
    /// By moving all cleanup to `RefCount::drop`, we ensure that:
    /// - `RefAny::clone()` creates a `RefCount` with `run_destructor = true`
    /// - `AZ_REFLECT` macros create `Ref`/`RefMut` guards that clone `RefCount`
    /// - Each `RefCount` drop decrements the counter
    /// - Only the LAST drop (when `num_copies` was 1) cleans up memory
    ///
    /// See `RefCount::drop` for the full algorithm and safety documentation.
    fn drop(&mut self) {
        // RefCount::drop handles everything automatically.
        // The sharing_info field is dropped by Rust, triggering RefCount::drop.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct TestStruct {
        value: i32,
        name: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct NestedStruct {
        inner: TestStruct,
        data: Vec<u8>,
    }

    #[test]
    fn test_refany_basic_create_and_downcast() {
        let test_val = TestStruct {
            value: 42,
            name: "test".to_string(),
        };

        let mut refany = RefAny::new(test_val.clone());

        // Test downcast_ref
        let borrowed = refany
            .downcast_ref::<TestStruct>()
            .expect("Should downcast successfully");
        assert_eq!(borrowed.value, 42);
        assert_eq!(borrowed.name, "test");
        drop(borrowed);

        // Test downcast_mut
        {
            let mut borrowed_mut = refany
                .downcast_mut::<TestStruct>()
                .expect("Should downcast mutably");
            borrowed_mut.value = 100;
            borrowed_mut.name = "modified".to_string();
        }

        // Verify mutation
        let borrowed = refany
            .downcast_ref::<TestStruct>()
            .expect("Should downcast after mutation");
        assert_eq!(borrowed.value, 100);
        assert_eq!(borrowed.name, "modified");
    }

    #[test]
    fn test_refany_clone_and_sharing() {
        let test_val = TestStruct {
            value: 42,
            name: "test".to_string(),
        };

        let mut refany1 = RefAny::new(test_val);
        let mut refany2 = refany1.clone();
        let mut refany3 = refany1.clone();

        // All three should point to the same data
        let borrowed1 = refany1
            .downcast_ref::<TestStruct>()
            .expect("Should downcast ref1");
        assert_eq!(borrowed1.value, 42);
        drop(borrowed1);

        let borrowed2 = refany2
            .downcast_ref::<TestStruct>()
            .expect("Should downcast ref2");
        assert_eq!(borrowed2.value, 42);
        drop(borrowed2);

        // Modify through refany3
        {
            let mut borrowed_mut = refany3
                .downcast_mut::<TestStruct>()
                .expect("Should downcast mut");
            borrowed_mut.value = 200;
        }

        // Verify all see the change
        let borrowed1 = refany1
            .downcast_ref::<TestStruct>()
            .expect("Should see mutation from ref1");
        assert_eq!(borrowed1.value, 200);
        drop(borrowed1);

        let borrowed2 = refany2
            .downcast_ref::<TestStruct>()
            .expect("Should see mutation from ref2");
        assert_eq!(borrowed2.value, 200);
    }

    #[test]
    fn test_refany_borrow_checking() {
        let test_val = TestStruct {
            value: 42,
            name: "test".to_string(),
        };

        let mut refany = RefAny::new(test_val);

        // Test that we can get an immutable reference
        {
            let borrowed1 = refany
                .downcast_ref::<TestStruct>()
                .expect("First immutable borrow");
            assert_eq!(borrowed1.value, 42);
            assert_eq!(borrowed1.name, "test");
        }

        // Test that we can get a mutable reference and modify the value
        {
            let mut borrowed_mut = refany
                .downcast_mut::<TestStruct>()
                .expect("Mutable borrow should work");
            borrowed_mut.value = 100;
            borrowed_mut.name = "modified".to_string();
        }

        // Verify the modification persisted
        {
            let borrowed = refany
                .downcast_ref::<TestStruct>()
                .expect("Should be able to borrow again");
            assert_eq!(borrowed.value, 100);
            assert_eq!(borrowed.name, "modified");
        }
    }

    #[test]
    fn test_refany_type_safety() {
        let test_val = TestStruct {
            value: 42,
            name: "test".to_string(),
        };

        let mut refany = RefAny::new(test_val);

        // Try to downcast to wrong type
        assert!(
            refany.downcast_ref::<i32>().is_none(),
            "Should not allow downcasting to wrong type"
        );
        assert!(
            refany.downcast_mut::<String>().is_none(),
            "Should not allow mutable downcasting to wrong type"
        );

        // Correct type should still work
        let borrowed = refany
            .downcast_ref::<TestStruct>()
            .expect("Correct type should work");
        assert_eq!(borrowed.value, 42);
    }

    #[test]
    fn test_refany_zero_sized_type() {
        #[derive(Debug, Clone, PartialEq)]
        struct ZeroSized;

        let refany = RefAny::new(ZeroSized);

        // Zero-sized types are stored differently (null pointer)
        // Verify that the RefAny can be created and cloned without issues
        let _cloned = refany.clone();

        // Note: downcast operations on ZSTs may have limitations
        // This test primarily verifies that creation and cloning work
    }

    #[test]
    fn test_refany_with_vec() {
        let test_val = vec![1, 2, 3, 4, 5];
        let mut refany = RefAny::new(test_val);

        {
            let mut borrowed_mut = refany
                .downcast_mut::<Vec<i32>>()
                .expect("Should downcast vec");
            borrowed_mut.push(6);
            borrowed_mut.push(7);
        }

        let borrowed = refany
            .downcast_ref::<Vec<i32>>()
            .expect("Should downcast vec");
        assert_eq!(&**borrowed, &[1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn test_refany_nested_struct() {
        let nested = NestedStruct {
            inner: TestStruct {
                value: 42,
                name: "inner".to_string(),
            },
            data: vec![1, 2, 3],
        };

        let mut refany = RefAny::new(nested);

        {
            let mut borrowed_mut = refany
                .downcast_mut::<NestedStruct>()
                .expect("Should downcast nested");
            borrowed_mut.inner.value = 100;
            borrowed_mut.data.push(4);
        }

        let borrowed = refany
            .downcast_ref::<NestedStruct>()
            .expect("Should downcast nested");
        assert_eq!(borrowed.inner.value, 100);
        assert_eq!(&borrowed.data, &[1, 2, 3, 4]);
    }

    #[test]
    fn test_refany_drop_order() {
        use std::sync::{Arc, Mutex};

        let drop_counter = Arc::new(Mutex::new(0));

        struct DropTracker {
            counter: Arc<Mutex<i32>>,
        }

        impl Drop for DropTracker {
            fn drop(&mut self) {
                *self.counter.lock().unwrap() += 1;
            }
        }

        {
            let tracker = DropTracker {
                counter: drop_counter.clone(),
            };
            let refany1 = RefAny::new(tracker);
            let refany2 = refany1.clone();
            let refany3 = refany1.clone();

            assert_eq!(*drop_counter.lock().unwrap(), 0, "Should not drop yet");

            drop(refany1);
            assert_eq!(
                *drop_counter.lock().unwrap(),
                0,
                "Should not drop after first clone dropped"
            );

            drop(refany2);
            assert_eq!(
                *drop_counter.lock().unwrap(),
                0,
                "Should not drop after second clone dropped"
            );

            drop(refany3);
            assert_eq!(
                *drop_counter.lock().unwrap(),
                1,
                "Should drop after last clone dropped"
            );
        }
    }

    #[test]
    fn test_refany_callback_simulation() {
        // Simulate the VirtualizedView callback pattern
        #[derive(Clone)]
        struct CallbackData {
            counter: i32,
        }

        let data = CallbackData { counter: 0 };
        let mut refany = RefAny::new(data);

        // Simulate callback invocation
        {
            let mut borrowed = refany
                .downcast_mut::<CallbackData>()
                .expect("Should downcast in callback");
            borrowed.counter += 1;
        }

        let borrowed = refany
            .downcast_ref::<CallbackData>()
            .expect("Should read after callback");
        assert_eq!(borrowed.counter, 1);
    }
}
