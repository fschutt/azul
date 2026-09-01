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

    public static void subscribe(Activity activity, long handle, boolean highAccuracy,
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
                // Silent: the subscription simply produces no fixes until the
                // permission manager has been through its own flow. Throwing
                // here would take down an app that asked politely and was told
                // "not yet".
                return;
            }
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
                provider = fine ? LocationManager.GPS_PROVIDER
                                : LocationManager.NETWORK_PROVIDER;
            }
            manager.requestLocationUpdates(provider, Math.max(0, minIntervalMs), 0f, listener,
                    activity.getMainLooper());
        } catch (Throwable t) {
            listener = null;
        }
    }

    public static void release(long handle) {
        try {
            if (manager != null && listener != null) {
                manager.removeUpdates(listener);
            }
        } catch (Throwable ignored) {
        } finally {
            listener = null;
        }
    }

    private static native void nativeOnLocationFix(long handle, double latitudeDeg,
                                                   double longitudeDeg, float accuracyM,
                                                   float altitudeM, float altitudeAccuracyM,
                                                   float headingDeg, float speedMps,
                                                   long timestampMs);
}
