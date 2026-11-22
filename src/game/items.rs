//! Lucky boxes, box items and gadgets: the Bevy adapter over `core::items`.
//!
//! Only the player's racer (the one carrying a `Loadout`) collects boxes and
//! fires gadgets, but every effect can hit any racer.

use std::f32::consts::TAU;

use bevy::prelude::*;

use crate::core::car::{CarModifiers, DT};
use crate::core::economy::{BoxItem, GadgetId, roll_box_item};
use crate::core::items::{self as rules, Hit, Slick};
use crate::core::settings::Action as Bind;
use crate::core::tracks::track_def;

use super::audio::{PlaySfx, Sfx};
use super::confirm::no_dialog;
use super::controls::Controls;
use super::meshes::MeshBuilder;
use super::pixel::WORLD_LAYER;
use super::props::{BakeCache, Model, PropColors};
use super::race::{CurrentTrack, Loadout, Phase, RaceSession, RaceSet, Racer, Toast};
use super::stack::{BOX_SORT, SHADOW_OFFSET, SHADOW_Z, StackPose, frame_angle, frame_index, ground_z};
use super::theme::world_theme;
use super::{AppState, RaceMode};

/// Spin frames baked per race for the lucky-box crate.
const BOX_FRAMES: usize = 32;
/// Crate spin speed in turns per second.
const BOX_SPIN: f32 = 0.25;
/// Mean hover height of the crate above its ground point, and the bob amplitude, in px.
const BOX_HOVER: f32 = 4.0;
const BOX_BOB: f32 = 2.0;
const BOX_SHADOW_ALPHA: f32 = 0.32;
const SLICK_Z: f32 = 6.0;
const RING_Z: f32 = 30.0;
const RING_LIFE: f32 = 0.45;
const PHANTOM_ALPHA: f32 = 0.45;

pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ItemsState>()
            .add_systems(Startup, init_assets)
            .add_systems(OnEnter(AppState::Race), reset_state)
            .add_systems(
                FixedUpdate,
                use_gadget_or_item
                    .after(RaceSet::Input)
                    .before(RaceSet::Simulate)
                    .run_if(in_state(AppState::Race)),
            )
            .add_systems(
                FixedUpdate,
                (tick_loadout, ram_contacts, slick_hits, box_pickups)
                    .chain()
                    .after(RaceSet::Simulate)
                    .before(RaceSet::Post)
                    .run_if(in_state(AppState::Race)),
            )
            .add_systems(
                Update,
                (queue_use.run_if(no_dialog), spawn_boxes, animate_boxes, animate_rings, phantom_fade)
                    .run_if(in_state(AppState::Race)),
            );
    }
}

// ---------------------------------------------------------------------------
// State and components
// ---------------------------------------------------------------------------

/// Per-race item bookkeeping; reset on every race start.
#[derive(Resource, Default)]
struct ItemsState {
    boxes_spawned: bool,
    /// A use key was pressed since the last fixed step.
    use_pending: bool,
    /// Player position at the end of the previous fixed step (swept pickups).
    prev_pos: Option<Vec2>,
    /// Lap index at which the player last collected each station.
    collected: Vec<i32>,
    slicks: Vec<Slick>,
    /// Visual entity of each slick, parallel to `slicks`.
    slick_entities: Vec<Entity>,
    /// Racer slots already rammed during the current battering boost.
    ram_hits: Vec<usize>,
    rng: u32,
}

/// Mesh and material shared by all slicks.
#[derive(Resource)]
struct ItemAssets {
    material: Handle<ColorMaterial>,
    slick_mesh: Handle<Mesh>,
}

#[derive(Component)]
struct LuckyBox {
    station: usize,
    pos: Vec2,
    /// Seconds until the box reappears (0 = available).
    hidden: f32,
}

/// The spinning, bobbing crate sprite of a `LuckyBox`. The baked spin frames
/// and shadows are strong handles, freed when the race's boxes despawn.
#[derive(Component)]
struct BoxCrate {
    phase: f32,
    frames: Vec<Handle<Image>>,
    shadows: Vec<Handle<Image>>,
    /// The crate's ground shadow sprite.
    shadow: Entity,
}

