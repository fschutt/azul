// Java half of the Android accessibility bridge.
//
// Android exposes a self-drawn UI to TalkBack through a VIRTUAL VIEW
// HIERARCHY: one real View implements getAccessibilityNodeProvider() and
// vends AccessibilityNodeInfo objects for virtual children addressed by an
// int id. accesskit has no Android backend, so this — plus
// dll/src/desktop/shell2/android/accessibility.rs — is azul's own.
//
// Without this class, every button, link and text node azul draws is
// invisible to TalkBack: the app is one opaque rectangle. That is the state
// Android shipped in.
//
// Compiled outside Gradle (same as NativeGestureBridge):
//   javac -source 11 -target 11 \
//       -classpath $ANDROID_HOME/platforms/android-34/android.jar \
//       -d classes/ scripts/android/AzulAccessibilityBridge.java
//   $ANDROID_HOME/build-tools/34.0.0/d8 \
//       classes/com/azul/a11y/*.class --output dex/
//
// The nativePtr long is the address of the Rust AndroidWindow, the same
// cookie NativeGestureBridge carries. It is passed back on every call so
// this class holds no static state of its own.
//
// THREADING: every method here runs on the UI / accessibility thread. The
// Rust side answers reads from an owned snapshot behind a lock and merely
// QUEUES actions; it never touches the LayoutWindow, which belongs to the
// android_main thread.

package com.azul.a11y;

import android.graphics.Rect;
import android.os.Bundle;
import android.view.View;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.accessibility.AccessibilityNodeProvider;

import java.util.ArrayList;
import java.util.List;

public final class AzulAccessibilityBridge extends View.AccessibilityDelegate {

    /** AndroidWindow* — opaque cookie passed back to every JNI call. */
    private final long nativePtr;

    /** The View this delegate is attached to; needed for screen coordinates. */
    private View host;

    private final Provider provider = new Provider();

    public AzulAccessibilityBridge(long nativePtr) {
        this.nativePtr = nativePtr;
    }

