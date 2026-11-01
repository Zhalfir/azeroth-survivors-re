//! The cooldown states every action button shares (the mobility, the potion, later the
//! ultimate), on the HUD and on the touch cluster alike:
//!
//! - **Charging:** the icon greyed, a dark sweep emptying clockwise as WoW's cooldown clock, and
//!   the seconds left in the centre (tenths under 3 s). The number changes only when the shown
//!   value does, the sweep at 0.1 s steps so it turns smoothly.
//! - **Ready:** full colour, one gold ring flash (1.0→1.4×, fading over 0.35 s) with a soft click,
//!   then a slow gold edge glow (1 Hz) until it is used.
//! - **Pressed:** a 0.9× squash for 0.08 s.
//! - **Pressed while charging:** a short red edge flash, and nothing happens.

use bevy::prelude::*;
use bevy::ui::UiTransform;

use super::hud::{IconDone, SweepUiMaterial};
use super::{Phase, Run};

/// What a button shows for `cd` seconds left of a `max`-second cooldown.
#[derive(Clone, PartialEq, Debug)]
pub(super) enum ActionState {
    /// `frac` of the cooldown left (1 just used), and the number over the icon.
    Charging(f32, String),
    Ready,
}

/// The tenths shown from here down.
const TENTHS_BELOW: f32 = 3.0;

pub(super) fn action_state(cd: f32, max: f32) -> ActionState {
    if cd <= 0.0 {
        return ActionState::Ready;
    }
    let frac = (cd / max.max(0.01)).clamp(0.0, 1.0);
    let label = if cd < TENTHS_BELOW {
        // Up to the next tenth (a hair of float slack, so 2.4 reads "2.4"), and "0.1" until the
        // very end, never "0.0".
        format!("{:.1}", ((cd * 10.0) - 1e-3).ceil().max(1.0) / 10.0)
    } else {
        format!("{}", cd.ceil() as u32)
    };
    ActionState::Charging(frac, label)
}

/// The sweep moves in tenth-of-a-second steps: `cd` rounded up to its tenth, and the share of
/// `max` that step leaves.
pub(super) fn sweep_step(cd: f32, max: f32) -> (i32, f32) {
    if cd <= 0.0 {
        return (0, 0.0);
    }
    let step = ((cd * 10.0) - 1e-3).ceil().max(1.0) as i32;
    (step, (step as f32 / 10.0 / max.max(0.01)).clamp(0.0, 1.0))
}

/// Which run cooldown a button shows, and the key it presses.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ActionSource {
    Mobility,
    Potion,
}

impl ActionSource {
    fn key(self) -> KeyCode {
        match self {
            Self::Mobility => super::mobility::KEY,
            Self::Potion => super::potion::POTION_KEY,
        }
    }
}

/// A button's cooldown, fed from the run each frame it changes.
#[derive(Component, Clone, Copy, PartialEq, Default, Debug)]
pub(super) struct ActionButton {
    pub cd: f32,
    pub max: f32,
    /// Ready but pointless now (the potion at full health): dimmed, no glow, still pressable.
    pub dim: bool,
}

/// What the button last drew, and its running flashes.
#[derive(Component, Default)]
pub(super) struct ActionView {
    /// The label last drawn; `None` before the first frame.
    shown: Option<String>,
    /// The sweep's step last drawn, in tenths of a second left ([`sweep_step`]).
    swept: Option<i32>,
    ready: Option<bool>,
    flash_t: f32,
    press_t: f32,
    deny_t: f32,
}

/// The icon of the button `0`, greyed while it charges.
#[derive(Component)]
pub(super) struct ActionIcon(pub Entity);
#[derive(Component)]
pub(super) struct ActionSweep {
    owner: Entity,
    handle: Handle<SweepUiMaterial>,
}
#[derive(Component)]
pub(super) struct ActionLabel(Entity);

#[derive(Clone, Copy, PartialEq, Eq)]
enum EdgeRole {
    /// The ready flash's expanding ring.
    Ring,
    /// The steady glow while ready.
    Glow,
    /// The red "not ready" flash.
    Deny,
}

#[derive(Component)]
pub(super) struct ActionEdge {
    owner: Entity,
    role: EdgeRole,
}

const READY_FLASH: f32 = 0.35;
const PRESS_SQUASH: f32 = 0.08;
const DENY_FLASH: f32 = 0.25;
const GOLD: Color = Color::srgb(1.0, 0.82, 0.0);
const RED: Color = Color::srgb(1.0, 0.15, 0.1);
/// The charging icon's tint: about 35 % luminance.
const GREY: Color = Color::srgb(0.35, 0.35, 0.35);
/// A ready but pointless button's tint.
const DIMMED: Color = Color::srgb(0.6, 0.6, 0.6);
/// The click when a button comes ready.
const READY_SOUND: &str = "igMainMenuOptionCheckBoxOn";

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (feed_buttons, draw_buttons, tint_icons)
            .chain()
            .after(super::hud::fill_icons),
    );
}

