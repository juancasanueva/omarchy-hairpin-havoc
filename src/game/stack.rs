//! Sprite-stacked cars: a car is a stack of horizontal slices (ground slice
//! first). Every slice is rotated by the car heading, the stack is squashed
//! vertically and each slice is lifted one pixel above the previous one, so at
//! the game's pixel scale the slices read as one solid 3D car.
//!
//! Stacks are baked on the CPU: each car is pre-rendered at `HEADINGS`
//! headings when a race starts (one outline around each finished composite,
//! never per slice), and drawn as a single sprite showing the frame nearest to
//! its heading, plus a baked ground shadow. Whole cars are y-sorted by their
//! ground point, so slices of two cars never interleave.
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

/// Vertical squash of the whole stack (fakes the camera tilt). Used by the
/// race cars, the ghost, the menu previews, the shadows and the FX offsets.
/// 0.88 was picked over 0.75: the track is drawn straight down, so a mild
/// squash keeps cars' plan shape consistent with the unsquashed road, while
/// 0.75 made sideways cars look stubby next to cars driving up/down.
pub const SQUASH: f32 = 0.88;
/// Screen pixels each slice sits above the previous one (rounded per slice).
pub const SPACING: f32 = 1.0;
/// Headings baked per car. 128 keeps the nose of a 36 px car moving under one
/// pixel per frame step while turning (64 jumps ~1.8 px); a car's frames take
/// about 2 MB of GPU memory.
pub const HEADINGS: usize = 128;
/// Heading of the menu previews (nose toward the viewer, right side visible).
pub const PREVIEW_ANGLE: f32 = -35.0 * std::f32::consts::PI / 180.0;

const SHADOW_ALPHA: f32 = 0.3;
/// Shadow offset from the car, in art pixels: 2 right, 3 down.
const SHADOW_OFFSET: Vec2 = Vec2::new(2.0, -3.0);
/// World z of every car shadow: above skids and slicks, below item boxes and cars.
const SHADOW_Z: f32 = 14.0;
/// Outline traced around each finished composite.
const OUTLINE: [u8; 4] = [14, 14, 18, 255];

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
        for y in 0..v.wid {
            for x in 0..v.len {
                let Some(m) = v.get(x, y, z) else { continue };
                let exposed = v.get(x, y, z + 1).is_none();
                let c = voxel_color(m, z, v.layers, exposed, stripe_rows.contains(&y), body, accent);
                // Grid rows run top to bottom; world +Y is the top row.
                let row = (v.wid - 1 - y) as u32;
                g.set(x as u32, row, c);
            }
        }
        slices.push(g);
    }
    while slices.last().is_some_and(Grid::is_empty) {
        slices.pop();
    }
    slices
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

/// Direction the car's nose points inside one slice image. Strips from
/// common sprite-stack tools have the nose pointing down (+Y in the image).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Facing {
    Up,
    #[default]
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
        Self { slices: None, facing: Facing::Down, scale: 1.0 }
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
// CPU compositing
// ---------------------------------------------------------------------------

/// Screen pixels slice `i` sits above the ground slice (whole pixels only, so
/// slices never smear across rows).
pub fn lift(i: usize, spacing: f32) -> i32 {
    (i as f32 * spacing).round() as i32
}

/// Fixed canvas that fits a stack of `count` slices of `w` x `h` px at every
/// heading. The ground point sits exactly at the canvas centre and both sides
/// are even, so the canvas centre falls on a pixel corner (like the
/// `(w-1)/2` centre of even-sized slices) and nothing moves between headings.
/// One pixel of margin on every side leaves room for the outline.
pub fn stack_canvas(w: u32, h: u32, count: usize, squash: f32, spacing: f32) -> UVec2 {
    let r = ((w * w + h * h) as f32).sqrt() / 2.0;
    let rise = lift(count.saturating_sub(1), spacing).max(0) as f32;
    let half_w = r.ceil() as u32 + 1;
    let half_h = (r * squash + rise).ceil() as u32 + 1;
    UVec2::new(2 * half_w, 2 * half_h)
}

