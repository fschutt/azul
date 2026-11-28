// Biometric authentication (fingerprint / face / iris).
//
// androidx.biometric is NOT available: this APK is built with javac + d8 and
// ships no AndroidX. So this uses the platform BiometricPrompt (API 28+) and
// degrades to "unavailable" below that, which is exactly what the Rust side
// already expects when the class is missing entirely.
//
// Result contract (biometric/android.rs): 0=Authenticated, 1=Failed,
// 2=Cancelled, 3=FellBackToPasscode, 4=Unavailable, 5=Error, 6=LockedOut,
// 7=NotEnrolled. Kind contract: 0=NotAvailable, 1=Fingerprint, 2=Face, 3=Iris.
//
// Every code names ONE cause: the sheet closed by the user (or the negative button, or the system) is Cancelled;
// too many wrong tries are LockedOut; nothing enrolled (no biometric, no device credential) is NotEnrolled; no sensor
// or an unusable one is Unavailable; Failed is kept for a platform that reports a terminal mismatch (Android does not:
// a rejected finger keeps the sheet up).

package com.azul.biometric;

import android.app.Activity;
import android.content.pm.PackageManager;
import android.hardware.biometrics.BiometricPrompt;
import android.os.Build;
import android.os.CancellationSignal;

public final class AzulBiometric {

    private AzulBiometric() {}

    private static final int RESULT_AUTHENTICATED = 0;
    private static final int RESULT_FAILED = 1;
    private static final int RESULT_CANCELLED = 2;
    private static final int RESULT_FALLBACK = 3;
    private static final int RESULT_UNAVAILABLE = 4;
    private static final int RESULT_ERROR = 5;
    private static final int RESULT_LOCKED_OUT = 6;
    private static final int RESULT_NOT_ENROLLED = 7;

    // android.hardware.biometrics.BiometricPrompt error codes (some are not public constants on API 28/29).
    private static final int ERR_HW_UNAVAILABLE = 1;
    private static final int ERR_TIMEOUT = 3;
    private static final int ERR_CANCELED = 5;
    private static final int ERR_LOCKOUT = 7;
    private static final int ERR_LOCKOUT_PERMANENT = 9;
    private static final int ERR_USER_CANCELED = 10;
    private static final int ERR_NO_BIOMETRICS = 11;
    private static final int ERR_HW_NOT_PRESENT = 12;
    private static final int ERR_NEGATIVE_BUTTON = 13;
    private static final int ERR_NO_DEVICE_CREDENTIAL = 14;

