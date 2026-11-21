//! Hairpin Havoc: a top-down pixel-art arcade racer for Omarchy.
//!
//! `core` holds pure game rules (no ECS systems, unit-tested).
//! `game` holds the Bevy plugins that adapt those rules to rendering, input and audio.

pub mod core;
pub mod game;

pub fn run() {
    game::build_app().run();
}