    /** Attach to the activity's content view. Call once the AndroidWindow
     *  pointer is published (see AzulActivity.onWindowFocusChanged). */
    public void attach(View v) {
        this.host = v;
        v.setAccessibilityDelegate(this);
        // Without this the framework may decide the view is unimportant and
        // never ask for the provider at all.
        v.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_YES);
    }

    @Override
    public AccessibilityNodeProvider getAccessibilityNodeProvider(View host) {
        this.host = host;
        return provider;
    }

    // ─── Wire format (must match android/accessibility.rs) ─────────────

    private static final char FIELD_SEP = '\u0001';

    /** AccessibilityNodeProvider.HOST_VIEW_ID is API 26+; azul targets lower,
     *  and the value is -1 by contract. Named AZ_ so it cannot be confused
     *  with the constant inherited into Provider. */
    private static final int AZ_HOST_VIEW_ID = -1;

    // Action bitmask, Rust -> Java.
    private static final int ACT_CLICK           = 1;
    private static final int ACT_FOCUS           = 1 << 1;
    private static final int ACT_CLEAR_FOCUS     = 1 << 2;
    private static final int ACT_SCROLL_FORWARD  = 1 << 3;
    private static final int ACT_SCROLL_BACKWARD = 1 << 4;

    // Node flags, Rust -> Java.
    private static final int FLAG_FOCUSABLE  = 1;
    private static final int FLAG_FOCUSED    = 1 << 1;
    private static final int FLAG_ENABLED    = 1 << 2;
    private static final int FLAG_CHECKABLE  = 1 << 3;
    private static final int FLAG_CHECKED    = 1 << 4;
    private static final int FLAG_EDITABLE   = 1 << 5;
    private static final int FLAG_CLICKABLE  = 1 << 6;
    private static final int FLAG_SCROLLABLE = 1 << 7;

    // Verb ids, Java -> Rust. Deliberately NOT the framework's ACTION_*
    // constants: translating here means neither side hard-codes the other
    // platform's numbers, so a framework constant changing value cannot
    // silently retarget an azul action.
    private static final int VERB_CLICK            = 0;
    private static final int VERB_FOCUS            = 1;
    private static final int VERB_CLEAR_FOCUS      = 2;
    private static final int VERB_SCROLL_FORWARD   = 3;
    private static final int VERB_SCROLL_BACKWARD  = 4;
    private static final int VERB_A11Y_FOCUS       = 5;
    private static final int VERB_CLEAR_A11Y_FOCUS = 6;

    // ─── The provider ──────────────────────────────────────────────────

    private final class Provider extends AccessibilityNodeProvider {

        @Override
        public AccessibilityNodeInfo createAccessibilityNodeInfo(int virtualViewId) {
            if (host == null) {
                return null;
            }
            String packed = nativeDescribeNode(nativePtr, virtualViewId);
            if (packed == null) {
                // The id is not in the current snapshot (a relayout replaced
                // it). null is the documented "that virtual view is gone".
                return null;
            }

            // -1 = split with limit so trailing empty fields survive.
            String[] f = packed.split(String.valueOf(FIELD_SEP), -1);
            if (f.length < 10) {
                return null;
            }

            AccessibilityNodeInfo node =
                    (virtualViewId == AZ_HOST_VIEW_ID)
                            ? AccessibilityNodeInfo.obtain(host)
                            : AccessibilityNodeInfo.obtain(host, virtualViewId);

            node.setPackageName(host.getContext().getPackageName());
            node.setClassName(f[2]);
            if (!f[0].isEmpty()) {
                node.setContentDescription(f[0]);
            }
            if (!f[1].isEmpty()) {
                node.setText(f[1]);
            }

            // Bounds. Rust hands us view-local PHYSICAL pixels; the screen
            // rect needs the view's own offset added, which only Java knows.
            int left   = parseInt(f[3]);
            int top    = parseInt(f[4]);
            int right  = parseInt(f[5]);
            int bottom = parseInt(f[6]);
            node.setBoundsInParent(new Rect(left, top, right, bottom));
            int[] origin = new int[2];
            host.getLocationOnScreen(origin);
            node.setBoundsInScreen(new Rect(
                    origin[0] + left, origin[1] + top,
                    origin[0] + right, origin[1] + bottom));

            int actions = parseInt(f[7]);
            int flags   = parseInt(f[8]);

            node.setVisibleToUser(true);
            node.setEnabled((flags & FLAG_ENABLED) != 0);
            node.setFocusable((flags & FLAG_FOCUSABLE) != 0);
            node.setFocused((flags & FLAG_FOCUSED) != 0);
            node.setCheckable((flags & FLAG_CHECKABLE) != 0);
            node.setChecked((flags & FLAG_CHECKED) != 0);
            node.setEditable((flags & FLAG_EDITABLE) != 0);
            node.setClickable((flags & FLAG_CLICKABLE) != 0);
            node.setScrollable((flags & FLAG_SCROLLABLE) != 0);

            // Only advertise what the engine will actually act on — an
            // advertised action that gets dropped tells the user something
            // happened when nothing did.
            if ((actions & ACT_CLICK) != 0) {
                node.addAction(AccessibilityNodeInfo.ACTION_CLICK);
            }
            if ((actions & ACT_FOCUS) != 0) {
                node.addAction(AccessibilityNodeInfo.ACTION_FOCUS);
                node.addAction(AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS);
            }
            if ((actions & ACT_CLEAR_FOCUS) != 0) {
                node.addAction(AccessibilityNodeInfo.ACTION_CLEAR_FOCUS);
                node.addAction(AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS);
            }
            if ((actions & ACT_SCROLL_FORWARD) != 0) {
                node.addAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD);
            }
            if ((actions & ACT_SCROLL_BACKWARD) != 0) {
                node.addAction(AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD);
            }

            // Parent + children. A node with no parent link is unreachable by
            // TalkBack's linear navigation even when it is in the tree.
            if (virtualViewId != AZ_HOST_VIEW_ID) {
                node.setParent(host);
            }
            for (int child : parseIds(f[9])) {
                node.addChild(host, child);
            }

            return node;
        }

        @Override
        public boolean performAction(int virtualViewId, int action, Bundle args) {
            int verb;
            if (action == AccessibilityNodeInfo.ACTION_CLICK) {
                verb = VERB_CLICK;
            } else if (action == AccessibilityNodeInfo.ACTION_FOCUS) {
                verb = VERB_FOCUS;
            } else if (action == AccessibilityNodeInfo.ACTION_CLEAR_FOCUS) {
                verb = VERB_CLEAR_FOCUS;
            } else if (action == AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS) {
                verb = VERB_A11Y_FOCUS;
            } else if (action == AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS) {
                verb = VERB_CLEAR_A11Y_FOCUS;
            } else if (action == AccessibilityNodeInfo.ACTION_SCROLL_FORWARD) {
                verb = VERB_SCROLL_FORWARD;
            } else if (action == AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD) {
                verb = VERB_SCROLL_BACKWARD;
            } else {
                // Not an action azul implements. Returning false lets the
                // framework report the failure instead of us claiming success.
                return false;
            }

            boolean accepted = nativePerformAction(nativePtr, virtualViewId, verb);
            if (accepted && host != null) {
                // Tell the framework the node changed; the engine applies the
                // action on its own thread on the next frame.
                host.postInvalidate();
            }
            return accepted;
        }

        @Override
        public List<AccessibilityNodeInfo> findAccessibilityNodeInfosByText(
                String text, int virtualViewId) {
            List<AccessibilityNodeInfo> out = new ArrayList<>();
            if (text == null || text.isEmpty()) {
                return out;
            }
            String needle = text.toLowerCase();
            int count = nativeGetNodeCount(nativePtr);
            for (int i = 0; i < count; i++) {
                String packed = nativeDescribeNode(nativePtr, i);
                if (packed == null) {
                    continue;
                }
                String[] f = packed.split(String.valueOf(FIELD_SEP), -1);
                if (f.length < 10) {
                    continue;
                }
                if (f[0].toLowerCase().contains(needle)
                        || f[1].toLowerCase().contains(needle)) {
                    AccessibilityNodeInfo n = createAccessibilityNodeInfo(i);
                    if (n != null) {
                        out.add(n);
                    }
                }
            }
            return out;
        }
    }

    // ─── Parsing helpers ───────────────────────────────────────────────

    private static int parseInt(String s) {
        try {
            return Integer.parseInt(s);
        } catch (NumberFormatException e) {
            return 0;
        }
    }

    private static int[] parseIds(String csv) {
        if (csv == null || csv.isEmpty()) {
            return new int[0];
        }
        String[] parts = csv.split(",");
        int[] out = new int[parts.length];
        for (int i = 0; i < parts.length; i++) {
            out[i] = parseInt(parts[i]);
        }
        return out;
    }

    // ─── JNI extern declarations ───────────────────────────────────────

    private static native int nativeGetNodeCount(long nativePtr);

    /** Packed node record, or null when the id is not in the snapshot. */
    private static native String nativeDescribeNode(long nativePtr, int virtualViewId);

    private static native boolean nativePerformAction(
            long nativePtr, int virtualViewId, int verb);
}
