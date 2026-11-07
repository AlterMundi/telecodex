iOS foreground capture uses the bundled native-capture.js wrapper and exact UUID/
document reply handler before getUserMedia. Session ownership never grants mic
permission; keep selected HTTPS main-frame microphone prompt and camera refusal.
Freeze duplex/spoken session mode while holders remain; output idle cannot release
an active capture. Stop WebKit's actual microphone before exact-holder release;
unknown stop/deactivation stays held, no refresh retry. Late permissions stop their
old tracks. Native capabilities remain off until actual duplex/account/device
acceptance. Preserve the shared Android adapter and hosted recorder/provider paths.

iOS preparation is in clients/ios. Keep the Android-compatible verified PCM span
contract, one serial owner, bounded queue/source metadata and nonrenewable lifetime
across flush. Submission is not playback; the prepared audio driver fences stale owner/epoch
callbacks and stops outside Apple completion threads. Its host must supply owned
AVAudioSession activation/release and close on lifecycle/route/interruption. Swift package tests are not an installable app,
WKWebView/shared-login, AVAudioSession, simulator or physical iPhone acceptance.
Reuse the hosted Conversation/grants; no new login/editor/server or provider work.
The iOS spoken-session host closes both prepared/active owners on background or
selected interruptions/routes; it never resumes them automatically. Keep unknown
session releases blocked until explicit exact-owner recovery. The spoken-output
policy is not duplex capture/background entitlement. macOS tests and iOS SDK
compilation do not establish simulator/device audio or shared-login acceptance.
The prepared iOS WebView embeds the existing hosted Conversation, accepting only
exact-origin/main-frame app commands with per-document IDs. Reuse app-bundled
native-audio.js; never fetch executable adapter source or create another login.
The synthetic simulator injection/iframe/PCM journey passedCI42380042745; native
capabilities still stay off pending real duplex/capture/account/device acceptance.
Navigation, process loss and foreground exit close sound/capture without replay.
The simulator app uses the exact existing adapter bytes/hash. Keep QUALIFICATION
code and its separate bundle ID out of normal builds. Its disposable loopback TLS
fixture may trust a one-day CA only in its own new simulator; never weaken the
production TLS policy or existing device trust. No capture/provider/human-original
fixture, automatic timeout relaunch or permanent simulator/server/cache is allowed.

Native Beacon preparation reuses the existing Sonic score/explicit activation.
Keep six normalized phase-continuous sine carriers, bounded latest frame/level,
20second maximum observation validity and nonrenewable two-hour owner lifetime.
Level changes do not refresh observations. Share one native audio-focus request
with independent speech/Beacon holders; bind quiet/focus callbacks to their owner
and applied control. Foreground/noisy/Stop cancels pending starts, without new
background grants. Android API35 WebView/AudioTrack and separate-UID focus qualification passed
PR726 preparation065691d. APK0.8 now advertises the native renderer; qualify the
exact release before delivery. Emulator success is not physical phone/hearing
acceptance. Older APKs keep browser audio without a native-failure fallback.

Public Live usage observations are page-local/bounded8, using trusted server
usage/finalization receipts. Only explicit voice_usage_confirmed and valid final
seconds permit a voice-only estimate; missing final usage is unconfirmed, never
zero or a wall-clock bill. Keep the dated published rate, excluded backend/tools/
ASR/TTS costs and lack of invoice/project-credit proof visible. No diagnostic
callback may break voice cleanup. See public GPT-Live guide.

Android0.7 Public Live output uses the advertised webrtcPcm bridge and existing
single native PCM owner, reserved before microphone/provider creation. Preserve
the zero-output Web Audio graph,600second maximum,300second legacy default,
flush/epoch retirement and immediate native close on Hold. No HTML fallback after
native selection. Node/JVM/desktop Chromium checks do not establish physical
WebView/AudioTrack focus/background/hearing acceptance. See native-clients guide.

