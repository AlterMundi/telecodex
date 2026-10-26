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
`execution_enabled=false`. Unavailable is not idle or recovered. These reads have
no native mutation, inference, Telegram request or lifecycle effect.

This slice does **not** implement delegated execution, scheduling, automatic
resumption, retry settings, Alerts or client recovery controls. It is not a
claim that #297 acceptance has passed or recovery is deployed/default-enabled.
Existing explicit pauses, blocked goals and maintenance/Stop fences are unchanged.
The old integrator confirmation policy still applies until the retained-delegation
extension is delivered. No existing goal has been resumed by this work.

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

## Shared execution contract for the next slice

Extend the retained `interaction_goal_controls` and existing conditional native
operations; do not manufacture a Telegram update or introduce a parallel goal
writer. Realtime, the current independent app, Sessions and Telegram use one
backend service. Reuse the authenticated owner-control gateway and controller
facade; client code does not decide whether a retry is eligible.

A human delegation must be durable and revocable, retaining its actual source,
authorizing human, permitted daimon/recovery principal, target scope, revision and
supported actions. The receiving backend resolves actual actor identity. Tool
arguments and peer context cannot invent or broaden a delegation. Retain the
actual controller native execution or recovery-worker identity on every intent.
Use native `origin=automatic` for delegated/recovery controls; native `origin=user`
remains reserved for an actual human instruction. Budget expansion/reset and
destructive clear/replace require their separate explicit authority.

Agent tool and authenticated client operations must share inspect, prepare,
execute/status and supported pause/edit/continue behavior. Preserve existing
proposal confirmation when no applicable delegation exists. Freeze native
revision, binding, plan, authorization revision and idempotency key at preparation.
The service rejects stale conditions rather than silently refreshing a plan.
Native acknowledgment is not completed work or receipt by the human.

Recovery runs without a model and independently of Telegram ingress. Subscribe
to terminal/goal observations and reconcile retained pending state at startup and
periodically (proposed initial interval:60 seconds). A missed goal-turn association
stays unknown; do not infer it from proximity. Qualification must establish how
missed native events are reconciled before claiming full startup coverage.

Retain one retry record with session binding, goal revision/incarnation, failed
turn, cause, attempt count, next eligible time and native admission outcome.
Respect provider retry timing only when actually present. Otherwise use capped
exponential backoff with jitter, without an arbitrary retry-count lockout for a
temporary outage. Initial automatic eligibility is structured `serverOverloaded`;
other transports/rate limits need explicit classification and tests.

Immediately before admission recheck current goal/revision, usage/budget, binding,
delegation, newer goal intent, active/pending input, Stop epoch and maintenance
fences. A later pause, edit, clear, replacement, completion or Stop wins. Default
recovery is enabled only within the retained authorized deployment scope; retain
inspectable disable controls and explicit per-session overrides. Configuration
defaults cannot create human delegation.

Single-flight claims, write-boundary retention and native conditional admission
must handle duplicate events, concurrent workers, process restart and lost ACK.
Never replay uncertain native work, input or tools to repair observation. Link
retries to the existing goal-control outcome. Preserve partially committed effects
and reconcile them before another attempt. Acknowledged continuation and useful
provider recovery remain distinct outcomes.

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
