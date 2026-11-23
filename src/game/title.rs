//! Title screen: the game name and the main menu.

use bevy::prelude::*;

use super::audio::{PlaySfx, Sfx};
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

const ROW_WIDTH: f32 = 400.0;

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
            .init_resource::<MainMenu>()
            .add_systems(OnEnter(AppState::Title), spawn_title)
            .add_systems(OnExit(AppState::Title), despawn_title)
            .add_systems(
                Update,
                (title_input, refresh_title)
                    .chain()
                    .run_if(in_state(AppState::Title)),
            )
            .add_systems(Startup, dev_autostart);
    }
}

/// Development aid: `HAIRPIN_AUTOSTART=race|trial|tracks|garage|scores|about|settings` skips the main menu.
fn dev_autostart(mut mode: ResMut<RaceMode>, mut next: ResMut<NextState<AppState>>) {
    match std::env::var("HAIRPIN_AUTOSTART").as_deref() {
        Ok("tracks") => next.set(AppState::TrackSelect),
        Ok("garage") => next.set(AppState::Garage),
        Ok("scores") => next.set(AppState::HighScores),
        Ok("about") => next.set(AppState::About),
        Ok("settings") => next.set(AppState::Settings),
        Ok("race") => {
            *mode = RaceMode::Race;
            next.set(AppState::Race);
        }
        Ok("trial") => {
            *mode = RaceMode::TimeTrial;
            next.set(AppState::Race);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Menu model
// ---------------------------------------------------------------------------

/// Moves `index` by `delta` within `0..len`, wrapping around both ends.
pub fn wrap_index(index: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (index as i64 + delta as i64).rem_euclid(len as i64) as usize
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuItem {
    Play,
    Garage,
    HighScores,
    Settings,
    About,
    Quit,
}

impl MenuItem {
    const ALL: [MenuItem; 6] = [
        MenuItem::Play,
        MenuItem::Garage,
        MenuItem::HighScores,
        MenuItem::Settings,
        MenuItem::About,
        MenuItem::Quit,
    ];

    fn label(self) -> &'static str {
        match self {
            MenuItem::Play => "PLAY",
            MenuItem::Garage => "GARAGE",
            MenuItem::HighScores => "HIGH SCORES",
            MenuItem::Settings => "SETTINGS",
            MenuItem::About => "ABOUT",
            MenuItem::Quit => "QUIT",
        }
    }

    /// Shortcut key. Must not clash with the navigation keys (arrows, W, S, Enter).
    fn key(self) -> KeyCode {
        match self {
            MenuItem::Play => KeyCode::KeyP,
            MenuItem::Garage => KeyCode::KeyG,
            MenuItem::HighScores => KeyCode::KeyH,
            MenuItem::Settings => KeyCode::KeyO,
            MenuItem::About => KeyCode::KeyI,
            MenuItem::Quit => KeyCode::Escape,
        }
    }

    /// The shortcut as shown next to the label.
    fn key_hint(self) -> &'static str {
        match self {
            MenuItem::Play => "[P]",
            MenuItem::Garage => "[G]",
            MenuItem::HighScores => "[H]",
            MenuItem::Settings => "[O]",
            MenuItem::About => "[I]",
            MenuItem::Quit => "[ESC]",
        }
    }

    /// Screen this entry opens; `None` quits the game.
    fn target(self) -> Option<AppState> {
        match self {
            MenuItem::Play => Some(AppState::TrackSelect),
            MenuItem::Garage => Some(AppState::Garage),
            MenuItem::HighScores => Some(AppState::HighScores),
            MenuItem::Settings => Some(AppState::Settings),
            MenuItem::About => Some(AppState::About),
            MenuItem::Quit => None,
        }
    }
}

/// Highlighted entry; kept across visits so returning lands on the last choice.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
struct MainMenu {
    row: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuAction {
    Move(i32),
    Activate(usize),
}

/// The action for this frame's key presses, given the highlighted row.
fn menu_action(row: usize, keys: &ButtonInput<KeyCode>) -> Option<MenuAction> {
    if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
        Some(MenuAction::Move(-1))
    } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
        Some(MenuAction::Move(1))
    } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        Some(MenuAction::Activate(row))
    } else {
        MenuItem::ALL
            .iter()
            .position(|item| keys.just_pressed(item.key()))
            .map(MenuAction::Activate)
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

#[derive(Component)]
struct TitleRoot;

#[derive(Component)]
struct MenuRow(usize);

/// The `>` and `<` around the highlighted entry.
#[derive(Component)]
struct MenuMarker(usize);

/// Text of a row and whether it is the shortcut hint.
#[derive(Component)]
struct MenuText {
    row: usize,
    hint: bool,
}

fn spawn_title(mut commands: Commands, font: Res<GameFont>, pal: Res<UiPalette>, mut menu: ResMut<MainMenu>) {
    // Freshly spawned rows need a first highlight pass.
    menu.set_changed();
    let label = |text: &str, size: f32, color: Color| (Text::new(text), font.text(size), TextColor(color));
    commands
        .spawn((
            TitleRoot,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                padding: UiRect {
                    left: px(28),
                    right: px(28),
                    top: px(96),
                    bottom: px(28),
                },
                ..default()
            },
            BackgroundColor(pal.background),
        ))
        .with_children(|root| {
            root.spawn(label("HAIRPIN HAVOC", 48.0, pal.accent));

            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                ..default()
            })
            .with_children(|list| {
                for (i, item) in MenuItem::ALL.into_iter().enumerate() {
                    list.spawn((
                        MenuRow(i),
                        Node {
                            width: px(ROW_WIDTH),
                            align_items: AlignItems::Center,
                            column_gap: px(16),
                            padding: UiRect::axes(px(16), px(14)),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ))
                    .with_children(|row| {
                        row.spawn((MenuMarker(i), Visibility::Hidden, label(">", 16.0, pal.background)));
                        // The label column grows so every shortcut starts at the same x.
                        row.spawn((
                            MenuText { row: i, hint: false },
                            label(item.label(), 16.0, pal.foreground),
                            Node {
                                flex_grow: 1.0,
                                ..default()
                            },
                        ));
                        row.spawn((
                            MenuText { row: i, hint: true },
                            label(item.key_hint(), 16.0, pal.muted),
                            Node {
                                width: px(96),
                                ..default()
                            },
                        ));
                        row.spawn((MenuMarker(i), Visibility::Hidden, label("<", 16.0, pal.background)));
                    });
                }
            });

            root.spawn(label("^/v MOVE   ENTER SELECT", 8.0, pal.muted));
        });
}

