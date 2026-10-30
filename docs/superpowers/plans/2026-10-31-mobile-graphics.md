# Mobile Graphics and Performance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Route game work through the gamedev skills (`performance-optimization` first: measure, then fix).

**Goal:** Make late-game survivors runs smooth, cool and battery-friendly on the Galaxy Tab S9+ (Android, Adreno 740) and ready for iPhone, then spend the headroom on a better look.

**Architecture:** Phase A is code that can be built and checked on the Mac without the tablet: measurement, a frame cap, cheap CPU fixes, an upscaler, compressed textures and screen-shake feedback. Phase B starts on the tablet with a measured baseline and only then picks the GPU-side or CPU-side fixes, keeping each one only if its numbers improve. Phase C (iPhone) gets its own plan.

**Tech Stack:** Rust, Bevy 0.18.1 (pinned, do not migrate), wgpu 27 (Vulkan on Android, Metal on Apple), WGSL, benilla's own renderer (`crates/benilla-world`), cargo-ndk APK build.

**Spec:** the 2026-10-31 brainstorm in this session (gamedev skills: `performance-optimization`, `bevy-ecs`, `shader-programming`, `game-feel`, `game-ui-ux`). No separate design doc; the decisions are restated here.

## Global Constraints

- Bevy stays at 0.18.1; the `bevy-ecs` skill's 0.20 examples are concepts, not APIs.
- Upstream-file changes stay small and `#[cfg]`-gated or in our own files (AGENTS.md "Carries").
- Every new player-facing option lives in the survivors Settings panel and persists like the graphics preset (`survivors/graphics.rs` `preset_path()` pattern).
- Nothing in Phase B is kept without a before/after number from the tablet in `docs/BALANCE.md` (performance section).
- Build check for every task: `crates/survivors-android/android/build-apk.sh` exits 0 (env: `CARGO_TARGET_DIR=$HOME/azeroth-target`, JDK 17). Commit and push only after a successful build.
- Feedback that moves the camera never moves the hero or enemies (game-feel: shake the view, not the simulation).

## Review Focus

1. A frame cap of 60 on a 120 Hz screen must give an even 60, not alternating 8/25 ms frames: pacing (`benilla-world/src/frame_pace.rs`) and the cap must agree. Test in Task A2.
2. A device without ASTC (desktop GPUs) must keep today's BC or RGBA8 path untouched. Test in Task A5.
3. Screen shake at 0% must be exactly no offset, and shake must decay to rest after the last event. Test in Task A6.
4. The texture cache must survive a half-written file (app killed mid-write) without crashing or rendering garbage. Test in Task A5.
5. The combat-log tick must stay valid JSON when the GPU meter is off (no `gpu_ms` value). Test in Task A1.

---

## Phase A — before the tablet (build and test on the Mac)

### Task A1: Frame-time split in the combat log

**Files:**
- Create: `crates/benilla-app/src/survivors/frametime.rs`
- Modify: `crates/benilla-app/src/survivors/combatlog.rs` (`tick`, ~line 322), `crates/benilla-app/src/survivors/mod.rs` (register plugin)
- Modify: `scripts/balance/analyze.py` (two new columns)

**Interfaces:**
- Produces: `pub(super) struct FrameSplit { pub cpu_ms_mean: f32, pub cpu_ms_max: f32, pub gpu_ms_mean: Option<f32> }` resource, reset each second by `combatlog::tick`; `pub(super) fn plugin(app: &mut App)`.
- Consumes: `crate::perf::gpu::GpuMsShared` (optional resource; present only with `WOW_GPU_MS=1`).

- [ ] **Step 1: Test** `frametime::tests::split_mean_and_max`: feeding samples `[4.0, 6.0, 20.0]` gives mean `10.0`, max `20.0`; `combatlog::tests::tick_json_without_gpu` checks a tick line with `gpu_ms: None` parses as JSON and has no `gpu_ms` key.
- [ ] **Step 2:** Run `cargo test -p benilla-app survivors::frametime` → FAIL (module missing).
- [ ] **Step 3:** Implement: a system in `First` stores `Instant::now()` (use `bevy::platform::time::Instant`), one in `Last` adds the elapsed ms to the second's samples; `tick` appends `"cpu_ms":mean,"cpu_ms_max":max` and, when present, `"gpu_ms":mean` read from `GpuMsShared` (nanoseconds → ms).
- [ ] **Step 4:** Tests pass; `analyze.py` prints `cpu50`, `gpu50` per class.
- [ ] **Step 5:** Add `WOW_GPU_MS=1` to the balance runs' `EXTRA_ENV` in `scripts/balance/run_batch.sh`. Build APK, commit `Combat log: CPU and GPU frame time per second`.

