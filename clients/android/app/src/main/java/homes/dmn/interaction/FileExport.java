package homes.dmn.interaction;

import android.content.Context;
import android.net.Uri;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.provider.DocumentsContract;
import android.util.Base64;
import android.util.Log;
import org.json.JSONObject;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.security.MessageDigest;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.BiConsumer;
import java.util.function.Consumer;

/** Single temporary export to a destination returned by ACTION_CREATE_DOCUMENT. */
final class FileExport implements AutoCloseable {
    private static final long LIFETIME = 10 * 60 * 1000L;
    private final Context context;
    private final Handler main = new Handler(Looper.getMainLooper());
    private final ThreadPoolExecutor worker = new ThreadPoolExecutor(1, 1, 0, TimeUnit.SECONDS, new ArrayBlockingQueue<>(4));
    private final AtomicLong epoch = new AtomicLong();
    private final Consumer<JSONObject> emit;
    private final BiConsumer<String, String> choose;
    private final File directory;
    private VerifiedFileStage stage;
    private volatile String id;
    private volatile boolean choosing, closed;
    private String name, mime;
    private long deadline;
    private Runnable expiry;

    FileExport(Context context, Consumer<JSONObject> emit, BiConsumer<String, String> choose) {
        this.context = context.getApplicationContext(); this.emit = emit; this.choose = choose;
        directory = new File(context.getCacheDir(), "verified-file-export");
        submit(null, () -> { try { cleanStale(); } catch (IOException error) { Log.w("DaimonFiles", "Transient export cleanup pending"); } });
    }
    boolean choosing() { return choosing; }
    private void event(String selected, String phase, String status, int sequence, String message) {
        JSONObject value = new JSONObject();
        try {
            value.put("type", "file").put("id", selected).put("stage", phase).put("status", status);
            if (sequence >= 0) value.put("sequence", sequence);
            if (message != null) value.put("message", message);
        } catch (Exception ignored) { }
        main.post(() -> emit.accept(value));
    }
    private void submit(String selected, Runnable action) {
        if (closed) return;
        try { worker.execute(action); }
        catch (RuntimeException error) { event(selected, "result", "error", -1, "Save is busy. The original remains available."); }
    }
    private void check(long owned) throws IOException {
        if (owned != epoch.get() || closed || SystemClock.elapsedRealtime() >= deadline) throw new IOException("Save interrupted or expired");
    }
    private void clear() {
        choosing = false; id = null;
        if (expiry != null) main.removeCallbacks(expiry); expiry = null;
        if (stage != null) {
            try { stage.close(); } catch (IOException error) { Log.w("DaimonFiles", "Transient export cleanup pending"); }
            stage = null;
        }
    }
    private void cleanStale() throws IOException {
        File[] files = directory.listFiles();
        if (files != null) for (File file : files) {
            if (!file.getName().startsWith("verified-") || !file.getName().endsWith(".tmp") || !file.isFile() || !file.delete())
                throw new IOException("Save staging cleanup unavailable");
        }
    }
    void command(JSONObject value) {
        String selected = value.optString("id"), command = value.optString("command");
        if (!selected.matches("export_[a-zA-Z0-9_-]{1,72}")) return;
        if (command.equals("file_cancel")) { if (selected.equals(id)) cancel("Save cancelled; the original remains available"); return; }
        long owned = epoch.get();
        submit(selected, () -> {
            if (owned != epoch.get() || closed) return;
            if (command.equals("file_begin") && id != null) {
                event(selected, "begin", "error", -1, "Finish the current save first"); return;
            }
            if (!command.equals("file_begin") && !selected.equals(id)) return;
            try {
                if (command.equals("file_begin")) {
                    cleanStale();
                    name = value.getString("name"); mime = value.getString("mime");
                    if (name.isEmpty() || name.length() > 180 || name.chars().anyMatch(c -> c == 47 || c == 92 || Character.isISOControl(c))) throw new IOException("Invalid save filename");
                    if (!mime.matches("[a-zA-Z0-9!#$&^_.+-]+/[a-zA-Z0-9!#$&^_.+-]+")) mime = "application/octet-stream";
                    deadline = SystemClock.elapsedRealtime() + LIFETIME;
                    stage = new VerifiedFileStage(directory, value.getLong("bytes"), value.getString("sha256")); check(owned); id = selected;
                    expiry = () -> { if (owned == epoch.get() && selected.equals(id)) cancel("Save expired; the original remains available"); };
                    main.postDelayed(expiry, LIFETIME); event(selected, "begin", "ok", -1, null); return;
                }
                check(owned);
                if (choosing) throw new IOException("Save destination already requested");
                if (command.equals("file_chunk")) {
                    String encoded = value.getString("data");
                    if (encoded.length() > 44000) throw new IOException("Save chunk exceeds bound");
                    int sequence = value.getInt("sequence"); stage.append(sequence, Base64.decode(encoded, Base64.NO_WRAP));
                    event(selected, "chunk", "ok", sequence, null);
                } else if (command.equals("file_ready")) {
                    stage.finish(); choosing = true; event(selected, "ready", "ok", -1, null);
                    main.post(() -> { if (owned == epoch.get() && choosing && !closed) choose.accept(name, mime); });
                } else throw new IOException("Unknown save operation");
            } catch (Exception error) {
                clear(); event(selected, "result", "error", -1, "Could not prepare this save. The original remains available.");
            }
        });
    }
    // URI is supplied only by the Activity result, never by page JSON.
    void result(Uri destination) {
        String selected = id; long owned = epoch.get();
        submit(selected, () -> {
            if (selected == null || !selected.equals(id) || !choosing || owned != epoch.get()) return;
            choosing = false;
            try {
                check(owned);
                if (destination == null) { clear(); event(selected, "result", "cancelled", -1, null); return; }
                if (!"content".equals(destination.getScheme())) throw new IOException("Invalid save destination");
                MessageDigest hash = MessageDigest.getInstance("SHA-256"); long count = 0;
                try (InputStream input = stage.open(); OutputStream output = context.getContentResolver().openOutputStream(destination, "w")) {
                    if (output == null) throw new IOException("Save destination unavailable");
                    byte[] buffer = new byte[32768]; int length;
                    while ((length = input.read(buffer)) != -1) { check(owned); output.write(buffer, 0, length); hash.update(buffer, 0, length); count += length; }
                    output.flush(); check(owned);
                    if (count != stage.size() || !VerifiedFileStage.hex(hash.digest()).equals(stage.hash())) throw new IOException("Saved bytes changed");
                }
                clear(); event(selected, "result", "saved", -1, null);
            } catch (Exception error) {
                // CREATE_DOCUMENT creates a new document; never remove an existing
                // arbitrary page-supplied URI or retry an uncertain provider write.
                if (destination != null && DocumentsContract.isDocumentUri(context, destination))
                    try { DocumentsContract.deleteDocument(context.getContentResolver(), destination); } catch (Exception ignored) { }
                clear(); event(selected, "result", "error", -1, "Save failed or was interrupted. The chosen destination may be incomplete; the original remains available.");
            }
        });
    }
    void cancel(String message) {
        String selected = id; epoch.incrementAndGet(); worker.getQueue().clear();
        if (selected != null) event(selected, "result", "error", -1, message);
        submit(selected, this::clear);
    }
    @Override public void close() {
        cancel("Save closed; the original remains available"); closed = true; worker.shutdown();
    }
}
