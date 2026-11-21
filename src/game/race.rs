//! Race session: setup, fixed-step simulation, laps, countdown and finish.

use bevy::prelude::*;

use crate::core::ai::{self, AiBrain};
use crate::core::car::{self, CarInput, CarModifiers, CarState, CarTuning, DT};
use crate::core::cars::{CarId, car_spec};
use crate::core::economy::{BoxItem, GadgetId, PaintId};
use crate::core::race::{LapEvent, LapTracker, standings};
use crate::core::track::Track;
use crate::core::tracks::{LAPS, TrackId, track_def};

use super::audio::{EngineAudio, PlaySfx, Sfx};
use super::pixel::{WORLD_LAYER, WorldCamera};
use super::profile::Profile;
use super::stack::{CAR_SORT, SORT_STEP, StackArt, StackPose, car_slices, ground_z, spawn_stack};
use super::theme::world_theme;
use super::track_view::{grid_slot, spawn_track_view};
use super::{AppState, RaceMode};

pub const COUNTDOWN: f32 = 2.4;
const FINISH_HOLD: f32 = 1.65;

#[derive(Resource)]
pub struct CurrentTrack {
    pub id: TrackId,
    pub track: Track,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Countdown,
    Running,
    Finished,
}

#[derive(Resource)]
pub struct RaceSession {
    pub mode: RaceMode,
    pub phase: Phase,
    /// Seconds since the green light (negative during the countdown).
    pub clock: f32,
    pub finish_timer: f32,
    pub wide_camera: bool,
}

#[derive(Component)]
pub struct Racer {
    pub slot: usize,
    pub is_player: bool,
    pub car: CarId,
    pub name: &'static str,
    pub tuning: CarTuning,
    pub state: CarState,
    pub input: CarInput,
    pub mods: CarModifiers,
    pub lap: LapTracker,
    pub drift_points: f32,
    pub brain: Option<AiBrain>,
}

/// Ordering seams for other plugins' `FixedUpdate` systems.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum RaceSet {
    /// Player and AI inputs are written to `Racer::input`.
    Input,
    /// Physics, contacts and laps.
    Simulate,
    /// Race clock and phase changes.
    Post,
}

/// Gadget and lucky-box state; attached to the player's racer.
#[derive(Component, Debug, Clone)]
pub struct Loadout {
    pub gadget: GadgetId,
    /// Seconds until the gadget can be used again.
    pub cooldown: f32,
    /// Seconds the current gadget effect remains active.
    pub active: f32,
    pub item: Option<BoxItem>,
    pub last_item: Option<BoxItem>,
}

/// Snapshot of a finished race, kept after race entities despawn.
#[derive(Resource, Clone, Debug)]
pub struct RaceOutcome {
    pub mode: RaceMode,
    pub track: TrackId,
    /// Classified order.
    pub rows: Vec<OutcomeRow>,
    pub place: u8,
    pub lap_times: Vec<f32>,
    pub best_lap: Option<f32>,
    pub drift_points: f32,
}

#[derive(Clone, Debug)]
pub struct OutcomeRow {
    pub name: &'static str,
    pub is_player: bool,
    pub finished_at: Option<f32>,
}

fn snapshot(mode: RaceMode, track: TrackId, racers: &[&Racer]) -> Option<RaceOutcome> {
    let order = classification(racers);
    let player = racers.iter().find(|r| r.is_player)?;
    Some(RaceOutcome {
        mode,
        track,
        rows: order
            .iter()
            .map(|&i| OutcomeRow {
                name: racers[i].name,
                is_player: racers[i].is_player,
                finished_at: racers[i].lap.finished_at,
            })
            .collect(),
        place: order.iter().position(|&i| racers[i].is_player).map_or(4, |p| p + 1) as u8,
        lap_times: player.lap.lap_times.clone(),
        best_lap: player.lap.best_lap,
        drift_points: player.drift_points,
    })
}

/// Short on-screen callouts ("PAD BOOST", "LAP 2").
#[derive(Message, Clone, Debug)]
pub struct Toast(pub String);

