# Secret Races Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Route game work through the gamedev skills: `game-ui-ux` (Task S5), `game-feel` (Task S6), `shader-programming` (silhouettes, Task S5) and `create-game-assets` (asset QA, Task S1).

**Goal:** 13 hidden playable races (the Goblin plus 12 WoW creatures), unlocked by secret conditions, announced with a banner and listed on a Secrets page of riddles. They change the hero's look only.

**Architecture:**
- A secret race is data: its display IDs (colour variants), allowed classes, an unlock condition and a riddle.
- Picking one adds `(display, race, sex)` looks to `Selection.looks`, the same path the 8 races use (`mod.rs:226`, `hud.rs` → `hero::spawn_hero`).
- Account-wide progress counters are bumped from the events the run already produces. Pure check functions turn counters into unlocks.
- The Secrets page and the unlock banner are survivors UI.

**Tech Stack:** Rust, Bevy 0.18.1 (pinned), the benilla WoW 1.12 client, survivors mode (`crates/benilla-app/src/survivors/`), `toml` persistence through `crate::local_state`.

**Spec:** `docs/superpowers/specs/2026-10-31-secret-races-design.md`

## Global Constraints

- Look only: a secret race never changes stats, spells or passives.
- Classes per race, conditions and riddles exactly as the spec's table, including the Skeleton Paladin.
- Unlocks are account-wide, survive deleting characters, and are never earned by the auto-player or the Demo (`Auto` present).
- Locked entries show only the silhouette and the riddle, never the condition.
- Persistence is backward compatible: old `characters/<id>.toml` files load unchanged (`#[serde(default)]` on every new field).
- Bevy 0.18.1. Survivors code is ours; upstream benilla files are touched only where a task says so, minimally.
- Build discipline on this 8 GB Mac: ONE compile at a time, `-j2` for checks and `-j1` for tests, never `pgrep -f "cargo …"`. The controller builds the APK.
- Text ≥ ~18 px at 1080p equivalent, touch targets ≥ 44 pt, nothing in the gesture-bar zone (`game-ui-ux`).

## Review Focus

1. A character created with a secret race must still load if its secret's data entry changes. An unknown key falls back to the class's first normal look, with no panic.
2. Counters must not double-count: one death per run, kills counted once per enemy, a chest once.
3. The Demo/bot and god-mode runs must never bump counters or unlock anything.
4. Several unlocks at once (e.g. a Mythic Warlock win unlocking Dragonspawn and Felguard) must queue their banners, not overlap.
5. A secret race whose model lacks a cast animation must still play its spells (attack swing as cast), not freeze or T-pose.

---

### Task S1: Secret race data (display IDs and checks from the 1.12 data)

**Files:**
- Create: `crates/benilla-app/src/survivors/secrets/data.rs` (the 13 entries)
- Tool (throwaway, not committed): the probe at `<scratchpad>/races/` (`benilla-mpq` + `benilla-m2`), extended to read `CreatureDisplayInfo.dbc`, `CreatureModelData.dbc` and `CharSections.dbc`

**Interfaces:**
- Produces: `pub(super) enum SecretRace { Goblin, Skeleton, Murloc, Gnoll, Kobold, Ogre, Satyr, Furbolg, Dragonspawn, Quillboar, Felguard, Centaur, Imp }` with `ALL: [SecretRace; 13]`.
- Produces: `pub(super) struct SecretDef { key: &'static str, name: &'static str, classes: &'static [u8] /* ChrClasses ids */, displays: &'static [u32], scale: f32, casts: bool, riddle: &'static str, condition: &'static str }` and `fn def(r: SecretRace) -> &'static SecretDef`.

- [ ] **Step 1: Find the displays.** For each race, list the `CreatureDisplayInfo` rows whose model is that race's creature model (from the probe's model table), with their skin textures. Keep 1-4 visually distinct variants. `create-game-assets` QA: check each variant's texture exists and isn't a quest-specific or broken skin.
- [ ] **Step 2: The Goblin.**
  - Read `CharSections.dbc` rows for race 9: faces, skins, hair.
  - If they exist, the Goblin uses the character path (`race` 9 in `bytes0`, the same composite as the 8 races) and its display is the goblin character display.
  - If not, it uses NPC goblin displays as fixed skins.
  - Record which in the module doc.
