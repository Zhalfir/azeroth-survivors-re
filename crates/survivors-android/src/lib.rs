//! The Android launcher: `android_main` (from `bevy_main`) points `$WOW_DATA` and
//! `$BENILLA_HOME` into the app's storage, then hands the build stamp to the survivors mode.
//!
//! The MPQs are far too large for the APK, so they are pushed beside it, into the app-specific
//! external folder, which needs no storage permission:
//! `adb push Data /sdcard/Android/data/<package>/files/Data` (see `android/build-apk.sh`).

#![cfg(target_os = "android")]

use benilla_app::BuildId;
use bevy::android::ANDROID_APP;

#[bevy::prelude::bevy_main]
fn main() {
    if let Some(app) = ANDROID_APP.get() {
        // The state folder sits beside `Data/`, not in internal storage: an APK built here is not
        // debuggable, and this way `adb` (and a PC over USB) can still read and back it up.
        if let Some(files) = app
            .external_data_path()
            .or_else(|| app.internal_data_path())
        {
            // SAFETY: nothing else runs yet; the engine's threads start inside `run_survivors`.
            unsafe {
                std::env::set_var("WOW_DATA", files.join("Data"));
                std::env::set_var("BENILLA_HOME", files.join("benilla-config"));
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
