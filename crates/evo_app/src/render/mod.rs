//! Rendering: a background material driven by the environment fields, and one dynamic mesh
//! holding a quad per visible organism, shaded procedurally in `organism.wgsl`.

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::ImageSampler;
use bevy::mesh::{
    Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology,
    VertexAttributeValues, VertexFormat,
};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    TextureDimension, TextureFormat,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin};
use bevy::window::PrimaryWindow;

use crate::camera::CameraState;
use crate::sim::{Selection, SimState};

pub const ATTRIBUTE_SHAPE: MeshVertexAttribute =
    MeshVertexAttribute::new("Organism_Shape", 0x5EED_0001, VertexFormat::Float32x4);
pub const ATTRIBUTE_TINT: MeshVertexAttribute =
    MeshVertexAttribute::new("Organism_Tint", 0x5EED_0002, VertexFormat::Unorm8x4);
pub const ATTRIBUTE_BODY: MeshVertexAttribute =
    MeshVertexAttribute::new("Organism_Body", 0x5EED_0003, VertexFormat::Unorm8x4);
pub const ATTRIBUTE_DIET: MeshVertexAttribute =
    MeshVertexAttribute::new("Organism_Diet", 0x5EED_0004, VertexFormat::Unorm8x4);
pub const ATTRIBUTE_STATE: MeshVertexAttribute =
    MeshVertexAttribute::new("Organism_State", 0x5EED_0005, VertexFormat::Unorm8x4);
/// speed gene, agility gene, eating flash, hurt flash
pub const ATTRIBUTE_MOTION: MeshVertexAttribute =
    MeshVertexAttribute::new("Organism_Motion", 0x5EED_0006, VertexFormat::Unorm8x4);
/// sense range gene, field-of-view gene, 0, 0
pub const ATTRIBUTE_SENSE: MeshVertexAttribute =
    MeshVertexAttribute::new("Organism_Sense", 0x5EED_0007, VertexFormat::Unorm8x4);

/// Flags packed into the state attribute's blue channel.
const FLAG_SELECTED: u8 = 1;
const FLAG_HIGHLIGHT: u8 = 2;
const FLAG_DEAD: u8 = 4;
const FLAG_MOTILE: u8 = 8;
/// A visual effect quad (kind in shape.z, age in shape.w).
const FLAG_FX: u8 = 16;
/// Member of a species struck by plague.
const FLAG_SICK: u8 = 32;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct OrganismMaterial {
    /// x = time, y = world units per pixel.
    #[uniform(0)]
    pub params: Vec4,
}

impl Material2d for OrganismMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://evo_app/render/organism.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://evo_app/render/organism.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
            ATTRIBUTE_SHAPE.at_shader_location(2),
            ATTRIBUTE_TINT.at_shader_location(3),
            ATTRIBUTE_BODY.at_shader_location(4),
            ATTRIBUTE_DIET.at_shader_location(5),
            ATTRIBUTE_STATE.at_shader_location(6),
            ATTRIBUTE_MOTION.at_shader_location(7),
            ATTRIBUTE_SENSE.at_shader_location(8),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

pub const MAX_VENTS: usize = 16;

#[derive(ShaderType, Clone, Default)]
pub struct BackgroundUniform {
    /// width, height, time, world units per pixel
    pub world: Vec4,
    /// sun (0..1 of max), surface temperature, day phase, number of vents
    pub sun: Vec4,
    /// x, y, strength, radius
    pub vents: [Vec4; MAX_VENTS],
    /// impact-winter darkness (0..1), number of events, 0, 0
    pub effects: Vec4,
    /// Active visible events: x, width, kind (1 upwelling, 2 wall), param
    /// (upwelling: remaining strength; wall: top y, negative while rising).
    pub events: [Vec4; MAX_EVENTS],
}

