package kr.heroeslore.five;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.net.Uri;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.lang.ref.WeakReference;
import java.util.TimeZone;

/**
 * The phone's own document screens behind 환경설정's 세이브 내보내기 /
 * 불러오기 rows (hl5_core::savexfer, ANALYSIS.md §70).
 *
 * <p>The native side prepares the bundle as a file in the app's storage and
 * asks for it to be copied out ({@link #export}), or asks for a document to be
 * copied in ({@link #pick}); the answer comes back through
 * {@link #nativeDocResult}. Storage Access Framework, so no permission is
 * asked for and the player can choose any provider the phone has — the
 * Downloads folder, Google Drive, a USB stick.
 *
 * <p>Copies run on a thread of their own: a cloud provider may download inside
 * {@code openInputStream}, and that must not sit on the UI thread.
 */
final class Hl5Save {
    /** Must match {@code hl5_core::savexfer::DocStatus::from_code}. */
    static final int DONE = 0;
    static final int CANCELLED = 1;
    static final int FAILED = 2;

    private static final int REQ_EXPORT = 0x4835;
    private static final int REQ_IMPORT = 0x4836;
    /** One more than {@code hl5_core::savebundle::BUNDLE_MAX}: the native
     *  side refuses anything that size, without this side reading a whole
     *  stranger's file into the app. */
    private static final long IMPORT_CAP = (4L << 20) + 1;

    private static WeakReference<Activity> activity = new WeakReference<>(null);
    /** The app-storage file the open request is about. */
    private static String pendingPath;

    private Hl5Save() {}

    /** MainActivity, from onCreate. */
    static void attach(Activity a) {
        activity = new WeakReference<>(a);
    }

    /** For the dates in the native side's popups and the suggested name. */
    static int utcOffsetMinutes() {
        return TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 60000;
    }

    /** Offer "save as" for {@code path}, suggesting {@code name}. False if
     *  there is no activity to ask from; the native side then reports failure. */
    static boolean export(final String path, final String name) {
        final Activity a = activity.get();
        if (a == null) {
            return false;
        }
        a.runOnUiThread(() -> {
            pendingPath = path;
            Intent i = new Intent(Intent.ACTION_CREATE_DOCUMENT);
            i.addCategory(Intent.CATEGORY_OPENABLE);
            // Not application/zip: some providers append .zip to the name.
            i.setType("application/octet-stream");
            i.putExtra(Intent.EXTRA_TITLE, name);
            try {
                a.startActivityForResult(i, REQ_EXPORT);
            } catch (ActivityNotFoundException e) {
                new File(path).delete();
                nativeDocResult(FAILED);
            }
        });
        return true;
    }

    /** Offer "open", and copy what is picked to {@code path}. */
    static boolean pick(final String path) {
        final Activity a = activity.get();
        if (a == null) {
            return false;
        }
        a.runOnUiThread(() -> {
            pendingPath = path;
            Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            i.addCategory(Intent.CATEGORY_OPENABLE);
            // A .h5save has no type a provider knows; the native side checks
            // the contents instead.
            i.setType("*/*");
            try {
                a.startActivityForResult(i, REQ_IMPORT);
            } catch (ActivityNotFoundException e) {
                nativeDocResult(FAILED);
            }
        });
        return true;
    }

    /** From MainActivity.onActivityResult. True if the request was ours. */
    static boolean onActivityResult(final Activity a, int requestCode, int resultCode, Intent data) {
        if (requestCode != REQ_EXPORT && requestCode != REQ_IMPORT) {
            return false;
        }
        final String path = pendingPath;
        pendingPath = null;
        final Uri uri = data != null ? data.getData() : null;
        if (resultCode != Activity.RESULT_OK || uri == null || path == null) {
            if (requestCode == REQ_EXPORT && path != null) {
                new File(path).delete();
            }
            nativeDocResult(CANCELLED);
            return true;
        }
        final boolean export = requestCode == REQ_EXPORT;
        new Thread(() -> {
            int status = DONE;
            try {
                if (export) {
                    try (InputStream in = new FileInputStream(path);
                         OutputStream out = a.getContentResolver().openOutputStream(uri, "wt")) {
                        if (out == null) {
                            throw new IOException("no stream for " + uri);
                        }
                        copy(in, out, Long.MAX_VALUE);
                    }
                    new File(path).delete();
                } else {
                    try (InputStream in = a.getContentResolver().openInputStream(uri);
                         OutputStream out = new FileOutputStream(path)) {
                        if (in == null) {
                            throw new IOException("no stream for " + uri);
                        }
                        copy(in, out, IMPORT_CAP);
                    }
                }
            } catch (IOException | SecurityException | IllegalArgumentException e) {
                android.util.Log.w("hl5", "save document copy failed: " + e);
                status = FAILED;
            }
            nativeDocResult(status);
        }, "hl5-save-doc").start();
        return true;
    }

    private static void copy(InputStream in, OutputStream out, long cap) throws IOException {
        byte[] buf = new byte[64 * 1024];
        long total = 0;
        int n;
        while (total < cap && (n = in.read(buf, 0, (int) Math.min(buf.length, cap - total))) > 0) {
            out.write(buf, 0, n);
            total += n;
        }
        out.flush();
    }

    /** Implemented in Rust — lib.rs `save_jni`. Any thread. */
    private static native void nativeDocResult(int status);
}
