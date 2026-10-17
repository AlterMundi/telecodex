# Original conversation input

The owner-local controller accepts non-Telegram text into the configured native
integrating entry. It reuses the existing queue, native writer and active steer;
there is no second model session or fabricated Telegram update. The entry must
already have an owned native binding. The Mini App includes authenticated text
input and retained-message inspection. Microphone and native question/approval
controls are still being integrated. Receiving deployment requires the new
canonical-return policy in every relevant active collector before exposing input.

`telecodex --control CONFIG input submit JSON` accepts only `binding`, `id` and
`text`. The binding comes from the authenticated presentation list, `id` is a
canonical UUID and text is bounded to 32 KiB. The receiving configuration selects
the human and integrating entry. The existing owner UID/socket protections apply.
The exact text is committed before transport. Reusing an ID inspects its original
outcome; changed text is rejected. A lost response is inspected with
`telecodex --control CONFIG input status UUID`, without automatic resubmission.

The existing Telegram-signed dashboard session exposes `GET /api/interaction/inputs`
with `before` and `direction=before|after` cursors, `GET /api/interaction/inputs/UUID`
and `POST /api/interaction/inputs`. POST accepts exactly the same three fields and
requires the live owner grant, exact Origin and CSRF token. Owner/configuration
checks run again after control I/O, before returning private data. The Rust journal
owns idempotency; HTTP introduces neither another original store nor a native client.
The control request/reply bound is 256 KiB so JSON escaping can represent any valid
32 KiB original. Only this HTTP input endpoint uses that larger body bound; other
POST endpoints retain their 16 KiB limit, including for chunked bodies.

`input list CURSOR [before|after]` returns eight bounded original previews and
pagination metadata for the configured owned logical entry. Status returns the full
original. History remains inspectable after native rebinding, with retired authority;
another owner or deleted/recreated logical entry cannot inherit it. The client
catches up forward without skipping intervening pages, separately from older history.
Messages and credentials are not persisted in browser storage. Reload recovers
retained messages from the owning journal.

The composer preserves edits typed while a POST is pending. A lost acknowledgment
keeps the original UUID, frozen binding and exact text in memory, with a status
lookup and an explicit retry of that same identity. Retrying an already-retained
UUID only inspects its outcome; it does not replay native work. A request that never
reached the receiver can therefore recover without inventing a replacement ID.
There is no automatic retry. Terminal native acceptance, known-unsent and retirement
evidence survives overlapping HTTP responses. Retention or native ACK never claims
semantic incorporation, task completion or physical hearing.

`tests/conversation_http_io.py` qualifies authenticated HTTP through the actual
owner socket, SQLite and synthetic native peer, including worst-case escaped
32 KiB originals, pagination, deduplication, uncertainty, reopen, chunked bounds
and revoked grants. The shipped client regression tests cover response ordering,
offline/lost POST recovery and concurrent drafts without model calls.

Queued and steering inputs retain their original owner, native context and Stop
admission epoch. Grant revocation, binding retirement or deletion permanently
retire their execution authority; restoring current values does not revive them.
Writer maintenance is checked again at the native-write boundary. Restart retains
known-unsent inputs separately from uncertain writes. A matching late native ACK
can resolve uncertainty for the same execution, without another submission.
Cancellation that the native provider observes before its initial write produces
a typed never-submitted result, including corrections folded into that packet.
Retention and native acceptance are separate from worker incorporation.

Admitting an input selects canonical-conversation presentation for that native
context before possible native uptake. The durable selection also applies to an
already-running Telegram-root turn, later turns and receiving collectors. Live
text, finalization, artifacts and native human requests consult that selection.
It follows the native context across logical-session recreation; a new native
context has its own selection. Earlier publications already in transport are not
retracted by selecting another return surface.

Native questions retain their existing response channel and full request in the
scoped human-interaction store. Approvals likewise retain the exact proposal,
allowed choices and pending responder. Selecting canonical presentation neither
publishes these requests to Telegram nor grants an approval. Canonical request
controls still need their authenticated client integration. Retirement and restart
never resurrect a response channel. Generated files remain retained with their
native turn when automatic Telegram presentation is not selected.

`tests/conversation_input_control_io.py` uses the actual owner socket, listener,
SQLite and synthetic native/Telegram peers to qualify original-text preservation,
deduplication, steering, late ACKs, held resume, authority retirement and mixed-origin
presentation. Tests use no real models, Telegram accounts or external providers.

The controller discovers original references through its existing `inputs`
operation. `dispatch`, `create`, `creation_status` and `creation_resume` accept
`input_ref`: a Telegram update integer or a canonical conversation UUID. The
historical `input_update` argument remains accepted. The exact original is read
under its owner, native binding, captured epoch and monotonic retirement; the
model cannot supply replacement human text. Native dispatch preserves that frozen
epoch through every selected target and validates original authority again before
writing. Creating a new worker selects canonical presentation before native
preparation, including typing, and retains the native-context policy when bound.

Telegram returns keep their original message ID. Canonical returns have no
Telegram message or update ID and do not start a Telegram delivery. Results remain
in the existing canonical presentation store. Operational notices are attributed
`canonical_routing_notice` audit events, not human speech or delivery receipts.
The source-table migration preserves existing sources, request dependencies and
foreign keys while allowing a null Telegram return message. Historical creation
storage retains its `input_update` name; SQLite stores real conversation UUIDs as
text alongside existing integer updates, without inventing an update number.

New original captures receive a shared receiver ordinal in their existing SQLite
admission transaction, across Telegram and canonical text. Controller input lists,
worker provenance and recent target context retain both `input_ref` and
`capture_order`. The order describes receiver capture, separately from dispatch
order and client UUIDs. Legacy rows without that evidence remain explicitly
unknown; no ordering is backfilled from inferred times. Routing older corrections
after newer ones therefore preserves the newer original's identity and precedence.
`tests/conversation_routing_io.py` qualifies this through actual owner/native/store
paths with synthetic peers, parallel workers, reversed correction dispatch,
canonical results and creation/restart. It invokes no real model or provider.
