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
import android.content.Context;
import android.graphics.Insets;
import android.os.Build;
import android.text.Editable;
import android.text.SpannableStringBuilder;
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

    /**
     * Start delivering window insets to Rust.
     *
     * Called once from AzulActivity.onCreate. Requests insets immediately as
     * well as on change: the first dispatch happens before we can install a
     * listener, so a listener alone would leave the app laid out under the
     * status bar until something else moved.
     */
    public static void installInsetsListener(Activity activity, long nativePtr) {
        if (activity == null || nativePtr == 0L) {
            return;
        }
        activity.runOnUiThread(() -> {
            View root = activity.getWindow().getDecorView();
            root.setOnApplyWindowInsetsListener((v, insets) -> {
                dispatchInsets(nativePtr, insets);
                return insets;
            });
            root.requestApplyInsets();
        });
    }

    private static void dispatchInsets(long nativePtr, WindowInsets insets) {
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
        nativeOnWindowInsets(nativePtr, top, bottom, left, right, ime);
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

        @Override
        public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
            outAttrs.inputType = EditorInfo.TYPE_CLASS_TEXT
                    | EditorInfo.TYPE_TEXT_FLAG_MULTI_LINE;
            // NO_FULLSCREEN: without it a landscape phone replaces the whole
            // app with the IME's own full-screen editor, and the document the
            // user is editing disappears behind it.
            outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN
                    | EditorInfo.IME_ACTION_NONE;
            outAttrs.initialSelStart = -1;
            outAttrs.initialSelEnd = -1;
            return new AzulInputConnection(this, nativePtr);
        }
    }

    /** The input view installed by {@link #installInputView}, if any. */
    private static AzulInputView inputView;

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

    private static native void nativeDeleteSurrounding(long nativePtr, int before, int after);

    private static native String nativeGetTextBeforeCursor(long nativePtr, int n);

    private static native String nativeGetTextAfterCursor(long nativePtr, int n);

    private static native String nativeGetSelectedText(long nativePtr);

    private static native long nativeGetSelectionRect(long nativePtr);

    private static native void nativeSelectionAction(long nativePtr, int action);

    private static native void nativeOnWindowInsets(long nativePtr, int top, int bottom,
                                                    int left, int right, int ime);

    // int, not boolean: the Rust side returns `i32` (`jint`), and a jboolean is
    // a byte — declaring it boolean here would read one byte of a four-byte
    // return.
    static native int nativeIsComposing(long nativePtr);
}
