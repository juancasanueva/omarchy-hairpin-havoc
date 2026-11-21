//! Sprite-stacked scenery: orchard trees, pines, cacti, rocks, dock containers
//! and the lucky-box crates. Each one is a small voxel model turned into
//! slices (ground slice first, centred on the object's ground point) and baked
//! through the car compositor (`stack::render_stack`): one outline around the
//! composite, the same squash and 1 px slice lift, nearest sampling.
//!
//! Lighting is baked per angle: `build` takes the light direction in model
//! space (`light_at(angle)`), so every prop is lit from the same world
//! direction whichever way it was rotated. Colors use 4-tone hue-shifted ramps
//! (cool deep shadow, darker cool tone, base, warm highlight).

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;

use super::stack::{Grid, SPACING, SQUASH, footprint, render_shadow, render_stack};
use super::theme::WorldTheme;
use super::track_view::Rng;

/// World direction toward the light: mostly overhead, from the upper left of
/// the screen, so tops are brightest and right-hand faces fall into shadow.
const LIGHT: Vec3 = Vec3::new(-0.6, 0.12, 0.8);

// ---------------------------------------------------------------------------
// Colors
// ---------------------------------------------------------------------------

/// Four tones from deep shadow to highlight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ramp(pub [[u8; 4]; 4]);

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])]
}

fn scale(c: [u8; 3], f: f32) -> [u8; 3] {
    c.map(|v| (v as f32 * f).round().clamp(0.0, 255.0) as u8)
}

fn opaque(c: [u8; 3]) -> [u8; 4] {
    [c[0], c[1], c[2], 255]
}

impl Ramp {
    /// Hue-shifted ramp around `base`: shadows drift toward blue, the
    /// highlight toward warm yellow.
    pub fn new(base: [u8; 3]) -> Self {
        Self([
            opaque(mix(scale(base, 0.42), [22, 26, 66], 0.32)),
            opaque(mix(scale(base, 0.7), [38, 50, 96], 0.16)),
            opaque(base),
            opaque(mix(scale(base, 1.2), [255, 240, 168], 0.3)),
        ])
    }

    /// Shallow ramp for very light materials (snow, glyphs): cool blue shadows.
    pub fn soft(base: [u8; 3]) -> Self {
        Self([
            opaque(mix(scale(base, 0.66), [70, 96, 168], 0.3)),
            opaque(mix(scale(base, 0.82), [140, 166, 220], 0.25)),
            opaque(mix(base, [222, 234, 250], 0.4)),
            opaque(base),
        ])
    }

    /// Tone for lightness `t` (roughly 0..1).
    pub fn pick(&self, t: f32) -> [u8; 4] {
        let i = match t {
            t if t < 0.3 => 0,
            t if t < 0.5 => 1,
            t if t < 0.72 => 2,
            _ => 3,
        };
        self.0[i]
    }
}

/// Track colors the props are built from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PropColors {
    pub main: [u8; 3],
    pub dark: [u8; 3],
    pub accent: [u8; 3],
}

pub fn rgb(c: Color) -> [u8; 3] {
    let s = c.to_srgba();
    [s.red, s.green, s.blue].map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
}

impl PropColors {
    pub fn from_theme(th: &WorldTheme) -> Self {
        Self { main: rgb(th.prop_main), dark: rgb(th.prop_dark), accent: rgb(th.prop_accent) }
    }
}

const BARK: [u8; 3] = [112, 72, 46];
const STEEL: [u8; 3] = [70, 74, 84];

// ---------------------------------------------------------------------------
// Voxel volume
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Vox {
    /// Index into the model's ramps.
    ramp: u8,
    /// Lightness nudge in tenths (texture: ribs, corrugation, clumps).
    bias: i8,
}

const fn vox(ramp: u8, bias: i8) -> Vox {
    Vox { ramp, bias }
}

/// `w` x `d` x `layers` voxels; x right, y up the screen (world +Y), z up.
struct Volume {
    w: i32,
    d: i32,
    layers: i32,
    cells: Vec<Option<Vox>>,
}

impl Volume {
    fn new(w: i32, d: i32, layers: i32) -> Self {
        Self { w, d, layers, cells: vec![None; (w * d * layers) as usize] }
    }

    fn idx(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        let inside = (0..self.w).contains(&x) && (0..self.d).contains(&y) && (0..self.layers).contains(&z);
        inside.then(|| ((z * self.d + y) * self.w + x) as usize)
    }

    fn get(&self, x: i32, y: i32, z: i32) -> Option<Vox> {
        self.idx(x, y, z).and_then(|i| self.cells[i])
    }

    fn put(&mut self, x: i32, y: i32, z: i32, v: Vox) {
        if let Some(i) = self.idx(x, y, z) {
            self.cells[i] = Some(v);
        }
    }

    /// Solid at (x, y, z); the ground below the first slice counts as solid.
    fn solid(&self, x: i32, y: i32, z: i32) -> bool {
        z < 0 || self.get(x, y, z).is_some()
    }