Public native context sync coalesces retained trusted deltas inside the existing
finite visit:4096bytes or128events, at least10seconds between owned syncs.
Use the existing native continuation preparation; never invent provider token
usage or a checkpoint. Stop scheduling on Close; retain actor/admission ownership
through actual late context I/O. Unknown syncs stay closing without automatic
Close retry. Native work remains independent. See public GPT-Live context guide.

Native voice older-worker returns use advertised result_pages=v1 and the existing
owner-scoped trace-results/trace-result readers. Keep eight results per page,
one older page per foreground poll, at most five flat instruction cursors, and
exact current canonical revision/association rechecks before append. Legacy
readers retain their prior behavior; no work or uncertain-append replay.

Voice Studio close releases the panel and audio immediately; request at most one
owned cancellation and ignore late responses. Preserve unknown synthesis outcomes
without retry. Render HTTP allows45seconds; other preview writes retain20seconds.

Avatar background suspension uses both document visibility and Android's existing
nativecapturebackground event. Public Live Hold must silence its local audio
immediately while keeping the bounded provider finalization receiver alive.
This does not stop independent native work or enable background capture.

The source preview adapter is mounted at avatars/voice/previews with existing
owner/CSRF, voice admission, actor lease and shared TTS capacity. Unknown results
are status-only; never regenerate them. Explicit Prepare/Generate/Play/Stop/Retire
remain separate from Save/Use and from production deployment or paid acceptance.
The source voice studio extends the existing workshop account document and quotas,
not the visual model/design schema. Save and Use are separate. A new public Live
logical start may select a revision; physical reconnect/replacement freezes its
prior voice choice. Legacy clients keep default delivery. See
`docs/interaction-voice-studio.md`; source/browser qualification is not receiving
selection or paid audition/hearing acceptance. Do not deploy unfinished auditions
or change per-pair voice/provider defaults while preparing the studio.

Public Live grouped work supports up to32 raw originals from one sealed window,
within the existing600second visit,64KiB language and192KiB native document
bounds. Keep the legacy interpreted delegate at eight sources. A capacity refusal
retains the window and originals without truncation, second ASR or work retry.

# Interaction development boundaries

Prepared public Live context uses the existing native semantic producer, through
PublicLiveContextHistory and capability foreground_public_live_transcripts=v1.
Its input/output transcript fragments are approximate source data, not canonical
human ASR originals, complete utterances, new work or hearing. Keep speaker,
provider timestamps and exact source hashes. Compact journal blocks keep flat
speaker/time/UTF8 spans and one frozen range/hash reference, avoiding a full
envelope per subword delta. Retain the exact batch intent before native sync so
lost receipts followed by new deltas cannot regroup or duplicate old language.
Metadata lives in typed language
text that older projection readers preserve; do not add discardable document
fields. Cursor reconciliation is idempotent native source retention, never
permission to repeat uncertain media/ASR/work/provider writes. The source lifecycle mounts capability/actor/lease checks, native producer
registration, explicit old-visit catch-up and complete developer startup seeds
before microphone/paid creation. Native work receives the exact retained producer
reference after source sync. In-flight interpretation leaves the projection open;
provider close is separate. This source is not receiving selection or real
semantic/hearing acceptance. Native builds use CI's Rust1.95;
verify the rustc executable actually invoked, not only cargo's version.

Known-created disconnected native GPT-Live visits may use the authenticated
explicit replacement review in the existing socket admission. Keep capture ended,
app voice exclusion and exact source/journal revision checks before replacement;
unknown creation POSTs remain held. Review retires only the local transport for
admission/late-result routing, never confirms provider close/usage or replays
originals, ASR, native work or unknown appends. Preserve the flat claim-before-
admission intent and unchanged storage bounds. See the explicit replacement
section of docs/interaction-public-gpt-live.md; real semantic/device acceptance
is separate. Do not fake a human review to clear a diagnostic's uncertain bill.

Web-only maintenance must use the serving voice-admission fence through
`scripts/interaction_web_maintenance.py` and its actual local health/config
binding. Keep the existing finite scoped operator and all idle/unsettled outcome
checks. Busy means no update, never interrupt the voice to force admission. An
older reader without the matching header supports only the separately recorded
observed-idle bootstrap, not exclusion. Do not replace the lock, replay an
uncertain update, stop native work or infer global Stop authority. See
`docs/interaction-fleet-updates.md#voice-admission-during-web-maintenance`.

