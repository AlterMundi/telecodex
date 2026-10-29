package homes.dmn.interaction;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import java.io.File;
import java.io.FileNotFoundException;

/** Installed only in the isolated CI test APK; one synthetic destination. */
public class SyntheticSaveProvider extends ContentProvider {
    public boolean onCreate() { return true; }
    private File file(Uri uri) throws FileNotFoundException {
        if (!"/copy".equals(uri.getPath())) throw new FileNotFoundException();
        return new File(getContext().getCacheDir(), "synthetic-saved-copy.bin");
    }
    @Override public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        return ParcelFileDescriptor.open(file(uri), mode.startsWith("w")
            ? ParcelFileDescriptor.MODE_WRITE_ONLY | ParcelFileDescriptor.MODE_CREATE | ParcelFileDescriptor.MODE_TRUNCATE
            : ParcelFileDescriptor.MODE_READ_ONLY);
    }
    public String getType(Uri uri) { return "application/octet-stream"; }
    public Cursor query(Uri uri, String[] projection, String selection, String[] args, String sort) { return null; }
    public Uri insert(Uri uri, ContentValues values) { throw new UnsupportedOperationException(); }
    public int update(Uri uri, ContentValues values, String selection, String[] args) { throw new UnsupportedOperationException(); }
    public int delete(Uri uri, String selection, String[] args) {
        try { return file(uri).delete() ? 1 : 0; } catch (FileNotFoundException error) { return 0; }
    }
}
