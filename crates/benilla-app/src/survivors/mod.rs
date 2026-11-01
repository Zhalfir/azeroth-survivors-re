//! Warcraft Survivors: a top-down survival mode on the benilla world, started by
//! [`crate::run_survivors`]. There is no server: the hero, the enemies and the spells are local
//! entities dressed by the same attach, animation and spell-visual systems a streamed unit gets,
//! on a battleground picked at the menu ([`data::MAPS`]), with the player controller and FrameXML
//! off ([`crate::run_mode::CaptureMode`]).

mod abilities;
mod action_button;
mod auto;
mod bossbar;
mod cards;
mod characters;
mod combatlog;
mod data;
mod detail;
mod enemies;
mod feedback;
mod fill;
mod framecap;
mod frametime;
mod globe;
mod grade;
mod guard;
#[cfg(target_os = "android")]
mod graphics;
mod hero;
mod hud;
mod menu;
mod meter;
mod minimap;
mod mobility;
mod potion;
mod records;
mod secrets;
mod settings;
mod speed;
mod talents;
#[cfg(target_os = "android")]
mod touch;
mod units;
mod upscale;

use benilla_world::schedule::WorldStage;
use bevy::prelude::*;

use data::{AbilityDef, ClassDef, PassiveDef, Stat};

/// Set by [`crate::run_survivors`] before the app builds. wasm32 has no process environment, so
/// the flag lives here; `WOW_SURVIVORS` (`?survivors=1` / `__wenilla_env.survivors` on the web)
/// still turns it on from outside.
pub(crate) static ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn active() -> bool {
    ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
        || crate::webenv::var("WOW_SURVIVORS").is_some()
}

pub(crate) struct SurvivorsPlugin;

