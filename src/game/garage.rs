//! Garage: buy and select cars, install upgrades, buy and apply paints, and
//! buy and equip gadgets. Purchases go through `core::save` and are persisted
//! immediately.

use bevy::prelude::*;

use crate::core::cars::{CarId, STAT_LABELS, car_spec};
use crate::core::economy::{GadgetId, MAX_UPGRADE_LEVEL, PaintId, UpgradeKind, upgrade_price};
use crate::core::save::{PurchaseError, SaveData};

use super::AppState;
use super::audio::{PlaySfx, Sfx};
use super::palette::UiPalette;
use super::profile::Profile;
use super::sprites::car_image;
use super::title::GameFont;

/// Integer upscale of the big car preview.
const PREVIEW_SCALE: f32 = 6.0;
const PIP: f32 = 10.0;

pub struct GaragePlugin;

impl Plugin for GaragePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GaragePreview>()
            .add_systems(OnEnter(AppState::Garage), spawn_garage)
            .add_systems(OnExit(AppState::Garage), cleanup_garage)
            .add_systems(
                Update,
                (garage_input, refresh_garage)
                    .chain()
                    .run_if(in_state(AppState::Garage)),
            );
    }
}

/// Sprite of `car` painted with `paint`, pointing along +X.
pub fn car_preview_image(car: CarId, paint: PaintId) -> Image {
    let spec = car_spec(car);
    let (body, accent) = paint.resolve_colors(&spec);
    car_image(spec.body, body, accent)
}

// ---------------------------------------------------------------------------
// Pure menu logic
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Cars,
    Upgrades,
    Paint,
    Gadgets,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Cars, Tab::Upgrades, Tab::Paint, Tab::Gadgets];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Cars => "CARS",
            Tab::Upgrades => "UPGRADES",
            Tab::Paint => "PAINT",
            Tab::Gadgets => "GADGETS",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&t| t == self).unwrap_or(0)
    }

    /// The tab `delta` steps away, wrapping around.
    pub fn step(self, delta: i32) -> Tab {
        let n = Self::ALL.len() as i32;
        Self::ALL[(self.index() as i32 + delta).rem_euclid(n) as usize]
    }

    pub fn row_count(self) -> usize {
        match self {
            Tab::Cars => CarId::ALL.len(),
            Tab::Upgrades => UpgradeKind::ALL.len(),
            Tab::Paint => PaintId::ALL.len(),
            Tab::Gadgets => GadgetId::ALL.len(),
        }
    }

    /// Row to highlight when the tab opens: the current selection, if any.
    pub fn initial_row(self, data: &SaveData) -> usize {
        match self {
            Tab::Cars => data.selected_car.index(),
            Tab::Upgrades => 0,
            Tab::Paint => index_of(&PaintId::ALL, data.paint_for(data.selected_car)),
            Tab::Gadgets => index_of(&GadgetId::ALL, data.equipped_gadget),
        }
    }
}

fn index_of<T: PartialEq>(all: &[T], item: T) -> usize {
    all.iter().position(|x| *x == item).unwrap_or(0)
}

/// Moves `row` by `delta`, clamped to `0..len`.
pub fn clamp_row(row: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (row as i64 + delta as i64).clamp(0, len as i64 - 1) as usize
}

/// Result of pressing Enter on a garage row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Bought(String),
    Equipped(String),
    Failed(&'static str),
}

pub fn error_text(e: PurchaseError) -> &'static str {
    match e {
        PurchaseError::NotEnoughCredits => "NOT ENOUGH CREDITS",
        PurchaseError::Maxed => "MAXED",
        PurchaseError::AlreadyOwned => "ALREADY OWNED",
        PurchaseError::NotOwned => "NOT OWNED",
    }
}

/// Applies a per-car paint; `Factory` is stored as "no entry".
pub fn set_paint(data: &mut SaveData, car: CarId, paint: PaintId) {
    if paint == PaintId::Factory {
        data.paint.remove(car.key());
    } else {
        data.paint.insert(car.key().to_owned(), paint);
    }
}

