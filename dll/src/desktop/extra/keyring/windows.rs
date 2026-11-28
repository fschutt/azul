//! Windows keyring backend — Credential Manager (generic credentials) via the dlopen'd advapi32
//! `Cred*` functions, and Windows Hello for the protected items.
//!
//! An UNPROTECTED item is a generic credential under target `"<service>:<key>"`
//! (`CRED_TYPE_GENERIC`, the secret as the credential blob in UTF-8, `CRED_PERSIST_LOCAL_MACHINE`),
//! protected by DPAPI per logon.
//!
//! A PROTECTED item (`UserPresence`) is the same credential whose blob is the secret SEALED with
//! DPAPI, with the Windows Hello signature of a fixed challenge as the optional entropy, and whose
//! comment records the level ([`super::record`]). The signature is made by a per-item Hello key
//! (`KeyCredentialManager`, TPM-backed where the device has a TPM, named after the item) and
//! only exists after the user's Hello gesture (PIN or biometric): so the secret opens again only
//! after the gesture, and a process that reads the credential finds ciphertext. Reading the
//! record needs no gesture: [`need`] tells an app what a read needs before the sheet comes.
//!
//! Limits, said in `support()`: Windows Hello cannot require a biometric over the PIN, so
//! `Biometric` is `Unavailable` here (the app asks for `UserPresence`); the key is TPM-backed
//! only on a device with a TPM. A Hello key that is gone (Hello was reset) or signs differently
//! makes the item `Invalidated`.
//!
//! Written to Microsoft's documented API (`Windows.Security.Credentials.KeyCredentialManager`,
//! `CryptProtectData` with optional entropy); a CI runner has no Hello, so what it can run is
//! the pure part ([`super::record`]) and the plain credential path. The Hello sheet is parented
//! by the system to the foreground window.

use std::{future::IntoFuture, io, ptr};

use azul_core::keyring::{
    KeyringNeed, KeyringProtection, KeyringRequest, KeyringResult, KeyringSupport,
};
use windows::{
    core::HSTRING,
    Security::Credentials::{
        KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialOperationResult,
        KeyCredentialRetrievalResult,
    },
    Storage::Streams::{DataReader, DataWriter},
    Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED},
};
use winapi::{
    shared::{
        minwindef::{DWORD, FALSE, LPBYTE},
        winerror::ERROR_NOT_FOUND,
    },
    um::{
        dpapi::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN},
        errhandlingapi::GetLastError,
        wincred::{CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, PCREDENTIALW},
        wincrypt::DATA_BLOB,
        winbase::LocalFree,
    },
};

use super::{
    record::{
        comment_of, hello_account, hello_status, protection_of_comment, HelloStatus,
        HELLO_CHALLENGE,
    },
    Interaction,
};
use crate::desktop::shell2::windows::dlopen::{Advapi32Functions, Win32Libraries};

fn advapi32() -> io::Result<Advapi32Functions> {
    Win32Libraries::shared()
        .and_then(|win32| win32.advapi32)
        .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "advapi32 credential API unavailable"))
}

