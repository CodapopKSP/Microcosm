//! Bevy visual debug overlays (Phase 2H).
//!
//! - **L** — toggle light-field background gradient
//! - **Click** — select agent (energy bar + light ring)
//! - **Esc** — clear selection

use bevy::prelude::*;
use bevy::sprite::ColorMaterial;

use crate::sim::{
    Agent, Energy, LightExposure, LightField, Phenotype, SimConfigResource, WorldBounds,
};

/// Keyboard-driven debug overlay state (visual mode only).
#[derive(Resource)]
pub struct DebugVisualState {
    pub show_light_gradient: bool,
    pub selected: Option<Entity>,
}

impl Default for DebugVisualState {
    fn default() -> Self {
        Self {
            show_light_gradient: true,
            selected: None,
        }
    }
}

#[derive(Component)]
struct LightGradientBand;

#[derive(Component)]
struct DebugSelectionRing;

#[derive(Component)]
struct DebugEnergyBarBg;

#[derive(Component)]
struct DebugEnergyBarFill;

/// Registers Phase 2H debug overlay systems.
pub struct VisualDebugPlugin;

impl Plugin for VisualDebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugVisualState>()
            .add_systems(Startup, (spawn_light_gradient, spawn_selection_overlays))
            .add_systems(
                Update,
                (
                    debug_keyboard_input,
                    select_agent_on_click,
                    sync_light_gradient_visibility,
                    update_selection_overlay,
                ),
            );
    }
}

/// Map a light sample to a display color (shade → bright).
pub fn light_sample_to_display_color(light: f32, gradient_min: f32, gradient_max: f32) -> Color {
    let span = (gradient_max - gradient_min).max(1e-6);
    let t = ((light - gradient_min) / span).clamp(0.0, 1.0);
    const DARK: [f32; 3] = [0.07, 0.05, 0.16];
    const LIGHT: [f32; 3] = [0.84, 0.87, 0.45]; // cool off-white (B slightly above R/G)
    Color::srgb(
        DARK[0] + t * (LIGHT[0] - DARK[0]),
        DARK[1] + t * (LIGHT[1] - DARK[1]),
        DARK[2] + t * (LIGHT[2] - DARK[2]),
    )
}

const GRADIENT_BANDS: u32 = 48;
const ENERGY_BAR_HEIGHT: f32 = 6.0;
const ENERGY_BAR_GAP: f32 = 8.0;
const OVERLAY_Z: f32 = 5.0;
const GRADIENT_Z: f32 = -10.0;

fn spawn_light_gradient(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    light: Res<LightField>,
    config: Res<SimConfigResource>,
    bounds: Res<WorldBounds>,
    state: Res<DebugVisualState>,
) {
    let env = &config.0.environment;
    let width = bounds.half_x * 2.0;
    let height = bounds.half_y * 2.0;
    let band_h = height / GRADIENT_BANDS as f32;
    let visibility = if state.show_light_gradient {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    for i in 0..GRADIENT_BANDS {
        let center_y = -bounds.half_y + band_h * (i as f32 + 0.5);
        let light_val = light.sample(Vec2::new(0.0, center_y));
        let color = light_sample_to_display_color(light_val, env.gradient_min, env.gradient_max);
        commands.spawn((
            LightGradientBand,
            Mesh2d(meshes.add(Rectangle::new(width, band_h))),
            MeshMaterial2d(materials.add(ColorMaterial::from(color))),
            Transform::from_xyz(0.0, center_y, GRADIENT_Z),
            visibility,
        ));
    }
}

fn spawn_selection_overlays(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((
        DebugSelectionRing,
        Mesh2d(meshes.add(Circle::new(1.0))),
        MeshMaterial2d(materials.add(ColorMaterial::from(Color::srgba(1.0, 1.0, 0.6, 0.35)))),
        Transform::from_xyz(0.0, 0.0, 1.0),
        Visibility::Hidden,
    ));
    commands.spawn((
        DebugEnergyBarBg,
        Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial2d(materials.add(ColorMaterial::from(Color::srgba(0.1, 0.1, 0.1, 0.75)))),
        Transform::from_xyz(0.0, 0.0, OVERLAY_Z),
        Visibility::Hidden,
    ));
    commands.spawn((
        DebugEnergyBarFill,
        Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial2d(materials.add(ColorMaterial::from(Color::srgb(0.2, 0.85, 0.35)))),
        Transform::from_xyz(0.0, 0.0, OVERLAY_Z + 0.1),
        Visibility::Hidden,
    ));
}

fn debug_keyboard_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<DebugVisualState>,
) {
    if keyboard.just_pressed(KeyCode::KeyL) {
        state.show_light_gradient = !state.show_light_gradient;
    }
    if keyboard.just_pressed(KeyCode::Escape) {
        state.selected = None;
    }
}

fn sync_light_gradient_visibility(
    state: Res<DebugVisualState>,
    mut bands: Query<&mut Visibility, With<LightGradientBand>>,
) {
    if !state.is_changed() {
        return;
    }
    let visibility = if state.show_light_gradient {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut vis in &mut bands {
        *vis = visibility;
    }
}

fn cursor_world_2d(
    window: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
) -> Option<Vec2> {
    let window = window.get_single().ok()?;
    let (camera, transform) = camera.get_single().ok()?;
    let cursor = window.cursor_position()?;
    camera.viewport_to_world_2d(transform, cursor).ok()
}

fn select_agent_on_click(
    mouse: Res<ButtonInput<MouseButton>>,
    mut state: ResMut<DebugVisualState>,
    window: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    agents: Query<(Entity, &Transform, &Agent)>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(world_pos) = cursor_world_2d(window, camera) else {
        return;
    };

    let mut best: Option<(Entity, f32)> = None;
    for (entity, transform, agent) in &agents {
        let pos = transform.translation.truncate();
        let dist_sq = pos.distance_squared(world_pos);
        if dist_sq <= agent.radius * agent.radius {
            match best {
                Some((_, best_dist)) if dist_sq >= best_dist => {}
                _ => best = Some((entity, dist_sq)),
            }
        }
    }
    state.selected = best.map(|(entity, _)| entity);
}

type RingOverlayQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Transform,
        &'static mut Visibility,
        &'static MeshMaterial2d<ColorMaterial>,
    ),
    (
        With<DebugSelectionRing>,
        Without<Agent>,
        Without<DebugEnergyBarBg>,
        Without<DebugEnergyBarFill>,
    ),
