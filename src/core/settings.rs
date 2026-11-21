//! Player settings: audio levels, display, camera and driving key bindings.
//!
//! Pure data and rules. Keys are stored by name (`"KeyW"`, `"ArrowUp"`, ...,
//! the names of the physical key codes); the game layer maps each name in
//! [`KEY_NAMES`] to its input type.

use serde::{Deserialize, Serialize};

/// Volumes move in steps of this many percent.
pub const VOLUME_STEP: u8 = 10;

/// Keys the game always handles itself during a race (back, restart, camera,
/// mute, ghost, paste, finish): never bindable to a driving action.
pub const RESERVED_KEYS: [&str; 10] = [
    "Escape",
    "KeyR",
    "KeyV",
    "KeyM",
    "KeyG",
    "ControlLeft",
    "ControlRight",
    "Enter",
    "NumpadEnter",
    "Backspace",
];

/// Every key name a driving action can be bound to.
pub const KEY_NAMES: [&str; 76] = [
    "KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH", "KeyI", "KeyJ", "KeyK", "KeyL", "KeyM",
    "KeyN", "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT", "KeyU", "KeyV", "KeyW", "KeyX", "KeyY", "KeyZ",
    "Digit0", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9",
    "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Space", "ShiftLeft", "ShiftRight", "ControlLeft",
    "ControlRight", "AltLeft", "AltRight", "Tab", "CapsLock", "Enter", "NumpadEnter", "Escape", "Backspace",
    "Comma", "Period", "Slash", "Semicolon", "Quote", "BracketLeft", "BracketRight", "Minus", "Equal",
    "Backquote", "Backslash", "Numpad0", "Numpad1", "Numpad2", "Numpad3", "Numpad4", "Numpad5", "Numpad6",
    "Numpad7", "Numpad8", "Numpad9", "NumpadAdd", "NumpadSubtract",
];

/// Short on-screen label for a key name (`"KeyW"` → `"W"`, `"ShiftLeft"` → `"L-SHIFT"`).
pub fn key_label(name: &str) -> String {
    let fixed = match name {
        "ArrowUp" => "UP",
        "ArrowDown" => "DOWN",
        "ArrowLeft" => "LEFT",
        "ArrowRight" => "RIGHT",
        "ShiftLeft" => "L-SHIFT",
        "ShiftRight" => "R-SHIFT",
        "ControlLeft" => "L-CTRL",
        "ControlRight" => "R-CTRL",
        "AltLeft" => "L-ALT",
        "AltRight" => "R-ALT",
        "CapsLock" => "CAPS",
        "NumpadEnter" => "NUM ENTER",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Comma" => ",",
        "Period" => ".",
        "Slash" => "/",
        "Semicolon" => ";",
        "Quote" => "'",
        "Minus" => "-",
        "Equal" => "=",
        "Backquote" => "`",
        "Backslash" => "\\",
        "NumpadAdd" => "NUM +",
        "NumpadSubtract" => "NUM -",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_string();
    }
    let short = name
        .strip_prefix("Key")
        .or_else(|| name.strip_prefix("Digit"))
        .map(str::to_string)
        .or_else(|| name.strip_prefix("Numpad").map(|n| format!("NUM {n}")))
        .unwrap_or_else(|| name.to_string());
    short.to_uppercase()
}

/// A driving action the player can rebind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Throttle,
    Brake,
    SteerLeft,
    SteerRight,
    Drift,
    Nitro,
    UseItem,
}

