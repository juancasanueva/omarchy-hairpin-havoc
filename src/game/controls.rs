//! Driving controls: maps the saved key names (`core::settings`) to Bevy key
//! codes and answers "is this action held / just pressed" for the race systems.

use bevy::prelude::*;

use crate::core::settings::{Action, KeyBindings, key_label};

macro_rules! key_table {
    ($($name:ident),* $(,)?) => {
        /// The key code for a saved key name.
        pub fn key_code(name: &str) -> Option<KeyCode> {
            match name {
                $(stringify!($name) => Some(KeyCode::$name),)*
                _ => None,
            }
        }

        /// The saved name of a key code, if it is a key the game can bind.
        pub fn key_name(code: KeyCode) -> Option<&'static str> {
            match code {
                $(KeyCode::$name => Some(stringify!($name)),)*
                _ => None,
            }
        }
    };
}

key_table!(
    KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR,
    KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ, Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7,
    Digit8, Digit9, ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Space, ShiftLeft, ShiftRight, ControlLeft,
    ControlRight, AltLeft, AltRight, Tab, CapsLock, Enter, NumpadEnter, Escape, Backspace, Comma, Period, Slash,
    Semicolon, Quote, BracketLeft, BracketRight, Minus, Equal, Backquote, Backslash, Numpad0, Numpad1, Numpad2,
    Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9, NumpadAdd, NumpadSubtract,
);

/// Key codes of every driving action, rebuilt whenever the settings change.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct Controls {
    keys: [Vec<KeyCode>; Action::ALL.len()],
    labels: [String; Action::ALL.len()],
}

impl Default for Controls {
    fn default() -> Self {
        Self::from_bindings(&KeyBindings::default())
    }
}

fn index(action: Action) -> usize {
    Action::ALL.iter().position(|&a| a == action).unwrap_or(0)
}

impl Controls {
    pub fn from_bindings(bindings: &KeyBindings) -> Self {
        let keys = Action::ALL.map(|a| bindings.keys(a).into_iter().filter_map(key_code).collect());
        let labels = Action::ALL.map(|a| key_label(bindings.key(a)));
        Self { keys, labels }
    }

    pub fn pressed(&self, input: &ButtonInput<KeyCode>, action: Action) -> bool {
        input.any_pressed(self.keys[index(action)].iter().copied())
    }

    pub fn just_pressed(&self, input: &ButtonInput<KeyCode>, action: Action) -> bool {
        input.any_just_pressed(self.keys[index(action)].iter().copied())
    }

    /// On-screen label of the action's rebindable key.
    pub fn label(&self, action: Action) -> &str {
        &self.labels[index(action)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::settings::KEY_NAMES;

    #[test]
    fn every_saved_key_name_maps_both_ways() {
        for name in KEY_NAMES {
            let code = key_code(name).unwrap_or_else(|| panic!("{name} has no key code"));
            assert_eq!(key_name(code), Some(name));
        }
        assert_eq!(key_code("Nope"), None);
        assert_eq!(key_name(KeyCode::F13), None);
    }

    #[test]
    fn controls_follow_the_bindings() {
        let mut b = KeyBindings::default();
        b.assign(Action::Throttle, "KeyI").unwrap();
        let c = Controls::from_bindings(&b);
        let mut input = ButtonInput::<KeyCode>::default();
        input.press(KeyCode::KeyI);
        assert!(c.pressed(&input, Action::Throttle));
        assert!(c.just_pressed(&input, Action::Throttle));
        assert!(!c.pressed(&input, Action::Brake));
        // The alternate key keeps working, the old key does not.
        input.press(KeyCode::ArrowUp);
        input.release(KeyCode::KeyI);
        assert!(c.pressed(&input, Action::Throttle));
        let mut w = ButtonInput::<KeyCode>::default();
        w.press(KeyCode::KeyW);
        assert!(!c.pressed(&w, Action::Throttle));
        assert_eq!(c.label(Action::Throttle), "I");
        assert_eq!(c.label(Action::Nitro), "L-SHIFT");
    }
}
