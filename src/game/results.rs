//! Results screen: classification, lap times and credit payout.

use bevy::prelude::*;

use crate::core::economy::{race_payout, trial_payout};

use super::palette::UiPalette;
use super::profile::Profile;
use super::race::RaceOutcome;
use super::title::GameFont;
use super::{AppState, RaceMode};

pub struct ResultsPlugin;

impl Plugin for ResultsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Results), show_results)
            .add_systems(Update, results_input.run_if(in_state(AppState::Results)));
    }
}

/// Format seconds as `m:ss.cc`.
pub fn fmt_time(t: f32) -> String {
    if !t.is_finite() {
        return "--:--.--".into();
    }
    let cs = (t * 100.0).round() as u32;
    format!("{}:{:02}.{:02}", cs / 6000, (cs / 100) % 60, cs % 100)
}

fn show_results(
    mut commands: Commands,
    outcome: Option<Res<RaceOutcome>>,
    mut profile: ResMut<Profile>,
    font: Res<GameFont>,
    pal: Res<UiPalette>,
) {
    let Some(o) = outcome else { return };
    let place = o.place;
    let best = o.best_lap.unwrap_or(f32::INFINITY);
    let new_pb = best.is_finite() && profile.data.submit_lap(o.track, best);
    let drift = o.drift_points.max(0.0) as u32;
    let payout = match o.mode {
        RaceMode::Race => race_payout(place, drift, new_pb),
        RaceMode::TimeTrial => trial_payout(drift, new_pb),
    };
    profile.data.credits += payout.total;
    profile.persist();
    commands.remove_resource::<RaceOutcome>();

    let mut lines: Vec<(String, Color)> = Vec::new();
    let heading = match o.mode {
        RaceMode::Race => format!("FINISHED P{place}"),
        RaceMode::TimeTrial => "RUN COMPLETE".to_string(),
    };
    lines.push((heading, pal.accent));
    if o.mode == RaceMode::Race {
        for (pos, r) in o.rows.iter().enumerate() {
            let time = r.finished_at.map_or("RACING".to_string(), fmt_time);
            let color = if r.is_player { pal.accent } else { pal.foreground };
            lines.push((format!("{}. {:<8} {}", pos + 1, r.name.to_uppercase(), time), color));
        }
    }
    for (k, t) in o.lap_times.iter().enumerate() {
        lines.push((format!("LAP {}  {}", k + 1, fmt_time(*t)), pal.muted));
    }
    lines.push((
        format!("BEST LAP {}{}", fmt_time(best), if new_pb { "  NEW PB!" } else { "" }),
        if new_pb { pal.good } else { pal.foreground },
    ));
    lines.push((
        format!(
            "+{} CR  (base {} drift {} pb {})",
            payout.total, payout.base, payout.drift, payout.pb
        ),
        pal.warn,
    ));
    lines.push((format!("BANK {} CR", profile.data.credits), pal.foreground));
    lines.push(("ENTER race again   G garage   ESC menu".into(), pal.muted));

    commands
        .spawn((
            DespawnOnExit(AppState::Results),
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(14),
                ..default()
            },
            BackgroundColor(pal.background.with_alpha(0.92)),
        ))
        .with_children(|root| {
            for (i, (text, color)) in lines.into_iter().enumerate() {
                let size = if i == 0 { 28.0 } else { 12.0 };
                root.spawn((Text::new(text), font.text(size), TextColor(color)));
            }
        });
}

fn results_input(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<AppState>>) {
    if keys.just_pressed(KeyCode::Enter) {
        next.set(AppState::Race);
    } else if keys.just_pressed(KeyCode::KeyG) {
        next.set(AppState::Garage);
    } else if keys.just_pressed(KeyCode::Escape) {
        next.set(AppState::Title);
    }
}

#[cfg(test)]
mod tests {
    use super::fmt_time;

    #[test]
    fn formats_minutes_seconds_centis() {
        assert_eq!(fmt_time(83.456), "1:23.46");
        assert_eq!(fmt_time(5.0), "0:05.00");
        assert_eq!(fmt_time(f32::INFINITY), "--:--.--");
    }
}
