# Integrator controls and recovery: harness study and delivery plan

Status: implementation and qualification in progress, 2026-10-20. Owner: [Telecodex #210](https://github.com/AlterMundi/telecodex/issues/210),
under [Daimon Interaction](https://github.com/AlterMundi/daimon-matrix/issues/313).
This document does not enable controls, delegation, recovery services or model wakeups.

Development checkpoint: `goal_inspect` reads the native definition revision,
execution state and receiver bookkeeping separately. Durable `goal_prepare`,
`goal_decide`, `goal_status` and `goal_execute` retain human originals, explicit
controls and single-use confirmed proposals. Conditional metadata writes run on
the existing selected writer. The context-aware activation path uses the same
continuation collector and native goal-state permit to start or steer useful
work with its current invocation context, preserving goal accounting. A retained
human command can outlive the model turn that interpreted it; its frozen target,
owner, incarnation, deadline, holds and newer human directions still constrain
each separately identified submission attempt. A proven unsent outcome may retry
the unchanged intent without a second human confirmation, retaining earlier
outcomes. Attempt fences prevent late retirement/receipts from changing a newer
attempt. Restart does not replay uncertain or partially applied work.

This is development source, not receiving adoption. The native dependency is
[compaii/codex #5](https://github.com/compaii/codex/pull/5); older receivers must
refuse unsupported conditional activation rather than stage a legacy goal or
insert a priming model turn. Canonical results from the linked goal collector
return to the selected integrating conversation, while a later direct input
still selects its own presentation surface. For activating controls, an exact frozen human follow-up travels in the same
conditional context admission: the native goal CAS succeeds first, then the
clause reaches the first useful or steered model step. The matched response
acknowledges both admissions without a separate priming turn or an extra
application race. Incorporation remains distinct. The process fixture uses synthetic
native peers. Metadata-only controls with a human follow-up use the same collector:
its matched CAS precedes the exact attributed clause through ordinary native
start/steer, preserving paused/absent goal state. A committed metadata receipt is
retained before follow-up submission; failure retains a partial outcome and never
repeats the mutation. Actual native qualification of this path, receiving adoption
and the finished real-session campaign remain required by #210.

Human-selected initial policy: explicit human instructions to start, pause,
resume or clear a selected goal execute without asking for the same permission
again. An assistant-proposed lifecycle change states target/action/reason and
waits for human confirmation. A confirmation authorizes that proposal, not a
different target or a later changed objective. This policy does not authorize
unattended goal reactivation. Goal controls remain distinct from interruption;
each global Stop still needs its own fresh explicit human request.

## Intended experience

The person can direct work, inspect obstacles, answer questions and manage a
selected session's goal from the integrating conversation. There should be no
routine need to visit the destination topic or terminal. Independent authorized
work can keep progressing while another task needs an answer. An actual external
obstacle cannot be guaranteed away; the product must bring the necessary decision
to the integrator instead of silently stalling or inventing progress.

Use a supported control adapter on the existing host/listener. A shell script
can be a client, but the useful capability is a typed operation with retained
identity and outcome, not the location of the process executing it. Sending
literal `/goal resume` to a model is not equivalent to invoking goal control.

## Evidence and limits of this inspection

- Installed CLI: `codex-cli 0.163.0`. The actual daemon release and CLI executable
  have identical SHA-256 `399ff69c431c066e40de318fba45c2b07f45a76b19939d247ee4828fc70c4215`.
- Generated the experimental JSON schema using that installed executable. It
  advertises the methods and fields below. Schema generation performs no inference.
- Connected read-only to the existing daemon through the maintained activity
  client's WebSocket proxy. `thread/goal/get` succeeded; the selected conversation's
  old goal was `complete`, while `thread/read` reported an active session.
  No goal mutation, turn submission, service restart or model call was made.
- An initial scratch client incorrectly treated the shared proxy as JSON lines
  and timed out at initialize. The maintained client succeeded. This is a probe
  mistake, not evidence of a product outage. Shared proxy transport needs its
  WebSocket handshake; private app-server stdio transport is different.
- Inspected the Codex source checkout at `296a388a2afd70f8686fc5ea0d140486eefdf808`,
  especially `ext/goal/src/{spec,tool,api,runtime}.rs`,
  `app-server/src/request_processors/thread_goal_processor.rs`,
  `app-server-protocol/src/protocol/v2/{thread,turn}.rs`, and
  `state/src/runtime/goals.rs` (all beneath `codex-rs/`). This source inspection
  explains behavior; it is not a claim that the checkout commit produced the
  installed release. Mutation semantics still need the selected-runtime tests below.
- Inspected Telecodex `src/main.rs`, `src/control_client.rs`, `src/codex/goals.rs`,
  `src/app/{goals,stopping,dialogue,conversation_requests}.rs`, the activity observer
  and existing goal/question/control fixtures. Current owner CLI has no general
  `goal` control verb. Telegram already has the lifecycle adapter we should extend.

Private probe receipts and schemas remain in the owning operation record; do
not copy private conversations, credentials or connection bindings into this plan.

## Capability inventory

| Need | Supported primitive | Exact distinction / current gap |
| --- | --- | --- |
| Inspect goal | `thread/goal/get` | Returns a goal or null; no inference. Status is separate from execution |
| Set/edit/reactivate | `thread/goal/set` | Fields include `threadId`, optional objective/status/tokenBudget/origin. Source applies runtime effects for a registered runtime; a stored unloaded thread still needs the supported loading/subscription path |
| Clear goal | `thread/goal/clear` | Removes goal state, not conversation history or completed effects. Not a turn interrupt |
| Observe lifecycle | `thread/goal/updated`, `thread/goal/cleared`, turn/status events | Subscribe/retain exact thread scope; a metadata ACK is not proof of a new working turn |
| Load existing session | `thread/resume` | Restores the native thread; different operation from reactivating its goal. Never clone or replace history as a repair shortcut |
| Start or steer input | `turn/start`, `turn/steer` | Steer requires `expectedTurnId`; input has stable correlation. Steering acceptance is not incorporation |
| Interrupt one turn | `turn/interrupt` | Requires thread and turn; matched completion and external effects remain separate |
| Recover an existing live collector | Existing Telecodex `attach-active` | Selected writer attachment; must not start a new turn or mutate its goal |
| Answer questions | Existing conversation-request broker and controller question operations | Uses the live original request/revision; restart can expire its channel. Saved question text is not a surviving responder |
| Inspect sessions | `thread/read`, `thread/list` and bridge catalog | Native loaded/executing state, logical conversation identity, Telegram lifecycle and goal state are different facts |
| Compact selected history | `thread/compact/start` advertised by installed schema | Capability availability is not automatic permission or a continuity guarantee; renewal/handoff is separate work |

Goal statuses in the installed schema are `active`, `paused`, `blocked`,
`usageLimited`, `budgetLimited`, and `complete`. Mutation origin is `user` or
`automatic`. The field documents provenance; it is not a standalone authorization
token and its presence does not prove that the operation was human-directed.
In inspected source, `user` additionally records an explicit goal instruction
in native history. It must not be used to disguise an autonomous model decision.

The model's `update_goal` tool exposes only complete, blocked and explicitly
requested pause. It cannot resume or change limit states. An external client has
a broader protocol, but moving that same prohibited agent decision to a queue
or another session does not establish authority to perform it. Human-directed
controls through the integrator and future supported delegated controls must
remain distinguishable.

The inspected baseline labels complete/budget-limited goals terminal for `/goal resume`
and suggests edit/new goal. [Correction #211](https://github.com/AlterMundi/telecodex/pull/211)
allows explicit resumption while preserving accounting and checking actual
budget headroom; it avoids pointless kickoff on exhausted objective edits.
The old restriction is bridge behavior, not proof that the native
set API has no active transition. Inspected native state logic keeps an exhausted
budget limited when asked to become active; setting active does not reset usage.
Do not clear/recreate a goal to evade accounting. Account availability is also
independent of goal metadata. Qualify the exact intended transitions before
changing the bridge policy.

Source runtime can also set `blocked` after turn errors, repeated empty responses
or execution unavailability. A status alone does not identify the cause or mean
that the model deliberately declared an impossible task. Preserve a reason when
native events establish it; otherwise report the cause as unknown.

## Smallest coherent implementation

Extend the existing owner control socket and integrator controller with one
shared selected-session control adapter. Reuse goal activation, native writer
coordination, pending-request brokerage, ingress journal and return renderer.
Do not introduce a generic shell-command queue, fake Telegram updates, terminal
keystroke injection, another native daemon or a second session database.

Proposed operations are inspect, goal set/edit/pause/resume/clear, selected-turn
interrupt, pending-request inspection/answer, and recovery inspection/attachment.
Introduce mutating operations in the delivery order below rather than exposing
an unrestricted RPC tunnel. Return the result to the requesting integrator and
make the same event inspectable in the destination's ordinary projection.

A control record retains the original human input, logical target/current native
binding, selected goal snapshot and operation, exact payload, execution outcome
and the requested next input. Proposed outcome categories distinguish retained
known-unsent, submitted, native-acknowledged, observed effect, rejected and uncertain.
Reuse existing retained operation machinery wherever it fits. Control content
never becomes another human message or a duplicate worker admission.

`Resume Field, then ask it X` is an ordered control plus a correlated input.
`Clear its goal, then discuss X` must say whether ordinary active execution is
allowed to finish or whether a separately requested interrupt is needed. The
controller must not infer that clear means stop. Do not clear goals merely to
allow ordinary discussion: direct messages can already steer active work.

The installed goal-set schema has no expected-goal-revision precondition. A
client read/check/write is not a native atomic compare-and-set. Reuse writer
coordination and reject stale selections, but test model/external-client races;
if atomic protection is required, add a narrow supported native precondition
rather than claiming the wrapper eliminates that race. A timed-out mutation
requires inspection: compatible current state alone may not prove which attempt
caused it. Preserve uncertainty instead of promising exactly-once external effects.

## Installed-native race evidence — 2026-10-19

A bounded two-client experiment on three newly created test threads used the
installed 0.163.0 shared native server, with zero inference calls and zero
work-session mutations. It reproduced all three stale-selection cases:

- Select goal A, independently clear/create B, then apply the old status mutation:
  B became complete.
- Select A, independently clear/create B, then apply the old clear: B was deleted.
- Select A, independently edit its definition/status, then apply the old status
  mutation: the changed definition was affected.

These are actual native metadata outcomes, not synthetic fixture behavior or
inferred source findings. Scratch goals were cleared; their history and the
private case receipt remain retained. No active work or global Stop was touched.
Consequently #210 requires native definition/generation preconditions on both set
and clear before exposing stale-selection-sensitive integrator mutations. A
client lock or immediate reread cannot meet that requirement. Replacement also
needs an honest partial outcome if clear succeeds but new-goal creation fails.

## v0.2 review requirements

The [reviewed implementation plan](interaction-v02.md) governs delivery. Resolve
warm integrator/worker invocation and the standalone credential/grant path before
exposing controls. Persisted tool registration and a completion-only manifest do
not provide live control to every existing thread. Add only supported scoped
operations, not a generic owner RPC accessible to all devices.

Use one attributable control transaction for explicit commands and assistant
proposals. Retain original human instruction separately from interpreted arguments,
proposal/confirmation identities, logical/native target, goal definition/generation,
action and outcome. Confirmation is single use and invalidated by cancellation,
replacement or relevant definition change, not ordinary accounting increments.
Preserve ordered child actions for “yes, then ask X” without duplicate admission.

Run the native mutation race experiment early. Interleave a native model/external
pause/change/clear between client read and set/clear, and fail replacement between
clear and set. If a stale mutation can affect a new goal, require a supported native
precondition on both mutations or refuse the unsafe operation. No wrapper-only
serialization claim is sufficient. Preserve partial/unknown operations and usage.

Scoped integrator task withdrawal must retain issuer, actual withdrawing actor,
reason and evidence and work during an active issuing turn. A changed binding,
expired question channel, approval or uncertain effect cannot be silently settled.
Qualification: Q07/Q18 and V02–V04, followed by the complete real-session campaign
on the finished version. The human does not need to manually enter target topics.

## Recovery choices

| Situation | Useful response from the integrator | Automation boundary |
| --- | --- | --- |
| Human explicitly selects resume/pause/clear/edit | Apply supported control, show native outcome and appropriate continuation | Implement now; human need not visit target topic |
| Human changes direction | Steer the active turn, or sequence an explicitly requested goal change and next input | No blanket clear/interrupt for ordinary discussion |
| A task needs judgment/input | Bring its pending question into the integrator; route the actual answer to the live request | Semantic interpretation must remain grounded in that human input; stale/ambiguous questions need clarification |
| Presentation/collector disconnected but native work is still running | Inspect retained output and matched writer; attach collector where supported | Recover observation without replaying work |
| Input is retained and definitely not submitted | Existing selected recovery/admission can continue the same original | Respect current task selection, budget and cancellation; no new identity to hide retries |
| Submission/control may have reached native | Inspect retained operation and actual native state | Never automatic resubmission solely because ACK was lost |
| Goal became blocked | Explain observed cause and remaining independent work; accept a directed recovery decision here | No perpetual auto-resume loop. Automatic reactivation needs supported delegation; current model tool does not provide it |
| Usage/budget/auth unavailable | Explain actual condition; preserve work and handle a valid human-directed change when possible | No invented quota reset, credential substitution or accounting reset |
| Session context must be renewed | Use supported source-linked handoff and stable logical identity when that stage is implemented | No silent replacement because a thread is busy or old |
| Approval or physical action is required | Surface it through the existing authorized request mechanism | Reading/relaying is not approval; hardware tasks retain their actual scope |

An event-driven dependency can mark a selected obstacle ready for reevaluation;
it does not itself authorize native activation. If future delegated recovery is
implemented, specify selected tasks, triggering evidence, permitted transitions,
attempt/cost bounds, expiry and revocation. Preserve explicit human pause, Stop
and goal budgets. Do not install that service as part of this study. A new global
Stop always requires its own fresh explicit human request.

## Delivery order and qualification

1. **Inspect and explain.** Give the integrator current goal/execution/pending and
   recovery facts for a selected target. Reuse authoritative bindings and expose
   unknown state. Coordinate compact Telegram lifecycle visibility with #108.
2. **Human-directed lifecycle controls.** Add the owner/control adapter and
   integrator operations over the existing Telegram goal implementation. Preserve
   retained intent and outcomes; support active and idle loaded/unloaded cases.
3. **Questions and ordered follow-ups.** Bring selected workers' actionable
   questions to the integrator, apply actual answers to their original responders,
   and sequence a requested goal transition with the following message. Coordinate
   native adapter limitations with #203 and the existing conversation-request API.
   Requests should lead with one short concrete action, its purpose and how to
   answer, with detail separately accessible. Associate relevant ordinary voice/text
   replies without requiring duplicate entry. Retire obsolete agent-issued tasks
   through existing scoped withdrawals with reason/evidence; do not call them
   human-completed or fabricate answers to expired channels. Coordinate #61.
4. **Recovery without work replay.** Consolidate retained-result inspection,
   collector attachment and known-unsent recovery into the same conversation.
   Report missing ACKs, terminal states and external obstacles usefully here.
5. **Continuity and supported delegation.** Evaluate renewal (#38) and specific
   native autonomy gaps from actual failed journeys. Extend the native contract
   only where the supported external controls and existing authorization cannot
   solve the observed case. No blanket autonomous goal reset/restart mechanism.

Extend existing `tests/goals_io.py`, control/question fixtures and selected
single-chat journeys. Deterministically cover the six goal states, no goal,
loaded/unloaded and busy/idle sessions, ambiguous names, stale goal/binding,
duplicate control IDs, dropped ACKs, restart before/after mutation, active-turn
completion races, pause/clear vs interrupt, budget exhaustion, expired questions,
and ordinary direct-topic independence. Check actual underlying operation count,
not just success text. A projection or inspect call must never start model work.

Then qualify the installed native lifecycle using disposable **test** threads
and controlled provider output, without changing real sessions' goals. Inspect
get/set/clear and automatic continuation behavior, especially runtime loading,
budget accounting and concurrent edits. Run a small real inexpensive-worker
journey only where needed to prove that work actually resumes and later input is
incorporated. Inspect existing receiving journals rather than repeating effects.
The human-facing acceptance is one integrating conversation controlling two work
sessions, resolving a question and a recoverable obstacle, and returning results
without visiting either topic. These mutation/journey tests have not yet run.

References: [official Goals guide](https://developers.openai.com/cookbook/examples/codex/using_goals_in_codex),
[official App Server lifecycle](https://learn.chatgpt.com/docs/app-server),
[whole-experience qualification](interaction-qualification.md),
[current goal control task](https://github.com/AlterMundi/telecodex/issues/210).

## Conditional controls development checkpoint — 2026-10-20

The stable integrator now retains goal interpretations and assistant proposals in
the existing journal database. Original human speech, interpreted payload, logical
incarnation/native target, native definition revision, authoring execution, capture
chronology, expiry, confirmation and outcome remain separate. A later captured
confirmation can select a proposal once; an earlier capture cannot authorize it.
A control-selected original cannot also receive an independent routing/creation/
question-answer plan. Later retained human controls supersede earlier ones for the
same target even if tools invoke them backwards.

The metadata execution slice supports pause, clear and edits of inactive goals,
using only the native conditional RPCs. Unknown internal/transport outcomes stay
uncertain and prevent automatic replay; a native invalid-request refusal stays
rejected. An existing receiving worker processes the control on its own subscribed
connection, so acknowledging pause/clear retires a staged legacy activation. Source
ownership, bindings, holds and chronology are checked again at the write boundary.
No ordinary topic input is intercepted and pause/clear do not interrupt work.

The initial controller journey passed with synthetic native peers before the owning
writer correction. That correction and its staged-activation regression are under
qualification in PR223. Active transitions (start/resume/replace or active edits),
ordered child admission, durable proposal presentation and receiving rollout still
need integration. They are refused before mutation in this slice; #210 remains open
and the full plan and real-session protocol retain those requirements.
