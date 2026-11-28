//! SystemConfiguration, the macOS-only half, answered "nothing is configured" on iOS (engine ledger E63).
//!
//! iOS's SDK marks the dynamic store (`SCDynamicStore*`) and the network preferences (`SCNetworkInterface*`, `SCNetworkService*`,
//! `SCNetworkProtocol*`, `SCPreferences*`) `API_UNAVAILABLE(ios)`: an iOS app cannot read them, and `scripts/ci/ios_symbol_gate.py`
//! refuses a binary that imports them. Two dependencies of the `iroh` feature reference them anyway, because they were written
//! for a Mac and compile unchanged for iOS: `n0-dns-resolver` (the DNS servers of the system, through the `system-configuration`
//! crate) and `netdev` up to 0.46 (the interface lister of `netwatch` 0.19, iroh 1.2). With the references, AzMeet and AzCompanion
//! did not build for iOS. We cannot patch a published dependency, so the engine DEFINES the 23 symbols itself, in the table of
//! `common/ios_system_configuration.rs`: the linker then finds them in the engine's archive and the binary imports none of them.
//!
//! WHAT EACH ANSWERS (the same words as `ShimKind`): NULL for "no store / no value / no name / no service" (the dependency's own
//! error path runs: `n0-dns-resolver` logs "failed to read system DNS configuration, using fallback" and uses public resolvers,
//! `netdev` lists its interfaces from `getifaddrs` and Network.framework without the friendly names); an empty array and an empty
//! string for the three functions whose Rust bindings `expect` an object; and the eight `const CFStringRef` the dependencies read hold
//! `kCFNull` (set before `main` by the constructor below): `system-configuration` wraps `kSCDynamicStoreUseSessionKeys` under the
//! CREATE rule and releases it each time it builds a store, so a string made at start-up would be freed by the second call, while
//! the immortal singleton takes any number of retains and releases and nobody reads these constants as text.
//!
//! WHEN IT GOES. netdev 0.47 (netwatch 0.20, iroh 1.4 and later) drops the iOS use: after the lock file moves there only the three
//! symbols of `n0-dns-resolver` (see `ios_system_configuration::SHIMMED`) are still referenced, and the rest of this file can go. A
//! release of `n0-dns-resolver` that reads the DNS servers some other way on iOS removes the file.
//!
//! WHERE IT IS COMPILED. For iOS and its Simulator, not for Mac Catalyst (`target_abi = "macabi"`: there the real functions exist and
//! must not be replaced); `ios/mod.rs` declares the module under that condition. macOS never sees this file.

#![allow(non_snake_case, non_upper_case_globals)]

use std::{
    ffi::{c_char, c_void},
    ptr::{addr_of, addr_of_mut, null},
};

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    /// The immortal `CFNull` singleton.
    static kCFNull: *const c_void;
    /// The callbacks of an array of CF objects (only its address is used).
    static kCFTypeArrayCallBacks: [u8; 1];
    fn CFArrayCreate(
        allocator: *const c_void,
        values: *const *const c_void,
        count: isize,
        callbacks: *const c_void,
    ) -> *const c_void;
    fn CFStringCreateWithCString(allocator: *const c_void, c_str: *const c_char, encoding: u32) -> *const c_void;
}

/// `kCFStringEncodingUTF8`.
const UTF8: u32 = 0x0800_0100;

/// The key of a store that does not exist: an empty string (the Rust bindings of the key builders do not accept NULL).
fn empty_string() -> *const c_void {
    // SAFETY: a NULL allocator is the default one; the string literal is NUL terminated and valid UTF-8.
    unsafe { CFStringCreateWithCString(null(), c"".as_ptr(), UTF8) }
}

// ---- the functions that answer "nothing" ----------------------------------------------------------------------------------

/// `SCDynamicStoreCreate`: no store can be opened here.
#[no_mangle]
pub extern "C" fn SCDynamicStoreCreate(
    _allocator: *const c_void,
    _name: *const c_void,
    _callout: *const c_void,
    _context: *mut c_void,
) -> *const c_void {
    null()
}

/// `SCDynamicStoreCreateWithOptions`: no store can be opened here (`n0-dns-resolver` then uses public resolvers).
#[no_mangle]
pub extern "C" fn SCDynamicStoreCreateWithOptions(
    _allocator: *const c_void,
    _name: *const c_void,
    _options: *const c_void,
    _callout: *const c_void,
    _context: *mut c_void,
) -> *const c_void {
    null()
}

/// `SCDynamicStoreCopyValue`: nothing is stored.
#[no_mangle]
pub extern "C" fn SCDynamicStoreCopyValue(_store: *const c_void, _key: *const c_void) -> *const c_void {
    null()
}

