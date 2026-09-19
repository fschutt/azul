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
//! - [`AzApp_setHostHandleReleaser`] — register the host's "drop this id" callback once per
//!   process. Fires when a host-handle [`RefAny`] is collected.
//! - Per callback kind, [`crate::impl_managed_callback!`] expands to:
//!   - A static thunk (`extern "C" fn`) compiled into libazul.
//!   - A `<Wrapper>::create_from_host_handle(u64)` constructor.
//!   - An `AzApp_set<Kind>Invoker(...)` setter for the host-side per-kind pointer-arg invoker.
//!
//! ## Why a single shared releaser
//!
//! Per-kind invokers are necessarily distinct — each callback typedef has
//! a different signature, so the host has to register a libffi closure per
//! typedef anyway. The releaser, on the other hand, has the same signature
//! for every kind (`extern "C" fn(u64)`), so we can share one slot across
//! all callbacks; the host registers it once and every kind's destructor
//! routes through it.

use core::{
    ffi::c_void,
    sync::atomic::{AtomicUsize, Ordering},
};

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
    #[must_use]
    pub const fn new() -> Self {
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
        drop(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || releaser(payload.id),
        )));
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
#[must_use]
pub fn refany_to_host_handle(refany: &RefAny) -> Option<u64> {
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
#[allow(clippy::not_unsafe_ptr_arg_deref)] // SAFETY/FFI: `*const T` is the C-ABI signature; the fn
                                           // null-checks then derefs under the documented caller
                                           // contract (C guarantees a valid ptr/len). Marking it
                                           // `unsafe fn` would force unsafe blocks into the
                                           // generated dll bindings.
pub extern "C" fn AzRefAny_getHostHandle(refany: *const RefAny) -> u64 {
    if refany.is_null() {
        return 0;
    }
    // SAFETY: caller's responsibility per `*const` signature.
    let r = unsafe { &*refany };
    refany_to_host_handle(r).unwrap_or(0)
}

/// Implemented by every `info` type a host-invoked callback kind receives
/// (`CallbackInfo`, `LayoutCallbackInfo`, `VirtualViewCallbackInfo`,
/// `ThreadSender`, ...).
///
/// `install_host_ctx` makes a wrapper's host ctx visible to the per-kind
/// thunk (which reads `info.get_ctx()`) when the ENGINE invokes that wrapper
/// from inside another callback: widget-internal handlers (`CheckBox`'s
/// on-toggle, `TextInput`'s on-text-input, `DropDown`, `ColorInput`, ...)
/// forward the user's typed callback with the `info` they themselves
/// received, whose ctx is the internal handler's (`None`). Before this
/// existed every non-`Button` widget callback silently returned the kind's
/// default for EVERY managed language (found 2026-09-19 via the Node binding).
/// The macro-generated `<Wrapper>::invoke` installs the ctx and is the only
/// way engine code should call a managed-kind wrapper.
pub trait HostCtxCarrier {
    fn install_host_ctx(&mut self, ctx: &crate::refany::OptionRefAny);
}

/// The return type of a host-invoked callback kind, as the host writes it:
/// through an out-pointer, with a plain C copy that never drops what `out`
/// held before.
///
/// So what the thunk pre-fills `out` with must own nothing - a default that
/// owns memory (a `RefAny` clone, an image) would leak on every call the host
/// answers. The kind's own fallback (`default_ret`) is only built when the
/// host is missing, panics, or leaves `out` [`is_unwritten`](Self::is_unwritten).
pub trait HostOut: Sized {
    /// `out` before the host writes it. Must own nothing.
    fn unwritten() -> Self;
    /// Whether `out` still holds [`unwritten`](Self::unwritten), i.e. the
    /// host did not answer and the thunk returns the kind's fallback (the
    /// unwritten value is then never dropped). `false` for a type any of
    /// whose values the host may legitimately return.
    fn is_unwritten(&self) -> bool {
        false
    }
}

impl HostOut for () {
    fn unwritten() -> Self {}
}

impl HostOut for crate::callbacks::Update {
    fn unwritten() -> Self {
        Self::DoNothing
    }
}

impl HostOut for crate::callbacks::TimerCallbackReturn {
    /// A timer whose host never answers stops rather than firing forever.
    fn unwritten() -> Self {
        Self::terminate_unchanged()
    }
}

impl HostOut for crate::callbacks::VirtualViewReturn {
    fn unwritten() -> Self {
        Self::default()
    }
}

impl HostOut for crate::dom::Dom {
    /// Built from `const` empty slices: owns no heap memory.
    fn unwritten() -> Self {
        Self::create_body()
    }
}

impl HostOut for crate::geom::LogicalRect {
    /// A NaN origin: no host answers with one.
    fn unwritten() -> Self {
        Self::new(
            crate::geom::LogicalPosition::new(f32::NAN, f32::NAN),
            crate::geom::LogicalSize::zero(),
        )
    }
    fn is_unwritten(&self) -> bool {
        self.origin.x.is_nan()
    }
}

impl HostOut for crate::geom::LogicalRectVec {
    /// Empty: an unallocated vector.
    fn unwritten() -> Self {
        Self::from_const_slice(&[])
    }
    fn is_unwritten(&self) -> bool {
        self.is_empty()
    }
}

impl HostOut for AzString {
    fn unwritten() -> Self {
        Self::from_const_str("")
    }
}

impl HostOut for RefAny {
    /// A handle to nothing: a null `RefCount` never touches memory on drop.
    fn unwritten() -> Self {
        RefAny {
            sharing_info: crate::refany::RefCount {
                ptr: core::ptr::null(),
                run_destructor: false,
            },
            instance_id: 0,
        }
    }
    fn is_unwritten(&self) -> bool {
        self.sharing_info.ptr.is_null()
    }
}

impl HostOut for crate::resources::ImageRef {
    /// A handle to nothing, never dropped (`ImageRef`'s drop dereferences
    /// `copies`).
    fn unwritten() -> Self {
        Self {
            data: core::ptr::null(),
            copies: core::ptr::null(),
            id: 0,
            run_destructor: false,
        }
    }
    fn is_unwritten(&self) -> bool {
        self.data.is_null()
    }
}

impl HostOut for crate::db::DbValue {
    fn unwritten() -> Self {
        Self::Null
    }
}

impl HostOut for crate::events::CustomE2eOpResult {
    /// "Not my op", with a static empty payload.
    fn unwritten() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// The invocation slot: how a callee finds the context of the wrapper that
// invoked it when no argument of its kind carries one.
// ---------------------------------------------------------------------------

/// The data argument of a callback kind: a `RefAny` by value (every kind but
/// one) or `&mut RefAny` (`MarginBoxCallback`). Either way the host invoker
/// receives a pointer to the `RefAny` itself.
pub trait DataArg {
    fn data_ptr(&self) -> *const RefAny;
}

impl DataArg for RefAny {
    fn data_ptr(&self) -> *const RefAny {
        self
    }
}

impl DataArg for &mut RefAny {
    fn data_ptr(&self) -> *const RefAny {
        &**self
    }
}

#[cfg(feature = "std")]
std::thread_local! {
    /// The callback function libazul is invoking on this thread right now,
    /// with its wrapper's context. Set by every macro-generated
    /// `<Wrapper>::invoke`, restored when that call returns.
    static INVOCATION: core::cell::RefCell<Option<(usize, crate::refany::OptionRefAny)>> =
        const { core::cell::RefCell::new(None) };
}

/// Marks one callback invocation on this thread; dropping it restores the
/// invocation that was running before (callbacks nest).
#[must_use = "the invocation ends when this guard drops"]
#[derive(Debug)]
pub struct Invocation {
    #[cfg(feature = "std")]
    prev: Option<(usize, crate::refany::OptionRefAny)>,
}

/// Records that `callee` - the wrapper's `cb`, as a function address - is
/// being invoked with the wrapper's `ctx`, until the returned guard drops.
pub fn enter_invocation(callee: usize, ctx: &crate::refany::OptionRefAny) -> Invocation {
    #[cfg(feature = "std")]
    {
        let entry = Some((callee, ctx.clone()));
        let prev = INVOCATION.try_with(|s| s.replace(entry)).ok().flatten();
        Invocation { prev }
    }
    #[cfg(not(feature = "std"))]
    {
        let _ = (callee, ctx);
        Invocation {}
    }
}

impl Drop for Invocation {
    fn drop(&mut self) {
        #[cfg(feature = "std")]
        {
            let prev = self.prev.take();
            // The replaced entry drops after the slot is released: dropping a
            // host handle calls the host's releaser, which may run a callback.
            let current = INVOCATION.try_with(|s| s.replace(prev)).ok().flatten();
            drop(current);
        }
    }
}

/// The context of the running invocation of `callee` on this thread, or
/// `None` when the innermost invocation is of another function (a callee
/// never sees a context that was not meant for it).
#[must_use]
pub fn invocation_ctx(callee: usize) -> crate::refany::OptionRefAny {
    #[cfg(feature = "std")]
    {
        INVOCATION
            .try_with(|s| match &*s.borrow() {
                Some((c, ctx)) if *c == callee => ctx.clone(),
                _ => crate::refany::OptionRefAny::None,
            })
            .unwrap_or(crate::refany::OptionRefAny::None)
    }
    #[cfg(not(feature = "std"))]
    {
        let _ = callee;
        crate::refany::OptionRefAny::None
    }
}

/// C-ABI: the context of the wrapper whose callback `callee` libazul is
/// invoking on this thread right now. A binding's C-ABI trampoline calls it
/// with its own address to find the closure it stands for, for the callback
/// kinds whose arguments carry no context (an info type without `get_ctx`).
#[no_mangle]
pub extern "C" fn AzApp_getInvocationCtx(callee: *const c_void) -> crate::refany::OptionRefAny {
    invocation_ctx(callee as usize)
}

/// Out-pointer twin of [`AzApp_getInvocationCtx`] for FFIs that cannot
/// receive a struct by value.
///
/// # Safety
///
/// `out` must be null or valid for writing one `OptionRefAny`.
#[no_mangle]
pub unsafe extern "C" fn AzApp_getInvocationCtxByref(
    callee: *const c_void,
    out: *mut crate::refany::OptionRefAny,
) {
    if !out.is_null() {
        unsafe { core::ptr::write(out, invocation_ctx(callee as usize)) };
    }
}

/// Macro that expands to the per-callback-kind boilerplate:
///
/// a static thunk
/// (compiled into libazul) that the framework calls with by-value args, a
/// `<Wrapper>::create_from_host_handle(u64)` constructor, an
/// `AzApp_set<Kind>Invoker` setter the host calls once at module load, and
/// `<Wrapper>::invoke`, the only way engine code may call the wrapper.
///
/// All identifiers are passed in explicitly so we don't need a proc-macro
/// dependency just to concatenate idents.
///
/// The per-kind invoker receives the host handle, then EVERY argument of the
/// callback by pointer in declared order, then an out-pointer for the return
/// value - the signature `managed_host_invoker::invoker_c_arg_list` declares
/// for every binding.
///
/// Where the thunk finds the host handle (the wrapper's context):
///
/// - `info_ty` forms: in an info argument that implements [`HostCtxCarrier`]
///   and has `get_ctx()`; `invoke` installs the wrapper's context there.
/// - the `ctx_field` form, for kinds none of whose arguments can carry a
///   context: in the invocation slot ([`invocation_ctx`]) `invoke` sets.
///
/// Every `invoke` also sets the invocation slot, so a binding's own C
/// trampoline can read the context of ANY kind through
/// [`AzApp_getInvocationCtx`].
///
/// `default_ret` is returned when the context is not a host handle, no
/// invoker is registered, the host panics, or it leaves `out` unwritten (see
/// [`HostOut`]). It is only built then, so it may own memory and may read the
/// callback's arguments (the fresh dataset of a merge, the current caret
/// rectangle): a missing host degrades to "no callback", not to garbage.
#[macro_export]
macro_rules! impl_managed_callback {
    // Form 1: `(RefAny, info) -> ret` - `Callback`, `LayoutCallback`,
    // `ButtonOnClickCallback` and most widget callbacks.
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
        $( from_handle_byref_fn: $from_handle_byref_fn:ident, )?
    ) => {
        $crate::impl_managed_callback! {
            wrapper:        $wrapper,
            pre_args:       [],
            info_ty:        $info_ty,
            return_ty:      $ret,
            default_ret:    $default,
            invoker_static: $invoker_static,
            invoker_ty:     $invoker_ty,
            thunk_fn:       $thunk_fn,
            setter_fn:      $setter_fn,
            from_handle_fn: $from_handle_fn,
            $( from_handle_byref_fn: $from_handle_byref_fn, )?
            extra_args:     [],
        }
    };
    // Form 2: `(RefAny, info, extras...) -> ret` - e.g.
    // `CheckBoxOnToggleCallback(RefAny, CallbackInfo, CheckBoxState)`.
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
        $( from_handle_byref_fn: $from_handle_byref_fn:ident, )?
        extra_args:     [ $( $extra_name:ident : $extra_ty:ty ),* $(,)? ] $(,)?
    ) => {
        $crate::impl_managed_callback! {
            wrapper:        $wrapper,
            pre_args:       [],
            info_ty:        $info_ty,
            return_ty:      $ret,
            default_ret:    $default,
            invoker_static: $invoker_static,
            invoker_ty:     $invoker_ty,
            thunk_fn:       $thunk_fn,
            setter_fn:      $setter_fn,
            from_handle_fn: $from_handle_fn,
            $( from_handle_byref_fn: $from_handle_byref_fn, )?
            extra_args:     [ $( $extra_name : $extra_ty ),* ],
        }
    };
    // Form 3: `(RefAny, pre..., info, extras...) -> ret` - arguments before the
    // info, e.g. `WriteBackCallback(RefAny, RefAny, CallbackInfo)`.
    (
        wrapper:        $wrapper:ty,
        pre_args:       [ $( $pre_name:ident : $pre_ty:ty ),* $(,)? ],
        info_ty:        $info_ty:ty,
        return_ty:      $ret:ty,
        default_ret:    $default:expr,
        invoker_static: $invoker_static:ident,
        invoker_ty:     $invoker_ty:ident,
        thunk_fn:       $thunk_fn:ident,
        setter_fn:      $setter_fn:ident,
        from_handle_fn: $from_handle_fn:ident,
        $( from_handle_byref_fn: $from_handle_byref_fn:ident, )?
        extra_args:     [ $( $extra_name:ident : $extra_ty:ty ),* $(,)? ] $(,)?
    ) => {
        $crate::impl_managed_callback! {
            @expand
            wrapper:        $wrapper,
            ctx_field:      ctx,
            data:           [data: $crate::refany::RefAny],
            args:           [ $( $pre_name : $pre_ty, )* info: $info_ty $( , $extra_name : $extra_ty )* ],
            ctx_from:       [ info info ],
            return_ty:      $ret,
            default_ret:    $default,
            invoker_static: $invoker_static,
            invoker_ty:     $invoker_ty,
            thunk_fn:       $thunk_fn,
            setter_fn:      $setter_fn,
            from_handle_fn: $from_handle_fn,
            from_handle_byref_fn: [ $( $from_handle_byref_fn )? ],
            rest:           [],
        }
    };
    // Form 4: kinds none of whose arguments carry a context (`DbMergeCallback
    // (RefAny, DbConflict)`, `DatasetMergeCallback(RefAny, RefAny)`,
    // `MarginBoxCallback(&mut RefAny, PageInfo)`, ...): the thunk reads the
    // context from the invocation slot `invoke` sets. `ctx_field` is the
    // wrapper's `OptionRefAny` field; `rest` fills any further fields of
    // `create_from_host_handle`'s wrapper.
    (
        wrapper:        $wrapper:ty,
        ctx_field:      $ctx_field:ident,
        data:           $data:ident : $data_ty:ty,
        args:           [ $( $arg:ident : $arg_ty:ty ),* $(,)? ],
        return_ty:      $ret:ty,
        default_ret:    $default:expr,
        invoker_static: $invoker_static:ident,
        invoker_ty:     $invoker_ty:ident,
        thunk_fn:       $thunk_fn:ident,
        setter_fn:      $setter_fn:ident,
        from_handle_fn: $from_handle_fn:ident,
        $( from_handle_byref_fn: $from_handle_byref_fn:ident, )?
        $( rest: $rest:expr, )?
    ) => {
        $crate::impl_managed_callback! {
            @expand
            wrapper:        $wrapper,
            ctx_field:      $ctx_field,
            data:           [$data: $data_ty],
            args:           [ $( $arg : $arg_ty ),* ],
            ctx_from:       [ invocation ],
            return_ty:      $ret,
            default_ret:    $default,
            invoker_static: $invoker_static,
            invoker_ty:     $invoker_ty,
            thunk_fn:       $thunk_fn,
            setter_fn:      $setter_fn,
            from_handle_fn: $from_handle_fn,
            from_handle_byref_fn: [ $( $from_handle_byref_fn )? ],
            rest:           [ $( $rest )? ],
        }
    };

    // Form 5: a kind with no data argument (`RegisterComponentLibraryFn`,
    // `fn() -> ComponentLibrary`): like form 4, the context comes from the
    // invocation slot.
    (
        wrapper:        $wrapper:ty,
        ctx_field:      $ctx_field:ident,
        args:           [ $( $arg:ident : $arg_ty:ty ),* $(,)? ],
        return_ty:      $ret:ty,
        default_ret:    $default:expr,
        invoker_static: $invoker_static:ident,
        invoker_ty:     $invoker_ty:ident,
        thunk_fn:       $thunk_fn:ident,
        setter_fn:      $setter_fn:ident,
        from_handle_fn: $from_handle_fn:ident,
        $( from_handle_byref_fn: $from_handle_byref_fn:ident, )?
        $( rest: $rest:expr, )?
    ) => {
        $crate::impl_managed_callback! {
            @expand
            wrapper:        $wrapper,
            ctx_field:      $ctx_field,
            data:           [],
            args:           [ $( $arg : $arg_ty ),* ],
            ctx_from:       [ invocation ],
            return_ty:      $ret,
            default_ret:    $default,
            invoker_static: $invoker_static,
            invoker_ty:     $invoker_ty,
            thunk_fn:       $thunk_fn,
            setter_fn:      $setter_fn,
            from_handle_fn: $from_handle_fn,
            from_handle_byref_fn: [ $( $from_handle_byref_fn )? ],
            rest:           [ $( $rest )? ],
        }
    };

    // Where a thunk reads its host handle from.
    (@thunk_ctx [ info $info:ident ] $thunk_fn:ident) => {
        $info.get_ctx()
    };
    (@thunk_ctx [ invocation ] $thunk_fn:ident) => {
        $crate::host_invoker::invocation_ctx($thunk_fn as usize)
    };
    // What `invoke` installs besides the invocation slot.
    (@install [ info $info:ident ] $ctx:expr) => {
        let mut $info = $info;
        <_ as $crate::host_invoker::HostCtxCarrier>::install_host_ctx(&mut $info, $ctx);
    };
    (@install [ invocation ] $ctx:expr) => {};

    (
        @expand
        wrapper:        $wrapper:ty,
        ctx_field:      $ctx_field:ident,
        data:           [ $( $data:ident : $data_ty:ty )? ],
        args:           [ $( $arg:ident : $arg_ty:ty ),* ],
        ctx_from:       [ $( $ctx_from:tt )* ],
        return_ty:      $ret:ty,
        default_ret:    $default:expr,
        invoker_static: $invoker_static:ident,
        invoker_ty:     $invoker_ty:ident,
        thunk_fn:       $thunk_fn:ident,
        setter_fn:      $setter_fn:ident,
        from_handle_fn: $from_handle_fn:ident,
        from_handle_byref_fn: [ $( $from_handle_byref_fn:ident )? ],
        rest:           [ $( $rest:expr )? ],
    ) => {
        /// Process-global slot for this callback kind's host-side invoker.
        pub static $invoker_static: $crate::host_invoker::InvokerSlot =
            $crate::host_invoker::InvokerSlot::new();

        /// Pointer-arg variant of this callback kind's typedef: the host
        /// handle, every argument by pointer in declared order, and an
        /// out-pointer for the return value. Every managed-FFI runtime can
        /// call this shape (no aggregate by value anywhere; LuaJIT FFI in
        /// particular cannot return aggregates larger than 8 bytes from a
        /// callback, so even an `Update` return goes through `out`).
        pub type $invoker_ty = extern "C" fn(
            handle: u64,
            $( $data: *const $crate::refany::RefAny, )?
            $( $arg : *const $arg_ty , )*
            out: *mut $ret,
        );

        /// Register the host-side invoker for this callback kind.
        #[no_mangle]
        pub extern "C" fn $setter_fn(invoker: $invoker_ty) {
            $invoker_static.set(invoker as usize);
        }

        /// Static thunk compiled into libazul: the `cb` of every wrapper
        /// `create_from_host_handle` builds. Finds the host handle in the
        /// wrapper's context and forwards pointers to the registered invoker.
        extern "C" fn $thunk_fn(
            $( $data: $data_ty, )?
            $( $arg : $arg_ty , )*
        ) -> $ret {
            // The wrapper name as a C string: what the generic invoker's
            // dispatch table keys on.
            const KIND_STR: &str = concat!(stringify!($wrapper), "\0");

            // AUDIT: this thunk is `extern "C"` and dispatches into arbitrary
            // host code (via a transmuted invoker pointer). A panic escaping it
            // would unwind across the FFI boundary (UB), so the body runs
            // inside `catch_unwind`, with `default_ret` on a panic. The body
            // BORROWS the arguments, so `default_ret` can still read them.
            let body = || -> $ret {
                let ctx = $crate::impl_managed_callback!(@thunk_ctx [ $( $ctx_from )* ] $thunk_fn);
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
                    // Per-kind invoker not registered: fall back to the
                    // generic invoker, for hosts that wired up only
                    // `AzApp_setGenericInvoker`.
                    let generic_addr = $crate::host_invoker::GENERIC_INVOKER.get();
                    if generic_addr == 0 {
                        return $default;
                    }
                    // SAFETY: GENERIC_INVOKER only ever holds an address that
                    // came from `invoker as usize` in `AzApp_setGenericInvoker`.
                    let generic: $crate::host_invoker::AzGenericInvoker =
                        unsafe { core::mem::transmute(generic_addr) };
                    // One pointer per argument, in declared order; valid for
                    // the duration of this call only.
                    let args: &[*const core::ffi::c_void] = &[
                        $( $crate::host_invoker::DataArg::data_ptr(&$data) as *const core::ffi::c_void, )?
                        $( &raw const $arg as *const core::ffi::c_void , )*
                    ];
                    let mut out = core::mem::ManuallyDrop::new(
                        <$ret as $crate::host_invoker::HostOut>::unwritten(),
                    );
                    generic(
                        handle,
                        KIND_STR.as_ptr() as *const core::ffi::c_char,
                        args.as_ptr(),
                        args.len(),
                        &raw mut *out as *mut core::ffi::c_void,
                    );
                    if $crate::host_invoker::HostOut::is_unwritten(&*out) {
                        return $default;
                    }
                    return core::mem::ManuallyDrop::into_inner(out);
                }
                // SAFETY: $invoker_static only ever holds a value that came from
                // `invoker as usize` in `$setter_fn`, typed `$invoker_ty`.
                let invoker: $invoker_ty = unsafe { core::mem::transmute(invoker_addr) };
                // Pre-filled with a value that owns nothing (the host overwrites
                // it without dropping it), held in `ManuallyDrop` until the host
                // returned: an unwritten sentinel may not be droppable.
                let mut out = core::mem::ManuallyDrop::new(
                    <$ret as $crate::host_invoker::HostOut>::unwritten(),
                );
                invoker(
                    handle,
                    $( $crate::host_invoker::DataArg::data_ptr(&$data), )?
                    $( &raw const $arg , )*
                    &raw mut *out,
                );
                if $crate::host_invoker::HostOut::is_unwritten(&*out) {
                    return $default;
                }
                core::mem::ManuallyDrop::into_inner(out)
            };

            #[cfg(feature = "std")]
            {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
                    Ok(out) => out,
                    Err(_) => $default,
                }
            }
            #[cfg(not(feature = "std"))]
            {
                body()
            }
        }

        impl $wrapper {
            /// Build a wrapper whose `cb` is the static thunk above and
            /// whose context carries the host's `u64` handle. The host
            /// language is responsible for keeping its id->callable table
            /// in sync with the releaser registered via
            /// `AzApp_setHostHandleReleaser`.
            #[must_use] pub fn create_from_host_handle(handle: u64) -> Self {
                Self {
                    cb: $thunk_fn,
                    $ctx_field: $crate::refany::OptionRefAny::Some(
                        $crate::host_invoker::host_handle_to_refany(handle),
                    ),
                    $( ..$rest )?
                }
            }

            /// What this kind returns when its callee cannot answer (no host
            /// handle, no invoker, a host exception): the host-invoker thunk's
            /// own fallback, for bindings whose C trampolines need the same
            /// answer.
            #[allow(unused_variables)]
            pub fn fallback_return(
                $( $data: &$data_ty, )?
                $( $arg : &$arg_ty , )*
            ) -> $ret {
                $default
            }

            /// Invoke the wrapped callback with this wrapper's context where
            /// the callee looks for it: the invocation slot, and the info
            /// argument for kinds that have one (see
            /// [`$crate::host_invoker::HostCtxCarrier`]). Engine code MUST
            /// call a wrapper through this, never `(self.cb)(..)`: a
            /// managed-language callback reached any other way sees no
            /// context and returns its default without calling the host.
            pub fn invoke(
                &self,
                $( $data: $data_ty, )?
                $( $arg : $arg_ty , )*
            ) -> $ret {
                $crate::impl_managed_callback!(@install [ $( $ctx_from )* ] &self.$ctx_field);
                let _invocation =
                    $crate::host_invoker::enter_invocation(self.cb as usize, &self.$ctx_field);
                // direct-cb-call: this is `invoke` itself.
                (self.cb)($( $data, )? $( $arg ),* )
            }
        }

        /// C-ABI export wrapping `<Wrapper>::create_from_host_handle`.
        #[no_mangle]
        pub extern "C" fn $from_handle_fn(handle: u64) -> $wrapper {
            <$wrapper>::create_from_host_handle(handle)
        }

        $(
        #[no_mangle]
        #[doc(hidden)]
        pub unsafe extern "C" fn $from_handle_byref_fn(handle: u64, out: *mut $wrapper) { unsafe {
            if !out.is_null() {
                core::ptr::write(out, <$wrapper>::create_from_host_handle(handle));
            }
        }}
        )?
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
    private_interfaces
)] // test-only fakes drive the FFI macro; pedantic lints are noise here
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

    impl crate::host_invoker::HostOut for FakeRet {
        fn unwritten() -> Self {
            FakeRet(0)
        }
    }

    struct FakeInfo;
    impl crate::host_invoker::HostCtxCarrier for FakeInfo {
        fn install_host_ctx(&mut self, _ctx: &crate::refany::OptionRefAny) {}
    }
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
        from_handle_byref_fn: az_test_fake_from_handle_byref,
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

#[cfg(test)]
#[path = "host_invoker_test.rs"]
mod host_invoker_test;

#[no_mangle]
pub unsafe extern "C" fn AzRefAny_newHostHandleByref(id: u64, out: *mut RefAny) { unsafe {
    if !out.is_null() {
        core::ptr::write(out, host_handle_to_refany(id));
    }
}}
