# Selected visual context

The integrating conversation can retain one selected point in one still image
with an original text message or finalized voice clip. The original UUID,
authenticated owner, entry generation, admitted destinations, correction order
and return route remain the existing Interaction contracts. Uploading an image
alone starts no conversation or model work.

In the browser, open **Point to something in an image**, choose an image and
select a point. Clicking, arrow keys and explicit X/Y fields select the same
image-pixel coordinates. Then write or speak normally. A message can still use
`#Topic`, reply context or semantic routing. Choosing another image clears its
selection. Recording freezes the selected image before microphone acquisition;
a later view cannot silently replace it while ASR is preparing.

The browser normalizes the selected view to a bounded PNG, at most 2048 pixels
on each side and 2 MiB. The authenticated server accepts still PNGs of at most
4 megapixels, validates chunks and fully decodes them, and retains their exact
hash-named bytes in an owner-private directory beside the receiver database.
No public URL or caller-supplied filesystem path is accepted. Readback uses the
original input UUID under the same owner grant, not a public image URL.

## Reference and transport

`POST /api/interaction/visual` uploads PNG bytes with `Content-Type: image/png`,
`X-Image-Sha256`, the existing session/device authorization, Origin and action
token. It returns the hash and actual image dimensions, with
`work_admitted: false`. The existing original-input body accepts an optional
`visual` reference:

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
selected/image revisions, an unavailable/ambiguous/disconnected view, a missing
point or absent image bytes. The HTTP response asks for a fresh selection and
admits no original. A structured `visual_reselection` response certifies that
this attempt did not reach original submission. The composer can request a new
selection; an explicit recording retry can then freeze the same saved clip
under a new original identity. A transport failure or unknown outcome never
authorizes that replacement. Once retained, the frozen reference remains historical
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
