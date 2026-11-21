//! Time trial: lap ghost recording, personal-best ghost playback, sector
//! deltas against the ghost, and friend ghosts pasted as text codes.

use std::io::{self, Write};
use std::process::{Command, Stdio};

use bevy::prelude::*;

use crate::core::cars::{CarId, car_spec};
use crate::core::ghost::{Ghost, GhostRecorder, MAX_LAP_TIME, decode_code};
use crate::core::save;
use crate::core::tracks::{TrackId, track_def};

use super::palette::UiPalette;
use super::pixel::WORLD_LAYER;
use super::profile::Profile;
use super::race::{Phase, RaceSession, RaceSet, Racer, Toast};
use super::results::fmt_time;
use super::sprites::car_image;
use super::title::GameFont;
use super::{AppState, RaceMode};

/// Label stored in ghosts recorded on this machine.
pub const PLAYER_LABEL: &str = "YOU";
/// Seconds a sector delta stays on screen.
const READOUT_SECS: f32 = 2.5;
const GHOST_ALPHA: f32 = 0.4;
const GHOST_Z: f32 = 19.0;
/// Clipboard payloads larger than this cannot be ghost codes.
const MAX_PASTE_BYTES: usize = 64 * 1024;

/// A ghost loaded from a shared code; survives restarts until cleared.
#[derive(Resource, Default)]
pub struct FriendGhost(pub Option<Ghost>);

/// Per-race time-trial state.
#[derive(Resource)]
struct TrialState {
    track: TrackId,
    car: CarId,
    recorder: GhostRecorder,
    /// Lap time of the last recorded sample, to spot lap timing restarts.
    last_t: f32,
    watch: SplitWatch,
    /// Best ghost known for this track (from disk or set this session).
    pb: Option<Ghost>,
    ghost_visible: bool,
    readout: Option<Readout>,
    readout_timer: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Readout {
    pub text: String,
    pub faster: bool,
}

#[derive(Component)]
struct GhostCar {
    shown: Option<CarId>,
}

#[derive(Component)]
struct DeltaText;

#[derive(Component)]
struct GhostLabel;

pub struct TrialPlugin;

impl Plugin for TrialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FriendGhost>()
            .add_systems(OnEnter(AppState::Race), setup_trial.run_if(in_trial))
            .add_systems(OnExit(AppState::Race), |mut commands: Commands| {
                commands.remove_resource::<TrialState>();
            })
            .add_systems(
                FixedUpdate,
                record_player
                    .after(RaceSet::Simulate)
                    .before(RaceSet::Post)
                    .run_if(in_state(AppState::Race))
                    .run_if(resource_exists::<TrialState>),
            )
            .add_systems(
                Update,
                (trial_keys, drive_ghost_car, update_trial_ui)
                    .run_if(in_state(AppState::Race))
                    .run_if(resource_exists::<TrialState>),
            );
    }
}

fn in_trial(mode: Res<RaceMode>) -> bool {
    *mode == RaceMode::TimeTrial
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// What the split watcher saw change in the player's lap tracker this step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplitEvent {
    /// Sector split `index` (0 or 1) was passed at `time` seconds into the lap.
    Sector { index: usize, time: f32 },
    /// A lap completed; `splits` are the cumulative splits captured before it ended.
    Lap { time: f32, splits: [f32; 2] },
}

/// Follows `LapTracker::splits` and `lap_times` from outside the tracker.
/// A sector pass is a change of its split value (the tracker only writes a
/// split when it is passed); a lap is a new entry in `lap_times`.
#[derive(Debug, Clone, Default)]
pub struct SplitWatch {
    laps_seen: usize,
    last: [f32; 2],
}

impl SplitWatch {
    pub fn observe(&mut self, splits: [f32; 2], lap_times: &[f32]) -> Option<SplitEvent> {
        if lap_times.len() > self.laps_seen {
            self.laps_seen = lap_times.len();
            let captured = self.last;
            self.last = splits;
            return lap_times
                .last()
                .map(|&time| SplitEvent::Lap { time, splits: captured });
        }
        for (index, (&now, seen)) in splits.iter().zip(self.last.iter_mut()).enumerate() {
            if now != *seen {
                *seen = now;
                return Some(SplitEvent::Sector { index, time: now });
            }
        }
        None
    }
}

