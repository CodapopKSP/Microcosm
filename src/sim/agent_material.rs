//! Agent mesh material: uses Bevy's transparent 2D sort with a per-lineage [`Material2d::depth_bias`].

use bevy::color::{ColorToComponents, LinearRgba};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{AsBindGroup, AsBindGroupShaderType, ShaderRef};
use bevy::render::texture::GpuImage;
use bevy::sprite::{
    AlphaMode2d, ColorMaterialFlags, ColorMaterialUniform, Material2d, Material2dPlugin,
    COLOR_MATERIAL_SHADER_HANDLE,
};

/// Solid agent fill; sorted by [`lineage_id`](Self::lineage_id) via `depth_bias` (not opaque depth).
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
#[uniform(0, ColorMaterialUniform)]
pub struct AgentColorMaterial {
    pub color: Color,
    pub lineage_id: u64,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Option<Handle<Image>>,
}

impl AgentColorMaterial {
    pub fn new(color: Color, lineage_id: u64) -> Self {
        Self {
            color,
            lineage_id,
            texture: None,
        }
    }
}

impl AsBindGroupShaderType<ColorMaterialUniform> for AgentColorMaterial {
    fn as_bind_group_shader_type(&self, _images: &RenderAssets<GpuImage>) -> ColorMaterialUniform {
        let mut flags = ColorMaterialFlags::NONE;
        if self.texture.is_some() {
            flags |= ColorMaterialFlags::TEXTURE;
        }
        flags |= ColorMaterialFlags::ALPHA_MODE_BLEND;
        ColorMaterialUniform {
            color: LinearRgba::from(self.color).to_f32_array().into(),
            flags: flags.bits(),
            alpha_cutoff: 0.5,
        }
    }
}

impl Material2d for AgentColorMaterial {
    fn fragment_shader() -> ShaderRef {
        COLOR_MATERIAL_SHADER_HANDLE.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    /// Drives `Transparent2d` sort key (`mesh_z + depth_bias`). Lower values draw first (behind).
    fn depth_bias(&self) -> f32 {
        self.lineage_id as f32
    }
}

/// Registers [`AgentColorMaterial`] with the renderer (visual app only).
pub struct AgentColorMaterialPlugin;

impl Plugin for AgentColorMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<AgentColorMaterial>::default())
            .register_asset_reflect::<AgentColorMaterial>();
    }
}
