# iOS native audio preparation

The existing native-client plan selects an equivalent iOS client. This package
prepares its audio core with the same 24 kHz mono PCM16 span contract as Android's
`PcmPlaybackQueue`: SHA256 integrity, exact sequence/source offsets, at most
32768 bytes per chunk, 30 seconds queued, 128 source identities and 512 live
spans. Default lifetime is 300 seconds; an explicit duration may reach 600.
Flush retires sound and source identities without resetting sequence or lifetime.
Scheduling/submission never becomes a played receipt. The eventual native driver
must provide its current owner's actual playback observations and fence late
callbacks by owner and playback epoch, including after pause, flush or stop.

The queue belongs to one serial output owner. `PCMPlaybackDriver` adds that
main-actor owner, an absolute nonrenewable deadline and at most eight scheduled
buffers. Native callback receipts advance only a contiguous completed prefix;
flush/close invalidate the epoch before unscheduling. Pause retains the queue
and needs explicit resume. Device failure closes without replay. The host supplies
its owned audio-session activation/deactivation closures; a failed activation
must clean up its own partial session change. Idle/paused/closed output releases
its own holder. Host lifecycle/interruption/route changes must close the driver.

`ApplePCMOutput` uses AVAudioEngine/AVAudioPlayerNode with mono24kHz float buffers
converted from the exact signed little-endian PCM16 bytes. It requests
`.dataPlayedBack` completion and dispatches it back to the owner; no stop or
unscheduling occurs inside an audio-thread callback. Manual/offline playback
cannot emit a played receipt. This package makes no microphone, provider,
authentication or background activation request.

Five queue XCTest scenarios cover partial submission/playback, source offsets,
atomic refusal, queue/source/span bounds and ten minutes across flush. Five more
check the actual Apple buffer conversion and synthetic device callbacks for
ordered receipts, stale epochs, pause/eight-buffer bounds, expiry and failure
without retry. These tests do not establish actual device playback or session
policy. CI uses the existing public repository's standard macOS runner, two
compiler jobs and a ten-minute deadline. No dependency package or signing identity
is fetched. Scratch output belongs to the disposable runner; no generated audio
or new retained cache/artifact is uploaded.

`AudioSessionOwnership` prepares one shared session with at most two independent
holders. A stale release cannot deactivate a fresh owner. Failed activation gets
one owned cleanup attempt; failed cleanup/deactivation remains unconfirmed and
blocks new owners until exact explicit recovery. Four synthetic-session tests
cover these transitions without an audio device.

The iOS-only `IOSSpokenAudioSession` supplies `.playback`/`.spokenAudio` ducking
and deactivation notification to other apps. It closes prepared as well as active
outputs on background, interruption begin or selected route loss/change. Returning
to the foreground allows a later explicit start; no notification resumes sound.
One instance belongs to the app, not one per buffer. Output alone uses this
policy; the explicit foreground capture extension below adds frozen duplex
ownership without a background entitlement. Actual session,
route and music behavior is not qualified by source/SDK compilation.

CI also compiles every source against the actual iOS Simulator SDK with an iOS17
minimum target and signing disabled (a transient library, not an application).
This catches iOS-only API/type errors that a macOS package build cannot observe.
Neither the dylib nor build cache is uploaded; the ten-minute job bound remains.

The prepared `InteractionWebViewController` embeds the existing hosted Conversation
and shared-auth navigation using persistent WKWebsiteDataStore. Its injected
source must be the existing app-bundled Android `native-audio.js`, not a downloaded
script or a new fork. The wrapper keeps native capabilities off and encoded
playback in the existing browser path until the iOS journey is qualified.
A valid app home is HTTPS without credentials/query/fragment/nonstandard port.
Only exact-origin `/interaction/` main frames can send the closed PCM contract;
SSO/external pages have no native bridge. Other HTTPS links open externally only
from a deliberate link navigation. The server still owns login, actor and grants.

Per-document UUIDs fence callbacks and commands across navigation, even when
page-local PCM serial IDs repeat. At most16retired documents are retained per
controller; thereafter no native admission occurs until an explicit new view.
The controller closes output on navigation, WebContent termination and leaving
its visible/foreground lifecycle. It never replays a page/input/media submission.
Three Foundation tests cover selected-home/page boundaries, closed message fields
and numeric/bool refusal. SDK compilation checks the iOS-only WKWebView types;
the current receiving qualification below owns actual injection/iframe/native runtime evidence.

`App/AppDelegate.swift` now prepares the scene-based UIKit application: existing
shared-session ownership, native home entry, persistent home selection and the
hosted Conversation controller. `build-simulator.sh OUTPUT` builds an ad-hoc
simulator bundle with the exact existing native adapter and its checked SHA256.
It refuses an existing output directory; bundle bytes stay below16MiB. This is
not device signing or an IPA/TestFlight release.

