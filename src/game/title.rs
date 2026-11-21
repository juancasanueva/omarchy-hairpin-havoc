//! Title screen: game name, track carousel with preview, mode toggle, loadout
//! summary, and entry into the garage or a race.

use bevy::prelude::*;

use crate::core::cars::{CarId, car_spec};
use crate::core::tracks::{TrackId, track_def};

use super::audio::{PlaySfx, Sfx};
use super::garage::car_preview_image;
use super::palette::UiPalette;
use super::profile::Profile;
use super::results::fmt_time;
use super::{AppState, RaceMode};

/// The embedded pixel font (Press Start 2P, OFL-1.1, see `assets/fonts/OFL.txt`).
#[derive(Resource, Clone)]
pub struct GameFont(pub Handle<Font>);

impl GameFont {
    pub fn text(&self, size: f32) -> TextFont {
        TextFont {
            font: self.0.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        }
    }
}

/// Size of the track preview box in UI pixels.
const PREVIEW_SIZE: Vec2 = Vec2::new(220.0, 160.0);
const PREVIEW_MARGIN: f32 = 8.0;
/// Every n-th main-path sample becomes a preview dot.
const PREVIEW_STRIDE: usize = 8;
const PREVIEW_DOT: f32 = 3.0;
/// Integer upscale of the stacked car preview on the title screen.
const CAR_SCALE: f32 = 4.0;

/// Current title-screen selections; initialized from the profile on enter.
#[derive(Resource, Debug, Clone, Copy)]
struct TitleMenu {
    track: usize,
    mode: RaceMode,
}

/// Sampled main-path points of every track, in `TrackId::ALL` order.
/// Built once on first use.
#[derive(Resource, Default)]
struct TrackPreviews(Vec<Vec<Vec2>>);

impl TrackPreviews {
    fn ensure_built(&mut self) {
        if !self.0.is_empty() {
            return;
        }
        self.0 = TrackId::ALL
            .iter()
            .map(|&id| {
                let track = track_def(id).build();
                track
                    .main
                    .samples
                    .iter()
                    .step_by(PREVIEW_STRIDE)
                    .map(|s| s.pos)
                    .collect()
            })
            .collect();
    }
}

/// The car sprite image created for this visit to the title screen.
#[derive(Resource)]
struct TitleCarImage(Handle<Image>);

#[derive(Component)]
struct TitleRoot;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum TitleText {
    Credits,
    TrackName,
    Difficulty,
    Pb,
}

#[derive(Component)]
struct ModeRow(RaceMode);

#[derive(Component)]
struct PreviewBox;

#[derive(Component)]
struct PreviewDot;

pub struct TitlePlugin;

impl Plugin for TitlePlugin {
    fn build(&self, app: &mut App) {
        // Inserted at build time: the initial `OnEnter` runs before any startup commands apply.
        let bytes = include_bytes!("../../assets/fonts/PressStart2P-Regular.ttf").to_vec();
        let handle = app
            .world_mut()
            .resource_mut::<Assets<Font>>()
            .add(Font::from_bytes(bytes));
        app.insert_resource(GameFont(handle))
            .init_resource::<TrackPreviews>()
            .add_systems(OnEnter(AppState::Title), spawn_title)
            .add_systems(OnExit(AppState::Title), despawn_title)
            .add_systems(
                Update,
                (title_input, refresh_title)
                    .chain()
                    .run_if(in_state(AppState::Title)),
            )
            .add_systems(Startup, dev_autostart);
    }
}

