//! Settings: the settings screen (audio, display, camera, key rebinding) and
//! the system that applies the saved settings at startup and whenever the
//! profile changes.

use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

use crate::core::save::SaveData;
use crate::core::settings::{Action, BindError, KeyBindings, Settings, key_label, step_volume};

use super::AppState;
use super::audio::{AudioSettings, PlaySfx, Sfx};
use super::controls::{Controls, key_name};
use super::palette::UiPalette;
use super::pixel::CanvasZoom;
use super::profile::Profile;
use super::title::GameFont;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Controls>()
            .add_systems(Startup, apply_settings)
            .add_systems(PreUpdate, apply_settings.run_if(resource_changed::<Profile>))
            .add_systems(OnEnter(AppState::Settings), spawn_settings)
            .add_systems(OnExit(AppState::Settings), despawn_settings)
            .add_systems(
                Update,
                (settings_input, refresh_settings).chain().run_if(in_state(AppState::Settings)),
            );
    }
}

// ---------------------------------------------------------------------------
// Applying settings
// ---------------------------------------------------------------------------

/// Window mode for the fullscreen setting.
pub fn window_mode(fullscreen: bool) -> WindowMode {
    if fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    }
}

/// Audio resource values for `settings` and the mute flag.
pub fn audio_for(settings: &Settings, muted: bool) -> AudioSettings {
    let (engine, effects) = settings.gains();
    AudioSettings { muted, engine, effects }
}

fn apply_settings(
    profile: Res<Profile>,
    mut audio: ResMut<AudioSettings>,
    mut controls: ResMut<Controls>,
    mut zoom: ResMut<CanvasZoom>,
    window: Option<Single<&mut Window, With<PrimaryWindow>>>,
) {
    let s = &profile.data.settings;
    audio.set_if_neq(audio_for(s, profile.data.muted));
    controls.set_if_neq(Controls::from_bindings(&s.keys));
    zoom.set_if_neq(CanvasZoom(s.pixel_size.scale_offset()));
    if let Some(mut window) = window {
        let mode = window_mode(s.fullscreen);
        if window.mode != mode {
            window.mode = mode;
        }
    }
}

// ---------------------------------------------------------------------------
// Rows (pure)
// ---------------------------------------------------------------------------

/// One line of the settings screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Master,
    Engine,
    Effects,
    Sound,
    Fullscreen,
    PixelSize,
    Camera,
    Bind(Action),
    ResetKeys,
}

pub const ROWS: [Row; 15] = [
    Row::Master,
    Row::Engine,
    Row::Effects,
    Row::Sound,
    Row::Fullscreen,
    Row::PixelSize,
    Row::Camera,
    Row::Bind(Action::Throttle),
    Row::Bind(Action::Brake),
    Row::Bind(Action::SteerLeft),
    Row::Bind(Action::SteerRight),
    Row::Bind(Action::Drift),
    Row::Bind(Action::Nitro),
    Row::Bind(Action::UseItem),
    Row::ResetKeys,
];

/// Section headings, shown above the row at the given index.
const SECTIONS: [(usize, &str); 3] = [(0, "AUDIO"), (4, "DISPLAY"), (7, "CONTROLS")];

