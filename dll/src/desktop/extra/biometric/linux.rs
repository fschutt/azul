//! Linux biometric backend — fprintd over D-Bus (system bus) via `zbus`.
//!
//! One-shot fingerprint verify -> `BiometricResult`; `ListEnrolledFingers`
//! probe -> `BiometricKind`. fprintd draws NO UI of its own (the app shows
//! "touch the reader" from `prompt.reason`). NOT PAM (that's an interactive
//! auth stack — wrong layer + a libpam C dep). Mirrors apple.rs (spawn thread
//! -> push_biometric_result) + geolocation/linux.rs (the zbus signal loop).

use azul_core::biometric::{BiometricKind, BiometricPrompt, BiometricResult};
use azul_layout::managers::biometric::push_biometric_result;

const SVC: &str = "net.reactivated.Fprint";
const MGR_PATH: &str = "/net/reactivated/Fprint/Manager";
const MGR_IFACE: &str = "net.reactivated.Fprint.Manager";
const DEV_IFACE: &str = "net.reactivated.Fprint.Device";

/// Capability probe (no prompt): a default device with >=1 enrolled finger.
pub fn probe_availability() -> BiometricKind {
    probe().unwrap_or(BiometricKind::NotAvailable)
}

fn probe() -> Option<BiometricKind> {
    use zbus::blocking::{Connection, Proxy};
    let conn = Connection::system().ok()?;
    let mgr = Proxy::new(&conn, SVC, MGR_PATH, MGR_IFACE).ok()?;
    // GetDefaultDevice errors (NoSuchDevice) when no reader -> NotAvailable.
    let dev_path: zbus::zvariant::OwnedObjectPath = mgr.call("GetDefaultDevice", &()).ok()?;
    let dev = Proxy::new(&conn, SVC, dev_path.as_str(), DEV_IFACE).ok()?;
    // "" = the calling user (no polkit). Empty list -> no enrolled finger.
    let enrolled: Vec<String> = dev.call("ListEnrolledFingers", &"").unwrap_or_default();
    Some(if enrolled.is_empty() {
        BiometricKind::NotAvailable
    } else {
        BiometricKind::Fingerprint
    })
}

/// Start a one-shot verify; deliver the outcome async via the result channel.
/// fprintd shows no modal — the app renders `prompt.reason` itself.
pub fn request(_prompt: &BiometricPrompt) {
    std::thread::spawn(|| {
        let result = run_verify().unwrap_or(BiometricResult::Unavailable);
        push_biometric_result(result);
    });
}

fn run_verify() -> Option<BiometricResult> {
    use zbus::blocking::{Connection, Proxy};
    let conn = Connection::system().ok()?;
    let mgr = Proxy::new(&conn, SVC, MGR_PATH, MGR_IFACE).ok()?;
    let dev_path: zbus::zvariant::OwnedObjectPath = mgr.call("GetDefaultDevice", &()).ok()?;
    let dev = Proxy::new(&conn, SVC, dev_path.as_str(), DEV_IFACE).ok()?;

    let enrolled: Vec<String> = dev.call("ListEnrolledFingers", &"").unwrap_or_default();
    if enrolled.is_empty() {
        return Some(BiometricResult::Unavailable);
    }

    // Claim ("" = current user). Subscribe to VerifyStatus BEFORE VerifyStart
    // to avoid missing a fast match.
    let claimed: Result<(), _> = dev.call("Claim", &"");
    if claimed.is_err() {
        return Some(BiometricResult::Unavailable); // busy / already claimed
    }
    let signals = match dev.receive_signal("VerifyStatus") {
        Ok(s) => s,
        Err(_) => {
            let _: Result<(), _> = dev.call("Release", &());
            return Some(BiometricResult::Error);
        }
    };
    let started: Result<(), _> = dev.call("VerifyStart", &"any"); // any enrolled finger
    if started.is_err() {
        let _: Result<(), _> = dev.call("Release", &());
        return Some(BiometricResult::Error);
    }

    // VerifyStatus(result: s, done: b) — wait for the terminal (done) status.
    let mut outcome = BiometricResult::Failed;
    for msg in signals {
        let (res, done): (String, bool) = match msg.body().deserialize() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if done {
            outcome = match res.as_str() {
                "verify-match" => BiometricResult::Authenticated,
                "verify-no-match" => BiometricResult::Failed,
                // verify-disconnected / verify-unknown-error / …
                _ => BiometricResult::Error,
            };
            break;
        }
        // done == false: transient (retry-scan / swipe-too-short / …) — keep waiting.
    }

    // Always pair VerifyStop + Release with Claim/VerifyStart, even on error.
    let _: Result<(), _> = dev.call("VerifyStop", &());
    let _: Result<(), _> = dev.call("Release", &());
    Some(outcome)
}
