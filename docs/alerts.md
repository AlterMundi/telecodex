# Daimon Alerts receiving adapter

Telecodex consumes `daimon-alerts/v1` from
[AlterMundi/daimon-alerts](https://github.com/AlterMundi/daimon-alerts), pinned for
qualification to `d52c72102adc66078dd458cf870b2e1ac5727b1b`. The vendored schema
is `dashboard/contracts/daimon-alerts-v1.json`, SHA256
`284d50a5548198f2bdfb578ad14195d9d818c33ffa1031982aca4af521958be2`.
Schema origin is that repository's MIT-licensed consumer contract. Keep the pin,
schema and integration fixtures coherent when extending the protocol.

The optional receiving companion runs in the existing Sessions sidecar lifespan.
It continues while the Mini App is closed, independently of native model turns.
It neither consumes Telegram updates nor touches native goals, Stop, processing
cursors, provider selection or recovery. Daimon Alerts owns producer observation;
the companion observes its availability and performs receiving delivery only.

## Configure and qualify

Add the commented `[alerts]` section from `dashboard/config.example.toml` to the
owner's private Sessions configuration. Enable it explicitly; provide loopback
`base_url`, an owner-only regular `token_file`, the exact `receiving_scope` and
`adapter_id`. The service credential requires `status`, `read`, `acknowledge`,
and, for delivery, `deliver` and the matching adapter association. These IDs are
bindings, not identity proofs or new grants.

The existing receiving configuration supplies the control human, allowed-user
database and bot token-file reference. Qualify read-only mode first with
`deliver = false`. This private probe makes no Telegram send or native request:

```text
python -B -m dashboard.alerts --config /private/dashboard.toml
```

Set `deliver = true` after grants and destination qualify. Before claiming an
intent, the adapter checks `getMe` against the configured bot ID/username. Delivery
goes only to its currently allowed control human's private bot chat; v1 has no
verified topic/session association, so it never invents one. Credential rotation
or receiving-authority changes require requalification; old journal history is
preserved. No token, private error or endpoint enters frontend data or logs.

`interval_seconds` defaults to 15, bounded to 5–300. HTTP requests are bounded,
redirects and environmental proxies are disabled, and responses are checked
against the pinned closed schema. Incidents are paginated up to 200 per view,
with explicit truncation, and filtered again to this receiver's scope. Resolved
history is queried separately so it cannot hide active work; up to 20 resolved
items from a bounded scan are shown with explicit history truncation. Delivery
also checks the exact scope, adapter and captured projection association.

## Delivery and uncertainty

The owner-only `state_dir/alerts/delivery.sqlite3` journal binds credentials,
service, scope, adapter, bot and receiving authority. A local intent and the
service's atomic uncertain claim precede Telegram. Only a fresh successful claim
permits sending. Restart, parallel workers, read refresh and lost claim replies
never permit a second send. Telegram calls are plain text and use the existing
private-chat cadence; successful replies must identify the expected chat and a
positive message ID. API acceptance is delivery evidence, not human reading.

Persist the safe Telegram outcome before returning it to the service. Lost
receipt replies are reconciled from that journal, without another send. A crash
between sending and recording its reply stays uncertain. Preserve those records
for explicit reconciliation; no automatic replay or paid recovery is provided.
Uncertain/failed local outcomes remain visible even after the service is readable
again. Terminal service-confirmed journal entries may expire after 30 days;
unconfirmed entries are retained and a 10,000-entry admission cap fails visibly.

Keep the journal on software rollback. Before changing software, back up both the
Sessions and receiving delivery SQLite files with their online backup APIs and
verify integrity. Reuse the existing sidecar deployment/rollback procedure;
updating it does not require a native listener restart or coordinated global Stop.

## Sessions and acknowledgment

`GET /api/alerts` is separate from native `GET /api/state`, so a native failure
cannot hide incidents. Both use the existing signed Telegram launch, short-lived
cookie/Bearer session and live receiving grant. Observation is labelled available,
stale or unavailable; missing worker observation is explicitly unobserved.
The compact Operations disclosure retains literal producer text, freshness,
processing, delivery and acknowledgment separately.

`POST /api/alerts/acknowledge` also requires HTTPS Origin and CSRF, bounded closed
IDs, an explicit user click and the service's scope grant. Request IDs survive
rerenders; the service records them idempotently. Acknowledgment means neither
producer resolution nor human reading, and starts no processing. Controls are
disabled on stale/unavailable observations; ambiguous actions require inspection.

## Qualification and remaining live evidence

`tests/dashboard_alerts_io.py` runs real HTTP/SQLite/process journeys against the pinned
core: scope filtering, pagination, two receiving consumers, lost claim/receipt
replies, restart, resolution, grant/credential changes and authenticated Sessions
reads/actions with native state unavailable. Its Telegram sender is fictional.
CI checks out and builds the exact published core; missing core fails the tests.
`tests/dashboard_ui.js` exercises the shipped bootstrap, independent alert read,
literal hostile text and stable acknowledgment request IDs.

Real receiving acceptance remains separate: a fictional provider failure must
produce a real Telegram receipt and authenticated Sessions incident while work
remains preserved, including with Sessions closed. Restore access and explicitly
resume HMK; observe completion and resolution. Keep phone acceptance, another
body's skill/resource adoption and automated tests as distinct evidence in #82.
