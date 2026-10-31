//! The 13 secret races (`docs/superpowers/specs/2026-10-31-secret-races-design.md`): a look
//! only, never stats, spells or passives. Each is a list of `CreatureDisplayInfo` ids, its colour
//! variants, read from the 1.12 client's DBCs on 2026-10-31:
//!
//! - `CreatureDisplayInfo` → `ModelID` → `CreatureModelData.ModelPath`; each variant below is the
//!   first display carrying its own skin set, and every skin texture exists in the MPQs. Quest
//!   skins (Death's Head, zombie quillboar) and the outsized boss variants are left out.
//! - **Scale.** The unit's `OBJECT_FIELD_SCALE_X` alone sizes it (`0x613ef0`; the client does not
//!   apply the display's own scale), so [`SecretDef::scale`] is the whole render scale. It puts
//!   the model's vertex height within ±20 % of the Human male's (2.04 model units): the natural
//!   size where it already is, else clamped to the band's edge (a small Kobold or Imp stays the
//!   smallest, an Ogre or Felguard the tallest).
//! - **Casts.** Whether the model authors `SpellCastDirected`/`SpellCastOmni` (53/54); without
//!   them a spell plays the attack swing (spec: "Creatures without cast animations play the
//!   attack swing for spells"). Every model here has both hand attachments, Run, Death and a
//!   one-hand attack.
//!
//! **The Goblin is a character, not a creature.** `CharSections.dbc` has race 9 rows, but only
//! the base skin (section 0: three colours per sex), no faces, hair or facial hair, so there is
//! nothing to customise. It takes the character path the eight races take: race 9 in
//! `UNIT_FIELD_BYTES_0` and a character-model NPC display (`Character\Goblin\{Male,Female}`,
//! `CreatureDisplayInfoExtra` + its shipped `BakedNpcTextures` atlas, all present), the same
//! composite the class trainers' looks use.

/// A hidden race, unlocked account-wide by a hidden condition ([`super::newly_unlocked`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::survivors) enum SecretRace {
    Goblin,
    Skeleton,
    Murloc,
    Gnoll,
    Kobold,
    Ogre,
    Satyr,
    Furbolg,
    Dragonspawn,
    Quillboar,
    Felguard,
    Centaur,
    Imp,
}

impl SecretRace {
    pub(in crate::survivors) const ALL: [SecretRace; 13] = [
        SecretRace::Goblin,
        SecretRace::Skeleton,
        SecretRace::Murloc,
        SecretRace::Gnoll,
        SecretRace::Kobold,
        SecretRace::Ogre,
        SecretRace::Satyr,
        SecretRace::Furbolg,
        SecretRace::Dragonspawn,
        SecretRace::Quillboar,
        SecretRace::Felguard,
        SecretRace::Centaur,
        SecretRace::Imp,
    ];

    /// The race whose [`SecretDef::key`] is `key`; `None` for a key the data no longer has (a
    /// saved character then falls back to its class's first normal look).
    #[allow(dead_code)] // Read by Create Character and the Secrets page (later tasks).
    pub(in crate::survivors) fn from_key(key: &str) -> Option<SecretRace> {
        Self::ALL.into_iter().find(|&r| def(r).key == key)
    }
}

/// One secret race's data.
#[allow(dead_code)] // Read by Create Character and the Secrets page (later tasks).
pub(in crate::survivors) struct SecretDef {
    /// Stable save key: `Progress::unlocked` and a character's look name it. Never rename.
    pub key: &'static str,
    pub name: &'static str,
    /// `ChrClasses.dbc` ids it may be, as [`crate::survivors::data::ClassDef::id`].
    pub classes: &'static [u8],
    /// `CreatureDisplayInfo` ids, its colour variants.
    pub displays: &'static [u32],
    /// The unit's whole render scale (`OBJECT_FIELD_SCALE_X`).
    pub scale: f32,
    /// The model has spell-cast animations; without, spells play the attack swing.
    pub casts: bool,
    /// Shown while locked; never the condition.
    pub riddle: &'static str,
    /// Shown only once unlocked.
    pub condition: &'static str,
}

