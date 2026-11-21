//! World-space effects: skid marks, dust and tire smoke, boost flames, spinout
//! sparkles and camera shake. Everything here is cosmetic and never feeds back
//! into the simulation.

use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::core::track::Surface;
use crate::core::tracks::TrackId;

use super::AppState;
use super::flame::{FLICKER_FRAMES, FlameArt, flame_reach};
use super::pixel::{WORLD_LAYER, WorldCamera};
use super::race::{CurrentTrack, PlayerStep, Racer};
use super::stack::{ATTACH_Z, CarStack, SQUASH};
use super::theme::world_theme;
use super::track_view::Rng;

const SKID_CAP: usize = 800;
const SKID_LIFE: f32 = 8.0;
const SKID_SPACING: f32 = 6.0;
const SKID_Z: f32 = 5.2;
const REAR_AXLE: f32 = 11.0;
const REAR_TRACK: f32 = 6.0;

const PARTICLE_CAP: usize = 400;
/// Number of pre-built opacity steps per effect color.
const FADE_STEPS: usize = 8;

/// Flame z relative to the car: in front of the stack when the tail faces the
/// viewer, behind it otherwise. Within `ATTACH_Z`, so flames stay inside their
/// car's y-sort row and never cross another car or prop.
const FLAME_Z_FRONT: f32 = ATTACH_Z * 0.5;
const FLAME_Z_BACK: f32 = -ATTACH_Z * 0.5;
/// Fallback distance from car center to the tail, where flames start.
const TAIL: f32 = 15.0;
/// Flicker frames advance this many times per second.
const FLICKER_RATE: f32 = 18.0;

/// Exponential shake decay rate (1/s) and the largest offset in world pixels.
pub const SHAKE_DECAY: f32 = 14.0;
pub const SHAKE_MAX: f32 = 4.0;

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shake>()
            .add_systems(Startup, bake_flames)
            .add_systems(OnEnter(AppState::Race), reset_fx)
            .add_systems(PreUpdate, remove_shake)
            .add_systems(
                Update,
                (prepare_assets, attach_flames, skid_marks, emit_particles, update_particles, update_flames, feed_shake)
                    .chain()
                    .run_if(in_state(AppState::Race)),
            )
            .add_systems(
                PostUpdate,
                apply_shake
                    .before(TransformSystems::Propagate)
                    .run_if(in_state(AppState::Race)),
            );
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Opacity step `0..steps` for an effect of this age, or `None` once it expired.
pub fn fade_step(age: f32, life: f32, steps: usize) -> Option<usize> {
    if age >= life || life <= 0.0 || steps == 0 {
        return None;
    }
    Some((((age / life).max(0.0) * steps as f32) as usize).min(steps - 1))
}

/// Opacity multiplier of fade step `i`.
pub fn step_alpha(i: usize, steps: usize) -> f32 {
    1.0 - i as f32 / steps as f32
}

/// Particle size multiplier over its life (shrinks to 40%).
pub fn particle_scale(age: f32, life: f32) -> f32 {
    1.0 - 0.6 * (age / life).clamp(0.0, 1.0)
}

/// Whole number of particles to emit this frame for a `rate` per second, using a
/// uniform `roll` in `0..1` to carry the fractional part stochastically.
pub fn emit_count(rate: f32, dt: f32, roll: f32) -> u32 {
    let n = (rate * dt).max(0.0);
    n.floor() as u32 + u32::from(roll < n.fract())
}

/// Shake amplitude after `dt` seconds of decay; tiny values snap to zero.
pub fn decay_shake(amp: f32, dt: f32) -> f32 {
    let a = amp * (-SHAKE_DECAY * dt).exp();
    if a < 0.05 { 0.0 } else { a }
}

/// Shake amplitude for a wall impact (or sudden speed loss) of `impact` px/s.
pub fn impact_shake(impact: f32) -> f32 {
    (impact / 300.0 * SHAKE_MAX).clamp(0.0, SHAKE_MAX)
}

/// Squashes a car-relative ground offset like the stacked car sprite is squashed.
pub fn squashed(offset: Vec2) -> Vec2 {
    Vec2::new(offset.x, offset.y * SQUASH)
}

/// Rear wheel positions (left, right) of a car, matching the squashed stack.
pub fn rear_wheels(pos: Vec2, heading: f32) -> [Vec2; 2] {
    let f = Vec2::from_angle(heading);
    let axle = -f * REAR_AXLE;
    [pos + squashed(axle + f.perp() * REAR_TRACK), pos + squashed(axle - f.perp() * REAR_TRACK)]
}

