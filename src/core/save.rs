//! Player progress persisted as TOML, plus personal-best ghost files.
//!
//! All file helpers take an explicit directory so they can be tested in
//! isolation; `save_dir` resolves the real location.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use super::car::CarTuning;
use super::cars::{CarId, car_spec};
use super::economy::{
    GadgetId, PaintId, UpgradeKind, UpgradeLevels, apply_upgrades, upgrade_price,
};
use super::ghost::{Ghost, MAX_LAP_TIME, decode_code, encode_code};
use super::tracks::TrackId;

pub const APP_DIR_NAME: &str = "hairpin-havoc";
pub const SAVE_FILE_NAME: &str = "save.toml";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveData {
    pub credits: u32,
    #[serde(deserialize_with = "lenient_vec")]
    pub owned_cars: Vec<CarId>,
    #[serde(deserialize_with = "lenient_value")]
    pub selected_car: CarId,
    /// Index into `TrackId::ALL`.
    pub selected_track: usize,
    /// Installed upgrades keyed by `CarId::key`.
    #[serde(deserialize_with = "lenient_map")]
    pub upgrades: HashMap<String, UpgradeLevels>,
    /// Chosen paint keyed by `CarId::key`; missing means `PaintId::Factory`.
    #[serde(deserialize_with = "lenient_map")]
    pub paint: HashMap<String, PaintId>,
    #[serde(deserialize_with = "lenient_vec")]
    pub owned_paints: Vec<PaintId>,
    #[serde(deserialize_with = "lenient_vec")]
    pub owned_gadgets: Vec<GadgetId>,
    #[serde(deserialize_with = "lenient_value")]
    pub equipped_gadget: GadgetId,
    pub muted: bool,
    /// Best lap seconds keyed by `TrackId::key`.
    #[serde(deserialize_with = "lenient_map")]
    pub best_laps: HashMap<String, f32>,
}

impl Default for SaveData {
    fn default() -> Self {
        Self {
            credits: 0,
            owned_cars: CarId::ALL
                .into_iter()
                .filter(|&c| car_spec(c).is_starter())
                .collect(),
            selected_car: CarId::Pebble,
            selected_track: 0,
            upgrades: HashMap::new(),
            paint: HashMap::new(),
            owned_paints: PaintId::ALL
                .into_iter()
                .filter(|p| p.is_starter())
                .collect(),
            owned_gadgets: GadgetId::ALL
                .into_iter()
                .filter(|g| g.is_starter())
                .collect(),
            equipped_gadget: GadgetId::Thumper,
            muted: false,
            best_laps: HashMap::new(),
        }
    }
}

// Lenient deserializers: one unknown entry (e.g. from a newer version) is
// dropped instead of discarding the whole save.

fn lenient_vec<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let raw = Vec::<toml::Value>::deserialize(d)?;
    Ok(raw.into_iter().filter_map(|v| v.try_into().ok()).collect())
}

fn lenient_value<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let raw = toml::Value::deserialize(d)?;
    Ok(raw.try_into().unwrap_or_default())
}

fn lenient_map<'de, D, T>(d: D) -> Result<HashMap<String, T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let raw = HashMap::<String, toml::Value>::deserialize(d)?;
    Ok(raw
        .into_iter()
        .filter_map(|(k, v)| v.try_into().ok().map(|v| (k, v)))
        .collect())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PurchaseError {
    NotEnoughCredits,
    AlreadyOwned,
    Maxed,
    NotOwned,
}

impl fmt::Display for PurchaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PurchaseError::NotEnoughCredits => "NOT ENOUGH CREDITS",
            PurchaseError::AlreadyOwned => "ALREADY OWNED",
            PurchaseError::Maxed => "FULLY UPGRADED",
            PurchaseError::NotOwned => "NOT OWNED",
        })
    }
}

impl std::error::Error for PurchaseError {}

/// Keeps catalog order, drops duplicates and anything not owned, and always
/// includes starter items.
fn normalize<T: Copy + PartialEq>(all: &[T], owned: &[T], starter: impl Fn(T) -> bool) -> Vec<T> {
    all.iter()
        .copied()
        .filter(|&x| starter(x) || owned.contains(&x))
        .collect()
}

