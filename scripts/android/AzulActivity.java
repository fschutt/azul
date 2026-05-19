// Thin NativeActivity subclass that exists for one reason: instantiate
// NativeGestureBridge during onCreate so iOS UIKit-equivalent gesture
// dispatch flows from Android GestureDetector into the Rust
// GestureAndDragManager. NativeActivity itself can't construct Java
// objects from its native_main loop without a JNI call chain that
// dwarfs this 30-line subclass.
//
// AndroidManifest.xml references this class instead of
// android.app.NativeActivity. The native side (Rust android_main) still
// drives the event loop via android-activity's NativeActivity glue.
//
// Compiled outside Gradle:
//   javac -source 11 -target 11 \
//       -classpath $ANDROID_HOME/platforms/android-34/android.jar \
//       -d classes/ scripts/android/AzulActivity.java \
//                   scripts/android/NativeGestureBridge.java
//   $ANDROID_HOME/build-tools/34.0.0/d8 classes/com/azul/app/*.class \
//       classes/com/azul/gesture/*.class --output dex/
//
// build-android.sh handles the compile + dex + APK packaging.

package com.azul.app;

import android.app.NativeActivity;
import android.os.Bundle;
import android.view.View;

import com.azul.gesture.NativeGestureBridge;

public class AzulActivity extends NativeActivity {

    private NativeGestureBridge gestureBridge;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        // NativeActivity loads the cdylib (libazul.so) during super.onCreate.
        // By the time we return here, our static JNI_OnLoad has run and
        // android_main is starting on its own thread — but the
        // ANDROID_WINDOW_PTR may not be set yet. We attach lazily in
        // onWindowFocusChanged so the AndroidWindow* is guaranteed to
        // be populated by then.
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (!hasFocus || gestureBridge != null) {
            return;
        }
        long nativePtr = nativeGetWindowPointer();
        if (nativePtr == 0L) {
            // android_main hasn't published the window yet — try again
            // on the next focus event.
            return;
        }
        gestureBridge = new NativeGestureBridge(nativePtr);
        View decor = getWindow().getDecorView();
        gestureBridge.attach(this, decor);
    }

    /** Implemented in Rust (dll/src/desktop/shell2/android/mod.rs). */
    private static native long nativeGetWindowPointer();
}
