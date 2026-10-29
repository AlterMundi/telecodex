package homes.dmn.interaction;

import androidx.test.core.app.ActivityScenario;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import static org.junit.Assert.*;
import android.os.Handler;
import android.os.Looper;
import android.util.Base64;
import org.json.JSONObject;
import java.io.ByteArrayOutputStream;
import java.util.Arrays;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

/** Actual Android decoder, synthetic local bytes, no mic/network/provider input. */
public class ProgressiveSpeechTest {
    @Test public void decoderStartsBeforeTerminalAndStopWakesIncompleteRead() throws Exception {
        byte[] data;
        try (java.io.InputStream input = InstrumentationRegistry.getInstrumentation().getContext().getAssets().open("speech-prefix.mp3")) {
            ByteArrayOutputStream out = new ByteArrayOutputStream(); byte[] chunk = new byte[8192]; int count;
            while ((count = input.read(chunk)) != -1) out.write(chunk, 0, count); data = out.toByteArray();
        }
        assertTrue(data.length > 65536);
        CountDownLatch loaded = new CountDownLatch(1), played = new CountDownLatch(1), progressed = new CountDownLatch(1), ended = new CountDownLatch(1);
        AtomicBoolean terminal = new AtomicBoolean(), earlyEnd = new AtomicBoolean();
        AtomicReference<String> error = new AtomicReference<>(); AtomicReference<SpeechOutput> output = new AtomicReference<>();
        try (ActivityScenario<MainActivity> scenario = ActivityScenario.launch(MainActivity.class)) {
            scenario.onActivity(activity -> output.set(new SpeechOutput(activity, new Handler(Looper.getMainLooper()), value -> {
                String type = value.optString("type");
                if (type.equals("loaded")) loaded.countDown();
                if (type.equals("playing")) played.countDown();
                if (type.equals("timeupdate") && value.optDouble("position") > 0) progressed.countDown();
                if (type.equals("error")) error.set(value.toString());
                if (type.equals("ended")) { if (!terminal.get()) earlyEnd.set(true); ended.countDown(); }
            })));
            try {
                command(scenario, output, "stream_begin", new JSONObject());
                send(scenario, output, 0, Arrays.copyOfRange(data, 0, 32768));
                send(scenario, output, 1, Arrays.copyOfRange(data, 32768, 65536));
                assertTrue("Decoder readiness before EOF; error=" + error.get(), loaded.await(10, TimeUnit.SECONDS));
                command(scenario, output, "play", new JSONObject());
                assertTrue("Native playback; error=" + error.get(), played.await(5, TimeUnit.SECONDS));
                assertTrue("Native playback clock; error=" + error.get(), progressed.await(5, TimeUnit.SECONDS));
                assertFalse(terminal.get()); assertFalse(earlyEnd.get());
                int sequence = 2;
                for (int start = 65536; start < data.length; start += 32768) send(scenario, output, sequence++, Arrays.copyOfRange(data, start, Math.min(data.length, start + 32768)));
                terminal.set(true); command(scenario, output, "stream_end", new JSONObject().put("bytes", data.length).put("sha256", VerifiedAudioBuffer.hash(data)));
                assertTrue("Verified completion; error=" + error.get(), ended.await(20, TimeUnit.SECONDS));
                assertNull(error.get()); assertFalse(earlyEnd.get());

                // Re-open an incomplete decoder; release must wake its waiting I/O.
                command(scenario, output, "stream_begin", new JSONObject());
                send(scenario, output, 0, Arrays.copyOfRange(data, 0, 32768));
                long start = System.nanoTime(); scenario.onActivity(activity -> output.get().stop());
                assertTrue("Stopping a partial source must not block the UI", TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - start) < 2000);
            } finally { scenario.onActivity(activity -> output.get().stop()); }
        }
    }
    private void send(ActivityScenario<MainActivity> scenario, AtomicReference<SpeechOutput> output, int sequence, byte[] data) throws Exception {
        command(scenario, output, "stream_chunk", new JSONObject().put("sequence", sequence).put("data", Base64.encodeToString(data, Base64.NO_WRAP)).put("sha256", VerifiedAudioBuffer.hash(data)));
    }
    private void command(ActivityScenario<MainActivity> scenario, AtomicReference<SpeechOutput> output, String type, JSONObject value) throws Exception {
        value.put("command", type).put("id", "synthetic_progressive_source");
        scenario.onActivity(activity -> output.get().command(value));
    }
}