/// Ground point of the stacked flame behind a car, relative to the car root
/// (the flame's own slices lift it to exhaust height).
pub fn flame_offset(heading: f32, tail: f32) -> Vec2 {
    squashed(-Vec2::from_angle(heading) * flame_reach(tail))
}

// ---------------------------------------------------------------------------
// Resources and components
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Dust,
    Smoke,
    SparkA,
    SparkB,
}

/// Shared meshes and opacity-stepped materials for one track theme.
#[derive(Resource)]
struct FxAssets {
    track: TrackId,
    quad: Handle<Mesh>,
    skid: Vec<Handle<ColorMaterial>>,
    dust: Vec<Handle<ColorMaterial>>,
    smoke: Vec<Handle<ColorMaterial>>,
    spark_a: Vec<Handle<ColorMaterial>>,
    spark_b: Vec<Handle<ColorMaterial>>,
}

impl FxAssets {
    fn fades(&self, kind: Kind) -> &[Handle<ColorMaterial>] {
        match kind {
            Kind::Dust => &self.dust,
            Kind::Smoke => &self.smoke,
            Kind::SparkA => &self.spark_a,
            Kind::SparkB => &self.spark_b,
        }
    }
}

#[derive(Resource)]
struct FxState {
    rng: Rng,
    /// Last skid stamp (left, right rear wheel) per racer while it is skidding.
    trails: HashMap<Entity, [Vec2; 2]>,
    skids: VecDeque<Entity>,
    prev_speed: Option<f32>,
    prev_spin: bool,
}

impl Default for FxState {
    fn default() -> Self {
        Self {
            rng: Rng::new(0x5eed_f00d),
            trails: HashMap::new(),
            skids: VecDeque::new(),
            prev_speed: None,
            prev_spin: false,
        }
    }
}

#[derive(Resource, Default)]
struct Shake {
    amp: f32,
    /// Offset currently added to the world camera.
    applied: Vec2,
    rng_state: u32,
}

#[derive(Component)]
struct SkidMark {
    age: f32,
    step: usize,
}

#[derive(Component)]
struct Particle {
    kind: Kind,
    vel: Vec2,
    age: f32,
    life: f32,
    size: f32,
    step: usize,
}

#[derive(Component)]
struct Flame {
    owner: Entity,
}

#[derive(Component)]
struct HasFlame;

/// Racers that still need their flame quads.
type FlamelessRacers<'w, 's> = Query<'w, 's, Entity, (With<Racer>, Without<HasFlame>)>;

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

fn reset_fx(mut commands: Commands, mut shake: ResMut<Shake>) {
    commands.insert_resource(FxState::default());
    shake.amp = 0.0;
}

fn faded(materials: &mut Assets<ColorMaterial>, color: Color) -> Vec<Handle<ColorMaterial>> {
    let base = color.alpha();
    (0..FADE_STEPS)
        .map(|i| {
            materials.add(ColorMaterial {
                color: color.with_alpha(base * step_alpha(i, FADE_STEPS)),
                alpha_mode: bevy::sprite_render::AlphaMode2d::Blend,
                ..default()
            })
        })
        .collect()
}

fn prepare_assets(
    mut commands: Commands,
    track: Option<Res<CurrentTrack>>,
    assets: Option<Res<FxAssets>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let Some(track) = track else { return };
    if assets.is_some_and(|a| a.track == track.id) {
        return;
    }
    let theme = world_theme(track.id);
    commands.insert_resource(FxAssets {
        track: track.id,
        quad: meshes.add(Rectangle::new(1.0, 1.0)),
        skid: faded(&mut materials, theme.skid),
        dust: faded(&mut materials, theme.dust.with_alpha(0.85)),
        smoke: faded(&mut materials, Color::srgba(0.86, 0.86, 0.9, 0.7)),
        spark_a: faded(&mut materials, Color::srgb_u8(255, 250, 220)),
        spark_b: faded(&mut materials, Color::srgb_u8(255, 214, 72)),
    });
}

/// Boost flames are voxel plumes baked once at every heading.
fn bake_flames(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(FlameArt::bake(&mut images));
}

