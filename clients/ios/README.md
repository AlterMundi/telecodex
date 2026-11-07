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
One instance belongs to the app, not one per buffer. This is a spoken-output
policy, not a duplex capture policy or background entitlement. Actual session,
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
one-day public test anchors are copied only into that app's container and consumed
by the separate qualification-only TLS helper; system trust stores and normal
app TLS handling remain unchanged. Wrong-host and wrong-anchor validation must
fail before accepting the exact localhost chain (details below). The fixture sends synthetic PCM through
the actual WebView/bridge/engine, checks played receipts and pause/resume/flush,
attempts iframe reservation, then navigates outside the application path. No
microphone, provider, human voice original or real shared-login account is used.
It starts the fixture once, waits at most65seconds for its terminal receipt and
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


## Native Beacon DSP preparation

BeaconSynth ports the existing Android six-carrier synthesis without adding
partials, texture, a provider or a state observer. Frequencies/pulses/depth are
fixed per owner; weights stay within one normalized mix, and level stays<=0.8.
Transitions retarget current values. Identical observations cannot restart phase
or an unfinished ramp. A selected fade reaches exact silence; input arrays cannot
mutate the owner's state. Audible carriers must stay below0.45of the sample rate.
PCM16 rounding preserves Android's half-toward-positive-infinity rule.

Six new XCTest scenarios check spectrum/mix, chunk and refresh invariance,
selected breathing, retarget/fade, atomic invalid controls and a PCM SHA256 from
the actual existing Java BeaconSynth. The two-second reference includes six
components, breathing, retargeting, duplicate controls and fade. Java generation
used only disposable local classes, removed after producing the digest; no audio
artifact, mic, provider or live observer was retained. Sourceb7b7155 passed23XCTest cases and the actual iOS app build in
CI42386636123, including the Java/Swift PCM digest. The runtime journey then
failed after an unconfirmed90second launch and no page receipt; cause remains
unknown. Termination was unconfirmed, but shutdown/delete/server stop were
acknowledged. That journey did not exercise this unwired DSP core and is not
recorded as a native Beacon playback pass.

`BeaconPlaybackDriver` now prepares foreground output through the existing
ApplePCMOutput and IOSSpokenAudioSession. One reschedulable monotonic timer and
at most two40ms PCM buffers bound pending output; controls replace current targets
without accumulating work. Only a contiguous actual played-callback prefix can
report sound. A two-second callback stall closes the owner without retry. Frame
validity is at most20seconds and gain cannot refresh it; stale weights fade to
zero over one second. The nonrenewable two-hour deadline includes a final350ms
fade and an independent sample-production ceiling. Quiet output releases only
its own shared-session holder; host route/interruption/foreground-exit closes it
without automatic resume. The combined speech/Beacon owner count stays at two.

Five new synthetic-device tests cover queue/callback ordering, stale/gain/fresh
controls, independent speech ownership, absolute lifetime, failure and no retry.
The qualification-only app probe uses the actual native output/session to require
played callbacks, stale quiet, fresh resumption, selected gain-zero quiet, gain
restoration and host foreground-exit closure. It runs once with a12second bound,
requests no microphone/provider/observer and does not expose a new JS command.
Its receipt is required before the existing PCM WebView journey. This exercises
host foreground-exit policy, not an actual OS background transition or physical
hearing/music/route acceptance. SDK/runtime outcomes must be recorded separately.

The native Beacon capability stays off. The prepared closed per-document adapter
now reuses the existing beacon_begin/frame/level/stop contract. Continue actual
bridge/runtime and capture/duplex compatibility qualification before advertisement. Physical phone/music/hearing, shared login, distribution/signing
and the full original goal remain open.

Historical CA provisioning used a90second observation bound, qualified in
PR728source816fd9a andd1f1f4b after a30second timeout and an84.88second successful
admission. PR731 subsequently timed out even at90seconds before installing the
app. That system-trust-store provisioning is now removed from this fixture as
described below; its outcomes remain historical evidence, not a command to repeat.
The separate capture implementation/review is not included.


### Qualification TLS isolation after PR731's CA timeout

Sourceb5c440b passed28Swift cases (zero failures,1.638s), actual iOS SDK app build
and all five general checks. RuntimeCI42390310788 failed during90second fixture
CA admission before app install/launch; runtime listing took77.63s and boot103.1s.
Shutdown/delete/server cleanup passed. That outcome is not Beacon playback proof
and the terminal operation was not replayed.

The disposable fixture no longer mutates the simulator trust store. Only the
qualification bundle compiles `Qualification/FixtureTrust.swift`. After its one
launch, the fixture supplies two bounded public DER anchors through qualification
arguments. No app-container lookup/copy is needed to provision those public data. For localhost443 server-trust challenges only, the test delegate uses
Apple's SecTrust SSL hostname/chain validation with the fixture anchor exclusively
and network fetching off. It must first reject the same chain for another hostname
and an unrelated anchor. Other hosts/auth methods retain default WebKit handling.
Missing anchors, invalid chains and repeated failed challenges are refused.

