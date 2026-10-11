# Visible and recoverable human requests

This change implements [#43](https://github.com/AlterMundi/telecodex/issues/43)
using the existing native question broker and activity observer. It does not
add a native RPC queue, model attention, memory ingestion or a Mini App.

## Conversation first

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

`/pending` and `/questions` are equivalent. They list the requesting allowed
human's requests in this chat/topic, pending first, followed by recent resolved
records. Pages contain up to 50 requests with Previous/Next navigation. Select
an item to reopen its current native question or inspect its saved human task.
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
