# Independent native Interaction clients

Human selection, 2026-10-29: make an installable Android client the next delivery
focus, reusing the current independent web conversation. It must be usable as a
person's sole access to their hosted daimon. Telegram remains an optional
projection, not installation, sign-in or navigation infrastructure. Preserve the
unfinished continuous-voice/foreground/attention obligations in #301.

## Native Beacon candidate0.8 —7November2026

The prepared `BeaconSynth` is a bounded six-carrier PCM synthesizer for the
existing Sonic score. It accepts normalized component weights from the existing
observer/mapping, not session state, emotion, a new observer or executable art.
Six fixed carriers stay phase-continuous across chunk boundaries and zero weights.
Membership and master-level changes retarget their current envelopes; duplicate
observations do not restart a transition. Sinusoidal breathing follows the
configured shared/harmonic pulse frequencies. No extra partial stack or texture
is introduced. Mix weight stays at most one; the master maximum is0.8, with
the selected vocabulary's lower peak applied by the renderer. Unsupported
nonzero frequencies above45%of sample rate refuse rather than alias.

Six release17 JVM cases verify the exact40.4series/absent unwanted partials,
simultaneous components/peak bound, chunk/duplicate-update equivalence, harmonic
amplitude breathing, mid-transition retarget/fade-to-zero and atomic invalid input
refusal/defensive copying. Output arrays are transient synthetic samples; no
recordings, provider calls or audible device test occurred.

The source now also prepares `BeaconOutput` and `SpeechService` commands. One
audio worker keeps one latest frame/level instead of accumulating a queue.
Six-carrier configuration is frozen per owner; numeric frame bounds and mix are
validated before admission. Frame validity is at most20seconds and level changes
do not renew it. Readiness stamps the native monotonic clock; the prepared frame
contract requires an absolute deadline on that clock. The prepared JS adapter derives
it conservatively from receipt time and the caller's remaining source validity, so delayed bridge
delivery cannot manufacture a fresh20second allowance. Expired observations fade
their weights to zero. The two-hour
owner lifetime includes initialization and is not extended by refreshes. Quiet
workers sleep until a control or deadline rather than continuously synthesizing
silence. AudioTrack writes are nonblocking with a bounded native buffer and
two-second stalled-head refusal. Stop/lifecycle cannot admit later frames.

Speech and Beacon share the existing AudioFocusRequest with independent holders;
ending a spoken reply cannot abandon an audible Beacon, and stopping Beacon cannot
abandon a speaking reply. Stale focus callbacks are ignored. Quiet observations
carry the applied control revision so they cannot release a newer frame's focus.
One stopping owner may retain one explicitly requested next owner; UUID identities
prevent old-page callbacks from targeting the new page. Foreground loss/noisy
route/global audio stop cancels that pending start. Disabling spoken replies stays
separate from Beacon. These are source mechanics, not new background permission.

The seven real synth/output/service classes compile with release17 against the
verified API36 reference jar; that isolated signature check supplies an Activity
class-literal stub and does not compile or qualify the real Activity/WebView.
The six synth plus five PCM JVM tests pass. The prepared `openBeacon` JS adapter
has a separate UUID owner and five-second readiness bound; closing during readiness,
native failure or backgrounding clears it without resetting speech. Its closed
numeric frame contract rejects extra fields, excessive weights, invalid expiry
and audible aliased carriers before any bridge write. Level commands carry no
freshness. Node qualification covers a cached observation keeping its original
absolute expiry, invalid frames with no effects, late-owner events, pending-start
cancellation, continued speech on background and throwing diagnostic callbacks.
The existing renderer has a native branch gated by the APK0.8
`beacon=true` capability. It reuses the six components, equal per-session mix,
vocabulary frequencies/pulses/peak, source-age validity and capture/speech ducking;
no browser AudioContext is created after native selection. Node fixtures cover
the normalized mix, no freshness renewal by ducking, late starts and owner failure
without browser fallback. Older clients keep the browser renderer.
The preparation065691d passed mainCI42373895476 and Android42373895620:
release/debug/test builds, lint, JVM cases, exact test-only asset packaging and
all eight real API35 emulator instrumentation cases. Beacon's production
WebView/bridge/service/AudioTrack path showed playback-head progress, stale
silence despite level changes, fresh-state resume, duck-to-zero/recovery and
native background closure. The separate-UID music probe also verified that
speech completion cannot release Beacon's retained focus. These are synthetic
score/device observations, with no microphone, provider call or hearing claim.

