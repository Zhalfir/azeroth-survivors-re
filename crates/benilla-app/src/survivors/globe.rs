//! The health drop: a glowing red globe over a soft red pool of light, bobbing and pulsing, as the
//! reference genre draws it. The turkey leg it replaces read as scenery among the wisps (tablet
//! feedback, 2026-11-01).

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::RunEntity;

/// The globe's radius, in yards.
const BALL_RADIUS: f32 = 0.45;
/// The ground glow's radius, in yards.
const DECAL_RADIUS: f32 = 1.2;
/// `#ff2a1a`, its emissive strength.
const GLOW: LinearRgba = LinearRgba::rgb(1.0, 0.023, 0.010);
const GLOW_GAIN: f32 = 4.0;
/// The bob: 1.5 Hz, ±0.12 yd.
const BOB_HZ: f32 = 1.5;
const BOB_YD: f32 = 0.12;
/// The glow's pulse: 2 Hz, between 0.8 and 1.2 of its strength.
const PULSE_HZ: f32 = 2.0;
const PULSE_DEPTH: f32 = 0.2;
/// The ball's rest height over the pickup's origin, which sits half a yard over the ground.
const BALL_REST: f32 = 0.15;

/// The glow's strength at time `t`: 1 ± 0.2, every half second.
pub(super) fn pulse(t: f32) -> f32 {
    1.0 + PULSE_DEPTH * (std::f32::consts::TAU * PULSE_HZ * t).sin()
}

fn bob(t: f32) -> f32 {
    BOB_YD * (std::f32::consts::TAU * BOB_HZ * t).sin()
}

/// The meshes and materials every globe shares; the pulse writes the one material once a frame.
#[derive(Resource)]
pub(super) struct GlobeAssets {
    ball: Handle<Mesh>,
    ball_mat: Handle<StandardMaterial>,
    decal: Handle<Mesh>,
    decal_mat: Handle<StandardMaterial>,
}

#[derive(Component)]
pub(super) struct GlobeBall;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Startup, setup_assets)
        .add_systems(Update, animate_globes);
}

fn setup_assets(
    mut commands: Commands,
    meshes: Option<ResMut<Assets<Mesh>>>,
    mats: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(mut meshes), Some(mut mats)) = (meshes, mats) else {
        return;
    };
    let ball = meshes.add(Sphere::new(BALL_RADIUS).mesh().uv(16, 10));
    let ball_mat = mats.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.02, 0.02),
        emissive: GLOW * GLOW_GAIN,
        perceptual_roughness: 0.3,
        ..default()
    });
    let decal = meshes.add(glow_disc(DECAL_RADIUS, 24));
    let decal_mat = mats.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.16, 0.10, 0.9),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        cull_mode: None,
        ..default()
    });
    commands.insert_resource(GlobeAssets {
        ball,
        ball_mat,
        decal,
        decal_mat,
    });
}

/// A flat disc on XZ whose vertex colour fades from the centre (opaque) to the rim (clear): a
/// soft radial glow with no texture.
fn glow_disc(radius: f32, segments: u32) -> Mesh {
    let mut pos = vec![[0.0, 0.0, 0.0]];
    let mut color = vec![[1.0, 1.0, 1.0, 1.0]];
    let mut normal = vec![[0.0, 1.0, 0.0]];
    let mut uv = vec![[0.5, 0.5]];
    for i in 0..segments {
        let a = i as f32 / segments as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        pos.push([c * radius, 0.0, s * radius]);
        color.push([1.0, 1.0, 1.0, 0.0]);
        normal.push([0.0, 1.0, 0.0]);
        uv.push([0.5 + 0.5 * c, 0.5 + 0.5 * s]);
    }
    let mut idx = Vec::new();
    for i in 0..segments {
        idx.extend_from_slice(&[0, 1 + (i + 1) % segments, 1 + i]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normal)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, color)
    .with_inserted_indices(Indices::U32(idx))
}

/// A health globe at `pos` (the pickup's origin, half a yard over the ground), `scale` times its
/// size; the caller adds the pickup itself.
pub(super) fn spawn_globe(
    commands: &mut Commands,
    assets: &GlobeAssets,
    pos: Vec3,
    scale: f32,
) -> Entity {
    commands
        .spawn((
            Transform::from_translation(pos).with_scale(Vec3::splat(scale)),
            Visibility::default(),
            RunEntity,
        ))
        .with_children(|g| {
            g.spawn((
                GlobeBall,
                Mesh3d(assets.ball.clone()),
                MeshMaterial3d(assets.ball_mat.clone()),
                Transform::from_xyz(0.0, BALL_REST, 0.0),
            ));
            // On the ground, a hair above it so it doesn't fight the terrain.
            g.spawn((
                Mesh3d(assets.decal.clone()),
                MeshMaterial3d(assets.decal_mat.clone()),
                Transform::from_xyz(0.0, -0.45, 0.0),
                bevy::light::NotShadowCaster,
            ));
        })
        .id()
}

/// The bob on every ball, and the shared glow's pulse, only while a globe is out.
fn animate_globes(
    time: Res<Time>,
    assets: Option<Res<GlobeAssets>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut balls: Query<&mut Transform, With<GlobeBall>>,
) {
    if balls.is_empty() {
        return;
    }
    let t = time.elapsed_secs();
    let y = BALL_REST + bob(t);
    for mut tf in &mut balls {
        tf.translation.y = y;
    }
    if let Some(m) = assets.and_then(|a| mats.get_mut(&a.ball_mat)) {
        m.emissive = GLOW * (GLOW_GAIN * pulse(t));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globe_pulse_is_periodic_and_bounded() {
        for i in 0..400 {
            let t = i as f32 * 0.0137;
            let p = pulse(t);
            assert!((0.8 - 1e-4..=1.2 + 1e-4).contains(&p), "pulse({t}) = {p}");
            assert!((pulse(t + 0.5) - p).abs() < 1e-3, "period 0.5 s at {t}");
        }
        // It does swing: the peak a quarter period in.
        assert!((pulse(0.125) - 1.2).abs() < 1e-4);
        assert!((pulse(0.375) - 0.8).abs() < 1e-4);
    }
}
