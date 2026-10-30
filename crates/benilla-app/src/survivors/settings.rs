//! The pause menu's Settings panel: WoW's own sound options — Master, Sound effects, Music and
//! Ambience, each a volume and (but the effects) an on/off switch, the 1.12 CVars the mixer already
//! follows and `config.toml` already keeps — and the live damage meter's switch
//! ([`super::meter`]). Touch-sized: volumes step in tens with − and +.

use bevy::prelude::*;

use super::hud::{HudFont, UiAction};
use super::meter::{save_setting, MeterSetting};
use super::Phase;
use crate::cvars::Cvars;

/// The sound rows: label, volume CVar, and the switch CVar where the row has one.
const SOUND: [(&str, &str, Option<&str>); 4] = [
    ("Master", "MasterVolume", Some("MasterSoundEffects")),
    ("Sound effects", "SoundVolume", None),
    ("Music", "MusicVolume", Some("EnableMusic")),
    ("Ambience", "AmbienceVolume", Some("EnableAmbience")),
];
const STEP: f32 = 0.1;

const GOLD: Color = Color::srgb(1.0, 0.82, 0.0);
const BUTTON: Color = Color::srgba(0.16, 0.11, 0.04, 0.95);
const BUTTON_HOT: Color = Color::srgba(0.34, 0.22, 0.06, 0.95);
const EDGE: Color = Color::srgba(0.8, 0.62, 0.25, 0.9);

/// Whether the panel is open over the pause menu.
#[derive(Resource, Default)]
struct SettingsOpen(bool);

#[derive(Component)]
struct SettingsRoot;

