# Selected Matrix execution adapter

The optional host module `scripts/matrix_execution.py` connects an already
approved Matrix `ExecutionStore` cycle to its authenticated owner-local SDK
client. It is a building block for [Matrix #367](https://github.com/AlterMundi/daimon-matrix/issues/367),
part of [Daimon Interaction](https://github.com/AlterMundi/daimon-matrix/issues/313).
The conversation can retain explicitly configured Matrix requests and canonical
returns. No remote participant or receiving execution is selected by default.

The trusted host constructs one closed task with `make_task`, including the
original input UUID, exact execution scope and explicit being/body destination.
The task hash is part of the existing externally approved instruction. For a
reply, the host instead selects an SDK-verified native parent; the SDK derives
the destination from that parent's authenticated origin. The parent's directed
being/body kind is frozen in the task and preserved on return; broadcast parents
are rejected before mutation. Model output supplies
only text. It cannot override destination, channel, parent, tool or authority.

Construct `broker = MatrixExecutionBroker(instruction, task, authenticated_client)`
and use `broker.controller(approved_store, qualified_runner)` to create the existing
`ReviewController` with an exact instruction/task admission guard. A Scope alone
does not bind the recipient; do not reuse the broker with an unpinned controller.
The broker implements the existing `NativeBroker` interface; the model never
receives this Python object or native credentials. Before mutation, authenticated source observation must report the approved being
and body through `runtime.status`, within the same absolute deadline. Both native
clients must select the same runtime, server origin and socket. The SDK keeps
messaging-only and observation capabilities separate; the host adapter routes
each purpose through its existing capability rather than creating a mixed one. The SDK must support an
absolute `deadline_ns` as well as a per-operation socket timeout. The qualified
source is Matrix `9b2b8ec2c455d3bf316bc00dc9e1bac44e9d5094`.

The application relay explicitly identifies `interaction-controller` and
`session_request` or `worker_result`. The signed native event identifies its
actual author. Neither marker impersonates the human or grants receiving model
execution. The selected recipient's existing human execution approval, runner,
provider and budget must independently admit any inference. There is no default
recipient, paid model, arrival hook, inbox polling or automatic reply.

The original input UUID fixes the native send identity independently of session
renewal and destination. Native journals reject changed parameters under that
identity. A correction is a distinct retained human input, not permission to
rewrite or reissue the original. An ambiguous submission remains charged in
`ExecutionStore`; explicit `inspect` observes the same SDK send without replay,
refund or new execution. `accept_verified_reply` correlates an already verified
selected event to the original message ID, digest, author, thread and recipient.
It does not perform signature verification itself or read an inbox.

## Technical original transport

`MatrixConversationTransport(authenticated_status, selected_author, authenticated_client)`
relays an already retained human original without a model call, synthetic execution
instruction or per-message execution approval. The selected host supplies the
source `{being_ref, embodiment_id}` from owner setup and reads the status through
the authenticated controller. This class is not exposed to a model, does not
read an inbox, and offers no execution controller. It shares the broker's exact
source/client checks, absolute deadline, immutable original and recipient, signed
native send identity, delivery inspection and verified-return correlation.

The outgoing thread is the retained request UUID; the stable send identity uses
the same original-derived namespace as the existing broker. `send()` relays only
the exact original, its attribution and selected lineage. It emits
`execution_ref: null`: this is a technical transport event, not an assertion that
an inference cycle was approved. `make_received_task` can prepare this verified
request, but only the receiver's independently admitted execution path may run a
model or produce its work result. Transport cannot mint fuel or paid approval.
Worker replies still require their actual execution reference.

Reconstructing the transport from the same retained current status retains the
native journal identity. Do not automatically retry an uncertain operation;
inspect the same send. Switching a previously authored original between execution
and technical modes cannot alter its old relay: the journal rejects changed
parameters under the same identity. This is a matching host capability; deploy
receiving parsers that accept the explicit null transport reference. The pinned
receiver preparation already accepts it; third-party consumers may require an
update.

This removes sender-side inference admission from the technical transport. The
normal controller's return attachment and the selected receiving native
owner remain separate unfinished integration work; importing or constructing the
class installs neither listener nor peer activation.

## Normal-controller technical submission

An optional `[interaction.matrix_transport]` connects `matrix_request` to the
selected host command. Without it, request retention preserves the previous
passive behavior. With it, the normal integrating tool first atomically claims
the retained original, then invokes the configured argv through bounded stdin
and stdout. Commands, source identity and socket/credential references come only
from owner setup, never model text. No second native writer, inbox poller,
execution store or paid admission is introduced.

```toml
[interaction.matrix_transport]
command = ["/path/to/python", "-B", "/path/to/matrix_operator.py", "--binding", "/path/to/private/host-binding.json"]
source = {kind = "body", being_ref = "SOURCE_BEING_REF", embodiment_id = "SOURCE_EMBODIMENT_ID"}
timeout_seconds = 15
```

Replace placeholders with the explicitly selected existing source and recipients;
this example is not an enrollment or activation. The owner-only binding uses
`schema: telecodex.matrix-host-binding/v1`, the exact `source` object with
`being_ref` and `embodiment_id`, and two purpose references named `messaging` and
`observation`. Each reference has only absolute `socket`, `client_config` and
`client_key` paths to existing owner-local SDK material. The SDK verifies their
capabilities; the host requires matching runtime ID, expected server and socket.
The messaging capability stays messaging-only and permits `messaging.send`;
observation separately permits `runtime.status`. The adapter never loads the
runtime outside its daemon or creates credentials. Its two calls share one
absolute deadline. Do not use a combined capability: the SDK rejects mixed
messaging and observation profiles.

The existing request row freezes the selected source author as `claimed_source`
and records `submitting` atomically before the command can have an effect.
That source survives a lost ACK, restart or later owner configuration change.
Both the host ACK and an early canonical callback must match the retained source,
request-derived native send ID and request UUID thread. A different source cannot
establish the submission by returning before its ACK. Legacy passive rows without
a technical claim retain their existing verified-callback contract. A valid fixed-source/request/destination submission becomes `submitted`;
that proves the SDK-authenticated authored outcome, not receiving incorporation,
completion or a particular delivery receipt. The original, binding and minimal
native submission are retained; signed receipts remain owned by the SDK journal.
Concurrent or later repeated tool calls never claim an already submitting,
submitted or uncertain original again. A lost response, bounded-process failure
or cancellation remains `submission_uncertain`; a crash can leave `submitting`
with unknown outcome. Restart does not resend either state. Inspect the stable
native send identity and its actual journal before further recovery. Child
termination affects only that command's own process group and does not establish
that an SDK effect was cancelled. An early verified canonical reply cannot be
erased by a later submission ACK or failed host observation.

This wires the normal controller's outbound technical send. The existing
SDK-verified owner callback retains canonical returns; automatic return attachment
and actual receiving native steering remain unfinished. No configured host or
participant is enabled by default, and technical arrivals never admit inference.

## Retained conversation requests and returns

Add `matrix_targets` under the existing `[interaction]` profile only for explicitly
selected recipients. Each entry has a distinct `name` and a `destination` tagged
`kind = "being"` with `being_ref`, or `kind = "body"` with both `being_ref` and
`embodiment_id`. These are signed identity references, not labels resolved through
an archived pair lane. An empty list preserves ordinary local-only operation.

The integrator's `matrix_request` operation accepts one retained `input_ref`, a
configured `selector`, and a `new`, `amends` or `context` relation. It cannot supply
replacement text. Selection atomically freezes the original, recipient, entry
generation, admission epoch and up to four explicitly selected same-entry,
same-recipient parent originals. Parent text and authorship remain separate from
the current human text or committed machine transcript. The explicit relation
(`new`, `amends` or `context`) and its exact selected request IDs also travel into
the receiving task, protected by the approved task hash. A relation cannot name
parents outside that package or add a recipient selector. Older relays without
a relation remain unclassified; absence does not mean an amendment or grant
execution. The receiving native owner still owns actual steering and admission.
Deploy a matching receiving host: the previous closed parser rejects the new
relation field, even though the signed Matrix transport itself accepts the text.
Serialized selected content is bounded to 15360 UTF-8 bytes, reserving 1024 bytes
for the SDK relay's fixed metadata, relation and bounded execution reference. Oversized JSON-escaped context
is refused before consuming the original. A Matrix selection excludes local
routing, creation and implicit native-question answers for that same original.

`matrix_status` reads one selected request or canonical event. The authenticated
owner can use `telecodex --control CONFIG matrix status UUID`. The trusted host
passes that exact status to `make_conversation_task`; it verifies the immutable
binding and excludes private human return coordinates from the outbound relay.
A model cannot paraphrase a relayed original into apparent human speech.
`make_received_task` prepares a selected SDK-verified original for independent
receiving approval; it does not read an inbox or approve execution.

After SDK signature and reply-parent verification, the host derives a callback
with `broker.conversation_return` and records it through
`telecodex --control CONFIG matrix return JSON`. This owner-local callback is a
trusted host boundary, not a model tool or independent signature validator.
Exact event retries are idempotent; changed bytes under the same event fail.
Late replies remain historical after native renewal or admission changes, with
`current: false`. Corrections retain their parent request and make the prior
result noncurrent; a received reply alone never proves task completion.

The browser requests the optional third `matrix` cursor from `/api/interaction`.
Canonical sources use `matrix:<event UUID>` and `kind: matrix_result`, preserving
the selected label and authenticated source author. Text and speech share the
existing source/revision checks. A contextual reply sends a new human original
to the integrator; the integrator explicitly selects its relation and recipient.
Two-cursor clients retain their local-only behavior. Device-client adoption and
production receiving execution remain separate qualification requirements.

## Retained return correlation

`MatrixConversationReceipt(authenticated_status, authenticated_client)` rebuilds
read-only correlation from an acknowledged technical request. The status must
retain `claimed_source` and the closed native `submission`, with phase `submitted`
or `reply_received`. It validates the immutable original binding, frozen author,
recipient and request-derived send/thread IDs. It can inspect that same SDK
submission and correlate an already SDK-verified selected reply through the
existing `conversation_return` callback. The inspected outcome must exactly match
the acknowledged message ID/hash and route; a different native result cannot
replace the recorded submission.

A historical `current: false` request remains readable. Reconstruction neither
changes its phase nor recreates pending admission: the object has no sender or
execution controller, and its `submit` method always refuses effects. The
canonical callback leaves current-generation/Stop checks to the existing owner
store. This is a trusted host API, not independent signature verification or OS
isolation; pass only authenticated owner status and SDK-verified events.

An uncertain send without a known ACK is insufficient to reconstruct this
receipt. Delivery inspection can show that some native send exists under an ID,
but is not proof that its full original wire content matched this retained human
request. Preserve that uncertainty and inspect the original SDK evidence rather
than fabricate a submission, reset the phase, resend or select another body.
Legacy manually approved relays retain their existing broker correlation path.

This component supplies restart/late-return correlation. Automatic selected-event
intake still needs the SDK's selected-thread read path and an explicitly selected
host attachment; constructing the receipt does not read an inbox, install a
listener or activate a receiving model.

## Qualification and remaining integration

Run the zero-inference suite using the pinned SDK checkout and its test helpers:

```sh
PYTHONPATH=<matrix>/src:<matrix> python3 -B tests/matrix_execution.py -v
```

The suite uses synthetic signed identities, actual native journals and loopback
HTTP/receipts, plus the existing synthetic external-approval fixture and real
SQLite execution store. Its dataclass client seam calls `GroupMessaging`. A
separate test uses the real AF_UNIX/HMAC client and synthetic daemon to reject a
mismatched being before send. These tests do not qualify a complete installed
receiving runtime, paid accounting or live cross-body execution. CI pins the SDK
source.

`tests/matrix_conversation_http_io.py` also exercises the actual native controller,
authenticated owner CLI and HTTP server around signed SDK fixtures, including
canonical returns, correction context and local-plan exclusion. Run it with the
same SDK source, existing dashboard dependencies and a freshly built text binary.
Its native workers and execution instructions are synthetic, with zero inference.

Production deployment, qualified receiver execution and the selected live
cross-body correction/renewal trial remain open in #367. One inference/token admission does not establish a global
monetary ceiling. No existing local routing, selected runtime or paused goal is
changed by importing this module.
