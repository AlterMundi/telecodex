# Original conversation input

The owner-local controller accepts non-Telegram text into the configured native
integrating entry. It reuses the existing queue, native writer and active steer;
there is no second model session or fabricated Telegram update. The entry must
already have an owned native binding. HTTP/composer, microphone input and generic
controller dispatch/create references are still being integrated; this backend
is not yet exposed in the receiving Mini App.

`telecodex --control CONFIG input submit JSON` accepts only `binding`, `id` and
`text`. The binding comes from the authenticated presentation list, `id` is a
canonical UUID and text is bounded to 32 KiB. The receiving configuration selects
the human and integrating entry. The existing owner UID/socket protections apply.
The exact text is committed before transport. Reusing an ID inspects its original
outcome; changed text is rejected. A lost response is inspected with
`telecodex --control CONFIG input status UUID`, without automatic resubmission.

Queued and steering inputs retain their original owner, native context and Stop
admission epoch. Grant revocation, binding retirement or deletion permanently
retire their execution authority; restoring current values does not revive them.
Writer maintenance is checked again at the native-write boundary. Restart retains
known-unsent inputs separately from uncertain writes. A matching late native ACK
can resolve uncertainty for the same execution, without another submission.
Cancellation that the native provider observes before its initial write produces
a typed never-submitted result, including corrections folded into that packet.
Retention and native acceptance are separate from worker incorporation.

Admitting an input selects canonical-conversation presentation for that native
context before possible native uptake. The durable selection also applies to an
already-running Telegram-root turn, later turns and receiving collectors. Live
text, finalization, artifacts and native human requests consult that selection.
It follows the native context across logical-session recreation; a new native
context has its own selection. Earlier publications already in transport are not
retracted by selecting another return surface.

Native questions retain their existing response channel and full request in the
scoped human-interaction store. Approvals likewise retain the exact proposal,
allowed choices and pending responder. Selecting canonical presentation neither
publishes these requests to Telegram nor grants an approval. Canonical request
controls still need their authenticated client integration. Retirement and restart
never resurrect a response channel. Generated files remain retained with their
native turn when automatic Telegram presentation is not selected.

`tests/conversation_input_control_io.py` uses the actual owner socket, listener,
SQLite and synthetic native/Telegram peers to qualify original-text preservation,
deduplication, steering, late ACKs, held resume, authority retirement and mixed-origin
presentation. Tests use no real models, Telegram accounts or external providers.
