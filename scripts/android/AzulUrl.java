// Open a URL (or the app's own page in the system settings) in the app that handles it.
//
// `Url::open` has no opener process on a phone: the platform opens a link by starting an ACTIVITY with an intent. The
// call comes from the engine's loop thread; `Context.startActivity` may be called from any thread, so the answer is the
// real one: false when nothing on the device handles the URL (ActivityNotFoundException) or the system refuses the
// start, never a silent "true".
//
// A URL is opened with ACTION_VIEW in a NEW TASK: a browser or a map app is another app, and the back button returns
// to this one.
//
// The pseudo URL `azul-settings:app` is the app's OWN page in the system settings (permissions, notifications, battery):
// where an app sends a user who refused a permission for good, since Android shows no dialog for it any more
// (Settings.ACTION_APPLICATION_DETAILS_SETTINGS with the package: URI of the app).

package com.azul.url;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.net.Uri;
import android.provider.Settings;

public final class AzulUrl {

    private AzulUrl() {}

    /** The pseudo URL of the app's own page in the system settings. */
    public static final String APP_SETTINGS = "azul-settings:app";

    /**
     * Start the handler of `url`. Returns whether an activity was started.
     */
    public static boolean open(Activity activity, String url) {
        if (activity == null || url == null || url.isEmpty()) {
            return false;
        }
        try {
            Intent intent;
            if (APP_SETTINGS.equals(url)) {
                intent = new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                        Uri.fromParts("package", activity.getPackageName(), null));
            } else {
                intent = new Intent(Intent.ACTION_VIEW, Uri.parse(url));
                intent.addCategory(Intent.CATEGORY_BROWSABLE);
            }
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            activity.startActivity(intent);
            return true;
        } catch (ActivityNotFoundException e) {
            return false;
        } catch (RuntimeException e) {
            // SecurityException (an exported-activity rule), a malformed URI.
            return false;
        }
    }
}
