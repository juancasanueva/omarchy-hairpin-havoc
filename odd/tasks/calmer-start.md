# Calmer start — fewer spins right after GO

## Objective
Cars no longer pile up and spin in the first seconds of a race.

## Problem / why
With ten cars, several spin and leave the road right after GO. Spins there come from
`car::collide` (rivals have no gadgets). Every rival aims for the center line at once while the
cars accelerate at different rates.

## Scope
- Rivals hold their grid lane for the first couple of seconds, then merge gradually.
- For about 3 seconds after GO, contact bumps cars but does not spin them. Ramming with
  Battering Boost is exempt and still spins.
- Added during S2 (measured need): rivals follow the car ahead early in the race, fully for 12 s
  and fading out by 20 s, so it does not change racing after the opening.
- Measure start spins on every track before and after.

## Out of scope
Rival gadgets, general AI overtaking, collision strength after the start, time trial.

## Constraints
- Pure rules in `src/core`; same physics after the grace period.
- Do not run `cargo fmt`. Never inject keystrokes for testing.

## Test policy
- TDD: off (source: default, no project/session configuration). Runner: `cargo test`.
- Checks: `cargo test`, `cargo clippy --all-targets -- -D warnings`; user play-test.

## Tasks
- [x] S1 Baseline: headless ten-car start measurement on every track.
- [x] S2 Core and game: lane hold, spin grace + tests.
- [x] S3 Verify: after-measurement, tests, clippy, in-app check.
- [ ] S4 User play-test.

## Progress
Created 2026-11-22.
- Headless measurement, ten AI cars, 16 jittered starts per track, mean spins per start:

  | Version | 0-3 s | 3-8 s | 8-20 s | 20-40 s | Total |
  |---|---|---|---|---|---|
  | Before | 7.9 | 7.7 | 5.0 | 2.8 | 23.4 |
  | Lane hold and grace only | 0.0 | 11.0 | 6.4 | 2.8 | 20.2 |
  | Final (with following) | 0.0 | 2.2 | 5.2 | 4.5 | 12.0 |

  Lane hold and grace alone moved the spins later, so following was added. Following all race
  measured 7.4 in total but changes overtaking; left out. Average distance in 40 s is within 2%
  of before on every track. Spins at 20-40 s rose from 2.8 to 4.5: the field stays closer.
- `cargo test` 210 passed (5 new); clippy clean. `the_field_gets_away_cleanly` fails (14.1 against
  a limit of 4) with the AI changes off. Battering Boost spins through `ram_hit`, untouched.
- In-app check (autostart, autodrive, temporary save) on Orchard Ring and Ember Oval: clean
  starts; contact and wide lines remain in Ember Oval's first turn.
- The measurement drives all ten cars by AI; a human player was not simulated.

## Next step
User play-test. Delivery (merge, release) is the user's call.