APK0.8 now advertises the qualified native branch. Exact final-source CI, signing
and receiving delivery remain required; until selected, served clients and
signed APK0.7 remain unchanged. Physical phone/hearing, background voice and the
complete original goal remain separate.

## Public Live native output candidate0.7 —7November2026

The0.7 Android bridge advertises `webrtcPcm` and a600second maximum. Public
OpenAI Live reserves the existing native PCM owner before microphone permission
or provider offer creation. Remote WebRTC audio passes through a24kHz zero-output
AudioWorklet into that owner; no HTML Audio player is created on this path.
Older APKs keep their existing browser playback. A selected native failure closes
the visit rather than silently falling back. Hold releases native output while
the existing bounded provider final-usage receiver drains. Late preparation is
closed without opening the microphone. Cancellation releases the native owner
even while worklet module loading remains unresolved. Muting/pausing flushes queued spans;
resuming starts a new source identity and cannot replay old queued audio.

The legacy PCM default stays300seconds; an explicit session may select300..600.
The30second queue bound and lifetime allowance survive draining and flush.
This adds neither background microphone capture nor provider/default changes.
VersionCode7 requires a same-key APK upgrade; source preparation is not delivery.

Node exercises the real engine/helper and the full600second worklet allowance;
JVM tests exercise the queue lifetime across flush. The optional
`tests/dashboard_webrtc_native_audio_browser.cjs` uses Playwright/Chromium with
a synthetic48kHz oscillator to verify actual24kHz conversion, pause/resume and
closure without an audible browser destination. It uses no provider or mic.
The API35 instrumentation additionally loads these exact two web assets inside
the real host WebView at an intercepted HTTPS test origin. An actual button
gesture starts a synthetic stream through the production bridge/service and
AudioTrack; native frame-head progress, paused append exclusion, fresh resumed
audio and single closure are asserted. This test has no provider or microphone.
Its execution result must be retained separately from preparation. Physical
focus/background behavior and hearing acceptance remain required; desktop or
emulator evidence cannot prove them.

## Native conversation candidate0.6 —1November2026

The0.6 source candidate connects the existing native speech foreground above Sol
to Android's single speech owner through verified streaming PCM/AudioTrack.
It preserves ordinary encoded playback/background sessions, shared login,
permissions, original/question scope and existing native work. Older APKs retain
the prior avatar mode; the browser is unchanged. Native conversation remains
foreground-only and closes on Activity background/focus loss, retaining original
recovery. No new background capture or automatic model/input replay.

