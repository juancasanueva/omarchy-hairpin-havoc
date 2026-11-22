//! Results screen: classification, lap times and credit payout.

use bevy::prelude::*;

use crate::core::economy::{race_payout, trial_payout};
use crate::core::ghost::{Ghost, encode_code};
use crate::core::save;
use crate::core::tracks::TrackId;

use super::palette::UiPalette;
use super::profile::Profile;
use super::race::RaceOutcome;
use super::title::GameFont;
use super::trial::{ClipboardError, clipboard_write};
use super::{AppState, RaceMode};

pub struct ResultsPlugin;

impl Plugin for ResultsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Results), show_results)
            .add_systems(Update, results_input.run_if(in_state(AppState::Results)));
    }
}

/// What the results screen is showing, for actions after the outcome is consumed.
#[derive(Resource, Clone, Copy, Debug)]
struct ShownResults {
    mode: RaceMode,
    track: TrackId,
}

/// Status line under the time-trial footer ("COPIED").
#[derive(Component)]
struct CopyStatus;

/// Copies the code of `ghost` with `write` and returns the status to show.
pub fn copy_ghost_code<E>(
    ghost: Option<&Ghost>,
    write: impl FnOnce(&str) -> Result<(), E>,
) -> &'static str {
    match ghost {
        None => "NO GHOST YET",
        Some(g) => match write(&encode_code(g)) {
            Ok(()) => "COPIED",
            Err(_) => "COPY FAILED",
        },
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
    commands.insert_resource(ShownResults { mode: o.mode, track: o.track });

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
    lines.push(("ENTER race again   G garage   ESC tracks".into(), pal.muted));
    let trial = o.mode == RaceMode::TimeTrial;
    if trial {
        lines.push(("C copy ghost code".into(), pal.muted));
    }

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
            if trial {
                root.spawn((CopyStatus, Text::new(""), font.text(8.0), TextColor(pal.good)));
            }
        });
}

fn results_input(
    keys: Res<ButtonInput<KeyCode>>,
    shown: Option<Res<ShownResults>>,
    profile: Res<Profile>,
    pal: Res<UiPalette>,
    mut status: Query<(&mut Text, &mut TextColor), With<CopyStatus>>,
    mut next: ResMut<NextState<AppState>>,
) {
    if keys.just_pressed(KeyCode::KeyC)
        && let Some(shown) = shown.filter(|s| s.mode == RaceMode::TimeTrial)
    {
        let ghost = profile.dir().and_then(|dir| save::load_ghost(dir, shown.track));
        let msg = copy_ghost_code(ghost.as_ref(), |code| {
            clipboard_write(code).inspect_err(|e| {
                if let ClipboardError::Unavailable(err) = e {
                    warn!("clipboard write failed: {err}");
                }
            })
        });
        if let Ok((mut text, mut color)) = status.single_mut() {
            text.0 = msg.into();
            color.0 = if msg == "COPIED" { pal.good } else { pal.bad };
        }
    } else if keys.just_pressed(KeyCode::Enter) {
        next.set(AppState::Race);
    } else if keys.just_pressed(KeyCode::KeyG) {
        next.set(AppState::Garage);
    } else if keys.just_pressed(KeyCode::Escape) {
        next.set(AppState::TrackSelect);
    }
}

#[cfg(test)]
mod tests {
    use super::{copy_ghost_code, fmt_time};
    use crate::core::cars::CarId;
    use crate::core::ghost::{GhostRecorder, decode_code};
    use crate::core::tracks::TrackId;
    use bevy::math::Vec2;

    #[test]
    fn copy_ghost_code_reports_status() {
        assert_eq!(copy_ghost_code(None, |_| Ok::<(), ()>(())), "NO GHOST YET");
        let mut rec = GhostRecorder::new();
        for i in 0..60 {
            rec.record(i as f32 / 30.0, Vec2::new(i as f32, 0.0), 0.0, 50.0, false);
        }
        let g = rec.finish(2.0, [0.5, 1.0, 2.0], CarId::Pebble, TrackId::OrchardRing, "YOU".into());
        let mut copied = String::new();
        let msg = copy_ghost_code(Some(&g), |code| {
            copied = code.to_string();
            Ok::<(), ()>(())
        });
        assert_eq!(msg, "COPIED");
        assert_eq!(decode_code(&copied).unwrap().track, TrackId::OrchardRing);
        assert_eq!(copy_ghost_code(Some(&g), |_| Err(())), "COPY FAILED");
    }

    #[test]
    fn formats_minutes_seconds_centis() {
        assert_eq!(fmt_time(83.456), "1:23.46");
        assert_eq!(fmt_time(5.0), "0:05.00");
        assert_eq!(fmt_time(f32::INFINITY), "--:--.--");
    }
}