>;

type BarBgOverlayQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Transform, &'static mut Visibility),
    (
        With<DebugEnergyBarBg>,
        Without<Agent>,
        Without<DebugSelectionRing>,
        Without<DebugEnergyBarFill>,
    ),
>;

type BarFillOverlayQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Transform, &'static mut Visibility),
    (
        With<DebugEnergyBarFill>,
        Without<Agent>,
        Without<DebugSelectionRing>,
        Without<DebugEnergyBarBg>,
    ),
>;

fn update_selection_overlay(
    state: Res<DebugVisualState>,
    mut overlays: ParamSet<(
        RingOverlayQuery,
        BarBgOverlayQuery,
        BarFillOverlayQuery,
    )>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    config: Res<SimConfigResource>,
    agents: Query<
        (&Transform, &Agent, &Energy, &Phenotype, &LightExposure),
        (
            With<Agent>,
            Without<DebugSelectionRing>,
            Without<DebugEnergyBarBg>,
            Without<DebugEnergyBarFill>,
        ),
    >,
) {
    if state.selected.is_none() {
        if let Ok((_, mut vis, _)) = overlays.p0().get_single_mut() {
            *vis = Visibility::Hidden;
        }
        if let Ok((_, mut vis)) = overlays.p1().get_single_mut() {
            *vis = Visibility::Hidden;
        }
        if let Ok((_, mut vis)) = overlays.p2().get_single_mut() {
            *vis = Visibility::Hidden;
        }
        return;
    }

    let selected = state.selected.unwrap();

    let Ok((transform, agent, energy, phenotype, exposure)) = agents.get(selected) else {
        if let Ok((_, mut vis, _)) = overlays.p0().get_single_mut() {
            *vis = Visibility::Hidden;
        }
        if let Ok((_, mut vis)) = overlays.p1().get_single_mut() {
            *vis = Visibility::Hidden;
        }
        if let Ok((_, mut vis)) = overlays.p2().get_single_mut() {
            *vis = Visibility::Hidden;
        }
        return;
    };

    let pos = transform.translation.truncate();
    let env = &config.0.environment;
    let ring_color = light_sample_to_display_color(
        exposure.0,
        env.gradient_min,
        env.gradient_max,
    )
    .with_alpha(0.45);

    if let Ok((mut ring_tf, mut vis, mat)) = overlays.p0().get_single_mut() {
        *vis = Visibility::Visible;
        ring_tf.translation = pos.extend(1.0);
        ring_tf.scale = Vec3::splat(agent.radius + 4.0);
        if let Some(material) = materials.get_mut(&mat.0) {
            material.color = ring_color;
        }
    }

    let bar_width = agent.radius * 2.0;
    let bar_y = pos.y + agent.radius + ENERGY_BAR_GAP;
    let threshold = phenotype.0.repro_threshold.max(1e-3);
    let ratio = (energy.internal_energy / threshold).clamp(0.0, 1.0);
    let fill_width = bar_width * ratio;

    if let Ok((mut bg_tf, mut vis)) = overlays.p1().get_single_mut() {
        *vis = Visibility::Visible;
        bg_tf.translation = Vec3::new(pos.x, bar_y, OVERLAY_Z);
        bg_tf.scale = Vec3::new(bar_width, ENERGY_BAR_HEIGHT, 1.0);
    }

    if let Ok((mut fill_tf, mut vis)) = overlays.p2().get_single_mut() {
        if fill_width > 0.0 {
            *vis = Visibility::Visible;
            fill_tf.translation = Vec3::new(
                pos.x - bar_width / 2.0 + fill_width / 2.0,
                bar_y,
                OVERLAY_Z + 0.1,
            );
            fill_tf.scale = Vec3::new(fill_width, ENERGY_BAR_HEIGHT, 1.0);
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::sim::LightField;

    #[test]
    fn display_color_increases_with_light_sample() {
        let config = SimConfig::load_default().unwrap();
        let min = config.environment.gradient_min;
        let max = config.environment.gradient_max;
        let dark = light_sample_to_display_color(min, min, max);
        let bright = light_sample_to_display_color(max, min, max);
        let dark_s = dark.to_srgba();
        let bright_s = bright.to_srgba();
        assert!(bright_s.red > dark_s.red);
        assert!(bright_s.green > dark_s.green);
        assert!(bright_s.blue > dark_s.blue);
        assert!(bright_s.blue >= bright_s.red);
    }

    #[test]
    fn gradient_bands_cover_world_height() {
        let config = SimConfig::load_default().unwrap();
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let field = LightField::from_environment(&config.environment, &bounds);
        let height = bounds.half_y * 2.0;
        let band_h = height / GRADIENT_BANDS as f32;
        let bottom = field.sample(Vec2::new(0.0, -bounds.half_y + band_h * 0.5));
        let top = field.sample(Vec2::new(
            0.0,
            -bounds.half_y + band_h * (GRADIENT_BANDS as f32 - 0.5),
        ));
        assert!(top > bottom);
    }
}
