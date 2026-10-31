//! macOS thread QoS: keep frame-critical threads on the P-cores under system load. Apple Silicon
//! schedules by QoS class, and the main thread is user-interactive, but every worker Bevy spawns
//! starts at default, the class of `rustc`, so a background build starves the frame's workers.
//! `pthread_set_qos_class_self_np` is not in the `libc` crate, so the extern lives here; everything
//! is a no-op off macOS.

use bevy::prelude::*;
use bevy::render::{Render, RenderApp, RenderSystems};

/// The QoS classes used here, as Darwin's `qos_class_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum QosClass {
    /// Work on the critical path of the current frame (compute pool, render thread).
    UserInteractive = 0x21,
    /// Work the frame is waiting on soon but not this frame (asset IO, async compute, net IO).
    UserInitiated = 0x19,
    /// Darwin's default class, for a thread that blocks for seconds: Metal compiles at the calling
    /// thread's QoS, and Apple prescribes this class for pipeline prewarming (WWDC25, "Explore
    /// Metal 4 games").
    Default = 0x15,
}

/// Set by `pipe_warm` while the covered pipeline-warm burst blocks the render thread in Metal
/// compilation; [`promote_render_thread`] then drops that thread to `Default` from inside, since
/// only a thread can set its own QoS.
pub static COMPILE_BURST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Promote the calling thread to `class`. Safe to call repeatedly; logs once on failure.
pub fn promote_current_thread(class: QosClass) {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
        }
        let rc = unsafe { pthread_set_qos_class_self_np(class as u32, 0) };
        if rc != 0 {
            warn_once!("thread QoS promotion to {class:?} failed (rc={rc})");
        }
    }
    #[cfg(target_os = "android")]
    android::promote(class);
    #[cfg(not(any(target_os = "macos", target_os = "android")))]
    let _ = class;
}

/// Android has no QoS classes. Its levers: a display nice value for frame-critical threads, and
/// the Performance Hint API (ADPF, Android 13+), which takes a session of those threads plus each
/// frame's actual work time against a target and lets the OS raise clocks and place the threads.
/// Google advises against pinning cores yourself: the right core type varies by device and many
/// devices ignore a game's affinity (developer.android.com/games/optimize/adpf). On the Galaxy
/// Tab S9+ the survivors main thread ran at ~81 % of one core while five cores idled
/// (2026-10-31). The API is looked up at runtime (`dlsym`), so an older device just runs without.
#[cfg(target_os = "android")]
mod android {
    use std::ffi::c_void;
    use std::sync::Mutex;

    use super::QosClass;
    use bevy::prelude::*;

    /// `THREAD_PRIORITY_DISPLAY`: what an app's render threads may take (`URGENT_DISPLAY`, -8,
    /// is system-only).
    const DISPLAY_NICE: i32 = -4;

    /// The frame threads registered so far (main, render, compute workers), with a generation
    /// that moves on every change. Audio threads promote too but never register: their work is not
    /// the frame's, and a rebuilt output stream would leave a dead id in the session.
    static FRAME_THREADS: Mutex<(u64, Vec<i32>)> = Mutex::new((0, Vec::new()));

