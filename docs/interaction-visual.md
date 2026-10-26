# Selected visual context

The integrating conversation can retain up to eight ordered still images, each with an optional selected point
with an original text message or finalized voice clip. The original UUID,
authenticated owner, entry generation, admitted destinations, correction order
and return route remain the existing Interaction contracts. Uploading an image
alone starts no conversation or model work.

In the browser, use the paperclip to choose a photo or take one. The whole image is attached
by default; selecting a point is optional. Clicking, arrow keys and explicit X/Y fields select the same
image-pixel coordinates. Then write or speak normally. A message can still use
`#Topic`, reply context or semantic routing. Gallery selection can add several photos at once; the camera adds one photo. Each
thumbnail can be removed or moved earlier/later without discarding the draft.
Choose a thumbnail to view it or mark its own optional point. Recording freezes
the entire ordered selection before microphone acquisition;
a later view cannot silently replace it while ASR is preparing.

The browser normalizes the selected view to a bounded PNG, at most 2048 pixels
on each side and 2 MiB. The authenticated server accepts still PNGs of at most
4 megapixels, validates chunks and fully decodes them, and retains their exact
hash-named bytes in an owner-private directory beside the receiver database.
No public URL or caller-supplied filesystem path is accepted. Readback uses the
original input UUID under the same owner grant, not a public image URL. Append `?index=0`, `?index=1`, and so on to the
existing image readback route for collection members; omitted index retains
first-image compatibility. Every read enforces the same original audience.

## Reference and transport

`POST /api/interaction/visual` uploads PNG bytes with `Content-Type: image/png`,
`X-Image-Sha256`, the existing session/device authorization, Origin and action
token. It returns the hash and actual image dimensions, with
`work_admitted: false`. The existing original-input body accepts an optional `visual`: a single reference
retains the legacy object shape below, while several images use an ordered array
of one to eight such references. Every member has a distinct `source_id`; each
image retains its own bytes, dimensions, times and optional point:

```json
{
  "source_kind": "selected_image",
  "source_id": "019cf620-0000-7000-8000-000000000001",
  "image_sha256": "<64 lowercase hex characters>",
  "width": 160,
  "height": 96,
  "captured_at": "2026-10-17T23:00:00Z",
  "selected_at": "2026-10-17T23:00:01Z",
  "selected_revision": "<the same image hash>",
  "state": "selected",
  "point": {"x": 20, "y": 30}
}
```

Array order is immutable under the original UUID. Empty or over-eight arrays,
repeated source identities and any unavailable member reject the entire new
submission before admission. Successful individual uploads may be reused on an
explicit retry; an upload alone never submits the message.

Set `point` to `null` to refer to the whole image, without invented coordinates.

Coordinates start at the top-left of the retained image: X points right and Y
down, within the actual pixel bounds. `source_id` identifies the client-selected
view; the revision identifies its exact bytes. Time and producer kind are
client-reported provenance, not authenticated camera observations. For a
`selected_image`, capture time is when that view was frozen for this interaction,
not an invented exposure/EXIF date. Camera frames and application selections
remain distinct producer kinds; a camera frame selected more than five seconds
after its reported capture is refused. This consumer does not install or enable
a camera/gesture producer.

A new selection must be no older than five minutes and cannot name different
selected/image revisions, an unavailable/ambiguous/disconnected view, or absent
image bytes. The HTTP response asks for a fresh selection and
admits no original. A structured `visual_reselection` response certifies that
this attempt did not reach original submission. The composer can request a new
selection; an explicit recording retry can then freeze the same saved clip
under a new original identity. A transport failure or unknown outcome never
authorizes that replacement. A later refusal cannot settle an earlier uncertain
attempt; once any submission is unconfirmed, its identity remains fixed until
its original receipt is observed. Once retained, the frozen reference remains historical
evidence while work progresses; elapsed worker/ASR time does not make it become
a different scene. Reusing its original UUID cannot change pixels, selected point
or original words and does not resubmit native work.

Finalized voice uses the same reference in the `X-Visual-Reference` JSON header;
the controller retains it before asynchronous ASR. The image accompanies the
native integrating turn and the selected work session through existing native
image inputs, including busy-turn steering and inputs coalesced during initial
native preparation. Routed provenance identifies the original reference.
Original human text is kept separately from controller/reference metadata.

Native structured questions return text through the harness tool protocol. A
visual answer includes an attributed description of its selected reference and
the validated owner-local image file, plus the original speech. The worker is
instructed to inspect those exact bytes with its image-reading tool before
interpreting it. This is a local native adapter; it does not promise that another
host can read that path. Source loss prevents dispatch/answer admission rather
than selecting a newer image from another message.

Images are evidence, not controller authority or recognized objects. A selected
pixel does not prove what object the human intended, the current physical scene
or that an action is appropriate now. The integrator/worker must ask when those
meanings are ambiguous. Existing tool/action/audience grants remain unchanged.

## Qualification

`tests/conversation_visual_http_io.py` exercises actual HTTP, controller, SQLite
and native image packets with synthetic native/ASR peers and zero model/sensor
calls. It covers preparation coalescing, the busy integrating/worker turn,
voice held across a later image, immutable retries, native question source
metadata, private readback, stale/changed/ambiguous/disconnected/missing evidence
and restart. The browser selector also requires actual browser interaction and
narrow-layout verification. Source tests do not establish semantic object
recognition, physical pointing or continuous realtime video quality. Receiving
selection and actual model/device acceptance must be recorded separately.

Owning visual adapter: [Integrators #5](https://github.com/AlterMundi/daimon-integrators/issues/5).
Parent: [Daimon Interaction](https://github.com/AlterMundi/daimon-matrix/issues/313).

## Receiving adoption and rollback

Deploy the qualified native consumer and web assets together before enabling
collection inputs. Legacy single-image originals remain readable. After accepting
a collection, keep a consumer that understands both JSON forms: selecting an old
single-image-only executable is not a compatible rollback for that journal.
Preserve recorded originals rather than rewriting arrays or deleting their history.
The receiving upgrade retains active writers/goals and reattaches collectors through
the existing supported route; no global Stop or original replay is authorized.
