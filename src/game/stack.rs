//! Sprite-stacked cars: a car is a stack of horizontal slices (ground slice
//! first). Every slice is rotated by the car heading, the stack is squashed
//! vertically and each slice is lifted one pixel above the previous one, so at
//! the game's pixel scale the slices read as one solid 3D car.
//!
//! Slices come from a procedural voxel model per body style, or from a slice
//! strip dropped into `assets/cars/<car-key>.png` (see `assets/cars/README.md`).
//! Cars point along +X at heading 0; world Y is up.

use std::f32::consts::TAU;
use std::path::{Path, PathBuf};

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::transform::TransformSystems;
use serde::Deserialize;

use crate::core::cars::{BodyStyle, CarId, car_spec};
use crate::core::economy::PaintId;

use super::pixel::WORLD_LAYER;

/// Vertical squash of the whole stack (fakes the camera tilt).
pub const SQUASH: f32 = 0.88;
/// Screen pixels each slice sits above the previous one.
pub const LIFT: f32 = 1.0;
/// Pre-rendered headings used when a car is drawn translucent.
pub const TURNS: usize = 64;
/// Heading of the menu previews (nose toward the viewer, right side visible).
pub const PREVIEW_ANGLE: f32 = -35.0 * std::f32::consts::PI / 180.0;

const SHADOW_ALPHA: f32 = 0.3;
const SHADOW_OFFSET: Vec2 = Vec2::new(3.0, -3.0);
const PREVIEW_OUTLINE: [u8; 4] = [14, 14, 18, 255];

// ---------------------------------------------------------------------------
// Pixel grids
// ---------------------------------------------------------------------------

/// RGBA pixels, row 0 at the top (image order, i.e. world +Y).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid {
    pub width: u32,
    pub height: u32,
    pub px: Vec<[u8; 4]>,
}

impl Grid {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, px: vec![[0; 4]; (width * height) as usize] }
    }

    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        self.px[(y * self.width + x) as usize]
    }

    pub fn set(&mut self, x: u32, y: u32, c: [u8; 4]) {
        let w = self.width;
        self.px[(y * w + x) as usize] = c;
    }

    /// Opaque (non-transparent) at signed coordinates; outside is transparent.
    fn solid(&self, x: i64, y: i64) -> bool {
        x >= 0
            && y >= 0
            && (x as u32) < self.width
            && (y as u32) < self.height
            && self.get(x as u32, y as u32)[3] > 0
    }

    pub fn is_empty(&self) -> bool {
        self.px.iter().all(|p| p[3] == 0)
    }

    /// Rotated 90 degrees clockwise (the top edge becomes the right edge).
    pub fn rotate_cw(&self) -> Self {
        let mut out = Grid::new(self.height, self.width);
        for y in 0..self.height {
            for x in 0..self.width {
                out.set(self.height - 1 - y, x, self.get(x, y));
            }
        }
        out
    }

    /// Rotated 90 degrees counter-clockwise (the top edge becomes the left edge).
    pub fn rotate_ccw(&self) -> Self {
        let mut out = Grid::new(self.height, self.width);
        for y in 0..self.height {
            for x in 0..self.width {
                out.set(y, self.width - 1 - x, self.get(x, y));
            }
        }
        out
    }

    pub fn rotate_180(&self) -> Self {
        let mut px = self.px.clone();
        px.reverse();
        Grid { width: self.width, height: self.height, px }
    }

    /// A `w` x `h` window starting at (`x0`, `y0`).
    pub fn crop(&self, x0: u32, y0: u32, w: u32, h: u32) -> Self {
        let mut out = Grid::new(w, h);
        for y in 0..h {
            for x in 0..w {
                out.set(x, y, self.get(x0 + x, y0 + y));
            }
        }
        out
    }

    pub fn to_image(&self, usage: RenderAssetUsages) -> Image {
        Image::new(
            Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
            TextureDimension::D2,
            self.px.iter().flatten().copied().collect(),
            TextureFormat::Rgba8UnormSrgb,
            usage,
        )
    }
}

/// The slices of one car, ground slice first, nose along +X.
#[derive(Clone, Debug, PartialEq)]
pub struct CarSlices {
    pub slices: Vec<Grid>,
    /// Length along +X in pixels.
    pub width: u32,
    /// Width across the car (along Y) in pixels.
    pub height: u32,
    /// World pixels per art pixel (1.0 for the procedural cars).
    pub scale: f32,
}

impl CarSlices {
    fn new(slices: Vec<Grid>, scale: f32) -> Self {
        let (width, height) = slices.first().map_or((0, 0), |g| (g.width, g.height));
        Self { slices, width, height, scale }
    }

