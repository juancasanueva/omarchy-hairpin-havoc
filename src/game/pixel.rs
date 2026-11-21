//! Pixel-perfect rendering: the world is drawn by a camera into a low-resolution
//! image, which an outer camera upscales by an integer factor to fill the window.
//! UI is drawn by the outer camera at native resolution.

use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::window::{PrimaryWindow, WindowResized};

/// Layer for everything inside the pixel world.
pub const WORLD_LAYER: RenderLayers = RenderLayers::layer(0);
/// Layer for the upscaled canvas sprite.
pub const CANVAS_LAYER: RenderLayers = RenderLayers::layer(1);

/// Target geometric-mean size of the pixel canvas (about 480x270 on 16:9 screens).
pub const TARGET_SIZE: f32 = 400.0;

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
    material: Handle<ColorMaterial>,
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

/// Integer scale and canvas size for a window, keeping the canvas area near
/// `TARGET_SIZE`² so tall tiled windows still show enough of the track.
pub fn canvas_for_window(width: f32, height: f32) -> (u32, UVec2) {
    let scale = ((width * height).sqrt() / TARGET_SIZE).round().clamp(1.0, 8.0) as u32;
    let w = (width / scale as f32).ceil().max(1.0) as u32;
    let h = (height / scale as f32).ceil().max(1.0) as u32;
    // Even sizes keep the canvas centered on whole pixels.
    (scale, UVec2::new(w + (w & 1), h + (h & 1)))
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
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
    // A textured quad (rather than a sprite) shows the canvas; its scale tracks the canvas size.
    let material = materials.add(ColorMaterial {
        texture: Some(image.clone()),
        ..default()
    });
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial2d(material.clone()),
        Transform::from_scale(size.as_vec2().extend(1.0)),
        Canvas,
        CANVAS_LAYER,
    ));
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
    commands.insert_resource(PixelCanvas {
        image,
        material,
        scale,
        size,
    });
}

fn fit_canvas(
    mut resized: MessageReader<WindowResized>,
    mut canvas: ResMut<PixelCanvas>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut projection: Single<&mut Projection, With<OuterCamera>>,
    mut quad: Single<&mut Transform, With<Canvas>>,
    mut target: Single<&mut RenderTarget, With<WorldCamera>>,
) {
    let Some(last) = resized.read().last() else {
        return;
    };
    let (scale, size) = canvas_for_window(last.width, last.height);
    if size != canvas.size {
        // Resizing a live render target in place leaves it blank, so swap in a new image.
        let image = images.add(Image::new_target_texture(
            size.x,
            size.y,
            TextureFormat::Bgra8UnormSrgb,
            None,
        ));
        let old = std::mem::replace(&mut canvas.image, image.clone());
        images.remove(&old);
        **target = RenderTarget::Image(image.clone().into());
        if let Some(mut mat) = materials.get_mut(&canvas.material) {
            mat.texture = Some(image);
        }
        canvas.size = size;
        quad.scale = size.as_vec2().extend(1.0);
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
    fn integer_scale_tracks_window_area() {
        assert_eq!(canvas_for_window(1280.0, 720.0).0, 2);
        assert_eq!(canvas_for_window(1920.0, 1080.0).0, 4);
        assert_eq!(canvas_for_window(2560.0, 1440.0).0, 5);
        assert_eq!(canvas_for_window(1313.0, 1386.0).0, 3);
        assert_eq!(canvas_for_window(300.0, 100.0).0, 1);
    }

    #[test]
    fn canvas_covers_window() {
        let (s, size) = canvas_for_window(1921.0, 1080.0);
        assert!(size.x * s >= 1921 && size.y * s >= 1080);
        assert_eq!(size.x % 2, 0);
    }
}
