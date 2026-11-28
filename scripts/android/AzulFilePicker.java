// The activity-result proxy of the file picker. Nothing else of the picker is Java: the intents are built and started FROM RUST
// (dll/src/desktop/extra/file_picker/android.rs: ACTION_OPEN_DOCUMENT through JNI and Activity.startActivityForResult), the state of
// a request on the screen is in the App's own registry (AppServices::pickers) under an id that rides in the low bits of the request
// code, and the picked files are imported by Rust on a worker thread (the same inbox as a file the system opens the app with).
//
// Why a Java class exists at all: the system delivers an activity result to a METHOD of the activity (AzulActivity.onActivityResult),
// and that method has to hand it to native code. This class is that hand-over: it keeps no state, copies nothing and builds no intent.
//
// Compiled outside Gradle together with AzulActivity (scripts/build-android.sh).

package com.azul.picker;

import android.app.Activity;
import android.content.Intent;
import android.util.Log;

public final class AzulFilePicker {

    private static final String TAG = "AzulFilePicker";

    /** The base of the request codes the picker uses; the id of a request is the low 16 bits (Rust keeps the same base). */
    private static final int REQUEST_CODE_BASE = 0x4A5400;

    private AzulFilePicker() { /* static-only */ }

    /** Called from AzulActivity.onActivityResult for every request code. Returns true when the code is the picker's, so that the
     *  activity knows the result was taken. */
    public static boolean onActivityResultProxy(Activity activity, int requestCode, int resultCode, Intent data) {
        if (requestCode < REQUEST_CODE_BASE || requestCode >= REQUEST_CODE_BASE + (1 << 16)) {
            return false;
        }
        try {
            nativeOnActivityResult(activity, requestCode - REQUEST_CODE_BASE, resultCode, data);
        } catch (Throwable e) {
            // the engine's library is not loaded (a result that comes before it, a build without the picker): the result is
            // lost, and the app must not die of it
            Log.e(TAG, "the picker's result could not be handed to the engine", e);
        }
        return true;
    }

    /** Implemented in Rust (dll/src/desktop/extra/file_picker/android.rs): the request, the answer and the copies are there. */
    private static native void nativeOnActivityResult(Activity activity, int ticket, int resultCode, Intent data);
}
