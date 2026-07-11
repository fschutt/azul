//! Host-language callback invoker registry.
//!
//! Managed-FFI bindings (Lua, Ruby, Perl, PHP, OCaml, Node, C#, Java, …) can't
//! generate C-ABI trampolines for callback typedefs that take aggregate args
//! by value — that's a libffi / LuaJIT FFI / ruby-ffi limitation we can't fix
//! at the host. This module provides the alternative the user's analysis
//! settled on: each language registers **one** generic invoker function at
//! module load time, plus a releaser that fires when a host-language handle
//! goes out of use.
//!
//! Every callback the host registers becomes a `Callback { cb, ctx }` pair
//! whose `cb` is a *static thunk* in libazul (so by-value args land on a
//! native frame the way the framework already expects), and whose `ctx` is
//! a `RefAny` payload that carries an opaque host-language `u64` handle.
//! The thunk reads `info.get_ctx()`, extracts the handle, and dispatches to
//! the registered per-kind invoker — which, on the host side, looks up the
//! callable by id in a host-managed table and runs it. When the RefAny's
//! refcount drops to zero, the destructor calls back through the registered
//! releaser so the host can drop its table entry, mirroring Python's
//! `Py<PyAny>` lifetime story without making libazul link against any host
//! runtime.
//!
//! ## API surface
//!
//! - [`AzApp_setHostHandleReleaser`] — register the host's "drop this id"
//!   callback once per process. Fires when a host-handle [`RefAny`] is
//!   collected.
//! - Per callback kind, [`crate::impl_managed_callback!`] expands to:
//!   - A static thunk (`extern "C" fn`) compiled into libazul.
//!   - A `<Wrapper>::create_from_host_handle(u64)` constructor.
//!   - An `AzApp_set<Kind>Invoker(...)` setter for the host-side per-kind
//!     pointer-arg invoker.
//!
//! ## Why a single shared releaser
//!
//! Per-kind invokers are necessarily distinct — each callback typedef has
//! a different signature, so the host has to register a libffi closure per
//! typedef anyway. The releaser, on the other hand, has the same signature
//! for every kind (`extern "C" fn(u64)`), so we can share one slot across
//! all callbacks; the host registers it once and every kind's destructor
//! routes through it.

use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};

use azul_css::AzString;

use crate::refany::RefAny;

/// RTTI id stamped into every `RefAny` created via [`host_handle_to_refany`].
///
/// Hosts must not reuse this id for their own user-data `RefAnys`, otherwise
/// `refany_to_host_handle` would mis-identify their data as a host handle
/// and the destructor would call the registered releaser with a bogus id.
/// The high 32 bits are reserved for azul-internal RTTI ids; the low 32
/// spell `'H','S','T','H'` so the value reads `0xA20A_4853_5448_5F44`.
pub const AZ_HOST_HANDLE_RTTI_ID: u64 = 0xA20A_4853_5448_5F44;

/// Heap payload stored inside the [`RefAny`] returned by
/// [`host_handle_to_refany`]. Just the opaque host-language id — the actual
/// host callable lives on the host side keyed by this id.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct HostHandlePayload {
    pub id: u64,
}

/// A single atomic-pointer slot for one registered host-side function
/// pointer.
///
/// `0` means "not registered"; the static thunks bail out (returning
/// the kind's default value) when they see an unregistered slot rather than
/// transmuting `0` into a fn pointer and crashing.
#[repr(C)]
#[derive(Debug)]
pub struct InvokerSlot {
    fn_ptr: AtomicUsize,
}

impl InvokerSlot {
    /// Create an empty slot. `const` so it can be used to declare `static`
    /// per-kind slots in `impl_managed_callback!` expansions.
    #[must_use] pub const fn new() -> Self {
        Self {
            fn_ptr: AtomicUsize::new(0),
        }
    }

    /// Replace the registered function pointer.
    ///
    /// `SeqCst` because the slot is read on every callback fire and we
    /// don't want any stale-pointer windows after the host swaps invokers
    /// (rare but legal — e.g. unloading a Lua module that registered).
    pub fn set(&self, ptr: usize) {
        self.fn_ptr.store(ptr, Ordering::SeqCst);
    }

    /// Read the current function pointer; `0` if unregistered.
    pub fn get(&self) -> usize {
        self.fn_ptr.load(Ordering::SeqCst)
    }
}

impl Default for InvokerSlot {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-global slot for the host's "drop a handle id" callback. Set via
/// [`AzApp_setHostHandleReleaser`]. Read by [`host_handle_destructor`]
/// when a host-handle [`RefAny`]'s last clone drops.
pub static HOST_HANDLE_RELEASER: InvokerSlot = InvokerSlot::new();

/// Process-global slot for the host's *generic* invoker.
///
/// Set via
/// [`AzApp_setGenericInvoker`]. Used as a fallback in macro-generated
/// per-kind thunks when the per-kind invoker is not registered, and as
/// the **only** dispatch path for user-defined custom callback kinds in
/// libffi-restricted hosts (Lua, PHP, koffi, …) that can't easily ship
/// an upstream `impl_managed_callback!` invocation.
///
/// Signature on the host side:
///
/// ```c
/// typedef void (*AzGenericInvoker)(
///     uint64_t           handle,    /* host-handle id from the RefAny ctx */
///     const char*        kind,      /* null-terminated wrapper name */
///     const void* const* args,      /* array of pointers, one per arg, in declared order */
///     size_t             n_args,    /* args[] length */
///     void*              ret        /* where to write the return value (kind-specific size) */
/// );
/// extern void AzApp_setGenericInvoker(AzGenericInvoker);
/// ```
///
/// The args array carries pointers into the framework's by-value frame
/// — host code must not retain them past the call. The host decides what
/// to do per kind from the `kind` string (which matches the wrapper
/// struct name, e.g. `"Callback"`, `"LayoutCallback"`,
/// `"ButtonOnClickCallback"`).
pub static GENERIC_INVOKER: InvokerSlot = InvokerSlot::new();

/// Type alias for the generic invoker callable. Hosts cast a libffi
/// closure to this signature once at module load.
pub type AzGenericInvoker = extern "C" fn(
    handle: u64,
    kind: *const core::ffi::c_char,
    args: *const *const c_void,
    n_args: usize,
    ret: *mut c_void,
);

/// Register the generic invoker for user-defined custom callback kinds
/// or as a fallback for per-kind dispatch. Called once at module load;
/// subsequent registrations replace the previous slot.
///
/// Safety: `invoker` must be a valid [`AzGenericInvoker`] function
/// pointer for the lifetime of any callback that might be dispatched
/// through it — typically the whole process.
#[no_mangle]
pub extern "C" fn AzApp_setGenericInvoker(invoker: AzGenericInvoker) {
    GENERIC_INVOKER.set(invoker as usize);
}

/// Register the host-language releaser. Hosts call this once at module
/// load time; subsequent registrations replace the previous slot.
///
/// `releaser` will be invoked as `releaser(id)` whenever a host-handle
/// `RefAny` (the kind built by [`host_handle_to_refany`]) drops its last
/// reference. The host should remove `id` from whatever id→callable table
/// it maintains.
///
/// Safety: `releaser` must be a valid `extern "C" fn(u64)` for the lifetime
/// of any host-handle [`RefAny`] that may still be alive — typically the
/// whole process. Passing a function pointer that becomes invalid (e.g.,
/// from an unloaded library) without first re-registering will cause a
/// crash on the next collection.
#[no_mangle]
pub extern "C" fn AzApp_setHostHandleReleaser(releaser: extern "C" fn(u64)) {
    HOST_HANDLE_RELEASER.set(releaser as usize);
}

/// Destructor stamped into every host-handle [`RefAny`]. Reads the payload's
/// `id` and forwards it to the registered releaser; if no releaser has been
/// registered (e.g., host hasn't initialized yet, or this is a release-build
/// dll loaded by a non-managed-FFI consumer) the destructor is a no-op so
/// the C side doesn't crash.
extern "C" fn host_handle_destructor(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: the destructor only runs for RefAnys built via
    // host_handle_to_refany, whose payload type is HostHandlePayload.
    let payload = unsafe { &*(ptr as *const HostHandlePayload) };

    let releaser_addr = HOST_HANDLE_RELEASER.get();
    if releaser_addr == 0 {
        return;
    }
    // SAFETY: HOST_HANDLE_RELEASER only ever holds a value that came from
    // `releaser as usize` in `AzApp_setHostHandleReleaser`, where `releaser`
    // is an `extern "C" fn(u64)`.
    let releaser: extern "C" fn(u64) = unsafe { core::mem::transmute(releaser_addr) };
    // AUDIT: this destructor is `extern "C"` and the host releaser is arbitrary
    // (often a Rust closure via libffi). A panic escaping it would unwind across
    // the FFI boundary (UB), so contain it. `catch_unwind` needs `std`; `no_std`
    // builds use `panic = "abort"` where unwinding cannot occur.
    #[cfg(feature = "std")]
    {
        drop(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| releaser(payload.id))));
    }
    #[cfg(not(feature = "std"))]
    {
        releaser(payload.id);
    }
}