/// Development aid: `HAIRPIN_AUTOSTART=race|trial|garage|about|settings` skips the title screen.
fn dev_autostart(mut mode: ResMut<RaceMode>, mut next: ResMut<NextState<AppState>>) {
    match std::env::var("HAIRPIN_AUTOSTART").as_deref() {
        Ok("garage") => next.set(AppState::Garage),
        Ok("about") => next.set(AppState::About),
        Ok("settings") => next.set(AppState::Settings),
        Ok("race") => {
            *mode = RaceMode::Race;
            next.set(AppState::Race);
        }
        Ok("trial") => {
            *mode = RaceMode::TimeTrial;
            next.set(AppState::Race);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Moves `index` by `delta` within `0..len`, wrapping around both ends.
pub fn wrap_index(index: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (index as i64 + delta as i64).rem_euclid(len as i64) as usize
}

/// Maps world points (Y up) into a `size` box in UI coordinates (Y down),
/// uniformly scaled to fit inside `margin` and centered.
pub fn fit_points(points: &[Vec2], size: Vec2, margin: f32) -> Vec<Vec2> {
    let Some(&first) = points.first() else {
        return Vec::new();
    };
    let (min, max) = points
        .iter()
        .fold((first, first), |(lo, hi), &p| (lo.min(p), hi.max(p)));
    let extent = (max - min).max(Vec2::splat(f32::EPSILON));
    let room = (size - Vec2::splat(2.0 * margin)).max(Vec2::ZERO);
    let scale = (room.x / extent.x).min(room.y / extent.y);
    let world_center = (min + max) * 0.5;
    let box_center = size * 0.5;
    points
        .iter()
        .map(|&p| {
            let d = (p - world_center) * scale;
            Vec2::new(box_center.x + d.x, box_center.y - d.y)
        })
        .collect()
}

/// The cheapest car not yet owned (catalog order breaks ties) and the credits
/// still missing to buy it.
pub fn next_unlock(owned: &[CarId], credits: u32) -> Option<(CarId, u32)> {
    CarId::ALL
        .into_iter()
        .filter(|c| !owned.contains(c))
        .min_by_key(|&c| car_spec(c).price)
        .map(|c| (c, car_spec(c).price.saturating_sub(credits)))
}

/// Summary line for [`next_unlock`].
pub fn next_unlock_text(owned: &[CarId], credits: u32) -> String {
    match next_unlock(owned, credits) {
        None => "ALL CARS OWNED".into(),
        Some((car, 0)) => format!("NEXT: {} READY TO BUY", car_spec(car).name.to_uppercase()),
        Some((car, need)) => format!("NEXT: {} {need} CR", car_spec(car).name.to_uppercase()),
    }
}

pub fn mode_label(mode: RaceMode) -> &'static str {
    match mode {
        RaceMode::Race => "RACE",
        RaceMode::TimeTrial => "TIME TRIAL",
    }
}

fn toggled(mode: RaceMode) -> RaceMode {
    match mode {
        RaceMode::Race => RaceMode::TimeTrial,
        RaceMode::TimeTrial => RaceMode::Race,
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

fn panel(pal: &UiPalette) -> (Node, BackgroundColor) {
    (
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            padding: UiRect::all(px(16)),
            row_gap: px(10),
            ..default()
        },
        BackgroundColor(pal.panel),
    )
}

#[allow(clippy::too_many_arguments)]
fn spawn_title(
    mut commands: Commands,
    font: Res<GameFont>,
    pal: Res<UiPalette>,
    profile: Res<Profile>,
    mode: Res<RaceMode>,
    mut previews: ResMut<TrackPreviews>,
    mut images: ResMut<Assets<Image>>,
) {
    previews.ensure_built();
    let data = &profile.data;
    commands.insert_resource(TitleMenu {
        track: data.selected_track_id().index(),
        mode: *mode,
    });

    let car = data.selected_car;
    let spec = car_spec(car);
    let image = car_preview_image(car, data.paint_for(car));
    let car_size = image.size().as_vec2() * CAR_SCALE;
    let car_handle = images.add(image);
    commands.insert_resource(TitleCarImage(car_handle.clone()));

    let label = |text: String, size: f32, color: Color| {
        (Text::new(text), font.text(size), TextColor(color))
    };

    commands
        .spawn((
            TitleRoot,
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
            root.spawn((
                TitleText::Credits,
                label(String::new(), 16.0, pal.warn),
                Node {
                    position_type: PositionType::Absolute,
                    top: px(24),
                    right: px(28),
                    ..default()
                },
            ));
            root.spawn(label("HAIRPIN HAVOC".into(), 32.0, pal.accent));

            // Middle: track carousel + mode toggle.
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(32),
                align_items: AlignItems::Stretch,
                ..default()
            })
            .with_children(|mid| {
                mid.spawn(panel(&pal)).with_children(|p| {
                    p.spawn(label("TRACK".into(), 8.0, pal.muted));
                    p.spawn((
                        TitleText::TrackName,
                        label(String::new(), 16.0, pal.foreground),
                    ));
                    p.spawn((TitleText::Difficulty, label(String::new(), 8.0, pal.accent)));
                    p.spawn((
                        PreviewBox,
                        Node {
                            width: px(PREVIEW_SIZE.x),
                            height: px(PREVIEW_SIZE.y),
                            ..default()
                        },
                        BackgroundColor(pal.background),
                    ));
                    p.spawn((TitleText::Pb, label(String::new(), 8.0, pal.foreground)));
                });
                mid.spawn((
                    Node {
                        justify_content: JustifyContent::Center,
                        min_width: px(260),
                        ..panel(&pal).0
                    },
                    BackgroundColor(pal.panel),
                ))
                .with_children(|p| {
                    p.spawn(label("MODE".into(), 8.0, pal.muted));
                    for m in [RaceMode::Race, RaceMode::TimeTrial] {
                        p.spawn((
                            ModeRow(m),
                            Node {
                                width: percent(100),
                                justify_content: JustifyContent::Center,
                                padding: UiRect::axes(px(12), px(10)),
                                ..default()
                            },
                            BackgroundColor(pal.panel),
                        ))
                        .with_child(label(
                            mode_label(m).into(),
                            12.0,
                            pal.foreground,
                        ));
                    }
                });
            });

            // Loadout summary.
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(28),
                    padding: UiRect::axes(px(20), px(8)),
                    ..default()
                },
                BackgroundColor(pal.panel),
            ))
            .with_children(|row| {
                // Stacked 3/4 view of the selected car.
                row.spawn((
                    ImageNode::new(car_handle),
                    Node {
                        width: px(car_size.x),
                        height: px(car_size.y),
                        flex_shrink: 0.0,
                        ..default()
                    },
                ));
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(10),
                    ..default()
                })
                .with_children(|col| {
                    col.spawn(label(spec.name.to_uppercase(), 16.0, pal.foreground));
                    col.spawn(label(
                        format!("GADGET: {}", data.equipped_gadget.name()),
                        8.0,
                        pal.accent,
                    ));
                    col.spawn(label(
                        next_unlock_text(&data.owned_cars, data.credits),
                        8.0,
                        pal.muted,
                    ));
                });
            });

            root.spawn(label(
                "</> TRACK   ^/v MODE   ENTER START   G GARAGE   O SETTINGS   I ABOUT   ESC QUIT".into(),
                8.0,
                pal.muted,
            ));
        });
}