The two exact public Live voice-brief templates use the always-run dashboard and
speech qualification. Their prompt changes do not repeat unchanged Rust/native
fixtures. Unknown template paths, mixed native/dependency edits and changed owning
jobs keep their existing broader checks. See tests/ci_matrix_host_scope.py.

Shared avatar workshop integration continues under the SAME full App goal.
Read docs/interaction-avatar-workshop.md and the pinned module provenance before
changing the adapter. Existing App authentication/CSRF own the account scope;
imports never select a human or grant. Edit/save and explicit active-avatar use
have separate generation-fenced pointers. Preserve previous versions and exact
lost-ACK outcomes. The original MIT validator/renderer is reused; Node runs only
on bounded save/import, never per frame. Editor/renderer and authenticated storage
journeys are implemented and synthetically qualified; receiving acceptance is
still open. Do not
activate peer avatars, voice/provider/default/heartbeat flags or publish school
media as a side effect. Fixed workshop cues are artistic choices, not emotion or
agent/microphone measurements. Full original voice/client/UX scope remains open.

Native public-voice renewal keeps work on its original native thread. Historical
returns need current original/result proofs and flat origin-to-destination claims;
never reuse an old provider delegation ID as a new visit's delegation, copy nested
transcript archives, or replay uncertain appends/ASR/work. Claim-before-intent
partial commits stay held. Historical discovery and return reads belong in the
owned result task, with the existing finite journal bounds and foreground cadence;
they must not block browser Close or install attention. Source/synthetic continuity
is separate from real semantic/physical renewal acceptance. See the native-results
renewal section in docs/interaction-public-gpt-live.md.

The native text CI job keeps its30minute bound. Its isolated native-return and
goal-control fixtures share the existing four-worker ceiling, with independent
temporary stores/peers, all original invocations/flags and every exit/log retained.
This replaces serial scheduling that exhausted the measured job deadline in
run42262198798; it does not skip tests, retry actions or change product behavior.

Current Avatar planning is the4November slice in docs/interaction-roadmap.md;
GPT-Live adaptation follows docs/interaction-gpt-live-practices.md. Keep the
diagnostic context/visit limits unchanged. Qwen as integrator was a rejected,
completed experiment, never pending migration work. Keep deliberate integrator
provider/model selection supported and independent of the Qwen voice foreground.
The integrating role is not permanently tied to Sol. Nicolas5November closes
the Qwen iteration and selects OpenAI now; outstanding Qwen device checks remain
pending in issues, not a gate. Evaluate native Codex WebRTC on the persistent
integrating thread and managed Responses before choosing custom delegation.
Native handoffs can be the intended integrating workflow; do not reject them to
preserve our architecture. GPT-Live needs its own continuous-audio/delegation
lifecycle, not the existing manual-turn adapter. Gemini remains deferred without
balance; do not call it. Receiving native WebRTC handoff/reconnect evidence and
its task-quality/context-cost limitations are recorded in the practices note.
Nicolas6ea37cd3,5November: keep the current Qwen/Gemini mode working while adding
OpenAI as a separately selectable mode. Freeze the selected engine for each voice
visit; do not replace defaults, mix engines during a recording or silently fall
back to another provider. Back-port a better OpenAI workflow only after comparison.

Interaction is an independent application: web now, native/badge clients later.
Human priority, 2026-11-01: implement native speech above the existing Sol
integrator, with sourced asynchronous delegation and integrating returns. See
`docs/interaction-voice-foreground.md`; the mounted browser pilot is opt-in,
not receiving/device acceptance. Retain the unfinished independent Android
native-audio client selected on2026-10-29. Source is `clients/android`;
read `docs/interaction-native-clients.md` for the current foreground pilot,
same shared login, native bridge/byte/focus boundaries, signing/build retention
and remaining progressive/background/Beacon/iOS work. Telegram is optional.
Do not equate packaged WebView, CI APK or bridge tests with phone acceptance.
Telegram is an optional session projection, never the application shell or a
required launch path. A child should be able to use only this app. Current UI
and Beacon sound delivery is owned by #213/#221 and docs/interaction-v02.md:
readable conversation first, machinery in the header inspector (initially closed),
with coordination pause/resume only through an explicitly provisioned human
operator/grant. Observation remains read-only without that binding; no default
funding or inference activation follows. Preserve
perceptible per-being ASCII expression and an audible opt-in sonic composition.
Do not equate renderer/mapping tests with the delivered visual or sonic experience.

