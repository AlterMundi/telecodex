# Private Bot API traffic metering

The metering endpoint streams requests to the existing local Bot API server. It
records a single HTTP attempt, including failures and uncertain disconnects;
it does not retry requests, consume updates, start model work or implement a
second Telegram client. All existing clients must use this endpoint for their
traffic to be covered. Direct cloud API calls and internal TDLib/MTProto traffic
are outside this capture.

The journal records timestamp, producer service/PID where observable, recognized
Bot API method, numeric chat/topic/message routing, body byte counts, latency,
HTTP/API status, `retry_after`, and incomplete/uncertain outcomes. It does not
record tokens, URLs, request/response bodies, message text, filenames, headers,
credentials or command lines. Only the configured bot is metered; other bots
continue through the endpoint without being journaled. Unknown producers/topics
and oversized, multipart or compressed metadata stay explicit. Request and
response metadata completeness are recorded separately. Numeric routing remains
private receiving data.

A received `getUpdates` message is an ingress count, not proof of native
submission, a completed assistant response or human reading. HTTP success and
Bot API error status remain separate. In-flight requests interrupted by collector
restart become `collector_restarted_unknown`; they are not silently replayed.

## Receiving setup

Use the existing pinned `dashboard/requirements.lock` environment. The service
binds loopback only and forwards to one fixed loopback upstream, with redirects,
environment proxies, access logs and HTTP retries disabled. Keep the config and
state directory owner-private. Example values below are placeholders:

```toml
host = "127.0.0.1"
port = 8081
upstream = "http://127.0.0.1:8082"
token_file = "/private/receiving/bot-token"
state_dir = "/private/receiving/bot-traffic"
retention_days = 7
max_events = 250000
rate_control = true # default; false retains observation-only forwarding
activity_receipts = "/private/receiving/activity/messages.json" # optional routing seed

[sources]
bridge = "receiving-telegram.service"
activity = "receiving-telegram-activity.service"
alerts_dashboard = "receiving-dashboard.service"
```

Producer attribution uses Linux loopback socket ownership and explicit service
cgroups, not model inference or message contents. Non-Linux/unmatched callers
are `unknown`. Existing accepted activity-message receipts can seed edit/delete
routing; the seed imports only numeric receipt metadata. No old traffic is
manufactured from that seed.

```text
python scripts/bot_traffic.py serve --config /private/receiving/meter.toml
python scripts/bot_traffic.py report --state-dir /private/receiving/bot-traffic --hours 24
```

The local JSON report includes source/method and chat/topic totals, per-minute
rates and peaks, ingress counts, failures, body volume and the latest observed
cooldown deadline. It is private; do not publish raw routing IDs. Seven days and
250,000 attempts and routing receipts are retained by default, pruned on startup,
admission and completion without a timer. Age expiry is enforced when the service
is next started or handles traffic.
SQLite reuses deleted space; these limits bound retained rows, not an exact
filesystem quota. The loopback `/healthz` reports collector health and dropped
journal writes; the local report includes the last persisted count. A final loss
count can itself be lost if storage stays unwritable through process termination;
service warnings are independent evidence. Health exposes no traffic data or token.
Observation-only journal loss never replays a request; dropped records mean
incomplete coverage. With rate control enabled, unavailable admission storage
explicitly refuses forwarding rather than forgetting a cooldown.

## Adoption and rollback

Qualify the forwarding seam with `tests/bot_traffic_io.py` before adoption. Reuse
the receiving server's existing data, files, identity and credentials. Change only
its local HTTP port, then place the meter on the original client-facing port.
This briefly reconnects Bot API HTTP clients; it does not require a native Codex
or Telegram listener restart, a coordinated Stop, migration, logout or new bot.
Do not start another `getUpdates` consumer for validation. Retain exact old
launcher bytes/config and source hashes. Observe normal existing polling through
the meter; a harmless `getMe` can verify forwarding without sending a message.

Rollback stops only the meter and restores the original Bot API listening port
using the preserved launcher. Preserve traffic journals and receiving input
journals/offsets. Never roll back message stores or replay uncertain requests.

## Investigating a flood limit

Capture actual `429`/`retry_after` once and respect its deadline. Repeated probes,
listener restarts and token/config replacement are not evidence that Telegram's
cooldown ended. Use the journal to separate ordinary responses, edits, typing,
automatic cards, cleanup, alerts and polling, including concurrent producers.
The request that first encountered a limit is not necessarily its cause. A
counter measures traffic; it does not establish which call caused a limit.

## Shared admission and optional traffic

The default admission controller uses the configured bot's private SQLite journal
to retain observed server cooldowns across producers, card replacement and meter
restart. A genuine message-write 429 conservatively blocks all message-write
methods for that bot until the reported deadline plus one second. Other controls
retain method-specific cooldowns. Polling and file downloads remain available.
The controller never retries a transmitted request. An admission rejection is
explicitly marked `telecodex_local: true` and `deferred_local`: the request was
not forwarded. These events are counted separately from Telegram rejections and
cannot extend a server cooldown or create a false Telegram incident.

