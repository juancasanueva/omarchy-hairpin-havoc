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
- [ ] T1 Core: five track ids, keys, layouts with both routes + stable-order test update.
- [ ] T2 Game: five world themes, README.
- [ ] T3 Verify: tests, clippy, rendered layouts checked, in-app track select and a race.
- [ ] T4 User play-test.

## Progress
Created 2026-11-23.

## Next step
T1.
