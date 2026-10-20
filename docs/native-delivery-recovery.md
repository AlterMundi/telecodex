# Native receipt and Telegram delivery recovery

Ordinary owner-authored direct topics on the shared native server retain completed
native items in the existing topic-publication queue. A unique native thread,
physical turn, item and destination identifies a publication; multipart receipts
survive restart. The queue sends independently of the native event pump. Explicit
Telegram rejection with retry_after defers the same known-unsent part. A lost
response after a possible send stays uncertain and is never blindly repeated.
`/status` separates pending Telegram delivery, uncertain delivery, native
observation and inputs already accepted by Codex. `/history` remains available
inside Telegram when an uncertain delivery needs inspection.

Native input receipts are independent of logical collector success. An exact
late acknowledgment or client-ID history match remains acceptance even when a
later goal continuation/collection fails. It never claims that the model used
the instruction or that the human received its answer. Other scoped participants
retain the existing input and standard status paths; owner-private recovery
metadata is not exposed to them.

The existing listener heartbeat performs up to four bounded native observations,
without another polling owner, model call, goal mutation or daemon restart. Only
new registered direct publication lifetimes are eligible. Before native
submission, their exact original client message ID is retained; a lost initial
acknowledgment can establish the first physical boundary only from an exact
native receipt. A definitely rejected candidate retires. Missing history is
unknown, never authorization to retry. Goal-active gaps remain observable.

Observation scans at most six twenty-turn pages and 8 MiB within eight seconds,
including the last checkpoint. It commits completed canonical items oldest first
into the same destination queue used by live events. Streaming partial items are
not imported as completed. A newer execution bounds the older observation; a
replacement collector on the same physical turn transfers the checkpoint. Owner,
native binding, source lifetime and publication selection are checked again before
effects. Missing boundaries and unobservable native state remain visible in status.
An internally generated preparation-failure notice can return to the same
authorized topic before any native binding is observed. It claims no native
acceptance or output; the owner and allowlist are checked before publication.

A proved terminal history observation releases a collector still waiting for a
lost event, without turn interruption. An orphaned existing active writer can be
reattached through the existing attach-active path; that path cannot start a turn,
resume a goal or replay a prompt. Paused/blocked goals and Stop/maintenance fences
remain intact. Routed secondary projection keeps its explicit opt-in; enabling
direct durability does not enroll ordinary messages into the integrator.

First adoption atomically freezes pre-upgrade canonical item IDs so attaching
to an existing writer cannot republish messages from its earlier presentation.
Existing durable publication receipts retain their original outcomes.

This upgrade does not replay legacy history: old publication receipts were not
persisted with sufficient identity to make every historical resend safe. Preserve
those records for explicit inspection. Roll back software only, never the input
journal, publication receipts or polling offsets. New additive tables preserve old opt-in registrations as mirrors; the old
two-column registration table remains unchanged for software rollback.
Explicit local session removal retires its publication parts and observation
checkpoints atomically with the existing turn cascade. Original ingress, native
acceptance receipts and independent archived request decisions remain retained;
the removal never recreates a destination or retries an uncertain send.

Qualification: SQLite tests cover late acceptance, failure/restart, owner/binding
changes, partial snapshots and observation boundaries. The native/HTTP journey
covers an eleven-second ACK, lost completion, explicit 429, restart deduplication
and a subsequent ordinary turn with zero inference. These tests are separate
from actual Telegram receiving evidence. Owning issue: #234; outbound recovery
also contributes to #103.
