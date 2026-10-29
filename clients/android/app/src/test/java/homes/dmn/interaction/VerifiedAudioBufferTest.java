package homes.dmn.interaction;

import org.junit.Test;
import static org.junit.Assert.*;
import java.io.IOException;
import java.util.concurrent.*;

public class VerifiedAudioBufferTest {
    @Test public void readsVerifiedPrefixBeforeTerminalAndWaitsForLaterBytes() throws Exception {
        VerifiedAudioBuffer buffer = new VerifiedAudioBuffer(32, 1000, 5000);
        byte[] first = {1,2,3}, second = {4,5}, read = new byte[5];
        buffer.append(0, first, VerifiedAudioBuffer.hash(first));
        assertEquals(-1, buffer.size()); assertEquals(3, buffer.readAt(0, read, 0, 5));
        ExecutorService executor = Executors.newSingleThreadExecutor();
        try {
            Future<Integer> pending = executor.submit(() -> buffer.readAt(3, read, 3, 2));
            try { pending.get(30, TimeUnit.MILLISECONDS); fail("Missing data must not be EOF"); }
            catch (TimeoutException expected) { }
            buffer.append(1, second, VerifiedAudioBuffer.hash(second)); assertEquals(2, (int)pending.get(1, TimeUnit.SECONDS));
            assertArrayEquals(new byte[]{1,2,3,4,5}, read);
            buffer.finish(5, VerifiedAudioBuffer.hash(read)); assertEquals(5, buffer.size());
            assertEquals(-1, buffer.readAt(5, read, 0, 1));
            byte[] cross = new byte[3]; assertEquals(3, buffer.readAt(2, cross, 0, 3));
            assertArrayEquals(new byte[]{3,4,5}, cross);
        } finally { buffer.close(); executor.shutdownNow(); }
    }
    @Test public void rejectsIntegritySequenceAndSizeWithoutSuccessfulEof() throws Exception {
        for (int fault = 0; fault < 3; fault++) {
            VerifiedAudioBuffer buffer = new VerifiedAudioBuffer(2, 10, 100);
            final int selected = fault;
            assertThrows(IOException.class, () -> buffer.append(selected == 0 ? 1 : 0,
                selected == 1 ? new byte[]{1,2,3} : new byte[]{1}, selected == 2 ? "0".repeat(64) : VerifiedAudioBuffer.hash(selected == 1 ? new byte[]{1,2,3} : new byte[]{1})));
            assertFalse(buffer.complete()); assertThrows(IOException.class, () -> buffer.readAt(0, new byte[1], 0, 1));
        }
        VerifiedAudioBuffer buffer = new VerifiedAudioBuffer(4, 10, 100);
        buffer.append(0, new byte[]{1}, VerifiedAudioBuffer.hash(new byte[]{1}));
        assertThrows(IOException.class, () -> buffer.finish(1, "0".repeat(64))); assertFalse(buffer.complete());
    }
    @Test public void closeWakesBlockedReaderBeforeDecoderRelease() throws Exception {
        VerifiedAudioBuffer buffer = new VerifiedAudioBuffer(32, 30000, 60000);
        ExecutorService executor = Executors.newSingleThreadExecutor();
        try {
            Future<Integer> pending = executor.submit(() -> buffer.readAt(0, new byte[1], 0, 1));
            buffer.close();
            ExecutionException error = assertThrows(ExecutionException.class, () -> pending.get(1, TimeUnit.SECONDS));
            assertTrue(error.getCause() instanceof IOException);
        } finally { buffer.close(); executor.shutdownNow(); }
    }
    @Test public void stalledStreamHasFiniteDeadlineAndInputIsOwned() throws Exception {
        VerifiedAudioBuffer buffer = new VerifiedAudioBuffer(32, 10, 100);
        byte[] input = {7}, result = new byte[1];buffer.append(0, input, VerifiedAudioBuffer.hash(input));input[0] = 99;
        assertEquals(1, buffer.readAt(0, result, 0, 1)); assertEquals(7, result[0]);
        assertThrows(IOException.class, () -> buffer.readAt(1, result, 0, 1)); assertFalse(buffer.complete());
    }
}
