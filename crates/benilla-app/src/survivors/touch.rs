//! Touch controls for the Android build. Nothing downstream learns about touch: fingers are
//! translated, in `PreUpdate` ahead of [`InputSystems`], into the mouse, keyboard and wheel input
//! the desktop game already reads.
//!
//! - Off the battleground (menu, level-up cards, pause, results) the first finger is the mouse:
//!   it moves the cursor and holds the left button, so glue clicks, bevy_ui buttons and the menu
//!   hero's drag-to-turn work unchanged.
//! - In play, a finger that lands on the left half is a floating joystick that holds W/A/S/D and
//!   points the cursor (the hero faces it) the way it runs; a finger on the right half aims the
//!   cursor; the round buttons tap F (the class movement spell), C (lock / free the camera) and
//!   Escape (pause); two fingers pinch the camera in and out through the wheel. With the camera
//!   free, a finger on the right half drags it around the hero instead, as a held right button.
//!
//! Each synthesized input is written both as its own message and as a [`WindowEvent`], as
//! bevy_winit does: picking reads the latter.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::input::{ButtonState, InputSystems};
use bevy::picking::input::PointerInputSettings;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::ui::{ComputedNode, UiGlobalTransform, UiScale};
use bevy::window::{CursorMoved, PrimaryWindow, WindowEvent};

use super::hero::{CamMode, CamRig};
use super::hud::SAFE_BOTTOM;
use super::Phase;

/// How far, in logical pixels, the joystick's knob travels from where the thumb landed.
const STICK_RADIUS: f32 = 70.0;
/// The share of [`STICK_RADIUS`] an axis must pass before its key is held.
const STICK_DEAD: f32 = 0.35;
/// How far ahead of the hero the joystick points the cursor.
const STICK_AIM: f32 = 160.0;
/// A play button's side, in UI units (the HUD's, 768 to the screen's height): an action slot
/// half again the HUD's own 40, for a thumb.
const BUTTON_SIZE: f32 = 60.0;
/// The play buttons: the key each taps, its icon (`None`: the class's movement spell, filled
/// once learned), and its centre's inset in UI units from the right edge and from the bottom
/// (`true`) or top edge. Pause sits under the minimap, the rest by the right thumb, their feet
/// over the gesture-bar band ([`SAFE_BOTTOM`]).
const BUTTONS: [(KeyCode, Option<&str>, Vec2, bool); 3] = [
    (
        super::mobility::KEY,
        None,
        Vec2::new(70.0, SAFE_BOTTOM + 108.0),
        true,
    ),
    (
        KeyCode::KeyC,
        Some("Interface\\Icons\\Ability_Hunter_EagleEye"),
        Vec2::new(165.0, SAFE_BOTTOM + 38.0),
        true,
    ),
    (
        KeyCode::Escape,
        Some("Interface\\Icons\\Spell_Nature_Sleep"),
        Vec2::new(45.0, 215.0),
        false,
    ),
];
/// An icon's inner 86%: the outer rows are the painted bevel the slot frame replaces.
const ICON_CROP: [f32; 4] = [0.07, 0.93, 0.07, 0.93];
/// Swing per logical pixel of drag, in the mouse counts the orbit rate is tuned for.
const ORBIT_PER_PX: f32 = 1.6;
const GOLD: Color = Color::srgba(1.0, 0.85, 0.5, 0.6);
/// Pinch distance, in logical pixels, per wheel line.
const PINCH_PER_LINE: f32 = 40.0;

pub(super) fn plugin(app: &mut App) {
    // The mouse stands in for every finger; bevy's own touch pointer would press twice.
    app.insert_resource(PointerInputSettings {
        is_touch_enabled: false,
        is_mouse_enabled: true,
    })
    .init_resource::<TouchState>()
    .add_systems(Startup, spawn_overlay.after(super::hud::setup_hud))
    .add_systems(PreUpdate, translate_touches.before(InputSystems))
    .add_systems(Update, show_overlay);
}

/// What a finger is doing, decided where it landed.
#[derive(Clone, Copy, PartialEq)]
enum Role {
    Mouse,
    Stick {
        origin: Vec2,
    },
    Aim,
    /// A right-half finger with the camera free: holds the right button and drags the view.
    Orbit,
    Button(KeyCode),
    /// A finger that landed while another phase owned the screen, or a second mouse finger.
    Ignored,
}

