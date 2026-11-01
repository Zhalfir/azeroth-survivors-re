//! The Secrets page (`docs/superpowers/specs/2026-10-31-secret-races-design.md`): how many of the
//! 13 secret races the account has found, one card each. A **Secrets** button in the character
//! screen's left column ([`LeftSlot::Secrets`]) opens it over the menu; Back, Escape, the Android
//! back key or the gamepad's East button close it.
//!
//! - **Locked:** the race's silhouette, "???" and its riddle; never the condition.
//! - **Unlocked:** the race in full colour, its name, its classes' icons and, under a thin rule,
//!   the condition that opened it, with a NEW ribbon until the page has been opened once since.
//!
//! **The silhouette** is the race's own icon (`SecretDef::icon`) multiplied near black with a "?"
//! over it, an interim: survivors has no model-in-Bevy-UI path. `crate::ui_models` and
//! `crate::portrait` are FrameXML `<Model>` booths driven by the Lua UI, and a survivors unit is a
//! net entity the world camera draws, so a per-race viewport with a flat-colour override needs
//! render-layer and material plumbing in the engine's entity pipeline.
//!
//! The grid is 5 cards wide on the tablet's ~1230-unit-wide canvas, 4 (or 3) when narrower, and
//! scrolls (`Overflow::scroll_y`) by wheel, by touch drag and by moving the focus. A card is as
//! tall as its content (a row as its tallest card), so a riddle sits right under its "???"; a
//! "more" hint with a down chevron shows under the grid while rows remain below.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy::math::Rot2;
use bevy::ui::{UiGlobalTransform, UiTransform};
use bevy::window::PrimaryWindow;

use super::{def, Progress, SecretRace};
use crate::survivors::characters::LeftSlot;
use crate::survivors::hud::{HudFont, UiArt, SAFE_BOTTOM, TEXT_FLOOR};
use crate::survivors::Phase;

const GOLD: Color = Color::srgb(1.0, 0.82, 0.0);
const PALE_GOLD: Color = Color::srgb(0.95, 0.85, 0.6);
const DIM_GOLD: Color = Color::srgb(0.55, 0.45, 0.22);
const FILL: Color = Color::srgba(0.05, 0.035, 0.02, 0.97);
const LOCKED_FILL: Color = Color::srgba(0.03, 0.025, 0.02, 0.97);
const EDGE: Color = Color::srgba(0.8, 0.62, 0.25, 0.9);
const LOCKED_EDGE: Color = Color::srgba(0.4, 0.32, 0.16, 0.9);
const FOCUS_EDGE: Color = Color::srgb(1.0, 0.92, 0.45);
const BUTTON: Color = Color::srgba(0.16, 0.11, 0.04, 0.95);
const BUTTON_HOT: Color = Color::srgba(0.34, 0.22, 0.06, 0.95);
/// The silhouette: the icon multiplied to the near-black the brief's flat colour asks for.
const SILHOUETTE: Color = Color::srgb(0.09, 0.075, 0.06);
const RIBBON: Color = Color::srgb(0.7, 0.08, 0.04);

/// A card's width, UI units (768 to the screen's height); its height follows its content.
const CARD_W: f32 = 220.0;
const GAP: f32 = 12.0;
/// The page's side gutter.
const SIDE: f32 = 16.0;
const PORTRAIT: f32 = 84.0;
/// Class icons on an unlocked card: the brief's 32 floor; six (the Goblin) fit one row.
const CLASS_ICON: f32 = 32.0;
const CLASS_SHEET: &str = "Interface\\Glues\\CharacterCreate\\UI-CharacterCreate-Classes";
/// The riddle's size and its four-line cap.
const RIDDLE: f32 = 15.0;
const RIDDLE_LINES: f32 = 4.0;

/// One card as the page shows it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Entry {
    pub race: SecretRace,
    pub unlocked: bool,
    /// "???" while locked.
    pub name: &'static str,
    /// Only while locked.
    pub riddle: Option<&'static str>,
    /// Only once unlocked.
    pub condition: Option<&'static str>,
}

