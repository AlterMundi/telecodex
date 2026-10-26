# Shared goal control and transient recovery

Owner: [Telecodex #297](https://github.com/AlterMundi/telecodex/issues/297).
The human delegation/proposal in that issue selects a shared backend capability,
not a Telegram command handler, UI-owned timer or second native goal engine.

## Implementation checkpoint

The first implementation slice retains native failure evidence in the existing
backend SQLite store. `thread/goal/updated` supplies the blocked goal and its
actual causal `turnId`; a terminal `turn/completed` failure supplies its structured
`error.codexErrorInfo`. Receipt order may vary. Only the same observing human, current owned
session, native thread and physical turn join. Duplicate observations retain the
same record; conflicting evidence is rejected. Rebinding hides old evidence from
the current receiver, preserving it as history. A new session owner never
inherits or joins the previous human's goal/error evidence. Reads include their
actual native binding; goal inspection requires its already-observed binding. No goal or native history is
rewritten. Existing read-only collection can recover the terminal-error half.

`serverOverloaded` is the initial classified capacity cause. Matching words in a
message, `blocked` alone, quota, authentication errors and unknown classifications
are insufficient. A linked cause is historical evidence, not a current goal
revision or permission to act.

The existing authenticated presentation result includes one compact
`goal_failure_evidence` summary per current entry/enrolled target, schema
`telecodex.goal-failure-summary/v1`. Detailed inspection uses
`historical_failure_evidence` in `goal_inspect`, schema
`telecodex.goal-failure-evidence/v1`: at most sixteen observations, an explicit
`more` flag, bounded goal/error previews and full evidence retained privately.
Aggregate presentation never multiplies full objectives/provider details across
targets. Current revision, delegation and next attempt remain unknown;
a separate `recovery` projection distinguishes retained authority, configured settings and current scheduler phase. Unavailable is not idle or recovered. These reads have
no native mutation, inference, Telegram request or lifecycle effect.

The current implementation also extends retained controls with revocable owner-
installed human delegation and receiver-attributed daimon execution. That extension
is undergoing qualification; it is not deployed. The next source slice adds automatic scheduling and retry settings, still under qualification. Alerts and real receiving-native acceptance remain unfinished. No existing
pause, blocked goal or receiving installation has changed.

## Retained delegation and shared client contract

The existing authenticated owner gateway accepts `goal delegation`,
`goal delegation set JSON` and `goal inspect CHAT TOPIC`. The CLI forwards them
through `telecodex --control CONFIG goal ...`; it does not start a second backend.
The authenticated gateway supplies the human scope; JSON cannot select another
human. A grant records an actual delegation source and the installing owner
principal, rather than fabricating human speech or a Telegram update.

The bounded update is:

```json
{
  "request_id": "a caller-retained UUID",
  "expected_revision": null,
  "source": "the actual retained human delegation record",
  "scope": "owned_sessions",
  "agents": true,
  "recovery": true
}
```

First installation expects no revision. Later changes require the exact current
revision. Reuse the request UUID when inspecting/retrying the same update;
changed arguments are rejected. Retrying an earlier grant after revocation returns
its historical receipt and current grant, never reactivating the older revision.
Turning `agents`/`recovery` off revokes that principal. Grant storage survives
restart and stays isolated by authorizing human and current session ownership.
The recovery flag is retained authorization; backend settings and per-session policy also constrain execution. Publication of this draft has not activated any receiving installation.

An effective agent grant exposes only `goal_inspect`, `goal_prepare`,
`goal_execute` and `goal_status` through the existing live native controller.
It does not grant routing, Matrix, arbitrary original-input access, or global
Stop. Targets are current owned native sessions, selected by `self`, exact title
or stable `tID`; ambiguous matches require clarification. The actual current
native execution must prepare/claim the intent. Intents retain that author,
executor, target binding, native revision and delegation revision. Revocation,
rebinding, newer direction, Stop or maintenance invalidate stale admission.

Omit `input_ref` for a daimon action. Delegation supports objective edit, pause
and resume of an existing goal; budget changes, start/clear/replace and invented
human follow-up are outside this grant. Unsupported proposals retain the existing
integrator confirmation route, while ordinary work executions cannot acquire
that role. Delegated native controls use `origin=automatic`; actual retained
human commands keep `origin=user`. Linking a delegated continuation preserves the
latest actual human-selected return surface. It never inserts a dummy original
or return selection. The receiver still distinguishes metadata/work admission,
uncertain ACK and useful completed work.

## Optional Telegram transport

`[telegram] enabled = false` starts the same backend and authenticated owner
socket without resolving Telegram secrets, `getMe`, command registration,
polling, typing, draft/placeholder creation or publication recovery. Native and
private artifact retention remain independent. The disabled adapter rejects
operations rather than fabricating delivery receipts. Other explicitly configured
adapters retain their own settings; turn them off if a deployment needs none.

An identity actually observed by an earlier enabled run can label retained
Telegram originals; a fresh backend does not invent a bot identity. Conversation
originals remain usable when historical Telegram provenance is unavailable.
Existing databases, offsets, native bindings and return selection are preserved.
Re-enable the adapter to use its existing transport, without replaying uncertain
inputs. With Telegram enabled, bootstrap still requires actual `getMe` success.

`interaction.telegram_ingress=false` disables only integrating capture. It does
not disable the whole adapter; use `telegram.enabled=false` for the independent
app/Realtime qualification. Tests cover credentialless configuration, actual
backend startup/owner socket, zero manufactured inputs and a disabled client
that rejects all operations. Passing these tests is not yet the end-to-end
native goal recovery journey required below.

## Model-free recovery implementation (under qualification)

Recovery shares the same control-intent ledger, single-flight claim, native
conditional activation, invocation context, write-boundary retention and actual
collector settlement as delegated controls. It does not create a second goal
engine or replay the failed input/tools. A generic agent cannot claim a
backend-owned recovery intent and impersonate that worker.

A blocked notification captures its actual physical turn, current delegation,
owned session/native binding, Stop epoch, command generation, per-session policy
and capture barrier. Only a later/equal matching terminal `serverOverloaded`
observation creates one durable job in the same transaction. Either receipt order
works. A grant installed after an earlier block does not backfill or revive it. Native
block timestamps have second precision; equal-second ordering cannot prove prior
delegation, so those delayed observations remain ineligible.
Missing causal block context remains unqualified; periodic terminal-history reads
cannot invent the missed association.

`[goal_recovery]` defaults to `enabled=true`, `interval_seconds=60`, `batch_size=4`.
Those defaults create no human authority: effective retained `recovery=true`
delegation is required. The loop starts with the backend, reconciles at startup
and periodically, and receives event wakeups independently of Telegram and model
availability. A backend settings disable or an owner-installed per-session policy
can stop automatic attempts. `goal recovery set JSON` accepts `request_id`,
`target`, `expected_revision` and `enabled`; target ownership and policy CAS are
checked. The per-session setting and backend settings are inspectable in recovery
state; source defaults never override a retained receiver opt-out.

Initial fallback delay is thirty seconds. Consecutive capacity failures caused by
the acknowledged recovery continuation retain the same episode and exponentially
increase delay to a thirty-minute base, with stable ±10% jitter. Restarts and
duplicates preserve timing. A completed/returned continuation or later independent
control resets the episode. There is no arbitrary retry-count lockout. The native
terminal error currently supplies no Retry-After value; none is invented.

Before preparing a conditional control, the backend observes the actual native
snapshot/revision, readiness and latest terminal turn. It requires the same
blocked definition and causal failed turn, with budget remaining. The opaque
native revision is frozen in the existing plan; a subsequent revision change
rejects admission instead of refreshing the plan. Usage/accounting timestamps are
not treated as new goal definitions. At claim and the native write boundary,
recheck current ownership/binding/delegation, latest goal generation, later
captured human input, active/pending work and native questions, per-session policy,
Stop epoch and maintenance. Explicit pauses, clears, edits, replacements,
completion and exhausted budgets take precedence. Read-only `/status` does not
become another goal command. A later direct `/stop` supersedes recovery.

The durable job distinguishes waiting, claimed, submitted, acknowledged,
returned/native-completed, failed-again, superseded, rejected, partial and
uncertain outcomes. The exact matched native control receipt updates it in the
same transaction. A queued/claimed attempt interrupted before its write boundary
can wait and retry its frozen plan; submitted/unknown effects are never replayed.
Native return observations remain distinct from native acceptance, goal completion
and human receipt. Routine capacity retries do not produce one control/failure
message each; their structured evidence and actual output remain retained.

Attempt receipts are retained separately, while the inline previous-outcome view
is bounded to sixteen records. Duplicated receipts do not shift retry deadlines.
The per-target shared presentation and owned topic inventory project recovery
state and the next eligible check; this is not a guarantee that admission will
occur then. Receiving UI rendering and sustained Alerts delivery still require
qualification. Existing goals, active work and deliberate pauses have not been
changed by source development.

## Client projection and qualification still required

Project waiting-for-provider, next attempt, submitted continuation, accepted turn,
uncertain result, actual recovery and disabled/superseded state through the same
backend. Realtime must work with Telegram ingress disabled and no Telegram
interaction. No author-model availability is required to recover provider capacity.
Use existing Alerts for sustained incidents with deduplicated receipts; not every
attempt is an alert. Delivery or acknowledgment never authorizes recovery.

Required qualification includes a real receiving-native conditional-control
journey and deterministic multi-session failure cases; controller/client authority;
restart, races and uncertain ACK; explicit pause/Stop/edit/clear/completion/budget
precedence; and the same independent-app/Realtime journey with Telegram disabled.
Then qualify the Telegram adapter against the same service. Preserve deliberate
receiver overrides and publish/update the maintained skill and receiving records.
Do not claim installed/adopted receivers from a source publication alone. No new
Codex patch, global Stop or change to another daimon's installation is authorized.

## Current evidence and receiving adoption

Head `40a7a59` passed CI40318325090: 379 text-only Rust tests and 380 all-features
tests (one ignored in each), Clippy, builds and the controller/native-wire journeys.
The delegated resume/self-pause journey used no Telegram API or fabricated ingress
during its tested runtime, preserving accounting and revocation. Its native peer
is synthetic; fixture enrollment used earlier synthetic Telegram sessions. The
additional scheduler/receipt/source slice is not covered by that earlier head.
Its final-head CI and focused review remain required.

A real human-selected blocked Realtime conversation is deliberately retained for
later rescue acceptance. It is not resumed as a deployment or test convenience.
No global Stop was authorized. No Oliva/Eko installation is changed without the
human's prior consultation. Eko's current host is MEDUSA, not its stopped source
container; resolve future authorized adoption from the maintained receiving map.

Before capability activation, update the maintained shared skill and context used
by all affected CompAII sessions; publish through AlterMundi/Skills. Capability
publication, local installation, receiving adoption and actual use are separate.
Do not claim that existing sessions or other daimons learned a capability merely
because this draft describes it.
