// Native notifications on Android: the Java half of
// dll/src/desktop/notifications/android.rs.
//
// Rust decides WHAT to show (and names it: the app's notification id is the
// NotificationManager TAG); this file does what only Java can do well:
//
//   * build the Notification with the framework builder (no AndroidX), on a
//     channel (required from API 26: without one nothing is shown);
//   * make PendingIntents back into the app. A tap or a button opens THIS
//     activity (`getActivity`: trampolines through a receiver are banned on
//     Android 12+), and the activity is `launchMode="singleTop"`, so a tap on
//     a running app arrives in AzulActivity.onNewIntent instead of starting a
//     second NativeActivity. A swipe-away arrives at the manifest Receiver
//     below through the delete intent;
//   * forward all of it to Rust (`nativeOnNotificationEvent`). A cold start
//     by a tap is forwarded from AzulActivity.onCreate with launchedApp=true;
//     a dismissal whose process was dead (the Receiver runs WITHOUT the native
//     library loaded) is kept in SharedPreferences and forwarded at the next
//     start.
//
// Every intent carries the notification id, the action ("default" = the
// body, "azul.dismiss" = the delete intent, otherwise the button's id) and the
// app's payload - the constants match `wire::ANDROID_*` on the Rust side.
// Request codes come from Rust (`wire::android_request_code`, distinct per
// notification, low nibble zero): +0 the body, +1.. the buttons, +15 the
// delete intent. Intents that differ only in extras are the SAME
// PendingIntent, so the codes are what keeps two notifications apart.
//
// Lenient by construction: every entry point catches what it can and reports
// a failure as a string (post) or drops the event, because an exception
// escaping into NativeActivity or a receiver takes the app down.

package com.azul.notify;

import android.app.Activity;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.graphics.drawable.Icon;
import android.os.Build;

import org.json.JSONArray;
import org.json.JSONException;

public final class AzulNotifications {

    private AzulNotifications() {}

    public static final String EXTRA_ID = "azul.notification.id";
    public static final String EXTRA_ACTION = "azul.notification.action";
    public static final String EXTRA_PAYLOAD = "azul.notification.payload";
    /** A tap on the body. `wire::ANDROID_DEFAULT_ACTION`. */
    public static final String ACTION_DEFAULT = "default";
    /** The delete intent. `wire::ANDROID_DISMISS_ACTION`. */
    public static final String ACTION_DISMISS = "azul.dismiss";
    /** The broadcast action of the delete intent (explicit, to Receiver). */
    private static final String BROADCAST_DISMISS = "com.azul.notify.DISMISS";

    private static final String CHANNEL_DEFAULT = "azul.default";
    private static final String CHANNEL_SILENT = "azul.silent";
    private static final String PERMISSION = "android.permission.POST_NOTIFICATIONS";

    private static final String PREFS = "azul.notifications";
    private static final String PREFS_QUEUE = "pending";
    private static final int MAX_PERSISTED = 64;

    /** Set by AzulActivity once System.loadLibrary succeeded in THIS process. */
    private static volatile boolean nativeReady = false;

    public static void markNativeReady() {
        nativeReady = true;
    }

    public static int sdkInt() {
        return Build.VERSION.SDK_INT;
    }

    private static NotificationManager manager(Context context) {
        if (context == null) {
            return null;
        }
        return (NotificationManager) context.getSystemService(Context.NOTIFICATION_SERVICE);
    }

    /** The app's notification switch (Settings > Apps > Notifications). */
    public static boolean areEnabled(Context context) {
        NotificationManager nm = manager(context);
        if (nm == null) {
            return false;
        }
        return nm.areNotificationsEnabled();
    }

    /**
     * 0 = never asked (API 33+: POST_NOTIFICATIONS not yet requested),
     * 1 = denied or switched off, 2 = granted.
     */
    public static int permissionState(Activity activity) {
        if (activity == null) {
            return 0;
        }
        try {
            if (Build.VERSION.SDK_INT >= 33) {
                if (activity.checkSelfPermission(PERMISSION) == PackageManager.PERMISSION_GRANTED) {
                    return areEnabled(activity) ? 2 : 1;
                }
                // The rationale flag is true only after a real denial; a
                // fresh install and "don't ask again" both read false, and
                // the request that follows learns which it was.
                return activity.shouldShowRequestPermissionRationale(PERMISSION) ? 1 : 0;
            }
            return areEnabled(activity) ? 2 : 1;
        } catch (Throwable t) {
            return 0;
        }
    }

