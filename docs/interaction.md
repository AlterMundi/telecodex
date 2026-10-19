# Integrating conversation controller

Owning plan: [Daimon Interaction](https://github.com/AlterMundi/daimon-matrix/issues/313).
Implementation: #109–#119. The controller runs inside the existing listener.
The single-body native controller and browser monitor have a qualified receiving
deployment, recorded in [#170](https://github.com/AlterMundi/telecodex/issues/170).
A deployment record establishes that receiver's selected source and observed
behavior; it does not enroll another body or certify every end-to-end scenario.

Current delivery is one body with multiple native work sessions. Cross-body
activation is deferred. Combined parallel voice acceptance is recorded in
[Integrators #4](https://github.com/AlterMundi/daimon-integrators/issues/4#issuecomment-6255807590):
physical button/Jabra input, transcription, native Alpha/Beta work, a mid-work
correction, an independent question/answer and reconnect/playback. Genuine human
routing feedback, subjective voice usability and the remaining attention
evaluation are separate from that automated acceptance.

## Using the integrating conversation

Open Conversation in the authenticated Sessions app, or use an operator-enrolled
Talk device. Text and committed voice clips enter the same retained conversation.
Talk starts a recording; a second press sends it. Holding the button discards the
current recording. Work continues while the device is disconnected.

Discuss normally, name the work you want to continue, or deliberately begin a
message with #Name. A reply to a returned result carries that session's context.
A new leading selector overrides that reply focus. If multiple requests fit,
clarify which work you mean; the controller does not infer a new task merely
because the intended correction is unclear. Existing context can keep progressing
while that decision is unresolved.

Later input reaches a selected running worker through native steering. Input
retention, native acceptance, declared incorporation and verified task behavior
remain different facts. Stopping spoken playback does not cancel the worker.
Use the selected work session's explicit controls to change its execution.

Expand **Routing monitor · your inputs and destinations** to inspect the retained
original text or voice transcript, actual destinations, exact forwarded originals
and correlated session results. Long text has read-more controls. A message that
has not been routed remains visible without an invented destination. Refreshing
this view never transcribes, dispatches, retries or renders speech.

For a selected desktop/device display, an operator can enroll a separate expiring
read-only credential and open ?view=monitor without Telegram. This reuses the
same renderer and gives no message, question-answer, audio or session-control
authority. See [standalone monitor setup](interaction-companion.md#standalone-read-only-routing-monitor).
The ordinary Talk credential does not grant inspection access.

The integrating conversation retains conversational continuity while independently
running native sessions retain their own work. Native input identity, request
relations, output identity and publication are distinct. An acknowledged steer
proves admission; a worker's coverage declaration identifies what it reports
addressing; behavior checks establish whether the requested change actually
occurred. Finishing a physical turn does not complete all application requests.

## Configuration and explicit selection

```toml
[interaction]
human_id = 100
chat_id = 100
topic_id = 1

[[interaction.targets]]
name = "Nexus"
topic_id = 7

[[interaction.targets]]
name = "Research"
topic_id = 8
```

Use actual existing owned bindings. The entry and targets cannot overlap the
bounded I/O profile. One selected shared native App Server remains the owner.
Names are human selectors; `#t7` selects the stable configured topic directly.
Duplicate names require explicit selection. A stable selector cannot alias a
different destination. Only deliberate top-level selectors route: quoted code,
quoted hashtags and incidental mentions remain ordinary content. An explicit
selector overrides inherited reply focus.

## Durable request state

`interaction_sources` retains immutable source identity, original text, selected
logical session IDs and exact return binding. Each authorized destination gets
one `interaction_instructions` record with a monotonic sequence. An existing
source cannot be expanded to another target plan, change its text or acquire a
different return audience. Explicit multi-target admission retains separate
per-target results under the original frozen selection.

`interaction_requests` and `interaction_request_links` distinguish new requests,
amendments and contextual information. Correcting A does not suppress unrelated
B, including when both share one native turn. Status reads do not create worker
instructions. Source messages and native items remain independently inspectable.

Transport states are queued, submitting, steering, accepted, uncertain, rejected,
retained unsent and cancelled. Native evidence links the actual local execution,
thread and turn. Restart preserves accepted evidence, retains known-unsent work
and marks in-flight submissions uncertain. Only known-unsent work can be selected
for recovery; uncertainty is never a reason to replay or create another session.

An instruction's transport state is not a task-completion flag. Canonical native
outputs are recorded separately. Worker coverage and correction-aware publication
use these retained records independently. Operational state stays in Telecodex;
DMK provides consolidation techniques rather than becoming the transient journal.

## Worker coverage

The result adapter recognizes an internal final metadata tail:

```text
Normal answer text
<daimon-result>{"requests":["request-id"],"covered":["instruction-id"],"completed":[],"deferred":[]}</daimon-result>
```

The human presentation strips this tail; the canonical native item retains the
original. `completed` is a separate declaration about selected addressed requests;
addressing a request or incorporating an instruction alone does not finish it.
This is an attributable worker declaration. The controller validates
identifiers against its admitted target/request records, then uses retained
return bindings; metadata cannot select an audience or destination. Missing or
invalid coverage remains unreported and cannot silently settle a correction.
The output reservation/transmission boundary must also recheck admitted
corrections before an unsent item is published.

## Listener routing and return

The native worker is explicitly told that an interaction controller is relaying
an authenticated human input. The packet keeps controller guidance separate from
the human-authored content and carries original author and source-message
provenance. Direct enrolled-topic input retains that direct source even though
it uses the same admission controller. Relay does not impersonate a direct human
intervention, and future integrator interpretations must remain separately
attributed. Canonical worker answers also retain their own authorship.

`#Name message` or `#t7 message` in the integrating entry selects an existing
owned native conversation. Replying to an original admitted message or returned
result inherits its request and target; an explicit selector overrides that
focus. Direct text in an enrolled worker topic uses the same controller. Plain
unselected entry text enters the native integrating conversation. Its controller
tool can inspect authorized observer cards and retained originals, propose an
existing destination or a justified new session, or ask for clarification.
Deterministic explicit/reply selection remains independent of that model.

For a correction, the integrator can inspect target_inputs to discover retained
request IDs and relations before dispatching an amendment. An unresolved parent
is a reason to clarify, never to silently downgrade the correction to a new
request. Model-written interpretation does not replace the original human text.
Specialized classifiers remain evaluation candidates rather than a selected
production dependency.

An ambiguous or unknown name retains the original source without dispatch and
asks for a stable selector. Replying with that selector resolves the original
input once. `/cancel` in reply withdraws pending routing. Reply bindings preserve
the actual requesting topic and human, including after restart.

The worker control channel opens before asynchronous preparation or presentation.
Inputs already waiting before native start enter its initial packet; later inputs
use `turn/steer`. Acceptance is journaled before subsequent canonical output is
processed. Unconfirmed acceptance is retained without automatic replay. An exact
native rejection can queue the same instruction for a subsequent native turn.

`#Name /interrupt` selects that listener-owned turn only. `/interaction` displays
transport state and instruction IDs. `/interaction cancel ID` withdraws only
known-unsent input; `/interaction retry ID` selects retained known-unsent input
for recovery. Other native controls remain available in their own conversation.
These operations do not initiate coordinated global Stop.

All enrolled owner executions reserve the same worker registry before native
preparation, including starts through goal, merge and Nexus controls. A routed
instruction joins that actual local execution; it does not wait behind an
unregistered native start. Native acceptance is matched to the individual
instruction, while a start acknowledgement accepts only its initiating input.
Definite steer rejection waits for the next physical native turn, or continues
on the same serialized worker after that execution ends. Unconfirmed acceptance
never takes this path.

Each dispatch attempt retains its captured admission generation. Preparation,
queued drainage, definite-rejection continuation and the native write boundary
recheck that generation and the existing owner/maintenance fences. Only explicit
known-unsent recovery can bind a later generation. A local admission rejection
does not interrupt the original native work. Routed steering uses the existing
frozen Nexus input perspective and revalidates its target rather than sampling
a later view or silently dropping its context.

A native control keeps its original conversation return. If routing joins that
execution later, its canonical answers need explicit valid worker coverage before
being associated with the newly routed request. Earlier answers remain in native
history; missing coverage leaves the routed input unresolved. Normal conversation
presentation identifies the separate follow-up status and strips controller
metadata only when routing actually participates. Questions and retained-file or
human-action notices use the actual participating source, without inventing an
original human message for the native execution.

Enrolled native presentation drains through a bounded queue outside the native
event pump. Backlog or connection loss does not imply delivery: canonical items
remain retained, and a local presentation-gap marker records a dropped queued
presentation. Control admission does not wait for that HTTP presentation. The
observer must preserve that incomplete state until reconciled from original
evidence.

A newly queued instruction that has not reached native submission after ten
seconds gets a source-linked preparation notice. The notice rechecks the
instruction phase after Telegram cooldown; an already submitted or withdrawn
input does not receive that queued-state notice. This is an observation bound,
not an expiry, cancellation or automatic retry. Active steer acceptance has its
own bounded observation and uncertainty handling.

`/interaction status OFFSET` pages over five enrolled conversations at a time.
Each card distinguishes the latest input transport state, retained result
transmission state and unresolved correction/coverage evidence. `/interaction
input ID` inspects the admitted instruction without resubmitting it.

`/interaction result ID` explicitly inspects a retained canonical answer and its
current relevance. That inspection is a new controller response; it does not
confirm or rewrite the original transmission. `/interaction deliver ID` uses
the original publication boundary only for a reservation with no prior attempted
parts. Restart restores preparation that never crossed that boundary to reserved;
an attempted transmission remains uncertain. Delivered or uncertain output is
never replayed by the recovery command. A failed output attempt produces a
separate status notice, subject to the same recipient and transport constraints.

Canonical native finals and their per-request return reservations commit in one
SQLite transaction. A failure between capture and reservation leaves neither
committed alone; repeated observations reuse their exact immutable identity.
Canonical outputs reserve per-request deliveries independently from task
completion. Each part rechecks corrections after Telegram cooldown and retains
its exact payload before the HTTP attempt. An obsolete answer is withheld and replaced with
a notice about its missing correction coverage; another request's result can still be delivered.
Every confirmed text part has an exact reply binding. A confirmed withholding
notice is recorded as `withheld`, distinct from delivery of the canonical answer. Missing transmission
receipts stay uncertain and are not automatically retransmitted. Worker metadata
cannot choose a return address, and presentation strips its metadata tail.

Native question publication runs outside the native event pump. Routed questions
appear in the original requesting conversation while their durable interaction
and native answer retain the actual source worker. The ordinary question UI and
explicit answer controls preserve that distinction.

## Read-only session observation

`/interaction` cards now read a bounded, owner-scoped SQLite snapshot containing
its admitted-input sequence, latest return reservation, recorded local execution,
last retained canonical native item, pending questions/tasks and known presentation
gaps. Each card identifies the source session, native binding and observation time.
Pending items retain their owning turn and ID; their existing worker `/pending`
surface remains the place to answer them.

Native activity is sampled separately through the existing bounded read-only
`thread/read` adapter. Reads run asynchronously so disconnected status cannot
block new input. A changed binding, unavailable daemon or unknown native status
produces an explicit unknown state. Local recorded execution, native activity,
worker coverage and completion remain separate observations. Reading status never
submits a turn, steers, resumes work or advances an instruction sequence.

These cards explicitly describe their bounded, unreconciled native-history
coverage. Presentation gaps remain visible even when newer native activity is
available. They are the deterministic observer surface; durable incremental
history reconciliation and semantic summaries remain subsequent observer work.

## Derived semantic cards

`/interaction observe #Topic` reads only the selected owned native source through
the existing bounded history adapter. It freezes original completed/provisional
entries, their native turn references, source revision and changed-entry indices
before reserving one read-only synthesis. The summary describes purpose,
decisions/corrections, current work and open questions. It is an attributed
interpretation of source statements, not verified completion or new authority.

The retained source, summary support indices, processed boundary and expected
summary revision commit together. A concurrent or late response cannot overwrite
a newer revision. Unchanged source revisions reuse the existing card without a
synthesis call. Missing previous boundaries remain explicit continuity gaps;
truncation/window bounds are preserved. Original snapshots remain available for
rebuild and inspection rather than relying on summaries of summaries.

`/interaction summary #Topic` inspects the current binding's retained card without
native/model calls. `/interaction sources #Topic` shows bounded original support
excerpts with their entry and native source references. Native binding changes
select a separate card lineage; they never relabel an earlier source as current.

Interrupted/uncertain synthesis attempts remain retained across restart. Merely
asking to observe again does not replay them; `/interaction observe #Topic
--retry` explicitly selects a new attempt while preserving the old record. Source
workers are never resumed, steered or interrupted by observation. Only the
read-only synthesis uses a separate ephemeral context with tools disabled.

Local canonical message, turn, input and human-question changes invalidate the
card through a counter updated in the owning transaction. The counter is captured
before the native read; the read completion time is retained separately from
synthesis commit time. Activity arriving during synthesis therefore leaves the
result visibly stale. Unchanged-source rereads update the check without another
synthesis. A restart requires native reconciliation even if the local counter
has not changed. An equal counter proves only no observed local change; external
native history remains sampled. Unsettled transport inputs are shown separately.

Original snapshots and cards survive explicit session deletion as historical
provenance; derived activity/check metadata is removed. Startup migrates the
previous session foreign keys transactionally, retaining original bytes. Current
owner and native-binding checks still control access to retained cards.

`/interaction candidates [OFFSET]` inspects five enrolled candidates per page.
Missing, locally stale, post-restart or more-than-sixty-second-old source checks
receive one bounded refresh; up to two run concurrently outside foreground
ingress. Recent locally reconciled cards require no native or model call.
Unchanged source history refreshes the check without synthesis, even if a local
transport event invalidated the prior check. A newer retained synthesis from a
different native source revision also invalidates recent-card reuse, including
external changes not seen in the local event stream. Its pending/uncertain state
stays visible until a later committed source supersedes it. Errors or uncertain attempts retain
the old evidence and unknown/stale status, without a retry loop. Each card shows
pending human items, presentation gaps and source provenance. Candidate inspection
never selects a destination or dispatches worker input.

Source-driven refresh currently runs on explicit observation/candidate queries.
Semantic routing integration remains continuing work.
Runtime quality qualification must check factual summaries separately from
successful processing and support IDs.

## Observer evidence inside the integrating dialogue

A newly created native entry thread advertises `interaction_candidates`, a
client-executed tool. The conversational model can request a bounded inventory
of all enrolled labels/current retained cards, then refresh one to five selected
sources through the same observer reconciliation path. Inventory performs no
native/model calls; source summary excerpts retain attribution, bounds, revision,
freshness and missing evidence. Refresh does not dispatch input, create a work
session or interrupt a worker. This is the observer input to later semantic
selection, not completed semantic dispatch.

The client validates the current native thread and active turn for each request,
suppresses duplicate request/call identities, and returns deferred results while
continuing to process events, steering and interruption. Entry ownership, allowed
human and native binding are checked before and after asynchronous source reads.
Other conversations, namespaces, unknown selectors or unsupported arguments get
a failed tool result. A late result is not sent to a different active turn.

The installed experimental protocol declares tools only on `thread/start`.
Native rollout metadata restores the tools on `thread/resume`; the client does
not replace an existing binding or send an unsupported resume override. An entry
created before this registration may lack the tool; explicit observer commands
remain available. The receiving pilot must qualify a newly created entry with
actual native tool invocation before claiming this conversational path works.
See [official App Server documentation](https://learn.chatgpt.com/docs/app-server).
Finite observer reconciliations already started by a tool can finish separately
from the integrating native turn; closing that turn does not resume or cancel
source workers or authorize another query.

## Semantic destination selection through the integrating dialogue

New native integrating entries also declare the stable `interaction_controller`
tool. Its `capabilities` operation returns the supported operations and schemas,
so later controller features can evolve without replacing a native conversation's
persisted tool set. Existing observer-only entries retain their original tools;
the adapter never silently changes a native binding to add tools.

`inputs` returns bounded excerpts and original ingress references for up to twelve
human text inputs actually linked to the current owned integrating native entry.
`candidates` reuses the observer. `dispatch` selects an original input reference
and one to five distinct enrolled destinations. It accepts no replacement human
text. Each destination declares a new request, amendment or context relation;
amendments/context require real request IDs owned by the same human/destination.
The conversational model can keep a topic local or ask for clarification through
its ordinary dialogue. New-session creation and broader routing remain pending.

The controller resolves the exact original human message from the immutable
existing ingress journal. Eligibility checks its actual entry/native binding,
owner, live allowlist, journal state and original admission generation. The
complete selected plan, source identity and request links are retained in one
SQLite transaction before transport. A rejected target/relation rolls the plan
back without starting any worker. Source text/origin cannot be replaced, and a
closed native tool cannot commit a previously unadmitted plan. No additional
message bus or autobiographical memory store is introduced.

After plan commitment, each original input follows the existing per-target
queue/steer/return path under its captured ingress generation. The deliveries are
independent of the front dialogue's lifetime; closing speech or an integrating
turn does not silently retract work already admitted. Repeating the same source
and selected plan returns retained identities/outcomes without another dispatch.
An uncertain or known-unsent outcome uses the existing explicit recovery path.
Admission/native acceptance remains separate from worker-declared incorporation
and task completion.

Worker packets identify the integrator, original human and original ingress
reference, retain the original message unchanged and label controller context
separately. Their bounded recent target context includes original source order
and transport state. A later original correction takes precedence within the same
channel even if an older semantic selection completes afterward; transport
admission order cannot rewrite human chronology. Context excerpts are bounded,
and incomparable channels do not establish an automatic precedence rule.

## Verification and remaining scope

The native identity and deferred-approval foundations are merged in #121/#123.
The controller connects the registry, explicit ingress, live steering, question
presentation and correction-aware return. `tests/routing_io.py` exercises the
process boundary with synthetic Telegram HTTP, native WebSocket and persistent
SQLite peers. Registry/publication tests distinguish preparation, uncertain
transmission, native acceptance, worker declarations and task completion.
These are automated fixture checks; they are not a receiving deployment or a
real model's semantic coverage qualification.

`tests/routing_native.py` is a separate opt-in qualification against the selected
existing native daemon, using synthetic Telegram HTTP receipts. It creates only
isolated test conversations and retains its SQLite journal, original inputs,
configuration and reports. Pass `--run-native`, an existing `--native-home`, the
native `--codex` executable and a new private `--state` directory. It exercises
live correction, independent work, reverse completion, entry responsiveness and
original-human/controller provenance, checking produced fixture contents
separately from acceptance and worker coverage. Scoped cleanup must be confirmed;
it never initiates a global Stop. `--resume` is restricted to the first input of
an incomplete fixture whose native threads were confirmed idle and whose input
remains known unsent; it uses the existing recovery command and retained identity.
This test does not establish real Telegram delivery, production deployment or
currency cost. Debug-level native preparation stage/elapsed logs contain no
message payloads and distinguish proxy initialization from thread binding.

The authorized native-model journey passed on the CI-qualified `a25f877` binary:
Alpha incorporated a mid-work rate correction in its actual fixture output,
Beta completed independently first, and the integrating entry answered while
Alpha remained active. Original-human/controller provenance, worker coverage,
exact synthetic return bindings and terminal native status were checked
separately. One cold preparation took about 36 seconds; an active correction
was acknowledged in about 66 ms and Beta admission in about 759 ms. These are
individual observations under the test host's load, not latency guarantees.
Native token totals were recorded; currency cost remains unknown. That journey
used synthetic Telegram receipts; it does not establish live Telegram delivery.
Receiving adoption was subsequently observed, including blocked/active goal
preservation and exact collector reattachment, in
[#113's deployment record](https://github.com/AlterMundi/telecodex/issues/113#issuecomment-6239250452).
The later voice/device journey above separately qualifies actual native output
through the adopted device path; deployment health alone is not that evidence.

Routed human-action declarations use the existing validated registration path
and remain owned by the actual native worker. The controller points to that
worker's `/pending` surface. Entry-wide canonical request presentation now exposes
questions and approvals through the existing owner-local source and authenticated
client, including full proposal paging and explicit response receipts; see
`tests/conversation_requests_http_io.py`. Invalid declarations remain retained
for inspection.

Routed file output remains retained with its worker turn. This text adapter does
not upload it through the legacy worker-topic uploader: attachment publication
needs the same per-request correction and return reservation as text.

The original roadmap's later increments are tracked in the owning issues rather
than inferred from this early two-session pilot. The current receiving source
includes semantic selection/new-session creation, observer/interviews, canonical
requests/results, text/voice/device input and the evolving routing graph.
Attachment publication retains the limitation above. Cross-body activation is
deferred by human direction. The configuration above is opt-in and does not
enroll arbitrary new sessions.

`tests/attention_io.py` exercises twenty retained contexts with at most two active
synthetic workers. During sustained Alpha progress it steers Alpha, keeps the
integrating dialogue available, reads an actionable native question without
answering it, and reads Beta's completed canonical result directly. It then
drains the remaining work and waits for committed return receipts. Reported
delays are observations of the local fixture, with zero model calls; they do not
measure provider latency, billing or human intent. Real human routing corrections
and clarification burden remain unqualified by the synthetic routing corpus.

`tests/semantic_routing_io.py` qualifies original-input discovery and dispatch
through the process boundary: rejected model-authored replacements, one atomic
two-target selection, repeated-selection deduplication, a correction while the
integrator remains active, and an older semantic selection arriving after an
already accepted correction. The delayed case inspects actual packet ingress
references and retained correction context; synthetic worker output does not
prove that a real model obeys the precedence instruction.

`tests/observer_tools_native.py` is an opt-in continuation of the retained native
fixture. It qualifies actual observer tool calls in one new integrating entry
and native tool restoration across listener restart, preserving earlier source
bindings and completed reports. It does not qualify semantic dispatch or real
Telegram. Its narrowly guarded `--resume` continues only the pending second
entry turn after the recorded reader-method failure; completed turns are never
replayed.

## Observer synthesis settings and capacity

`interaction.observer_model` and `interaction.observer_reasoning_effort` select
synthesis independently of source work sessions. Omitted model preserves the
source model; omitted effort preserves the native default. Select a model/effort
supported by the receiving account. Existing unchanged cards remain reusable;
changing synthesis settings does not force a new model call. These settings apply only to ephemeral
observer summaries; they do not modify source bindings, goals or preferences,
and do not change other handoff/merge summarizers.

All observer consumers share two synthesis slots across the receiver, including
concurrent text commands and native tool calls. Original-source reconciliation,
unchanged-card reuse and inspection of retained attempts do not wait for these
slots. The retained `summarizing` phase includes queued attempts; it is not proof
that inference has started. Ownership/native binding is rechecked after waiting.
Shutdown releases waiters into retained uncertainty, without retrying model work.
Foreground dialogue and worker capacity remain independent. This is an observer
capacity bound, not a provider-wide token, spend or inference limit.

## Discovering existing owned conversations

Set `interaction.discover_owned_sessions = true` to include existing, currently
bound conversations owned by the configured human in the same chat. The default
is false, preserving explicitly selected profiles. Discovery excludes the entry,
other humans/chats, unbound sources and bindings marked for replacement. It reads
only receiver metadata: it does not enumerate unrelated native histories, synthesize
summaries or start work. The resulting registry is shared by observer inventory,
explicit selectors and dispatch; ownership is checked again by those operations.

Configured and created destinations retain their assigned names. Discovered
single-token titles are presentation labels; duplicate labels require `#tID`.
Other titles use `#tID` directly, including titles that could impersonate a stable
selector or collide with a Matrix target. Discovery does not change client grant
scope or expiry. Changing the receiving configuration changes its binding bytes;
existing companion credentials require the supported explicit owner
reauthorization before they can use that binding. The shared 64-target limit
still applies; an oversized profile is
rejected rather than silently omitting candidates. Native tool restoration for
existing work threads follows the same existing enrolled-session lifecycle.

## Creating a distinct work session

The stable integrating controller exposes `create` for a clearly distinct task,
after inspecting existing candidates. Its arguments select a retained original
input, a single-token topic name, a reason and up to five enrolled observer
context sources. Ambiguity and a busy worker call for clarification; neither
justifies a new session. Context interpretations retain their source, revision,
support indices, freshness and boundedness, separately from the exact original
human input. Selected context does not authorize repeating source effects.

An interaction profile may start with no enrolled targets; an empty inventory
is not by itself a reason to create a session. One immutable creation intent
reserves the original input before any topic RPC.
The existing Telegram topic adapter waits for outbound capacity, revalidates
original ownership, native binding and ingress generation, and marks the one
mutation immediately before HTTP. A failed pre-send check is known unsent. A
lost or invalid topic receipt is uncertain and is never retried automatically.
`creation_status` takes the original `input_update` reference to inspect the
retained operation even when the original tool response was lost. A confirmed
topic is retained if later initialization or admission fails; inspection is not
a request to create another topic. The explicit `creation_resume` operation takes the original `input_update`.
It transactionally claims only `retained_unsent` (one still-unsent topic mutation)
or `retained_topic` (initialization/admission of the already confirmed topic).
Concurrent, completed and uncertain operations are inspected without replay.
Original owner, binding and ingress generation must still be eligible; a new
request cannot revive a retired original. After commitment the resumed operation
continues independently of the front dialogue, using the same checked HTTP seam.
Failures retain the last known boundary. `creation_status` also exposes actual
instruction IDs/phases for the existing downstream known-unsent recovery path.

The existing empty-session initializer copies the selected entry settings into
a fresh owned destination. Final admission atomically enrolls that destination
and retains the original instruction, then uses the same independent worker
delivery and return path. Enrolled created destinations appear in observer
inventory, explicit selectors and status without modifying the static config,
and remain available across restarts. Creation cannot change an existing input's
selected target plan or grant access to another human's sessions. Configured and
created contexts share the existing 64-target bound; pending uncertain creation
intents reserve capacity. A new topic or native acceptance is not proof of
incorporation or task completion.

## Worker capacity and retained contexts

`interaction.max_active_workers` defaults to two, matching the actual qualified
two-worker native pilot; it may be selected from one to 64. It bounds enrolled
top-level native turn reservations in this listener, including native control
launches into enrolled topics. It does not claim a provider-wide model-call,
child-agent or monetary limit. The integrating entry and bounded observer
synthesis remain separate. No twenty-worker latency or cost guarantee follows
from retaining twenty contexts.

Waiting workers retain their input and one ordered writer reservation. They
acquire capacity before native preparation, context work or user presentation.
Corrections to a worker already running use its existing native steer path.
Corrections to a waiting worker stay queued and join that actual execution when
it starts. A scoped interruption can withdraw a waiting execution without a
native RPC; its admitted inputs remain known unsent for explicit recovery.
Owner, captured input generation and maintenance/Stop fences are checked again
after the capacity wait. A queued goal revoked during that wait cannot activate;
attached inputs remain known unsent. Existing explicit Stop-resume validation
stays intact.

`/interaction attention` reports current-instance active/waiting reservations,
peak concurrency and capacity waiting time without a model call. These counters
are not task completion or receipt evidence and reset with the listener; durable
request/result/return state remains in the existing journals. The semaphore is
FIFO for waiting acquisitions. A native turn keeps its permit until its local
execution finishes, including native goal/tool waits; this is not preemptive
round-robin scheduling. Higher-load provider costs and attention policy still
require their selected live qualification.

After ten seconds, a still-queued input receives a notice. If its worker is
waiting for capacity, the notice reports active workers, the configured limit
and the number waiting, and explains that tools/process waits keep a slot.
Otherwise it describes conversation preparation or earlier work without
inventing capacity saturation. The existing queue starts work automatically
when admitted; the notice itself never cancels or resubmits input.

## Attributed requests between local work sessions

A selected enrolled worker can use the stable `interaction_controller` envelope
with its own role-specific capabilities. `inputs` identifies accepted inputs in
that actual native execution. `session_request` selects an original human parent,
a stable caller request ID, one existing unambiguous topic and at most 16,000
bytes of session-authored text. It does not copy a transcript or create a topic.
`session_requests` inspects the latest sixteen retained dependencies.

Runtime context names the selected worker topic and separates its task from
integrator-only clauses in the unchanged human message. For a continuing request,
coverage declarations explicitly include earlier applicable instruction IDs whose
constraints were actually carried forward, together with new incorporated context.
A declaration covering only the most recent message does not establish cumulative
coverage. Missing or unresolved constraints remain visible rather than being
inferred from a prior answer or native history.

A local dependency is a single level: another session's request or result cannot
serve as a human parent. An accepted original human instruction can authorize at
most four dependencies. Self-addressing is rejected. Reusing a request ID retains
the same bytes, target and outcome; changing the ID to hide uncertain submission
is not recovery. Native acceptance and worker-declared coverage remain distinct
from observed incorporation. Existing explicit controls and known-unsent recovery
use the same instruction journals as human routing.

Admission is nonblocking. A caller can continue independent work or finish its
turn with a dependency deferred; it must not poll or wait in a tool loop. The
helper's first current, worker-declared completed canonical result reserves one
context input to the original logical caller. An active caller receives it through
steer; a caller that finished can receive a new native turn through the existing
worker queue and capacity limit. The helper result is attributed evidence, not a
new human instruction. Original request and return references survive logical
session renewal, while the original native executions stay in provenance.

A scoped caller interruption or goal pause/clear withdraws future continuations
and the existing accepted parent grants. It does not undo already accepted helper
work or erase its canonical answer. Retired input generations cannot authorize a
new request or return. Queued withdrawn context remains known unsent; in-flight
uncertainty is not relabeled as cancellation. Existing completed returns remain
historical evidence.

If a process stops after saving a helper answer but before reserving its return,
`session_receive` can explicitly select that retained local request ID. It uses
the existing canonical result and reserves the missing context once, without
running the helper again. Already reserved or uncertain returns are inspected;
they are not automatically replayed. Other known-unsent input recovery remains
explicit. This adapter installs no polling or background model attention.

`/self#Name` is a local conversation selector equivalent to `#Name` at the start
of a human message; it is not a Matrix scope or an alias for `/we` or `/tribe`.
Quoted or incidental selectors do not dispatch. Duplicate names require selection
of the stable `t<topic_id>`; a native renewal does not silently choose a different
logical conversation. A worker may use the same local spelling in its structured
selected destination. This grants no cross-body routing.

Native tools persist at thread creation. Newly created enrolled workers advertise
the controller; existing native threads without that registration are not silently
replaced. Runtime and real receiving qualification remain distinct from synthetic
process tests. No live deployment or general twenty-model throughput follows from
this source implementation.

The integrating controller's `target_inputs` operation reads up to sixteen recent
inputs for one explicitly selected owned target. It returns receiver-retained
request IDs, relations, source references, bounded attributed excerpts and transport
states without a native read, summary synthesis or dispatch. These records are
separate from observer interpretations and do not certify task completion. Use
these IDs to link corrections with `amends` or additional material with `context`.
If the intended earlier request is absent or ambiguous, ask rather than inventing
an ID or turning the correction into a new task. Existing native conversations
can discover this operation through the stable controller's `capabilities` call.
