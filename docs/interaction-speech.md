# Speech rendering component

`scripts/speech_render.py` renders exact selected text to a private WAV using an
owner-configured NVIDIA Magpie endpoint. This is a provider component for #118
and AlterMundi/daimon-matrix#313. The receiving presentation API below connects it
to canonical results; device buttons and client playback remain separate work.
The helper itself does not authenticate a human or authorize access to a conversation. Do not expose this CLI as a public
endpoint or let a model choose its binding or receiving scope.

The existing text result remains canonical. A future receiving caller must
validate the human, source result, session, audience and current correction
state, then derive an opaque receiving scope. Reuse that scope on reconnect.
The scope separates caches; knowing it grants no authority. Rendering a result
must never rerun worker work or delay the canonical text response.

## Owner-local configuration

Create a private route JSON file, mode `0600`, using absolute local paths:

```json
{
  "schema": "telecodex.speech-route/v1",
  "provider": "nvidia-magpie",
  "cache_root": "/absolute/private/speech-cache",
  "env_file": "/absolute/private/provider.env",
  "voice": "Magpie-Multilingual.ES-US.Isabela",
  "language": "es-US",
  "timeout_seconds": 45
}
```

The environment file must also be an owner-only regular file. Only its
`NVIDIA_API_KEY` value is parsed; dotenv contents are never executed. The default
endpoint is the published Magpie REST synthesis endpoint. An optional endpoint
override is owner configuration; HTTPS is required except for loopback fixtures.
Redirects are disabled. No credential is accepted in CLI arguments, request JSON,
query strings or the cache identity.

Run the helper with `--binding /absolute/private/route.json` and one JSON object
on stdin:

```json
{"scope":"opaque-authorized-receiving-scope","text":"The exact selected text."}
```

The input is bounded to 32 KiB and 1,800 text characters. The output is one small
JSON object containing the private audio path, text/audio hashes, render identity,
format and cache status. It always reports `playback: "not_observed"`.
The receiving caller must enforce a total process-group deadline and bounded
stdout. `timeout_seconds` bounds individual HTTP socket operations, not total
wall time. Cancellation kills only this owned renderer; it does not cancel native
work, another device or previously published text.

## Retained outcomes

The owner-only cache key includes receiving scope, exact text, provider endpoint,
voice, language and format. A per-render lock prevents concurrent requests for
the same identity. An intent is atomically persisted and fsynced before the
external request. Complete mono 16-bit PCM WAV at 22,050 Hz is bounded to 8 MiB
and 120 seconds, validated before publication, then retained with a hash.

A completed render is checked again on reuse, including after process restart.
An interrupted intent, rejected request, uncertain transfer, corrupt artifact or
orphaned audio is retained without automatic resynthesis. A renderer crash after
provider acceptance cannot prove that repeating the request would be safe. A
future explicit recovery operation must show that uncertainty rather than
silently bypass it with a new identity. Provider error payloads and credentials
are not reflected in errors. Cache state stores text hashes; audio still carries
the spoken content and must retain the original audience's access policy.

There is no playback lease, client acknowledgment, reconnect UI or speech
streaming implementation here yet. Telegram delivery
does not establish control over Telegram's audio player. Those remain receiving
presentation work; synthesis success alone is not hearing or comprehension.

## Qualification

`python3 -B tests/speech_render.py` uses actual private files, separate helper
processes and loopback HTTP. It tests concurrent deduplication, restart reuse,
process death after intent, redirects, rejected/truncated transfers, corrupt or
orphaned artifacts and input/response bounds. All provider voices, credentials
and responses are synthetic; the suite makes zero external provider/model calls.
Actual language quality, provider account access and human device playback need
their own receiving evidence.

## Authenticated receiving presentation

The existing owner-local socket provides read-only `presentation list
DELIVERY_CURSOR NATIVE_CURSOR` and `presentation part SOURCE REVISION OFFSET`
commands. The same verbs are available through `telecodex --control CONFIG`.
They neither start a native turn nor change a delivery receipt. The configured
integrating entry, its current native binding, owning allowed human and original
return route select sources. Worker results for other return conversations are
excluded. Entry commentary is excluded; final entry answers remain distinct from
worker declarations of coverage and completion.

The list returns up to eight sources of each kind in increasing retained sequence,
metadata/previews, separate continuation cursors, a binding identity and `more`.
Continue while `more`; reset both cursors if the binding changes. These cursors
advance through retained sources, not through acknowledgments or changes to old
sources. Reload earlier metadata when reviewing history. A part is at most 1,600
Unicode characters; follow `next_offset` to reconstruct the complete canonical
text without truncation. Sources carry the full text hash, audience/entry binding,
revision and relevance flags. An integrator answer does not declare a worker
request complete. A later amendment can make an unchanged worker text pending;
its revision changes without rewriting that text.

The existing signed Telegram dashboard session exposes:

- `GET /api/interaction?delivery=0&native=0` for the source list.
- `GET /api/interaction/part?source=...&revision=...&offset=0` for exact text.
- `POST /api/interaction/audio` with exactly `source`, `revision` and `offset`
  for private WAV. The existing CSRF header and exact request origin are required.

No browser-authored text, command, provider, path, human or destination is accepted
by the audio endpoint. Add optional receiving dashboard configuration to enable
synthesis:

```toml
[speech]
binding = "/absolute/private/speech-route.json"
timeout_seconds = 60
```

The listener must load the presentation commands before the dashboard can use
them. The dashboard's authority binding includes the selected interaction profile;
changes require requalification rather than silently adopting a different entry.
It checks the source before synthesis and again after synthesis, then rechecks the
live human grant immediately before returning audio. A changed source returns
409; revoked access returns no private audio. Complete generated audio remains
retained for an authorized later selection. Original text and native work are
independent of this request, its connection and its deadline.

