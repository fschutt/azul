//! Windows geolocation backend (MWA-C-geolocation): classic COM Location
//! API (`LocationApi.dll`, Windows 7+, still present on Win11).
//!
//! Dependency policy D9 (dlopen + hand-rolled, no windows-rs) rules out
//! the WinRT `Windows.Devices.Geolocation.Geolocator` route for now — its
//! `PositionChanged` event needs combase activation plus hand-written
//! `IAsyncOperation`/event-token vtables. The classic COM `ILocation`
//! interface gives the same GPS/WiFi/IP position source through two
//! small vtables and a poll:
//!
//! ```text
//! CoInitializeEx → CoCreateInstance(CLSID_Location)
//!   → ILocation::RequestPermissions(NULL, &IID_ILatLongReport, 1, FALSE)
//!   → loop: ILocation::GetReport(IID_ILatLongReport) → ILatLongReport::
//!       GetLatitude/GetLongitude/GetErrorRadius/GetAltitude/GetAltitudeError
//!   → push_location_fix(...)
//! ```
//!
//! The poll runs on a one-shot worker thread (same lifecycle pattern as
//! the GeoClue thread in `linux.rs`: Subscribe starts it, Release sets a
//! stop flag, Reconfigure restarts). Results park in the process-global
//! channel; the capability pump (MWA-A1b) folds them into the manager and
//! fires `GeolocationFix` events. Poll cadence 1s — the Location API
//! coalesces sensor updates internally, and the pump's 200ms drain cadence
//! bounds delivery latency after that.
//!
//! The location consent verdict itself is read by the permission backend
//! (`extra/permission/windows.rs`, ConsentStore registry); `GetReport`
//! failing with access-denied simply produces no fixes here.

use azul_layout::managers::geolocation::GeolocationDiffEvent;

pub fn handle_event(event: &GeolocationDiffEvent) {
    imp::handle_event(event);
}

mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use azul_layout::managers::geolocation::{
        push_location_fix, GeolocationDiffEvent, LocationFix,
    };

    /// Stop flag of the currently-running poll thread (if any) — same
    /// lifecycle shape as the GeoClue loop in linux.rs.
    static ACTIVE: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

    pub fn handle_event(event: &GeolocationDiffEvent) {
        match event {
            GeolocationDiffEvent::Subscribe { .. } => start(),
            GeolocationDiffEvent::Reconfigure { .. } => {
                // Accuracy is a hint only in the classic API; restart keeps
                // the lifecycle simple and matches linux.rs.
                stop();
                start();
            }
            GeolocationDiffEvent::Release => stop(),
        }
    }

    fn start() {
        let mut active = match ACTIVE.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if active.is_some() {
            return; // already subscribed
        }
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        std::thread::spawn(move || location_poll_loop(&stop_thread));
        *active = Some(stop);
    }

    fn stop() {
        if let Ok(mut active) = ACTIVE.lock() {
            if let Some(flag) = active.take() {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }

    // ---- minimal COM plumbing (D9: dlopen ole32/LocationApi, hand-rolled
    // vtables; no windows-rs) ------------------------------------------------

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    /// CLSID_Location {E5B8E079-EE6D-4E33-A438-C87F2E959254}
    const CLSID_LOCATION: Guid = Guid {
        data1: 0xE5B8_E079,
        data2: 0xEE6D,
        data3: 0x4E33,
        data4: [0xA4, 0x38, 0xC8, 0x7F, 0x2E, 0x95, 0x92, 0x54],
    };
    /// IID_ILocation {AB2ECE69-56D9-4F28-B525-DE1B0EE44237}
    const IID_ILOCATION: Guid = Guid {
        data1: 0xAB2E_CE69,
        data2: 0x56D9,
        data3: 0x4F28,
        data4: [0xB5, 0x25, 0xDE, 0x1B, 0x0E, 0xE4, 0x42, 0x37],
    };
    /// IID_ILatLongReport {7FED806D-0EF8-4F91-AB6C-BB7AE307AF73}
    const IID_ILATLONG_REPORT: Guid = Guid {
        data1: 0x7FED_806D,
        data2: 0x0EF8,
        data3: 0x4F91,
        data4: [0xAB, 0x6C, 0xBB, 0x7A, 0xE3, 0x07, 0xAF, 0x73],
    };

    type Hresult = i32;
    type ComPtr = *mut core::ffi::c_void;

    /// ILocation vtable (IUnknown + the documented method order of
    /// locationapi.h — do NOT reorder).
    #[repr(C)]
    struct ILocationVtbl {
        query_interface: unsafe extern "system" fn(ComPtr, *const Guid, *mut ComPtr) -> Hresult,
        add_ref: unsafe extern "system" fn(ComPtr) -> u32,
        release: unsafe extern "system" fn(ComPtr) -> u32,
        register_for_report:
            unsafe extern "system" fn(ComPtr, ComPtr, *const Guid, u32) -> Hresult,
        unregister_for_report: unsafe extern "system" fn(ComPtr, *const Guid) -> Hresult,
        get_report: unsafe extern "system" fn(ComPtr, *const Guid, *mut ComPtr) -> Hresult,
        get_report_status: unsafe extern "system" fn(ComPtr, *const Guid, *mut u32) -> Hresult,
        get_report_interval: unsafe extern "system" fn(ComPtr, *const Guid, *mut u32) -> Hresult,
        set_report_interval: unsafe extern "system" fn(ComPtr, *const Guid, u32) -> Hresult,
        get_desired_accuracy: unsafe extern "system" fn(ComPtr, *const Guid, *mut u32) -> Hresult,
        set_desired_accuracy: unsafe extern "system" fn(ComPtr, *const Guid, u32) -> Hresult,
        request_permissions:
            unsafe extern "system" fn(ComPtr, ComPtr, *mut Guid, u32, i32) -> Hresult,
    }

    /// ILatLongReport vtable: IUnknown + ILocationReport (GetSensorID,
    /// GetTimestamp, GetValue) + the lat/long getters.
    #[repr(C)]
    struct ILatLongReportVtbl {
        query_interface: unsafe extern "system" fn(ComPtr, *const Guid, *mut ComPtr) -> Hresult,
        add_ref: unsafe extern "system" fn(ComPtr) -> u32,
        release: unsafe extern "system" fn(ComPtr) -> u32,
        get_sensor_id: unsafe extern "system" fn(ComPtr, *mut Guid) -> Hresult,
        get_timestamp: unsafe extern "system" fn(ComPtr, *mut Systemtime) -> Hresult,
        get_value:
            unsafe extern "system" fn(ComPtr, *const Guid, *mut core::ffi::c_void) -> Hresult,
        get_latitude: unsafe extern "system" fn(ComPtr, *mut f64) -> Hresult,
        get_longitude: unsafe extern "system" fn(ComPtr, *mut f64) -> Hresult,
        get_error_radius: unsafe extern "system" fn(ComPtr, *mut f64) -> Hresult,
        get_altitude: unsafe extern "system" fn(ComPtr, *mut f64) -> Hresult,
        get_altitude_error: unsafe extern "system" fn(ComPtr, *mut f64) -> Hresult,
    }

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Systemtime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    unsafe fn vtbl<T>(ptr: ComPtr) -> *const T {
        *(ptr as *const *const T)
    }

    /// COINIT_APARTMENTTHREADED — the Location API is an STA citizen.
    const COINIT_APARTMENTTHREADED: u32 = 0x2;
    const CLSCTX_INPROC_SERVER: u32 = 0x1;
    const S_OK: Hresult = 0;

    fn location_poll_loop(stop: &Arc<AtomicBool>) {
        type CoInitializeEx =
            unsafe extern "system" fn(*mut core::ffi::c_void, u32) -> Hresult;
        type CoUninitialize = unsafe extern "system" fn();
        type CoCreateInstance = unsafe extern "system" fn(
            *const Guid,
            ComPtr,
            u32,
            *const Guid,
            *mut ComPtr,
        ) -> Hresult;

        let Ok(ole32) = (unsafe { libloading::Library::new("ole32.dll") }) else {
            return;
        };
        let co_init = match unsafe { ole32.get::<CoInitializeEx>(b"CoInitializeEx\0") } {
            Ok(s) => s,
            Err(_) => return,
        };
        let co_uninit = match unsafe { ole32.get::<CoUninitialize>(b"CoUninitialize\0") } {
            Ok(s) => s,
            Err(_) => return,
        };
        let co_create = match unsafe { ole32.get::<CoCreateInstance>(b"CoCreateInstance\0") } {
            Ok(s) => s,
            Err(_) => return,
        };

        unsafe {
            let hr = co_init(core::ptr::null_mut(), COINIT_APARTMENTTHREADED);
            // S_FALSE (already initialized) is fine; real failures bail.
            if hr < 0 {
                return;
            }

            let mut location: ComPtr = core::ptr::null_mut();
            let hr = co_create(
                &CLSID_LOCATION,
                core::ptr::null_mut(),
                CLSCTX_INPROC_SERVER,
                &IID_ILOCATION,
                &mut location,
            );
            if hr != S_OK || location.is_null() {
                crate::plog_warn!(
                    "[geolocation] windows: Location API unavailable (hr={:#x}) — \
                     no fixes will be delivered",
                    hr
                );
                co_uninit();
                return;
            }
            let loc_vtbl = vtbl::<ILocationVtbl>(location);

            // Surface the consent prompt if needed (fWaitForPermissions =
            // FALSE keeps this non-blocking; a denied state just means
            // GetReport keeps failing below).
            let mut report_ids = [IID_ILATLONG_REPORT];
            let _ = ((*loc_vtbl).request_permissions)(
                location,
                core::ptr::null_mut(),
                report_ids.as_mut_ptr(),
                1,
                0,
            );

            while !stop.load(Ordering::Relaxed) {
                let mut report: ComPtr = core::ptr::null_mut();
                let hr = ((*loc_vtbl).get_report)(location, &IID_ILATLONG_REPORT, &mut report);
                if hr == S_OK && !report.is_null() {
                    let rep_vtbl = vtbl::<ILatLongReportVtbl>(report);
                    let mut lat = f64::NAN;
                    let mut lon = f64::NAN;
                    let mut err_radius = f64::NAN;
                    let mut alt = f64::NAN;
                    let mut alt_err = f64::NAN;
                    let lat_ok = ((*rep_vtbl).get_latitude)(report, &mut lat) == S_OK;
                    let lon_ok = ((*rep_vtbl).get_longitude)(report, &mut lon) == S_OK;
                    let _ = ((*rep_vtbl).get_error_radius)(report, &mut err_radius);
                    let _ = ((*rep_vtbl).get_altitude)(report, &mut alt);
                    let _ = ((*rep_vtbl).get_altitude_error)(report, &mut alt_err);

                    if lat_ok && lon_ok && lat.is_finite() && lon.is_finite() {
                        // MWA-C-geolocation: real wall-clock stamp (other
                        // backends hardcoded 0 — fixed alongside this).
                        let timestamp_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        push_location_fix(LocationFix {
                            latitude_deg: lat,
                            longitude_deg: lon,
                            accuracy_m: if err_radius.is_finite() {
                                err_radius as f32
                            } else {
                                f32::NAN
                            },
                            altitude_m: if alt.is_finite() { alt as f32 } else { f32::NAN },
                            altitude_accuracy_m: if alt_err.is_finite() {
                                alt_err as f32
                            } else {
                                f32::NAN
                            },
                            heading_deg: f32::NAN, // not exposed by ILatLongReport
                            speed_mps: f32::NAN,   // not exposed by ILatLongReport
                            timestamp_ms,
                        });
                    }
                    let _ = ((*rep_vtbl).release)(report);
                }
                std::thread::sleep(Duration::from_secs(1));
            }

            let _ = ((*loc_vtbl).release)(location);
            co_uninit();
        }
    }
}
