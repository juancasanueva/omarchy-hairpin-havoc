//! Confirmation dialog for restarting or quitting a race. The race is paused while it is open.

use bevy::prelude::*;

use super::palette::UiPalette;
use super::race::race_keys;
use super::title::{GameFont, wrap_index};
use super::{AppState, RaceMode};

const PANEL_WIDTH: f32 = 440.0;
/// Above the HUD.
const DIALOG_Z: i32 = 10;

pub struct ConfirmPlugin;

impl Plugin for ConfirmPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(AppState::Race), |mut commands: Commands| {
            commands.remove_resource::<ConfirmDialog>();
        })
        .add_systems(
            Update,
            (
                sync_pause,
                spawn_dialog.run_if(resource_added::<ConfirmDialog>),
                despawn_dialog.run_if(resource_removed::<ConfirmDialog>),
                refresh_dialog.run_if(resource_exists_and_changed::<ConfirmDialog>),
            )
                .chain()
                .after(race_keys),
        );
    }
}

/// Run condition for race keys that must do nothing while the dialog is open.
pub fn no_dialog(dialog: Option<Res<ConfirmDialog>>) -> bool {
    dialog.is_none()
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// What the player asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    Restart,
    Quit,
}

impl Confirm {
    /// The request made by this frame's key presses.
    pub fn requested(keys: &ButtonInput<KeyCode>) -> Option<Self> {
        if keys.just_pressed(KeyCode::Escape) {
            Some(Confirm::Quit)
        } else if keys.just_pressed(KeyCode::KeyR) {
            Some(Confirm::Restart)
        } else {
            None
        }
    }

    /// Screen to go to once confirmed. Re-entering the race rebuilds it.
    pub fn target(self) -> AppState {
        match self {
            Confirm::Restart => AppState::Race,
            Confirm::Quit => AppState::TrackSelect,
        }
    }

    fn title(self, mode: RaceMode) -> &'static str {
        match (self, mode) {
            (Confirm::Restart, RaceMode::Race) => "RESTART RACE?",
            (Confirm::Restart, RaceMode::TimeTrial) => "RESTART RUN?",
            (Confirm::Quit, RaceMode::Race) => "QUIT RACE?",
            (Confirm::Quit, RaceMode::TimeTrial) => "QUIT RUN?",
        }
    }

    fn note(mode: RaceMode) -> &'static str {
        match mode {
            RaceMode::Race => "This race will be lost.",
            RaceMode::TimeTrial => "This run will be lost.",
        }
    }

    fn accept_label(self) -> &'static str {
        match self {
            Confirm::Restart => "RESTART",
            Confirm::Quit => "QUIT TO TRACKS",
        }
    }
}

const CANCEL_LABEL: &str = "KEEP RACING";
const CANCEL_ROW: usize = 0;
const ROWS: usize = 2;

/// Outcome of one frame of dialog input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    Moved,
    Cancelled,
    Accepted,
}

/// The open dialog; while this resource exists the race is paused.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfirmDialog {
    pub action: Confirm,
    row: usize,
}

impl ConfirmDialog {
    /// Opens on the cancel row, so a stray Enter keeps racing.
    pub fn new(action: Confirm) -> Self {
        Self { action, row: CANCEL_ROW }
    }

    /// `row` after this frame's key presses, and what they mean.
    fn step(row: usize, keys: &ButtonInput<KeyCode>) -> (usize, Option<Reply>) {
        if keys.just_pressed(KeyCode::Escape) {
            (row, Some(Reply::Cancelled))
        } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
            let reply = if row == CANCEL_ROW { Reply::Cancelled } else { Reply::Accepted };
            (row, Some(reply))
        } else if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
            (wrap_index(row, -1, ROWS), Some(Reply::Moved))
        } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
            (wrap_index(row, 1, ROWS), Some(Reply::Moved))
        } else {
            (row, None)
        }
    }

    /// Applies this frame's key presses.
    pub fn input(dialog: &mut ResMut<Self>, keys: &ButtonInput<KeyCode>) -> Option<Reply> {
        let (row, reply) = Self::step(dialog.row, keys);
        // Only a real move may mark the dialog as changed.
        if row != dialog.row {
            dialog.row = row;
        }
        reply
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Virtual time drives the fixed-step race and every race animation.
fn sync_pause(dialog: Option<Res<ConfirmDialog>>, mut time: ResMut<Time<Virtual>>) {
    match (dialog.is_some(), time.is_paused()) {
        (true, false) => time.pause(),
        (false, true) => time.unpause(),
        _ => {}
    }
}

#[derive(Component)]
struct DialogRoot;

#[derive(Component)]
struct DialogRow(usize);

#[derive(Component)]
struct DialogText(usize);

fn spawn_dialog(
    mut commands: Commands,
    dialog: Res<ConfirmDialog>,
    mode: Res<RaceMode>,
    font: Res<GameFont>,
    pal: Res<UiPalette>,
) {
    let label = |text: &str, size: f32, color: Color| (Text::new(text), font.text(size), TextColor(color));
    commands
        .spawn((
            DialogRoot,
            DespawnOnExit(AppState::Race),
            GlobalZIndex(DIALOG_Z),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(PANEL_WIDTH),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(18),
                    padding: UiRect::axes(px(28), px(28)),
                    border: UiRect::all(px(2)),
                    ..default()
                },
                BackgroundColor(pal.background),
                BorderColor::all(pal.accent),
            ))
            .with_children(|panel| {
                panel.spawn(label(dialog.action.title(*mode), 24.0, pal.accent));
                panel.spawn(label(Confirm::note(*mode), 10.0, pal.foreground));
                panel
                    .spawn(Node {
                        width: percent(100),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        ..default()
                    })
                    .with_children(|list| {
                        for (i, text) in [CANCEL_LABEL, dialog.action.accept_label()].into_iter().enumerate() {
                            list.spawn((
                                DialogRow(i),
                                Node {
                                    justify_content: JustifyContent::Center,
                                    padding: UiRect::axes(px(16), px(14)),
                                    ..default()
                                },
                                BackgroundColor(Color::NONE),
                            ))
                            .with_children(|row| {
                                row.spawn((DialogText(i), label(text, 16.0, pal.foreground)));
                            });
                        }
                    });
                panel.spawn(label("^/v MOVE   ENTER SELECT   ESC KEEP RACING", 8.0, pal.muted));
            });
        });
}

