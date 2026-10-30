# Progression design: maps, talents, gear

Decided with the player, 2026-10-30. Nothing here is built yet; the build order is at the end.

## The loop

Pick a saved character → pick a map and difficulty → survive the timer → kill the final boss →
**map completed** (1 talent point, loot kept, Endless unlocked on that map/difficulty) → spend
points and equip gear → next map.

Every run starts at **level 1** with the level-up cards as today. Power between runs comes only
from **talents** and **gear**; there are no character levels.

## Maps and difficulty

- **51 completions = 51 talent points**, the Vanilla maximum:
  - the **16 zones** of today × **Normal / Heroic / Mythic** = 48;
  - **3 raids**: Molten Core, Blackwing Lair, Naxxramas = 3.
- **Raid unlocks (tiered):** Molten Core once all 16 zones are done on Normal, Blackwing Lair on
  Heroic, Naxxramas on Mythic. Naxxramas is the 51st point.
- **Run length scales with difficulty:** Normal 10 min, Heroic 15, Mythic 20. Regular bosses keep
  arriving on the existing `BOSS_EVERY_SECS` pace; at the timer the **final boss** spawns.
- **Each zone has a recommended level** that sets enemy strength.
- **Heroic and Mythic** add, on top of the zone level:
  - stronger enemies (HP, damage, denser waves);
  - **elite affixes** on some enemies: shielded, fast, explode on death, enrage;
  - **better loot** odds; legendaries drop on Heroic and up only;
  - **boss mechanics** on final bosses (knockbacks, adds, fire zones).
- **Endless:** a completed map/difficulty can be replayed with no end; enemies keep scaling with
  time. No new talent point.

## Loot

- **Hardcore:** loot found in a run is kept **only if the map is completed**. Dying loses it — in
  Endless too, with no way to extract.
- Normal enemies drop XP gems only. **Bosses drop a chest** with one rolled item.
- **Rarities:** Poor (grey), Common (white), Uncommon (green), Rare (blue), Epic (purple), plus
  **Legendary** (orange) from late bosses on Heroic+, rare, with a hidden pity counter.
- More rarity = more and stronger stats from the game's own: Might, MaxHp, Speed, Regen, Haste,
  Area, Magnet, Growth, Armor, Crit. Epics add a special modifier (e.g. spells chain to one extra
  target).
- **Real WoW names, icons and models**, our own stat table: the 1.12 client data carries no item
  stats (the server sent those). WoW stats map onto the game's: Strength/Agility → Might,
  Stamina → MaxHp, Spirit → Regen.
- **Legendaries with unique effects:** Thunderfury (chain lightning across 5 enemies + slow),
  Sulfuras (periodic fire eruptions around the hero); later Atiesh, Ashkandi, Benediction.

## Gear

**5 slots:** Weapon, Head, Chest, Ring, Trinket. The equipped weapon can show in the hero's hand.

## Characters

- Saved characters, WoW-style select screen: name, race, class, gender. Create and **Delete**
  (confirm by typing the name).
- Each character keeps: talents, completed map/difficulty list, equipped gear, bags.
- One file per character in the save folder (`benilla-config/characters/`).

## Talents

- **All 9 classes, 3 trees each**, WoW names. 5 points in a tree open its next tier; the last
  tier is a capstone. Reset button.
- Most talents raise the game's stats; **signature talents** change a spell (Ignite, Shatter,
  Deep Wounds).
- **Healing trees become holy/nature damage plus self-sustain**, keeping the WoW name and theme:
  Holy Paladin (Consecration, Holy Nova, lifesteal), Holy/Discipline Priest (Holy Nova, Smite,
  shields), Restoration Druid (Treants, Thorns, regeneration), Restoration Shaman (totems that heal
  the hero and strike enemies).

## Build order

Each phase ends with a build that plays on the tablet.

1. **Characters:** create / select / delete, saved per character; map completion with a final
   boss and difficulty timers; completion records.
2. **Talents:** the tree screen and points from completions; all 9 classes.
3. **Gear and boss chests:** rarities, 5 slots, bags, keep-on-completion.
4. **Difficulty depth:** Heroic/Mythic affixes and boss mechanics, Endless, raids.
5. **Legendaries:** Thunderfury and Sulfuras first.
