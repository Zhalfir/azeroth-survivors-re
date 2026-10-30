//! The Android launcher: `android_main` (from `bevy_main`) points `$WOW_DATA` and
//! `$BENILLA_HOME` into the app's storage, then hands the build stamp to the survivors mode.
//!
//! The MPQs are far too large for the APK, so they are pushed beside it, into the app-specific
//! external folder, which needs no storage permission:
//! `adb push Data /sdcard/Android/data/<package>/files/Data` (see `android/build-apk.sh`).
//!
//! The engine's levers are environment variables, which an Android launch cannot pass, so an
//! optional `android-env.txt` in the same folder supplies them: one `KEY=VALUE` per line, `#`
//! comments. Edit it with `adb push`, no rebuild; e.g. `WOW_FPS_JOURNAL=/sdcard/Android/data/
//! local.azeroth.survivors/files/fps.csv` records frame times.

#![cfg(target_os = "android")]

use std::path::Path;

use benilla_app::BuildId;
use bevy::android::ANDROID_APP;

/// The long side the 3D world renders at by default: Full HD. A tablet's native 2960 px is
/// 2.4× the pixels for a picture the eye barely tells apart, and the world is GPU-bound. The UI
/// stays native. `WOW_RENDER_SCALE` in `android-env.txt` overrides it.
const WORLD_LONG_SIDE: f32 = 1920.0;

#[bevy::prelude::bevy_main]
fn main() {
    if let Some(app) = ANDROID_APP.get() {
        // The state folder sits beside `Data/`, not in internal storage: an APK built here is not
        // debuggable, and this way `adb` (and a PC over USB) can still read and back it up.
        if let Some(files) = app
            .external_data_path()
            .or_else(|| app.internal_data_path())
        {
            // SAFETY (every `set_var` here): nothing else runs yet; the engine's threads start
            // inside `run_survivors`.
            unsafe {
                std::env::set_var("WOW_DATA", files.join("Data"));
                std::env::set_var("BENILLA_HOME", files.join("benilla-config"));
            }
            load_env_file(&files.join("android-env.txt"));
        }
        if std::env::var_os("WOW_RENDER_SCALE").is_none() {
            let config = app.config();
            let long_dp = config.screen_width_dp().max(config.screen_height_dp());
            if let (Some(dp), Some(dpi)) = (long_dp, config.density()) {
                let long_px = dp as f32 * dpi as f32 / 160.0;
                if long_px > WORLD_LONG_SIDE {
                    let scale = WORLD_LONG_SIDE / long_px;
                    unsafe { std::env::set_var("WOW_RENDER_SCALE", format!("{scale:.3}")) };
                }
            }
        }
    }
    let _ = benilla_app::run_survivors(BuildId {
        version: env!("CARGO_PKG_VERSION"),
        describe: env!("BENILLA_GIT_DESCRIBE"),
        sha: env!("BENILLA_GIT_SHA"),
        short: env!("BENILLA_GIT_SHORT"),
        date: env!("BENILLA_GIT_DATE"),
        profile: env!("BENILLA_PROFILE"),
        project_dir: env!("BENILLA_PROJECT_DIR"),
        ..Default::default()
    });
    // Android keeps the process after the activity is destroyed and runs `android_main` again in
    // it on the next launch, where bevy_winit cannot build a second event loop and panics. The
    // game holds no state worth keeping across that, so the process ends with its activity.
    std::process::exit(0);
}

/// `KEY=VALUE` lines into the environment; a missing file is no overrides.
fn load_env_file(path: &Path) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            unsafe { std::env::set_var(key.trim(), value.trim()) };
        }
    }
}
