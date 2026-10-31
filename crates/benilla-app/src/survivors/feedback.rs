//! Screen shake for the moments that matter: a heavy blow to the hero, a boss down, the final boss
//! down. Rides the client's own shake (`crate::camera_shake`), the same sinusoid WoW's creature
//! footfalls use, with rows built here rather than read from `CameraShakes.dbc`.
//!
//! Proportional, as the game-feel tiers go: a medium shake for a blow worth 15% of the pool or more,
//! a large one for a boss. Ordinary kills and level-ups get nothing: they come every few seconds
//! and a shake on each would bury the ones that mean something. A setting (`super::settings`),
//! 100% / 50% / Off, kept in `benilla-config/shake.txt`.

use bevy::prelude::*;
use benilla_formats::CameraShake;

use benilla_world::view::WorldCamera;

use super::enemies::Loot;
use super::{Phase, Run};
use crate::camera_shake::CameraShakes;

/// Something that happened that may deserve a shake.
#[derive(Clone, Copy, Debug)]
pub(super) enum FeedbackEvent {
    /// The hero lost this share of the health pool in one frame.
    HeroHurt { frac_of_max: f32 },
    BossKilled,
    FinalBossKilled,
    /// Never raised: it exists so the rule "level-ups don't shake" is stated and tested.
    #[allow(dead_code)]
    LevelUp,
}

/// How hard an event shakes the view.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Tier {
    Medium,
    Large,
}

/// A blow worth this share of the pool or more shakes; smaller ones are routine.
const HURT_SHAKES_AT: f32 = 0.15;

pub(super) fn tier_for(event: FeedbackEvent) -> Option<Tier> {
    match event {
        FeedbackEvent::HeroHurt { frac_of_max } if frac_of_max >= HURT_SHAKES_AT => {
            Some(Tier::Medium)
        }
        FeedbackEvent::HeroHurt { .. } | FeedbackEvent::LevelUp => None,
        FeedbackEvent::BossKilled | FeedbackEvent::FinalBossKilled => Some(Tier::Large),
    }
}

/// The player's shake strength.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ShakeSetting {
    Full,
    Half,
    Off,
}

static SHAKE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

impl ShakeSetting {
    pub(super) fn current() -> Self {
        match SHAKE.load(std::sync::atomic::Ordering::Relaxed) {
            1 => Self::Half,
            2 => Self::Off,
            _ => Self::Full,
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Full => "100%",
            Self::Half => "50%",
            Self::Off => "Off",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Full => Self::Half,
            Self::Half => Self::Off,
            Self::Off => Self::Full,
        }
    }

    fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "50%" => Self::Half,
            "off" => Self::Off,
            _ => Self::Full,
        }
    }

    fn path() -> Option<std::path::PathBuf> {
        crate::local_state::home().map(|h| h.join("shake.txt"))
    }

    /// The saved choice into force; at startup.
    pub(super) fn load() {
        let s = Self::path()
            .and_then(|p| crate::local_state::read_to_string(&p).ok())
            .map(|s| Self::parse(&s))
            .unwrap_or(Self::Full);
        SHAKE.store(s as u8, std::sync::atomic::Ordering::Relaxed);
    }

    /// The next strength, in force and saved.
    pub(super) fn cycle() -> Self {
        let next = Self::current().next();
        SHAKE.store(next as u8, std::sync::atomic::Ordering::Relaxed);
        if let Some(path) = Self::path() {
            let _ = crate::local_state::write_atomic(&path, next.name());
        }
        next
    }
}

/// The amplitude scale a tier gets under a setting; 0 is no shake at all.
pub(super) fn scaled(tier: Tier, setting: ShakeSetting) -> f32 {
    let strength = match setting {
        ShakeSetting::Full => 1.0,
        ShakeSetting::Half => 0.5,
        ShakeSetting::Off => return 0.0,
    };
    let base = match tier {
        Tier::Medium => 0.4,
        Tier::Large => 0.8,
    };
    base * strength
}

