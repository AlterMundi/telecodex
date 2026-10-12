# Sessions Mini App

The receiving bot's owner can inspect native conversations and host resources
and request the same coordinated Stop used by `/stop all`. This separate Python
service does not restart the listener, consume Telegram updates, start inference,
read Matrix, replace Nexus, or change its Main Mini App/table/topic bindings.

## Install and launch

Use an exact qualified source, Python 3.11+ on Linux, the receiving native home
and config, and the existing `session_control` grant. Install the pinned Python
dependencies in a selected environment. `dashboard/requirements.lock` reuses the
inspected Nexus service versions; no Nexus code or frontend is imported.

```text
python -m pip install -r dashboard/requirements.lock
python -B tests/dashboard_io.py
TELECODEX_DASHBOARD_CONFIG=/private/dashboard.toml uvicorn dashboard.app:create_app --factory --host 127.0.0.1 --port 8794 --no-access-log
```

Run as the receiving owner with umask 0077, one worker and a bounded HTTP
concurrency limit. Reverse-proxy the configured HTTPS prefix to this loopback
service, stripping the prefix. Preserve other host routes. Only `web/` shell
assets are public; never mount config/state/source/native directories. Select
the observed `getMe` bot ID and username. The service needs no plaintext bot
token: it validates untouched Telegram `initData` against the maintained
production Ed25519 key using `init-data-py==1.0.0`, with five-minute freshness,
30-second future skew, duplicate rejection and an independently checked user.

There is no invitation/first-enrollment workaround: the account must be exactly
the receiver's configured control human and currently allowed in its existing
database. Receiver configuration drift denies private reads and Stop until an
operator requalifies/restarts the dashboard. Cookies expire after one hour and
are bound to bot/config identity. All writes require the exact HTTPS Origin;
Stop additionally requires a per-session action token. Anonymous browser visits
receive only a login shell, never session titles, goals or host metrics.

The menu is a private-chat menu selected for this human, not the attachment menu
or a second Main App registration. Existing slash commands remain available
through their normal completion UI; profile/Nexus entrypoints remain unchanged.

```text
python -m dashboard.operator --config /private/dashboard.toml inspect-menu
python -m dashboard.operator --config /private/dashboard.toml publish-menu
python -m dashboard.operator --config /private/dashboard.toml restore-menu
```

The operator reuses the existing safe receiving token-file reader, snapshots the
previous menu once, sets **Sessions**, and requires a read-back receipt. It does
not register a webhook or poll updates. Menu configuration success is not a real
Telegram WebView authentication/phone acceptance receipt. Keep those separate.

## What the view means

- Owned sessions only, active first; paused/blocked goals remain discoverable.
  Goals show native state and a literal 180-character objective preview.
  Supergroup links use the established topic/message syntax. Private bot links
  open the bot chat, where the human selects the named topic: a cross-client
  private-topic deep link has not been qualified and is not fabricated.
- Existing observer RPC/rollout helpers supply activity, elapsed time and tool
  counts. Metadata reads use `includeTurns:false`; no full native history is
  transported to the browser. Missing/unsupported data remains unavailable.
- The rooted tree uses native index parent metadata and the existing ancestry
  validator. It includes up to 32 nonarchived descendants and eight levels;
  incomplete enumeration is labeled partial. Parent queue admission does not
  prove child recovery. The view does not claim host subprocess supervision.
- CPU and network rates use counter deltas; first samples show sampling.
  Memory uses available RAM, including reclaimable cache. Storage shows space
  available to the service user at explicitly selected mounts. Network totals
  aggregate non-loopback interfaces and may double-count VPN/virtual traffic;
  they do not establish ISP usage or link health. No addresses/interface names
  or host configuration appear. Observation times use Argentina time.
- Refresh is bounded and cached for eight seconds. The client requests every ten
  seconds while visible and stops requesting when suspended. Stale/error states
  disable the Stop control. Native connection failures never become idle.

## Stop and recovery

The button describes the entire receiving owner's scope; session filters do not
alter it. Confirming journals a unique action before connect-only native control
and uses that same nonce in the existing durable Stop coordinator. Concurrent
retries or service restart do not redispatch uncertain effects. Lost acceptance
is unconfirmed, not stopped. The live view reads the coordinator's persisted
phase/target receipts to distinguish requested, observed and failed/unconfirmed.
Resume/restart/release stay on the existing `/stop` command surface for this MVP.

Preserve the dashboard SQLite action/session journal on code rollback. Take a
verified snapshot before future migrations; this first version changes no
Telecodex/native schema. Restore the previous Telegram menu and remove only the
added proxy/service route when rolling back this surface. The listener, original
input journal, polling offsets, native bindings, goals and Stop operations stay
intact. Do not run a real general Stop merely to qualify the UI.

Owning scope: [#70](https://github.com/AlterMundi/telecodex/issues/70); richer
activity streams remain [#20](https://github.com/AlterMundi/telecodex/issues/20).
Telegram launch constraints: [Mini Apps](https://core.telegram.org/bots/webapps),
[private-chat menu](https://core.telegram.org/bots/api#setchatmenubutton).
