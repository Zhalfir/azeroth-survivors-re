# Balance: targets, method, rounds

The working log of tuning Azeroth Survivors on Android. Every change names the evidence behind it.

## Targets

Each difficulty is a pass through the whole game (`PROGRESSION.md`):

| Difficulty | Run | Who should clear it |
|---|---|---|
| **Normal** | 10 min | everyone who plays reasonably; deaths come from mistakes, not from the curve |
| **Heroic** | 15 min | about half of the players who cleared Normal, with Normal's ~17 talent points and T1 |
| **Mythic** | 20 min | an achievement: a strong build, ~34 points and T2, and good play; most runs fail |

The power that carries a character up a difficulty: talents (up to 51), the tier set's stats, and,
once built, gear from boss chests. If talents and tiers alone cannot carry the step, gear fills it.

## Design principles (a review against Vampire Survivors)

What Vampire Survivors gets right, and what this game should keep:

1. **The first minute is a promise, not a test.** Enemies trickle in; the player learns the
   spells. An idle hero survives tens of seconds, not ten.
2. **Power outgrows the horde, then the horde catches up.** Levels come fast early (a card every
   few seconds), the build snowballs around minutes 3–6, and the last third is the test.
3. **Death comes from being caught, not from attrition.** Every scratch should not stay for the
   whole run; being surrounded should be the danger, and escaping it should be possible.
4. **Bosses are milestones, not walls.** A regular boss is a damage check that drops a reward; the
   final boss is the climax, beatable by a build that survived to it.
5. **A maxed build never stalls the game.** No card screens with nothing to choose.
6. **Readability is part of balance.** The player must see the hero, the threats and the gems.

## Method

- The auto-player (`survivors/auto.rs`) plays unattended on the tablet. It scores sixteen
  headings (enemy pressure, gems, the arena edge, inertia) and picks cards by value, a careful but
  unremarkable player. It is a lower bound for a person.
- Every run writes a full combat log (`survivors/combatlog.rs`): per second, damage per spell,
  damage taken per enemy type, kills, spawns, XP, health, level, frames per second; every boss
  event; the killing blow.
- `scripts/balance/run_batch.sh <round> <runs> <zone> <difficulty> <class…>` plays N runs per
  class on the tablet (over `adb`) and pulls the logs into `$BALANCE_DIR/<round>`;
  `scripts/balance/analyze.py <dir…>` summarises survival, level pace, damage share, killers and
  frame rate per class.

## Round 0 — the old auto-player (2026-10-30)

One Warrior run, Elwynn Forest, Normal: died at 3:48, level 9 (level 5 at 0:59, never 10). The
old bot walked to the nearest gem and picked cards at random; the slow levels were mostly the bot.
The bot was rewritten before going on.

## Round 1 — the new auto-player, one run per class, Elwynn Forest, Normal

| Class | Survived | Level | Kills | Lv 5 / 10 | Top damage | Killed by | FPS median / p10 |
|---|---|---|---|---|---|---|---|
| Warrior | 8:18 | 57 | 2,616 | 0:19 / 0:49 | Execute 38%, Thunder Clap 33% | Rockhide Boar, 48 dmg | 42 / 20 |
| Paladin | 8:08 | 103 | 6,950 | 0:18 / 0:49 | Consecration 57%, Holy Wrath 23% | Defias Bandit, 54 dmg | 30 / 12 |
| Hunter | 0:52 | 9 | 132 | 0:19 / — | Volley 45%, Arcane Shot 27% | Young Wolf | 66 / 24 |

Findings:

1. **Chip damage never healed.** The Hunter fell to 86/135 by 0:20 and sat at 47 HP for fifteen
   seconds: no regeneration at all, potions at 1.5% a kill. (Principle 3.)
2. **Fortitude raised the pool, not the health.** Max 135 → 189, current stayed at 47.
3. **The first ring came at 0:45 on Normal** (rings fall half way between bosses, and Normal's
   bosses come every 90 s): thirty-odd foes closed on a level-8 hero with 47 HP. The commonest
   early death. (Principle 1.)
