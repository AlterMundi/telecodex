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
receives this Python object or native credentials. Before mutation, the same
authenticated client must report the approved being
and body through `runtime.status`, within the same absolute deadline. The client
configuration must already select the approved body. The SDK must support an
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
the current human text or committed machine transcript. Serialized selected
content is bounded to 15360 UTF-8 bytes, reserving 1024 bytes for the SDK relay's
fixed metadata and bounded execution reference. Oversized JSON-escaped context
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
