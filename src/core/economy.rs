//! Garage economy: performance upgrades, paints, gadgets, lucky-box rolls,
//! and race payouts. Pure data and arithmetic only.

use serde::{Deserialize, Serialize};

use super::car::CarTuning;
use super::cars::CarSpec;

// ---------------------------------------------------------------------------
// Upgrades
// ---------------------------------------------------------------------------

pub const MAX_UPGRADE_LEVEL: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UpgradeKind {
    Engine,
    Tires,
    Exhaust,
}

impl UpgradeKind {
    pub const ALL: [UpgradeKind; 3] = [
        UpgradeKind::Engine,
        UpgradeKind::Tires,
        UpgradeKind::Exhaust,
    ];

    pub fn name(self) -> &'static str {
        match self {
            UpgradeKind::Engine => "ENGINE",
            UpgradeKind::Tires => "TIRES",
            UpgradeKind::Exhaust => "EXHAUST",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            UpgradeKind::Engine => "+4% top speed and +5% acceleration per stage.",
            UpgradeKind::Tires => "+6% grip and drift control per stage.",
            UpgradeKind::Exhaust => "Longer, stronger nitro that refills faster.",
        }
    }

    fn prices(self) -> [u32; MAX_UPGRADE_LEVEL as usize] {
        match self {
            UpgradeKind::Engine => [150, 320],
            UpgradeKind::Tires => [140, 300],
            UpgradeKind::Exhaust => [160, 340],
        }
    }
}

/// Installed upgrade stages for one car, each 0..=`MAX_UPGRADE_LEVEL`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct UpgradeLevels {
    pub engine: u8,
    pub tires: u8,
    pub exhaust: u8,
}

impl UpgradeLevels {
    pub fn get(self, kind: UpgradeKind) -> u8 {
        match kind {
            UpgradeKind::Engine => self.engine,
            UpgradeKind::Tires => self.tires,
            UpgradeKind::Exhaust => self.exhaust,
        }
    }

    pub fn set(&mut self, kind: UpgradeKind, level: u8) {
        let level = level.min(MAX_UPGRADE_LEVEL);
        match kind {
            UpgradeKind::Engine => self.engine = level,
            UpgradeKind::Tires => self.tires = level,
            UpgradeKind::Exhaust => self.exhaust = level,
        }
    }

    pub fn clamped(self) -> Self {
        Self {
            engine: self.engine.min(MAX_UPGRADE_LEVEL),
            tires: self.tires.min(MAX_UPGRADE_LEVEL),
            exhaust: self.exhaust.min(MAX_UPGRADE_LEVEL),
        }
    }
}

/// Price of the next stage of `kind`, or `None` when already maxed.
pub fn upgrade_price(kind: UpgradeKind, current_level: u8) -> Option<u32> {
    kind.prices().get(current_level as usize).copied()
}

/// Applies installed upgrade stages on top of a car's base tuning.
pub fn apply_upgrades(base: CarTuning, levels: UpgradeLevels) -> CarTuning {
    let l = levels.clamped();
    let engine = f32::from(l.engine);
    let tires = f32::from(l.tires);
    let exhaust = f32::from(l.exhaust);
    CarTuning {
        top_speed_mul: base.top_speed_mul * (1.0 + 0.04 * engine),
        accel_mul: base.accel_mul * (1.0 + 0.05 * engine),
        grip_mul: base.grip_mul * (1.0 + 0.06 * tires),
        drift_control_mul: base.drift_control_mul * (1.0 + 0.06 * tires),
        nitro_duration: base.nitro_duration * (1.0 + 0.14 * exhaust),
        nitro_top_bonus: base.nitro_top_bonus + 10.0 * exhaust,
        nitro_accel_mul: base.nitro_accel_mul * (1.0 + 0.12 * exhaust),
        nitro_refill_mul: base.nitro_refill_mul * (1.0 + 0.18 * exhaust),
    }
}

// ---------------------------------------------------------------------------
// Gadgets
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GadgetId {
    #[default]
    Thumper,
    Slick,
    StaticBurst,
    BatteringBoost,
    AllTerrain,
    Phantom,
    DriftSurge,
}

impl GadgetId {
    pub const ALL: [GadgetId; 7] = [
        GadgetId::Thumper,
        GadgetId::Slick,
        GadgetId::StaticBurst,
        GadgetId::BatteringBoost,
        GadgetId::AllTerrain,
        GadgetId::Phantom,
        GadgetId::DriftSurge,
    ];

