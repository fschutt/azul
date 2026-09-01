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
import android.content.Intent;
import android.content.res.Configuration;
import android.os.Bundle;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;

import com.azul.a11y.AzulAccessibilityBridge;
import com.azul.gesture.NativeGestureBridge;
import com.azul.picker.AzulFilePicker;
import com.azul.text.NativeTextBridge;
import com.azul.gamepad.AzulGamepad;
import com.azul.permission.AzulPermissions;
import com.azul.sensors.AzulSensors;

public class AzulActivity extends NativeActivity {

    private NativeGestureBridge gestureBridge;
    private AzulAccessibilityBridge accessibilityBridge;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        // NativeActivity dlopens the cdylib NATIVELY during super.onCreate —
        // which does NOT register it with this ClassLoader for Java
        // native-method resolution. Without an explicit System.loadLibrary,
        // every `native` method in this class (and the bridge classes) throws
        // UnsatisfiedLinkError at its first call even though the symbols are
        // exported (caught live by the post-release emulator check: crash in
        // onWindowFocusChanged -> nativeGetWindowPointer). The lib name is
        // per-app, so read it from the same manifest metadata NativeActivity
        // itself uses; loading a lib twice in one ClassLoader is a no-op.
        try {
            android.content.pm.ActivityInfo ai = getPackageManager().getActivityInfo(
                getComponentName(), android.content.pm.PackageManager.GET_META_DATA);
            String lib = ai.metaData != null
                ? ai.metaData.getString("android.app.lib_name")
                : null;
            if (lib != null) {
                System.loadLibrary(lib);
            }
        } catch (Exception e) {
            // Fall through: if the lib is truly unloadable, NativeActivity's
            // own load in super.onCreate produces the canonical error.
        }
        super.onCreate(savedInstanceState);
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
        View decor = getWindow().getDecorView();
        gestureBridge = new NativeGestureBridge(nativePtr);
        gestureBridge.attach(this, decor);

        // AccessibilityNodeProvider bridge. Without it TalkBack sees one
        // opaque View and every button, link and text node azul draws is
        // unreachable — the state Android shipped in. Attached to the same
        // decor view and on the same lazily-resolved nativePtr, because the
        // provider needs the AndroidWindow to describe nodes at all.
        accessibilityBridge = new AzulAccessibilityBridge(nativePtr);
        accessibilityBridge.attach(decor);

        // Push the CURRENT ui mode as well as future changes. Android built its
        // window from SystemStyle::default() and never read the device setting,
        // so an app launched on a dark-mode device rendered light and stayed
        // light. onConfigurationChanged alone would only fix the second half.
        pushUiMode(nativePtr);

        // Window insets: status bar, navigation bar, display cutout and the
        // IME. Android had NO inset handling at all, so the system bars drew
        // ON TOP of the app's content — the app's own titlebar rendered under
        // the clock. Since API 35 edge-to-edge is not opt-in, so this is not a
        // nicety.
        NativeTextBridge.installInsetsListener(this, nativePtr);

        // The view the IME attaches to. `onCreateInputConnection` is a VIEW
        // method, so an Activity cannot supply one — NativeActivity's content
        // view answers null and every soft-keyboard keystroke was dropped.
        NativeTextBridge.installInputView(this, nativePtr);

        // Controller hotplug. The native input queue already delivers gamepad
        // buttons and axes; what it cannot deliver is "a pad appeared", which
        // only InputManager.InputDeviceListener reports.
        AzulGamepad.attach(this);
    }

    /**
     * Runtime-permission results.
     *
     * Rust calls Activity.requestPermissions directly, but the ANSWER is a Java
     * callback that native code cannot receive — so it lands here.
     */
    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions,
                                           int[] grantResults) {
        boolean handled = AzulPermissions.onRequestPermissionsResultProxy(
                this, requestCode, permissions, grantResults);
        if (!handled) {
            super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        }
    }

    // Gamepad buttons and sticks. NativeActivity would consume these itself;
    // forwarding first lets the hotplug-aware path see them, and returning
    // false keeps the normal native queue working for everything else.
    @Override
    public boolean dispatchKeyEvent(KeyEvent event) {
        if (event != null && AzulGamepad.onKey(event, event.getAction() == KeyEvent.ACTION_DOWN)) {
            return true;
        }
        return super.dispatchKeyEvent(event);
    }

    @Override
    public boolean dispatchGenericMotionEvent(MotionEvent event) {
        if (AzulGamepad.onMotion(event)) {
            return true;
        }
        return super.dispatchGenericMotionEvent(event);
    }

    @Override
    protected void onPause() {
        // Sensors and location keep draining the battery behind a backgrounded
        // app unless something stops them; nothing did.
        AzulSensors.stop(this);
        super.onPause();
    }

    @Override
    protected void onDestroy() {
        AzulGamepad.detach(this);
        AzulSensors.stop(this);
        super.onDestroy();
    }



    @Override
    public void onConfigurationChanged(Configuration newConfig) {
        super.onConfigurationChanged(newConfig);
        // AndroidManifest declares uiMode in android:configChanges, so the
        // activity is NOT recreated on a dark-mode toggle and this is the only
        // notification we get.
        long nativePtr = nativeGetWindowPointer();
        if (nativePtr != 0L) {
            pushUiMode(nativePtr);
        }
    }

    /** Map UI_MODE_NIGHT_MASK onto the codes the Rust side expects. */
    private void pushUiMode(long nativePtr) {
        int night = getResources().getConfiguration().uiMode
                & Configuration.UI_MODE_NIGHT_MASK;
        // 0 = undefined (let the Rust side keep whatever it has), 1 = light,
        // 2 = dark. These are azul's own numbers, not Android's, so the
        // constants stay on this side of JNI.
        int mapped = (night == Configuration.UI_MODE_NIGHT_YES) ? 2
                   : (night == Configuration.UI_MODE_NIGHT_NO) ? 1
                   : 0;
        nativeOnUiModeChanged(nativePtr, mapped);
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        // Route file-picker (and any other dispatched-by-azul) results
        // through AzulFilePicker first. If it claims the request code,
        // we suppress the NativeActivity fall-through (it would be a
        // no-op anyway since NativeActivity doesn't process activity
        // results).
        boolean handled =
                AzulFilePicker.onActivityResultProxy(this, requestCode, resultCode, data);
        if (!handled) {
            super.onActivityResult(requestCode, resultCode, data);
        }
    }

    /** Implemented in Rust (dll/src/desktop/shell2/android/mod.rs). */
    private static native long nativeGetWindowPointer();

    /** Implemented in Rust (dll/src/desktop/shell2/android/mod.rs). */
    private static native void nativeOnUiModeChanged(long nativePtr, int nightMode);
}
