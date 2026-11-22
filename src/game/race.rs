//! Race session: setup, fixed-step simulation, laps, countdown and finish.

use bevy::prelude::*;

use crate::core::ai::{self, AiBrain};
use crate::core::car::{self, CarInput, CarModifiers, CarState, CarTuning, DT};
use crate::core::cars::{CarId, car_spec};
use crate::core::economy::{BoxItem, GadgetId, PaintId};
use crate::core::race::{FIELD_SIZE, LapEvent, LapTracker, PLAYER_GRID_SLOT, contact_spins, standings};
use crate::core::settings::Action;
use crate::core::track::Track;
use crate::core::tracks::{LAPS, TrackId, track_def};

use super::audio::{EngineAudio, PlaySfx, Sfx};
use super::confirm::{Confirm, ConfirmDialog, Reply};
use super::controls::Controls;
use super::pixel::{WORLD_LAYER, WorldCamera};
use super::profile::Profile;
use super::stack::{CAR_SORT, SORT_STEP, StackArt, StackPose, car_slices, ground_z, spawn_stack};
use super::theme::world_theme;
use super::track_view::{grid_lateral, grid_slot, spawn_track_view};
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
    pub name: String,
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
    pub name: String,
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
                name: racers[i].name.clone(),
                is_player: racers[i].is_player,
                finished_at: racers[i].lap.finished_at,
            })
            .collect(),
        place: order.iter().position(|&i| racers[i].is_player).map_or(FIELD_SIZE, |p| p + 1) as u8,
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
    let player = Entrant {
        car: player_car,
        paint: profile.data.paint_for(player_car),
        name: car_spec(player_car).name.to_string(),
        is_player: true,
    };
    let field = if *mode == RaceMode::Race { race_field(player, id.index()) } else { vec![player] };

    for (slot, entrant) in field.into_iter().enumerate() {
        let Entrant { car, paint, name, is_player } = entrant;
        let spec = car_spec(car);
        let art = StackArt::new(&car_slices(car, paint), &mut images);
        let (pos, dir) = grid_slot(&track, slot);
        let state = CarState::new(pos, dir.to_angle());
        let progress = track.query(pos).progress;
        let tuning = if is_player { profile.data.tuning_for(car) } else { spec.tuning };
        // `HAIRPIN_AUTODRIVE=1` lets the AI drive the player car (attract mode, testing).
        let autodrive = std::env::var_os("HAIRPIN_AUTODRIVE").is_some();
        let brain = (!is_player || autodrive).then(|| AiBrain::new(slot, ai::personality(slot), grid_lateral(slot)));
        let mut entity = commands.spawn((
            Racer {
                slot,
                is_player,
                car,
                name,
                tuning,
                state,
                input: CarInput::default(),
                mods: CarModifiers { spin_recovery: is_player, ..default() },
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
        wide_camera: profile.data.settings.wide_camera,
    });
}

fn silence_engine(mut engine: ResMut<EngineAudio>) {
    engine.active = false;
}

fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    controls: Res<Controls>,
    mut memory: ResMut<SteerMemory>,
    session: Res<RaceSession>,
    mut racers: Query<&mut Racer>,
) {
    let held = |a: Action| controls.pressed(&keys, a);
    let left = held(Action::SteerLeft);
    let right = held(Action::SteerRight);
    if controls.just_pressed(&keys, Action::SteerLeft) {
        memory.last = 1.0;
    }
    if controls.just_pressed(&keys, Action::SteerRight) {
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
        throttle: if held(Action::Throttle) { 1.0 } else { 0.0 },
        brake: if held(Action::Brake) { 1.0 } else { 0.0 },
        steer,
        handbrake: held(Action::Drift),
        nitro: held(Action::Nitro),
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
    let spins = contact_spins(session.clock);
    for i in 0..list.len() {
        for j in (i + 1)..list.len() {
            if list[i].mods.phantom || list[j].mods.phantom {
                continue;
            }
            let (a, b) = list.split_at_mut(j);
            let (ra, rb) = (&mut *a[i], &mut *b[0]);
            let ram = [ra.mods.ramming, rb.mods.ramming];
            if let Some(contact) = car::collide(&mut ra.state, &mut rb.state, ram, spins) {
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

/// Race screen keys. Restarting and quitting ask first; the open dialog takes every key.
#[allow(clippy::too_many_arguments)]
pub fn race_keys(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut session: ResMut<RaceSession>,
    track: Res<CurrentTrack>,
    racers: Query<&Racer>,
    dialog: Option<ResMut<ConfirmDialog>>,
    mut next: ResMut<NextState<AppState>>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if let Some(mut dialog) = dialog {
        let Some(reply) = ConfirmDialog::input(&mut dialog, &keys) else { return };
        if reply == Reply::Accepted {
            next.set(dialog.action.target());
        }
        if reply != Reply::Moved {
            commands.remove_resource::<ConfirmDialog>();
        }
        sfx.write(PlaySfx(if reply == Reply::Accepted { Sfx::MenuSelect } else { Sfx::MenuMove }));
    } else if let Some(action) = Confirm::requested(&keys) {
        commands.insert_resource(ConfirmDialog::new(action));
        sfx.write(PlaySfx(Sfx::MenuMove));
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

/// One car on the starting grid.
struct Entrant {
    car: CarId,
    paint: PaintId,
    name: String,
    is_player: bool,
}

/// The race grid: `FIELD_SIZE - 1` rivals around the player at
/// [`PLAYER_GRID_SLOT`]. Rivals use the other cars, rotated per track so each
/// circuit has a different field; there are fewer cars than rivals, so a car's
/// repeat entries get another paint scheme and a numbered name.
fn race_field(player: Entrant, track_index: usize) -> Vec<Entrant> {
    const SUFFIXES: [&str; 4] = ["", " II", " III", " IV"];
    let player_car = player.car;
    let mut seen: Vec<CarId> = Vec::new();
    let mut field: Vec<Entrant> = CarId::ALL
        .iter()
        .copied()
        .filter(|&c| c != player_car)
        .cycle()
        .skip(track_index * 2)
        .take(FIELD_SIZE - 1)
        .enumerate()
        .map(|(i, car)| {
            let repeat = seen.iter().filter(|&&c| c == car).count();
            seen.push(car);
            let paint = if repeat == 0 { PaintId::Factory } else { PaintId::ALL[1 + i % (PaintId::ALL.len() - 1)] };
            let suffix = SUFFIXES[repeat.min(SUFFIXES.len() - 1)];
            Entrant { car, paint, name: format!("{}{suffix}", car_spec(car).name), is_player: false }
        })
        .collect();
    field.insert(PLAYER_GRID_SLOT.min(field.len()), player);
    field
}

/// World z of the car in `slot` standing at ground `y`: y-sorted with the
/// props and lucky boxes through the shared [`ground_z`]. Cars on the same row
/// keep slot order; each car is one baked sprite, so stacks never interleave.
fn car_z(slot: usize, y: f32) -> f32 {
    ground_z(CAR_SORT + slot as f32 * CAR_SLOT_STEP, y)
}

/// z between grid slots on one ground row. The whole field (plus a flame's
/// `ATTACH_Z`) stays below `PROP_SORT`, so props on the row still draw in front.
const CAR_SLOT_STEP: f32 = SORT_STEP / 32.0;

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

fn engine_audio(
    session: Res<RaceSession>,
    racers: Query<&Racer>,
    dialog: Option<Res<ConfirmDialog>>,
    mut engine: ResMut<EngineAudio>,
) {
    let Some(p) = racers.iter().find(|r| r.is_player) else { return };
    engine.active = dialog.is_none();
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

    #[test]
    fn the_whole_field_sorts_below_props_on_its_row() {
        use super::super::stack::{ATTACH_Z, PROP_SORT};
        let last = car_z(FIELD_SIZE - 1, 7.0);
        assert!(last + ATTACH_Z < ground_z(PROP_SORT, 7.0));
        assert!(car_z(0, 6.0) > last, "one row lower still draws in front");
    }

    fn player() -> Entrant {
        Entrant { car: CarId::ALL[0], paint: PaintId::Factory, name: "You".into(), is_player: true }
    }

    #[test]
    fn race_field_fills_the_grid_around_the_player() {
        for track in 0..TrackId::ALL.len() {
            let field = race_field(player(), track);
            assert_eq!(field.len(), FIELD_SIZE);
            assert!(field[PLAYER_GRID_SLOT].is_player);
            assert_eq!(field.iter().filter(|e| e.is_player).count(), 1);
            assert!(field.iter().filter(|e| !e.is_player).all(|e| e.car != CarId::ALL[0]));
        }
    }

    #[test]
    fn repeated_rivals_look_and_read_different() {
        let field = race_field(player(), 0);
        for (i, a) in field.iter().enumerate() {
            for b in &field[i + 1..] {
                assert_ne!(a.name, b.name);
                if a.car == b.car {
                    assert_ne!(a.paint, b.paint);
                }
            }
        }
    }

    fn press(app: &mut App, key: KeyCode) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        keys.press(key);
        app.update();
    }

    fn keys_app(phase: Phase) -> App {
        let mut app = App::new();
        let id = TrackId::ALL[0];
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<NextState<AppState>>()
            .insert_resource(CurrentTrack { id, track: track_def(id).build() })
            .insert_resource(RaceSession {
                mode: RaceMode::Race,
                phase,
                clock: 1.0,
                finish_timer: 0.0,
                wide_camera: false,
            })
            .add_message::<PlaySfx>()
            .add_systems(Update, race_keys);
        app
    }

    fn pending(app: &App) -> Option<AppState> {
        match app.world().resource::<NextState<AppState>>() {
            NextState::Unchanged => None,
            NextState::Pending(s) | NextState::PendingIfDifferent(s) => Some(*s),
        }
    }

    fn dialog(app: &App) -> Option<Confirm> {
        app.world().get_resource::<ConfirmDialog>().map(|d| d.action)
    }

    #[test]
    fn restart_and_quit_ask_first_in_every_phase() {
        for phase in [Phase::Countdown, Phase::Running, Phase::Finished] {
            for (key, action, target) in [
                (KeyCode::KeyR, Confirm::Restart, AppState::Race),
                (KeyCode::Escape, Confirm::Quit, AppState::TrackSelect),
            ] {
                let mut app = keys_app(phase);
                press(&mut app, key);
                assert_eq!((dialog(&app), pending(&app)), (Some(action), None), "{phase:?} {key:?}");
                press(&mut app, KeyCode::ArrowDown);
                assert_eq!((dialog(&app), pending(&app)), (Some(action), None));
                press(&mut app, KeyCode::Enter);
                assert_eq!((dialog(&app), pending(&app)), (None, Some(target)), "{phase:?} {key:?}");
            }
        }
    }

    #[test]
    fn cancelling_keeps_the_race_and_swallows_the_key() {
        for phase in [Phase::Running, Phase::Finished] {
            for cancel in [KeyCode::Escape, KeyCode::Enter] {
                let mut app = keys_app(phase);
                press(&mut app, KeyCode::KeyR);
                press(&mut app, cancel);
                assert_eq!((dialog(&app), pending(&app)), (None, None), "{phase:?} {cancel:?}");
            }
        }
    }

    #[test]
    fn other_race_keys_wait_for_the_dialog() {
        let mut app = keys_app(Phase::Running);
        press(&mut app, KeyCode::Escape);
        press(&mut app, KeyCode::KeyV);
        press(&mut app, KeyCode::KeyR);
        assert!(!app.world().resource::<RaceSession>().wide_camera);
        assert_eq!(dialog(&app), Some(Confirm::Quit));
    }

    /// Spins per race start, averaged over every track and slightly jittered
    /// grids: `[first 3 s, 3 to 8 s]`.
    fn start_spins(trials: u32) -> [f32; 2] {
        let mut spins = [0u32; 2];
        for id in TrackId::ALL {
            let track = track_def(id).build();
            for trial in 0..trials {
                let mut rng = 0x1357 + trial * 7919;
                let mut jitter = || (crate::core::items::next_rand(&mut rng) % 1000) as f32 / 1000.0 * 3.0 - 1.5;
                let mut cars: Vec<_> = race_field(player(), id.index())
                    .into_iter()
                    .enumerate()
                    .map(|(slot, e)| {
                        let (p, d) = grid_slot(&track, slot);
                        let state = CarState::new(p + Vec2::new(jitter(), jitter()), d.to_angle());
                        (state, AiBrain::new(slot, ai::personality(slot), grid_lateral(slot)), car_spec(e.car).tuning)
                    })
                    .collect();
                for step in 0..(8.0 / DT) as usize {
                    let clock = step as f32 * DT;
                    let seen: Vec<(Vec2, Vec2)> = cars.iter().map(|c| (c.0.pos, c.0.vel)).collect();
                    for (i, (state, brain, tuning)) in cars.iter_mut().enumerate() {
                        let mut others = seen.clone();
                        others.remove(i);
                        let input = brain.drive(&track, state, tuning, &others, 0.0);
                        car::step(state, input, tuning, CarModifiers::default(), &track);
                    }
                    for j in 1..cars.len() {
                        let (a, b) = cars.split_at_mut(j);
                        for other in a {
                            let hit = car::collide(&mut other.0, &mut b[0].0, [false; 2], contact_spins(clock));
                            if hit.is_some_and(|c| c.spun.is_some()) {
                                spins[usize::from(clock >= 3.0)] += 1;
                            }
                        }
                    }
                }
            }
        }
        spins.map(|n| n as f32 / (trials as f32 * TrackId::ALL.len() as f32))
    }

    /// Without the lane hold, following and spin grace this measured 7.9 and 7.7.
    #[test]
    fn the_field_gets_away_cleanly() {
        let [grace, after] = start_spins(6);
        assert_eq!(grace, 0.0);
        assert!(after < 4.0, "{after:.1} spins per start after the grace period");
    }
}
