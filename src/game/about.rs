//! About screen: version, source repository, license, author and credits,
//! with keys to open the repository in the browser or copy its link.

use std::process::{Command, Stdio};

use bevy::prelude::*;

use super::AppState;
use super::audio::{PlaySfx, Sfx};
use super::palette::UiPalette;
use super::title::GameFont;
use super::trial::clipboard_write;

/// Source repository, from `Cargo.toml`.
pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const LICENSE: &str = env!("CARGO_PKG_LICENSE");
pub const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

/// The repository link without its scheme, for display.
pub fn repo_display(url: &str) -> &str {
    url.split_once("://").map_or(url, |(_, rest)| rest)
}

/// Label and value of every About row, in display order.
pub fn about_rows() -> Vec<(&'static str, String)> {
    vec![
        ("VERSION", VERSION.to_string()),
        ("SOURCE", repo_display(REPOSITORY).to_string()),
        ("LICENSE", LICENSE.to_string()),
        ("AUTHOR", AUTHORS.replace(':', ", ").to_uppercase()),
        ("ENGINE", "RUST + BEVY".to_string()),
        ("FONT", "PRESS START 2P (SIL OFL 1.1)".to_string()),
        ("MADE FOR", "OMARCHY".to_string()),
    ]
}

#[derive(Component)]
struct AboutRoot;

/// Feedback line for the open/copy actions.
#[derive(Component)]
struct AboutStatus;

pub struct AboutPlugin;

impl Plugin for AboutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::About), spawn_about)
            .add_systems(OnExit(AppState::About), despawn_about)
            .add_systems(Update, about_input.run_if(in_state(AppState::About)));
    }
}

fn spawn_about(mut commands: Commands, font: Res<GameFont>, pal: Res<UiPalette>) {
    let label = |text: String, size: f32, color: Color| (Text::new(text), font.text(size), TextColor(color));
    commands
        .spawn((
            AboutRoot,
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
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(12),
                ..default()
            })
            .with_children(|head| {
                head.spawn(label("HAIRPIN HAVOC".into(), 32.0, pal.accent));
                head.spawn(label("A TOP-DOWN PIXEL RACER FOR OMARCHY".into(), 8.0, pal.muted));
            });

            root.spawn((
                Node {
                    display: Display::Grid,
                    grid_template_columns: vec![GridTrack::auto(), GridTrack::auto()],
                    column_gap: px(28),
                    row_gap: px(18),
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(px(32), px(24)),
                    ..default()
                },
                BackgroundColor(pal.panel),
            ))
            .with_children(|grid| {
                for (name, value) in about_rows() {
                    grid.spawn(label(name.into(), 8.0, pal.muted));
                    let color = if name == "SOURCE" { pal.accent } else { pal.foreground };
                    grid.spawn(label(value, 12.0, color));
                }
            });

            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(14),
                ..default()
            })
            .with_children(|foot| {
                foot.spawn((AboutStatus, label(String::new(), 8.0, pal.good)));
                foot.spawn(label("ENTER OPEN LINK   C COPY LINK   ESC BACK".into(), 8.0, pal.muted));
            });
        });
}

fn despawn_about(mut commands: Commands, q: Query<Entity, With<AboutRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

/// Opens `url` in the default browser without blocking the game.
fn open_url(url: &str) -> bool {
    let child = Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match child {
        Ok(mut child) => {
            // Reap it in the background so no zombie is left behind.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
}

fn about_input(
    keys: Res<ButtonInput<KeyCode>>,
    pal: Res<UiPalette>,
    mut status: Query<(&mut Text, &mut TextColor), With<AboutStatus>>,
    mut next: ResMut<NextState<AppState>>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let result = if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        Some(if open_url(REPOSITORY) {
            Ok("OPENING IN YOUR BROWSER")
        } else {
            Err("COULD NOT OPEN A BROWSER (XDG-OPEN)")
        })
    } else if keys.just_pressed(KeyCode::KeyC) {
        Some(match clipboard_write(REPOSITORY) {
            Ok(()) => Ok("LINK COPIED TO CLIPBOARD"),
            Err(_) => Err("CLIPBOARD UNAVAILABLE (WL-COPY)"),
        })
    } else {
        if keys.any_just_pressed([KeyCode::Escape, KeyCode::KeyI, KeyCode::Backspace]) {
            sfx.write(PlaySfx(Sfx::MenuSelect));
            next.set(AppState::Title);
        }
        None
    };
    let Some(result) = result else { return };
    let (msg, color, sound) = match result {
        Ok(msg) => (msg, pal.good, Sfx::MenuSelect),
        Err(msg) => (msg, pal.bad, Sfx::Denied),
    };
    sfx.write(PlaySfx(sound));
    for (mut text, mut tc) in &mut status {
        text.0 = msg.to_string();
        tc.0 = color;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str) -> String {
        about_rows().into_iter().find(|(n, _)| *n == name).map(|(_, v)| v).unwrap()
    }

    #[test]
    fn rows_come_from_the_package_metadata() {
        assert_eq!(row("VERSION"), env!("CARGO_PKG_VERSION"));
        assert_eq!(row("LICENSE"), "MIT");
        assert_eq!(row("SOURCE"), "github.com/juancasanueva/omarchy-hairpin-havoc");
        assert!(!row("AUTHOR").is_empty());
    }

    #[test]
    fn repo_display_drops_the_scheme_only() {
        assert_eq!(repo_display("https://github.com/a/b"), "github.com/a/b");
        assert_eq!(repo_display("github.com/a/b"), "github.com/a/b");
    }

    #[test]
    fn repository_is_an_https_link() {
        assert!(REPOSITORY.starts_with("https://"));
    }
}