    pub fn name(self) -> &'static str {
        match self {
            GadgetId::Thumper => "THUMPER",
            GadgetId::Slick => "SLICK",
            GadgetId::StaticBurst => "STATIC BURST",
            GadgetId::BatteringBoost => "BATTERING BOOST",
            GadgetId::AllTerrain => "ALL-TERRAIN",
            GadgetId::Phantom => "PHANTOM",
            GadgetId::DriftSurge => "DRIFT SURGE",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            GadgetId::Thumper => "Shockwave that shoves nearby rivals aside.",
            GadgetId::Slick => "Drops an oil slick that spins out whoever follows.",
            GadgetId::StaticBurst => "Zaps nearby rivals, briefly cutting their engines.",
            GadgetId::BatteringBoost => "Armored surge: ram rivals off line without losing pace.",
            GadgetId::AllTerrain => "Ignore grass and sand slowdowns for a few seconds.",
            GadgetId::Phantom => "Turn intangible and slip straight through traffic.",
            GadgetId::DriftSurge => "Drifts fill nitro twice as fast for a while.",
        }
    }

    /// Shop price in credits; 0 for starter gadgets.
    pub fn price(self) -> u32 {
        match self {
            GadgetId::Thumper | GadgetId::Slick => 0,
            GadgetId::StaticBurst => 160,
            GadgetId::BatteringBoost => 220,
            GadgetId::AllTerrain => 260,
            GadgetId::Phantom => 340,
            GadgetId::DriftSurge => 420,
        }
    }

    /// Seconds between uses.
    pub fn cooldown(self) -> f32 {
        match self {
            GadgetId::Thumper => 11.0,
            GadgetId::Slick => 9.0,
            GadgetId::StaticBurst => 13.0,
            GadgetId::BatteringBoost => 11.0,
            GadgetId::AllTerrain => 15.0,
            GadgetId::Phantom => 14.0,
            GadgetId::DriftSurge => 16.0,
        }
    }

    /// Seconds the effect stays active once triggered.
    pub fn active(self) -> f32 {
        match self {
            GadgetId::Thumper => 0.45,
            GadgetId::Slick => 0.4,
            GadgetId::StaticBurst => 0.45,
            GadgetId::BatteringBoost => 2.4,
            GadgetId::AllTerrain => 2.8,
            GadgetId::Phantom => 3.5,
            GadgetId::DriftSurge => 5.0,
        }
    }

    pub fn is_starter(self) -> bool {
        self.price() == 0
    }
}

// ---------------------------------------------------------------------------
// Lucky boxes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoxItem {
    Slick,
    Thumper,
    Jammer,
}

impl BoxItem {
    pub const ALL: [BoxItem; 3] = [BoxItem::Slick, BoxItem::Thumper, BoxItem::Jammer];

    pub fn name(self) -> &'static str {
        match self {
            BoxItem::Slick => "SLICK",
            BoxItem::Thumper => "THUMPER",
            BoxItem::Jammer => "JAMMER",
        }
    }

    /// The equipped gadget that duplicates this item, if any.
    fn twin_gadget(self) -> Option<GadgetId> {
        match self {
            BoxItem::Slick => Some(GadgetId::Slick),
            BoxItem::Thumper => Some(GadgetId::Thumper),
            BoxItem::Jammer => None,
        }
    }
}

/// Picks a lucky-box item uniformly, skipping the previous item and the item
/// that duplicates the equipped gadget. At most two of three items are ever
/// excluded, so a result always exists.
pub fn roll_box_item(rng_u32: u32, last: Option<BoxItem>, equipped: GadgetId) -> BoxItem {
    let mut pool = [BoxItem::Slick; 3];
    let mut len = 0;
    for item in BoxItem::ALL {
        if Some(item) != last && item.twin_gadget() != Some(equipped) {
            pool[len] = item;
            len += 1;
        }
    }
    if len == 0 {
        // Unreachable with three items and two exclusions; kept as a guard.
        return BoxItem::ALL[rng_u32 as usize % BoxItem::ALL.len()];
    }
    pool[rng_u32 as usize % len]
}

// ---------------------------------------------------------------------------
// Paints
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaintId {
    /// The car's own catalog colors.
    #[default]
    Factory,
    Sunburst,
    Glacier,
    Toxic,
    Midnight,
    Candy,
}

