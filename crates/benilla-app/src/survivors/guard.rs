//! The melee classes' own survival: the Warrior blocks with his shield, the Rogue dodges. Only a
//! direct blow (an enemy's swing or missile) can be turned; ground areas and auras cannot.

/// A class's guard against direct blows. These are starting values; the balance pass tunes them
/// from the run logs.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Guard {
    None,
    /// `chance` to block, and the share of the blow a block lets through.
    Block {
        chance: f32,
        keep: f32,
    },
    /// `chance` to dodge the blow whole.
    Dodge {
        chance: f32,
    },
}

/// How a blow reaches the hero.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Hit {
    /// A swing or a missile: it can be blocked or dodged.
    Direct,
    /// A ground area, an aura or a damage over time: it cannot.
    #[allow(dead_code)]
    Area,
}

/// What the guard did to a blow.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Guarded {
    Block,
    Dodge,
}

impl Guarded {
    /// The floating word over the hero.
    pub(super) fn word(self) -> &'static str {
        match self {
            Self::Block => "Block",
            Self::Dodge => "Dodge",
        }
    }

    /// The swing's `victimState`, which picks the sound: 5 the shield's clang, 2 the whoosh.
    pub(super) fn victim_state(self) -> u32 {
        match self {
            Self::Block => 5,
            Self::Dodge => 2,
        }
    }
}

/// The damage `dmg` leaves after the guard, given a uniform `roll` in [0, 1).
pub(super) fn mitigate(guard: Guard, hit: Hit, dmg: f32, roll: f32) -> (f32, Option<Guarded>) {
    if hit != Hit::Direct {
        return (dmg, None);
    }
    match guard {
        Guard::Block { chance, keep } if roll < chance => (dmg * keep, Some(Guarded::Block)),
        Guard::Dodge { chance } if roll < chance => (0.0, Some(Guarded::Dodge)),
        _ => (dmg, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: Guard = Guard::Block {
        chance: 0.25,
        keep: 0.35,
    };
    const DODGE: Guard = Guard::Dodge { chance: 0.18 };

    #[test]
    fn guard_only_mitigates_direct_hits() {
        for g in [BLOCK, DODGE] {
            assert_eq!(mitigate(g, Hit::Area, 40.0, 0.0), (40.0, None));
            assert_ne!(mitigate(g, Hit::Direct, 40.0, 0.0).1, None);
        }
    }

    #[test]
    fn block_keeps_share() {
        let (dmg, g) = mitigate(BLOCK, Hit::Direct, 40.0, 0.1);
        assert_eq!(g, Some(Guarded::Block));
        assert!((dmg - 14.0).abs() < 1e-4);
        // Past the chance: the whole blow.
        assert_eq!(mitigate(BLOCK, Hit::Direct, 40.0, 0.25), (40.0, None));
    }

    #[test]
    fn dodge_takes_nothing() {
        assert_eq!(
            mitigate(DODGE, Hit::Direct, 40.0, 0.17),
            (0.0, Some(Guarded::Dodge))
        );
        assert_eq!(mitigate(DODGE, Hit::Direct, 40.0, 0.18), (40.0, None));
    }

    #[test]
    fn none_guard_is_identity() {
        for roll in [0.0, 0.5, 0.99] {
            for hit in [Hit::Direct, Hit::Area] {
                assert_eq!(mitigate(Guard::None, hit, 33.0, roll), (33.0, None));
            }
        }
    }
}
