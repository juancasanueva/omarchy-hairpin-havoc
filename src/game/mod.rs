//! Bevy adapters: app wiring, screens, rendering, input and audio.

pub mod about;
pub mod audio;
pub mod controls;
pub mod flame;
pub mod fx;
pub mod garage;
pub mod hud;
pub mod items;
pub mod meshes;
pub mod palette;
pub mod pixel;
pub mod profile;
pub mod props;
pub mod race;
pub mod results;
pub mod settings;
pub mod stack;
pub mod theme;
pub mod title;
pub mod track_select;
pub mod track_view;
pub mod trial;

use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

pub const APP_ID: &str = "hairpin-havoc";

/// Top-level screen flow.
#[derive(States, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum AppState {
    /// Main menu.
    #[default]
    Title,
    TrackSelect,
    Garage,
    Race,
    Results,
    About,
    Settings,
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
        track_select::TrackSelectPlugin,
        race::RacePlugin,
        stack::StackPlugin,
        results::ResultsPlugin,
    ))
    .add_plugins((
        hud::HudPlugin,
        fx::FxPlugin,
        items::ItemsPlugin,
        garage::GaragePlugin,
        trial::TrialPlugin,
        about::AboutPlugin,
        settings::SettingsPlugin,
    ));
    app
}
