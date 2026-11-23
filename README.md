# Hairpin Havoc

A top-down pixel-art arcade racer for [Omarchy](https://omarchy.org), written in Rust with
[Bevy](https://bevyengine.org). Drift to charge mini-turbos, burn nitro on the straights,
grab lucky boxes, and fire gadgets at nine rivals across ten circuits — or chase your own
ghost in Time Trial. Menus follow your active Omarchy theme.

![Hairpin Havoc: the pack slides through the first corner of Ember Oval, leaving skid marks](docs/screenshot.png)

## Quick path

```bash
git clone https://github.com/juancasanueva/omarchy-hairpin-havoc.git
cd omarchy-hairpin-havoc
make install      # builds release and installs for the current user (~/.local)
hairpin-havoc     # or launch "Hairpin Havoc" from the Omarchy app launcher
```

Or grab the prebuilt x86_64 binary from the
[latest release](https://github.com/juancasanueva/omarchy-hairpin-havoc/releases/latest):

```bash
tar -xzf hairpin-havoc-v*-x86_64-linux.tar.gz
cp -r hairpin-havoc-v*-x86_64-linux/{bin,share} ~/.local/
```

Requirements: Rust (stable, only to build from source), Vulkan drivers, `alsa-lib`, `wayland`, `libxkbcommon`
(all present on a standard Omarchy install). Remove with `make uninstall`.

## Tour

### Race

| | |
|---|---|
| ![Ten cars leaving the starting grid on GO](docs/screenshots/grid-start.png) <br> **Ten-car grid.** You start mid-pack against nine rivals (wide camera, `V`). | ![Cars heading for two floating lucky boxes](docs/screenshots/lucky-boxes.png) <br> **Lucky boxes.** Drive through one for a Slick, Thumper or Jammer. |
| ![A car with orange boost flames and a Drift Boost message](docs/screenshots/drift-boost.png) <br> **Drift boost.** Hold a slide to charge it, release for a kick. | ![Two cars in line with blue nitro flames](docs/screenshots/nitro.png) <br> **Nitro.** Blue flames, and a tank that empties. |
| ![A shockwave ring spinning out three rivals](docs/screenshots/thumper.png) <br> **Thumper.** A shockwave that spins out everyone close by. | ![A car spinning out on an oil slick](docs/screenshots/slick.png) <br> **Slick.** Oil dropped behind you for whoever follows. |

### Ten circuits

| | |
|---|---|
| ![Orchard Ring, beside its boost lane](docs/screenshots/orchard-ring.png) <br> **Orchard Ring**, with a boost lane on the right | ![Dune Sprint, just past the start line](docs/screenshots/dune-sprint.png) <br> **Dune Sprint** |
| ![Neon Docks, cars drifting past containers](docs/screenshots/neon-docks.png) <br> **Neon Docks** | ![Glacier Pass, a hairpin in the snow](docs/screenshots/glacier-pass.png) <br> **Glacier Pass** |
| ![Ember Oval, the pack drifting through a wide curve](docs/screenshots/ember-oval.png) <br> **Ember Oval** | ![Maple Loop, cars drifting where the shortcut rejoins](docs/screenshots/maple-loop.png) <br> **Maple Loop** |
| ![Salt Flats, two cars passing the shortcut on the salt pan](docs/screenshots/salt-flats.png) <br> **Salt Flats** | ![Iron Harbor, the pack sweeping past rusty containers](docs/screenshots/iron-harbor.png) <br> **Iron Harbor** |
| ![Aurora Ridge, cars drifting through a snowy night curve](docs/screenshots/aurora-ridge.png) <br> **Aurora Ridge** | ![Jungle Twist, the pack leaving a kerbed bend](docs/screenshots/jungle-twist.png) <br> **Jungle Twist** |
| ![Track select screen showing the Neon Docks map](docs/screenshots/track-select.png) <br> **Track select**: circuit, mode and car | |

### Beyond the race

| | |
|---|---|
| ![Time Trial: the player chasing a translucent ghost car](docs/screenshots/time-trial.png) <br> **Time Trial.** Race your own ghost, with sector splits. | ![Garage with a rotating car preview, stats and prices](docs/screenshots/garage.png) <br> **Garage.** Spend credits on cars, upgrades, paint and gadgets. |

![The main menu in four Omarchy themes](docs/screenshots/themes.png)

**Your theme, everywhere.** The main menu in Tokyo Night, Gruvbox, Catppuccin Latte and Rosé Pine.

## Controls

The game opens on a main menu: Play (`P`), Garage (`G`), High Scores (`H`), Settings (`O`), About (`I`) and Quit (`Esc`).
Move with `↑` `↓` or `W` `S` and press `Enter`, or use the shortcut. Play opens track select.

Driving keys are the defaults; rebind them in Settings.

| Key | Action |
|-----|--------|
| `W` / `↑` | Throttle |
| `S` / `↓` | Brake, then reverse |
| `A` `D` / `←` `→` | Steer (last pressed wins) |
| `Space` | Handbrake — start a drift |
| `Shift` | Nitro |
| `E` / `Q` | Use held item, otherwise your gadget |
| `V` | Toggle wide camera |
| `R` | Restart (asks first) |
| `G` | Toggle ghost (Time Trial) |
| `Ctrl+V` | Load a friend's ghost code (Time Trial) |
| `M` | Mute |
| `Esc` | Back: a race (asks first) or its results → track select → main menu |

## How it plays

| Mechanic | Summary |
|----------|---------|
| Drift charge | Hold a slide on tarmac to fill three tiers; releasing gives a 0.45 / 0.75 / 1.05 s boost. |
| Nitro | A tank that refills while driving and drifting; fire it with `Shift`. |
| Surfaces | Road → kerb (corners only) → gravel → grass → barrier. Each costs grip and speed. |
| Routes | Every circuit has a risky shortcut and a boost lane with pads. |
| Lucky boxes | Three stations per lap: Slick, Thumper or Jammer. |
| Gadgets | Thumper, Slick, Static Burst, Battering Boost, All-Terrain, Phantom, Drift Surge. |
| Garage | Earn credits by placing, drifting and setting PBs; buy cars, upgrades, paints, gadgets. |
| Time Trial | Your best lap is saved as a ghost; share it as an `HH1:` code. |
| High Scores | Two views per track: best lap, and best race finish (position, then race time). Switch with `←` `→`. |

## Details

| Topic | Decision |
|-------|----------|
| Saves | `$XDG_DATA_HOME/hairpin-havoc/` (`save.toml`, `pb-<track>.txt` ghost codes). |
| Settings | Master, engine and effects volume, sound, fullscreen, pixel size, default race camera, and rebindable driving keys (each keeps a fixed alternate: arrows, right Shift, `Q`). Saved in `save.toml`. |
| Theme | UI colors come from `~/.local/state/omarchy/current/theme/colors.toml`. |
| Window | Wayland app id `hairpin-havoc`, usable in Hyprland window rules. |
| Art & audio | Procedural: tracks, engine sounds, voxel cars and boost flames are generated in code. |
| Cars | Sprite-stacked: each car is a stack of slices rotated and lifted 1 px apart, so it reads as 3D. Each car is pre-rendered at 128 headings with one outline when a race starts. |
| Props | Stacked 3D too: trees, pines, cacti, rocks, dock containers and the spinning lucky-box crates are voxel models baked once per variant and angle at race start, y-sorted with the cars by their ground point. |
| Custom car art | Drop a slice strip in `~/.local/share/hairpin-havoc/cars/<car-key>.png` — see [`assets/cars/README.md`](assets/cars/README.md). |
| Font | Press Start 2P, SIL Open Font License (`assets/fonts/OFL.txt`). |

## Development

```bash
make test         # unit tests (pure rules live in src/core)
make lint         # clippy, warnings as errors
HAIRPIN_AUTOSTART=race HAIRPIN_AUTODRIVE=1 cargo run   # skip menus, AI drives your car
```

`src/core/` holds the pure game rules (track geometry, physics, laps, AI, economy, ghosts,
saves). `src/game/` holds the Bevy plugins that render, play audio and read input.

## License

MIT, see [`LICENSE`](LICENSE). The bundled Press Start 2P font keeps its own SIL Open Font
License ([`assets/fonts/OFL.txt`](assets/fonts/OFL.txt)).
