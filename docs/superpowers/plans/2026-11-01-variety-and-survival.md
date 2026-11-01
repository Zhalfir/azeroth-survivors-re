# Variety and Survival — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make runs survivable for melee classes and different from one another, Vampire Survivors-style, using Vanilla 1.12 class fantasy.

**Architecture:** Everything lives in `crates/benilla-app/src/survivors/`. New mechanics go in small new modules (`potion.rs`, `guard.rs`, `soulstone.rs`, `evolve.rs`). Content stays as data in `data.rs` (abilities, pools, evolutions). The HUD gets one right-hand action cluster that the stick-side option mirrors.

**Tech Stack:** Rust, Bevy 0.18.1, winit 0.30.13 (Android soft keyboard via `Window.ime_enabled`), the user's 1.12 client data (spells, `Talent.dbc`).

**Spec:** the user's tablet feedback of 2026-11-01, recorded in the "Decisions" section below. Phase C (ultimates) gets its own design spec before any code.

## Decisions (user, 2026-11-01)

- Talents stay **generic** (same stat rows for every class) for easy balancing. Only the icons follow class fantasy.
- **Ultimates** are late-game: they unlock with the **31-point capstone**, and the talent pace does not change. Separate spec (Phase C).
- Health drops stay **instant pickups**, drawn as a **red globe**.
- **On-demand potion**: heals a share of max HP, has a cooldown, and is weaker on harder difficulties. It is for "oh shit" moments, not a rotation.
- **Melee survivability**: the Warrior **blocks** (shield), the Rogue **dodges**.
- **Charge** should be as useful as Blink: no target required.
- **Records** show names of at least 6 characters: the full character name.
- **Soft keyboard** for typing character names.
- **Stick on the right** as an option, for smaller screens.
- **Variety**: more Vanilla skills per class, **Reroll / Skip / Banish**, **evolutions**.
- **Revive**: a self-revive called **Soulstone**, a **rare boss drop**.

## Global Constraints

- Bevy 0.18.1. Survivors code is ours; engine crates (`benilla-*`) only where a task says so.
- Build discipline (8 GB Mac): write a whole phase, then **one** `cargo ndk -t arm64-v8a -P 26 check -j2 -p survivors-android` and **one** focused `cargo test -j1 -p benilla-app --lib -- survivors::<module>`. Never two compiles at once.
- Before every APK, check for **B0001**: any system with two `&mut` queries of the same component needs disjoint `With`/`Without` filters, and ParamSet members count too. Scanner: `python3 /tmp/claude-503/scan.py <files>`; re-create it from the transcript if missing.
- Before handing an APK over: install it, launch it over adb, check it is alive after 25 s, then tap into **every screen the phase touched** and screenshot it. Device pixels are 2960×1848 and the UI scale is ~2.47 px per unit.
- Full-screen pages use `GlobalZIndex(455)`, above the menu screen (410). HUD overlays in play stay below the card screen.
- Text is at least `TEXT_FLOOR` (13 units, about 32 px on the tablet). Touch targets are at least 44 units. Nothing sits below `SAFE_BOTTOM` (36 units, the gesture bar).
- HUD text updates only on change: no per-frame `Text` writes, and no `DerefMut` on resources without a change.
- The Demo bot (`auto.rs`) must use every new button sensibly, so balance batches exercise it.
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage only the task's files. Push `mobile-graphics` to `fork` as a backup after each phase.

## UI layout (game-ui-ux review)

The tablet is held in two hands. The left thumb owns the stick, the right thumb owns actions.

**One action cluster** sits bottom-right (mirrored bottom-left when the stick is on the right). It is an arc around the thumb's rest point, all at least 64 units:
- **Mobility** (F): the existing button, at the arc's centre.
- **Potion** (Q), new: above-left of mobility. It shows a radial cooldown sweep and dims when the hero is at full HP, but stays tappable.
- **Ultimate** (R), Phase C: above mobility, the largest (80 units), gold edge, with a "ready" pulse.

