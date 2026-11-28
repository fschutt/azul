// The way an intent that asks the app to open something reaches the engine: a file another app shared to it ("Share to
// AzWriter"), a file opened with it from a file manager or a mail ("Open with"), a link of a scheme the app declared in its manifest
// (mailto:, geo:), a text. AzulActivity forwards every intent it gets - the one that STARTED it (onCreate) and the ones that reach
// the running app (onNewIntent) - and everything else happens in Rust (dll/src/desktop/extra/open_files/android.rs): the
// intent is read through JNI, classified (azul_layout::open_intent), the files are copied into the app's own folder on a
// worker thread and the app's handler (CallbackInfo::set_open_handler) is run by the loop.
//
// This class only exists to hold the one native method and to say "nothing to tell" when the engine's library is not loaded (yet):
// an UnsatisfiedLinkError in onCreate would end the app before it drew its first frame.

package com.azul.open;

import android.app.Activity;
import android.content.Intent;

public final class AzulOpenIntent {

    private AzulOpenIntent() {}

    /**
     * An intent reached `activity`.
     *
     * @param launchedApp the intent STARTED the app (onCreate of a fresh activity), not one delivered to a running app
     */
    public static void forward(Activity activity, Intent intent, boolean launchedApp) {
        if (activity == null || intent == null) {
            return;
        }
        try {
            nativeOnIntent(activity, intent, launchedApp);
        } catch (UnsatisfiedLinkError e) {
            // the engine's library is not loaded: nothing can hear it
        }
    }

    /** Implemented in Rust (dll/src/desktop/extra/open_files/android.rs). */
    private static native void nativeOnIntent(Activity activity, Intent intent, boolean launchedApp);
}