    /// Offset of column `x` from the centre; half-integers on even sizes, so
    /// shapes are symmetric about the ground point.
    fn dx(&self, x: i32) -> f32 {
        x as f32 - (self.w as f32 - 1.0) * 0.5
    }

    fn dy(&self, y: i32) -> f32 {
        y as f32 - (self.d as f32 - 1.0) * 0.5
    }

    /// Fills every voxel for which `f(dx, dy, z)` returns a voxel.
    fn fill_with(&mut self, mut f: impl FnMut(f32, f32, i32) -> Option<Vox>) {
        for z in 0..self.layers {
            for y in 0..self.d {
                for x in 0..self.w {
                    if let Some(v) = f(self.dx(x), self.dy(y), z) {
                        self.put(x, y, z, v);
                    }
                }
            }
        }
    }

    /// Surface normal estimated from the empty cells around a voxel.
    fn normal(&self, x: i32, y: i32, z: i32) -> Vec3 {
        let mut n = Vec3::ZERO;
        for oz in -1..=1 {
            for oy in -1..=1 {
                for ox in -1..=1 {
                    if (ox, oy, oz) != (0, 0, 0) && !self.solid(x + ox, y + oy, z + oz) {
                        n += Vec3::new(ox as f32, oy as f32, oz as f32);
                    }
                }
            }
        }
        n.try_normalize().unwrap_or(Vec3::Z)
    }

    /// Lit slices, ground slice first; empty top slices are dropped.
    fn shade(&self, ramps: &[Ramp], light: Vec3) -> Vec<Grid> {
        let mut slices = Vec::new();
        for z in 0..self.layers {
            let mut g = Grid::new(self.w as u32, self.d as u32);
            for y in 0..self.d {
                for x in 0..self.w {
                    let Some(v) = self.get(x, y, z) else { continue };
                    let n = self.normal(x, y, z);
                    let h = z as f32 / (self.layers - 1).max(1) as f32;
                    let t = 0.12 + 0.62 * n.dot(light).max(0.0) + 0.26 * h + v.bias as f32 * 0.1;
                    // Grid rows run top to bottom; world +Y is the top row.
                    g.set(x as u32, (self.d - 1 - y) as u32, ramps[v.ramp as usize].pick(t));
                }
            }
            slices.push(g);
        }
        while slices.last().is_some_and(Grid::is_empty) {
            slices.pop();
        }
        slices
    }
}

pub(crate) fn hash3(x: i32, y: i32, z: i32, salt: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ (z as u32).wrapping_mul(0xcb1a_b31f)
        ^ salt.wrapping_mul(0x9e37_79b9);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^ (h >> 15)
}

/// Coarse texture bias: 2x2x2 clumps a little darker or lighter.
fn clump(dx: f32, dy: f32, z: i32, salt: u32) -> i8 {
    let c = |v: f32| (v + 32.0).div_euclid(2.0) as i32;
    match hash3(c(dx), c(dy), z.div_euclid(2), salt) % 6 {
        0 => -1,
        1 => 1,
        _ => 0,
    }
}

/// Even size that fits `reach` px each side of the centre plus a margin.
fn even_span(reach: f32) -> i32 {
    2 * (reach.ceil() as i32 + 1)
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    Tree,
    Pine,
    Cactus,
    Rock,
    Container,
    Crate,
}

impl Model {
    pub const ALL: [Model; 6] = [Model::Tree, Model::Pine, Model::Cactus, Model::Rock, Model::Container, Model::Crate];

    /// Distinct shapes (or colors, for containers) per model.
    pub fn variants(self) -> u8 {
        match self {
            Model::Tree => 4,
            Model::Pine | Model::Cactus | Model::Rock | Model::Container => 3,
            Model::Crate => 1,
        }
    }

    /// Angles each scenery variant is baked at; placed props pick one.
    pub fn angles(self) -> &'static [f32] {
        match self {
            Model::Tree | Model::Pine | Model::Rock => &[0.0, 2.1, 4.2],
            // Arms stay mostly sideways, where they read.
            Model::Cactus => &[0.0, PI, 0.4, PI - 0.4],
            Model::Container => &[0.0, FRAC_PI_2, 0.45, 2.2],
            Model::Crate => &[0.0],
        }
    }
}

/// Light direction in the model frame of a prop rotated by `angle`.
pub fn light_at(angle: f32) -> Vec3 {
    let l = LIGHT.normalize();
    let (s, c) = (-angle).sin_cos();
    Vec3::new(l.x * c - l.y * s, l.x * s + l.y * c, l.z)
}

/// Lit slices of `model` (ground slice first, centred on its ground point).
pub fn build(model: Model, variant: u8, light: Vec3, colors: &PropColors) -> Vec<Grid> {
    let (vol, ramps) = match model {
        Model::Tree => tree(variant, colors),
        Model::Pine => pine(variant, colors),
        Model::Cactus => cactus(variant, colors),
        Model::Rock => rock(variant, colors),
        Model::Container => container(variant, colors),
        Model::Crate => lucky_crate(),
    };
    vol.shade(&ramps, light)
}

