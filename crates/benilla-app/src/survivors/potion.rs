//! The on-demand potion: `Q` (or the cluster's potion button) heals a share of the pool at once,
//! then waits a minute of run time. It is for the "oh no" moment, not a rotation, and weaker on
//! the harder difficulties.

use bevy::prelude::*;

use super::data::{self, Difficulty};
use super::hero::Hero;
use super::units::Fx;
use super::{Phase, Run};

pub(super) const POTION_KEY: KeyCode = KeyCode::KeyQ;
/// Run seconds, so it follows the 2× speed and stops while paused.
pub(super) const POTION_COOLDOWN: f32 = 60.0;
/// Healing Potion: its visual and sound.
pub(super) const POTION_SPELL: u32 = 441;

/// The share of the maximum health one drink heals.
pub(super) fn potion_heal(d: Difficulty) -> f32 {
    match d {
        Difficulty::Normal => 0.30,
        Difficulty::Heroic => 0.25,
        Difficulty::Mythic => 0.20,
    }
}

/// Whether a press drinks: alive, in play, hurt, and off cooldown. At full health a press does
/// nothing and spends no cooldown; the button is dimmed to say so (batch 1 review).
pub(super) fn can_drink(run: &Run, phase: Phase) -> bool {
    phase == Phase::Playing
        && run.dead_t.is_none()
        && run.hp > 0.0
        && run.hp < run.stats().max_hp
        && run.potion_cd <= 0.0
}

/// The cooldown after `dt` seconds of run time.
fn tick(cd: f32, dt: f32) -> f32 {
    (cd - dt).max(0.0)
}

/// Ticks the cooldown on run time; drinks on `Q`, or for the Demo bot when it is low.
pub(super) fn drink_potion(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    phase: Res<State<Phase>>,
    mut run: ResMut<Run>,
    auto: Option<Res<super::auto::Auto>>,
    hero: Query<Entity, With<Hero>>,
    loading: Res<crate::loading_screen::LoadingScreen>,
    mut fx: Fx,
) {
    // Behind a battleground's loading screen the run waits, and so does the potion.
    if loading.covering() {
        return;
    }
    let dt = time.delta_secs();
    if run.potion_cd > 0.0 {
        run.potion_cd = tick(run.potion_cd, dt);
    }
    let wants = keys.just_pressed(POTION_KEY) || auto.is_some() && super::auto::wants_potion(&run);
    if !wants || !can_drink(&run, *phase.get()) {
        return;
    }
    let Ok(me) = hero.single() else { return };
    let max = run.stats().max_hp;
    let before = run.hp;
    run.hp = (run.hp + potion_heal(data::difficulty()) * max).min(max);
    run.potion_cd = POTION_COOLDOWN;
    fx.cast(me, POTION_SPELL, &[me], None);
    let healed = run.hp - before;
    if healed >= 0.5 {
        fx.heal_text(me, healed);
    }
    super::combatlog::event(run.elapsed, "potion");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn potion_heal_by_difficulty() {
        assert_eq!(potion_heal(Difficulty::Normal), 0.30);
        assert_eq!(potion_heal(Difficulty::Heroic), 0.25);
        assert_eq!(potion_heal(Difficulty::Mythic), 0.20);
    }

    #[test]
    fn potion_ignored_when_dead_or_paused() {
        let mut run = Run::default();
        run.hp = run.stats().max_hp * 0.5;
        assert!(can_drink(&run, Phase::Playing));
        // Full health: nothing, and no cooldown spent (the button only dims).
        let hurt = run.hp;
        run.hp = run.stats().max_hp;
        assert!(!can_drink(&run, Phase::Playing));
        run.hp = hurt;
        // Level-up choices wait in play (`cards`): a drink stays open while they do.
        for phase in [Phase::Paused, Phase::Menu, Phase::GameOver] {
            assert!(!can_drink(&run, phase), "{phase:?}");
        }
        run.dead_t = Some(0.0);
        assert!(!can_drink(&run, Phase::Playing));
        run.dead_t = None;
        run.potion_cd = 0.1;
        assert!(!can_drink(&run, Phase::Playing));
    }

    #[test]
    fn potion_cooldown_counts_run_time() {
        // A minute of run time at 1×: sixty one-second frames.
        let mut cd = POTION_COOLDOWN;
        for _ in 0..59 {
            cd = tick(cd, 1.0);
        }
        assert!(cd > 0.0);
        cd = tick(cd, 1.0);
        assert_eq!(cd, 0.0);
        // At 2× the run clock's frames are twice as long: thirty of them.
        let mut cd = POTION_COOLDOWN;
        for _ in 0..30 {
            cd = tick(cd, 2.0);
        }
        assert_eq!(cd, 0.0);
        // Paused, the run clock stands.
        assert_eq!(tick(12.0, 0.0), 12.0);
    }
}
