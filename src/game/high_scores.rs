//! High scores screen: per-track records in two views, best laps and best finishes.

use bevy::prelude::*;

use crate::core::save::SaveData;
use crate::core::tracks::{TrackId, track_def};

use super::AppState;
use super::audio::{PlaySfx, Sfx};
use super::palette::UiPalette;
use super::profile::Profile;
use super::results::fmt_time;
use super::title::GameFont;

/// Columns of the score table.
const COLUMNS: usize = 3;
/// Shown where nothing is recorded yet.
pub const NO_RECORD: &str = "--";

/// Which records the table shows; kept across visits.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScoreView {
    #[default]
    BestLaps,
    BestFinishes,
}

impl ScoreView {
    pub const ALL: [ScoreView; 2] = [ScoreView::BestLaps, ScoreView::BestFinishes];

    pub fn title(self) -> &'static str {
        match self {
            ScoreView::BestLaps => "BEST LAPS",
            ScoreView::BestFinishes => "BEST FINISHES",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            ScoreView::BestLaps => ScoreView::BestFinishes,
            ScoreView::BestFinishes => ScoreView::BestLaps,
        }
    }

    fn header(self) -> [&'static str; COLUMNS] {
        match self {
            ScoreView::BestLaps => ["TRACK", "BEST LAP", ""],
            ScoreView::BestFinishes => ["TRACK", "FINISH", "RACE TIME"],
        }
    }
}

/// One table row per track, in `TrackId::ALL` order. Empty cells are `NO_RECORD`
/// (the unused third column of the lap view is blank).
pub fn score_rows(view: ScoreView, data: &SaveData) -> Vec<[String; COLUMNS]> {
    TrackId::ALL
        .iter()
        .map(|&id| {
            let name = track_def(id).name.to_uppercase();
            match view {
                ScoreView::BestLaps => {
                    let lap = data.best_lap(id).map_or(NO_RECORD.into(), fmt_time);
                    [name, lap, String::new()]
                }
                ScoreView::BestFinishes => match data.best_race(id) {
                    Some(r) => [name, format!("P{}", r.place), fmt_time(r.time)],
                    None => [name, NO_RECORD.into(), NO_RECORD.into()],
                },
            }
        })
        .collect()
}

pub struct HighScoresPlugin;

impl Plugin for HighScoresPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScoreView>()
            .add_systems(OnEnter(AppState::HighScores), spawn_high_scores)
            .add_systems(
                Update,
                (high_scores_input, refresh_high_scores)
                    .chain()
                    .run_if(in_state(AppState::HighScores)),
            );
    }
}

/// A view tab at the top of the screen.
#[derive(Component)]
struct ViewTab(ScoreView);

/// A table cell; row 0 is the header.
#[derive(Component)]
struct ScoreCell {
    row: usize,
    col: usize,
}

fn spawn_high_scores(mut commands: Commands, font: Res<GameFont>, pal: Res<UiPalette>, mut view: ResMut<ScoreView>) {
    // Freshly spawned cells need a first fill.
    view.set_changed();
    let label = |text: &str, size: f32, color: Color| (Text::new(text), font.text(size), TextColor(color));
    commands
        .spawn((
            DespawnOnExit(AppState::HighScores),
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                padding: UiRect::all(px(28)),
                ..default()
            },
            BackgroundColor(pal.background),
        ))
        .with_children(|root| {
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(20),
                ..default()
            })
            .with_children(|head| {
                head.spawn(label("HIGH SCORES", 32.0, pal.accent));
                head.spawn(Node { column_gap: px(12), ..default() }).with_children(|tabs| {
                    for v in ScoreView::ALL {
                        tabs.spawn((
                            ViewTab(v),
                            Node { padding: UiRect::axes(px(14), px(10)), ..default() },
                            BackgroundColor(Color::NONE),
                        ))
                        .with_children(|tab| {
                            tab.spawn(label(v.title(), 12.0, pal.foreground));
                        });
                    }
                });
            });

            root.spawn((
                Node {
                    display: Display::Grid,
                    grid_template_columns: vec![GridTrack::auto(); COLUMNS],
                    column_gap: px(40),
                    row_gap: px(14),
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(px(32), px(24)),
                    ..default()
                },
                BackgroundColor(pal.panel),
            ))
            .with_children(|grid| {
                for row in 0..=TrackId::ALL.len() {
                    let size = if row == 0 { 8.0 } else { 12.0 };
                    for col in 0..COLUMNS {
                        grid.spawn((ScoreCell { row, col }, label("", size, pal.foreground)));
                    }
                }
            });

            root.spawn(label("</> SWITCH VIEW   ESC BACK", 8.0, pal.muted));
        });
}

