# Race confirm — ask before restarting or quitting a race

## Objective
Pressing `R` or `Esc` during a race by accident no longer throws the race away.

## Problem / why
`R` restarts and `Esc` leaves to track select immediately, without asking.

## Scope
- `R` and `Esc` open a confirmation dialog on the race screen (races and Time Trial, from the
  countdown to the results).
- The race is paused while the dialog is open (physics, clock, effects, engine sound).
- "Keep racing" is selected by default; `Esc` closes the dialog; Enter picks the highlighted row.
- Other race keys (camera, gadget, ghost keys) do nothing while the dialog is open. Mute still works.

## Out of scope
A general pause menu; confirmations on other screens; shortcuts that confirm in one key.

## Constraints
- Confirming must need `Enter`, which cannot be bound to a driving action.
- Do not run `cargo fmt`. Never inject keystrokes for testing.

## Test policy
- TDD: off (source: default, no project/session configuration). Runner: `cargo test`.
- Checks: `cargo test`, `cargo clippy --all-targets -- -D warnings`; user play-test.

## Tasks
- [x] C1 Dialog model, input and pause in `src/game/confirm.rs`, opened from `race_keys`, + tests.
- [x] C2 Dialog UI, muted engine, other race keys gated, README controls.
- [x] C3 Verify: tests, clippy, in-app check of the dialog and the pause.
- [x] C4 User play-test (passed 2026-11-22).

## Progress
Created 2026-11-22.
- `cargo test` 224 passed (8 new); clippy clean.
- In-app check with a throwaway build that presses keys inside its own process (temporary save,
  hidden workspace): clock and player position frozen and engine off while the dialog was open;
  `V` and `E` ignored; `Esc` and Enter on "Keep racing" resumed; confirming restarted from the
  countdown and quit to track select.
- Pause: `sync_pause` pauses `Time<Virtual>`, which drives `FixedUpdate` and every race animation.

- Native review (reliability lens) approved and acknowledged; three optional suggestions left: text
  during the finish hold, README wording, tests for the gadget, ghost and engine gating.

## Next step
Feature complete. Released as v1.7.0 at the user's request.