/// Per-step physics events of the player, for FX and HUD.
#[derive(Message, Clone, Copy, Debug)]
pub struct PlayerStep(pub car::StepEvents);

#[derive(Resource, Default)]
struct SteerMemory {
    last: f32,
}

pub struct RacePlugin;

impl Plugin for RacePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(120.0))
            .init_resource::<SteerMemory>()
            .add_message::<Toast>()
            .add_message::<PlayerStep>()
            .add_systems(OnEnter(AppState::Race), setup_race)
            .add_systems(OnExit(AppState::Race), silence_engine)
            .configure_sets(
                FixedUpdate,
                (RaceSet::Input, RaceSet::Simulate, RaceSet::Post)
                    .chain()
                    .run_if(in_state(AppState::Race)),
            )
            .add_systems(
                FixedUpdate,
                (
                    (player_input, ai_input).chain().in_set(RaceSet::Input),
                    simulate.in_set(RaceSet::Simulate),
                    advance_clock.in_set(RaceSet::Post),
                ),
            )
            .add_systems(
                Update,
                (race_keys, sync_visuals, follow_camera, engine_audio).run_if(in_state(AppState::Race)),
            );
    }
}

fn setup_race(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    profile: Res<Profile>,
    mode: Res<RaceMode>,
    mut camera: Single<&mut Transform, With<WorldCamera>>,
) {
    let id = profile.data.selected_track_id();
    let def = track_def(id);
    let track = def.build();
    let theme = world_theme(id);
    spawn_track_view(&mut commands, &mut meshes, &mut materials, &mut images, &track, &theme, 0x9e37 + id.index() as u32 * 7919);

    let player_car = profile.data.selected_car;
    let mut field: Vec<(CarId, bool)> = Vec::new();
    if *mode == RaceMode::Race {
        // Rivals use other cars, rotated per track so each circuit has a different field.
        let rivals: Vec<CarId> = CarId::ALL
            .iter()
            .copied()
            .filter(|&c| c != player_car)
            .cycle()
            .skip(id.index() * 2)
            .take(3)
            .collect();
        field.extend([(rivals[0], false), (rivals[1], false), (player_car, true), (rivals[2], false)]);
    } else {
        field.push((player_car, true));
    }

    for (slot, &(car, is_player)) in field.iter().enumerate() {
        let spec = car_spec(car);
        let paint = if is_player { profile.data.paint_for(car) } else { PaintId::Factory };
        let art = StackArt::new(&car_slices(car, paint), &mut images);
        let (pos, dir) = grid_slot(&track, slot);
        let state = CarState::new(pos, dir.to_angle());
        let progress = track.query(pos).progress;
        let tuning = if is_player { profile.data.tuning_for(car) } else { spec.tuning };
        // `HAIRPIN_AUTODRIVE=1` lets the AI drive the player car (attract mode, testing).
        let autodrive = std::env::var_os("HAIRPIN_AUTODRIVE").is_some();
        let brain = (!is_player || autodrive).then(|| AiBrain::new(slot, ai::personality(slot)));
        let mut entity = commands.spawn((
            Racer {
                slot,
                is_player,
                car,
                name: spec.name,
                tuning,
                state,
                input: CarInput::default(),
                mods: CarModifiers::default(),
                lap: LapTracker::new(track.length(), progress, LAPS),
                drift_points: 0.0,
                brain,
            },
            // The root only carries position; the stacked slices are children.
            Transform::from_xyz(pos.x, pos.y, car_z(slot, pos.y.round())),
            Visibility::default(),
            WORLD_LAYER,
            DespawnOnExit(AppState::Race),
        ));
        let root = entity.id();
        if is_player {
            entity.insert(Loadout {
                gadget: profile.data.equipped_gadget,
                cooldown: 0.0,
                active: 0.0,
                item: None,
                last_item: None,
            });
            camera.translation = pos.extend(camera.translation.z);
        }
        let pose = StackPose { heading: dir.to_angle(), alpha: 1.0 };
        spawn_stack(&mut commands, root, &art, pose);
    }

    commands.insert_resource(CurrentTrack { id, track });
    commands.insert_resource(RaceSession {
        mode: *mode,
        phase: Phase::Countdown,
        clock: -COUNTDOWN,
        finish_timer: 0.0,
        wide_camera: false,
    });
}

