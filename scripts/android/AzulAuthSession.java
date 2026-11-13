// Sign-in sessions on Android (OAuth 2.0 for native apps, RFC 8252): the provider's page in a
// Custom Tab, the redirect back through the app's own URL scheme.
//
// Rust (dll/src/desktop/extra/auth_session/android.rs) calls start(); the answer goes back
// through nativeOnRedirect(id, url, null) when the redirect arrives, or
// nativeOnRedirect(id, null, reason) when the app comes back without it (the user closed the
// tab) - the same symbol either way.
//
// The redirect: the provider sends the browser to <scheme>:/..., which Android hands to
// RedirectActivity below (the manifest declares it with an intent filter for the scheme:
// AZ_ANDROID_AUTH_SCHEME in build-android.sh). RedirectActivity forwards the URL to
// AzulActivity with CLEAR_TOP | SINGLE_TOP - the Custom Tab above it closes and the running
// instance gets the intent in onNewIntent (no second NativeActivity, no second android_main) -
// and AzulActivity.onNewIntent calls onIntent() here.
//
// No androidx: a Custom Tab is an ACTION_VIEW intent with the session extra (a null binder),
// which Chrome and the other Custom Tabs browsers honour; a browser without Custom Tabs opens
// the page as a normal page and the redirect works the same way.

package com.azul.auth;

import android.app.Activity;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;

public final class AzulAuthSession {

    /** CustomTabsIntent.EXTRA_SESSION: present (even null) = open as a Custom Tab. */
    private static final String EXTRA_SESSION = "android.support.customtabs.extra.SESSION";
    /** CustomTabsIntent's ephemeral browsing (no cookies shared with the browser), where the
     *  browser has it; ignored elsewhere. */
    private static final String EXTRA_EPHEMERAL =
            "androidx.browser.customtabs.extra.ENABLE_EPHEMERAL_BROWSING";

    /** The session waiting for its redirect (0: none), its scheme, and whether the app was
     *  paused since it started (the tab covered it). One at a time. */
    private static long pendingId = 0;
    private static String pendingScheme = null;
    private static boolean paused = false;

    private AzulAuthSession() {}

    /** Opens url (the authorize URL with its redirect_uri) in a Custom Tab for session id. */
    public static void start(final Activity activity, long id, String url, String scheme,
                             boolean ephemeral) {
        synchronized (AzulAuthSession.class) {
            if (pendingId != 0) {
                finishPending(null, "another sign-in started");
            }
            pendingId = id;
            pendingScheme = scheme;
            paused = false;
        }
        final Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(url));
        Bundle extras = new Bundle();
        extras.putBinder(EXTRA_SESSION, null);
        intent.putExtras(extras);
        if (ephemeral) {
            intent.putExtra(EXTRA_EPHEMERAL, true);
        }
        intent.addCategory(Intent.CATEGORY_BROWSABLE);
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    activity.startActivity(intent);
                } catch (Exception e) {
                    synchronized (AzulAuthSession.class) {
                        finishPending(null, "no browser could open the sign-in: " + e);
                    }
                }
            }
        });
    }

    /** From AzulActivity.onNewIntent: true when the intent is the waiting session's redirect. */
    public static boolean onIntent(Intent intent) {
        synchronized (AzulAuthSession.class) {
            if (pendingId == 0 || intent == null) {
                return false;
            }
            Uri data = intent.getData();
            if (data == null || data.getScheme() == null
                    || !data.getScheme().equalsIgnoreCase(pendingScheme)) {
                return false;
            }
            finishPending(data.toString(), null);
            return true;
        }
    }

    /** From AzulActivity.onPause: the tab covers the app. */
    public static void onPause() {
        synchronized (AzulAuthSession.class) {
            if (pendingId != 0) {
                paused = true;
            }
        }
    }

    /** From AzulActivity.onResume: back in the app. A redirect arrives in onNewIntent, BEFORE
     *  onResume - so a session still waiting after a pause was closed without one. */
    public static void onResume() {
        synchronized (AzulAuthSession.class) {
            if (pendingId != 0 && paused) {
                finishPending(null, "the sign-in tab was closed");
            }
        }
    }

    /** From Rust: session id's time is up; a late redirect is ignored. */
    public static void cancel(long id) {
        synchronized (AzulAuthSession.class) {
            if (pendingId == id) {
                pendingId = 0;
                pendingScheme = null;
                paused = false;
            }
        }
    }

    /** Ends the waiting session with url (the redirect) or reason. Holds the class lock. */
    private static void finishPending(String url, String reason) {
        long id = pendingId;
        pendingId = 0;
        pendingScheme = null;
        paused = false;
        if (id != 0) {
            nativeOnRedirect(id, url, reason);
        }
    }

    /** Implemented in Rust (dll/src/desktop/extra/auth_session/android.rs). */
    private static native void nativeOnRedirect(long id, String urlOrNull, String reasonOrNull);

    /**
     * Where a redirect lands: declared in the manifest with the app's scheme. Hands the URL to
     * AzulActivity (CLEAR_TOP closes the Custom Tab above it, SINGLE_TOP delivers it to the
     * running instance's onNewIntent) and finishes.
     */
    public static final class RedirectActivity extends Activity {
        @Override
        protected void onCreate(Bundle savedInstanceState) {
            super.onCreate(savedInstanceState);
            Intent forward = new Intent(this, com.azul.app.AzulActivity.class);
            forward.setData(getIntent() != null ? getIntent().getData() : null);
            forward.addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_SINGLE_TOP);
            startActivity(forward);
            finish();
        }
    }
}