/// Composites the stack at `angle` (radians, CCW, 0 = nose along +X) into
/// `canvas`, without an outline. Every slice is rotated about its symmetric
/// centre `((w-1)/2, (h-1)/2)`, squashed vertically and lifted
/// `lift(i, spacing)` px; each canvas pixel inverse-maps into the slices
/// (rounded nearest sampling), top slice first, until it is opaque.
///
/// Odd-sized slices are sampled as if padded with one transparent column at
/// the tail (left) and/or one row at the bottom: an even-sized slice rotates
/// about a pixel corner like the even canvas, so rotated pixels land on whole
/// canvas pixels instead of rounding ties (no wobble between headings).
pub fn composite_stack(slices: &[Grid], angle: f32, squash: f32, spacing: f32, canvas: UVec2) -> Grid {
    let mut out = Grid::new(canvas.x, canvas.y);
    let Some(first) = slices.first() else { return out };
    let pad_x = (first.width % 2) as f32;
    let (cx, cy) = (
        (first.width as f32 + pad_x - 1.0) * 0.5,
        (first.height as f32 + (first.height % 2) as f32 - 1.0) * 0.5,
    );
    let (gx, gy) = ((canvas.x as f32 - 1.0) * 0.5, (canvas.y as f32 - 1.0) * 0.5);
    let (sin, cos) = angle.sin_cos();
    let lifts: Vec<f32> = (0..slices.len()).map(|i| lift(i, spacing) as f32).collect();
    for oy in 0..canvas.y {
        // Screen offset from the ground point, +Y up.
        let dy = gy - oy as f32;
        for ox in 0..canvas.x {
            let u = ox as f32 - gx;
            let mut acc = [0u8; 4];
            for (s, &l) in slices.iter().zip(&lifts).rev() {
                // Undo the lift and the squash, then the rotation.
                let v = (dy - l) / squash;
                let lx = u * cos + v * sin;
                let ly = v * cos - u * sin;
                let col = (cx + lx).round() - pad_x;
                let row = (cy - ly).round();
                if col < 0.0 || row < 0.0 || col >= s.width as f32 || row >= s.height as f32 {
                    continue;
                }
                let p = s.get(col as u32, row as u32);
                if p[3] == 0 {
                    continue;
                }
                // `acc` is in front of `p`.
                acc = blend(p, acc);
                if acc[3] == 255 {
                    break;
                }
            }
            out.set(ox, oy, acc);
        }
    }
    out
}

/// Traces a 1 px outline around the opaque area: every transparent pixel next
/// to an opaque one (4-neighbourhood) becomes `color`. Run it once on the
/// finished composite, never per slice, so no seams appear inside the body.
pub fn outline(g: &Grid, color: [u8; 4]) -> Grid {
    let mut out = g.clone();
    for y in 0..g.height {
        for x in 0..g.width {
            let (xi, yi) = (x as i64, y as i64);
            let near = g.solid(xi - 1, yi) || g.solid(xi + 1, yi) || g.solid(xi, yi - 1) || g.solid(xi, yi + 1);
            if g.get(x, y)[3] == 0 && near {
                out.set(x, y, color);
            }
        }
    }
    out
}

/// The finished stack at `angle`: composite plus one outline, on the car's
/// fixed canvas (see [`stack_canvas`]).
pub fn render_stack(slices: &[Grid], angle: f32, squash: f32, spacing: f32) -> Grid {
    let Some(first) = slices.first() else { return Grid::new(2, 2) };
    let canvas = stack_canvas(first.width, first.height, slices.len(), squash, spacing);
    outline(&composite_stack(slices, angle, squash, spacing, canvas), OUTLINE)
}

