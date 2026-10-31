//! Game speed, 1× or 2×, as Vampire Survivors has it: the run plays at double pace while the
//! player knows the early minutes by heart. A "2×" toggle sits right of the clock (gold when on),
//! `X` flips it, and the choice is kept between runs in `benilla-config/speed.txt` (the
//! [`super::framecap`] pattern). `WOW_SURVIVORS_SPEED=2` starts at 2× for the auto-player's
//! batches, without saving.
//!
//! The whole run speeds up through `Time<Virtual>`'s relative speed, set every frame from the
//! phase ([`speed_for`]): the run clock, spawns, cooldowns, movement, scheduled hits and
//! animation all read virtual time. The menu and the results are always 1×. Real time stays
//! real: the camera, the card animations, the frame cap, the frame-time samples and the combat
//! log's fps. The combat log ticks on `Run::elapsed`, so it still writes once a *run* second.
//! Hits land by schedule, not by a projectile's swept path, so 2× cannot tunnel a missile past a
//! small enemy; the crowd's shove is clamped per step (`enemies::move_enemies`).

use bevy::prelude::*;

use super::hud::{HudFont, GOLD_TONE};
use super::Phase;

/// The key that flips the speed; the touch layer taps it for the HUD toggle.
pub(super) const KEY: KeyCode = KeyCode::KeyX;
/// The toggle's box, in UI units: touch-sized (≥ 44).
pub(super) const BUTTON_W: f32 = 52.0;
pub(super) const BUTTON_H: f32 = 44.0;
/// The toggle's pop on a flip, in real seconds.
const POP: f32 = 0.18;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum GameSpeed {
    Normal,
    Double,
}

static SPEED: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

impl GameSpeed {
    pub(super) fn current() -> Self {
        match SPEED.load(std::sync::atomic::Ordering::Relaxed) {
            1 => Self::Double,
            _ => Self::Normal,
        }
    }

    fn store(self) {
        SPEED.store(self as u8, std::sync::atomic::Ordering::Relaxed);
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Normal => "1",
            Self::Double => "2",
        }
    }

    pub(super) fn factor(self) -> f32 {
        match self {
            Self::Normal => 1.0,
            Self::Double => 2.0,
        }
    }

    /// 1× ⇄ 2×.
    pub(super) fn next(self) -> Self {
        match self {
            Self::Normal => Self::Double,
            Self::Double => Self::Normal,
        }
    }

    /// The file's (or the environment's) text: "2", "2x" or "2×" is 2×; anything else 1×.
    pub(super) fn parse(s: &str) -> Self {
        match s.trim().trim_end_matches(['x', 'X', '×']).trim() {
            "2" | "2.0" => Self::Double,
            _ => Self::Normal,
        }
    }

    fn path() -> Option<std::path::PathBuf> {
        crate::local_state::home().map(|h| h.join("speed.txt"))
    }

    /// The saved choice into force at startup; `WOW_SURVIVORS_SPEED` overrides it unsaved.
    fn load() {
        let speed = crate::webenv::var("WOW_SURVIVORS_SPEED")
            .map(|s| Self::parse(&s))
            .or_else(|| {
                Self::path()
                    .and_then(|p| crate::local_state::read_to_string(&p).ok())
                    .map(|s| Self::parse(&s))
            })
            .unwrap_or(Self::Normal);
        speed.store();
    }

    /// The other speed, in force and saved.
    fn cycle() -> Self {
        let next = Self::current().next();
        next.store();
        if let Some(path) = Self::path() {
            let _ = crate::local_state::write_atomic(&path, next.name());
        }
        next
    }
}

/// The virtual clock's relative speed in `phase` with `choice` picked: the choice while a run is
/// on (play, the card pick and the pause menu keep it, the clock being paused under the last
/// two), 1× at the menu and on the results.
pub(super) fn speed_for(phase: Phase, choice: GameSpeed) -> f32 {
    match phase {
        Phase::Playing | Phase::LevelUp | Phase::Paused => choice.factor(),
        Phase::Menu | Phase::GameOver => 1.0,
    }
}

/// The HUD's 2× toggle, right of the clock.
#[derive(Component)]
pub(super) struct SpeedButton;
#[derive(Component)]
struct SpeedLabel;

/// The toggle's look.
const OFF_TEXT: Color = Color::srgba(1.0, 1.0, 1.0, 0.6);
const OFF_EDGE: Color = Color::srgba(0.6, 0.5, 0.3, 0.55);
const OFF_FILL: Color = Color::srgba(0.0, 0.0, 0.0, 0.35);
const ON_FILL: Color = Color::srgba(0.32, 0.2, 0.02, 0.8);
const HOVER_FILL: Color = Color::srgba(0.4, 0.26, 0.06, 0.85);

