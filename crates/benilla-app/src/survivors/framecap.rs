//! The frame-rate cap: 30, 60 (default) or uncapped. The tablet's screen is 120 Hz, so an
//! uncapped early game, which is light, may render up to 120 fps and burn battery on frames
//! nobody needs; 60 halves that and still looks smooth. A setting (`super::settings`), kept in
//! `benilla-config/framecap.txt`.
//!
//! The cap sleeps the rest of the frame in `Last`, ordered after [`super::frametime::frame_end`]:
//! the combat log's `cpu_ms` is stamped there, so the sleep never counts as CPU work. The sleep is
//! measured from the frame's start in `First` (the same stamp), so the wait on the render thread
//! counts as time spent, and a frame already slower than the cap does not sleep at all.

use std::time::Duration;

use bevy::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum FrameCap {
    Thirty,
    Sixty,
    Uncapped,
}

static FRAME_CAP: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);

impl FrameCap {
    pub(super) fn current() -> Self {
        match FRAME_CAP.load(std::sync::atomic::Ordering::Relaxed) {
            0 => Self::Thirty,
            2 => Self::Uncapped,
            _ => Self::Sixty,
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Thirty => "30",
            Self::Sixty => "60",
            Self::Uncapped => "Uncapped",
        }
    }

    /// 30 → 60 → Uncapped → 30.
    pub(super) fn next(self) -> Self {
        match self {
            Self::Thirty => Self::Sixty,
            Self::Sixty => Self::Uncapped,
            Self::Uncapped => Self::Thirty,
        }
    }

    /// The frame time the cap aims for, 97% of the nominal period; `None` when uncapped. Exactly
    /// 1/60 s is exactly two refreshes of a 120 Hz panel (30: four), a knife edge: a late wake or
    /// a 119.9 Hz panel puts the frame past the second vblank and Fifo waits for a third, an
    /// occasional 25 ms frame. Aiming a little under keeps the frame inside its refreshes; vsync
    /// owns the exact beat and the cap only stops the game from running ahead of it.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(super) fn target(self) -> Option<Duration> {
        let nominal = match self {
            Self::Thirty => 1.0 / 30.0,
            Self::Sixty => 1.0 / 60.0,
            Self::Uncapped => return None,
        };
        Some(Duration::from_secs_f64(nominal * 0.97))
    }

    fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "30" => Self::Thirty,
            "uncapped" => Self::Uncapped,
            _ => Self::Sixty,
        }
    }

    fn path() -> Option<std::path::PathBuf> {
        crate::local_state::home().map(|h| h.join("framecap.txt"))
    }

    /// The saved choice into force; at startup.
    pub(super) fn load() {
        let cap = Self::path()
            .and_then(|p| crate::local_state::read_to_string(&p).ok())
            .map(|s| Self::parse(&s))
            .unwrap_or(Self::Sixty);
        FRAME_CAP.store(cap as u8, std::sync::atomic::Ordering::Relaxed);
    }

    /// The next cap, in force and saved.
    pub(super) fn cycle() -> Self {
        let next = Self::current().next();
        FRAME_CAP.store(next as u8, std::sync::atomic::Ordering::Relaxed);
        if let Some(path) = Self::path() {
            let _ = crate::local_state::write_atomic(&path, next.name());
        }
        next
    }
}

/// How long to sleep after `spent` of work and waiting to make the frame `target` long, or `None`
/// when the frame already ran over it. Uncapped has no target and never reaches this.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(super) fn sleep_for(target: Duration, spent: Duration) -> Option<Duration> {
    target.checked_sub(spent).filter(|d| !d.is_zero())
}

pub(super) fn plugin(app: &mut App) {
    FrameCap::load();
    // wasm has no thread to sleep, and the browser paces its own frames.
    #[cfg(not(target_arch = "wasm32"))]
    app.add_systems(
        Last,
        (hint_frame, hold_frame)
            .chain()
            .after(super::frametime::frame_end),
    );
    #[cfg(target_arch = "wasm32")]
    let _ = app;
}

/// The frame's work so far against the cap's target, to the OS's performance hints (Android
/// ADPF, `benilla_world::thread_qos::report_frame`), before the cap's sleep so the sleep never
/// counts as work. Uncapped has no target to hint, so nothing is reported (a made-up 60 fps
/// target would let the OS lower clocks on a 120 Hz uncapped run).
#[cfg(not(target_arch = "wasm32"))]
fn hint_frame(samples: Res<super::frametime::FrameSamples>) {
    let (Some(started), Some(target)) = (samples.frame_started(), FrameCap::current().target())
    else {
        return;
    };
    benilla_world::thread_qos::report_frame(started.elapsed(), target);
}

/// Sleep out the rest of the frame: coarse `thread::sleep` to 1 ms short of the target (the OS
/// wakes a sleeper late by about that much), then spin the last millisecond.
#[cfg(not(target_arch = "wasm32"))]
fn hold_frame(samples: Res<super::frametime::FrameSamples>) {
    let Some(started) = samples.frame_started() else {
        return;
    };
    let Some(target) = FrameCap::current().target() else {
        return;
    };
    let Some(rest) = sleep_for(target, started.elapsed()) else {
        return;
    };
    if let Some(coarse) = rest.checked_sub(Duration::from_millis(1)) {
        std::thread::sleep(coarse);
    }
    while started.elapsed() < target {
        std::hint::spin_loop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(x: f64) -> Duration {
        Duration::from_secs_f64(x / 1000.0)
    }

    #[test]
    fn sleep_for() {
        let target = FrameCap::Sixty.target().unwrap();
        let rest = super::sleep_for(target, ms(10.0)).unwrap();
        assert!(
            (rest.as_secs_f64() * 1000.0 - 6.167).abs() < 0.01,
            "{rest:?}"
        );
        assert_eq!(super::sleep_for(target, ms(20.0)), None);
        assert_eq!(super::sleep_for(target, target), None);
        let thirty = super::sleep_for(FrameCap::Thirty.target().unwrap(), ms(10.0)).unwrap();
        assert!((thirty.as_secs_f64() * 1000.0 - 22.333).abs() < 0.01);
    }

    #[test]
    fn uncapped_never_sleeps() {
        assert_eq!(FrameCap::Uncapped.target(), None);
    }

    #[test]
    fn cycle_order_and_parse() {
        assert_eq!(FrameCap::Thirty.next(), FrameCap::Sixty);
        assert_eq!(FrameCap::Sixty.next(), FrameCap::Uncapped);
        assert_eq!(FrameCap::Uncapped.next(), FrameCap::Thirty);
        for c in [FrameCap::Thirty, FrameCap::Sixty, FrameCap::Uncapped] {
            assert_eq!(FrameCap::parse(c.name()), c);
        }
        assert_eq!(FrameCap::parse("garbage"), FrameCap::Sixty);
    }
}
