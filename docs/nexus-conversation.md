# Nexus Mini App conversation adapter

Owning issue: [Telecodex #30](https://github.com/AlterMundi/telecodex/issues/30).
This opt-in, owner-local adapter connects selected retained human contributions
to an **existing** private Telegram topic and its existing native Codex thread.
It reuses the topic worker, native steering and `/stop`; it creates no bot
consumer, topic, native session, transcription client or Matrix attention.

## Local prerequisites and deliberate connection

Use the existing Nexus service environment and its
[native conversation v1 contract](https://github.com/compaii/daimon-nexus/blob/main/docs/native-conversation-contract.md).
The installed operator imports those public backend APIs; it does not duplicate
their SQL or human authentication. The service owner must already have access
to the existing owner-private `nexus.db`, a regular non-symlink file with mode
0600. Missing state is refused rather than creating a second database. All
operator paths are explicit local configuration; API responses cannot choose
host paths. This transport is not a public/remote service.

Configure `[nexus]` in the selected listener's private TOML. The following values
are placeholders, not working identities or launch destinations:

```toml
[nexus]
python = "/local/nexus/.venv/bin/python"
operator = "/local/telecodex/scripts/nexus_operator.py"
workspace = "/local/nexus"
state = "/local/nexus-state"
listener_id = "selected-human-listener"
responder_ref = "selected-daimon"
launch_selector = "registered-opaque-selector"
poll_seconds = 3

[[nexus.routes]]
table_id = "selected-table"
author_ref = "selected-human"
human_user_id = 100 # Synthetic example; use the actually approved account locally.
label = "Nexus"
# Optional per-table destination; omitted routes keep the legacy global selector.
launch_selector = "registered-selector-for-this-table"
```

Requires `codex.shared_app_server = true`, the actual configured bot's Main Mini
App, an existing registered launch selector and approved Nexus Telegram
membership. Installing code alone does not enroll a human or activate a route.
Select the **existing** topic and issue `/nexus connect` (or
`/nexus connect <table>` when more than one route is configured). The command
checks the actual bot, private chat, authorized topic creator and current native
thread before Nexus's revision-checked configure call. It refuses fresh/unbound
contexts and the dedicated Tribu topic. No `/new`, history reset or latest-cwd
replacement is performed, including when the bound history is unavailable.

`/nexus open <table>` returns that route's Main Mini App link. Multiple research,
project and recording tables can use distinct `launch_selector` values without
changing their participant/native bindings. The global selector remains the
fallback for existing single-table deployments. A start selector routes
the table; signed Mini App identity and membership remain Nexus's responsibility.
The link is not proof of launch or an exact private-topic return URL. The
existing Nexus frontend owns its verified conversation/return link, source
versions, playback state and close/reopen behavior. Group rooms, private voice
capture and per-message topic deep links are not qualified by this slice.

## Requests, observations and controls

Bindings advertise `supported_requests` when `/nexus connect` is performed.
This receiver implements `contribution`, `iteration` and `peer`. Nexus treats
older bindings as contribution-only; installing a backend or frontend does not
silently upgrade a running listener. Deploy the paired receiver and explicitly
reconnect before submitting a new kind. This prevents a new package from being
claimed by an incompatible old listener.

An explicit `peer` request uses the same journal/worker/steer and receipt path.
It freezes the chosen whole-Tribe disclosure, responder and exact node/relation
version. Its native prompt asks the bound Daimon to use Nexus's existing finite
Matrix adapter and preserve the exact message/send identity. It neither polls
Matrix itself nor creates peer model attention. The human-input echo identifies
the requested forwarding. A native final response stays in the author's private
conversation; it is not the selected peer's signed response or task completion.
Nexus rechecks withdrawal, route and requesting grant before native dispatch;
the Matrix adapter rechecks them before external transmission as well.

Only IDs from the owner-local Nexus pending API are discovered. Telecodex
inspects their canonical scoped contribution and journals before dispatch.
Dispatch is claimed when the existing worker is ready, or through its live
steer handle. Nexus commits one dispatch before native I/O. The adapter checks
the binding revision, authorized human and exact thread again before submission.
Changing or deactivating the binding withdraws queued inputs; already accepted
work requires an observed native interruption. `/nexus disconnect` is not a
claim that an in-flight model stopped.

The human input is echoed with its Mini App origin. That echo and a queue
acknowledgement are **not** native acceptance. `accepted` requires a matched
native `turn/start` or `turn/steer` RPC acknowledgement with the actual turn ID;
generic `turn/started` notifications alone cannot establish incorporation.
Completion requires a literal nonempty native final reply and terminal event
for that same turn. Failed/interrupted receipts require the corresponding
observed terminal status. Source excerpts are bounded, retain their source and
transcript version, and explicitly mark missing/limited evidence. Source and
contribution text are data, never runtime policy.

Ordinary Telegram messages can steer active work. A selected new contribution
from Nexus uses the same native steer primitive when available. A definite
rejection is reported as rejected; an uncertain acknowledgement is retained
without fallback or replay. `/stop` in the actual topic remains the interruption
control. This version does not reinterpret Mini App question text as commands
or add an independent Mini App interruption API.

## Ordinary chat perspective at ingress

[Telecodex #94](https://github.com/AlterMundi/telecodex/issues/94) extends the same
adapter with Nexus's [visit contract](https://github.com/compaii/daimon-nexus/blob/main/docs/visit-contract.md).
After Telegram input is journaled, an allowed non-bot sender in their deliberately
connected private topic is matched to the actual existing native thread. Before
per-topic dispatch, attachment download, transcription or native steering, the
adapter synchronously reserves a durable capture row and an ingress time cutoff,
then starts `capture-input` independently of the topic dispatcher with stable
`telegram:BOT:UPDATE` identity and the authenticated target. This local read/capture
has a two-second deadline and does not block global polling or other topics. The
topic dispatcher awaits its result in original message order. A view revision
committed after the ingress cutoff returns `view_changed_during_ingress`, never
silently B for an input received during A. Failure is explicitly unavailable and never repaired
by sampling a later view when a worker starts. Other operator operations retain
their existing ten-second bound. No call happens for commands, callbacks,
reactions, other participants or unconnected topics.

The same immutable envelope accompanies a queued turn or active-turn steering.
A rejected steer can queue the input with the original perspective. Current
focus may move independently; source data in this envelope is not runtime policy
or permission to execute an effect. Missing, stale, ambiguous and no-selection
states remain explicit. Native routing and private drafts/camera are excluded
from the model-visible prompt. A changed native target refuses dispatch rather
than silently moving the contextual input to another conversation.

A crash during capture retains `capture_interrupted`; restart and duplicate
Telegram delivery do not recapture or replay input. The existing uncertain-input
journal remains authoritative. This is a separate additive SQLite table; existing
journals and offsets are not rewritten. Current authorized views can be read on
demand through the operator's `current-visits` verb using the actual native target;
that read does not replace any frozen input or create model attention.

Ordinary Telegram audio is captured before download/STT, at receipt time. The Bot
API message supplies no trusted earlier voice/composition visit revision: receipt
perspective must not be described as the user's view when recording began. An
explicit earlier composition using the owner-local `capture-input` contract can
supply `visit_id` and `view_revision`; existing Mini App contribution snapshots
keep their own composition context. Never parse claimed routing IDs out of chat
text or guess a recording target. Group presence, general routing and voice-room
integration remain their owning increments.

## Recovery and deployment

`/nexus status` shows this human/topic's retained adapter state. `/nexus reconcile`
inspects retained request IDs, retries immutable metadata receipts and, only
where acceptance recorded an actual native turn ID, reads that exact selected
turn's final/status. It does not start or steer a model. Unknown sends remain
unknown; absence in a bounded history page is not proof of non-admission.

A restart never automatically replays inputs. `/nexus recover` is an explicit
recovery for old journal-only queues: the local row must have no dispatch or
native ID and canonical Nexus inspection must still show queued/no dispatch.
Claimed, submitting or uncertain rows cannot use that recovery. Original
bindings, source references and outcomes remain retained. Receipt acknowledgements
lost in transit can be retried with their exact immutable identity/content.
Private replies are not automatically shared contributions or memories.

Deploy a reviewed, CI-qualified exact commit/binary at verified idle with a
SQLite backup, current configuration/binary hashes and rollback. Preserve the
existing native home, journal/offsets and topic/thread association; do not use
database restoration as routine cutover. Record actual installed hashes and
health separately from CI. This adapter is opt-in per receiver; never copy
another human's private Nexus state/routes into the tribal runtime package.

Qualification commands:

```text
cargo fmt --all --check
cargo clippy --locked --all-targets --no-default-features -- -D warnings
cargo test --locked --no-default-features
cargo build --locked --release --no-default-features
python3 -B tests/nexus_io.py target/release/telecodex
python3 -B tests/nexus_context_io.py target/release/telecodex
/local/nexus/.venv/bin/python -B tests/nexus_operator_io.py /local/nexus
```

The first journey uses synthetic peers over actual pipes/HTTP/WebSocket/SQLite.
The second uses Nexus's actual backend with synthetic sources/receipts. Neither
proves a real model or phone delivery. Acceptance still requires the human's
real-source Mini App → intended existing native conversation → actual reply,
correction/interruption and preserved return/close/reopen loop.

## Reviewed iteration requests

The same authenticated binding and dispatch ledger also accepts Nexus requests
with `request_kind: iteration`. Their frozen package contains the reviewed
iteration spec, selected node/relation versions and task states at admission.
The native prompt asks for scoped work through existing authorized work/provider
tools. It does not fabricate an audio contribution, grant new permissions or
retarget the package when navigation changes. Ingress, dispatch and retries
compare the complete frozen iteration; restart reconciliation does not resubmit.

A native response is a private conversation receipt, not a task or iteration
result. Provider observations and the Nexus work ledger own execution/completion.
The optional `tests/nexus_operator_io.py` qualification covers the real Nexus API;
`tests/nexus_io.py ... reviewed_iteration` exercises process/HTTP/WebSocket/SQLite
with a synthetic native endpoint. Neither claims a genuine model/provider effect.

## Narrated report reflection mode

The paired Nexus receiver may request `response_mode:"report_reflection"` for an
anchored report contribution. `/nexus connect` advertises this mode only in the
new receiver code; deploy a compatible Nexus backend before reconnecting. The
backend freezes the exact report/chapter/narration context and rejects use through
an older binding that has not advertised support. Existing ordinary contribution,
iteration and peer prompt paths retain their meanings.

The native reflection prompt asks for a brief source-grounded interpretation,
question or proposed action during continued listening. Room statements and ASR
remain evidence; they do not authorize execution, messages, issue creation or
audio deletion. Original discussion audio remains retained for hearing review,
with separate explicit human confirmation before deletion. Native completion
still requires an actual matched turn and final reply. Frontend polling reads
retained receipts; it never resubmits a question or starts another model.

## Report receiver adoption checkpoint — 2026-10-21

The CompAII receiver adopted integrated source `49cde4d` after exact-source CI
and a single-job release build. The existing writer-preserving route retained
the native daemon, existing topic/thread, active goal and canonical journals;
the successor collector was observed on the same active native writer. The web
configuration was unchanged. No global Stop or new model kickoff occurred.
The selected Nexus operator now uses the compatible report backend.

This qualifies receiving adoption, not a completed report reflection. Each
personal association still uses the authenticated `/nexus connect <table>`
operation in its existing human topic. Browser table access alone never creates
a private conversation binding, and another human's access does not enroll their
daimon. Real response/continued-listening and physical hearing acceptance remain
separate from source tests and service readiness. Exact receiving identifiers,
configuration and receipts stay in the private owning runbook.
