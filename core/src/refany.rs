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

/// C-compatible destructor function type for `RefAny`.
/// Called when the last reference to a `RefAny` is dropped.
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
// `_internal_*` are C-ABI field names exposed in api.json; the `_` prefix is the
// intentional "internal" convention and cannot be renamed without breaking the ABI.
#[allow(clippy::pub_underscore_fields)]
pub struct RefCountInner {
    /// Type-erased pointer to heap-allocated data.
    ///
    /// SAFETY: Must be properly aligned for the stored type (guaranteed by
    /// `Layout::from_size_align` in `new_c`). Never null for non-ZST types.
    ///
    /// This pointer is shared by all `RefAny` clones, so `replace_contents`
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

    /// Human-readable type name (e.g., "`MyStruct`") for debugging.
    pub type_name: AzString,

    /// Function pointer to correctly drop the type-erased data.
    /// SAFETY: Must be called with a pointer to data of the correct type.
    pub custom_destructor: extern "C" fn(*mut c_void),

    /// Function pointer to serialize `RefAny` to JSON (0 = not set).
    /// Cast to `RefAnySerializeFnType` (defined in `azul_layout::json`) when called.
    /// Type: extern "C" fn(RefAny) -> Json
    pub serialize_fn: usize,

    /// Function pointer to deserialize JSON to new `RefAny` (0 = not set).
    /// Cast to `RefAnyDeserializeFnType` (defined in `azul_layout::json`) when called.
    /// Type: extern "C" fn(Json) -> `ResultRefAnyString`
    pub deserialize_fn: usize,

    /// Function pointer to an on-update observer (0 = not set).
    /// Cast to `extern "C" fn(*const c_void, usize)` — the (data ptr, byte len)
    /// of the *pre-mutation* data — and fired from `downcast_mut` BEFORE the
    /// mutable borrow is handed out. This is the foundation for undo/redo
    /// snapshots and client/server state sync. Set via `RefAny::set_update_fn`.
    pub update_fn: usize,
}

/// Wrapper around a heap-allocated `RefCountInner`.
///
/// This is the shared metadata that all `RefAny` clones point to.
/// The `RefCount` is responsible for all memory management:
///
/// - `RefCount::clone()` increments `num_copies` in `RefCountInner`
/// - `RefCount::drop()` decrements `num_copies` and, if it reaches 0:
///   1. Frees the `RefCountInner`
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.downcast().fmt(f)
    }
}

impl Clone for RefCount {
    /// Clones the `RefCount` and increments the reference count.
    ///
    /// # Safety
    ///
    /// This is safe because:
    /// - The ptr is valid (created from `Box::into_raw`)
    /// - `num_copies` is atomically incremented with `SeqCst` ordering
    /// - This ensures the `RefCountInner` is not freed while clones exist
    fn clone(&self) -> Self {
        // CRITICAL: Must increment num_copies so the RefCountInner is not freed
        // while this clone exists. The C macros (AZ_REFLECT) use AzRefCount_clone
        // to create Ref/RefMut guards, and those guards must keep the data alive.
        if !self.ptr.is_null() {
            // SAFETY: `ptr` is non-null (checked) and came from `Box::into_raw`
            // in `RefCount::new`; it stays alive as long as any clone exists
            // because every clone increments `num_copies` here.
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
    /// Decrements the reference count when a `RefCount` clone is dropped.
    ///
    /// If this was the last reference (`num_copies` reaches 0), this will also
    /// free the `RefCountInner` and call the custom destructor.
    #[allow(clippy::used_underscore_binding)] // `_`-prefixed fields are an intentional FFI/api.json naming convention; internal access is required
    fn drop(&mut self) {
        // Only decrement if run_destructor is true (meaning this is a clone)
        // and the pointer is valid
        if !self.run_destructor || self.ptr.is_null() {
            return;
        }
        self.run_destructor = false;

        // Take the inner pointer and NULL the field before doing anything
        // else. The C ABI reaches this drop via `AzRefCount_delete` →
        // `drop_in_place` on C-owned struct memory, and writes through
        // `&mut self` persist in that memory. Nulling the pointer here
        // (mirroring the `ptr = 0` convention the AZ_REFLECT C macros use
        // for their downcast guards) makes a SECOND delete of the same
        // struct — easy to hit in C example failure paths, and unguarded
        // in pre-0.2.1 copies of azul.h — a safe no-op via the null check
        // above, instead of a double-free of the RefCountInner allocation
        // or a read through a dangling pointer.
        let inner = self.ptr;
        self.ptr = core::ptr::null();

        // Atomically decrement and get the PREVIOUS value. `checked_sub`
        // refuses to underflow: an unmatched decrement (e.g. a C caller
        // deleting a byte-copied Ref struct twice) becomes a no-op instead
        // of wrapping `num_copies` to `usize::MAX` and corrupting the
        // reference count for the rest of the process.
        // SAFETY: `inner` is non-null (guarded above) and points to the live
        // `RefCountInner` from `Box::into_raw`; only the atomic field is touched.
        let current_copies = unsafe {
            match (*inner).num_copies.fetch_update(
                AtomicOrdering::SeqCst,
                AtomicOrdering::SeqCst,
                |n| n.checked_sub(1),
            ) {
                Ok(prev) => prev,
                Err(_zero) => return,
            }
        };

        // If previous value wasn't 1, other references still exist
        if current_copies != 1 {
            return;
        }

        // We're the last reference! Clean up.
        // SAFETY: ptr came from Box::into_raw, and we're the last reference
        let sharing_info = unsafe { Box::from_raw(inner.cast_mut()) };
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
            (sharing_info.custom_destructor)(_dummy.as_mut_ptr().cast::<c_void>());
        } else {
            // Reconstruct the layout used during allocation. Removed the
            // `unsafe { Layout::from_size_align_unchecked(..) }`: these size/align
            // were produced by a valid `Layout` in `new_c` (`layout.size()` /
            // `layout.align()`), so the safe checked constructor always succeeds
            // and is behaviorally identical here — no unsafe needed.
            let layout = Layout::from_size_align(
                sharing_info._internal_layout_size,
                sharing_info._internal_layout_align,
            )
            .expect("RefCount::drop: stored layout was invalid");

            // Phase 1: Run the custom destructor
            (sharing_info.custom_destructor)(data_ptr.cast_mut());

            // Phase 2: Deallocate the memory
            // SAFETY: `data_ptr` was allocated in `new_c` (or `replace_contents`)
            // with exactly this `layout`, and we are the last reference, so no
            // other clone can observe the freed block.
            unsafe {
                alloc::alloc::dealloc(data_ptr as *mut u8, layout);
            }
        }
    }
}

/// Debug-friendly snapshot of `RefCountInner` with non-atomic values.
#[derive(Debug, Clone)]
pub(crate) struct RefCountInnerDebug {
    pub(crate) num_copies: usize,
    pub(crate) num_refs: usize,
    pub(crate) num_mutable_refs: usize,
    pub(crate) _internal_len: usize,
    pub(crate) _internal_layout_size: usize,
    pub(crate) _internal_layout_align: usize,
    pub(crate) type_id: u64,
    pub(crate) type_name: AzString,
    pub(crate) custom_destructor: usize,
    /// Serialization function pointer (0 = not set)
    pub(crate) serialize_fn: usize,
    /// Deserialization function pointer (0 = not set)
    pub(crate) deserialize_fn: usize,
}

impl RefCount {
    /// Creates a new `RefCount` by boxing the metadata on the heap.
    ///
    /// # Safety
    ///
    /// Safe because we're creating a new allocation with `Box::new`,
    /// then immediately leaking it with `into_raw` to get a stable pointer.
    fn new(ref_count: RefCountInner) -> Self {
        Self {
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
        assert!(!self.ptr.is_null(), "[RefCount::downcast] FATAL: self.ptr is null!");
        // SAFETY: `ptr` is non-null (asserted) and came from `Box::into_raw`; the
        // returned reference is bounded by `&self`, and refcounting keeps the
        // `RefCountInner` alive for at least that long.
        unsafe { &*self.ptr }
    }

    /// Creates a debug snapshot of the current reference counts.
    ///
    /// Loads all atomic values with `SeqCst` ordering to get a consistent view.
    #[allow(clippy::used_underscore_binding)] // `_`-prefixed fields are an intentional FFI/api.json naming convention; internal access is required
    pub(crate) fn debug_get_refcount_copied(&self) -> RefCountInnerDebug {
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
    #[must_use] pub fn can_be_shared(&self) -> bool {
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
    #[must_use] pub fn can_be_shared_mut(&self) -> bool {
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
    /// # Underflow guard
    ///
    /// Saturates at 0: an unmatched decrement — e.g. a C caller running
    /// `FooRef_delete` after a FAILED downcast with a pre-0.2.1 copy of
    /// `azul.h` (whose macro did not skip the decrease), or a plain
    /// double-delete — must not wrap `num_refs` to `usize::MAX`, which
    /// would make `can_be_shared_mut()` return `false` for the rest of
    /// the process (callbacks silently stop mutating state).
    ///
    /// # Memory Ordering
    ///
    /// `SeqCst` ensures this decrement is immediately visible to other threads
    /// waiting to acquire a mutable borrow.
    pub fn decrease_ref(&self) {
        let _ = self.downcast().num_refs.fetch_update(
            AtomicOrdering::SeqCst,
            AtomicOrdering::SeqCst,
            |n| n.checked_sub(1),
        );
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
    /// # Underflow guard
    ///
    /// Saturates at 0 (see [`Self::decrease_ref`]): a double
    /// `FooRefMut_delete` from C must not wrap `num_mutable_refs`, which
    /// would corrupt the runtime borrow checker and let a second thread
    /// or timer callback obtain an aliasing mutable borrow.
    ///
    /// # Memory Ordering
    ///
    /// `SeqCst` ensures this decrement is immediately visible, allowing
    /// other threads to acquire borrows.
    pub fn decrease_refmut(&self) {
        let _ = self.downcast().num_mutable_refs.fetch_update(
            AtomicOrdering::SeqCst,
            AtomicOrdering::SeqCst,
            |n| n.checked_sub(1),
        );
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

impl<T> Drop for Ref<'_, T> {
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

impl<T> core::ops::Deref for Ref<'_, T> {
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
/// # Deref / `DerefMut`
///
/// Implements both `Deref` and `DerefMut` so you can use it like `&mut T`.
#[derive(Debug)]
#[repr(C)]
pub struct RefMut<'a, T> {
    ptr: &'a mut T,
    sharing_info: RefCount,
}

impl<T> Drop for RefMut<'_, T> {
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

impl<T> core::ops::Deref for RefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.ptr
    }
}

impl<T> core::ops::DerefMut for RefMut<'_, T> {
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
/// Fixed critical UB bugs in alignment, copy count, and pointer provenance.
/// All operations are verified with Miri to ensure absence of undefined behavior.
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
#[derive(Debug)]
#[repr(C)]
pub struct RefAny {
    /// Shared metadata: reference counts, type info, destructor, AND data pointer.
    ///
    /// All `RefAny` clones point to the same `RefCountInner` via this field.
    /// The data pointer is stored in `RefCountInner` so all clones see the same
    /// pointer, even after `replace_contents()` is called.
    ///
    /// The `run_destructor` flag on `RefCount` controls whether dropping this
    /// `RefAny` should decrement the reference count and potentially free memory.
    pub sharing_info: RefCount,

    /// Unique ID for this specific clone (root = 0, subsequent clones increment).
    ///
    /// Used to distinguish between the original and clones for debugging.
    pub instance_id: u64,
}

// The comparison traits below are hand-written, NOT derived, and key on
// `sharing_info` ALONE. `instance_id` is deliberately omitted:
//
//     // self.instance_id == other.instance_id   <-- NEVER compare this
//
// `instance_id` is a debug-only counter that `clone()` increments (original = 0,
// first clone = 1, ...). Deriving equality folded it in, so a `RefAny` never
// equaled its own clone even though both point at the same `RefCountInner` — the
// same heap data, same refcount. Equality here means "same data", not "same
// handle"; `sharing_info` (a pointer + flag) already distinguishes unrelated
// instances.
//
// Hash/Ord must key on exactly the same fields as PartialEq or they break their
// own contracts (equal values must hash equally; `cmp() == Equal` must imply
// `==`), so all five delegate to `sharing_info`.
impl PartialEq for RefAny {
    fn eq(&self, other: &Self) -> bool {
        self.sharing_info == other.sharing_info
    }
}

impl Eq for RefAny {}

impl core::hash::Hash for RefAny {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::hash::Hash::hash(&self.sharing_info, state);
    }
}

impl PartialOrd for RefAny {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RefAny {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.sharing_info.cmp(&other.sharing_info)
    }
}

impl_option!(
    RefAny,
    OptionRefAny,
    copy = false,
    [Debug, Hash, Clone, PartialEq, PartialOrd, Ord, Eq]
);

// AUDIT: unsound-but-required. These `Send`/`Sync` impls are unconditional in
// `T`: a `!Send`/`!Sync` payload moved or shared cross-thread races its own
// internals. This is an INTENTIONAL FFI design constraint — `RefAny` is a
// type-erased C-ABI handle with no way to carry `T: Send + Sync` bounds across
// the boundary, and the framework's threading model keeps a given payload on
// one thread in practice. Left as-is per the audit; do not "fix" by adding
// bounds (it would break the erased FFI type).
//
// SAFETY: RefAny is Send because:
// - The data pointer points to heap memory (can be sent between threads)
// - All shared state (RefCountInner) uses atomic operations
// - No thread-local storage is used
#[allow(clippy::non_send_fields_in_send_ty)] // see SAFETY note above: atomic refcount, no TLS, no cross-thread deref
unsafe impl Send for RefAny {}

// SAFETY: RefAny is Sync because:
// - Methods on `&RefAny` (like `clone`, `get_type_id`) only use atomic operations or
//   read immutable data, which is inherently thread-safe
// - The runtime borrow checker (via `can_be_shared/shared_mut`) uses SeqCst atomics
//
// AUDIT: unsound-but-required (same intentional FFI constraint as `Send` above).
//
// The check-then-increment race that this note described in `downcast_ref/mut`
// is now FIXED (both use atomic `fetch_add`+validate / `compare_exchange`
// acquisition — see those methods). The remaining unsoundness is only the
// unconditional-in-`T` `Sync`, which is required by the erased C-ABI type.
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