    /// Top-down union of every slice, white where any slice is opaque.
    pub fn footprint(&self) -> Grid {
        let mut out = Grid::new(self.width, self.height);
        for s in &self.slices {
            for (o, p) in out.px.iter_mut().zip(&s.px) {
                if p[3] > 0 {
                    *o = [255; 4];
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Procedural voxel cars
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mat {
    Under,
    Body,
    Glass,
    Tire,
    Hub,
    Head,
    Tail,
    Wing,
    Trim,
}

struct Shape {
    len: i32,
    wid: i32,
    /// Front plan taper in px per side over the last quarter.
    taper: f32,
    /// Underbody slices below the body.
    ride: i32,
    /// Body slices above the underbody.
    body: i32,
    /// Slices the hood drops toward the nose.
    nose_drop: i32,
    /// Cabin footprint as fractions of the length (rear, front).
    cabin: (f32, f32),
    cabin_h: i32,
    cabin_inset: i32,
    /// Rear wing height above the rear deck, in slices.
    wing: Option<i32>,
    double_stripe: bool,
    scoop: bool,
    roof_rails: bool,
    roof_vent: bool,
}

fn shape(style: BodyStyle) -> Shape {
    let base = Shape {
        len: 30,
        wid: 16,
        taper: 1.0,
        ride: 1,
        body: 5,
        nose_drop: 1,
        cabin: (0.3, 0.66),
        cabin_h: 4,
        cabin_inset: 2,
        wing: None,
        double_stripe: false,
        scoop: false,
        roof_rails: false,
        roof_vent: false,
    };
    match style {
        // Short and tall, roof runs almost to the tail.
        BodyStyle::Hatch => Shape { len: 28, cabin: (0.1, 0.64), cabin_h: 5, ..base },
        // Smooth fastback.
        BodyStyle::Coupe => Shape { len: 31, taper: 1.5, cabin: (0.26, 0.64), ..base },
        // Raised, wide, wing and roof vent.
        BodyStyle::Rally => Shape {
            wid: 17,
            ride: 2,
            cabin: (0.22, 0.66),
            cabin_h: 5,
            wing: Some(2),
            roof_vent: true,
            ..base
        },
        // Low with a long hood and a lip spoiler.
        BodyStyle::Sports => Shape {
            len: 33,
            taper: 2.0,
            body: 4,
            nose_drop: 2,
            cabin: (0.32, 0.62),
            wing: Some(1),
            ..base
        },
        // Flat doorstop: sharp nose, tiny canopy, tall wing.
        BodyStyle::Wedge => Shape {
            len: 34,
            taper: 3.5,
            body: 4,
            nose_drop: 3,
            cabin: (0.34, 0.6),
            cabin_h: 4,
            cabin_inset: 3,
            wing: Some(3),
            ..base
        },
        // Long estate with roof rails.
        BodyStyle::Tourer => Shape {
            len: 35,
            wid: 17,
            taper: 1.5,
            cabin: (0.1, 0.7),
            cabin_h: 5,
            roof_rails: true,
            ..base
        },
        // Wide and tall, hood scoop and twin stripes.
        BodyStyle::Muscle => Shape {
            len: 34,
            wid: 18,
            body: 6,
            cabin: (0.3, 0.6),
            cabin_inset: 3,
            double_stripe: true,
            scoop: true,
            ..base
        },
    }
}

struct Voxels {
    len: i32,
    wid: i32,
    layers: i32,
    cells: Vec<Option<Mat>>,
}

impl Voxels {
    fn new(len: i32, wid: i32, layers: i32) -> Self {
        Self { len, wid, layers, cells: vec![None; (len * wid * layers) as usize] }
    }

    fn idx(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        let inside = (0..self.len).contains(&x) && (0..self.wid).contains(&y) && (0..self.layers).contains(&z);
        inside.then(|| ((z * self.wid + y) * self.len + x) as usize)
    }

    fn get(&self, x: i32, y: i32, z: i32) -> Option<Mat> {
        self.idx(x, y, z).and_then(|i| self.cells[i])
    }

    fn put(&mut self, x: i32, y: i32, z: i32, m: Mat) {
        if let Some(i) = self.idx(x, y, z) {
            self.cells[i] = Some(m);
        }
    }

    /// Fills only empty cells.
    fn fill(&mut self, x: i32, y: i32, z: i32, m: Mat) {
        if let Some(i) = self.idx(x, y, z)
            && self.cells[i].is_none()
        {
            self.cells[i] = Some(m);
        }
    }
}

fn shade(c: [u8; 3], f: f32) -> [u8; 4] {
    let m = |v: u8| (v as f32 * f).round().clamp(0.0, 255.0) as u8;
    [m(c[0]), m(c[1]), m(c[2]), 255]
}

fn build_voxels(sh: &Shape) -> Voxels {
    let (len, wid) = (sh.len, sh.wid);
    let deck = sh.ride + sh.body - 1;
    let cabin_top = deck + sh.cabin_h;
    let extra = sh.wing.unwrap_or(0).max(i32::from(sh.scoop || sh.roof_rails || sh.roof_vent));
    let mut v = Voxels::new(len, wid, (cabin_top + 1).max(deck + extra + 1) + 1);

    // Plan inset per side: rounded corners plus the front taper. Never below 1,
    // so the wheels stick out of the body.
    let inset = |x: i32| -> i32 {
        let q = len as f32 * 0.75;
        let taper = if x as f32 > q { (x as f32 - q) / (len as f32 - q) * sh.taper } else { 0.0 };
        let corner = if x <= 1 || x >= len - 2 { 2 } else { 1 };
        corner.max(1 + taper.round() as i32)
    };
    // Body top slice per x: the hood drops toward the nose, bumpers are rounded.
    let top = |x: i32| -> i32 {
        let front = len as f32 * 0.7;
        let drop = if x as f32 > front {
            ((x as f32 - front) / (len as f32 - front) * sh.nose_drop as f32).round() as i32
        } else {
            0
        };
        let bumper = i32::from(x == 0 || x == len - 1);
        (deck - drop - bumper).max(sh.ride)
    };

    // Wheels: dark tires with a light hub on the outer face.
    let wheel_h = sh.ride + 3;
    for ax in [(len as f32 * 0.22).round() as i32, (len as f32 * 0.76).round() as i32] {
        for z in 0..wheel_h {
            let reach = if z == 0 || z == wheel_h - 1 { 1 } else { 2 };
            for x in ax - reach..=ax + reach {
                for y in (0..3).chain(wid - 3..wid) {
                    let outer = y == 0 || y == wid - 1;
                    let hub = outer && (x - ax).abs() <= 1 && z > 0 && z < wheel_h - 1;
                    v.put(x, y, z, if hub { Mat::Hub } else { Mat::Tire });
                }
            }
        }
    }

    // Underbody.
    for z in 0..sh.ride {
        for x in 2..len - 2 {
            let i = inset(x) + 1;
            for y in i..wid - i {
                v.fill(x, y, z, Mat::Under);
            }
        }
    }

    // Body, with rounded shoulders on its top slice.
    for x in 0..len {
        let t = top(x);
        for z in sh.ride..=t {
            let i = inset(x) + i32::from(z == t && t > sh.ride);
            for y in i..wid - i {
                v.fill(x, y, z, Mat::Body);
            }
        }
    }

    // Head and tail lights one slice below the deck, plus a dark grille.
    let lights = |v: &mut Voxels, x: i32, z: i32, m: Mat, grille: bool| {
        let ys: Vec<i32> = (0..wid).filter(|&y| v.get(x, y, z) == Some(Mat::Body)).collect();
        let (Some(&lo), Some(&hi)) = (ys.first(), ys.last()) else { return };
        let span = if m == Mat::Head { 2 } else { 3 };
        for k in 1..=span {
            v.put(x, lo + k, z, m);
            v.put(x, hi - k, z, m);
        }
        if grille {
            for y in lo + span + 2..hi - span - 1 {
                v.put(x, y, z, Mat::Trim);
            }
        }
    };
    let (nose, tail) = (top(len - 1), top(0));
    lights(&mut v, len - 1, nose, Mat::Head, false);
    if nose > sh.ride {
        lights(&mut v, len - 1, nose - 1, Mat::Head, true);
    }
    lights(&mut v, 0, tail, Mat::Tail, false);

    // Cabin: glass ring with pillars, roof cap on top. The windshield rakes back.
    let (c0, c1) = ((len as f32 * sh.cabin.0).round() as i32, (len as f32 * sh.cabin.1).round() as i32);
    let mid = (c0 + c1) / 2;
    for k in 0..sh.cabin_h {
        let z = deck + 1 + k;
        let roof = k == sh.cabin_h - 1;
        let x0 = c0 + k / 2;
        let x1 = c1 - k;
        let i = sh.cabin_inset + i32::from(roof);
        for x in x0..=x1 {
            for y in i..wid - i {
                let ring_x = x == x0 || x == x1;
                let ring_y = y == i || y == wid - 1 - i;
                let pillar = ring_y && (ring_x || (x == mid && x1 - x0 > 8));
                let m = if roof || pillar { Mat::Body } else { Mat::Glass };
                v.put(x, y, z, m);
            }
        }
    }

    // Style details.
    let roof_z = cabin_top + 1;
    let roof_x = (c0 + sh.cabin_h / 2, c1 - sh.cabin_h);
    if sh.roof_rails {
        for x in roof_x.0..=roof_x.1 {
            v.put(x, sh.cabin_inset + 1, roof_z, Mat::Trim);
            v.put(x, wid - 2 - sh.cabin_inset, roof_z, Mat::Trim);
        }
    }
    if sh.roof_vent {
        let cx = (roof_x.0 + roof_x.1) / 2;
        for x in cx..cx + 2 {
            for y in wid / 2 - 1..=wid / 2 {
                v.put(x, y, roof_z, Mat::Trim);
            }
        }
    }
    if sh.scoop {
        let (s0, s1) = ((len as f32 * 0.68) as i32, (len as f32 * 0.8) as i32);
        for x in s0..=s1 {
            for y in wid / 2 - 2..wid / 2 + 2 {
                v.put(x, y, top(x) + 1, if x == s1 { Mat::Trim } else { Mat::Body });
            }
        }
    }
    if let Some(h) = sh.wing {
        let z = top(1) + h;
        for x in 0..2 {
            for y in 1..wid - 1 {
                v.put(x, y, z, Mat::Wing);
            }
        }
        for y in [4, wid - 5] {
            for zz in top(1) + 1..z {
                v.put(1, y, zz, Mat::Wing);
            }
        }
    }
    v
}

/// Colors one voxel; `exposed` means nothing sits on top of it.
fn voxel_color(m: Mat, z: i32, layers: i32, exposed: bool, stripe: bool, body: [u8; 3], accent: [u8; 3]) -> [u8; 4] {
    let height = z as f32 / (layers - 1).max(1) as f32;
    match m {
        Mat::Body if exposed && stripe => shade(accent, 1.0),
        Mat::Body if exposed => shade(body, 1.12),
        // Sides darken toward the ground to fake top lighting.
        Mat::Body => shade(body, 0.5 + 0.5 * height),
        Mat::Glass if exposed => [92, 124, 160, 255],
        Mat::Glass => [40, 56, 80, 255],
        Mat::Under => [26, 26, 32, 255],
        Mat::Tire if exposed => [48, 48, 54, 255],
        Mat::Tire => [22, 22, 27, 255],
        Mat::Hub => [168, 170, 178, 255],
        Mat::Head => [255, 244, 200, 255],
        Mat::Tail => [228, 50, 50, 255],
        Mat::Wing if exposed => shade(accent, 0.95),
        Mat::Wing => shade(accent, 0.6),
        Mat::Trim => [30, 30, 36, 255],
    }
}

/// Voxel model of a car as slices (ground slice first), nose along +X.
pub fn voxel_car(style: BodyStyle, body: [u8; 3], accent: [u8; 3]) -> Vec<Grid> {
    let sh = shape(style);
    let v = build_voxels(&sh);
    let stripe_rows: &[i32] = if sh.double_stripe {
        &[sh.wid / 2 - 3, sh.wid / 2 - 2, sh.wid / 2 + 1, sh.wid / 2 + 2]
    } else {
        &[sh.wid / 2 - 1, sh.wid / 2]
    };
    let mut slices = Vec::new();
    for z in 0..v.layers {
        let mut g = Grid::new(v.len as u32, v.wid as u32);
        let mut emissive = vec![false; g.px.len()];
        for y in 0..v.wid {
            for x in 0..v.len {
                let Some(m) = v.get(x, y, z) else { continue };
                let exposed = v.get(x, y, z + 1).is_none();
                let c = voxel_color(m, z, v.layers, exposed, stripe_rows.contains(&y), body, accent);
                // Grid rows run top to bottom; world +Y is the top row.
                let row = (v.wid - 1 - y) as u32;
                g.set(x as u32, row, c);
                emissive[(row * g.width + x as u32) as usize] = matches!(m, Mat::Head | Mat::Tail | Mat::Hub);
            }
        }
        outline(&mut g, &emissive, 0.72);
        slices.push(g);
    }
    while slices.last().is_some_and(Grid::is_empty) {
        slices.pop();
    }
    slices
}

/// Darkens opaque pixels that border transparency (except `keep` pixels).
fn outline(g: &mut Grid, keep: &[bool], factor: f32) {
    let src = g.clone();
    for y in 0..g.height {
        for x in 0..g.width {
            let i = (y * g.width + x) as usize;
            let p = src.px[i];
            if p[3] == 0 || keep[i] {
                continue;
            }
            let (xi, yi) = (x as i64, y as i64);
            let edge = !(src.solid(xi - 1, yi) && src.solid(xi + 1, yi) && src.solid(xi, yi - 1) && src.solid(xi, yi + 1));
            if edge {
                g.px[i] = shade([p[0], p[1], p[2]], factor);
                g.px[i][3] = p[3];
            }
        }
    }
}

/// Procedural slices for `car` in `paint`.
pub fn procedural_car(car: CarId, paint: PaintId) -> CarSlices {
    let spec = car_spec(car);
    let (body, accent) = paint.resolve_colors(&spec);
    CarSlices::new(voxel_car(spec.body, body, accent), 1.0)
}

/// Slices for `car`: drop-in art when present and valid, else the procedural
/// model in `paint`. Paint does not tint drop-in art.
pub fn car_slices(car: CarId, paint: PaintId) -> CarSlices {
    load_override(car.key()).unwrap_or_else(|| procedural_car(car, paint))
}

// ---------------------------------------------------------------------------
// Drop-in slice strips
// ---------------------------------------------------------------------------

/// Direction the car's nose points inside one slice image.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Facing {
    #[default]
    Up,
    Down,
    Right,
    Left,
}

/// Sidecar options (`<car-key>.toml`).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StripMeta {
    /// Number of slices in the strip; default: square slices.
    pub slices: Option<u32>,
    pub facing: Facing,
    pub scale: f32,
}

impl Default for StripMeta {
    fn default() -> Self {
        Self { slices: None, facing: Facing::Up, scale: 1.0 }
    }
}

pub fn parse_meta(text: &str) -> Result<StripMeta, String> {
    let meta: StripMeta = toml::from_str(text).map_err(|e| e.to_string())?;
    if !(meta.scale.is_finite() && meta.scale > 0.0) {
        return Err(format!("scale must be positive, got {}", meta.scale));
    }
    Ok(meta)
}

/// Splits a horizontal strip (ground slice on the left) into slices.
pub fn split_strip(strip: &Grid, slices: Option<u32>) -> Result<Vec<Grid>, String> {
    if strip.width == 0 || strip.height == 0 {
        return Err("empty image".into());
    }
    let n = match slices {
        Some(0) => return Err("slices must be at least 1".into()),
        Some(n) => n,
        None => strip.width / strip.height,
    };
    if n == 0 || !strip.width.is_multiple_of(n) {
        return Err(format!(
            "width {} is not a multiple of {} slices (set `slices` in the sidecar)",
            strip.width,
            n.max(1)
        ));
    }
    let w = strip.width / n;
    let mut out: Vec<Grid> = (0..n).map(|i| strip.crop(i * w, 0, w, strip.height)).collect();
    while out.last().is_some_and(Grid::is_empty) {
        out.pop();
    }
    if out.is_empty() {
        return Err("every slice is transparent".into());
    }
    Ok(out)
}

/// Rotates slices so the nose points along +X (right in the image).
pub fn normalize(slices: Vec<Grid>, facing: Facing) -> Vec<Grid> {
    slices
        .into_iter()
        .map(|g| match facing {
            Facing::Right => g,
            Facing::Up => g.rotate_cw(),
            Facing::Down => g.rotate_ccw(),
            Facing::Left => g.rotate_180(),
        })
        .collect()
}

pub fn strip_to_car(strip: &Grid, meta: StripMeta) -> Result<CarSlices, String> {
    let slices = normalize(split_strip(strip, meta.slices)?, meta.facing);
    Ok(CarSlices::new(slices, meta.scale))
}

fn decode_png(bytes: &[u8]) -> Result<Grid, String> {
    let image = Image::from_buffer(
        bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::Default,
        RenderAssetUsages::MAIN_WORLD,
    )
    .map_err(|e| e.to_string())?;
    let image = image
        .convert(TextureFormat::Rgba8UnormSrgb)
        .ok_or("unsupported pixel format")?;
    let size = image.size();
    let data = image.data.ok_or("no pixel data")?;
    Ok(Grid {
        width: size.x,
        height: size.y,
        px: data.as_chunks::<4>().0.to_vec(),
    })
}

/// Override directories in search order.
pub fn override_dirs(
    xdg_data_home: Option<PathBuf>,
    home: Option<PathBuf>,
    exe_dir: Option<PathBuf>,
    cwd: Option<PathBuf>,
) -> Vec<PathBuf> {
    let abs = |p: Option<PathBuf>| p.filter(|p| p.is_absolute());
    let mut dirs: Vec<PathBuf> = [
        abs(xdg_data_home).map(|d| d.join("hairpin-havoc/cars")),
        abs(home).map(|h| h.join(".local/share/hairpin-havoc/cars")),
        exe_dir.map(|d| d.join("assets/cars")),
        cwd.map(|d| d.join("assets/cars")),
    ]
    .into_iter()
    .flatten()
    .collect();
    dirs.dedup();
    dirs
}

fn env_override_dirs() -> Vec<PathBuf> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    override_dirs(env("XDG_DATA_HOME"), env("HOME"), exe_dir, std::env::current_dir().ok())
}

fn read_override(png: &Path) -> Result<CarSlices, String> {
    let bytes = std::fs::read(png).map_err(|e| e.to_string())?;
    let meta = match std::fs::read_to_string(png.with_extension("toml")) {
        Ok(text) => parse_meta(&text)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => StripMeta::default(),
        Err(e) => return Err(e.to_string()),
    };
    strip_to_car(&decode_png(&bytes)?, meta)
}

/// Drop-in art for `key`, from the first directory that has `<key>.png`.
/// Invalid files log a warning and yield `None` (procedural fallback).
pub fn load_override(key: &str) -> Option<CarSlices> {
    let png = env_override_dirs()
        .into_iter()
        .map(|d| d.join(format!("{key}.png")))
        .find(|p| p.is_file())?;
    read_override(&png)
        .inspect_err(|e| warn!("ignoring car art {}: {e}", png.display()))
        .ok()
}

// ---------------------------------------------------------------------------
// CPU compositing (menu previews, translucent cars)
// ---------------------------------------------------------------------------

/// Size of a composited stack and where the ground center lands in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewLayout {
    pub width: u32,
    pub height: u32,
    /// Ground center, in pixels from the left and from the top.
    pub cx: f32,
    pub cy: f32,
}

/// The layout fits every heading, so rotating a preview never resizes it.
pub fn preview_layout(w: u32, h: u32, n: usize, squash: f32, lift: f32) -> PreviewLayout {
    let diag = ((w * w + h * h) as f32).sqrt().ceil();
    let rise = n.saturating_sub(1) as f32 * lift;
    let ry = (diag * squash / 2.0).ceil();
    // One pixel margin on each side for the outline.
    let width = diag as u32 + 2;
    let height = (2.0 * ry + rise).ceil() as u32 + 2;
    PreviewLayout { width, height, cx: width as f32 / 2.0, cy: 1.0 + rise.ceil() + ry }
}

/// Composites the stack at `angle` (radians, CCW), bottom slice first, with a
/// dark 1 px silhouette outline.
pub fn render_stack_preview(slices: &[Grid], angle: f32, squash: f32, lift: f32) -> Grid {
    let Some(first) = slices.first() else { return Grid::new(1, 1) };
    let (w, h) = (first.width, first.height);
    let lay = preview_layout(w, h, slices.len(), squash, lift);
    let mut out = Grid::new(lay.width, lay.height);
    let (sin, cos) = (-angle).sin_cos();
    for (i, s) in slices.iter().enumerate() {
        for oy in 0..lay.height {
            for ox in 0..lay.width {
                let sx = ox as f32 + 0.5 - lay.cx;
                // Screen offset from this slice's lifted center, unsquashed.
                let sy = (lay.cy - (oy as f32 + 0.5) - i as f32 * lift) / squash;
                // Inverse rotation back into slice space.
                let lx = sx * cos - sy * sin;
                let ly = sx * sin + sy * cos;
                let col = (lx + w as f32 / 2.0).floor();
                let row = (h as f32 / 2.0 - ly).floor();
                if col < 0.0 || row < 0.0 || col >= w as f32 || row >= h as f32 {
                    continue;
                }
                let p = s.get(col as u32, row as u32);
                if p[3] > 0 {
                    out.set(ox, oy, blend(out.get(ox, oy), p));
                }
            }
        }
    }
    let src = out.clone();
    for y in 0..out.height {
        for x in 0..out.width {
            let (xi, yi) = (x as i64, y as i64);
            let near = src.solid(xi - 1, yi) || src.solid(xi + 1, yi) || src.solid(xi, yi - 1) || src.solid(xi, yi + 1);
            if src.get(x, y)[3] == 0 && near {
                out.set(x, y, PREVIEW_OUTLINE);
            }
        }
    }
    out
}

fn blend(dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
    if src[3] == 255 || dst[3] == 0 {
        return src;
    }
    let a = src[3] as f32 / 255.0;
    let mix = |s: u8, d: u8| (s as f32 * a + d as f32 * (1.0 - a)).round() as u8;
    let alpha = (src[3] as f32 + dst[3] as f32 * (1.0 - a)).round().min(255.0) as u8;
    [mix(src[0], dst[0]), mix(src[1], dst[1]), mix(src[2], dst[2]), alpha]
}

/// Menu preview image of `car` at `angle`, keeping CPU data so it can be re-rendered.
pub fn preview_image(car: &CarSlices, angle: f32) -> Image {
    render_stack_preview(&car.slices, angle, SQUASH, LIFT)
        .to_image(RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD)
}

/// Index into the pre-rendered headings for `heading`.
pub fn turn_index(heading: f32, turns: usize) -> usize {
    ((heading / TAU * turns as f32).round() as i64).rem_euclid(turns as i64) as usize
}

// ---------------------------------------------------------------------------
// ECS
// ---------------------------------------------------------------------------

/// GPU images of one car, ready to spawn as a stack.
#[derive(Clone)]
pub struct StackArt {
    slices: Vec<Handle<Image>>,
    shadow: Handle<Image>,
    turns: Vec<Handle<Image>>,
    /// Offset from the ground center to the center of a turn image (art px).
    turn_offset: Vec2,
    scale: f32,
    half_length: f32,
}

impl StackArt {
    pub fn new(car: &CarSlices, images: &mut Assets<Image>) -> Self {
        let usage = RenderAssetUsages::RENDER_WORLD;
        let lay = preview_layout(car.width, car.height, car.slices.len(), SQUASH, LIFT);
        Self {
            slices: car.slices.iter().map(|g| images.add(g.to_image(usage))).collect(),
            shadow: images.add(car.footprint().to_image(usage)),
            turns: (0..TURNS)
                .map(|i| {
                    let angle = i as f32 / TURNS as f32 * TAU;
                    images.add(render_stack_preview(&car.slices, angle, SQUASH, LIFT).to_image(usage))
                })
                .collect(),
            turn_offset: Vec2::new(lay.width as f32 / 2.0 - lay.cx, lay.cy - lay.height as f32 / 2.0),
            scale: car.scale,
            half_length: car.width as f32 * car.scale / 2.0,
        }
    }
}

/// Where a stacked car points and how opaque it is. Alpha below 1 draws the
/// pre-rendered composite instead of the slices (overlapping translucent
/// slices would otherwise add up to an opaque core).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct StackPose {
    pub heading: f32,
    pub alpha: f32,
}

