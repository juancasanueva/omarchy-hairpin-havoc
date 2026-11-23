# High scores — best finish and times per track

## Objective
A HIGH SCORES screen from the main menu lists, for every track, the best race finish (position
and race time of that finish) and the best lap.

## Problem / why
The save only keeps the best lap per track, shown one track at a time on track select. There is
no record of finishing positions and no overview of all tracks.

## Scope
- Save a best race result per track: finishing position plus total race time. Better position
  wins; a tie goes to the faster race time. Only finished Race-mode runs count.
- Results screen marks a new best finish.
- New HIGH SCORES screen (main menu entry, shortcut `H`) with two views (user request
  2026-11-23): BEST LAPS (track, best lap) and BEST FINISHES (track, position, race time).
  `←` `→` / `A` `D` / `Tab` switch views; `--` where nothing is recorded; `Esc` returns.
- README menu line and screen table.

## Out of scope
Leaderboards with several entries per track, online scores, resetting scores.

## Constraints
- Old saves keep loading; the new field defaults to empty and malformed entries are dropped.
- Do not run `cargo fmt`. Never inject keystrokes for testing.

## Test policy
- TDD: off (source: default, no project/session configuration). Runner: `cargo test`.
- Checks: `cargo test`, `cargo clippy --all-targets -- -D warnings`; user play-test.

## Tasks
- [x] H1 `BestRace` in `src/core/save.rs`: field, `best_race`, `submit_race`, sanitize + tests.
- [x] H2 Results screen records the best finish in Race mode and shows "NEW BEST FINISH".
- [x] H3 `src/game/high_scores.rs` two-view screen, `AppState::HighScores`, menu entry `H`,
  autostart `scores` + tests.
- [x] H4 README; verify: tests, clippy, in-app look at the screen.
- [ ] H5 User play-test.

## Progress
Created 2026-11-23 on branch `feat/high-scores`.
- `cargo test` 234 passed (8 new); clippy clean.
- In-app check from a throwaway worktree on a hidden workspace with a temporary seeded save:
  screenshots of both views and the six-entry main menu render correctly (no overlap).

## Next step
H5: user play-test, then commit.
