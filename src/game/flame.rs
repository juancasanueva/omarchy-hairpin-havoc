//! Stacked boost flames: a small voxel plume baked through the car compositor
//! at every heading, in two palettes (drift boost and nitro) and a few flicker
//! frames. Flames are emissive, so they get no lighting and no shadow, and a
//! dark rim in their own hue instead of the black outline.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;

use super::props::hash3;
use super::stack::{Grid, HEADINGS, SPACING, SQUASH, composite_stack, frame_angle, frame_index, outline, stack_canvas};

/// Slice length along the plume, in px. Even, so the slice centre sits on a
/// pixel corner like the car slices; the plume's base is the last column.
pub const FLAME_LEN: u32 = 16;
/// Slice width across the plume, in px (even, see `FLAME_LEN`).
const FLAME_WID: u32 = 6;
/// Slice count; the plume's axis is at `AXIS_Z`, so slice 0 stays empty and
/// lifts the flame off the ground to about exhaust height.
const FLAME_SLICES: usize = 7;
const AXIS_Z: f32 = 3.5;
/// Radius of the plume at its base, in px.
const BASE_RADIUS: f32 = 2.7;
/// Flicker frames per palette.
pub const FLICKER_FRAMES: usize = 3;
/// Plume length per flicker frame, in px.
const BOOST_LENGTHS: [u32; FLICKER_FRAMES] = [10, 13, 11];
const NITRO_LENGTHS: [u32; FLICKER_FRAMES] = [13, 16, 14];
/// How far the plume's base reaches into the car's tail, in px, so no gap
/// shows between the car and its flame.
const OVERLAP: f32 = 2.0;

/// Core, mid, outer and rim colors of one flame palette.
struct FlamePalette {
    tones: [[u8; 3]; 3],
    rim: [u8; 3],
    lengths: [u32; FLICKER_FRAMES],
}

const BOOST: FlamePalette = FlamePalette {
    tones: [[255, 248, 200], [255, 214, 72], [255, 132, 36]],
    rim: [170, 50, 24],
    lengths: BOOST_LENGTHS,
};

const NITRO: FlamePalette = FlamePalette {
    tones: [[235, 255, 255], [150, 240, 255], [64, 150, 255]],
    rim: [32, 60, 168],
    lengths: NITRO_LENGTHS,
};

fn palette(nitro: bool) -> &'static FlamePalette {
    if nitro { &NITRO } else { &BOOST }
}

/// Voxel slices of the plume for flicker `frame`, ground slice first. The
/// base sits at the `+X` end (toward the car when baked at the car's
/// heading) and the plume tapers to its tip toward `-X`.
pub fn flame_slices(nitro: bool, frame: usize) -> Vec<Grid> {
    let pal = palette(nitro);
    let len = pal.lengths[frame % FLICKER_FRAMES].min(FLAME_LEN) as f32;
    let salt = frame as u32 * 7 + u32::from(nitro) * 131;
    let cy = (FLAME_WID as f32 - 1.0) * 0.5;
    (0..FLAME_SLICES)
        .map(|z| {
            let mut g = Grid::new(FLAME_LEN, FLAME_WID);
            for x in 0..FLAME_LEN {
                // 0 at the base, 1 at the tip.
                let s = (FLAME_LEN - 1 - x) as f32 / len;
                if s >= 1.0 {
                    continue;
                }
                for y in 0..FLAME_WID {
                    let h = hash3(x as i32, y as i32, z as i32, salt);
                    // Ragged edges, more so toward the tip.
                    let jitter = ((h % 3) as f32 - 1.0) * 0.4 * s;
                    // Narrow at the nozzle, swelling just behind it, then tapering.
                    let swell = 0.75 + 0.25 * (s * 4.0).min(1.0);
                    let r = BASE_RADIUS * swell * (1.0 - s).powf(0.55) + jitter;
                    let (dy, dz) = (y as f32 - cy, z as f32 - AXIS_Z);
                    if r <= 0.0 || (dy * dy + dz * dz).sqrt() > r {
                        continue;
                    }
                    // Seen from above, the hot core is a ridge along the axis
                    // (lateral distance only) that cools toward the tip.
                    let heat = (1.0 - dy.abs() / r) * (1.0 - 0.8 * s);
                    let mut band = if heat > 0.5 {
                        0
                    } else if heat > 0.22 {
                        1
                    } else {
                        2
                    };
                    // A few voxels flicker a tone down.
                    band += usize::from((h >> 4).is_multiple_of(5));
                    let [r, g2, b] = pal.tones[band.min(2)];
                    g.set(x, y, [r, g2, b, 255]);
                }
            }
            g
        })
        .collect()
}

