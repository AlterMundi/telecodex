package homes.dmn.interaction;

import java.io.Closeable;
import java.io.IOException;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Arrays;

/** Bounded random-access encoded stream. Missing data is not EOF. */
final class VerifiedAudioBuffer implements Closeable {
    private final ArrayList<byte[]> chunks = new ArrayList<>();
    private final MessageDigest digest;
    private final int maximum;
    private final long waitMillis, expires;
    private int bytes, sequence;
    private boolean complete, closed;

    VerifiedAudioBuffer() { this(8 * 1024 * 1024, 30000, 145000); }
    VerifiedAudioBuffer(int maximum, long waitMillis, long lifetimeMillis) {
        this.maximum = maximum; this.waitMillis = waitMillis;
        expires = System.nanoTime() + lifetimeMillis * 1000000;
        try { digest = MessageDigest.getInstance("SHA-256"); }
        catch (NoSuchAlgorithmException error) { throw new IllegalStateException(error); }
    }
    static String hash(byte[] value) {
        try { return hex(MessageDigest.getInstance("SHA-256").digest(value)); }
        catch (NoSuchAlgorithmException error) { throw new IllegalStateException(error); }
    }
    private static String hex(byte[] value) {
        StringBuilder result = new StringBuilder(value.length * 2);
        for (byte b : value) result.append(String.format("%02x", b & 255));
        return result.toString();
    }
    private IOException invalid(String message) { close(); return new IOException(message); }
    synchronized void append(int next, byte[] value, String sha256) throws IOException {
        if (closed || complete || next != sequence || value.length < 1 || value.length > 32768
            || value.length > maximum - bytes || !hash(value).equals(sha256)) throw invalid("Invalid audio chunk");
        byte[] owned = Arrays.copyOf(value, value.length);
        chunks.add(owned); digest.update(owned); bytes += owned.length; sequence++; notifyAll();
    }
    synchronized void finish(int expected, String sha256) throws IOException {
        if (closed || complete || bytes < 1 || expected != bytes || !hex(digest.digest()).equals(sha256))
            throw invalid("Incomplete audio stream");
        complete = true; notifyAll();
    }
    synchronized long size() { return complete && !closed ? bytes : -1; }
    synchronized boolean complete() { return complete && !closed; }
    synchronized int readAt(long position, byte[] target, int offset, int length) throws IOException {
        if (position < 0 || offset < 0 || length < 0 || offset > target.length - length) throw invalid("Invalid audio read");
        if (length == 0) return 0;
        long deadline = Math.min(expires, System.nanoTime() + waitMillis * 1000000);
        while (!closed && !complete && position >= bytes) {
            long remaining = deadline - System.nanoTime();
            if (remaining <= 0) throw invalid("Audio data deadline exceeded");
            try { wait(Math.max(1, remaining / 1000000)); }
            catch (InterruptedException error) { Thread.currentThread().interrupt(); throw invalid("Audio read interrupted"); }
        }
        if (closed) throw new IOException("Audio source closed");
        if (position >= bytes) return -1; // Only a verified terminal stream exposes EOF.
        int count = Math.min(length, bytes - (int)position), copied = 0, start = 0;
        for (byte[] chunk : chunks) {
            if (position + copied < start + chunk.length) {
                int within = (int)(position + copied) - start;
                int take = Math.min(count - copied, chunk.length - within);
                System.arraycopy(chunk, within, target, offset + copied, take); copied += take;
                if (copied == count) break;
            }
            start += chunk.length;
        }
        return copied;
    }
    @Override public synchronized void close() { closed = true; chunks.clear(); notifyAll(); }
}