Browser sign-in uses the existing shared authentication at auth.dmn.homes and
its remembered session. Follow the daimon-home service-integration guidance and
the maintained Cluster shared-login adapter. Do not add browser credential-file
uploads, a second pairing flow or another identity store. Companion enrollment
is a device API primitive, not the human browser onboarding path.

Human correction, 2026-10-19. Treat destination discovery as registry/observer eligibility only. It never enrolls direct work-topic messages into routing or changes ordinary native input, commentary, final answers, controls or file delivery. Direct native work does not consume the integrator worker quota. Retain the shared single-writer coordination needed when actual routed work reaches an existing native execution.

The optional public GPT-Live comparison uses a default-off WebRTC/Responses route,
not the native Codex protocol or manual PCM turn loop. Follow
`docs/interaction-public-gpt-live.md` for configuration, bounded trusted sources,
original-only retry, graceful close and unfinished integration/reconciliation.
Engine selection belongs in Under the hood; preserve current-provider defaults.
The optional native_backend_enabled flag exposes an ADDITIONAL explicit
GPT-Live+CompAII choice. Do not implement its rollout by replacing the configured
Responses backend/default. Native admission is server-checked before paid voice
or microphone permission and frozen for the visit; no managed-tool mixing or
silent fallback. Its backend_mode stays in the existing visit journal.
Synthetic checks are not public access or full CompAII acceptance.
The prepared public client-delegation bridge uses existing native foreground
originals/formulations, not transcript fragments or raw durability chunks as
completed human turns. The owner-only backend_mode=native prototype now connects
sealed audio, interpretation, native work, same-turn results and exact current
trace-associated worker deliveries. Missing trace revisions never authorize an
excerpt-based speech fallback. Default Responses
and other providers remain unchanged. It is not receiving/real voice acceptance;
returns across voice visits, complete older-trace pagination, native callbacks,
continuation and physical qualification remain unfinished.
A retained work intent only permits same-root status
reads after unknown outcomes; never resubmit it to repair an observation.
Prepared native result returns use exact current canonical revisions and native
turn/original-request association. Append intent, socket send, context-injection
ACK, task completion and human hearing are separate observations. Unknown
appends cannot be resent or skipped to repair speech; preserve their source cursor.
Prepared source-seal controls are a default-off raw PCM-prefix contract, not an
utterance-end signal. They reference confirmed existing originals and do not
choose a backend or start inference. When a work intent has a seal, use only its
exact acknowledged nonempty source window. Keep first-marker questions narrowed
until integrated, and never equate provider transcript offsets with local samples.
Native preparation has one finite owned task per visit. End/close fences new
interpretation/dispatch before cleanup waits for any in-flight outcome; it never
cancels independent native work. Reader capability is checked before paid voice
creation. Keep native client renewal explicit until qualified; no fallback.
Native result discovery likewise owns one task and checks that visit's stop
fence between reads/before append admission. Close must stay responsive while
that task is blocked. A recorded `not_sent` append is a known close-before-write
outcome, not retry authority; a transport uncertainty remains distinct. Preserve
the source and independent native work through finite cleanup.
The optional managed Responses conversation reader follows the documented
function-result/continuation lifecycle. Keep owner reads distinct from full DMK
memory and native work execution. Never replay a retained result/continuation
intent after an uncertain write, or infer completion from a socket send.