// `ChrClasses.dbc` ids.
const WARRIOR: u8 = 1;
const PALADIN: u8 = 2;
const HUNTER: u8 = 3;
const ROGUE: u8 = 4;
const PRIEST: u8 = 5;
const SHAMAN: u8 = 7;
const MAGE: u8 = 8;
const WARLOCK: u8 = 9;
const DRUID: u8 = 11;

/// The spec's "Ashenvale/Felwood", which the game does not have: the nearest real zones are
/// Teldrassil (the night elves' corrupted forest) and Mount Hyjal (Felwood's southern
/// neighbour, where the satyrs and the Legion are).
pub(in crate::survivors) const SATYR_ZONES: [&str; 2] = ["Teldrassil", "Mount Hyjal"];
/// The spec's "Razorfen-area zones": Razorfen Kraul and Downs lie in the southern Barrens, and
/// the Razormane quillboar hold Durotar's and the Barrens' camps.
pub(in crate::survivors) const QUILLBOAR_ZONES: [&str; 2] = ["The Barrens", "Durotar"];
/// The Centaur's zone: the Kolkar centaurs of the riddle roam the Barrens.
pub(in crate::survivors) const CENTAUR_ZONE: &str = "The Barrens";

static DEFS: [SecretDef; 13] = [
    SecretDef {
        key: "goblin",
        name: "Goblin",
        classes: &[WARRIOR, ROGUE, HUNTER, MAGE, WARLOCK, PRIEST],
        // Two male, two female goblin NPCs (character model + baked atlas, no helmets).
        displays: &[7135, 6952, 16431, 7909],
        // 1.48 tall at 1.0: raised to the band's floor.
        scale: 1.1,
        casts: true,
        riddle: "Time is money, friend — and you've spent it all, from the forest to the fallen king.",
        condition: "clear the whole Normal pass",
    },
    SecretDef {
        key: "skeleton",
        name: "Skeleton",
        classes: &[WARRIOR, ROGUE, PALADIN],
        // Bone, dark teal, green.
        displays: &[158, 200, 201],
        scale: 1.0,
        casts: false,
        riddle: "The Light fails those who fall too often in its name.",
        condition: "die 100 times as a Paladin",
    },
    SecretDef {
        key: "murloc",
        name: "Murloc",
        classes: &[WARRIOR, HUNTER, SHAMAN],
        // Green, blue, orange, gray.
        displays: &[31, 188, 1755, 346],
        // 1.39 hunched: raised to the floor.
        scale: 1.2,
        casts: false,
        riddle: "Mrgllll… ten thousand brothers cry out from the shore.",
        condition: "kill 10,000 murlocs",
    },
    SecretDef {
        key: "gnoll",
        name: "Gnoll",
        classes: &[WARRIOR, HUNTER],
        // Riverpaw, Redridge, Shadowhide, Mosshide.
        displays: &[174, 269, 275, 355],
        scale: 1.0,
        casts: false,
        riddle: "The Riverpaw count their dead in thousands.",
        condition: "kill 5,000 gnolls",
    },
    SecretDef {
        key: "kobold",
        name: "Kobold",
        classes: &[WARRIOR, ROGUE],
        // Brown, albino.
        displays: &[26, 163],
        scale: 1.05,
        casts: false,
        riddle: "You no take candle… but you take everything else.",
        condition: "open 100 chests",
    },
    SecretDef {
        key: "ogre",
        name: "Ogre",
        classes: &[WARRIOR, MAGE],
        // Yellow, gray, blue, red.
        displays: &[415, 448, 597, 10707],
        // 2.96 tall: lowered to the ceiling.
        scale: 0.82,
        casts: true,
        riddle: "Heroes are small. Make them smaller — with an axe.",
        condition: "win a Heroic run as a Warrior",
    },
    SecretDef {
        key: "satyr",
        name: "Satyr",
        classes: &[ROGUE, WARLOCK],
        // Pink, crimson, violet, green.
        displays: &[467, 2013, 2875, 2879],
        scale: 0.88,
        casts: true,
        riddle: "Where the forest burns green, the corrupted wait for a hero.",
        condition: "clear Teldrassil and Mount Hyjal on Heroic",
    },
    SecretDef {
        key: "furbolg",
        name: "Furbolg",
        classes: &[WARRIOR, DRUID, SHAMAN],
        // Brown, blue, black, white.
        displays: &[145, 937, 1010, 6211],
        scale: 1.0,
        casts: true,
        riddle: "The wild spirit walks unharmed through the storm.",
        condition: "win a Druid run never below 50 % health",
    },
    SecretDef {
        key: "dragonspawn",
        name: "Dragonspawn",
        classes: &[WARRIOR, MAGE],
        // Black, blue, green, bronze.
        displays: &[143, 196, 181, 177],
        scale: 0.9,
        casts: true,
        riddle: "Only those who conquer the hardest trials earn the flight's respect.",
        condition: "clear Mythic",
    },
    SecretDef {
        key: "quillboar",
        name: "Quillboar",
        classes: &[WARRIOR, HUNTER, SHAMAN],
        // Razormane, gray, white, black.
        displays: &[924, 1218, 1963, 6078],
        scale: 1.0,
        casts: true,
        riddle: "The thorns remember who walked the Razorfen.",
        condition: "clear The Barrens and Durotar",
    },
    SecretDef {
        key: "felguard",
        name: "Felguard",
        classes: &[WARRIOR, WARLOCK],
        // Black, blue, yellow.
        displays: &[5047, 5048, 5049],
        // 3.87 tall: lowered to the ceiling.
        scale: 0.62,
        casts: true,
        riddle: "Command the Legion at its hardest, and it will kneel.",
        condition: "win a Mythic run as a Warlock",
    },
    SecretDef {
        key: "centaur",
        name: "Centaur",
        classes: &[WARRIOR, HUNTER],
        // Kolkar, gray, yellow warrior, black.
        displays: &[172, 9419, 9428, 9434],
        scale: 0.85,
        casts: false,
        riddle: "The Kolkar respect only those who never stop running.",
        condition: "win a run in The Barrens without ever standing still more than 3 seconds",
    },
    SecretDef {
        key: "imp",
        name: "Imp",
        classes: &[WARLOCK, MAGE],
        // Black (the warlock's imp), jade, red.
        displays: &[4449, 7552, 10811],
        scale: 0.75,
        casts: true,
        riddle: "Faster, master, faster! One minute is all the big one gets!",
        condition: "kill the final boss within 60 seconds of its arrival",
    },
];

