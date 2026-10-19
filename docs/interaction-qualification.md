# Daimon Interaction: qualification and next roadmap

Status: proposed acceptance campaign, 2026-10-19. This document does not report
execution of the campaign or authorize new capture, publication or background
attention. Owner: [Daimon Interaction](https://github.com/AlterMundi/daimon-matrix/issues/313).
Design: [integrating conversation](interaction.md).

## Product outcome

One habitual text/voice conversation lets a person discuss ideas and advance
several complex, persistent work contexts. Sessions can progress concurrently;
the person can correct work, answer questions and receive useful progress without
manually managing queues or interpreting transport metadata. A simple button is
enough to participate. Read-only inspection remains available beneath the simple
surface. Fluency means useful coordination, not imitation of a serial human chat.

The immutable `daimon-interaction-v0.1.0` tags mark the first functional baseline,
not daily-use acceptance. Keep that milestone while improving the experience.

## What existing evidence establishes

Evidence types are distinct: synthetic automation, real native/provider execution,
and observed human use. Reuse retained evidence when the relevant code and conditions
are unchanged. Re-run the affected scenario when a change invalidates it; do not
repeat all model work after every cosmetic edit.

| Capability / owning work | Existing evidence | Still needed |
| --- | --- | --- |
| Text ingress, durable originals, steering, returns and recovery (#109–#113) | Automated process qualification; selected real native journeys | Broader combined journeys and normal daily use |
| Observer, semantic route/clarification/new context, explicit local addressing (#114–#116) | Source-linked implementation, synthetic routing comparisons, bounded receiving checks | Held-out human intent, summary fidelity across changing work, adapter-specific dependency returns |
| Many retained contexts / execution budget (#117) | Synthetic 20-context scenario; real two-worker overlap | Mixed sustained use; 20 retained contexts must not be described as 20 paid concurrent workers |
| Voice (#118, Integrators #4) | Real ASR/TTS and native journey; human confirmed hearing the routed reply on 2026-10-19 | Everyday audio timing, background/reconnect and bilingual usability |
| Monitor (#170) | Retained graph and browser qualification | Comprehension and unobstructed small-screen use |
| Selected visual references (Integrators #5) | Bounded selected-reference implementation/qualification | Combined user journey; not continuous video perception |
| Observer interviews (#119) | Explicit selected interview / source-isolation slice | User evaluation of useful interviews; no automatic publisher acceptance |
| Web repair (#196) | Direct-topic isolation, canonical returns, correction and receiving checks | Regression across surface switching and historical requests |
| Native mobile/badge, cross-body routing, continuous audiovisual interaction | Future work or limited research prototypes | Not delivered as part of this baseline |

The human voice round trip confirms one successful real journey. It does not
establish statistical routing accuracy, faithful summaries for every session,
or complete coverage of the interactions between implemented features.

## Execution method

1. Use existing deterministic/process fixtures for ordering, fault injection,
   duplicate delivery, budgets and permissions. No model or physical recording
   is needed for those invariants.
2. Use a small number of real native workers for incorporation, semantic context
   and result correctness. Prefer inexpensive configured models; increase model
   capability only when task semantics require it. Verify task effects, not just
   a worker's completion declaration.
3. Use a short phone session with the human for actual intent, visual comprehension
   and hearing. Automate setup, measurements, assertions and cleanup. Ask the human
   only for judgments or physical behavior that cannot be established automatically.
4. Follow with ordinary use over several days, recording defects without requiring
   the human to follow a script all day. This establishes ecological usability;
   it does not replace fault-injection checks.

Each executed row records source revision, relevant configuration/model, case ID,
retained original/result identities, expected outcome, observed outcome, evidence
class and unresolved defect. A small campaign report links existing journals and
receipts instead of duplicating state. `Not run`, `failed`, and `uncertain` remain
distinct. Resume missing observations; never replay uncertain work to improve a report.

Use isolated test credentials/workers where needed. Do not replace the human's
active access file. Use synthetic destructive scenarios. A global Stop requires
a fresh explicit human request for that operation, including during testing.
Hardware capture and shared-device tests need their actual authorization/window.

## Acceptance scenarios

`A` means deterministic/process automation, `N` real native/provider execution,
and `H` human judgment. These are evidence needs, not separate mandatory test suites.
Combine rows in a few reusable journeys wherever practical.

| ID | Action | Expected user-visible result / evidence | Method |
| --- | --- | --- | --- |
| Q01 | Discuss a possibility without requesting work; then explicitly request action | Useful local discussion; dispatch only when warranted. Inspect actual admissions | A, N, H |
| Q02 | Use a deliberate leading selector, reply to a result, then override reply focus; quote a selector as content | Selected destination follows explicit intent; quoted/incidental tags do not launch work | A, H |
| Q03 | Offer overlapping topics with different relevant histories; add an ambiguous message and a genuinely new subject | Route from source-linked context; ask when necessary; do not manufacture a session to escape ambiguity. Compare with the human's intended acceptable outcome | A, N, H |
| Q04 | Let two meaningful tasks overlap and finish in reverse order while continuing discussion | Both keep progressing; intermediate communication and final results remain attributed and legible; neither result disappears behind the other | A, N, H |
| Q05 | Correct a running task; also delay preparation of an older voice input until newer text is accepted | Appropriate steer/interrupt reaches work in time; later-captured correction prevails in actual output. Inspect native acknowledgment and task effect separately | A, N |
| Q06 | Change context after an observer card was generated; include a retracted fact/open question | Refresh uses the relevant delta/source cut; correction survives consolidation; stale or unavailable context is visible rather than invented | A, N, H |
| Q07 | Ask a worker a question / trigger a native clarification while other work is active | The question reaches the current requesting conversation; answer reaches the correct waiting work; it is not buried inside collapsed diagnostics | A, N, H |
| Q08 | Explicitly address another local topic from a session | Authored dependency and correlated return stay visible; no quoted dispatch, recursive activation or silent unsupported adapter path | A, N |
| Q09 | Stop playback during speech; separately issue a task-specific cancellation/interrupt | Audio stops promptly without silently cancelling work. Execution cancellation has its own matched evidence; uncertain external effects are not called rolled back | A, N, H |
| Q10 | Reload, lose connectivity, suspend/background the phone, then reconnect | Retained inputs/results recover once; no automatic replay of uncertain dispatch or surprise replay of old audio; fresh useful progress is distinguishable from history | A, N, H |
| Q11 | Fail ASR, routing, a worker and TTS at their respective boundaries | Retained state explains the problem, text remains usable, known-unsent and uncertain work differ; no invented transcript/result or retry side effect | A, selected N |
| Q12 | Deliver a final answer covering several old requests; deliver the same native event twice | One canonical presentation per requesting conversation, correction-aware revision/history; no copy per historical request and no visible metadata tail | A |
| Q13 | Use routed work and ordinary direct input in the same session while quota is active; inspect CLI, session Telegram topic and integrator | The topic also projects attributed integrator inputs, progress and answers once, without creating another input or task. The integrator still receives its correlated return. Direct input remains ordinary, with no router admission/quota charge; no internal JSON or metadata tail leaks | A, N, H |
| Q14 | Reach the configured execution budget with many retained contexts | Inputs remain visible and recoverable; direct work is responsive; bounded actual execution and waiting are truthful. Record context count separately from running workers | A, selected N |
| Q15 | Speak mixed Spanish/English; queue several speech chunks/results; start speaking during output | Understandable transcription, natural spacing, no overlapping playback, clear capture state and cancelable queued speech. Judge actual sound, not just the `ended` event | A, N, H |
| Q16 | Complete transcription, fail transcription, render speech, then exercise expiry/size limits | Durable text is retained; successful source audio is removed; failed/uncertain audio is bounded and identifiable. TTS pruning matches its documented cache policy, without pretending immediate playback deletion | A |
| Q17 | Use a narrow screen, large text and keyboard; open read-only diagnostics during parallel work | Main text/voice interaction remains readable and reachable. Diagnostics start collapsed; truthful compact activity is understandable. No overlap or inaccessible clarification | A browser, H |
| Q18 | Use a simple/advanced presentation and an independently read-only monitor credential | Read-only inspection stays accessible; presentation does not grant destructive authority. If user profiles are not implemented, mark them planned rather than certifying them | A, H |
| Q19 | Select an immutable image/reference; later change the live scene | Work uses the selected capture/revision and coordinates; uncertainty prompts clarification; no claim to see a changed scene | A, selected N/H |
| Q20 | Interview completed and busy source sessions through an explicitly selected observer finding | Exact fork only where supported; derived/pinned context otherwise labeled. Interview does not mutate source work; publication is a separate selected act | A, N, H |
| Q21 | Request an artifact from routed work; revisit after session renewal/compaction | Correct continuity and truthful access to retained output. Current routed adapter is text-only: no false file-delivery claim. Automatic authorized artifact return is a future capability to qualify once implemented | A, N |

## Human journeys

Proposed sessions are short and adjustable, not reserved appointments:

- Text: overlapping familiar contexts, one ambiguity, one local discussion and a
  deliberate correction. The human states intended destinations before seeing
  outcomes where possible, without turning the implementation's guesses into labels.
- Voice: continue the same work, introduce a second task, answer a question and
  interrupt playback. Include mixed language and natural pauses; judge both
  intelligibility and whether the conversation is easy to follow.
- Continuity: reconnect on the phone, inspect the collapsed monitor, switch to an
  ordinary direct topic and return. Explore one selected reference or interview
  only if useful, rather than manufacturing irrelevant tasks.

The system should explain enough that the person can describe what is happening
without reading raw JSON. Children using a simple interface and accessible
read-only inspection are a design target, not a claim of tested child usability.

## Measurements and acceptance

Measure the journey by original input: end of speech/commit, transcript ready,
routing decision, native acceptance, first useful progress, final result and
playback start/end. Separate queue, preparation, inference and execution time;
report distributions and case counts, not one impressive total or batch latency.
Record model calls/tokens and attributed service costs where observable; missing
billing stays unknown.

Routing measures include wrong destination, necessary/unnecessary clarification,
unnecessary new context and appropriate local conversation. Keep ambiguous
acceptable sets; do not punish a useful clarification just to increase accuracy.
Use held-out human intent distinct from synthetic cases and real corrections.
Inspect observer factual fidelity, retained corrections, open questions, source
coverage and freshness. A fluent card or a worker coverage declaration is not proof.

For the campaign's exercised cases, require no silent input loss, duplicate task
effects, wrong audience, involuntary direct-topic interception or false delivery
claims. Required corrections must affect the resulting work. Every unresolved
failure blocks acceptance of its capability, not unrelated exploration. Finite
success is evidence for exercised conditions, not a claim of universal zero failure.

Set latency/cost targets after the baseline is measured; do not invent production
SLOs from synthetic tests. Human acceptance additionally requires a comprehensible
screen and comfortable turn-taking. Record actual judgments and examples.

Retain transcripts, event timings and selected evidence under their existing
audiences. Do not build a raw-audio archive or publish private conversations as a
research corpus. An explicitly selected diagnostic sample needs its own purpose
and retention; routine audio follows the existing deletion/cache policy.

## Goals, directed controls and visible session lifecycle

Human requirements added 2026-10-19; pending implementation. A habitual integrating
conversation should also make session lifecycle understandable in Telegram,
without requiring the dashboard or CLI.
The [harness control study and delivery plan](integrator-controls.md) records the
installed capability inventory, supported external controls, recovery choices
and exact distinction between human-directed commands and delegated autonomy.

- Distinguish an active native goal (suggested marker: target) from actual
  execution without an active goal (suggested marker: lightning). An active goal
  can be waiting; its marker must not claim current execution. Idle sessions
  need no ordinary activity marker. Paused/blocked goals and human questions
  remain inspectable and must not be mistaken for completed work.
- Offer a compact inventory such as active goals, executing sessions without
  goals, and currently passive sessions with conversational activity in the
  last configurable X days. Seven days is an initial proposal. Count actual
  attributed exchanges, not status reads, metadata polling or title updates.
  Expose observation freshness and unknown lifecycle state. A topic and a
  renewed native thread are not automatically two user-visible conversations.
- Preserve human titles, topic icons, stable selectors and native bindings.
  Status decoration must not become canonical identity or a rename loop. Reuse
  the existing activity observer/catalog; this calls for no second Telegram
  consumer, model wakeups or new background attention.
- Accept explicit human-directed goal controls through the integrator, with
  a resolved target and native outcome. For example, a human's instruction to
  resume one session's goal or clear it before proceeding with a replacement
  should not require manually visiting that topic to type a slash command.
  Keep control intent separate from text forwarded to the worker; validate
  the current goal/binding and retain the actual acknowledgment. Quoted slash
  commands or a model's proposal are not themselves human control requests.
- Clear is not interrupt, and changing a goal does not itself undo completed
  effects or cancel running work. An ambiguous target or replacement requires
  clarification. Do not clear a goal merely to make an unrelated next message
  fit. Concurrent human changes must invalidate a stale control selection.

Current native `update_goal` permits complete, blocked, and explicitly requested
pause, but not resume or limit changes. The checked native runtime can also mark
goals blocked on turn errors or execution unavailability. Improving agent-owned
lifecycle control requires a supported harness capability and an explicit
delegation model. Sending the same agent-originated decision through another
session or a user-origin API must not manufacture human authorship. Today's
integrator-directed human controls and future delegated autonomy are separate
capabilities to evaluate.

Extend Q13/Q14/Q18 to exercise: active-goal/idle vs executing/no-goal display,
completion and blocking, recent-idle count ageing, observer disconnect, explicit
human resume/clear, ambiguous targets, stale goal changes and exactly one control
effect. Reads and projections must never activate or clear a goal. Preserve
budget, explicit pause and Stop semantics. Unknown resume/control outcomes are
inspected, never automatically replayed.

## Development sequence and creative horizon

1. **Usable surface and broad qualification.** Implement the human's simple
   text/voice view, collapsed read-only diagnostics, clear compact activity and
   cancelable spacing between speech segments. Project integrator activity into
   the corresponding session's Telegram topic as well as CLI, preserving the
   correlated integrator return and direct-topic independence. Run affected browser/audio checks,
   then the integrated campaign. User-profile configuration is a separate
   presentation/access requirement; CSS hiding cannot implement permissions.
2. **Attention and semantic reliability.** Improve observer fidelity and intent
   evaluation from observed failures. Compare classifier/embedding candidates on
   held-out intent before adopting them. Coordinate timely questions, grouped
   progress and results without making the human manage every worker. Priorities
   for interruptions and summaries should be inspectable and adjustable.
3. **Continuity of useful work.** Close artifact-return and session-renewal gaps,
   improve reconnection/background use and verify long mixed sessions. Preserve
   source evidence and ordinary direct interaction. Explicit session dependencies
   must work through the actual supported native adapter.
4. **Replaceable realtime frontends and mobile button.** Compare native realtime
   services against the working modular ASR/text/TTS baseline over the same
   controller. Preserve interruptible speech separately from work cancellation.
   Develop a minimal native mobile client and then qualify real Bluetooth button,
   headset and OS background behavior. Android/iOS parity and an arbitrary headset
   power-button binding are not assumed.
5. **Situated perception and selected observer consumers.** Extend selected frames
   toward useful live visual context only where the task benefits. Add explicitly
   enabled interview/editorial consumers with source provenance and separate
   publication. Do not convert every observed event into a new task or retain
   continuous raw media by default.
6. **Multiple bodies after local polish.** Complete canonical Matrix correlation,
   receiving adoption and device consumption under #163 and Matrix #367/#373.
   Distinct identities, audiences and observed delivery remain explicit. This is
   deferred scope, not a prerequisite for a usable single-body product.

Steps 1–3 are the immediate product path. Later tracks can share the stable
controller; they do not require throwaway intermediate architectures. Their order
depends on observed value, rather than promising a date for every imagined feature.

The broader Field/Nexus direction is a situated doorway into ongoing thought:
move between a simple conversation and inspectable relationships among projects,
experiences, evidence and perspectives. Encounters can become exploration,
research, interviews, decisions or creative work without turning all conversation
into tasks. A child can use the button and later explore what happens beneath it.
Voluntary co-presence and richer spatial interfaces remain a creative horizon,
not capabilities delivered by this routing milestone. Shared context does not
imply a universal memory pool or automatic disclosure of personal sessions.

## Research grounding

The collective research on integrating conversations (2026-10-15) and native
WebRTC (2026-10-18) motivates the architecture; it does not establish product
acceptance. The following primary references explain specific design lessons:

- [JarvisBench](https://github.com/cchen1436/JarvisBench): coordination includes
  allocating scarce human attention, asking useful questions and returning
  judgments to execution. Evaluate both directions, not only destination labels.
  Its distinct intervention/replay tracks should not be presented as proof of
  20 simultaneous production tasks or a drop-in runtime.
- [Qwen Live Harness, inspected revision](https://github.com/QwenLM/Qwen-Live-Harness/tree/b2d17b8706b226ff7792683ecd8f050f18f9752e):
  separate conversational interaction from asynchronous execution and speech
  scheduling. Its display reports are not our durable observer; the desktop
  implementation is not an already-integrated Linux/mobile client.
- [Magentic-UI](https://www.microsoft.com/en-us/research/publication/magentic-ui-report/):
  shared, inspectable work and collaboration suggest keeping the workspace
  available beneath the chat, rather than forcing all understanding into speech.

Use the configured collective corpus for the complete evidence ledger and
provider-access findings. New model releases warrant targeted research when a
decision depends on them; they are not a reason to postpone qualifying this MVP.
