//! In-race HUD: position, laps and timers, minimap, speedometer, nitro and drift
//! gauges, gadget pod, countdown, finish banner and toasts. Also owns the global
//! mute toggle (`M`), which works in every state.

use bevy::prelude::*;

use crate::core::car::KMH_PER_PX;
use crate::core::cars::car_spec;
use crate::core::race::standings;
use crate::core::tracks::track_def;

use super::audio::AudioSettings;
use super::controls::Controls;
use crate::core::settings::Action;
use super::palette::UiPalette;
use super::profile::Profile;
use super::race::{CurrentTrack, Loadout, Phase, RaceSession, Racer, Toast};
use super::results::fmt_time;
use super::title::GameFont;
use super::{AppState, RaceMode};

/// Side length of the minimap box in UI pixels.
pub const MINIMAP_SIZE: f32 = 150.0;
/// Seconds a lap delta stays on screen.
const DELTA_SHOW: f32 = 3.0;
/// Seconds a toast lives; it starts fading `TOAST_FADE` seconds before the end.
const TOAST_LIFE: f32 = 1.2;
const TOAST_FADE: f32 = 0.4;
const MAX_TOASTS: usize = 3;
/// Drift charge thresholds of the three boost tiers (mirrors the car model).
pub const DRIFT_TIER_MARKS: [f32; 3] = [0.32, 0.66, 0.95];
/// Seconds `GO!` stays visible after the green light.
const GO_SHOW: f32 = 0.6;

const BAR_WIDTH: f32 = 240.0;
const EDGE: f32 = 12.0;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LapDeltaState>()
            .add_systems(Update, toggle_mute)
            .add_systems(OnEnter(AppState::Race), spawn_hud)
            .add_systems(
                Update,
                (build_minimap, update_hud, update_minimap_dots, update_toasts)
                    .chain()
                    .run_if(in_state(AppState::Race)),
            );
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Map a world point into a `size`x`size` box (UI coordinates: origin top-left, Y down),
/// scaling uniformly to fit the `lo..hi` world bounds and centering the shorter axis.
pub fn minimap_project(p: Vec2, lo: Vec2, hi: Vec2, size: f32) -> Vec2 {
    let ext = (hi - lo).max(Vec2::splat(1.0));
    let s = size / ext.x.max(ext.y);
    let pad = (Vec2::splat(size) - ext * s) * 0.5;
    Vec2::new((p.x - lo.x) * s + pad.x, (hi.y - p.y) * s + pad.y)
}

/// Color of the drift charge for a boost tier (0 = no boost yet).
pub fn tier_color(tier: u8) -> Color {
    match tier {
        0 => Color::srgb_u8(125, 132, 148),
        1 => Color::srgb_u8(95, 200, 255),
        2 => Color::srgb_u8(255, 161, 61),
        _ => Color::srgb_u8(255, 95, 210),
    }
}

/// Big center text for the start sequence: `3`, `2`, `1`, then `GO!` briefly.
pub fn countdown_label(phase: Phase, clock: f32) -> Option<&'static str> {
    match phase {
        Phase::Countdown => Some(if clock < -1.6 {
            "3"
        } else if clock < -0.8 {
            "2"
        } else {
            "1"
        }),
        Phase::Running if (0.0..GO_SHOW).contains(&clock) => Some("GO!"),
        _ => None,
    }
}

/// Opacity of a toast of the given age; zero once it has expired.
pub fn toast_alpha(age: f32) -> f32 {
    if age >= TOAST_LIFE {
        0.0
    } else {
        ((TOAST_LIFE - age) / TOAST_FADE).clamp(0.0, 1.0)
    }
}

/// Delta of the latest lap against the best lap before it in this run, or the
/// stored personal best when this is the first lap.
pub fn lap_delta(lap_times: &[f32], pb: Option<f32>) -> Option<f32> {
    let (&last, earlier) = lap_times.split_last()?;
    let reference = earlier.iter().copied().reduce(f32::min).or(pb)?;
    Some(last - reference)
}