    /**
     * Which biometric the device can actually use, or 0 when none.
     *
     * Derived from PackageManager features rather than assumed: a device may
     * have a fingerprint reader, a face unlock, both, or a sensor that exists
     * but has nothing enrolled.
     */
    public static int canAuthenticate(Activity activity) {
        if (activity == null || Build.VERSION.SDK_INT < Build.VERSION_CODES.P) {
            return 0;
        }
        try {
            PackageManager pm = activity.getPackageManager();
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                // A feature flag is a claim of the device's build; the manager is the system's answer. No hardware
                // (or hardware that cannot be used) means no kind, whatever the flags say. NOT enrolled still has a
                // kind: the prompt answers NotEnrolled and the app can send the user to the settings.
                android.hardware.biometrics.BiometricManager bm =
                        activity.getSystemService(android.hardware.biometrics.BiometricManager.class);
                if (bm != null) {
                    int can = bm.canAuthenticate();
                    if (can == android.hardware.biometrics.BiometricManager.BIOMETRIC_ERROR_NO_HARDWARE
                            || can == android.hardware.biometrics.BiometricManager.BIOMETRIC_ERROR_HW_UNAVAILABLE) {
                        return 0;
                    }
                }
                if (pm.hasSystemFeature(PackageManager.FEATURE_FACE)) {
                    return 2;
                }
                if (pm.hasSystemFeature(PackageManager.FEATURE_IRIS)) {
                    return 3;
                }
            }
            if (pm.hasSystemFeature(PackageManager.FEATURE_FINGERPRINT)) {
                return 1;
            }
            return 0;
        } catch (Throwable t) {
            // A probe must never be the reason an app dies.
            return 0;
        }
    }

    /**
     * Show the system biometric prompt.
     *
     * `allowFallback` maps onto the device-credential (PIN/pattern/password)
     * escape hatch. Below API 30 there is no way to offer it without also
     * supplying a crypto object, so the negative button is used instead and a
     * press is reported as Cancelled rather than pretending it fell back.
     */
    public static void authenticate(Activity activity, long handle, String reason,
                                    String cancelLabel, boolean allowFallback) {
        if (activity == null) {
            nativeOnBiometricResult(handle, RESULT_UNAVAILABLE);
            return;
        }
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) {
            nativeOnBiometricResult(handle, RESULT_UNAVAILABLE);
            return;
        }
        activity.runOnUiThread(() -> {
            try {
                BiometricPrompt.Builder b = new BiometricPrompt.Builder(activity)
                        .setTitle(reason == null ? "" : reason);
                if (allowFallback && Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    b.setAllowedAuthenticators(
                            android.hardware.biometrics.BiometricManager.Authenticators
                                    .BIOMETRIC_WEAK
                            | android.hardware.biometrics.BiometricManager.Authenticators
                                    .DEVICE_CREDENTIAL);
                } else {
                    // A negative button is MANDATORY when no device-credential
                    // authenticator is allowed; the prompt throws without one.
                    b.setNegativeButton(
                            cancelLabel == null || cancelLabel.isEmpty() ? "Cancel" : cancelLabel,
                            activity.getMainExecutor(),
                            (dialog, which) -> nativeOnBiometricResult(handle, RESULT_CANCELLED));
                }
                b.build().authenticate(
                        new CancellationSignal(),
                        activity.getMainExecutor(),
                        new BiometricPrompt.AuthenticationCallback() {
                            @Override
                            public void onAuthenticationSucceeded(
                                    BiometricPrompt.AuthenticationResult result) {
                                // API 30+: the sheet says what the user used. Only a passcode that stood in for the
                                // biometric is a fall back (AUTHENTICATION_RESULT_TYPE_DEVICE_CREDENTIAL = 1).
                                boolean credential = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
                                        && result.getAuthenticationType()
                                                == BiometricPrompt.AUTHENTICATION_RESULT_TYPE_DEVICE_CREDENTIAL;
                                nativeOnBiometricResult(handle,
                                        credential ? RESULT_FALLBACK : RESULT_AUTHENTICATED);
                            }

                            @Override
                            public void onAuthenticationFailed() {
                                // A rejected finger, not the end of the attempt: the prompt stays up and the
                                // user tries again (the system shows its own "not recognized" feedback; too
                                // many tries end in onAuthenticationError with a lockout code). The native
                                // side resolves a request at its FIRST result, so reporting this one made the
                                // success that follows a dropped call: the user was authenticated and the app
                                // was told Failed. The terminal answer comes from succeeded/error only.
                            }

                            @Override
                            public void onAuthenticationError(int code, CharSequence msg) {
                                nativeOnBiometricResult(handle, mapError(code));
                            }
                        });
            } catch (Throwable t) {
                nativeOnBiometricResult(handle, RESULT_UNAVAILABLE);
            }
        });
    }

    /**
     * One cause per code. The user closing the sheet (back, outside, the negative button, a timeout) is Cancelled and
     * never a fall back to the passcode: a passcode is only reported when the system says it was USED (the success
     * callback above).
     */
    private static int mapError(int code) {
        switch (code) {
            case ERR_USER_CANCELED:
            case ERR_CANCELED:
            case ERR_NEGATIVE_BUTTON:
            case ERR_TIMEOUT:
                return RESULT_CANCELLED;
            case ERR_LOCKOUT:
            case ERR_LOCKOUT_PERMANENT:
                return RESULT_LOCKED_OUT;
            case ERR_NO_BIOMETRICS:
            case ERR_NO_DEVICE_CREDENTIAL:
                return RESULT_NOT_ENROLLED;
            case ERR_HW_NOT_PRESENT:
            case ERR_HW_UNAVAILABLE:
                return RESULT_UNAVAILABLE;
            default:
                return RESULT_ERROR;
        }
    }

    private static native void nativeOnBiometricResult(long handle, int resultCode);
}
