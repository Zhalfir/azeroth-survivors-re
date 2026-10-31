//! Secret races (`docs/superpowers/specs/2026-10-31-secret-races-design.md`): account-wide
//! progress, the hidden unlock rules, and the counting during runs.
//!
//! **Progress** ([`Progress`]) lives in `benilla-config/secrets.toml`, apart from the characters,
//! so an unlock survives deleting every character. It counts deaths per class, kills per creature
//! family, chests, and keeps every won run ([`WinRecord`]); the zones cleared come from those wins
//! and from every saved character's completions ([`Clears`]).
//!
//! **Counting.** The run's sites write one [`SecretEvent`] each: a kill (`abilities::apply_hits`,
//! once per enemy as it dies), a chest (`enemies::collect_gems`), the run's end ([`run_end`], on
//! entering the results). [`count`] alone touches [`Progress`]; it ignores everything while the
//! auto-player or Demo drives (`auto::Auto`) or under `WOW_SURVIVORS_GOD`. It saves at the run's
//! end and on an unlock, never per kill (and on the way back to the menu, for an abandoned run).
//! A kill or chest that crosses a threshold unlocks mid-run.
//!
//! Each unlock writes one [`Revealed`], for the banner ([`reveal`]). The Secrets page is [`page`].
//!
//! **Playing one.** A look is `(display, race, sex)`; a secret race's looks carry a look code,
//! [`look_code`] (100 + its index), in the race slot, one look per display variant. Only the
//! spawn turns the code into the unit's race (9 for the Goblin, 0 for a creature) and scale. A
//! saved character names its secret by key (`Character::secret`), never by code, so a reordered
//! table still loads it; an unknown key falls back to the class's first look.
//!
//! The run also tracks, for the Imp and the Centaur, when the final boss arrived and fell
//! (`Run::final_boss_at`, `Run::won_at`) and the longest the hero stood still ([`Stillness`],
//! in run time, so the pauses and card screens never count).

pub(super) mod data;
pub(super) mod page;
pub(super) mod reveal;

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::characters::Characters;
use super::data::Difficulty;
use super::{Phase, Run, Selection};
pub(super) use data::{def, SecretRace};

/// One won run, kept for the conditions that ask how it was won.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(super) struct WinRecord {
    pub class_id: u8,
    /// [`super::data::MapDef::name`].
    pub zone: String,
    /// [`Difficulty::name`].
    pub difficulty: String,
    /// Run time at the win.
    pub secs: f32,
    /// The hero's lowest health over the run, as a share of the pool.
    pub min_hp_frac: f32,
    /// Seconds from the final boss's arrival to its fall; `None` in a record from before.
    #[serde(default)]
    pub boss_secs: Option<f32>,
    /// The longest the hero stood still, in run seconds; a record from before counts as never
    /// moving.
    #[serde(default = "never_moved")]
    pub longest_still: f32,
}

fn never_moved() -> f32 {
    f32::MAX
}

/// The account's secret progress. Every field defaults, so an older or partial file loads.
#[derive(Resource, Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub(super) struct Progress {
    /// [`data::SecretDef::key`]s, in the order found.
    pub unlocked: Vec<String>,
    pub chests: u32,
    /// Defeats per `ChrClasses` id.
    #[serde(with = "class_keys")]
    pub deaths_by_class: HashMap<u8, u32>,
    /// Kills per creature family ([`family`]): "murloc", "gnoll".
    pub kills_by_family: HashMap<String, u32>,
    pub wins: Vec<WinRecord>,
    /// Unlocks the Secrets page has shown (`page`): the rest wear a NEW ribbon.
    pub seen: Vec<String>,
    /// Counted since the last save.
    #[serde(skip)]
    dirty: bool,
    #[serde(skip)]
    loaded: bool,
}

impl Progress {
    pub fn is_unlocked(&self, r: SecretRace) -> bool {
        self.unlocked.iter().any(|k| k == def(r).key)
    }

    fn deaths(&self, class_id: u8) -> u32 {
        self.deaths_by_class.get(&class_id).copied().unwrap_or(0)
    }

    fn kills(&self, family: &str) -> u32 {
        self.kills_by_family.get(family).copied().unwrap_or(0)
    }
}

/// TOML keys are strings: the class ids are written as `"2" = 100`.
mod class_keys {
    use std::collections::HashMap;

    use serde::ser::SerializeMap;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(m: &HashMap<u8, u32>, s: S) -> Result<S::Ok, S::Error> {
        let mut rows: Vec<(&u8, &u32)> = m.iter().collect();
        rows.sort();
        let mut map = s.serialize_map(Some(rows.len()))?;
        for (k, v) in rows {
            map.serialize_entry(&k.to_string(), v)?;
        }
        map.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<HashMap<u8, u32>, D::Error> {
        let raw = HashMap::<String, u32>::deserialize(d)?;
        Ok(raw
            .into_iter()
            .filter_map(|(k, v)| Some((k.parse().ok()?, v)))
            .collect())
    }
}

/// The zones cleared on the account, `(zone, difficulty)` by name: every saved character's
/// completions and every recorded win.
#[derive(Default, Debug)]
pub(super) struct Clears {
    done: HashSet<(String, String)>,
}

impl Clears {
    pub fn from_parts<'a>(
        completions: impl IntoIterator<Item = &'a (String, String)>,
        wins: &[WinRecord],
    ) -> Self {
        let mut done: HashSet<(String, String)> = completions.into_iter().cloned().collect();
        done.extend(wins.iter().map(|w| (w.zone.clone(), w.difficulty.clone())));
        Self { done }
    }

    #[cfg(test)]
    pub fn insert(&mut self, zone: &str, d: Difficulty) {
        self.done.insert((zone.to_string(), d.name().to_string()));
    }

    fn has(&self, zone: &str, d: Difficulty) -> bool {
        self.done
            .iter()
            .any(|(z, n)| z == zone && n == d.name())
    }

