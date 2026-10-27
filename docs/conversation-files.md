# Original files in the conversation — implementation candidate

Owning issue:#380, milestone8. The independent composer adds Files beside
Gallery/Camera. Files retains original bytes rather than fitting them onto the
image canvas. Text and microphone capture freeze ordered file references into the
same durable original; later selections remain available for a later message.

The existing authenticated/CSRF-protected app receives at most20MiB per file,
eight files and40MiB per original. It verifies filename/media-type bounds, byte
length and SHA256, then retains an owner-private content-addressed original.
Upload acknowledgement admits no native work. Original capture compares the
complete ordered references on duplicate IDs and rechecks retained bytes before
admission. The native start/steer and routed worker receive those actual retained
paths and metadata; file contents are untrusted evidence, never instructions.

Downloads use the owned original input plus its file index. Responses force
attachment/octet-stream/no-sniff and do not provide arbitrary-path reads. The
server does not interpret or execute uploaded content. Declared MIME identifies
the user's file; it is not verified content classification. Generic files remain
separate from image capture/pixel provenance and from voice transcription.

Qualification added: original PDF/text/binary/empty byte retention and malformed
reference/changed source/symlink rejection; UI selection order, upload on send,
acknowledgement reuse and preserving later selections; actual HTTP/native peers
for text+voice and cold/busy front+routed workers, immutable retry and restart.
These controlled peers make no model calls. The HTTP journey can optionally read
an operator-selected local PDF via TELECODEX_ORIGINAL_FILE_FIXTURE; no private
fixture is versioned. Passing stub journeys do not prove human-device acceptance.

At this checkpoint the code is not deployed. Source checks/media/UI regressions
have passed; final native HTTP qualification and receiving adoption are pending.
Keep this status explicit until the exact candidate is qualified and selected.
