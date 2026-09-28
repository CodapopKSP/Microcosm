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
use evo_core::Genome;
use evo_core::parts::{self, BodyShape, Instance, PartKind};

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
/// movement (speed / max speed), walking, eating flash, hurt flash
pub const ATTRIBUTE_MOTION: MeshVertexAttribute =
    MeshVertexAttribute::new("Organism_Motion", 0x5EED_0006, VertexFormat::Unorm8x4);
/// oxygenic share, toxin, nitrogen fixation, lure
pub const ATTRIBUTE_CHEM: MeshVertexAttribute =
    MeshVertexAttribute::new("Organism_Chem", 0x5EED_0007, VertexFormat::Unorm8x4);
/// direction the last bite came from (body-relative angle / 2π), unused ×3
pub const ATTRIBUTE_WOUND: MeshVertexAttribute =
    MeshVertexAttribute::new("Organism_Wound", 0x5EED_0008, VertexFormat::Unorm8x4);

/// Flags packed into the state attribute's blue channel.
const FLAG_SELECTED: u8 = 1;
const FLAG_HIGHLIGHT: u8 = 2;
const FLAG_DEAD: u8 = 4;
const FLAG_MOTILE: u8 = 8;
/// A visual effect quad (kind in shape.z, age in shape.w).
const FLAG_FX: u8 = 16;
/// Member of a species struck by plague.
const FLAG_SICK: u8 = 32;
/// A body part (kind, size, mirror/index, seed in the body attribute).
const FLAG_PART: u8 = 64;

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
            ATTRIBUTE_CHEM.at_shader_location(8),
            ATTRIBUTE_WOUND.at_shader_location(9),
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
    /// impact-winter darkness (0..1), number of events, map mode (0 none, 1 oxygen,
    /// 2 nutrients, 3 toxin), 0
    pub effects: Vec4,
    /// air oxygen, air CO₂ (1 = today), 0, 0
    pub air: Vec4,
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
    /// oxygen, nutrients, toxin, CO₂
    #[texture(7)]
    #[sampler(8)]
    pub chem: Handle<Image>,
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
    chem_image: Handle<Image>,
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
    let chem_image = images.add(empty_image(
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
        chem: chem_image.clone(),
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
        chem_image,
        frame: 0,
    });
}

