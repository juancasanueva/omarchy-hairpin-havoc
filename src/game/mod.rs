//! Bevy adapters: app wiring, screens, rendering, input and audio.

pub mod audio;
pub mod meshes;
pub mod palette;
pub mod pixel;
pub mod profile;
pub mod race;
pub mod results;
pub mod sprites;
pub mod theme;
pub mod title;
pub mod track_view;

use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

pub const APP_ID: &str = "hairpin-havoc";

/// Top-level screen flow.
#[derive(States, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Title,
    Garage,
    Race,
    Results,
}

/// Which mode the next race runs in.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RaceMode {
    #[default]
    Race,
    TimeTrial,
}

pub fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(ImagePlugin::default_nearest())
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Hairpin Havoc".into(),
                    // Wayland app_id, so Hyprland rules can target the game.
                    name: Some(APP_ID.into()),
                    resolution: WindowResolution::new(1280, 720),
                    present_mode: PresentMode::AutoVsync,
                    ..default()
                }),
                ..default()
            }),
    )
    .init_state::<AppState>()
    .init_resource::<RaceMode>()
    .add_plugins((
        palette::PalettePlugin,
        profile::ProfilePlugin,
        pixel::PixelPlugin,
        audio::GameAudioPlugin,
        title::TitlePlugin,
        race::RacePlugin,
        results::ResultsPlugin,
    ));
    app
}