impl Action {
    pub const ALL: [Action; 7] = [
        Action::Throttle,
        Action::Brake,
        Action::SteerLeft,
        Action::SteerRight,
        Action::Drift,
        Action::Nitro,
        Action::UseItem,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::Throttle => "THROTTLE",
            Action::Brake => "BRAKE / REVERSE",
            Action::SteerLeft => "STEER LEFT",
            Action::SteerRight => "STEER RIGHT",
            Action::Drift => "DRIFT",
            Action::Nitro => "NITRO",
            Action::UseItem => "USE ITEM",
        }
    }

    /// Default rebindable key.
    pub fn default_key(self) -> &'static str {
        match self {
            Action::Throttle => "KeyW",
            Action::Brake => "KeyS",
            Action::SteerLeft => "KeyA",
            Action::SteerRight => "KeyD",
            Action::Drift => "Space",
            Action::Nitro => "ShiftLeft",
            Action::UseItem => "KeyE",
        }
    }

    /// Fixed alternate key that always works alongside the rebindable one.
    pub fn alt_key(self) -> Option<&'static str> {
        match self {
            Action::Throttle => Some("ArrowUp"),
            Action::Brake => Some("ArrowDown"),
            Action::SteerLeft => Some("ArrowLeft"),
            Action::SteerRight => Some("ArrowRight"),
            Action::Drift => None,
            Action::Nitro => Some("ShiftRight"),
            Action::UseItem => Some("KeyQ"),
        }
    }
}

/// Why a key could not be bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindError {
    /// Not a key the game knows.
    Unknown,
    /// One of [`RESERVED_KEYS`].
    Reserved,
    /// The fixed alternate key of another action.
    AltOf(Action),
}

/// The rebindable key of every action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyBindings {
    pub throttle: String,
    pub brake: String,
    pub steer_left: String,
    pub steer_right: String,
    pub drift: String,
    pub nitro: String,
    pub use_item: String,
}

impl Default for KeyBindings {
    fn default() -> Self {
        let k = |a: Action| a.default_key().to_string();
        Self {
            throttle: k(Action::Throttle),
            brake: k(Action::Brake),
            steer_left: k(Action::SteerLeft),
            steer_right: k(Action::SteerRight),
            drift: k(Action::Drift),
            nitro: k(Action::Nitro),
            use_item: k(Action::UseItem),
        }
    }
}

impl KeyBindings {
    fn slot(&mut self, action: Action) -> &mut String {
        match action {
            Action::Throttle => &mut self.throttle,
            Action::Brake => &mut self.brake,
            Action::SteerLeft => &mut self.steer_left,
            Action::SteerRight => &mut self.steer_right,
            Action::Drift => &mut self.drift,
            Action::Nitro => &mut self.nitro,
            Action::UseItem => &mut self.use_item,
        }
    }

    /// The rebindable key of `action`.
    pub fn key(&self, action: Action) -> &str {
        match action {
            Action::Throttle => &self.throttle,
            Action::Brake => &self.brake,
            Action::SteerLeft => &self.steer_left,
            Action::SteerRight => &self.steer_right,
            Action::Drift => &self.drift,
            Action::Nitro => &self.nitro,
            Action::UseItem => &self.use_item,
        }
    }

    /// Every key that triggers `action`: its bound key, then its alternate.
    pub fn keys(&self, action: Action) -> Vec<&str> {
        let mut keys = vec![self.key(action)];
        keys.extend(action.alt_key().filter(|alt| *alt != self.key(action)));
        keys
    }

    /// Why `key` cannot drive `action`, if it cannot.
    pub fn check(action: Action, key: &str) -> Result<(), BindError> {
        if !KEY_NAMES.contains(&key) {
            return Err(BindError::Unknown);
        }
        if RESERVED_KEYS.contains(&key) {
            return Err(BindError::Reserved);
        }
        match Action::ALL.into_iter().find(|&o| o != action && o.alt_key() == Some(key)) {
            Some(other) => Err(BindError::AltOf(other)),
            None => Ok(()),
        }
    }

    /// Binds `key` to `action`. If another action already used `key`, the two
    /// actions swap keys, so every action stays bound.
    pub fn assign(&mut self, action: Action, key: &str) -> Result<(), BindError> {
        Self::check(action, key)?;
        let previous = self.key(action).to_string();
        if let Some(other) = Action::ALL.into_iter().find(|&o| o != action && self.key(o) == key) {
            *self.slot(other) = previous;
        }
        *self.slot(action) = key.to_string();
        Ok(())
    }