/// Wrap a host-language `u64` handle in a [`RefAny`] suitable for storing
/// in a callback wrapper's `ctx` field.
///
/// The returned `RefAny`'s destructor calls back through the registered
/// host releaser when the last clone is dropped, giving the host an
/// opportunity to release whatever its `id` was keying.
pub fn host_handle_to_refany(id: u64) -> RefAny {
    let payload = HostHandlePayload { id };
    let type_name: AzString = "AzHostHandle".into();
    RefAny::new_c(
        &raw const payload as *const c_void,
        size_of::<HostHandlePayload>(),
        align_of::<HostHandlePayload>(),
        AZ_HOST_HANDLE_RTTI_ID,
        type_name,
        host_handle_destructor,
        0,
        0,
    )
}

/// Read the host-language id back out of a [`RefAny`] previously created
/// via [`host_handle_to_refany`].
///
/// Returns `None` for any other `RefAny`, so
/// a static thunk that mistakenly receives a non-host-handle ctx falls
/// back to the kind's default value rather than reading random bytes.
#[must_use] pub fn refany_to_host_handle(refany: &RefAny) -> Option<u64> {
    if !refany.is_type(AZ_HOST_HANDLE_RTTI_ID) {
        return None;
    }
    let ptr = refany.get_data_ptr() as *const HostHandlePayload;
    if ptr.is_null() {
        return None;
    }
    // SAFETY: type-id check above guarantees the payload was a HostHandlePayload.
    Some(unsafe { (*ptr).id })
}

/// C-ABI: build a [`RefAny`] wrapping a host-language id.
///
/// Lets managed-FFI
/// bindings use the same machinery for user data that callbacks already use
/// — one releaser, one id-keyed table, one lifetime story.
///
/// The returned `RefAny`'s destructor fires the releaser registered via
/// [`AzApp_setHostHandleReleaser`] once the last clone drops, so the host
/// can drop its `id → value` entry.
#[no_mangle]
pub extern "C" fn AzRefAny_newHostHandle(id: u64) -> RefAny {
    host_handle_to_refany(id)
}

/// C-ABI: read the host-language id from a [`RefAny`] previously built via
/// [`AzRefAny_newHostHandle`] (or any other host-handle constructor).
///
/// Returns `0` if `refany` is null or wasn't a host handle. Host bindings
/// must reserve `0` as "no value" — [`host_handle_to_refany`] never produces
/// `0` if the host's id allocator starts at `1` (the convention used by
/// every binding in this repo).
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)] // SAFETY/FFI: `*const T` is the C-ABI signature; the fn null-checks then derefs under the documented caller contract (C guarantees a valid ptr/len). Marking it `unsafe fn` would force unsafe blocks into the generated dll bindings.
pub extern "C" fn AzRefAny_getHostHandle(refany: *const RefAny) -> u64 {
    if refany.is_null() {
        return 0;
    }
    // SAFETY: caller's responsibility per `*const` signature.
    let r = unsafe { &*refany };
    refany_to_host_handle(r).unwrap_or(0)
}

