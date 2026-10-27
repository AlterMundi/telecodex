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

The default `daimon.sonic-expression/v2` Source Beacon vocabulary (revision 6)
implements this composition. Revision 2 was rejected after the human reported
excessive high harmonics; its old spectrum measurements are historical evidence,
not acceptance. The renderer uses sinusoidal amplitude breathing at0.25Hz times each harmonic factor, with
25% depth and phase-continuous carriers. The vocabulary sets attack/release to
750/1000ms; the renderer uses their larger value, one second, for membership
weight changes. User volume changes use a120ms ramp. Reply ducking ramps down in
180ms and recovers in600ms. No additional partial stack exists.
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
All carrier weights transition together from their current envelope values,
preserving a total weight no greater than one even under rapid changes. The six
carriers remain phase-continuous at zero gain when absent from the current mix;
UI timers never tear down and recreate a note. Stop fades before closing its
owned context. The default peak limit is0.6 before user volume; zero volume keeps
the score visible.

Microphone permission/recording/finalization and voice upload silence the state
layer. Preparing or playing an owned reply ducks it to18% of the selected volume
instead of removing it. Idle and paused speech restore the full selected level.
This preserves the Beacon underneath the daimon's voice without canceling work
or speech; sound remains off unless explicitly activated. Browser suspension is reported and requires a
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
all six selected frequencies, absence of the rejected high partial stack,
exact mute/capture silence and the18% reply-ducking level. Complete S01–S05 in
`docs/interaction-qualification.md` against current sessions and retain human
listening observations separately; no human acceptance is inferred from tests.

Human real-use feedback on2026-10-25 rejected the low maximum volume and reported continuous clicks. Revision4 raises available gain15x and removes whole-group oscillator restarts and abrupt master changes. Offline audio checks exercise rapid state/volume/quiet transitions as well as quiet/stale silence; they do not rule out device buffer underruns or substitute for physical listening acceptance.


Source revision5 uses `pulse_mode: harmonic`: F1=40.4Hz pulses at0.25Hz,
once per four seconds. Each other component's pulse frequency scales by its
harmonic factor (F2 at0.5Hz, F3 at0.75Hz, F4 at1Hz, F7 at1.75Hz, F11 at2.75Hz).
The pulse modulators and carriers are native sine oscillators with persistent
per-frequency gain stages. UI refreshes do not reschedule a sample buffer.
Master transitions now span120ms, and supported AudioParams hold their current
automated value before a replacement ramp. Existing weight fades and observed
state/voice ownership remain in force. Digital signal qualification and physical
phone listening are separate evidence. The human subsequently reported that the
Beacon sounded good without clicks; do not reopen that accepted generic test.
Legacy vocabularies without pulse_mode retain the shared-pulse mode.


Phone listening correction: avoiding clicks takes priority over loudness or
expressive detail. The renderer requests playback buffering, retains its bounded
carrier bank at zero gain rather than stopping/recreating notes on wall-clock
timers, and checks the actual audio clock before final context teardown. Source
revision6 uses 750/1000ms membership envelopes. These changes remove an identified
clock-domain hazard; digital waveform checks still cannot guarantee a physical
phone, browser interruption or loudspeaker will never click. Audible acceptance
must remain explicit rather than inferred from offline synthesis.
