//! The horde's per-enemy detail: what each survivors enemy costs the engine besides its body.
//! The engine pays per unit for a blob shadow, footprints, footstep lookups, pose evaluation and
//! event keys; at 220 enemies on a tablet that is the frame. The enemies opt out through the
//! engine's own markers and budgets; the hero and the bosses keep their full detail.

use benilla_world::rig_anim::AnimHalfRate;
use benilla_world::view::WorldCamera;
use bevy::prelude::*;

use super::enemies::Enemy;
use super::hero::Hero;
use crate::blob_shadow::{NoUnitShadow, ShadowReproject};
use crate::creature_anim::AnimEventsMuted;
use crate::footprints::NoFootprints;
use crate::sound::footsteps::FootstepHearing;

/// A shadowed unit walks this far (yd), or turns this far, before its shadow is re-projected; in
/// between it slides. A blob is a soft oval, so a quarter yard of terrain mismatch does not show.
const SHADOW_SLIDE: ShadowReproject = ShadowReproject {
    distance: 0.25,
    turn: 5.0 * std::f32::consts::PI / 180.0,
};

/// Footfalls farther than this from the listener (at the hero) are not looked up: the mixer
/// refused ~900 of them per 5 s anyway, inaudible under the hero's own effects.
const HEARING: f32 = 18.0;

/// Beyond this (yd) from the hero an enemy's pose is evaluated every second frame, and nearer
/// than [`HALF_RATE_EXIT`] it is back to every frame; the gap keeps a pacing enemy from flapping.
const HALF_RATE_ENTER: f32 = 25.0;
const HALF_RATE_EXIT: f32 = 22.0;

/// An enemy whose body radius spans less than this share of the view's height is tiny on screen
/// and evaluates every second frame too; above [`TINY_EXIT`] it is back.
const TINY_ENTER: f32 = 0.010;
const TINY_EXIT: f32 = 0.012;

/// Beyond the hearing range an enemy's event keys (footsteps, swings) are not scanned; within
/// [`MUTE_EXIT`] they are again.
const MUTE_ENTER: f32 = HEARING;
const MUTE_EXIT: f32 = 16.0;

pub(super) fn plugin(app: &mut App) {
    app.insert_resource(SHADOW_SLIDE)
        .insert_resource(FootstepHearing(HEARING))
        .add_systems(Update, (crowd_decals, crowd_anim_lod));
}

/// Whether ordinary enemies keep their shadow and footprints: only on High. Off Android there is
/// no preset, and a desktop has the headroom, so everything stays.
fn full_crowd_detail(
    #[cfg(target_os = "android")] graphics: Option<&super::graphics::Graphics>,
) -> bool {
    #[cfg(target_os = "android")]
    {
        graphics.is_none_or(|g| g.preset == super::graphics::Preset::High)
    }
    #[cfg(not(target_os = "android"))]
    {
        true
    }
}

/// Whether this enemy goes without the shadow and the footprints.
fn sheds_decals(boss: bool, full: bool) -> bool {
    !boss && !full
}

/// Put the shadow and footprint opt-outs on the ordinary enemies below High, and take them off on
/// High. Only new enemies and a preset change cost a pass.
fn crowd_decals(
    mut commands: Commands,
    #[cfg(target_os = "android")] graphics: Option<Res<super::graphics::Graphics>>,
    enemies: Query<(Entity, &Enemy, Has<NoUnitShadow>)>,
    fresh: Query<(), Added<Enemy>>,
    mut last_full: Local<Option<bool>>,
) {
    let full = full_crowd_detail(
        #[cfg(target_os = "android")]
        graphics.as_deref(),
    );
    if *last_full == Some(full) && fresh.is_empty() {
        return;
    }
    *last_full = Some(full);
    for (entity, enemy, shed) in &enemies {
        let want = sheds_decals(enemy.boss, full);
        if want == shed {
            continue;
        }
        if want {
            commands
                .entity(entity)
                .try_insert((NoUnitShadow, NoFootprints));
        } else {
            commands
                .entity(entity)
                .try_remove::<(NoUnitShadow, NoFootprints)>();
        }
    }
}

/// Whether an enemy evaluates its pose every second frame: far from the hero or tiny on screen,
/// with hysteresis on both; a boss never.
fn wants_half_rate(boss: bool, dist: f32, screen_share: f32, was: bool) -> bool {
    if boss {
        return false;
    }
    if was {
        dist > HALF_RATE_EXIT || screen_share < TINY_EXIT
    } else {
        dist > HALF_RATE_ENTER || screen_share < TINY_ENTER
    }
}

