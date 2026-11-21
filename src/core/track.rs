//! Track geometry and surface queries.
//!
//! A track is a closed main loop plus optional branch routes (a shortcut and a
//! boost lane). Each route is a centripetal Catmull-Rom curve resampled to even
//! spacing. Surfaces are bands measured from the nearest centerline:
//! road → kerb (only on curved sections) → gravel (+ runoff outside corners) →
//! grass → wall. Pure math; no Bevy systems.

use std::collections::HashMap;

use bevy::math::Vec2;

/// Spacing between resampled centerline points, in world px.
pub const SAMPLE_SPACING: f32 = 8.0;
const KERB_INSIDE: f32 = 4.0;
const KERB_OUTSIDE: f32 = 8.0;
const KERB_CURVATURE: f32 = 0.0022;
const RUNOFF_MAX: f32 = 40.0;
const GRID_CELL: f32 = 96.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Surface {
    Road,
    Kerb,
    Gravel,
    Grass,
    Wall,
}

#[derive(Debug, Clone, Copy)]
pub struct ControlPoint {
    pub pos: Vec2,
    pub half_width: f32,
}

pub const fn cp(x: f32, y: f32, half_width: f32) -> ControlPoint {
    ControlPoint {
        pos: Vec2::new(x, y),
        half_width,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchKind {
    Shortcut,
    BoostLane,
}

/// A branch leaves the main loop at control-point index `from` (fractional, e.g.
/// 6.5 = halfway between points 6 and 7) and rejoins at `to`.
#[derive(Debug, Clone)]
pub struct BranchDef {
    pub kind: BranchKind,
    pub from: f32,
    pub to: f32,
    pub mids: Vec<ControlPoint>,
}

#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub pos: Vec2,
    pub tangent: Vec2,
    /// Unit normal pointing left of travel.
    pub normal: Vec2,
    pub half_width: f32,
    /// Signed curvature (positive = turning left).
    pub curvature: f32,
    pub kerb: bool,
    pub runoff_left: f32,
    pub runoff_right: f32,
    /// Arc length from the path start.
    pub distance: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct PathStyle {
    pub gravel: f32,
    pub grass: f32,
    pub kerbs: bool,
}

#[derive(Debug, Clone)]
pub struct Path {
    pub samples: Vec<Sample>,
    pub length: f32,
    pub closed: bool,
    pub style: PathStyle,
}

#[derive(Debug, Clone, Copy)]
pub struct BoostPad {
    pub center: Vec2,
    pub dir: Vec2,
    pub half_length: f32,
    pub half_width: f32,
}

impl BoostPad {
    pub fn contains(&self, p: Vec2) -> bool {
        let d = p - self.center;
        d.dot(self.dir).abs() <= self.half_length && d.dot(self.dir.perp()).abs() <= self.half_width
    }
}

#[derive(Debug, Clone)]
pub struct Branch {
    pub kind: BranchKind,
    pub path: Path,
    /// Main-loop distance where the branch starts and ends.
    pub start: f32,
    pub end: f32,
    pub pads: Vec<BoostPad>,
}

/// Result of a surface query at a world point.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceQuery {
    pub surface: Surface,
    /// Main-loop lap distance in `[0, length)`, remapped when on a branch.
    pub progress: f32,
    /// Signed lateral offset from the chosen route's centerline.
    pub lateral: f32,
    /// Tangent of the chosen route at the closest point.
    pub tangent: Vec2,
    /// `None` for the main loop, otherwise the branch index.
    pub route: Option<usize>,
    /// When inside a wall: direction to push the car and penetration depth.
    pub wall_push: Option<(Vec2, f32)>,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub main: Path,
    pub branches: Vec<Branch>,
    grid: HashMap<(i32, i32), Vec<(u16, u32)>>,
}

impl Path {
    fn build(control: &[ControlPoint], closed: bool, style: PathStyle) -> Self {
        let dense = dense_curve(control, closed, 24);
        let samples = resample(&dense, closed, SAMPLE_SPACING);
        let mut path = Path {
            length: 0.0,
            samples,
            closed,
            style,
        };
        path.finish();
        path
    }

    fn finish(&mut self) {
        let n = self.samples.len();
        let get = |i: isize, s: &[Sample]| -> Vec2 {
            if self.closed {
                s[i.rem_euclid(n as isize) as usize].pos
            } else {
                s[i.clamp(0, n as isize - 1) as usize].pos
            }
        };
        // Tangents, normals, arc length.
        let mut acc = 0.0;
        for i in 0..n {
            let t = (get(i as isize + 1, &self.samples) - get(i as isize - 1, &self.samples))
                .normalize_or_zero();
            let s = &mut self.samples[i];
            s.tangent = t;
            s.normal = t.perp();
            s.distance = acc;
            if i + 1 < n || self.closed {
                acc += self.samples[i]
                    .pos
                    .distance(self.samples[(i + 1) % n].pos);
            }
        }
        self.length = acc;

        // Signed curvature, smoothed.
        let raw: Vec<f32> = (0..n)
            .map(|i| {
                let a = self.tangent_wrapped(i as isize - 1);
                let b = self.tangent_wrapped(i as isize + 1);
                a.perp_dot(b) / (2.0 * SAMPLE_SPACING)
            })
            .collect();
        let smooth = box_blur(&raw, 4, self.closed);

        // Kerb flags: curved sections, extended past the corner, short runs dropped.
        let curved: Vec<bool> = smooth
            .iter()
            .map(|k| self.style.kerbs && k.abs() > KERB_CURVATURE)
            .collect();
        let mut kerb = dilate(&curved, 6, self.closed);
        drop_short_runs(&mut kerb, 10, self.closed);

        // Runoff on the outside of corners.
        let out_left: Vec<f32> = smooth
            .iter()
            .map(|&k| if k < 0.0 { (k.abs() * 12_000.0).min(RUNOFF_MAX) } else { 0.0 })
            .collect();
        let out_right: Vec<f32> = smooth
            .iter()
            .map(|&k| if k > 0.0 { (k * 12_000.0).min(RUNOFF_MAX) } else { 0.0 })
            .collect();
        let out_left = box_blur(&out_left, 8, self.closed);
        let out_right = box_blur(&out_right, 8, self.closed);

        for i in 0..n {
            let s = &mut self.samples[i];
            s.curvature = smooth[i];
            s.kerb = kerb[i];
            s.runoff_left = out_left[i];
            s.runoff_right = out_right[i];
        }
    }

    fn tangent_wrapped(&self, i: isize) -> Vec2 {
        let n = self.samples.len() as isize;
        let idx = if self.closed { i.rem_euclid(n) } else { i.clamp(0, n - 1) };
        let j = idx as usize;
        let next = if self.closed { (j + 1) % n as usize } else { (j + 1).min(n as usize - 1) };
        let prev = if self.closed {
            (j + n as usize - 1) % n as usize
        } else {
            j.saturating_sub(1)
        };
        (self.samples[next].pos - self.samples[prev].pos).normalize_or_zero()
    }

    /// Point and tangent at `distance` along the path (wraps when closed).
    pub fn sample_at(&self, distance: f32) -> (Vec2, Vec2) {
        let n = self.samples.len();
        let d = if self.closed {
            distance.rem_euclid(self.length)
        } else {
            distance.clamp(0.0, self.length)
        };
        let i = self
            .samples
            .partition_point(|s| s.distance <= d)
            .saturating_sub(1)
            .min(n - 1);
        let a = &self.samples[i];
        let j = if self.closed { (i + 1) % n } else { (i + 1).min(n - 1) };
        let b = &self.samples[j];
        let seg = a.pos.distance(b.pos).max(f32::EPSILON);
        let t = ((d - a.distance) / seg).clamp(0.0, 1.0);
        (a.pos.lerp(b.pos, t), a.tangent.lerp(b.tangent, t).normalize_or_zero())
    }

    /// Distance from the centerline to the wall on the given side at sample `i`.
    pub fn wall_offset(&self, i: usize, left: bool) -> f32 {
        let s = &self.samples[i];
        let runoff = if left { s.runoff_left } else { s.runoff_right };
        s.half_width + self.style.gravel + runoff + self.style.grass
    }

    pub fn gravel_offset(&self, i: usize, left: bool) -> f32 {
        let s = &self.samples[i];
        let runoff = if left { s.runoff_left } else { s.runoff_right };
        s.half_width + self.style.gravel + runoff
    }

    fn surface(&self, i: usize, lateral: f32) -> Surface {
        let s = &self.samples[i];
        let a = lateral.abs();
        let left = lateral > 0.0;
        if s.kerb && a > s.half_width - KERB_INSIDE && a <= s.half_width + KERB_OUTSIDE {
            Surface::Kerb
        } else if a <= s.half_width {
            Surface::Road
        } else if a <= self.gravel_offset(i, left) {
            Surface::Gravel
        } else if a <= self.wall_offset(i, left) {
            Surface::Grass
        } else {
            Surface::Wall
        }
    }

    /// Closest point on the path near sample `hint`: (segment start, t, point).
    fn refine(&self, p: Vec2, hint: usize) -> (usize, f32, Vec2) {
        let n = self.samples.len();
        let mut best = (f32::MAX, hint, 0.0, self.samples[hint].pos);
        for k in -2isize..=1 {
            let i = hint as isize + k;
            let (i, j) = if self.closed {
                let i = i.rem_euclid(n as isize) as usize;
                (i, (i + 1) % n)
            } else {
                if i < 0 || i as usize + 1 >= n {
                    continue;
                }
                (i as usize, i as usize + 1)
            };
            let a = self.samples[i].pos;
            let ab = self.samples[j].pos - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(f32::EPSILON)).clamp(0.0, 1.0);
            let q = a + ab * t;
            let d2 = q.distance_squared(p);
            if d2 < best.0 {
                best = (d2, i, t, q);
            }
        }
        (best.1, best.2, best.3)
    }
}