/// Buys the item on `row` of `tab` if it is not owned yet, otherwise selects
/// or equips it. Upgrades always buy the next stage for the selected car.
pub fn activate(data: &mut SaveData, tab: Tab, row: usize) -> Outcome {
    let fail = |e| Outcome::Failed(error_text(e));
    match tab {
        Tab::Cars => {
            let car = CarId::ALL[row.min(CarId::ALL.len() - 1)];
            let name = car_spec(car).name.to_uppercase();
            if !data.owns_car(car) {
                return match data.buy_car(car) {
                    Ok(_) => Outcome::Bought(format!("{name} BOUGHT - ENTER TO DRIVE IT")),
                    Err(e) => fail(e),
                };
            }
            data.selected_car = car;
            Outcome::Equipped(format!("{name} SELECTED"))
        }
        Tab::Upgrades => {
            let kind = UpgradeKind::ALL[row.min(UpgradeKind::ALL.len() - 1)];
            match data.buy_upgrade(data.selected_car, kind) {
                Ok(_) => Outcome::Bought(format!(
                    "{} STAGE {} INSTALLED",
                    kind.name(),
                    data.upgrade_levels(data.selected_car).get(kind)
                )),
                Err(e) => fail(e),
            }
        }
        Tab::Paint => {
            let paint = PaintId::ALL[row.min(PaintId::ALL.len() - 1)];
            if !data.owned_paints.contains(&paint) {
                return match data.buy_paint(paint) {
                    Ok(_) => Outcome::Bought(format!("{} BOUGHT - ENTER TO APPLY", paint.name())),
                    Err(e) => fail(e),
                };
            }
            let car = data.selected_car;
            set_paint(data, car, paint);
            Outcome::Equipped(format!("{} APPLIED", paint.name()))
        }
        Tab::Gadgets => {
            let gadget = GadgetId::ALL[row.min(GadgetId::ALL.len() - 1)];
            if !data.owned_gadgets.contains(&gadget) {
                return match data.buy_gadget(gadget) {
                    Ok(_) => Outcome::Bought(format!("{} BOUGHT - ENTER TO EQUIP", gadget.name())),
                    Err(e) => fail(e),
                };
            }
            data.equipped_gadget = gadget;
            Outcome::Equipped(format!("{} EQUIPPED", gadget.name()))
        }
    }
}

/// Which car and paint the detail panel previews, if any.
pub fn preview_key(data: &SaveData, tab: Tab, row: usize) -> Option<(CarId, PaintId)> {
    match tab {
        Tab::Cars => {
            let car = CarId::ALL[row.min(CarId::ALL.len() - 1)];
            Some((car, data.paint_for(car)))
        }
        Tab::Upgrades => Some((data.selected_car, data.paint_for(data.selected_car))),
        Tab::Paint => Some((
            data.selected_car,
            PaintId::ALL[row.min(PaintId::ALL.len() - 1)],
        )),
        Tab::Gadgets => None,
    }
}

// ---------------------------------------------------------------------------
// ECS
// ---------------------------------------------------------------------------

#[derive(Resource, Debug, Clone)]
struct GarageMenu {
    tab: Tab,
    row: usize,
    message: Option<(String, bool)>,
}

/// The current big preview image and what it shows.
#[derive(Resource, Default)]
struct GaragePreview {
    key: Option<(CarId, PaintId)>,
    handle: Option<Handle<Image>>,
}

#[derive(Component)]
struct GarageRoot;

#[derive(Component)]
struct CreditsText;

#[derive(Component)]
struct MessageText;

#[derive(Component)]
struct TabLabel(Tab);

/// Container whose children are rebuilt whenever the menu changes.
#[derive(Component)]
struct GarageBody;

