// Java bridge for the Storage Access Framework file picker. Called from
// Rust via JNI; the Rust side handles the `FilePickerHandle` registry
// and writes the resulting paths back through `nativeOnResult`.
//
// Compiled outside Gradle (build-android.sh handles this automatically
// once the file is present alongside NativeGestureBridge.java):
//
//   javac -source 11 -target 11 \
//       -classpath $ANDROID_HOME/platforms/android-34/android.jar \
//       -d classes/ scripts/android/AzulFilePicker.java \
//                   scripts/android/AzulActivity.java
//   $ANDROID_HOME/build-tools/34.0.0/d8 \
//       classes/com/azul/picker/*.class classes/com/azul/app/*.class \
//       --output dex/
//
// AzulActivity.onActivityResult routes incoming results back into this
// class. The `requestId` long is the same opaque cookie the Rust side
// registered in its `PENDING_PICKERS` map, encoded into the low bits of
// the Android activity-result request code.

package com.azul.picker;

import android.app.Activity;
import android.content.ContentResolver;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.provider.OpenableColumns;
import android.util.Log;

import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicInteger;

public final class AzulFilePicker {

    private static final String TAG = "AzulFilePicker";

    /** Base for request codes — keeps us clear of any other code paths the
     *  framework may use (gestures, IME, etc.). Real request code is
     *  REQUEST_CODE_BASE + ticket, where ticket is a small per-call
     *  counter. */
    private static final int REQUEST_CODE_BASE = 0x4A5400;

    private static final AtomicInteger NEXT_TICKET = new AtomicInteger(1);

    /** ticket → requestId cookie passed back to native via
     *  nativeOnResult. The Rust side looks up the FilePickerHandle by
     *  requestId in its PENDING_PICKERS map. */
    private static final ConcurrentHashMap<Integer, Long> PENDING =
            new ConcurrentHashMap<>();

    private AzulFilePicker() { /* static-only */ }