/// The sweep, the number, the ready ring, the glow and the red flash over a button's icon:
/// spawned inside the button (`slot`), which also gets [`ActionButton`] and `source`. The caller
/// marks the icon with [`ActionIcon`]. `size` is the number's font size (at least 18).
pub(super) fn spawn_overlay(
    slot: &mut ChildSpawnerCommands,
    mats: &mut Assets<SweepUiMaterial>,
    font: &Handle<Font>,
    size: f32,
    source: ActionSource,
) {
    let owner = slot.target_entity();
    slot.commands().entity(owner).insert((
        source,
        ActionButton::default(),
        ActionView::default(),
        UiTransform::IDENTITY,
    ));
    let handle = mats.add(SweepUiMaterial { sweep: 0.0 });
    let fill = Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        ..default()
    };
    slot.spawn((
        fill.clone(),
        MaterialNode(handle.clone()),
        ActionSweep { owner, handle },
        Pickable::IGNORE,
    ));
    slot.spawn((
        Node {
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..fill.clone()
        },
        Pickable::IGNORE,
    ))
    .with_children(|n| {
        n.spawn((
            ActionLabel(owner),
            Text::new(""),
            TextFont {
                font: font.clone(),
                font_size: size.max(18.0),
                ..default()
            },
            TextColor(Color::WHITE),
            // An outline of sorts: the shadow on the dark sweep.
            TextShadow {
                offset: Vec2::splat(1.5),
                color: Color::BLACK,
            },
            TextLayout::new(Justify::Center, LineBreak::NoWrap),
            Pickable::IGNORE,
        ));
    });
    for role in [EdgeRole::Glow, EdgeRole::Deny, EdgeRole::Ring] {
        slot.spawn((
            Node {
                border: UiRect::all(Val::Px(3.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..fill.clone()
            },
            BorderColor::all(Color::NONE),
            UiTransform::IDENTITY,
            ActionEdge { owner, role },
            Pickable::IGNORE,
        ));
    }
}

/// Each button's cooldown from the run, written only when it changed.
fn feed_buttons(run: Res<Run>, mut buttons: Query<(&ActionSource, &mut ActionButton)>) {
    for (source, mut b) in &mut buttons {
        let want = match source {
            ActionSource::Mobility => match run.mobility() {
                Some(def) => ActionButton {
                    cd: run.mobile.cd,
                    max: def.cooldown,
                    dim: false,
                },
                None => ActionButton::default(),
            },
            ActionSource::Potion => ActionButton {
                cd: run.potion_cd,
                max: super::potion::POTION_COOLDOWN,
                dim: run.hp >= run.stats().max_hp,
            },
        };
        if *b != want {
            *b = want;
        }
    }
}

/// What one button's children should draw this frame.
struct Look {
    owner: Entity,
    /// A new label, when the shown value changed.
    label: Option<String>,
    /// A new sweep share, when its tenth-of-a-second step changed.
    sweep: Option<f32>,
    ring: f32,
    glow: f32,
    deny: f32,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw_buttons(
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    phase: Res<State<Phase>>,
    run: Res<Run>,
    mut buttons: Query<
        (
            Entity,
            &ActionSource,
            &ActionButton,
            &mut ActionView,
            &mut UiTransform,
        ),
        Without<ActionEdge>,
    >,
    mut edges: Query<(&ActionEdge, &mut BorderColor, &mut UiTransform), Without<ActionButton>>,
    mut labels: Query<(&ActionLabel, &mut Text)>,
    sweeps: Query<&ActionSweep>,
    mut mats: ResMut<Assets<SweepUiMaterial>>,
    mut sounds: MessageWriter<crate::sound::GlueSound>,
) {
    let dt = time.delta_secs();
    let live = *phase.get() == Phase::Playing && run.dead_t.is_none();
    let mut looks: Vec<Look> = Vec::new();
    for (owner, source, b, mut view, mut tf) in &mut buttons {
        let state = action_state(b.cd, b.max);
        let ready = state == ActionState::Ready;
        let view = &mut *view;
        // Ready from charging: the flash and the click. Charging from ready: it was used.
        match (view.ready, ready) {
            (Some(false), true) => {
                view.flash_t = READY_FLASH;
                if live {
                    sounds.write(crate::sound::GlueSound(READY_SOUND));
                }
            }
            (Some(true), false) => view.press_t = PRESS_SQUASH,
            _ => {}
        }
        view.ready = Some(ready);
        if live && !ready && keys.just_pressed(source.key()) {
            view.deny_t = DENY_FLASH;
        }
        let label = match state {
            ActionState::Charging(_, label) => label,
            ActionState::Ready => String::new(),
        };
        let changed = view.shown.as_ref() != Some(&label);
        if changed {
            view.shown = Some(label.clone());
        }
        let (step, frac) = sweep_step(b.cd, b.max);
        let swept = view.swept != Some(step);
        if swept {
            view.swept = Some(step);
        }
        let squash = if view.press_t > 0.0 { 0.9 } else { 1.0 };
        if tf.scale.x != squash {
            tf.scale = Vec2::splat(squash);
        }
        let ring = view.flash_t / READY_FLASH;
        let glow = if ready && !b.dim && view.flash_t <= 0.0 {
            // 1 Hz between 0.5 and 1.0.
            0.75 + 0.25 * (std::f32::consts::TAU * time.elapsed_secs()).sin()
        } else {
            0.0
        };
        looks.push(Look {
            owner,
            label: changed.then_some(label),
            sweep: swept.then_some(frac),
            ring,
            glow,
            deny: view.deny_t / DENY_FLASH,
        });
        view.flash_t = (view.flash_t - dt).max(0.0);
        view.press_t = (view.press_t - dt).max(0.0);
        view.deny_t = (view.deny_t - dt).max(0.0);
    }
    let look = |e: Entity| looks.iter().find(|l| l.owner == e);
    for (label, mut text) in &mut labels {
        if let Some(s) = look(label.0).and_then(|l| l.label.as_ref()) {
            if text.0 != *s {
                text.0 = s.clone();
            }
        }
    }
    for sweep in &sweeps {
        if let Some(f) = look(sweep.owner).and_then(|l| l.sweep) {
            if let Some(m) = mats.get_mut(&sweep.handle) {
                m.sweep = f;
            }
        }
    }
    for (edge, mut color, mut tf) in &mut edges {
        let Some(l) = look(edge.owner) else { continue };
        let (want, scale) = match edge.role {
            // Grows 1.0→1.4 as it fades.
            EdgeRole::Ring => (GOLD.with_alpha(l.ring), 1.0 + 0.4 * (1.0 - l.ring)),
            EdgeRole::Glow => (GOLD.with_alpha(l.glow), 1.0),
            EdgeRole::Deny => (RED.with_alpha(l.deny), 1.0),
        };
        let want = if want.alpha() <= 0.0 {
            Color::NONE
        } else {
            want
        };
        if color.top != want {
            *color = BorderColor::all(want);
        }
        let scale = if edge.role == EdgeRole::Ring && l.ring <= 0.0 {
            1.0
        } else {
            scale
        };
        if tf.scale.x != scale {
            tf.scale = Vec2::splat(scale);
        }
    }
}

/// The icon greyed while its button charges, dimmed when pointless, full colour when ready. Only
/// once the icon is drawn ([`IconDone`]), whose fill sets it white.
fn tint_icons(
    buttons: Query<&ActionButton>,
    mut icons: Query<(&ActionIcon, &mut ImageNode), With<IconDone>>,
) {
    for (icon, mut img) in &mut icons {
        let Ok(b) = buttons.get(icon.0) else { continue };
        let want = if b.cd > 0.0 {
            GREY
        } else if b.dim {
            DIMMED
        } else {
            Color::WHITE
        };
        if img.color != want {
            img.color = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_state_labels_and_ready() {
        assert_eq!(action_state(0.0, 60.0), ActionState::Ready);
        assert_eq!(action_state(-0.2, 60.0), ActionState::Ready);
        match action_state(12.0, 60.0) {
            ActionState::Charging(f, label) => {
                assert_eq!(label, "12");
                assert!((f - 0.2).abs() < 1e-6);
            }
            s => panic!("{s:?}"),
        }
        // Whole seconds round up, as WoW's clock: 11.3 left reads 12.
        assert_eq!(
            action_state(11.3, 60.0),
            ActionState::Charging(11.3 / 60.0, "12".into())
        );
        assert_eq!(
            action_state(2.4, 60.0),
            ActionState::Charging(2.4 / 60.0, "2.4".into())
        );
        assert_eq!(
            action_state(0.01, 8.0),
            ActionState::Charging(0.01 / 8.0, "0.1".into())
        );
        // Just used: the whole sweep.
        assert_eq!(
            action_state(8.0, 8.0),
            ActionState::Charging(1.0, "8".into())
        );
    }

    #[test]
    fn sweep_moves_in_tenths() {
        assert_eq!(sweep_step(0.0, 8.0), (0, 0.0));
        assert_eq!(sweep_step(8.0, 8.0), (80, 1.0));
        // 7.93 and 7.91 share a step; 7.89 is the next.
        assert_eq!(sweep_step(7.93, 8.0).0, 80);
        assert_eq!(sweep_step(7.93, 8.0), sweep_step(7.91, 8.0));
        assert_ne!(sweep_step(7.91, 8.0), sweep_step(7.89, 8.0));
        let (s, f) = sweep_step(4.0, 8.0);
        assert_eq!(s, 40);
        assert!((f - 0.5).abs() < 1e-6);
        // A hair left still shows one step.
        assert_eq!(sweep_step(0.01, 8.0).0, 1);
    }
}
