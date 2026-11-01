//! The horde: spawning on a ring outside the view, the chase, contact damage, death, and the
//! experience gems the dead leave.

use std::collections::HashMap;

use benilla_world::rig_anim::GlobalSeqDrive;
use benilla_world::world_point::WorldPoint;
use bevy::prelude::*;

use super::abilities::Hit;
use super::data::{self, EnemyDef};
use super::guard;
use super::hero::{self, Hero, HERO_RADIUS};
use super::units::{self, flat, yaw_toward, Fx, Marks, UnitSpec, COLOR_HURT};
use super::{Rng, Run, Selection};
use crate::creature_anim::{move_flags, MovementState};
use crate::net::{FieldChanged, Guid, ObjectStore};

/// "Block" and "Dodge" over the hero: white, as the client floats its own misses.
const COLOR_GUARD: u32 = 0xFFFF_FFFF;

#[derive(Component)]
pub(super) struct Enemy {
    pub def: &'static EnemyDef,
    pub hp: f32,
    pub max_hp: f32,
    pub speed: f32,
    pub damage: f32,
    pub radius: f32,
    pub xp: u32,
    pub boss: bool,
    /// The map's last boss: its death wins the run.
    pub final_boss: bool,
    pub attack_cd: f32,
    pub slow: f32,
    pub slow_t: f32,
    pub root_t: f32,
    pub stun_t: f32,
    pub fear_t: f32,
    pub weaken: f32,
    pub weaken_t: f32,
    pub dots: Vec<Dot>,
    pub dead: bool,
}

pub(super) struct Dot {
    pub dps: f32,
    pub left: f32,
    pub tick: f32,
    /// The spell the damage meter credits.
    pub source: u32,
}

/// A corpse playing its death before it fades.
#[derive(Component)]
pub(super) struct Dying(f32);

#[derive(Resource, Default)]
pub(super) struct Spawner {
    timer: f32,
}

/// An enemy died here, worth this much.
#[derive(Message, Clone, Copy)]
pub(super) struct Loot {
    pub pos: Vec3,
    pub xp: u32,
    pub boss: bool,
}

#[derive(Clone, Copy)]
pub(super) enum GemKind {
    Xp(u32),
    /// Health, as a share of the hero's maximum.
    Heal(f32),
    /// A boss's chest: a free card pick, as Vampire Survivors' chests.
    Chest,
}

#[derive(Component)]
pub(super) struct Gem {
    kind: GemKind,
    pulled: bool,
    speed: f32,
}

/// The `GameObjectDisplayInfo` row and scale a pickup wears: a wisp per experience tier, the
/// bigger tiers rarer. Health is a red globe ([`super::globe`]); the turkey leg is only its
/// fallback while the globe's meshes are not up.
fn look_for(kind: GemKind) -> (u32, f32) {
    match kind {
        // `TurkeyLeg.mdx`.
        GemKind::Heal(_) => (114, 2.5),
        // `TreasureChest01.mdx`, the open world's treasure chest.
        GemKind::Chest => (259, 1.3),
        // `BFD_WispSmallPurple.mdx`.
        GemKind::Xp(xp) if xp >= 20 => (1307, 1.0),
        // `BFD_WispSmallGreen.mdx`.
        GemKind::Xp(xp) if xp >= 5 => (1267, 1.0),
        // `BFD_WispSmall.mdx`.
        GemKind::Xp(_) => (426, 1.0),
    }
}

/// Body radius: the big beasts need more room.
fn radius_for(def: &EnemyDef) -> f32 {
    (if def.big { 1.3 } else { 0.6 }) * def.scale
}

/// A ring's size at most, the room it may take above the alive cap, and the smallest worth
/// closing.
const RING_MAX: usize = 70;
const RING_MARGIN: usize = 40;
const RING_MIN: usize = 10;
const RING_HEALTH: f32 = 0.5;
/// The horde's cap is divided by this while the final boss stands: ~36 of 220. A third left a
/// Normal Warrior's final fight to ~73 adds that did the killing while the boss stood at 92 %
/// (the user's run, tablet, 2026-10-31).
const FINAL_CROWD_DIVISOR: usize = 6;
/// The final boss's pace: near the hero's own, so it closes in. Slower, it could be outrun for
/// the whole fight (round 2: 40 s at 100%, 23 to 36 yd away).
const FINAL_BOSS_SPEED: f32 = 6.3;