impl SaveData {
    /// Repairs any inconsistency: restores starter items, drops unknown and
    /// duplicate entries, makes selections valid, and clamps upgrade levels.
    pub fn sanitize(&mut self) {
        self.owned_cars = normalize(&CarId::ALL, &self.owned_cars, |c| car_spec(c).is_starter());
        self.owned_paints = normalize(&PaintId::ALL, &self.owned_paints, PaintId::is_starter);
        self.owned_gadgets = normalize(&GadgetId::ALL, &self.owned_gadgets, GadgetId::is_starter);

        if !self.owned_cars.contains(&self.selected_car) {
            self.selected_car = CarId::Pebble;
        }
        if self.selected_track >= TrackId::ALL.len() {
            self.selected_track = 0;
        }
        if !self.owned_gadgets.contains(&self.equipped_gadget) {
            self.equipped_gadget = GadgetId::Thumper;
        }

        self.upgrades.retain(|k, _| CarId::from_key(k).is_some());
        for levels in self.upgrades.values_mut() {
            *levels = levels.clamped();
        }
        self.upgrades.retain(|_, l| *l != UpgradeLevels::default());

        let owned_paints = &self.owned_paints;
        self.paint.retain(|k, p| {
            CarId::from_key(k).is_some() && owned_paints.contains(p) && *p != PaintId::Factory
        });

        self.best_laps.retain(|k, t| {
            TrackId::ALL.iter().any(|tr| tr.key() == k)
                && t.is_finite()
                && *t > 0.0
                && *t <= MAX_LAP_TIME
        });
    }

    pub fn owns_car(&self, car: CarId) -> bool {
        self.owned_cars.contains(&car)
    }

    pub fn selected_track_id(&self) -> TrackId {
        TrackId::ALL
            .get(self.selected_track)
            .copied()
            .unwrap_or(TrackId::ALL[0])
    }

    pub fn upgrade_levels(&self, car: CarId) -> UpgradeLevels {
        self.upgrades
            .get(car.key())
            .copied()
            .unwrap_or_default()
            .clamped()
    }

    pub fn paint_for(&self, car: CarId) -> PaintId {
        self.paint.get(car.key()).copied().unwrap_or_default()
    }

    /// Base tuning of `car` with its installed upgrades applied.
    pub fn tuning_for(&self, car: CarId) -> CarTuning {
        apply_upgrades(car_spec(car).tuning, self.upgrade_levels(car))
    }

    pub fn best_lap(&self, track: TrackId) -> Option<f32> {
        self.best_laps.get(track.key()).copied()
    }

    /// Records a lap time; returns `true` when it is a new personal best.
    pub fn submit_lap(&mut self, track: TrackId, lap_time: f32) -> bool {
        if !(lap_time.is_finite() && lap_time > 0.0 && lap_time <= MAX_LAP_TIME) {
            return false;
        }
        let is_pb = self.best_lap(track).is_none_or(|best| lap_time < best);
        if is_pb {
            self.best_laps.insert(track.key().to_owned(), lap_time);
        }
        is_pb
    }

    fn spend(&mut self, price: u32) -> Result<u32, PurchaseError> {
        self.credits = self
            .credits
            .checked_sub(price)
            .ok_or(PurchaseError::NotEnoughCredits)?;
        Ok(price)
    }

    pub fn buy_car(&mut self, car: CarId) -> Result<u32, PurchaseError> {
        if self.owns_car(car) {
            return Err(PurchaseError::AlreadyOwned);
        }
        let price = self.spend(car_spec(car).price)?;
        self.owned_cars.push(car);
        self.owned_cars = normalize(&CarId::ALL, &self.owned_cars, |c| car_spec(c).is_starter());
        Ok(price)
    }

    /// Buys the next stage of `kind` for an owned car.
    pub fn buy_upgrade(&mut self, car: CarId, kind: UpgradeKind) -> Result<u32, PurchaseError> {
        if !self.owns_car(car) {
            return Err(PurchaseError::NotOwned);
        }
        let mut levels = self.upgrade_levels(car);
        let current = levels.get(kind);
        let price = upgrade_price(kind, current).ok_or(PurchaseError::Maxed)?;
        self.spend(price)?;
        levels.set(kind, current + 1);
        self.upgrades.insert(car.key().to_owned(), levels);
        Ok(price)
    }

