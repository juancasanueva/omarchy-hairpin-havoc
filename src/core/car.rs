//! Arcade car physics (pure, fixed-step). See `step`.

/// Per-car tuning multipliers. Base cars and upgrades both produce one of these.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarTuning {
    pub top_speed_mul: f32,
    pub accel_mul: f32,
    pub grip_mul: f32,
    pub drift_control_mul: f32,
    /// Seconds a full nitro tank lasts.
    pub nitro_duration: f32,
    /// Extra top speed while nitro burns, px/s.
    pub nitro_top_bonus: f32,
    /// Extra engine force fraction while nitro burns.
    pub nitro_accel_mul: f32,
    pub nitro_refill_mul: f32,
}

impl Default for CarTuning {
    fn default() -> Self {
        Self {
            top_speed_mul: 1.0,
            accel_mul: 1.0,
            grip_mul: 1.0,
            drift_control_mul: 1.0,
            nitro_duration: 2.8,
            nitro_top_bonus: 80.0,
            nitro_accel_mul: 0.65,
            nitro_refill_mul: 1.0,
        }
    }
}

use std::f32::consts::{PI, TAU};

use bevy::math::Vec2;

use super::track::{Surface, Track};

/// Fixed physics timestep.
pub const DT: f32 = 1.0 / 120.0;
pub const CAR_RADIUS: f32 = 17.0;
/// Conversion for the speedometer.
pub const KMH_PER_PX: f32 = 0.4;

const BASE_TOP_SPEED: f32 = 465.0;
const BOOST_CEILING: f32 = 525.0;
const GEAR_TOPS: [f32; 5] = [110.0, 200.0, 290.0, 385.0, 540.0];
const GEAR_FORCE: [f32; 5] = [1.0, 0.84, 0.68, 0.54, 0.44];
const ENGINE_FORCE: f32 = 280.0;
const BRAKE_FORCE: f32 = 400.0;
const REVERSE_FORCE: f32 = 220.0;
const REVERSE_MAX: f32 = 145.0;
const COAST_DRAG: f32 = 28.0;
const HANDBRAKE_DRAG: f32 = 170.0;
const BOOST_FORCE: f32 = 190.0;
const WHEELBASE: f32 = 42.0;
const MAX_STEER: f32 = 0.55;
/// Fraction of `MAX_STEER` used by the bicycle model; lower = gentler turning.
const STEER_GAIN: f32 = 0.45;
/// Steering wheel speeds (per second): building lock, returning to centre, countersteering.
/// Keyboard input is all-or-nothing, so a slow build lets taps give partial lock.
const STEER_RATE_IN: f32 = 5.0;
const STEER_RATE_CENTER: f32 = 9.0;
const STEER_RATE_COUNTER: f32 = 14.0;
/// Remaining steering at top speed (1.0 = no reduction).
const STEER_AT_TOP_SPEED: f32 = 0.55;
const DRIFT_TIERS: [f32; 3] = [0.32, 0.66, 0.95];
const DRIFT_BOOST: [f32; 3] = [0.45, 0.75, 1.05];
const NITRO_MIN: f32 = 0.08;
/// Turning speed of a spinout, in radians per second.
const SPIN_RATE: f32 = 8.4;
/// Seconds over which a recovering spin eases to a stop.
const SPIN_EASE: f32 = 0.3;
/// Fastest turn of a recovering spin, so a sudden change of track direction never snaps the car.
const SPIN_RATE_MAX: f32 = 15.0;

/// Player or AI intent for one step. `steer` is +1 for left, -1 for right.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CarInput {
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub handbrake: bool,
    pub nitro: bool,
}

/// Effects on how one car drives, mostly temporary and granted by gadgets.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CarModifiers {
    /// Every surface drives like road, with extra grip.
    pub all_terrain: bool,
    /// Drift charge, points and nitro refill doubled; any release is a top-tier boost.
    pub drift_surge: bool,
    /// Extra acceleration while ramming.
    pub ramming: bool,
    /// Small acceleration bonus while phased.
    pub phantom: bool,
    /// Spinouts end facing along the track. Permanent; only the player's car has it.
    pub spin_recovery: bool,
}

/// What the ground looks like at a point. Implemented by `Track`, and by flat
/// ground in tests.
pub trait Ground {
    fn probe(&self, p: Vec2) -> GroundProbe;
}

#[derive(Debug, Clone, Copy)]
pub struct GroundProbe {
    pub surface: Surface,
    pub wall_push: Option<(Vec2, f32)>,
    /// Direction of a boost pad under this point, if any.
    pub pad: Option<Vec2>,
    /// Unit direction of travel of the route at this point.
    pub track_dir: Vec2,
}