    fn has_any(&self, zone: &str) -> bool {
        Difficulty::ALL.into_iter().any(|d| self.has(zone, d))
    }

    /// Every zone cleared on `d`: the whole pass.
    fn all_on(&self, d: Difficulty) -> bool {
        super::data::MAPS.iter().all(|m| self.has(m.name, d))
    }
}

fn at_least(name: &str, d: Difficulty) -> bool {
    Difficulty::ALL
        .into_iter()
        .position(|x| x.name() == name)
        .is_some_and(|i| i >= d as usize)
}

// `ChrClasses.dbc` ids the conditions name.
const PALADIN: u8 = 2;
const WARRIOR: u8 = 1;
const WARLOCK: u8 = 9;
const DRUID: u8 = 11;

/// The Imp: the final boss down within this long of its arrival.
const IMP_BOSS_SECS: f32 = 60.0;
/// The Centaur: never standing still longer than this.
const CENTAUR_STILL_SECS: f32 = 3.0;

/// Whether `r`'s hidden condition holds.
fn met(r: SecretRace, p: &Progress, c: &Clears) -> bool {
    let won = |f: &dyn Fn(&WinRecord) -> bool| p.wins.iter().any(f);
    match r {
        SecretRace::Goblin => c.all_on(Difficulty::Normal),
        SecretRace::Skeleton => p.deaths(PALADIN) >= 100,
        SecretRace::Murloc => p.kills("murloc") >= 10_000,
        SecretRace::Gnoll => p.kills("gnoll") >= 5_000,
        SecretRace::Kobold => p.chests >= 100,
        // Heroic or harder.
        SecretRace::Ogre => {
            won(&|w| w.class_id == WARRIOR && at_least(&w.difficulty, Difficulty::Heroic))
        }
        SecretRace::Satyr => data::SATYR_ZONES
            .iter()
            .all(|z| c.has(z, Difficulty::Heroic)),
        SecretRace::Furbolg => won(&|w| w.class_id == DRUID && w.min_hp_frac >= 0.5),
        SecretRace::Dragonspawn => c.all_on(Difficulty::Mythic),
        SecretRace::Quillboar => data::QUILLBOAR_ZONES.iter().all(|z| c.has_any(z)),
        SecretRace::Felguard => won(&|w| {
            w.class_id == WARLOCK && w.difficulty == Difficulty::Mythic.name()
        }),
        SecretRace::Centaur => {
            won(&|w| w.zone == data::CENTAUR_ZONE && w.longest_still <= CENTAUR_STILL_SECS)
        }
        SecretRace::Imp => won(&|w| w.boss_secs.is_some_and(|s| s <= IMP_BOSS_SECS)),
    }
}

/// The races whose condition now holds and that are not unlocked yet, in [`SecretRace::ALL`]'s
/// order.
pub(super) fn newly_unlocked(p: &Progress, clears: &Clears) -> Vec<SecretRace> {
    SecretRace::ALL
        .into_iter()
        .filter(|&r| !p.is_unlocked(r) && met(r, p, clears))
        .collect()
}

/// The creature family a kill counts for, from the enemy's name: the murlocs and the gnolls the
/// rosters field (Riverpaw, Redridge, Rot Hide, Palemane and the named ones).
pub(super) fn family(name: &str) -> Option<&'static str> {
    const GNOLL_TRIBES: [&str; 6] = [
        "Riverpaw",
        "Redridge ",
        "Rot Hide",
        "Palemane",
        "Mosshide",
        "Shadowhide",
    ];
    const GNOLL_NAMED: [&str; 4] = ["Hogger", "Sergeant Brashclaw", "Rohh the Silent", "Snagglespear"];
    if name.contains("Murloc") {
        Some("murloc")
    } else if name.contains("Gnoll")
        || GNOLL_TRIBES.iter().any(|t| name.starts_with(t))
        || GNOLL_NAMED.contains(&name)
    {
        Some("gnoll")
    } else {
        None
    }
}

/// Whether a run's results may be recorded anywhere: secrets, completions, records. The bot,
/// the Demo (`auto::Auto`) and god mode (`WOW_SURVIVORS_GOD`) record nothing.
pub(super) fn records_progress(auto: bool, god: bool) -> bool {
    !auto && !god
}

/// The longest stretch the hero stood still this run, in run time ([`Run::elapsed`] stops for
/// pauses, card screens, the loading screen and death, so none of them counts).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Stillness {
    /// Run time of the last movement.
    since: f32,
    pub longest: f32,
    last: Option<Vec3>,
    /// Run time of the last step.
    last_t: Option<f32>,
}

/// Slower than this across the ground, in yards a second, is standing still: a speed, so the
/// test is the same at 30 and 120 frames a second.
const STILL_SPEED: f32 = 0.5;

impl Stillness {
    pub fn step(&mut self, pos: Vec3, now: f32) {
        let dt = self.last_t.map_or(0.0, |t| now - t);
        let moved = self.last.is_none_or(|l| {
            let d = Vec3::new(pos.x - l.x, 0.0, pos.z - l.z).length();
            // No run time passed (a paused frame): only a real displacement counts as moving.
            if dt > 0.0 {
                d / dt > STILL_SPEED
            } else {
                d > 0.0
            }
        });
        self.last = Some(pos);
        self.last_t = Some(now);
        if moved {
            self.since = now;
        } else {
            self.longest = self.longest.max(now - self.since);
        }
    }
}

/// The hero's stillness, every frame of play.
fn track_stillness(mut run: ResMut<Run>, hero: Query<&Transform, With<super::hero::Hero>>) {
    let Ok(h) = hero.single() else { return };
    let now = run.elapsed;
    run.stillness.step(h.translation, now);
}

// --- Looks. ---

/// A secret race's code in a look's race slot: past every `ChrRaces` id.
const LOOK_BASE: u8 = 100;