/// Gives every racer a stacked flame sprite as a child, hidden until boosting.
fn attach_flames(mut commands: Commands, art: Option<Res<FlameArt>>, racers: FlamelessRacers) {
    let Some(art) = art else { return };
    for e in &racers {
        // Placed relative to the (unrotated) car root each frame in `update_flames`.
        commands.entity(e).insert(HasFlame).with_children(|c| {
            c.spawn((
                Flame { owner: e },
                Sprite::from_image(art.get(false, 0, 0.0).clone()),
                Transform::from_xyz(-TAIL, 0.0, FLAME_Z_BACK),
                Visibility::Hidden,
                WORLD_LAYER,
            ));
        });
    }
}

fn spawn_quad(
    commands: &mut Commands,
    assets: &FxAssets,
    material: Handle<ColorMaterial>,
    pos: Vec3,
    angle: f32,
    scale: Vec2,
) -> Entity {
    commands
        .spawn((
            Mesh2d(assets.quad.clone()),
            MeshMaterial2d(material),
            Transform::from_translation(pos)
                .with_rotation(Quat::from_rotation_z(angle))
                .with_scale(scale.extend(1.0)),
            WORLD_LAYER,
            DespawnOnExit(AppState::Race),
        ))
        .id()
}

fn skid_marks(
    mut commands: Commands,
    time: Res<Time>,
    assets: Option<Res<FxAssets>>,
    state: Option<ResMut<FxState>>,
    racers: Query<(Entity, &Racer)>,
    mut marks: Query<(Entity, &mut SkidMark, &mut MeshMaterial2d<ColorMaterial>)>,
) {
    let (Some(assets), Some(mut state)) = (assets, state) else { return };
    let dt = time.delta_secs();

    // Age and fade existing marks.
    for (e, mut mark, mut mat) in &mut marks {
        mark.age += dt;
        match fade_step(mark.age, SKID_LIFE, FADE_STEPS) {
            Some(step) if step != mark.step => {
                mark.step = step;
                mat.0 = assets.skid[step].clone();
            }
            Some(_) => {}
            None => commands.entity(e).despawn(),
        }
    }
    state.skids.retain(|e| marks.contains(*e));

    for (e, r) in &racers {
        let s = &r.state;
        let speed = s.speed();
        let skidding = (s.drifting && speed > 40.0)
            || (r.input.handbrake && speed > 60.0)
            || (r.input.brake > 0.0 && s.forward_speed() > 120.0)
            || (s.spin_time > 0.0 && speed > 60.0);
        if !skidding {
            state.trails.remove(&e);
            continue;
        }
        let wheels = rear_wheels(s.pos, s.heading);
        let Some(last) = state.trails.get(&e).copied() else {
            state.trails.insert(e, wheels);
            continue;
        };
        if wheels[0].distance(last[0]) < SKID_SPACING {
            continue;
        }
        for k in 0..2 {
            let seg = wheels[k] - last[k];
            let len = seg.length();
            // Very long jumps (respawn, teleport) are not drawn.
            if len > 60.0 {
                continue;
            }
            let mid = (wheels[k] + last[k]) * 0.5;
            let id = spawn_quad(
                &mut commands,
                &assets,
                assets.skid[0].clone(),
                mid.extend(SKID_Z),
                seg.to_angle(),
                Vec2::new(len + 1.0, 2.0),
            );
            commands.entity(id).insert(SkidMark { age: 0.0, step: 0 });
            state.skids.push_back(id);
        }
        state.trails.insert(e, wheels);
    }

    while state.skids.len() > SKID_CAP {
        if let Some(old) = state.skids.pop_front() {
            commands.entity(old).try_despawn();
        }
    }
}