4. **Late, horde mobs hit for ~50 against a pool of ~320**; runs that were winning (7,000 kills,
   level 103) ended in a couple of seconds once caught. Health only grew through Fortitude.
5. **Frame rate** falls to 12–20 late, and dips to ~11 at 3:00 with only 76 enemies: new spell
   visuals compiling their pipelines on first use, not the crowd. A performance pass follows.
6. **A maxed build** (the stress test, level 112+) stopped for a one-card screen every few seconds.

Changes (built 2026-10-31, to be measured in round 2):

- Every hero mends 0.4 HP/s, plus 1.5% of its pool a second after 4 s without a blow.
- A max-health card heals what it adds.
- Rings: 12 + 76 × progress enemies (first ring ~18 on Normal, was 32; the same 88 at the end).
- Horde damage scaling: ×1.6 at the bell (was ×1.9).
- +1.5 max health per hero level, before Fortitude.
- Potions heal 25% of the pool (was a flat 30), and drop at 1.5% / 1.1% / 0.7% by difficulty.
- Regular bosses drop a chest: a free card.
- A maxed build banks its levels as heals without a card screen.
- Final boss (earlier, 2026-10-30): no time scaling on its damage, ×0.75, a blow every 1.4 s.

## Round 2 — regeneration and gentler first rings (stopped after 4 classes)

| Class | Survived (round 1 → 2) | Level | Killed by |
|---|---|---|---|
| Hunter | 0:52 → 1:47 | 16 | Defias Thug (constant contact, ~10 damage/s against 0.4 regeneration) |
| Druid | 0:52 → 5:18 | 26 | Forest Spider |
| Priest | 0:51 → **11:19** | 91 | Defias Bandit, after the bell |
| Shaman | 0:50 → 1:45 | 13 | Mangy Wolf (single-target kit, 70-145 damage/s against a crowd of 40-50) |

Findings:

1. **A ring after the bell** spawned 93 enemies on top of 220 alive (rings ignored the cap); eight
   Defias Bandit blows took the Priest from a full 510 to 43 in a second.
2. **The final boss**, slower than the hero, stood 23-36 yd off at 100% for 40 s: it could be
   outrun for the whole fight, and spells that pick the nearest target never reached it.
3. The early crowd still drowned single-target starters.

Changes: no rings once the final boss stands, rings at most 70 and within the cap + 40; the
final boss at least 6.3 pace and ×2 health (was ×3); the crowd eased in (10 + 225·p^1.3: ~29 alive
at 1:30 on Normal, was 48; the same cap at the end); the auto-player faces the final boss above
40% health.

## Round 3 (stopped after 3 classes)

| Class | Survived (r1 → r2 → r3) | Level | Bosses | Killed by |
|---|---|---|---|---|
| Hunter | 0:52 → 1:47 → **8:51** | 108 | all five regular bosses | a dozen late blows, 588 → 0 in 2 s |
| Druid | 0:52 → 5:18 → 3:48 | 29 | 2 | a ring at 3:45, 124 → 13 in 3 s |
| Shaman | 0:50 → 1:45 → 2:22 | 16 | 1 | the ring at 2:15 |

Findings: **rings are the weaker classes' killer** (a closing wall a single-target kit cannot
break), and **late, being caught once is death** even at 590 health (blows of 40-60, ×12). At
16-18 fps, which is where the late game runs on the tablet, reacting in time is harder still: the
frame rate is part of the balance.

Changes: ring enemies spawn at half health; horde damage ×1.35 at the bell on Normal (Heroic and
Mythic keep their multipliers on top).

## Performance notes (2026-10-31)

