# Integrating conversation controller

Product direction: [full roadmap and current continuous-voice plan](interaction-roadmap.md).
Current execution: [milestone9](https://github.com/AlterMundi/telecodex/milestone/9).
Remaining v0.2 obligations: [milestone8](https://github.com/AlterMundi/telecodex/milestone/8).

## Current repair direction — human correction, 2026-10-19

The delivered pilot did not establish a coherent daily experience. The owner observed hidden progress in ordinary Telegram topics, repeated copies of shared answers and verbose relay packets. Prior completion records retain their bounded evidence but do not establish completion of the repaired product.

The current MVP is a single **web conversation** for text and two-way voice, coordinating existing native work sessions. Ordinary Telegram work topics retain their native input, commentary, answers, controls and file handling. Destination discovery and observer summaries never authorize routing interception. Only actual integrator work uses its capacity quota; the existing per-session writer still coordinates concurrent ingress.

`interaction.telegram_ingress` defaults to false. The optional Telegram adapter applies only to the explicitly configured integrating entry. Even when enabled, ordinary work topics, replies and literal selectors bypass routing. Enable it only for a human-selected integrating entry, never as a test/deployment convenience. With telegram_ingress and telegram_entry_projection selected, that one topic is another surface of the same conversation: committed app text/voice transcripts, integrator output, captured-scope worker progress and canonical results are projected without creating new input. Telegram originals remain their own messages and are not echoed again. Other work topics retain ordinary direct input. Existing per-source Telegram receipts and uncertainty prevent automatic replay; enabling the option does not backfill old messages.

Independent `interaction.telegram_projection` enables attributed work-topic
copies without changing ingress. It shares canonical event/destination identity
with direct answers and retains uncertain delivery without replay; see
[work-topic projections](topic-projections.md). It starts disabled until selected
in the receiving configuration.

Acceptance must demonstrate the entire actual client/controller/native/return journey: visible input retention, attributable intermediate communication, appropriate destination or clarification, mid-work corrections, and one canonical response in the requesting conversation. Historical unresolved work must not repeatedly fan out the same answer. Worker relays include the current attributed input once. Their delivery frame identifies the receiver as the work session: destination selection/forwarding has already happened. Integrator-addressed wording in the unchanged original does not ask the worker to become a second router; the requested work and its restrictions still apply. An unclear assignment or conflicting intended destination requires clarification. Native history retains earlier messages; only later-captured related deltas accompany a delayed dispatch so an older message cannot override a newer correction. Unrelated topic inputs and repeated policy walls are not sent. Native mobile/badge and cross-body activation remain deferred.

Owning plan: [Daimon Interaction](https://github.com/AlterMundi/daimon-matrix/issues/313).
Next acceptance campaign and product sequence:
[qualification and roadmap](interaction-qualification.md). The first functional
baseline is distinct from daily-use acceptance.
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

The browser entry uses [shared authentication](interaction-companion.md#browser-access)
and a remembered session through its registered home. The existing
Conversation view in authenticated Sessions and enrolled Talk devices use the
same retained conversation. Text and committed voice clips enter that conversation.
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
The ordinary Talk credential can inspect its canonical web input traces. The
separate monitor can also read retained legacy Telegram routing originals and
has no input, playback or session-control authority.

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

At startup, an explicit integrating entry creates only its missing local session metadata for an already allowed owner. It performs no native/model request and preserves existing bindings and settings. An unallowed owner or another owner's existing entry is refused. No priming human message, model turn or manual database seed is required.

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

Router capacity waits do not hold a session's ordinary native input queue. If a
direct input arrives while an older routed original is waiting for capacity, the
direct input starts its normal native execution. The older original remains
correlated and can join that execution through the existing steering mechanism.
Its delayed relay carries bounded chronology references to later direct inputs
already in native history. Those references come from the existing ingress
journal; they never create router requests or grant discovery subscribers access
to direct conversations. Preserve applicable later corrections without repeating
their actions. Historical progress cards describe an update, not a claim that
the session is still working after its final answer.

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

## Canonical responses and requesting conversations

A native final item is one response. Several work requests addressed by that
item do not create several copies in the same requesting conversation. Its
shared delivery retains all associated request IDs, while the canonical native
claim preserves the complete reported coverage. A response is complete only
when the worker declares every associated request complete. Freshness checks at
inspection and before transmission include corrections to every association,
including a request other than the primary compatibility ID.

Only requests with an accepted input in the actual responding execution receive
a new return. Mentioning older unfinished work in cumulative coverage does not
make its earlier messages unanswered again. A contextual follow-up uses the
participating input's retained return origin when it differs from the original
work request; the model cannot choose a recipient. Distinct authorized audiences
and session-authored dependencies retain their own correlations.

The response transaction reserves every return once under the native item
identity. Re-observing the item cannot create new returns after later input
arrives. Different native items can legitimately contain identical text. Existing
publication history, uncertain attempts and source originals are preserved;
no content-string deduplication or work replay repairs earlier copies.

## Listener routing and return

The native worker is explicitly told that an interaction controller is relaying
an authenticated human input. The packet keeps controller guidance separate from
the human-authored content and carries original author and source-message
provenance. Direct work-topic input bypasses the routing adapter entirely. Relay does not impersonate a direct human
intervention, and future integrator interpretations must remain separately
attributed. Canonical worker answers also retain their own authorship.

`#Name message` or `#t7 message` in the integrating entry selects an existing
owned native conversation. Replying to an original admitted message or returned
result inherits its request and target; an explicit selector overrides that
focus. Direct text in a work topic stays with its native writer and ordinary presentation. Plain
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

The existing owner executions share writer coordination so actual integrator work can reach a busy native session. Ordinary starts through goal, merge and Nexus controls do not consume the integrator capacity quota. A routed
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
presentation retains ordinary native output and strips a structurally valid
controller metadata tail even when an older native context emits it. Questions and retained-file or
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

Canonical native finals and their grouped return reservations commit in one
SQLite transaction. A failure between capture and reservation leaves neither
committed alone; repeated observations reuse their exact immutable identity.
Canonical outputs reserve one return per requesting conversation independently
from task completion, retaining every associated request. Each part rechecks corrections after Telegram cooldown and retains
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

The same owned native turn also carries its scoped structured-question journal
when native text history omits the question/answer RPC. The observer sees the
request's current recorded state and any retained conversation decision, with
secret answers omitted. A recorded answer is distinct from native incorporation
or task completion; an answered question must not be summarized as unanswered.
Answer-only changes invalidate the source revision. Evidence stays attached to
its original turn and preserves native boundaries, support indices and bounded
history; missing turns do not import unrelated question records.

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

Unconfirmed synthesis emits a local warning with the retained attempt ID and a
fixed `failure_code` (for example `support_outside_source`, `invalid_json` or
`timeout`). Unclassified failures use `other_refresh_failure`; this label does
not establish a provider or source diagnosis. Raw native errors and private
source/summary text are not included in this warning.

Synthesis requests constrain support selection to at most six zero-based indices
within the supplied original-entry array, including the array's current upper
bound in the structured output schema. The prompt distinguishes these positions
from native turn identifiers. Receiver validation remains in place; a
nonconforming result is retained as unconfirmed rather than published as a card.

Interrupted/uncertain synthesis attempts remain retained across restart. Merely
asking to observe again does not replay them; `/interaction observe #Topic
--retry` explicitly selects a new attempt while preserving the old record. Source
workers are never resumed, steered or interrupted by observation. Only the
read-only synthesis uses a separate ephemeral context with tools disabled.

The authenticated owner-local client offers the same selected operations:
`telecodex --control CONFIG observer observe '#Topic' [--retry]`,
`observer summary '#Topic'` and `observer sources '#Topic'`. Quote selectors in
shell commands. Observe acknowledges scheduling, not successful synthesis; it
does not fabricate a human message or a Telegram delivery. Operational notices
are retained as local `canonical_routing_notice` audit records. Summary and
sources return their bounded retained text directly without a model call. After
an uncertain command acknowledgment, inspect retained evidence before repeating
the operation; no automatic retry is added by this adapter.

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

### Foreground routing context

The ordinary integrating native turn receives a compact metadata snapshot from
its existing receiving registry, plus the reference to the original that started
that turn. This removes capabilities/inputs/inventory round trips solely to
recover an already known original and clearly named destination. It introduces
no extra model, native thread, registry, dispatcher or persistent writer.

The snapshot is bounded to8192 JSON bytes of candidates. `complete` and `total`
distinguish truncation or unavailable metadata from an empty registry. Stable
topic selectors remain separate from untrusted display labels; neither labels
nor eligibility prove relevance, readiness or completion. Contextual/ambiguous
requests still need observer evidence or human clarification. A source reference
is scoped to the **initial** turn input: later steered messages retain their own
references and must never inherit that earlier message's routing decision.

The foreground role keeps ordinary dialogue local and routes authorized long
work to its owning session without waiting/polling for completion. It does not
create a duplicate session because one is busy, rewrite human text, change the
selected model, cancel a job or mutate a goal. All dispatches still use the
existing exact-original controller with current owner/binding/generation checks.
A stale snapshot cannot bypass those checks. Unknown controller operations still
require capability discovery; this is an input hint, not a capability grant.

`tests/semantic_routing_io.py` exercises the first provider-facing turn context,
stable named dispatch without discovery calls, a fresh subsequent input while
two workers remain active and their independent attributed returns. Synthetic
native/model peers prove wiring and lifecycle, not actual model obedience or a
latency improvement. Those require the v0.3 integrated selected-model journey in
[#303](https://github.com/AlterMundi/telecodex/issues/303).


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
Creation `name` is the human-visible Telegram/native title, including ordinary
spaces (1-64 UTF-8 bytes, no surrounding whitespace, control/markup characters
or reserved `tID` titles). A multiword title enrolls under the returned stable
`tID` selector; it is never silently renamed into a single-token title. Existing
single-token labels retain their selector. The store reserves the title across
creation intents, including uncertain effects, to prevent duplicate creation.
Fixed pre-admission title/argument failures return `creation_rejected` with a
specific code and `stage=before_intent`; private error chains remain hidden.
Inspect `creation_status` before correcting the same original's arguments:
absence describes one read, not permission to replay a concurrent operation.
Changing from dynamic tools to scoped CLI does not bypass the same controller.

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
two-worker native pilot; it may be selected from one to 64. It bounds actual integrator worker launches in this listener. Direct human/native-control launches do not consume that quota; registry eligibility alone is insufficient. It does not claim a provider-wide model-call,
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

## Attributed requests between local work sessions

A selected enrolled worker can use the stable `interaction_controller` envelope
with its own role-specific capabilities. `inputs` identifies accepted inputs in
that actual native execution. `session_request` selects an original human parent,
a stable caller request ID, one existing unambiguous topic and at most 16,000
bytes of session-authored text. It does not copy a transcript or create a topic.
`session_requests` inspects the latest sixteen retained dependencies.

Runtime context names the selected worker topic and instructs it to distinguish
its task from integrator-only clauses in the unchanged human message. This is
model guidance, not a retained per-destination task partition. Dispatch stores
the entire original for every destination; commentary about a split does not
encode one. Follow [routing ownership](interaction-routing-ownership.md) for
mixed requests and corrections. A local helper request does not transfer the
original request, its owner or its human-facing return responsibility. For a continuing request,
coverage declarations explicitly include earlier applicable instruction IDs whose
constraints were actually carried forward, together with new incorporated context.
A declaration covering only the most recent message does not establish cumulative
coverage. Missing or unresolved constraints remain visible rather than being
inferred from a prior answer or native history.

Local requests, helper answers and return-only caller wakeups retain their original
provenance but do not inherit permission to publish into that human conversation.
The backend freezes an internal audience per canonical delivery. Such records
remain available for dependency return/inspection, but are excluded from ordinary
presentation reads, direct source reads and final transport reservation. Returning
context does not publish a helper answer or request an unsolicited caller narration.
A current covered human input in the caller's actual execution can still produce
one ordinary human-facing answer incorporating that evidence. A genuinely deferred
logical human task can also complete once across physical turns when its cumulative
human/context coverage is current and complete; a previously completed human request
is not re-announced merely because internal context arrived. Unreported or partial
helper coverage stays internal and cannot reserve a completed return. Internal-only
executions also do not inherit an older native progress return binding. Legacy
records use retained session authorship/coverage without erasing any delivery or
uncertain-send receipt. Later corrections keep their existing freshness semantics.

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


## Switching between web and direct conversations

### Requested next behavior: visible session parity — 2026-10-19

The human requested that a session's Telegram topic also show the integrator
activity visible in its CLI. This supersedes exclusive return-surface selection
as the desired presentation design. It is a pending implementation requirement,
not a claim that the deployed baseline already mirrors those events.

Project the attributed input/transcript arriving through the integrator, useful
native progress and canonical answers into that same owned session's topic.
Keep the correlated response in the requesting integrator conversation. Label
the origin concisely, without displaying relay JSON or internal coverage tails.
Use the existing audience bindings; this grants no other audience access.

These are projections of the same events, not new human inputs, controller
admissions or native executions. A topic projection must never feed back into
routing, trigger another response or multiply copies per associated request.
Retain per-destination publication identity and uncertainty using existing
delivery mechanisms. Failure to mirror must not cause replay of native work or
prevent the canonical integrator return. Ordinary direct topic input remains
ordinary native ingress, outside the router quota and without controller wrapping.
Textual visibility does not require retaining raw audio or sending a second
spoken copy. Qualify coexistence with direct interaction in the same session.

### Baseline behavior before session parity

Web routing selects a return surface for the work it admits; it does not take
permanent ownership of a native session's presentation. A subsequent ordinary
message in that session's Telegram topic retains raw native ingress and restores
commentary, answers and artifacts to the direct topic. This also applies when the
native turn is still running. Selection uses the existing receiver capture order,
so delayed transcription or dispatch cannot replace a later human selection.
The small presentation-selection record is not a router source/instruction and
does not consume routing capacity. Canonical answers already retained in the web
conversation remain history; switching channels never replays model work.


## Standalone conversation timeline

The standalone client presents saved human inputs, integrating-session commentary
and canonical answers in one chronological stream. Receive/observation timestamps
come from the existing input and native journals; refresh updates existing cards
rather than adding another copy. The routing graph and full trace remain separate
inspection tools. Older inputs and replies remain accessible from their existing
paging controls. Commentary is attributed progress, never a final answer or proof
of task completion. Worker commentary is exposed only when the original native observation retained a web return selection; switching to a direct topic never enrolls later direct progress into the web chat. Real provider duplex voice acceptance remains unfinished.


The stable integrator controller exposes `conversation` and `conversation_part`
for the same scoped progress and answer sources the web client displays. Reads
reuse presentation cursors and source revisions and do not dispatch, synthesize,
or claim hearing. The integrator can consult returned evidence when the human
refers to an answer from a worker; it keeps that evidence's session authorship
rather than injecting it as new human speech. Native worker progress snapshots
its web source at first observation. Duplicate native observations retain that
original scope, and old progress without a recorded web source is not imported.

Compact worker updates offer **Reply to this update** when the receiving native
backend advertises `can_reply_updates`. The selection freezes that source ID and
revision even when a later update replaces the preview. Native worker progress
and final messages resolve through the same owned presentation source; direct
work that never selected this web entry remains unavailable. The backend fences
source revision, target ownership and current native binding, including changes
during ASR preparation. The original text remains unchanged; the existing relay
`selection.reply` provenance carries the compact reference. The receiving session
can read only the needed exact-revision part with `conversation_part`. A reply to
an update is neither completion of the previous task nor a request for workers
to arbitrate its destination.

For an interruption or reference to spoken text, `playback_context` reads only the
selected original's frozen device report and a bounded exact-revision source
window on demand. See [interrupted-source inspection](interaction-speech.md#on-demand-interrupted-source-inspection).
This is advisory evidence, not a hearing receipt or an automatic resume command.

## Spoken conversation scheduling

Talk enables new spoken replies when synthesis is available. The browser captures
one utterance between Talk and Send voice, then uses the existing committed clip,
transcription and native delivery path. This is utterance-based ASR and scheduled
speech output; it does not claim continuous full-duplex transport.

New replies and native questions play sequentially through the existing
source/revision-fenced player. Enabling voice does not recite history. Pending
progress from one speaker is coalesced, and a final answer supersedes that
speaker's unspoken progress; every original update remains in the text history.
Capture and transcription preparation pause automatic output. Voice can be
turned off without affecting retained inputs or ongoing native work.

Playback supplies contextual source metadata. It is not proof of human hearing,
a destination selection, an answer or an approval. The next utterance can answer
or change topic. Voice uploads preserve `context_only` through the existing
`context:SOURCE` header form, and automatic question playback uses that same
interpretive boundary. Explicit reply actions retain their existing semantics.

## Live controller fallback for existing native threads

The native dynamic-tool set is persisted at thread creation. An older integrating
conversation or worker can invoke the same scoped controller through
`telecodex --controller TURN_ACCESS_FILE OPERATION_JSON` when terminal execution
and receiving session control are available. This is a live invocation, not a
completion manifest, new human message or generic owner-command tunnel.

The owner-only access file is bound to the actual local execution and human;
current native/physical binding and role are checked by the existing handlers.
It is invalid on cancellation, completion, binding change or listener restart.
Its token is read from the private file, never placed in argv or normal output.
A dormant local lease can exist for an enrolled direct execution so that a later
explicit routed steer can use it. Ordinary direct text, runtime instructions,
router admission and quota remain unchanged; the hint is disclosed only with
actual integrating/routed input. No new topic consumer or model wakeup follows.

The fallback shares canonical original reservations, dependency limits and
uncertain-outcome behavior with dynamic tools. Inspect retained outcomes rather
than automatically repeating an unavailable/uncertain operation. An old active
execution without a receiving collector/access lease still requires supported
collector attachment; its missing tool is not repaired by replacing history.

## Generated file return

See [retained generated files](retained-artifacts.md) for the explicitly configured
immutable artifact store, logical audiences, device file entitlement and recovery.
File preparation runs independently of native conversation progress.

### Long observer source turns

The native source reader keeps a 48,000-byte history window. Individual turns
use at most 32,000 bytes, retaining recent original statements and explicit
omission markers when a long-running turn exceeds that allowance. UTF-8 text,
role, native turn identity/status and provisional-versus-completed attribution
remain intact; older completed context can still occupy the remaining window.
A large current turn must not become an empty source merely because it cannot
fit whole. Source-read failures emit fixed diagnostic categories, without private
conversation text. `tests/observer_io.py` exercises a long active turn and unchanged
reinspection without new source work or repeated synthesis.

### Native runtime fragment limits

Native App Server truncates each additional-context value at 1,000 approximate
tokens (4,000 UTF-8 bytes). Telecodex preserves runtime control in ordered
`application` fragments of at most 3,000 bytes, with `telecodex.runtime` first
and zero-padded continuation keys. Concatenating those values restores the exact
original instructions, including multilingual text. A separate revision/count
manifest identifies the current set when native history emits only changed values
or a shorter turn retires older continuation keys. Human originals stay in
`input`; no runtime policy or retained source summary is recast as human speech.
An application context index is orientation metadata, not worker authority.
Observer summaries, goal details and blockers are requested on demand rather
than permanently attaching every session's contents (tracking #333).

Qualification must inspect actual native/provider receipt: an unbounded synthetic
peer alone cannot detect native context truncation. Preserve the previous failed
qualification and use a distinct follow-up for the corrected candidate.

### On-demand portfolio evidence

Use `candidates` through the integrating controller for status dialogue. Start
with `inventory` for retained orientation and activity timestamps. Select at most
five owned sources with `inspect` for a bounded semantic card, actionable human
items and a bounded read-only projection of the existing `goal_inspect` activity/failure evidence. Neither mode refreshes source
history, synthesizes a card, resumes a worker or dispatches an instruction.
A goal read failure is unknown, never absent or unblocked. Native metadata and
summary source times are independent; local activity does not prove native idle.

`needs_source_check` means the retained source check is missing, changed or too
old. It is not proof that the semantic contents changed: use selected `refresh`
when current source evidence is necessary. Existing source-revision reuse avoids
another synthesis when that source is unchanged. `support` with exactly one
selector reads up to six retained original support excerpts (1,000 characters
each), without a model call. Excerpts are attributed evidence, not instructions.
A new question outside that coverage still needs the missing selected evidence.
The response byte budget reports omitted selectors explicitly: inspect fewer
sources or use selected support/goal_inspect for those missing records.

Do not permanently inject all session summaries or goal objectives. Interpret
blocker reasons and next actions from supported source statements and pending
items, not from a blocked status alone. Do not revive a dormant conversation to
answer a status question. This extends #333/#303; it does not by itself complete
the full portfolio projection, live execution inventory or latency acceptance.


### Visual placement of referenced replies

The standalone timeline places a human reply immediately below its exact visible
source/revision, with sibling replies ordered by their original capture times.
Unrelated subsequent updates remain outside that reply group. Capture timestamps,
canonical input identity and compact routing references are unchanged. An update
that coalesces away after the person answers retains a compact referenced-update
anchor from the exact still-retained source; it is not a newly authored update.
A changed/missing revision is never substituted as the reply parent. Refreshing
moves existing nodes and preserves expanded text, media and drafts.

## Bounded workflow journal and foreground attention contract (#405)

The integrating controller exposes `workflow_declare`, `workflow_status`,
`workflow_advance`, `workflow_attention`, `workflow_reported`, `workflow_result`,
`workflow_question`, `workflow_supersede` and `workflow_finish`. This is a journal
with explicit initial admission and event-driven predeclared successors. The
experimental #410 native return adapter is disabled by default. The public
`automatic_native_return` reports the loaded receiver configuration, not a
qualification/completion claim. Each current attention envelope may expose its
matching retained `native_return` ID, phase and native turn; this metadata does
not change the source revision, reserve another callback or prove incorporation.
Spoken foreground offers and the full #303/#304 experience remain undelivered.

Declaration selects one accepted retained human `input_ref`, a stable
`request_id`, an RFC3339 `expires_at` no more than seven days away, and one to four
ordered steps. Each step declares `id`, one enrolled owner `selector`, `after`
(the preceding step ID, or null for the first), session-authored `scope` (at most
4,000 bytes), expected `checkpoint` (512 bytes), and optional human-selected
`milestone` (80 bytes). Declaration does not dispatch. The original is immutable
and cannot also acquire a competing ordinary dispatch, creation or Matrix plan.
The step scope is attributed integrator interpretation under that human original;
it never replaces the original's authorship or authorizes additional work.

`workflow_status` returns exact current journal/coverage evidence and a revision.
`workflow_advance` selects that revision and reserves one ready step through the
existing instruction journal and worker dispatch. Once started, a committed
current completed predecessor result triggers the same integrator-owned advance
primitive for its next predeclared step. The callback adds no scope and invokes no
integrator model. It rechecks the exact source, original authority, deadline and
current workflow revision transactionally; duplicate callbacks never resend an
existing admission. A restart/uncertain dispatch still requires its owning
known-unsent recovery instead of callback replay. A predecessor must have a
current completed canonical result covering its cumulative instructions, including
corrections. The successor pins that exact predecessor result revision. Its packet
contains a source descriptor, not a copied result body: use `workflow_result`
with workflow ID, predecessor step ID, source revision and optional character
offset to read 4,000 characters at a time. Only the integrating entry and the
admitted successor owner can read it. Workers cannot declare/progress a workflow
or turn its internal step into another helper's human parent.

Exact admission retries never dispatch an existing instruction. Queued,
known-unsent, accepted and uncertain native operations retain the owning journal's
phases and recovery path. Changed predecessor coverage/revisions expose
`correction_pending` or `predecessor_changed`; an already admitted successor is
retained for reconciliation, never silently replayed. Every required step must
currently complete before `awaiting_integration`; only explicit `workflow_finish`
with that current revision and a final integration checkpoint retains overall
completion. A worker completing one step cannot complete the whole workflow.
`workflow_supersede` requires a later accepted human original with retained ingress
order. It withdraws future/definitely queued work and preserves accepted and
uncertain effects. Existing scoped caller controls also withdraw future workflow
admission. Entry binding/generation changes and expiry retire future progression.

The existing presentation-list response includes `workflow_attention` with schema
`telecodex.workflow-attention/v1`. Its `envelopes` contain workflow/step IDs, exact
canonical result or question capture version, source session, current workflow
revision, reason, optional reporting milestone, deadline and an exact envelope
revision. Reasons include `reporting_milestone`, `successor_ready`,
`integration_required`, `reconciliation_required`, `admission_attention_required`
and `human_decision_required`.
No source body is injected. Question offers are scoped to the step's exact native
execution, exclude approvals and disappear when that version is no longer pending.
`workflow_question` reads the exact offered capture version on demand; it cannot
answer or approve. Existing human-selected question controls retain their own
separate decision revision and admission requirements.

The model-free projection reuses the existing presentation/event transport; it
starts no polling model, coordinator, helper chain or integrating-native wakeup.
Only a committed canonical result can trigger the bounded predeclared successor;
progress events cannot create model work. At most
16 active workflows per entry, four steps per workflow, one result envelope and
two pending question envelopes per step can be exposed. There is one current
offer acknowledgment per step/source slot, with no accumulating callback log or
result-body copies. `workflow_reported` deduplicates an exact integrated offer
across reconnect; it is neither proof of hearing nor a completion/publication
receipt. Admission and canonical history remain retained by the existing durable
request journal; expired/terminal workflows are tombstones needed for exact
identity, not disposable caches. Active projection and acknowledgment growth are
bounded independently of that required history. There is no existing automatic
workflow-journal pruning route to claim as protection. Retention therefore has
an additional hard stop: at most256 workflow rows for the entire listener store,
and at most256KiB serialized JSON per row (64MiB of workflow payload in total,
separate from existing canonical journals/SQLite overhead). Declaration reserves
32KiB within its document for future step/checkpoint metadata. Capacity refusal
is explicit as `workflow_capacity_reached`; exact retained retries and current
workflow progression remain usable. Terminal/expired rows and originals are not
automatically erased to create space. Further new workflow admission at capacity
requires a separately authorized compatible journal archival/retirement route;
until such a route exists, admission stops at the hard bound.

All workflow results stay internal under the existing canonical audience rules.
An attention envelope is evidence, never a human instruction, automatic publication
grant, approval or permission to start an unplanned action. Native entry return
must use an actual attributed internal-context seam, preserving the runtime's
admission gates. The existing local-session return path ultimately submits an
attributed text input to an enrolled worker; it does not establish that seam for
the separately owned integrating Conversation. UI/audio adoption must route
meaningful offers through its selected finite foreground and the existing
Conversation player while internal tracks remain muted, without manufacturing
human input or narrating every progress event.

### Experimental native entry return (#410)

`interaction.native_workflow_returns` defaults to `false`. When explicitly enabled,
committed canonical results and newly retained pending questions select only the
bounded meaningful attention envelopes already declared by the original human
workflow. `successor_ready` is model-free progression and does not cause another
entry model turn. There is no poller, timer or model rewrite of intermediate progress.
An idle integrating entry uses its existing worker/admission queue; a live entry
uses its current native execution. Both submit `turn/start` with `input:[]` and
`toolOutput:{name,namespace:null,output,retain:true}`. No callback text is submitted
as human speech or given a native user-message correlation ID. Ordinary direct
human ingress keeps its existing path. Native admission, Stop, generation, writer
ownership and maintenance fences remain authoritative.

The retained return is in the same bounded workflow document: at most16 source
revisions per workflow, each with an envelope of at most2,048 bytes, and capacity
reserved for later step/final checkpoints. The existing document cap still applies.
The existing execution journal supplies the local/native turn binding; a bounded
index (at most32 returns per local execution) links it to the workflow receipt.
These are transport receipts, not another queue or coordinator. Capacity leaves
pending source evidence visible; it does not silently drop the workflow or grant
new model work. Source text remains available through exact `workflow_result` and
`workflow_question` reads.

Before native submission the adapter retains intent and rechecks original authority,
expiry, generation and the exact source revision. It records `submitting` at the
native boundary, then distinguishes `accepted`, `rejected`, `uncertain` and
`retained_unsent`. Restart changes an unsubmitted reservation to `retained_unsent`
and an unresolved submission to `uncertain`. With the existing adapter explicitly
enabled, the maintenance heartbeat can claim the same retained known-unsent intent
again after2 then5 seconds, at most3 total attempts and4 claims per heartbeat.
It rechecks current authority, binding, generation, expiry, unreported source and
revision; it does not discover new events on startup. Submitting, uncertain,
rejected and acknowledged returns are never retried automatically. Attempts and
next eligible time remain visible in the existing workflow status. Asynchronous
observations carry their frozen attempt, so a late earlier ACK cannot acquire a
later attempt's collection or publication authority. Restart preserves the budget.
Once retry metadata exists, rollback must retain a compatible journal writer;
disable the adapter on that writer rather than restoring an older database or
silently resetting attempt counts.
An exact native ACK remains retained even if the local waiter later times out.
For a live-return race whose ACK names a different native turn, the immutable ACK
is `accepted_uncollected` with that actual turn ID. A distinct callback-only local
observation row anchors exactly that physical turn; the old execution collector
keeps its own identity and excludes the detached turn's events. The existing native
recovery heartbeat observes this row without submitting input, changing goals or
creating another attention loop. It does not claim live dynamic-tool collection.
An observed terminal canonical final is imported through the existing canonical
journal and changes the transport receipt to `accepted_reconciled`. The original
submission local ID, new collection local ID and exact native ACK remain distinct.
Receipts from the same submission that ACK the same detached physical turn share
one observation row and reconcile atomically. A sibling with an unresolved native
target keeps terminal import pending; an ACK is never guessed from another receipt.
The canonical report freezes every incorporated receipt's current source grant.
The helper result still cannot complete overall integration.

Observation uses the existing bounded native turn-history API. On an actual
unsupported-method response, it requests only the owned thread's metadata through
`thread/read(includeTurns:false)` and reads the returned regular rollout path. No
peer/controller argument selects a filesystem path and no other history is scanned.
The first `session_meta` must name that exact native thread, and the selected
physical `task_started` and terminal boundary must both be present. Persisted
assistant IDs/phases are retained, without invented item identities or truncation.
A header of at most256KiB and tail of at most8MiB bound each file snapshot; a missing
start, partial terminal, oversized source or missing canonical final remains
explicitly uncollected. Terminal import admits at most16 canonical items and64KiB
of canonical text per return. At the existing256-workflow/16-return limits this
bounds additional collection rows at4,096 and recovered canonical text at256MiB,
separate from existing native journals and SQLite overhead. Rows retain the existing
journal/explicit-removal lifecycle, never age-based deletion of authority or history.
One shared filesystem-read permit remains held until the actual read ends, including
after the8s caller deadline, so slow I/O cannot accumulate background readers.
Original deadline, generation, binding, supersession and Stop/maintenance fences
continue to govern observation; retirement retains the ACK and uncertainty. Turning
off new native returns does not erase a previously accepted finite observation.

ACK is not incorporation, a useful provider answer, reporting or hearing. An active
human turn has no native per-input causal attribution for its assistant output;
its ordinary publication lifetime remains governed by that human turn. For an idle
callback-only turn, canonical text, response drafts and cached speech are fenced by
the exact accepted source set and original reporting policies. A possibly submitted
or stale sibling result/question cannot borrow a current offer's publication grant.
Rejected/known-unsent context and an exact ACK into another physical collector do
not widen that source set. Each canonical item freezes its own bounded set, so later
inputs cannot change which sources authorized an earlier report. Existing
`workflow_reported` acknowledgment of the exact current version explicitly
supersedes an older accepted revision in that same workflow/attention slot; native
ACK alone does not do so. An unresolved question sibling remains fenced. A question
followed by a resolved final result in another slot has no implicit cross-slot
coverage declaration: a mixed callback-only output can remain withheld. This
integration/coverage gap stays explicit, without another journal or inferred grant.
A withdrawn,
expired or corrected source cannot publish newly generated callback-only output.
Previously published canonical text remains historical evidence with `current:false`.
Later actual human input receives its ordinary publication authority. A completed
workflow permits the final integrating answer from its retained final-result offer.
`workflow_reported` acknowledges evidence integration, and does not by itself prove
human publication. Telegram requires its existing explicit projection selection;
foreground audio still requires the human-selected Conversation surface/player.

Idle-root and held-active native `toolOutput` seams have separate actual-native
evidence of useful provider incorporation and retained internal history. These Core
proofs do not qualify this adapter, its collector or receiver installation. Mock
boundary tests qualify only the adapter contract. Keep the receiving
`interaction.native_workflow_returns` configuration off until the wrapper's useful
provider return and retained internal history are observed at the actual supported
native boundary, and retain any active-turn qualification gap. The configured
policy must remain observable even during qualification; do not hard-code a false
status to represent missing acceptance. A native ACK, delivery uncertainty,
source integration and human hearing remain different observations.

CompAII receiving selection,28October2026: PR423/424 is installed and the existing
adapter is enabled for **declared human workflows**. Full CI40725360141 passed.
An actual Sol6.1/openai/medium callback-only Root received the source-bound internal
return through this wrapper, called `workflow_result`, `workflow_reported`,
`workflow_status` and `workflow_finish`, and published one canonical answer in the
same Conversation. It contained no native human message or production test input;
the test context was archived after observed completion. This qualifies that idle
provider/controller/publication journey, not physical hearing, latency, arbitrary
session awareness or the whole continuous-voice roadmap. Earlier active-seam
provider evidence and synthetic active-wrapper coverage retain their own scope.

For actual-provider fixtures, set and inspect the stored entry model/effort:
changing default preferences does not update seeded synthetic sessions. Use the
existing `thread/name/set` durability operation before disconnecting an empty
paginated test context. Its rollout may exist before its metadata index row;
that does not establish a model turn or justify replacing an uncertain context.
Retain earlier unknown creation/submission outcomes; fix verifier-only failures
without repeating model work. Source, installation, incorporation and hearing
remain separate observations. Other receivers retain their explicit selection.

Once a workflow has been declared, preserve `interaction_workflows`, all underlying
instructions/results and session authorship during upgrades or rollback. A
rollback listener must retain this reader and original-selection exclusion while
workflows are active; an older binary unaware of workflow selections is not a
compatible operational rollback. Scoped withdrawal/expiry stops future
admission without deleting checkpoints, accepted work or uncertain operations.


### Ordinary routed native attention (#426 / PR427)

`interaction.native_routed_returns` defaults to false, independently of the
qualified declared-workflow adapter. It extends that same native return transport
for a current completed canonical result or pending canonical native question
from actual human-routed work. Internal helper/workflow legs, direct-topic work
and intermediate progress do not acquire this ordinary return path. No synthetic
workflow, human input, execution queue or continuous model polling is created.

The source row retains at most16KiB of return metadata and four question versions;
source text remains in its original journal. Offers expire after15minutes. Current
entry/native binding, Stop/maintenance epoch, retained original and semantic source
revision are checked before admission and callback publication. Cosmetic carrier
or artifact changes do not change the result's semantic revision. On-demand
`attention_source` reads at most1600 characters; `attention_reported` records
incorporation only, never hearing, task completion or an answer/approval.

Committed result/question events reserve offers. The existing heartbeat recovers
only retained known-unsent offers: at most4 claims per heartbeat across both
adapters, at most3 attempts with2/5-second delays. Submitting/uncertain/acknowledged
returns are never replayed. Each scoped recovery scan reads at most16 matching
source rows per canonical table; startup normalization reads at most256 unresolved
rows per table. No history discovery or replay occurs on enabling the flag.
Callback source sets use `native-public-set` for ordinary/mixed sources and retain
legacy workflow marker compatibility. Readers expose neutral `native_returns`
without mislabelling ordinary sources as workflows. Rollback must keep this
compatible reader/writer and preserve the journal; disabling the flag does not
revoke or erase retained outcomes.

PR427/source4372cbdd, mergeb149a458, fullCI40743262608 passed and is selected
on CompAII's local listener with `native_routed_returns=true`. The existing
workflow policy remains enabled. Web422/ui5149ca483732, Core, voice configuration,
two current native collectors and their goal identity/accounting were preserved;
no native restart, new human input, global Stop or peer adoption is claimed.
Current receiving/operator details live in the existing Shared Resources
Interaction record. Rollback keeps the427-compatible writer with the flag false.

The synthetic active-result / idle-question / idle-result real-wrapper journey,
known-unsent recovery, loaded-policy observation and default-off/no-backfill
cases passed in final-tree CI. Twenty-one targeted store tests include ordinary
source restart/correction, retained uncertainty, same-intent finite retries and
stale-attempt fences. Earlier actual Core/Sol internal toolOutput evidence is
reused at the unchanged native boundary; no provider or production work was
replayed. The presentation reports loaded `automatic_routed_native_return`
separately from retained receipt phases. Full stages2/3/required4, foreground
speech/coalescing and real-device acceptance remain unfinished; this is not a
whole-stage completion or hearing claim.
