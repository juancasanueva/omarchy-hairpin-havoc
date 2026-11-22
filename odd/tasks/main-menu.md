# Main menu — title screen with vertical options

## Objective
The game starts on a main menu: the game title and a vertical list (Play, Garage, Settings,
About, Quit), each row showing its keyboard shortcut in brackets.

## Problem / why
The title screen mixed the track picker, mode toggle, loadout and a footer of shortcuts. The user
asked for a menu system with visible shortcuts.

## Scope
- `AppState::Title` becomes the main menu. `Up`/`Down` and `W`/`S` move (wrapping), `Enter`
  selects, shortcuts act directly: Play `P`, Garage `G`, Settings `O`, About `I`, Quit `Esc`.
- New `AppState::TrackSelect` holds the former title content (track, mode, car). `Enter` starts
  the race, `Esc` returns to the main menu.
- `Esc` from Garage, Settings and About returns to the main menu (unchanged).
- `Esc` from a race or the results returns to track select (accepted by the user 2026-11-22).
- The highlight stays on the last chosen entry when returning to the menu.

## Out of scope
Mouse support, gamepad, pause menu, new art.

## Constraints
- Settings keeps `O`: `S` already moves the highlight down.
- Do not run `cargo fmt`; the repo is not rustfmt-default formatted.

## Test policy
- TDD: off (source: default, no project/session configuration). Runner: `cargo test`.
- Checks: `cargo test`, `cargo clippy --all-targets -- -D warnings`; user play-test.

## Tasks
- [x] S1 Move the track/mode/car screen to `track_select.rs` under `AppState::TrackSelect`;
      race and results `Esc` go there.
- [x] S2 Main menu in `title.rs` with entries, shortcuts, navigation + tests.
- [x] S3 README and dev autostart; tests, clippy, in-app check.
- [ ] S4 User play-test.

## Progress
Created 2026-11-22.
- S1-S3 done: `cargo test` 201 passed (5 new: menu order, shortcut uniqueness and hints,
  navigation, wrapping, Enter and shortcuts open the same screen); clippy clean.
- In-app check with injected keys and an isolated save: Down moved to Garage, `P` opened track
  select, `Enter` started a race, `Esc` went to track select, then the menu, then quit.
  Results `Esc`, Garage, Settings and About were not driven in the app.
- `HAIRPIN_AUTOSTART=tracks` added. Screenshots: `grim -T <stableId>` captures only the game
  window; keys go to it with `hl.dsp.send_shortcut({ mods, key, window = "pid:<pid>" })`.

## Next step
User play-test. Delivery (merge, release) is the user's call.
