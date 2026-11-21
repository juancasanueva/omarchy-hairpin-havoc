# Hairpin Havoc

A top-down pixel-art arcade racer for [Omarchy](https://omarchy.org), written in Rust with
[Bevy](https://bevyengine.org). Drift to charge mini-turbos, burn nitro on the straights,
grab lucky boxes, and fire gadgets at nine rivals across four circuits — or chase your own
ghost in Time Trial. Menus follow your active Omarchy theme.

![Hairpin Havoc: a stacked 3D car firing nitro across the Orchard Ring start line](docs/screenshot.png)

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

## Controls

Driving keys are the defaults; rebind them in Settings (`O`).

| Key | Action |
|-----|--------|
| `W` / `↑` | Throttle |
| `S` / `↓` | Brake, then reverse |
| `A` `D` / `←` `→` | Steer (last pressed wins) |
| `Space` | Handbrake — start a drift |
| `Shift` | Nitro |
| `E` / `Q` | Use held item, otherwise your gadget |
| `V` | Toggle wide camera |
| `R` | Restart |
| `G` | Toggle ghost (Time Trial) |
| `Ctrl+V` | Load a friend's ghost code (Time Trial) |
| `M` | Mute |
| `O` | Settings (title screen) |
| `I` | About: version, source link, license (title screen) |
| `Esc` | Back to menu |

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
