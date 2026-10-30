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
