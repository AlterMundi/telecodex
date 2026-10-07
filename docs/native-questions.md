# Native planning and pending questions in Telegram

`/plan [prompt]` selects native Plan mode for the topic's next turn. `/default
[prompt]` selects native Default mode. The optional prompt queues a new turn;
it does not change a running turn's mode. Without a prompt these commands only
save the selection. `/status` shows that selection. An unset selection means
the bridge does not override the native mode, including changes made in the CLI.

The bridge initializes the experimental App Server capability and checks
`collaborationMode/list` before starting a selected mode. If the mode is not
advertised, it reports the failure without submitting a turn. It sends the real
`turn/start.collaborationMode`, using the resolved thread model and
`developer_instructions: null` for the native mode's built-in instructions.
Adding the word “plan” to a normal prompt is not a substitute for this contract.

## Answering and reopening questions

Native `item/tool/requestUserInput` requests appear as Telegram questions with
suggested choices, their complete descriptions and a free-text control. Tap a
choice, reply directly to the question, or tap **Write answer** / **Other answer**
before sending text. Ordinary unquoted text otherwise continues to steer the
active turn. Commands and attachments retain their ordinary handling. `/stop`
and **Cancel turn** interrupt the native turn; they do not fabricate an answer.

`/questions` lists the requesting human's pending questions in the current topic,
similar to reopening the CLI's pending-question interface. Choosing a question
posts its current step again. Controls on the older copy expire. A request can
contain up to three questions; choices and text are assembled into one native
response keyed by the original question IDs. Long questions are split without
dropping content; reply to a current part or use the last part's buttons.

The RPC receiver keeps reading while a human considers a question. Nonblocking
native requests can coexist with continuing commentary and work. Responses are
sent to the original JSON-RPC request, never as new prompts or model turns.

## Correlation and recovery

Controls bind the requesting authorized human, chat, topic, displayed message,
native thread, accepted turn, request and question. Opaque short callback tokens
keep native IDs and answer text out of Telegram callback data. Duplicate,
expired, wrong-user and wrong-topic controls cannot answer another request.
Foreign native events and duplicate request delivery are ignored.

Question-message receipts and answer admission are durable SQLite records.
Native request resolution is a closure receipt, not proof of how a choice
influenced the model. Completion settles admitted input; disconnects or missing
resolution leave its outcome undetermined. Restart does not replay an answer or
reconstruct an expired response channel. Quoted replies to old questions are
rejected without creating turns, including the gap where Telegram displayed a
question but its receipt had not yet reached the database. `/questions` explains
when there are no live questions, and `/status` preserves uncertain inputs.

Secret input and unsupported question shapes fail visibly rather than asking
for secrets in ordinary Telegram messages. Other native server-request kinds
are tracked separately in [#18](https://github.com/AlterMundi/telecodex/issues/18).

## Qualification and native handoff

`python3 -B tests/questions_io.py target/release/telecodex` exercises real
loopback HTTP, WebSocket framing and SQLite: multistep answers, reopening,
steering, asynchronous continuation, integer/string IDs, long Unicode text,
two simultaneous topics, mode transitions, stop/cancel, restart, lost receipt,
disconnect, unavailable modes and mismatched Telegram delivery receipts.
Synthetic qualification does not establish human Telegram acceptance.

The bridge keeps the native thread binding. After the turn completes, continue
it with the existing `codex resume THREAD_ID` shown by the binding in `/status`;
the CLI owns its own UI and reads that native history. No history copying or
second memory backend is introduced. A bridge upgrade waits for idle and
preserves bindings, accepted input and polling offsets. Roll back the binary,
not the conversation database.

Contract baseline: native Codex `rust-v0.160.0`, generated
`ToolRequestUserInputParams` and
[official App Server documentation](https://learn.chatgpt.com/docs/app-server).
Owning change: [Telecodex #10](https://github.com/AlterMundi/telecodex/issues/10),
parent [Matrix #262](https://github.com/AlterMundi/daimon-matrix/issues/262).
