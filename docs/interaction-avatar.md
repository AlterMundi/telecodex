# Conversation companion

The standalone conversation keeps its portrait decorative and its operational
indicators readable as ordinary HTML. The original compact character draws on
the orb, hub and character-built vocabulary of **ASCII-Interfaces**, created by
**Ani and Oliva** for Nexus. The source examined was revision
`4e7d6c0092fb1bf4c6d8909ff1026a1642b35646` (MIT, copyright 2026
ASCII-Interfaces contributors). No private case, media, renderer or existing
portrait was copied. The maintained portrait and finite gestures live in
`dashboard/web/companion.js`.

## Evidence and limits

- Microphone permission, recording, finalization and upload come from the actual
  local capture state. Playback preparation and speaking follow the owned player;
  muted source validation is not shown as speaking.
- The existing presentation response includes `activity` with schema
  `telecodex.interaction-activity/v1`. The receiver reads its own bookkeeping for
  the integrating entry and up to 64 configured, still-owned session bindings.
  `busy` means receiver-observed session activity, not model reasoning, finished
  work, input incorporation or hardware execution. Other sessions outside the
  selected configuration are not inspected or exposed.
- Observer indicators count only `summarizing` attempts belonging to the current
  receiving instance and current owned native source. Old, uncertain, completed
  or replaced-source attempts cannot animate current observer work.
- Transcription/routing indicators derive from retained input/route phases;
  queued routing is labeled pending. Current question indicators use the native
  request broker. Multiple indicators can coexist.
- Receiver indications expire after 20 seconds without another successful
  activity observation. Missing fields from an older receiver are unknown, not
  idle. A failed connection clears the live-work claim. Local capture/playback
  remain independent of that observation.

There is no animation model, frame loop or additional poller. A new canonical
reply may produce one 550 ms local gesture; repeat polling, historical loading,
older responses and progress items do not restart it. Reduced-motion preference
and a hidden page cancel animation. Unchanged activity does not rebuild its live
region on each poll. No image/font/animation service is needed.

The client reserves a bounded optional `companion_hint.rest` enum on an actual
returned source. Invalid values are ignored, and an older reply cannot override
a newer pose. This is presentation only. **Controller authoring/registration of
those hints is not implemented yet**; current receivers use deterministic local
defaults. It must not be advertised as an available model operation until the
native-turn-bound registration and its denial/replay cases are implemented.

## Development qualification

`tests/companion_ui.js` covers concurrent evidence, unavailable/stale receiver
state, local capture independence, finite reply gestures, invalid/duplicate/old
hints, hidden view and reduced motion. Store presentation tests cover current
ownership, native-source replacement and receiving-instance scoping. Keep the
actual execution result separate from tests merely added to the tree.

The required final campaign remains Q17/Q18/V07 after the integrated version is
complete. Browser rendering, real native activity, receiving adoption and human
appearance/hearing observations are separate evidence.

### Development checkpoint — 2026-10-20

The companion/client suites and 26 HTTP grant/capture tests pass locally. On
revision `c66494a`, CI run `39350034029` completed the Rust formatting, Clippy,
unit-test and release-build steps successfully; the larger process journeys were
still running at this checkpoint. The local Rust qualification had previously
been canceled deliberately to serialize large builds under host memory pressure.

Chromium rendering was inspected at 390×844 and 1100×850 with synthetic retained
text/progress, dark fallback and simulated light `--dm-*` roles, plus emulated
reduced motion. No horizontal overflow or history/composer overlap was observed;
diagnostics remained initially collapsed. This browser used the older development
receiver binary without the new activity response and correctly showed activity
unconfirmed. It does not qualify new native activity end to end, an actual
Homemaker-owner update, an Android keyboard or the final real-session campaign.