            // The actual drop glue. `U::drop` is arbitrary user code and this
            // function is `extern "C"` (called across the FFI boundary from the
            // C ABI teardown), so a panic escaping here would unwind across that
            // boundary = UB.
            // SAFETY: this fn is only installed by `RefAny::new::<U>`, so `ptr`
            // points to an initialized, properly aligned `U` that no other code
            // still references (we are in the final drop). We move it out exactly
            // once (`count = 1`) and run its drop glue.
            let run = || unsafe {
                // A ZST has no bytes to move, and `ptr` is not a real pointer to one:
                // `RefAny::new` never allocates for a ZST, and `RefCount::drop`
                // substitutes a 1-byte-aligned dummy. Feeding that to
                // `copy_nonoverlapping` violates its "aligned and non-null"
                // precondition (`[u64; 0]` demands align 8) — UB, and Rust's debug
                // check turns it into a NON-UNWINDING abort that kills the process.
                //
                // A ZST has exactly one value, so conjure it directly and run its drop
                // glue without touching `ptr` at all.
                if size_of::<U>() == 0 {
                    // Sound for a ZST (exactly one value, touches no memory); the
                    // size_of == 0 guard is what makes assume_init well-defined here.
                    #[allow(clippy::uninit_assumed_init)]
                    drop(mem::MaybeUninit::<U>::uninit().assume_init());
                    return;
                }

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
                drop(stack_mem); // Runs U's Drop implementation
            };

