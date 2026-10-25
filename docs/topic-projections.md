# Work-topic projections

`interaction.telegram_projection = true` enables secondary presentation in the
existing work topics selected by this receiver's interaction profile. It defaults
to false; the v0.2 receiving qualification must explicitly enable and test it.
This does not enable the legacy Telegram integrator entry, intercept ordinary
work-topic inputs or charge them to the router's worker quota.

Accepted relays show the exact selected text/transcript with a concise
integrator/source label. Queued, rejected and uncertain native inputs cannot
claim acceptance through this echo. Session-originated relays are labelled as
session messages. Human text stays source data, including protocol-looking
quotations. Native commentary and completed answers use their actual immutable
thread/turn/item identity, with the valid protocol footer removed for display.
Raw canonical evidence remains retained.

Both routed and direct output in an enabled work topic use the same canonical
event/destination queue. One native item covering several requests produces one
topic publication; the independent correlated returns stay available to the
integrating conversation. Distinct native items remain distinct, even when they
have the same text. Bot-authored projections are ignored by ordinary ingress and
never become another human input, steer, task or model invocation.

Publication order is retained at native admission/item observation. A slow or
failed Telegram copy does not hold the native event pump, the web return or
another destination's queue. Each destination has one SQLite publication lease.
The existing Telegram limiter still applies across the bot's outbound calls.

Parts and attempts are persisted before the network write. Positive receipts
retain their message IDs; an acknowledged prefix is not resent after restart.
Known-unsent parts can continue from their frozen rendering. Explicit rate-limit
rejection uses the existing cooldown and the same publication identity. Other
known client rejections remain inspectable. A lost/mismatched acknowledgment or
interrupted in-flight send becomes uncertain and is not automatically repeated.
Uncertainty does not block later distinct publications or replay any model work.
The existing bridge heartbeat resumes only retained, known-unsent publication;
it performs no native input, inference or extra Telegram update consumption.

Each network write revalidates the current allowed creator, logical destination,
native binding and fresh-thread flag. Changed ownership/bindings retire unsent
projection, while canonical text remains available from its original return.
Disable the setting to stop further projection; retain its journal. A finite
owner-local inspection is available without model work:

```text
telecodex --control CONFIG topics publications
```

The last 256 records include the destination, kind, bounded readable preview,
publication phase and acknowledged-part count. This is delivery evidence, not
proof of hearing or reading. Telegram does not provide an exactly-once guarantee
after a lost send receipt; the system retains that uncertainty explicitly.

Source qualification uses `store::topic_publications` and
`tests/topic_projection_io.py`: actual SQLite, bridge/control socket, synthetic
native peers and loopback Telegram HTTP. The process journey covers parallel
destinations, a held mirror with a completed primary return, repeated native
events/input IDs, same-session direct interaction, bot feedback, restart,
lost ACK and known rejection. These fixtures do not replace V01/V05 real-session
and receiving qualification in [interaction-qualification.md](interaction-qualification.md).

## Optional integrating entry surface

When the human selects `telegram_ingress` and `telegram_entry_projection`, the configured integrating topic also projects app text/committed voice transcripts, integrator commentary/answers, captured-scope worker progress and canonical routed results. The original app input identity deduplicates its echo across integrator and multiple worker admissions. Telegram originals already exist in the topic and are not echoed. The same current entry/native binding is used; work-topic ingress stays direct. This selection is recorded from receiver configuration at bootstrap before admitting work, so a disabled entry creates no new secondary entry queue. Enabling it does not backfill old turns or replay uncertain deliveries.

`telegram_entry_projection` defaults to false independently of the existing work-topic projection flag. Existing receivers that use Telegram ingress plus work-topic copies retain their web-only return behavior unless they explicitly select entry copies. Entry copies can also be selected without work-topic copies; input admission remains controlled only by `telegram_ingress`.
