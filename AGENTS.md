# Interaction development boundaries

Interaction is an independent application: web now, native/badge clients later.
Telegram is an optional session projection, never the application shell or a
required launch path. A child should be able to use only this app. Current UI
and Beacon sound delivery is owned by #213/#221 and docs/interaction-v02.md:
readable conversation first, read-only machinery in the header inspector (initially closed),
perceptible per-being ASCII expression and an audible opt-in sonic composition.
Do not equate renderer/mapping tests with the delivered visual or sonic experience.

Browser sign-in uses the existing shared authentication at auth.dmn.homes and
its remembered session. Follow the daimon-home service-integration guidance and
the maintained Cluster shared-login adapter. Do not add browser credential-file
uploads, a second pairing flow or another identity store. Companion enrollment
is a device API primitive, not the human browser onboarding path.

Human correction, 2026-10-19. Treat destination discovery as registry/observer eligibility only. It never enrolls direct work-topic messages into routing or changes ordinary native input, commentary, final answers, controls or file delivery. Direct native work does not consume the integrator worker quota. Retain the shared single-writer coordination needed when actual routed work reaches an existing native execution.

The current MVP is one web conversation supporting routed text and two-way voice. Telegram work topics use ordinary Telecodex. The Telegram entry adapter defaults off; enabling it requires an explicit human-selected entry and never authorizes interception of work topics. When explicitly selected with telegram_entry_projection, the entry topic is another text surface of the same integrating conversation, including app originals, integrator output and correlated worker returns. Preserve the existing native/source identities and Telegram uncertainty; do not backfill/replay work or silently enable it as a deployment/test convenience.

Worker relays carry the attributed current message, its durable correlation and necessary related correction deltas. Do not resend all recent work-topic inputs, repeat unrelated open requests or confuse an unfinished project with a message awaiting an answer. Preserve original authorship and chronology without impersonating the human.

Evaluate the whole user-visible flow: input received, destination/progress understandable, later directions incorporated, canonical answer returned once to the current requesting conversation. An observer summary, native ACK, coverage declaration or green isolated fixture is not proof of that experience. Check ordinary direct conversations alongside the web integration. Preserve uncertain effects; never replay work to repair a missing observation.

Read docs/interaction-roadmap.md for the complete seven-stage product horizon and current v0.3 continuous-voice plan before selecting the next Interaction task. Milestone9/#301 owns streaming voice plus responsive foreground and required attention coordination; milestone8/docs/interaction-v02.md retains unfinished base-experience obligations. Issues belong to their owning milestone, and each milestone description must link its maintained plan. The owning controller design is docs/interaction.md; the full Matrix parent remains AlterMundi/daimon-matrix#313. Historical completion records do not override the current repair requirements. Use existing repository qualification and supported receiving adoption; global Stop requires fresh explicit human consent.

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
later original or explicit controls. See docs/interaction-streaming.md.


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
Avatar voice mode must eventually surface meaningful internal results/questions
through the foreground Conversation despite internal-audio mute. Follow the
required journal/event extension in docs/interaction-routing-ownership.md; it is
not yet implemented and creates no autonomous attention or publication grant.

Workflow implementation #405 extends the existing instruction/result journal with
bounded `workflow_*` integrating controller operations, committed-result successor
progression and source-only worker reads. See docs/interaction.md. Preserve the original human authority, exact
request/result revisions, internal audience and final integrating responsibility.
One helper completion leaves remaining steps unfinished; only current complete
coverage plus the integrator checkpoint can finish the workflow. Presentation
includes bounded result/question attention descriptors; automatic entry wakeup
and foreground speech are still unsupported (`automatic_native_return:false`).
Do not emulate that missing seam with fabricated human input, recursive helper
chains, continuous model polling or an extra coordinator.
