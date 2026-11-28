// Text input, IME and window insets for an Azul NativeActivity.
//
// WHY THIS FILE HAS TO EXIST
//
// A NativeActivity gets raw KeyEvents, and that is enough for a hardware
// keyboard's navigation keys. It is not enough for anything else on a phone:
//
//   * A soft keyboard does not synthesise KeyEvents for characters. It talks to
//     an InputConnection, which is a Java object; native code cannot implement
//     one or hand one to the framework.
//   * An IME composing Japanese, Chinese or Korean produces NO KeyEvent at all
//     — only setComposingText/commitText on that same InputConnection.
//   * InputMethodManager, which is what actually raises the keyboard, is a
//     Java-only API with no NDK entry point.
//   * Window insets (status bar, navigation bar, display cutout, and the
//     keyboard's own height) arrive through View.onApplyWindowInsets. Since
//     API 35 edge-to-edge is mandatory, so without this the system bars sit ON
//     TOP of the app's content.
//
// The Rust side of all four has been written for a while
// (`Java_com_azul_text_NativeTextBridge_*` in shell2/android/mod.rs). JNI is a
// callee: those symbols did nothing because no Java ever called them.
//
// Compiled outside Gradle by scripts/build-android.sh (javac -> d8 ->
// classes.dex inside the APK), same as the gesture and accessibility bridges.

package com.azul.text;

import android.app.Activity;
import android.content.ClipData;
import android.content.ClipDescription;
import android.content.ClipboardManager;
import android.content.Context;
import android.graphics.Insets;
import android.os.Build;
import android.text.Editable;
import android.text.SpannableStringBuilder;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.graphics.Rect;
import android.util.Log;
import android.view.ActionMode;
import android.view.Menu;
import android.view.MenuItem;
import android.view.WindowInsets;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;

public final class NativeTextBridge {

    private NativeTextBridge() {}

    // ---- Rust -> Java ------------------------------------------------------

    /** Raise the soft keyboard. Called from Rust when focus lands on an editable node. */
    public static void showKeyboard(Activity activity) {
        if (activity == null) {
            return;
        }
        activity.runOnUiThread(() -> {
            // Target the input view, not the decor view: showSoftInput only
            // does anything for a view that is focused AND is a text editor,
            // and it is the focused view the IME asks for an InputConnection.
            View view = inputView != null ? inputView : activity.getWindow().getDecorView();
            boolean focused = view.requestFocus();
            InputMethodManager imm =
                    (InputMethodManager) activity.getSystemService(Context.INPUT_METHOD_SERVICE);
            boolean shown = false;
            if (imm != null) {
                // SHOW_IMPLICIT is the polite flag and can be ignored — most
                // visibly when the view was not actually focused. Retry
                // unconditionally in that case rather than silently doing
                // nothing.
                shown = imm.showSoftInput(view, InputMethodManager.SHOW_IMPLICIT);
                if (!shown) {
                    shown = imm.showSoftInput(view, 0);
                }
            }
            Log.i("azul", "NativeTextBridge.showKeyboard: view=" + (inputView != null ? "input" : "decor")
                    + " focused=" + focused + " shown=" + shown
                    + " isTextEditor=" + view.onCheckIsTextEditor());
        });
    }

    /** Dismiss the soft keyboard. */
    public static void hideKeyboard(Activity activity) {
        if (activity == null) {
            return;
        }
        activity.runOnUiThread(() -> {
            View view = inputView != null ? inputView : activity.getWindow().getDecorView();
            InputMethodManager imm =
                    (InputMethodManager) activity.getSystemService(Context.INPUT_METHOD_SERVICE);
            if (imm != null) {
                imm.hideSoftInputFromWindow(view.getWindowToken(), 0);
            }
        });
    }

    // ---- Window insets ------------------------------------------------------------------------------------------------

    /** The latest insets the window was given, `[top, bottom, left, right, ime]` physical px; null before the first dispatch. */
    private static volatile int[] lastInsets;
    /** The latest display features (the array of `android_core/display.rs`), physical px; null before the first dispatch or when they could not be read. */
    private static volatile int[] lastFeatures;
    /** The engine's bridge the insets are forwarded to as they change, 0 until the engine has published it (`installInsetsListener`). */
    private static volatile long insetsTarget;
    /** Dispatches of this window's insets seen so far (UI thread): the first few are on the record. */
    private static int insetsDispatches;

