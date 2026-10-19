# Enrolled conversation devices

The selected device uses the existing integrating conversation and original
input/audio APIs. It has no native session owner, Telegram polling consumer or
independent model client. This enrollment is the receiving credential primitive
for daimon-integrators#4 and AlterMundi/daimon-matrix#313; the local button,
capture/playback state and physical-device qualification remain Integrators work.

An operator explicitly enables the receiving route in the private dashboard
configuration:

```toml
[companion]
enabled = true
grants_dir = "/absolute/private/companion-grants"
```

Enroll the actual selected device using the existing receiving configuration:

```sh
python -m dashboard.companion --config /private/dashboard.toml enroll \
  --label "Selected button" --output /private/new-device.json --seconds 86400
```

The command checks the actual allowed human and owned integrating entry before
publishing an expiring grant. It creates a new owner-only device credential file;
it refuses an existing output. Standard output contains the opaque grant ID,
scope and expiry, never the credential. Transfer only that private device file
through the selected authorized access route. Do not copy bot credentials,
native authentication, other session history or the server grant directory.

The device file supplies the exact HTTPS origin/prefix, `Authorization: Companion
TOKEN` and `X-Dashboard-CSRF` value. POST requests retain the exact configured
`Origin`. Input UUIDs, immutable clip hashes, source/revision reply references and
receipt-only recovery use the existing [original-input contract](interaction-input.md).
Selected audio uses the [existing source-linked speech contract](interaction-speech.md).

The server permits conversation sources, original history/status, pending request
reads, original text/finalized voice, explicit destination selection and selected
audio. It denies Sessions administration, global Stop/Resume, approvals and login
exchange. An explicit device credential never falls back to a browser's ambient
Telegram session. The native controller independently enforces the existing
human/entry/enrolled-target profile; model output cannot authorize an unlisted
destination. Underlying work retains that actual owner's selected environment.
This is not a child enrollment or a new sandbox for an adult development profile.

Grants bind the selected bot, HTTPS origin/prefix and actual receiving authority.
The live human grant and device record are checked again after private I/O,
including synthesis. Changed authority, expiry, disabled device access, unsafe
records or revocation deny the next request. Restart retains grants without a
new token or model call. The default lifetime is one day, bounded to 30 days;
renewal is an explicit owner operation. Disabling the route does not erase records.

Extend an existing interaction-only button enrollment without replacing the
credential or its saved capture identity:

```sh
python -m dashboard.companion --config /private/dashboard.toml renew GRANT_ID \
  --seconds 2592000
```

The receiving expiry is authoritative for this button client; the unchanged
credential file retains its original issuance metadata. Renewal requires the
same live human, bot/entry authority, scope and CSRF, and an expiry later than
the existing one, at most 30 days from now. Expired retained grants may be
explicitly renewed; revoked or absent grants cannot be recreated. Renewal and
revocation share an owner mutation lock: a busy operation reports failure rather
than racing a deletion. No automatic renewal, media, model call, device restart
or permission expansion occurs. Read the returned receiving expiry in the local
operation receipt. A read-only browser monitor instead requires a newly enrolled
file when its client-side expiry is reached.

A receiving-profile change deliberately invalidates the old grant. After the
owner has qualified the selected new profile, adopt it for one existing device:

```sh
python -m dashboard.companion --config /private/dashboard.toml reauthorize GRANT_ID \
  --previous-authority EXACT_RETAINED_PREVIOUS_AUTHORITY_HASH
```

This owner-local command requires an unexpired retained grant, the exact previous
authority and the same currently allowed human, then verifies the live owned
entry. It preserves device bytes, scope, CSRF, lifetime and captured client
identity. HTTP requests and ordinary renewal cannot perform this migration.
Revoked, absent, foreign-human and expired grants are refused. Repeated adoption
of the already selected profile reports its current state without another write.
The owner migration shares the existing mutation lock with renewal/revocation.
It creates no media, model execution or automatic access migration.

Revoke the selected grant using its opaque ID:

```sh
python -m dashboard.companion --config /private/dashboard.toml revoke GRANT_ID
```

Revocation durably removes its record. An identical retry establishes the same
durable absence; it does not recreate a credential. Audio already received by a
device cannot be recalled, and serving audio does not establish hearing.