/// Orchard tree: a short trunk under a lumpy canopy of blended spheres, with
/// a few 2 px fruit on the outer shell.
fn tree(variant: u8, c: &PropColors) -> (Volume, Vec<Ramp>) {
    const LEAF: u8 = 0;
    const TRUNK: u8 = 1;
    const FRUIT: u8 = 2;
    // (canopy radius, trunk slices, vertical stretch, sub-blobs)
    let (r, trunk, vs, blobs) = [(8.0, 4, 0.8, 3), (10.0, 5, 0.78, 4), (11.0, 5, 0.9, 5), (9.0, 6, 0.68, 4)]
        [(variant % 4) as usize];
    let mut rng = Rng::new(0x7ee5 + variant as u32 * 977);
    let zc = trunk as f32 + r * vs - 1.0;
    let mut spheres = vec![(Vec3::new(0.0, 0.0, zc), r)];
    for k in 0..blobs {
        let a = k as f32 / blobs as f32 * TAU + rng.range(-0.5, 0.5);
        let dist = r * rng.range(0.5, 0.68);
        let rr = r * rng.range(0.46, 0.6);
        let z = zc + r * vs * rng.range(-0.3, 0.35);
        spheres.push((Vec3::new(a.cos() * dist, a.sin() * dist, z), rr));
    }
    let reach = spheres.iter().fold(0.0f32, |m, (p, rr)| m.max(p.truncate().length() + rr));
    let top = spheres.iter().fold(0.0f32, |m, (p, rr)| m.max(p.z + rr * vs));
    let span = even_span(reach);
    let mut v = Volume::new(span, span, top.ceil() as i32 + 1);
    let inside = |p: Vec3| {
        spheres.iter().any(|(s, rr)| {
            let d = p - *s;
            d.x * d.x + d.y * d.y + (d.z / vs) * (d.z / vs) <= rr * rr
        })
    };
    let salt = variant as u32;
    v.fill_with(|dx, dy, z| {
        if inside(Vec3::new(dx, dy, z as f32)) {
            Some(vox(LEAF, clump(dx, dy, z, salt)))
        } else if (z as f32) < zc && dx.abs() < 1.0 && dy.abs() < 1.0 {
            Some(vox(TRUNK, 0))
        } else if z == 0 && dx.abs() < 2.0 && dy.abs() < 2.0 && dx.abs() + dy.abs() < 3.0 {
            // Root flare: 4x4 without corners.
            Some(vox(TRUNK, -1))
        } else {
            None
        }
    });
    // Fruit: 2x2x2 clusters on the canopy's outer shell, upper half mostly.
    for _ in 0..3 + variant as usize {
        let (s, rr) = spheres[(rng.next_u32() as usize) % spheres.len()];
        let a = rng.range(0.0, TAU);
        let up = rng.range(-0.2, 0.75);
        let dir = Vec3::new(a.cos() * (1.0 - up * up).sqrt(), a.sin() * (1.0 - up * up).sqrt(), up);
        let p = s + Vec3::new(dir.x * rr, dir.y * rr, dir.z * rr * vs);
        let (cx, cy) = (p.x + (span as f32 - 1.0) * 0.5, p.y + (span as f32 - 1.0) * 0.5);
        let (x0, y0, z0) = (cx.round() as i32 - 1, cy.round() as i32 - 1, p.z.round() as i32 - 1);
        for (x, y, z) in (0..8).map(|i| (x0 + (i & 1), y0 + ((i >> 1) & 1), z0 + (i >> 2))) {
            if v.get(x, y, z).is_some_and(|c| c.ramp == LEAF) && on_shell(&v, x, y, z) {
                v.put(x, y, z, vox(FRUIT, 1));
            }
        }
    }
    (v, vec![Ramp::new(c.main), Ramp::new(BARK), Ramp::new(c.accent)])
}

/// Has an empty 6-neighbour (outside of the solid).
fn on_shell(v: &Volume, x: i32, y: i32, z: i32) -> bool {
    [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1)]
        .iter()
        .any(|&(ox, oy, oz)| !v.solid(x + ox, y + oy, z + oz))
}

