# Coordinated Stop and Resume

`/stop all` controls the requesting allowed human's registered conversations in
this receiving provider. `/stop status [ID]` reports the operation and
`/stop resume [ID]` requests continuation. An omitted ID selects that human's
latest operation. Bare `/stop` retains the existing single-turn cancellation.

The provider requires the existing shared native App Server. It never starts a
replacement server, consumes another bot or infers ownership from a harness
account. Native histories, settings, identities, goals, pending records and
polling offsets remain in their existing stores.

Stop first persists an owner-scoped admission fence and target snapshots.
Journal admission captures a durable ownership epoch before attachment downloads
or command preparation. Stop retires earlier epochs; queued/native-submission
paths and Nexus respect the fence. Releasing it never revives an earlier input. New messages remain
journaled without native submission; uncertain already-transmitted input is not
replayed. Known held messages remain retained for an explicit human decision;
Resume does not secretly replay commands, expired question replies or input.

The finite coordinator runs outside model turns. Its subscribed-worker control
captures the actual before/after goal state, disables staged activation and
pauses only active goals. It requests physical interruption and observes native
inactivity and non-active goals. Native descendants are enumerated using the
installed protocol's ancestor filter, with complete bounded pagination and a
validated parent chain; foreign or incomplete ancestry prevents confirmation.
Descendant turns and active goals receive Stop controls too. Parent idle alone
never establishes child inactivity. These receipts certify selected native
sessions/descendants, not arbitrary detached host processes. Other maintenance
requires the relevant resource's own quiescence checks.

Resume verifies the unchanged authorized native binding and paused goal
definition. Only formerly active or already-staged goal intent resumes; already
paused/blocked/terminal goals retain their meaning. Ordinary interrupted work
gets a new explicit continuation using retained history, rather than replay of
its initiating prompt. The parent receives recorded child IDs and previous goal
states for recovery through native agent tools. Parent admission does not prove
child recovery; report child receipts separately.

Resume keeps the fence until every intended continuation has a native acceptance
or terminal receipt, or is explicitly unchanged. Queueing alone is not called
resumed. Partial/unconfirmed legs keep their state and are not retransmitted.
Restart retains the fence and marks interrupted coordination unconfirmed; it
never recreates native effects. A new Stop can supersede a previous resume,
invalidating its unadmitted packets. Independent maintenance holds are preserved.
`/stop release [ID]` explicitly removes this operation's fence without issuing
continuation or restoring goals; in-flight unknown effects remain unknown.

Coordinated interrupted work retains attachments and partial artifacts for
recovery. `/retry` refuses its old initiating prompt; use the operation's explicit
continuation. Removing retained files is a separately justified cleanup, never a
way to manufacture idle status.

## Maintenance restart and automatic continuation

After Stop reports `stopped`, `/stop restart [ID]` explicitly authorizes one
continuation after the service restarts. It does not restart the service or host;
the maintenance operator still performs and verifies that operation. Only
conversations with a previously active turn or active/staged goal receive the
restart and service-return notices. Previously idle conversations remain quiet.
Notices record attempted/delivered/unconfirmed status; an uncertain Telegram
send is not automatically repeated.

The receiving provider claims this persisted request once, on a later instance,
before effects. It validates the same bindings, holds and paused definitions as
manual Resume. Formerly active goals and ordinary interrupted turns continue;
previously paused/blocked/terminal goals remain unchanged. A second restart
never retransmits a continuation. A crash or validation failure keeps uncertainty
visible in Stop status, rather than manufacturing a successful resumption.
Startup without this explicit request never resumes sessions by inference.

## Owner-local invocation from any conversation

Configure an explicit private receiver binding, replacing placeholders locally:

```toml
[session_control]
socket_path = "/private/receiving-directory/control.sock"
human_id = 100000000  # replace with the receiving allowed human
```

The directory must be owner-only. The Unix socket is mode0600 and both sides
verify the peer UID; unsafe or already-live paths are refused. Request/response
sizes, connection time and concurrent client count are bounded. This is a
privileged owner-local interface, not a remote human-authentication endpoint.
Installing or reading a skill does not configure/start this listener.

The connect-only client uses the same receiving provider as Telegram:

```text
telecodex --control RECEIVING_CONFIG stop all
telecodex --control RECEIVING_CONFIG stop status OPERATION_ID
telecodex --control RECEIVING_CONFIG stop resume OPERATION_ID
telecodex --control RECEIVING_CONFIG stop restart OPERATION_ID
```

It starts no bot or model. Invoking it from an existing native conversation does
not make an unregistered CLI/Hermes session an authorized target. Other bodies
need their own explicit binding and grant; unsupported harnesses remain
unsupported. AlterMundi/Skills owns the portable `session-stop` workflow.

Qualification uses locked text checks, real Unix/HTTP/WebSocket/SQLite journeys,
scoped restart/admission/goal/descendant cases and the existing affected goal and
merge journeys. Live human Stop-update-Resume and actual child recovery are
separate receiving acceptance evidence. Implementation: issue #68.
