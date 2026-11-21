//! Title screen: game name, track and mode selection, entry into the garage or race.

use bevy::prelude::*;

use super::palette::UiPalette;
use super::{AppState, RaceMode};

/// The embedded pixel font (Press Start 2P, OFL-1.1, see `assets/fonts/OFL.txt`).
#[derive(Resource, Clone)]
pub struct GameFont(pub Handle<Font>);

impl GameFont {
    pub fn text(&self, size: f32) -> TextFont {
        TextFont {
            font: self.0.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        }
    }
}

#[derive(Component)]
struct TitleRoot;

pub struct TitlePlugin;

impl Plugin for TitlePlugin {
    fn build(&self, app: &mut App) {
        // Inserted at build time: the initial `OnEnter` runs before any startup commands apply.
        let bytes = include_bytes!("../../assets/fonts/PressStart2P-Regular.ttf").to_vec();
        let handle = app
            .world_mut()
            .resource_mut::<Assets<Font>>()
            .add(Font::from_bytes(bytes));
        app.insert_resource(GameFont(handle))
            .add_systems(OnEnter(AppState::Title), spawn_title)
            .add_systems(OnExit(AppState::Title), despawn_title)
            .add_systems(Update, title_input.run_if(in_state(AppState::Title)));
    }
}

fn spawn_title(mut commands: Commands, font: Res<GameFont>, pal: Res<UiPalette>) {
    commands.spawn((
        TitleRoot,
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(24),
            ..default()
        },
        children![
            (
                Text::new("HAIRPIN HAVOC"),
                font.text(40.0),
                TextColor(pal.accent),
            ),
            (
                Text::new("ENTER  race      T  time trial      ESC  quit"),
                font.text(12.0),
                TextColor(pal.foreground),
            ),
        ],
    ));
}

fn despawn_title(mut commands: Commands, q: Query<Entity, With<TitleRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn title_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<RaceMode>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::Enter) {
        *mode = RaceMode::Race;
        next.set(AppState::Race);
    } else if keys.just_pressed(KeyCode::KeyT) {
        *mode = RaceMode::TimeTrial;
        next.set(AppState::Race);
    } else if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}
