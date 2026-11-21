# Hairpin Havoc — original top-down pixel racer for Omarchy (Rust + Bevy 0.20)

## Objective
Build an original top-down pixel-art arcade racer for Omarchy in Rust/Bevy: circuits
with shortcuts and boost lanes, drift mini-turbo, nitro, lucky boxes, gadgets,
garage/upgrades, credits, time trial with ghosts and shareable ghost codes, and
procedural audio.

## Constraints
- Original identity: own name, tracks, cars, gadget names, palettes and art.
- Procedural art and audio (no external asset files except an OFL-licensed pixel font).
- Pure game logic lives in `src/core/` (no ECS systems) and is unit-tested; Bevy
  plugins in `src/game/` adapt it (hexagonal: core = domain, game = adapters).
- Runs on Omarchy (Arch + Hyprland/Wayland). Saves under `$XDG_DATA_HOME/hairpin-havoc/`.
- Bevy 0.20.1, features `2d`, `ui`, `audio`. Code against the local crate examples.

## Original content
- Tracks: Orchard Ring (BALANCED, countryside), Dune Sprint (FAST, desert),
  Neon Docks (TECHNICAL, night harbor), Glacier Pass (TECHNICAL, snow).
- Cars: Pebble, Sparrow (starters), Bandit, Mako, Tempest, Halcyon, Baron.
- Gadgets: Thumper (radial push), Slick (oil), Static Burst (area slow),
  Battering Boost (ram), All-Terrain (surface immunity), Phantom (no collisions),
  Drift Surge (2x drift charge). Box items: Slick, Thumper, Jammer.

## Mechanics reference (tuning targets, own values allowed)
- North-up camera, internal width ~480 px, integer scaling, velocity look-ahead.
- Fixed physics step 1/120 s. 5-gear box, bicycle steering, surface table
  (road/kerb/gravel/grass/wall), drift start rules, 3-tier drift charge, nitro tank.
- 4 cars per race (3 AI), 3 laps, sectors at 1/3 and 2/3, rubber-banding AI.
- Payouts by place, drift points → credits, PB bonus. Upgrades: engine/tires/exhaust x2.
- Ghost samples at 30 Hz; code = `HH1:` + base64url binary.

## Test policy
- TDD: off (no project/session configuration; source: default). Runner: `cargo test`.
- Checks per task: `cargo build`, `cargo test`, `cargo clippy -- -D warnings`
  (clippy once the crate compiles cleanly).

## Tasks
- [x] T1 App shell: states (Title/Garage/Race/Results), low-res render target with
      integer scaling, palette (Omarchy theme loader + fallback), window setup.
- [x] T2 Track core: geometry, curvature-based kerbs, gravel runoff, wall, branch
      routes (shortcut + boost lane) with progress remap, boost pads, 4 track defs.
- [x] T3 Track rendering: band meshes, kerbs, start grid, props, minimap.
- [x] T4 Car physics core (pure): gearbox, steering, drift, charge tiers, nitro,
      surfaces, wall response, car-car collision + spinout; unit tests.
- [x] T5 Player car in Bevy: input, procedural car sprites, camera, skid/smoke FX.
- [x] T6 Race flow: countdown, laps/sectors, standings, finish banner, results.
- [x] T7 AI rivals: centerline follow, corner speed, overtaking lane, rubber band,
      stuck recovery, nitro/handbrake use.
- [x] T8 Lucky boxes + gadgets + items.
- [x] T9 HUD: position, laps, timers, PB delta, nitro, gadget pod, gear/speed,
      drift bar, toasts, minimap.
- [x] T10 Garage + economy + persistence: cars, upgrades, paints, gadget shop,
      payouts, save file.
- [x] T11 Time trial + ghosts: recording, playback, PB, ghost codes, clipboard.
- [x] T12 Audio: procedural engine synth per car + SFX.
- [x] T13 Omarchy integration: .desktop entry, install target, README.
- [x] T14 Sprite-stacked 3D-looking cars: procedural voxel car models sliced into layers,
      stacked rendering (rotate all slices, lift 1 px each, vertical squash), ghosts and
      phantom fade on stacks, stacked previews in menus, and drop-in slice strips exported
      from the SpriteCook sprite stack generator (`assets/cars/<car-key>.png` + optional toml).
- [x] T15 Stack quality pass: bake each car into N pre-rendered headings at load (symmetric
      `(w-1)/2` rotation centre, rounded nearest sampling), one outline around the composite
      (no per-slice outlines), single sprite per car picking the nearest heading frame,
      `facing = "down"` default for imported strips, squash as one tunable constant, and
      document `views_to_slices.py` (MIT) as an offline way to make slice strips.

## Progress
- All tasks T1–T13 implemented and committed on `feat/hairpin-havoc`.
- Checks (observed): `cargo test` 130 passed; `cargo clippy --all-targets -- -D warnings` clean;
  integrated autodrive race screenshot shows HUD, minimap, items, FX, kerbs and props.
- Decisions: gadgets disabled in Time Trial (comparable ghost times); clipboard via
  `wl-copy`/`wl-paste` (bevy `system_clipboard` feature is off); `.atl/` untracked.
- Risk assessment over the branch: medium, no review due (RDD entry rule not met).
- T14 (observed): `cargo build` ok; `cargo test` 143 passed; `cargo clippy --all-targets -- -D warnings`
  clean. Screenshots: autodrive race (stacked, lit cars with shadows, y-sorted), title and spinning
  garage previews, translucent ghost + phantom (composited turn images), forced boost flames at
  the rear mid-stack, and a drop-in `pebble.png` + sidecar override loaded via `XDG_DATA_HOME`.
  New `src/game/stack.rs`; `src/game/sprites.rs` removed.
- T15 (observed): `cargo build` ok; `cargo test` 150 passed (+1 ignored contact-sheet renderer);
  `cargo clippy --all-targets -- -D warnings` clean. Cars are baked at load into `HEADINGS = 128`
  frames (~16 ms and ~2 MB per car; 64 would step the nose ~1.8 px per frame) plus baked shadows,
  drawn as one sprite each; per-slice outline removed, one outline traced per composite; symmetric
  `(w-1)/2` centre with rounded nearest sampling (odd slices sampled as padded to even, matching the
  even canvas); `SQUASH = 0.88` kept after a 0.75 vs 0.88 contact sheet (0.75 looked stubby);
  imported strips default to `facing = "down"`. Screenshots: autodrive race, time-trial ghost
  (single translucent sprite) with a nose-down drop-in strip, garage and title previews.
- Not yet verified by a human: keyboard play, feel/tuning, audio by ear, garage purchases.

- Play-test (user): stacked 3D cars approved; keyboard steering was too twitchy → softened
  (STEER_GAIN 0.45, ramp 5/s, 55% steering at top speed); user confirmed "much better".

## Next step
Human play-test (`make run` or `make install`), then tuning: AI skid-mark density,
drift feel, AI pace per track.