pub const MAX_EVENTS: usize = 8;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct BackgroundMaterial {
    #[uniform(0)]
    pub uniform: BackgroundUniform,
    #[texture(1)]
    #[sampler(2)]
    pub terrain: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub light: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    pub fields: Handle<Image>,
}

impl Material2d for BackgroundMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://evo_app/render/background.wgsl".into()
    }
}

#[derive(Resource)]
struct RenderHandles {
    organism_mesh: Handle<Mesh>,
    organism_material: Handle<OrganismMaterial>,
    background_material: Handle<BackgroundMaterial>,
    terrain_image: Handle<Image>,
    terrain_version: u64,
    light_image: Handle<Image>,
    fields_image: Handle<Image>,
    frame: u64,
}

/// Encode the terrain SDF into an R8 texture (±64 units around the surface).
fn write_terrain(image: &mut Image, sdf: &evo_core::field::Field) {
    if let Some(data) = image.data.as_mut() {
        for y in 0..sdf.ny {
            for x in 0..sdf.nx {
                let d = sdf.data[sdf.index(x, y)];
                let v = (d / 128.0 + 0.5).clamp(0.0, 1.0);
                data[(sdf.ny - 1 - y) * sdf.nx + x] = (v * 255.0).round() as u8;
            }
        }
    }
}

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "organism.wgsl");
        embedded_asset!(app, "background.wgsl");
        app.add_plugins((
            Material2dPlugin::<OrganismMaterial>::default(),
            Material2dPlugin::<BackgroundMaterial>::default(),
        ))
        .add_systems(Startup, setup)
        .add_systems(
            PostUpdate,
            (update_background, update_organisms, draw_overlays)
                .before(bevy::transform::TransformSystems::Propagate),
        );
    }
}

fn empty_image(width: u32, height: u32, format: TextureFormat, bytes_per_pixel: usize) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0; width as usize * height as usize * bytes_per_pixel],
        format,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

fn setup(
    mut commands: Commands,
    state: Res<SimState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut organism_materials: ResMut<Assets<OrganismMaterial>>,
    mut background_materials: ResMut<Assets<BackgroundMaterial>>,
) {
    let world = &state.world;
    let (w, h) = (world.config.world.width, world.config.world.height);

    // Terrain SDF texture (static until terrain changes).
    let sdf = &world.terrain.sdf;
    let mut terrain = empty_image(sdf.nx as u32, sdf.ny as u32, TextureFormat::R8Unorm, 1);
    write_terrain(&mut terrain, sdf);
    let light = &world.env.light;
    let light_image = images.add(empty_image(
        light.nx as u32,
        light.ny as u32,
        TextureFormat::R8Unorm,
        1,
    ));
    let m = &world.env.minerals;
    let fields_image = images.add(empty_image(
        m.nx as u32,
        m.ny as u32,
        TextureFormat::Rgba8Unorm,
        4,
    ));
    let terrain = images.add(terrain);

    let background_material = background_materials.add(BackgroundMaterial {
        uniform: BackgroundUniform::default(),
        terrain: terrain.clone(),
        light: light_image.clone(),
        fields: fields_image.clone(),
    });
    // A little larger than the world so the edges don't show a seam.
    let margin = 4000.0;
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(w + margin * 2.0, h + margin * 2.0))),
        MeshMaterial2d(background_material.clone()),
        Transform::from_xyz(w * 0.5, h * 0.5, 0.0),
    ));

    let organism_mesh = meshes.add(Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    ));
    let organism_material = organism_materials.add(OrganismMaterial { params: Vec4::ZERO });
    commands.spawn((
        Mesh2d(organism_mesh.clone()),
        MeshMaterial2d(organism_material.clone()),
        Transform::from_xyz(0.0, 0.0, 1.0),
        NoFrustumCulling,
    ));

    commands.insert_resource(RenderHandles {
        organism_mesh,
        organism_material,
        background_material,
        terrain_image: terrain,
        terrain_version: world.terrain.version,
        light_image,
        fields_image,
        frame: 0,
    });
}

