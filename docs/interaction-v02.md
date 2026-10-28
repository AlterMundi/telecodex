# Daimon Interaction v0.2 — coherent single-chat experience

Full horizon and next selected delivery: [maintained roadmap](interaction-roadmap.md).
This document owns the remaining v0.2 obligations; continuous voice is milestone9.

Status: the human accepted the current independent app and integrating channel as a solid, reasonably reliable and versatile functional base on 2026-10-26. The immutable tag is `interaction-functional-baseline-2026-10-26`. Preserve that working experience while completing the remaining milestone 8 / Project 14 / #208 requirements. The human rejected Qwen as the conversational/integrating LLM. All owned conversations and receiving defaults now select gpt-6.1-sol / openai / medium, with same-thread native verification; Qwen API TTS and separate STT experiments remain independent. Do not restore the earlier Qwen LLM choice from PR 288 / #285. The header identifies the selected daimon and authenticated human on two lines; remaining #213/#221 refinements stay in scope. General functional acceptance does not invent specific physical or aesthetic judgments. Parent: AlterMundi/daimon-matrix#313.

## Outcome and scope

One beautiful, understandable web conversation supports text and two-way voice while multiple meaningful native sessions progress. The person can discuss without dispatch, direct work, correct it, answer pending requests, manage selected goals and recover communication without visiting target topics. The interface exposes a small animated ASCII companion, clear truthful activity, and read-only machinery beneath it. Session Telegram topics show attributed integrator exchanges alongside CLI without becoming router ingress. Useful outputs include authorized generated artifacts, not false upload claims.

This is the single-body product stage following immutable first-functional v0.1.0. Cross-body Matrix routing, native Android/iOS apps, Bluetooth/physical badge qualification, continuous live video and automatic publication remain later tracks. Keep them in the parent roadmap and preserve compatible contracts. Existing selected visual references and explicitly requested observer interviews are qualified here; no new hardware capture or publisher attention is activated. Context renewal is qualified through supported native compaction/identity continuity; a new autonomous renewal framework is not a prerequisite unless a real failure requires it.

## Human-selected rules

- Explicit human start/pause/resume/clear instructions execute without asking the same permission twice. An assistant-proposed goal lifecycle change states the target/action/reason and waits for human confirmation. Confirmation binds that proposal/current goal. Pause/clear are not turn interruption or rollback. Each global Stop still requires fresh specific human consent.
- Ordinary direct Telegram work-topic messages stay ordinary native ingress, outside routing/quota/controller wrapping. Discovery is not enrollment. New Telegram visibility projects existing events; it does not re-admit them, broaden audiences or repeat execution.
- Pending requests lead with one short concrete action, its brief purpose and how to answer. Ordinary text/voice answers should reach the selected live request without duplicate entry. Ambiguity gets conversational clarification. Agent-issued tasks that become unnecessary are withdrawn with a reason/evidence, not called human-completed. Expired native answer channels cannot be fabricated.
- Automate all meaningful tests possible. Use inexpensive real workers for necessary semantic/effect checks. Ask the human only for a genuinely non-automatable observation, after preparing the whole journey. No physical capture is implicit in a deployment or readiness check.
- Preserve existing credentials, native bindings, accepted/uncertain work, live goals, raw canonical events and immutable release tags. Successful source audio is deleted after durable transcript verification; failed/uncertain audio and TTS caches retain their actual bounded policy. No new raw-media archive.

## Current UI and sound delivery

Interaction is an independent application: web now, native later, compatible
with a physical/Bluetooth badge. Telegram is an optional session projection, not
the app shell, entry requirement or identity experience. A child should be able
to interact with their daimon entirely inside this app. Reuse shared Matrix auth;
account provisioning gaps must remain explicit rather than becoming another login
or pairing system.

The next delivery must be assessed as one experience:

- An authored, beautiful composition centered on readable conversation and one
  obvious text/voice interaction. Useful intermediate communication and live
  questions remain understandable and reachable.
- Monitoring, raw worker output, test-session material, graph and operational
  detail live in the initially closed header inspector overlay. Inspection has
  readable space and its own scrolling, without tiny nested scroll panes. Do not hide the only copy of a
  useful worker answer merely because it lacks an integrator paraphrase.
- Simple presentation is the default; read-only exploration remains discoverable.
  Advanced presentation never grants otherwise unavailable permissions.
- A cared-for per-being ASCII presence, inspired by inspected Ani/Oliva/Nexus
  material with attribution; independent terminal, Telegram and app renderings.
  Local finite gestures, response/rest expressions and real phase changes must be
  perceptible in actual use, with reduced motion and static accessible alternatives.
- Compact, truthful transcription, routing, observer and parallel-work indicators.
  Visual polish must not counterfeit progress or conceal latency.
