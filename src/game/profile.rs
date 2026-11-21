//! The player's persistent profile (credits, garage, best laps) as a resource.

use std::path::PathBuf;

use bevy::prelude::*;

use crate::core::save::{self, SaveData};

#[derive(Resource)]
pub struct Profile {
    pub data: SaveData,
    dir: Option<PathBuf>,
}

impl Profile {
    /// Write the profile to disk; failures are logged, never fatal.
    pub fn persist(&self) {
        let Some(dir) = &self.dir else { return };
        if let Err(e) = save::store(dir, &self.data) {
            warn!("could not save profile: {e}");
        }
    }

    pub fn dir(&self) -> Option<&PathBuf> {
        self.dir.as_ref()
    }
}

pub struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        let dir = save::save_dir();
        let data = dir.as_deref().map(save::load).unwrap_or_default();
        app.insert_resource(Profile { data, dir });
    }
}