/// One op on the credentials of `service`, on the calling thread. Only a `UserCheck` op shows
/// the Hello sheet (the dispatcher refuses the others before it gets here).
pub(super) fn handle(service: &str, req: &KeyringRequest, interaction: Interaction) -> KeyringResult {
    match req {
        KeyringRequest::Store {
            key,
            secret,
            protection,
            ..
        } => match protection {
            KeyringProtection::DeviceUnlocked => {
                let was_protected = is_protected(service, key.as_str());
                match store(service, key.as_str(), secret.as_str().as_bytes(), None) {
                    Ok(()) => {
                        // An item that was protected before is plain now: its Hello key goes.
                        if was_protected {
                            hello_delete(&hello_account(service, key.as_str()));
                        }
                        KeyringResult::Stored
                    }
                    Err(e) => store_failed(key.as_str(), &e),
                }
            }
            // Hello cannot require a biometric over the PIN: refuse rather than store weaker.
            KeyringProtection::Biometric => KeyringResult::Unavailable,
            KeyringProtection::UserPresence => {
                if interaction.user_check() {
                    store_protected(service, key.as_str(), secret.as_str())
                } else {
                    KeyringResult::WouldPrompt
                }
            }
        },
        KeyringRequest::Get { key } => match read(service, key.as_str()) {
            Ok(Some(record)) if record.protection.needs_user() => {
                match hello_key(&hello_account(service, key.as_str())) {
                    HelloKey::Present => KeyringResult::WouldPrompt,
                    HelloKey::Missing => KeyringResult::Invalidated,
                    // No word from Windows Hello: not a reason to call the secret gone.
                    HelloKey::Unknown => KeyringResult::Error,
                }
            }
            Ok(Some(record)) => plain_text(record.blob),
            Ok(None) => KeyringResult::NotFound,
            Err(_) => KeyringResult::Error,
        },
        KeyringRequest::Unlock { key, prompt } => match read(service, key.as_str()) {
            Ok(Some(record)) if record.protection.needs_user() => {
                let _ = prompt; // Windows Hello draws its own sheet: the app's text has no place in it.
                unlock_protected(service, key.as_str(), &record.blob)
            }
            Ok(Some(record)) => plain_text(record.blob),
            Ok(None) => KeyringResult::NotFound,
            Err(_) => KeyringResult::Error,
        },
        KeyringRequest::Delete { key } => {
            let was_protected = is_protected(service, key.as_str());
            match delete(service, key.as_str()) {
                // Idempotent: a missing target also reports Deleted.
                Ok(_) => {
                    if was_protected {
                        hello_delete(&hello_account(service, key.as_str()));
                    }
                    KeyringResult::Deleted
                }
                Err(_) => KeyringResult::Error,
            }
        }
    }
}

/// Whether the credential `service:key` is there and protected (it has a Hello key to remove).
fn is_protected(service: &str, key: &str) -> bool {
    matches!(read(service, key), Ok(Some(record)) if record.protection.needs_user())
}

fn store_failed(key: &str, e: &io::Error) -> KeyringResult {
    // Not the secret: the name of the entry and why (a secret over what one entry
    // holds is told with both sizes).
    crate::plog_warn!(
        "[keyring] storing \"{}\" in the Credential Manager failed: {}",
        key,
        e
    );
    KeyringResult::Error
}

fn plain_text(bytes: Vec<u8>) -> KeyringResult {
    match String::from_utf8(bytes) {
        Ok(s) => KeyringResult::Retrieved(s.into()),
        Err(_) => KeyringResult::Error,
    }
}

/// What a read of `key` needs: the credential's own record, never a gesture.
pub(super) fn needs_prompt(service: &str, key: &str) -> KeyringNeed {
    match read(service, key) {
        Ok(None) => KeyringNeed::NotFound,
        Ok(Some(record)) => match record.protection {
            KeyringProtection::DeviceUnlocked => KeyringNeed::Nothing,
            protection => match hello_key(&hello_account(service, key)) {
                HelloKey::Present => match protection {
                    KeyringProtection::Biometric => KeyringNeed::Biometric,
                    _ => KeyringNeed::UserPresence,
                },
                HelloKey::Missing => KeyringNeed::Invalidated,
                HelloKey::Unknown => KeyringNeed::Error,
            },
        },
        Err(_) => KeyringNeed::Error,
    }
}

/// What Windows does for `protection`: the Credential Manager for the plain level; Hello for
/// `UserPresence`; nothing for `Biometric` (Hello takes a PIN as readily as a finger).
pub(super) fn support(protection: KeyringProtection) -> KeyringSupport {
    match protection {
        KeyringProtection::DeviceUnlocked => {
            if advapi32().is_ok() {
                KeyringSupport::Library
            } else {
                KeyringSupport::Unavailable
            }
        }
        KeyringProtection::Biometric => KeyringSupport::Unavailable,
        // A Hello key is held by the TPM where the device has one; azul does not read which kind
        // it got, so it does not claim the secret is out of reach of the user's own session.
        KeyringProtection::UserPresence => match on_mta_thread(hello_supported) {
            Some(Ok(true)) => KeyringSupport::Library,
            Some(Ok(false)) => KeyringSupport::NotEnrolled,
            _ => KeyringSupport::Unavailable,
        },
    }
}