- A recognizable Beacon-derived hum/purr/rhythm and aggregate session chord,
  using the Beacon series of 40.4 and additive components: idle F1, active F2,
  running tools/subagents F3, active goal F4, pending F7, blockage/error F11.
  Follow the corrected composition in sonic-expression.md. Provide an obvious opt-in listening control, volume and mute;
  voice capture and spoken replies take precedence.

The current renderer work removes fixed-height/nested inspection scroll limits,
keeps history controls and full worker commentary under inspection, retains one
attributed recent update per session and all useful final answers in the chat,
and separates authored response poses
from background work indicators so concurrent work cannot suppress expression.
Beacon activation/status stays visible in the main app, including errors,
suspension, quiet arbitration and mute. Browser-generated signal and muted zero
output are measured separately from physical listening. Source Beacon revision 6 is the current accepted click-free phone baseline: additive 40.4 Hz sine components, independent proportional LFOs and continuous carriers with smooth transitions. Reply playback/preparation ducks to 18%; recording may fade to silence. The compact controls and functional application have human acceptance. The shipped Source portrait has 12 authored colored frames and state-driven speaking/ambient motion, with separate per-being packs and reduced motion. Preserve these deliveries. Remaining specific sonic, aesthetic, background-phone and real-session qualification stays explicit in #213/#221 and the current qualification protocol; automation alone does not establish human perception.

## Existing basis and corrections

The accepted v0.1 text/voice round trip, existing 21-case qualification protocol, source-linked observer, canonical event/return journal, native writer coordination, scoped request broker, device grants and browser graph are the starting point. Do not rebuild them behind another queue or daemon. Human acceptance covered one actual heard voice return; automated/synthetic/native/human evidence remain separate.

The selected-session goal adapter is implemented with retained explicit commands or single-use confirmed proposals, native goal preconditions and ordered follow-ups. See docs/integrator-controls.md and docs/native-goals.md for the current contract. Retained real native goal/artifact journeys include a failed campaign boundary followed by observation-only recovery of the original work; do not report the failed run as a pass or replay it. #212's presentation filter preserves canonical originals while removing valid controller metadata from visible replies. #241 still owns destination visibility for controls that produce no worker speech. Current receiving evidence and remaining acceptance are recorded in docs/interaction-qualification.md; implementation is not complete product acceptance.

Detailed basis: docs/interaction-qualification.md and docs/integrator-controls.md in PR #209; docs/native-goals.md in current master; docs/human-requests.md. Existing concerns remain owned in #208 (stage/acceptance), #210 (goal controls), #61 (pending usability), #108 (topic catalog/lifecycle), and #203 (supported attributed native cross-session delivery).

## Delivery slices and dependencies

1. **Conversation surface and ASCII companion.** Build on the existing standalone conversation client. This receiving Interaction app currently selects authored appearance; automatic Homemaker inheritance is disabled for it while Sessions retains its configured consumer. Future opt-in inheritance remains supported by configuration. Default to a readable text/voice stream and one obvious voice action. Keep diagnostics/graph/history in the initially closed header inspector; native questions and confirmed control proposals remain in the main interaction. Keep graph history and read-only detail accessible, including original inputs and exact routes. Add an operator-selected simple/advanced presentation using existing actor/grant checks; hiding controls is not permission. This is child-friendly visual design, not a claim of new child accounts or tested child usability.

   Adapt original glyph motifs/rendering from Ani and Oliva's MIT ASCII-Interfaces, as already integrated in Nexus, preserving attribution and license when copying code/art. Inspect actual sources and prototype first; do not import private research cases/media or a full map engine when a compact companion suffices. Use a stable lightweight glyph portrait with a restrained procedural setting, keeping chat/composer visible with the keyboard open. Honor existing style roles, light/dark choices and local fallback. Critical information remains readable HTML with accessible labels, never ASCII alone.

   Motion uses local finite macros (e.g. attentive, blink, curious, thinking, smile, speaking, rest). Operational phase comes from actual events: capture/transcription, observer refresh, routing/wait, worker activity, questions and playback. Several phases can coexist and their icons must not pretend a serial pipeline. Optional model choice of one gesture/rest is a bounded display hint during an ordinary response; no per-frame prompts or separate inference loop. Prefer an operation on the existing discoverable integrator controller; do not add required tool registration to old threads or overload trusted coverage/auth metadata. A missing hint has deterministic defaults. Unknown hint names have no effect. Bind hints to the response/source generation, with operational state taking precedence over cosmetic rest. Pause animation hidden/offscreen; reduced-motion uses readable static states.

2. **Voice pacing and attention.** Extend the existing source/revision-fenced sequential player with cancelable pauses between chunks and replies, preserving attribution and urgent live questions. Add a generation-owned waiting state to the existing player. Start capture, stop (including when no audio element is active), hide, disable voice and source/generation changes cancel any pending playback delay. Urgent live questions take queue priority with a visible answer opportunity; speech context remains advisory rather than forcing the next utterance to answer. Stopping speech never silently stops work. Keep text usable during ASR/TTS/provider failure, backlog, reconnect and out-of-order completion. Reuse existing scheduling/coalescing; add only gaps needed by measured cases.