impl Track {
    pub fn build(main: &[ControlPoint], branches: &[BranchDef]) -> Self {
        let main_style = PathStyle {
            gravel: 30.0,
            grass: 16.0,
            kerbs: true,
        };
        let branch_style = PathStyle {
            gravel: 22.0,
            grass: 10.0,
            kerbs: false,
        };
        let main_path = Path::build(main, true, main_style);
        let cp_distances = control_point_distances(main, &main_path);
        let at = |f: f32| -> f32 {
            let n = main.len();
            let i = f.floor() as usize % n;
            let t = f.fract();
            let a = cp_distances[i];
            let mut b = cp_distances[(i + 1) % n];
            if b < a {
                b += main_path.length;
            }
            (a + (b - a) * t).rem_euclid(main_path.length)
        };

        let built: Vec<Branch> = branches
            .iter()
            .map(|def| {
                let start = at(def.from);
                let end = at(def.to);
                let (p0, _) = main_path.sample_at(start);
                let (p1, _) = main_path.sample_at(end);
                let w0 = main_path.samples[nearest_index(&main_path, p0)].half_width;
                let w1 = main_path.samples[nearest_index(&main_path, p1)].half_width;
                let mut pts = vec![cp(p0.x, p0.y, w0 * 0.8)];
                pts.extend(def.mids.iter().copied());
                pts.push(cp(p1.x, p1.y, w1 * 0.8));
                let path = Path::build(&pts, false, branch_style);
                let pads = if def.kind == BranchKind::BoostLane {
                    [0.35, 0.65]
                        .iter()
                        .map(|f| {
                            let (c, d) = path.sample_at(path.length * f);
                            BoostPad {
                                center: c,
                                dir: d,
                                half_length: 26.0,
                                half_width: 30.0,
                            }
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                Branch {
                    kind: def.kind,
                    path,
                    start,
                    end,
                    pads,
                }
            })
            .collect();

        let mut track = Track {
            main: main_path,
            branches: built,
            grid: HashMap::new(),
        };
        track.index();
        track
    }

    fn index(&mut self) {
        let mut grid: HashMap<(i32, i32), Vec<(u16, u32)>> = HashMap::new();
        for (pid, path) in self.paths().enumerate() {
            for (i, s) in path.samples.iter().enumerate() {
                grid.entry(cell(s.pos)).or_default().push((pid as u16, i as u32));
            }
        }
        self.grid = grid;
    }

    /// Main loop first, then branches in order.
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        std::iter::once(&self.main).chain(self.branches.iter().map(|b| &b.path))
    }

    pub fn length(&self) -> f32 {
        self.main.length
    }

    pub fn pads(&self) -> impl Iterator<Item = &BoostPad> {
        self.branches.iter().flat_map(|b| b.pads.iter())
    }

    /// Signed forward delta between two lap distances, in `(-L/2, L/2]`.
    pub fn delta(&self, from: f32, to: f32) -> f32 {
        let l = self.main.length;
        let mut d = (to - from).rem_euclid(l);
        if d > l * 0.5 {
            d -= l;
        }
        d
    }

    /// Classify a world point: surface, lap progress, and wall response.
    pub fn query(&self, p: Vec2) -> SurfaceQuery {
        let path_count = 1 + self.branches.len();
        let mut nearest: Vec<Option<(f32, usize)>> = vec![None; path_count];
        let (cx, cy) = cell(p);
        for dx in -2..=2 {
            for dy in -2..=2 {
                if let Some(list) = self.grid.get(&(cx + dx, cy + dy)) {
                    for &(pid, i) in list {
                        let path = self.path(pid as usize);
                        let d2 = path.samples[i as usize].pos.distance_squared(p);
                        let slot = &mut nearest[pid as usize];
                        if slot.is_none_or(|(b, _)| d2 < b) {
                            *slot = Some((d2, i as usize));
                        }
                    }
                }
            }
        }
        if nearest[0].is_none() {
            // Far outside the indexed area: fall back to a full scan of the main loop.
            nearest[0] = Some((0.0, nearest_index(&self.main, p)));
        }

        let mut best: Option<(Surface, usize, usize, f32, f32, Vec2)> = None;
        for (pid, slot) in nearest.iter().enumerate() {
            let Some((_, hint)) = *slot else { continue };
            let path = self.path(pid);
            let (i, t, q) = path.refine(p, hint);
            let s = &path.samples[i];
            let lateral = s.normal.dot(p - q);
            let surface = path.surface(i, lateral);
            let local = s.distance + SAMPLE_SPACING * t;
            let replace = match best {
                None => true,
                Some((bs, ..)) => surface < bs,
            };
            if replace {
                best = Some((surface, pid, i, lateral, local, s.tangent));
            }
        }
        let (surface, pid, i, lateral, local, tangent) = best.expect("main loop always queried");

        let progress = if pid == 0 {
            local.rem_euclid(self.main.length)
        } else {
            let b = &self.branches[pid - 1];
            let span = (b.end - b.start).rem_euclid(self.main.length);
            let f = (local / b.path.length.max(1.0)).clamp(0.0, 1.0);
            (b.start + span * f).rem_euclid(self.main.length)
        };

        let wall_push = (surface == Surface::Wall).then(|| {
            let path = self.path(pid);
            let left = lateral > 0.0;
            let limit = path.wall_offset(i, left);
            let n = path.samples[i].normal * lateral.signum();
            (-n, lateral.abs() - limit)
        });

        SurfaceQuery {
            surface,
            progress,
            lateral,
            tangent,
            route: (pid > 0).then(|| pid - 1),
            wall_push,
        }
    }

    fn path(&self, pid: usize) -> &Path {
        if pid == 0 { &self.main } else { &self.branches[pid - 1].path }
    }

    /// Axis-aligned bounds of everything drivable, padded by `margin`.
    pub fn bounds(&self, margin: f32) -> (Vec2, Vec2) {
        let mut lo = Vec2::splat(f32::MAX);
        let mut hi = Vec2::splat(f32::MIN);
        for path in self.paths() {
            for s in &path.samples {
                lo = lo.min(s.pos);
                hi = hi.max(s.pos);
            }
        }
        (lo - Vec2::splat(margin), hi + Vec2::splat(margin))
    }
}

fn cell(p: Vec2) -> (i32, i32) {
    ((p.x / GRID_CELL).floor() as i32, (p.y / GRID_CELL).floor() as i32)
}

fn nearest_index(path: &Path, p: Vec2) -> usize {
    path.samples
        .iter()
        .enumerate()
        .min_by(|a, b| {
            a.1.pos
                .distance_squared(p)
                .total_cmp(&b.1.pos.distance_squared(p))
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn control_point_distances(control: &[ControlPoint], path: &Path) -> Vec<f32> {
    control
        .iter()
        .map(|c| path.samples[nearest_index(path, c.pos)].distance)
        .collect()
}

/// Centripetal Catmull-Rom (alpha = 0.5), densely sampled with interpolated widths.
fn dense_curve(control: &[ControlPoint], closed: bool, per_segment: usize) -> Vec<ControlPoint> {
    let n = control.len();
    assert!(n >= if closed { 4 } else { 2 }, "not enough control points");
    let get = |i: isize| -> ControlPoint {
        if closed {
            control[i.rem_euclid(n as isize) as usize]
        } else if i < 0 {
            // Mirror the first point to create a phantom.
            let a = control[0];
            let b = control[1];
            cp(2.0 * a.pos.x - b.pos.x, 2.0 * a.pos.y - b.pos.y, a.half_width)
        } else if i as usize >= n {
            let a = control[n - 1];
            let b = control[n - 2];
            cp(2.0 * a.pos.x - b.pos.x, 2.0 * a.pos.y - b.pos.y, a.half_width)
        } else {
            control[i as usize]
        }
    };
    let segments = if closed { n } else { n - 1 };
    let mut out = Vec::with_capacity(segments * per_segment + 1);
    for seg in 0..segments {
        let i = seg as isize;
        let (p0, p1, p2, p3) = (get(i - 1), get(i), get(i + 1), get(i + 2));
        for s in 0..per_segment {
            let t = s as f32 / per_segment as f32;
            let pos = centripetal(p0.pos, p1.pos, p2.pos, p3.pos, t);
            let w = p1.half_width + (p2.half_width - p1.half_width) * smoothstep(t);
            out.push(ControlPoint { pos, half_width: w });
        }
    }
    if !closed {
        out.push(control[n - 1]);
    }
    out
}

fn centripetal(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f32) -> Vec2 {
    let knot = |a: Vec2, b: Vec2| a.distance(b).sqrt().max(1e-4);
    let t0 = 0.0;
    let t1 = t0 + knot(p0, p1);
    let t2 = t1 + knot(p1, p2);
    let t3 = t2 + knot(p2, p3);
    let tt = t1 + (t2 - t1) * t;
    let a1 = p0 * ((t1 - tt) / (t1 - t0)) + p1 * ((tt - t0) / (t1 - t0));
    let a2 = p1 * ((t2 - tt) / (t2 - t1)) + p2 * ((tt - t1) / (t2 - t1));
    let a3 = p2 * ((t3 - tt) / (t3 - t2)) + p3 * ((tt - t2) / (t3 - t2));
    let b1 = a1 * ((t2 - tt) / (t2 - t0)) + a2 * ((tt - t0) / (t2 - t0));
    let b2 = a2 * ((t3 - tt) / (t3 - t1)) + a3 * ((tt - t1) / (t3 - t1));
    b1 * ((t2 - tt) / (t2 - t1)) + b2 * ((tt - t1) / (t2 - t1))
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn resample(dense: &[ControlPoint], closed: bool, spacing: f32) -> Vec<Sample> {
    let mut pts: Vec<ControlPoint> = dense.to_vec();
    if closed {
        pts.push(dense[0]);
    }
    let total: f32 = pts.windows(2).map(|w| w[0].pos.distance(w[1].pos)).sum();
    let count = ((total / spacing).round() as usize).max(2);
    let step = total / if closed { count as f32 } else { (count - 1) as f32 };
    let mut out = Vec::with_capacity(count);
    let mut seg = 0usize;
    let mut seg_start = 0.0f32;
    for k in 0..count {
        let target = k as f32 * step;
        while seg + 1 < pts.len() - 1
            && seg_start + pts[seg].pos.distance(pts[seg + 1].pos) < target
        {
            seg_start += pts[seg].pos.distance(pts[seg + 1].pos);
            seg += 1;
        }
        let a = pts[seg];
        let b = pts[seg + 1];
        let len = a.pos.distance(b.pos).max(f32::EPSILON);
        let t = ((target - seg_start) / len).clamp(0.0, 1.0);
        out.push(Sample {
            pos: a.pos.lerp(b.pos, t),
            tangent: Vec2::X,
            normal: Vec2::Y,
            half_width: a.half_width + (b.half_width - a.half_width) * t,
            curvature: 0.0,
            kerb: false,
            runoff_left: 0.0,
            runoff_right: 0.0,
            distance: 0.0,
        });
    }
    out
}

fn box_blur(v: &[f32], radius: usize, closed: bool) -> Vec<f32> {
    let n = v.len() as isize;
    (0..n)
        .map(|i| {
            let mut sum = 0.0;
            let mut cnt = 0.0;
            for k in -(radius as isize)..=radius as isize {
                let j = i + k;
                let j = if closed {
                    j.rem_euclid(n)
                } else if j < 0 || j >= n {
                    continue;
                } else {
                    j
                };
                sum += v[j as usize];
                cnt += 1.0;
            }
            sum / cnt
        })
        .collect()
}

fn dilate(v: &[bool], radius: usize, closed: bool) -> Vec<bool> {
    let n = v.len() as isize;
    (0..n)
        .map(|i| {
            (-(radius as isize)..=radius as isize).any(|k| {
                let j = i + k;
                if closed {
                    v[j.rem_euclid(n) as usize]
                } else {
                    j >= 0 && j < n && v[j as usize]
                }
            })
        })
        .collect()
}

fn drop_short_runs(v: &mut [bool], min_len: usize, closed: bool) {
    let n = v.len();
    if v.iter().all(|&b| b) {
        return;
    }
    // Start scanning just after a `false` so wrapped runs are handled once.
    let start = if closed { v.iter().position(|&b| !b).unwrap_or(0) } else { 0 };
    let mut i = 0;
    while i < n {
        let idx = (start + i) % n;
        if v[idx] {
            let mut len = 0;
            while i + len < n && v[(start + i + len) % n] {
                len += 1;
            }
            if len < min_len {
                for k in 0..len {
                    v[(start + i + k) % n] = false;
                }
            }
            i += len;
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oval() -> Track {
        let main = [
            cp(-600.0, -300.0, 50.0),
            cp(600.0, -300.0, 50.0),
            cp(900.0, 0.0, 50.0),
            cp(600.0, 300.0, 50.0),
            cp(-600.0, 300.0, 50.0),
            cp(-900.0, 0.0, 50.0),
        ];
        let branches = [BranchDef {
            kind: BranchKind::BoostLane,
            from: 1.0,
            to: 3.0,
            mids: vec![cp(1200.0, -150.0, 36.0), cp(1200.0, 150.0, 36.0)],
        }];
        Track::build(&main, &branches)
    }

    #[test]
    fn main_loop_is_closed_and_evenly_sampled() {
        let t = oval();
        assert!(t.length() > 3000.0, "len={}", t.length());
        let s = &t.main.samples;
        for w in s.windows(2) {
            let d = w[0].pos.distance(w[1].pos);
            assert!((d - SAMPLE_SPACING).abs() < 1.0, "spacing {d}");
        }
    }

    #[test]
    fn centerline_is_road_and_progress_matches() {
        let t = oval();
        let (p, _) = t.main.sample_at(500.0);
        let q = t.query(p);
        assert_eq!(q.surface, Surface::Road);
        assert!(q.route.is_none());
        assert!((q.progress - 500.0).abs() < 2.0, "progress {}", q.progress);
    }

    #[test]
    fn straights_have_no_kerbs_corners_do() {
        let t = oval();
        let straight = &t.main.samples[t.main.samples.len() / 2 - 120];
        assert!(!straight.kerb || straight.curvature.abs() > KERB_CURVATURE * 0.5);
        assert!(t.main.samples.iter().any(|s| s.kerb));
    }

    #[test]
    fn far_point_is_wall_with_inward_push() {
        let t = oval();
        let q = t.query(Vec2::new(0.0, -700.0));
        assert_eq!(q.surface, Surface::Wall);
        let (dir, depth) = q.wall_push.unwrap();
        assert!(dir.y > 0.9, "push should point back toward the track: {dir:?}");
        assert!(depth > 0.0);
    }

    #[test]
    fn branch_progress_is_remapped_between_endpoints() {
        let t = oval();
        let b = &t.branches[0];
        let (p, _) = b.path.sample_at(b.path.length * 0.5);
        let q = t.query(p);
        assert_eq!(q.route, Some(0));
        assert_eq!(q.surface, Surface::Road);
        let forward = t.delta(b.start, q.progress);
        let span = t.delta(b.start, b.end);
        assert!(forward > 0.0 && forward < span, "{forward} not inside (0,{span})");
        assert_eq!(b.pads.len(), 2);
        assert!(b.pads[0].contains(b.pads[0].center));
    }

    #[test]
    fn delta_wraps_around_finish_line() {
        let t = oval();
        let l = t.length();
        assert!((t.delta(l - 5.0, 5.0) - 10.0).abs() < 1e-2);
        assert!((t.delta(5.0, l - 5.0) + 10.0).abs() < 1e-2);
    }

    #[test]
    fn drop_short_runs_handles_wraparound() {
        let mut v = vec![true, true, false, false, false, false, true];
        drop_short_runs(&mut v, 4, true);
        assert!(v.iter().all(|&b| !b));
        let mut w = vec![true, true, false, false, true, true];
        drop_short_runs(&mut w, 4, true);
        assert_eq!(w, vec![true, true, false, false, true, true]);
    }
}