fn update_background(
    state: Res<SimState>,
    cam: Res<CameraState>,
    time: Res<Time<Real>>,
    mut handles: ResMut<RenderHandles>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<BackgroundMaterial>>,
) {
    let world = &state.world;
    let cfg = &world.config;
    handles.frame += 1;
    let max_sun = cfg.light.surface_intensity * (1.0 + cfg.light.season_amplitude);

    // Terrain changes only through events (rising barriers).
    if world.terrain.version != handles.terrain_version {
        handles.terrain_version = world.terrain.version;
        if let Some(mut image) = images.get_mut(&handles.terrain_image) {
            write_terrain(&mut image, &world.terrain.sdf);
        }
    }

    // Field textures change slowly; refresh every few frames.
    if handles.frame % 4 == 1 {
        let light = &world.env.light;
        if let Some(mut image) = images.get_mut(&handles.light_image)
            && let Some(data) = image.data.as_mut()
        {
            let (nx, ny) = (light.nx, light.ny);
            for x in 0..nx {
                let column = &light.level[x * ny..(x + 1) * ny];
                for (y, &level) in column.iter().enumerate() {
                    let v = (level / max_sun).clamp(0.0, 1.0).sqrt();
                    data[(ny - 1 - y) * nx + x] = (v * 255.0) as u8;
                }
            }
        }
    }
    if handles.frame % 8 == 5 {
        let env = &world.env;
        if let Some(mut image) = images.get_mut(&handles.fields_image)
            && let Some(data) = image.data.as_mut()
        {
            let m = &env.minerals;
            let (nx, ny) = (m.nx, m.ny);
            for y in 0..ny {
                for x in 0..nx {
                    let p = glam::Vec2::new((x as f32 + 0.5) * m.cell, (y as f32 + 0.5) * m.cell);
                    let minerals = m.data[m.index(x, y)];
                    let detritus = env.detritus.get(p);
                    let temp = env.temperature.sample(p);
                    let i = ((ny - 1 - y) * nx + x) * 4;
                    data[i] = ((1.0 - (-minerals * 3.0).exp()) * 255.0) as u8;
                    data[i + 1] = ((1.0 - (-detritus * 40.0).exp()) * 255.0) as u8;
                    data[i + 2] = ((temp / 50.0).clamp(0.0, 1.0) * 255.0) as u8;
                    data[i + 3] = 255;
                }
            }
        }
    }

    if let Some(mut material) = materials.get_mut(&handles.background_material) {
        let clock = &world.env.clock;
        let u = &mut material.uniform;
        u.world = Vec4::new(
            cfg.world.width,
            cfg.world.height,
            time.elapsed_secs_wrapped(),
            cam.zoom,
        );
        u.sun = Vec4::new(
            clock.sun / max_sun,
            clock.surface_temp,
            clock.day_phase,
            world.env.vents.len().min(MAX_VENTS) as f32,
        );
        u.vents = [Vec4::ZERO; MAX_VENTS];
        for (slot, vent) in u.vents.iter_mut().zip(world.env.vents.iter()) {
            *slot = Vec4::new(vent.pos.x, vent.pos.y, vent.strength, cfg.vents.radius);
        }
        let now = world.time();
        u.events = [Vec4::ZERO; MAX_EVENTS];
        let mut n = 0;
        for e in &world.happenings.active {
            use evo_core::events::WorldEvent;
            let slot = match e {
                WorldEvent::Upwelling {
                    x, width, until, ..
                } => {
                    let left = ((until - now) as f32 / cfg.events.upwelling_duration.max(1.0))
                        .clamp(0.0, 1.0);
                    // Fade in over the first 10% and out over the last 20%.
                    let strength = ((1.0 - left) * 10.0).min(1.0).min(left * 5.0);
                    Some(Vec4::new(*x, *width, 1.0, strength))
                }
                WorldEvent::Barrier {
                    x,
                    width,
                    started,
                    rise,
                    stand,
                    built,
                    ..
                } => {
                    let t = now - started;
                    let moving = t < *rise || t > rise + stand;
                    Some(Vec4::new(
                        *x,
                        *width,
                        2.0,
                        if moving { -built } else { *built },
                    ))
                }
                _ => None,
            };
            if let Some(v) = slot
                && n < MAX_EVENTS
            {
                u.events[n] = v;
                n += 1;
            }
        }
        u.effects = Vec4::new(1.0 - world.happenings.sun_factor(now), n as f32, 0.0, 0.0);
    }
}

