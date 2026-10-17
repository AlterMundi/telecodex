# Original conversation input

The owner-local controller accepts non-Telegram text into the configured native
integrating entry. It reuses the existing queue, native writer and active steer;
there is no second model session or fabricated Telegram update. The selected entry can be empty. Its first ordinary human message creates the
native context through the existing writer; explicit routes can reach workers
before the front context starts. Its logical input binding stays stable during
that initial creation, including while a voice transcript or question is pending. The Mini App includes authenticated text
input, retained-message inspection and native question/approval controls.
Microphone recording uses the same journal and writer. Receiving deployment requires the new
canonical-return policy in every relevant active collector before exposing input.

The optional microphone uses the existing receiving `[voice]` configuration and
its bounded ASR command. Talk starts microphone capture and pauses reply playback;
Send voice finalizes the clip. Discard, hidden/deactivated views and late permission
responses stop device tracks without submitting partial speech or cancelling work.
The 120-second limit stops recording for explicit send/discard rather than silently
submitting it. Unsupported browsers retain text input. Browser device access uses
[MediaRecorder](https://developer.mozilla.org/en-US/docs/Web/API/MediaRecorder) and
[getUserMedia](https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getUserMedia)
feature detection; actual Telegram client microphone permissions require receiving
qualification.

`POST /api/interaction/voice` takes bounded raw audio with `X-Input-Id`,
`X-Input-Binding`, `X-Audio-Sha256` and its supported audio Content-Type, plus the
existing Origin/authentication/CSRF checks before buffering a large upload.
The owner-private immutable file is retained before `input voice submit JSON`
reserves the original UUID, audio identity, owner/native/epoch and capture order
in the existing conversation journal. Up to two preparations run asynchronously;
text and independent workers remain responsive. The existing FFmpeg/ASR adapter
accepts final interpretations only. Its transcript and source hash are distinct
from verified written human speech and are attributed in both integrator and
routed-worker input. No Telegram file/update identity is fabricated.

Later text retains later receiver capture order even if the earlier voice ASR
finishes last. Grant/binding retirement and admission fences apply before and
after preparation and again at native write. Restart retains preparation
uncertainty without ASR or native replay. A lost upload acknowledgment is inspected
with the original input status; explicit retry preserves the same blob/hash/UUID,
and existing captures never start ASR again. Browser memory holds an unconfirmed
recording, never localStorage. Original audio remains receiving-local.

`telecodex --control CONFIG input submit JSON` accepts `binding`, `id`,
`text` and optional `reply: {source, revision}`. The binding comes from the authenticated presentation list, `id` is a
canonical UUID and text is bounded to 32 KiB. The receiving configuration selects
the human and integrating entry. The existing owner UID/socket protections apply.
The exact text is committed before transport. Reusing an ID inspects its original
outcome; changed text is rejected. A lost response is inspected with
`telecodex --control CONFIG input status UUID`, without automatic resubmission.

An explicit leading `#Topic` or `/self#Topic` routes text or a finalized voice
transcript directly to an enrolled work session, without an integrator model call.
A literal hashtag elsewhere remains ordinary conversation. Reply to this session
on a worker result or routed original freezes an owner-scoped source/revision;
the server derives the request lineage and destination. The next message amends
that work. An explicit selector overrides reply focus; a different session starts
independent work. Focus is consumed by one retained message. Microphone focus is
frozen when capture starts and survives permission delays, duration review and
explicit retries. Pending replies retire on target deletion, owner changes or a
replaced native binding; initial native establishment remains supported. Fresh
inputs can select the logical session's current generation after renewal.

Unknown or ambiguous selectors retain the exact original as `awaiting_target`.
The human chooses an enrolled destination through
`POST /api/interaction/inputs/{id}/route` with `{binding, selector}`. There is no
preselected destination. The first route is immutable; identical retry inspects
its outcome, and another target cannot reuse the same original. Queued transport
and native acceptance remain distinct, and stale status reads cannot reopen a
confirmed route. Browser destination choices are presentation, never authority.

The conversational controller can also read `pending_routes` and call
`resolve_route` with `pending_input` (the saved UUID), `input_ref` (a later
retained human utterance) and one enrolled `selector`. A finalized voice
transcript is eligible through the same original-input reference. The controller
routes the saved original text, not a model-authored replacement or the short
clarification as a new work task. `routing.selection = integrator_clarification`
and `routing.selection_input` retain the interpreter's selected destination and
the actual human clarification separately. When the intended saved message or
destination is unclear, the integrator must ask; this operation does not infer
that a busy worker needs a new conversation.

Capture order, rather than transcript completion time, must put the clarification
after its saved original. Ownership, native generation, input retirement and the
original first-selection checks run together in the admission transaction.
A repeated identical selection only inspects the retained result; another target
or clarification cannot replace it. This uses the existing writer, native steer,
return correlation and scoped interruption behavior. No automatic approval,
publication or global Stop is introduced. Native question/approval decisions
remain the separate request controls described below.

A scoped `#Topic /interrupt` uses only the cancellation token selected at input
arrival, including while voice transcription is pending. It never selects a newer
execution or invokes global Stop. Clarification after that original token scope
has become unavailable records `scope_unavailable`, not a claim of inactivity;
no interruption is made or replayed. A fresh explicit control selects current work.

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
controls have authenticated owner/HTTP endpoints and visible Mini App cards.
Retirement and restart
never resurrect a response channel. Generated files remain retained with their
native turn when automatic Telegram presentation is not selected.

`requests list CURSOR`, `requests detail ID OFFSET [REVISION]`,
`requests decide JSON` and `requests receipt ID REVISION` extend the existing
owner-local controller. Authenticated HTTP exposes the same operations at
`/api/interaction/requests`, `/api/interaction/requests/ID` and
`/api/interaction/requests/ID/receipt?revision=REVISION`. A decision POST requires
exact `binding`, `revision` and typed `action`, plus the existing live owner,
Origin and CSRF checks. Only offered approval choices are accepted. Questions
retain their original native question IDs and accumulate answers on that same
response channel. They never become new turns or fake Telegram updates.

Request discovery is scoped to currently owned native contexts whose canonical
return is attributed to the configured integrating entry, including derived
session-request helpers. Proposal text is returned in 4,000-character parts;
continuation reads must name the same revision. Lists return bounded previews,
not full proposals or ready-to-click approval controls. A revision binds exact
proposal/choices/question index to the owner, entry/source native bindings,
physical execution, process instance and admission generations. Native event
processing and additional steer input continue while requests wait.

The Mini App discovers bounded pages of requests and shows their source session.
A permanent discovery sequence survives native-row deletion and SQLite rowid reuse;
its order describes discovery, not original human chronology. Choice controls stay
disabled until the full proposal has been read. Changed proposals require another
explicit review, and unsent drafts remain available under previous drafts. Request
cards survive ordinary reply refreshes. Pending requests do not prevent new text
input or steering to the integrating conversation and its workers.

A lost decision acknowledgment retains the exact revision and action in memory.
Check decision reads the immutable receipt without another POST. Explicit retry
uses the same frozen decision; no automatic retry, approval or replay occurs.
Expired requests display closure even when the live responder and its question
index no longer exist. Native question and approval response channels are bound
to their physical turn and close when that turn ends or changes. An earlier
question's cancellation cannot interrupt a later goal-continuation turn.

Each exact revision/action commits an immutable receipt before consuming the
live responder. The independent `conversation_decisions` table retains the full
frozen proposal, question ID/index, resolved answer or approval choice and source
scope without foreign keys to deletable sessions or turns. Repeated identical
POSTs inspect that receipt; replacement decisions are rejected. Receipt reads
require the current owned entry and never recreate a response channel. Recording
an answer/approval does not establish native incorporation or execution.

Binding/owner changes, grant revocation and entry deletion retire pending native
request authority monotonically, including derived helpers selected through the
same lineage used for admission. Restoring the old values cannot reactivate a
proposal. Existing Telegram question/approval controls consult the same pending
state. Restart expires outstanding response channels while retaining decisions;
there is no automatic replay or approval.

A fully read current question can also select **Answer in conversation**. This
freezes `reply = {source: "request:ID", revision: REVISION}` for one ordinary
composer message or microphone capture. Native approvals keep their existing
explicit controls. Selecting focus does not answer or start recording.

Fresh capture validates the live owned question; finalized text is applied only
to that exact revision through the existing responder. The decision receipt
retains `source_input`, and the same transaction marks the original `answered`.
Voice answers carry the retained automatic-transcription attribution rather than
claiming verified written human speech. `answered` means the decision was recorded,
not that native incorporation or task completion was observed. No new model
session or work turn is needed, including for a cold integrating entry.

A question that advances or closes during transcription cannot consume that old
recording as its next answer. Its original and transcript remain inspectable
without a second submission. Identical capture retries inspect the original
receipt even after the question closes or the receiver restarts. A deliberate
leading `#Topic` overrides question focus and uses the normal work route instead.

`tests/conversation_requests_ui.js` and the host conversation integration tests
qualify full-source gating, source changes, saved drafts, frozen retries,
receipt-only recovery, terminal closure and overlapping scope changes.
`tests/request_turn_lifetime_io.py` qualifies an earlier question and approval
expiring while a native goal advances, without interrupting the later turn.

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

## Reported playback context and semantic intent

A device may report the question or worker result that was playing by adding
`context_only: true` to its original input's existing `reply` reference. That
immutable hint is context, not a destination or an answer selection. Plain text
and committed voice with this hint enter the integrating native conversation;
the existing explicit `#Topic` shortcut still takes precedence. An unavailable
or advanced reported source does not block a new human message. Its hint is
untrusted device metadata and never establishes hearing or expands ownership.

For finalized voice, `X-Reply-Source` uses `context:request:ID` or
`context:delivery:UUID`; the dashboard decodes this to the typed contextual
reference. Older dashboards reject this source form before ASR instead of
silently answering a question. An ordinary `request:ID` header and the existing
explicit composer reply keep their direct-answer behavior. Context mode is part
of the immutable original: an uncertain clip cannot be retried under another
mode, source or revision.

The stable `interaction_controller` envelope discovers two operations:

- `questions` takes an original `input_ref` UUID and an optional `after` cursor.
  It pages through owned pending native questions, excluding approvals. Each
  item reports whether its exact version was present when that original was
  captured. Inspection does not select an answer or a destination.
- `question_context` takes an original UUID, an optional explicit
  `question: {id, revision}` from that catalog, and a character `offset`. Without
  an explicit selector it reads the question reported by contextual metadata.
  It exposes the full bounded proposal, `matches_current` and
  `available_at_capture`; these are evidence for interpretation.
- `answer_question` takes that original UUID and an optional explicit question
  selector. It never accepts model-authored answer text. Without a selector, it
  preserves the frozen contextual-question contract. With an explicit selector,
  the controller atomically requires that exact owned question version existed
  at capture and remains current. The answer uses only the retained human text
  or attributed transcript. A reported playback context may point elsewhere; it
  remains immutable context, not a destination or a veto on a clear new intent.

New originals retain bounded opaque versions of up to 128 current canonical
questions in the same capture transaction, before ASR. Bodies are not copied
into originals. The native input envelope reports how many question versions
were retained and prompts current catalogue inspection when the utterance could
answer work; it does not force an answer or claim hearing. Versions bind the current proposal (including options), question
index and owned native scope, rather than a presentation timestamp. Reopening
unchanged controls does not invalidate an original; even an identically worded
next step is a different question version. An advanced question, including the next part of the same native
request, or one created after capture cannot consume an older original through
explicit selection. Other questions advancing do not invalidate an unchanged
question. Older originals without that capture evidence remain readable and
keep their existing frozen-context behavior; explicit selection cannot invent
a missing snapshot. Approval actions remain outside semantic answering.

The current owner, entry/native binding, admission epochs, live responder and
interpretation, question revision and original consumption are rechecked at
commit. An original already routed or reserved for creation cannot also answer,
and a consumed answer cannot later dispatch. The existing decision journal
retains the exact original reference and actual selected question, including for
unhinted text. Identical retries inspect that receipt without answering again.

The integrating model decides whether to answer, discuss ambiguity or use its
existing routing/creation tools. A result being played must not force a later
utterance into that session. Once a question consumes an original, that original
cannot subsequently be dispatched as a new task; a committed dispatch/creation
reservation likewise prevents answering it as well. A late native ingress ACK
cannot reopen an answered original. Approvals are not answerable through this
operation. Native uptake and recording an answer still do not prove task
incorporation or human comprehension.

`tests/conversation_context_http_io.py BINARY` qualifies actual HTTP/native/SQLite
behavior with synthetic native/ASR peers: a new topic while Alpha waits, an
original voice answer after explicit semantic tool selection, stale/unavailable
context, competing use and idempotent receipts. It uses zero inference and does
not establish real classification quality or same-entry blocking-question
behavior in a live model runtime.

Select the contextual-capable native controller before the matching dashboard
and companion client. No SQL table migration is needed; old originals still
omit the optional mode. After retaining contextual originals, an older native
binary whose strict reply decoder lacks this field is not a compatible software
rollback. Recover forward or retain the new decoder when reverting behavior;
never restore an earlier input journal or reinterpret those originals as direct
answers. An old dashboard's rejection is not permission to resend a clip under a
new identity or mode.

## Audible destination clarification

An eligible `awaiting_target` original exposes a `route_clarification`
presentation source named `input:UUID`. Its deterministic question quotes a
bounded excerpt of the retained original and lists enrolled conversation labels.
It does not require another inference call. The revision binds the current
logical entry generation, original and complete available choices; initial
native-context assignment alone does not invalidate it. Resolution, retirement,
ineligibility or changed choices invalidate queued or rendered old questions.

The companion discovers these descriptors through original receipts and paged
history, using the existing source/part/audio and playback revalidation path. A
later utterance carries `context:input:UUID`, with `context_only: true`. That hint
never selects a destination. The integrator may inspect `pending_routes` and use
`resolve_route` with the exact pending UUID and the new retained input reference.
The existing immutable selection and capture-order checks still apply: the busy
worker receives the original task, while the clarification remains attributed
to its own original. An unrelated new task remains possible.

The actual HTTP/native/SQLite clarification journey and GTK/XTest/TLS companion
journey pass with synthetic native, ASR and speech peers and no model inference
or physical sensors. The latter also covers a semantic native-question answer,
a concurrent worker result and five selected capture stops with three voice
submissions. Provider tests cover multipart questions, stale sources and choices
changing during synthesis. These results qualify the mechanism, not real-model
interpretation or audible-device acceptance. Deploy the supporting native
controller before its matching dashboard and client. Older source decoders
refuse this context form; never resend an original under another identity.

Each native entry input includes its immutable original UUID in controller
metadata, including ordinary text without a playback hint. The bounded recent
input list is navigation, not a limit on which retained input may be selected.
The twenty-input native/controller regression routes the oldest exact original
after it falls outside that list, including preparation-time coalescing or
steering. Metadata does not enter the human original or worker task text.
The fixture uses synthetic native peers and zero inference.

The unhinted-question native HTTP/SQLite regression covers independent questions,
explicit interpretation despite another playback hint, unchanged native control
reopening, identical-text next steps, late ASR, old originals without snapshots,
approval exclusion, dispatch/creation exclusivity, exact answers and receipts
after restart. Native/ASR peers are synthetic and no model inference or physical
sensors are used; real interpretation quality is separate. Existing direct
question-answer and finalized-voice journeys also pass this source.
