# Explicit conversation context integration

`/merge` exchanges selected context between conversations owned by the requesting
human in the same bot/chat. It retains each source's native history and settings.
It never copies goals, pending requests, processes, identity or permissions.

From conversation A:

| Command | Destination |
| --- | --- |
| `/merge B` | Existing A |
| `/merge B --new "Shared exploration"` | A new named topic/native conversation |
| `/merge B --both` | Existing A and B, with a different synthesis for each perspective |
| `/merge B --focus "Memory and experience"` | Guide relevance in any mode |

Use the exact source topic name, quoting it when useful, or `#topic-id`.
Ambiguous names return the matching owned topics rather than choosing one.
Both source conversations must have existing native bindings. Policy-controlled
Tribu topics and cross-human/cross-bot delivery are outside this initial adapter.

## Source and interpretation

Both source snapshots precede either delivery. Each reader requests at most six
pages of 20 native turns, with a 48 KB selected-text budget per source and at most
4000 characters per text item. Source references and completed boundaries are
retained, with a deterministic selected-snapshot revision. This is a bounded
excerpt, not an assertion that the whole history was included. Cursor repetition
or unavailable source text refuses preparation before destination mutation.

Completed history and provisional observations retain their actual native status.
Unfinished or failed work is not a completed decision. Previously imported merge
packets retain their provenance marker instead of circulating their whole content
as a new contribution. Subsequent original conversation responses can still add
new perspectives; this does not establish a new fact about the original event.

Schema `telecodex.context-merge/v1` and interpretation policy `merge-research/v1`
are separately versioned. The shared tool-disabled, ephemeral handoff summarizer
retains prior understanding, received contributions, corrections, superseded
suggestions, alternatives, inspiration and questions. Divergence need not produce
agreement. Received context is attributed material, not firsthand experience or
new owner instructions. HMK qualification can refine this small policy seam;
implementing the command does not wait for that separate research.

Prepared packets and their source cuts are persisted before creating a topic or
submitting destination input. Recovery does not resynthesize a different packet
under the same identity. Selected real usage can feed HMK diagnosis through its
authorized research route; packet receipt alone is not evidence of understanding.

## Native delivery and recovery

An active destination uses its existing steering channel. An idle destination
queues an explicit context-incorporation turn on its existing native worker.
A new topic uses the normal topic initializer and title reconciliation.
Native goal controls and ongoing work remain on their existing threads.

`/merge status` displays retained operation IDs, destinations and per-leg states.
`/status` and the activity companion also show preparation and receipt state.
Bidirectional delivery has two independent receipts: one may succeed while the
other is unconfirmed. No atomic transaction across native conversations is claimed.

Native `turn/start` and `turn/steer` acknowledgements are correlated separately.
Physical-turn response and terminal events are retained even when they arrive
before an acknowledgement; startup reconciles those records without inference
or input replay. Known failed/interrupted outcomes retain native acceptance.
An observed response remains separate from semantic incorporation, which needs
source-grounded model/human qualification.

After restart, an unsent queue remains retained. A detached native writer is
preserved and its packet waits for explicit recovery. `/merge resume OPERATION-ID`
resumes only a known unadmitted leg, under current owner/binding checks, using the
immutable packet. Accepted or unconfirmed input is never resent by that command.
`/retry` cannot replay a context-merge turn. A creation timeout retains the operation
and prepared packet; inspect its topic receipt/audit instead of creating another
topic automatically. Source bindings, journals and polling offsets are preserved.

## Qualification and publication

Use the committed locked checks and `tests/merge_io.py`, plus relevant fork/goal
regressions. Synthetic HTTP/WebSocket/SQLite results, production activation,
actual native contextual uptake and phone acceptance are distinct evidence.
After human acceptance, publish the exact qualified runtime and portable procedure
through AlterMundi/Skills and the authorized receiving cohort. Check actual upstream
prerequisites before proposing a focused backport. Owning issue: [#64](https://github.com/AlterMundi/telecodex/issues/64).
