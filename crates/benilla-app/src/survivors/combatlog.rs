//! The survivors combat log: what happened in a run, line by line, for finding out why a run went
//! the way it did — in the system log (target `survivors_log`, `adb logcat` on Android) and in
//! `benilla-config/Diagnostics/survivors-log.txt` (this launch; the last one is kept as `.prev`).
//!
//! It records the run's start, every card taken, each boss's arrival and death, the hero's
//! damage taken by source every five seconds with the final boss's state, the killing blow, and
//! the end.
//!
//! **The full run file**, for balance work: `Diagnostics/runs/run-<unix>-<map>-<difficulty>.jsonl`,
//! one JSON object per line, every run kept:
//! - `start` — class, race, zone, difficulty, character, seconds to the final boss;
//! - `event` — each line of the readable log above (cards, bosses, completions, the end);
//! - `build` — every ability and passive with its rank, on each change (`items`: none yet);
//! - `boss_hit` — each blow a boss lands on the hero: who, how much, health before;
//! - `tick` — every second of run time: hero health, level, XP gained, enemies alive, frames per
//!   second, and per spell its damage, hits, crits and casts; per enemy type the damage it dealt
//!   the hero (and how many blows), its kills and its spawns;
//! - `end` — won or lost, the killing blow, the totals.
//!
//! Single hits are folded into the second's tick: late in a run they come by the thousand a
//! second, and one line each would cost the phone frames and megabytes.

use std::sync::Mutex;

use bevy::prelude::*;

use super::enemies::Enemy;
use super::hero::Hero;
use super::{Phase, Run};