/// sRGB (0..1) → linear, for vertex colors going into an HDR pipeline.
fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB color → linear RGBA8 via a lookup table (called for every organism every frame).
fn linear_tint(rgb: [f32; 3]) -> [u8; 4] {
    static LUT: std::sync::OnceLock<[u8; 256]> = std::sync::OnceLock::new();
    let lut = LUT.get_or_init(|| std::array::from_fn(|i| unorm(to_linear(i as f32 / 255.0))));
    let idx = |c: f32| lut[(c.clamp(0.0, 1.0) * 255.0 + 0.5) as usize];
    [idx(rgb[0]), idx(rgb[1]), idx(rgb[2]), 255]
}

fn unorm(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Per-quad data (the same for all four corners).
#[derive(Clone, Copy, Default)]
struct Quad {
    shape: [f32; 4],
    tint: [u8; 4],
    body: [u8; 4],
    diet: [u8; 4],
    state: [u8; 4],
    motion: [u8; 4],
    sense: [u8; 4],
}

#[derive(Default)]
struct QuadBuffers {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    shape: Vec<[f32; 4]>,
    tint: Vec<[u8; 4]>,
    body: Vec<[u8; 4]>,
    diet: Vec<[u8; 4]>,
    state: Vec<[u8; 4]>,
    motion: Vec<[u8; 4]>,
    sense: Vec<[u8; 4]>,
}

impl QuadBuffers {
    fn push(&mut self, center: glam::Vec2, half: f32, q: Quad) {
        for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            self.positions
                .push([center.x + dx * half, center.y + dy * half, 0.0]);
            self.uvs.push([dx, dy]);
            self.shape.push(q.shape);
            self.tint.push(q.tint);
            self.body.push(q.body);
            self.diet.push(q.diet);
            self.state.push(q.state);
            self.motion.push(q.motion);
            self.sense.push(q.sense);
        }
    }
}

