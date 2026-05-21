//! Windows keyring backend — Credential Manager (generic credentials) via
//! winapi `wincred`. Mirrors apple.rs: each op runs on a spawned thread and
//! parks the outcome via `push_keyring_result`.
//!
//! Stores under target `"<SERVICE>:<key>"`, `CRED_TYPE_GENERIC`, the secret as
//! the credential blob (UTF-8 bytes), `CRED_PERSIST_LOCAL_MACHINE`. Generic
//! credentials are DPAPI-protected per logon but NOT biometry-gated (that's
//! Windows Hello — a separate backend), so `require_biometry` is ignored here.

use std::{io, ptr};

use azul_core::keyring::{KeyringRequest, KeyringResult};
use azul_layout::managers::keyring::push_keyring_result;
use winapi::shared::minwindef::{DWORD, FALSE, LPBYTE};
use winapi::shared::winerror::ERROR_NOT_FOUND;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::wincred::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
    CRED_TYPE_GENERIC, PCREDENTIALW,
};

const SERVICE: &str = "com.azul.keyring";

pub fn request(req: &KeyringRequest) {
    let req = req.clone();
    std::thread::spawn(move || {
        push_keyring_result(handle(&req));
    });
}

fn handle(req: &KeyringRequest) -> KeyringResult {
    match req {
        KeyringRequest::Store { key, secret, .. } => {
            match store(key.as_str(), secret.as_str().as_bytes()) {
                Ok(()) => KeyringResult::Stored,
                Err(_) => KeyringResult::Error,
            }
        }
        KeyringRequest::Get { key } => match read(key.as_str()) {
            Ok(Some(bytes)) => match String::from_utf8(bytes) {
                Ok(s) => KeyringResult::Retrieved(s.into()),
                Err(_) => KeyringResult::Error,
            },
            Ok(None) => KeyringResult::NotFound,
            Err(_) => KeyringResult::Error,
        },
        KeyringRequest::Delete { key } => match delete(key.as_str()) {
            // Idempotent: a missing target also reports Deleted.
            Ok(_) => KeyringResult::Deleted,
            Err(_) => KeyringResult::Error,
        },
    }
}

/// UTF-16LE, NUL-terminated `"<SERVICE>:<key>"` for the credential TargetName.
fn target_of(key: &str) -> Vec<u16> {
    format!("{SERVICE}:{key}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

fn store(key: &str, secret: &[u8]) -> io::Result<()> {
    let mut target_w = target_of(key);
    let mut user_w: Vec<u16> = SERVICE.encode_utf16().chain(std::iter::once(0)).collect();
    let mut cred: CREDENTIALW = unsafe { std::mem::zeroed() };
    cred.Type = CRED_TYPE_GENERIC;
    cred.TargetName = target_w.as_mut_ptr();
    cred.CredentialBlobSize = secret.len() as DWORD; // <= CRED_MAX_CREDENTIAL_BLOB_SIZE (2560)
    cred.CredentialBlob = secret.as_ptr() as LPBYTE;
    cred.Persist = CRED_PERSIST_LOCAL_MACHINE;
    cred.UserName = user_w.as_mut_ptr(); // must be non-empty for generic creds
    let ok = unsafe { CredWriteW(&mut cred as PCREDENTIALW, 0) };
    if ok == FALSE {
        return Err(io::Error::from_raw_os_error(unsafe { GetLastError() } as i32));
    }
    Ok(())
}

fn read(key: &str) -> io::Result<Option<Vec<u8>>> {
    let target_w = target_of(key);
    let mut pcred: PCREDENTIALW = ptr::null_mut();
    let ok = unsafe { CredReadW(target_w.as_ptr(), CRED_TYPE_GENERIC, 0, &mut pcred) };
    if ok == FALSE {
        let err = unsafe { GetLastError() };
        if err == ERROR_NOT_FOUND {
            return Ok(None);
        }
        return Err(io::Error::from_raw_os_error(err as i32));
    }
    // pcred is one allocated block (inner ptrs point inside it); copy out then free.
    let bytes = unsafe {
        let c = &*pcred;
        let len = c.CredentialBlobSize as usize;
        let mut v = vec![0u8; len];
        if len > 0 {
            ptr::copy_nonoverlapping(c.CredentialBlob, v.as_mut_ptr(), len);
        }
        v
    };
    unsafe { CredFree(pcred as *mut _) };
    Ok(Some(bytes))
}

fn delete(key: &str) -> io::Result<bool> {
    let target_w = target_of(key);
    let ok = unsafe { CredDeleteW(target_w.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if ok == FALSE {
        let err = unsafe { GetLastError() };
        if err == ERROR_NOT_FOUND {
            return Ok(false);
        }
        return Err(io::Error::from_raw_os_error(err as i32));
    }
    Ok(true)
}
