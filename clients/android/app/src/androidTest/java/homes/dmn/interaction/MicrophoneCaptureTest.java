package homes.dmn.interaction;

import android.Manifest;
import android.content.Context;
import android.content.pm.PackageManager;
import android.webkit.JavascriptInterface;
import android.webkit.WebView;
import androidx.test.core.app.ActivityScenario;
import androidx.test.platform.app.InstrumentationRegistry;
import java.lang.reflect.Field;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.Test;
import static org.junit.Assert.*;

/** Real WebView permission and MediaRecorder, emulator input only; no upload/provider/archive. */
public class MicrophoneCaptureTest {
    public static final class Receipt {
        final CountDownLatch done = new CountDownLatch(1);
        volatile String result;
        @JavascriptInterface public void finish(String value) { result = value; done.countDown(); }
    }

    @Test public void grantedAndroidPermissionOpensWebViewMicrophone() throws Exception {
        Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();
        assertEquals("Chromium input-device permission", PackageManager.PERMISSION_GRANTED,
            context.checkSelfPermission(Manifest.permission.MODIFY_AUDIO_SETTINGS));
        InstrumentationRegistry.getInstrumentation().getUiAutomation()
            .grantRuntimePermission(context.getPackageName(), Manifest.permission.RECORD_AUDIO);
        assertEquals(PackageManager.PERMISSION_GRANTED, context.checkSelfPermission(Manifest.permission.RECORD_AUDIO));
        AtomicReference<android.content.SharedPreferences> preferences = new AtomicReference<>();
        AtomicReference<String> previous = new AtomicReference<>();
        Field field = MainActivity.class.getDeclaredField("web"); field.setAccessible(true);
        Receipt receipt = new Receipt();
        try (ActivityScenario<MainActivity> scenario = ActivityScenario.launch(MainActivity.class)) {
            AtomicReference<WebView> view = new AtomicReference<>();
            scenario.onActivity(activity -> {
                // Activity.getPreferences uses getLocalClassName, not a guessed preference filename.
                preferences.set(activity.getPreferences(Context.MODE_PRIVATE));
                previous.set(preferences.get().getString("home", null));
                preferences.get().edit().putString("home", "https://capture.invalid").commit();
                try {
                    java.lang.reflect.Method open = MainActivity.class.getDeclaredMethod("open", String.class);
                    open.setAccessible(true); open.invoke(activity, "https://capture.invalid");
                } catch (Exception error) { throw new AssertionError(error); }
            });
            long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(5);
            while (view.get() == null && System.nanoTime() < deadline) {
                scenario.onActivity(activity -> { try { view.set((WebView)field.get(activity)); } catch (Exception error) { throw new AssertionError(error); } });
                if (view.get() == null) Thread.sleep(50);
            }
            assertNotNull("Host WebView initialized", view.get());
            scenario.onActivity(activity -> {
                WebView web = view.get(); web.stopLoading(); web.getSettings().setBlockNetworkLoads(true);
                web.addJavascriptInterface(receipt, "CaptureReceipt");
                web.loadDataWithBaseURL("https://capture.invalid/interaction/", "<!doctype html><script>" +
                    "(async()=>{let stream;try{stream=await navigator.mediaDevices.getUserMedia({audio:true,video:false});" +
                    "const recorder=new MediaRecorder(stream);let bytes=0;recorder.ondataavailable=e=>bytes+=e.data.size;" +
                    "recorder.onstop=()=>{stream.getTracks().forEach(t=>t.stop());CaptureReceipt.finish(bytes>0?'recorded':'empty');};" +
                    "recorder.start();setTimeout(()=>recorder.stop(),350);" +
                    "}catch(e){stream?.getTracks().forEach(t=>t.stop());CaptureReceipt.finish(e.name+': '+e.message);}})();" +
                    "</script>", "text/html", "UTF-8", null);
            });
            assertTrue("Capture completed: " + receipt.result, receipt.done.await(15, TimeUnit.SECONDS));
            assertEquals("Actual WebView capture", "recorded", receipt.result);
        } finally {
            if (preferences.get() != null) {
                if (previous.get() == null) preferences.get().edit().remove("home").commit();
                else preferences.get().edit().putString("home", previous.get()).commit();
            }
        }
    }
}