/// Macro that expands to the per-callback-kind boilerplate:
///
/// a static thunk
/// (compiled into libazul) that the framework calls with by-value args, a
/// `<Wrapper>::create_from_host_handle(u64)` constructor, and an
/// `AzApp_set<Kind>Invoker` setter the host calls once at module load.
///
/// All identifiers are passed in explicitly so we don't need a proc-macro
/// dependency just to concatenate idents. Codegen emits invocations of this
/// macro from `ir.callback_typedefs`.
///
/// Caller responsibilities:
///
/// - The wrapper type must have public fields `cb: <typedef>` and
///   `ctx: OptionRefAny` — that's the standard shape every callback wrapper
///   in the framework already follows.
/// - `info_ty` must expose a `.get_ctx() -> OptionRefAny` method (also
///   standard for `*CallbackInfo` types).
/// - `default_ret` is returned when:
///   - the framework invokes the thunk with `OptionRefAny::None` ctx
///     (host called the typedef directly without going through this path),
///   - the ctx isn't a host-handle (host registered the wrapper but the
///     ctx came from somewhere else),
///   - or no invoker has been registered yet for this kind. Pick a value
///     that can't be confused with a "real" return — typically the kind's
///     "do nothing" / "empty body" default.
#[macro_export]
macro_rules! impl_managed_callback {
    // Form 1: simple two-argument callbacks `(RefAny, info) -> ret` —
    // matches `Callback`, `LayoutCallback`, `ButtonOnClickCallback`,
    // and the bulk of widget event callbacks. Identical to the
    // extras-form below with an empty extra-args list.
    (
        wrapper:        $wrapper:ty,
        info_ty:        $info_ty:ty,
        return_ty:      $ret:ty,
        default_ret:    $default:expr,
        invoker_static: $invoker_static:ident,
        invoker_ty:     $invoker_ty:ident,
        thunk_fn:       $thunk_fn:ident,
        setter_fn:      $setter_fn:ident,
        from_handle_fn: $from_handle_fn:ident,
    ) => {
        $crate::impl_managed_callback! {
            wrapper:        $wrapper,
            info_ty:        $info_ty,
            return_ty:      $ret,
            default_ret:    $default,
            invoker_static: $invoker_static,
            invoker_ty:     $invoker_ty,
            thunk_fn:       $thunk_fn,
            setter_fn:      $setter_fn,
            from_handle_fn: $from_handle_fn,
            extra_args:     [],
        }
    };
    // Form 2: callbacks that take additional state after info — e.g.
    // `CheckBoxOnToggleCallback(RefAny, CallbackInfo, CheckBoxState)`.
    // The extras list is forwarded by reference into the host invoker
    // so libffi-style runtimes never have to handle aggregate-by-value
    // returns OR aggregate-by-value args.
    (
        wrapper:        $wrapper:ty,
        info_ty:        $info_ty:ty,
        return_ty:      $ret:ty,
        default_ret:    $default:expr,
        invoker_static: $invoker_static:ident,
        invoker_ty:     $invoker_ty:ident,
        thunk_fn:       $thunk_fn:ident,
        setter_fn:      $setter_fn:ident,
        from_handle_fn: $from_handle_fn:ident,
        extra_args:     [ $( $extra_name:ident : $extra_ty:ty ),* $(,)? ] $(,)?
    ) => {
        /// Process-global slot for this callback kind's host-side invoker.
        pub static $invoker_static: $crate::host_invoker::InvokerSlot =
            $crate::host_invoker::InvokerSlot::new();

        /// Pointer-arg variant of this callback kind's typedef.
        ///
        /// The host's libffi closure casts to this signature (which all
        /// managed-FFI runtimes can handle — args and return are passed
        /// by pointer, no aggregate-by-value anywhere). The static thunk
        /// in libazul does the by-value plumbing on the C ABI side.
        ///
        /// `LuaJIT` FFI in particular cannot return aggregates larger than
        /// 8 bytes from a callback, so we use an out-pointer for the
        /// return value uniformly across kinds — even for `Update` which
        /// would fit in a register, so the macro stays homogeneous.
        pub type $invoker_ty = extern "C" fn(
            handle: u64,
            data: *const $crate::refany::RefAny,
            info: *const $info_ty,
            $( $extra_name : *const $extra_ty , )*
            out: *mut $ret,
        );

        /// Register the host-side invoker for this callback kind.
        #[no_mangle]
        pub extern "C" fn $setter_fn(invoker: $invoker_ty) {
            $invoker_static.set(invoker as usize);
        }

        /// Static thunk compiled into libazul. The framework calls this
        /// with by-value args; we extract the host handle from `info.ctx`,
        /// allocate space for the return value on our stack, and forward
        /// pointers to the registered invoker.
        extern "C" fn $thunk_fn(
            data: $crate::refany::RefAny,
            info: $info_ty,
            $( $extra_name : $extra_ty , )*
        ) -> $ret {
            // Wrapper name as a null-terminated C string. `stringify!`
            // expands `$wrapper:ty` to e.g. `Callback`,
            // `ButtonOnClickCallback`, etc. — matching what the host's
            // dispatch table keys on.
            const KIND_STR: &str = concat!(stringify!($wrapper), "\0");

            // AUDIT: this thunk is `extern "C"` and dispatches into arbitrary
            // host code (via a transmuted invoker pointer). A panic escaping the
            // dispatch would unwind across the FFI boundary (UB), so run the
            // whole body inside `catch_unwind` and fall back to `$default` on a
            // panic. `catch_unwind` needs `std`; `no_std` builds use
            // `panic = "abort"` where unwinding cannot occur. The body captures
            // `data`/`info`/extras by move (they are consumed either way).
            let body = move || -> $ret {
                let ctx = info.get_ctx();
                let handle = match ctx {
                    $crate::refany::OptionRefAny::Some(ref refany) => {
                        match $crate::host_invoker::refany_to_host_handle(refany) {
                            Some(id) => id,
                            None => return $default,
                        }
                    }
                    _ => return $default,
                };
                let invoker_addr = $invoker_static.get();
                if invoker_addr == 0 {
                    // Per-kind invoker not registered — fall back to the
                    // generic invoker for hosts that wired up only the
                    // single `AzApp_setGenericInvoker` slot (or for custom
                    // user-defined kinds emitted by a downstream
                    // `impl_managed_callback!` whose host hasn't shipped a
                    // per-kind invoker setter yet).
                    let generic_addr = $crate::host_invoker::GENERIC_INVOKER.get();
                    if generic_addr == 0 {
                        return $default;
                    }
                    // SAFETY: GENERIC_INVOKER only ever holds an address that
                    // came from `invoker as usize` in `AzApp_setGenericInvoker`,
                    // whose parameter is typed as `AzGenericInvoker`.
                    let generic: $crate::host_invoker::AzGenericInvoker =
                        unsafe { core::mem::transmute(generic_addr) };

                    // Build the args array: pointers to each by-value frame
                    // arg, in declared order (data, info, extras…). Lifetime
                    // is the scope of this thunk; the host MUST NOT retain
                    // these pointers past the call. Array size is inferred
                    // (2 base args + however many extras the macro forwarded).
                    let args = [
                        &raw const data as *const core::ffi::c_void,
                        &raw const info as *const core::ffi::c_void,
                        $( & $extra_name as *const _ as *const core::ffi::c_void , )*
                    ];

                    let mut out: $ret = $default;
                    generic(
                        handle,
                        KIND_STR.as_ptr() as *const core::ffi::c_char,
                        args.as_ptr(),
                        args.len(),
                        &raw mut out as *mut core::ffi::c_void,
                    );
                    return out;
                }
                // SAFETY: $invoker_static only ever holds a value that came from
                // `invoker as usize` in `$setter_fn`, where `invoker` has type
                // `$invoker_ty`.
                let invoker: $invoker_ty = unsafe { core::mem::transmute(invoker_addr) };

                // Pre-fill `out` with the kind's default so a host that fails
                // to write to the out-pointer (e.g. a buggy invoker) leaves us
                // with a sane value rather than uninitialized memory.
                let mut out: $ret = $default;
                invoker(
                    handle,
                    &raw const data,
                    &raw const info,
                    $( & $extra_name as *const $extra_ty , )*
                    &raw mut out,
                );
                out
            };

            #[cfg(feature = "std")]
            {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(body))
                    .unwrap_or($default)
            }
            #[cfg(not(feature = "std"))]
            {
                body()
            }
        }

        impl $wrapper {
            /// Build a wrapper whose `cb` is the static thunk above and
            /// whose `ctx` carries the host's `u64` handle. The host
            /// language is responsible for keeping its id→callable table
            /// in sync with the releaser registered via
            /// `AzApp_setHostHandleReleaser`.
            #[must_use] pub fn create_from_host_handle(handle: u64) -> Self {
                Self {
                    cb: $thunk_fn,
                    ctx: $crate::refany::OptionRefAny::Some(
                        $crate::host_invoker::host_handle_to_refany(handle),
                    ),
                }
            }
        }

        /// C-ABI export wrapping `<Wrapper>::create_from_host_handle`.
        #[no_mangle]
        pub extern "C" fn $from_handle_fn(handle: u64) -> $wrapper {
            <$wrapper>::create_from_host_handle(handle)
        }
    };
}