pub(in crate::survivors) fn def(r: SecretRace) -> &'static SecretDef {
    &DEFS[r as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_race_has_classes_displays_and_a_riddle() {
        for r in SecretRace::ALL {
            let d = def(r);
            assert!(!d.classes.is_empty(), "{} has no class", d.name);
            assert!(!d.displays.is_empty(), "{} has no display", d.name);
            assert!(d.displays.len() <= 4, "{} has too many variants", d.name);
            assert!(!d.riddle.is_empty(), "{} has no riddle", d.name);
            assert!(!d.condition.is_empty(), "{} has no condition", d.name);
            assert!((0.5..=1.5).contains(&d.scale), "{} scale", d.name);
            for c in d.classes {
                assert!(
                    crate::survivors::data::CLASSES.iter().any(|k| k.id == *c),
                    "{} offers unknown class {c}",
                    d.name
                );
            }
        }
        assert!(def(SecretRace::Skeleton).classes.contains(&PALADIN));
        // Paladin stays Human and Dwarf, plus the Skeleton as the one dark exception.
        for r in SecretRace::ALL {
            if r != SecretRace::Skeleton {
                assert!(!def(r).classes.contains(&PALADIN), "{:?} offers Paladin", r);
            }
        }
    }

    #[test]
    fn keys_are_unique_and_round_trip() {
        for r in SecretRace::ALL {
            assert_eq!(SecretRace::from_key(def(r).key), Some(r));
            assert_eq!(def(r).key, def(r).key.to_lowercase());
        }
        assert_eq!(SecretRace::from_key("blood_elf"), None);
    }

    #[test]
    fn the_condition_zones_exist() {
        let maps = crate::survivors::data::MAPS;
        for z in SATYR_ZONES.iter().chain(&QUILLBOAR_ZONES).chain([&CENTAUR_ZONE]) {
            assert!(maps.iter().any(|m| m.name == *z), "{z} is not a zone");
        }
    }
}
