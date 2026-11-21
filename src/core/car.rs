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