fn high_scores_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut view: ResMut<ScoreView>,
    mut next: ResMut<NextState<AppState>>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let switch = [KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::KeyA, KeyCode::KeyD, KeyCode::Tab];
    if keys.any_just_pressed(switch) {
        *view = view.toggled();
        sfx.write(PlaySfx(Sfx::MenuMove));
    } else if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace, KeyCode::KeyH]) {
        sfx.write(PlaySfx(Sfx::MenuSelect));
        next.set(AppState::Title);
    }
}

/// Fills the table and highlights the tab of the current view.
fn refresh_high_scores(
    view: Res<ScoreView>,
    profile: Res<Profile>,
    pal: Res<UiPalette>,
    mut tabs: Query<(&ViewTab, &mut BackgroundColor, &Children)>,
    mut cells: Query<(&ScoreCell, &mut Text, &mut TextColor)>,
    mut tab_labels: Query<&mut TextColor, Without<ScoreCell>>,
) {
    if !view.is_changed() {
        return;
    }
    for (tab, mut bg, children) in &mut tabs {
        let active = tab.0 == *view;
        bg.0 = if active { pal.accent } else { Color::NONE };
        for &child in children {
            if let Ok(mut color) = tab_labels.get_mut(child) {
                color.0 = if active { pal.background } else { pal.muted };
            }
        }
    }
    let header = view.header();
    let rows = score_rows(*view, &profile.data);
    for (cell, mut text, mut text_color) in &mut cells {
        let (value, color) = if cell.row == 0 {
            (header[cell.col].to_string(), pal.muted)
        } else {
            let value = rows[cell.row - 1][cell.col].clone();
            let color = match (cell.col, value.as_str()) {
                (0, _) => pal.foreground,
                (_, NO_RECORD) => pal.muted,
                (_, "P1") => pal.good,
                _ => pal.accent,
            };
            (value, color)
        };
        text.0 = value;
        text_color.0 = color;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::save::BestRace;

    #[test]
    fn every_track_gets_a_row_with_placeholders() {
        let data = SaveData::default();
        for view in ScoreView::ALL {
            let rows = score_rows(view, &data);
            assert_eq!(rows.len(), TrackId::ALL.len());
            assert_eq!(rows[0][0], track_def(TrackId::ALL[0]).name.to_uppercase());
            assert!(rows.iter().all(|r| r[1] == NO_RECORD));
        }
    }

    #[test]
    fn records_fill_their_track_rows() {
        let mut data = SaveData::default();
        data.submit_lap(TrackId::NeonDocks, 48.25);
        data.submit_race(TrackId::NeonDocks, BestRace { place: 2, time: 151.5 });
        let i = TrackId::NeonDocks.index();

        let laps = score_rows(ScoreView::BestLaps, &data);
        assert_eq!(laps[i][1], "0:48.25");
        assert_eq!(laps[i][2], "");

        let finishes = score_rows(ScoreView::BestFinishes, &data);
        assert_eq!(finishes[i][1], "P2");
        assert_eq!(finishes[i][2], "2:31.50");
        assert_eq!(finishes[0][2], NO_RECORD);
    }

    #[test]
    fn toggling_switches_between_both_views() {
        for view in ScoreView::ALL {
            assert_ne!(view.toggled(), view);
            assert_eq!(view.toggled().toggled(), view);
        }
    }
}
