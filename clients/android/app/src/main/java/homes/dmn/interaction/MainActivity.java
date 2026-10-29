package homes.dmn.interaction;

import android.Manifest;
import android.app.AlertDialog;
import android.content.Intent;
import android.content.ComponentName;
import android.content.ServiceConnection;
import android.os.IBinder;
import android.content.pm.PackageManager;
import android.graphics.Color;
import android.net.Uri;
import android.os.Bundle;
import android.webkit.CookieManager;
import android.webkit.PermissionRequest;
import android.webkit.ValueCallback;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.TextView;
import androidx.webkit.JavaScriptReplyProxy;
import androidx.webkit.WebViewCompat;
import androidx.webkit.WebViewFeature;
import androidx.activity.ComponentActivity;
import androidx.activity.OnBackPressedCallback;
import java.nio.charset.StandardCharsets;
import java.util.Collections;
import org.json.JSONObject;

/** Thin independent client; authority, originals and work remain on the selected receiver. */
public final class MainActivity extends ComponentActivity {
    private WebView web;
    private SpeechService speech;
    private FileExport exports;
    private ServiceConnection connection;
    private boolean bound;
    private JavaScriptReplyProxy reply;
    private String origin;
    private boolean foreground;
    private PermissionRequest microphone;
    private ValueCallback<Uri[]> chooser;

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        exports = new FileExport(this, value -> { if (reply != null) try { reply.postMessage(value.toString()); } catch (Exception ignored) { } },
            (name, mime) -> {
                if (!foreground || !appPage()) { exports.cancel("Save interrupted; the original remains available"); return; }
                try { startActivityForResult(new Intent(Intent.ACTION_CREATE_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE)
                    .setType(mime).putExtra(Intent.EXTRA_TITLE, name), 3); }
                catch (RuntimeException error) { exports.cancel("Save destination unavailable; the original remains available"); }
            });
        getOnBackPressedDispatcher().addCallback(this, new OnBackPressedCallback(true) {
            @Override public void handleOnBackPressed() { navigateBack(); }
        });
        String saved = getPreferences(MODE_PRIVATE).getString("home", "");
        if (saved.isEmpty()) setup(); else open(saved);
    }
    private void setup() {
        LinearLayout box = new LinearLayout(this); box.setOrientation(LinearLayout.VERTICAL);
        int padding = (int)(24 * getResources().getDisplayMetrics().density); box.setPadding(padding, padding, padding, padding);
        box.setBackgroundColor(Color.rgb(11,16,24));
        TextView title = new TextView(this); title.setText("Daimon Interaction\nAndroid pilot"); title.setTextColor(Color.rgb(134,210,230)); title.setTextSize(24); box.addView(title);
        TextView description = new TextView(this);
        description.setText("Enter your daimon’s HTTPS home address. Sign in with the existing shared login. Telegram is not required. Enable spoken replies for up to two hours of background listening. Audio off or closing the app ends that listening session.");
        description.setTextColor(Color.WHITE); box.addView(description);
        EditText input = new EditText(this); input.setSingleLine(true); input.setHint("https://your-daimon.example"); input.setInputType(17); box.addView(input);
        Button start = new Button(this); start.setText("Open my daimon"); box.addView(start);
        start.setOnClickListener(v -> {
            try {
                Uri uri = Uri.parse(input.getText().toString().trim());
                if (!"https".equals(uri.getScheme()) || uri.getHost() == null || uri.getUserInfo() != null
                    || uri.getPort() != -1 && uri.getPort() != 443 || uri.getQuery() != null || uri.getFragment() != null
                    || uri.getPath() != null && !uri.getPath().isEmpty() && !uri.getPath().equals("/") && !uri.getPath().equals("/interaction/")) throw new IllegalArgumentException();
                String home = "https://" + uri.getHost();
                getPreferences(MODE_PRIVATE).edit().putString("home", home).apply(); open(home);
            } catch (Exception error) { input.setError("Use an HTTPS home address, without a query or credentials."); }
        });
        setContentView(box);
    }
    private boolean sameOrigin(Uri uri) { return origin != null && origin.equals(uri.getScheme() + "://" + uri.getAuthority()); }
    private boolean appPage() {
        if (web == null || web.getUrl() == null) return false;
        Uri uri = Uri.parse(web.getUrl()); return sameOrigin(uri) && "/interaction/".equals(uri.getPath());
    }
    private void open(String home) {
        if (speech != null) { openWeb(home); return; }
        TextView waiting = new TextView(this); waiting.setText("Opening conversation…"); setContentView(waiting);
        connection = new ServiceConnection() {
            @Override public void onServiceConnected(ComponentName name, IBinder binder) {
                speech = ((SpeechService.LocalBinder) binder).service(); openWeb(home);
            }
            @Override public void onServiceDisconnected(ComponentName name) {
                speech = null;
                if (web != null) web.evaluateJavascript("window.NativeSpeechAudio?.serviceLost()", null);
            }
        };
        bound = bindService(new Intent(this, SpeechService.class), connection, BIND_AUTO_CREATE);
        if (!bound) { waiting.setText("Audio service unavailable. Close and reopen the app."); }
    }
    private void openWeb(String home) {
        origin = home;
        web = new WebView(this); web.setBackgroundColor(Color.rgb(11,16,24));
        web.setFitsSystemWindows(true);
        web.setOnApplyWindowInsetsListener((view, insets) -> {
            view.setPadding(insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(), insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
            return insets;
        });
        WebSettings settings = web.getSettings(); settings.setJavaScriptEnabled(true); settings.setDomStorageEnabled(true);
        settings.setAllowFileAccess(false); settings.setAllowContentAccess(false);
        settings.setMixedContentMode(WebSettings.MIXED_CONTENT_NEVER_ALLOW);
        settings.setMediaPlaybackRequiresUserGesture(true);
        CookieManager.getInstance().setAcceptCookie(true); CookieManager.getInstance().setAcceptThirdPartyCookies(web, false);
        speech.attach(value -> { if (reply != null) try { reply.postMessage(value.toString()); } catch (Exception ignored) { } });
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)
            || !WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            new AlertDialog.Builder(this).setMessage("Update Android System WebView to use this pilot’s native audio.")
                .setPositiveButton("Close", (d,w) -> finish()).show(); return;
        }
        WebViewCompat.addWebMessageListener(web, "DaimonAudio", Collections.singleton(home), (view, message, sourceOrigin, mainFrame, proxy) -> {
            if (!mainFrame || !sameOrigin(sourceOrigin) || !appPage() || speech == null || (!foreground && !speech.enabled())) return;
            String data = message.getData(); if (data == null || data.length() > 45000) return;
            try {
                JSONObject value = new JSONObject(data); reply = proxy;
                if (value.optString("command").startsWith("file_")) {
                    if (foreground) exports.command(value); return;
                }
                if (value.optString("command").equals("session") && value.optBoolean("enabled") && !foreground) return;
                if (!value.optString("command").equals("hello")) speech.command(value);
            } catch (Exception ignored) { }
        });
        try {
            java.io.ByteArrayOutputStream scriptBytes = new java.io.ByteArrayOutputStream();
            try (java.io.InputStream input = getAssets().open("native-files.js")) {
                byte[] buffer = new byte[8192]; int count;
                while ((count = input.read(buffer)) != -1) scriptBytes.write(buffer, 0, count);
            }
            String script = scriptBytes.toString(StandardCharsets.UTF_8.name()) + "\n";
            scriptBytes.reset();
            try (java.io.InputStream input = getAssets().open("native-audio.js")) {
                byte[] buffer = new byte[8192]; int count;
                while ((count = input.read(buffer)) != -1) scriptBytes.write(buffer, 0, count);
            }
            script += scriptBytes.toString(StandardCharsets.UTF_8.name());
            WebViewCompat.addDocumentStartJavaScript(web, script, Collections.singleton(home));
        } catch (Exception error) { throw new IllegalStateException("Missing native audio adapter", error); }
        web.setWebViewClient(new WebViewClient() {
            @Override public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                if (!request.isForMainFrame()) return false;
                Uri uri = request.getUrl();
                if (sameOrigin(uri) || "https://auth.dmn.homes".equals(uri.getScheme() + "://" + uri.getAuthority())) return false;
                if ("https".equals(uri.getScheme()) && request.hasGesture()) startActivity(new Intent(Intent.ACTION_VIEW, uri));
                return true;
            }
            @Override public void onPageStarted(WebView view, String url, android.graphics.Bitmap icon) {
                exports.cancel("Page changed; the original remains available"); speech.shutdown(); reply = null;
                if (microphone != null) { microphone.deny(); microphone = null; }
            }
        });
        web.setWebChromeClient(new WebChromeClient() {
            @Override public void onPermissionRequest(PermissionRequest request) {
                if (!foreground || !appPage() || !sameOrigin(request.getOrigin()) || request.getResources().length != 1
                    || !PermissionRequest.RESOURCE_AUDIO_CAPTURE.equals(request.getResources()[0])) { request.deny(); return; }
                if (microphone != null) microphone.deny(); microphone = request;
                if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) grantMicrophone();
                else requestPermissions(new String[]{Manifest.permission.RECORD_AUDIO}, 1);
            }
            @Override public void onPermissionRequestCanceled(PermissionRequest request) { if (microphone == request) microphone = null; }
            @Override public boolean onShowFileChooser(WebView view, ValueCallback<Uri[]> callback, FileChooserParams params) {
                if (chooser != null) chooser.onReceiveValue(null); chooser = callback;
                Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("*/*");
                intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, params.getMode() == FileChooserParams.MODE_OPEN_MULTIPLE);
                startActivityForResult(intent, 2); return true;
            }
        });
        setContentView(web); web.loadUrl(home + "/interaction/?view=conversation&client=standalone");
    }
    private void grantMicrophone() {
        if (microphone == null) return;
        if (foreground && appPage() && sameOrigin(microphone.getOrigin())) microphone.grant(new String[]{PermissionRequest.RESOURCE_AUDIO_CAPTURE});
        else microphone.deny(); microphone = null;
    }
    @Override public void onRequestPermissionsResult(int request, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(request, permissions, results);
        if (request == 1) { if (results.length > 0 && results[0] == PackageManager.PERMISSION_GRANTED) grantMicrophone(); else if (microphone != null) { microphone.deny(); microphone = null; } }
    }
    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request == 3) exports.result(result == RESULT_OK && data != null ? data.getData() : null);
        if (request == 2 && chooser != null) { chooser.onReceiveValue(WebChromeClient.FileChooserParams.parseResult(result, data)); chooser = null; }
    }
    @Override protected void onResume() { super.onResume(); foreground = true; if (web != null) web.onResume(); }
    @Override protected void onStop() {
        foreground = false;
        if (microphone != null) { microphone.deny(); microphone = null; }
        if (!exports.choosing()) exports.cancel("Save interrupted; the original remains available");
        boolean keepPlayback = speech != null && speech.enabled();
        if (speech != null && !keepPlayback) speech.shutdown();
        if (web != null) web.evaluateJavascript("window.NativeSpeechAudio?.background(" + keepPlayback + ")", null);
        super.onStop();
    }
    private void navigateBack() {
        if (web != null && web.canGoBack()) web.goBack();
        else new AlertDialog.Builder(this).setMessage("Leave this conversation?")
            .setPositiveButton("Close", (d,w) -> finish()).setNeutralButton("Change home", (d,w) -> {
                exports.cancel("Conversation changed; the original remains available"); if (speech != null) speech.shutdown(); if (web != null) { web.destroy(); web = null; }
                getPreferences(MODE_PRIVATE).edit().remove("home").apply(); setup();
            }).setNegativeButton("Stay", null).show();
    }
    @Override protected void onDestroy() { exports.close(); if (speech != null) speech.shutdown(); if (bound) { unbindService(connection); bound = false; } if (web != null) web.destroy(); if (chooser != null) chooser.onReceiveValue(null); super.onDestroy(); }
}
