# Experimental reaction receipt capture

Owning investigation: [#56](https://github.com/AlterMundi/telecodex/issues/56).

Telegram's [Update contract](https://core.telegram.org/bots/api#update) requires
explicit `message_reaction` subscription and administrator status. A historical
[private-chat payload](https://github.com/telegraf/telegraf/issues/1996) warrants
a controlled private-topic experiment; it does not establish current delivery.
This option records receipt only. Reactions do not reach native Codex context yet.

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

Capture does not authorize the actor or resolve a topic. It retains anonymous
actors, unknown targets and future reaction representations for diagnosis, under
the same private journal boundary as other received updates. It sends no reply,
creates no session, invokes no native turn or steering, and cannot answer an
armed question. Do not interpret an emoji as consent or an instruction.

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
not a universal claim that private reactions cannot work.

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

Unknown or ambiguous targets must remain unresolved. This diagnostic join does
not authenticate the actor, publish feedback, or deliver native context.
Capture does not subscribe to anonymous aggregate `message_reaction_count`.

## Disable or roll back

At idle, stop the same listener, set the option to false or remove it, and restart
it. Telegram may still deliver previously queued reaction events; they remain
receipt-only and are journaled. Keep that journal private after the experiment.
For software rollback restore the preserved binary/configuration, never an old
database or polling offset. Already captured reactions remain preserved; an
older binary has no reaction capture support.