// NOTE on Miri coverage: the *genuine* FFI transmutes here (a raw host fn
// pointer stored as `usize` in an `InvokerSlot`, transmuted back to a fn
// pointer) cannot be driven from real C under Miri. Instead the tests below
// register real Rust `extern "C"` fns through the public C-ABI setters, so the
// `set(ptr as usize)` -> `get()` -> `transmute` round-trip is exercised
// end-to-end with a live pointer (Miri-clean, no UB). The panic-containment
// test drives the macro-generated thunk's `catch_unwind` with a pure-Rust
// panic raised *inside* the thunk body (before any extern-"C" boundary), which
// is the realistic containment path.
#[cfg(all(test, feature = "std"))]
#[allow(clippy::items_after_statements, clippy::redundant_clone, clippy::cast_possible_truncation, clippy::cast_sign_loss, trivial_casts, clippy::borrow_as_ptr, clippy::cast_ptr_alignment, clippy::unused_self, unused_qualifications, unreachable_pub, private_interfaces)] // test-only fakes drive the FFI macro; pedantic lints are noise here
mod tests {
    use core::sync::atomic::{AtomicU64, Ordering as AtOrdering};
    use std::sync::Mutex;

    use super::*;

    // The invoker/releaser slots are process-global; serialize tests that
    // touch them so parallel test threads don't clobber each other.
    // `pub(super)` so `autotest_generated` below locks the SAME mutex — a
    // second, independent lock would not serialize the two modules against
    // each other.
    pub(super) static TEST_LOCK: Mutex<()> = Mutex::new(());

    // Records the id the releaser was called with, so we can assert the
    // transmuted-back fn pointer was invoked with the correct payload id.
    static LAST_RELEASED: AtomicU64 = AtomicU64::new(0);

    extern "C" fn recording_releaser(id: u64) {
        LAST_RELEASED.store(id, AtOrdering::SeqCst);
    }

    #[test]
    fn destructor_transmutes_and_invokes_releaser() {
        let _g = TEST_LOCK.lock().unwrap();
        LAST_RELEASED.store(0, AtOrdering::SeqCst);
        // Register via the real C-ABI setter (exercises `releaser as usize`).
        AzApp_setHostHandleReleaser(recording_releaser);
        let mut payload = HostHandlePayload { id: 0xABCD_1234 };
        // Drive the destructor directly with a pointer to the payload — the
        // same shape a host-handle RefAny hands it. Exercises the payload
        // deref + the usize->fn-pointer transmute + the invoke.
        host_handle_destructor((&raw mut payload).cast::<c_void>());
        assert_eq!(LAST_RELEASED.load(AtOrdering::SeqCst), 0xABCD_1234);
        // Clear the slot so a later drop can't call a stale test fn pointer.
        HOST_HANDLE_RELEASER.set(0);
    }

    #[test]
    fn destructor_null_ptr_is_noop() {
        // Returns before touching any global; no lock needed.
        host_handle_destructor(core::ptr::null_mut());
    }

    #[test]
    fn host_handle_roundtrips_through_refany() {
        let _g = TEST_LOCK.lock().unwrap();
        // Ensure the round-trip RefAny's drop fires no releaser.
        HOST_HANDLE_RELEASER.set(0);
        let refany = host_handle_to_refany(0x55);
        // Exercises the type-id-guarded raw-ptr deref in refany_to_host_handle.
        assert_eq!(refany_to_host_handle(&refany), Some(0x55));
    }

    // A fake callback kind used to instantiate `impl_managed_callback!` and
    // assert the generated thunk contains a panic instead of unwinding out of
    // its `extern "C"` boundary.
    #[derive(PartialEq, Debug)]
    struct FakeRet(u32);

    struct FakeInfo;
    impl FakeInfo {
        // Panics from *inside* the thunk body (pure-Rust unwind), so the
        // thunk's `catch_unwind` is the thing under test.
        fn get_ctx(&self) -> crate::refany::OptionRefAny {
            panic!("boom from get_ctx");
        }
    }

    struct FakeWrapper {
        #[allow(dead_code)]
        cb: extern "C" fn(crate::refany::RefAny, FakeInfo) -> FakeRet,
        #[allow(dead_code)]
        ctx: crate::refany::OptionRefAny,
    }

    crate::impl_managed_callback! {
        wrapper:        FakeWrapper,
        info_ty:        FakeInfo,
        return_ty:      FakeRet,
        default_ret:    FakeRet(99),
        invoker_static: AZ_TEST_FAKE_INVOKER,
        invoker_ty:     AzTestFakeInvoker,
        thunk_fn:       az_test_fake_thunk,
        setter_fn:      az_test_fake_set_invoker,
        from_handle_fn: az_test_fake_from_handle,
    }

    #[test]
    fn thunk_contains_panic_and_returns_default() {
        let _g = TEST_LOCK.lock().unwrap();
        HOST_HANDLE_RELEASER.set(0);
        let data = host_handle_to_refany(1);
        // get_ctx() panics inside the thunk body; catch_unwind must contain it
        // and hand back `default_ret` rather than unwinding across FFI.
        let out = az_test_fake_thunk(data, FakeInfo);
        assert_eq!(out, FakeRet(99));
    }
}