impl PaintId {
    pub const ALL: [PaintId; 6] = [
        PaintId::Factory,
        PaintId::Sunburst,
        PaintId::Glacier,
        PaintId::Toxic,
        PaintId::Midnight,
        PaintId::Candy,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PaintId::Factory => "FACTORY",
            PaintId::Sunburst => "SUNBURST",
            PaintId::Glacier => "GLACIER",
            PaintId::Toxic => "TOXIC",
            PaintId::Midnight => "MIDNIGHT",
            PaintId::Candy => "CANDY",
        }
    }

    pub fn price(self) -> u32 {
        match self {
            PaintId::Factory => 0,
            PaintId::Sunburst => 60,
            PaintId::Glacier => 90,
            PaintId::Toxic => 130,
            PaintId::Midnight => 170,
            PaintId::Candy => 220,
        }
    }

    /// Body color of the scheme. `Factory` reports a neutral swatch; use
    /// `resolve_colors` to get the car's own colors for it.
    pub fn body_color(self) -> [u8; 3] {
        match self {
            PaintId::Factory => [180, 184, 190],
            PaintId::Sunburst => [250, 176, 40],
            PaintId::Glacier => [170, 220, 245],
            PaintId::Toxic => [140, 230, 40],
            PaintId::Midnight => [28, 32, 64],
            PaintId::Candy => [240, 90, 160],
        }
    }

    pub fn accent_color(self) -> [u8; 3] {
        match self {
            PaintId::Factory => [60, 64, 70],
            PaintId::Sunburst => [210, 50, 30],
            PaintId::Glacier => [255, 255, 255],
            PaintId::Toxic => [30, 30, 30],
            PaintId::Midnight => [90, 200, 255],
            PaintId::Candy => [255, 240, 120],
        }
    }

    /// `(body, accent)` colors to draw `spec` with this paint.
    pub fn resolve_colors(self, spec: &CarSpec) -> ([u8; 3], [u8; 3]) {
        match self {
            PaintId::Factory => (spec.base_color, spec.accent_color),
            other => (other.body_color(), other.accent_color()),
        }
    }

    pub fn is_starter(self) -> bool {
        self.price() == 0
    }
}

// ---------------------------------------------------------------------------
// Payouts
// ---------------------------------------------------------------------------

pub const PLACE_CREDITS: [u32; 4] = [260, 170, 110, 70];
pub const TRIAL_BASE_CREDITS: u32 = 120;
pub const DRIFT_POINTS_PER_CREDIT: u32 = 35;
pub const DRIFT_BONUS_CAP: u32 = 200;
pub const PB_BONUS_CREDITS: u32 = 90;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Payout {
    pub base: u32,
    pub drift: u32,
    pub pb: u32,
    pub total: u32,
}

impl Payout {
    fn new(base: u32, drift_points: u32, new_pb: bool) -> Self {
        let drift = (drift_points / DRIFT_POINTS_PER_CREDIT).min(DRIFT_BONUS_CAP);
        let pb = if new_pb { PB_BONUS_CREDITS } else { 0 };
        Self {
            base,
            drift,
            pb,
            total: base + drift + pb,
        }
    }
}

/// Credits for a finished race. `place` is 1..=4; anything outside that
/// range pays last-place credits.
pub fn race_payout(place: u8, drift_points: u32, new_pb: bool) -> Payout {
    let base = match place {
        1..=4 => PLACE_CREDITS[usize::from(place) - 1],
        _ => PLACE_CREDITS[3],
    };
    Payout::new(base, drift_points, new_pb)
}