fn spawn_enemy(
    commands: &mut Commands,
    run: &mut Run,
    def: &'static EnemyDef,
    pos: Vec3,
    hero: Vec3,
    boss: bool,
    final_boss: bool,
    health: f32,
) {
    super::combatlog::spawned(def.name);
    let scale = health
        * data::hp_scale(run.elapsed)
        * if final_boss {
            data::FINAL_BOSS_HEALTH
        } else {
            1.0
        };
    let serial = run.guid();
    let e = units::spawn_unit(
        commands,
        &UnitSpec {
            serial,
            display: def.display,
            scale: def.scale,
            level: def.level,
            faction: units::FACTION_MONSTER,
            bytes0: 1 << 8,
            weapons: &[],
            sheath: 0,
        },
        pos,
        yaw_toward(hero - pos),
    );
    let hp = def.hp * scale;
    commands.entity(e).insert((
        Enemy {
            def,
            hp,
            max_hp: hp,
            speed: if final_boss {
                def.speed.max(FINAL_BOSS_SPEED)
            } else {
                def.speed
            },
            damage: if final_boss {
                def.damage * data::FINAL_BOSS_DAMAGE * data::difficulty().damage()
            } else {
                def.damage * data::damage_scale(run.elapsed)
            },
            radius: radius_for(def),
            xp: def.xp,
            boss,
            final_boss,
            attack_cd: 0.5,
            slow: 1.0,
            slow_t: 0.0,
            root_t: 0.0,
            stun_t: 0.0,
            fear_t: 0.0,
            weaken: 1.0,
            weaken_t: 0.0,
            dots: Vec::new(),
            dead: false,
        },
        Marks::debuffs(),
    ));
}

fn ground(world: &WorldPoint, mut p: Vec3, fallback: f32) -> Vec3 {
    p.y = world.terrain_height_under(p).unwrap_or(fallback);
    p
}

