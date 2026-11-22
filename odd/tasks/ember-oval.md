# Ember Oval — a fifth, oval track

## Objective
Add an oval circuit, Ember Oval, as the fifth track.

## Problem / why
The user asked for a new track that is an oval. All current tracks are winding circuits.

## Scope
- Two long straights joined by two even 180-degree turns, `FAST`, about 80 px half-width.
- Both side routes, as on every other track (chosen by the user 2026-11-22): a narrow infield
  shortcut inside one turn and a boost lane outside the other.
- Dusk world theme reusing the cactus and rock models.
- Appended last in `TrackId::ALL`: saves and ghost codes store the track by position.

## Out of scope
New prop models, AI changes, track editor, banking physics.

## Constraints
- Wall offset must stay within the surface lookup reach (2 cells of 96 px).
- Turn radius about 400 px so the corner speed is below top speed and kerbs appear.
- Do not run `cargo fmt`. Never inject keystrokes for testing.

## Test policy
- TDD: off (source: default, no project/session configuration). Runner: `cargo test`.
- Checks: `cargo test`, `cargo clippy --all-targets -- -D warnings`; user play-test.

## Tasks
- [x] S1 Core: track id, key, layout with both routes, balanced against the other tracks + tests.
- [x] S2 Game: world theme, track-count assumptions, README.
- [x] S3 Verify: tests, clippy, in-app screenshots and an autodriven race.
- [ ] S4 User play-test.

## Progress
Created 2026-11-22.
- Layout: straights of 3400 px, turn radius 400, half-width 80, lap 9311 px. Shortcut is 0.80 of
  the distance it replaces (52 degree entry and exit, like Orchard Ring); boost lane is 1.68.
  A wider 700 px turn was dropped: it could be taken flat out and had no kerbs.
- `cargo test` 205 passed (4 new: stable track order, wall lookup reach, oval shape, route
  ratios); clippy clean. The ghost test for unknown tracks no longer hardcodes the track count.
- AI laps alone: about 20 s, never off the road. In-app check with a temporary save, autostart
  and autodrive: track select, 10-car race to the results (best lap 19.34, PB saved) and a time
  trial. The shortcut and the boost lane were seen but not driven (the AI does not use them).
- The spinning cars off the line also happen on Dune Sprint; not specific to this track.
- Known: the preview and minimap show a long, thin oval (about 5:1); the road swings about 1 px
  wide just before each turn (spline transition).

## Next step
User play-test. Delivery (merge, release) is the user's call.