impl Row {
    pub fn label(self) -> &'static str {
        match self {
            Row::Master => "MASTER VOLUME",
            Row::Engine => "ENGINE VOLUME",
            Row::Effects => "EFFECTS VOLUME",
            Row::Sound => "SOUND",
            Row::Fullscreen => "FULLSCREEN",
            Row::PixelSize => "PIXEL SIZE",
            Row::Camera => "RACE CAMERA",
            Row::Bind(action) => action.label(),
            Row::ResetKeys => "RESET CONTROLS",
        }
    }

    /// The value column for this row.
    pub fn value(self, data: &SaveData) -> String {
        let s = &data.settings;
        let on_off = |on: bool| if on { "ON" } else { "OFF" }.to_string();
        match self {
            Row::Master => format!("{}%", s.master_volume),
            Row::Engine => format!("{}%", s.engine_volume),
            Row::Effects => format!("{}%", s.effects_volume),
            Row::Sound => on_off(!data.muted),
            Row::Fullscreen => on_off(s.fullscreen),
            Row::PixelSize => s.pixel_size.label().to_string(),
            Row::Camera => if s.wide_camera { "WIDE" } else { "NORMAL" }.to_string(),
            Row::Bind(action) => {
                let key = key_label(s.keys.key(action));
                match action.alt_key() {
                    Some(alt) if alt != s.keys.key(action) => format!("{key} / {}", key_label(alt)),
                    _ => key,
                }
            }
            Row::ResetKeys => String::new(),
        }
    }

    /// Changes the row by one step in `dir` (left -1, right +1). Returns
    /// whether anything changed.
    pub fn adjust(self, data: &mut SaveData, dir: i32) -> bool {
        let before = data.clone();
        let s = &mut data.settings;
        match self {
            Row::Master => s.master_volume = step_volume(s.master_volume, dir),
            Row::Engine => s.engine_volume = step_volume(s.engine_volume, dir),
            Row::Effects => s.effects_volume = step_volume(s.effects_volume, dir),
            Row::Sound => data.muted = !data.muted,
            Row::Fullscreen => s.fullscreen = !s.fullscreen,
            Row::PixelSize => s.pixel_size = s.pixel_size.stepped(dir),
            Row::Camera => s.wide_camera = !s.wide_camera,
            Row::Bind(_) | Row::ResetKeys => {}
        }
        *data != before
    }
}

/// Message for a refused key.
pub fn bind_error_text(key: &str, err: BindError) -> String {
    match err {
        BindError::Unknown => "THAT KEY CANNOT BE USED".to_string(),
        BindError::Reserved => format!("{} IS RESERVED", key_label(key)),
        BindError::AltOf(other) => format!("{} IS THE ALT KEY FOR {}", key_label(key), other.label()),
    }
}

// ---------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct SettingsMenu {
    row: usize,
    /// Waiting for a key for this action.
    capturing: Option<Action>,
    /// Feedback line and whether it is good news.
    message: Option<(String, bool)>,
}

#[derive(Component)]
struct SettingsRoot;

#[derive(Component)]
struct RowBox(usize);

#[derive(Component)]
struct RowValue(usize);

#[derive(Component)]
struct StatusLine;

const ROW_WIDTH: f32 = 600.0;

fn spawn_settings(mut commands: Commands, font: Res<GameFont>, pal: Res<UiPalette>) {
    commands.insert_resource(SettingsMenu::default());
    let label = |text: String, size: f32, color: Color| (Text::new(text), font.text(size), TextColor(color));
    commands
        .spawn((
            SettingsRoot,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                padding: UiRect::all(px(24)),
                ..default()
            },
            BackgroundColor(pal.background),
        ))
        .with_children(|root| {
            root.spawn(label("SETTINGS".into(), 24.0, pal.accent));
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(px(20), px(12)),
                    row_gap: px(2),
                    ..default()
                },
                BackgroundColor(pal.panel),
            ))
            .with_children(|panel| {
                for (i, row) in ROWS.iter().enumerate() {
                    if let Some((_, title)) = SECTIONS.iter().find(|(at, _)| *at == i) {
                        panel.spawn((
                            label((*title).into(), 8.0, pal.muted),
                            Node { margin: UiRect::new(px(0), px(0), px(if i == 0 { 2 } else { 12 }), px(4)), ..default() },
                        ));
                    }
                    panel
                        .spawn((
                            RowBox(i),
                            Node {
                                width: px(ROW_WIDTH),
                                justify_content: JustifyContent::SpaceBetween,
                                padding: UiRect::axes(px(10), px(6)),
                                ..default()
                            },
                            BackgroundColor(pal.panel),
                        ))
                        .with_children(|r| {
                            r.spawn(label(row.label().into(), 12.0, pal.foreground));
                            r.spawn((RowValue(i), label(String::new(), 12.0, pal.foreground)));
                        });
                }
            });
            root.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(10), ..default() })
                .with_children(|foot| {
                    foot.spawn((StatusLine, label(String::new(), 8.0, pal.good)));
                    foot.spawn(label("^/v SELECT   </> CHANGE   ENTER REBIND/RESET   ESC BACK".into(), 8.0, pal.muted));
                });
        });
}