`tests/dashboard_companion_io.py` exercises actual private files, HTTP and speech
helper/cache with synthetic identity and provider peers. It covers administrative
route refusal with an ambient full session, exact Origin/CSRF, profile changes,
expiry, reconnect and revocation during synthesis. It also checks interrupted
revocation retry and durable publication in newly created directories.

`tests/dashboard_companion_native_io.py BINARY` additionally uses the actual
native-controller process, SQLite and synthetic native/ASR peers for original
text and a committed voice question answer. An unlisted destination remains
unrouted; revoked requests are refused before voice buffering or ASR. Both
fixtures use zero inference. They do not prove a physical button, audible output
or real Telegram device acceptance.

## Standalone conversation browser

Open `?view=conversation&client=standalone` in an ordinary browser, then select the private
interaction-only access file issued through the existing enrollment command.
This entry loads without the Telegram SDK. It uses the same conversation, retained originals, text/voice endpoints and
selected speech service as Talk. It does not exchange a Telegram login, create
another native session owner or inherit ambient Sessions cookies.

The selected credential stays in this tab's session storage for reconnects.
Its receiving grant remains authoritative, including operator renewal. Changing
or refusing the file clears the selected access instead of falling back to an
ambient session. Keep the file private; revocation and expiry use the existing
grant commands above.

The conversation can inspect its own canonical input routing traces. Ordinary
Telegram-topic history and Sessions administration remain outside this grant.
Pending native questions can be read and answered through the conversation;
approval controls require Sessions. Standalone mode does not display controls
that its grant cannot operate.

## Standalone read-only routing monitor

The existing browser UI also supports ?view=monitor without Telegram. An
operator enrolls a separate, expiring monitor_readonly credential using the
same receiving grant service:

    python -m dashboard.companion --config /absolute/dashboard.toml enroll \
      --label "Selected monitor" --scope monitor_readonly \
      --seconds 86400 --output /absolute/private/monitor.json

Open the configured Sessions URL with ?view=monitor, then select that private
credential file. It must belong to the exact origin and receiving prefix. The
browser keeps it in the current tab's session storage so a reload can reconnect;
it is never put into a URL, query string or public artifact. Keep the file private
and use the existing revoke command to withdraw access.

This mode reuses the Conversation renderer for retained originals, Telegram
transcripts, exact routing traces and correlated replies. It has no message,
route-selection, question-answer, playback or session-control authority. The
server rejects every write and administrative route even if the browser also
has an owner cookie. Existing interaction-only device grants remain unchanged
and cannot inspect routing traces. Enrollment does not start media, models or a
browser; opening a selected monitor is an explicit operator action.

The monitor displays the operator-assigned source label returned by the receiver, so a separately enrolled experiment can be visibly distinguished from the ordinary conversation. Credential-file labels do not determine the displayed source.


Receiving-profile adoption also affects configured Alerts and Traffic companions:
both retain fingerprints separately from device grants. Reauthorizing a device
does not requalify their journals. Before selecting a changed profile, exercise
all configured receiving constructors and reconcile their exact dependencies.
For an ASR-command-only change, verify the same bot/human, Alerts endpoint and
credential, receiving scope, adapter, traffic meter and producer before adopting
new fingerprints. Preserve delivery attempts/receipts, menu selection/pending
state, traffic incidents and outbox; never clear or recreate them to bypass a
binding mismatch. A listener successor must retain native writers/goals and
collectors independently of this web-side requalification.


The read-only monitor presents a virtualized horizontal conversation graph.
Inputs and routing share the integrating conversation lane; work sessions have
separate lanes keyed by retained target identity, even when labels match. New
observations append to the right; stable source IDs update their own nodes.
Each node retains at most the first300 Unicode characters and opens the existing
saved-input/reply inspector. Declared instruction coverage is required before
linking a session reply to an input; an earlier result must never acquire an edge
from a later correction. Integrator replies without a retained exact input link
remain visibly unlinked.

The first saved-history layout reconstructs input/workflow relationships rather
than inventing historical timestamps. Later placement reflects browser observation
order. Scrolling, keyboard navigation or selecting a step pauses automatic follow;
Return to live resumes it. The existing foreground refresh reads at most six closed
input traces per cycle, rotating through retained inputs independently of open
inspectors. No extra poller, model session, routing action or media is introduced.
The graph resets with the existing credential/binding invalidation path.
