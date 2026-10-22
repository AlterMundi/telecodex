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
database. Receiving authority drift denies private reads and control until an
operator requalifies/restarts the dashboard. The pinned authority covers the
database, control actor/socket, native executable and Telegram credential/endpoint
references. Unrelated delivery, language, project or timing settings and TOML
formatting can change without revoking the dashboard grant. Current allowed-user
state remains checked for every request. Sessions expire after one hour and
are bound to bot/config identity. The verified launch exchange returns an opaque
session token held only in JavaScript memory and sent in an Authorization Bearer
header; this allows WebViews that omit the secure session cookie to authenticate.
No browser storage, URL, Telegram message or log contains it. The existing
HttpOnly/Secure/SameSiteStrict cookie remains supported for compatibility.
An invalid explicit header fails closed rather than falling back to a cookie;
both transports use the same session hash, expiry and live receiving grant.
Upgrading from full-file session bindings requires reopening the Mini App once
to exchange a new signed launch; existing native history/control records are not
reset. All writes require the exact HTTPS Origin;
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
disable the control. Native connection failures never become idle.
  The first authenticated observation runs once even if the WebView initially
  reports hidden; manual Refresh also runs independently of that flag. HTTP
  reads/authentication have a20s client deadline; Stop has30s. A timeout restores
  feedback/retry controls without replaying an uncertain Stop. Control remains
  hidden and disabled until an authenticated observation enables it.

## Stop and recovery

The button describes the entire receiving owner's scope; session filters do not
alter it. Confirming journals a unique action before connect-only native control
and uses that same nonce in the existing durable Stop coordinator. Concurrent
retries or service restart do not redispatch uncertain effects. Lost acceptance
is unconfirmed, not stopped. The live view reads the coordinator's persisted
phase/target receipts to distinguish requested, observed and failed/unconfirmed.
When the operation is observed `stopped`, the same button becomes **Resume owned
sessions**, with a separate confirmation. It submits the exact observed operation
through the receiving native `resume ID` control, which rechecks target bindings,
inactivity and unchanged goal definitions. Formerly active work continues from
retained history; previously paused/blocked goals remain unchanged. Partial or
unknown Stop states cannot resume, and a stale operation cannot select another.
Both actions share Origin/CSRF/live-grant checks, but each Resume intent has its
own operation/nonce journal namespace. Retries and restart never redispatch an
uncertain action. Resume acceptance is distinct from actually observed recovered
turns and child recovery. `/stop resume ID` remains the console/Telegram fallback;
restart/release remain on that command surface.

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

## Compact overview

The default view uses a small header, aggregate working/goal/active-subagent
counts, one collapsed host summary and scrollable session rows that fill the
remaining stable WebView height. Header, filters and control remain readable;
short screens and opened disclosures can scroll the page rather than clipping
controls. Compact launch and expanding the native window both resize the list. Each
row exposes its state/elapsed time and retained goal state; opening it restores
the full name, literal objective, tools, tree and return link. Host details and
Stop scope remain available through disclosures. Refresh retains opened rows
and list position. Readable text and44px touch targets are preserved.

