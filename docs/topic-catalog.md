# Topic metadata and lifecycle

The bridge retains dated Telegram topic facts separately from native thread titles,
first-message summaries, goal state and execution state. A legacy `session_title`
is not proof of a Telegram name. Discovery never enrolls ordinary direct messages
into routing. This implementation belongs to #108 under Interaction #208.

The existing authenticated update consumer handles `forum_topic_created`,
`forum_topic_edited`, `forum_topic_closed` and `forum_topic_reopened` as metadata,
without a native turn, router admission or conversation-activity timestamp.
Creation receipts and acknowledged bridge renames, icons, close/delete operations
also retain their observed fields. Failed or false acknowledgments do not assert
a successful change. Metadata-retention errors after acknowledged creation are
reported without disguising the known creation as a failed operation to repeat.

Each name, icon component and lifecycle value carries its own observation time
and source. Telegram update IDs order fields from that consumer; an older event
cannot overwrite a newer field. Missing names/icons do not erase known values.
A delayed creation receipt cannot reset a subsequent rename or close. An explicit
empty custom-emoji ID is a known reset, preserving the separately observed color.
These are observations, not a live enumeration or proof that a topic still has
that state. Missing lifecycle evidence remains unknown. No second update consumer
or model wakeup is introduced.

## Owner-local inventory

```sh
telecodex --control CONFIG topics list 7
```

The finite inventory includes only the configured human's allowed sessions,
with native bindings, unverified title summaries and retained Telegram facts.
Telegram’s `is_name_implicit` flag is preserved; an implicit creation name is not
registered as an explicit human title.
`native_execution` and `goal_state` remain `unobserved` in this metadata-only
adapter. `bridge_busy` does not substitute for native status. Conversation
recency is explicitly scoped to attributed bridge turns, excluding metadata
reads, renames and polls; native-only conversations are not inferred absent.
The window accepts 1–365 days. The 512-row bound reports truncation.

Use the existing explicit `/rename` path to reconcile a selected topic, bridge
name and native name; its response distinguishes partial acknowledgments.
Do not infer legacy names from native titles or backfill them from first-message
summaries. Unknown metadata needs retained evidence or human confirmation.

The next #108 step connects the existing activity observer's goal/execution
observations to compact counts and reversible status decoration, preserving
canonical human titles/icons. That step and receiving qualification are not
claimed by this catalog implementation. #215 separately owns attributed exchange
projection and shared publication deduplication.

Wire fields follow the [Telegram service-message contract](https://core.telegram.org/bots/api#forumtopiccreated), including explicit empty emoji resets.
