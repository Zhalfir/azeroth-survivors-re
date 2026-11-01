//! The damage meter, in the manner of Recount and Details!: every hit is credited to the spell that
//! dealt it ([`super::Run::record`]), and the lines are shown twice — the whole breakdown on the
//! results screen, and the top few live in a corner during the run (a setting, on by default,
//! kept in `benilla-config/meter.txt`).

use bevy::prelude::*;

use super::hud::{spell_name, HudFont, SpellIcon};
use super::{Phase, Run};

/// Lines on the results screen, and live.
const RESULTS_LINES: usize = 8;
const LIVE_LINES: usize = 5;
/// The live meter redraws this often, in real seconds.
const LIVE_EVERY: f32 = 0.5;
/// The live meter's line height, text size and width, in UI units: its text at the HUD's floor
/// and a step over, to read on a tablet (it was 17 / 11 / 210).
const LIVE_LINE_H: f32 = 21.0;
const LIVE_TEXT: f32 = super::hud::TEXT_FLOOR + 1.0;
const LIVE_W: f32 = 250.0;

const BAR_TOP: Color = Color::srgba(0.75, 0.52, 0.12, 0.9);
const BAR_BOTTOM: Color = Color::srgba(0.42, 0.24, 0.04, 0.9);
const TRACK: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);

/// Whether the live meter shows.
#[derive(Resource)]
pub(super) struct MeterSetting {
    pub(super) live: bool,
}

/// The results screen's slot for the breakdown, placed by `hud::setup_hud`.
#[derive(Component)]
pub(super) struct MeterList;

/// The live meter's panel.
#[derive(Component)]
struct LiveMeter;

/// A live line's bar and its numbers, by row: updated in place between rebuilds.
#[derive(Component)]
struct LiveBar(usize);
#[derive(Component)]
struct LiveNumbers(usize);

/// One meter line, worked out for display.
struct Line {
    source: u32,
    name: String,
    damage: f64,
    /// Of the run's total.
    share: f32,
    /// Of the top line's damage, for the bar.
    bar: f32,
    dps: f64,
}

fn setting_path() -> Option<std::path::PathBuf> {
    crate::local_state::home().map(|h| h.join("meter.txt"))
}

pub(super) fn save_setting(setting: &MeterSetting) {
    if let Some(path) = setting_path() {
        let _ = crate::local_state::write_atomic(&path, if setting.live { "on" } else { "off" });
    }
}

pub(super) fn plugin(app: &mut App) {
    let live = setting_path()
        .and_then(|p| crate::local_state::read_to_string(&p).ok())
        .is_none_or(|s| s.trim() != "off");
    app.insert_resource(MeterSetting { live })
        .add_systems(Startup, spawn_live)
        .add_systems(OnEnter(Phase::GameOver), fill_results)
        .add_systems(Update, refresh_live);
}

/// The run's lines, most damage first.
fn lines(run: &Run, spells: Option<&crate::ui_action::Spells>) -> Vec<Line> {
    let total = run.damage_done.max(1.0);
    let mut rows = run.meter.clone();
    rows.sort_by(|a, b| b.damage.total_cmp(&a.damage));
    let top = rows.first().map_or(1.0, |r| r.damage.max(1.0));
    rows.iter()
        .map(|r| {
            let owned = run.abilities.iter().find(|o| o.def.spell == r.source);
            let name = match owned {
                Some(o) => spell_name(spells, o.def.spell, o.def.key),
                None if r.source == 0 => "Other".to_string(),
                None => spell_name(spells, r.source, "Other"),
            };
            // At least five seconds, so an ability's first burst does not read as its pace.
            let secs = f64::from((run.elapsed - r.since).max(5.0));
            Line {
                source: r.source,
                name,
                damage: r.damage,
                share: (r.damage / total) as f32,
                bar: (r.damage / top) as f32,
                dps: r.damage / secs,
            }
        })
        .collect()
}

/// 12.3k, 4.5M.
fn compact(v: f64) -> String {
    if v >= 1e6 {
        format!("{:.1}M", v / 1e6)
    } else if v >= 1e4 {
        format!("{:.1}k", v / 1e3)
    } else {
        format!("{}", v.round() as u64)
    }
}

fn label(font: &Handle<Font>, s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(s),
        TextFont {
            font: font.clone(),
            font_size: size,
            ..default()
        },
        TextColor(color),
        TextShadow {
            offset: Vec2::splat(1.0),
            color: Color::srgba(0.0, 0.0, 0.0, 0.85),
        },
        Pickable::IGNORE,
    )
}

