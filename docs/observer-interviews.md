# Interviews from observer findings

The integrating conversation can explicitly select `interaction_controller`
operation `interview` after inspecting an authorized observer finding. It reuses
creation's original-input identity, immutable intent, one topic effect, enrollment,
ordered writer, recovery and correlated return. It does not start a publisher,
background observer consumer or peer conversation.

Use the existing creation schema: `input_ref`, `name`, `reason` and one to five
`context_selectors`. The input must be an actual retained human request for the
independent conversation; selecting a source is not permission to publish it.
Missing summaries or unsupported/ambiguous selectors fail before topic creation.
The existing 48 KB total context bound applies. Long captured windows use
explicitly marked interview excerpts within that bound; metadata that still
cannot fit is refused before an external effect.

The frozen package contains each selected observer card and its actual retained
original source window, including native identity, revision, completed boundary,
activity cut, observation time, uncertainty and in-progress statements where
available. It records the owning session and its logical generation at selection.
That generation describes the selection-time binding; it does not reconstruct a
missing generation from an older source capture. Per-source selection timestamps
remain distinct from historical evidence timestamps.

When full windows exceed the bound, supported source entries receive priority
over other entries. Excerpts preserve every original entry position, including
in-progress entries, so the observer card's support indices remain valid.
Truncated indices and character caps are explicit. Each excerpt also records the
full captured source's JSON byte count and SHA-256. The complete captured window
remains in the scoped observer record; clipping the interview package never
rewrites it or the observer card. A bounded excerpt is not the full observer
input, an exact fork, or the source's subsequent live state.

This path deliberately creates a **pinned derived context**, not an exact native
fork. The source can remain busy and continue independently. No source turn,
goal, pending task or tool execution is copied or resumed. Durable local-request
admission rejects an interview worker trying to activate any pinned source,
independently of model output; an explicit new human instruction remains separate. The new conversation
receives an interview role and attributed evidence, not an assertion that it
performed the source work. Source changes after the cut do not silently update
the package. Ask for a newly selected context when those changes matter.

`creation_status` inspects the retained interview intent. For an owned original
in the current entry binding with no retained intent, it returns `phase: not_found`.
Unknown/foreign originals and storage failures remain errors, not absence. This
read-only observation is not permission to create and cannot rule out a concurrent
creation; the existing creation transaction still owns uniqueness and authorization.
`creation_resume`
continues only an existing known-unsent intent or confirmed topic through the
same non-replaying recovery rules. An uncertain external outcome is not a reason
to create another topic. Returned interview answers preserve their own request
and worker identity through the ordinary result path.

The synthetic HTTP/native/SQLite qualification in
`tests/observer_interview_io.py` covers idle and busy sources, frozen originals,
later corrections, source-work isolation, missing-source refusal and the exact
correlated answer, including a large multilingual source that previously exceeded
the creation bound. It makes zero model or real Telegram calls. A separate actual
model sample is needed to assess interview quality; fixtures prove contracts and
attribution, not the quality of generated explanations.

Refs: AlterMundi/telecodex#119 and AlterMundi/daimon-matrix#313.
