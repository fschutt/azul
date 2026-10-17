// Runtime permissions: the request, run on the UI thread, and the result,
// routed back into Rust.
//
// The REQUEST comes from Rust (permission/android.rs) on android_main's
// thread, but Activity.requestPermissions starts the system dialog's activity
// and belongs on the UI thread - so request() hops there. The RESULT arrives
// as a Java callback - Activity.onRequestPermissionsResult - which native code
// cannot receive, so it lands here and is forwarded.
//
// Lenient by construction: an unknown request code or an empty grant array is
// reported as "not granted" rather than throwing. A permission dialog the user
// dismissed must not take the app with it.

package com.azul.permission;

import android.app.Activity;

public final class AzulPermissions {

    private AzulPermissions() {}

    /**
     * Ask for runtime permissions, on the UI thread. Called from Rust on
     * android_main's thread; the answer arrives, as for any request, through
     * AzulActivity.onRequestPermissionsResult and onRequestPermissionsResultProxy.
     * A request the framework refuses to start is reported as a denial, so the
     * Rust side is never left waiting for an answer that cannot come.
     */
    public static void request(final Activity activity, final String[] permissions,
                               final int requestCode) {
        activity.runOnUiThread(() -> {
            try {
                activity.requestPermissions(permissions, requestCode);
            } catch (RuntimeException e) {
                nativeOnPermissionResult(requestCode, false);
            }
        });
    }

    /**
     * Forward a permission result. Called from AzulActivity.
     *
     * @return true when at least one result was reported, so the caller knows
     *         the request belonged to azul.
     */
    public static boolean onRequestPermissionsResultProxy(Activity activity, int requestCode,
                                                          String[] permissions,
                                                          int[] grantResults) {
        if (permissions == null || grantResults == null) {
            return false;
        }
        // Android reports an EMPTY array when a request is cancelled (the user
        // swiped the dialog away, or another dialog pre-empted it). That is a
        // denial, not a missing answer — reporting nothing would leave the
        // Rust side waiting on a result that is never coming.
        if (permissions.length == 0 || grantResults.length == 0) {
            nativeOnPermissionResult(requestCode, false);
            return true;
        }
        boolean granted = true;
        for (int r : grantResults) {
            // PackageManager.PERMISSION_GRANTED == 0
            if (r != 0) {
                granted = false;
                break;
            }
        }
        nativeOnPermissionResult(requestCode, granted);
        return true;
    }

    private static native void nativeOnPermissionResult(int requestCode, boolean granted);
}