/// One line: the spell's icon, then a bar the length of its damage against the top line's, with
/// the name over its left end and the numbers over its right.
fn spawn_line(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    line: &Line,
    height: f32,
    text_size: f32,
    numbers: String,
    slot: Option<usize>,
) {
    parent
        .spawn((
            Node {
                height: Val::Px(height),
                column_gap: Val::Px(4.0),
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    width: Val::Px(height),
                    height: Val::Px(height),
                    flex_shrink: 0.0,
                    ..default()
                },
                ImageNode {
                    color: Color::NONE,
                    ..default()
                },
                SpellIcon(line.source),
                Pickable::IGNORE,
            ));
            row.spawn((
                Node {
                    flex_grow: 1.0,
                    height: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::horizontal(Val::Px(5.0)),
                    ..default()
                },
                BackgroundColor(TRACK),
                Pickable::IGNORE,
            ))
            .with_children(|bar| {
                let mut fill = bar.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        bottom: Val::Px(0.0),
                        width: Val::Percent(line.bar.clamp(0.0, 1.0) * 100.0),
                        ..default()
                    },
                    BackgroundGradient::from(LinearGradient::to_bottom(vec![
                        ColorStop::percent(BAR_TOP, 0.0),
                        ColorStop::percent(BAR_BOTTOM, 100.0),
                    ])),
                    Pickable::IGNORE,
                ));
                if let Some(i) = slot {
                    fill.insert(LiveBar(i));
                }
                bar.spawn(label(font, line.name.clone(), text_size, Color::WHITE));
                let mut n = bar.spawn(label(font, numbers, text_size, Color::srgb(1.0, 0.92, 0.7)));
                if let Some(i) = slot {
                    n.insert(LiveNumbers(i));
                }
            });
        });
}

fn fill_results(
    mut commands: Commands,
    run: Res<Run>,
    spells: Option<Res<crate::ui_action::Spells>>,
    font: Option<Res<HudFont>>,
    lists: Query<Entity, With<MeterList>>,
) {
    let Some(font) = font else { return };
    let lines = lines(&run, spells.as_deref());
    for list in &lists {
        commands.entity(list).despawn_related::<Children>();
        commands.entity(list).with_children(|ui| {
            if lines.is_empty() {
                return;
            }
            ui.spawn(label(
                &font.0,
                "Damage done",
                15.0,
                Color::srgb(1.0, 0.82, 0.0),
            ));
            for line in lines.iter().take(RESULTS_LINES) {
                let numbers = format!(
                    "{}  ({:.0}%)  {}/s",
                    compact(line.damage),
                    line.share * 100.0,
                    compact(line.dps)
                );
                spawn_line(ui, &font.0, line, 24.0, 14.0, numbers, None);
            }
        });
    }
}

fn spawn_live(mut commands: Commands) {
    commands.spawn((
        LiveMeter,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(10.0),
            top: Val::Px(110.0),
            width: Val::Px(LIVE_W),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            display: Display::None,
            ..default()
        },
        GlobalZIndex(200),
        Pickable::IGNORE,
    ));
}

fn refresh_live(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut next_at: Local<f32>,
    phase: Res<State<Phase>>,
    setting: Res<MeterSetting>,
    run: Res<Run>,
    spells: Option<Res<crate::ui_action::Spells>>,
    font: Option<Res<HudFont>>,
    mut panel: Query<(Entity, &mut Node), (With<LiveMeter>, Without<LiveBar>)>,
    mut bars: Query<(&LiveBar, &mut Node), Without<LiveMeter>>,
    mut numbers: Query<(&LiveNumbers, &mut Text)>,
    mut order: Local<Vec<u32>>,
) {
    let Ok((e, mut node)) = panel.single_mut() else {
        return;
    };
    let in_run = matches!(phase.get(), Phase::Playing | Phase::Paused);
    let shown = in_run && setting.live && !run.meter.is_empty();
    let display = if shown { Display::Flex } else { Display::None };
    if node.display != display {
        node.display = display;
    }
    let now = time.elapsed_secs();
    if !shown || (now < *next_at && !setting.is_changed()) {
        return;
    }
    *next_at = now + LIVE_EVERY;
    let Some(font) = font else { return };
    let lines = lines(&run, spells.as_deref());
    let shown_lines = &lines[..lines.len().min(LIVE_LINES)];
    let now_order: Vec<u32> = shown_lines.iter().map(|l| l.source).collect();
    // The same spells in the same order: update the bars and numbers in place. A rebuild
    // re-lays every text and re-finds every icon; twice a second it cost frames on the phone.
    if *order == now_order {
        for (bar, mut node) in &mut bars {
            if let Some(line) = shown_lines.get(bar.0) {
                let want = Val::Percent(line.bar.clamp(0.0, 1.0) * 100.0);
                if node.width != want {
                    node.width = want;
                }
            }
        }
        for (slot, mut text) in &mut numbers {
            if let Some(line) = shown_lines.get(slot.0) {
                let want = format!("{}/s", compact(line.dps));
                if text.0 != want {
                    text.0 = want;
                }
            }
        }
        return;
    }
    *order = now_order;
    commands.entity(e).despawn_related::<Children>();
    commands.entity(e).with_children(|ui| {
        for (i, line) in shown_lines.iter().enumerate() {
            let numbers = format!("{}/s", compact(line.dps));
            spawn_line(ui, &font.0, line, LIVE_LINE_H, LIVE_TEXT, numbers, Some(i));
        }
    });
}