The cluster anchors to the corner (`right`/`bottom` offsets, not absolute x/y), so it survives any aspect ratio. It sits above `SAFE_BOTTOM`, and never under the 2× speed button or the clock row.

**Cooldown states, shared by every action button** (mobility, potion, ultimate). One component, `ActionButton { cd: f32, max: f32 }`, drives all three:
- **Charging:** the icon is greyscale (tinted to 35% luminance), with a dark radial sweep that empties clockwise like WoW's cooldown clock. Whole seconds are shown in the centre (white, outlined, at least 18 units). Below 3 s, tenths show.
- **Ready:** the icon returns to full colour. One bright "ready" flash plays: a gold ring expands 1.0→1.4× and fades over 0.35 s, with a soft click sound. The edge then keeps a slow gold glow pulse (1 Hz, alpha 0.5–1.0) until it is used.
- **Pressed:** a 0.9× squash for 0.08 s, then the button goes back to Charging.
- **Pressed while charging:** a short red edge flash and no action. This doubles as the "not ready" feedback.
- **Updates:** the sweep and number update only when the shown value changes (every whole second, or every 0.1 s under 3 s). The glow animates its own alpha only.
- **Pure test:** `action_state(cd, max) -> {Charging(frac, label), Ready}`, with labels "12", "2.4" and Ready at cd ≤ 0.

