//! Rival driver: follows the centerline with an inside-biased lane, brakes for
//! upcoming corners, overtakes, uses nitro on straights and recovers when stuck.

use bevy::math::Vec2;

use super::car::{CarInput, CarState, CarTuning, DT};
use super::track::Track;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Personality {
    /// Corner speed multiplier.
    pub pace: f32,
    /// How readily the driver flicks the handbrake into tight corners (0..1).
    pub drift_appetite: f32,
    /// Minimum nitro tank before firing on a straight.
    pub nitro_min: f32,
    /// How late the driver brakes (higher = later).
    pub brake_lateness: f32,
}

pub fn personality(slot: usize) -> Personality {
    match slot % 4 {
        0 => Personality { pace: 1.02, drift_appetite: 0.3, nitro_min: 0.45, brake_lateness: 1.05 },
        1 => Personality { pace: 0.98, drift_appetite: 0.8, nitro_min: 0.6, brake_lateness: 0.95 },
        _ => Personality { pace: 0.94, drift_appetite: 0.5, nitro_min: 0.35, brake_lateness: 1.0 },
    }
}

#[derive(Debug, Clone)]
pub struct AiBrain {
    pub personality: Personality,
    stuck_time: f32,
    reverse_time: f32,
    flick_time: f32,
    flick_cooldown: f32,
    lane: f32,
    seed: u32,
}

impl AiBrain {
    pub fn new(slot: usize, personality: Personality) -> Self {
        Self {
            personality,
            stuck_time: 0.0,
            reverse_time: 0.0,
            flick_time: 0.0,
            flick_cooldown: 0.0,
            lane: 0.0,
            seed: 0x1234_5678 ^ (slot as u32).wrapping_mul(2_654_435_761),
        }
    }

    /// Compute this step's input. `lead` is how many laps this car is ahead of
    /// the player (negative when behind), used for rubber-banding.
    pub fn drive(
        &mut self,
        track: &Track,
        me: &CarState,
        tuning: &CarTuning,
        others: &[(Vec2, Vec2)],
        lead: f32,
    ) -> CarInput {
        let speed = me.speed();
        self.flick_cooldown = (self.flick_cooldown - DT).max(0.0);

        // Stuck recovery: reverse briefly, steering the other way.
        if self.reverse_time > 0.0 {
            self.reverse_time -= DT;
            let q = track.query(me.pos);
            let to_track = q.tangent.angle_to(me.forward());
            return CarInput { brake: 1.0, steer: to_track.signum(), ..CarInput::default() };
        }
        if speed < 20.0 {
            self.stuck_time += DT;
            if self.stuck_time > 1.4 {
                self.stuck_time = 0.0;
                self.reverse_time = 0.8;
            }
        } else {
            self.stuck_time = 0.0;
        }

        let mut pace = self.personality.pace;
        if lead > 0.25 {
            pace *= 0.96;
        } else if lead < -0.25 {
            pace *= 1.04;
        }

        let main = &track.main;
        let n = main.samples.len();
        let q = track.query(me.pos);
        let here = ((q.progress / main.length) * n as f32) as usize % n;
        let spacing = main.length / n as f32;

        // Target speed from upcoming curvature, with braking distance.
        let decel = 300.0 * self.personality.brake_lateness;
        let horizon = ((120.0 + speed * 1.1) / spacing) as usize;
        let mut target_speed = 540.0 * tuning.top_speed_mul;
        let mut max_curve: f32 = 0.0;
        for k in 0..horizon.max(8) {
            let s = &main.samples[(here + k) % n];
            let c = s.curvature.abs().max(1e-5);
            max_curve = max_curve.max(c);
            let corner = (600.0 * tuning.grip_mul * pace * pace / c).sqrt();
            let d = k as f32 * spacing;
            target_speed = target_speed.min((corner * corner + 2.0 * decel * d).sqrt());
        }

        // Lane: inside of the next corner, shifted to pass slower cars ahead.
        let aim_ahead = ((48.0 + speed * 0.22) / spacing) as usize;
        let aim_sample = &main.samples[(here + aim_ahead) % n];
        let upcoming = main.samples[(here + aim_ahead * 2) % n].curvature;
        let mut lane = 0.45 * aim_sample.half_width * (upcoming / 0.006).clamp(-1.0, 1.0);
        let fwd = me.forward();
        for &(pos, vel) in others {
            let rel = pos - me.pos;
            let ahead = rel.dot(fwd);
            let side = rel.dot(fwd.perp());
            if ahead > 0.0 && ahead < 190.0 && side.abs() < 40.0 && vel.dot(fwd) < speed + 10.0 {
                lane += if side > 0.0 { -50.0 } else { 50.0 };
            }
        }
        let max_lane = aim_sample.half_width - 14.0;
        self.lane += (lane.clamp(-max_lane, max_lane) - self.lane) * (3.0 * DT).min(1.0);
        let aim = aim_sample.pos + aim_sample.normal * self.lane;

        let desired = (aim - me.pos).to_angle();
        let err = wrap_angle(desired - me.heading);
        let steer = (err * 2.6).clamp(-1.0, 1.0);

        let mut input = CarInput { steer, ..CarInput::default() };
        if speed < target_speed - 10.0 {
            input.throttle = 1.0;
        } else if speed > target_speed + 25.0 {
            input.brake = 1.0;
        } else {
            input.throttle = 0.4;
        }

        // Handbrake flick into tight, fast corners.
        if self.flick_time > 0.0 {
            self.flick_time -= DT;
            input.handbrake = true;
        } else if self.flick_cooldown <= 0.0
            && max_curve > 0.008
            && speed > 200.0
            && err.abs() > 0.25
            && self.rand() < self.personality.drift_appetite * 0.05
        {
            self.flick_time = 0.14;
            self.flick_cooldown = 1.6;
        }

        // Nitro on straights.
        if max_curve < 0.0012 && me.nitro > self.personality.nitro_min && speed > 200.0 {
            input.nitro = true;
        }
        input
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

fn wrap_angle(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut a = a.rem_euclid(tau);
    if a > std::f32::consts::PI {
        a -= tau;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::car::{self, CarModifiers};
    use crate::core::race::LapTracker;
    use crate::core::tracks::{TrackId, track_def};

    /// An AI car should complete a lap of every circuit within a sane time.
    #[test]
    fn ai_completes_a_lap_on_every_track() {
        for id in TrackId::ALL {
            let track = track_def(id).build();
            let (p, t) = track.main.sample_at(track.length() - 40.0);
            let mut state = CarState::new(p, t.to_angle());
            let tuning = CarTuning::default();
            let mut brain = AiBrain::new(1, personality(1));
            let mut lap = LapTracker::new(track.length(), track.query(p).progress, 1);
            let mut now = 0.0;
            while lap.finished_at.is_none() && now < 120.0 {
                let input = brain.drive(&track, &state, &tuning, &[], 0.0);
                car::step(&mut state, input, &tuning, CarModifiers::default(), &track);
                now += DT;
                lap.update(track.length(), track.query(state.pos).progress, now);
            }
            let time = lap.finished_at.unwrap_or(f32::INFINITY);
            assert!(time < 75.0, "{id:?}: AI lap took {time:.1}s");
        }
    }

    #[test]
    fn wrap_angle_stays_in_range() {
        assert!((wrap_angle(3.0 * std::f32::consts::PI) - std::f32::consts::PI).abs() < 1e-4);
        assert!((wrap_angle(-0.5) + 0.5).abs() < 1e-6);
    }
}
