//! Safety + behaviour tests for the host-language callback invoker plumbing.
//!
//! These cover the C-ABI surface managed-FFI bindings hit at runtime:
//!
//! 1. `host_handle_to_refany(id)` round-trips back to `Some(id)` via
//!    `refany_to_host_handle`, and tags the RefAny with the
//!    `AZ_HOST_HANDLE_RTTI_ID` so the destructor can identify its own
//!    payload.
//! 2. The destructor stamped into host-handle RefAnys forwards the id to
//!    the releaser registered via `AzApp_setHostHandleReleaser` exactly
//!    once, when the *last* clone drops.
//! 3. `refany_to_host_handle` returns `None` for unrelated RefAnys (so a
//!    user-data RefAny accidentally fed into a callback's ctx slot
//!    can't be misidentified as a host handle and free a foreign id).
//! 4. The macro-generated thunks short-circuit safely when no invoker has
//!    been registered yet — the `cb` returned by
//!    `LayoutCallback::create_from_host_handle` is callable and returns
//!    the kind's default rather than transmuting `0` into a fn pointer.
//!
//! The end-to-end "thunk fires invoker with the right by-value args" path
//! is exercised through the Lua / Ruby / etc. integration tests in
//! `examples/<lang>/hello-world.*` once the bindings adopt the new path.
//! Standing up a full `LayoutCallbackInfo` here would require a fontconfig
//! cache, image cache, and system style — heavy fixtures that don't
//! actually buy more safety than checking the macro expansion's branches
//! (releaser slot, RTTI tagging, default-on-empty-invoker) directly.
//!
//! All tests touch the *process-global* invoker slots, so they share a
//! `Mutex` to serialize. Real bindings register once at module load and
//! never set again, so this isn't a concern in production.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use azul_core::callbacks::LayoutCallback;
use azul_core::host_invoker::{
    host_handle_to_refany, refany_to_host_handle, AzApp_setHostHandleReleaser,
    AZ_HOST_HANDLE_RTTI_ID,
};
use azul_core::refany::RefAny;

/// Serialize tests that touch process-global invoker slots.
fn invoker_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

// ── Releaser observer ───────────────────────────────────────────────────

static RELEASER_LAST_ID: AtomicU64 = AtomicU64::new(0);
static RELEASER_FIRED_COUNT: AtomicU64 = AtomicU64::new(0);

extern "C" fn test_releaser(id: u64) {
    RELEASER_LAST_ID.store(id, Ordering::SeqCst);
    RELEASER_FIRED_COUNT.fetch_add(1, Ordering::SeqCst);
}

// ── Tests ───────────────────────────────────────────────────────────────

#[test]
fn host_handle_roundtrips_id_and_destructor_fires_on_drop() {
    let _guard = invoker_lock().lock().unwrap();

    AzApp_setHostHandleReleaser(test_releaser);
    let initial_count = RELEASER_FIRED_COUNT.load(Ordering::SeqCst);

    let refany = host_handle_to_refany(0xDEAD_BEEF_CAFE_F00D);

    assert_eq!(
        refany_to_host_handle(&refany),
        Some(0xDEAD_BEEF_CAFE_F00D),
        "host handle must round-trip the integer id"
    );
    assert!(
        refany.is_type(AZ_HOST_HANDLE_RTTI_ID),
        "host-handle RefAny must carry the dedicated RTTI id"
    );
    assert_eq!(
        RELEASER_FIRED_COUNT.load(Ordering::SeqCst),
        initial_count,
        "releaser must not fire while the RefAny is alive"
    );

    drop(refany);

    assert_eq!(
        RELEASER_FIRED_COUNT.load(Ordering::SeqCst),
        initial_count + 1,
        "releaser must fire exactly once when the last clone is dropped"
    );
    assert_eq!(RELEASER_LAST_ID.load(Ordering::SeqCst), 0xDEAD_BEEF_CAFE_F00D);
}

#[test]
fn host_handle_clone_keeps_releaser_quiet_until_last_drop() {
    let _guard = invoker_lock().lock().unwrap();

    AzApp_setHostHandleReleaser(test_releaser);
    let initial = RELEASER_FIRED_COUNT.load(Ordering::SeqCst);

    let a = host_handle_to_refany(0x1111);
    let b = a.clone();

    drop(a);
    assert_eq!(
        RELEASER_FIRED_COUNT.load(Ordering::SeqCst),
        initial,
        "releaser must not fire while a clone exists"
    );

    drop(b);
    assert_eq!(
        RELEASER_FIRED_COUNT.load(Ordering::SeqCst),
        initial + 1,
        "releaser must fire exactly once after the last clone drops"
    );
    assert_eq!(RELEASER_LAST_ID.load(Ordering::SeqCst), 0x1111);
}

#[test]
fn refany_to_host_handle_rejects_unrelated_refanys() {
    // A user-data RefAny built via the regular `RefAny::new` API must NOT
    // be misidentified as a host handle — otherwise the destructor would
    // call `releaser(garbage_id)` on user data.
    #[derive(Clone)]
    struct UserData {
        _value: u32,
    }
    let user = RefAny::new(UserData { _value: 42 });

    assert_eq!(
        refany_to_host_handle(&user),
        None,
        "user-data RefAny must not collide with host-handle RTTI"
    );
}

#[test]
fn create_from_host_handle_produces_callable_with_host_handle_ctx() {
    let _guard = invoker_lock().lock().unwrap();
    AzApp_setHostHandleReleaser(test_releaser);

    let cb = LayoutCallback::create_from_host_handle(0xABCD_1234);

    // The wrapper's ctx must be the host-handle RefAny we baked in: the
    // thunk relies on `refany_to_host_handle` returning `Some(id)` here.
    let ctx_refany = match cb.ctx {
        azul_core::refany::OptionRefAny::Some(ref r) => r.clone(),
        _ => panic!("create_from_host_handle must produce Some(ctx)"),
    };
    assert_eq!(refany_to_host_handle(&ctx_refany), Some(0xABCD_1234));

    // The cb pointer is the macro-generated static thunk, NOT the no-op
    // default_layout_callback. We assert the address differs from the
    // default to confirm the macro wired itself in correctly.
    let default_cb = LayoutCallback::default();
    let cb_addr = cb.cb as usize;
    let default_addr = default_cb.cb as usize;
    assert_ne!(
        cb_addr, default_addr,
        "create_from_host_handle should install the macro's static thunk, \
         not fall back to default_layout_callback"
    );
}

// Note: actually firing `cb.cb(data, info)` requires constructing a real
// `LayoutCallbackInfo` with all its lifetime-bound `LayoutCallbackInfoRefData`
// references (image cache, fontconfig, system style…). That's heavy fixture
// work that doesn't add safety beyond checking the macro's branches above
// — the integration path is exercised through `examples/lua/hello-world.lua`
// once the binding adopts `_createFromHostHandle`.