fn despawn_settings(mut commands: Commands, q: Query<Entity, With<SettingsRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<SettingsMenu>();
}

fn settings_input(
    keys: Res<ButtonInput<KeyCode>>,
    menu: Option<ResMut<SettingsMenu>>,
    mut profile: ResMut<Profile>,
    mut next: ResMut<NextState<AppState>>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let Some(mut menu) = menu else { return };

    if let Some(action) = menu.capturing {
        let Some(&code) = keys.get_just_pressed().next() else { return };
        menu.capturing = None;
        if code == KeyCode::Escape {
            menu.message = Some(("REBIND CANCELLED".into(), true));
            sfx.write(PlaySfx(Sfx::MenuMove));
            return;
        }
        let Some(name) = key_name(code) else {
            menu.message = Some((bind_error_text("", BindError::Unknown), false));
            sfx.write(PlaySfx(Sfx::Denied));
            return;
        };
        match profile.data.settings.keys.assign(action, name) {
            Ok(()) => {
                profile.persist();
                menu.message = Some((format!("{}: {}", action.label(), key_label(name)), true));
                sfx.write(PlaySfx(Sfx::MenuSelect));
            }
            Err(err) => {
                menu.message = Some((bind_error_text(name, err), false));
                sfx.write(PlaySfx(Sfx::Denied));
            }
        }
        return;
    }

    let any = |codes: &[KeyCode]| keys.any_just_pressed(codes.iter().copied());
    let row = ROWS[menu.row];
    if any(&[KeyCode::ArrowUp, KeyCode::KeyW]) {
        menu.row = super::title::wrap_index(menu.row, -1, ROWS.len());
        sfx.write(PlaySfx(Sfx::MenuMove));
    } else if any(&[KeyCode::ArrowDown, KeyCode::KeyS]) {
        menu.row = super::title::wrap_index(menu.row, 1, ROWS.len());
        sfx.write(PlaySfx(Sfx::MenuMove));
    } else if any(&[KeyCode::ArrowLeft, KeyCode::KeyA, KeyCode::ArrowRight, KeyCode::KeyD, KeyCode::Enter, KeyCode::NumpadEnter]) {
        let dir = if any(&[KeyCode::ArrowLeft, KeyCode::KeyA]) { -1 } else { 1 };
        let enter = any(&[KeyCode::Enter, KeyCode::NumpadEnter]);
        match row {
            Row::Bind(action) if enter => {
                menu.capturing = Some(action);
                menu.message = None;
                sfx.write(PlaySfx(Sfx::MenuSelect));
            }
            Row::ResetKeys if enter => {
                if profile.data.settings.keys != KeyBindings::default() {
                    profile.data.settings.keys = KeyBindings::default();
                    profile.persist();
                }
                menu.message = Some(("CONTROLS RESET TO DEFAULTS".into(), true));
                sfx.write(PlaySfx(Sfx::MenuSelect));
            }
            _ => {
                if row.adjust(&mut profile.data, dir) {
                    profile.persist();
                    menu.message = None;
                    sfx.write(PlaySfx(Sfx::MenuMove));
                }
            }
        }
    } else if any(&[KeyCode::Escape, KeyCode::KeyO, KeyCode::Backspace]) {
        sfx.write(PlaySfx(Sfx::MenuSelect));
        next.set(AppState::Title);
    }
}

