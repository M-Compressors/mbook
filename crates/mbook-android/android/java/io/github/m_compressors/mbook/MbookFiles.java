package io.github.m_compressors.mbook;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.content.Context;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import android.webkit.MimeTypeMap;

import java.io.File;
import java.io.FileNotFoundException;
import java.io.IOException;

/**
 * Lends the files mbook shares to the app the user chose, read-only.
 *
 * A file of the application cannot be read by another app, so the share
 * sheet is given a content URI of this provider, with permission to read
 * it. Only the folder of shared files, {@code <files>/cache/share}, is
 * served; the provider is not exported, so an app reads only what it was
 * granted.
 */
public final class MbookFiles extends ContentProvider {
    private static final String[] COLUMNS = {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE};

    /** The authority of the provider, named in the manifest. */
    static String authority(Context context) {
        return context.getPackageName() + ".files";
    }

    /** The folder whose files are served. */
    static File root(Context context) {
        return new File(context.getFilesDir(), "cache/share");
    }

    /**
     * The URI of {@code file}, or {@code null} when it is not in the folder of
     * shared files.
     */
    static Uri uriFor(Context context, File file) {
        try {
            String root = root(context).getCanonicalPath() + File.separator;
            String path = file.getCanonicalPath();
            if (!path.startsWith(root)) {
                return null;
            }
            Uri.Builder uri = new Uri.Builder().scheme("content").authority(authority(context));
            for (String part : path.substring(root.length()).split(File.separator)) {
                uri.appendPath(part);
            }
            return uri.build();
        } catch (IOException e) {
            return null;
        }
    }

    /** The file of {@code uri}, which must be in the folder of shared files. */
    private File fileFor(Uri uri) throws FileNotFoundException {
        Context context = getContext();
        if (context == null) {
            throw new FileNotFoundException("the provider has no context");
        }
        try {
            File root = root(context).getCanonicalFile();
            File file = new File(root, String.join(File.separator, uri.getPathSegments()))
                    .getCanonicalFile();
            if (!file.getPath().startsWith(root.getPath() + File.separator) || !file.isFile()) {
                throw new FileNotFoundException(uri.toString());
            }
            return file;
        } catch (IOException e) {
            throw new FileNotFoundException(uri.toString());
        }
    }

    /** The type of a file, from its extension; data when it has none known. */
    static String typeOf(String name) {
        int dot = name.lastIndexOf('.');
        String extension = dot < 0 ? "" : name.substring(dot + 1).toLowerCase();
        String type = MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension);
        return type != null ? type : "application/octet-stream";
    }

    @Override
    public boolean onCreate() {
        return true;
    }

    @Override
    public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        if (!"r".equals(mode)) {
            throw new FileNotFoundException("the shared files are read-only");
        }
        return ParcelFileDescriptor.open(fileFor(uri), ParcelFileDescriptor.MODE_READ_ONLY);
    }

    @Override
    public Cursor query(
            Uri uri, String[] projection, String selection, String[] args, String order) {
        File file;
        try {
            file = fileFor(uri);
        } catch (FileNotFoundException e) {
            return null;
        }
        String[] columns = projection != null ? projection : COLUMNS;
        MatrixCursor cursor = new MatrixCursor(columns, 1);
        Object[] row = new Object[columns.length];
        for (int i = 0; i < columns.length; i++) {
            if (OpenableColumns.DISPLAY_NAME.equals(columns[i])) {
                row[i] = file.getName();
            } else if (OpenableColumns.SIZE.equals(columns[i])) {
                row[i] = file.length();
            }
        }
        cursor.addRow(row);
        return cursor;
    }

    @Override
    public String getType(Uri uri) {
        return typeOf(uri.getLastPathSegment() == null ? "" : uri.getLastPathSegment());
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        throw new UnsupportedOperationException("the shared files are read-only");
    }

    @Override
    public int delete(Uri uri, String selection, String[] args) {
        throw new UnsupportedOperationException("the shared files are read-only");
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] args) {
        throw new UnsupportedOperationException("the shared files are read-only");
    }
}