/// Snowy pine: trunk plus three stepped cone tiers, snow on each tier's top
/// faces (the outer rim stays green so the tiers read).
fn pine(variant: u8, c: &PropColors) -> (Volume, Vec<Ramp>) {
    const NEEDLES: u8 = 0;
    const TRUNK: u8 = 1;
    const SNOW: u8 = 2;
    let (r0, trunk) = [(9.0f32, 3), (11.0, 4), (8.0, 3)][(variant % 3) as usize];
    // (first slice, slices, base radius)
    let mut tiers = Vec::new();
    let mut z0 = trunk;
    for k in 0..3 {
        let h = 7 - k;
        tiers.push((z0, h, r0 * (1.0 - 0.22 * k as f32)));
        z0 += h - 1;
    }
    let tip = z0 + 1;
    let radius_at = |z: i32| -> f32 {
        let tier = tiers
            .iter()
            .filter(|&&(t0, h, _)| (t0..t0 + h).contains(&z))
            .map(|&(t0, h, r)| {
                // Radius shrinks in 2-slice steps, so every step leaves a ledge.
                let j = (z - t0) / 2 * 2;
                (r * (1.0 - j as f32 / h as f32)).max(1.0)
            })
            .fold(0.0f32, f32::max);
        if (tip..tip + 2).contains(&z) { tier.max(1.0) } else { tier }
    };
    let span = even_span(r0);
    let mut v = Volume::new(span, span, tip + 2);
    let salt = 40 + variant as u32;
    v.fill_with(|dx, dy, z| {
        let d = (dx * dx + dy * dy).sqrt();
        if d <= radius_at(z) + 0.25 {
            Some(vox(NEEDLES, clump(dx, dy, z, salt)))
        } else if z <= trunk && dx.abs() < 1.0 && dy.abs() < 1.0 {
            Some(vox(TRUNK, 0))
        } else {
            None
        }
    });
    for z in 0..v.layers {
        for y in 0..v.d {
            for x in 0..v.w {
                let Some(cell) = v.get(x, y, z) else { continue };
                let d = (v.dx(x).powi(2) + v.dy(y).powi(2)).sqrt();
                let exposed = !v.solid(x, y, z + 1);
                if cell.ramp == NEEDLES && exposed && (d <= radius_at(z) - 1.25 || z >= tip) {
                    v.put(x, y, z, vox(SNOW, 0));
                }
            }
        }
    }
    (v, vec![Ramp::new(c.main), Ramp::new(BARK), Ramp::soft(c.accent)])
}

/// Saguaro: a ribbed round column with arms that leave it sideways at mid
/// height and turn upward.
fn cactus(variant: u8, c: &PropColors) -> (Volume, Vec<Ramp>) {
    const SKIN: u8 = 0;
    // (height, arms as (direction, first slice, top slice))
    let (height, arms): (i32, &[(Vec2, i32, i32)]) = match variant % 3 {
        0 => (21, &[(Vec2::X, 8, 17)]),
        1 => (24, &[(Vec2::X, 9, 20), (Vec2::NEG_X, 6, 15)]),
        _ => (17, &[(Vec2::NEG_X, 6, 13)]),
    };
    let rib = |dx: f32, dy: f32| -> i8 {
        let k = ((dy.atan2(dx) + PI) / TAU * 10.0).floor() as i32;
        if k % 2 == 0 { 0 } else { -1 }
    };
    let mut v = Volume::new(20, 20, height + 1);
    v.fill_with(|dx, dy, z| {
        let d = (dx * dx + dy * dy).sqrt();
        // Rounded crown over the last two slices.
        let r = match height - z {
            1 => 2.2,
            2 => 2.6,
            n if n >= 3 => 3.0,
            _ => 0.0,
        };
        if d <= r {
            return Some(vox(SKIN, rib(dx, dy)));
        }
        for &(dir, z0, z1) in arms {
            let along = dx * dir.x + dy * dir.y;
            let across = (dx * dir.y - dy * dir.x).abs();
            // Elbow: a 4 px wide, 3 slice tall bar out to the arm column.
            if (z0..z0 + 3).contains(&z) && (1.0..=7.0).contains(&along) && across < 2.0 {
                return Some(vox(SKIN, 0));
            }
            // Upright arm, 4 px round, centred 6 px out.
            let (ax, ay) = (dx - dir.x * 6.0, dy - dir.y * 6.0);
            let ar = if z == z1 { 1.0 } else { 2.0 };
            if (z0..=z1).contains(&z) && (ax * ax + ay * ay).sqrt() <= ar {
                return Some(vox(SKIN, rib(ax, ay)));
            }
        }
        None
    });
    (v, vec![Ramp::new(c.main)])
}

/// Desert rock: a flattened dome, sometimes with a smaller boulder leaning on it.
fn rock(variant: u8, c: &PropColors) -> (Volume, Vec<Ramp>) {
    const STONE: u8 = 0;
    let (rx, ry, h, buddy) = [(7.0, 6.0, 5.5, false), (9.5, 7.5, 6.5, true), (6.0, 5.0, 4.5, true)]
        [(variant % 3) as usize];
    let span = even_span(rx + 1.0);
    let mut v = Volume::new(span, span, h as i32 + 2);
    let salt = 80 + variant as u32;
    let b = Vec2::new(rx * 0.55, -ry * 0.55);
    v.fill_with(|dx, dy, z| {
        let zf = z as f32 + 0.5;
        let main = (dx / rx).powi(2) + (dy / ry).powi(2) + (zf / h).powi(2) <= 1.0;
        let small = buddy && ((dx - b.x) / (rx * 0.5)).powi(2) + ((dy - b.y) / (ry * 0.5)).powi(2) + (zf / (h * 0.65)).powi(2) <= 1.0;
        (main || small).then(|| vox(STONE, clump(dx, dy, z, salt)))
    });
    (v, vec![Ramp::new(c.accent)])
}

