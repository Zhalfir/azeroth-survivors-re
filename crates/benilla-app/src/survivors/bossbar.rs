//! Boss frames and boss nameplates. Survivors' lean set turns the client's nameplates off, so a
//! boss out of sight, or one nobody noticed arrive, was never found and never killed. Two things
//! show every living boss:
//!
//! * the frames, top centre under the clock: one row per boss (the final boss first, then in
//!   arrival order), at most [`MAX_ROWS`] with a "+N" for the rest. A row eases in when its boss
//!   arrives and fades when it dies;
//! * a nameplate over each boss's head, name and a thin bar, clamped to the screen edge with an
//!   arrow when the boss is off screen.
//!
//! Both are pooled UI nodes built once under the HUD root (so they hide with it). Text and bar
//! widths are written only when their value changes.

use std::collections::HashMap;

use benilla_ui::script::nameplate;
use benilla_world::view::WorldCamera;
use bevy::prelude::*;

use super::enemies::Enemy;
use super::hud::{self, HudFont, HudRoot, UiArt};
use super::{Phase, Run};

/// Rows shown at once; the rest collapse into "+N".
pub(super) const MAX_ROWS: usize = 4;
/// Nameplates pooled: the regular bosses and the final one with room to spare.
const MAX_PLATES: usize = 6;

/// The frames' top, in UI units: under the clock, the level line and the demo label.
const FRAMES_TOP: f32 = 92.0;
const FRAME_W: f32 = 320.0;
const NAME_SIZE: f32 = 18.0;
const PCT_SIZE: f32 = 16.0;
const MORE_SIZE: f32 = 16.0;
const ICON: f32 = 20.0;
const BAR_H: f32 = 14.0;

const PLATE_W: f32 = 190.0;
const PLATE_BAR_W: f32 = 150.0;
const PLATE_BAR_H: f32 = 7.0;
const PLATE_NAME: f32 = 16.0;
/// The plate's box: the name line over the bar.
const PLATE_H: f32 = 30.0;
/// Keep-out at the screen edges for a clamped plate, in UI units: the bottom clears the XP bar
/// and the gesture-bar zone, the top the clock line.
const EDGE_X: f32 = 8.0;
const EDGE_TOP: f32 = 64.0;
const EDGE_BOTTOM: f32 = hud::SAFE_BOTTOM + 12.0 + 17.0 + 8.0;
/// A boss's plate floats this far over the model's top, in world units.
const PLATE_LIFT: f32 = 0.6;

/// The ease-in and the fade-out, in seconds.
const EASE_IN: f32 = 0.3;
const FADE_OUT: f32 = 0.6;
/// Slide travel of the ease-in, in UI units.
const SLIDE: f32 = 14.0;

const NAME_GOLD: Color = Color::srgb(1.0, 0.82, 0.0);
const FILL_RED: Color = Color::srgb(0.78, 0.08, 0.06);
const BAR_BACK: Color = Color::srgb(0.06, 0.02, 0.02);
const FRAME_EDGE: Color = Color::srgb(0.55, 0.45, 0.25);

// --- Pure logic ---

/// What a row needs to be ordered: whether it is the final boss, and its arrival number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RowKey {
    pub final_boss: bool,
    pub seq: u32,
}

/// The rows to draw: indices into the input, final boss first, then by arrival, at most `cap`;
/// and how many more there are.
pub(super) fn rows_of(items: &[RowKey], cap: usize) -> (Vec<usize>, usize) {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&i| (!items[i].final_boss, items[i].seq));
    let more = order.len().saturating_sub(cap);
    order.truncate(cap);
    (order, more)
}

/// A boss's health as a whole percent, rounded up so a living boss never reads 0 %.
pub(super) fn percent(hp: f32, max_hp: f32) -> u32 {
    if hp <= 0.0 || max_hp <= 0.0 {
        return 0;
    }
    ((hp / max_hp * 100.0).ceil() as u32).clamp(1, 100)
}

/// Where a plate sits: its centre in UI units, and when its boss is off screen, the heading of
/// the arrow (radians clockwise from screen right, y down) from the screen centre to the boss.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Placed {
    pub center: Vec2,
    pub arrow: Option<f32>,
}

