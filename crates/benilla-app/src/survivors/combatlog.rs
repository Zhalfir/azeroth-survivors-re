//! The survivors combat log: what happened in a run, line by line, for finding out why a run went
//! the way it did — in the system log (target `survivors_log`, `adb logcat` on Android) and in
//! `benilla-config/Diagnostics/survivors-log.txt` (this launch; the last one is kept as `.prev`).
//!
//! It records the run's start, every card taken, each boss's arrival and death, the hero's
//! damage taken by source every five seconds with the final boss's state, the killing blow, and
//! the end.

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
    if let Ok(mut pending) = PENDING.lock() {
        pending.push(line);
    }
}

/// A blow on the hero: `taken` after armour, shield and immunity.
pub(super) fn hurt(source: &'static str, taken: f32, hp_before: f32) {
    if taken <= 0.0 {
        return;
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
        .add_systems(Update, summary.run_if(in_state(Phase::Playing)))
        .add_systems(OnEnter(Phase::GameOver), run_over)
        .add_systems(Last, flush);
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
    if let Ok(mut b) = LAST_BLOW.lock() {
        *b = None;
    }
    if let Ok(mut t) = TAKEN.lock() {
        t.clear();
    }
}
