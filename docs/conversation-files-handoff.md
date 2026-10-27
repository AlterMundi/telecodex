# Work-in-progress handoff: original files (#380)

2026-10-27. Nicolas corrected scope: file transport belongs to Telecodex, while
HBC must return to Ani's measured PDF. This is a source checkpoint, not a release.

Branch:feat/conversation-files, worktree~/Projects/telecodex-report-recovery.
This worktree is transferred for the Telecodex session to adopt; HBC will stop
editing it after this checkpoint. Preserve the cached target and unrelated
untracked Python caches. Ordinary~/Projects/telecodex has other dirty work.

Implemented candidate: immutable opaque original file storage and upload/download
API; Files selector with bounded original bytes; frozen files paired with text
and microphone capture; original-store immutability, native start/steer file
context and routed worker attachment/context. See conversation-files.md.
No code was deployed or selected. No new model calls, human inputs or global Stop.

Verified locally: cargo check; cargo clippy --locked --all-targets
--no-default-features -- -D warnings; Python syntax;66dashboard/companion tests;
existing microphone/conversation/visual JS journeys; new files_ui.js and
conversation_files_media.py. The new Rust unit compiles under all-target clippy
but has NOT been executed. The new conversation_files_http_io.py is registered
in CI but has NOT been run against a built candidate. No PR/CI run was started.

Next: review exact file/context propagation and failure behavior, run the native
HTTP test with the actual compiled candidate (prefer CI artifact to another large
local build: Projects is near21GiB free,20GiB floor). Fix its actual failures;
qualify a real browser Files/text/voice flow and safe old-client regressions;
then publish/adopt through existing listener/web routes preserving active native
collectors and goals. The optional TELECODEX_ORIGINAL_FILE_FIXTURE can exercise
an existing operator-selected local scanned PDF without committing private bytes.
Do not send, reupload or replay the human's old PDF input to prove delivery.

Known review points: no full native process journey yet; upload metadata is not
proof of file content review. Files are opaque and never executed; download forces
attachment/octet-stream/no-sniff. Current native packets contain local retained
paths, so qualify both cold and busy front/routed workers. Multiple attachments
retain order; later UI selections must survive consumption of frozen originals.

Current installed listener remains qualified PR378 SHA256
 d7abb7692a7920c69cd25796783b106b448c8fffd6a4b1b4785da6b2dd6c4449.
The deployed dashboard release is independently selected at
800ae05f81edda02330d6817f2aced457800f4c3-pending-navigation; inspect its source
before adoption and preserve unrelated current web changes. Do not infer that
this branch's web tree is the currently selected release.
