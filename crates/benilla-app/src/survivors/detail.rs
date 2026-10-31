//! The horde's per-enemy detail: what each survivors enemy costs the engine besides its body.
//! The engine pays per unit for a blob shadow and footprints; at 220 enemies on a tablet that is
//! the frame. The enemies opt out through the engine's own markers; the hero and the bosses keep
//! their full detail.

use bevy::prelude::*;

use super::enemies::Enemy;
use crate::blob_shadow::{NoUnitShadow, ShadowReproject};
use crate::footprints::NoFootprints;

/// A shadowed unit walks this far (yd), or turns this far, before its shadow is re-projected; in
/// between it slides. A blob is a soft oval, so a quarter yard of terrain mismatch does not show.
const SHADOW_SLIDE: ShadowReproject = ShadowReproject {
    distance: 0.25,
    turn: 5.0 * std::f32::consts::PI / 180.0,
};

pub(super) fn plugin(app: &mut App) {
    app.insert_resource(SHADOW_SLIDE)
        .add_systems(Update, crowd_decals);
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
}
