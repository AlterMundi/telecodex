# Scoped original-audio handoff

An explicitly selected dialogue can preserve original incoming recordings and
actual generated speech for an archive consumer. This is a producer handoff,
not an archive assembler, transcription service or speech-regeneration job.
Default operation and all unselected conversations keep their existing retention.

Configure this optional table in the existing dashboard owner-local TOML:

```toml
[media_retention]
directory = "/private/selected-dialogue/producer-originals"
human_id = 100
chat_id = 100
topic_id = 1
native_thread = "11111111-1111-4111-8111-111111111111"
since = "2026-10-29T00:00:00+00:00"
max_bytes = 268435456
```

The exact authenticated human, entry and native thread must match certified
server provenance. The timestamp fences older committed human captures; it is
not permission to retrieve other history. Native renewal does not widen the
selected dialogue. The table stays local; portable source contains no actor
bindings. Enabling it records nothing by itself and starts no process or timer.
It applies to finalized recordings through the independent conversation composer
and its native presentation speech, not Telegram audio ingress, ambient microphone
input or another program's recording path.

Incoming original bytes are handed off before native submission and before the
ordinary matching-transcript cleanup. Status/list cleanup also protects matching
sources when available. Human media's input ID joins the native input journal.
A failed protection step never deletes the source clip or converts a committed
input into permission to resend. A507 during upload leaves the client recording
available to save; older uncertain submissions must still be checked.

Speech reserves at most eight MiB before the renderer starts. Streamed bytes are
synced before being emitted; cancellation retains an explicitly incomplete prefix,
not a complete recording. If a WAV renderer wrote its exact identity-addressed
artifact before parent confirmation was interrupted, that original is retained
as unconfirmed/incomplete without another provider call. Protected audio is
independent of ordinary speech cache expiry. This proves generated/retained bytes,
never that a human heard them. Reused cached originals retain their source;
there is no automatic re-synthesis to recover missing bytes.

## Consumer contract and lifecycle

The directory is owner-private. Complete records are JSON named by a SHA256
receipt identity, with schema `telecodex.original-audio/v1`, speaker, MIME,
original source binding/ID, revision/offset when present, protection start,
`complete`, byte count, content SHA256, `reconstruction=false`, and
`playback=not_observed`. The content-addressed file named by `audio_sha256` is the
exact original. Consumers verify count/hash, retain provenance and import into
one owning archive. `complete=false` means an interrupted/unconfirmed prefix or
outcome; do not label it complete or invent duration. Actual speech reconstruction
for older missing audio is a separate archive action and must be labeled there.

`capture-*.part` plus `.reservation` are pending/uncertain writes. They consume
reserved capacity across restarts; do not mistake them for complete media or
remove them while a producer is active. A crash cannot turn a partial into an
acknowledged original. The consumer/operator may inspect and recover a known
finished prefix under its retained provenance, without replaying speech/input.

The hard receiving maximum is256MiB (including receipts/reservations), with4096
entries. No original is evicted to make room. A full or unavailable store refuses
new selected capture/speech visibly; existing ordinary text remains usable.
Conservative synthesis reservations can refuse before actual bytes fill the cap.
An identical complete human handoff is reusable at capacity. Stop only the
selected protection by removing its table and applying the existing web-only
configuration replacement at a safe voice boundary. Do not stop Codex/goals.
There is no implicit expiry: protection ends on explicit disable/dialogue close;
retained originals still require a verified consumer transfer and explicit
retirement. Backups and unfinished sources are not disposable cache.

The archive owner is responsible for its separately bounded final archive and
releasing producer copies only after verified incorporation. There is no new
background observer, public download route, authority grant or global retention
increase. Code rollback preserves the protected directory and journals; do not
restore databases, replay inputs or discard pending reservations.

## Qualification

`tests/dashboard_media_retention.py` covers exact scope, forward-only human
capture, actual renderer bytes, interruption after a generated WAV, actual
streamed bytes before emission and cancellation, cache pruning, restart retry,
capacity/lock errors, HTTP507 and retain-before-unlink. Providers are synthetic;
production installation and a new real selected recording are separate evidence.
