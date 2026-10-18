# Selected Matrix execution adapter

The optional host module `scripts/matrix_execution.py` connects an already
approved Matrix `ExecutionStore` cycle to its authenticated owner-local SDK
client. It is a building block for [Matrix #367](https://github.com/AlterMundi/daimon-matrix/issues/367),
part of [Daimon Interaction](https://github.com/AlterMundi/daimon-matrix/issues/313).
It is not enabled in the local conversation router yet.

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

The production integrating conversation must still bind this host adapter to
its retained request/return path, qualified receiver execution and immutable
origin. The selected cross-body trial and complete correction/renewal journey
remain open in #367. One inference/token admission does not establish a global
monetary ceiling. No existing local routing, selected runtime or paused goal is
changed by importing this module.
