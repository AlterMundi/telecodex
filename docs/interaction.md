# Integrating conversation controller

Owning plan: [Daimon Interaction](https://github.com/AlterMundi/daimon-matrix/issues/313).
Implementation: #109–#119. The controller is being built in the existing listener;
the configuration below is not yet a qualified receiving deployment.

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

`#Name message` or `#t7 message` in the integrating entry selects an existing
owned native conversation. Replying to an original admitted message or returned
result inherits its request and target; an explicit selector overrides that
focus. Direct text in an enrolled worker topic uses the same controller. Plain
unselected entry text remains ordinary conversation. Semantic routing is a later
extension; this adapter does not pretend an explicit selector is a classifier.

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

Canonical outputs reserve per-request deliveries independently from task
completion. Each part rechecks corrections after Telegram cooldown and retains
its exact payload before the HTTP attempt. An obsolete answer is replaced with
a correction-pending notice; another request's result can still be delivered.
Every confirmed text part has an exact reply binding. Missing transmission
receipts stay uncertain and are not automatically retransmitted. Worker metadata
cannot choose a return address, and presentation strips its metadata tail.

Native question publication runs outside the native event pump. Routed questions
appear in the original requesting conversation while their durable interaction
and native answer retain the actual source worker. The ordinary question UI and
explicit answer controls preserve that distinction.

## Verification and remaining scope

The native identity and deferred-approval foundations are merged in #121/#123.
This branch connects the registry, explicit ingress, live steering, question
presentation and correction-aware return. `tests/routing_io.py` exercises the
process boundary with synthetic Telegram HTTP, native WebSocket and persistent
SQLite peers. Registry/publication tests distinguish preparation, uncertain
transmission, native acceptance, worker declarations and task completion.
These are automated fixture checks; they are not a receiving deployment or a
real model's semantic coverage qualification.

Routed file output remains retained with its worker turn. This text adapter does
not upload it through the legacy worker-topic uploader: attachment publication
needs the same per-request correction and return reservation as text.

The remaining product plan includes attachment publication, semantic selection, observer summaries,
attention across many contexts, unified approval/task presentation, voice,
device input, derived interviews and authorized cross-body operation. The
configuration above is opt-in and does not enroll arbitrary new sessions.
