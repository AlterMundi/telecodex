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
targets. A separate `recovery` projection distinguishes retained authority,
configured settings and current scheduler phase; historical failure evidence
does not stand in for a current native revision. Unavailable is not idle or recovered. These reads have
no native mutation, inference, Telegram request or lifecycle effect.

The implementation extends retained controls with revocable owner-installed
human delegation and receiver-attributed daimon execution, plus a model-free
scheduler, safe sustained-incident outbox and receiving status. Qualification
and deployment are separate; existing deliberate pauses remain untouched.

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
occur then. Sessions, Telegram status and the activity companion render the retained state
and relative next check. These labels do not infer recovery from acknowledgment. Existing goals, active work and deliberate pauses have not been
changed by source development.

## Sustained incidents through existing Alerts

The optional `[goal_recovery.alerts]` selects one human, producer, receiving scope
and opaque resource reference plus an operator-owned argv adapter. Its default
threshold is 600 seconds. There is no separate service, model, Telegram consumer
or notification per retry. `scripts/goal_recovery_alerts.py --config PRIVATE_JSON`
reuses the maintained loopback Alerts client and closed v1 schema. Private JSON
contains `base_url`, `token_file`, `producer_id`, `receiving_scope`, `resource_ref`.
The token file and config must be regular owner-only files; symlinks are refused.
Provision a dedicated publish-only producer grant for the selected scope; do not
borrow HMK, traffic or another daimon's credential. Core installation and producer
adoption follow Daimon Alerts' maintained operator runbook.

Each failure episode retains one immutable open projection and, after matched
native completion or explicit retirement, a separate resolution revision. No
objective, transcript, raw provider error or native ID is published. Alerts treats
`phase`, `source_at` and the incident/resource/operation/scope identifiers as
immutable provenance across revisions. Resolution preserves them and changes
`processing_state`, summary/action and update time; runtime retry progress stays
in the separate recovery projection. Unknown
native effects remain open. Retirement explicitly does not declare provider
health. A recurrence after actual return uses a new episode. The backend's
SQLite outbox commits exact content before HTTP, preserves service acceptance
and its uncertainty across restart, and prevents resolution overtaking an
unconfirmed open upsert. The finite helper has one eight-second deadline and
bounded input/output. Repeating identical incident upserts is supported; that
rule never authorizes a repeated Telegram delivery claim or native execution.
Alerts reads, delivery and acknowledgment do not resume goals or paid work.
Receiving delivery uses the existing supervised bot adapter with the app closed.
A missing/unavailable Alerts producer does not block native recovery.

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

Coherent head `bf62084` passed all jobs in CI40372232887, including text/audio
Rust checks, durable scheduler/delegation native-wire journeys, dashboard and
activity tests. Those deterministic capacity failures use a synthetic native
peer; they do not claim a real provider outage. The later exact-accepted,
completed-lifetime pending-input correction passed focused independent review.
Missing ACK and open/transferred/boundary collectors remain fenced.

Receiving-native qualification used Codex0.163.0 and Luna low with Telegram
fully disabled and no Telegram credential in the tested backend. An actual live
execution lease prepared/executed the attributed automatic-origin continuation;
the matched native turn returned useful output and completed the same goal,
preserving objective, budget, creation identity and cumulative accounting, with
no fabricated ingress. The test driver exercised that real lease; this does not
claim that the model independently selected the operation. The verified binary
source tree was bf62084. Its native writer is unchanged by the later pending-input
and Alerts provenance corrections.

A separate old-to-new real-native listener handoff passed: the existing writer
survived disconnect, exact collector attachment created no duplicate turn, and
the original result returned once with goal accounting retained. Both journeys
used only isolated fictional enrollment, not existing human work. Fixture
preparation failures were retained and corrected without reseeding the target
goal or replaying a submitted continuation.

The actual scoped Alerts core accepted an open qualification projection and
its resolution after preserving immutable provenance. That check exposed and
corrected the earlier changing-phase resolution bug; the HTTP fixture now
checks the same provenance invariant. Final-head checks and receiving runtime
adoption remain separate from these retained results.

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

## Integrating entry and dormant receivers

Goal selection includes the integrating entry through `self` or its exact `tID`,
without making that entry an ordinary routing destination. An owned integrating
session uses the same conditional revision, source/delegation and accounting
checks as a work session; a blocked goal is edited before explicit resumption,
never falsely completed or replaced to clear an unfinished-goal refusal.

A cached receiver channel is not an active native writer. When no attached
subscriber exists, metadata controls refuse an actual preparing execution
(the receiver's active cancellation slot), but a dormant channel can use the
existing conditional metadata RPC. Native revision/admission checks still apply;
uncertain receipts, preparation, Stop and maintenance fences do not authorize
independent retry. Check live admission, not only map membership or native idle.