/// The 13 cards in [`SecretRace::ALL`]'s order, and how many are found.
pub(super) fn page_entries(p: &Progress) -> (Vec<Entry>, usize) {
    let entries: Vec<Entry> = SecretRace::ALL
        .into_iter()
        .map(|r| {
            let d = def(r);
            let unlocked = p.is_unlocked(r);
            Entry {
                race: r,
                unlocked,
                name: if unlocked { d.name } else { "???" },
                riddle: (!unlocked).then_some(d.riddle),
                condition: unlocked.then_some(d.condition),
            }
        })
        .collect();
    let found = entries.iter().filter(|e| e.unlocked).count();
    (entries, found)
}

/// The races unlocked but not yet seen on the page: their NEW ribbons.
pub(super) fn unseen(p: &Progress) -> Vec<SecretRace> {
    SecretRace::ALL
        .into_iter()
        .filter(|&r| p.is_unlocked(r) && !p.seen.iter().any(|k| k == def(r).key))
        .collect()
}

/// Cards a row holds at a canvas width, UI units: 5 on the tablet, fewer when narrower.
pub(super) fn columns(width: f32) -> usize {
    (3..=5)
        .rev()
        .find(|&n| grid_width(n) + 2.0 * SIDE <= width)
        .unwrap_or(3)
}

fn grid_width(cols: usize) -> f32 {
    cols as f32 * CARD_W + (cols.saturating_sub(1)) as f32 * GAP
}

/// The focus after an arrow step on a grid of `n` cards, `cols` wide; it stays put at an edge.
pub(super) fn step_focus(focus: usize, n: usize, cols: usize, dx: i32, dy: i32) -> usize {
    if n == 0 {
        return 0;
    }
    let (col, row) = ((focus % cols) as i32, (focus / cols) as i32);
    let (c, r) = (col + dx, row + dy);
    if c < 0 || c >= cols as i32 || r < 0 {
        return focus;
    }
    let i = (r as usize) * cols + c as usize;
    if i < n {
        i
    } else {
        focus
    }
}

/// The page: open, its build, the focused card, the NEW ribbons of this opening.
#[derive(Resource, Default)]
pub(in crate::survivors) struct SecretsPage {
    open: bool,
    version: u32,
    focus: usize,
    cols: usize,
    new: Vec<SecretRace>,
    /// The focus moved: scroll it into view.
    reveal_focus: bool,
}

impl SecretsPage {
    pub(in crate::survivors) fn is_open(&self) -> bool {
        self.open
    }
}

#[derive(Component)]
struct PageRoot;
#[derive(Component)]
struct PageTitle;
#[derive(Component)]
struct Grid;
#[derive(Component)]
struct Card(usize);
#[derive(Component)]
struct OpenButton;
#[derive(Component)]
struct OpenLabel;
/// The "more" hint under the grid, shown while rows remain below.
#[derive(Component)]
struct MoreHint;

#[derive(Component, Clone, Copy, PartialEq)]
enum PageButton {
    Open,
    Close,
}

/// A race icon on a card, drawn in `tint` once the archives are readable.
#[derive(Component)]
struct RaceIcon {
    path: &'static str,
    tint: Color,
}

#[derive(Component)]
struct RaceIconDone;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<SecretsPage>()
        .add_systems(
            Startup,
            spawn_ui.after(crate::survivors::characters::spawn_ui),
        )
        .add_systems(
            Update,
            (press, keys, show, layout, rebuild, focus_style, scroll, fill_icons).chain(),
        );
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
        TextShadow {
            offset: Vec2::splat(1.0),
            color: Color::srgba(0.0, 0.0, 0.0, 0.85),
        },
        TextLayout::new_with_justify(Justify::Center),
        Pickable::IGNORE,
    )
}

