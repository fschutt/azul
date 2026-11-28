package com.azul.sharelab;

import android.app.Activity;
import android.content.ClipData;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.util.Log;

import java.util.ArrayList;

/**
 * Sends an app of ours a document ShareLab serves, the way a share target of a real app does: an explicit intent for the package,
 * a read grant, and NO FLAG_ACTIVITY_NEW_TASK (the activity of the receiving app starts in the task of the sender unless its
 * launch mode says otherwise: the proof that it does not start a second copy of the app).
 *
 *   am start -n com.azul.sharelab/.SendActivity --es to com.azul.AzSelfTest --es action VIEW --es docs hello.md [--es mime text/markdown]
 *   am start -n com.azul.sharelab/.SendActivity --es to com.azul.AzSelfTest --es action SEND_MULTIPLE --es docs hello.md,plain.txt
 *   (--ez newtask true adds FLAG_ACTIVITY_NEW_TASK; --es text 'a line' adds EXTRA_TEXT; --es link 'https://example.org/x' for a link)
 */
public final class SendActivity extends Activity {
    private static final String TAG = "AzShareLab";

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Intent in = getIntent();
        String to = in.getStringExtra("to");
        String action = in.getStringExtra("action");
        String docs = in.getStringExtra("docs");
        String mime = in.getStringExtra("mime");
        String text = in.getStringExtra("text");
        ArrayList<Uri> uris = new ArrayList<>();
        if (docs != null) {
            for (String doc : docs.split(",")) {
                if (!doc.isEmpty()) {
                    uris.add(Uri.parse("content://com.azul.sharelab.docs/" + Uri.encode(doc)));
                }
            }
        }
        try {
            Intent out;
            if ("VIEW".equals(action)) {
                out = new Intent(Intent.ACTION_VIEW);
                if (!uris.isEmpty()) {
                    out.setDataAndType(uris.get(0), mime != null ? mime : "*/*");
                } else if (in.getStringExtra("link") != null) {
                    out.setData(Uri.parse(in.getStringExtra("link")));
                }
            } else if ("SEND_MULTIPLE".equals(action)) {
                out = new Intent(Intent.ACTION_SEND_MULTIPLE);
                out.setType(mime != null ? mime : "*/*");
                out.putParcelableArrayListExtra(Intent.EXTRA_STREAM, uris);
            } else {
                out = new Intent(Intent.ACTION_SEND);
                out.setType(mime != null ? mime : (uris.isEmpty() ? "text/plain" : "*/*"));
                if (!uris.isEmpty()) {
                    out.putExtra(Intent.EXTRA_STREAM, uris.get(0));
                }
            }
            if (text != null) {
                out.putExtra(Intent.EXTRA_TEXT, text);
            }
            if (!uris.isEmpty()) {
                ClipData clip = ClipData.newRawUri("lab", uris.get(0));
                for (int i = 1; i < uris.size(); i++) {
                    clip.addItem(new ClipData.Item(uris.get(i)));
                }
                out.setClipData(clip);
                out.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
            }
            if (to != null) {
                out.setPackage(to);
            }
            if (in.getBooleanExtra("newtask", false)) {
                out.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            }
            startActivity(out);
            Log.i(TAG, "AZ_SHARELAB_SENT action=" + action + " to=" + to + " task=" + getTaskId());
        } catch (Exception e) {
            Log.i(TAG, "AZ_SHARELAB_SEND_FAILED " + e);
        }
        finish();
    }
}
