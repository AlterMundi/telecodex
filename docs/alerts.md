# Daimon Alerts receiving adapter

Telecodex consumes `daimon-alerts/v1` from
[AlterMundi/daimon-alerts](https://github.com/AlterMundi/daimon-alerts), pinned for
qualification to `8a7d66977d2efdfc954f8a7fad2d499e01da6973`. The vendored schema
is `dashboard/contracts/daimon-alerts-v1.json`, SHA256
`d1a2f3773cfcbbbaccc98e9490647b0e35f0ad4233b235bce223058d06b58602`.
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

The disclosure uses an amber warning icon and explicit unacknowledged count,
including displayed resolved notices that have not been acknowledged. Open
incidents stay visible with an Open badge, a separate acknowledgment badge and
their delivery state. Resolved incidents move into a history disclosure closed
by default. Acknowledgment removes attention for that notice immediately after
confirmed acceptance, but never moves an open incident into resolved history.
Refreshing reads the service's durable acknowledgment state.

History is a bounded receiving view, not deletion. The service owns retention;
the installed core's 30-day policy only prunes eligible old resolved incidents
without pending/uncertain delivery on new-incident admission. Open work and
uncertain receipts stay retained. There is no browser clear-all operation.

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


## Apps menu attention

Explicitly enabling `alerts.menu_attention = true` projects this receiving human's
existing authorized alert snapshot into the **existing private-chat Apps menu**.
The menu URL and original label are retained privately for rollback. The receiver
refuses another app selection, a changed destination, different human/bot binding
or an externally changed label. It neither registers an app nor consumes updates.

Telegram's `MenuButtonWebApp` exposes text and a URL, without an icon color or
animation field: https://core.telegram.org/bots/api#menubuttonwebapp . The supported
indicator is therefore a colored emoji in the label: red for error/critical, orange
for warning, blue for information, plain Apps when clear. Only open unacknowledged
incidents request attention. Acknowledgment does not resolve their conditions.
Unavailable/stale/unobserved or truncated observation displays a white unknown
indicator unless a known error/critical already warrants red. Resolved history
does not request attention. Rendering and refresh timing depend on Telegram clients.

The existing Alerts receiving loop owns this projection. Updates happen only on
attention transitions, at most once per minute, and respect the receiving traffic
cooldown. Menu errors are recorded independently and cannot break delivery. A lost
setter reply is reconciled by reading the current value; there is no animated
API polling or new notification consumer. The private `alerts/menu-attention.json`
retains original/selected values and observed time, distinct from human observation.
To roll back, disable the option and restore that record's original menu through
the existing operator authority after verifying the current binding and destination.

## Follow attention from Apps to the incident

The same `attention_projection` drives the Telegram menu, launcher card/banner
and Sessions summary. Only open unacknowledged incidents contribute. The compact
projection includes a count, at most three safe producer summaries and matching
severity symbols. Resolved history and acknowledgment remain separate from recovery.

An explicit `alerts.launcher_origins` list enables **only** the read-only
`POST /api/alerts/attention` route for selected exact HTTPS origins. It validates
Telegram's current signed launch for this receiving bot and rechecks the existing
human grant before and after reading the snapshot. It issues no session/control
credential, cookie or write capability. Other Sessions routes retain their original
Origin/authentication requirements. Proxies must expose only this exact POST route,
limit bodies, preserve Origin and prevent ambient credential forwarding. No private
attention enters the public app catalog, browser storage, URLs or logs.

Signed launches retain the existing five-minute validity budget. An expired launch,
revoked grant, unavailable route or stale observation cannot certify clear state:
the launcher removes summaries and shows unknown, inviting the user to reopen or
check Sessions. Non-Telegram anonymous browsing carries no private alert summary.
The current receiving adoption is CompAII-only; another bot needs its own configured
route and signature/grant verification, not reuse of this body's credentials.

## Incident dates in Sessions and Interaction

Each incident shows its original **Issued** date (`projection.source_at`) and,
when different, its latest **Updated** date (`projection.updated_at`). Dates use
the viewer's local timezone, with an ISO timestamp on the time element. Polling
and receiving timestamps never replace the issue date; missing or invalid source
dates are explicitly unavailable. Dates appear on both active and resolved cards.

Old open incidents do not expire merely because of age. A stale observation is
not proof of recovery. Reconcile the owning producer or its explicit retirement
after verified successor coverage; preserve receipts and original provenance.
Resolved incidents remain in the bounded, initially collapsed history and do not
contribute to active attention. This display change performs no acknowledgment,
resolution, deletion or replay.
