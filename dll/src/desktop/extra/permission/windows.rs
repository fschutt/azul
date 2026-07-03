//! Windows permission backend (MWA-C-permission): CapabilityAccessManager
//! consent store.
//!
//! Win32 (unpackaged) apps cannot pop the consent prompt themselves — it
//! appears on first device use, or the user flips the switch under
//! `ms-settings:privacy-<cap>`. What Win32 CAN do reliably since 1809 is
//! READ the per-user consent store the Settings app writes:
//!
//! ```text
//! HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\
//!      ConsentStore\{webcam,microphone,location,...}\Value = "Allow"|"Deny"
//! ```
//!
//! plus the machine-wide policy mirror under HKLM (Group Policy / MDM →
//! `Restricted`). This backend reads those via a dlopen'd
//! `advapi32!RegGetValueW` (dependency policy D9: dlopen + hand-rolled,
//! no windows-rs) and parks the result through `push_async_result`, where
//! the capability pump folds it into the `PermissionManager`.
//!
//! The WinRT `AppCapability.CheckAccessAsync` path (needed for PACKAGED
//! apps whose per-app grants live outside the global store) requires
//! hand-rolled combase activation + IAsyncOperation completion vtables —
//! recorded as a FOLLOW-UP in scripts/MANAGER_FIX_PROGRESS.md.
//!
//! Mapping: "Allow" → Granted{Full}; "Deny" → Denied; HKLM "Deny" →
//! Restricted; missing/other → NotDetermined.

use azul_layout::managers::permission::{
    push_async_result, Capability, PermissionDiffEvent, PermissionQuality, PermissionState,
};

pub fn handle_event(event: &PermissionDiffEvent) {
    match event {
        PermissionDiffEvent::Subscribe { capability, .. } => {
            // No prompt surface from Win32 — seed the manager with the
            // consent store's current answer so get_permission_status is
            // truthful. A Settings-app flip takes effect on the next
            // Subscribe.
            let state = probe_status(*capability);
            if state != PermissionState::NotDetermined {
                push_async_result(*capability, state);
            }
        }
        // Grants are user-scoped and persistent; nothing to release.
        PermissionDiffEvent::Release { .. } | PermissionDiffEvent::Reconfigure { .. } => {}
    }
}

pub fn probe_status(capability: Capability) -> PermissionState {
    let Some(subkey) = consent_store_subkey(capability) else {
        return PermissionState::NotDetermined;
    };
    // Machine policy first: an HKLM Deny means Group Policy / MDM blocks
    // the capability for every user → Restricted (no prompt can help).
    if let Some(v) = read_consent_value(HKEY_LOCAL_MACHINE, subkey) {
        if v.eq_ignore_ascii_case("Deny") {
            return PermissionState::Restricted;
        }
    }
    match read_consent_value(HKEY_CURRENT_USER, subkey).as_deref() {
        Some(v) if v.eq_ignore_ascii_case("Allow") => PermissionState::Granted {
            quality: PermissionQuality::Full,
        },
        Some(v) if v.eq_ignore_ascii_case("Deny") => PermissionState::Denied,
        _ => PermissionState::NotDetermined,
    }
}

/// ConsentStore subkey names (the Settings app's internal capability ids).
fn consent_store_subkey(capability: Capability) -> Option<&'static str> {
    Some(match capability {
        Capability::Camera => "webcam",
        Capability::Microphone => "microphone",
        Capability::Geolocation | Capability::GeolocationBackground => "location",
        // graphicsCaptureProgrammatic governs Windows.Graphics.Capture;
        // classic BitBlt/DXGI duplication is not consent-gated on Win32.
        Capability::ScreenCapture => "graphicsCaptureProgrammatic",
        _ => return None,
    })
}

type Hkey = *mut core::ffi::c_void;
const HKEY_CURRENT_USER: Hkey = 0x8000_0001_usize as Hkey;
const HKEY_LOCAL_MACHINE: Hkey = 0x8000_0002_usize as Hkey;
/// RRF_RT_REG_SZ
const RRF_RT_REG_SZ: u32 = 0x0000_0002;

/// Read `<root>\...\ConsentStore\<subkey>\Value` as a string via a
/// dlopen'd RegGetValueW (D9: no static advapi32 import-table entry).
fn read_consent_value(root: Hkey, subkey: &str) -> Option<String> {
    type RegGetValueW = unsafe extern "system" fn(
        Hkey,
        *const u16,             // lpSubKey
        *const u16,             // lpValue
        u32,                    // dwFlags
        *mut u32,               // pdwType
        *mut core::ffi::c_void, // pvData
        *mut u32,               // pcbData
    ) -> i32;

    let lib = unsafe { libloading::Library::new("advapi32.dll") }.ok()?;
    let reg_get_value: libloading::Symbol<'_, RegGetValueW> =
        unsafe { lib.get(b"RegGetValueW\0") }.ok()?;

    let path = format!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\{subkey}"
    );
    let path_w: Vec<u16> = path.encode_utf16().chain(core::iter::once(0)).collect();
    let value_w: Vec<u16> = "Value".encode_utf16().chain(core::iter::once(0)).collect();

    let mut buf = [0u16; 32];
    let mut cb: u32 = (buf.len() * 2) as u32;
    let status = unsafe {
        reg_get_value(
            root,
            path_w.as_ptr(),
            value_w.as_ptr(),
            RRF_RT_REG_SZ,
            core::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut cb,
        )
    };
    if status != 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}