fn emit_particles(
    mut commands: Commands,
    time: Res<Time>,
    assets: Option<Res<FxAssets>>,
    state: Option<ResMut<FxState>>,
    racers: Query<&Racer>,
    particles: Query<(), With<Particle>>,
) {
    let (Some(assets), Some(mut state)) = (assets, state) else { return };
    let dt = time.delta_secs();
    let mut budget = PARTICLE_CAP.saturating_sub(particles.iter().count());

    for r in &racers {
        let s = &r.state;
        let speed = s.speed();
        let wheels = rear_wheels(s.pos, s.heading);
        let mut jobs: Vec<(Kind, f32)> = Vec::new();
        if matches!(s.surface, Surface::Gravel | Surface::Grass) && speed > 60.0 {
            jobs.push((Kind::Dust, 25.0 + speed * 0.12));
        }
        if s.drifting && speed > 40.0 {
            jobs.push((Kind::Smoke, 40.0));
        }
        if s.spin_time > 0.0 {
            jobs.push((Kind::SparkA, 14.0));
            jobs.push((Kind::SparkB, 14.0));
        }
        for (kind, rate) in jobs {
            let n = emit_count(rate, dt, state.rng.f());
            for _ in 0..n {
                if budget == 0 {
                    return;
                }
                budget -= 1;
                let rng = &mut state.rng;
                let (pos, vel, life, size, z) = match kind {
                    Kind::Dust | Kind::Smoke => {
                        let w = wheels[(rng.next_u32() & 1) as usize];
                        let spread = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * 22.0;
                        let size = if kind == Kind::Dust { rng.range(1.0, 3.0) } else { rng.range(2.0, 3.0) };
                        (w, -s.vel * 0.12 + spread, rng.range(0.45, 0.7), size, 6.0)
                    }
                    Kind::SparkA | Kind::SparkB => {
                        let dir = Vec2::from_angle(rng.range(0.0, std::f32::consts::TAU));
                        let p = s.pos + dir * rng.range(10.0, 18.0);
                        (p, dir.perp() * 40.0 + s.vel * 0.6, rng.range(0.3, 0.5), 2.0, 18.0)
                    }
                };
                let id = spawn_quad(
                    &mut commands,
                    &assets,
                    assets.fades(kind)[0].clone(),
                    pos.extend(z),
                    0.0,
                    Vec2::splat(size.round().max(1.0)),
                );
                commands.entity(id).insert(Particle { kind, vel, age: 0.0, life, size, step: 0 });
            }
        }
    }
}

fn update_particles(
    mut commands: Commands,
    time: Res<Time>,
    assets: Option<Res<FxAssets>>,
    mut q: Query<(Entity, &mut Particle, &mut Transform, &mut MeshMaterial2d<ColorMaterial>)>,
) {
    let Some(assets) = assets else { return };
    let dt = time.delta_secs();
    for (e, mut p, mut tf, mut mat) in &mut q {
        p.age += dt;
        let Some(step) = fade_step(p.age, p.life, FADE_STEPS) else {
            commands.entity(e).despawn();
            continue;
        };
        let vel = p.vel;
        tf.translation += (vel * dt).extend(0.0);
        p.vel = vel * (1.0 - 3.0 * dt).max(0.0);
        let size = (p.size * particle_scale(p.age, p.life)).max(1.0).round();
        tf.scale = Vec3::new(size, size, 1.0);
        if step != p.step {
            p.step = step;
            mat.0 = assets.fades(p.kind)[step].clone();
        }
    }
}

fn update_flames(
    time: Res<Time>,
    art: Option<Res<FlameArt>>,
    racers: Query<(&Racer, Option<&CarStack>)>,
    mut flames: Query<(&Flame, &mut Transform, &mut Visibility, &mut Sprite)>,
) {
    let Some(art) = art else { return };
    let tick = (time.elapsed_secs() * FLICKER_RATE) as usize;
    for (flame, mut tf, mut vis, mut sprite) in &mut flames {
        let Ok((r, stack)) = racers.get(flame.owner) else { continue };
        if !r.state.is_boosting() {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        }
        vis.set_if_neq(Visibility::Inherited);
        // Cars flicker out of step with each other.
        let frame = (tick + flame.owner.index_u32() as usize) % FLICKER_FRAMES;
        let tail = stack.map_or(TAIL, |s| s.half_length);
        let heading = r.state.heading;
        let tail_toward_viewer = heading.sin() > 0.0;
        let z = if tail_toward_viewer { FLAME_Z_FRONT } else { FLAME_Z_BACK };
        tf.translation = flame_offset(heading, tail).round().extend(z);
        let image = art.get(r.state.nitro_active, frame, heading);
        if sprite.image != *image {
            sprite.image = image.clone();
        }
    }
}

/// Turns player impacts, spinouts and kerb rumble into camera shake.
fn feed_shake(
    mut steps: MessageReader<PlayerStep>,
    state: Option<ResMut<FxState>>,
    racers: Query<&Racer>,
    mut shake: ResMut<Shake>,
) {
    let mut add: f32 = 0.0;
    let mut wall = false;
    for PlayerStep(ev) in steps.read() {
        if let Some(impact) = ev.wall_hit {
            add = add.max(impact_shake(impact));
            wall = true;
        }
    }
    let Some(mut state) = state else { return };
    if let Some(p) = racers.iter().find(|r| r.is_player) {
        let speed = p.state.speed();
        if let Some(prev) = state.prev_speed {
            let loss = prev - speed;
            // Big car contacts show up as a sudden speed loss without a wall hit.
            if !wall && loss > 90.0 {
                add = add.max(impact_shake(loss));
            }
        }
        let spinning = p.state.spin_time > 0.0;
        if spinning && !state.prev_spin {
            add = add.max(2.5);
        }
        if p.state.surface == Surface::Kerb && speed > 150.0 {
            add = add.max(1.0);
        }
        state.prev_speed = Some(speed);
        state.prev_spin = spinning;
    }
    shake.amp = shake.amp.max(add).min(SHAKE_MAX);
}