fn update_organisms(
    state: Res<SimState>,
    selection: Res<Selection>,
    cam: Res<CameraState>,
    time: Res<Time<Real>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    handles: Res<RenderHandles>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<OrganismMaterial>>,
) {
    let Ok(window) = windows.single() else { return };
    let world = &state.world;
    let alpha = state.alpha;
    let px = cam.zoom; // world units per pixel
    let half_view = Vec2::new(window.width(), window.height()) * 0.5 * px;
    let (view_min, view_max) = (cam.center - half_view, cam.center + half_view);
    let min_half = 1.6 * px; // never smaller than ~3 px across

    let mut b = QuadBuffers::default();
    let capacity = world.organisms.len() + world.carcasses.len() + world.fx.len();
    b.positions.reserve(capacity * 4);
    let dt = world.config.time.dt;
    // Brightness of a flash that started at `tick` and lasts `secs`.
    let flash = |tick: u64, secs: f32| {
        if tick == 0 {
            return 0u8;
        }
        let age = world.tick.saturating_sub(tick) as f32 * dt;
        unorm(1.0 - age / secs)
    };
    let visible = |p: glam::Vec2, half: f32| {
        !(p.x + half < view_min.x
            || p.x - half > view_max.x
            || p.y + half < view_min.y
            || p.y - half > view_max.y)
    };

    for c in &world.carcasses {
        let half = (c.radius * (1.4 + c.spikes * 0.5)).max(min_half);
        if !visible(c.pos, half) {
            continue;
        }
        b.push(
            c.pos,
            half,
            Quad {
                shape: [half / c.radius.max(0.01), c.radius / px, c.heading, c.seed],
                tint: linear_tint(c.color),
                body: [unorm(c.armor), unorm(c.spikes), 0, c.pattern],
                diet: [unorm(c.consumed()), 0, 0, 0],
                state: [0, 0, FLAG_DEAD, 255],
                motion: [0, 0, 0, flash(c.hurt_tick, 0.5)],
                sense: [0; 4],
            },
        );
    }

    // Species currently struck by plague (and their unnamed offshoots).
    let plagued: Vec<u32> = world
        .happenings
        .active
        .iter()
        .filter_map(|e| match e {
            evo_core::events::WorldEvent::Plague { species, .. } => Some(*species),
            _ => None,
        })
        .collect();
    let mut sick_cache: std::collections::HashMap<u32, bool> = std::collections::HashMap::new();

    for o in &world.organisms {
        let t = &o.traits;
        let pos = o.prev_pos.lerp(o.pos, alpha);
        let motile = t.is_motile();
        // Flagella get longer with speed.
        let tail = if motile {
            1.0 + 0.5 + 1.6 * t.speed
        } else {
            1.0
        };
        // Mandibles (strong jaws on a hunter) and a scavenger's proboscis stick out in front.
        let mouth = if t.hunt > 0.3 { 0.5 * t.jaws } else { 0.0 }.max(if t.scavenge > 0.15 {
            0.2 + 0.4 * t.scavenge
        } else {
            0.0
        });
        let reach = tail.max(1.0 + t.spikes * 0.5).max(1.1 + mouth);
        let half = (o.radius * reach.max(1.25)).max(min_half);
        if !visible(pos, half) {
            continue;
        }
        let mut flags = 0u8;
        if selection.organism == Some(o.id) {
            flags |= FLAG_SELECTED;
        }
        if selection.species == Some(o.species) {
            flags |= FLAG_HIGHLIGHT;
        }
        if motile {
            flags |= FLAG_MOTILE;
        }
        if !plagued.is_empty() {
            let sick = *sick_cache.entry(o.species).or_insert_with(|| {
                let root = world
                    .species
                    .established_ancestor(o.species)
                    .unwrap_or(o.species);
                plagued.contains(&root)
            });
            if sick {
                flags |= FLAG_SICK;
            }
        }
        let tint = linear_tint(t.color);
        let seed = ((o.id.wrapping_mul(2_654_435_761) >> 8) % 1000) as f32 / 1000.0;
        let shape = [half / o.radius, o.radius / px, o.heading, seed];
        let body = [unorm(t.armor), unorm(t.spikes), unorm(t.jaws), t.pattern];
        let diet = [
            unorm(t.photo),
            unorm(t.chemo),
            unorm(t.scavenge),
            unorm(t.hunt),
        ];
        let energy = o.energy / o.max_energy().max(1e-6);
        let health = o.hp / o.max_hp().max(1e-6);
        let st = [
            unorm(energy),
            unorm(health),
            flags,
            unorm(o.mass / t.adult_mass),
        ];
        let agility = o.genome.frac(evo_core::genome::gene::AGILITY);
        let motion = [
            unorm(t.speed),
            unorm(agility),
            flash(o.ate_tick, 0.6),
            flash(o.hurt_tick, 0.5),
        ];
        let sense = [
            o.genome.get(evo_core::genome::gene::SENSE_RANGE),
            o.genome.get(evo_core::genome::gene::FOV),
            0,
            0,
        ];
        b.push(
            pos,
            half,
            Quad {
                shape,
                tint,
                body,
                diet,
                state: st,
                motion,
                sense,
            },
        );
    }

    // Effects (bites, gulps, shockwaves) on top.
    for f in &world.fx {
        use evo_core::world::FxKind;
        let (kind, scale) = match f.kind {
            FxKind::Engulf => (1.0, 2.0),
            FxKind::Bite => (2.0, 2.4),
            FxKind::Scavenge => (3.0, 2.2),
            FxKind::Impact => (4.0, 1.4),
        };
        let half = (f.radius * scale).max(6.0 * px);
        if !visible(f.pos, half) {
            continue;
        }
        let age =
            (world.tick.saturating_sub(f.tick) as f32 * dt / f.kind.duration()).clamp(0.0, 1.0);
        let seed = ((f.tick.wrapping_mul(2_654_435_761) ^ f.pos.x.to_bits() as u64) % 1000) as f32
            / 1000.0;
        b.push(
            f.pos,
            half,
            Quad {
                shape: [half / f.radius.max(0.01), f.radius / px, kind, age],
                tint: linear_tint(f.color),
                body: [(seed * 255.0) as u8, 0, 0, 0],
                state: [0, 0, FLAG_FX, 255],
                ..Default::default()
            },
        );
    }

    let quads = b.positions.len() / 4;
    let mut indices = Vec::with_capacity(quads * 6);
    for q in 0..quads as u32 {
        let v = q * 4;
        indices.extend_from_slice(&[v, v + 1, v + 2, v, v + 2, v + 3]);
    }
    if let Some(mut mesh) = meshes.get_mut(&handles.organism_mesh) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, b.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, b.uvs);
        mesh.insert_attribute(ATTRIBUTE_SHAPE, b.shape);
        mesh.insert_attribute(ATTRIBUTE_TINT, VertexAttributeValues::Unorm8x4(b.tint));
        mesh.insert_attribute(ATTRIBUTE_BODY, VertexAttributeValues::Unorm8x4(b.body));
        mesh.insert_attribute(ATTRIBUTE_DIET, VertexAttributeValues::Unorm8x4(b.diet));
        mesh.insert_attribute(ATTRIBUTE_STATE, VertexAttributeValues::Unorm8x4(b.state));
        mesh.insert_attribute(ATTRIBUTE_MOTION, VertexAttributeValues::Unorm8x4(b.motion));
        mesh.insert_attribute(ATTRIBUTE_SENSE, VertexAttributeValues::Unorm8x4(b.sense));
        mesh.insert_indices(Indices::U32(indices));
    }
    if let Some(mut m) = materials.get_mut(&handles.organism_material) {
        m.params = Vec4::new(time.elapsed_secs_wrapped(), px, 0.0, 0.0);
    }
}

