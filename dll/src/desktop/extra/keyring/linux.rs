//! Linux keyring backend — libsecret (Secret Service) via dlopen.
//!
//! Uses the NON-variadic `secret_password_{storev,lookupv,clearv}_sync` forms
//! (stable Rust can't call C-variadic fn-pointers) with a one-attribute `"key"`
//! GHashTable: the secret is the stored password, the request `key` is the
//! lookup attribute; the namespace's service is the schema name (libsecret
//! matches items by it), so two namespaces never see each other's items.
//! If libsecret / a Secret Service isn't present, every op -> Unavailable.
//!
//! The Secret Service has no per-item user check, so a PROTECTED item (`UserPresence` /
//! `Biometric`) is guarded by the LIBRARY (`KeyringSupport::Library`): its record is a sibling
//! item under [`record::meta_key`] (level, and for `Biometric` the digest of the enrolled
//! fingers), and an `Unlock` runs the user's check first - the desktop's polkit dialog for
//! `UserPresence` (the action `io.azul.keyring.unlock`, a policy file packagers install:
//! `scripts/linux/io.azul.keyring.policy`), the fingerprint reader (fprintd) for `Biometric` -
//! and only then reads the item. Another process of the same user can read the item without
//! the check: that is what `Library` says.

use std::{
    ffi::{c_char, c_int, c_uint, c_void, CStr, CString},
    ptr,
    sync::OnceLock,
};

use azul_core::{
    biometric::BiometricResult,
    keyring::{
        KeyringNeed, KeyringProtection, KeyringPrompt, KeyringRequest, KeyringResult,
        KeyringSupport,
    },
};

use super::{
    record::{
        fingers_digest, meta_key, need_of_record, run_check, Authorization, Fingers, GateRecord,
        UserCheck,
    },
    Collection, Interaction,
};
use crate::{
    desktop::shell2::{
        common::{dlopen::load_first_available, DlError, DynamicLibrary as DynamicLibraryTrait},
        linux::x11::dlopen::Library,
    },
    load_symbol,
};

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
type ClearvFn =
    unsafe extern "C" fn(*const SecretSchema, *mut c_void, *mut c_void, *mut *mut GError) -> c_int;

type ForAliasFn = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    c_uint,
    *mut c_void,
    *mut *mut GError,
) -> *mut c_void;
type IsLockedFn = unsafe extern "C" fn(*mut c_void) -> c_int;
type UnrefFn = unsafe extern "C" fn(*mut c_void);

/// What it takes to ask the default collection whether it is locked, without unlocking it: loaded
/// as OPTIONAL symbols (`secret_collection_get_locked` and the alias lookup are libsecret 0.18 and
/// later; an older one keeps working, only without the look).
struct LockProbe {
    for_alias: ForAliasFn,
    is_locked: IsLockedFn,
    unref: UnrefFn,
}

