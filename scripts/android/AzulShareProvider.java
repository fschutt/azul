// The ContentProvider that serves the files of a share (`CallbackInfo::share`, dll/src/desktop/extra/share/android.rs) to the
// app the user picked in the chooser.
//
// A share hands over COPIES: the engine stages them in <cache>/azul-share/<token>/<name> and gives the receiver
// content://<package>.azul.share/<token>/<name>. This class serves exactly such paths, read-only, and nothing else of the
// app: a URI that is not <token>/<name> (a token of 32 lower-case hexadecimal characters, a plain visible file name), whose
// canonical file is not inside the area, or that is not a regular file is "not found". The provider is NOT exported: another
// app reaches it only through the read grant the chooser gives for the URIs of THIS share (grantUriPermissions).
//
// Why this is Java and has no native method: a provider is created in EVERY process start of the app, before the activity
// has prepared the app's folders (AzulActivity.prepareAppDirs), and it may be asked in a process that never ran the engine (a
// mail app reads an attachment hours after the sheet, from a cold process). Loading the engine's library from here would run
// the app's load-time constructor at the wrong moment and in a process with no window; the rules below are the same as
// `ShareArea::resolve` / `describe` (layout/src/managers/share_area.rs), which pin them with tests, and the contract
// dll/tests/android_share_open_contract.rs checks that the two name the same token length, folder and types file.

package com.azul.share;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.List;

public final class AzulShareProvider extends ContentProvider {

    /** The folder of the staged shares under the app's cache directory (`AREA_FOLDER` of share/android.rs). */
    private static final String AREA = "azul-share";
    /** The file of a staged share that holds the MIME type of each file (`TYPES_FILE` of share_area.rs). */
    private static final String TYPES = ".types";
    /** The length of a share's token (`TOKEN_LEN` of share_area.rs). */
    private static final int TOKEN_LEN = 32;

    @Override
    public boolean onCreate() {
        return true;
    }

    /** `token` is TOKEN_LEN lower-case hexadecimal characters. */
    private static boolean isToken(String token) {
        if (token == null || token.length() != TOKEN_LEN) {
            return false;
        }
        for (int i = 0; i < token.length(); i++) {
            char c = token.charAt(i);
            if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'))) {
                return false;
            }
        }
        return true;
    }

    /** The staged file `uri` names, or null for anything else. */
    private File resolve(Uri uri) {
        if (getContext() == null) {
            return null;
        }
        List<String> segments = uri.getPathSegments();
        if (segments.size() != 2) {
            return null;
        }
        String token = segments.get(0);
        String name = segments.get(1);
        if (!isToken(token) || name.isEmpty() || name.length() > 255 || name.startsWith(".")
                || name.indexOf('/') >= 0 || name.indexOf('\\') >= 0 || name.indexOf('\0') >= 0) {
            return null;
        }
        File root = new File(getContext().getCacheDir(), AREA);
        File file = new File(new File(root, token), name);
        try {
            String inside = root.getCanonicalPath() + File.separator;
            if (!file.getCanonicalPath().startsWith(inside)) {
                return null;
            }
        } catch (IOException e) {
            return null;
        }
        return file.isFile() ? file : null;
    }

    /** The MIME type the engine recorded for `file` (`<token>/.types`: name TAB type per line), else a stream of bytes. */
    private static String typeOf(File file) {
        File types = new File(file.getParentFile(), TYPES);
        try (BufferedReader in = new BufferedReader(
                new InputStreamReader(new FileInputStream(types), StandardCharsets.UTF_8))) {
            String line;
            while ((line = in.readLine()) != null) {
                int tab = line.indexOf('\t');
                if (tab > 0 && line.substring(0, tab).equals(file.getName())) {
                    return line.substring(tab + 1);
                }
            }
        } catch (IOException e) {
            // no types file: the generic type
        }
        return "application/octet-stream";
    }

    @Override
    public Cursor query(Uri uri, String[] projection, String selection, String[] selectionArgs, String sortOrder) {
        File file = resolve(uri);
        if (file == null) {
            return null;
        }
        String[] columns = projection != null && projection.length > 0
                ? projection
                : new String[] {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE};
        Object[] row = new Object[columns.length];
        for (int i = 0; i < columns.length; i++) {
            if (OpenableColumns.DISPLAY_NAME.equals(columns[i])) {
                row[i] = file.getName();
            } else if (OpenableColumns.SIZE.equals(columns[i])) {
                row[i] = file.length();
            }
        }
        MatrixCursor cursor = new MatrixCursor(columns, 1);
        cursor.addRow(row);
        return cursor;
    }

    @Override
    public String getType(Uri uri) {
        File file = resolve(uri);
        return file == null ? null : typeOf(file);
    }

    @Override
    public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        if (!"r".equals(mode)) {
            throw new FileNotFoundException("read only: " + uri);
        }
        File file = resolve(uri);
        if (file == null) {
            throw new FileNotFoundException(String.valueOf(uri));
        }
        return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY);
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        throw new UnsupportedOperationException("read only");
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] selectionArgs) {
        throw new UnsupportedOperationException("read only");
    }

    @Override
    public int delete(Uri uri, String selection, String[] selectionArgs) {
        throw new UnsupportedOperationException("read only");
    }
}
