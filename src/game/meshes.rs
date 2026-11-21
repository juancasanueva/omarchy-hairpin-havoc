//! Small helpers to build vertex-colored 2D triangle meshes.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Accumulates colored triangles, then converts to a `Mesh`.
#[derive(Default)]
pub struct MeshBuilder {
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    pub fn quad(&mut self, a: Vec2, b: Vec2, c: Vec2, d: Vec2, color: Color) {
        let base = self.positions.len() as u32;
        let col = LinearRgba::from(color).to_f32_array();
        for p in [a, b, c, d] {
            self.positions.push([p.x, p.y, 0.0]);
            self.colors.push(col);
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub fn tri(&mut self, a: Vec2, b: Vec2, c: Vec2, color: Color) {
        let base = self.positions.len() as u32;
        let col = LinearRgba::from(color).to_f32_array();
        for p in [a, b, c] {
            self.positions.push([p.x, p.y, 0.0]);
            self.colors.push(col);
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    /// Oriented rectangle centered at `c`, with `dir` along its length.
    pub fn rect(&mut self, c: Vec2, dir: Vec2, half_len: f32, half_wid: f32, color: Color) {
        let f = dir.normalize_or(Vec2::X) * half_len;
        let s = dir.normalize_or(Vec2::X).perp() * half_wid;
        self.quad(c - f - s, c + f - s, c + f + s, c - f + s, color);
    }

    pub fn circle(&mut self, c: Vec2, r: f32, segments: usize, color: Color) {
        for i in 0..segments {
            let a0 = i as f32 / segments as f32 * std::f32::consts::TAU;
            let a1 = (i + 1) as f32 / segments as f32 * std::f32::consts::TAU;
            self.tri(c, c + Vec2::from_angle(a0) * r, c + Vec2::from_angle(a1) * r, color);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn build(self) -> Mesh {
        let normals = vec![[0.0, 0.0, 1.0]; self.positions.len()];
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
            .with_inserted_indices(Indices::U32(self.indices))
    }
}
