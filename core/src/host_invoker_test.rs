#[allow(unused_imports)]
pub use super::*;
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

    use crate::host_invoker::{tests::TEST_LOCK, *};
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
        let kind_ok =
            !kind.is_null() && unsafe { CStr::from_ptr(kind) }.to_str() == Ok("AutoWrapper");
        GENERIC_KIND_OK.store(kind_ok, AtOrdering::SeqCst);

        // args[0] is the by-value `data: RefAny` frame slot, args[1] the info.
        if !args.is_null() && n_args == 2 {
            let arg0 = unsafe { *args };
            if !arg0.is_null() {
                let data = unsafe { &*(arg0.cast::<RefAny>()) };
                GENERIC_ARG0_ID.store(refany_to_host_handle(data).unwrap_or(0), AtOrdering::SeqCst);
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
        assert!(
            GENERIC_KIND_OK.load(AtOrdering::SeqCst),
            "kind must be \"AutoWrapper\\0\""
        );
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
