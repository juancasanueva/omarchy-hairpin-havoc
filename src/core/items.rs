//! Lucky boxes, box items and gadget effects (pure rules, no ECS).
//!
//! Distances are world pixels, speeds px/s, times seconds. Each effect is a
//! small function over `CarState`s so the Bevy adapter only has to pick
//! targets, play sounds and draw.

use bevy::math::Vec2;

use super::car::{CAR_RADIUS, CarState};

// ---------------------------------------------------------------------------
// Lucky boxes
// ---------------------------------------------------------------------------

/// Pickup radius of a lucky box.
pub const BOX_RADIUS: f32 = 27.0;
/// Lateral offset of the two boxes of a station from the centerline.
pub const BOX_LATERAL: f32 = 23.0;
/// Seconds a collected box stays hidden.
pub const BOX_RESPAWN: f32 = 3.0;

/// Whether the segment `a -> b` passes within `r` of `center` (swept pickup).
pub fn segment_hits_circle(a: Vec2, b: Vec2, center: Vec2, r: f32) -> bool {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 < 1e-8 { 0.0 } else { ((center - a).dot(ab) / len2).clamp(0.0, 1.0) };
    (a + ab * t).distance_squared(center) <= r * r
}

/// Lap index from accumulated lap distance: -1 on the grid, 0 on lap one, ...
pub fn lap_index(total: f32, track_len: f32) -> i32 {
    if track_len <= 0.0 {
        return 0;
    }
    (total / track_len).floor() as i32
}

/// The two box positions of a station at `center` with lap tangent `tangent`.
pub fn station_boxes(center: Vec2, tangent: Vec2) -> [Vec2; 2] {
    let n = tangent.normalize_or(Vec2::X).perp();
    [center + n * BOX_LATERAL, center - n * BOX_LATERAL]
}

/// Small xorshift PRNG for box rolls; never returns 0 for a non-zero state.
pub fn next_rand(state: &mut u32) -> u32 {
    let mut x = if *state == 0 { 0x9e37_79b9 } else { *state };
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Drift points the player earns for spinning out a rival with an effect.
pub const SPINOUT_POINTS: f32 = 150.0;

/// Spin direction for a car pushed along `push`: away from the side it was hit on.
pub fn spin_sign(push: Vec2, target_forward: Vec2) -> f32 {
    if target_forward.perp_dot(push) >= 0.0 { 1.0 } else { -1.0 }
}

/// Unit direction from `from` to `to`, falling back to `fallback` when they coincide.
fn away(from: Vec2, to: Vec2, fallback: Vec2) -> Vec2 {
    (to - from).try_normalize().unwrap_or_else(|| fallback.normalize_or(Vec2::X))
}

/// Outcome of an effect on one target car.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Miss,
    /// Affected, but not (newly) spun out.
    Hit,
    /// The target started a new spinout because of this effect.
    SpunOut,
}

impl Hit {
    pub fn landed(self) -> bool {
        self != Hit::Miss
    }
}

/// Spin `target` out and report whether that started a new spinout.
fn spin(target: &mut CarState, duration: f32, speed_mul: f32, push: Vec2) -> Hit {
    let was_spinning = target.spin_time > 0.0;
    let dir = spin_sign(push, target.forward());
    target.spin_out(duration, speed_mul, dir);
    if was_spinning { Hit::Hit } else { Hit::SpunOut }
}

// ---------------------------------------------------------------------------
// Thumper
// ---------------------------------------------------------------------------

pub const THUMPER_RADIUS: f32 = 195.0;
pub const THUMPER_SPIN_RADIUS: f32 = 135.0;
pub const THUMPER_PUSH: f32 = 220.0;
pub const THUMPER_SELF_KICK: f32 = 42.0;

/// Shockwave from `user_pos` on one other car.
pub fn thumper_hit(user_pos: Vec2, target: &mut CarState) -> Hit {
    let d = user_pos.distance(target.pos);
    if d >= THUMPER_RADIUS {
        return Hit::Miss;
    }
    let dir = away(user_pos, target.pos, target.forward());
    let falloff = 1.0 - d / THUMPER_RADIUS;
    let hit = if d < THUMPER_SPIN_RADIUS { spin(target, 0.72, 0.65, dir) } else { Hit::Hit };
    target.vel += dir * THUMPER_PUSH * falloff;
    hit
}

/// Forward hop the user gets when firing the thumper.
pub fn thumper_self(user: &mut CarState) {
    user.vel += user.forward() * THUMPER_SELF_KICK;
}

// ---------------------------------------------------------------------------
// Slick
// ---------------------------------------------------------------------------

