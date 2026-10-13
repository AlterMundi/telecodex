# Daimon Alerts receiving integration

Consumes the versioned contract in AlterMundi/daimon-alerts, initially published
at d52c72102adc66078dd458cf870b2e1ac5727b1b. Core service owns durable projections,
claims, acknowledgment and receipts. HMK owns processing; this receiver never
invokes recovery or native control. There is no second Telegram update consumer.

Add optional `[alerts]` to the private Sessions TOML, following the commented
configuration example: `base_url`, `token_file`, `receiving_scope`, `adapter_id`,
`interval_seconds`, `deliver`. Credentials require `status`, `read`, `acknowledge`,
`deliver` and the exact scope/adapter. They stay server-side. Use a loopback HTTP
endpoint. Actual remote routes require existing resource-access authority, not a
URL selector or shared producer credential. Do not configure another bot's token.

The receiving bot/control human and live allowed-user grant come from the existing
Provider. Bot identity is verified with getMe before delivery; configuration and
credential changes require operator requalification. The response validator uses
the pinned closed v1 schema and rejects unsupported shapes. Browser payloads are
filtered again to the exact receiving scope; grant revocation is rechecked after
I/O and immediately before each external effect.

Sessions includes source/observer freshness, processing, delivery and acknowledgment.
Its all-state operations view retains resolutions while the background receiver
focuses on open attention. Private receiving journal state identifies uncertain
local attempts. Service failure remains explicit and native Sessions remains
usable. Opening or acknowledging starts no source processing. Acknowledgment uses
existing signed launch/session, live grant, HTTPS Origin/CSRF boundaries and a
stable in-memory request ID; response loss never becomes confirmed acknowledgment.
No credential, provider exception or source text reaches the browser.

Delivery is a deterministic receiving thread owned by the supervised Sessions
sidecar, independent of model turns and Mini App visibility. It is enabled only by
`deliver = true`; the default is read-only. Core observation runs independently
of this sidecar. The receiver checks service/observer health and persists a local
attempt before a service claim. A crash, lost claim reply or repeated claim never
allows another dispatch. External outcome is persisted before posting the service
receipt. Restart can reconcile a retained receipt without another Telegram send.
A crash between external send and receipt persistence remains uncertain.
Telegram API acceptance is delivery evidence, not human reading or producer recovery.

The read-only operator probe performs no Telegram send:

```text
python -B -m dashboard.alerts --config /private/dashboard.toml
```

Keep the existing dashboard session/action SQLite and the new Alerts delivery
journal on software rollback; snapshot before migration. Restart only the Sessions
sidecar for its pinned code/config release. No global Stop or Telegram listener
redeployment is needed. Verify real Telegram/Sessions and device use separately
from mocked senders and synthetic browser fixtures.