fn button(parent: &mut ChildSpawnerCommands, font: &Handle<Font>, action: PageButton, label: &str) {
    parent
        .spawn((
            Button,
            action,
            Node {
                width: Val::Px(236.0),
                height: Val::Px(44.0),
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
            let mut t = b.spawn(text(font, label, 18.0, Color::WHITE));
            if action == PageButton::Open {
                t.insert(OpenLabel);
            }
        });
}

fn spawn_ui(mut commands: Commands, font: Option<Res<HudFont>>, slots: Query<(Entity, &LeftSlot)>) {
    let Some(font) = font else { return };
    let f = &font.0;
    // Over the menu screen (410) and Talents (455), the whole screen; the Back row stays above the
    // gesture band.
    commands
        .spawn((
            PageRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(10.0),
                padding: UiRect {
                    left: Val::Px(SIDE),
                    right: Val::Px(SIDE),
                    top: Val::Px(14.0),
                    bottom: Val::Px(SAFE_BOTTOM + 6.0),
                },
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.94)),
            // Over the menu screen (410), so its zone list and buttons never show through.
            GlobalZIndex(455),
        ))
        .with_children(|root| {
            root.spawn((text(f, "", 28.0, GOLD), PageTitle));
            // The scrolling grid: it takes what the title and Back leave.
            root.spawn((
                Grid,
                Node {
                    flex_grow: 1.0,
                    flex_basis: Val::Px(0.0),
                    min_height: Val::Px(0.0),
                    width: Val::Px(grid_width(5)),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    align_content: AlignContent::FlexStart,
                    column_gap: Val::Px(GAP),
                    row_gap: Val::Px(GAP),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                ScrollPosition::default(),
            ));
            // Its place always kept, so the grid and Back never jump as it comes and goes.
            root.spawn((
                MoreHint,
                Node {
                    height: Val::Px(18.0),
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                    ..default()
                },
                Visibility::Hidden,
                Pickable::IGNORE,
            ))
            .with_children(|h| {
                // A chevron: a square's two lower edges, turned a quarter (the UI font has no ▼).
                h.spawn((
                    Node {
                        width: Val::Px(9.0),
                        height: Val::Px(9.0),
                        margin: UiRect::bottom(Val::Px(5.0)),
                        border: UiRect {
                            right: Val::Px(2.0),
                            bottom: Val::Px(2.0),
                            ..default()
                        },
                        ..default()
                    },
                    BorderColor::all(PALE_GOLD),
                    UiTransform::from_rotation(Rot2::degrees(45.0)),
                    Pickable::IGNORE,
                ));
                h.spawn(text(f, "more", TEXT_FLOOR, PALE_GOLD));
            });
            button(root, f, PageButton::Close, "Back");
        });
    // Its button, in the character screen's left column, under Records.
    let Some(slot) = slots
        .iter()
        .find_map(|(e, s)| (*s == LeftSlot::Secrets).then_some(e))
    else {
        return;
    };
    commands.entity(slot).with_children(|c| {
        c.spawn((
            OpenButton,
            Node {
                width: Val::Px(236.0),
                margin: UiRect::top(Val::Px(8.0)),
                display: Display::None,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|c| button(c, f, PageButton::Open, "Secrets"));
    });
}

/// Open the page: this opening's NEW ribbons are the unseen unlocks, which are then seen.
fn open(page: &mut SecretsPage, progress: &mut Progress) {
    page.open = true;
    page.focus = 0;
    page.reveal_focus = true;
    page.new = unseen(progress);
    page.version += 1;
    if !page.new.is_empty() {
        progress.seen = progress.unlocked.clone();
        super::save(progress);
    }
}

fn press(
    mut buttons: Query<(&Interaction, &PageButton, &mut BackgroundColor), Changed<Interaction>>,
    mut page: ResMut<SecretsPage>,
    mut progress: ResMut<Progress>,
) {
    for (interaction, action, mut bg) in &mut buttons {
        bg.0 = match interaction {
            Interaction::None => BUTTON,
            _ => BUTTON_HOT,
        };
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            PageButton::Open => open(&mut page, &mut progress),
            PageButton::Close => page.open = false,
        }
    }
}

/// Escape, the Android back key or the gamepad's East closes; arrows and the d-pad move the focus.
fn keys(keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, mut page: ResMut<SecretsPage>) {
    if !page.open {
        return;
    }
    let pad = |b: GamepadButton| pads.iter().any(|g| g.just_pressed(b));
    if keys.any_just_pressed([KeyCode::Escape, KeyCode::BrowserBack]) || pad(GamepadButton::East)
    {
        page.open = false;
        return;
    }
    let mut d = (0, 0);
    if keys.just_pressed(KeyCode::ArrowLeft) || pad(GamepadButton::DPadLeft) {
        d.0 -= 1;
    }
    if keys.just_pressed(KeyCode::ArrowRight) || pad(GamepadButton::DPadRight) {
        d.0 += 1;
    }
    if keys.just_pressed(KeyCode::ArrowUp) || pad(GamepadButton::DPadUp) {
        d.1 -= 1;
    }
    if keys.just_pressed(KeyCode::ArrowDown) || pad(GamepadButton::DPadDown) {
        d.1 += 1;
    }
    if d != (0, 0) {
        let cols = page.cols.max(1);
        let to = step_focus(page.focus, SecretRace::ALL.len(), cols, d.0, d.1);
        if to != page.focus {
            page.focus = to;
            page.reveal_focus = true;
        }
    }
}

#[allow(clippy::type_complexity)]
fn show(
    phase: Res<State<Phase>>,
    mut page: ResMut<SecretsPage>,
    progress: Res<Progress>,
    mut nodes: ParamSet<(
        Query<&mut Node, With<PageRoot>>,
        Query<&mut Node, With<OpenButton>>,
    )>,
    mut label: Query<(&mut Text, &mut TextColor), With<OpenLabel>>,
    selection: Res<crate::survivors::Selection>,
) {
    let menu = *phase.get() == Phase::Menu;
    // Create Character has no opener; the page stays reachable from the character list.
    let listing = selection.character.is_some();
    if !menu && page.open {
        page.open = false;
    }
    let set = |mut n: Mut<Node>, on: bool| {
        let d = if on { Display::Flex } else { Display::None };
        if n.display != d {
            n.display = d;
        }
    };
    for n in &mut nodes.p0() {
        set(n, menu && page.open);
    }
    for n in &mut nodes.p1() {
        set(n, menu && listing && !page.open);
    }
    // The opener says when something new waits behind it.
    if progress.is_changed() {
        let fresh = !unseen(&progress).is_empty();
        for (mut t, mut c) in &mut label {
            let want = if fresh { "Secrets  (new!)" } else { "Secrets" };
            if t.0 != want {
                t.0 = want.to_string();
            }
            let color = if fresh { GOLD } else { Color::WHITE };
            if c.0 != color {
                c.0 = color;
            }
        }
    }
}

/// The grid's width follows the canvas: 5 cards on the tablet, 4 or 3 when narrower.
fn layout(
    windows: Query<&Window, With<PrimaryWindow>>,
    scale: Res<UiScale>,
    mut page: ResMut<SecretsPage>,
    mut grid: Query<&mut Node, With<Grid>>,
) {
    let Ok(w) = windows.single() else { return };
    let width = w.width() / scale.0.max(1e-3);
    let cols = columns(width);
    if page.cols == cols {
        return;
    }
    page.cols = cols;
    for mut n in &mut grid {
        n.width = Val::Px(grid_width(cols));
    }
}

fn rebuild(
    mut commands: Commands,
    page: Res<SecretsPage>,
    progress: Res<Progress>,
    font: Option<Res<HudFont>>,
    grid: Query<Entity, With<Grid>>,
    mut title: Query<&mut Text, With<PageTitle>>,
    mut built: Local<Option<u32>>,
) {
    if !page.open || *built == Some(page.version) {
        return;
    }
    let (Some(font), Ok(grid)) = (font, grid.single()) else {
        return;
    };
    *built = Some(page.version);
    let f = &font.0;
    let (entries, found) = page_entries(&progress);
    if let Ok(mut t) = title.single_mut() {
        t.0 = format!("{found} / {} secrets discovered", entries.len());
    }
    commands.entity(grid).despawn_related::<Children>();
    commands.entity(grid).with_children(|g| {
        for (i, e) in entries.iter().enumerate() {
            card(g, f, i, e, page.new.contains(&e.race));
        }
    });
}

fn card(g: &mut ChildSpawnerCommands, f: &Handle<Font>, i: usize, e: &Entry, new: bool) {
    let d = def(e.race);
    g.spawn((
        Card(i),
        Node {
            width: Val::Px(CARD_W),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(6.0),
            padding: UiRect::all(Val::Px(8.0)),
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(if e.unlocked { FILL } else { LOCKED_FILL }),
        BorderColor::all(if e.unlocked { EDGE } else { LOCKED_EDGE }),
    ))
    .with_children(|c| {
        // The portrait: the race in colour, or its silhouette with a "?" and a padlock.
        c.spawn((
            Node {
                width: Val::Px(PORTRAIT),
                height: Val::Px(PORTRAIT),
                flex_shrink: 0.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.01, 0.01, 0.01)),
            BorderColor::all(if e.unlocked { GOLD } else { LOCKED_EDGE }),
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                ImageNode {
                    color: Color::NONE,
                    ..default()
                },
                RaceIcon {
                    path: d.icon,
                    tint: if e.unlocked { Color::WHITE } else { SILHOUETTE },
                },
                Pickable::IGNORE,
            ));
            if !e.unlocked {
                p.spawn(text(f, "?", 44.0, DIM_GOLD));
                padlock(p);
            }
        });
        c.spawn(text(f, e.name, 20.0, if e.unlocked { GOLD } else { DIM_GOLD }));
        if let Some(riddle) = e.riddle {
            // At most four lines; WoW's quest-text gold on the dark card.
            c.spawn((
                Node {
                    width: Val::Percent(100.0),
                    max_height: Val::Px(RIDDLE * 1.25 * RIDDLE_LINES),
                    justify_content: JustifyContent::Center,
                    overflow: Overflow::clip(),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_child(text(f, riddle, RIDDLE, PALE_GOLD));
        }
        if e.unlocked {
            c.spawn((
                Node {
                    column_gap: Val::Px(1.5),
                    flex_shrink: 0.0,
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|row| {
                for &class in d.classes {
                    let Some(tc) = crate::glue::art::class_tc(class) else {
                        continue;
                    };
                    row.spawn((
                        Node {
                            width: Val::Px(CLASS_ICON),
                            height: Val::Px(CLASS_ICON),
                            ..default()
                        },
                        ImageNode {
                            color: Color::NONE,
                            ..default()
                        },
                        UiArt::crop(CLASS_SHEET, tc),
                        Pickable::IGNORE,
                    ));
                }
            });
            // A thin rule, then the condition, revealed only now.
            c.spawn((
                Node {
                    width: Val::Percent(70.0),
                    height: Val::Px(1.0),
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(DIM_GOLD),
                Pickable::IGNORE,
            ));
            if let Some(cond) = e.condition {
                c.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_child(text(f, capitalised(cond), TEXT_FLOOR, Color::WHITE));
            }
        }
        if new {
            c.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(8.0),
                    right: Val::Px(8.0),
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(RIBBON),
                Pickable::IGNORE,
            ))
            .with_child(text(f, "NEW", TEXT_FLOOR + 1.0, Color::WHITE));
        }
    });
}

/// A small padlock in the portrait's corner, drawn from two boxes: the shackle and the body.
fn padlock(p: &mut ChildSpawnerCommands) {
    p.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(3.0),
            bottom: Val::Px(3.0),
            width: Val::Px(20.0),
            height: Val::Px(24.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|l| {
        l.spawn((
            Node {
                width: Val::Px(12.0),
                height: Val::Px(11.0),
                border: UiRect {
                    left: Val::Px(3.0),
                    right: Val::Px(3.0),
                    top: Val::Px(3.0),
                    bottom: Val::Px(0.0),
                },
                border_radius: BorderRadius::top(Val::Px(6.0)),
                ..default()
            },
            BorderColor::all(DIM_GOLD),
            Pickable::IGNORE,
        ));
        l.spawn((
            Node {
                width: Val::Px(20.0),
                height: Val::Px(13.0),
                border_radius: BorderRadius::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(DIM_GOLD),
            Pickable::IGNORE,
        ));
    });
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|first| first.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// The focused card's edge lights; the rest keep their own.
fn focus_style(
    page: Res<SecretsPage>,
    progress: Res<Progress>,
    mut cards: Query<(&Card, &mut BorderColor)>,
) {
    if !page.open {
        return;
    }
    for (card, mut border) in &mut cards {
        let unlocked = SecretRace::ALL
            .get(card.0)
            .is_some_and(|&r| progress.is_unlocked(r));
        let want = if card.0 == page.focus {
            FOCUS_EDGE
        } else if unlocked {
            EDGE
        } else {
            LOCKED_EDGE
        };
        if border.top != want {
            *border = BorderColor::all(want);
        }
    }
}

/// A finger must travel this far, in UI units, before it scrolls: a tap on a card or Back that
/// wobbles a little stays a tap.
const DRAG_THRESHOLD: f32 = 8.0;

/// The grid scrolls by wheel, by touch drag, and to keep the focused card in view.
#[allow(clippy::type_complexity)]
fn scroll(
    mut page: ResMut<SecretsPage>,
    mut wheel: MessageReader<MouseWheel>,
    touches: Res<Touches>,
    scale: Res<UiScale>,
    windows: Query<&Window, With<PrimaryWindow>>,
    buttons: Query<(&PageButton, &ComputedNode, &UiGlobalTransform)>,
    cards: Query<(&Card, &ComputedNode)>,
    mut grid: Query<(&ComputedNode, &mut ScrollPosition), With<Grid>>,
    mut more: Query<&mut Visibility, With<MoreHint>>,
) {
    let mut dy = 0.0;
    for ev in wheel.read() {
        dy -= match ev.unit {
            MouseScrollUnit::Line => ev.y * 40.0,
            MouseScrollUnit::Pixel => ev.y,
        };
    }
    if !page.open {
        return;
    }
    // A finger drags the content: up to scroll down. Window px to UI units.
    let k = 1.0 / scale.0.max(1e-3);
    let px = windows.single().map_or(1.0, |w| w.scale_factor());
    // Touches are in logical pixels, the layout in physical ones.
    let on_back = |at: Vec2| {
        let at = at * px;
        buttons.iter().any(|(b, node, tf)| {
            *b == PageButton::Close
                && (at - tf.translation).abs().cmple(node.size() * 0.5).all()
        })
    };
    for t in touches.iter() {
        if t.distance().length() * k < DRAG_THRESHOLD || on_back(t.start_position()) {
            continue;
        }
        dy -= t.delta().y * k;
    }
    let Ok((node, mut pos)) = grid.single_mut() else {
        return;
    };
    let view = node.size.y * node.inverse_scale_factor;
    let max = ((node.content_size.y - node.size.y) * node.inverse_scale_factor).max(0.0);
    let mut y = pos.0.y + dy;
    if page.reveal_focus && view > 0.0 {
        page.reveal_focus = false;
        // Rows are as tall as their tallest card: each row's height from the laid-out cards.
        let cols = page.cols.max(1);
        let mut rows: Vec<f32> = Vec::new();
        for (card, n) in &cards {
            let r = card.0 / cols;
            if rows.len() <= r {
                rows.resize(r + 1, 0.0);
            }
            rows[r] = rows[r].max(n.size.y * n.inverse_scale_factor);
        }
        let row = page.focus / cols;
        let top: f32 = rows.iter().take(row).map(|h| h + GAP).sum();
        let height = rows.get(row).copied().unwrap_or(0.0);
        if top < y {
            y = top;
        } else if top + height > y + view {
            y = top + height - view;
        }
    }
    let y = y.clamp(0.0, max);
    if pos.0.y != y {
        pos.0.y = y;
    }
    // "more" while there is more than a hair left to scroll.
    let want = if max - y > 2.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut more {
        v.set_if_neq(want);
    }
}

/// Draw each card's race icon once its file can be read; nothing to do touches no asset.
fn fill_icons(
    mut commands: Commands,
    mut icons: Query<(Entity, &mut ImageNode, &RaceIcon), Without<RaceIconDone>>,
    mut assets: Option<ResMut<benilla_assets::WorldAssets>>,
    mut images: ResMut<Assets<Image>>,
) {
    if icons.is_empty() {
        return;
    }
    let Some(assets) = assets.as_deref_mut() else {
        return;
    };
    for (e, mut img, icon) in &mut icons {
        // Not done until the texture is in: an archive not yet readable is tried again.
        if let Some(h) = assets.sprite_texture(&format!("{}.blp", icon.path), &mut images) {
            img.image = h;
            img.color = icon.tint;
            commands.entity(e).insert(RaceIconDone);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thirteen_entries_in_order_with_riddles_locked_and_conditions_unlocked() {
        let mut p = Progress::default();
        let (entries, found) = page_entries(&p);
        assert_eq!(entries.len(), 13);
        assert_eq!(found, 0);
        for (e, r) in entries.iter().zip(SecretRace::ALL) {
            assert_eq!(e.race, r);
            assert!(!e.unlocked);
            assert_eq!(e.name, "???");
            assert_eq!(e.riddle, Some(def(r).riddle));
            assert_eq!(e.condition, None, "{r:?}'s condition shows while locked");
        }
        p.unlocked.push("murloc".into());
        p.unlocked.push("imp".into());
        let (entries, found) = page_entries(&p);
        assert_eq!(found, 2);
        for e in &entries {
            let open = matches!(e.race, SecretRace::Murloc | SecretRace::Imp);
            assert_eq!(e.unlocked, open);
            if open {
                assert_eq!(e.name, def(e.race).name);
                assert_eq!(e.condition, Some(def(e.race).condition));
                assert_eq!(e.riddle, None);
            } else {
                assert_eq!(e.condition, None);
                assert!(e.riddle.is_some());
            }
        }
    }

    #[test]
    fn an_unknown_unlock_key_counts_for_nothing() {
        let p = Progress {
            unlocked: vec!["blood_elf".into()],
            ..default()
        };
        assert_eq!(page_entries(&p).1, 0);
    }

    #[test]
    fn new_until_the_page_is_opened_once() {
        let mut p = Progress {
            unlocked: vec!["kobold".into()],
            ..default()
        };
        assert_eq!(unseen(&p), vec![SecretRace::Kobold]);
        // What opening the page does (without its save): the ribbons, then everything seen.
        let ribbons = unseen(&p);
        p.seen = p.unlocked.clone();
        assert_eq!(ribbons, vec![SecretRace::Kobold]);
        assert!(unseen(&p).is_empty());
        p.unlocked.push("gnoll".into());
        assert_eq!(unseen(&p), vec![SecretRace::Gnoll]);
    }

    #[test]
    fn five_across_on_the_tablet_fewer_when_narrow() {
        // The Galaxy Tab's 16:10 at 768 units tall is ~1229 wide.
        assert_eq!(columns(1229.0), 5);
        assert_eq!(columns(1024.0), 4);
        assert_eq!(columns(768.0), 3);
        assert!(grid_width(5) + 2.0 * SIDE <= 1229.0);
    }

    #[test]
    fn arrows_move_the_focus_and_stop_at_the_edges() {
        // 13 cards, 5 wide: rows 0-4, 5-9, 10-12.
        assert_eq!(step_focus(0, 13, 5, 1, 0), 1);
        assert_eq!(step_focus(0, 13, 5, -1, 0), 0);
        assert_eq!(step_focus(4, 13, 5, 1, 0), 4);
        assert_eq!(step_focus(2, 13, 5, 0, 1), 7);
        assert_eq!(step_focus(7, 13, 5, 0, 1), 12);
        // Under the last row's gap there is no card: stay.
        assert_eq!(step_focus(8, 13, 5, 0, 1), 8);
        assert_eq!(step_focus(1, 13, 5, 0, -1), 1);
    }
}
