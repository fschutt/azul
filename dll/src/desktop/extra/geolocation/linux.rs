//! Linux geolocation backend - GeoClue2 over D-Bus (via `zbus`).
//!
//! On `Subscribe`, a background thread connects to the system bus, asks
//! `org.freedesktop.GeoClue2.Manager` for a `Client`, configures its accuracy +
//! `Start`s it, then pushes every `LocationUpdated` into azul-layout's channel
//! via `push_location_fix` (the same channel CoreLocation / Android feed). On
//! `Release` the thread is stopped; `Reconfigure` restarts it with the new
//! accuracy.
//!
//! Where GeoClue isn't present or has no location source (servers, minimal
//! desktops, the now-defunct wifi backend), the thread exits quietly and the
//! app simply sees no fix - the same observable behaviour as the old stub, but
//! real wherever GeoClue can resolve a position (GPS dongle, agent, portal).

#[cfg(target_os = "linux")]
pub use imp::handle_event;

#[cfg(not(target_os = "linux"))]
pub fn handle_event(event: &azul_layout::managers::geolocation::GeolocationDiffEvent) {
    let _ = event;
}

#[cfg(target_os = "linux")]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use azul_layout::managers::geolocation::{
        push_location_fix, GeolocationDiffEvent, LocationFix,
    };

    /// The stop flag of the currently-running GeoClue thread (if any).
    static ACTIVE: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

    pub fn handle_event(event: &GeolocationDiffEvent) {
        match event {
            GeolocationDiffEvent::Subscribe { config } => start(config.high_accuracy),
            GeolocationDiffEvent::Reconfigure { config } => {
                stop();
                start(config.high_accuracy);
            }
            GeolocationDiffEvent::Release => stop(),
        }
    }

    fn start(high_accuracy: bool) {
        let mut active = match ACTIVE.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if active.is_some() {
            return; // already subscribed
        }
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        std::thread::spawn(move || geoclue_loop(stop_thread, high_accuracy));
        *active = Some(stop);
    }

    fn stop() {
        if let Ok(mut active) = ACTIVE.lock() {
            if let Some(flag) = active.take() {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }

    /// Connect to GeoClue2, start a client, and push each `LocationUpdated` as a
    /// `LocationFix` until `stop` is set or the bus/GeoClue is unavailable.
    fn geoclue_loop(stop: Arc<AtomicBool>, high_accuracy: bool) {
        const SVC: &str = "org.freedesktop.GeoClue2";

        let conn = match zbus::blocking::Connection::system() {
            Ok(c) => c,
            Err(_) => return,
        };
        let manager = match zbus::blocking::Proxy::new(
            &conn,
            SVC,
            "/org/freedesktop/GeoClue2/Manager",
            "org.freedesktop.GeoClue2.Manager",
        ) {
            Ok(p) => p,
            Err(_) => return,
        };
        let client_path: zbus::zvariant::OwnedObjectPath = match manager.call("GetClient", &()) {
            Ok(p) => p,
            Err(_) => return,
        };
        let client = match zbus::blocking::Proxy::new(
            &conn,
            SVC,
            client_path.as_str(),
            "org.freedesktop.GeoClue2.Client",
        ) {
            Ok(p) => p,
            Err(_) => return,
        };

        let _ = client.set_property("DesktopId", "org.azul.app");
        // GClueAccuracyLevel: 6 = City, 8 = Exact.
        let _ = client.set_property(
            "RequestedAccuracyLevel",
            if high_accuracy { 8u32 } else { 6u32 },
        );
        let started: Result<(), _> = client.call("Start", &());
        // MWA-C-permission: geoclue is the Linux geolocation authority — its
        // Start outcome IS the permission answer (agent-mediated). Park it so
        // PermissionManager state stops sitting at NotDetermined while fixes
        // flow (or explains why they don't).
        {
            use azul_layout::managers::permission::{
                push_async_result, Capability, PermissionQuality, PermissionState,
            };
            match &started {
                Ok(()) => push_async_result(
                    Capability::Geolocation,
                    PermissionState::Granted {
                        quality: if high_accuracy {
                            PermissionQuality::Full
                        } else {
                            PermissionQuality::Reduced
                        },
                    },
                ),
                Err(_) => push_async_result(Capability::Geolocation, PermissionState::Denied),
            }
        }
        if started.is_err() {
            return;
        }

        let signals = match client.receive_signal("LocationUpdated") {
            Ok(s) => s,
            Err(_) => return,
        };
        for msg in signals {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let (_old, new): (
                zbus::zvariant::OwnedObjectPath,
                zbus::zvariant::OwnedObjectPath,
            ) = match msg.body().deserialize() {
                Ok(v) => v,
                Err(_) => continue,
            };
            let loc = match zbus::blocking::Proxy::new(
                &conn,
                SVC,
                new.as_str(),
                "org.freedesktop.GeoClue2.Location",
            ) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let lat: f64 = loc.get_property("Latitude").unwrap_or(f64::NAN);
            let lon: f64 = loc.get_property("Longitude").unwrap_or(f64::NAN);
            let acc: f64 = loc.get_property("Accuracy").unwrap_or(f64::NAN);
            let alt: f64 = loc.get_property("Altitude").unwrap_or(f64::NAN);
            let heading: f64 = loc.get_property("Heading").unwrap_or(-1.0);
            let speed: f64 = loc.get_property("Speed").unwrap_or(-1.0);

            push_location_fix(LocationFix {
                latitude_deg: lat,
                longitude_deg: lon,
                accuracy_m: acc as f32,
                altitude_m: alt as f32,
                altitude_accuracy_m: f32::NAN,
                heading_deg: if heading >= 0.0 { heading as f32 } else { f32::NAN },
                speed_mps: if speed >= 0.0 { speed as f32 } else { f32::NAN },
                timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0), // MWA-C-geolocation: was hardcoded 0,
            });
        }
        let _: Result<(), _> = client.call("Stop", &());
    }
}
