//! Linux keyring backend — libsecret (Secret Service) via dlopen.
//!
//! Uses the NON-variadic `secret_password_{storev,lookupv,clearv}_sync` forms
//! (stable Rust can't call C-variadic fn-pointers) with a one-attribute `"key"`
//! GHashTable: the secret is the stored password, the request `key` is the
//! lookup attribute. Mirrors apple.rs (spawn thread -> push_keyring_result).
//! If libsecret / a Secret Service isn't present, every op -> Unavailable.
//! `require_biometry` has no libsecret equivalent (the secret-service has no
//! per-item biometric gate) — stored normally, the flag ignored.

use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
use std::ptr;
use std::sync::OnceLock;

use azul_core::keyring::{KeyringRequest, KeyringResult};
use azul_layout::managers::keyring::push_keyring_result;

use crate::desktop::shell2::common::{
    dlopen::load_first_available, DlError, DynamicLibrary as DynamicLibraryTrait,
};
use crate::desktop::shell2::linux::x11::dlopen::Library;
use crate::load_symbol;

const SERVICE: &str = "com.azul.keyring";

#[repr(C)]
#[derive(Copy, Clone)]
struct GError {
    domain: c_uint,
    code: c_int,
    message: *mut c_char,
}
#[repr(C)]
#[derive(Copy, Clone)]
struct SecretSchemaAttribute {
    name: *const c_char,
    type_: c_uint,
}
#[repr(C)]
struct SecretSchema {
    name: *const c_char,
    flags: c_uint,
    attributes: [SecretSchemaAttribute; 32],
    reserved: c_int,
    reserved1: *mut c_void,
    reserved2: *mut c_void,
    reserved3: *mut c_void,
    reserved4: *mut c_void,
    reserved5: *mut c_void,
    reserved6: *mut c_void,
    reserved7: *mut c_void,
}

type StorevFn = unsafe extern "C" fn(
    *const SecretSchema,
    *mut c_void,
    *const c_char,
    *const c_char,
    *const c_char,
    *mut c_void,
    *mut *mut GError,
) -> c_int;
type LookupvFn = unsafe extern "C" fn(
    *const SecretSchema,
    *mut c_void,
    *mut c_void,
    *mut *mut GError,
) -> *mut c_char;
type ClearvFn = unsafe extern "C" fn(
    *const SecretSchema,
    *mut c_void,
    *mut c_void,
    *mut *mut GError,
) -> c_int;

struct SecretLib {
    _lib: Library,
    _glib: Library,
    storev: StorevFn,
    lookupv: LookupvFn,
    clearv: ClearvFn,
    pw_free: unsafe extern "C" fn(*mut c_char),
    ht_new: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void,
    ht_insert: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> c_int,
    ht_unref: unsafe extern "C" fn(*mut c_void),
    str_hash: *mut c_void,
    str_equal: *mut c_void,
    g_error_free: unsafe extern "C" fn(*mut GError),
}
unsafe impl Send for SecretLib {}
unsafe impl Sync for SecretLib {}

impl SecretLib {
    fn load() -> Result<Self, DlError> {
        let lib = load_first_available::<Library>(&["libsecret-1.so.0", "libsecret-1.so"])?;
        let glib = load_first_available::<Library>(&["libglib-2.0.so.0", "libglib-2.0.so"])?;
        Ok(Self {
            storev: load_symbol!(lib, StorevFn, "secret_password_storev_sync"),
            lookupv: load_symbol!(lib, LookupvFn, "secret_password_lookupv_sync"),
            clearv: load_symbol!(lib, ClearvFn, "secret_password_clearv_sync"),
            pw_free: load_symbol!(lib, _, "secret_password_free"),
            ht_new: load_symbol!(glib, _, "g_hash_table_new"),
            ht_insert: load_symbol!(glib, _, "g_hash_table_insert"),
            ht_unref: load_symbol!(glib, _, "g_hash_table_unref"),
            str_hash: load_symbol!(glib, *mut c_void, "g_str_hash"),
            str_equal: load_symbol!(glib, *mut c_void, "g_str_equal"),
            g_error_free: load_symbol!(glib, _, "g_error_free"),
            _lib: lib,
            _glib: glib,
        })
    }

