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

## Browser access

Mount the trusted conversation on the receiving registered home and reuse its
existing [shared login](https://github.com/nicoechaniz/daimon-cluster/blob/main/docs/design/home-browser-client.md#trusted-application-identity)
at `https://auth.dmn.homes`. The browser keeps the home's HttpOnly session;
no credential file, browser token store or additional pairing service is used.
The receiving backend checks `/access/identity` on that exact home, matches its
verified human to the existing native receiving human, and enforces the same
interaction-only permissions. Expiry and revocation stay with shared auth.

Enable the receiving adapter with `[shared_auth]`, `enabled = true`, an exact
`home_origin`, and `entry_path = "/interaction/"`. Add that exact home to
`additional_origins`. The selected proxy strips only the entry prefix, preserves
Host/cookies and supplies `X-Forwarded-Prefix: /interaction`. Mount `/access/*`
and `/access-client.js` on the existing home gateway. The clean entry redirects
to the standalone conversation and shared sign-in returns to that same path.
`artifact_access = true` additionally permits the receiving conversation's
existing generated-file downloads; administration and approvals stay separate.

`alerts_access = true` optionally adds a read-only operational-alert list at
`GET /api/interaction/alerts`. It defaults to false. The Conversation badge and
list reuse the same receiving Alerts projection and attention rules as Sessions
and the Apps menu. Existing SSE invalidations include receiving alert changes;
subscribing does not initiate observation. Opening the badge is explicit and
leaves the draft and playback nodes intact. Shared-home access cannot acknowledge
incidents or invoke Sessions controls, and existing device grants are unchanged.
This grant does not activate model attention or automatic incident handling.

Visible standalone answers expand through the existing immutable source/part
endpoint without requiring a Read reply click. Intersection-driven reads use at
most two automatic readers and do not block the next text/voice input. Completed
text is kept only in the current page; ordinary metadata refresh does not fetch
it again. Changed source revisions cannot apply late text to their replacement.
Hidden pages stop starting further parts and resume pending reads in foreground.
Very long answers retain Continue reading after the bounded automatic expansion;
manual reading remains available when the browser lacks intersection observation.

The enrollment commands above remain for physical Talk clients and the selected
read-only monitor. They are not human browser onboarding.

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
and can inspect only their own canonical input traces. Enrollment does not start
media, models or a browser; opening a selected monitor is an explicit operator action.

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

## Conversation portrait and activity

The standalone client adds a decorative ASCII portrait and evidence-based activity
indicators. See [the avatar contract](interaction-avatar.md) for source attribution,
freshness, finite motion, optional hint limits and qualification. This presentation
adds no enrollment, device grant, native owner or capture authority.

## Presentation profiles

Set `companion.presentation_profile = "simple"` (default) or `"explorer"` in the
receiving dashboard configuration. Enrollment may select the device's initial
presentation with `enroll ... --profile explorer`; credentials themselves do not
supply authority for this setting. Existing grants without the field use the
configured default without rewriting credentials or changing their audience.

The receiver returns this setting as `presentation_profile` on the conversation
response. Simple starts its bottom diagnostic panel collapsed; explorer starts
it open. Both may open/close the same read-only evidence. Refresh preserves the
person's panel choice until the selected profile or credential/binding changes.
The portrait, readable text/voice interaction and live questions stay in the main
area in both modes. Forged query parameters or client-file preferences cannot
change the receiving profile or expand its grant.

A profile is presentation, not a child account or a permission tier. Device
credentials continue to deny Sessions administration, direct native-request
decisions and approvals; read-only monitors still reject all writes. Owner
control remains separately authorized. Actual child enrollment/sandbox policy
and native mobile/badge clients remain later work.

## Recover a message that was not sent

A current saved original can expose **Try again with #Topic** when the receiver
knows that its selected instruction never reached native admission. This action
uses the same original and instruction IDs, the existing recovery transition and
current owner/entry/admission checks. It creates no replacement human utterance,
source transcript or task. Duplicate clicks cannot submit an accepted, queued
or uncertain instruction again. A lost recovery acknowledgment requires a saved
status check before another action. Read-only clients can inspect saved status
but cannot recover. Retired originals, unrelated instructions and held targets
remain unavailable. The action is explicit; loading a page does not resend work.

`POST /api/interaction/inputs/UUID/retry` accepts the current `binding` and exact
`instruction` offered in that original's `recoveries`. It uses the same scoped
conversation credential and CSRF check as destination selection. Existing
`/interaction retry` semantics remain unchanged; this is their first-client
adapter, not permission to replay an uncertain result.

Human-task Dismiss uses POST `/api/interaction/tasks/{id}` with the existing
interaction identity, CSRF token, current entry binding and exact task revision.
The only action is `dismiss`; arbitrary state changes and native decisions are
not supported. Monitor grants remain read-only and have no task access. The
receipt records human attention cancellation, never work completion or approval.