fn silence_engine(mut engine: ResMut<EngineAudio>) {
    engine.active = false;
}

fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut memory: ResMut<SteerMemory>,
    session: Res<RaceSession>,
    mut racers: Query<&mut Racer>,
) {
    let held = |a: KeyCode, b: KeyCode| keys.pressed(a) || keys.pressed(b);
    let left = held(KeyCode::KeyA, KeyCode::ArrowLeft);
    let right = held(KeyCode::KeyD, KeyCode::ArrowRight);
    if keys.just_pressed(KeyCode::KeyA) || keys.just_pressed(KeyCode::ArrowLeft) {
        memory.last = 1.0;
    }
    if keys.just_pressed(KeyCode::KeyD) || keys.just_pressed(KeyCode::ArrowRight) {
        memory.last = -1.0;
    }
    // When both are held, the most recently pressed direction wins.
    let steer = match (left, right) {
        (true, true) => memory.last,
        (true, false) => 1.0,
        (false, true) => -1.0,
        _ => 0.0,
    };
    let input = CarInput {
        throttle: if held(KeyCode::KeyW, KeyCode::ArrowUp) { 1.0 } else { 0.0 },
        brake: if held(KeyCode::KeyS, KeyCode::ArrowDown) { 1.0 } else { 0.0 },
        steer,
        handbrake: keys.pressed(KeyCode::Space),
        nitro: held(KeyCode::ShiftLeft, KeyCode::ShiftRight),
    };
    let racing = session.phase == Phase::Running;
    for mut r in &mut racers {
        if r.is_player && r.brain.is_none() {
            r.input = if r.lap.finished_at.is_some() {
                // Coast after the flag.
                CarInput { throttle: 0.3, ..CarInput::default() }
            } else if racing {
                input
            } else {
                CarInput::default()
            };
        }
    }
}

fn ai_input(track: Res<CurrentTrack>, session: Res<RaceSession>, mut racers: Query<&mut Racer>) {
    let snapshot: Vec<(usize, Vec2, Vec2, f32)> = racers
        .iter()
        .map(|r| (r.slot, r.state.pos, r.state.vel, r.lap.total))
        .collect();
    let player_total = racers.iter().find(|r| r.is_player).map(|r| r.lap.total);
    let racing = session.phase == Phase::Running;
    for mut r in &mut racers {
        let Some(mut brain) = r.brain.take() else { continue };
        r.input = if racing {
            let others: Vec<(Vec2, Vec2)> = snapshot
                .iter()
                .filter(|o| o.0 != r.slot)
                .map(|o| (o.1, o.2))
                .collect();
            let lead = player_total.map_or(0.0, |p| (r.lap.total - p) / track.track.length());
            brain.drive(&track.track, &r.state, &r.tuning, &others, lead)
        } else {
            CarInput::default()
        };
        r.brain = Some(brain);
    }
}