/// Container plan size and height.
pub const CONTAINER: (i32, i32, i32) = (32, 14, 12);

/// Shipping container: corrugated sides, a darker roof frame and a door end
/// (+X) with two lock bars. The variant picks the track's container color.
fn container(variant: u8, c: &PropColors) -> (Volume, Vec<Ramp>) {
    const BODY: u8 = 0;
    const BARS: u8 = 1;
    let (w, d, layers) = CONTAINER;
    let color = [c.main, c.dark, c.accent][(variant % 3) as usize];
    let mut v = Volume::new(w, d, layers);
    for z in 0..layers {
        for y in 0..d {
            for x in 0..w {
                let side_y = y == 0 || y == d - 1;
                let side_x = x == 0 || x == w - 1;
                let cell = if side_x && side_y || z == 0 {
                    // Corner posts and the base rail.
                    vox(BODY, -2)
                } else if z == layers - 1 {
                    if side_x || side_y { vox(BODY, -2) } else { vox(BODY, if (x / 2) % 2 == 0 { 1 } else { 0 }) }
                } else if x == w - 1 && [3, 4, 9, 10].contains(&y) && z < layers - 2 {
                    vox(BARS, 0)
                } else if side_y {
                    vox(BODY, if (x / 2) % 2 == 0 { 0 } else { -1 })
                } else if side_x {
                    vox(BODY, if (y / 2) % 2 == 0 { 0 } else { -1 })
                } else {
                    vox(BODY, 0)
                };
                v.put(x, y, z, cell);
            }
        }
    }
    (v, vec![Ramp::new(color), Ramp::new(STEEL)])
}

/// Lucky-box crate plan size and height.
pub const CRATE: (i32, i32, i32) = (14, 14, 12);

/// "?" glyph, 2 px strokes, top row first.
const GLYPH: [&str; 10] = [
    "..####..",
    ".######.",
    ".##..##.",
    ".....##.",
    "....###.",
    "...###..",
    "...##...",
    "........",
    "...##...",
    "...##...",
];
/// First column of the glyph on a crate face.
const GLYPH_X: i32 = 3;

fn glyph_at(col: i32, row: i32) -> bool {
    (0..10).contains(&row)
        && (0..8).contains(&col)
        && GLYPH[row as usize].as_bytes()[col as usize] == b'#'
}

/// Lucky-box crate: gold panels in a darker frame, a light bevel round the
/// top, and a "?" on the top and on every side.
fn lucky_crate() -> (Volume, Vec<Ramp>) {
    const PANEL: u8 = 0;
    const FRAME: u8 = 1;
    const BEVEL: u8 = 2;
    const MARK: u8 = 3;
    const MARK_SHADOW: u8 = 4;
    let (w, d, layers) = CRATE;
    let top = layers - 1;
    let mut v = Volume::new(w, d, layers);
    for z in 0..layers {
        for y in 0..d {
            for x in 0..w {
                let edge_x = x == 0 || x == w - 1;
                let edge_y = y == 0 || y == d - 1;
                // Glyph coordinates on the face this voxel belongs to.
                let side_row = top - 1 - z;
                let face = if z == top {
                    // Top face: rows run from far (+Y) to near.
                    Some((x - GLYPH_X, d - 3 - y))
                } else if y == 0 {
                    Some((x - GLYPH_X, side_row))
                } else if y == d - 1 {
                    Some((w - 1 - x - GLYPH_X, side_row))
                } else if x == 0 {
                    Some((d - 1 - y - GLYPH_X, side_row))
                } else if x == w - 1 {
                    Some((y - GLYPH_X, side_row))
                } else {
                    None
                };
                let mark = face.is_some_and(|(col, row)| glyph_at(col, row));
                let shadow = face.is_some_and(|(col, row)| glyph_at(col - 1, row - 1));
                let cell = if edge_x && edge_y || z == 0 {
                    vox(FRAME, 0)
                } else if z == top && (edge_x || edge_y) {
                    vox(BEVEL, 1)
                } else if mark {
                    vox(MARK, 2)
                } else if shadow {
                    vox(MARK_SHADOW, 0)
                } else if z == top {
                    // A mid-gold lid keeps the white "?" readable.
                    vox(PANEL, -2)
                } else {
                    vox(PANEL, 0)
                };
                v.put(x, y, z, cell);
            }
        }
    }
    // Hand-tuned warm ramps: the generic hue shift turns gold muddy brown.
    let ramps = vec![
        Ramp([[204, 112, 34, 255], [232, 146, 36, 255], [250, 184, 48, 255], [255, 222, 112, 255]]),
        Ramp([[112, 46, 24, 255], [150, 70, 26, 255], [184, 96, 30, 255], [214, 126, 44, 255]]),
        Ramp([[240, 176, 70, 255], [255, 206, 96, 255], [255, 228, 140, 255], [255, 244, 190, 255]]),
        Ramp::soft([255, 252, 236]),
        Ramp::new([120, 54, 20]),
    ];
    (v, ramps)
}