            // AUDIT: contain any panic from `U::drop` so it can't unwind across
            // the `extern "C"` boundary. `catch_unwind` needs `std`; `no_std`
            // builds use `panic = "abort"`, where unwinding cannot occur.
            #[cfg(feature = "std")]
            {
                drop(std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)));
            }
            #[cfg(not(feature = "std"))]
            {
                run();
            }
        }

        let type_name = ::core::any::type_name::<T>();
        let type_id = Self::get_type_id_static::<T>();

        let st = AzString::from_const_str(type_name);
        let s = Self::new_c(
            (&raw const value) as *const c_void,
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
    ///
    /// # Panics
    ///
    /// Panics if `ptr` is null while `len > 0` (a non-empty value must have a
    /// valid backing pointer).
    #[allow(clippy::used_underscore_binding)] // `_`-prefixed fields are an intentional FFI/api.json naming convention; internal access is required
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
        assert!(!(len > 0 && ptr.is_null()), 
                "RefAny::new_c: NULL pointer passed for non-ZST type (size={}). \
                This would cause undefined behavior. Type: {:?}",
                len,
                type_name.as_str()
            );

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
            // SAFETY: `layout` has non-zero size (this branch is `len != 0`), the
            // required precondition for `alloc`; null return is handled below.
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
            update_fn: 0, // on-update observer not set by default; see set_update_fn
        };

        let sharing_info = RefCount::new(ref_count_inner);

        Self {
            sharing_info,
            instance_id: 0, // Root instance
        }
    }

    /// Returns the raw data pointer for FFI downcasting.
    ///
    /// This is used by the `AZ_REFLECT` macros in C/C++ to access the
    /// type-erased data pointer for downcasting operations.
    ///
    /// # Safety
    ///
    /// The returned pointer must only be dereferenced after verifying
    /// the type ID matches the expected type. Callers are responsible
    /// for proper type safety checks.
    #[allow(clippy::used_underscore_binding)] // `_`-prefixed fields are an intentional FFI/api.json naming convention; internal access is required
    #[must_use] pub fn get_data_ptr(&self) -> *const c_void {
        self.sharing_info.downcast()._internal_ptr
    }

    /// Returns the byte length of the type-erased payload behind
    /// [`Self::get_data_ptr`] (`size_of::<T>()` of the stored type;
    /// `0` for ZSTs).
    #[allow(clippy::used_underscore_binding)] // `_`-prefixed fields are an intentional FFI/api.json naming convention; internal access is required
    #[must_use] pub fn get_data_len(&self) -> usize {
        self.sharing_info.downcast()._internal_len
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
    pub(crate) fn has_no_copies(&self) -> bool {
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
    #[allow(clippy::used_underscore_binding)] // `_`-prefixed fields are an intentional FFI/api.json naming convention; internal access is required
    #[inline]
    pub fn downcast_ref<U: 'static>(&mut self) -> Option<Ref<'_, U>> {
        // Runtime type check: prevent downcasting to wrong type
        let stored_type_id = self.get_type_id();
        let target_type_id = Self::get_type_id_static::<U>();
        let is_same_type = stored_type_id == target_type_id;

        if !is_same_type {
            return None;
        }

        // AUDIT: ATOMIC shared-borrow acquisition.
        //
        // `RefAny` is `Sync` and clones share one `RefCountInner`, so the old
        // check-then-increment (`can_be_shared()` then `increase_ref()`) raced a
        // concurrent `downcast_mut` on another clone: both could pass their
        // pre-checks and hand out aliasing `&`/`&mut` to the same memory (UB).
        //
        // Fix (mirrors the `compare_exchange` discipline in `replace_contents`):
        // increment `num_refs` FIRST, then validate that no mutable borrow is
        // live. `SeqCst` imposes a single total order, so a writer (which CASes
        // `num_mutable_refs` 0->1 then reads `num_refs`) and this reader (which
        // adds to `num_refs` then reads `num_mutable_refs`) can never both
        // succeed — at least one observes the other's write. Back the increment
        // out on any failure path.
        self.sharing_info.increase_ref();

        if !self.sharing_info.can_be_shared() {
            // A mutable borrow is (being) acquired — release and fail.
            self.sharing_info.decrease_ref();
            return None;
        }

        // Get data pointer from shared RefCountInner (stable while we hold the
        // shared borrow: `replace_contents` needs `num_refs == 0` to proceed).
        let data_ptr = self.sharing_info.downcast()._internal_ptr;

        // Null check: ZSTs or uninitialized
        if data_ptr.is_null() {
            self.sharing_info.decrease_ref();
            return None;
        }

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
    #[allow(clippy::used_underscore_binding)] // `_`-prefixed fields are an intentional FFI/api.json naming convention; internal access is required
    #[inline]
    pub fn downcast_mut<U: 'static>(&mut self) -> Option<RefMut<'_, U>> {
        // Runtime type check
        let is_same_type = self.get_type_id() == Self::get_type_id_static::<U>();
        if !is_same_type {
            return None;
        }

        // AUDIT: ATOMIC exclusive-borrow acquisition (mirror `replace_contents`).
        //
        // The old check-then-increment (`can_be_shared_mut()` then
        // `increase_refmut()`) raced concurrent borrows on sibling clones and
        // could hand out an aliasing `&mut` (UB). Instead, `compare_exchange`
        // `num_mutable_refs` 0->1 to atomically take the exclusive slot, THEN
        // verify no shared borrow is live; release + fail otherwise. The CAS
        // both acquires and rejects a second mutable borrow in one step.
        let inner = self.sharing_info.downcast();
        if inner
            .num_mutable_refs
            .compare_exchange(0, 1, AtomicOrdering::SeqCst, AtomicOrdering::SeqCst)
            .is_err()
        {
            return None;
        }
        if inner.num_refs.load(AtomicOrdering::SeqCst) != 0 {
            // A shared borrow is live — release the exclusive slot and fail.
            inner.num_mutable_refs.store(0, AtomicOrdering::SeqCst);
            return None;
        }

        // Get data pointer from shared RefCountInner
        let data_ptr = inner._internal_ptr;

        // Null check
        if data_ptr.is_null() {
            inner.num_mutable_refs.store(0, AtomicOrdering::SeqCst);
            return None;
        }

        // Fire the on-update observer (if registered) BEFORE handing out the
        // mutable borrow: the callback sees the pre-mutation data + its byte
        // length, enabling undo/redo snapshots and client/server state sync.
        let update_fn = inner.update_fn;
        if update_fn != 0 {
            // SAFETY: `update_fn` is non-zero (checked) and, per `set_update_fn`'s
            // contract, is a valid `extern "C" fn(*const c_void, usize)`. The
            // round-trip goes through an int-to-pointer CAST (not a direct
            // usize->fn transmute): a transmuted integer carries no provenance,
            // which is UB to call (Miri rejects it); the cast re-acquires it.
            let cb: extern "C" fn(*const c_void, usize) =
                unsafe { core::mem::transmute(update_fn as *const ()) };
            let len = inner._internal_len;
            // AUDIT: the observer is a host-provided `extern "C"` fn. A Rust
            // panic escaping it would unwind across the FFI boundary (UB), so
            // contain it. `catch_unwind` needs `std`; `no_std` builds use
            // `panic = "abort"` where no unwinding can occur.
            #[cfg(feature = "std")]
            {
                drop(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cb(data_ptr, len);
                })));
            }
            #[cfg(not(feature = "std"))]
            {
                cb(data_ptr, len);
            }
        }

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
                (&raw const t_id) as *const u8,
                size_of::<TypeId>(),
            )
        };

        // AUDIT: fold ALL bytes of the `TypeId` (16 on current toolchains),
        // not just the first 8. This u64 is the ONLY runtime type guard used by
        // `downcast_*`; dropping the high 8 bytes let two distinct types whose
        // `TypeId`s differ only in their upper half collide, permitting a
        // wrong-type downcast (UB). An FxHash-style rotate+multiply mixes every
        // byte into the result and is deterministic within a process run (which
        // is all `TypeId` itself guarantees).
        struct_as_bytes.iter().fold(0u64, |hash, &b| {
            (hash.rotate_left(5) ^ u64::from(b)).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95)
        })
    }

    /// Checks if the stored type matches the given type ID.
    #[must_use] pub fn is_type(&self, type_id: u64) -> bool {
        self.sharing_info.downcast().type_id == type_id
    }

    /// Returns the stored type ID.
    #[must_use] pub fn get_type_id(&self) -> u64 {
        self.sharing_info.downcast().type_id
    }

    /// Returns the human-readable type name for debugging.
    #[must_use] pub fn get_type_name(&self) -> AzString {
        self.sharing_info.downcast().type_name.clone()
    }

    /// Returns the current reference count (number of `RefAny` clones sharing this data).
    ///
    /// This is useful for debugging and metadata purposes.
    #[must_use] pub fn get_ref_count(&self) -> usize {
        self.sharing_info
            .downcast()
            .num_copies
            .load(AtomicOrdering::SeqCst)
    }

    /// Returns the serialize function pointer (0 = not set).
    /// 
    /// This is used for JSON serialization of `RefAny` contents.
    #[must_use] pub fn get_serialize_fn(&self) -> usize {
        self.sharing_info.downcast().serialize_fn
    }

    /// Returns the deserialize function pointer (0 = not set).
    /// 
    /// This is used for JSON deserialization to create a new `RefAny`.
    #[must_use] pub fn get_deserialize_fn(&self) -> usize {
        self.sharing_info.downcast().deserialize_fn
    }

    /// Sets the serialize function pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure the function pointer is valid and has the correct
    /// signature: `extern "C" fn(RefAny) -> Json`
    ///
    /// **Known issue:** `&mut self` is exclusive to this clone, not to the shared
    /// `RefCountInner`. Concurrent calls via different clones are a data race
    /// because `serialize_fn` is a plain `usize`, not atomic.
    pub fn set_serialize_fn(&mut self, serialize_fn: usize) {
        // FIXME: &mut self is exclusive to this clone only, not to the shared
        // RefCountInner — concurrent calls via different clones are a data race.
        let inner = self.sharing_info.ptr.cast_mut();
        // SAFETY: `inner` came from `Box::into_raw` and is live (we hold `self`).
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
    ///
    /// **Known issue:** `&mut self` is exclusive to this clone, not to the shared
    /// `RefCountInner`. Concurrent calls via different clones are a data race
    /// because `deserialize_fn` is a plain `usize`, not atomic.
    pub fn set_deserialize_fn(&mut self, deserialize_fn: usize) {
        // FIXME: &mut self is exclusive to this clone only, not to the shared
        // RefCountInner — concurrent calls via different clones are a data race.
        let inner = self.sharing_info.ptr.cast_mut();
        // SAFETY: `inner` came from `Box::into_raw` and is live (we hold `self`).
        unsafe {
            (*inner).deserialize_fn = deserialize_fn;
        }
    }

    /// Registers an on-update observer (`0` = unset). It is fired from
    /// [`Self::downcast_mut`] with the (data ptr, byte len) of the *pre-mutation*
    /// data, just before the mutable borrow is handed out — the foundation for
    /// undo/redo snapshots and client/server state sync.
    ///
    /// # Safety
    ///
    /// If `update_fn != 0` it must be a valid `extern "C" fn(*const c_void, usize)`.
    /// Same shared-`RefCountInner` caveat as [`Self::set_serialize_fn`]: `&mut self`
    /// is exclusive to this clone, not to the shared inner.
    pub fn set_update_fn(&mut self, update_fn: usize) {
        let inner = self.sharing_info.ptr.cast_mut();
        // SAFETY: `inner` came from `Box::into_raw` and is live (we hold `self`).
        unsafe {
            (*inner).update_fn = update_fn;
        }
    }

    /// Returns the registered on-update observer fn pointer (`0` = unset).
    #[must_use] pub fn get_update_fn(&self) -> usize {
        self.sharing_info.downcast().update_fn
    }

    /// Returns true if this `RefAny` supports JSON serialization.
    #[must_use] pub fn can_serialize(&self) -> bool {
        self.get_serialize_fn() != 0
    }

    /// Returns true if this `RefAny` type supports JSON deserialization.
    #[must_use] pub fn can_deserialize(&self) -> bool {
        self.get_deserialize_fn() != 0
    }

    /// Replaces the contents of this `RefAny` with a new value from another `RefAny`.
    ///
    /// This method:
    /// 1. Atomically acquires a mutable "lock" via `compare_exchange`
    /// 2. Calls the destructor on the old value
    /// 3. Deallocates the old memory
    /// 4. Copies the new value's memory
    /// 5. Updates metadata (`type_id`, `type_name`, destructor, serialize/deserialize fns)
    /// 6. Updates the shared _`internal_ptr` so ALL clones see the new data
    /// 7. Releases the lock
    ///
    /// Since all clones of a `RefAny` share the same `RefCountInner`, this change
    /// will be visible to ALL clones of this `RefAny`.
    ///
    /// # Returns
    ///
    /// - `true` if the replacement was successful
    /// - `false` if there are active borrows (would cause UB)
    ///
    /// # Thread Safety
    ///
    /// Uses `compare_exchange` to atomically acquire exclusive access, preventing
    /// any race condition between checking for borrows and modifying the data.
    ///
    /// # Safety
    ///
    /// Safe because:
    /// - We atomically acquire exclusive access before modifying
    /// - The old destructor is called before deallocation
    /// - Memory is properly allocated with correct alignment
    /// - All metadata is updated while holding the lock
    ///
    /// # Panics
    ///
    /// Panics if a memory `Layout` for the replacement value cannot be
    /// constructed (its size overflows `isize::MAX`).
    #[allow(clippy::used_underscore_binding)] // `_`-prefixed fields are an intentional FFI/api.json naming convention; internal access is required
    pub fn replace_contents(&mut self, new_value: Self) -> bool {
        use core::ptr;

        let inner = self.sharing_info.ptr.cast_mut();
        
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
        // SAFETY: we hold the exclusive lock (num_mutable_refs==1, num_refs==0),
        // so no live `Ref`/`RefMut` aliases the data; `inner` is the live
        // `RefCountInner` from `Box::into_raw`. Old data is destructed+freed with
        // its own stored layout before the pointer is overwritten, and the new
        // data is freshly allocated and byte-copied.
        unsafe {
            // Get old layout info before we overwrite it
            let old_ptr = (*inner)._internal_ptr;
            let old_len = (*inner)._internal_len;
            let old_layout_size = (*inner)._internal_layout_size;
            let old_layout_align = (*inner)._internal_layout_align;
            let old_destructor = (*inner).custom_destructor;

            // Step 1: Call destructor on old value (if non-ZST)
            if old_len > 0 && !old_ptr.is_null() {
                old_destructor(old_ptr.cast_mut());
            }

            // Step 2: Deallocate old memory (if non-ZST). Use the *checked*
            // `Layout::from_size_align` (not `_unchecked`): the stored
            // size/align came from a valid `Layout`, so it always succeeds, and
            // this shrinks the unchecked surface inside this unsafe block.
            if old_layout_size > 0 && !old_ptr.is_null() {
                let old_layout = Layout::from_size_align(old_layout_size, old_layout_align)
                    .expect("replace_contents: stored old layout was invalid");
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
            (*inner).update_fn = new_inner.update_fn;
        }

        // Release the mutable lock
        self.sharing_info.downcast().num_mutable_refs.store(0, AtomicOrdering::SeqCst);

        // AUDIT: reclaim `new_value` instead of leaking it.
        //
        // The old code `mem::forget(new_value)` to stop `RefAny::drop` from
        // running the T-destructor a SECOND time on the bytes we just copied
        // into our own allocation — but that leaked `new_value`'s entire
        // `RefCountInner` box AND its heap data block on every single call.
        //
        // Instead, neutralize `new_value`'s destructor to a no-op and let the
        // normal refcount teardown run: it frees BOTH allocations (data block +
        // inner box) when this was the last reference, without re-running the
        // real T-destructor (which now lives on OUR inner, to run exactly once
        // when `self` is finally dropped). If `new_value` still had clones, the
        // no-op keeps them from double-dropping the shared T while their own
        // last drop still reclaims the shared block — no double free, no leak.
        #[allow(clippy::items_after_statements)]
        const extern "C" fn noop_destructor(_: *mut c_void) {}
        let new_inner = new_value.sharing_info.ptr.cast_mut();
        if !new_inner.is_null() {
            // SAFETY: `new_inner` came from `Box::into_raw` in `RefCount::new`
            // and is still alive (we hold `new_value`).
            unsafe {
                (*new_inner).custom_destructor = noop_destructor;
            }
        }
        drop(new_value);

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
    /// - `load`: Gets the updated value for the `instance_id`
    ///
    /// This prevents race conditions where two threads clone simultaneously
    /// and both see the same `instance_id`.
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
        let prev = inner.num_copies.fetch_add(1, AtomicOrdering::SeqCst);

        let new_instance_id = (prev + 1) as u64;

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
#[allow(clippy::items_after_statements, clippy::redundant_clone, clippy::cast_possible_truncation, clippy::cast_sign_loss, trivial_casts, clippy::borrow_as_ptr, clippy::cast_ptr_alignment, clippy::unused_self, unused_qualifications, unreachable_pub, private_interfaces)] // pedantic lints are noise in unsafe-exercising test code
mod audit_tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    // The tests below share the single `DROP_COUNT` static: each resets it to 0
    // and then asserts an exact drop count. Under the default multi-threaded
    // test runner they would otherwise interleave and corrupt each other's
    // counts (a real, if test-only, isolation bug). Every `DROP_COUNT`-using
    // test takes this lock first to serialize; it is poison-tolerant so one
    // failing test does not cascade `.unwrap()` panics into the rest.
    static DROP_COUNT_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn serialize_drop_count() -> std::sync::MutexGuard<'static, ()> {
        DROP_COUNT_SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    struct DropCounter(#[allow(dead_code)] u32);
    impl Drop for DropCounter {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    // AUDIT: exclusive borrow must be denied while a shared borrow is live and
    // vice-versa (runtime borrow checker), and must be recoverable after the
    // guard drops. Exercises the atomic acquire/release added to downcast_*.
    #[test]
    fn borrow_exclusion_and_recovery() {
        // The runtime borrow guard lives in the *shared* refcount inner, so it
        // is only observable across two clones (a single `RefAny` can't hold two
        // guards at once — the methods take `&mut self`). `b` shares `a`'s inner.
        let mut a = RefAny::new(7i32);
        let mut b = a.clone();

        {
            let r = a.downcast_ref::<i32>().unwrap();
            assert_eq!(*r, 7);
            // shared borrow live -> no mutable borrow via the shared inner
            assert!(b.downcast_mut::<i32>().is_none());
            // another shared borrow is fine
            assert!(b.downcast_ref::<i32>().is_some());
        }

        {
            let mut m = a.downcast_mut::<i32>().unwrap();
            *m = 42;
            // mutable borrow live -> no shared borrow via the shared inner
            assert!(b.downcast_ref::<i32>().is_none());
        }

        assert_eq!(*a.downcast_ref::<i32>().unwrap(), 42);
    }

    // AUDIT: wrong-type downcast must be rejected. Same type -> same id.
    #[test]
    fn type_id_guard() {
        let mut a = RefAny::new(1u64);
        assert!(a.downcast_ref::<i32>().is_none());
        assert!(a.downcast_ref::<u64>().is_some());

        assert_eq!(
            RefAny::get_type_id_static::<u64>(),
            RefAny::get_type_id_static::<u64>()
        );
        assert_ne!(
            RefAny::get_type_id_static::<u64>(),
            RefAny::get_type_id_static::<i64>()
        );
    }

    // AUDIT: replace_contents must run each stored value's destructor exactly
    // once (old value on replace, new value on final drop) and must not leak.
    #[test]
    fn replace_contents_drops_exactly_once() {
        let _serial = serialize_drop_count();
        DROP_COUNT.store(0, Ordering::SeqCst);
        {
            let mut a = RefAny::new(DropCounter(1));
            let b = RefAny::new(DropCounter(2));
            assert!(a.replace_contents(b));
            // The original `a` value was dropped during replacement.
            assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);
            // `a` now holds the (copied) `b` value; dropped at end of scope.
        }
        // Two DropCounter values were constructed; both must be dropped once.
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 2);
    }

    // AUDIT: replace_contents must fail (return false) while a borrow is live.
    #[test]
    fn replace_contents_denied_while_borrowed() {
        let mut a = RefAny::new(1i32);
        // Clone first: `r` will exclusively borrow `a`, so the sibling clone
        // must exist beforehand. Both share the same inner RefCountInner.
        let mut a2 = a.clone();
        let r = a.downcast_ref::<i32>().unwrap();
        // A live shared borrow (num_refs != 0) on the shared inner must block
        // replace_contents via the sibling clone.
        assert!(!a2.replace_contents(RefAny::new(2i32)));
        drop(r);
        assert!(a2.replace_contents(RefAny::new(2i32)));
    }

    // ---- Miri-focused unit tests -------------------------------------------
    // These exercise the pure-Rust memory behavior of each unsafe path so Miri
    // can detect UB (bad provenance, misalignment, use-after-free, leaks,
    // refcount corruption). No FFI, no threads, no OS calls; tiny allocations.

    // MIRI: covers RefAny::new + new_c alloc/copy_nonoverlapping + downcast_ref
    // (&*(ptr as *const U)) + the final Drop path (Box::from_raw + dealloc +
    // custom destructor). A non-Copy heap type checks the destructor runs.
    #[test]
    fn miri_new_downcast_drop_roundtrip() {
        let _serial = serialize_drop_count();
        DROP_COUNT.store(0, Ordering::SeqCst);
        {
            let mut a = RefAny::new(DropCounter(9));
            // downcast_ref exercises the type-id guard + aligned pointer cast.
            assert!(a.downcast_ref::<DropCounter>().is_some());
            assert!(a.downcast_ref::<u8>().is_none());
        }
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);
    }

    // MIRI: alignment correctness of new_c's Layout::from_size_align path. An
    // over-aligned payload downcast to a misaligned pointer would be UB.
    #[test]
    fn miri_alignment_preserved() {
        #[repr(align(16))]
        #[derive(Debug)]
        struct Over(u64);
        let mut a = RefAny::new(Over(0xABCD));
        let r = a.downcast_ref::<Over>().unwrap();
        assert_eq!(r.0, 0xABCD);
        assert_eq!((&raw const *r) as usize % 16, 0);
    }

    // MIRI: clone shares one RefCountInner; num_copies increments on clone and
    // decrements on drop (RefCount::clone / RefCount::drop fetch paths). Data
    // must survive while any clone lives and be freed exactly once at the end.
    #[test]
    fn miri_clone_refcount_increment_decrement() {
        let _serial = serialize_drop_count();
        DROP_COUNT.store(0, Ordering::SeqCst);
        {
            let a = RefAny::new(DropCounter(1));
            assert_eq!(a.get_ref_count(), 1);
            let b = a.clone();
            assert_eq!(a.get_ref_count(), 2);
            assert_eq!(b.get_ref_count(), 2);
            {
                let c = b.clone();
                assert_eq!(c.get_ref_count(), 3);
            }
            // c dropped -> back to 2, nothing freed yet.
            assert_eq!(a.get_ref_count(), 2);
            assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0);
        }
        // all clones dropped -> data destructed exactly once.
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);
    }

    // MIRI: downcast_mut hands out &mut *(ptr as *mut U); mutation must be
    // visible through a shared clone (shared RefCountInner data pointer).
    #[test]
    fn miri_downcast_mut_mutation_visible_across_clones() {
        let mut a = RefAny::new(10u32);
        let mut b = a.clone();
        {
            let mut m = a.downcast_mut::<u32>().unwrap();
            *m += 5;
        }
        assert_eq!(*b.downcast_ref::<u32>().unwrap(), 15);
    }

    // MIRI: the runtime borrow refcount on the shared inner. Exercises
    // increase_ref/decrease_ref/increase_refmut/decrease_refmut and the
    // can_be_shared / can_be_shared_mut predicates directly, plus the
    // checked_sub underflow guard (decrement at zero must saturate, not wrap).
    #[test]
    fn miri_borrow_counter_transitions_and_underflow_guard() {
        let a = RefAny::new(0i32);
        let rc = &a.sharing_info;

        assert!(rc.can_be_shared());
        assert!(rc.can_be_shared_mut());

        rc.increase_ref();
        assert!(rc.can_be_shared()); // shared borrows coexist
        assert!(!rc.can_be_shared_mut()); // but block a mutable borrow
        rc.decrease_ref();
        assert!(rc.can_be_shared_mut());

        rc.increase_refmut();
        assert!(!rc.can_be_shared()); // mutable borrow blocks shared
        assert!(!rc.can_be_shared_mut());
        rc.decrease_refmut();
        assert!(rc.can_be_shared_mut());

        // Underflow guard: extra decrements must saturate at 0, never wrap to
        // usize::MAX (which would permanently break the borrow checker).
        rc.decrease_ref();
        rc.decrease_refmut();
        assert!(rc.can_be_shared());
        assert!(rc.can_be_shared_mut());
    }

    // MIRI: get_type_id_static reads TypeId via from_raw_parts and folds ALL
    // bytes. Same type -> same id (stable within a run); distinct types differ.
    #[test]
    fn miri_type_id_static_stable_and_distinct() {
        assert_eq!(
            RefAny::get_type_id_static::<(u8, u64)>(),
            RefAny::get_type_id_static::<(u8, u64)>()
        );
        assert_ne!(
            RefAny::get_type_id_static::<u32>(),
            RefAny::get_type_id_static::<[u32; 2]>()
        );
    }

    // MIRI: ZST payload uses a null data pointer but must still construct,
    // clone, run its destructor once, and reject downcasts (null ptr path).
    #[test]
    fn miri_zst_roundtrip_and_destructor() {
        let _serial = serialize_drop_count();
        DROP_COUNT.store(0, Ordering::SeqCst);
        struct ZstDrop;
        impl Drop for ZstDrop {
            fn drop(&mut self) {
                DROP_COUNT.fetch_add(1, Ordering::SeqCst);
            }
        }
        {
            let mut a = RefAny::new(ZstDrop);
            assert_eq!(a.get_data_len(), 0);
            // downcast_ref bails on the null data pointer for a ZST.
            assert!(a.downcast_ref::<ZstDrop>().is_none());
            let _b = a.clone();
        }
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);
    }

    // MIRI: replace_contents alloc/dealloc/copy path plus the neutralized
    // new_value destructor. Old value destructed once, new value destructed
    // once at final drop, with no leak/double-free of either heap block.
    #[test]
    fn miri_replace_contents_alloc_paths() {
        let _serial = serialize_drop_count();
        DROP_COUNT.store(0, Ordering::SeqCst);
        {
            let mut a = RefAny::new(DropCounter(1));
            assert!(a.replace_contents(RefAny::new(DropCounter(2))));
            assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1); // old value gone
            assert_eq!(a.downcast_ref::<DropCounter>().unwrap().0, 2u32);
        }
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 2);
    }

    // MIRI: replacing across differing sizes/alignments (u8 -> u64) reallocates
    // correctly and keeps the shared pointer aligned for the new type.
    #[test]
    fn miri_replace_contents_changes_layout() {
        let mut a = RefAny::new(7u8);
        assert!(a.replace_contents(RefAny::new(0x1122_3344_5566_7788u64)));
        {
            // downcast_ref takes &mut self, so scope the guard before the next call.
            let r = a.downcast_ref::<u64>().unwrap();
            assert_eq!(*r, 0x1122_3344_5566_7788u64);
            assert_eq!((&raw const *r) as usize % core::mem::align_of::<u64>(), 0);
        }
        // old u8 type must no longer downcast.
        assert!(a.downcast_ref::<u8>().is_none());
    }

    // MIRI: RefCount clone/drop in isolation keeps the inner alive until the
    // last handle drops (Box::into_raw / Box::from_raw balance).
    #[test]
    fn miri_refcount_clone_keeps_inner_alive() {
        let a = RefAny::new(5usize);
        let rc0 = a.sharing_info.clone(); // +1 copy
        let rc1 = rc0.clone(); // +1 copy
        assert_eq!(a.get_ref_count(), 3);
        drop(rc1);
        drop(rc0);
        assert_eq!(a.get_ref_count(), 1);
        // `a` still usable -> inner not freed.
        assert_eq!(*a.clone().downcast_ref::<usize>().unwrap(), 5);
    }
}