/// Ground shadow at `angle`: the white footprint, rotated and squashed like
/// the ground slice, on a canvas centred on the ground point.
pub fn render_shadow(footprint: &Grid, angle: f32, squash: f32) -> Grid {
    let canvas = stack_canvas(footprint.width, footprint.height, 1, squash, 0.0);
    composite_stack(std::slice::from_ref(footprint), angle, squash, 0.0, canvas)
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
    render_stack(&car.slices, angle, SQUASH, SPACING).to_image(RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD)
}

/// Angle of baked frame `i` out of `n`.
pub fn frame_angle(i: usize, n: usize) -> f32 {
    i as f32 / n as f32 * TAU
}

/// Baked frame closest to `heading`: `round(heading / TAU * n) mod n`.
pub fn frame_index(heading: f32, n: usize) -> usize {
    ((heading / TAU * n as f32).round() as i64).rem_euclid(n as i64) as usize
}

// ---------------------------------------------------------------------------
// ECS
// ---------------------------------------------------------------------------

/// Baked GPU frames of one car: `HEADINGS` stack images and matching ground
/// shadows. The handles are strong, so the images are freed once the last
/// car (or `StackArt`) using them is gone.
#[derive(Clone)]
pub struct StackArt {
    frames: Vec<Handle<Image>>,
    shadows: Vec<Handle<Image>>,
    scale: f32,
    half_length: f32,
    height: f32,
}

impl StackArt {
    pub fn new(car: &CarSlices, images: &mut Assets<Image>) -> Self {
        let usage = RenderAssetUsages::RENDER_WORLD;
        let footprint = car.footprint();
        let (frames, shadows) = (0..HEADINGS)
            .map(|i| {
                let angle = frame_angle(i, HEADINGS);
                let body = render_stack(&car.slices, angle, SQUASH, SPACING).to_image(usage);
                let shadow = render_shadow(&footprint, angle, SQUASH).to_image(usage);
                (images.add(body), images.add(shadow))
            })
            .unzip();
        Self {
            frames,
            shadows,
            scale: car.scale,
            half_length: car.width as f32 * car.scale / 2.0,
            height: car.slices.len() as f32 * SPACING * car.scale,
        }
    }
}

/// Where a stacked car points and how opaque it is (alpha applies to the one
/// baked sprite, so translucent cars never show their inner slices).
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

/// The car sprite and its shadow; lives on the car's root entity.
#[derive(Component)]
pub struct CarStack {
    body: Entity,
    shadow: Entity,
    frames: Vec<Handle<Image>>,
    shadows: Vec<Handle<Image>>,
    /// Distance from the center to the tail, in world px.
    pub half_length: f32,
    /// Screen height of the stack, in px.
    pub height: f32,
}

#[derive(Component)]
struct StackPart;

/// Spawns the car sprite and its shadow as children of `root` (which keeps
/// translation only) and inserts `CarStack` and `pose` on it. Children get
/// `WORLD_LAYER`.
pub fn spawn_stack(commands: &mut Commands, root: Entity, art: &StackArt, pose: StackPose) {
    let s = art.scale;
    let i = frame_index(pose.heading, art.frames.len());
    let scale = Vec3::new(s, s, 1.0);
    let mut body = Entity::PLACEHOLDER;
    let mut shadow = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|c| {
        shadow = c
            .spawn((
                StackPart,
                Sprite { image: art.shadows[i].clone(), color: shadow_color(pose.alpha), ..default() },
                Transform::from_translation((SHADOW_OFFSET * s).extend(-1.0)).with_scale(scale),
                WORLD_LAYER,
            ))
            .id();
        body = c
            .spawn((
                StackPart,
                Sprite { image: art.frames[i].clone(), color: Color::WHITE.with_alpha(pose.alpha), ..default() },
                Transform::from_scale(scale),
                WORLD_LAYER,
            ))
            .id();
    });
    commands.entity(root).insert((
        CarStack {
            body,
            shadow,
            frames: art.frames.clone(),
            shadows: art.shadows.clone(),
            half_length: art.half_length,
            height: art.height,
        },
        pose,
    ));
}