See the [PCM contract](interaction-voice-foreground.md#android-native-pcm-candidate)
for chunk/frame-head/interruption bounds and actual qualification limits. JVM
queue and Node engine/adapter checks are component evidence; real AudioTrack/
WebView/provider/phone acceptance must remain separate. VersionCode6 requires
same-key upgrade. Source or a CI APK is not a deployed/installed client.

## Avatar background suspension correction —7November2026

The source fullscreen controller now consumes Android's existing
`nativecapturebackground` event even while the WebView still reports visible.
Continuous Avatar suspends through its current engine owner; the legacy capture
view closes. Returning to the app does not reopen the microphone. Ordinary
explicit background reply playback retains its separate existing service grant.

Public GPT-Live also silences and disconnects its local audio element as soon as
Hold starts. It keeps WebRTC/sideband alive for the existing bounded final-usage
drain; late native work and original recovery remain independently owned. A
second lifecycle signal cannot issue another close intent.

The shipped Android bridge plus fullscreen controller are exercised together in
Node with a still-visible document. A delayed-finalization public Live fixture
proves immediate local silence while the provider receiver remains open, followed
by one confirmed close. These are synthetic lifecycle checks, not physical phone,
background survival or hearing acceptance. Source delivery remains separate from
selection; no APK, provider/default or service change is implied by this fix.

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

The ordinary audio dock uses one speaker enable/off control and one shared slot
for Play/Pause or a state icon. The controls form one vertical column: speaker, state/Play/Pause, then a compact Follow toggle. All three controls are icons without background
fills. A64px dock uses36px audio buttons with20px glyphs and a24px-high Follow row, fitting
beside the composer and returning64px compared with the former128px dock. Tapping a state icon explains its current meaning without
playing or requesting audio. The explanation is a body-level fixed overlay clamped to the visual viewport,
including keyboard/zoom bounds. Long press opens the same explanation with no
native title/callout. It closes on state change, another tap, Escape, viewport
change or after8seconds; keyboard/screen-reader labels retain full meaning.

A foreground refresh already waiting on response/question snapshots uses the
existing saved-original status reader for at most two `preparing` voice cards.
The existing refresh cadence supplies at most four single-flight reads per
original within30seconds, spaced at least2seconds; no extra timer, provider
request, submission or retry.
This lets a committed transcript leave "Confirming transcript" before a slow
response snapshot finishes. Retired/read-only/hidden captures are excluded and
binding generations retain their existing fence. Browser-stage attribution of a
particular phone delay remains separate from server preparation receipts.

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

The emulator suite includes a separate test APK/UID as the music focus holder.
Its bounded foreground player asks to pause when ducked, handles the real
system's transient loss/gain callbacks, and reports actual playback-head advance
after resumption. Two PCM utterances must return focus while the same native
session stays open between them. This tests the cross-app OS boundary, not
YouTube Music's own policy, physical sound or phone acceptance. The probe has no
network/microphone/storage permission, accepts only same-debug-signer control,
and stops after30seconds even if instrumentation fails. It is a test fixture,
not a product audio owner or a cross-app Play workaround.

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

The six-case decoder suite requires a debug focus probe in addition to the app
and instrumentation APKs. Enable only that fixture with `-PfocusProbe=true` and
build `:focus-probe:assembleDebug` beside the existing debug/test tasks; the
ordinary release command above omits the module. The app's probe permission is
in `src/debug` only. CI retains the separate probe artifact for1day and invokes
`test-decoder.sh <app-apk-directory> <focus-probe-apk>`. Install the probe first
so Android can grant its signature permission to the same-debug-signed target.
The emulator remains muted, with the existing140second instrumentation deadline;
failed runs also retain bounded logcat before teardown. These checks do not
require a new signed/released APK or production recording.

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

The iOS implementation lives in `clients/ios`: verified bounded PCM queue,
main-actor AVAudioEngine playback, independent audio-session holders, exact-origin
Conversation WebView and a scene-based UIKit simulator app. It reuses existing
hosted Conversation/shared SSO and the exact bundled native-audio.js; no separate
login/editor/store/backend. Queue30s/default300/explicit600s/source/sequence/hash
and owner/epoch/lifecycle bounds remain. Unconfirmed release blocks new owners
until explicit exact-owner recovery. The spoken-output session does not grant
capture, duplex or background audio.

App code e8eac0a passed17macOS cases and the actual iPhone16/iOS26.2 simulator
WebView/native PCM journey inCI42380042745:28800played bytes, pause/resume/flush,
iframe unable to reserve output and native facade absent outside the app path.
Ad-hoc simulator build, exact adapter, install/launch and complete scoped fixture/
device/CA cleanup passed. Earlier timeout/cleanup failures retain their outcomes;
the supported fixture observes unknown install/launch without replay. A bounded
one-day CA is trusted only on the new disposable simulator, never production.
These are synthetic source/device observations, not physical hearing or shared
login/passkey/capture acceptance. Native capabilities stay off pending those paths.
Normal device packaging/signing, real account, duplex/progressive/background,
Beacon and physical iPhone acceptance remain open. No iOS distribution is claimed.

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

The Follow control is a pressed button with a focus-target icon in the bottom
audio-dock row. Accent color, heavier strokes and `aria-pressed` expose the
enabled preference; inactive uses muted strokes. It reuses the existing persisted
Follow setting and manual-scroll hold/resume behavior. Long press (context menu)
or keyboard F1 opens the existing viewport-clamped explanation headed “Follow”,
without toggling the preference or starting audio. The dock has no background
fills; keyboard focus outlines and accessible button names remain available.

### Foreground iOS capture ownership preparation

PR728 adds a bundled iOS-only wrapper around the shared recorder/adapter. One
UUID/document-scoped capture holder selects duplex before WebKit/OS permission;
PCM idle cannot release it or change its frozen mode. Native WebKit microphone
state none must be observed before holder release. Preserve permission prompts,
600second stop request, unknown cleanup, no automatic resume and disabled native
capabilities. Source/SDK/bridge-session simulation is not actual microphone,
account, device or hearing acceptance. See clients/ios/README.md for exact scope.