/// The plume at `angle` (radians, CCW, 0 = base toward `+X`), rimmed in the
/// palette's dark tone, on a canvas centred on the flame's ground point.
pub fn render_flame(nitro: bool, frame: usize, angle: f32) -> Grid {
    let slices = flame_slices(nitro, frame);
    let canvas = stack_canvas(FLAME_LEN, FLAME_WID, slices.len(), SQUASH, SPACING);
    let [r, g, b] = palette(nitro).rim;
    outline(&composite_stack(&slices, angle, SQUASH, SPACING, canvas), [r, g, b, 255])
}

/// Ground offset of the flame sprite from the car's centre, before the
/// vertical squash: the plume's base just inside the tail.
pub fn flame_reach(tail: f32) -> f32 {
    tail + FLAME_LEN as f32 * 0.5 - OVERLAP
}

/// Baked flame frames: `HEADINGS` per flicker frame per palette.
#[derive(Resource)]
pub struct FlameArt {
    frames: Vec<Handle<Image>>,
}

impl FlameArt {
    pub fn bake(images: &mut Assets<Image>) -> Self {
        let usage = RenderAssetUsages::RENDER_WORLD;
        let mut frames = Vec::with_capacity(2 * FLICKER_FRAMES * HEADINGS);
        for nitro in [false, true] {
            for frame in 0..FLICKER_FRAMES {
                for i in 0..HEADINGS {
                    let img = render_flame(nitro, frame, frame_angle(i, HEADINGS)).to_image(usage);
                    frames.push(images.add(img));
                }
            }
        }
        Self { frames }
    }

    /// Frame for a car at `heading` (the flame points the way the car does,
    /// its base at the tail).
    pub fn get(&self, nitro: bool, frame: usize, heading: f32) -> &Handle<Image> {
        let i = frame_index(heading, HEADINGS);
        &self.frames[((usize::from(nitro) * FLICKER_FRAMES) + frame % FLICKER_FRAMES) * HEADINGS + i]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque(g: &Grid) -> usize {
        g.px.iter().filter(|p| p[3] > 0).count()
    }

    fn voxels(slices: &[Grid]) -> usize {
        slices.iter().map(opaque).sum()
    }

    #[test]
    fn plume_is_a_tapered_solid_lifted_off_the_ground() {
        for nitro in [false, true] {
            for frame in 0..FLICKER_FRAMES {
                let s = flame_slices(nitro, frame);
                assert_eq!(s.len(), FLAME_SLICES);
                assert!(s.iter().all(|g| g.width == FLAME_LEN && g.height == FLAME_WID));
                assert!(s[0].is_empty(), "ground slice must stay empty");
                // The axis slice is solid at the base and empty past the tip.
                let mid = &s[AXIS_Z as usize];
                assert!(mid.get(FLAME_LEN - 1, FLAME_WID / 2)[3] > 0);
                let len = palette(nitro).lengths[frame];
                if len < FLAME_LEN {
                    assert!((0..FLAME_WID).all(|y| mid.get(FLAME_LEN - 1 - len, y)[3] == 0));
                }
                // Wider at the base than near the tip.
                let col = |x: u32| s.iter().map(|g| (0..FLAME_WID).filter(|&y| g.get(x, y)[3] > 0).count()).sum::<usize>();
                assert!(col(FLAME_LEN - 1) > col(FLAME_LEN - len + 1));
            }
        }
    }

    #[test]
    fn flicker_frames_differ_and_nitro_burns_longer() {
        let boost: Vec<usize> = (0..FLICKER_FRAMES).map(|f| voxels(&flame_slices(false, f))).collect();
        assert!(boost[0] != boost[1] && boost[1] != boost[2]);
        for f in 0..FLICKER_FRAMES {
            assert!(voxels(&flame_slices(true, f)) > voxels(&flame_slices(false, f)));
        }
    }

    #[test]
    fn hot_core_faces_the_car() {
        // Baked at heading 0 the car is to the right (+X), so the hot base and
        // core pixels sit in the right half; at PI they swap sides.
        let core = |g: &Grid, right: bool| {
            let half = g.width / 2;
            (0..g.height)
                .flat_map(|y| (0..g.width).map(move |x| (x, y)))
                .filter(|&(x, _)| (x >= half) == right)
                .filter(|&(x, y)| g.get(x, y)[..3] == BOOST.tones[0])
                .count()
        };
        let east = render_flame(false, 1, 0.0);
        assert!(core(&east, true) > core(&east, false));
        let west = render_flame(false, 1, std::f32::consts::PI);
        assert!(core(&west, false) > core(&west, true));
    }

    #[test]
    fn rendered_flame_has_its_own_rim_not_black() {
        let g = render_flame(true, 0, 0.7);
        let rim = [NITRO.rim[0], NITRO.rim[1], NITRO.rim[2], 255];
        assert!(g.px.contains(&rim));
        assert!(!g.px.contains(&[14, 14, 18, 255]));
    }

    #[test]
    fn base_reaches_just_inside_the_tail() {
        assert_eq!(flame_reach(15.0), 15.0 + 8.0 - OVERLAP);
    }
}
