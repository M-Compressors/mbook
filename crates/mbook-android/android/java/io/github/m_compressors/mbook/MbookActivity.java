package io.github.m_compressors.mbook;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.ClipData;
import android.content.ContentResolver;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.provider.OpenableColumns;
import android.util.Log;
import android.webkit.MimeTypeMap;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.List;

import dev.gpui.mobile.GpuiInputActivity;

/**
 * The activity of mbook: the activity of gpui-mobile, which also receives the
 * documents other apps open or share with mbook.
 *
 * A document comes as a content URI that only this activity may read, and
 * only while it holds the intent. Each one is copied into the cache of the
 * application on a thread of its own, and its path waits in a queue until
 * the interface takes it with {@link #takeOpened()}.
 */
public class MbookActivity extends GpuiInputActivity {
    private static final String TAG = "mbook";

    /** The paths of the copies the interface has not taken yet. */
    private static final List<String> OPENED = new ArrayList<>();

    /** The number of the next folder of a copy. */
    private static int next;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        // A recreated activity gets its old intent again: it was taken.
        if (state == null) {
            receive(getIntent());
        }
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        receive(intent);
    }

    /**
     * The paths of the documents copied since the last call, one per line,
     * or an empty string. Called by the interface.
     */
    public static String takeOpened() {
        synchronized (OPENED) {
            String paths = String.join("\n", OPENED);
            OPENED.clear();
            return paths;
        }
    }

    /**
     * Shows the share sheet of the system with the file at {@code path}, in
     * the folder of shared files, called {@code title}. Called by the
     * interface on its own thread: the sheet is started on the thread of
     * the activity, and the call returns at once, so that the thread of the
     * interface never waits for the activity, which may be waiting for it.
     *
     * @return whether the file can be shared
     */
    public static boolean share(Activity activity, String path, String title) {
        File file = new File(path);
        Uri uri = MbookFiles.uriFor(activity, file);
        if (uri == null || !file.isFile()) {
            Log.w(TAG, "not a shared file: " + path);
            return false;
        }
        Intent send = new Intent(Intent.ACTION_SEND)
                .setType(MbookFiles.typeOf(file.getName()))
                .putExtra(Intent.EXTRA_STREAM, uri)
                .putExtra(Intent.EXTRA_SUBJECT, title)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        // The chooser passes the grant on to the app chosen through the clip.
        send.setClipData(ClipData.newRawUri(file.getName(), uri));
        Intent chooser = Intent.createChooser(send, title)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        activity.runOnUiThread(() -> {
            try {
                activity.startActivity(chooser);
            } catch (ActivityNotFoundException e) {
                Log.w(TAG, "no app can receive " + path, e);
            }
        });
        return true;
    }

    private void receive(Intent intent) {
        List<Uri> uris = documents(intent);
        if (uris.isEmpty()) {
            return;
        }
        ContentResolver resolver = getContentResolver();
        File cache = new File(getCacheDir(), "opened");
        new Thread(() -> {
            for (Uri uri : uris) {
                try {
                    File copy = copy(resolver, uri, cache);
                    synchronized (OPENED) {
                        OPENED.add(copy.getAbsolutePath());
                    }
                } catch (IOException | SecurityException e) {
                    Log.w(TAG, "could not read the document " + uri, e);
                }
            }
        }, "mbook-open").start();
    }

    /** The documents an intent to view or to send hands over. */
    private static List<Uri> documents(Intent intent) {
        List<Uri> uris = new ArrayList<>();
        if (intent == null || intent.getAction() == null) {
            return uris;
        }
        switch (intent.getAction()) {
            case Intent.ACTION_VIEW:
                if (intent.getData() != null) {
                    uris.add(intent.getData());
                }
                break;
            case Intent.ACTION_SEND: {
                Uri uri = stream(intent);
                if (uri != null) {
                    uris.add(uri);
                }
                break;
            }
            case Intent.ACTION_SEND_MULTIPLE: {
                List<Uri> streams = streams(intent);
                if (streams != null) {
                    uris.addAll(streams);
                }
                break;
            }
            default:
                break;
        }
        return uris;
    }

    @SuppressWarnings("deprecation")
    private static Uri stream(Intent intent) {
        if (Build.VERSION.SDK_INT >= 33) {
            return intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri.class);
        }
        return intent.getParcelableExtra(Intent.EXTRA_STREAM);
    }

    @SuppressWarnings("deprecation")
    private static List<Uri> streams(Intent intent) {
        if (Build.VERSION.SDK_INT >= 33) {
            return intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri.class);
        }
        return intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM);
    }

    /**
     * Copies the document at {@code uri} into a new folder under
     * {@code cache}, under the name it is shown with, and returns the copy.
     */
    private static File copy(ContentResolver resolver, Uri uri, File cache) throws IOException {
        File folder;
        synchronized (OPENED) {
            folder = new File(cache, Integer.toString(next++));
        }
        if (!folder.isDirectory() && !folder.mkdirs()) {
            throw new IOException("could not create " + folder);
        }
        File copy = new File(folder, name(resolver, uri));
        try (InputStream input = resolver.openInputStream(uri);
                OutputStream output = new FileOutputStream(copy)) {
            if (input == null) {
                throw new IOException("the provider gave no stream");
            }
            byte[] buffer = new byte[64 * 1024];
            int read;
            while ((read = input.read(buffer)) != -1) {
                output.write(buffer, 0, read);
            }
        }
        return copy;
    }

    /**
     * The name a document is shown with, which carries the extension the
     * importer reads the format from; never a path.
     */
    private static String name(ContentResolver resolver, Uri uri) {
        String name = null;
        try (Cursor cursor = resolver.query(
                uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                name = cursor.getString(0);
            }
        } catch (RuntimeException e) {
            // A provider that cannot be asked: the last part of the URI is used.
        }
        if (name == null || name.isEmpty()) {
            name = uri.getLastPathSegment();
        }
        if (name == null || name.isEmpty()) {
            name = "book";
        }
        name = new File(name).getName();
        if (name.indexOf('.') < 0) {
            String extension = MimeTypeMap.getSingleton()
                    .getExtensionFromMimeType(resolver.getType(uri));
            if (extension != null) {
                name = name + "." + extension;
            }
        }
        return name;
    }
}