impl Plugin for SurvivorsPlugin {
    fn build(&self, app: &mut App) {
        // Present before `OnEnter(InWorld)`, where the UI lifecycle decides not to load FrameXML.
        // Trees stay single entities, so the camera can see through the ones over the hero
        // (`hero::see_through`); set before the first tile assembles.
        benilla_world::model_fade::SEE_THROUGH.store(true, std::sync::atomic::Ordering::Relaxed);
        app.insert_resource(crate::run_mode::CaptureMode)
            .add_plugins((
                grade::plugin,
                meter::plugin,
                settings::plugin,
                characters::plugin,
                combatlog::plugin,
                framecap::plugin,
                frametime::plugin,
                records::plugin,
                talents::plugin,
                upscale::plugin,
                feedback::plugin,
                detail::plugin,
                bossbar::plugin,
                secrets::plugin,
                speed::plugin,
            ))
            .add_plugins((globe::plugin, action_button::plugin))
            .init_state::<Phase>()
            .init_resource::<Rng>()
            .init_resource::<Selection>()
            .init_resource::<Run>()
            .init_resource::<hero::CamRig>()
            .init_resource::<hero::AimPoint>()
            .init_resource::<hero::MoveCarry>()
            .init_resource::<abilities::PendingHits>()
            .init_resource::<enemies::Spawner>()
            .init_resource::<hud::Offer>()
            .init_resource::<cards::Stack>()
            .init_resource::<cards::StackTaps>()
            .add_message::<abilities::Hit>()
            .add_message::<enemies::Loot>()
            .add_plugins(bevy::ui_render::UiMaterialPlugin::<hud::SweepUiMaterial>::default())
            .add_plugins(bevy::ui_render::UiMaterialPlugin::<fill::FillMaterial>::default())
            .add_systems(Startup, hud::setup_hud)
            .add_systems(Startup, cards::spawn_stack.after(hud::setup_hud))
            .add_systems(Update, hero::daylight)
            .add_systems(
                Update,
                hero::seat_listener
                    .after(WorldStage::Stream)
                    .before(WorldStage::Present),
            )
            .add_systems(
                Update,
                (
                    hero::drop_stray_buttons,
                    hero::aim_cursor,
                    mobility::cast_mobility,
                    potion::drink_potion,
                    hero::drive_hero,
                    // Before the client's shake adds its offset, which this would overwrite.
                    hero::place_camera.before(crate::camera_shake::apply_camera_shake),
                    hero::cutaway.after(benilla_world::view::stamp_near_clip),
                    hero::see_through,
                )
                    .chain()
                    .in_set(WorldStage::Input),
            )
            .add_systems(
                Update,
                (
                    enemies::spawn_enemies,
                    enemies::move_enemies,
                    abilities::cast_abilities,
                    abilities::tick_areas,
                    abilities::apply_hits,
                    enemies::tick_marks,
                    enemies::drop_loot,
                    enemies::collect_gems,
                    hero::hero_vitals,
                )
                    .chain()
                    .after(WorldStage::Input)
                    .run_if(in_state(Phase::Playing))
                    // Entering a battleground raises its loading screen: the run waits behind it.
                    .run_if(|l: Res<crate::loading_screen::LoadingScreen>| !l.covering()),
            )
            .add_systems(
                Update,
                (enemies::reap_dead, enemies::still_gems).after(WorldStage::Input),
            )
            .add_systems(
                Update,
                (
                    hero::follow_map,
                    hud::menu_preview,
                    hud::buttons,
                    hud::keys,
                    hud::refresh_menu,
                    menu::restore_pick,
                    menu::build_menu,
                    menu::menu_clicks,
                    menu::refresh_menu_ui.before(crate::glue::GlueVisuals),
                    // After `hud::keys`, which leaves Escape to a picked-out row.
                    cards::deal_offer,
                    cards::stack_input,
                    cards::build_stack,
                    cards::animate_stack,
                    hud::refresh_hud,
                    hud::refresh_results,
                    hud::fill_icons,
                    hud::update_cooldown_sweeps,
                    hud::refresh_mobility,
                    hud::show_panels,
                )
                    .chain()
                    .after(WorldStage::Input),
            )
            .add_systems(
                Update,
                (
                    hud::apply_ui_scale,
                    hud::fill_art,
                    hud::place_thumb,
                    hud::icon_tooltip,
                    hud::thumb_vitals,
                    hud::demo_label,
                    minimap::map_buttons,
                    minimap::zone_text,
                    minimap::compose,
                )
                    .chain()
                    .after(hud::show_panels),
            )
            .add_systems(OnEnter(Phase::Playing), (unpause, grace))
            .add_systems(OnEnter(Phase::Paused), pause)
            .add_systems(OnEnter(Phase::GameOver), unpause)
            .add_systems(OnEnter(Phase::Menu), (unpause, reset_run));
        #[cfg(target_os = "android")]
        app.add_plugins((touch::plugin, graphics::plugin));
        // The bot's systems always exist: `WOW_SURVIVORS_AUTO` inserts `Auto` at boot, the menu's
        // Demo button inserts it for one run, and they idle while it is absent.
        if let Some(auto) = auto::from_env() {
            app.insert_resource(auto);
        }
        app.add_systems(
            Update,
            auto::drive
                .after(hud::keys)
                .before(hud::refresh_menu)
                .run_if(resource_exists::<auto::Auto>),
        )
        .add_systems(
            Update,
            auto::plan
                .before(WorldStage::Input)
                .run_if(resource_exists::<auto::Auto>),
        )
        .add_systems(OnEnter(Phase::Menu), auto::end_demo.after(reset_run));
    }
}

/// The screen the mode is on. A level-up is not one: its choices wait in the stack over the
/// action thumb while play goes on (`cards`, `Run::pending_levels`).
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(super) enum Phase {
    #[default]
    Menu,
    Playing,
    Paused,
    GameOver,
}

/// Everything spawned for a run, despawned when the next begins.
#[derive(Component)]
pub(super) struct RunEntity;

/// The class, look and battleground picked at the menu; the battleground holds for the run.
#[derive(Resource, Default)]
pub(super) struct Selection {
    pub class: Option<usize>,
    /// `(display, race, sex)`, one per race and sex the class's trainers come in.
    pub looks: Vec<(u32, u8, u8)>,
    pub look: Option<usize>,
    /// An index into [`data::MAPS`].
    pub map: usize,
    /// The battleground the world is standing in: the last one entered (or the saved pick at
    /// boot). Picking another at the menu changes only `map`, so the menu hero stays put and no
    /// loading screen rises until Enter World.
    pub stage: usize,
    /// Bumped on every change, so the menu rebuilds once.
    pub version: u32,
    /// The difficulty picked at the menu, for the next run.
    pub difficulty: data::Difficulty,
    /// The saved character playing (`characters`); Enter World waits for one.
    pub character: Option<u32>,
}