/// Signed delta formatted to centiseconds: `-0.32`, `+1.05`; a rounded zero is `+0.00`.
pub fn fmt_delta(delta: f32) -> String {
    let cs = (delta * 100.0).round() as i64;
    let sign = if cs < 0 { '-' } else { '+' };
    let abs = cs.unsigned_abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

/// Sector readout comparing the player's cumulative time with the ghost's.
/// `index` 0 and 1 are sector splits, 2 is the full lap.
pub fn sector_readout(index: usize, player: f32, ghost: f32) -> Readout {
    let delta = player - ghost;
    let label = match index {
        0 => "S1",
        1 => "S2",
        _ => "LAP",
    };
    Readout {
        text: format!("{label} {}", fmt_delta(delta)),
        faster: (delta * 100.0).round() < 0.0,
    }
}

/// The ghost's cumulative time for the same split, if it has one.
pub fn ghost_split(ghost: &Ghost, event: SplitEvent) -> (usize, f32, f32) {
    match event {
        SplitEvent::Sector { index, time } => (index, time, ghost.sectors[index.min(1)]),
        SplitEvent::Lap { time, .. } => (2, time, ghost.lap_time),
    }
}

/// Whether a finished lap can be stored as a ghost (decodable code).
pub fn lap_is_storable(lap_time: f32, splits: [f32; 2]) -> bool {
    lap_time.is_finite()
        && lap_time <= MAX_LAP_TIME
        && splits[0] >= 0.01
        && splits[1] > splits[0] + 0.005
        && lap_time > splits[1] + 0.005
}

/// A lap becomes the new PB ghost when there is none or it is faster.
pub fn beats_pb(lap_time: f32, pb: Option<f32>) -> bool {
    pb.is_none_or(|best| lap_time < best)
}

/// The ghost to race: a friend ghost for this track wins over the PB.
pub fn playback_ghost<'a>(
    friend: Option<&'a Ghost>,
    pb: Option<&'a Ghost>,
    track: TrackId,
) -> Option<&'a Ghost> {
    friend
        .filter(|g| g.track == track)
        .or(pb.filter(|g| g.track == track))
}

#[derive(Debug, Clone, PartialEq)]
pub enum PasteOutcome {
    Loaded(Box<Ghost>),
    WrongTrack(TrackId),
    Invalid,
}

/// Interprets pasted text as a friend ghost for `track`.
pub fn classify_paste(text: &str, track: TrackId) -> PasteOutcome {
    if text.len() > MAX_PASTE_BYTES {
        return PasteOutcome::Invalid;
    }
    match decode_code(text) {
        Ok(g) if g.track == track => PasteOutcome::Loaded(Box::new(g)),
        Ok(g) => PasteOutcome::WrongTrack(g.track),
        Err(_) => PasteOutcome::Invalid,
    }
}

// ---------------------------------------------------------------------------
// Clipboard (Wayland tools; Bevy's clipboard is an in-process buffer unless
// the `system_clipboard` feature is enabled)
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum ClipboardError {
    /// The clipboard tool is missing or failed to start.
    Unavailable(io::Error),
    /// The clipboard holds no text.
    Empty,
    /// The tool ran but reported a failure.
    Failed,
}

