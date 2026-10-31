//! The world's upscaler: "Sharp" (default) runs Snapdragon Game Super Resolution 1 where a world
//! rendered below the window is stretched to it; "Plain" keeps the bilinear stretch. A setting
//! (`super::settings`), kept in `benilla-config/upscale.txt`.
//!
//! Where the stretch happens: the Android presets (`super::graphics`) set `renderScale` to the
//! world's long side over the window's, so the world camera's main texture is `window × scale`
//! (`world_backdrop.rs` `render_target_for`), and nothing resamples it in the world's own graph.
//! The player-UI camera draws the world's FFXGlow combine as the first draw of its native-size
//! pass (`benilla_world::ffx_glow` `FfxTransparent2dNode`, the [`FfxBackdrop`] ground), and the
//! combine's one `textureSample` of that main texture through a linear sampler is the upscale
//! (`shaders/ffx_glow.wgsl` `combine_body`). GSR replaces that tap ([`FfxUpscale`]), inside the
//! same draw, so it costs no extra pass or tile store, and the HUD, drawn after it in the same
//! pass at native size, is never resampled.
//!
//! Not a `FullscreenMaterial` on the world camera like [`super::grade`]: that pass runs at the
//! world's size and writes the world's size, so it can sharpen but has no pixels to upscale into.
//!
//! Every preset whose world is smaller than the window upscales, High included: on the Adreno 740
//! tablet (2560×1600 or 2800×1752) High's 1920 px world is still about 0.7 of the screen, as soft
//! under a bilinear stretch as Medium's, and GSR's price is per window pixel, the same at every
//! preset (Qualcomm's README: 0.29 ms against bilinear's 0.16 ms, 1240×576 → 2400×1600 on an
//! SD8Gen2). A world at or above the window (desktop's default `renderScale` 1, or
//! `$WOW_RENDER_SCALE` ≥ 1) keeps the plain tap: there is nothing to upscale.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use benilla_world::ffx_glow::FfxUpscale;
#[cfg(doc)]
use benilla_world::ffx_glow::FfxBackdrop;
use benilla_world::view::WorldCamera;

use crate::world_backdrop::WorldBackdrop;

/// GSR's edge sharpness: Qualcomm's default, the top of its 1..=2 range.
const EDGE_SHARPNESS: f32 = 2.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Upscaler {
    Sharp,
    Plain,
}

/// The choice in force, read by the render-facing system each frame; a static like the frame cap,
/// so the settings row and the system share it without a resource.
static UPSCALER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

impl Upscaler {
    pub(super) fn current() -> Self {
        if UPSCALER.load(std::sync::atomic::Ordering::Relaxed) {
            Self::Sharp
        } else {
            Self::Plain
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Sharp => "Sharp",
            Self::Plain => "Plain",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Sharp => Self::Plain,
            Self::Plain => Self::Sharp,
        }
    }

    /// Anything but "plain" is Sharp, the default.
    fn parse(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("plain") {
            Self::Plain
        } else {
            Self::Sharp
        }
    }

    fn path() -> Option<std::path::PathBuf> {
        crate::local_state::home().map(|h| h.join("upscale.txt"))
    }

    fn store(self) {
        UPSCALER.store(self == Self::Sharp, std::sync::atomic::Ordering::Relaxed);
    }

    /// The saved choice into force; at startup.
    fn load() {
        Self::path()
            .and_then(|p| crate::local_state::read_to_string(&p).ok())
            .map_or(Self::Sharp, |s| Self::parse(&s))
            .store();
    }

    /// The other choice, in force and saved.
    pub(super) fn cycle() -> Self {
        let next = Self::current().next();
        next.store();
        if let Some(path) = Self::path() {
            let _ = crate::local_state::write_atomic(&path, next.name());
        }
        next
    }
}

/// The upscale on the world camera: what the combine is told, through [`FfxUpscale`].
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub(super) struct Upscale {
    pub on: bool,
    pub sharpness: f32,
}

/// The upscale a world of `world` px owes a window of `window` px under `choice`: GSR only when
/// Sharp and the world is narrower than the window (the backdrop keeps the window's aspect, so one
/// axis decides).
fn upscale_for(choice: Upscaler, world: UVec2, window: UVec2) -> Upscale {
    Upscale {
        on: choice == Upscaler::Sharp && world.x < window.x,
        sharpness: EDGE_SHARPNESS,
    }
}

pub(super) fn plugin(app: &mut App) {
    Upscaler::load();
    app.add_systems(Update, drive);
}

/// Keep each world camera's [`Upscale`], and the [`FfxUpscale`] the combine reads, in step with the
/// choice and the world's render size; writes only on a change.
fn drive(
    mut commands: Commands,
    backdrop: Option<Res<WorldBackdrop>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cams: Query<(Entity, Option<&Upscale>), (With<WorldCamera>, With<Camera3d>)>,
) {
    let (Some(backdrop), Ok(window)) = (backdrop, windows.single()) else {
        return;
    };
    let window_px = UVec2::new(window.physical_width(), window.physical_height());
    let want = upscale_for(Upscaler::current(), backdrop.render_size(), window_px);
    for (entity, have) in &cams {
        if have == Some(&want) {
            continue;
        }
        let mut cam = commands.entity(entity);
        cam.insert(want);
        if want.on {
            cam.insert(FfxUpscale {
                sharpness: want.sharpness,
            });
        } else {
            cam.remove::<FfxUpscale>();
        }
        if have.is_none_or(|h| h.on != want.on) {
            info!(
                "upscale: {} ({}x{} world into a {}x{} window)",
                if want.on { "GSR" } else { "bilinear" },
                backdrop.render_size().x,
                backdrop.render_size().y,
                window_px.x,
                window_px.y
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tablet's screen and the presets' worlds at its aspect, `render_target_for`'s rounding.
    const TABLET: UVec2 = UVec2::new(2800, 1752);

    fn world(long_side: f32) -> UVec2 {
        (TABLET.as_vec2() * (long_side / TABLET.x as f32))
            .round()
            .as_uvec2()
    }

    #[test]
    fn sharp_upscales_every_preset_below_the_window() {
        for long in [1920.0, 1440.0, 1080.0] {
            let u = upscale_for(Upscaler::Sharp, world(long), TABLET);
            assert!(u.on, "{long} px world on a 2800 px screen");
            assert_eq!(u.sharpness, 2.0);
        }
    }

    #[test]
    fn plain_never_upscales() {
        for long in [1920.0, 1440.0, 1080.0] {
            assert!(!upscale_for(Upscaler::Plain, world(long), TABLET).on);
        }
    }

    #[test]
    fn a_world_at_or_above_the_window_is_left_alone() {
        assert!(!upscale_for(Upscaler::Sharp, TABLET, TABLET).on, "scale 1");
        // A 1600-wide desktop window under High: the preset's scale clamps at 1.
        let small = UVec2::new(1600, 900);
        assert!(!upscale_for(Upscaler::Sharp, small, small).on);
        assert!(!upscale_for(Upscaler::Sharp, TABLET * 2, TABLET).on, "supersampled");
    }

    #[test]
    fn cycle_order_and_parse() {
        assert_eq!(Upscaler::Sharp.next(), Upscaler::Plain);
        assert_eq!(Upscaler::Plain.next(), Upscaler::Sharp);
        for u in [Upscaler::Sharp, Upscaler::Plain] {
            assert_eq!(Upscaler::parse(u.name()), u);
        }
        assert_eq!(Upscaler::parse(" plain\n"), Upscaler::Plain);
        assert_eq!(Upscaler::parse("garbage"), Upscaler::Sharp, "default Sharp");
    }
}