#[allow(clippy::too_many_arguments)]
fn refresh_settings(
    menu: Option<Res<SettingsMenu>>,
    profile: Res<Profile>,
    pal: Res<UiPalette>,
    mut boxes: Query<(&RowBox, &mut BackgroundColor)>,
    mut values: Query<(&RowValue, &mut Text, &mut TextColor), Without<StatusLine>>,
    mut status: Query<(&mut Text, &mut TextColor), With<StatusLine>>,
) {
    let Some(menu) = menu else { return };
    if !menu.is_changed() && !profile.is_changed() {
        return;
    }
    for (b, mut bg) in &mut boxes {
        let c = if b.0 == menu.row { pal.panel_hi } else { pal.panel };
        if bg.0 != c {
            bg.0 = c;
        }
    }
    for (v, mut text, mut color) in &mut values {
        let row = ROWS[v.0];
        let selected = v.0 == menu.row;
        let capturing = selected && matches!(row, Row::Bind(a) if menu.capturing == Some(a));
        text.0 = if capturing {
            "PRESS A KEY  (ESC CANCELS)".into()
        } else {
            let value = row.value(&profile.data);
            let adjustable = !matches!(row, Row::Bind(_) | Row::ResetKeys);
            if selected && adjustable { format!("< {value} >") } else { value }
        };
        color.0 = if capturing || selected { pal.accent } else { pal.foreground };
    }
    for (mut text, mut color) in &mut status {
        let (msg, good) = menu.message.clone().unwrap_or_default();
        text.0 = msg;
        color.0 = if good { pal.good } else { pal.bad };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::settings::PixelSize;

    #[test]
    fn audio_follows_settings_and_mute() {
        let s = Settings { master_volume: 50, engine_volume: 20, ..Settings::default() };
        let a = audio_for(&s, true);
        assert!(a.muted);
        assert!((a.engine - 0.1).abs() < 1e-6);
        assert!((a.effects - 0.5).abs() < 1e-6);
        assert_eq!(audio_for(&Settings::default(), false), AudioSettings::default());
    }

    #[test]
    fn fullscreen_is_borderless_on_the_current_monitor() {
        assert_eq!(window_mode(false), WindowMode::Windowed);
        assert_eq!(window_mode(true), WindowMode::BorderlessFullscreen(MonitorSelection::Current));
    }

    #[test]
    fn every_action_has_exactly_one_row_and_sections_are_ordered() {
        for a in Action::ALL {
            assert_eq!(ROWS.iter().filter(|r| **r == Row::Bind(a)).count(), 1);
        }
        assert!(SECTIONS.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(SECTIONS.iter().all(|(i, _)| *i < ROWS.len()));
    }

    #[test]
    fn adjusting_rows_changes_the_save() {
        let mut d = SaveData::default();
        assert!(Row::Master.adjust(&mut d, 1));
        assert_eq!(d.settings.master_volume, 80);
        assert_eq!(Row::Master.value(&d), "80%");
        assert!(Row::Sound.adjust(&mut d, -1));
        assert!(d.muted);
        assert_eq!(Row::Sound.value(&d), "OFF");
        assert!(Row::Fullscreen.adjust(&mut d, 1));
        assert_eq!(Row::Fullscreen.value(&d), "ON");
        assert!(Row::PixelSize.adjust(&mut d, 1));
        assert_eq!(d.settings.pixel_size, PixelSize::Larger);
        assert!(!Row::PixelSize.adjust(&mut d, 1), "already at the largest size");
        assert!(Row::Camera.adjust(&mut d, 1));
        assert_eq!(Row::Camera.value(&d), "WIDE");
        assert!(!Row::Bind(Action::Drift).adjust(&mut d, 1));
    }

    #[test]
    fn binding_rows_show_key_and_alternate() {
        let mut d = SaveData::default();
        assert_eq!(Row::Bind(Action::Throttle).value(&d), "W / UP");
        assert_eq!(Row::Bind(Action::Drift).value(&d), "SPACE");
        d.settings.keys.assign(Action::Throttle, "ArrowUp").unwrap();
        assert_eq!(Row::Bind(Action::Throttle).value(&d), "UP");
    }

    #[test]
    fn refused_keys_explain_why() {
        assert_eq!(bind_error_text("KeyR", BindError::Reserved), "R IS RESERVED");
        assert_eq!(
            bind_error_text("ArrowDown", BindError::AltOf(Action::Brake)),
            "DOWN IS THE ALT KEY FOR BRAKE / REVERSE"
        );
    }
}