impl Default for StackPose {
    fn default() -> Self {
        Self { heading: 0.0, alpha: 1.0 }
    }
}

/// Child entities of a stacked car; lives on the car's root entity.
#[derive(Component)]
pub struct CarStack {
    slices: Vec<Entity>,
    shadow: Entity,
    flat: Entity,
    turns: Vec<Handle<Image>>,
    /// Distance from the center to the tail, in world px.
    pub half_length: f32,
    /// Screen height of the stack, in px.
    pub height: f32,
}

#[derive(Component)]
struct StackPart;

/// Spawns the stack as children of `root` (which keeps translation only) and
/// inserts `CarStack` and `pose` on it. Children get `WORLD_LAYER`.
pub fn spawn_stack(commands: &mut Commands, root: Entity, art: &StackArt, pose: StackPose) {
    let s = art.scale;
    let mut slices = Vec::with_capacity(art.slices.len());
    let mut shadow = Entity::PLACEHOLDER;
    let mut flat = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|c| {
        c.spawn((
            Transform::from_scale(Vec3::new(s, SQUASH * s, 1.0)),
            Visibility::default(),
            WORLD_LAYER,
        ))
        .with_children(|n| {
            shadow = n
                .spawn((
                    StackPart,
                    Sprite { image: art.shadow.clone(), color: Color::BLACK.with_alpha(SHADOW_ALPHA), ..default() },
                    Transform::from_xyz(SHADOW_OFFSET.x / s, SHADOW_OFFSET.y / (SQUASH * s), -0.002),
                    WORLD_LAYER,
                ))
                .id();
            for (i, img) in art.slices.iter().enumerate() {
                let tf = Transform::from_xyz(0.0, i as f32 * LIFT / SQUASH, i as f32 * 0.001);
                slices.push(n.spawn((StackPart, Sprite::from_image(img.clone()), tf, WORLD_LAYER)).id());
            }
        });
        flat = c
            .spawn((
                StackPart,
                Sprite::from_image(art.turns[0].clone()),
                Transform::from_translation((art.turn_offset * s).extend(0.0)).with_scale(Vec3::new(s, s, 1.0)),
                Visibility::Hidden,
                WORLD_LAYER,
            ))
            .id();
    });
    let height = art.slices.len() as f32 * LIFT * s;
    commands.entity(root).insert((
        CarStack { slices, shadow, flat, turns: art.turns.clone(), half_length: art.half_length, height },
        pose,
    ));
}