// ---------------------------------------------------------------------------
// Baking
// ---------------------------------------------------------------------------

/// One baked prop: the outlined stack and its ground shadow (white
/// footprint, tinted by the sprite color). Both canvases are centred on the
/// ground point.
#[derive(Clone, Debug)]
pub struct BakedProp {
    pub image: Handle<Image>,
    pub shadow: Handle<Image>,
}

/// CPU images of one prop at `angle`.
pub fn bake_grids(model: Model, variant: u8, angle: f32, colors: &PropColors) -> (Grid, Grid) {
    let slices = build(model, variant, light_at(angle), colors);
    let body = render_stack(&slices, angle, SQUASH, SPACING);
    let shadow = render_shadow(&footprint(&slices), angle, SQUASH);
    (body, shadow)
}

/// Bakes each (model, variant, angle) once per race and hands out the same
/// strong handles afterwards. Drop it after spawning: the sprites keep the
/// images alive, and they are freed when the race's entities despawn.
pub struct BakeCache {
    colors: PropColors,
    baked: HashMap<(Model, u8, u32), BakedProp>,
}

impl BakeCache {
    pub fn new(colors: PropColors) -> Self {
        Self { colors, baked: HashMap::new() }
    }

    pub fn get(&mut self, model: Model, variant: u8, angle: f32, images: &mut Assets<Image>) -> BakedProp {
        let colors = self.colors;
        self.baked
            .entry((model, variant, angle.to_bits()))
            .or_insert_with(|| {
                let (body, shadow) = bake_grids(model, variant, angle, &colors);
                let usage = RenderAssetUsages::RENDER_WORLD;
                BakedProp { image: images.add(body.to_image(usage)), shadow: images.add(shadow.to_image(usage)) }
            })
            .clone()
    }

    /// Number of distinct bakes so far.
    pub fn len(&self) -> usize {
        self.baked.len()
    }

    pub fn is_empty(&self) -> bool {
        self.baked.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tracks::TrackId;
    use crate::game::theme::world_theme;

    fn colors() -> PropColors {
        PropColors::from_theme(&world_theme(TrackId::OrchardRing))
    }

    fn opaque_count(g: &Grid) -> usize {
        g.px.iter().filter(|p| p[3] > 0).count()
    }

    /// Bounding box (x0, y0, x1, y1) of the opaque pixels, inclusive.
    fn bbox(g: &Grid) -> (u32, u32, u32, u32) {
        let mut b = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..g.height {
            for x in 0..g.width {
                if g.get(x, y)[3] > 0 {
                    b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
                }
            }
        }
        b
    }

    fn light() -> Vec3 {
        light_at(0.0)
    }

    #[test]
    fn every_model_builds_solid_even_centred_slices() {
        for model in Model::ALL {
            for variant in 0..model.variants() {
                let s = build(model, variant, light(), &colors());
                assert!(s.len() >= 4, "{model:?}/{variant}: {} slices", s.len());
                let (w, h) = (s[0].width, s[0].height);
                assert!(w % 2 == 0 && h % 2 == 0, "{model:?}/{variant}: {w}x{h} not even");
                assert!((8..=40).contains(&w) && (8..=40).contains(&h), "{model:?}/{variant}: {w}x{h}");
                assert!(s.iter().all(|g| g.width == w && g.height == h));
                assert!(!s[0].is_empty() && !s.last().unwrap().is_empty());
                // The footprint is centred on the ground point (symmetric bbox
                // within a pixel), except for the one-armed cacti.
                let fp = footprint(&s);
                let (x0, y0, x1, y1) = bbox(&fp);
                if model != Model::Cactus {
                    assert!((x0 as i32 - (w - 1 - x1) as i32).abs() <= 2, "{model:?}/{variant} x {x0}..{x1} of {w}");
                    assert!((y0 as i32 - (h - 1 - y1) as i32).abs() <= 2, "{model:?}/{variant} y {y0}..{y1} of {h}");
                }
            }
        }
    }

    #[test]
    fn variants_differ() {
        for model in Model::ALL {
            let all: Vec<Vec<Grid>> = (0..model.variants()).map(|v| build(model, v, light(), &colors())).collect();
            for i in 0..all.len() {
                for j in i + 1..all.len() {
                    assert_ne!(all[i], all[j], "{model:?} {i} == {j}");
                }
            }
        }
    }

    #[test]
    fn tree_canopy_is_round_in_plan() {
        for variant in 0..Model::Tree.variants() {
            let fp = footprint(&build(Model::Tree, variant, light(), &colors()));
            let (x0, y0, x1, y1) = bbox(&fp);
            // No full square corners: each bbox corner pixel is empty.
            for (x, y) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
                assert_eq!(fp.get(x, y)[3], 0, "tree {variant} corner ({x},{y})");
            }
            // And the disc fills most of its box (not a thin ring or cross).
            let area = ((x1 - x0 + 1) * (y1 - y0 + 1)) as usize;
            assert!(opaque_count(&fp) * 10 > area * 6, "tree {variant} too sparse");
        }
    }