/// `SCDynamicStoreKeyCreateNetworkGlobalEntity`: a key that no store holds (the binding does not accept NULL).
#[no_mangle]
pub extern "C" fn SCDynamicStoreKeyCreateNetworkGlobalEntity(
    _allocator: *const c_void,
    _domain: *const c_void,
    _entity: *const c_void,
) -> *const c_void {
    empty_string()
}

/// `SCDynamicStoreKeyCreateNetworkServiceEntity`: a key that no store holds (the binding does not accept NULL).
#[no_mangle]
pub extern "C" fn SCDynamicStoreKeyCreateNetworkServiceEntity(
    _allocator: *const c_void,
    _domain: *const c_void,
    _service_id: *const c_void,
    _entity: *const c_void,
) -> *const c_void {
    empty_string()
}

/// `SCNetworkInterfaceCopyAll`: no interface is configured (an empty array: the binding does not accept NULL).
#[no_mangle]
pub extern "C" fn SCNetworkInterfaceCopyAll() -> *const c_void {
    // SAFETY: zero values, the callbacks of a CF array; the result is owned by the caller (the create rule).
    unsafe { CFArrayCreate(null(), null(), 0, addr_of!(kCFTypeArrayCallBacks).cast()) }
}

/// `SCNetworkInterfaceGetBSDName`: no interface, no name.
#[no_mangle]
pub extern "C" fn SCNetworkInterfaceGetBSDName(_interface: *const c_void) -> *const c_void {
    null()
}

/// `SCNetworkInterfaceGetInterfaceType`: no interface, no type.
#[no_mangle]
pub extern "C" fn SCNetworkInterfaceGetInterfaceType(_interface: *const c_void) -> *const c_void {
    null()
}

/// `SCNetworkInterfaceGetLocalizedDisplayName`: no interface, no name.
#[no_mangle]
pub extern "C" fn SCNetworkInterfaceGetLocalizedDisplayName(_interface: *const c_void) -> *const c_void {
    null()
}

/// `SCNetworkProtocolGetConfiguration`: no protocol, no configuration.
#[no_mangle]
pub extern "C" fn SCNetworkProtocolGetConfiguration(_protocol: *const c_void) -> *const c_void {
    null()
}

/// `SCNetworkServiceCopyAll`: no preferences, no services.
#[no_mangle]
pub extern "C" fn SCNetworkServiceCopyAll(_prefs: *const c_void) -> *const c_void {
    null()
}

/// `SCNetworkServiceCopyProtocol`: no service, no protocol.
#[no_mangle]
pub extern "C" fn SCNetworkServiceCopyProtocol(_service: *const c_void, _protocol_type: *const c_void) -> *const c_void {
    null()
}

/// `SCNetworkServiceGetInterface`: no service, no interface.
#[no_mangle]
pub extern "C" fn SCNetworkServiceGetInterface(_service: *const c_void) -> *const c_void {
    null()
}

/// `SCNetworkServiceGetName`: no service, no name.
#[no_mangle]
pub extern "C" fn SCNetworkServiceGetName(_service: *const c_void) -> *const c_void {
    null()
}

/// `SCPreferencesCreate`: the network preferences cannot be opened here.
#[no_mangle]
pub extern "C" fn SCPreferencesCreate(_allocator: *const c_void, _name: *const c_void, _prefs_id: *const c_void) -> *const c_void {
    null()
}

// ---- the constants the dependencies read ---------------------------------------------------------------------------------

/// Defines the constants (`const CFStringRef` of the framework) and the function that fills them.
macro_rules! constants {
    ($($name:ident),+ $(,)?) => {
        $(
            #[no_mangle]
            pub static mut $name: *const c_void = null();
        )+

        /// Runs before `main`: every constant holds `kCFNull` (see the module documentation).
        extern "C" fn fill_constants() {
            // SAFETY: the dynamic loader runs this when the image is loaded, after CoreFoundation's own initialisation and before any
            // thread of the app exists; `kCFNull` is a pointer-sized constant of CoreFoundation, the statics are written only here.
            unsafe {
                let null_object = kCFNull;
                $( addr_of_mut!($name).write(null_object); )+
            }
        }
    };
}

constants!(
    kSCDynamicStoreDomainState,
    kSCDynamicStorePropNetPrimaryService,
    kSCDynamicStoreUseSessionKeys,
    kSCEntNetDNS,
    kSCEntNetIPv4,
    kSCNetworkProtocolTypeIPv4,
    kSCNetworkProtocolTypeIPv6,
    kSCPropNetDNSServerAddresses,
);

/// The loader's list of initialisers of this image (`S_MOD_INIT_FUNC_POINTERS`).
#[used]
#[link_section = "__DATA,__mod_init_func"]
static FILL_CONSTANTS: extern "C" fn() = fill_constants;
