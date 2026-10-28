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

With `continuous` absent or false, the initial trial alternates capture, durable
input and spoken response. That fallback is **not full duplex**. The opt-in
continuous owner described below keeps capture armed during playback.
Foreground response latency, real echo/noise/device acceptance and meaningful
internal-result/question offers remain required by the full roadmap.

## Continuous capture source under construction

`StreamingVoice.openCapture(media)` separates the foreground PCM graph from an
individual ASR socket. An explicitly owned source can feed successive recorders
through `captureSource`; the previous provider may finish while the next recorder
already receives PCM. Ordinary recording still owns and closes its own graph.
The foreground owner below mounts this primitive only with explicit continuous
avatar configuration.

Worklet boundary acknowledgments flush complete PCM samples without resetting
the resampler or disconnecting the microphone. Each recorder accepts samples
only between its exact begin/end acknowledgments and translates absolute activity
offsets to its own original. Provider failure keeps the same local WAV fallback;
closing a shared graph preserves each accumulated fragment as unconfirmed rather
than pretending that its final flush succeeded. Compressed-original captions do
not accept this splitting path.

The source has no PCM history, at most three consumers, and a five-minute wall
and sample limit. The foreground coordinator remains subscribed across utterance boundaries, owns
the bounded pre-speech ring and unresolved originals, and closes the source on
exit/hidden/auth loss. The primitive alone does **not** prove continuous listening,
automatic barge-in, echo rejection or device acceptance. The default alternating
avatar trial remains available until continuous mode is explicitly selected.

## Continuous foreground owner in development

`voice-session.js` now owns one explicitly selected microphone/PCM graph and
one warm activity connection across utterances. It requests browser echo
cancellation, retains an eight-second gap ring, and creates at most two unresolved
originals. Confirmed speech onset acquires the conversational floor and invokes
the existing playback-interruption callback; simply arming the microphone does
not pause output. Native work is outside this controller's authority.

A current semantic endpoint plus recognized words can select one original for
commit. Its final PCM acknowledgment starts the next recorder while the prior
ASR finishes. The recorder takes any samples between boundaries from the gap
ring, preserving exact WAV continuity. Final ASR results can arrive out of order;
commit waits for the earliest original. An uncertain predecessor blocks later
automatic dispatch. Missing recognition, unconfirmed flush, capacity, view exit,
hidden state and binding/analysis loss retain originals for review. No provider
or native request is replayed. Retention and commit are injected from the owning
ordinary voice path, not a second input endpoint.

The avatar mounts this owner only when the existing optional turn-detection
configuration also selects `continuous: true`. That selection defaults off. The
ordinary voice writer retains/submits each exact original; there is no second
input endpoint. Its existing recovery key can contain two originals, together
at most20MiB, ordered by capture start. One matching receipt/discard removes only
that original and exposes the next. A two-item `telecodex.voice-recovery/v2`
envelope needs this reader; older clients retain it without overwriting, but
cannot expose both recordings. Use a compatible client for recovery. Ordinary
one-item storage keeps its prior shape. This is source implementation, not
receiving activation or real continuous-voice acceptance.

The authenticated `/api/interaction/voice/activity` connection loads the existing
pinned CPU worker once for one foreground session. It reuses voice origin,
credential/CSRF, actor and binding checks; monitor-only grants remain excluded.
Actor authorization is rechecked on every message/output, and the Conversation
binding every fifteen seconds and after startup. It neither transcribes nor
retains media/transcripts nor admits work. At most one analyzer per actor and
two globally, 300seconds PCM/305seconds handler lifetime, 150CPU seconds,
90turn scores, an eight-second tail and two recent boundary receipts are allowed.
Only exact emitted endpoint boundaries can retire the scored prefix; later PCM
and recurrent VAD state stay intact. Each new speech generation must establish
its own three positive frames; earlier speech cannot promote later brief noise.

The ordinary ASR pool remains bounded to two streams globally. Distinct UUIDs
from the same actor can now overlap finalization; duplicate active UUIDs are
rejected. Normal capture analysis keeps its existing15second startup bound.
Continuous analysis has one bounded30second preparation (client35seconds), before
microphone acquisition. One first readiness attempt returned unavailable after
17.1seconds; the failed receipt is retained. A later fixture initially chose the
wrong local speech-generator executable, which is a verifier error. The corrected
actual handler/worker session became ready in2.917seconds and processed two
synthetic Spanish phrases, each with a speech onset, endpoint and exact boundary
acknowledgment through that same connection. No ASR, TTS, microphone, original
admission or native model ran. These are protocol/load observations, not human
endpoint accuracy, barge-in silence, useful-response latency or phone acceptance.