- **Pipeline cache** (`third_party/bevy_render`): upstream Bevy never gives wgpu a pipeline cache,
  so the tablet recompiled every pipeline on every launch; the warm pass timed out at 10 s with
  pipelines pending, and the rest compiled live in combat (a run dipped to 11 fps at 3:00 with 76
  alive). With the cache: 1.3 MB, 1,601 pipelines; the warm pass drains all 2,061 variants in
  ~6 s, nothing compiles live. The warm pass is now limited by its own pacing (24 a frame), not by
  compiling: a faster reveal on a warm cache would shorten loading.
- **Late game** runs at 16-30 fps median (Paladin's Consecration and Hunter's Volley the
  heaviest): 150-220 enemies and their spell visuals. Next: measure the Medium and Low presets,
  and profile what costs most per enemy.
- **The first card screen** stalled (2 fps for a second): the survivors UI's pipelines (cards,
  gradients, shadows) are not in the warm pass. To check with the cache in place.

## Round 4 (stopped after 4 classes)

Druid 3:48 → 8:18 (four bosses; late horde blows now ~34, were 40-60). The squishy classes still
fell to the **first boss and its pack at 1:30** on Normal: Hunter 1:46 (150 → 3 health in six
seconds), Priest 1:59. Shaman 3:10: little area, one slow on a 3 s cooldown, kills slow, levels slow
(level 5 at 0:37, others 0:18-0:25), and spirals.

Changes: the first regular boss at 20% of the run (2:00 on Normal), then every 15% (3:30, 5:00,
6:30, 8:00); rings shifted with it (first ring 1:15). Shaman: Frost Shock slows two targets,
Chain Lightning jumps to four, Lightning Bolt 17 (was 15).

Note: one run per class is noisy (the Hunter lived 8:51 in round 3 and 1:46 in round 4 with
nearly the same build); trends across rounds are what to read.

## Round 5 — the first Normal victory

| Class | Round 1 | Round 5 |
|---|---|---|
| Priest | 0:51 | **Victory, 12:48** (final boss 70k, ~2:50 fight, the hero down to 327/450 and back) |
| Rogue | 5:20 | reached the final boss; alive at 16:10, the boss only 21% down |
| Druid | 0:52 | reached the final boss; died at 12:35, level 159 |
| Warlock | 6:24 | reached the final boss; died at 10:17 |
| Hunter | 0:52 | 6:22, three bosses |
| Shaman | 0:50 | 6:11, two bosses |

Findings:

1. **The horde walls the final boss off.** Topped up to 220 through the fight, it kept a melee
   Rogue from the boss for six minutes; ranged spells that pick the nearest target hit the wall.
2. **The 6:00 tier** (Kobold Miners 25 a blow, Murloc Streamrunners 34) met heroes of 210-290.
3. **Frame-time hitches** (7-11 fps for single seconds, with only ~120 alive) coincided with the
   blows that killed the Hunter: hitches, not a steady low rate. Pipelines no longer compile live,
   so something else: first loads of new creature models (the Murlocs arrived then), or UI
   rebuilds (the live meter rebuilt its texts twice a second).

Changes: while the final boss stands, the horde refills only to a third of the cap; health per
level 2.5 (was 1.5); the live meter updates in place.

## Round 6 — Heroic, 17 talent points + T1 (Elwynn Forest)

The power a full Normal clear gives: on paper ×1.51 damage and ×1.10 health (+31% Might, +10%
haste, +5% crit, +10% health), against Heroic's ×1.7 enemy health, ×1.4 damage, ×1.15 crowd and
a 15-minute run.

| Class | Result | Level | Final boss |
|---|---|---|---|
| Warrior | **Victory, 17:23** | 154 | 119k health, 2:23 |
| Mage | **Victory, 17:47** | 206 | 2:47 |
| Priest | **Victory, 17:50** | 137 | 2:50 |
| Hunter | died at 13:11, four bosses | 140 | — |

Three of four. The auto-player plays below a decent person, so Heroic's *first* zone sits on the
easy side of "about half of Normal's clearers". The zones' rosters climb with their level ranges
(Elwynn 1-10 … Naxxramas 53-61), so a later zone is the real read; if the middle of the Heroic
pass is still this kind, Heroic's multipliers go up.
