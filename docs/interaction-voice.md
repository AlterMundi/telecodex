# Committed voice input

Voice preparation is opt-in on the selected human's integrating entry. A finalized
Telegram voice clip is transcribed before entering a native turn or selecting a
worker. Text controls and other topics continue while ASR runs. This is the first
input adapter for #118. The [canonical Mini App microphone](interaction-input.md)
uses the same transcriber with a one-button recording flow, while the separate
[speech player](interaction-speech.md) renders selected retained replies. Accessible
realtime comparison and receiving device/human acceptance remain separate work.

```toml
[voice]
transcription_command = ["/absolute/python3", "/absolute/transcribe-file.py"]
transcriber_label = "selected-provider/model"
timeout_seconds = 60
```

The existing `[interaction]` profile must be selected. The transcriber is trusted
owner configuration, never model-authored argv. It receives the exact local audio
path as its final argument and must exit successfully with bounded JSON containing
`{"text":"the completed transcript"}`. An explicitly partial (`final:false` or `partial:true`) result is rejected.
Additional provider metadata is ignored.
A file-transcription adapter emits one final result, not streaming ASR fragments.
Use environment/private-file credentials, never tokens in arguments. The existing
configured `audio-transcription` foreground helper can satisfy this contract;
its route, credentials and response cache remain receiving-local. Label the
configured adapter accurately; its label is not verified provider attestation.

FFmpeg must be available on PATH. Preparation accepts one voice clip without a
caption or mixed media, up to 20 MiB (enforced during transfer) and 120 seconds decoded mono audio. It rejects
digital silence before invoking ASR. This is a digital-silence check, not proof
that noise is speech or that words were heard correctly. The external command has
a 1–120 second deadline and 64 KiB output bound. At most two preparations run;
excess inputs remain retained without automatic transcription/dispatch.

The bridge owns each preparation process group; timeout, failure or cancellation
cleans that group only. Explicit listener restart cancels and drains preparations
before its hard process exit. This never interrupts a native worker or controls another
device. Preparing input does not block the entry dispatcher. Native work starts
only after the immutable transcript commit and the existing routing/admission
checks. Audio bypasses the general slash-command dispatcher; existing routing,
status and explicitly scoped controls retain their existing admission path. A
voice-scoped interruption selects only the owned execution captured when the clip
arrived; it cannot cancel a newer execution or withdraw its dependencies.

Original ingress JSON and the retained audio stay separate from machine-derived
text. The canonical source includes the media hash, original file reference,
configured adapter label, commit timestamp, measured clip duration and preparation
wall times. Routed packets and the integrator's original-input lookup identify
that transcription explicitly; it is not fabricated written human speech.
An automatic transcript excerpt is shown in the originating conversation. Exact
native admission and worker incorporation remain separate evidence.

Later text/voice corrections retain their original ingress order even if ASR
finishes late. A changed owner, entry native binding or Stop epoch prevents late
preparation from admitting work. Queue, steer and native-submit boundaries recheck
that authority after asynchronous waits; a first voice may atomically establish
its own new native binding and carry other pending clips from that same entry
generation. Human clear/rebind still retires them. Original voice records survive live-session deletion
without becoming eligible for a replacement session. A restart retains interrupted preparation and
uncertain ingress without rerunning transcription or replaying a native prompt.
Playback interruption and selected worker cancellation remain separate concerns;
this input adapter does not claim to stop Telegram client audio playback.

Qualification uses real SQLite transactions, actual bounded subprocesses and
FFmpeg decoding with synthetic HTTP/native peers. Generated tones/silence and
scripted ASR are fixtures, not speech-recognition or live Telegram evidence. Actual
Spanish/noise/hearing, accessible realtime comparison, TTS/playback/reconnect and
one-button acceptance remain #118 and the linked Integrators tasks. The entire
parent remains AlterMundi/daimon-matrix#313.

The separate [speech rendering component](interaction-speech.md) provides a
durable exact-text provider adapter. It is not yet wired to result playback.