### Task A2: Frame-rate cap setting (30 / 60 / Uncapped, default 60)

**Files:**
- Create: `crates/benilla-app/src/survivors/framecap.rs`
- Modify: `crates/benilla-app/src/survivors/settings.rs` (new `SettingsButton::FrameCap`, `Readout::FrameCap`), `mod.rs`

**Interfaces:**
- Produces: `pub(super) enum FrameCap { Thirty, Sixty, Uncapped }` with `name()`, `next()`, `load()`, `cycle()` mirroring `units::DamageNumbers`; `fn sleep_for(target: Duration, spent: Duration) -> Option<Duration>`.

- [ ] **Step 1: Test** `framecap::tests::sleep_for`: target 16.667 ms, spent 10 ms → `Some(~6.667 ms)`; spent 20 ms → `None`; `Uncapped` → never sleeps.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement a `Last` system that sleeps the remainder (coarse `thread::sleep` to 1 ms short, then spin). Persist next to the preset file. Reason the default is 60: the tablet's screen is 120 Hz, so today an uncapped early game may render up to 120 fps and burn battery for frames nobody needs.
- [ ] **Step 4:** Check `frame_pace.rs` still sees an on-cadence median at 60 on a 120 Hz Fifo swapchain (every frame = 2 refreshes); add a unit test there: deltas alternating 16.6/16.8 ms are snapped, not passed through.
- [ ] **Step 5:** Tests pass, APK builds, commit `Settings: frame-rate cap, 60 by default`.

### Task A3: No per-frame allocation in enemy separation

**Files:** Modify `crates/benilla-app/src/survivors/enemies.rs:315-370` (`move_enemies`).

**Interfaces:** Produces `fn fill_grid(grid: &mut HashMap<(i32, i32), Vec<usize>>, positions: impl Iterator<Item = Vec2>, cell: f32)`.

- [ ] **Step 1: Test** `enemies::tests::fill_grid_reuses`: fill with 3 points, refill with 2 → old entries gone, the map's capacity not shrunk, same cells as a fresh build.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Keep the grid in a `Local`, clear each `Vec` instead of rebuilding the map (the `performance-optimization` skill's "reuse buffers" fix).
- [ ] **Step 4:** Tests pass, APK builds, commit `Enemies: reuse the separation grid`.

### Task A4: Snapdragon GSR upscaler for the 3D world (the mobile "DLSS")

**Files:**
- Create: `crates/benilla-app/src/survivors/upscale.rs`, `crates/benilla-app/assets/shaders/survivors_gsr.wgsl` (or wherever `survivors_grade.wgsl` lives)
- Modify: `crates/benilla-app/src/survivors/graphics.rs` (where `world_long_side` is applied), `mod.rs`
- Add: third-party notice for Qualcomm Snapdragon Game Super Resolution 1 (BSD-3-Clause) beside the shader

**Interfaces:** Produces `pub(super) struct Upscale { pub on: bool, pub sharpness: f32 }` component on the world camera, built like `grade::Grade` (`FullscreenMaterial`).

- [ ] **Step 1:** Read how the world is rendered at `world_long_side` and composited to the native-size window (`graphics.rs` `apply_preset`); write down in the module doc which pass does the upscale today (likely bilinear).
- [ ] **Step 2:** Port GSR1 (one-pass spatial upscale with edge-directed sharpening) to WGSL; validate with `naga crates/.../survivors_gsr.wgsl`.
- [ ] **Step 3:** Run it in place of the bilinear upscale when the world renders below native; Settings toggle "Upscaler: Sharp / Plain". Medium and Low default to Sharp.
- [ ] **Step 4:** On the Mac: screenshot the Medium preset with and without it (`WOW_SURVIVORS` desktop run or the capture probes) and keep the pair in `docs/perf/`.
- [ ] **Step 5:** APK builds, commit `Graphics: GSR upscale for Medium and Low`.

### Task A5: ASTC textures where the GPU has no BC

**Files:**
- Modify: `crates/benilla-assets/src/gpu_blp.rs` (`for_upload`, `BlpGpuSupportPlugin::finish`)
- Create: `crates/benilla-assets/src/astc_cache.rs`
- Modify: `crates/benilla-assets/Cargo.toml` (encoder crate, chosen in Step 1)