#[derive(Resource, Default)]
struct TouchState {
    fingers: HashMap<u64, (Role, Vec2)>,
    /// Keys this module is holding down, released when nothing holds them any more.
    held: Vec<KeyCode>,
    mouse_down: bool,
    right_down: bool,
    /// Where the cursor was last put, so the hero keeps facing it once the thumb lifts.
    cursor: Option<Vec2>,
    /// The two-finger spread last frame, for the pinch.
    pinch: Option<f32>,
    /// Keys tapped this frame, released next frame.
    tapped: Vec<KeyCode>,
}

#[derive(Component)]
struct StickRing;
#[derive(Component)]
struct StickKnob;
#[derive(Component)]
/// A play button and the key it taps.
struct PlayButton(KeyCode);
/// The camera button's checked glow, shown while the camera is free.
#[derive(Component)]
struct CamGlow;
/// The movement-spell button's icon, re-pointed at the class's spell once it is learned.
#[derive(Component)]
struct MobilityButtonIcon;

type ButtonRects<'w, 's> = Query<
    'w,
    's,
    (
        &'static PlayButton,
        &'static ComputedNode,
        &'static UiGlobalTransform,
        &'static InheritedVisibility,
    ),
>;

#[allow(clippy::too_many_arguments)]
fn translate_touches(
    mut touches: MessageReader<TouchInput>,
    phase: Res<State<Phase>>,
    rig: Res<CamRig>,
    mut state: ResMut<TouchState>,
    play_buttons: ButtonRects,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut keys: MessageWriter<KeyboardInput>,
    mut buttons: MessageWriter<MouseButtonInput>,
    mut moves: MessageWriter<CursorMoved>,
    mut wheel: MessageWriter<MouseWheel>,
    mut motion: MessageWriter<MouseMotion>,
    mut events: MessageWriter<WindowEvent>,
) {
    let Ok((window_id, mut window)) = windows.single_mut() else {
        return;
    };
    let size = window.size();
    let playing = *phase.get() == Phase::Playing;
    let state = &mut *state;

    let key = |code: KeyCode, down: bool| KeyboardInput {
        key_code: code,
        logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
        state: if down {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
        text: None,
        repeat: false,
        window: window_id,
    };

    // Last frame's taps go back up.
    for code in std::mem::take(&mut state.tapped) {
        let ev = key(code, false);
        events.write(WindowEvent::KeyboardInput(ev.clone()));
        keys.write(ev);
    }

    let mut cursor_to = None;
    let mut mouse_to = None;
    for touch in touches.read() {
        let pos = touch.position;
        match touch.phase {
            TouchPhase::Started => {
                let role = if !playing {
                    if state.fingers.values().any(|(r, _)| *r == Role::Mouse) {
                        Role::Ignored
                    } else {
                        mouse_to = Some(true);
                        cursor_to = Some(pos);
                        Role::Mouse
                    }
                } else if let Some(code) = button_at(pos, window.scale_factor(), &play_buttons) {
                    state.tapped.push(code);
                    let ev = key(code, true);
                    events.write(WindowEvent::KeyboardInput(ev.clone()));
                    keys.write(ev);
                    Role::Button(code)
                } else if pos.x < size.x * 0.5
                    && !state
                        .fingers
                        .values()
                        .any(|(r, _)| matches!(r, Role::Stick { .. }))
                {
                    Role::Stick { origin: pos }
                } else if rig.mode == CamMode::Orbit {
                    Role::Orbit
                } else {
                    cursor_to = Some(pos);
                    Role::Aim
                };
                state.fingers.insert(touch.id, (role, pos));
            }
            TouchPhase::Moved => {
                if let Some((role, at)) = state.fingers.get_mut(&touch.id) {
                    if *role == Role::Orbit {
                        let delta = (pos - *at) * ORBIT_PER_PX;
                        if delta != Vec2::ZERO {
                            motion.write(MouseMotion { delta });
                        }
                    }
                    *at = pos;
                    if matches!(role, Role::Mouse | Role::Aim) {
                        cursor_to = Some(pos);
                    }
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                if let Some((Role::Mouse, _)) = state.fingers.remove(&touch.id) {
                    mouse_to = Some(false);
                }
            }
        }
    }

    // A phase change mid-touch (a level-up while the thumb is on the stick) hands every finger
    // back: each must lift before it means anything in the new phase.
    if !playing {
        for (role, _) in state.fingers.values_mut() {
            if matches!(
                role,
                Role::Stick { .. } | Role::Aim | Role::Orbit | Role::Button(_)
            ) {
                *role = Role::Ignored;
            }
        }
    }

    // The joystick: the keys it holds, and the cursor ahead of the hero the way it points.
    let stick = state.fingers.values().find_map(|(r, at)| match r {
        Role::Stick { origin } => Some((*at - *origin).clamp_length_max(STICK_RADIUS)),
        _ => None,
    });
    let mut want: Vec<KeyCode> = Vec::new();
    if let Some(d) = stick {
        let dead = STICK_RADIUS * STICK_DEAD;
        if d.y < -dead {
            want.push(KeyCode::KeyW);
        }
        if d.y > dead {
            want.push(KeyCode::KeyS);
        }
        if d.x < -dead {
            want.push(KeyCode::KeyA);
        }
        if d.x > dead {
            want.push(KeyCode::KeyD);
        }
        let aiming = state.fingers.values().any(|(r, _)| *r == Role::Aim);
        if !aiming && d.length() > dead {
            cursor_to = Some(size * 0.5 + d.normalize() * STICK_AIM);
        }
    }
    for code in state.held.clone() {
        if !want.contains(&code) {
            let ev = key(code, false);
            events.write(WindowEvent::KeyboardInput(ev.clone()));
            keys.write(ev);
        }
    }
    for &code in &want {
        if !state.held.contains(&code) {
            let ev = key(code, true);
            events.write(WindowEvent::KeyboardInput(ev.clone()));
            keys.write(ev);
        }
    }
    state.held = want;

    // Two fingers off the stick pinch the camera; spreading them zooms in, as the wheel's up.
    let spread = {
        let mut free = state
            .fingers
            .values()
            .filter(|(r, _)| matches!(r, Role::Aim | Role::Mouse | Role::Ignored))
            .map(|(_, at)| *at);
        match (free.next(), free.next()) {
            (Some(a), Some(b)) => Some(a.distance(b)),
            _ => None,
        }
    };
    if let (Some(now), Some(before)) = (spread, state.pinch) {
        let lines = (now - before) / PINCH_PER_LINE;
        if lines.abs() > 0.01 {
            let ev = MouseWheel {
                unit: MouseScrollUnit::Line,
                x: 0.0,
                y: lines,
                window: window_id,
            };
            events.write(WindowEvent::MouseWheel(ev));
            wheel.write(ev);
        }
    }
    state.pinch = spread;

    if let Some(pos) = cursor_to {
        let delta = state.cursor.map(|c| pos - c);
        state.cursor = Some(pos);
        window.set_cursor_position(Some(pos));
        let ev = CursorMoved {
            window: window_id,
            position: pos,
            delta,
        };
        events.write(WindowEvent::CursorMoved(ev.clone()));
        moves.write(ev);
    } else if window.cursor_position().is_none() {
        // winit never reports a cursor on Android; keep the last one put, so the hero's facing
        // and the menu's hover survive the finger lifting.
        if let Some(pos) = state.cursor {
            window.set_cursor_position(Some(pos));
        }
    }

    let mut button = |button: MouseButton, down: bool| {
        let ev = MouseButtonInput {
            button,
            state: if down {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            window: window_id,
        };
        events.write(WindowEvent::MouseButtonInput(ev));
        buttons.write(ev);
    };
    if let Some(down) = mouse_to.filter(|d| *d != state.mouse_down) {
        state.mouse_down = down;
        button(MouseButton::Left, down);
    }
    // The right button is held while any finger drags the free camera.
    let orbiting = state.fingers.values().any(|(r, _)| *r == Role::Orbit);
    if orbiting != state.right_down {
        state.right_down = orbiting;
        button(MouseButton::Right, orbiting);
    }
}

/// The play button under `pos` (logical pixels), if any. Hit-tested against where the layout put
/// each button, not the insets: the survivors HUD scales the whole UI ([`UiScale`]).
fn button_at(pos: Vec2, scale: f32, buttons: &ButtonRects) -> Option<KeyCode> {
    let at = pos * scale;
    buttons
        .iter()
        .find(|(_, node, tf, vis)| vis.get() && at.distance(tf.translation) < node.size().x * 0.75)
        .map(|(b, ..)| b.0)
}

fn circle(radius: f32, color: Color, place: impl FnOnce(&mut Node)) -> impl Bundle {
    let mut node = Node {
        position_type: PositionType::Absolute,
        width: Val::Px(radius * 2.0),
        height: Val::Px(radius * 2.0),
        border: UiRect::all(Val::Px(2.0)),
        border_radius: BorderRadius::MAX,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    };
    place(&mut node);
    (
        node,
        BackgroundColor(color),
        BorderColor::all(GOLD),
        GlobalZIndex(1000),
        Pickable::IGNORE,
        Visibility::Hidden,
    )
}

/// A play button's key hint, as an action button shows its binding: over a bottom button, under
/// the top one (Pause, under the minimap).
fn hint(key: KeyCode) -> &'static str {
    match key {
        KeyCode::KeyF => "F",
        KeyCode::KeyC => "C",
        KeyCode::Escape => "Esc",
        _ => "",
    }
}

fn spawn_overlay(
    mut commands: Commands,
    font: Option<Res<super::hud::HudFont>>,
    mut sweeps: ResMut<Assets<super::hud::SweepUiMaterial>>,
) {
    let Some(font) = font else { return };
    let f = &font.0;
    let fill = Color::srgba(0.05, 0.04, 0.03, 0.35);
    commands.spawn((StickRing, circle(STICK_RADIUS, fill, |_| {})));
    commands.spawn((
        StickKnob,
        circle(
            STICK_RADIUS * 0.4,
            Color::srgba(1.0, 0.85, 0.5, 0.35),
            |_| {},
        ),
    ));
    for (key, icon, inset, from_bottom) in BUTTONS {
        let mut node = Node {
            position_type: PositionType::Absolute,
            width: Val::Px(BUTTON_SIZE),
            height: Val::Px(BUTTON_SIZE),
            right: Val::Px(inset.x - BUTTON_SIZE / 2.0),
            ..default()
        };
        if from_bottom {
            node.bottom = Val::Px(inset.y - BUTTON_SIZE / 2.0);
        } else {
            node.top = Val::Px(inset.y - BUTTON_SIZE / 2.0);
        }
        let hidden = ImageNode {
            color: Color::NONE,
            ..default()
        };
        commands
            .spawn((
                PlayButton(key),
                node,
                super::detail::TextKeepOut,
                GlobalZIndex(1000),
                Pickable::IGNORE,
                Visibility::Hidden,
            ))
            .with_children(|slot| {
                // The icon under the frame, as the HUD's mobility slot lays them.
                let mut art = slot.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    hidden.clone(),
                    Pickable::IGNORE,
                ));
                match icon {
                    Some(path) => art.insert(super::hud::UiArt::crop(path, ICON_CROP)),
                    None => art.insert(MobilityButtonIcon),
                };
                // The movement spell's cooldown, as on the HUD's F slot.
                if key == super::mobility::KEY {
                    super::hud::mobility_cooldown(slot, &mut sweeps, f, 24.0);
                }
                // The key hint, clear of the slot frame's ring.
                let mut label = Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                };
                if from_bottom {
                    label.bottom = Val::Px(BUTTON_SIZE + 6.0);
                } else {
                    label.top = Val::Px(BUTTON_SIZE + 6.0);
                }
                slot.spawn((label, Pickable::IGNORE)).with_children(|l| {
                    l.spawn((
                        super::hud::centered_bundle(f, hint(key), 16.0, Color::WHITE),
                        Pickable::IGNORE,
                    ));
                });
                // `UI-Quickslot2`: the 64×64 slot frame, its button 36 inside.
                slot.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(-BUTTON_SIZE * 0.42),
                        top: Val::Px(-BUTTON_SIZE * 0.42),
                        width: Val::Px(BUTTON_SIZE * 1.84),
                        height: Val::Px(BUTTON_SIZE * 1.84),
                        ..default()
                    },
                    hidden.clone(),
                    super::hud::UiArt::crop(
                        "Interface\\Buttons\\UI-Quickslot2",
                        [0.0, 1.0, 0.0, 1.0],
                    ),
                    Pickable::IGNORE,
                ));
                if key == KeyCode::KeyC {
                    slot.spawn((
                        CamGlow,
                        // A gold rim, the checked state. `CheckButtonHilight` is drawn additive in
                        // the client; bevy_ui blends it normally, a black square over the icon.
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            border: UiRect::all(Val::Px(3.0)),
                            border_radius: BorderRadius::all(Val::Px(3.0)),
                            ..default()
                        },
                        BorderColor::all(Color::srgb(1.0, 0.82, 0.0)),
                        BoxShadow::new(
                            Color::srgba(1.0, 0.75, 0.1, 0.8),
                            Val::Px(0.0),
                            Val::Px(0.0),
                            Val::Px(2.0),
                            Val::Px(8.0),
                        ),
                        Pickable::IGNORE,
                        Visibility::Hidden,
                    ));
                }
            });
    }
}

