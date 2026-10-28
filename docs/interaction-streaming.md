# Continuous voice: streaming recognition preparation

Owner: [v0.3 plan #301](https://github.com/AlterMundi/telecodex/issues/301),
[architecture #302](https://github.com/AlterMundi/telecodex/issues/302) and
[streaming/recovery #271](https://github.com/AlterMundi/telecodex/issues/271).
The full horizon remains [the maintained roadmap](interaction-roadmap.md).

## Current implementation boundary

CompAII's selected receiver enables `streaming_transcription` in `prepared`
mode with Gemini `gemini-3.5-transcribe-live`, plus `speech.streaming_output` and
`speech.incremental_segments`. PR341 was receiving-adopted on 27 October 2026,
with successful CI40449397345. Final input text is reused for its exact original
WAV; Groq file-ASR remains the fallback. The private streaming credential never
reaches the browser. Sol remains the integrating model and Qwen remains TTS.
Portable defaults remain off. Streaming output is source-bound and uses the
existing HTML audio player. These flags do not establish measured end-to-end
latency or complete v0.3 acceptance. Current receiving software, health and
rollback are recorded through
[Shared Resources](https://github.com/compaii/shared-resources/blob/main/docs/services/interaction-receiving.md).


The optional app integration connects `TranscriptionStream` to the existing
microphone through an authenticated same-origin WebSocket. In `captions` mode,
revisable text appears while speaking, while the ordinary compressed MediaRecorder
retains the canonical original. Send closes the preview immediately and uploads
that original once through the existing final-file ASR path. Captions never
prepare trusted text or delay Send waiting for a recognition final. This preserves
the accepted final transcriber's mixed-language command quality; it does not
claim faster final recognition. Preview connection, backpressure or audio-graph
startup failure closes only the preview and preserves the recording.

The selected `prepared` mode prepares server-derived text for an exact finalized
WAV. The alternative `captions` mode retains compressed file-ASR originals. The selected
mode is server configuration, checked before opening the provider. A stale client
cannot choose the other contract. Both modes remain disabled by default.

Output streaming is separately delivered through the existing HTML audio element
(see below). Neither this input slice nor output streaming completes endpointing,
barge-in, foreground separation or the continuous-voice milestone.

The retained alternative Qwen Audio3.0 route uses **input transcription only** with
explicit manual mode. It never sends `response.create`, tools, conversation
history or a native prompt. Sol remains the conversational/integrating model.
Unexpected generated-response events fail the stream and are never played.
Keep this separate from the rejected Qwen conversational experiment.

The existing account's read-only model catalog and bounded synthetic tests showed
that the separately documented streaming-ASR model names were unavailable on its
route. The available audio session returned provisional transcripts during input
and a final transcript after explicit recognition commit, without a conversational
response. This is evidence for the tested account/route, not global model access
or a provider-quality ranking. Mixed-language command recognition remains imperfect;
do not silently replace the accepted final transcriber on this evidence alone.

## Gemini transcription route

The adapter also accepts `provider = "gemini"` with the exact endpoint shown in
`dashboard/config.example.toml`. It uses the dedicated
`gemini-3.5-transcribe-live` model, a private server-side `x-goog-api-key` header,
`VERBATIM` transcription and disabled automatic activity detection. After setup,
one manual activity starts; explicit Send ends it. Interim hypotheses replace
previous text; only a final input transcript after Send can prepare trusted text.
Generated answers, tools and output audio are rejected. Text and binary UTF-8 JSON
frames share the same event-size bound. Original audio binding, authentication,
receipt persistence and file-ASR fallback are the existing shared path.

A single 5.76-second synthetic Spanish/English trial on 27 October 2026 succeeded
after the human funded the selected project. The final preserved `git status`
and arrived approximately 330 ms after the last audio chunk. An earlier attempt
was explicitly refused before setup or audio because prepayment was depleted.
This confirms access and one sample, not longer-utterance quality, a latency
distribution, receiving deployment or device acceptance. A separate 15.52-second synthetic journey through the authenticated application
WebSocket and production adapter preserved a two-second pause, routing negation,
spoken self-correction and `git status` / `git diff`. Final preparation arrived
425 ms after Send; the exact original WAV reused its durable transcript without
file-ASR or native dispatch. These two journeys qualify this input slice only;
receiving adoption verified the successor process, selected configuration and public
assets while preserving native work, shared-auth binding and TTS. Human-device
acceptance and end-to-end latency remain separate.
See Google's [transcription guide](https://ai.google.dev/gemini-api/docs/live-api/live-transcribe)
and [Live API contract](https://ai.google.dev/api/live).

## Transport contract

- The operator supplies an exact `wss` endpoint and a private credential-file
  reference through the existing receiving configuration path. No credentials in
  browser code, query tokens, tracing, source or issues. Redirects and automatic
  reconnection/retry are disabled.
- Send explicit `turn_detection: null` and text-only modality. Wait for
  `session.updated` before audio. The actual provider acknowledges the update but
  omits manual-mode configuration from its echo; reject a contradictory mode,
  model or modality instead of requiring an undocumented echo field.
- Input is mono16kHz PCM16, ordered on one socket, bounded to120seconds and
  32,000bytes per chunk. Text/events have independent bounds. One stream owns
  one provider transcription item.
- Partial `text + stash` replaces the previous revision. Early hypotheses can
  change substantially; they are captions/preparation, never executable input.
- `finish()` sends only `input_audio_buffer.commit`. Final text is usable only
  after a nonempty completed transcript for that same item. A disconnect/error
  leaves a terminal failed stream; no automatic replay or invented final result.
- `close()` before recognition finish discards preparation without committing.
  Closing output audio or this preparation stream does not cancel native work.

## App and native preparation boundary

`/api/interaction/voice/live` accepts only the configured origins and reuses the
existing browser/shared-login or companion grant, CSRF, actor authorization and
receiving binding. Read-only grants cannot open preparation. Revocation and
binding changes are checked again before recognition finish and retained text.
One active capture per actor and two per receiving server are bounded to140seconds;
audio itself is at most120seconds. A canceled connection setup closes its late
provider socket. No provider credential reaches the browser.

The worklet resamples the device rate to16kHz with continuous interpolation across
blocks and a7400Hz input low-pass filter. Its graph output is muted.100ms chunks
feed recognition, with bounded socket backpressure. Captions replace rather than
append and never alter the draft, selected image/reply context or execute a turn.

In `prepared` mode, only explicit Send uploads the finalized original WAV through the existing
HTTP voice endpoint, with the capture UUID chosen before recording. The duration
limit can end recognition for review without submitting work. Its headers,
source SHA, selection and uncertain-receipt recovery use the existing microphone
state machine. Discard closes preparation without recognition commit or original
admission. Socket failure or unavailable/late preparation keeps the complete local
WAV for ordinary file-ASR; there is no provider reconnect or automatic audio replay.

Only `prepared` mode completed recognition writes an immutable owner-only text receipt identified by
capture UUID, binding, PCM SHA, canonical WAV SHA, model and duration. It stores no
raw preparation audio. The native `interaction_asr.py` helper may consume it only
for the exact original UUID/WAV, and validates its final state, format, duration,
model and both hashes. It durably commits its ordinary ASR result before returning
text. Existing completed or uncertain native preparation always takes precedence;
a later live receipt cannot replace an uncertain provider operation. Missing live
receipts retain ordinary file-ASR fallback; mismatched present receipts fail
visibly. Browser-supplied caption text is never trusted for admission.

## Receiving configuration and rollback

Use the existing optional `[streaming_transcription]` configuration documented in
`dashboard/config.example.toml`: `enabled`, exact `endpoint`, private
`credential_file`, `cache_root`, optional `provider` (`qwen` by default) and
optional `mode` (`prepared` by default).
For `mode = "captions"`, leave native ASR configuration unchanged: no
`streaming_cache_root`, no trusted preview receipts and no canonical PCM copy.
The browser retains its normal compressed MediaRecorder original. For prepared
mode, the selected native ASR configuration must reference that same absolute
directory as `streaming_cache_root`. Keep its existing
helper, route binding, durable cache and file-ASR provider. Describe the configured
transcriber honestly as streaming recognition plus the existing file fallback;
do not keep a file-provider-only label after enabling this path.

The locked receiving dependencies include `wsproto` for the Uvicorn WebSocket
server, as required by its [installation contract](https://uvicorn.dev/installation/).
The server advertises `can_stream_transcribe` only when configured and authorized;
unsupported browsers and unavailable worklet setup keep the current MediaRecorder
path. No new microphone permission, login, conversational model, or input journal
format is introduced. Disable streaming to restore ordinary file-ASR for new
captures; preserve already retained originals, uncertain operations and native
ASR results. An explicit completed-only recovery read may adopt a later verified
streaming receipt for the exact same original even when its file-ASR job is
already unconfirmed. It preserves that uncertain job and its evidence, invokes
no provider, and does not claim its preparation audio was deleted. Missing or
mismatched receipts remain unconfirmed; ordinary ASR calls never retry that job.
This configuration does not resume/pause goals or cancel work.


## Qualification

`python -B tests/dashboard_transcription.py` exercises partial replacement,
manual readiness, refusal of generated answers, item identity, malformed/bounded
input, explicit finish/discard and terminal disconnect without reconnect. It uses
a synthetic provider, no microphone or native work. `tests/dashboard_live_transcription.py`
exercises actual authenticated WebSockets, actor/grant revocation before delayed
captions, immutable prepared text, binding replacement, canceled dial/close slot
reuse, and disconnect/discard with no dispatch. Its loopback Uvicorn journey
qualifies the locked WebSocket server and native exact-WAV receipt reader.
`node tests/voice_stream_ui.js` checks the real worklet at16/44.1/48kHz, block
continuity, WAV identity against Python's standard wave writer, partials, ordered
finish, discard, setup cleanup and complete local fallback after network failure.
Caption-mode regressions additionally prove exact compressed original delivery
without a provider-final wait, including disconnected/not-ready sockets and
preview AudioContext resume/connect failures. Server tests reject cross-mode
clients and prove that captions cannot write a trusted final receipt.
The existing microphone suite verifies that live preparation and the ordinary
original upload retain one capture identity. Native ASR tests verify exact receipt
matching, durable recovery and precedence of uncertain file transcription.

A Chromium journey with a synthetic MediaStream and preview socket confirmed
that real AudioWorklet captions arrived before Send and all five compressed
MediaRecorder chunks retained their exact bytes (9,443bytes, Opus/WebM). The
last original bytes arrived3.3ms after Send without a provider recognition commit.
This qualifies browser mechanics only: no hardware, provider or native admission.

A separate real-provider adapter journey used one5.76second synthetic Spanish /
English clip paced at100ms. First provisional text arrived about0.62seconds after
audio transmission began; final text arrived about0.43seconds after the last
chunk. Connection setup was about1.49seconds. These are point observations on a
server-side synthetic journey, not physical-device latency, final intelligible
response latency or human acceptance. Retain that evidence rather than repeating
paid provider calls for unchanged code.

Primary protocol sources:
[Qwen Audio client events](https://www.alibabacloud.com/help/en/model-studio/fun-audiochat-client-events),
[Qwen Audio guide](https://www.alibabacloud.com/help/en/model-studio/qwen-audio-realtime-user-guides).

## Incremental spoken segments (optional output preparation)

The ordinary speech binding may opt into `incremental_segments = true`. The
server then synthesizes an exact canonical prefix: up to240 Unicode characters
for the first segment, up to640 for continuations, preferring a sentence or word
boundary. Source labels are still spoken once; native questions retain their
existing label/offset contract. The default remains the existing full-part path.
The server revalidates the full source and current actor after synthesis, then
returns the prefix end offset, text digest and existing WAV digest.

Before unmuting, the browser checks that prefix against the current canonical
reader. Follow uses the segment actually played. While it plays, at most one
next segment is prepared; continuation consumes that same retained success or
failure. An ambiguous failed preparation is never automatically repeated. Pause
retains the owned preparation; Stop releases/aborts it and prevents late playback.
No subsequent worker response is synthesized speculatively.

This reduces the amount of committed text required for the first audio and
allows the next segment to prepare during playback. It is not token-level native
LLM output streaming, a measured latency result, or completion of foreground
separation/barge-in. The option is receiving-enabled in CompAII’s selected app after actual-provider
and Chromium qualification (PR316). This does not change its conversational model.
Streaming input captions are now receiving-enabled; current prepared recognition
uses Gemini with exact-original reuse and final-file fallback (PR341). The earlier deployment record below
describes PR316 at its adoption time, rather than the current software selection.


### Receiving qualification and next transport

PR316 passed CI40415564333 and independent review. Real Chromium at phone,
keyboard-sized phone and desktop dimensions exercised exact prefix validation,
0→240→880 offset ordering, one-ahead preparation, Pause/Play reuse, Stop and
composer containment with a synthetic audio element and no audio-device opens.
Native image-array support from PR300 was selected before these web assets;
at that adoption native source was d6667ba8 and web/scripts source884f884. Active native
collectors were reattached without replay, kickoff or goal changes. Retain the
collection-capable reader when rolling back; never restore old input journals.

One synthetic mixed-language text (1,149 characters) on the existing
qwen-audio-3.0-tts-plus / longanlufeng route took18.74seconds to synthesize in
full. Its228-character first segment took6.23seconds and produced14.88seconds
of audio; the following626-character segment took15.63seconds. These are
single uncached server observations, not a latency distribution or phone/hearing
acceptance. The next segment can still outlast current playback; a gap remains
possible. Source-linked batch preparation is useful but insufficient for the
continuous-voice target.

A separate, single bounded request qualified the same selected model/voice's
[documented SSE output](https://www.alibabacloud.com/help/en/model-studio/qwen-audio-tts-http-api)
with `X-DashScope-SSE: enable`, mono24kHz PCM and no URL download. First audio
bytes arrived at1.58seconds; completion at3.36seconds supplied468,480 bytes
(9.76seconds of PCM) in23 events. No physical playback or native admission ran.
This proves the receiving route accepts streaming output; it does not prove
intelligible sound latency or a working browser streaming player.

The next output slice must connect this stream to a bounded authenticated
browser player, preserving canonical source/revision checks before audible
output, one owned provider attempt, cancellation without native-job cancellation,
played/remaining position, explicit Voice controls and existing batch fallback.
Do not repeat synthesis to repair an uncertain observation, send credentials to
the browser, or count this transport probe as delivered continuous voice.


### Progressive encoded output

The additional optional `[speech] streaming_output = true` setting advertises
source-bound progressive output. A compatible client requests `stream: true`
on the existing authenticated audio endpoint. Qwen uses its existing private
route/voice with SSE enabled and mono24kHz MP3; ordinary clients and native
questions keep the existing WAV path. No credential or provider URL reaches
the browser. The option defaults off until receiving qualification/adoption.

A durable request intent precedes the single provider call. Ordered bounded
frames carry sequence and per-chunk digest, followed by a confirmed whole-audio
byte count/digest. Incomplete or disconnected attempts remain uncertain, never
automatically replayed. Completed MP3 media shares the existing private quota,
retention and hash-checked cache; expired known media may be explicitly prepared
again. The server revalidates the current actor, source/revision and receiving
binding before each outgoing frame. Closing the HTTP reader releases only its
owned renderer process, without touching native work.

MediaSource appends verified MP3 chunks to the existing HTML audio element.
This keeps Pause/Play/Stop, one-ahead preparation, source validation before
unmuting, Media Session and current Beacon integration on the same player.
Unsupported clients choose batch output before requesting synthesis; a failed
stream never silently triggers another provider call. Follow holds at the
segment start while duration is unknown, then uses actual finite media duration;
no total duration or word timestamps are guessed.

Qualification includes actual loopback HTTP delivery of the first frame while
the provider's terminal response is deliberately held, cache/revocation/source
replacement tests, client corruption/cancellation/EOF tests and real Chromium
muted decoding before stream completion. One distinct actual Qwen MP3 request
returned first bytes at1.57s and completed at3.73s. Those server observations and
muted browser decoding are not phone sound latency, background execution or
human hearing acceptance. Full foreground separation, heard/remaining attention
and streaming input quality remain unfinished under milestone9.

## Gemini receiving rollback

The selected receiving releases include PR341's dashboard/provider contract. The
prepared-transcript cache is shared with the existing native ASR adapter binding.
For UI/provider rollback, select the retained previous dashboard with the current
compatible completion reader, restore the previous dashboard configuration and keep
the streaming-cache reference in the ASR adapter until retained prepared originals
are consumed. Restart only the web unit. Never erase completed/uncertain receipts
or restore native history. The native configured transcriber label describes the Groq file fallback. The
owner helper also reports the source-bound streaming model, and new native
transcript provenance retains it separately as optional `actual_transcriber`.
Older receipts without that field remain readable; never infer actual provider
from the static configured label or rewrite history. Receiving adoption of this
additive provenance follow-up is recorded separately from PR341 input delivery.

Prepared completion recovery can retain a flat `asr-<identity>.json` in the same
private prepared-text store, rather than copying text into the file-ASR media
cache. The identity still covers original bytes, configured route and helper;
the existing file-ASR identity lock serializes both paths. Existing completed or
uncertain legacy jobs take precedence. File-ASR still durably establishes its
job namespace and intent before contacting a provider.

Flat recovery records share the web writer's64MiB/32768-file bound and quota
lock. They contain text/provenance only; no raw media. Full prepared storage
refuses a new flat completion, without evicting any outcome or disabling existing
legacy completion reads or ordinary file-ASR fallback. No automatic cleanup of
these records is installed. Retain this compatible completion reader during
rollback, even if streaming input is turned off; do not replace it with an older
helper that cannot read flat outcomes. Adopt after existing helper processes
have exited, without terminating or replaying them. Native journals and retained legacy outcomes are preserved. This removes a historical-cache scan and data-
volume durability barriers from the prepared completion path; it does not claim
an end-to-end latency target or explain every reported wait.

## Per-session automatic audio attention

Worker update/result titles expose a speaker toggle. Muting uses the stable
session target, never its display name: text stays visible, queued automatic
updates are removed and current automatic playback from that session stops.
Explicit Listen remains available, live questions keep their existing priority,
and neither native work nor global Voice is paused. Unmuting admits future
updates without speaking a backlog.

The preference is local to the browser and conversation binding. At most256
session identities are retained, without transcript text, labels or audio.
Other bindings do not inherit it. If browser storage is unavailable, the current
view still honors the preference; persistence across reload is then unavailable.
At the limit, unmute an existing session before muting another. This does not
claim cross-device synchronization or hearing acceptance. Owning issue: #339;
attention coordination: #304; full executable plan: #301.

Standalone Voice intent also survives API request deadlines (`AbortError`), as
well as network failures/server outages. A timeout must not become a human
pause. Explicit player/OS Pause, Stop, access revocation and leaving the ordinary
non-standalone view retain their existing behavior. This fixes a reproduced
timeout path; it does not identify the cause of every reported device pause.

Resuming the speaker toggle also resumes an already-loaded paused reply through
the ordinary canonical-source validation, without synthesizing it again. An
ended segment or cancelled preparation releases its queue ownership, allowing
new replies to play without a Stop/off/on cycle. Cancelled or uncertain speech
preparation is never retried automatically. Recording still takes precedence.
The speaker element retains only its last on-to-paused reason and client time
in `data-last-pause-reason`/`data-last-pause-at`, for distinguishing player/OS,
access, capability and observation transitions during device diagnosis. This
view-local diagnostic contains no transcript and creates no history collector.

## Prepared completion receiving evidence

PR348 source637836cdb08f5f18ec3e64e77073d994bba6e56c is receiving-selected
with both configured ASR helper aliases following the immutable scripts release.
Final CI40456253842 passed;28 ASR contracts cover bounded flat recovery, old
uncertainty, full-quota fallback and an older helper attempting the same actual
identity lock during a prepared commit. Focused independent review corrections
were rechecked. Existing helpers drained without interruption; the selected
completed-only alias recovered a flat completion without a provider/native call.
Native listener/collectors, model selection and Qwen TTS configuration remained
unchanged. Existing legacy directories may gain an empty identity-lock namespace;
retained legacy outcomes are preserved, rather than migrated or rewritten.

One isolated local pair reused the same retained synthetic provider preparation
and100 synthetic historical jobs in each file-ASR cache: baseline0.9767seconds
and new completion0.01862seconds, with historical reservations1→0. This supports
the reduction in local completion work; it is not a speed guarantee, a complete
conversation latency distribution or physical-device/hearing acceptance. Useful
foreground response latency, semantic heard/remaining coordination and full
stage2–4 acceptance remain unfinished under milestone9 and #301/#303/#304.

## Selected-stack response measurements — 2026-10-27

Three short Spanish questions were submitted through authenticated HTTP originals,
owner control and actual native Sol6.1/medium, then the existing source-bound Qwen
TTS adapter (`qwen-audio-3.0-tts-plus`, `longanlufeng`). The functional tag used its
retained native6eff7b4/batch speech code; current used native352/web353 with
progressive MP3 output. Each path used one new isolated conversation: one cold
turn, then two warm turns. Runtime control storage matched the receiving layout;
the earlier probe with different storage was retained and excluded from this
comparison. No production session, hardware capture or physical playback changed.

| Path / sample | Native request sent | First model text delta | Canonical reply visible | First audio payload available |
| --- | ---: | ---: | ---: | ---: |
| Functional baseline, cold | 12.85 s | 17.79 s | 18.50 s | 22.85 s |
| Functional baseline, warm1 | 1.62 s | 6.01 s | 7.08 s | 11.27 s |
| Functional baseline, warm2 | 3.24 s | 8.45 s | 9.20 s | 14.48 s |
| Current, cold | 13.32 s | 18.56 s | 19.36 s | 26.60 s |
| Current, warm1 | 1.90 s | 7.58 s | 8.65 s | 10.07 s |
| Current, warm2 | 1.57 s | 5.45 s | 6.25 s | 7.55 s |

Times start at HTTP submission. Current's first payload is an encoded MP3 frame;
the baseline exposes its complete WAV. These are not speech-end-to-intelligible
sound, WAN/browser-decoding or real-device measurements. ASR is excluded, answers
vary naturally, and sequential samples share uncontrolled provider/host load.
Three samples are not an SLO or a provider ranking. In particular, the cold
progressive sample was slower; do not claim a blanket improvement. Warm TTS's
own first-frame preparation measured1.29–1.38 s, versus4.18–5.26 s for complete
baseline audio. Source completion followed the first text delta by0.62–0.99 s.
Full continuous-voice/attention and end-to-end qualification remain #301/#303/#304.

The roughly13 s cold gap before native admission led to a concrete avoidable
step: quota-banner preparation scans up to200 recent/archived rollout files even
when canonical app/topic delivery never displays that legacy preview. A
model-free blocked-history fixture reproduced the admission dependency. Native
turn setup now fetches that quota only for an enabled, visible legacy unfinished
preview. Canonical returns, projected topics and disabled previews bypass it;
explicit limits inspection and actual legacy quota banners retain their existing
behavior. The fix removes the unnecessary dependency, without attributing every
measured cold second to it or changing model, effort, routing or journals.

## Selected Qwen word timing and robot-style direction — 2026-10-27

The current [HTTP API](https://www.alibabacloud.com/help/en/model-studio/qwen-audio-tts-http-api)
accepts `input.word_timestamp_enabled=true` for streaming synthesis. The
[voice list](https://www.alibabacloud.com/help/en/model-studio/qwen-audio-tts-voice-list)
explicitly lists instruction control and timestamps for the selected
`qwen-audio-3.0-tts-plus` / `longanlufeng`. Changing the voice is unnecessary to
investigate timed interruption context.

One distinct synthetic Spanish/English request on the actual selected account
returned MP3 audio and15 unique timed word entries in2.51seconds, without
physical playback or native dispatch. Provider events contained cumulative
snapshots: summing their array lengths counts83 entries and is incorrect.
`sentence-end` retained the complete15-entry snapshot. In this observed response,
`begin_index`/`end_index` advanced once per word, despite documentation describing
character positions; punctuation/spacing also differed (`.Podemos` versus
`. Podemos`). Never use these indices as canonical Unicode offsets. Any integration
must reconcile provider word text to the actual Markdown-projected spoken text,
retain exact source/revision/chunk binding, handle repeated snapshots and seek,
and keep unknown alignment explicit. A device cursor still does not prove hearing.
The application does not yet transport or use these word timings: current Follow
and interruption reports remain segment/media-position based. Full semantic
heard/remaining coordination remains unfinished under #304.

Under the human's robot-voice request, two new bounded synthesis trials retained
the same model and voice: a calm, measured onboard-computer direction
(rate0.92, pitch0.95), and a bright, formal protocol-robot direction
(rate1.04, pitch1.04). Both returned verified24kHz mono PCM samples. The first
is receiving-selected through the existing hot-read speech binding; no native
model or process changed. These are descriptive style directions, not verified
replicas or hearing acceptance. An alternative `qwen-audio-3.1-tts-plus` /
`xunanchuan_v3.1` trial was explicitly rejected on this account/route; no automatic
retry or fallback request was made. Documentation of a voice is not proof of
receiving account access. The functional Qwen voice remains selected.

## Optional provider-aligned playback positions

`speech.word_timestamps` (defaultfalse) requests Qwen word timing through the
existing SSE preparation. It adds no second synthesis: timed and untimed readers
share the same audio identity, lock, completed cache and uncertain outcome. Existing
completed audio without timing remains playable without regeneration. Invalid,
partial, reordered or mismatched timing leaves ordinary audio and approximate
Follow available; it does not authorize another provider request.

Only final sentence snapshots are reconciled. Alphabetic sentence-ending periods
may differ, while internal punctuation such as3.0 or foo.bar and
command operators remain exact. Provider indices are never canonical positions.
A bounded compact timing map is stored with the existing successful audio state,
without retaining another text/audio journal. Timing is delivered in the terminal
frame after renderer success and audio integrity; during ongoing generation the
player still has only segment position. Exact plaintext suffix matching separates
a spoken session introduction from the canonical body. Transformed Markdown and
other uncertain projections keep approximate Follow rather than fabricated offsets.

The existing media player's current time chooses the last completed word. Seeking
backward moves that cursor backward. A newer native presentation advertises
`can_report_word_position`; only then may a new original freeze optional
`playback.word_offset`. Old receivers receive the legacy report. The field is a
bounded device media cursor, never proof of hearing, a consumed prefix, an answer,
work cancellation or a resumption instruction. On-demand `playback_context` resolves
the exact current canonical revision and exposes that position within its bounded
source window. Preserve a reader supporting the optional field once originals
contain it; rollback must retain live journals and uncertainty.

Worker introductions now separate the session name from the body with a paragraph
break, removing the inserted dot reported in #356. Canonical text and body offsets
remain unchanged. The separate human report of punctuation spoken within paragraphs
is not attributed solely to that prefix; a retained calm-voice sample's ASR draft
did not reproduce “dot”/“punto”, which does not invalidate the intermittent report.
The exact reported app text contains ordinary sentence punctuation, not those words.

The later human report confirms that the paragraph-only mitigation did not
resolve the heading problem. Qwen routes may opt into `ssml_breaks: true` only
with a model/voice documented to support SSML. The renderer escapes all source
XML before wrapping it in `<speak>` and emits a provider `<break time="450ms"/>`
for paragraph/line boundaries. Source-authored tags stay literal text; they cannot
create pauses or external-sound controls. Trailing punctuation on the generated
session label is removed from the introduction; canonical text and offsets stay
unchanged. The actual serialized SSML is included in synthesis identity, so old
plain-text cache entries do not masquerade as the new audio.

The selected voice instruction uses positive delivery guidance rather than
mentioning punctuation names. The optional SSML path preserves the default for
other routes; voice/model/credentials remain receiving-selected. Verify provider
acceptance and listening separately from payload tests. References:
[Qwen SSML](https://docs.qwencloud.com/developer-guides/speech/ssml) and
[voice feature support](https://docs.qwencloud.com/developer-guides/speech/voice-list/qwen-audio-tts).

## Stable pre-realtime receiving checkpoint — 2026-10-27

The human selected `interaction-stable-pre-realtime-2026-10-27` before the next duplex/foreground
integration. The [component manifest](releases/interaction-stable-pre-realtime-2026-10-27.json)
pins Telecodex native/web/scripts, the loaded Codex runtime and the exact shared
auth/Apps composition, with published per-repository tags and executable hashes.
Earlier functional tags remain immutable. This checkpoint includes current streaming
ASR, progressive TTS and optional final word timing; it does not claim completion
of the continuous-voice milestone. Roll back software/configuration only, retaining
compatible readers, current journals, goals, grants and uncertain effects.

## Provisional native response text

The collector can expose `response_draft` alongside presentation metadata while
an owned integrating native item is still generating. This is separate from
canonical `items`: it has no source/revision for Listen, replies, artifacts,
question decisions or publication. Completion replaces it with the ordinary
confirmed source, which may differ. This advances useful text display, not early
spoken response or completion of the stage2/3/4 goal.

The actual native thread/physical-turn/item correlation is required. Only the
configured entry/human writes this bounded64KiB replaceable snapshot. A lifecycle
start owns the current physical producer without changing native recovery's
first-turn boundary; late deltas from a preceding physical turn are refused.
The existing store bounds it to one snapshot per entry and at most five writes
per second per item. Reads recheck ownership, native binding/generation, active
local execution and canonical-item retirement. Empty item completion retires
only that draft; terminal events and process startup retire ephemeral state.
No partial result gains canonical request coverage or Telegram publication.
The browser uses existing timeline placement and follows growing text only when
already at the bottom; older manual reading keeps its position. Observation
failure hides the provisional card. Reconnect reads current state without native
input, synthesis or recovery replay. Older clients/readers ignore the optional
field; rollback does not restore old journals or change native work.

The PR357 final aggregate40473421537 failed in the direct-topic projection
fixture waiting for a reply. That fixture observed a peer request before the
native acknowledgment, which leaves an explicit scheduling race. The fixture
now waits for the actual matching native-input receipt before publishing its
synthetic completion; the complete projection journey passes against the existing
receiving357 executable. This is verifier evidence, not a claimed production
routing repair or a reason to replay an original. Other final357 checks passed.


## Live native speech prefixes (opt-in)

`speech.live_response` defaults to false. A compatible browser can select complete
sentence/paragraph prefixes from the actual integrating response while it is being
written. The dedicated `presentation speech` reader verifies the owned current
native identity, entry generation, Unicode range and exact prefix hash. These
selectors are not canonical answer, reply, question or artifact sources. They
use the existing source-linked renderer, progressive player, explicit Voice
controls and provider-attempt journal. No new prompt, model or result journal.

Appending native text does not invalidate frozen audio. Confirmation preserves
a prefix only when the exact canonical item agrees; changed/empty/inaccessible
text stops it. The client reserves an automatic attempt once and reconciles its
prefix against confirmation before choosing the remaining canonical text. A
failed/uncertain preparation is not repeated at confirmation. Explicit Resume
retains the completed media boundary separately; that cursor is not proof of
hearing. Input capture and native-job cancellation remain distinct. The existing bounded playback report also accepts this dedicated source and
its range; read-only `playback_context` uses the same prefix verification, without
turning it into a reply/action source. Completed media ranges advance a contiguous
resume cursor, so later playback cannot erase an earlier interrupted gap. Live
completion after canonical arrival updates the same cursor. The option is not
receiving-enabled until integrated interruption/context and HTTP/native
qualification are complete.


## First live-prefix timing — 2026-10-27

Three distinct isolated text originals used actual Sol6.1/medium, the shipped
LiveSpeech selector, owned native prefix reads and the current Qwen3.0 /
longanlufeng streaming adapter. Each requested useful content without tools or
dispatch. The second question matches an earlier warm baseline question; response
lengths and current provider/host load differ, so these are observations rather
than a controlled speedup or latency distribution.

| Turn | First model delta | Frozen prefix available | First encoded audio | Native message complete | Audio lead over completion |
| --- | ---: | ---: | ---: | ---: | ---: |
| Four useful sentences, cold | 6.39s | 7.38s | 9.73s | 10.66s | 0.93s |
| One sentence, warm | 5.31s | canonical at6.36s | 7.68s | 6.27s | none; audio1.41s later |
| About150 words, warm | 6.59s | 7.79s | 9.10s | 14.28s | 5.18s |

Times start at HTTP submission. One prefix request per turn returned confirmed
encoded audio; all three native turns completed and dispatched zero work. Qwen's
own first frame took2.32s cold and1.30–1.31s warm. This verifies real synthesis
overlapping native generation on multi-sentence replies; a short single sentence
used the ordinary canonical fallback. It does not establish speech-end-to-sound,
ASR/WAN/browser decode, hearing or a subsecond target. The remaining model wait
still dominates short turns. The isolated originals/provider outcomes are retained
without replay, and production sessions/model/voice were not changed.

Input capture owns an interruption: an unchanged interrupted live item and its
matching canonical tail remain available for explicit Resume/Listen, rather than
restarting automatically when capture ends. Explicit Play reuses loaded media
and queues its matching canonical remainder once; another native question keeps
queue priority. A definitely empty/unsent selection can be released without a
playback error. Native jobs and global Voice intent remain independent.


## One-ahead live speech preparation

While a live prefix plays, the client may prepare one next useful prefix or its
matching canonical remainder. Preparation never replaces the audible media
element. The same-reply transition uses the existing180ms chunk pause; provider
and network delays can still extend it. This does not promise a measured device
gap or lower first-response latency.

A pending native question keeps priority. The prepared reply moves into the
existing queue with its single provider attempt, including failed/uncertain
outcomes, and is consumed without a second synthesis afterward. Capture holds
interrupted narration; explicit Play can reuse loaded media and its existing
prepared prefix. Changed confirmation, binding replacement and explicit Stop
release obsolete preparations. Completed-range tracking still preserves any
unsaid gap; preparation is neither playback nor evidence of hearing.


## Explicit word-aligned Resume

For canonical text with an available provider-aligned word position, an explicit
Resume after Stop/reload starts at that saved canonical word-end offset. It does
not estimate a word from media duration. Missing/unavailable/out-of-range timing
retains the existing segment boundary, and an earlier interrupted live prefix
still takes precedence over a later aligned tail. A backward seek updates the
current position rather than a played high-water mark. Loaded media continues
at its current media time without another synthesis.

The existing bounded, binding/source/revision-scoped local position record stores
only the offset, never source text. No native input or persistent schema changes.
A saved word-end cursor is device playback evidence, not proof of hearing; a
partly spoken word may repeat. Provisional live prefixes still retain completed
segment ranges until canonical confirmation; this change refines canonical
Resume and does not establish complete semantic heard/unsaid coordination.


## Capture-time question eligibility

A voice recording freezes the currently observed native question versions before
microphone permission and streaming recognition begin. The updated receiver
advertises `can_limit_capture_questions`; the client sends at most32 opaque
`capture_version` values through `X-Capture-Questions` on the finalized original.
An empty object explicitly excludes all questions from automatic interpretation
of that recording. Missing capability keeps the legacy voice contract. No extra
read or provider operation delays capture. Captions still cannot submit work.

In explicitly selected continuous avatar mode, microphone/ASR arming is passive
preparation for a finite session. The particular utterance freezes its observed
question versions on the first local acoustic marker, before confirmed speech
can acquire the floor. Later acoustic activity, captions, Send, retry and recovery
do not widen that snapshot. If the local marker is unavailable, keep the earlier
arm-time snapshot. This permits a question presented while passively listening
to be answered by the next actual utterance, while excluding questions first
observed after that utterance starts. The acoustic marker is not the speech
classifier or proof of hearing/intent, and the native intersection remains the
authoritative limit. Normal push-to-talk keeps its pre-permission snapshot.

This is an untrusted restriction, not a grant: native admission independently
captures current owned question versions and intersects that set with the frozen
limit. A supplied ID/version can never add an unrelated, unavailable or changed
question. The limit stays immutable for that original UUID through upload retry
and completed-only recovery. A later question or a later step of the same
question cannot become eligible merely because the person is still recording.
A distinct later original may answer it. Unknown/not-observed questions require
clarification or the existing explicit question controls; client timing is not
proof of hearing or human intent.

The integrated native/HTTP/WebSocket/SQLite fixture in
`tests/conversation_live_response_http_io.py` combines two active workers, early
speech, revised streaming ASR, question arrival during capture, exact original
answers, reconnection and independent returns. Its baseline reproduced a late
question reported as available for earlier speech. Providers/native model peers
are synthetic; this does not measure real hearing, ASR quality or device latency.

The optional field uses the existing input document and requires no SQLite
migration. Deploy the compatible native reader before the updated web scripts;
roll back UI/configuration while preserving that reader, current journals,
prepared ASR receipts and uncertain originals. Never restore an old database or
resend an original to change its capture-time question set.


### Partial provisional speech position

A provider-aligned word cursor from the currently loaded provisional prefix now
survives capture/pause and matching canonical confirmation. The player snapshots
its current bounded offset before handover and stores only the confirmed source,
revision and offset in the existing binding-scoped local position record. Resume
after view reload starts there; explicit loaded Play still reuses the existing
media. Missing/invalid timing retains the actual segment boundary, with no
interpolation. A backward seek replaces the current partial cursor rather than
keeping its high-water mark.

The live scheduler retains one partial media range separately from completed
ranges. It cannot advance over an earlier unsaid gap. Completion of the matching
prefix or an explicitly resumed canonical remainder joins those ranges. A new canonical mapping stays unpublished while prefix validation is pending or
offline. Changed confirmation retires the partial cursor; entry replacement and
superseded validations cannot publish it later. Completed media retains the full
segment boundary even if word alignment ends before trailing punctuation. No new
native schema, provider request, transcript storage or answer/work authority is
introduced. Position is device/provider playback evidence, never proof of hearing.
Full semantic attention remains under301/304.

The shipped player regression failed0 versus20 against the previous code and
passes with aligned/missing/invalid timing, capture-to-confirmation, backward seek
and explicit Resume after reload. The loopback stream fixture now uses NDJSON
headers for progressive live prefixes, matching the real endpoint instead of
silently taking the batch path. Scheduler checks cover earlier gaps, partial-to-
canonical completion, changed source and binding retirement. These are synthetic
digital checks, not new provider latency or physical-device acceptance.

### Reload before canonical confirmation

The standalone app also retains bounded, binding-scoped provisional selectors
and media ranges in local browser storage. It stores no provisional transcript,
audio or provider credential. Reload holds these previous attempts: it does not
automatically synthesize another prefix, infer hearing or resume native work.
Once the canonical item appears, including on the initial history page, the
existing source reader validates the complete frozen prefix before transferring
its cursor to ordinary Resume audio. Changed confirmation resets the cursor but
keeps the restored attempt held for explicit Listen. Failed observation retains
the checkpoint and does not authorize another synthesis.

Only a successful write to the existing canonical position store retires the
provisional checkpoint. A failed position write leaves the previous checkpoint
available for the next reload. Missing/blocked browser storage cannot guarantee
continuity across a closed view; ordinary in-memory controls remain available.
The shipped-player regression exercises reload before completion, canonical text
on the first page, exact word position, no automatic historical speech, and a
failed canonical write followed by recovery. Scheduler checks cover binding
isolation, invalid storage and changed/unavailable canonical sources. These are
synthetic browser-state checks, not physical playback or hearing acceptance.


### Coupled actual streaming voice-to-answer observations — 2026-10-27

Three sequential cold/warm trials now traverse one monotonic timeline through
actual Gemini streaming recognition, the shipped exact-WAV upload/prepared-ASR
adapter, selected native Sol6.1/medium and the source-revalidated Qwen streaming
speech adapter. This extends the earlier text-original trials, which excluded
recognition. No production conversation, microphone, physical playback, work
dispatch or model/provider configuration changed. Native365 and web366 are the
receiving source baseline.

The source is the retained5.76-second synthetic Spanish/English demo mentioning
`git status`. A fixed isolated session task explains the mentioned command in
one useful Spanish sentence without executing it, using tools or adding filler.
This is a synthetic statement-to-explanation task, not an ordinary human question
benchmark. Audio was paced in100ms chunks. Each explicit Send admitted one
original, using its prepared transcript without a file-ASR provider call. All
three replies explained `git status`; their returned MP3 bytes decoded to
non-silent audio offline.

| Trial | First partial from audio start | Audio end to prepared | Send to native request | Native request to first text | Speech request to first frame | Audio end to first response frame |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| cold | 0.69s | 0.49s | 4.07s | 4.44s | 3.74s | 13.88s |
| warm1 | 0.72s | 0.41s | 2.04s | 5.42s | 2.48s | 11.22s |
| warm2 | 0.69s | 0.47s | 2.29s | 4.00s | 1.80s | 9.51s |

The three audio-end-to-encoded-frame observations are9.51–13.88s, median11.22s.
The useful sentence completed before a provisional speech boundary was selected,
so these three replies used the canonical source. First text to speech-request
selection added0.86–1.12s. The Send-to-native-request interval includes transcript
commit/orchestration/client startup, not just recognition. Cold committed-ASR
preparation reported3.14s (2.69s in its helper); warm preparation reported0.38s
and0.36s (helper0.18s/0.12s). The data identifies these separate waits, but does
not by itself attribute the warm admission overhead. Retained native stage logs
below narrow the next investigation.

One verifier initially expected a new durable transcript mirror for every
capture. Identical source bytes intentionally share one immutable result. Both
completed originals, native turns and synthesis results were retained; only the
third, proven unattempted planned original continued after correcting that count.
Final totals are3 recognition streams,3 originals,3 native turn/start,0 steer,
0 work dispatches,3 completed speech attempts,3 capture-specific prepared records
and1 exact-PCM transcript mirror. No provider/native operation was replayed.

These are sequential uncontrolled observations, not a distribution or matched
voice baseline speedup. The sample's last energy above PCM magnitude200 occurs
at5.569s; using that proxy adds about0.19s to each final column. Neither waveform
energy nor audio file end proves the human's final intended word. Encoded frames
and offline non-silence do not establish browser decode/first intelligible sound,
WAN/device scheduling or hearing. No subsecond success is claimed. Current
selected-stack latency and full receiving/device acceptance remain unfinished
under301/303/304. Inspect native admission and output-boundary waits before
changing a conversational model; a different foreground model still requires
the human's explicit choice.


#### Retained native admission stages

The same isolated listener already logged native preparation stages, so no new
model request was needed to narrow the admission interval. From `proxy_start`,
proxy initialization began at162/167/163ms; thread binding began at170/173/167ms
and completed at831/1554/1805ms. The binding phase therefore took661ms cold and
1381/1638ms warm. The warm delay is concentrated in the current
`start_or_resume_thread` path (profile application, native binding request/response
and verification), rather than solely transcription or proxy initialization.
The underlying reason within that phase is not established by these logs.

A separate finite read-only check of the single idle qualification thread found
three new client initializations at99/299/99ms and thread metadata reads at55/38/
41ms. That supports neither a general slow-client diagnosis nor permission to
skip native profile/binding verification. It did not exercise resume, alter a
thread or submit a model input. Inspect the supported loaded-thread/profile
semantics before attempting to shorten the measured binding path; preserve
actual selected provider/model/effort, event ownership and current native jobs.


### Question-aware playback refresh

New automatic speech waits for the current native-question registry before
selection admission, including a selection whose asynchronous hash or prepared
slot completes during a refresh. Current questions retain queue priority.
Registry refreshes share their in-flight outcome; failed or superseded list reads
do not open the speech gate. An older cached question that is absent from the
current list and returns404 stays visibly unknown and is excluded from automatic
audio and capture context; that one unavailable source does not silence other
current replies. Fresh accepted metadata can make the question available again. Explicit listening and already active playback retain
their existing controls.

After that question observation, input history, task cards, Telegram projection
and inspector reads no longer gate starting a ready live prefix. A synthetic
shipped-player regression delays those reads and the question registry, fails
and recovers the registry, and resolves hashes on either side of question
arrival. It checks source order and no replay of prepared attempts. This is
scheduling evidence, not a measured device or provider latency improvement.

## Unconfirmed recording review and local recovery

An interrupted final upload retains the exact capture ID, audio digest, bytes and
selected context. Checking status is read-only; explicit retry uses that same
original, while a local discard does not cancel already saved server work.
The shared recording-review controls keep Check, retry, Save audio and explicit
local discard reachable, including when capture is unconfirmed. The composer
buttons remain contained at narrow widths; pending text is separate from playback.

Where IndexedDB is available, the client retains at most one unfinished recording
per origin: at most20MiB of audio plus64KiB of metadata. It is kept until a matching
server receipt or explicit local discard; an unfinished original is never evicted
to admit another. Storage operations time out after3s. If storage is unavailable
or full, the current tab keeps the bounded original and tells the person to save
an audio copy before leaving. Download offers the original local bytes.

Reload restores only a recording matching the current conversation binding. It
never captures audio, retranscribes, uploads or dispatches automatically. Matching
confirmation retires only that local original. The automatic file-ASR fallback,
uncertain server outcome and explicit decision/retry rules remain unchanged.
This is recovery for newly protected recordings, not recovery of an earlier blob
already lost by an old page reload. Browser storage eviction or clearing remains
a platform limitation; an explicit downloaded copy is independent of it.

A reported lockout on2026-10-27 overlapped a web receiver replacement and subsequent
concurrency rejections. Those observations do not identify the failed original or
prove its individual failure cause. The verified UI defect was that uncertainty
hid the capture/discard controls and a failed status read only repeated a generic
notice. Keep active-capture continuity distinct from native-session preservation
when deploying the web service. Full plan301/milestone9; streaming/recovery #271.

## Recording countdown and limit provenance

The microphone shows remaining recording time in the existing caption line,
right aligned, with a warning color for the final fifteen seconds. The timer
uses a monotonic clock and starts after microphone permission and capture setup.
It measures the existing 120-second recording bound, not model tokens or an
estimate of prompt capacity. The streaming PCM path retains its same bound.
Reaching it stops capture for explicit Send/Discard review; it never commits an
original automatically. Prepared text survives that review.

A limit-stopped original carries device-reported `capture_stop: duration_limit`
through local recovery, exact upload identity, completed-transcript recovery,
canonical transcript provenance and worker relay. The native prompt explicitly
warns that the ending may be missing and asks the integrator to wait for or
clarify continuation. This is a device report, not proof the human intended more
speech. Transcript text itself stays unchanged. Ordinary manual Send omits it.
A Send racing streaming cutoff retains the limit marker. A retry cannot add or
remove that marker from an existing original.

Limited recordings POST to `/api/interaction/voice/limited`, the same handler and
writer with a required bounded stop reason. The separate route prevents older
web receivers from silently ignoring a new header and dispatching an unmarked
fragment; they refuse it while local recovery retains the exact original. Native
receivers also reject unsupported fields. Deploy the compatible backend before
this client. Durable documents containing the new field require retaining this
compatible reader during rollback; do not select an older strict document reader
against them. Existing no-limit documents omit the field and remain unchanged.

## Shared input provider and media policy

Both Telegram voice and app recordings use the native `transcribe_committed`
preparation and the configured `interaction_asr.py` adapter. Reuse an exact
completed live transcript when capture identity, original/PCM hashes and final
receipt agree. A prerecorded Telegram note has no live-capture receipt and uses
the selected Groq `whisper-large-v3` file route. The human selects that route
because it is free on the receiving account; do not replace it with a paid live
request when streaming adds no value. The configured model label alone does not
identify the recognizer that actually handled an individual clip.

For live captions, select Gemini first. An optional explicit
`[streaming_transcription.fallback]` selects Qwen with its existing endpoint and
credential-file reference. Only these two candidates may be opened, and only
before recognition readiness has been advertised: no PCM can have been admitted
at that point. Startup readiness has an eight-second bound in addition to the
existing dial bound and capture deadline. Failed startup closes the old socket
before selecting the fallback; cancellation closes a late socket as before.
After readiness, an uncertain stream is never replayed into another paid stream.
The unchanged local recording/file-ASR recovery remains available and no partial
caption is admitted as an original. Exact completed receipts record the provider
actually used, including Qwen fallback.

Audio is transient, not project history. Incoming Telegram downloads retire only
after durable transcript/provenance commit; browser copies retire after matching
confirmation and retain unresolved originals under their existing quota. The ASR
adapter retires only hashed media in a completed preparation manifest. Generated
reply audio uses the existing bounded speech cache (default one-hour unused age,
256 MiB ceiling, minimum five-minute reader grace). Maintenance and subsequent
renders prune eligible completed audio without provider calls; text/receipts and
uncertain originals remain. These are local service policies, not deletion claims
about Telegram's remote message storage.

### Skip the current spoken reply

While a reply is preparing or playing, its existing Listen control becomes Skip,
including the visible coalesced update card. Skip cancels the remaining segments
of that message and continues the existing automatic queue in order. It does not
answer questions, modify work, disable Voice Replies, or report unsaid text as
heard. The actual word/segment cursor remains available for an explicit later
resume. Pausing retains Resume audio and the ordinary player Play control.

Live provisional and canonical parts share the same native message identity, so
a skipped tail does not return when its canonical result arrives. A bounded
128-message in-memory skip selection is cleared when the conversation binding
changes; existing persisted playback cursors remain independently bounded.

### Unconfirmed capture flush

The PCM recorder waits for its worklet's final buffer before closing an ordinary
recording. If that acknowledgment does not arrive within the existing six-second
bound, preserve only the bytes actually received and mark audio_flush_unconfirmed.
Do not represent that fragment as a confirmed complete capture, even when Send was
already clicked. Reuse Send/Discard review, retain the fragment in bounded local
recovery before showing review, and carry the immutable stop reason through the
existing limited-recording endpoint and native/worker transcription provenance.
Late worklet events cannot change the offered recording identity. Empty capture
produces no original. This reproduces a silent-cut path, not the cause of any
specific historical human recording.

Gemini push-to-talk activityStart/activityEnd agrees with the
[provider's manual-transcription contract](https://ai.google.dev/gemini-api/docs/live-api/live-transcribe#manual-vad-push-to-talk).
That contract check alone does not identify a historical missing tail.


## Page-local browser timing

Under the hood → Browser voice timing keeps at most eight recording histories
and sixteen observed response items in memory. It never stores media or text,
writes storage, sends telemetry, installs a poller or changes native work. Clear,
reload or a binding change removes these local marks. The existing capture UUID
owns permission/start, first caption, stop/Send, finalization and confirmed HTTP
receipt marks. A saved original is not native acceptance.

Presentation drafts and canonical native sources expose optional `local_turn`
metadata from the already-authorized turn. The browser joins only accepted
originals and Conversation output in that same local turn, never worker returns,
old history before Send or a guessed latest reply. When several recorded inputs
share a steered turn, it leaves response timing unassigned. This observation is
not proof that a correction was incorporated. Older receivers without the field
retain capture timing and leave the response link unavailable.

Playback starts are the first unmuted `playing` event of automatic speech; muted
source validation and deliberate playback of history are excluded. The recorded
pause duration covers the recording handler applying pause/mute, not acoustic
silence. First intelligible physical sound, noise/echo and real-device acceptance
remain separate. Do not add server durations to this browser clock or present
these diagnostics as a provider benchmark or subsecond speech guarantee.

## Receiving preparation diagnostics

An unfinished prepared-mode stream with accepted audio emits one metadata-only
`voice_preparation_closed` journal line: capture UUID, last stage, accepted byte
count, finish-received/prepared-saved flags and exception class when present.
No exception message, provider event, speech, audio or credential is logged.
Intentional discard and successful preparation do not emit failure warnings.
This uses existing bounded system-journal retention, not another capture store,
retry worker or model invocation. Locate finish/final/save failure from that
line before selecting a repair; partial captions do not establish a final receipt.
Do not replay a retained original to manufacture the missing observation.


Worker update reply validation: `native:<positive message id>` is supported for
explicit worker replies as well as contextual integrator quotes. The dashboard
validates its bounded syntax; the native presentation reader checks worker kind,
exact current revision, current binding and owned destination before admission.
Do not force `context_only` onto an explicit worker reply: that changes routing.
A rejected upload must not be mistaken for a failed Gemini transcript. Retain the
exact recording and prepared receipt while inspecting the original admission.

Speech capacity counts retained audio bytes and the maximum output reservation
for each actively locked renderer. An interrupted renderer's retained `intent`
remains uncertain and cannot be replayed, but its released job lock proves that
it can no longer consume an in-flight output reservation. Preserve its receipt;
do not delete it or increase the cache limit to recover ordinary playback.

Visible integrator progress messages use the same bounded canonical text readers
as completed answers and worker updates. A list preview is only an initial
placeholder, never the complete displayed reply. Keep both integrator kinds in
automatic expansion; retain the existing visibility, source-revision and reader
limits, and manual continuation for messages exceeding the automatic bound.
