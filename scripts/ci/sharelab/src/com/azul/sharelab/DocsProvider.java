package com.azul.sharelab;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;

import java.io.File;
import java.io.FileNotFoundException;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;

/** A few documents ShareLab serves to an Az app (the "open with" / "share in" direction of the proof). */
public final class DocsProvider extends ContentProvider {
    @Override
    public boolean onCreate() {
        File dir = new File(getContext().getFilesDir(), "docs");
        dir.mkdirs();
        try {
            write(dir, "hello.md", "# Hello from ShareLab\n\nA Markdown document.\n".getBytes(StandardCharsets.UTF_8));
            write(dir, "plain.txt", "plain text from ShareLab\n".getBytes(StandardCharsets.UTF_8));
            write(dir, "unicode äöü 😀.txt", "umlauts and an emoji in the name\n".getBytes(StandardCharsets.UTF_8));
            write(dir, "big.bin", new byte[3 * 1024 * 1024]);
            write(dir, "card.vcf", "BEGIN:VCARD\nVERSION:3.0\nFN:Lab Contact\nEND:VCARD\n".getBytes(StandardCharsets.UTF_8));
        } catch (IOException e) {
            return false;
        }
        return true;
    }

    private static void write(File dir, String name, byte[] bytes) throws IOException {
        try (FileOutputStream out = new FileOutputStream(new File(dir, name))) {
            out.write(bytes);
        }
    }

    private File file(Uri uri) {
        String name = uri.getLastPathSegment();
        if (name == null || name.contains("/")) {
            return null;
        }
        File f = new File(new File(getContext().getFilesDir(), "docs"), name);
        return f.isFile() ? f : null;
    }

    @Override
    public Cursor query(Uri uri, String[] projection, String selection, String[] selectionArgs, String sortOrder) {
        File f = file(uri);
        if (f == null) {
            return null;
        }
        MatrixCursor c = new MatrixCursor(new String[] {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE}, 1);
        c.addRow(new Object[] {f.getName(), f.length()});
        return c;
    }

    @Override
    public String getType(Uri uri) {
        File f = file(uri);
        if (f == null) {
            return null;
        }
        String n = f.getName();
        if (n.endsWith(".md")) {
            return "text/markdown";
        }
        if (n.endsWith(".txt")) {
            return "text/plain";
        }
        if (n.endsWith(".vcf")) {
            return "text/vcard";
        }
        return "application/octet-stream";
    }

    @Override
    public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        File f = file(uri);
        if (f == null) {
            throw new FileNotFoundException(String.valueOf(uri));
        }
        return ParcelFileDescriptor.open(f, ParcelFileDescriptor.MODE_READ_ONLY);
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        throw new UnsupportedOperationException();
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] selectionArgs) {
        throw new UnsupportedOperationException();
    }

    @Override
    public int delete(Uri uri, String selection, String[] selectionArgs) {
        throw new UnsupportedOperationException();
    }
}