**Soulstone held**: a small soulstone icon (spell 20707's icon) right of the HP bar, with no text. When the soulstone is used, a full-screen purple flash and the "Soulstone" spell visual on the hero mark the revive.

**Card screen** (level-up): a row of three secondary buttons under the cards, `Reroll (n)`, `Skip`, `Banish (n)`, each 44 units tall. Count 0 dims a button. Keys: R reroll, K skip, B banish, so they don't clash with WASD.
- **Banish is a mode**: tapping Banish tints the cards red and the next card tapped is banished. Tapping Banish again cancels.
- Focus order is cards first, then the row. Escape cancels banish mode.

**Records**: the name column widens to 12 characters. Rows stay one line.

**Create Character name**: the name becomes a tappable field. Tapping it opens the Android keyboard and shows a caret. Enter confirms, and tapping outside confirms too. Rules: 2–12 letters, first letter capitalised, letters only.

**Locked difficulty**: tapping it shakes the pill (±4 units, 0.25 s) and flashes its red reason text brighter for 1 s.

**Secrets page**: cards shrink to content, so the riddle sits right under the "???". A small "▼ more" hint shows while rows remain below.

## Review Focus

1. **Death with a Soulstone during the level-up screen or the final-boss kill frame.** Expected: the revive happens once. A win on the same frame wins and keeps the stone. Test: `soulstone_revives_once_and_win_beats_death`.
2. **Banishing until the pool is empty.** Expected: `choices_for` falls back to `Choice::Heal`, as it does today when everything is maxed, and never offers a banished key again in the same run. Test: `banish_never_reoffers_and_empty_pool_heals`.
3. **Potion pressed while dead, at full HP, during level-up or paused.** Expected: nothing happens and no cooldown is spent, except at full HP, where it stays allowed but dimmed. Test: `potion_ignored_when_dead_or_paused`.
4. **Block and dodge against ground and DoT damage.** Expected: only direct enemy melee and missile hits can be blocked or dodged; ground areas and auras cannot. Test: `guard_only_mitigates_direct_hits`.
5. **The keyboard left open when the app is backgrounded or the screen changes.** Expected: leaving the name field or the Create screen turns `ime_enabled` off. Test: the pure `name_input` reducer, plus the tablet check.

---

## Phase A — Survival and feedback fixes (one APK at the end)

### Task A1: Red health globe

**Files:** Modify `survivors/enemies.rs` (`look_for`, `spawn_gem`, gem animation); create `survivors/globe.rs`.

**Interfaces:**
- Produces `globe::spawn_globe(commands, meshes, mats, pos) -> Entity`: an emissive red sphere (r = 0.45 yd, emissive `#ff2a1a` ×4), a child ground decal (soft red radial quad, r = 1.2 yd, additive), a 1.5 Hz bob of ±0.12 yd and a 2 Hz glow pulse.
- `GemKind::Heal` uses it instead of the creature display. Boss heals (1.0) are 1.4× bigger.

- [ ] Step 1: test `globe_pulse_is_periodic_and_bounded`, which checks the pure `pulse(t)` stays in [0.8, 1.2] with period 0.5 s.
- [ ] Step 2: implement `globe.rs`, then wire `spawn_gem` for `Heal`. On pickup, keep the existing heal and add a green "+N" combat text.
- [ ] Step 3: commit "Survivors: health drops are glowing red globes".

### Task A2: On-demand potion

**Files:** create `survivors/potion.rs`; modify `mod.rs` (plugin), `hud.rs` (button), `touch.rs` (button tap), `auto.rs` (bot).

**Interfaces:**
- `pub(super) const POTION_KEY: KeyCode = KeyCode::KeyQ;` (grep `KeyQ` first; if taken, use `KeyE`).
- `pub(super) fn potion_heal(d: Difficulty) -> f32` returns 0.30 on Normal, 0.25 on Heroic and 0.20 on Mythic, as a share of max HP.
- `pub(super) const POTION_COOLDOWN: f32 = 60.0;` in run seconds, so it follows 2× speed.
- `Run.potion_cd: f32`.
- `pub(super) fn can_drink(run: &Run, phase: Phase) -> bool`: false when dead, outside `Playing`, or on cooldown.

- [ ] Step 1: tests `potion_heal_by_difficulty`, `potion_ignored_when_dead_or_paused` and `potion_cooldown_counts_run_time`.
- [ ] Step 2: implement. It heals instantly, plays the Healing Potion visual (spell 441) and its sound, then starts the cooldown.
- [ ] Step 3: create `survivors/action_button.rs` (the shared cooldown states in the UI layout section: greyscale and sweep while charging, a ready flash plus a gold glow pulse, a red flash when pressed early), with test `action_state_labels_and_ready`. Use it for the potion button **and** retrofit the existing mobility button to it.
- [ ] Step 4: the bot drinks below 30% HP.
- [ ] Step 5: commit.

### Task A3: Warrior block, Rogue dodge

**Files:** create `survivors/guard.rs`; modify `data.rs` (`ClassDef.guard`), the hero damage intake (grep `run.hp -=` in `enemies.rs`/`hero.rs`), `combat_text` hook.

**Interfaces:**
- `pub(crate) enum Guard { None, Block { chance: f32, keep: f32 }, Dodge { chance: f32 } }`, with `ClassDef.guard: Guard`.
  - Warrior: `Block { chance: 0.25, keep: 0.35 }` (a blocked hit deals 35%).
  - Rogue: `Dodge { chance: 0.18 }`.
  - Everyone else: `None`.
  - These are starting values; Task A9 tunes them from logs.
- `pub(super) enum Hit { Direct, Area }`.
- `pub(super) fn mitigate(guard: Guard, hit: Hit, dmg: f32, roll: f32) -> (f32, Option<Guarded>)` with `Guarded { Block, Dodge }`. Only `Hit::Direct` can be guarded.
- `Run.guarded: Option<(Guarded, f32)>` records the last guard for counterattack skills (B3).

- [ ] Step 1: tests `guard_only_mitigates_direct_hits`, `block_keeps_share`, `dodge_takes_nothing` and `none_guard_is_identity`.
- [ ] Step 2: implement and route enemy melee and missiles through `mitigate`. Show white "Block" / "Dodge" combat text at the hero (TEXT_FLOOR or larger) and play the shield-block sound (Warrior).
- [ ] Step 3: commit.

### Task A4: Charge rework and animation fix

**Files:** `survivors/mobility.rs`.

**Interfaces:** `Move::Charge(f32)` now always lunges along the move direction (the facing direction if still) for `f32` yards in 0.25 s.
- If the path passes within 1.5 yd of an enemy, it stops there and stuns every enemy within 4 yd for 1.5 s, knocking them back 3 yd.
- With no enemy on the path, it is a plain lunge, as fast as Blink.
- `pub(super) fn charge_path(from: Vec2, dir: Vec2, len: f32, enemies: &[Vec2]) -> (Vec2, Option<usize>)` is pure.

- [ ] Step 1: tests `charge_lunges_without_target`, `charge_stops_at_first_enemy_on_path` and `charge_ignores_enemies_off_path`.
- [ ] Step 2: find why the animation sometimes doesn't play. Read `Fx::cast` and the Charge branch. Likely cause: the cast is issued on the same frame the run animation re-asserts. Fix it by holding the charge anim for the lunge's duration.
- [ ] Step 3: implement.
- [ ] Step 4: commit.

### Task A5: Records show full names

**Files:** `survivors/records.rs`, `survivors/hud.rs` (results).

**Interfaces:** `Record.initials` becomes `Record.name: String` (serde alias `initials`, so old files load). New records store the character's name automatically, with no entry screen; the Demo stores "Demo". Remove the three-letter `Entry` flow and its `EntryRoot` UI. The table's name column fits 12 characters.

- [ ] Step 1: tests `old_records_load_with_initials_as_name` and `new_record_takes_character_name`.
- [ ] Step 2: implement, remove the entry UI, and check that the results screen still says "New record!".
- [ ] Step 3: commit.

### Task A6: Soft keyboard for character names

**Files:** `survivors/characters.rs`; possibly `crates/survivors-android` if key events need mapping.

**Interfaces:**
- `pub(super) fn name_input(name: &str, key: NameKey) -> String` is a pure reducer, with `NameKey::{Char(char), Backspace, Done}`. It enforces 2–12 letters, letters only, and capitalises the first letter.
- When the name field is focused, set `Window.ime_enabled = true` (winit 0.30.13 calls `show_soft_input`). Clear it on Done, on tapping outside, on leaving Create, and on `Phase` change.

- [ ] Step 1: tests `name_input_caps_and_limits` and `name_input_rejects_non_letters`.
- [ ] Step 2: implement the field (a caret blink at 1 Hz, which touches Text only on toggle) and the keyboard events: `KeyboardInput` with `Key::Character`, and `Ime::Commit`.
- [ ] Step 3: tablet check that the keyboard appears and typed letters land. If NativeActivity delivers no key events, fall back to an in-game A–Z grid and record why in the report.
- [ ] Step 4: commit.

### Task A7: Stick-side option

**Files:** `survivors/settings.rs` (a "Stick: Left / Right" row, saved like framecap), `survivors/touch.rs` (stick zone and camera-drag zone swap), `survivors/hud.rs` (the action cluster mirrors).

**Interfaces:** `pub(super) enum StickSide { Left, Right }` as a resource. `pub(super) fn stick_zone(side, screen: Vec2) -> Rect` is pure.

- [ ] Step 1: test `stick_zone_mirrors`.
- [ ] Step 2: implement. The swap is immediate (the cluster re-anchors on change only).
- [ ] Step 3: commit.

### Task A8: Menu polish

**Files:** `menu.rs`, `characters.rs`, `secrets/page.rs`.
- **Locked difficulty feedback:** shake plus a brighter reason text (UI layout section).
- **Secrets cards:** sized to their content, with a "▼ more" hint.
- **Quit button**, under Secrets in the left column. It sends `AppExit::Success`. Tablet check that the activity closes cleanly with no crash report; if it doesn't, call `AndroidApp::finish()`.
- **Zone list:** the Elwynn row's text and icons no longer touch (a padding-right equal to the icon width).

- [ ] Step 1: implement.
- [ ] Step 2: commit.

### Task A9: Class-themed talent icons

**Files:** `talents.rs`; read `Talent.dbc` and `TalentTab.dbc` through `benilla_formats::talents`.

**Interfaces:** `pub(super) fn tree_icons(class_id: u8) -> [[u32; 5]; 3]` gives, per tree, the spell of the first-column talent at tiers 1, 2, 3, 4 and 7 (the 31-point talent). It falls back to today's generic icon when the data is missing. Names and stats are unchanged.

- [ ] Step 1: test `tree_icons_fall_back_without_data`.
- [ ] Step 2: implement, loaded once when the archives mount, as a resource.
- [ ] Step 3: commit.

### Task A11: Thumb HUD (level-up stack, health bottom-right) and keyboard fixes

The user approved the mockup on 2026-11-01: `docs/superpowers/mockups/thumb-hud.png` (source `thumb-hud.svg`). The level-up choices no longer take over the screen. **The game keeps running at full speed** (the user's choice) while they wait.

**Files:** `cards.rs` and `hud.rs` (replace the full-screen card screen and the `Phase::LevelUp` pause), `mod.rs` (phase and time), `touch.rs` (taps on the stack), `settings.rs`/`StickSide` (mirroring), `auto.rs` (the bot picks from the stack), and `characters.rs` (keyboard fixes).

**Layout** (anchored to the action side; Stick: Right mirrors it):
- **Stack:** a header row reading "Level up" with a pending badge "+N" when N > 1, plus Reroll/Skip/Banish chips (placeholders until B1, so hide them until then). Below it, three rows each 44 units tall and about 200 units wide, with an icon, the name, and a rarity- or rank-coloured sub line. The stack's bottom sits above the action arc.
- **Tooltip:** opens beside the selected row, towards the screen centre, with the full description and "Tap again to take".
- **Health and XP:** a level circle, the HP bar with "hp / max" and the XP bar, bottom-right, left of the action arc. They replace the current HP/XP position. Updates happen on change only.
- **Action arc:** F (bottom-right), Q (left of F), and a slot for R above F (hidden until Phase C).
- **The gesture-bar strip stays clear.**

**Behaviour:**
- Pending level-ups queue in `run.pending_levels`. The stack shows while that count is above 0, and the game does **not** pause.
- Tap a row to select it (it highlights and the tooltip opens). Tap it again to take it. Tapping elsewhere, or after 4 s, deselects it.
- Keys: 1, 2 and 3 select, the same key again (or Enter) takes, and Escape deselects. Pad: D-pad and A.
- While a stack is open, movement and abilities continue. Boss chests also offer through the stack (gold header "Chest").
- Taking a choice deals the next pending set at once, with a short slide-in of about 0.15 s.
- The bot picks through the same path.

**Keyboard fixes from the tablet test:**
- Tapping the name field selects it all, so the first letter replaces the random name.
- While the keyboard is up, the field moves to the top centre (above the soft keyboard), and it moves back on close.
- The Android Enter key (`KeyCode::Enter` / `NumpadEnter` / `Key::Enter`) confirms and closes.
- Tapping the field while it is focused with the keyboard hidden re-shows the keyboard (toggle `ime_enabled` off for one frame, then on).
- The field must not overlap the race/class label: move the label above the field.

**Tests:**
- `stack_shows_while_pending_and_game_runs`
- `second_tap_takes_first_tap_selects`
- `deselect_after_timeout`
- `pending_badge_counts`
- `name_field_select_all_on_focus`

- [ ] Step 1: write the tests.
- [ ] Step 2: implement the stack and remove the full-screen card screen.
- [ ] Step 3: health and XP, and the action arc.
- [ ] Step 4: keyboard fixes.
- [ ] Step 5: commit.

### Task A10: Phase A check, balance pass and APK

- [ ] One `cargo ndk check`, then the B0001 scan, then the focused tests.
- [ ] Pull the user's latest run logs (`benilla-config/Diagnostics/runs/*.jsonl`): damage taken from bosses versus adds, and time to kill bosses for the Warrior and the Rogue. Tune `Guard` values and `potion_heal` and record them in `docs/BALANCE.md` (round 10).
- [ ] APK, install, launch, then screenshot the menu, Create (keyboard open), Settings (stick row), a run (globe, potion button, block text) and Records.
- [ ] Push to the fork.

---

## Phase B — Run variety (one APK at the end)

### Task B1: Reroll, Skip, Banish

**Files:** `hud.rs` (`choices_for`, the card screen), `cards.rs`, `mod.rs` (`Run`), `auto.rs`.

**Interfaces:**
- `Run.rerolls: u8` (start 2), `Run.skips: u8` (start 3), `Run.banishes: u8` (start 2), `Run.banished: Vec<&'static str>`.
- Reroll deals 3 new choices. Skip closes the screen with no pick. Banish removes the key for the rest of the run.
- `choices_for` filters `banished`. With an empty pool it offers `Choice::Heal`.
- A boss chest gives +1 reroll.
- The bot never skips. It banishes the lowest-scoring offer when it has more than 4 abilities.

- [ ] Step 1: tests `banish_never_reoffers_and_empty_pool_heals`, `reroll_spends_and_redeals` and `skip_spends_and_closes`.
- [ ] Step 2: implement the buttons and banish mode (UI layout section).
- [ ] Step 3: commit.

### Task B2: Soulstone

**Files:** create `survivors/soulstone.rs`; modify `enemies.rs` (drop), `hero.rs` (`hero_vitals` death branch), `hud.rs` (held icon).

**Interfaces:**
- `GemKind::Soulstone`. It drops from **non-final bosses** with chance 0.20 on Normal, 0.12 on Heroic and 0.08 on Mythic. At most one is held, and one is never dropped while one is held.
- `Run.soulstone: bool`.
- `pub(super) fn try_revive(run: &mut Run) -> bool`: when `hp <= 0` and a stone is held, it consumes the stone and sets hp to 50% of max, `immune_t` to 3.0 and `dead_t` to None, then knocks back every enemy within 8 yd.
- It is checked **before** `dead_t = Some(0.0)`. `run.won` takes precedence.

- [ ] Step 1: tests `soulstone_revives_once_and_win_beats_death`, `soulstone_drop_chance_by_difficulty` and `no_drop_while_held`.
- [ ] Step 2: implement, with the Soulstone Resurrection visual (spell 20707) and the purple flash.
- [ ] Step 3: commit.

### Task B3: New Warrior and Rogue skills

**Files:** `data.rs` (abilities and pools), `abilities.rs` (new kinds or effects), `guard.rs`.

**New mechanics:**
- `Effect::Knockback(yd)`.
- `Kind::Counter`: fires when `Run.guarded` is set within the last 1.0 s.
- `Kind::Rage`: damage scales up as HP goes down, `1 + 0.6 × (1 − hp/max)`.

**Warrior pool adds:**
- `shield_block` (Shield, +block chance 15% for 6 s, cooldown 12 s);
- `shield_slam` (Cone, Knockback 4);
- `revenge` (Counter);
- `berserker_rage` (Rage passive-style Aura);
- `intimidating_shout` (Nova, Fear 3 s);
- `bloodrage` (self-haste 30% for 8 s, costs 5% HP).

**Rogue pool adds:**
- `fan_of_knives` (Nova);
- `riposte` (Counter);
- `evasion` (+50% dodge for 5 s, cooldown 20 s);
- `kidney_shot` (Strike, Stun 2 s);
- `deadly_poison` (Dot);
- `blade_flurry` (Aura cleave).

Spell ids, visuals and damage numbers come from the Vanilla 1.12 data. Each new entry's damage starts at the class's median ability, scaled by its cooldown.

- [ ] Step 1: tests `counter_fires_only_after_guard`, `rage_scales_with_missing_hp` and `knockback_moves_away_from_hero`.
- [ ] Step 2: implement the mechanics, then the data.
- [ ] Step 3: bot scoring for the new kinds.
- [ ] Step 4: commit.

### Task B4: Evolutions

**Files:** create `survivors/evolve.rs`; modify `data.rs` (`EVOLUTIONS`), `hud.rs` (`Choice::Evolve`), `cards.rs` (gold "Evolution" card frame).

**Interfaces:**
- `pub(crate) struct EvolutionDef { a: &str, b: &str, into: &str }`.
- When both `a` and `b` are at `MAX_ABILITY_LEVEL`, a **boss chest** offers `Choice::Evolve(i)` first. It replaces `a` with `into` at level 1, which is already stronger than `a` at max, and frees `b`'s slot.
- Warrior:
  - `whirlwind` + `sweeping_strikes` → `bladestorm`;
  - `thunder_clap` + `demoralizing_shout` → `thunderstorm_roar`;
  - `shield_slam` + `shield_block` → `shield_wall_bash`.
- Rogue:
  - `fan_of_knives` + `blade_flurry` → `blade_dance`;
  - `deadly_poison` + `kidney_shot` → `crippling_venom`.

- [ ] Step 1: tests `evolve_offered_only_when_both_maxed`, `evolve_replaces_and_frees_slot` and `evolve_only_from_chest`.
- [ ] Step 2: implement.
- [ ] Step 3: commit.

### Task B5: Phase B check, balance batch and APK

Same gate as A10. A bot balance batch per difficulty with Warrior, Rogue and Mage. Record the results in BALANCE.md round 11. Tablet screenshots: the card screen (Reroll/Skip/Banish and banish mode), an evolution card, the soulstone icon and revive.

### Task B6+: The other seven classes

One task per class, the same shape as B3 and B4: about 6 skills and 2 evolutions each. Order: Paladin, Hunter, Mage, Priest, Warlock, Druid, Shaman.

---

## Phase C — Ultimates (spec first)

### Task C1: Ultimates design spec

`docs/superpowers/specs/2026-11-xx-ultimates-design.md`. It covers:
- one ultimate per tree's 31-point capstone;
- a 90–120 s cooldown, the R key and the cluster button;
- the point pace measured from save data, with 31 points expected around Heroic/Mythic progression;
- the list of 27 ultimates from 1.12 spells (for example Warlock Inferno with an Infernal for 20 s, and Mage Blizzard across the screen);
- the bot rule.

The user approves it, then a plan follows.

---

## Queued (user feedback, 2026-11-01, not started)

- **D1: desktop WoW-style mouse controls** (Mac/desktop only; the tablet is unchanged).
  - The camera starts unlocked, orbiting behind the hero; the wheel zooms; C toggles the locked top-down view.
  - Left-drag orbits the camera without turning the hero. Right-drag turns the camera and the hero together.
  - WASD moves relative to the camera. Both mouse buttons held runs forward along the camera heading.
  - F casts along facing/movement instead of at the cursor.
- **P1: preload during the loading screen.** On the Mac run, 0.1-0.8 s hitches came the first time each new thing appeared: new skills' effects, a boss arriving, new enemy types, the win. Warm the zone roster's creature displays, its bosses, the class's spell visuals and sounds, and the GPU pipelines before the run starts. This helps the tablet too.
- **Optional:** copy the WoW Data folder (5.5 GB) from the Passport drive to the Mac's internal SSD, and point the app's launcher at it.
- **iOS:** deferred. It first needs ~30 GB free and Xcode; then a survivors-ios shell, and touch cfgs extended to iOS.
- **M1: masteries after max** (user approved 2026-11-01). The Mac run maxed every ability and passive at level 75 (7:25), then had 70 levels of heal-only picks. Once the build is full, the level-up stack offers 3 stackable boosts, rarity-coloured:
  - Might: +3/6/10% damage.
  - Vitality: +4/8/12% max HP.
  - Haste: +1.5/3/5% cooldowns, capped.
  - Precision: +1/2/3% crit, capped.
  - Reach: +2/4/6% area.
  - Swiftness: +1.5/3% speed, capped.
  - "Second wind": heal 50% now.
  
  Counts show beside the skills and are logged. Replaces the Heal-only fallback (`bank_maxed`).
