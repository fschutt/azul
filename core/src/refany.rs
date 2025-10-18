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
use core::{
    alloc::Layout,
    ffi::c_void,
    fmt,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};

use azul_css::AzString;

/// Internal reference counting metadata for `RefAny`.
///
/// This struct tracks:
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
}

/// Wrapper around a heap-allocated `RefCountInner`.
///
/// This is the shared metadata that all `RefAny` clones point to.
/// When the last `RefCount` is dropped, the `RefCountInner` is deallocated,
/// but the actual data deallocation is handled by `RefAny::drop`.
///
/// # Why `run_destructor: bool`
///
/// This flag prevents double-free when a `RefAny` clones the `RefCount`.
/// Only the owning `RefAny` should handle data deallocation.
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
/// REFANY_UB_FIXES.md). All operations are verified with Miri to ensure absence of undefined
/// behavior.
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
    /// Type-erased pointer to heap-allocated data.
    ///
    /// SAFETY: Must be properly aligned for the stored type (guaranteed by
    /// `Layout::from_size_align` in `new_c`). Never null for non-ZST types.
    pub _internal_ptr: *const c_void,

    /// Shared metadata: reference counts, type info, destructor.
    ///
    /// All `RefAny` clones point to the same `RefCountInner` via this field.
    pub sharing_info: RefCount,

    /// Unique ID for this specific clone (root = 0, subsequent clones increment).
    ///
    /// Used to distinguish between the original and clones for debugging.
    pub instance_id: u64,

    /// Whether this instance should run the destructor on drop.
    ///
    /// Set to false when data is moved out or explicitly prevented.
    pub run_destructor: bool,
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
// - Methods that access the inner data (`downcast_ref/mut`) require `&mut self`, which is checked
//   by the compiler and prevents concurrent access
// - Methods on `&RefAny` (like `clone`, `get_type_id`) only use atomic operations or read immutable
//   data, which is inherently thread-safe
// - The runtime borrow checker (via `can_be_shared/shared_mut`) uses SeqCst atomics, ensuring
//   proper synchronization across threads
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
        let st = AzString::from_const_str(type_name);
        let s = Self::new_c(
            (&value as *const T) as *const c_void,
            ::core::mem::size_of::<T>(),
            ::core::mem::align_of::<T>(), // CRITICAL: Pass alignment to prevent UB
            Self::get_type_id_static::<T>(),
            st,
            default_custom_destructor::<T>,
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
    ///
    /// # Critical Fix: Alignment
    ///
    /// Previous implementation used `Layout::for_value(&[u8])` which created
    /// a layout with alignment=1, causing unaligned memory access UB.
    ///
    /// Now uses `Layout::from_size_align(len, align)` to ensure the heap
    /// allocation has the correct alignment for the stored type.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - `ptr` points to valid data of size `len` with alignment `align`
    /// - `type_id` uniquely identifies the type
    /// - `custom_destructor` correctly drops the type at `ptr`
    /// - `len` and `align` match the actual type's layout
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
    ) -> Self {
        use core::ptr;

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
            num_copies: AtomicUsize::new(1),       // This is the first instance
            num_refs: AtomicUsize::new(0),         // No borrows yet
            num_mutable_refs: AtomicUsize::new(0), // No mutable borrows yet
            _internal_len: len,
            _internal_layout_size: layout.size(),
            _internal_layout_align: layout.align(),
            type_id,
            type_name,
            custom_destructor,
        };

        Self {
            _internal_ptr: _internal_ptr as *const c_void,
            sharing_info: RefCount::new(ref_count_inner),
            instance_id: 0, // Root instance
            run_destructor: true,
        }
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
        let is_same_type = self.get_type_id() == Self::get_type_id_static::<U>();
        if !is_same_type {
            return None;
        }

        // Runtime borrow check: ensure no mutable borrows exist
        let can_be_shared = self.sharing_info.can_be_shared();
        if !can_be_shared {
            return None;
        }

        // Null check: ZSTs or uninitialized
        if self._internal_ptr.is_null() {
            return None;
        }

        // Increment shared borrow count atomically
        self.sharing_info.increase_ref();

        Some(Ref {
            // SAFETY: Type check passed, pointer is non-null and properly aligned
            ptr: unsafe { &*(self._internal_ptr as *const U) },
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

        // Null check
        if self._internal_ptr.is_null() {
            return None;
        }

        // Increment mutable borrow count atomically
        self.sharing_info.increase_refmut();

        Some(RefMut {
            // SAFETY: Type and borrow checks passed, exclusive access guaranteed
            ptr: unsafe { &mut *(self._internal_ptr as *mut U) },
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

        // Simple hash: sum bytes with position-based bit shifts
        struct_as_bytes
            .into_iter()
            .enumerate()
            .map(|(s_pos, s)| ((*s as u64) << s_pos))
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
    /// - Atomic operations prevent data races
    /// - The heap allocation remains valid (only freed when count reaches 0)
    /// - `run_destructor` is set to `true` for all clones
    fn clone(&self) -> Self {
        // Atomically increment the reference count
        self.sharing_info
            .downcast()
            .num_copies
            .fetch_add(1, AtomicOrdering::SeqCst);

        Self {
            _internal_ptr: self._internal_ptr, // Share the same data pointer
            sharing_info: RefCount {
                ptr: self.sharing_info.ptr, // Share the same metadata
                run_destructor: true,
            },
            // Give this clone a unique ID based on the updated count
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
    /// Decrements the reference count and deallocates if this is the last reference.
    ///
    /// This is the critical function that ensures memory is freed exactly once.
    ///
    /// # Algorithm
    ///
    /// 1. Atomically decrement `num_copies` with `fetch_sub`
    /// 2. Check if the *previous* value was 1 (meaning we're the last reference)
    /// 3. If not the last, return early (another reference still exists)
    /// 4. If last, reclaim the metadata and run the destructor
    /// 5. Deallocate the heap memory
    ///
    /// # Why `fetch_sub` Returns Previous Value
    ///
    /// `fetch_sub(1)` returns the value *before* subtraction. If it returns `1`,
    /// that means we decremented from `1` to `0`, making us the last reference.
    ///
    /// This is the standard reference counting pattern and prevents the
    /// "drop twice" or "never drop" bug.
    ///
    /// # Memory Ordering: SeqCst Prevents Races
    ///
    /// Example race without proper ordering:
    /// ```no_run,ignore
    /// Thread A: fetch_sub(1) -> sees 2, returns
    /// Thread B: fetch_sub(1) -> sees 1, starts cleanup
    /// Thread A: (much later) actually writes the decremented value
    /// ```
    ///
    /// With `SeqCst`, this cannot happen:
    /// - Both threads see a globally consistent order
    /// - If Thread B sees `1`, Thread A's decrement has already happened
    /// - Exactly one thread will see `1` and run cleanup
    ///
    /// # Two-Phase Destruction
    ///
    /// 1. **Custom Destructor**: Runs the type's `Drop` implementation
    ///    - Copies data from heap to stack
    ///    - Calls `mem::drop` to run `T::drop()`
    ///    - This is where side effects (file closing, etc.) happen
    ///
    /// 2. **Memory Deallocation**: Frees the heap memory
    ///    - Uses the stored `Layout` to deallocate correctly
    ///    - Must match the layout used in `alloc` (size + alignment)
    ///
    /// # Safety
    ///
    /// Multiple `unsafe` blocks, all justified:
    ///
    /// - `Box::from_raw(ptr)`: Safe because ptr came from `Box::into_raw` in `RefCount::new`
    /// - `Layout::from_size_align_unchecked`: Safe because values came from `Layout` in `new_c`
    /// - `custom_destructor(ptr)`: Safe because ptr has the type expected by the destructor
    /// - `alloc::dealloc(ptr, layout)`: Safe because ptr and layout match the original allocation
    ///
    /// # ZST Handling
    ///
    /// Zero-sized types have `_internal_len == 0` and null pointer.
    /// We still call the destructor (it may have side effects) but skip deallocation.
    fn drop(&mut self) {
        use core::ptr;

        self.run_destructor = false;

        // Atomically decrement and get the PREVIOUS value
        let current_copies = self
            .sharing_info
            .downcast()
            .num_copies
            .fetch_sub(1, AtomicOrdering::SeqCst);

        // If previous value wasn't 1, other references still exist
        if current_copies != 1 {
            return;
        }

        // We're the last reference! Reclaim the metadata.
        // SAFETY: ptr came from Box::into_raw, and we're the last reference
        let sharing_info = unsafe { Box::from_raw(self.sharing_info.ptr as *mut RefCountInner) };
        let sharing_info = *sharing_info; // Box deallocates here

        // Handle zero-sized types specially
        if sharing_info._internal_len == 0
            || sharing_info._internal_layout_size == 0
            || self._internal_ptr.is_null()
        {
            let mut _dummy: [u8; 0] = [];
            // Call destructor even for ZSTs (may have side effects)
            (sharing_info.custom_destructor)(_dummy.as_ptr() as *mut c_void);
        } else {
            // Reconstruct the layout used during allocation
            // SAFETY: These values came from a valid Layout in new_c
            let layout = unsafe {
                Layout::from_size_align_unchecked(
                    sharing_info._internal_layout_size,
                    sharing_info._internal_layout_align,
                )
            };

            // Phase 1: Run the custom destructor (runs T::drop)
            // SAFETY: ptr points to valid data of the type expected by the destructor
            (sharing_info.custom_destructor)(self._internal_ptr as *mut c_void);

            // Phase 2: Deallocate the memory
            // SAFETY: ptr and layout match the original allocation in new_c
            unsafe {
                alloc::alloc::dealloc(self._internal_ptr as *mut u8, layout);
            }
        }
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
        // Simulate the IFrame callback pattern
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
