# Secret races — design

Status: draft for the user's approval (brainstormed 2026-10-31). Secret *weapons* (Ashbringer and friends) are a follow-up brainstorm, noted at the end.

## Intent

Give the survivors mode Vampire Survivors-style secrets: hidden playable races you discover by doing unusual things, with a page that tells you secrets exist without saying what they are. The pleasure is the hunt and the surprise ("I'm a Murloc now"), not power.

## Goals

- **13 secret races**, each unlocked by a hidden condition and announced when it happens.
- **Look only.** A secret race plays exactly like its class; no new passives or spells (the user's choice).
- **Classic-WoW class limits per race**, chosen from lore and from what the model can animate.
- **A Secrets page** that shows the count, one silhouette per secret and a cryptic riddle per locked one.

## Non-goals

- New mechanics, stats or abilities per race.
- Customisation the 1.12 data does not have (faces or hair for creatures).
- Blood Elf or Draenei: they are not in the 1.12 data.

## Anti-goals (a failure even if it works)

- A secret spelled out before it is found: no condition text on locked entries, only the riddle.
- A secret race that looks broken: T-posing, a missing attack animation, weapons floating off the hands.
- An unlock lost because a character was deleted. Unlocks are account-wide.

## Evidence from the data (read 2026-10-31 from the user's 1.12 client)

- **`ChrRaces.dbc` has 9 races.** Race 9 is the **Goblin**, flagged not playable. Its `Character\Goblin\{Male,Female}` models carry both hand attachments and every animation the hero uses: Run, Attack 1H/2H/Unarmed, SpellCastDirected/Omni (female) and Death.
- **Creature models with hand attachments**, so they can hold weapons:
  - Skeleton, Murloc, Kobold, Gnoll, Ogre, Furbolg, Satyr, Dragonspawn, Quillboar, Felguard, Imp, Centaur, the creature Troll.
  - Of these, Ogre, Furbolg, Satyr, Felguard, Dragonspawn, Quillboar and Imp also have spell-cast animations.
- Harpy and Infernal have no hands. Ghoul has unarmed attacks only.

## The races

| Race | Classes | Look | Unlock (hidden) | Riddle |
|---|---|---|---|---|
| Goblin | Warrior, Rogue, Hunter, Mage, Warlock, Priest | character model; full customisation if the data has it, else NPC goblin looks | clear the whole Normal pass | "Time is money, friend — and you've spent it all, from the forest to the fallen king." |
| Skeleton | Warrior, Rogue, **Paladin** | creature, colour variants | die 100 times as a Paladin | "The Light fails those who fall too often in its name." |
| Murloc | Warrior, Hunter, Shaman | creature, colour variants | kill 10,000 murlocs | "Mrgllll… ten thousand brothers cry out from the shore." |
| Gnoll | Warrior, Hunter | creature | kill 5,000 gnolls | "The Riverpaw count their dead in thousands." |
| Kobold | Warrior, Rogue | creature | open 100 chests | "You no take candle… but you take everything else." |
| Ogre | Warrior, Mage | creature | win a Heroic run as a Warrior | "Heroes are small. Make them smaller — with an axe." |
| Satyr | Rogue, Warlock | creature | clear Teldrassil and Mount Hyjal on Heroic | "Where the forest burns green, the corrupted wait for a hero." |
| Furbolg | Warrior, Druid, Shaman | creature | win a Druid run never below 50 % health | "The wild spirit walks unharmed through the storm." |
| Dragonspawn | Warrior, Mage | creature | clear Mythic | "Only those who conquer the hardest trials earn the flight's respect." |
| Quillboar | Warrior, Hunter, Shaman | creature | clear The Barrens and Durotar | "The thorns remember who walked the Razorfen." |
| Felguard | Warrior, Warlock | creature | win a Mythic run as a Warlock | "Command the Legion at its hardest, and it will kneel." |
| Centaur | Warrior, Hunter | creature | win a run in The Barrens without ever standing still more than 3 seconds | "The Kolkar respect only those who never stop running." |
| Imp | Warlock, Mage | creature | kill the final boss within 60 seconds of its arrival | "Faster, master, faster! One minute is all the big one gets!" |

Paladin stays limited to Human and Dwarf, plus the Skeleton as the one dark exception (user's choice: "Skeleton paladin is cool"). Creatures without cast animations play the attack swing for spells.

## The Secrets page

- On the main menu beside Talents and Records.
- Header: "N / 13 secrets discovered".
- A grid of 13 slots:
  - **Locked:** a dark silhouette of the model (its shape is the only clue) and its riddle.
  - **Unlocked:** the model turning slowly, its name, its classes, and the condition, shown only now.
- **The unlock moment**, mid-run or at the end: a gold banner reading "A secret has been revealed… <Race>", the WoW achievement sound, and the race then appears in Create Character.

## Approaches considered

- **A. A race is a list of display IDs (chosen).** The hero is already spawned like an enemy, as a unit with a display ID and weapon slots (`survivors/hero.rs` `spawn_hero` → `units.rs` `spawn_unit`). A secret race is then a data entry: name, the display IDs of its colour variants (which serve as its skins), its allowed classes, its condition and its riddle. Weapons attach through the existing path. The Goblin uses race 9's character model.
- **B. A morph at run start (Noggenfogger-style).** Spawn as the normal race, then transform. It only adds a transform effect and complicates everything else. Rejected.
- **C. Full customisation for creatures.** The data does not exist. Impossible.

## Persistence

- Account-wide unlocks and counters (deaths per class, kills per creature family, chests opened, best run time, Endless time per zone, clears), stored next to the characters' data (`survivors/characters.rs`).
- Counters are bumped from the combat log's existing events (kills, deaths, chests, run end) so nothing new is needed in combat.

## Left to the builder

- The exact display IDs per race and variant (from `CreatureDisplayInfo.dbc`), and a sanity check that each one animates, holds weapons and is the right size beside the 8 races.
- Whether the Goblin has `CharSections` rows (faces/skins). If it doesn't, NPC looks.
- The silhouette rendering: the model in the existing portrait booth with a black material, or a pre-rendered dark image.
- How "Razorfen-area zones" and "Ashenvale/Felwood" map onto the game's zone list. Adjust the condition to the zones that exist.

## Follow-up: secret weapons (next brainstorm)

The 1.12 data holds NPC-only weapons, most notably the **pure Ashbringer** (display IDs 23875, 24602, 29991), never a player item, and an unused blue variant. They fit as a hidden tier beside the planned iconic weapons, on the same Secrets page. This waits for the gear phase (boss chests, 5 slots).
