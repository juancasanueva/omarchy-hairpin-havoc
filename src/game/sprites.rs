//! Procedural top-down pixel car sprites. Cars point along +X.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::core::cars::BodyStyle;

struct Shape {
    len: u32,
    wid: u32,
    /// Front taper in px per side over the last quarter.
    taper: f32,
    spoiler: bool,
    double_stripe: bool,
}

fn shape(style: BodyStyle) -> Shape {
    let s = |len, wid, taper, spoiler, double_stripe| Shape { len, wid, taper, spoiler, double_stripe };
    match style {
        BodyStyle::Hatch => s(28, 16, 1.0, false, false),
        BodyStyle::Coupe => s(31, 16, 1.5, false, false),
        BodyStyle::Rally => s(30, 17, 1.0, true, false),
        BodyStyle::Sports => s(33, 16, 2.0, true, false),
        BodyStyle::Wedge => s(34, 16, 3.5, true, false),
        BodyStyle::Tourer => s(35, 17, 1.5, false, false),
        BodyStyle::Muscle => s(34, 18, 1.0, false, true),
    }
}

fn shade(c: [u8; 3], f: f32) -> [u8; 4] {
    let m = |v: u8| ((v as f32 * f).clamp(0.0, 255.0)) as u8;
    [m(c[0]), m(c[1]), m(c[2]), 255]
}

/// Build the sprite image for a car body style and paint.
pub fn car_image(style: BodyStyle, body: [u8; 3], accent: [u8; 3]) -> Image {
    let sh = shape(style);
    let (w, h) = (sh.len as i32, sh.wid as i32);
    let mut px = vec![[0u8; 4]; (w * h) as usize];
    let mut put = |x: i32, y: i32, c: [u8; 4]| {
        if x >= 0 && y >= 0 && x < w && y < h {
            // Image rows run top to bottom; flip so +Y in world is the top row.
            px[((h - 1 - y) * w + x) as usize] = c;
        }
    };

    let tire = [21, 22, 26, 255];
    let glass = [30, 42, 58, 255];
    let glass_hi = [92, 118, 150, 255];
    for (x0, x1) in [(4, 9), (w - 10, w - 5)] {
        for x in x0..=x1 {
            for y in [0, 1, 2, h - 3, h - 2, h - 1] {
                put(x, y, tire);
            }
        }
    }

    // Body mask with rounded corners and front taper.
    let inset_at = |x: i32| -> i32 {
        let q = w as f32 * 0.75;
        let taper = if (x as f32) > q { (x as f32 - q) / (w as f32 - q) * sh.taper } else { 0.0 };
        let corner = match x {
            0 | 1 => 2,
            _ if x >= w - 2 => 2,
            _ => 1,
        };
        corner.max(1 + taper.round() as i32)
    };
    let inside = |x: i32, y: i32| -> bool {
        if x < 1 || x > w - 2 {
            return false;
        }
        let i = inset_at(x);
        y >= i && y <= h - 1 - i
    };
    for x in 0..w {
        for y in 0..h {
            if !inside(x, y) {
                continue;
            }
            let edge = !inside(x - 1, y) || !inside(x + 1, y) || !inside(x, y - 1) || !inside(x, y + 1);
            let light = if y >= h / 2 { 1.08 } else { 0.94 };
            put(x, y, if edge { shade(body, 0.55) } else { shade(body, light) });
        }
    }

    // Stripes.
    let stripe_rows: Vec<i32> = if sh.double_stripe {
        vec![h / 2 - 3, h / 2 - 2, h / 2 + 1, h / 2 + 2]
    } else {
        vec![h / 2 - 1, h / 2]
    };
    for x in 2..w - 2 {
        for &y in &stripe_rows {
            if inside(x, y) && inside(x, y - 1) && inside(x, y + 1) {
                put(x, y, shade(accent, 1.0));
            }
        }
    }

    // Cabin: rear window, roof, windshield.
    let at = |f: f32| (w as f32 * f) as i32;
    for x in at(0.28)..at(0.36) {
        for y in 3..h - 3 {
            put(x, y, glass);
        }
    }
    for x in at(0.36)..at(0.58) {
        for y in 3..h - 3 {
            let on_stripe = stripe_rows.contains(&y);
            put(x, y, if on_stripe { shade(accent, 1.15) } else { shade(body, 1.18) });
        }
    }
    for x in at(0.58)..at(0.70) {
        for y in 3..h - 3 {
            put(x, y, glass);
        }
    }
    put(at(0.66), h - 5, glass_hi);
    put(at(0.64), h - 5, glass_hi);

    // Lights.
    for y in [2, 3, h - 4, h - 3] {
        if inside(w - 2, y) {
            put(w - 2, y, [255, 243, 196, 255]);
        }
        put(1, y, [210, 58, 58, 255]);
    }
    if sh.spoiler {
        for y in 1..h - 1 {
            put(0, y, shade(accent, 0.6));
            put(1, y, shade(accent, 0.8));
        }
    }

    let data: Vec<u8> = px.into_iter().flatten().collect();
    Image::new(
        Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_style_builds_a_sprite_with_opaque_body() {
        for style in [
            BodyStyle::Hatch,
            BodyStyle::Coupe,
            BodyStyle::Rally,
            BodyStyle::Sports,
            BodyStyle::Wedge,
            BodyStyle::Tourer,
            BodyStyle::Muscle,
        ] {
            let img = car_image(style, [200, 50, 50], [240, 240, 240]);
            let data = img.data.as_ref().unwrap();
            let opaque = data.chunks(4).filter(|p| p[3] == 255).count();
            let total = data.len() / 4;
            assert!(opaque * 2 > total, "{style:?} sprite mostly empty");
        }
    }
}
