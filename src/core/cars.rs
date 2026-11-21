//! The car catalog: seven original cars, each with its own handling
//! personality, shop price, display stats, and sprite hints.

use serde::{Deserialize, Serialize};

use super::car::CarTuning;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CarId {
    #[default]
    Pebble,
    Sparrow,
    Bandit,
    Mako,
    Tempest,
    Halcyon,
    Baron,
}

impl CarId {
    /// Every car in shop order.
    pub const ALL: [CarId; 7] = [
        CarId::Pebble,
        CarId::Sparrow,
        CarId::Bandit,
        CarId::Mako,
        CarId::Tempest,
        CarId::Halcyon,
        CarId::Baron,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&c| c == self).unwrap_or(0)
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    /// Stable kebab-case key used in save files.
    pub fn key(self) -> &'static str {
        match self {
            CarId::Pebble => "pebble",
            CarId::Sparrow => "sparrow",
            CarId::Bandit => "bandit",
            CarId::Mako => "mako",
            CarId::Tempest => "tempest",
            CarId::Halcyon => "halcyon",
            CarId::Baron => "baron",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }
}

/// Silhouette family used by the procedural sprite generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BodyStyle {
    Hatch,
    Coupe,
    Rally,
    Sports,
    Wedge,
    Tourer,
    Muscle,
}