- [ ] **Step 3: Scale.** For each race, pick a `scale` so its height on screen is within ~±20 % of a Human's. The arena camera is the same for all.
- [ ] **Step 4: Write `data.rs`** with the table. Conditions and riddles are copied verbatim from the spec. Test `every_race_has_classes_displays_and_a_riddle` checks:
  - non-empty `classes`, `displays` and `riddle`;
  - Skeleton's `classes` contains Paladin (2);
  - no other race but Human/Dwarf/Skeleton offers Paladin.
- [ ] **Step 5:** `cargo check -j2`, then commit `Secrets: the 13 secret races' data`.

### Task S2: Account-wide progress and the unlock rules

**Files:**
- Create: `crates/benilla-app/src/survivors/secrets/mod.rs` (`Progress`, `check`, persistence, plugin)
- Modify: `crates/benilla-app/src/survivors/mod.rs` (register `secrets::plugin`)

**Interfaces:**
- Produces: `#[derive(Resource, Serialize, Deserialize, Default)] pub(super) struct Progress` with:
  - `unlocked: Vec<String>` (race keys)
  - `deaths_by_class: HashMap<u8, u32>`
  - `kills_by_family: HashMap<String, u32>` ("murloc", "gnoll")
  - `chests: u32`
  - `wins: Vec<WinRecord>`
  - `endless_best_secs: HashMap<String, f32>` (zone → seconds)
- Produces: `pub(super) struct WinRecord { class_id: u8, zone: String, difficulty: String, secs: f32, min_hp_frac: f32 }`.
- Produces: `pub(super) fn newly_unlocked(p: &Progress, clears: &Clears) -> Vec<SecretRace>`. It is pure, returns races whose condition is now met and not yet in `unlocked`. `Clears` gives the passes and zones cleared, read from `characters::Unlocks`/completions.
- Produces: `fn load() -> Progress` and `fn save(&Progress)` → `benilla-config/secrets.toml` via `local_state::write_atomic`.

- [ ] **Step 1: Tests (RED).** One per condition, at the boundary:
  - 99 Paladin deaths → no Skeleton; 100 → Skeleton;
  - 9,999 / 10,000 murloc kills → Murloc;
  - 4,999 / 5,000 gnolls → Gnoll;
  - 99 / 100 chests → Kobold;
  - a Heroic Warrior win → Ogre;
  - a Druid win with `min_hp_frac` 0.51 → Furbolg, 0.49 → none;
  - a Mythic Warlock win → Felguard **and** (if all Mythic clears are true) Dragonspawn;
  - a win at 479 s → Imp, 481 s → none;
  - Endless Desolace 1,200 s → Centaur;
  - the whole Normal pass → Goblin;
  - the Ashenvale/Felwood Heroic clears → Satyr; the Razorfen-area clears → Quillboar.

  Map the zone names onto `data::MAPS`. If a zone in the spec doesn't exist, the test names the nearest real zone and the report says so. Plus: `already_unlocked_is_not_returned_again` and `progress_round_trips_through_toml`.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement. Endless: if the Endless mode isn't built yet, `endless_best_secs` stays empty, and the Centaur's condition is noted in the report as waiting on Endless. Don't invent Endless.
- [ ] **Step 4:** Tests pass. Commit `Secrets: account-wide progress and the unlock rules`.

### Task S3: Counting during runs (never for the bot)

**Files:**
- Modify:
  - `secrets/mod.rs` (systems)
  - `survivors/enemies.rs` (the chest-opened site, ~line 642: emit a `SecretEvent::Chest`)
  - `survivors/combatlog.rs` (`killed`, `run_over`) or `hud.rs` (run end), wherever the run's end, its result, the hero's lowest health and the elapsed time are known.
- Prefer one new `#[derive(Message)] enum SecretEvent { Kill(&'static str), Chest, RunEnd { won: bool, … } }` written at those sites and read by `secrets`. Avoid scattering progress writes.

