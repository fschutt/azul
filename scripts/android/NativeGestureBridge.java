// Java bridge that turns Android's native gesture-recognizer callbacks
// (GestureDetector, ScaleGestureDetector, custom 2-finger rotation) into
// JNI calls into the Rust GestureAndDragManager.
//
// Compiled outside Gradle:
//   javac -source 11 -target 11 \
//       -classpath $ANDROID_HOME/platforms/android-34/android.jar \
//       -d classes/ scripts/android/NativeGestureBridge.java
//   $ANDROID_HOME/build-tools/34.0.0/d8 \
//       classes/com/azul/gesture/NativeGestureBridge.class \
//       --output dex/
//   cd dex && zip -r ../base.apk classes.dex
//
// The native_ptr long is the address of the Rust AndroidWindow,
// stashed by the Rust side when the NativeActivity is created.
// Pass it through every dispatch call so the Java side has no static
// state of its own. The Rust side is responsible for keeping the
// pointer valid (i.e. not moving the AndroidWindow on the heap).

package com.azul.gesture;

import android.content.Context;
import android.view.GestureDetector;
import android.view.MotionEvent;
import android.view.ScaleGestureDetector;
import android.view.View;

public final class NativeGestureBridge
        extends GestureDetector.SimpleOnGestureListener
        implements ScaleGestureDetector.OnScaleGestureListener {

    /** AndroidWindow* — opaque cookie passed back to every JNI call. */
    private final long nativePtr;

    /** Built lazily once attached to the activity's root View. */
    private GestureDetector tapDetector;
    private ScaleGestureDetector scaleDetector;

    /** Bookkeeping for the custom two-finger rotation detector
     *  (Android has no built-in `RotationGestureDetector`). */
    private float lastAngle;
    private boolean rotationInProgress;
    private long rotationStartUptimeMs;

    public NativeGestureBridge(long nativePtr) {
        this.nativePtr = nativePtr;
    }

    /** Wire into a View. Call this from android_main once the
     *  activity's content view is available. */
    public void attach(Context ctx, View v) {
        this.tapDetector = new GestureDetector(ctx, this);
        this.scaleDetector = new ScaleGestureDetector(ctx, this);
        v.setOnTouchListener((view, ev) -> {
            scaleDetector.onTouchEvent(ev);
            tapDetector.onTouchEvent(ev);
            handleRotation(ev);
            return false; // never consume — touch keeps flowing to the
                          // native input-event queue picked up by
                          // android_main's MainEvent::InputAvailable.
        });
    }

    // ─── GestureDetector callbacks ─────────────────────────────────

    @Override
    public boolean onDoubleTap(MotionEvent e) {
        nativeOnDoubleTap(nativePtr);
        return true;
    }

    @Override
    public void onLongPress(MotionEvent e) {
        nativeOnLongPress(nativePtr, e.getX(), e.getY(), 0);
    }

    @Override
    public boolean onFling(
            MotionEvent e1, MotionEvent e2,
            float velocityX, float velocityY) {
        int dir;
        if (Math.abs(velocityX) > Math.abs(velocityY)) {
            dir = velocityX > 0 ? DIR_RIGHT : DIR_LEFT;
        } else {
            dir = velocityY > 0 ? DIR_DOWN : DIR_UP;
        }
        nativeOnSwipe(nativePtr, dir);
        return true;
    }

    // ─── ScaleGestureDetector callbacks ────────────────────────────

    @Override
    public boolean onScale(ScaleGestureDetector d) {
        nativeOnPinch(
                nativePtr,
                d.getScaleFactor(),
                d.getFocusX(), d.getFocusY(),
                d.getPreviousSpan(), d.getCurrentSpan(),
                d.getEventTime() - d.getTimeDelta());
        return true;
    }

    @Override
    public boolean onScaleBegin(ScaleGestureDetector d) { return true; }

    @Override
    public void onScaleEnd(ScaleGestureDetector d) { /* no-op */ }

    // ─── Custom two-finger rotation ────────────────────────────────

    private void handleRotation(MotionEvent ev) {
        if (ev.getPointerCount() < 2) {
            rotationInProgress = false;
            return;
        }
        float dx = ev.getX(1) - ev.getX(0);
        float dy = ev.getY(1) - ev.getY(0);
        float angle = (float) Math.atan2(dy, dx);
        if (!rotationInProgress) {
            rotationInProgress = true;
            rotationStartUptimeMs = ev.getEventTime();
            lastAngle = angle;
            return;
        }
        float delta = angle - lastAngle;
        lastAngle = angle;
        nativeOnRotation(
                nativePtr, delta,
                (ev.getX(0) + ev.getX(1)) * 0.5f,
                (ev.getY(0) + ev.getY(1)) * 0.5f,
                ev.getEventTime() - rotationStartUptimeMs);
    }

    // ─── JNI extern declarations ───────────────────────────────────

    private static native void nativeOnDoubleTap(long nativePtr);
    private static native void nativeOnLongPress(long nativePtr, float x, float y, long durationMs);
    private static native void nativeOnSwipe(long nativePtr, int direction);
    private static native void nativeOnPinch(
            long nativePtr,
            float scale, float centerX, float centerY,
            float initialDistance, float currentDistance,
            long durationMs);
    private static native void nativeOnRotation(
            long nativePtr,
            float angleRadians, float centerX, float centerY,
            long durationMs);

    // Direction constants — must match `azul_layout::managers::gesture::GestureDirection`
    // (which is #[repr(C)] enum starting at 0).
    private static final int DIR_UP = 0;
    private static final int DIR_DOWN = 1;
    private static final int DIR_LEFT = 2;
    private static final int DIR_RIGHT = 3;
}