fn despawn_dialog(mut commands: Commands, q: Query<Entity, With<DialogRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

/// Moves the highlight to the selected row.
fn refresh_dialog(
    dialog: Res<ConfirmDialog>,
    pal: Res<UiPalette>,
    mut rows: Query<(&DialogRow, &mut BackgroundColor)>,
    mut texts: Query<(&DialogText, &mut TextColor)>,
) {
    for (row, mut bg) in &mut rows {
        bg.0 = if row.0 == dialog.row { pal.accent } else { Color::NONE };
    }
    for (text, mut color) in &mut texts {
        color.0 = if text.0 == dialog.row { pal.background } else { pal.foreground };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCEPT_ROW: usize = 1;

    fn step(row: usize, key: KeyCode) -> (usize, Option<Reply>) {
        let mut keys = ButtonInput::default();
        keys.press(key);
        ConfirmDialog::step(row, &keys)
    }

    #[test]
    fn opens_on_keep_racing_and_a_stray_enter_cancels() {
        let dialog = ConfirmDialog::new(Confirm::Quit);
        assert_eq!(dialog.row, CANCEL_ROW);
        assert_eq!(step(dialog.row, KeyCode::Enter).1, Some(Reply::Cancelled));
        assert_eq!(step(dialog.row, KeyCode::NumpadEnter).1, Some(Reply::Cancelled));
    }

    #[test]
    fn accepting_takes_a_move_and_enter() {
        for key in [KeyCode::ArrowDown, KeyCode::KeyS, KeyCode::ArrowUp, KeyCode::KeyW] {
            assert_eq!(step(CANCEL_ROW, key), (ACCEPT_ROW, Some(Reply::Moved)));
            assert_eq!(step(ACCEPT_ROW, key), (CANCEL_ROW, Some(Reply::Moved)));
        }
        assert_eq!(step(ACCEPT_ROW, KeyCode::Enter).1, Some(Reply::Accepted));
        assert_eq!(step(ACCEPT_ROW, KeyCode::NumpadEnter).1, Some(Reply::Accepted));
    }

    #[test]
    fn escape_always_cancels_and_driving_keys_do_nothing() {
        for row in 0..ROWS {
            assert_eq!(step(row, KeyCode::Escape), (row, Some(Reply::Cancelled)));
            for key in [KeyCode::KeyR, KeyCode::Space, KeyCode::ShiftLeft, KeyCode::KeyE, KeyCode::KeyA] {
                assert_eq!(step(row, key), (row, None));
            }
        }
    }

    #[test]
    fn titles_name_the_mode() {
        assert_eq!(Confirm::Restart.title(RaceMode::Race), "RESTART RACE?");
        assert_eq!(Confirm::Quit.title(RaceMode::TimeTrial), "QUIT RUN?");
    }

    #[test]
    fn the_race_is_paused_only_while_the_dialog_is_open() {
        let mut app = App::new();
        app.init_resource::<Time<Virtual>>().add_systems(Update, sync_pause);
        let paused = |app: &mut App| {
            app.update();
            app.world().resource::<Time<Virtual>>().is_paused()
        };
        assert!(!paused(&mut app));
        app.insert_resource(ConfirmDialog::new(Confirm::Restart));
        assert!(paused(&mut app));
        assert!(paused(&mut app), "stays paused");
        app.world_mut().remove_resource::<ConfirmDialog>();
        assert!(!paused(&mut app));
    }
}