fn despawn_title(
    mut commands: Commands,
    q: Query<Entity, With<TitleRoot>>,
    car_image: Option<Res<TitleCarImage>>,
    mut images: ResMut<Assets<Image>>,
) {
    for e in &q {
        commands.entity(e).despawn();
    }
    if let Some(img) = car_image {
        images.remove(&img.0);
        commands.remove_resource::<TitleCarImage>();
    }
    commands.remove_resource::<TitleMenu>();
}

#[allow(clippy::too_many_arguments)]
fn title_input(
    keys: Res<ButtonInput<KeyCode>>,
    menu: Option<ResMut<TitleMenu>>,
    mut profile: ResMut<Profile>,
    mut mode: ResMut<RaceMode>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let Some(mut menu) = menu else { return };
    let any = |codes: &[KeyCode]| keys.any_just_pressed(codes.iter().copied());
    let len = TrackId::ALL.len();

    if any(&[KeyCode::ArrowLeft, KeyCode::KeyA]) {
        menu.track = wrap_index(menu.track, -1, len);
        sfx.write(PlaySfx(Sfx::MenuMove));
    } else if any(&[KeyCode::ArrowRight, KeyCode::KeyD]) {
        menu.track = wrap_index(menu.track, 1, len);
        sfx.write(PlaySfx(Sfx::MenuMove));
    } else if any(&[
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::KeyW,
        KeyCode::KeyS,
        KeyCode::Tab,
    ]) {
        menu.mode = toggled(menu.mode);
        sfx.write(PlaySfx(Sfx::MenuMove));
    } else if any(&[KeyCode::Enter, KeyCode::NumpadEnter]) {
        if profile.data.selected_track != menu.track {
            profile.data.selected_track = menu.track;
            profile.persist();
        }
        *mode = menu.mode;
        sfx.write(PlaySfx(Sfx::MenuSelect));
        next.set(AppState::Race);
    } else if keys.just_pressed(KeyCode::KeyG) {
        sfx.write(PlaySfx(Sfx::MenuSelect));
        next.set(AppState::Garage);
    } else if keys.just_pressed(KeyCode::KeyO) {
        sfx.write(PlaySfx(Sfx::MenuSelect));
        next.set(AppState::Settings);
    } else if keys.just_pressed(KeyCode::KeyI) {
        sfx.write(PlaySfx(Sfx::MenuSelect));
        next.set(AppState::About);
    } else if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

/// Updates the dynamic texts, mode highlight and track preview when the
/// selections change.
#[allow(clippy::too_many_arguments)]
fn refresh_title(
    mut commands: Commands,
    menu: Option<Res<TitleMenu>>,
    profile: Res<Profile>,
    previews: Res<TrackPreviews>,
    pal: Res<UiPalette>,
    mut texts: Query<(&TitleText, &mut Text)>,
    mut modes: Query<(&ModeRow, &mut BackgroundColor, &Children)>,
    mut colors: Query<&mut TextColor>,
    preview_box: Query<Entity, With<PreviewBox>>,
    dots: Query<Entity, With<PreviewDot>>,
) {
    let Some(menu) = menu else { return };
    if !menu.is_changed() && !profile.is_changed() {
        return;
    }
    let track = TrackId::ALL[menu.track.min(TrackId::ALL.len() - 1)];
    let def = track_def(track);

    for (kind, mut text) in &mut texts {
        text.0 = match kind {
            TitleText::Credits => format!("CR {}", profile.data.credits),
            TitleText::TrackName => format!("< {} >", def.name.to_uppercase()),
            TitleText::Difficulty => def.difficulty.label().to_string(),
            TitleText::Pb => match profile.data.best_lap(track) {
                Some(t) => format!("PB {}", fmt_time(t)),
                None => "NO PB".to_string(),
            },
        };
    }

    for (row, mut bg, children) in &mut modes {
        let active = row.0 == menu.mode;
        bg.0 = if active { pal.accent } else { pal.panel };
        for &child in children {
            if let Ok(mut c) = colors.get_mut(child) {
                c.0 = if active { pal.background } else { pal.muted };
            }
        }
    }

    for e in &dots {
        commands.entity(e).despawn();
    }
    let (Ok(box_entity), Some(points)) = (preview_box.single(), previews.0.get(track.index()))
    else {
        return;
    };
    let fitted = fit_points(points, PREVIEW_SIZE, PREVIEW_MARGIN);
    for (i, p) in fitted.iter().enumerate() {
        let (size, color) = if i == 0 {
            (PREVIEW_DOT * 2.0, pal.accent)
        } else {
            (PREVIEW_DOT, pal.foreground)
        };
        commands.spawn((
            PreviewDot,
            ChildOf(box_entity),
            Node {
                position_type: PositionType::Absolute,
                left: px(p.x - size * 0.5),
                top: px(p.y - size * 0.5),
                width: px(size),
                height: px(size),
                ..default()
            },
            BackgroundColor(color),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_index_wraps_both_ways() {
        assert_eq!(wrap_index(0, -1, 4), 3);
        assert_eq!(wrap_index(3, 1, 4), 0);
        assert_eq!(wrap_index(1, 1, 4), 2);
        assert_eq!(wrap_index(2, -7, 4), 3);
        assert_eq!(wrap_index(0, 1, 0), 0);
    }

    #[test]
    fn fit_points_fills_box_and_flips_y() {
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0)];
        let out = fit_points(&pts, Vec2::new(220.0, 160.0), 10.0);
        // Width-limited: 200 / 100 = 2x; height 50 * 2 = 100, centered vertically.
        assert!((out[0] - Vec2::new(10.0, 130.0)).length() < 1e-3);
        assert!((out[1] - Vec2::new(210.0, 30.0)).length() < 1e-3);
    }

    #[test]
    fn fit_points_handles_degenerate_input() {
        assert!(fit_points(&[], Vec2::splat(100.0), 5.0).is_empty());
        let out = fit_points(&[Vec2::new(5.0, 5.0)], Vec2::splat(100.0), 5.0);
        assert_eq!(out.len(), 1);
        assert!(out[0].is_finite());
    }

    #[test]
    fn real_track_preview_stays_inside_box() {
        let track = track_def(TrackId::ALL[0]).build();
        let pts: Vec<Vec2> = track
            .main
            .samples
            .iter()
            .step_by(PREVIEW_STRIDE)
            .map(|s| s.pos)
            .collect();
        assert!(pts.len() > 10);
        for p in fit_points(&pts, PREVIEW_SIZE, PREVIEW_MARGIN) {
            assert!(p.x >= PREVIEW_MARGIN - 1e-3 && p.x <= PREVIEW_SIZE.x - PREVIEW_MARGIN + 1e-3);
            assert!(p.y >= PREVIEW_MARGIN - 1e-3 && p.y <= PREVIEW_SIZE.y - PREVIEW_MARGIN + 1e-3);
        }
    }

    #[test]
    fn next_unlock_picks_cheapest_missing_car() {
        let starters = [CarId::Pebble, CarId::Sparrow];
        let (car, need) = next_unlock(&starters, 0).unwrap();
        let cheapest = CarId::ALL
            .into_iter()
            .filter(|c| !starters.contains(c))
            .map(|c| car_spec(c).price)
            .min()
            .unwrap();
        assert_eq!(car_spec(car).price, cheapest);
        assert_eq!(need, cheapest);
        assert_eq!(next_unlock(&starters, cheapest + 10).unwrap().1, 0);
        assert!(next_unlock_text(&starters, 0).starts_with("NEXT: "));
        assert!(next_unlock_text(&starters, 0).ends_with(" CR"));
        assert_eq!(next_unlock(&CarId::ALL, 0), None);
        assert_eq!(next_unlock_text(&CarId::ALL, 0), "ALL CARS OWNED");
    }

    #[test]
    fn mode_toggle_round_trips() {
        assert_eq!(toggled(toggled(RaceMode::Race)), RaceMode::Race);
        assert_eq!(mode_label(toggled(RaceMode::Race)), "TIME TRIAL");
    }
}