fn shadow_color(alpha: f32) -> Color {
    Color::BLACK.with_alpha(SHADOW_ALPHA * alpha)
}

pub struct StackPlugin;

impl Plugin for StackPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, apply_pose.before(TransformSystems::Propagate));
    }
}

type Stacks<'w, 's> =
    Query<'w, 's, (&'static StackPose, &'static CarStack, &'static Transform), Without<StackPart>>;
type Parts<'w, 's> = Query<'w, 's, (&'static mut Transform, &'static mut Sprite), With<StackPart>>;

/// Picks the baked frame for each car's heading, applies its alpha and keeps
/// every shadow on one ground layer below all cars.
fn apply_pose(stacks: Stacks, mut parts: Parts) {
    for (pose, stack, root) in &stacks {
        let i = frame_index(pose.heading, stack.frames.len());
        if let Ok((_, mut sprite)) = parts.get_mut(stack.body) {
            set_frame(&mut sprite, &stack.frames[i], Color::WHITE.with_alpha(pose.alpha));
        }
        if let Ok((mut tf, mut sprite)) = parts.get_mut(stack.shadow) {
            set_frame(&mut sprite, &stack.shadows[i], shadow_color(pose.alpha));
            let z = SHADOW_Z - root.translation.z;
            if tf.translation.z != z {
                tf.translation.z = z;
            }
        }
    }
}

fn set_frame(sprite: &mut Mut<Sprite>, image: &Handle<Image>, color: Color) {
    if sprite.image != *image {
        sprite.image = image.clone();
    }
    if sprite.color != color {
        sprite.color = color;
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
    fn slices_have_bright_lights_and_no_per_slice_outline() {
        let body = [200, 100, 50];
        let slices = voxel_car(BodyStyle::Coupe, body, [240, 240, 240]);
        // A low body slice: its leftmost pixel matches its neighbour (no darkened rim).
        let s = &slices[1];
        let row = s.height / 2;
        let x = (0..s.width).find(|&x| s.get(x, row)[3] > 0).unwrap();
        assert_eq!(s.get(x, row), s.get(x + 1, row));
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
        let meta = StripMeta { slices: Some(1), facing: Facing::Up, scale: 1.0 };
        let car = strip_to_car(&slice, meta).unwrap();
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
    fn strips_face_down_by_default() {
        assert_eq!(StripMeta::default().facing, Facing::Down);
        assert_eq!(parse_meta("").unwrap().facing, Facing::Down);
        // A 2x4 slice whose nose (marked) is the bottom row; its left column is
        // the car's left side (viewed from above, nose toward the viewer's bottom).
        let mut slice = Grid::new(2, 4);
        slice.set(0, 3, [1, 2, 3, 255]);
        slice.set(1, 3, [4, 5, 6, 255]);
        let car = strip_to_car(&slice, StripMeta { slices: Some(1), ..StripMeta::default() }).unwrap();
        assert_eq!((car.width, car.height), (4, 2));
        let s = &car.slices[0];
        // Nose in the rightmost column; the car's right side (image left) ends up at the bottom (-Y).
        assert_eq!(s.get(3, 1), [1, 2, 3, 255]);
        assert_eq!(s.get(3, 0), [4, 5, 6, 255]);
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

    /// Solid `w` x `h` slices, one color each.
    fn solid_slices(w: u32, h: u32, colors: &[[u8; 4]]) -> Vec<Grid> {
        colors.iter().map(|&c| Grid { width: w, height: h, px: vec![c; (w * h) as usize] }).collect()
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

    const RGB: [[u8; 4]; 3] = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]];

    #[test]
    fn straight_down_view_is_one_slice_wide_and_count_taller() {
        for (w, h) in [(6, 4), (7, 3), (30, 16)] {
            let slices = solid_slices(w, h, &RGB);
            let canvas = stack_canvas(w, h, slices.len(), 1.0, 1.0);
            let g = composite_stack(&slices, 0.0, 1.0, 1.0, canvas);
            let (x0, y0, x1, y1) = bbox(&g);
            assert_eq!(x1 - x0 + 1, w, "{w}x{h} width");
            assert_eq!(y1 - y0 + 1, h + slices.len() as u32 - 1, "{w}x{h} height");
            // The top slice covers the highest rows, the ground slice peeks out at the bottom.
            assert_eq!(g.get(x0, y0), RGB[2]);
            assert_eq!(g.get(x0, y1), RGB[0]);
            assert!((0..g.width).all(|x| g.get(x, y1)[3] == 0 || g.get(x, y1) == RGB[0]));
        }
    }

    #[test]
    fn zero_spacing_shows_only_the_top_slice() {
        let slices = solid_slices(6, 4, &RGB);
        let canvas = stack_canvas(6, 4, 3, 1.0, 0.0);
        let g = composite_stack(&slices, 0.0, 1.0, 0.0, canvas);
        assert_eq!(opaque(&g), 24);
        assert!(g.px.iter().all(|p| p[3] == 0 || *p == RGB[2]));
    }

    #[test]
    fn symmetric_even_slices_do_not_wobble_when_turned() {
        // A 180-degree symmetric even-sized slice: a frame with a centred dot pair.
        let mut s = Grid::new(6, 4);
        for (x, y) in [(0, 0), (5, 3), (1, 1), (4, 2), (2, 1), (3, 2)] {
            s.set(x, y, [200, 10, 10, 255]);
        }
        assert_eq!(s.rotate_180(), s);
        let slices = vec![s.clone(), s];
        let canvas = stack_canvas(6, 4, 2, 1.0, 1.0);
        let a = composite_stack(&slices, 0.0, 1.0, 1.0, canvas);
        let b = composite_stack(&slices, std::f32::consts::PI, 1.0, 1.0, canvas);
        assert_eq!(a, b);
        // A solid even square looks the same at every quarter turn.
        let sq = solid_slices(4, 4, &RGB);
        let canvas = stack_canvas(4, 4, 3, 1.0, 1.0);
        let base = composite_stack(&sq, 0.0, 1.0, 1.0, canvas);
        for k in 1..4 {
            let g = composite_stack(&sq, k as f32 * TAU / 4.0, 1.0, 1.0, canvas);
            assert_eq!(g, base, "quarter turn {k}");
        }
    }

    #[test]
    fn outline_only_rings_the_composite() {
        let slices = solid_slices(10, 6, &RGB);
        for k in 0..16 {
            let angle = k as f32 * TAU / 16.0;
            let canvas = stack_canvas(10, 6, 3, SQUASH, SPACING);
            let plain = composite_stack(&slices, angle, SQUASH, SPACING, canvas);
            let g = render_stack(&slices, angle, SQUASH, SPACING);
            assert_eq!((g.width, g.height), (canvas.x, canvas.y));
            for (p, q) in g.px.iter().zip(&plain.px) {
                if q[3] > 0 {
                    // Inside the body nothing changed: no dark speckle or seams.
                    assert_eq!(p, q, "heading {k}");
                    assert_ne!(*p, OUTLINE);
                } else if p[3] > 0 {
                    assert_eq!(*p, OUTLINE);
                }
            }
            // The body is ringed by the outline.
            assert!(g.px.contains(&OUTLINE));
        }
    }

    #[test]
    fn canvas_fits_every_heading_and_stays_fixed() {
        let car = procedural_car(CarId::Baron, PaintId::Factory);
        let first = render_stack(&car.slices, 0.0, SQUASH, SPACING);
        assert!(first.width.is_multiple_of(2) && first.height.is_multiple_of(2));
        for k in 0..32 {
            let g = render_stack(&car.slices, frame_angle(k, 32), SQUASH, SPACING);
            assert_eq!((g.width, g.height), (first.width, first.height));
            // Nothing touches the border: the outline always fits.
            for x in 0..g.width {
                assert_eq!(g.get(x, 0)[3], 0);
                assert_eq!(g.get(x, g.height - 1)[3], 0);
            }
            for y in 0..g.height {
                assert_eq!(g.get(0, y)[3], 0);
                assert_eq!(g.get(g.width - 1, y)[3], 0);
            }
        }
    }

    #[test]
    fn shadow_lines_up_with_the_ground_slice() {
        let slices = solid_slices(8, 4, &RGB);
        let car = CarSlices::new(slices, 1.0);
        for k in 0..8 {
            let angle = frame_angle(k, 8);
            let shadow = render_shadow(&car.footprint(), angle, SQUASH);
            let canvas = stack_canvas(8, 4, 3, SQUASH, SPACING);
            let ground = composite_stack(&car.slices[..1], angle, SQUASH, SPACING, canvas);
            // Both canvases are centred on the ground point; compare centred boxes.
            let (s, g) = (bbox(&shadow), bbox(&ground));
            let (dx, dy) = ((canvas.x - shadow.width) / 2, (canvas.y - shadow.height) / 2);
            assert_eq!((s.0 + dx, s.1 + dy, s.2 + dx, s.3 + dy), g, "heading {k}");
        }
    }

    #[test]
    fn headings_point_the_nose_the_right_way() {
        let car = procedural_car(CarId::Pebble, PaintId::Factory);
        let (head, tail) = ([255, 244, 200, 255], [228, 50, 50, 255]);
        // Mean light position relative to the canvas centre (+x right, +y down).
        let lights = |angle: f32, color: [u8; 4]| -> Vec2 {
            let g = render_stack(&car.slices, angle, SQUASH, SPACING);
            let pts: Vec<Vec2> = (0..g.height)
                .flat_map(|y| (0..g.width).map(move |x| (x, y)))
                .filter(|&(x, y)| g.get(x, y) == color)
                .map(|(x, y)| Vec2::new(x as f32 - g.width as f32 / 2.0, y as f32 - g.height as f32 / 2.0))
                .collect();
            assert!(!pts.is_empty(), "no headlights at {angle}");
            pts.iter().sum::<Vec2>() / pts.len() as f32
        };
        assert!(lights(0.0, head).x > 8.0);
        assert!(lights(0.0, tail).x < -8.0);
        assert!(lights(std::f32::consts::PI, head).x < -8.0);
        // Nose up (world +Y): the tail lights face the viewer, low in the image.
        assert!(lights(std::f32::consts::FRAC_PI_2, tail).y > 4.0);
        assert!(lights(-std::f32::consts::FRAC_PI_2, head).y > 4.0);
    }

    #[test]
    fn frame_index_rounds_and_wraps() {
        assert_eq!(frame_index(0.0, 64), 0);
        assert_eq!(frame_index(TAU, 64), 0);
        assert_eq!(frame_index(-TAU / 64.0, 64), 63);
        assert_eq!(frame_index(TAU / 4.0, 64), 16);
        assert_eq!(frame_index(-TAU / 4.0, 128), 96);
        assert_eq!(frame_index(TAU * 0.49 / 64.0, 64), 0);
        assert_eq!(frame_index(TAU * 0.51 / 64.0, 64), 1);
        assert_eq!(frame_index(-3.0 * TAU - TAU / 8.0, 8), 7);
        assert_eq!(frame_index(5.0 * TAU + TAU / 8.0, 8), 1);
        for i in 0..HEADINGS {
            assert_eq!(frame_index(frame_angle(i, HEADINGS), HEADINGS), i);
        }
    }

    #[test]
    fn lift_is_whole_pixels() {
        assert_eq!(lift(0, 1.0), 0);
        assert_eq!(lift(3, 1.0), 3);
        assert_eq!(lift(3, 0.5), 2);
        assert_eq!(lift(5, 0.0), 0);
    }

    /// Writes a contact sheet of every car at 8 headings and two squash values
    /// (rows: car x squash; cell backgrounds cycle the track road colors) as a
    /// PAM image. Run with
    /// `STACK_SHEET=/path/sheet.pam cargo test contact_sheet -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn contact_sheet() {
        use crate::core::tracks::TrackId;
        let Some(path) = std::env::var_os("STACK_SHEET") else { return };
        let squashes = [0.75, 0.88];
        let cars: Vec<CarSlices> = CarId::ALL.iter().map(|&c| procedural_car(c, PaintId::Factory)).collect();
        let cell = cars.iter().fold(UVec2::ZERO, |m, c| {
            m.max(stack_canvas(c.width, c.height, c.slices.len(), 0.88, SPACING) + UVec2::new(0, 8))
        });
        let (cols, rows) = (8u32, (cars.len() * squashes.len()) as u32);
        let mut sheet = Grid::new(cell.x * cols, cell.y * rows);
        for (ci, car) in cars.iter().enumerate() {
            let road = crate::game::theme::world_theme(TrackId::ALL[ci % 4]).road.to_srgba();
            let bg = [road.red, road.green, road.blue].map(|v| (v * 255.0).round() as u8);
            for (si, &sq) in squashes.iter().enumerate() {
                let row = (ci * squashes.len() + si) as u32;
                for col in 0..cols {
                    let mut shadow = render_shadow(&car.footprint(), frame_angle(col as usize, 8), sq);
                    shadow.px.iter_mut().for_each(|p| *p = if p[3] > 0 { [0, 0, 0, 77] } else { *p });
                    let g = render_stack(&car.slices, frame_angle(col as usize, 8), sq, SPACING);
                    let (ox, oy) = (col * cell.x, row * cell.y);
                    for y in 0..cell.y {
                        for x in 0..cell.x {
                            sheet.set(ox + x, oy + y, [bg[0], bg[1], bg[2], 255]);
                        }
                    }
                    // Centre both canvases on the cell centre; shadow offset 2 right, 3 down.
                    let place = |sheet: &mut Grid, img: &Grid, dx: u32, dy: u32| {
                        let (x0, y0) = (ox + (cell.x - img.width) / 2 + dx, oy + (cell.y - img.height) / 2 + dy);
                        for y in 0..img.height {
                            for x in 0..img.width {
                                let p = img.get(x, y);
                                if p[3] > 0 {
                                    let d = sheet.get(x0 + x, y0 + y);
                                    sheet.set(x0 + x, y0 + y, blend(d, p));
                                }
                            }
                        }
                    };
                    place(&mut sheet, &shadow, 2, 3);
                    place(&mut sheet, &g, 0, 0);
                }
            }
        }
        let mut bytes = format!(
            "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n",
            sheet.width, sheet.height
        )
        .into_bytes();
        bytes.extend(sheet.px.iter().flatten());
        std::fs::write(path, bytes).unwrap();
        // Bake cost of one car at the shipped settings.
        let t = std::time::Instant::now();
        for i in 0..HEADINGS {
            let a = frame_angle(i, HEADINGS);
            std::hint::black_box(render_stack(&cars[6].slices, a, SQUASH, SPACING));
            std::hint::black_box(render_shadow(&cars[6].footprint(), a, SQUASH));
        }
        let c = stack_canvas(cars[6].width, cars[6].height, cars[6].slices.len(), SQUASH, SPACING);
        println!("baked {HEADINGS} headings in {:?}; frame canvas {}x{}", t.elapsed(), c.x, c.y);
    }
}