#[cfg(test)]
#[allow(
    clippy::items_after_statements,
    clippy::redundant_clone,
    clippy::needless_pass_by_value,
    clippy::needless_range_loop,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::unusual_byte_groupings,
    clippy::many_single_char_names,
    clippy::used_underscore_binding,
    clippy::borrow_as_ptr,
    clippy::cast_ptr_alignment,
    clippy::fn_to_numeric_cast_any,
    trivial_casts,
    unused_qualifications,
    unreachable_pub,
    private_interfaces,
    missing_debug_implementations,
    missing_copy_implementations
)] // pedantic lints are noise in unsafe-exercising test code
mod autotest_generated {
    use alloc::{string::String, vec::Vec};
    use core::{
        ffi::c_void,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    /// Destructor for payloads that need no drop glue (`Copy` types built via
    /// the raw C-ABI `new_c` path).
    extern "C" fn noop_destructor(_: *mut c_void) {}

    /// Store `value` in a `RefAny` and read it back out: the byte-copy into the
    /// heap allocation and the type-checked pointer cast must be lossless.
    fn round_trip<T: 'static + Clone + PartialEq + core::fmt::Debug>(value: T) {
        let mut a = RefAny::new(value.clone());
        let r = a
            .downcast_ref::<T>()
            .expect("downcast to the stored type must succeed");
        assert_eq!(*r, value);
    }