    /**
     * THE FIRST INSETS, BEFORE THE ENGINE HAS A WINDOW.
     *
     * The window's insets (the status bar, the navigation bar, the cutout) are dispatched in the first traversal of the window, before its
     * surface exists. A listener installed at the first window focus (where it used to be installed: the engine's bridge does not exist
     * sooner) missed that dispatch, so the engine laid its first picture out under the status bar, got the insets after the picture, laid
     * everything out AGAIN (a second full layout, 0.3 s on the widget gallery) and moved the page down by the height of the bar: a jump
     * between the first picture and the second (ledger E60, the "Resize" pass after the first present). This listener is installed in
     * onCreate, sees the first dispatch and keeps what it saw; the engine asks for it ({@link #insetsSnapshot}) when its surface is created,
     * before its first layout. Later dispatches are forwarded to the engine as they come once it has published its bridge.
     */
    public static void installEarlyInsetsListener(final Activity activity) {
        if (activity == null) {
            return;
        }
        // A window made again starts from nothing: what the window before it was given is not this one's, and the engine's bridge the insets were
        // forwarded to is the one of an engine that ended (the new one publishes its own, `installInsetsListener`). The process ends with its activity
        // (`AzulActivity.onDestroy`), so this is only so that nothing of an earlier window is ever read.
        lastInsets = null;
        lastFeatures = null;
        insetsTarget = 0L;
        insetsDispatches = 0;
        final View root = activity.getWindow().getDecorView();
        root.setOnApplyWindowInsetsListener((v, insets) -> {
            rememberInsets(activity, v, insets);
            return insets;
        });
    }

    /**
     * What the engine asks for when its surface is created: the insets of the window's latest dispatch (five ints: top, bottom, left, right,
     * the keyboard) followed by its display features (the array of `android_core/display.rs`, absent when they could not be read); null when
     * the window has had no dispatch yet. Any thread.
     */
    public static int[] insetsSnapshot() {
        int[] insets = lastInsets;
        if (insets == null) {
            return null;
        }
        int[] features = lastFeatures;
        int[] both = new int[insets.length + (features == null ? 0 : features.length)];
        System.arraycopy(insets, 0, both, 0, insets.length);
        if (features != null) {
            System.arraycopy(features, 0, both, insets.length, features.length);
        }
        return both;
    }

    /**
     * Start forwarding window insets to Rust (called once the engine has published its bridge, from AzulActivity.attachBridges): what the
     * window has been given so far goes at once, and every later dispatch as it comes.
     */
    public static void installInsetsListener(final Activity activity, final long nativePtr) {
        if (activity == null || nativePtr == 0L) {
            return;
        }
        activity.runOnUiThread(() -> {
            insetsTarget = nativePtr;
            int[] insets = lastInsets;
            if (insets != null) {
                nativeOnWindowInsets(nativePtr, insets[0], insets[1], insets[2], insets[3], insets[4]);
            }
            int[] features = lastFeatures;
            if (features != null) {
                nativeOnDisplayFeatures(nativePtr, features);
            }
            // a window that had no dispatch yet (or an early listener something replaced): ask again
            activity.getWindow().getDecorView().requestApplyInsets();
        });
    }

    /** One dispatch of the window's insets: kept for {@link #insetsSnapshot}, and forwarded when the engine's bridge is there. UI thread. */
    private static void rememberInsets(Activity activity, View root, WindowInsets insets) {
        int[] values = insetsArray(insets);
        int[] features = featuresArray(activity, root, insets);
        lastInsets = values;
        lastFeatures = features;
        // On the record, with the time: the first dispatches of a window and what the app's request of the system bars makes of them (a start whose
        // first layout was made for one state of the bars and the picture moved for the next is told by these lines and the engine's).
        if (insetsDispatches < 4) {
            insetsDispatches++;
            boolean statusShown = Build.VERSION.SDK_INT < Build.VERSION_CODES.R || insets.isVisible(WindowInsets.Type.statusBars());
            Log.i("azul", "NativeTextBridge: insets dispatch " + insetsDispatches + " top=" + values[0] + " bottom=" + values[1] + " left=" + values[2]
                    + " right=" + values[3] + " ime=" + values[4] + " status bar shown=" + statusShown + ", engine bridge "
                    + (insetsTarget != 0L ? "attached" : "not yet"));
        }
        long target = insetsTarget;
        if (target != 0L) {
            nativeOnWindowInsets(target, values[0], values[1], values[2], values[3], values[4]);
            if (features != null) {
                nativeOnDisplayFeatures(target, features);
            }
        }
    }