impl Selection {
    pub fn class_def(&self) -> Option<&'static ClassDef> {
        self.class.map(|i| &data::CLASSES[i])
    }

    pub fn map_def(&self) -> &'static data::MapDef {
        &data::MAPS[self.map.min(data::MAPS.len() - 1)]
    }

    pub fn stage_def(&self) -> &'static data::MapDef {
        &data::MAPS[self.stage.min(data::MAPS.len() - 1)]
    }
}

/// An ability the hero has, with its own cooldown.
pub(super) struct Owned {
    pub def: &'static AbilityDef,
    pub level: u8,
    pub cd: f32,
}

impl Owned {
    pub fn damage_mult(&self) -> f32 {
        1.0 + 0.3 * f32::from(self.level - 1)
    }
    pub fn cooldown(&self, stats: &Stats) -> f32 {
        let base = self.def.cooldown * 0.93f32.powi(i32::from(self.level) - 1);
        (base * (1.0 - stats.haste).max(0.35)).max(0.2)
    }
    pub fn count(&self) -> u32 {
        let extra = [3u8, 5, 7].iter().filter(|&&l| self.level >= l).count() as u32;
        self.def.count + extra
    }
    pub fn radius(&self, stats: &Stats) -> f32 {
        self.def.radius * (1.0 + 0.08 * f32::from(self.level - 1)) * stats.area
    }
    pub fn duration(&self) -> f32 {
        self.def.duration * (1.0 + 0.1 * f32::from(self.level - 1))
    }
}

/// The hero's derived numbers, from the base and the passives.
#[derive(Clone, Copy)]
pub(super) struct Stats {
    pub might: f32,
    pub max_hp: f32,
    pub speed: f32,
    pub regen: f32,
    pub haste: f32,
    pub area: f32,
    pub magnet: f32,
    pub growth: f32,
    pub armor: f32,
    pub crit: f32,
    pub lifesteal: f32,
}

/// The most Lifestealing heals per second of run time, as a share of the maximum health.
pub(super) const LEECH_CAP: f32 = 0.04;

/// The heal one hit's `dealt` buys at `lifesteal`, within what is left of the second's budget.
pub(super) fn leech_amount(dealt: f32, lifesteal: f32, max_hp: f32, used: f32) -> f32 {
    (dealt * lifesteal).min((max_hp * LEECH_CAP - used).max(0.0))
}

pub(super) const BASE_HP: f32 = 120.0;
/// Health each hero level adds to the base pool, before Fortitude multiplies it. 1.5 left a
/// level-57 Hunter at 210-290 when the 6:00 tier (Kobold Miners 25, Murloc Streamrunners 34 a
/// blow) arrived (balance round 5).
pub(super) const HP_PER_LEVEL: f32 = 2.5;
pub(super) const BASE_SPEED: f32 = 7.0;
pub(super) const BASE_MAGNET: f32 = 4.5;

/// One damage-meter line: everything a spell dealt this run.
#[derive(Clone, Copy, Debug)]
pub(super) struct MeterRow {
    pub source: u32,
    pub damage: f64,
    /// When it first hit, so its damage per second counts only the time it was in play.
    pub since: f32,
}