    // ---- RefAny::new_c — raw C-ABI constructor, malformed/boundary inputs ----

    // A NULL pointer with a non-zero length is the classic FFI mistake: copying
    // from it would be UB, so `new_c` must panic instead of reading it.
    #[test]
    #[should_panic(expected = "NULL pointer passed for non-ZST type")]
    fn new_c_null_ptr_with_nonzero_len_panics() {
        drop(RefAny::new_c(
            core::ptr::null(),
            4,
            4,
            RefAny::get_type_id_static::<u32>(),
            AzString::from_const_str("autotest::NullPtr"),
            noop_destructor,
            0,
            0,
        ));
    }

    // A non-power-of-two alignment cannot form a valid `Layout`; it must panic
    // before allocating rather than allocate with a bogus layout (which would
    // make the matching `dealloc` in `drop` UB).
    #[test]
    #[should_panic(expected = "Failed to create layout")]
    fn new_c_non_power_of_two_align_panics() {
        let value: u32 = 7;
        drop(RefAny::new_c(
            (&raw const value).cast::<c_void>(),
            4,
            3, // not a power of two
            RefAny::get_type_id_static::<u32>(),
            AzString::from_const_str("autotest::BadAlign"),
            noop_destructor,
            0,
            0,
        ));
    }

    // `usize::MAX` bytes overflows `isize::MAX` and cannot be a `Layout`: the
    // checked constructor must reject it (no silent overflow into a tiny alloc).
    #[test]
    #[should_panic(expected = "Failed to create layout")]
    fn new_c_huge_len_panics_instead_of_overflowing() {
        let value: u8 = 1;
        drop(RefAny::new_c(
            (&raw const value).cast::<c_void>(),
            usize::MAX,
            1,
            RefAny::get_type_id_static::<u8>(),
            AzString::from_const_str("autotest::HugeLen"),
            noop_destructor,
            0,
            0,
        ));
    }

    // len == 0 is the ZST path: NULL data pointer is legal, `align` is ignored
    // (even a nonsensical 0), nothing is allocated, and downcasts fail cleanly.
    #[test]
    fn new_c_zero_len_null_ptr_is_a_clean_zst() {
        let mut a = RefAny::new_c(
            core::ptr::null(),
            0,
            0, // invalid alignment, but unused on the ZST path
            RefAny::get_type_id_static::<()>(),
            AzString::from_const_str("autotest::Zst"),
            noop_destructor,
            0,
            0,
        );
        assert_eq!(a.get_data_len(), 0);
        assert!(a.get_data_ptr().is_null());
        assert!(a.is_type(RefAny::get_type_id_static::<()>()));
        // Type matches, but there is no data behind the pointer -> None, and the
        // borrow counters must be released again on that bail-out path.
        assert!(a.downcast_ref::<()>().is_none());
        assert!(a.downcast_mut::<()>().is_none());
        assert!(a.sharing_info.can_be_shared_mut());
    }

    // Round-trip through the raw C-ABI constructor: what `new_c` encodes,
    // `downcast_ref` must decode bit-for-bit.
    #[test]
    fn new_c_round_trip_matches_rust_constructor() {
        let value: u64 = 0xDEAD_BEEF_CAFE_BABE;
        let mut a = RefAny::new_c(
            (&raw const value).cast::<c_void>(),
            core::mem::size_of::<u64>(),
            core::mem::align_of::<u64>(),
            RefAny::get_type_id_static::<u64>(),
            AzString::from_const_str("u64"),
            noop_destructor,
            7,
            9,
        );
        assert_eq!(a.get_data_len(), core::mem::size_of::<u64>());
        assert_eq!(a.get_ref_count(), 1);
        assert_eq!(a.get_serialize_fn(), 7);
        assert_eq!(a.get_deserialize_fn(), 9);
        assert!(a.can_serialize());
        assert!(a.can_deserialize());
        assert_eq!(*a.downcast_ref::<u64>().unwrap(), value);
    }

