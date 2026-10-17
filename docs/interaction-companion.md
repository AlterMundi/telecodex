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
renewal is an explicit enrollment. Disabling the route does not erase records.

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