/// `+0.42` / `-1.20`.
pub fn fmt_delta(d: f32) -> String {
    format!("{}{:.2}", if d < 0.0 { "-" } else { "+" }, d.abs())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PodState {
    Item,
    Active,
    Cooling(f32),
    Ready,
}

/// What the gadget pod shows.
#[derive(Debug, Clone, PartialEq)]
pub struct PodView {
    pub tag: &'static str,
    pub name: &'static str,
    /// 0..1 fill of the pod bar.
    pub fill: f32,
    pub state: PodState,
}

pub fn pod_view(l: &Loadout) -> PodView {
    if let Some(item) = l.item {
        return PodView { tag: "ITEM", name: item.name(), fill: 1.0, state: PodState::Item };
    }
    let g = l.gadget;
    let (fill, state) = if l.active > 0.0 {
        ((l.active / g.active()).clamp(0.0, 1.0), PodState::Active)
    } else if l.cooldown > 0.0 {
        ((l.cooldown / g.cooldown()).clamp(0.0, 1.0), PodState::Cooling(l.cooldown))
    } else {
        (1.0, PodState::Ready)
    };
    PodView { tag: "GADGET", name: g.name(), fill, state }
}

// ---------------------------------------------------------------------------
// Mute
// ---------------------------------------------------------------------------

fn toggle_mute(keys: Res<ButtonInput<KeyCode>>, mut audio: ResMut<AudioSettings>, mut profile: ResMut<Profile>) {
    if keys.just_pressed(KeyCode::KeyM) {
        audio.muted = !audio.muted;
        profile.data.muted = audio.muted;
        profile.persist();
    }
}

// ---------------------------------------------------------------------------
// Components and state
// ---------------------------------------------------------------------------

/// Every text the HUD refreshes each frame.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum HudText {
    Position,
    Lap,
    LapTime,
    TotalTime,
    BestLap,
    Pb,
    Delta,
    Speed,
    Gear,
    DriftPending,
    DriftTotal,
    PodTag,
    PodName,
    PodState,
    Countdown,
    Finish,
    Muted,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum HudBar {
    Rpm,
    Nitro,
    Drift,
    Pod,
}

/// Nitro gauge frame, highlighted while nitro burns.
#[derive(Component)]
struct NitroFrame;

#[derive(Component)]
struct TierMark(usize);

#[derive(Component)]
struct MinimapBox;

#[derive(Component)]
struct MinimapBuilt;

#[derive(Component)]
struct TrackName;

#[derive(Component)]
struct RacerDot {
    racer: Entity,
    size: f32,
}

#[derive(Resource, Clone, Copy)]
struct MinimapFrame {
    lo: Vec2,
    hi: Vec2,
}

#[derive(Component)]
struct ToastStack;

#[derive(Component)]
struct ToastEntry {
    age: f32,
}

#[derive(Resource, Default)]
struct LapDeltaState {
    seen_laps: usize,
    delta: Option<f32>,
    timer: f32,
}

// ---------------------------------------------------------------------------
// Spawning
// ---------------------------------------------------------------------------

/// Secondary text color; readable on panels even when the theme's `muted` is dark.
fn dim(pal: &UiPalette) -> Color {
    pal.foreground.with_alpha(0.62)
}

fn panel(pal: &UiPalette) -> BackgroundColor {
    BackgroundColor(pal.panel.with_alpha(0.78))
}

fn text(font: &GameFont, field: HudText, size: f32, color: Color) -> impl Bundle {
    (field, Text::new(""), font.text(size), TextColor(color))
}

fn bar(field: HudBar, width: f32, height: f32, track: Color, fill: Color) -> impl Bundle {
    (
        Node { width: px(width), height: px(height), ..default() },
        BackgroundColor(track),
        children![(
            field,
            Node { width: percent(0), height: percent(100), ..default() },
            BackgroundColor(fill),
        )],
    )
}

fn spawn_hud(
    mut commands: Commands,
    font: Res<GameFont>,
    pal: Res<UiPalette>,
    mode: Res<RaceMode>,
    controls: Res<Controls>,
    mut delta: ResMut<LapDeltaState>,
) {
    *delta = LapDeltaState::default();
    let use_key = controls.label(Action::UseItem).to_string();
    let use_key_size = if use_key.chars().count() > 2 { 8.0 } else { 12.0 };
    let f = &*font;
    let p = &*pal;
    let track_bg = p.panel_hi;

    commands
        .spawn((
            DespawnOnExit(AppState::Race),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
        ))
        .with_children(|root| {
            // Top-left: standings and timing.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(EDGE),
                    top: px(EDGE),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(8)),
                    row_gap: px(5),
                    min_width: px(190),
                    ..default()
                },
                panel(p),
            ))
            .with_children(|c| {
                if *mode == RaceMode::Race {
                    c.spawn(text(f, HudText::Position, 24.0, p.accent));
                }
                c.spawn(text(f, HudText::Lap, 16.0, p.foreground));
                c.spawn(text(f, HudText::LapTime, 12.0, p.foreground));
                c.spawn(text(f, HudText::TotalTime, 8.0, dim(p)));
                c.spawn(text(f, HudText::BestLap, 8.0, p.foreground));
                c.spawn(text(f, HudText::Pb, 8.0, dim(p)));
                c.spawn(text(f, HudText::Delta, 12.0, p.good));
            });

            // Top-right: minimap, track name and mute tag.
            root.spawn(Node {
                position_type: PositionType::Absolute,
                right: px(EDGE),
                top: px(EDGE),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(6),
                ..default()
            })
            .with_children(|c| {
                c.spawn((
                    MinimapBox,
                    Node {
                        width: px(MINIMAP_SIZE + 12.0),
                        height: px(MINIMAP_SIZE + 12.0),
                        padding: UiRect::all(px(6)),
                        ..default()
                    },
                    panel(p),
                ));
                c.spawn((TrackName, Text::new(""), f.text(8.0), TextColor(dim(p))));
                c.spawn(text(f, HudText::Muted, 8.0, p.warn));
            });

            // Bottom-left: speedometer.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(EDGE),
                    bottom: px(EDGE),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(8)),
                    row_gap: px(6),
                    ..default()
                },
                panel(p),
            ))
            .with_children(|c| {
                c.spawn(Node { align_items: AlignItems::FlexEnd, column_gap: px(6), ..default() })
                    .with_children(|row| {
                        row.spawn(text(f, HudText::Speed, 32.0, p.foreground));
                        row.spawn((Text::new("KM/H"), f.text(8.0), TextColor(dim(p))));
                    });
                c.spawn(text(f, HudText::Gear, 12.0, p.accent));
                c.spawn(bar(HudBar::Rpm, 130.0, 6.0, track_bg, p.accent));
            });

            // Bottom-center: nitro and drift gauges.
            root.spawn(Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                bottom: px(EDGE),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(8)),
                        row_gap: px(5),
                        ..default()
                    },
                    panel(p),
                ))
                .with_children(|c| {
                    c.spawn((Text::new("NITRO"), f.text(8.0), TextColor(dim(p))));
                    c.spawn((
                        NitroFrame,
                        Node {
                            width: px(BAR_WIDTH),
                            height: px(10),
                            border: UiRect::all(px(1)),
                            ..default()
                        },
                        BackgroundColor(track_bg),
                        BorderColor::all(p.panel_hi),
                        children![(
                            HudBar::Nitro,
                            Node { width: percent(0), height: percent(100), ..default() },
                            BackgroundColor(Color::srgb_u8(80, 200, 255)),
                        )],
                    ));
                    c.spawn((Text::new("DRIFT"), f.text(8.0), TextColor(dim(p))));
                    c.spawn((
                        Node { width: px(BAR_WIDTH), height: px(8), ..default() },
                        BackgroundColor(track_bg),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            HudBar::Drift,
                            Node { width: percent(0), height: percent(100), ..default() },
                            BackgroundColor(tier_color(0)),
                        ));
                        for (k, mark) in DRIFT_TIER_MARKS.iter().enumerate() {
                            b.spawn((
                                TierMark(k),
                                Node {
                                    position_type: PositionType::Absolute,
                                    left: percent(mark * 100.0),
                                    top: px(-2),
                                    width: px(2),
                                    height: px(12),
                                    ..default()
                                },
                                BackgroundColor(dim(p)),
                            ));
                        }
                    });
                    c.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() })
                        .with_children(|r| {
                            r.spawn(text(f, HudText::DriftPending, 12.0, p.warn));
                            r.spawn(text(f, HudText::DriftTotal, 8.0, p.foreground));
                        });
                });
            });

            // Bottom-right: gadget pod.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: px(EDGE),
                    bottom: px(EDGE),
                    padding: UiRect::all(px(8)),
                    column_gap: px(8),
                    align_items: AlignItems::Center,
                    ..default()
                },
                panel(p),
            ))
            .with_children(|c| {
                c.spawn((
                    Node {
                        min_width: px(26),
                        height: px(26),
                        padding: UiRect::horizontal(px(4)),
                        border: UiRect::all(px(2)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BorderColor::all(dim(p)),
                    children![(Text::new(use_key), f.text(use_key_size), TextColor(p.foreground))],
                ));
                c.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), ..default() })
                    .with_children(|col| {
                        col.spawn(text(f, HudText::PodTag, 8.0, dim(p)));
                        col.spawn(text(f, HudText::PodName, 12.0, p.foreground));
                        col.spawn(bar(HudBar::Pod, 140.0, 5.0, track_bg, p.accent));
                        col.spawn(text(f, HudText::PodState, 8.0, p.accent));
                    });
            });

            // Center: countdown and finish banner.
            root.spawn(Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|c| {
                c.spawn(text(f, HudText::Countdown, 32.0, p.accent));
                c.spawn((
                    text(f, HudText::Finish, 32.0, p.accent),
                    Node { padding: UiRect::axes(px(24), px(14)), ..default() },
                    panel(p),
                ));
            });

            // Center-lower: toast stack.
            root.spawn((
                ToastStack,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    right: px(0),
                    top: percent(60),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(6),
                    ..default()
                },
            ));
        });
}