/// Keep the horde topped up on a ring outside the view, with the bosses and the encircling waves
/// on the clock.
pub(super) fn spawn_enemies(
    mut commands: Commands,
    time: Res<Time>,
    mut run: ResMut<Run>,
    mut rng: ResMut<Rng>,
    mut spawner: ResMut<Spawner>,
    selection: Res<Selection>,
    hero: Query<&Transform, With<Hero>>,
    enemies: Query<&Enemy>,
    world: WorldPoint,
) {
    let Ok(h) = hero.single() else { return };
    if run.dead_t.is_some() {
        return;
    }
    let hpos = h.translation;
    let elapsed = run.elapsed;
    let roster = data::roster(selection.map);
    let pool = data::wave_pool(elapsed, roster.enemies.len());

    // The roster's bosses but the last on the clock; the last is the final boss, at the bell.
    let regulars = roster.bosses.len().saturating_sub(1) as u32;
    let scheduled = (elapsed - data::boss_offset()).max(0.0);
    let due_bosses = ((scheduled / data::boss_every()) as u32).min(regulars);
    let final_due = elapsed >= data::run_secs() && !roster.bosses.is_empty();
    while run.bosses < due_bosses || (final_due && run.bosses == regulars) {
        let is_final = run.bosses == regulars;
        let def = &roster.bosses[run.bosses as usize];
        let a = rng.range(0.0, std::f32::consts::TAU);
        let pos = ground(
            &world,
            hpos + Vec3::new(a.cos(), 0.0, a.sin()) * 30.0,
            hpos.y,
        );
        spawn_enemy(&mut commands, &mut run, def, pos, hpos, true, is_final, 1.0);
        if is_final {
            run.final_boss_at = Some(elapsed);
        }
        super::combatlog::event(
            elapsed,
            format!(
                "{} {} arrives",
                if is_final { "FINAL BOSS" } else { "boss" },
                def.name
            ),
        );
        run.bosses += 1;
        run.banner = Some(if is_final {
            (format!("{} has come. Defeat it to win!", def.name), 6.0)
        } else {
            (format!("{} approaches!", def.name), 4.0)
        });
    }

    // Half way between bosses, a closing ring of the moment's weakest.
    let due_rings = ((scheduled + data::boss_every() / 2.0) / data::boss_every()) as u32;
    // No ring once the final boss stands: the climax is the boss. A ring adds to the horde at
    // most up to the cap plus a margin; at 220 alive a ring of 93 after the bell killed a full
    // 510-health Priest in one second (balance round 2, 2026-10-31).
    let alive_now = enemies.iter().filter(|e| !e.dead).count();
    let room = (data::target_alive(elapsed) + RING_MARGIN).saturating_sub(alive_now);
    if run.rings < due_rings && (run.bosses > regulars || room < RING_MIN) {
        run.rings = due_rings;
    }
    if run.rings < due_rings {
        run.rings = due_rings;
        let def = &roster.enemies[pool[0]];
        // Small early, the full ring late: at 28 + 60 the first ring, at 45 s on Normal, closed
        // thirty-odd foes on a level-8 hero and was the commonest early death.
        let n = (12 + (data::progress(elapsed) * 76.0) as usize)
            .min(RING_MAX)
            .min(room);
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let pos = ground(
                &world,
                hpos + Vec3::new(a.cos(), 0.0, a.sin()) * 26.0,
                hpos.y,
            );
            // A ring is fodder: at half health any build can break a hole through it, so the
            // danger is being caught, not an unbreakable wall (round 3: Shaman, 2:22).
            spawn_enemy(
                &mut commands,
                &mut run,
                def,
                pos,
                hpos,
                false,
                false,
                RING_HEALTH,
            );
        }
        run.banner = Some(("They're surrounding you!".into(), 3.0));
    }

    spawner.timer -= time.delta_secs();
    if spawner.timer > 0.0 {
        return;
    }
    spawner.timer = 0.12;
    let alive = enemies.iter().filter(|e| !e.dead).count();
    // Once the final boss stands, it takes the stage: the horde is refilled only to a third, so
    // the crowd thins as it falls. Topped up to 220, it walled the boss off: a melee Rogue took
    // it 21% in six minutes (balance round 5).
    let target = if run.bosses > regulars {
        data::target_alive(elapsed) / FINAL_CROWD_DIVISOR
    } else {
        data::target_alive(elapsed)
    };
    if alive >= target {
        return;
    }
    let n = (((target - alive) as f32) / 6.0).ceil().min(4.0) as usize;
    for _ in 0..n {
        let def = &roster.enemies[pool[rng.index(pool.len())]];
        let a = rng.range(0.0, std::f32::consts::TAU);
        let r = rng.range(34.0, 42.0);
        let pos = ground(&world, hpos + Vec3::new(a.cos(), 0.0, a.sin()) * r, hpos.y);
        spawn_enemy(&mut commands, &mut run, def, pos, hpos, false, false, 1.0);
    }
}

/// The separation grid's cell edge, in yards.
const CELL: f32 = 3.0;

/// The grid cell a position falls in.
fn cell_of(p: Vec3) -> (i32, i32) {
    ((p.x / CELL).floor() as i32, (p.z / CELL).floor() as i32)
}

/// [`move_enemies`]' per-frame buffers, kept between frames so the chase allocates nothing once
/// warm (the `performance-optimization` reuse-buffers fix; 2026-10-31).
#[derive(Default)]
pub(super) struct Separation {
    bodies: Vec<(Entity, Vec3, f32)>,
    grid: HashMap<(i32, i32), Vec<usize>>,
    push: HashMap<Entity, Vec3>,
}

/// Rebuild `grid` from `positions` (indexed in order), reusing its cells' storage: cells are
/// emptied, not dropped, and cells left empty are removed so stale ones are never visited.
fn fill_grid(grid: &mut HashMap<(i32, i32), Vec<usize>>, positions: impl Iterator<Item = Vec3>) {
    for cell in grid.values_mut() {
        cell.clear();
    }
    for (i, p) in positions.enumerate() {
        grid.entry(cell_of(p)).or_default().push(i);
    }
    grid.retain(|_, cell| !cell.is_empty());
}