/// Keep a plate's box of half-size `half` inside the view, less the keep-outs `min` and `edge_max`
/// (distances from the top-left and bottom-right corners). A plate that moved is off screen and
/// carries an arrow toward `target`.
pub(super) fn clamp_plate(
    target: Vec2,
    half: Vec2,
    view: Vec2,
    keep_min: Vec2,
    keep_max: Vec2,
) -> Placed {
    let lo = keep_min + half;
    let hi = (view - keep_max - half).max(lo);
    let center = target.clamp(lo, hi);
    let arrow = (center != target).then(|| {
        let d = target - view / 2.0;
        d.y.atan2(d.x)
    });
    Placed { center, arrow }
}

/// Ease-out cubic, 0 to 1.
fn ease_out(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

// --- State ---

struct Row {
    entity: Entity,
    name: &'static str,
    final_boss: bool,
    seq: u32,
    frac: f32,
    pct: u32,
    t_in: f32,
    dying: Option<f32>,
}

impl Row {
    fn alpha(&self) -> f32 {
        let fade = self.dying.map_or(1.0, |d| 1.0 - (d / FADE_OUT).clamp(0.0, 1.0));
        ease_out(self.t_in / EASE_IN) * fade
    }
}

/// What a pooled frame row last showed, so only changes are written.
#[derive(Default)]
struct Shown {
    entity: Option<Entity>,
    pct: Option<u32>,
    frac: f32,
    alpha: f32,
    final_boss: bool,
    visible: bool,
}

struct Slot {
    root: Entity,
    name: Entity,
    pct: Entity,
    bar: Entity,
    fill: Entity,
    skull: Entity,
    shown: Shown,
}

struct Plate {
    root: Entity,
    name: Entity,
    fill: Entity,
    arrow: Entity,
    entity: Option<Entity>,
    frac: f32,
    pos: Vec2,
    arrow_on: bool,
    visible: bool,
}

#[derive(Resource)]
pub(super) struct BossUi {
    rows: Vec<Row>,
    next_seq: u32,
    seqs: HashMap<Entity, u32>,
    slots: Vec<Slot>,
    more: Entity,
    more_n: usize,
    plates: Vec<Plate>,
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            build.run_if(not(resource_exists::<BossUi>)),
            update.run_if(resource_exists::<BossUi>),
        )
            .chain()
            .after(hud::hero_plate),
    )
    .add_systems(OnEnter(Phase::Menu), reset);
}

fn with_alpha(c: Color, a: f32) -> Color {
    c.with_alpha(a)
}

fn label(font: &Handle<Font>, s: &str, size: f32, color: Color) -> impl Bundle + use<> {
    hud::text_bundle(font, s.to_string(), size, color)
}

// --- Build ---

