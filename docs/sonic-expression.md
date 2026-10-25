# Sonic expression

The independent conversation client offers **Beacon**, off by default. Clicking it
deliberately unlocks this browser's Web Audio context. It does not record audio,
start inference or alter work. Activation, suspension, quieting, errors and mute are visible in the conversation.
The volume appears there when listening is activated; the detailed score remains
under **Under the hood**. Muting, Stop audio, leaving the conversation, hiding the document or
going offline closes its owned context and cancels transitions. Returning does
not resume playback without another click.

## Observation and scope

Set the dashboard's `activity_state_dir` to the existing activity indicator's
private state directory, which contains its database-bound `topics.json`. Reuse
the receiving indicator's actual selected directory; do not create another
observer. Without that configuration the UI reports that state observation is
unavailable. The read-only authenticated `GET /api/interaction/sonic` endpoint
performs a finite cache/SQLite read; it never starts native work. Both restricted
conversation devices and read-only monitor devices may inspect it.

Sources are the integrating topic and the actual canonical routing registry,
including configured, enrolled and optionally discovered targets (at most 64),
filtered against current ownership and native binding. The existing owner-control
input-list read supplies membership; a failed registry read is unavailable rather
than a fallback to static targets. Each source uses stable
`chat:topic` identity, its current logical session, observation time, execution,
goal and native human-wait flag. A missing/legacy wait flag, disconnected cache,
changed database/native binding or observation older than 20 seconds means
unknown. Browser elapsed time and request duration also age the observation.
The score distinguishes omitted, unconfirmed and unavailable sources. It does
not enumerate arbitrary private sessions or infer emotion from conversation.

## Required composition — human correction, 2026-10-22

Use the Beacon series of **40.4**, with additive session-state components:

| Observed condition | Component |
| --- | --- |
| Idle session | F1 — 40.4 Hz |
| Active session | Add F2 — 80.8 Hz |
| Running tool calls or subagents | Add F3 — 121.2 Hz |
| Active goal | Add F4 — 161.6 Hz |
| Pending human action | Add F7 — 282.8 Hz |
| Blockage or error | Add F11 — 444.4 Hz |

Conditions coexist: an active session with an active goal and running tools must
retain F1, F2, F3 and F4 together. Pending/error components do not
replace the activity or goal components. F1 is the 40.4 Hz fundamental; Fn = n × 40.4 Hz.
Use only sine oscillators with gently pulsing amplitude. No added overtone stacks,
triangle waves, 18 Hz roughness texture or frequency modulation. Do not
replace this specification with octave folding, a 65 Hz base, arbitrary per-note
partials, a tone for unknown state, or a mutually exclusive state classifier.
Unknown evidence remains visibly unknown; it is not an error or idle assertion.
Tool/subagent, pending and error components require their actual current evidence,
not inference from `execution=working` or from one generic wait flag.

The default `daimon.sonic-expression/v2` Source Beacon vocabulary (revision 4)
implements this composition. Revision 2 was rejected after the human reported
excessive high harmonics; its old spectrum measurements are historical evidence,
not acceptance. The renderer uses 0.22 Hz sinusoidal amplitude breathing with
25% depth and phase-continuous carriers. Weight changes use a shared 350 ms linear transition (the larger attack/release duration), and volume/voice quieting uses a 12 ms fade. No additional partial stack exists.
An owner can supply a complete v2 vocabulary; legacy v1 palettes fail explicitly
rather than silently restoring the rejected sound.

The observer exposes actual outstanding rollout calls or currently active linked
subagents as `tools_running`, and native `systemError` as `error`. Blocked/limited
goal states also contribute F11. Pending comes from the existing human-request
ledger plus a current native approval/input wait. Missing legacy tool/error flags
remain unknown. Known independent components can still sound; missing data never
creates an unknown-state pitch. Stale/disconnected sources are silent.

## Composition and speech priority

The score lists every contributing F component per session. At most 24 sessions
sound, with waiting/blocked/working sessions prioritized before idle sessions.
Each session's sine components share its normalized gain budget; the sum is
normalized across included sessions. Idle sessions contribute F1 rather than
silence. Repeated frequencies retain their session membership in the score and share one persistent oscillator in the audio graph. The default maximum
frequency is F11 (444.4 Hz), plus the narrow sidebands inherent in gentle amplitude
pulsing. Notes outside the browser sample-rate limit are omitted and reported.
All carrier weights transition together from their current envelope values, preserving a total weight no greater than one even under rapid changes. Zero-weight carriers are removed only after their fade; returning frequencies cancel retirement. Stop fades before closing its owned context. The default peak limit is 0.6 before user volume (previously an effective 0.04); zero volume keeps the score visible. Muting/voice priority reaches exact zero after the 12 ms fade.

Microphone permission/recording/finalization, voice upload and preparing/playing
an owned speech reply silence the state layer. Idle and paused speech do not.
This prioritizes intelligibility without canceling work or speech; sound remains
off unless explicitly activated. Browser suspension is reported and requires a
tap to resume. No microphone, external synth or hardware audio process is opened
by the state renderer.

## Qualification boundary

`tests/dashboard_sonic.py` covers actual private cache/SQLite/HTTP audience,
freshness and revocation. `tests/dashboard_sonic_ui.js` exercises shipped mapping,
bounded voice membership, node/timer ownership, expiry and client lifecycle with
an instrumented audio graph. A muted Chromium development journey additionally
checked real browser activation and actual OfflineAudioContext samples, using
synthetic states/native peers. These checks do not prove receiving operation or
comfortable, recognizable sound. The corrected-series qualification measures actual OfflineAudioContext output,
all six selected frequencies, absence of the rejected high partial stack, and
exact mute/speech-priority silence. Complete S01–S05 in
`docs/interaction-qualification.md` against current sessions and retain human
listening observations separately; no human acceptance is inferred from tests.

Human real-use feedback on2026-10-25 rejected the low maximum volume and reported continuous clicks. Revision4 raises available gain15x and removes whole-group oscillator restarts and abrupt master changes. Offline audio checks exercise rapid state/volume/quiet transitions as well as quiet/stale silence; they do not rule out device buffer underruns or substitute for physical listening acceptance.