Two owned renderers may run at once. Each has an overall deadline, bounded stdout
and process-group cleanup; child error output is discarded. The receiving caller
verifies the private artifact path, text/audio hashes, PCM format and route before
responding. Reconnection uses the same scope derived from entry/source/part and
therefore reuses a completed render. Response headers identify source, revision,
offset, next offset, audio hash and `not_observed` playback. Receiving these bytes
is not a playback, hearing, comprehension or worker-incorporation receipt.

The first spoken part of a worker result includes its controller-certified
session label, so an audio-only listener can distinguish concurrent results.
Later parts retain only their selected text. Canonical text, hashes and character
offsets remain unchanged; the renderer hashes the exact labeled spoken text in
its existing cache. Source revalidation includes the label, so changing it during
synthesis cannot return audio attributed to the old session.

`tests/presentation_io.py` checks the HTTP/auth boundary through an actual helper
process and private cache against a synthetic loopback speech provider, including
reconnect reuse and corrections/revocation during a held synthesis. Rust store
tests check canonical text reassembly, SQLite reopen, entry final/progress
separation and original owner/native/correction fences. These tests make no
external provider or native model calls and do not qualify a real Telegram device.

## Conversation reader and audio

The existing authenticated Sessions app has a Conversation view, also selected by
`?view=conversation` or the Telegram launch parameter `conversation`. These select
the view only; the same validated identity and live owner grant remain required.
The view reads canonical integrating replies separately from attributed worker
results, shows correction/coverage warnings and opens complete text in bounded
parts. It initially loads recent replies, retains older history on request and
reads new replies with independent forward cursors. Recent metadata refreshes do
not skip a backlog that is still being read forward. It adds no native work.

When the receiving speech binding is configured, Listen requests only the selected
source revision and part. The browser verifies the returned source, offset and
audio digest, then checks the source again before unmuting playback. Explicit
Play remains available when the browser requires another user gesture. Pause,
Stop audio, loss of connectivity and leaving the view affect playback only.
They never send native Stop, interrupt a worker or discard its original reply.
Hidden/deactivated views pause playback; return/reconnect refreshes sources without
automatically resuming audio. A stopped or replaced selection owns a generation
fence so late synthesis and stale source checks cannot control a newer selection.
The next part is requested only after the selected audio actually ends and is
validated again before playback. A changed source requires a new selection.

Complete audio remains in the private server cache for an authorized reconnect;
browser audio URLs are temporary and released on Stop/page exit. No browser
playback state is a durable hearing receipt. The same view supports original
[typed input](interaction-input.md) and finalized microphone clips through the
existing input journal; playback never creates either input on its own.

Pending native questions also support **Listen to question** when speech is
configured. The dashboard resolves `request:ID` through the existing owned
request-detail command, selecting its exact current revision. It reads the full
question and option descriptions in parts of at most 1,500 Unicode characters,
with a bounded session label on the first part. Approval requests and closed or
changed questions are excluded. This adapter reuses the same authenticated part,
synthesis, cache and playback paths; it does not add a native presentation verb.

After the final audio part ends, the browser checks that question again and
selects its exact reply reference for the existing composer and Talk button.
Stop, hidden views, question advancement and a later human focus selection prevent
that completion from replacing the current reply focus. Listening and selecting
focus submit no answer. A subsequent original text or finalized voice input uses
the existing native-question answer path; serving audio is no evidence that the
human heard or understood it. Browsers may still require an explicit Play gesture.

`node tests/conversation_ui.js` runs the shipped state machine against synthetic
DOM/network, including more than forty new replies, arrivals during metadata
refresh and a stale validation that fails after a newer selection. Browser
qualification uses an owned Chromium context with fictional identity, synthetic
loopback audio and muted speakers. It establishes rendering and media lifecycle
behavior, not physical audibility or real Telegram Android/iOS acceptance.

## Ephemeral audio retention

The text result is durable; synthesized WAV files are disposable. The owner route
accepts `audio_retention_seconds` (default3600, minimum300) and
`audio_cache_max_bytes` (default256MiB, maximum1GiB). Rendering performs maintenance
without calling the provider. The same cleanup can be explicitly run with:

```sh
python3 scripts/speech_render.py --binding /private/route.json --prune
```

Only verified completed audio is eligible. A five-minute grace and the existing
per-render lock protect immediate consumers. Oldest completed payloads are
removed when idle expiry or the byte budget requires it; small source hashes and
outcome state remain. An interrupted, uncertain, corrupt or orphaned render is
not treated as disposable successful audio. New provider requests reserve their
maximum possible payload before transmission; when protected audio/intents fill
the budget, speech fails without another provider call and canonical text remains
available.

A later authenticated playback request may synthesize a previously completed,
explicitly expired payload again. Cleanup never does that itself, never repeats
ASR or worker work, and never converts an uncertain synthesis into a new attempt.
This policy governs the speech cache; original capture/transcription media has its
own lifecycle and must not be deleted through this cache command.


### Uploaded capture disposal

The native receiver retires an uploaded `.voice/<capture UUID>` payload as soon
as it commits a nonempty transcript with the exact capture ID and audio hash;
closing the client does not defer this cleanup. Dashboard input status/history
reads and completed duplicate submissions finish interrupted cleanup.
Transcript text, capture identity, provenance and
routing remain in the native store. A delivery still pending after ASR does not
need the original audio. Cleanup errors never change the original input outcome
or retry transcription. Pending/unconfirmed ASR, mismatched bytes, links and
nonprivate files are retained. This applies to dashboard uploads only; companion
recordings, Telegram downloads and transcription preparation copies have separate
owners and are not recursively removed by this mechanism.