fn build(
    mut commands: Commands,
    root: Query<Entity, With<HudRoot>>,
    font: Option<Res<HudFont>>,
) {
    let (Ok(root), Some(font)) = (root.single(), font) else {
        return;
    };
    let f = &font.0;
    let mut slots = Vec::new();
    let mut plates = Vec::new();
    let mut more = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|hud_root| {
        hud_root
            .spawn(Node {
                position_type: PositionType::Absolute,
                top: Val::Px(FRAMES_TOP),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-FRAME_W / 2.0)),
                width: Val::Px(FRAME_W),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|frames| {
                for _ in 0..MAX_ROWS {
                    let mut ids = [Entity::PLACEHOLDER; 5];
                    let slot_root = frames
                        .spawn((
                            Node {
                                display: Display::None,
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(2.0),
                                ..default()
                            },
                            UiTransform::IDENTITY,
                        ))
                        .with_children(|row| {
                            row.spawn(Node {
                                width: Val::Percent(100.0),
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(6.0),
                                ..default()
                            })
                            .with_children(|line| {
                                ids[4] = line
                                    .spawn((
                                        Node {
                                            display: Display::None,
                                            width: Val::Px(ICON),
                                            height: Val::Px(ICON),
                                            ..default()
                                        },
                                        ImageNode {
                                            color: Color::NONE,
                                            ..default()
                                        },
                                        UiArt::crop(nameplate::SKULL_TEXTURE, [0.0, 1.0, 0.0, 1.0]),
                                    ))
                                    .id();
                                ids[0] = line
                                    .spawn((
                                        Node {
                                            flex_grow: 1.0,
                                            ..default()
                                        },
                                        label(f, "", NAME_SIZE, NAME_GOLD),
                                    ))
                                    .id();
                                ids[1] = line.spawn(label(f, "100%", PCT_SIZE, Color::WHITE)).id();
                            });
                            ids[2] = row
                                .spawn((
                                    Node {
                                        width: Val::Percent(100.0),
                                        height: Val::Px(BAR_H),
                                        border: UiRect::all(Val::Px(2.0)),
                                        ..default()
                                    },
                                    BackgroundColor(BAR_BACK),
                                    BorderColor::all(FRAME_EDGE),
                                ))
                                .with_children(|bar| {
                                    ids[3] = bar
                                        .spawn((
                                            Node {
                                                width: Val::Percent(100.0),
                                                height: Val::Percent(100.0),
                                                ..default()
                                            },
                                            BackgroundColor(FILL_RED),
                                        ))
                                        .id();
                                })
                                .id();
                        })
                        .id();
                    slots.push(Slot {
                        root: slot_root,
                        name: ids[0],
                        pct: ids[1],
                        bar: ids[2],
                        fill: ids[3],
                        skull: ids[4],
                        shown: Shown::default(),
                    });
                }
                more = frames
                    .spawn((
                        Node {
                            display: Display::None,
                            align_self: AlignSelf::Center,
                            ..default()
                        },
                        label(f, "", MORE_SIZE, NAME_GOLD),
                    ))
                    .id();
            });
        for _ in 0..MAX_PLATES {
            let mut ids = [Entity::PLACEHOLDER; 3];
            let plate_root = hud_root
                .spawn(Node {
                    display: Display::None,
                    position_type: PositionType::Absolute,
                    width: Val::Px(PLATE_W),
                    height: Val::Px(PLATE_H),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(2.0),
                    ..default()
                })
                .with_children(|plate| {
                    plate
                        .spawn(Node {
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(4.0),
                            ..default()
                        })
                        .with_children(|line| {
                            ids[2] = line
                                .spawn((
                                    Node {
                                        display: Display::None,
                                        ..default()
                                    },
                                    UiTransform::IDENTITY,
                                    label(f, ">", PLATE_NAME + 4.0, Color::srgb(1.0, 0.3, 0.2)),
                                ))
                                .id();
                            ids[0] = line.spawn(label(f, "", PLATE_NAME, NAME_GOLD)).id();
                        });
                    plate
                        .spawn((
                            Node {
                                width: Val::Px(PLATE_BAR_W),
                                height: Val::Px(PLATE_BAR_H),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(BAR_BACK),
                            BorderColor::all(FRAME_EDGE),
                        ))
                        .with_children(|bar| {
                            ids[1] = bar
                                .spawn((
                                    Node {
                                        width: Val::Percent(100.0),
                                        height: Val::Percent(100.0),
                                        ..default()
                                    },
                                    BackgroundColor(FILL_RED),
                                ))
                                .id();
                        });
                })
                .id();
            plates.push(Plate {
                root: plate_root,
                name: ids[0],
                fill: ids[1],
                arrow: ids[2],
                entity: None,
                frac: 1.0,
                pos: Vec2::splat(f32::NAN),
                arrow_on: false,
                visible: false,
            });
        }
    });
    commands.insert_resource(BossUi {
        rows: Vec::new(),
        next_seq: 0,
        seqs: HashMap::new(),
        slots,
        more,
        more_n: 0,
        plates,
    });
}

fn reset(ui: Option<ResMut<BossUi>>) {
    if let Some(mut ui) = ui {
        ui.rows.clear();
        ui.seqs.clear();
    }
}

