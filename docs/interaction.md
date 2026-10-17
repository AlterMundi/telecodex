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
are the next layer on these retained records. Operational state stays in Telecodex;
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

## Current implementation checkpoint

The native identity and deferred-approval foundations are merged in #121/#123.
The request registry, explicit-selector parser, frozen selection/return binding,
per-request correction links and transport recovery are implemented on the
interaction-router branch. Registry tests exercise separate requests in one
physical turn, ownership failures, duplicate admission, restart uncertainty and
cancellation before submission. The listener ingress, worker packet delivery,
clarification bindings and publication reservation still need to be connected;
this is not an end-to-end text-router acceptance claim.

Continue inside the existing App and native worker. Establish the worker's steer
channel before asynchronous presentation/preparation. Keep native event handling
responsive by deferring network presentation and human dialogue waits. Use the
existing immutable ingress journal for clarification source references. Wire
both explicit entry input and direct enrolled-topic input through the same
controller. Native outputs must pass request coverage and publication reservation
before returning to the recorded requesting conversation.