/// The chase: straight at the hero, pushed apart from each other, stopped by crowd control, and
/// swinging when in reach.
#[allow(clippy::type_complexity)]
pub(super) fn move_enemies(
    time: Res<Time>,
    mut run: ResMut<Run>,
    world: WorldPoint,
    hero: Query<(Entity, &Transform), (With<Hero>, Without<Enemy>)>,
    mut enemies: Query<(Entity, &mut Transform, &mut Enemy, &mut MovementState), Without<Hero>>,
    mut fx: Fx,
    mut hits: MessageWriter<Hit>,
    mut scratch: Local<Separation>,
    mut rng: ResMut<Rng>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let Ok((hero_e, htf)) = hero.single() else {
        return;
    };
    let hpos = htf.translation;
    let hero_down = run.dead_t.is_some();

    // Separation over a coarse grid: only neighbours in the same or adjacent cells push. The
    // buffers live across frames, so a crowd of 220 allocates nothing per frame once warm.
    let Separation { bodies, grid, push } = &mut *scratch;
    bodies.clear();
    bodies.extend(
        enemies
            .iter()
            .filter(|(_, _, e, _)| !e.dead)
            .map(|(ent, tf, e, _)| (ent, tf.translation, e.radius)),
    );
    fill_grid(grid, bodies.iter().map(|b| b.1));
    push.clear();
    let key = cell_of;
    for (i, &(ent, pos, r)) in bodies.iter().enumerate() {
        let (cx, cz) = key(pos);
        let mut acc = Vec3::ZERO;
        for dx in -1..=1 {
            for dz in -1..=1 {
                let Some(cell) = grid.get(&(cx + dx, cz + dz)) else {
                    continue;
                };
                for &j in cell {
                    if j == i {
                        continue;
                    }
                    let (_, other, or) = bodies[j];
                    let d = flat(pos - other);
                    let len = d.length();
                    let min = r + or;
                    if len < min && len > 1e-4 {
                        acc += d / len * (min - len);
                    } else if len <= 1e-4 {
                        acc += Vec3::new((i as f32).sin(), 0.0, (i as f32).cos()) * 0.1;
                    }
                }
            }
        }
        push.insert(ent, acc);
    }

    for (ent, mut tf, mut e, mut mv) in &mut enemies {
        let e = &mut *e;
        if e.dead {
            if mv.speed != 0.0 || mv.flags != 0 {
                mv.speed = 0.0;
                mv.flags = 0;
            }
            continue;
        }
        e.attack_cd -= dt;
        for t in [
            &mut e.slow_t,
            &mut e.root_t,
            &mut e.stun_t,
            &mut e.fear_t,
            &mut e.weaken_t,
        ] {
            *t = (*t - dt).max(0.0);
        }
        if e.slow_t <= 0.0 {
            e.slow = 1.0;
        }
        if e.weaken_t <= 0.0 {
            e.weaken = 1.0;
        }
        let pos = tf.translation;
        let to_hero = flat(hpos - pos);
        let dist = to_hero.length();

        // Strays far behind are brought back in front, as the reference genre does.
        if dist > 75.0 {
            let back = hpos + to_hero.normalize_or_zero() * 38.0;
            tf.translation = ground(&world, back, hpos.y);
            continue;
        }

        // Under Fade the horde has lost the hero: it mills where it stands.
        let lost = run.mobile.vanish_t > 0.0 && !e.boss;
        let held = e.stun_t > 0.0 || e.root_t > 0.0 || lost;
        let contact = dist <= HERO_RADIUS + e.radius + 0.35;
        let mut step = Vec3::ZERO;
        let mut speed = 0.0;
        if !held && !hero_down {
            let dir = if e.fear_t > 0.0 {
                -to_hero.normalize_or_zero()
            } else {
                to_hero.normalize_or_zero()
            };
            if !contact || e.fear_t > 0.0 {
                speed = e.speed * e.slow;
                step = dir * speed;
            }
            if dir != Vec3::ZERO {
                let target = Quat::from_rotation_y(yaw_toward(dir));
                let turned = tf.rotation.slerp(target, (dt * 10.0).min(1.0));
                // Only a real turn is written: a write marks `Transform` changed, and every
                // enemy's transform, rig and shadow work downstream follows the flag.
                if turned.angle_between(tf.rotation) > 1e-4 {
                    tf.rotation = turned;
                }
            }
        }
        let shove = push.get(&ent).copied().unwrap_or_default();
        let delta = step * dt + shove * (dt * 8.0).min(0.5);
        // A standing enemy (in contact, held, or idle) keeps its transform untouched and skips the
        // terrain lookup (B3c hotspot survey, 2026-10-31).
        if delta.length_squared() > 1e-8 {
            tf.translation = ground(&world, pos + delta, pos.y);
        }
        if speed > 0.0 {
            mv.speed = speed;
            mv.flags = move_flags::FORWARD;
        } else if mv.speed != 0.0 || mv.flags != 0 {
            mv.speed = 0.0;
            mv.flags = 0;
        }

        if contact
            && !lost
            && e.stun_t <= 0.0
            && e.fear_t <= 0.0
            && e.attack_cd <= 0.0
            && !hero_down
        {
            e.attack_cd = if e.final_boss {
                data::FINAL_BOSS_SWING
            } else if e.boss {
                1.0
            } else {
                1.3
            };
            let raw = e.damage * e.weaken;
            let hp_before = run.hp;
            // A swing is a direct blow: the class's guard may turn it. Not while the hero is
            // immune anyway, so no "Block" floats over a blow that would not land.
            let open = run.immune_t <= 0.0 && !hero::god_mode();
            let swung = raw;
            let (raw, guarded) = if open {
                guard::mitigate(run.class.guard, guard::Hit::Direct, raw, rng.f32())
            } else {
                (raw, None)
            };
            if let Some(g) = guarded {
                super::combatlog::guarded(g, swung - raw);
            }
            let taken = match guarded {
                Some(guard::Guarded::Dodge) => 0.0,
                _ => hero::hurt_hero(&mut run, raw),
            };
            super::combatlog::hurt(e.def.name, taken, hp_before, e.boss, run.elapsed);
            match guarded {
                Some(g) => {
                    run.guarded = Some((g, run.elapsed));
                    fx.swing_as(ent, hero_e, taken, g.victim_state());
                    fx.word(hero_e, g.word(), COLOR_GUARD);
                }
                None => fx.swing(ent, hero_e, taken),
            }
            if taken > 0.0 {
                fx.number(hero_e, taken, false, COLOR_HURT);
            }
            if run.thorns_t > 0.0 {
                hits.write(Hit {
                    source: run.thorns_spell,
                    ..Hit::direct(ent, run.thorns * run.stats().might, false)
                });
            }
        }
    }
}

