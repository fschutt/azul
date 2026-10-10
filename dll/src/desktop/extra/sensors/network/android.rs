//! Android: `ConnectivityManager` through JNI - the activity's `connectivity` system service,
//! reached the way the haptics path reaches the vibrator. Android tells changes only to a Java
//! `NetworkCallback` subclass, which this path has none of, so a thread of its own reads the
//! service every [`POLL`] (a handful of JNI calls) and a query reads the last reading:
//!
//! - connected: `getActiveNetwork()` (API 23) is not null and its `NetworkCapabilities` have
//!   `NET_CAPABILITY_INTERNET`;
//! - metered: `isActiveNetworkMetered()`;
//! - constrained: `getRestrictBackgroundStatus()` (API 24) is `ENABLED`: Data Saver is on and
//!   the app is not let past it;
//! - the kind: `NetworkCapabilities.hasTransport` - Wi-Fi, cellular, Ethernet, anything else
//!   other.
//!
//! The app's manifest needs the `ACCESS_NETWORK_STATE` permission (a normal one, granted at
//! install); without it the service throws, and there is no reading ([`NetworkState::UNKNOWN`]).

use std::{sync::OnceLock, time::Duration};

use jni::{
    objects::{JObject, JValue},
    JNIEnv, JavaVM,
};

use super::{last_seen, seen, NetworkKind, NetworkState};

/// How often the thread reads the service.
const POLL: Duration = Duration::from_secs(10);

/// `NetworkCapabilities` constants.
const NET_CAPABILITY_INTERNET: i32 = 12;
const TRANSPORT_CELLULAR: i32 = 0;
const TRANSPORT_WIFI: i32 = 1;
const TRANSPORT_ETHERNET: i32 = 3;
/// `ConnectivityManager.RESTRICT_BACKGROUND_STATUS_ENABLED`.
const RESTRICT_BACKGROUND_STATUS_ENABLED: i32 = 3;

/// `caps.<method>(value)`, a `(I)Z` method of `NetworkCapabilities`; `false` when it throws.
fn caps_say(env: &mut JNIEnv<'_>, caps: &JObject<'_>, method: &str, value: i32) -> bool {
    match env
        .call_method(caps, method, "(I)Z", &[JValue::Int(value)])
        .and_then(|v| v.z())
    {
        Ok(answer) => answer,
        Err(_) => {
            let _ = env.exception_clear();
            false
        }
    }
}

/// The connectivity service's answer; `None` when it cannot be asked (an exception is left for
/// the caller to clear).
fn read_with(env: &mut JNIEnv<'_>, activity: &JObject<'_>) -> Option<NetworkState> {
    let name = JObject::from(env.new_string("connectivity").ok()?);
    let manager = env
        .call_method(
            activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&name)],
        )
        .and_then(|v| v.l())
        .ok()?;
    if manager.is_null() {
        return None;
    }
    let network = env
        .call_method(&manager, "getActiveNetwork", "()Landroid/net/Network;", &[])
        .and_then(|v| v.l())
        .ok()?;
    if network.is_null() {
        return Some(NetworkState::OFFLINE);
    }
    let caps = env
        .call_method(
            &manager,
            "getNetworkCapabilities",
            "(Landroid/net/Network;)Landroid/net/NetworkCapabilities;",
            &[JValue::Object(&network)],
        )
        .and_then(|v| v.l())
        .ok()?;
    if caps.is_null() {
        return Some(NetworkState::OFFLINE);
    }
    let connected = caps_say(env, &caps, "hasCapability", NET_CAPABILITY_INTERNET);
    let kind = if caps_say(env, &caps, "hasTransport", TRANSPORT_WIFI) {
        NetworkKind::WiFi
    } else if caps_say(env, &caps, "hasTransport", TRANSPORT_CELLULAR) {
        NetworkKind::Cellular
    } else if caps_say(env, &caps, "hasTransport", TRANSPORT_ETHERNET) {
        NetworkKind::Wired
    } else {
        NetworkKind::Other
    };
    let metered = env
        .call_method(&manager, "isActiveNetworkMetered", "()Z", &[])
        .and_then(|v| v.z())
        .ok()?;
    let constrained = match env
        .call_method(&manager, "getRestrictBackgroundStatus", "()I", &[])
        .and_then(|v| v.i())
    {
        Ok(status) => status == RESTRICT_BACKGROUND_STATUS_ENABLED,
        // Before API 24: no Data Saver.
        Err(_) => {
            let _ = env.exception_clear();
            false
        }
    };
    Some(NetworkState {
        kind,
        connected,
        metered,
        constrained,
    })
}

/// One reading on this thread, attached to the app's Java VM for it; `None` before the activity
/// published its VM and when the service cannot be asked.
fn reading() -> Option<NetworkState> {
    let vm_ptr = crate::desktop::shell2::android::java_vm_ptr();
    let activity_ptr = crate::desktop::shell2::android::activity_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return None;
    }
    // SAFETY: the VM and the activity (a global reference the activity layer keeps alive) the
    // activity published.
    let vm = unsafe { JavaVM::from_raw(vm_ptr.cast()) }.ok()?;
    // Detached again when the guard drops, which frees this reading's local references.
    let mut env = vm.attach_current_thread().ok()?;
    // SAFETY: as above.
    let activity = unsafe { JObject::from_raw(activity_ptr.cast()) };
    let state = read_with(&mut env, &activity);
    if state.is_none() {
        let _ = env.exception_clear();
    }
    state
}

/// Starts the monitor thread once.
fn start() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        let spawned = std::thread::Builder::new()
            .name(String::from("azul-network-monitor"))
            .spawn(|| loop {
                if let Some(state) = reading() {
                    seen(state);
                }
                std::thread::sleep(POLL);
            });
        if let Err(e) = spawned {
            crate::plog_warn!("[network] no monitor thread ({e}): no network readings");
        }
    });
}

/// The monitor's last reading; `None` before its first one and where the service cannot be
/// asked.
pub(super) fn read() -> Option<NetworkState> {
    start();
    last_seen()
}