#[allow(clippy::type_complexity)]
fn show_overlay(
    phase: Res<State<Phase>>,
    state: Res<TouchState>,
    rig: Res<CamRig>,
    run: Res<super::Run>,
    ui_scale: Res<UiScale>,
    mut commands: Commands,
    mobility_icon: Query<(Entity, Option<&super::hud::SpellIcon>), With<MobilityButtonIcon>>,
    mut glow: Query<
        &mut Visibility,
        (
            With<CamGlow>,
            Without<PlayButton>,
            Without<StickRing>,
            Without<StickKnob>,
        ),
    >,
    mut ring: Query<(&mut Node, &mut Visibility), (With<StickRing>, Without<StickKnob>)>,
    mut knob: Query<(&mut Node, &mut Visibility), (With<StickKnob>, Without<StickRing>)>,
    mut play: Query<
        (&PlayButton, &mut Visibility),
        (Without<StickRing>, Without<StickKnob>, Without<CamGlow>),
    >,
) {
    let playing = *phase.get() == Phase::Playing;
    let shown = |v: bool| {
        if v {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    };
    let mobility = run.mobility();
    for (button, mut v) in &mut play {
        let learned = button.0 != super::mobility::KEY || mobility.is_some();
        v.set_if_neq(shown(playing && learned));
    }
    // The class's spell, through the HUD's own icon fill (`hud::fill_icons`).
    if let (Some(def), Ok((e, have))) = (mobility, mobility_icon.single()) {
        if have.is_none_or(|s| s.0 != def.spell) {
            commands
                .entity(e)
                .remove::<super::hud::IconDone>()
                .insert(super::hud::SpellIcon(def.spell));
        }
    }
    for mut v in &mut glow {
        v.set_if_neq(shown(rig.mode == CamMode::Orbit));
    }
    let stick = state.fingers.values().find_map(|(r, at)| match r {
        Role::Stick { origin } => Some((*origin, *at)),
        _ => None,
    });
    // Touches are in logical pixels and the UI in its scaled units.
    let k = 1.0 / ui_scale.0.max(0.01);
    let place = |node: &mut Node, centre: Vec2, radius: f32| {
        node.left = Val::Px((centre.x - radius) * k);
        node.top = Val::Px((centre.y - radius) * k);
        node.width = Val::Px(radius * 2.0 * k);
        node.height = Val::Px(radius * 2.0 * k);
    };
    if let Ok((mut node, mut v)) = ring.single_mut() {
        v.set_if_neq(shown(playing && stick.is_some()));
        if let Some((origin, _)) = stick {
            place(&mut node, origin, STICK_RADIUS);
        }
    }
    if let Ok((mut node, mut v)) = knob.single_mut() {
        v.set_if_neq(shown(playing && stick.is_some()));
        if let Some((origin, at)) = stick {
            let at = origin + (at - origin).clamp_length_max(STICK_RADIUS);
            place(&mut node, at, STICK_RADIUS * 0.4);
        }
    }
}