    #[test]
    fn tree_has_a_trunk_and_fruit() {
        let c = colors();
        let s = build(Model::Tree, 2, light(), &c);
        // The ground slice is only the small root flare.
        assert!((1..=16).contains(&opaque_count(&s[0])), "ground slice {}", opaque_count(&s[0]));
        let fruit = Ramp::new(c.accent);
        assert!(s.iter().any(|g| g.px.iter().any(|p| fruit.0.contains(p))));
    }

    #[test]
    fn pine_tiers_shrink_with_height_and_carry_snow() {
        let c = PropColors::from_theme(&world_theme(TrackId::GlacierPass));
        for variant in 0..Model::Pine.variants() {
            let s = build(Model::Pine, variant, light(), &c);
            let widths: Vec<usize> = s.iter().map(opaque_count).collect();
            // Each tier starts with a slice wider than the one below it (its
            // skirt); those tier bases shrink with height.
            let bases: Vec<usize> = widths.windows(2).filter(|w| w[1] > w[0]).map(|w| w[1]).collect();
            assert_eq!(bases.len(), 3, "pine {variant}: {widths:?}");
            assert!(bases.windows(2).all(|b| b[1] < b[0]), "pine {variant}: {widths:?}");
            assert!(*widths.last().unwrap() * 8 < bases[0], "pine {variant}: {widths:?}");
            let snow = Ramp::soft(c.accent);
            let snowy = s.iter().flat_map(|g| &g.px).filter(|p| snow.0.contains(p)).count();
            assert!(snowy > 20, "pine {variant}: {snowy} snow px");
        }
    }

    #[test]
    fn cactus_has_arms_and_rocks_are_low_domes() {
        let c = PropColors::from_theme(&world_theme(TrackId::DuneSprint));
        for variant in 0..3 {
            let s = build(Model::Cactus, variant, light(), &c);
            // The column is 6 px round; arms make mid-height slices wider.
            let mid = opaque_count(&s[s.len() / 2]);
            assert!(mid > opaque_count(&s[1]), "cactus {variant}");
            let r = build(Model::Rock, variant, light(), &c);
            assert!(r.len() <= 8, "rock {variant}: {} slices", r.len());
            assert!(opaque_count(&r[0]) > opaque_count(r.last().unwrap()));
        }
    }

    #[test]
    fn container_is_long() {
        let s = build(Model::Container, 0, light(), &colors());
        let (x0, y0, x1, y1) = bbox(&footprint(&s));
        assert!(x1 - x0 > 2 * (y1 - y0), "{x0}..{x1} x {y0}..{y1}");
        assert_eq!(s.len() as i32, CONTAINER.2);
    }

    #[test]
    fn crate_shows_a_question_mark_on_top_and_sides() {
        let s = build(Model::Crate, 0, light(), &colors());
        let mark = Ramp::soft([255, 252, 236]);
        let top = s.last().unwrap();
        let marked: Vec<(u32, u32)> = (0..top.height)
            .flat_map(|y| (0..top.width).map(move |x| (x, y)))
            .filter(|&(x, y)| mark.0.contains(&top.get(x, y)))
            .collect();
        // The glyph has 26 cells, all on the top slice.
        assert_eq!(marked.len(), GLYPH.iter().map(|r| r.matches('#').count()).sum::<usize>());
        // The dot sits below the hook (rows grow downward = toward the viewer).
        let lowest = marked.iter().map(|p| p.1).max().unwrap();
        assert!(marked.iter().filter(|p| p.1 == lowest).all(|p| (6..=7).contains(&p.0)));
        // Side faces carry it too: the near (south) face of a middle slice.
        let mid = &s[5];
        assert!((0..mid.width).any(|x| mark.0.contains(&mid.get(x, mid.height - 1))));
    }

    #[test]
    fn light_follows_the_world_when_rotated() {
        let l0 = light_at(0.0);
        let l1 = light_at(FRAC_PI_2);
        // A model turned 90 degrees CCW sees the world light 90 degrees clockwise.
        assert!((l1.x - l0.y).abs() < 1e-5 && (l1.y + l0.x).abs() < 1e-5 && l1.z == l0.z);
    }

    #[test]
    fn ramps_go_dark_to_light() {
        let r = Ramp::new([47, 110, 44]);
        let lum = |c: [u8; 4]| c[0] as u32 * 3 + c[1] as u32 * 6 + c[2] as u32;
        assert!(r.0.windows(2).all(|w| lum(w[0]) < lum(w[1])));
        // The deep tone leans blue relative to the base.
        assert!(r.0[0][2] as f32 / r.0[0][1] as f32 > 44.0 / 110.0);
        assert_eq!(r.pick(0.0), r.0[0]);
        assert_eq!(r.pick(1.0), r.0[3]);
    }

