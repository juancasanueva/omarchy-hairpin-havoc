//! Pixel-perfect rendering: the world is drawn by a camera into a low-resolution
//! image, which an outer camera upscales by an integer factor to fill the window.
//! UI is drawn by the outer camera at native resolution.

use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureFormat};
use bevy::window::{PrimaryWindow, WindowResized};

/// Layer for everything inside the pixel world.
pub const WORLD_LAYER: RenderLayers = RenderLayers::layer(0);
/// Layer for the upscaled canvas sprite.
pub const CANVAS_LAYER: RenderLayers = RenderLayers::layer(1);

/// Target height of the pixel canvas; actual height depends on the integer scale.
pub const TARGET_HEIGHT: f32 = 300.0;

/// Camera that renders the world into the low-res canvas. Move this to follow cars.
#[derive(Component)]
pub struct WorldCamera;

#[derive(Component)]
pub struct OuterCamera;

#[derive(Component)]
struct Canvas;

#[derive(Resource)]
pub struct PixelCanvas {
    pub image: Handle<Image>,
    pub scale: u32,
    pub size: UVec2,
}

pub struct PixelPlugin;

impl Plugin for PixelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(PreUpdate, fit_canvas);
    }
}

/// Integer scale and canvas size for a window, keeping the canvas near `TARGET_HEIGHT`.
pub fn canvas_for_window(width: f32, height: f32) -> (u32, UVec2) {
    let scale = (height / TARGET_HEIGHT).round().clamp(1.0, 8.0) as u32;
    let w = (width / scale as f32).ceil().max(1.0) as u32;
    let h = (height / scale as f32).ceil().max(1.0) as u32;
    // Even sizes keep the canvas centered on whole pixels.
    (scale, UVec2::new(w + (w & 1), h + (h & 1)))
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
) {
    // The window may not exist yet; `fit_canvas` corrects the size on the first resize.
    let (w, h) = window.map_or((1280.0, 720.0), |w| (w.width(), w.height()));
    let (scale, size) = canvas_for_window(w, h);
    let canvas = Image::new_target_texture(size.x, size.y, TextureFormat::Bgra8UnormSrgb, None);
    let image = images.add(canvas);

    commands.spawn((
        Camera2d,
        Camera {
            order: -1,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.05, 0.06, 0.08)),
            ..default()
        },
        RenderTarget::Image(image.clone().into()),
        Msaa::Off,
        WorldCamera,
        WORLD_LAYER,
    ));
    commands.spawn((Sprite::from_image(image.clone()), Canvas, CANVAS_LAYER));
    commands.spawn((
        Camera2d,
        Msaa::Off,
        OuterCamera,
        IsDefaultUiCamera,
        Projection::Orthographic(OrthographicProjection {
            scale: 1.0 / scale as f32,
            ..OrthographicProjection::default_2d()
        }),
        CANVAS_LAYER,
    ));
    commands.insert_resource(PixelCanvas { image, scale, size });
}

fn fit_canvas(
    mut resized: MessageReader<WindowResized>,
    mut canvas: ResMut<PixelCanvas>,
    mut images: ResMut<Assets<Image>>,
    mut projection: Single<&mut Projection, With<OuterCamera>>,
) {
    let Some(last) = resized.read().last() else {
        return;
    };
    let (scale, size) = canvas_for_window(last.width, last.height);
    if size != canvas.size {
        if let Some(mut img) = images.get_mut(&canvas.image) {
            img.resize(Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            });
        }
        canvas.size = size;
    }
    canvas.scale = scale;
    if let Projection::Orthographic(p) = &mut **projection {
        p.scale = 1.0 / scale as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_scale_tracks_window_height() {
        assert_eq!(canvas_for_window(1280.0, 720.0).0, 2);
        assert_eq!(canvas_for_window(2560.0, 1440.0).0, 5);
        assert_eq!(canvas_for_window(300.0, 100.0).0, 1);
    }

    #[test]
    fn canvas_covers_window() {
        let (s, size) = canvas_for_window(1921.0, 1080.0);
        assert!(size.x * s >= 1921 && size.y * s >= 1080);
        assert_eq!(size.x % 2, 0);
    }
}