/// Reads text from the system clipboard with `wl-paste`.
pub fn clipboard_read() -> Result<String, ClipboardError> {
    let out = Command::new("wl-paste")
        .args(["--no-newline", "--type", "text/plain"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(ClipboardError::Unavailable)?;
    if !out.status.success() {
        return Err(ClipboardError::Empty);
    }
    String::from_utf8(out.stdout).map_err(|_| ClipboardError::Empty)
}

/// Copies `text` to the system clipboard with `wl-copy` (it forks to serve the selection).
pub fn clipboard_write(text: &str) -> Result<(), ClipboardError> {
    let mut child = Command::new("wl-copy")
        .args(["--type", "text/plain"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(ClipboardError::Unavailable)?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).map_err(ClipboardError::Unavailable)?;
    }
    let status = child.wait().map_err(ClipboardError::Unavailable)?;
    if status.success() { Ok(()) } else { Err(ClipboardError::Failed) }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

fn setup_trial(
    mut commands: Commands,
    profile: Res<Profile>,
    font: Res<GameFont>,
    pal: Res<UiPalette>,
) {
    let track = profile.data.selected_track_id();
    let pb = profile.dir().and_then(|dir| save::load_ghost(dir, track));
    commands.insert_resource(TrialState {
        track,
        car: profile.data.selected_car,
        recorder: GhostRecorder::new(),
        last_t: 0.0,
        watch: SplitWatch::default(),
        pb,
        ghost_visible: true,
        readout: None,
        readout_timer: 0.0,
    });

    // The image is assigned once the ghost to play is known.
    commands.spawn((
        GhostCar { shown: None },
        Sprite {
            color: Color::srgba(1.0, 1.0, 1.0, GHOST_ALPHA),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, GHOST_Z),
        Visibility::Hidden,
        WORLD_LAYER,
        DespawnOnExit(AppState::Race),
    ));

    // Top-center band: sector delta plus a small ghost status line.
    commands.spawn((
        DespawnOnExit(AppState::Race),
        Node {
            position_type: PositionType::Absolute,
            top: px(0),
            left: px(0),
            width: percent(100),
            height: px(70),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::FlexStart,
            padding: UiRect::top(px(12)),
            row_gap: px(8),
            ..default()
        },
        children![
            (
                DeltaText,
                Text::new(""),
                font.text(24.0),
                TextColor(pal.foreground),
                Visibility::Hidden,
            ),
            (GhostLabel, Text::new(""), font.text(8.0), TextColor(pal.muted)),
        ],
    ));
}

fn record_player(
    session: Res<RaceSession>,
    friend: Res<FriendGhost>,
    profile: Res<Profile>,
    mut state: ResMut<TrialState>,
    racers: Query<&Racer>,
    mut toasts: MessageWriter<Toast>,
) {
    if session.phase != Phase::Running {
        return;
    }
    let Some(player) = racers.iter().find(|r| r.is_player) else { return };
    let st = &player.state;
    let lap_t = player.lap.current_lap_time(session.clock);
    let state = &mut *state;

    let event = state.watch.observe(player.lap.splits, &player.lap.lap_times);
    if let Some(ev) = event
        && let Some(ghost) = playback_ghost(friend.0.as_ref(), state.pb.as_ref(), state.track)
    {
        let (index, mine, theirs) = ghost_split(ghost, ev);
        state.readout = Some(sector_readout(index, mine, theirs));
        state.readout_timer = READOUT_SECS;
    }

    match event {
        Some(SplitEvent::Lap { time, splits }) => {
            let mut recorder = std::mem::take(&mut state.recorder);
            recorder.record(time, st.pos, st.heading, st.speed(), st.drifting);
            if lap_is_storable(time, splits)
                && beats_pb(time, state.pb.as_ref().map(|g| g.lap_time))
            {
                let ghost = recorder.finish(
                    time,
                    [splits[0], splits[1], time],
                    state.car,
                    state.track,
                    PLAYER_LABEL.into(),
                );
                if let Some(dir) = profile.dir()
                    && let Err(e) = save::store_ghost(dir, &ghost)
                {
                    warn!("could not save PB ghost: {e}");
                }
                state.pb = Some(ghost);
                toasts.write(Toast("NEW PB GHOST".into()));
            }
            state.last_t = 0.0;
        }
        _ if lap_t + 1e-3 < state.last_t => {
            // Lap timing restarted (first crossing of the line from the grid).
            state.recorder = GhostRecorder::new();
        }
        _ => {}
    }
    if player.lap.finished_at.is_none() {
        state
            .recorder
            .record(lap_t, st.pos, st.heading, st.speed(), st.drifting);
        state.last_t = lap_t;
    }
}

fn trial_keys(
    keys: Res<ButtonInput<KeyCode>>,
    session: Res<RaceSession>,
    mut state: ResMut<TrialState>,
    mut friend: ResMut<FriendGhost>,
    mut toasts: MessageWriter<Toast>,
) {
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    if keys.just_pressed(KeyCode::KeyG) && !ctrl {
        state.ghost_visible = !state.ghost_visible;
        let msg = if state.ghost_visible { "GHOST ON" } else { "GHOST OFF" };
        toasts.write(Toast(msg.into()));
    }
    if session.phase == Phase::Finished || !ctrl {
        return;
    }
    if keys.just_pressed(KeyCode::KeyV) {
        let msg = match clipboard_read() {
            Err(ClipboardError::Unavailable(e)) => {
                warn!("clipboard read failed: {e}");
                "CLIPBOARD UNAVAILABLE".to_string()
            }
            Err(_) => "CLIPBOARD EMPTY".to_string(),
            Ok(text) => match classify_paste(&text, state.track) {
                PasteOutcome::Loaded(ghost) => {
                    friend.0 = Some(*ghost);
                    "FRIEND GHOST LOADED".to_string()
                }
                PasteOutcome::WrongTrack(t) => {
                    format!("GHOST IS FOR {}", track_def(t).name.to_uppercase())
                }
                PasteOutcome::Invalid => "INVALID GHOST CODE".to_string(),
            },
        };
        toasts.write(Toast(msg));
    } else if keys.just_pressed(KeyCode::Backspace) && friend.0.take().is_some() {
        toasts.write(Toast("FRIEND GHOST CLEARED".into()));
    }
}

fn drive_ghost_car(
    session: Res<RaceSession>,
    state: Res<TrialState>,
    friend: Res<FriendGhost>,
    racers: Query<&Racer>,
    mut images: ResMut<Assets<Image>>,
    mut ghost_car: Query<(&mut GhostCar, &mut Sprite, &mut Transform, &mut Visibility)>,
) {
    let Ok((mut car, mut sprite, mut tf, mut vis)) = ghost_car.single_mut() else { return };
    let ghost = playback_ghost(friend.0.as_ref(), state.pb.as_ref(), state.track);
    let player = racers.iter().find(|r| r.is_player);
    let sample = match (ghost, player) {
        (Some(g), Some(p)) if state.ghost_visible && session.phase == Phase::Running => {
            if p.lap.finished_at.is_some() {
                None
            } else {
                g.sample_at(p.lap.current_lap_time(session.clock))
            }
        }
        _ => None,
    };
    let Some((g, s)) = ghost.zip(sample) else {
        *vis = Visibility::Hidden;
        return;
    };
    if car.shown != Some(g.car) {
        let spec = car_spec(g.car);
        sprite.image = images.add(car_image(spec.body, spec.base_color, spec.accent_color));
        car.shown = Some(g.car);
    }
    tf.translation.x = s.pos.x.round();
    tf.translation.y = s.pos.y.round();
    tf.rotation = Quat::from_rotation_z(s.heading);
    *vis = Visibility::Inherited;
}

type DeltaTextParts = (&'static mut Text, &'static mut TextColor, &'static mut Visibility);

fn update_trial_ui(
    time: Res<Time>,
    pal: Res<UiPalette>,
    friend: Res<FriendGhost>,
    mut state: ResMut<TrialState>,
    mut delta: Query<DeltaTextParts, (With<DeltaText>, Without<GhostLabel>)>,
    mut label: Query<&mut Text, (With<GhostLabel>, Without<DeltaText>)>,
) {
    state.readout_timer = (state.readout_timer - time.delta_secs()).max(0.0);
    if let Ok((mut text, mut color, mut vis)) = delta.single_mut() {
        match &state.readout {
            Some(r) if state.readout_timer > 0.0 => {
                if text.0 != r.text {
                    text.0.clone_from(&r.text);
                }
                color.0 = if r.faster { pal.good } else { pal.bad };
                *vis = Visibility::Inherited;
            }
            _ => *vis = Visibility::Hidden,
        }
    }
    if let Ok(mut text) = label.single_mut() {
        let friend_ghost = friend.0.as_ref().filter(|g| g.track == state.track);
        let line = if !state.ghost_visible {
            "GHOST HIDDEN  G SHOW".to_string()
        } else if let Some(g) = friend_ghost {
            let name = if g.label.is_empty() { "FRIEND" } else { g.label.as_str() };
            format!("VS {} {}  CTRL+BKSP CLEAR", name.to_uppercase(), fmt_time(g.lap_time))
        } else if let Some(g) = &state.pb {
            format!("VS PB {}  CTRL+V FRIEND CODE", fmt_time(g.lap_time))
        } else {
            "NO GHOST YET  CTRL+V FRIEND CODE".to_string()
        };
        if text.0 != line {
            text.0 = line;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ghost::encode_code;
    use bevy::math::Vec2;

    fn ghost(track: TrackId, lap: f32) -> Ghost {
        let mut rec = GhostRecorder::new();
        let mut t = 0.0;
        while t <= lap {
            rec.record(t, Vec2::new(t * 10.0, 0.0), 0.0, 100.0, false);
            t += 1.0 / 30.0;
        }
        rec.finish(lap, [lap * 0.3, lap * 0.6, lap], CarId::Pebble, track, "Pal".into())
    }

    #[test]
    fn delta_formatting() {
        assert_eq!(fmt_delta(-0.321), "-0.32");
        assert_eq!(fmt_delta(1.049), "+1.05");
        assert_eq!(fmt_delta(0.001), "+0.00");
        assert_eq!(fmt_delta(-0.004), "+0.00");
        assert_eq!(fmt_delta(-12.5), "-12.50");
    }

    #[test]
    fn sector_readout_colors_and_labels() {
        let r = sector_readout(0, 10.0, 10.32);
        assert_eq!(r, Readout { text: "S1 -0.32".into(), faster: true });
        let r = sector_readout(1, 21.5, 21.0);
        assert_eq!(r, Readout { text: "S2 +0.50".into(), faster: false });
        let r = sector_readout(2, 40.0, 40.0);
        assert_eq!(r, Readout { text: "LAP +0.00".into(), faster: false });
    }

    #[test]
    fn ghost_split_picks_matching_time() {
        let g = ghost(TrackId::OrchardRing, 30.0);
        let (i, mine, theirs) = ghost_split(&g, SplitEvent::Sector { index: 1, time: 17.0 });
        assert_eq!((i, mine), (1, 17.0));
        assert!((theirs - 18.0).abs() < 1e-4);
        let lap = SplitEvent::Lap { time: 29.0, splits: [9.0, 18.0] };
        assert_eq!(ghost_split(&g, lap), (2, 29.0, 30.0));
    }

    #[test]
    fn split_watch_reports_sectors_and_captured_lap_splits() {
        let mut w = SplitWatch::default();
        assert_eq!(w.observe([0.0, 0.0], &[]), None);
        assert_eq!(
            w.observe([10.0, 0.0], &[]),
            Some(SplitEvent::Sector { index: 0, time: 10.0 })
        );
        assert_eq!(w.observe([10.0, 0.0], &[]), None);
        assert_eq!(
            w.observe([10.0, 21.0], &[]),
            Some(SplitEvent::Sector { index: 1, time: 21.0 })
        );
        assert_eq!(
            w.observe([10.0, 21.0], &[30.0]),
            Some(SplitEvent::Lap { time: 30.0, splits: [10.0, 21.0] })
        );
        assert_eq!(w.observe([10.0, 21.0], &[30.0]), None);
        // Next lap overwrites the stale splits.
        assert_eq!(
            w.observe([9.5, 21.0], &[30.0]),
            Some(SplitEvent::Sector { index: 0, time: 9.5 })
        );
        assert_eq!(
            w.observe([9.5, 20.0], &[30.0, 29.0]),
            Some(SplitEvent::Lap { time: 29.0, splits: [9.5, 21.0] })
        );
    }

    #[test]
    fn storable_laps_and_pb_rule() {
        assert!(lap_is_storable(30.0, [10.0, 20.0]));
        assert!(!lap_is_storable(30.0, [0.0, 20.0]));
        assert!(!lap_is_storable(30.0, [20.0, 10.0]));
        assert!(!lap_is_storable(20.0, [10.0, 20.0]));
        assert!(!lap_is_storable(MAX_LAP_TIME + 1.0, [10.0, 20.0]));
        assert!(!lap_is_storable(f32::INFINITY, [10.0, 20.0]));
        assert!(beats_pb(30.0, None));
        assert!(beats_pb(29.9, Some(30.0)));
        assert!(!beats_pb(30.0, Some(30.0)));
    }

    #[test]
    fn friend_ghost_wins_only_on_its_track() {
        let pb = ghost(TrackId::OrchardRing, 30.0);
        let friend = ghost(TrackId::OrchardRing, 28.0);
        let other = ghost(TrackId::DuneSprint, 25.0);
        let t = TrackId::OrchardRing;
        assert_eq!(playback_ghost(Some(&friend), Some(&pb), t), Some(&friend));
        assert_eq!(playback_ghost(Some(&other), Some(&pb), t), Some(&pb));
        assert_eq!(playback_ghost(None, None, t), None);
        assert_eq!(playback_ghost(None, Some(&other), t), None);
    }

    #[test]
    fn paste_classification() {
        let g = ghost(TrackId::NeonDocks, 20.0);
        let code = encode_code(&g);
        match classify_paste(&format!("{code}\n"), TrackId::NeonDocks) {
            PasteOutcome::Loaded(back) => assert_eq!(back.track, TrackId::NeonDocks),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(
            classify_paste(&code, TrackId::OrchardRing),
            PasteOutcome::WrongTrack(TrackId::NeonDocks)
        );
        assert_eq!(classify_paste("hello", TrackId::OrchardRing), PasteOutcome::Invalid);
        assert_eq!(classify_paste("", TrackId::OrchardRing), PasteOutcome::Invalid);
        let huge = "A".repeat(MAX_PASTE_BYTES + 1);
        assert_eq!(classify_paste(&huge, TrackId::OrchardRing), PasteOutcome::Invalid);
    }
}