fn update_background(
    state: Res<SimState>,
    cam: Res<CameraState>,
    ui: Res<crate::ui::UiState>,
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
    if handles.frame % 8 == 1 {
        let env = &world.env;
        if let Some(mut image) = images.get_mut(&handles.chem_image)
            && let Some(data) = image.data.as_mut()
        {
            let f = &env.oxygen;
            let (nx, ny) = (f.nx, f.ny);
            let scale = |v: f32, max: f32| ((v / max).clamp(0.0, 1.0) * 255.0) as u8;
            let deep = cfg.chemistry.nutrient_deep.max(1e-3);
            for y in 0..ny {
                for x in 0..nx {
                    let k = f.index(x, y);
                    let i = ((ny - 1 - y) * nx + x) * 4;
                    data[i] = scale(f.data[k], 1.5);
                    data[i + 1] = scale(env.nutrients.data[k], deep * 1.5);
                    data[i + 2] = ((1.0 - (-env.toxin.data[k] * 20.0).exp()) * 255.0) as u8;
                    data[i + 3] = scale(env.co2.data[k], 2.0);
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
        u.effects = Vec4::new(
            1.0 - world.happenings.sun_factor(now),
            n as f32,
            ui.map_mode as u32 as f32,
            0.0,
        );
        let air = &world.env.atmosphere;
        u.air = Vec4::new(air.oxygen, air.co2, 0.0, 0.0);
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

/// The wound attribute: where a bite came from, relative to the body's heading.
fn wound(hurt_dir: f32, heading: f32) -> [u8; 4] {
    let turn = (hurt_dir - heading).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
    [unorm(turn), 0, 0, 0]
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
    chem: [u8; 4],
    wound: [u8; 4],
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
    chem: Vec<[u8; 4]>,
    wound: Vec<[u8; 4]>,
}

impl QuadBuffers {
    /// An axis-aligned square quad.
    fn push(&mut self, center: glam::Vec2, half: f32, q: Quad) {
        self.push_oriented(center, glam::Vec2::splat(half), glam::Vec2::X, q);
    }

    /// A rectangle with half extents `half` whose local x axis points along `axis` (unit).
    fn push_oriented(&mut self, center: glam::Vec2, half: glam::Vec2, axis: glam::Vec2, q: Quad) {
        let side = glam::Vec2::new(-axis.y, axis.x);
        for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = center + axis * (dx * half.x) + side * (dy * half.y);
            self.positions.push([p.x, p.y, 0.0]);
            self.uvs.push([dx, dy]);
            self.shape.push(q.shape);
            self.tint.push(q.tint);
            self.body.push(q.body);
            self.diet.push(q.diet);
            self.state.push(q.state);
            self.motion.push(q.motion);
            self.chem.push(q.chem);
            self.wound.push(q.wound);
        }
    }
}

/// Laid-out body parts per genome (many organisms share a genome exactly).
#[derive(Default)]
struct LayoutCache {
    map: std::collections::HashMap<Genome, std::sync::Arc<Vec<Instance>>>,
}

impl LayoutCache {
    fn get(&mut self, genome: &Genome) -> std::sync::Arc<Vec<Instance>> {
        if self.map.len() > 50_000 {
            self.map.clear();
        }
        self.map
            .entry(*genome)
            .or_insert_with(|| {
                let body = BodyShape::from_genome(genome);
                let mut out = Vec::new();
                parts::layout(&parts::decode_parts(genome), &body, &mut out);
                std::sync::Arc::new(out)
            })
            .clone()
    }
}

/// Seconds a newborn takes to bud off its parent (the animation).
pub const BUD_TIME: f32 = 0.8;
/// Seconds for a dead body's colours to drain to grey.
pub const DEATH_FADE: f32 = 0.4;
/// Seconds tentacles take to reach for something they grab, and to let go of it.
pub const GRASP_TIME: f32 = 0.25;
const RELEASE_TIME: f32 = 0.35;

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Extra room around a part (in part lengths) for its bending animation.
fn part_margin(kind: PartKind) -> f32 {
    match kind {
        PartKind::Flagellum => 0.2,
        PartKind::Cilia => 0.25,
        PartKind::Tentacle => 0.4,
        PartKind::Frond => 0.15,
        PartKind::Fin => 0.2,
        PartKind::Fluke => 0.25,
        PartKind::Leg => 0.45,
        PartKind::Holdfast => 0.1,
        PartKind::Stalk => 0.12,
        PartKind::Eye => 0.02,
        PartKind::Antenna => 0.3,
        PartKind::Lure => 0.6,
        PartKind::Spine => 0.05,
        PartKind::Jaw => 0.35,
        PartKind::Shell => 0.08,
    }
}

/// Everything about the owner that its part quads share.
struct PartOwner {
    pos: glam::Vec2,
    radius: f32,
    heading: f32,
    seed: f32,
    quad: Quad,
    /// What its tentacles hold: the tentacle (part index), the held body's center and radius,
    /// and how far the tentacle has closed on it (0..1).
    holds: Vec<(usize, glam::Vec2, f32, f32)>,
}

/// Angle wrapped to (−π, π].
fn wrap_angle(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Seconds a swallowed prey takes to slide into the mouth, and to pass inside.
const SWALLOW_SLIDE: f32 = 0.25;
const SWALLOW_ENTER: f32 = 0.5;

/// Where and how to draw a swallowed prey `t` seconds after it was swallowed.
struct SwallowPose {
    pos: glam::Vec2,
    radius: f32,
    heading: f32,
    /// Freshness (colour) and fade-out of the body, and fade-out of its parts.
    fresh: f32,
    fade: f32,
    parts_fade: f32,
}

/// The swallowing animation: the prey slides into the hunter's mouth head first, squeezing to
/// fit, passes inside, and is digested there as a shrinking, fading food vacuole that moves
/// with the hunter. Without a hunter (it died or is gone) the prey just fades where it was.
fn swallow_pose(
    f: &evo_core::world::Fx,
    body: &evo_core::world::FxBody,
    hunter: Option<&evo_core::Organism>,
    t: f32,
    alpha: f32,
) -> Option<SwallowPose> {
    let duration = f.kind.duration();
    let fresh = 1.0 - smooth((t - 0.3) / 0.5);
    let Some(h) = hunter else {
        let age = t / SWALLOW_ENTER;
        return (age < 1.0).then_some(SwallowPose {
            pos: f.pos,
            radius: f.radius,
            heading: body.heading,
            fresh: 1.0 - age * 3.0,
            fade: age,
            parts_fade: age,
        });
    };
    let at = h.prev_pos.lerp(h.pos, alpha);
    let rot = glam::Vec2::from_angle(h.heading);
    let hr = h.radius;
    // The mouth's opening, at the front edge of the head.
    let lip = (h.traits.mouth_offset + 0.6 * h.traits.mouth_radius) * hr;
    // A resting place inside, off-center by the prey's seed.
    let inside = at + rot.rotate(glam::Vec2::new(-0.1, 0.5 * (body.seed - 0.5)) * hr);
    let fit = f.radius.min(h.traits.mouth_radius * hr * 0.7);
    let small = f.radius.min(0.4 * hr);
    // Slide until it's half in the opening.
    let mouth = at + rot.rotate(glam::Vec2::new(lip + 0.3 * fit, 0.0));
    let into_mouth = (mouth - f.pos).to_angle();
    let resting = h.heading + std::f32::consts::PI * (body.seed - 0.5);
    let turn = |from: f32, to: f32, e: f32| from + wrap_angle(to - from) * e;
    let pose = if t < SWALLOW_SLIDE {
        let e = smooth(t / SWALLOW_SLIDE);
        SwallowPose {
            pos: f.pos.lerp(mouth, e),
            radius: f.radius + (fit - f.radius) * e,
            heading: turn(body.heading, into_mouth, e),
            fresh,
            fade: 0.0,
            parts_fade: e,
        }
    } else if t < SWALLOW_ENTER {
        let e = smooth((t - SWALLOW_SLIDE) / (SWALLOW_ENTER - SWALLOW_SLIDE));
        SwallowPose {
            pos: mouth.lerp(inside, e),
            radius: fit + (small - fit) * e,
            heading: turn(into_mouth, resting, e),
            fresh,
            // Seen through the hunter's body as soon as it's in.
            fade: 0.45 * smooth(e * 2.5),
            parts_fade: 1.0,
        }
    } else {
        let e = ((t - SWALLOW_ENTER) / (duration - SWALLOW_ENTER)).clamp(0.0, 1.0);
        SwallowPose {
            pos: inside,
            radius: small * (1.0 - 0.6 * smooth(e)),
            heading: resting,
            fresh,
            fade: 0.45 + 0.55 * smooth((e - 0.4) / 0.6),
            parts_fade: 1.0,
        }
    };
    Some(pose)
}

/// Push the quads of `instances` that are behind (`front == false`) or in front of the body.
fn push_parts(
    b: &mut QuadBuffers,
    instances: &[Instance],
    owner: &PartOwner,
    front: bool,
    px: f32,
) {
    let rot = glam::Vec2::from_angle(owner.heading);
    for (index, inst) in instances.iter().enumerate() {
        if inst.kind.in_front() != front {
            continue;
        }
        let length = inst.length * owner.radius;
        if length < 1.2 * px {
            continue;
        }
        let base = owner.pos + rot.rotate(inst.base * owner.radius);
        let mut angle = owner.heading + inst.dir;
        // A tentacle holding something swings to touch it, bowing out to keep its length.
        let mut chord = length;
        let mut bend = 0.0;
        let mut grip = 0.0;
        if inst.kind == PartKind::Tentacle
            && let Some(&(_, target, target_r, g)) = owner.holds.iter().find(|h| h.0 == index)
        {
            let to = target - base;
            let dist = to.length();
            if dist > 1e-3 {
                let turn = wrap_angle(to.to_angle() - angle);
                let tip = (dist - 0.6 * target_r).clamp(0.25 * length, length);
                angle += turn * g;
                chord = length + (tip - length) * g;
                let side = if turn.abs() > 0.05 {
                    -turn.signum()
                } else {
                    inst.mirror.signum()
                };
                let height = (2.0 / std::f32::consts::PI) * (length / chord - 1.0).max(0.0).sqrt();
                bend = side * height.min(0.75);
                grip = g;
            }
        }
        let axis = glam::Vec2::from_angle(angle);
        let center = base + axis * (chord * 0.5);
        let half_width = inst.width * owner.radius / chord * 0.5;
        let margin = part_margin(inst.kind) + 2.0 * px / chord;
        let hx = 0.5 + margin.min(0.3);
        let hy = half_width + margin + bend.abs();
        let mut quad = owner.quad;
        quad.shape = [hx, hy, half_width, chord / px];
        if inst.kind == PartKind::Tentacle {
            // In the part's mirrored frame, as the shader draws it.
            quad.chem[2] = unorm(grip);
            quad.chem[3] = unorm(0.5 + 0.5 * bend * inst.mirror.signum() / 0.75);
        }
        let mirror = if inst.mirror < 0.0 { 1u8 } else { 0 };
        let seed = (owner.seed * 255.0) as u8;
        quad.body = [
            inst.kind.code(),
            unorm(inst.size),
            mirror | inst.index.min(100) << 1,
            seed.wrapping_add(inst.slot as u8 * 37),
        ];
        quad.state[2] |= FLAG_PART;
        b.push_oriented(center, glam::Vec2::new(hx, hy) * chord, axis, quad);
    }
}

#[allow(clippy::too_many_arguments)]
fn update_organisms(
    state: Res<SimState>,
    selection: Res<Selection>,
    cam: Res<CameraState>,
    time: Res<Time<Real>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    handles: Res<RenderHandles>,
    mut cache: Local<LayoutCache>,
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
    // Parts are drawn once a body is at least this many pixels in radius.
    let parts_from = 2.5 * px;

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
    // Half extents of a body quad (radius units): the ellipse plus room for rings and halos.
    let body_extent = |shape: &BodyShape| {
        glam::Vec2::new(
            shape.half_length * 1.55 + 0.1,
            shape.half_width * 1.4 + 0.25,
        )
    };
    // How far parts can reach from the center (radius units), for culling.
    let reach = |instances: &[Instance], shape: &BodyShape| {
        instances
            .iter()
            .map(|i| i.tip().length() + i.width)
            .fold(shape.half_length * 1.4, f32::max)
    };
    let body_attr = |shape: &BodyShape, jaws: f32, pattern: u8| {
        [
            unorm(shape.elongation),
            unorm(shape.segments as f32 / 6.0),
            unorm(jaws),
            pattern,
        ]
    };

    for c in &world.carcasses {
        let shape = BodyShape::from_genome(&c.genome);
        let instances = cache.get(&c.genome);
        let extent = reach(&instances, &shape);
        if !visible(c.pos, c.radius * extent) {
            continue;
        }
        let ext = (body_extent(&shape) * c.radius).max(glam::Vec2::splat(min_half));
        let quad = Quad {
            shape: [ext.x / c.radius, ext.y / c.radius, c.radius / px, c.seed],
            tint: linear_tint(c.color),
            body: body_attr(&shape, 0.0, c.genome.get(evo_core::genome::gene::PATTERN)),
            diet: [unorm(c.consumed()), 0, 0, 0],
            state: [0, 0, FLAG_DEAD, 255],
            // Dying: the colours drain to grey just after death.
            motion: [
                0,
                flash(c.died_tick, DEATH_FADE),
                0,
                flash(c.hurt_tick, 0.5),
            ],
            chem: [0; 4],
            wound: wound(c.hurt_dir, c.heading),
        };
        let owner = PartOwner {
            pos: c.pos,
            radius: c.radius,
            heading: c.heading,
            seed: c.seed,
            quad,
            holds: Vec::new(),
        };
        // Carcasses lose their parts as they are eaten.
        let draw_parts = c.radius >= parts_from && c.consumed() < 0.6;
        if draw_parts {
            push_parts(&mut b, &instances, &owner, false, px);
        }
        b.push_oriented(c.pos, ext, glam::Vec2::from_angle(c.heading), quad);
        if draw_parts {
            push_parts(&mut b, &instances, &owner, true, px);
        }
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
        let mut pos = o.prev_pos.lerp(o.pos, alpha);
        let shape = BodyShape::new(t.elongation, t.segments);
        // Budding: a parent shrinks back from its size before the split; a newborn grows out of
        // its parent through a neck that pinches off.
        let since = |tick: u64| world.tick.saturating_sub(tick) as f32 * dt;
        let mut radius = o.radius;
        if o.budded_tick > 0 && since(o.budded_tick) < BUD_TIME {
            let e = smooth(since(o.budded_tick) / BUD_TIME);
            radius *= o.bud_scale + (1.0 - o.bud_scale) * e;
        }
        let mut neck = None;
        let offset = o.bud_offset.length();
        if offset > 0.0 && since(o.born) < BUD_TIME {
            let t = since(o.born) / BUD_TIME;
            let e = smooth(t);
            let dir = o.bud_offset / offset;
            let parent_center = pos - o.bud_offset;
            let parent_r = (offset - o.radius - 0.5).max(o.radius * 0.5);
            let start = parent_center + dir * (parent_r - o.radius * 0.6).max(0.0);
            pos = start.lerp(pos, e);
            radius = o.radius * (0.6 + 0.4 * e);
            neck = Some((
                parent_center + dir * parent_r * 0.7,
                pos,
                parent_r.min(radius),
                t,
            ));
        }
        let with_parts = t.part_count > 0 && radius >= parts_from;
        let instances = if with_parts {
            Some(cache.get(&o.genome))
        } else {
            None
        };
        let extent = match &instances {
            Some(i) => reach(i, &shape),
            None => shape.half_length * 1.4,
        };
        if !visible(pos, (radius * extent).max(min_half)) {
            continue;
        }
        let mut flags = 0u8;
        if selection.organism == Some(o.id) {
            flags |= FLAG_SELECTED;
        }
        if selection.species == Some(o.species) {
            flags |= FLAG_HIGHLIGHT;
        }
        if t.is_motile() {
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
        let seed = ((o.id.wrapping_mul(2_654_435_761) >> 8) % 1000) as f32 / 1000.0;
        let ext = (body_extent(&shape) * radius).max(glam::Vec2::splat(min_half));
        let energy = o.energy / o.max_energy().max(1e-6);
        let health = o.hp / o.max_hp().max(1e-6);
        let max_speed = t.max_speed(o.radius).max(t.crawl_coeff * o.radius.sqrt());
        let moving = if max_speed > 0.0 {
            o.vel.length() / max_speed
        } else {
            0.0
        };
        let walking = o.touching && t.crawl_coeff > 0.0 && !o.anchored;
        let quad = Quad {
            shape: [ext.x / radius, ext.y / radius, radius / px, seed],
            tint: linear_tint(t.color),
            body: body_attr(&shape, t.jaws, t.pattern),
            diet: [
                unorm(t.photo),
                unorm(t.chemo),
                unorm(t.scavenge),
                unorm(t.hunt),
            ],
            state: [
                unorm(energy),
                unorm(health),
                flags,
                unorm(o.mass / t.adult_mass),
            ],
            motion: [
                unorm(moving),
                if walking { 255 } else { 0 },
                flash(o.ate_tick, 0.6),
                flash(o.hurt_tick, 0.5),
            ],
            chem: [
                unorm(t.oxygenic),
                unorm(t.toxin),
                unorm(t.fixation),
                unorm(t.lure),
            ],
            wound: wound(o.hurt_dir, o.heading),
        };
        let holds = if with_parts {
            o.grasps
                .iter()
                .zip(&t.grasps[..t.grasp_count as usize])
                .filter(|(g, _)| g.tick > 0)
                .filter_map(|(g, zone)| {
                    let closed = smooth(since(g.since) / GRASP_TIME);
                    let open = smooth((since(g.tick) - dt).max(0.0) / RELEASE_TIME);
                    let amount = closed * (1.0 - open);
                    (amount > 0.01).then_some((zone.part as usize, g.pos, g.radius, amount))
                })
                .collect()
        } else {
            Vec::new()
        };
        let owner = PartOwner {
            pos,
            radius,
            heading: o.heading,
            seed,
            quad,
            holds,
        };
        if let Some((from, to, width, t)) = neck {
            // The neck between parent and bud narrows in the middle and pinches off.
            let along = to - from;
            let length = along.length().max(1e-3);
            let half =
                glam::Vec2::new(length * 0.5 + width, width * 1.1).max(glam::Vec2::splat(px));
            b.push_oriented(
                (from + to) * 0.5,
                half,
                along / length,
                Quad {
                    shape: [half.x / width, width / px, 5.0, t],
                    tint: quad.tint,
                    body: [0, 0, 0, 0],
                    state: [0, 0, FLAG_FX, 255],
                    ..Default::default()
                },
            );
        }
        if let Some(i) = &instances {
            push_parts(&mut b, i, &owner, false, px);
        }
        b.push_oriented(pos, ext, glam::Vec2::from_angle(o.heading), quad);
        if let Some(i) = &instances {
            push_parts(&mut b, i, &owner, true, px);
        }
    }

    // Swallowed prey follow the hunter that ate them: find those hunters once per frame.
    let wanted: std::collections::HashSet<u64> = world
        .fx
        .iter()
        .filter_map(|f| f.body.and_then(|b| b.hunter))
        .collect();
    let hunters: std::collections::HashMap<u64, &evo_core::Organism> = if wanted.is_empty() {
        Default::default()
    } else {
        world
            .organisms
            .iter()
            .filter(|o| wanted.contains(&o.id))
            .map(|o| (o.id, o))
            .collect()
    };

    // Effects (bites, gulps, shockwaves) on top.
    for f in &world.fx {
        use evo_core::world::FxKind;
        if let (FxKind::Engulf, Some(body)) = (f.kind, f.body) {
            let t = world.tick.saturating_sub(f.tick) as f32 * dt;
            let hunter = body.hunter.and_then(|id| hunters.get(&id));
            let Some(pose) = swallow_pose(f, &body, hunter.copied(), t, alpha) else {
                continue;
            };
            let shape = BodyShape::from_genome(&body.genome);
            if !visible(pose.pos, pose.radius * shape.half_length * 1.6) {
                continue;
            }
            let ext = (body_extent(&shape) * pose.radius).max(glam::Vec2::splat(min_half));
            let quad = Quad {
                shape: [
                    ext.x / pose.radius,
                    ext.y / pose.radius,
                    pose.radius / px,
                    body.seed,
                ],
                tint: linear_tint(f.color),
                body: body_attr(
                    &shape,
                    0.0,
                    body.genome.get(evo_core::genome::gene::PATTERN),
                ),
                diet: [0; 4],
                state: [0, 0, FLAG_DEAD, 255],
                motion: [unorm(pose.fade), unorm(pose.fresh), 0, 0],
                chem: [0; 4],
                wound: [0; 4],
            };
            // Parts are drawn only until they're pulled into the mouth.
            let mut part_quad = quad;
            part_quad.motion[0] = unorm(pose.parts_fade);
            let owner = PartOwner {
                pos: pose.pos,
                radius: pose.radius,
                heading: pose.heading,
                seed: body.seed,
                quad: part_quad,
                holds: Vec::new(),
            };
            let instances = if pose.parts_fade < 0.99 && pose.radius >= parts_from {
                Some(cache.get(&body.genome))
            } else {
                None
            };
            if let Some(i) = &instances {
                push_parts(&mut b, i, &owner, false, px);
            }
            b.push_oriented(pose.pos, ext, glam::Vec2::from_angle(pose.heading), quad);
            if let Some(i) = &instances {
                push_parts(&mut b, i, &owner, true, px);
            }
            continue;
        }
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

    // An empty mesh upsets Bevy's buffer allocator; keep one invisible zero-size quad.
    if b.positions.is_empty() {
        b.push(cam.center, 0.0, Quad::default());
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
        mesh.insert_attribute(ATTRIBUTE_CHEM, VertexAttributeValues::Unorm8x4(b.chem));
        mesh.insert_attribute(ATTRIBUTE_WOUND, VertexAttributeValues::Unorm8x4(b.wound));
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
    let t = &o.traits;
    let cap = state.world.config.genes.sense_cap;
    let p = o.prev_pos.lerp(o.pos, state.alpha);
    let center = Vec2::new(p.x, p.y);
    let color = Color::srgba(0.6, 0.9, 1.0, 0.25);
    // Smell (antennae): a circle.
    let smell = (t.smell_radii * o.radius).min(cap);
    if smell > o.radius {
        gizmos.circle_2d(
            Isometry2d::from_translation(center),
            smell,
            Color::srgba(0.9, 0.8, 0.5, 0.25),
        );
    }
    // Sight: one cone per eye.
    for eye in &t.eyes[..t.eye_count as usize] {
        let range = (eye.range * o.radius).min(cap);
        let half = eye.cos_half.acos();
        let dir = o.heading + eye.dir;
        let arc_rotation = Rot2::radians(dir - half - std::f32::consts::FRAC_PI_2);
        gizmos.arc_2d(
            Isometry2d::new(center, arc_rotation),
            half * 2.0,
            range,
            color,
        );
        for a in [dir - half, dir + half] {
            gizmos.line_2d(center, center + Vec2::from_angle(a) * range, color);
        }
    }
    // What each tentacle can grasp (around its base), and where jaws away from the mouth bite.
    let rot = Vec2::from_angle(o.heading);
    let place = |x: f32, y: f32| center + rot.rotate(Vec2::new(x, y)) * o.radius;
    for z in &t.grasps[..t.grasp_count as usize] {
        gizmos.circle_2d(
            Isometry2d::from_translation(place(z.x, z.y)),
            z.radius * o.radius,
            Color::srgba(1.0, 0.5, 0.6, 0.3),
        );
    }
    for z in &t.bites[..t.bite_count as usize] {
        gizmos.circle_2d(
            Isometry2d::from_translation(place(z.x, z.y)),
            z.radius * o.radius,
            Color::srgba(1.0, 0.85, 0.4, 0.45),
        );
    }
}