/// One run's state.
#[derive(Resource)]
pub(super) struct Run {
    pub class: &'static ClassDef,
    pub race: u8,
    pub elapsed: f32,
    pub kills: u32,
    pub level: u32,
    pub xp: f32,
    pub hp: f32,
    pub abilities: Vec<Owned>,
    pub passives: Vec<(&'static PassiveDef, u8)>,
    pub shield: f32,
    pub shield_t: f32,
    pub immune_t: f32,
    /// Run time of the last blow that reached the hero, for out-of-combat regeneration.
    pub last_hurt: f32,
    /// Lifestealing's per-second budget: the run second it is for, and what it has healed.
    pub leech_second: f32,
    pub leech_used: f32,
    pub thorns: f32,
    pub thorns_t: f32,
    pub thorns_spell: u32,
    pub heal_rate: f32,
    pub heal_t: f32,
    /// The class mobility's cooldown and running effect.
    mobile: mobility::MobilityState,
    /// The potion's cooldown left, in run seconds (`potion`).
    pub potion_cd: f32,
    /// The last blow the class's guard turned (`guard`) and the run time it did, for the
    /// counterattack skills (read from Task B3 on).
    #[allow(dead_code)]
    pub guarded: Option<(guard::Guarded, f32)>,
    /// Choice sets owed: one per level gained and per boss chest opened (`cards`).
    pub pending_levels: u32,
    /// How many of `pending_levels` are chests', dealt first under a "Chest" header.
    pub pending_chests: u32,
    pub bosses: u32,
    pub rings: u32,
    pub won: bool,
    /// Seconds since the hero fell, while the death plays out.
    pub dead_t: Option<f32>,
    pub next_guid: u64,
    pub damage_done: f64,
    /// What the character's talents and tier set add (`talents`), set as play starts.
    pub bonus: talents::Bonus,
    /// The damage meter: per crediting spell, the damage dealt and the run time of its first hit.
    pub meter: Vec<MeterRow>,
    /// Bumped whenever abilities or passives change, so the HUD rebuilds the icon rows once.
    pub version: u32,
    /// A centre-screen announcement and its seconds left.
    pub banner: Option<(String, f32)>,
    /// The hero's lowest health this run, as a share of the pool (`secrets`: the Furbolg).
    pub min_hp_frac: f32,
    /// Run time the final boss arrived, and fell (`secrets`: the Imp).
    pub final_boss_at: Option<f32>,
    pub won_at: Option<f32>,
    /// The longest the hero stood still (`secrets`: the Centaur).
    pub(in crate::survivors) stillness: secrets::Stillness,
}

impl Default for Run {
    fn default() -> Self {
        Self {
            class: &data::CLASSES[0],
            race: 0,
            elapsed: 0.0,
            kills: 0,
            level: 1,
            xp: 0.0,
            hp: BASE_HP,
            abilities: Vec::new(),
            passives: Vec::new(),
            shield: 0.0,
            shield_t: 0.0,
            immune_t: 0.0,
            last_hurt: 0.0,
            leech_second: -1.0,
            leech_used: 0.0,
            thorns: 0.0,
            thorns_t: 0.0,
            thorns_spell: 0,
            heal_rate: 0.0,
            heal_t: 0.0,
            mobile: mobility::MobilityState::default(),
            potion_cd: 0.0,
            guarded: None,
            pending_levels: 0,
            pending_chests: 0,
            bosses: 0,
            rings: 0,
            won: false,
            dead_t: None,
            next_guid: 1,
            damage_done: 0.0,
            meter: Vec::new(),
            bonus: talents::Bonus::default(),
            version: 0,
            banner: None,
            min_hp_frac: 1.0,
            final_boss_at: None,
            won_at: None,
            stillness: secrets::Stillness::default(),
        }
    }
}

impl Run {
    /// Credit `damage` to `source` (a spell, 0 for unattributed) and to the run's total.
    pub fn record(&mut self, source: u32, damage: f32) {
        let damage = f64::from(damage);
        self.damage_done += damage;
        let now = self.elapsed;
        match self.meter.iter_mut().find(|r| r.source == source) {
            Some(row) => row.damage += damage,
            None => self.meter.push(MeterRow {
                source,
                damage,
                since: now,
            }),
        }
    }

    pub fn stats(&self) -> Stats {
        let mut s = Stats {
            might: 1.0,
            max_hp: BASE_HP + HP_PER_LEVEL * self.level.saturating_sub(1) as f32,
            speed: BASE_SPEED,
            regen: 0.0,
            haste: 0.0,
            area: 1.0,
            magnet: BASE_MAGNET,
            growth: 1.0,
            armor: 0.0,
            crit: 0.05,
            lifesteal: 0.0,
        };
        // Tanky classes start tougher, cloth starts quicker.
        match self.class.id {
            1 | 2 => s.max_hp += 40.0,
            7 | 11 | 3 | 4 => s.max_hp += 15.0,
            _ => s.speed += 0.5,
        }
        for &(p, level) in &self.passives {
            let v = p.per_level * f32::from(level);
            match p.stat {
                Stat::Might => s.might += v,
                Stat::MaxHp => s.max_hp *= 1.0 + v,
                Stat::Speed => s.speed *= 1.0 + v,
                Stat::Regen => s.regen += v,
                Stat::Haste => s.haste += v,
                Stat::Area => s.area += v,
                Stat::Magnet => s.magnet *= 1.0 + v,
                Stat::Growth => s.growth += v,
                Stat::Armor => s.armor += v,
                Stat::Crit => s.crit += v,
                Stat::Lifesteal => s.lifesteal += v,
            }
        }
        // The character's talents and tier set, after the run's own passives.
        let b = self.bonus;
        s.might += b.might;
        s.max_hp *= 1.0 + b.max_hp;
        s.speed *= 1.0 + b.speed;
        s.regen += b.regen;
        s.haste += b.haste;
        s.area += b.area;
        s.magnet *= 1.0 + b.magnet;
        s.growth += b.growth;
        s.armor += b.armor;
        s.crit += b.crit;
        s.lifesteal += b.lifesteal;
        s
    }