Two completed ASR streams can now race when retaining their transcript receipts.
A paired-finalization test exposed contention on the existing nonblocking cache
lock. One bounded writer slot now serializes those writes; queue admission waits
at most2seconds and the client awaits at most3seconds. The slot remains held until
the actual filesystem job finishes even if the HTTP/WebSocket waiter leaves.
Another process's held cache lock remains a controlled unavailable outcome. No
recognition, receipt identity or native input is replayed to resolve contention.
The browser preserves the original and requires review on uncertainty.

The browser worklet also enforces its five-minute sample ceiling on the audio
thread; a frozen main-thread timer cannot accumulate unlimited PCM messages.
The full stages2/3/required4 roadmap and real-device/provider acceptance remain
required. Configure no continuous activation from these source fixtures alone.

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
  "continuous": true,
  "command": [
    "/selected/voice-environment/bin/python",
    "/selected/release/scripts/capture_turn.py",
    "--vad-model", "/selected/models/silero_vad_16k_op15.onnx",
    "--turn-model", "/selected/models/smart-turn-v3.2-cpu.onnx"
  ]
}
```

Omit `continuous` (or set it false) to keep the earlier alternating trial.
The new flag only selects the explicitly opened avatar mode; it never starts
capture on load or persists microphone opt-in. Remove this field before selecting
an older strict configuration parser.

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
with the capture. Its pipe input/output, startup and write waits are bounded. Analysis startup runs
beside ASR; it never gates recognition readiness, captions or final preparation.
The analyzer's startup wait is bounded to15seconds alongside ASR. A late first
join uses only the original's current eight-second tail, without discarding any
original samples. Worker offsets are translated back to absolute original byte
positions before reaching the client. Startup frames establish VAD history, but
scoring is deferred until the current tail, so an old buffered pause cannot create
an endpoint. Later backlog overflow or worker failure disables analysis while
keeping ASR and the original available.
Deploy the bridge and analysis worker from the same source revision: startup
backfill uses the worker's optional `defer` command. A mismatched older worker
fails analysis without switching or restarting the existing ASR capture.
No ML inference, model weights or raw audio are retained in the web process.
Fullscreen microphone opt-in expires after five minutes and is not persisted.
Raw original recovery and existing ASR/TTS retention remain separately owned.

One real pinned-model child qualified ready/PCM/close with a silent100ms chunk:
startup4.346s and chunk plus close365ms under concurrent host load. These are
not utterance-to-response measurements. Cold model startup per capture remains an analysis
latency limitation even with the ASR-independent startup path; do not claim the sub-second product target from inference
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


## Progressive decoder recovery — 2026-10-28

An MSE/source-buffer or media-element decode failure can use the same completed,
hash-verified MP3 as a normal Blob source. The existing stream retains at most
8MiB of verified encoded media for its active source (plus the already bounded
one-ahead preparation); no raw media is added to durable browser storage.
Corrupt/incomplete transport has no fallback. Switching decoder preserves the
media cursor, revalidates the same text/source and changes playback generation
so an earlier play promise cannot stop the replacement. Stop, capture, access
loss and explicit Pause retain ownership over late fallback completion. No new
render request, provider call or automatic uncertain retry is permitted.

This fixes an observed implementation gap: media-element errors previously had
no queue-release/recovery handler. The reported Redmi failure's individual cause
is still unconfirmed; successful Chromium decoding is not Android/hearing
acceptance. See #409 for the retained investigation.


## Cold analyzer join — 28 October 2026

A single author-held-out natural Spanish clip was streamed through the actual
Gemini preparation handler: captions and one exact-source prepared receipt
completed, but analysis became unavailable. A separate exact CPU worker startup
took9.227seconds under host load, beyond the previous8second ready deadline.
That exposes a startup boundary mismatch; the first journey did not retain an
individual worker error proving its precise cause. No native original was sent.

The revised bounded first join was exercised with the same hashed public row220
from the pinned Pipecat dataset, including a nine-second initial PCM history.
Real Silero established speech and Smart Turn emitted probability0.9854 at the
correct absolute original offset288000bytes. The worker closed and raw media
was not retained. This CPU-only follow-up made no additional ASR request. The
existing final transcript remains valid; this is not full-duplex, acoustic
endpoint acceptance or a final-speech-to-audible-response latency result.