    // The runtime guard is the type ID, nothing else: a matching size, name and
    // destructor must NOT be enough to downcast if the ID differs by one bit.
    #[test]
    fn new_c_wrong_type_id_rejects_downcast() {
        let value: u64 = 0x0102_0304_0506_0708;
        let real_id = RefAny::get_type_id_static::<u64>();
        let mut a = RefAny::new_c(
            (&raw const value).cast::<c_void>(),
            core::mem::size_of::<u64>(),
            core::mem::align_of::<u64>(),
            real_id ^ 1, // one bit off
            AzString::from_const_str("u64"),
            noop_destructor,
            0,
            0,
        );
        assert!(!a.is_type(real_id));
        assert!(a.downcast_ref::<u64>().is_none());
        assert!(a.downcast_mut::<u64>().is_none());
        // The rejected downcasts must not have left a borrow behind.
        assert!(a.sharing_info.can_be_shared_mut());
    }

    // Over-alignment (align > len) is a valid `Layout`; the payload must land on
    // an address that satisfies the requested alignment.
    #[test]
    fn new_c_over_aligned_small_payload() {
        let value: u8 = 0x5A;
        let mut a = RefAny::new_c(
            (&raw const value).cast::<c_void>(),
            1,
            16,
            RefAny::get_type_id_static::<u8>(),
            AzString::from_const_str("u8"),
            noop_destructor,
            0,
            0,
        );
        assert_eq!(a.get_data_ptr() as usize % 16, 0);
        assert_eq!(*a.downcast_ref::<u8>().unwrap(), 0x5A);
    }

    // The type name is arbitrary caller-supplied UTF-8 (generated by foreign
    // codegen): empty, unicode, RTL overrides and embedded NULs must survive.
    #[test]
    fn new_c_preserves_unicode_and_empty_type_names() {
        let value: u32 = 0;
        let weird = "app::💥Ünïcødé<T>\u{202E}rtl\u{0}nul";
        let a = RefAny::new_c(
            (&raw const value).cast::<c_void>(),
            4,
            4,
            1,
            AzString::from(String::from(weird)),
            noop_destructor,
            0,
            0,
        );
        assert_eq!(a.get_type_name().as_str(), weird);

        let b = RefAny::new_c(
            (&raw const value).cast::<c_void>(),
            4,
            4,
            2,
            AzString::from_const_str(""),
            noop_destructor,
            0,
            0,
        );
        assert_eq!(b.get_type_name().as_str(), "");
    }

    // ---- RefAny::new — post-construction invariants ----

    #[test]
    fn new_invariants_hold() {
        let mut a = RefAny::new(0x1122_3344u32);
        assert_eq!(a.get_data_len(), core::mem::size_of::<u32>());
        assert!(!a.get_data_ptr().is_null());
        assert_eq!(a.get_data_ptr() as usize % core::mem::align_of::<u32>(), 0);
        assert_eq!(a.get_type_id(), RefAny::get_type_id_static::<u32>());
        assert!(a.is_type(RefAny::get_type_id_static::<u32>()));
        assert_eq!(a.get_type_name().as_str(), "u32");
        assert_eq!(a.get_ref_count(), 1);
        assert!(a.has_no_copies());
        assert_eq!(a.get_serialize_fn(), 0);
        assert_eq!(a.get_deserialize_fn(), 0);
        assert_eq!(a.get_update_fn(), 0);
        assert!(!a.can_serialize());
        assert!(!a.can_deserialize());
        assert!(a.sharing_info.can_be_shared());
        assert!(a.sharing_info.can_be_shared_mut());
        assert_eq!(a.instance_id, 0);
        assert_eq!(*a.downcast_ref::<u32>().unwrap(), 0x1122_3344);
    }

    // A zero-length array of an 8-aligned element is still a ZST: `new` must take
    // the null-pointer path (no zero-size allocation, which would be UB).
    #[test]
    fn new_zero_sized_array_of_aligned_type_is_a_zst() {
        let mut a = RefAny::new([0u64; 0]);
        assert_eq!(a.get_data_len(), 0);
        assert!(a.get_data_ptr().is_null());
        assert_eq!(
            a.sharing_info.debug_get_refcount_copied()._internal_layout_size,
            0
        );
        assert!(a.downcast_ref::<[u64; 0]>().is_none());
        assert_eq!(a.get_ref_count(), 1);
    }

    // Large + heavily over-aligned payload: the alignment recorded at
    // construction must be honoured by the allocation, or every downcast would
    // hand out a misaligned reference.
    #[test]
    fn new_large_over_aligned_payload_round_trips() {
        #[repr(align(64))]
        #[derive(Clone)]
        struct Big([u8; 4096]);

        let mut a = RefAny::new(Big([0xAB; 4096]));
        assert_eq!(a.get_data_len(), 4096);
        assert_eq!(a.get_data_ptr() as usize % 64, 0);
        let r = a.downcast_ref::<Big>().unwrap();
        assert_eq!((&raw const *r) as usize % 64, 0);
        assert!(r.0.iter().all(|&b| b == 0xAB));
    }

    // ---- numeric limits / round-trip ----

    #[test]
    fn integer_limits_round_trip() {
        round_trip(u8::MIN);
        round_trip(u8::MAX);
        round_trip(i8::MIN);
        round_trip(i8::MAX);
        round_trip(u16::MAX);
        round_trip(i16::MIN);
        round_trip(u32::MAX);
        round_trip(i32::MIN);
        round_trip(u64::MAX);
        round_trip(i64::MIN);
        // u128/i128 are 16-aligned on most targets -> exercises the align path
        round_trip(u128::MAX);
        round_trip(i128::MIN);
        round_trip(i128::MAX);
        round_trip(usize::MAX);
        round_trip(isize::MIN);
        round_trip(0usize);
    }

    // Floats are copied as raw bytes, so every bit pattern (NaN payloads, signed
    // zero, infinities) must survive unchanged — no normalization, no rounding.
    #[test]
    fn float_extremes_round_trip_bit_exact() {
        let mut nan = RefAny::new(f64::NAN);
        assert!(nan.downcast_ref::<f64>().unwrap().is_nan());

        // A NaN with a non-canonical payload must come back bit-identical.
        let bits = 0x7FF0_0000_0000_0001u64;
        let mut payload_nan = RefAny::new(f64::from_bits(bits));
        assert_eq!(payload_nan.downcast_ref::<f64>().unwrap().to_bits(), bits);

        let mut neg_zero = RefAny::new(-0.0f64);
        let nz = neg_zero.downcast_ref::<f64>().unwrap();
        assert!(*nz == 0.0 && nz.is_sign_negative());
        drop(nz);

        let mut inf = RefAny::new(f32::NEG_INFINITY);
        assert_eq!(*inf.downcast_ref::<f32>().unwrap(), f32::NEG_INFINITY);
        // f32 and f64 are distinct types even though both are "floats".
        assert!(inf.downcast_ref::<f64>().is_none());

        round_trip(f64::MIN);
        round_trip(f64::MAX);
        round_trip(f64::MIN_POSITIVE);
        round_trip(f32::EPSILON);
        round_trip(f32::MAX);
    }

    // Owned heap payloads: the value is moved in (`mem::forget` on the original)
    // and dropped exactly once at the end — a double-drop here would be a
    // double-free of the String/Vec buffers.
    #[test]
    fn owned_unicode_payloads_round_trip() {
        round_trip(String::new());
        round_trip(String::from("héllo 🌍 \u{202E}rtl\u{0}nul"));
        round_trip('🌍');

        let v: Vec<String> = vec![String::from("a"), String::from("🎉"), String::new()];
        round_trip(v);
    }

    // A struct with interior padding is byte-copied, padding included: the copy
    // must not disturb the initialized fields.
    #[test]
    fn padded_struct_round_trips() {
        #[derive(Clone, PartialEq, Debug)]
        #[repr(C)]
        struct Padded {
            a: u8,
            b: u64,
            c: u8,
        }
        round_trip(Padded {
            a: 0xFF,
            b: u64::MAX,
            c: 0x01,
        });
    }

    // ---- setters: 0 / 1 / usize::MAX (never dereferenced by azul-core) ----

    #[test]
    fn set_serialize_fn_zero_and_extremes() {
        let mut a = RefAny::new(1u32);
        assert_eq!(a.get_serialize_fn(), 0);
        assert!(!a.can_serialize());

        a.set_serialize_fn(usize::MAX);
        assert_eq!(a.get_serialize_fn(), usize::MAX);
        assert!(a.can_serialize());

        a.set_serialize_fn(1);
        assert_eq!(a.get_serialize_fn(), 1);
        assert!(a.can_serialize());

        a.set_serialize_fn(0);
        assert_eq!(a.get_serialize_fn(), 0);
        assert!(!a.can_serialize());

        // The fn pointer lives in the SHARED inner, so a clone's setter is
        // visible through the original.
        let mut b = a.clone();
        b.set_serialize_fn(42);
        assert_eq!(a.get_serialize_fn(), 42);
        assert!(a.can_serialize());
        b.set_serialize_fn(0);
        assert!(!a.can_serialize());
    }

    #[test]
    fn set_deserialize_fn_zero_and_extremes() {
        let mut a = RefAny::new(1u32);
        assert_eq!(a.get_deserialize_fn(), 0);
        assert!(!a.can_deserialize());

        a.set_deserialize_fn(usize::MAX);
        assert_eq!(a.get_deserialize_fn(), usize::MAX);
        assert!(a.can_deserialize());

        a.set_deserialize_fn(1);
        assert_eq!(a.get_deserialize_fn(), 1);

        a.set_deserialize_fn(0);
        assert_eq!(a.get_deserialize_fn(), 0);
        assert!(!a.can_deserialize());

        let mut b = a.clone();
        b.set_deserialize_fn(42);
        assert_eq!(a.get_deserialize_fn(), 42);
        b.set_deserialize_fn(0);
        assert!(!a.can_deserialize());
    }