    /// Heal a share of `dealt` by the Lifestealing stat, at most [`LEECH_CAP`] of the pool per
    /// second of run time, so a big crowd can't make the hero immortal.
    pub fn leech(&mut self, dealt: f32) {
        let s = self.stats();
        if s.lifesteal <= 0.0 || dealt <= 0.0 || self.dead_t.is_some() {
            return;
        }
        let second = self.elapsed.floor();
        if second != self.leech_second {
            self.leech_second = second;
            self.leech_used = 0.0;
        }
        let heal = leech_amount(dealt, s.lifesteal, s.max_hp, self.leech_used);
        self.leech_used += heal;
        self.hp = (self.hp + heal).min(s.max_hp);
    }

    pub fn guid(&mut self) -> u64 {
        self.next_guid += 1;
        self.next_guid
    }
}

/// xorshift64*: the mode's dice, seeded from the clock.
#[derive(Resource)]
pub(super) struct Rng(u64);

impl Default for Rng {
    fn default() -> Self {
        let seed = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self(seed | 1)
    }
}

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in `[0, 1)`.
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
    pub fn index(&mut self, len: usize) -> usize {
        (self.next_u64() % len.max(1) as u64) as usize
    }
}

fn pause(mut time: ResMut<Time<Virtual>>) {
    time.pause();
}

fn unpause(mut time: ResMut<Time<Virtual>>) {
    time.unpause();
}

/// Back in play after the pause menu or at the start, the hero is immune for a moment (the
/// Divine Shield bubble shows it): on a tablet the thumb has to travel from the button back to
/// the joystick.
const GRACE_SECS: f32 = 1.0;

fn grace(mut run: ResMut<Run>) {
    run.immune_t = run.immune_t.max(GRACE_SECS);
}

/// Back at the menu: everything the last run spawned goes, and the run starts over.
fn reset_run(
    mut commands: Commands,
    spawned: Query<Entity, With<RunEntity>>,
    mut run: ResMut<Run>,
    mut pending: ResMut<abilities::PendingHits>,
    mut spawner: ResMut<enemies::Spawner>,
    mut selection: ResMut<Selection>,
    mut offer: ResMut<hud::Offer>,
    mut stack: ResMut<cards::Stack>,
) {
    // No set left over for the next run: its indices would point into this run's build.
    offer.clear();
    stack.selected = None;
    for e in &spawned {
        commands.entity(e).despawn();
    }
    // Guids stay unique across runs: caches keyed by guid may outlive a despawn by a frame.
    let next_guid = run.next_guid;
    *run = Run {
        next_guid,
        ..Run::default()
    };
    pending.0.clear();
    *spawner = enemies::Spawner::default();
    // The preview hero is respawned for the look still picked.
    selection.version += 1;
}

#[cfg(test)]
mod leech_tests {
    use super::leech_amount;

    #[test]
    fn lifestealing_heals_a_share_within_the_second_budget() {
        // 15 % of a 100-damage hit is 15; a 400 pool allows 16 a second.
        assert_eq!(leech_amount(100.0, 0.15, 400.0, 0.0), 15.0);
        // 10 of the second's 16 already used: only 6 left.
        assert_eq!(leech_amount(100.0, 0.15, 400.0, 10.0), 6.0);
        // Budget spent: nothing, never negative.
        assert_eq!(leech_amount(100.0, 0.15, 400.0, 20.0), 0.0);
    }
}
