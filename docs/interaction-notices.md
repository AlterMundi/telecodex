# Passive control notices

Implementation for #241. Receiving adoption and client qualification must be
recorded separately from source publication. A notice is an attributed observation
of an action, never human input, worker admission or a reason to repeat the action.

Goal outcomes and cross-session retirement of agent-issued pending requests reuse
the topic publication journal. They keep their canonical goal/task record and
existing Telegram delivery identity. Ordinary direct messages and same-session
task retirement are not enrolled in routing. The destination's projection setting
applies independently of how the integrating turn originally entered.

Goal notices distinguish acknowledgment, rejection, known-unsent, partial and
uncertain outcomes. Task notices say withdrawn rather than completed. They include
the destination task's short title; private explanation from the integrating
conversation is not copied into its topic. Publication failure never retries the
control. Unknown Telegram delivery is retained without automatic resend.
A disabled secondary projection is retained as `not_selected`, without a delivery
claim. It cannot hold later ordinary direct output in the destination's lane.
Collector retirement and restart retain known-unsent versus uncertain outcomes
without submitting a native control again. An unexecuted proposal is not announced
as an action.

## Owner-local read contract

The existing owner-only Unix control socket accepts:

```text
telecodex --control CONFIG topics notices NATIVE_THREAD [AFTER]
```

No new listener, credentials or native RPC is involved. The socket's existing
owner check selects the configured human. Reads require an allowed human, owned
current destination binding and the actual retained goal/task source. Unknown,
changed or unauthorized bindings return `available: false` with no notice text.

The result schema is `telecodex.thread-notices/v1`. It includes the exact
`native_thread`, `available`, `history`, `history_truncated`, `cursor`, `has_more`
and `items`. Each item has an opaque `id`, increasing integer `sequence`, source
`interaction` and short plain `text`. Omit AFTER for the latest 32 retained
notices, in ascending order, with `history: true`. This snapshot cursor is the
latest returned sequence; omitted older history is explicit. Supply AFTER for
up to 32 subsequent notices, in ascending order; continue from `cursor` while
`has_more` is true. IDs are presentation identities, never action permissions.
The partial journal index keeps these reads separate from ordinary message scans.

## Optional native CLI reader

The Interaction Codex fork adds an optional Unix TUI reader selected by the local
operator through `CODEX_FOREGROUND_NOTICES_SOCKET`. Its value is the existing
receiving owner socket's absolute path, kept in local configuration. This is not
browser onboarding or a credential file. Unset means no reader or scheduled work.

The reader uses the existing foreground frame scheduler, only for the displayed
local thread. It does not read a local source for remote workspaces. Scope and
request identities discard replies after a thread switch. Owner socket type,
mode and peer checks match the existing owner client. Replies are bounded to
256 KiB and a 750 ms deadline; invalid data never becomes a command.

The initial snapshot shows only its latest outcome, labeled as retained history.
Subsequent notices appear once as ordinary information lines. A temporary source
failure leaves conversation input usable and reports unavailability once, with
bounded retry. Closing the UI or changing threads cancels an outstanding read.
There is no independent daemon, model attention loop, native input injection or
new model turn. The records remain in the owning journal; these display lines
are not additions to the model's native conversation history.

Qualify goal and task notices through actual owner/Telegram boundaries, duplicate
observation, lost acknowledgment, revoked/replaced binding, cursor recovery and
foreground CLI rendering. Match executable/source identity at receiving adoption.
Do not equate a journal read with human seeing or hearing its contents.

The successful text qualification job retains `telecodex-text-linux` for seven
days: the tested executable, its SHA-256 and the checked-out source commit.
Use that executable for compatible Linux receiving deployments instead of
rebuilding on a memory-constrained session host. Retain the run's head and tested
commit separately: pull-request qualification can check out a merge commit.
Verify compatibility and reuse the existing writer-preserving listener handoff;
artifact availability does not authorize a native daemon restart or work replay.