/// Mark timers run down; the state kits reap with them.
pub(super) fn tick_marks(
    time: Res<Time>,
    mut units: Query<(Entity, &Guid, &mut ObjectStore, &mut Marks), With<Enemy>>,
    mut fields: MessageWriter<FieldChanged>,
) {
    let dt = time.delta_secs();
    for (entity, guid, mut store, mut marks) in &mut units {
        marks.tick(dt);
        // Borrowing the store mutably marks it changed even when nothing is written, which kept
        // every enemy's `ObjectStore` changed every frame and the unit feed's dirty gate
        // (`ui_unit::feed_units`) open (B3c hotspot survey, 2026-10-31).
        if marks.is_dirty() {
            marks.flush(entity, guid.0, &mut store, &mut fields);
        }
    }
}

/// A corpse's hold before it fades: its death's 2.5 s while few lie about, shorter as they pile
/// up, so a crowd's bodies do not hide the live enemies around the hero.
const CORPSE_HOLD: f32 = 2.5;
const CORPSE_HOLD_MIN: f32 = 0.6;
/// The corpses at once the full hold is kept for.
const CORPSES_CALM: usize = 12;

fn corpse_hold(corpses: usize) -> f32 {
    if corpses <= CORPSES_CALM {
        CORPSE_HOLD
    } else {
        (CORPSE_HOLD * CORPSES_CALM as f32 / corpses as f32).max(CORPSE_HOLD_MIN)
    }
}