/// UTF-16LE, NUL-terminated `"<service>:<key>"` for the credential TargetName.
fn target_of(service: &str, key: &str) -> Vec<u16> {
    format!("{service}:{key}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

/// A credential as read: its blob and the level its comment records.
struct Record {
    blob: Vec<u8>,
    protection: KeyringProtection,
}

/// Writes the credential `service:key` with `blob`; `comment`: the record of a protected item.
fn store(service: &str, key: &str, blob: &[u8], comment: Option<&str>) -> io::Result<()> {
    // CredWriteW refuses a longer blob with an error nobody can read; say what is wrong (AZ-W16).
    if let Some(why) = super::blob_refusal(blob.len()) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, why));
    }
    let advapi32 = advapi32()?;
    let mut target_w = target_of(service, key);
    let mut user_w: Vec<u16> = service.encode_utf16().chain(std::iter::once(0)).collect();
    let mut comment_w: Vec<u16> = comment
        .map(|text| text.encode_utf16().chain(std::iter::once(0)).collect())
        .unwrap_or_default();
    let mut cred: CREDENTIALW = unsafe { std::mem::zeroed() };
    cred.Type = CRED_TYPE_GENERIC;
    cred.TargetName = target_w.as_mut_ptr();
    cred.CredentialBlobSize = blob.len() as DWORD; // <= CREDENTIAL_BLOB_MAX, checked above
    cred.CredentialBlob = blob.as_ptr() as LPBYTE;
    cred.Persist = CRED_PERSIST_LOCAL_MACHINE;
    cred.UserName = user_w.as_mut_ptr(); // must be non-empty for generic creds
    if !comment_w.is_empty() {
        cred.Comment = comment_w.as_mut_ptr();
    }
    let ok = unsafe { (advapi32.CredWriteW)(&mut cred as PCREDENTIALW, 0) };
    if ok == FALSE {
        return Err(io::Error::from_raw_os_error(
            unsafe { GetLastError() } as i32
        ));
    }
    Ok(())
}

fn read(service: &str, key: &str) -> io::Result<Option<Record>> {
    let advapi32 = advapi32()?;
    let target_w = target_of(service, key);
    let mut pcred: PCREDENTIALW = ptr::null_mut();
    let ok = unsafe { (advapi32.CredReadW)(target_w.as_ptr(), CRED_TYPE_GENERIC, 0, &mut pcred) };
    if ok == FALSE {
        let err = unsafe { GetLastError() };
        if err == ERROR_NOT_FOUND {
            return Ok(None);
        }
        return Err(io::Error::from_raw_os_error(err as i32));
    }
    // pcred is one allocated block (inner ptrs point inside it); copy out then free.
    let record = unsafe {
        let c = &*pcred;
        let len = c.CredentialBlobSize as usize;
        let mut v = vec![0u8; len];
        if len > 0 {
            ptr::copy_nonoverlapping(c.CredentialBlob, v.as_mut_ptr(), len);
        }
        let comment = if c.Comment.is_null() {
            None
        } else {
            let mut n = 0usize;
            while *c.Comment.add(n) != 0 && n < 512 {
                n += 1;
            }
            Some(String::from_utf16_lossy(std::slice::from_raw_parts(c.Comment, n)))
        };
        Record {
            blob: v,
            protection: protection_of_comment(comment.as_deref()),
        }
    };
    unsafe { (advapi32.CredFree)(pcred.cast()) };
    Ok(Some(record))
}

