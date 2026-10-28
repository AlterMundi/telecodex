# Original files in the conversation

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

Qualification: original PDF/text/binary/empty byte retention and malformed
reference/changed source/symlink rejection; UI selection order, upload on send,
acknowledgement reuse and preserving later selections; actual HTTP/native peers
for text+voice and cold/busy front+routed workers, immutable retry and restart.
These controlled peers make no model calls. The HTTP journey can optionally read
an operator-selected local PDF via TELECODEX_ORIGINAL_FILE_FIXTURE; no private
fixture is versioned. Passing stub journeys do not prove human-device acceptance.

The actual compiled CI candidate passed the HTTP/native journey, including a
valid five-page raster PDF, direct reads of retained paths in cold/busy front and
routed packets, immutable retry, voice and restart. Chromium qualification
covers real file selection, original submission, authenticated verified download
and reload. Synthetic peers/capture spend no model calls; human-device acceptance
and content interpretation remain separate evidence. Receiving adoption is
recorded in the owning issue and operator release receipt.

Each receiving file store is capped at256MiB and1024entries, serialized across
uploads. Full storage returns507 before admitting work; verified existing-byte
retries still succeed. Originals are retained without automatic eviction.
This is a bounded stop policy: operators must reconcile source references and
preserve required originals before explicit archival/retirement; do not delete
files by age or restore an old input journal. Temporary upload files are removed
on ordinary success/failure. Interrupted staging files count against the same
capacity until an operator inspects them; no unbounded cleanup daemon is added.