/// Credits for a completed time-trial run.
pub fn trial_payout(drift_points: u32, new_pb: bool) -> Payout {
    Payout::new(TRIAL_BASE_CREDITS, drift_points, new_pb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cars::{CarId, car_spec};

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn upgrade_prices_and_maxing() {
        assert_eq!(upgrade_price(UpgradeKind::Engine, 0), Some(150));
        assert_eq!(upgrade_price(UpgradeKind::Engine, 1), Some(320));
        assert_eq!(upgrade_price(UpgradeKind::Tires, 0), Some(140));
        assert_eq!(upgrade_price(UpgradeKind::Tires, 1), Some(300));
        assert_eq!(upgrade_price(UpgradeKind::Exhaust, 0), Some(160));
        assert_eq!(upgrade_price(UpgradeKind::Exhaust, 1), Some(340));
        for kind in UpgradeKind::ALL {
            assert_eq!(upgrade_price(kind, 2), None);
            assert_eq!(upgrade_price(kind, 200), None);
        }
    }

    #[test]
    fn no_upgrades_is_identity() {
        let base = car_spec(CarId::Mako).tuning;
        assert_eq!(apply_upgrades(base, UpgradeLevels::default()), base);
    }

    #[test]
    fn full_upgrades_match_per_stage_math() {
        let base = CarTuning::default();
        let levels = UpgradeLevels {
            engine: 2,
            tires: 2,
            exhaust: 2,
        };
        let t = apply_upgrades(base, levels);
        assert!(close(t.top_speed_mul, 1.08));
        assert!(close(t.accel_mul, 1.10));
        assert!(close(t.grip_mul, 1.12));
        assert!(close(t.drift_control_mul, 1.12));
        assert!(close(t.nitro_duration, 2.8 * 1.28));
        assert!(close(t.nitro_top_bonus, 100.0));
        assert!(close(t.nitro_accel_mul, 0.65 * 1.24));
        assert!(close(t.nitro_refill_mul, 1.36));
    }

    #[test]
    fn upgrade_levels_are_clamped() {
        let base = CarTuning::default();
        let wild = UpgradeLevels {
            engine: 9,
            tires: 3,
            exhaust: 255,
        };
        let maxed = UpgradeLevels {
            engine: 2,
            tires: 2,
            exhaust: 2,
        };
        assert_eq!(apply_upgrades(base, wild), apply_upgrades(base, maxed));
        let mut l = UpgradeLevels::default();
        l.set(UpgradeKind::Tires, 7);
        assert_eq!(l.get(UpgradeKind::Tires), 2);
        assert_eq!(l.get(UpgradeKind::Engine), 0);
    }

    #[test]
    fn gadget_table() {
        let prices: Vec<u32> = GadgetId::ALL.iter().map(|g| g.price()).collect();
        assert_eq!(prices, [0, 0, 160, 220, 260, 340, 420]);
        let cds: Vec<f32> = GadgetId::ALL.iter().map(|g| g.cooldown()).collect();
        assert_eq!(cds, [11.0, 9.0, 13.0, 11.0, 15.0, 14.0, 16.0]);
        let act: Vec<f32> = GadgetId::ALL.iter().map(|g| g.active()).collect();
        assert_eq!(act, [0.45, 0.4, 0.45, 2.4, 2.8, 3.5, 5.0]);
        assert_eq!(GadgetId::StaticBurst.name(), "STATIC BURST");
        for g in GadgetId::ALL {
            assert_eq!(g.name(), g.name().to_uppercase());
            assert!(!g.blurb().is_empty());
        }
    }

    #[test]
    fn box_roll_excludes_last_and_twin() {
        for rng in 0..64u32 {
            for gadget in GadgetId::ALL {
                for last in [
                    None,
                    Some(BoxItem::Slick),
                    Some(BoxItem::Thumper),
                    Some(BoxItem::Jammer),
                ] {
                    let item = roll_box_item(rng, last, gadget);
                    assert_ne!(Some(item), last, "rng {rng} {gadget:?} {last:?}");
                    assert_ne!(item.twin_gadget(), Some(gadget), "rng {rng} {gadget:?}");
                }
            }
        }
        // Two exclusions leave exactly one option.
        for rng in 0..8 {
            assert_eq!(
                roll_box_item(rng, Some(BoxItem::Jammer), GadgetId::Slick),
                BoxItem::Thumper
            );
        }
    }

    #[test]
    fn box_roll_is_uniform_over_the_pool() {
        let mut counts = [0u32; 3];
        for rng in 0..3000u32 {
            let item = roll_box_item(rng, None, GadgetId::Phantom);
            counts[BoxItem::ALL.iter().position(|&b| b == item).unwrap()] += 1;
        }
        assert_eq!(counts, [1000, 1000, 1000]);
    }

    #[test]
    fn paints_table() {
        assert_eq!(PaintId::ALL[0].price(), 0);
        for p in &PaintId::ALL[1..] {
            assert!((60..=220).contains(&p.price()), "{p:?}");
        }
        let spec = car_spec(CarId::Bandit);
        assert_eq!(
            PaintId::Factory.resolve_colors(&spec),
            (spec.base_color, spec.accent_color)
        );
        assert_eq!(
            PaintId::Candy.resolve_colors(&spec),
            (PaintId::Candy.body_color(), PaintId::Candy.accent_color())
        );
    }

    #[test]
    fn race_payouts() {
        assert_eq!(
            race_payout(1, 0, false),
            Payout {
                base: 260,
                drift: 0,
                pb: 0,
                total: 260
            }
        );
        assert_eq!(race_payout(2, 350, false).total, 170 + 10);
        assert_eq!(
            race_payout(3, 34, true),
            Payout {
                base: 110,
                drift: 0,
                pb: 90,
                total: 200
            }
        );
        assert_eq!(
            race_payout(4, 1_000_000, true),
            Payout {
                base: 70,
                drift: 200,
                pb: 90,
                total: 360
            }
        );
        assert_eq!(race_payout(0, 0, false).base, 70);
        assert_eq!(race_payout(9, 0, false).base, 70);
    }

    #[test]
    fn trial_payouts() {
        assert_eq!(
            trial_payout(0, false),
            Payout {
                base: 120,
                drift: 0,
                pb: 0,
                total: 120
            }
        );
        assert_eq!(
            trial_payout(7000, true),
            Payout {
                base: 120,
                drift: 200,
                pb: 90,
                total: 410
            }
        );
        assert_eq!(trial_payout(70, false).drift, 2);
    }
}