/// Adversarial tests for the host-invoker registry.
///
/// Everything here that touches `HOST_HANDLE_RELEASER` / `GENERIC_INVOKER` /
/// the per-kind slot holds [`tests::TEST_LOCK`] — those slots are
/// process-global, so a parallel test thread would otherwise observe (or
/// clobber) another test's registration.
///
/// Deliberately NOT tested: a host releaser / host invoker that panics. Those
/// are `extern "C" fn`s, so Rust's abort-on-unwind shim fires *inside the
/// callee*, before the caller's `catch_unwind` can see the payload — such a
/// test would abort the whole test binary rather than assert anything. The
/// realistic containment path (a panic raised inside the thunk body, before
/// the FFI boundary) is already covered by
/// `tests::thunk_contains_panic_and_returns_default`.
#[cfg(all(test, feature = "std"))]
#[allow(
    clippy::items_after_statements,
    clippy::redundant_clone,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    trivial_casts,
    clippy::borrow_as_ptr,
    clippy::cast_ptr_alignment,
    clippy::unused_self,
    unused_qualifications,
    unreachable_pub,
    private_interfaces,
    improper_ctypes_definitions,
    missing_debug_implementations,
    missing_copy_implementations
)] // test-only fakes drive the FFI macro; pedantic lints are noise here
mod autotest_generated {
    use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering as AtOrdering};
    use std::{ffi::CStr, sync::PoisonError};

    use super::{tests::TEST_LOCK, *};
    use crate::refany::OptionRefAny;

    /// Lock the shared slot mutex, tolerating poisoning from an earlier failed
    /// test (otherwise one genuine failure cascades into N spurious ones).
    fn lock_slots() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Zero every process-global slot this module touches, so a test that
    /// asserts "unregistered" behaviour can't be fooled by a leftover pointer.
    fn clear_all_slots() {
        HOST_HANDLE_RELEASER.set(0);
        GENERIC_INVOKER.set(0);
        AZ_AUTOTEST_INVOKER.set(0);
    }

    /// Ids chosen to bracket every interesting `u64` boundary: the "no value"
    /// sentinel, the low/high extremes, the 32-bit rollover (managed hosts
    /// love to truncate to i32/f64), the sign bit, and the RTTI id itself.
    const BOUNDARY_IDS: [u64; 11] = [
        0,
        1,
        2,
        u32::MAX as u64,
        u32::MAX as u64 + 1, // 32-bit rollover: a host truncating to u32 wraps to 0
        0xDEAD_BEEF_CAFE_BABE,
        1 << 63,
        (1 << 53) + 1, // > f64 mantissa: a JS/Lua host would round this
        u64::MAX - 1,
        u64::MAX,
        AZ_HOST_HANDLE_RTTI_ID,
    ];

    // ---------------------------------------------------------------------
    // InvokerSlot — constructor / numeric set / getter
    // ---------------------------------------------------------------------

    #[test]
    fn slot_new_and_default_start_unregistered() {
        assert_eq!(InvokerSlot::new().get(), 0);
        assert_eq!(InvokerSlot::default().get(), 0);
    }

    #[test]
    fn slot_new_is_const_usable_in_static() {
        // The whole point of `const fn new()` — `impl_managed_callback!`
        // declares per-kind slots as `static`.
        static SLOT: InvokerSlot = InvokerSlot::new();
        assert_eq!(SLOT.get(), 0);
        SLOT.set(0x1234);
        assert_eq!(SLOT.get(), 0x1234);
        SLOT.set(0); // leave the process-global-shaped static clean
    }

    #[test]
    fn slot_set_get_roundtrips_at_usize_boundaries() {
        let slot = InvokerSlot::new();
        for ptr in [
            0usize,
            1,
            2,
            usize::MAX,
            usize::MAX - 1,
            usize::MAX / 2,
            1usize << (usize::BITS - 1), // sign bit, if reinterpreted as isize
            usize::try_from(u32::MAX).unwrap(),
            0xDEAD_BEEF,
        ] {
            slot.set(ptr);
            assert_eq!(slot.get(), ptr, "set/get must round-trip {ptr:#x} exactly");
        }
    }

    #[test]
    fn slot_set_is_last_write_wins_and_zero_clears() {
        let slot = InvokerSlot::new();
        slot.set(usize::MAX);
        slot.set(0x42);
        assert_eq!(slot.get(), 0x42);
        // `0` is the "unregistered" sentinel — setting it back must actually
        // un-register, not be treated as a no-op.
        slot.set(0);
        assert_eq!(slot.get(), 0);
    }

    #[test]
    fn slot_get_is_idempotent() {
        // `get` is a load, not a take: reading must not clear the slot.
        let slot = InvokerSlot::new();
        slot.set(0xABCD);
        assert_eq!(slot.get(), 0xABCD);
        assert_eq!(slot.get(), 0xABCD);
        assert_eq!(slot.get(), 0xABCD);
    }

    #[test]
    fn slot_concurrent_writes_never_tear() {
        // The slot is read on every callback fire while a host may be swapping
        // invokers. A torn read would transmute into a wild fn pointer, so
        // assert every observed value is one that was actually written.
        let slot = InvokerSlot::new();
        let written: [usize; 4] = [0, 1, usize::MAX, 1usize << (usize::BITS - 1)];
        let slot_ref = &slot;
        std::thread::scope(|s| {
            for &w in &written {
                // `move` copies `w`/`written` (both Copy) and the &-borrow of
                // `slot`; a borrowing closure would capture the loop-local `w`,
                // which does not outlive the scope.
                s.spawn(move || {
                    for _ in 0..200 {
                        slot_ref.set(w);
                        let seen = slot_ref.get();
                        assert!(
                            written.contains(&seen),
                            "torn/garbage value observed in slot: {seen:#x}"
                        );
                    }
                });
            }
        });
        assert!(written.contains(&slot.get()));
    }

    // ---------------------------------------------------------------------
    // Layout / RTTI invariants the FFI contract depends on
    // ---------------------------------------------------------------------

    #[test]
    fn rtti_id_matches_documented_constant() {
        // Hosts hard-code this value in their bindings; changing it silently
        // would make every previously-built host handle unrecognisable.
        assert_eq!(AZ_HOST_HANDLE_RTTI_ID, 0xA20A_4853_5448_5F44);
        assert_ne!(AZ_HOST_HANDLE_RTTI_ID, 0);
    }

    #[test]
    fn host_handle_payload_layout_is_a_bare_u64() {
        assert_eq!(size_of::<HostHandlePayload>(), size_of::<u64>());
        assert_eq!(align_of::<HostHandlePayload>(), align_of::<u64>());
    }

    // ---------------------------------------------------------------------
    // host_handle_to_refany / refany_to_host_handle — round-trip + rejection
    // ---------------------------------------------------------------------

    #[test]
    fn host_handle_roundtrips_at_every_u64_boundary() {
        let _g = lock_slots();
        clear_all_slots(); // no releaser: these RefAnys drop into a no-op
        for id in BOUNDARY_IDS {
            let refany = host_handle_to_refany(id);
            assert_eq!(
                refany_to_host_handle(&refany),
                Some(id),
                "encode/decode must be lossless for id {id:#x}"
            );
        }
    }

    #[test]
    fn host_handle_refany_carries_the_expected_rtti_metadata() {
        let _g = lock_slots();
        clear_all_slots();
        let refany = host_handle_to_refany(9);
        assert!(refany.is_type(AZ_HOST_HANDLE_RTTI_ID));
        assert_eq!(refany.get_type_id(), AZ_HOST_HANDLE_RTTI_ID);
        assert_eq!(refany.get_type_name().as_str(), "AzHostHandle");
        assert_eq!(refany.get_data_len(), size_of::<HostHandlePayload>());
        assert_eq!(refany.get_ref_count(), 1);
        assert!(!refany.get_data_ptr().is_null());
    }

    #[test]
    fn host_handle_id_survives_cloning() {
        let _g = lock_slots();
        clear_all_slots();
        let refany = host_handle_to_refany(u64::MAX);
        let clone = refany.clone();
        assert_eq!(refany.get_ref_count(), 2);
        assert_eq!(refany_to_host_handle(&clone), Some(u64::MAX));
        assert_eq!(refany_to_host_handle(&refany), Some(u64::MAX));
    }

    #[test]
    fn refany_to_host_handle_rejects_foreign_refanys() {
        // A stray ctx must decode as None (-> thunk returns its default),
        // never as random bytes reinterpreted as an id.
        assert_eq!(refany_to_host_handle(&RefAny::new(0u64)), None);
        assert_eq!(refany_to_host_handle(&RefAny::new(u64::MAX)), None);
        assert_eq!(refany_to_host_handle(&RefAny::new(())), None);
        assert_eq!(refany_to_host_handle(&RefAny::new([0xFFu8; 64])), None);
        // Same *payload type*, but built through RefAny::new -> TypeId-derived
        // id, not the host RTTI id. Layout-compatible but must still be
        // rejected: the guard is the id, not the shape.
        let same_shape = RefAny::new(HostHandlePayload { id: 0x1111 });
        assert_ne!(same_shape.get_type_id(), AZ_HOST_HANDLE_RTTI_ID);
        assert_eq!(refany_to_host_handle(&same_shape), None);
    }

    extern "C" fn noop_destructor(_ptr: *mut c_void) {}

    #[test]
    fn refany_to_host_handle_trusts_the_rtti_id_alone() {
        // Pins the documented hazard on AZ_HOST_HANDLE_RTTI_ID: a host that
        // reuses the id for its own (layout-compatible) payload gets its bytes
        // read back as a handle. If this ever starts returning None, the guard
        // grew a second check and the doc comment needs updating.
        let _g = lock_slots();
        clear_all_slots();
        let payload = HostHandlePayload {
            id: 0x1234_5678_9ABC_DEF0,
        };
        let spoofed = RefAny::new_c(
            (&raw const payload).cast::<c_void>(),
            size_of::<HostHandlePayload>(),
            align_of::<HostHandlePayload>(),
            AZ_HOST_HANDLE_RTTI_ID,
            "NotAHostHandle".into(),
            noop_destructor,
            0,
            0,
        );
        assert_eq!(refany_to_host_handle(&spoofed), Some(0x1234_5678_9ABC_DEF0));
    }

    // ---------------------------------------------------------------------
    // C-ABI surface: AzRefAny_newHostHandle / AzRefAny_getHostHandle
    // ---------------------------------------------------------------------

    #[test]
    fn c_abi_new_and_get_host_handle_roundtrip() {
        let _g = lock_slots();
        clear_all_slots();
        for id in BOUNDARY_IDS {
            let refany = AzRefAny_newHostHandle(id);
            assert_eq!(refany_to_host_handle(&refany), Some(id));
            assert_eq!(AzRefAny_getHostHandle(&raw const refany), id);
        }
    }

    #[test]
    fn c_abi_get_host_handle_null_returns_zero() {
        assert_eq!(AzRefAny_getHostHandle(core::ptr::null()), 0);
    }

    #[test]
    fn c_abi_get_host_handle_foreign_refany_returns_zero() {
        let foreign = RefAny::new(0xDEAD_BEEF_u64);
        assert_eq!(AzRefAny_getHostHandle(&raw const foreign), 0);
    }

    #[test]
    fn c_abi_get_host_handle_cannot_distinguish_id_zero_from_failure() {
        // Documented contract: `0` is reserved as "no value", so a host whose
        // id allocator starts at 0 gets an unfixable ambiguity across the C
        // ABI. Assert the ambiguity exists (so nobody "fixes" getHostHandle
        // without also fixing the bindings) AND that the Rust-side accessor
        // stays lossless.
        let _g = lock_slots();
        clear_all_slots();
        let zero_handle = AzRefAny_newHostHandle(0);
        assert_eq!(AzRefAny_getHostHandle(&raw const zero_handle), 0);
        assert_eq!(AzRefAny_getHostHandle(core::ptr::null()), 0);
        // Rust callers can still tell the two apart:
        assert_eq!(refany_to_host_handle(&zero_handle), Some(0));
    }

    // ---------------------------------------------------------------------
    // Releaser registration + destructor firing
    // ---------------------------------------------------------------------

    static RELEASE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static RELEASED_ID: AtomicU64 = AtomicU64::new(0);

    extern "C" fn counting_releaser(id: u64) {
        RELEASED_ID.store(id, AtOrdering::SeqCst);
        RELEASE_COUNT.fetch_add(1, AtOrdering::SeqCst);
    }

    static OTHER_RELEASE_COUNT: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn other_releaser(_id: u64) {
        OTHER_RELEASE_COUNT.fetch_add(1, AtOrdering::SeqCst);
    }

    fn reset_release_recorder() {
        RELEASE_COUNT.store(0, AtOrdering::SeqCst);
        RELEASED_ID.store(0, AtOrdering::SeqCst);
        OTHER_RELEASE_COUNT.store(0, AtOrdering::SeqCst);
    }

    #[test]
    fn set_releaser_stores_the_fn_address_and_replaces_it() {
        let _g = lock_slots();
        clear_all_slots();
        let expected: extern "C" fn(u64) = counting_releaser;
        AzApp_setHostHandleReleaser(counting_releaser);
        assert_eq!(HOST_HANDLE_RELEASER.get(), expected as usize);
        assert_ne!(HOST_HANDLE_RELEASER.get(), 0);
        // "subsequent registrations replace the previous slot"
        let replacement: extern "C" fn(u64) = other_releaser;
        AzApp_setHostHandleReleaser(other_releaser);
        assert_eq!(HOST_HANDLE_RELEASER.get(), replacement as usize);
        clear_all_slots();
    }

    #[test]
    fn releaser_fires_exactly_once_on_the_last_drop() {
        let _g = lock_slots();
        clear_all_slots();
        reset_release_recorder();
        AzApp_setHostHandleReleaser(counting_releaser);

        let refany = host_handle_to_refany(0xABC_DEF);
        let clone_a = refany.clone();
        let clone_b = refany.clone();

        drop(clone_a);
        drop(clone_b);
        // Two of three refs gone — the host's table entry must still be alive.
        assert_eq!(RELEASE_COUNT.load(AtOrdering::SeqCst), 0);

        drop(refany);
        assert_eq!(RELEASE_COUNT.load(AtOrdering::SeqCst), 1);
        assert_eq!(RELEASED_ID.load(AtOrdering::SeqCst), 0xABC_DEF);

        clear_all_slots();
    }

    #[test]
    fn releaser_receives_boundary_ids_verbatim() {
        let _g = lock_slots();
        clear_all_slots();
        reset_release_recorder();
        AzApp_setHostHandleReleaser(counting_releaser);

        for (n, id) in BOUNDARY_IDS.into_iter().enumerate() {
            RELEASED_ID.store(0, AtOrdering::SeqCst);
            drop(host_handle_to_refany(id));
            assert_eq!(
                RELEASE_COUNT.load(AtOrdering::SeqCst),
                n + 1,
                "one release per dropped handle"
            );
            assert_eq!(
                RELEASED_ID.load(AtOrdering::SeqCst),
                id,
                "releaser must see id {id:#x} unmangled (no truncation/saturation)"
            );
        }

        clear_all_slots();
    }

    #[test]
    fn dropping_a_handle_with_no_releaser_registered_is_a_noop() {
        let _g = lock_slots();
        clear_all_slots();
        reset_release_recorder();
        // Slot is 0 ("host hasn't initialised yet") — the destructor must bail
        // rather than transmute 0 into a fn pointer and jump to it.
        drop(host_handle_to_refany(1));
        drop(host_handle_to_refany(u64::MAX));
        assert_eq!(RELEASE_COUNT.load(AtOrdering::SeqCst), 0);
        assert_eq!(HOST_HANDLE_RELEASER.get(), 0);
    }

    #[test]
    fn re_registering_the_releaser_retires_the_old_one() {
        let _g = lock_slots();
        clear_all_slots();
        reset_release_recorder();
        AzApp_setHostHandleReleaser(counting_releaser);
        let live = host_handle_to_refany(7);
        // Host swaps releasers (e.g. module reload) while a handle is alive:
        // the *current* slot wins at drop time, not the one in force at
        // construction.
        AzApp_setHostHandleReleaser(other_releaser);
        drop(live);
        assert_eq!(RELEASE_COUNT.load(AtOrdering::SeqCst), 0);
        assert_eq!(OTHER_RELEASE_COUNT.load(AtOrdering::SeqCst), 1);
        clear_all_slots();
    }

    #[test]
    fn destructor_on_null_payload_is_a_noop_even_with_a_releaser() {
        let _g = lock_slots();
        clear_all_slots();
        reset_release_recorder();
        AzApp_setHostHandleReleaser(counting_releaser);
        host_handle_destructor(core::ptr::null_mut());
        assert_eq!(
            RELEASE_COUNT.load(AtOrdering::SeqCst),
            0,
            "a null payload must not be deref'd, nor reported as id 0"
        );
        clear_all_slots();
    }

    // ---------------------------------------------------------------------
    // A fake callback kind, so the generic/per-kind dispatch paths in
    // `impl_managed_callback!` can be driven end-to-end.
    // ---------------------------------------------------------------------

    #[repr(C)]
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    struct AutoRet(u32);

    const DEFAULT_RET: AutoRet = AutoRet(0xDEAD);

    #[repr(C)]
    #[derive(Debug)]
    struct AutoInfo {
        ctx: OptionRefAny,
    }

    impl AutoInfo {
        fn get_ctx(&self) -> OptionRefAny {
            self.ctx.clone()
        }
    }

    #[repr(C)]
    #[derive(Debug)]
    struct AutoWrapper {
        cb: extern "C" fn(RefAny, AutoInfo) -> AutoRet,
        ctx: OptionRefAny,
    }

    crate::impl_managed_callback! {
        wrapper:        AutoWrapper,
        info_ty:        AutoInfo,
        return_ty:      AutoRet,
        default_ret:    DEFAULT_RET,
        invoker_static: AZ_AUTOTEST_INVOKER,
        invoker_ty:     AzAutotestInvoker,
        thunk_fn:       az_autotest_thunk,
        setter_fn:      az_autotest_set_invoker,
        from_handle_fn: az_autotest_from_handle,
    }

    // What the fake host invokers saw. Recorded into atomics rather than
    // asserted in-place: these fns are `extern "C"`, so a failing assert!
    // inside one would abort the test binary instead of failing the test.
    static GENERIC_CALLS: AtomicUsize = AtomicUsize::new(0);
    static GENERIC_HANDLE: AtomicU64 = AtomicU64::new(0);
    static GENERIC_NARGS: AtomicUsize = AtomicUsize::new(0);
    static GENERIC_KIND_OK: AtomicBool = AtomicBool::new(false);
    static GENERIC_ARG0_ID: AtomicU64 = AtomicU64::new(0);
    static PERKIND_CALLS: AtomicUsize = AtomicUsize::new(0);
    static PERKIND_HANDLE: AtomicU64 = AtomicU64::new(0);

    fn reset_invoker_recorders() {
        GENERIC_CALLS.store(0, AtOrdering::SeqCst);
        GENERIC_HANDLE.store(0, AtOrdering::SeqCst);
        GENERIC_NARGS.store(0, AtOrdering::SeqCst);
        GENERIC_KIND_OK.store(false, AtOrdering::SeqCst);
        GENERIC_ARG0_ID.store(0, AtOrdering::SeqCst);
        PERKIND_CALLS.store(0, AtOrdering::SeqCst);
        PERKIND_HANDLE.store(0, AtOrdering::SeqCst);
    }

    /// Stand-in for a host's libffi generic-invoker closure.
    extern "C" fn recording_generic(
        handle: u64,
        kind: *const core::ffi::c_char,
        args: *const *const c_void,
        n_args: usize,
        ret: *mut c_void,
    ) {
        GENERIC_CALLS.fetch_add(1, AtOrdering::SeqCst);
        GENERIC_HANDLE.store(handle, AtOrdering::SeqCst);
        GENERIC_NARGS.store(n_args, AtOrdering::SeqCst);

        // The kind string must be a NUL-terminated "AutoWrapper" — that's what
        // the host's dispatch table keys on. Also gates the `ret` write below:
        // another kind's thunk falling back here would have a differently-sized
        // out-slot.
        let kind_ok = !kind.is_null()
            && unsafe { CStr::from_ptr(kind) }.to_str() == Ok("AutoWrapper");
        GENERIC_KIND_OK.store(kind_ok, AtOrdering::SeqCst);

        // args[0] is the by-value `data: RefAny` frame slot, args[1] the info.
        if !args.is_null() && n_args == 2 {
            let arg0 = unsafe { *args };
            if !arg0.is_null() {
                let data = unsafe { &*(arg0.cast::<RefAny>()) };
                GENERIC_ARG0_ID.store(
                    refany_to_host_handle(data).unwrap_or(0),
                    AtOrdering::SeqCst,
                );
            }
        }

        if kind_ok && !ret.is_null() {
            unsafe { ret.cast::<AutoRet>().write(AutoRet(0x2222)) };
        }
    }

    /// Stand-in for a host's per-kind libffi closure.
    extern "C" fn recording_perkind(
        handle: u64,
        _data: *const RefAny,
        _info: *const AutoInfo,
        out: *mut AutoRet,
    ) {
        PERKIND_CALLS.fetch_add(1, AtOrdering::SeqCst);
        PERKIND_HANDLE.store(handle, AtOrdering::SeqCst);
        if !out.is_null() {
            unsafe { out.write(AutoRet(0x1111)) };
        }
    }

    /// A buggy host invoker: never writes the out-pointer.
    extern "C" fn silent_perkind(
        _handle: u64,
        _data: *const RefAny,
        _info: *const AutoInfo,
        _out: *mut AutoRet,
    ) {
        PERKIND_CALLS.fetch_add(1, AtOrdering::SeqCst);
    }

    fn info_with_ctx(ctx: OptionRefAny) -> AutoInfo {
        AutoInfo { ctx }
    }

    #[test]
    fn set_generic_invoker_stores_the_fn_address_and_replaces_it() {
        let _g = lock_slots();
        clear_all_slots();
        let expected: AzGenericInvoker = recording_generic;
        AzApp_setGenericInvoker(recording_generic);
        assert_eq!(GENERIC_INVOKER.get(), expected as usize);
        assert_ne!(GENERIC_INVOKER.get(), 0);
        AzApp_setGenericInvoker(recording_generic); // idempotent re-register
        assert_eq!(GENERIC_INVOKER.get(), expected as usize);
        clear_all_slots();
    }

    #[test]
    fn create_from_host_handle_wires_the_thunk_and_ctx() {
        let _g = lock_slots();
        clear_all_slots();
        for id in BOUNDARY_IDS {
            let wrapper = AutoWrapper::create_from_host_handle(id);
            let expected: extern "C" fn(RefAny, AutoInfo) -> AutoRet = az_autotest_thunk;
            assert_eq!(wrapper.cb as usize, expected as usize);
            match &wrapper.ctx {
                OptionRefAny::Some(refany) => {
                    assert_eq!(refany_to_host_handle(refany), Some(id));
                }
                OptionRefAny::None => panic!("ctx must carry the host handle for id {id:#x}"),
            }
            // The C-ABI export must produce the identical wrapper.
            let from_c = az_autotest_from_handle(id);
            assert_eq!(from_c.cb as usize, expected as usize);
        }
    }

    #[test]
    fn thunk_returns_default_when_ctx_is_none() {
        let _g = lock_slots();
        clear_all_slots();
        reset_invoker_recorders();
        AzApp_setGenericInvoker(recording_generic);
        az_autotest_set_invoker(recording_perkind);

        // Framework invoked the typedef directly, without a host ctx: neither
        // invoker may fire (there is no handle to dispatch on).
        let out = az_autotest_thunk(RefAny::new(1u32), info_with_ctx(OptionRefAny::None));
        assert_eq!(out, DEFAULT_RET);
        assert_eq!(GENERIC_CALLS.load(AtOrdering::SeqCst), 0);
        assert_eq!(PERKIND_CALLS.load(AtOrdering::SeqCst), 0);
        clear_all_slots();
    }

    #[test]
    fn thunk_returns_default_when_ctx_is_not_a_host_handle() {
        let _g = lock_slots();
        clear_all_slots();
        reset_invoker_recorders();
        AzApp_setGenericInvoker(recording_generic);
        az_autotest_set_invoker(recording_perkind);

        // A foreign ctx must NOT be reinterpreted as a handle — that would
        // dispatch the host on a garbage id.
        let ctx = OptionRefAny::Some(RefAny::new(0xFFFF_FFFF_FFFF_FFFF_u64));
        let out = az_autotest_thunk(RefAny::new(1u32), info_with_ctx(ctx));
        assert_eq!(out, DEFAULT_RET);
        assert_eq!(GENERIC_CALLS.load(AtOrdering::SeqCst), 0);
        assert_eq!(PERKIND_CALLS.load(AtOrdering::SeqCst), 0);
        clear_all_slots();
    }

    #[test]
    fn thunk_returns_default_when_nothing_is_registered() {
        let _g = lock_slots();
        clear_all_slots();
        // Valid host handle, but both slots are 0 — the thunk must bail with
        // the default instead of transmuting 0 into a fn pointer.
        let ctx = OptionRefAny::Some(host_handle_to_refany(5));
        let out = az_autotest_thunk(RefAny::new(1u32), info_with_ctx(ctx));
        assert_eq!(out, DEFAULT_RET);
    }

    #[test]
    fn thunk_falls_back_to_the_generic_invoker() {
        let _g = lock_slots();
        clear_all_slots();
        reset_invoker_recorders();
        AzApp_setGenericInvoker(recording_generic);
        // AZ_AUTOTEST_INVOKER deliberately left at 0.

        let data = host_handle_to_refany(0xDA7A);
        let ctx = OptionRefAny::Some(host_handle_to_refany(0xC7_C7_C7));
        let out = az_autotest_thunk(data, info_with_ctx(ctx));

        assert_eq!(GENERIC_CALLS.load(AtOrdering::SeqCst), 1);
        assert_eq!(GENERIC_HANDLE.load(AtOrdering::SeqCst), 0xC7_C7_C7);
        assert!(GENERIC_KIND_OK.load(AtOrdering::SeqCst), "kind must be \"AutoWrapper\\0\"");
        assert_eq!(GENERIC_NARGS.load(AtOrdering::SeqCst), 2, "data + info");
        // args[] must be in *declared* order: data first, then info.
        assert_eq!(GENERIC_ARG0_ID.load(AtOrdering::SeqCst), 0xDA7A);
        // ...and the host's out-pointer write must be what the thunk returns.
        assert_eq!(out, AutoRet(0x2222));
        clear_all_slots();
    }

    #[test]
    fn thunk_prefers_the_per_kind_invoker_over_the_generic_one() {
        let _g = lock_slots();
        clear_all_slots();
        reset_invoker_recorders();
        AzApp_setGenericInvoker(recording_generic);
        az_autotest_set_invoker(recording_perkind);

        let ctx = OptionRefAny::Some(host_handle_to_refany(0x99));
        let out = az_autotest_thunk(RefAny::new(1u32), info_with_ctx(ctx));

        assert_eq!(out, AutoRet(0x1111));
        assert_eq!(PERKIND_CALLS.load(AtOrdering::SeqCst), 1);
        assert_eq!(PERKIND_HANDLE.load(AtOrdering::SeqCst), 0x99);
        assert_eq!(
            GENERIC_CALLS.load(AtOrdering::SeqCst),
            0,
            "generic is a fallback only — it must not also fire"
        );
        clear_all_slots();
    }

    #[test]
    fn thunk_returns_default_when_the_host_ignores_the_out_pointer() {
        let _g = lock_slots();
        clear_all_slots();
        reset_invoker_recorders();
        az_autotest_set_invoker(silent_perkind);

        // A buggy host invoker that never writes `out` must leave us with the
        // pre-filled default, not uninitialised memory.
        let ctx = OptionRefAny::Some(host_handle_to_refany(3));
        let out = az_autotest_thunk(RefAny::new(1u32), info_with_ctx(ctx));
        assert_eq!(PERKIND_CALLS.load(AtOrdering::SeqCst), 1);
        assert_eq!(out, DEFAULT_RET);
        clear_all_slots();
    }

    #[test]
    fn thunk_dispatches_boundary_handles_without_truncation() {
        let _g = lock_slots();
        clear_all_slots();
        reset_invoker_recorders();
        az_autotest_set_invoker(recording_perkind);

        for id in BOUNDARY_IDS {
            let ctx = OptionRefAny::Some(host_handle_to_refany(id));
            let out = az_autotest_thunk(RefAny::new(1u32), info_with_ctx(ctx));
            assert_eq!(out, AutoRet(0x1111));
            assert_eq!(
                PERKIND_HANDLE.load(AtOrdering::SeqCst),
                id,
                "handle {id:#x} must reach the host invoker unmangled"
            );
        }
        clear_all_slots();
    }
}