The current MVP is one web conversation supporting routed text and two-way voice. Telegram work topics use ordinary Telecodex. The Telegram entry adapter defaults off; enabling it requires an explicit human-selected entry and never authorizes interception of work topics. When explicitly selected with telegram_entry_projection, the entry topic is another text surface of the same integrating conversation, including app originals, integrator output and correlated worker returns. Preserve the existing native/source identities and Telegram uncertainty; do not backfill/replay work or silently enable it as a deployment/test convenience.

Worker relays carry the attributed current message, its durable correlation and necessary related correction deltas. Do not resend all recent work-topic inputs, repeat unrelated open requests or confuse an unfinished project with a message awaiting an answer. Preserve original authorship and chronology without impersonating the human.

Evaluate the whole user-visible flow: input received, destination/progress understandable, later directions incorporated, canonical answer returned once to the current requesting conversation. An observer summary, native ACK, coverage declaration or green isolated fixture is not proof of that experience. Check ordinary direct conversations alongside the web integration. Preserve uncertain effects; never replay work to repair a missing observation.

Read docs/interaction-roadmap.md for the complete seven-stage product horizon and current v0.3 continuous-voice plan before selecting the next Interaction task. Milestone9/#301 owns streaming voice plus responsive foreground and required attention coordination; milestone8/docs/interaction-v02.md retains unfinished base-experience obligations. Issues belong to their owning milestone, and each milestone description must link its maintained plan. The owning controller design is docs/interaction.md; the full Matrix parent remains AlterMundi/daimon-matrix#313. Historical completion records do not override the current repair requirements. Use existing repository qualification and supported receiving adoption; global Stop requires fresh explicit human consent. Async cancellation/cleanup changes must be exercised with the locked dashboard dependencies on CI's Python3.11; a receiving Python3.13 pass does not cover its cancellation behavior. Reuse the existing source-wakeup and mounted-close regressions, and an existing compatible environment when available.

A native session is not permanently attached to the web return surface. A later ordinary direct message selects its own topic for native presentation, including during an active routed turn. This is presentation selection from the existing ingress journal, never router admission or controller wrapping. A delayed earlier transcript must not reverse a later surface selection. Qualify switching surfaces in the same session, not only direct work in a different session.

## Beacon composition — human correction, 2026-10-22

The sonic target is the Beacon series of 40.4: idle F1; active adds F2; running
tools/subagents add F3; active goal adds F4; pending adds F7; blockage/error adds
F11. These are concurrent additive components, not one exclusive note per state.
Follow docs/sonic-expression.md. The earlier 65 Hz revision-2 mapping is rejected,
not an accepted target. Do not fix its high harmonics by inventing another palette.

F1 = 40.4 Hz; Fn = n × 40.4 Hz. Use only sine waves with gently pulsing
amplitude, no additional harmonic stacks or 18 Hz texture (human confirmation,
2026-10-22). Pending stays F7 and blockage/error stays F11.

Native delivery:async structured questions use the existing durable human-task
ledger/cards, with exact native source metadata. Their replies are ordinary human
input, never native RPC answers or approvals. Read human_task before interpreting
feedback; explicit dismissal/scoped withdrawal does not complete work. Duplicate
source events and recovery must not reopen closed cards. Regular RPC questions
keep their existing live broker and capture-version checks.

Durable human tasks and native questions have distinct read paths. The independent
client's For you section reuses the ledger and exact entry/routed-local-turn task
scope. A task:ID reply is untrusted context for ordinary text/voice, never a native
answer, human completion or approval. Inspect human_task before interpreting it;
use the existing scoped withdrawal only when obsolete. Explicit human Dismiss uses
the same task ledger and exact current entry/task revision, without completion,
native approval or work cancellation. Reply feedback remains ordinary conversation
context; its visible progress is not proof of task completion. Do not import later
direct tasks or create another task store, responder or arbitrary completion route.


Voice capture freezes observed question versions before permission/streaming.
`capture_questions` is an untrusted narrowing limit, intersected with the native
owned question snapshot; it cannot add scope or become an answer. Keep it frozen
through exact-original retry/recovery. Questions first observed later require a
later original or explicit controls. In explicit continuous mode, ASR arming is
passive preparation: freeze the particular utterance's observed question versions
at its first local acoustic marker, before confirmed speech can acquire the floor.
If that marker is unavailable, retain the earlier arm-time snapshot. Never widen
it at finalization, Send, retry or recovery. See docs/interaction-streaming.md.


