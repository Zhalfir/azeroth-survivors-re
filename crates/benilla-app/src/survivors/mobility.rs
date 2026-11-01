//! The class's mobility spell: learned at [`LEARN_LEVEL`], cast only on `F`, never by itself. Each
//! is the class's own signature escape: a mage blinks, a rogue sprints, a warrior charges.

use bevy::prelude::*;

use super::enemies::Enemy;
use super::hero::{self, Hero};
use super::units::{flat, Fx};
use super::{Phase, Run};
use benilla_world::world_point::WorldPoint;

pub(super) const LEARN_LEVEL: u32 = 5;
pub(super) const KEY: KeyCode = KeyCode::KeyF;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Move {
    /// Jump this many yards along the way the hero faces.
    Blink(f32),
    /// Lunge this many yards along the way the hero moves (or faces, standing) in
    /// [`LUNGE_SECS`]; the first enemy on the path stops it, and everything near is stunned and
    /// knocked back ([`charge_path`]).
    Charge(f32),
    /// Run faster by this factor for `secs`.
    Haste(f32),
    /// Haste, and enemies lose the hero (they stop chasing) for `secs`.
    Vanish(f32),
    /// Haste, and every enemy within this many yards flees in fear.
    Scatter(f32),
}

pub(super) struct Mobility {
    pub spell: u32,
    pub name: &'static str,
    pub kind: Move,
    pub cooldown: f32,
    pub secs: f32,
    pub desc: &'static str,
}

/// One per class, indexed by class id.
pub(super) fn for_class(class: u8) -> Option<&'static Mobility> {
    MOBILITY.iter().find(|(c, _)| *c == class).map(|(_, m)| m)
}

static MOBILITY: &[(u8, Mobility)] = &[
    (
        1,
        Mobility {
            spell: 100,
            name: "Charge",
            kind: Move::Charge(25.0),
            cooldown: 8.0,
            secs: 1.5,
            desc: "Charge up to 25 yards the way you are running. The first enemy in your path \
                   stops you: every enemy within 4 yards is stunned for 1.5 sec and knocked back.",
        },
    ),
    (
        2,
        Mobility {
            spell: 1044,
            name: "Blessing of Freedom",
            kind: Move::Haste(1.6),
            cooldown: 15.0,
            secs: 5.0,
            desc: "Nothing can hold you: run 60% faster for 5 sec.",
        },
    ),
    (
        3,
        Mobility {
            spell: 5118,
            name: "Aspect of the Cheetah",
            kind: Move::Haste(1.7),
            cooldown: 14.0,
            secs: 5.0,
            desc: "Run 70% faster for 5 sec.",
        },
    ),
    (
        4,
        Mobility {
            spell: 2983,
            name: "Sprint",
            kind: Move::Haste(1.8),
            cooldown: 15.0,
            secs: 4.0,
            desc: "Run 80% faster for 4 sec.",
        },
    ),
    (
        5,
        Mobility {
            spell: 586,
            name: "Fade",
            kind: Move::Vanish(1.4),
            cooldown: 18.0,
            secs: 4.0,
            desc: "Fade from the horde's sight: enemies lose you and you run 40% faster for 4 sec.",
        },
    ),
    (
        7,
        Mobility {
            spell: 2645,
            name: "Ghost Wolf",
            kind: Move::Haste(1.6),
            cooldown: 15.0,
            secs: 6.0,
            desc: "Take the form of a ghost wolf, running 60% faster for 6 sec.",
        },
    ),
    (
        8,
        Mobility {
            spell: 1953,
            name: "Blink",
            kind: Move::Blink(18.0),
            cooldown: 10.0,
            secs: 0.0,
            desc: "Teleport 18 yards forward.",
        },
    ),
    (
        9,
        Mobility {
            spell: 5484,
            name: "Howl of Terror",
            kind: Move::Scatter(12.0),
            cooldown: 20.0,
            secs: 4.0,
            desc: "Howl, sending every enemy within 12 yards fleeing in terror for 4 sec.",
        },
    ),
    (
        11,
        Mobility {
            spell: 1850,
            name: "Dash",
            kind: Move::Haste(1.8),
            cooldown: 15.0,
            secs: 4.0,
            desc: "Run 80% faster for 4 sec.",
        },
    ),
];

/// The mobility's run state, on [`Run`]: cooldown left, and the haste, vanish or charge still
/// running.
#[derive(Default, Clone, Copy)]
pub(super) struct MobilityState {
    pub cd: f32,
    pub haste: f32,
    pub haste_t: f32,
    pub vanish_t: f32,
    pub lunge: Option<Lunge>,
}

/// A Charge under way: from, to, the seconds into it, and whether an enemy stops it at `to`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Lunge {
    from: Vec3,
    to: Vec3,
    t: f32,
    hit: bool,
}

