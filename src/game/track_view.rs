//! Builds the visual track: ground, barriers, gravel, road, kerbs, start line
//! and boost pads as a handful of vertex-colored meshes, plus the scenery
//! props as baked sprite stacks y-sorted with the cars.

use bevy::prelude::*;

use crate::core::race::FIELD_SIZE;
use crate::core::track::{Path, Track};

use super::meshes::MeshBuilder;
use super::pixel::WORLD_LAYER;
use super::props::{BakeCache, BakedProp, Model, PropColors};
use super::stack::{PROP_SORT, SHADOW_OFFSET, ground_z};
use super::theme::{PropKind, WorldTheme};
use super::AppState;

/// World z of prop shadows: above the track paint, below skid marks.
const PROP_SHADOW_Z: f32 = 5.1;

/// Small deterministic RNG for scenery placement.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    pub fn f(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f()
    }
}

/// Band between two lateral offsets on one side (`side` = +1 left, -1 right).
fn band(
    mb: &mut MeshBuilder,
    path: &Path,
    side: f32,
    inner: impl Fn(usize) -> f32,
    outer: impl Fn(usize) -> f32,
    color: impl Fn(usize) -> Option<Color>,
) {
    let n = path.samples.len();
    let segs = if path.closed { n } else { n - 1 };
    for i in 0..segs {
        let j = (i + 1) % n;
        let Some(c) = color(i) else { continue };
        let (a, b) = (&path.samples[i], &path.samples[j]);
        let pa_in = a.pos + a.normal * side * inner(i);
        let pa_out = a.pos + a.normal * side * outer(i);
        let pb_in = b.pos + b.normal * side * inner(j);
        let pb_out = b.pos + b.normal * side * outer(j);
        mb.quad(pa_in, pb_in, pb_out, pa_out, c);
    }
}

fn spawn_mesh(commands: &mut Commands, meshes: &mut Assets<Mesh>, mat: &Handle<ColorMaterial>, mb: MeshBuilder, z: f32) {
    if mb.is_empty() {
        return;
    }
    commands.spawn((
        Mesh2d(meshes.add(mb.build())),
        MeshMaterial2d(mat.clone()),
        Transform::from_xyz(0.0, 0.0, z),
        WORLD_LAYER,
        DespawnOnExit(AppState::Race),
    ));
}

