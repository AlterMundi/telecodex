# Independent native Interaction clients

Human selection, 2026-10-29: make an installable Android client the next delivery
focus, reusing the current independent web conversation. It must be usable as a
person's sole access to their hosted daimon. Telegram remains an optional
projection, not installation, sign-in or navigation infrastructure. Preserve the
unfinished continuous-voice/foreground/attention obligations in #301.

## First Android pilot

Source: `clients/android`. This is a thin Android WebView host with a Java audio
owner and an origin-scoped message bridge; it does not need Capacitor or a second
frontend. The user selects their HTTPS home once. The app opens its existing
`/interaction/` standalone entry and reuses shared authentication, cookies,
receiver authorization, original IDs, source revisions and pending ledgers.
There are no embedded accounts, bot tokens, speech keys or new enrollment API.
The shared login offers its existing code fallback when passkeys/Google cannot
operate in an embedded browser; do not copy browser cookies or build another
login system. Real receiving sign-in remains a device acceptance requirement.

The native bridge is allowed only on the exact selected HTTPS home, main frame,
foreground activity and `/interaction/` path. Shared-auth pages and external
links cannot control speech. Microphone access requires the page's existing
explicit capture action plus Android runtime permission and the exact receiving
origin. Other WebView media permissions are denied. Attachments use Android's
document picker, without broad storage permission. Page navigation/destruction
releases native output. Backgrounding stops microphone tracks and speech; it
does not start a service or promise background capture/playback.

The pilot's speech driver reuses the canonical web player's source validation
and complete, integrity-checked response bytes. Only after source validation
unmutes that exact segment does it transfer bounded bytes to the native decoder.
One segment at a time, at most8MiB, ordered chunks at most32KiB, SHA-256 verified
again natively;30s transfer/readiness deadlines. The native player has no network
access or provider credentials. Audio remains in memory and is released on
replacement/stop/error/background/destruction; no additional recording/archive
or persistent speech cache is created. Ordinary server retention and protected
Bitácora originals are unchanged.

Android requests `AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK` with navigation/speech audio
attributes immediately before playback. Completion, pause, failure and stop
abandon that exact focus request. Focus loss pauses the conversation rather than
fighting the other app or issuing cross-app Play commands. The former holder's
actual resume behavior must be tested; a successful focus API call is not proof
that YouTube Music resumed. See #454 and Android's
[audio focus contract](https://developer.android.com/media/optimize/audio-focus).

This first pilot deliberately buffers each speech segment. Native progressive
decoding is unfinished; the normal browser keeps its existing progressive path.
There is no foreground media service/notification yet, so this is a foreground
pilot, not the completed mobile experience. Native Beacon synthesis, filters,
mixing, output-route changes and iOS are not implemented. Never label a shell,
compiled APK or synthetic bridge test as completed stages2/3/required4.

## Build, delivery and operator path

JDK17, Android SDK36, Android build tools35 and the checksum-pinned Gradle8.13
wrapper build this project. AGP8.13.2 and AndroidX Webkit1.14.0 are pinned in the
owning Gradle files. `clients/android/gradlew --no-daemon :app:assembleRelease
:app:lintRelease` (from that directory) creates an **unsigned** release APK.
The scoped Android CI performs the same build/lint and native-adapter tests and
retains that artifact for7days. Build success is not signing or installation.

Sign the exact qualified release with the receiving operator's retained private
Android signing key using the standard SDK `apksigner`; verify the signature and
SHA-256 before distributing. Keep custody material/passwords outside the source,
artifacts and public logs. Preserve the same key for future upgrades. A debug key
or a new key on every CI run is not the update strategy. A private pilot APK may
be sent directly to the selected human without Play Store publication. General
public distribution and iOS signing/store delivery remain separate selections.

Place SDK/Gradle/build outputs on the operator's existing data filesystem, bound
the build to2workers/1.5GiB Gradle heap and20minutes in CI, and retain only the
current artifact plus its actual prior rollback. Intermediate completed CI
artifacts expire after7days; private signing custody and unfinished acceptance
evidence are not disposable caches. Do not accumulate per-turn SDK copies or
use Beacon-reserved compute. No production deployment/restart is needed to
compile the APK. Its small web adapter must be receiving-selected before native
speech works; an older receiver will retain its web behavior.

## Next increments and acceptance

1. Build/sign/install the pilot; qualify shared login, complete text/voice input,
   reply/source binding, copy/files, native speech and music recovery on Android.
2. Implement progressive native speech and a single Android media service/session
   for deliberate background playback, notification/headset controls and route
   changes. Preserve visible audio enable, pause, Mute/Solo, heard position and
   prompt recovery. Do not change ownership midway through a spoken segment.
3. Move Beacon composition/mixing into the native audio graph, retaining the
  40.4Hz sine vocabulary and gentle envelopes. Add selected filters through an
   explicit bounded graph; no new synthesis palette or hidden recording.
4. Implement the equivalent client driver on iOS using its native audio session
   and graph. Share conversation/protocol behavior, not Android APIs or assumed
   entitlement/signing. iOS delivery is unqualified until built and received.

Device acceptance covers music already playing, end of reply, multipart speech,
waiting for a later result, user pause/resume/stop, error, independent music pause,
barge-in, capture exit, rotation, reconnect, page/auth navigation and app lifecycle.
An existing player must not acquire permanent focus behind the native owner.
Background functionality is separately qualified after its service exists.
Synthetic JS tests prove byte/source/bridge fences; Android build/lint prove
compilability/API discipline. Neither proves hearing, physical mic quality,
external music recovery, phone login or complete mobile usability.