    pub(super) fn promote(class: QosClass) {
        if class != QosClass::UserInteractive {
            return;
        }
        let tid = unsafe { libc::gettid() };
        if unsafe { libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, DISPLAY_NICE) } != 0 {
            warn_once!("thread priority: setpriority({DISPLAY_NICE}) refused");
        }
    }

    pub(super) fn register_frame_thread() {
        promote(QosClass::UserInteractive);
        let tid = unsafe { libc::gettid() };
        if let Ok(mut frame) = FRAME_THREADS.lock() {
            if !frame.1.contains(&tid) {
                frame.1.push(tid);
                frame.0 += 1;
            }
        }
    }

    /// A thread id still alive in this process.
    fn alive(tid: i32) -> bool {
        std::path::Path::new(&format!("/proc/self/task/{tid}")).exists()
    }

    type GetManager = unsafe extern "C" fn() -> *mut c_void;
    type CreateSession = unsafe extern "C" fn(*mut c_void, *const i32, usize, i64) -> *mut c_void;
    type SessionNanos = unsafe extern "C" fn(*mut c_void, i64) -> i32;
    type SetThreads = unsafe extern "C" fn(*mut c_void, *const i32, usize) -> i32;
    type CloseSession = unsafe extern "C" fn(*mut c_void);

    /// The ADPF entry points, resolved once from `libandroid.so`.
    struct Api {
        manager: *mut c_void,
        create: CreateSession,
        report: SessionNanos,
        target: SessionNanos,
        /// Android 14+; without it a grown thread set reopens the session.
        set_threads: Option<SetThreads>,
        close: CloseSession,
    }

    struct Session {
        api: Api,
        session: *mut c_void,
        /// The [`FRAME_THREADS`] generation the session was last given.
        generation: u64,
        target_ns: i64,
        /// Consecutive failures to (re)build the session; ADPF is given up after a few.
        failures: u8,
    }

    // SAFETY: the session handle is only touched under `SESSION`'s lock; the NDK documents the
    // session as usable from any thread.
    unsafe impl Send for Session {}

    static SESSION: Mutex<Option<Option<Session>>> = Mutex::new(None);

    fn api() -> Option<Api> {
        unsafe {
            let lib = libc::dlopen(c"libandroid.so".as_ptr(), libc::RTLD_NOW);
            if lib.is_null() {
                return None;
            }
            let sym = |name: &std::ffi::CStr| {
                let p = libc::dlsym(lib, name.as_ptr());
                (!p.is_null()).then_some(p)
            };
            let get: GetManager = std::mem::transmute(sym(c"APerformanceHint_getManager")?);
            let manager = get();
            if manager.is_null() {
                return None;
            }
            Some(Api {
                manager,
                create: std::mem::transmute(sym(c"APerformanceHint_createSession")?),
                report: std::mem::transmute(sym(c"APerformanceHint_reportActualWorkDuration")?),
                target: std::mem::transmute(sym(c"APerformanceHint_updateTargetWorkDuration")?),
                set_threads: sym(c"APerformanceHint_setThreads").map(|p| std::mem::transmute(p)),
                close: std::mem::transmute(sym(c"APerformanceHint_closeSession")?),
            })
        }
    }

    /// One frame's actual work against its target, both in nanoseconds; called once a frame.
    pub(super) fn report_frame(work_ns: i64, target_ns: i64) {
        // `WOW_ADPF=0` (`android-env.txt`) turns the hints off: the A/B lever.
        static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *OFF.get_or_init(|| std::env::var("WOW_ADPF").as_deref() == Ok("0")) {
            return;
        }
        if work_ns <= 0 || target_ns <= 0 {
            return;
        }
        let Ok(mut slot) = SESSION.lock() else { return };
        // Resolve once; `Some(None)` means no ADPF (or given up), so stop trying.
        let state = slot.get_or_insert_with(|| {
            let api = api();
            if api.is_none() {
                info!("performance hint: unavailable on this device (Android 13+ needed)");
            }
            api.map(|api| Session {
                api,
                session: std::ptr::null_mut(),
                generation: u64::MAX,
                target_ns: 0,
                failures: 0,
            })
        });
        let Some(s) = state.as_mut() else { return };
        let generation = FRAME_THREADS.lock().map(|f| f.0).unwrap_or(0);
        if s.session.is_null() || s.generation != generation {
            // The thread set changed (or there is no session yet): copy it, drop dead ids, and
            // hand it over, by `setThreads` when the device has it, else by a new session.
            let tids: Vec<i32> = FRAME_THREADS
                .lock()
                .map(|f| f.1.iter().copied().filter(|&t| alive(t)).collect())
                .unwrap_or_default();
            if tids.is_empty() {
                return;
            }
            let updated = !s.session.is_null()
                && s.api.set_threads.is_some_and(|set| unsafe {
                    set(s.session, tids.as_ptr(), tids.len()) == 0
                });
            if !updated {
                unsafe {
                    if !s.session.is_null() {
                        (s.api.close)(s.session);
                    }
                    s.session =
                        (s.api.create)(s.api.manager, tids.as_ptr(), tids.len(), target_ns);
                }
                if s.session.is_null() {
                    s.failures += 1;
                    warn!("performance hint: createSession refused ({} of 5)", s.failures);
                    if s.failures >= 5 {
                        *state = None;
                    }
                    return;
                }
                info!("performance hint: session over {} frame threads", tids.len());
                s.target_ns = target_ns;
            }
            s.failures = 0;
            s.generation = generation;
        }
        unsafe {
            if s.target_ns != target_ns {
                (s.api.target)(s.session, target_ns);
                s.target_ns = target_ns;
            }
            (s.api.report)(s.session, work_ns);
        }
    }
}