/// A Charge's travel time, whatever its length: as quick as a Blink to the eye.
pub(super) const LUNGE_SECS: f32 = 0.25;
/// An enemy this close to the charge's line stops it, this far short of it.
const CHARGE_REACH: f32 = 1.5;
/// At the stop, enemies within this are stunned for the spell's `secs` and knocked back.
const CHARGE_STUN_RADIUS: f32 = 4.0;
const CHARGE_KNOCKBACK: f32 = 3.0;
/// The run speed the animation is told during the lunge: past `FAST_RUN_SPEED` (11), so the
/// Sprint gait plays, without the rate scaling a 100 yd/s figure would bring.
const LUNGE_ANIM_SPEED: f32 = 14.0;

/// Where a charge from `from` along `dir`, `len` yards long, ends on the ground plane (x, z):
/// short of the first enemy within [`CHARGE_REACH`] of its line, with that enemy's index, or at
/// full length with none.
pub(super) fn charge_path(
    from: Vec2,
    dir: Vec2,
    len: f32,
    enemies: &[Vec2],
) -> (Vec2, Option<usize>) {
    let dir = dir.normalize_or(Vec2::NEG_Y);
    let mut first: Option<(f32, usize)> = None;
    for (i, &e) in enemies.iter().enumerate() {
        let rel = e - from;
        let along = rel.dot(dir);
        if !(0.0..=len + CHARGE_REACH).contains(&along) {
            continue;
        }
        if (rel - dir * along).length() > CHARGE_REACH {
            continue;
        }
        if first.is_none_or(|(a, _)| along < a) {
            first = Some((along, i));
        }
    }
    match first {
        Some((along, i)) => (from + dir * (along - CHARGE_REACH).clamp(0.0, len), Some(i)),
        None => (from + dir * len, None),
    }
}

impl Run {
    /// The class's mobility once the hero is level enough.
    pub(super) fn mobility(&self) -> Option<&'static Mobility> {
        (self.level >= LEARN_LEVEL)
            .then(|| for_class(self.class.id))
            .flatten()
    }

    /// While a Charge carries the hero: the speed the animation is told, so the Sprint gait plays
    /// for the whole lunge ([`super::hero::drive_hero`] holds it and leaves the hero's place to
    /// [`cast_mobility`]).
    pub(super) fn lunge_speed(&self) -> Option<f32> {
        self.mobile.lunge.map(|_| LUNGE_ANIM_SPEED)
    }

    /// The run speed's multiplier from a haste still running.
    pub(super) fn haste_mult(&self) -> f32 {
        if self.mobile.haste_t > 0.0 {
            self.mobile.haste
        } else {
            1.0
        }
    }
}

