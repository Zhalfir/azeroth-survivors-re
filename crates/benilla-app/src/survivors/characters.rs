//! Saved characters and map completion (`docs/PROGRESSION.md`, phase 2).
//!
//! A character is a name, class, race and gender plus the maps it has completed, one per
//! difficulty; each completion is a talent point, 51 in all. At most [`MAX_CHARACTERS`].
//!
//! The menu opens on **character select**: the list on the left where the create tower stands
//! ([`super::menu::TowerRoot`], hidden), the zone list on the right with a mark per difficulty
//! completed, a difficulty picker above Enter World. **Create** shows the tower again, with a
//! generated name (there is no keyboard on a NativeActivity) and Create / Back where Enter World
//! was ([`super::menu::EnterSlot`]). Delete takes a second tap to confirm.
//!
//! Files, through [`crate::local_state`] so the page's store answers too:
//! `benilla-config/characters/index.txt` (the ids, in order, then `active <id>`) and
//! `characters/<id>.toml` per character.
//!
//! **Progression:** the zones unlock in the list's order ([`menu::zone_order`], easiest first),
//! each once the one before is completed on Normal; a locked zone shows only "???". On a zone,
//! Heroic opens after Normal and Mythic after Heroic ([`Unlocks`]).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::data::{self, Difficulty};
use super::hud::HudFont;
use super::menu::{self, EnterSlot, MenuAction, TowerRoot};
use super::{Phase, Rng, Run, Selection};
use crate::char_select::race_name;

pub(super) const MAX_CHARACTERS: usize = 8;
/// 16 zones and 3 raids (not yet built) × the difficulties they count on: one point each.
pub(super) const MAX_TALENT_POINTS: usize = 51;

const GOLD: Color = Color::srgb(1.0, 0.82, 0.0);
const DIM: Color = Color::srgb(0.55, 0.5, 0.42);
const FILL: Color = Color::srgba(0.09, 0.09, 0.09, 0.88);
const EDGE: Color = Color::srgba(0.8, 0.62, 0.25, 0.9);
const BUTTON: Color = Color::srgba(0.16, 0.11, 0.04, 0.95);
const BUTTON_HOT: Color = Color::srgba(0.34, 0.22, 0.06, 0.95);
/// An icon's inner 86%: the outer rows are its painted bevel.
const ICON_CROP: [f32; 4] = [0.07, 0.93, 0.07, 0.93];
const PICKED: Color = Color::srgba(0.45, 0.3, 0.05, 0.95);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(super) struct Character {
    pub id: u32,
    pub name: String,
    /// `ChrClasses.dbc` id ([`data::ClassDef::id`]): stable where the class list's order is not.
    pub class_id: u8,
    pub race: u8,
    pub sex: u8,
    /// Maps completed, as `(map name, difficulty name)`.
    #[serde(default)]
    pub completions: Vec<(String, String)>,
}

impl Character {
    pub fn talent_points(&self) -> usize {
        self.completions.len().min(MAX_TALENT_POINTS)
    }

    pub fn completed(&self, map: usize, difficulty: Difficulty) -> bool {
        let name = data::MAPS[map].name;
        self.completions
            .iter()
            .any(|(m, d)| m == name && d == difficulty.name())
    }