`--qualification` builds a separate fixture-only bundle ID and finite page-start
hook that are absent from the normal app. `test-simulator.py --app BUNDLE` creates
one disposable installed iOS simulator and a loopback-only HTTPS fixture. Its
one-day test CA is added only to that simulator; production app TLS policy and
existing device/keychains are unchanged. The fixture sends synthetic PCM through
the actual WebView/bridge/engine, checks played receipts and pause/resume/flush,
attempts iframe reservation, then navigates outside the application path. No
microphone, provider, human voice original or real shared-login account is used.
It starts the fixture once, waits at most35seconds for its terminal receipt and
never relaunches on timeout. Teardown removes that device, server, CA and state.
CI replaces the redundant standalone SDK library build with this application
build and journey under the same ten-minute cap. No app/certificate/audio artifact
is uploaded. App code e8eac0a passed macOSCI42380042745 (17cases) and the actual iPhone16/
iOS26.2 simulator journey:28800played bytes, pause/resume/flush, iframe unable to
reserve native output, and native facade absent outside the app path. App build,
exact adapter, install/launch and scoped terminate/shutdown/delete/server cleanup
all passed. This is synthetic PCM/device evidence, not physical hearing, capture,
duplex voice or real shared-login/passkey acceptance. The native capabilities
remain off for normal users until those missing paths qualify. CI also follows
changes to the shared Android adapter; documentation-only changes need no new
native execution.

Final head9fa7cfc did not reproduce that journey in CI42380585273: launch
observation timed out and no terminal fixture receipt arrived. Device/server
cleanup passed; the failure remains unresolved. The fixture now allows90seconds
for the one launch observation and retains a bounded test-only startup stage,
loopback request observations and a three-minute native log on failure. It never
relaunches an uncertain start. These diagnostics are absent from pilot builds.

This is not a device-distributable iOS app. Continue with actual AVAudioEngine/device and iOS AVAudioSession acceptance,
actual WKWebView bridge/document injection and shared-login shell acceptance,
foreground capture/route/interruption/return handling, progressive/background
speech, native Beacon and actual simulator/device acceptance. Reuse the hosted
Conversation, account grants and existing wire contract; do not create another
login, editor or server. No iOS capability is advertised by this package.
Distribution, signing and physical iPhone acceptance remain unqualified.

References:

- [Selected client plan](../../docs/interaction-native-clients.md)
- [Apple playback completion types](https://developer.apple.com/documentation/avfaudio/avaudioplayernodecompletioncallbacktype)
- [Swift Package Manager](https://docs.swift.org/main/documentation/packagemanagerdocs/)
- [Standard GitHub runner contract](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)

## Prepared foreground duplex capture

The iOS-only native-capture.js hook wraps the existing shared adapter, without
changing Android or the hosted recorder. Before getUserMedia it acquires one
UUID-owned native session holder. WebKit and the OS still prompt; only the selected
HTTPS main-frame microphone request can reach that prompt, never camera/iframe or
SSO. The closed reply handler accepts only begin/end with canonical document and
owner UUIDs, with at most128 nonreusable capture identities per controller. This is ownership, not permission or proof of recording.

A duplex holder selects playAndRecord/voiceChat before microphone permission.
PCM output joins that same mode and cannot deactivate it when becoming idle.
Mode stays frozen while any holder remains; starting capture during existing
spoken-only playback refuses before asking permission, instead of changing an
active audio graph. Capture and output share the existing two-holder bound.
A capture stop is requested at600seconds, route/interruption/background, explicit
track stop or page exit. Native cancellation requests actual WebKit microphone state none;
only its observed completion releases that exact session holder. Unknown stop or
session-deactivation outcomes stay held, never retried by refresh. A late device
permission closes its returned tracks after the old capture epoch has ended.
Returning foreground never resumes an old capture or sound.

Synthetic Node checks cover explicit track stop (which emits no ended event),
concurrent requests, late native/OS grants, permission refusal and lifecycle.
Foundation cases cover frozen mode/independent capture-output holders and the
closed native contract. These are not real microphone, permission, WebKit duplex,
route/interruption or phone acceptance. Native audio capabilities remain off;
no production build, provider call, entitlement or distribution is selected.
Apple references: [voiceChat](https://developer.apple.com/documentation/avfaudio/avaudiosession/mode-swift.struct/voicechat),
[media permission delegate](https://developer.apple.com/documentation/webkit/wkuidelegate),
[actual capture stop](https://developer.apple.com/documentation/webkit/wkwebview/setmicrophonecapturestate(_:completionhandler:)).

The first duplex source9eacc4b passed20Swift cases, Node capture races and the
actual iOS app build in CI42382542969. Its runtime stopped before installation
at the fixture CA admission30second timeout; shutdown/delete/server cleanup
passed. Two consecutive runs hit this same prerequisite. The fixture now gives
that single certificate operation90seconds, without retrying an uncertain write
or weakening TLS/PCM assertions. Runtime capture/permission acceptance is pending.

The extended simulator fixture tests the actual with-reply capture bridge from
its own same-origin iframe (must refuse), then reserves one main-frame duplex
session across real PCM playback/idle/pause/flush. It never requests getUserMedia
or changes a microphone permission. This checks session/bridge coexistence only;
actual microphone permission, audio input, duplex processing and physical hearing
remain unqualified. Source20tests/SDK evidence is separate from this runtime result.

Extended fixturea251f42 reached the actual app in CI42383442620 but received no
capture iframe reply. Its srcdoc frame was incompatible with the shell's
HTTPS-only navigation contract. The probes now load distinct same-origin HTTPS
child documents and require their own submission/reply receipts before proceeding;
no production navigation or frame authorization policy was widened. Earlier
srcdoc non-reservation does not prove the native handler saw a frame request.
