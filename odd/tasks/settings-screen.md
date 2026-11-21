# Settings screen — volume, display, camera and key rebinding

## Objective
Add a Settings screen to Hairpin Havoc so players can tune audio, display and camera, and
rebind the driving keys, with every choice saved and applied on the next launch.

## Problem / why
Today volume is fixed at 70%, mute is the only audio control, the window is always windowed,
the pixel size is automatic only, and the driving keys are hard-coded. Settings are the
expected baseline of a finished game.

## Scope
- Audio: master, engine and effects volume (0–100% in 10% steps), sound on/off (same flag as `M`).
- Display: fullscreen on/off (borderless), pixel size Smaller / Auto / Larger.
- Camera: default Normal or Wide at race start (`V` still toggles during a race).
- Controls: rebind Throttle, Brake, Steer left, Steer right, Drift, Nitro, Use item. Each
  action keeps a primary (rebindable) and a secondary (default arrows/right-shift/Q) key.
  Reset controls to defaults.
- Entry from the title screen with `O`; `Esc` returns; changes apply live and persist.

## Out of scope
Music volume (no music yet), gamepad, rebinding menu/system keys (Esc, R, V, G, M, I, O, Enter).

## Constraints
- Pure settings model and binding rules live in `src/core/` (no Bevy types), unit-tested; the
  Bevy key-name mapping and screen live in `src/game/`.
- Old saves without settings must keep loading (serde defaults, lenient values).
- Reserved keys cannot be bound; binding a key already used by another action moves it.

## Test policy
- TDD: off (source: default, no project/session configuration). Runner: `cargo test`.
- Checks per task: `cargo test`, `cargo clippy --all-targets -- -D warnings`; screen capture
  via `HAIRPIN_AUTOSTART=settings` with a temporary `XDG_DATA_HOME`.

## Tasks
- [x] S1 Settings model in `src/core/settings.rs`: values, steps/clamping, defaults, actions and
      key-name bindings with assign (conflict move, reserved reject) and reset; `SaveData.settings`
      with defaults so old saves load; tests.
- [x] S2 Apply settings in the game: audio split volumes (engine/effects × master), fullscreen
      window mode, pixel-size offset in the canvas scale, default wide camera; bindings resource
      mapping key names to `KeyCode`; `race.rs` and `items.rs` read bindings; applied at startup
      and whenever the profile changes.
- [x] S3 Settings screen: `AppState::Settings`, rows with values, up/down select, left/right
      change, Enter toggles / starts key capture, "PRESS A KEY" capture with reserved feedback,
      reset row, `O` from title, `HAIRPIN_AUTOSTART=settings`, README controls.
- [x] S4 Verify: tests, clippy, screen capture, settings persist across a relaunch.

## Acceptance criteria
- Every setting changes live and survives a restart (written to `save.toml`).
- Rebinding Throttle to a new key drives the car with that key; reserved keys are refused.
- An old `save.toml` without `[settings]` loads with defaults.

## Progress
- S1 (observed): `src/core/settings.rs` (volumes, pixel size, camera, key bindings with swap/
  reserved/alt rules, sanitize) + `SaveData.settings` (lenient, old saves load defaults); 11 tests.
- S2 (observed): `src/game/controls.rs` (key name ↔ KeyCode table, `Controls`), `src/game/settings.rs`
  (`apply_settings` at startup and on profile change: audio engine/effects gains, borderless
  fullscreen, `CanvasZoom`, controls); `race.rs`/`items.rs` read bindings; HUD pod shows the bound
  use-item key; default wide camera from settings. 188 tests passed, clippy clean.
- S3 (observed): `AppState::Settings` screen (15 rows in AUDIO / DISPLAY / CONTROLS), `O` from the
  title, `HAIRPIN_AUTOSTART=settings`, key capture with reserved/alt feedback, reset controls,
  README controls and settings rows. Pure row logic tested (adjust, values, error text).
- S4 (observed): 192 tests passed, clippy clean. Screen capture with defaults; relaunch with a
  hand-written `save.toml` (30% master, larger pixels, wide camera, throttle I, item F) shows
  larger pixels and an `F` item key in the race HUD, and the settings screen lists those values.
  Not verified here (no synthetic keystrokes): live navigation, key capture and the fullscreen
  toggle — left for the user's play-test.

- Play-test (user): settings screen confirmed working ("it works"); the run exited cleanly and
  wrote a `[settings]` table to the real `save.toml`.

## Next step
Done. Optional: push `main` and cut a v1.1.0 release.
