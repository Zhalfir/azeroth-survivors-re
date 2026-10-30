//! Graphics presets for the Android build: one pause-menu button cycles High → Medium → Low, each
//! a set of the engine's own CVars, applied live and kept in `benilla-config/graphics.txt`. The
//! CVars persist themselves (`config.toml`); the preset is re-applied at every start anyway,
//! because its resolution is a long side in pixels and the render scale a ratio of the window.
//!
//! `$WOW_RENDER_SCALE` (`android-env.txt`) still wins over the preset's resolution.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::hud::UiAction;
use crate::cvars::Cvars;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum Preset {
    #[default]
    High,
    Medium,
    Low,
}

impl Preset {
    fn name(self) -> &'static str {
        match self {
            Preset::High => "High",
            Preset::Medium => "Medium",
            Preset::Low => "Low",
        }
    }

    fn next(self) -> Self {
        match self {
            Preset::High => Preset::Medium,
            Preset::Medium => Preset::Low,
            Preset::Low => Preset::High,
        }
    }

    fn parse(s: &str) -> Option<Self> {
        [Preset::High, Preset::Medium, Preset::Low]
            .into_iter()
            .find(|p| p.name().eq_ignore_ascii_case(s.trim()))
    }

    /// The long side, in physical pixels, the 3D world renders at; the UI stays native.
    fn world_long_side(self) -> f32 {
        match self {
            Preset::High => 1920.0,
            Preset::Medium => 1440.0,
            Preset::Low => 1080.0,
        }
    }

    /// The CVars besides the resolution: view distance, shadows, particles, glow, weather.
    fn cvars(self) -> [(&'static str, &'static str); 5] {
        match self {
            Preset::High => [
                ("farclip", "350"),
                ("shadowLevel", "1"),
                ("spellEffectLevel", "2"),
                ("ffxGlow", "1"),
                ("useWeatherShaders", "1"),
            ],
            Preset::Medium => [
                ("farclip", "250"),
                ("shadowLevel", "0"),
                ("spellEffectLevel", "1"),
                ("ffxGlow", "1"),
                ("useWeatherShaders", "0"),
            ],
            Preset::Low => [
                ("farclip", "177"),
                ("shadowLevel", "0"),
                ("spellEffectLevel", "0"),
                ("ffxGlow", "0"),
                ("useWeatherShaders", "0"),
            ],
        }
    }
}

/// The preset in force, and the window's long side it was last applied for: the preset is
/// applied again whenever that changes. On Android the window starts at a 1600×900 placeholder
/// before the screen's real size arrives, and a scale worked out then would stay at 1.
#[derive(Resource)]
pub(super) struct Graphics {
    pub(super) preset: Preset,
    applied_for: Option<u32>,
}

/// The pause-menu button's label.
#[derive(Component)]
pub(super) struct GraphicsLabel;

pub(super) fn label(preset: Preset) -> String {
    format!("Graphics: {}", preset.name())
}

fn preset_path() -> Option<std::path::PathBuf> {
    crate::local_state::home().map(|h| h.join("graphics.txt"))
}

pub(super) fn plugin(app: &mut App) {
    let preset = preset_path()
        .and_then(|p| crate::local_state::read_to_string(&p).ok())
        .and_then(|s| Preset::parse(&s))
        .unwrap_or_default();
    app.insert_resource(Graphics {
        preset,
        applied_for: None,
    })
    .add_systems(Update, (cycle_preset, apply_preset).chain());
}

fn cycle_preset(
    presses: Query<(&Interaction, &UiAction), Changed<Interaction>>,
    mut graphics: ResMut<Graphics>,
) {
    for (interaction, action) in &presses {
        if *interaction == Interaction::Pressed && *action == UiAction::Graphics {
            graphics.preset = graphics.preset.next();
            graphics.applied_for = None;
            if let Some(path) = preset_path() {
                let _ = crate::local_state::write_atomic(&path, graphics.preset.name());
            }
        }
    }
}

fn apply_preset(
    mut graphics: ResMut<Graphics>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cvars: Option<ResMut<Cvars>>,
    mut labels: Query<&mut Text, With<GraphicsLabel>>,
) {
    let (Some(mut cvars), Ok(window)) = (cvars, windows.single()) else {
        return;
    };
    let long_px = window.physical_width().max(window.physical_height());
    if long_px == 0 || graphics.applied_for == Some(long_px) {
        return;
    }
    let long = long_px as f32;
    let preset = graphics.preset;
    if std::env::var_os("WOW_RENDER_SCALE").is_none() {
        let scale = (preset.world_long_side() / long).min(1.0);
        cvars.set("renderScale", &format!("{scale:.3}"));
    }
    for (name, value) in preset.cvars() {
        cvars.set(name, value);
    }
    for mut text in &mut labels {
        text.0 = label(preset);
    }
    info!(
        "graphics: preset {} ({}px world on a {long_px}px screen)",
        preset.name(),
        preset.world_long_side()
    );
    graphics.applied_for = Some(long_px);
}