pub fn spawn_track_view(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    images: &mut Assets<Image>,
    track: &Track,
    theme: &WorldTheme,
    seed: u32,
) {
    let mat = materials.add(ColorMaterial::default());
    let mut rng = Rng::new(seed);
    let (lo, hi) = track.bounds(500.0);

    // Ground with dithered tufts.
    let mut ground = MeshBuilder::default();
    ground.quad(lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y), theme.grass);
    let area = (hi - lo).x * (hi - lo).y;
    for _ in 0..(area / 900.0) as usize {
        let p = Vec2::new(rng.range(lo.x, hi.x), rng.range(lo.y, hi.y)).floor();
        let c = if rng.f() < 0.5 { theme.grass_dark } else { theme.grass_light };
        let s = if rng.f() < 0.7 { 1.0 } else { 2.0 };
        ground.quad(p, p + Vec2::new(s, 0.0), p + Vec2::splat(s), p + Vec2::new(0.0, s), c);
    }
    spawn_mesh(commands, meshes, &mat, ground, 0.0);

    let mut barrier = MeshBuilder::default();
    let mut gravel = MeshBuilder::default();
    let mut road = MeshBuilder::default();
    let mut paint = MeshBuilder::default();

    for path in track.paths() {
        for side in [1.0f32, -1.0] {
            let left = side > 0.0;
            band(
                &mut barrier,
                path,
                side,
                |i| path.wall_offset(i, left),
                |i| path.wall_offset(i, left) + 6.0,
                |_| Some(theme.barrier),
            );
            band(
                &mut barrier,
                path,
                side,
                |i| path.wall_offset(i, left) + 1.0,
                |i| path.wall_offset(i, left) + 3.0,
                |i| (i % 6 < 3).then_some(theme.barrier_top),
            );
            band(
                &mut gravel,
                path,
                side,
                |i| path.samples[i].half_width - 1.0,
                |i| path.gravel_offset(i, left),
                |_| Some(theme.gravel),
            );
            band(
                &mut road,
                path,
                side,
                |_| 0.0,
                |i| path.samples[i].half_width,
                |_| Some(theme.road),
            );
            band(
                &mut road,
                path,
                side,
                |i| path.samples[i].half_width - 3.0,
                |i| path.samples[i].half_width,
                |_| Some(theme.road_edge),
            );
            if path.style.kerbs {
                band(
                    &mut paint,
                    path,
                    side,
                    |i| path.samples[i].half_width - 8.0,
                    |i| path.samples[i].half_width - 6.0,
                    |i| (!path.samples[i].kerb).then_some(theme.line),
                );
                band(
                    &mut paint,
                    path,
                    side,
                    |i| path.samples[i].half_width - 4.0,
                    |i| path.samples[i].half_width + 8.0,
                    |i| {
                        path.samples[i]
                            .kerb
                            .then_some(if (i / 2) % 2 == 0 { theme.kerb_a } else { theme.kerb_b })
                    },
                );
            }
        }
        // Gravel speckles.
        for _ in 0..(path.length / 6.0) as usize {
            let i = (rng.next_u32() as usize) % path.samples.len();
            let s = &path.samples[i];
            let side = if rng.f() < 0.5 { 1.0 } else { -1.0 };
            let off = rng.range(s.half_width + 2.0, path.gravel_offset(i, side > 0.0));
            let p = (s.pos + s.normal * side * off).floor();
            gravel.quad(p, p + Vec2::X, p + Vec2::ONE, p + Vec2::Y, theme.grass_dark);
        }
    }

    // Start/finish checker and grid boxes.
    let s0 = &track.main.samples[0];
    let cells = (s0.half_width * 2.0 / 6.0) as i32;
    for row in 0..2 {
        for k in 0..cells {
            let white = (k + row) % 2 == 0;
            let lat = -s0.half_width + 3.0 + k as f32 * 6.0;
            let c = s0.pos + s0.tangent * (row as f32 * 6.0 - 3.0) + s0.normal * lat;
            paint.rect(c, s0.tangent, 3.0, 3.0, if white { Color::WHITE } else { Color::BLACK });
        }
    }
    for slot in 0..FIELD_SIZE {
        let (pos, dir) = grid_slot(track, slot);
        // A short line marking the front of each grid box.
        paint.rect(pos + dir * 16.0, dir.perp(), 12.0, 1.0, theme.line);
    }

    // Boost pads: chevrons.
    for pad in track.pads() {
        paint.rect(pad.center, pad.dir, pad.half_length, pad.half_width, Color::srgb(0.1, 0.35, 0.55));
        for k in -1..=1 {
            let c = pad.center + pad.dir * (k as f32 * 14.0);
            let tip = c + pad.dir * 6.0;
            let w = pad.dir.perp() * (pad.half_width - 6.0);
            paint.tri(tip, c - pad.dir * 4.0 + w, c - pad.dir * 1.0, Color::srgb(0.4, 0.95, 1.0));
            paint.tri(tip, c - pad.dir * 1.0, c - pad.dir * 4.0 - w, Color::srgb(0.4, 0.95, 1.0));
        }
    }

    spawn_mesh(commands, meshes, &mat, barrier, 1.0);
    spawn_mesh(commands, meshes, &mat, gravel, 2.0);
    spawn_mesh(commands, meshes, &mat, road, 3.0);
    spawn_mesh(commands, meshes, &mat, paint, 4.0);

    // Scenery props beyond the barriers: each (model, variant, angle) is baked
    // once; every placed prop is one sprite plus its ground shadow.
    let mut cache = BakeCache::new(PropColors::from_theme(theme));
    let mut placed: Vec<Vec2> = Vec::new();
    for _ in 0..(area / 9000.0) as usize {
        let p = Vec2::new(rng.range(lo.x, hi.x), rng.range(lo.y, hi.y)).floor();
        let q = track.query(p);
        let Some((_, depth)) = q.wall_push else { continue };
        if depth < 26.0 || placed.iter().any(|o| o.distance_squared(p) < 30.0 * 30.0) {
            continue;
        }
        placed.push(p);
        let model = prop_model(theme.prop, &mut rng);
        let variant = (rng.next_u32() % model.variants() as u32) as u8;
        let angles = model.angles();
        let angle = angles[rng.next_u32() as usize % angles.len()];
        let baked = cache.get(model, variant, angle, images);
        spawn_prop(commands, &baked, p, theme.shadow);
    }
}

/// Which model a placed prop of `kind` uses (desert tracks mix cacti and rocks).
fn prop_model(kind: PropKind, rng: &mut Rng) -> Model {
    match kind {
        PropKind::Tree => Model::Tree,
        PropKind::Pine => Model::Pine,
        PropKind::Container => Model::Container,
        PropKind::Cactus => {
            if rng.f() < 0.55 {
                Model::Cactus
            } else {
                Model::Rock
            }
        }
    }
}

/// One prop standing at `p` (whole pixels): the baked canvas is centred on
/// the ground point, so the sprite sits at `p` and sorts by `p.y`.
fn spawn_prop(commands: &mut Commands, baked: &BakedProp, p: Vec2, shadow: Color) {
    commands.spawn((
        Sprite { image: baked.shadow.clone(), color: shadow, ..default() },
        Transform::from_translation((p + SHADOW_OFFSET).extend(PROP_SHADOW_Z)),
        WORLD_LAYER,
        DespawnOnExit(AppState::Race),
    ));
    commands.spawn((
        Sprite::from_image(baked.image.clone()),
        Transform::from_translation(p.extend(ground_z(PROP_SORT, p.y))),
        WORLD_LAYER,
        DespawnOnExit(AppState::Race),
    ));
}

/// Grid position and facing for starting slot `slot` (0 = pole).
pub fn grid_slot(track: &Track, slot: usize) -> (Vec2, Vec2) {
    let back = 48.0 + 36.0 * slot as f32;
    let (p, t) = track.main.sample_at(track.length() - back);
    (p + t.perp() * grid_lateral(slot), t)
}

/// Sideways offset of starting slot `slot` (positive = left of travel).
pub fn grid_lateral(slot: usize) -> f32 {
    if slot.is_multiple_of(2) { 20.0 } else { -20.0 }
}
