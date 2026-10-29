package homes.dmn.interaction;

import java.io.Closeable;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.security.MessageDigest;

/** One transient verified file, never an archive or a source-of-authority store. */
final class VerifiedFileStage implements Closeable {
    static final long MAX_BYTES = 256L * 1024 * 1024;
    private final File file;
    private final long expected;
    private final String hash;
    private final MessageDigest digest;
    private FileOutputStream output;
    private long bytes;
    private int sequence;
    private boolean ready, closed;

    VerifiedFileStage(File directory, long expected, String hash) throws IOException {
        if (expected < 0 || expected > MAX_BYTES || !hash.matches("[a-f0-9]{64}")) throw new IOException("File exceeds save bounds");
        if (!directory.isDirectory() && !directory.mkdirs()) throw new IOException("Save staging unavailable");
        if (directory.getUsableSpace() < expected + 32L * 1024 * 1024) throw new IOException("Not enough space to prepare this save");
        this.expected = expected; this.hash = hash;
        try { digest = MessageDigest.getInstance("SHA-256"); }
        catch (Exception error) { throw new IOException(error); }
        file = File.createTempFile("verified-", ".tmp", directory);
        try { output = new FileOutputStream(file); }
        catch (IOException error) { file.delete(); throw error; }
    }
    void append(int next, byte[] value) throws IOException {
        if (closed || ready || next != sequence || value.length < 1 || value.length > 32768 || value.length > expected - bytes)
            throw new IOException("File transfer changed or was interrupted");
        output.write(value); digest.update(value); bytes += value.length; sequence++;
    }
    void finish() throws IOException {
        if (closed || ready || bytes != expected || !hex(digest.digest()).equals(hash)) throw new IOException("File verification failed; nothing was saved");
        output.flush(); output.getFD().sync(); output.close(); output = null; ready = true;
    }
    InputStream open() throws IOException {
        if (!ready || closed) throw new IOException("File is not verified");
        return new FileInputStream(file);
    }
    long size() { return expected; }
    String hash() { return hash; }
    static String hex(byte[] value) {
        StringBuilder result = new StringBuilder(value.length * 2);
        for (byte b : value) result.append(String.format("%02x", b & 255));
        return result.toString();
    }
    @Override public void close() throws IOException {
        if (closed) return; closed = true;
        try { if (output != null) output.close(); }
        finally { output = null; if (file.exists() && !file.delete()) throw new IOException("Save staging cleanup failed"); }
    }
}
