// Biometric authentication (fingerprint / face / iris).
//
// androidx.biometric is NOT available: this APK is built with javac + d8 and
// ships no AndroidX. So this uses the platform BiometricPrompt (API 28+) and
// degrades to "unavailable" below that, which is exactly what the Rust side
// already expects when the class is missing entirely.
//
// Result contract (biometric/android.rs): 0=Authenticated, 1=Failed,
// 2=Cancelled, 3=FellBackToPasscode, 4=Unavailable, other=Error.
// Kind contract: 0=NotAvailable, 1=Fingerprint, 2=Face, 3=Iris.

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
                                nativeOnBiometricResult(handle, RESULT_AUTHENTICATED);
                            }

                            @Override
                            public void onAuthenticationFailed() {
                                // A rejected finger, not the end of the attempt
                                // — the prompt stays up. Reported so the app can
                                // show its own feedback; the terminal answer
                                // still comes from succeeded/error.
                                nativeOnBiometricResult(handle, RESULT_FAILED);
                            }

                            @Override
                            public void onAuthenticationError(int code, CharSequence msg) {
                                int mapped;
                                switch (code) {
                                    case BiometricPrompt.BIOMETRIC_ERROR_USER_CANCELED:
                                    case BiometricPrompt.BIOMETRIC_ERROR_CANCELED:
                                        mapped = RESULT_CANCELLED;
                                        break;
                                    case BiometricPrompt.BIOMETRIC_ERROR_NO_BIOMETRICS:
                                    case BiometricPrompt.BIOMETRIC_ERROR_HW_NOT_PRESENT:
                                    case BiometricPrompt.BIOMETRIC_ERROR_HW_UNAVAILABLE:
                                        mapped = RESULT_UNAVAILABLE;
                                        break;
                                    // 13 == BIOMETRIC_ERROR_NEGATIVE_BUTTON. The
                                    // constant lives on the hidden
                                    // BiometricConstants interface and is not
                                    // public on the platform BiometricPrompt, so
                                    // the value is spelled out rather than
                                    // dropping the case.
                                    case 13:
                                        mapped = allowFallback ? RESULT_FALLBACK : RESULT_CANCELLED;
                                        break;
                                    default:
                                        mapped = RESULT_FAILED;
                                        break;
                                }
                                nativeOnBiometricResult(handle, mapped);
                            }
                        });
            } catch (Throwable t) {
                nativeOnBiometricResult(handle, RESULT_UNAVAILABLE);
            }
        });
    }

    private static native void nativeOnBiometricResult(long handle, int resultCode);
}