**Interfaces:**
- Consumes: `newly_unlocked`, `Progress`.
- Produces: `#[derive(Message)] pub(super) struct Revealed(pub SecretRace)`, one per new unlock (for S6).

- [ ] **Step 1: Tests.** A small headless `World`: send `Kill("Murloc Forager")` ×3 → `kills_by_family["murloc"] == 3`. With `Auto` present, nothing changes. `RunEnd` lost as a Paladin → `deaths_by_class[2] += 1` exactly once. A run with min health 0.6 that wins as a Druid records `min_hp_frac` 0.6.
- [ ] **Step 2:** Implement:
  - The family comes from the creature name (contains "Murloc"/"Gnoll"), with a unit test of the matcher.
  - The hero's minimum health fraction is tracked per run in `Run` (new field, reset in `start_run`).
  - On `RunEnd`, save `Progress`, call `newly_unlocked`, push to `unlocked`, write `Revealed` for each.
  - Saving happens at run end and on unlock only, not per kill.
- [ ] **Step 3:** Tests pass; `cargo check -j2` host + Android. Commit `Secrets: count kills, chests and runs; reveal unlocks`.

### Task S4: Playing a secret race

**Files:**
- Modify:
  - `survivors/characters.rs`: `Character` gains `#[serde(default)] secret: Option<String>`.
  - `survivors/menu.rs`: race picker and looks.
  - `survivors/mod.rs`: `Selection.looks` building.
  - `hud.rs`: `spawn_hero` input stays `(display, race, sex)`.

**Interfaces:**
- Consumes: `secrets::data::{SecretRace, def}`, `Progress.unlocked`.
- Produces: `pub(super) fn looks_for(class: &ClassDef, unlocked: &[String]) -> Vec<Look>` (the existing looks plus one per unlocked secret race that allows the class and per display variant). `Look` gains the secret key.

- [ ] **Step 1: Tests.**
  - `looks_for(Paladin, ["skeleton"])` includes Skeleton looks; `looks_for(Paladin, [])` has none.
  - `looks_for(Mage, ["skeleton"])` has none, since a Skeleton can't be a Mage.
  - A `Character` TOML without `secret` loads (old saves).
  - One with an unknown `secret = "naga"` loads and falls back to the class's first look.
- [ ] **Step 2: Create Character.**
  - Unlocked secret races appear after the 8 races in the race picker, in their own row headed "Secrets" with a gold edge.
  - Locked ones are not shown there; the Secrets page is where they're hinted.
  - Sex is hidden for creature races (one body).
  - Skins cycle through the race's display variants with the existing look arrows.
- [ ] **Step 3: Spawn.**
  - The chosen look's display and `scale` go to `spawn_hero` (add a `scale` parameter; today it is fixed 1.0).
  - Weapons attach through the existing `weapons` slots. The S1 data notes which races lack a weapon hand; there are none in the list.
  - For `casts: false` races, the cast animation request falls back to the 1H attack. Find where spells pick an animation (`creature_anim` / `units.rs` `Fx`). Test the fallback choice as a pure function.
- [ ] **Step 4:** `cargo check -j2` host + Android. Commit `Secrets: secret races in Create Character and in play`.

### Task S5: The Secrets page

Skill: `game-ui-ux`. Anchors and containers, a text floor, safe area, 44 pt targets, and a screen stack (opens over the menu, Back returns). Pattern to copy: the Records page (`survivors/records.rs`: `spawn_ui` at Startup, a `Page` resource, `press`/`show`/`rebuild_page`).

**Files:**
- Create: `crates/benilla-app/src/survivors/secrets/page.rs`
- Modify: `menu.rs` (a "Secrets" button in the left column; A11 has just reflowed that column into one container, so add to it, never with absolute positions)

**Layout:**
- **Header:** "Secrets — N / 13 discovered", gold, 28 px.
- **Grid:** 13 cards in rows of 5 (4 at narrow widths). Card ~180×240 UI units.
- **Locked card:**
  - a silhouette of the race (below);
  - "???" as the name;
  - the riddle in italic WoW gold-on-dark, 18 px, wrapped, max 4 lines;
  - a padlock badge.