3. **Selected controls and pending requests from the integrator.** Extend existing host owner socket/native goal adapter with typed inspect/set/edit/pause/resume/clear operations. Retain original human instruction, target/binding, selected goal definition, proposal confirmation where required, exact operation/outcome and ordered next input. Use the same operation from text and voice, not literal slash text sent to a model, fake human updates or an arbitrary shell RPC tunnel. Shared server get/check/set has no native expected-goal-revision precondition. Run the concurrent native mutation experiment early, before exposing mutation: if stale operations can affect a changed goal, require a supported native precondition for both set and clear, or refuse that unsafe operation. A wrapper lock is not protection from native model/external-client updates. No indefinite auto-resume or usage reset.

   Reuse the request broker for selected-session pending inspection, live answers and scoped withdrawal; keep human tasks distinct from native live questions and approvals. UI/voice presentation should be concise while detail and uncertain receipts remain inspectable. Ordinary spontaneous discussion should not manufacture pending tasks. Bind answers to actually available request versions and keep actual human authorship. #203 provides a supported attributed same-body backend route for warm/native sessions where persisted tool registration limits the existing adapter; qualification must not quietly restrict the product to newly created test threads.

4. **Telegram projections and visible lifecycle.** Publish a selected session's incoming integrator transcript/text, useful progress and canonical answers once in its own existing Telegram topic, while retaining the correlated integrator return. Use concise source labels, no footer/transport JSON, existing audience/receipt limits and one canonical event plus destination publication identity shared by direct/projected sinks. Revisions update that publication. Deduplication cannot guarantee exactly-once Telegram delivery after an ambiguous send acknowledgment; retain uncertainty without automatic resend. A projection is never ingress, execution or a new request. A failed projection does not replay work or block the primary return. Preserve ordinary direct interaction and surface switching in the SAME native session.

   Extend the existing metadata catalog/activity observer. Suggested Telegram decoration: active goal marker versus executing-without-active-goal marker; ordinary passive topics stay undecorated. An active goal may be waiting. Retain canonical human title/icon/binding; decoration must not create rename loops. Provide compact counts and an inspectable inventory, including currently passive sessions with real conversation activity in configurable X days (7 initially). Polls/reads/renames do not count as conversation. Expose unknown/stale observation rather than confidently idle; preserve explicit paused/blocked states in detail. No second Telegram consumer or model wakeup service.

5. **Authorized artifacts and continuity.** Close the existing routed adapter's text-only return gap with scoped retained artifact references from actual native outputs; never arbitrary model-supplied host paths or public uploads. Reuse direct attachment handling/storage and actor/request audience where possible. A file can remain available from its source session and appear once in the requesting conversation without duplicating work. Register retained artifact identity from the actual output turn before cleanup, binding immutable hash/bytes and audience. Validate logical ownership, physical source provenance, media policy, integrity, expiry, reconnect and denied access; supported renewal must not require the old physical thread to remain current. Show honest unavailable/expired output and retain canonical text. Qualify native compaction/continuity, selected image evidence and exact/pinned interview behavior using existing supported paths. File types unsupported by a selected client need a useful authorized download/access path, not a false delivery claim.

6. **Integrated acceptance and receiving delivery.** Execute the 21-case protocol plus avatar/control/projection/artifact extensions across the actual supported adapters. Use deterministic fixtures for races/faults/quotas; a few cheap real native workers for incorporated corrections, semantic context and returned effects; actual browser journeys for mobile layout, interaction, permissions, recovery and audio lifecycle. Reuse earlier valid evidence and re-run only invalidated behavior. Native/browser capture can use synthetic selected stimuli without unrequested physical hardware. A static screenshot and a worker completion declaration do not certify the journey.

   Qualification report records each case/evidence class, exact source/config/model, original/result identities, observed effect, latency segments and unresolved limits. For required in-scope behavior, not-run or unavailable is not a pass or a reason to close the milestone. Keep subjective child appeal and phone sound judgment separate from browser/automation claims; request a short human observation only if truly necessary after all automatable work. Adopt the qualified native/web release with existing writer-preserving rollout, keeping config/grants/native work and independent Pi services. Do not execute a completed one-use adoption script. Verify the actual receiving journey and ordinary direct topics. No global Stop, new cross-body activation or hidden resource reset.

## Order and issue organization

Resolve warm-session invocation, credential/grant coverage and the native race experiment first. Use #208 as this milestone's coordinating acceptance issue, with implementation in the owning slices instead of duplicating its old broad body into every issue. Independent UI/audio work can progress while backend controls are prepared, but final acceptance uses one integrated source. Reuse #210/#61/#108/#203; add focused UI/avatar, voice-pacing, Telegram-projection and artifact-return issues. Put all selected issues in a v0.2 milestone and Project 14, with explicit dependencies and next action. Matrix #313 stays the wider roadmap; later cross-body/mobile/video/publication work remains visible outside this milestone.

