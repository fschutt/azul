// Java side of azul's `<webview>` on Android
// (dll/src/desktop/shell2/android/webview.rs). Compiled and dexed by
// scripts/build-android.sh like the other bridges in this directory.
//
// AzulActivity is a NativeActivity: it hands its window's surface to native
// code (`takeSurface`), so a View added to its own hierarchy would never be
// drawn. Each web view therefore lives in a PopupWindow of its own over the
// activity's window - the popup is the CLIP (the visible part of the view's
// box, in window pixels) and the WebView inside it is the whole box, moved,
// scaled and turned with View properties. Touches outside a popup go to the
// activity (not touch-modal), the popup is focusable so the page's fields
// take the keyboard.
//
// Every navigation of the main frame is the app's to allow
// (`shouldOverrideUrlLoading` answers "handled" and reports it; the Rust side
// calls `load` once the app's callback allowed it, so the redirect that
// carries a sign-in code can be caught). `file:` is never loaded, there is no
// JavaScript interface (no bridge), file and content access are off. One
// cookie / web-storage store per app process: an ephemeral view's data is
// cleared when the first one comes and the last one goes, unless a
// persistent view was used in this process.
//
// All UI work runs on the UI thread (`runOnUiThread`); the native callbacks
// only queue and wake the azul loop.
//
// NEEDS-RUNTIME-VERIFY (written without a device): PopupWindow placement
// against the window's origin, keyboard focus moving between the popup and
// the activity.

package com.azul.webview;

import android.app.Activity;
import android.os.Build;
import android.view.Gravity;
import android.view.View;
import android.webkit.CookieManager;
import android.webkit.RenderProcessGoneDetail;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceError;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebStorage;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.FrameLayout;
import android.widget.PopupWindow;

import java.util.HashMap;

public final class AzulWebView {

    /** One web view: its popup (the clip) and the page in it. */
    private static final class Entry {
        PopupWindow popup;
        FrameLayout clip;
        WebView web;
        boolean ephemeral;
        boolean shown;
    }

    /** By the engine's view id. UI thread only. */
    private static final HashMap<Long, Entry> VIEWS = new HashMap<>();
    private static int ephemeralViews = 0;
    private static boolean persistentUsed = false;

    private AzulWebView() { /* static-only */ }

    /** Make view `id`, hidden until placed. */
    public static void create(final Activity activity, final long id, final boolean persistent) {
        activity.runOnUiThread(() -> {
            if (VIEWS.containsKey(id)) {
                return;
            }
            Entry e = new Entry();
            e.ephemeral = !persistent;
            if (persistent) {
                persistentUsed = true;
            } else if (ephemeralViews++ == 0 && !persistentUsed) {
                clearStore();
            }
            e.clip = new FrameLayout(activity);
            e.clip.setClipChildren(true);
            e.web = new WebView(activity);
            WebSettings s = e.web.getSettings();
            s.setJavaScriptEnabled(true);
            s.setDomStorageEnabled(true);
            s.setAllowFileAccess(false);
            s.setAllowContentAccess(false);
            e.web.setWebViewClient(new Client(id));
            e.web.setWebChromeClient(new Chrome(id));
            e.web.setPivotX(0f);
            e.web.setPivotY(0f);
            e.clip.addView(e.web, new FrameLayout.LayoutParams(1, 1));
            e.popup = new PopupWindow(e.clip, 1, 1, true);
            e.popup.setClippingEnabled(false);
            e.popup.setOutsideTouchable(false);
            if (Build.VERSION.SDK_INT >= 29) {
                e.popup.setTouchModal(false);
            }
            VIEWS.put(id, e);
        });
    }

    /** Load `url` in view `id` - only ever after the app allowed it. */
    public static void load(final Activity activity, final long id, final String url) {
        activity.runOnUiThread(() -> {
            Entry e = VIEWS.get(id);
            if (e != null) {
                e.web.loadUrl(url);
            }
        });
    }

    public static void reload(final Activity activity, final long id) {
        activity.runOnUiThread(() -> {
            Entry e = VIEWS.get(id);
            if (e != null) {
                e.web.reload();
            }
        });
    }