/// Corpses fade once their death has played, sooner in a crowd ([`corpse_hold`]).
pub(super) fn reap_dead(
    mut commands: Commands,
    time: Res<Time>,
    mut dying: Query<(Entity, &mut Dying)>,
) {
    let hold = corpse_hold(dying.iter().len());
    for (e, mut d) in &mut dying {
        d.0 += time.delta_secs();
        if d.0 > hold {
            commands
                .entity(e)
                .remove::<Dying>()
                .insert(benilla_world::model_fade::DespawnFade::default());
        }
    }
}

pub(super) fn mark_dying(commands: &mut Commands, e: Entity) {
    commands.entity(e).insert(Dying(0.0));
}

/// Most gems on the ground at once; past it, a kill's worth joins an existing gem.
const MAX_GEMS: usize = 350;

pub(super) fn drop_loot(
    mut commands: Commands,
    mut drops: MessageReader<Loot>,
    mut rng: ResMut<Rng>,
    mut run: ResMut<Run>,
    mut gems: Query<&mut Gem>,
    globes: Option<Res<super::globe::GlobeAssets>>,
) {
    let globes = globes.as_deref();
    let mut count = gems.iter().count();
    for d in drops.read() {
        // A boss leaves a full heal and a chest; the horde, now and then, a quarter of the
        // health pool, rarer on the harder difficulties. Normal was 1.5 %; a reasonable Normal
        // Warrior run still dropped to 15/262 at 5:00 (the user's run, 2026-10-31).
        let potion_chance = match data::difficulty() {
            data::Difficulty::Normal => 0.025,
            data::Difficulty::Heroic => 0.011,
            data::Difficulty::Mythic => 0.007,
        };
        if d.boss || rng.f32() < potion_chance {
            let heal = GemKind::Heal(if d.boss { 1.0 } else { 0.25 });
            spawn_gem(&mut commands, globes, run.guid(), d.pos + Vec3::X * 0.8, heal);
        }
        if d.boss {
            spawn_gem(
                &mut commands,
                globes,
                run.guid(),
                d.pos - Vec3::X * 0.8,
                GemKind::Chest,
            );
        }
        if count >= MAX_GEMS && !d.boss {
            if let Some(mut g) = gems.iter_mut().find(|g| matches!(g.kind, GemKind::Xp(_))) {
                if let GemKind::Xp(xp) = &mut g.kind {
                    *xp += d.xp;
                }
                continue;
            }
        }
        spawn_gem(&mut commands, globes, run.guid(), d.pos, GemKind::Xp(d.xp));
        count += 1;
    }
}

fn spawn_gem(
    commands: &mut Commands,
    globes: Option<&super::globe::GlobeAssets>,
    serial: u64,
    pos: Vec3,
    kind: GemKind,
) {
    let at = pos + Vec3::Y * 0.5;
    let e = match (kind, globes) {
        // A boss's full heal is a bigger globe.
        (GemKind::Heal(share), Some(g)) => {
            super::globe::spawn_globe(commands, g, at, if share >= 1.0 { 1.4 } else { 1.0 })
        }
        _ => {
            let (display, scale) = look_for(kind);
            units::spawn_object(commands, serial, display, scale, at)
        }
    };
    commands.entity(e).insert(Gem {
        kind,
        pulled: false,
        speed: 6.0,
    });
}

