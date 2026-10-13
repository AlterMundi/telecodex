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

`/nexus open` returns the configured Main Mini App link. A start selector routes
the table; signed Mini App identity and membership remain Nexus's responsibility.
The link is not proof of launch or an exact private-topic return URL. The
existing Nexus frontend owns its verified conversation/return link, source
versions, playback state and close/reopen behavior. Group rooms, private voice
capture and per-message topic deep links are not qualified by this slice.

## Requests, observations and controls

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
/local/nexus/.venv/bin/python -B tests/nexus_operator_io.py /local/nexus
```

The first journey uses synthetic peers over actual pipes/HTTP/WebSocket/SQLite.
The second uses Nexus's actual backend with synthetic sources/receipts. Neither
proves a real model or phone delivery. Acceptance still requires the human's
real-source Mini App → intended existing native conversation → actual reply,
correction/interruption and preserved return/close/reopen loop.