    fn class_index(&self) -> Option<usize> {
        data::CLASSES.iter().position(|c| c.id == self.class_id)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum Mode {
    #[default]
    Select,
    Create,
}

#[derive(Resource, Default)]
pub(super) struct Characters {
    pub list: Vec<Character>,
    pub active: Option<u32>,
    mode: Mode,
    /// The name the character being created will take.
    new_name: String,
    /// A first tap on Delete arms it for this character; the second deletes.
    delete_armed: Option<u32>,
    loaded: bool,
    /// Bumped on every change, so the list rebuilds once.
    version: u32,
}

impl Characters {
    fn active(&self) -> Option<&Character> {
        self.active
            .and_then(|id| self.list.iter().find(|c| c.id == id))
    }
}

/// What the active character may pick, worked out from its completions.
#[derive(Resource)]
pub(super) struct Unlocks {
    /// Per map ([`data::MAPS`] index): open, and the hardest difficulty open on it.
    zones: Vec<Option<Difficulty>>,
}

impl Default for Unlocks {
    fn default() -> Self {
        Self::for_character(None)
    }
}

impl Unlocks {
    pub fn zone(&self, map: usize) -> bool {
        self.zones.get(map).is_some_and(Option::is_some)
    }

    pub fn difficulty(&self, map: usize, d: Difficulty) -> bool {
        self.zones
            .get(map)
            .copied()
            .flatten()
            .is_some_and(|top| d as u8 <= top as u8)
    }

    fn for_character(c: Option<&Character>) -> Self {
        let mut zones = vec![None; data::MAPS.len()];
        let done = |m: usize, d: Difficulty| c.is_some_and(|c| c.completed(m, d));
        let mut previous_done = true;
        for (m, ..) in menu::zone_order() {
            if !previous_done {
                break;
            }
            let top = if done(m, Difficulty::Heroic) {
                Difficulty::Mythic
            } else if done(m, Difficulty::Normal) {
                Difficulty::Heroic
            } else {
                Difficulty::Normal
            };
            zones[m] = Some(top);
            previous_done = done(m, Difficulty::Normal);
        }
        Self { zones }
    }
}

/// A locked zone row's cover: "???" over its name and marks.
#[derive(Component)]
struct LockCover(usize);

/// The results screen's completion line, placed by `hud::setup_hud`.
#[derive(Component)]
pub(super) struct CompletionLine;

#[derive(Component)]
struct CharPanel;
#[derive(Component)]
struct CharRows;
#[derive(Component)]
struct DifficultyBar;
#[derive(Component)]
struct CreateBar;
#[derive(Component)]
struct NewName;
#[derive(Component)]
struct DeleteLabel;
#[derive(Component)]
struct CountLabel;
/// A zone row's mark for one difficulty: gold once completed.
#[derive(Component)]
struct ZoneMark(usize, Difficulty);
/// On a zone row once its marks are in.
#[derive(Component)]
struct ZoneMarked;

#[derive(Component, Clone, Copy, PartialEq)]
enum CharButton {
    Pick(u32),
    Create,
    Delete,
    Difficulty(Difficulty),
    Reroll,
    Confirm,
    Back,
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Characters>()
        .init_resource::<Unlocks>()
        .add_systems(Startup, spawn_ui.after(super::hud::setup_hud))
        .add_systems(
            Update,
            (
                load,
                press,
                apply_active,
                compute_unlocks,
                rebuild_rows,
                show,
                mark_zones,
                refresh_labels,
            )
                .chain(),
        )
        .add_systems(OnEnter(Phase::GameOver), record_completion);
}

// --- Saving. ---

fn dir() -> Option<std::path::PathBuf> {
    crate::local_state::home().map(|h| h.join("characters"))
}

fn save_index(chars: &Characters) {
    let Some(dir) = dir() else { return };
    let mut text: String = chars.list.iter().map(|c| format!("{}\n", c.id)).collect();
    if let Some(id) = chars.active {
        text.push_str(&format!("active {id}\n"));
    }
    let _ = crate::local_state::write_atomic(&dir.join("index.txt"), &text);
}

fn save_character(c: &Character) {
    let Some(dir) = dir() else { return };
    if let Ok(text) = toml::to_string(c) {
        let _ = crate::local_state::write_atomic(&dir.join(format!("{}.toml", c.id)), &text);
    }
}

fn load(mut chars: ResMut<Characters>) {
    if chars.loaded {
        return;
    }
    chars.loaded = true;
    let Some(dir) = dir() else { return };
    let Ok(index) = crate::local_state::read_to_string(&dir.join("index.txt")) else {
        chars.mode = Mode::Create;
        return;
    };
    let mut active = None;
    for line in index.lines().map(str::trim) {
        if let Some(id) = line.strip_prefix("active ") {
            active = id.trim().parse().ok();
            continue;
        }
        let Ok(id) = line.parse::<u32>() else {
            continue;
        };
        let path = dir.join(format!("{id}.toml"));
        if let Some(c) = crate::local_state::read_to_string(&path)
            .ok()
            .and_then(|t| toml::from_str::<Character>(&t).ok())
        {
            chars.list.push(c);
        }
    }
    chars.list.truncate(MAX_CHARACTERS);
    chars.active = active.filter(|id| chars.list.iter().any(|c| c.id == *id));
    if chars.active.is_none() {
        chars.active = chars.list.first().map(|c| c.id);
    }
    if chars.list.is_empty() {
        chars.mode = Mode::Create;
    }
    chars.version += 1;
}

// --- Names. ---

const NAME_HEADS: &[&str] = &[
    "Thal", "Mor", "Kael", "Bran", "Gar", "Lor", "Vol", "Jai", "Syl", "Dra", "Ur", "Zul", "Ar",
    "Bel", "Cor", "Fen", "Hal", "Kar", "Mal", "Nar", "Or", "Rha", "Tor", "Vel", "Ash", "Ery",
];
const NAME_TAILS: &[&str] = &[
    "dor", "ion", "ar", "ok", "eth", "an", "is", "ra", "uk", "en", "os", "ira", "gar", "wyn",
    "thas", "ak", "ella", "ius", "om", "ash", "ina", "rek", "dris", "mir",
];

fn random_name(rng: &mut Rng, taken: &[Character]) -> String {
    for _ in 0..32 {
        let name = format!(
            "{}{}",
            NAME_HEADS[rng.index(NAME_HEADS.len())],
            NAME_TAILS[rng.index(NAME_TAILS.len())]
        );
        if !taken.iter().any(|c| c.name == name) {
            return name;
        }
    }
    format!("Hero{}", taken.len() + 1)
}

// --- The screens. ---

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
        Pickable::IGNORE,
    )
}

