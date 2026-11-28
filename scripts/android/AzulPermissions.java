// Runtime permissions: the request, run on the UI thread, and the result,
// routed back into Rust.
//
// The REQUEST comes from Rust (permission/android.rs) on android_main's
// thread, but Activity.requestPermissions starts the system dialog's activity
// and belongs on the UI thread - so request() hops there. The RESULT arrives
// as a Java callback - Activity.onRequestPermissionsResult - which native code
// cannot receive, so it lands here and is forwarded.
//
// ONE dialog at a time. The system cancels a request made while another one is on the screen (its result
// arrays come back EMPTY, with nothing shown to the user), and two capabilities that want a permission in the
// same frame (a camera tile and a microphone tile, a location probe and the first notification) are the
// ordinary case: the requests wait for each other here, in the order they were made.
//
// Lenient by construction: an empty result array - the system dropped the request: the user pressed Home, the
// activity was paused, the framework refused to start it - is reported as "cancelled", NOT as a denial (nobody
// said no, and a denial is remembered). A permission dialog the user dismissed must not take the app with it.

package com.azul.permission;

import android.app.Activity;
import android.content.Context;
import android.content.SharedPreferences;

import com.azul.geolocation.AzulGeolocation;

import java.util.ArrayDeque;

public final class AzulPermissions {

    private AzulPermissions() {}

    private static final String PREFS = "azul_permissions";

    /**
     * Whether a request of ours for `permission` ever ended in a "no": the system reads "never asked" and "denied, don't ask
     * again" the same (`shouldShowRequestPermissionRationale` is false for both), and a denial that is forgotten at the next
     * start is asked about again at every launch. Cleared by a later grant (the user turned it on in the settings).
     */
    public static boolean wasDenied(Activity activity, String permission) {
        try {
            return activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean("denied." + permission, false);
        } catch (RuntimeException e) {
            return false;
        }
    }

    private static void remember(Activity activity, String[] permissions, int[] grantResults) {
        try {
            SharedPreferences.Editor edit = activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit();
            for (int i = 0; i < permissions.length && i < grantResults.length; i++) {
                // PackageManager.PERMISSION_GRANTED == 0
                edit.putBoolean("denied." + permissions[i], grantResults[i] != 0);
            }
            edit.apply();
        } catch (RuntimeException e) {
            // Not remembering is the lesser harm: the answer still reaches the app below.
        }
    }

    /** A request that waits for its turn. */
    private static final class Pending {
        final String[] permissions;
        final int requestCode;

        Pending(String[] permissions, int requestCode) {
            this.permissions = permissions;
            this.requestCode = requestCode;
        }
    }

    // Both only ever touched on the UI thread (request() hops there, the result arrives there): no lock.
    private static final ArrayDeque<Pending> waiting = new ArrayDeque<>();
    private static Pending inFlight;

    /**
     * Ask for runtime permissions, on the UI thread. Called from Rust on
     * android_main's thread; the answer arrives, as for any request, through
     * AzulActivity.onRequestPermissionsResult and onRequestPermissionsResultProxy.
     * A request waits until the dialog of the one before it is answered (see the header): the system would
     * cancel it unasked. A request the framework refuses to start is reported as dropped, so the Rust side is
     * never left waiting for an answer that cannot come.
     */
    public static void request(final Activity activity, final String[] permissions,
                               final int requestCode) {
        activity.runOnUiThread(() -> {
            waiting.add(new Pending(permissions, requestCode));
            startNext(activity);
        });
    }

    /** Show the dialog of the next waiting request, unless one is on the screen. UI thread only. */
    private static void startNext(Activity activity) {
        while (inFlight == null) {
            Pending next = waiting.poll();
            if (next == null) {
                return;
            }
            try {
                activity.requestPermissions(next.permissions, next.requestCode);
                inFlight = next;
            } catch (RuntimeException e) {
                nativeOnPermissionCancelled(next.requestCode);
            }
        }
    }

    /**
     * Forward a permission result. Called from AzulActivity.
     *
     * Reports whether EVERY permission of the request was granted and whether ANY was: a user may
     * grant the approximate location and refuse the precise one, which is a reduced grant, not a
     * denial.
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
        Pending done = inFlight;
        if (done == null || done.requestCode != requestCode) {
            // Not a request of ours (or one this class no longer waits for).
            return false;
        }
        inFlight = null;
        final boolean dropped = permissions.length == 0 || grantResults.length == 0;
        if (dropped) {
            // Android reports EMPTY arrays when a request is cancelled (the user pressed Home, the activity was
            // paused, another dialog pre-empted it). Nobody said no: the Rust side puts the capability back to
            // what the system says, and nothing is remembered as a denial.
            nativeOnPermissionCancelled(requestCode);
        } else {
            remember(activity, permissions, grantResults);
            boolean all = true;
            boolean any = false;
            for (int r : grantResults) {
                // PackageManager.PERMISSION_GRANTED == 0
                if (r == 0) {
                    any = true;
                } else {
                    all = false;
                }
            }
            nativeOnPermissionResult(requestCode, all, any);
        }
        // A location subscription that came before its permission starts now.
        AzulGeolocation.onPermissionAnswered(activity, done.permissions, dropped);
        startNext(activity);
        return true;
    }

    private static native void nativeOnPermissionResult(int requestCode, boolean allGranted,
                                                        boolean anyGranted);

    /** The system dropped the request without an answer (see the header). */
    private static native void nativeOnPermissionCancelled(int requestCode);
}