This is a test-only provisioning replacement, not production certificate pinning,
a TLS bypass, login or relaxed application origin/bridge admission. CI also builds
the normal app without that extra source and checks its symbols exclude the helper.
No root certificate is installed in system/device trust. Both one-day public anchors,
private signing files, server and disposable simulator stay within the
existing job/deletion bounds; nothing is uploaded. The native receipt must include
the positive chain and both negative trust results before PCM playback acceptance.
This fixture no longer qualifies system-trust-store installation; normal shared
SSO/account and physical device trust remain separate acceptance requirements.

Apple contracts: [custom trust anchors](https://developer.apple.com/documentation/security/configuring-a-trust)
and [WebKit authentication challenge handling](https://developer.apple.com/documentation/webkit/wknavigationdelegate/webview(_:didreceive:completionhandler:)).


Source339c266 passed28Swift cases, both actual iOS SDK builds and the normal-app
helper-absence check. RuntimeCI42391234480 installed the app in27.74seconds but
then timed out reading its data container in30seconds before launch. Shutdown
and delete were unconfirmed; the server stopped. There was no TLS/audio runtime
receipt. The current fixture removes that unnecessary prelaunch read/copy: its
single launch carries only two bounded public DER anchors as base64 arguments,
parsed only by the qualification source. Keys never leave disposable runner state;
production bundles still compile no fixture helper. All old outcomes stay retained.


Source68c7fca passed28Swift cases (zero failures,1.689s), both actual SDK app
builds and normal-app helper absence inCI42391956558; all five general checks
passed inCI42391956382. Runtime installed in51.62seconds, but the one launch
observation timed out at90seconds. No page request, stage read or application log
established launch, TLS checks or new Beacon playback. Termination was unconfirmed;
shutdown/delete/server stop were acknowledged. Cause remains unknown; do not
classify this as a TLS or audio failure, retry the terminal launch or advertise
native capability. PR731 remains open pending actual new output qualification.
The earlier successful PCM baseline does not cover this new Beacon driver/probe.


## Prepared closed Beacon bridge

The existing `audio` handler now accepts only the closed Beacon commands from the
same exact main-frame origin/page/document checks as PCM. Beacon UUIDs are canonical
and separate from PCM IDs. One controller admits at most128distinct starts and
never reuses an accepted ID, including across document changes. A duplicate/new
begin cannot replace another current owner. Explicit Stop, page navigation,
WebContent loss and foreground exit close the exact Beacon owner without replay.
Speech and Beacon continue to use independent holders in the existing shared
session; no extra login, provider, observer or entitlement is introduced.

A frame is at most4096UTF8 bytes, six strict finite non-boolean numeric vectors,
normalized active weights, valid frequencies/ramp/depth/level and an exact safe
integer absolute uptime deadline in milliseconds. It is converted to seconds
without manufacturing a new lifetime. The native driver enforces its20second
maximum and stale fade. Gain commands cannot include or renew a deadline. Replies
bind to the accepted owner and document; played events still require actual device
callbacks. The existing adapter is reused unchanged; normal capability is false.

Three additional Foundation tests exercise independent identities/closed fields,
absolute expiry/numeric refusal and deadline-free gain. The actual simulator
fixture additionally requires JS Beacon played/stale-quiet/fresh/duck-quiet events,
PCM completion while Beacon remains selected, and a native refusal of a previously
closed ID. This is a new required journey, not a claim that it has passed. The old
srcdoc iframe probe remains non-proof of frame-handler admission; actual receiving
frame/capture qualification and independent boundary review remain required.


Source6bdcd10 passed31Swift tests (zero failures,1.642s), both SDK apps and normal
helper absence inCI42393067284. Runtime launched in16.06s, served the actual HTTPS
Conversation, then expired its20second page-readiness loop at16:52:59 before the
page finished loading at16:53:03. No Beacon probe or JS audio journey ran. Native
termination/shutdown/delete and fixture server stop were acknowledged. This is a
measured fixture-readiness failure, not a Beacon playback failure or pass.

The fixture now has a40second absolute monotonic page-readiness deadline, checked
again on late JS evaluation completion without renewal. The outer terminal-receipt
observation is65seconds, allowing that cold page load plus the existing12second
native probe and bounded JS/PCM journey. The job remains capped at ten minutes;
there is one launch and no timeout relaunch. Retirement qualification now waits
for the native closed receipt before testing the same ID's refusal. Normal app
behavior and audio/provider/TTL/lifetime bounds remain unchanged.


## Current source qualification —2c021f1

CI42393667849 passed31Swift cases (zero failures,1.609s), both actual SDK app
builds and normal-helper absence. Actual iPhone16/iOS26.2 runtime passed: native
Beacon34560played frames with stalequiet/fresh/duckquiet/restored/host-foreground
closure; exact valid TLS chain plus wrong-host/alien-anchor refusals; unchanged
system trust; JS Beacon played/quiet/played/quiet; retired-ID refusal after native
close; PCM28800played bytes with Beacon retained and pause/resume/flush; outside
facade absent. Terminate/shutdown/delete/server cleanup were acknowledged. No
microphone, provider, observer or human original was used. GeneralCI42393667873
passed all five applicable checks. Earlier failed/unconfirmed outcomes stay above.

The source still needs independent native-message-boundary review before merge.
Normal capability remains off; actual OS background, physical hearing/music/routes,
capture/duplex compatibility, shared login/account, signing and phone delivery are
not established by this synthetic runtime. The old srcdoc non-reservation result
still does not prove actual frame-handler admission. The full original goal stays
open; this record does not grant production selection or peer activation.