    public static void goBack(final Activity activity, final long id) {
        activity.runOnUiThread(() -> {
            Entry e = VIEWS.get(id);
            if (e != null && e.web.canGoBack()) {
                e.web.goBack();
            }
        });
    }

    /**
     * Place view `id`: the clip at (clipX, clipY) clipW x clipH in window
     * pixels, the page inside it at (pageX, pageY) pageW x pageH, scaled and
     * turned (degrees, clockwise) about its own top-left corner.
     */
    public static void place(
            final Activity activity, final long id,
            final int clipX, final int clipY, final int clipW, final int clipH,
            final int pageX, final int pageY, final int pageW, final int pageH,
            final float scaleX, final float scaleY, final float rotation,
            final boolean visible) {
        activity.runOnUiThread(() -> {
            Entry e = VIEWS.get(id);
            if (e == null) {
                return;
            }
            if (!visible || clipW <= 0 || clipH <= 0) {
                if (e.shown) {
                    e.popup.dismiss();
                    e.shown = false;
                }
                return;
            }
            FrameLayout.LayoutParams page = new FrameLayout.LayoutParams(pageW, pageH);
            e.web.setLayoutParams(page);
            e.web.setTranslationX(pageX);
            e.web.setTranslationY(pageY);
            e.web.setScaleX(scaleX);
            e.web.setScaleY(scaleY);
            e.web.setRotation(rotation);
            View anchor = activity.getWindow().getDecorView();
            if (e.shown) {
                e.popup.update(clipX, clipY, clipW, clipH);
            } else if (anchor.getWindowToken() != null) {
                e.popup.setWidth(clipW);
                e.popup.setHeight(clipH);
                e.popup.showAtLocation(anchor, Gravity.TOP | Gravity.START, clipX, clipY);
                e.shown = true;
            }
        });
    }

    /** Destroy view `id`. */
    public static void destroy(final Activity activity, final long id) {
        activity.runOnUiThread(() -> {
            Entry e = VIEWS.remove(id);
            if (e == null) {
                return;
            }
            if (e.shown) {
                e.popup.dismiss();
            }
            e.clip.removeView(e.web);
            e.web.destroy();
            if (e.ephemeral && --ephemeralViews == 0 && !persistentUsed) {
                clearStore();
            }
        });
    }

    private static void clearStore() {
        CookieManager.getInstance().removeAllCookies(null);
        WebStorage.getInstance().deleteAllData();
    }

    private static final class Client extends WebViewClient {
        private final long id;

        Client(long id) {
            this.id = id;
        }

        @Override
        public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
            if (!request.isForMainFrame()) {
                return false;
            }
            String url = request.getUrl().toString();
            if (url.regionMatches(true, 0, "file:", 0, 5)) {
                return true;
            }
            nativeOnNavigation(id, url, request.isRedirect());
            return true;
        }

        @Override
        public void onPageFinished(WebView view, String url) {
            nativeOnPageFinished(id, url);
        }

        @Override
        public void onReceivedError(
                WebView view, WebResourceRequest request, WebResourceError error) {
            if (request.isForMainFrame()) {
                nativeOnLoadFailed(id, request.getUrl().toString(),
                        String.valueOf(error.getDescription()));
            }
        }

        @Override
        public boolean onRenderProcessGone(WebView view, RenderProcessGoneDetail detail) {
            nativeOnLoadFailed(id, String.valueOf(view.getUrl()),
                    "the page's renderer process ended");
            // Handled: the app keeps running; the view is unusable until it
            // is made again.
            return true;
        }
    }

    private static final class Chrome extends WebChromeClient {
        private final long id;

        Chrome(long id) {
            this.id = id;
        }

        @Override
        public void onReceivedTitle(WebView view, String title) {
            nativeOnTitle(id, title);
        }
    }

    static native void nativeOnNavigation(long id, String url, boolean redirect);

    static native void nativeOnPageFinished(long id, String url);

    static native void nativeOnLoadFailed(long id, String url, String reason);

    static native void nativeOnTitle(long id, String title);
}
