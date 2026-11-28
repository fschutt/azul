// Location fixes.
//
// Platform LocationManager, not Google Play Services' FusedLocationProvider:
// this APK ships no Play libraries, and an azul app must work on a device with
// no Google services at all.
//
// Permission is NOT requested here. The Rust permission manager owns that
// (extra/permission/android.rs -> Activity.requestPermissions), so this checks
// and reports rather than prompting — two prompt owners racing is how you get
// a dialog that reopens forever.
//
// A subscription that comes BEFORE its permission (the probe node and the permission request are made in
// the same layout pass: the dialog is still up when subscribe() runs) is remembered, not dropped, and starts
// when the answer arrives (onPermissionAnswered, called by AzulPermissions on the UI thread): the first
// run of an app used to get no fix at all until it was started a second time. An answer that is a "no"
// is reported to the engine (nativeOnLocationError), so the app can say so.

package com.azul.geolocation;

import android.app.Activity;
import android.content.pm.PackageManager;
import android.location.Location;
import android.location.LocationListener;
import android.location.LocationManager;
import android.os.Build;
import android.os.Bundle;

public final class AzulGeolocation {

    private AzulGeolocation() {}

    private static LocationManager manager;
    private static LocationListener listener;

    /** A subscription waiting for its permission: its handle (0 = none) and options. */
    private static long waitingHandle;
    private static boolean waitingHighAccuracy;
    private static int waitingMinIntervalMs;

    /** The live subscription: what `resume` starts again after a `pause` (the app asked for fixes once and expects them again). */
    private static long liveHandle;
    private static boolean liveHighAccuracy;
    private static int liveMinIntervalMs;
    private static boolean paused;

    /** What `nativeOnLocationError` reports (the engine reads them as the `code` of its location error). */
    private static final int ERROR_PERMISSION_DENIED = 1;

