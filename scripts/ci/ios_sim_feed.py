#!/usr/bin/env python3
"""ios_sim_feed.py: what the iOS Simulator can be FED from a script, as pure command builders plus a small runner
(standard library only). It is what the self-test (scripts/ci/ios_sim_selftest.py) and the QA harness use to put a Simulator
into a state: a Face ID / Touch ID enrolled or not, a finger or face that matches or does not, a location, a privacy
decision, the appearance. The Simulator has NO accelerometer, gyroscope, magnetometer, barometer, camera or haptic engine:
nothing here can feed those; they need a real iPhone (scripts/ci/ios_phone_selftest.py).

    python3 scripts/ci/ios_sim_feed.py <udid|booted> biometric enroll|unenroll
    python3 scripts/ci/ios_sim_feed.py <udid|booted> biometric match|nomatch [--touch]
    python3 scripts/ci/ios_sim_feed.py <udid|booted> location set <lat> <lon>
    python3 scripts/ci/ios_sim_feed.py <udid|booted> privacy grant|revoke|reset <service> <bundle-id>
    python3 scripts/ci/ios_sim_feed.py <udid|booted> appearance light|dark

The biometric words are the notifications the Simulator app's Features menu posts (Face ID / Touch ID: Enrolled, Matching,
Non-matching), delivered with `notifyutil` inside the simulated device:
    enrol / unenrol   notifyutil -s com.apple.BiometricKit.enrollmentChanged 1|0   then   -p com.apple.BiometricKit.enrollmentChanged
    match / nomatch   notifyutil -p com.apple.BiometricKit_Sim.pearl.match|nomatch          (Face ID)
                      notifyutil -p com.apple.BiometricKit_Sim.fingerTouch.match|nomatch    (Touch ID)
The state and the post are TWO calls (one `notifyutil -s ... -p ...` call is not reliable on every runtime). A match posted
while NOTHING is waiting for a face is dropped: post it after the app has asked (the biometric sheet is up), which the
self-test does by reading the app's own log line first. The command names are those of the Simulator's BiometricKit
(Detox, Appium and wix/AppleSimulatorUtils use the same); the pairs are interchangeable on some runtimes (iOS 26.5 reported).
"""
import subprocess
import sys

ENROLLMENT = "com.apple.BiometricKit.enrollmentChanged"
FACE = "com.apple.BiometricKit_Sim.pearl"
TOUCH = "com.apple.BiometricKit_Sim.fingerTouch"

# `simctl privacy` services (xcrun simctl privacy --help): the ones the capabilities of the engine need.
PRIVACY_SERVICES = (
    "all", "calendar", "contacts-limited", "contacts", "location", "location-always", "photos-add", "photos",
    "media-library", "microphone", "motion", "reminders", "siri",
)
PRIVACY_ACTIONS = ("grant", "revoke", "reset")


def spawn(udid, *argv):
    """`xcrun simctl spawn <udid> <argv...>`: run a command INSIDE the simulated device."""
    return ["xcrun", "simctl", "spawn", udid, *argv]


def enrollment_commands(udid, enrolled):
    """Face ID / Touch ID enrolled (True) or not (False): the state, then the notification (two calls)."""
    state = "1" if enrolled else "0"
    return [spawn(udid, "notifyutil", "-s", ENROLLMENT, state), spawn(udid, "notifyutil", "-p", ENROLLMENT)]


def match_commands(udid, ok, touch=False):
    """A finger / face that matches (True) or does not (False), for a Touch ID (touch=True) or Face ID phone."""
    base = TOUCH if touch else FACE
    return [spawn(udid, "notifyutil", "-p", f"{base}.{'match' if ok else 'nomatch'}")]


def location_commands(udid, lat, lon):
    """A fixed location (`simctl location <udid> set <lat>,<lon>`)."""
    lat, lon = float(lat), float(lon)
    if not (-90.0 <= lat <= 90.0 and -180.0 <= lon <= 180.0):
        raise ValueError(f"not a coordinate: {lat},{lon}")
    return [["xcrun", "simctl", "location", udid, "set", f"{lat},{lon}"]]


def privacy_commands(udid, action, service, bundle_id):
    """`simctl privacy <udid> grant|revoke|reset <service> <bundle-id>`: the decision a user would make in the dialog."""
    if action not in PRIVACY_ACTIONS:
        raise ValueError(f"privacy action {action!r}: one of {PRIVACY_ACTIONS}")
    if service not in PRIVACY_SERVICES:
        raise ValueError(f"privacy service {service!r}: one of {PRIVACY_SERVICES}")
    return [["xcrun", "simctl", "privacy", udid, action, service, bundle_id]]


def appearance_commands(udid, mode):
    if mode not in ("light", "dark"):
        raise ValueError(f"appearance {mode!r}: light or dark")
    return [["xcrun", "simctl", "ui", udid, "appearance", mode]]


def run(commands, timeout=60):
    """Run the commands in order; stop at the first failure. -> (ok, [(argv, returncode, output)])"""
    log = []
    for argv in commands:
        try:
            p = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
        except (OSError, subprocess.TimeoutExpired) as e:
            log.append((argv, 127, f"{type(e).__name__}: {e}"))
            return False, log
        log.append((argv, p.returncode, (p.stdout + p.stderr).strip()))
        if p.returncode != 0:
            return False, log
    return True, log


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    udid, what, *rest = argv
    try:
        if what == "biometric" and rest and rest[0] in ("enroll", "unenroll"):
            commands = enrollment_commands(udid, rest[0] == "enroll")
        elif what == "biometric" and rest and rest[0] in ("match", "nomatch"):
            commands = match_commands(udid, rest[0] == "match", touch="--touch" in rest[1:])
        elif what == "location" and len(rest) == 3 and rest[0] == "set":
            commands = location_commands(udid, rest[1], rest[2])
        elif what == "privacy" and len(rest) == 3:
            commands = privacy_commands(udid, *rest)
        elif what == "appearance" and len(rest) == 1:
            commands = appearance_commands(udid, rest[0])
        else:
            print(__doc__)
            return 2
    except ValueError as e:
        print(f"ios_sim_feed: {e}")
        return 2
    ok, log = run(commands)
    for cmd, code, out in log:
        print(f"$ {' '.join(cmd)}  -> {code}" + (f"\n{out}" if out else ""))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