/// Builds the minimap once the race's track resource is in place (it is inserted
/// by deferred commands during `OnEnter`, so it is only reliable from `Update`).
fn build_minimap(
    mut commands: Commands,
    pal: Res<UiPalette>,
    track: Option<Res<CurrentTrack>>,
    boxes: Query<Entity, (With<MinimapBox>, Without<MinimapBuilt>)>,
    mut names: Query<&mut Text, With<TrackName>>,
    racers: Query<(Entity, &Racer)>,
) {
    let Some(track) = track else { return };
    let Ok(map) = boxes.single() else { return };
    let t = &track.track;
    let (lo, hi) = t.bounds(10.0);
    commands.insert_resource(MinimapFrame { lo, hi });
    for mut name in &mut names {
        name.0 = track_def(track.id).name.to_uppercase();
    }

    let dot = |c: &mut ChildSpawnerCommands, p: Vec2, size: f32, color: Color| {
        let q = minimap_project(p, lo, hi, MINIMAP_SIZE);
        c.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(6.0 + q.x - size * 0.5),
                top: px(6.0 + q.y - size * 0.5),
                width: px(size),
                height: px(size),
                ..default()
            },
            BackgroundColor(color),
        ));
    };
    commands.entity(map).insert(MinimapBuilt).with_children(|c| {
        for b in &t.branches {
            for s in b.path.samples.iter().step_by(10) {
                dot(c, s.pos, 2.0, pal.foreground.with_alpha(0.3));
            }
        }
        for s in t.main.samples.iter().step_by(10) {
            dot(c, s.pos, 2.0, pal.foreground.with_alpha(0.85));
        }
        // Racer dots last so they draw on top; the player's goes last of all.
        let mut list: Vec<(Entity, &Racer)> = racers.iter().collect();
        list.sort_by_key(|(_, r)| r.is_player);
        for (e, r) in list {
            let (size, color) = if r.is_player {
                (6.0, pal.accent)
            } else {
                let [cr, cg, cb] = car_spec(r.car).base_color;
                (4.0, Color::srgb_u8(cr, cg, cb))
            };
            c.spawn((
                RacerDot { racer: e, size },
                Node {
                    position_type: PositionType::Absolute,
                    width: px(size),
                    height: px(size),
                    ..default()
                },
                BackgroundColor(color),
            ));
        }
    });
}