pub(super) fn look_code(r: SecretRace) -> u8 {
    LOOK_BASE + r as u8
}

pub(super) fn from_look_code(code: u8) -> Option<SecretRace> {
    code.checked_sub(LOOK_BASE)
        .and_then(|i| SecretRace::ALL.get(usize::from(i)).copied())
}

/// The race `UNIT_FIELD_BYTES_0` carries for a look's race slot.
pub(super) fn unit_race(code: u8) -> u8 {
    from_look_code(code).map_or(code, data::unit_race)
}

/// The hero's render scale for a look's race slot.
pub(super) fn look_scale(code: u8) -> f32 {
    from_look_code(code).map_or(1.0, |r| def(r).scale)
}

/// The race's name for a look's race slot, secret or not.
pub(super) fn race_label(code: u8) -> &'static str {
    from_look_code(code).map_or_else(|| crate::char_select::race_name(code), |r| def(r).name)
}

/// A creature secret has one body: the gender pair hides.
pub(super) fn one_body(code: u8) -> bool {
    from_look_code(code).is_some_and(|r| r != SecretRace::Goblin)
}

/// `AnimationData.dbc` Attack1H: the cast a model without spell-cast animations plays.
const ATTACK_1H: u16 = 17;

/// The one-shot a spell cast adds on a caster: a race without cast animations swings its
/// weapon instead, so the cast never stands frozen.
pub(super) fn cast_fallback(casts: bool) -> Option<u16> {
    (!casts).then_some(ATTACK_1H)
}

/// The run's last flourish on a win (`hero::hero_vitals`), not one of the hero's spells.
pub(super) const VICTORY_FLOURISH: u32 = 642;

/// The Hunter's shots (`data`: Arcane, Multi-, Serpent Sting, Volley, Concussive, Aimed): their
/// kit plays the bow, which a swing would override.
const RANGED_SPELLS: [u32; 6] = [3044, 2643, 1978, 1510, 5116, 19434];

/// The one-shot a cast by `spell` adds on a caster that swings to cast ([`swings_to_cast`]):
/// only a true spell cast swings; a ranged attack keeps its bow and the victory flourish its own.
pub(super) fn swing_for_cast(swings: bool, spell: u32) -> Option<u16> {
    if !swings || spell == VICTORY_FLOURISH || RANGED_SPELLS.contains(&spell) {
        return None;
    }
    cast_fallback(false)
}

/// Whether a look's race slot casts with the attack swing ([`cast_fallback`]).
pub(super) fn swings_to_cast(code: u8) -> bool {
    from_look_code(code).is_some_and(|r| cast_fallback(def(r).casts).is_some())
}

/// The unlocked secret races' looks for a class, `(display, look code, sex)`, one per display
/// variant, in [`SecretRace::ALL`]'s order.
pub(super) fn secret_looks(class_id: u8, unlocked: &[String]) -> Vec<(u32, u8, u8)> {
    SecretRace::ALL
        .into_iter()
        .filter(|&r| unlocked.iter().any(|k| k == def(r).key))
        .filter(|&r| def(r).classes.contains(&class_id))
        .flat_map(|r| {
            def(r)
                .displays
                .iter()
                .map(move |&d| (d, look_code(r), data::display_sex(d)))
        })
        .collect()
}