/// Ground shadow of a lucky box (stays on the ground while the crate bobs).
#[derive(Component)]
struct BoxShadow;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RingKind {
    Thumper,
    Static,
}

/// Expanding shockwave visual.
#[derive(Component)]
struct Ring {
    kind: RingKind,
    age: f32,
    radius: f32,
    mesh: Handle<Mesh>,
    material: Handle<ColorMaterial>,
}

/// What firing a gadget or item produced, flushed by the calling system.
#[derive(Default)]
struct Feedback {
    sfx: Vec<Sfx>,
    toasts: Vec<String>,
    rings: Vec<(Vec2, f32, RingKind)>,
    slicks: Vec<(Vec2, usize)>,
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

fn init_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.insert_resource(ItemAssets {
        material: materials.add(ColorMaterial::default()),
        slick_mesh: meshes.add(slick_mesh()),
    });
}

fn reset_state(mut state: ResMut<ItemsState>) {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0x5eed);
    *state = ItemsState { rng: seed | 1, ..default() };
}

fn spawn_boxes(
    mut commands: Commands,
    mut state: ResMut<ItemsState>,
    mut images: ResMut<Assets<Image>>,
    track: Option<Res<CurrentTrack>>,
    session: Option<Res<RaceSession>>,
) {
    if state.boxes_spawned {
        return;
    }
    let (Some(track), Some(session)) = (track, session) else { return };
    state.boxes_spawned = true;
    if session.mode != RaceMode::Race {
        return;
    }
    // One spin bake per race, shared by every box.
    let mut cache = BakeCache::new(PropColors::from_theme(&world_theme(track.id)));
    let (frames, shadows): (Vec<_>, Vec<_>) = (0..BOX_FRAMES)
        .map(|i| {
            let baked = cache.get(Model::Crate, 0, frame_angle(i, BOX_FRAMES), &mut images);
            (baked.image, baked.shadow)
        })
        .unzip();
    let stations = track_def(track.id).box_stations;
    state.collected = vec![i32::MIN; stations.len()];
    let len = track.track.length();
    for (station, frac) in stations.iter().enumerate() {
        let (center, tangent) = track.track.main.sample_at(frac * len);
        for (side, pos) in rules::station_boxes(center, tangent).into_iter().enumerate() {
            let ground = pos.round();
            let z = ground_z(BOX_SORT, ground.y);
            let root = commands
                .spawn((
                    LuckyBox { station, pos, hidden: 0.0 },
                    Transform::from_translation(ground.extend(z)),
                    Visibility::Visible,
                    WORLD_LAYER,
                    DespawnOnExit(AppState::Race),
                ))
                .id();
            // The shadow sits on the shared ground shadow layer, below every car.
            let shadow = commands
                .spawn((
                    BoxShadow,
                    Sprite { image: shadows[0].clone(), color: Color::BLACK.with_alpha(BOX_SHADOW_ALPHA), ..default() },
                    Transform::from_translation(SHADOW_OFFSET.extend(SHADOW_Z - z)),
                    WORLD_LAYER,
                    ChildOf(root),
                ))
                .id();
            commands.spawn((
                BoxCrate {
                    phase: station as f32 * 1.7 + side as f32 * 0.9,
                    frames: frames.clone(),
                    shadows: shadows.clone(),
                    shadow,
                },
                Sprite::from_image(frames[0].clone()),
                Transform::from_xyz(0.0, BOX_HOVER, 0.0),
                WORLD_LAYER,
                ChildOf(root),
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Use (gadget or held item)
// ---------------------------------------------------------------------------

fn queue_use(keys: Res<ButtonInput<KeyCode>>, controls: Res<Controls>, mut state: ResMut<ItemsState>) {
    if controls.just_pressed(&keys, Bind::UseItem) {
        state.use_pending = true;
    }
}

enum Action {
    Item(BoxItem),
    Gadget(GadgetId),
}

#[allow(clippy::too_many_arguments)]
fn use_gadget_or_item(
    mut commands: Commands,
    mut state: ResMut<ItemsState>,
    assets: Res<ItemAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    session: Res<RaceSession>,
    mut racers: Query<(&mut Racer, Option<&mut Loadout>)>,
    mut sfx: MessageWriter<PlaySfx>,
    mut toasts: MessageWriter<Toast>,
) {
    // Gadgets are off in Time Trial so ghost lap times stay comparable.
    if !std::mem::take(&mut state.use_pending)
        || session.phase != Phase::Running
        || session.mode != RaceMode::Race
    {
        return;
    }
    let mut list: Vec<_> = racers.iter_mut().collect();
    let Some(pi) = list.iter().position(|(_, l)| l.is_some()) else { return };
    let action = {
        let Some(lo) = list[pi].1.as_mut() else { return };
        if let Some(item) = lo.item.take() {
            Action::Item(item)
        } else if lo.cooldown <= 0.0 {
            lo.cooldown = lo.gadget.cooldown();
            lo.active = lo.gadget.active();
            Action::Gadget(lo.gadget)
        } else {
            return;
        }
    };
    let mut rs: Vec<Mut<Racer>> = list.into_iter().map(|(r, _)| r).collect();
    let mut fb = Feedback::default();
    match action {
        Action::Item(BoxItem::Thumper) | Action::Gadget(GadgetId::Thumper) => fire_thumper(&mut rs, pi, &mut fb),
        Action::Item(BoxItem::Slick) | Action::Gadget(GadgetId::Slick) => {
            fb.slicks.push((rules::slick_drop_pos(&rs[pi].state), rs[pi].slot));
            fb.sfx.push(Sfx::Slick);
        }
        Action::Item(BoxItem::Jammer) => fire_jammer(&mut rs, pi, &mut fb),
        Action::Gadget(GadgetId::StaticBurst) => fire_static(&mut rs, pi, &mut fb),
        Action::Gadget(GadgetId::BatteringBoost) => {
            let user = &mut *rs[pi];
            rules::battering_start(&mut user.state);
            user.mods.ramming = true;
            state.ram_hits.clear();
            fb.sfx.push(Sfx::BatteringBoost);
        }
        Action::Gadget(GadgetId::AllTerrain) => {
            let user = &mut *rs[pi];
            rules::all_terrain_start(&mut user.state);
            user.mods.all_terrain = true;
            fb.sfx.push(Sfx::AllTerrain);
        }
        Action::Gadget(GadgetId::Phantom) => {
            rs[pi].mods.phantom = true;
            fb.sfx.push(Sfx::Phantom);
        }
        Action::Gadget(GadgetId::DriftSurge) => {
            rs[pi].mods.drift_surge = true;
            fb.sfx.push(Sfx::DriftSurge);
        }
    }

    for s in fb.sfx {
        sfx.write(PlaySfx(s));
    }
    for t in fb.toasts {
        toasts.write(Toast(t));
    }
    for (pos, radius, kind) in fb.rings {
        let mesh = meshes.add(ring_mesh(4.0, kind, 0));
        let material = materials.add(ColorMaterial::default());
        commands.spawn((
            Ring { kind, age: 0.0, radius, mesh: mesh.clone(), material: material.clone() },
            Mesh2d(mesh),
            MeshMaterial2d(material),
            Transform::from_xyz(pos.x.round(), pos.y.round(), RING_Z),
            WORLD_LAYER,
            DespawnOnExit(AppState::Race),
        ));
    }
    for (pos, owner) in fb.slicks {
        drop_slick(&mut commands, &mut state, &assets, pos, owner);
    }
}

fn others(len: usize, pi: usize) -> impl Iterator<Item = usize> {
    (0..len).filter(move |&j| j != pi)
}

/// Credits the player for rivals its effects spun out.
fn award_spinouts(player: &mut Racer, count: usize, fb: &mut Feedback) {
    if count == 0 {
        return;
    }
    let points = rules::SPINOUT_POINTS * count as f32;
    player.drift_points += points;
    fb.toasts.push(if count == 1 {
        format!("SPINOUT! +{points:.0}")
    } else {
        format!("SPINOUT x{count}! +{points:.0}")
    });
}

fn fire_thumper(rs: &mut [Mut<Racer>], pi: usize, fb: &mut Feedback) {
    let pos = rs[pi].state.pos;
    let mut spun = 0;
    for j in others(rs.len(), pi) {
        if rules::thumper_hit(pos, &mut rs[j].state) == Hit::SpunOut {
            spun += 1;
        }
    }
    rules::thumper_self(&mut rs[pi].state);
    fb.sfx.push(Sfx::Thumper);
    fb.rings.push((pos, rules::THUMPER_RADIUS, RingKind::Thumper));
    if spun > 0 {
        fb.sfx.push(Sfx::Spinout);
    }
    award_spinouts(&mut rs[pi], spun, fb);
}

fn fire_static(rs: &mut [Mut<Racer>], pi: usize, fb: &mut Feedback) {
    let pos = rs[pi].state.pos;
    let hits = others(rs.len(), pi)
        .filter(|&j| rules::static_hit(pos, &mut rs[j].state).landed())
        .count();
    fb.sfx.push(Sfx::StaticBurst);
    fb.rings.push((pos, rules::STATIC_RADIUS, RingKind::Static));
    if hits > 0 {
        let points = rules::STATIC_POINTS * hits as f32;
        rs[pi].drift_points += points;
        fb.toasts.push(format!("ZAPPED x{hits} +{points:.0}"));
    }
}

fn fire_jammer(rs: &mut [Mut<Racer>], pi: usize, fb: &mut Feedback) {
    fb.sfx.push(Sfx::Jammer);
    let rivals: Vec<(usize, f32)> = others(rs.len(), pi).map(|j| (j, rs[j].lap.total)).collect();
    if let Some(j) = rules::jammer_target(rs[pi].lap.total, &rivals) {
        rules::jammer_hit(&mut rs[j].state);
        fb.toasts.push(format!("JAMMED {}", rs[j].name.to_uppercase()));
    }
}

fn drop_slick(commands: &mut Commands, state: &mut ItemsState, assets: &ItemAssets, pos: Vec2, owner: usize) {
    let spin = rules::next_rand(&mut state.rng) as f32 / u32::MAX as f32 * TAU;
    let entity = commands
        .spawn((
            Mesh2d(assets.slick_mesh.clone()),
            MeshMaterial2d(assets.material.clone()),
            Transform::from_xyz(pos.x.round(), pos.y.round(), SLICK_Z).with_rotation(Quat::from_rotation_z(spin)),
            WORLD_LAYER,
            DespawnOnExit(AppState::Race),
        ))
        .id();
    let removed = rules::push_slick(&mut state.slicks, Slick::new(pos, owner));
    state.slick_entities.push(entity);
    for e in state.slick_entities.drain(..removed) {
        commands.entity(e).despawn();
    }
}

// ---------------------------------------------------------------------------
// Post-physics: timers, rams, slicks, pickups
// ---------------------------------------------------------------------------

fn clear_gadget_flag(mods: &mut CarModifiers, gadget: GadgetId) {
    match gadget {
        GadgetId::BatteringBoost => mods.ramming = false,
        GadgetId::AllTerrain => mods.all_terrain = false,
        GadgetId::Phantom => mods.phantom = false,
        GadgetId::DriftSurge => mods.drift_surge = false,
        GadgetId::Thumper | GadgetId::Slick | GadgetId::StaticBurst => {}
    }
}

fn tick_loadout(mut q: Query<(&mut Racer, &mut Loadout)>) {
    for (mut r, mut lo) in &mut q {
        if lo.cooldown > 0.0 {
            lo.cooldown = (lo.cooldown - DT).max(0.0);
        }
        if lo.active > 0.0 {
            lo.active -= DT;
            if lo.active <= 0.0 {
                lo.active = 0.0;
                let gadget = lo.gadget;
                clear_gadget_flag(&mut r.mods, gadget);
            }
        }
    }
}

fn pair_mut<T>(v: &mut [T], i: usize, j: usize) -> (&mut T, &mut T) {
    debug_assert_ne!(i, j);
    if i < j {
        let (a, b) = v.split_at_mut(j);
        (&mut a[i], &mut b[0])
    } else {
        let (a, b) = v.split_at_mut(i);
        (&mut b[0], &mut a[j])
    }
}

fn ram_contacts(
    mut state: ResMut<ItemsState>,
    mut racers: Query<(&mut Racer, Option<&Loadout>)>,
    mut sfx: MessageWriter<PlaySfx>,
    mut toasts: MessageWriter<Toast>,
) {
    let mut list: Vec<_> = racers.iter_mut().collect();
    let Some(pi) = list
        .iter()
        .position(|(r, l)| r.mods.ramming && l.is_some_and(|l| l.active > 0.0))
    else {
        return;
    };
    let mut fb = Feedback::default();
    let mut spun = 0;
    for j in others(list.len(), pi) {
        let ((user, _), (target, _)) = pair_mut(&mut list, pi, j);
        if target.mods.phantom || state.ram_hits.contains(&target.slot) {
            continue;
        }
        match rules::ram_hit(&mut user.state, &mut target.state) {
            Hit::Miss => {}
            hit => {
                state.ram_hits.push(target.slot);
                fb.sfx.push(Sfx::Spinout);
                if hit == Hit::SpunOut {
                    spun += 1;
                }
            }
        }
    }
    award_spinouts(&mut list[pi].0, spun, &mut fb);
    for s in fb.sfx {
        sfx.write(PlaySfx(s));
    }
    for t in fb.toasts {
        toasts.write(Toast(t));
    }
}

fn slick_hits(
    mut commands: Commands,
    mut state: ResMut<ItemsState>,
    mut racers: Query<&mut Racer>,
    mut sfx: MessageWriter<PlaySfx>,
    mut toasts: MessageWriter<Toast>,
) {
    if state.slicks.is_empty() {
        return;
    }
    let mut list: Vec<Mut<Racer>> = racers.iter_mut().collect();
    // (owner slot, rival newly spun) pairs for scoring.
    let mut spun_by: Vec<usize> = Vec::new();
    let mut any_hit = false;
    for slick in state.slicks.iter_mut() {
        slick.age += DT;
        for r in list.iter_mut() {
            let r = &mut **r;
            let hit = slick.try_hit(r.slot, &mut r.state);
            any_hit |= hit.landed();
            if hit == Hit::SpunOut && !r.is_player {
                spun_by.push(slick.owner);
            }
        }
    }
    let mut fb = Feedback::default();
    if any_hit {
        fb.sfx.push(Sfx::Spinout);
    }
    if let Some(player) = list.iter_mut().find(|r| r.is_player) {
        let slot = player.slot;
        let count = spun_by.iter().filter(|&&o| o == slot).count();
        award_spinouts(player, count, &mut fb);
    }
    for s in fb.sfx {
        sfx.write(PlaySfx(s));
    }
    for t in fb.toasts {
        toasts.write(Toast(t));
    }

    // Drop expired slicks and their visuals.
    let st = &mut *state;
    let mut i = 0;
    while i < st.slicks.len() {
        if st.slicks[i].expired() {
            st.slicks.remove(i);
            commands.entity(st.slick_entities.remove(i)).despawn();
        } else {
            i += 1;
        }
    }
}

fn box_pickups(
    mut state: ResMut<ItemsState>,
    track: Res<CurrentTrack>,
    session: Res<RaceSession>,
    mut players: Query<(&Racer, &mut Loadout)>,
    mut boxes: Query<&mut LuckyBox>,
    mut sfx: MessageWriter<PlaySfx>,
    mut toasts: MessageWriter<Toast>,
) {
    for mut b in &mut boxes {
        if b.hidden > 0.0 {
            b.hidden = (b.hidden - DT).max(0.0);
        }
    }
    let st = &mut *state;
    for (r, mut lo) in &mut players {
        let cur = r.state.pos;
        let prev = st.prev_pos.replace(cur).unwrap_or(cur);
        if session.phase != Phase::Running || lo.item.is_some() {
            continue;
        }
        let lap = rules::lap_index(r.lap.total, track.track.length());
        for mut b in &mut boxes {
            let taken = st.collected.get(b.station).is_none_or(|&l| l == lap);
            if b.hidden > 0.0 || taken || !rules::segment_hits_circle(prev, cur, b.pos, rules::BOX_RADIUS) {
                continue;
            }
            b.hidden = rules::BOX_RESPAWN;
            st.collected[b.station] = lap;
            let item = roll_box_item(rules::next_rand(&mut st.rng), lo.last_item, lo.gadget);
            lo.item = Some(item);
            lo.last_item = Some(item);
            sfx.write(PlaySfx(Sfx::ItemPickup));
            toasts.write(Toast(item.name().to_string()));
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Crate spin angle at time `t`: quarter turns that ease in and out and rest
/// face-on in between, so the "?" reads most of the time.
fn spin_angle(t: f32) -> f32 {
    let u = t * BOX_SPIN * 4.0;
    let ease = |f: f32| f * f * (3.0 - 2.0 * f);
    (u.floor() + ease(ease(u.fract()))) * TAU / 4.0
}

/// Crate hover height (px above the ground point) at bob phase `bob` (-1..1).
fn hover_height(bob: f32) -> f32 {
    (BOX_HOVER + bob * BOX_BOB).round()
}

/// Shadow alpha for a crate hovering `height` px up: fades a little as it rises.
fn shadow_alpha(height: f32) -> f32 {
    let t = ((height - (BOX_HOVER - BOX_BOB)) / (2.0 * BOX_BOB)).clamp(0.0, 1.0);
    BOX_SHADOW_ALPHA * (1.0 - 0.35 * t)
}

fn animate_boxes(
    time: Res<Time>,
    mut roots: Query<(&LuckyBox, &mut Visibility)>,
    mut crates: Query<(&BoxCrate, &mut Transform, &mut Sprite), Without<BoxShadow>>,
    mut shadows: Query<&mut Sprite, With<BoxShadow>>,
) {
    for (b, mut vis) in &mut roots {
        let want = if b.hidden > 0.0 { Visibility::Hidden } else { Visibility::Visible };
        if *vis != want {
            *vis = want;
        }
    }
    let t = time.elapsed_secs();
    for (c, mut tf, mut sprite) in &mut crates {
        let height = hover_height((t * 3.2 + c.phase).sin());
        if tf.translation.y != height {
            tf.translation.y = height;
        }
        let i = frame_index(spin_angle(t + c.phase), c.frames.len());
        if sprite.image != c.frames[i] {
            sprite.image = c.frames[i].clone();
        }
        if let Ok(mut s) = shadows.get_mut(c.shadow) {
            if s.image != c.shadows[i] {
                s.image = c.shadows[i].clone();
            }
            let color = Color::BLACK.with_alpha(shadow_alpha(height));
            if s.color != color {
                s.color = color;
            }
        }
    }
}

fn animate_rings(
    mut commands: Commands,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut rings: Query<(Entity, &mut Ring)>,
) {
    for (e, mut ring) in &mut rings {
        ring.age += time.delta_secs();
        let t = ring.age / RING_LIFE;
        if t >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        let radius = (ring.radius * (1.0 - (1.0 - t) * (1.0 - t))).max(4.0);
        let frame = (ring.age * 30.0) as u32;
        let _ = meshes.insert(ring.mesh.id(), ring_mesh(radius, ring.kind, frame));
        if let Some(mut m) = materials.get_mut(&ring.material) {
            m.color = Color::WHITE.with_alpha((1.0 - t) * 0.9);
        }
    }
}

/// Fades the player's whole car stack while Phantom is active, and restores it.
fn phantom_fade(mut q: Query<(&Racer, &mut StackPose), With<Loadout>>) {
    for (r, mut pose) in &mut q {
        let want = if r.mods.phantom { PHANTOM_ALPHA } else { 1.0 };
        if (pose.alpha - want).abs() > 0.01 {
            pose.alpha = want;
        }
    }
}

// ---------------------------------------------------------------------------
// Procedural meshes
// ---------------------------------------------------------------------------

fn slick_mesh() -> Mesh {
    let mut mb = MeshBuilder::default();
    let oil = Color::srgba(0.06, 0.05, 0.09, 0.9);
    mb.circle(Vec2::ZERO, 16.0, 16, oil);
    for (x, y) in [(11.0, 5.0), (-10.0, -7.0), (3.0, -12.0), (-5.0, 11.0)] {
        mb.circle(Vec2::new(x, y), 8.0, 10, oil);
    }
    // Glossy sheen.
    mb.circle(Vec2::new(-5.0, 5.0), 4.5, 10, Color::srgba(0.42, 0.40, 0.62, 0.55));
    mb.circle(Vec2::new(6.0, -4.0), 2.5, 8, Color::srgba(0.35, 0.55, 0.55, 0.45));
    mb.circle(Vec2::new(-6.0, 6.0), 1.5, 6, Color::srgba(0.92, 0.92, 1.0, 0.8));
    mb.build()
}

/// Ring of the given radius; the static ring is jagged and flickers per `frame`.
fn ring_mesh(radius: f32, kind: RingKind, frame: u32) -> Mesh {
    let mut mb = MeshBuilder::default();
    let (color, glow, segments) = match kind {
        RingKind::Thumper => (Color::srgb(1.0, 0.88, 0.6), Color::srgba(1.0, 0.6, 0.25, 0.45), 40),
        RingKind::Static => (Color::srgb(0.55, 0.98, 1.0), Color::srgba(0.2, 0.75, 1.0, 0.45), 48),
    };
    let mut seed = frame.wrapping_mul(0x9e37_79b9) | 1;
    let jitter: Vec<f32> = (0..segments)
        .map(|_| match kind {
            RingKind::Thumper => 0.0,
            RingKind::Static => (rules::next_rand(&mut seed) % 9) as f32 - 4.0,
        })
        .collect();
    let at = |i: usize, r: f32| Vec2::from_angle(i as f32 / segments as f32 * TAU) * (r + jitter[i % segments]).max(0.0);
    for i in 0..segments {
        let j = i + 1;
        // Soft outer glow band, then the bright core.
        mb.quad(at(i, radius - 6.0), at(j, radius - 6.0), at(j, radius + 3.0), at(i, radius + 3.0), glow);
        mb.quad(at(i, radius - 2.0), at(j, radius - 2.0), at(j, radius + 1.0), at(i, radius + 1.0), color);
    }
    mb.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_bobs_above_the_ground_and_its_shadow_fades_as_it_rises() {
        let (low, high) = (hover_height(-1.0), hover_height(1.0));
        assert!(low >= 1.0 && high > low);
        assert_eq!(low.fract(), 0.0);
        assert!(shadow_alpha(high) < shadow_alpha(low));
        assert!(shadow_alpha(high) > 0.5 * BOX_SHADOW_ALPHA);
    }

    #[test]
    fn crate_spin_rests_face_on_and_never_turns_back() {
        // Quarter-turn boundaries land exactly on the axis-aligned frames.
        for k in 0..8 {
            let t = k as f32 / (BOX_SPIN * 4.0);
            let i = frame_index(spin_angle(t), BOX_FRAMES);
            assert_eq!(i % (BOX_FRAMES / 4), 0, "t {t}");
        }
        // Near a rest the angle barely moves; it always advances.
        let rest = 1.0 / (BOX_SPIN * 4.0);
        assert!(spin_angle(rest + 0.05) - spin_angle(rest) < 0.01);
        let mut prev = spin_angle(0.0);
        for k in 1..200 {
            let a = spin_angle(k as f32 * 0.02);
            assert!(a >= prev);
            prev = a;
        }
    }

    #[test]
    fn pair_mut_returns_requested_order() {
        let mut v = [1, 2, 3];
        let (a, b) = pair_mut(&mut v, 2, 0);
        assert_eq!((*a, *b), (3, 1));
        let (a, b) = pair_mut(&mut v, 0, 1);
        assert_eq!((*a, *b), (1, 2));
    }

    #[test]
    fn clearing_a_gadget_flag_leaves_others() {
        let mut m = CarModifiers { all_terrain: true, drift_surge: true, ramming: true, phantom: true, spin_recovery: true };
        clear_gadget_flag(&mut m, GadgetId::Phantom);
        assert!(!m.phantom && m.all_terrain && m.ramming && m.drift_surge);
        clear_gadget_flag(&mut m, GadgetId::Thumper);
        assert!(m.all_terrain && m.ramming && m.drift_surge);
        for g in GadgetId::ALL {
            clear_gadget_flag(&mut m, g);
        }
        assert!(m.spin_recovery, "spin recovery is not a gadget");
    }
}