impl Ground for Track {
    fn probe(&self, p: Vec2) -> GroundProbe {
        let q = self.query(p);
        GroundProbe {
            surface: q.surface,
            wall_push: q.wall_push,
            pad: self.pads().find(|pad| pad.contains(p)).map(|pad| pad.dir),
            track_dir: q.tangent,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SurfaceParams {
    roll: f32,
    grip: f32,
    power: f32,
    brake: f32,
}

fn surface_params(s: Surface) -> SurfaceParams {
    match s {
        Surface::Road => SurfaceParams { roll: 0.06, grip: 1.0, power: 1.0, brake: 1.0 },
        Surface::Kerb => SurfaceParams { roll: 0.12, grip: 0.9, power: 0.97, brake: 0.92 },
        Surface::Gravel => SurfaceParams { roll: 0.55, grip: 0.6, power: 0.8, brake: 0.65 },
        Surface::Grass | Surface::Wall => {
            SurfaceParams { roll: 1.0, grip: 0.5, power: 0.7, brake: 0.55 }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarState {
    pub pos: Vec2,
    pub vel: Vec2,
    /// Radians; forward is `(cos, sin)`.
    pub heading: f32,
    pub yaw_rate: f32,
    /// Smoothed steering, -1..1.
    pub steer: f32,
    pub gear: u8,
    /// 0..1 within the current gear, for audio and the tachometer.
    pub rpm: f32,
    pub surface: Surface,
    pub drifting: bool,
    drift_time: f32,
    low_slip_time: f32,
    pub drift_charge: f32,
    /// Points accumulated by the current drift, banked on release.
    pub drift_pending: f32,
    pub nitro: f32,
    pub nitro_active: bool,
    /// The tank burnt out; nitro stays off until the button is released.
    nitro_spent: bool,
    pub boost_time: f32,
    pub spin_time: f32,
    spin_dir: f32,
    spin_speed_mul_applied: bool,
    pub slow_time: f32,
    handbrake_prev: bool,
    pad_cooldown: f32,
}

/// Things that happened during a step, for sound, particles and scoring.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StepEvents {
    pub gear_changed: bool,
    pub drift_started: bool,
    /// Tier 1..=3 of a released drift boost.
    pub drift_boost: Option<u8>,
    pub drift_points_banked: f32,
    pub nitro_fired: bool,
    /// Impact speed of a wall hit.
    pub wall_hit: Option<f32>,
    pub pad_boost: bool,
}

impl StepEvents {
    pub fn merge(&mut self, o: StepEvents) {
        self.gear_changed |= o.gear_changed;
        self.drift_started |= o.drift_started;
        self.drift_boost = o.drift_boost.or(self.drift_boost);
        self.drift_points_banked += o.drift_points_banked;
        self.nitro_fired |= o.nitro_fired;
        self.wall_hit = match (self.wall_hit, o.wall_hit) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        self.pad_boost |= o.pad_boost;
    }
}

impl CarState {
    pub fn new(pos: Vec2, heading: f32) -> Self {
        Self {
            pos,
            vel: Vec2::ZERO,
            heading,
            yaw_rate: 0.0,
            steer: 0.0,
            gear: 1,
            rpm: 0.0,
            surface: Surface::Road,
            drifting: false,
            drift_time: 0.0,
            low_slip_time: 0.0,
            drift_charge: 0.0,
            drift_pending: 0.0,
            nitro: 0.35,
            nitro_active: false,
            nitro_spent: false,
            boost_time: 0.0,
            spin_time: 0.0,
            spin_dir: 1.0,
            spin_speed_mul_applied: false,
            slow_time: 0.0,
            handbrake_prev: false,
            pad_cooldown: 0.0,
        }
    }

    pub fn forward(&self) -> Vec2 {
        Vec2::from_angle(self.heading)
    }

    pub fn speed(&self) -> f32 {
        self.vel.length()
    }

    pub fn forward_speed(&self) -> f32 {
        self.vel.dot(self.forward())
    }

    /// Slip angle in radians between heading and velocity.
    pub fn slip(&self) -> f32 {
        let f = self.forward();
        let vf = self.vel.dot(f);
        let vs = self.vel.dot(f.perp());
        if self.vel.length_squared() < 1.0 {
            0.0
        } else {
            vs.abs().atan2(vf.abs())
        }
    }

    /// Current drift charge tier (0 = none).
    pub fn drift_tier(&self) -> u8 {
        DRIFT_TIERS.iter().filter(|&&t| self.drift_charge >= t).count() as u8
    }

    pub fn is_boosting(&self) -> bool {
        self.boost_time > 0.0 || self.nitro_active
    }

    /// Start a spinout (oil, shockwave, ram, hard collision).
    pub fn spin_out(&mut self, duration: f32, speed_mul: f32, dir: f32) {
        if self.spin_time <= 0.0 {
            self.vel *= speed_mul;
        }
        self.spin_time = self.spin_time.max(duration);
        self.spin_dir = if dir >= 0.0 { 1.0 } else { -1.0 };
        self.spin_speed_mul_applied = true;
        self.drifting = false;
        self.drift_charge = 0.0;
        self.drift_pending = 0.0;
        self.boost_time = 0.0;
        self.nitro_active = false;
    }

    /// Engine and drag penalty (static burst, jammer); cancels boosts.
    pub fn slow_down(&mut self, duration: f32, speed_mul: f32) {
        self.vel *= speed_mul;
        self.slow_time = self.slow_time.max(duration);
        self.boost_time = 0.0;
        self.nitro_active = false;
    }

    pub fn add_boost(&mut self, duration: f32, kick: f32) {
        self.boost_time = self.boost_time.max(duration);
        self.vel += self.forward() * kick;
    }
}

fn shape_steer(x: f32) -> f32 {
    let a = x.abs().min(1.0);
    x.signum() * (0.45 * a + 0.55 * a * a)
}

fn approach(current: f32, target: f32, max_delta: f32) -> f32 {
    current + (target - current).clamp(-max_delta, max_delta)
}

fn engine_force(gear: u8, rpm: f32, tuning: &CarTuning) -> f32 {
    let g = (gear.clamp(1, 5) - 1) as usize;
    ENGINE_FORCE * GEAR_FORCE[g] * tuning.accel_mul * (0.82 + 0.18 * (PI * rpm.clamp(0.0, 1.0)).sin())
}

fn gear_top(gear: u8, tuning: &CarTuning) -> f32 {
    GEAR_TOPS[(gear.clamp(1, 5) - 1) as usize] * tuning.top_speed_mul
}

/// Engine speed fraction within `gear` at forward speed `vf`.
fn rpm_for(gear: u8, vf: f32, tuning: &CarTuning) -> f32 {
    let lo = if gear > 1 { gear_top(gear - 1, tuning) * 0.8 } else { 0.0 };
    let hi = gear_top(gear, tuning);
    ((vf.max(0.0) - lo) / (hi - lo)).clamp(0.0, 1.0)
}

/// Air drag coefficient chosen so full throttle in top gear settles at `top`.
fn drag_for(top: f32, tuning: &CarTuning, extra_force: f32) -> f32 {
    let rpm = rpm_for(5, top, tuning);
    let force = engine_force(5, rpm, tuning) + extra_force;
    let roll = surface_params(Surface::Road).roll * top;
    ((force - roll) / (top * top)).max(1e-6)
}

/// Yaw rate for this step of a spin in direction `dir` with `time_left` to go, so that
/// it eases to a stop facing `target`. It makes the number of turns that keeps the spin
/// closest to [`SPIN_RATE`].
fn recovery_yaw_rate(heading: f32, target: f32, dir: f32, time_left: f32) -> f32 {
    let steps = (time_left / DT).ceil();
    let ease = (SPIN_EASE / DT).round();
    // Steps it would take to cover the remaining angle without slowing down.
    let full_rate_steps = if steps > ease { steps - (ease - 1.0) * 0.5 } else { steps * (steps + 1.0) / (2.0 * ease) };
    let offset = (target - heading) * dir;
    let nominal = SPIN_RATE * full_rate_steps * DT;
    let angle = offset + TAU * ((nominal - offset) / TAU).round();
    let this_step = if steps > ease { 1.0 } else { steps / ease };
    (dir * angle * this_step / (full_rate_steps * DT)).clamp(-SPIN_RATE_MAX, SPIN_RATE_MAX)
}

/// Advance one car by `DT`.
pub fn step(
    car: &mut CarState,
    input: CarInput,
    tuning: &CarTuning,
    mods: CarModifiers,
    ground: &impl Ground,
) -> StepEvents {
    let mut ev = StepEvents::default();
    let probe = ground.probe(car.pos);
    car.surface = probe.surface;
    let mut sp = surface_params(probe.surface);
    if mods.all_terrain {
        sp = surface_params(Surface::Road);
        sp.grip *= 1.12;
    }
    let grip = sp.grip * tuning.grip_mul;

    let fwd = car.forward();
    let side = fwd.perp();
    let mut vf = car.vel.dot(fwd);
    let mut vs = car.vel.dot(side);
    let speed = car.vel.length();

    // Timers.
    car.boost_time = (car.boost_time - DT).max(0.0);
    car.slow_time = (car.slow_time - DT).max(0.0);
    car.pad_cooldown = (car.pad_cooldown - DT).max(0.0);
    let spin_left = car.spin_time;
    let spinning = spin_left > 0.0;
    car.spin_time = (car.spin_time - DT).max(0.0);

    // Steering with asymmetric rates.
    let target = if spinning { 0.0 } else { shape_steer(input.steer) };
    let rate = if target == 0.0 {
        STEER_RATE_CENTER
    } else if target.signum() != car.steer.signum() && car.steer != 0.0 {
        STEER_RATE_COUNTER
    } else {
        STEER_RATE_IN
    };
    car.steer = approach(car.steer, target, rate * DT);

    // Gearbox.
    let old_gear = car.gear;
    if car.gear < 5 && vf > 0.97 * gear_top(car.gear, tuning) {
        car.gear += 1;
    } else if car.gear > 1 && vf < 0.86 * gear_top(car.gear - 1, tuning) {
        car.gear -= 1;
    }
    ev.gear_changed = car.gear != old_gear;
    car.rpm = rpm_for(car.gear, vf, tuning);

    // Nitro.
    car.nitro_spent &= input.nitro;
    let can_fire = input.nitro && !spinning && !car.nitro_spent && (car.nitro_active || car.nitro >= NITRO_MIN);
    if can_fire && car.nitro > 0.0 {
        if !car.nitro_active {
            ev.nitro_fired = true;
            vf += 22.0;
        }
        car.nitro_active = true;
        car.nitro = (car.nitro - DT / tuning.nitro_duration).max(0.0);
        car.nitro_spent = car.nitro <= 0.0;
    } else {
        car.nitro_active = false;
    }

    // Longitudinal forces.
    let top = BASE_TOP_SPEED * tuning.top_speed_mul;
    let mut power = sp.power;
    if spinning {
        power *= 0.2;
    }
    if car.slow_time > 0.0 {
        power *= 0.46;
    }
    if input.handbrake {
        power *= 0.5;
    }
    let base_force = engine_force(car.gear, car.rpm, tuning);
    let mut drive = base_force * power * input.throttle.clamp(0.0, 1.0);
    if mods.ramming {
        drive += 120.0;
    }
    if mods.phantom {
        drive *= 1.05;
    }
    let mut drag = drag_for(top, tuning, 0.0);
    if car.nitro_active {
        let extra = base_force * tuning.nitro_accel_mul;
        drive += extra;
        drag = drag_for(top + tuning.nitro_top_bonus, tuning, extra);
    }
    if car.boost_time > 0.0 {
        drive += BOOST_FORCE;
        drag = drag.min(drag_for(BOOST_CEILING * tuning.top_speed_mul, tuning, BOOST_FORCE));
    }
    let mut accel = drive - drag * vf * vf.abs() - sp.roll * vf;
    if input.throttle <= 0.01 && vf.abs() > 1.0 {
        accel -= COAST_DRAG * vf.signum();
    }
    if input.brake > 0.01 {
        if vf > 8.0 {
            accel -= BRAKE_FORCE * sp.brake * input.brake;
        } else if vf > -REVERSE_MAX {
            accel -= REVERSE_FORCE * input.brake;
        }
    }
    if input.handbrake && vf.abs() > 1.0 {
        accel -= HANDBRAKE_DRAG * vf.signum();
    }
    if spinning {
        accel -= 1.45 * vf;
    }
    if car.slow_time > 0.0 {
        accel -= 0.9 * vf;
    }
    let new_vf = vf + accel * DT;
    // Brakes and coasting must not reverse the direction by themselves.
    vf = if vf > 0.0 && new_vf < 0.0 && input.brake <= 0.01 { 0.0 } else { new_vf };

    // Drift start: the handbrake, braking into a turn, or sliding off the road.
    // Quick left-right corrections never start one on their own: with a
    // keyboard they are how you drive straight.
    let steer_abs = input.steer.abs();
    let slip = car.slip();
    let fresh_handbrake = input.handbrake && !car.handbrake_prev;
    car.handbrake_prev = input.handbrake;
    if !car.drifting && !spinning && vf > 0.0 {
        let kick = if input.handbrake && speed > 85.0 && steer_abs > 0.15 {
            Some(if fresh_handbrake { 1.6 } else { 1.2 })
        } else if (input.brake > 0.2 && steer_abs > 0.45 && speed > 170.0)
            || (grip < 0.8 && steer_abs > 0.6 && speed > 140.0)
        {
            Some(0.9)
        } else {
            None
        };
        if let Some(k) = kick {
            car.drifting = true;
            car.drift_time = 0.0;
            car.low_slip_time = 0.0;
            car.yaw_rate += k * input.steer.signum() * tuning.drift_control_mul;
            ev.drift_started = true;
        }
    }

    // Drift maintenance, charge and release.
    if car.drifting {
        car.drift_time += DT;
        if slip < 0.14 && !input.handbrake {
            car.low_slip_time += DT;
        } else {
            car.low_slip_time = 0.0;
        }
        let surge = if mods.drift_surge { 2.0 } else { 1.0 };
        let on_road = matches!(probe.surface, Surface::Road | Surface::Kerb) || mods.all_terrain;
        if on_road && speed > 110.0 && slip > 0.22 {
            car.drift_charge += 0.36 * (slip / 0.6).clamp(0.5, 1.2) * DT * surge * tuning.drift_control_mul;
            car.drift_pending += speed * slip * DT * surge;
            car.nitro += 0.14 * (slip / 0.5).min(1.0) * tuning.nitro_refill_mul * surge * DT;
        } else if !on_road {
            car.drift_charge -= 0.25 * DT;
        }
        car.drift_charge = car.drift_charge.clamp(0.0, 1.0);

        let ended = (car.drift_time > 0.12 && car.low_slip_time > 0.06) || speed < 55.0 || vf < 0.0;
        if ended {
            car.drifting = false;
            let mut tier = car.drift_tier();
            if mods.drift_surge && (tier >= 1 || car.drift_charge >= 0.15) {
                tier = 3;
            }
            if tier > 0 {
                car.boost_time = car.boost_time.max(DRIFT_BOOST[tier as usize - 1]);
                vf += 24.0;
                if tier >= 2 {
                    car.nitro += 0.10;
                }
                ev.drift_boost = Some(tier);
            }
            ev.drift_points_banked = car.drift_pending;
            car.drift_pending = 0.0;
            car.drift_charge = 0.0;
        }
    }
    if speed > 40.0 {
        car.nitro += 0.014 * DT;
    }
    car.nitro = car.nitro.clamp(0.0, 1.0);

    // Lateral grip: bleed sideways velocity.
    let lat_rate = if input.handbrake {
        1.6
    } else if car.drifting {
        3.2 - 0.8 * input.throttle.clamp(0.0, 1.0)
    } else {
        9.5 * grip
    };
    vs *= (-lat_rate * DT).exp();

    // Yaw from a bicycle model.
    if spinning {
        car.yaw_rate = if mods.spin_recovery {
            recovery_yaw_rate(car.heading, probe.track_dir.to_angle(), car.spin_dir, spin_left)
        } else {
            car.spin_dir * SPIN_RATE
        };
    } else {
        let sf = if speed < 55.0 {
            (speed / 55.0).sqrt()
        } else if speed <= 260.0 {
            1.0
        } else {
            1.0 - (1.0 - STEER_AT_TOP_SPEED) * ((speed - 260.0) / (465.0 - 260.0)).min(1.0)
        };
        let angle = car.steer * MAX_STEER * STEER_GAIN;
        let mut target_yaw = vf * angle.tan() / WHEELBASE * sf;
        if car.drifting {
            target_yaw *= 1.0 + 0.45 * tuning.drift_control_mul;
        }
        let cap = 1050.0 * grip / speed.max(1.0);
        let cap = if car.drifting { cap * 1.6 } else { cap };
        target_yaw = target_yaw.clamp(-cap, cap);
        let follow = if car.drifting { 6.0 } else { 14.0 * grip };
        car.yaw_rate += (target_yaw - car.yaw_rate) * (follow * DT).min(1.0);
    }
    car.heading = (car.heading + car.yaw_rate * DT).rem_euclid(2.0 * PI);
    car.vel = fwd * vf + side * vs;

    car.pos += car.vel * DT;

    // Walls.
    if let Some((dir, depth)) = ground.probe(car.pos).wall_push {
        car.pos += dir * depth.max(0.0);
        let vn = car.vel.dot(dir);
        if vn < 0.0 {
            let impact = -vn;
            let tangent = car.vel - dir * vn;
            let keep = 1.0 - 0.16 * (impact / 150.0).min(1.0);
            car.vel = tangent * keep - dir * vn * 0.22;
            let kick = 1.5 * (impact / 200.0).min(1.0);
            car.yaw_rate += kick * dir.perp_dot(car.forward()).signum();
            if impact > 55.0 {
                car.drift_charge = 0.0;
            }
            if impact > 20.0 {
                ev.wall_hit = Some(impact);
            }
        }
    }

    // Boost pads.
    if let Some(dir) = probe.pad
        && car.pad_cooldown <= 0.0
        && car.forward().dot(dir) > 0.4
    {
        car.boost_time = car.boost_time.max(1.05);
        car.nitro = (car.nitro + 0.16).min(1.0);
        car.vel += dir * 95.0;
        car.pad_cooldown = 0.6;
        ev.pad_boost = true;
    }
    ev
}

/// Outcome of a car-to-car contact.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    pub impact: f32,
    /// Which car (0 = a, 1 = b) was spun out, if any.
    pub spun: Option<usize>,
}

/// Resolve overlap between two cars. `ram` flags say whether a car is ramming.
/// With `spins` off, contact still bumps but only a ramming car spins the other.
pub fn collide(a: &mut CarState, b: &mut CarState, ram: [bool; 2], spins: bool) -> Option<Contact> {
    let d = b.pos - a.pos;
    let dist = d.length();
    let min = CAR_RADIUS * 2.0;
    if dist >= min || dist < 1e-4 {
        return None;
    }
    let n = d / dist;
    let push = (min - dist) * 0.5;
    a.pos -= n * push;
    b.pos += n * push;

    let vrel = (b.vel - a.vel).dot(n);
    if vrel >= 0.0 {
        return Some(Contact { impact: 0.0, spun: None });
    }
    let impact = -vrel;
    let j = (1.0 + 0.35) * impact * 0.5 * if ram.iter().any(|&r| r) { 1.8 } else { 1.0 };
    a.vel -= n * j;
    b.vel += n * j;
    let t = n.perp();
    let vt = (b.vel - a.vel).dot(t);
    let f = vt.clamp(-0.12 * j, 0.12 * j) * 0.5;
    a.vel += t * f;
    b.vel -= t * f;

    // Who hit whom: the hitter drives into the contact at least as fast as the target.
    let a_into = a.vel.dot(n) > 0.0 && a.forward().dot(n) > 0.3;
    let b_into = b.vel.dot(-n) > 0.0 && b.forward().dot(-n) > 0.3;
    let mut spun = None;
    for (hitter, target_idx, into) in [(0usize, 1usize, a_into), (1, 0, b_into)] {
        if !into || !(spins || ram[hitter]) {
            continue;
        }
        let (h, tg) = if hitter == 0 { (&*a, &*b) } else { (&*b, &*a) };
        let threshold = if ram[hitter] {
            12.0
        } else if h.is_boosting() {
            28.0
        } else {
            52.0
        };
        if impact >= threshold && h.speed() >= 0.78 * tg.speed() {
            spun = Some(target_idx);
            break;
        }
    }
    if let Some(idx) = spun {
        let duration = 0.85 + 0.4 * ((impact - 28.0) / 120.0).clamp(0.0, 1.0);
        let dir = if idx == 0 { -n.perp_dot(a.forward()) } else { n.perp_dot(b.forward()) };
        let target = if idx == 0 { a } else { b };
        target.spin_out(duration, 0.58, dir);
    }
    Some(Contact { impact, spun })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Flat(Surface);
    impl Ground for Flat {
        fn probe(&self, _p: Vec2) -> GroundProbe {
            GroundProbe { surface: self.0, wall_push: None, pad: None, track_dir: Vec2::X }
        }
    }

    /// Wall along y = 100 (cars live below it).
    struct WallAbove;
    impl Ground for WallAbove {
        fn probe(&self, p: Vec2) -> GroundProbe {
            let wall_push = (p.y > 100.0).then_some((Vec2::NEG_Y, p.y - 100.0));
            GroundProbe { surface: Surface::Road, wall_push, pad: None, track_dir: Vec2::X }
        }
    }

    fn run(car: &mut CarState, input: CarInput, secs: f32, g: &impl Ground) -> StepEvents {
        let mut all = StepEvents::default();
        let t = CarTuning::default();
        for _ in 0..(secs / DT) as usize {
            all.merge(step(car, input, &t, CarModifiers::default(), g));
        }
        all
    }

    fn full_throttle() -> CarInput {
        CarInput { throttle: 1.0, ..Default::default() }
    }

    #[test]
    fn reaches_top_speed_and_shifts_up() {
        let mut car = CarState::new(Vec2::ZERO, 0.0);
        let ev = run(&mut car, full_throttle(), 20.0, &Flat(Surface::Road));
        assert!(ev.gear_changed);
        assert_eq!(car.gear, 5);
        let v = car.speed();
        assert!((v - BASE_TOP_SPEED).abs() < 15.0, "top speed {v}");
    }

    #[test]
    fn grass_is_much_slower_than_road() {
        let mut road = CarState::new(Vec2::ZERO, 0.0);
        let mut grass = road;
        run(&mut road, full_throttle(), 6.0, &Flat(Surface::Road));
        run(&mut grass, full_throttle(), 6.0, &Flat(Surface::Grass));
        assert!(grass.speed() < road.speed() * 0.6, "{} vs {}", grass.speed(), road.speed());
    }

    #[test]
    fn brake_stops_then_reverses() {
        let mut car = CarState::new(Vec2::ZERO, 0.0);
        run(&mut car, full_throttle(), 3.0, &Flat(Surface::Road));
        let brake = CarInput { brake: 1.0, ..Default::default() };
        run(&mut car, brake, 1.5, &Flat(Surface::Road));
        assert!(car.forward_speed() < 5.0);
        run(&mut car, brake, 3.0, &Flat(Surface::Road));
        let v = car.forward_speed();
        assert!((-REVERSE_MAX - 1.0..-100.0).contains(&v), "reverse {v}");
    }

    /// Keyboard steering is all or nothing, so holding a line at speed means
    /// tapping left and right; that alone must never throw the car into a drift.
    #[test]
    fn quick_steering_corrections_do_not_start_a_drift() {
        let mut car = CarState::new(Vec2::ZERO, 0.0);
        run(&mut car, full_throttle(), 4.0, &Flat(Surface::Road));
        assert!(car.speed() > 300.0, "speed {}", car.speed());
        for (i, tap) in [0.08, 0.12, 0.2, 0.3, 0.1, 0.15, 0.25, 0.05].iter().cycle().take(24).enumerate() {
            let steer = if i % 2 == 0 { 1.0 } else { -1.0 };
            let ev = run(&mut car, CarInput { throttle: 1.0, steer, ..Default::default() }, *tap, &Flat(Surface::Road));
            assert!(!ev.drift_started, "tap {i} ({tap} s) started a drift");
        }
        assert!(!car.drifting);
    }

    #[test]
    fn handbrake_drift_charges_and_releases_boost() {
        let mut car = CarState::new(Vec2::ZERO, 0.0);
        run(&mut car, full_throttle(), 4.0, &Flat(Surface::Road));
        let drift = CarInput { throttle: 1.0, steer: 1.0, handbrake: true, ..Default::default() };
        let ev = run(&mut car, drift, 0.3, &Flat(Surface::Road));
        assert!(ev.drift_started);
        let hold = CarInput { throttle: 1.0, steer: 1.0, ..Default::default() };
        run(&mut car, hold, 2.0, &Flat(Surface::Road));
        assert!(car.drift_charge > DRIFT_TIERS[0] || !car.drifting, "charge {}", car.drift_charge);
        let ev = run(&mut car, full_throttle(), 2.0, &Flat(Surface::Road));
        assert!(!car.drifting);
        assert!(ev.drift_boost.is_some(), "expected a drift boost");
        assert!(ev.drift_points_banked > 0.0);
    }

    #[test]
    fn nitro_drains_and_raises_speed() {
        let mut a = CarState::new(Vec2::ZERO, 0.0);
        a.nitro = 1.0;
        let mut b = a;
        run(&mut a, full_throttle(), 2.0, &Flat(Surface::Road));
        let ev = run(&mut b, CarInput { throttle: 1.0, nitro: true, ..Default::default() }, 2.0, &Flat(Surface::Road));
        assert!(ev.nitro_fired);
        assert!(b.speed() > a.speed() + 20.0);
        assert!(b.nitro < 0.5);
    }

    #[test]
    fn empty_nitro_cuts_out_until_the_button_is_pressed_again() {
        let hold = CarInput { throttle: 1.0, nitro: true, ..Default::default() };
        let mut car = CarState::new(Vec2::ZERO, 0.0);
        car.vel = Vec2::new(300.0, 0.0);
        car.nitro = 1.0;
        run(&mut car, hold, 2.5, &Flat(Surface::Road));
        assert!(car.nitro_active, "still has fuel");
        run(&mut car, hold, 0.5, &Flat(Surface::Road));
        assert!(!car.nitro_active, "burnt out");

        // Long enough to refill past the firing minimum: holding never re-fires.
        let ev = run(&mut car, hold, 12.0, &Flat(Surface::Road));
        assert!(!ev.nitro_fired && !car.nitro_active);
        assert!(car.nitro > NITRO_MIN, "refilled to {}", car.nitro);
        assert!((car.speed() - BASE_TOP_SPEED).abs() < 15.0, "top speed {}", car.speed());

        run(&mut car, full_throttle(), DT * 1.5, &Flat(Surface::Road));
        let ev = run(&mut car, hold, DT * 1.5, &Flat(Surface::Road));
        assert!(ev.nitro_fired && car.nitro_active, "fires again on a new press");
    }

    #[test]
    fn wall_pushes_car_back_and_reports_impact() {
        let mut car = CarState::new(Vec2::new(0.0, 0.0), PI / 2.0);
        car.vel = Vec2::new(0.0, 300.0);
        let ev = run(&mut car, CarInput::default(), 1.0, &WallAbove);
        assert!(car.pos.y <= 100.5, "y={}", car.pos.y);
        assert!(ev.wall_hit.is_some_and(|v| v > 200.0));
        assert!(car.vel.y <= 0.0);
    }

    #[test]
    fn hard_rear_end_spins_the_target() {
        let mut hitter = CarState::new(Vec2::new(0.0, 0.0), 0.0);
        hitter.vel = Vec2::new(400.0, 0.0);
        let mut target = CarState::new(Vec2::new(30.0, 0.0), 0.0);
        target.vel = Vec2::new(250.0, 0.0);
        let c = collide(&mut hitter, &mut target, [false, false], true).unwrap();
        assert!(c.impact > 100.0);
        assert_eq!(c.spun, Some(1));
        assert!(target.spin_time > 0.8);
        assert!(hitter.pos.distance(target.pos) >= CAR_RADIUS * 2.0 - 0.01);
    }

    #[test]
    fn without_spins_contact_only_bumps_unless_ramming() {
        let crash = |speed: f32, ram: bool, spins: bool| {
            let mut hitter = CarState::new(Vec2::ZERO, 0.0);
            hitter.vel = Vec2::new(speed, 0.0);
            let mut target = CarState::new(Vec2::new(30.0, 0.0), 0.0);
            target.vel = Vec2::new(250.0, 0.0);
            let c = collide(&mut hitter, &mut target, [ram, false], spins).unwrap();
            assert!(c.spun.is_some() || target.vel.x > 250.0, "still bumped");
            (c.spun, target.spin_time > 0.0)
        };
        assert_eq!(crash(400.0, false, true), (Some(1), true));
        assert_eq!(crash(400.0, false, false), (None, false));
        assert_eq!(crash(280.0, true, false), (Some(1), true));
    }

    /// Signed angle from `b` to `a`, in `(-PI, PI]`.
    fn angle_between(a: f32, b: f32) -> f32 {
        Vec2::from_angle(b).angle_to(Vec2::from_angle(a))
    }

    /// Spin a car on flat ground, whose track runs along +X, until the spin ends.
    /// Returns the final car and the angle it turned through.
    fn spin_flat(heading: f32, duration: f32, dir: f32, surface: Surface, recovery: bool) -> (CarState, f32) {
        let mut car = CarState::new(Vec2::ZERO, heading);
        car.vel = car.forward() * 300.0;
        car.spin_out(duration, 0.58, dir);
        let mods = CarModifiers { spin_recovery: recovery, ..Default::default() };
        let mut turned = 0.0;
        while car.spin_time > 0.0 {
            let before = car.heading;
            step(&mut car, full_throttle(), &CarTuning::default(), mods, &Flat(surface));
            turned += angle_between(car.heading, before);
        }
        (car, turned)
    }

    #[test]
    fn recovering_spin_ends_facing_along_the_track() {
        for k in 0..24 {
            let heading = k as f32 * TAU / 24.0;
            for duration in [0.72, 0.85, 1.0, 1.25] {
                for dir in [-1.0, 1.0] {
                    for surface in [Surface::Road, Surface::Grass] {
                        let (mut car, turned) = spin_flat(heading, duration, dir, surface, true);
                        let case = format!("heading {heading} duration {duration} dir {dir}");
                        assert!(angle_between(car.heading, 0.0).abs() < 0.01, "{case}: {}", car.heading);
                        assert!(turned * dir > 1.0, "{case}: turned {turned}");
                        // Stays straight and drives off once the spin is over.
                        run(&mut car, full_throttle(), 0.5, &Flat(surface));
                        assert!(angle_between(car.heading, 0.0).abs() < 0.05, "{case}: {} after", car.heading);
                        assert!(car.vel.x > 0.0, "{case}: vel {}", car.vel);
                    }
                }
            }
        }
    }

    #[test]
    fn recovering_spin_makes_one_full_turn_after_a_crash_on_the_straight() {
        for duration in [0.85, 1.05, 1.25] {
            let (_, turned) = spin_flat(0.0, duration, 1.0, Surface::Road, true);
            assert!((turned - TAU).abs() < 0.01, "duration {duration}: turned {turned}");
        }
    }

    #[test]
    fn recovering_spin_survives_a_second_hit() {
        let mods = CarModifiers { spin_recovery: true, ..Default::default() };
        let mut car = CarState::new(Vec2::ZERO, 0.3);
        car.vel = Vec2::new(300.0, 0.0);
        car.spin_out(0.85, 0.58, 1.0);
        for _ in 0..50 {
            step(&mut car, full_throttle(), &CarTuning::default(), mods, &Flat(Surface::Road));
        }
        car.spin_out(1.1, 0.58, -1.0);
        while car.spin_time > 0.0 {
            step(&mut car, full_throttle(), &CarTuning::default(), mods, &Flat(Surface::Road));
        }
        assert!(angle_between(car.heading, 0.0).abs() < 0.01, "{}", car.heading);
    }

    #[test]
    fn recovering_spin_follows_the_road_on_every_track() {
        use crate::core::tracks::{TrackId, track_def};
        let mods = CarModifiers { spin_recovery: true, ..Default::default() };
        for id in TrackId::ALL {
            let track = track_def(id).build();
            for path in track.paths() {
                for s in path.samples.iter().step_by(7) {
                    for dir in [-1.0, 1.0] {
                        let mut car = CarState::new(s.pos, s.tangent.to_angle());
                        car.vel = s.tangent * 350.0;
                        car.spin_out(1.0, 0.58, dir);
                        while car.spin_time > 0.0 {
                            let before = car.heading;
                            step(&mut car, full_throttle(), &CarTuning::default(), mods, &track);
                            let turn = angle_between(car.heading, before).abs();
                            assert!(turn <= SPIN_RATE_MAX * DT + 1e-3, "{id:?}: snapped by {turn}");
                        }
                        let off = angle_between(car.heading, track.query(car.pos).tangent.to_angle());
                        assert!(off.abs() < 0.2, "{id:?} at {}: off by {off}", s.pos);
                    }
                }
            }
        }
    }

    #[test]
    fn spins_without_recovery_keep_the_fixed_rate() {
        let (_, turned) = spin_flat(0.0, 1.0, 1.0, Surface::Road, false);
        assert!((turned - SPIN_RATE).abs() < 2.0 * SPIN_RATE * DT, "turned {turned}");
    }

    #[test]
    fn gentle_contact_does_not_spin() {
        let mut a = CarState::new(Vec2::ZERO, 0.0);
        a.vel = Vec2::new(210.0, 0.0);
        let mut b = CarState::new(Vec2::new(33.0, 0.0), 0.0);
        b.vel = Vec2::new(200.0, 0.0);
        let c = collide(&mut a, &mut b, [false, false], true).unwrap();
        assert_eq!(c.spun, None);
    }
}