## Pending navigation — human correction, 2026-10-27

Preserve the separation between canonical integrator **For you** and the header
carousel for pending internal-session items. For you has permanent explicit
access even when empty. The carousel rotates slowly and supports drag/click and
keyboard selection, pausing while the human interacts or reduced motion applies.
Do not populate an all-session carousel with only canonical routed cards and
claim full coverage. Reuse owner-scoped pending ledgers/responders, current
revisions and real destinations; direct-topic ingress remains direct. Opening a
pending is never approval, completion, cancellation or input replay. See #61 and
the pending-navigation section of docs/interaction-v02.md; full roadmap #301.

## Routing ownership — human correction, 2026-10-27

One implementation owner per distinct task. Preserve the exact human original;
do not treat an integrator commentary as an encoded per-worker task scope. The
current dispatch schema has no such field. Multi-target routing requires clearly
distinct clauses addressed to those workers. A scope correction is an integrator
selection, not a request for several workers to arbitrate ownership. Preserve
existing checkpoints and inspect actual recipient acceptance. A bounded
`session_request` consultation does not transfer the original request or its
return responsibility; do not mark unfinished work complete to simulate transfer.
See docs/interaction-routing-ownership.md for the verified gap and qualification.

## Dedicated voice mode — human correction, 2026-10-28

Continuous voice is an avatar-entered fullscreen local Conversation view. It
preserves the ordinary composer and its draft/reply/files, temporarily silences
internal tracks without overwriting Mute/Solo, and leaves recoverable unfinished
recordings on exit. Meaningful internal results/questions need bounded,
revision-aware spoken offers through existing events; progress is not completion.
Do not replace the normal app or inject all observer history into every turn.
See docs/interaction-roadmap.md and docs/interaction-turn-taking.md for actual
implementation status; configuration alone is not receiving qualification.


Human clarification, 2026-10-28: multi-step work keeps its integrating return
responsibility until the required successor/receiving steps finish. A worker's
completed helper result does not complete that whole workflow. Keep canonical
ownership with the integrator; do not start recursive helper/ownership chains.
Avatar voice mode surfaces meaningful internal results/questions through the
foreground Conversation despite internal-audio mute. The journal/event extension
and ordinary native returns are implemented; CompAII has selected-stack digital
spoken-return qualification. Follow docs/interaction-routing-ownership.md and the
receiving record for remaining device, hearing and useful-latency acceptance.
Source availability is not peer adoption or a new autonomous attention or publication grant.

Workflow implementation #405 extends the existing instruction/result journal with
bounded `workflow_*` integrating controller operations, committed-result successor
progression and source-only worker reads. See docs/interaction.md. Preserve the original human authority, exact
request/result revisions, internal audience and final integrating responsibility.
One helper completion leaves remaining steps unfinished; only current complete
coverage plus the integrator checkpoint can finish the workflow. Presentation
includes bounded result/question attention descriptors. The #410 native adapter
is experimental and disabled by default (`interaction.native_workflow_returns`).
Preserve its intent/ACK/uncertainty and exact source publication fences; never replay
an uncertain internal return or manufacture a user input. Detached ACK recovery
uses a distinct exact native observation and the existing heartbeat; never rebind the
old human collector. Exact physical-turn collector replacement transfers retained
return-source bindings, including stale/unknown negative fences, without another
native input or ACK. Attachment bookkeeping text is not human speech; preserve
real accepted human execution publication and later originals. Keep owned regular
rollout/physical-turn boundaries, partial source uncertainty and the frozen
callback source-set publication fence. Native ACK does not prove
incorporation. `automatic_native_return` reports actual loaded configuration, separately from
retained per-envelope native return phases; neither is a completion or hearing
claim. Keep each receiver feature off until its actual wrapper qualification.
CompAII has qualified and adopted workflow/ordinary returns and useful digital
Conversation speech; this does not establish peer adoption or physical hearing.
Do not substitute fabricated human input, recursive helper chains, continuous
model polling or an extra coordinator for the supported return path.