/// Lines waiting for the file.
static PENDING: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Damage the hero took since the last summary, by source.
static TAKEN: Mutex<Vec<(&'static str, f32)>> = Mutex::new(Vec::new());
/// The last blow that landed: who, how much, and the hero's health before it.
static LAST_BLOW: Mutex<Option<(&'static str, f32, f32)>> = Mutex::new(None);

/// Seconds of run time between summaries.
const SUMMARY_EVERY: f32 = 5.0;

/// One spell's second in the run file.
struct SpellRow {
    spell: u32,
    damage: f64,
    hits: u32,
    crits: u32,
    casts: u32,
}

/// What the current second gathered, written by [`tick`].
struct Tick {
    spells: Vec<SpellRow>,
    /// Per enemy type: damage dealt to the hero and blows landed.
    taken: Vec<(&'static str, f32, u32)>,
    kills: Vec<(&'static str, u32)>,
    spawns: Vec<(&'static str, u32)>,
    xp: f32,
}

static TICK: Mutex<Tick> = Mutex::new(Tick {
    spells: Vec::new(),
    taken: Vec::new(),
    kills: Vec::new(),
    spawns: Vec::new(),
    xp: 0.0,
});
/// The run file's pending lines, and the file itself while a run is on.
static RUN_LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());
static RUN_FILE: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

fn run_line(json: String) {
    if let Ok(mut lines) = RUN_LINES.lock() {
        lines.push(json);
    }
}

/// A string as a JSON string literal.
fn js(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn bump<K: PartialEq + Copy>(rows: &mut Vec<(K, u32)>, key: K) {
    match rows.iter_mut().find(|(k, _)| *k == key) {
        Some(row) => row.1 += 1,
        None => rows.push((key, 1)),
    }
}

fn spell_row(rows: &mut Vec<SpellRow>, spell: u32) -> &mut SpellRow {
    if let Some(i) = rows.iter().position(|r| r.spell == spell) {
        return &mut rows[i];
    }
    rows.push(SpellRow {
        spell,
        damage: 0.0,
        hits: 0,
        crits: 0,
        casts: 0,
    });
    rows.last_mut().expect("just pushed")
}

/// A run begins: open its file and write who, where and how hard.
pub(super) fn begin(
    class: &str,
    race: &str,
    map: &str,
    difficulty: &str,
    character: Option<u32>,
    run_secs: f32,
) {
    if let Ok(mut t) = TICK.lock() {
        t.spells.clear();
        t.taken.clear();
        t.kills.clear();
        t.spawns.clear();
        t.xp = 0.0;
    }
    let path = crate::local_state::home().map(|h| {
        let unix = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let slug: String = map
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        h.join("Diagnostics")
            .join("runs")
            .join(format!("run-{unix}-{slug}-{difficulty}.jsonl"))
    });
    if let Ok(mut file) = RUN_FILE.lock() {
        *file = path;
    }
    run_line(format!(
        "{{\"t\":0,\"type\":\"start\",\"class\":{},\"race\":{},\"map\":{},\"difficulty\":{},\"character\":{},\"run_secs\":{run_secs}}}",
        js(class),
        js(race),
        js(map),
        js(difficulty),
        character.map_or("null".to_string(), |c| c.to_string()),
    ));
}

/// A hit or a damage-over-time tick the hero's spell dealt.
pub(super) fn dealt(spell: u32, damage: f32, crit: bool) {
    if let Ok(mut t) = TICK.lock() {
        let row = spell_row(&mut t.spells, spell);
        row.damage += f64::from(damage);
        row.hits += 1;
        if crit {
            row.crits += 1;
        }
    }
}

/// An ability went off.
pub(super) fn cast(spell: u32) {
    if let Ok(mut t) = TICK.lock() {
        spell_row(&mut t.spells, spell).casts += 1;
    }
}

pub(super) fn spawned(name: &'static str) {
    if let Ok(mut t) = TICK.lock() {
        bump(&mut t.spawns, name);
    }
}

pub(super) fn killed(name: &'static str) {
    if let Ok(mut t) = TICK.lock() {
        bump(&mut t.kills, name);
    }
}

pub(super) fn xp(amount: f32) {
    if let Ok(mut t) = TICK.lock() {
        t.xp += amount;
    }
}

/// One line, stamped with the run's clock.
pub(super) fn event(elapsed: f32, msg: impl AsRef<str>) {
    let secs = elapsed.max(0.0);
    let line = format!(
        "[{:02}:{:04.1}] {}",
        (secs / 60.0) as u32,
        secs % 60.0,
        msg.as_ref()
    );
    info!(target: "survivors_log", "{line}");
    run_line(format!(
        "{{\"t\":{:.1},\"type\":\"event\",\"msg\":{}}}",
        secs,
        js(msg.as_ref())
    ));
    if let Ok(mut pending) = PENDING.lock() {
        pending.push(line);
    }
}

/// A blow on the hero: `taken` after armour, shield and immunity. A boss's blows are written
/// one by one as well.
pub(super) fn hurt(source: &'static str, taken: f32, hp_before: f32, boss: bool, elapsed: f32) {
    if taken <= 0.0 {
        return;
    }
    if let Ok(mut t) = TICK.lock() {
        match t.taken.iter_mut().find(|(s, ..)| *s == source) {
            Some(row) => {
                row.1 += taken;
                row.2 += 1;
            }
            None => t.taken.push((source, taken, 1)),
        }
    }
    if boss {
        run_line(format!(
            "{{\"t\":{elapsed:.1},\"type\":\"boss_hit\",\"from\":{},\"amount\":{taken:.1},\"hp_before\":{hp_before:.1}}}",
            js(source)
        ));
    }
    if let Ok(mut t) = TAKEN.lock() {
        match t.iter_mut().find(|(s, _)| *s == source) {
            Some(row) => row.1 += taken,
            None => t.push((source, taken)),
        }
    }
    if let Ok(mut last) = LAST_BLOW.lock() {
        *last = Some((source, taken, hp_before));
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Startup, open_file)
        .add_systems(
            Update,
            (summary, tick, build).run_if(in_state(Phase::Playing)),
        )
        .add_systems(OnEnter(Phase::GameOver), run_over)
        .add_systems(Last, (flush, flush_run));
}

fn path() -> Option<std::path::PathBuf> {
    crate::local_state::home().map(|h| h.join("Diagnostics").join("survivors-log.txt"))
}

/// Keep the last launch's log as `.prev` and start this one empty.
fn open_file() {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::rename(&path, path.with_extension("txt.prev"));
        let _ = std::fs::write(&path, "");
    }
}

/// Append what is pending, once a frame at most.
fn flush() {
    let lines = match PENDING.lock() {
        Ok(mut p) if !p.is_empty() => std::mem::take(&mut *p),
        _ => return,
    };
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = path() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            for line in lines {
                let _ = writeln!(f, "{line}");
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    drop(lines);
}

/// Append the run file's pending lines.
fn flush_run() {
    let lines = match RUN_LINES.lock() {
        Ok(mut p) if !p.is_empty() => std::mem::take(&mut *p),
        _ => return,
    };
    let path = RUN_FILE.lock().ok().and_then(|f| f.clone());
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = path {
        use std::io::Write;
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            for line in lines {
                let _ = writeln!(f, "{line}");
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    drop((lines, path));
}

/// Every second of run time, the second's numbers into the run file.
fn tick(
    run: Res<Run>,
    time: Res<Time<Real>>,
    mut state: Local<(f32, u32, f32)>,
    enemies: Query<&Enemy>,
    spells: Option<Res<crate::ui_action::Spells>>,
) {
    let (next_at, frames, since) = &mut *state;
    *frames += 1;
    if run.elapsed < *next_at {
        if run.elapsed + 1.0 < *next_at {
            *next_at = 0.0; // a new run started
        }
        return;
    }
    let now = time.elapsed_secs();
    let fps = if now > *since {
        *frames as f32 / (now - *since)
    } else {
        0.0
    };
    *next_at = run.elapsed.floor() + 1.0;
    *frames = 0;
    *since = now;
    let Ok(mut t) = TICK.lock() else { return };
    let spell_json: Vec<String> = t
        .spells
        .iter()
        .map(|r| {
            let name = super::hud::spell_name(spells.as_deref(), r.spell, "other");
            format!(
                "{}:{{\"id\":{},\"damage\":{:.0},\"hits\":{},\"crits\":{},\"casts\":{}}}",
                js(&name),
                r.spell,
                r.damage,
                r.hits,
                r.crits,
                r.casts
            )
        })
        .collect();
    let taken_json: Vec<String> = t
        .taken
        .iter()
        .map(|(s, d, n)| format!("{}:{{\"damage\":{d:.0},\"blows\":{n}}}", js(s)))
        .collect();
    let counts = |rows: &[(&'static str, u32)]| -> String {
        rows.iter()
            .map(|(s, n)| format!("{}:{n}", js(s)))
            .collect::<Vec<_>>()
            .join(",")
    };
    let alive = enemies.iter().filter(|e| !e.dead).count();
    let line = format!(
        "{{\"t\":{:.0},\"type\":\"tick\",\"hp\":{:.0},\"max_hp\":{:.0},\"level\":{},\"xp_gained\":{:.1},\"kills_total\":{},\"alive\":{alive},\"fps\":{fps:.0},\"spells\":{{{}}},\"taken\":{{{}}},\"kills\":{{{}}},\"spawns\":{{{}}}}}",
        run.elapsed,
        run.hp,
        run.stats().max_hp,
        run.level,
        t.xp,
        run.kills,
        spell_json.join(","),
        taken_json.join(","),
        counts(&t.kills),
        counts(&t.spawns),
    );
    t.spells.clear();
    t.taken.clear();
    t.kills.clear();
    t.spawns.clear();
    t.xp = 0.0;
    drop(t);
    run_line(line);
}

/// The build into the run file whenever it changes: every ability and passive with its rank.
fn build(run: Res<Run>, mut seen: Local<Option<u32>>) {
    if *seen == Some(run.version) {
        return;
    }
    *seen = Some(run.version);
    let abilities: Vec<String> = run
        .abilities
        .iter()
        .map(|o| format!("[{},{}]", js(o.def.key), o.level))
        .collect();
    let passives: Vec<String> = run
        .passives
        .iter()
        .map(|(p, n)| format!("[{},{n}]", js(p.key)))
        .collect();
    run_line(format!(
        "{{\"t\":{:.1},\"type\":\"build\",\"level\":{},\"abilities\":[{}],\"passives\":[{}],\"items\":[]}}",
        run.elapsed,
        run.level,
        abilities.join(","),
        passives.join(",")
    ));
}

/// Every five seconds of play: the hero, what hurt it, and the final boss if it stands.
fn summary(
    run: Res<Run>,
    mut next_at: Local<f32>,
    enemies: Query<(&Enemy, &Transform)>,
    hero: Query<&Transform, With<Hero>>,
) {
    if run.elapsed < *next_at {
        if run.elapsed + SUMMARY_EVERY < *next_at {
            *next_at = 0.0; // a new run started
        }
        return;
    }
    *next_at = run.elapsed + SUMMARY_EVERY;
    let alive = enemies.iter().filter(|(e, _)| !e.dead).count();
    let mut taken = TAKEN
        .lock()
        .map(|mut t| std::mem::take(&mut *t))
        .unwrap_or_default();
    taken.sort_by(|a, b| b.1.total_cmp(&a.1));
    let hurt: Vec<String> = taken
        .iter()
        .take(3)
        .map(|(s, d)| format!("{s} {d:.0}"))
        .collect();
    let max_hp = run.stats().max_hp;
    let mut line = format!(
        "hero {:.0}/{:.0} hp, level {}, {} kills, {} alive; took {}",
        run.hp,
        max_hp,
        run.level,
        run.kills,
        alive,
        if hurt.is_empty() {
            "nothing".to_string()
        } else {
            hurt.join(", ")
        }
    );
    if let Some((boss, tf)) = enemies.iter().find(|(e, _)| e.final_boss && !e.dead) {
        let dist = hero
            .single()
            .map(|h| super::units::flat(tf.translation - h.translation).length())
            .unwrap_or(f32::NAN);
        line.push_str(&format!(
            "; final boss {} {:.0}% ({:.0}/{:.0}), {:.0} yd away",
            boss.def.name,
            boss.hp / boss.max_hp.max(1.0) * 100.0,
            boss.hp,
            boss.max_hp,
            dist
        ));
    }
    event(run.elapsed, line);
}

fn run_over(run: Res<Run>) {
    if run.won {
        event(
            run.elapsed,
            format!(
                "VICTORY: level {}, {} kills, {:.0} damage",
                run.level, run.kills, run.damage_done
            ),
        );
    } else {
        let blow = LAST_BLOW.lock().ok().and_then(|b| *b);
        let cause = match blow {
            Some((who, dmg, before)) => {
                format!("killed by {who}: {dmg:.0} damage with {before:.0} hp left")
            }
            None => "no blow recorded".into(),
        };
        event(
            run.elapsed,
            format!(
                "DEFEAT: {cause}; level {}, {} kills, {:.0} damage",
                run.level, run.kills, run.damage_done
            ),
        );
    }
    let blow = LAST_BLOW.lock().ok().and_then(|b| *b);
    run_line(format!(
        "{{\"t\":{:.1},\"type\":\"end\",\"won\":{},\"level\":{},\"kills\":{},\"damage\":{:.0},\"killing_blow\":{}}}",
        run.elapsed,
        run.won,
        run.level,
        run.kills,
        run.damage_done,
        match blow {
            Some((who, dmg, before)) if !run.won => format!(
                "{{\"from\":{},\"amount\":{dmg:.1},\"hp_before\":{before:.1}}}",
                js(who)
            ),
            _ => "null".to_string(),
        }
    ));
    if let Ok(mut b) = LAST_BLOW.lock() {
        *b = None;
    }
    if let Ok(mut t) = TAKEN.lock() {
        t.clear();
    }
}