fn apply_shake(time: Res<Time>, mut shake: ResMut<Shake>, mut cam: Query<&mut Transform, With<WorldCamera>>) {
    shake.amp = decay_shake(shake.amp, time.delta_secs());
    let Ok(mut tf) = cam.single_mut() else { return };
    if shake.amp <= 0.0 {
        return;
    }
    let mut rng = Rng::new(shake.rng_state.wrapping_add(0x9e37_79b9));
    shake.rng_state = rng.next_u32();
    let a = shake.amp;
    let offset = Vec2::new(rng.range(-a, a), rng.range(-a, a)).round();
    tf.translation += offset.extend(0.0);
    shake.applied = offset;
}

/// Undo last frame's shake before anything reads or rewrites the camera.
fn remove_shake(mut shake: ResMut<Shake>, mut cam: Query<&mut Transform, With<WorldCamera>>) {
    if shake.applied == Vec2::ZERO {
        return;
    }
    if let Ok(mut tf) = cam.single_mut() {
        tf.translation -= shake.applied.extend(0.0);
    }
    shake.applied = Vec2::ZERO;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_steps_progress_and_expire() {
        assert_eq!(fade_step(0.0, 8.0, 8), Some(0));
        assert_eq!(fade_step(4.0, 8.0, 8), Some(4));
        assert_eq!(fade_step(7.99, 8.0, 8), Some(7));
        assert_eq!(fade_step(8.0, 8.0, 8), None);
        assert_eq!(fade_step(1.0, 0.0, 8), None);
        assert_eq!(step_alpha(0, 8), 1.0);
        assert!(step_alpha(7, 8) > 0.0);
    }

    #[test]
    fn particles_shrink() {
        assert_eq!(particle_scale(0.0, 0.6), 1.0);
        assert!((particle_scale(0.6, 0.6) - 0.4).abs() < 1e-6);
        assert!((particle_scale(9.0, 0.6) - 0.4).abs() < 1e-6);
    }

    #[test]
    fn emission_carries_fraction() {
        assert_eq!(emit_count(60.0, 1.0 / 60.0, 0.5), 1);
        assert_eq!(emit_count(30.0, 1.0 / 60.0, 0.4), 1);
        assert_eq!(emit_count(30.0, 1.0 / 60.0, 0.6), 0);
        assert_eq!(emit_count(150.0, 1.0 / 60.0, 0.99), 2);
        assert_eq!(emit_count(-5.0, 1.0, 0.0), 0);
    }

    #[test]
    fn shake_decays_and_caps() {
        let a = decay_shake(4.0, 1.0 / 60.0);
        assert!(a < 4.0 && a > 3.0);
        assert_eq!(decay_shake(0.04, 0.0), 0.0);
        let mut amp = SHAKE_MAX;
        for _ in 0..30 {
            amp = decay_shake(amp, 1.0 / 60.0);
        }
        assert_eq!(amp, 0.0);
        assert_eq!(impact_shake(1000.0), SHAKE_MAX);
        assert_eq!(impact_shake(-10.0), 0.0);
        assert!((impact_shake(150.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn rear_wheels_sit_behind_center() {
        let [l, r] = rear_wheels(Vec2::ZERO, 0.0);
        assert_eq!(l, Vec2::new(-REAR_AXLE, REAR_TRACK * SQUASH));
        assert_eq!(r, Vec2::new(-REAR_AXLE, -REAR_TRACK * SQUASH));
    }

    #[test]
    fn flames_sit_behind_the_tail() {
        let reach = flame_reach(15.0);
        let o = flame_offset(0.0, 15.0);
        assert_eq!(o, Vec2::new(-reach, 0.0));
        // Pointing up, the flame hangs below the car, squashed like the stack.
        let up = flame_offset(std::f32::consts::FRAC_PI_2, 15.0);
        assert!((up.x).abs() < 1e-4);
        assert!((up.y + reach * SQUASH).abs() < 1e-4);
    }
}
