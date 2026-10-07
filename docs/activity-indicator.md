# Compact native activity in Telegram

The optional `scripts/activity_indicator.py` observer maintains one silent,
editable activity message per authorized Telegram topic. It works with turns
already running, including CLI continuations of the topic's bound native thread.
Starting it does not restart Telecodex or the native daemon. The main bridge
remains the only Telegram update consumer.

The message appears after eight seconds, updates at most every thirty seconds,
and summarizes elapsed time, outstanding native tool calls, active child agents
and native approval/input waits. Short answers remain clean. The line also shows
the available weekly allowance from
native `account/rateLimits/read`, cached for sixty seconds. The weekly window is
identified by its seven-day duration; the Codex bucket is preferred when multiple
buckets are returned. Missing or unavailable quota is shown as `weekly n/a` rather
than guessed. This read starts no inference.

Completed assistant messages are never edited or deleted. The observer removes its own message when
native work and tracked child work end and foreground delivery is no longer
running. If deletion is unavailable, it marks that message inactive.

The observer uses only `initialize`, metadata-only `thread/read` and
`account/rateLimits/read` on the existing Codex App Server proxy. It reads the
bridge SQLite database and native index in
read-only mode, selecting only topic bindings whose creator remains allowed.
Linked rollout metadata supplies tool correlation and child references; native
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
python3 -B scripts/activity_indicator.py --config /absolute/local/config.toml --state-dir /absolute/private/activity-state
```

The probe reads native status but never reads a bot token or contacts Telegram.
Normal activation requires the human's authorization to show activity on that
bot. Use a private state directory with mode 0700. One observer holds a local
process lock. An optional owner-local service may manage it alongside the existing
human listener; installing a skill alone must not activate it.

`health.json` reports connection health and aggregate topic/publication counts.
`messages.json` privately tracks only the observer's message receipts, correlation
and retry timing. After an abrupt restart, these message IDs are reused instead
of sending duplicates. A graceful stop cleans up the observer's own messages.
An ambiguous initial send is never automatically repeated for the same native
turn. Explicit rate-limit rejection permits a delayed retry. Disconnects replace
existing live claims with an unconfirmed-connection warning; the timestamp on
every live line makes a stopped observer's last observation apparent. An abrupt
kill may leave that timestamped message until restart; it cannot promise immediate
cleanup without a running observer. Stop the observer to roll back; preserve its
state for cleanup and leave the bridge, input journal and polling offset intact.

Qualification: `python3 -B tests/activity_io.py` exercises actual HTTP delivery,
ambiguous sends, persistent message reuse, topic receipt binding, cleanup,
rate-limit retry, incremental native records and SQLite read-only access. The
native probe establishes live attachment without inference. These are separate
from a real Telegram receipt and the human's observation of the interface.

Source comparison and owning task: [Telecodex #11](https://github.com/AlterMundi/telecodex/issues/11),
within [the convergence pilot](https://github.com/AlterMundi/daimon-matrix/issues/262).
