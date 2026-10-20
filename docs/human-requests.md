# Visible and recoverable human requests

This change implements [#43](https://github.com/AlterMundi/telecodex/issues/43)
using the existing native question broker and activity observer. It does not
add a native RPC queue, model attention, memory ingestion or a Mini App.

## Conversation first

Human clarification, 2026-10-19: a pending request must be easy to understand
from a short phone message or spoken exchange. Lead with the one concrete action,
briefly explain why it matters, and state how to answer. Keep technical evidence
and history available in detail rather than making the human reconstruct them.
Do not bundle an immediate handoff with eventual project acceptance in one task.

When ordinary conversation supplies the answer, associate that actual human input
with the intended current request through the supported responder/feedback path;
do not demand duplicate form entry. Ambiguous replies need conversational
clarification. If continued work makes the requested action unnecessary, withdraw
the agent-issued task with a short reason and retained evidence. Withdrawal is
not a declaration that the human performed the action or accepted the product.
Never manufacture an answer to an expired native channel. The existing manifest
withdrawal mechanism below is available; integrated reassessment and voice
usability still need qualification under #61/#208/#210.

Ordinary conversation is the default. Discuss ideas, acknowledge spontaneous
feedback and answer status questions directly. Native question widgets are useful
when the human requests one or a consequential choice needs structured input.
A durable task is for a concrete important external action that needs to remain
findable after the turn, such as a necessary phone test or private credential
handoff. Routine confirmations, optional suggestions and conversational questions
do not need a widget or a task. Do not redeclare an existing pending action.
The runtime names this distinction in each turn's application context; it does
not infer or create requests from ordinary messages.

## Reopen without scrolling

`/pending` and `/questions` are equivalent. They show this allowed human's
unresolved requests in the selected topic, with lifecycle at the start of each
short mobile label. **History** opens answered/completed/cancelled/withdrawn and
expired/source-changed records separately. Unconfirmed receipts remain visible
for inspection without a new answer channel or replay. Pending, receipt and
history counts cover the whole human/topic scope, not the current page. Pages
hold up to 50 entries and clearly show the displayed range.

Set `telegram.human_request_language = "es"` for Spanish list labels/headings or
`"en"` (default) for English. This listener preference does not alter native
question content, task text, incoming-update serialization or authorization.
A compact count and **Open human requests** button appear on the activity card
and `/status`. The observer reads only scoped ledger metadata, never task text.
Automatic cards count the topic creator's requests; requested `/status` counts
the actual sender's requests. A shared-topic reader cannot use the button to
open another human's requests.

A pending human action alone can leave a silent attention card after 30 seconds
without conversation exchanges. It says **Human requests**, not **Working**;
it does not count as active native work in health or prevent an idle upgrade.
Later exchanges retire and relocate the card using the existing observer rules.
Answering or cancelling the last pending item removes that attention card.

## Two distinct lifetimes

Native requests retain the existing original thread/turn/item/request IDs,
requesting human and topic. Blocking and nonblocking requests can coexist with
continued commentary. Reopening refreshes the current answer controls. Answers
use the original RPC once; they never become a new model turn. Native resolution
acknowledges receipt, not that a requested real-world action occurred.

The ledger records pending, answering (receipt unconfirmed), answered,
cancelled, expired and unknown outcomes. Native completion expires an unanswered
question. On bridge restart, old pending native channels expire and unanswered
receipt attempts become unknown; no response channel or input is recreated.
Saved records remain readable. A changed native binding is explicit and cannot
redirect an old answer to a replacement session.

A durable human task survives the native turn and bridge restart. **Reply with
feedback** opens ForceReply and explicitly selects the next text from the same
human in this topic. This also works when a Telegram client sends no task quote
or attaches an unrelated reply instead. A directly quoted task remains supported.
Commands and attachments do not consume the selection; `/stop` clears it.
Selecting another task or native **Other answer** replaces the selection. The
selection exists only in this bridge process: after restart, reopen the pending
task and select **Reply with feedback** again. Explicit expired task/native
quotes never consume another selected answer. Feedback is recorded and forwarded through
the existing native steering/turn admission path, on the same source binding.
Its answered state means the human feedback was recorded. Its detail separately
shows the original input journal's delivery state, including unknown outcomes;
it does not assert native acceptance. **Done** explicitly records human-reported
completion and **Cancel request** cancels only that task. Neither starts inference.
An agent withdrawal is separately attributed and displays **Withdrawn**, with
its explicit reason. It does not assert human completion or product acceptance.
The original task text/source and prior resolved records are retained.
Old controls or quoted replies after resolution cannot create a new turn.

## Explicit declaration through the current turn workspace

Each native turn's application context names an optional `human-requests.json`
file beside `out`, not inside the upload directory. A successful native turn can
write the following file to declare human actions that must remain recoverable:

```json
{
  "schema": "telecodex.human-requests/v1",
  "requests": [
    {
      "id": "phone-playback",
      "title": "Test playback on the phone",
      "action": "Open the app, try playback, and reply with what happened.",
      "links": [{"label": "App", "url": "https://example.org/app"}]
    }
  ]
}
```

Use at most 10 requests per turn, unique IDs up to 64 ASCII letters/digits,
dash or underscore, titles up to 120 characters, actions up to 8000 characters,
and up to five HTTP(S) links (80-character labels, 2048-byte URLs, no URL
credentials). Never include secrets or private endpoints in previews or links.
The declaration is bounded to 64 KiB and must be a regular file, not a symlink
or FIFO. The whole declaration is validated before one SQLite transaction;
invalid declarations cause a visible delivery failure, retain the file for
explicit recovery and admit no partial tasks. Ordinary prose admits no task.

The bridge derives scope and provenance from the admitted current turn: the
manifest cannot supply a human, topic, source thread or another turn. It is not
uploaded as an artifact. This workspace transport works with existing warm
native sessions; it requires neither dynamic-tool registration on thread/start
nor a native fork. A task is not marked complete by elapsed time or successful
code/tools. Declare only actions that remain for the human at completion.

## Withdraw an obsolete agent-issued task

The same bounded manifest may include optional `withdrawals`. An empty or absent
`requests` list is valid. For example:

```json
{
  "schema": "telecodex.human-requests/v1",
  "withdrawals": [
    {
      "id": "retained-server-request-id",
      "reason": "The requested handoff is now observed. This does not approve the product.",
      "links": [{"label": "Handoff evidence", "url": "https://example.org/receipt"}]
    }
  ]
}
```

Use the exact retained task ID, not an invented ID or its originating manifest's
per-turn client ID. Obtain it from authorized metadata for this human/topic;
never scan another conversation's tasks. The manifest cannot provide a human,
topic, thread, source turn or completion state. There are at most ten combined
requests/withdrawals, distinct IDs, a nonempty reason up to 2000 characters, and
the same bounded public evidence-link rules as new tasks. Reasons/links report
an observed source or a deliberate retirement decision, never instructions.

The bridge derives the operation's provenance from the successfully completed
native turn's observed thread and admitted human. The store verifies that current
binding/turn and the original task's exact human/topic/native thread, then applies
new tasks and withdrawals in one transaction. Only a still-pending agent-issued
task can be withdrawn. Native questions, unconfirmed receipts and already
answered/completed/cancelled tasks are refused, preserving their outcomes. Any
invalid operation rolls back the entire declaration and retains its manifest.

Withdrawal closes the task in the existing ledger with separate agent provenance
(`source.agentWithdrawal`: reason, links, current source turn and time), and an
explicit `human_task_withdrawn_by_agent` audit event. An identical repeated
withdrawal is a no-op; changed content is refused. The persisted state remains
`cancelled` for older-runtime compatibility and is projected as `withdrawn` by
this version. Original text, source references and history stay intact. Retired
text selection is cleared; stale buttons and quoted replies cannot revive the
request or become a fresh native turn. Withdrawal never creates inference,
marks a human action complete or establishes phone/product acceptance.

## Qualification and deployment

SQLite tests exercise atomic admission, multiple scopes, native ID types,
restart recovery, binding replacement, answer receipt uncertainty and duplicate
controls. Loopback HTTP/WebSocket journeys exercise simultaneous blocking and
nonblocking questions, exact reopening and single native response, durable
phone tasks after completion/restart, ForceReply, cross-user/topic controls,
stale controls and rejected duplicate/symlink declarations. Observer HTTP/SQLite
tests cover scoped counts, expired owners, button edits and idle attention.
These fixtures do not establish human Telegram acceptance.

Upgrade the bridge and observer together from an exact reviewed, CI-qualified
revision, while idle. Preserve the private configuration, native bindings,
SQLite journal and polling offsets; take and verify a SQLite backup. The additive
ledger is harmless to the older binary. Roll back software, never the conversation
DB. Installing reusable skills alone does not activate the listener or observer.

## Live reassessment through the existing controller

The stable controller now advertises `human_tasks`, `human_task` and
`withdraw_task` for integrating and enrolled working sessions, including their
existing live terminal fallback. No new native thread or model loop is needed.
`human_tasks` provides at most ten active task summaries per page (`after` cursor,
`more` flag); `human_task` reads the selected full action and revision. Task text
is session-authored source data, never new human speech or an instruction to
execute an external action.

Workers can inspect/retire tasks only from their own current source binding. An
integrator can also reassess a task whose original local turn is proven to be
routed from that exact entry. Knowing a topic or having routed to its native
thread earlier is insufficient: a task from a later ordinary direct turn is not
silently imported. Ownership, allowed human, source binding and actual currently
running controller execution are checked on reads and again in the retirement
transaction.

`withdraw_task` takes the exact retained `id`, inspected `revision`, a short
`reason` and optional existing HTTP(S) evidence `links` (`label`/`url`). The
SQLite immediate transaction refuses a changed, already answered/completed,
uncertain or out-of-scope action. It reuses the manifest withdrawal mutation and
audit, preserving the original human requester, task text and source. It records
the actual withdrawing session/native physical turn separately. An identical
current-execution retry reads the retained outcome without a second retirement
or audit; changed retries fail. Native questions and approvals are never targets.

The result says **Withdrawn**, not human completion or product acceptance. The
same capture-arming lock retires a selected ForceReply so unrelated later text
cannot be consumed by an obsolete task. Existing `/pending`, history, observer
metadata and task controls read the same ledger; no parallel request store,
responder or human impersonation is introduced. This is development work for
#61/#208: new real-controller process cases use synthetic model peers and do not
replace the finished-version real-session/phone/voice campaign.