**Interfaces:**
- Produces: `pub fn astc_supported() -> bool` (published like `bc_supported`); `UploadChain` gains the `TextureFormat::Astc { block: B4x4, channel: Unorm }` arm; `astc_cache::get_or_encode(key: u64, chain: &BlpMipChain) -> Option<Vec<u8>>` with the cache directory from `BENILLA_HOME`.

- [ ] **Step 1: Decision:** pick the encoder. Requirements: pure Rust or vendored C that builds with cargo-ndk for `aarch64-linux-android`, ASTC 4x4 LDR, mip by mip. Check `intel_tex_2` (ISPC kernels, has ASTC encode) first; if it cannot cross-build, fall back to ETC2 RGBA8 (`etcpak`-style) with the same cache. Record the choice and why in the module doc.
- [ ] **Step 2: Tests:** `astc_cache::tests::round_trip_quality` (encode an 8×8 gradient and a 64×64 BLP from the fixtures, decode with a test-only decoder, PSNR ≥ 35 dB); `astc_cache::tests::torn_file_is_reencoded` (truncate a cache file → `get_or_encode` re-encodes, no panic); `gpu_blp::tests::bc_device_ignores_astc` (BC supported → BC path chosen even when ASTC is too).
- [ ] **Step 3:** Run → FAIL.
- [ ] **Step 4:** Implement: decode DXT → RGBA8 (existing path), encode to ASTC per mip, write `<hash>.astc.tmp` then rename (as `third_party/bevy_render` does for the pipeline cache). Order: BC if supported, else ASTC if supported, else RGBA8. `WOW_NO_ASTC=1` forces the old path (the A/B lever).
- [ ] **Step 5:** On the Mac (Apple GPUs take ASTC): run with `WOW_NO_BC=1` so the ASTC path is exercised; compare a screenshot to the BC one.
- [ ] **Step 6:** Tests pass, APK builds, commit `Textures: ASTC on GPUs without BC, cached on disk`.

### Task A6: Screen-shake feedback by importance, with a setting

**Files:**
- Modify: `crates/benilla-app/src/camera_shake.rs` (make `CameraShakes::add` `pub(crate)`; small, upstream file — keep the change to visibility)
- Create: `crates/benilla-app/src/survivors/feedback.rs`
- Modify: `survivors/hero.rs` (`hurt_hero`, line 484), `survivors/enemies.rs` (boss and final-boss death), `survivors/hud.rs` (level up), `survivors/settings.rs` (row "Screen shake: 100% / 50% / Off")

**Interfaces:**
- Produces: `pub(super) enum Tier { Medium, Large }`; `pub(super) fn tier_for(event: FeedbackEvent) -> Option<Tier>` where `FeedbackEvent` is `HeroHurt { frac_of_max: f32 } | BossKilled | FinalBossKilled | LevelUp`; `fn scaled(tier: Tier, setting: ShakeSetting) -> f32`.
- Consumes: `CameraShakes::add(row, pos, now)`.

- [ ] **Step 1: Tests:** `tier_for(HeroHurt{0.05})` is `None`, `{0.15}` is `Medium`; `BossKilled` and `FinalBossKilled` are `Large`; `LevelUp` is `None` (it happens every few seconds early; the game-feel skill says not to juice routine events); `scaled(Large, Off) == 0.0`.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement with the game-feel tier table: Medium ≈ trauma 0.4, Large ≈ 0.8, short and decaying; build `CameraShake` rows in code rather than from the DBC. Kills of normal enemies get nothing.
- [ ] **Step 4:** Confirm `apply_camera_shake` runs after `hero::place_camera` in survivors (otherwise the camera placement overwrites the offset); add the ordering if not.
- [ ] **Step 5:** Tests pass, APK builds, commit `Feedback: screen shake for big hits and boss kills, with a setting`.

### Task A7: Demo button on the main menu; Play is always the player's own run

Requested by the user on 2026-10-31: "add a demo button (that is your bot) and the normal mode is my test; now it just starts the bot automatically".

Why the bot starts by itself today: `scripts/balance/run_batch.sh` (and the working copy `/Volumes/AzerothBuild/balance/run_batch2.sh`) push `android-env.txt` with `WOW_SURVIVORS_AUTO=<class>` to the tablet's app folder and delete it only on the script's last line. Batches that were killed or lost the device never reached that line, so the file stayed and every launch reads it (`survivors/auto.rs` `from_env()`).