fn spawn_garage(
    mut commands: Commands,
    font: Res<GameFont>,
    pal: Res<UiPalette>,
    profile: Res<Profile>,
) {
    commands.insert_resource(GarageMenu {
        tab: Tab::Cars,
        row: Tab::Cars.initial_row(&profile.data),
        message: None,
    });

    commands
        .spawn((
            GarageRoot,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(28)),
                row_gap: px(16),
                ..default()
            },
            BackgroundColor(pal.background),
        ))
        .with_children(|root| {
            root.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|head| {
                head.spawn((Text::new("GARAGE"), font.text(24.0), TextColor(pal.accent)));
                head.spawn((
                    CreditsText,
                    Text::new(""),
                    font.text(16.0),
                    TextColor(pal.warn),
                ));
            });

            root.spawn(Node {
                column_gap: px(8),
                ..default()
            })
            .with_children(|tabs| {
                for (i, tab) in Tab::ALL.into_iter().enumerate() {
                    tabs.spawn((
                        TabLabel(tab),
                        Node {
                            padding: UiRect::axes(px(14), px(10)),
                            ..default()
                        },
                        BackgroundColor(pal.panel),
                    ))
                    .with_child((
                        Text::new(format!("{} {}", i + 1, tab.label())),
                        font.text(12.0),
                        TextColor(pal.foreground),
                    ));
                }
            });

            root.spawn((
                GarageBody,
                Node {
                    flex_grow: 1.0,
                    column_gap: px(20),
                    min_height: px(0),
                    ..default()
                },
            ));

            root.spawn((
                MessageText,
                Text::new(""),
                font.text(12.0),
                TextColor(pal.foreground),
            ));
            root.spawn((
                Text::new("Q/E TAB   ^/v SELECT   ENTER BUY / EQUIP   ESC BACK"),
                font.text(8.0),
                TextColor(pal.muted),
            ));
        });
}

fn cleanup_garage(
    mut commands: Commands,
    roots: Query<Entity, With<GarageRoot>>,
    mut preview: ResMut<GaragePreview>,
    mut images: ResMut<Assets<Image>>,
) {
    for e in &roots {
        commands.entity(e).despawn();
    }
    if let Some(h) = preview.handle.take() {
        images.remove(&h);
    }
    preview.key = None;
    commands.remove_resource::<GarageMenu>();
}

fn garage_input(
    keys: Res<ButtonInput<KeyCode>>,
    menu: Option<ResMut<GarageMenu>>,
    mut profile: ResMut<Profile>,
    mut next: ResMut<NextState<AppState>>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let Some(mut menu) = menu else { return };
    let any = |codes: &[KeyCode]| keys.any_just_pressed(codes.iter().copied());

    let digit_tab = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
    ]
    .into_iter()
    .zip(Tab::ALL)
    .find(|(k, _)| keys.just_pressed(*k))
    .map(|(_, t)| t);
    let new_tab = if let Some(t) = digit_tab {
        Some(t)
    } else if keys.just_pressed(KeyCode::KeyQ) {
        Some(menu.tab.step(-1))
    } else if keys.just_pressed(KeyCode::KeyE) {
        Some(menu.tab.step(1))
    } else {
        None
    };

    if let Some(tab) = new_tab {
        if tab != menu.tab {
            menu.tab = tab;
            menu.row = tab.initial_row(&profile.data);
            menu.message = None;
            sfx.write(PlaySfx(Sfx::MenuMove));
        }
        return;
    }

    let delta = if any(&[KeyCode::ArrowUp, KeyCode::KeyW]) {
        -1
    } else if any(&[KeyCode::ArrowDown, KeyCode::KeyS]) {
        1
    } else {
        0
    };
    if delta != 0 {
        let row = clamp_row(menu.row, delta, menu.tab.row_count());
        if row != menu.row {
            menu.row = row;
            menu.message = None;
            sfx.write(PlaySfx(Sfx::MenuMove));
        }
        return;
    }

    if any(&[KeyCode::Enter, KeyCode::NumpadEnter]) {
        let (tab, row) = (menu.tab, menu.row);
        let outcome = activate(&mut profile.data, tab, row);
        let (text, ok, sound) = match outcome {
            Outcome::Bought(text) => (text, true, Sfx::Purchase),
            Outcome::Equipped(text) => (text, true, Sfx::MenuSelect),
            Outcome::Failed(text) => (text.to_string(), false, Sfx::Denied),
        };
        if ok {
            profile.persist();
        }
        menu.message = Some((text, ok));
        sfx.write(PlaySfx(sound));
    } else if keys.just_pressed(KeyCode::Escape) {
        sfx.write(PlaySfx(Sfx::MenuSelect));
        next.set(AppState::Title);
    }
}