    /** Dispatch a single-file or multi-file open picker. Called from Rust
     *  via JNI. Returns immediately; the result arrives via
     *  AzulActivity.onActivityResult → onActivityResultProxy →
     *  nativeOnResult. */
    public static void pickDocument(
            Activity activity,
            long requestId,
            String[] mimeTypes,
            boolean allowMultiple) {
        if (activity == null) {
            Log.w(TAG, "pickDocument: null activity");
            nativeOnResult(requestId, new String[0], "no activity context");
            return;
        }

        int ticket = NEXT_TICKET.getAndIncrement();
        PENDING.put(ticket, requestId);

        try {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);

            if (mimeTypes == null || mimeTypes.length == 0) {
                intent.setType("*/*");
            } else if (mimeTypes.length == 1) {
                intent.setType(mimeTypes[0]);
            } else {
                intent.setType("*/*");
                intent.putExtra(Intent.EXTRA_MIME_TYPES, mimeTypes);
            }

            if (allowMultiple) {
                intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
            }

            activity.startActivityForResult(intent, REQUEST_CODE_BASE + ticket);
        } catch (Exception e) {
            Log.e(TAG, "startActivityForResult failed", e);
            PENDING.remove(ticket);
            nativeOnResult(requestId, new String[0], "startActivityForResult: " + e.getMessage());
        }
    }

    /** Save-file dispatch via ACTION_CREATE_DOCUMENT. */
    public static void saveDocument(
            Activity activity,
            long requestId,
            String suggestedName,
            String mimeType) {
        if (activity == null) {
            nativeOnResult(requestId, new String[0], "no activity context");
            return;
        }

        int ticket = NEXT_TICKET.getAndIncrement();
        PENDING.put(ticket, requestId);

        try {
            Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType(mimeType == null || mimeType.isEmpty() ? "*/*" : mimeType);
            if (suggestedName != null && !suggestedName.isEmpty()) {
                intent.putExtra(Intent.EXTRA_TITLE, suggestedName);
            }
            activity.startActivityForResult(intent, REQUEST_CODE_BASE + ticket);
        } catch (Exception e) {
            Log.e(TAG, "startActivityForResult(CREATE_DOCUMENT) failed", e);
            PENDING.remove(ticket);
            nativeOnResult(requestId, new String[0], "startActivityForResult: " + e.getMessage());
        }
    }

    /** Directory picker via ACTION_OPEN_DOCUMENT_TREE. Returns a single
     *  `content://...tree/...` URI as a String; conversion to a usable path
     *  is the caller's job (SAF tree URIs aren't filesystem paths). */
    public static void pickDirectory(Activity activity, long requestId) {
        if (activity == null) {
            nativeOnResult(requestId, new String[0], "no activity context");
            return;
        }
        int ticket = NEXT_TICKET.getAndIncrement();
        PENDING.put(ticket, requestId);
        try {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
            activity.startActivityForResult(intent, REQUEST_CODE_BASE + ticket);
        } catch (Exception e) {
            Log.e(TAG, "startActivityForResult(OPEN_DOCUMENT_TREE) failed", e);
            PENDING.remove(ticket);
            nativeOnResult(requestId, new String[0], "startActivityForResult: " + e.getMessage());
        }
    }

    /** Called from AzulActivity.onActivityResult for every request code in
     *  our REQUEST_CODE_BASE window. Returns true if we handled the
     *  request (so AzulActivity knows it doesn't need to fall through). */
    public static boolean onActivityResultProxy(
            Activity activity,
            int requestCode,
            int resultCode,
            Intent data) {
        if (requestCode < REQUEST_CODE_BASE
                || requestCode >= REQUEST_CODE_BASE + (1 << 16)) {
            return false;
        }
        int ticket = requestCode - REQUEST_CODE_BASE;
        Long requestIdBoxed = PENDING.remove(ticket);
        if (requestIdBoxed == null) {
            Log.w(TAG, "no pending entry for ticket " + ticket);
            return true;
        }
        long requestId = requestIdBoxed;

        if (resultCode != Activity.RESULT_OK || data == null) {
            // RESULT_CANCELED, or null intent (back button / outside tap).
            nativeOnResult(requestId, new String[0], null);
            return true;
        }

        ArrayList<String> paths = new ArrayList<>();
        try {
            if (data.getClipData() != null) {
                int count = data.getClipData().getItemCount();
                for (int i = 0; i < count; i++) {
                    Uri uri = data.getClipData().getItemAt(i).getUri();
                    String p = uriToCachedFile(activity, uri);
                    if (p != null) paths.add(p);
                }
            } else if (data.getData() != null) {
                Uri uri = data.getData();
                String p = uriToCachedFile(activity, uri);
                if (p != null) paths.add(p);
            }
        } catch (Exception e) {
            Log.e(TAG, "result extraction failed", e);
            nativeOnResult(requestId, new String[0], "extraction: " + e.getMessage());
            return true;
        }

        nativeOnResult(requestId, paths.toArray(new String[0]), null);
        return true;
    }

    /** Copy a SAF `content://` URI into the app's cache dir and return the
     *  resulting `file://`-style absolute path. Mirrors iOS asCopy:YES so
     *  the Rust side sees the same flow on both platforms. Returns null
     *  on failure. */
    private static String uriToCachedFile(Activity activity, Uri uri) {
        if (uri == null) return null;
        ContentResolver resolver = activity.getContentResolver();
        String name = queryDisplayName(resolver, uri);
        if (name == null) name = "picked-" + System.currentTimeMillis();

        File outDir = activity.getCacheDir();
        // Avoid name collisions across calls.
        File outFile = new File(outDir, ticketedName(name));
        try (InputStream in = resolver.openInputStream(uri);
             OutputStream out = new FileOutputStream(outFile)) {
            if (in == null) return null;
            byte[] buf = new byte[16 * 1024];
            int n;
            while ((n = in.read(buf)) > 0) {
                out.write(buf, 0, n);
            }
        } catch (Exception e) {
            Log.e(TAG, "copy failed for " + uri, e);
            return null;
        }
        return outFile.getAbsolutePath();
    }

    private static String queryDisplayName(ContentResolver resolver, Uri uri) {
        try (Cursor c = resolver.query(uri, null, null, null, null)) {
            if (c != null && c.moveToFirst()) {
                int idx = c.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                if (idx >= 0) return c.getString(idx);
            }
        } catch (Exception e) {
            Log.w(TAG, "queryDisplayName failed for " + uri, e);
        }
        return null;
    }

    private static String ticketedName(String name) {
        // Sanitize + uniquify so concurrent calls don't clobber each other.
        String safe = name.replaceAll("[^A-Za-z0-9._-]", "_");
        return System.currentTimeMillis() + "_" + safe;
    }

    /** Implemented in Rust (dll/src/desktop/extra/file_picker/android.rs). */
    private static native void nativeOnResult(long requestId, String[] paths, String errorOrNull);
}
