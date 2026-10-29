package homes.dmn.interaction;

import org.junit.Test;
import org.junit.Rule;
import org.junit.rules.TemporaryFolder;
import static org.junit.Assert.*;
import java.io.File;
import java.io.IOException;
import java.security.MessageDigest;
import java.util.Arrays;

public class VerifiedFileStageTest {
    @Rule public TemporaryFolder temporary = new TemporaryFolder();
    private String hash(byte[] value) throws Exception { return VerifiedFileStage.hex(MessageDigest.getInstance("SHA-256").digest(value)); }
    @Test public void completeBytesVerifiedBeforeReadingAndRemovedOnClose() throws Exception {
        byte[] value = new byte[65537]; for (int n = 0; n < value.length; n++) value[n] = (byte)n;
        File directory = temporary.newFolder();
        try (VerifiedFileStage stage = new VerifiedFileStage(directory, value.length, hash(value))) {
            assertThrows(IOException.class, stage::open);
            stage.append(0, Arrays.copyOfRange(value, 0, 32768));
            stage.append(1, Arrays.copyOfRange(value, 32768, 65536));
            stage.append(2, Arrays.copyOfRange(value, 65536, value.length)); stage.finish();
            try (java.io.InputStream input = stage.open()) { assertArrayEquals(value, input.readAllBytes()); }
            assertEquals(1, directory.list().length);
        }
        assertEquals(0, directory.list().length);
    }
    @Test public void mismatchOrderingAndSizeNeverBecomeReadable() throws Exception {
        byte[] value = {1, 2}; File directory = temporary.newFolder();
        try (VerifiedFileStage stage = new VerifiedFileStage(directory, 2, hash(value))) {
            assertThrows(IOException.class, () -> stage.append(1, value));
            assertThrows(IOException.class, () -> stage.append(0, new byte[32769]));
            assertThrows(IOException.class, () -> stage.append(0, new byte[3]));
            stage.append(0, new byte[]{2, 1}); assertThrows(IOException.class, stage::finish);
            assertThrows(IOException.class, stage::open);
        }
        assertEquals(0, directory.list().length);
        assertThrows(IOException.class, () -> new VerifiedFileStage(directory, VerifiedFileStage.MAX_BYTES + 1, hash(value)));
        assertEquals(0, directory.list().length);
    }
    @Test public void emptyFileAndIncompleteCancellation() throws Exception {
        File directory = temporary.newFolder();
        try (VerifiedFileStage stage = new VerifiedFileStage(directory, 0, hash(new byte[0]))) {
            stage.finish(); try (java.io.InputStream input = stage.open()) { assertEquals(-1, input.read()); }
        }
        try (VerifiedFileStage stage = new VerifiedFileStage(directory, 2, hash(new byte[]{1, 2}))) {
            stage.append(0, new byte[]{1}); assertThrows(IOException.class, stage::finish);
        }
        assertEquals(0, directory.list().length);
    }
}