pub const SLICK_RADIUS: f32 = 20.0;
/// A car hits a slick when its center is within this distance of the slick.
pub const SLICK_HIT_RANGE: f32 = SLICK_RADIUS + CAR_RADIUS * 0.5;
pub const SLICK_BEHIND: f32 = 40.0;
pub const SLICK_LIFETIME: f32 = 10.0;
pub const SLICK_MAX: usize = 6;
pub const SLICK_MAX_HITS: usize = 2;
pub const SLICK_OWNER_GRACE: f32 = 0.5;

/// Where a slick lands when dropped by `user`.
pub fn slick_drop_pos(user: &CarState) -> Vec2 {
    user.pos - user.forward() * SLICK_BEHIND
}

/// An oil slick on the track.
#[derive(Debug, Clone, PartialEq)]
pub struct Slick {
    pub pos: Vec2,
    /// Racer slot that dropped it.
    pub owner: usize,
    pub age: f32,
    /// Slots already hit (each car once, at most `SLICK_MAX_HITS`).
    pub hit: Vec<usize>,
}

impl Slick {
    pub fn new(pos: Vec2, owner: usize) -> Self {
        Self { pos, owner, age: 0.0, hit: Vec::new() }
    }

    pub fn expired(&self) -> bool {
        self.age >= SLICK_LIFETIME || self.hit.len() >= SLICK_MAX_HITS
    }

    /// Whether racer `slot` at `pos` would hit this slick now.
    pub fn touches(&self, slot: usize, pos: Vec2) -> bool {
        !self.expired()
            && !(slot == self.owner && self.age < SLICK_OWNER_GRACE)
            && !self.hit.contains(&slot)
            && self.pos.distance(pos) < SLICK_HIT_RANGE
    }

    /// Spin out `target` (racer `slot`) if it touches the slick.
    pub fn try_hit(&mut self, slot: usize, target: &mut CarState) -> Hit {
        if !self.touches(slot, target.pos) {
            return Hit::Miss;
        }
        self.hit.push(slot);
        let push = target.vel.try_normalize().unwrap_or(target.forward());
        spin(target, 1.15, 0.56, push)
    }
}

/// Adds a slick, removing the oldest ones beyond `SLICK_MAX`. Returns the
/// number of removed slicks (they were at the front of `list`).
pub fn push_slick(list: &mut Vec<Slick>, slick: Slick) -> usize {
    list.push(slick);
    let excess = list.len().saturating_sub(SLICK_MAX);
    list.drain(..excess);
    excess
}

// ---------------------------------------------------------------------------
// Static burst
// ---------------------------------------------------------------------------

pub const STATIC_RADIUS: f32 = 285.0;
pub const STATIC_POINTS: f32 = 100.0;

pub fn static_hit(user_pos: Vec2, target: &mut CarState) -> Hit {
    if user_pos.distance(target.pos) >= STATIC_RADIUS {
        return Hit::Miss;
    }
    target.slow_down(2.8, 0.68);
    Hit::Hit
}

// ---------------------------------------------------------------------------
// Battering boost
// ---------------------------------------------------------------------------

pub const RAM_START_KICK: f32 = 55.0;
/// Center distance that counts as contact (cars are separated after physics).
pub const RAM_CONTACT: f32 = CAR_RADIUS * 2.0 + 2.0;
pub const RAM_TARGET_PUSH: f32 = 135.0;
pub const RAM_USER_BONUS: f32 = 28.0;

pub fn battering_start(user: &mut CarState) {
    user.vel += user.forward() * RAM_START_KICK;
}

/// Ram contact between the ramming `user` and `target`. The user keeps its drift charge.
pub fn ram_hit(user: &mut CarState, target: &mut CarState) -> Hit {
    if user.pos.distance(target.pos) >= RAM_CONTACT {
        return Hit::Miss;
    }
    let dir = away(user.pos, target.pos, user.forward());
    let hit = spin(target, 1.25, 0.7, dir);
    target.vel += dir * RAM_TARGET_PUSH;
    let charge = user.drift_charge;
    user.vel += user.forward() * RAM_USER_BONUS;
    user.drift_charge = charge;
    hit
}

// ---------------------------------------------------------------------------
// All-terrain, jammer
// ---------------------------------------------------------------------------

pub const ALL_TERRAIN_NITRO: f32 = 0.55;

pub fn all_terrain_start(user: &mut CarState) {
    user.nitro = (user.nitro + ALL_TERRAIN_NITRO).min(1.0);
}

