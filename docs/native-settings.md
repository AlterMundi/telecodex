# Native Codex settings through Telegram

Telecodex uses the configured existing Codex executable, home, account and native
tools. A setting stored for a Telegram topic can override the native CLI default.
Changing a bridge default affects new topics; it does not erase saved choices.

| Setting | Bridge behavior | Change for an existing topic |
| --- | --- | --- |
| Web search | Defaults to `live`; explicitly sends the saved mode to native thread/turn requests. | `/search live`, `/search cached`, `/search off` |
| Model | An omitted `default_model` inherits native settings unless the topic has a saved model. | `/model` |
| Reasoning effort | An omitted `default_reasoning_effort` inherits native settings unless the topic has a saved effort. | `/think default` to inherit; `/think high` for an explicit choice |
| Fast service tier | A topic can save an explicit tier; no explicit tier is sent otherwise. | `/fast on` or `/fast off` |
| Approval and sandbox | Explicit bridge/topic settings are sent to native Codex. The tribal template uses the human-selected `never` and `danger-full-access` baseline. | `/approval` and `/sandbox` |
| Skills, apps, MCP and native tools | Discovered by the existing native harness. The bridge does not send a reduced feature list. Platform/account availability still matters. | Manage in the existing Codex setup. |
| Native memory and hooks | Remain controlled by native configuration and actual handlers. The bridge does not replace or enable them. | Evaluate the installed memory integration separately. |

The tribal template enables live search and full access without interactive
approval. Preserve deliberate receiver overrides and separate credentials,
identity, memory and topic histories. Installing a shared template starts no bot.

CLI slash commands are interface operations, not ordinary model prompts. The
bridge implements a subset and forwards some command-looking text to the model;
that forwarding does not establish native CLI-command parity. Native Plan mode
and pending questions remain tracked in [#10](https://github.com/AlterMundi/telecodex/issues/10).
UI affordances, comment streaming and topic history selection differ from the
TUI; those differences do not imply shell, search or reasoning tools are absent.

Native reference: [Codex configuration](https://learn.chatgpt.com/docs/config-file/config-reference).
Owning audit: [#14](https://github.com/AlterMundi/telecodex/issues/14).
