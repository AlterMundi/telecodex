# Compact native activity in Telegram

The optional `scripts/activity_indicator.py` observer maintains one silent,
editable activity message per authorized Telegram topic. It works with turns
already running, including CLI continuations of the topic's bound native thread.
Starting it does not restart Telecodex or the native daemon. The main bridge
remains the only Telegram update consumer.

A single silent message is maintained per topic and edited in place while
there are no new exchanges. If a human message, slash command or assistant
response follows the card, the observer removes its old card. After the next
thirty seconds of silence it posts the current status at the bottom of the
conversation. Subsequent updates edit that new card until another exchange.
The message appears when native work is active and
thirty seconds have passed since the last human/assistant exchange. It updates at most every thirty seconds
and summarizes elapsed time, outstanding native tool calls, active child agents
and native approval/input waits. Silent ordinary work also shows `Working`:
native activity does not guarantee Telegram typing, and the Bot API does not
expose typing visibility for this observer to query. Recent exchanges keep the
conversation clean; reasoning and changing tool identities do not reset the
silence grace. Completed final answers still retire the indicator unless actual
pending work remains.

The line also shows the available weekly allowance from native
`account/rateLimits/read`, cached for sixty seconds. The weekly window is
identified by its seven-day duration; the Codex bucket is preferred when multiple
buckets are returned. Missing or unavailable quota is shown as `weekly n/a` rather
than guessed. This read starts no inference.
The same weekly window's native `resetsAt` supplies a compact countdown, such as
`weekly 78% available · reset in 2d 3h`, in both the indicator and `/status`.
The countdown is recalculated from the cached reset timestamp whenever displayed;
it needs no additional quota requests. Missing reset metadata shows `reset n/a`.

There is no redundant `updated` clock in the message. A new exchange
replaces the previous indicator with a silent message after that exchange,
so editing an older message does not leave all subsequent requests hidden above
the conversation. Completed native final answers end the visible request even
when steering keeps the outer native task open. Pending tools, child agents and
native approval/input waits still keep the indicator active.

With explicit `--status-requests` opt-in, plain `/status` receives a compact
activity/quota response in addition to the bridge's existing session details.
While work is active it creates or immediately edits the same live indicator;
repeated commands do not leave separate frozen `Working` replies. An explicitly
requested indicator continues updating every thirty seconds while it is the
last message and is removed when its work ends. Subsequent exchanges retire it
and restore the automatic thirty-second silence grace. Idle and unavailable results remain ordinary
snapshots. A stored native thread reported as `notLoaded` is idle,
not a connection failure; linked active children still take precedence. A separate command reader checks the existing journal
every 250 ms and replies from the most recent activity/quota snapshot, without
waiting for the eight-second native observation sweep or making a new native
request. Native snapshots older than twenty seconds are reported as unconfirmed
with quota unavailable. Each reply revalidates its current native binding and
authorized creator. The observer projects only routing metadata for new,
successfully handled `/status` commands from the bridge's durable ingress
journal. It checks sender authorization and the current authorized topic binding.
Activation does not reply to old commands. Requests older than two minutes are
ignored, and bounded private attempt receipts prevent replay after restart or
ambiguous delivery. A rejected/uncertain supplemental reply can be requested
again with a new `/status`. The main bridge remains the sole update consumer;
no command changes the input journal or starts a model turn.

Completed assistant messages are never edited or deleted. The observer removes its own message when
native work and tracked child work end and foreground delivery is no longer
running. If deletion is unavailable, it marks that message inactive.

When a topic has a native goal, both requested status and automatic working
cards include its literal state and the beginning of its objective, capped at
180 characters with an ellipsis. Reading `thread/goal/get` starts no inference.
Goal metadata alone never creates a working card: paused or blocked goals in
otherwise idle topics remain visible through `/status` without implying activity.

Pending human requests add a scoped count and an **Open human requests** button.
A durable human task may keep a silent attention card after native completion;
it is labelled Human requests and does not count as active work for idle upgrades.
See [human request lifecycle and declarations](human-requests.md).

