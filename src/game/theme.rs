//! Per-circuit world palettes. The race world keeps its own colors so each
//! circuit reads clearly regardless of the desktop theme.

use bevy::prelude::*;

use crate::core::tracks::TrackId;

use super::palette::hex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropKind {
    Tree,
    Cactus,
    Container,
    Pine,
}

#[derive(Debug, Clone)]
pub struct WorldTheme {
    pub grass: Color,
    pub grass_dark: Color,
    pub grass_light: Color,
    pub gravel: Color,
    pub road: Color,
    pub road_edge: Color,
    pub line: Color,
    pub kerb_a: Color,
    pub kerb_b: Color,
    pub barrier: Color,
    pub barrier_top: Color,
    pub prop: PropKind,
    pub prop_main: Color,
    pub prop_dark: Color,
    pub prop_accent: Color,
    pub shadow: Color,
    pub skid: Color,
    pub dust: Color,
}

pub fn world_theme(id: TrackId) -> WorldTheme {
    match id {
        TrackId::OrchardRing => WorldTheme {
            grass: hex("#5a9a3c"),
            grass_dark: hex("#4b8634"),
            grass_light: hex("#6aae46"),
            gravel: hex("#cdb989"),
            road: hex("#555a64"),
            road_edge: hex("#474b54"),
            line: hex("#e9e6da"),
            kerb_a: hex("#d8433b"),
            kerb_b: hex("#f1efe6"),
            barrier: hex("#2e3138"),
            barrier_top: hex("#5b616d"),
            prop: PropKind::Tree,
            prop_main: hex("#2f6e2c"),
            prop_dark: hex("#23541f"),
            prop_accent: hex("#e05a4f"),
            shadow: Color::srgba(0.0, 0.0, 0.0, 0.25),
            skid: Color::srgba(0.1, 0.1, 0.12, 0.35),
            dust: hex("#d9cfa9"),
        },
        TrackId::DuneSprint => WorldTheme {
            grass: hex("#d9a95b"),
            grass_dark: hex("#c8954a"),
            grass_light: hex("#e6ba6c"),
            gravel: hex("#b8783f"),
            road: hex("#5e5550"),
            road_edge: hex("#4f4743"),
            line: hex("#f4e9cf"),
            kerb_a: hex("#2f7fc1"),
            kerb_b: hex("#f4efe1"),
            barrier: hex("#6b3f24"),
            barrier_top: hex("#9a6038"),
            prop: PropKind::Cactus,
            prop_main: hex("#4f8a4a"),
            prop_dark: hex("#3c6c39"),
            prop_accent: hex("#a0643a"),
            shadow: Color::srgba(0.2, 0.08, 0.0, 0.25),
            skid: Color::srgba(0.15, 0.1, 0.08, 0.35),
            dust: hex("#f0d49a"),
        },
        TrackId::NeonDocks => WorldTheme {
            grass: hex("#1c2233"),
            grass_dark: hex("#171c2b"),
            grass_light: hex("#232a3e"),
            gravel: hex("#2c3348"),
            road: hex("#3a3f52"),
            road_edge: hex("#30354a"),
            line: hex("#5ff2e0"),
            kerb_a: hex("#ff3fa4"),
            kerb_b: hex("#1d1f2b"),
            barrier: hex("#0e1019"),
            barrier_top: hex("#ffb400"),
            prop: PropKind::Container,
            prop_main: hex("#c2463d"),
            prop_dark: hex("#2f6f9a"),
            prop_accent: hex("#e8b13a"),
            shadow: Color::srgba(0.0, 0.0, 0.0, 0.35),
            skid: Color::srgba(0.0, 0.0, 0.0, 0.4),
            dust: hex("#6e7896"),
        },
        TrackId::GlacierPass => WorldTheme {
            grass: hex("#e4eef4"),
            grass_dark: hex("#d2e1ea"),
            grass_light: hex("#f3f8fb"),
            gravel: hex("#a9bfcc"),
            road: hex("#5b6676"),
            road_edge: hex("#4d5766"),
            line: hex("#ffe27a"),
            kerb_a: hex("#2a63b8"),
            kerb_b: hex("#f6f9fb"),
            barrier: hex("#39424f"),
            barrier_top: hex("#c84a3c"),
            prop: PropKind::Pine,
            prop_main: hex("#2c5a4e"),
            prop_dark: hex("#1f4239"),
            prop_accent: hex("#ffffff"),
            shadow: Color::srgba(0.1, 0.15, 0.25, 0.25),
            skid: Color::srgba(0.2, 0.25, 0.3, 0.35),
            dust: hex("#ffffff"),
        },
    }
}
