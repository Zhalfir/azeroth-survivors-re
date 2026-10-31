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
//! Each unlock writes one [`Revealed`], for the banner (a later task).


pub(super) mod data;

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
}

/// Less than this between two frames is standing still.
const STILL_EPSILON: f32 = 0.01;

impl Stillness {
    pub fn step(&mut self, pos: Vec3, now: f32) {
        let moved = self.last.is_none_or(|l| {
            Vec3::new(pos.x - l.x, 0.0, pos.z - l.z).length() > STILL_EPSILON
        });
        self.last = Some(pos);
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

// --- Saving. ---

fn path() -> Option<std::path::PathBuf> {
    crate::local_state::home().map(|h| h.join("secrets.toml"))
}

/// The saved progress, or a fresh one (no file, or a file that does not parse).
pub(super) fn load() -> Progress {
    path()
        .and_then(|p| crate::local_state::read_to_string(&p).ok())
        .and_then(|t| toml::from_str::<Progress>(&t).ok())
        .unwrap_or_default()
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
        .add_systems(OnEnter(Phase::Menu), save_if_dirty);
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
                    *p.kills_by_family.entry(f.to_string()).or_default() += 1;
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