The observer uses only `initialize`, metadata-only `thread/read`, `thread/goal/get` and
`account/rateLimits/read` on the existing Codex App Server proxy. It reads the
bridge SQLite database and native index in
read-only mode, selecting only topic bindings whose creator remains allowed.
Linked rollout metadata supplies tool correlation and child references; native
`SubAgentActivity.interacted` events re-register resumed children, and async
question messages do not mark continuing native work completed. Native
index ancestry must link a referenced child back to the selected parent before
its status is read. Native status reads establish current parent/child activity. No model inference, native
turn/resume/steer, Telegram polling, Matrix operation or history import occurs.
Raw commands, arguments, output, prompts, paths and agent identities never enter
status text or logs. The native `sessions` root and first rollout metadata must
match the selected binding. Initial reads are bounded to four MiB per linked file,
then consume new records incrementally. At most 32 reported child threads are
checked per parent. This version targets the native `state_5.sqlite` index and
rollout metadata verified with Codex 0.160.0; probe other versions before adoption.

Python 3.11+ and a POSIX host are required. Reuse the bridge's existing local
configuration, HOME, CODEX_HOME and executable. Its token must be an owner-only
regular file; this observer currently supports `telegram.bot_token_file`.

```text
python3 -B scripts/activity_indicator.py --config /absolute/local/config.toml --state-dir /absolute/private/activity-state --probe
python3 -B scripts/activity_indicator.py --config /absolute/local/config.toml --state-dir /absolute/private/activity-state --status-requests
```

The probe reads native status but never reads a bot token or contacts Telegram.
Normal activation requires the human's authorization to show activity on that
bot. Use a private state directory with mode 0700. One observer holds a local
process lock. An optional owner-local service may manage it alongside the existing
human listener; installing a skill alone must not activate it.

Topics do not need manual subscription. Each polling pass reads the bridge's
current session bindings and allowed creators. A new topic joins observation
when its native Codex thread is bound; it is not observable before that binding.
The scope is this configured bot/database, not other bots on the host.

For a systemd deployment, give the listener a `Wants=` dependency on the observer
and give the observer `After=` and `PartOf=` dependencies on the listener. Use
the actual local unit names; `systemctl --user add-wants LISTENER OBSERVER`
can create the startup dependency. `After=` alone only orders startup and
`PartOf=` alone does not start the observer after separate listener stop/start
operations. Verify both services are active after an executable cutover.

`health.json` reports connection health and aggregate topic/publication counts.
`messages.json` privately tracks only the observer's message receipts, correlation
and retry timing. `retirements.json` retains at most 32 topic cleanup outcomes,
distinguishing confirmed deletion, an already absent message and an inactive
fallback. `status-requests.json` bounds supplemental command attempt receipts.
After an abrupt restart, activity message IDs are reused instead
of sending duplicates. A graceful stop cleans up the observer's own messages.
An ambiguous initial send is never automatically repeated for the same native
turn. Explicit rate-limit rejection permits a delayed retry. Disconnects replace
existing live claims with an unconfirmed-connection warning. An abrupt
kill may leave an old activity message until restart; it cannot promise immediate
cleanup without a running observer. Stop the observer to roll back; preserve its
state for cleanup and leave the bridge, input journal and polling offset intact.

Qualification: `python3 -B tests/activity_io.py` exercises actual HTTP delivery,
ambiguous sends, persistent message reuse, topic receipt binding, cleanup,
rate-limit retry, incremental native records and SQLite read-only access. The
tests also cover final-answer cleanup while an outer task remains active, new
request positioning, authorized `/status` routing and no replay after restart.
The tests include quiet ordinary work, changing tool identities, source-binding
replacement, unloaded idle sessions, stale snapshots and a fast command reply while native observation
is not running. Real HTTP regressions also exercise shared manual/automatic
message reuse, concurrent publication, requested-card cleanup and retry deadlines.
They also cover same-turn relocation after assistant responses and slash commands,
child reactivation after a final answer and async question continuations.
The native probe establishes live attachment without inference. These are separate
from a real Telegram receipt and the human's observation of the interface.

