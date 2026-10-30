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
- **Limit Break in Endless**, as in Vampire Survivors: once every ability is at its normal maximum,
  each further level raises one past its cap (damage, area, projectiles, cooldown) in small
  steps with no ceiling, so the build keeps pace with the enemies' unbounded scaling.
- **No card screen once there is nothing left to pick:** a maxed build's level-ups apply their
  Limit Break (or, outside Endless, a small heal) without stopping play. Seen on the tablet: a
  level-112 hero with every ability at rank 8 paused for a card every few seconds.

## Unlock order

- **Zones unlock in order**, easiest first (the menu's list order); each opens when the one before
  is completed on Normal. A locked zone shows "???" and "Locked": what comes next is a surprise.
- **On each zone, Heroic opens after Normal, Mythic after Heroic.** (Built.)
- Every completion is still 1 talent point: Normal gives 16 + Molten Core = 17, Heroic 17, Mythic
  17, so a full 51-point character has cleared everything on Mythic.

## Player character model and on-screen keyboard

One piece of work under three wishes: WoW's own character customisation, typed names, and the tier
armour and iconic weapons below.

- **The hero becomes a player character model.** Today it is a class-trainer NPC model whose looks
  and armour are baked in. A player model is built, as in WoW, from race and gender plus skin
  colour, face, hair style, hair colour and facial hair (`CharSections.dbc` and friends), and is
  dressed per slot from items. The engine is a full 1.12 client and dresses players already; how
  much the survivors hero has to change is the first thing to check.
- **Create screen:** WoW's own customisation arrows (skin, face, hair, hair colour, facial hair)
  under the race and class tower.
- **On-screen keyboard** in the glue style: Android's NativeActivity passes no text from the system
  keyboard, so names are typed on a WoW-styled A–Z keyboard (the generated name stays as a
  default). The same keyboard enters the arcade records' initials.

## Tier armour: cosmetic rewards per difficulty

- **Every zone on Normal → Tier 1** (Molten Core sets: Arcanist, Lawbringer, Might, …),
  **every zone on Heroic → Tier 2** (Blackwing Lair: Netherwind, Judgement, Wrath, …),
  **every zone on Mythic → Tier 3** (Naxxramas: Frostfire, Redemption, Dreadnaught, …).
- Selectable on the character screen once unlocked; looks only, no stats.
- Open question: the hero is drawn today as a class-trainer NPC model with its armour baked in.
  Showing tier sets means dressing it as a player character, piece by piece from the game data.
  The engine dresses players already; how much the survivors hero needs to change is to be
  checked before this is scheduled.

## Iconic class weapons: a reward per difficulty

- **Every zone on Normal → an iconic blue weapon** for the class, **every zone on Heroic → an
  iconic epic**, **every zone on Mythic → a legendary**. Unlocked on the character, alongside the
  tier armour.
- Legendaries, matching Vanilla's own class ties: **Atiesh, Greatstaff of the Guardian** for Mage,
  Priest, Warlock and Druid (Vanilla had a version for each of the four); **Rhok'delar, Longbow of
  the Ancient Keepers** for Hunter; **Thunderfury, Blessed Blade of the Windseeker** for Warrior and
  Rogue; **Sulfuras, Hand of Ragnaros** for Paladin and Shaman.
- The blue and epic weapon per class: to choose together.
- Open question: looks only (like the tier armour), or real gear with stats feeding the loot
  system's Weapon slot.

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

- Saved characters, WoW-style select screen: name, race, class, gender. **At most 8.** Create and
  **Delete**. A NativeActivity has no on-screen keyboard, so names are generated WoW-style with a
  re-roll button, and Delete confirms with a second tap instead of a typed name. (Built.)
- Each character keeps: talents, completed map/difficulty list, equipped gear, bags.
- One file per character in the save folder (`benilla-config/characters/`).

## Field pickups (in a run)

Vampire Survivors-style drops on the ground, **auto-looted** by walking over them (the magnet
radius applies). Few of them: a pickup should feel like a small win, not litter. Rates and
strength are tuned per difficulty and scale with the run's progress, like enemy health already
does (`data::progress`).

- **Healing Potion** (WoW's red potion icon): heals a share of max HP. Rare drop from normal
  enemies, more likely from elites; fewer on Mythic.
- **Small chest** from elites and regular bosses: an immediate reward inside the run (a free
  level-up card, or an upgrade to an owned spell), as Vampire Survivors' chests do. Distinct from
  the **boss loot chest** (gear, kept only on completion).
- Candidates for later: a gold-coin drop if a currency is added, a magnet pickup that pulls every
  gem on screen, a short-lived damage buff (Elixir of the Mongoose / Giants).

## Records: an arcade high-score table per map

- Every map keeps a **top 10** like an old arcade cabinet: kills, time survived, class, difficulty,
  date. Saved locally.
- When a run (won or lost) makes the table, the game asks for **three initials** the arcade way:
  up/down arrows per letter, then OK. Works without a keyboard.
- A **Records** page from the menu shows each map's table; the current character's entries
  highlighted.

## Combat stats: damage meter

To compare builds and see which ability carries a run:

- Every hit is attributed to the ability that dealt it (today only the run total is kept).
- **Results screen:** a Details!/Recount-style list, each ability with its icon, total damage,
  share of the run, and damage per second while owned.
- **In the run:** an optional small live meter (top N abilities), toggled in settings.
- Later: keep each character's best run per map for comparison.

## Settings

In the pause menu, all saved between sessions:

- **Graphics: High / Medium / Low** (built, Android only, not yet tested on a device): the world's
  render resolution (1920 / 1440 / 1080 px long side; the UI stays sharp), view distance, shadows,
  spell effects, glow, weather.
- **Sound, as WoW's own panel:** Master, Sound effects (combat), Music and Ambience volume, each
  with its on/off switch. The engine already has all of these as 1.12 CVars (`MasterVolume`,
  `SoundVolume`, `MusicVolume`, `AmbienceVolume`, `EnableMusic`, …); only the screen is new.
- **Show damage meter** in the run.
- Anti-aliasing (`gxMultisample`, applies after a restart) once it is checked on the tablet.

## Talents

- **All 9 classes, 3 trees each**, WoW names. 5 points in a tree open its next tier; the last
  tier is a capstone. Reset button.
- Most talents raise the game's stats; **signature talents** change a spell (Ignite, Shatter,
  Deep Wounds).
- **Healing trees become holy/nature damage plus self-sustain**, keeping the WoW name and theme:
  Holy Paladin (Consecration, Holy Nova, lifesteal), Holy/Discipline Priest (Holy Nova, Smite,
  shields), Restoration Druid (Treants, Thorns, regeneration), Restoration Shaman (totems that heal
  the hero and strike enemies).

## Later: an X Hero Siege map and local co-op

A new mode after the progression is done, after Warcraft III's X Hero Siege: heroes hold a base
against waves marching down lanes, with shops, upgrades and boss waves. Its own map and rules.

- **Co-op over the local network only** (same Wi-Fi), no internet servers. One device hosts and
  runs the simulation; the others send their inputs and draw the state it sends back. The game
  already runs everything locally, which suits a host-authoritative model; the network layer is
  new work, and so is the mode itself.
- To scope once the progression and gear phases are in.

## What exists today

Much of the loop is already in the game and gets reshaped rather than written:

- A win condition: survive `VICTORY_SECS` (15 min), "Victory!" on the results screen, and a
  victory line per zone ("Elwynn Forest is safe once more.").
- Six bosses per zone from its own roster, one every `BOSS_EVERY_SECS` (2:15).
- Enemy count, health and damage scale with the run's progress (`data::progress`).
- The total damage of a run (`Run::damage_done`), shown on the results screen.
- A pick screen built from the 1.12 character create/select glue art (`survivors/menu.rs`): race,
  gender and class on the left, zones on the right. It becomes the **Create** screen; character
  select is a second screen in the same style.

## Build order

Each phase ends with a build that plays on the tablet. Nothing is built for a phase until the
plan for it is agreed.

After the gameplay phases: a **balance pass**, driving many runs with the built-in auto-player
(`survivors/auto.rs`) across classes, zones and difficulties to catch errors and scaling problems
(win rates, time-to-kill, deaths by minute), then tuning, reviewed against Vampire Survivors'
design principles.

1. **Settings and meter:** the Sound panel next to Graphics; the damage meter (results screen
   first, then the live one). Small, and it makes every later phase easier to tune.
2. **Characters and completion:** create (the existing pick screen) / select / delete, max 8;
   difficulty choice with Normal 10 / Heroic 15 / Mythic 20 minute timers; a final boss at the
   timer, the map completed when it dies; completions recorded per character.
3. **Records page:** the arcade top 10 per map with initials entry.
4. **Field pickups:** potions and small chests, auto-loot, tuned per difficulty.
5. **Talents:** the tree screen and points from completions; all 9 classes.
6. **Gear and boss chests:** rarities, 5 slots, bags, keep-on-completion.
7. **Difficulty depth:** Heroic/Mythic affixes and boss mechanics, Endless, raids.
8. **Legendaries:** Thunderfury and Sulfuras first.