    // `set_update_fn` only *stores* the address; a bogus value must round-trip
    // and must be resettable to 0. (Deliberately no `downcast_mut` while the
    // observer is bogus — `downcast_mut` transmutes and CALLS it.)
    #[test]
    fn set_update_fn_zero_and_extremes() {
        let mut a = RefAny::new(1u32);
        assert_eq!(a.get_update_fn(), 0);

        a.set_update_fn(usize::MAX);
        assert_eq!(a.get_update_fn(), usize::MAX);

        a.set_update_fn(0);
        assert_eq!(a.get_update_fn(), 0);
        // With the observer unset again, mutable borrows work as normal.
        assert!(a.downcast_mut::<u32>().is_some());
    }

    // The registered observer must fire exactly once per *successful*
    // `downcast_mut`, and must see the PRE-mutation bytes + the payload length.
    static UPDATE_CALLS: AtomicUsize = AtomicUsize::new(0);
    static UPDATE_LEN: AtomicUsize = AtomicUsize::new(0);
    static UPDATE_PRE_VALUE: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn record_update(ptr: *const c_void, len: usize) {
        UPDATE_CALLS.fetch_add(1, Ordering::SeqCst);
        UPDATE_LEN.store(len, Ordering::SeqCst);
        if !ptr.is_null() && len == core::mem::size_of::<u32>() {
            // SAFETY: only installed on a `RefAny` holding a `u32`, and
            // `downcast_mut` fires it with that live payload pointer.
            let pre = unsafe { core::ptr::read_unaligned(ptr.cast::<u32>()) };
            UPDATE_PRE_VALUE.store(pre as usize, Ordering::SeqCst);
        }
    }