#[derive(Component, Clone, Copy)]
enum SettingsButton {
    Volume(&'static str, f32),
    Toggle(&'static str),
    Meter,
    Back,
}

/// A label kept in step with a CVar: a volume in percent, or a switch's On / Off.
#[derive(Component, Clone, Copy)]
enum Readout {
    Volume(&'static str),
    Toggle(&'static str),
    Meter,
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<SettingsOpen>()
        .add_systems(Update, (open_close, press, show, readouts).chain())
        .add_systems(OnExit(Phase::Paused), |mut open: ResMut<SettingsOpen>| {
            open.0 = false;
        });
}

fn text(font: &Handle<Font>, s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(s),
        TextFont {
            font: font.clone(),
            font_size: size,
            ..default()
        },
        TextColor(color),
        Pickable::IGNORE,
    )
}

fn button(
    parent: &mut ChildSpawnerCommands,
    action: SettingsButton,
    width: f32,
    label: impl Bundle,
) {
    parent
        .spawn((
            Button,
            action,
            Node {
                width: Val::Px(width),
                height: Val::Px(40.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(BUTTON),
            BorderColor::all(EDGE),
        ))
        .with_children(|b| {
            b.spawn(label);
        });
}

/// Build the panel on first open, after the HUD's font is in.
fn spawn_panel(commands: &mut Commands, font: &Handle<Font>) {
    commands
        .spawn((
            SettingsRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(10.0),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.015, 0.01, 0.96)),
            GlobalZIndex(435),
        ))
        .with_children(|root| {
            root.spawn(text(font, "Settings", 40.0, GOLD));
            root.spawn(text(font, "Sound", 20.0, GOLD));
            for (name, volume, switch) in SOUND {
                root.spawn(Node {
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Node {
                            width: Val::Px(150.0),
                            ..default()
                        },
                        Pickable::IGNORE,
                    ))
                    .with_children(|c| {
                        c.spawn(text(font, name, 18.0, Color::WHITE));
                    });
                    button(
                        row,
                        SettingsButton::Volume(volume, -STEP),
                        44.0,
                        text(font, "-", 22.0, Color::WHITE),
                    );
                    row.spawn((
                        Node {
                            width: Val::Px(64.0),
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        Pickable::IGNORE,
                    ))
                    .with_children(|c| {
                        c.spawn((text(font, "", 18.0, Color::WHITE), Readout::Volume(volume)));
                    });
                    button(
                        row,
                        SettingsButton::Volume(volume, STEP),
                        44.0,
                        text(font, "+", 22.0, Color::WHITE),
                    );
                    match switch {
                        Some(cvar) => button(
                            row,
                            SettingsButton::Toggle(cvar),
                            70.0,
                            (text(font, "", 16.0, Color::WHITE), Readout::Toggle(cvar)),
                        ),
                        None => {
                            row.spawn(Node {
                                width: Val::Px(70.0),
                                ..default()
                            });
                        }
                    }
                });
            }
            root.spawn(text(font, "Interface", 20.0, GOLD));
            root.spawn(Node {
                column_gap: Val::Px(8.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Node {
                        width: Val::Px(310.0),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|c| {
                    c.spawn(text(
                        font,
                        "Damage meter during the run",
                        18.0,
                        Color::WHITE,
                    ));
                });
                button(
                    row,
                    SettingsButton::Meter,
                    70.0,
                    (text(font, "", 16.0, Color::WHITE), Readout::Meter),
                );
            });
            root.spawn(Node {
                height: Val::Px(8.0),
                ..default()
            });
            button(
                root,
                SettingsButton::Back,
                240.0,
                text(font, "Back", 20.0, Color::WHITE),
            );
        });
}

fn open_close(
    mut commands: Commands,
    presses: Query<(&Interaction, &UiAction), Changed<Interaction>>,
    mut open: ResMut<SettingsOpen>,
    roots: Query<(), With<SettingsRoot>>,
    font: Option<Res<HudFont>>,
) {
    for (interaction, action) in &presses {
        if *interaction == Interaction::Pressed && *action == UiAction::Settings {
            if roots.is_empty() {
                let Some(font) = font.as_deref() else { return };
                spawn_panel(&mut commands, &font.0);
            }
            open.0 = true;
        }
    }
}

fn press(
    mut buttons: Query<(&Interaction, &SettingsButton, &mut BackgroundColor), Changed<Interaction>>,
    mut open: ResMut<SettingsOpen>,
    cvars: Option<ResMut<Cvars>>,
    mut meter: ResMut<MeterSetting>,
) {
    let Some(mut cvars) = cvars else { return };
    for (interaction, action, mut bg) in &mut buttons {
        bg.0 = match interaction {
            Interaction::None => BUTTON,
            _ => BUTTON_HOT,
        };
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *action {
            SettingsButton::Volume(cvar, delta) => {
                let now = volume(&cvars, cvar);
                let next = ((now + delta) * 10.0).round() / 10.0;
                cvars.set(cvar, &format!("{:.1}", next.clamp(0.0, 1.0)));
            }
            SettingsButton::Toggle(cvar) => {
                let on = switch(&cvars, cvar);
                cvars.set(cvar, if on { "0" } else { "1" });
            }
            SettingsButton::Meter => {
                meter.live = !meter.live;
                save_setting(&meter);
            }
            SettingsButton::Back => open.0 = false,
        }
    }
}

fn volume(cvars: &Cvars, cvar: &str) -> f32 {
    cvars
        .get(cvar)
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(1.0)
}

fn switch(cvars: &Cvars, cvar: &str) -> bool {
    cvars.get(cvar).is_none_or(|v| v.trim() != "0")
}

/// Open only over the pause menu; out of the layout when closed, so its buttons take no clicks.
fn show(
    open: Res<SettingsOpen>,
    phase: Res<State<Phase>>,
    mut roots: Query<&mut Node, With<SettingsRoot>>,
) {
    let on = open.0 && *phase.get() == Phase::Paused;
    let display = if on { Display::Flex } else { Display::None };
    for mut node in &mut roots {
        if node.display != display {
            node.display = display;
        }
    }
}

fn readouts(
    cvars: Option<Res<Cvars>>,
    meter: Res<MeterSetting>,
    mut texts: Query<(&Readout, &mut Text)>,
) {
    let Some(cvars) = cvars else { return };
    for (readout, mut text) in &mut texts {
        let want = match *readout {
            Readout::Volume(cvar) => format!("{:.0}%", volume(&cvars, cvar) * 100.0),
            Readout::Toggle(cvar) => on_off(switch(&cvars, cvar)),
            Readout::Meter => on_off(meter.live),
        };
        if text.0 != want {
            text.0 = want;
        }
    }
}

fn on_off(on: bool) -> String {
    if on { "On" } else { "Off" }.to_string()
}