Source comparison and owning task: [Telecodex #11](https://github.com/AlterMundi/telecodex/issues/11),
within [the convergence pilot](https://github.com/AlterMundi/daimon-matrix/issues/262).

An explicitly configured `--pending-update-record FILE` adds a finite bot
update wait to its recorded topic. The owner-only producer receipt must identify
the topic, live worker, wait reason, blockers, heartbeat and deadline. Fresh
receipts show what blocks the update and the remaining deadline; stale or dead
workers show uncertainty, never fabricated progress. This deadline is a maximum
wait, not a scheduled wakeup. Native activity and external update work are shown
as separate lines. No receipt means no inferred external task. Existing quiet
grace, card relocation and completion cleanup still apply.

## Bridge command activity

Fork operations publish topic-scoped phases separately from native conversation
turns: locating a quote, synthesizing a focused handoff, cloning native history,
creating a topic, preparing its binding, and delivering links. `/status` can
report an admitted request while the long command still occupies normal topic
dispatch; observation never consumes, completes or replays that input. Automatic
cards use the existing 30-second quiet threshold and weekly limit/reset display.

Live claims require the current bridge instance and a fresh instance heartbeat.
The command scope retires its own operation on completion, error or cancellation;
a restart discards command activity while retaining pending handoff context.
The companion does not infer progress from elapsed time or from a preparing
message. This reusable tracking currently covers native forks and focused reply
handoffs.

## Topic lifecycle and compact activity

The same observation sweep retains `topics.json` with each selected native
binding, allowed creator, observation time, execution and literal goal state.
An unsupported goal read is **unknown**, distinct from a successful null goal.
The read-only catalog joins those facts with the bridge's retained topic metadata:

```text
python3 -B scripts/activity_indicator.py --config CONFIG --state-dir STATE --inventory --activity-days 7
```

This finite command opens neither a native connection nor Telegram and needs no
bot token. It revalidates the current allowed creator, physical native binding,
fresh-thread flag and source database identity, and recalculates every count.
Observations older than twenty seconds, disconnected observations and future
timestamps cannot certify current state. Metadata-less and unbound sessions stay
visible as unknown. The 512-row bound reports truncation.

Categories distinguish `🎯` active goals (which may be idle/waiting), `⚙` work
without an active goal, passive sessions and unknown state. Paused, blocked,
complete and limited goals retain their literal state in detail. The inventory
also counts passive sessions with actual conversation observed within X days.
Recency uses attributed bridge turns and native UserMessage/AgentMessage records;
tools, polls, renames, reasoning and goal changes do not advance it. A bounded
native tail can prove recency, but cannot prove that an unobserved conversation
never happened. Missing/older evidence therefore remains unknown, not an
assertion of inactivity. No native history is copied into the catalog.

With explicit `--topic-markers` opt-in on the existing observer, those first two
symbols are projected into known canonical Telegram names. Passive/unknown
sessions receive no new decoration. Only a name with retained provenance and an
open lifecycle is eligible for a new marker. Unknown legacy names remain
untouched. Names too long for the prefix retain their full human name. No icon,
canonical catalog name, native title, input or model turn is changed. Bot-authored
edit service messages continue to be ignored by the bridge, preventing rename
loops. Human renames supersede the previous projection; explicit `/rename`
continues through its existing reconciliation path.

`topic-markers.json` retains the selected canonical-name revision and each
attempt **before** transmission. A known rejection permits a delayed retry;
lost acknowledgments and interrupted attempts remain uncertain and are never
automatically replayed or blindly restored. `--inventory` exposes the matched
projection phase, attempted name and acknowledged name. A new genuine canonical
name can supersede an old uncertain projection. Successful title changes are
spaced by at least thirty seconds. The native-status API and Telegram title API
do not form an atomic transaction: markers are dated observations, not a lock
on human renames or a guarantee of instant status.

A graceful observer exit restores its own acknowledged names, checking the
current allowed creator and canonical revision again. For a finite retirement
after stopping this observer, use:

```text
python3 -B scripts/activity_indicator.py --config CONFIG --state-dir STATE --restore-topic-markers
```

This uses the same exclusive observer lock and starts no native work. It cannot
resolve an uncertain send, and never restores a superseded human name. Keep the
private receipts when disabling the feature. Catalog/activity/marker fixtures in
`tests/activity_io.py` cover real SQLite reads and loopback HTTP, fresh/stale and
revoked bindings, unknown goals, native-only recency, human renames, retry limits,
long names, retirement and lost ACK/restart. These tests do not certify receiving
adoption or a real Telegram visual observation.
