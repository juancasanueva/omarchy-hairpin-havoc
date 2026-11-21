# Custom car art

Cars in Hairpin Havoc are sprite stacks: a car is a pile of horizontal slices, ground
slice first. The game rotates every slice by the car's heading, squashes the pile a little
and lifts each slice 1 px above the one below, so the stack reads as a solid 3D car.

By default every car is a procedural voxel model. You can replace any car with your own
slice strip, for example one exported from the
[SpriteCook sprite stack generator](https://www.spritecook.ai/sprite-stack-generator).

## Make a car

1. Draw or generate reference views of the car at the same scale, on a transparent
   background: top, side, front and back.
2. Drop the image into the sprite stack generator and export the slices. The export is
   one PNG row of slices with the ground slice on the left. You can leave the outline
   toggle on; a dark outline helps the car read on any track surface.
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
facing = "up"      # where the nose points inside one slice: "up" (default), "down", "left", "right"
scale = 0.5        # world pixels per art pixel; default 1.0
```

Fully transparent slices at the end of the strip are ignored.

## Notes

- Paint jobs from the garage do not tint custom art; the car always shows its own colors.
- Invalid files (unreadable PNG, a width that does not divide into the slice count, a bad
  sidecar) log a warning and the game falls back to the procedural car.