fn simulate(
    track: Res<CurrentTrack>,
    session: Res<RaceSession>,
    mut racers: Query<&mut Racer>,
    mut sfx: MessageWriter<PlaySfx>,
    mut toasts: MessageWriter<Toast>,
    mut steps: MessageWriter<PlayerStep>,
) {
    let t = &track.track;
    let mut list: Vec<Mut<Racer>> = racers.iter_mut().collect();
    for r in list.iter_mut() {
        let r = &mut **r;
        let ev = car::step(&mut r.state, r.input, &r.tuning, r.mods, t);
        r.drift_points += ev.drift_points_banked;
        if r.is_player {
            steps.write(PlayerStep(ev));
            if ev.gear_changed {
                sfx.write(PlaySfx(Sfx::GearShift));
            }
            if let Some(tier) = ev.drift_boost {
                sfx.write(PlaySfx(Sfx::BoostPop { tier }));
                toasts.write(Toast(format!("DRIFT BOOST {}", "I".repeat(tier as usize))));
            }
            if ev.nitro_fired {
                sfx.write(PlaySfx(Sfx::NitroIgnite));
            }
            if ev.pad_boost {
                sfx.write(PlaySfx(Sfx::PadBoost));
                toasts.write(Toast("PAD BOOST".into()));
            }
            if let Some(impact) = ev.wall_hit {
                sfx.write(PlaySfx(Sfx::Bump { strength: (impact / 300.0).min(1.0) }));
            }
        }
    }

    // Car-to-car contacts.
    for i in 0..list.len() {
        for j in (i + 1)..list.len() {
            if list[i].mods.phantom || list[j].mods.phantom {
                continue;
            }
            let (a, b) = list.split_at_mut(j);
            let (ra, rb) = (&mut *a[i], &mut *b[0]);
            let ram = [ra.mods.ramming, rb.mods.ramming];
            if let Some(contact) = car::collide(&mut ra.state, &mut rb.state, ram) {
                let involves_player = ra.is_player || rb.is_player;
                if involves_player && contact.impact > 30.0 {
                    sfx.write(PlaySfx(Sfx::Bump { strength: (contact.impact / 250.0).min(1.0) }));
                }
                if let Some(victim) = contact.spun {
                    let (victim, other) = if victim == 0 { (&*ra, &mut *rb) } else { (&*rb, &mut *ra) };
                    if other.is_player && !victim.is_player {
                        other.drift_points += 150.0;
                        toasts.write(Toast("SPINOUT! +150".into()));
                    }
                    if involves_player {
                        sfx.write(PlaySfx(Sfx::Spinout));
                    }
                }
            }
        }
    }

    // Laps.
    if session.phase == Phase::Countdown {
        return;
    }
    for r in list.iter_mut() {
        let progress = t.query(r.state.pos).progress;
        let is_player = r.is_player;
        match r.lap.update(t.length(), progress, session.clock) {
            Some(LapEvent::Sector { .. }) if is_player => {
                sfx.write(PlaySfx(Sfx::SectorChime));
            }
            Some(LapEvent::Lap { .. }) if is_player => {
                sfx.write(PlaySfx(Sfx::LapComplete));
                toasts.write(Toast(format!("LAP {}", r.lap.display_lap())));
            }
            Some(LapEvent::Finish { .. }) if is_player => {
                sfx.write(PlaySfx(Sfx::FinishSlam));
            }
            _ => {}
        }
    }
}

fn advance_clock(
    mut commands: Commands,
    mut session: ResMut<RaceSession>,
    track: Res<CurrentTrack>,
    racers: Query<&Racer>,
    mut sfx: MessageWriter<PlaySfx>,
    mut next: ResMut<NextState<AppState>>,
) {
    let before = session.clock;
    session.clock += DT;
    match session.phase {
        Phase::Countdown => {
            // Ticks at -2.4, -1.6, -0.8; green at 0.
            for mark in [-COUNTDOWN, -COUNTDOWN * 2.0 / 3.0, -COUNTDOWN / 3.0] {
                if before <= mark + 1e-4 && session.clock > mark + 1e-4 {
                    sfx.write(PlaySfx(Sfx::CountdownTick));
                }
            }
            if session.clock >= 0.0 {
                session.phase = Phase::Running;
                session.clock = 0.0;
                sfx.write(PlaySfx(Sfx::CountdownGo));
            }
        }
        Phase::Running => {
            if racers.iter().any(|r| r.is_player && r.lap.finished_at.is_some()) {
                session.phase = Phase::Finished;
                session.finish_timer = 0.0;
            }
        }
        Phase::Finished => {
            session.finish_timer += DT;
            if session.finish_timer >= FINISH_HOLD {
                let list: Vec<&Racer> = racers.iter().collect();
                if let Some(o) = snapshot(session.mode, track.id, &list) {
                    commands.insert_resource(o);
                }
                next.set(AppState::Results);
            }
        }
    }
}