pub struct StackPlugin;

impl Plugin for StackPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, apply_pose.before(TransformSystems::Propagate));
    }
}

type PartItems<'w, 's> =
    Query<'w, 's, (&'static mut Transform, &'static mut Sprite, &'static mut Visibility), With<StackPart>>;

/// Stacks whose pose changed or that were just spawned.
type ChangedStacks<'w, 's> =
    Query<'w, 's, (&'static StackPose, &'static CarStack), Or<(Changed<StackPose>, Added<CarStack>)>>;

fn apply_pose(
    stacks: ChangedStacks,
    mut parts: PartItems,
) {
    for (pose, stack) in &stacks {
        let rot = Quat::from_rotation_z(pose.heading);
        let solid = pose.alpha >= 0.999;
        for &e in &stack.slices {
            if let Ok((mut tf, _, mut vis)) = parts.get_mut(e) {
                tf.rotation = rot;
                vis.set_if_neq(if solid { Visibility::Inherited } else { Visibility::Hidden });
            }
        }
        if let Ok((mut tf, mut sprite, _)) = parts.get_mut(stack.shadow) {
            tf.rotation = rot;
            sprite.color.set_alpha(SHADOW_ALPHA * pose.alpha);
        }
        if let Ok((_, mut sprite, mut vis)) = parts.get_mut(stack.flat) {
            vis.set_if_neq(if solid { Visibility::Hidden } else { Visibility::Inherited });
            if !solid {
                let img = &stack.turns[turn_index(pose.heading, stack.turns.len())];
                if sprite.image != *img {
                    sprite.image = img.clone();
                }
                sprite.color = Color::WHITE.with_alpha(pose.alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLES: [BodyStyle; 7] = [
        BodyStyle::Hatch,
        BodyStyle::Coupe,
        BodyStyle::Rally,
        BodyStyle::Sports,
        BodyStyle::Wedge,
        BodyStyle::Tourer,
        BodyStyle::Muscle,
    ];

    fn opaque(g: &Grid) -> usize {
        g.px.iter().filter(|p| p[3] > 0).count()
    }

    /// A 3x2 grid with a distinct pixel at each position.
    fn marked() -> Grid {
        let mut g = Grid::new(3, 2);
        for (i, p) in g.px.iter_mut().enumerate() {
            *p = [i as u8 + 1, 0, 0, 255];
        }
        g
    }

    #[test]
    fn every_style_is_a_tall_solid_stack() {
        for style in STYLES {
            let slices = voxel_car(style, [200, 50, 50], [240, 240, 240]);
            assert!((9..=15).contains(&slices.len()), "{style:?} has {} slices", slices.len());
            let (w, h) = (slices[0].width, slices[0].height);
            assert!((28..=35).contains(&w) && (16..=18).contains(&h), "{style:?} is {w}x{h}");
            assert!(slices.iter().all(|s| s.width == w && s.height == h));
            // The body slices cover most of the footprint.
            let body = &slices[3];
            assert!(opaque(body) * 2 > (w * h) as usize, "{style:?} body slice mostly empty");
            assert!(slices.iter().all(|s| !s.is_empty()));
        }
    }

    #[test]
    fn styles_look_different() {
        let cars: Vec<Vec<Grid>> = STYLES.iter().map(|&s| voxel_car(s, [200, 50, 50], [240, 240, 240])).collect();
        for i in 0..cars.len() {
            for j in i + 1..cars.len() {
                assert_ne!(cars[i], cars[j], "{:?} == {:?}", STYLES[i], STYLES[j]);
            }
        }
    }

    #[test]
    fn slices_have_a_darker_outline_and_bright_lights() {
        let body = [200, 100, 50];
        let slices = voxel_car(BodyStyle::Coupe, body, [240, 240, 240]);
        let top = slices.iter().rev().find(|s| opaque(s) > 20).unwrap();
        // Find a pixel on the left edge of a row; it must be darker than the body top color.
        let row = top.height / 2;
        let x = (0..top.width).find(|&x| top.get(x, row)[3] > 0).unwrap();
        let edge = top.get(x, row);
        assert!(edge[0] < shade(body, 1.12)[0]);
        // Headlights sit at the nose.
        let lit = slices.iter().any(|s| (0..s.height).any(|y| s.get(s.width - 1, y) == [255, 244, 200, 255]));
        assert!(lit);
    }

    #[test]
    fn paint_changes_the_car() {
        let a = procedural_car(CarId::Pebble, PaintId::Factory);
        let b = procedural_car(CarId::Pebble, PaintId::ALL[2]);
        assert_eq!((a.width, a.height), (b.width, b.height));
        assert_ne!(a, b);
    }

    #[test]
    fn rotations_move_pixels_as_expected() {
        let g = marked();
        let cw = g.rotate_cw();
        assert_eq!((cw.width, cw.height), (2, 3));
        // Top-left goes to top-right.
        assert_eq!(cw.get(1, 0), g.get(0, 0));
        // Bottom-left goes to top-left.
        assert_eq!(cw.get(0, 0), g.get(0, 1));
        let ccw = g.rotate_ccw();
        // Top-left goes to bottom-left.
        assert_eq!(ccw.get(0, 2), g.get(0, 0));
        assert_eq!(g.rotate_cw().rotate_ccw(), g);
        assert_eq!(g.rotate_180().rotate_180(), g);
        assert_eq!(g.rotate_cw().rotate_cw(), g.rotate_180());
    }

    #[test]
    fn strips_split_into_square_slices_and_drop_empty_tops() {
        let mut strip = Grid::new(8, 2);
        // Slices 0 and 1 have content, 2 and 3 are empty.
        strip.set(0, 0, [9, 9, 9, 255]);
        strip.set(3, 1, [8, 8, 8, 255]);
        let s = split_strip(&strip, None).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].get(0, 0), [9, 9, 9, 255]);
        assert_eq!(s[1].get(1, 1), [8, 8, 8, 255]);
        // Explicit count with non-square slices.
        assert_eq!(split_strip(&strip, Some(2)).unwrap().len(), 1);
        assert!(split_strip(&strip, Some(3)).is_err());
        assert!(split_strip(&strip, Some(0)).is_err());
        assert!(split_strip(&Grid::new(4, 2), None).is_err());
    }

    #[test]
    fn upward_art_is_turned_to_face_plus_x() {
        // A 2x4 slice whose nose (marked) is the top row.
        let mut slice = Grid::new(2, 4);
        slice.set(0, 0, [1, 2, 3, 255]);
        slice.set(1, 0, [1, 2, 3, 255]);
        slice.set(0, 3, [7, 7, 7, 255]);
        let car = strip_to_car(&slice, StripMeta { slices: Some(1), ..StripMeta::default() }).unwrap();
        assert_eq!((car.width, car.height), (4, 2));
        let s = &car.slices[0];
        // The nose ends up in the rightmost column.
        assert_eq!(s.get(3, 0), [1, 2, 3, 255]);
        assert_eq!(s.get(3, 1), [1, 2, 3, 255]);
        let right = normalize(vec![slice.clone()], Facing::Right);
        assert_eq!(right[0], slice);
        let left = normalize(vec![slice.clone()], Facing::Left);
        assert_eq!(left[0], slice.rotate_180());
    }

    #[test]
    fn sidecar_parses_and_validates() {
        assert_eq!(parse_meta("").unwrap(), StripMeta::default());
        let m = parse_meta("slices = 12\nfacing = \"right\"\nscale = 0.5\n").unwrap();
        assert_eq!(m, StripMeta { slices: Some(12), facing: Facing::Right, scale: 0.5 });
        assert!(parse_meta("facing = \"sideways\"").is_err());
        assert!(parse_meta("scale = 0").is_err());
        assert!(parse_meta("colour = 3").is_err());
    }

    #[test]
    fn override_dirs_follow_search_order() {
        let dirs = override_dirs(
            Some("/xdg".into()),
            Some("/home/u".into()),
            Some("/opt/game".into()),
            Some("/work".into()),
        );
        let want: Vec<PathBuf> = [
            "/xdg/hairpin-havoc/cars",
            "/home/u/.local/share/hairpin-havoc/cars",
            "/opt/game/assets/cars",
            "/work/assets/cars",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        assert_eq!(dirs, want);
        // Relative env paths are ignored.
        assert_eq!(override_dirs(Some("rel".into()), None, None, None), Vec::<PathBuf>::new());
    }

    #[test]
    fn preview_layout_fits_every_heading() {
        let car = procedural_car(CarId::Baron, PaintId::Factory);
        let first = render_stack_preview(&car.slices, 0.0, SQUASH, LIFT);
        for k in 0..16 {
            let g = render_stack_preview(&car.slices, k as f32 * TAU / 16.0, SQUASH, LIFT);
            assert_eq!((g.width, g.height), (first.width, first.height));
            // Nothing touches the border except the outline ring.
            for x in 0..g.width {
                for y in [0, g.height - 1] {
                    let p = g.get(x, y);
                    assert!(p[3] == 0 || p == PREVIEW_OUTLINE);
                }
            }
        }
    }

    #[test]
    fn preview_stacks_upward_and_squashes() {
        // Three 4x2 slices, each a solid color; the top slice must be highest.
        let colors = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]];
        let slices: Vec<Grid> = colors
            .iter()
            .map(|&c| Grid { width: 4, height: 2, px: vec![c; 8] })
            .collect();
        let g = render_stack_preview(&slices, 0.0, 1.0, 1.0);
        let rows_with = |c: [u8; 4]| -> Vec<u32> {
            (0..g.height).filter(|&y| (0..g.width).any(|x| g.get(x, y) == c)).collect()
        };
        let red = rows_with(colors[0]);
        let blue = rows_with(colors[2]);
        // Bottom slice peeks out below; top slice covers the highest rows.
        assert_eq!(red.len(), 1);
        assert_eq!(blue.len(), 2);
        assert_eq!(red.last().unwrap() - blue.last().unwrap(), 2);
        let lay = preview_layout(4, 2, 3, 1.0, 1.0);
        assert_eq!((g.width, g.height), (lay.width, lay.height));
    }

    #[test]
    fn turn_index_wraps() {
        assert_eq!(turn_index(0.0, 64), 0);
        assert_eq!(turn_index(TAU, 64), 0);
        assert_eq!(turn_index(-TAU / 64.0, 64), 63);
        assert_eq!(turn_index(TAU / 4.0, 64), 16);
    }
}