    /// Restores invalid or duplicated bindings to the defaults.
    pub fn sanitize(&mut self) {
        for action in Action::ALL {
            if Self::check(action, self.key(action)).is_err() {
                *self.slot(action) = action.default_key().to_string();
            }
        }
        let mut seen: Vec<&str> = Vec::new();
        let duplicated = Action::ALL.into_iter().any(|a| {
            let k = self.key(a);
            let dup = seen.contains(&k);
            seen.push(k);
            dup
        });
        if duplicated {
            *self = Self::default();
        }
    }
}

/// How large one canvas pixel is relative to the automatic size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelSize {
    Smaller,
    #[default]
    Auto,
    Larger,
}

impl PixelSize {
    pub const ALL: [PixelSize; 3] = [PixelSize::Smaller, PixelSize::Auto, PixelSize::Larger];

    pub fn label(self) -> &'static str {
        match self {
            PixelSize::Smaller => "SMALLER",
            PixelSize::Auto => "AUTO",
            PixelSize::Larger => "LARGER",
        }
    }

    /// Added to the automatic integer canvas scale.
    pub fn scale_offset(self) -> i32 {
        match self {
            PixelSize::Smaller => -1,
            PixelSize::Auto => 0,
            PixelSize::Larger => 1,
        }
    }

    /// The next size in `dir` (-1 or +1), stopping at both ends.
    pub fn stepped(self, dir: i32) -> Self {
        let i = Self::ALL.iter().position(|&p| p == self).unwrap_or(1) as i32;
        Self::ALL[(i + dir.signum()).clamp(0, 2) as usize]
    }
}

/// Everything on the settings screen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Percent, 0..=100 in [`VOLUME_STEP`] steps; scales engine and effects.
    pub master_volume: u8,
    pub engine_volume: u8,
    pub effects_volume: u8,
    pub fullscreen: bool,
    pub pixel_size: PixelSize,
    /// Start races with the wide camera.
    pub wide_camera: bool,
    pub keys: KeyBindings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            master_volume: 70,
            engine_volume: 100,
            effects_volume: 100,
            fullscreen: false,
            pixel_size: PixelSize::Auto,
            wide_camera: false,
            keys: KeyBindings::default(),
        }
    }
}

/// `volume` moved one step in `dir` (-1 or +1), snapped to the step grid and clamped to 0..=100.
pub fn step_volume(volume: u8, dir: i32) -> u8 {
    let snapped = (volume.min(100) as i32 + VOLUME_STEP as i32 / 2) / VOLUME_STEP as i32 * VOLUME_STEP as i32;
    (snapped + dir.signum() * VOLUME_STEP as i32).clamp(0, 100) as u8
}

impl Settings {
    /// Linear gains (engine, effects), each already multiplied by the master volume.
    pub fn gains(&self) -> (f32, f32) {
        let pct = |v: u8| v.min(100) as f32 / 100.0;
        let master = pct(self.master_volume);
        (master * pct(self.engine_volume), master * pct(self.effects_volume))
    }

