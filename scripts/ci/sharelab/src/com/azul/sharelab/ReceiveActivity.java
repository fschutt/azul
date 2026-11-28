package com.azul.sharelab;

import android.app.Activity;
import android.content.ClipData;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.provider.OpenableColumns;
import android.util.Log;
import android.widget.TextView;

import java.io.InputStream;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.LinkedHashSet;

/** Receives a share or an "open with" and says, in ONE log line, exactly what arrived and what could be read of it. */
public final class ReceiveActivity extends Activity {
    private static final String TAG = "AzShareLab";
    private TextView view;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        view = new TextView(this);
        view.setTextSize(14f);
        view.setPadding(32, 96, 32, 32);
        setContentView(view);
        handle(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        handle(intent);
    }

    private void handle(Intent intent) {
        LinkedHashSet<Uri> uris = new LinkedHashSet<>();
        if (intent.getData() != null) {
            uris.add(intent.getData());
        }
        Object stream = intent.getParcelableExtra(Intent.EXTRA_STREAM);
        if (stream instanceof Uri) {
            uris.add((Uri) stream);
        }
        ArrayList<Uri> list = intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM);
        if (list != null) {
            uris.addAll(list);
        }
        ClipData clip = intent.getClipData();
        if (clip != null) {
            for (int i = 0; i < clip.getItemCount(); i++) {
                if (clip.getItemAt(i).getUri() != null) {
                    uris.add(clip.getItemAt(i).getUri());
                }
            }
        }
        StringBuilder json = new StringBuilder("{");
        json.append("\"action\":").append(quote(intent.getAction()));
        json.append(",\"type\":").append(quote(intent.getType()));
        json.append(",\"text\":").append(quote(intent.getStringExtra(Intent.EXTRA_TEXT)));
        json.append(",\"subject\":").append(quote(intent.getStringExtra(Intent.EXTRA_SUBJECT)));
        json.append(",\"flags\":").append(intent.getFlags());
        json.append(",\"task\":").append(getTaskId());
        json.append(",\"items\":[");
        boolean first = true;
        for (Uri uri : uris) {
            if (!first) {
                json.append(',');
            }
            first = false;
            json.append(describe(uri));
        }
        json.append("]}");
        String line = json.toString();
        Log.i(TAG, "AZ_SHARELAB_RECEIVED " + line);
        view.setText("ShareLab received:\n\n" + line.replace(",", ",\n"));
    }

    private String describe(Uri uri) {
        StringBuilder out = new StringBuilder("{\"uri\":").append(quote(uri.toString()));
        try (Cursor c = getContentResolver().query(uri, null, null, null, null)) {
            if (c != null && c.moveToFirst()) {
                int name = c.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                int size = c.getColumnIndex(OpenableColumns.SIZE);
                out.append(",\"name\":").append(quote(name >= 0 ? c.getString(name) : null));
                out.append(",\"size\":").append(size >= 0 && !c.isNull(size) ? c.getLong(size) : -1);
            } else {
                out.append(",\"name\":null,\"size\":-1");
            }
        } catch (Exception e) {
            out.append(",\"query_error\":").append(quote(e.toString()));
        }
        out.append(",\"provider_type\":").append(quote(getContentResolver().getType(uri)));
        try (InputStream in = getContentResolver().openInputStream(uri)) {
            MessageDigest sha = MessageDigest.getInstance("SHA-256");
            byte[] buf = new byte[16 * 1024];
            long total = 0;
            StringBuilder head = new StringBuilder();
            int n;
            while ((n = in.read(buf)) > 0) {
                if (head.length() < 120) {
                    head.append(new String(buf, 0, Math.min(n, 120), "UTF-8").replaceAll("[^\\x20-\\x7e]", "?"));
                }
                sha.update(buf, 0, n);
                total += n;
            }
            StringBuilder hex = new StringBuilder();
            for (byte b : sha.digest()) {
                hex.append(String.format("%02x", b));
            }
            out.append(",\"read\":").append(total).append(",\"sha256\":\"").append(hex).append('"');
            out.append(",\"head\":").append(quote(head.length() > 120 ? head.substring(0, 120) : head.toString()));
        } catch (Exception e) {
            out.append(",\"read_error\":").append(quote(e.toString()));
        }
        out.append(",\"fd_proc\":").append(quote(reopenThroughProc(uri)));
        return out.append('}').toString();
    }

    /**
     * The /proc/self/fd question, settled on a real provider file: the descriptor a provider gave this app works (it is read above), but does
     * the PATH of that descriptor (`/proc/self/fd/N`, a magic link) open again? Answer: "ok <n> bytes" or the exception's text.
     * Handing such a path to code that wants a path (a decoder, std::fs) is only worth anything when this says ok, for every provider.
     */
    private String reopenThroughProc(Uri uri) {
        try (android.os.ParcelFileDescriptor pfd = getContentResolver().openFileDescriptor(uri, "r")) {
            if (pfd == null) {
                return "no descriptor";
            }
            String path = "/proc/self/fd/" + pfd.getFd();
            try (InputStream in = new java.io.FileInputStream(path)) {
                byte[] buf = new byte[4096];
                int n = in.read(buf);
                return "ok " + n + " bytes through " + path;
            }
        } catch (Exception e) {
            return "denied: " + e;
        }
    }

    private static String quote(String s) {
        if (s == null) {
            return "null";
        }
        StringBuilder b = new StringBuilder("\"");
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '"' || c == '\\') {
                b.append('\\').append(c);
            } else if (c < 0x20) {
                b.append(String.format("\\u%04x", (int) c));
            } else {
                b.append(c);
            }
        }
        return b.append('"').toString();
    }
}