/// Jammer target: the nearest rival ahead by lap distance, or the nearest
/// behind when the user leads. `rivals` are `(id, lap_total)`.
pub fn jammer_target(user_total: f32, rivals: &[(usize, f32)]) -> Option<usize> {
    let ahead = rivals
        .iter()
        .filter(|r| r.1 > user_total)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    let behind = || {
        rivals
            .iter()
            .filter(|r| r.1 <= user_total)
            .max_by(|a, b| a.1.total_cmp(&b.1))
    };
    ahead.or_else(behind).map(|r| r.0)
}

pub fn jammer_hit(target: &mut CarState) {
    target.slow_down(1.4, 0.9);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn car(x: f32, y: f32) -> CarState {
        CarState::new(Vec2::new(x, y), 0.0)
    }

    #[test]
    fn swept_pickup_catches_fast_pass_through() {
        let c = Vec2::new(50.0, 0.0);
        // Both endpoints are far outside the radius, but the path crosses it.
        assert!(segment_hits_circle(Vec2::new(0.0, 5.0), Vec2::new(100.0, 5.0), c, BOX_RADIUS));
        assert!(!segment_hits_circle(Vec2::new(0.0, 40.0), Vec2::new(100.0, 40.0), c, BOX_RADIUS));
        // Degenerate segment acts like a point test.
        assert!(segment_hits_circle(Vec2::new(60.0, 0.0), Vec2::new(60.0, 0.0), c, BOX_RADIUS));
        assert!(!segment_hits_circle(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), c, BOX_RADIUS));
    }

    #[test]
    fn lap_index_counts_from_the_line() {
        assert_eq!(lap_index(-20.0, 1000.0), -1);
        assert_eq!(lap_index(0.0, 1000.0), 0);
        assert_eq!(lap_index(999.0, 1000.0), 0);
        assert_eq!(lap_index(1500.0, 1000.0), 1);
    }

    #[test]
    fn station_boxes_sit_either_side_of_the_line() {
        let [a, b] = station_boxes(Vec2::ZERO, Vec2::X);
        assert!((a.y - BOX_LATERAL).abs() < 1e-4 && a.x.abs() < 1e-4);
        assert!((b.y + BOX_LATERAL).abs() < 1e-4);
    }

    #[test]
    fn rng_is_deterministic_and_nonzero() {
        let (mut a, mut b) = (0u32, 0u32);
        for _ in 0..100 {
            let x = next_rand(&mut a);
            assert_eq!(x, next_rand(&mut b));
            assert_ne!(x, 0);
        }
    }

    #[test]
    fn thumper_pushes_with_falloff_and_spins_close_cars() {
        let mut near = car(100.0, 0.0);
        assert_eq!(thumper_hit(Vec2::ZERO, &mut near), Hit::SpunOut);
        assert!(near.spin_time > 0.0);
        let expected = THUMPER_PUSH * (1.0 - 100.0 / THUMPER_RADIUS);
        assert!((near.vel.x - expected).abs() < 1e-3);

        let mut mid = car(0.0, 160.0);
        assert_eq!(thumper_hit(Vec2::ZERO, &mut mid), Hit::Hit);
        assert_eq!(mid.spin_time, 0.0);
        assert!(mid.vel.y > 0.0);

        let mut far = car(200.0, 0.0);
        assert_eq!(thumper_hit(Vec2::ZERO, &mut far), Hit::Miss);
        assert_eq!(far.vel, Vec2::ZERO);
    }

    #[test]
    fn thumper_on_spinning_car_is_not_a_new_spinout() {
        let mut c = car(50.0, 0.0);
        c.spin_out(1.0, 1.0, 1.0);
        assert_eq!(thumper_hit(Vec2::ZERO, &mut c), Hit::Hit);
    }

    #[test]
    fn thumper_self_kicks_forward() {
        let mut u = car(0.0, 0.0);
        thumper_self(&mut u);
        assert!((u.vel.x - THUMPER_SELF_KICK).abs() < 1e-4);
    }

    #[test]
    fn slick_drops_behind_the_user() {
        let mut u = car(100.0, 0.0);
        u.heading = std::f32::consts::FRAC_PI_2;
        let p = slick_drop_pos(&u);
        assert!((p - Vec2::new(100.0, -SLICK_BEHIND)).length() < 1e-3);
    }

    #[test]
    fn slick_spares_owner_during_grace_and_hits_two_cars_max() {
        let mut s = Slick::new(Vec2::ZERO, 2);
        let mut owner = car(0.0, 0.0);
        owner.vel = Vec2::new(200.0, 0.0);
        assert_eq!(s.try_hit(2, &mut owner), Hit::Miss);
        s.age = SLICK_OWNER_GRACE;
        assert_eq!(s.try_hit(2, &mut owner), Hit::SpunOut);
        assert!((owner.vel.x - 200.0 * 0.56).abs() < 1e-3);
        // Same car never hit twice.
        owner.spin_time = 0.0;
        assert_eq!(s.try_hit(2, &mut owner), Hit::Miss);
        let mut other = car(5.0, 0.0);
        assert_eq!(s.try_hit(0, &mut other), Hit::SpunOut);
        assert!(s.expired());
        let mut third = car(0.0, 5.0);
        assert_eq!(s.try_hit(1, &mut third), Hit::Miss);
    }

    #[test]
    fn slick_misses_far_cars_and_expires_with_age() {
        let mut s = Slick::new(Vec2::ZERO, 0);
        let mut c = car(SLICK_HIT_RANGE + 1.0, 0.0);
        assert_eq!(s.try_hit(1, &mut c), Hit::Miss);
        s.age = SLICK_LIFETIME;
        assert!(s.expired());
    }

    #[test]
    fn slick_list_keeps_the_newest_six() {
        let mut list = Vec::new();
        for i in 0..SLICK_MAX {
            assert_eq!(push_slick(&mut list, Slick::new(Vec2::splat(i as f32), 0)), 0);
        }
        assert_eq!(push_slick(&mut list, Slick::new(Vec2::splat(99.0), 0)), 1);
        assert_eq!(list.len(), SLICK_MAX);
        assert_eq!(list[0].pos, Vec2::splat(1.0));
        assert_eq!(list[SLICK_MAX - 1].pos, Vec2::splat(99.0));
    }

    #[test]
    fn static_burst_slows_and_cancels_boost_in_range() {
        let mut c = car(200.0, 0.0);
        c.vel = Vec2::new(300.0, 0.0);
        c.add_boost(1.0, 0.0);
        assert_eq!(static_hit(Vec2::ZERO, &mut c), Hit::Hit);
        assert!((c.vel.x - 300.0 * 0.68).abs() < 1e-3);
        assert!(c.slow_time >= 2.8 && !c.is_boosting());
        let mut far = car(STATIC_RADIUS + 1.0, 0.0);
        assert_eq!(static_hit(Vec2::ZERO, &mut far), Hit::Miss);
    }

    #[test]
    fn ram_shoves_and_spins_target_and_keeps_user_charge() {
        let mut user = car(0.0, 0.0);
        user.drift_charge = 0.5;
        let mut target = car(CAR_RADIUS * 2.0, 0.0);
        assert_eq!(ram_hit(&mut user, &mut target), Hit::SpunOut);
        assert!(target.spin_time >= 1.25);
        assert!((target.vel.x - RAM_TARGET_PUSH).abs() < 1e-3);
        assert!((user.vel.x - RAM_USER_BONUS).abs() < 1e-3);
        assert_eq!(user.drift_charge, 0.5);

        let mut apart = car(RAM_CONTACT + 1.0, 0.0);
        assert_eq!(ram_hit(&mut user, &mut apart), Hit::Miss);
    }

    #[test]
    fn battering_and_all_terrain_starts() {
        let mut u = car(0.0, 0.0);
        battering_start(&mut u);
        assert!((u.vel.x - RAM_START_KICK).abs() < 1e-4);
        u.nitro = 0.3;
        all_terrain_start(&mut u);
        assert!((u.nitro - 0.85).abs() < 1e-5);
        all_terrain_start(&mut u);
        assert_eq!(u.nitro, 1.0);
    }

    #[test]
    fn jammer_targets_nearest_ahead_else_nearest_behind() {
        let rivals = [(0, 500.0), (1, 300.0), (3, 100.0)];
        assert_eq!(jammer_target(200.0, &rivals), Some(1));
        assert_eq!(jammer_target(50.0, &rivals), Some(3));
        assert_eq!(jammer_target(900.0, &rivals), Some(0));
        assert_eq!(jammer_target(0.0, &[]), None);
    }

    #[test]
    fn jammer_slows_target() {
        let mut c = car(0.0, 0.0);
        c.vel = Vec2::new(100.0, 0.0);
        jammer_hit(&mut c);
        assert!((c.vel.x - 90.0).abs() < 1e-3 && c.slow_time >= 1.4);
    }

    #[test]
    fn spin_sign_depends_on_hit_side() {
        assert_eq!(spin_sign(Vec2::Y, Vec2::X), 1.0);
        assert_eq!(spin_sign(-Vec2::Y, Vec2::X), -1.0);
    }
}
