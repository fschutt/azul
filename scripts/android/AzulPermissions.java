// Runtime-permission results, routed back into Rust.
//
// The REQUEST side needs no helper: Rust calls Activity.requestPermissions
// directly through JNI (permission/android.rs). Only the RESULT arrives as a
// Java callback — Activity.onRequestPermissionsResult — which native code
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