/// Index names for `CarSpec::stats`.
pub const STAT_LABELS: [&str; 4] = ["SPEED", "ACCEL", "DRIFT", "NITRO"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarSpec {
    pub id: CarId,
    pub name: &'static str,
    pub blurb: &'static str,
    /// Shop price in credits; 0 for starter cars.
    pub price: u32,
    /// Display ratings 1..=5: speed, accel, drift, nitro.
    pub stats: [u8; 4],
    pub tuning: CarTuning,
    pub body: BodyStyle,
    pub base_color: [u8; 3],
    pub accent_color: [u8; 3],
}

impl CarSpec {
    pub fn is_starter(&self) -> bool {
        self.price == 0
    }
}

#[allow(clippy::too_many_arguments)]
const fn tuning(
    top_speed_mul: f32,
    accel_mul: f32,
    grip_mul: f32,
    drift_control_mul: f32,
    nitro_duration: f32,
    nitro_top_bonus: f32,
    nitro_accel_mul: f32,
    nitro_refill_mul: f32,
) -> CarTuning {
    CarTuning {
        top_speed_mul,
        accel_mul,
        grip_mul,
        drift_control_mul,
        nitro_duration,
        nitro_top_bonus,
        nitro_accel_mul,
        nitro_refill_mul,
    }
}

pub fn car_spec(id: CarId) -> CarSpec {
    match id {
        CarId::Pebble => CarSpec {
            id,
            name: "Pebble",
            blurb: "A plucky little hatch. Forgiving grip, honest pace.",
            price: 0,
            stats: [2, 3, 3, 2],
            tuning: tuning(1.00, 1.03, 1.04, 1.00, 2.8, 76.0, 0.62, 1.00),
            body: BodyStyle::Hatch,
            base_color: [232, 124, 52],
            accent_color: [250, 226, 170],
        },
        CarId::Sparrow => CarSpec {
            id,
            name: "Sparrow",
            blurb: "Light coupe that refills nitro fast. Short bursts, often.",
            price: 0,
            stats: [2, 3, 3, 3],
            tuning: tuning(1.02, 1.06, 1.00, 1.04, 2.6, 72.0, 0.58, 1.12),
            body: BodyStyle::Coupe,
            base_color: [72, 148, 230],
            accent_color: [236, 240, 250],
        },
        CarId::Bandit => CarSpec {
            id,
            name: "Bandit",
            blurb: "Loose rally build. Lives sideways and loves gravel.",
            price: 450,
            stats: [3, 3, 5, 3],
            tuning: tuning(1.04, 1.07, 0.98, 1.14, 3.0, 80.0, 0.64, 1.15),
            body: BodyStyle::Rally,
            base_color: [70, 170, 90],
            accent_color: [245, 210, 60],
        },
        CarId::Mako => CarSpec {
            id,
            name: "Mako",
            blurb: "Planted sports car. Bites into corners, hates sliding.",
            price: 650,
            stats: [4, 4, 3, 3],
            tuning: tuning(1.08, 1.10, 1.10, 0.98, 2.8, 84.0, 0.68, 1.00),
            body: BodyStyle::Sports,
            base_color: [60, 90, 120],
            accent_color: [120, 230, 230],
        },
        CarId::Tempest => CarSpec {
            id,
            name: "Tempest",
            blurb: "Razor wedge with the hardest launch and the biggest nitro.",
            price: 900,
            stats: [4, 5, 3, 5],
            tuning: tuning(1.10, 1.16, 1.06, 1.02, 3.4, 98.0, 0.78, 1.05),
            body: BodyStyle::Wedge,
            base_color: [150, 70, 200],
            accent_color: [255, 120, 200],
        },
        CarId::Halcyon => CarSpec {
            id,
            name: "Halcyon",
            blurb: "Grand tourer with no weak spot. Calm, quick, composed.",
            price: 1150,
            stats: [4, 4, 4, 4],
            tuning: tuning(1.09, 1.08, 1.12, 1.10, 3.1, 88.0, 0.70, 1.08),
            body: BodyStyle::Tourer,
            base_color: [232, 232, 220],
            accent_color: [200, 160, 60],
        },
        CarId::Baron => CarSpec {
            id,
            name: "Baron",
            blurb: "Brutal muscle. Untouchable on straights, a handful in bends.",
            price: 1500,
            stats: [5, 2, 2, 4],
            tuning: tuning(1.13, 1.00, 0.98, 0.94, 3.2, 94.0, 0.72, 0.94),
            body: BodyStyle::Muscle,
            base_color: [170, 30, 40],
            accent_color: [20, 20, 24],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn within(v: f32, lo: f32, hi: f32) -> bool {
        (lo - 1e-4..=hi + 1e-4).contains(&v)
    }

    #[test]
    fn index_round_trips_and_keys_are_unique() {
        for (i, id) in CarId::ALL.into_iter().enumerate() {
            assert_eq!(id.index(), i);
            assert_eq!(CarId::from_index(i), Some(id));
            assert_eq!(CarId::from_key(id.key()), Some(id));
            assert_eq!(car_spec(id).id, id);
        }
        assert_eq!(CarId::from_index(CarId::ALL.len()), None);
        let mut keys: Vec<_> = CarId::ALL.iter().map(|c| c.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), CarId::ALL.len());
    }

    #[test]
    fn starters_are_free_and_prices_increase() {
        assert!(car_spec(CarId::Pebble).is_starter());
        assert!(car_spec(CarId::Sparrow).is_starter());
        let paid: Vec<u32> = CarId::ALL[2..].iter().map(|&c| car_spec(c).price).collect();
        assert!(paid.iter().all(|p| (450..=1500).contains(p)), "{paid:?}");
        assert!(paid.windows(2).all(|w| w[0] < w[1]), "{paid:?}");
    }

    #[test]
    fn tuning_and_stats_stay_in_range() {
        for id in CarId::ALL {
            let s = car_spec(id);
            let t = s.tuning;
            assert!(within(t.top_speed_mul, 1.00, 1.13), "{id:?}");
            assert!(within(t.accel_mul, 1.00, 1.16), "{id:?}");
            assert!(within(t.grip_mul, 0.98, 1.12), "{id:?}");
            assert!(within(t.drift_control_mul, 0.94, 1.14), "{id:?}");
            assert!(within(t.nitro_duration, 2.6, 3.4), "{id:?}");
            assert!(within(t.nitro_top_bonus, 72.0, 98.0), "{id:?}");
            assert!(within(t.nitro_accel_mul, 0.58, 0.78), "{id:?}");
            assert!(within(t.nitro_refill_mul, 0.94, 1.15), "{id:?}");
            assert!(s.stats.iter().all(|v| (1..=5).contains(v)), "{id:?}");
            assert!(!s.name.is_empty() && !s.blurb.is_empty());
        }
    }

    #[test]
    fn personalities_are_distinct() {
        let top = |c| car_spec(c).tuning.top_speed_mul;
        let drift = |c| car_spec(c).tuning.drift_control_mul;
        assert!(CarId::ALL.iter().all(|&c| top(CarId::Baron) >= top(c)));
        assert!(CarId::ALL.iter().all(|&c| drift(CarId::Bandit) >= drift(c)));
        let launch = |c| car_spec(c).tuning.accel_mul;
        assert!(
            CarId::ALL
                .iter()
                .all(|&c| launch(CarId::Tempest) >= launch(c))
        );
        let mut tunings: Vec<_> = CarId::ALL.iter().map(|&c| car_spec(c).tuning).collect();
        tunings.dedup();
        assert_eq!(tunings.len(), CarId::ALL.len());
    }
}
