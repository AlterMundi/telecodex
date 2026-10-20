# Daimon Interaction v0.2 — coherent single-chat experience

Status: executable plan, amended after the human-requested Astra high constructive adversarial review on 2026-10-19. Implementation and acceptance remain pending; the review inspected source and did not execute native race experiments. Parent: AlterMundi/daimon-matrix#313; coordination: Daimon Interaction Project 14. Build progressively toward the final local product, reuse native state and existing adapters, and use Git for history. No throwaway staging architecture.

## Outcome and scope

One beautiful, understandable web conversation supports text and two-way voice while multiple meaningful native sessions progress. The person can discuss without dispatch, direct work, correct it, answer pending requests, manage selected goals and recover communication without visiting target topics. The interface exposes a small animated ASCII companion, clear truthful activity, and read-only machinery beneath it. Session Telegram topics show attributed integrator exchanges alongside CLI without becoming router ingress. Useful outputs include authorized generated artifacts, not false upload claims.

This is the single-body product stage following immutable first-functional v0.1.0. Cross-body Matrix routing, native Android/iOS apps, Bluetooth/physical badge qualification, continuous live video and automatic publication remain later tracks. Keep them in the parent roadmap and preserve compatible contracts. Existing selected visual references and explicitly requested observer interviews are qualified here; no new hardware capture or publisher attention is activated. Context renewal is qualified through supported native compaction/identity continuity; a new autonomous renewal framework is not a prerequisite unless a real failure requires it.

## Human-selected rules

- Explicit human start/pause/resume/clear instructions execute without asking the same permission twice. An assistant-proposed goal lifecycle change states the target/action/reason and waits for human confirmation. Confirmation binds that proposal/current goal. Pause/clear are not turn interruption or rollback. Each global Stop still requires fresh specific human consent.
- Ordinary direct Telegram work-topic messages stay ordinary native ingress, outside routing/quota/controller wrapping. Discovery is not enrollment. New Telegram visibility projects existing events; it does not re-admit them, broaden audiences or repeat execution.
- Pending requests lead with one short concrete action, its brief purpose and how to answer. Ordinary text/voice answers should reach the selected live request without duplicate entry. Ambiguity gets conversational clarification. Agent-issued tasks that become unnecessary are withdrawn with a reason/evidence, not called human-completed. Expired native answer channels cannot be fabricated.
- Automate all meaningful tests possible. Use inexpensive real workers for necessary semantic/effect checks. Ask the human only for a genuinely non-automatable observation, after preparing the whole journey. No physical capture is implicit in a deployment or readiness check.
- Preserve existing credentials, native bindings, accepted/uncertain work, live goals, raw canonical events and immutable release tags. Successful source audio is deleted after durable transcript verification; failed/uncertain audio and TTS caches retain their actual bounded policy. No new raw-media archive.

## Existing basis and corrections

The accepted v0.1 text/voice round trip, existing 21-case qualification protocol, source-linked observer, canonical event/return journal, native writer coordination, scoped request broker, device grants and browser graph are the starting point. Do not rebuild them behind another queue or daemon. Human acceptance covered one actual heard voice return; automated/synthetic/native/human evidence remain separate.

Native CLI/daemon 0.163.0 hashes match. Installed schema and a read-only daemon probe establish external goal get/set/clear and execution state reads. Native source inspection explains further behavior, but is not exact-runtime mutation qualification. Telecodex #211 is merged: explicit completed-goal resume preserves accounting and actual exhausted budget is checked. Local 12 goal journeys, Clippy and 314 unit tests passed; receiving adoption is still pending. #212 filters valid controller footer metadata at the shared presentation boundary, preserving canonical originals. Neither is a claim that the general integrator control adapter exists.

Detailed basis: docs/interaction-qualification.md and docs/integrator-controls.md in PR #209; docs/native-goals.md in current master; docs/human-requests.md. Existing concerns remain owned in #208 (stage/acceptance), #210 (goal controls), #61 (pending usability), #108 (topic catalog/lifecycle), and #203 (supported attributed native cross-session delivery).

## Delivery slices and dependencies

1. **Conversation surface and ASCII companion.** Build on the existing standalone conversation client and supported Homemaker appearance consumer. Default to a readable text/voice stream and one obvious voice action. Collapse diagnostics/graph/history below it; native questions and confirmed control proposals remain in the main interaction. Keep graph history and read-only detail accessible, including original inputs and exact routes. Add an operator-selected simple/advanced presentation using existing actor/grant checks; hiding controls is not permission. This is child-friendly visual design, not a claim of new child accounts or tested child usability.

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

## Implementation checkpoint — 2026-10-19

- Plan/protocol review merged as #217; shared visible-footer correction merged
  as #212. Receiving adoption remains batched with the qualified product.
- The installed-native two-client experiment reproduced all three stale goal
  mutation cases without inference or work-session mutation. #210 requires native
  generation/definition preconditions on set and clear; see integrator-controls.md.
- A live scoped terminal fallback for older native tool sets is implemented in
  the #203 branch. It reuses the same controller handlers and current execution,
  including a routed steer into an already-running worker. The synthetic process
  journey passes routing, dependency/return, immutable retry, binding/role/token
  denial, expiry and ordinary direct ingress independence; Clippy passes.
  This is development evidence, not receiving or real-model acceptance.
- UI/companion, speech pacing, native control preconditions/transactions, task
  withdrawal, projections/lifecycle and artifact delivery remain in progress or
  pending in their owning issues. The finished-version real-session campaign
  has not started.

### Conversation shell development checkpoint — 2026-10-20

The standalone client now groups observation controls and routing/history panes
under one initially collapsed **Under the hood** panel. Live requests remain in
the main interaction area. The shell uses explicit grid rows for history, text,
voice, playback and diagnostics, preserving the existing DOM/API contracts and
Homemaker roles. The existing foreground refresh timer uses two seconds in
conversation mode and ten seconds for the session dashboard; this adds no poller.

Development evidence: existing conversation/request/microphone/dashboard UI
checks pass. Actual Chromium with synthetic native messages was visually checked
at 1100×850 and 390×844; the diagnostic panel opens, retains refresh/monitor
controls, and a 390×460 viewport remains scrollable without horizontal overflow.
These checks are not a real-session campaign, actual Android keyboard test or
receiving adoption. Companion gestures, truthful phase indicators and the rest of
#213 remain unfinished; the issue stays in progress.
