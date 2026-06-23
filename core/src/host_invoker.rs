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
pub struct HostHandlePayload {
    pub id: u64,
}

/// A single atomic-pointer slot for one registered host-side function
/// pointer. `0` means "not registered"; the static thunks bail out (returning
/// the kind's default value) when they see an unregistered slot rather than
/// transmuting `0` into a fn pointer and crashing.
#[repr(C)]
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

/// Process-global slot for the host's *generic* invoker. Set via
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
    releaser(payload.id);
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
/// via [`host_handle_to_refany`]. Returns `None` for any other `RefAny`, so
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

/// C-ABI: build a [`RefAny`] wrapping a host-language id. Lets managed-FFI
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
pub extern "C" fn AzRefAny_getHostHandle(refany: *const RefAny) -> u64 {
    if refany.is_null() {
        return 0;
    }
    // SAFETY: caller's responsibility per `*const` signature.
    let r = unsafe { &*refany };
    refany_to_host_handle(r).unwrap_or(0)
}

/// Macro that expands to the per-callback-kind boilerplate: a static thunk
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
        /// LuaJIT FFI in particular cannot return aggregates larger than
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

                // Wrapper name as a null-terminated C string. `stringify!`
                // expands `$wrapper:ty` to e.g. `Callback`,
                // `ButtonOnClickCallback`, etc. — matching what the host's
                // dispatch table keys on.
                const KIND_STR: &str = concat!(stringify!($wrapper), "\0");

                // Build the args array: pointers to each by-value frame
                // arg, in declared order (data, info, extras…). Lifetime
                // is the scope of this thunk; the host MUST NOT retain
                // these pointers past the call. Array size is inferred
                // (2 base args + however many extras the macro forwarded).
                let args = [
                    &data as *const _ as *const core::ffi::c_void,
                    &info as *const _ as *const core::ffi::c_void,
                    $( & $extra_name as *const _ as *const core::ffi::c_void , )*
                ];

                let mut out: $ret = $default;
                generic(
                    handle,
                    KIND_STR.as_ptr() as *const core::ffi::c_char,
                    args.as_ptr(),
                    args.len(),
                    &mut out as *mut _ as *mut core::ffi::c_void,
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
                &data as *const $crate::refany::RefAny,
                &info as *const $info_ty,
                $( & $extra_name as *const $extra_ty , )*
                &mut out as *mut $ret,
            );
            out
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
