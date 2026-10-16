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
service warnings are independent evidence. Health exposes no traffic data or token. Disk-write failure does not
replay or block delivery; dropped records mean incomplete coverage.

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
counter measures traffic; it does not itself provide a shared send budget or
repair a bridge waiting silently for that budget. Cross-producer rate control,
priority for human commands and visible cooldown state are follow-up behavior.