// --- Update ---

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update(
    time: Res<Time>,
    run: Res<Run>,
    ui_scale: Res<UiScale>,
    mut ui: ResMut<BossUi>,
    bosses: Query<(Entity, &Enemy, &Transform)>,
    cams: Query<(&Camera, &Transform), (With<WorldCamera>, Without<Enemy>)>,
    mut nodes: Query<&mut Node>,
    mut texts: Query<&mut Text>,
    mut text_colors: Query<&mut TextColor>,
    mut bgs: Query<&mut BackgroundColor>,
    mut borders: Query<&mut BorderColor>,
    mut images: Query<&mut ImageNode>,
    mut ui_tf: Query<&mut UiTransform>,
) {
    let _ = &run;
    let dt = time.delta_secs();
    let ui = &mut *ui;

    // Sync the rows with the living bosses.
    let mut living: Vec<Entity> = Vec::new();
    for (entity, e, _) in &bosses {
        if !e.boss || e.dead {
            continue;
        }
        living.push(entity);
        let seq = *ui.seqs.entry(entity).or_insert_with(|| {
            ui.next_seq += 1;
            ui.next_seq
        });
        let frac = if e.max_hp > 0.0 {
            (e.hp / e.max_hp).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let pct = percent(e.hp, e.max_hp);
        match ui.rows.iter_mut().find(|r| r.entity == entity) {
            Some(r) => {
                r.frac = frac;
                r.pct = pct;
            }
            None => ui.rows.push(Row {
                entity,
                name: e.def.name,
                final_boss: e.final_boss,
                seq,
                frac,
                pct,
                t_in: 0.0,
                dying: None,
            }),
        }
    }
    for r in &mut ui.rows {
        if !living.contains(&r.entity) && r.dying.is_none() {
            r.dying = Some(0.0);
            r.frac = 0.0;
            r.pct = 0;
        }
        r.t_in += dt;
        if let Some(d) = &mut r.dying {
            *d += dt;
        }
    }
    ui.rows
        .retain(|r| r.dying.is_none_or(|d| d < FADE_OUT));
    let alive_rows: Vec<Entity> = ui.rows.iter().map(|r| r.entity).collect();
    ui.seqs.retain(|e, _| alive_rows.contains(e));

    // The frames.
    let keys: Vec<RowKey> = ui
        .rows
        .iter()
        .map(|r| RowKey {
            final_boss: r.final_boss,
            seq: r.seq,
        })
        .collect();
    let (shown, more) = rows_of(&keys, MAX_ROWS);
    for (i, slot) in ui.slots.iter_mut().enumerate() {
        let Some(&ri) = shown.get(i) else {
            if slot.shown.visible {
                slot.shown.visible = false;
                slot.shown.entity = None;
                if let Ok(mut n) = nodes.get_mut(slot.root) {
                    n.display = Display::None;
                }
            }
            continue;
        };
        let row = &ui.rows[ri];
        let s = &mut slot.shown;
        if !s.visible {
            s.visible = true;
            s.alpha = -1.0;
            if let Ok(mut n) = nodes.get_mut(slot.root) {
                n.display = Display::Flex;
            }
        }
        if s.entity != Some(row.entity) {
            s.entity = Some(row.entity);
            s.pct = None;
            s.frac = -1.0;
            s.alpha = -1.0;
            if let Ok(mut t) = texts.get_mut(slot.name) {
                t.0 = row.name.to_string();
            }
            s.final_boss = row.final_boss;
            if let Ok(mut n) = nodes.get_mut(slot.skull) {
                n.display = if row.final_boss {
                    Display::Flex
                } else {
                    Display::None
                };
            }
        }
        if s.pct != Some(row.pct) {
            s.pct = Some(row.pct);
            if let Ok(mut t) = texts.get_mut(slot.pct) {
                t.0 = format!("{}%", row.pct);
            }
        }
        if (s.frac - row.frac).abs() > 1e-4 {
            s.frac = row.frac;
            if let Ok(mut n) = nodes.get_mut(slot.fill) {
                n.width = Val::Percent(row.frac * 100.0);
            }
        }
        let a = row.alpha();
        if (s.alpha - a).abs() > 1e-3 {
            s.alpha = a;
            if let Ok(mut c) = text_colors.get_mut(slot.name) {
                c.0 = with_alpha(NAME_GOLD, a);
            }
            if let Ok(mut c) = text_colors.get_mut(slot.pct) {
                c.0 = with_alpha(Color::WHITE, a);
            }
            if let Ok(mut c) = bgs.get_mut(slot.bar) {
                c.0 = with_alpha(BAR_BACK, a);
            }
            if let Ok(mut c) = bgs.get_mut(slot.fill) {
                c.0 = with_alpha(FILL_RED, a);
            }
            if let Ok(mut c) = borders.get_mut(slot.bar) {
                *c = BorderColor::all(with_alpha(FRAME_EDGE, a));
            }
            if let Ok(mut img) = images.get_mut(slot.skull) {
                // Before the art lands the tint stays clear; after, it is white.
                if img.image != Handle::default() {
                    img.color = with_alpha(Color::WHITE, a);
                }
            }
            if let Ok(mut t) = ui_tf.get_mut(slot.root) {
                let slide = -SLIDE * (1.0 - ease_out(row.t_in / EASE_IN));
                t.translation = Val2::new(Val::Px(0.0), Val::Px(slide));
            }
        }
    }
    if ui.more_n != more {
        ui.more_n = more;
        if let Ok(mut n) = nodes.get_mut(ui.more) {
            n.display = if more > 0 {
                Display::Flex
            } else {
                Display::None
            };
        }
        if let Ok(mut t) = texts.get_mut(ui.more) {
            t.0 = if more > 0 {
                format!("+{more}")
            } else {
                String::new()
            };
        }
    }

    // The nameplates: one per living boss, in the frames' order.
    let view = cams.single().ok().and_then(|(cam, tf)| {
        cam.logical_viewport_size()
            .map(|v| (cam, GlobalTransform::from(*tf), v))
    });
    let s = ui_scale.0.max(0.01);
    let mut living_keys: Vec<(RowKey, Entity)> = ui
        .rows
        .iter()
        .filter(|r| r.dying.is_none())
        .map(|r| {
            (
                RowKey {
                    final_boss: r.final_boss,
                    seq: r.seq,
                },
                r.entity,
            )
        })
        .collect();
    living_keys.sort_by_key(|(k, _)| (!k.final_boss, k.seq));
    for (i, plate) in ui.plates.iter_mut().enumerate() {
        let target = living_keys.get(i).map(|&(_, e)| e);
        let placed = target.zip(view.as_ref()).and_then(|(entity, (cam, gtf, vp))| {
            let (_, e, tf) = bosses.get(entity).ok()?;
            let top = tf.translation + Vec3::Y * (e.radius * 2.0 + PLATE_LIFT);
            let px = cam.world_to_viewport(gtf, top).ok()?;
            let placed = clamp_plate(
                px / s,
                Vec2::new(PLATE_W / 2.0, PLATE_H / 2.0),
                *vp / s,
                Vec2::new(EDGE_X, EDGE_TOP),
                Vec2::new(EDGE_X, EDGE_BOTTOM),
            );
            Some((entity, e.def.name, (e.hp / e.max_hp.max(1.0)).clamp(0.0, 1.0), placed))
        });
        let Some((entity, name, frac, placed)) = placed else {
            if plate.visible {
                plate.visible = false;
                plate.entity = None;
                if let Ok(mut n) = nodes.get_mut(plate.root) {
                    n.display = Display::None;
                }
            }
            continue;
        };
        if !plate.visible {
            plate.visible = true;
            plate.pos = Vec2::splat(f32::NAN);
            if let Ok(mut n) = nodes.get_mut(plate.root) {
                n.display = Display::Flex;
            }
        }
        if plate.entity != Some(entity) {
            plate.entity = Some(entity);
            plate.frac = -1.0;
            if let Ok(mut t) = texts.get_mut(plate.name) {
                t.0 = name.to_string();
            }
        }
        if (plate.frac - frac).abs() > 1e-4 {
            plate.frac = frac;
            if let Ok(mut n) = nodes.get_mut(plate.fill) {
                n.width = Val::Percent(frac * 100.0);
            }
        }
        let topleft = placed.center - Vec2::new(PLATE_W / 2.0, PLATE_H / 2.0);
        if plate.pos.distance_squared(topleft) > 0.0625 || plate.pos.is_nan() {
            plate.pos = topleft;
            if let Ok(mut n) = nodes.get_mut(plate.root) {
                n.left = Val::Px(topleft.x);
                n.top = Val::Px(topleft.y);
            }
        }
        let on = placed.arrow.is_some();
        if on != plate.arrow_on {
            plate.arrow_on = on;
            if let Ok(mut n) = nodes.get_mut(plate.arrow) {
                n.display = if on { Display::Flex } else { Display::None };
            }
        }
        if let (Some(angle), Ok(mut t)) = (placed.arrow, ui_tf.get_mut(plate.arrow)) {
            let want = Rot2::radians(angle);
            if t.rotation != want {
                t.rotation = want;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(final_boss: bool, seq: u32) -> RowKey {
        RowKey { final_boss, seq }
    }

    #[test]
    fn final_boss_first_then_arrival() {
        // Arrival order a(1), b(2), final(3), c(4) listed scrambled.
        let items = [key(false, 4), key(true, 3), key(false, 1), key(false, 2)];
        let (rows, more) = rows_of(&items, 4);
        assert_eq!(rows, vec![1, 2, 3, 0]);
        assert_eq!(more, 0);
    }

    #[test]
    fn cap_of_four_with_a_plus_n() {
        let items: Vec<RowKey> = (1..=7).map(|s| key(false, s)).collect();
        let (rows, more) = rows_of(&items, MAX_ROWS);
        assert_eq!(rows, vec![0, 1, 2, 3]);
        assert_eq!(more, 3);
        // The final boss survives the cap even when it arrives last.
        let mut items = items;
        items.push(key(true, 8));
        let (rows, more) = rows_of(&items, MAX_ROWS);
        assert_eq!(rows[0], 7);
        assert_eq!(more, 4);
        assert_eq!(rows_of(&[], MAX_ROWS), (vec![], 0));
    }

    #[test]
    fn percent_rounds_up_and_never_reads_zero_alive() {
        assert_eq!(percent(100.0, 100.0), 100);
        assert_eq!(percent(50.0, 100.0), 50);
        assert_eq!(percent(50.2, 100.0), 51);
        assert_eq!(percent(0.1, 1000.0), 1);
        assert_eq!(percent(0.0, 100.0), 0);
        assert_eq!(percent(-5.0, 100.0), 0);
        assert_eq!(percent(150.0, 100.0), 100);
        assert_eq!(percent(10.0, 0.0), 0);
    }

    #[test]
    fn on_screen_plate_is_untouched() {
        let view = Vec2::new(1000.0, 768.0);
        let p = clamp_plate(
            Vec2::new(500.0, 300.0),
            Vec2::new(95.0, 15.0),
            view,
            Vec2::new(8.0, 64.0),
            Vec2::new(8.0, 80.0),
        );
        assert_eq!(p.center, Vec2::new(500.0, 300.0));
        assert_eq!(p.arrow, None);
    }

    #[test]
    fn off_screen_plate_clamps_to_the_edge_with_an_arrow() {
        let view = Vec2::new(1000.0, 768.0);
        let half = Vec2::new(95.0, 15.0);
        let (kmin, kmax) = (Vec2::new(8.0, 64.0), Vec2::new(8.0, 80.0));
        // Far right, level with the centre: pinned to the right edge, arrow points right (0).
        let p = clamp_plate(Vec2::new(2000.0, 384.0), half, view, kmin, kmax);
        assert_eq!(p.center, Vec2::new(1000.0 - 8.0 - 95.0, 384.0));
        assert!(p.arrow.unwrap().abs() < 1e-4);
        // Far below: pinned over the bottom keep-out, arrow points down (+90 degrees).
        let p = clamp_plate(Vec2::new(500.0, 3000.0), half, view, kmin, kmax);
        assert_eq!(p.center.y, 768.0 - 80.0 - 15.0);
        assert!((p.arrow.unwrap() - std::f32::consts::FRAC_PI_2).abs() < 0.01);
        // Above the top: pinned under the clock line, arrow points up (-90 degrees).
        let p = clamp_plate(Vec2::new(500.0, -400.0), half, view, kmin, kmax);
        assert_eq!(p.center.y, 64.0 + 15.0);
        assert!((p.arrow.unwrap() + std::f32::consts::FRAC_PI_2).abs() < 0.01);
        // A corner: both axes clamp.
        let p = clamp_plate(Vec2::new(-500.0, -500.0), half, view, kmin, kmax);
        assert_eq!(p.center, Vec2::new(8.0 + 95.0, 64.0 + 15.0));
        assert!(p.arrow.is_some());
    }

    #[test]
    fn a_view_too_small_for_the_plate_does_not_panic() {
        let p = clamp_plate(
            Vec2::new(10.0, 10.0),
            Vec2::new(95.0, 15.0),
            Vec2::new(100.0, 50.0),
            Vec2::new(8.0, 64.0),
            Vec2::new(8.0, 80.0),
        );
        assert!(p.center.is_finite());
    }
}