struct SecretLib {
    _lib: Library,
    _glib: Library,
    probe: Option<LockProbe>,
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
        // SAFETY: the types are libsecret's and GObject's own; `g_object_unref` is found through
        // libsecret's handle, which searches the libraries it depends on.
        let probe = unsafe {
            match (
                lib.get_symbol::<ForAliasFn>("secret_collection_for_alias_sync")
                    .ok(),
                lib.get_symbol::<IsLockedFn>("secret_collection_get_locked")
                    .ok(),
                lib.get_symbol::<UnrefFn>("g_object_unref").ok(),
            ) {
                (Some(for_alias), Some(is_locked), Some(unref)) => Some(LockProbe {
                    for_alias,
                    is_locked,
                    unref,
                }),
                _ => None,
            }
        };
        Ok(Self {
            probe,
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

    /// Whether the default collection exists and is locked, asked without unlocking anything:
    /// `secret_collection_for_alias_sync` with no flags loads no items and shows no prompt.
    /// [`Collection::Unknown`] when libsecret lacks the calls or refuses this one.
    fn default_collection(&self) -> Collection {
        let Some(probe) = &self.probe else {
            return Collection::Unknown;
        };
        let mut err: *mut GError = ptr::null_mut();
        // SAFETY: libsecret's own call with the types it declares: the default service (NULL), the
        // collection `default`, SECRET_COLLECTION_NONE, no cancellable, an out-pointer for the error.
        let collection = unsafe {
            (probe.for_alias)(
                ptr::null_mut(),
                b"default\0".as_ptr() as *const c_char,
                0,
                ptr::null_mut(),
                &mut err,
            )
        };
        if !err.is_null() {
            // SAFETY: the error the call just made, freed once here.
            let why = unsafe { gerror_text_and_free(self, err) };
            crate::plog_warn!(
                "[keyring] could not tell whether the default collection is locked ({}); the op \
                 goes on",
                why
            );
            return Collection::Unknown;
        }
        if collection.is_null() {
            return Collection::Missing;
        }
        // SAFETY: the collection the call returned, asked once and released once.
        let locked = unsafe {
            let locked = (probe.is_locked)(collection) != 0;
            (probe.unref)(collection);
            locked
        };
        if locked {
            Collection::Locked
        } else {
            Collection::Unlocked
        }
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

/// The schema of `service`'s items; it points into `service`, which must outlive it.
fn schema(service: &CStr) -> SecretSchema {
    let mut s: SecretSchema = unsafe { std::mem::zeroed() };
    s.name = service.as_ptr();
    s.flags = 0; // SECRET_SCHEMA_NONE
    s.attributes[0] = SecretSchemaAttribute {
        name: b"key\0".as_ptr() as *const c_char,
        type_: 0, // SECRET_SCHEMA_ATTRIBUTE_STRING
    };
    s
}

fn lib() -> Option<&'static SecretLib> {
    static LIB: OnceLock<Option<SecretLib>> = OnceLock::new();
    LIB.get_or_init(|| match SecretLib::load() {
        Ok(l) => Some(l),
        Err(e) => {
            // SecretLib::load builds a full DlError (libraries tried + loader
            // errors + install suggestion) — it was discarded by .ok() and
            // every keyring call just reported Unavailable.
            crate::plog_warn!(
                "[keyring] libsecret/libglib not loadable ({}) — keyring unavailable for this \
                 process",
                e
            );
            None
        }
    })
    .as_ref()
}

/// Log the GError message — the exact Secret Service / D-Bus reason ("no such
/// secret service", "prompt dismissed", "collection locked") — before freeing
/// it. It used to be freed unread at all three call sites, making a locked
/// keyring indistinguishable from a missing daemon.
unsafe fn warn_gerror_and_free(lib: &SecretLib, op: &str, err: *mut GError) {
    // SAFETY: `err` is a GError libsecret made; its domain and code are read before it is freed.
    let (domain, code) = unsafe { ((*err).domain, (*err).code) };
    // SAFETY: the same error, freed once, here.
    let msg = unsafe { gerror_text_and_free(lib, err) };
    crate::plog_warn!(
        "[keyring] {} failed: {} (domain={}, code={}) — reporting Unavailable",
        op,
        msg,
        domain,
        code
    );
}

/// The GError's message, and the error freed.
unsafe fn gerror_text_and_free(lib: &SecretLib, err: *mut GError) -> String {
    // SAFETY: `err` is a GError libsecret made, with a NUL-terminated message or none; freed once.
    unsafe {
        let msg = if (*err).message.is_null() {
            String::from("<no message>")
        } else {
            CStr::from_ptr((*err).message)
                .to_string_lossy()
                .into_owned()
        };
        (lib.g_error_free)(err);
        msg
    }
}

/// One op on the items of `service`, on the calling thread. A quiet op shows no prompt: it asks
/// libsecret whether the default collection is locked first, and answers `WouldPrompt` instead of
/// letting the unlock dialog come ([`quiet_verdict`]). The user-check sheet (polkit / fprintd)
/// shows for an `Unlock` and a protected `Store` of a [`Interaction::UserCheck`] op only.
pub(super) fn handle(service: &str, req: &KeyringRequest, interaction: Interaction) -> KeyringResult {
    let lib = match lib() {
        Some(l) => l,
        None => return KeyringResult::Unavailable, // no libsecret / Secret Service
    };
    let Ok(service_c) = CString::new(service) else {
        return KeyringResult::Error;
    };
    if interaction.quiet() {
        if let Some(answer) = super::quiet_verdict(lib.default_collection(), req) {
            return answer;
        }
    }
    let sch = schema(&service_c);
    match req {
        KeyringRequest::Store {
            key,
            secret,
            protection,
            prompt,
        } => store_item(lib, &sch, service, key.as_str(), secret.as_str(), *protection, prompt),
        KeyringRequest::Get { key } => {
            // Without the user: an unprotected item is read, a protected one is not.
            match record_of(lib, &sch, key.as_str()) {
                Err(answer) => answer,
                Ok(Some(record)) if record.protection.needs_user() => {
                    match need_of_record(&record, &LinuxCheck.fingers()) {
                        KeyringNeed::Invalidated => KeyringResult::Invalidated,
                        _ => KeyringResult::WouldPrompt,
                    }
                }
                Ok(_) => lookup_item(lib, &sch, key.as_str()),
            }
        }
        KeyringRequest::Unlock { key, prompt } => match record_of(lib, &sch, key.as_str()) {
            Err(answer) => answer,
            Ok(Some(record)) if record.protection.needs_user() => {
                match lookup_item(lib, &sch, key.as_str()) {
                    // No item behind the record: nothing to open.
                    KeyringResult::NotFound => KeyringResult::NotFound,
                    KeyringResult::Retrieved(secret) => {
                        match run_check(&record, &LinuxCheck, prompt.reason.as_str()) {
                            Ok(()) => KeyringResult::Retrieved(secret),
                            Err(answer) => answer,
                        }
                    }
                    other => other,
                }
            }
            Ok(_) => lookup_item(lib, &sch, key.as_str()),
        },
        KeyringRequest::Delete { key } => {
            let answer = clear_item(lib, &sch, key.as_str());
            if answer == KeyringResult::Deleted {
                // Its record goes with it.
                let _ = clear_item(lib, &sch, &meta_key(key.as_str()));
            }
            answer
        }
    }
}

/// The record of the item `key`: `None` for an item with none (it is unprotected).
fn record_of(
    lib: &SecretLib,
    sch: &SecretSchema,
    key: &str,
) -> Result<Option<GateRecord>, KeyringResult> {
    match lookup_item(lib, sch, &meta_key(key)) {
        KeyringResult::Retrieved(text) => Ok(GateRecord::parse(text.as_str())),
        KeyringResult::NotFound => Ok(None),
        other => Err(other),
    }
}

/// Stores `secret` under `key`; a protected one first checks the device can meet the level and
/// writes the record after the item.
fn store_item(
    lib: &SecretLib,
    sch: &SecretSchema,
    service: &str,
    key: &str,
    secret: &str,
    protection: KeyringProtection,
    prompt: &KeyringPrompt,
) -> KeyringResult {
    let _ = prompt; // the library gate asks at the read; a store of a secret shows nothing
    let record = if protection.needs_user() {
        let fingers = LinuxCheck.fingers();
        // Never weaker than asked: the level must be one this device can meet.
        match (protection, &fingers) {
            (KeyringProtection::Biometric, Fingers::Absent) => return KeyringResult::Unavailable,
            (KeyringProtection::Biometric, Fingers::None) => return KeyringResult::NotEnrolled,
            (KeyringProtection::UserPresence, _) => {
                if LinuxCheck.authorize_registered() == Authorization::Unregistered {
                    return KeyringResult::NotEnrolled;
                }
            }
            _ => {}
        }
        Some(GateRecord {
            protection,
            enrolled: match fingers {
                Fingers::Enrolled(digest) if protection == KeyringProtection::Biometric => {
                    Some(digest)
                }
                _ => None,
            },
        })
    } else {
        None
    };
    let stored = store_raw(lib, sch, service, key, secret);
    if stored != KeyringResult::Stored {
        return stored;
    }
    let meta = meta_key(key);
    match record {
        Some(record) => store_raw(lib, sch, service, &meta, &record.text()),
        None => {
            // An item that was protected before is plain now: its record goes.
            let _ = clear_item(lib, sch, &meta);
            KeyringResult::Stored
        }
    }
}

/// One `secret_password_storev_sync` of `secret` under `key`.
fn store_raw(
    lib: &SecretLib,
    sch: &SecretSchema,
    service: &str,
    key: &str,
    secret: &str,
) -> KeyringResult {
    // A NUL ends a C string: `answer_with` refuses one first; this is the last line of
    // that, so a secret is never stored as "".
    let (Ok(key_c), Ok(label), Ok(pw)) = (
        CString::new(key),
        CString::new(format!("{service}:{key}")),
        CString::new(secret),
    ) else {
        return KeyringResult::Error;
    };
    unsafe {
        let ht = lib.attrs(&key_c);
        let mut err: *mut GError = ptr::null_mut();
        let ok = (lib.storev)(
            sch,
            ht,
            b"default\0".as_ptr() as *const c_char, // SECRET_COLLECTION_DEFAULT
            label.as_ptr(),
            pw.as_ptr(),
            ptr::null_mut(),
            &mut err,
        ) != 0;
        (lib.ht_unref)(ht);
        if !err.is_null() {
            warn_gerror_and_free(lib, "secret_password_storev_sync", err);
            return KeyringResult::Unavailable;
        }
        if ok {
            KeyringResult::Stored
        } else {
            KeyringResult::Error
        }
    }
}

/// One `secret_password_lookupv_sync` of `key`.
fn lookup_item(lib: &SecretLib, sch: &SecretSchema, key: &str) -> KeyringResult {
    let Ok(key_c) = CString::new(key) else {
        return KeyringResult::Error;
    };
    unsafe {
        let ht = lib.attrs(&key_c);
        let mut err: *mut GError = ptr::null_mut();
        let p = (lib.lookupv)(sch, ht, ptr::null_mut(), &mut err);
        (lib.ht_unref)(ht);
        if !err.is_null() {
            warn_gerror_and_free(lib, "secret_password_lookupv_sync", err);
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

/// One `secret_password_clearv_sync` of `key`.
fn clear_item(lib: &SecretLib, sch: &SecretSchema, key: &str) -> KeyringResult {
    let Ok(key_c) = CString::new(key) else {
        return KeyringResult::Error;
    };
    unsafe {
        let ht = lib.attrs(&key_c);
        let mut err: *mut GError = ptr::null_mut();
        let _ = (lib.clearv)(sch, ht, ptr::null_mut(), &mut err);
        (lib.ht_unref)(ht);
        if !err.is_null() {
            warn_gerror_and_free(lib, "secret_password_clearv_sync", err);
            return KeyringResult::Unavailable;
        }
        KeyringResult::Deleted // idempotent (no match also reports Deleted)
    }
}

/// What a read of `key` needs: the record and the default collection, never a sheet.
pub(super) fn needs_prompt(service: &str, key: &str) -> KeyringNeed {
    let Some(lib) = lib() else {
        return KeyringNeed::Unavailable;
    };
    let Ok(service_c) = CString::new(service) else {
        return KeyringNeed::Error;
    };
    match lib.default_collection() {
        Collection::Locked => return KeyringNeed::Unlock,
        Collection::Missing => return KeyringNeed::NotFound,
        Collection::Unlocked | Collection::Unknown => {}
    }
    let sch = schema(&service_c);
    match record_of(lib, &sch, key) {
        Err(KeyringResult::Unavailable) => KeyringNeed::Unavailable,
        Err(_) => KeyringNeed::Error,
        Ok(Some(record)) => need_of_record(&record, &LinuxCheck.fingers()),
        Ok(None) => match lookup_item(lib, &sch, key) {
            KeyringResult::Retrieved(_) => KeyringNeed::Nothing,
            KeyringResult::NotFound => KeyringNeed::NotFound,
            KeyringResult::Unavailable => KeyringNeed::Unavailable,
            _ => KeyringNeed::Error,
        },
    }
}

/// What this machine does for `protection`: libsecret for the plain level; the library's gate
/// for the others, when the service it asks the user through is there.
pub(super) fn support(protection: KeyringProtection) -> KeyringSupport {
    if lib().is_none() {
        return KeyringSupport::Unavailable;
    }
    match protection {
        KeyringProtection::DeviceUnlocked => KeyringSupport::Library,
        KeyringProtection::Biometric => match LinuxCheck.fingers() {
            Fingers::Absent => KeyringSupport::Unavailable,
            Fingers::None => KeyringSupport::NotEnrolled,
            Fingers::Enrolled(_) => KeyringSupport::Library,
        },
        KeyringProtection::UserPresence => match LinuxCheck.authorize_registered() {
            Authorization::Unregistered => KeyringSupport::NotEnrolled,
            Authorization::Unavailable | Authorization::Error => KeyringSupport::Unavailable,
            Authorization::Granted | Authorization::Cancelled => KeyringSupport::Library,
        },
    }
}

/// The polkit action an `Unlock` of a `UserPresence` item asks the desktop to authorise.
const POLKIT_ACTION: &str = "io.azul.keyring.unlock";

/// The fingerprint reader (fprintd) and the desktop's authorisation dialog (polkit), over D-Bus.
struct LinuxCheck;

impl UserCheck for LinuxCheck {
    fn fingers(&self) -> Fingers {
        match crate::desktop::extra::biometric::linux::enrolled_fingers() {
            None => Fingers::Absent,
            Some(names) if names.is_empty() => Fingers::None,
            Some(names) => Fingers::Enrolled(fingers_digest(&names)),
        }
    }

    fn verify_finger(&self) -> BiometricResult {
        crate::desktop::extra::biometric::linux::verify_once()
    }

    fn authorize(&self, reason: &str) -> Authorization {
        polkit_check(reason, true)
    }
}

impl LinuxCheck {
    /// Whether the policy file of the polkit action is installed, asked without showing a dialog.
    fn authorize_registered(&self) -> Authorization {
        polkit_check("", false)
    }
}

/// `org.freedesktop.PolicyKit1.Authority.CheckAuthorization` for [`POLKIT_ACTION`] with this
/// process as the subject. `interactive`: the desktop's agent may ask the user (flag
/// `AllowUserInteraction`); else only whether the action is known is learned.
fn polkit_check(reason: &str, interactive: bool) -> Authorization {
    use std::collections::HashMap;

    use zbus::{
        blocking::{Connection, Proxy},
        zvariant::Value,
    };

    let Ok(conn) = Connection::system() else {
        return Authorization::Unavailable;
    };
    let Ok(authority) = Proxy::new(
        &conn,
        "org.freedesktop.PolicyKit1",
        "/org/freedesktop/PolicyKit1/Authority",
        "org.freedesktop.PolicyKit1.Authority",
    ) else {
        return Authorization::Unavailable;
    };
    let pid = std::process::id();
    let Some(start_time) = process_start_time(pid) else {
        return Authorization::Error;
    };
    let mut subject_details: HashMap<&str, Value<'_>> = HashMap::new();
    subject_details.insert("pid", Value::from(pid));
    subject_details.insert("start-time", Value::from(start_time));
    let subject = ("unix-process", subject_details);
    let mut details: HashMap<&str, &str> = HashMap::new();
    if !reason.is_empty() {
        details.insert("polkit.message", reason);
    }
    let flags: u32 = u32::from(interactive); // 1 = AllowUserInteraction
    // (is_authorized, is_challenge, details)
    let reply: Result<(bool, bool, HashMap<String, String>), zbus::Error> =
        authority.call("CheckAuthorization", &(subject, POLKIT_ACTION, details, flags, ""));
    match reply {
        Ok((true, _, _)) => Authorization::Granted,
        // The answer of a known action the user did not (or, asked without interaction, could
        // not yet) authorise.
        Ok((false, _, _)) => Authorization::Cancelled,
        Err(e) => {
            let text = e.to_string();
            if text.contains("not registered") || text.contains("Unknown action") {
                Authorization::Unregistered
            } else if text.contains("ServiceUnknown") || text.contains("NameHasNoOwner") {
                Authorization::Unavailable
            } else {
                crate::plog_warn!("[keyring] polkit CheckAuthorization failed: {}", text);
                Authorization::Error
            }
        }
    }
}

/// The start time of process `pid` in clock ticks since boot (`/proc/<pid>/stat`, field 22),
/// which polkit asks of a `unix-process` subject so a recycled pid cannot be taken for this one.
fn process_start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name is in parentheses and may hold spaces: the fields count from the last ')'.
    let after = &stat[stat.rfind(')')? + 1..];
    after.split_whitespace().nth(19)?.parse().ok()
}
