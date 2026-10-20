# Sonic expression

The conversation client offers **State sound**, off by default. Clicking it
deliberately unlocks this browser's Web Audio context. It does not record audio,
start inference or alter work. The score and volume control live under **Under
the hood**. Muting, Stop audio, leaving the conversation, hiding the document or
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

## Authored vocabulary

The default `daimon.sonic-expression/v1` vocabulary is Source / CompAII's
**Source hum**, revision 1: 65 Hz sine anchor, 0.22 Hz breathing gain, 100 ms
attack and 180 ms release. Initial interval choices are working 1:3, waiting
1:7, blocked/limited 1:11, active goal without current work 1:2, unknown 1:13
and passive silence. Here 1:N sounds at N times the fundamental. Waiting and
blocked associations come from Nicolás; the remaining composition is authored
by Source, drawing on NaturalHarmony and harmonic-shaper. These are artistic
choices, not universal consonance, emotional or physiological claims.

An owner may supply a complete `sonic_vocabulary` object in dashboard
configuration. `dashboard/sonic.py` defines its fields and validation: 20–256 Hz
fundamental, sine/triangle, 0–4 Hz pulse, 20–2000 ms envelopes, 1–32 session
voices and complete state-to-ratio arrays. Empty arrays mean silence; two
positive integer terms define a ratio. Catalog ID, revision, author and label
are required. The server transmits exact canonical vocabulary JSON and SHA-256;
the browser verifies them before rendering. No executable macros or model loop
are involved. Each being may choose a different vocabulary.

## Composition and speech priority

Voices are ordered waiting, blocked, working, goal-idle, unknown, passive, then
stable source identity. The default mixes at most 24 session voices plus one
anchor. Repeated intervals remain separate source voices. Tones beyond the
browser sample-rate limit are omitted and reported. Old/new groups crossfade
with at most two groups alive; owned timers and oscillators are canceled on
Stop. Gain is normalized across voices and bounded by the authored peak limit
(default 0.08 before the user volume). A zero volume keeps the score visible.

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
comfortable, recognizable sound. Complete S01–S05 in
`docs/interaction-qualification.md` against live sessions, then retain separately
attributed human listening observations.