fn race_keys(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut session: ResMut<RaceSession>,
    track: Res<CurrentTrack>,
    racers: Query<&Racer>,
    mut next: ResMut<NextState<AppState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        next.set(AppState::Title);
    } else if keys.just_pressed(KeyCode::KeyR) {
        // Leaving and re-entering the state rebuilds the race.
        next.set(AppState::Race);
    } else if keys.just_pressed(KeyCode::KeyV) {
        session.wide_camera = !session.wide_camera;
    } else if session.phase == Phase::Finished
        && (keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space))
    {
        let list: Vec<&Racer> = racers.iter().collect();
        if let Some(o) = snapshot(session.mode, track.id, &list) {
            commands.insert_resource(o);
        }
        // Prevent a second snapshot from the finish timer this frame.
        session.finish_timer = f32::NEG_INFINITY;
        next.set(AppState::Results);
    }
}

/// World z of the car in `slot` standing at ground `y`: y-sorted with the
/// props and lucky boxes through the shared [`ground_z`]. Cars on the same row
/// keep slot order; each car is one baked sprite, so stacks never interleave.
fn car_z(slot: usize, y: f32) -> f32 {
    ground_z(CAR_SORT + slot as f32 * SORT_STEP / 8.0, y)
}

fn sync_visuals(mut q: Query<(&Racer, &mut Transform, &mut StackPose)>) {
    for (r, mut tf, mut pose) in &mut q {
        let y = r.state.pos.y.round();
        tf.translation.x = r.state.pos.x.round();
        tf.translation.y = y;
        tf.translation.z = car_z(r.slot, y);
        pose.heading = r.state.heading;
    }
}

fn follow_camera(
    time: Res<Time>,
    session: Res<RaceSession>,
    racers: Query<&Racer>,
    mut cam: Single<(&mut Transform, &mut Projection), With<WorldCamera>>,
    mut look: Local<Vec2>,
) {
    let Some(player) = racers.iter().find(|r| r.is_player) else { return };
    let dt = time.delta_secs();
    let (tf, proj) = &mut *cam;
    let cap = if session.wide_camera { 92.0 } else { 72.0 };
    let mut target_look = (player.state.vel * 0.24).clamp_length_max(cap);
    if player.state.nitro_active {
        target_look *= 1.25;
    }
    let cur_look = *look;
    *look = cur_look + (target_look - cur_look) * (4.5 * dt).min(1.0);
    let target = player.state.pos + *look;
    let cur = tf.translation.truncate();
    let next = cur + (target - cur) * (7.5 * dt).min(1.0);
    tf.translation.x = next.x;
    tf.translation.y = next.y;
    if let Projection::Orthographic(p) = &mut **proj {
        let intro = if session.phase == Phase::Countdown {
            // Ease in from a wider shot during the countdown.
            1.0 + 0.9 * (-session.clock / COUNTDOWN).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let base = if session.wide_camera { 1.4 } else { 1.0 };
        p.scale = base * intro;
    }
}

fn engine_audio(session: Res<RaceSession>, racers: Query<&Racer>, mut engine: ResMut<EngineAudio>) {
    let Some(p) = racers.iter().find(|r| r.is_player) else { return };
    engine.active = true;
    engine.profile = p.car.index();
    engine.rpm = if session.phase == Phase::Countdown {
        // Free revving on the grid.
        if p.input.throttle > 0.0 { 0.7 } else { 0.1 }
    } else {
        p.state.rpm
    };
    engine.throttle = p.input.throttle;
    engine.boost = if p.state.is_boosting() { 1.0 } else { 0.0 };
}

/// Final classification, available to the results screen.
pub fn classification(racers: &[&Racer]) -> Vec<usize> {
    let trackers: Vec<&LapTracker> = racers.iter().map(|r| &r.lap).collect();
    standings(&trackers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_cars_draw_in_front() {
        let ys = [(0, 10.0), (1, -5.0), (2, 30.0), (3, 10.0)];
        let mut order: Vec<usize> = (0..4).collect();
        order.sort_by(|&a, &b| car_z(ys[a].0, ys[a].1).total_cmp(&car_z(ys[b].0, ys[b].1)));
        // Highest on screen (slot 2) is drawn first, lowest (slot 1) last; ties keep slot order.
        assert_eq!(order, vec![2, 0, 3, 1]);
        // Any car one row lower is in front of every car on the row above.
        assert!(car_z(0, 9.0) > car_z(3, 10.0));
    }
}