Known-unsent workflow returns may resume through the existing maintenance heartbeat
only with the adapter enabled and unchanged current source/authority. Keep the
same intent ID, at most3 attempts (2/5-second delays), frozen attempt correlation
and compatible-writer rollback. Never retry uncertain, submitting, rejected or
acknowledged returns; never reuse an earlier local writer's publication grant.

Real-provider workflow fixtures must inspect retained entry model/provider/effort;
changing defaults leaves seeded session preferences unchanged. Make a task-owned
empty paginated context durable through existing thread/name/set before cold
resume. Preserve actual input/ACK history and distinguish fixture rejection from
Core submission. Current receiving evidence and remaining acceptance live in the
existing Shared Resources Interaction record; do not rerun completed qualifiers.

Ordinary canonical routed result/question attention is implemented under #426 with
`interaction.native_routed_returns` default off. It reuses the native return
transport beside canonical source rows; never invent workflows or human inputs
for ordinary returns. Keep helper/direct/progress exclusions, exact source and
original binding, source-set publication fences and shared finite retry bounds.
Use `attention_source` for bounded source reads and `attention_reported` only
for actual incorporation. Neither operation answers a question, approves work,
proves hearing or completes the human task. See docs/interaction.md for the
current development/qualification state and compatible-writer rollback.


CI scope follows the owning implementation: browser voice/timing and Python
preparation retain the always-run dashboard/speech suites, not unchanged Rust
optional-ASR builds. The optional implementation lives in `src/transcribe.rs` and
uses `AttachmentTranscript` from `src/models.rs`; generic `src/voice.rs` source
proof/envelope changes retain text/native qualification without rebuilding the
unchanged optional recognizer. Native source/test, compiler/dependency and relevant workflow
changes retain their native checks. In particular timing_ui.js is exercised by
the dashboard job. Reuse exact unchanged native source qualification when shipping
a web-only delta; do not turn a filename containing voice into native evidence.

Qualify an exact native SDK pin with the existing manually dispatched
`native-sdk-qualification.yml` workflow. Record the native SHA, run and outcome
with the caller's qualification. Dispatch when the pin or its native qualification
contract changes; preserve a currently running exact-pin run and reuse its valid
result for later App-only commits. App changes still run their own affected CI.
Do not declare native qualification complete from the caller's checks alone.


Enabling native_workflow_returns/native_routed_returns changes the dashboard's
receiving-profile fingerprint even when only the listener is being deployed.
Before adoption, exercise every configured receiving constructor and reconcile
Alerts/menu, Traffic and device bindings independently under the same verified
actor/bot/credentials/scope. Preserve all attempts, receipts, incidents/outbox,
menu selected/pending state and historical grants. Never clear a journal or
silently re-enroll devices to bypass a mismatch. See docs/interaction-companion.md
and the maintained Shared Resources Interaction receiving record. Loaded binary,
health and served assets alone do not establish authenticated app continuity.


Python-only Matrix host adapter changes use the dedicated `matrix-host` CI
profile: the existing SDK adapter/legacy suite plus both ordinary and funded
canonical HTTP journeys against the compiled native consumer. The profile accepts
only its four explicit host/test paths and documentation. Rust, dependencies,
unknown files or changed full-text qualification keep their owning full checks.
Changes to the profile itself exercise that job; the actual embedded scope
program is regression-tested. Reuse unchanged native qualification instead of
repeating unrelated voice, lifecycle and multi-worker fixtures for host Python.

## Shared application release coverage — Nicolas,2026-11-06

Ordinary common-app updates include every registered living receiver by default,
using Shared Resources and each actual receiving maintenance route. Preserve
receiver-selected settings, local source improvements, credentials, native work,
originals and compatible readers. Keep exact per-receiver installed/served receipts
and pending failures; publication or one author-body deployment is not completion.
A software update does not enable experimental Avatar/provider billing elsewhere.
Use docs/interaction-fleet-updates.md; no new model attention or global Stop.