**Files:**
- Modify: `crates/benilla-app/src/survivors/auto.rs` (a constructor for a demo run beside `from_env`)
- Modify: the survivors main menu (find where the Play/Start button is built — `survivors/menu.rs` and/or `survivors/hud.rs`; `UiAction` is in `hud.rs:163`) to add a **Demo** button in the same WoW button style as its neighbours
- Modify: the HUD to show a small "Demo" label while a demo run plays
- Modify: `scripts/balance/run_batch.sh` and `/Volumes/AzerothBuild/balance/run_batch2.sh`: `trap` on EXIT/INT/TERM that removes `android-env.txt` from the device (keep the existing final `rm`; the trap makes it unconditional)

**Interfaces:**
- Produces: `pub(super) fn for_demo(class: usize, map: usize) -> Auto` in `auto.rs`, the same defaults as `from_env` with Normal difficulty, no `idle/all/late`, no screenshots.
- Consumes: the menu's current class and map selection (`super::Selection` / the selected character), and whatever starts a run today (`hud::start_run` or the Play button's handler).

**Behaviour:**
- Play: never inserts `Auto` (only `WOW_SURVIVORS_AUTO` in the environment can, as today, for the balance batches).
- Demo: inserts `Auto::for_demo(<selected class>, <selected map>)` and starts the run exactly as Play does. The demo uses the selected class and map, Normal difficulty.
- A demo run records nothing for the character: no completions, talents or records (the existing `record_completion` / records paths already skip when `Auto` exists — verify, don't duplicate).
- Leaving a demo run to the main menu (pause → Main Menu, or the run ending) removes the `Auto` resource, so the next Play is the player's own. The env-driven batch run (`from_env`) is unchanged.

- [ ] **Step 1: Tests:** `auto::tests::demo_defaults` — `for_demo(2, 1)` has class 2, map 1, `Difficulty::Normal`, `idle == all == late == false`, no shots dir. A test for the menu-exit cleanup if the cleanup is a plain function you can call on a `World` (Bevy 0.18 `World::new()` + `insert_resource` + run the system once); if it is not reasonably testable, say so in the report and describe the manual check instead.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement as above. Match the existing button code (look at how Play/Resume/Settings are spawned and handled); touch targets work through `survivors/touch.rs` the same way as the other menu buttons.
- [ ] **Step 4:** Tests pass; `bash -n` on both scripts; APK builds.
- [ ] **Step 5:** Commit `Menu: a Demo button runs the bot; Play is always yours`.

### Task A8: README — thanks, what this fork is, the Android build, the new modes, screenshots

Requested by the user on 2026-10-31: "update the readme file, saying thank you to the original creators, and explaining what we are doing, android build, new modes, put some screenshots".

This fork is `Zhalfir/azeroth-survivors-re`, branch `android` (this branch, `mobile-graphics`, merges into it). It is a personal, sideloaded, experimental project — say so plainly; it is not distributed and it ships no game data.

**Files:**
- Modify: `README.md` (keep the existing top banner, the "Not an original project" note and the existing sections that still hold; add, don't rewrite history)
- Create: `docs/screenshots/android/*.jpg` (new images)

**Content to add (plain, friendly English; the user is not a native speaker, so short sentences):**
1. **Thank you** section near the top: Aethus (the original *Azeroth Survivors*, wow.aethus.pl), the `azeroth-survivors-re` authors this was forked from (check `git log` / `git remote -v` / the existing README for the exact upstream name and link — OutThisLife/azeroth-survivors-re), samwhosung's `benilla` client and the `wenilla` fork (links already in the README / AGENTS.md), and Blizzard for World of Warcraft (data not included; you need your own 1.12.1 client data).
2. **What this fork does:** the native Android build (NativeActivity, no Gradle; touch controls; WoW-style buttons; app icon; graphics presets High/Medium/Low; persistent shader cache), and how to build and sideload it — from `crates/survivors-android/android/build-apk.sh` and `install.sh` (read both; list prerequisites: Rust + cargo-ndk, Android SDK/NDK, JDK 17; where the WoW `Data/` goes on the device: `/sdcard/Android/data/local.azeroth.survivors/files/Data`).
3. **New modes and systems** — read `docs/PROGRESSION.md` and `docs/BALANCE.md` and summarise only what is built: characters (up to 8), Normal / Heroic / Mythic passes, talents earned per map (51), tier sets with stats, records page, chests and potions, damage meter, combat log, settings (graphics, sound, damage numbers), the Demo button (bot) vs Play, balance work with the auto-player. Mark planned items (co-op, raids, legendaries, iPhone) as "planned", not done.
4. **Screenshots:** pick 6–8 good ones from `/Volumes/AzerothBuild/*.png` (look at them with the Read tool; prefer the `*s.png` already-scaled ones; cover: menu/character pick, combat with a horde, a boss, level-up cards, talents, settings or records). Convert to JPEG, long side 1600 px, quality ~80: `sips -s format jpeg -s formatOptions 80 -Z 1600 in.png --out docs/screenshots/android/<name>.jpg`. Keep each under ~400 KB. Lay them out in a 2-column table like the existing README does, with descriptive `alt` text.
5. A short **Status / roadmap** line linking `docs/superpowers/plans/2026-10-31-mobile-graphics.md`.

- [ ] **Step 1:** Draft the sections; check every claim against the code or docs it comes from (no feature described that isn't built).
- [ ] **Step 2:** Screenshots converted and placed; total added size under 3 MB.
- [ ] **Step 3:** Render-check: no broken relative links (`grep -o 'docs/[^")]*' README.md | xargs ls`).
- [ ] **Step 4:** Commit `README: thanks, the Android build, new modes, screenshots`.

---

## Phase B — on the tablet (measure, then fix)

Each task: run, read numbers, keep or revert, write the before/after in `docs/BALANCE.md` → "Performance".

### Task B1: Baseline

- [ ] Charge the tablet, wall charger, brightness at minimum, volume 0.
- [ ] Extend `run_batch2.sh` to log battery level and temperature at start and end of each run (`adb shell dumpsys battery`) and add a 2-minute rest between runs.
- [ ] Runs: Mythic Elwynn, 34 points + T2, Warrior and Hunter, High then Medium, cap 60, `WOW_GPU_MS=1`, ASTC on and off (`WOW_NO_ASTC=1`).
- [ ] Read `cpu50/gpu50`: if GPU frame time is near the frame time, the game is GPU-bound (Task B2); if CPU is, Task B3. Also verify the GPU meter reports on Adreno at all; if it reads zero, say so and use the CPU split only.

### Task B2: If GPU-bound

- [ ] Dynamic resolution: a controller that moves `world_long_side` between the preset's floor and ceiling to hold the cap, using `gpu_ms` (with GSR from A4 doing the upscale).
- [ ] Spell-effect overdraw: measure Consecration / Volley seconds; try drawing particles at half resolution or lowering `spellEffectLevel` only for the hero's own repeated effects.
- [ ] Shadows: check whether survivors enemies cast real shadow-map shadows on High; if so, use the engine's existing blob shadow (`blob_shadow.rs`) for the horde and keep real shadows for hero and bosses.

### Task B3: If CPU-bound

- [ ] Enemy pooling: hide and reuse dead enemies instead of `despawn` (`enemies.rs:602`), prewarmed at run start; measure spawn-second hitches.
- [ ] Animation LOD: update the bone palette (`benilla-world/src/rig_palette.rs`) at full rate near the hero, half or quarter rate for small or distant enemies.
- [ ] Per-system timing with `bevy` trace spans, then the biggest system first.

### Task B4: Heat

- [ ] Read Android's thermal headroom (`AThermal_getThermalHeadroom`, NDK, Android 12+) once a second; step the preset down one level above 0.85 and log it in the combat log. Never step up during a run.

### Task B5: Look, paid for with measured headroom (High preset only unless free)

- [ ] Per-zone grade: extend `grade::Grade` with a tint and per-zone values (warm Elwynn, cold Duskwood).
- [ ] Rim light on elites and bosses, and a short white hit flash: both need a per-instance tint value in the model shader (`benilla-world/src/model_render/batch.rs`); design that channel first.
- [ ] Dissolve on enemy death instead of the corpse popping out (noise texture, no `discard` on mobile tile GPUs; alpha instead).
- [ ] Each one measured; dropped from Medium and Low if it costs more than 1 ms.

### Task B6: Budgets

- [ ] `analyze.py` flags a run whose p50 frame time is above the cap or whose battery drop per 10 minutes is above the baseline; run it after every performance change.

---

## Phase C — iPhone (separate plan)

Safe-area insets for the notch and rounded corners (`game-ui-ux`), the Metal build and signing, getting the game data onto the phone, and Apple's MetalFX upscaler in place of GSR. ASTC from Task A5 carries over unchanged.
