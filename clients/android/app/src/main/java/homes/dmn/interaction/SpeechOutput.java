package homes.dmn.interaction;

import android.content.Context;
import android.media.AudioAttributes;
import android.media.AudioFocusRequest;
import android.media.AudioManager;
import android.media.MediaDataSource;
import android.media.MediaPlayer;
import android.os.Handler;
import org.json.JSONObject;
import java.io.ByteArrayOutputStream;
import java.security.MessageDigest;
import java.util.function.Consumer;

/** One foreground speech owner. No network, credentials, synthesis or disk audio. */
final class SpeechOutput {
    private static final int MAX_BYTES = 8 * 1024 * 1024;
    private final AudioManager manager;
    private final Handler handler;
    private final Consumer<JSONObject> emit;
    private final AudioAttributes attributes = new AudioAttributes.Builder()
        .setUsage(AudioAttributes.USAGE_ASSISTANCE_NAVIGATION_GUIDANCE)
        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build();
    private AudioFocusRequest focus;
    private MediaPlayer player;
    private ByteArrayOutputStream pending;
    private String id, hash;
    private int expected, sequence;
    private boolean prepared, seeking, playWhenSeekDone;
    private float volume = 1;

    SpeechOutput(Context context, Handler handler, Consumer<JSONObject> emit) {
        manager = (AudioManager) context.getSystemService(Context.AUDIO_SERVICE);
        this.handler = handler; this.emit = emit;
    }
    private void event(String type) {
        if (id == null) return;
        JSONObject value = new JSONObject();
        try {
            value.put("id", id).put("type", type);
            if (prepared && player != null) value.put("position", player.getCurrentPosition() / 1000.0)
                .put("duration", player.getDuration() / 1000.0);
        } catch (Exception ignored) { }
        emit.accept(value);
    }
    private void releaseFocus() {
        if (focus != null) { manager.abandonAudioFocusRequest(focus); focus = null; }
    }
    private void pause(String reason) {
        playWhenSeekDone = false;
        if (prepared && player != null && player.isPlaying()) player.pause();
        releaseFocus(); event(reason);
    }
    private void play() {
        if (!prepared || player == null) throw new IllegalStateException("Audio not prepared");
        if (seeking) { playWhenSeekDone = true; return; }
        if (focus == null) {
            focus = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK)
                .setAudioAttributes(attributes).setAcceptsDelayedFocusGain(false)
                .setOnAudioFocusChangeListener(change -> {
                    if (change == AudioManager.AUDIOFOCUS_LOSS || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT
                        || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK) pause("focuslost");
                }, handler).build();
            if (manager.requestAudioFocus(focus) != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
                releaseFocus(); throw new IllegalStateException("Audio focus unavailable");
            }
        }
        player.setVolume(volume, volume); player.start(); event("playing"); handler.post(tick);
    }
    private final Runnable tick = new Runnable() {
        public void run() {
            if (player != null && prepared && player.isPlaying()) { event("timeupdate"); handler.postDelayed(this, 250); }
        }
    };
    void stop() {
        handler.removeCallbacks(tick); releaseFocus(); prepared = seeking = playWhenSeekDone = false;
        if (player != null) { player.release(); player = null; }
        if (pending != null) { pending.reset(); pending = null; }
        id = hash = null;
    }
    void background() { pause("focuslost"); stop(); }
    void command(JSONObject value) {
        try {
            String command = value.getString("command"), requested = value.getString("id");
            if (!requested.matches("[a-zA-Z0-9_-]{1,80}")) throw new IllegalArgumentException();
            if (command.equals("begin")) {
                stop(); id = requested; expected = value.getInt("bytes"); hash = value.getString("sha256"); sequence = 0;
                if (expected < 1 || expected > MAX_BYTES || !hash.matches("[a-f0-9]{64}")) throw new IllegalArgumentException();
                pending = new ByteArrayOutputStream(Math.min(expected, 65536));
                handler.postDelayed(() -> { if (requested.equals(id) && pending != null) { event("error"); stop(); } }, 30000);
                return;
            }
            if (!requested.equals(id)) return; // Late commands cannot affect another source.
            switch (command) {
                case "chunk": {
                    String encoded = value.getString("data");
                    if (pending == null || encoded.length() > 44000 || value.getInt("sequence") != sequence++) throw new IllegalArgumentException();
                    byte[] bytes = android.util.Base64.decode(encoded, android.util.Base64.NO_WRAP);
                    if (bytes.length < 1 || bytes.length > 32768 || pending.size() + bytes.length > expected) throw new IllegalArgumentException();
                    pending.write(bytes); break;
                }
                case "ready": {
                    if (pending == null || pending.size() != expected) throw new IllegalArgumentException();
                    byte[] bytes = pending.toByteArray(); pending.reset(); pending = null;
                    StringBuilder digest = new StringBuilder();
                    for (byte b : MessageDigest.getInstance("SHA-256").digest(bytes)) digest.append(String.format("%02x", b & 255));
                    if (!hash.contentEquals(digest)) throw new IllegalArgumentException("Audio integrity mismatch");
                    MediaPlayer owned = new MediaPlayer(); player = owned;
                    owned.setAudioAttributes(attributes);
                    owned.setDataSource(new MediaDataSource() {
                        private byte[] audio = bytes;
                        public int readAt(long position, byte[] target, int offset, int length) {
                            if (audio == null || position >= audio.length) return -1;
                            if (position < 0) return -1;
                            int count = Math.min(length, audio.length - (int)position);
                            System.arraycopy(audio, (int)position, target, offset, count); return count;
                        }
                        public long getSize() { return audio == null ? 0 : audio.length; }
                        public void close() { audio = null; }
                    });
                    owned.setOnPreparedListener(p -> {
                        if (p != player) return;
                        prepared = true; event("loaded");
                    });
                    owned.setOnSeekCompleteListener(p -> {
                        if (p != player) return;
                        seeking = false;
                        if (playWhenSeekDone) {
                            playWhenSeekDone = false;
                            try { play(); } catch (Exception error) { event("error"); stop(); }
                        }
                    });
                    owned.setOnCompletionListener(p -> { if (p == player) { releaseFocus(); handler.removeCallbacks(tick); event("ended"); } });
                    owned.setOnErrorListener((p, what, extra) -> { if (p == player) { event("error"); stop(); } return true; });
                    owned.prepareAsync(); break;
                }
                case "play": play(); break;
                case "pause": pause("pause"); break;
                case "seek": if (prepared) { seeking = true; player.seekTo(Math.max(0, Math.min(player.getDuration(), value.getInt("millis")))); } break;
                case "volume": volume = (float)Math.max(0, Math.min(1, value.getDouble("value"))); if (player != null && prepared) player.setVolume(volume, volume); break;
                case "stop": stop(); break;
                default: throw new IllegalArgumentException();
            }
        } catch (Exception error) { event("error"); stop(); }
    }
}