Automatic cards declare `X-Telecodex-Priority: background`. The endpoint strips
this local scheduling hint before forwarding; it is not an authorization grant.
Optional writes respect a shared one-second private-chat / 3.5-second group-chat
quiet period and 40ms global spacing. Foreground clients retain their existing
pacing; ordinary contention never rejects or queues a human reply behind cards.
Foreground writes reserve that same quiet period against subsequent optional
work. This does not claim a strict cross-process foreground queue or Telegram's
undocumented exact quotas. Explicit `/status` details are foreground requests.
Duplicate typing requests are coalesced for four seconds per chat/topic without
spending the reply budget; typing success does not establish reply recovery.

The activity companion persists only a digest of the last accepted text and
keyboard. Unchanged cards cause no API edit, including after restart. Changed
text/buttons and relocation still publish. An unchanged rejected 400/403 payload
is not repeated every cycle; a confirmed `message is not modified` establishes
the desired existing presentation without inventing a new message receipt.
Cleanup respects retry deadlines and does not try an edit immediately after a
rate-limited delete. Ambiguous sends remain non-replayable.

The native final-output sink still needs durable known-unsent recovery after a
genuine 429; this controller does not supply an output outbox or replay native
turns. That separate remaining scope is tracked in #103. Preserve completed
native history and uncertain delivery evidence. Expiry alone is not recovery.

Admission records contain deadlines and numeric routing keys, never message/file
content. Streamed uploads retain their complete bytes after bounded prefix
inspection. Admission-storage failures refuse forwarding explicitly instead of
silently forgetting the shared cooldown. Set `rate_control=false` only for a
deliberate observation-only deployment; keep all existing journals on rollback.

## Sessions and actionable alerts

Enable the optional `[traffic]` section in the receiving Sessions config and point
`meter_config` at this bot's existing private configuration. The meter now pins a
bot fingerprint in its journal; a different bot refuses reuse. When adopting a
pre-fingerprint journal, verify its existing bot/configuration and retained
receiving receipt first, then update only the meter. Retain all original data.
Sessions checks the same endpoint/token binding and reads SQLite in read-only
mode. Its authenticated `/api/traffic` works independently of native Codex state.
It projects 24-hour totals, a last-hour minute chart, service/method counts and the
receiving human's conversation titles (top 20 methods / 12 conversations). No raw
chat/topic identifiers or unrelated titles enter the projection. Unknown origins,
unattributed topics, incomplete metadata and dropped writes remain visible.

The deterministic Sessions companion runs only when explicitly enabled. With
`[traffic.alerts]` it uses its own scoped Daimon Alerts credential with `publish`,
`observe`, `status` and exact producer/observer/receiving-scope associations.
The existing Sessions receiver retains its own read/delivery/acknowledgment
credential. Reading the page neither publishes nor acknowledges an incident.
No model turns, new Telegram update consumer or native control is involved.

Actionable conditions are:

- An observed HTTP/API 429, retained until each affected method succeeds after
  its last rejection. A successful typing call cannot resolve a rejected reply;
  expiry of `retry_after` alone means awaiting recovery, not recovered.
- Three upstream/HTTP 5xx failures for one method within five minutes, retained
  until a later successful response for that method. Uncertain calls are not retried.
- Two consecutive collector observations failing (default about 20 seconds),
  resolving when the collector/journal become observable again.
- Newly dropped journal writes, resolving after 60 seconds without a new loss
  while capture is healthy. The historical coverage gap remains visible.

Raw volume alone has no alert threshold: method-specific Telegram quotas cannot
be inferred from aggregate HTTP counts. The independent Alerts observer-freshness
view exposes a stopped/stale Sessions observer; a running core is not proof the
companion is healthy. Missing first observation stays unobserved.

Incident transitions persist before HTTP, with stable IDs, increasing revisions
and ordered exact replay after lost acceptance. Recurrence gets a new incident
ID. The local pending transition outbox is bounded at 16; if exceeded it retains
pending work, exposes a permanent history-incomplete flag and reconciles retained
transitions before resuming current observation. Raw traffic remains independently
retained by the meter. Acknowledgment does not imply producer recovery.

The existing receiving delivery adapter defers before claiming a notification
while a known sendMessage retry deadline is active, or capture is not healthy,
fresh and initialized. Incidents remain visible in Sessions throughout. After
the deadline it may make the original pending delivery attempt; it never repeats
a claimed send. Service acceptance, Telegram API delivery and human reading remain
separate. An already failed/uncertain delivery does not become retryable.

Enable this only for the configured receiving body. Preserve exact prior config,
core actor grants and sidecar release. A producer grant addition needs only an
Alerts-core reload and the Sessions sidecar update; no listener restart or global
Stop. Roll back software/config while preserving incident/outbox, meter, input
and delivery journals. Stop/retire an intentionally removed observer through the
Alerts operator contract; never disguise a stopped observer as healthy.