/// Gems in the magnet's reach fly to the hero; levels bank as they land.
pub(super) fn collect_gems(
    mut commands: Commands,
    time: Res<Time>,
    mut run: ResMut<Run>,
    hero: Query<(Entity, &Transform), (With<Hero>, Without<Gem>)>,
    mut gems: Query<(Entity, &mut Transform, &mut Gem)>,
    mut secrets: MessageWriter<super::secrets::SecretEvent>,
    mut fx: Fx,
) {
    let Ok((hero_e, h)) = hero.single() else { return };
    if run.dead_t.is_some() {
        return;
    }
    let dt = time.delta_secs();
    let stats = run.stats();
    let target = h.translation + Vec3::Y * 1.0;
    for (e, mut tf, mut g) in &mut gems {
        let d = target - tf.translation;
        let dist = d.length();
        if !g.pulled && flat(d).length() <= stats.magnet {
            g.pulled = true;
        }
        if !g.pulled {
            continue;
        }
        if dist < 0.9 {
            match g.kind {
                GemKind::Xp(xp) => {
                    let gained = xp as f32 * data::XP_RATE * stats.growth;
                    run.xp += gained;
                    super::combatlog::xp(gained);
                }
                GemKind::Heal(share) => {
                    let before = run.hp;
                    run.hp = (run.hp + share * stats.max_hp).min(stats.max_hp);
                    let healed = run.hp - before;
                    if healed >= 0.5 {
                        fx.heal_text(hero_e, healed);
                    }
                }
                GemKind::Chest => {
                    run.pending_levels += 1;
                    run.pending_chests += 1;
                    run.banner = Some(("A treasure chest! Choose a power.".into(), 3.0));
                    super::combatlog::event(run.elapsed, "chest opened");
                    secrets.write(super::secrets::SecretEvent::Chest);
                }
            }
            commands.entity(e).despawn();
            continue;
        }
        g.speed += 30.0 * dt;
        tf.translation += d / dist * (g.speed * dt).min(dist);
    }
    while run.xp >= data::xp_to_next(run.level) {
        run.xp -= data::xp_to_next(run.level);
        run.level += 1;
        run.pending_levels += 1;
        if run.level == super::mobility::LEARN_LEVEL {
            if let Some(m) = super::mobility::for_class(run.class.id) {
                run.banner = Some((
                    format!("You have learned {}! {} F.", m.name, super::hud::PRESS),
                    4.0,
                ));
            }
        }
    }
    // Nothing left to pick: each level heals a fifth of the pool, and play goes on.
    super::hud::bank_maxed(&mut run);
    // Owed sets wait in the stack over the action thumb (`cards`); play goes on.
}

/// Pickups hang still: a wisp's own loop (its circling motes and their trails) stops on its first
/// frame, clip and global sequences alike, as soon as the model attaches.
pub(super) fn still_gems(
    mut players: Query<&mut AnimationPlayer, (With<Gem>, Added<AnimationPlayer>)>,
    mut drives: Query<&mut GlobalSeqDrive, (With<Gem>, Added<GlobalSeqDrive>)>,
) {
    for mut player in &mut players {
        for (_, active) in player.playing_animations_mut() {
            active.set_speed(0.0);
        }
    }
    for mut drive in &mut drives {
        drive.set_paused(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A few corpses keep their whole death; a pile fades sooner, never instantly.
    #[test]
    fn corpses_fade_sooner_in_a_crowd() {
        assert_eq!(corpse_hold(0), CORPSE_HOLD);
        assert_eq!(corpse_hold(CORPSES_CALM), CORPSE_HOLD);
        assert!(corpse_hold(24) < CORPSE_HOLD);
        assert!(corpse_hold(24) > corpse_hold(48));
        assert_eq!(corpse_hold(500), CORPSE_HOLD_MIN);
    }

    #[test]
    fn fill_grid_reuses() {
        let mut grid = HashMap::new();
        let first = [Vec3::new(0.5, 0.0, 0.5), Vec3::new(1.0, 0.0, 1.0), Vec3::new(7.0, 0.0, 0.0)];
        fill_grid(&mut grid, first.into_iter());
        let capacity = grid.capacity();
        let second = [Vec3::new(10.0, 0.0, 10.0), Vec3::new(0.2, 0.0, 0.2)];
        fill_grid(&mut grid, second.into_iter());

        let mut fresh = HashMap::new();
        fill_grid(&mut fresh, second.into_iter());
        assert_eq!(grid, fresh, "the same cells as a fresh build, old entries gone");
        assert!(grid.capacity() >= capacity, "the map is not shrunk between frames");
        assert_eq!(grid[&cell_of(second[1])], vec![1]);
    }
}