    private static int[] insetsArray(WindowInsets insets) {
        int top = 0, bottom = 0, left = 0, right = 0, ime = 0;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            // systemBars() | displayCutout(): the notch is NOT part of
            // systemBars, so a cutout device would still clip content that only
            // avoided the status bar.
            Insets bars = insets.getInsets(
                    WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
            top = bars.top;
            bottom = bars.bottom;
            left = bars.left;
            right = bars.right;
            // The keyboard is reported SEPARATELY and stays separate all the
            // way into the engine: an app scrolls the caret above the IME, it
            // does not permanently inset its layout by it.
            ime = insets.getInsets(WindowInsets.Type.ime()).bottom;
        } else {
            top = insets.getSystemWindowInsetTop();
            bottom = insets.getSystemWindowInsetBottom();
            left = insets.getSystemWindowInsetLeft();
            right = insets.getSystemWindowInsetRight();
        }
        return new int[] {top, bottom, left, right, ime};
    }

    /** The window's size in physical px: its decor view's once laid out, before that the window's bounds (the first dispatch comes before the first layout). */
    private static int[] windowSize(Activity activity, View root) {
        int width = root.getWidth();
        int height = root.getHeight();
        if (width > 0 && height > 0) {
            return new int[] {width, height};
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Rect bounds = activity.getWindowManager().getCurrentWindowMetrics().getBounds();
            return new int[] {bounds.width(), bounds.height()};
        }
        android.util.DisplayMetrics metrics = activity.getResources().getDisplayMetrics();
        return new int[] {metrics.widthPixels, metrics.heightPixels};
    }

