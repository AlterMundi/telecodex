# Native goals in Telegram

`/goal` inspects the selected session's native objective, status, usage and token
budget without starting model work. Execution activity and goal state are
independent: a session can be working while its previous goal is complete.

`/goal resume` explicitly reactivates the same goal, including a completed goal,
when its retained token budget permits it. It preserves objective, history,
creation time and accounting. It does not create a replacement goal, reset usage
or interrupt an existing turn. An existing writer is used without another kickoff.
An idle activation retains the existing bridge continuation path.

A `budgetLimited` status is not by itself an irreversible state. Resumption checks
the retained budget and usage again at native admission. Exhaustion rejects
activation before a kickoff; it explains that resumption and objective editing
do not reset usage. Raising the budget requires a separate explicit native goal
budget change within the applicable limits. A previously increased budget can
allow the same goal to resume. Account/service availability is separate.

`/goal edit` preserves usage and budget. Editing a completed or budget-limited
goal with exhausted budget saves the revised objective without starting another
turn. Editing a paused goal preserves its pause. `/goal <new objective>` retains
the existing distinct replacement behavior and its stated accounting semantics;
replacement is not a recovery shortcut for exhausted usage.

`/goal pause` stops future goal continuations. `/goal clear` removes the goal.
Neither interrupts an already running turn or undoes effects. Use separately
requested turn interruption when that is the intended action. Do not infer a
global Stop from goal controls.

## Integrator control policy

Human selection, 2026-10-19: an explicit human instruction to start, pause, resume
or clear a selected goal needs no repeated permission request. If the assistant
proposes the lifecycle change, explain the target/action/reason and wait for
human confirmation. This is the selected initial policy, not blanket autonomous
goal reactivation. The selected integrator adapter in
[Telecodex #210](https://github.com/AlterMundi/telecodex/issues/210) uses retained
human originals, confirmed proposals, native preconditions and ordered follow-ups;
see [integrator controls](integrator-controls.md). This is not an arbitrary owner
shell interface or an autonomous recovery service. Destination visibility for
controls without worker speech is owned by #241.

## Verification

`python3 -B tests/goals_io.py target/release/telecodex` exercises the shared native
proxy contract, actual local HTTP presentation and SQLite with synthetic native
output. Added cases cover completed-goal resumption during and outside existing
work, budget-limited resumption after budget headroom exists, exhausted activation
rejection and objective edits without kickoff, preserving accounting and explicit
pause. They assert native operation counts as well as visible results. This
qualification does not claim actual model execution or human receiving acceptance.

## Shared backend recovery work

[Issue297](https://github.com/AlterMundi/telecodex/issues/297) selects retained
human delegation and default transient-failure recovery shared with independent
clients. See [the implementation checkpoint and client contract](goal-service.md).
The first slice retains attributable native failure evidence only; it does not
yet enable delegated execution or automatic recovery, and does not resume any
existing goal or replace the currently implemented confirmation policy.