fn delete(service: &str, key: &str) -> io::Result<bool> {
    let advapi32 = advapi32()?;
    let target_w = target_of(service, key);
    let ok = unsafe { (advapi32.CredDeleteW)(target_w.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if ok == FALSE {
        let err = unsafe { GetLastError() };
        if err == ERROR_NOT_FOUND {
            return Ok(false);
        }
        return Err(io::Error::from_raw_os_error(err as i32));
    }
    Ok(true)
}

// ───────── Windows Hello ────────────────────────────────────────────────

/// Why a Hello step did not give a signature.
#[derive(Debug)]
enum HelloFailure {
    Cancelled,
    LockedOut,
    NotFound,
    /// Hello is not set up on this device (no PIN / no security device).
    NotSet,
    Other(String),
}

impl HelloFailure {
    fn of_status(status: HelloStatus) -> Option<Self> {
        match status {
            HelloStatus::Success => None,
            HelloStatus::Cancelled => Some(Self::Cancelled),
            HelloStatus::LockedOut => Some(Self::LockedOut),
            HelloStatus::NotFound => Some(Self::NotFound),
            HelloStatus::Other => Some(Self::Other("Windows Hello answered with an error".into())),
        }
    }

    /// The keyring's answer for a failed `Store` / `Unlock`.
    fn result(&self) -> KeyringResult {
        match self {
            Self::Cancelled => KeyringResult::Cancelled,
            Self::LockedOut => KeyringResult::LockedOut,
            Self::NotFound => KeyringResult::Invalidated,
            Self::NotSet => KeyringResult::NotEnrolled,
            Self::Other(why) => {
                crate::plog_warn!("[keyring] Windows Hello failed: {}", why);
                KeyringResult::Error
            }
        }
    }
}

impl From<windows::core::Error> for HelloFailure {
    fn from(e: windows::core::Error) -> Self {
        Self::Other(e.to_string())
    }
}

/// Runs `f` on a thread of its own in the multithreaded COM apartment, where a WinRT call can
/// be waited for: the caller may be the UI thread (a single-threaded apartment). `None` when
/// the thread did not finish.
fn on_mta_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    std::thread::spawn(move || {
        // SAFETY: COM initialisation of this thread, once, before its first WinRT call.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        f()
    })
    .join()
    .ok()
}

fn hello_supported() -> Result<bool, HelloFailure> {
    Ok(pollster::block_on(KeyCredentialManager::IsSupportedAsync()?.into_future())?)
}

/// What Windows Hello says about the key of an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelloKey {
    /// The key is there.
    Present,
    /// Windows says there is no key of that name (Hello was reset, or the key was removed).
    Missing,
    /// Windows did not say (a failed call, a locked security device): nothing is known.
    Unknown,
}

/// Whether the Hello key `account` exists (opening it shows nothing). Only a `NotFound` answer
/// says it is gone; a failed call is `Unknown`.
fn hello_key(account: &str) -> HelloKey {
    let account = HSTRING::from(account);
    on_mta_thread(move || {
        let open = || -> windows::core::Result<HelloKey> {
            let result: KeyCredentialRetrievalResult =
                pollster::block_on(KeyCredentialManager::OpenAsync(&account)?.into_future())?;
            Ok(match hello_status(result.Status()?.0) {
                HelloStatus::Success => HelloKey::Present,
                HelloStatus::NotFound => HelloKey::Missing,
                _ => HelloKey::Unknown,
            })
        };
        open().unwrap_or(HelloKey::Unknown)
    })
    .unwrap_or(HelloKey::Unknown)
}

/// Removes the Hello key `account`, if there is one (best effort: it is a key, not a secret).
fn hello_delete(account: &str) {
    let account = HSTRING::from(account);
    let _ = on_mta_thread(move || {
        if let Ok(action) = KeyCredentialManager::DeleteAsync(&account) {
            let _ = pollster::block_on(action.into_future());
        }
    });
}

/// The signature of the fixed challenge by the Hello key `account`: made new (`create`: the
/// user's gesture creates the key and signs) or the existing one (the gesture signs). The bytes
/// are the entropy a secret is sealed with.
fn hello_signature(account: &str, create: bool) -> Result<Vec<u8>, HelloFailure> {
    let account = HSTRING::from(account);
    on_mta_thread(move || -> Result<Vec<u8>, HelloFailure> {
        if create && !hello_supported()? {
            return Err(HelloFailure::NotSet);
        }
        let retrieval: KeyCredentialRetrievalResult = if create {
            // Storing again keeps the item's Hello key: replacing it first would leave the item
            // that is there unreadable when the user cancels the sheet that follows. A key is
            // made only when there is none.
            let existing: KeyCredentialRetrievalResult =
                pollster::block_on(KeyCredentialManager::OpenAsync(&account)?.into_future())?;
            if hello_status(existing.Status()?.0) == HelloStatus::Success {
                existing
            } else {
                pollster::block_on(
                    KeyCredentialManager::RequestCreateAsync(
                        &account,
                        KeyCredentialCreationOption::ReplaceExisting,
                    )?
                    .into_future(),
                )?
            }
        } else {
            pollster::block_on(KeyCredentialManager::OpenAsync(&account)?.into_future())?
        };
        if let Some(failure) = HelloFailure::of_status(hello_status(retrieval.Status()?.0)) {
            return Err(failure);
        }
        let credential = retrieval.Credential()?;
        let writer = DataWriter::new()?;
        writer.WriteBytes(HELLO_CHALLENGE)?;
        let challenge = writer.DetachBuffer()?;
        let signed: KeyCredentialOperationResult =
            pollster::block_on(credential.RequestSignAsync(&challenge)?.into_future())?;
        if let Some(failure) = HelloFailure::of_status(hello_status(signed.Status()?.0)) {
            return Err(failure);
        }
        let buffer = signed.Result()?;
        let reader = DataReader::FromBuffer(&buffer)?;
        let mut bytes = vec![0u8; reader.UnconsumedBufferLength()? as usize];
        reader.ReadBytes(&mut bytes)?;
        if bytes.is_empty() {
            return Err(HelloFailure::Other("Windows Hello signed to nothing".into()));
        }
        Ok(bytes)
    })
    .unwrap_or_else(|| Err(HelloFailure::Other("the Windows Hello thread ended".into())))
}

