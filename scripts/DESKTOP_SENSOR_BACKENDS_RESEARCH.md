# Desktop sensor/auth backends — researched 2026-05-21 (tasks #8/#9/#10)

Wiring order: motion sensors → keyring → biometric (each Windows → Linux; macОS done). Feed the managers via the existing `push_*` fns; mirror `apple.rs` (spawn thread → blocking OS call → `push_*_result`). Backends live in `dll/src/desktop/extra/<name>/`; dispatch in each `mod.rs` (Windows/Linux currently stub to Unavailable). Add deps under `dll/Cargo.toml` `[target.'cfg(target_os="windows")'.dependencies]` (~line 237).

## 1. Windows motion sensors (task #8) — WinRT Windows.Devices.Sensors
- **Dep:** `windows = { version = "0.62", features = ["Devices_Sensors"] }` (windows cfg block). The `windows` crate CROSS-COMPILES to x86_64-pc-windows-gnu (bundled `windows_x86_64_gnu` import lib; `windows-sys` is Win32-only, won't reach the sensor classes). cargo check is the gate, links too.
- **API:** `Accelerometer::GetDefault() -> Result<Accelerometer>`; `.GetCurrentReading() -> Result<AccelerometerReading>`; `.AccelerationX/Y/Z() -> Result<f64>`. Same shape: `Gyrometer`/`AngularVelocityX/Y/Z`, `Magnetometer`/`MagneticFieldX/Y/Z` (f32). POLL each frame (MS-preferred for frame-rate UIs); call `SetReportInterval(MinimumReportInterval())` once in start().
- **Units → azul SensorReading:** accel g→m/s² ×9.80665; gyro deg/s→rad/s ×(π/180); mag µT→µT ×1. (Compass/Inclinometer are headings, skip.)
- **Graceful no-op:** `GetDefault().ok()` (None on no sensor) + every reading/axis `if let Ok(...)`. Never unwrap. SensorKind=Accelerometer/Gyroscope/Magnetometer.
- **Init:** UI thread usually COM-init'd; optional `RoInitialize(RO_INIT_MULTITHREADED)` (feat Win32_System_WinRT), ignore RPC_E_CHANGED_MODE.
- **Files:** new `sensors/windows.rs` (start()/poll() like apple/linux); wire empty Windows arms in `sensors/mod.rs`; `push_sensor_reading` exists (layout/src/managers/sensors.rs).

## 2. Windows keyring (task #9) — Win32 Credential Manager (winapi 0.3.9, VERIFIED compiles gnu)
- **Dep:** `winapi` features add `["wincred","minwindef","errhandlingapi","winerror"]` (links advapi32, bundled on gnu).
- **API (winapi::um::wincred):** `CredWriteW(PCREDENTIALW, 0)->BOOL`, `CredReadW(LPCWSTR target, CRED_TYPE_GENERIC=1, 0, *mut PCREDENTIALW)->BOOL`, `CredDeleteW(target, 1, 0)->BOOL`, `CredFree(PVOID)`. `CREDENTIALW{Flags,Type,TargetName(LPWSTR),Comment,LastWritten,CredentialBlobSize(DWORD),CredentialBlob(LPBYTE),Persist,AttributeCount,Attributes,TargetAlias,UserName}`.
- **Store:** Type=CRED_TYPE_GENERIC; TargetName=UTF-16LE NUL-term wide; CredentialBlob=secret UTF-8 bytes + Size (≤2560); Persist=CRED_PERSIST_LOCAL_MACHINE(2); UserName=non-empty wide (e.g. "azul-vault"). **Read:** CredReadW→one block; copy CredentialBlob[0..Size]→Vec; CredFree(pcred). **Delete:** CredDeleteW. On FALSE→GetLastError; ERROR_NOT_FOUND=1168→NotFound (not error).
- **Files:** new `keyring/windows.rs`; wire in `keyring/mod.rs`; map Store→Stored, Get→Retrieved/NotFound, Delete→Deleted; feed via `push_keyring_result` (mirror apple.rs thread).

## 3. Windows biometric (task #10) — Windows Hello UserConsentVerifier (WinRT, gnu OK)
- **Dep:** `windows` features add `["Foundation","Security_Credentials_UI","Win32_Foundation","Win32_System_WinRT","Win32_System_Com"]`.
- **Availability (no HWND):** `UserConsentVerifier::CheckAvailabilityAsync()?.get()? -> UserConsentVerifierAvailability`. Available→BiometricKind::Fingerprint (Hello can't say face vs finger), else NotAvailable.
- **Verify (DESKTOP needs HWND):** `factory::<UserConsentVerifier, IUserConsentVerifierInterop>()?` then `interop.RequestVerificationForWindowAsync(HWND(hwnd), &HSTRING)?.get()?`. Direct RequestVerificationAsync FAILS on Win32 (no CoreWindow). Result: Verified→Authenticated, Canceled→Cancelled, RetriesExhausted→Failed, DeviceBusy→Error, else Unavailable. MTA worker thread; `.get()` blocks (don't on UI/STA). **Integration wrinkle:** thread the app's top-level HWND into the biometric dispatcher's Windows verify arm (availability needs none). No Win32 alternative to Hello.
- **Files:** new `biometric/windows.rs`; wire `biometric/mod.rs`; `push_biometric_result`.

## 4. Linux keyring (task #9) — libsecret via dlopen
- **dlopen** `libsecret-1.so.0` (+ `libglib-2.0.so.0`) with the repo's `Library`/`load_symbol!`/`load_first_available` (re-export from x11::dlopen::Library; template = shell2/linux/dbus/dlopen.rs).
- **Use the NON-variadic `*v_sync` forms** (stable Rust can't call variadic fn-ptrs): `secret_password_storev_sync(*const SecretSchema, GHashTable* attrs, *const c_char collection, label, password, GCancellable* NULL, GError** ) -> gboolean`; `secret_password_lookupv_sync(schema, attrs, NULL, GError**) -> *mut c_char` (NULL=notfound-or-error; free with `secret_password_free`); `secret_password_clearv_sync(schema, attrs, NULL, GError**) -> gboolean`.
- **SecretSchema repr(C):** `{name:*const c_char, flags:c_uint(SECRET_SCHEMA_NONE=0), attributes:[SecretSchemaAttribute;32], reserved:c_int, reserved1..7:*mut c_void}`; `SecretSchemaAttribute{name:*const c_char, type_:c_uint(STRING=0)}`. One attr "key". collection="default" or NULL.
- **GHashTable:** dlsym glib `g_hash_table_new(g_str_hash,g_str_equal)`, `g_hash_table_insert`, `g_hash_table_unref`; insert ("key", keyval). `GError{domain:u32,code:i32,message:*mut c_char}` free via `g_error_free`. (Alt: zero-attr fixed-arity transmute w/ trailing NULL works on x86_64-SysV, but `*v_sync` is portable — prefer it.)
- load() fail / *error set → KeyringResult::Unavailable. `require_biometry` has no libsecret equivalent (ignore). Files: `keyring/linux.rs`, wire `keyring/mod.rs`.

## 5. Linux biometric (task #10) — fprintd over D-Bus via zbus (already a dep!)
- **`zbus` (5.x, pure Rust, already in dll/Cargo.toml:129)** blocking API, **system bus**. Template = `geolocation/linux.rs` (zbus::blocking signal-loop). NOT PAM (interactive auth, wrong layer + libpam C dep).
- **Names:** svc `net.reactivated.Fprint`; Manager `/net/reactivated/Fprint/Manager` iface `net.reactivated.Fprint.Manager` (`GetDefaultDevice()->o`, `GetDevices()->ao`); Device `/net/reactivated/Fprint/Device/N` iface `net.reactivated.Fprint.Device`.
- **Availability probe (no prompt):** GetDefaultDevice (err NoSuchDevice→NotAvailable) → Device.`ListEnrolledFingers("")->as` (empty→NotAvailable, else Fingerprint). "" = current user (no polkit).
- **Verify:** Device.`Claim("")`; subscribe `VerifyStatus(s result, b done)` BEFORE `VerifyStart("any")`; loop signals until done=true; always `VerifyStop()`+`Release()`. result strings: verify-match→Authenticated, verify-no-match→Failed, verify-disconnected/unknown→Error; done=false transient (keep waiting). fprintd draws NO UI (app shows "touch reader" from prompt.reason). Files: `biometric/linux.rs`, wire `biometric/mod.rs`; `push_biometric_result`.

## azul-vault fix (task #10 tail)
`examples/azul-vault` — make it use the public api.json codegen functions (not internal APIs). Review after the keyring/biometric backends land (the vault exercises them).