    /** The channel a notification goes to; created on first use. */
    private static String ensureChannel(NotificationManager nm, String name, boolean silent) {
        String id = silent ? CHANNEL_SILENT : CHANNEL_DEFAULT;
        if (nm.getNotificationChannel(id) == null) {
            // IMPORTANCE_DEFAULT (3): sound, status bar, no heads-up.
            // IMPORTANCE_LOW (2): no sound. A channel's sound cannot change
            // after creation, which is why "silent" is its own channel.
            NotificationChannel channel = new NotificationChannel(
                    id,
                    silent ? name + " (silent)" : name,
                    silent ? NotificationManager.IMPORTANCE_LOW
                           : NotificationManager.IMPORTANCE_DEFAULT);
            nm.createNotificationChannel(channel);
        }
        return id;
    }

    /** The launcher icon, or a framework one when the APK has none. */
    private static int smallIcon(Context context) {
        int icon = context.getApplicationInfo().icon;
        return icon != 0 ? icon : android.R.drawable.ic_dialog_info;
    }

    private static PendingIntent activityIntent(Activity activity, int requestCode, String tag,
                                                String action, String payload, int flags) {
        Intent intent = new Intent(activity, activity.getClass());
        intent.setFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP);
        intent.putExtra(EXTRA_ID, tag);
        intent.putExtra(EXTRA_ACTION, action);
        intent.putExtra(EXTRA_PAYLOAD, payload);
        return PendingIntent.getActivity(activity, requestCode, intent, flags);
    }

    private static PendingIntent dismissIntent(Context context, int requestCode, String tag,
                                               String payload, int flags) {
        Intent intent = new Intent(context, Receiver.class);
        intent.setAction(BROADCAST_DISMISS);
        intent.putExtra(EXTRA_ID, tag);
        intent.putExtra(EXTRA_ACTION, ACTION_DISMISS);
        intent.putExtra(EXTRA_PAYLOAD, payload);
        return PendingIntent.getBroadcast(context, requestCode, intent, flags);
    }

    /**
     * Show (or, under the same tag, replace) a notification.
     *
     * @return "" when it was handed to the NotificationManager, otherwise why
     *         not - Rust turns that into a `Failed` event.
     */
    public static String post(Activity activity, String channelName, String tag, int requestCode,
                              String title, String body, String[] actionIds,
                              String[] actionLabels, String payload, boolean silent,
                              int pendingIntentFlags) {
        try {
            NotificationManager nm = manager(activity);
            if (nm == null) {
                return "the NotificationManager service is unavailable";
            }
            if (!areEnabled(activity)) {
                return "notifications are turned off for this app (Settings > Apps > "
                        + "Notifications), or POST_NOTIFICATIONS was not granted";
            }
            Notification.Builder builder;
            if (Build.VERSION.SDK_INT >= 26) {
                builder = new Notification.Builder(activity, ensureChannel(nm, channelName, silent));
            } else {
                builder = new Notification.Builder(activity);
                builder.setDefaults(silent ? 0
                        : (Notification.DEFAULT_SOUND | Notification.DEFAULT_LIGHTS));
            }
            builder.setSmallIcon(smallIcon(activity));
            builder.setContentTitle(title);
            if (body != null && !body.isEmpty()) {
                builder.setContentText(body);
            }
            builder.setAutoCancel(true);
            builder.setContentIntent(
                    activityIntent(activity, requestCode, tag, ACTION_DEFAULT, payload,
                            pendingIntentFlags));
            builder.setDeleteIntent(
                    dismissIntent(activity, requestCode + 15, tag, payload, pendingIntentFlags));
            if (actionIds != null && actionLabels != null) {
                int n = Math.min(Math.min(actionIds.length, actionLabels.length), 3);
                for (int i = 0; i < n; i++) {
                    PendingIntent pi = activityIntent(activity, requestCode + 1 + i, tag,
                            actionIds[i], payload, pendingIntentFlags);
                    builder.addAction(
                            new Notification.Action.Builder((Icon) null, actionLabels[i], pi)
                                    .build());
                }
            }
            nm.notify(tag, 0, builder.build());
            return "";
        } catch (Throwable t) {
            String why = String.valueOf(t);
            return why.isEmpty() ? "NotificationManager.notify failed" : why;
        }
    }

    /** Take the notification posted under `tag` away. */
    public static void cancel(Activity activity, String tag) {
        try {
            NotificationManager nm = manager(activity);
            if (nm != null) {
                nm.cancel(tag, 0);
            }
        } catch (Throwable t) {
            // Nothing to report: a notification that is already gone is
            // exactly what a withdraw wants.
        }
    }

    /**
     * An intent that reached AzulActivity: onCreate (launchedApp = true, for
     * the tap that cold-started the app) or onNewIntent (a tap on a running
     * app). Returns whether it was a notification's. Consumes the extras, so
     * a recreated activity does not report the same tap twice.
     */
    public static boolean onIntent(Activity activity, Intent intent, boolean launchedApp) {
        if (intent == null) {
            return false;
        }
        String id = intent.getStringExtra(EXTRA_ID);
        if (id == null) {
            return false;
        }
        String action = intent.getStringExtra(EXTRA_ACTION);
        String payload = intent.getStringExtra(EXTRA_PAYLOAD);
        intent.removeExtra(EXTRA_ID);
        intent.removeExtra(EXTRA_ACTION);
        intent.removeExtra(EXTRA_PAYLOAD);
        deliver(activity, id, action == null ? ACTION_DEFAULT : action,
                payload == null ? "" : payload, launchedApp);
        return true;
    }

    /** To Rust if the library is loaded; kept for the next start otherwise. */
    private static void deliver(Context contextForPersisting, String id, String action,
                                String payload, boolean launchedApp) {
        if (nativeReady) {
            try {
                nativeOnNotificationEvent(id, action, payload, launchedApp);
                return;
            } catch (UnsatisfiedLinkError e) {
                // Fall through to persisting.
            }
        }
        if (contextForPersisting != null) {
            persist(contextForPersisting, id, action, payload);
        }
    }

    private static synchronized void persist(Context context, String id, String action,
                                             String payload) {
        try {
            SharedPreferences prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
            JSONArray queue = new JSONArray(prefs.getString(PREFS_QUEUE, "[]"));
            if (queue.length() >= MAX_PERSISTED) {
                return;
            }
            JSONArray entry = new JSONArray();
            entry.put(id);
            entry.put(action);
            entry.put(payload);
            queue.put(entry);
            prefs.edit().putString(PREFS_QUEUE, queue.toString()).apply();
        } catch (JSONException e) {
            // A corrupt queue is dropped rather than repaired.
        } catch (Throwable t) {
            // Never let a receiver crash the process.
        }
    }

    /**
     * Forward what a Receiver kept while the native library was not loaded.
     * Called from AzulActivity.onCreate once the library is.
     */
    public static synchronized void drainPersisted(Context context) {
        if (context == null || !nativeReady) {
            return;
        }
        try {
            SharedPreferences prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
            String raw = prefs.getString(PREFS_QUEUE, null);
            if (raw == null) {
                return;
            }
            prefs.edit().remove(PREFS_QUEUE).apply();
            JSONArray queue = new JSONArray(raw);
            for (int i = 0; i < queue.length(); i++) {
                JSONArray entry = queue.getJSONArray(i);
                deliver(null, entry.getString(0), entry.getString(1), entry.getString(2), false);
            }
        } catch (JSONException e) {
            // Dropped, see persist.
        } catch (Throwable t) {
            // Never let a start-up crash on this.
        }
    }

    /**
     * The delete intent's receiver (declared in the manifest, not exported).
     * It can run in a process whose activity never started - the user
     * cleared the notification of an app that is not running - so it
     * persists the event when the native library is not there to take it.
     */
    public static final class Receiver extends BroadcastReceiver {
        @Override
        public void onReceive(Context context, Intent intent) {
            if (intent == null) {
                return;
            }
            String id = intent.getStringExtra(EXTRA_ID);
            if (id == null) {
                return;
            }
            String payload = intent.getStringExtra(EXTRA_PAYLOAD);
            deliver(context, id, ACTION_DISMISS, payload == null ? "" : payload, false);
        }
    }

    /** Implemented in Rust (dll/src/desktop/notifications/android.rs). */
    private static native void nativeOnNotificationEvent(String id, String action, String payload,
                                                         boolean launchedApp);
}
