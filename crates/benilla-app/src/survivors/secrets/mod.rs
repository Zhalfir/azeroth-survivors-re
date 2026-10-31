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
//!
//! **Endless** does not exist yet: [`Progress::endless_best_secs`] is kept and read, but nothing
//! writes it, so the Centaur waits on that mode.

pub(super) mod data;

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::data::Difficulty;
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
    /// Endless mode's best time per zone, in seconds. Nothing writes it yet.
    pub endless_best_secs: HashMap<String, f32>,
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
        SecretRace::Centaur => p
            .endless_best_secs
            .get(data::CENTAUR_ZONE)
            .is_some_and(|&s| s >= 1200.0),
        SecretRace::Imp => won(&|w| w.secs < 480.0),
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

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Progress>()
        .add_systems(Update, load_once);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn win(class_id: u8, zone: &str, d: Difficulty, secs: f32, min_hp_frac: f32) -> WinRecord {
        WinRecord {
            class_id,
            zone: zone.to_string(),
            difficulty: d.name().to_string(),
            secs,
            min_hp_frac,
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
    fn a_win_under_eight_minutes_calls_the_imp() {
        let mut p = Progress::default();
        p.wins.push(win(8, "Durotar", Difficulty::Normal, 481.0, 0.3));
        assert!(found(&p, &none()).is_empty());
        p.wins.push(win(8, "Durotar", Difficulty::Normal, 479.0, 0.3));
        assert_eq!(found(&p, &none()), vec![SecretRace::Imp]);
    }

    #[test]
    fn twenty_endless_minutes_call_the_centaur() {
        let mut p = Progress::default();
        p.endless_best_secs
            .insert(data::CENTAUR_ZONE.to_string(), 1_199.0);
        assert!(found(&p, &none()).is_empty());
        p.endless_best_secs
            .insert(data::CENTAUR_ZONE.to_string(), 1_200.0);
        assert_eq!(found(&p, &none()), vec![SecretRace::Centaur]);
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
        p.endless_best_secs.insert("The Barrens".into(), 300.0);
        let text = toml::to_string(&p).unwrap();
        let back: Progress = toml::from_str(&text).unwrap();
        assert_eq!(back, p);
        // An empty or older file loads as a fresh progress.
        assert_eq!(toml::from_str::<Progress>("").unwrap(), Progress::default());
        let old: Progress = toml::from_str("chests = 3\n").unwrap();
        assert_eq!(old.chests, 3);
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
}