Deliver fixes #211/#212 as part of the qualified release; do not redeploy after every small isolated commit. Preserve v0.1 tags and keep one receiving release/rollback record. Review this coherent plan once using the human-requested Astra high agent; incorporate substantive findings, not recursive reviewer rounds.

## Review decisions incorporated

The requested Astra high review was constructive and source-based. Its findings
are implementation requirements below, not claims that native race tests passed.

- **Warm sessions (#203).** Support both already-open integrators and workers,
  busy and idle, without replacing their histories. Persisted tools cannot be
  assumed to gain new operations. Use a supported attributable current-turn
  invocation/fallback, retaining turn binding and nonblocking return. A
  completion-only manifest cannot resolve a question blocking that same turn.
  Cosmetic companion hints are optional where unavailable.
- **Grants (#210/#61/#213/#216).** Document which operations the existing human
  conversation credential can perform through the integrator and which require
  a narrowly extended grant. Standalone currently denies direct request decisions;
  a conversational native-question answer is not an approval. Simple/advanced
  is presentation only. Test allowed and denied goals, approvals and artifacts;
  do not turn all existing devices into owner clients.
- **Control transaction (#210).** Retain original human text, interpreted action
  arguments, actual actor, logical/native target, goal generation/definition and
  outcome under one operation identity. Assistant proposals bind their human
  confirmation once; cancellation, replacement or relevant state change invalidates
  them, ordinary usage-counter increments do not. Explicit human commands use
  the same transaction without another approval. Split “yes, then ask X” into
  ordered child actions under the retained original; neither duplicate dispatch
  of the whole sentence nor loss of X is acceptable. Test mutation between read
  and write and failure between replacement clear/set. Retain partial/unknown
  outcomes without replay or accounting reset.
- **Pending withdrawal (#61).** Add supported scoped integrator withdrawal of an
  obsolete agent-issued task, retaining the issuing task/author and actual
  withdrawal author, reason and evidence. It must work while the issuer is still
  running and reject changed native bindings; do not wait for a successful
  completion manifest. It cannot withdraw approvals or settle uncertain effects.
- **Secondary publication (#215).** Share canonical publication identity with
  ordinary direct presentation when the human switches surfaces. Inject an
  accepted Telegram send followed by lost acknowledgment. The primary integrator
  return remains independent; uncertainty never re-executes work.
- **Speech (#214).** Exercise capture/hide/disable/source changes during gaps,
  TTS failure followed by another reply, progress arriving after final, and
  reconnect without historical autoplay. Reuse player generation/coalescing.
- **Artifacts (#216).** Separate retained artifact registration from destination
  delivery. Reuse existing validation, with independent authorized references for
  browser and Telegram. Test browser download, partial multifile return,
  uncertain Telegram acknowledgment, expiry/revocation and logical renewal.
  This requires no general-purpose arbitrary-path file service.
- **Companion (#213).** Ani/Oliva sources provide motifs/scenery, not an existing
  expressive portrait. Design a small original companion with attribution.
  Accept at most a bounded finite cosmetic hint per response/turn generation;
  actual operational HTML remains independent and defaults deterministic.

## Qualification sequence and adapter coverage

Development fixtures qualify individual changes while implementing. Once the
integrated version is complete, run the revised full campaign with real native
sessions, as requested by Nicolás; do not substitute mock success for that
campaign or ask the human to accept a partially built version. Automate real
session setup and assertions where possible. Reserve human observation for
intent, subjective appearance/hearing and actual daily use.

The campaign in [interaction-qualification.md](interaction-qualification.md) is
the acceptance protocol for this milestone. Q18 presentation/grants and Q21
artifact return are required, with no “planned” escape hatch. A small adapter
matrix records old/new integrator, old/new worker, busy/idle, standalone/Telegram,
and authorized/read-only cases. Cover these in a few paired journeys instead of
an exhaustive Cartesian product. One combined journey overlaps two existing
sessions, switches one to direct input, answers a live question, rejects a stale
proposal, applies an explicit goal change, receives an artifact and reconnects.
Fault injection surrounds the same journey; real worker effects and actual
receiving behavior are inspected separately from synthetic invariants.

Keep the milestone, owning issues, Project 14 status/dependencies and campaign
report aligned with implementation, merged revisions, deployment and evidence.
Record failures and missing observations without closing their capability. Link
public summaries to source/CI; keep credentials and private session evidence out
of GitHub. Required automated and real-session cases gate completion. Subjective
child delight and days of use stay explicitly unobserved until actually assessed.

## Milestone exit

- One normal web conversation performs local discussion, observer-grounded existing/new routing and clarification while meaningful tasks overlap, receive later steering and return intermediate/final communication in order understandable to the person.
- All selected existing-session adapters work; no artificial two-topic title-only routing, no interception of ordinary direct conversations and no requirement to manually enter a target topic for ordinary controls or answers.
- Main text/voice UI is visually reviewed on desktop and narrow phone/keyboard layouts, honors appearance/motion preferences, and has a delightful accessible ASCII companion whose animations require no continuous inference. Read-only machinery remains available but starts collapsed.
- Human-directed controls and confirmed assistant proposals have attributable matched effects; goals, activity and pending action remain distinct. Obsolete human tasks do not keep demanding attention; uncertain answers/effects are not silently settled or replayed.
- Attributed Telegram projections and scoped artifact return work without audience expansion, duplicate effects or technical metadata leakage. Audio retention remains bounded with durable text.
- Required campaign cases pass on exact integrated source and selected receiver, with private evidence retained appropriately and practical limits stated honestly. Deployment is observed, not inferred from CI. Record any genuinely required human acceptance still pending; do not claim it happened.

## Tracking

Milestone: [v0.2](https://github.com/AlterMundi/telecodex/milestone/8).

| Slice | Owning issue |
| --- | --- |
| ui | [#213](https://github.com/AlterMundi/telecodex/issues/213) |
| voice | [#214](https://github.com/AlterMundi/telecodex/issues/214) |
| projection | [#215](https://github.com/AlterMundi/telecodex/issues/215) |
| artifacts | [#216](https://github.com/AlterMundi/telecodex/issues/216) |
| stage | [#208](https://github.com/AlterMundi/telecodex/issues/208) |
| goals | [#210](https://github.com/AlterMundi/telecodex/issues/210) |
| pending | [#61](https://github.com/AlterMundi/telecodex/issues/61) |
| lifecycle | [#108](https://github.com/AlterMundi/telecodex/issues/108) |
| native_delivery | [#203](https://github.com/AlterMundi/telecodex/issues/203) |

## Retained receiving checkpoint — 2026-10-21

The standalone web selects `d65ff7a`, including #243–#246. Populated desktop,
phone and keyboard-sized browser checks cover readable replies, collapsed
inspection, finite Source glyph/response transitions and reduced motion.
The receiver-selected expression pack retains independent client defaults;
[companion contracts](interaction-companion.md), [visual expression](interaction-avatar.md)
and the shared ASCII skill own that vocabulary. They do not imply every existing
terminal or Telegram client automatically renders an avatar.

Source hum revision 2 and its actual generated spectrum/mute/speech-priority
behavior have device-free evidence. The receiving app exposes activation, volume
and distinct quiet/suspended/error states. [Sonic expression](sonic-expression.md)
owns configuration and composition. Human recognition, comfort and visual appeal
remain unaccepted in #213/#221.

The listener now selects the exact CI-tested executable from #249/#250
(PR head `1b7ad96`, tested commit `fdd5aedd`). Three active collectors reattached
to the unchanged native daemon; observed goal definitions, states, budgets and
accounting were preserved. No global Stop or new native kickoff was used.
The current owner notice feed is available. The optional foreground client from
[Codex#7](https://github.com/compaii/codex/pull/7) is selected for new ordinary local
CLI launches, after full native CI and an actual TUI check with a synthetic owner
notice. Existing running clients do not hot-reload. See
[passive notices](interaction-notices.md) for source/client boundaries.

A real two-worker journey received ordinary direct input while routed work was
active in that same native execution. The direct input created no router
admission, its later correction changed the actual output, the other worker
finished independently, and one destination answer plus the canonical integrating
reply were retained. API observation of the existing reply and authorized file
verified its actual bytes/hash without replaying work. The verifier must follow
normalized shared-request membership, distinguish result IDs from delivery IDs
and page history; one representative request or first page is insufficient.

The [qualification protocol](interaction-qualification.md) retains the scope and
limits of earlier native, deterministic and human evidence. Complete only after
its explicit requirements are reconciled against the delivered version. Human
visual/hearing assessment and normal daily use remain open; source publication,
installation, observed transport and human perception are separate findings.

## Real-use composer feedback — 2026-10-25

The composer groups text, Send and an accessible microphone action in one block, with an accessible speaker toggle for spoken replies. Camera and gallery are separate image choices feeding the same immutable-image preview/point/upload path. Camera capture is user initiated through the device chooser; desktop/browser fallback may remain a file chooser. No live camera stream or automatic microphone start is added. Voice/draft/selection/retry ownership is unchanged. Beacon revision 6 keeps the accepted click-free 40.4 Hz sine composition and reply ducking; see sonic-expression.md and #221.

The real-use follow-up also requires stable composer coordinates across asynchronous conversation/observer updates, and a permanently visible recording/send icon. The compact player appears to the left only while needed; Send and recording retain their right-hand positions. Messages scroll separately. Image decoding falls back to browser Image/HTML canvas when ImageBitmap/OffscreenCanvas is unavailable or rejects the file. The selected image accompanies the next text or voice submission without a separate inclusion checkbox; a failed attachment remains visible rather than being silently dropped. The current backend accepts one selected image per original; multi-image messages remain a separate unimplemented increment, not a claimed fix.

While recording or retaining an unsent recording, the composer has one Send action owned by that recording. It cannot submit the retained text draft. The microphone is hidden during capture and a visible × discards the recording; finishing/discarding restores ordinary text Send without clearing the draft. Voice retries preserve the existing capture identity and uncertainty rules. The attachment affordance is an inline SVG paperclip with an accessible image label, independent of font glyph support.

The independent client moves the existing companion/status node into a compact fixed header beside Apps, with no duplicate renderer or separate presence card. The center conversation takes the remaining viewport and scrolls independently; the composer stays below. Current state labels must not resize the header or move Send.

Ordinary camera/gallery originals up to20MiB are accepted by the client. The client transparently fits both dimensions and actual encoded bytes to the existing immutable PNG transport bounds; it never rejects a normal photo merely because its first2048px PNG exceeds2MiB. Every fit is rendered from the decoded original, and preview/point/hash/dimensions refer to the final transmitted copy. No high-resolution original is uploaded or archived as a side effect.

Standalone Interaction can use `conversation_appearance = "authored"` to omit the automatic Homemaker stylesheet/script while Sessions continues to inherit its configured appearance. This is an explicit product choice; shared login and permissions remain unchanged. Its compact header, conversation area and composer remain separate, with recording status inside the composer and a paperclip-only attachment trigger.

With Voice on, standalone Interaction permits verified reply playback, continuation gaps and incoming-reply live updates while the document is hidden. Returning resumes a browser-paused segment after source validation, preserving its position. A human pause, Stop, loss of authorization or real page unload still stops automatic playback. Transient connection failures and offline transitions preserve opted-in Voice intent and already loaded audio; returning online revalidates a paused segment before resuming. Standalone playback exposes standard OS Media Session play/pause/stop controls where supported. These controls retain explicit human pause/Stop semantics. Mobile OS/browser suspension can prevent background execution; returning restores the existing opted-in intent, never a human-paused one. Physical phone background acceptance remains a separate check.

Presentation resolves fallback `t<ID>` labels from the current authorized owner's session title without changing routing selectors, source IDs, revisions or explicit route aliases. A renamed session needs no route recreation.

The default standalone composer is two compact rows: one text line above a single attachment/voice/send strip. Routine capture/transcription/playback phases remain in the header presence; the composer retains their accessible status text without a duplicate visible line or reserved blank height. Actionable errors remain readable inside the composer and may expand it upward. The paperclip opens an anchored Camera/Gallery sheet and selected-image preview; closed attachment controls take no extra conversation space.

Standalone chat startup preserves the authored paperclip and one-row text field. The Beacon toggle lives beside Apps in the fixed header; its volume and detailed sound status remain under the hood. The empty former sound row no longer reserves conversation space.

A selected image uses the same immutable attachment for the next text or voice submission. There is no separate image-with-voice switch; Voice on controls spoken replies. Successful consumption clears that selection.

The conversational caption uses a full-width presence row below the fixed header. The header shows `daimon: <configured name>` and `human: <authenticated name>` on two adjacent lines. The account name no longer sits under the inspector or occupies the full-width caption row with a connected label. `daimon_name` is escaped display configuration, not native identity or authority.

The presence caption keeps one fixed-height row. Overflow scrolls at18CSSpx/second with pauses and reverses smoothly; fitting text stays still. Reduced-motion preference keeps it static, with the full caption available as text/title.

Discarding a voice recording requires explicit browser confirmation. Declining keeps the captured recording; confirming deletes it without sending. The centered discard icon has a fixed44px hit target.


### Installed client and live updates

The independent conversation advertises a scoped web manifest with standalone
window display, app icons and the existing shared-auth start URL. Installation
adds no account or browser credential upload. There is no private offline cache;
conversation APIs and authentication remain network-only.

Conversation updates use an authenticated fetch/SSE connection. Linux inotify
watches the existing receiving SQLite/WAL and atomically replaced observer file;
no additional HTTP polling or message queue is introduced. Notifications only
invalidate the current read view. Reconnection catches up through existing
canonical cursors; it never replays a submission. Bursts and refreshes already
in flight are coalesced. A slow stream is reconnected with bounded backoff; proxy
buffering is disabled. Stream authorization is checked on admission and every
15 seconds, including current scope expiry. Other Sessions views retain their
existing refresh behavior. Voice-on/background intent remains independent.


### Recovering a failed voice original

A failed preparation retains the original audio UUID, digest and ingress order.
The owner control `input voice recover` accepts that exact UUID and current entry
binding. It claims only `preparation_unconfirmed` under the existing ingress lock;
already active/completed originals are observed without repeating work. The
optional `voice.completed_transcription_command` must be a completed-result reader,
never a transcription command. The maintained adapter's `--completed-only` mode
checks the source, route, helper and completed result digest without probing or
calling a provider. Missing or uncertain results remain unconfirmed. Normal
upload retries continue to observe existing captures without restarting ASR.

For operator recovery, inspect the adapter journal before doing anything. A job
with no intent and only its lock precedes the maintained adapter's provider call;
an explicitly requested recovery may finish that preparation once. A retained
provider intent/uncertain outcome cannot be retried this way. Once completed text
exists, recover the same original through owner control, not a fabricated text
input or direct database mutation. Commit precedes audio retirement and routing;
all original authorship, corrections and receiving grants remain applicable.

## Human-task attention in the conversation

`Read action` expands the selected action; `Collapse action` only folds its text.
Explicit human `Dismiss` cancels that selected pending human-task entry in the
existing ledger, with the current entry binding, exact revision and an audited,
idempotent receipt. It never marks the requested work done, answers a native
question, approves anything or stops a session. Direct-only tasks remain outside
the integrating conversation scope. A lost acknowledgment is read back without
automatically repeating the write. Existing Telegram /pending reads the same state.

`Reply in conversation` prepares ordinary text/voice context. Show the saved
original's real ingress progress beside the matching task revision: accepted
conversation feedback awaits source-session reassessment; uncertain delivery stays
uncertain. The reply does not complete the task. A changed action cannot inherit an
earlier reply badge. Dismissal and ordinary status refresh preserve drafts, image
selection and recording objects, including stale advisory reply context.

## Spoken-reply following

Follow starts enabled unless explicitly disabled. With Voice on and verified
playback running, it first positions at the answer and then advances a reading
window using real media time/duration within the certified text segment. Segment
offsets remain exact; positioning within a segment is approximate, not word-level
alignment or a claim derived from generated tokens. Continuations do not return
to the beginning. The existing canonical reader reuses the already-validated
segment and loads missing prefixes only when needed. Visible Update from cards
share that exact source/revision text rather than scrolling a hidden duplicate.

Pause, Voice off, Stop, hidden pages and the open inspector suspend movement.
Resuming keeps the current segment and position; replaced sources stop playback
and following. Unknown media duration never invents progress. Wheel/touch reading,
scrollbar dragging and reading keys release Follow in the current view; checking
it again resumes at the current position. Only explicit checkbox changes persist
the preference across reloads. Reduced motion uses instant
positioning. Ordinary incoming replies do not pull the reader to the bottom while
spoken following owns the reading window.

Telegram entry originals appear as human bubbles in the independent chat through
the existing read-only ingress endpoint, without dispatch or replay. New Telegram
capture timestamps are retained atomically in the shared ingress-order ledger.
Older missing timestamps stay explicitly unknown instead of being backfilled
with refresh time. Follow resolves the visible Update from card for the exact
spoken progress source; it never scrolls to its hidden diagnostic duplicate.

Spoken output uses one accessible speaker icon with on/off/paused states instead
of a visible checkbox and status label. The compact player retains accessible
status announcements without helper copy below its centered playback icons.
Attachment choices and recording use authored stroke vectors, avoiding
platform-dependent emoji appearance. Input and speech state machines are unchanged.

Shared-login and interaction-device grants permit read-only Telegram originals
and their existing trace/image routes, with the same receiving human and entry
binding as the app inputs. A denied optional Telegram observation must not clear
the conversation, draft or recording; it is not evidence that the whole browser
session expired. Authentication expiry still follows the existing reconnect path.

### Chat message time labels

Human originals, integrating replies and visible session updates show a small
bottom-right day and 24-hour time label, such as `mon 23:40`. Use retained receive
or observation time in the device local timezone, matching the current English
UI. The semantic time element retains the full ISO timestamp and a readable full
local date tooltip. Missing or invalid timestamps stay hidden; refresh/status
updates never fabricate a send time. Assessment of coherent application-wide
internationalization is tracked separately in #295; this slice introduces no
language selector or partial translated UI.

Human correction (2026-10-26): Follow remains enabled during manual scrolling. A held pointer/touch owns the reading viewport; release resumes the spoken position. Wheel and keyboard scrolling have a short settle interval. Only the explicit Follow checkbox changes its preference. Background Voice must preserve opted-in intent across transient network errors, with OS media controls where supported; phone screen-lock playback remains a separate actual-device acceptance item.

### Keyboard message entry

Enter submits the existing composer form; Shift+Enter keeps the textarea's normal
newline behavior. IME composition, composition keycode229 and modifier shortcuts
do not submit. Repeated keydown and a disabled Send do not create another input.
The same submit handler retains draft, attachment, pending voice and uncertain
delivery behavior; there is no separate keyboard dispatch path.


### Pending navigation — human correction, 2026-10-27

Keep two surfaces separate: **For you** contains the integrating conversation's
canonical questions, durable actions and proposals; the header carousel is for
pending items from internal owned sessions. For you needs permanent explicit
access, including an understandable empty state. It must not depend on a card
happening to be visible during refresh.

The internal-session carousel should show one concise session/item line, rotate
slowly, support horizontal drag and keyboard navigation, and open the selected
current action. Pause rotation on hover/focus/drag, hidden state, an open detail
and reduced motion. Retain a stable list entry. Reuse existing ledgers and native
responders; reading never approves, completes, cancels or resends anything.
Direct-topic ingress stays direct. Canonical For you scope must never be labelled
all sessions: global attention needs its own owner-scoped read/navigation contract.

PR377 (source800ae05f81edda02330d6817f2aced457800f4c3) delivered a permanent For you
header control and readable panel over the same retained card nodes. Existing
request/conversation state tests pass. A synthetic Chromium component verifies
open/close, reply-to-composer, preserved draft and explicit empty access. The full
shipped HTML/JS with synthetic read-only HTTP verifies Escape/draft preservation,
question/task reply preparation and scrollable dialogs at320/390/1040px. A found
desktop header overflow was corrected; inspected screenshots and updated metrics
show contained44px header access and no horizontal overflow. These fixtures send
no native decisions, models or capture. Global session data/navigation, carousel
drag/focus and the exact reported Review-item diagnosis remain unfinished.
PR377 CI40587818579 passed; its web-only release was receiving-verified against
local/public assets and the successor web process, preserving the existing native
listener and daemon. This is digital verification, not new device acceptance.

PR379 extends `requests internal list CURSOR` into a separate owner-scoped
internal-session carousel. Its bounded existing-ledger query excludes the entry,
rechecks current native/session/turn ownership and retains source dates and
revisions. Internal detail, question decisions and receipts reuse the existing
native responder and transactional decision ledger; they never widen canonical
For you or ordinary capture-question context. Internal durable task detail and
explicit Dismiss reuse the same task ledger without human completion or work
cancellation. The internal task view offers no misleading reply-to-integrator
shortcut for direct-session work.

HTTP and device access use the existing authenticated provider and CSRF contract.
Conversation-only devices may answer current questions; approval and cancellation
remain unavailable, with approvals directing the human to Sessions. The carousel
rotates every6.5seconds when idle, supports drag/arrows and pauses on interaction,
open detail, hidden state and reduced motion. Its count describes loaded items,
not a complete global total. Existing card readers retain at most200 requests and
200 actions per selected source; overflow is visible, pagination does not advance
past skipped cards, and unsent drafts are never evicted to make room. Closed cards
can be dismissed locally to load more. Metadata refresh reuses the conversation's
existing refresh path; it adds no separate network poller or pending store.

This is a draft implementation, not receiving adoption. Qualification includes
owner-scope/ledger tests, synthetic UI checks and an actual authenticated HTTP to
synthetic-native journey for a direct-session question, denied device actions,
CSRF, one decision/receipt and revocation. A passing CI run and integrated browser
qualification must be recorded separately; this paragraph does not assert either.
Owning task #61; stage4 attention and native repair remain in plan #301/milestone9.

## Original generic files — #380

The Files composer increment carries unchanged PDFs and other original bytes
through the retained conversation and native/routed attachment path. This remains
milestone8 base-experience work. See [the contract and qualification boundary](conversation-files.md).
A PNG-only selector or an upload receipt does not close native file delivery.


### Background reply continuity

Voice-on in the standalone client remains opted in across hidden state and the
end of an individual reply. Tests cover a later reply arriving while the page is
still hidden, as well as continuation within one reply; explicit OS/manual Pause
and Stop keep their existing meaning. This does not prove phone background
acceptance or override browser/OS suspension.

The notification stream previously aborted on pagehide but restored only for a
BFCache pageshow. A non-BFCache return could leave the client marked active with
no subscription. Lifecycle and network recovery now replace that exact stream,
request a retained-view catch-up and fence obsolete callbacks by generation.
A single45-second deadline detects missing15-second server heartbeats, then uses
the existing1–10second reconnect backoff. Each mount retains at most one selected
subscription, one heartbeat/retry timer and64KiB of unfinished event text. Stop,
expired access and pagehide abort owned I/O and clear timers; no work is submitted
or replayed by reconnect. This is a reproduced client failure and correction,
not an established diagnosis of the human's particular Android interruption.
Physical testing must still cover two separate replies while another app is
foreground and after screen lock. For full OS-managed background playback beyond
web lifecycle guarantees, the native-mobile roadmap remains the owning track.