/// Whether an enemy's event keys go unscanned: out of earshot, with hysteresis; a boss never.
fn wants_muted(boss: bool, dist: f32, was: bool) -> bool {
    !boss && dist > if was { MUTE_EXIT } else { MUTE_ENTER }
}

/// The share of the view's height a sphere of `radius` at `dist` from the eye spans.
fn screen_share(radius: f32, dist: f32, fov_y: f32) -> f32 {
    radius / (dist.max(0.1) * 2.0 * (fov_y * 0.5).tan())
}

/// The animation detail by distance and size: the half-rate pose and the muted event keys, put on
/// and taken off only when an enemy crosses a threshold.
#[allow(clippy::type_complexity)]
fn crowd_anim_lod(
    mut commands: Commands,
    hero: Query<&Transform, (With<Hero>, Without<Enemy>)>,
    cam: Query<(&GlobalTransform, &Projection), With<WorldCamera>>,
    enemies: Query<(
        Entity,
        &Transform,
        &Enemy,
        Has<AnimHalfRate>,
        Has<AnimEventsMuted>,
    )>,
) {
    let Ok(hero) = hero.single() else { return };
    let eye = cam.single().ok().and_then(|(g, p)| match p {
        Projection::Perspective(p) => Some((g.translation(), p.fov)),
        _ => None,
    });
    for (entity, tf, enemy, half, muted) in &enemies {
        let dist = tf.translation.distance(hero.translation);
        let share = eye.map_or(1.0, |(at, fov)| {
            screen_share(enemy.radius, tf.translation.distance(at), fov)
        });
        let want_half = wants_half_rate(enemy.boss, dist, share, half);
        let want_muted = wants_muted(enemy.boss, dist, muted);
        if want_half != half {
            if want_half {
                commands.entity(entity).try_insert(AnimHalfRate);
            } else {
                commands.entity(entity).try_remove::<AnimHalfRate>();
            }
        }
        if want_muted != muted {
            if want_muted {
                commands.entity(entity).try_insert(AnimEventsMuted);
            } else {
                commands.entity(entity).try_remove::<AnimEventsMuted>();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Below High ordinary enemies shed both decals; bosses never do, and High keeps everyone's.
    #[test]
    fn only_ordinary_enemies_shed_decals_and_only_below_high() {
        assert!(sheds_decals(false, false));
        assert!(!sheds_decals(true, false), "a boss keeps its shadow");
        assert!(!sheds_decals(false, true), "High keeps every shadow");
        assert!(!sheds_decals(true, true));
    }

    /// Far or tiny goes half rate, with a gap before it comes back; a boss never does.
    #[test]
    fn the_half_rate_gate_has_hysteresis_and_spares_bosses() {
        let big = 0.05;
        assert!(!wants_half_rate(false, 24.0, big, false));
        assert!(wants_half_rate(false, 26.0, big, false));
        assert!(
            wants_half_rate(false, 23.0, big, true),
            "inside the gap it stays"
        );
        assert!(!wants_half_rate(false, 21.0, big, true));
        assert!(wants_half_rate(false, 5.0, 0.005, false), "tiny on screen");
        assert!(wants_half_rate(false, 5.0, 0.011, true));
        assert!(!wants_half_rate(false, 5.0, 0.011, false));
        assert!(
            !wants_half_rate(true, 70.0, 0.001, false),
            "a boss is always full rate"
        );
    }

    /// Out of earshot mutes the keys, with a gap before they sound again; a boss never.
    #[test]
    fn the_hearing_gate_has_hysteresis_and_spares_bosses() {
        assert!(!wants_muted(false, 17.0, false));
        assert!(wants_muted(false, 18.5, false));
        assert!(wants_muted(false, 17.0, true));
        assert!(!wants_muted(false, 15.0, true));
        assert!(!wants_muted(true, 60.0, false));
    }

    /// A body at the survivors camera's default reach is not tiny; at three times it is.
    #[test]
    fn screen_share_shrinks_with_distance() {
        let fov = std::f32::consts::FRAC_PI_4;
        assert!(screen_share(0.6, 30.0, fov) > TINY_EXIT);
        assert!(screen_share(0.6, 90.0, fov) < TINY_ENTER);
    }
}