    #[test]
    fn bake_cache_reuses_handles() {
        let mut images = Assets::<Image>::default();
        let mut cache = BakeCache::new(colors());
        let a = cache.get(Model::Tree, 1, 2.1, &mut images);
        let b = cache.get(Model::Tree, 1, 2.1, &mut images);
        assert_eq!(a.image, b.image);
        assert_eq!(a.shadow, b.shadow);
        assert_eq!(cache.len(), 1);
        let c = cache.get(Model::Tree, 1, 4.2, &mut images);
        let d = cache.get(Model::Tree, 2, 2.1, &mut images);
        let e = cache.get(Model::Pine, 1, 2.1, &mut images);
        assert_ne!(a.image, c.image);
        assert_ne!(a.image, d.image);
        assert_ne!(a.image, e.image);
        assert_eq!(cache.len(), 4);
        assert_eq!(images.len(), 8);
    }

    /// Writes a contact sheet of every prop model and variant at its baked angles,
    /// with shadows, on each track's ground color, plus a car for scale, as a
    /// PAM image. Run with
    /// `PROP_SHEET=/path/sheet.pam cargo test props::tests::contact_sheet -- --ignored`.
    #[test]
    #[ignore]
    fn contact_sheet() {
        use crate::core::cars::CarId;
        use crate::core::economy::PaintId;
        use crate::game::stack::procedural_car;
        let Some(path) = std::env::var_os("PROP_SHEET") else { return };
        let tracks = [
            (TrackId::OrchardRing, vec![Model::Tree]),
            (TrackId::GlacierPass, vec![Model::Pine]),
            (TrackId::DuneSprint, vec![Model::Cactus, Model::Rock]),
            (TrackId::NeonDocks, vec![Model::Container, Model::Crate]),
        ];
        let angles = [1.0, 3.0, 5.0, 2.6];
        let (cell_w, cell_h) = (72u32, 88u32);
        let mut rows: Vec<(TrackId, Option<(Model, u8)>)> = Vec::new();
        for (t, models) in &tracks {
            for &m in models {
                for v in 0..m.variants() {
                    rows.push((*t, Some((m, v))));
                }
            }
        }
        rows.push((TrackId::OrchardRing, None));
        let cols = angles.len() as u32;
        let mut sheet = Grid::new(cell_w * cols, cell_h * rows.len() as u32);
        let car = procedural_car(CarId::Pebble, PaintId::Factory);
        for (ri, (track, item)) in rows.iter().enumerate() {
            let th = world_theme(*track);
            let bg = rgb(th.grass);
            let shadow_c = rgb(th.shadow);
            let shadow_a = (th.shadow.alpha() * 255.0) as u8;
            let c = PropColors::from_theme(&th);
            for (ci, &a) in angles.iter().enumerate() {
                let (body, shadow) = match item {
                    // The angles placed props use, then extra ones.
                    Some((m, v)) => bake_grids(*m, *v, *m.angles().get(ci).unwrap_or(&a), &c),
                    None => (
                        render_stack(&car.slices, a, SQUASH, SPACING),
                        render_shadow(&car.footprint(), a, SQUASH),
                    ),
                };
                let (ox, oy) = (ci as u32 * cell_w, ri as u32 * cell_h);
                for y in 0..cell_h {
                    for x in 0..cell_w {
                        sheet.set(ox + x, oy + y, opaque(bg));
                    }
                }
                // Ground point at the cell centre, a bit low so tall stacks fit.
                let (gx, gy) = (ox + cell_w / 2, oy + cell_h * 2 / 3);
                let place = |sheet: &mut Grid, img: &Grid, dx: i32, dy: i32, tint: Option<[u8; 4]>| {
                    let x0 = gx as i32 - img.width as i32 / 2 + dx;
                    let y0 = gy as i32 - img.height as i32 / 2 + dy;
                    for y in 0..img.height {
                        for x in 0..img.width {
                            let p = img.get(x, y);
                            let (sx, sy) = (x0 + x as i32, y0 + y as i32);
                            if p[3] == 0 || sx < ox as i32 || sy < oy as i32 || sx >= (ox + cell_w) as i32 || sy >= (oy + cell_h) as i32 {
                                continue;
                            }
                            let p = tint.unwrap_or(p);
                            let d = sheet.get(sx as u32, sy as u32);
                            let a = p[3] as f32 / 255.0;
                            let m = |s: u8, t: u8| (s as f32 * a + t as f32 * (1.0 - a)).round() as u8;
                            sheet.set(sx as u32, sy as u32, [m(p[0], d[0]), m(p[1], d[1]), m(p[2], d[2]), 255]);
                        }
                    }
                };
                place(&mut sheet, &shadow, 2, 3, Some([shadow_c[0], shadow_c[1], shadow_c[2], shadow_a]));
                place(&mut sheet, &body, 0, 0, None);
            }
        }
        let mut bytes = format!(
            "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n",
            sheet.width, sheet.height
        )
        .into_bytes();
        bytes.extend(sheet.px.iter().flatten());
        std::fs::write(path, bytes).unwrap();
    }
}