/// A secret race's button face in the race picker: its icon, loaded once the archives are.
#[derive(Component)]
pub(super) struct SecretFace(pub &'static str);

#[derive(Component)]
struct FaceDone;

fn fill_secret_faces(
    mut commands: Commands,
    mut faces: Query<(Entity, &mut ImageNode, &SecretFace), Without<FaceDone>>,
    mut assets: Option<ResMut<benilla_assets::WorldAssets>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Nothing to fill: never touch `WorldAssets`, whose `DerefMut` would mark it changed.
    if faces.is_empty() {
        return;
    }
    let Some(assets) = assets.as_deref_mut() else {
        return;
    };
    for (e, mut img, face) in &mut faces {
        if let Some(h) = assets.sprite_texture(&format!("{}.blp", face.0), &mut images) {
            img.image = h;
            img.color = Color::WHITE;
        }
        commands.entity(e).insert(FaceDone);
    }
}

// --- Saving. ---

fn path() -> Option<std::path::PathBuf> {
    crate::local_state::home().map(|h| h.join("secrets.toml"))
}

/// The saved progress, or a fresh one (no file, or a file that does not parse).
pub(super) fn load() -> Progress {
    path().map(|p| load_from(&p)).unwrap_or_default()
}

/// The progress at `path`. A file that is there but does not parse is kept as `<path>.bak`
/// (an older `.bak` moves to `.bak.1`) before anything can save over it, with a warning: the
/// account's unlocks are never lost silently.
fn load_from(path: &std::path::Path) -> Progress {
    let Ok(text) = crate::local_state::read_to_string(path) else {
        return Progress::default();
    };
    match toml::from_str::<Progress>(&text) {
        Ok(p) => p,
        Err(e) => {
            let bak = path.with_extension("toml.bak");
            if let Ok(old) = crate::local_state::read_to_string(&bak) {
                let _ = crate::local_state::write_atomic(&path.with_extension("toml.bak.1"), &old);
            }
            let kept = crate::local_state::write_atomic(&bak, &text);
            warn!(
                "survivors: {} does not parse ({e}); kept as {} ({kept:?}), starting fresh",
                path.display(),
                bak.display()
            );
            Progress::default()
        }
    }
}

pub(super) fn save(p: &Progress) {
    let Some(path) = path() else { return };
    if let Ok(text) = toml::to_string(p) {
        let _ = crate::local_state::write_atomic(&path, &text);
    }
}

// --- Counting. ---

/// Something a run did that a secret counts.
#[derive(Message, Clone, Debug)]
pub(super) enum SecretEvent {
    /// An enemy died, by name; once per enemy.
    Kill(&'static str),
    Chest,
    /// The run is over, at the results.
    RunEnd {
        won: bool,
        class_id: u8,
        zone: &'static str,
        difficulty: Difficulty,
        secs: f32,
        min_hp_frac: f32,
        /// Run time the final boss arrived and fell.
        final_boss_at: Option<f32>,
        won_at: Option<f32>,
        /// The longest the hero stood still, in run seconds.
        longest_still: f32,
    },
}

/// A secret race just unlocked.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Revealed(pub SecretRace);

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Progress>()
        .add_message::<SecretEvent>()
        .add_message::<Revealed>()
        .add_systems(OnEnter(Phase::GameOver), run_end)
        .add_systems(
            Update,
            track_stillness
                .after(super::hero::hero_vitals)
                .run_if(in_state(Phase::Playing)),
        )
        .add_systems(Update, (load_once, count).chain())
        .add_systems(OnEnter(Phase::Menu), save_if_dirty)
        .add_systems(Update, fill_secret_faces)
        .add_plugins((page::plugin, reveal::plugin));
}

/// Read the file once, on the first frame (as `characters` does, for the page's store).
fn load_once(mut progress: ResMut<Progress>) {
    if progress.loaded {
        return;
    }
    *progress = Progress {
        loaded: true,
        ..load()
    };
}

/// The run's end, as entering the results knows it.
fn run_end(run: Res<Run>, selection: Res<Selection>, mut out: MessageWriter<SecretEvent>) {
    out.write(SecretEvent::RunEnd {
        won: run.won,
        class_id: run.class.id,
        zone: selection.stage_def().name,
        difficulty: super::data::difficulty(),
        secs: run.elapsed,
        min_hp_frac: run.min_hp_frac,
        final_boss_at: run.final_boss_at,
        won_at: run.won_at,
        longest_still: run.stillness.longest,
    });
}

/// Count what the run did, reveal what that unlocks, save at the end and on an unlock.
pub(super) fn count(
    mut events: MessageReader<SecretEvent>,
    mut progress: ResMut<Progress>,
    mut revealed: MessageWriter<Revealed>,
    chars: Res<Characters>,
    run: Option<Res<Run>>,
    auto: Option<Res<super::auto::Auto>>,
) {
    // The bot, the Demo and god mode never count.
    if !records_progress(auto.is_some(), super::hero::god_mode()) {
        events.read().for_each(drop);
        return;
    }
    let p = &mut *progress;
    let (mut ended, mut counted) = (false, false);
    for e in events.read() {
        match *e {
            SecretEvent::Kill(name) => {
                if let Some(f) = family(name) {
                    // `get_mut` first: no allocation on the thousands of kills after the first.
                    match p.kills_by_family.get_mut(f) {
                        Some(n) => *n += 1,
                        None => {
                            p.kills_by_family.insert(f.to_string(), 1);
                        }
                    }
                    counted = true;
                }
            }
            SecretEvent::Chest => {
                p.chests += 1;
                counted = true;
            }
            SecretEvent::RunEnd {
                won,
                class_id,
                zone,
                difficulty,
                secs,
                min_hp_frac,
                final_boss_at,
                won_at,
                longest_still,
            } => {
                if won {
                    p.wins.push(WinRecord {
                        class_id,
                        zone: zone.to_string(),
                        difficulty: difficulty.name().to_string(),
                        secs,
                        min_hp_frac,
                        boss_secs: final_boss_at.zip(won_at).map(|(a, b)| b - a),
                        longest_still,
                    });
                } else {
                    *p.deaths_by_class.entry(class_id).or_default() += 1;
                }
                ended = true;
            }
        }
    }
    if !ended && !counted {
        return;
    }
    p.dirty = true;
    // The zone conditions can change only at a run's end, when a completion is recorded.
    let clears = if ended {
        Clears::from_parts(chars.list.iter().flat_map(|c| &c.completions), &p.wins)
    } else {
        Clears::default()
    };
    let found = newly_unlocked(p, &clears);
    let elapsed = run.map_or(0.0, |r| r.elapsed);
    for &r in &found {
        p.unlocked.push(def(r).key.to_string());
        revealed.write(Revealed(r));
        super::combatlog::event(elapsed, format!("secret revealed: {}", def(r).name));
    }
    if ended || !found.is_empty() {
        save(p);
        p.dirty = false;
    }
}

/// A run left from the pause menu never reaches the results: keep what it counted.
fn save_if_dirty(mut progress: ResMut<Progress>) {
    if progress.dirty {
        save(&progress);
        progress.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::message::Messages;
    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    fn win(class_id: u8, zone: &str, d: Difficulty, secs: f32, min_hp_frac: f32) -> WinRecord {
        WinRecord {
            class_id,
            zone: zone.to_string(),
            difficulty: d.name().to_string(),
            secs,
            min_hp_frac,
            boss_secs: None,
            longest_still: 30.0,
        }
    }

    fn found(p: &Progress, c: &Clears) -> Vec<SecretRace> {
        newly_unlocked(p, c)
    }

    fn none() -> Clears {
        Clears::default()
    }

    /// Every zone on `d`.
    fn pass(c: &mut Clears, d: Difficulty) {
        for m in super::super::data::MAPS {
            c.insert(m.name, d);
        }
    }

    #[test]
    fn a_hundred_paladin_deaths_raise_the_skeleton() {
        let mut p = Progress::default();
        p.deaths_by_class.insert(PALADIN, 99);
        assert!(found(&p, &none()).is_empty());
        p.deaths_by_class.insert(PALADIN, 100);
        assert_eq!(found(&p, &none()), vec![SecretRace::Skeleton]);
        // Another class's deaths do not count.
        let mut q = Progress::default();
        q.deaths_by_class.insert(WARRIOR, 500);
        assert!(found(&q, &none()).is_empty());
    }

    #[test]
    fn ten_thousand_murlocs() {
        let mut p = Progress::default();
        p.kills_by_family.insert("murloc".into(), 9_999);
        assert!(found(&p, &none()).is_empty());
        p.kills_by_family.insert("murloc".into(), 10_000);
        assert_eq!(found(&p, &none()), vec![SecretRace::Murloc]);
    }

    #[test]
    fn five_thousand_gnolls() {
        let mut p = Progress::default();
        p.kills_by_family.insert("gnoll".into(), 4_999);
        assert!(found(&p, &none()).is_empty());
        p.kills_by_family.insert("gnoll".into(), 5_000);
        assert_eq!(found(&p, &none()), vec![SecretRace::Gnoll]);
    }

    #[test]
    fn a_hundred_chests() {
        let mut p = Progress {
            chests: 99,
            ..default()
        };
        assert!(found(&p, &none()).is_empty());
        p.chests = 100;
        assert_eq!(found(&p, &none()), vec![SecretRace::Kobold]);
    }

    #[test]
    fn a_heroic_warrior_win_calls_the_ogre() {
        let mut p = Progress::default();
        p.wins.push(win(WARRIOR, "Elwynn Forest", Difficulty::Normal, 700.0, 0.2));
        assert!(found(&p, &none()).is_empty());
        p.wins.push(win(WARRIOR, "Elwynn Forest", Difficulty::Heroic, 1000.0, 0.2));
        assert_eq!(found(&p, &none()), vec![SecretRace::Ogre]);
    }

    #[test]
    fn a_druid_never_below_half_health_calls_the_furbolg() {
        let mut p = Progress::default();
        p.wins.push(win(DRUID, "Teldrassil", Difficulty::Normal, 700.0, 0.49));
        assert!(found(&p, &none()).is_empty());
        p.wins.push(win(DRUID, "Teldrassil", Difficulty::Normal, 700.0, 0.51));
        assert_eq!(found(&p, &none()), vec![SecretRace::Furbolg]);
    }

    #[test]
    fn a_mythic_warlock_win_calls_the_felguard_and_the_full_pass_the_dragonspawn() {
        let mut p = Progress::default();
        p.wins.push(win(WARLOCK, "Naxxramas", Difficulty::Mythic, 1300.0, 0.3));
        assert_eq!(found(&p, &none()), vec![SecretRace::Felguard]);
        let mut all = Clears::default();
        pass(&mut all, Difficulty::Mythic);
        // The Mythic pass holds The Barrens and Durotar too: the Quillboar comes with it.
        assert_eq!(
            found(&p, &all),
            vec![
                SecretRace::Dragonspawn,
                SecretRace::Quillboar,
                SecretRace::Felguard
            ]
        );
        // A Heroic Warlock win is not enough.
        let mut q = Progress::default();
        q.wins.push(win(WARLOCK, "Naxxramas", Difficulty::Heroic, 1000.0, 0.3));
        assert!(found(&q, &none()).is_empty());
    }

    #[test]
    fn the_final_boss_down_within_a_minute_calls_the_imp() {
        let mut p = Progress::default();
        p.wins.push(WinRecord {
            boss_secs: Some(60.1),
            ..win(8, "Durotar", Difficulty::Normal, 660.1, 0.3)
        });
        // A win from before the arrival was recorded.
        p.wins.push(win(8, "Durotar", Difficulty::Normal, 300.0, 0.3));
        assert!(found(&p, &none()).is_empty());
        p.wins.push(WinRecord {
            boss_secs: Some(59.9),
            ..win(8, "Durotar", Difficulty::Normal, 659.9, 0.3)
        });
        assert_eq!(found(&p, &none()), vec![SecretRace::Imp]);
    }

    #[test]
    fn a_barrens_win_never_standing_still_calls_the_centaur() {
        let still = |s: f32, zone: &str| WinRecord {
            longest_still: s,
            ..win(3, zone, Difficulty::Normal, 650.0, 0.3)
        };
        let mut p = Progress::default();
        p.wins.push(still(3.1, data::CENTAUR_ZONE));
        p.wins.push(still(0.5, "Durotar"));
        assert!(found(&p, &none()).is_empty());
        p.wins.push(still(2.9, data::CENTAUR_ZONE));
        assert_eq!(found(&p, &none()), vec![SecretRace::Centaur]);
    }

    #[test]
    fn stillness_counts_only_run_time_without_movement() {
        // Creeping at 0.3 yd/s is still, at any frame rate: 120 frames a second for 2 s...
        let mut fast = Stillness::default();
        for i in 0..=240 {
            let t = i as f32 / 120.0;
            fast.step(Vec3::new(0.3 * t, 0.0, 0.0), t);
        }
        // ...or 20 frames a second.
        let mut slow = Stillness::default();
        for i in 0..=40 {
            let t = i as f32 / 20.0;
            slow.step(Vec3::new(0.3 * t, 0.0, 0.0), t);
        }
        assert!((fast.longest - 2.0).abs() < 1e-3, "{}", fast.longest);
        assert!((slow.longest - 2.0).abs() < 1e-3, "{}", slow.longest);
        // Walking at 1 yd/s is moving, at either rate.
        let mut walk = Stillness::default();
        for i in 0..=240 {
            let t = i as f32 / 120.0;
            walk.step(Vec3::new(t, 0.0, 0.0), t);
        }
        assert!(walk.longest < 1e-3, "{}", walk.longest);

        let mut s = Stillness::default();
        let here = Vec3::new(1.0, 0.0, 1.0);
        s.step(here, 0.0);
        s.step(here, 1.0);
        s.step(here, 2.9);
        // A step away resets the stretch; falling or rising in place is not moving.
        s.step(here + Vec3::X, 3.0);
        s.step(here + Vec3::X + Vec3::Y, 4.0);
        assert!((s.longest - 2.9).abs() < 1e-5);
        // A pause stops run time, so standing through it adds nothing: 4.0 → 4.0.
        s.step(here + Vec3::X, 4.0);
        assert!((s.longest - 2.9).abs() < 1e-5);
        s.step(here + Vec3::X, 7.1);
        assert!((s.longest - 4.1).abs() < 1e-5);
    }

    #[test]
    fn the_bot_the_demo_and_god_mode_record_nothing() {
        assert!(records_progress(false, false));
        assert!(!records_progress(true, false));
        assert!(!records_progress(false, true));
        assert!(!records_progress(true, true));
    }

    #[test]
    fn the_whole_normal_pass_calls_the_goblin() {
        let p = Progress::default();
        let mut c = Clears::default();
        let maps = super::super::data::MAPS;
        for m in &maps[1..] {
            c.insert(m.name, Difficulty::Normal);
        }
        // One zone short.
        assert!(!found(&p, &c).contains(&SecretRace::Goblin));
        c.insert(maps[0].name, Difficulty::Normal);
        assert!(found(&p, &c).contains(&SecretRace::Goblin));
    }

    #[test]
    fn teldrassil_and_hyjal_on_heroic_call_the_satyr() {
        // The spec's Ashenvale/Felwood: Teldrassil and Mount Hyjal (`data::SATYR_ZONES`).
        let p = Progress::default();
        let mut c = Clears::default();
        c.insert("Teldrassil", Difficulty::Heroic);
        c.insert("Mount Hyjal", Difficulty::Normal);
        assert!(found(&p, &c).is_empty());
        c.insert("Mount Hyjal", Difficulty::Heroic);
        assert_eq!(found(&p, &c), vec![SecretRace::Satyr]);
    }

    #[test]
    fn the_barrens_and_durotar_call_the_quillboar() {
        // The spec's Razorfen area: The Barrens and Durotar (`data::QUILLBOAR_ZONES`).
        let p = Progress::default();
        let mut c = Clears::default();
        c.insert("The Barrens", Difficulty::Normal);
        assert!(found(&p, &c).is_empty());
        c.insert("Durotar", Difficulty::Heroic);
        assert_eq!(found(&p, &c), vec![SecretRace::Quillboar]);
    }

    #[test]
    fn wins_count_as_clears() {
        let p = Progress {
            wins: vec![
                win(1, "The Barrens", Difficulty::Normal, 700.0, 0.2),
                win(1, "Durotar", Difficulty::Normal, 700.0, 0.2),
            ],
            ..default()
        };
        let c = Clears::from_parts(std::iter::empty(), &p.wins);
        assert_eq!(found(&p, &c), vec![SecretRace::Quillboar]);
    }

    #[test]
    fn already_unlocked_is_not_returned_again() {
        let mut p = Progress {
            chests: 500,
            ..default()
        };
        p.deaths_by_class.insert(PALADIN, 100);
        assert_eq!(
            found(&p, &none()),
            vec![SecretRace::Skeleton, SecretRace::Kobold]
        );
        p.unlocked.push("kobold".into());
        assert_eq!(found(&p, &none()), vec![SecretRace::Skeleton]);
        p.unlocked.push("skeleton".into());
        assert!(found(&p, &none()).is_empty());
    }

    #[test]
    fn progress_round_trips_through_toml() {
        let mut p = Progress {
            unlocked: vec!["murloc".into(), "imp".into()],
            chests: 42,
            wins: vec![win(11, "Mulgore", Difficulty::Heroic, 912.5, 0.62)],
            ..default()
        };
        p.deaths_by_class.insert(PALADIN, 7);
        p.deaths_by_class.insert(WARLOCK, 3);
        p.kills_by_family.insert("gnoll".into(), 1234);
        let text = toml::to_string(&p).unwrap();
        let back: Progress = toml::from_str(&text).unwrap();
        assert_eq!(back, p);
        // An empty or older file loads as a fresh progress.
        assert_eq!(toml::from_str::<Progress>("").unwrap(), Progress::default());
        let old: Progress = toml::from_str("chests = 3\n").unwrap();
        assert_eq!(old.chests, 3);
        // A file from the Endless draft, and a win recorded before the boss and stillness
        // fields, still load.
        let older: Progress = toml::from_str(
            "chests = 1\n[endless_best_secs]\n\"The Barrens\" = 300.0\n\n[[wins]]\nclass_id = 3\nzone = \"The Barrens\"\ndifficulty = \"Normal\"\nsecs = 650.0\nmin_hp_frac = 0.4\n",
        )
        .unwrap();
        assert_eq!(older.wins[0].boss_secs, None);
        assert_eq!(older.wins[0].longest_still, f32::MAX);
        assert!(newly_unlocked(&older, &none()).is_empty());
    }

    #[test]
    fn families_from_names() {
        assert_eq!(family("Murloc Forager"), Some("murloc"));
        assert_eq!(family("Murloc"), Some("murloc"));
        assert_eq!(family("Riverpaw Bandit"), Some("gnoll"));
        assert_eq!(family("Redridge Mongrel"), Some("gnoll"));
        assert_eq!(family("Rot Hide Gnoll"), Some("gnoll"));
        assert_eq!(family("Palemane Tanner"), Some("gnoll"));
        assert_eq!(family("Hogger"), Some("gnoll"));
        assert_eq!(family("Kobold Miner"), None);
        assert_eq!(family("Defias Thug"), None);
        assert_eq!(family("Hogger's Pet"), None);
    }

    // --- Looks. ---

    #[test]
    fn a_skeleton_paladin_only_once_unlocked() {
        let unlocked = vec!["skeleton".to_string()];
        let looks = secret_looks(PALADIN, &unlocked);
        assert_eq!(looks.len(), data::def(SecretRace::Skeleton).displays.len());
        assert!(looks
            .iter()
            .all(|&(_, r, s)| from_look_code(r) == Some(SecretRace::Skeleton) && s == 0));
        assert!(secret_looks(PALADIN, &[]).is_empty());
        // A Skeleton cannot be a Mage.
        assert!(secret_looks(8, &unlocked).is_empty());
    }

    #[test]
    fn goblin_looks_come_in_both_sexes_and_the_character_race() {
        let looks = secret_looks(8, &["goblin".to_string()]);
        assert!(looks.iter().any(|l| l.2 == 0) && looks.iter().any(|l| l.2 == 1));
        let code = look_code(SecretRace::Goblin);
        assert_eq!(unit_race(code), 9);
        assert!(!one_body(code));
        assert_eq!(unit_race(look_code(SecretRace::Murloc)), 0);
        assert!(one_body(look_code(SecretRace::Murloc)));
        // A normal race passes through.
        assert_eq!(unit_race(5), 5);
        assert_eq!(look_scale(5), 1.0);
        assert_eq!(race_label(look_code(SecretRace::Imp)), "Imp");
        assert_eq!(from_look_code(99), None);
        assert_eq!(from_look_code(LOOK_BASE + 13), None);
    }

    #[test]
    fn a_race_without_cast_animations_casts_with_the_swing() {
        assert_eq!(cast_fallback(false), Some(ATTACK_1H));
        assert_eq!(cast_fallback(true), None);
        assert!(swings_to_cast(look_code(SecretRace::Skeleton)));
        assert!(!swings_to_cast(look_code(SecretRace::Ogre)));
        assert!(!swings_to_cast(1));
    }

    #[test]
    fn only_true_spell_casts_swing() {
        // A Frostbolt (116) on a swinger swings; on a caster it does not.
        assert_eq!(swing_for_cast(true, 116), Some(ATTACK_1H));
        assert_eq!(swing_for_cast(false, 116), None);
        // A Murloc, Gnoll or Centaur Hunter keeps its bow on every shot.
        for spell in RANGED_SPELLS {
            assert_eq!(swing_for_cast(true, spell), None, "spell {spell}");
        }
        for key in ["arcane_shot", "multi_shot", "aimed_shot", "concussive_shot"] {
            let spell = crate::survivors::data::ability(key).spell;
            assert_eq!(swing_for_cast(true, spell), None, "{key}");
        }
        // The win's flourish is not a cast to swing.
        assert_eq!(swing_for_cast(true, VICTORY_FLOURISH), None);
    }

    #[test]
    fn secret_races_are_neither_alliance_nor_horde_on_the_minimap() {
        use crate::survivors::minimap::faction_groups;
        for r in SecretRace::ALL {
            assert_eq!(faction_groups(look_code(r)), (0, 0), "{r:?}");
        }
        assert_eq!(faction_groups(1), (2, 4));
        assert_eq!(faction_groups(2), (4, 2));
    }

    #[test]
    fn a_file_that_does_not_parse_is_kept_aside() {
        let dir = std::env::temp_dir().join(format!("secrets-bak-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secrets.toml");
        std::fs::write(&path, "unlocked = [\"murloc\"\nchests = ").unwrap();
        std::fs::write(dir.join("secrets.toml.bak"), "older").unwrap();
        assert_eq!(load_from(&path), Progress::default());
        assert_eq!(
            std::fs::read_to_string(dir.join("secrets.toml.bak")).unwrap(),
            "unlocked = [\"murloc\"\nchests = "
        );
        assert_eq!(std::fs::read_to_string(dir.join("secrets.toml.bak.1")).unwrap(), "older");
        // A good file loads as it is, and a missing one is fresh.
        std::fs::write(&path, "unlocked = [\"murloc\"]\nchests = 3\n").unwrap();
        let p = load_from(&path);
        assert_eq!((p.unlocked.len(), p.chests), (1, 3));
        assert_eq!(load_from(&dir.join("none.toml")), Progress::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_picked_race_again_cycles_its_skins() {
        let m = look_code(SecretRace::Murloc);
        let looks = vec![(1, 1, 0), (2, 1, 1), (31, m, 0), (188, m, 0), (1755, m, 0)];
        assert_eq!(super::super::menu::next_skin(&looks, 2), Some(3));
        assert_eq!(super::super::menu::next_skin(&looks, 4), Some(2));
        // One look of its race and sex: itself.
        assert_eq!(super::super::menu::next_skin(&looks, 0), Some(0));
    }

    #[test]
    fn the_skin_hint_shows_only_for_a_secret_with_another_look() {
        use super::super::menu::has_other_skin;
        let m = look_code(SecretRace::Murloc);
        let k = look_code(SecretRace::Imp);
        let looks = vec![(1, 1, 0), (2, 1, 0), (31, m, 0), (188, m, 0), (4449, k, 0)];
        assert!(has_other_skin(&looks, Some(2)));
        // A normal race with two looks: no hint; a secret with one: none either.
        assert!(!has_other_skin(&looks, Some(0)));
        assert!(!has_other_skin(&looks, Some(4)));
        assert!(!has_other_skin(&looks, None));
    }

    fn saved(text: &str) -> super::super::characters::Character {
        toml::from_str(text).expect("a saved character loads")
    }

    const OLD_SAVE: &str =
        "id = 3\nname = \"Thaldor\"\nclass_id = 2\nrace = 1\nsex = 0\ncompletions = []\n";

    #[test]
    fn an_old_character_loads_without_a_secret() {
        let c = saved(OLD_SAVE);
        assert_eq!(c.secret, None);
        assert_eq!(c.skin, 0);
        let looks = vec![(1, 1, 0), (2, 3, 0)];
        assert_eq!(super::super::characters::look_of(&c, &looks), Some(0));
    }

    #[test]
    fn a_secret_character_wears_its_skin_and_an_unknown_one_falls_back() {
        let s = look_code(SecretRace::Skeleton);
        let looks = vec![(1, 1, 0), (2, 3, 0), (158, s, 0), (200, s, 0), (201, s, 0)];
        let mut c = saved(&format!("{OLD_SAVE}secret = \"skeleton\"\nskin = 1\n"));
        // The stored code is stale; the key decides.
        c.race = 1;
        assert_eq!(super::super::characters::look_of(&c, &looks), Some(3));
        c.skin = 9;
        assert_eq!(super::super::characters::look_of(&c, &looks), Some(2));
        let naga = saved(&format!("{OLD_SAVE}secret = \"naga\"\n"));
        assert_eq!(super::super::characters::look_of(&naga, &looks), Some(0));
        assert_eq!(naga.race_label(), "Unknown");
        assert_eq!(c.race_label(), "Skeleton");
    }

    // --- Counting in a world. ---

    fn world() -> World {
        let mut w = World::new();
        w.init_resource::<Messages<SecretEvent>>();
        w.init_resource::<Messages<Revealed>>();
        w.insert_resource(Progress::default());
        w.init_resource::<Characters>();
        w
    }

    fn send(w: &mut World, e: SecretEvent) {
        w.resource_mut::<Messages<SecretEvent>>().write(e);
    }

    fn run_end_event(won: bool, class_id: u8, min_hp_frac: f32) -> SecretEvent {
        SecretEvent::RunEnd {
            won,
            class_id,
            zone: "Mulgore",
            difficulty: Difficulty::Normal,
            secs: 650.0,
            min_hp_frac,
            // The boss took 90 s: no Imp.
            final_boss_at: Some(560.0),
            won_at: won.then_some(650.0),
            longest_still: 10.0,
        }
    }

    fn revealed(w: &mut World) -> Vec<SecretRace> {
        w.resource_mut::<Messages<Revealed>>()
            .drain()
            .map(|r| r.0)
            .collect()
    }

    #[test]
    fn kills_count_by_family() {
        let mut w = world();
        for _ in 0..3 {
            send(&mut w, SecretEvent::Kill("Murloc Forager"));
        }
        send(&mut w, SecretEvent::Kill("Kobold Miner"));
        // One system, as the app runs it: its reader keeps its place between frames.
        let system = w.register_system(count);
        w.run_system(system).unwrap();
        let p = w.resource::<Progress>();
        assert_eq!(p.kills_by_family.get("murloc"), Some(&3));
        assert_eq!(p.kills_by_family.len(), 1);
        // Read once: the next frame counts nothing again.
        w.run_system(system).unwrap();
        assert_eq!(w.resource::<Progress>().kills_by_family["murloc"], 3);
    }

    #[test]
    fn the_bot_never_counts() {
        let mut w = world();
        w.insert_resource(super::super::auto::for_demo(0, 0));
        send(&mut w, SecretEvent::Kill("Murloc Forager"));
        send(&mut w, SecretEvent::Chest);
        send(&mut w, run_end_event(false, PALADIN, 0.0));
        w.run_system_once(count).unwrap();
        assert_eq!(*w.resource::<Progress>(), Progress::default());
        assert!(revealed(&mut w).is_empty());
    }

    #[test]
    fn a_lost_paladin_run_is_one_death() {
        let mut w = world();
        send(&mut w, run_end_event(false, PALADIN, 0.0));
        w.run_system_once(count).unwrap();
        let p = w.resource::<Progress>();
        assert_eq!(p.deaths_by_class.get(&PALADIN), Some(&1));
        assert!(p.wins.is_empty());
    }

    #[test]
    fn a_won_druid_run_keeps_its_lowest_health() {
        let mut w = world();
        send(&mut w, run_end_event(true, DRUID, 0.6));
        w.run_system_once(count).unwrap();
        let p = w.resource::<Progress>();
        assert_eq!(p.wins.len(), 1);
        assert_eq!(p.wins[0].min_hp_frac, 0.6);
        assert_eq!(p.wins[0].zone, "Mulgore");
        assert!(p.deaths_by_class.is_empty());
        assert!(p.is_unlocked(SecretRace::Furbolg));
        assert_eq!(revealed(&mut w), vec![SecretRace::Furbolg]);
    }

    #[test]
    fn the_hundredth_chest_reveals_the_kobold_mid_run_once() {
        let mut w = world();
        w.resource_mut::<Progress>().chests = 99;
        let system = w.register_system(count);
        send(&mut w, SecretEvent::Chest);
        w.run_system(system).unwrap();
        assert_eq!(revealed(&mut w), vec![SecretRace::Kobold]);
        send(&mut w, SecretEvent::Chest);
        w.run_system(system).unwrap();
        assert!(revealed(&mut w).is_empty());
        assert_eq!(w.resource::<Progress>().unlocked, vec!["kobold".to_string()]);
    }

    #[test]
    fn two_unlocks_at_once_are_two_reveals() {
        let mut w = world();
        // One more Warlock Mythic win with the whole Mythic pass held by a character.
        let mut hero = Vec::new();
        for m in super::super::data::MAPS {
            hero.push((m.name.to_string(), "Mythic".to_string()));
        }
        w.resource_mut::<Characters>()
            .list
            .push(super::super::characters::Character {
                id: 1,
                name: "Test".into(),
                class_id: WARLOCK,
                race: 1,
                sex: 0,
                secret: None,
                skin: 0,
                completions: hero,
                talents: Default::default(),
            });
        send(
            &mut w,
            SecretEvent::RunEnd {
                won: true,
                class_id: WARLOCK,
                zone: "Naxxramas",
                difficulty: Difficulty::Mythic,
                secs: 1300.0,
                min_hp_frac: 0.2,
                final_boss_at: Some(1200.0),
                won_at: Some(1300.0),
                longest_still: 10.0,
            },
        );
        w.run_system_once(count).unwrap();
        assert_eq!(
            revealed(&mut w),
            vec![
                SecretRace::Dragonspawn,
                SecretRace::Quillboar,
                SecretRace::Felguard
            ]
        );
    }
}
