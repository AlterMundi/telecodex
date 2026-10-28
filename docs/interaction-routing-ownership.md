# Routing ownership and scope corrections

Owning plan: [continuous voice and responsive conversation](interaction-roadmap.md),
[#301](https://github.com/AlterMundi/telecodex/issues/301), milestone9.
This is part of foreground coordination (#303), not another routing service.
Controller completion: [#383](https://github.com/AlterMundi/telecodex/issues/383).

## Verified failure — 2026-10-27

A human original combined a project-specific document question with a request
for general application file transport. The integrator said it had split those
tasks, but the retained dispatch sent the whole original to both workers. The
project worker implemented transport. The correction was again sent to both
workers asking them to coordinate ownership. That worker then issued a local
helper request asking the transport owner to take over an existing candidate.
The human subsequently explicitly selected that transport owner.

The exact selected originals, instructions, native commentary and helper
admission were inspected in the retained owner records. Detailed private source
references remain in the project checkpoint; this document does not publish
the original document, transcripts or private operational paths.

Three concrete gaps explain the loop:

1. `dialogue::Destination` retains selector/relation/request IDs, but no
   per-destination task scope. `store::interaction::admit_on` copies the entire
   source text to each instruction. The spoken task split was absent from the
   retained work instruction.
2. The correction delegated ownership arbitration to workers rather than making
   one selection in the integrating conversation.
3. `session_request` creates a bounded dependency with a return to its caller.
   It does not reassign the canonical human request. Recipient acceptance is
   evidence of the helper's admission, not transfer or implementation completion.

This is evidence of scope/ownership ambiguity. It does not establish duplicate
transport delivery, prove a model defect, or justify replaying the original.
The useful existing file candidate stays with the human-selected transport
owner under #380; Realtime does not create a competing implementation.

## Use the current supported path

The integrator selects one implementation owner per distinct task using the
human direction and actual component ownership. Send a mixed original to several
workers only when its distinct clauses clearly address each worker's task.
Preserve the exact original and its chronology; never rewrite session-authored
interpretation as human speech. If the partition is ambiguous, clarify the
missing choice instead of claiming a split that the controller cannot represent.

For a correction, preserve the existing artifact/checkpoint and make the new
owner explicit. Use actual request IDs for that destination's amendments; IDs
owned by another destination are not portable. Stop only the superseded subtask,
preserving unrelated work, sessions, goals and uncertain effects. Report retained
recipient acceptance separately from incorporation and completion. Do not ask
several workers to decide who owns it, manufacture completion of unfinished work,
or use a helper request as proof that canonical ownership changed.

Helpers remain useful for bounded consultation. Their internal result goes back
to the caller with its attribution. A worker reporting a scope mismatch should
return its finding and useful checkpoint to the integrator; it should not start
an ownership-negotiation chain. No global Stop or session cancellation is needed.

## Remaining controller work

The immediate correction is runtime/maintained guidance. It is not a structural
guarantee: there is still no retained per-destination scope or canonical transfer
operation. Extend the existing dispatch/request journal rather than adding a
second queue or coordinator. A proposed scope must remain attributed integrator
interpretation alongside the immutable original. Exact retries must retain the
same scope; changed scope is an explicit amendment, not a silent overwrite.

A transfer must retain old/new owner lineage, the selected human correction,
checkpoint and actual receiving outcome. It must retire only superseded work
responsibility without declaring the work complete or replaying already admitted
input. Old and new results remain visible history, with one current responsibility
for each task. Consultation and transfer must be distinguishable in the API and
agent context. Do not advertise a transfer primitive before it exists.

Qualification should cover a synthetic mixed document/transport original, a
correction during active work, a retained helper consultation and a real transfer,
including lost acknowledgment/reconnect, immutable retry, direct-topic behavior
and one current human-facing result. Synthetic evidence must not be labeled as
human correction or production acceptance. Reuse the existing routing evaluator
for selection and store/controller fixtures for ownership semantics; no paid
model reruns or replay of this private incident are needed.