// ---------------------------------------------------------------------------
// Updating
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn update_hud(
    time: Res<Time>,
    session: Option<Res<RaceSession>>,
    mode: Res<RaceMode>,
    profile: Res<Profile>,
    track: Option<Res<CurrentTrack>>,
    audio: Res<AudioSettings>,
    pal: Res<UiPalette>,
    racers: Query<(&Racer, Option<&Loadout>)>,
    mut delta: ResMut<LapDeltaState>,
    mut texts: Query<(&HudText, &mut Text, &mut TextColor, &mut Visibility)>,
    mut bars: Query<(&HudBar, &mut Node, &mut BackgroundColor)>,
    mut marks: Query<(&TierMark, &mut BackgroundColor), Without<HudBar>>,
    mut frame: Query<&mut BorderColor, With<NitroFrame>>,
) {
    let (Some(session), Some(track)) = (session, track) else { return };
    let list: Vec<(&Racer, Option<&Loadout>)> = racers.iter().collect();
    let Some(&(player, loadout)) = list.iter().find(|(r, _)| r.is_player) else { return };
    let st = &player.state;
    let lap = &player.lap;
    let now = session.clock.max(0.0);

    // Lap delta bookkeeping.
    if lap.lap_times.len() > delta.seen_laps {
        delta.seen_laps = lap.lap_times.len();
        delta.delta = lap_delta(&lap.lap_times, profile.data.best_lap(track.id));
        delta.timer = DELTA_SHOW;
    }
    delta.timer = (delta.timer - time.delta_secs()).max(0.0);

    let position = if *mode == RaceMode::Race {
        let trackers: Vec<_> = list.iter().map(|(r, _)| &r.lap).collect();
        let order = standings(&trackers);
        let place = order.iter().position(|&i| list[i].0.is_player).map_or(0, |p| p + 1);
        format!("P{place}/{}", list.len())
    } else {
        String::new()
    };
    let lap_time = match lap.finished_at {
        Some(_) => lap.lap_times.last().copied().unwrap_or(0.0),
        None if session.phase == Phase::Countdown => 0.0,
        None => lap.current_lap_time(now),
    };
    let total = lap.finished_at.unwrap_or(now);
    let pod = loadout.map(pod_view);
    let tier = st.drift_tier();

    for (field, mut text, mut color, mut vis) in &mut texts {
        let (s, c): (String, Option<Color>) = match field {
            HudText::Position => (position.clone(), None),
            HudText::Lap => (format!("LAP {}/{}", lap.display_lap(), lap.total_laps), None),
            HudText::LapTime => (format!("TIME  {}", fmt_time(lap_time)), None),
            HudText::TotalTime => (format!("TOTAL {}", fmt_time(total)), None),
            HudText::BestLap => (format!("BEST  {}", lap.best_lap.map_or("--:--.--".into(), fmt_time)), None),
            HudText::Pb => (
                format!("PB    {}", profile.data.best_lap(track.id).map_or("--:--.--".into(), fmt_time)),
                None,
            ),
            HudText::Delta => match delta.delta {
                Some(d) if delta.timer > 0.0 => (fmt_delta(d), Some(if d < 0.0 { pal.good } else { pal.bad })),
                _ => (String::new(), None),
            },
            HudText::Speed => (format!("{:>3}", (st.speed() * KMH_PER_PX).round() as i32), None),
            HudText::Gear => (format!("GEAR {}", st.gear), None),
            HudText::DriftPending => {
                if st.drifting && st.drift_pending > 0.0 {
                    (format!("+{}", st.drift_pending as i32), Some(tier_color(tier.max(1))))
                } else {
                    (String::new(), None)
                }
            }
            HudText::DriftTotal => (format!("DRIFT {}", player.drift_points.max(0.0) as i32), None),
            HudText::PodTag => (pod.as_ref().map_or("", |v| v.tag).to_string(), None),
            HudText::PodName => (pod.as_ref().map_or("", |v| v.name).to_string(), None),
            HudText::PodState => match pod.as_ref().map(|v| v.state) {
                Some(PodState::Item) => ("READY".into(), Some(pal.warn)),
                Some(PodState::Active) => ("ACTIVE".into(), Some(pal.good)),
                Some(PodState::Cooling(t)) => (format!("{t:.1}s"), Some(dim(&pal))),
                Some(PodState::Ready) => ("READY".into(), Some(pal.accent)),
                None => (String::new(), None),
            },
            HudText::Countdown => (countdown_label(session.phase, session.clock).unwrap_or("").into(), None),
            HudText::Finish => {
                let s = if session.phase == Phase::Finished { "FINISH" } else { "" };
                (s.into(), None)
            }
            HudText::Muted => ((if audio.muted { "MUTED" } else { "" }).into(), None),
        };
        if text.0 != s {
            text.0 = s;
        }
        if let Some(c) = c {
            color.0 = c;
        }
        let want = if text.0.is_empty() { Visibility::Hidden } else { Visibility::Inherited };
        vis.set_if_neq(want);
    }

    for (field, mut node, mut bg) in &mut bars {
        let (fill, c) = match field {
            HudBar::Rpm => (st.rpm, if st.rpm > 0.9 { pal.bad } else { pal.accent }),
            HudBar::Nitro => (
                st.nitro,
                if st.nitro_active { Color::srgb_u8(170, 240, 255) } else { Color::srgb_u8(80, 200, 255) },
            ),
            HudBar::Drift => (st.drift_charge, tier_color(tier)),
            HudBar::Pod => match pod.as_ref() {
                Some(v) => (
                    v.fill,
                    match v.state {
                        PodState::Item => pal.warn,
                        PodState::Active => pal.good,
                        PodState::Cooling(_) => dim(&pal),
                        PodState::Ready => pal.accent,
                    },
                ),
                None => (0.0, dim(&pal)),
            },
        };
        let w = percent(fill.clamp(0.0, 1.0) * 100.0);
        if node.width != w {
            node.width = w;
        }
        bg.0 = c;
    }
    for (mark, mut bg) in &mut marks {
        bg.0 = if (tier as usize) > mark.0 { tier_color(mark.0 as u8 + 1) } else { dim(&pal) };
    }
    for mut border in &mut frame {
        *border = BorderColor::all(if st.nitro_active { pal.accent } else { pal.panel_hi });
    }
}

