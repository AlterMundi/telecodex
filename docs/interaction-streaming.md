# Continuous voice: streaming recognition preparation

Owner: [v0.3 plan #301](https://github.com/AlterMundi/telecodex/issues/301),
[architecture #302](https://github.com/AlterMundi/telecodex/issues/302) and
[streaming/recovery #271](https://github.com/AlterMundi/telecodex/issues/271).
The full horizon remains [the maintained roadmap](interaction-roadmap.md).

## Current implementation boundary

`dashboard.transcription.TranscriptionStream` is the first provider transport
slice. It is not yet registered as a public app endpoint, enabled in receiving
configuration or connected to the microphone. Production still uses final-file
ASR. Completing this adapter does not complete the continuous-voice milestone.

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

The app integration must retain current authenticated owner/binding checks,
selected visual/reply context, immutable audio identity and one original on Send.
It must durably store completed server-derived text before consuming it, retire
bounded raw preparation audio after verified transcript retention, and retain
ordinary file-ASR fallback. A browser-supplied caption is not a trusted shortcut
around native transcription/admission. Those integration requirements are still
unfinished, alongside output streaming, barge-in and parallel attention.

## Qualification

`python -B tests/dashboard_transcription.py` exercises partial replacement,
manual readiness, refusal of generated answers, item identity, malformed/bounded
input, explicit finish/discard and terminal disconnect without reconnect. It uses
a synthetic provider, no microphone or native work.

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
