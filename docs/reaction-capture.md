# Authenticated reaction feedback

Owning investigation: [#56](https://github.com/AlterMundi/telecodex/issues/56).

Telegram's [Update contract](https://core.telegram.org/bots/api#update) requires
explicit `message_reaction` subscription and administrator status. A historical
[private-chat payload](https://github.com/telegraf/telegraf/issues/1996) warrants
a controlled private-topic experiment; it does not establish current delivery.
The opt-in path retains events and delivers eligible reactions as quoted human
feedback in the next ordinary turn of the same native conversation. Private-topic
delivery remains unproven until an actual controlled exchange after activation.

## Enable in the existing listener

After qualifying the candidate and obtaining independent review, stop the owning
listener at actual idle. Take and verify a SQLite backup using the backup API,
preserving the existing configuration and binary for rollback. Record the exact
candidate commit, source and binary hashes, and build feature set privately.
Build with Rust 1.95.0 and `cargo build --locked --release --no-default-features`.

Add this setting to the existing `[telegram]` section:

```toml
capture_message_reactions = true
```

It defaults to false. Restart the same dedicated bot's sole listener with its
existing configuration, native home and database. Do not run a second
`getUpdates` consumer, reset offsets, create a webhook or substitute a user
account. Confirm the single listener is healthy and ordinary topic delivery
still works before asking for a fresh human reaction.

The listener requests `message`, `callback_query` and `message_reaction`.
Each delivered reaction is retained as complete event JSON inside the existing
`incoming_updates.payload_json`. The event and polling offset commit together;
existing deduplication, conflict detection and no-automatic-replay rules apply.
Absent reaction fields are omitted when serializing ordinary updates, preserving
compatibility with their existing journal entries.

Eligible feedback commits in that same transaction. The reacting user must be
allowed and own the target session; private-chat actors must match the chat.
The target must match exactly one confirmed assistant message in
`handoff_messages`, with the same current native binding as its recorded turn.
Anonymous/bot/disallowed actors and unknown, ambiguous or rebound targets remain
raw receipts only. Unsupported reaction representations also remain raw; their
contents are never guessed into another topic. No old journal is retrospectively
projected into a new binding.

Each feedback record preserves additions, replacements and removals, old/new
reaction lists, actor, update/date, chat/topic, message/turn and native provenance,
plus an assistant text excerpt (1,000 characters, explicitly marked if truncated).
Custom emoji remain opaque IDs. Records are delivered in update order, bounded
to 20 events/32 KB per ordinary turn; overflow waits for a later human turn.

Only a subsequent ordinary human message in that same session/native thread
can claim earlier events. Commands, review/goal activations, Stop resume, merges,
Nexus/Tribu requests, steering and question answers do not consume the queue.
Native input includes a labeled JSON block as quoted **user-origin data**, not
developer instructions, new authorization or the assistant's own experience.
The current human prompt and normal attachments still follow it.

Reactions alone send no reply, create no session, invoke/steer no native turn,
resume no goal and cannot answer an armed question or authorize an action.
They never trigger inference or paid work by themselves.

## Persistence and native receipts

`reaction_feedback` is a journal-linked SQLite queue with source binding and
delivery-turn provenance. Pending events survive restart. Preparation reserves a
batch; actor and actual resumed native binding are checked again immediately
before transport. A pre-transport failure releases that batch. Before native
submission it is durably marked transmitted. Explicit native rejection makes it
eligible for a later human request; native acceptance records accepted status.
An uncertain submission becomes unconfirmed after restart and is never replayed
automatically. Accepted feedback is not injected again after completion/restart.

Acceptance proves the native protocol accepted the input, not that a live model
understood it. Read-only operator inspection can distinguish the phases:

```sql
SELECT update_id, session_id, phase, delivery_turn
FROM reaction_feedback
ORDER BY update_id;
```

## Observe the controlled exchange

After activation, ask the human to add a reaction to a known assistant message
in the selected private topic, replace it and remove it, then send an ordinary
text message reporting those actions. A reaction alone creates no native turn.
An operator can inspect the existing database read-only, selecting events after
the experiment's starting update ID:

```sql
SELECT update_id, status, json_extract(payload_json, '$.message_reaction')
FROM incoming_updates
WHERE update_id > :starting_update_id
  AND json_type(payload_json, '$.message_reaction') = 'object'
ORDER BY update_id;
```

Keep payloads, IDs and private message content local. A saved event proves Bot
API delivery to this listener. A synthetic private-chat fixture proves only
bridge handling. No event during the controlled interval is an observation,
not a universal claim that private reactions cannot work. After actual event
receipt, verify the queued record's actor and target, then inspect the ordinary
turn's native input/acceptance separately from the final human-visible response.

Reaction events omit `message_thread_id` and original message text. For known
assistant chunks, correlate event chat/message IDs with `handoff_messages`:

```sql
SELECT u.update_id, h.thread_id, h.turn_id
FROM incoming_updates AS u
JOIN handoff_messages AS h
  ON h.chat_id = json_extract(u.payload_json, '$.message_reaction.chat.id')
 AND h.message_id = json_extract(u.payload_json, '$.message_reaction.message_id')
WHERE u.update_id > :starting_update_id
ORDER BY u.update_id;
```

This diagnostic join alone does not authenticate actors or establish native
delivery. Use the recorded feedback provenance and native receipt for those
checks. Capture does not subscribe to anonymous aggregate `message_reaction_count`.

## Disable or roll back

At idle, stop the same listener, set the option to false or remove it, and restart
it. Delivery of queued feedback is also disabled. Telegram may still deliver
previously queued reaction events; they remain raw receipts and are journaled.
Keep that journal private after the experiment.
For software rollback restore the preserved binary/configuration, never an old
database or polling offset. Already captured reactions remain preserved; an
older binary may have only receipt capture or no reaction support. Preserve the
new queue even if the older binary does not use it; no database downgrade or
historical replay is part of rollback.