    #[test]
    fn update_fn_fires_once_with_pre_mutation_data() {
        UPDATE_CALLS.store(0, Ordering::SeqCst);

        let mut a = RefAny::new(7u32);
        let cb: extern "C" fn(*const c_void, usize) = record_update;
        a.set_update_fn(cb as usize);
        assert_eq!(a.get_update_fn(), cb as usize);

        {
            let mut m = a.downcast_mut::<u32>().unwrap();
            *m = 9;
        }
        assert_eq!(UPDATE_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(UPDATE_LEN.load(Ordering::SeqCst), 4);
        // The observer saw 7, not 9: it runs BEFORE the borrow is handed out.
        assert_eq!(UPDATE_PRE_VALUE.load(Ordering::SeqCst), 7);

        // A wrong-type downcast must not fire it.
        assert!(a.downcast_mut::<u64>().is_none());
        assert_eq!(UPDATE_CALLS.load(Ordering::SeqCst), 1);

        // A shared borrow is not a mutation -> must not fire it.
        assert_eq!(*a.downcast_ref::<u32>().unwrap(), 9);
        assert_eq!(UPDATE_CALLS.load(Ordering::SeqCst), 1);

        // A *denied* mutable borrow (shared borrow live on a sibling clone)
        // must not fire it either.
        let mut b = a.clone();
        let r = a.downcast_ref::<u32>().unwrap();
        assert!(b.downcast_mut::<u32>().is_none());
        assert_eq!(UPDATE_CALLS.load(Ordering::SeqCst), 1);
        drop(r);

        // Unregistering stops the observer.
        b.set_update_fn(0);
        assert!(b.downcast_mut::<u32>().is_some());
        assert_eq!(UPDATE_CALLS.load(Ordering::SeqCst), 1);
    }

    // ---- predicates ----

    #[test]
    fn is_type_true_false_and_extremes() {
        let a = RefAny::new(0u32);
        let id = a.get_type_id();

        assert!(a.is_type(id));
        assert!(!a.is_type(!id)); // every bit flipped -> always a different id
        assert!(!a.is_type(id.wrapping_add(1)));
        assert!(!a.is_type(RefAny::get_type_id_static::<i32>()));
        if id != 0 {
            assert!(!a.is_type(0));
        }
        if id != u64::MAX {
            assert!(!a.is_type(u64::MAX));
        }
    }

    #[test]
    fn has_no_copies_transitions() {
        let mut a = RefAny::new(1u32);
        assert!(a.has_no_copies());

        {
            let b = a.clone();
            assert!(!a.has_no_copies()); // num_copies == 2
            assert!(!b.has_no_copies());
        }
        assert!(a.has_no_copies()); // clone dropped -> exclusive again

        {
            // A live shared borrow (taken via a sibling clone) also disqualifies.
            let mut c = a.clone();
            let r = c.downcast_ref::<u32>().unwrap();
            assert_eq!(*r, 1);
            assert!(!a.has_no_copies());
        }
        assert!(a.has_no_copies());

        {
            let mut c = a.clone();
            let m = c.downcast_mut::<u32>().unwrap();
            assert_eq!(*m, 1);
            assert!(!a.has_no_copies());
        }
        assert!(a.has_no_copies());
    }

    #[test]
    fn can_serialize_and_can_deserialize_track_the_fn_pointers() {
        let mut a = RefAny::new(1u32);
        assert!(!a.can_serialize());
        assert!(!a.can_deserialize());

        a.set_serialize_fn(1);
        assert!(a.can_serialize());
        assert!(!a.can_deserialize());

        a.set_deserialize_fn(usize::MAX);
        assert!(a.can_serialize());
        assert!(a.can_deserialize());

        a.set_serialize_fn(0);
        a.set_deserialize_fn(0);
        assert!(!a.can_serialize());
        assert!(!a.can_deserialize());
    }

    // ---- getters ----

    #[test]
    fn get_ref_count_tracks_clones_and_borrow_guards() {
        let mut a = RefAny::new(5u8);
        assert_eq!(a.get_ref_count(), 1);

        let mut b = a.clone();
        assert_eq!(a.get_ref_count(), 2);
        assert_eq!(b.get_ref_count(), 2);

        {
            // The guard clones the RefCount, so it keeps the data alive.
            let r = b.downcast_ref::<u8>().unwrap();
            assert_eq!(*r, 5);
            assert_eq!(a.get_ref_count(), 3);
        }
        assert_eq!(a.get_ref_count(), 2);

        {
            let m = b.downcast_mut::<u8>().unwrap();
            assert_eq!(*m, 5);
            assert_eq!(a.get_ref_count(), 3);
        }
        assert_eq!(a.get_ref_count(), 2);

        drop(b);
        assert_eq!(a.get_ref_count(), 1);
        assert_eq!(*a.downcast_ref::<u8>().unwrap(), 5);
    }

    #[test]
    fn debug_snapshot_matches_the_live_counters() {
        let a = RefAny::new(0x1122_3344u32);
        let d = a.sharing_info.debug_get_refcount_copied();
        assert_eq!(d.num_copies, 1);
        assert_eq!(d.num_refs, 0);
        assert_eq!(d.num_mutable_refs, 0);
        assert_eq!(d._internal_len, 4);
        assert_eq!(d._internal_layout_size, 4);
        assert_eq!(d._internal_layout_align, core::mem::align_of::<u32>());
        assert_eq!(d.type_id, RefAny::get_type_id_static::<u32>());
        assert_eq!(d.type_name.as_str(), "u32");
        assert_ne!(d.custom_destructor, 0);
        assert_eq!(d.serialize_fn, 0);
        assert_eq!(d.deserialize_fn, 0);

        a.sharing_info.increase_ref();
        a.sharing_info.increase_refmut();
        let d2 = a.sharing_info.debug_get_refcount_copied();
        assert_eq!(d2.num_refs, 1);
        assert_eq!(d2.num_mutable_refs, 1);
        // The first snapshot is a copy, not a view: it must not have changed.
        assert_eq!(d.num_refs, 0);

        a.sharing_info.decrease_ref();
        a.sharing_info.decrease_refmut();
        let d3 = a.sharing_info.debug_get_refcount_copied();
        assert_eq!((d3.num_refs, d3.num_mutable_refs), (0, 0));

        // The Debug impl goes through `downcast()` — it must not panic.
        assert!(!alloc::format!("{:?}", a.sharing_info).is_empty());
    }

    #[test]
    fn get_type_name_reports_the_rust_type() {
        #[derive(Clone)]
        struct AutotestNamed(#[allow(dead_code)] u8);

        let a = RefAny::new(AutotestNamed(1));
        let name = a.get_type_name();
        assert!(
            name.as_str().contains("AutotestNamed"),
            "unexpected type name: {}",
            name.as_str()
        );

        let generic = RefAny::new(Vec::<String>::new());
        assert!(generic.get_type_name().as_str().contains("Vec"));

        assert_eq!(RefAny::new(1u32).get_type_name().as_str(), "u32");
    }

    // ---- RefCount: construction, downcast, clone/drop balance ----

    #[test]
    fn refcount_new_downcast_and_clone_lifecycle() {
        let rc = RefCount::new(RefCountInner {
            _internal_ptr: core::ptr::null(),
            num_copies: AtomicUsize::new(1),
            num_refs: AtomicUsize::new(0),
            num_mutable_refs: AtomicUsize::new(0),
            _internal_len: 0,
            _internal_layout_size: 0,
            _internal_layout_align: 1,
            type_id: 0xDEAD_BEEF,
            type_name: AzString::from_const_str("autotest::Synthetic"),
            custom_destructor: noop_destructor,
            serialize_fn: 0,
            deserialize_fn: 0,
            update_fn: 0,
        });
        assert!(!rc.ptr.is_null());
        assert!(rc.run_destructor);

        let inner = rc.downcast();
        assert_eq!(inner.type_id, 0xDEAD_BEEF);
        assert_eq!(inner.type_name.as_str(), "autotest::Synthetic");
        assert_eq!(inner._internal_len, 0);
        assert!(rc.can_be_shared());
        assert!(rc.can_be_shared_mut());

        // Clones must keep the boxed inner alive; the counters must return to 1
        // so the final drop frees it exactly once.
        let c1 = rc.clone();
        assert_eq!(rc.debug_get_refcount_copied().num_copies, 2);
        let c2 = c1.clone();
        assert_eq!(rc.debug_get_refcount_copied().num_copies, 3);
        drop(c2);
        drop(c1);
        assert_eq!(rc.debug_get_refcount_copied().num_copies, 1);
    }

    // The borrow counters must saturate at 0 instead of wrapping to usize::MAX
    // (an unmatched `FooRef_delete` from C would otherwise permanently wedge the
    // runtime borrow checker), and stay usable afterwards.
    #[test]
    fn borrow_counters_saturate_at_zero_and_stay_usable() {
        let mut a = RefAny::new(3i64);
        {
            let rc = &a.sharing_info;

            // 64 unmatched decrements on both counters.
            for _ in 0..64 {
                rc.decrease_ref();
                rc.decrease_refmut();
            }
            let d = rc.debug_get_refcount_copied();
            assert_eq!(d.num_refs, 0);
            assert_eq!(d.num_mutable_refs, 0);
            assert!(rc.can_be_shared());
            assert!(rc.can_be_shared_mut());

            // Many shared borrows coexist, but block a mutable one.
            for _ in 0..256 {
                rc.increase_ref();
            }
            assert_eq!(rc.debug_get_refcount_copied().num_refs, 256);
            assert!(rc.can_be_shared());
            assert!(!rc.can_be_shared_mut());
            for _ in 0..256 {
                rc.decrease_ref();
            }
            assert_eq!(rc.debug_get_refcount_copied().num_refs, 0);
            assert!(rc.can_be_shared_mut());

            // Same for the mutable counter, plus one extra decrement.
            rc.increase_refmut();
            rc.increase_refmut();
            assert!(!rc.can_be_shared());
            rc.decrease_refmut();
            rc.decrease_refmut();
            rc.decrease_refmut();
            assert_eq!(rc.debug_get_refcount_copied().num_mutable_refs, 0);
        }

        // The borrow checker still works after all those underflow attempts.
        assert_eq!(*a.downcast_ref::<i64>().unwrap(), 3);
        assert!(a.downcast_mut::<i64>().is_some());
    }

    // ---- get_type_id_static ----

    // The u64 type ID is the ONLY runtime guard against a wrong-type downcast,
    // so distinct types must not collide (this is what folding ALL TypeId bytes
    // buys us) and it must be stable within a process run.
    #[test]
    fn type_id_static_is_stable_and_collision_free() {
        let ids = [
            RefAny::get_type_id_static::<u8>(),
            RefAny::get_type_id_static::<u16>(),
            RefAny::get_type_id_static::<u32>(),
            RefAny::get_type_id_static::<u64>(),
            RefAny::get_type_id_static::<u128>(),
            RefAny::get_type_id_static::<usize>(),
            RefAny::get_type_id_static::<i8>(),
            RefAny::get_type_id_static::<i16>(),
            RefAny::get_type_id_static::<i32>(),
            RefAny::get_type_id_static::<i64>(),
            RefAny::get_type_id_static::<i128>(),
            RefAny::get_type_id_static::<isize>(),
            RefAny::get_type_id_static::<f32>(),
            RefAny::get_type_id_static::<f64>(),
            RefAny::get_type_id_static::<bool>(),
            RefAny::get_type_id_static::<char>(),
            RefAny::get_type_id_static::<()>(),
            RefAny::get_type_id_static::<String>(),
            RefAny::get_type_id_static::<Vec<u8>>(),
            RefAny::get_type_id_static::<Vec<u16>>(),
            RefAny::get_type_id_static::<[u8; 1]>(),
            RefAny::get_type_id_static::<[u8; 2]>(),
            RefAny::get_type_id_static::<(u8, u8)>(),
            RefAny::get_type_id_static::<(u8, u16)>(),
            RefAny::get_type_id_static::<Option<u8>>(),
            RefAny::get_type_id_static::<Option<u16>>(),
        ];

        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j], "type id collision between {i} and {j}");
            }
        }

        // Deterministic within a run.
        assert_eq!(RefAny::get_type_id_static::<Vec<u8>>(), ids[18]);
        assert_eq!(RefAny::get_type_id_static::<u8>(), ids[0]);
    }

    // ---- clone / instance ids ----

    #[test]
    fn root_instance_id_is_zero_and_clones_are_distinct() {
        let a = RefAny::new(0u8);
        assert_eq!(a.instance_id, 0);

        let b = a.clone();
        let c = b.clone();
        assert_ne!(b.instance_id, 0);
        assert_ne!(c.instance_id, 0);
        assert_ne!(b.instance_id, c.instance_id);
        assert_eq!(a.get_ref_count(), 3);
    }

    // ---- replace_contents ----

    #[test]
    fn replace_contents_zst_and_value_transitions() {
        #[derive(Clone)]
        struct Zst;

        let mut a = RefAny::new(Zst);
        assert_eq!(a.get_data_len(), 0);
        assert!(a.get_data_ptr().is_null());

        // ZST -> sized: a real allocation must appear.
        assert!(a.replace_contents(RefAny::new(0x4142_4344u32)));
        assert_eq!(a.get_data_len(), 4);
        assert!(!a.get_data_ptr().is_null());
        assert!(a.is_type(RefAny::get_type_id_static::<u32>()));
        assert_eq!(*a.downcast_ref::<u32>().unwrap(), 0x4142_4344);

        // sized -> ZST: the pointer goes back to null and downcasts must fail
        // safely (releasing the borrow slot they speculatively took).
        assert!(a.replace_contents(RefAny::new(Zst)));
        assert_eq!(a.get_data_len(), 0);
        assert!(a.get_data_ptr().is_null());
        assert!(a.downcast_ref::<Zst>().is_none());
        assert!(a.downcast_mut::<Zst>().is_none());
        assert!(a.sharing_info.can_be_shared_mut());
    }

    #[test]
    fn replace_contents_is_visible_to_all_clones() {
        let mut a = RefAny::new(1u32);
        let mut b = a.clone();

        assert!(a.replace_contents(RefAny::new(2u32)));
        assert_eq!(*b.downcast_ref::<u32>().unwrap(), 2);

        // The type may change too — every clone sees the new type.
        assert!(a.replace_contents(RefAny::new(String::from("swapped"))));
        assert!(b.downcast_ref::<u32>().is_none());
        assert_eq!(b.downcast_ref::<String>().unwrap().as_str(), "swapped");
        assert!(b.get_type_name().as_str().contains("String"));
        assert_eq!(b.get_type_id(), RefAny::get_type_id_static::<String>());
    }

    #[test]
    fn replace_contents_denied_while_mutably_borrowed() {
        let mut a = RefAny::new(1u32);
        let mut b = a.clone();

        let m = a.downcast_mut::<u32>().unwrap();
        // A live mutable borrow on the shared inner must block the replacement
        // (performing it would free memory the `RefMut` still points at).
        assert!(!b.replace_contents(RefAny::new(2u32)));
        drop(m);

        assert!(b.replace_contents(RefAny::new(2u32)));
        assert_eq!(*b.downcast_ref::<u32>().unwrap(), 2);
    }

    // The serialize/deserialize/update hooks are part of the replaced metadata:
    // after a replacement they describe the NEW value, not the old one.
    #[test]
    fn replace_contents_resets_the_fn_pointers_to_the_new_value() {
        let mut a = RefAny::new(1u32);
        a.set_serialize_fn(3);
        a.set_deserialize_fn(4);
        assert!(a.can_serialize());
        assert!(a.can_deserialize());

        assert!(a.replace_contents(RefAny::new(2u32)));
        assert_eq!(a.get_serialize_fn(), 0);
        assert_eq!(a.get_deserialize_fn(), 0);
        assert_eq!(a.get_update_fn(), 0);
        assert!(!a.can_serialize());
        assert!(!a.can_deserialize());
    }

    // Repeated replacement across changing sizes/alignments must neither leak nor
    // corrupt the payload (Miri checks the alloc/dealloc balance here).
    #[test]
    fn repeated_replace_contents_stays_consistent() {
        let mut a = RefAny::new(String::from("start"));
        for i in 0..16u32 {
            assert!(a.replace_contents(RefAny::new(i)));
            assert_eq!(*a.downcast_ref::<u32>().unwrap(), i);
            assert!(a.replace_contents(RefAny::new(u128::from(i) | (1 << 100))));
            assert_eq!(
                *a.downcast_ref::<u128>().unwrap(),
                u128::from(i) | (1 << 100)
            );
            assert!(a.replace_contents(RefAny::new(String::from("s"))));
        }
        assert_eq!(a.downcast_ref::<String>().unwrap().as_str(), "s");
    }

    // ---- destructor robustness / concurrency ----

    // `default_custom_destructor` is `extern "C"`: a panic from the payload's
    // `Drop` must be caught there, not unwound across the FFI boundary (UB).
    #[cfg(feature = "std")]
    #[test]
    fn panicking_payload_drop_is_contained() {
        struct PanicOnDrop(#[allow(dead_code)] u64);
        impl Drop for PanicOnDrop {
            fn drop(&mut self) {
                panic!("autotest: payload Drop panicked (expected, must be contained)");
            }
        }

        let a = RefAny::new(PanicOnDrop(1));
        drop(a); // must not propagate the panic out of the extern "C" destructor
    }

    // RefAny is Send + Sync: concurrent clone/borrow/drop from several threads
    // must leave the reference count exactly where it started.
    #[cfg(feature = "std")]
    #[test]
    fn concurrent_clone_and_borrow_keeps_the_refcount_balanced() {
        use std::{sync::Arc, thread};

        let shared = Arc::new(RefAny::new(11u32));
        let mut handles = Vec::new();

        for _ in 0..4 {
            let s = Arc::clone(&shared);
            handles.push(thread::spawn(move || {
                for _ in 0..16 {
                    let mut local = (*s).clone();
                    // No thread takes a mutable borrow, so a shared borrow can
                    // never be denied.
                    let r = local
                        .downcast_ref::<u32>()
                        .expect("shared borrow must always succeed here");
                    assert_eq!(*r, 11);
                }
            }));
        }
        for h in handles {
            h.join().expect("worker thread panicked");
        }

        assert_eq!(shared.get_ref_count(), 1);
    }
}