/// Sense range of the selected organism, drawn with gizmos.
fn draw_overlays(
    mut gizmos: Gizmos,
    state: Res<SimState>,
    selection: Res<Selection>,
    ui: Res<crate::ui::UiState>,
) {
    if !ui.show_senses {
        return;
    }
    let Some(o) = selection.organism.and_then(|id| state.world.find(id)) else {
        return;
    };
    let range = (o.traits.sense_radii * o.radius).min(state.world.config.genes.sense_cap);
    if range <= o.radius {
        return;
    }
    let p = o.prev_pos.lerp(o.pos, state.alpha);
    let center = Vec2::new(p.x, p.y);
    let color = Color::srgba(0.6, 0.9, 1.0, 0.25);
    if o.traits.fov_deg >= 359.0 {
        gizmos.circle_2d(Isometry2d::from_translation(center), range, color);
    } else {
        let half = o.traits.fov_deg.to_radians() * 0.5;
        let arc_rotation = Rot2::radians(o.heading - half - std::f32::consts::FRAC_PI_2);
        gizmos.arc_2d(
            Isometry2d::new(center, arc_rotation),
            half * 2.0,
            range,
            color,
        );
        for a in [o.heading - half, o.heading + half] {
            gizmos.line_2d(center, center + Vec2::from_angle(a) * range, color);
        }
    }
}