    pub fn buy_paint(&mut self, paint: PaintId) -> Result<u32, PurchaseError> {
        if self.owned_paints.contains(&paint) {
            return Err(PurchaseError::AlreadyOwned);
        }
        let price = self.spend(paint.price())?;
        self.owned_paints.push(paint);
        self.owned_paints = normalize(&PaintId::ALL, &self.owned_paints, PaintId::is_starter);
        Ok(price)
    }

    pub fn buy_gadget(&mut self, gadget: GadgetId) -> Result<u32, PurchaseError> {
        if self.owned_gadgets.contains(&gadget) {
            return Err(PurchaseError::AlreadyOwned);
        }
        let price = self.spend(gadget.price())?;
        self.owned_gadgets.push(gadget);
        self.owned_gadgets = normalize(&GadgetId::ALL, &self.owned_gadgets, GadgetId::is_starter);
        Ok(price)
    }
}

// ---------------------------------------------------------------------------
// File IO
// ---------------------------------------------------------------------------

/// `$XDG_DATA_HOME/hairpin-havoc`, falling back to `~/.local/share/hairpin-havoc`.
pub fn save_dir() -> Option<PathBuf> {
    let from_env = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let base =
        from_env("XDG_DATA_HOME").or_else(|| from_env("HOME").map(|h| h.join(".local/share")))?;
    Some(base.join(APP_DIR_NAME))
}

/// Loads the save from `dir`; a missing or corrupt file yields the default.
/// The result is always sanitized.
pub fn load(dir: &Path) -> SaveData {
    let mut data = fs::read_to_string(dir.join(SAVE_FILE_NAME))
        .ok()
        .and_then(|text| toml::from_str::<SaveData>(&text).ok())
        .unwrap_or_default();
    data.sanitize();
    data
}

/// Writes `contents` to `dir/name` atomically via a temp file and rename.
fn write_atomic(dir: &Path, name: &str, contents: &str) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{name}.tmp"));
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, dir.join(name))
}

pub fn store(dir: &Path, data: &SaveData) -> io::Result<()> {
    let text = toml::to_string(data).map_err(io::Error::other)?;
    write_atomic(dir, SAVE_FILE_NAME, &text)
}

fn ghost_file_name(track: TrackId) -> String {
    format!("pb-{}.txt", track.key())
}

/// Loads the personal-best ghost for `track`, if present and valid.
pub fn load_ghost(dir: &Path, track: TrackId) -> Option<Ghost> {
    let text = fs::read_to_string(dir.join(ghost_file_name(track))).ok()?;
    decode_code(&text).ok().filter(|g| g.track == track)
}

