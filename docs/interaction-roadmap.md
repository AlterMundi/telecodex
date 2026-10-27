# Daimon Interaction — full roadmap and continuous-voice delivery plan

This is the maintained product roadmap, not a claim that the later stages are
implemented. Human selection, 2026-10-26: move from the accepted single-chat base
to genuinely continuous voice, responsive foreground dialogue and the necessary
parallel-work attention coordination as one coherent delivery.

## Navigation and current selection

| Scope | Plan | Execution |
| --- | --- | --- |
| Full product vision | This document and [Matrix parent #313](https://github.com/AlterMundi/daimon-matrix/issues/313) | [Project14](https://github.com/orgs/AlterMundi/projects/14) |
| Accepted base and remaining v0.2 obligations | [v0.2 plan](interaction-v02.md) and [qualification protocol](interaction-qualification.md) | [Milestone8](https://github.com/AlterMundi/telecodex/milestone/8), [#208](https://github.com/AlterMundi/telecodex/issues/208) |
| Current v0.3 continuous-voice delivery | Executable plan below | [Milestone9](https://github.com/AlterMundi/telecodex/milestone/9), [#301](https://github.com/AlterMundi/telecodex/issues/301) |

Issues carry their owning milestone; milestone descriptions link back to the
relevant plan. A native goal is an execution continuation, not the only place
where the roadmap can be found. Update these existing records when scope changes;
do not create a competing plan in a new conversation.

## The complete horizon

1. **One conversation, many persistent jobs.** Independent authenticated app,
   text and voice, retained routing, intermediate/final returns, useful questions,
   steering, and continuity across app/Telegram/CLI. This is the human-accepted
   functional base; specific remaining obligations stay in milestone8.
2. **Genuinely continuous voice.** Recognize during speech, show revisable
   captions, stream useful output, handle pauses and endpointing, and interrupt
   speech without losing the conversation or cancelling work. Current file-ASR
   does not meet this stage. This is the next selected outcome.
3. **Responsive dialogue separated from long reasoning/work.** A fast foreground
   conversation remains available while integrator/reasoner/native workers run.
   Reuse the persistent task registry, native sessions and versioned observer.
   Architectural separation does not authorize an unrequested model change.
4. **Attention and coordination.** Order simultaneous progress, results and
   questions; track what was actually heard; avoid overlapping voices and stale
   answers; let corrections reach the right ongoing work. Implement the part
   needed for stages2/3 now, retain broader scheduling work in the roadmap.
5. **Continuity across clients and bodies.** Carry the same authored conversation,
   permissions and useful context across devices and authorized embodiments,
   including cross-body work. Cross-body Weave/sync implementation belongs to
   the Matrix workstream; this goal does not duplicate it.
6. **Situated embodiments.** Native mobile, physical/Bluetooth badge, selected
   camera/visual and gesture interaction, appropriate local audio handling.
   Existing selected images do not mean continuous camera/video is enabled.
7. **Full individual and collective daimon experience.** Evolving per-being
   visual/sonic expression, appropriate memory and personal continuity,
   multi-human/tribe collaboration and accessible profiles. Preserve distinct
   identities, audiences and authority while expanding the experience.

Stages2/3/4 are being developed together where they depend on each other. Stages5–7
remain the visible horizon, not hidden prerequisites or completed features.

## Evidence and research used for architecture selection

The updated Collective Research speech frontier and integrating-conversation
studies guide experiments. Keep their audience restrictions; do not publish the
private corpus. Recheck selected provider documentation and actual receiving
capabilities before implementation. Relevant public primary sources include
[Agents Thinking Fast and Slow](https://arxiv.org/html/2410.08328v1),
[LiveKit delegation](https://docs.livekit.io/agents/logic/patterns/subagent-delegation/),
[Kyutai Unmute](https://github.com/kyutai-labs/unmute),
[Qwen streaming ASR client events](https://www.alibabacloud.com/help/en/model-studio/qwen-asr-realtime-client-events)
and [server events](https://www.alibabacloud.com/help/en/model-studio/qwen-asr-realtime-server-events).
These establish documented designs/contracts, not an independent performance
ranking or this account's entitlement. Native account WebRTC previously returned
synthetic audio, but that alone did not qualify backing-work admission, real
microphone/speaker use or the full production journey.

## Current v0.3 executable plan

## Outcome
From the human-accepted functional app, deliver genuinely continuous voice and a more responsive conversation: recognize speech while the person speaks, begin useful speech promptly, allow interruption, and keep long native jobs progressing with coherent intermediate/final communication. Implement roadmap stage 2 together with the useful separation from stage 3 and the attention coordination required from stage 4. A streaming demo that weakens routing, recovery or ordinary chat is not the outcome.

Baseline: immutable `interaction-functional-sol-2026-10-26`, web/scripts source2917d8ee and native source6eff7b4. Preserve the earlier baseline tag. Committed ASR remains Groq final-file transcription; Qwen API supplies TTS. Revisable live captions and progressive spoken output are receiving-enabled. Complete continuous voice/full duplex and measured responsiveness remain unfinished. Parent: AlterMundi/daimon-matrix#313; Project14; previous integrated milestone: #208. PR300/#263 ordered-image support is now receiving-deployed with native sourced6667ba8 and web source884f884. It remains outside the earlier immutable tagged baseline.

## Architecture and execution sequence
1. **Inspect and measure the existing path.** Reuse current PWA/shared auth, fetch-SSE events, player, ingress journal, observer, native session routing and results. Record actual capture/final-speech, partial/final ASR, commit, first useful model text, first playable sound and interruption-to-silence times on the same journey. Existing accepted behavior needs no generic repeat test. Consult the updated Collective Research speech frontier and prior native WebRTC/Talker-Reasoner evidence. That evidence is not a provider benchmark or a production entitlement.
2. **Choose the smallest complete streaming path.** Compare the already exercised native account WebRTC route with modular streaming ASR + foreground response + incremental TTS. Inspect supported native admission/handoff behavior before choosing: a realtime connection must not silently start unintended backing work or suppress useful returns. Recheck selected primary provider docs, existing credentials, Spanish/English support, actual capacity and grants. Start with existing resources; do not assume a free GPU or buy a service. Preserve the current text/file-ASR fallback.
3. **Streaming input and durable utterance commits.** Send bounded audio chunks while recording; render revisable transcript deltas promptly. Partial hypotheses are display/preparation data, never executable human instructions. Retain utterance identity and ordering; explicit Send commits exactly one original. An optional conversational endpointing mode must be explicit and handle pauses/corrections; retain push-to-talk. Discard never dispatches. Reconnect observes the existing outcome, without blind re-transcription or native replay. Preserve selected image/text context and transient raw-media retention with durable transcripts.
4. **Responsive foreground separated from long work.** Keep gpt-6.1-sol / openai / medium as the selected conversational model and native jobs intact. Separate rapid conversational handling from long integrator/reasoner/work execution using supported existing seams. Do not reinstate the rejected Qwen conversational model or silently change every session's effort/model. If a genuinely different foreground model is needed, present the measured tradeoff and obtain the human's model choice. Stream useful response text and playable speech; do not manufacture filler to improve a latency metric.
5. **Duplex and attention.** Barge-in fades/stops audible output immediately, preserves what was heard and unsaid, and lets the person correct/continue. It does not cancel native jobs. Coordinate simultaneous session updates, questions and foreground turns; useful intermediate/final results remain visible, ordered, attributed and deduplicated across app/Telegram/CLI. Avoid overlapping voices and stale spoken answers. Keep bounded context and versioned observer freshness; no mandatory extra LLM rewrite of every callback. Preserve explicit goal controls and direct-topic ingress boundaries.
6. **Deliver and qualify one coherent experience.** Keep the existing interface, Follow, accessible controls, ASCII and click-free Beacon/ducking working. Finish the relevant pending fixes rather than rebuilding accepted functionality. Ship progressively behind the existing configuration path with an actual rollback to the tagged baseline; once new durable formats exist, rollback must retain a compatible reader. Publish current source/deployment/health/operator guidance through existing shared records.

## Acceptance and evidence
- Streaming captions appear during speech; one committed original routes once with its context. Finalization and provider failure are visible and recoverable without losing or duplicating input.
- Useful conversational response is measurably faster than the tagged baseline on equivalent short Spanish/mixed-English journeys. Target under one second from final intended speech to first intelligible sound for simple turns where the chosen stack supports it; report actual distributions and conditions, never promise provider latency or count filler as success.
- Long speech, pauses, self-corrections, barge-in during playback, noisy/echo conditions, background/foreground transitions, network loss and late worker returns retain their meaning. Test automatic mechanics with synthetic/delayed workers and bounded fixtures; use real providers only for uncertainties they can resolve.
- Speak while a long native task and another question/result are pending; interrupt, answer/correct, reconnect and finish the conversation without visiting session topics. Stop audio and cancel work remain different actions. No unseen concurrent dispatch, duplicate result, lost draft, capture or answer.
- Measure partial/final ASR, final speech-to-useful audio, inter-segment gaps and barge-in-to-silence separately. Track the heard portion. Real phone/hearing acceptance is separate from digital evidence; do not reopen the obsolete generic testing request already answered.

## Existing work and limits
Reuse #271 for streaming ASR/latency/recovery, #278 for speech providers/voice character and Voice Lab, #297 for shared backend goal controls, #61 for pending, and #208 for remaining integrated experience. Retain the receiving-deployed PR300 image-array capability and finish any remaining #263 acceptance and v0.2 polish; cosmetic work is not a prerequisite to exploring continuous voice. Internationalization relevance remains #295. Native mobile/badge/video, cross-body Weave and the full later roadmap stay future work unless needed and explicitly selected. No global Stop, new hardware capture, background peer attention, credential uploads or uncertain input replay.

Progress and remaining gaps belong here and in the linked issues. Complete this milestone only after the integrated receiving experience is delivered, with its real limitations recorded.

## Delivery tasks

- [ ] #302: select transport and qualify architecture
- [ ] #271: streaming ASR, latency and retained-input recovery
- [ ] #303: responsive foreground with persistent native work
- [ ] #304: interruption, heard speech and concurrent attention
- [ ] #278: speech provider/voice qualification
- [ ] Integrated receiving delivery and current operator guidance