fn store_protected(service: &str, key: &str, secret: &str) -> KeyringResult {
    let account = hello_account(service, key);
    let entropy = match hello_signature(&account, true) {
        Ok(entropy) => entropy,
        Err(failure) => return failure.result(),
    };
    let sealed = match seal(secret.as_bytes(), &entropy) {
        Ok(sealed) => sealed,
        Err(e) => return store_failed(key, &e),
    };
    let comment = comment_of(KeyringProtection::UserPresence);
    match store(service, key, &sealed, comment.as_deref()) {
        Ok(()) => KeyringResult::Stored,
        Err(e) => store_failed(key, &e),
    }
}

fn unlock_protected(service: &str, key: &str, sealed: &[u8]) -> KeyringResult {
    let account = hello_account(service, key);
    let entropy = match hello_signature(&account, false) {
        Ok(entropy) => entropy,
        Err(failure) => return failure.result(),
    };
    match unseal(sealed, &entropy) {
        // The key signs differently now (Hello was reset and made a new key of the name): the
        // sealed secret is gone for good.
        Err(_) => KeyringResult::Invalidated,
        Ok(bytes) => plain_text(bytes),
    }
}

// ───────── DPAPI ────────────────────────────────────────────────────────

fn blob_of(bytes: &[u8]) -> DATA_BLOB {
    DATA_BLOB {
        cbData: bytes.len() as DWORD,
        pbData: bytes.as_ptr() as *mut u8,
    }
}

/// Seals `secret` for this user, with `entropy` that must be given again to open it.
fn seal(secret: &[u8], entropy: &[u8]) -> io::Result<Vec<u8>> {
    let mut input = blob_of(secret);
    let mut extra = blob_of(entropy);
    let mut out = DATA_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    // SAFETY: the blobs point into slices that outlive the call; `out` is filled by DPAPI and
    // freed below with LocalFree, as documented.
    let ok = unsafe {
        CryptProtectData(
            &mut input,
            ptr::null(),
            &mut extra,
            ptr::null_mut(),
            ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    };
    if ok == FALSE {
        return Err(io::Error::from_raw_os_error(unsafe { GetLastError() } as i32));
    }
    // SAFETY: DPAPI returned `cbData` bytes at `pbData`, one LocalAlloc block.
    let sealed = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    unsafe { LocalFree(out.pbData.cast()) };
    Ok(sealed)
}

/// Opens what [`seal`] sealed; an error when `entropy` is not the one it was sealed with.
fn unseal(sealed: &[u8], entropy: &[u8]) -> io::Result<Vec<u8>> {
    let mut input = blob_of(sealed);
    let mut extra = blob_of(entropy);
    let mut out = DATA_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    // SAFETY: as in `seal`.
    let ok = unsafe {
        CryptUnprotectData(
            &mut input,
            ptr::null_mut(),
            &mut extra,
            ptr::null_mut(),
            ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    };
    if ok == FALSE {
        return Err(io::Error::from_raw_os_error(unsafe { GetLastError() } as i32));
    }
    // SAFETY: DPAPI returned `cbData` bytes at `pbData`, one LocalAlloc block.
    let opened = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    unsafe { LocalFree(out.pbData.cast()) };
    Ok(opened)
}