/// Promote the calling thread as frame-critical (`UserInteractive`) and, on Android, add it to
/// the performance-hint session's threads. For the main thread, the render thread and the compute
/// pool's workers only; other user-interactive threads (audio) call [`promote_current_thread`].
pub fn register_frame_thread() {
    #[cfg(target_os = "android")]
    android::register_frame_thread();
    #[cfg(not(target_os = "android"))]
    promote_current_thread(QosClass::UserInteractive);
}

/// Report one frame's actual work time and its target to the OS's performance hints (Android
/// ADPF; a no-op elsewhere). Call once a frame, from any thread.
pub fn report_frame(work: std::time::Duration, target: std::time::Duration) {
    #[cfg(target_os = "android")]
    android::report_frame(work.as_nanos() as i64, target.as_nanos() as i64);
    #[cfg(not(target_os = "android"))]
    let _ = (work, target);
}

/// Promotes the pipelined-rendering thread, which Bevy spawns with no hook. The exclusive system
/// runs on whichever thread drives the render schedule, so it re-checks each frame behind a
/// thread-local latch; it sits in `ExtractCommands`, where its barrier has nothing to drain.
pub struct ThreadQosPlugin;

impl Plugin for ThreadQosPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.add_systems(
            Render,
            promote_render_thread.in_set(RenderSystems::ExtractCommands),
        );
    }
}

/// The calling thread's QoS class, for the tests.
#[cfg(all(test, target_os = "macos"))]
fn current_thread_qos() -> Option<u32> {
    unsafe extern "C" {
        fn pthread_self() -> *mut core::ffi::c_void;
        fn pthread_get_qos_class_np(
            thread: *mut core::ffi::c_void,
            qos_class: *mut u32,
            relative_priority: *mut i32,
        ) -> i32;
    }
    let mut qos: u32 = 0;
    let rc = unsafe { pthread_get_qos_class_np(pthread_self(), &mut qos, core::ptr::null_mut()) };
    (rc == 0).then_some(qos)
}

fn promote_render_thread(_world: &mut World) {
    // Frame-critical, or default while it is the shader compiler's caller under a cover.
    let want = if COMPILE_BURST.load(std::sync::atomic::Ordering::Relaxed) {
        QosClass::Default
    } else {
        QosClass::UserInteractive
    };
    std::thread_local! {
        static APPLIED: std::cell::Cell<Option<QosClass>> = const { std::cell::Cell::new(None) };
    }
    APPLIED.with(|p| {
        if p.get() != Some(want) {
            if want == QosClass::UserInteractive {
                register_frame_thread();
            } else {
                promote_current_thread(want);
            }
            debug!(
                "thread QoS: render-schedule thread {:?} → {want:?}",
                std::thread::current().name().unwrap_or("<unnamed>")
            );
            p.set(Some(want));
        }
    });
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn promotion_applies_to_spawned_thread() {
        for class in [QosClass::UserInteractive, QosClass::UserInitiated] {
            let observed = std::thread::spawn(move || {
                let before = current_thread_qos();
                promote_current_thread(class);
                (before, current_thread_qos())
            })
            .join()
            .unwrap();
            // A bare std thread spawns at default (0x15).
            assert_eq!(observed.0, Some(0x15), "spawned thread not default-QoS");
            assert_eq!(observed.1, Some(class as u32), "promotion did not apply");
        }
    }

    /// kira's decode threads, spawned from the promoted main thread, land at default too, which is
    /// why `sound::mixer::PromotingSource` promotes from inside.
    #[test]
    fn qos_is_not_inherited_by_spawned_threads() {
        let (parent, child) = std::thread::spawn(|| {
            promote_current_thread(QosClass::UserInteractive);
            let child = std::thread::spawn(current_thread_qos).join().unwrap();
            (current_thread_qos(), child)
        })
        .join()
        .unwrap();
        assert_eq!(parent, Some(QosClass::UserInteractive as u32));
        assert_eq!(child, Some(0x15), "QoS inherited — PromotingSource is moot");
    }
}