    /**
     * What the display has built in and the system draws over the window, flattened into ONE int array for Rust
     * (the layout of the array is the contract of `android_core/display.rs`, version 1): the cutouts
     * (`DisplayCutout.getBoundingRects()`, API 28), the radii of the rounded corners (`RoundedCorner`, API 31), the status
     * and the navigation bar with the rect they have WHEN SHOWN and whether they are (`getInsetsIgnoringVisibility`,
     * `isVisible`, API 30) and whether the device has a fingerprint sensor. Rects are in the window's own physical px.
     *
     * The fingerprint sensor's KIND and PLACE are not public API (`FingerprintSensorPropertiesInternal` is hidden): there is
     * no reflection here, the array says only whether a sensor exists. Null when a platform call refuses.
     */
    private static int[] featuresArray(Activity activity, View root, WindowInsets insets) {
        try {
            java.util.List<Rect> cutouts = null;
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P && insets.getDisplayCutout() != null) {
                cutouts = insets.getDisplayCutout().getBoundingRects();
            }
            int count = cutouts == null ? 0 : cutouts.size();
            int[] flat = new int[2 + 4 * count + 4 + 6 + 6 + 1];
            int at = 0;
            flat[at++] = 1;
            flat[at++] = count;
            for (int i = 0; i < count; i++) {
                Rect r = cutouts.get(i);
                flat[at++] = r.left;
                flat[at++] = r.top;
                flat[at++] = r.right;
                flat[at++] = r.bottom;
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                flat[at++] = cornerRadius(insets, android.view.RoundedCorner.POSITION_TOP_LEFT);
                flat[at++] = cornerRadius(insets, android.view.RoundedCorner.POSITION_TOP_RIGHT);
                flat[at++] = cornerRadius(insets, android.view.RoundedCorner.POSITION_BOTTOM_RIGHT);
                flat[at++] = cornerRadius(insets, android.view.RoundedCorner.POSITION_BOTTOM_LEFT);
            } else {
                at += 4;
            }
            int[] size = windowSize(activity, root);
            int width = size[0];
            int height = size[1];
            int statusHeight;
            int navBottom = 0;
            int navLeft = 0;
            int navRight = 0;
            boolean statusVisible;
            boolean navVisible;
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                Insets status = insets.getInsetsIgnoringVisibility(WindowInsets.Type.statusBars());
                Insets nav = insets.getInsetsIgnoringVisibility(WindowInsets.Type.navigationBars());
                statusHeight = status.top;
                navBottom = nav.bottom;
                navLeft = nav.left;
                navRight = nav.right;
                statusVisible = insets.isVisible(WindowInsets.Type.statusBars());
                navVisible = insets.isVisible(WindowInsets.Type.navigationBars());
            } else {
                // Before API 30 the insets are the visible bars' only: a hidden bar reads 0.
                statusHeight = insets.getSystemWindowInsetTop();
                navBottom = insets.getSystemWindowInsetBottom();
                navLeft = insets.getSystemWindowInsetLeft();
                navRight = insets.getSystemWindowInsetRight();
                statusVisible = statusHeight > 0;
                navVisible = navBottom > 0 || navLeft > 0 || navRight > 0;
            }
            // the status bar: the band along the top edge
            at = putBar(flat, at, statusHeight > 0, 0, 0, width, statusHeight, statusVisible);
            // the navigation bar: along the bottom edge, else the side the three-button bar sits on in landscape
            if (navBottom > 0) {
                at = putBar(flat, at, true, 0, height - navBottom, width, height, navVisible);
            } else if (navRight > 0) {
                at = putBar(flat, at, true, width - navRight, 0, width, height, navVisible);
            } else if (navLeft > 0) {
                at = putBar(flat, at, true, 0, 0, navLeft, height, navVisible);
            } else {
                at = putBar(flat, at, false, 0, 0, 0, 0, false);
            }
            boolean fingerprint = root.getContext().getPackageManager()
                    .hasSystemFeature(android.content.pm.PackageManager.FEATURE_FINGERPRINT);
            flat[at] = fingerprint ? 0 : -1;
            return flat;
        } catch (RuntimeException e) {
            // The display features are an extra: a platform that refuses one of the calls leaves the app with the insets.
            Log.w("AzulDisplay", "display features not reported: " + e);
            return null;
        }
    }

    private static int cornerRadius(WindowInsets insets, int position) {
        android.view.RoundedCorner corner = insets.getRoundedCorner(position);
        return corner == null ? 0 : corner.getRadius();
    }

    private static int putBar(int[] flat, int at, boolean present, int left, int top, int right, int bottom,
                              boolean visible) {
        flat[at++] = present ? 1 : 0;
        flat[at++] = left;
        flat[at++] = top;
        flat[at++] = right;
        flat[at++] = bottom;
        flat[at++] = visible ? 1 : 0;
        return at;
    }

    // ---- Native text selection --------------------------------------------

    /**
     * Show Android's own floating Cut/Copy/Paste toolbar over the selection.
     *
     * The floating ActionMode is the part of "native selection" that IS a
     * system API. The draggable teardrop handles are NOT — no platform API
     * hands those to a custom view, every app that has them draws them itself
     * — so this wires the half Android actually owns and leaves handle
     * rendering to the engine, which already knows the selection geometry.
     *
     * `Callback2.onGetContentRect` is what positions the toolbar; it is polled
     * while the bar is up, so it re-reads the rect from Rust each time and the
     * bar follows a selection that changes under it.
     */
    public static void startSelectionToolbar(Activity activity, long nativePtr) {
        if (activity == null || nativePtr == 0L || inputView == null) {
            return;
        }
        activity.runOnUiThread(() -> {
            if (selectionMode != null) {
                selectionMode.invalidateContentRect();
                return;
            }
            selectionMode = inputView.startActionMode(
                    new SelectionCallback(nativePtr), ActionMode.TYPE_FLOATING);
        });
    }

    /** Dismiss the toolbar (selection collapsed, focus lost). */
    public static void stopSelectionToolbar(Activity activity) {
        if (activity == null) {
            return;
        }
        activity.runOnUiThread(() -> {
            if (selectionMode != null) {
                selectionMode.finish();
                selectionMode = null;
            }
        });
    }

    private static ActionMode selectionMode;

    private static final class SelectionCallback extends ActionMode.Callback2 {
        private final long nativePtr;

        SelectionCallback(long nativePtr) {
            this.nativePtr = nativePtr;
        }

        @Override
        public boolean onCreateActionMode(ActionMode mode, Menu menu) {
            // android.R.string.* so the items are in the user's language and
            // match every other text field on the device.
            menu.add(Menu.NONE, 0, 0, android.R.string.cut);
            menu.add(Menu.NONE, 1, 1, android.R.string.copy);
            menu.add(Menu.NONE, 2, 2, android.R.string.paste);
            menu.add(Menu.NONE, 3, 3, android.R.string.selectAll);
            return true;
        }

        @Override
        public boolean onPrepareActionMode(ActionMode mode, Menu menu) {
            return false;
        }

        @Override
        public boolean onActionItemClicked(ActionMode mode, MenuItem item) {
            int id = item.getItemId();
            if (id < 0 || id > 3) {
                return false;
            }
            nativeSelectionAction(nativePtr, id);
            // Cut and paste destroy the selection they acted on; copy and
            // select-all leave it, so the bar stays up for a second action.
            if (id == 0 || id == 2) {
                mode.finish();
            }
            return true;
        }

        @Override
        public void onDestroyActionMode(ActionMode mode) {
            selectionMode = null;
        }

        @Override
        public void onGetContentRect(ActionMode mode, View view, Rect outRect) {
            long packed = nativeGetSelectionRect(nativePtr);
            if (packed == 0L) {
                super.onGetContentRect(mode, view, outRect);
                return;
            }
            // Four 16-bit fields: x | y | w | h. See nativeGetSelectionRect.
            int x = (int) ((packed >> 48) & 0xFFFF);
            int y = (int) ((packed >> 32) & 0xFFFF);
            int w = (int) ((packed >> 16) & 0xFFFF);
            int h = (int) (packed & 0xFFFF);
            outRect.set(x, y, x + w, y + h);
        }
    }

    // ---- Java -> Rust: the InputConnection ---------------------------------

    /**
     * The View the IME actually attaches to.
     *
     * `onCreateInputConnection` is a VIEW method — an Activity cannot override
     * it — and NativeActivity's own content view answers null, which is why a
     * soft keyboard could never deliver anything. So we add a zero-size,
     * focusable view whose only job is to be a text editor: it takes focus,
     * reports `onCheckIsTextEditor() == true`, and hands the IME an
     * InputConnection wired to Rust. It draws nothing.
     */
    public static final class AzulInputView extends View {
        private final long nativePtr;

        public AzulInputView(Context context, long nativePtr) {
            super(context);
            this.nativePtr = nativePtr;
            setFocusable(true);
            setFocusableInTouchMode(true);
            // Zero-size and transparent: it must be in the hierarchy and
            // focusable, and must never take a pixel or a touch away from the
            // surface azul renders into.
            setWillNotDraw(true);
        }

        @Override
        public boolean onCheckIsTextEditor() {
            return true;
        }

        /**
         * Captured-pointer input (API 26, while {@link #setPointerCapture}
         * holds the capture). This is the ONLY way Android exposes a
         * DualShock/DualSense touch surface: uncaptured, the platform turns
         * it into an on-screen mouse pointer; captured, it arrives here as a
         * SOURCE_TOUCHPAD event with absolute surface positions per pointer
         * (8f-i-a-ii). Mouse events (SOURCE_MOUSE_RELATIVE) fall through.
         */
        /**
         * The framework's answer to {@link #setPointerCapture}, and the
         * notice that a capture was taken away (window focus lost). Fed to
         * Rust so the window's pointer-lock flag says what is actually held
         * (9d-android-a); the flag going false reaches the app as
         * PointerLockChange from the state diff, and nothing re-captures
         * behind the user's back.
         */
        @Override
        public void onPointerCaptureChange(boolean hasCapture) {
            super.onPointerCaptureChange(hasCapture);
            nativeOnPointerCaptureChanged(nativePtr, hasCapture ? 1 : 0);
        }

        @Override
        public boolean onCapturedPointerEvent(MotionEvent event) {
            if (com.azul.gamepad.AzulGamepad.onCapturedPointer(event)) {
                return true;
            }
            // A captured MOUSE (SOURCE_MOUSE_RELATIVE: getX/getY ARE the
            // deltas) or a captured touchpad that is not a pad's surface
            // (AXIS_RELATIVE_X/Y) is the RawMouseMotion producer on Android
            // (9d-android-b), the unbounded delta stream every other backend
            // feeds while the pointer is locked.
            int source = event.getSource();
            float dx;
            float dy;
            if ((source & InputDevice.SOURCE_MOUSE_RELATIVE) == InputDevice.SOURCE_MOUSE_RELATIVE) {
                dx = event.getX();
                dy = event.getY();
            } else if ((source & InputDevice.SOURCE_TOUCHPAD) == InputDevice.SOURCE_TOUCHPAD) {
                dx = event.getAxisValue(MotionEvent.AXIS_RELATIVE_X);
                dy = event.getAxisValue(MotionEvent.AXIS_RELATIVE_Y);
            } else {
                return super.onCapturedPointerEvent(event);
            }
            if (dx != 0f || dy != 0f) {
                nativeOnRawMotion(nativePtr, event.getDeviceId(), dx, dy);
            }
            return true;
        }

        @Override
        public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
            // The focused control's purpose, from its HTML `type` attribute.
            // Previously hardcoded to multiline text, so every field got the
            // full keyboard: a phone number field had no phone pad, an email
            // field no `@` key, and a single-line input showed a newline key
            // with no way to dismiss the keyboard.
            int hints = nativeGetInputHints(nativePtr);
            int purpose = hints & 0xFF;
            boolean multiline = ((hints >> 8) & 1) != 0;

            int type;
            int action;
            switch (purpose) {
                case 1: // Number
                    type = EditorInfo.TYPE_CLASS_NUMBER;
                    action = EditorInfo.IME_ACTION_DONE;
                    break;
                case 2: // Decimal
                    type = EditorInfo.TYPE_CLASS_NUMBER
                            | EditorInfo.TYPE_NUMBER_FLAG_DECIMAL;
                    action = EditorInfo.IME_ACTION_DONE;
                    break;
                case 3: // Phone
                    type = EditorInfo.TYPE_CLASS_PHONE;
                    action = EditorInfo.IME_ACTION_DONE;
                    break;
                case 4: // Email
                    type = EditorInfo.TYPE_CLASS_TEXT
                            | EditorInfo.TYPE_TEXT_VARIATION_EMAIL_ADDRESS;
                    action = EditorInfo.IME_ACTION_NEXT;
                    break;
                case 5: // Url
                    type = EditorInfo.TYPE_CLASS_TEXT
                            | EditorInfo.TYPE_TEXT_VARIATION_URI;
                    action = EditorInfo.IME_ACTION_GO;
                    break;
                case 6: // Password
                    // VARIATION_PASSWORD is what excludes the field from the
                    // IME's learning dictionary and suggestion strip, not just
                    // what dots the characters out.
                    type = EditorInfo.TYPE_CLASS_TEXT
                            | EditorInfo.TYPE_TEXT_VARIATION_PASSWORD;
                    action = EditorInfo.IME_ACTION_DONE;
                    break;
                case 7: // Search
                    type = EditorInfo.TYPE_CLASS_TEXT;
                    action = EditorInfo.IME_ACTION_SEARCH;
                    break;
                default: // Text
                    type = EditorInfo.TYPE_CLASS_TEXT;
                    action = EditorInfo.IME_ACTION_DONE;
                    break;
            }

            if (multiline) {
                // MULTI_LINE and an action key are mutually exclusive: Enter
                // has to be a newline, so the action becomes NONE. Setting
                // both makes Enter ambiguous and some IMEs drop the newline.
                type |= EditorInfo.TYPE_TEXT_FLAG_MULTI_LINE;
                action = EditorInfo.IME_ACTION_NONE;
            }

            outAttrs.inputType = type;
            // NO_FULLSCREEN: without it a landscape phone replaces the whole
            // app with the IME's own full-screen editor, and the document the
            // user is editing disappears behind it.
            outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN | action;
            outAttrs.initialSelStart = -1;
            outAttrs.initialSelEnd = -1;
            return new AzulInputConnection(this, nativePtr);
        }
    }

    /** The input view installed by {@link #installInputView}, if any. */
    private static AzulInputView inputView;

    /**
     * Pointer capture on the input view (Android's pointer lock, API 26):
     * the grab behind {@code CallbackInfo::set_pointer_lock} here. The view
     * must be focused and the window must have focus for the capture to
     * take; the framework answers that asynchronously through
     * {@code onPointerCaptureChange}, which AzulInputView feeds back to Rust
     * (9d-android-a).
     *
     * @return true when the request was dispatched (API 26+, view installed).
     */
    public static boolean setPointerCapture(Activity activity, boolean captured) {
        if (activity == null || inputView == null
                || Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
            return false;
        }
        final AzulInputView view = inputView;
        activity.runOnUiThread(() -> {
            try {
                if (captured) {
                    view.requestFocus();
                    view.requestPointerCapture();
                } else {
                    view.releasePointerCapture();
                }
            } catch (Throwable t) {
                Log.w("azul", "NativeTextBridge.setPointerCapture(" + captured + "): " + t);
            }
        });
        return true;
    }

    /**
     * The primary clip as plain text, or null.
     *
     * `coerceToText` rather than `getText`: a clip whose item is a URI or an
     * Intent still has a text form, and a paste that silently dropped those
     * would look like an empty clipboard. Since Android 10 the system returns
     * null here unless this app holds focus, so null means "empty, or not
     * ours to read" - the Rust side treats both as nothing to paste.
     */
    public static String getClipboardText(Activity activity) {
        if (activity == null) {
            return null;
        }
        try {
            ClipboardManager cm =
                    (ClipboardManager) activity.getSystemService(Context.CLIPBOARD_SERVICE);
            if (cm == null) {
                return null;
            }
            ClipData clip = cm.getPrimaryClip();
            if (clip == null || clip.getItemCount() == 0) {
                return null;
            }
            CharSequence text = clip.getItemAt(0).coerceToText(activity);
            return text == null ? null : text.toString();
        } catch (Throwable t) {
            Log.w("azul", "NativeTextBridge.getClipboardText: " + t);
            return null;
        }
    }

    /**
     * The primary clip's HTML markup, or null when the source published none.
     *
     * Checked against the description first: asking a plain-text clip for its
     * HTML returns null anyway, but the MIME check keeps the common case from
     * touching the item at all.
     */
    public static String getClipboardHtml(Activity activity) {
        if (activity == null) {
            return null;
        }
        try {
            ClipboardManager cm =
                    (ClipboardManager) activity.getSystemService(Context.CLIPBOARD_SERVICE);
            if (cm == null) {
                return null;
            }
            ClipData clip = cm.getPrimaryClip();
            if (clip == null || clip.getItemCount() == 0) {
                return null;
            }
            ClipDescription desc = clip.getDescription();
            if (desc != null && !desc.hasMimeType(ClipDescription.MIMETYPE_TEXT_HTML)) {
                return null;
            }
            return clip.getItemAt(0).getHtmlText();
        } catch (Throwable t) {
            Log.w("azul", "NativeTextBridge.getClipboardHtml: " + t);
            return null;
        }
    }

    /**
     * Publish the primary clip. `html` may be null for a plain clip.
     *
     * Returns false on any failure, because the caller gates Cut's deletion
     * on it: the clipboard is a Binder transaction and an oversized clip
     * throws in the system server, so "it did not land" has to be reportable.
     */
    public static boolean setClipboard(Activity activity, String text, String html) {
        if (activity == null || text == null) {
            return false;
        }
        try {
            ClipboardManager cm =
                    (ClipboardManager) activity.getSystemService(Context.CLIPBOARD_SERVICE);
            if (cm == null) {
                return false;
            }
            ClipData clip = html == null
                    ? ClipData.newPlainText("azul", text)
                    : ClipData.newHtmlText("azul", text, html);
            cm.setPrimaryClip(clip);
            return true;
        } catch (Throwable t) {
            Log.w("azul", "NativeTextBridge.setClipboard: " + t);
            return false;
        }
    }

    /**
     * Add the input view to the activity's content.
     *
     * Called once from AzulActivity, alongside the other bridges.
     */
    public static void installInputView(Activity activity, long nativePtr) {
        if (activity == null || nativePtr == 0L) {
            return;
        }
        activity.runOnUiThread(() -> {
            if (inputView != null) {
                return;
            }
            inputView = new AzulInputView(activity, nativePtr);
            // 1x1, NOT 0x0. A zero-size view is not focusable — the framework
            // skips it in focus search and `showSoftInput` then has no served
            // view to attach the IME to, so the keyboard silently never
            // appears even though every call returns success.
            activity.addContentView(inputView,
                    new android.view.ViewGroup.LayoutParams(1, 1));
            Log.i("azul", "NativeTextBridge: input view installed");
        });
    }

    /**
     * Forwards each InputConnection callback into Rust.
     *
     * Extends BaseInputConnection because it supplies the Editable bookkeeping
     * an IME expects to interrogate. Azul owns the real text, so the local
     * Editable is scratch — it exists to keep the IME's own state machine
     * happy, not to be the document.
     */
    private static final class AzulInputConnection extends BaseInputConnection {
        private final long nativePtr;
        private final Editable editable = new SpannableStringBuilder();

        AzulInputConnection(View targetView, long nativePtr) {
            super(targetView, /* fullEditor = */ true);
            this.nativePtr = nativePtr;
        }

        @Override
        public Editable getEditable() {
            return editable;
        }

        @Override
        public boolean commitText(CharSequence text, int newCursorPosition) {
            nativeCommitText(nativePtr, text == null ? "" : text.toString());
            editable.clear();
            return true;
        }

        @Override
        public boolean setComposingText(CharSequence text, int newCursorPosition) {
            nativeSetComposingText(nativePtr, text == null ? "" : text.toString(),
                    newCursorPosition);
            return true;
        }

        @Override
        public boolean finishComposingText() {
            nativeFinishComposing(nativePtr);
            editable.clear();
            return true;
        }

        @Override
        public boolean deleteSurroundingText(int beforeLength, int afterLength) {
            nativeDeleteSurrounding(nativePtr, beforeLength, afterLength);
            return true;
        }

        // The three queries that make an IME useful rather than merely
        // functional. BaseInputConnection answers them from its own local
        // Editable — an empty scratch buffer here — so autocorrect, the
        // suggestion strip, swipe typing and double-space-inserts-a-period all
        // saw a permanently blank document and offered nothing. Answered from
        // the real text instead.

        @Override
        public CharSequence getTextBeforeCursor(int n, int flags) {
            String s = nativeGetTextBeforeCursor(nativePtr, n);
            return s == null ? "" : s;
        }

        @Override
        public CharSequence getTextAfterCursor(int n, int flags) {
            String s = nativeGetTextAfterCursor(nativePtr, n);
            return s == null ? "" : s;
        }

        @Override
        public CharSequence getSelectedText(int flags) {
            // null, not "": an IME treats an empty string as "a selection that
            // exists and is empty", which changes how it offers replacements.
            return nativeGetSelectedText(nativePtr);
        }
    }

    // ---- native declarations ----------------------------------------------
    //
    // Resolved by JNI name: Java_com_azul_text_NativeTextBridge_<method>.
    // Implemented in dll/src/desktop/shell2/android/mod.rs.

    private static native void nativeCommitText(long nativePtr, String text);

    private static native void nativeSetComposingText(long nativePtr, String text,
                                                      int cursorPos);

    private static native void nativeFinishComposing(long nativePtr);

    /**
     * The focused control's IME hints, packed as {@code purpose | (multiline << 8)}.
     *
     * Packed into one call because {@link AzulInputView#onCreateInputConnection}
     * runs while the IME is opening and every JNI crossing is a chance for the
     * window to have gone away.
     */
    private static native int nativeGetInputHints(long nativePtr);

    private static native void nativeDeleteSurrounding(long nativePtr, int before, int after);

    private static native String nativeGetTextBeforeCursor(long nativePtr, int n);

    private static native String nativeGetTextAfterCursor(long nativePtr, int n);

    private static native String nativeGetSelectedText(long nativePtr);

    private static native long nativeGetSelectionRect(long nativePtr);

    private static native void nativeSelectionAction(long nativePtr, int action);

    private static native void nativeOnWindowInsets(long nativePtr, int top, int bottom,
                                                    int left, int right, int ime);

    /** The display features of `featuresArray` (the array layout: android_core/display.rs). */
    private static native void nativeOnDisplayFeatures(long nativePtr, int[] flat);

    // int, not boolean: the Rust side returns `i32` (`jint`), and a jboolean is
    // a byte — declaring it boolean here would read one byte of a four-byte
    // return.
    static native int nativeIsComposing(long nativePtr);

    private static native void nativeOnPointerCaptureChanged(long nativePtr, int hasCapture);

    private static native void nativeOnRawMotion(long nativePtr, int deviceId, float dx, float dy);

    /**
     * A key the IME and the native input queue passed by: its character and meta state go to Rust,
     * which decides whether it types (a key-down without Ctrl/Alt/Meta, not a control character).
     * Called from AzulActivity.dispatchKeyEvent for a hardware keyboard's letters and for the keys
     * {@code adb shell input text} injects: the native queue cannot read a character off a key.
     */
    public static void keyText(long nativePtr, KeyEvent event) {
        if (nativePtr == 0L || event == null) {
            return;
        }
        nativeOnKeyText(nativePtr, event.getUnicodeChar(), event.getMetaState(),
                event.getAction() == KeyEvent.ACTION_DOWN ? 1 : 0);
    }

    private static native void nativeOnKeyText(long nativePtr, int unicode, int metaState, int down);
}
