# Telegram harness convergence pilot

The pilot transfers useful interaction patterns into the existing native Codex
frontend. It preserves each harness's backend and existing session, authentication
and memory boundaries. It does not require a new shared agent runtime first.
Parent: [Matrix #262](https://github.com/AlterMundi/daimon-matrix/issues/262).

## Bounded source comparison

These are inspected source baselines, not claims of current feature parity or
live receiving acceptance in every harness:

| Source and pinned revision | Useful behavior and evidence | Decision for this delivery |
| --- | --- | --- |
| [Headcrab/telecodex `0aa3231`](https://github.com/Headcrab/telecodex/tree/0aa3231e98748364a4fcb63b6ebcf73199c5a2c0) | README and inherited runtime: independent topics, native thread binding, steering, attachments/artifacts, authentication, settings, history and rate-aware Telegram output. | Reuse the existing Rust bridge. Preserve its license and submit generally useful changes upstream. |
| [AlterMundi/telecodex `289d972`](https://github.com/AlterMundi/telecodex/tree/289d972f64be4cc55d2ec00f7e7b4ff00bfd13c1) | Native proxy/SQLite/HTTP qualification: durable ingress, completed messages, owner-selected search/access defaults, synchronized rename, scoped event reception and post-final continuation. | Keep this qualified behavior while adding native modes and questions. The activity indicator remains a separately qualified optional companion. |
| [NousResearch/hermes-agent `0c2ff85`](https://github.com/NousResearch/hermes-agent/tree/0c2ff854a5c5121c75ac0b0a35241a580ee27fb7) | [`send_clarify` and callback dispatch](https://github.com/NousResearch/hermes-agent/blob/0c2ff854a5c5121c75ac0b0a35241a580ee27fb7/plugins/platforms/telegram/adapter.py), plus [clarification-button tests](https://github.com/NousResearch/hermes-agent/blob/0c2ff854a5c5121c75ac0b0a35241a580ee27fb7/tests/gateway/test_telegram_clarify_buttons.py): complete choices in the body, short controls, an Other text path, authorized callbacks and expired-request feedback. | Adapt the interaction to Codex's existing typed request protocol. Do not copy Hermes's process-global clarification registry or invent another model tool. Keep full option descriptions and stale-control feedback. |
| [benedict2310/telecodex `fd2a241`](https://github.com/benedict2310/telecodex/tree/fd2a24134f0459e15df877bd5c8c7fc7455253fd) | README advertises per-context sessions, tool verbosity, live todo display, handback commands and optional reactions/usage. These advertised features were not independently live-qualified in this pilot. | Retain native resume handoff and topic independence. Prefer the current human-selected minimal output. Verbosity, todo presentation and a dedicated handback shortcut are subsequent candidates, not substitutes for working native questions. |

## Combined behavior and qualification

The first useful increment adds genuine topic-local `/plan`, `/default` and
`/questions` to the already functioning Telegram frontend. Buttons and quoted
or explicitly armed text respond to the exact native request; ordinary messages
still steer. Multiple questions, asynchronous work, stale controls, concurrent
topics, cancellation, unavailable modes and uncertain delivery have actual
HTTP/WebSocket/SQLite journeys in `tests/questions_io.py`. See
[native-questions.md](native-questions.md) for the public contract and limitations.

A disposable native Codex 0.160.0 qualification used the real App Server motor
with loopback Telegram HTTP: Codex generated a question, the bridge rendered its
choices, a synthetic authorized human chose one, and Codex continued with that
choice in the same completed native turn. This establishes native integration,
not a claim that the real human has tried the new Telegram controls.

The portable `telegram-codex` package remains one canonical directory in
[AlterMundi/Skills](https://github.com/AlterMundi/Skills/tree/main/skills/telegram-codex).
Its immutable source reference, archive hash, setup and recovery instructions
carry the qualified feature set to another Codex embodiment. Reading the skill
starts no listener. Installing it does not select a bot, copy another being's
identity/history or enroll anyone in Matrix. Hermes continues using its own
native gateway. Receiving acceptance belongs to each actual deployment.

## Next imports

Keep the next cycle small: choose one useful capability, cite its source, adapt
it to the receiving harness, qualify its changed boundaries, and propose the
focused upstream change. A dedicated native handback shortcut and configurable
tool/todo presentation can use the existing motor rather than a replacement
runtime. Broader watching and community analysis remain parent work; they do
not block this functioning increment. Generic skills and cross-being adoption
remain attributed choices with receiving acceptance.
