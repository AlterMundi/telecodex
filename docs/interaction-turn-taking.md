# Conversation turn-taking: candidate CPU analyzer

Owner: [#301](https://github.com/AlterMundi/telecodex/issues/301),
[architecture #302](https://github.com/AlterMundi/telecodex/issues/302), and
[duplex/attention #304](https://github.com/AlterMundi/telecodex/issues/304).
The [full roadmap](interaction-roadmap.md) remains the delivery objective.

## Current boundary

`scripts/turn_end.py` provides optional Smart Turn v3.2 CPU scoring.
The application now has an opt-in capture-analysis bridge and an avatar-entered
fullscreen voice trial. Default/manual interaction remains unchanged. This is
source implementation, not evidence of receiving activation or full duplex.
Sol6.1 / openai / medium, Gemini prepared streaming recognition and Qwen TTS
remain selected. No new foreground model is selected.

The voice view uses the same PCM, final-flush confirmation, UUID/hash, prepared
transcript and one-original upload/recovery path. It does not inherit a worker
reply selection, selected image or attached draft files. Leaving or hiding it
keeps captured audio for explicit review; it never retries an uncertain upload.
A temporary Conversation-only audio mix also silences Beacon without overwriting
the ordinary mix. Keyboard exit and the prior avatar/draft are preserved.

Silero establishes speech before Smart Turn scores a pause. A current endpoint
and nonempty recognition can finalize only the explicitly selected trial.
Client sample offsets fence verdicts if newer acoustic activity arrives. This
energy hint rejects stale decisions; it is not the speech classifier. Detection
or stream failure returns the original to review rather than automatic admission.

The initial loop alternates capture, durable input and spoken response. It is
**not yet full duplex**: capture is not continuously armed during playback.
Foreground response latency, real echo/noise/device acceptance and meaningful
internal-result/question offers remain required by the full roadmap.

## Why combine speech activity with turn analysis

[Pipecat Smart Turn](https://github.com/pipecat-ai/smart-turn) analyzes audio
phrasing rather than using silence alone. Its documented language set includes
Spanish; the CPU model uses quantized ONNX weights and the last eight seconds of
16 kHz mono audio, left-padded for shorter turns. Their recommended architecture
pairs it with speech-activity detection such as Silero. Published speed figures
are not our host's measured latency or phone acceptance.

Selected implementation reference:
[`4786657e`](https://github.com/pipecat-ai/smart-turn/tree/4786657e242dfe77dd138699ac564ee074a2a543).
The sample `inference.py` still names a v3.1 model file; do not silently substitute
that sample filename for the selected v3.2 CPU artifact.

Selected [weights](https://huggingface.co/pipecat-ai/smart-turn-v3):

- Revision: `f766f81d3cfdf7737ac64aad813d91bbfd56bf93`.
- File: `smart-turn-v3.2-cpu.onnx`, 8,679,182 bytes.
- SHA256: `2bb026316b14a660486a75b1733cd3fbab8c2fd0314dc9af7be49f8cca967e4f`.
- The source/model declare BSD-2-Clause. Obtain the artifact separately; no
  runtime download or inference-provider entitlement is assumed.

## Measured evidence — 28 October 2026

One CPU-only shared host, one inference thread, NumPy2.5.4, ONNX Runtime1.31.1,
Transformers5.21.0 and Python3.13.5. Conditions included concurrent host load.
No microphone, paid speech inference or native conversation was used.

The first experiment deliberately used espeak Spanish, three executions of ten
fixed cases. Preparation plus inference had median178.3ms and maximum269.6ms;
model setup was88.9ms, imports plus setup2.784s, and process peak RSS143.6MiB.
These are CPU scoring times, not utterance-to-response latency.

All four intended-open synthetic phrases were classified complete at the
upstream0.5 threshold. This includes a two-second pause before a continuation.
Espeak's prosody does not establish human behavior, so these are not four
independent human errors or a population accuracy estimate. They do establish
that this synthetic set cannot qualify automatic Send. Silence also scored
complete without a speech gate, reinforcing the documented need for VAD.

A separate, frozen sample used the first up-to-six rows per endpoint label
marked `language=spa, synthetic=false` in the first500rows of the authors'
[test dataset](https://huggingface.co/datasets/pipecat-ai/smart-turn-data-v3.2-test).
Five rows met that selection. Dataset revision observed before and after:
`dfc179241fee65b0efe81ca37786f41ce26f38ec`. The threshold remained0.5;
selection was not based on scores.

| Dataset row | Author label | Completion probability | Classification | Preparation + inference |
| --- | --- | --- | --- | --- |
| 18 | Open | 0.0103 | Open | 464.3ms |
| 88 | Open | 0.0125 | Open | 212.6ms |
| 220 | Complete | 0.9042 | Complete | 170.8ms |
| 489 | Complete | 0.9830 | Complete | 227.0ms |
| 498 | Open | 0.0099 | Open | 154.2ms |

This is five matches to the authors' labels, not independent collection,
statistical validation, live conversation, Argentine-Spanish/noise/echo acceptance
or evidence of the user's final intended word. The public viewer's filtered
query returnedHTTP500; bounded row paging provided the selected source records.
No cause for that API failure was established.

Synthetic files were retired after scoring. Held-out clips were decoded in
memory and not retained; source IDs, PCM/encoded hashes, scores and dataset
revision were retained. Expiring asset URLs were removed from the selection
record. Keep one pinned candidate model and its optional dependency environment
for integration, rather than accumulating model variants or raw recordings.

## Runner and bounds

The optional dependency closure in `scripts/requirements-turn.lock` records the
Linux/Python3.13 qualification environment. It is separate from dashboard defaults.
Use the existing development environment; do not install it into a content-addressed
runtime ad hoc. The runner verifies and loads the same model bytes, limits CPU
intra/inter-op parallelism to one, and imports no optional ML package until an
operator explicitly creates the analyzer.

The file reader accepts canonical mono PCM16/16kHz WAV only, at most20MiB and
120seconds, and reads only the last eight seconds. Scoring accepts at most256kB
and requires an explicit boolean speech observation. Missing speech returns no
candidate without inference. The score cannot create, answer or cancel work.

A single analyzer rejects concurrent calls immediately without a queue. Each
call owns an ONNX `RunOptions` and a0.75s deadline (operator-bound range0.01–2s).
The deadline requests cooperative termination and rejects late results, including
late success; it is not a claim of hard OS preemption. Exceptions, invalid scores
and busy/deadline outcomes keep explicit Send. See the
[ONNX run-options contract](https://onnxruntime.ai/docs/api/python/api_summary.html).

```sh
.venv/bin/python scripts/turn_end.py \
  --model /selected/data/path/smart-turn-v3.2-cpu.onnx \
  --audio /selected/canonical-16khz.wav --speech-present
```

The CLI's speech flag asserts an already established segment for this analysis;
it is not itself a VAD or a human admission grant. The module does not infer that
silence, an ASR partial, a timeout or a high score is permission to Send.

## Required integration

1. The person explicitly selects continuous conversation. Preserve push-to-talk
   and manual Send. Limit that session and stop capture on hidden view, auth loss,
   binding change, explicit exit or its resource limit; do not persist mic opt-in.
2. Real speech-activity onset interrupts local playback while retaining the heard
   position and native work. Preserve pre-speech samples so onset detection does
   not eat the beginning of an utterance. Qualify speaker echo and noise.
3. After a VAD pause, score the current turn's bounded tail. Discard a verdict if
   speech/capture generation changes while it is running. Analyzer failure must
   leave the recording recoverable, not silently cut, send or discard it.
4. Only the explicitly selected conversational mode may use a current decision
   to finalize a turn. Preserve the existing confirmed final PCM flush, exact
   capture UUID/hash and one-original commit. Unconfirmed flush requires the
   existing fragment review; no automatic uncertain retry.
5. Foreground audio is Conversation only. Internal progress/results/questions
   remain visible and native jobs continue. Explicit Listen remains available.
6. Qualify these mechanics with delayed speech, pauses, corrections and late
   worker returns, then actual device/hearing. Keep end-of-turn delay, ASR final,
   useful native response, TTS/playback and acoustic silence separate. The
   responsive foreground/background work in#303 remains required.

## Optional receiving configuration and limits

`streaming_transcription.turn_detection` is disabled unless explicitly configured:

```json
{
  "enabled": true,
  "command": [
    "/selected/voice-environment/bin/python",
    "/selected/release/scripts/capture_turn.py",
    "--vad-model", "/selected/models/silero_vad_16k_op15.onnx",
    "--turn-model", "/selected/models/smart-turn-v3.2-cpu.onnx"
  ]
}
```

Use prepared streaming ASR mode and a separately qualified ML interpreter with
`scripts/requirements-turn.lock`; never add ML dependencies to another owning
application's web environment. There is no runtime model download. Silero's
MIT license is retained at `docs/silero-vad-LICENSE.txt`; selected upstream
revision is `066409f3d9db45a30d21dfcbf9147826ffdd7052`, model size1,289,603bytes,
SHA256 `7ed98ddbad84ccac4cd0aeb3099049280713df825c610a8ed34543318f1b2c49`.
Both models load only their verified bytes and use one CPU inference thread.

The authenticated existing capture socket explicitly requests analysis; normal
captures do not create an analysis process. At most two actor-scoped captures
are active, each120seconds, at most30turn scores, an eight-second PCM tail and
one pending PCM frame. The child has60CPU seconds/145wall seconds and closes
with the capture. Its pipe input/output, startup and write waits are bounded.
No ML inference, model weights or raw audio are retained in the web process.
Fullscreen microphone opt-in expires after five minutes and is not persisted.
Raw original recovery and existing ASR/TTS retention remain separately owned.

One real pinned-model child qualified ready/PCM/close with a silent100ms chunk:
startup4.346s and chunk plus close365ms under concurrent host load. These are
not utterance-to-response measurements. Cold startup per capture remains a
latency limitation; do not claim the sub-second product target from inference
speed or activate ordinary captures to hide that cost. The real model emitted
no speech/endpoint for silence, and the child exited0. Browser qualification at
390/1040px covered actual portrait/fullscreen modules, Escape/keyboard exit,
no overflow and draft/portrait preservation, with synthetic capture callbacks.
It did not use a microphone, provider or native model.

Configure only after the selected worker path/models and existing ASR have been
qualified together. Disable the optional config to retain ordinary Push-to-talk
and Send. After native update replies have been accepted, rollback must retain
a reader supporting those compact native references; do not reinterpret or replay
an original to make an older binary accept it.