fn despawn_title(mut commands: Commands, q: Query<Entity, With<TitleRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn title_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<MainMenu>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    match menu_action(menu.row, &keys) {
        Some(MenuAction::Move(delta)) => {
            menu.row = wrap_index(menu.row, delta, MenuItem::ALL.len());
            sfx.write(PlaySfx(Sfx::MenuMove));
        }
        Some(MenuAction::Activate(row)) => {
            menu.row = row;
            match MenuItem::ALL[row].target() {
                Some(state) => {
                    sfx.write(PlaySfx(Sfx::MenuSelect));
                    next.set(state);
                }
                None => {
                    exit.write(AppExit::Success);
                }
            }
        }
        None => {}
    }
}

/// Moves the highlight to the selected row.
fn refresh_title(
    menu: Res<MainMenu>,
    pal: Res<UiPalette>,
    mut rows: Query<(&MenuRow, &mut BackgroundColor)>,
    mut markers: Query<(&MenuMarker, &mut Visibility)>,
    mut texts: Query<(&MenuText, &mut TextColor)>,
) {
    if !menu.is_changed() {
        return;
    }
    for (row, mut bg) in &mut rows {
        bg.0 = if row.0 == menu.row { pal.accent } else { Color::NONE };
    }
    for (marker, mut visibility) in &mut markers {
        *visibility = if marker.0 == menu.row {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (text, mut color) in &mut texts {
        color.0 = match (text.row == menu.row, text.hint) {
            (true, _) => pal.background,
            (false, true) => pal.muted,
            (false, false) => pal.foreground,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pressed(key: KeyCode) -> ButtonInput<KeyCode> {
        let mut keys = ButtonInput::default();
        keys.press(key);
        keys
    }

    /// Runs the menu's input system on a single key press from `row`.
    fn press_in_app(row: usize, key: KeyCode) -> App {
        let mut app = App::new();
        app.insert_resource(pressed(key))
            .insert_resource(MainMenu { row })
            .init_resource::<NextState<AppState>>()
            .add_message::<AppExit>()
            .add_message::<PlaySfx>()
            .add_systems(Update, title_input);
        app.update();
        app
    }

    fn pending(app: &App) -> Option<AppState> {
        match app.world().resource::<NextState<AppState>>() {
            NextState::Unchanged => None,
            NextState::Pending(s) | NextState::PendingIfDifferent(s) => Some(*s),
        }
    }

    #[test]
    fn wrap_index_wraps_both_ways() {
        assert_eq!(wrap_index(0, -1, 4), 3);
        assert_eq!(wrap_index(3, 1, 4), 0);
        assert_eq!(wrap_index(1, 1, 4), 2);
        assert_eq!(wrap_index(2, -7, 4), 3);
        assert_eq!(wrap_index(0, 1, 0), 0);
    }

    #[test]
    fn entries_are_in_menu_order() {
        let labels = MenuItem::ALL.map(MenuItem::label);
        assert_eq!(labels, ["PLAY", "GARAGE", "HIGH SCORES", "SETTINGS", "ABOUT", "QUIT"]);
    }

    #[test]
    fn shortcuts_are_unique_and_match_their_hints() {
        for (i, item) in MenuItem::ALL.into_iter().enumerate() {
            let name = match item.key() {
                KeyCode::Escape => "ESC".to_string(),
                key => format!("{key:?}").trim_start_matches("Key").to_string(),
            };
            assert_eq!(item.key_hint(), format!("[{name}]"));
            // A clash with another shortcut or a navigation key would hide this entry.
            assert_eq!(menu_action(0, &pressed(item.key())), Some(MenuAction::Activate(i)));
        }
    }

    #[test]
    fn navigation_keys_move_and_enter_activates_the_row() {
        for key in [KeyCode::ArrowUp, KeyCode::KeyW] {
            assert_eq!(menu_action(2, &pressed(key)), Some(MenuAction::Move(-1)));
        }
        for key in [KeyCode::ArrowDown, KeyCode::KeyS] {
            assert_eq!(menu_action(2, &pressed(key)), Some(MenuAction::Move(1)));
        }
        for key in [KeyCode::Enter, KeyCode::NumpadEnter] {
            assert_eq!(menu_action(2, &pressed(key)), Some(MenuAction::Activate(2)));
        }
        assert_eq!(menu_action(2, &pressed(KeyCode::KeyZ)), None);
    }

    #[test]
    fn highlight_wraps_at_both_ends() {
        let last = MenuItem::ALL.len() - 1;
        let app = press_in_app(0, KeyCode::ArrowUp);
        assert_eq!(app.world().resource::<MainMenu>().row, last);
        let app = press_in_app(last, KeyCode::ArrowDown);
        assert_eq!(app.world().resource::<MainMenu>().row, 0);
        assert_eq!(pending(&app), None);
    }

    #[test]
    fn enter_and_shortcuts_open_the_same_screen() {
        let screens = [
            Some(AppState::TrackSelect),
            Some(AppState::Garage),
            Some(AppState::HighScores),
            Some(AppState::Settings),
            Some(AppState::About),
            None,
        ];
        for (i, (item, screen)) in MenuItem::ALL.into_iter().zip(screens).enumerate() {
            for app in [press_in_app(i, KeyCode::Enter), press_in_app(0, item.key())] {
                assert_eq!(pending(&app), screen, "{item:?}");
                assert_eq!(app.world().resource::<MainMenu>().row, i);
                let quits = !app.world().resource::<Messages<AppExit>>().is_empty();
                assert_eq!(quits, item == MenuItem::Quit, "{item:?}");
            }
        }
    }
}