Native package-operator Python changes use the always-run process-boundary unit
suite in matrix-execution. Only its three exact script/test paths are excluded
from unchanged Rust/audio builds; source/dependency/unknown-file changes keep
normal qualification. The embedded scope program is regression-tested. Actual
isolated old-to-new package qualification remains required separately; mocks do
not prove receiving adoption. This moves an existing check, never skips it.

The shared avatar wardrobe remains the pinned Ani/Oliva editor, adapted through
the existing account API and a closed same-origin parent bridge. Keep models,
designs and immutable revisions distinct. Saving/editing never activates; render
the candidate before a generation-conditional use and preserve the previous art
on failure. Initial account loading keeps editor inputs inert. Revision lists
carry summaries, not all designs. Artwork choices never assert mic/agent state.
See docs/interaction-avatar-workshop.md; source/browser fixtures are not real
Home-cookie, receiver, child or device acceptance.

Read-only history fixtures must close an admitted voice socket before changing
the synthetic owner binding. A live binding change correctly revokes the voice
admission; accepting that race in a fixture would weaken the real ownership check.

A composed public-native voice receiver must load the qualified Provider
source-window delegation method and native foreground_live_window=journal-input/v1
capability before paid creation. New Avatar handoffs use only the input-speaker
Live journal window, bound atomically to exact retained acoustic originals; no
per-clip file ASR or fallback. Provider fragment timing remains approximate,
not verified utterance alignment. Context fragments alone never authorize work;
the existing trusted delegation/source seal still owns admission. Constructor
health and synthetic fixtures do not establish real voice/hearing acceptance.
Keep a window-aware native writer after these delegation proofs are retained.


OpenAI-only Conversation is an explicit optional speech + named native ASR
profile. Freeze it with each original before microphone permission; no legacy
Qwen/Gemini preview or fallback. Keep profiled originals on a compatible native
reader during web rollback (local recovery v5 refuses older clients). Reuse
canonical part authentication, shared speech capacity/media quotas and the
existing one-attempt OpenAI transcription adapter; no second work/session store.
See docs/interaction-speech.md. Configuration is not receiving/hearing acceptance.

An explicitly selected `voice_services_default="openai"` also makes omitted audio
API selections and new client Conversation/Avatar selections OpenAI. Native
`voice.default_transcription_profile="openai"` selects new Telegram ASR. New
Conversation clients explicitly freeze their selected default; classic unprofiled
captures retain the legacy route across updates, including unfinished local audio. Existing originals retain their original route,
including legacy completed-only recovery. Explicit previous-provider captures
use the named `previous` route; changing a default never changes an active capture.
Same-actor device/Alerts/menu/Traffic requalification remains required when the
native receiving profile changes. Native work/provider identity is unchanged.


Live microphone transcription is the sole recognition for that capture. New
OpenAI and previous-provider captures freeze `openai-live` / `previous-live`;
these named profiles only read the same live session's completed receipt. They
have no recognizer, credential or file fallback. Missing completion keeps the
original held; do not verify/correct it with another ASR. Explicit Avatar
recovery uses the original's frozen completed-only reader, never the base file
recognizer to fill a missing live result. File-only Telegram and
historical originals retain their original profiles and outcomes. Recovery v6
freezes live-only selection; older readers must refuse it. Qualify exact native
receipt ingestion and Home/device admission. Previous-provider native Avatar requires the loaded listener to advertise
`previous-live` before opening a provider; qualify that actual profile in native
fixtures as well as mocks. Public Avatar's independently imported live-window
path requires its own source-bound live integration; do not claim that updating
ordinary recordings completes that path.

New voice routes must be qualified through the actual shared Home/device access
path, not only an administrative test cookie. Home and enrolled devices share
companion.permitted; add only the exact intended action and retain monitor,
foreign actor, CSRF and unknown-path refusals. A provider socket smoke test does
not establish browser grant admission. No grant reenrollment follows from an
ordinary route correction.
