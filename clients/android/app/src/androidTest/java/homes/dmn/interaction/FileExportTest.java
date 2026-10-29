package homes.dmn.interaction;

import android.content.Context;
import android.net.Uri;
import android.util.Base64;
import androidx.test.platform.app.InstrumentationRegistry;
import org.json.JSONObject;
import org.junit.Test;
import static org.junit.Assert.*;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;

/** Real Android ContentResolver I/O; picker choice is synthetic, not UI acceptance. */
public class FileExportTest {
    @Test public void verifiedCopyCancellationAndCorruptTransfer() throws Exception {
        Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();
        Uri destination = Uri.parse("content://homes.dmn.interaction.pilot.test.saved/copy");
        LinkedBlockingQueue<JSONObject> events = new LinkedBlockingQueue<>(); LinkedBlockingQueue<String> choices = new LinkedBlockingQueue<>();
        byte[] bytes = new byte[32769]; for (int n = 0; n < bytes.length; n++) bytes[n] = (byte)n;
        File directory = new File(context.getCacheDir(), "verified-file-export");
        try (FileExport output = new FileExport(context, events::add, (name, mime) -> choices.add(name))) {
            begin(output, "export_copy", bytes, VerifiedAudioBuffer.hash(bytes)); expect(events, "begin", "ok");
            chunk(output, "export_copy", 0, java.util.Arrays.copyOfRange(bytes, 0, 32768)); expect(events, "chunk", "ok");
            chunk(output, "export_copy", 1, java.util.Arrays.copyOfRange(bytes, 32768, bytes.length)); expect(events, "chunk", "ok");
            command(output, "export_copy", "file_ready", new JSONObject()); expect(events, "ready", "ok");
            assertEquals("synthetic.bin", choices.poll(5, TimeUnit.SECONDS));
            output.result(destination); expect(events, "result", "saved");
            try (java.io.InputStream input = context.getContentResolver().openInputStream(destination)) {
                ByteArrayOutputStream data = new ByteArrayOutputStream(); byte[] part = new byte[8192]; int length;
                while ((length = input.read(part)) != -1) data.write(part, 0, length); assertArrayEquals(bytes, data.toByteArray());
            }
            assertEquals(0, directory.list().length);

            begin(output, "export_cancel", new byte[0], VerifiedAudioBuffer.hash(new byte[0])); expect(events, "begin", "ok");
            command(output, "export_copy", "file_cancel", new JSONObject()); // stale ID cannot cancel current save.
            command(output, "export_cancel", "file_ready", new JSONObject()); expect(events, "ready", "ok");
            assertEquals("synthetic.bin", choices.poll(5, TimeUnit.SECONDS));
            output.result(null); expect(events, "result", "cancelled"); assertEquals(0, directory.list().length);

            begin(output, "export_corrupt", new byte[]{1}, "0".repeat(64)); expect(events, "begin", "ok");
            chunk(output, "export_corrupt", 0, new byte[]{1}); expect(events, "chunk", "ok");
            command(output, "export_corrupt", "file_ready", new JSONObject()); expect(events, "result", "error");
            assertTrue(choices.isEmpty()); assertEquals(0, directory.list().length);
        } finally { context.getContentResolver().delete(destination, null, null); }
    }
    private void expect(LinkedBlockingQueue<JSONObject> events, String stage, String status) throws Exception {
        JSONObject event = events.poll(10, TimeUnit.SECONDS); assertNotNull("Native file receipt", event);
        assertEquals(event.toString(), stage, event.optString("stage")); assertEquals(event.toString(), status, event.optString("status"));
    }
    private void begin(FileExport output, String id, byte[] bytes, String hash) throws Exception {
        command(output, id, "file_begin", new JSONObject().put("name", "synthetic.bin").put("mime", "application/octet-stream").put("bytes", bytes.length).put("sha256", hash));
    }
    private void chunk(FileExport output, String id, int sequence, byte[] bytes) throws Exception {
        command(output, id, "file_chunk", new JSONObject().put("sequence", sequence).put("data", Base64.encodeToString(bytes, Base64.NO_WRAP)));
    }
    private void command(FileExport output, String id, String command, JSONObject value) throws Exception { output.command(value.put("id", id).put("command", command)); }
}