- **Unlocked card:**
  - the race's model portrait, full colour, turning slowly;
  - the name in its class-colour-neutral gold;
  - its classes as class icons (≥ 32 px);
  - the condition (now revealed) in 16 px under a thin divider;
  - "NEW" ribbon until the page has been opened once after the unlock.
- **Scrolling:** the grid scrolls vertically if it doesn't fit (Bevy 0.18 `Overflow::scroll_y`); touch drag scrolls.

**Silhouette (`shader-programming`):**
- Preferred: draw the race's model in a UI viewport through the existing model-in-UI path (`crate::ui_models` / the portrait booth). Use a flat-colour override: output `vec4(0.04, 0.03, 0.02, alpha)` from the texture's alpha. A flat fragment output keeps the silhouette's shape and costs one texture sample. Avoid `discard` (mobile tile GPUs). Use alpha-to-coverage or alpha blend for cut-out parts.
- If a per-instance colour override isn't available without engine changes, spike it first (≤ 1 hour). Fall back to a pre-rendered dark image per race: render once at load into a 256² texture with the model lit only by a black ambient (no lights), cached. Report which path.
- Unlocked portraits reuse the same viewport with normal shading.

- [ ] **Step 1: Tests** (logic only): `page_entries(&Progress)` returns 13 entries in `SecretRace::ALL` order; locked ones carry `riddle` and no `condition`; unlocked ones carry `condition`; the count is right.
- [ ] **Step 2:** Implement the page and the menu button. Gamepad/keyboard focus: the first card focused on open, arrows move, Escape/Back closes.
- [ ] **Step 3:** `cargo check -j2`. Commit `Secrets: the Secrets page`. The controller screenshots it on the tablet and reviews it with `game-ui-ux`.

### Task S6: The unlock moment

Skill: `game-feel`. A large-tier event: layered, short, eased, returns to rest, respects the accessibility settings.

**Files:**
- Create: `crates/benilla-app/src/survivors/secrets/reveal.rs`
- Consumes: `Revealed` messages (S3), `feedback::ShakeSetting` (A6), the existing sound kit path.

**Feel:**
- **Timing:** a gold banner top-centre: "A secret has been revealed…" (22 px), then the race name (40 px).
  - pops in: scale 0.6 → 1.05 → 1.0 over 0.35 s, ease-out-back;
  - holds 2.5 s;
  - fades out over 0.4 s, ease-in.
- **Glow:** a soft gold glow behind it fades with it.
- **Sound:** WoW's quest-complete or level-up fanfare from the 1.12 sound kits (pick the one that reads as an achievement; note the kit id).
- **Shake:** one large-tier screen shake scaled by `ShakeSetting` (Off → none). Mid-run, only if not in the card screen; else after it closes.
- **Queue:** several reveals play one after another, never overlapping (Review Focus 4).
- **Not during the card pick:** a reveal that arrives in `Phase::LevelUp` waits for `Playing` or the results screen.
- **Results screen:** the game-over/victory screen lists any races revealed this run under the score ("Secret revealed: Murloc").

- [ ] **Step 1: Tests:** the queue (2 reveals → shown in order, the second starts after the first ends); `banner_scale(t)` curve values (0 → 0.6, 0.35 s → 1.0, overshoot ≤ 1.06); deferral during LevelUp.
- [ ] **Step 2:** Implement. **Step 3:** `cargo check -j2`. Commit `Secrets: the reveal banner, sound and shake`.

### Task S7: In-game polish pass (tablet)

- [ ] For each of the 13 races, the controller (or a Demo run per race with a test unlock lever `WOW_SURVIVORS_SECRETS=all`, dev builds only) checks on the tablet:
  - run, attack and death animations;
  - weapon in hand;
  - size against enemies;
  - the cast fallback;
  - the blob shadow sized right.
- [ ] Screenshots of the Secrets page (locked and unlocked) and one per race for the README, each reviewed with `game-ui-ux`.
- [ ] Fix-ups as found. Commit `Secrets: per-race polish from the tablet pass`.

---

## Order and dependencies

S1 → S2 → S3 → S4 → S5 → S6 → S7. S5 waits for A11, which is reworking the menu's left column. S4 and S5 both touch `menu.rs`, so they run sequentially.