fn button_node(width: f32, height: f32) -> Node {
    Node {
        width: Val::Px(width),
        height: Val::Px(height),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        border: UiRect::all(Val::Px(2.0)),
        border_radius: BorderRadius::all(Val::Px(4.0)),
        ..default()
    }
}

fn spawn_button(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    action: CharButton,
    label: &str,
    width: f32,
    extra: impl Bundle,
) {
    parent
        .spawn((
            Button,
            action,
            button_node(width, 40.0),
            BackgroundColor(BUTTON),
            BorderColor::all(EDGE),
        ))
        .with_children(|b| {
            b.spawn((text(font, label, 16.0, Color::WHITE), extra));
        });
}

/// The three panels, empty and out of the layout until the menu shows them.
fn spawn_ui(mut commands: Commands, font: Option<Res<HudFont>>) {
    let Some(font) = font else { return };
    let f = &font.0;
    // The list, where the create tower stands.
    commands
        .spawn((
            CharPanel,
            Node {
                position_type: PositionType::Absolute,
                // Under the WoW logo, which draws over the top of the tower's place.
                left: Val::Px(28.0),
                top: Val::Px(136.0),
                width: Val::Px(236.0),
                height: Val::Px(528.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(10.0)),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(FILL),
            BorderColor::all(EDGE),
            GlobalZIndex(50),
        ))
        .with_children(|p| {
            p.spawn((text(f, "Characters", 18.0, GOLD), CountLabel));
            p.spawn((
                CharRows,
                Node {
                    width: Val::Percent(100.0),
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    ..default()
                },
            ));
            spawn_button(p, f, CharButton::Create, "Create New Character", 210.0, ());
            spawn_button(
                p,
                f,
                CharButton::Delete,
                "Delete Character",
                210.0,
                DeleteLabel,
            );
        });
    // The difficulty picker, above the character's name line and Enter World.
    commands
        .spawn((
            DifficultyBar,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(136.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(8.0),
                display: Display::None,
                ..default()
            },
            GlobalZIndex(50),
            Pickable::IGNORE,
        ))
        .with_children(|bar| {
            for d in Difficulty::ALL {
                let label = format!("{} · {} min", d.name(), d.run_secs() as u32 / 60);
                spawn_button(bar, f, CharButton::Difficulty(d), &label, 150.0, ());
            }
        });
    // Creating: the name, a new one, Create and Back, where Enter World stands.
    commands
        .spawn((
            CreateBar,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(30.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(8.0),
                display: Display::None,
                ..default()
            },
            GlobalZIndex(50),
            Pickable::IGNORE,
        ))
        .with_children(|bar| {
            bar.spawn((text(f, "", 24.0, GOLD), NewName));
            bar.spawn((
                Node {
                    column_gap: Val::Px(8.0),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|row| {
                spawn_button(row, f, CharButton::Back, "Back", 120.0, ());
                spawn_button(row, f, CharButton::Reroll, "New name", 140.0, ());
                spawn_button(row, f, CharButton::Confirm, "Create", 160.0, ());
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn press(
    mut buttons: Query<(&Interaction, &CharButton, &mut BackgroundColor), Changed<Interaction>>,
    mut chars: ResMut<Characters>,
    mut selection: ResMut<Selection>,
    mut rng: ResMut<Rng>,
    creatures: Option<Res<crate::entities::Creatures>>,
    unlocks: Res<Unlocks>,
) {
    for (interaction, action, mut bg) in &mut buttons {
        if !matches!(action, CharButton::Pick(_) | CharButton::Difficulty(_)) {
            bg.0 = match interaction {
                Interaction::None => BUTTON,
                _ => BUTTON_HOT,
            };
        }
        if *interaction != Interaction::Pressed {
            continue;
        }
        if *action != CharButton::Delete {
            chars.delete_armed = None;
        }
        match *action {
            CharButton::Pick(id) => {
                chars.active = Some(id);
                save_index(&chars);
            }
            CharButton::Create if chars.list.len() < MAX_CHARACTERS => {
                chars.mode = Mode::Create;
                chars.new_name = random_name(&mut rng, &chars.list);
            }
            CharButton::Create => {}
            CharButton::Delete => {
                let Some(id) = chars.active else { continue };
                if chars.delete_armed == Some(id) {
                    chars.list.retain(|c| c.id != id);
                    chars.active = chars.list.first().map(|c| c.id);
                    chars.delete_armed = None;
                    selection.character = None;
                    if chars.list.is_empty() {
                        chars.mode = Mode::Create;
                        chars.new_name = random_name(&mut rng, &chars.list);
                    }
                    save_index(&chars);
                } else {
                    chars.delete_armed = Some(id);
                }
            }
            CharButton::Difficulty(d) if !unlocks.difficulty(selection.map, d) => {}
            CharButton::Difficulty(d) => {
                selection.difficulty = d;
                selection.version += 1;
            }
            CharButton::Reroll => chars.new_name = random_name(&mut rng, &chars.list),
            CharButton::Back if !chars.list.is_empty() => chars.mode = Mode::Select,
            CharButton::Back => {}
            CharButton::Confirm => {
                let (Some(class), Some(&(_, race, sex))) = (
                    selection.class_def(),
                    selection.look.and_then(|i| selection.looks.get(i)),
                ) else {
                    continue;
                };
                if chars.list.len() >= MAX_CHARACTERS {
                    continue;
                }
                let id = chars.list.iter().map(|c| c.id).max().unwrap_or(0) + 1;
                if chars.new_name.is_empty() {
                    chars.new_name = random_name(&mut rng, &chars.list);
                }
                let c = Character {
                    id,
                    name: chars.new_name.clone(),
                    class_id: class.id,
                    race,
                    sex,
                    completions: Vec::new(),
                };
                save_character(&c);
                chars.list.push(c);
                chars.active = Some(id);
                chars.mode = Mode::Select;
                save_index(&chars);
            }
        }
        chars.version += 1;
    }
    // A fresh install opens on Create with a name already rolled.
    if chars.mode == Mode::Create && chars.new_name.is_empty() && creatures.is_some() {
        chars.new_name = random_name(&mut rng, &chars.list);
        chars.version += 1;
    }
}

/// On select, the active character dresses the menu hero: its class, race and gender.
fn apply_active(
    chars: Res<Characters>,
    mut selection: ResMut<Selection>,
    creatures: Option<Res<crate::entities::Creatures>>,
) {
    let Some(creatures) = creatures.as_deref() else {
        return;
    };
    if chars.mode != Mode::Select {
        if selection.character.is_some() {
            selection.character = None;
        }
        return;
    }
    let Some(c) = chars.active() else { return };
    if selection.character == Some(c.id) {
        return;
    }
    let Some(class) = c.class_index() else { return };
    let all = menu::all_looks(creatures);
    if let Some(look) = menu::find(&all[class], Some(c.race), Some(c.sex))
        .or_else(|| menu::find(&all[class], Some(c.race), None))
    {
        menu::set(&mut selection, &all, class, look);
        selection.character = Some(c.id);
    }
}

fn rebuild_rows(
    mut commands: Commands,
    chars: Res<Characters>,
    font: Option<Res<HudFont>>,
    rows: Query<Entity, With<CharRows>>,
    mut built: Local<Option<u32>>,
) {
    if *built == Some(chars.version) {
        return;
    }
    let (Some(font), Ok(list)) = (font, rows.single()) else {
        return;
    };
    *built = Some(chars.version);
    let f = &font.0;
    commands.entity(list).despawn_related::<Children>();
    commands.entity(list).with_children(|ui| {
        for c in &chars.list {
            let picked = chars.active == Some(c.id);
            let class = c.class_index().map(|i| &data::CLASSES[i]);
            let class_name = class.map_or("", |k| k.name);
            let class_color = class.map_or(Color::WHITE, |k| super::hud::rgb(k.color));
            ui.spawn((
                Button,
                CharButton::Pick(c.id),
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(1.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(if picked { PICKED } else { Color::NONE }),
                BorderColor::all(if picked { EDGE } else { Color::NONE }),
            ))
            .with_children(|row| {
                row.spawn(text(f, c.name.clone(), 16.0, GOLD));
                row.spawn(text(
                    f,
                    format!("{} {}", race_name(c.race), class_name),
                    12.0,
                    class_color,
                ));
                row.spawn(text(
                    f,
                    format!("Talent points {}/{}", c.talent_points(), MAX_TALENT_POINTS),
                    11.0,
                    DIM,
                ));
            });
        }
    });
}

/// Which screen is up: select (list, difficulties, Enter World) or create (tower, name, Create).
#[allow(clippy::type_complexity)]
fn show(
    phase: Res<State<Phase>>,
    chars: Res<Characters>,
    mut panels: ParamSet<(
        Query<&mut Node, With<CharPanel>>,
        Query<&mut Node, With<DifficultyBar>>,
        Query<&mut Node, With<CreateBar>>,
        Query<&mut Node, With<TowerRoot>>,
        Query<&mut Node, With<EnterSlot>>,
    )>,
) {
    let menu = *phase.get() == Phase::Menu;
    let select = menu && chars.mode == Mode::Select;
    let create = menu && chars.mode == Mode::Create;
    let set = |mut node: Mut<Node>, on: bool| {
        let display = if on { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    };
    for n in &mut panels.p0() {
        set(n, select);
    }
    for n in &mut panels.p1() {
        set(n, select);
    }
    for n in &mut panels.p2() {
        set(n, create);
    }
    // The menu's own panels: off only while the other screen needs their place.
    for n in &mut panels.p3() {
        set(n, !select);
    }
    for n in &mut panels.p4() {
        set(n, !create);
    }
}

/// Three marks on every zone row, N H M, gold where the active character completed it.
fn mark_zones(
    mut commands: Commands,
    rows: Query<(Entity, &MenuAction), (With<Button>, Without<ZoneMarked>)>,
    font: Option<Res<HudFont>>,
) {
    let Some(font) = font else { return };
    for (e, action) in &rows {
        let MenuAction::Map(i) = *action else {
            continue;
        };
        commands.entity(e).insert(ZoneMarked).with_children(|row| {
            row.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(6.0),
                    top: Val::Px(4.0),
                    column_gap: Val::Px(4.0),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|marks| {
                for d in Difficulty::ALL {
                    marks.spawn((
                        Node {
                            width: Val::Px(15.0),
                            height: Val::Px(15.0),
                            ..default()
                        },
                        ImageNode {
                            color: Color::NONE,
                            ..default()
                        },
                        super::hud::UiArt::crop(d.icon(), ICON_CROP),
                        ZoneMark(i, d),
                        Pickable::IGNORE,
                    ));
                }
            });
            // Over the name and the marks while the zone is locked: nothing of what is next.
            row.spawn((
                LockCover(i),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(-4.0),
                    right: Val::Px(-4.0),
                    top: Val::Px(-2.0),
                    bottom: Val::Px(-2.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::left(Val::Px(4.0)),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.07, 0.07, 0.07)),
                Pickable::IGNORE,
            ))
            .with_children(|cover| {
                cover.spawn(text(&font.0, "???", 15.0, DIM));
                cover.spawn(text(&font.0, "Locked", 12.0, DIM));
            });
        });
    }
}

#[allow(clippy::type_complexity)]
fn refresh_labels(
    chars: Res<Characters>,
    selection: Res<Selection>,
    mut marks: Query<(&ZoneMark, &mut ImageNode)>,
    fresh: Query<(), (With<ZoneMark>, Added<super::hud::ArtDone>)>,
    mut texts: ParamSet<(
        Query<&mut Text, With<NewName>>,
        Query<&mut Text, With<DeleteLabel>>,
        Query<&mut Text, With<CountLabel>>,
    )>,
    mut difficulty: Query<(
        &CharButton,
        &mut BackgroundColor,
        &mut BorderColor,
        &Children,
    )>,
    mut covers: Query<(&LockCover, &mut Node)>,
    mut label_colors: Query<&mut TextColor, Without<ZoneMark>>,
    unlocks: Res<Unlocks>,
) {
    if !chars.is_changed() && !selection.is_changed() && !unlocks.is_changed() && fresh.is_empty() {
        return;
    }
    for (cover, mut node) in &mut covers {
        let display = if unlocks.zone(cover.0) {
            Display::None
        } else {
            Display::Flex
        };
        if node.display != display {
            node.display = display;
        }
    }
    let active = chars.active();
    for (mark, mut image) in &mut marks {
        let done = active.is_some_and(|c| c.completed(mark.0, mark.1));
        // Lit once completed; until then dark and faint, the icon only a hint.
        let want = if done {
            Color::WHITE
        } else {
            Color::srgba(0.3, 0.3, 0.3, 0.55)
        };
        if image.color != want {
            image.color = want;
        }
    }
    if let Ok(mut t) = texts.p0().single_mut() {
        t.0 = chars.new_name.clone();
    }
    if let Ok(mut t) = texts.p1().single_mut() {
        t.0 = match (chars.delete_armed, active) {
            (Some(_), Some(c)) => format!("Tap again: delete {}", c.name),
            _ => "Delete Character".into(),
        };
    }
    if let Ok(mut t) = texts.p2().single_mut() {
        t.0 = format!("Characters {}/{}", chars.list.len(), MAX_CHARACTERS);
    }
    for (action, mut bg, mut border, children) in &mut difficulty {
        let CharButton::Difficulty(d) = *action else {
            continue;
        };
        let open = unlocks.difficulty(selection.map, d);
        let on = selection.difficulty == d;
        bg.0 = if on { PICKED } else { BUTTON };
        *border = BorderColor::all(match (on, open) {
            (true, _) => GOLD,
            (false, true) => EDGE,
            (false, false) => DIM.with_alpha(0.4),
        });
        for child in children.iter() {
            if let Ok(mut color) = label_colors.get_mut(child) {
                color.0 = if open {
                    Color::WHITE
                } else {
                    DIM.with_alpha(0.6)
                };
            }
        }
    }
}

/// The active character's unlocks; a pick that is no longer open falls back to what is: the
/// furthest open zone, Normal.
fn compute_unlocks(
    chars: Res<Characters>,
    mut unlocks: ResMut<Unlocks>,
    mut selection: ResMut<Selection>,
    mut seen: Local<Option<(Option<u32>, u32)>>,
) {
    let key = (chars.active, chars.version);
    if *seen != Some(key) {
        *seen = Some(key);
        *unlocks = Unlocks::for_character(chars.active());
    }
    if !unlocks.zone(selection.map) {
        if let Some(m) = menu::zone_order()
            .into_iter()
            .map(|(m, ..)| m)
            .filter(|&m| unlocks.zone(m))
            .last()
        {
            selection.map = m;
            selection.version += 1;
        }
    }
    if !unlocks.difficulty(selection.map, selection.difficulty) {
        selection.difficulty = Difficulty::Normal;
        selection.version += 1;
    }
}

// --- Completion. ---

/// A won run completes its map on its difficulty for the character playing; a first completion
/// is a talent point.
fn record_completion(
    run: Res<Run>,
    selection: Res<Selection>,
    mut chars: ResMut<Characters>,
    mut lines: Query<&mut Text, With<CompletionLine>>,
) {
    let mut line = String::new();
    if run.won {
        let difficulty = data::difficulty();
        let map = data::MAPS[selection.stage.min(data::MAPS.len() - 1)].name;
        if let Some(c) = selection
            .character
            .and_then(|id| chars.list.iter_mut().find(|c| c.id == id))
        {
            let first = !c
                .completions
                .iter()
                .any(|(m, d)| m == map && d == difficulty.name());
            super::combatlog::event(
                run.elapsed,
                format!(
                    "completion: {} on {} for {} ({})",
                    map,
                    difficulty.name(),
                    c.name,
                    if first {
                        "first, talent point"
                    } else {
                        "repeat"
                    }
                ),
            );
            if first {
                c.completions
                    .push((map.to_string(), difficulty.name().to_string()));
                save_character(c);
            }
            line = if first {
                format!(
                    "{map} completed on {}! Talent point earned: {}/{}",
                    difficulty.name(),
                    c.talent_points(),
                    MAX_TALENT_POINTS
                )
            } else {
                format!("{map} completed on {} again.", difficulty.name())
            };
        }
        chars.version += 1;
    }
    for mut t in &mut lines {
        t.0 = line.clone();
    }
}