/// Saves `ghost` as the personal best of its track.
pub fn store_ghost(dir: &Path, ghost: &Ghost) -> io::Result<()> {
    let mut code = encode_code(ghost);
    code.push('\n');
    write_atomic(dir, &ghost_file_name(ghost.track), &code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ghost::GhostRecorder;
    use bevy::math::Vec2;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("hairpin-havoc-test-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn default_has_starters_and_is_sane() {
        let d = SaveData::default();
        assert_eq!(d.credits, 0);
        assert_eq!(d.owned_cars, [CarId::Pebble, CarId::Sparrow]);
        assert_eq!(d.owned_gadgets, [GadgetId::Thumper, GadgetId::Slick]);
        assert_eq!(d.owned_paints, [PaintId::ALL[0]]);
        assert_eq!(d.selected_car, CarId::Pebble);
        let mut s = d.clone();
        s.sanitize();
        assert_eq!(s, d);
    }

    #[test]
    fn sanitize_repairs_everything() {
        let mut d = SaveData {
            owned_cars: vec![CarId::Baron, CarId::Baron],
            selected_car: CarId::Mako,
            selected_track: 99,
            owned_gadgets: vec![GadgetId::Phantom, GadgetId::Phantom],
            equipped_gadget: GadgetId::DriftSurge,
            owned_paints: vec![PaintId::Candy],
            ..SaveData::default()
        };
        d.upgrades.insert(
            "baron".into(),
            UpgradeLevels {
                engine: 9,
                tires: 1,
                exhaust: 0,
            },
        );
        d.upgrades.insert(
            "hovercraft".into(),
            UpgradeLevels {
                engine: 1,
                tires: 1,
                exhaust: 1,
            },
        );
        d.paint.insert("baron".into(), PaintId::Candy);
        d.paint.insert("pebble".into(), PaintId::Toxic);
        d.best_laps.insert("orchard-ring".into(), 61.5);
        d.best_laps.insert("moon-base".into(), 50.0);
        d.best_laps.insert("dune-sprint".into(), f32::NAN);
        d.sanitize();

        assert_eq!(d.owned_cars, [CarId::Pebble, CarId::Sparrow, CarId::Baron]);
        assert_eq!(d.selected_car, CarId::Pebble);
        assert_eq!(d.selected_track, 0);
        assert_eq!(
            d.owned_gadgets,
            [GadgetId::Thumper, GadgetId::Slick, GadgetId::Phantom]
        );
        assert_eq!(d.equipped_gadget, GadgetId::Thumper);
        assert_eq!(d.owned_paints, [PaintId::Factory, PaintId::Candy]);
        assert_eq!(d.upgrades.len(), 1);
        assert_eq!(
            d.upgrade_levels(CarId::Baron),
            UpgradeLevels {
                engine: 2,
                tires: 1,
                exhaust: 0
            }
        );
        assert_eq!(d.paint_for(CarId::Baron), PaintId::Candy);
        assert_eq!(d.paint_for(CarId::Pebble), PaintId::Factory);
        assert_eq!(d.best_laps.len(), 1);
    }

    #[test]
    fn toml_round_trip_and_lenient_parsing() {
        let mut d = SaveData {
            credits: 1234,
            muted: true,
            selected_track: 2,
            ..SaveData::default()
        };
        d.owned_cars.push(CarId::Tempest);
        d.selected_car = CarId::Tempest;
        d.upgrades.insert(
            "tempest".into(),
            UpgradeLevels {
                engine: 1,
                tires: 2,
                exhaust: 0,
            },
        );
        d.best_laps.insert("neon-docks".into(), 48.25);
        let text = toml::to_string(&d).unwrap();
        let back: SaveData = toml::from_str(&text).unwrap();
        assert_eq!(back, d);

        let text = r#"
            credits = 50
            owned_cars = ["Pebble", "Spaceship", "Mako"]
            selected_car = "Spaceship"
            owned_gadgets = ["Thumper", "Laser"]
            equipped_gadget = "Laser"

            [upgrades.mako]
            engine = 1

            [upgrades.pebble]
            engine = -3
        "#;
        let mut parsed: SaveData = toml::from_str(text).unwrap();
        parsed.sanitize();
        assert_eq!(parsed.credits, 50);
        assert_eq!(
            parsed.owned_cars,
            [CarId::Pebble, CarId::Sparrow, CarId::Mako]
        );
        assert_eq!(parsed.selected_car, CarId::Pebble);
        assert_eq!(parsed.owned_gadgets, [GadgetId::Thumper, GadgetId::Slick]);
        assert_eq!(parsed.upgrade_levels(CarId::Mako).engine, 1);
        assert_eq!(
            parsed.upgrade_levels(CarId::Pebble),
            UpgradeLevels::default()
        );
    }

    #[test]
    fn file_store_and_load() {
        let tmp = TempDir::new("save");
        assert_eq!(load(&tmp.0), SaveData::default());

        let mut d = SaveData {
            credits: 777,
            ..SaveData::default()
        };
        d.buy_gadget(GadgetId::StaticBurst).unwrap();
        store(&tmp.0, &d).unwrap();
        assert!(!tmp.0.join("save.toml.tmp").exists());
        assert_eq!(load(&tmp.0), d);

        fs::write(tmp.0.join(SAVE_FILE_NAME), "credits = [[[ nope").unwrap();
        assert_eq!(load(&tmp.0), SaveData::default());
    }

    #[test]
    fn ghost_store_and_load() {
        let tmp = TempDir::new("ghost");
        assert!(load_ghost(&tmp.0, TrackId::DuneSprint).is_none());
        let mut rec = GhostRecorder::new();
        for i in 0..90 {
            let t = i as f32 / 30.0;
            rec.record(t, Vec2::new(t * 100.0, -t * 50.0), 0.5, 240.0, false);
        }
        let ghost = rec.finish(
            3.0,
            [1.0, 2.0, 3.0],
            CarId::Bandit,
            TrackId::DuneSprint,
            "Me".into(),
        );
        store_ghost(&tmp.0, &ghost).unwrap();
        assert!(tmp.0.join("pb-dune-sprint.txt").exists());
        let back = load_ghost(&tmp.0, TrackId::DuneSprint).unwrap();
        assert_eq!(back.car, CarId::Bandit);
        assert_eq!(back.label, "Me");
        assert!(load_ghost(&tmp.0, TrackId::NeonDocks).is_none());

        fs::write(tmp.0.join("pb-dune-sprint.txt"), "garbage").unwrap();
        assert!(load_ghost(&tmp.0, TrackId::DuneSprint).is_none());
    }

    #[test]
    fn purchases() {
        let mut d = SaveData {
            credits: 1000,
            ..SaveData::default()
        };
        assert_eq!(d.buy_car(CarId::Pebble), Err(PurchaseError::AlreadyOwned));
        assert_eq!(
            d.buy_car(CarId::Baron),
            Err(PurchaseError::NotEnoughCredits)
        );
        assert_eq!(d.credits, 1000);
        assert_eq!(d.buy_car(CarId::Bandit), Ok(450));
        assert_eq!(d.credits, 550);
        assert_eq!(d.owned_cars, [CarId::Pebble, CarId::Sparrow, CarId::Bandit]);

        assert_eq!(
            d.buy_upgrade(CarId::Mako, UpgradeKind::Engine),
            Err(PurchaseError::NotOwned)
        );
        assert_eq!(d.buy_upgrade(CarId::Bandit, UpgradeKind::Engine), Ok(150));
        assert_eq!(d.buy_upgrade(CarId::Bandit, UpgradeKind::Engine), Ok(320));
        assert_eq!(
            d.buy_upgrade(CarId::Bandit, UpgradeKind::Engine),
            Err(PurchaseError::Maxed)
        );
        assert_eq!(d.credits, 80);
        assert_eq!(d.upgrade_levels(CarId::Bandit).engine, 2);
        let base = car_spec(CarId::Bandit).tuning;
        assert!(
            (d.tuning_for(CarId::Bandit).top_speed_mul - base.top_speed_mul * 1.08).abs() < 1e-5
        );
        assert_eq!(d.tuning_for(CarId::Pebble), car_spec(CarId::Pebble).tuning);

        assert_eq!(
            d.buy_paint(PaintId::Factory),
            Err(PurchaseError::AlreadyOwned)
        );
        assert_eq!(d.buy_paint(PaintId::Sunburst), Ok(60));
        assert_eq!(
            d.buy_paint(PaintId::Candy),
            Err(PurchaseError::NotEnoughCredits)
        );
        assert_eq!(
            d.buy_gadget(GadgetId::Slick),
            Err(PurchaseError::AlreadyOwned)
        );
        assert_eq!(
            d.buy_gadget(GadgetId::StaticBurst),
            Err(PurchaseError::NotEnoughCredits)
        );
        assert_eq!(d.credits, 20);
    }

    #[test]
    fn personal_bests() {
        let mut d = SaveData::default();
        assert!(d.submit_lap(TrackId::GlacierPass, 70.0));
        assert!(!d.submit_lap(TrackId::GlacierPass, 71.0));
        assert!(d.submit_lap(TrackId::GlacierPass, 65.5));
        assert!(!d.submit_lap(TrackId::GlacierPass, f32::NAN));
        assert_eq!(d.best_lap(TrackId::GlacierPass), Some(65.5));
        assert_eq!(d.best_lap(TrackId::OrchardRing), None);
    }
}