    /// Build a 1-entry GHashTable `{"key": <key>}`. Caller unrefs it.
    unsafe fn attrs(&self, key_c: &CStr) -> *mut c_void {
        let ht = (self.ht_new)(self.str_hash, self.str_equal);
        (self.ht_insert)(
            ht,
            b"key\0".as_ptr() as *mut c_void,
            key_c.as_ptr() as *mut c_void,
        );
        ht
    }
}

fn schema() -> SecretSchema {
    let mut s: SecretSchema = unsafe { std::mem::zeroed() };
    s.name = b"com.azul.keyring\0".as_ptr() as *const c_char;
    s.flags = 0; // SECRET_SCHEMA_NONE
    s.attributes[0] = SecretSchemaAttribute {
        name: b"key\0".as_ptr() as *const c_char,
        type_: 0, // SECRET_SCHEMA_ATTRIBUTE_STRING
    };
    s
}

fn lib() -> Option<&'static SecretLib> {
    static LIB: OnceLock<Option<SecretLib>> = OnceLock::new();
    LIB.get_or_init(|| SecretLib::load().ok()).as_ref()
}

pub fn request(req: &KeyringRequest) {
    let req = req.clone();
    std::thread::spawn(move || {
        push_keyring_result(handle(&req));
    });
}

fn handle(req: &KeyringRequest) -> KeyringResult {
    let lib = match lib() {
        Some(l) => l,
        None => return KeyringResult::Unavailable, // no libsecret / Secret Service
    };
    let sch = schema();
    match req {
        KeyringRequest::Store { key, secret, .. } => {
            let key_c = CString::new(key.as_str()).unwrap_or_default();
            let label = CString::new(format!("{SERVICE}:{}", key.as_str())).unwrap_or_default();
            let pw = CString::new(secret.as_str()).unwrap_or_default();
            unsafe {
                let ht = lib.attrs(&key_c);
                let mut err: *mut GError = ptr::null_mut();
                let ok = (lib.storev)(
                    &sch,
                    ht,
                    b"default\0".as_ptr() as *const c_char, // SECRET_COLLECTION_DEFAULT
                    label.as_ptr(),
                    pw.as_ptr(),
                    ptr::null_mut(),
                    &mut err,
                ) != 0;
                (lib.ht_unref)(ht);
                if !err.is_null() {
                    (lib.g_error_free)(err);
                    return KeyringResult::Unavailable;
                }
                if ok {
                    KeyringResult::Stored
                } else {
                    KeyringResult::Error
                }
            }
        }
        KeyringRequest::Get { key } => {
            let key_c = CString::new(key.as_str()).unwrap_or_default();
            unsafe {
                let ht = lib.attrs(&key_c);
                let mut err: *mut GError = ptr::null_mut();
                let p = (lib.lookupv)(&sch, ht, ptr::null_mut(), &mut err);
                (lib.ht_unref)(ht);
                if !err.is_null() {
                    (lib.g_error_free)(err);
                    return KeyringResult::Unavailable;
                }
                if p.is_null() {
                    return KeyringResult::NotFound;
                }
                let s = CStr::from_ptr(p).to_string_lossy().into_owned();
                (lib.pw_free)(p);
                KeyringResult::Retrieved(s.into())
            }
        }
        KeyringRequest::Delete { key } => {
            let key_c = CString::new(key.as_str()).unwrap_or_default();
            unsafe {
                let ht = lib.attrs(&key_c);
                let mut err: *mut GError = ptr::null_mut();
                let _ = (lib.clearv)(&sch, ht, ptr::null_mut(), &mut err);
                (lib.ht_unref)(ht);
                if !err.is_null() {
                    (lib.g_error_free)(err);
                    return KeyringResult::Unavailable;
                }
                KeyringResult::Deleted // idempotent (no match also reports Deleted)
            }
        }
    }
}
