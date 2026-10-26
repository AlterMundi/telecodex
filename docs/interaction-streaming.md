# Continuous voice: streaming recognition preparation

Owner: [v0.3 plan #301](https://github.com/AlterMundi/telecodex/issues/301),
[architecture #302](https://github.com/AlterMundi/telecodex/issues/302) and
[streaming/recovery #271](https://github.com/AlterMundi/telecodex/issues/271).
The full horizon remains [the maintained roadmap](interaction-roadmap.md).

## Current implementation boundary

The optional app integration now connects `TranscriptionStream` to the existing
microphone through an authenticated same-origin WebSocket. Its AudioWorklet
captures mono PCM while the person speaks, shows revisable captions, and prepares
one server-derived transcript for the exact finalized WAV. It is disabled by
default and has not yet been enabled in the receiving deployment. Production
still uses final-file ASR. This input layer does not complete output streaming,
endpointing, barge-in, foreground separation or the continuous-voice milestone.

The selected experiment uses Qwen Audio3.0's **input transcription only** with
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

Only explicit Send uploads the finalized original WAV through the existing
HTTP voice endpoint, with the capture UUID chosen before recording. The duration
limit can end recognition for review without submitting work. Its headers,
source SHA, selection and uncertain-receipt recovery use the existing microphone
state machine. Discard closes preparation without recognition commit or original
admission. Socket failure or unavailable/late preparation keeps the complete local
WAV for ordinary file-ASR; there is no provider reconnect or automatic audio replay.

Completed recognition writes an immutable owner-only text receipt identified by
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
`credential_file` and `cache_root`. The selected native ASR configuration must
reference that same absolute directory as `streaming_cache_root`. Keep its existing
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
ASR results. This configuration does not resume/pause goals or cancel work.


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
The existing microphone suite verifies that live preparation and the ordinary
original upload retain one capture identity. Native ASR tests verify exact receipt
matching, durable recovery and precedence of uncertain file transcription.

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
Streaming input recognition remains receiving-disabled.


### Receiving qualification and next transport

PR316 passed CI40415564333 and independent review. Real Chromium at phone,
keyboard-sized phone and desktop dimensions exercised exact prefix validation,
0→240→880 offset ordering, one-ahead preparation, Pause/Play reuse, Stop and
composer containment with a synthetic audio element and no audio-device opens.
Native image-array support from PR300 was selected before these web assets;
current native source is d6667ba8 and web/scripts source884f884. Active native
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