    // synchronized: the engine's loop thread subscribes and releases, the UI thread delivers the permission answer
    // (onPermissionAnswered): the wish of a subscription that waits for its permission is read by the other thread.
    public static synchronized void subscribe(Activity activity, long handle, boolean highAccuracy,
                                              int minIntervalMs) {
        if (activity == null) {
            return;
        }
        try {
            boolean fine = activity.checkSelfPermission(
                    android.Manifest.permission.ACCESS_FINE_LOCATION)
                    == PackageManager.PERMISSION_GRANTED;
            boolean coarse = activity.checkSelfPermission(
                    android.Manifest.permission.ACCESS_COARSE_LOCATION)
                    == PackageManager.PERMISSION_GRANTED;
            if (!fine && !coarse) {
                // Not an error yet: the permission manager may be in the middle of its own flow. The wish is kept
                // and starts at the answer (onPermissionAnswered). Throwing here would take down an app that
                // asked politely and was told "not yet".
                waitingHandle = handle;
                waitingHighAccuracy = highAccuracy;
                waitingMinIntervalMs = minIntervalMs;
                return;
            }
            waitingHandle = 0;
            manager = (LocationManager) activity.getSystemService(Activity.LOCATION_SERVICE);
            if (manager == null) {
                return;
            }
            release(handle);
            listener = new LocationListener() {
                @Override
                public void onLocationChanged(Location loc) {
                    if (loc == null) {
                        return;
                    }
                    // hasX() before getX(): the getters return 0.0 for
                    // "absent", which is a REAL value for altitude and heading.
                    // Reported as NaN so the engine can tell missing from zero.
                    float altitude = loc.hasAltitude() ? (float) loc.getAltitude() : Float.NaN;
                    float altAcc = Float.NaN;
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O
                            && loc.hasVerticalAccuracy()) {
                        altAcc = loc.getVerticalAccuracyMeters();
                    }
                    float heading = loc.hasBearing() ? loc.getBearing() : Float.NaN;
                    float speed = loc.hasSpeed() ? loc.getSpeed() : Float.NaN;
                    float acc = loc.hasAccuracy() ? loc.getAccuracy() : Float.NaN;
                    nativeOnLocationFix(handle, loc.getLatitude(), loc.getLongitude(),
                            acc, altitude, altAcc, heading, speed, loc.getTime());
                }

                @Override public void onProviderEnabled(String p) {}
                @Override public void onProviderDisabled(String p) {}
                @Override public void onStatusChanged(String p, int s, Bundle e) {}
            };
            // Ask for the accurate provider only when it was granted AND
            // requested; falling back to network keeps a coarse-only app working.
            String provider = (highAccuracy && fine)
                    ? LocationManager.GPS_PROVIDER
                    : LocationManager.NETWORK_PROVIDER;
            if (!manager.isProviderEnabled(provider)) {
                // The preferred provider is switched off (updates on it deliver nothing): the OTHER one, if it
                // is on and the permission allows it. The fallback used to be `fine ? GPS : NETWORK`, which is
                // GPS again when GPS was the one that was off. GPS needs the fine permission; the network
                // provider works with either.
                String other = LocationManager.GPS_PROVIDER.equals(provider)
                        ? LocationManager.NETWORK_PROVIDER
                        : LocationManager.GPS_PROVIDER;
                boolean allowed = !LocationManager.GPS_PROVIDER.equals(other) || fine;
                if (allowed && manager.isProviderEnabled(other)) {
                    provider = other;
                }
            }
            manager.requestLocationUpdates(provider, Math.max(0, minIntervalMs), 0f, listener,
                    activity.getMainLooper());
            liveHandle = handle;
            liveHighAccuracy = highAccuracy;
            liveMinIntervalMs = minIntervalMs;
            paused = false;
        } catch (Throwable t) {
            listener = null;
        }
    }

    /**
     * A permission request was answered (`dropped` = the system cancelled it without an answer): a subscription
     * that was waiting for the location permission starts now when it is there, and is told to the engine as an
     * error when the answer was a "no" (a refusal is final for this subscription; a grant in the settings and a
     * new probe node subscribe again). A request for another capability, or a dropped one, changes nothing.
     * Called on the UI thread by AzulPermissions.
     */
    public static synchronized void onPermissionAnswered(Activity activity, String[] requested, boolean dropped) {
        final long handle = waitingHandle;
        if (handle == 0 || activity == null || !asksForLocation(requested)) {
            return;
        }
        boolean allowed = activity.checkSelfPermission(android.Manifest.permission.ACCESS_FINE_LOCATION)
                == PackageManager.PERMISSION_GRANTED
                || activity.checkSelfPermission(android.Manifest.permission.ACCESS_COARSE_LOCATION)
                == PackageManager.PERMISSION_GRANTED;
        if (allowed) {
            subscribe(activity, handle, waitingHighAccuracy, waitingMinIntervalMs);
        } else if (!dropped) {
            waitingHandle = 0;
            nativeOnLocationError(handle, ERROR_PERMISSION_DENIED);
        }
    }

    private static boolean asksForLocation(String[] requested) {
        if (requested == null) {
            return false;
        }
        for (String permission : requested) {
            if (android.Manifest.permission.ACCESS_FINE_LOCATION.equals(permission)
                    || android.Manifest.permission.ACCESS_COARSE_LOCATION.equals(permission)) {
                return true;
            }
        }
        return false;
    }

    /**
     * The activity left the screen (onPause): the updates stop, the subscription stays. A location listener that goes on in
     * the background drains the battery for an app nobody looks at (and keeps the system's location indicator lit), and the
     * fixes it delivers have no window to go to.
     */
    public static synchronized void pause() {
        if (liveHandle == 0 || paused) {
            return;
        }
        paused = true;
        try {
            if (manager != null && listener != null) {
                manager.removeUpdates(listener);
            }
        } catch (Throwable ignored) {
        } finally {
            listener = null;
        }
    }

    /** The activity is back (onResume): a paused subscription starts again with the options it had. */
    public static synchronized void resume(Activity activity) {
        if (liveHandle == 0 || !paused) {
            return;
        }
        paused = false;
        subscribe(activity, liveHandle, liveHighAccuracy, liveMinIntervalMs);
    }

    public static synchronized void release(long handle) {
        if (handle == waitingHandle) {
            waitingHandle = 0;
        }
        if (handle == liveHandle) {
            liveHandle = 0;
            paused = false;
        }
        try {
            if (manager != null && listener != null) {
                manager.removeUpdates(listener);
            }
        } catch (Throwable ignored) {
        } finally {
            listener = null;
        }
    }

    /** The subscription of `handle` can produce no fix; `code` is one of the ERROR_ constants. */
    private static native void nativeOnLocationError(long handle, int code);

    private static native void nativeOnLocationFix(long handle, double latitudeDeg,
                                                   double longitudeDeg, float accuracyM,
                                                   float altitudeM, float altitudeAccuracyM,
                                                   float headingDeg, float speedMps,
                                                   long timestampMs);
}