/// The rows a tier fires, at amplitude `scale` (from [`scaled`]). Both decay (`shake_type` 1), so
/// they return to rest by themselves; a large one also sways sideways. Amplitudes are inches, as
/// the DBC's are: at 1.0 a 30-inch bob, past the 7 of a giant's footfall, because the survivors
/// camera hangs ~30 yd back where a footfall-sized shake would not read.
pub(super) fn rows(tier: Tier, scale: f32) -> impl Iterator<Item = CameraShake> {
    let bob = CameraShake {
        id: 0,
        shake_type: 1,
        direction: 2,
        amplitude: 30.0 * scale,
        frequency: 11.0,
        duration: if tier == Tier::Large { 0.55 } else { 0.3 },
        phase: 0.0,
        coefficient: if tier == Tier::Large { 5.0 } else { 9.0 },
    };
    let sway = (tier == Tier::Large).then_some(CameraShake {
        direction: 1,
        frequency: 7.0,
        amplitude: 18.0 * scale,
        ..bob
    });
    std::iter::once(bob).chain(sway)
}

pub(super) fn plugin(app: &mut App) {
    ShakeSetting::load();
    app.add_systems(
        Update,
        shake_on_events
            .run_if(in_state(Phase::Playing))
            .before(crate::camera_shake::apply_camera_shake),
    );
}

/// Turn this frame's blows and boss kills into shakes. The shake is spawned at the camera, so the
/// client's distance falloff leaves it at full strength.
fn shake_on_events(
    time: Res<Time>,
    run: Res<Run>,
    mut loot: MessageReader<Loot>,
    mut last: Local<Option<(f32, bool)>>,
    camera: Query<&Transform, With<WorldCamera>>,
    mut shakes: ResMut<CameraShakes>,
) {
    let max = run.stats().max_hp.max(1.0);
    let (prev_hp, was_won) = last.unwrap_or((run.hp, run.won));
    *last = Some((run.hp, run.won));

    let mut events = Vec::new();
    if prev_hp > run.hp {
        events.push(FeedbackEvent::HeroHurt {
            frac_of_max: (prev_hp - run.hp) / max,
        });
    }
    if loot.read().any(|l| l.boss) {
        events.push(FeedbackEvent::BossKilled);
    }
    if run.won && !was_won {
        events.push(FeedbackEvent::FinalBossKilled);
    }

    let setting = ShakeSetting::current();
    let Ok(eye) = camera.single().map(|t| t.translation) else {
        return;
    };
    let now = time.elapsed_secs();
    for tier in events.into_iter().filter_map(tier_for) {
        let scale = scaled(tier, setting);
        if scale <= 0.0 {
            continue;
        }
        for row in rows(tier, scale) {
            shakes.add(row, eye, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_heavy_blows_and_bosses_shake() {
        assert_eq!(tier_for(FeedbackEvent::HeroHurt { frac_of_max: 0.05 }), None);
        assert_eq!(
            tier_for(FeedbackEvent::HeroHurt { frac_of_max: 0.15 }),
            Some(Tier::Medium)
        );
        assert_eq!(tier_for(FeedbackEvent::BossKilled), Some(Tier::Large));
        assert_eq!(tier_for(FeedbackEvent::FinalBossKilled), Some(Tier::Large));
        assert_eq!(tier_for(FeedbackEvent::LevelUp), None);
    }

    #[test]
    fn off_is_no_shake_and_half_is_half() {
        assert_eq!(scaled(Tier::Large, ShakeSetting::Off), 0.0);
        assert_eq!(scaled(Tier::Medium, ShakeSetting::Off), 0.0);
        assert_eq!(
            scaled(Tier::Large, ShakeSetting::Half) * 2.0,
            scaled(Tier::Large, ShakeSetting::Full)
        );
    }

    #[test]
    fn shakes_decay_and_end() {
        for tier in [Tier::Medium, Tier::Large] {
            for row in rows(tier, 1.0) {
                assert_eq!(row.shake_type, 1, "decaying, so it returns to rest");
                assert!(row.duration > 0.0 && row.duration <= 0.6);
            }
        }
        assert_eq!(rows(Tier::Medium, 1.0).count(), 1);
        assert_eq!(rows(Tier::Large, 1.0).count(), 2);
    }

    #[test]
    fn setting_round_trips_by_name() {
        for s in [ShakeSetting::Full, ShakeSetting::Half, ShakeSetting::Off] {
            assert_eq!(ShakeSetting::parse(s.name()), s);
        }
    }
}
