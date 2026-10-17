# Speech rendering component

`scripts/speech_render.py` renders exact selected text to a private WAV using an
owner-configured NVIDIA Magpie endpoint. This is a provider component for #118
and AlterMundi/daimon-matrix#313. It is not yet connected to canonical result
delivery, the dashboard, a device button or playback. It does not authenticate a
human or authorize access to a conversation. Do not expose this CLI as a public
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

There is no playback lease, client acknowledgment, reconnect UI, stale-result
selection policy or speech streaming implementation here yet. Telegram delivery
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
