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
releases native output. The0.1/0.2 pilots stop speech when backgrounded. The0.3 candidate below stops
microphone capture while allowing an explicitly enabled speech session to continue.

Android0.5 corrects a missing manifest permission: `MODIFY_AUDIO_SETTINGS` is
required alongside runtime `RECORD_AUDIO` for Chromium's communication input
device enumeration. Granting the latter alone does not satisfy that contract;
see [Chromium AudioManagerAndroid](https://chromium.googlesource.com/chromium/src/media/+/refs/heads/main/base/android/java/src/org/chromium/media/AudioManagerAndroid.java).
The added permission is normal/install-time, not another user permission dialog.
The exact-origin, foreground and audio-only WebView grant remains unchanged.
A real API35 WebView test exercises the host permission callback, opens emulator
input and records a short nonempty MediaRecorder result without any upload,
provider, physical capture or retained recording. It does not establish Redmi
acceptance or test the Android runtime permission dialog itself. The reported
Redmi `NotReadableError` after granting microphone permission is consistent with
the missing prerequisite; the phone's exact underlying exception is unobserved.
Upgrade with the same package/signing key and higher versionCode; retain login
and app data. Do not clear data or reinstall to repair this permission.

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

The first0.1 pilot deliberately buffers each speech segment. The0.2 candidate
reuses the existing verified NDJSON speech reader for progressive native output:
each checked block is passed to Android, whose bounded random-access source
waits for missing bytes rather than returning premature EOF. Android repeats
sequence/block-digest checks and exposes EOF only after the terminal size/hash
match. Prefetched streams remain in JS until the exact source has been validated
and unmuted; preparation cannot replace the active native player or acquire focus.
At most3 prepared/active stream descriptors,8MiB per source,32KiB blocks,30s
missing-data/readiness bounds and145s stream lifetime reuse the existing transport
limits. Encoded prefetch data stays in memory. Cancel/stop/background closes the
source and wakes decoder I/O before releasing the player. Decode recovery uses
the existing completed verified Blob, never a second provider request. If native decoding fails before transport completion, detach only the decoder and retain the bounded verified transport until that Blob is available; failure must not cancel its own recovery bytes.

Android0.1 remains on the complete-byte path when the web reader is updated;
ordinary browsers keep their existing MediaSource path. The candidate is not
received or physically accepted solely because bridge/buffer tests pass. The
scoped CI additionally exercises the actual Android decoder on an API35 emulator:
a12s synthetic MP3 prefix must start and advance playback before terminal bytes,
then finish after verified EOF; releasing a partial source must not block the UI.
The emulator has no physical audio/microphone/camera and uses no provider/model.

The delivered0.2 pilot is foreground only. The0.3 candidate binds the same single
`SpeechOutput` to an unexported Android `mediaPlayback` foreground service and
framework MediaSession. Enabling spoken replies in the visible app starts a
notification and a maximum two-hour session; repeated status refreshes do not
extend it. Audio off, notification Stop, task removal, page/auth navigation,
Activity destruction or the deadline release the player, focus, wake lock and
notification. No boot restart, sticky service, capture, synthesis or input replay.
The partial CPU wake lock has the same two-hour bound; the foreground service
keeps the existing WebView conversation/SSE queue available between replies,
without a second server reader or source store. Android/browser power policy and
real-device late-result behavior still need qualification; do not promise survival
of OS process destruction or automatic listening after restart.

MediaStyle notifications carry the real native MediaSession and use Android’s
[media-session notification exemption](https://developer.android.com/develop/ui/compose/notifications/notification-permission#exemptions), not a general notification permission. The scoped lint annotation covers only that notification publisher; the fresh-emulator service test exercises it without requesting POST_NOTIFICATIONS.

Notification/headset Play/Pause/Audio off return to the canonical web controls;
Pause silences native output immediately before that reconciliation. The browser
MediaSession is not installed beside this native session. Noisy-route changes
pause rather than emitting speech onto the speaker. One transient speech-focus
owner remains; no commands are sent to resume another music app. Background
capture always stops (including late permission grants), and cannot restart from
notification Play. The exact selected HTTPS home/main frame/application path
still owns the bridge. Background audio commands require an already-enabled
session; background code cannot start or extend it. Navigation/auth pages retain
no bridge or active listening grant. Ordinary browsers and older APKs retain
existing behavior.

CI adds a real Android service journey with synthetic local MP3: stop the Activity,
finish one reply, prepare/play another while still stopped, exercise notification
Pause and Audio off, and verify notification teardown. This proves service/decoder
mechanics, not the complete WebView/provider late-result path or phone acceptance.
Native JS tests separately keep the player on background while closing capture,
route media controls, and retain later-source selection. The candidate is not
received merely because those checks pass. Native Beacon synthesis, filters,
mixing, output-route changes and iOS are not implemented. Native0.4 adds an export candidate to the existing artifact, original-file and
local recording Save buttons. Web source/revision/length/hash/authorization checks
remain before export. The native bridge accepts only in-page Blob bytes, never a
page URL, path, credential or destination URI. Each operation has a fresh random
identity and one acknowledged32KiB chunk at a time. Android independently checks
sequence/length/SHA-256 before opening ACTION_CREATE_DOCUMENT. The person chooses
one new destination; there is no broad storage permission, directory grant,
automatic open/share, file upload, transcription or message dispatch.

One private cache stage is allowed, maximum256MiB (matching existing artifact
limits), with32MiB free-space reserve. A single worker with a four-command queue
avoids writing on the UI thread. The ten-minute lifetime includes transfer and
picker; cancellation/deadline is checked between provider I/O calls. Arbitrary
provider calls can block; do not claim a hard kernel I/O deadline. Navigation,
non-picker backgrounding and Activity destruction cancel. Picker backgrounding
is expected and keeps only its verified stage. Completion/cancellation/failure
removes that cache file; a crash orphan is cleaned at the next launch or save,
and remains bounded to one stage while the app is stopped. Server originals,
protected Bitácora audio and recoverable local recording originals are unchanged.
No new durable media archive or automatic save retry.

Only a successful close of the selected provider output reports Saved. Picker
cancel reports not saved and keeps the original. Failed/uncertain writes are not
retried; deletion of the newly-created partial document is best effort, and the
UI warns it may be incomplete. No URI is accepted from page JSON or retained as
an ongoing grant. Existing browsers/older APKs keep Blob-download behavior.
Native JS/JVM tests exercise chunk/identity/verification/cancellation; the scoped
Android test writes and reads back real ContentResolver bytes against a fixed
synthetic test-only provider. It does not prove the real system picker UI, cloud
storage, phone acceptance or sole-client completion. That provider is absent from
the release APK; debug/test APKs remain CI-only, expiring after one day. Never label a shell,
compiled APK or synthetic bridge test as completed stages2/3/required4.

## Build, delivery and operator path

JDK17, Android SDK36, Android build tools35 and the checksum-pinned Gradle8.13
wrapper build this project. AGP8.13.2 and AndroidX Webkit1.14.0 are pinned in the
owning Gradle files. `clients/android/gradlew --no-daemon :app:assembleRelease
:app:lintRelease` (from that directory) creates an **unsigned** release APK.
The scoped Android CI performs the same build/lint, native-adapter and JVM
verified-buffer tests and retains that artifact for7days. Debug APKs used only
by the decoder emulator expire after1day; compact decoder evidence after7days.
The emulator job has a15-minute limit,2virtual cores/1536MiB RAM,10GiB initial
runner headroom, bounded SDK/boot/test commands and a trap that stops its own
emulator. AVD creation and emulator lookup use the same explicit task-owned Android user/AVD directories; the AVD index must exist before launch. The trap removes that directory after stopping its emulator. SDK state is ephemeral to that hosted job. Build success is not
signing, receiving selection or installation.

Sign the exact qualified release with the receiving operator's retained private
Android signing key using the standard SDK `apksigner`; verify the signature and
SHA-256 before distributing. Keep custody material/passwords outside the source,
artifacts and public logs. Preserve the same key for future upgrades. A debug key
or a new key on every CI run is not the update strategy.
Android [prevents normal lower-versionCode installs](https://developer.android.com/studio/publish/versioning).
An installed-client rollback is a forward release of known code with a higher
versionCode and the same package/key, retaining app data and login. Do not uninstall
or clear app data to restore older code. Retained older APKs are evidence/source
references, not direct downgrade instructions; compatible web rollback can be
selected independently. A private pilot APK may
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
