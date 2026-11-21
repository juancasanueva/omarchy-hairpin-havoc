# Custom car art

Cars in Hairpin Havoc are sprite stacks: a car is a pile of horizontal slices, ground
slice first. The game rotates every slice by the car's heading, squashes the pile a little
and lifts each slice 1 px above the one below, so the stack reads as a solid 3D car.
When a race starts, every car is pre-rendered at 128 headings with one dark outline traced
around the finished car, plus a matching ground shadow.

By default every car is a procedural voxel model. You can replace any car with your own
slice strip, for example one exported from the
[SpriteCook sprite stack generator](https://www.spritecook.ai/sprite-stack-generator).

## Make a car

1. Draw or generate reference views of the car at the same scale, on a transparent
   background: top, side, front and back.
2. Drop the image into the sprite stack generator and export the slices. The export is
   one PNG row of slices with the ground slice on the left and the nose pointing down,
   which is what the game expects by default. Turn the outline toggle off: the game draws
   its own outline around the whole car, and per-slice outlines show up as dark seams and
   speckles inside the body.
3. Save the strip as `<car-key>.png`, using one of these keys:

   | Car | Key |
   |-----|-----|
   | Pebble | `pebble` |
   | Sparrow | `sparrow` |
   | Bandit | `bandit` |
   | Mako | `mako` |
   | Tempest | `tempest` |
   | Halcyon | `halcyon` |
   | Baron | `baron` |

4. Put the file in the first of these folders that suits you (the game searches them in
   this order and uses the first match):
   1. `$XDG_DATA_HOME/hairpin-havoc/cars/`
   2. `~/.local/share/hairpin-havoc/cars/`
   3. `assets/cars/` next to the game executable
   4. `assets/cars/` in the current directory (handy with `cargo run`)

The art is loaded when a race, the title screen or the garage preview starts, so restart
the race or reopen the menu to see changes.

Aim for the size of the built-in cars: about 28–35 px long, 16–18 px wide and 10–14
slices tall. Bigger art works too; shrink it with `scale` (see below).

## Sidecar options

An optional `<car-key>.toml` next to the PNG tunes how the strip is read:

```toml
slices = 12        # slices in the strip; default: square slices (slice width = image height)
facing = "down"    # where the nose points inside one slice: "down" (default), "up", "left", "right"
scale = 0.5        # world pixels per art pixel; default 1.0
```

Fully transparent slices at the end of the strip are ignored. The image width must be an
exact multiple of the slice count.

## Make slices offline

If you have reference views but no slicing tool, the MIT-licensed `sprite-stacking` agent
skill by danielwh2 (<https://github.com/danielwh2/sprite-stacking>) ships
`views_to_slices.py`. It carves a 4-view sheet (top view with the front pointing down, side
view with the front pointing left, front view and back view, all at the same scale) into a
slice strip:

```sh
python3 views_to_slices.py views.png <car-key>.png --slices 16 --top-nose down --preview check.png
```

Its strips have the nose pointing down and the ground slice on the left, which is the
game's default, so no sidecar is needed beyond `slices` when the slices are not square.
Open `check.png` to see the stacked result before you drop the strip into a cars folder.
The script is not bundled with the game.

## Notes

- Paint jobs from the garage do not tint custom art; the car always shows its own colors.
- Invalid files (unreadable PNG, a width that does not divide into the slice count, a bad
  sidecar) log a warning and the game falls back to the procedural car.