/// The toggle, spawned into the clock's row by `hud::setup_hud`.
pub(super) fn spawn_button(row: &mut ChildSpawnerCommands, font: &Handle<Font>) {
    row.spawn((
        SpeedButton,
        Button,
        Node {
            width: Val::Px(BUTTON_W),
            height: Val::Px(BUTTON_H),
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(OFF_FILL),
        BorderColor::all(OFF_EDGE),
        UiTransform::IDENTITY,
    ))
    .with_child((
        SpeedLabel,
        super::hud::text_bundle(font, "2×", 22.0, OFF_TEXT),
        Pickable::IGNORE,
    ));
}

/// A placeholder the toggle's width on the clock's other side, so the clock stays centred.
pub(super) fn spawn_balance(row: &mut ChildSpawnerCommands) {
    row.spawn(Node {
        width: Val::Px(BUTTON_W),
        height: Val::Px(BUTTON_H),
        ..default()
    });
}

pub(super) fn plugin(app: &mut App) {
    GameSpeed::load();
    app.add_systems(Update, (flip, apply, style).chain());
}

/// `X`, or a press on the toggle, flips the speed while a run is on.
fn flip(
    keys: Res<ButtonInput<KeyCode>>,
    phase: Res<State<Phase>>,
    buttons: Query<&Interaction, (Changed<Interaction>, With<SpeedButton>)>,
    secrets_page: Res<super::secrets::page::SecretsPage>,
    mut popped: Local<Option<f32>>,
    real: Res<Time<Real>>,
    mut tf: Query<&mut UiTransform, With<SpeedButton>>,
) {
    let in_run = matches!(phase.get(), Phase::Playing | Phase::LevelUp | Phase::Paused);
    let pressed = buttons.iter().any(|i| *i == Interaction::Pressed);
    if in_run && !secrets_page.is_open() && (keys.just_pressed(KEY) || pressed) {
        let now = GameSpeed::cycle();
        info!("survivors: game speed {}×", now.name());
        *popped = Some(0.0);
    }
    // The pop: 1.15 → 1.0, eased out, in real time (the clock may be paused under the cards).
    if let Some(t) = popped.as_mut() {
        *t += real.delta_secs();
        let x = (*t / POP).min(1.0);
        let scale = 1.0 + 0.15 * (1.0 - x) * (1.0 - x);
        for mut tf in &mut tf {
            tf.scale = Vec2::splat(scale);
        }
        if x >= 1.0 {
            *popped = None;
        }
    }
}

/// The virtual clock's speed from the phase and the choice, set only on a change.
fn apply(phase: Res<State<Phase>>, mut time: ResMut<Time<Virtual>>) {
    let want = speed_for(*phase.get(), GameSpeed::current());
    if time.relative_speed() != want {
        time.set_relative_speed(want);
    }
}

/// Gold when 2× is on; a dim outline when off; warmer under the cursor.
#[allow(clippy::type_complexity)]
fn style(
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        With<SpeedButton>,
    >,
    mut labels: Query<&mut TextColor, With<SpeedLabel>>,
    font: Option<Res<HudFont>>,
) {
    if font.is_none() {
        return;
    }
    let on = GameSpeed::current() == GameSpeed::Double;
    for (interaction, mut bg, mut border) in &mut buttons {
        let fill = match (interaction, on) {
            (Interaction::Hovered | Interaction::Pressed, _) => HOVER_FILL,
            (Interaction::None, true) => ON_FILL,
            (Interaction::None, false) => OFF_FILL,
        };
        if bg.0 != fill {
            bg.0 = fill;
        }
        let edge = if on { GOLD_TONE } else { OFF_EDGE };
        if border.top != edge {
            *border = BorderColor::all(edge);
        }
    }
    let text = if on { GOLD_TONE } else { OFF_TEXT };
    for mut c in &mut labels {
        if c.0 != text {
            c.0 = text;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_for_keeps_the_choice_through_the_run_and_one_x_outside_it() {
        for p in [Phase::Playing, Phase::LevelUp, Phase::Paused] {
            assert_eq!(speed_for(p, GameSpeed::Double), 2.0);
            assert_eq!(speed_for(p, GameSpeed::Normal), 1.0);
        }
        for p in [Phase::Menu, Phase::GameOver] {
            assert_eq!(speed_for(p, GameSpeed::Double), 1.0);
            assert_eq!(speed_for(p, GameSpeed::Normal), 1.0);
        }
    }

    #[test]
    fn toggle_cycles_and_parses_its_own_names() {
        assert_eq!(GameSpeed::Normal.next(), GameSpeed::Double);
        assert_eq!(GameSpeed::Double.next(), GameSpeed::Normal);
        for s in [GameSpeed::Normal, GameSpeed::Double] {
            assert_eq!(GameSpeed::parse(s.name()), s);
        }
        for s in ["2", " 2\n", "2x", "2X", "2×", "2.0"] {
            assert_eq!(GameSpeed::parse(s), GameSpeed::Double, "{s:?}");
        }
        for s in ["", "1", "garbage", "3"] {
            assert_eq!(GameSpeed::parse(s), GameSpeed::Normal, "{s:?}");
        }
    }
}