The page calls neither expand nor requestFullscreen. Telegram owns the native
window height; compact page content does not prove half-height launch. The
mode=compact parameter documented for registered Main/named direct links must
not be appended to an unrelated HTTPS URL or redirected into the existing Nexus
Main App. To use a genuinely half-height entry, register a separate named app
on the receiving bot through BotFather, pointing to this service's HTTPS page.
Once BotFather confirms the app's actual short name, set `miniapp_short_name`
in the private dashboard config and run `publish-menu`. The operator selects
`https://t.me/<verified-bot>/<registered-short-name>?mode=compact`, which
[MenuButtonWebApp supports](https://core.telegram.org/bots/api#menubuttonwebapp).
This preserves the existing Nexus Main App and consumer. Do not set a guessed
name or claim that menu read-back proves app registration or phone launch:
registration is a separate BotFather action, and half-height needs real client
acceptance. Omit the setting to retain the existing HTTPS menu. The same scoped
`restore-menu` command remains available.

The visual direction follows Cluster's maintained Star Trek-inspired Matrix
reference: Space Grotesk, orbital/interference mark, deep-space canvas, cyan
navigation and amber actions with restrained segmented control rails. The
existing licensed Latin font is self-hosted with its original OFL and hash
provenance. The selected canvas stays dark regardless of Telegram or OS light
mode. Supported Telegram header, background and bottom-bar colors follow it;
older clients retain their own chrome without preventing authentication or use.

## Shared appearance and public asset loading

The optional `appearance_origin` is the exact HTTPS origin of this receiving
body's established home style consumer. It is not another Homemaker deployment.
The HTML head loads that home's public `style.css?surface=sessions` before paint
and reuses `home-style.js` for visible updates and last-valid failure fallback.
Palette/font roles map to existing controls; native Telegram chrome follows the
computed background. The editor's authored shell is unaffected. Without this
configuration, the existing local Matrix composition remains the fallback.

This source is public author appearance only: no dashboard bearer, initData,
human ID or private viewer preference is sent to it. CSP permits only the
configured style/font origin and its exact shared-consumer script; private
connections and control endpoints remain self-only. HTML/private data stay
no-store. Content-versioned public JS/CSS is immutable; other static resources
can revalidate through ETag. New code gets new URLs without clearing private
state. A currently open page still needs a safe reload to adopt new code;
foreground style changes do not reload or replay native operations.

Keep the existing dedicated bot/receiver grant and menu selection. Shared style
and a shared launcher do not transfer another bot's native session authority.

The header includes a direct **Apps** link to the shared launcher at
`https://apps.dmn.homes/`. It navigates in the current WebView without forwarding
Telegram launch data or changing receiving grants. It does not replace internal
BackButton navigation. Native conversation work continues independently.

## Bot traffic

The optional **Bot traffic** panel reads the receiving bot's private metering
journal, independently of native state. It shows 24h request/failure/body-volume
counts, recent rates, service/method and conversation counts, plus capture gaps
and Telegram cooldown/recovery state. Refresh uses the existing visible-page
cadence; an unavailable native probe does not hide the traffic or Alerts panels.
See [capture setup and actionable Alerts](bot-traffic.md#sessions-and-actionable-alerts)
for scoped configuration, condition/recovery policy, qualification and rollback.

## Pulso: open directly to private computer health

Optional `computer_health` reuses the existing receiving owner, Telegram signature
validation, session cookie/bearer, live allowed-human check and private prefix.
There is no public metrics endpoint, browser credential file, new bot consumer,
model call or native conversation read. `?view=health` opens the Pulso view;
`?view=sessions` retains the ordinary Sessions view. `default_view=true` makes
health the default only for this explicitly configured receiver.

Install the qualified `computer-health` skill 1.1.0 or newer through its supported
selective updater. Keep the receiving machine profile owner-only and outside the
repository; it selects only machines the human has actually authorized. Resolve
its paths from the receiving infrastructure map/binding, never the author's.

```toml
[computer_health]
enabled = true
default_view = true
package_dir = "/PRIVATE/installed-skills/computer-health"
config_file = "/PRIVATE/computer-health/config.json"
```

The backend pins the profile and collector/renderer bytes at startup. Drift
returns 503 until operator requalification rather than showing old data under a
changed binding. `/api/computer-health` checks the current receiving grant before
and after the read. Interaction-only and monitor companions acquire no health
scope. Anonymous access receives only the empty shell. The live renderer hides
file import, reuses the shared validated report event and holds its short-lived
session only in memory. Sessions' strict CSP externalizes the maintained viewer
script/style; artifact sandbox rules are unchanged.

Opening starts one finite read; visible clients reconcile every 30 seconds.
Concurrent readers share one in-flight collection and the previous successful
reading is returned immediately. Cold clients see collection progress. Collection
stops being requested while hidden; an already admitted finite read can finish.
The backend stores only one bounded owner-only current report, not an immutable
publication per refresh or an unbounded history. A missing remote machine retains
its original timestamp separately from the failed attempt. Authorization errors
clear the private view. Whole-host/cgroup scope, reclaimable cache, cumulative
counters and untested application readiness remain visible distinctions.

For qualification:

```text
python -B tests/dashboard_computer_health_io.py
PULSO_TEST_PACKAGE=/PRIVATE/installed-skills/computer-health python -B tests/dashboard_computer_health_io.py
```

The first runs offline transport/auth/single-flight contract fixtures; the second
also exercises the real installed collector and renderer. Browser fixtures must
verify cold open, automatic reads without a file picker, stale remote readings,
visibility suspension and authorization loss. They do not establish real Telegram
launch or a phone acceptance receipt.

Deploy through the existing receiving sidecar operator, preserving native/bot
units, reverse proxy, receiving grants and SQLite action/session journals. Restart
only the sidecar at a suitable point; no global Stop is needed. A system-owned
unit requires its actual administrator when the receiving user cannot manage it.
Qualify the existing HTTPS prefix and private 401 boundary after activation, then
publish the receiving launch through the maintained home/Apps contract. A general
pair capability requires its registered receiving route; do not invent one or
redirect other humans into the author's private bot. An explicitly author-owned
machine dashboard may use the documented author dependency with that reason
visible. Shared source publication is not receiving deployment.