/// `F`: cast the mobility if it is learned and ready; ticks the cooldown and the running effect.
#[allow(clippy::too_many_arguments)]
pub(super) fn cast_mobility(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    phase: Res<State<Phase>>,
    mut run: ResMut<Run>,
    world: WorldPoint,
    mut hero: Query<(Entity, &mut Transform), With<Hero>>,
    mut enemies: Query<(Entity, &mut Transform, &mut Enemy), Without<Hero>>,
    selection: Res<super::Selection>,
    aim: Res<hero::AimPoint>,
    carry: Res<hero::MoveCarry>,
    auto: Option<Res<super::auto::Auto>>,
    mut fx: Fx,
) {
    let dt = time.delta_secs();
    let m = &mut run.mobile;
    m.cd = (m.cd - dt).max(0.0);
    m.haste_t = (m.haste_t - dt).max(0.0);
    m.vanish_t = (m.vanish_t - dt).max(0.0);
    let ground = |p: Vec3| world.terrain_height_under(p).unwrap_or(p.y);
    // A Charge under way carries the hero; at its end, the stop's stun and knockback.
    if let Some(mut l) = run.mobile.lunge {
        l.t += dt;
        let k = (l.t / LUNGE_SECS).min(1.0);
        if let Ok((_, mut tf)) = hero.single_mut() {
            let mut at = l.from.lerp(l.to, k);
            at.y = ground(at);
            if tf.translation != at {
                tf.translation = at;
            }
        }
        run.mobile.lunge = if k >= 1.0 { None } else { Some(l) };
        if k >= 1.0 && l.hit {
            let secs = run.mobility().map_or(1.5, |d| d.secs);
            let away = flat(l.to - l.from).normalize_or(Vec3::NEG_Z);
            for (_, mut t, mut en) in &mut enemies {
                let off = flat(t.translation - l.to);
                if en.dead || off.length() > CHARGE_STUN_RADIUS {
                    continue;
                }
                en.stun_t = en.stun_t.max(secs);
                let mut to = t.translation + off.normalize_or(away) * CHARGE_KNOCKBACK;
                to.y = ground(to);
                t.translation = to;
            }
        }
    }
    if *phase.get() != Phase::Playing || run.dead_t.is_some() || !keys.just_pressed(KEY) {
        return;
    }
    let Some(def) = run.mobility() else { return };
    if run.mobile.cd > 0.0 {
        return;
    }
    let Ok((me, mut tf)) = hero.single_mut() else {
        return;
    };
    let pos = tf.translation;
    let facing = tf.rotation * Vec3::NEG_Z;
    let facing = flat(facing).normalize_or(Vec3::NEG_Z);
    // Aimed at the cursor where there is one, else straight ahead.
    let cursor = aim.0.filter(|a| flat(*a - pos).length() > 0.5);
    let toward = cursor.map_or(facing, |a| flat(a - pos).normalize());
    // Keep any jump inside the battleground's leash.
    let map = selection.map_def();
    let center = hero::arena(map);
    let leash = |p: Vec3| {
        let off = flat(p - center);
        if off.length() > map.leash {
            center + off.normalize() * map.leash
        } else {
            p
        }
    };
    match def.kind {
        Move::Blink(yd) => {
            fx.cast(me, def.spell, &[me], None);
            // To the cursor, or as far as the spell reaches toward it.
            let far = cursor.map_or(yd, |a| flat(a - pos).length().min(yd));
            tf.rotation = Quat::from_rotation_y(super::units::yaw_toward(toward));
            let mut to = leash(pos + toward * far);
            to.y = ground(to);
            tf.translation = to;
        }
        Move::Charge(yd) => {
            // The way the hero runs (the bot's steer, else last frame's stick), else ahead.
            let run_dir = auto.as_deref().map_or(carry.heading(), super::auto::steer);
            let dir = flat(run_dir).normalize_or(facing);
            let alive: Vec<(Entity, Vec2)> = enemies
                .iter()
                .filter(|(_, _, e)| !e.dead)
                .map(|(e, t, _)| (e, t.translation.xz()))
                .collect();
            let spots: Vec<Vec2> = alive.iter().map(|a| a.1).collect();
            let (stop, first) = charge_path(pos.xz(), dir.xz(), yd, &spots);
            let mut to = leash(Vec3::new(stop.x, pos.y, stop.y));
            to.y = ground(to);
            tf.rotation = Quat::from_rotation_y(super::units::yaw_toward(dir));
            let target = first.map(|i| alive[i].0);
            fx.cast(me, def.spell, &[target.unwrap_or(me)], None);
            run.mobile.lunge = Some(Lunge {
                from: pos,
                to,
                t: 0.0,
                hit: target.is_some(),
            });
        }
        Move::Haste(x) => {
            fx.cast(me, def.spell, &[me], None);
            run.mobile.haste = x;
            run.mobile.haste_t = def.secs;
        }
        Move::Vanish(x) => {
            fx.cast(me, def.spell, &[me], None);
            run.mobile.haste = x;
            run.mobile.haste_t = def.secs;
            run.mobile.vanish_t = def.secs;
        }
        Move::Scatter(r) => {
            fx.cast(me, def.spell, &[me], None);
            for (_, t, mut en) in &mut enemies {
                if !en.dead && !en.boss && hero::reach(pos, t.translation) <= r {
                    en.fear_t = en.fear_t.max(def.secs);
                }
            }
        }
    }
    run.mobile.cd = def.cooldown;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charge_lunges_without_target() {
        let (to, hit) = charge_path(Vec2::ZERO, Vec2::new(0.0, 2.0), 25.0, &[]);
        assert_eq!(hit, None);
        assert!((to - Vec2::new(0.0, 25.0)).length() < 1e-4);
        // No direction at all: straight ahead, never nowhere.
        let (to, _) = charge_path(Vec2::ZERO, Vec2::ZERO, 10.0, &[]);
        assert!((to.length() - 10.0).abs() < 1e-4);
    }

    #[test]
    fn charge_stops_at_first_enemy_on_path() {
        let enemies = [
            Vec2::new(0.5, 15.0),
            Vec2::new(-1.0, 8.0),
            Vec2::new(0.0, 20.0),
        ];
        let (to, hit) = charge_path(Vec2::ZERO, Vec2::Y, 25.0, &enemies);
        assert_eq!(hit, Some(1));
        // Short of it by the reach, along the line.
        assert!((to - Vec2::new(0.0, 8.0 - CHARGE_REACH)).length() < 1e-4);
        // One within reach of the start: no lunge backwards, the hero stays.
        let (to, hit) = charge_path(Vec2::ZERO, Vec2::Y, 25.0, &[Vec2::new(0.0, 1.0)]);
        assert_eq!(hit, Some(0));
        assert_eq!(to, Vec2::ZERO);
    }

    #[test]
    fn charge_ignores_enemies_off_path() {
        let enemies = [
            // Beside the line, past the reach.
            Vec2::new(2.0, 10.0),
            // Behind the hero.
            Vec2::new(0.0, -3.0),
            // Past the end and its reach.
            Vec2::new(0.0, 27.0),
        ];
        let (to, hit) = charge_path(Vec2::ZERO, Vec2::Y, 25.0, &enemies);
        assert_eq!(hit, None);
        assert!((to - Vec2::new(0.0, 25.0)).length() < 1e-4);
    }
}