    pub fn sanitize(&mut self) {
        for v in [&mut self.master_volume, &mut self.engine_volume, &mut self.effects_volume] {
            *v = (*v).min(100);
        }
        self.keys.sanitize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_distinct() {
        let k = KeyBindings::default();
        for a in Action::ALL {
            assert_eq!(KeyBindings::check(a, k.key(a)), Ok(()), "{a:?}");
            if let Some(alt) = a.alt_key() {
                assert!(KEY_NAMES.contains(&alt) && !RESERVED_KEYS.contains(&alt));
            }
        }
        let mut sanitized = k.clone();
        sanitized.sanitize();
        assert_eq!(sanitized, k);
        assert!(RESERVED_KEYS.iter().all(|r| KEY_NAMES.contains(r)));
    }

    #[test]
    fn volume_steps_snap_and_clamp() {
        assert_eq!(step_volume(70, 1), 80);
        assert_eq!(step_volume(70, -1), 60);
        assert_eq!(step_volume(100, 1), 100);
        assert_eq!(step_volume(0, -1), 0);
        assert_eq!(step_volume(73, 1), 80);
        assert_eq!(step_volume(250, -1), 90);
    }

    #[test]
    fn gains_multiply_by_master() {
        let s = Settings { master_volume: 50, engine_volume: 40, effects_volume: 100, ..Settings::default() };
        let (engine, effects) = s.gains();
        assert!((engine - 0.2).abs() < 1e-6);
        assert!((effects - 0.5).abs() < 1e-6);
        // Matches the old fixed 0.7 volume out of the box.
        assert_eq!(Settings::default().gains(), (0.7, 0.7));
    }

    #[test]
    fn assigning_a_used_key_swaps_the_two_actions() {
        let mut k = KeyBindings::default();
        assert_eq!(k.assign(Action::Throttle, "KeyS"), Ok(()));
        assert_eq!(k.key(Action::Throttle), "KeyS");
        assert_eq!(k.key(Action::Brake), "KeyW");
        assert_eq!(k.assign(Action::Nitro, "KeyN"), Ok(()));
        assert_eq!(k.keys(Action::Nitro), ["KeyN", "ShiftRight"]);
    }

    #[test]
    fn reserved_unknown_and_alt_keys_are_refused() {
        let mut k = KeyBindings::default();
        assert_eq!(k.assign(Action::Drift, "Escape"), Err(BindError::Reserved));
        assert_eq!(k.assign(Action::Drift, "KeyR"), Err(BindError::Reserved));
        assert_eq!(k.assign(Action::Drift, "Hyper"), Err(BindError::Unknown));
        assert_eq!(k.assign(Action::Throttle, "ArrowDown"), Err(BindError::AltOf(Action::Brake)));
        // An action may take its own alternate key.
        assert_eq!(k.assign(Action::Throttle, "ArrowUp"), Ok(()));
        assert_eq!(k.keys(Action::Throttle), ["ArrowUp"]);
        assert_eq!(k, KeyBindings { throttle: "ArrowUp".into(), ..KeyBindings::default() });
    }

    #[test]
    fn sanitize_repairs_bad_bindings() {
        let mut k = KeyBindings { drift: "Escape".into(), nitro: "Nope".into(), ..KeyBindings::default() };
        k.sanitize();
        assert_eq!(k, KeyBindings::default());
        let mut dup = KeyBindings { brake: "KeyW".into(), ..KeyBindings::default() };
        dup.sanitize();
        assert_eq!(dup, KeyBindings::default());
    }

    #[test]
    fn pixel_size_steps_stop_at_the_ends() {
        assert_eq!(PixelSize::Auto.stepped(1), PixelSize::Larger);
        assert_eq!(PixelSize::Larger.stepped(1), PixelSize::Larger);
        assert_eq!(PixelSize::Auto.stepped(-1), PixelSize::Smaller);
        assert_eq!(PixelSize::Smaller.stepped(-1), PixelSize::Smaller);
        assert_eq!(PixelSize::Smaller.scale_offset(), -1);
    }

    #[test]
    fn key_labels_are_short() {
        assert_eq!(key_label("KeyW"), "W");
        assert_eq!(key_label("Digit7"), "7");
        assert_eq!(key_label("ArrowUp"), "UP");
        assert_eq!(key_label("ShiftLeft"), "L-SHIFT");
        assert_eq!(key_label("Numpad4"), "NUM 4");
        assert_eq!(key_label("Space"), "SPACE");
    }

    #[test]
    fn toml_roundtrip_and_partial_tables() {
        let mut s = Settings { master_volume: 40, fullscreen: true, pixel_size: PixelSize::Larger, ..Settings::default() };
        s.keys.assign(Action::Drift, "KeyX").unwrap();
        let text = toml::to_string(&s).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), s);
        let partial: Settings = toml::from_str("master_volume = 20\n[keys]\nnitro = \"KeyN\"\n").unwrap();
        assert_eq!(partial.master_volume, 20);
        assert_eq!(partial.keys.nitro, "KeyN");
        assert_eq!(partial.keys.throttle, "KeyW");
        assert_eq!(partial.engine_volume, 100);
    }
}
