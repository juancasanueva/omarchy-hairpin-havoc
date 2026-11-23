# Ten tracks — five new circuits

## Objective
Grow the track list from five to ten circuits.

## Problem / why
The user asked for five more tracks so the game has ten in total.

## Scope
- Five new circuits, each with its own name, difficulty, layout and world theme:
  Maple Loop (BALANCED, autumn trees), Salt Flats (FAST, pale salt pan with cacti),
  Iron Harbor (TECHNICAL, rusty containers), Aurora Ridge (BALANCED, night snow with pines),
  Jungle Twist (TECHNICAL, deep green trees).
- Both side routes on every new track (a narrow shortcut and a boost lane with two pads), like
  the existing five.
- Start/finish on a long straight at control point 0, driven counterclockwise like the others.
- Appended after Ember Oval in `TrackId::ALL`: saves and ghost codes store the track by position.
- README track count.

## Out of scope
New prop models, AI changes, physics changes, track select redesign.

## Constraints
- Every track passes the existing all-track tests: no main-loop overlap, walls within the
  surface lookup reach, branches leave the main road and run forward, start line is road,
  AI lap under 75 s, clean race starts, sort layer bounds, spin recovery.
- Do not run `cargo fmt`. Never inject keystrokes for testing.

## Test policy
- TDD: off (source: default, no project/session configuration). Runner: `cargo test`.
- Checks: `cargo test`, `cargo clippy --all-targets -- -D warnings`; user play-test.

## Tasks
- [x] T1 Core: five track ids, keys, layouts with both routes + stable-order test update.
- [x] T2 Game: five world themes, README.
- [x] T3 Verify: tests, clippy, rendered layouts checked, in-app track select and a race.
- [x] T4 User play-test (passed 2026-11-23).

## Progress
Created 2026-11-23.
- Layouts drafted, then checked with a throwaway surface-query renderer (removed). Fixes from the
  render: Maple Loop shortcut moved across the top of its hook (it ran parallel to the road),
  Iron Harbor boost lane moved to the top-left corner (it hugged the main road), Aurora Ridge
  boost lane widened (was 0.98 of the distance it replaces), Jungle Twist shortcut moved (it
  crossed the main road).
- Lap 12.0k-12.7k px. Shortcut ratios 0.77-0.88, boost lanes 1.27-1.70.
- AI lap alone: Maple 33.4 s, Salt 29.5 s, Iron 38.6 s, Aurora 40.1 s, Jungle 44.9 s
  (originals 22.9-39.0 s); off-road time in the same range as the originals.
- `cargo test` 224 passed; clippy clean.
- In-app (temporary save, autostart, autodrive, hidden workspace): a 10-car race on each new
  track and track select showing Aurora Ridge. README screenshots taken from those runs.
- Pre-existing, not changed: a surface query past a branch end reads as road along the branch
  tangent (no lengthwise clamp), so a thin strip beyond some branch ends is drivable. Seen on
  the original tracks too.
- Gentle AI review (reliability lens, user granted): approved with no findings, acknowledged.

## Next step
Feature complete. Released as v1.8.0 at the user's request.
