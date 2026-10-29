package homes.dmn.interaction;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.Context;
import android.content.BroadcastReceiver;
import android.content.IntentFilter;
import android.media.AudioManager;
import android.content.pm.ServiceInfo;
import android.media.MediaMetadata;
import android.media.session.MediaSession;
import android.media.session.PlaybackState;
import android.os.Binder;
import android.os.Build;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.PowerManager;
import org.json.JSONObject;
import java.util.function.Consumer;

/** One explicit, finite listening session. No network, synthesis or microphone. */
public final class SpeechService extends Service {
    private static final String CHANNEL = "conversation-speech";
    private static final int NOTIFICATION = 31;
    private static final long SESSION_MILLIS = 2 * 60 * 60 * 1000L;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final LocalBinder binder = new LocalBinder();
    private SpeechOutput output;
    private MediaSession media;
    private PowerManager.WakeLock awake;
    private Consumer<JSONObject> listener;
    private volatile boolean enabled;
    private boolean playing, paused;
    private String source;
    private long position;
    private final BroadcastReceiver noisy = new BroadcastReceiver() {
        @Override public void onReceive(Context context, Intent intent) { if (enabled) userAction("pause"); }
    };
    private final Runnable expiry = () -> userAction("stop");

    public final class LocalBinder extends Binder { SpeechService service() { return SpeechService.this; } }
    @Override public IBinder onBind(Intent intent) { return binder; }
    @Override public void onCreate() {
        super.onCreate();
        getSystemService(NotificationManager.class).createNotificationChannel(
            new NotificationChannel(CHANNEL, "Conversation audio", NotificationManager.IMPORTANCE_LOW));
        output = new SpeechOutput(this, handler, this::receive);
        media = new MediaSession(this, "Daimon conversation");
        media.setMetadata(new MediaMetadata.Builder().putString(MediaMetadata.METADATA_KEY_TITLE, "Daimon Interaction").build());
        media.setCallback(new MediaSession.Callback() {
            @Override public void onPlay() { userAction("play"); }
            @Override public void onPause() { userAction("pause"); }
            @Override public void onStop() { userAction("stop"); }
        }, handler);
        awake = getSystemService(PowerManager.class).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, getPackageName() + ":conversation-audio");
        awake.setReferenceCounted(false);
        IntentFilter filter = new IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY);
        if (Build.VERSION.SDK_INT >= 33) registerReceiver(noisy, filter, Context.RECEIVER_NOT_EXPORTED);
        else registerReceiver(noisy, filter);
    }
    void attach(Consumer<JSONObject> value) { listener = value; }
    boolean enabled() { return enabled; }
    void session(boolean value) {
        if (value == enabled) return; // Status refreshes cannot extend the finite session.
        if (!value) { shutdown(); return; }
        enabled = true;
        try {
            // Called only by the selected visible conversation, before backgrounding.
            startForegroundService(new Intent(this, SpeechService.class));
            if (Build.VERSION.SDK_INT >= 29) startForeground(NOTIFICATION, notification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK);
            else startForeground(NOTIFICATION, notification());
            media.setActive(true); updateState();
            awake.acquire(SESSION_MILLIS); handler.postDelayed(expiry, SESSION_MILLIS);
        } catch (RuntimeException error) { shutdown(); control("stop"); }
    }
    void command(JSONObject value) {
        String command = value.optString("command");
        if (command.equals("session")) { session(value.optBoolean("enabled")); return; }
        if (command.equals("begin") || command.equals("stream_begin")) {
            source = value.optString("id"); position = 0; playing = paused = false;
        }
        if (!command.equals("begin") && !command.equals("stream_begin") && !value.optString("id").equals(source)) return;
        output.command(value);
        if (command.equals("stop")) { source = null; position = 0; playing = paused = false; updateState(); }
    }
    private void receive(JSONObject value) {
        String type = value.optString("type");
        if (value.has("position")) position = (long)(value.optDouble("position") * 1000);
        boolean changed = !type.equals("timeupdate");
        if (type.equals("playing")) { playing = true; paused = false; }
        if (type.equals("pause") || type.equals("focuslost")) { playing = false; paused = true; }
        if (type.equals("ended") || type.equals("error")) { playing = paused = false; }
        if (changed) updateState();
        if (listener != null) listener.accept(value);
    }
    private void control(String action) {
        JSONObject value = new JSONObject();
        try { value.put("type", "control").put("action", action); } catch (Exception ignored) { }
        if (listener != null) listener.accept(value);
    }
    private void userAction(String action) {
        // Silence immediately; canonical web controls keep queue/heard/source meaning.
        if (action.equals("pause") && source != null) {
            JSONObject value = new JSONObject();
            try { value.put("id", source).put("command", "pause"); output.command(value); } catch (Exception ignored) { }
        }
        if (action.equals("stop")) shutdown();
        control(action);
    }
    private void updateState() {
        if (media == null) return;
        int state = playing ? PlaybackState.STATE_PLAYING : paused ? PlaybackState.STATE_PAUSED : PlaybackState.STATE_STOPPED;
        media.setPlaybackState(new PlaybackState.Builder().setState(state, position, playing ? 1 : 0)
            .setActions(PlaybackState.ACTION_PLAY | PlaybackState.ACTION_PAUSE | PlaybackState.ACTION_STOP).build());
        if (enabled) getSystemService(NotificationManager.class).notify(NOTIFICATION, notification());
    }
    private Notification notification() {
        PendingIntent open = PendingIntent.getActivity(this, 0, new Intent(this, MainActivity.class)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP | Intent.FLAG_ACTIVITY_CLEAR_TOP), PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
        String action = playing ? "pause" : "play";
        PendingIntent toggle = PendingIntent.getService(this, 1, new Intent(this, SpeechService.class).setAction(action), PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
        PendingIntent stop = PendingIntent.getService(this, 2, new Intent(this, SpeechService.class).setAction("stop"), PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
        return new Notification.Builder(this, CHANNEL).setSmallIcon(android.R.drawable.ic_lock_silent_mode_off)
            .setContentTitle("Daimon Interaction").setContentText(playing ? "Speaking" : paused ? "Audio paused" : "Waiting for replies · 2-hour listening session")
            .setContentIntent(open).setVisibility(Notification.VISIBILITY_PRIVATE).setOnlyAlertOnce(true).setOngoing(true)
            .addAction(new Notification.Action.Builder(playing ? android.R.drawable.ic_media_pause : android.R.drawable.ic_media_play, playing ? "Pause" : "Play", toggle).build())
            .addAction(new Notification.Action.Builder(android.R.drawable.ic_lock_silent_mode, "Audio off", stop).build())
            .setStyle(new Notification.MediaStyle().setMediaSession(media.getSessionToken()).setShowActionsInCompactView(0, 1)).build();
    }
    void shutdown() {
        enabled = playing = paused = false; source = null; position = 0;
        handler.removeCallbacks(expiry);
        if (output != null) output.stop();
        if (awake != null && awake.isHeld()) awake.release();
        if (media != null) { updateState(); media.setActive(false); }
        stopForeground(STOP_FOREGROUND_REMOVE); stopSelf();
    }
    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && enabled && intent.getAction() != null) userAction(intent.getAction());
        return START_NOT_STICKY;
    }
    @Override public void onTaskRemoved(Intent intent) { shutdown(); control("stop"); }
    @Override public void onDestroy() {
        shutdown(); listener = null; unregisterReceiver(noisy);
        if (media != null) { media.release(); media = null; }
        super.onDestroy();
    }
}