#[allow(clippy::too_many_arguments)]
fn refresh_garage(
    mut commands: Commands,
    menu: Option<Res<GarageMenu>>,
    profile: Res<Profile>,
    font: Res<GameFont>,
    pal: Res<UiPalette>,
    mut preview: ResMut<GaragePreview>,
    mut images: ResMut<Assets<Image>>,
    mut credits: Query<&mut Text, (With<CreditsText>, Without<MessageText>)>,
    mut message: Query<(&mut Text, &mut TextColor), With<MessageText>>,
    mut tabs: Query<(&TabLabel, &mut BackgroundColor)>,
    body: Query<Entity, With<GarageBody>>,
) {
    let Some(menu) = menu else { return };
    if !menu.is_changed() && !profile.is_changed() {
        return;
    }
    let data = &profile.data;

    for mut t in &mut credits {
        t.0 = format!("CR {}", data.credits);
    }
    for (mut t, mut c) in &mut message {
        let (text, ok) = menu.message.clone().unwrap_or_default();
        t.0 = text;
        c.0 = if ok { pal.good } else { pal.bad };
    }
    for (label, mut bg) in &mut tabs {
        bg.0 = if label.0 == menu.tab {
            pal.panel_hi
        } else {
            pal.panel
        };
    }

    // Swap the preview image only when the shown car or paint changes.
    let key = preview_key(data, menu.tab, menu.row);
    if key != preview.key {
        if let Some(old) = preview.handle.take() {
            images.remove(&old);
        }
        preview.handle = key.map(|(car, paint)| images.add(car_preview_image(car, paint)));
        preview.key = key;
    }
    let preview_image = preview
        .handle
        .clone()
        .and_then(|h| images.get(&h).map(|img| (h, img.size().as_vec2())));

    let Ok(body) = body.single() else { return };
    commands.entity(body).despawn_children();
    let ui = Ui {
        font: &font,
        pal: &pal,
    };
    commands.entity(body).with_children(|b| {
        b.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                width: px(560),
                row_gap: px(4),
                padding: UiRect::all(px(8)),
                ..default()
            },
            BackgroundColor(pal.panel),
        ))
        .with_children(|list| {
            for row in 0..menu.tab.row_count() {
                ui.list_row(list, data, menu.tab, row, row == menu.row);
            }
        });
        b.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                row_gap: px(12),
                padding: UiRect::all(px(16)),
                ..default()
            },
            BackgroundColor(pal.panel),
        ))
        .with_children(|detail| ui.detail(detail, data, menu.tab, menu.row, preview_image));
    });
}

/// Borrowed styling context for building garage widgets.
struct Ui<'a> {
    font: &'a GameFont,
    pal: &'a UiPalette,
}

