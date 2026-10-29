package homes.dmn.interaction;

import android.app.Notification;
import android.app.NotificationManager;
import android.media.session.MediaController;
import android.media.session.MediaSession;
import android.media.session.PlaybackState;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.os.IBinder;
import android.util.Base64;
import androidx.lifecycle.Lifecycle;
import androidx.test.core.app.ActivityScenario;
import androidx.test.platform.app.InstrumentationRegistry;
import org.json.JSONObject;
import org.junit.Test;
import static org.junit.Assert.*;
import java.io.ByteArrayOutputStream;
import java.util.Arrays;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

/** Real foreground service/decoder, synthetic bytes. No WebView/provider/hearing claim. */
public class BackgroundSpeechTest {
    @Test public void laterReplyAndNotificationControlsWhileActivityStopped() throws Exception {
        Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();
        AtomicReference<SpeechService> service = new AtomicReference<>();
        CountDownLatch bound = new CountDownLatch(1);
        ServiceConnection connection = new ServiceConnection() {
            public void onServiceConnected(ComponentName name, IBinder binder) { service.set(((SpeechService.LocalBinder)binder).service()); bound.countDown(); }
            public void onServiceDisconnected(ComponentName name) { }
        };
        byte[] data;
        try (java.io.InputStream input = InstrumentationRegistry.getInstrumentation().getContext().getAssets().open("speech-prefix.mp3")) {
            ByteArrayOutputStream out = new ByteArrayOutputStream(); byte[] part = new byte[8192]; int count;
            while ((count = input.read(part)) != -1) out.write(part, 0, count); data = out.toByteArray();
        }
        try (ActivityScenario<MainActivity> activity = ActivityScenario.launch(MainActivity.class)) {
            assertTrue(context.bindService(new Intent(context, SpeechService.class), connection, Context.BIND_AUTO_CREATE));
            assertTrue(bound.await(5, TimeUnit.SECONDS));
            try {
                activity.onActivity(value -> service.get().session(true));
                activity.moveToState(Lifecycle.State.CREATED);
                assertTrue(service.get().enabled());
                NotificationManager notifications = context.getSystemService(NotificationManager.class);
                Notification initial = awaitNotification(notifications, "Play");
                MediaController media = new MediaController(context, (MediaSession.Token)initial.extras.getParcelable(Notification.EXTRA_MEDIA_SESSION));
                // Finish one complete segment, then prepare a distinct later reply
                // with the actual Activity stopped and focus reacquired natively.
                for (int reply = 0; reply < 2; reply++) {
                    String id = "background_synthetic_" + reply;
                    CountDownLatch ready = new CountDownLatch(1), played = new CountDownLatch(1), ended = new CountDownLatch(1), pause = new CountDownLatch(1);
                    AtomicReference<String> error = new AtomicReference<>(); StringBuffer phases = new StringBuffer();
                    InstrumentationRegistry.getInstrumentation().runOnMainSync(() -> service.get().attach(value -> {
                        if (!value.optString("type").equals("timeupdate")) phases.append(value.toString()).append("\n");
                        switch (value.optString("type")) {
                            case "loaded": ready.countDown(); break;
                            case "playing": played.countDown(); break;
                            case "ended": ended.countDown(); break;
                            case "error": error.set(value.toString()); break;
                            case "control": if (value.optString("action").equals("pause")) pause.countDown(); break;
                        }
                    }));
                    command(service, id, "begin", new JSONObject().put("bytes", data.length).put("sha256", VerifiedAudioBuffer.hash(data)));
                    int sequence = 0;
                    for (int start = 0; start < data.length; start += 32768) command(service, id, "chunk", new JSONObject()
                        .put("sequence", sequence++).put("data", Base64.encodeToString(Arrays.copyOfRange(data, start, Math.min(data.length, start + 32768)), Base64.NO_WRAP)));
                    command(service, id, "ready", new JSONObject());
                    assertTrue("Background decoder readiness: " + error.get(), ready.await(10, TimeUnit.SECONDS));
                    command(service, id, "play", new JSONObject());
                    assertTrue("Background native focus/play: " + error.get(), played.await(5, TimeUnit.SECONDS));
                    if (reply == 1) {
                        Notification speaking = awaitNotification(notifications, "Pause");
                        assertEquals(phases.toString(), PlaybackState.STATE_PLAYING, media.getPlaybackState().getState());
                        command(service, "background_synthetic_0", "stop", new JSONObject());
                        assertEquals("Late old-source stop cannot change native session: " + phases, PlaybackState.STATE_PLAYING, media.getPlaybackState().getState());
                        speaking.actions[0].actionIntent.send();
                        assertTrue("Notification pause returns to canonical web control", pause.await(5, TimeUnit.SECONDS));
                        // Simulate only that canonical controller's accepted resume.
                        command(service, id, "play", new JSONObject());
                    }
                    assertTrue("Background completion: " + error.get(), ended.await(20, TimeUnit.SECONDS));
                    assertNull(error.get()); assertTrue(service.get().enabled());
                }
                notifications.getActiveNotifications()[0].getNotification().actions[1].actionIntent.send();
                long limit = System.nanoTime() + TimeUnit.SECONDS.toNanos(5);
                while ((service.get().enabled() || notifications.getActiveNotifications().length != 0) && System.nanoTime() < limit) Thread.sleep(20);
                assertFalse("Audio off ends session", service.get().enabled());
                assertEquals(0, notifications.getActiveNotifications().length);
            } finally {
                InstrumentationRegistry.getInstrumentation().runOnMainSync(() -> { service.get().attach(null); service.get().shutdown(); });
                context.unbindService(connection);
            }
        }
    }
    private Notification awaitNotification(NotificationManager manager, String title) throws Exception {
        long until = System.nanoTime() + TimeUnit.SECONDS.toNanos(5);
        // notify() queues publication in Android; observe that same notification,
        // do not replay playback or substitute a locally constructed notification.
        while (System.nanoTime() < until) {
            android.service.notification.StatusBarNotification[] active = manager.getActiveNotifications();
            if (active.length == 1 && active[0].getNotification().actions != null
                && title.contentEquals(active[0].getNotification().actions[0].title)) return active[0].getNotification();
            Thread.sleep(20);
        }
        throw new AssertionError("Android did not publish the expected " + title + " notification");
    }
    private void command(AtomicReference<SpeechService> service, String id, String type, JSONObject value) throws Exception {
        value.put("id", id).put("command", type);
        InstrumentationRegistry.getInstrumentation().runOnMainSync(() -> service.get().command(value));
    }
}
