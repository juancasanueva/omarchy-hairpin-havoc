# Ten-car field — grow races from 4 to 10 cars

## Objective
Races start with 10 cars (the player plus 9 AI rivals) instead of 4.

## Problem / why
A 4-car field feels empty; the user asked for about 10 cars.

## Scope
- Field of 10 in Race mode (time trial stays solo). Player starts mid-pack (P6).
- Only 7 cars exist, so repeated rivals get a different paint and a " II" name suffix.
- Grid boxes painted for every slot; grid spacing tightened so the field fits.
- Per-slot z offset compressed so 10 cars on one row never sort past props/boxes.
- Payouts cover places 1–10.

## Out of scope
Configurable field size, new car models, AI tuning beyond existing personalities.

## Constraints
- `car_z` slot offsets must stay below `PROP_SORT` (0.5 step) minus `ATTACH_Z`.
- Existing saves unaffected (no save format change).

## Test policy
- TDD: off (source: default, no project/session configuration). Runner: `cargo test`.
- Checks: `cargo test`, `cargo clippy --all-targets -- -D warnings`; user play-test.

## Tasks
- [x] S1 Core: `FIELD_SIZE`/`PLAYER_GRID_SLOT` constants, 10-place payout table + tests.
- [x] S2 Game: spawn 9 rivals (distinct paint/name for repeats), grid spacing and boxes,
      `car_z` compression + test, snapshot fallback place.
- [x] S3 Verify: tests, clippy (green), play-test by the user (passed 2026-11-21).

## Progress
Created 2026-11-21.
- S1/S2 done: `cargo test` 196 passed (4 new: payout order, field fill, repeat rivals, z-sort
  bound); `cargo clippy --all-targets -- -D warnings` clean.
- Payouts: 260/210/170/140/115/95/80/70/65/60. Grid spacing 46 -> 36 px per slot.
- Note: do not run `cargo fmt` here; the repo is not rustfmt-default formatted.
- Screenshot attempt via grim captured the wrong window; visual check left to the user.

- Play-test 2026-11-21: user confirmed "it works" (grid, race and results with 10 cars).

## Next step
Feature complete. Delivery (merge, release) is the user's call.