fn update_minimap_dots(
    frame: Option<Res<MinimapFrame>>,
    racers: Query<&Racer>,
    mut dots: Query<(&RacerDot, &mut Node)>,
) {
    let Some(frame) = frame else { return };
    for (dot, mut node) in &mut dots {
        let Ok(r) = racers.get(dot.racer) else { continue };
        let q = minimap_project(r.state.pos, frame.lo, frame.hi, MINIMAP_SIZE);
        node.left = px(6.0 + q.x - dot.size * 0.5);
        node.top = px(6.0 + q.y - dot.size * 0.5);
    }
}

fn update_toasts(
    mut commands: Commands,
    time: Res<Time>,
    font: Res<GameFont>,
    pal: Res<UiPalette>,
    mut incoming: MessageReader<Toast>,
    stack: Query<Entity, With<ToastStack>>,
    mut toasts: Query<(Entity, &mut ToastEntry, &mut TextColor, &mut BackgroundColor)>,
) {
    let dt = time.delta_secs();
    let mut alive: Vec<(Entity, f32)> = Vec::new();
    for (e, mut toast, mut color, mut bg) in &mut toasts {
        toast.age += dt;
        let a = toast_alpha(toast.age);
        if a <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        color.0 = color.0.with_alpha(a);
        bg.0 = bg.0.with_alpha(0.7 * a);
        alive.push((e, toast.age));
    }
    let Ok(stack) = stack.single() else {
        incoming.clear();
        return;
    };
    let fresh: Vec<String> = incoming.read().map(|t| t.0.clone()).collect();
    // Keep at most MAX_TOASTS on screen, dropping the oldest first.
    let excess = (alive.len() + fresh.len()).saturating_sub(MAX_TOASTS);
    alive.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (e, _) in alive.iter().take(excess) {
        commands.entity(*e).despawn();
    }
    let skip = fresh.len().saturating_sub(MAX_TOASTS);
    for msg in fresh.into_iter().skip(skip) {
        let child = commands
            .spawn((
                ToastEntry { age: 0.0 },
                Text::new(msg),
                font.text(16.0),
                TextColor(pal.warn),
                Node { padding: UiRect::axes(px(10), px(5)), ..default() },
                BackgroundColor(pal.panel.with_alpha(0.7)),
            ))
            .id();
        commands.entity(stack).add_child(child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::economy::{BoxItem, GadgetId};

    fn loadout(cooldown: f32, active: f32, item: Option<BoxItem>) -> Loadout {
        Loadout { gadget: GadgetId::Thumper, cooldown, active, item, last_item: None }
    }

    #[test]
    fn minimap_fits_and_flips_y() {
        let (lo, hi) = (Vec2::new(-100.0, -50.0), Vec2::new(100.0, 50.0));
        assert_eq!(minimap_project(lo, lo, hi, 150.0), Vec2::new(0.0, 112.5));
        assert_eq!(minimap_project(hi, lo, hi, 150.0), Vec2::new(150.0, 37.5));
        // Center maps to the center of the box.
        assert_eq!(minimap_project(Vec2::ZERO, lo, hi, 150.0), Vec2::splat(75.0));
    }

    #[test]
    fn minimap_handles_degenerate_bounds() {
        let p = minimap_project(Vec2::ONE, Vec2::ONE, Vec2::ONE, 150.0);
        assert!(p.is_finite());
    }

    #[test]
    fn tier_colors_are_distinct() {
        let c: Vec<Color> = (0..4).map(tier_color).collect();
        for i in 0..4 {
            for j in (i + 1)..4 {
                assert_ne!(c[i], c[j]);
            }
        }
        assert_eq!(tier_color(7), tier_color(3));
    }

    #[test]
    fn countdown_sequence() {
        assert_eq!(countdown_label(Phase::Countdown, -2.3), Some("3"));
        assert_eq!(countdown_label(Phase::Countdown, -1.2), Some("2"));
        assert_eq!(countdown_label(Phase::Countdown, -0.1), Some("1"));
        assert_eq!(countdown_label(Phase::Running, 0.2), Some("GO!"));
        assert_eq!(countdown_label(Phase::Running, 0.7), None);
        assert_eq!(countdown_label(Phase::Finished, 0.1), None);
    }

    #[test]
    fn toast_fades_out() {
        assert_eq!(toast_alpha(0.0), 1.0);
        assert_eq!(toast_alpha(0.7), 1.0);
        assert!((toast_alpha(1.0) - 0.5).abs() < 1e-4);
        assert_eq!(toast_alpha(1.2), 0.0);
    }

    #[test]
    fn lap_delta_uses_run_best_then_pb() {
        assert_eq!(lap_delta(&[], Some(60.0)), None);
        assert_eq!(lap_delta(&[62.0], None), None);
        assert_eq!(lap_delta(&[62.0], Some(60.0)), Some(2.0));
        assert_eq!(lap_delta(&[62.0, 61.0, 59.5], Some(50.0)), Some(-1.5));
        assert_eq!(fmt_delta(-1.5), "-1.50");
        assert_eq!(fmt_delta(0.0), "+0.00");
    }

    #[test]
    fn pod_states() {
        assert_eq!(pod_view(&loadout(0.0, 0.0, None)).state, PodState::Ready);
        let cooling = pod_view(&loadout(5.5, 0.0, None));
        assert_eq!(cooling.state, PodState::Cooling(5.5));
        assert!((cooling.fill - 5.5 / 11.0).abs() < 1e-5);
        assert_eq!(pod_view(&loadout(9.0, 0.2, None)).state, PodState::Active);
        let item = pod_view(&loadout(3.0, 0.0, Some(BoxItem::Jammer)));
        assert_eq!((item.tag, item.name, item.state), ("ITEM", "JAMMER", PodState::Item));
    }
}