impl Ui<'_> {
    fn text(&self, s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
        (Text::new(s.into()), self.font.text(size), TextColor(color))
    }

    /// `(label, badge, badge color)` for one list row.
    fn row_content(&self, data: &SaveData, tab: Tab, row: usize) -> (String, String, Color) {
        let pal = self.pal;
        let price = |p: u32| {
            let c = if data.credits >= p {
                pal.warn
            } else {
                pal.muted
            };
            (format!("{p} CR"), c)
        };
        match tab {
            Tab::Cars => {
                let car = CarId::ALL[row];
                let spec = car_spec(car);
                let (badge, c) = if data.selected_car == car {
                    ("SELECTED".into(), pal.accent)
                } else if data.owns_car(car) {
                    ("OWNED".into(), pal.good)
                } else {
                    price(spec.price)
                };
                (spec.name.to_uppercase(), badge, c)
            }
            Tab::Upgrades => {
                let kind = UpgradeKind::ALL[row];
                let level = data.upgrade_levels(data.selected_car).get(kind);
                let (badge, c) = match upgrade_price(kind, level) {
                    Some(p) => price(p),
                    None => ("MAXED".into(), pal.good),
                };
                (format!("{} LV{level}", kind.name()), badge, c)
            }
            Tab::Paint => {
                let paint = PaintId::ALL[row];
                let (badge, c) = if data.paint_for(data.selected_car) == paint {
                    ("EQUIPPED".into(), pal.accent)
                } else if data.owned_paints.contains(&paint) {
                    ("OWNED".into(), pal.good)
                } else {
                    price(paint.price())
                };
                (paint.name().into(), badge, c)
            }
            Tab::Gadgets => {
                let g = GadgetId::ALL[row];
                let (badge, c) = if data.equipped_gadget == g {
                    ("EQUIPPED".into(), pal.accent)
                } else if data.owned_gadgets.contains(&g) {
                    ("OWNED".into(), pal.good)
                } else {
                    price(g.price())
                };
                (g.name().into(), badge, c)
            }
        }
    }

    fn list_row(
        &self,
        list: &mut ChildSpawnerCommands,
        data: &SaveData,
        tab: Tab,
        row: usize,
        hot: bool,
    ) {
        let pal = self.pal;
        let (label, badge, badge_color) = self.row_content(data, tab, row);
        list.spawn((
            Node {
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                padding: UiRect::axes(px(12), px(10)),
                column_gap: px(12),
                ..default()
            },
            BackgroundColor(if hot { pal.panel_hi } else { pal.panel }),
        ))
        .with_children(|r| {
            r.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(10),
                ..default()
            })
            .with_children(|left| {
                let marker = if hot { ">" } else { " " };
                left.spawn(self.text(marker, 12.0, pal.accent));
                if tab == Tab::Paint {
                    let spec = car_spec(data.selected_car);
                    let (body, accent) = PaintId::ALL[row].resolve_colors(&spec);
                    self.swatches(left, body, accent);
                }
                if tab == Tab::Upgrades {
                    let kind = UpgradeKind::ALL[row];
                    let level = data.upgrade_levels(data.selected_car).get(kind);
                    left.spawn(self.text(kind.name(), 12.0, pal.foreground));
                    self.pips(left, level, MAX_UPGRADE_LEVEL);
                } else {
                    left.spawn(self.text(label, 12.0, pal.foreground));
                }
            });
            let mut right = String::new();
            if tab == Tab::Gadgets {
                right = format!("{:.0}S CD  ", GadgetId::ALL[row].cooldown());
            }
            r.spawn(Node {
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|rt| {
                if !right.is_empty() {
                    rt.spawn(self.text(right, 8.0, pal.muted));
                }
                rt.spawn(self.text(badge, 12.0, badge_color));
            });
        });
    }

    fn swatches(&self, parent: &mut ChildSpawnerCommands, body: [u8; 3], accent: [u8; 3]) {
        for (rgb, w) in [(body, 18.0), (accent, 8.0)] {
            parent.spawn((
                Node {
                    width: px(w),
                    height: px(16),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(Color::srgb_u8(rgb[0], rgb[1], rgb[2])),
                BorderColor::all(self.pal.background),
            ));
        }
    }

    fn pips(&self, parent: &mut ChildSpawnerCommands, filled: u8, total: u8) {
        parent
            .spawn(Node {
                column_gap: px(3),
                ..default()
            })
            .with_children(|p| {
                for i in 0..total {
                    let color = if i < filled {
                        self.pal.accent
                    } else {
                        self.pal.panel_hi
                    };
                    p.spawn((
                        Node {
                            width: px(PIP),
                            height: px(PIP),
                            ..default()
                        },
                        BackgroundColor(color),
                    ));
                }
            });
    }

    fn preview(&self, parent: &mut ChildSpawnerCommands, image: Option<(Handle<Image>, Vec2)>) {
        let Some((handle, size)) = image else { return };
        let size = size * PREVIEW_SCALE;
        parent
            .spawn((
                Node {
                    height: px(size.y + 24.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(self.pal.background),
            ))
            .with_child((
                ImageNode::new(handle),
                Node {
                    width: px(size.x),
                    height: px(size.y),
                    ..default()
                },
            ));
    }

    fn detail(
        &self,
        d: &mut ChildSpawnerCommands,
        data: &SaveData,
        tab: Tab,
        row: usize,
        image: Option<(Handle<Image>, Vec2)>,
    ) {
        let pal = self.pal;
        let blurb = |d: &mut ChildSpawnerCommands, s: &str| {
            d.spawn(self.text(s, 8.0, pal.foreground));
        };
        match tab {
            Tab::Cars => {
                let spec = car_spec(CarId::ALL[row]);
                self.preview(d, image);
                d.spawn(self.text(spec.name.to_uppercase(), 16.0, pal.accent));
                blurb(d, spec.blurb);
                for (label, value) in STAT_LABELS.iter().zip(spec.stats) {
                    d.spawn(Node {
                        align_items: AlignItems::Center,
                        column_gap: px(12),
                        ..default()
                    })
                    .with_children(|r| {
                        r.spawn((
                            self.text(*label, 8.0, pal.muted),
                            Node {
                                width: px(56),
                                ..default()
                            },
                        ));
                        self.pips(r, value.clamp(1, 5), 5);
                    });
                }
            }
            Tab::Upgrades => {
                let car = data.selected_car;
                let kind = UpgradeKind::ALL[row];
                self.preview(d, image);
                d.spawn(self.text(
                    format!("FOR {}", car_spec(car).name.to_uppercase()),
                    12.0,
                    pal.muted,
                ));
                d.spawn(self.text(kind.name(), 16.0, pal.accent));
                blurb(d, kind.blurb());
                let level = data.upgrade_levels(car).get(kind);
                let next = match upgrade_price(kind, level) {
                    Some(p) => format!("STAGE {} OF {MAX_UPGRADE_LEVEL}: {p} CR", level + 1),
                    None => "MAXED".into(),
                };
                d.spawn(self.text(next, 12.0, pal.warn));
            }
            Tab::Paint => {
                let paint = PaintId::ALL[row];
                self.preview(d, image);
                d.spawn(self.text(
                    format!("FOR {}", car_spec(data.selected_car).name.to_uppercase()),
                    12.0,
                    pal.muted,
                ));
                d.spawn(self.text(paint.name(), 16.0, pal.accent));
                let note = if data.owned_paints.contains(&paint) {
                    "ENTER TO APPLY".to_string()
                } else {
                    format!("{} CR - ENTER TO BUY", paint.price())
                };
                d.spawn(self.text(note, 8.0, pal.foreground));
            }
            Tab::Gadgets => {
                let g = GadgetId::ALL[row];
                d.spawn(self.text(g.name(), 16.0, pal.accent));
                blurb(d, g.blurb());
                d.spawn(self.text(
                    format!("COOLDOWN {:.0}S", g.cooldown()),
                    12.0,
                    pal.foreground,
                ));
                d.spawn(self.text(format!("ACTIVE {:.1}S", g.active()), 12.0, pal.foreground));
                if !data.owned_gadgets.contains(&g) {
                    d.spawn(self.text(format!("{} CR - ENTER TO BUY", g.price()), 12.0, pal.warn));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rich() -> SaveData {
        SaveData {
            credits: 10_000,
            ..SaveData::default()
        }
    }

    #[test]
    fn tab_step_wraps() {
        assert_eq!(Tab::Cars.step(-1), Tab::Gadgets);
        assert_eq!(Tab::Gadgets.step(1), Tab::Cars);
        assert_eq!(Tab::Upgrades.step(1), Tab::Paint);
    }

    #[test]
    fn clamp_row_stays_in_range() {
        assert_eq!(clamp_row(0, -1, 5), 0);
        assert_eq!(clamp_row(4, 1, 5), 4);
        assert_eq!(clamp_row(2, 1, 5), 3);
        assert_eq!(clamp_row(3, 0, 0), 0);
    }

    #[test]
    fn initial_rows_follow_current_selection() {
        let d = SaveData {
            equipped_gadget: GadgetId::Slick,
            ..SaveData::default()
        };
        assert_eq!(Tab::Cars.initial_row(&d), CarId::Pebble.index());
        assert_eq!(Tab::Gadgets.initial_row(&d), 1);
        assert_eq!(Tab::Paint.initial_row(&d), 0);
    }

    #[test]
    fn cars_buy_then_select() {
        let mut d = rich();
        let row = CarId::Bandit.index();
        assert!(matches!(
            activate(&mut d, Tab::Cars, row),
            Outcome::Bought(_)
        ));
        assert!(d.owns_car(CarId::Bandit));
        assert_eq!(d.selected_car, CarId::Pebble);
        assert!(matches!(
            activate(&mut d, Tab::Cars, row),
            Outcome::Equipped(_)
        ));
        assert_eq!(d.selected_car, CarId::Bandit);

        let mut poor = SaveData::default();
        assert_eq!(
            activate(&mut poor, Tab::Cars, CarId::Baron.index()),
            Outcome::Failed("NOT ENOUGH CREDITS")
        );
    }

    #[test]
    fn upgrades_buy_until_maxed() {
        let mut d = rich();
        for _ in 0..MAX_UPGRADE_LEVEL {
            assert!(matches!(
                activate(&mut d, Tab::Upgrades, 0),
                Outcome::Bought(_)
            ));
        }
        assert_eq!(activate(&mut d, Tab::Upgrades, 0), Outcome::Failed("MAXED"));
        assert_eq!(
            d.upgrade_levels(CarId::Pebble).get(UpgradeKind::ALL[0]),
            MAX_UPGRADE_LEVEL
        );
    }

    #[test]
    fn paint_buy_apply_and_back_to_factory() {
        let mut d = rich();
        let row = index_of(&PaintId::ALL, PaintId::Toxic);
        assert!(matches!(
            activate(&mut d, Tab::Paint, row),
            Outcome::Bought(_)
        ));
        assert_eq!(d.paint_for(CarId::Pebble), PaintId::Factory);
        assert!(matches!(
            activate(&mut d, Tab::Paint, row),
            Outcome::Equipped(_)
        ));
        assert_eq!(d.paint_for(CarId::Pebble), PaintId::Toxic);
        assert_eq!(d.paint_for(CarId::Sparrow), PaintId::Factory);
        activate(&mut d, Tab::Paint, 0);
        assert_eq!(d.paint_for(CarId::Pebble), PaintId::Factory);
        assert!(d.paint.is_empty());
    }

    #[test]
    fn gadgets_buy_then_equip() {
        let mut d = rich();
        let row = index_of(&GadgetId::ALL, GadgetId::Phantom);
        assert!(matches!(
            activate(&mut d, Tab::Gadgets, row),
            Outcome::Bought(_)
        ));
        assert_eq!(d.equipped_gadget, GadgetId::Thumper);
        assert!(matches!(
            activate(&mut d, Tab::Gadgets, row),
            Outcome::Equipped(_)
        ));
        assert_eq!(d.equipped_gadget, GadgetId::Phantom);
    }

    #[test]
    fn preview_key_per_tab() {
        let d = SaveData::default();
        assert_eq!(
            preview_key(&d, Tab::Cars, CarId::Mako.index()),
            Some((CarId::Mako, PaintId::Factory))
        );
        assert_eq!(
            preview_key(&d, Tab::Paint, 2),
            Some((CarId::Pebble, PaintId::ALL[2]))
        );
        assert_eq!(preview_key(&d, Tab::Gadgets, 0), None);
    }

    #[test]
    fn car_preview_uses_paint() {
        let a = car_preview_image(CarId::Pebble, PaintId::Factory);
        let b = car_preview_image(CarId::Pebble, PaintId::Toxic);
        assert_eq!(a.size(), b.size());
        assert_ne!(a.data, b.data);
    }
}
