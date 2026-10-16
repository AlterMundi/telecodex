# Telegram topics in tmux

`codex-tg2tmux` opens a topic's **existing native conversation**, using its stored
`codex_thread_id`. Topic names label tmux sessions; names never select a native
conversation. The command is foreground and finite. It sends no prompt, starts no
model turn, resumes no goal, consumes no Telegram updates and changes no bridge
binding, journal or polling offset. Opening the TUI is separate from submitting input.

Requires Python 3.11+, tmux and the receiving body's configured Codex installation.
Keep `codex_tg2tmux.py` alongside `activity_indicator.py`. The latter supplies the
existing native WebSocket proxy transport; importing it starts no companion service.
Do not copy another body's configuration or history.

```sh
export TELECODEX_CONFIG=/path/to/your/telecodex.toml
python3 -B scripts/codex_tg2tmux.py list
python3 -B scripts/codex_tg2tmux.py open --topic 'Project name'
python3 -B scripts/codex_tg2tmux.py open --topic 123 --topic 456
python3 -B scripts/codex_tg2tmux.py open --all
python3 -B scripts/codex_tg2tmux.py sync-names --all
```

An installed `codex-tg2tmux` launcher can supply the receiving configuration and
script path. `--config` overrides the environment; `--chat` is required when the
store contains several chats. `--json` returns one metadata-only record per topic.
`list` is the default. `open` and `sync-names` require explicit selection.

## Naming and reconciliation

The bridge's explicit topic name is authoritative; a recorded Telegram title event
is the fallback. Text-derived `session_title` values remain **unverified summaries**.
`--all` selects established conversations with explicit or observed topic titles.
A specifically selected numeric topic may be opened under its unverified summary,
but `sync-names` refuses to promote that summary to a native name. First use the
existing `/rename Correct title` in that Telegram topic: it reconciles the topic,
bridge explicit name and native Codex name. Retry the same rename if the bridge
reports a partial result. Native-title-only edits do not establish Telegram truth.

`sync-names` uses supported `thread/name/set` and confirms with `thread/read`.
A missing native title or absent `session_index.jsonl` entry does not mean a lost
conversation. Availability is checked against the bound ID, without loading turns.
Missing/unavailable/fresh bindings and duplicate native IDs are preserved and
reported, never replaced by a fuzzy title match or a new conversation.

The installed Bot API exposes no list/read operation for forum topic names. The
command inventories the existing bridge store, not a live Telegram topic list.
Deleted topics, missed title updates and topic open/closed state cannot be inferred
reliably from this store's schema. `--all` means all eligible **known bindings**,
not a verified list of currently open Telegram topics. No second update consumer,
Telegram user-account client or background synchronizer is installed.

Tmux does not accept dots or colons in session names: these and control characters
become underscores (`dmn.homes` → `dmn_homes`). Spaces and Unicode remain. Duplicate
resulting names receive `[topic-id]`. The original title is retained in tmux
`@telegram_topic_title`. Existing managed sessions are reused and renamed by their
binding, without typing into their panes; unrelated name collisions are reported.
Managed tmux metadata includes the native ID and a hash of the local store/topic/ID.
If a topic changes its native binding, the old tmux session is preserved and any
name collision is reported. No existing user session is killed or repurposed.
Concurrent invocations of this tool share a local lock; this is not a bridge lock.

```sh
tmux list-sessions
tmux attach-session -t '=Project name'
# From inside tmux: Ctrl-b s to choose a session; Ctrl-b d to detach.
```

A managed session starts `codex resume UUID` in its configured working directory,
without a prompt. It attaches to native activity if the conversation is already
running. A paused goal remains paused. Existing tmux panes opened outside this tool
are preserved; they are not automatically adopted. If a managed TUI exits, its
session may disappear; rerun `open` to recreate it. No timer or rename watcher runs.
Each additional TUI uses resources; select only the topics needed on a small host.

Verification: `python3 -B tests/tg2tmux_io.py` covers metadata projection, explicit
name precedence, exact native identity, collision handling, real isolated tmux
creation/reuse/rename and argument quoting. It starts no native model turn.
